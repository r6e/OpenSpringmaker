# Torsion Phase 2 — Engine Input Modes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add RateBased, Dimensional, and TwoLoad input scenarios plus the force-on-a-leg helper (`M = F·r`) to the torsion engine, each deriving one quantity and delegating to the proven `solve_forward`.

**Architecture:** Two files. `springcore/src/torsion/mechanics.rs` gains two cited pure helpers (`active_coils_for_rate` — the `angular_rate` formula inverted — and `moment_from_force_at_radius`). `springcore/src/torsion/scenario.rs` gains three scenario structs implementing the existing `Scenario` trait, with two private shared helpers (geometry pre-validation for error precedence; rate→body-coils derivation). `mod.rs` re-exports. No GUI, no persistence change.

**Tech Stack:** Rust (MSRV 1.88), approx (test asserts), cargo-mutants (in-diff gate).

## Global Constraints

- springcore is mutation-gated to **literal 0 survivors**: `git diff origin/main -- > /tmp/pr.diff && cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features` after every task.
- Strict TDD: write the failing test, watch it fail, implement, watch it pass.
- Engine-only: NO changes to `springmaker/`, `persistence.rs`, or `TorsionSpec` (the struct→enum migration is the GUI phase's).
- All formulas cited inline (Shigley Ch. 10; EN 13906-3). No commercial/vendor names anywhere.
- **Error precedence:** RateBased/TwoLoad validate wire/mean geometry (positive, finite, `mean > wire`, with `solve_forward`'s exact messages) BEFORE the rate/slope derivation.
- Local gate before the final commit: `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features`, `typos`, `cargo test --workspace --all-features`, and the in-diff mutation gate.
- Oracle geometry (phase-1 golden): d=2 mm, D=20 mm, Music Wire (E=203.4 GPa), Nₐ=5 → k′=0.5085 N·m/rad (PureBending, exact) / 0.47958689518357805 (ShigleyFriction); legs 50+50 mm ↔ leg term 0.530516476972984.

---

## File Structure

- Modify `springcore/src/torsion/mechanics.rs` — add `Force` to the units import; two pub helpers + tests.
- Modify `springcore/src/torsion/scenario.rs` — extend imports; two private helpers; three scenario structs + `Scenario` impls + tests.
- Modify `springcore/src/torsion/mod.rs` — extend the two existing `pub use` lines.

---

### Task 1: Mechanics helpers — `active_coils_for_rate` + `moment_from_force_at_radius`

**Files:**
- Modify: `springcore/src/torsion/mechanics.rs` (imports line 4; helpers after `angular_rate` ~line 90; tests in `mod tests`)
- Modify: `springcore/src/torsion/mod.rs` (mechanics `pub use` line)

**Interfaces:**
- Consumes: existing `angular_rate`, `FrictionModel`, `SHIGLEY_TURN_DENOM`, units.
- Produces: `pub fn active_coils_for_rate(youngs_modulus: Stress, wire_dia: Length, mean_dia: Length, rate: AngularRate, friction: FrictionModel) -> f64` and `pub fn moment_from_force_at_radius(force: Force, radius: Length) -> Moment` — both pure (no guards, like the sibling mechanics fns; scenario-layer guards come in Task 2). Re-exported as `springcore::torsion::{active_coils_for_rate, moment_from_force_at_radius}`.

- [ ] **Step 1: Write the failing tests**

In `springcore/src/torsion/mechanics.rs` `mod tests` (module already imports `super::*`, units, `assert_relative_eq`):

```rust
#[test]
fn active_coils_for_rate_inverts_pure_bending_oracle() {
    // Na = E·d⁴/(64·D·k'); E=203.4 GPa, d=2 mm, D=20 mm, k'=0.5085 → 5.0 (exact).
    let na = active_coils_for_rate(
        Stress::from_pascals(203.4e9),
        Length::from_millimeters(2.0),
        Length::from_millimeters(20.0),
        AngularRate::from_newton_meters_per_radian(0.5085),
        FrictionModel::PureBending,
    );
    assert_relative_eq!(na, 5.0, max_relative = 1e-12);
}

#[test]
fn active_coils_for_rate_round_trips_angular_rate_both_models() {
    // active_coils_for_rate(angular_rate(Na)) == Na for both friction models —
    // pins the two functions as exact inverses (and the denom per model).
    for friction in [FrictionModel::PureBending, FrictionModel::ShigleyFriction] {
        let k = angular_rate(
            Stress::from_pascals(203.4e9),
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            5.0,
            friction,
        );
        let na = active_coils_for_rate(
            Stress::from_pascals(203.4e9),
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            k,
            friction,
        );
        assert_relative_eq!(na, 5.0, max_relative = 1e-12);
    }
}

#[test]
fn moment_from_force_at_radius_exact() {
    // M = F·r; 10 N at 50 mm = 0.5 N·m (exact).
    let m = moment_from_force_at_radius(Force::from_newtons(10.0), Length::from_millimeters(50.0));
    assert_relative_eq!(m.newton_meters(), 0.5, max_relative = 1e-12);
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p springcore --lib torsion::mechanics`
Expected: FAIL — `cannot find function active_coils_for_rate` / `moment_from_force_at_radius`.

- [ ] **Step 3: Implement**

Extend the units import (mechanics.rs line 4) to include `Force`:

```rust
use crate::units::{Angle, AngularRate, Force, Length, Moment, Stress};
```

Insert after `angular_rate` (before `wound_mean_diameter`):

```rust
/// Effective active coils that produce angular rate `k'` — the [`angular_rate`]
/// formula inverted: `Nₐ = E·d⁴ / (denom · D · k')`, with `denom` = 64
/// (PureBending, EN 13906-3 energy method) or 2π·10.8 (ShigleyFriction,
/// Shigley Eq. 10-51). Pure formula (no guards), like its forward counterpart;
/// scenarios validate the inputs. Exact inverse of [`angular_rate`].
pub fn active_coils_for_rate(
    youngs_modulus: Stress,
    wire_dia: Length,
    mean_dia: Length,
    rate: AngularRate,
    friction: FrictionModel,
) -> f64 {
    let e = youngs_modulus.pascals();
    let d = wire_dia.meters();
    let dm = mean_dia.meters();
    let denom_factor = match friction {
        FrictionModel::PureBending => 64.0,
        FrictionModel::ShigleyFriction => TAU * SHIGLEY_TURN_DENOM,
    };
    e * d.powi(4) / (denom_factor * dm * rate.newton_meters_per_radian())
}

/// Moment produced by a force applied at a radius: `M = F·r` (elementary statics;
/// the torsion-spring loading model of Shigley Ch. 10 — a load on a leg at a
/// moment arm). The GUI exposes this as a force-at-radius moment-entry convenience.
pub fn moment_from_force_at_radius(force: Force, radius: Length) -> Moment {
    Moment::from_newton_meters(force.newtons() * radius.meters())
}
```

Extend the mod.rs mechanics re-export line to:

```rust
pub use mechanics::{
    active_coils_for_rate, moment_from_force_at_radius, FrictionModel, ALL_FRICTION_MODELS,
};
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p springcore --lib torsion::mechanics`
Expected: PASS (all mechanics tests, including the 3 new).

- [ ] **Step 5: Mutation-check + commit**

```bash
cargo test -p springcore --lib
cargo fmt --all && cargo clippy -p springcore --all-targets -- -D warnings
git diff origin/main -- > /tmp/pr.diff
cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features
# Expected: 0 survivors. The denom-swap mutant (64 ↔ TAU·10.8) is killed by the
# round-trip test running BOTH models; formula mutants by the exact oracle.
git add springcore/src/torsion/mechanics.rs springcore/src/torsion/mod.rs
git commit -m "feat(torsion): active_coils_for_rate inversion + moment_from_force_at_radius"
```

---

### Task 2: RateBased scenario + shared derivation helpers

**Files:**
- Modify: `springcore/src/torsion/scenario.rs` (imports; helpers + struct after `PowerUser`; tests)
- Modify: `springcore/src/torsion/mod.rs` (scenario `pub use` line)

**Interfaces:**
- Consumes: Task 1's `active_coils_for_rate`; existing `active_coils_with_legs`, `solve_forward`, `TorsionInputs`, `Scenario` trait, `SpringError`.
- Produces: `pub struct RateBased { wire_dia: Length, mean_dia: Length, rate: AngularRate, leg1: Length, leg2: Length, arbor_dia: Option<Length>, moments: Vec<Moment> }` implementing `Scenario`; private `fn validate_rate_geometry(wire_dia: Length, mean_dia: Length) -> Result<()>` and `fn body_coils_for_rate_input(material: &Material, wire_dia: Length, mean_dia: Length, rate: AngularRate, leg1: Length, leg2: Length, friction: FrictionModel) -> Result<f64>` — Task 4's TwoLoad reuses both.

- [ ] **Step 1: Write the failing tests**

In `springcore/src/torsion/scenario.rs` `mod tests`, replace the existing
`use crate::units::{Length, Moment};` line with:

```rust
    use crate::units::{AngularRate, Length, Moment};
```

(`Angle` joins this import in Task 4, where its tests first use it — adding it now
would be an unused import and fail clippy `-D warnings`.)

```rust
#[test]
fn rate_based_derives_body_coils_and_round_trips_rate() {
    // k'=0.5085 N·m/rad PureBending on the oracle geometry → Nb = 5.0 (no legs),
    // and the solved design reproduces the requested rate (exact inverses).
    let m = crate::test_support::music_wire();
    let d = RateBased {
        wire_dia: Length::from_millimeters(2.0),
        mean_dia: Length::from_millimeters(20.0),
        rate: AngularRate::from_newton_meters_per_radian(0.5085),
        leg1: Length::from_meters(0.0),
        leg2: Length::from_meters(0.0),
        arbor_dia: None,
        moments: vec![Moment::from_newton_meters(1.0)],
    }
    .solve(&m, FrictionModel::PureBending)
    .unwrap();
    assert_relative_eq!(d.inputs.body_coils, 5.0, max_relative = 1e-12);
    assert_relative_eq!(d.rate.newton_meters_per_radian(), 0.5085, max_relative = 1e-12);
}

#[test]
fn rate_based_subtracts_leg_contribution() {
    // Legs 50+50 mm at D=20 mm contribute 0.530516476972984 coils (phase-1 oracle);
    // the derived body coils must be Na − that term.
    let m = crate::test_support::music_wire();
    let d = RateBased {
        wire_dia: Length::from_millimeters(2.0),
        mean_dia: Length::from_millimeters(20.0),
        rate: AngularRate::from_newton_meters_per_radian(0.5085),
        leg1: Length::from_millimeters(50.0),
        leg2: Length::from_millimeters(50.0),
        arbor_dia: None,
        moments: vec![Moment::from_newton_meters(1.0)],
    }
    .solve(&m, FrictionModel::PureBending)
    .unwrap();
    assert_relative_eq!(
        d.inputs.body_coils,
        5.0 - 0.530516476972984,
        max_relative = 1e-9
    );
    // Rate still round-trips: solve_forward recomputes Na = Nb + leg term = 5.0.
    assert_relative_eq!(d.rate.newton_meters_per_radian(), 0.5085, max_relative = 1e-9);
}

#[test]
fn rate_based_rejects_non_positive_and_non_finite_rate() {
    let m = crate::test_support::music_wire();
    for bad in [0.0, -1.0, f64::INFINITY, f64::NAN] {
        let r = RateBased {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            rate: AngularRate::from_newton_meters_per_radian(bad),
            leg1: Length::from_meters(0.0),
            leg2: Length::from_meters(0.0),
            arbor_dia: None,
            moments: vec![Moment::from_newton_meters(1.0)],
        }
        .solve(&m, FrictionModel::PureBending);
        match r {
            Err(crate::SpringError::InconsistentInputs(msg)) => assert!(
                msg.contains("rate must be a positive finite number"),
                "unexpected message for rate={bad}: {msg}"
            ),
            other => panic!("rate={bad} must be rejected, got {other:?}"),
        }
    }
}

#[test]
fn rate_based_rejects_legs_that_consume_all_active_coils() {
    // Exact boundary: legs sum = Na·3πD (leg term == Na == 5) → Nb == 0 → the
    // named leg-contribution error, NOT solve_forward's generic body-coils error
    // (asserting the message kills a > → >= mutant that would otherwise defer
    // to the engine's different message).
    let m = crate::test_support::music_wire();
    let legs_total = 5.0 * 3.0 * std::f64::consts::PI * 0.02; // metres
    let r = RateBased {
        wire_dia: Length::from_millimeters(2.0),
        mean_dia: Length::from_millimeters(20.0),
        rate: AngularRate::from_newton_meters_per_radian(0.5085),
        leg1: Length::from_meters(legs_total / 2.0),
        leg2: Length::from_meters(legs_total / 2.0),
        arbor_dia: None,
        moments: vec![Moment::from_newton_meters(1.0)],
    }
    .solve(&m, FrictionModel::PureBending);
    match r {
        Err(crate::SpringError::InconsistentInputs(msg)) => assert!(
            msg.contains("leg contribution"),
            "expected the named leg-contribution error, got: {msg}"
        ),
        other => panic!("Nb == 0 must be rejected, got {other:?}"),
    }
}

#[test]
fn rate_based_rejects_tiny_rate_with_finite_coils_error() {
    // A denormal-scale rate drives Na to +Inf; the derivation must reject it with
    // the finite-coils message, not pass Inf body coils to the engine.
    let m = crate::test_support::music_wire();
    let r = RateBased {
        wire_dia: Length::from_millimeters(2.0),
        mean_dia: Length::from_millimeters(20.0),
        rate: AngularRate::from_newton_meters_per_radian(1e-320),
        leg1: Length::from_meters(0.0),
        leg2: Length::from_meters(0.0),
        arbor_dia: None,
        moments: vec![Moment::from_newton_meters(1.0)],
    }
    .solve(&m, FrictionModel::PureBending);
    match r {
        Err(crate::SpringError::InconsistentInputs(msg)) => assert!(
            msg.contains("finite"),
            "expected the finite-coils error, got: {msg}"
        ),
        other => panic!("tiny rate must be rejected, got {other:?}"),
    }
}

#[test]
fn rate_based_geometry_error_precedes_derivation_error() {
    // Error precedence (spec requirement): wire_dia = 0 must surface the geometry
    // message, not a misleading derived-coils error.
    let m = crate::test_support::music_wire();
    let r = RateBased {
        wire_dia: Length::from_meters(0.0),
        mean_dia: Length::from_millimeters(20.0),
        rate: AngularRate::from_newton_meters_per_radian(0.5085),
        leg1: Length::from_meters(0.0),
        leg2: Length::from_meters(0.0),
        arbor_dia: None,
        moments: vec![Moment::from_newton_meters(1.0)],
    }
    .solve(&m, FrictionModel::PureBending);
    match r {
        Err(crate::SpringError::InconsistentInputs(msg)) => assert!(
            msg.contains("wire diameter must be a positive finite number"),
            "expected the geometry error first, got: {msg}"
        ),
        other => panic!("expected InconsistentInputs, got {other:?}"),
    }
}

#[test]
fn rate_based_geometry_guards_reject_infinite_and_index_one() {
    // Kills && → || mutants in the geometry pre-validation: an infinite mean passes
    // `> 0` but fails `is_finite`; mean == wire passes both but fails the index guard.
    let m = crate::test_support::music_wire();
    let base = |mean_mm: f64| RateBased {
        wire_dia: Length::from_millimeters(2.0),
        mean_dia: Length::from_millimeters(mean_mm),
        rate: AngularRate::from_newton_meters_per_radian(0.5085),
        leg1: Length::from_meters(0.0),
        leg2: Length::from_meters(0.0),
        arbor_dia: None,
        moments: vec![Moment::from_newton_meters(1.0)],
    };
    let inf = RateBased {
        mean_dia: Length::from_meters(f64::INFINITY),
        ..base(20.0)
    };
    assert!(matches!(
        inf.solve(&m, FrictionModel::PureBending),
        Err(crate::SpringError::InconsistentInputs(_))
    ));
    match base(2.0).solve(&m, FrictionModel::PureBending) {
        Err(crate::SpringError::InconsistentInputs(msg)) => assert!(
            msg.contains("spring index must exceed 1"),
            "expected the index guard, got: {msg}"
        ),
        other => panic!("mean == wire must be rejected, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p springcore --lib torsion::scenario`
Expected: FAIL — `cannot find struct RateBased`.

- [ ] **Step 3: Implement**

Extend scenario.rs imports:

```rust
use crate::torsion::mechanics::{active_coils_for_rate, active_coils_with_legs, FrictionModel};
use crate::units::{AngularRate, Length, Moment};
use crate::{Result, SpringError};
```

(keep the existing `Material`, `design::…` imports; `Angle` joins the units import in Task 4.)

Append after the `PowerUser` impl:

```rust
/// Validate wire/mean geometry with `solve_forward`'s messages, for scenarios whose
/// derivation consumes the geometry BEFORE delegation. Error precedence (spec
/// requirement): a degenerate wire/mean must surface the geometry error, not a
/// misleading derived-coils error.
fn validate_rate_geometry(wire_dia: Length, mean_dia: Length) -> Result<()> {
    let d = wire_dia.meters();
    if !(d.is_finite() && d > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "wire diameter must be a positive finite number".into(),
        ));
    }
    let dm = mean_dia.meters();
    if !(dm.is_finite() && dm > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "mean diameter must be a positive finite number".into(),
        ));
    }
    if dm <= d {
        return Err(SpringError::InconsistentInputs(
            "mean diameter must exceed wire diameter (spring index must exceed 1)".into(),
        ));
    }
    Ok(())
}

/// Body coils that produce `rate`: effective `Nₐ` from the inverted rate formula
/// ([`active_coils_for_rate`]) minus the straight-leg contribution (Shigley
/// Eq. 10-50, via [`active_coils_with_legs`] with zero body coils). Shared by the
/// RateBased and TwoLoad scenarios. Callers validate geometry first
/// (`validate_rate_geometry`).
fn body_coils_for_rate_input(
    material: &Material,
    wire_dia: Length,
    mean_dia: Length,
    rate: AngularRate,
    leg1: Length,
    leg2: Length,
    friction: FrictionModel,
) -> Result<f64> {
    let k = rate.newton_meters_per_radian();
    if !(k.is_finite() && k > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "rate must be a positive finite number".into(),
        ));
    }
    let na = active_coils_for_rate(material.youngs_modulus, wire_dia, mean_dia, rate, friction);
    // Leg term via the forward helper with zero body coils — single source for
    // the (L₁+L₂)/(3πD) formula.
    let leg_term = active_coils_with_legs(0.0, leg1, leg2, mean_dia);
    let body_coils = na - leg_term;
    if !body_coils.is_finite() {
        return Err(SpringError::InconsistentInputs(
            "derived body coils must be finite (rate too small for this geometry)".into(),
        ));
    }
    if body_coils <= 0.0 {
        return Err(SpringError::InconsistentInputs(
            "leg contribution alone meets or exceeds the active coils the required rate allows (body coils would be \u{2264} 0)".into(),
        ));
    }
    Ok(body_coils)
}

/// Geometry + required angular rate given; body coils derived (the rate formula
/// inverted, minus the leg contribution). The torsion counterpart to the
/// compression/extension `RateBased` scenario.
#[derive(Debug, Clone)]
pub struct RateBased {
    /// Wire diameter `d`.
    pub wire_dia: Length,
    /// Mean coil diameter `D`.
    pub mean_dia: Length,
    /// Required angular rate `k'`; body coils are derived from it.
    pub rate: AngularRate,
    /// First straight-leg length `L₁`.
    pub leg1: Length,
    /// Second straight-leg length `L₂`.
    pub leg2: Length,
    /// Optional arbor diameter for the wind-up clearance check.
    pub arbor_dia: Option<Length>,
    /// Applied moments (one load point each).
    pub moments: Vec<Moment>,
}

impl Scenario for RateBased {
    fn solve(&self, material: &Material, friction: FrictionModel) -> Result<TorsionDesign> {
        validate_rate_geometry(self.wire_dia, self.mean_dia)?;
        let body_coils = body_coils_for_rate_input(
            material,
            self.wire_dia,
            self.mean_dia,
            self.rate,
            self.leg1,
            self.leg2,
            friction,
        )?;
        solve_forward(
            material,
            TorsionInputs {
                wire_dia: self.wire_dia,
                mean_dia: self.mean_dia,
                body_coils,
                leg1: self.leg1,
                leg2: self.leg2,
                arbor_dia: self.arbor_dia,
            },
            &self.moments,
            friction,
        )
    }
}
```

Extend the mod.rs scenario re-export line to:

```rust
pub use scenario::{PowerUser, RateBased, Scenario};
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p springcore --lib torsion::scenario`
Expected: PASS.

- [ ] **Step 5: Mutation-check + commit**

```bash
cargo test -p springcore --lib
cargo fmt --all && cargo clippy -p springcore --all-targets -- -D warnings
git diff origin/main -- > /tmp/pr.diff
cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features
# Expected: 0 survivors. If a guard mutant survives, the matching rejection test
# above targets the wrong guard — strengthen its message assertion rather than loosening.
git add springcore/src/torsion/scenario.rs springcore/src/torsion/mod.rs
git commit -m "feat(torsion): RateBased scenario — body coils derived from required rate"
```

---

### Task 3: Dimensional scenario

**Files:**
- Modify: `springcore/src/torsion/scenario.rs` (struct + impl after RateBased; tests)
- Modify: `springcore/src/torsion/mod.rs` (scenario `pub use` line)

**Interfaces:**
- Consumes: existing `solve_forward`, `TorsionInputs`, `Scenario`.
- Produces: `pub struct Dimensional { wire_dia: Length, outer_dia: Length, body_coils: f64, leg1: Length, leg2: Length, arbor_dia: Option<Length>, moments: Vec<Moment> }` implementing `Scenario`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn dimensional_matches_power_user_with_derived_mean() {
    // OD = 22 mm, d = 2 mm → mean = 20 mm: identical design to the PowerUser oracle.
    let m = crate::test_support::music_wire();
    let dim = Dimensional {
        wire_dia: Length::from_millimeters(2.0),
        outer_dia: Length::from_millimeters(22.0),
        body_coils: 5.0,
        leg1: Length::from_meters(0.0),
        leg2: Length::from_meters(0.0),
        arbor_dia: None,
        moments: vec![Moment::from_newton_meters(1.0)],
    }
    .solve(&m, FrictionModel::PureBending)
    .unwrap();
    assert_relative_eq!(dim.rate.newton_meters_per_radian(), 0.5085, max_relative = 1e-9);
    assert_relative_eq!(dim.inputs.mean_dia.millimeters(), 20.0, max_relative = 1e-12);
    assert_relative_eq!(dim.index, 10.0, max_relative = 1e-12);
}

#[test]
fn dimensional_rejects_outer_at_or_below_two_wire_diameters() {
    // OD == 2d → mean == d → index == 1 → engine's index guard (delegation).
    // OD < 2d → mean < d → same guard. OD ≤ d → mean ≤ 0 → the positivity guard.
    let m = crate::test_support::music_wire();
    let with_od = |od_mm: f64| Dimensional {
        wire_dia: Length::from_millimeters(2.0),
        outer_dia: Length::from_millimeters(od_mm),
        body_coils: 5.0,
        leg1: Length::from_meters(0.0),
        leg2: Length::from_meters(0.0),
        arbor_dia: None,
        moments: vec![Moment::from_newton_meters(1.0)],
    };
    for od in [4.0, 3.0, 1.5] {
        assert!(
            matches!(
                with_od(od).solve(&m, FrictionModel::PureBending),
                Err(crate::SpringError::InconsistentInputs(_))
            ),
            "OD = {od} mm with d = 2 mm must be rejected"
        );
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p springcore --lib torsion::scenario::tests::dimensional`
Expected: FAIL — `cannot find struct Dimensional`.

- [ ] **Step 3: Implement**

Append after the RateBased impl:

```rust
/// Outer diameter given instead of mean; mean is derived as `OD − d`. The torsion
/// counterpart to the compression/extension `Dimensional` scenario. No
/// scenario-level guard beyond delegation: `solve_forward` rejects the derived
/// `mean ≤ 0` (positivity guard) and `mean ≤ d` (spring-index guard), covering
/// every `OD ≤ 2d` input.
#[derive(Debug, Clone)]
pub struct Dimensional {
    /// Wire diameter `d`.
    pub wire_dia: Length,
    /// Coil outer diameter; mean is derived as `OD − d`.
    pub outer_dia: Length,
    /// Body (active) coil count `N_b`.
    pub body_coils: f64,
    /// First straight-leg length `L₁`.
    pub leg1: Length,
    /// Second straight-leg length `L₂`.
    pub leg2: Length,
    /// Optional arbor diameter for the wind-up clearance check.
    pub arbor_dia: Option<Length>,
    /// Applied moments (one load point each).
    pub moments: Vec<Moment>,
}

impl Scenario for Dimensional {
    fn solve(&self, material: &Material, friction: FrictionModel) -> Result<TorsionDesign> {
        let mean_dia = Length::from_meters(self.outer_dia.meters() - self.wire_dia.meters());
        solve_forward(
            material,
            TorsionInputs {
                wire_dia: self.wire_dia,
                mean_dia,
                body_coils: self.body_coils,
                leg1: self.leg1,
                leg2: self.leg2,
                arbor_dia: self.arbor_dia,
            },
            &self.moments,
            friction,
        )
    }
}
```

Extend the mod.rs scenario re-export line to:

```rust
pub use scenario::{Dimensional, PowerUser, RateBased, Scenario};
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p springcore --lib torsion::scenario`
Expected: PASS.

- [ ] **Step 5: Mutation-check + commit**

```bash
cargo test -p springcore --lib
cargo fmt --all && cargo clippy -p springcore --all-targets -- -D warnings
git diff origin/main -- > /tmp/pr.diff
cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features
# Expected: 0 survivors — the subtraction mutant (− → +) is killed by
# dimensional_matches_power_user_with_derived_mean (mean 24 ≠ 20 changes rate/index).
git add springcore/src/torsion/scenario.rs springcore/src/torsion/mod.rs
git commit -m "feat(torsion): Dimensional scenario — mean derived from outer diameter"
```

---

### Task 4: TwoLoad scenario + final gate

**Files:**
- Modify: `springcore/src/torsion/scenario.rs` (struct + impl after Dimensional; tests; add `Angle` to test/unit imports)
- Modify: `springcore/src/torsion/mod.rs` (scenario `pub use` line)

**Interfaces:**
- Consumes: Task 2's `validate_rate_geometry` + `body_coils_for_rate_input` (exact signatures in Task 2's Produces block); `AngularRate`, `Angle`.
- Produces: `pub struct TwoLoad { wire_dia: Length, mean_dia: Length, leg1: Length, leg2: Length, arbor_dia: Option<Length>, point1: (Moment, Angle), point2: (Moment, Angle) }` implementing `Scenario`.

- [ ] **Step 1: Write the failing tests**

Add `Angle` to the scenario.rs unit imports (both the module's `use crate::units::{…}` and the test module's) if not already present. Tests:

```rust
#[test]
fn two_load_derives_rate_from_slope_and_body_coils() {
    // Two points on the k' = 0.5085 N·m/rad line: (0.5085, 1 rad), (1.0170, 2 rad)
    // → slope 0.5085 → Nb = 5.0; load points are the two moments in input order.
    let m = crate::test_support::music_wire();
    let d = TwoLoad {
        wire_dia: Length::from_millimeters(2.0),
        mean_dia: Length::from_millimeters(20.0),
        leg1: Length::from_meters(0.0),
        leg2: Length::from_meters(0.0),
        arbor_dia: None,
        point1: (Moment::from_newton_meters(0.5085), Angle::from_radians(1.0)),
        point2: (Moment::from_newton_meters(1.0170), Angle::from_radians(2.0)),
    }
    .solve(&m, FrictionModel::PureBending)
    .unwrap();
    assert_relative_eq!(d.rate.newton_meters_per_radian(), 0.5085, max_relative = 1e-9);
    assert_relative_eq!(d.inputs.body_coils, 5.0, max_relative = 1e-9);
    assert_eq!(d.load_points.len(), 2);
    assert_relative_eq!(d.load_points[0].moment.newton_meters(), 0.5085, max_relative = 1e-12);
    assert_relative_eq!(d.load_points[1].moment.newton_meters(), 1.0170, max_relative = 1e-12);
}

#[test]
fn two_load_is_offset_tolerant() {
    // Shifting BOTH measured angles by a constant zero-reference offset must
    // derive the identical design — only the slope matters (documented contract).
    let m = crate::test_support::music_wire();
    let build = |offset: f64| TwoLoad {
        wire_dia: Length::from_millimeters(2.0),
        mean_dia: Length::from_millimeters(20.0),
        leg1: Length::from_meters(0.0),
        leg2: Length::from_meters(0.0),
        arbor_dia: None,
        point1: (Moment::from_newton_meters(0.5085), Angle::from_radians(1.0 + offset)),
        point2: (Moment::from_newton_meters(1.0170), Angle::from_radians(2.0 + offset)),
    };
    let a = build(0.0).solve(&m, FrictionModel::PureBending).unwrap();
    let b = build(0.3).solve(&m, FrictionModel::PureBending).unwrap();
    assert_relative_eq!(
        a.inputs.body_coils,
        b.inputs.body_coils,
        max_relative = 1e-12
    );
    assert_relative_eq!(
        a.rate.newton_meters_per_radian(),
        b.rate.newton_meters_per_radian(),
        max_relative = 1e-12
    );
}

#[test]
fn two_load_rejects_degenerate_points() {
    let m = crate::test_support::music_wire();
    let build = |m1: f64, th1: f64, m2: f64, th2: f64| TwoLoad {
        wire_dia: Length::from_millimeters(2.0),
        mean_dia: Length::from_millimeters(20.0),
        leg1: Length::from_meters(0.0),
        leg2: Length::from_meters(0.0),
        arbor_dia: None,
        point1: (Moment::from_newton_meters(m1), Angle::from_radians(th1)),
        point2: (Moment::from_newton_meters(m2), Angle::from_radians(th2)),
    };
    // Same angle → "different angles"; same moment → "different moments";
    // larger moment at SMALLER angle → negative slope → the positive-rate error.
    let cases: [(f64, f64, f64, f64, &str); 3] = [
        (0.5, 1.0, 1.0, 1.0, "different angles"),
        (0.5, 1.0, 0.5, 2.0, "different moments"),
        (1.0, 1.0, 0.5, 2.0, "positive finite rate"),
    ];
    for (m1, th1, m2, th2, expect) in cases {
        match build(m1, th1, m2, th2).solve(&m, FrictionModel::PureBending) {
            Err(crate::SpringError::InconsistentInputs(msg)) => assert!(
                msg.contains(expect),
                "case ({m1},{th1})/({m2},{th2}): expected '{expect}', got: {msg}"
            ),
            other => panic!("case ({m1},{th1})/({m2},{th2}) must be rejected, got {other:?}"),
        }
    }
}

#[test]
fn two_load_geometry_error_precedes_slope_error() {
    // Error precedence: degenerate geometry surfaces the geometry message even
    // when the points are also degenerate.
    let m = crate::test_support::music_wire();
    let r = TwoLoad {
        wire_dia: Length::from_meters(0.0),
        mean_dia: Length::from_millimeters(20.0),
        leg1: Length::from_meters(0.0),
        leg2: Length::from_meters(0.0),
        arbor_dia: None,
        point1: (Moment::from_newton_meters(0.5), Angle::from_radians(1.0)),
        point2: (Moment::from_newton_meters(0.5), Angle::from_radians(1.0)),
    }
    .solve(&m, FrictionModel::PureBending);
    match r {
        Err(crate::SpringError::InconsistentInputs(msg)) => assert!(
            msg.contains("wire diameter must be a positive finite number"),
            "expected the geometry error first, got: {msg}"
        ),
        other => panic!("expected InconsistentInputs, got {other:?}"),
    }
}

#[test]
fn two_load_rejects_non_finite_point_values() {
    // NaN/Inf in either coordinate must be rejected (the slope guard catches what
    // the distinct-point guards let through, since NaN ≠ anything).
    let m = crate::test_support::music_wire();
    let build = |m2: f64, th2: f64| TwoLoad {
        wire_dia: Length::from_millimeters(2.0),
        mean_dia: Length::from_millimeters(20.0),
        leg1: Length::from_meters(0.0),
        leg2: Length::from_meters(0.0),
        arbor_dia: None,
        point1: (Moment::from_newton_meters(0.5), Angle::from_radians(1.0)),
        point2: (Moment::from_newton_meters(m2), Angle::from_radians(th2)),
    };
    for (m2, th2) in [(f64::NAN, 2.0), (1.0, f64::NAN), (f64::INFINITY, 2.0), (1.0, f64::INFINITY)] {
        assert!(
            matches!(
                build(m2, th2).solve(&m, FrictionModel::PureBending),
                Err(crate::SpringError::InconsistentInputs(_))
            ),
            "non-finite point ({m2}, {th2}) must be rejected"
        );
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p springcore --lib torsion::scenario::tests::two_load`
Expected: FAIL — `cannot find struct TwoLoad`.

- [ ] **Step 3: Implement**

Append after the Dimensional impl (and add `Angle` to the module's units import):

```rust
/// Two measured (moment, angle) operating points; rate, then body coils, derived.
/// The torsion counterpart to the compression/extension `TwoLoad` scenario:
/// `k' = (M₂ − M₁)/(θ₂ − θ₁)`, then the body coils that produce `k'`.
///
/// **Offset-tolerant by design:** the static model is linear through the free
/// position (`M = k'·θ_from_free`), so a constant zero-reference offset in the
/// *measured* angles cancels in the slope. Measured angles need not be referenced
/// to the free position — only their difference matters; result deflections are
/// the true from-free values `M/k'` computed by `solve_forward`.
#[derive(Debug, Clone)]
pub struct TwoLoad {
    /// Wire diameter `d`.
    pub wire_dia: Length,
    /// Mean coil diameter `D`.
    pub mean_dia: Length,
    /// First straight-leg length `L₁`.
    pub leg1: Length,
    /// Second straight-leg length `L₂`.
    pub leg2: Length,
    /// Optional arbor diameter for the wind-up clearance check.
    pub arbor_dia: Option<Length>,
    /// First measured (moment, angle) operating point.
    pub point1: (Moment, Angle),
    /// Second measured (moment, angle) operating point.
    pub point2: (Moment, Angle),
}

impl Scenario for TwoLoad {
    fn solve(&self, material: &Material, friction: FrictionModel) -> Result<TorsionDesign> {
        validate_rate_geometry(self.wire_dia, self.mean_dia)?;
        let (m1, th1) = self.point1;
        let (m2, th2) = self.point2;
        let d_theta = th2.radians() - th1.radians();
        if d_theta == 0.0 {
            return Err(SpringError::InconsistentInputs(
                "the two operating points must have different angles".into(),
            ));
        }
        let d_moment = m2.newton_meters() - m1.newton_meters();
        if d_moment == 0.0 {
            return Err(SpringError::InconsistentInputs(
                "the two operating points must have different moments".into(),
            ));
        }
        let slope = d_moment / d_theta;
        if !(slope.is_finite() && slope > 0.0) {
            return Err(SpringError::InconsistentInputs(
                "the two operating points must define a positive finite rate (larger moment at larger angle)".into(),
            ));
        }
        let rate = AngularRate::from_newton_meters_per_radian(slope);
        let body_coils = body_coils_for_rate_input(
            material,
            self.wire_dia,
            self.mean_dia,
            rate,
            self.leg1,
            self.leg2,
            friction,
        )?;
        // The two measured moments become the design's load points, in input order.
        solve_forward(
            material,
            TorsionInputs {
                wire_dia: self.wire_dia,
                mean_dia: self.mean_dia,
                body_coils,
                leg1: self.leg1,
                leg2: self.leg2,
                arbor_dia: self.arbor_dia,
            },
            &[m1, m2],
            friction,
        )
    }
}
```

Extend the mod.rs scenario re-export line to:

```rust
pub use scenario::{Dimensional, PowerUser, RateBased, Scenario, TwoLoad};
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p springcore --lib torsion::scenario`
Expected: PASS.

- [ ] **Step 5: Full final gate + commit**

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
typos
cargo test --workspace --all-features
git diff origin/main -- > /tmp/pr.diff
cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features
# Expected: everything green; mutation literal 0 survivors across the whole branch diff.
git add springcore/src/torsion/scenario.rs springcore/src/torsion/mod.rs
git commit -m "feat(torsion): TwoLoad scenario — rate from two measured operating points"
```

- [ ] **Step 6: Final whole-branch review**

Dispatch the mandatory adversarial panel on the full branch diff (general-code, architect, simplifier, MANDATORY input-domain adversary — no persistence reviewer needed; this branch touches no persistence). Cycle to convergence; push only when every reviewer APPROVES.

---

## Notes for the implementer

- **Delegation is the second validation layer, not the first.** Do not remove the
  derivation-layer guards because "the engine checks too" — the spec requires the
  named, precedence-ordered errors listed per scenario.
- **`d.inputs.body_coils`** on a solved `TorsionDesign` is the derived value —
  that's how the oracle tests read it back.
- **`active_coils_with_legs(0.0, leg1, leg2, mean_dia)`** is the sanctioned way to
  compute the leg term (single source of the `(L₁+L₂)/(3πD)` formula) — do not
  re-derive it inline.
- No `#[allow(dead_code)]`; no `todo!()`; no commercial/vendor names.
