# ADR 0009: Render the results chart as a plotters bitmap with a bundled font

**Status:** Accepted

## Context

The GUI's results chart (force–deflection line + operating-point markers) was
drawn with `plotters` through the `plotters-iced` bridge, which renders plotters
output onto an iced `canvas`. That bridge couples directly to a specific iced
version: `plotters-iced 0.11` depends on `iced 0.13`. This is exactly what
blocked the iced 0.14 upgrade (and, transitively, kept us on the `lru` line
flagged by RUSTSEC-2026-0002 — see [ADR 0007](0007-accept-transitive-lru-advisory.md)):
the original `plotters-iced` has shipped no iced-0.14 release.

The only drop-in for iced 0.14 was `plotters-iced2`, a brand-new, single-version,
single-maintainer **fork**. Adopting it would have unblocked the upgrade with the
least code change, but it runs against our preference for dependencies with
community size and active maintenance, and adds an unproven crate to the tree.

## Decision

**Drop the iced-coupled bridge entirely. Draw the chart with `plotters` into an
in-memory RGB bitmap and display it through iced's built-in `image` widget.**

- `plotters` is kept (well-established, widely used) but with `default-features =
  false`, enabling only `bitmap_backend`, `line_series`, and `ab_glyph`. This
  drops `font-kit`, `svg`, `image` codecs, `gif`, and `chrono` from the tree.
- The existing chart-drawing code (`ChartBuilder` / mesh / `LineSeries` /
  markers) is backend-generic and reused almost verbatim; only the backend
  (`BitMapBackend::with_buffer`) and the widget wrapper change. The pure data
  layer (`force_deflection_series`, `finite_positive_extent`) and its tests are
  untouched.
- **Text uses a bundled font** via plotters' `ab_glyph` backend. `ab_glyph` ships
  no built-in fonts, so we register `assets/DejaVuSans.ttf` once under the
  `"sans-serif"` family. This removes the runtime *system-font lookup* that the
  default `ttf`/`font-kit` path performs — that lookup can return nothing on
  minimal/headless environments (including CI), producing blank labels or
  errors. A bundled font makes rendering deterministic everywhere. A unit test
  rasterizes a chart and asserts content was drawn, exercising registration end
  to end.

The bitmap is rendered at a fixed resolution and scaled to fit by iced.

## Consequences

- **No iced-version coupling for charts.** The plotters bitmap backend does not
  depend on iced, so a future iced bump can never be blocked by a charting
  crate again — and we took on no fork.
- iced gains the `image-without-codecs` feature: the chart feeds raw RGBA via
  `Handle::from_rgba`, so no image-format *decoders* (PNG/JPEG/GIF) are pulled
  in — there is no untrusted-image decode path. (The `image` crate's core types
  remain, as iced's image widget needs them.)
- **Tradeoff: raster, not vector.** The chart is rasterized at a fixed size and
  scaled, so it is slightly less crisp on HiDPI / large resizes than a canvas
  would be, and it is re-rendered when the view rebuilds (cheap for a chart that
  only changes on recompute; revisit with a cache if it ever shows up in
  profiles). Accepted for an engineering desktop tool.

## Bundled font provenance and license

`assets/DejaVuSans.ttf` is **DejaVu Sans 2.37**, taken from the upstream release
`dejavu-fonts-ttf-2.37` (github.com/dejavu-fonts/dejavu-fonts). DejaVu fonts are
distributed under a permissive license (Bitstream Vera + Arev) that allows
bundling and redistribution; the full text is committed alongside the font at
`assets/LICENSE-DejaVu.txt`. The license is compatible with this project's
`MIT OR Apache-2.0` terms.

## Alternatives considered

- **`plotters-iced2` (the fork).** Least effort, but adds a new single-maintainer
  dependency; rejected on supply-chain grounds.
- **Reimplement the chart on iced's native `canvas`** (drop plotters too).
  Cleanest dependency surface and vector-crisp, but the most work — it
  reimplements tick selection, label formatting, and axis layout that plotters
  provides for free, moving that correctness risk into hand-rolled code. A
  reasonable future move if the charting needs grow; not warranted now.
- **Stay on iced 0.13.** Leaves RUSTSEC-2026-0002 unresolved and the toolkit
  aging; rejected.
