//! Determined (closed-form) solve scenarios. Each scenario derives the four
//! geometry unknowns (d, D, Na, L0) from its inputs, then delegates to
//! `design::solve_forward`.

use crate::design::{solve_forward, SpringDesign};
use crate::end_type::EndType;
use crate::material::Material;
use crate::mechanics::{active_coils_for_rate, EndFixity};
use crate::units::{Force, Length, SpringRate};
use crate::{CurvatureCorrection, Result, SpringError};

/// A solve scenario: a particular fixed assignment of which quantities are inputs.
pub trait Scenario {
    /// Compute a complete spring design given this scenario's inputs and the specified material.
    fn solve(&self, material: &Material, correction: CurvatureCorrection) -> Result<SpringDesign>;
}

/// All geometry given; compute performance.
#[derive(Debug, Clone)]
pub struct PowerUser {
    pub end_type: EndType,
    pub fixity: EndFixity,
    pub wire_dia: Length,
    pub mean_dia: Length,
    pub active: f64,
    pub free_length: Length,
    pub loads: Vec<Force>,
}

impl Scenario for PowerUser {
    fn solve(&self, material: &Material, correction: CurvatureCorrection) -> Result<SpringDesign> {
        solve_forward(
            material,
            self.end_type,
            self.fixity,
            self.wire_dia,
            self.mean_dia,
            self.active,
            self.free_length,
            &self.loads,
            correction,
        )
    }
}

/// Two (force, length) operating points; solve rate and free length.
#[derive(Debug, Clone)]
pub struct TwoLoad {
    pub end_type: EndType,
    pub fixity: EndFixity,
    pub wire_dia: Length,
    pub mean_dia: Length,
    pub point1: (Force, Length),
    pub point2: (Force, Length),
}

impl Scenario for TwoLoad {
    fn solve(&self, material: &Material, correction: CurvatureCorrection) -> Result<SpringDesign> {
        let (f1, l1) = self.point1;
        let (f2, l2) = self.point2;
        // Reject non-finite points up front; otherwise NaN slips the `<= 0.0`
        // comparisons below (NaN compares false) and surfaces later as a confusing
        // error about a derived quantity the user never entered.
        if [f1.newtons(), f2.newtons(), l1.meters(), l2.meters()]
            .iter()
            .any(|v| !v.is_finite())
        {
            return Err(SpringError::InconsistentInputs(
                "load points must be finite".into(),
            ));
        }
        let df = f2.newtons() - f1.newtons();
        let dl = l1.meters() - l2.meters();
        // A valid compression pair has more force at the shorter length.
        if dl <= 0.0 || df <= 0.0 {
            return Err(SpringError::InconsistentInputs(
                "two load points must show increasing force with decreasing length".into(),
            ));
        }
        let rate = SpringRate::from_newtons_per_meter(df / dl);
        // Free length: F1 = k (L0 - L1)  ->  L0 = L1 + F1/k.
        let free_length =
            Length::from_meters(l1.meters() + f1.newtons() / rate.newtons_per_meter());
        let active =
            active_coils_for_rate(material.shear_modulus, self.wire_dia, self.mean_dia, rate);
        solve_forward(
            material,
            self.end_type,
            self.fixity,
            self.wire_dia,
            self.mean_dia,
            active,
            free_length,
            &[f1, f2],
            correction,
        )
    }
}

/// Target rate given; solve active coils.
#[derive(Debug, Clone)]
pub struct RateBased {
    pub end_type: EndType,
    pub fixity: EndFixity,
    pub wire_dia: Length,
    pub mean_dia: Length,
    pub rate: SpringRate,
    pub free_length: Length,
    pub loads: Vec<Force>,
}

impl Scenario for RateBased {
    fn solve(&self, material: &Material, correction: CurvatureCorrection) -> Result<SpringDesign> {
        // Validate the target rate here; otherwise a non-positive/non-finite rate
        // makes the derived active-coil count bad and surfaces downstream as a
        // misleading "active coils must be positive" error.
        if !(self.rate.newtons_per_meter().is_finite() && self.rate.newtons_per_meter() > 0.0) {
            return Err(SpringError::InconsistentInputs(
                "required rate must be a positive finite number".into(),
            ));
        }
        let active = active_coils_for_rate(
            material.shear_modulus,
            self.wire_dia,
            self.mean_dia,
            self.rate,
        );
        solve_forward(
            material,
            self.end_type,
            self.fixity,
            self.wire_dia,
            self.mean_dia,
            active,
            self.free_length,
            &self.loads,
            correction,
        )
    }
}

/// Outer diameter given; derive mean diameter.
#[derive(Debug, Clone)]
pub struct Dimensional {
    pub end_type: EndType,
    pub fixity: EndFixity,
    pub wire_dia: Length,
    pub outer_dia: Length,
    pub active: f64,
    pub free_length: Length,
    pub loads: Vec<Force>,
}

impl Scenario for Dimensional {
    fn solve(&self, material: &Material, correction: CurvatureCorrection) -> Result<SpringDesign> {
        // Validate the outer diameter here so a non-finite/non-positive value gives
        // a clear message rather than a derived "mean diameter" error.
        if !(self.outer_dia.meters().is_finite() && self.outer_dia.meters() > 0.0) {
            return Err(SpringError::InconsistentInputs(
                "outer diameter must be a positive finite number".into(),
            ));
        }
        let mean = Length::from_meters(self.outer_dia.meters() - self.wire_dia.meters());
        solve_forward(
            material,
            self.end_type,
            self.fixity,
            self.wire_dia,
            mean,
            self.active,
            self.free_length,
            &self.loads,
            correction,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mechanics::EndFixity;
    use crate::units::{Force, Length, SpringRate};
    use approx::assert_relative_eq;

    #[test]
    fn power_user_passes_through() {
        let s = PowerUser {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            active: 10.0,
            free_length: Length::from_millimeters(60.0),
            loads: vec![Force::from_newtons(10.0)],
        };
        let d = s
            .solve(
                &crate::test_support::music_wire(),
                CurvatureCorrection::Bergstrasser,
            )
            .unwrap();
        assert_relative_eq!(d.rate.newtons_per_meter(), 2000.0, max_relative = 1e-9);
    }

    #[test]
    fn two_load_recovers_rate_and_free_length() {
        // From the clean case: k=2000 N/m, L0=60mm. Points: (10N,55mm),(20N,50mm).
        let s = TwoLoad {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            point1: (Force::from_newtons(10.0), Length::from_millimeters(55.0)),
            point2: (Force::from_newtons(20.0), Length::from_millimeters(50.0)),
        };
        let d = s
            .solve(
                &crate::test_support::music_wire(),
                CurvatureCorrection::Bergstrasser,
            )
            .unwrap();
        assert_relative_eq!(d.rate.newtons_per_meter(), 2000.0, max_relative = 1e-9);
        assert_relative_eq!(d.free_length.millimeters(), 60.0, max_relative = 1e-9);
        assert_relative_eq!(d.active_coils, 10.0, max_relative = 1e-6);
    }

    #[test]
    fn two_load_rejects_inconsistent_points() {
        // Higher force at longer length is impossible for a compression spring.
        let s = TwoLoad {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            point1: (Force::from_newtons(20.0), Length::from_millimeters(55.0)),
            point2: (Force::from_newtons(10.0), Length::from_millimeters(50.0)),
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
    fn rate_based_hits_target_rate() {
        let s = RateBased {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            rate: SpringRate::from_newtons_per_meter(2000.0),
            free_length: Length::from_millimeters(60.0),
            loads: vec![Force::from_newtons(10.0)],
        };
        let d = s
            .solve(
                &crate::test_support::music_wire(),
                CurvatureCorrection::Bergstrasser,
            )
            .unwrap();
        assert_relative_eq!(d.rate.newtons_per_meter(), 2000.0, max_relative = 1e-6);
        assert_relative_eq!(d.active_coils, 10.0, max_relative = 1e-6);
    }

    #[test]
    fn dimensional_uses_outer_diameter() {
        // OD = 22mm, d = 2mm -> mean = 20mm -> C = 10.
        let s = Dimensional {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            outer_dia: Length::from_millimeters(22.0),
            active: 10.0,
            free_length: Length::from_millimeters(60.0),
            loads: vec![Force::from_newtons(10.0)],
        };
        let d = s
            .solve(
                &crate::test_support::music_wire(),
                CurvatureCorrection::Bergstrasser,
            )
            .unwrap();
        assert_relative_eq!(d.index, 10.0, max_relative = 1e-9);
        assert_relative_eq!(d.mean_dia.millimeters(), 20.0, max_relative = 1e-9);
    }

    /// Dimensional with outer_dia == wire_dia → mean = 0 → InconsistentInputs.
    #[test]
    fn dimensional_rejects_outer_equal_to_wire() {
        let s = Dimensional {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(5.0),
            outer_dia: Length::from_millimeters(5.0), // mean = 0 → rejected
            active: 10.0,
            free_length: Length::from_millimeters(60.0),
            loads: vec![Force::from_newtons(10.0)],
        };
        assert!(
            matches!(
                s.solve(
                    &crate::test_support::music_wire(),
                    CurvatureCorrection::Bergstrasser
                ),
                Err(crate::SpringError::InconsistentInputs(_))
            ),
            "outer_dia == wire_dia must return InconsistentInputs"
        );
    }

    /// Dimensional with outer_dia < wire_dia → mean < 0 → InconsistentInputs.
    #[test]
    fn dimensional_rejects_outer_less_than_wire() {
        let s = Dimensional {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(5.0),
            outer_dia: Length::from_millimeters(3.0), // mean = −2 mm → rejected
            active: 10.0,
            free_length: Length::from_millimeters(60.0),
            loads: vec![Force::from_newtons(10.0)],
        };
        assert!(
            matches!(
                s.solve(
                    &crate::test_support::music_wire(),
                    CurvatureCorrection::Bergstrasser
                ),
                Err(crate::SpringError::InconsistentInputs(_))
            ),
            "outer_dia < wire_dia must return InconsistentInputs"
        );
    }

    /// A non-positive target rate is rejected at the scenario layer with a
    /// rate-specific message (not the derived "active coils" error).
    #[test]
    fn rate_based_rejects_non_positive_rate() {
        let s = RateBased {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            rate: SpringRate::from_newtons_per_meter(0.0),
            free_length: Length::from_millimeters(60.0),
            loads: vec![Force::from_newtons(10.0)],
        };
        assert!(
            matches!(s.solve(&crate::test_support::music_wire(), CurvatureCorrection::Bergstrasser),
                Err(crate::SpringError::InconsistentInputs(m)) if m == "required rate must be a positive finite number"),
            "non-positive rate must be rejected with a rate-specific message"
        );
    }

    /// A non-finite load point is rejected before it slips the `<= 0.0` checks.
    #[test]
    fn two_load_rejects_non_finite_point() {
        let s = TwoLoad {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            point1: (
                Force::from_newtons(10.0),
                Length::from_millimeters(f64::NAN),
            ),
            point2: (Force::from_newtons(20.0), Length::from_millimeters(50.0)),
        };
        assert!(
            matches!(s.solve(&crate::test_support::music_wire(), CurvatureCorrection::Bergstrasser),
                Err(crate::SpringError::InconsistentInputs(m)) if m == "load points must be finite"),
            "a NaN point must be rejected with a points-finite message"
        );
    }

    /// A non-finite outer diameter is rejected with an OD-specific message.
    #[test]
    fn dimensional_rejects_non_finite_outer() {
        let s = Dimensional {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            outer_dia: Length::from_millimeters(f64::INFINITY),
            active: 10.0,
            free_length: Length::from_millimeters(60.0),
            loads: vec![Force::from_newtons(10.0)],
        };
        assert!(
            matches!(s.solve(&crate::test_support::music_wire(), CurvatureCorrection::Bergstrasser),
                Err(crate::SpringError::InconsistentInputs(m)) if m == "outer diameter must be a positive finite number"),
            "non-finite outer_dia must be rejected with an OD-specific message"
        );
    }

    /// outer_dia == 0 must be rejected by the OD guard (with the OD message),
    /// not slip through to a derived "mean diameter" error. Pins the `> 0.0`
    /// (not `>= 0.0`) boundary.
    #[test]
    fn dimensional_rejects_zero_outer() {
        let s = Dimensional {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            outer_dia: Length::from_millimeters(0.0),
            active: 10.0,
            free_length: Length::from_millimeters(60.0),
            loads: vec![Force::from_newtons(10.0)],
        };
        assert!(
            matches!(s.solve(&crate::test_support::music_wire(), CurvatureCorrection::Bergstrasser),
                Err(crate::SpringError::InconsistentInputs(m)) if m == "outer diameter must be a positive finite number"),
            "outer_dia == 0 must be rejected with the OD message"
        );
    }
}
