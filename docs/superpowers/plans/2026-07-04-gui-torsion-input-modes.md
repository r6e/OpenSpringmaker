# Torsion GUI Input Modes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the phase-2 torsion engine scenarios (RateBased, Dimensional, TwoLoad) and the force-at-radius entry toggle to the springmaker GUI, with the clean-break `TorsionSpec` struct→tagged-enum persistence migration.

**Architecture:** Task 1 migrates springcore persistence (tagged enum; legacy tag-less files error) AND adapts the torsion form's build/populate to the PowerUser variant so springmaker compiles at every task boundary. Tasks 2–4 fan out the extension-1c scenario-picker pattern inside `springmaker/src/torsion/`: scenario enum threaded through every match, per-scenario fields/helpers, then the unpersisted force-at-radius toggle. Task 5 adds Simulator E2E (incl. the legacy-file error) and the final whole-branch review.

**Tech Stack:** Rust (MSRV 1.88), iced 0.14, serde/toml, approx, iced_test Simulator.

## Global Constraints

- springcore changes (Task 1) mutation-gated to **literal 0 survivors**: `git diff origin/main -- > /tmp/pr.diff && cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features`. springmaker (Tasks 2–5) NOT mutation-gated.
- Strict TDD. ADR 0008: `form.rs`/`view_model.rs` pure (no iced); `view.rs` iced-only. One-way boundary: torsion never imports compression/extension.
- `tor_field_id` is the single widget-id source (view + Simulator tests). Results/status panels UNCHANGED (every scenario yields the same `TorsionDesign`).
- Clean break: a legacy tag-less torsion TOML fails with `SpringError::DataFile` (test-pinned); no fallback. F@r is NOT persisted (spec stores derived `moments_nmm`; `populate_from_spec` resets `Direct` and clears `forces`/`load_radius`).
- Angular-rate display/canonical unit: N·mm/° (metric) / lbf·in/° (US); persisted field `rate_nmm_per_deg`. Angles: degrees, both unit systems, any finite value.
- Engine surface (exists): `springcore::torsion::{PowerUser, RateBased, Dimensional, TwoLoad, Scenario, moment_from_force_at_radius, FrictionModel, ALL_FRICTION_MODELS}`; `AngularRate::from_newton_meters_per_degree`; `Moment::from_pound_force_inches/newton_millimeters`; `Angle::from_degrees`.
- No commercial/vendor names. No `#[allow(dead_code)]`. Local gate before each commit: crate tests + `cargo fmt --all` + `cargo clippy --all-targets --all-features -- -D warnings`; full gate (incl. doc/typos/workspace/mutation) at Task 5.
- Oracle geometry: d=2 mm, D=20 mm, Music Wire; Nₐ=5 ↔ k′=0.5085 N·m/rad (PureBending) = `0.5085*1000.0*PI/180.0` ≈ 8.875 N·mm/°; 1 rad = `180/π` ≈ 57.29578°.

---

## File Structure

- Modify `springcore/src/persistence.rs` — `TorsionSpec` → tagged enum + guardrail-comment update + test migration/additions (Task 1).
- Modify `springmaker/src/torsion/form.rs` — Task 1 minimal adaptation; Tasks 2–4 scenario enum, fields, per-scenario matches, F@r.
- Modify `springmaker/src/form_helpers.rs` — `ang_rate_nmm_per_deg`, `fmt_ang_rate_nmm_per_deg`, `angle_deg`, `fmt_angle_deg` (Tasks 2–3).
- Modify `springmaker/src/torsion/view_model.rs` — per-scenario `tor_inputs_view` (Tasks 2–4).
- Modify `springmaker/src/torsion/view.rs` — scenario pick-list, moment-entry selector, field maps/ids (Tasks 2–4).
- Modify `springmaker/src/app.rs` — `Message::TorScenario`, `Message::TorMomentEntry`, `set_tor_field` arms (Tasks 2–4).
- Modify `springmaker/src/ui_tests.rs` — per-mode + F@r + legacy-error E2E (Task 5).

---

### Task 1: springcore — TorsionSpec tagged-enum migration (clean break)

**Files:**
- Modify: `springcore/src/persistence.rs` (the `TorsionSpec` block ~lines 99–125; torsion tests ~1120–1250)
- Modify: `springmaker/src/torsion/form.rs` (build_spec/populate_from_spec adaptation — keeps springmaker compiling)

**Interfaces:**
- Produces: `pub enum TorsionSpec` with variants `PowerUser`, `RateBased { rate_nmm_per_deg, .. }`, `Dimensional { outer_dia_mm, .. }`, `TwoLoad { moment1_nmm, angle1_deg, moment2_nmm, angle2_deg, .. }` (exact shapes below). Tasks 2–4 construct/match every variant.
- Consumes: existing `FrictionModel` serde, `DesignSpec::Torsion`, `reject_non_finite` (generic — unchanged).

- [ ] **Step 1: Write the failing springcore tests**

In `springcore/src/persistence.rs` `mod tests`, ADD (keep the existing torsion tests for now — Step 3 updates them):

```rust
#[test]
fn torsion_ratebased_dimensional_twoload_round_trip() {
    use crate::torsion::FrictionModel;
    for design in [
        DesignSpec::Torsion(TorsionSpec::RateBased {
            wire_dia_mm: 2.0,
            mean_dia_mm: 20.0,
            rate_nmm_per_deg: 8.875,
            leg1_mm: 10.0,
            leg2_mm: 0.0,
            arbor_dia_mm: Some(10.0),
            friction_model: FrictionModel::PureBending,
            moments_nmm: vec![1000.0],
        }),
        DesignSpec::Torsion(TorsionSpec::Dimensional {
            wire_dia_mm: 2.0,
            outer_dia_mm: 22.0,
            body_coils: 5.0,
            leg1_mm: 0.0,
            leg2_mm: 0.0,
            arbor_dia_mm: None,
            friction_model: FrictionModel::ShigleyFriction,
            moments_nmm: vec![100.0, 250.0],
        }),
        DesignSpec::Torsion(TorsionSpec::TwoLoad {
            wire_dia_mm: 2.0,
            mean_dia_mm: 20.0,
            leg1_mm: 0.0,
            leg2_mm: 0.0,
            arbor_dia_mm: None,
            friction_model: FrictionModel::ShigleyFriction,
            moment1_nmm: 508.5,
            angle1_deg: -10.0, // negative-but-finite angle is legal (offset-tolerant)
            moment2_nmm: 1017.0,
            angle2_deg: 47.29578,
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
fn legacy_tagless_torsion_file_fails_cleanly() {
    // The exact flat layout the single-scenario GUI wrote (NO `type` key). The
    // clean-break decision: it must ERROR (DataFile, naming the missing tag), never
    // silently parse as some variant.
    let legacy = r#"
material = "Music Wire"
unit_system = "Metric"

[design]
family = "Torsion"
wire_dia_mm = 2.0
mean_dia_mm = 20.0
body_coils = 5.0
leg1_mm = 0.0
leg2_mm = 0.0
friction_model = "ShigleyFriction"
moments_nmm = [1000.0]
"#;
    match SavedDesign::from_toml(legacy) {
        Err(SpringError::DataFile(msg)) => assert!(
            msg.contains("type"),
            "clean-break error should name the missing `type` tag; got: {msg}"
        ),
        other => panic!("legacy tag-less torsion file must fail to load, got {other:?}"),
    }
}

#[test]
fn from_toml_rejects_non_finite_twoload_angle() {
    // Angles may be negative (offset-tolerant) but never non-finite; the generic
    // reject_non_finite tree-walk must cover the new angle fields.
    let toml = r#"
material = "Music Wire"
unit_system = "Metric"

[design]
family = "Torsion"
type = "TwoLoad"
wire_dia_mm = 2.0
mean_dia_mm = 20.0
leg1_mm = 0.0
leg2_mm = 0.0
friction_model = "ShigleyFriction"
moment1_nmm = 508.5
angle1_deg = inf
moment2_nmm = 1017.0
angle2_deg = 114.59156
"#;
    assert!(matches!(
        SavedDesign::from_toml(toml),
        Err(SpringError::DataFile(_))
    ));
}

#[test]
fn from_toml_rejects_non_finite_ratebased_rate() {
    let toml = r#"
material = "Music Wire"
unit_system = "Metric"

[design]
family = "Torsion"
type = "RateBased"
wire_dia_mm = 2.0
mean_dia_mm = 20.0
rate_nmm_per_deg = inf
leg1_mm = 0.0
leg2_mm = 0.0
friction_model = "PureBending"
moments_nmm = [1000.0]
"#;
    assert!(matches!(
        SavedDesign::from_toml(toml),
        Err(SpringError::DataFile(_))
    ));
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p springcore --lib torsion_ratebased legacy_tagless from_toml_rejects_non_finite_twoload`
Expected: FAIL — `TorsionSpec` has no variant syntax (it is a struct).

- [ ] **Step 3: Migrate the type + update guardrails + existing tests**

Replace the `TorsionSpec` struct (persistence.rs ~99–125) with:

```rust
/// Torsion scenario inputs (SI millimetres / newton-millimetres, as stored).
/// One variant per input mode, `type`-tagged — MIGRATED from the original flat
/// single-scenario struct (a conscious clean break: tag-less files written by the
/// single-scenario GUI no longer load; `legacy_tagless_torsion_file_fails_cleanly`
/// pins that they error rather than parse as the wrong shape).
//
// GUARDRAIL: Do NOT add `#[serde(deny_unknown_fields)]` here. The enum is
// flattened under `DesignSpec`'s `#[serde(tag = "family")]` internally-tagged
// enum; serde rejects `deny_unknown_fields` in that position because the injected
// `family` discriminant would be treated as an unknown field, breaking
// deserialization of every torsion TOML file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TorsionSpec {
    PowerUser {
        wire_dia_mm: f64,
        mean_dia_mm: f64,
        body_coils: f64,
        leg1_mm: f64,
        leg2_mm: f64,
        // The only optional field: the `toml` deserializer maps a missing key to
        // `None` for `Option` types (no `#[serde(default)]` needed), so a missing
        // or misspelled `arbor_dia_mm` deserializes to `None` rather than erroring.
        arbor_dia_mm: Option<f64>,
        friction_model: crate::torsion::FrictionModel,
        moments_nmm: Vec<f64>,
    },
    RateBased {
        wire_dia_mm: f64,
        mean_dia_mm: f64,
        /// Required angular rate in N·mm per degree — the family's mm/N·mm storage
        /// flavor and the degree-primary UI unit (exact conversion to the engine's
        /// N·m/rad via `AngularRate::from_newton_meters_per_degree(v / 1000.0)`).
        rate_nmm_per_deg: f64,
        leg1_mm: f64,
        leg2_mm: f64,
        arbor_dia_mm: Option<f64>,
        friction_model: crate::torsion::FrictionModel,
        moments_nmm: Vec<f64>,
    },
    Dimensional {
        wire_dia_mm: f64,
        outer_dia_mm: f64,
        body_coils: f64,
        leg1_mm: f64,
        leg2_mm: f64,
        arbor_dia_mm: Option<f64>,
        friction_model: crate::torsion::FrictionModel,
        moments_nmm: Vec<f64>,
    },
    TwoLoad {
        wire_dia_mm: f64,
        mean_dia_mm: f64,
        leg1_mm: f64,
        leg2_mm: f64,
        arbor_dia_mm: Option<f64>,
        friction_model: crate::torsion::FrictionModel,
        /// Two measured operating points. Angles are degrees and may be NEGATIVE
        /// (the engine's TwoLoad is offset-tolerant) but never non-finite.
        moment1_nmm: f64,
        angle1_deg: f64,
        moment2_nmm: f64,
        angle2_deg: f64,
    },
}
```

Update the EXISTING torsion tests in the same file:
- `torsion_round_trips_both_arbor_states_and_friction_models`: construct `TorsionSpec::PowerUser { … }` (same field values, variant syntax).
- `VALID_TORSION_TOML` const: add `type = "PowerUser"` as the first key under `[design]` after `family = "Torsion"`.
- `from_toml_rejects_non_finite_torsion_moment` / `_arbor`: their fixtures gain the same `type = "PowerUser"` line.
- `solve_with_material_rejects_torsion_design` (persistence.rs:1120): construct the `PowerUser` variant. The `solve_with_material` arm at ~501 (`DesignSpec::Torsion(_) => Err(…)`) already matches any variant — no change.

- [ ] **Step 4: Adapt `springmaker/src/torsion/form.rs` (compile-keeper — cross-crate lesson)**

Replacing the struct breaks form.rs's struct-literal construction/reads. Adapt minimally (Tasks 2–4 replace these with full per-scenario matches):

`build_spec` returns the PowerUser variant:

```rust
    Ok(TorsionSpec::PowerUser {
        wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
        mean_dia_mm: length_mm("mean diameter", &form.mean_dia, us)?,
        body_coils: positive_num("body coils", &form.body_coils)?,
        leg1_mm: non_negative_length_mm("leg 1", &form.leg1, us)?,
        leg2_mm: non_negative_length_mm("leg 2", &form.leg2, us)?,
        arbor_dia_mm,
        friction_model: form.friction_model,
        moments_nmm: parse_moments_nmm_nonempty(form, us)?,
    })
```

`populate_from_spec` becomes a match. The PowerUser arm populates exactly as today. The other three arms are TRANSITIONAL: populate the fields the current form has (wire, mean/N-A, legs, arbor, friction, moments where present) and carry a `// Tasks 2–4 replace this arm with full per-scenario population.` comment — no `todo!()`. Exact transitional arms:

```rust
pub fn populate_from_spec(form: &mut TorFormState, spec: &TorsionSpec, us: UnitSystem) {
    match spec {
        TorsionSpec::PowerUser {
            wire_dia_mm, mean_dia_mm, body_coils, leg1_mm, leg2_mm,
            arbor_dia_mm, friction_model, moments_nmm,
        } => {
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.mean_dia = fmt_len(*mean_dia_mm, us);
            form.body_coils = format!("{body_coils}");
            form.leg1 = fmt_len(*leg1_mm, us);
            form.leg2 = fmt_len(*leg2_mm, us);
            form.arbor_dia = match arbor_dia_mm {
                Some(v) => fmt_len(*v, us),
                None => String::new(),
            };
            form.friction_model = *friction_model;
            form.moments = fmt_moments(moments_nmm, us);
        }
        // Tasks 2–4 replace these arms with full per-scenario population (scenario
        // kind + mode-specific fields). Until then only PowerUser specs exist on
        // disk — nothing writes the other tags before those tasks land.
        TorsionSpec::RateBased {
            wire_dia_mm, leg1_mm, leg2_mm, arbor_dia_mm, friction_model, moments_nmm, ..
        }
        | TorsionSpec::Dimensional {
            wire_dia_mm, leg1_mm, leg2_mm, arbor_dia_mm, friction_model, moments_nmm, ..
        } => {
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.leg1 = fmt_len(*leg1_mm, us);
            form.leg2 = fmt_len(*leg2_mm, us);
            form.arbor_dia = match arbor_dia_mm {
                Some(v) => fmt_len(*v, us),
                None => String::new(),
            };
            form.friction_model = *friction_model;
            form.moments = fmt_moments(moments_nmm, us);
        }
        TorsionSpec::TwoLoad {
            wire_dia_mm, mean_dia_mm, leg1_mm, leg2_mm, arbor_dia_mm, friction_model, ..
        } => {
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.mean_dia = fmt_len(*mean_dia_mm, us);
            form.leg1 = fmt_len(*leg1_mm, us);
            form.leg2 = fmt_len(*leg2_mm, us);
            form.arbor_dia = match arbor_dia_mm {
                Some(v) => fmt_len(*v, us),
                None => String::new(),
            };
            form.friction_model = *friction_model;
        }
    }
}
```

form.rs tests: the existing round-trip tests keep passing (they exercise the PowerUser path). Add nothing here yet.

- [ ] **Step 5: Run to verify all pass**

Run: `cargo test -p springcore --lib && cargo test -p springmaker`
Expected: PASS (all existing + 3 new springcore tests).

- [ ] **Step 6: Mutation-check + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git diff origin/main -- > /tmp/pr.diff
cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features
# Expected: 0 survivors (data variants + tests; the solve_with_material arm keeps its test).
git add springcore/src/persistence.rs springmaker/src/torsion/form.rs
git commit -m "feat(persistence): TorsionSpec tagged-enum migration — clean break for legacy files"
```

---

### Task 2: Scenario picker + RateBased mode

**Files:**
- Modify: `springmaker/src/torsion/form.rs` (scenario enum; `rate` field; per-scenario matches)
- Modify: `springmaker/src/form_helpers.rs` (`ang_rate_nmm_per_deg`, `fmt_ang_rate_nmm_per_deg` + tests)
- Modify: `springmaker/src/app.rs` (Message::TorScenario; update arm; set_tor_field Rate arm)
- Modify: `springmaker/src/torsion/view_model.rs` (`tor_inputs_view` per-scenario)
- Modify: `springmaker/src/torsion/view.rs` (scenario pick-list; Rate field map + id)

**Interfaces:**
- Produces: `TorScenarioKind { PowerUser, RateBased, Dimensional, TwoLoad }` + `ALL_TOR_SCENARIOS` + `Display` ("Power User"/"Rate Based"/"Dimensional"/"Two Load"); `TorFormState { scenario, rate, … }`; `Field::Rate`; `Message::TorScenario(TorScenarioKind)`; helpers `ang_rate_nmm_per_deg(field, value, us) -> Result<f64>` (strictly positive; metric N·mm/° passthrough, US lbf·in/° via `Moment::from_pound_force_inches(v).newton_millimeters()`; `finite_or_err` after conversion) and `fmt_ang_rate_nmm_per_deg(nmm_per_deg, us) -> String`.
- Consumes: Task 1's `TorsionSpec::{PowerUser, RateBased}` variants; engine `springcore::torsion::RateBased`; `AngularRate::from_newton_meters_per_degree`.

**Scenario enum + threading (form.rs).** Add after the imports:

```rust
/// Which torsion input scenario is active. The torsion family's own enum — the
/// module boundary forbids importing the sibling families' kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TorScenarioKind {
    #[default]
    PowerUser,
    RateBased,
    Dimensional,
    TwoLoad,
}

/// All `TorScenarioKind` variants in display order.
pub const ALL_TOR_SCENARIOS: &[TorScenarioKind] = &[
    TorScenarioKind::PowerUser,
    TorScenarioKind::RateBased,
    TorScenarioKind::Dimensional,
    TorScenarioKind::TwoLoad,
];

impl std::fmt::Display for TorScenarioKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            TorScenarioKind::PowerUser => "Power User",
            TorScenarioKind::RateBased => "Rate Based",
            TorScenarioKind::Dimensional => "Dimensional",
            TorScenarioKind::TwoLoad => "Two Load",
        })
    }
}
```

`TorFormState` gains `pub scenario: TorScenarioKind,` (first field) and `pub rate: String,`. `Field` gains `Rate`. **Transitional Dimensional/TwoLoad arms in this task** (the picker is live for all four modes from here, so these arms ARE reachable): `parse_and_solve` and `build_spec` return `Err(springcore::SpringError::InconsistentInputs("this input mode arrives in a later task".into()))` — honest named errors, no panic, no silent-PowerUser fallback; `is_blank` and `tor_inputs_view` use the PowerUser field set transitionally. All four are replaced with real arms in Task 3, each marked `// Task 3 replaces this arm.`

**is_blank** becomes:

```rust
    pub fn is_blank(&self) -> bool {
        let all_empty = |fields: &[&String]| fields.iter().all(|f| f.trim().is_empty());
        match self.scenario {
            // Task 3 replaces the Dimensional/TwoLoad arms with their own field sets.
            TorScenarioKind::PowerUser
            | TorScenarioKind::Dimensional
            | TorScenarioKind::TwoLoad => all_empty(&[
                &self.wire_dia,
                &self.mean_dia,
                &self.body_coils,
                &self.leg1,
                &self.leg2,
                &self.arbor_dia,
                &self.moments,
            ]),
            TorScenarioKind::RateBased => all_empty(&[
                &self.wire_dia,
                &self.mean_dia,
                &self.rate,
                &self.leg1,
                &self.leg2,
                &self.arbor_dia,
                &self.moments,
            ]),
        }
    }
```

**parse_and_solve** becomes a match; PowerUser arm is the existing body; RateBased arm:

```rust
        TorScenarioKind::RateBased => {
            let scenario = springcore::torsion::RateBased {
                wire_dia: Length::from_millimeters(length_mm("wire diameter", &form.wire_dia, us)?),
                mean_dia: Length::from_millimeters(length_mm("mean diameter", &form.mean_dia, us)?),
                rate: AngularRate::from_newton_meters_per_degree(
                    ang_rate_nmm_per_deg("rate", &form.rate, us)? / 1000.0,
                ),
                leg1: Length::from_millimeters(non_negative_length_mm("leg 1", &form.leg1, us)?),
                leg2: Length::from_millimeters(non_negative_length_mm("leg 2", &form.leg2, us)?),
                arbor_dia: parse_arbor(form, us)?,
                moments: parse_moments_nmm_nonempty(form, us)?
                    .into_iter()
                    .map(Moment::from_newton_millimeters)
                    .collect(),
            };
            Ok(TorFormOutcome {
                design: scenario.solve(material, form.friction_model)?,
            })
        }
```

(`use springcore::units::AngularRate;` joins the units import.) **build_spec** RateBased arm mirrors it, producing `TorsionSpec::RateBased { rate_nmm_per_deg: ang_rate_nmm_per_deg("rate", &form.rate, us)?, … }`. **populate_from_spec**: the RateBased arm becomes FULL population — sets `form.scenario = TorScenarioKind::RateBased`, `form.rate = fmt_ang_rate_nmm_per_deg(*rate_nmm_per_deg, us)`, plus the shared fields incl. `mean_dia`; the PowerUser arm sets `form.scenario = TorScenarioKind::PowerUser`.

**form_helpers.rs** additions (after `rate_npm`):

```rust
/// Parse a strictly-positive angular rate, returning N·mm per degree (canonical):
/// metric input is already N·mm/°; US input is lbf·in/°, converted via `Moment`.
pub(crate) fn ang_rate_nmm_per_deg(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    let v = positive_num(field, value)?;
    let v_si = match us {
        UnitSystem::Us => Moment::from_pound_force_inches(v).newton_millimeters(),
        UnitSystem::Metric => v,
    };
    finite_or_err(field, value, v_si)
}

/// Convert N·mm/° (canonical) → display string (metric N·mm/°, US lbf·in/°).
pub(crate) fn fmt_ang_rate_nmm_per_deg(nmm_per_deg: f64, us: UnitSystem) -> String {
    match us {
        UnitSystem::Metric => format!("{nmm_per_deg}"),
        UnitSystem::Us => format!(
            "{}",
            Moment::from_newton_millimeters(nmm_per_deg).pound_force_inches()
        ),
    }
}
```

with helper tests: metric passthrough + `0`/negative rejected; US `1` lbf·in/° → `112.98482…` N·mm/° (`4.4482216152605 * 0.0254 * 1000.0`, `assert_relative_eq` 1e-9); metric/US `fmt` round-trips.

**app.rs:** `Message` gains `TorScenario(crate::torsion::form::TorScenarioKind),` (after `TorFriction`); `update()` gains `Message::TorScenario(s) => { self.torsion.scenario = s; true }`; `set_tor_field` gains `TF::Rate => f.rate = value,`.

**view_model.rs `tor_inputs_view`** becomes a per-scenario match; PowerUser (and transitionally Dimensional/TwoLoad) return today's seven; RateBased swaps `BodyCoils` for `FieldDescriptor::new(format!("Rate ({moment}/°)"), Field::Rate)` (after MeanDia).

**view.rs:** the design panel Setup group gains, ABOVE the friction pick-list:

```rust
        column![
            field_label("Input mode"),
            styled_pick_list(
                crate::torsion::form::ALL_TOR_SCENARIOS,
                Some(app.torsion.scenario),
                Message::TorScenario,
            ),
        ]
        .spacing(4),
```

`tor_field_value` gains `Field::Rate => &form.rate,`; `tor_field_id` gains `Field::Rate => "tor-rate",`.

- [ ] **Step 1: Write the failing tests** — form.rs tests to add:

```rust
    fn ratebased_metric_form() -> TorFormState {
        TorFormState {
            scenario: TorScenarioKind::RateBased,
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            rate: format!("{}", 0.5085_f64 * 1000.0 * std::f64::consts::PI / 180.0),
            leg1: "0".into(),
            leg2: "0".into(),
            moments: "1000".into(),
            ..TorFormState::default()
        }
    }

    #[test]
    fn ratebased_derives_body_coils_and_round_trips_rate() {
        let out = parse_and_solve(&ratebased_metric_form(), "Music Wire", UnitSystem::Metric, &store())
            .expect("RateBased should solve");
        assert_relative_eq!(out.design.inputs.body_coils, 5.0, max_relative = 1e-9);
        assert_relative_eq!(
            out.design.rate.newton_meters_per_radian(),
            0.5085,
            max_relative = 1e-9
        );
    }

    #[test]
    fn ratebased_build_spec_populate_round_trip() {
        for us in [UnitSystem::Metric, UnitSystem::Us] {
            let spec = build_spec(&ratebased_metric_form(), us).unwrap();
            let mut form2 = TorFormState::default();
            populate_from_spec(&mut form2, &spec, us);
            assert_eq!(form2.scenario, TorScenarioKind::RateBased);
            assert_eq!(build_spec(&form2, us).unwrap(), spec);
        }
    }

    #[test]
    fn is_blank_ratebased_trips_on_rate() {
        let mut f = TorFormState {
            scenario: TorScenarioKind::RateBased,
            ..TorFormState::default()
        };
        assert!(f.is_blank(), "untouched RateBased form is blank");
        f.rate = "8.9".into();
        assert!(!f.is_blank(), "entering the rate clears blank");
    }
```

view_model tests: RateBased inputs list contains `Field::Rate`, not `Field::BodyCoils`; label contains "/°".

- [ ] **Step 2: Run to verify they fail** — `cargo test -p springmaker torsion` → FAIL (no `TorScenarioKind`).
- [ ] **Step 3: Implement** all the code above.
- [ ] **Step 4: Run to verify pass** — `cargo test -p springmaker` → PASS; `cargo test -p springcore --lib` unchanged-green.
- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add springmaker/src/torsion/form.rs springmaker/src/form_helpers.rs springmaker/src/app.rs springmaker/src/torsion/view_model.rs springmaker/src/torsion/view.rs
git commit -m "feat(gui): torsion scenario picker + RateBased input mode"
```

---

### Task 3: Dimensional + TwoLoad modes

**Files:**
- Modify: `springmaker/src/torsion/form.rs` (fields `outer_dia`, `moment1/angle1/moment2/angle2`; `dimensional_mean_check`; real Dimensional/TwoLoad arms everywhere)
- Modify: `springmaker/src/form_helpers.rs` (`angle_deg`, `fmt_angle_deg` + tests)
- Modify: `springmaker/src/app.rs` (`set_tor_field` arms), `view_model.rs` (scenario input lists), `view.rs` (field maps + ids)

**Interfaces:**
- Consumes: Task 2's threading; Task 1's `TorsionSpec::{Dimensional, TwoLoad}` shapes; engine `springcore::torsion::{Dimensional, TwoLoad}`; `Angle::from_degrees`.
- Produces: `TorFormState` gains `pub outer_dia: String, pub moment1: String, pub angle1: String, pub moment2: String, pub angle2: String`; `Field::{OuterDia, Moment1, Angle1, Moment2, Angle2}`; ids `tor-outer-dia`, `tor-moment1`, `tor-angle1`, `tor-moment2`, `tor-angle2`; `angle_deg(field, value) -> Result<f64>` (any finite; `num` only — degrees both unit systems, NO unit conversion) and `fmt_angle_deg(deg) -> String` (`format!("{deg}")`); private `fn dimensional_mean_check(wire_dia_mm: f64, outer_dia_mm: f64) -> Result<()>` rejecting `outer − wire ≤ 0` with `"outer diameter must be greater than wire diameter"`.

**dimensional_mean_check** (form.rs, mirroring extension's `dimensional_mean_mm` rationale — reject at the form boundary against the field the user typed; the engine's OD/mean/index guards remain the backstop; applied in BOTH `parse_and_solve` and `build_spec` so an unloadable spec is never persisted):

```rust
fn dimensional_mean_check(wire_dia_mm: f64, outer_dia_mm: f64) -> Result<()> {
    if outer_dia_mm - wire_dia_mm <= 0.0 {
        return Err(springcore::SpringError::InconsistentInputs(
            "outer diameter must be greater than wire diameter".into(),
        ));
    }
    Ok(())
}
```

**parse_and_solve** arms (replacing Task 2's transitional errors):

```rust
        TorScenarioKind::Dimensional => {
            let wire_dia_mm = length_mm("wire diameter", &form.wire_dia, us)?;
            let outer_dia_mm = length_mm("outer diameter", &form.outer_dia, us)?;
            dimensional_mean_check(wire_dia_mm, outer_dia_mm)?;
            let scenario = springcore::torsion::Dimensional {
                wire_dia: Length::from_millimeters(wire_dia_mm),
                outer_dia: Length::from_millimeters(outer_dia_mm),
                body_coils: positive_num("body coils", &form.body_coils)?,
                leg1: Length::from_millimeters(non_negative_length_mm("leg 1", &form.leg1, us)?),
                leg2: Length::from_millimeters(non_negative_length_mm("leg 2", &form.leg2, us)?),
                arbor_dia: parse_arbor(form, us)?,
                moments: parse_moments_nmm_nonempty(form, us)?
                    .into_iter()
                    .map(Moment::from_newton_millimeters)
                    .collect(),
            };
            Ok(TorFormOutcome {
                design: scenario.solve(material, form.friction_model)?,
            })
        }
        TorScenarioKind::TwoLoad => {
            let scenario = springcore::torsion::TwoLoad {
                wire_dia: Length::from_millimeters(length_mm("wire diameter", &form.wire_dia, us)?),
                mean_dia: Length::from_millimeters(length_mm("mean diameter", &form.mean_dia, us)?),
                leg1: Length::from_millimeters(non_negative_length_mm("leg 1", &form.leg1, us)?),
                leg2: Length::from_millimeters(non_negative_length_mm("leg 2", &form.leg2, us)?),
                arbor_dia: parse_arbor(form, us)?,
                point1: (
                    Moment::from_newton_millimeters(moment_nmm("moment 1", &form.moment1, us)?),
                    Angle::from_degrees(angle_deg("angle 1", &form.angle1)?),
                ),
                point2: (
                    Moment::from_newton_millimeters(moment_nmm("moment 2", &form.moment2, us)?),
                    Angle::from_degrees(angle_deg("angle 2", &form.angle2)?),
                ),
            };
            Ok(TorFormOutcome {
                design: scenario.solve(material, form.friction_model)?,
            })
        }
```

(`moment_nmm` is the existing single-moment helper; `Angle` joins the units import.) **build_spec** arms mirror these into `TorsionSpec::Dimensional { outer_dia_mm, … }` (with `dimensional_mean_check` FIRST) and `TorsionSpec::TwoLoad { moment1_nmm, angle1_deg, … }`. **populate_from_spec**: replace Task 1's transitional arms with full population (sets `scenario`, `outer_dia` / the four point fields via `fmt_moment`/`fmt_angle_deg`). **is_blank**: Dimensional arm = {wire, outer_dia, body_coils, legs, arbor, moments}; TwoLoad arm = {wire, mean, legs, arbor, moment1, angle1, moment2, angle2}. **tor_inputs_view**: Dimensional swaps MeanDia→`Outer diameter ({len})`/`Field::OuterDia`; TwoLoad = wire, mean, legs, arbor + `Moment 1 ({moment})`, `Angle 1 (°)`, `Moment 2 ({moment})`, `Angle 2 (°)` (no Moments field). **app.rs set_tor_field** + **view.rs** maps/ids extended for the five new fields.

**angle_deg / fmt_angle_deg** (form_helpers.rs):

```rust
/// Parse an angle in degrees: any FINITE number (TwoLoad is offset-tolerant, so
/// negative and zero angles are legal). Degrees in both unit systems.
pub(crate) fn angle_deg(field: &str, value: &str) -> Result<f64> {
    num(field, value)
}

/// Format an angle in degrees for form population.
pub(crate) fn fmt_angle_deg(deg: f64) -> String {
    format!("{deg}")
}
```

with tests: `-10` accepted, `nan`/`inf` rejected, `fmt` round-trip.

- [ ] **Step 1: failing tests** (form.rs):

```rust
    #[test]
    fn dimensional_matches_power_user_geometry() {
        let form = TorFormState {
            scenario: TorScenarioKind::Dimensional,
            wire_dia: "2".into(),
            outer_dia: "22".into(), // mean = 20
            body_coils: "5".into(),
            leg1: "0".into(),
            leg2: "0".into(),
            moments: "1000".into(),
            ..TorFormState::default()
        };
        let out = parse_and_solve(&form, "Music Wire", UnitSystem::Metric, &store()).unwrap();
        assert_relative_eq!(out.design.index, 10.0, max_relative = 1e-9);
    }

    #[test]
    fn dimensional_outer_not_greater_than_wire_rejected_both_sites() {
        // The owed field-named boundary error, at BOTH call sites, metric and US.
        for us in [UnitSystem::Metric, UnitSystem::Us] {
            let form = TorFormState {
                scenario: TorScenarioKind::Dimensional,
                wire_dia: "2".into(),
                outer_dia: "2".into(), // mean = 0
                body_coils: "5".into(),
                leg1: "0".into(),
                leg2: "0".into(),
                moments: "1000".into(),
                ..TorFormState::default()
            };
            for err in [
                parse_and_solve(&form, "Music Wire", us, &store()).unwrap_err(),
                build_spec(&form, us).unwrap_err(),
            ] {
                assert!(
                    err.to_string().contains("outer diameter must be greater than wire diameter"),
                    "expected the form-boundary outer>wire error ({us:?}); got: {err}"
                );
            }
        }
    }

    fn twoload_metric_form() -> TorFormState {
        // Two points on the oracle k' = 0.5085 N·m/rad line, in display units:
        // (508.5 N·mm, 1 rad = 57.29578°), (1017 N·mm, 2 rad = 114.59156°).
        TorFormState {
            scenario: TorScenarioKind::TwoLoad,
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            leg1: "0".into(),
            leg2: "0".into(),
            moment1: "508.5".into(),
            angle1: format!("{}", 180.0_f64 / std::f64::consts::PI),
            moment2: "1017".into(),
            angle2: format!("{}", 2.0_f64 * 180.0 / std::f64::consts::PI),
            ..TorFormState::default()
        }
    }

    #[test]
    fn twoload_derives_rate_and_body_coils_from_points() {
        let out = parse_and_solve(&twoload_metric_form(), "Music Wire", UnitSystem::Metric, &store())
            .expect("TwoLoad should solve");
        assert_relative_eq!(out.design.rate.newton_meters_per_radian(), 0.5085, max_relative = 1e-9);
        assert_relative_eq!(out.design.inputs.body_coils, 5.0, max_relative = 1e-9);
        assert_eq!(out.design.load_points.len(), 2);
    }

    #[test]
    fn twoload_degenerate_points_surface_engine_message() {
        let form = TorFormState {
            angle2: twoload_metric_form().angle1.clone(), // same angle both points
            ..twoload_metric_form()
        };
        let err = parse_and_solve(&form, "Music Wire", UnitSystem::Metric, &store()).unwrap_err();
        assert!(
            err.to_string().contains("different angles"),
            "engine degenerate-point message must surface; got: {err}"
        );
    }

    #[test]
    fn dimensional_and_twoload_round_trip_and_blank() {
        for us in [UnitSystem::Metric, UnitSystem::Us] {
            for form in [
                TorFormState {
                    scenario: TorScenarioKind::Dimensional,
                    wire_dia: "2".into(),
                    outer_dia: "22".into(),
                    body_coils: "5".into(),
                    leg1: "0".into(),
                    leg2: "0".into(),
                    moments: "1000".into(),
                    ..TorFormState::default()
                },
                twoload_metric_form(),
            ] {
                let spec = build_spec(&form, us).unwrap();
                let mut form2 = TorFormState::default();
                populate_from_spec(&mut form2, &spec, us);
                assert_eq!(form2.scenario, form.scenario);
                assert_eq!(build_spec(&form2, us).unwrap(), spec);
            }
        }
        let mut blank = TorFormState {
            scenario: TorScenarioKind::TwoLoad,
            ..TorFormState::default()
        };
        assert!(blank.is_blank());
        blank.angle1 = "-10".into(); // negative angle is a legal, intent-signaling entry
        assert!(!blank.is_blank(), "typing a TwoLoad point field clears blank");
    }
```

- [ ] **Step 2: verify fail** — `cargo test -p springmaker torsion` → FAIL (no `outer_dia` field).
- [ ] **Step 3: implement** everything above (fields, helpers, arms, view/app wiring).
- [ ] **Step 4: verify pass** — `cargo test -p springmaker` + `cargo test -p springcore --lib` → PASS.
- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add springmaker/src/torsion/form.rs springmaker/src/form_helpers.rs springmaker/src/app.rs springmaker/src/torsion/view_model.rs springmaker/src/torsion/view.rs
git commit -m "feat(gui): torsion Dimensional + TwoLoad input modes"
```

---

### Task 4: Force-at-radius entry toggle

**Files:**
- Modify: `springmaker/src/torsion/form.rs` (`MomentEntry`, `forces`/`load_radius` fields, shared moment parsing, is_blank, populate resets)
- Modify: `springmaker/src/app.rs` (`Message::TorMomentEntry`; `set_tor_field` arms)
- Modify: `springmaker/src/torsion/view_model.rs` (inputs swap in F@r mode), `view.rs` (selector + field maps/ids)

**Interfaces:**
- Consumes: `springcore::torsion::moment_from_force_at_radius`; `Force::from_newtons`; existing `positive_force_n`, `length_mm`.
- Produces: `MomentEntry { Direct, ForceAtRadius }` + `ALL_MOMENT_ENTRIES` + `Display` ("Moments" / "Force @ radius"); `Field::{Forces, LoadRadius}`; ids `tor-forces`, `tor-load-radius`; private `fn parse_applied_moments_nmm(form: &TorFormState, us: UnitSystem) -> Result<Vec<f64>>`.

**form.rs additions:**

```rust
/// How applied moments are entered: directly, or as forces on a leg at one radius
/// (`M = F·r`, converted at the form boundary — the choice is NOT persisted; specs
/// always store the derived moments).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MomentEntry {
    #[default]
    Direct,
    ForceAtRadius,
}

/// All `MomentEntry` variants in display order.
pub const ALL_MOMENT_ENTRIES: &[MomentEntry] = &[MomentEntry::Direct, MomentEntry::ForceAtRadius];

impl std::fmt::Display for MomentEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            MomentEntry::Direct => "Moments",
            MomentEntry::ForceAtRadius => "Force @ radius",
        })
    }
}
```

`TorFormState` gains `pub moment_entry: MomentEntry, pub forces: String, pub load_radius: String`. `Field` gains `Forces, LoadRadius`. Replace every `parse_moments_nmm_nonempty(form, us)` call site (PowerUser/RateBased/Dimensional arms in BOTH `parse_and_solve` and `build_spec`) with `parse_applied_moments_nmm(form, us)`:

```rust
/// The applied-moment list per the active entry mode: Direct parses the moments
/// field; ForceAtRadius derives each moment as `M = F·r` (engine helper, cited)
/// from strictly-positive forces at one strictly-positive load radius. Both modes
/// reject an empty list at the form boundary.
fn parse_applied_moments_nmm(form: &TorFormState, us: UnitSystem) -> Result<Vec<f64>> {
    match form.moment_entry {
        MomentEntry::Direct => parse_moments_nmm_nonempty(form, us),
        MomentEntry::ForceAtRadius => {
            let radius_mm = length_mm("load radius", &form.load_radius, us)?;
            let moments: Vec<f64> = form
                .forces
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| {
                    let force_n = positive_force_n("force", s, us)?;
                    Ok(springcore::torsion::moment_from_force_at_radius(
                        Force::from_newtons(force_n),
                        Length::from_millimeters(radius_mm),
                    )
                    .newton_millimeters())
                })
                .collect::<Result<_>>()?;
            if moments.is_empty() {
                return Err(springcore::SpringError::InconsistentInputs(
                    "provide at least one applied force".into(),
                ));
            }
            Ok(moments)
        }
    }
}
```

(`Force` joins the units import; `positive_force_n` joins the form_helpers import.) **is_blank**: add `&self.forces, &self.load_radius` to the PowerUser/RateBased/Dimensional arms (NOT TwoLoad); `moment_entry` is a default-holding selector, excluded. **populate_from_spec**: every arm sets `form.moment_entry = MomentEntry::Direct; form.forces = String::new(); form.load_radius = String::new();` (no stale-field leak — extension's hook-radii rule). **app.rs**: `Message::TorMomentEntry(crate::torsion::form::MomentEntry),` + `update()` arm (`self.torsion.moment_entry = m; true`) + `set_tor_field` `TF::Forces`/`TF::LoadRadius` arms. **view_model `tor_inputs_view`**: in the three moments-scenario arms, when `app.torsion.moment_entry == MomentEntry::ForceAtRadius` replace the Moments descriptor with `Forces ({force}), comma-separated` / `Field::Forces` and `Load radius ({len})` / `Field::LoadRadius` (uses `unit_force_label`). **view.rs**: below the scenario pick-list, shown only when `app.torsion.scenario != TorScenarioKind::TwoLoad`:

```rust
        column![
            field_label("Moment entry"),
            styled_pick_list(
                crate::torsion::form::ALL_MOMENT_ENTRIES,
                Some(app.torsion.moment_entry),
                Message::TorMomentEntry,
            ),
        ]
        .spacing(4),
```

plus `tor_field_value`/`tor_field_id` arms (`tor-forces`, `tor-load-radius`).

- [ ] **Step 1: failing tests** (form.rs):

```rust
    #[test]
    fn force_at_radius_equals_direct_moments() {
        // 10 N @ 50 mm ≡ 500 N·mm — identical solve AND identical persisted spec.
        let far = TorFormState {
            moment_entry: MomentEntry::ForceAtRadius,
            forces: "10".into(),
            load_radius: "50".into(),
            moments: String::new(),
            ..metric_form()
        };
        let direct = TorFormState {
            moments: "500".into(),
            ..metric_form()
        };
        let out_far =
            parse_and_solve(&far, "Music Wire", UnitSystem::Metric, &store()).unwrap();
        let out_direct =
            parse_and_solve(&direct, "Music Wire", UnitSystem::Metric, &store()).unwrap();
        assert_relative_eq!(
            out_far.design.load_points[0].moment.newton_millimeters(),
            out_direct.design.load_points[0].moment.newton_millimeters(),
            max_relative = 1e-12
        );
        assert_eq!(
            build_spec(&far, UnitSystem::Metric).unwrap(),
            build_spec(&direct, UnitSystem::Metric).unwrap(),
            "F@r persists the derived moments — specs must be identical"
        );
    }

    #[test]
    fn force_at_radius_empty_forces_rejected() {
        let f = TorFormState {
            moment_entry: MomentEntry::ForceAtRadius,
            forces: String::new(),
            load_radius: "50".into(),
            moments: String::new(),
            ..metric_form()
        };
        let err = parse_and_solve(&f, "Music Wire", UnitSystem::Metric, &store()).unwrap_err();
        assert!(
            err.to_string().contains("provide at least one applied force"),
            "expected the F@r non-empty guard; got: {err}"
        );
    }

    #[test]
    fn populate_resets_moment_entry_and_clears_far_fields() {
        let far = TorFormState {
            moment_entry: MomentEntry::ForceAtRadius,
            forces: "10".into(),
            load_radius: "50".into(),
            moments: String::new(),
            ..metric_form()
        };
        let spec = build_spec(&far, UnitSystem::Metric).unwrap();
        let mut form2 = far.clone();
        populate_from_spec(&mut form2, &spec, UnitSystem::Metric);
        assert_eq!(form2.moment_entry, MomentEntry::Direct);
        assert!(form2.forces.is_empty() && form2.load_radius.is_empty());
        assert_eq!(form2.moments, "500", "derived moments shown in Direct mode");
    }

    #[test]
    fn is_blank_trips_on_far_fields_but_not_selector() {
        let mut f = TorFormState {
            moment_entry: MomentEntry::ForceAtRadius,
            ..TorFormState::default()
        };
        assert!(f.is_blank(), "selector alone cannot distinguish blank");
        f.forces = "10".into();
        assert!(!f.is_blank(), "typing a force clears blank");
    }
```

- [ ] **Step 2: verify fail** → FAIL (no `MomentEntry`).
- [ ] **Step 3: implement**; **Step 4: verify pass** (`cargo test -p springmaker`); **Step 5: commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add springmaker/src/torsion/form.rs springmaker/src/app.rs springmaker/src/torsion/view_model.rs springmaker/src/torsion/view.rs
git commit -m "feat(gui): torsion force-at-radius moment entry toggle"
```

---

### Task 5: Simulator E2E + full gate (+ final review by the controller)

**Files:**
- Modify: `springmaker/src/ui_tests.rs`

**Interfaces:**
- Consumes: the full wired feature; existing helpers `test_app()`, `type_into_tor(Field)`, `shows()`; `Message::{TorScenario, TorMomentEntry, SelectFamily}`.

- [ ] **Step 1: Write the E2E tests** (mirror the existing torsion E2E section; `typewrite` APPENDS — type each field once):

```rust
#[test]
fn torsion_scenario_switch_solves_each_mode() {
    use crate::torsion::form::{Field as TF, TorScenarioKind};
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Torsion));

    // RateBased through real widgets.
    app.update(Message::TorScenario(TorScenarioKind::RateBased));
    type_into_tor(&mut app, TF::WireDia, "2");
    type_into_tor(&mut app, TF::MeanDia, "20");
    type_into_tor(&mut app, TF::Rate, "8.875");
    type_into_tor(&mut app, TF::Leg1, "0");
    type_into_tor(&mut app, TF::Leg2, "0");
    type_into_tor(&mut app, TF::Moments, "1000");
    assert!(app.tor_outcome.is_some(), "RateBased must solve");

    // Dimensional: switch + fill its distinct field (shared fields carry over).
    app.update(Message::TorScenario(TorScenarioKind::Dimensional));
    type_into_tor(&mut app, TF::BodyCoils, "5");
    type_into_tor(&mut app, TF::OuterDia, "22");
    assert!(app.tor_outcome.is_some(), "Dimensional must solve");

    // TwoLoad: switch + the four point fields.
    app.update(Message::TorScenario(TorScenarioKind::TwoLoad));
    type_into_tor(&mut app, TF::Moment1, "508.5");
    type_into_tor(&mut app, TF::Angle1, "57.29578");
    type_into_tor(&mut app, TF::Moment2, "1017");
    type_into_tor(&mut app, TF::Angle2, "114.59156");
    assert!(app.tor_outcome.is_some(), "TwoLoad must solve");
    assert!(app.error.is_none());
}

#[test]
fn torsion_force_at_radius_e2e() {
    use crate::torsion::form::{Field as TF, MomentEntry};
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Torsion));
    type_into_tor(&mut app, TF::WireDia, "2");
    type_into_tor(&mut app, TF::MeanDia, "20");
    type_into_tor(&mut app, TF::BodyCoils, "5");
    type_into_tor(&mut app, TF::Leg1, "0");
    type_into_tor(&mut app, TF::Leg2, "0");
    app.update(Message::TorMomentEntry(MomentEntry::ForceAtRadius));
    type_into_tor(&mut app, TF::Forces, "10");
    type_into_tor(&mut app, TF::LoadRadius, "50");
    assert!(app.tor_outcome.is_some(), "F@r entry must solve");
}

#[test]
fn torsion_mode_save_load_round_trips() {
    use crate::torsion::form::{TorFormState, TorScenarioKind};
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Torsion));
    app.torsion = TorFormState {
        scenario: TorScenarioKind::RateBased,
        wire_dia: "2".into(),
        mean_dia: "20".into(),
        rate: "8.875".into(),
        leg1: "0".into(),
        leg2: "0".into(),
        moments: "1000".into(),
        ..TorFormState::default()
    };
    app.recompute();
    let dir = std::env::temp_dir().join(format!("osm_tor_modes_e2e_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("ratebased.toml");
    app.save_to(&path);
    let mut app2 = test_app();
    assert!(app2.load_from(&path));
    assert_eq!(app2.family, Family::Torsion);
    assert_eq!(app2.torsion.scenario, TorScenarioKind::RateBased);
    assert_eq!(app2.torsion.rate, "8.875");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn legacy_tagless_torsion_file_surfaces_clean_break_error() {
    // A file in the pre-migration flat layout must fail to load with the error in
    // `action_error` (status panel), leaving the current form untouched.
    let legacy = r#"
material = "Music Wire"
unit_system = "Metric"

[design]
family = "Torsion"
wire_dia_mm = 2.0
mean_dia_mm = 20.0
body_coils = 5.0
leg1_mm = 0.0
leg2_mm = 0.0
friction_model = "ShigleyFriction"
moments_nmm = [1000.0]
"#;
    let dir = std::env::temp_dir().join(format!("osm_tor_legacy_e2e_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("legacy.toml");
    std::fs::write(&path, legacy).unwrap();
    let mut app = test_app();
    assert!(!app.load_from(&path), "legacy file must fail to load");
    assert!(
        app.action_error.as_deref().is_some_and(|m| m.contains("type")),
        "the clean-break error (missing `type` tag) must surface in action_error"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
```

- [ ] **Step 2: Run** — `cargo test -p springmaker torsion` → PASS.
- [ ] **Step 3: Full local gate**

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
typos
cargo test --workspace --all-features
git diff origin/main -- > /tmp/pr.diff
cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features
# Expected: all green; springcore mutation literal 0 survivors.
```

- [ ] **Step 4: Commit**

```bash
git add springmaker/src/ui_tests.rs
git commit -m "test(gui): torsion mode-switch, F@r, save/load, and legacy clean-break E2E"
```

- [ ] **Step 5: Final whole-branch review** — the controller dispatches the mandatory adversarial panel (general-code, architect, simplifier, MANDATORY input-domain adversary, persistence/wire-format reviewer for the migration), cycles to convergence, then pushes and opens the PR.

---

## Notes for the implementer

- **Cross-crate compile order:** Task 1's form.rs adaptation is NOT optional — replacing the springcore struct breaks springmaker otherwise, and later tasks' tests couldn't run (established lesson).
- **Transitional arms are honest, never `todo!()`:** Task 2's Dimensional/TwoLoad `parse_and_solve`/`build_spec` arms return a named `InconsistentInputs` error; Task 3 replaces them. The picker is live for all four modes from Task 2, so those arms ARE reachable — an error is correct, a panic or silent-PowerUser fallback is not.
- **`tor_field_id` stays the single id source** — Task 5's Simulator resolves ids through it; never hardcode an id string in a test.
- **Results/status panels don't change** in any task; every scenario produces the same `TorsionDesign`.
- **Populate always resets `moment_entry` to Direct and clears `forces`/`load_radius`** — the F@r toggle is a pure input convenience (decision 2); a persisted spec never encodes it.
