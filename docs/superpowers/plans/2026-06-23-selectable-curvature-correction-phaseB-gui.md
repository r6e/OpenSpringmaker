# Selectable Curvature-Correction — Phase B (GUI + Settings) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a persisted, app-level curvature-correction preference and a Settings screen to `springmaker`, and thread the preference into every solve so the user's choice governs the calculator's static + fatigue results. This also re-fixes the `springmaker` call sites that Phase A's `springcore` signature changes broke, returning `cargo test --workspace` to green.

**Architecture:** A new `springmaker::settings` module persists `AppSettings { curvature_correction }` as `settings.toml` in the platform config dir (same `ProjectDirs` base as the materials overlay). `App` holds the live `correction`, initialized from that file; a new `Screen::Settings` (humble view + pure presenter per ADR 0008) lets the user change it, which persists and recomputes. `parse_and_solve`/`recompute` pass `App.correction` to the (Phase-A) correction-taking `springcore` APIs.

**Tech Stack:** Rust, `iced` 0.14 GUI, `iced_test` Simulator (headless, `ICED_TEST_BACKEND=tiny-skia`), `serde` + `toml` + `directories` (added to `springmaker`), `springcore` (Phase A).

## Global Constraints

- MSRV 1.88; dual MIT/Apache.
- Humble-view presenter standard (ADR 0008): all new GUI screens split a pure, iced-free presenter (unit-tested) from a humble view that only renders presenter output.
- Two-layer GUI testing: presenter unit tests (decisions) + `iced_test` Simulator E2E (wiring). Simulator runs headless under `ICED_TEST_BACKEND=tiny-skia` (set in CI).
- `cargo fmt`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `RUSTDOCFLAGS=-D warnings cargo doc`, repo-wide `typos`, `cargo deny check all`, `cargo mutants --in-diff` (springcore) all green before push.
- No commercial-product/vendor references in any persisted file.
- Default correction is **Bergsträsser**.
- Same branch as Phase A (`feat/selectable-curvature-correction`); A + B ship as **one PR**. Run the full `--workspace` gate at the end of Phase B (Phase A left the workspace red on purpose).
- New dependencies must pass `cargo deny` (license/advisory). `serde`, `toml`, `directories` are already in the workspace via `springcore`, so versions/licenses are already vetted.

---

## File Structure

- `springmaker/Cargo.toml` — **(modify)** add `serde` (derive), `toml`, `directories` deps.
- `springmaker/src/settings.rs` — **(create)** `AppSettings { curvature_correction }`, `settings_path()`, `load()`/`save()` + testable `load_from`/`save_to`.
- `springmaker/src/main.rs` — **(modify)** register `mod settings;`.
- `springmaker/src/app.rs` — **(modify)** `Screen::Settings`; `App.correction`; `Message::SetCorrection`; init from settings; `update` handling (persist + recompute); `recompute` passes correction; `view` routes Settings.
- `springmaker/src/form.rs` — **(modify)** `parse_and_solve(form, materials, correction)`; thread to `.solve`/`analyze_fatigue`/`solve_min_weight`.
- `springmaker/src/plot.rs` — **(modify)** the test-only `.solve(&m)` call site gains the correction arg.
- `springmaker/src/settings_view_model.rs` — **(create)** pure presenter for the Settings screen.
- `springmaker/src/settings_view.rs` — **(create)** humble view rendering the presenter output.
- `springmaker/src/view.rs` — **(modify)** add a "Settings →" nav button.
- `springmaker/src/ui_tests.rs` — **(modify)** Simulator E2E for the settings flow.

---

## Task B1: `AppSettings` persistence module

**Files:**
- Modify: `springmaker/Cargo.toml`
- Create: `springmaker/src/settings.rs`
- Modify: `springmaker/src/main.rs` (add `mod settings;`)

**Interfaces:**
- Produces: `pub struct AppSettings { pub curvature_correction: CurvatureCorrection }` with `Default`; `pub fn settings_path() -> Option<PathBuf>`; `pub fn load_from(path: &Path) -> AppSettings` (missing/malformed → default); `pub fn save_to(&self, path: &Path) -> std::io::Result<()>`; `pub fn load() -> AppSettings` and `pub fn save(&self) -> std::io::Result<()>` wrappers over `settings_path()`.

- [ ] **Step 1: Add deps** to `springmaker/Cargo.toml` under `[dependencies]`:

```toml
serde = { version = "1", features = ["derive"] }
toml = "0.8"
directories = "5"
```
(Match the exact versions `springcore/Cargo.toml` already uses — copy them verbatim so `cargo deny` and the lockfile stay consistent. Check `springcore/Cargo.toml` first.)

- [ ] **Step 2: Write the failing tests** in `springmaker/src/settings.rs` (create the file with the test module):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use springcore::CurvatureCorrection;

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("osm_settings_{}_{}.toml", name, std::process::id()))
    }

    #[test]
    fn round_trips() {
        let p = temp("round");
        AppSettings { curvature_correction: CurvatureCorrection::Wahl }
            .save_to(&p)
            .unwrap();
        assert_eq!(load_from(&p).curvature_correction, CurvatureCorrection::Wahl);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn missing_file_is_default() {
        let p = temp("missing");
        let _ = std::fs::remove_file(&p);
        assert_eq!(load_from(&p), AppSettings::default());
    }

    #[test]
    fn malformed_file_is_default() {
        let p = temp("malformed");
        std::fs::write(&p, "this is not = valid : toml ][").unwrap();
        assert_eq!(load_from(&p), AppSettings::default());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn default_is_bergstrasser() {
        assert_eq!(
            AppSettings::default().curvature_correction,
            CurvatureCorrection::Bergstrasser
        );
    }
}
```

- [ ] **Step 3: Run the tests — expect a compile failure** (module body not written)

Run: `cargo test -p springmaker --lib settings:: 2>&1 | tail -5`
Expected: compile error (`AppSettings`/`load_from`/`save_to` undefined).

- [ ] **Step 4: Implement the module** at the top of `springmaker/src/settings.rs`:

```rust
//! App-level preferences persisted as `settings.toml` in the platform config
//! directory (same base as the materials overlay). v1 holds only the curvature-
//! correction preference; the struct is the home for future preferences.

use serde::{Deserialize, Serialize};
use springcore::CurvatureCorrection;
use std::path::{Path, PathBuf};

/// Persisted application preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// Body-shear curvature-correction factor applied to all designs.
    pub curvature_correction: CurvatureCorrection,
}

/// Path to the settings file, or `None` if the platform config dir is unavailable.
pub fn settings_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("co", "r6e", "OpenSpringmaker")
        .map(|pd| pd.config_dir().join("settings.toml"))
}

/// Load settings from `path`; a missing or malformed file yields defaults.
pub fn load_from(path: &Path) -> AppSettings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

/// Persist settings to `path`, creating parent directories as needed.
pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
    // (free function form below — see note)
    unreachable!()
}
```

Note: implement `save_to`/`load`/`save` as methods/functions cleanly — the canonical bodies:

```rust
impl AppSettings {
    /// Load from the platform settings path (defaults if unavailable/malformed).
    pub fn load() -> Self {
        settings_path().map(|p| load_from(&p)).unwrap_or_default()
    }

    /// Persist to `path`, creating parent directories.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let toml = toml::to_string(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, toml)
    }

    /// Persist to the platform settings path; no-op error if unavailable.
    pub fn save(&self) -> std::io::Result<()> {
        match settings_path() {
            Some(p) => self.save_to(&p),
            None => Ok(()),
        }
    }
}
```
(Remove the placeholder free-function `save_to`; keep `load_from` as the free function the tests call, and `save_to` as the method the tests call via `AppSettings { .. }.save_to(&p)`.)

- [ ] **Step 5: Register the module** — add `mod settings;` to `springmaker/src/main.rs` (with the other `mod` declarations).

- [ ] **Step 6: Run the tests — expect pass**

Run: `cargo test -p springmaker --lib settings:: 2>&1 | tail -5`
Expected: `test result: ok. 4 passed`.

- [ ] **Step 7: fmt + clippy + deny**

Run: `cargo fmt && cargo clippy -p springmaker --all-targets -- -D warnings 2>&1 | tail -2 && cargo deny check all 2>&1 | tail -2`
Expected: clean (the three new deps are already vetted via springcore).

- [ ] **Step 8: Commit**

```bash
git add springmaker/Cargo.toml Cargo.lock springmaker/src/settings.rs springmaker/src/main.rs
git commit -m "feat(springmaker): persist app settings (curvature correction) to config dir"
```

---

## Task B2: `App` correction state + Settings screen plumbing (re-greens the workspace)

This task threads `App.correction` into the solve path, fixing the call sites Phase A broke. After it, `cargo build -p springmaker` compiles again.

**Files:**
- Modify: `springmaker/src/app.rs`
- Modify: `springmaker/src/form.rs`
- Modify: `springmaker/src/plot.rs`

**Interfaces:**
- Consumes: `springcore::CurvatureCorrection` (Phase A); `AppSettings` (B1).
- Produces: `App.correction: CurvatureCorrection`; `Message::SetCorrection(CurvatureCorrection)`; `Screen::Settings`; `parse_and_solve(form, materials, correction) -> Result<FormOutcome>`.

- [ ] **Step 1: Write the failing test** in `springmaker/src/app.rs` `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn changing_correction_recomputes_with_new_factor() {
        let mut app = App::from_store(
            MaterialStore::new(MaterialSet::load_default()),
            Vec::new(),
        );
        // A valid PowerUser design (C=10).
        app.form = crate::form::tests::valid_power_user_form(); // reuse existing helper if present; else inline
        app.update(Message::SetCorrection(springcore::CurvatureCorrection::Bergstrasser));
        let berg = app.outcome.as_ref().unwrap().design.load_points[0].shear_stress.pascals();
        app.update(Message::SetCorrection(springcore::CurvatureCorrection::Wahl));
        let wahl = app.outcome.as_ref().unwrap().design.load_points[0].shear_stress.pascals();
        assert!(wahl > berg, "Wahl factor exceeds Bergsträsser at C=10");
        assert_eq!(app.correction, springcore::CurvatureCorrection::Wahl);
    }
```
(Before writing, check `app.rs`/`form.rs` tests for an existing valid-form constructor; if none, inline a valid `FormState` like the existing `recompute_produces_outcome_for_valid_form` test uses. Match the real field name for the solved outcome — confirm whether it is `app.outcome` and the design accessor.)

- [ ] **Step 2: Run it — expect compile failure** (`SetCorrection`, `App.correction` undefined)

Run: `cargo test -p springmaker --lib changing_correction_recomputes 2>&1 | tail -5`
Expected: compile errors.

- [ ] **Step 3: Add `Screen::Settings`** to the `Screen` enum in `app.rs` (alongside `Calculator`, `Materials`).

- [ ] **Step 4: Add `App.correction`** field to the `App` struct, and initialize it in `from_store` (and via `Default`) from `crate::settings::AppSettings::load().curvature_correction`. (In `from_store`, set `correction: crate::settings::AppSettings::load().curvature_correction`.)

- [ ] **Step 5: Add `Message::SetCorrection(CurvatureCorrection)`** to the `Message` enum.

- [ ] **Step 6: Thread correction into `parse_and_solve`** in `form.rs`: change the signature to `pub fn parse_and_solve(form: &FormState, materials: &MaterialStore, correction: springcore::CurvatureCorrection) -> Result<FormOutcome>` and pass `correction` to the scenario `.solve(material, correction)`, `analyze_fatigue(.., correction)`, and `springcore::solve_min_weight(material, &req, correction)` calls inside it.

- [ ] **Step 7: Update `App::recompute`** in `app.rs` to pass the correction: `parse_and_solve(&self.form, &self.materials, self.correction)`.

- [ ] **Step 8: Handle the new messages** in `App::update`:
  - `Message::SetCorrection(c)`: set `self.correction = c;`, persist via `let _ = crate::settings::AppSettings { curvature_correction: c }.save();` (best-effort; a save failure must not crash the UI — optionally set a status note), and recompute. Add it to the `should_recompute` arm so the calculator refreshes (return `true`).
  - `Message::NavigateTo(Screen::Settings)`: already handled by the existing `NavigateTo(s) => { self.screen = s; matches!(s, Screen::Calculator) }` arm (Settings → no recompute). Confirm no extra handling needed.

- [ ] **Step 9: Route the Settings screen** in `App::view`: add `Screen::Settings => crate::settings_view::view(self),` (the view module lands in B3 — to keep this task compiling, temporarily route Settings to a placeholder `crate::view::view(self)` and switch it in B3, OR sequence B3's view file before this line; simplest: add the `settings_view` module stub in B3 and add this match arm there. For B2, leave `Screen::Settings` routed to the calculator view as a placeholder and note it.) **Decision: in B2 route `Screen::Settings => crate::view::view(self)` as a temporary placeholder; B3 replaces it with `settings_view::view`.**

- [ ] **Step 10: Fix the remaining Phase-A-broken call site** in `springmaker/src/plot.rs`: the test-only `.solve(&m)` (~line 280) becomes `.solve(&m, springcore::CurvatureCorrection::Bergstrasser)`.

- [ ] **Step 11: Update existing springmaker tests** that call `parse_and_solve(form, materials)` to pass a correction (use `CurvatureCorrection::Bergstrasser`). Grep: `grep -rn "parse_and_solve(" springmaker/src`.

- [ ] **Step 12: Run the springmaker suite + clippy**

Run: `cargo test -p springmaker 2>&1 | grep "test result:"` then `cargo clippy -p springmaker --all-targets -- -D warnings 2>&1 | tail -2`
Expected: all green; `changing_correction_recomputes_with_new_factor` passes.

- [ ] **Step 13: Commit**

```bash
git add springmaker/src/app.rs springmaker/src/form.rs springmaker/src/plot.rs
git commit -m "feat(springmaker): hold curvature-correction state and thread it through recompute"
```

---

## Task B3: Settings screen — presenter + view + nav

**Files:**
- Create: `springmaker/src/settings_view_model.rs`
- Create: `springmaker/src/settings_view.rs`
- Modify: `springmaker/src/main.rs` (`mod settings_view_model; mod settings_view;`)
- Modify: `springmaker/src/app.rs` (route `Screen::Settings => crate::settings_view::view(self)`)
- Modify: `springmaker/src/view.rs` (add "Settings →" nav button)

**Interfaces:**
- Consumes: `App.correction`, `Message::SetCorrection`, `Message::NavigateTo` (B2).
- Produces: `settings_view_model::SettingsViewModel` (pure) describing the screen; `settings_view::view(app) -> Element<Message>`.

- [ ] **Step 1: Write the presenter's failing test** in `springmaker/src/settings_view_model.rs` (create with test module). Mirror the shape of `materials_view_model.rs`.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use springcore::CurvatureCorrection;

    #[test]
    fn marks_the_active_correction_selected() {
        let vm = SettingsViewModel::from_correction(CurvatureCorrection::Wahl);
        let wahl = vm.options.iter().find(|o| o.value == CurvatureCorrection::Wahl).unwrap();
        let berg = vm.options.iter().find(|o| o.value == CurvatureCorrection::Bergstrasser).unwrap();
        assert!(wahl.selected);
        assert!(!berg.selected);
        // Bergsträsser is presented as the recommended/standard default.
        assert!(berg.label.contains("Bergsträsser"));
        assert!(wahl.label.contains("Wahl"));
    }

    #[test]
    fn offers_exactly_the_two_factors() {
        let vm = SettingsViewModel::from_correction(CurvatureCorrection::Bergstrasser);
        assert_eq!(vm.options.len(), 2);
    }
}
```

- [ ] **Step 2: Run — expect compile failure**

Run: `cargo test -p springmaker --lib settings_view_model:: 2>&1 | tail -5`
Expected: compile errors.

- [ ] **Step 3: Implement the pure presenter** in `springmaker/src/settings_view_model.rs`:

```rust
//! Pure presenter for the Settings screen (no iced). Decides what the settings
//! UI shows; `settings_view` renders it. (Humble-view standard, ADR 0008.)

use springcore::CurvatureCorrection;

/// One selectable correction option as the view should render it.
pub struct CorrectionOption {
    pub value: CurvatureCorrection,
    pub label: String,
    pub selected: bool,
}

/// Rendering decisions for the Settings screen.
pub struct SettingsViewModel {
    pub options: Vec<CorrectionOption>,
}

impl SettingsViewModel {
    /// Build the view model from the currently-active correction.
    pub fn from_correction(active: CurvatureCorrection) -> Self {
        let mk = |value, label: &str| CorrectionOption {
            value,
            label: label.to_string(),
            selected: value == active,
        };
        Self {
            options: vec![
                mk(CurvatureCorrection::Bergstrasser, "Bergsträsser (EN 13906-1 / Shigley default)"),
                mk(CurvatureCorrection::Wahl, "Wahl"),
            ],
        }
    }
}
```

- [ ] **Step 4: Run the presenter tests — expect pass**

Run: `cargo test -p springmaker --lib settings_view_model:: 2>&1 | tail -5`
Expected: `test result: ok. 2 passed`.

- [ ] **Step 5: Implement the humble view** in `springmaker/src/settings_view.rs`. Render the presenter output: a back nav ("← Calculator" → `Message::NavigateTo(Screen::Calculator)`), a heading, and the correction selector. Use the codebase's existing widget style helpers (mirror `materials_view.rs`). The selector emits `Message::SetCorrection(option.value)` — a `radio` group or a `pick_list` over the two options is fine; render each `CorrectionOption` with its `label` and `selected` state. Keep the view free of decisions: build it from `SettingsViewModel::from_correction(app.correction)`.

```rust
//! Humble view for the Settings screen — renders SettingsViewModel only.
use crate::app::{App, Message, Screen};
use crate::settings_view_model::SettingsViewModel;
use iced::Element;
// ... use the same widget/style imports as materials_view.rs ...

pub fn view(app: &App) -> Element<'_, Message> {
    let vm = SettingsViewModel::from_correction(app.correction);
    // back button -> Message::NavigateTo(Screen::Calculator)
    // heading "Settings"
    // for each vm.options: a radio/pick_list entry emitting Message::SetCorrection(opt.value),
    //   showing opt.label, checked when opt.selected
    // assemble into a column; return as Element
    todo!("assemble per materials_view.rs patterns")
}
```
(Replace the `todo!` with the actual iced widget tree following `materials_view.rs`. The view has no logic to unit-test; it is covered by the Simulator E2E in B4.)

- [ ] **Step 6: Register modules + route + nav.**
  - `springmaker/src/main.rs`: add `mod settings_view_model;` and `mod settings_view;`.
  - `app.rs` `view`: replace the B2 placeholder with `Screen::Settings => crate::settings_view::view(self),`.
  - `view.rs`: add a "Settings →" nav button next to "Materials →": `button(text("Settings →")...).on_press(Message::NavigateTo(crate::app::Screen::Settings))` with `nav_button_style`.

- [ ] **Step 7: Run the springmaker suite + clippy + doc**

Run: `cargo test -p springmaker 2>&1 | grep "test result:"`; `cargo clippy -p springmaker --all-targets -- -D warnings 2>&1 | tail -2`; `RUSTDOCFLAGS=-D warnings cargo doc -p springmaker --no-deps 2>&1 | tail -1`
Expected: all green, no warnings.

- [ ] **Step 8: Commit**

```bash
git add springmaker/src/settings_view_model.rs springmaker/src/settings_view.rs springmaker/src/main.rs springmaker/src/app.rs springmaker/src/view.rs
git commit -m "feat(springmaker): Settings screen for the curvature-correction preference"
```

---

## Task B4: Simulator E2E + full-workspace gate

**Files:**
- Modify: `springmaker/src/ui_tests.rs`

**Interfaces:**
- Consumes: the full wired feature (B1–B3).

- [ ] **Step 1: Write the Simulator E2E test** in `springmaker/src/ui_tests.rs`, mirroring the existing helpers (`test_app`, `click`, `type_into`, `shows`). It should: open the calculator, type a valid design, capture that results render; navigate to Settings; select Wahl; navigate back; confirm the design still solves (and, if a stable selector exists, that a result value changed). Keep selectors text/id-based per the existing tests.

```rust
#[test]
fn settings_changes_correction_and_recomputes() {
    let mut app = test_app();
    // Enter a valid PowerUser design (reuse the field-by-field pattern from
    // `typing_a_valid_power_user_design_renders_results`).
    type_into(&mut app, Field::WireDia, "2.0");
    type_into(&mut app, Field::MeanDia, "20.0");
    type_into(&mut app, Field::Active, "10");
    type_into(&mut app, Field::FreeLength, "60");
    type_into(&mut app, Field::Loads, "10, 30");
    assert!(shows(&app, "Spring rate"));
    let before = app.outcome.as_ref().unwrap().design.load_points[0].shear_stress.pascals();

    // Go to Settings, pick Wahl, come back.
    click(&mut app, "Settings →");
    click(&mut app, "Wahl");
    assert_eq!(app.correction, springcore::CurvatureCorrection::Wahl);
    click(&mut app, "← Calculator");

    let after = app.outcome.as_ref().unwrap().design.load_points[0].shear_stress.pascals();
    assert!(after > before, "Wahl raises body shear vs the Bergsträsser default at C=10");
}
```
(Adjust the result accessor to the real field names confirmed in B2. If clicking a `pick_list` option by label is awkward in the Simulator, use a `radio` in B3's view so each option is a distinct clickable labeled widget — choose the widget in B3 with this test in mind.)

- [ ] **Step 2: Run the E2E test (headless backend)**

Run: `ICED_TEST_BACKEND=tiny-skia cargo test -p springmaker --lib settings_changes_correction_and_recomputes 2>&1 | tail -8`
Expected: PASS.

- [ ] **Step 3: Full workspace gate** (Phase A + B now complete; the workspace must be green)

Run, expecting all green:
```bash
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
typos
cargo deny check all
```

- [ ] **Step 4: Mutation gate** (springcore in-diff)

Run: `git diff main...HEAD -- springcore/ > /tmp/cc.diff && cargo mutants --in-diff /tmp/cc.diff -p springcore 2>&1 | tail -5`
Expected: 0 survivors (Phase A's `factor()` + threaded calls are covered; add a pinning test if any survive).

- [ ] **Step 5: Commit**

```bash
git add springmaker/src/ui_tests.rs
git commit -m "test(springmaker): Simulator E2E for the Settings correction toggle"
```

---

## Self-review notes

- **Spec coverage:** app-settings persistence (spec §3.2) → B1; `App` state + Settings screen + presenter/view + nav + threading into recompute (spec §3.3, §4) → B2/B3; Simulator E2E + presenter unit tests + settings round-trip (spec §6) → B1/B3/B4. Engine threading + golden tightening (spec §3.1) were Phase A.
- **Re-green ordering:** B2 is what makes `springmaker` compile again (it updates the call sites Phase A broke); the full `--workspace` gate is deliberately deferred to B4. Within B2, `cargo test -p springmaker` is the check (springcore already green from Phase A).
- **Type consistency:** `CurvatureCorrection` (springcore), `AppSettings.curvature_correction`, `App.correction`, `Message::SetCorrection`, `parse_and_solve(.., correction)`, `SettingsViewModel::from_correction` — names consistent across tasks. **Confirm the solved-design accessor** (`app.outcome…design.load_points[0].shear_stress`) against the real `FormOutcome`/`App` field names before writing B2/B4 tests; adjust uniformly if different.
- **Widget choice:** B3 should pick `radio` (distinct labeled clickable per option) so the B4 Simulator can click "Wahl" by label; note this dependency in B3.
