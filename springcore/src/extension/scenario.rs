//! Determined (closed-form) solve scenarios for extension springs. Each scenario
//! delegates to `design::solve_forward` with the appropriate arguments.

use crate::extension::design::{solve_forward, ExtensionDesign};
use crate::extension::ends::HookEnds;
use crate::material::Material;
use crate::mechanics::active_coils_for_rate;
use crate::units::{Force, Length, SpringRate};
use crate::{CurvatureCorrection, Result, SpringError};

/// A solve scenario for extension springs: a particular fixed assignment of
/// which quantities are inputs.
pub trait Scenario {
    /// Compute a complete extension-spring design given this scenario's inputs
    /// and the specified material.
    fn solve(
        &self,
        material: &Material,
        correction: CurvatureCorrection,
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
        correction: CurvatureCorrection,
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

/// Two (force, length) operating points; solve the rate AND the initial tension.
/// Free length is given so the per-point deflections y = L − L0 are known (see
/// the plan's "Resolved design decision"). Shigley extension relations:
/// k = (F2−F1)/(y2−y1), F_i = F1 − k·y1.
#[derive(Debug, Clone)]
pub struct TwoLoad {
    pub wire_dia: Length,
    pub mean_dia: Length,
    pub free_length: Length,
    pub hooks: HookEnds,
    pub point1: (Force, Length),
    pub point2: (Force, Length),
}

impl Scenario for TwoLoad {
    fn solve(
        &self,
        material: &Material,
        correction: CurvatureCorrection,
    ) -> Result<ExtensionDesign> {
        let (f1, l1) = self.point1;
        let (f2, l2) = self.point2;
        // Deflection from the free length (an extension spring lengthens under load).
        let y1 = l1.meters() - self.free_length.meters();
        let y2 = l2.meters() - self.free_length.meters();
        let df = f2.newtons() - f1.newtons();
        let dy = y2 - y1;
        // A valid extension pair has more force at the greater length.
        if !(df.is_finite() && dy.is_finite()) || df <= 0.0 || dy <= 0.0 {
            return Err(SpringError::InconsistentInputs(
                "two load points must show increasing force with increasing length".into(),
            ));
        }
        let rate = SpringRate::from_newtons_per_meter(df / dy);
        let initial_tension = Force::from_newtons(f1.newtons() - rate.newtons_per_meter() * y1);
        let active =
            active_coils_for_rate(material.shear_modulus, self.wire_dia, self.mean_dia, rate);
        solve_forward(
            material,
            self.wire_dia,
            self.mean_dia,
            active,
            self.free_length,
            initial_tension,
            self.hooks,
            &[f1, f2],
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

    #[test]
    fn two_load_recovers_rate_and_initial_tension() {
        // Music wire, d=2mm D=20mm → k=2000 N/m. Choose F_i=10 N, L0=60mm.
        // At y=10mm (L=70mm): F = F_i + k·y = 10 + 2000·0.010 = 30 N.
        // At y=20mm (L=80mm): F = 10 + 2000·0.020 = 50 N.
        let s = TwoLoad {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            free_length: Length::from_millimeters(60.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            point1: (Force::from_newtons(30.0), Length::from_millimeters(70.0)),
            point2: (Force::from_newtons(50.0), Length::from_millimeters(80.0)),
        };
        let d = s
            .solve(
                &crate::test_support::music_wire(),
                CurvatureCorrection::Bergstrasser,
            )
            .unwrap();
        assert_relative_eq!(d.rate.newtons_per_meter(), 2000.0, max_relative = 1e-9);
        assert_relative_eq!(d.initial_tension.newtons(), 10.0, max_relative = 1e-9);
        // The first operating point round-trips to its given length.
        assert_relative_eq!(
            d.load_points[0].length.millimeters(),
            70.0,
            max_relative = 1e-9
        );
    }

    #[test]
    fn two_load_rejects_non_increasing_points() {
        // More force at a shorter length is impossible for an extension spring.
        let s = TwoLoad {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            free_length: Length::from_millimeters(60.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            point1: (Force::from_newtons(50.0), Length::from_millimeters(70.0)),
            point2: (Force::from_newtons(30.0), Length::from_millimeters(80.0)),
        };
        assert!(matches!(
            s.solve(
                &crate::test_support::music_wire(),
                CurvatureCorrection::Bergstrasser
            ),
            Err(crate::SpringError::InconsistentInputs(_))
        ));
    }

    // --- Input-domain robustness tests (mandate: every guard sub-condition exercised) ---

    #[test]
    fn two_load_rejects_non_finite_point() {
        // df = +inf passes df<=0 and dy<=0, but fails is_finite() — exercises that branch.
        // Use +infinity, not NaN: NaN<=0 is false so NaN is caught by the df<=0 arm already.
        let s = TwoLoad {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            free_length: Length::from_millimeters(60.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            point1: (Force::from_newtons(30.0), Length::from_millimeters(70.0)),
            point2: (
                Force::from_newtons(f64::INFINITY),
                Length::from_millimeters(80.0),
            ),
        };
        assert!(matches!(
            s.solve(
                &crate::test_support::music_wire(),
                CurvatureCorrection::Bergstrasser
            ),
            Err(crate::SpringError::InconsistentInputs(_))
        ));
    }

    #[test]
    fn two_load_rejects_decreasing_length() {
        // df = +20 > 0 (does NOT fire df<=0), dy = -0.010 < 0 — isolates the dy<=0 branch.
        let s = TwoLoad {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            free_length: Length::from_millimeters(60.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            point1: (Force::from_newtons(30.0), Length::from_millimeters(80.0)),
            point2: (Force::from_newtons(50.0), Length::from_millimeters(70.0)),
        };
        assert!(matches!(
            s.solve(
                &crate::test_support::music_wire(),
                CurvatureCorrection::Bergstrasser
            ),
            Err(crate::SpringError::InconsistentInputs(_))
        ));
    }

    #[test]
    fn two_load_rejects_equal_forces() {
        // df = 0 exactly — pins the <= boundary on df (zero rate is not a valid spring).
        let s = TwoLoad {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            free_length: Length::from_millimeters(60.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            point1: (Force::from_newtons(30.0), Length::from_millimeters(70.0)),
            point2: (Force::from_newtons(30.0), Length::from_millimeters(80.0)),
        };
        assert!(matches!(
            s.solve(
                &crate::test_support::music_wire(),
                CurvatureCorrection::Bergstrasser
            ),
            Err(crate::SpringError::InconsistentInputs(_))
        ));
    }
}
