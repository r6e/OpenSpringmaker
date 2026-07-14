# 2D Engineering Diagram View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a third `VisualMode::Diagram` that renders a dimensioned, true-scale 2D engineering side elevation of the solved spring — reusing each family's `viz::SceneData`, drawn as a crossing double-strand wire silhouette on a native iced canvas with toggleable dimension layers and zoom/pan.

**Architecture:** Two pure concerns feed a shared pure layout engine and one humble view. `diagram/geometry.rs` projects the existing `SceneData` to `(axial, radial)` model-mm edges (wire silhouette = centerline offset ±wire/2). Per-family `<family>/diagram_model.rs` presenters produce `Vec<Dimension>` anchored to the geometry and labeled from design fields (the mirror-drift guard, asserted equal in tests). `diagram/layout.rs` places dimension lines (linear ladders + angular arcs) in model mm. `diagram/canvas.rs` applies one affine (fit→zoom→pan) and draws with native `Frame`/`Path`/`Text`.

**Tech Stack:** Rust, iced 0.14 (`widget::canvas`), the existing `springmaker::viz` scene contract. springmaker-only; no `springcore` changes.

## Global Constraints

- **springmaker-only** — no `springcore` edits. The `Polyline3::wire_mm` field added in Task 1 is in `springmaker/src/viz/`, not springcore.
- **ADR 0008** humble-view/presenter split: all layout/anchor/geometry math is pure and computed in **model mm**; the canvas applies exactly one affine plus the single constant-px font-size exception; no layout is derived from frame bounds.
- **Mirror-drift guard** — every dimension corresponding to a drawn feature is anchored to the projected geometry's computed bounds and labeled with the design-field value; a test asserts the two are equal within `max_relative = 1e-9`.
- **No-recompute discipline** — `DiagramZoom`/`DiagramPan`/`DiagramLayer` update arms return `false` and mutate only view state via single-writer step helpers with finiteness guards (mirrors `Message::Orbit`/`Message::Zoom`).
- **Degenerate/placeholder discipline** — a degenerate `SceneData` (empty/capped/non-finite, via `viz::scene_extent`) shows the existing placeholder wording; non-finite design fields never produce a NaN/inf label.
- **Both palettes** (light/dark): dimension lines, text, wire use palette tokens; no hardcoded colors.
- **No machine-dependent snapshot** of canvas output (fonts/AA vary) — same rule as the shader path. Assert on transform math and event→message mapping, not pixels.
- **Strict TDD**; run BOTH clippy commands, `cargo doc -D warnings`, repo-wide `typos` before each commit's task is marked done. springmaker is not mutation-gated (springcore stays 0 in-diff, untouched here).
- **Naming** — no commercial product/vendor names in any persisted file. Use `seg` (not the two-letter form) for any segment-vector identifier the `typos` gate would reject.

## Confirmed springcore design fields (bound during planning)

All dimensional fields are `springcore::units::Length` with `.millimeters()`. Coil counts / index are `f64`. `Force`/`Angle`/`Moment` expose their own SI accessors.

| Family | Struct | Fields used by the diagram |
|---|---|---|
| Compression | `SpringDesign` | `wire_dia`, `mean_dia`, `active_coils`, `total_coils`, `free_length`, `solid_length`, `pitch`, `outer_dia`, `inner_dia`, `end_type` |
| Conical | `conical::ConicalDesign` | `inputs.wire_dia`, `inputs.large_mean_dia`, `inputs.small_mean_dia`, `inputs.active_coils`, `inputs.free_length`, `large_outer_dia`, `large_inner_dia`, `small_outer_dia`, `small_inner_dia`, `total_coils`, `solid_length`, `pitch` |
| Extension | `extension::ExtensionDesign` | `wire_dia`, `mean_dia`, `active_coils`, `free_length`, `initial_tension` (`Force`), `outer_dia`, `inner_dia`, `hooks.r1`, `hooks.r2` |
| Torsion | `torsion::TorsionDesign` | `inputs.wire_dia`, `inputs.mean_dia`, `inputs.body_coils`, `inputs.leg1`, `inputs.leg2`, `inputs.arbor_dia` (`Option<Length>`), `index` |
| Assembly | `assembly::AssemblyDesign` | `topology` (`Topology::{Nested,Series}`), `members[i].design` (`SpringDesign`), `free_length`, `solid_length` |

## scene_model coordinate conventions (the anchoring truth)

Every scene is built in true mm with `y` = spring axis; the diagram projects `axial = y`, `radial = x`, dropping `z`.

- **Compression** (`compression_scene`): one Wire polyline; radius `mean_dia/2` constant; axial span `[0, H]` with `H == free_length` (pinned by the existing `compression_scene_matches_solved_geometry` test: `last.1 == dead*wire + active*pitch`).
- **Conical** (`conical_scene`): radius `r_large` at axial 0 → `r_small` at axial `H`; `r_large = large_mean_dia/2`, `r_small = small_mean_dia/2`; axial span `[0, free_length]`.
- **Extension** (`extension_scene`): Wire body `[0, body_h]` with `body_h = free_length - 2*(2*r1 - wire) - wire`; two Detail hook arcs, bottom hanging to `-2*r1`, top to `body_h + 2*r1`; the **inside-hooks span** (bottom inner surface → top inner surface) `== free_length` (pinned by `rendered_inside_hook_span_equals_the_specified_free_length`).
- **Torsion** (`torsion_scene`): close-wound Wire body `[0, body_coils*wire]`, radius `mean_dia/2`; two Detail straight legs **in the cross-section plane** (constant axial `y`), tangential at the body ends — leg1 tangent `(0,1)` in `(x,z)` collapses under a side projection, so torsion legs are drawn in an **end-on inset** (project `(x,z)`), not the side elevation.
- **Assembly** (`assembly_scene`): one polyline per member from `scene_from_radius`. Nested → concentric, all axial-start 0. Series → stacked with a `2*max_member_wire` gap between members, so the drawn axial span **exceeds** `design.free_length` (gaps are schematic spacing).

---

## File Structure

- `springmaker/src/viz/mod.rs` — MODIFY: add `pub wire_mm: f64` to `Polyline3`; set it at every construction site.
- `springmaker/src/diagram/mod.rs` — CREATE: module root; shared types (`P2`, `Edge2`, `Projected`, `Bounds`, `DimLayer`, `DimKind`, `Dimension`, `DiagramInput`, `Inset`, `DimLayers`, `DiagramView`); `diagram_element` entry + degenerate placeholder; view step helpers.
- `springmaker/src/diagram/geometry.rs` — CREATE: `project_silhouette` (family-agnostic projection + wire-edge offset) + `Bounds` computation.
- `springmaker/src/diagram/layout.rs` — CREATE: `layout` engine (linear ladders + angular arcs), all model mm.
- `springmaker/src/diagram/canvas.rs` — CREATE: `DiagramCanvas` humble view (affine, draw, scroll→zoom, drag→pan).
- `springmaker/src/compression/diagram_model.rs` — CREATE (Task 2).
- `springmaker/src/conical/diagram_model.rs` — CREATE (Task 6).
- `springmaker/src/extension/diagram_model.rs` — CREATE (Task 8).
- `springmaker/src/torsion/diagram_model.rs` — CREATE (Task 9).
- `springmaker/src/assembly/diagram_model.rs` — CREATE (Task 10).
- `springmaker/src/{compression,conical,extension,torsion,assembly}/mod.rs` — MODIFY: `mod diagram_model;`.
- `springmaker/src/{compression,conical,extension,torsion,assembly}/view.rs` — MODIFY: pass the `diagram` closure to `results_visual_element`.
- `springmaker/src/widgets.rs` — MODIFY: `visual_toggle` third segment; `results_visual_element` fourth closure; a `diagram_layer_toggle` row.
- `springmaker/src/app.rs` — MODIFY: `VisualMode::Diagram`; `Message::{DiagramZoom,DiagramPan,DiagramLayer}`; `App::{diagram_view, diagram_layers}`; update arms.
- `springmaker/src/main.rs` — MODIFY: `mod diagram;`.
- `springmaker/src/ui_tests.rs` — MODIFY: integration tests (Task 5).

---

## Task 1: Projection + wire silhouette (`diagram/` scaffold + `geometry.rs`)

**Files:**
- Modify: `springmaker/src/viz/mod.rs` (add `Polyline3::wire_mm`)
- Modify: `springmaker/src/main.rs` (add `mod diagram;`)
- Create: `springmaker/src/diagram/mod.rs`
- Create: `springmaker/src/diagram/geometry.rs`

**Interfaces:**
- Consumes: `viz::{SceneData, Polyline3, SceneRole, finite3}`.
- Produces:
  ```rust
  pub type P2 = (f64, f64); // (axial, radial) in model mm; axial = horizontal
  pub struct Edge2 { pub points: Vec<P2>, pub role: viz::SceneRole }
  pub struct Bounds { pub axial_min: f64, pub axial_max: f64, pub radial_min: f64, pub radial_max: f64 }
  pub struct Projected { pub edges: Vec<Edge2>, pub bounds: Bounds }
  pub fn project_silhouette(scene: &viz::SceneData) -> Option<Projected>;
  ```
  `None` when the scene is degenerate (mirrors `viz::scene_extent` returning `None`).

- [ ] **Step 1: Add `wire_mm` to `Polyline3`**

In `springmaker/src/viz/mod.rs`, extend the struct and set the field at every construction site. The 3D renderer reads only `.points/.role/.stroke_px`, so this is inert for the 3D path.

```rust
// viz/mod.rs — struct definition (was: points, role, stroke_px)
pub struct Polyline3 {
    pub points: Vec<(f64, f64, f64)>,
    pub role: SceneRole,
    /// Stroke width in pixels (from `stroke_for`) — 3D path only.
    pub stroke_px: u32,
    /// True wire diameter in mm — the 2D diagram's silhouette offset needs the
    /// real gauge (`stroke_px` is clamped to [1,8] and dimensionally dishonest).
    /// The 3D renderer ignores this field.
    pub wire_mm: f64,
}
```

Set `wire_mm` in `scene_from_radius` (both the degenerate empty-body branch and the normal branch):
```rust
// degenerate branch:
return SceneData {
    polylines: vec![Polyline3 { points: Vec::new(), role: SceneRole::Wire, stroke_px: 1, wire_mm }],
};
// normal branch:
SceneData {
    polylines: vec![Polyline3 {
        points, role: SceneRole::Wire, stroke_px: stroke_for(wire_mm, extent), wire_mm,
    }],
}
```
(`scene_from_radius` already binds `wire_mm` as its parameter name.)

- [ ] **Step 2: Fix the other `Polyline3` construction sites**

Set `wire_mm` on every remaining constructor so it carries the body's wire gauge:
- `extension/scene_model.rs` — both `scene.polylines.push(Polyline3 { ... })` for hooks: add `wire_mm: wire` (the `wire` local already exists).
- `torsion/scene_model.rs` — both leg pushes: add `wire_mm: wire`.
- `viz/canvas3d.rs` test `scene_with_nan_points` and any `viz/mod.rs` test constructing `Polyline3` literally: add `wire_mm: 1.0` (value irrelevant to those tests).
Run `cargo build -p springmaker` and fix any remaining literal `Polyline3 { ... }` the compiler flags (exhaustive struct literals fail loudly — the ADR 0013 compile-loud property).

- [ ] **Step 3: Write the failing projection tests**

Create `springmaker/src/diagram/geometry.rs` with the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::compression::scene_model::compression_scene;
    use approx::assert_relative_eq;
    use springcore::units::{Force, Length};
    use springcore::{EndFixity, EndType, MaterialSet, PowerUser};

    fn compression_design() -> springcore::SpringDesign {
        let m = MaterialSet::load_default().get("Music Wire").unwrap().clone();
        PowerUser {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            active: 10.0,
            free_length: Length::from_millimeters(60.0),
            loads: vec![Force::from_newtons(10.0), Force::from_newtons(30.0)],
        }
        .solve(&m, springcore::CurvatureCorrection::Bergstrasser)
        .unwrap()
    }

    #[test]
    fn projects_two_silhouette_edges_per_wire_polyline() {
        let scene = compression_scene(&compression_design());
        let p = project_silhouette(&scene).expect("finite scene projects");
        // One body Wire polyline → two silhouette edges (outer + inner),
        // index-aligned with the centerline.
        assert_eq!(p.edges.len(), 2);
        assert!(p.edges.iter().all(|e| e.points.len() == scene.polylines[0].points.len()));
    }

    #[test]
    fn silhouette_edge_separation_equals_the_true_wire_diameter() {
        // The point of the crossing double-strand choice: the perpendicular gap
        // between the two silhouette edges is the TRUE wire diameter in model mm
        // (a real geometric width, not a clamped stroke). EXACT at every sample —
        // this is the honest-thickness invariant that makes OD/ID fall out.
        let d = compression_design(); // wire 2mm
        let p = project_silhouette(&compression_scene(&d)).unwrap();
        let (outer, inner) = (&p.edges[0], &p.edges[1]);
        assert_eq!(outer.points.len(), inner.points.len());
        for (a, b) in outer.points.iter().zip(&inner.points) {
            assert_relative_eq!((a.0 - b.0).hypot(a.1 - b.1), d.wire_dia.millimeters(), max_relative = 1e-9);
        }
    }

    #[test]
    fn silhouette_midpoint_at_the_axial_start_is_the_mean_radius() {
        // Drop-z side elevation: radial = x. At the first sample (θ=0) the
        // centerline sits at radial = mean/2 exactly, and the two silhouette
        // points straddle it, so their midpoint == mean/2. EXACT — ties the
        // drawn geometry to the mean-diameter field regardless of the discrete
        // perpendicular's direction.
        let d = compression_design(); // mean 20mm
        let p = project_silhouette(&compression_scene(&d)).unwrap();
        let mid = (p.edges[0].points[0].1 + p.edges[1].points[0].1) / 2.0;
        assert_relative_eq!(mid, d.mean_dia.millimeters() / 2.0, max_relative = 1e-9);
    }

    #[test]
    fn axial_span_matches_free_length() {
        // axial = y ∈ [0, H] with H = free_length (compression scene_model). The
        // silhouette offset perturbs each end by at most wire/2, so the edge
        // bounds' axial span sits within a wire diameter of the true free length.
        let d = compression_design();
        let p = project_silhouette(&compression_scene(&d)).unwrap();
        let span = p.bounds.axial_max - p.bounds.axial_min;
        assert!(
            (span - d.free_length.millimeters()).abs() <= d.wire_dia.millimeters(),
            "axial span {span} not within one wire dia of free length {}",
            d.free_length.millimeters()
        );
    }

    #[test]
    fn degenerate_scene_projects_to_none() {
        let mut d = compression_design();
        d.mean_dia = Length::from_millimeters(f64::NAN);
        let scene = compression_scene(&d);
        assert!(project_silhouette(&scene).is_none());
    }
}

// PROJECTION MODEL (locked): drop-z side elevation — `radial = x`, `axial = y`,
// z discarded. This is what makes the crossing double-strand look: radial
// oscillates ±R along the axis, so adjacent coils' sine ribbons overlap and
// their edges cross (the textbook interlocking-X look, for the true-scale case
// diameter > pitch). Do NOT switch to `radial = hypot(x,z)` (the developed/
// envelope model) — it draws a flat constant-radius band with no visible coils.
// Consequence: the OUTER silhouette envelope only approaches OD/2 to sampling
// resolution (~3% at 32 samples/turn), so OD/ID are NOT verified from the
// geometry envelope here — they are verified in Task 2 as `presenter-half ==
// design OD/2` (exact). The EXACT geometry ties in Task 1 are edge-separation
// == wire_dia and edge-midpoint == mean/2.
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cargo test -p springmaker diagram::geometry`
Expected: FAIL — `project_silhouette` not found.

- [ ] **Step 5: Implement `project_silhouette`**

```rust
//! Family-agnostic projection of a 3D `SceneData` to the 2D side elevation:
//! `axial = y`, `radial = x` (drop `z`), then each polyline's centerline is
//! offset ±wire/2 perpendicular to its local 2D tangent to draw the wire's two
//! silhouette edges (the crossing double-strand look). All coordinates are
//! model mm; the humble canvas applies the only affine.

use crate::viz::{self, SceneData};

pub type P2 = (f64, f64);

pub struct Edge2 {
    pub points: Vec<P2>,
    pub role: viz::SceneRole,
}

pub struct Bounds {
    pub axial_min: f64,
    pub axial_max: f64,
    pub radial_min: f64,
    pub radial_max: f64,
}

pub struct Projected {
    pub edges: Vec<Edge2>,
    pub bounds: Bounds,
}

/// Project a 3D point to `(axial, radial)`: axial = y (the spring axis, drawn
/// horizontal), radial = x. z is dropped (orthographic side elevation).
fn to_2d(p: (f64, f64, f64)) -> P2 {
    (p.1, p.0)
}

/// Unit perpendicular to the local tangent at centerline index `i`, using the
/// adjacent segment(s). Perpendicular of tangent (tx, ty) is (-ty, tx).
/// Returns (0, 0) for a zero-length tangent (coincident points).
fn normal_at(pts: &[P2], i: usize) -> P2 {
    let n = pts.len();
    let (a, b) = if i == 0 {
        (pts[0], pts[1])
    } else if i == n - 1 {
        (pts[n - 2], pts[n - 1])
    } else {
        (pts[i - 1], pts[i + 1])
    };
    let (tx, ty) = (b.0 - a.0, b.1 - a.1);
    let len = tx.hypot(ty);
    if len == 0.0 {
        (0.0, 0.0)
    } else {
        (-ty / len, tx / len)
    }
}

pub fn project_silhouette(scene: &SceneData) -> Option<Projected> {
    // Gate on the same finiteness contract the renderer uses.
    viz::scene_extent(scene)?;

    let mut edges = Vec::with_capacity(scene.polylines.len() * 2);
    for line in &scene.polylines {
        // Skip empty (capped) bodies; scene_extent already vetoed a fully
        // degenerate scene, but a mixed scene could carry an empty polyline.
        if line.points.len() < 2 {
            continue;
        }
        let center: Vec<P2> = line.points.iter().copied().map(to_2d).collect();
        let wire_r = line.wire_mm / 2.0;
        let mut outer = Vec::with_capacity(center.len());
        let mut inner = Vec::with_capacity(center.len());
        for i in 0..center.len() {
            let (nx, ny) = normal_at(&center, i);
            let (cx, cy) = center[i];
            outer.push((cx + nx * wire_r, cy + ny * wire_r));
            inner.push((cx - nx * wire_r, cy - ny * wire_r));
        }
        edges.push(Edge2 { points: outer, role: line.role });
        edges.push(Edge2 { points: inner, role: line.role });
    }

    let mut b = Bounds {
        axial_min: f64::INFINITY,
        axial_max: f64::NEG_INFINITY,
        radial_min: f64::INFINITY,
        radial_max: f64::NEG_INFINITY,
    };
    for (a, r) in edges.iter().flat_map(|e| e.points.iter().copied()) {
        if !a.is_finite() || !r.is_finite() {
            continue;
        }
        b.axial_min = b.axial_min.min(a);
        b.axial_max = b.axial_max.max(a);
        b.radial_min = b.radial_min.min(r);
        b.radial_max = b.radial_max.max(r);
    }
    (b.axial_min.is_finite() && b.axial_max.is_finite()).then_some(Projected { edges, bounds: b })
}
```

Add the module wiring. `springmaker/src/diagram/mod.rs`:
```rust
//! 2D engineering-diagram visual mode (ADR 0008): pure projection
//! (`geometry`) + pure layout (`layout`) feeding the humble `canvas`.
pub mod geometry;

pub use geometry::{project_silhouette, Bounds, Edge2, Projected, P2};
```
`springmaker/src/main.rs`: add `mod diagram;` in the module-declaration group (alphabetical among the sibling `mod` lines near `mod plot;`/`mod viz;`).

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p springmaker diagram::geometry`
Expected: PASS (5 tests). Then `cargo test -p springmaker viz::` and the family `scene_model` tests to confirm the `wire_mm` addition broke nothing.

- [ ] **Step 7: Lint + commit**

```bash
cargo fmt -p springmaker
cargo clippy -p springmaker --all-targets -- -D warnings
cargo clippy -p springmaker --all-targets --no-default-features -- -D warnings
cargo doc -p springmaker --no-deps -D warnings
typos
git add springmaker/src/viz/mod.rs springmaker/src/extension/scene_model.rs springmaker/src/torsion/scene_model.rs springmaker/src/viz/canvas3d.rs springmaker/src/diagram/ springmaker/src/main.rs
git commit -m "feat(diagram): project SceneData to 2D wire-silhouette edges"
```

---

## Task 2: Dimension model + compression dimension presenter

**Files:**
- Modify: `springmaker/src/diagram/mod.rs` (add the `Dimension` data model)
- Create: `springmaker/src/compression/diagram_model.rs`
- Modify: `springmaker/src/compression/mod.rs` (`mod diagram_model;`)

**Interfaces:**
- Consumes: `diagram::{P2, project_silhouette}`, `springcore::SpringDesign`.
- Produces:
  ```rust
  pub enum DimLayer { Lengths, Diameters, Coils }
  pub enum DimKind {
      Linear { from: P2, to: P2 },           // measures the |to - from| distance
      Diameter { at_axial: f64, half: f64 }, // full extent 2*half at station at_axial
      Angular { vertex: P2, start_deg: f64, sweep_deg: f64, radius: f64 },
      Note,
  }
  pub struct Dimension { pub kind: DimKind, pub layer: DimLayer, pub value: f64, pub label: String, pub at: P2 }
  pub fn compression::diagram_model::dimensions(design: &SpringDesign) -> Vec<Dimension>;
  ```

- [ ] **Step 1: Add the `Dimension` data model to `diagram/mod.rs`**

```rust
/// Which toggleable layer a dimension belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DimLayer {
    Lengths,
    Diameters,
    Coils,
}

/// The geometric primitive a dimension draws as. Coordinates are model mm in
/// projection space `(axial, radial)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DimKind {
    /// Distance between two anchor points (measured along the line joining them).
    Linear { from: P2, to: P2 },
    /// A diameter across the envelope at axial station `at_axial`, full span `2*half`.
    Diameter { at_axial: f64, half: f64 },
    /// Angular measurement: `sweep_deg` from `start_deg`, drawn at arc `radius`.
    Angular { vertex: P2, start_deg: f64, sweep_deg: f64, radius: f64 },
    /// Text-only annotation placed at `at` by the layout engine (no line).
    Note,
}

/// One callout: geometry (`kind`), which layer it toggles with, the numeric
/// `value` from the design field (the label source), the formatted `label`,
/// and a reference anchor `at` on the geometry (leader origin for `Note`s).
#[derive(Debug, Clone, PartialEq)]
pub struct Dimension {
    pub kind: DimKind,
    pub layer: DimLayer,
    pub value: f64,
    pub label: String,
    pub at: P2,
}
```
Add `pub mod` / `pub use` so `Dimension`, `DimKind`, `DimLayer` are reachable as `crate::diagram::Dimension` etc.

- [ ] **Step 2: Write the failing compression dimension tests**

`springmaker/src/compression/diagram_model.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram::{project_silhouette, DimKind, DimLayer};
    use crate::compression::scene_model::compression_scene;
    use approx::assert_relative_eq;
    use springcore::units::{Force, Length};
    use springcore::{EndFixity, EndType, MaterialSet, PowerUser, SpringDesign};

    fn design() -> SpringDesign {
        let m = MaterialSet::load_default().get("Music Wire").unwrap().clone();
        PowerUser {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            active: 10.0,
            free_length: Length::from_millimeters(60.0),
            loads: vec![Force::from_newtons(10.0), Force::from_newtons(30.0)],
        }
        .solve(&m, springcore::CurvatureCorrection::Bergstrasser)
        .unwrap()
    }

    fn find(dims: &[Dimension], label_starts: &str) -> Dimension {
        dims.iter().find(|d| d.label.starts_with(label_starts))
            .unwrap_or_else(|| panic!("no dimension labeled {label_starts}: {:?}",
                dims.iter().map(|d| &d.label).collect::<Vec<_>>()))
            .clone()
    }

    #[test]
    fn free_length_dimension_matches_the_projected_body_span() {
        let d = design();
        let dims = dimensions(&d);
        let fl = find(&dims, "L\u{2080}"); // "L₀"
        assert_eq!(fl.layer, DimLayer::Lengths);
        // Presenter ↔ design: EXACT (the callout number is the design field).
        assert_relative_eq!(fl.value, d.free_length.millimeters(), max_relative = 1e-9);
        // Presenter anchor span == the labeled value: EXACT (the drawn dimension
        // line length equals its own number).
        let DimKind::Linear { from, to } = fl.kind else { panic!("free length must be a Linear dim") };
        assert_relative_eq!((to.0 - from.0).abs(), d.free_length.millimeters(), max_relative = 1e-9);
        // Mirror-drift vs geometry: the projected silhouette's axial span matches
        // free_length to within a wire diameter (the drop-z offset perturbs the
        // ends by ≤ wire/2 each — the envelope peak is sampling-approximate, see
        // Task 1's PROJECTION MODEL note).
        let b = project_silhouette(&compression_scene(&d)).unwrap().bounds;
        let axial = b.axial_max - b.axial_min;
        assert!((axial - d.free_length.millimeters()).abs() <= d.wire_dia.millimeters());
    }

    #[test]
    fn outer_diameter_dimension_matches_the_projected_outer_edge() {
        let d = design();
        let dims = dimensions(&d);
        let od = find(&dims, "OD");
        assert_eq!(od.layer, DimLayer::Diameters);
        assert_relative_eq!(od.value, d.outer_dia.millimeters(), max_relative = 1e-9);
        if let DimKind::Diameter { half, .. } = od.kind {
            assert_relative_eq!(2.0 * half, d.outer_dia.millimeters(), max_relative = 1e-9);
        } else {
            panic!("OD must be a Diameter dim");
        }
    }

    #[test]
    fn inner_diameter_dimension_matches_id() {
        let d = design();
        let od = find(&dimensions(&d), "ID");
        if let DimKind::Diameter { half, .. } = od.kind {
            assert_relative_eq!(2.0 * half, d.inner_dia.millimeters(), max_relative = 1e-9);
        } else {
            panic!("ID must be a Diameter dim");
        }
    }

    #[test]
    fn wire_and_coil_and_solid_callouts_are_present_and_layered() {
        let d = design();
        let dims = dimensions(&d);
        assert_eq!(find(&dims, "\u{2300}").layer, DimLayer::Diameters); // wire ⌀
        // Coil-count note carries total & active, in the Coils layer.
        let coils = find(&dims, "N");
        assert_eq!(coils.layer, DimLayer::Coils);
        assert_eq!(coils.kind, DimKind::Note);
        // Solid length is a Lengths reference dim.
        assert_relative_eq!(find(&dims, "L\u{209B}").value, d.solid_length.millimeters(), max_relative = 1e-9);
    }

    #[test]
    fn degenerate_design_yields_finite_labels_only() {
        let mut d = design();
        d.mean_dia = Length::from_millimeters(f64::NAN);
        // A NaN field must not crash the presenter; labels stay finite-guarded.
        let dims = dimensions(&d);
        assert!(dims.iter().all(|dm| dm.value.is_finite() || dm.label.contains('\u{2014}')));
    }
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p springmaker compression::diagram_model`
Expected: FAIL — `dimensions` not found.

- [ ] **Step 4: Implement the compression presenter**

```rust
//! Pure 2D-diagram dimension presenter for the compression family (ADR 0008).
//! Anchors are in projection space `(axial, radial)` model mm; axial spans
//! `[0, free_length]` and the radial envelope is ±OD/2 (see scene_model). Each
//! feature dimension is anchored to that geometry and labeled from the design
//! field — the mirror-drift equality is asserted in tests.

use crate::diagram::{DimKind, DimLayer, Dimension, P2};
use springcore::SpringDesign;

/// Format a millimetre value, or an em dash for a non-finite field so no NaN/inf
/// ever reaches a label (defense in depth; the engine rejects these upstream).
fn mm(v: f64) -> String {
    if v.is_finite() {
        format!("{v:.1}")
    } else {
        "\u{2014}".into() // em dash
    }
}

pub fn dimensions(design: &SpringDesign) -> Vec<Dimension> {
    let l0 = design.free_length.millimeters();
    let ls = design.solid_length.millimeters();
    let od = design.outer_dia.millimeters();
    let id = design.inner_dia.millimeters();
    let wire = design.wire_dia.millimeters();
    let na = design.active_coils;
    let nt = design.total_coils;
    let mid = l0 / 2.0; // an axial station for the diameter callouts

    vec![
        Dimension {
            kind: DimKind::Linear { from: (0.0, 0.0), to: (l0, 0.0) },
            layer: DimLayer::Lengths,
            value: l0,
            label: format!("L\u{2080} {}", mm(l0)), // L₀
            at: (mid, 0.0),
        },
        Dimension {
            kind: DimKind::Linear { from: (0.0, 0.0), to: (ls, 0.0) },
            layer: DimLayer::Lengths,
            value: ls,
            label: format!("L\u{209B} {}", mm(ls)), // Lₛ (reference)
            at: (ls / 2.0, 0.0),
        },
        Dimension {
            kind: DimKind::Diameter { at_axial: mid, half: od / 2.0 },
            layer: DimLayer::Diameters,
            value: od,
            label: format!("OD {}", mm(od)),
            at: (mid, od / 2.0),
        },
        Dimension {
            kind: DimKind::Diameter { at_axial: mid, half: id / 2.0 },
            layer: DimLayer::Diameters,
            value: id,
            label: format!("ID {}", mm(id)),
            at: (mid, id / 2.0),
        },
        Dimension {
            kind: DimKind::Note,
            layer: DimLayer::Diameters,
            value: wire,
            label: format!("wire \u{2300}{}", mm(wire)), // ⌀
            at: (mid, od / 2.0),
        },
        Dimension {
            kind: DimKind::Note,
            layer: DimLayer::Coils,
            value: na,
            label: format!(
                "N {} active / {} total",
                if na.is_finite() { format!("{na:.1}") } else { "\u{2014}".into() },
                if nt.is_finite() { format!("{nt:.1}") } else { "\u{2014}".into() },
            ),
            at: (mid, 0.0),
        },
    ]
}
```
Add `mod diagram_model;` to `springmaker/src/compression/mod.rs` (next to `mod scene_model;`).

- [ ] **Step 5: Run to verify pass**

Run: `cargo test -p springmaker compression::diagram_model`
Expected: PASS (5 tests).

- [ ] **Step 6: Lint + commit**

```bash
cargo fmt -p springmaker && cargo clippy -p springmaker --all-targets -- -D warnings
cargo clippy -p springmaker --all-targets --no-default-features -- -D warnings
cargo doc -p springmaker --no-deps -D warnings && typos
git add springmaker/src/diagram/mod.rs springmaker/src/compression/
git commit -m "feat(diagram): compression dimension presenter with mirror-drift pins"
```

---

## Task 3: Shared layout engine (`diagram/layout.rs`)

**Files:**
- Create: `springmaker/src/diagram/layout.rs`
- Modify: `springmaker/src/diagram/mod.rs` (`pub mod layout;`, re-exports)

**Interfaces:**
- Consumes: `diagram::{Bounds, Dimension, DimKind, DimLayer, P2}`.
- Produces:
  ```rust
  pub struct LayoutedDim {
      pub lines: Vec<(P2, P2)>,   // dimension + extension line segments, model mm
      pub arrows: Vec<(P2, f64)>, // arrowhead tip + direction (radians)
      pub arc: Option<(P2, f64, f64, f64)>, // (vertex, radius, start_deg, sweep_deg)
      pub text: (P2, String),     // text anchor (model mm) + content
  }
  pub fn layout(dims: &[Dimension], bounds: &Bounds, active: DimLayers) -> Vec<LayoutedDim>;
  ```
  `DimLayers` is the app toggle state (Task 5) — for this task define it here in `diagram/mod.rs` (Step 1) so layout can consume it.

- [ ] **Step 1: Define `DimLayers` in `diagram/mod.rs`**

```rust
/// Which dimension layers are currently shown (app state; toggled in the UI).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DimLayers {
    pub lengths: bool,
    pub diameters: bool,
    pub coils: bool,
}

impl Default for DimLayers {
    fn default() -> Self {
        Self { lengths: true, diameters: true, coils: true }
    }
}

impl DimLayers {
    /// Whether a dimension's layer is currently visible.
    pub fn shows(&self, layer: DimLayer) -> bool {
        match layer {
            DimLayer::Lengths => self.lengths,
            DimLayer::Diameters => self.diameters,
            DimLayer::Coils => self.coils,
        }
    }
}
```

- [ ] **Step 2: Write the failing layout tests**

`springmaker/src/diagram/layout.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram::{Bounds, DimKind, DimLayer, DimLayers, Dimension};
    use approx::assert_relative_eq;

    fn bounds() -> Bounds {
        Bounds { axial_min: 0.0, axial_max: 60.0, radial_min: -11.0, radial_max: 11.0 }
    }

    fn linear(label: &str, layer: DimLayer, to: f64) -> Dimension {
        Dimension {
            kind: DimKind::Linear { from: (0.0, 0.0), to: (to, 0.0) },
            layer, value: to, label: label.into(), at: (to / 2.0, 0.0),
        }
    }

    #[test]
    fn hidden_layers_produce_no_layouted_dims() {
        let dims = vec![linear("L\u{2080}", DimLayer::Lengths, 60.0)];
        let out = layout(&dims, &bounds(), DimLayers { lengths: false, diameters: true, coils: true });
        assert!(out.is_empty());
    }

    #[test]
    fn linear_length_dims_stack_in_ladders_clear_of_the_envelope() {
        let dims = vec![
            linear("L\u{2080}", DimLayer::Lengths, 60.0),
            linear("L\u{209B}", DimLayer::Lengths, 26.0),
        ];
        let out = layout(&dims, &bounds(), DimLayers::default());
        assert_eq!(out.len(), 2);
        // Length ladders sit below the envelope (radial < radial_min) and at
        // increasing offsets so they never overlap.
        let rungs: Vec<f64> = out.iter().map(|d| d.text.0 .1).collect();
        assert!(rungs.iter().all(|&r| r < bounds().radial_min));
        assert!((rungs[0] - rungs[1]).abs() > 1e-6, "ladder rungs must differ");
    }

    #[test]
    fn diameter_dims_span_the_full_envelope_and_place_text_to_the_side() {
        let dims = vec![Dimension {
            kind: DimKind::Diameter { at_axial: 30.0, half: 11.0 },
            layer: DimLayer::Diameters, value: 22.0, label: "OD 22.0".into(), at: (30.0, 11.0),
        }];
        let out = layout(&dims, &bounds(), DimLayers::default());
        assert_eq!(out.len(), 1);
        // The diameter line spans -half..+half in radial at its axial station.
        let spans_full = out[0].lines.iter().any(|(a, b)| {
            (a.1 - (-11.0)).abs() < 1e-6 && (b.1 - 11.0).abs() < 1e-6 && (a.0 - b.0).abs() < 1e-6
        });
        assert!(spans_full, "diameter line must span the full envelope");
    }

    #[test]
    fn angular_dims_carry_an_arc() {
        let dims = vec![Dimension {
            kind: DimKind::Angular { vertex: (0.0, 0.0), start_deg: 0.0, sweep_deg: 90.0, radius: 8.0 },
            layer: DimLayer::Coils, value: 90.0, label: "90\u{00b0}".into(), at: (0.0, 0.0),
        }];
        let out = layout(&dims, &bounds(), DimLayers::default());
        let (_, r, _, sweep) = out[0].arc.expect("angular dim carries an arc");
        assert_relative_eq!(r, 8.0, max_relative = 1e-9);
        assert_relative_eq!(sweep, 90.0, max_relative = 1e-9);
    }
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p springmaker diagram::layout`
Expected: FAIL — `layout` not found.

- [ ] **Step 4: Implement the layout engine**

```rust
//! Pure dimension layout (ADR 0008): given the model-mm `Bounds` of the drawn
//! geometry and the callouts, place dimension lines, extension lines,
//! arrowheads, and text anchors — all in model mm. Linear length dims stack in
//! ladders below the envelope; diameter dims span the envelope and sit to its
//! right; angular dims carry an arc. The humble canvas applies the only affine.

use crate::diagram::{Bounds, DimKind, DimLayers, Dimension, P2};

pub struct LayoutedDim {
    pub lines: Vec<(P2, P2)>,
    pub arrows: Vec<(P2, f64)>,
    pub arc: Option<(P2, f64, f64, f64)>,
    pub text: (P2, String),
}

/// Radial gap between successive ladder rungs (model mm, scaled by the view).
const RUNG_STEP: f64 = 6.0;
/// Gap from the envelope to the first ladder rung.
const LADDER_GAP: f64 = 8.0;

fn arrow_dir(from: P2, to: P2) -> f64 {
    (to.1 - from.1).atan2(to.0 - from.0)
}

pub fn layout(dims: &[Dimension], bounds: &Bounds, active: DimLayers) -> Vec<LayoutedDim> {
    let mut out = Vec::new();
    let mut length_rung = 0usize; // ladder index for axial length dims
    let mut diameter_rung = 0usize;

    for d in dims.iter().filter(|d| active.shows(d.layer)) {
        match d.kind {
            DimKind::Linear { from, to } => {
                // Drop the dimension line onto a ladder rung below the envelope.
                let r = bounds.radial_min - LADDER_GAP - RUNG_STEP * length_rung as f64;
                length_rung += 1;
                let a = (from.0, r);
                let b = (to.0, r);
                out.push(LayoutedDim {
                    lines: vec![
                        (a, b),               // dimension line
                        (from, a),            // extension line from geometry
                        (to, b),              // extension line from geometry
                    ],
                    arrows: vec![(a, arrow_dir(b, a)), (b, arrow_dir(a, b))],
                    arc: None,
                    text: (((a.0 + b.0) / 2.0, r), d.label.clone()),
                });
            }
            DimKind::Diameter { at_axial, half } => {
                // Full radial span at the station; text parked to the right.
                let a = (at_axial, -half);
                let b = (at_axial, half);
                let text_x = bounds.axial_max + LADDER_GAP + RUNG_STEP * diameter_rung as f64;
                diameter_rung += 1;
                out.push(LayoutedDim {
                    lines: vec![(a, b), (b, (text_x, half))],
                    arrows: vec![(a, arrow_dir(b, a)), (b, arrow_dir(a, b))],
                    arc: None,
                    text: ((text_x, half), d.label.clone()),
                });
            }
            DimKind::Angular { vertex, start_deg, sweep_deg, radius } => {
                out.push(LayoutedDim {
                    lines: Vec::new(),
                    arrows: Vec::new(),
                    arc: Some((vertex, radius, start_deg, sweep_deg)),
                    text: (
                        {
                            let mid = (start_deg + sweep_deg / 2.0).to_radians();
                            (vertex.0 + radius * mid.cos(), vertex.1 + radius * mid.sin())
                        },
                        d.label.clone(),
                    ),
                });
            }
            DimKind::Note => {
                out.push(LayoutedDim {
                    lines: Vec::new(),
                    arrows: Vec::new(),
                    arc: None,
                    text: (d.at, d.label.clone()),
                });
            }
        }
    }
    out
}
```
Wire `pub mod layout;` and re-exports (`pub use layout::{layout, LayoutedDim};`) into `diagram/mod.rs`.

- [ ] **Step 5: Run to verify pass**

Run: `cargo test -p springmaker diagram::layout`
Expected: PASS (4 tests).

- [ ] **Step 6: Lint + commit**

```bash
cargo fmt -p springmaker && cargo clippy -p springmaker --all-targets -- -D warnings
cargo clippy -p springmaker --all-targets --no-default-features -- -D warnings
cargo doc -p springmaker --no-deps -D warnings && typos
git add springmaker/src/diagram/
git commit -m "feat(diagram): shared dimension layout engine (linear + diameter + angular)"
```

---

## Task 4: Humble canvas + `diagram_element` entry

**Files:**
- Create: `springmaker/src/diagram/canvas.rs`
- Modify: `springmaker/src/diagram/mod.rs` (`DiagramView`, `DiagramInput`, `Inset`, `diagram_element`, view step helpers, re-exports)
- Modify: `springmaker/src/app.rs` (`Message::{DiagramZoom,DiagramPan,DiagramLayer}`, `App::{diagram_view,diagram_layers}` + defaults + no-recompute update arms — the canvas publishes these messages, so they must exist here for the crate to compile; `VisualMode::Diagram` and the visual wiring stay in Task 5)
- Modify: `springmaker/src/ui_tests.rs` (update-arm tests)

**Interfaces:**
- Consumes: `diagram::{project_silhouette, layout, Projected, LayoutedDim, DimLayers, DimLayer, zoom_step, pan_step}`, `viz::canvas3d::{placeholder_for}` idiom via `widgets::placeholder_text`, `app::{Message, Palette}`.
- Produces: `Message::{DiagramZoom(f32), DiagramPan(f32,f32), DiagramLayer(DimLayer)}`; `App::{diagram_view: DiagramView, diagram_layers: DimLayers}`.
- Produces:
  ```rust
  pub struct DiagramView { pub zoom: f32, pub pan: iced::Vector }
  pub struct Inset { pub edges: Vec<Edge2>, pub dims: Vec<Dimension> } // torsion end-view; Task 9
  pub struct DiagramInput { pub scene: viz::SceneData, pub dims: Vec<Dimension>, pub inset: Option<Inset> }
  impl DiagramInput { pub fn new(scene, dims) -> Self; pub fn with_inset(self, Inset) -> Self; }
  pub fn diagram_element(pal, input: DiagramInput, view: DiagramView, layers: DimLayers) -> Element<'static, Message>;
  pub fn zoom_step(view: DiagramView, delta: f32) -> DiagramView;
  pub fn pan_step(view: DiagramView, dx: f32, dy: f32) -> DiagramView;
  ```

- [ ] **Step 1: Add `DiagramView`, `DiagramInput`, step helpers to `diagram/mod.rs`**

```rust
use crate::viz::SceneData;

/// View transform for the diagram (app state). `zoom` multiplies the
/// fit-to-canvas baseline; `pan` translates in screen px. Default = fit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiagramView {
    pub zoom: f32,
    pub pan: iced::Vector,
}

impl Default for DiagramView {
    fn default() -> Self {
        Self { zoom: 1.0, pan: iced::Vector::ZERO }
    }
}

/// Zoom clamp bounds (mirrors the 3D `viz::zoom_step` discipline: a single
/// writer, finiteness-guarded, clamped).
const ZOOM_MIN: f32 = 0.2;
const ZOOM_MAX: f32 = 8.0;

/// Single writer for `DiagramView::zoom`. A non-finite delta is a no-op; the
/// result is clamped so no other code can push zoom out of range.
pub fn zoom_step(view: DiagramView, delta: f32) -> DiagramView {
    if !delta.is_finite() {
        return view;
    }
    let zoom = (view.zoom * (1.0 + delta * 0.1)).clamp(ZOOM_MIN, ZOOM_MAX);
    DiagramView { zoom, ..view }
}

/// Single writer for `DiagramView::pan`. Non-finite deltas are no-ops.
pub fn pan_step(view: DiagramView, dx: f32, dy: f32) -> DiagramView {
    if !dx.is_finite() || !dy.is_finite() {
        return view;
    }
    DiagramView { pan: view.pan + iced::Vector::new(dx, dy), ..view }
}

/// Optional secondary end-on projection (torsion legs; Task 9). Empty for other
/// families.
pub struct Inset {
    pub edges: Vec<Edge2>,
    pub dims: Vec<Dimension>,
}

/// Everything the diagram needs for one family, built lazily by the caller.
pub struct DiagramInput {
    pub scene: SceneData,
    pub dims: Vec<Dimension>,
    pub inset: Option<Inset>,
}

impl DiagramInput {
    pub fn new(scene: SceneData, dims: Vec<Dimension>) -> Self {
        Self { scene, dims, inset: None }
    }
    pub fn with_inset(mut self, inset: Inset) -> Self {
        self.inset = Some(inset);
        self
    }
}
```

- [ ] **Step 1b: Wire the diagram messages, state, and update arms in `app.rs`**

The canvas (Step 4) publishes `Message::DiagramZoom`/`DiagramPan`, so these variants and their update arms must exist for the crate to compile. Add the view-only messages, `App` state, defaults, and no-recompute arms now. (`VisualMode::Diagram`, the segmented `"2D"` toggle, the results dispatch, and the layer-toggle row are Task 5.) Write the two update-arm tests first (RED: the `Message::Diagram*` variants don't exist yet), then add the variants+state+arms (GREEN).

`app.rs` — `Message` (near `Zoom`/`Orbit`):
```rust
    /// 2D-diagram wheel-zoom delta (published by `DiagramCanvas::update`),
    /// accumulated by the `DiagramZoom` arm via `diagram::zoom_step`.
    DiagramZoom(f32),
    /// 2D-diagram drag-pan delta (dx, dy) in px, accumulated via `diagram::pan_step`.
    DiagramPan(f32, f32),
    /// Toggle one 2D-diagram dimension layer.
    DiagramLayer(crate::diagram::DimLayer),
```
`app.rs` — `App` fields (near `orbit`/`zoom`/`results_visual`):
```rust
    pub diagram_view: crate::diagram::DiagramView,
    pub diagram_layers: crate::diagram::DimLayers,
```
`app.rs` — defaults (near `orbit: ...`, `zoom: 1.0`):
```rust
            diagram_view: crate::diagram::DiagramView::default(),
            diagram_layers: crate::diagram::DimLayers::default(),
```
`app.rs` — update arms (next to the `Message::Zoom` arm; all return `false` — no recompute; mirror the `Orbit`/`Zoom` single-writer discipline):
```rust
            // Same non-recompute shape as `Zoom`/`Orbit`: `zoom_step`/`pan_step`
            // are the single writers (finiteness-guarded), so the view stays valid
            // by induction. Layer toggles are pure view state.
            Message::DiagramZoom(delta) => {
                self.diagram_view = crate::diagram::zoom_step(self.diagram_view, delta);
                false
            }
            Message::DiagramPan(dx, dy) => {
                self.diagram_view = crate::diagram::pan_step(self.diagram_view, dx, dy);
                false
            }
            Message::DiagramLayer(layer) => {
                let l = &mut self.diagram_layers;
                match layer {
                    crate::diagram::DimLayer::Lengths => l.lengths = !l.lengths,
                    crate::diagram::DimLayer::Diameters => l.diameters = !l.diameters,
                    crate::diagram::DimLayer::Coils => l.coils = !l.coils,
                }
                false
            }
```

Add these update-arm tests to `springmaker/src/ui_tests.rs` (use `test_app()`, the constructor at `ui_tests.rs:36`):
```rust
#[test]
fn diagram_zoom_and_pan_do_not_recompute_and_stay_finite() {
    let mut app = test_app();
    let before = app.diagram_view;
    app.update(Message::DiagramZoom(2.0));
    app.update(Message::DiagramPan(5.0, -3.0));
    assert!(app.diagram_view.zoom.is_finite() && app.diagram_view.zoom > 0.0);
    assert_ne!(app.diagram_view, before);
    // A non-finite delta is a no-op (single-writer guard).
    let held = app.diagram_view;
    app.update(Message::DiagramZoom(f32::NAN));
    assert_eq!(app.diagram_view, held);
}

#[test]
fn diagram_layer_toggle_flips_exactly_its_group() {
    let mut app = test_app();
    assert!(app.diagram_layers.coils);
    app.update(Message::DiagramLayer(crate::diagram::DimLayer::Coils));
    assert!(!app.diagram_layers.coils);
    assert!(app.diagram_layers.lengths && app.diagram_layers.diameters);
}
```

- [ ] **Step 2: Write the failing view-transform + degenerate tests**

Add to `springmaker/src/diagram/canvas.rs` (transform math is pure and testable without a GPU):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram::{zoom_step, DiagramView};

    #[test]
    fn zoom_step_clamps_and_ignores_non_finite() {
        let v = DiagramView::default();
        assert_eq!(zoom_step(v, f32::NAN).zoom, 1.0);
        // Repeated zoom-in saturates at the max, never past it.
        let mut z = v;
        for _ in 0..200 { z = zoom_step(z, 1.0); }
        assert!(z.zoom <= 8.0 + 1e-6);
        let mut zo = v;
        for _ in 0..200 { zo = zoom_step(zo, -1.0); }
        assert!(zo.zoom >= 0.2 - 1e-6);
    }

    #[test]
    fn fit_transform_centers_and_scales_bounds_into_the_canvas() {
        // A 60×22 model box fit into a 600×300 canvas (with margin) maps the
        // box center to the canvas center and keeps aspect ratio (uniform scale).
        let b = crate::diagram::Bounds { axial_min: 0.0, axial_max: 60.0, radial_min: -11.0, radial_max: 11.0 };
        let t = fit_transform(&b, 600.0, 300.0, DiagramView::default());
        let center_model = (30.0, 0.0);
        let (cx, cy) = t.apply(center_model);
        assert!((cx - 300.0).abs() < 1.0 && (cy - 150.0).abs() < 1.0);
        assert!(t.scale > 0.0);
    }
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p springmaker diagram::canvas`
Expected: FAIL — `fit_transform`/`Transform` not found.

- [ ] **Step 4: Implement the canvas + transform + `diagram_element`**

`springmaker/src/diagram/canvas.rs`:
```rust
//! Humble 2D-diagram canvas (ADR 0008): applies ONE affine (fit → zoom → pan)
//! to model-mm geometry and draws the wire silhouette, laid-out dimensions, and
//! text with native iced `Frame`/`Path`/`Text`. The sole screen-space exception
//! is dimension-text size, held constant px per CAD convention. Scroll publishes
//! `DiagramZoom`; drag publishes `DiagramPan` — deltas, never absolute values
//! read back from `self` (the OrbitCanvas stale-base rule).

use crate::app::{Message, Palette};
use crate::diagram::{
    layout, project_silhouette, Bounds, DiagramView, DimLayers, LayoutedDim, Projected, P2,
};
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Event, Frame, Geometry, Path, Stroke, Text};
use iced::{Color, Element, Length, Point, Rectangle, Renderer, Theme, Vector};

/// A uniform-scale affine from model mm `(axial, radial)` to screen px.
pub struct Transform {
    pub scale: f32,
    pub offset: Vector, // screen-space translation (fit-center + pan)
}

impl Transform {
    pub fn apply(&self, p: P2) -> (f32, f32) {
        (
            p.0 as f32 * self.scale + self.offset.x,
            // radial grows downward on screen; keep axial left→right.
            -(p.1 as f32) * self.scale + self.offset.y,
        )
    }
    fn point(&self, p: P2) -> Point {
        let (x, y) = self.apply(p);
        Point::new(x, y)
    }
}

/// Fit `bounds` (plus a dimension-ladder margin) into `w × h`, centered, then
/// apply the view's zoom (about the center) and pan. Uniform scale preserves
/// true proportions.
pub fn fit_transform(bounds: &Bounds, w: f32, h: f32, view: DiagramView) -> Transform {
    const MARGIN: f32 = 40.0; // room for ladders/text around the envelope
    let span_a = (bounds.axial_max - bounds.axial_min).max(1e-6) as f32;
    let span_r = (bounds.radial_max - bounds.radial_min).max(1e-6) as f32;
    let sx = (w - 2.0 * MARGIN) / span_a;
    let sy = (h - 2.0 * MARGIN) / span_r;
    let scale = sx.min(sy).max(1e-6) * view.zoom;
    let cx = ((bounds.axial_min + bounds.axial_max) / 2.0) as f32;
    let cy = ((bounds.radial_min + bounds.radial_max) / 2.0) as f32;
    // Place the model center at the canvas center, then pan.
    let offset = Vector::new(w / 2.0 - cx * scale + view.pan.x, h / 2.0 + cy * scale + view.pan.y);
    Transform { scale, offset }
}

pub struct DiagramCanvas {
    projected: Projected,
    laid_out: Vec<LayoutedDim>,
    view: DiagramView,
    wire: Color,
    ink: Color,
}

#[derive(Default)]
pub struct DragState {
    last: Option<Point>,
}

impl canvas::Program<Message> for DiagramCanvas {
    type State = DragState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                cursor.position_in(bounds)?; // only when the cursor is over us
                let d = match delta {
                    mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. } => *y,
                };
                Some(canvas::Action::publish(Message::DiagramZoom(d)).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                state.last = cursor.position_in(bounds);
                None
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let pos = cursor.position_in(bounds)?;
                let last = state.last?;
                state.last = Some(pos);
                Some(canvas::Action::publish(Message::DiagramPan(pos.x - last.x, pos.y - last.y)))
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Mouse(mouse::Event::CursorLeft) => {
                state.last = None;
                None
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let t = fit_transform(&self.projected.bounds, bounds.width, bounds.height, self.view);

        // Wire silhouette edges.
        for edge in &self.projected.edges {
            if edge.points.len() < 2 {
                continue;
            }
            let path = Path::new(|b| {
                b.move_to(t.point(edge.points[0]));
                for &p in &edge.points[1..] {
                    b.line_to(t.point(p));
                }
            });
            frame.stroke(&path, Stroke::default().with_color(self.wire).with_width(1.5));
        }

        // Dimensions: lines, arrowheads, arcs, constant-px text.
        for d in &self.laid_out {
            for (a, b) in &d.lines {
                let seg = Path::line(t.point(*a), t.point(*b));
                frame.stroke(&seg, Stroke::default().with_color(self.ink).with_width(1.0));
            }
            if let Some((vertex, radius, start_deg, sweep_deg)) = d.arc {
                let arc = Path::new(|bld| {
                    let steps = 24;
                    for i in 0..=steps {
                        let a = (start_deg + sweep_deg * i as f64 / steps as f64).to_radians();
                        let p = (vertex.0 + radius * a.cos(), vertex.1 + radius * a.sin());
                        if i == 0 { bld.move_to(t.point(p)); } else { bld.line_to(t.point(p)); }
                    }
                });
                frame.stroke(&arc, Stroke::default().with_color(self.ink).with_width(1.0));
            }
            let (anchor, label) = &d.text;
            let (tx, ty) = t.apply(*anchor);
            frame.fill_text(Text {
                content: label.clone(),
                position: Point::new(tx, ty),
                color: self.ink,
                size: 12.0.into(), // constant px — the CAD text-size exception
                ..Text::default()
            });
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(&self, state: &Self::State, bounds: Rectangle, cursor: mouse::Cursor) -> mouse::Interaction {
        if state.last.is_some() {
            mouse::Interaction::Grabbing
        } else if cursor.position_in(bounds).is_some() {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::default()
        }
    }
}

/// Build the diagram element, or the shared placeholder for a degenerate scene.
pub fn diagram_element(
    pal: &'static Palette,
    input: crate::diagram::DiagramInput,
    view: DiagramView,
    layers: DimLayers,
) -> Element<'static, Message> {
    match project_silhouette(&input.scene) {
        None => crate::widgets::placeholder_text(pal, crate::viz::canvas3d::placeholder_for(&input.scene)),
        Some(projected) => {
            let laid_out = layout(&input.dims, &projected.bounds, layers);
            // `inset` handled in Task 9; ignored here.
            Canvas::new(DiagramCanvas {
                projected,
                laid_out,
                view,
                wire: pal.ink,   // primary stroke token (Palette in app.rs)
                ink: pal.muted,  // muted dimension-line + text token
            })
            .width(Length::Fill)
            .height(Length::Fixed(crate::plot::CHART_H as f32))
            .into()
        }
    }
}
```
Wire `pub mod canvas;` + `pub use canvas::{diagram_element, DiagramCanvas};` into `diagram/mod.rs`.

**Note for the implementer (verified during planning):** the `Palette` struct (`app.rs:22`) has fields `ink, panel, raised, line, text, muted, accent, accent_tint, hover, warn, danger, success` — use `pal.ink` for the wire stroke and `pal.muted` for dimension lines + text. The iced 0.14 canvas API is confirmed: `canvas::Action::publish(msg).and_capture()` (`iced_widget::action.rs:67`, used by `viz/shader3d.rs`), `WheelScrolled { delta }` with `ScrollDelta::{Lines,Pixels} { y, .. }` (`viz/shader3d.rs:114`), and `Text` fills via `frame.fill_text` (see `plot/canvas.rs`). If you want pixel-wheel normalization matching the 3D path, reuse `viz`'s `WHEEL_PIXELS_PER_LINE` divisor; otherwise the raw `y` into `zoom_step` is acceptable.

- [ ] **Step 5: Run to verify pass**

Run: `cargo test -p springmaker diagram::canvas`
Expected: PASS (2 tests). Then `cargo build -p springmaker` to confirm the canvas compiles against iced 0.14.

- [ ] **Step 6: Lint + commit**

```bash
cargo fmt -p springmaker && cargo clippy -p springmaker --all-targets -- -D warnings
cargo clippy -p springmaker --all-targets --no-default-features -- -D warnings
cargo doc -p springmaker --no-deps -D warnings && typos
git add springmaker/src/diagram/
git commit -m "feat(diagram): humble canvas with fit/zoom/pan transform + degenerate placeholder"
```

---

## Task 5: App wiring — `VisualMode::Diagram` end-to-end (compression)

**Files:**
- Modify: `springmaker/src/app.rs` (add ONLY the `VisualMode::Diagram` variant — `Message::Diagram*`, `App::diagram_view`/`diagram_layers`, and the update arms already landed in Task 4)
- Modify: `springmaker/src/widgets.rs` (`visual_toggle` 3rd segment, `results_visual_element` 4th closure + arm, `diagram_layer_toggle` row)
- Modify: `springmaker/src/diagram/canvas.rs` (draw arrowheads — deferred D1; center dimension text — deferred D2)
- Modify: `springmaker/src/compression/view.rs` (pass the `diagram` closure + layer row)
- Modify: the other four families' `view.rs` (temporary empty-dims `diagram` closure so the workspace compiles)
- Modify: `springmaker/src/ui_tests.rs` (the `VisualMode::Diagram` round-trip test)

**Interfaces:**
- Consumes: `diagram::{diagram_element, DiagramInput}` (Task 4), `compression::{scene_model::compression_scene, diagram_model::dimensions}`. `Message::Diagram*`, `App::diagram_*`, `zoom_step`/`pan_step` already exist from Task 4.
- Produces: `VisualMode::Diagram`; the segmented `"2D"` option; the `results_visual_element` 4th closure; `diagram_layer_toggle`.

- [ ] **Step 1: Write the failing integration test**

The `Message::Diagram*` / `App::diagram_*` update-arm tests already landed in Task 4. This task adds only the `VisualMode::Diagram` round-trip. Add to `springmaker/src/ui_tests.rs`:
```rust
#[test]
fn visual_toggle_round_trips_through_diagram_mode() {
    let mut app = test_app(); // the existing test constructor in ui_tests.rs (line 36)
    app.update(Message::Visual(VisualMode::Diagram));
    assert_eq!(app.results_visual, VisualMode::Diagram);
    app.update(Message::Visual(VisualMode::Chart));
    assert_eq!(app.results_visual, VisualMode::Chart);
}
```
(`test_app()` is the confirmed constructor at `ui_tests.rs:36`, used by the sibling `VisualMode::Spring3d` tests.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p springmaker ui_tests::visual_toggle_round_trips_through_diagram_mode`
Expected: FAIL — `VisualMode::Diagram` not found.

- [ ] **Step 3: Add the `VisualMode::Diagram` variant**

`app.rs` — enum (the `Message` variants, `App` fields, defaults, and update arms already exist from Task 4; add ONLY the variant):
```rust
pub enum VisualMode {
    #[default]
    Chart,
    Spring3d,
    Diagram,
}
```

- [ ] **Step 4: Extend `visual_toggle` and `results_visual_element`; add the layer row**

`widgets.rs` — `visual_toggle`:
```rust
    segmented(
        pal,
        &[
            ("Chart", VisualMode::Chart),
            ("3D", VisualMode::Spring3d),
            ("2D", VisualMode::Diagram),
        ],
        selected,
        Message::Visual,
    )
```
`widgets.rs` — `results_visual_element` gains a fourth closure and arm:
```rust
pub(crate) fn results_visual_element<'a>(
    pal: &'static Palette,
    app: &App,
    chart: impl FnOnce() -> Element<'a, Message>,
    wire3d: impl FnOnce() -> crate::viz::SceneData,
    sdf3d: impl FnOnce() -> crate::viz::sdf::SdfScene,
    diagram: impl FnOnce() -> crate::diagram::DiagramInput,
) -> Element<'a, Message> {
    match app.results_visual {
        VisualMode::Chart => chart(),
        VisualMode::Spring3d => { /* unchanged */ }
        VisualMode::Diagram => crate::diagram::diagram_element(
            pal, diagram(), app.diagram_view, app.diagram_layers,
        ),
    }
}
```
`widgets.rs` — a layer-toggle row (shown by each family's view only in Diagram mode):
```rust
/// The 2D-diagram layer toggles (lengths / diameters / coils). Rendered above
/// the canvas in Diagram mode only. Each button flips exactly its group.
pub(crate) fn diagram_layer_toggle(pal: &'static Palette, layers: crate::diagram::DimLayers) -> Element<'static, Message> {
    use crate::diagram::DimLayer;
    row![
        toggle_chip(pal, "Lengths", layers.lengths, Message::DiagramLayer(DimLayer::Lengths)),
        toggle_chip(pal, "Diameters", layers.diameters, Message::DiagramLayer(DimLayer::Diameters)),
        toggle_chip(pal, "Coils", layers.coils, Message::DiagramLayer(DimLayer::Coils)),
    ]
    .spacing(SP_XS)
    .into()
}
```
**Implementer note (verified):** there is no existing `toggle_chip` helper. `segmented` (`widgets.rs:547`) is single-select, so it doesn't fit three independent on/off toggles. Build `diagram_layer_toggle` as a `row![]` of three `button`s (imports `button`, `row`, `SP_XS` already in `widgets.rs`), each styled via the existing `segmented_style` (`widgets.rs:506`) with the pressed/selected style when its layer is on and the unselected style when off — so the toggles visually match the segmented controls in both palettes. Each button's `on_press` is `Message::DiagramLayer(<layer>)`.

- [ ] **Step 5: Wire the compression view**

`springmaker/src/compression/view.rs` — extend the `results_visual_element` call with the `diagram` closure, and render the layer row when in Diagram mode:
```rust
let visual = crate::widgets::results_visual_element(
    pal,
    app,
    || /* existing chart closure */,
    || crate::compression::scene_model::compression_scene(&outcome.design),
    || /* existing sdf3d closure */,
    || crate::diagram::DiagramInput::new(
        crate::compression::scene_model::compression_scene(&outcome.design),
        crate::compression::diagram_model::dimensions(&outcome.design),
    ),
);
```
Above/around the `visual` in the panel column, add the layer row conditionally:
```rust
let controls = if app.results_visual == crate::app::VisualMode::Diagram {
    Some(crate::widgets::diagram_layer_toggle(pal, app.diagram_layers))
} else {
    None
};
// push `visual_toggle`, then `controls` (if Some), then `visual` into the panel column.
```
Match the exact column-assembly idiom already in `compression/view.rs` (it uses `column![...]` with the `visual_toggle` and `visual`); insert the optional `controls` between them following that file's spacing/`push` pattern.

- [ ] **Step 6: Run tests + fix the other families' compile**

Run: `cargo test -p springmaker ui_tests` then `cargo build -p springmaker`. The other four families' `results_visual_element` calls now fail to compile (missing 4th arg) — fix each minimally by passing a `diagram` closure that builds from their `scene_model` + a **temporary** empty `Vec::new()` dims, so the workspace compiles; real per-family dims land in Tasks 6/8/9/10. Example for each family `view.rs`:
```rust
|| crate::diagram::DiagramInput::new(
    crate::<family>::scene_model::<family>_scene(&outcome.<design_expr>),
    Vec::new(), // dims added in the family's diagram task
),
```
(For assembly, the scene builder takes the outcome directly — match its existing `assembly_scene(outcome)` call shape.)

- [ ] **Step 7: Deferred polish — draw arrowheads (D1) and center dimension text (D2)**

Now that the diagram is visible, consume the layout's `arrows` output (Task-4 review deferral D1) and fix the label anchor (D2). In `diagram/canvas.rs` `draw()`:

Draw an arrowhead at each `LayoutedDim.arrows` entry — a constant-px V at the transformed anchor, pointing along the stored model-space direction mapped to screen (the affine flips y, so negate the direction's sin component). Add near the dimension-line drawing loop:
```rust
use std::f64::consts::PI;
const ARROW_LEN: f32 = 7.0;   // screen px, constant regardless of zoom
const ARROW_HALF: f64 = 0.42; // ~24° half-angle
for (anchor, dir) in &d.arrows {
    let tip = t.point(*anchor);
    // Model→screen direction: uniform positive scale keeps the angle, the
    // y-flip negates the sin component.
    let screen_dir = (-dir.sin()).atan2(dir.cos());
    for barb in [screen_dir + PI - ARROW_HALF, screen_dir + PI + ARROW_HALF] {
        let end = iced::Point::new(
            tip.x + ARROW_LEN * barb.cos() as f32,
            tip.y + ARROW_LEN * barb.sin() as f32,
        );
        frame.stroke(&Path::line(tip, end), Stroke::default().with_color(self.dim).with_width(1.0));
    }
}
```
For D2, center the dimension text on its layout anchor instead of the default top-left. Set the `Text`'s horizontal + vertical alignment to centered — verify the exact iced 0.14 field/enum spelling against `plot/canvas.rs` (which draws chart labels via `fill_text`) and use whatever it uses (e.g. `align_x: iced::alignment::Horizontal::Center`, `align_y: iced::alignment::Vertical::Center`, or the `Text { horizontal_alignment, vertical_alignment, .. }` form for this iced version). Keep the constant `size: 12.0`.

No unit test (draw code; no machine snapshot). Verify visually in the Step 8 smoke run: arrowheads sit at the dimension-line ends pointing outward toward the extension lines, and labels are centered on their anchors.

- [ ] **Step 8: Manual smoke + commit**

Build and run the app (`cargo run -p springmaker`), switch a solved compression design to the **2D** segment, confirm the silhouette draws with the crossing double-strand look, dimension lines carry arrowheads and centered labels, layer toggles show/hide callout groups, and scroll/drag zoom/pan. Then:
```bash
cargo fmt -p springmaker && cargo clippy -p springmaker --all-targets -- -D warnings
cargo clippy -p springmaker --all-targets --no-default-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -p springmaker --no-deps && typos
git add springmaker/src/app.rs springmaker/src/widgets.rs springmaker/src/diagram/canvas.rs springmaker/src/*/view.rs springmaker/src/ui_tests.rs
git commit -m "feat(diagram): wire VisualMode::Diagram end-to-end for compression"
```
(commit trailer: `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`)

---

## Task 6: Conical dimension presenter

**Files:**
- Create: `springmaker/src/conical/diagram_model.rs`
- Modify: `springmaker/src/conical/mod.rs` (`mod diagram_model;`), `springmaker/src/conical/view.rs` (real dims)

**Interfaces:**
- Consumes: `springcore::conical::ConicalDesign`, `diagram::{Dimension, DimKind, DimLayer}`.
- Produces: `conical::diagram_model::dimensions(&ConicalDesign) -> Vec<Dimension>`.

Conical geometry: radius `large_mean_dia/2` at axial 0 → `small_mean_dia/2` at axial `free_length`. So the **large** OD/ID diameter dims anchor at `at_axial = 0.0` and the **small** at `at_axial = free_length`.

- [ ] **Step 1: Write the failing tests**

`springmaker/src/conical/diagram_model.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::conical::form::ConFormState;
    use crate::conical::scene_model::conical_scene;
    use crate::diagram::{project_silhouette, DimKind, DimLayer};
    use approx::assert_relative_eq;
    use springcore::{CurvatureCorrection, MaterialSet, MaterialStore, UnitSystem};

    fn design() -> springcore::conical::ConicalDesign {
        let materials = MaterialStore::new(MaterialSet::load_default());
        let form = ConFormState {
            end_type: "squared_ground".into(), wire_dia: "2".into(),
            large_mean_dia: "20".into(), small_mean_dia: "12".into(),
            active: "10".into(), free_length: "60".into(), loads: "10, 25".into(),
        };
        crate::conical::form::parse_and_solve(&form, "Music Wire", UnitSystem::Metric, &materials, CurvatureCorrection::default())
            .unwrap().design
    }

    fn find(dims: &[Dimension], s: &str) -> Dimension {
        dims.iter().find(|d| d.label.contains(s)).cloned()
            .unwrap_or_else(|| panic!("no dim containing {s}"))
    }

    #[test]
    fn large_and_small_od_anchor_to_the_projected_ends() {
        let d = design(); // large mean 20, small 12, wire 2, free 60, active 10 → total 12 (integer)
        let dims = dimensions(&d);
        let large = find(&dims, "large OD");
        let small = find(&dims, "small OD");
        // Presenter ↔ design: EXACT.
        assert_relative_eq!(large.value, d.large_outer_dia.millimeters(), max_relative = 1e-9);
        assert_relative_eq!(small.value, d.small_outer_dia.millimeters(), max_relative = 1e-9);
        // Anchors: large OD at the large end (axial 0), small OD at the free length; halves == OD/2.
        let DimKind::Diameter { at_axial: la, half: lhalf } = large.kind else { panic!("large OD must be a Diameter") };
        let DimKind::Diameter { at_axial: sa, half: shalf } = small.kind else { panic!("small OD must be a Diameter") };
        assert_relative_eq!(la, 0.0, epsilon = 1e-9);
        assert_relative_eq!(sa, d.inputs.free_length.millimeters(), max_relative = 1e-9);
        assert_relative_eq!(2.0 * lhalf, d.large_outer_dia.millimeters(), max_relative = 1e-9);
        assert_relative_eq!(2.0 * shalf, d.small_outer_dia.millimeters(), max_relative = 1e-9);
        assert_eq!(large.layer, DimLayer::Diameters);
        assert_eq!(small.layer, DimLayer::Diameters);
        // Mirror-drift vs geometry (EXACT, drop-z-robust): the silhouette
        // edge-MIDPOINT equals the centerline radius (the ±wire/2 offset cancels),
        // independent of the discrete perpendicular. At the first sample (large
        // end, θ=0) it is the large mean/2; at the last sample (small end,
        // θ=total·2π with integer total → cos≈1) the small mean/2. This is the
        // exact tie; the envelope PEAK is only sampling-approximate under drop-z.
        let p = project_silhouette(&conical_scene(&d)).unwrap();
        let last = p.edges[0].points.len() - 1;
        let mid = |i: usize| (p.edges[0].points[i].1 + p.edges[1].points[i].1) / 2.0;
        assert_relative_eq!(mid(0), d.inputs.large_mean_dia.millimeters() / 2.0, max_relative = 1e-9);
        assert_relative_eq!(mid(last), d.inputs.small_mean_dia.millimeters() / 2.0, max_relative = 1e-9);
    }

    #[test]
    fn free_length_and_coils_present() {
        let d = design();
        let dims = dimensions(&d);
        assert_relative_eq!(find(&dims, "L\u{2080}").value, d.inputs.free_length.millimeters(), max_relative = 1e-9);
        assert_eq!(find(&dims, "N").layer, DimLayer::Coils);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p springmaker conical::diagram_model`
Expected: FAIL — `dimensions` not found.

- [ ] **Step 3: Implement**

```rust
//! Pure 2D-diagram dimension presenter for the conical family. Large end at
//! axial 0, small end at axial `free_length` (see conical scene_model).

use crate::diagram::{DimKind, DimLayer, Dimension};
use springcore::conical::ConicalDesign;

fn mm(v: f64) -> String {
    if v.is_finite() { format!("{v:.1}") } else { "\u{2014}".into() }
}

pub fn dimensions(design: &ConicalDesign) -> Vec<Dimension> {
    let l0 = design.inputs.free_length.millimeters();
    let large_od = design.large_outer_dia.millimeters();
    let large_id = design.large_inner_dia.millimeters();
    let small_od = design.small_outer_dia.millimeters();
    let small_id = design.small_inner_dia.millimeters();
    let wire = design.inputs.wire_dia.millimeters();
    let na = design.inputs.active_coils;
    let nt = design.total_coils;

    vec![
        Dimension { kind: DimKind::Linear { from: (0.0, 0.0), to: (l0, 0.0) }, layer: DimLayer::Lengths, value: l0, label: format!("L\u{2080} {}", mm(l0)), at: (l0 / 2.0, 0.0) },
        Dimension { kind: DimKind::Diameter { at_axial: 0.0, half: large_od / 2.0 }, layer: DimLayer::Diameters, value: large_od, label: format!("large OD {}", mm(large_od)), at: (0.0, large_od / 2.0) },
        Dimension { kind: DimKind::Diameter { at_axial: 0.0, half: large_id / 2.0 }, layer: DimLayer::Diameters, value: large_id, label: format!("large ID {}", mm(large_id)), at: (0.0, large_id / 2.0) },
        Dimension { kind: DimKind::Diameter { at_axial: l0, half: small_od / 2.0 }, layer: DimLayer::Diameters, value: small_od, label: format!("small OD {}", mm(small_od)), at: (l0, small_od / 2.0) },
        Dimension { kind: DimKind::Diameter { at_axial: l0, half: small_id / 2.0 }, layer: DimLayer::Diameters, value: small_id, label: format!("small ID {}", mm(small_id)), at: (l0, small_id / 2.0) },
        Dimension { kind: DimKind::Note, layer: DimLayer::Diameters, value: wire, label: format!("wire \u{2300}{}", mm(wire)), at: (l0 / 2.0, large_od / 2.0) },
        Dimension { kind: DimKind::Note, layer: DimLayer::Coils, value: na, label: format!("N {} active / {} total", if na.is_finite() { format!("{na:.1}") } else { "\u{2014}".into() }, if nt.is_finite() { format!("{nt:.1}") } else { "\u{2014}".into() }), at: (l0 / 2.0, 0.0) },
    ]
}
```
Add `mod diagram_model;` to `conical/mod.rs`; replace the temporary `Vec::new()` dims in `conical/view.rs` with `crate::conical::diagram_model::dimensions(&outcome.design)`.

- [ ] **Step 4: Run to verify pass + lint + commit**

Run: `cargo test -p springmaker conical::diagram_model` (PASS), then the lint suite and:
```bash
git add springmaker/src/conical/
git commit -m "feat(diagram): conical dimension presenter (large/small OD anchoring)"
```

---

## Task 7: Extract shared helical-body dimension helpers

**Files:**
- Modify: `springmaker/src/diagram/mod.rs` (or a new `diagram/common.rs`) — add shared builders
- Modify: `springmaker/src/compression/diagram_model.rs`, `springmaker/src/conical/diagram_model.rs` — use them

Two concrete families now exist, so the DRY-on-second-occurrence extraction is warranted (the compression and conical presenters share the `Note` wire/coil callouts, the `mm` formatter, and the linear free-length pattern). This is a pure refactor — behavior-preserving.

- [ ] **Step 1: Confirm current tests are green**

Run: `cargo test -p springmaker diagram:: compression::diagram_model conical::diagram_model`
Expected: PASS (baseline before refactor).

- [ ] **Step 2: Extract the shared helpers**

Create `springmaker/src/diagram/common.rs`:
```rust
//! Shared dimension-presenter helpers used by the helical families.
use crate::diagram::{DimKind, DimLayer, Dimension, P2};

/// Format a millimetre value; em dash for non-finite (no NaN/inf label).
pub fn mm(v: f64) -> String {
    if v.is_finite() { format!("{v:.1}") } else { "\u{2014}".into() }
}

/// Coil-count note: "N {active} active / {total} total" in the Coils layer.
pub fn coil_note(active: f64, total: f64, at: P2) -> Dimension {
    let f = |x: f64| if x.is_finite() { format!("{x:.1}") } else { "\u{2014}".into() };
    Dimension { kind: DimKind::Note, layer: DimLayer::Coils, value: active,
        label: format!("N {} active / {} total", f(active), f(total)), at }
}

/// Wire-diameter note in the Diameters layer.
pub fn wire_note(wire: f64, at: P2) -> Dimension {
    Dimension { kind: DimKind::Note, layer: DimLayer::Diameters, value: wire,
        label: format!("wire \u{2300}{}", mm(wire)), at }
}

/// A free-length linear dimension along the axis, `[0, l0]`.
pub fn free_length(l0: f64) -> Dimension {
    Dimension { kind: DimKind::Linear { from: (0.0, 0.0), to: (l0, 0.0) },
        layer: DimLayer::Lengths, value: l0, label: format!("L\u{2080} {}", mm(l0)), at: (l0 / 2.0, 0.0) }
}
```
Wire `pub mod common;` into `diagram/mod.rs`. Replace the inlined `mm`/coil-note/wire-note/free-length constructions in the compression and conical presenters with these helpers (keeping the family-specific diameter dims inline).

- [ ] **Step 3: Run to verify unchanged behavior + lint + commit**

Run the same test set as Step 1 — still PASS (no assertion changed). Then lint and:
```bash
git add springmaker/src/diagram/ springmaker/src/compression/ springmaker/src/conical/
git commit -m "refactor(diagram): extract shared helical-body dimension helpers"
```

---

## Task 8: Extension dimension presenter (hooks)

**Files:**
- Create: `springmaker/src/extension/diagram_model.rs`
- Modify: `springmaker/src/extension/mod.rs`, `springmaker/src/extension/view.rs`

Extension geometry: body `[0, body_h]`, `body_h = free_length - 2*(2*r1 - wire) - wire`; the **inside-hooks span** equals `free_length`. The free-length dimension anchors from the bottom hook's inner surface to the top hook's inner surface — which the projection places at axial `-2*r1 + wire/2` (bottom inner) and `body_h + 2*r1 - wire/2` (top inner). Body length is a Lengths dim over `[0, body_h]`. Hook opening = loop inside diameter `2*r1 - wire` (Diameters). Initial tension is a Coils-layer `Note` (`initial_tension` Force). OD/ID/wire as usual.

- [ ] **Step 1: Write the failing tests**

`springmaker/src/extension/diagram_model.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension::form::{parse_and_solve, ExtFormState};
    use crate::extension::scene_model::extension_scene;
    use crate::diagram::project_silhouette;
    use approx::assert_relative_eq;
    use springcore::{CurvatureCorrection, MaterialSet, MaterialStore, UnitSystem};

    fn design() -> springcore::extension::ExtensionDesign {
        let materials = MaterialStore::new(MaterialSet::load_default());
        let form = ExtFormState {
            wire_dia: "2".into(), mean_dia: "20".into(), active: "10".into(),
            free_length: "100".into(), initial_tension: "5".into(), loads: "10, 30".into(),
            ..Default::default()
        };
        parse_and_solve(&form, "Music Wire", UnitSystem::Metric, &materials, CurvatureCorrection::default()).unwrap().design
    }

    fn find(dims: &[Dimension], s: &str) -> Dimension {
        dims.iter().find(|d| d.label.contains(s)).cloned().unwrap_or_else(|| panic!("no dim {s}"))
    }

    #[test]
    fn free_length_spans_inside_hooks_matching_the_projection() {
        let d = design();
        let dims = dimensions(&d);
        let fl = find(&dims, "L\u{2080}");
        assert_relative_eq!(fl.value, d.free_length.millimeters(), max_relative = 1e-9);
        // Mirror-drift: the projected inside-hooks axial span equals free_length.
        let p = project_silhouette(&extension_scene(&d)).unwrap();
        assert_relative_eq!(p.bounds.axial_max - p.bounds.axial_min,
            d.free_length.millimeters() + 2.0 * d.wire_dia.millimeters(), // outer surfaces
            max_relative = 1e-9);
        if let crate::diagram::DimKind::Linear { from, to } = fl.kind {
            assert_relative_eq!((to.0 - from.0).abs(), d.free_length.millimeters(), max_relative = 1e-9);
        } else { panic!("free length must be Linear"); }
    }

    #[test]
    fn hook_opening_and_initial_tension_present() {
        let d = design();
        let dims = dimensions(&d);
        let opening = find(&dims, "hook");
        assert_relative_eq!(opening.value, 2.0 * d.hooks.r1.millimeters() - d.wire_dia.millimeters(), max_relative = 1e-9);
        let fi = find(&dims, "F\u{1d62}"); // Fᵢ initial tension
        assert_relative_eq!(fi.value, d.initial_tension.newtons(), max_relative = 1e-9);
        assert_eq!(fi.layer, crate::diagram::DimLayer::Coils);
    }
}
```
**Note:** the outer-surface span pin (`free_length + 2*wire`) documents the geometry; the *dimension* itself measures the inside-hooks `free_length`. Anchor the free-length `Linear` from `(bottom_inner_axial, 0)` to `(top_inner_axial, 0)` where those come from the design relation, and let the equality test confirm the `|to-from|` equals `free_length`.

- [ ] **Step 2–4: Run-fail, implement, run-pass**

Implement `dimensions(&ExtensionDesign)` using `crate::diagram::common::{mm, wire_note, coil_note}` where applicable, plus:
```rust
use crate::diagram::{common, DimKind, DimLayer, Dimension};
use springcore::extension::ExtensionDesign;

pub fn dimensions(design: &ExtensionDesign) -> Vec<Dimension> {
    let wire = design.wire_dia.millimeters();
    let r1 = design.hooks.r1.millimeters();
    let l0 = design.free_length.millimeters();
    let od = design.outer_dia.millimeters();
    let id = design.inner_dia.millimeters();
    let na = design.active_coils;
    // Body height from the engine's inside-hooks relation (matches scene_model).
    let body_h = l0 - 2.0 * (2.0 * r1 - wire) - wire;
    let bottom_inner = -2.0 * r1 + wire / 2.0;
    let top_inner = body_h + 2.0 * r1 - wire / 2.0;
    let fi = design.initial_tension.newtons();

    vec![
        // Free length = inside-hooks span (feature-anchored).
        Dimension { kind: DimKind::Linear { from: (bottom_inner, 0.0), to: (top_inner, 0.0) }, layer: DimLayer::Lengths, value: l0, label: format!("L\u{2080} {}", common::mm(l0)), at: ((bottom_inner + top_inner) / 2.0, 0.0) },
        // Body length.
        Dimension { kind: DimKind::Linear { from: (0.0, 0.0), to: (body_h, 0.0) }, layer: DimLayer::Lengths, value: body_h, label: format!("body {}", common::mm(body_h)), at: (body_h / 2.0, 0.0) },
        // Hook opening = loop inside diameter.
        Dimension { kind: DimKind::Diameter { at_axial: bottom_inner, half: (2.0 * r1 - wire) / 2.0 }, layer: DimLayer::Diameters, value: 2.0 * r1 - wire, label: format!("hook \u{2300}{}", common::mm(2.0 * r1 - wire)), at: (bottom_inner, r1) },
        Dimension { kind: DimKind::Diameter { at_axial: body_h / 2.0, half: od / 2.0 }, layer: DimLayer::Diameters, value: od, label: format!("OD {}", common::mm(od)), at: (body_h / 2.0, od / 2.0) },
        Dimension { kind: DimKind::Diameter { at_axial: body_h / 2.0, half: id / 2.0 }, layer: DimLayer::Diameters, value: id, label: format!("ID {}", common::mm(id)), at: (body_h / 2.0, id / 2.0) },
        common::wire_note(wire, (body_h / 2.0, od / 2.0)),
        common::coil_note(na, na, (body_h / 2.0, 0.0)), // extension body: active ≈ total
        Dimension { kind: DimKind::Note, layer: DimLayer::Coils, value: fi, label: format!("F\u{1d62} {}N", if fi.is_finite() { format!("{fi:.1}") } else { "\u{2014}".into() }), at: (body_h / 2.0, 0.0) },
    ]
}
```
Add `mod diagram_model;` to `extension/mod.rs`; replace the temporary dims in `extension/view.rs`.

- [ ] **Step 5: Run-pass, lint, commit** — `cargo test -p springmaker extension::diagram_model`, then lint suite, then `git commit -m "feat(diagram): extension dimension presenter (inside-hooks, hook opening, Fᵢ)"`.

---

## Task 9: Torsion dimension presenter + end-on leg inset (angular)

**Files:**
- Create: `springmaker/src/torsion/diagram_model.rs`
- Modify: `springmaker/src/torsion/mod.rs`, `springmaker/src/torsion/view.rs`
- Modify: `springmaker/src/diagram/canvas.rs` (draw the `Inset` in a corner box)

Torsion legs are in the cross-section plane, so the **side elevation** carries the body dims (body length `[0, body_coils*wire]`, OD, ID, wire, coil count) and a **torsion-only end-on inset** carries the legs at true length + the **angular** included leg angle. The inset projects `(x, z)` (drop axial `y`): the coil is a circle radius `mean_dia/2`, legs radiate at azimuths `0` and `body_coils*TAU`. The included leg angle is the azimuth difference `(body_coils.fract())*360°` (from the drawn leg directions — geometry-anchored). This is the one place a side elevation alone is insufficient; the inset is the conventional torsion end view.

- [ ] **Step 1: Write the failing tests**

`springmaker/src/torsion/diagram_model.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::torsion::form::{parse_and_solve, TorFormState};
    use crate::diagram::{DimKind, DimLayer};
    use approx::assert_relative_eq;
    use springcore::{MaterialSet, MaterialStore, UnitSystem};

    fn design() -> springcore::torsion::TorsionDesign {
        let materials = MaterialStore::new(MaterialSet::load_default());
        let form = TorFormState {
            wire_dia: "2".into(), mean_dia: "20".into(), body_coils: "5.25".into(),
            leg1: "15".into(), leg2: "10".into(), moments: "500, 1000".into(),
            ..Default::default()
        };
        parse_and_solve(&form, "Music Wire", UnitSystem::Metric, &materials).unwrap().design
    }

    #[test]
    fn body_dims_present_in_side_elevation() {
        let d = design();
        let (dims, _inset) = diagram(&d);
        let body = dims.iter().find(|x| x.label.contains("body")).unwrap();
        assert_relative_eq!(body.value, d.inputs.body_coils.millimeters_body(), max_relative = 1e-9);
    }

    #[test]
    fn inset_carries_leg_lengths_and_the_angular_leg_angle() {
        let d = design(); // body_coils 5.25 → legs 0.25 turn = 90° apart
        let (_dims, inset) = diagram(&d);
        let leg_angle = inset.dims.iter().find(|x| matches!(x.kind, DimKind::Angular { .. })).unwrap();
        assert_relative_eq!(leg_angle.value, 90.0, max_relative = 1e-6);
        // Both leg lengths are represented (true length in the end-view plane).
        assert!(inset.dims.iter().any(|x| (x.value - 15.0).abs() < 1e-6));
        assert!(inset.dims.iter().any(|x| (x.value - 10.0).abs() < 1e-6));
        assert!(!inset.edges.is_empty());
    }
}
```
**Note:** `millimeters_body()` above is shorthand for the body-length expression `d.inputs.body_coils * d.inputs.wire_dia.millimeters()` — in the real test, compute it inline (there is no such method). `diagram(&TorsionDesign) -> (Vec<Dimension>, crate::diagram::Inset)` returns the side-elevation dims and the inset.

- [ ] **Step 2–4: implement**

```rust
//! Pure 2D-diagram presenter for torsion: body dims in the side elevation, plus
//! an end-on inset (project x,z) for the legs and their included angle — a side
//! elevation cannot show cross-section-plane legs.
use crate::diagram::{common, DimKind, DimLayer, Dimension, Edge2, Inset};
use springcore::torsion::TorsionDesign;
use std::f64::consts::TAU;

pub fn diagram(design: &TorsionDesign) -> (Vec<Dimension>, Inset) {
    let wire = design.inputs.wire_dia.millimeters();
    let r = design.inputs.mean_dia.millimeters() / 2.0;
    let nb = design.inputs.body_coils;
    let body_h = nb * wire;
    let od = design.inputs.mean_dia.millimeters() + wire;
    let id = design.inputs.mean_dia.millimeters() - wire;
    let l1 = design.inputs.leg1.millimeters();
    let l2 = design.inputs.leg2.millimeters();

    let side = vec![
        Dimension { kind: DimKind::Linear { from: (0.0, 0.0), to: (body_h, 0.0) }, layer: DimLayer::Lengths, value: body_h, label: format!("body {}", common::mm(body_h)), at: (body_h / 2.0, 0.0) },
        Dimension { kind: DimKind::Diameter { at_axial: body_h / 2.0, half: od / 2.0 }, layer: DimLayer::Diameters, value: od, label: format!("OD {}", common::mm(od)), at: (body_h / 2.0, od / 2.0) },
        Dimension { kind: DimKind::Diameter { at_axial: body_h / 2.0, half: id / 2.0 }, layer: DimLayer::Diameters, value: id, label: format!("ID {}", common::mm(id)), at: (body_h / 2.0, id / 2.0) },
        common::wire_note(wire, (body_h / 2.0, od / 2.0)),
        common::coil_note(nb, nb, (body_h / 2.0, 0.0)),
    ];

    // End-on inset: coil circle + two legs at azimuths 0 and nb*TAU (x,z plane).
    let end_angle = nb * TAU;
    let leg_dir = |az: f64| (az.cos(), az.sin());
    let (d1x, d1y) = leg_dir(0.0);
    let (d2x, d2y) = leg_dir(end_angle);
    // Legs start at the coil radius, extend outward by their length (true length).
    let leg1_edge = Edge2 { points: vec![(r * 1.0, r * 0.0), ((r + l1) * d1x, (r + l1) * d1y)], role: springcore::viz_role_detail() };
    let leg2_edge = Edge2 { points: vec![(r * d2x, r * d2y), ((r + l2) * d2x, (r + l2) * d2y)], role: springcore::viz_role_detail() };
    // Included leg angle from the drawn leg directions (fractional turn → degrees).
    let included = (end_angle.to_degrees()).rem_euclid(360.0);
    let inset_dims = vec![
        Dimension { kind: DimKind::Linear { from: (r * 1.0, 0.0), to: ((r + l1) * d1x, (r + l1) * d1y) }, layer: DimLayer::Lengths, value: l1, label: format!("L\u{2081} {}", common::mm(l1)), at: ((r + l1 / 2.0), 0.0) },
        Dimension { kind: DimKind::Linear { from: (r * d2x, r * d2y), to: ((r + l2) * d2x, (r + l2) * d2y) }, layer: DimLayer::Lengths, value: l2, label: format!("L\u{2082} {}", common::mm(l2)), at: ((r + l2 / 2.0) * d2x, (r + l2 / 2.0) * d2y) },
        Dimension { kind: DimKind::Angular { vertex: (0.0, 0.0), start_deg: 0.0, sweep_deg: included, radius: r + 4.0 }, layer: DimLayer::Coils, value: included, label: format!("{included:.0}\u{00b0}"), at: (0.0, 0.0) },
    ];
    (side, Inset { edges: vec![leg1_edge, leg2_edge], dims: inset_dims })
}
```
**Implementer note:** `springcore::viz_role_detail()` is a placeholder — use `crate::viz::SceneRole::Detail` directly (Edge2's `role` field is `viz::SceneRole`). Add `mod diagram_model;` to `torsion/mod.rs`. In `torsion/view.rs`, build the input: `let (dims, inset) = crate::torsion::diagram_model::diagram(&outcome.design); crate::diagram::DiagramInput::new(scene, dims).with_inset(inset)`.

- [ ] **Step 5: Draw the inset in the canvas**

In `diagram/canvas.rs`, when `DiagramInput.inset` is `Some`, render it in a bordered box in a corner (its own `fit_transform` over the inset edges' bounds). Add an inset test asserting the inset element is produced when present. Keep the main side elevation unchanged. Store the inset (projected + laid-out) on `DiagramCanvas` and draw it after the main frame content within the same `Frame`, translated/scaled into a corner sub-rectangle.

- [ ] **Step 6: Run-pass, lint, commit** — `cargo test -p springmaker torsion::diagram_model diagram::`, lint suite, `git commit -m "feat(diagram): torsion presenter with end-on leg inset + angular leg angle"`.

---

## Task 10: Assembly dimension presenter (per-member + overall)

**Files:**
- Create: `springmaker/src/assembly/diagram_model.rs`
- Modify: `springmaker/src/assembly/mod.rs`, `springmaker/src/assembly/view.rs`

Assembly composes member scenes: **Nested** = concentric from axial 0 (each member drawn at its own height); **Series** = axially stacked with `2*max_wire` gaps. So per-member OD dims are feature-anchored (each member's outer edge), while the **overall free length** is a reference dim (value from `design.free_length`; for series it does NOT equal the drawn span because of gaps — labeled as the assembly total, not anchored to the full drawn span).

- [ ] **Step 1: Write the failing tests**

`springmaker/src/assembly/diagram_model.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::form::{parse_and_solve, AsmFormState, AsmMemberForm};
    use crate::diagram::DimLayer;
    use approx::assert_relative_eq;
    use springcore::{CurvatureCorrection, MaterialSet, MaterialStore, UnitSystem};

    fn two_member(topology: &str) -> springcore::assembly::AssemblyDesign {
        let mut f = AsmFormState::with_default_material("Music Wire");
        f.topology = topology.into(); f.loads = "10, 25".into();
        f.members[0] = AsmMemberForm { wire_dia: "2".into(), mean_dia: "20".into(), active: "10".into(), free_length: "60".into(), ..AsmMemberForm::blank("Music Wire") };
        f.members.push(AsmMemberForm { wire_dia: "1.5".into(), mean_dia: "16".into(), active: "8".into(), free_length: "60".into(), ..AsmMemberForm::blank("Music Wire") });
        parse_and_solve(&f, UnitSystem::Metric, &MaterialStore::new(MaterialSet::load_default()), CurvatureCorrection::Bergstrasser).unwrap()
    }

    fn find(dims: &[Dimension], s: &str) -> Dimension {
        dims.iter().find(|d| d.label.contains(s)).cloned().unwrap_or_else(|| panic!("no dim {s}"))
    }

    #[test]
    fn per_member_od_and_overall_free_length_present() {
        let d = two_member("nested");
        let dims = dimensions(&d);
        // Each member's OD appears (envelope OD = member 0's 22, inner member 17.5).
        assert!(dims.iter().filter(|x| x.label.contains("OD")).count() >= 2);
        let overall = find(&dims, "L\u{2080}");
        assert_relative_eq!(overall.value, d.free_length.millimeters(), max_relative = 1e-9);
        assert_eq!(overall.layer, DimLayer::Lengths);
    }

    #[test]
    fn series_reports_stage_summary() {
        let d = two_member("series");
        let dims = dimensions(&d);
        let stages = find(&dims, "stage");
        assert_eq!(stages.layer, DimLayer::Coils);
    }
}
```

- [ ] **Step 2–4: implement**

```rust
//! Pure 2D-diagram presenter for assemblies: per-member OD/wire dims anchored to
//! each member body, plus overall free/solid reference dims and a stage summary.
//! Series drawn span includes schematic gaps, so overall free length is a
//! reference dim (value from design.free_length), not a full-span anchor.
use crate::diagram::{common, DimKind, DimLayer, Dimension};
use springcore::assembly::{AssemblyDesign, Topology};

pub fn dimensions(design: &AssemblyDesign) -> Vec<Dimension> {
    let l0 = design.free_length.millimeters();
    let ls = design.solid_length.millimeters();
    let mut dims = vec![
        // Overall free length (reference; series includes schematic gaps).
        Dimension { kind: DimKind::Linear { from: (0.0, 0.0), to: (l0, 0.0) }, layer: DimLayer::Lengths, value: l0, label: format!("L\u{2080} {}", common::mm(l0)), at: (l0 / 2.0, 0.0) },
        Dimension { kind: DimKind::Linear { from: (0.0, 0.0), to: (ls, 0.0) }, layer: DimLayer::Lengths, value: ls, label: format!("L\u{209B} {}", common::mm(ls)), at: (ls / 2.0, 0.0) },
    ];
    // Per-member OD/wire notes.
    let mut axial = 0.0;
    for (i, m) in design.members.iter().enumerate() {
        let od = m.design.outer_dia.millimeters();
        let wire = m.design.wire_dia.millimeters();
        let member_h = m.design.free_length.millimeters();
        let station = match design.topology {
            Topology::Nested => member_h / 2.0,
            Topology::Series => axial + member_h / 2.0,
        };
        dims.push(Dimension { kind: DimKind::Diameter { at_axial: station, half: od / 2.0 }, layer: DimLayer::Diameters, value: od, label: format!("m{} OD {}", i + 1, common::mm(od)), at: (station, od / 2.0) });
        dims.push(common::wire_note(wire, (station, od / 2.0)));
        if design.topology == Topology::Series {
            axial += member_h; // (gap is cosmetic; per-member stations approximate)
        }
    }
    // Stage summary.
    let topo = match design.topology { Topology::Nested => "nested", Topology::Series => "series" };
    dims.push(Dimension { kind: DimKind::Note, layer: DimLayer::Coils, value: design.members.len() as f64, label: format!("{} stage {}", design.members.len(), topo), at: (l0 / 2.0, 0.0) });
    dims
}
```
Add `mod diagram_model;` to `assembly/mod.rs`; replace the temporary dims in `assembly/view.rs` with `crate::assembly::diagram_model::dimensions(outcome)` (assembly's `view.rs` passes the outcome/design per its `assembly_scene(outcome)` idiom — match its existing call shape).

- [ ] **Step 5: Run-pass, lint, commit** — `cargo test -p springmaker assembly::diagram_model`, full `cargo test -p springmaker`, lint suite, `git commit -m "feat(diagram): assembly dimension presenter (per-member + overall reference)"`.

---

## Final review

After Task 10, dispatch the final whole-branch adversarial multi-reviewer panel (per `superpowers:requesting-code-review` and the house Code Review standard): general reviewer, architect, simplifier, plus an input-domain/robustness adversary and a stateful-UI/cross-family reviewer. Cycle to convergence (every reviewer APPROVED, no unresolved findings) before the push cycle. Verify: mirror-drift pins present for every feature dimension in all five families; degenerate scenes placeholder (never panic); both palettes; no machine-dependent snapshot; `wire_mm` addition inert for the 3D path; the torsion inset is the only cross-section-plane departure and is documented.

---

## Self-review notes (author)

- **Spec coverage:** projection (T1), silhouette double-strand (T1), comprehensive per-family callouts (T2/6/8/9/10), three toggle layers (T3/T5), zoom+pan no-recompute (T4/T5), mirror-drift guard (every family task), degenerate/placeholder (T1/T4), both palettes (T4/T5), on-screen-only/export-deferred (no export task — correct), "2D" label (T5), L₀/Lₛ only (T2, no loaded state) — all mapped.
- **Type consistency:** `P2`, `Edge2`, `Bounds`, `Projected`, `Dimension`, `DimKind`, `DimLayer`, `DimLayers`, `DiagramView`, `DiagramInput`, `Inset`, `LayoutedDim`, `project_silhouette`, `layout`, `diagram_element`, `zoom_step`, `pan_step` are defined once and consumed consistently.
- **Resolved during planning (were verification points):** `Palette` tokens (`ink` wire / `muted` dimensions, `app.rs:22`); iced 0.14 canvas `Action::publish(..).and_capture()` (`action.rs:67`), `WheelScrolled`/`ScrollDelta` (`shader3d.rs:114`); `test_app()` constructor (`ui_tests.rs:36`); `segmented_style` reuse for the toggle row (no `toggle_chip` exists).
- **Remaining "read the sibling and match it" bindings (not placeholders):** each family `view.rs`'s exact column-assembly + outcome-expression idiom (e.g. `outcome.design` vs `outcome` for assembly); the `springcore::viz_role_detail()`/`millimeters_body()` shorthands flagged inline (use `crate::viz::SceneRole::Detail` and the inline body-length product). The SDD implementer resolves these against real code, with the tests as the gate.
