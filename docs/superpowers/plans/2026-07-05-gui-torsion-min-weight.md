# Torsion GUI MinWeight Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The fifth torsion scenario — MinWeight — driving `springcore::torsion::solve_min_weight` from the GUI, with mass/binding result rows and the additive `TorsionSpec::MinWeight` persistence variant.

**Architecture:** Task 1 makes the two ADDITIVE springcore changes (DiaPolicy serde/Display/ALL; the persistence variant) plus the springmaker `populate_from_spec` transitional-arm compile-keeper. Task 2 builds the form layer (scenario, fields with the pre-filled index bounds, the solve arm, the outcome extra, round-trips, is_blank). Task 3 wires presenter/view/app (Optimization rows, descriptors, the MinWeight-gated DiaPolicy pick-list) and the E2E + full gate.

**Tech Stack:** Rust (MSRV 1.88), iced 0.14, serde/toml, approx, iced_test Simulator.

## Global Constraints

- springcore changes (Task 1) mutation-gated to **literal 0 survivors**: `git diff origin/main -- > /tmp/pr.diff && cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features`. springmaker NOT gated.
- Strict TDD. ADR 0008 (form/view_model iced-free; view iced-only). One-way boundary. `tor_field_id` single widget-id source. No vendor names. No `#[allow(dead_code)]`, no `todo!()`.
- **Implementers commit DIRECTLY on `feat/gui-torsion-min-weight` and NEVER push, NEVER create/edit PRs, NEVER run review panels, NEVER touch `.git/REVIEW_CONVERGED_OK`.**
- Oracle rule (recorded): fixtures asserting optimizer mass/coils set `FrictionModel::PureBending` EXPLICITLY (the form default is ShigleyFriction; the denominator changes the mass). Golden geometry: rate = `0.5085 N·m/rad` = `format!("{}", 0.5085_f64 * 1000.0 * std::f64::consts::PI / 180.0)` N·mm/° in the rate field.
- Verified engine surface: `springcore::torsion::{solve_min_weight, TorMinWeightRequest, TorMinWeightSolution, DiaPolicy, TorBindingConstraint, ALL_FRICTION_MODELS}`; request fields `{required_rate, max_moment, leg1, leg2, friction_model, dia_policy, index_bounds, max_outer_dia, arbor_dia, candidate_diameters}`; the engine validates `1 < c_min < c_max` (its message names the bounds — the form does NOT duplicate that guard).
- Verified form-helpers: `ang_rate_nmm_per_deg, fmt_ang_rate_nmm_per_deg, moment_nmm, fmt_moment, num, length_mm, non_negative_length_mm, fmt_len`.

---

## File Structure

- Modify `springcore/src/torsion/optimize.rs` (DiaPolicy derives + Display + ALL const), `springcore/src/torsion/mod.rs` (re-export), `springcore/src/persistence.rs` (variant + tests) — Task 1.
- Modify `springmaker/src/torsion/form.rs` — Task 1 compile-keeper arm; Task 2 everything else.
- Modify `springmaker/src/torsion/view_model.rs`, `view.rs`, `springmaker/src/app.rs`, `springmaker/src/ui_tests.rs` — Task 3.

---

### Task 1: springcore additive — DiaPolicy GUI surface + TorsionSpec::MinWeight

**Files:**
- Modify: `springcore/src/torsion/optimize.rs` (DiaPolicy block), `springcore/src/torsion/mod.rs` (re-export line), `springcore/src/persistence.rs` (variant after `TwoLoad` + tests)
- Modify: `springmaker/src/torsion/form.rs` (transitional populate arm — compile-keeper)

**Interfaces:**
- Produces: `DiaPolicy` additionally derives `serde::Serialize, serde::Deserialize` and implements `Display` ("Max Margin" / "Compact"); `pub const ALL_DIA_POLICIES: &[DiaPolicy]` re-exported as `springcore::torsion::ALL_DIA_POLICIES`; `TorsionSpec::MinWeight { rate_nmm_per_deg: f64, max_moment_nmm: f64, leg1_mm: f64, leg2_mm: f64, arbor_dia_mm: Option<f64>, friction_model: FrictionModel, dia_policy: DiaPolicy, index_min: f64, index_max: f64, max_outer_dia_mm: Option<f64>, candidate_diameters_mm: Vec<f64> }`. Tasks 2–3 rely on these exact shapes.

- [ ] **Step 1: Write the failing springcore tests**

In `springcore/src/torsion/optimize.rs` `mod tests`:

```rust
    #[test]
    fn dia_policy_display_and_all_const() {
        assert_eq!(DiaPolicy::MaxMargin.to_string(), "Max Margin");
        assert_eq!(DiaPolicy::Compact.to_string(), "Compact");
        assert_eq!(ALL_DIA_POLICIES, &[DiaPolicy::MaxMargin, DiaPolicy::Compact]);
    }
```

In `springcore/src/persistence.rs` `mod tests`:

```rust
#[test]
fn torsion_min_weight_round_trips_both_options_and_policies() {
    use crate::torsion::{DiaPolicy, FrictionModel};
    for design in [
        DesignSpec::Torsion(TorsionSpec::MinWeight {
            rate_nmm_per_deg: 8.875,
            max_moment_nmm: 100.0,
            leg1_mm: 10.0,
            leg2_mm: 0.0,
            arbor_dia_mm: Some(10.0),
            friction_model: FrictionModel::PureBending,
            dia_policy: DiaPolicy::MaxMargin,
            index_min: 4.0,
            index_max: 12.0,
            max_outer_dia_mm: Some(30.0),
            candidate_diameters_mm: vec![1.5, 2.0, 2.5],
        }),
        DesignSpec::Torsion(TorsionSpec::MinWeight {
            rate_nmm_per_deg: 8.875,
            max_moment_nmm: 100.0,
            leg1_mm: 0.0,
            leg2_mm: 0.0,
            arbor_dia_mm: None,
            friction_model: FrictionModel::ShigleyFriction,
            dia_policy: DiaPolicy::Compact,
            index_min: 4.0,
            index_max: 12.0,
            max_outer_dia_mm: None,
            candidate_diameters_mm: vec![2.0],
        }),
    ] {
        let saved = SavedDesign {
            material: "Music Wire".into(),
            unit_system: UnitSystem::Metric,
            design,
        };
        let back = SavedDesign::from_toml(&saved.to_toml().unwrap()).unwrap();
        assert_eq!(saved, back);
    }
}

#[test]
fn torsion_min_weight_missing_required_field_errors() {
    // dia_policy omitted → DataFile (only the two *_mm Options may be absent).
    let toml = r#"
material = "Music Wire"
unit_system = "Metric"

[design]
family = "Torsion"
type = "MinWeight"
rate_nmm_per_deg = 8.875
max_moment_nmm = 100.0
leg1_mm = 0.0
leg2_mm = 0.0
friction_model = "PureBending"
index_min = 4.0
index_max = 12.0
candidate_diameters_mm = [2.0]
"#;
    assert!(matches!(
        SavedDesign::from_toml(toml),
        Err(SpringError::DataFile(_))
    ));
}

#[test]
fn torsion_min_weight_rejects_non_finite_candidate_and_bound() {
    // Two complete fixtures: a non-finite Vec ENTRY and a non-finite scalar bound —
    // both must trip reject_non_finite's tree-walk.
    const NON_FINITE_CANDIDATE: &str = r#"
material = "Music Wire"
unit_system = "Metric"

[design]
family = "Torsion"
type = "MinWeight"
rate_nmm_per_deg = 8.875
max_moment_nmm = 100.0
leg1_mm = 0.0
leg2_mm = 0.0
friction_model = "PureBending"
dia_policy = "MaxMargin"
index_min = 4.0
index_max = 12.0
candidate_diameters_mm = [2.0, inf]
"#;
    const NON_FINITE_BOUND: &str = r#"
material = "Music Wire"
unit_system = "Metric"

[design]
family = "Torsion"
type = "MinWeight"
rate_nmm_per_deg = 8.875
max_moment_nmm = 100.0
leg1_mm = 0.0
leg2_mm = 0.0
friction_model = "PureBending"
dia_policy = "MaxMargin"
index_min = inf
index_max = 12.0
candidate_diameters_mm = [2.0]
"#;
    for (name, toml) in [
        ("candidate", NON_FINITE_CANDIDATE),
        ("bound", NON_FINITE_BOUND),
    ] {
        assert!(
            matches!(SavedDesign::from_toml(toml), Err(SpringError::DataFile(_))),
            "non-finite {name} must be rejected"
        );
    }
}
```

- [ ] **Step 2: Run to verify fail** — `cargo test -p springcore --lib dia_policy_display torsion_min_weight` → FAIL (no Display / no variant).

- [ ] **Step 3: Implement**

`optimize.rs` — extend the DiaPolicy block (keep `#[non_exhaustive]` and `#[default]`):

```rust
#[non_exhaustive] // sibling parity (HookSpec precedent): variants may be added
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize,
)]
pub enum DiaPolicy {
    /// Largest allowed D: minimum bending stress (K_bi falls with index), maximum
    /// margin (default).
    #[default]
    MaxMargin,
    /// Smallest D that satisfies the stress allowable: the most compact coil.
    Compact,
}

impl std::fmt::Display for DiaPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            DiaPolicy::MaxMargin => "Max Margin",
            DiaPolicy::Compact => "Compact",
        })
    }
}

/// All `DiaPolicy` variants in display order (pick-list source).
pub const ALL_DIA_POLICIES: &[DiaPolicy] = &[DiaPolicy::MaxMargin, DiaPolicy::Compact];
```

`mod.rs`: extend the optimize re-export line with `ALL_DIA_POLICIES`.

`persistence.rs`: append the variant after `TwoLoad` exactly per the Interfaces block, with docs:

```rust
    MinWeight {
        /// Required angular rate in N·mm per degree (the family's storage flavor).
        rate_nmm_per_deg: f64,
        max_moment_nmm: f64,
        leg1_mm: f64,
        leg2_mm: f64,
        arbor_dia_mm: Option<f64>,
        friction_model: crate::torsion::FrictionModel,
        dia_policy: crate::torsion::DiaPolicy,
        index_min: f64,
        index_max: f64,
        /// Optional outer-diameter cap; missing key → None (documented rule).
        max_outer_dia_mm: Option<f64>,
        candidate_diameters_mm: Vec<f64>,
    },
```

`springmaker/src/torsion/form.rs` compile-keeper — `populate_from_spec` gains a TRANSITIONAL arm (Task 2 replaces it; the established pattern):

```rust
        // Task 2 replaces this arm with full MinWeight population (scenario kind,
        // optimizer fields, both selectors). Until then nothing writes this tag.
        TorsionSpec::MinWeight {
            rate_nmm_per_deg, leg1_mm, leg2_mm, arbor_dia_mm, friction_model, ..
        } => {
            form.rate = fmt_ang_rate_nmm_per_deg(*rate_nmm_per_deg, us);
            form.leg1 = fmt_len(*leg1_mm, us);
            form.leg2 = fmt_len(*leg2_mm, us);
            form.arbor_dia = match arbor_dia_mm {
                Some(v) => fmt_len(*v, us),
                None => String::new(),
            };
            form.friction_model = *friction_model;
            form.moment_entry = MomentEntry::Direct;
            form.forces = String::new();
            form.load_radius = String::new();
        }
```

- [ ] **Step 4: Run to verify pass** — `cargo test -p springcore --lib && cargo test -p springmaker` → PASS.

- [ ] **Step 5: Mutation-check + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git diff origin/main -- > /tmp/pr.diff
cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features
# Expected 0 survivors: Display-arm mutants die on the exact-string test; the
# variant is data-only; ALL const content is asserted.
git add springcore/src/torsion/optimize.rs springcore/src/torsion/mod.rs springcore/src/persistence.rs springmaker/src/torsion/form.rs
git commit -m "feat(torsion): DiaPolicy GUI surface + additive TorsionSpec::MinWeight variant"
```

---

### Task 2: Form layer — scenario, fields, solve arm, outcome extra

**Files:**
- Modify: `springmaker/src/torsion/form.rs`

**Interfaces:**
- Consumes: Task 1's shapes; engine `solve_min_weight`/`TorMinWeightRequest`/`TorMinWeightSolution`/`DiaPolicy`/`TorBindingConstraint`; helpers per Global Constraints.
- Produces (Task 3 relies on): `TorScenarioKind::MinWeight` (Display "Min Weight", fifth in `ALL_TOR_SCENARIOS`); `TorFormState` fields `max_moment, index_min, index_max, max_outer_dia, candidate_diameters: String` + `dia_policy: DiaPolicy` with a MANUAL `Default` (index bounds `"4"`/`"12"`; derive removed); `Field::{MaxMoment, IndexMin, IndexMax, MaxOuterDia, CandidateDiameters}`; `pub(crate) struct TorMinWeightExtra { pub binding: TorBindingConstraint, pub mass_kg: f64 }`; `TorFormOutcome { design, min_weight: Option<TorMinWeightExtra> }` (ALL existing construction sites — form.rs arms, any view_model/ui_tests literals — gain `min_weight: None`; the MinWeight arm fills it).

Implementation code (Step 3), given here in full:

- `TorScenarioKind` + `ALL_TOR_SCENARIOS` + Display gain `MinWeight` / `"Min Weight"` (fifth).
- `TorFormState`: remove `Default` from the derive; add the five String fields + `pub dia_policy: DiaPolicy,`; add:

```rust
impl Default for TorFormState {
    fn default() -> Self {
        Self {
            scenario: TorScenarioKind::default(),
            moment_entry: MomentEntry::default(),
            wire_dia: String::new(),
            mean_dia: String::new(),
            outer_dia: String::new(),
            body_coils: String::new(),
            rate: String::new(),
            leg1: String::new(),
            leg2: String::new(),
            arbor_dia: String::new(),
            moments: String::new(),
            forces: String::new(),
            load_radius: String::new(),
            moment1: String::new(),
            angle1: String::new(),
            moment2: String::new(),
            angle2: String::new(),
            max_moment: String::new(),
            // Pre-filled sensible defaults (extension's exact values and the engine's
            // caution range). is_blank EXCLUDES these two — a pre-filled field cannot
            // distinguish an untouched form (extension's documented rule).
            index_min: "4".into(),
            index_max: "12".into(),
            max_outer_dia: String::new(),
            candidate_diameters: String::new(),
            dia_policy: DiaPolicy::default(),
            friction_model: FrictionModel::default(),
        }
    }
}
```

- Candidates parser (mirrors extension's):

```rust
/// Parse the comma-separated candidate-diameter list into SI millimetres, rejecting
/// an empty list at the form boundary. Shared by the MinWeight `parse_and_solve` and
/// `build_spec` arms (extension's `parse_candidate_diameters_mm` precedent).
fn parse_candidate_diameters_mm(form: &TorFormState, us: UnitSystem) -> Result<Vec<f64>> {
    let candidates: Vec<f64> = form
        .candidate_diameters
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| length_mm("candidate diameter", s, us))
        .collect::<Result<_>>()?;
    if candidates.is_empty() {
        return Err(springcore::SpringError::InconsistentInputs(
            "provide at least one candidate diameter".into(),
        ));
    }
    Ok(candidates)
}
```

- `TorFormOutcome` + extra:

```rust
/// Min-weight optimisation extras when the active outcome is a MinWeight solve
/// (extension's plain-Option precedent: no other extra section exists yet, so an
/// enum would be speculative).
#[derive(Debug, Clone)]
pub(crate) struct TorMinWeightExtra {
    pub binding: springcore::torsion::TorBindingConstraint,
    pub mass_kg: f64,
}

#[derive(Debug, Clone)]
pub struct TorFormOutcome {
    pub design: TorsionDesign,
    /// `Some` only for MinWeight solves.
    pub(crate) min_weight: Option<TorMinWeightExtra>,
}
```

(every existing `TorFormOutcome { design: … }` literal gains `min_weight: None`.)

- `parse_and_solve` MinWeight arm:

```rust
        TorScenarioKind::MinWeight => {
            let req = springcore::torsion::TorMinWeightRequest {
                required_rate: AngularRate::from_newton_meters_per_degree(
                    ang_rate_nmm_per_deg("rate", &form.rate, us)? / 1000.0,
                ),
                max_moment: Moment::from_newton_millimeters(moment_nmm(
                    "max moment",
                    &form.max_moment,
                    us,
                )?),
                leg1: Length::from_millimeters(non_negative_length_mm("leg 1", &form.leg1, us)?),
                leg2: Length::from_millimeters(non_negative_length_mm("leg 2", &form.leg2, us)?),
                friction_model: form.friction_model,
                dia_policy: form.dia_policy,
                // Plain finite parses: the ENGINE's `1 < c_min < c_max` guard is the
                // validator and its message names the bounds — no duplicated form guard.
                index_bounds: (
                    num("index min", &form.index_min)?,
                    num("index max", &form.index_max)?,
                ),
                max_outer_dia: if form.max_outer_dia.trim().is_empty() {
                    None
                } else {
                    Some(Length::from_millimeters(length_mm(
                        "max outer diameter",
                        &form.max_outer_dia,
                        us,
                    )?))
                },
                arbor_dia: parse_arbor(form, us)?,
                candidate_diameters: parse_candidate_diameters_mm(form, us)?
                    .into_iter()
                    .map(Length::from_millimeters)
                    .collect(),
            };
            let sol = springcore::torsion::solve_min_weight(material, &req)?;
            Ok(TorFormOutcome {
                design: sol.design,
                min_weight: Some(TorMinWeightExtra {
                    binding: sol.binding,
                    mass_kg: sol.mass_kg,
                }),
            })
        }
```

- `build_spec` MinWeight arm → `TorsionSpec::MinWeight` (same parsers; `dia_policy: form.dia_policy`; candidates via `parse_candidate_diameters_mm`). `populate_from_spec`: REPLACE Task 1's transitional arm with full population — sets `scenario = TorScenarioKind::MinWeight`, `max_moment = fmt_moment(*max_moment_nmm, us)`, `index_min = format!("{index_min}")`, `index_max = format!("{index_max}")`, `max_outer_dia` (Option→fmt_len/empty), `candidate_diameters = candidate_diameters_mm.iter().map(|&d| fmt_len(d, us)).collect::<Vec<_>>().join(", ")`, `dia_policy = *dia_policy`, plus the shared fields and the established F@r resets.
- `is_blank` MinWeight arm (index bounds EXCLUDED, comment carrying the rationale):

```rust
            TorScenarioKind::MinWeight => all_empty(&[
                &self.rate,
                &self.max_moment,
                &self.leg1,
                &self.leg2,
                &self.arbor_dia,
                &self.max_outer_dia,
                &self.candidate_diameters,
            ]),
```

- [ ] **Step 1: failing tests** (form.rs `mod tests`):

```rust
    fn min_weight_metric_form() -> TorFormState {
        TorFormState {
            scenario: TorScenarioKind::MinWeight,
            // Oracle rule: PureBending EXPLICITLY (default Shigley changes the mass).
            friction_model: FrictionModel::PureBending,
            rate: format!("{}", 0.5085_f64 * 1000.0 * std::f64::consts::PI / 180.0),
            max_moment: "100".into(),
            leg1: "0".into(),
            leg2: "0".into(),
            candidate_diameters: "1.5, 2, 2.5".into(),
            ..TorFormState::default()
        }
    }

    #[test]
    fn min_weight_solves_smallest_candidate_with_mass_extra() {
        let out = parse_and_solve(&min_weight_metric_form(), "Music Wire", UnitSystem::Metric, &store())
            .expect("MinWeight should solve");
        assert_relative_eq!(
            out.design.inputs.wire_dia.meters(),
            0.0015,
            max_relative = 1e-12
        );
        assert_eq!(out.design.load_points.len(), 1);
        let mw = out.min_weight.expect("MinWeight fills the extra");
        // Closed-form mass (no legs): ρ·(π/4)d²·(π·E·d⁴/(64·k′)), constants read
        // from the material so the oracle is exact without hardcoding them.
        let m = store();
        let mat = m.get("Music Wire").unwrap();
        let d = 0.0015_f64;
        let len = std::f64::consts::PI * mat.youngs_modulus.pascals() * d.powi(4) / (64.0 * 0.5085);
        let expected = mat.density.kg_per_m3() * (std::f64::consts::PI / 4.0) * d * d * len;
        assert_relative_eq!(mw.mass_kg, expected, max_relative = 1e-9);
    }

    #[test]
    fn min_weight_policies_agree_on_mass_and_other_scenarios_have_no_extra() {
        let base = min_weight_metric_form();
        let compact = TorFormState {
            dia_policy: springcore::torsion::DiaPolicy::Compact,
            ..base.clone()
        };
        let a = parse_and_solve(&base, "Music Wire", UnitSystem::Metric, &store()).unwrap();
        let b = parse_and_solve(&compact, "Music Wire", UnitSystem::Metric, &store()).unwrap();
        let (ma, mb) = (a.min_weight.unwrap(), b.min_weight.unwrap());
        assert_relative_eq!(ma.mass_kg, mb.mass_kg, max_relative = 1e-9);
        assert!(
            b.design.inputs.mean_dia.meters() <= a.design.inputs.mean_dia.meters(),
            "Compact D ≤ MaxMargin D"
        );
        // The other scenarios never fill the extra.
        let pu = parse_and_solve(&metric_form(), "Music Wire", UnitSystem::Metric, &store()).unwrap();
        assert!(pu.min_weight.is_none());
    }

    #[test]
    fn min_weight_infeasible_and_empty_candidates_error() {
        let infeasible = TorFormState {
            max_moment: "1e9".into(),
            ..min_weight_metric_form()
        };
        let err = parse_and_solve(&infeasible, "Music Wire", UnitSystem::Metric, &store())
            .expect_err("hugely overstressed request is infeasible");
        assert!(
            err.to_string().contains("no feasible design"),
            "engine Infeasible must surface; got: {err}"
        );
        let empty = TorFormState {
            candidate_diameters: "  ,  ".into(),
            ..min_weight_metric_form()
        };
        let err = parse_and_solve(&empty, "Music Wire", UnitSystem::Metric, &store())
            .expect_err("empty candidate list rejected at the form boundary");
        assert!(
            err.to_string().contains("provide at least one candidate diameter"),
            "form guard message expected; got: {err}"
        );
    }

    #[test]
    fn min_weight_build_spec_populate_round_trips() {
        for us in [UnitSystem::Metric, UnitSystem::Us] {
            let form = TorFormState {
                arbor_dia: "10".into(),
                max_outer_dia: "30".into(),
                dia_policy: springcore::torsion::DiaPolicy::Compact,
                ..min_weight_metric_form()
            };
            let spec = build_spec(&form, us).unwrap();
            let mut form2 = TorFormState::default();
            populate_from_spec(&mut form2, &spec, us);
            assert_eq!(form2.scenario, TorScenarioKind::MinWeight);
            assert_eq!(form2.dia_policy, springcore::torsion::DiaPolicy::Compact);
            assert_eq!(build_spec(&form2, us).unwrap(), spec);
        }
    }

    #[test]
    fn min_weight_is_blank_excludes_prefilled_bounds() {
        let fresh = TorFormState {
            scenario: TorScenarioKind::MinWeight,
            ..TorFormState::default()
        };
        assert!(fresh.is_blank(), "pre-filled index bounds cannot signal intent");
        let edited_bound = TorFormState {
            scenario: TorScenarioKind::MinWeight,
            index_min: "5".into(),
            ..TorFormState::default()
        };
        assert!(
            edited_bound.is_blank(),
            "bounds are excluded from is_blank entirely (extension's rule)"
        );
        for (field, value) in [
            ("rate", "8.9"),
            ("max_moment", "100"),
            ("candidate_diameters", "2"),
            ("max_outer_dia", "30"),
        ] {
            let mut f = TorFormState {
                scenario: TorScenarioKind::MinWeight,
                ..TorFormState::default()
            };
            match field {
                "rate" => f.rate = value.into(),
                "max_moment" => f.max_moment = value.into(),
                "candidate_diameters" => f.candidate_diameters = value.into(),
                _ => f.max_outer_dia = value.into(),
            }
            assert!(!f.is_blank(), "typing {field} clears blank");
        }
    }
```

- [ ] **Step 2: verify fail** — `cargo test -p springmaker torsion` → FAIL (no `MinWeight` variant).
- [ ] **Step 3: implement** (all code above; update every `TorFormOutcome` literal across springmaker with `min_weight: None`).
- [ ] **Step 4: verify pass** — `cargo test -p springmaker && cargo test -p springcore --lib` → PASS.
- [ ] **Step 5: commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add springmaker/src/torsion/form.rs springmaker/src/torsion/view_model.rs springmaker/src/ui_tests.rs
git commit -m "feat(gui): torsion MinWeight form layer — fields, solve arm, outcome extra"
```

(view_model/ui_tests only if `TorFormOutcome` literals there needed the `min_weight: None` addition.)

---

### Task 3: Presenter, view, app, E2E + full gate

**Files:**
- Modify: `springmaker/src/torsion/view_model.rs`, `springmaker/src/torsion/view.rs`, `springmaker/src/app.rs`, `springmaker/src/ui_tests.rs`

**Interfaces:**
- Consumes: Task 2's `TorMinWeightExtra`/outcome field/scenario/fields; `springcore::torsion::{ALL_DIA_POLICIES, DiaPolicy, TorBindingConstraint}`.
- Produces: `tor_min_weight_rows(out: &TorFormOutcome) -> Option<Vec<ResultRow>>` (view_model, private); `TorPopulatedResults` gains `pub min_weight: Option<Vec<ResultRow>>`; `Message::TorDiaPolicy(springcore::torsion::DiaPolicy)`; ids `tor-max-moment`, `tor-index-min`, `tor-index-max`, `tor-max-outer-dia`, `tor-candidate-diameters`.

Implementation (Step 3), in full:

- view_model:

```rust
/// Min-weight optimisation rows when the active outcome is a MinWeight solve.
fn tor_min_weight_rows(out: &crate::torsion::form::TorFormOutcome) -> Option<Vec<ResultRow>> {
    let mw = out.min_weight.as_ref()?;
    let binding = match mw.binding {
        TorBindingConstraint::BendingStress => "bending stress",
        TorBindingConstraint::Index => "index",
        TorBindingConstraint::OuterDiameter => "outer diameter",
        // `TorBindingConstraint` is `#[non_exhaustive]`; a future variant falls here.
        _ => "other",
    };
    Some(vec![
        ResultRow::new("Wire mass", format!("{:.4}", mw.mass_kg), "kg"),
        ResultRow::new("Binding constraint", binding, ""),
    ])
}
```

  `TorPopulatedResults` gains `/// Min-weight optimisation rows (MinWeight solves only).\n    pub min_weight: Option<Vec<ResultRow>>,`; `tor_results_view` fills it with `tor_min_weight_rows(out)`. `tor_inputs_view` gains the MinWeight arm (labels exactly): `Rate ({moment}/°)`→`Field::Rate`, `Max moment ({moment})`→`MaxMoment`, `Leg 1 ({len})`, `Leg 2 ({len})`, `Arbor diameter ({len}, optional)`, `"Index min"`→`IndexMin`, `"Index max"`→`IndexMax`, `Max outer diameter ({len}, optional)`→`MaxOuterDia`, `Candidate diameters ({len}), comma-separated`→`CandidateDiameters`.
- view.rs: `tor_field_value`/`tor_field_id` arms for the five fields (ids above); the moment-entry selector gate becomes `app.torsion.scenario != TorScenarioKind::TwoLoad && app.torsion.scenario != TorScenarioKind::MinWeight`; a DiaPolicy pick-list block after it, gated `== MinWeight`:

```rust
    if app.torsion.scenario == TorScenarioKind::MinWeight {
        setup_col = setup_col.push(
            column![
                field_label("Diameter policy"),
                styled_pick_list(
                    springcore::torsion::ALL_DIA_POLICIES,
                    Some(app.torsion.dia_policy),
                    Message::TorDiaPolicy,
                ),
            ]
            .spacing(4),
        );
    }
```

  (adapt to the design panel's actual builder shape — the moment-entry block at view.rs:41 shows the established conditional-push pattern to mirror); results panel: after the rate section, `if let Some(rows) = &p.min_weight { … section_divider() + rows_section("Optimization", rows) … }`.
- app.rs: `TorDiaPolicy(springcore::torsion::DiaPolicy),` message + `Message::TorDiaPolicy(p) => { self.torsion.dia_policy = p; true }` + `set_tor_field` arms for the five fields.

- [ ] **Step 1: failing tests** — view_model tests:

```rust
    // The view_model test module defines its own fixture (its convention — it already
    // duplicates metric_form): the same oracle values as form.rs's, PureBending explicit.
    fn min_weight_form_fixture() -> TorFormState {
        TorFormState {
            scenario: crate::torsion::form::TorScenarioKind::MinWeight,
            friction_model: springcore::torsion::FrictionModel::PureBending,
            rate: format!("{}", 0.5085_f64 * 1000.0 * std::f64::consts::PI / 180.0),
            max_moment: "100".into(),
            leg1: "0".into(),
            leg2: "0".into(),
            candidate_diameters: "1.5, 2, 2.5".into(),
            ..TorFormState::default()
        }
    }

    #[test]
    fn min_weight_rows_render_mass_and_binding_and_none_elsewhere() {
        let m = store();
        let out = crate::torsion::form::parse_and_solve(
            &min_weight_form_fixture(),
            "Music Wire",
            UnitSystem::Metric,
            &m,
        )
        .unwrap();
        let rows = tor_min_weight_rows(&out).expect("MinWeight outcome has the section");
        assert!(rows.iter().any(|r| r.label == "Wire mass" && r.unit == "kg"));
        assert!(rows.iter().any(|r| r.label == "Binding constraint" && r.value == "index"));
        // Non-MinWeight outcome → None.
        let pu = crate::torsion::form::parse_and_solve(&metric_form(), "Music Wire", UnitSystem::Metric, &m).unwrap();
        assert!(tor_min_weight_rows(&pu).is_none());
    }

    #[test]
    fn min_weight_inputs_view_lists_nine_descriptors() {
        let mut app = fresh_app_torsion();
        app.torsion.scenario = crate::torsion::form::TorScenarioKind::MinWeight;
        let fields = tor_inputs_view(&app);
        assert_eq!(fields.len(), 9);
        assert!(fields.iter().any(|f| f.label.contains("Candidate diameters")));
        assert!(fields.iter().any(|f| f.label == "Index min"));
        assert!(!fields.iter().any(|f| f.label.contains("Moments")));
    }
```

  (fixture helper duplicated minimally in the view_model test module per its existing convention; binding "index" holds — MaxMargin with no OD cap on the oracle geometry.) E2E (ui_tests.rs):

```rust
#[test]
fn torsion_min_weight_e2e_and_save_load() {
    use crate::torsion::form::{Field as TF, TorScenarioKind};
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Torsion));
    app.update(Message::TorScenario(TorScenarioKind::MinWeight));
    app.update(Message::TorFriction(springcore::torsion::FrictionModel::PureBending));
    type_into_tor(&mut app, TF::Rate, "8.875");
    type_into_tor(&mut app, TF::MaxMoment, "100");
    type_into_tor(&mut app, TF::Leg1, "0");
    type_into_tor(&mut app, TF::Leg2, "0");
    type_into_tor(&mut app, TF::CandidateDiameters, "1.5, 2, 2.5");
    let out = app.tor_outcome.as_ref().expect("MinWeight must solve");
    assert!(out.min_weight.is_some(), "the optimisation extra is filled");

    let dir = std::env::temp_dir().join(format!("osm_tor_mw_e2e_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("minweight.toml");
    app.save_to(&path);
    let mut app2 = test_app();
    assert!(app2.load_from(&path));
    assert_eq!(app2.torsion.scenario, TorScenarioKind::MinWeight);
    assert!(app2.torsion.candidate_diameters.contains("1.5"));
    let _ = std::fs::remove_dir_all(&dir);
}
```

- [ ] **Step 2: verify fail** — `cargo test -p springmaker min_weight` → FAIL.
- [ ] **Step 3: implement** (all code above).
- [ ] **Step 4: verify pass** — `cargo test -p springmaker` → PASS.
- [ ] **Step 5: Full local gate + commit**

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
typos
cargo test --workspace --all-features
git diff origin/main -- > /tmp/pr.diff
cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features
# Expected: all green; springcore literal 0 survivors.
git add springmaker/src/torsion/view_model.rs springmaker/src/torsion/view.rs springmaker/src/app.rs springmaker/src/ui_tests.rs
git commit -m "feat(gui): torsion MinWeight presenter, DiaPolicy picker, and E2E"
```

- [ ] **Step 6: Final whole-branch review** — the controller dispatches the panel (general-code, architect, simplifier, MANDATORY input-domain adversary, persistence/wire-format reviewer for the new persisted variant), cycles to convergence, then pushes and opens the PR.

---

## Notes for the implementer

- **Never push, never create/edit PRs, never run review panels** — controller-only.
- The `TorFormOutcome.min_weight` field addition breaks EVERY existing struct literal — grep `TorFormOutcome {` across springmaker and add `min_weight: None` (Task 2's compile surface).
- `dia_policy` is `pub(crate)`-reachable via `form.dia_policy` — no getter ceremony (field like `friction_model`).
- Results/status panels beyond the new Optimization section are untouched; every scenario still yields the same `TorsionDesign` rendering.
- The engine's Infeasible Display is `"no feasible design: …"` — tests assert that prefix, not the inner message.
