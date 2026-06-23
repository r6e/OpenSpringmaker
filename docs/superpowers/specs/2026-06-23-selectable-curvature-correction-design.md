# OpenSpringmaker — Selectable Curvature-Correction Factor

- **Status:** Approved (design), pending implementation plan
- **Date:** 2026-06-23
- **Builds on:** the compression + extension families and the materials/
  persistence/GUI layers already on `main`. Implementation branches off `main`
  after the compression-golden PR (#28) merges, so the goldens it tightens are
  present.

## 1. Purpose & context

The engine corrects body torsional shear stress for wire curvature with one of
two accepted factors. Today the choice is **hard-coded and inconsistent**: static
body shear uses the **Wahl** factor (`springcore::mechanics::wahl_factor`, called
from compression `design.rs`, `optimize.rs`, and extension `design.rs`), while
fatigue uses the **Bergsträsser** factor (`fatigue.rs`). For the same spring, the
static and fatigue paths therefore apply different curvature conventions.

EN 13906-1:2013 (Formula 1) and Shigley both name **Bergsträsser** the primary
factor, with Wahl the documented alternative ("approximately the same results").

This feature makes the factor a **user-selectable global application preference**
(`Wahl | Bergsträsser`, default **Bergsträsser**) that governs **every** body-shear
curvature correction — static and fatigue, compression and extension. This (a)
resolves the static/fatigue inconsistency, (b) aligns the default with the cited
standards, and (c) lets each golden test assert against its source's own
convention exactly (tightening body-shear tolerances).

Carry-over project constraints apply: every formula/constant cited inline; strict
TDD; SI canonical internally; humble-view presenter standard (ADR 0008) for new
GUI; `cargo fmt`/`clippy -D warnings`/`cargo doc -D warnings`/`typos`/`cargo deny`/
`cargo mutants --in-diff` green before each push; mandatory adversarial review to
convergence; MSRV 1.88; dual MIT/Apache.

## 2. Decisions (locked during brainstorming)

- **Where the choice lives:** a single **global application preference**, *not* a
  per-design field. The persisted per-design schema (`ScenarioSpec`) is unchanged.
- **Scope:** the chosen factor governs **both static and fatigue** body shear, in
  **both** the compression and extension families. Hook curvature factors
  (`σ_A`/`τ_B`, a different formula family) are unaffected.
- **Default:** Bergsträsser.
- **Preference persistence:** remembered across app restarts via a new small
  app-settings file in the platform config directory.
- **GUI:** a new **Settings screen** (also a home for future app preferences).

## 3. Architecture

### 3.1 Engine (`springcore`) — the factor as a parameter

`springcore` is a pure library with no global state, so the factor is threaded as
an explicit **parameter**, supplied by the caller (the app holds the preference).

- New enum `CurvatureCorrection { Wahl, Bergstrasser }`, `#[derive(Default)]` with
  `#[default] Bergstrasser`, plus `pub fn factor(self, index: f64) -> f64`
  dispatching to the existing `wahl_factor`/`bergstrasser_factor`. Exported from
  the crate root. `Serialize`/`Deserialize` (serde) for the settings file.
- The `Scenario` trait becomes `fn solve(&self, material, correction:
  CurvatureCorrection) -> Result<…>`; the compression `solve_forward` and
  extension `solve_forward` take the parameter and call
  `corrected_shear_stress(.., correction.factor(index))` instead of
  `wahl_factor(index)`.
- `analyze_fatigue(.., correction)` uses `correction.factor(c)` in place of the
  current hard-coded `bergstrasser_factor(c)` for `τ_a`/`τ_m`.
- The min-weight optimizer's stress check (`optimize.rs`) takes the parameter and
  uses `correction.factor(c)`.
- `SavedDesign::solve`/`solve_with_material` gain the `correction` parameter
  (supplied by the caller); they do **not** store it.
- At the default (Bergsträsser): static compression/extension shear shifts
  Wahl→Bergsträsser (~1% lower); fatigue is unchanged (already Bergsträsser).

### 3.2 App-settings persistence (`springmaker`)

- A new settings module persists app-level preferences as a small `settings.toml`
  in the platform config directory (the same base the materials overlay uses).
  v1 content: `curvature_correction = "bergstrasser" | "wahl"`.
- Loaded once on startup; saved whenever the preference changes. A missing or
  malformed file falls back to the default (Bergsträsser) without error (mirrors
  the materials-overlay tolerance to malformed input).

### 3.3 GUI (`springmaker`) — Settings screen

- New `Screen::Settings` variant and a "Settings →" navigation entry.
- `settings_view_model.rs` — pure presenter (no iced), per the humble-view
  standard (ADR 0008): owns the rendering decisions for the settings screen and is
  unit-tested in isolation.
- `settings_view.rs` — the humble view rendering the presenter output: a
  Bergsträsser/Wahl selector for the curvature correction.
- `App` holds the global `correction: CurvatureCorrection`, initialized from the
  settings file at startup. Changing it: updates `App` state, persists to the
  settings file, and triggers a recompute.
- `recompute` passes the global `correction` to every `solve()` call (both
  families); loading a saved design likewise solves with the current global.

## 4. Data flow

1. Startup: app reads `settings.toml` → `App.correction` (default Bergsträsser).
2. User edits a design → `recompute` calls `scenario.solve(material,
   App.correction)`; fatigue/optimize calls thread the same value.
3. User opens Settings, switches the factor → `App.correction` updates, the
   settings file is written, and the calculator recomputes with the new factor.
4. Saved designs reload and solve with the current global `correction`.

## 5. Error handling

- Settings load: missing/malformed file → default, no hard error (a non-fatal
  status note may be surfaced, consistent with the materials-overlay pattern).
- Settings save failure → surfaced as a status message; the in-memory preference
  still applies for the session.
- No new failure paths in the engine: `correction` is a total enum; `factor()` is
  defined for all indices the existing factors accept (callers already guard
  spring index > 1).

## 6. Testing

- **Engine unit tests:** `CurvatureCorrection::factor` dispatch (Wahl vs
  Bergsträsser at sample indices) and `Default` == Bergsträsser; `solve_forward`
  (both families) and `analyze_fatigue` produce the Wahl vs Bergsträsser stress
  for each selection.
- **Golden tightening:** pass each source's own convention explicitly so the
  body-shear assertions become near-exact —
  - Shigley 10-1 compression and Shigley 10-6 extension → `Bergstrasser`
    (tolerances drop from 2% to formula-exact);
  - Victory Spring comprehensive compression → `Wahl` (its published `Ka = 1.221`
    is the Wahl factor).
- **Presenter unit tests** (`settings_view_model`): hermetic, selection state →
  rendered options.
- **Simulator E2E:** navigate to Settings, switch the factor, return to the
  calculator, and assert the results recompute (a known design's body shear
  changes by the Wahl/Bergsträsser ratio).
- **Settings round-trip:** write a preference, read it back; malformed file →
  default.

## 7. Phasing

Two independently reviewable/shippable PRs:

1. **Engine (PR A):** `CurvatureCorrection` enum + threading through
   `solve_forward` (both families), `analyze_fatigue`, the optimizer, the
   `Scenario` trait, and `SavedDesign::solve`; default flip to Bergsträsser;
   golden tightening. No GUI. Self-contained: resolves the static/fatigue
   inconsistency and makes the goldens exact.
2. **GUI + settings (PR B):** the app-settings module + persistence, the Settings
   screen (presenter + view + nav), wiring the global into `recompute`, and the
   presenter + Simulator + round-trip tests. Builds on PR A.

## 8. Out of scope

- Per-design factor selection (deliberately a global preference).
- Other app preferences (the Settings screen is structured to host them later,
  but none are added now).
- Changing the hook curvature factors or any non-shear formula.
