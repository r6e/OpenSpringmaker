# Shaded 3D View (SDF Ray-Marching) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The `Spring3d` mode renders a shaded, ray-marched spring via iced's shader widget — smooth at any zoom, metallic, exact ground ends — with the plotters wireframe as the automatic fallback and the deterministic test path.

**Architecture:** A pure Rust SDF mirror (`viz/sdf.rs`) owns the analytic scene: helix distance by periodic reduction with neighbor-turn checking, exact torus/capsule/plane primitives, `min` union + `max` ground cuts, fixed-layout uniform packing. A humble `viz/shader3d.rs` implements `iced::widget::shader::Program`: a WGSL fragment shader ray-marches the same math over one quad drawn into iced's own render pass. `App.zoom` + `Message::Zoom` mirror the orbit-delta discipline; a boot-time wgpu adapter probe sets `App.shader_available`, and the `Spring3d` arm chooses shaded vs the existing wireframe through a pure, unit-tested decision fn.

**Tech Stack:** Rust, iced 0.14 (`widget::shader` — in default features, zero new deps; wgpu types via `iced_wgpu`'s re-export path resolved in Task 5), WGSL, existing viz/plot infrastructure.

## Global Constraints

- Branch `feat/gui-shaded-3d` (spec 55ff64e). springmaker-only.
- Strict TDD; BOTH clippy commands (`cargo clippy -p springmaker -- -D warnings` AND `--all-targets`); fmt/doc/typos gates; trailer `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.
- ADR 0008: `sdf.rs` is pure (NO iced, NO wgpu imports — plain math + plain data); `shader3d.rs` is humble.
- **Verified iced facts (vendored 0.14 — cite, don't re-derive):** `shader::Program { type State: Default; type Primitive: iced_wgpu::Primitive; fn update(&self, &mut State, &shader::Event, Rectangle, Cursor) -> Option<Action<Message>>; fn draw(&self, &State, Cursor, Rectangle) -> Primitive; fn mouse_interaction(..) }`. `iced_wgpu::Primitive { type Pipeline: Pipeline; fn prepare(&self, &mut Pipeline, &Device, &Queue, &Rectangle, &Viewport); fn draw(&self, &Pipeline, &mut RenderPass) -> bool }` — return `true` from `draw` (single quad, viewport/scissor pre-set by iced, NO depth attachment needed or available). `Pipeline::new(&Device, &Queue, TextureFormat)` called once. Fallback renderer's tiny-skia arm logs a warning and draws nothing (no panic). The Simulator's headless renderer tries wgpu first per machine → **HARD RULE: no snapshot_hash test may contain a Shader element** (machine-dependent pixels).
- Mirror-drift discipline: every numeric constant shared by Rust and WGSL (epsilon, max steps, march safety factor, AO params, part budget, uniform layout offsets) lives ONCE in `sdf.rs` as `pub(crate) const`s; the WGSL is generated-with/`format!`-substituted from those constants at Pipeline::new time (no duplicated literals in the .wgsl source — it carries `{{PLACEHOLDER}}` tokens).
- Zoom clamp exactly [0.3, 4.0]. Copy canon: none new (no new user-facing text; placeholders unchanged).
- `MAX_RENDER_TURNS` (existing, 2000) still caps upstream; the SDF march cost is resolution-bound, not turn-bound.

---

### Task 1: SDF core — primitives, profiles, contract types

**Files:**
- Create: `springmaker/src/viz/sdf.rs` (+ `pub mod sdf;` in `viz/mod.rs`)
- Test: same file, `#[cfg(test)] mod tests`

**Interfaces (later tasks rely on these EXACTLY):**

```rust
pub(crate) type Vec3 = [f64; 3];

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Profile {
    Circle { radius_mm: f64 },
    Rectangle { half_w_mm: f64, half_h_mm: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Appearance {
    pub base_color: [f32; 3],
    pub metallic: f32,
    pub roughness: f32,
}

pub(crate) fn steel() -> Appearance; // base [0.62,0.64,0.67], metallic 0.9, roughness 0.35
pub(crate) fn member_appearance(index: usize) -> Appearance; // hue-shifted steel per assembly member

// Primitive distances (all pure; p in mm):
pub(crate) fn sd_capsule(p: Vec3, a: Vec3, b: Vec3, radius_mm: f64) -> f64;
pub(crate) fn sd_torus_arc(p_local: Vec3, major_r: f64, minor_r: f64, sweep: f64) -> f64;
    // torus in its local frame (axis = local Y, arc from angle 0 to `sweep`);
    // beyond the sweep, distance to the arc's endpoint circles' nearest point (capped ends)
pub(crate) fn sd_profile_2d(d_radial: f64, d_axial: f64, profile: Profile) -> f64;
    // distance in the wire's local cross-section plane: Circle => hypot - r;
    // Rectangle => classic 2D box SDF on (|d_radial|-hw, |d_axial|-hh)
pub(crate) fn cut_plane(d: f64, p: Vec3, plane_point: Vec3, plane_normal: Vec3) -> f64;
    // max(d, dot(p - point, normal)) — exact half-space intersection
```

- [ ] **Step 1: failing tests** — write these EXACT properties (plus the constructors used):

```rust
const EPS: f64 = 1e-6;

#[test]
fn capsule_surface_points_are_zero() {
    let (a, b, r) = ([0.0, 0.0, 0.0], [0.0, 10.0, 0.0], 2.0);
    // On the barrel:
    assert!(sd_capsule([2.0, 5.0, 0.0], a, b, r).abs() < EPS);
    // On the end sphere:
    assert!(sd_capsule([0.0, -2.0, 0.0], a, b, r).abs() < EPS);
    // Inside negative / outside positive:
    assert!(sd_capsule([0.0, 5.0, 0.0], a, b, r) < 0.0);
    assert!(sd_capsule([5.0, 5.0, 0.0], a, b, r) > 0.0);
    // Exact distance outside: 3 mm radial => 1.0
    assert!((sd_capsule([5.0, 5.0, 0.0], a, b, r) - 3.0).abs() < EPS);
}

#[test]
fn torus_arc_matches_full_torus_inside_sweep_and_caps_beyond() {
    // Full-circle point at angle π/4 (inside a 3π/2 sweep): classic torus distance.
    let (maj, min) = (10.0, 1.5);
    let ang = std::f64::consts::FRAC_PI_4;
    let on_ring = [maj * ang.cos(), 0.0, maj * ang.sin()];
    assert!(sd_torus_arc(on_ring, maj, min, 1.5 * std::f64::consts::PI).abs() - min < EPS
        || (sd_torus_arc(on_ring, maj, min, 1.5 * std::f64::consts::PI) + min).abs() < EPS,
        "center of the tube ring is -minor_r deep");
    // Beyond the sweep: nearest endpoint-circle governs (distance from the angular gap).
    let past = [maj * (1.6 * std::f64::consts::PI).cos(), 0.0, maj * (1.6 * std::f64::consts::PI).sin()];
    assert!(sd_torus_arc(past, maj, min, 1.5 * std::f64::consts::PI) > 0.0);
}

#[test]
fn profile_circle_vs_rectangle() {
    assert!((sd_profile_2d(3.0, 4.0, Profile::Circle { radius_mm: 5.0 })).abs() < EPS); // 3-4-5
    let r = Profile::Rectangle { half_w_mm: 2.0, half_h_mm: 1.0 };
    assert!((sd_profile_2d(3.0, 0.0, r) - 1.0).abs() < EPS);  // 1 beyond half_w
    assert!(sd_profile_2d(0.0, 0.0, r) < 0.0);                // center inside
    assert!((sd_profile_2d(3.0, 2.0, r) - std::f64::consts::SQRT_2).abs() < EPS); // corner
}

#[test]
fn plane_cut_is_exact_halfspace() {
    // A point 2mm above the plane keeps its (smaller) base distance replaced by 2.0
    assert!((cut_plane(-5.0, [0.0, 2.0, 0.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]) - 2.0).abs() < EPS);
    // Below the plane: base distance wins (max with negative)
    assert!((cut_plane(-5.0, [0.0, -3.0, 0.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]) + 5.0).abs() < EPS);
}

const MEMBER_HUE_TABLE_LEN: usize = 4; // documented length of the member hue-shift table

#[test]
fn appearances_are_sane() {
    let s = steel();
    assert!(s.metallic > 0.5 && s.roughness < 0.6);
    assert_ne!(member_appearance(0).base_color, member_appearance(1).base_color);
    // The hue table wraps: member N gets member 0's appearance again.
    assert_eq!(
        member_appearance(0).base_color,
        member_appearance(MEMBER_HUE_TABLE_LEN).base_color
    );
}
```

  (`MEMBER_HUE_TABLE_LEN` is the implementation's table length — export it `pub(crate)` from
  sdf.rs so the test reads the real constant rather than a copied literal.)
- [ ] **Step 2:** run `cargo test -p springmaker sdf::` — FAIL (module missing).
- [ ] **Step 3:** implement. Reference math: capsule = `|p - a - clamp(dot(p-a, ba)/dot(ba,ba), 0, 1) * ba| - r`. Torus arc in local frame: `q = (hypot(p.x, p.z) - major_r, p.y)`; if the azimuth of `(p.x, p.z)` ∈ [0, sweep] → `|q| - minor_r`; else distance to nearest endpoint circle point (endpoint at angle 0 or sweep; compute the 3D point on the ring at that angle offset by the tube — use distance to the endpoint-circle center ring point minus minor_r via the capsule-style closest point on that terminal circle's cross-section disc center). Keep it conservative (never OVER-estimates distance — required for safe marching; document the invariant on each fn).
- [ ] **Step 4:** green + both clippy. **Step 5: Commit** `feat(gui): SDF core — primitives, profiles, appearance contract`

### Task 2: Helix SDF — periodic reduction, neighbor turns, taper

**Files:**
- Modify: `springmaker/src/viz/sdf.rs`
- Test: same module

**Interfaces:**

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HelixParams {
    pub radius_mm: f64,          // mean coil radius at t=0 (large end for conical)
    pub taper_small_r: Option<f64>, // Some(small_r) linear taper across `turns`
    pub pitch_mm: f64,
    pub turns: f64,
    pub profile: Profile,
    pub axial_offset_mm: f64,    // stacking offset (assembly series)
}

pub(crate) fn sd_helix(p: Vec3, h: &HelixParams) -> f64;
```

Algorithm (document in code; the WGSL mirrors it exactly):
1. Shift `p.y -= axial_offset`; cylindrical coords `r = hypot(p.x, p.z)`, `theta = atan2(p.z, p.x)` normalized to [0, τ).
2. Continuous turn estimate `k_est = (p.y - pitch * theta / TAU) / pitch`.
3. For `k` in `{floor(k_est)-1, floor(k_est), floor(k_est)+1, floor(k_est)+2}` (FOUR candidates — the close-wound guard): clamp the wire parameter `phi = theta + TAU*k` to `[0, TAU*turns]`; helix axial center `y_k = pitch * phi / TAU`; local coil radius `R_k = radius_mm` or the linear taper `radius_mm + (taper_small_r - radius_mm) * (phi / (TAU*turns))`; cross-section plane distance `d_k = sd_profile_2d(r - R_k, p.y - y_k, profile)`.
4. Result = min over candidates. Conservative: the reduction treats the coil locally as a ring in the cross-section plane — an UNDER-estimate near high pitch angles; multiply nothing here (the march safety factor lives in the march loop).
Clamping `phi` handles the helix ENDS exactly (the terminal cross-section behaves like a capped end; the profile distance at clamped phi is the distance to the end disc's cross-section — slightly conservative, fine).

- [ ] **Step 1: failing property tests:**

```rust
fn on_surface(h: &HelixParams, phi: f64, ring_ang: f64) -> Vec3 {
    // exact analytic surface point: center of wire at phi, offset by profile radius
    // in the cross-section plane direction (radial component cos(ring_ang), axial sin(ring_ang))
    let t = phi / (std::f64::consts::TAU * h.turns);
    let big_r = match h.taper_small_r { Some(s) => h.radius_mm + (s - h.radius_mm) * t, None => h.radius_mm };
    let wire_r = match h.profile { Profile::Circle { radius_mm } => radius_mm, _ => unreachable!() };
    let cy = h.pitch_mm * phi / std::f64::consts::TAU + h.axial_offset_mm;
    let rr = big_r + wire_r * ring_ang.cos();
    [rr * phi.cos(), cy + wire_r * ring_ang.sin(), rr * phi.sin()]
}

#[test]
fn helix_surface_points_are_near_zero() {
    let h = HelixParams { radius_mm: 10.0, taper_small_r: None, pitch_mm: 6.0,
                          turns: 8.0, profile: Profile::Circle { radius_mm: 1.0 }, axial_offset_mm: 0.0 };
    for i in 0..200 {
        let phi = (i as f64 / 199.0) * std::f64::consts::TAU * h.turns;
        for ring in [0.0, 1.0, 2.5, 4.0, 5.5] {
            let d = sd_helix(on_surface(&h, phi, ring), &h);
            assert!(d.abs() < 0.08, "phi={phi} ring={ring} d={d}"); // 8% of wire_r tolerance:
            // the ring-plane reduction is approximate at nonzero pitch angle
        }
    }
}

#[test]
fn close_wound_gap_never_reports_negative_between_coils() {
    // pitch == wire diameter (extension body): the midpoint BETWEEN adjacent coil
    // centers on the coil cylinder is a surface-contact point; just outside the
    // cylinder it must be OUTSIDE (>= -eps) — the wrong-turn failure mode reports
    // deep negative here.
    let h = HelixParams { radius_mm: 10.0, taper_small_r: None, pitch_mm: 2.0,
                          turns: 10.0, profile: Profile::Circle { radius_mm: 1.0 }, axial_offset_mm: 0.0 };
    for i in 0..100 {
        let theta = (i as f64 / 99.0) * std::f64::consts::TAU;
        let y_between = h.pitch_mm * (theta / std::f64::consts::TAU) + h.pitch_mm * 3.5; // between turns 3 and 4
        let p = [(h.radius_mm + 1.2) * theta.cos(), y_between, (h.radius_mm + 1.2) * theta.sin()];
        assert!(sd_helix(p, &h) > -1e-9, "between-coil point reported inside at theta={theta}");
    }
}

#[test]
fn helix_inside_wire_is_negative_and_far_outside_positive() {
    let h = HelixParams { radius_mm: 10.0, taper_small_r: None, pitch_mm: 6.0,
                          turns: 8.0, profile: Profile::Circle { radius_mm: 1.0 }, axial_offset_mm: 0.0 };
    // wire centerline point:
    let phi = std::f64::consts::TAU * 3.25;
    let c = [10.0 * phi.cos(), 6.0 * phi / std::f64::consts::TAU, 10.0 * phi.sin()];
    assert!(sd_helix(c, &h) < -0.9); // ~ -wire_r
    assert!(sd_helix([40.0, 10.0, 0.0], &h) > 25.0); // conservative but must be well positive
}

#[test]
fn taper_tracks_local_radius() {
    let h = HelixParams { radius_mm: 15.0, taper_small_r: Some(7.0), pitch_mm: 5.0,
                          turns: 6.0, profile: Profile::Circle { radius_mm: 0.8 }, axial_offset_mm: 0.0 };
    for frac in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let phi = frac * std::f64::consts::TAU * h.turns;
        let d = sd_helix(on_surface(&h, phi, 0.0), &h);
        assert!(d.abs() < 0.1, "taper frac={frac} d={d}");
    }
}

#[test]
fn ends_are_clamped_not_infinite() {
    let h = HelixParams { radius_mm: 10.0, taper_small_r: None, pitch_mm: 6.0,
                          turns: 4.0, profile: Profile::Circle { radius_mm: 1.0 }, axial_offset_mm: 0.0 };
    // Below the start: distance grows with axial gap, never negative.
    let d = sd_helix([10.0, -5.0, 0.0], &h);
    assert!(d > 4.0 - 1.0 - 0.1 && d.is_finite());
}
```

- [ ] **Step 2:** RED. **Step 3:** implement per the algorithm. **Step 4:** green + clippy. **Step 5: Commit** `feat(gui): helix SDF — periodic reduction with close-wound neighbor guard and taper`

### Task 3: SdfScene — composition, family builders, degenerate discipline

**Files:**
- Modify: `springmaker/src/viz/sdf.rs` (SdfScene + eval + builders)
- Test: same module

**Interfaces:**

```rust
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SdfPart {
    Helix(HelixParams),
    TorusArc { center: Vec3, y_rotation: f64, tilt: f64, major_r: f64, minor_r: f64, sweep: f64 },
    Capsule { a: Vec3, b: Vec3, radius_mm: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GroundPlane { pub point: Vec3, pub normal: Vec3 }

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScenePart { pub shape: SdfPart, pub appearance: Appearance }

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SdfScene { pub parts: Vec<ScenePart>, pub cuts: Vec<GroundPlane> }

pub(crate) fn sdf_eval(scene: &SdfScene, p: Vec3) -> f64;       // min over parts, max over cuts
pub(crate) fn sdf_eval_part(scene: &SdfScene, p: Vec3) -> (f64, usize); // + winning part index (appearance lookup)
pub(crate) fn scene_extent_mm(scene: &SdfScene) -> Option<f64>; // None when parts empty → placeholder

// Family builders (same engine fields the wireframe scene_models read;
// same capped/degenerate checks — an empty/hostile design yields SdfScene::default()):
pub(crate) fn compression_sdf(d: &springcore::SpringDesign) -> SdfScene;   // + ground cuts per EndType
pub(crate) fn conical_sdf(d: &springcore::conical::ConicalDesign) -> SdfScene;
pub(crate) fn extension_sdf(d: &springcore::extension::ExtensionDesign) -> SdfScene; // body + 2 TorusArc hooks
pub(crate) fn torsion_sdf(d: &springcore::torsion::TorsionDesign) -> SdfScene;      // body + 2 Capsule legs
pub(crate) fn assembly_sdf(d: &springcore::assembly::AssemblyDesign) -> SdfScene;   // union of members, per-member appearance
```

Builder rules: reuse the numeric relationships from each family's wireframe `scene_model.rs` (READ them; radii/pitch/turn/offset/hook/leg values must MATCH the wireframe geometry — same design renders the same shape both paths). Ground cuts: for compression/conical `EndType` squared-ground variants, one plane per ground end (point at the ground height, normal ±Y). Degenerate/capped: run the same guards the wireframe builders run (non-finite or `> MAX_RENDER_TURNS` coil counts, empty members) → `SdfScene::default()`; `scene_extent_mm(default) == None` drives the existing placeholder.

- [ ] **Step 1: failing tests** — per family: a solved fixture (copy each `scene_model.rs` test fixture verbatim) asserting (a) part counts (compression: 1 helix + cuts per end type; extension: 1 helix + 2 torus arcs; torsion: 1 helix + 2 capsules; conical: tapered helix; assembly nested: N member helices with distinct appearances), (b) surface-sample agreement: sample the wireframe centerline points (call the family's existing `*_scene` fn!) and assert `sdf_eval` at each centerline point ≈ `-wire_r` within 10% (the two geometry paths describe the SAME spring); (c) degenerate + capped fixtures → `SdfScene::default()`; (d) `sdf_eval` union == elementwise min (construct a two-part scene, compare against manual min); (e) ground-cut flattening: a compression squared-ground fixture's eval at a point below the ground plane is ≥ the plane distance.
- [ ] **Step 2:** RED. **Step 3:** implement. **Step 4:** green + clippy. **Step 5: Commit** `feat(gui): SDF scene composition and per-family builders`

### Task 4: Uniform packing + camera math + zoom

**Files:**
- Modify: `springmaker/src/viz/sdf.rs` (packing + shared constants), `springmaker/src/viz/mod.rs` (camera + zoom pure fns)
- Test: both modules

**Interfaces:**

```rust
// sdf.rs — the shared-constants source of truth (WGSL placeholders substituted in Task 5):
pub(crate) const MAX_PARTS: usize = 24;         // fits max assembly members × parts + cuts
pub(crate) const MAX_CUTS: usize = 4;
pub(crate) const MARCH_MAX_STEPS: u32 = 160;
pub(crate) const MARCH_SAFETY: f32 = 0.8;
pub(crate) const MARCH_EPS: f32 = 1e-3;         // mm-scale surface epsilon
pub(crate) const FLOATS_PER_PART: usize = 16;   // fixed stride, layout documented per variant
pub(crate) const FLOATS_PER_CUT: usize = 8;

pub(crate) fn scene_uniforms(scene: &SdfScene) -> Option<Vec<f32>>;
    // None if parts.len() > MAX_PARTS || cuts.len() > MAX_CUTS (representability guard →
    // placeholder, never truncation); layout: [n_parts, n_cuts, pad, pad] then parts then cuts,
    // each part: [kind, 15 kind-specific floats documented in a layout table comment],
    // appearance packed per part.
pub(crate) fn unpack_scene(u: &[f32]) -> Option<SdfScene>;  // test-only inverse (behind #[cfg(test)])

// mod.rs — camera:
pub(crate) const ZOOM_MIN: f32 = 0.3;
pub(crate) const ZOOM_MAX: f32 = 4.0;
pub(crate) fn zoom_step(current: f32, scroll_delta: f32) -> f32; // multiplicative, clamped, non-finite-guarded
pub(crate) fn camera_uniforms(extent_mm: f64, orbit: Orbit, zoom: f32, aspect: f32) -> [f32; 32];
    // view-proj matrix + inverse + eye position, fit-to-extent distance / zoom,
    // equal scale on all axes (aspect-honesty), 45° vertical FOV, near/far from extent
```

- [ ] **Step 1: failing tests:** representability per family at fixtures + the assembly member cap (build max-member assembly → Some; MAX_PARTS+1 synthetic parts → None); pack→unpack round-trip equality for a three-part scene with cuts; zoom_step clamps at EXACTLY 0.3/4.0, NaN/inf delta returns current (orbit_step precedent); camera: known orbit/zoom/extent → pinned eye distance; matrix × its inverse ≈ identity; two different aspects keep the extent fully inside the frustum (project the extent-sphere's 6 poles, all NDC ≤ 1).
- [ ] **Step 2:** RED. **Step 3:** implement (matrices hand-rolled — 4×4 helpers local to mod.rs; no new deps). **Step 4:** green + clippy. **Step 5: Commit** `feat(gui): SDF uniforms, camera math, zoom step`

### Task 5: shader3d.rs — WGSL + Pipeline/Primitive/Program

**Files:**
- Create: `springmaker/src/viz/shader3d.rs`, `springmaker/src/viz/shader3d.wgsl`
- Modify: `springmaker/src/viz/mod.rs` (`pub mod shader3d;`)
- Test: shader3d.rs `mod tests` (constant-substitution + event mapping; NO GPU tests)

**Interfaces:**
- Wgpu type path: `use iced_wgpu::wgpu;` is NOT directly available (iced doesn't re-export iced_wgpu publicly from the umbrella crate in all configurations) — FIRST try `iced::widget::shader::wgpu` (check the vendored shader module for a `pub use` — iced_widget/shader.rs re-exports `crate::renderer::wgpu::primitive::{Pipeline, Primitive, Storage}`; the `wgpu` crate itself may be re-exported at `iced::wgpu` — grep `pub use iced_wgpu` in iced-0.14.0/src/lib.rs). If no public re-export exists, add `iced_wgpu = "0.14"` to springmaker's Cargo.toml (version-locked to iced's) and `use iced_wgpu::wgpu;` — record which in the report.
- `pub(crate) struct SpringShader { pub uniforms: Vec<f32>, pub camera: [f32; 32], pub bg: [f32; 4] }` implementing `shader::Program<Message>`:
  - `type State = DragState` (mirror canvas3d's: `last: Option<Point>`);
  - `update`: ButtonPressed(in-bounds) starts drag; CursorMoved publishes `Message::Orbit(dx, dy)` via `Action::publish`; ButtonReleased/CursorLeft ends; `WheelScrolled { delta }` publishes `Message::Zoom(lines-or-pixels normalized: lines×0.1, pixels×0.002)`; request_redraw on publish (mirror OrbitCanvas's discipline exactly — read it first).
  - `draw`: clones the packed uniforms into the Primitive.
- `struct SpringPrimitive { … } impl iced_wgpu::Primitive`: `type Pipeline = SpringPipeline`; `prepare` writes camera+scene uniform buffers (create-or-grow in the Pipeline); `draw(&self, pipeline, render_pass) -> bool` sets pipeline + bind group, draws a 3-vertex fullscreen triangle, returns `true`.
- `SpringPipeline::new(device, queue, format)`: loads `shader3d.wgsl` via `include_str!`, substitutes the `{{CONST_*}}` placeholders from the Task-4 constants (`format!`/replace), creates the render pipeline (no depth, no MSAA — iced's pass), uniform buffers + bind group layout.
- WGSL content (write it COMPLETELY in this task; the fragment shader mirrors sdf.rs function-for-function): cylindrical reduction helix SDF with 4 turn candidates, torus-arc, capsule, profile (circle + rectangle), union/cuts loops over the packed arrays, sphere-traced march loop (`MARCH_MAX_STEPS`, `MARCH_SAFETY`, `MARCH_EPS`), normals via central-difference gradient (h = 2×EPS), Blinn-Phong: key light fixed in camera space (dir normalize(-0.4, 0.7, 0.6)), ambient = bg luminance × 0.35, specular pow lerped by roughness (shininess = mix(8, 128, 1-roughness)), metallic lerps specular color toward base_color, AO = 5-tap distance-field occlusion along the normal, miss → bg color. Vertex shader: fullscreen triangle from vertex_index, pass NDC ray through the inverse view-proj.
- The Rust file carries a `const WGSL_TEMPLATE: &str = include_str!("shader3d.wgsl");` and `fn instantiate_wgsl() -> String`.

- [ ] **Step 1: failing tests (no GPU):** `instantiate_wgsl()` output contains no `{{` (all placeholders substituted) AND contains the stringified values of `MARCH_MAX_STEPS`/`MAX_PARTS` (drift pin); `naga` validation — iced_wgpu depends on naga; if `naga` is reachable as a dev-dep, parse-validate the instantiated WGSL in a unit test (`naga::front::wgsl::parse_str` + validate) — this is the strongest no-GPU shader gate; add `naga` as a dev-dependency version-matched to iced_wgpu's. Event-mapping tests: construct the Program, feed synthetic shader::Events through `update`, assert published Orbit/Zoom deltas + drag lifecycle (mirror canvas3d's tests).
- [ ] **Step 2:** RED. **Step 3:** implement fully. **Step 4:** green + BOTH clippy + `cargo build -p springmaker` (the pipeline code compiles against real wgpu types). **Step 5: Commit** `feat(gui): shaded 3D shader — WGSL ray-marcher + iced pipeline`

### Task 6: App wiring — zoom state, shader probe, Spring3d arm chooser

**Files:**
- Modify: `springmaker/src/app.rs` (zoom field + Message::Zoom + shader_available), `springmaker/src/main.rs` (boot probe), the five family `view.rs` Spring3d arms, `springmaker/src/viz/mod.rs` (chooser + element builder)
- Test: app.rs tests + ui_tests.rs

**Interfaces:**
- `App.zoom: f32` (init 1.0), `App.shader_available: bool` (init FALSE — tests and `App::from_store` default to the deterministic wireframe path; main.rs's `initial_app` sets it via the probe).
- `Message::Zoom(f32)` arm: `self.zoom = crate::viz::zoom_step(self.zoom, delta); false` (non-recompute; orbit-arm precedent).
- Probe in main.rs: `fn shader_probe() -> bool { futures::executor::block_on(async { wgpu::Instance::default().request_adapter(&wgpu::RequestAdapterOptions::default()).await.is_some() }) }` (path per Task 5's wgpu resolution; `futures` executor — check springmaker's existing deps; if `futures` isn't a direct dep, use `pollster = "0.4"` as the smallest new dep and record it).
- `pub(crate) fn spring3d_element(pal, scene: SceneData, sdf: SdfScene, orbit, zoom, shader_available) -> Element<Message>` in viz/mod.rs: pure chooser `fn use_shaded(shader_available: bool, uniforms: Option<&Vec<f32>>) -> bool` (true iff available AND representable) + the element builder — shaded path builds `SpringShader` (camera from `scene_extent_mm`), fallback path calls the existing `scene_element(pal, scene, orbit)`. Degenerate scenes short-circuit to the existing placeholder BEFORE choosing.
- Family views: the Spring3d arm gains the sdf builder call: `crate::viz::spring3d_element(pal, compression_scene(&outcome.design), crate::viz::sdf::compression_sdf(&outcome.design), app.orbit, app.zoom, app.shader_available)` (each family its own builder pair).

- [ ] **Step 1: failing tests:** `use_shaded` truth table (available×representable); Zoom arm update-boundary tests (clamp via zoom_step, non-recompute, action_error + solve-error sentinels preserved, NaN delta no-op) mirroring the orbit tests; ui_tests: all existing Spring3d pins stay green with `shader_available=false` (the default — assert one explicitly: `test_app().shader_available == false`); a `spring3d_arm_dispatches_shaded_when_available` pin — set `app.shader_available = true` on a solved app, render, assert NEITHER placeholder shows AND the wireframe's... (the shaded widget has no queryable text; pin the DISPATCH at the chooser level — the ui-level pin asserts only no-placeholder/no-panic; the chooser unit test carries the branch logic). HARD RULE check: grep the test suite — no `snapshot_hash` call site may build an app with `shader_available = true` (add a comment on the field documenting why).
- [ ] **Step 2:** RED. **Step 3:** implement. **Step 4:** full suite + both clippy. **Step 5: Commit** `feat(gui): shaded/wireframe dispatch, zoom state, boot-time shader probe`

### Task 7: Full gate + manual visual checklist

- [ ] **Step 1:** full gate: `cargo fmt --all --check`; `cargo test --workspace`; BOTH clippy; `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`; `typos`; record counts.
- [ ] **Step 2:** append to the report a MANUAL CHECKLIST for the human (cannot be automated headless): shaded view renders on a real display for all five families; orbit + zoom feel; close-wound extension body shows no between-coil artifacts; ground ends read flat; both themes' backgrounds; assembly member hues distinguishable. These are demo-critical and PR-body items.
- [ ] **Step 3: Commit** any straggler fixes; otherwise no-op.

---

## Self-review notes (applied)

- Spec coverage: contract+mirror (T1-T3), uniforms/camera/zoom (T4), WGSL+pipeline+widget (T5), fallback/probe/state (T6), gates (T7). Mirror-drift mitigations: shared consts + placeholder substitution + naga parse-validation (T5) + drift pin. Simulator hard rule encoded (T6). All three resolved verification items are stated as verified facts in Global Constraints with their consequences.
- Type consistency: `HelixParams`/`SdfScene`/`scene_uniforms` names identical across T2-T6; `zoom_step`/`camera_uniforms` T4→T5/T6; `spring3d_element` T6 signature matches the family-view call.
- Judgment points left to implementers ON PURPOSE (record in reports): torus-arc endpoint-cap distance formulation (conservative bound required); WGSL wgpu-type re-export path vs direct dep; pollster-vs-futures for the probe; exact hue-shift table values; the 8%/10% property-test tolerances may need loosening with evidence if the reduction approximation demands it (document measured error if so).
