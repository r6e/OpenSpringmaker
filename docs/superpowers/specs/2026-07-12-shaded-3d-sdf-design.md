# Shaded 3D View (SDF ray-marching) — Design

**Date:** 2026-07-12
**Status:** Approved (brainstorm 2026-07-12)
**Increment:** first of two post-demo-prep stretch features (shaded 3D → 2D
engineering diagram). Follow-on to the shipped wireframe 3D (PR #63) and the
theming system (PR #65).

## Goal

The `Spring3d` visual mode renders a shaded, materially-plausible spring —
smooth silhouettes at any zoom, metallic lighting, exact ground ends —
replacing the plotters wireframe as the primary 3D look, with the wireframe
retained as the automatic fallback and Simulator-pinned path.

## Decisions (from the brainstorm)

1. **Replace, wireframe as fallback.** The existing `VisualMode::Spring3d`
   slot renders the shaded view when the wgpu renderer is active; the
   plotters wireframe (`scene_element`) stays as the automatic fallback and
   remains the Simulator-testable path. No third toggle mode; no code
   deleted.
2. **Fidelity scope (all in):** metallic material look; rectangular
   cross-section support in the geometry contract (the accepted-limitations
   table from the 3D-viz spec names this as the wgpu path's purpose);
   scroll-wheel zoom; ground/flattened end rendering; and a forward-ready
   per-part `Appearance` contract so future per-material render properties
   plug in without a contract change.
3. **Integration: iced's `shader` widget** (Approach A) — ships in iced
   0.14's default features, which springmaker already enables; zero new
   dependencies. Offscreen render-to-texture (B) was declined as paying a
   per-frame GPU→CPU copy and duplicating device management; CPU software
   shading (C) as not the named ask with worse lighting-per-effort.
4. **Geometry: SDF ray-marching, not polygonal sweep.** The domain is
   nearly closed-form — circle/rectangle profiles swept along helices, torus
   arcs (hooks), capsules (legs), plane cuts (ground ends) — so a
   signed-distance description gives resolution-independent silhouettes,
   exact ground-end cuts (`max(sdf, plane)`), trivial assembly composition
   (`min`), no mesh budget at high turn counts, and near-free ambient
   occlusion. The polygonal sweep is the documented fallback STRATEGY if
   close-wound artifacts resist tuning (the widget/camera layer is identical
   either way). Known approximations, named: the helix nearest-point problem
   is transcendental, so the helix SDF uses periodic reduction checking 2–3
   neighboring turns (the close-wound guard); the conical taper uses a
   local-pitch approximation.

## Architecture (ADR 0008 split)

```
springmaker/src/viz/
├── mod.rs        existing: SceneData/samplers/orbit (unchanged consumers)
├── sdf.rs        NEW pure: SdfScene contract + Rust-mirror evaluation +
│                 uniform packing
├── shader3d.rs   NEW humble: iced shader::Program — WGSL pipeline,
│                 buffers/uniforms, camera
├── render3d.rs   existing wireframe renderer (fallback path, unchanged)
└── canvas3d.rs   existing OrbitCanvas (fallback path; shaded widget handles
                  its own input variant — see §Interaction)
```

### `viz/sdf.rs` (pure contract + Rust mirror)

- `SdfScene { parts: Vec<SdfPart>, ground_cuts: Vec<GroundPlane> }` built
  from the SAME engine fields the wireframe scene_models read. Parts:
  - `Helix { radius_mm, profile, pitch_mm, turns, taper: Option<(large_r, small_r)>, axial_offset_mm, appearance }`
  - `TorusArc { center, axis_frame, major_r, minor_profile, sweep_angle, appearance }` (extension hooks)
  - `Capsule { a, b, radius_mm, appearance }` (torsion legs)
- `Profile { Circle { radius_mm }, Rectangle { half_w_mm, half_h_mm } }` —
  circle in v1 rendering; rectangle carried in the contract for the
  rectangular-family increment.
- `Appearance { base_color: [f32; 3], metallic: f32, roughness: f32 }` —
  v1 derives spring-steel defaults; assembly members differentiate by a hue
  shift keyed off the existing role/index scheme; future material-DB render
  properties flow through this struct unchanged.
- `sdf_eval(&SdfScene, p: [f64; 3]) -> f64` — the Rust mirror of the WGSL:
  periodic-reduction helix distance with neighbor-turn checking, local-pitch
  taper, exact torus/capsule/plane primitives, profile distance in the local
  frame, `min` union across parts, `max` against ground cuts.
- `scene_uniforms(&SdfScene) -> Vec<f32>` — fixed-layout packing shared
  with the WGSL (layout constants from one Rust source of truth). A fixed
  part budget bounds the uniform size; every family's solved design must fit
  (representability test), including assemblies at the member cap.
- Degenerate discipline REUSED: the builder runs the same capped/
  non-finite/empty checks the wireframe path runs (`coil_body_is_empty`
  analog on the part list) BEFORE any uniforms exist — hostile or degenerate
  designs show the existing placeholders and never reach the march loop.

### `viz/shader3d.rs` (humble)

- `shader::Program` impl: one full-screen-quad pipeline; WGSL fragment
  shader ray-marches the scene uniforms, computes normals from the SDF
  gradient, shades with Blinn-Phong + a metallic term (tight specular),
  one camera-space key light + ambient tied to palette background
  luminance, SDF-derived ambient occlusion, background = `pal.panel`
  (both themes native).
- Camera: perspective projection; orbit yaw/pitch (existing `App.orbit`)
  plus zoom distance; fit-to-extent initial framing with equal scale on all
  axes (the wireframe increment's aspect-honesty lesson).
- Plan-time verification items (iced-internals empirics the design does not
  depend on): (a) `Shader` element behavior on the tiny-skia fallback
  renderer — the fallback-detection mechanism follows from the answer;
  (b) depth attachment availability/necessity (a single ray-marched quad
  needs no depth buffer — verify nothing else does either); (c) whether the
  Simulator can exercise shader output (golden pixel cross-checks against
  CPU-marching the mirror if yes; documented boundary if no).

## Interaction & state

- `App.zoom: f32` (default 1.0, clamped to [0.3, 4.0]) + `Message::Zoom(f32)`
  scroll-wheel deltas — same non-recompute, error-channel-preserving,
  no-op-guarded discipline as `Message::Orbit`. Orbit drag reuses the
  existing `Message::Orbit` deltas unchanged. The shader widget's
  `Program::update` handles its own mouse events and PUBLISHES those same
  `Message::Orbit`/`Message::Zoom` deltas (mirroring `OrbitCanvas`'s
  publish discipline) — drag tracking stays widget-local ephemeral state,
  committed angles/zoom stay App state.
- The `Spring3d` arm renders: wgpu active → shaded widget; else → the
  existing wireframe `scene_element`. Toggle, placeholder, and cross-family
  state contracts unchanged.

## Testing

- **Rust-mirror property tests carry the bulk:** surface points |sdf| < ε /
  inside < 0 / outside > 0 (per part type and composed); numeric-gradient
  normals unit-length and outward; neighbor-turn reduction never returns a
  farther-turn distance (close-wound correctness); ground cuts exact;
  taper monotonic; assembly union = elementwise min; uniform round-trip +
  representability at the part budget; appearance mapping per role/member.
- **Camera/zoom pure math pinned:** view/projection matrices for known
  inputs; zoom clamp boundaries; fit-to-extent equal-axis framing.
- **Simulator:** existing wireframe pins keep covering the fallback path;
  toggle/placeholder/state pins are renderer-independent; zoom gets
  update-boundary tests. Shader output coverage per plan-verification (c).
- **Named #1 risk — mirror drift** (Rust `sdf_eval` vs WGSL): mitigated by
  the shared-constants source of truth, the uniform-layout round-trip test,
  golden pixel cross-checks if (c) allows, and the panel's input-domain
  adversary attacking the mirror's domain edges (close-wound gaps, taper
  extremes, hostile-but-solvable coil counts — `MAX_RENDER_TURNS` caps
  upstream as today).
- Final adversarial panel per house rules; springmaker-only (mutation gate
  trivially clean); stateful lens on zoom × orbit × mode × theme × family.

## Out of scope

- Shadows, environment maps/reflections beyond the metallic tint.
- Per-material render-property DATA (the `Appearance` contract ships; the
  material-DB plumbing is a later increment).
- Hover/readout on the 3D view; export/screenshot.
- Rectangular-family RENDERING (the profile contract ships; the family
  arrives with its own GUI increment).
- The 2D engineering diagram (the follow-on increment, separate brainstorm).
