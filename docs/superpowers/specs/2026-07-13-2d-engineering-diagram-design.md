# 2D Engineering Diagram View — Design

**Date:** 2026-07-13
**Status:** Approved (brainstorm 2026-07-13)
**Increment:** second of two post-demo-prep stretch features (shaded 3D → 2D
engineering diagram). Follow-on to the shipped shaded 3D (PR #69), the
wireframe 3D (PR #63), and the theming system (PR #65).

## Goal

A third `VisualMode::Diagram` renders a dimensioned 2D engineering drawing of
the solved spring: a true-scale side elevation of the actual solved geometry,
overlaid with dimension lines and callouts per classic spring-drawing
convention. It sits alongside the existing `Chart` and `Spring3d` modes in
every family's results panel, on screen only (file export deferred).

## Decisions (from the brainstorm)

1. **True-scale side elevation, reusing the solved geometry.** The drawing
   projects the *same* `viz::SceneData` the 3D path builds — no second
   geometry source. It is an orthographic side view of the actual solved
   spring, not a schematic symbol. This inherits every degenerate/cap guard
   `SceneData` already carries.
2. **Crossing double-strand wire silhouette.** The wire is drawn as its two
   silhouette edges (the projected centerline offset ±wire_radius
   perpendicular to its local 2D tangent), so near and far sides of each coil
   cross — the textbook interlocking-coil look. Wire thickness is therefore
   *real geometry* (edge-to-edge span = wire ⌀ in model mm), not a stroke
   width. This structurally avoids the clamped `stroke_px` (1–8 px, dimensionally
   dishonest) and makes the outer edge land at OD/2 and the inner at ID/2 for
   free.
3. **Horizontal axis.** The spring axis runs left→right (spring lies on its
   side), filling the landscape results canvas; free length is measured
   horizontally. The projection maps `(x, y, z) → (axial = horizontal,
   radial = vertical)`, dropping z.
4. **Comprehensive per-family callouts, in three toggleable layers.** Every
   standard dimension the family carries, grouped into **lengths**,
   **diameters**, and **coils** layers that the user can toggle on/off
   independently. Defaults: lengths + diameters on, coils on.
5. **Zoom + pan.** Scroll to zoom, drag to pan, reusing the 3D view's
   wheel-capture and no-recompute discipline (view-only; never re-solves).
6. **On screen only.** File export (DXF/SVG/PNG) is deferred to its own
   future increment. The presenter geometry is designed export-ready (all
   layout in model mm) so the later increment reuses it.
7. **Segment label `"2D"`** (pairs with the existing `"3D"` segment). v1 shows
   the free/solid geometry (L₀/Lₛ) only; the loaded-length state stays in the
   chart/results panel, not the drawing.

## Architecture (ADR 0008 humble-view / presenter split)

Two pure concerns (projection + dimensioning) feed a shared pure layout
engine; one humble view draws the result and handles input.

```
springmaker/src/diagram/
├── mod.rs        NEW public API: DiagramInput{scene, dims}, Dimension,
│                 DimLayer, DimKind, diagram_element(...) + degenerate placeholder
├── geometry.rs   NEW pure: project_silhouette(&SceneData, orientation) →
│                 Vec<Edge2>  (family-agnostic projection + wire-edge offset)
├── layout.rs     NEW pure: layout(dims, bounds) → LayoutedDims
│                 (linear ladders + angular arcs, all in model mm)
└── canvas.rs     NEW humble: DiagramCanvas (iced canvas::Program) — one
                  affine (fit→zoom→pan), native Frame/Path/Stroke/Text,
                  scroll→zoom + drag→pan events

springmaker/src/<family>/diagram_model.rs
                  NEW per-family pure dimension builder (sibling to the
                  existing scene_model.rs), one per family
```

This mirrors the established split: shared visualization core (`viz/`, `plot/`)
plus per-family presenters (`scene_model.rs`, `plot_model.rs`). `diagram/`
depends on `viz::SceneData`; the dependency direction stays clean (diagram →
viz, never the reverse).

### The four units

1. **Projection (`diagram/geometry.rs`, family-agnostic, pure).**
   `project_silhouette` takes the `SceneData` the 3D path already builds,
   projects each `Polyline3` `(x, y, z) → (axial, radial)` dropping z, and
   offsets it by ±wire_radius perpendicular to its local 2D tangent to yield
   the two silhouette edges. Wire thickness is the geometric edge gap in model
   mm. Inherits `SceneData`'s degenerate/cap guards; a degenerate scene
   projects to the empty set and the caller shows the placeholder.

2. **Dimensioning (`<family>/diagram_model.rs`, per-family, pure).** Each
   family produces `Vec<Dimension>` from its solved design. **Every dimension
   that corresponds to a drawn feature is anchored to the projected geometry's
   computed bounds and labeled with the design-field value** — the two MUST
   agree (see §Mirror-drift guard). All anchor, extension-line, arrowhead, and
   text-position math is computed here in **model mm** and unit-tested. The
   view derives no layout from frame bounds.

3. **Shared layout engine (`diagram/layout.rs`, pure).** `layout` stacks
   linear dimensions into offset ladders outside the geometry envelope, one
   ladder per active layer, and carries — **from day one** — an angular
   primitive (arc + angle text) for torsion's leg angle (and any helix/pitch
   angle). Linear and angular are both first-class so torsion needs no
   retrofit. Layout consumes only the toggled-on layers.

4. **Humble view (`diagram/canvas.rs`).** `DiagramCanvas` implements iced's
   `canvas::Program`, drawing with native `Frame`/`Path`/`Stroke`/`Text` (not
   the bitmap pipeline the chart and 3D use, so lines and text stay crisp at
   any zoom). It applies **one affine** to model-mm coordinates: fit-to-canvas
   baseline scale → zoom multiplier → pan translation. The **sole**
   screen-space exception is dimension-text font size, held constant px
   regardless of zoom (CAD convention), drawn at the transformed anchor. It
   converts scroll into `DiagramZoom` and drag into `DiagramPan`, publishing
   deltas the way `OrbitCanvas` publishes orbit deltas (never reads committed
   view state back — avoids stale-base publishing).

## Data model

```rust
// diagram/mod.rs
pub struct DiagramInput {
    pub scene: viz::SceneData,     // reused, projected by geometry.rs
    pub dims: Vec<Dimension>,      // from the per-family diagram_model
}

pub enum DimLayer { Lengths, Diameters, Coils }

pub enum DimKind {
    // Linear measurement between two model-mm anchor points along an axis.
    Linear { from: (f64, f64), to: (f64, f64) },
    // Diameter across the envelope at a given axial station.
    Diameter { at_axial: f64, half_span: f64 },
    // Angular measurement (torsion leg angle, helix angle): vertex + two rays.
    Angular { vertex: (f64, f64), start_deg: f64, sweep_deg: f64 },
    // Text-only annotation (coil counts, initial tension, end/hook type).
    Note { at: (f64, f64) },
}

pub struct Dimension {
    pub kind: DimKind,
    pub layer: DimLayer,
    pub value: f64,        // the design-field number (label source)
    pub label: String,     // formatted with unit by the presenter
}
```

App state (per the no-recompute discipline):

```rust
pub struct DiagramView { pub zoom: f32, pub pan: iced::Vector }   // default fit
pub struct DimLayers  { pub lengths: bool, pub diameters: bool, pub coils: bool }
```

Messages: `DiagramZoom(f32)`, `DiagramPan(f32, f32)`, `DiagramLayer(DimLayer)`
(toggle). All are view-only — they never trigger a re-solve, matching the
`Orbit`/`Zoom` precedent.

## Mirror-drift guard (the #1 risk)

The drawing and the callouts are two renderings of the same numbers — geometry
from projected `SceneData`, labels from design fields. On screen they must
agree exactly. The binding rule: **anchor every feature-dimension to the
geometry's computed bounds, label it with the design-field value, and assert
they are equal within tolerance in tests.** Concretely, per family:

- `projected coil-body axial span == free_length`
- `envelope half-height (radial extent incl. wire edges) == outer_dia / 2`
- inner silhouette edge `== inner_dia / 2`
- (family-specific spans: body length, leg lengths, hook opening, per-stage OD)

These equality assertions are the drift guard — the same discipline as the SDF
uniform round-trip tests. If projection and design field disagree, a test
fails rather than the user seeing a dimension line that doesn't match its own
number.

## Integration

- `VisualMode` (`app.rs`) gains a `Diagram` variant. `widgets::visual_toggle`
  gains a third segment `("2D", VisualMode::Diagram)`.
- `widgets::results_visual_element` gains a fourth lazy closure
  `diagram: impl FnOnce() -> DiagramInput`. The `Diagram` arm calls
  `diagram::diagram_element(pal, diagram(), app.diagram_view, app.diagram_layers)`.
  Laziness is preserved — the `DiagramInput` (scene + dims) is built only when
  Diagram mode is active, exactly as `wire3d`/`sdf3d` are today.
- Each family's `view.rs` passes a closure that builds `DiagramInput` from its
  solved outcome: reuse its existing `scene_model` for the `SceneData`, and
  call its new `diagram_model` for the `dims`.
- A compact layer-toggle row (the `[✓ lengths] [✓ diameters] [ coils]` control)
  renders above the canvas in Diagram mode only, wired to `DiagramLayer`.
- Degenerate `SceneData` (empty/capped/non-finite, via `coil_body_is_empty` /
  `scene_extent`) → the same placeholder discipline and wording the 3D view
  uses. Non-finite design fields are guarded in the dimension presenter (no
  NaN/inf label reaches the view).
- Both palettes (light/dark) are honored — dimension lines, text, and wire use
  palette tokens, no hardcoded colors.

## Per-family callout sets

Comprehensive; grouped by layer. Exact springcore design-struct field names
are bound during planning (verified against each family's design struct — only
compression's fields are confirmed so far: `free_length`, `solid_length`,
`outer_dia`, `inner_dia`, `wire_dia`, `mean_dia`, `active_coils`,
`total_coils`).

| Family | Lengths | Diameters | Coils / Angular |
|---|---|---|---|
| Compression | free L₀, solid Lₛ, pitch | OD, ID, wire ⌀ | active Nₐ, total Nₜ, end-type note |
| Conical | free length, solid height | large OD, small OD, wire ⌀ | active/total, pitch |
| Extension | free length, body length, hook opening | OD, ID, wire ⌀ | coil count, initial-tension note, hook-type note |
| Torsion | body length, leg lengths L₁/L₂ | OD, ID, wire ⌀ | coil count, **leg angle (angular)**, free-angle note |
| Assembly | overall free, overall solid | envelope OD, per-stage OD | stage summary (series/nested), per-member note |

## Testing (strict TDD)

The bulk is pure-presenter tests; the humble view carries the thin drawing/
event logic only.

- **Projection:** silhouette edge-offset correctness; outer edge = OD/2, inner
  edge = ID/2 identity; degenerate scene → empty projection.
- **Dimensioning (per family):** each dimension's `value` equals the design
  field; each feature-dimension's anchor equals the geometry bound (the
  **mirror-drift equality assertions**); layer assignment correct.
- **Layout engine:** ladder stacking and non-overlap; angular arc geometry;
  only toggled-on layers laid out.
- **Toggles:** each `DiagramLayer` message flips exactly its group; combinations
  render the expected dimension subset.
- **View transform:** fit baseline; zoom clamp bounds; pan; the constant-px
  font-size exception.
- **Integration:** `VisualMode::Diagram` round-trips through `visual_toggle`;
  the `diagram` closure is invoked only in Diagram mode (laziness pin).
- **Degenerate:** capped/non-finite scene shows the placeholder, not a panic.
- Both palettes exercised. **No machine-dependent canvas snapshot** — fonts/AA
  vary across machines (same rule as the shader path).

## Build order (prove end-to-end before generalizing)

1. **Compression, fully end-to-end** — projection + dimension presenter +
   layout + canvas + app wiring + toggles + zoom/pan. The shared units get
   their first real shape here.
2. **Conical** as family #2 — varying radius and distinct large/small OD stress
   the anchoring and prove the projection generalizes.
3. **Extract/confirm the shared layout seams** now that two concrete families
   exist (avoids designing the fully-general engine before the second family).
4. **Extension** — hook silhouette + hook-opening/initial-tension callouts.
5. **Torsion** — leg silhouette + the **angular** leg-angle primitive.
6. **Assembly** — per-stage/overall callouts over the composed scene.
7. **Final adversarial multi-reviewer panel** cycled to convergence before ship.

~10 SDD tasks.

## Global constraints

- **springmaker-only** — no `springcore` changes (the engine already exposes
  every needed design field).
- **ADR 0008** humble-view/presenter split; all layout math pure and in model
  mm; the view applies one affine + the constant-px font exception only.
- **Both palettes** (light/dark), palette tokens only, no hardcoded colors.
- **No-recompute discipline** for all view messages (zoom/pan/layer) — mirrors
  `Orbit`/`Zoom`; never re-solves.
- **Degenerate/placeholder discipline** reused from the 3D path.
- **Strict TDD**; both `clippy` commands; `cargo doc -D warnings`; repo-wide
  `typos`; springmaker not mutation-gated (springcore stays 0 in-diff).
- **No machine-dependent snapshot** of canvas output.
- **Naming:** no commercial product/vendor names in persisted files.

## Out of scope

- File export (DXF/SVG/PNG) — its own future increment.
- Rectangular-wire GUI tab; torsion/extension output-guard hardening;
  springmaker view-layer mutation coverage — all unchanged, still benched.
