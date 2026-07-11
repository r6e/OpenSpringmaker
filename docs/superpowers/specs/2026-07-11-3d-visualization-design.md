# 3D Spring Visualization — Design

**Date:** 2026-07-11
**Status:** Approved (brainstorm 2026-07-11)
**Increment:** second of three demo-prep GUI increments (plots ✓ PR #62 → **3D visualization** → display polish)

## Goal

Every family tab can show its solved design as a 3D spring — real geometry
(coil diameters, pitch, wire diameter, coil counts, hooks/legs/members),
drag-to-orbit, toggled with the load-deflection chart in the results panel.

## Decisions (from the brainstorm)

1. **Rendering:** plotters `build_cartesian_3d` with `with_projection(yaw,
   pitch)`, drawn through the SHIPPED bitmap→canvas pipeline (palette tokens,
   bundled font, 760×300 RGBA, letterbox, degenerate-placeholder pattern).
   The wgpu shader-widget path (shaded solid) was considered and declined:
   bigger wow, much bigger subsystem, too risky for the runway. The
   hand-rolled canvas projection was declined as re-implementing what
   plotters has. Consequence accepted: wire renders as a stroked polyline
   (round cross-section regardless of family; thickness ≈ wire diameter
   mapped to a clamped pixel width), an engineering-sketch look rather than
   a render.
2. **Interactivity:** drag-to-orbit. Orbit angles must survive re-render, so
   unlike chart hover this uses one `Message` and App state (see §Orbit).
   No zoom; no hover readout (inverse-projecting a cursor onto a 3D polyline
   is ill-posed — the readout stays a chart feature).
3. **Scope:** all five current GUI families. Rectangular is structurally
   excluded until its GUI increment (no tab).
4. **Placement:** a chart ↔ 3D toggle in each results panel (saves ~300 px of
   panel height; the two visuals share one slot). Toggle state is global
   (`App.results_visual`), so the chosen mode follows the user across tabs.

## Architecture (ADR 0008 split; mirrors `plot/`)

```
springmaker/src/viz/
├── mod.rs        SceneData contract + shared helix sampler + scene_extent
├── render3d.rs   plotters 3D → RGBA bitmap (humble; consumes SceneData + orbit)
└── canvas3d.rs   OrbitCanvas — canvas::Program (humble; bitmap + drag→Message)

springmaker/src/<family>/scene_model.rs   per-family pure presenters
```

### `SceneData` (pure contract)

The 3D sibling of `ChartData` — deliberately a separate type (a 2D chart and
a 3D scene share philosophy, not fields):

```rust
pub struct SceneData {
    pub polylines: Vec<Polyline3>,
}
pub enum SceneRole { Wire, Member, Detail }   // Detail = hooks, legs
pub struct Polyline3 {
    pub points: Vec<(f64, f64, f64)>,  // true millimetres, y = spring axis
    pub role: SceneRole,
    pub stroke_px: u32,                // from wire dia, clamped legible range
}
```

Roles map to palette tokens in the renderer only (Wire → ACCENT, Member →
MUTED-differentiated per member, Detail → WARN). Coordinates stay in true mm
(aspect-honest); the renderer frames the scene's bounding box symmetrically
about the axis.

### Shared helix sampler (`viz/mod.rs`, pure)

```rust
pub fn helix(
    radius_at: impl Fn(f64) -> f64,   // t ∈ [0, 1] along the wire
    height_at: impl Fn(f64) -> f64,   // axial position at t (integrated pitch)
    turns: f64,
    samples_per_turn: usize,          // fixed constant, e.g. 32
) -> Vec<(f64, f64, f64)>
```

Every family parameterizes this one generator; hooks and legs are the only
geometry built outside it.

### Renderer (humble)

`render_scene(scene: &SceneData, orbit: Orbit) -> Option<Vec<u8>>` — plotters
`build_cartesian_3d` over the mm bounding box, `with_projection` applying
yaw/pitch, axes and mesh suppressed (clean sketch look; no tick labels).
`None` iff `scene_extent` is `None`. Per-point finite filtering as defense in
depth (the 3D extension of the chart's `plottable` discipline).

### `OrbitCanvas` + orbit state

- Drag *tracking* (press origin, last position) is ephemeral canvas `State`.
- Committed angles are App state: `App.orbit: Orbit { yaw: f32, pitch: f32 }`
  (global, defaults to a pleasing three-quarter view), updated via
  `Message::Orbit(Orbit)` published from the canvas
  (`canvas::Action::publish`) during drags; `view()` re-rasterizes with the
  new projection — the same re-render-on-view cost profile as the shipped
  chart pipeline.
- The pure `orbit_step(current: Orbit, dx: f32, dy: f32) -> Orbit` owns
  sensitivity and the pitch clamp (no pole-flip); unit-tested. Yaw wraps.
- `mouse_interaction` shows the grab cursor over the canvas.

### Toggle

`App.results_visual: VisualMode { Chart, Spring3d }` (defaults `Chart`) +
`Message::VisualMode(VisualMode)`. Each results panel's Populated arm renders
a small two-option control and the selected visual; which visual renders is a
presenter decision (pure fn over App state). No outcome → neither visual nor
toggle (existing Populated gating).

## Per-family geometry (honest simplifications named)

| Family | Body | Family-specific |
|---|---|---|
| Compression | helix, R = mean/2 constant, solved pitch across active coils | squared/squared-ground end coils flatten to ≈ wire-dia pitch (one flattened turn per squared end); plain ends flatten nothing — driven by the design's `EndType` |
| Conical | helix, R(t) linear large→small | taper shows telescoping naturally |
| Extension | close-wound body (pitch = wire dia) | two simplified hook loops: circular arcs from the design's hook radii, rotated into the end planes; continuity with the body endpoint is exact |
| Torsion | close-wound body | two straight tangential legs at the solved leg lengths |
| Assembly | each member's solved design as its own body | nested → concentric, one axis; series → stacked axially with a small visual gap; members role-differentiated like the chart legend |

Accepted limitations (documented in module docs, not silently): round
stroked cross-section for all families (rectangular wire would need the wgpu
path); hooks are representative arcs, not exact hook developments; no
self-intersection/clearance rendering beyond what true geometry shows.

## Degenerate handling

Presenters emit only finite geometry with non-negative radii; `scene_extent`
(3D bounding box) returns `None` for degenerate scenes → the shipped
placeholder-text pattern in the shared slot. Post-solve-mutation fixtures
test the path per family (the chart precedent). Orbit angles are finite by
construction (`orbit_step` from finite cursor deltas, clamped pitch).

## Testing

- **Presenter tests (bulk):** helix sampler point counts, radius endpoints,
  height integration (total height ≈ Σ pitch); conical taper endpoints pinned
  against engine fields; extension body pitch == wire dia + hook arc
  continuity at 1e-9; torsion leg tangency + solved lengths; assembly
  concentric radii ordering (nested) / axial stacking without overlap
  (series); degenerate → extent `None`.
- **Pure math:** `orbit_step` clamp/wrap/sensitivity; `scene_extent`;
  bounding-box framing.
- **Render smoke:** bitmap content-over-background for one representative
  scene (shipped test pattern).
- **Simulator pins:** the toggle actually swaps the visual (pinned via the
  toggle's own labels — canvases have no queryable text); `Message::Orbit`
  → re-render wiring; per-family 3D presence after a solve.
- springmaker-only; springcore untouched (mutation gate trivially clean);
  full adversarial panel with the input-domain adversary pointed at the
  projection/orbit math and geometry samplers.

## Out of scope

- Zoom/pan, hover readout on the 3D view.
- Shaded/solid rendering (wgpu shader widget) — the documented fidelity
  trade-off; revisit only if the demo demands it and runway allows.
- Rectangular-family scene (arrives with its GUI increment).
- Export of the 3D view (reports increment candidate).
- Display polish (increment three).
