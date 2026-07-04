# Torsion-Family GUI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a torsion spring family to the springmaker GUI — enter a PowerUser torsion design, solve it, view angular results, and save/load it — reaching parity with the compression and extension families.

**Architecture:** Two layers. springcore gains additive, mutation-gated data: two `AngularRate` US accessors, `serde` on `FrictionModel`, a `Family::Torsion` variant, and a `DesignSpec::Torsion(TorsionSpec)` persistence struct. springmaker gains a new `torsion/` family module (`mod`/`form`/`view_model`/`view`) mirroring `extension/`'s presenter/humble-view split (ADR 0008), plus dispatch wiring in `app.rs` and `calculator.rs`. Torsion is single-scenario (PowerUser) so there is no scenario-picker enum.

**Tech Stack:** Rust (MSRV 1.88), iced 0.14, serde/toml, approx (test asserts), iced_test `Simulator` (E2E).

## Global Constraints

- springcore additions (Tasks 1, 2) are mutation-gated to **literal 0 survivors** via `git diff origin/main -- > /tmp/pr.diff && cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features`. springmaker (Tasks 3–6) is NOT mutation-gated.
- SI-canonical engine; unit conversion happens only at the form boundary (`form_helpers`).
- Presenter/humble-view split per ADR 0008: `form.rs` + `view_model.rs` are pure (no iced); `view.rs` is the only iced-dependent file.
- One-way module boundary: `torsion/` never imports `compression/` or `extension/`; it depends only on `springcore`, `form_helpers`, `presenter`, `widgets`.
- No commercial product or vendor names in any file (legal).
- Moment display unit: N·mm (metric) / lbf·in (US). Angular deflection & rate: degrees primary + revolutions secondary.
- Local gate before every push: `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features`, `typos`, `cargo test --workspace --all-features`, in-diff mutation (springcore).

---

## File Structure

**springcore (Tasks 1–2, mutation-gated):**
- Modify `springcore/src/units.rs` — 2 new `AngularRate` US read accessors + tests.
- Modify `springcore/src/torsion/mechanics.rs` — add `Serialize, Deserialize` to `FrictionModel`.
- Modify `springcore/src/family.rs` — `Family::Torsion` variant, Display arm, `ALL_FAMILIES`, tests.
- Modify `springcore/src/persistence.rs` — `TorsionSpec` struct + `DesignSpec::Torsion` variant + round-trip / non-finite tests.

**springmaker (Tasks 3–6, not mutation-gated):**
- Modify `springmaker/src/form_helpers.rs` — `moment_nmm`, `moments_nmm`, `fmt_moment`, `fmt_moments` + tests.
- Modify `springmaker/src/presenter.rs` — `display_moment`, `display_angle_degrees`, `display_angle_turns`, `display_ang_rate_per_deg`, `display_ang_rate_per_turn` + unit labels + tests.
- Create `springmaker/src/torsion/mod.rs`, `form.rs`, `view_model.rs`, `view.rs`.
- Modify `springmaker/src/lib.rs` (or wherever modules are declared) — add `pub mod torsion;`.
- Modify `springmaker/src/app.rs` — `Message::TorField`/`TorFriction`, `torsion`/`tor_outcome` state, `recompute`/`update`/`set_tor_field`/`save_to`/`apply_saved` arms.
- Modify `springmaker/src/calculator.rs` — `Family::Torsion` arms in `view()` and `status_panel()`.
- Modify `springmaker/src/ui_tests.rs` — a torsion Simulator E2E test.

---

### Task 1: springcore units — AngularRate US accessors

**Files:**
- Modify: `springcore/src/units.rs` (append two methods to `impl AngularRate`, ends at line 239; tests in the existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `AngularRate::pound_force_inches_per_degree(self) -> f64`, `AngularRate::pound_force_inches_per_turn(self) -> f64`. Module consts `NEWTONS_PER_LBF` and `METERS_PER_INCH` already exist (units.rs:11–12). `Moment::from_pound_force_inches`/`pound_force_inches` and `Angle::turns` already exist — reuse, do not re-add.

- [ ] **Step 1: Write the failing tests**

Add to the units test module (`springcore/src/units.rs`, inside `mod tests`):

```rust
#[test]
fn angular_rate_us_per_degree_round_trips() {
    // 1 N·m/rad = π/180 N·m/deg; in lbf·in/deg divide by (NEWTONS_PER_LBF·METERS_PER_INCH).
    let r = AngularRate::from_newton_meters_per_radian(1.0);
    let expected =
        (std::f64::consts::PI / 180.0) / (4.4482216152605 * 0.0254);
    approx::assert_relative_eq!(r.pound_force_inches_per_degree(), expected, max_relative = 1e-12);
}

#[test]
fn angular_rate_us_per_turn_is_360x_per_degree() {
    // Per-revolution is 360× the per-degree value (1 turn = 360°).
    let r = AngularRate::from_newton_meters_per_radian(2.5);
    approx::assert_relative_eq!(
        r.pound_force_inches_per_turn(),
        r.pound_force_inches_per_degree() * 360.0,
        max_relative = 1e-12
    );
}
```

If the units test module does not already `use approx;`, reference it fully as `approx::assert_relative_eq!` (shown above) to avoid touching imports.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p springcore --lib angular_rate_us`
Expected: FAIL — `no method named pound_force_inches_per_degree found`.

- [ ] **Step 3: Implement the accessors**

Insert before the closing `}` of `impl AngularRate` (units.rs:239):

```rust
    /// Return value in pound-force inches per degree (US).
    pub fn pound_force_inches_per_degree(self) -> f64 {
        self.newton_meters_per_degree() / (NEWTONS_PER_LBF * METERS_PER_INCH)
    }
    /// Return value in pound-force inches per turn / revolution (US).
    pub fn pound_force_inches_per_turn(self) -> f64 {
        self.newton_meters_per_turn() / (NEWTONS_PER_LBF * METERS_PER_INCH)
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p springcore --lib angular_rate_us`
Expected: PASS (2 passed).

- [ ] **Step 5: Mutation-check + commit**

```bash
cargo test -p springcore --lib
git diff origin/main -- > /tmp/pr.diff
cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features
# Expected: all mutants CAUGHT (0 survivors).
git add springcore/src/units.rs
git commit -m "feat(units): AngularRate US read accessors (lbf·in per degree/turn)"
```

---

### Task 2: springcore persistence — Family::Torsion + TorsionSpec

**Files:**
- Modify: `springcore/src/torsion/mechanics.rs:16` (FrictionModel derive)
- Modify: `springcore/src/family.rs` (Family variant + Display + ALL_FAMILIES + tests)
- Modify: `springcore/src/persistence.rs` (TorsionSpec + DesignSpec::Torsion + tests)

**Interfaces:**
- Produces: `springcore::Family::Torsion`; `springcore::persistence::TorsionSpec { wire_dia_mm: f64, mean_dia_mm: f64, body_coils: f64, leg1_mm: f64, leg2_mm: f64, arbor_dia_mm: Option<f64>, friction_model: FrictionModel, moments_nmm: Vec<f64> }`; `DesignSpec::Torsion(TorsionSpec)`. `FrictionModel` becomes serde-serializable.
- Consumes: Task 1's crate builds.

- [ ] **Step 1: Add serde to FrictionModel**

`springcore/src/torsion/mechanics.rs`, change the derive on line 16 from:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
```
to:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
```

(Unit variants serialize as the string `"ShigleyFriction"` / `"PureBending"`.)

- [ ] **Step 2: Write the failing family + persistence tests**

Add to `springcore/src/family.rs` `mod tests`:

```rust
#[test]
fn torsion_display_and_in_all_families() {
    assert_eq!(Family::Torsion.to_string(), "Torsion");
    assert!(ALL_FAMILIES.contains(&Family::Torsion));
}
```

Add to `springcore/src/persistence.rs` `mod tests`:

```rust
#[test]
fn torsion_round_trips_both_arbor_states_and_friction_models() {
    use crate::torsion::FrictionModel;
    for arbor in [None, Some(10.0)] {
        for friction in [FrictionModel::ShigleyFriction, FrictionModel::PureBending] {
            let saved = SavedDesign {
                material: "Music Wire".into(),
                unit_system: UnitSystem::Metric,
                design: DesignSpec::Torsion(TorsionSpec {
                    wire_dia_mm: 2.0,
                    mean_dia_mm: 20.0,
                    body_coils: 5.0,
                    leg1_mm: 50.0,
                    leg2_mm: 50.0,
                    arbor_dia_mm: arbor,
                    friction_model: friction,
                    moments_nmm: vec![100.0, 250.0],
                }),
            };
            let back = SavedDesign::from_toml(&saved.to_toml().unwrap()).unwrap();
            assert_eq!(saved, back);
        }
    }
}

#[test]
fn from_toml_rejects_non_finite_torsion_moment() {
    // reject_non_finite must reject an inf inside the moments array of a Torsion spec.
    let toml = r#"
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
moments_nmm = [100.0, inf]
"#;
    assert!(matches!(
        SavedDesign::from_toml(toml),
        Err(crate::SpringError::DataFile(_))
    ));
}
```

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test -p springcore --lib torsion_round_trips torsion_display from_toml_rejects_non_finite_torsion`
Expected: FAIL — `no variant named Torsion`, `cannot find type TorsionSpec`.

- [ ] **Step 4: Add the Family variant**

`springcore/src/family.rs`: add `Torsion,` after `Extension,` in the enum; add `Family::Torsion => "Torsion",` to the `Display` match; change `ALL_FAMILIES` to `&[Family::Compression, Family::Extension, Family::Torsion]`.

- [ ] **Step 5: Add TorsionSpec + DesignSpec::Torsion**

`springcore/src/persistence.rs`: add `Torsion(TorsionSpec)` to the `DesignSpec` enum (after `Extension(ExtScenarioSpec)`, persistence.rs:95), and define `TorsionSpec` immediately after the `DesignSpec` enum:

```rust
/// Torsion scenario inputs (SI millimetres / newton-millimetres, as stored).
/// Single-scenario (PowerUser) family: a struct, not a `#[serde(tag="type")]` enum.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TorsionSpec {
    pub wire_dia_mm: f64,
    pub mean_dia_mm: f64,
    pub body_coils: f64,
    pub leg1_mm: f64,
    pub leg2_mm: f64,
    // The only optional field: serde maps a missing key to `None` for `Option`
    // types (no `#[serde(default)]` needed), so a missing or misspelled
    // `arbor_dia_mm` deserializes to `None` rather than erroring.
    pub arbor_dia_mm: Option<f64>,
    pub friction_model: crate::torsion::FrictionModel,
    pub moments_nmm: Vec<f64>,
}
```

Add `pub use persistence::TorsionSpec;` alongside the existing `DesignSpec` re-export in `springcore/src/lib.rs` (grep `pub use persistence::` to match the existing style). `reject_non_finite` is a generic TOML tree-walk (persistence.rs:312) — it already covers the new floats; no change needed.

- [ ] **Step 6: Run to verify they pass**

Run: `cargo test -p springcore --lib` (all springcore tests; the new + existing family/persistence tests).
Expected: PASS, 0 failed. If a non-torsion match over `DesignSpec` or `Family` now fails to compile, that is expected surfacing in springmaker (Task 5), not springcore — springcore has no such match.

- [ ] **Step 7: Mutation-check + commit**

```bash
git diff origin/main -- > /tmp/pr.diff
cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features
# Expected: 0 survivors. If a Display-arm or ALL_FAMILIES mutant survives, the
# family test above already asserts both; re-run to confirm.
git add springcore/src/family.rs springcore/src/persistence.rs springcore/src/torsion/mechanics.rs springcore/src/lib.rs
git commit -m "feat(persistence): Family::Torsion + DesignSpec::Torsion(TorsionSpec)"
```

---

### Task 3: springmaker torsion form.rs + moment helpers

**Files:**
- Modify: `springmaker/src/form_helpers.rs` (add moment parse/format helpers + tests)
- Create: `springmaker/src/torsion/mod.rs`, `springmaker/src/torsion/form.rs`
- Modify: `springmaker/src/lib.rs` — add `pub mod torsion;` next to `pub mod extension;`

**Interfaces:**
- Consumes: `springcore::torsion::{PowerUser, TorsionDesign, FrictionModel, Scenario}`, `springcore::{TorsionSpec, Family}`, Task-1/2 crate.
- Produces: `torsion::form::{TorFormState, Field, TorFormOutcome, parse_and_solve, build_spec, populate_from_spec}`; helpers `form_helpers::{moment_nmm, moments_nmm, fmt_moment, fmt_moments}`.

- [ ] **Step 1: Write the failing moment-helper tests**

Add to `springmaker/src/form_helpers.rs` `mod tests`:

```rust
#[test]
fn moment_nmm_metric_passthrough_and_positive() {
    assert_eq!(moment_nmm("moment", "100", UnitSystem::Metric).unwrap(), 100.0);
    assert!(moment_nmm("moment", "0", UnitSystem::Metric).is_err()); // must be > 0
    assert!(moment_nmm("moment", "-1", UnitSystem::Metric).is_err());
}

#[test]
fn moment_nmm_us_converts_lbf_in_to_nmm() {
    // 1 lbf·in = 4.4482216152605 N × 0.0254 m = 0.112984829... N·m = 112.984829 N·mm.
    let v = moment_nmm("moment", "1", UnitSystem::Us).unwrap();
    approx::assert_relative_eq!(v, 4.4482216152605 * 0.0254 * 1000.0, max_relative = 1e-9);
}

#[test]
fn moments_nmm_parses_comma_list_and_fmt_moments_round_trips_metric() {
    let v = moments_nmm("100, 250", UnitSystem::Metric).unwrap();
    assert_eq!(v, vec![100.0, 250.0]);
    assert_eq!(fmt_moments(&v, UnitSystem::Metric), "100, 250");
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p springmaker moment_nmm moments_nmm`
Expected: FAIL — `cannot find function moment_nmm`.

- [ ] **Step 3: Implement the moment helpers**

Add to `springmaker/src/form_helpers.rs`. Extend the `use springcore::units::{...}` line to include `Moment`. Then:

```rust
/// Parse a strictly-positive moment, returning newton-millimetres (SI internal):
/// US inputs are lbf·in, metric inputs are already N·mm. Moments must be > 0
/// (a torsion load winds the coils tighter).
pub(crate) fn moment_nmm(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    let v = positive_num(field, value)?;
    let v_si = match us {
        UnitSystem::Us => Moment::from_pound_force_inches(v).newton_millimeters(),
        UnitSystem::Metric => v,
    };
    finite_or_err(field, value, v_si)
}

/// Parse a comma-separated moment list into SI newton-millimetres.
pub(crate) fn moments_nmm(value: &str, us: UnitSystem) -> Result<Vec<f64>> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| moment_nmm("moment", s, us))
        .collect()
}

/// Convert N·mm (SI internal) → display string.
pub(crate) fn fmt_moment(nmm: f64, us: UnitSystem) -> String {
    match us {
        UnitSystem::Metric => format!("{nmm}"),
        UnitSystem::Us => format!("{}", Moment::from_newton_millimeters(nmm).pound_force_inches()),
    }
}

/// Join a slice of N·mm values → comma-separated display string.
pub(crate) fn fmt_moments(moments: &[f64], us: UnitSystem) -> String {
    moments
        .iter()
        .map(|&m| fmt_moment(m, us))
        .collect::<Vec<_>>()
        .join(", ")
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p springmaker moment_nmm moments_nmm`
Expected: PASS.

- [ ] **Step 5: Create the torsion module + form**

`springmaker/src/torsion/mod.rs`:

```rust
//! Torsion-spring Calculator GUI: form (pure), presenter (pure), humble iced view.
pub mod form;
pub mod view;
pub mod view_model;
```

Add `pub mod torsion;` to `springmaker/src/lib.rs` next to `pub mod extension;`.

`springmaker/src/torsion/form.rs` (the pure form logic — no scenario enum; a single PowerUser form):

```rust
//! Pure torsion form-to-design logic. No iced dependency.
use crate::form_helpers::{fmt_len, fmt_moments, length_mm, moments_nmm, positive_num};
use springcore::torsion::{FrictionModel, PowerUser, Scenario, TorsionDesign};
use springcore::units::{Length, Moment};
use springcore::{Material, MaterialStore, Result, TorsionSpec, UnitSystem};

/// Which torsion text field a `Message::TorField` targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    WireDia,
    MeanDia,
    BodyCoils,
    Leg1,
    Leg2,
    ArborDia,
    Moments,
}

/// Torsion form inputs as raw strings, plus the friction-model selector.
#[derive(Debug, Clone)]
pub struct TorFormState {
    pub wire_dia: String,
    pub mean_dia: String,
    pub body_coils: String,
    pub leg1: String,
    pub leg2: String,
    pub arbor_dia: String,
    pub moments: String,
    pub friction_model: FrictionModel,
}

impl Default for TorFormState {
    fn default() -> Self {
        Self {
            wire_dia: String::new(),
            mean_dia: String::new(),
            body_coils: String::new(),
            leg1: String::new(),
            leg2: String::new(),
            arbor_dia: String::new(),
            moments: String::new(),
            friction_model: FrictionModel::default(),
        }
    }
}

impl TorFormState {
    /// Whether the user has entered none of the input fields. Drives the
    /// "untouched form" suppression in `App::recompute`. All seven text fields
    /// count; `arbor_dia` and `moments` count when typed (typing signals intent,
    /// the `max_outer_dia`/`loads` rule). `friction_model` is excluded — it always
    /// holds a default and cannot distinguish a blank form.
    pub fn is_blank(&self) -> bool {
        [
            &self.wire_dia,
            &self.mean_dia,
            &self.body_coils,
            &self.leg1,
            &self.leg2,
            &self.arbor_dia,
            &self.moments,
        ]
        .iter()
        .all(|f| f.trim().is_empty())
    }
}

/// A solved torsion form: the design (which carries engine-computed status).
#[derive(Debug, Clone)]
pub struct TorFormOutcome {
    pub design: TorsionDesign,
}

/// Parse the optional arbor field: empty → None; non-empty → a positive length.
fn parse_arbor(form: &TorFormState, us: UnitSystem) -> Result<Option<Length>> {
    if form.arbor_dia.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(Length::from_millimeters(length_mm(
            "arbor diameter",
            &form.arbor_dia,
            us,
        )?)))
    }
}

/// Parse the form, build the PowerUser scenario, and solve. The engine's own
/// input guards remain the defense-in-depth backstop.
pub fn parse_and_solve(
    form: &TorFormState,
    material_name: &str,
    us: UnitSystem,
    materials: &MaterialStore,
) -> Result<TorFormOutcome> {
    let material: &Material = materials.get(material_name)?;
    let scenario = PowerUser {
        wire_dia: Length::from_millimeters(length_mm("wire diameter", &form.wire_dia, us)?),
        mean_dia: Length::from_millimeters(length_mm("mean diameter", &form.mean_dia, us)?),
        body_coils: positive_num("body coils", &form.body_coils)?,
        leg1: Length::from_millimeters(length_mm("leg 1", &form.leg1, us)?),
        leg2: Length::from_millimeters(length_mm("leg 2", &form.leg2, us)?),
        arbor_dia: parse_arbor(form, us)?,
        moments: moments_nmm(&form.moments, us)?
            .into_iter()
            .map(Moment::from_newton_millimeters)
            .collect(),
    };
    Ok(TorFormOutcome {
        design: scenario.solve(material, form.friction_model)?,
    })
}

/// Parse `form` into a persisted `TorsionSpec` (SI mm / N·mm). The caller wraps it
/// in `DesignSpec::Torsion`. Round-trips with `populate_from_spec`.
pub fn build_spec(form: &TorFormState, us: UnitSystem) -> Result<TorsionSpec> {
    let arbor_dia_mm = if form.arbor_dia.trim().is_empty() {
        None
    } else {
        Some(length_mm("arbor diameter", &form.arbor_dia, us)?)
    };
    Ok(TorsionSpec {
        wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
        mean_dia_mm: length_mm("mean diameter", &form.mean_dia, us)?,
        body_coils: positive_num("body coils", &form.body_coils)?,
        leg1_mm: length_mm("leg 1", &form.leg1, us)?,
        leg2_mm: length_mm("leg 2", &form.leg2, us)?,
        arbor_dia_mm,
        friction_model: form.friction_model,
        moments_nmm: moments_nmm(&form.moments, us)?,
    })
}

/// Write a persisted `TorsionSpec` back into `form`, converting SI to display
/// units. After this call, `build_spec(form, us)` reproduces the spec.
pub fn populate_from_spec(form: &mut TorFormState, spec: &TorsionSpec, us: UnitSystem) {
    form.wire_dia = fmt_len(spec.wire_dia_mm, us);
    form.mean_dia = fmt_len(spec.mean_dia_mm, us);
    form.body_coils = format!("{}", spec.body_coils);
    form.leg1 = fmt_len(spec.leg1_mm, us);
    form.leg2 = fmt_len(spec.leg2_mm, us);
    form.arbor_dia = match spec.arbor_dia_mm {
        Some(v) => fmt_len(v, us),
        None => String::new(),
    };
    form.friction_model = spec.friction_model;
    form.moments = fmt_moments(&spec.moments_nmm, us);
}
```

- [ ] **Step 6: Write the form tests**

Add a `#[cfg(test)] mod tests` to `torsion/form.rs`. Use a metric fixture (d=2mm, D=20mm, N_b=5, legs 0, one 1000 N·mm moment):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use springcore::{MaterialSet, MaterialStore, UnitSystem};

    fn store() -> MaterialStore {
        MaterialStore::new(MaterialSet::load_default())
    }
    fn metric_form() -> TorFormState {
        TorFormState {
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            body_coils: "5".into(),
            leg1: "0".into(),
            leg2: "0".into(),
            moments: "1000".into(),
            ..TorFormState::default()
        }
    }

    #[test]
    fn is_blank_true_until_a_field_is_filled() {
        let mut f = TorFormState::default();
        assert!(f.is_blank());
        f.wire_dia = "2".into();
        assert!(!f.is_blank());
    }

    #[test]
    fn changing_only_friction_model_stays_blank() {
        let mut f = TorFormState::default();
        f.friction_model = FrictionModel::PureBending;
        assert!(f.is_blank(), "friction model default cannot distinguish blank");
    }

    #[test]
    fn typing_arbor_or_moments_clears_blank() {
        let mut f = TorFormState::default();
        f.arbor_dia = "10".into();
        assert!(!f.is_blank(), "arbor is optional but typing it signals intent");
        let mut g = TorFormState::default();
        g.moments = "500".into();
        assert!(!g.is_blank());
    }

    #[test]
    fn metric_power_user_solves_with_index_ten() {
        let out = parse_and_solve(&metric_form(), "Music Wire", UnitSystem::Metric, &store()).unwrap();
        assert_relative_eq!(out.design.index, 10.0, max_relative = 1e-9);
        assert_eq!(out.design.load_points.len(), 1);
    }

    #[test]
    fn blank_wire_dia_errors() {
        let f = TorFormState { wire_dia: String::new(), ..metric_form() };
        assert!(parse_and_solve(&f, "Music Wire", UnitSystem::Metric, &store()).is_err());
    }

    #[test]
    fn non_positive_moment_errors() {
        let f = TorFormState { moments: "0".into(), ..metric_form() };
        assert!(parse_and_solve(&f, "Music Wire", UnitSystem::Metric, &store()).is_err());
    }

    #[test]
    fn mean_at_or_below_wire_errors() {
        let f = TorFormState { mean_dia: "2".into(), ..metric_form() }; // C = 1
        assert!(parse_and_solve(&f, "Music Wire", UnitSystem::Metric, &store()).is_err());
    }

    #[test]
    fn build_spec_populate_round_trips_metric_and_us() {
        for us in [UnitSystem::Metric, UnitSystem::Us] {
            let form = TorFormState {
                wire_dia: "2".into(),
                mean_dia: "20".into(),
                body_coils: "5".into(),
                leg1: "10".into(),
                leg2: "10".into(),
                arbor_dia: "10".into(),
                moments: "100, 250".into(),
                friction_model: FrictionModel::PureBending,
            };
            let spec = build_spec(&form, us).unwrap();
            let mut form2 = TorFormState::default();
            populate_from_spec(&mut form2, &spec, us);
            assert_eq!(build_spec(&form2, us).unwrap(), spec);
            assert_eq!(form2.friction_model, FrictionModel::PureBending);
        }
    }

    #[test]
    fn empty_arbor_round_trips_as_none() {
        let spec = build_spec(&metric_form(), UnitSystem::Metric).unwrap();
        assert_eq!(spec.arbor_dia_mm, None);
    }
}
```

- [ ] **Step 7: Run the form tests + commit**

Run: `cargo test -p springmaker torsion::form`
Expected: PASS. Then:

```bash
cargo fmt --all && cargo clippy -p springmaker --all-targets -- -D warnings
git add springmaker/src/form_helpers.rs springmaker/src/torsion/mod.rs springmaker/src/torsion/form.rs springmaker/src/lib.rs
git commit -m "feat(torsion): form state, parse/solve, spec round-trip + moment helpers"
```

---

### Task 4: springmaker torsion presenter (view_model)

**Files:**
- Modify: `springmaker/src/presenter.rs` (shared quantity converters + labels + tests)
- Create: `springmaker/src/torsion/view_model.rs`

**Interfaces:**
- Consumes: `torsion::form::{TorFormOutcome, Field}`, `springcore::torsion::TorsionDesign`, presenter types (`ResultRow`, `StatusLine`, `FieldDescriptor`, `common_status_lines`, `append_status_messages`).
- Produces: `torsion::view_model::{TorResultsView, tor_results_view, tor_status_view, tor_inputs_view}` and the torsion load-row aggregate.

- [ ] **Step 1: Write the failing presenter-converter tests**

Add to `springmaker/src/presenter.rs` `mod tests`:

```rust
#[test]
fn moment_conversion_matches_unit_system() {
    use springcore::Moment;
    assert_relative_eq!(display_moment(Moment::from_newton_millimeters(100.0), UnitSystem::Metric), 100.0);
    assert_relative_eq!(
        display_moment(Moment::from_pound_force_inches(1.0), UnitSystem::Us),
        1.0,
        epsilon = 1e-9
    );
}

#[test]
fn angle_degrees_and_turns() {
    use springcore::Angle;
    assert_relative_eq!(display_angle_degrees(Angle::from_degrees(90.0)), 90.0, epsilon = 1e-9);
    assert_relative_eq!(display_angle_turns(Angle::from_turns(0.25)), 0.25, epsilon = 1e-9);
}
```

(`Moment` and `Angle` must be re-exported from `springcore` — grep `pub use units::` in `springcore/src/lib.rs`; they already are alongside `Force`/`Length`.)

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p springmaker display_moment angle_degrees`
Expected: FAIL — `cannot find function display_moment`.

- [ ] **Step 3: Add the shared converters + labels to presenter.rs**

Extend the `use springcore::{...}` in `presenter.rs` to include `Angle, AngularRate, Moment`. Then add (near `display_stress`):

```rust
/// Moment unit label for the active unit system.
pub(crate) fn unit_moment_label(us: UnitSystem) -> &'static str {
    match us {
        UnitSystem::Metric => "N·mm",
        UnitSystem::Us => "lbf·in",
    }
}

/// Moment in the active unit system: N·mm (metric) or lbf·in (US).
pub(crate) fn display_moment(m: Moment, us: UnitSystem) -> f64 {
    match us {
        UnitSystem::Metric => m.newton_millimeters(),
        UnitSystem::Us => m.pound_force_inches(),
    }
}

/// Angular deflection in degrees (unit-system independent).
pub(crate) fn display_angle_degrees(a: Angle) -> f64 {
    a.degrees()
}

/// Angular deflection in revolutions / turns (unit-system independent).
pub(crate) fn display_angle_turns(a: Angle) -> f64 {
    a.turns()
}

/// Angular rate as moment per degree: N·mm/° (metric) or lbf·in/° (US).
pub(crate) fn display_ang_rate_per_deg(r: AngularRate, us: UnitSystem) -> f64 {
    match us {
        UnitSystem::Metric => r.newton_meters_per_degree() * MM_PER_M,
        UnitSystem::Us => r.pound_force_inches_per_degree(),
    }
}

/// Angular rate as moment per revolution: N·mm/rev (metric) or lbf·in/rev (US).
pub(crate) fn display_ang_rate_per_turn(r: AngularRate, us: UnitSystem) -> f64 {
    match us {
        UnitSystem::Metric => r.newton_meters_per_turn() * MM_PER_M,
        UnitSystem::Us => r.pound_force_inches_per_turn(),
    }
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p springmaker display_moment angle_degrees`
Expected: PASS.

- [ ] **Step 5: Write the failing view_model tests**

Create `springmaker/src/torsion/view_model.rs` test module first (write the tests, then the impl). Tests assert: `tor_results_view` is `Empty` on a fresh app, `Error` when `app.error` set, `Populated` after solve; the summary rows include "Spring index", "Active coils", "Angular rate"; the per-moment rows format deflection with both `°` and `rev`; `tor_inputs_view` returns 7 field descriptors with unit-aware labels (mm/N·mm metric, in/lbf·in US) and a friction row is not part of `tor_inputs_view` (the pick-list is rendered separately in the view). Mirror the structure of `extension/view_model.rs` tests (`fresh_app`, `app_with_tor`, `tor_populated`). Use `App::from_store(store(), Vec::new(), CurvatureCorrection::Bergstrasser)`, set `app.family = Family::Torsion`, `app.torsion = form`, `app.recompute()`.

Example assertions:

```rust
#[test]
fn results_empty_then_populated() {
    let app = fresh_app();
    assert_eq!(tor_results_view(&app), TorResultsView::Empty);
    let solved = app_with_tor(metric_form());
    assert!(matches!(tor_results_view(&solved), TorResultsView::Populated(_)));
}

#[test]
fn deflection_row_shows_degrees_and_revolutions() {
    let p = tor_populated(&app_with_tor(metric_form()));
    let row0 = &p.load_table.rows[0];
    assert!(row0.deflection.contains('°') && row0.deflection.contains("rev"));
}

#[test]
fn inputs_view_has_seven_unit_aware_fields() {
    let app = fresh_app_torsion();
    let fields = tor_inputs_view(&app);
    assert_eq!(fields.len(), 7);
    assert_eq!(fields[0].field, Field::WireDia);
    assert!(fields[0].label.contains("mm"));
    assert!(fields.iter().any(|f| f.label.contains("N·mm"))); // moment field
}
```

- [ ] **Step 6: Run to verify they fail**

Run: `cargo test -p springmaker torsion::view_model`
Expected: FAIL — `tor_results_view` etc. not found.

- [ ] **Step 7: Implement view_model.rs**

Mirror `extension/view_model.rs`. Define `TorLoadRow { point, moment, deflection, stress, pct_allow, wound_inner }` and `TorLoadTable { stress_unit, rows }`; `TorResultsView { Error(String), Empty, Populated(Box<TorPopulatedResults>) }` with `TorPopulatedResults { rate_per_deg: ResultRow, rate_per_turn: ResultRow, geometry: Vec<ResultRow>, load_table: TorLoadTable }`. Build rows from `TorsionDesign`:

- Summary: `ResultRow::new("Spring index", format!("{:.3}", d.index), "")`, `ResultRow::new("Active coils", format!("{:.3}", d.active_coils), "")`, `ResultRow::new("Angular rate", format!("{:.4}", display_ang_rate_per_deg(d.rate, us)), format!("{}/°", unit_moment_label(us)))`, and a per-rev row `format!("{}/rev", unit_moment_label(us))`.
- Load rows (per `load_point`): moment `format!("{:.3} {}", display_moment(lp.moment, us), unit_moment_label(us))`; deflection `format!("{:.2}° ({:.4} rev)", display_angle_degrees(lp.deflection), display_angle_turns(lp.deflection))`; stress via `display_stress(lp.stress_inner, us)`; `pct_allow` = `format!("{:.1}%", lp.pct_bending_allow * 100.0)` (mark `ResultRow`/cell danger when `> 1.0`); wound inner diameter via `display_len(lp.wound_inner_dia, us)`.
- `tor_status_view(app)`: `common_status_lines(app)` then `append_status_messages(&mut lines, &out.design.status.messages)` when `app.tor_outcome` is set — identical shape to `ext_status_view`.
- `tor_inputs_view(app)`: seven `FieldDescriptor::new(label, Field::_)` with unit-aware labels — `Wire diameter ({len})`, `Mean diameter ({len})`, `Body coils`, `Leg 1 ({len})`, `Leg 2 ({len})`, `Arbor diameter ({len}, optional)`, `Moments ({moment}), comma-separated` where `moment = unit_moment_label(us)`.

`tor_results_view` reads `app.tor_outcome` (Populated), else `app.error` (Error), else Empty — identical control flow to `ext_results_view` (extension/view_model.rs:135–151).

- [ ] **Step 8: Run + commit**

Run: `cargo test -p springmaker torsion::view_model presenter`
Expected: PASS. Then `cargo fmt --all && cargo clippy -p springmaker --all-targets -- -D warnings`, commit:

```bash
git add springmaker/src/presenter.rs springmaker/src/torsion/view_model.rs
git commit -m "feat(torsion): presenter — angular results, status, inputs view"
```

---

### Task 5: springmaker torsion view.rs + app/calculator dispatch

**Files:**
- Create: `springmaker/src/torsion/view.rs`
- Modify: `springmaker/src/app.rs` (Message, App state, recompute, update, set_tor_field, save_to, apply_saved)
- Modify: `springmaker/src/calculator.rs` (view + status_panel arms)

**Interfaces:**
- Consumes: everything from Tasks 3–4; the humble-view helpers in `widgets.rs` (`labeled_input`, `panel_container`, `section_heading`, `styled_pick_list`, `rows_section`, `results_error`, `results_empty`, `section_divider`).
- Produces: `torsion::view::{design_panel, results_panel, tor_field_value, tor_field_id}`; `Family::Torsion` fully wired.

- [ ] **Step 1: Add the app.rs state, message, and dispatch (compile-driven)**

There is no unit test for iced wiring; the compiler + the Task-6 E2E drive it. Make these edits, each mirroring the `Extension` equivalent:

`app.rs` `Message` enum (after the extension variants, ~line 156):
```rust
    // Calculator screen — torsion
    TorField(crate::torsion::form::Field, String),
    TorFriction(springcore::torsion::FrictionModel),
```

`App` struct (after `ext_outcome`, ~line 184): `pub torsion: crate::torsion::form::TorFormState,` and `pub tor_outcome: Option<crate::torsion::form::TorFormOutcome>,`. Initialize both in `from_store` (`torsion: Default::default(), tor_outcome: None,`). Add the imports at the top of `app.rs`.

`recompute()`: in the `Family::Compression` and `Family::Extension` arms, also clear `self.tor_outcome = None;` (next to the existing cross-family clears). Add a `Family::Torsion` arm:
```rust
    Family::Torsion => {
        self.outcome = None;
        self.ext_outcome = None;
        if self.torsion.is_blank() {
            self.error = None;
            self.tor_outcome = None;
            return;
        }
        match crate::torsion::form::parse_and_solve(
            &self.torsion,
            &self.material,
            self.unit_system,
            &self.materials,
        ) {
            Ok(out) => { self.tor_outcome = Some(out); self.error = None; }
            Err(e) => { self.tor_outcome = None; self.error = Some(format_error(&e, self.unit_system)); }
        }
    }
```

`update()` (after the extension arms, ~line 368):
```rust
    Message::TorField(f, v) => { self.set_tor_field(f, v); true }
    Message::TorFriction(m) => { self.torsion.friction_model = m; true }
```

`set_tor_field` (new method beside `set_ext_field`):
```rust
fn set_tor_field(&mut self, field: crate::torsion::form::Field, value: String) {
    use crate::torsion::form::Field as TF;
    let f = &mut self.torsion;
    match field {
        TF::WireDia => f.wire_dia = value,
        TF::MeanDia => f.mean_dia = value,
        TF::BodyCoils => f.body_coils = value,
        TF::Leg1 => f.leg1 = value,
        TF::Leg2 => f.leg2 = value,
        TF::ArborDia => f.arbor_dia = value,
        TF::Moments => f.moments = value,
    }
}
```

`save_to()`: add a `Family::Torsion` arm:
```rust
    Family::Torsion => match crate::torsion::form::build_spec(&self.torsion, self.unit_system) {
        Ok(s) => springcore::DesignSpec::Torsion(s),
        Err(e) => { self.action_error = Some(e.to_string()); return; }
    },
```

`apply_saved()`: add
```rust
    springcore::DesignSpec::Torsion(spec) => {
        self.family = Family::Torsion;
        crate::torsion::form::populate_from_spec(&mut self.torsion, &spec, self.unit_system);
    }
```

- [ ] **Step 2: Add the calculator.rs dispatch arms**

`calculator.rs` `view()` (the `match app.family`, line 21): add
```rust
    Family::Torsion => (
        crate::torsion::view::design_panel(app),
        crate::torsion::view::results_panel(app),
    ),
```
`status_panel()` (line 123): add `Family::Torsion => crate::torsion::view_model::tor_status_view(app),`.

- [ ] **Step 3: Create torsion/view.rs (humble iced view)**

Mirror `springmaker/src/extension/view.rs` structure exactly, with torsion specifics. `design_panel(app)`: a `section_heading("Torsion spring")`, the friction pick-list

```rust
styled_pick_list(
    springcore::torsion::ALL_FRICTION_MODELS, // add this const in mechanics.rs if absent; else &[ShigleyFriction, PureBending]
    Some(app.torsion.friction_model),
    Message::TorFriction,
)
```

then iterate `crate::torsion::view_model::tor_inputs_view(app)` building `labeled_input(label, tor_field_value(&app.torsion, fd.field), tor_field_id(fd.field), move |s| Message::TorField(fd.field, s))` (copy the `labeled_input` call shape from extension/view.rs:44–52). `results_panel(app)`: match `tor_results_view(app)` → `results_empty()` / `results_error(msg)` / render the summary `ResultRow`s + the load-points table via `rows_section` (mirror extension/view.rs:294+). `tor_field_value(form, field) -> &str` and `tor_field_id(field) -> &'static str`: 7-arm matches (ids like `"tor-wire-dia"`, `"tor-mean-dia"`, `"tor-body-coils"`, `"tor-leg1"`, `"tor-leg2"`, `"tor-arbor-dia"`, `"tor-moments"`).

`FrictionModel` needs `Display` for the pick-list — add to `mechanics.rs` if absent:
```rust
impl std::fmt::Display for FrictionModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            FrictionModel::ShigleyFriction => "Shigley (with friction)",
            FrictionModel::PureBending => "Pure bending (EN 13906-3)",
        })
    }
}
```
and a `pub const ALL_FRICTION_MODELS: &[FrictionModel] = &[FrictionModel::ShigleyFriction, FrictionModel::PureBending];`, re-exported from `torsion/mod.rs`. (These are springcore additions — if added, they are trivial and covered by the pick-list E2E; a `friction_model_display_names` unit test in mechanics.rs keeps them mutation-clean.)

- [ ] **Step 4: Build + run the full suite**

Run: `cargo build -p springmaker && cargo test --workspace --all-features`
Expected: compiles (all `Family` matches now exhaustive), 0 failed. Fix any missed match arm the compiler flags.

- [ ] **Step 5: fmt/clippy + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add springmaker/src/torsion/view.rs springmaker/src/app.rs springmaker/src/calculator.rs springcore/src/torsion/mechanics.rs springcore/src/torsion/mod.rs
git commit -m "feat(torsion): humble view + app/calculator dispatch wiring"
```

If `mechanics.rs` changed (Display + ALL_FRICTION_MODELS), re-run the springcore in-diff mutation gate before committing that file and add the `friction_model_display_names` test to keep 0 survivors.

---

### Task 6: Simulator E2E + final review

**Files:**
- Modify: `springmaker/src/ui_tests.rs` (a torsion end-to-end test)

**Interfaces:**
- Consumes: the whole wired app. Uses the existing `ui_tests.rs` helpers (`click`, `type_into`/`typewrite`, `find`).

- [ ] **Step 1: Write the torsion E2E test**

Add to `springmaker/src/ui_tests.rs`, mirroring the extension-family Simulator tests (search `Extension family Simulator tests`, ~line 236). The harness's `type_into`/`type_into_ext` take a **typed** field enum and resolve the widget id through the family's `*_field_id` fn; add a torsion analog, then drive the flow. Existing helpers to reuse: `test_app()` (hermetic constructor, line 31), `shows(app, label)` (presence, line 55), `ui`/`click`. Note: `typewrite` APPENDS to the focused input — type each field exactly once.

```rust
/// Torsion analog of `type_into_ext`: focus a torsion field by its stable id and
/// type `text`, then apply the resulting messages.
fn type_into_tor(app: &mut App, field: crate::torsion::form::Field, text: &str) {
    let id = iced_test::core::widget::Id::from(crate::torsion::view::tor_field_id(field));
    let mut sim = ui(app);
    sim.click(id)
        .unwrap_or_else(|e| panic!("could not focus torsion input for {field:?}: {e}"));
    sim.typewrite(text);
    for message in sim.into_messages() {
        app.update(message);
    }
}

#[test]
fn torsion_family_solves_end_to_end() {
    use crate::torsion::form::Field as TF;
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Torsion));
    assert_eq!(app.family, Family::Torsion);
    assert!(shows(&app, "Enter design parameters to see results."));

    type_into_tor(&mut app, TF::WireDia, "2");
    type_into_tor(&mut app, TF::MeanDia, "20");
    type_into_tor(&mut app, TF::BodyCoils, "5");
    type_into_tor(&mut app, TF::Leg1, "0");
    type_into_tor(&mut app, TF::Leg2, "0");
    type_into_tor(&mut app, TF::Moments, "1000");

    assert!(app.tor_outcome.is_some(), "torsion design must solve");
    assert!(app.error.is_none());
}

#[test]
fn torsion_save_load_round_trip() {
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Torsion));
    app.torsion = crate::torsion::form::TorFormState {
        wire_dia: "2".into(), mean_dia: "20".into(), body_coils: "5".into(),
        leg1: "0".into(), leg2: "0".into(), moments: "1000".into(),
        ..Default::default()
    };
    app.recompute();

    let dir = std::env::temp_dir().join(format!("osm_tor_e2e_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("design.toml");
    app.save_to(&path);

    let mut app2 = test_app();
    assert!(app2.load_from(&path));
    assert_eq!(app2.family, Family::Torsion);
    assert_eq!(app2.torsion.mean_dia, "20");
    let _ = std::fs::remove_dir_all(&dir);
}
```

The exact `Field`/`Message`/`Family` imports at the top of `ui_tests.rs` already cover compression + extension; add `use` entries for the torsion `Field` only where referenced (the fully-qualified paths above avoid most import churn).

- [ ] **Step 2: Run the E2E test**

Run: `cargo test -p springmaker torsion_family_solves torsion_save_load`
Expected: PASS.

- [ ] **Step 3: Full local gate**

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
typos
cargo test --workspace --all-features
git diff origin/main -- > /tmp/pr.diff
cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features
```
Expected: all green; springcore mutation 0 survivors.

- [ ] **Step 4: Commit**

```bash
git add springmaker/src/ui_tests.rs
git commit -m "test(torsion): Simulator E2E — solve + save/load round-trip"
```

- [ ] **Step 5: Final whole-branch review**

Dispatch the mandatory adversarial panel on the full branch diff: general-code, architect, simplifier, the input-domain adversary, and a persistence/wire-format reviewer (the new `TorsionSpec` variant + `FrictionModel` serde + `reject_non_finite` coverage). Cycle to convergence; push only when every reviewer APPROVES.

---

## Notes for the implementer

- **Verify re-export paths early** (Task 3 step 5): `springcore::torsion::{PowerUser, Scenario, TorsionDesign, FrictionModel}` and `springcore::{TorsionSpec, Moment, Angle, AngularRate}`. If any differs, the compiler names the correct path; grep `pub use` in `springcore/src/lib.rs` and `springcore/src/torsion/mod.rs`.
- **The engine is the backstop, not the primary validator.** Form helpers produce field-named errors; the engine's `solve_forward` re-checks. Do not remove either layer.
- **Moment sign:** every moment must be `> 0` (winds the coils tighter) — `moment_nmm` enforces it at the boundary; the engine enforces it again.
- **Do not add a scenario picker.** Torsion is single-scenario; the form has no `TorScenarioKind`.
