//! Determined solve scenarios for torsion springs. Each scenario is a fixed assignment
//! of which quantities are inputs; it delegates to `design::solve_forward`.

use crate::material::Material;
use crate::torsion::design::{solve_forward, TorsionDesign, TorsionInputs};
use crate::torsion::mechanics::FrictionModel;
use crate::units::{Length, Moment};
use crate::Result;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::torsion::FrictionModel;
    use crate::units::{Length, Moment};
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
}
