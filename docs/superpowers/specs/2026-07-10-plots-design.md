# Plots Library — Design

**Date:** 2026-07-10
**Status:** Approved (brainstorm 2026-07-10)
**Increment:** first of three demo-prep GUI increments (plots → 3D visualization → display polish)

## Goal

Every spring-family tab shows a live chart. Load-deflection charts for all five
GUI families (extension, torsion, conical, assembly join the existing
compression chart), fatigue diagrams for the two families with fatigue engines
(compression Goodman, torsion Gerber), and a hover readout on every chart.

## Context — what already exists

`springmaker/src/plot.rs` has shipped since PR #3: a load-vs-deflection chart
drawn by `plotters` into an in-memory RGB bitmap and shown via iced's `image`
widget, with a bundled DejaVu Sans font (`ab_glyph`), palette-token colors, and
degenerate-design guards (`finite_positive_extent` → text placeholder). That
rendering-approach decision is documented in `Cargo.toml` and the module header
(no iced-coupled plotters backend, so no `plotters-iced` version lag) and is
**not** revisited here. The chart is wired only into compression's results
panel and is static.

Both fatigue engines already expose everything a fatigue diagram needs:
`springcore::FatigueResult` (τa, τm, Se, Ssu, Goodman nf — Shigley §10-9) and
`springcore::torsion::TorFatigueResult` (σa, σm, Se, Sut, Sa, Gerber nf —
Shigley §10-12, Eqs. 10-58/59/60). The GUI shows these only as numbers today.

**This increment touches springmaker only.** Every needed quantity (rates,
load points, `initial_tension`, `travel_limit_*`, fatigue stresses/strengths)
is already public springcore API. The springcore mutation-gate surface is
empty; the review panel still runs in full.

## Decisions (from the brainstorm)

1. **Scope:** all five current families get load-deflection charts; compression
   and torsion additionally get fatigue diagrams. Rectangular is excluded until
   its GUI increment (no `Family::Rectangular` yet).
2. **Interactivity:** hover readout (crosshair + value box). No zoom/pan.
3. **Assembly chart:** composite line + per-member overlay lines with a legend,
   travel-limit point marked on the composite.
4. **Rendering/hover integration (approach A):** keep the plotters→bitmap
   pipeline; wrap the bitmap in an iced `canvas::Program` that draws it via
   `Frame::draw_image` (available in iced 0.14, verified against
   `iced_graphics-0.14.0`) and draws the hover overlay as fresh geometry.
   Hover state is ephemeral view state — **no `Message` plumbing, no `App`
   state**.

## Architecture (ADR 0008 split)

```
springmaker/src/plot/            (grows out of today's plot.rs)
├── mod.rs        ChartData contract + public entry points (re-exports)
├── mapping.rs    ChartMapping — pure data↔pixel affine transform
├── render.rs     plotters → RGBA bitmap (humble; consumes ChartData)
└── canvas.rs     ChartCanvas — canvas::Program (humble; bitmap + overlay)

springmaker/src/<family>/plot_model.rs   per-family pure presenters
```

### `ChartData` (pure contract)

The boundary between families and the chart core. Families know nothing about
pixels; the renderer knows nothing about springs.

```rust
pub struct ChartData {
    pub x_axis: AxisMeta,        // label text incl. unit, readout symbol ("y", "θ", "τm"…)
    pub y_axis: AxisMeta,
    pub lines: Vec<Line>,        // points + role + optional legend name
    pub markers: Vec<Marker>,    // operating points, travel limit, fatigue points
}

pub enum LineRole { Primary, Member, Envelope, LoadLine }
```

Both chart kinds (load-deflection, fatigue) are plain `ChartData` instances.
Roles map to stroke style/color in the renderer only. Axis labels carry the
display unit chosen by the presenter (`UnitSystem` is a presenter input), so
the core is unit-agnostic.

### Per-family presenters (pure, tested)

One function per family, `<family>_chart(&<Design>, UnitSystem) -> ChartData`,
in `<family>/plot_model.rs`, following the `view_model` file convention.
Compression's existing `force_deflection_series`/`finite_positive_extent` logic
migrates into this shape (`compression/plot_model.rs`); `plot.rs`'s
compression-specific public fn is retired.

Fatigue presenters live beside them: `compression/plot_model.rs` also builds
the Goodman `ChartData` from `FatigueResult`; `torsion/plot_model.rs` builds
the Gerber `ChartData` from `TorFatigueResult`.

### Renderer (humble)

Today's `draw_chart`/`render_rgba`, generalized to iterate `ChartData.lines`
(stroke per role, legend entries for named lines) and `ChartData.markers`.
Fixed 760×300 bitmap, same margins (24), label bands (x 44, y 64), palette
tokens, bundled font. Emits `(RGBA buffer, ChartMapping)` — the mapping is
built from the same constants and the same axis ranges in the same place, so
it cannot drift from what plotters actually drew.

### `ChartCanvas` + `ChartMapping`

`ChartCanvas` implements `canvas::Program`:

- **Bitmap layer** through a `canvas::Cache`, drawn with `Frame::draw_image`
  at the bitmap's native aspect ratio, letterboxed inside the widget bounds
  (fill-width responsive). Cache is invalidated only when a new `ChartData`
  arrives (re-solve or unit toggle) — mouse movement never re-renders plotters
  output.
- **Overlay layer** drawn fresh per frame: crosshair lines + a readout box,
  only while the cursor is inside the plot rectangle proper (not the label
  bands). Readout text comes from a pure formatter over `AxisMeta` + the
  mapped data coordinates (e.g. `y = 12.3 mm · F = 45.6 N`,
  `θ = 30.0° · M = 250.0 N·mm`). Near the right/top edges the box flips to the
  cursor's other side so it never clips.

`ChartMapping` (pure): the data ranges handed to plotters (after the existing
1.1 headroom factor) + the plot rectangle inside the bitmap. Methods:
`data_to_pixel`, `pixel_to_data`, `in_plot_rect`, each composed with the
bitmap→bounds scale supplied at draw time. Fully unit-testable without a
renderer.

## Per-family chart semantics

| Family | X axis | Y axis | Lines | Markers |
|---|---|---|---|---|
| Compression | deflection (mm/in) | load (N/lbf) | origin→max-load linear | operating points |
| Conical | deflection | load | same linear shape (linear-range model) | operating points |
| Extension | deflection | load | three-branch: Fi>0 jump (0,0)→(0,Fi)→(y_max,F_max) when max load exceeds Fi; two-point (0,0)→(0,Fi) when every load ≤ Fi (renders via the x=0/y>0 extent allowance); Fi=0 → plain origin→max line | operating points |
| Torsion | angle (deg) | moment (N·mm / lbf·in) | origin→max ideal rate line | actual load points — markers read the engine's solved fields (currently coincident with the ideal line under the linear engine; the field-read is what is pinned, not the coincidence) |
| Assembly | assembly deflection | force | composite k_total line (Primary) + one Member line per member (slope kᵢ, legend "Member N") | assembly operating points + travel limit on the composite |

Assembly reads: nested members visibly stack (composite steeper than every
member); series assemblies visibly soften (composite shallower than every
member). Member lines draw thinner/muted; the composite draws in the accent
color.

Extent rule: x/y extents come from the max finite operating point; x extents
may be zero when y is positive (the extension family's initial-tension jump
sits at x = 0), and the renderer floors the axis rather than treating a zero
x-extent as degenerate. Fallbacks when no operating loads exist: compression
and conical fall back to the at-solid point (compression-family precedent);
assembly to the travel-limit point; extension and torsion have no
solid-state analogue and render the placeholder instead (their forms already
require loads to produce a solved outcome, so the fallback is unreachable in
practice — the placeholder is the defensive path).

## Fatigue diagrams

Axes: (mean stress, alternating stress) in display stress units (MPa / ksi —
matching the app-wide US convention, `presenter::display_stress`), rendered
inside the existing fatigue results sections, only when the analysis returned
`Ok` (the `NoFatigueData` degradation path is untouched). Compression's axes
are SHEAR stress (τm/τa symbols — its `FatigueResult` quantities are
torsional shear); torsion's Gerber axes are normal stress (σm/σa symbols —
its bending fatigue check).

- **Compression — Goodman (Shigley §10-9).** Envelope: straight line
  (0, Se)→(Ssu, 0). Load line: origin through (τm, τa), role `LoadLine`.
  Operating-point marker. All values from `FatigueResult` — no formula is
  re-derived in the GUI.
- **Torsion — Gerber (Shigley §10-12).** Envelope: σa = Se·(1 − (σm/Sut)²)
  sampled as a ~64-point polyline over [0, Sut] (presentation-side sampling of
  the engine-cited criterion; the engine remains the factor-of-safety
  authority). Load line r = Ma/Mm. Two markers: the operating point (σm, σa)
  and the strength-amplitude intersection (Sm\* = Sa·Mm/Ma, Sa) from
  `TorFatigueResult` — the visible gap between them is the factor of safety.

## Error handling / degenerate designs

- Presenters emit only finite points (per-line filtering — assembly has
  several lines).
- `finite_positive_extent` generalizes to multi-line `ChartData`; a chart with
  no finite positive extent renders the existing text placeholder and never
  reaches plotters (the non-finite-range panic class stays impossible).
- Fatigue presenters re-guard Se/Ssu/Sut finite-positive before building the
  envelope — the engine's output guards already ensure this; the presenter
  doesn't trust its caller (defense in depth).
- No chart ⇒ no canvas ⇒ no mapping: division can't enter `pixel_to_data`
  because a mapping exists only alongside a successfully rendered bitmap,
  which required positive extents.

## Testing

- **Presenter tests (bulk):** per-family `ChartData` builders — extension's
  intercept at exactly Fi; torsion axis units in both unit systems; conical
  linearity; assembly member slopes + composite slope + travel-limit marker
  for both topologies; Goodman endpoints; Gerber envelope endpoints/vertex +
  intersection point; degenerate inputs → placeholder path; readout
  formatting.
- **`ChartMapping` tests:** data→pixel→data round-trip identity; corner
  pinning (data origin ↔ plot-rect bottom-left); `in_plot_rect` boundary
  cases; readout edge-flip logic.
- **Render smoke tests:** the existing bitmap-content + font-rasterization
  pattern, one per chart kind (single-line, multi-line + legend, fatigue
  envelope).
- **Simulator (`ui_tests`):** chart element present per family after a solve;
  placeholder on a degenerate design; fatigue chart appears only when the
  fatigue section has a result.
- Panel: full adversarial panel incl. the input-domain adversary against the
  mapping math and per-family series builders.

## Out of scope

- Rectangular-family charts (arrive with its GUI increment).
- Zoom/pan, plot export to file (candidates for the reports increment).
- Any springcore change.
- 3D visualization and display polish (increments two and three).

## Amendments (2026-07-10, post-review)

- **Marker co-gating on rate validity is crate-wide.** Every round-wire and
  torsion presenter suppresses operating/limit markers alongside the line
  whenever the design's rate (or, for extension, Fi) is invalid — not just
  the round-wire family. The design is degenerate as a whole, not just its
  line geometry.
- **Fatigue charts are gated through presenter functions**, not inline in the
  view: `goodman_chart`/`gerber_chart` return an empty `ChartData` (no lines,
  no markers) when the engine's Se/Ssu/Sut inputs aren't finite-positive,
  keeping the "no chart ⇒ no canvas ⇒ no mapping" invariant intact for the
  fatigue path too.
- **Compression's chart is repositioned beside the load table**, not below
  the whole results panel, for cross-family layout symmetry with
  extension/torsion/conical/assembly (all of which place their chart next to
  the load table).
- Fixed the US fatigue-stress display bug (psi shown instead of ksi) and the
  torsion friction erratum above; see this file's per-family table and
  fatigue-axes paragraph, already amended in place.
