//! Determined (closed-form) solve scenarios for extension springs. Each scenario
//! delegates to `design::solve_forward` with the appropriate arguments.

use crate::extension::design::{solve_forward, ExtensionDesign};
use crate::extension::ends::HookEnds;
use crate::material::Material;
use crate::units::{Force, Length};
use crate::Result;

/// A solve scenario for extension springs: a particular fixed assignment of
/// which quantities are inputs.
pub trait Scenario {
    /// Compute a complete extension-spring design given this scenario's inputs
    /// and the specified material.
    fn solve(
        &self,
        material: &Material,
        correction: crate::CurvatureCorrection,
    ) -> Result<ExtensionDesign>;
}

/// All geometry given; compute performance. The extension-spring counterpart
/// to the compression `PowerUser` scenario (minus end_type/fixity, plus
/// initial_tension/hooks).
#[derive(Debug, Clone)]
pub struct PowerUser {
    pub wire_dia: Length,
    pub mean_dia: Length,
    pub active: f64,
    pub free_length: Length,
    pub initial_tension: Force,
    pub hooks: HookEnds,
    pub loads: Vec<Force>,
}

impl Scenario for PowerUser {
    fn solve(
        &self,
        material: &Material,
        correction: crate::CurvatureCorrection,
    ) -> Result<ExtensionDesign> {
        solve_forward(
            material,
            self.wire_dia,
            self.mean_dia,
            self.active,
            self.free_length,
            self.initial_tension,
            self.hooks,
            &self.loads,
            correction,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension::ends::HookEnds;
    use crate::units::{Force, Length};
    use approx::assert_relative_eq;

    #[test]
    fn power_user_solves() {
        let s = PowerUser {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            active: 10.0,
            free_length: Length::from_millimeters(60.0),
            initial_tension: Force::from_newtons(10.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            loads: vec![Force::from_newtons(30.0)],
        };
        let d = s
            .solve(
                &crate::test_support::music_wire(),
                crate::CurvatureCorrection::Bergstrasser,
            )
            .unwrap();
        assert_relative_eq!(d.rate.newtons_per_meter(), 2000.0, max_relative = 1e-9);
        assert_relative_eq!(
            d.load_points[0].length.millimeters(),
            70.0,
            max_relative = 1e-9
        );
    }
}
