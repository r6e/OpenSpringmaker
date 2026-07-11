# Display Polish — Design

**Date:** 2026-07-11
**Status:** Approved (brainstorm 2026-07-11)
**Increment:** third of three demo-prep GUI increments (plots ✓ PR #62 → 3D
visualization ✓ PR #63 → **display polish**)

## Goal

Make the app demo-ready: one visual language across all five family panels, a
family tab row that shows the app's breadth at a glance, every control styled
through the shared system, and a Light/Dark/System theme.

## Decisions (from the brainstorm)

1. **Scope:** full visual refresh INCLUDING a light theme (Settings toggle:
   System · Light · Dark, default System).
2. **Decomposition:** two PRs under this one spec, refresh first.
   - **PR 1 `feat/gui-visual-refresh`** — everything demo-visible plus the
     palette-struct pre-staging.
   - **PR 2 `feat/gui-light-theme`** — the theme system on top of the
     cleaned-up surface. PR 2 must not need to touch the five family views.
3. **Theme architecture:** `Palette` struct resolved once per view build via
   `app.pal()`; style fns are palette-parameterized closure factories (the
   pre-existing `correction_option_style(selected)` precedent). The
   brainstorm's original "Approach A" framing (`Palette::of(&Theme)`
   theme-keyed lookup) was superseded during PR 1 implementation: views
   re-run on every theme change, so build-time resolution is equally correct
   and the factory precedent already existed in-tree — see §Resolution.
   iced stock themes (Approach C) remain declined: they abandon the
   engineering-instrument identity.

## PR 1 — Visual refresh

### Family tab row

Replace the 180 px `styled_pick_list` family dropdown (`calculator.rs`) with
a horizontal segmented row of five styled buttons — Compression · Extension ·
Torsion · Conical · Assembly — under the header. Styling follows the
`settings_view` correction-option precedent (selected = accent-tinted
background, hover = raised, radius 4). Buttons wrap `text()` children, so
family switching becomes Simulator-queryable; a future Rectangular tab slots
in without layout change. Dispatches the existing family-selection message —
no App state changes.

### Shared segmented control

One `segmented(options, selected, on_pick)` widget in `widgets.rs` replaces
every radio cluster:

- Units toggle (Metric / US) — moved to the calculator footer beside Save/Load
  during Task 5 (plan-authorized: the five-tab header exceeded the 1200 px budget
  with the units control in it).
- Chart / 3D visual toggle (replaces `visual_toggle`'s radios; same
  `Message::Visual` dispatch).
- Extension hook-mode (Default / Custom).
- (PR 2 reuses it for the theme picker.)

This closes the carried radio→button item: radio labels are structurally
invisible to `iced_test`'s Simulator (no `operate()` override — verified
against vendored iced 0.14 during the 3D increment); buttons with `text()`
children are queryable.

### Five-panel consistency canon

Each known divergence resolves to a named winner:

| Divergence | Canon |
|---|---|
| Torsion headline is two plain rows | Torsion adopts `render_governing_rate` hero (governing angular rate); secondary rate stays a normal row below |
| Section order varies per family | hero → "Geometry" rows → "Load points" → Chart/3D toggle + visual slot → Fatigue → Min-weight → family footer |
| "Geometry" vs "Summary" heading | "Geometry" everywhere except assembly, which keeps "Summary" (its block genuinely summarizes members) |
| Only torsion colors overstressed cells | Torsion's `Emphasis` (DANGER on overstress) promoted to ALL families' load tables |
| `%MTS` vs `% Allow` header forms | Spaced form everywhere ("% MTS", "% Allow", …) |
| Compression pushes Hidden fatigue/min-weight unconditionally (phantom spacing gaps) | Conditional push (torsion's pattern) |
| Compression's two-column Setup | Stays — four setup controls earn the density; not a defect |

### Spacing tokens

Named constants beside `SZ_*` in `widgets.rs`: `SP_XS=4, SP_SM=8, SP_MD=12,
SP_LG=16, SP_XL=24`, plus a panel-padding constant (20) and names for the
one-off fixed widths (status prefix column 72, header spacer 160, load-table
"Pt" column 24). Existing spacings map to the nearest token (6 → XS or SM,
10 → SM or MD — per-site judgment at plan time); no visual redesign, just
tokenization.

### Stragglers

- Both placeholders (`CHART_PLACEHOLDER`, `SCENE_PLACEHOLDER`) styled
  (`SZ_BODY`, muted) instead of bare default `text()`.
- The scene placeholder becomes **state-aware**: an empty coil body (the
  `MAX_RENDER_TURNS` cap — valid input) reads "3D view unavailable: coil
  count exceeds the renderable 3D limit."; non-finite geometry keeps
  "…(check inputs)". Distinguishable today: empty points vs non-finite
  points.
- Assembly member blocks become sub-cards (`RAISED` background, `LINE`
  border, radius 4) instead of padded text.
- `screen_shell(header, content)` helper unifies the triplicated root chrome
  (INK background, padding 24, `max_width(1200)`) across Calculator /
  Materials / Settings. Calculator keeps whole-page scroll; Materials keeps
  its per-panel scrolling (justified: independent list + form); Settings
  adopts whole-page scroll and the 1200 cap.

### Carried panel items (fold in)

- Letterbox draw-boilerplate helper shared by `ChartCanvas`/`OrbitCanvas`.
- Split the bundled `every_family_renders_3d_after_solve` test per family.
- Assembly scene comment precision nit (NaN coil counts route through the
  entry guard, not the cascade).

### PR 2 pre-staging

The `C` const namespace becomes `struct Palette` (same ten token names as
fields) with `const DARK` only; every `C::X` read migrates to the token
field. `chart_element` / `scene_element` / `render_chart` / `render_scene`
gain their `&Palette` parameter NOW (always `DARK` in PR 1), so the view
call sites change here and PR 2 genuinely never touches the five family
views. Purely mechanical, lands before any behavior change on top of it.

## PR 2 — Theme system

### Light palette

`Palette::LIGHT`, designed as the dark theme's mirror (not an inversion):
paper-white background; panel/raised as warm greys a step darker than the
background; the accent blue darkened to pass contrast on white; WARN /
DANGER / SUCCESS re-tuned for light backgrounds; TEXT near-black, MUTED
mid-grey. Exact values picked at plan time against WCAG AA for body text and
the hero readout.

### Resolution

Style fns are palette-parameterized closure factories (the
`correction_option_style` precedent); views resolve `app.pal()` once per
build — theme switches re-run `view()`, so no `&Theme` lookup is needed.
Views and both bitmap renderers read `app.pal()`; `render_chart` /
`render_scene` already take `&Palette` (pre-staged in PR 1) — presenters
stay pure data.

### Settings toggle + system tracking

- Settings gains a three-option segmented control: **System · Light · Dark**
  (default System), reusing PR 1's widget.
- `App` holds `theme_pref` and `system_mode: theme::Mode`. Effective mode =
  pref override, else live system mode.
- Startup: `iced::system::theme()` task seeds `system_mode`; a permanent
  `iced::system::theme_changes()` subscription keeps it current (both APIs
  verified present in vendored iced 0.14). OS-level switches retheme the app
  live while pref = System.
- `theme_pref` persists through the existing settings store, same
  load/save/atomic-write path as the curvature-correction choice.

### Renderer awareness

Charts, fatigue diagrams, and 3D scenes re-rasterize through `app.pal()` on
the next view pass after a mode change — no cache invalidation needed; the
shipped pipeline already re-renders per view build.

## Testing

- **PR 1:** the existing test floor is the regression net; arm-dispatch
  discriminators, placeholder pins, and stateful probes must survive with
  assertions intact. Radio-workaround comments/assertions are REPLACED with
  direct label queries (strict strengthening — segmented buttons are
  queryable). New pins: tab row switches family; canonical section order per
  family; overstress emphasis presenter tests for the four families gaining
  it; capped-vs-invalid scene placeholder wording.
- **PR 2:** `Palette::of` totality (both themes resolve); pref persistence
  round-trip; effective-mode resolution table (pref × system_mode);
  simulator pin that switching pref changes rendered output (chart-bitmap
  background pixel differs between modes); settings-toggle pins.
- Both PRs: springmaker-only (mutation gate trivially clean), full
  adversarial panel per house rules — input-domain + stateful-UI adversaries
  included; PR 2's panel adds a both-themes sweep lens (every screen × both
  palettes).

## Risks (named)

- `iced::system::theme()` behavior inside the `iced_test` Simulator is
  unverified. Mitigation: the effective-mode resolution is a pure function
  pinned directly; the subscription stays a thin humble shell (OrbitCanvas
  discipline). If the Simulator can't drive theme events, System mode is
  still fully covered minus the event plumbing.
- Light-palette taste/contrast: mitigated by the WCAG AA check at plan time.
- The `Palette` migration is wide but mechanical; it lands in PR 1 before
  any behavior changes stack on it.

## Out of scope

- Rectangular family tab (arrives with the rectangular GUI increment; the
  tab row is built to absorb it).
- Reduced-motion/accessibility audit beyond contrast (future increment).
- Materials/Settings content redesign (only the shared shell changes).
- Window-size/responsive-layout work beyond the existing min-size.
