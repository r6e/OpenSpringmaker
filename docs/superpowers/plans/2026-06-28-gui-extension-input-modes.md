# Extension GUI Input Modes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the extension family's four remaining input modes (RateBased, Dimensional, TwoLoad, MinWeight) to the `springmaker` GUI by fanning out the proven compression scenario-picker pattern.

**Architecture:** A new family-local `ExtScenarioKind` enum + scenario pick-list drives per-mode branching in the extension presenter/form layer; the engine scenarios and min-weight optimizer already exist (`springcore::extension`), so the only `springcore` change is four additive `ExtScenarioSpec` persistence variants. Each mode is added as a self-contained vertical slice (persistence variant + form fields + match arms + tests) that keeps every match exhaustive and the app consistent.

**Tech Stack:** Rust (2-crate workspace: `springcore` engine + `springmaker` iced 0.14 GUI), serde/TOML persistence, `cargo mutants` gate, headless `Simulator` E2E.

## Global Constraints

- MSRV 1.88; iced 0.14; dual MIT/Apache; **SI canonical** in the engine, convert at the boundary.
- ADR 0008 presenter / humble-view split: pure presenter (`form.rs`, `view_model.rs`, no iced) + humble view (`view.rs`).
- No new engine formulas — the engine scenarios and optimizer already exist and are cited.
- One-way module boundary: `extension` never imports `compression`; both depend only on shared `presenter.rs`/`widgets.rs`/`form_helpers.rs` + `springcore`.
- All gates green before push: `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`, repo-wide `typos`, `cargo deny check all`, `cargo test --workspace`, and the springcore mutation gate (below) with **literal 0 survivors**.
- Mutation gate (CI parity, run from repo root): `git diff origin/main -- > /tmp/pr.diff && cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features`.
- No commercial-product/vendor references in any persisted file (Shigley / EN 13906 citations are allowed).
- No `#[allow(dead_code)]` or lint-suppression scaffolding. Our GUI enums (`ExtScenarioKind`) stay exhaustive. The engine's `ExtBindingConstraint` and `HookSpec` are `#[non_exhaustive]`, so a match on them from `springmaker` **must** include a trailing wildcard arm — this is language-mandated, not a style violation.
- Mandatory adversarial multi-agent review panel before push, cycling to convergence.

## File Structure

- `springcore/src/persistence.rs` — add four variants to `ExtScenarioSpec` (RateBased, Dimensional, TwoLoad, MinWeight), SI mm/N. No other engine change.
- `springmaker/src/extension/form.rs` — `ExtScenarioKind` enum + `ALL_EXT_SCENARIOS` + `Display`; `ExtFormState` gains `scenario` + the new per-mode string fields; `Field` gains the new variants; `resolve_hooks_spec`; `ExtMinWeightExtra`; `ExtFormOutcome.min_weight`; `parse_and_solve`/`build_spec`/`populate_from_spec`/`is_blank` become per-scenario matches.
- `springmaker/src/extension/view_model.rs` — `ext_inputs_view` branches per scenario; `ExtPopulatedResults` gains `min_weight: Option<Vec<ResultRow>>`; binding→label mapping.
- `springmaker/src/extension/view.rs` — scenario `styled_pick_list` in the Setup group; `ext_field_id` gains the new field ids; render the min-weight section.
- `springmaker/src/app.rs` — `Message::ExtScenario(ExtScenarioKind)` + `update` arm; `set_ext_field` arms for the new `Field` variants; import `ExtScenarioKind`.
- `springmaker/src/ui_tests.rs` — Simulator E2E switching scenarios.

Order: Task 1 establishes the scenario machinery + RateBased; Tasks 2–4 add one mode each (one new enum variant, kept exhaustive); Task 5 is the cross-mode E2E.

---

### Task 1: Scenario picker scaffolding + RateBased mode

Establishes `ExtScenarioKind`, the pick-list, the `Message`, and converts every PowerUser-only function in the extension form/presenter into a per-scenario `match` — then adds RateBased as the first new mode. After this task PowerUser is unchanged and RateBased works end-to-end.

**Files:**
- Modify: `springcore/src/persistence.rs` (`ExtScenarioSpec` — add `RateBased`)
- Modify: `springmaker/src/extension/form.rs`
- Modify: `springmaker/src/extension/view_model.rs` (`ext_inputs_view`)
- Modify: `springmaker/src/extension/view.rs` (pick-list, `ext_field_id`)
- Modify: `springmaker/src/app.rs` (`Message`, `update`, `set_ext_field`, import)

**Interfaces:**
- Consumes: `springcore::extension::{RateBased, Scenario, HookEnds}`; `form_helpers::{rate_npm, length_mm, loads_n, non_negative_force_n, positive_num, fmt_len, fmt_force, fmt_loads, fmt_rate}`; `springcore::ExtScenarioSpec`.
- Produces: `extension::form::ExtScenarioKind` (`{ PowerUser, RateBased }` so far), `ALL_EXT_SCENARIOS`, `extension::form::Field::Rate`, `Message::ExtScenario(ExtScenarioKind)`, `ExtScenarioSpec::RateBased { wire_dia_mm, mean_dia_mm, rate_n_per_m, free_length_mm, initial_tension_n, hooks, loads_n }`.

- [ ] **Step 1: Add the `ExtScenarioSpec::RateBased` persistence variant + a failing round-trip test (springcore)**

In `springcore/src/persistence.rs`, add the variant to the `ExtScenarioSpec` enum (after `PowerUser`):

```rust
    RateBased {
        wire_dia_mm: f64,
        mean_dia_mm: f64,
        rate_n_per_m: f64,
        free_length_mm: f64,
        initial_tension_n: f64,
        hooks: HookSpecSpec,
        loads_n: Vec<f64>,
    },
```

Add this test to the existing `#[cfg(test)] mod tests` in `persistence.rs` (it round-trips through TOML and asserts the non-finite guard rejects an `inf` in the new variant):

```rust
    #[test]
    fn ext_ratebased_round_trips_through_toml() {
        let saved = SavedDesign {
            material: "Music Wire".into(),
            unit_system: UnitSystem::Metric,
            design: DesignSpec::Extension(ExtScenarioSpec::RateBased {
                wire_dia_mm: 2.0,
                mean_dia_mm: 20.0,
                rate_n_per_m: 2000.0,
                free_length_mm: 100.0,
                initial_tension_n: 5.0,
                hooks: HookSpecSpec::Default,
                loads_n: vec![10.0, 30.0],
            }),
        };
        let toml = saved.to_toml().unwrap();
        let back = SavedDesign::from_toml(&toml).unwrap();
        assert_eq!(saved, back);
    }

    #[test]
    fn from_toml_rejects_non_finite_ratebased_rate() {
        let toml = r#"
material = "Music Wire"
unit_system = "Metric"
[design]
family = "Extension"
type = "RateBased"
wire_dia_mm = 2.0
mean_dia_mm = 20.0
rate_n_per_m = inf
free_length_mm = 100.0
initial_tension_n = 5.0
loads_n = [10.0, 30.0]
[design.hooks]
mode = "Default"
"#;
        assert!(matches!(
            SavedDesign::from_toml(toml),
            Err(SpringError::DataFile(_))
        ));
    }
```

- [ ] **Step 2: Run the springcore tests to verify they fail**

Run: `cargo test -p springcore ext_ratebased_round_trips_through_toml from_toml_rejects_non_finite_ratebased_rate`
Expected: the round-trip test passes once the variant compiles; if `populate_from_spec` in springmaker now fails to compile, that is expected and fixed in Step 5. Run springcore in isolation first — Expected: PASS for both (springcore has no exhaustive `ExtScenarioSpec` match that breaks).

> Note: `springcore`'s own code does not match `ExtScenarioSpec` exhaustively, so adding a variant compiles cleanly there. The exhaustive match lives in `springmaker::extension::form::populate_from_spec` and is updated in Step 5; until then `cargo test -p springmaker` will not build. Implement Steps 3–7 before running the workspace build.

- [ ] **Step 3: Add `ExtScenarioKind` + the new form field + `Field::Rate` (springmaker form)**

In `springmaker/src/extension/form.rs`, add near the top (after the imports), and extend `Field`:

```rust
/// Which extension input scenario is active. The extension family's own enum
/// (not compression's `ScenarioKind`) — the module boundary forbids importing
/// compression, and the per-mode field sets and solve paths differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExtScenarioKind {
    #[default]
    PowerUser,
    RateBased,
}

/// All `ExtScenarioKind` variants in display order.
pub const ALL_EXT_SCENARIOS: &[ExtScenarioKind] = &[
    ExtScenarioKind::PowerUser,
    ExtScenarioKind::RateBased,
];

impl std::fmt::Display for ExtScenarioKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtScenarioKind::PowerUser => write!(f, "Power User"),
            ExtScenarioKind::RateBased => write!(f, "Rate Based"),
        }
    }
}
```

Add `Rate` to the `Field` enum (after `Loads`, before `HookR1`):

```rust
    Rate,
```

Add `scenario` and `rate` to `ExtFormState` (add `pub scenario: ExtScenarioKind,` as the first field, and `pub rate: String,` after `loads`), and to its `Default` impl (`scenario: ExtScenarioKind::default(),` and `rate: String::new(),`). Update the `import` line to bring in the engine types and helpers used below:

```rust
use crate::form_helpers::{
    fmt_force, fmt_len, fmt_loads, fmt_rate, length_mm, loads_n, non_negative_force_n,
    positive_num, rate_npm,
};
use springcore::extension::{ExtensionDesign, HookEnds, PowerUser, RateBased, Scenario};
```

- [ ] **Step 4: Convert `parse_and_solve` to a per-scenario match (springmaker form)**

Replace the body of `parse_and_solve` in `extension/form.rs` with the scenario match. The hook resolution stays mean-parameterized (`resolve_hooks` already takes `mean_dia_mm`):

```rust
pub fn parse_and_solve(
    form: &ExtFormState,
    material_name: &str,
    us: UnitSystem,
    materials: &MaterialStore,
    correction: CurvatureCorrection,
) -> Result<ExtFormOutcome> {
    let material: &Material = materials.get(material_name)?;
    match form.scenario {
        ExtScenarioKind::PowerUser => {
            let mean_dia_mm = length_mm("mean diameter", &form.mean_dia, us)?;
            let hooks = resolve_hooks(form, mean_dia_mm, us)?;
            let scenario = PowerUser {
                wire_dia: Length::from_millimeters(length_mm("wire diameter", &form.wire_dia, us)?),
                mean_dia: Length::from_millimeters(mean_dia_mm),
                active: positive_num("active coils", &form.active)?,
                free_length: Length::from_millimeters(length_mm(
                    "free length",
                    &form.free_length,
                    us,
                )?),
                initial_tension: Force::from_newtons(non_negative_force_n(
                    "initial tension",
                    &form.initial_tension,
                    us,
                )?),
                hooks,
                loads: loads_n(&form.loads, us)?
                    .into_iter()
                    .map(Force::from_newtons)
                    .collect(),
            };
            Ok(ExtFormOutcome {
                design: scenario.solve(material, correction)?,
            })
        }
        ExtScenarioKind::RateBased => {
            let mean_dia_mm = length_mm("mean diameter", &form.mean_dia, us)?;
            let hooks = resolve_hooks(form, mean_dia_mm, us)?;
            let scenario = RateBased {
                wire_dia: Length::from_millimeters(length_mm("wire diameter", &form.wire_dia, us)?),
                mean_dia: Length::from_millimeters(mean_dia_mm),
                rate: springcore::units::SpringRate::from_newtons_per_meter(rate_npm(
                    "spring rate",
                    &form.rate,
                    us,
                )?),
                free_length: Length::from_millimeters(length_mm(
                    "free length",
                    &form.free_length,
                    us,
                )?),
                initial_tension: Force::from_newtons(non_negative_force_n(
                    "initial tension",
                    &form.initial_tension,
                    us,
                )?),
                hooks,
                loads: loads_n(&form.loads, us)?
                    .into_iter()
                    .map(Force::from_newtons)
                    .collect(),
            };
            Ok(ExtFormOutcome {
                design: scenario.solve(material, correction)?,
            })
        }
    }
}
```

(`ExtFormOutcome` stays `{ design: ExtensionDesign }` for now; the `min_weight` field is added in Task 4.)

- [ ] **Step 5: Convert `build_spec`, `populate_from_spec`, and `is_blank` to per-scenario matches (springmaker form)**

Replace `build_spec`:

```rust
pub fn build_spec(form: &ExtFormState, us: UnitSystem) -> Result<ExtScenarioSpec> {
    match form.scenario {
        ExtScenarioKind::PowerUser => {
            let mean_dia_mm = length_mm("mean diameter", &form.mean_dia, us)?;
            Ok(ExtScenarioSpec::PowerUser {
                wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
                mean_dia_mm,
                active: positive_num("active coils", &form.active)?,
                free_length_mm: length_mm("free length", &form.free_length, us)?,
                initial_tension_n: non_negative_force_n("initial tension", &form.initial_tension, us)?,
                hooks: build_hooks_spec(form, us)?,
                loads_n: loads_n(&form.loads, us)?,
            })
        }
        ExtScenarioKind::RateBased => Ok(ExtScenarioSpec::RateBased {
            wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
            mean_dia_mm: length_mm("mean diameter", &form.mean_dia, us)?,
            rate_n_per_m: rate_npm("spring rate", &form.rate, us)?,
            free_length_mm: length_mm("free length", &form.free_length, us)?,
            initial_tension_n: non_negative_force_n("initial tension", &form.initial_tension, us)?,
            hooks: build_hooks_spec(form, us)?,
            loads_n: loads_n(&form.loads, us)?,
        }),
    }
}
```

Extract the hook-spec construction (used by every mode's `build_spec`) into a helper next to `resolve_hooks`:

```rust
/// Build the persisted hook spec from the form's hook mode (shared by every scenario).
fn build_hooks_spec(form: &ExtFormState, us: UnitSystem) -> Result<HookSpecSpec> {
    Ok(match form.hook_mode {
        HookMode::Default => HookSpecSpec::Default,
        HookMode::Custom => HookSpecSpec::Custom {
            r1_mm: length_mm("hook radius r1", &form.hook_r1, us)?,
            r2_mm: length_mm("hook radius r2", &form.hook_r2, us)?,
        },
    })
}
```

Replace `populate_from_spec` with a match over the spec (apply the hooks/scenario together):

```rust
pub fn populate_from_spec(form: &mut ExtFormState, spec: &ExtScenarioSpec, us: UnitSystem) {
    match spec {
        ExtScenarioSpec::PowerUser {
            wire_dia_mm,
            mean_dia_mm,
            active,
            free_length_mm,
            initial_tension_n,
            hooks,
            loads_n,
        } => {
            form.scenario = ExtScenarioKind::PowerUser;
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.mean_dia = fmt_len(*mean_dia_mm, us);
            form.active = format!("{active}");
            form.free_length = fmt_len(*free_length_mm, us);
            form.initial_tension = fmt_force(*initial_tension_n, us);
            form.loads = fmt_loads(loads_n, us);
            apply_hooks_spec(form, hooks, us);
        }
        ExtScenarioSpec::RateBased {
            wire_dia_mm,
            mean_dia_mm,
            rate_n_per_m,
            free_length_mm,
            initial_tension_n,
            hooks,
            loads_n,
        } => {
            form.scenario = ExtScenarioKind::RateBased;
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.mean_dia = fmt_len(*mean_dia_mm, us);
            form.rate = fmt_rate(*rate_n_per_m, us);
            form.free_length = fmt_len(*free_length_mm, us);
            form.initial_tension = fmt_force(*initial_tension_n, us);
            form.loads = fmt_loads(loads_n, us);
            apply_hooks_spec(form, hooks, us);
        }
    }
}
```

Extract the hook-applying half of the old `populate_from_spec` into a shared helper:

```rust
/// Apply a persisted hook spec back onto the form (shared by every scenario).
fn apply_hooks_spec(form: &mut ExtFormState, hooks: &HookSpecSpec, us: UnitSystem) {
    match hooks {
        HookSpecSpec::Default => {
            form.hook_mode = HookMode::Default;
            form.hook_r1 = String::new();
            form.hook_r2 = String::new();
        }
        HookSpecSpec::Custom { r1_mm, r2_mm } => {
            form.hook_mode = HookMode::Custom;
            form.hook_r1 = fmt_len(*r1_mm, us);
            form.hook_r2 = fmt_len(*r2_mm, us);
        }
    }
}
```

Replace `is_blank` with a per-scenario match (factor the hooks check, since every mode shares it):

```rust
    pub fn is_blank(&self) -> bool {
        let all_empty = |fields: &[&String]| fields.iter().all(|f| f.trim().is_empty());
        let hooks_blank = match self.hook_mode {
            HookMode::Default => true,
            HookMode::Custom => self.hook_r1.trim().is_empty() && self.hook_r2.trim().is_empty(),
        };
        let core_blank = match self.scenario {
            ExtScenarioKind::PowerUser => all_empty(&[
                &self.wire_dia,
                &self.mean_dia,
                &self.active,
                &self.free_length,
                &self.initial_tension,
                &self.loads,
            ]),
            ExtScenarioKind::RateBased => all_empty(&[
                &self.wire_dia,
                &self.mean_dia,
                &self.rate,
                &self.free_length,
                &self.initial_tension,
                &self.loads,
            ]),
        };
        core_blank && hooks_blank
    }
```

Bring `HookSpecSpec` into scope by adding it to the `use springcore::{...}` line in `form.rs` (it is already partly imported; ensure `HookSpecSpec` is listed).

- [ ] **Step 6: Make `ext_inputs_view` scenario-aware (springmaker view_model)**

Replace `ext_inputs_view` in `extension/view_model.rs`:

```rust
pub fn ext_inputs_view(app: &App) -> Vec<FieldDescriptor<Field>> {
    let us = app.unit_system;
    let len = unit_length_label(us);
    let force = unit_force_label(us);
    let rate = crate::presenter::unit_rate_label(us);
    let wire = FieldDescriptor::new(format!("Wire diameter ({len})"), Field::WireDia);
    let mean = FieldDescriptor::new(format!("Mean diameter ({len})"), Field::MeanDia);
    let free_length = FieldDescriptor::new(format!("Free length ({len})"), Field::FreeLength);
    let initial_tension =
        FieldDescriptor::new(format!("Initial tension ({force})"), Field::InitialTension);
    let loads = FieldDescriptor::new(format!("Loads ({force}), comma-separated"), Field::Loads);
    match app.extension.scenario {
        ExtScenarioKind::PowerUser => vec![
            wire,
            mean,
            FieldDescriptor::new("Active coils".to_string(), Field::Active),
            free_length,
            initial_tension,
            loads,
        ],
        ExtScenarioKind::RateBased => vec![
            wire,
            mean,
            FieldDescriptor::new(format!("Spring rate ({rate})"), Field::Rate),
            free_length,
            initial_tension,
            loads,
        ],
    }
}
```

Add `use crate::extension::form::ExtScenarioKind;` to the `view_model.rs` imports (alongside the existing `Field` import).

- [ ] **Step 7: Add the scenario pick-list + `ext_field_id` entry (springmaker view) and the `Message`/`update`/`set_ext_field` wiring (app)**

In `extension/view.rs`, add to the imports: `field_label` and `styled_pick_list` from `crate::widgets`, and `ALL_EXT_SCENARIOS` + `ExtScenarioKind` from `crate::extension::form`. Replace the `setup_group` in `design_panel` so it includes the scenario pick-list:

```rust
    let setup_group = column![
        section_heading("Setup"),
        crate::widgets::material_picker(app),
        column![
            field_label("Scenario"),
            styled_pick_list(
                ALL_EXT_SCENARIOS,
                Some(app.extension.scenario),
                Message::ExtScenario
            ),
        ]
        .spacing(4),
    ]
    .spacing(10);
```

Add the `Rate` arm to `ext_field_id` (keep it exhaustive):

```rust
        Field::Rate => "ext-rate",
```

In `app.rs`: add `ExtScenarioKind` to the `use crate::extension::form::{...}` import; add the message variant to `Message` (after `ExtHookMode`):

```rust
    ExtScenario(crate::extension::form::ExtScenarioKind),
```

Add the `update` arm (after the `ExtHookMode` arm):

```rust
            Message::ExtScenario(s) => {
                self.extension.scenario = s;
                true
            }
```

Add the `Rate` arm to `set_ext_field` (before `HookR1`):

```rust
            EF::Rate => f.rate = value,
```

- [ ] **Step 8: Add the RateBased form-layer tests (springmaker)**

Add to the `#[cfg(test)] mod tests` in `extension/form.rs`:

```rust
    fn ratebased_metric_form() -> ExtFormState {
        ExtFormState {
            scenario: ExtScenarioKind::RateBased,
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            rate: "2".into(), // 2 N/mm
            free_length: "100".into(),
            initial_tension: "5".into(),
            loads: "10, 30".into(),
            ..ExtFormState::default()
        }
    }

    #[test]
    fn ratebased_solves_and_rate_matches_input() {
        let materials = default_materials();
        let out = parse_and_solve(
            &ratebased_metric_form(),
            default_material_name(),
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .expect("RateBased should solve");
        // The solved rate must reproduce the 2 N/mm = 2000 N/m input.
        assert_relative_eq!(out.design.rate.newtons_per_meter(), 2000.0, epsilon = 1.0);
    }

    #[test]
    fn ratebased_build_spec_populate_round_trip() {
        let us = UnitSystem::Metric;
        let form = ratebased_metric_form();
        let spec = build_spec(&form, us).unwrap();
        let mut form2 = ExtFormState::default();
        populate_from_spec(&mut form2, &spec, us);
        assert_eq!(form2.scenario, ExtScenarioKind::RateBased);
        assert_eq!(build_spec(&form2, us).unwrap(), spec);
    }

    #[test]
    fn is_blank_ratebased_trips_on_rate() {
        let mut f = ExtFormState {
            scenario: ExtScenarioKind::RateBased,
            ..ExtFormState::default()
        };
        assert!(f.is_blank(), "untouched RateBased form is blank");
        f.rate = "2".into();
        assert!(!f.is_blank(), "entering the rate clears blank");
    }
```

Add to the `#[cfg(test)] mod tests` in `extension/view_model.rs`:

```rust
    #[test]
    fn inputs_view_ratebased_shows_rate_not_active() {
        let mut app = fresh_app();
        app.extension.scenario = crate::extension::form::ExtScenarioKind::RateBased;
        let fields = ext_inputs_view(&app);
        let kinds: Vec<Field> = fields.iter().map(|fd| fd.field).collect();
        assert!(kinds.contains(&Field::Rate), "RateBased shows the rate field");
        assert!(!kinds.contains(&Field::Active), "RateBased has no active-coils field");
    }
```

- [ ] **Step 9: Run the full workspace suite + gates**

Run: `cargo test --workspace`
Expected: PASS (all new tests green; existing 1b PowerUser tests unchanged).
Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean.
Run: `cargo fmt --all && RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
Expected: clean.
Run (mutation gate): `git diff origin/main -- > /tmp/pr.diff && cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features`
Expected: 0 survivors (the only springcore change is the additive `RateBased` variant; the round-trip + non-finite tests cover it).

- [ ] **Step 10: Commit**

```bash
git add springcore/src/persistence.rs springmaker/src/extension/form.rs springmaker/src/extension/view_model.rs springmaker/src/extension/view.rs springmaker/src/app.rs
git commit -m "feat(gui): extension scenario picker + RateBased input mode"
```

---

### Task 2: Dimensional mode

Adds the Dimensional mode (outer-diameter input). One new enum variant, one new persistence variant, one new form field, one arm in each match.

**Files:**
- Modify: `springcore/src/persistence.rs` (`ExtScenarioSpec` — add `Dimensional`)
- Modify: `springmaker/src/extension/form.rs`
- Modify: `springmaker/src/extension/view_model.rs`
- Modify: `springmaker/src/extension/view.rs`
- Modify: `springmaker/src/app.rs`

**Interfaces:**
- Consumes: `springcore::extension::Dimensional`; everything from Task 1.
- Produces: `ExtScenarioKind::Dimensional`, `Field::OuterDia`, `ExtScenarioSpec::Dimensional { wire_dia_mm, outer_dia_mm, active, free_length_mm, initial_tension_n, hooks, loads_n }`. For Default hooks, the hook mean diameter is `outer_dia_mm - wire_dia_mm`.

- [ ] **Step 1: Add the persistence variant + failing round-trip test (springcore)**

Add to `ExtScenarioSpec` in `persistence.rs`:

```rust
    Dimensional {
        wire_dia_mm: f64,
        outer_dia_mm: f64,
        active: f64,
        free_length_mm: f64,
        initial_tension_n: f64,
        hooks: HookSpecSpec,
        loads_n: Vec<f64>,
    },
```

Add the round-trip test (mirrors Task 1's, with `outer_dia_mm`):

```rust
    #[test]
    fn ext_dimensional_round_trips_through_toml() {
        let saved = SavedDesign {
            material: "Music Wire".into(),
            unit_system: UnitSystem::Metric,
            design: DesignSpec::Extension(ExtScenarioSpec::Dimensional {
                wire_dia_mm: 2.0,
                outer_dia_mm: 22.0,
                active: 10.0,
                free_length_mm: 100.0,
                initial_tension_n: 5.0,
                hooks: HookSpecSpec::Custom { r1_mm: 8.0, r2_mm: 4.0 },
                loads_n: vec![10.0, 30.0],
            }),
        };
        let back = SavedDesign::from_toml(&saved.to_toml().unwrap()).unwrap();
        assert_eq!(saved, back);
    }
```

- [ ] **Step 2: Run to verify it fails, then implement, then passes — springcore**

Run: `cargo test -p springcore ext_dimensional_round_trips_through_toml`
Expected after adding the variant: PASS. (springmaker won't build until Step 3 adds the `populate_from_spec` arm — implement Step 3 before the workspace build.)

- [ ] **Step 3: Add the form field, enum variant, and match arms (springmaker form)**

In `extension/form.rs`: add `Dimensional` to `ExtScenarioKind` and to `ALL_EXT_SCENARIOS` (after `RateBased`), add the `Display` arm `ExtScenarioKind::Dimensional => write!(f, "Dimensional"),`, add `OuterDia` to `Field` (after `Rate`), add `pub outer_dia: String,` to `ExtFormState` (after `mean_dia`) and `outer_dia: String::new(),` to its `Default`. Add `Dimensional` to the `use springcore::extension::{...}` import.

Add the `parse_and_solve` arm (note: hook mean = `outer_dia − wire_dia`):

```rust
        ExtScenarioKind::Dimensional => {
            let wire_dia_mm = length_mm("wire diameter", &form.wire_dia, us)?;
            let outer_dia_mm = length_mm("outer diameter", &form.outer_dia, us)?;
            let hooks = resolve_hooks(form, outer_dia_mm - wire_dia_mm, us)?;
            let scenario = Dimensional {
                wire_dia: Length::from_millimeters(wire_dia_mm),
                outer_dia: Length::from_millimeters(outer_dia_mm),
                active: positive_num("active coils", &form.active)?,
                free_length: Length::from_millimeters(length_mm(
                    "free length",
                    &form.free_length,
                    us,
                )?),
                initial_tension: Force::from_newtons(non_negative_force_n(
                    "initial tension",
                    &form.initial_tension,
                    us,
                )?),
                hooks,
                loads: loads_n(&form.loads, us)?
                    .into_iter()
                    .map(Force::from_newtons)
                    .collect(),
            };
            Ok(ExtFormOutcome {
                design: scenario.solve(material, correction)?,
            })
        }
```

Add the `build_spec` arm:

```rust
        ExtScenarioKind::Dimensional => Ok(ExtScenarioSpec::Dimensional {
            wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
            outer_dia_mm: length_mm("outer diameter", &form.outer_dia, us)?,
            active: positive_num("active coils", &form.active)?,
            free_length_mm: length_mm("free length", &form.free_length, us)?,
            initial_tension_n: non_negative_force_n("initial tension", &form.initial_tension, us)?,
            hooks: build_hooks_spec(form, us)?,
            loads_n: loads_n(&form.loads, us)?,
        }),
```

Add the `populate_from_spec` arm:

```rust
        ExtScenarioSpec::Dimensional {
            wire_dia_mm,
            outer_dia_mm,
            active,
            free_length_mm,
            initial_tension_n,
            hooks,
            loads_n,
        } => {
            form.scenario = ExtScenarioKind::Dimensional;
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.outer_dia = fmt_len(*outer_dia_mm, us);
            form.active = format!("{active}");
            form.free_length = fmt_len(*free_length_mm, us);
            form.initial_tension = fmt_force(*initial_tension_n, us);
            form.loads = fmt_loads(loads_n, us);
            apply_hooks_spec(form, hooks, us);
        }
```

Add the `is_blank` arm:

```rust
            ExtScenarioKind::Dimensional => all_empty(&[
                &self.wire_dia,
                &self.outer_dia,
                &self.active,
                &self.free_length,
                &self.initial_tension,
                &self.loads,
            ]),
```

- [ ] **Step 4: Add the inputs-view arm, pick-list coverage, id, and app wiring**

In `extension/view_model.rs` add the `ext_inputs_view` arm (outer diameter replaces mean):

```rust
        ExtScenarioKind::Dimensional => vec![
            wire,
            FieldDescriptor::new(format!("Outer diameter ({len})"), Field::OuterDia),
            FieldDescriptor::new("Active coils".to_string(), Field::Active),
            free_length,
            initial_tension,
            loads,
        ],
```

In `extension/view.rs` add the `ext_field_id` arm: `Field::OuterDia => "ext-outer-dia",`.
In `app.rs` add the `set_ext_field` arm: `EF::OuterDia => f.outer_dia = value,`.

- [ ] **Step 5: Add the Dimensional tests (springmaker)**

In `extension/form.rs` tests:

```rust
    #[test]
    fn dimensional_solves_with_outer_dia() {
        let materials = default_materials();
        let form = ExtFormState {
            scenario: ExtScenarioKind::Dimensional,
            wire_dia: "2".into(),
            outer_dia: "22".into(), // mean = 20
            active: "10".into(),
            free_length: "100".into(),
            initial_tension: "5".into(),
            loads: "10, 30".into(),
            ..ExtFormState::default()
        };
        let out = parse_and_solve(
            &form,
            default_material_name(),
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .expect("Dimensional should solve");
        // mean = OD - d = 20 mm → same geometry as the D=20 PowerUser case.
        assert_relative_eq!(out.design.outer_dia.millimeters(), 22.0, epsilon = 1e-6);
    }

    #[test]
    fn dimensional_round_trip_and_blank() {
        let us = UnitSystem::Metric;
        let form = ExtFormState {
            scenario: ExtScenarioKind::Dimensional,
            wire_dia: "2".into(),
            outer_dia: "22".into(),
            active: "10".into(),
            free_length: "100".into(),
            initial_tension: "5".into(),
            loads: "10".into(),
            ..ExtFormState::default()
        };
        let spec = build_spec(&form, us).unwrap();
        let mut form2 = ExtFormState::default();
        populate_from_spec(&mut form2, &spec, us);
        assert_eq!(form2.scenario, ExtScenarioKind::Dimensional);
        assert_eq!(build_spec(&form2, us).unwrap(), spec);

        let mut blank = ExtFormState {
            scenario: ExtScenarioKind::Dimensional,
            ..ExtFormState::default()
        };
        assert!(blank.is_blank());
        blank.outer_dia = "22".into();
        assert!(!blank.is_blank(), "Dimensional blank check uses outer_dia");
    }
```

- [ ] **Step 6: Run gates + commit**

Run: `cargo test --workspace` — Expected: PASS.
Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings` — Expected: clean.
Run mutation gate (as in Task 1 Step 9) — Expected: 0 survivors.

```bash
git add springcore/src/persistence.rs springmaker/src/extension/form.rs springmaker/src/extension/view_model.rs springmaker/src/extension/view.rs springmaker/src/app.rs
git commit -m "feat(gui): extension Dimensional input mode"
```

---

### Task 3: TwoLoad mode

Adds TwoLoad. **Asymmetry vs compression and vs the other extension modes:** no `initial_tension` input (it is derived: Fᵢ = F₁ − k·y₁), and `free_length` is required (anchors the deflections y = L − L₀). Engine struct: `point1`/`point2` are `(Force, Length)`.

**Files:** same five as Task 2.

**Interfaces:**
- Consumes: `springcore::extension::TwoLoad` (`{ wire_dia, mean_dia, free_length, hooks, point1: (Force, Length), point2: (Force, Length) }`).
- Produces: `ExtScenarioKind::TwoLoad`, `Field::{Force1, Length1, Force2, Length2}`, `ExtScenarioSpec::TwoLoad { wire_dia_mm, mean_dia_mm, free_length_mm, hooks, force1_n, length1_mm, force2_n, length2_mm }`.

- [ ] **Step 1: Add the persistence variant + round-trip test (springcore)**

Add to `ExtScenarioSpec`:

```rust
    TwoLoad {
        wire_dia_mm: f64,
        mean_dia_mm: f64,
        free_length_mm: f64,
        hooks: HookSpecSpec,
        force1_n: f64,
        length1_mm: f64,
        force2_n: f64,
        length2_mm: f64,
    },
```

Add the test:

```rust
    #[test]
    fn ext_twoload_round_trips_through_toml() {
        let saved = SavedDesign {
            material: "Music Wire".into(),
            unit_system: UnitSystem::Metric,
            design: DesignSpec::Extension(ExtScenarioSpec::TwoLoad {
                wire_dia_mm: 2.0,
                mean_dia_mm: 20.0,
                free_length_mm: 100.0,
                hooks: HookSpecSpec::Default,
                force1_n: 10.0,
                length1_mm: 110.0,
                force2_n: 30.0,
                length2_mm: 130.0,
            }),
        };
        let back = SavedDesign::from_toml(&saved.to_toml().unwrap()).unwrap();
        assert_eq!(saved, back);
    }
```

- [ ] **Step 2: Run to verify it passes once the variant compiles (springcore)**

Run: `cargo test -p springcore ext_twoload_round_trips_through_toml`
Expected after adding the variant + the springmaker `populate_from_spec` arm (Step 3): PASS.

- [ ] **Step 3: Add the form field set, enum variant, and match arms (springmaker form)**

Add `TwoLoad` to `ExtScenarioKind`, `ALL_EXT_SCENARIOS`, the `Display` arm `ExtScenarioKind::TwoLoad => write!(f, "Two Load"),`, the `Field` variants `Force1, Length1, Force2, Length2` (after `Loads`), the `ExtFormState` fields `pub force1: String,` `pub length1: String,` `pub force2: String,` `pub length2: String,` and their `String::new()` defaults. Add `TwoLoad` to the `use springcore::extension::{...}` import.

`parse_and_solve` arm (no initial tension; free length required):

```rust
        ExtScenarioKind::TwoLoad => {
            let mean_dia_mm = length_mm("mean diameter", &form.mean_dia, us)?;
            let hooks = resolve_hooks(form, mean_dia_mm, us)?;
            let scenario = TwoLoad {
                wire_dia: Length::from_millimeters(length_mm("wire diameter", &form.wire_dia, us)?),
                mean_dia: Length::from_millimeters(mean_dia_mm),
                free_length: Length::from_millimeters(length_mm(
                    "free length",
                    &form.free_length,
                    us,
                )?),
                hooks,
                point1: (
                    Force::from_newtons(non_negative_force_n("force 1", &form.force1, us)?),
                    Length::from_millimeters(length_mm("length 1", &form.length1, us)?),
                ),
                point2: (
                    Force::from_newtons(non_negative_force_n("force 2", &form.force2, us)?),
                    Length::from_millimeters(length_mm("length 2", &form.length2, us)?),
                ),
            };
            Ok(ExtFormOutcome {
                design: scenario.solve(material, correction)?,
            })
        }
```

`build_spec` arm:

```rust
        ExtScenarioKind::TwoLoad => Ok(ExtScenarioSpec::TwoLoad {
            wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
            mean_dia_mm: length_mm("mean diameter", &form.mean_dia, us)?,
            free_length_mm: length_mm("free length", &form.free_length, us)?,
            hooks: build_hooks_spec(form, us)?,
            force1_n: non_negative_force_n("force 1", &form.force1, us)?,
            length1_mm: length_mm("length 1", &form.length1, us)?,
            force2_n: non_negative_force_n("force 2", &form.force2, us)?,
            length2_mm: length_mm("length 2", &form.length2, us)?,
        }),
```

`populate_from_spec` arm:

```rust
        ExtScenarioSpec::TwoLoad {
            wire_dia_mm,
            mean_dia_mm,
            free_length_mm,
            hooks,
            force1_n,
            length1_mm,
            force2_n,
            length2_mm,
        } => {
            form.scenario = ExtScenarioKind::TwoLoad;
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.mean_dia = fmt_len(*mean_dia_mm, us);
            form.free_length = fmt_len(*free_length_mm, us);
            form.force1 = fmt_force(*force1_n, us);
            form.length1 = fmt_len(*length1_mm, us);
            form.force2 = fmt_force(*force2_n, us);
            form.length2 = fmt_len(*length2_mm, us);
            apply_hooks_spec(form, hooks, us);
        }
```

`is_blank` arm (no initial tension):

```rust
            ExtScenarioKind::TwoLoad => all_empty(&[
                &self.wire_dia,
                &self.mean_dia,
                &self.free_length,
                &self.force1,
                &self.length1,
                &self.force2,
                &self.length2,
            ]),
```

- [ ] **Step 4: Inputs-view arm, ids, app wiring**

`ext_inputs_view` arm in `view_model.rs` (no initial-tension field):

```rust
        ExtScenarioKind::TwoLoad => vec![
            wire,
            mean,
            free_length,
            FieldDescriptor::new(format!("Force 1 ({force})"), Field::Force1),
            FieldDescriptor::new(format!("Length 1 ({len})"), Field::Length1),
            FieldDescriptor::new(format!("Force 2 ({force})"), Field::Force2),
            FieldDescriptor::new(format!("Length 2 ({len})"), Field::Length2),
        ],
```

`ext_field_id` arms in `view.rs`:

```rust
        Field::Force1 => "ext-force1",
        Field::Length1 => "ext-length1",
        Field::Force2 => "ext-force2",
        Field::Length2 => "ext-length2",
```

`set_ext_field` arms in `app.rs`:

```rust
            EF::Force1 => f.force1 = value,
            EF::Length1 => f.length1 = value,
            EF::Force2 => f.force2 = value,
            EF::Length2 => f.length2 = value,
```

- [ ] **Step 5: TwoLoad tests (springmaker form + view_model)**

In `extension/form.rs` tests:

```rust
    fn twoload_metric_form() -> ExtFormState {
        // Two points 20 mm apart with a 20 N force delta → k = 1 N/mm = 1000 N/m.
        ExtFormState {
            scenario: ExtScenarioKind::TwoLoad,
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            free_length: "100".into(),
            force1: "10".into(),
            length1: "110".into(),
            force2: "30".into(),
            length2: "130".into(),
            ..ExtFormState::default()
        }
    }

    #[test]
    fn twoload_derives_rate_from_two_points() {
        let materials = default_materials();
        let out = parse_and_solve(
            &twoload_metric_form(),
            default_material_name(),
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .expect("TwoLoad should solve");
        // k = (30-10)/(130-110) mm = 1 N/mm = 1000 N/m.
        assert_relative_eq!(out.design.rate.newtons_per_meter(), 1000.0, epsilon = 1.0);
    }

    #[test]
    fn twoload_round_trip_and_blank_ignores_initial_tension() {
        let us = UnitSystem::Metric;
        let form = twoload_metric_form();
        let spec = build_spec(&form, us).unwrap();
        let mut form2 = ExtFormState::default();
        populate_from_spec(&mut form2, &spec, us);
        assert_eq!(form2.scenario, ExtScenarioKind::TwoLoad);
        assert_eq!(build_spec(&form2, us).unwrap(), spec);

        // initial_tension is NOT a TwoLoad input — filling it must not clear blank.
        let mut blank = ExtFormState {
            scenario: ExtScenarioKind::TwoLoad,
            initial_tension: "5".into(),
            ..ExtFormState::default()
        };
        assert!(blank.is_blank(), "initial tension is not a TwoLoad input");
        blank.force1 = "10".into();
        assert!(!blank.is_blank(), "a load point clears blank");
    }
```

In `extension/view_model.rs` tests:

```rust
    #[test]
    fn inputs_view_twoload_has_no_initial_tension() {
        let mut app = fresh_app();
        app.extension.scenario = crate::extension::form::ExtScenarioKind::TwoLoad;
        let kinds: Vec<Field> = ext_inputs_view(&app).iter().map(|fd| fd.field).collect();
        assert!(kinds.contains(&Field::Force1) && kinds.contains(&Field::Length2));
        assert!(
            !kinds.contains(&Field::InitialTension),
            "TwoLoad derives initial tension; it is not an input"
        );
    }
```

- [ ] **Step 6: Run gates + commit**

Run: `cargo test --workspace` — Expected: PASS.
Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings` — Expected: clean.
Run mutation gate — Expected: 0 survivors.

```bash
git add springcore/src/persistence.rs springmaker/src/extension/form.rs springmaker/src/extension/view_model.rs springmaker/src/extension/view.rs springmaker/src/app.rs
git commit -m "feat(gui): extension TwoLoad input mode"
```

---

### Task 4: MinWeight mode

Adds the optimizer mode. Unlike the forward modes it has a distinct solve path (`solve_min_weight`), an extra result section (binding constraint + mass), and uses `HookSpec` (not `HookEnds`). `ExtFormOutcome` gains `min_weight: Option<ExtMinWeightExtra>`.

**Files:** the five from Task 2 (plus the min-weight section in `view.rs`).

**Interfaces:**
- Consumes: `springcore::extension::{ExtMinWeightRequest, ExtMinWeightSolution, ExtBindingConstraint, HookSpec, solve_min_weight}` (all `#[non_exhaustive]` enums need a wildcard arm when matched), `springcore::units::{SpringRate, Force, Length}`.
- Produces: `ExtScenarioKind::MinWeight`, `Field::{MaxForce, CandidateDiameters, IndexMin, IndexMax, MaxOuterDia}`, `ExtMinWeightExtra { binding: ExtBindingConstraint, mass_kg: f64 }`, `ExtFormOutcome.min_weight`, `ExtScenarioSpec::MinWeight { required_rate_n_per_m, max_force_n, initial_tension_n, hooks, index_min, index_max, max_outer_dia_mm: Option<f64>, candidate_diameters_mm: Vec<f64> }`. The `rate` form field is reused as the required rate; `index_min`/`index_max` default to `"4"`/`"12"`. No clash-allowance (extension has no solid-height clash).

- [ ] **Step 1: Add the persistence variant + round-trip test (springcore)**

Add to `ExtScenarioSpec`:

```rust
    MinWeight {
        required_rate_n_per_m: f64,
        max_force_n: f64,
        initial_tension_n: f64,
        hooks: HookSpecSpec,
        index_min: f64,
        index_max: f64,
        max_outer_dia_mm: Option<f64>,
        candidate_diameters_mm: Vec<f64>,
    },
```

Add the test (covers `max_outer_dia_mm` both `None` and `Some`):

```rust
    #[test]
    fn ext_minweight_round_trips_both_max_outer_dia_states() {
        for max_od in [None, Some(30.0)] {
            let saved = SavedDesign {
                material: "Music Wire".into(),
                unit_system: UnitSystem::Metric,
                design: DesignSpec::Extension(ExtScenarioSpec::MinWeight {
                    required_rate_n_per_m: 2000.0,
                    max_force_n: 50.0,
                    initial_tension_n: 5.0,
                    hooks: HookSpecSpec::Default,
                    index_min: 4.0,
                    index_max: 12.0,
                    max_outer_dia_mm: max_od,
                    candidate_diameters_mm: vec![1.5, 2.0, 2.5],
                }),
            };
            let back = SavedDesign::from_toml(&saved.to_toml().unwrap()).unwrap();
            assert_eq!(saved, back);
        }
    }
```

- [ ] **Step 2: Verify the round-trip after the variant + springmaker arms compile**

Run: `cargo test -p springcore ext_minweight_round_trips_both_max_outer_dia_states`
Expected after Steps 3–4: PASS.

- [ ] **Step 3: `ExtFormOutcome.min_weight` + `ExtMinWeightExtra` + `resolve_hooks_spec` + the MinWeight solve branch (springmaker form)**

In `extension/form.rs`, extend the imports:

```rust
use springcore::extension::{
    solve_min_weight, Dimensional, ExtBindingConstraint, ExtMinWeightRequest, ExtensionDesign,
    HookEnds, HookSpec, PowerUser, RateBased, Scenario, TwoLoad,
};
use springcore::units::{Force, Length, SpringRate};
```

Add `MinWeight` to `ExtScenarioKind`, `ALL_EXT_SCENARIOS`, `Display` (`=> write!(f, "Min Weight")`). Add the `Field` variants `MaxForce, CandidateDiameters, IndexMin, IndexMax, MaxOuterDia`. Add `ExtFormState` fields and defaults:

```rust
    pub max_force: String,
    pub candidate_diameters: String,
    pub index_min: String,
    pub index_max: String,
    pub max_outer_dia: String,
```

Defaults: `max_force: String::new()`, `candidate_diameters: String::new()`, `index_min: "4".into()`, `index_max: "12".into()`, `max_outer_dia: String::new()`.

Add the `ExtMinWeightExtra` struct and extend `ExtFormOutcome`:

```rust
/// Extra outputs produced only by the extension Min-Weight optimisation path.
#[derive(Debug, Clone)]
pub struct ExtMinWeightExtra {
    pub binding: ExtBindingConstraint,
    pub mass_kg: f64,
}

/// A solved extension form: the design (which carries engine-computed status),
/// plus optimisation extras when the Min-Weight path produced it.
#[derive(Debug, Clone)]
pub struct ExtFormOutcome {
    pub design: ExtensionDesign,
    pub min_weight: Option<ExtMinWeightExtra>,
}
```

Every existing `Ok(ExtFormOutcome { design: ... })` in `parse_and_solve` (PowerUser, RateBased, Dimensional, TwoLoad) must now also set `min_weight: None`. Add the hook-spec resolver:

```rust
/// Resolve the form's hook mode into the optimiser's `HookSpec` (scaling Default,
/// or fixed radii). No mean diameter is needed — the optimiser varies D per candidate.
fn resolve_hooks_spec(form: &ExtFormState, us: UnitSystem) -> Result<HookSpec> {
    Ok(match form.hook_mode {
        HookMode::Default => HookSpec::Default,
        HookMode::Custom => HookSpec::Fixed {
            r1: Length::from_millimeters(length_mm("hook radius r1", &form.hook_r1, us)?),
            r2: Length::from_millimeters(length_mm("hook radius r2", &form.hook_r2, us)?),
        },
    })
}
```

Add the MinWeight arm to `parse_and_solve`'s match (parse the candidate-diameter list inline, like compression):

```rust
        ExtScenarioKind::MinWeight => {
            let candidate_diameters: Vec<Length> = form
                .candidate_diameters
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| Ok(Length::from_millimeters(length_mm("candidate diameter", s, us)?)))
                .collect::<Result<_>>()?;
            if candidate_diameters.is_empty() {
                return Err(springcore::SpringError::InconsistentInputs(
                    "provide at least one candidate wire diameter".into(),
                ));
            }
            let max_outer_dia = if form.max_outer_dia.trim().is_empty() {
                None
            } else {
                Some(Length::from_millimeters(length_mm(
                    "max outer diameter",
                    &form.max_outer_dia,
                    us,
                )?))
            };
            let req = ExtMinWeightRequest {
                required_rate: SpringRate::from_newtons_per_meter(rate_npm(
                    "required rate",
                    &form.rate,
                    us,
                )?),
                max_force: Force::from_newtons(non_negative_force_n(
                    "max force",
                    &form.max_force,
                    us,
                )?),
                initial_tension: Force::from_newtons(non_negative_force_n(
                    "initial tension",
                    &form.initial_tension,
                    us,
                )?),
                hooks: resolve_hooks_spec(form, us)?,
                index_bounds: (
                    positive_num("index min", &form.index_min)?,
                    positive_num("index max", &form.index_max)?,
                ),
                max_outer_dia,
                candidate_diameters,
            };
            let sol = solve_min_weight(material, &req, correction)?;
            Ok(ExtFormOutcome {
                design: sol.design,
                min_weight: Some(ExtMinWeightExtra {
                    binding: sol.binding,
                    mass_kg: sol.mass_kg,
                }),
            })
        }
```

Each forward arm parses `wire_dia` inline in its own struct literal (there is no shared top-level binding), so the MinWeight arm — which has no single wire diameter — never parses `form.wire_dia`. This is already how Tasks 1–3 wrote the forward arms; no change to them is needed here.

- [ ] **Step 4: MinWeight `build_spec`, `populate_from_spec`, `is_blank` arms (springmaker form)**

`build_spec` arm:

```rust
        ExtScenarioKind::MinWeight => {
            let candidate_diameters_mm: Vec<f64> = form
                .candidate_diameters
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| length_mm("candidate diameter", s, us))
                .collect::<Result<_>>()?;
            if candidate_diameters_mm.is_empty() {
                return Err(springcore::SpringError::InconsistentInputs(
                    "provide at least one candidate wire diameter".into(),
                ));
            }
            let max_outer_dia_mm = if form.max_outer_dia.trim().is_empty() {
                None
            } else {
                Some(length_mm("max outer diameter", &form.max_outer_dia, us)?)
            };
            Ok(ExtScenarioSpec::MinWeight {
                required_rate_n_per_m: rate_npm("required rate", &form.rate, us)?,
                max_force_n: non_negative_force_n("max force", &form.max_force, us)?,
                initial_tension_n: non_negative_force_n("initial tension", &form.initial_tension, us)?,
                hooks: build_hooks_spec(form, us)?,
                index_min: positive_num("index min", &form.index_min)?,
                index_max: positive_num("index max", &form.index_max)?,
                max_outer_dia_mm,
                candidate_diameters_mm,
            })
        }
```

`populate_from_spec` arm:

```rust
        ExtScenarioSpec::MinWeight {
            required_rate_n_per_m,
            max_force_n,
            initial_tension_n,
            hooks,
            index_min,
            index_max,
            max_outer_dia_mm,
            candidate_diameters_mm,
        } => {
            form.scenario = ExtScenarioKind::MinWeight;
            form.rate = fmt_rate(*required_rate_n_per_m, us);
            form.max_force = fmt_force(*max_force_n, us);
            form.initial_tension = fmt_force(*initial_tension_n, us);
            form.index_min = format!("{index_min}");
            form.index_max = format!("{index_max}");
            form.max_outer_dia = match max_outer_dia_mm {
                Some(v) => fmt_len(*v, us),
                None => String::new(),
            };
            form.candidate_diameters = candidate_diameters_mm
                .iter()
                .map(|&d| fmt_len(d, us))
                .collect::<Vec<_>>()
                .join(", ");
            apply_hooks_spec(form, hooks, us);
        }
```

`is_blank` arm — every displayed input clears blank except the pre-filled `index_*` defaults, so `rate`, `max_force`, `initial_tension`, `max_outer_dia`, and `candidate_diameters` all count. Only `max_outer_dia` is optional (valid-empty); it still counts because typing it signals intent — the same reason `loads` counts in the forward modes:

```rust
            ExtScenarioKind::MinWeight => all_empty(&[
                &self.rate,
                &self.max_force,
                &self.initial_tension,
                &self.max_outer_dia,
                &self.candidate_diameters,
            ]),
```

- [ ] **Step 5: MinWeight inputs view, results section, ids, app wiring**

`ext_inputs_view` — add a MinWeight early-return immediately after the `let len/force/rate` label bindings and **before** the shared `wire`/`mean` descriptors, mirroring compression:

```rust
    if app.extension.scenario == ExtScenarioKind::MinWeight {
        return vec![
            FieldDescriptor::new(format!("Required rate ({rate})"), Field::Rate),
            FieldDescriptor::new(format!("Max force ({force})"), Field::MaxForce),
            FieldDescriptor::new(format!("Initial tension ({force})"), Field::InitialTension),
            FieldDescriptor::new("Index min".to_string(), Field::IndexMin),
            FieldDescriptor::new("Index max".to_string(), Field::IndexMax),
            FieldDescriptor::new(
                format!("Max outer diameter ({len}, optional)"),
                Field::MaxOuterDia,
            ),
            FieldDescriptor::new(
                format!("Candidate wire diameters ({len}), comma-separated"),
                Field::CandidateDiameters,
            ),
        ];
    }
```

Because MinWeight is now handled by the early return, the `match app.extension.scenario` below must still stay exhaustive — add the arm (mirroring compression's `inputs_view`):

```rust
        ExtScenarioKind::MinWeight => unreachable!("MinWeight handled by the early return above"),
```

Add `min_weight: Option<Vec<ResultRow>>` to `ExtPopulatedResults` and populate it in `ext_results_view`. Add a binding-label helper (note the **mandatory wildcard** for the `#[non_exhaustive]` enum):

```rust
/// Min-weight optimisation result rows, or `None` when the active outcome is not
/// a Min-Weight solve.
fn ext_min_weight_rows(out: &crate::extension::form::ExtFormOutcome) -> Option<Vec<ResultRow>> {
    let mw = out.min_weight.as_ref()?;
    let binding = match mw.binding {
        ExtBindingConstraint::BodyShear => "body shear",
        ExtBindingConstraint::HookBending => "hook bending",
        ExtBindingConstraint::HookTorsion => "hook torsion",
        ExtBindingConstraint::Index => "index",
        ExtBindingConstraint::OuterDiameter => "outer diameter",
        // `ExtBindingConstraint` is `#[non_exhaustive]`; a future variant falls here.
        _ => "other",
    };
    Some(vec![
        ResultRow::new("Wire mass", format!("{:.4}", mw.mass_kg), "kg"),
        ResultRow::new("Binding constraint", binding, ""),
    ])
}
```

In `ext_results_view`'s `Populated` construction, set `min_weight: ext_min_weight_rows(out)`. Add `use springcore::extension::ExtBindingConstraint;` to `view_model.rs` imports.

In `extension/view.rs`, add `divided_result_section` to the `use crate::widgets::{...}` import, and replace the `ExtResultsView::Populated(p)` arm of `results_panel` (which currently builds an inline `column![...]`) with a mutable-column form that conditionally pushes the optimisation section:

```rust
        ExtResultsView::Populated(p) => {
            let mut col = column![
                section_heading("Results"),
                section_divider(),
                render_governing_rate(&p.governing_rate),
                section_divider(),
                rows_section("Geometry", &p.geometry),
                section_divider(),
                render_ext_load_table(&p.load_table),
            ]
            .spacing(6);
            if let Some(rows) = &p.min_weight {
                col = col.push(divided_result_section("Min-weight optimisation", rows));
            }
            col.into()
        }
```

Add the `ext_field_id` arms in `view.rs`:

```rust
        Field::MaxForce => "ext-max-force",
        Field::CandidateDiameters => "ext-candidate-diameters",
        Field::IndexMin => "ext-index-min",
        Field::IndexMax => "ext-index-max",
        Field::MaxOuterDia => "ext-max-outer-dia",
```

Add the `set_ext_field` arms in `app.rs`:

```rust
            EF::MaxForce => f.max_force = value,
            EF::CandidateDiameters => f.candidate_diameters = value,
            EF::IndexMin => f.index_min = value,
            EF::IndexMax => f.index_max = value,
            EF::MaxOuterDia => f.max_outer_dia = value,
```

- [ ] **Step 6: MinWeight tests (springmaker form + view_model)**

In `extension/form.rs` tests:

```rust
    fn minweight_metric_form() -> ExtFormState {
        ExtFormState {
            scenario: ExtScenarioKind::MinWeight,
            rate: "2".into(), // 2 N/mm required rate
            max_force: "50".into(),
            initial_tension: "5".into(),
            candidate_diameters: "1.5, 2.0, 2.5".into(),
            ..ExtFormState::default() // index_min="4", index_max="12" by default
        }
    }

    #[test]
    fn minweight_solves_with_binding_and_positive_mass() {
        let materials = default_materials();
        let out = parse_and_solve(
            &minweight_metric_form(),
            default_material_name(),
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .expect("MinWeight should solve");
        let mw = out.min_weight.expect("MinWeight path sets min_weight");
        assert!(mw.mass_kg > 0.0, "optimised wire mass is positive");
    }

    #[test]
    fn minweight_empty_candidates_errors() {
        let materials = default_materials();
        let form = ExtFormState {
            candidate_diameters: String::new(),
            ..minweight_metric_form()
        };
        assert!(parse_and_solve(
            &form,
            default_material_name(),
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .is_err());
    }

    #[test]
    fn minweight_round_trip_and_blank_ignores_prefilled_defaults() {
        let us = UnitSystem::Metric;
        let spec = build_spec(&minweight_metric_form(), us).unwrap();
        let mut form2 = ExtFormState::default();
        populate_from_spec(&mut form2, &spec, us);
        assert_eq!(form2.scenario, ExtScenarioKind::MinWeight);
        assert_eq!(build_spec(&form2, us).unwrap(), spec);

        // A default MinWeight form (index_min/max pre-filled) is still blank.
        let f = ExtFormState {
            scenario: ExtScenarioKind::MinWeight,
            ..ExtFormState::default()
        };
        assert!(f.is_blank(), "pre-filled index defaults do not count as input");
    }
```

In `extension/view_model.rs` tests:

```rust
    #[test]
    fn minweight_results_include_optimisation_section() {
        let materials = store();
        let mut app = fresh_app();
        let out = parse_and_solve(
            &{
                let mut f = ExtFormState::default();
                f.scenario = crate::extension::form::ExtScenarioKind::MinWeight;
                f.rate = "2".into();
                f.max_force = "50".into();
                f.initial_tension = "5".into();
                f.candidate_diameters = "1.5, 2.0, 2.5".into();
                f
            },
            "Music Wire",
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .unwrap();
        app.family = Family::Extension;
        app.ext_outcome = Some(out);
        let p = match ext_results_view(&app) {
            ExtResultsView::Populated(p) => *p,
            other => panic!("expected Populated, got {other:?}"),
        };
        let rows = p.min_weight.expect("MinWeight outcome shows the optimisation section");
        assert!(rows.iter().any(|r| r.label == "Wire mass"));
        assert!(rows.iter().any(|r| r.label == "Binding constraint"));
    }
```

- [ ] **Step 7: Run gates + commit**

Run: `cargo test --workspace` — Expected: PASS.
Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings` — Expected: clean (the `_ => "other"` wildcard silences the non-exhaustive match; no `dead_code`).
Run: `cargo fmt --all && RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` — Expected: clean.
Run mutation gate — Expected: 0 survivors (the `MinWeight` variant's serde + round-trip + None/Some coverage close it).

```bash
git add springcore/src/persistence.rs springmaker/src/extension/form.rs springmaker/src/extension/view_model.rs springmaker/src/extension/view.rs springmaker/src/app.rs
git commit -m "feat(gui): extension MinWeight optimisation mode"
```

---

### Task 5: Simulator E2E across scenarios

A headless `Simulator` end-to-end test that switches the extension scenario and solves each mode, plus a MinWeight run asserting the optimisation section renders. Reuses the `ext_field_id` ids as the single source of truth (the test resolves inputs through `ext_field_id`, never hardcoding strings).

**Files:**
- Modify: `springmaker/src/ui_tests.rs`

**Interfaces:**
- Consumes: the existing `ui_tests.rs` harness — `test_app() -> App`, `type_into_ext(&mut App, ExtField, &str)` (clicks the rendered widget by `ext_field_id`, then types), `app.update(Message)`, `ext_results_view`/`ExtResultsView` (already imported in the file). Add `ExtScenarioKind` to the existing `use crate::extension::form::{...}` import in the test module.

- [ ] **Step 1: Write the cross-scenario E2E test**

Add to `springmaker/src/ui_tests.rs`. The harness is a plain `App` from `test_app()` driven via `app.update(...)`; the scenario switch is a `Message::ExtScenario` dispatch, and `type_into_ext` focuses each field through `ext_field_id` (so the field must be rendered for the active scenario — which it is, because `ext_inputs_view` branches on the scenario):

```rust
    #[test]
    fn ext_scenario_switch_solves_each_mode() {
        let mut app = test_app();
        app.update(Message::SelectFamily(Family::Extension));

        // RateBased: rate + free length + loads → solves to a standard design.
        app.update(Message::ExtScenario(ExtScenarioKind::RateBased));
        type_into_ext(&mut app, ExtField::WireDia, "2");
        type_into_ext(&mut app, ExtField::MeanDia, "20");
        type_into_ext(&mut app, ExtField::Rate, "2");
        type_into_ext(&mut app, ExtField::FreeLength, "100");
        type_into_ext(&mut app, ExtField::InitialTension, "5");
        type_into_ext(&mut app, ExtField::Loads, "10, 30");
        assert!(
            matches!(ext_results_view(&app), ExtResultsView::Populated(_)),
            "RateBased should render results"
        );

        // MinWeight: required rate + max force + candidates → optimisation section.
        app.update(Message::ExtScenario(ExtScenarioKind::MinWeight));
        type_into_ext(&mut app, ExtField::Rate, "2");
        type_into_ext(&mut app, ExtField::MaxForce, "50");
        type_into_ext(&mut app, ExtField::CandidateDiameters, "1.5, 2.0, 2.5");
        match ext_results_view(&app) {
            ExtResultsView::Populated(p) => {
                assert!(p.min_weight.is_some(), "MinWeight shows the optimisation section");
            }
            other => panic!("expected Populated MinWeight results, got {other:?}"),
        }
    }
```

`ExtField` is the existing alias for `crate::extension::form::Field`; `Message`, `Family`, `ext_results_view`, and `ExtResultsView` are already in scope in the test module.

- [ ] **Step 2: Run the E2E test**

Run: `cargo test -p springmaker ext_scenario_switch_solves_each_mode`
Expected: PASS.

- [ ] **Step 3: Full suite + gates + commit**

Run: `cargo test --workspace` — Expected: PASS.
Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo fmt --all && RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` — Expected: clean.

```bash
git add springmaker/src/ui_tests.rs
git commit -m "test(gui): extension Simulator E2E across input modes"
```

---

## Notes for the implementer

- **Match exhaustiveness is the safety net.** Each task adds one `ExtScenarioKind` variant; the compiler forces you to add the arm in `parse_and_solve`, `build_spec`, `populate_from_spec`, `is_blank`, and `ext_inputs_view`. If the build complains about a non-exhaustive match, you missed one — that is the intended guard, not an error to suppress.
- **`ext_field_id` is the single source of truth for widget ids**, shared with the Simulator tests via `type_into_ext`. Add the new ids there only; never hardcode id strings in tests.
- **`fmt_rate` displays N/mm in metric**, matching `rate_npm`'s N/mm input — use it (not `fmt_force`) for the rate field round-trip.
- **The `#[non_exhaustive]` wildcard** in `ext_min_weight_rows` is required because `ExtBindingConstraint` lives in `springcore`; do not delete it to "tidy" the match — it will not compile without it.
- **Mean diameter for Default hooks** differs by mode: PowerUser/RateBased/TwoLoad use `form.mean_dia`; Dimensional uses `outer_dia − wire_dia`; MinWeight uses `HookSpec` (no mean).
