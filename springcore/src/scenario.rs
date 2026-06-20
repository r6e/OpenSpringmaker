//! Determined (closed-form) solve scenarios. Each scenario derives the four
//! geometry unknowns (d, D, Na, L0) from its inputs, then delegates to
//! `design::solve_forward`.

use crate::design::{solve_forward, SpringDesign};
use crate::end_type::EndType;
use crate::material::Material;
use crate::mechanics::{active_coils_for_rate, EndFixity};
use crate::units::{Force, Length, SpringRate};
use crate::{Result, SpringError};

/// A solve scenario: a particular fixed assignment of which quantities are inputs.
pub trait Scenario {
    /// Compute a complete spring design given this scenario's inputs and the specified material.
    fn solve(&self, material: &Material) -> Result<SpringDesign>;
}

/// All geometry given; compute performance.
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
    fn solve(&self, material: &Material) -> Result<SpringDesign> {
        solve_forward(
            material,
            self.end_type,
            self.fixity,
            self.wire_dia,
            self.mean_dia,
            self.active,
            self.free_length,
            &self.loads,
        )
    }
}

/// Two (force, length) operating points; solve rate and free length.
pub struct TwoLoad {
    pub end_type: EndType,
    pub fixity: EndFixity,
    pub wire_dia: Length,
    pub mean_dia: Length,
    pub point1: (Force, Length),
    pub point2: (Force, Length),
}

impl Scenario for TwoLoad {
    fn solve(&self, material: &Material) -> Result<SpringDesign> {
        let (f1, l1) = self.point1;
        let (f2, l2) = self.point2;
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
        )
    }
}

/// Target rate given; solve active coils.
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
    fn solve(&self, material: &Material) -> Result<SpringDesign> {
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
        )
    }
}

/// Outer diameter given; derive mean diameter.
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
    fn solve(&self, material: &Material) -> Result<SpringDesign> {
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
        let d = s.solve(&crate::test_support::music_wire()).unwrap();
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
        let d = s.solve(&crate::test_support::music_wire()).unwrap();
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
            s.solve(&crate::test_support::music_wire()),
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
        let d = s.solve(&crate::test_support::music_wire()).unwrap();
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
        let d = s.solve(&crate::test_support::music_wire()).unwrap();
        assert_relative_eq!(d.index, 10.0, max_relative = 1e-9);
        assert_relative_eq!(d.mean_dia.millimeters(), 20.0, max_relative = 1e-9);
    }
}
