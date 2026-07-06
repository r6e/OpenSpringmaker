# Conical GUI Family Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The fourth GUI family tab — conical compression springs (PowerUser-only), wiring the merged engine (PR #57) into a full form → solve → results experience and retiring the placeholder load-rejection.

**Architecture:** Task 1 lands the springcore `Family::Conical` surface + the extended telescoping message, the complete form layer, ALL app dispatch arms, and MINIMAL view/view_model skeletons (inputs render; results show Empty/Error) so every exhaustive match compiles. Task 2 fleshes out the presenter (geometry table, load table, statuses, the linear-model footer), the full view, E2E, and replaces the placeholder tests.

**Tech Stack:** Rust workspace — springcore (mutation-gated) + springmaker (iced 0.14, ADR 0008 humble-view/presenter).

## Global Constraints

- springcore is mutation-gated: `cargo mutants --in-diff` vs origin/main ends with literal 0 missed. springmaker is NOT gated.
- Strict TDD; every string quoted in this plan is VERBATIM.
- NO references to the commercial spring-design product this project serves as an alternative to, nor its vendor (tooling-attribution trailers exempt and required).
- MSRV 1.88; fmt zero deviation; clippy `-D warnings` clean; ADR 0008 (no iced imports in form.rs/view_model.rs).
- Commit DIRECTLY on `feat/gui-conical`. NEVER push, NEVER create/edit PRs, NEVER run panels or dispatch subagents, NEVER touch anything under `.git/`.
- Conventional commits; every commit ends with:
  `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`
- Spec: docs/superpowers/specs/2026-07-06-gui-conical-design.md. Zero persisted-format changes (`ConicalSpec` ships untouched).

---

### Task 1: springcore surface + form layer + app wiring (+ minimal view skeleton)

**Files:**
- Modify: `springcore/src/family.rs` (variant + ALL + Display + tests)
- Modify: `springcore/src/conical/design.rs` (telescoping message + its pinned test)
- Modify: `springcore/src/persistence.rs:268` (`parse_end_type` → `pub`) and `springcore/src/lib.rs` (re-export)
- Create: `springmaker/src/conical/mod.rs`, `springmaker/src/conical/form.rs`, `springmaker/src/conical/view_model.rs` (skeleton), `springmaker/src/conical/view.rs` (skeleton)
- Modify: `springmaker/src/main.rs` or `lib.rs` (wherever sibling family modules are declared — `mod conical;` beside `mod torsion;`)
- Modify: `springmaker/src/app.rs` (state, Message, set_con_field, recompute, save_to, apply_saved + placeholder-test replacement part 1)
- Modify: `springmaker/src/calculator.rs:21-34, 127-131` (family dispatch arms)
- Modify: `springmaker/src/compression/view.rs:42` + the shared picker module (hoist `END_TYPES`)

**Interfaces:**
- Consumes: `springcore::conical::{ConicalInputs, ConicalDesign, solve_forward, evaluate_status}`, `springcore::ConicalSpec`, form_helpers (`length_mm`, `positive_num`, `loads_n`, `fmt_len`, `fmt_loads`), the newly-pub `springcore::parse_end_type`.
- Produces (Task 2 relies on these exact names): `ConFormState` (fields per Step 4), `Field` enum, `ConFormOutcome { pub design: ConicalDesign }`, `parse_and_solve(form, material_name, us, materials, correction) -> Result<ConFormOutcome>`, `build_spec(form, us) -> Result<ConicalSpec>`, `populate_from_spec(&mut form, &spec, us)`, `is_blank()`; app state `self.conical` / `self.con_outcome`; `Message::ConField(Field, String)`; skeleton `conical::view::{design_panel, results_panel}` and `conical::view_model::{ConResultsView, con_results_view, con_inputs_view, con_status_view}`; the hoisted `END_TYPES` const (pub(crate) in the shared picker module beside `KeyLabel`).

- [ ] **Step 1: springcore Family surface (TDD)**

Add the failing test to `springcore/src/family.rs`'s test module (the file's exact pattern — see `torsion_display_and_in_all_families`):

```rust
    #[test]
    fn conical_display_and_in_all_families() {
        assert_eq!(Family::Conical.to_string(), "Conical");
        assert!(ALL_FAMILIES.contains(&Family::Conical));
    }
```

Run: `cargo test -p springcore family` — FAIL (no variant). Implement: add `Conical` after `Torsion` in the enum, `Family::Conical => "Conical"` in Display, and extend the const:

```rust
pub const ALL_FAMILIES: &[Family] = &[
    Family::Compression,
    Family::Extension,
    Family::Torsion,
    Family::Conical,
];
```

NOTE: springmaker will NOT compile until the app arms land (Step 6) IF any springmaker match on `Family` is exhaustive — that's expected mid-task; run `cargo test -p springcore` only at this step.

- [ ] **Step 2: telescoping message extension (TDD)**

In `springcore/src/conical/design.rs`, update the pinned status test's expected string FIRST (locate `telescoping_info_present_only_when_telescoping` and any message-content assert; the needle `"coils telescope"` stays valid, but the spec requires the FULL new string pinned — update/extend the assert to pin the complete message):

```rust
        assert!(status.messages.iter().any(|msg| msg.severity == crate::design::Severity::Info
            && msg.message
                == "coils telescope (per-coil radial step ≥ wire diameter); the reported \
                    solid length is conservative — the true solid height is lower and the \
                    reported at-solid stress is correspondingly understated"));
```

Run — FAIL against the old message. Then update the implementation's message to, VERBATIM (one logical string; `\`-continuation collapses to single spaces):

```rust
            message: "coils telescope (per-coil radial step ≥ wire diameter); the reported \
                      solid length is conservative — the true solid height is lower and the \
                      reported at-solid stress is correspondingly understated"
                .into(),
```

Green. Also promote `parse_end_type` (`springcore/src/persistence.rs:268`) from private to `pub` with a doc comment (`/// Parse a persisted end-type key ("plain" | "plain_ground" | "squared" | "squared_ground").`) and add `parse_end_type` to the `pub use persistence::{...}` list in `springcore/src/lib.rs` — conical's GUI form needs the canonical key→enum mapping (compression's conversion happens inside `spec.solve`, a path that rejects conical).

- [ ] **Step 3: Commit springcore**

```bash
git add springcore/src/family.rs springcore/src/conical/design.rs springcore/src/persistence.rs springcore/src/lib.rs
git commit -m "feat(springcore): Family::Conical + telescoping stress caveat + pub parse_end_type

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

(springmaker may not compile at this commit if Family matches are exhaustive — acceptable intra-task; the workspace gate runs at Step 8.)

- [ ] **Step 4: the form layer (TDD — tests and implementation in `springmaker/src/conical/form.rs`)**

Create `springmaker/src/conical/mod.rs`:

```rust
//! Conical compression spring family — GUI layer (PowerUser scenario only,
//! matching the engine). Humble view / pure presenter per ADR 0008.

pub mod form;
pub mod view;
pub mod view_model;
```

Declare `mod conical;` in springmaker's module root beside the sibling `mod torsion;` declaration (same visibility as siblings).

Create `springmaker/src/conical/form.rs`. Write the test module FIRST (red = compile failure), then the implementation:

```rust
//! Conical form state, parsing, and solve routing (PowerUser only).
//! iced-free per ADR 0008.

use springcore::conical::{solve_forward, ConicalDesign, ConicalInputs};
use springcore::units::{Force, Length};
use springcore::{
    parse_end_type, ConicalSpec, CurvatureCorrection, MaterialStore, Result, UnitSystem,
};

use crate::form_helpers::{fmt_len, fmt_loads, length_mm, loads_n, positive_num};

/// A form input field (one per text input; the end-type picker is separate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    WireDia,
    LargeMeanDia,
    SmallMeanDia,
    Active,
    FreeLength,
    Loads,
}

/// Conical form state. `end_type` holds the persisted key
/// ("plain" | "plain_ground" | "squared" | "squared_ground").
#[derive(Debug, Clone)]
pub struct ConFormState {
    pub end_type: String,
    pub wire_dia: String,
    pub large_mean_dia: String,
    pub small_mean_dia: String,
    pub active: String,
    pub free_length: String,
    /// Comma-separated loads (compression's idiom).
    pub loads: String,
}

impl Default for ConFormState {
    fn default() -> Self {
        Self {
            end_type: "squared_ground".into(),
            wire_dia: String::new(),
            large_mean_dia: String::new(),
            small_mean_dia: String::new(),
            active: String::new(),
            free_length: String::new(),
            loads: String::new(),
        }
    }
}

impl ConFormState {
    /// Blank when every text input is empty (the end-type selector holds a
    /// default and does not count).
    pub fn is_blank(&self) -> bool {
        [
            &self.wire_dia,
            &self.large_mean_dia,
            &self.small_mean_dia,
            &self.active,
            &self.free_length,
            &self.loads,
        ]
        .iter()
        .all(|f| f.trim().is_empty())
    }
}

/// A successful conical solve.
#[derive(Debug, Clone)]
pub struct ConFormOutcome {
    pub design: ConicalDesign,
}

/// Parse the form and solve. Takes the app-global curvature correction (the
/// compression pattern — torsion's solver takes none; documented divergence).
pub fn parse_and_solve(
    form: &ConFormState,
    material_name: &str,
    us: UnitSystem,
    materials: &MaterialStore,
    correction: CurvatureCorrection,
) -> Result<ConFormOutcome> {
    let inputs = ConicalInputs {
        wire_dia: Length::from_millimeters(length_mm("wire diameter", &form.wire_dia, us)?),
        large_mean_dia: Length::from_millimeters(length_mm(
            "large mean diameter",
            &form.large_mean_dia,
            us,
        )?),
        small_mean_dia: Length::from_millimeters(length_mm(
            "small mean diameter",
            &form.small_mean_dia,
            us,
        )?),
        active_coils: positive_num("active coils", &form.active)?,
        free_length: Length::from_millimeters(length_mm("free length", &form.free_length, us)?),
        end_type: parse_end_type(&form.end_type)?,
    };
    let loads: Vec<Force> = loads_n(&form.loads, us)?
        .into_iter()
        .map(Force::from_newtons)
        .collect();
    let material = materials.get(material_name)?;
    let design = solve_forward(material, &inputs, &loads, correction)?;
    Ok(ConFormOutcome { design })
}

/// Build the persisted spec from the form (SI millimetres / newtons).
pub fn build_spec(form: &ConFormState, us: UnitSystem) -> Result<ConicalSpec> {
    Ok(ConicalSpec::PowerUser {
        end_type: form.end_type.clone(),
        wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
        large_mean_dia_mm: length_mm("large mean diameter", &form.large_mean_dia, us)?,
        small_mean_dia_mm: length_mm("small mean diameter", &form.small_mean_dia, us)?,
        active: positive_num("active coils", &form.active)?,
        free_length_mm: length_mm("free length", &form.free_length, us)?,
        loads_n: loads_n(&form.loads, us)?,
    })
}

/// Fill the form from a loaded spec (round-trips with `build_spec`).
pub fn populate_from_spec(form: &mut ConFormState, spec: &ConicalSpec, us: UnitSystem) {
    match spec {
        ConicalSpec::PowerUser {
            end_type,
            wire_dia_mm,
            large_mean_dia_mm,
            small_mean_dia_mm,
            active,
            free_length_mm,
            loads_n,
        } => {
            form.end_type = end_type.clone();
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.large_mean_dia = fmt_len(*large_mean_dia_mm, us);
            form.small_mean_dia = fmt_len(*small_mean_dia_mm, us);
            form.active = format!("{active}");
            form.free_length = fmt_len(*free_length_mm, us);
            form.loads = fmt_loads(loads_n, us);
        }
    }
}
```

(Verify `MaterialStore::get`'s exact signature/name against how the sibling forms resolve materials — mirror them; adjust the `materials.get(material_name)?` line if the sibling idiom differs.)

Test module (same file; follow torsion form.rs's test conventions — fixtures as `fn`s, `approx::assert_relative_eq` for numerics):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    fn metric_form() -> ConFormState {
        ConFormState {
            end_type: "squared_ground".into(),
            wire_dia: "2".into(),
            large_mean_dia: "20".into(),
            small_mean_dia: "12".into(),
            active: "10".into(),
            free_length: "60".into(),
            loads: "10, 25".into(),
        }
    }

    fn store() -> MaterialStore {
        // Mirror how torsion/extension form tests construct a hermetic store.
        crate::test_support_store()
    }

    #[test]
    fn golden_through_form_matches_direct_engine_solve() {
        let outcome = parse_and_solve(
            &metric_form(),
            "Music Wire",
            UnitSystem::Metric,
            &store(),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        // Direct engine solve with identical inputs.
        let materials = store();
        let material = materials.get("Music Wire").unwrap();
        let inputs = springcore::conical::ConicalInputs {
            wire_dia: springcore::units::Length::from_millimeters(2.0),
            large_mean_dia: springcore::units::Length::from_millimeters(20.0),
            small_mean_dia: springcore::units::Length::from_millimeters(12.0),
            active_coils: 10.0,
            free_length: springcore::units::Length::from_millimeters(60.0),
            end_type: springcore::EndType::SquaredGround,
        };
        let direct = springcore::conical::solve_forward(
            material,
            &inputs,
            &[
                springcore::units::Force::from_newtons(10.0),
                springcore::units::Force::from_newtons(25.0),
            ],
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert_relative_eq!(
            outcome.design.rate.newtons_per_meter(),
            direct.rate.newtons_per_meter(),
            max_relative = 1e-12
        );
        assert_relative_eq!(
            outcome.design.load_points[0].shear_stress.pascals(),
            direct.load_points[0].shear_stress.pascals(),
            max_relative = 1e-12
        );
        assert_eq!(outcome.design.load_points.len(), 2);
    }

    #[test]
    fn correction_selection_changes_through_form_stress() {
        let mk = |corr| {
            parse_and_solve(&metric_form(), "Music Wire", UnitSystem::Metric, &store(), corr)
                .unwrap()
                .design
                .load_points[0]
                .shear_stress
                .pascals()
        };
        let wahl = mk(CurvatureCorrection::Wahl);
        let berg = mk(CurvatureCorrection::Bergstrasser);
        assert!(wahl > berg, "Wahl exceeds Bergsträsser at C=10");
    }

    #[test]
    fn build_populate_round_trips_metric_and_us() {
        for us in [UnitSystem::Metric, UnitSystem::Us] {
            let mut original = metric_form();
            if us == UnitSystem::Us {
                // US displays inches; use plain numerics that parse either way.
                original.wire_dia = "0.08".into();
                original.large_mean_dia = "0.8".into();
                original.small_mean_dia = "0.5".into();
                original.free_length = "2.4".into();
                original.loads = "2, 5".into();
            }
            let spec = build_spec(&original, us).unwrap();
            let mut round = ConFormState::default();
            populate_from_spec(&mut round, &spec, us);
            let spec2 = build_spec(&round, us).unwrap();
            assert_eq!(spec, spec2, "spec round-trip must be lossless ({us:?})");
        }
    }

    #[test]
    fn is_blank_matrix() {
        assert!(ConFormState::default().is_blank());
        for field in [
            Field::WireDia,
            Field::LargeMeanDia,
            Field::SmallMeanDia,
            Field::Active,
            Field::FreeLength,
            Field::Loads,
        ] {
            let mut f = ConFormState::default();
            match field {
                Field::WireDia => f.wire_dia = "1".into(),
                Field::LargeMeanDia => f.large_mean_dia = "1".into(),
                Field::SmallMeanDia => f.small_mean_dia = "1".into(),
                Field::Active => f.active = "1".into(),
                Field::FreeLength => f.free_length = "1".into(),
                Field::Loads => f.loads = "1".into(),
            }
            assert!(!f.is_blank(), "{field:?} alone must trip is_blank");
        }
        // The end-type selector alone does NOT count.
        let mut f = ConFormState::default();
        f.end_type = "plain".into();
        assert!(f.is_blank());
    }

    #[test]
    fn parse_errors_carry_field_prefixes() {
        let cases: &[(fn(&mut ConFormState), &str)] = &[
            (|f| f.wire_dia = "x".into(), "wire diameter"),
            (|f| f.large_mean_dia = "x".into(), "large mean diameter"),
            (|f| f.small_mean_dia = "x".into(), "small mean diameter"),
            (|f| f.active = "0".into(), "active coils"),
            (|f| f.free_length = "-1".into(), "free length"),
            (|f| f.loads = "10, x".into(), "load"),
        ];
        for (mutate, needle) in cases {
            let mut form = metric_form();
            mutate(&mut form);
            let err = parse_and_solve(
                &form,
                "Music Wire",
                UnitSystem::Metric,
                &store(),
                CurvatureCorrection::Bergstrasser,
            )
            .unwrap_err();
            assert!(
                err.to_string().contains(needle),
                "expected '{needle}' in: {err}"
            );
        }
    }

    #[test]
    fn unknown_end_type_key_errors() {
        let mut form = metric_form();
        form.end_type = "SquaredGround".into(); // PascalCase is NOT a valid key
        let err = parse_and_solve(
            &form,
            "Music Wire",
            UnitSystem::Metric,
            &store(),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap_err();
        assert!(err.to_string().contains("unknown end_type"), "got: {err}");
    }
}
```

(`crate::test_support_store()` is a stand-in for however torsion/extension form tests build a hermetic MaterialStore — READ their test modules first and use the identical helper/idiom; if each family builds its own, mirror that shape instead.)

Run: `cargo test -p springmaker conical::form` — red first (missing impl bits), then green.

- [ ] **Step 5: minimal view_model + view skeletons**

Create `springmaker/src/conical/view_model.rs` (presenter, iced-free):

```rust
//! Conical presenters (ADR 0008). Task 1 ships the inputs descriptors and the
//! Empty/Error results states; the Populated arm lands with the full results
//! panel in the next task.

use crate::app::App;
use crate::form_helpers::format_error;
use crate::presenter::{unit_force_label, unit_length_label, FieldDescriptor, StatusLine};

use super::form::Field;

/// Conical results panel state (Populated arrives in Task 2).
#[derive(Debug, Clone, PartialEq)]
pub enum ConResultsView {
    Error(String),
    Empty,
}

/// Results-panel state from app state.
pub fn con_results_view(app: &App) -> ConResultsView {
    if let Some(err) = &app.con_error {
        return ConResultsView::Error(err.clone());
    }
    ConResultsView::Empty
}

/// The six labeled inputs, in display order.
pub fn con_inputs_view(app: &App) -> Vec<FieldDescriptor<Field>> {
    let len = unit_length_label(app.unit_system);
    let force = unit_force_label(app.unit_system);
    vec![
        FieldDescriptor::new(format!("Wire diameter ({len})"), Field::WireDia),
        FieldDescriptor::new(format!("Large mean diameter ({len})"), Field::LargeMeanDia),
        FieldDescriptor::new(format!("Small mean diameter ({len})"), Field::SmallMeanDia),
        FieldDescriptor::new("Active coils".to_string(), Field::Active),
        FieldDescriptor::new(format!("Free length ({len})"), Field::FreeLength),
        FieldDescriptor::new(format!("Loads ({force}, comma-separated)"), Field::Loads),
    ]
}

/// Status lines (shared prefix + design messages arrive with Populated in Task 2).
pub fn con_status_view(app: &App) -> Vec<StatusLine> {
    crate::presenter::common_status_lines(app)
}
```

NOTE: the exact error-holding shape (`app.con_error` vs an error embedded in the outcome) MUST mirror how the sibling families store solve errors — READ how torsion's recompute stores its error (tor_outcome vs a tor_error string) and mirror it exactly in Step 6's app wiring; adjust this presenter to the real shape. The skeleton above assumes a `con_error: Option<String>` sibling of whatever torsion has — verify, don't assume.

Create `springmaker/src/conical/view.rs` (humble view; mirror torsion/view.rs's imports and helpers):

```rust
//! Conical humble view (ADR 0008): renders presenter output, no logic.

// Task 1 skeleton: setup group (material + end-type pickers) + the six-field
// inputs group; the results panel renders Empty/Error only. Task 2 adds the
// populated results (geometry, load table, footer).
```

with `design_panel(app) -> Element<'_, Message>` (material picker per the sibling idiom, end-type picker via the HOISTED `END_TYPES`, the descriptor loop with `con_field_id`), `results_panel(app)` matching `ConResultsView::{Error, Empty}` via the shared error/empty widgets the siblings use, and:

```rust
pub fn con_field_id(field: Field) -> &'static str {
    match field {
        Field::WireDia => "con-wire-dia",
        Field::LargeMeanDia => "con-large-mean-dia",
        Field::SmallMeanDia => "con-small-mean-dia",
        Field::Active => "con-active",
        Field::FreeLength => "con-free-length",
        Field::Loads => "con-loads",
    }
}
```

Hoist `END_TYPES` (`springmaker/src/compression/view.rs:42`, the 4-entry `KeyLabel` list) to the shared module where `KeyLabel`/`find_by_key` are defined, as `pub(crate) const END_TYPES`, and update compression's references — DRY on second consumer. Transcribe the view code from torsion/view.rs's structure (setup group, `section_heading`, `labeled_input` loop) — follow the file, not memory.

- [ ] **Step 6: app wiring (the four exhaustive matches + state + messages + apply_saved)**

In `springmaker/src/app.rs` (all sites verified at HEAD; re-locate by content):
- State: `pub conical: crate::conical::form::ConFormState,` + the outcome/error fields MIRRORING torsion's exact shape (see Step 5 note), initialized in the constructor/Default beside the siblings.
- `Message::ConField(crate::conical::form::Field, String)`; update arm `Message::ConField(f, v) => { self.set_con_field(f, v); true }`; a `fn set_con_field` matching the six fields to `self.conical.*`.
- `recompute()`: the conical arm mirrors the compression arm's shape — clear stale outcome/error, `if self.conical.is_blank() { return; }`, call `crate::conical::form::parse_and_solve(&self.conical, &self.material, self.unit_system, &self.materials, self.correction)` (correction THREADED — the torsion divergence), store outcome or error per the sibling idiom.
- `save_to()`: the conical arm builds `DesignSpec::Conical(crate::conical::form::build_spec(&self.conical, self.unit_system)?)` per the sibling arms.
- `apply_saved()`: DELETE the early-reject block AND the `unreachable!()` arm; the match gains the real arm:

```rust
            springcore::DesignSpec::Conical(spec) => {
                self.family = Family::Conical;
                crate::conical::form::populate_from_spec(
                    &mut self.conical,
                    &spec,
                    self.unit_system,
                );
            }
```

  The `-> bool` return stays; all arms now return `true` (restructure per the compiler — the early-return shape collapses back to a plain match; keep the doc comment about the recompute contract, updated to note all current families apply).
- `calculator.rs:21-34` and `:127-131`: conical arms routing to `crate::conical::view::{design_panel, results_panel}` and `crate::conical::view_model::con_status_view` per the sibling arms.
- PLACEHOLDER TESTS (app.rs ~1405): DELETE `loading_a_conical_design_surfaces_a_clean_action_error` and `conical_load_from_error_survives_post_load_recompute`; REPLACE with (TDD — write first, red because apply_saved still rejects, green after this step):

```rust
    #[test]
    fn loading_a_conical_design_populates_the_conical_form() {
        let mut app = test_app();
        let applied = app.apply_saved(springcore::SavedDesign {
            material: "Music Wire".to_string(),
            unit_system: springcore::UnitSystem::Metric,
            design: springcore::DesignSpec::Conical(springcore::ConicalSpec::PowerUser {
                end_type: "squared_ground".to_string(),
                wire_dia_mm: 2.0,
                large_mean_dia_mm: 20.0,
                small_mean_dia_mm: 12.0,
                active: 10.0,
                free_length_mm: 60.0,
                loads_n: vec![10.0],
            }),
        });
        assert!(applied, "conical loads now apply like any family");
        assert_eq!(app.family, springcore::Family::Conical);
        assert_eq!(app.conical.wire_dia, "2");
        assert_eq!(app.conical.large_mean_dia, "20");
        assert!(app.action_error.is_none());
    }
```

(Adapt `test_app()`/field-value assertions to the module's real helpers; `fmt_len(2.0, Metric)` renders "2" — verify and pin the actual rendering.)

- [ ] **Step 7: workspace green + commit**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all && cargo fmt --all --check`
Expected: all green (springmaker compiles again with all arms landed).

```bash
git add springmaker/src/conical springmaker/src/app.rs springmaker/src/calculator.rs springmaker/src/compression/view.rs <the-shared-picker-module-file> <the-module-root-file>
git commit -m "feat(gui): conical family — form layer, app dispatch, minimal panels; placeholder retired

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

- [ ] **Step 8: mutation gate**

```bash
git diff origin/main...HEAD > /tmp/gui-conical-t1.diff
cargo mutants --in-diff /tmp/gui-conical-t1.diff --package springcore
```
Expected: `0 missed` (the Family arms + message edit + pub visibility are the springcore surface). Kill survivors with tests; never reclassify.

---

### Task 2: full presenter + view + E2E

**Files:**
- Modify: `springmaker/src/conical/view_model.rs` (Populated + geometry table + load table + statuses + footer constant)
- Modify: `springmaker/src/conical/view.rs` (populated rendering + footer)
- Modify: `springmaker/src/ui_tests.rs` (conical E2E + save/load round-trip)

**Interfaces:**
- Consumes: Task 1's entire surface; `crate::presenter::{ResultRow, LoadTable, LoadRow, GoverningRate, fmt_row_value, display_len, display_force, display_stress, unit_stress_label, append_status_messages}`; `springcore::conical::evaluate_status`.
- Produces: `ConResultsView::Populated(Box<ConPopulatedResults>)`, `ConPopulatedResults { governing_rate, geometry, load_table, status }`, the footer const.

- [ ] **Step 1: presenter (TDD)**

Extend `ConResultsView` with `Populated(Box<ConPopulatedResults>)` and add:

```rust
/// The always-present linear-model disclosure (spec Decision 2 of the engine
/// increment, placed here per the GUI spec).
pub const CON_LINEAR_MODEL_NOTE: &str =
    "Linear-range model: progressive stiffening as coils bottom out is not modeled.";

#[derive(Debug, Clone, PartialEq)]
pub struct ConPopulatedResults {
    pub governing_rate: GoverningRate,
    pub geometry: Vec<ResultRow>,
    pub load_table: LoadTable,
    pub status: Vec<StatusLine>,
}
```

`con_results_view` gains the Populated arm building, from `out.design` (labels/decimals EXACT per the spec §C table; every numeric through `fmt_row_value`; lengths via `display_len` + `unit_length_label`):

| Label | Source field | fmt |
|---|---|---|
| Large end OD | large_outer_dia | fmt_row_value(display_len(…), 4) |
| Large end ID | large_inner_dia | 4 |
| Small end OD | small_outer_dia | 4 |
| Small end ID | small_inner_dia | 4 |
| Index (large end) | index_large | fmt_row_value(…, 3), unit "" |
| Index (small end) | index_small | 3, "" |
| Taper per coil | taper_per_coil | 4 (length) |
| Total coils | total_coils | 3, "" |
| Pitch | pitch | 4 (length) |
| Solid length (conservative) | solid_length | 4 (length) |

`governing_rate: GoverningRate::from_rate(design.rate, us)`. The load table mirrors compression's LoadRow construction (force/deflection/length value+unit cells through fmt_row_value per the hardening sweep; stress cell + pct); NO at-solid row (compression renders none — at-solid surfaces via the status warnings and the conservative solid-length row; note this in a comment). `status`: `common_status_lines(app)` + `append_status_messages(&mut lines, &evaluate_status(&out.design, material).messages)` — mirror exactly how the torsion presenter resolves the material for status evaluation.

Presenter tests (same file; follow the sibling presenter-test conventions — build the outcome via `parse_and_solve` on a fixture form):
- `geometry_rows_exact`: solve the metric fixture (Task 1's golden geometry), assert all ten labels in order and spot-check values (Large end OD = 22 mm → "22.0000"; Index (large end) → "10.000"; Taper per coil = 0.8 → "0.8000"; Solid length = 24 → "24.0000").
- `results_view_maps_error_empty_populated`: blank form → Empty; an app with a con error → Error(msg); solved → Populated.
- `huge_finite_load_renders_scientific`: loads "1e9" → the load table stress cell contains 'e' (hardening standard).
- `telescoping_message_passes_through`: a telescoping geometry (large 92 / small 52 / wire 2 / Na 10 fixture — mirrors the engine's boundary test) → a status line whose text == the NEW full engine message (pins the passthrough end-to-end).
- `footer_constant_is_exact`: `assert_eq!(CON_LINEAR_MODEL_NOTE, "Linear-range model: progressive stiffening as coils bottom out is not modeled.");`

- [ ] **Step 2: view (footer + populated rendering)**

`results_panel`'s Populated arm: hero rate → "Geometry" section → the load table → status handled by the calculator's status panel as siblings do → and LAST, the footer, using compression's fatigue-note muted idiom (`compression/view.rs` `render_fatigue`'s Note arm — `column![section_divider(), text(CON_LINEAR_MODEL_NOTE).size(SZ_LABEL).color(C::MUTED)].spacing(8)`), present whenever Populated renders. Empty/Error arms unchanged (footer absent — the presenter test pins the note's presence via the view? No: the footer is view-layer; pin its presence in the E2E via `shows(&app, "Linear-range model")`).

- [ ] **Step 3: E2E (`springmaker/src/ui_tests.rs`)**

Add `type_into_con` mirroring `type_into_tor` (ids via `con_field_id`). Tests:

```rust
#[test]
fn conical_e2e_solve_renders_results_and_footer() {
    let mut app = test_app();
    app.update(Message::SelectFamily(springcore::Family::Conical));
    type_into_con(&mut app, ConField::WireDia, "2");
    type_into_con(&mut app, ConField::LargeMeanDia, "20");
    type_into_con(&mut app, ConField::SmallMeanDia, "12");
    type_into_con(&mut app, ConField::Active, "10");
    type_into_con(&mut app, ConField::FreeLength, "60");
    type_into_con(&mut app, ConField::Loads, "10, 25");
    assert!(app.con_outcome.is_some(), "solve must succeed");
    assert!(shows(&app, "Geometry"));
    assert!(shows(&app, "Large end OD"));
    assert!(shows(&app, "Linear-range model"));
}

#[test]
fn conical_save_load_round_trip() {
    // Fill the form as above, save to a temp file, blank the app (fresh
    // test_app), load the file, assert: family switched to Conical, the form
    // fields repopulated, a recompute yields Populated results. Mirror the
    // sibling round-trip test's temp-file idiom exactly.
}
```

(Complete the round-trip body by mirroring `torsion_min_weight_e2e_and_save_load` / the compression round-trip's exact temp-file + save_to/load_from idiom — copy the neighboring shape, adapt fields.)

- [ ] **Step 4: full gate + commit**

```bash
cargo fmt --all && cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
typos
git diff origin/main...HEAD > /tmp/gui-conical-full.diff
cargo mutants --in-diff /tmp/gui-conical-full.diff --package springcore
```
All clean; mutation `0 missed`.

```bash
git add springmaker/src/conical springmaker/src/ui_tests.rs
git commit -m "feat(gui): conical results panel — geometry table, load table, linear-model footer + E2E

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```
