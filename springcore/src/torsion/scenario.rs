//! Determined solve scenarios for torsion springs. Each scenario is a fixed assignment
//! of which quantities are inputs; it delegates to `design::solve_forward`.

use crate::material::Material;
use crate::torsion::design::{solve_forward, TorsionDesign, TorsionInputs};
use crate::torsion::mechanics::{active_coils_for_rate, active_coils_with_legs, FrictionModel};
use crate::units::{AngularRate, Length, Moment};
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
    validate_derived_body_coils(na - leg_term)
}

/// Guard the derived body-coil count. Split from the derivation so the exact
/// `body_coils == 0` boundary is unit-testable: pipeline float arithmetic cannot
/// reliably construct an exact 0 (Na lands a ULP off), the same reason design.rs
/// tests its exact-allowable boundary on a hand-built value.
fn validate_derived_body_coils(body_coils: f64) -> Result<f64> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::torsion::FrictionModel;
    use crate::units::{AngularRate, Length, Moment};
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
}
