//! Determined solve scenarios for torsion springs. Each scenario is a fixed assignment
//! of which quantities are inputs; it delegates to `design::solve_forward`.

use crate::material::Material;
use crate::torsion::design::{
    solve_forward, validate_wire_mean_geometry, TorsionDesign, TorsionInputs,
};
use crate::torsion::mechanics::{active_coils_for_rate, active_coils_with_legs, FrictionModel};
use crate::units::{Angle, AngularRate, Length, Moment};
use crate::{Result, SpringError};

/// A solve scenario for torsion springs.
pub trait Scenario {
    /// Compute a complete torsion-spring design for this scenario's inputs.
    fn solve(&self, material: &Material, friction: FrictionModel) -> Result<TorsionDesign>;
}

/// All geometry given; compute performance. The torsion counterpart to the
/// compression/extension `PowerUser` scenario.
#[derive(Debug, Clone)]
pub struct PowerUser {
    /// Wire diameter `d`.
    pub wire_dia: Length,
    /// Mean coil diameter `D`.
    pub mean_dia: Length,
    /// Body coil count `N_b`.
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

impl Scenario for PowerUser {
    fn solve(&self, material: &Material, friction: FrictionModel) -> Result<TorsionDesign> {
        solve_forward(
            material,
            TorsionInputs {
                wire_dia: self.wire_dia,
                mean_dia: self.mean_dia,
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

/// Error text for a non-finite derived active-coil count, emitted by the `na` gate
/// in [`body_coils_for_rate_input`] and by [`validate_derived_body_coils`]'s
/// finiteness backstop — single-sourced so the two sites cannot drift.
const DERIVED_COILS_NOT_FINITE: &str =
    "derived body coils must be finite (rate too small for this geometry)";

/// Body coils that produce `rate`: effective `Nₐ` from the inverted rate formula
/// ([`active_coils_for_rate`]) minus the straight-leg contribution (Shigley
/// Eq. 10-50, via [`active_coils_with_legs`] with zero body coils). Shared by the
/// RateBased and TwoLoad scenarios. Callers validate geometry first
/// (`validate_wire_mean_geometry`).
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
    if !na.is_finite() {
        return Err(SpringError::InconsistentInputs(
            DERIVED_COILS_NOT_FINITE.into(),
        ));
    }
    // Leg term via the forward helper with zero body coils — single source for
    // the (L₁+L₂)/(3πD) formula.
    let leg_term = active_coils_with_legs(0.0, leg1, leg2, mean_dia);
    if !leg_term.is_finite() {
        return Err(SpringError::InconsistentInputs(
            "leg contribution must be finite (leg lengths non-finite or too large)".into(),
        ));
    }
    validate_derived_body_coils(na - leg_term)
}

/// Guard the derived body-coil count. Split from the derivation so the exact
/// `body_coils == 0` boundary is unit-testable: pipeline float arithmetic cannot
/// reliably construct an exact 0 (Na lands a ULP off), the same reason design.rs
/// tests its exact-allowable boundary on a hand-built value.
fn validate_derived_body_coils(body_coils: f64) -> Result<f64> {
    if !body_coils.is_finite() {
        return Err(SpringError::InconsistentInputs(
            DERIVED_COILS_NOT_FINITE.into(),
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
        validate_wire_mean_geometry(self.wire_dia, self.mean_dia)?;
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

/// Outer diameter given instead of mean; mean is derived as `OD − d`. The torsion
/// counterpart to the compression/extension `Dimensional` scenario. Scenario-level
/// guard (sibling parity): `outer_dia` must be a positive finite number —
/// `InconsistentInputs("outer diameter must be a positive finite number")`. The
/// derived `mean ≤ 0` and `mean ≤ d` cases still delegate to `solve_forward`.
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
        let od = self.outer_dia.meters();
        if !(od.is_finite() && od > 0.0) {
            return Err(SpringError::InconsistentInputs(
                "outer diameter must be a positive finite number".into(),
            ));
        }
        let mean_dia = Length::from_meters(od - self.wire_dia.meters());
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
        validate_wire_mean_geometry(self.wire_dia, self.mean_dia)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::torsion::FrictionModel;
    use crate::units::{Angle, AngularRate, Length, Moment};
    use approx::assert_relative_eq;

    #[test]
    fn power_user_solves_to_same_design_as_solve_forward() {
        let m = crate::test_support::music_wire();
        let pu = PowerUser {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            body_coils: 5.0,
            leg1: Length::from_meters(0.0),
            leg2: Length::from_meters(0.0),
            arbor_dia: None,
            moments: vec![Moment::from_newton_meters(1.0)],
        };
        let d = pu.solve(&m, FrictionModel::PureBending).unwrap();
        assert_relative_eq!(
            d.rate.newton_meters_per_radian(),
            0.5085,
            max_relative = 1e-9
        );
        assert_eq!(d.load_points.len(), 1);
        assert_relative_eq!(d.index, 10.0, max_relative = 1e-12);
    }

    #[test]
    fn power_user_propagates_validation_errors() {
        let m = crate::test_support::music_wire();
        let pu = PowerUser {
            wire_dia: Length::from_meters(0.0), // invalid
            mean_dia: Length::from_millimeters(20.0),
            body_coils: 5.0,
            leg1: Length::from_meters(0.0),
            leg2: Length::from_meters(0.0),
            arbor_dia: None,
            moments: vec![Moment::from_newton_meters(1.0)],
        };
        assert!(matches!(
            pu.solve(&m, FrictionModel::PureBending),
            Err(crate::SpringError::InconsistentInputs(_))
        ));
    }

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
        assert_relative_eq!(
            d.rate.newton_meters_per_radian(),
            0.5085,
            max_relative = 1e-12
        );
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
        assert_relative_eq!(
            d.rate.newton_meters_per_radian(),
            0.5085,
            max_relative = 1e-9
        );
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
    fn derived_body_coils_guard_rejects_boundary_and_domain() {
        // Unit-level guard test: the exact Nb == 0 boundary is not constructible
        // through the solve pipeline (Na lands a ULP off 5.0), so the guard is
        // exercised directly — the same pattern design.rs uses for its exact-allowable
        // boundary. Nb == 0.0 hitting the leg-contribution message kills the
        // `<= → <` mutant; Ok(5.0) kills an always-err mutant.
        for bad in [0.0, -1.0] {
            match validate_derived_body_coils(bad) {
                Err(crate::SpringError::InconsistentInputs(msg)) => assert!(
                    msg.contains("leg contribution"),
                    "expected the leg-contribution error for Nb={bad}, got: {msg}"
                ),
                other => panic!("Nb={bad} must be rejected, got {other:?}"),
            }
        }
        for bad in [f64::INFINITY, f64::NAN] {
            match validate_derived_body_coils(bad) {
                Err(crate::SpringError::InconsistentInputs(msg)) => assert!(
                    msg.contains("finite"),
                    "expected the finite-coils error for Nb={bad}, got: {msg}"
                ),
                other => panic!("Nb={bad} must be rejected, got {other:?}"),
            }
        }
        assert_eq!(validate_derived_body_coils(5.0).unwrap(), 5.0);
    }

    #[test]
    fn rate_based_rejects_legs_that_exceed_active_coils() {
        // Legs sum = 6·3πD → leg term ≈ 6 > Na ≈ 5 → Nb ≈ −1 (robustly negative
        // under float arithmetic) → the named leg-contribution error, NOT
        // solve_forward's generic body-coils error.
        let m = crate::test_support::music_wire();
        let legs_total = 6.0 * 3.0 * std::f64::consts::PI * 0.02; // metres
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
            other => panic!("Nb < 0 must be rejected, got {other:?}"),
        }
    }

    #[test]
    fn rate_based_rejects_non_finite_leg_with_leg_message() {
        // leg1 = +Inf → leg_term overflows to +Inf → leg-attributed message,
        // not the rate-too-small message. Pins the separate leg_term finiteness
        // gate in body_coils_for_rate_input.
        let m = crate::test_support::music_wire();
        let r = RateBased {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            rate: AngularRate::from_newton_meters_per_radian(0.5085),
            leg1: Length::from_meters(f64::INFINITY),
            leg2: Length::from_meters(0.0),
            arbor_dia: None,
            moments: vec![Moment::from_newton_meters(1.0)],
        }
        .solve(&m, FrictionModel::PureBending);
        match r {
            Err(crate::SpringError::InconsistentInputs(msg)) => assert!(
                msg.contains("leg contribution must be finite"),
                "expected the leg-contribution finite error, got: {msg}"
            ),
            other => panic!("non-finite leg must be rejected, got {other:?}"),
        }
    }

    #[test]
    fn rate_based_rejects_overflowing_finite_leg_with_leg_message() {
        // leg1 = 3.4e307 m is finite, but leg_term = (3.4e307)/(3πD) ≈ 1.8e308 >
        // f64::MAX overflows to +Inf. Pins that finite-but-huge legs still surface
        // the leg-attributed error — the case is_finite() leg pre-validation alone
        // would miss.
        let m = crate::test_support::music_wire();
        let r = RateBased {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            rate: AngularRate::from_newton_meters_per_radian(0.5085),
            leg1: Length::from_meters(3.4e307),
            leg2: Length::from_meters(0.0),
            arbor_dia: None,
            moments: vec![Moment::from_newton_meters(1.0)],
        }
        .solve(&m, FrictionModel::PureBending);
        match r {
            Err(crate::SpringError::InconsistentInputs(msg)) => assert!(
                msg.contains("leg contribution must be finite"),
                "expected the leg-contribution finite error, got: {msg}"
            ),
            other => panic!("overflowing finite leg must be rejected, got {other:?}"),
        }
    }

    #[test]
    fn rate_based_na_gate_precedes_leg_gate_for_combined_overflow() {
        // Tiny rate (na → +Inf) AND huge leg (leg_term → +Inf): the na gate fires
        // first with the "rate too small for this geometry" message. Pins na-before-leg
        // precedence and kills the condition-off mutant on the na gate: under that
        // mutant, execution with these inputs falls through to the LEG gate, whose
        // "leg contribution must be finite…" message lacks the asserted "rate too
        // small" text — that mismatch is the kill. The plain tiny-rate test (no legs)
        // cannot kill it: there the fall-through reaches validate_derived_body_coils,
        // whose finiteness backstop emits the byte-identical DERIVED_COILS_NOT_FINITE
        // text, so its assertion still passes.
        let m = crate::test_support::music_wire();
        let r = RateBased {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            rate: AngularRate::from_newton_meters_per_radian(1e-320),
            leg1: Length::from_meters(3.4e307),
            leg2: Length::from_meters(0.0),
            arbor_dia: None,
            moments: vec![Moment::from_newton_meters(1.0)],
        }
        .solve(&m, FrictionModel::PureBending);
        match r {
            Err(crate::SpringError::InconsistentInputs(msg)) => assert!(
                msg.contains("rate too small for this geometry"),
                "expected the rate-too-small error first, got: {msg}"
            ),
            other => panic!("tiny rate + huge leg must be rejected, got {other:?}"),
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
        // Kills && → || and > → >= mutants in the geometry pre-validation. Message
        // assertions are load-bearing: under those mutants an infinite/zero mean falls
        // through to a DIFFERENT InconsistentInputs (leg-contribution or index guard),
        // so variant-only matching would let them survive. An infinite mean passes
        // `> 0` but fails `is_finite`; a zero mean passes `>= 0` (the mutant) but must
        // hit the positive-finite guard; mean == wire passes both but fails the index
        // guard.
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
        for (label, spec) in [("infinite", inf), ("zero", base(0.0))] {
            match spec.solve(&m, FrictionModel::PureBending) {
                Err(crate::SpringError::InconsistentInputs(msg)) => assert!(
                    msg.contains("mean diameter must be a positive finite number"),
                    "expected the mean-diameter guard for {label} mean, got: {msg}"
                ),
                other => panic!("{label} mean must be rejected, got {other:?}"),
            }
        }
        match base(2.0).solve(&m, FrictionModel::PureBending) {
            Err(crate::SpringError::InconsistentInputs(msg)) => assert!(
                msg.contains("spring index must exceed 1"),
                "expected the index guard, got: {msg}"
            ),
            other => panic!("mean == wire must be rejected, got {other:?}"),
        }
    }

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
        assert_relative_eq!(
            dim.rate.newton_meters_per_radian(),
            0.5085,
            max_relative = 1e-9
        );
        assert_relative_eq!(
            dim.inputs.mean_dia.millimeters(),
            20.0,
            max_relative = 1e-12
        );
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

    #[test]
    fn dimensional_rejects_non_finite_or_non_positive_outer_dia() {
        // The scenario-level OD guard (sibling parity) must fire before the mean
        // derivation. Message assertions are load-bearing: for od=0.0 and od=+Inf,
        // the `&&→||` and `>→>=` mutants fall through to solve_forward's mean guard,
        // which returns a DIFFERENT InconsistentInputs message — asserting the exact
        // text distinguishes the two paths and kills both mutants.
        let m = crate::test_support::music_wire();
        let with_od = |od: f64| Dimensional {
            wire_dia: Length::from_millimeters(2.0),
            outer_dia: Length::from_meters(od),
            body_coils: 5.0,
            leg1: Length::from_meters(0.0),
            leg2: Length::from_meters(0.0),
            arbor_dia: None,
            moments: vec![Moment::from_newton_meters(1.0)],
        };
        for od in [0.0, -1.0, f64::INFINITY, f64::NAN] {
            match with_od(od).solve(&m, FrictionModel::PureBending) {
                Err(crate::SpringError::InconsistentInputs(msg)) => assert!(
                    msg.contains("outer diameter must be a positive finite number"),
                    "expected the OD guard message for od={od}, got: {msg}"
                ),
                other => panic!("od={od} must be rejected, got {other:?}"),
            }
        }
    }

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
        assert_relative_eq!(
            d.rate.newton_meters_per_radian(),
            0.5085,
            max_relative = 1e-9
        );
        assert_relative_eq!(d.inputs.body_coils, 5.0, max_relative = 1e-9);
        assert_eq!(d.load_points.len(), 2);
        assert_relative_eq!(
            d.load_points[0].moment.newton_meters(),
            0.5085,
            max_relative = 1e-12
        );
        assert_relative_eq!(
            d.load_points[1].moment.newton_meters(),
            1.0170,
            max_relative = 1e-12
        );
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
            point1: (
                Moment::from_newton_meters(0.5085),
                Angle::from_radians(1.0 + offset),
            ),
            point2: (
                Moment::from_newton_meters(1.0170),
                Angle::from_radians(2.0 + offset),
            ),
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
        for (m2, th2) in [
            (f64::NAN, 2.0),
            (1.0, f64::NAN),
            (f64::INFINITY, 2.0),
            (1.0, f64::INFINITY),
        ] {
            assert!(
                matches!(
                    build(m2, th2).solve(&m, FrictionModel::PureBending),
                    Err(crate::SpringError::InconsistentInputs(_))
                ),
                "non-finite point ({m2}, {th2}) must be rejected"
            );
        }
    }

    #[test]
    fn two_load_slope_computed_as_quotient_not_product() {
        // d_theta = 2.0 rad (not 1.0) so `/` and `*` diverge: slope = 1.017/2.0 = 0.5085
        // with division, but 1.017*2.0 = 2.034 with multiplication. The body-coil
        // assertion kills the `replace / with *` mutant.
        let m = crate::test_support::music_wire();
        let d = TwoLoad {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            leg1: Length::from_meters(0.0),
            leg2: Length::from_meters(0.0),
            arbor_dia: None,
            point1: (Moment::from_newton_meters(0.5085), Angle::from_radians(0.0)),
            point2: (Moment::from_newton_meters(1.5255), Angle::from_radians(2.0)),
        }
        .solve(&m, FrictionModel::PureBending)
        .unwrap();
        assert_relative_eq!(
            d.rate.newton_meters_per_radian(),
            0.5085,
            max_relative = 1e-9
        );
        assert_relative_eq!(d.inputs.body_coils, 5.0, max_relative = 1e-9);
    }

    #[test]
    fn two_load_rejects_zero_slope_from_underflow() {
        // d_moment > 0 and d_theta > 0 but d_moment/d_theta underflows to 0.0 in f64
        // (true quotient ≈ 1.2e-616 « smallest subnormal). The slope guard must still
        // reject it. The message assertion is load-bearing: under the `> → >=` mutant,
        // slope=0 passes the guard and reaches body_coils_for_rate_input, which errors
        // "rate must be a positive finite number" — a message that does NOT contain
        // "positive finite rate", so the assertion fails and the mutant is killed.
        let m = crate::test_support::music_wire();
        let r = TwoLoad {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            leg1: Length::from_meters(0.0),
            leg2: Length::from_meters(0.0),
            arbor_dia: None,
            point1: (Moment::from_newton_meters(0.0), Angle::from_radians(0.0)),
            point2: (
                Moment::from_newton_meters(f64::MIN_POSITIVE),
                Angle::from_radians(f64::MAX),
            ),
        }
        .solve(&m, FrictionModel::PureBending);
        match r {
            Err(crate::SpringError::InconsistentInputs(msg)) => assert!(
                msg.contains("positive finite rate"),
                "expected the positive-rate error for underflowing slope, got: {msg}"
            ),
            other => panic!("underflowing slope must be rejected, got {other:?}"),
        }
    }
}
