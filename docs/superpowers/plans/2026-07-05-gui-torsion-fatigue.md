# Torsion GUI Fatigue Section Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The torsion fatigue GUI — optional cycle-moment inputs, the CycleLife selector, and a Fatigue analysis results section — completing the torsion family.

**Architecture:** Task 1 adds the springcore CycleLife GUI surface (Display + ALL const, mutation-gated, no serde) and the whole form layer (fields, the three-state status, `compute_tor_fatigue` on the SOLVED geometry from every scenario arm). Task 2 wires presenter/view/app (the view enum with Hidden-under-MinWeight, the separate fatigue-inputs group, the pick-list) plus E2E and the full gate. Compression's fatigue GUI is the verbatim template.

**Tech Stack:** Rust (MSRV 1.88), iced 0.14, approx, iced_test Simulator.

## Global Constraints

- springcore changes (Task 1's Display/ALL only) mutation-gated to **literal 0 survivors**: STAGE first, then `git diff --staged origin/main -- > /tmp/pr.diff && cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features`. springmaker NOT gated.
- Strict TDD. ADR 0008 (form/view_model iced-free; view iced-only). `tor_field_id` single id source. NO persistence surface anywhere (no serde on CycleLife; no TorsionSpec change; populate CLEARS the fatigue fields).
- **Implementers commit DIRECTLY on `feat/gui-torsion-fatigue` and NEVER push, NEVER create/edit PRs, NEVER run review panels, NEVER touch `.git/REVIEW_CONVERGED_OK`.**
- Compression's verbatim strings: inputs heading `"Fatigue cycle (leave blank to skip)"`; results heading `"Fatigue analysis"`; labels `Min cycle moment ({moment})` / `Max cycle moment ({moment})`; notes `"No fatigue data for this material."` / `"Enter min and max cycle moments to compute fatigue."`.
- Golden through the form (Shigley Example 10-8(c), US units): wire `0.072`, mean `0.5218`, body coils `4.25`, legs `1`/`1`, moments `5`, fatigue `1`→`5` (lbf·in), Million → nf ≈ 1.13 at 5e-3. Fatigue is friction-independent (uses only wire/mean/material/moments).
- MSRV 1.88; no `#[allow(dead_code)]`; no `todo!()`; no vendor names.

---

## File Structure

- Modify `springcore/src/torsion/fatigue.rs` (Display + ALL + test) + `torsion/mod.rs` (re-export) — Task 1.
- Modify `springmaker/src/form_helpers.rs` (`non_negative_moment_nmm` + tests) — Task 1.
- Modify `springmaker/src/torsion/form.rs` (fields, status, compute, outcome, is_blank, populate + tests) — Task 1.
- Modify `springmaker/src/torsion/view_model.rs`, `view.rs`, `springmaker/src/app.rs`, `ui_tests.rs` — Task 2.

---

### Task 1: CycleLife GUI surface + the form layer

**Files:**
- Modify: `springcore/src/torsion/fatigue.rs`, `springcore/src/torsion/mod.rs`
- Modify: `springmaker/src/form_helpers.rs`, `springmaker/src/torsion/form.rs`

**Interfaces:**
- Consumes: `springcore::torsion::{analyze_torsion_fatigue, CycleLife, TorFatigueResult}`; `SpringError::NoFatigueData`; existing helpers (`num`, `Moment::from_pound_force_inches`).
- Produces (Task 2 relies on): `springcore::torsion::ALL_CYCLE_LIVES` + `CycleLife: Display`; `form_helpers::non_negative_moment_nmm(field, value, us) -> Result<f64>`; `TorFormState { fatigue_min, fatigue_max: String, cycle_life: CycleLife, … }`; `Field::{FatigueMin, FatigueMax}`; `pub(crate) enum TorFatigueStatus { Skipped, NoData, Computed(TorFatigueResult) }`; `TorFormOutcome { design, min_weight, fatigue: TorFatigueStatus }` (ALL five construction sites updated).

- [ ] **Step 1: Write the failing tests**

springcore (`fatigue.rs` `mod tests`):

```rust
    #[test]
    fn cycle_life_display_and_all_const() {
        assert_eq!(CycleLife::HundredThousand.to_string(), "10\u{2075} cycles");
        assert_eq!(CycleLife::Million.to_string(), "10\u{2076} cycles");
        assert_eq!(
            ALL_CYCLE_LIVES,
            &[CycleLife::HundredThousand, CycleLife::Million]
        );
    }
```

form_helpers tests:

```rust
    #[test]
    fn non_negative_moment_allows_zero_rejects_negative_converts_us() {
        assert_eq!(
            non_negative_moment_nmm("fatigue min", "0", UnitSystem::Metric).unwrap(),
            0.0
        );
        let err = non_negative_moment_nmm("fatigue min", "-1", UnitSystem::Metric).unwrap_err();
        assert!(err.to_string().contains("fatigue min must be zero or greater"));
        // 1 lbf·in = 112.98482... N·mm (the moment conversion, not force).
        assert_relative_eq!(
            non_negative_moment_nmm("fatigue max", "1", UnitSystem::Us).unwrap(),
            4.4482216152605 * 0.0254 * 1000.0,
            max_relative = 1e-9
        );
        assert!(non_negative_moment_nmm("fatigue min", "nan", UnitSystem::Metric).is_err());
    }
```

form.rs tests (`mod tests`):

```rust
    fn shigley_10_8_us_form() -> TorFormState {
        // Example 10-8's stock spring as direct PowerUser inputs (US units):
        // d = 0.072 in, D = 0.5218 in, Nb = 4.25, legs 1 in, load 5 lbf·in.
        TorFormState {
            wire_dia: "0.072".into(),
            mean_dia: "0.5218".into(),
            body_coils: "4.25".into(),
            leg1: "1".into(),
            leg2: "1".into(),
            moments: "5".into(),
            fatigue_min: "1".into(),
            fatigue_max: "5".into(),
            ..TorFormState::default()
        }
    }

    #[test]
    fn fatigue_golden_through_the_form() {
        let out = parse_and_solve(&shigley_10_8_us_form(), "Music Wire", UnitSystem::Us, &store())
            .expect("the worked example solves");
        match &out.fatigue {
            TorFatigueStatus::Computed(f) => {
                assert_relative_eq!(f.gerber_factor_of_safety, 1.13, max_relative = 5e-3);
            }
            other => panic!("expected Computed, got {other:?}"),
        }
    }

    #[test]
    fn fatigue_skipped_when_either_field_blank() {
        // Both blank (the default), and BOTH one-sided cases: compression's `||`
        // check treats any blank side as not-attempted.
        for (min, max) in [("", ""), ("1", ""), ("", "5")] {
            let form = TorFormState {
                fatigue_min: min.into(),
                fatigue_max: max.into(),
                ..shigley_10_8_us_form()
            };
            let out = parse_and_solve(&form, "Music Wire", UnitSystem::Us, &store()).unwrap();
            assert!(
                matches!(out.fatigue, TorFatigueStatus::Skipped),
                "({min:?},{max:?}) must be Skipped"
            );
        }
    }

    #[test]
    fn fatigue_no_data_for_material_without_table_10_10_grade() {
        // Oil-Tempered Wire is A229 — deliberately data-less; the solve succeeds
        // and the status degrades gracefully (never an error).
        let form = TorFormState {
            fatigue_min: "100".into(),
            fatigue_max: "500".into(),
            ..metric_form()
        };
        let out = parse_and_solve(&form, "Oil-Tempered Wire", UnitSystem::Metric, &store())
            .expect("the solve itself succeeds");
        assert!(matches!(out.fatigue, TorFatigueStatus::NoData));
    }

    #[test]
    fn fatigue_parse_error_propagates() {
        let form = TorFormState {
            fatigue_min: "-1".into(),
            fatigue_max: "5".into(),
            ..shigley_10_8_us_form()
        };
        let err = parse_and_solve(&form, "Music Wire", UnitSystem::Us, &store())
            .expect_err("a negative cycle moment is a form error, not a skip");
        assert!(err.to_string().contains("fatigue min must be zero or greater"));
    }

    #[test]
    fn cycle_life_changes_endurance_through_the_form() {
        let base = shigley_10_8_us_form();
        let short = TorFormState {
            cycle_life: springcore::torsion::CycleLife::HundredThousand,
            ..base.clone()
        };
        let se = |f: &TorFormState| match &parse_and_solve(f, "Music Wire", UnitSystem::Us, &store())
            .unwrap()
            .fatigue
        {
            TorFatigueStatus::Computed(r) => r.fully_reversed_endurance.pascals(),
            other => panic!("expected Computed, got {other:?}"),
        };
        assert!(
            se(&short) > se(&base),
            "10^5's higher Sr fraction must raise Se over 10^6's"
        );
    }

    #[test]
    fn fatigue_computed_on_derived_geometry() {
        // Dimensional derives mean = OD − d; fatigue must use the SOLVED geometry.
        let form = TorFormState {
            scenario: TorScenarioKind::Dimensional,
            wire_dia: "2".into(),
            outer_dia: "22".into(),
            body_coils: "5".into(),
            leg1: "0".into(),
            leg2: "0".into(),
            moments: "1000".into(),
            fatigue_min: "100".into(),
            fatigue_max: "500".into(),
            ..TorFormState::default()
        };
        let out = parse_and_solve(&form, "Music Wire", UnitSystem::Metric, &store()).unwrap();
        assert!(matches!(out.fatigue, TorFatigueStatus::Computed(_)));
    }

    #[test]
    fn fatigue_fields_trip_is_blank_except_min_weight() {
        // Displayed-inputs rule: the four scenarios that SHOW the fatigue inputs
        // count them; MinWeight (which hides them, mirroring its hidden section)
        // does not. Documented divergence from compression, which excludes them
        // everywhere (its pre-fatigue legacy).
        for scenario in [
            TorScenarioKind::PowerUser,
            TorScenarioKind::RateBased,
            TorScenarioKind::Dimensional,
            TorScenarioKind::TwoLoad,
        ] {
            let f = TorFormState {
                scenario,
                fatigue_min: "1".into(),
                ..TorFormState::default()
            };
            assert!(!f.is_blank(), "{scenario:?}: a typed fatigue field signals intent");
        }
        let mw = TorFormState {
            scenario: TorScenarioKind::MinWeight,
            fatigue_min: "1".into(),
            ..TorFormState::default()
        };
        assert!(mw.is_blank(), "MinWeight displays no fatigue inputs");
    }

    #[test]
    fn populate_clears_fatigue_fields_and_resets_life() {
        let form = TorFormState {
            fatigue_min: "1".into(),
            fatigue_max: "5".into(),
            cycle_life: springcore::torsion::CycleLife::HundredThousand,
            ..metric_form()
        };
        let spec = build_spec(&form, UnitSystem::Metric).unwrap();
        let mut form2 = form.clone();
        populate_from_spec(&mut form2, &spec, UnitSystem::Metric);
        assert!(form2.fatigue_min.is_empty() && form2.fatigue_max.is_empty());
        assert_eq!(form2.cycle_life, springcore::torsion::CycleLife::Million);
    }
```

- [ ] **Step 2: Run to verify fail** — `cargo test -p springcore --lib cycle_life_display` and `cargo test -p springmaker torsion` → FAIL (no Display / no fields).

- [ ] **Step 3: Implement**

springcore `fatigue.rs` (after the CycleLife enum; keep `#[non_exhaustive]`/`#[default]`, NO serde):

```rust
impl std::fmt::Display for CycleLife {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            CycleLife::HundredThousand => "10\u{2075} cycles",
            CycleLife::Million => "10\u{2076} cycles",
        })
    }
}

/// All `CycleLife` variants in display order (pick-list source).
pub const ALL_CYCLE_LIVES: &[CycleLife] = &[CycleLife::HundredThousand, CycleLife::Million];
```

`torsion/mod.rs`: extend the fatigue re-export line with `ALL_CYCLE_LIVES`.

form_helpers (after `non_negative_force_n`, its exact shape with the Moment conversion):

```rust
/// Like `num` but requires the value to be >= 0 (zero allowed, negative rejected),
/// returning SI newton-millimetres. Zero is legal for cycle-moment minimums — the
/// exact R = 0 repeated-bending case the fatigue data is defined for.
pub(crate) fn non_negative_moment_nmm(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    let v = num(field, value)?;
    if v < 0.0 {
        return Err(SpringError::InconsistentInputs(format!(
            "{field} must be zero or greater"
        )));
    }
    let v_si = match us {
        UnitSystem::Us => Moment::from_pound_force_inches(v).newton_millimeters(),
        UnitSystem::Metric => v,
    };
    finite_or_err(field, value, v_si)
}
```

form.rs:
- `TorFormState` gains `pub fatigue_min: String, pub fatigue_max: String, pub cycle_life: CycleLife` (Default: `String::new()` ×2, `CycleLife::default()`); `Field` gains `FatigueMin, FatigueMax`.
- The status + outcome (spec §B verbatim):

```rust
/// Three-state fatigue result distinguishing "not attempted" from "no data"
/// (compression's `FatigueStatus` shape).
#[derive(Debug, Clone)]
pub(crate) enum TorFatigueStatus {
    /// User left min/max cycle moments blank; fatigue was not attempted.
    Skipped,
    /// Cycle moments supplied but the material has no bending-fatigue data.
    NoData,
    /// Fatigue analysis succeeded.
    Computed(TorFatigueResult),
}
```

`TorFormOutcome` gains `pub(crate) fatigue: TorFatigueStatus,` — and EVERY arm restructures from `Ok(TorFormOutcome { design: scenario.solve(...)?, min_weight: … })` to:

```rust
            let design = scenario.solve(material, form.friction_model)?;
            let fatigue = compute_tor_fatigue(form, material, &design, us)?;
            Ok(TorFormOutcome { design, min_weight: None, fatigue })
```

(the MinWeight arm keeps its `min_weight: Some(…)` and computes fatigue the same way — one code path; the section hides presenter-side).

```rust
/// Compute the fatigue status for a solved design (compression's `compute_fatigue`
/// mirror): blank cycle-moment fields → `Skipped`; supplied but the material lacks
/// Table 10-10 data → `NoData`; parse errors and non-`NoFatigueData` analysis
/// errors propagate. Uses the SOLVED geometry, so derived-geometry scenarios
/// (Dimensional's mean, MinWeight's chosen wire) are analyzed correctly.
fn compute_tor_fatigue(
    form: &TorFormState,
    material: &Material,
    design: &TorsionDesign,
    us: UnitSystem,
) -> Result<TorFatigueStatus> {
    if form.fatigue_min.trim().is_empty() || form.fatigue_max.trim().is_empty() {
        return Ok(TorFatigueStatus::Skipped);
    }
    let m_min = Moment::from_newton_millimeters(non_negative_moment_nmm(
        "fatigue min",
        &form.fatigue_min,
        us,
    )?);
    let m_max = Moment::from_newton_millimeters(non_negative_moment_nmm(
        "fatigue max",
        &form.fatigue_max,
        us,
    )?);
    match springcore::torsion::analyze_torsion_fatigue(
        material,
        design.inputs.wire_dia,
        design.inputs.mean_dia,
        m_min,
        m_max,
        form.cycle_life,
    ) {
        Ok(r) => Ok(TorFatigueStatus::Computed(r)),
        Err(springcore::SpringError::NoFatigueData(_)) => Ok(TorFatigueStatus::NoData),
        Err(e) => Err(e),
    }
}
```

- `is_blank`: append `&self.fatigue_min, &self.fatigue_max` to the PowerUser/RateBased/Dimensional/TwoLoad `all_empty` lists with the comment:

```rust
            // The fatigue cycle fields count in the four scenarios that DISPLAY
            // them (displayed-inputs rule); MinWeight hides them (its section
            // yields to the optimizer readout) so its arm excludes them.
            // Deliberate divergence from compression, which excludes fatigue
            // fields from is_blank everywhere — a pre-fatigue legacy there.
```

- `populate_from_spec`: every arm adds `form.fatigue_min = String::new(); form.fatigue_max = String::new(); form.cycle_life = CycleLife::default();` beside the established F@r resets.
- Imports: `CycleLife, TorFatigueResult, analyze_torsion_fatigue` paths as used; `non_negative_moment_nmm` from form_helpers.

- [ ] **Step 4: Run to verify pass** — `cargo test -p springcore --lib && cargo test -p springmaker` → PASS.
- [ ] **Step 5: Mutation-check + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add -u && git add springcore springmaker
git diff --staged origin/main -- > /tmp/pr.diff
cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features
# springcore surface = the Display impl + const: Display-arm mutants die on the
# exact-string test; const content asserted. Expected literal 0 survivors.
git commit -m "feat(gui): torsion fatigue form layer — cycle moments, three-state status, CycleLife surface"
```

---

### Task 2: Presenter, view, app, E2E + full gate

**Files:**
- Modify: `springmaker/src/torsion/view_model.rs`, `springmaker/src/torsion/view.rs`, `springmaker/src/app.rs`, `springmaker/src/ui_tests.rs`

**Interfaces:**
- Consumes: Task 1's `TorFatigueStatus`/outcome field/fields/`ALL_CYCLE_LIVES`/Display; `display_stress(s, us) -> (f64, &'static str)`; `divided_result_section`; the compression Note-rendering shape (`section_divider()` + muted `text`).
- Produces: `pub enum TorFatigueView { Hidden, Computed(Vec<ResultRow>), Note(&'static str) }`; `tor_fatigue_view(out) -> TorFatigueView` (unit system threaded as compression does); `tor_fatigue_inputs_view(app) -> Vec<FieldDescriptor<Field>>`; `TorPopulatedResults.fatigue: TorFatigueView`; `Message::TorCycleLife(springcore::torsion::CycleLife)`; ids `tor-fatigue-min`, `tor-fatigue-max`.

Implementation (Step 3), in full — view_model.rs:

```rust
/// Fatigue section state (compression's shape).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TorFatigueView {
    /// Suppressed: a min-weight result occupies the panel instead.
    Hidden,
    /// Fatigue analysis succeeded; readout rows.
    Computed(Vec<ResultRow>),
    /// A muted note (`TOR_FATIGUE_NO_DATA` or `TOR_FATIGUE_SKIPPED`).
    Note(&'static str),
}

/// Shown when cycle moments were supplied but the material has no bending data.
const TOR_FATIGUE_NO_DATA: &str = "No fatigue data for this material.";
/// Shown when the user left the cycle moments blank.
const TOR_FATIGUE_SKIPPED: &str = "Enter min and max cycle moments to compute fatigue.";

fn tor_fatigue_view(out: &crate::torsion::form::TorFormOutcome, us: springcore::UnitSystem) -> TorFatigueView {
    if out.min_weight.is_some() {
        return TorFatigueView::Hidden;
    }
    match &out.fatigue {
        TorFatigueStatus::Computed(f) => {
            let (alt, alt_lbl) = display_stress(f.alternating_stress, us);
            let (mean, mean_lbl) = display_stress(f.mean_stress, us);
            let (se, se_lbl) = display_stress(f.fully_reversed_endurance, us);
            let (sut, sut_lbl) = display_stress(f.ultimate_tensile, us);
            let (sa, sa_lbl) = display_stress(f.strength_amplitude, us);
            TorFatigueView::Computed(vec![
                ResultRow::new("Alternating stress", format!("{alt:.2}"), alt_lbl),
                ResultRow::new("Mean stress", format!("{mean:.2}"), mean_lbl),
                ResultRow::new("Endurance (Se)", format!("{se:.2}"), se_lbl),
                ResultRow::new("Ultimate tensile (Sut)", format!("{sut:.2}"), sut_lbl),
                ResultRow::new("Strength amplitude (Sa)", format!("{sa:.2}"), sa_lbl),
                ResultRow::new(
                    "Gerber FOS",
                    format!("{:.3}", f.gerber_factor_of_safety),
                    "",
                ),
            ])
        }
        TorFatigueStatus::NoData => TorFatigueView::Note(TOR_FATIGUE_NO_DATA),
        TorFatigueStatus::Skipped => TorFatigueView::Note(TOR_FATIGUE_SKIPPED),
    }
}

/// The fatigue cycle inputs: a SEPARATE descriptor list (compression's shape),
/// EMPTY for MinWeight — its results section yields to the optimizer readout, so
/// the inputs hide with it.
pub fn tor_fatigue_inputs_view(app: &App) -> Vec<FieldDescriptor<Field>> {
    if app.torsion.scenario == crate::torsion::form::TorScenarioKind::MinWeight {
        return Vec::new();
    }
    let moment = unit_moment_label(app.unit_system);
    vec![
        FieldDescriptor::new(format!("Min cycle moment ({moment})"), Field::FatigueMin),
        FieldDescriptor::new(format!("Max cycle moment ({moment})"), Field::FatigueMax),
    ]
}
```

`TorPopulatedResults` gains `/// Fatigue section state.\n    pub fatigue: TorFatigueView,`; `tor_results_view` fills it with `tor_fatigue_view(out, us)`. Imports: `TorFatigueStatus` joins the form imports.

view.rs — three additions:
1. Setup chrome (unconditional, after the friction pick-list block):

```rust
        column![
            field_label("Cycle life"),
            styled_pick_list(
                springcore::torsion::ALL_CYCLE_LIVES,
                Some(app.torsion.cycle_life),
                Message::TorCycleLife,
            ),
        ]
        .spacing(4),
```

(mirror the friction block's placement idiom — if friction is inside the initial `column![…]`, add this beside it; if pushed, push it.)
2. Inputs panel, after the main descriptor loop:

```rust
    let fatigue_inputs = tor_fatigue_inputs_view(app);
    if !fatigue_inputs.is_empty() {
        inputs_col = inputs_col.push(section_divider());
        inputs_col = inputs_col.push(section_heading("Fatigue cycle (leave blank to skip)"));
        for fd in &fatigue_inputs {
            let field = fd.field;
            inputs_col = inputs_col.push(labeled_input(
                &fd.label,
                tor_field_value(&app.torsion, field),
                tor_field_id(field),
                move |s| Message::TorField(field, s),
            ));
        }
    }
```

3. Results panel, after the min-weight section:

```rust
            match &p.fatigue {
                TorFatigueView::Hidden => {}
                TorFatigueView::Computed(rows) => {
                    col = col.push(divided_result_section("Fatigue analysis", rows));
                }
                TorFatigueView::Note(msg) => {
                    col = col.push(section_divider());
                    col = col.push(text(*msg).size(SZ_LABEL).color(C::MUTED));
                }
            }
```

(adapt to the results panel's actual builder shape — the min-weight rendering shows the established pattern; compression view.rs:311-319 is the Note-shape template.) `tor_field_value` gains the two arms; `tor_field_id` gains `Field::FatigueMin => "tor-fatigue-min", Field::FatigueMax => "tor-fatigue-max"`.

app.rs: `TorCycleLife(springcore::torsion::CycleLife),` message + `Message::TorCycleLife(l) => { self.torsion.cycle_life = l; true }` + `set_tor_field` arms.

- [ ] **Step 1: failing tests** — view_model tests:

```rust
    #[test]
    fn fatigue_view_states_map_correctly() {
        let m = store();
        // Computed: the golden fixture (form.rs's shape, duplicated per module
        // convention) yields the six rows with a Gerber FOS row.
        let computed = crate::torsion::form::parse_and_solve(
            &shigley_10_8_us_form_fixture(),
            "Music Wire",
            UnitSystem::Us,
            &m,
        )
        .unwrap();
        match tor_fatigue_view(&computed, UnitSystem::Us) {
            TorFatigueView::Computed(rows) => {
                assert_eq!(rows.len(), 6);
                assert!(rows.iter().any(|r| r.label == "Gerber FOS"));
                assert!(rows.iter().any(|r| r.label == "Endurance (Se)" && r.unit == "ksi"));
            }
            other => panic!("expected Computed, got {other:?}"),
        }
        // Skipped → the note.
        let skipped = crate::torsion::form::parse_and_solve(
            &metric_form(),
            "Music Wire",
            UnitSystem::Metric,
            &m,
        )
        .unwrap();
        assert_eq!(
            tor_fatigue_view(&skipped, UnitSystem::Metric),
            TorFatigueView::Note("Enter min and max cycle moments to compute fatigue.")
        );
        // Hidden under MinWeight even with fatigue fields filled.
        let mw = crate::torsion::form::parse_and_solve(
            &TorFormState {
                fatigue_min: "10".into(),
                fatigue_max: "50".into(),
                ..min_weight_form_fixture()
            },
            "Music Wire",
            UnitSystem::Metric,
            &m,
        )
        .unwrap();
        assert_eq!(tor_fatigue_view(&mw, UnitSystem::Metric), TorFatigueView::Hidden);
    }

    #[test]
    fn fatigue_inputs_list_empty_only_for_min_weight() {
        let mut app = fresh_app_torsion();
        assert_eq!(tor_fatigue_inputs_view(&app).len(), 2);
        assert!(tor_fatigue_inputs_view(&app)
            .iter()
            .any(|f| f.label.contains("Min cycle moment")));
        app.torsion.scenario = crate::torsion::form::TorScenarioKind::MinWeight;
        assert!(tor_fatigue_inputs_view(&app).is_empty());
    }
```

(fixture helpers per the module's duplicate-fixture convention: `shigley_10_8_us_form_fixture` mirrors form.rs's `shigley_10_8_us_form`; `min_weight_form_fixture` already exists from the MinWeight increment.) E2E (ui_tests.rs):

```rust
#[test]
fn torsion_fatigue_e2e_rows_nodata_and_minweight_suppression() {
    use crate::torsion::form::{Field as TF, TorScenarioKind};
    // Rows render for a computed analysis.
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Torsion));
    type_into_tor(&mut app, TF::WireDia, "2");
    type_into_tor(&mut app, TF::MeanDia, "20");
    type_into_tor(&mut app, TF::BodyCoils, "5");
    type_into_tor(&mut app, TF::Leg1, "0");
    type_into_tor(&mut app, TF::Leg2, "0");
    type_into_tor(&mut app, TF::Moments, "1000");
    type_into_tor(&mut app, TF::FatigueMin, "100");
    type_into_tor(&mut app, TF::FatigueMax, "500");
    let out = app.tor_outcome.as_ref().expect("solves");
    assert!(matches!(
        out.fatigue,
        crate::torsion::form::TorFatigueStatus::Computed(_)
    ));
    assert!(shows(&mut app, "Gerber FOS"), "the fatigue rows render");

    // NoData note for a material without Table 10-10 data.
    app.update(Message::SelectMaterial("Oil-Tempered Wire".into()));
    assert!(
        shows(&mut app, "No fatigue data for this material."),
        "the NoData note renders"
    );

    // MinWeight suppression: switching scenario hides both inputs and section.
    app.update(Message::TorScenario(TorScenarioKind::MinWeight));
    assert!(
        !shows(&mut app, "Fatigue cycle (leave blank to skip)"),
        "fatigue inputs hide under MinWeight"
    );
}
```

(adapt the material-selection message name to app.rs's actual variant — the established material picker message; verify with grep, don't guess.)

- [ ] **Step 2: verify fail** — `cargo test -p springmaker fatigue` → FAIL (no `TorFatigueView`).
- [ ] **Step 3: implement** (all code above).
- [ ] **Step 4: verify pass** — `cargo test -p springmaker` → PASS.
- [ ] **Step 5: Full gate + commit**

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
typos
cargo test --workspace --all-features
git add -u
git diff --staged origin/main -- > /tmp/pr.diff
cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features
# Expected: all green; springcore literal 0 survivors (unchanged since Task 1).
git commit -m "feat(gui): torsion fatigue presenter, cycle-life picker, and E2E — the family complete"
```

- [ ] **Step 6: Final whole-branch review** — the controller dispatches the panel (general-code, architect, simplifier, MANDATORY input-domain adversary on the three-state × scenario × selector matrix; NO persistence reviewer — zero format surface, stated in the brief), cycles to convergence, then pushes and opens the PR.

---

## Notes for the implementer

- **Never push, never create/edit PRs, never run review panels** — controller-only.
- `TorFormOutcome` gains a field → ALL FIVE construction sites in form.rs restructure (bind `design`, compute `fatigue`, then construct); grep for any other `TorFormOutcome {` literals (view_model/ui_tests) and update.
- The E2E's material-selection message: use the app's ACTUAL variant (grep `SelectMaterial\|Material(` in app.rs) — never guess message names.
- Fatigue is friction-independent; fixtures don't need PureBending (unlike the optimizer oracles — the recorded rule applies to derived-coils/mass assertions, not fatigue).
- `tor_fatigue_view` takes `us` explicitly (compression's shape) even though `tor_results_view` has `app` — thread it the same way the other row-builders do.
