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
        // An extension spring only lengthens under load, so every operating length must be
        // at or above the free length (y ≥ 0). A negative y would clamp to 0 in
        // `deflection` (y = max(0,…)) and silently misreport the point's length.
        if y1 < 0.0 {
            return Err(SpringError::InconsistentInputs(
                "operating lengths must be at or above the free length".into(),
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

/// Required rate given; back out the active coils, then solve. Mirrors the
/// compression `RateBased` (plus initial tension / hooks).
#[derive(Debug, Clone)]
pub struct RateBased {
    pub wire_dia: Length,
    pub mean_dia: Length,
    pub rate: SpringRate,
    pub free_length: Length,
    pub initial_tension: Force,
    pub hooks: HookEnds,
    pub loads: Vec<Force>,
}

impl Scenario for RateBased {
    fn solve(
        &self,
        material: &Material,
        correction: CurvatureCorrection,
    ) -> Result<ExtensionDesign> {
        // Validate the target rate here so a non-positive/non-finite value gives a
        // rate-specific message rather than the derived "active coils" error.
        let k = self.rate.newtons_per_meter();
        if !(k.is_finite() && k > 0.0) {
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
            self.wire_dia,
            self.mean_dia,
            active,
            self.free_length,
            self.initial_tension,
            self.hooks,
            &self.loads,
            correction,
        )
    }
}

/// Outer diameter given; derive the mean diameter (D = OD − d), then solve.
/// Mirrors the compression `Dimensional` (plus initial tension / hooks).
#[derive(Debug, Clone)]
pub struct Dimensional {
    pub wire_dia: Length,
    pub outer_dia: Length,
    pub active: f64,
    pub free_length: Length,
    pub initial_tension: Force,
    pub hooks: HookEnds,
    pub loads: Vec<Force>,
}

impl Scenario for Dimensional {
    fn solve(
        &self,
        material: &Material,
        correction: CurvatureCorrection,
    ) -> Result<ExtensionDesign> {
        // Validate the outer diameter here so a non-finite/non-positive value gives
        // a clear message rather than a derived "mean diameter" error.
        let od = self.outer_dia.meters();
        if !(od.is_finite() && od > 0.0) {
            return Err(SpringError::InconsistentInputs(
                "outer diameter must be a positive finite number".into(),
            ));
        }
        let mean = Length::from_meters(od - self.wire_dia.meters());
        solve_forward(
            material,
            self.wire_dia,
            mean,
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
    use crate::units::{Force, Length, SpringRate};
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
        // Pin the exact message so the test fails when a mutation lets bad values
        // through to solve_forward (which catches them with a *different* message).
        let s = TwoLoad {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            free_length: Length::from_millimeters(60.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            point1: (Force::from_newtons(50.0), Length::from_millimeters(70.0)),
            point2: (Force::from_newtons(30.0), Length::from_millimeters(80.0)),
        };
        assert!(
            matches!(
                s.solve(
                    &crate::test_support::music_wire(),
                    CurvatureCorrection::Bergstrasser
                ),
                Err(crate::SpringError::InconsistentInputs(ref m))
                    if m == "two load points must show increasing force with increasing length"
            ),
            "expected TwoLoad guard message"
        );
    }

    // --- Input-domain robustness tests (mandate: every guard sub-condition exercised) ---

    #[test]
    fn two_load_rejects_non_finite_point() {
        // df = +inf passes df<=0 and dy<=0, but fails is_finite() — exercises that branch.
        // Use +infinity, not NaN: both fail is_finite(), but a NaN df also passes
        // df<=0 (NaN comparisons are always false), so +inf is the value that is
        // positive AND non-finite — only the is_finite() arm can reject it.
        // Pin the exact message to distinguish from solve_forward's own guards.
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
        assert!(
            matches!(
                s.solve(
                    &crate::test_support::music_wire(),
                    CurvatureCorrection::Bergstrasser
                ),
                Err(crate::SpringError::InconsistentInputs(ref m))
                    if m == "two load points must show increasing force with increasing length"
            ),
            "expected TwoLoad guard message"
        );
    }

    #[test]
    fn two_load_rejects_decreasing_length() {
        // df = +20 > 0 (does NOT fire df<=0), dy = -0.010 < 0 — isolates the dy<=0 branch.
        // Pin the exact message to distinguish from solve_forward's own guards.
        let s = TwoLoad {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            free_length: Length::from_millimeters(60.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            point1: (Force::from_newtons(30.0), Length::from_millimeters(80.0)),
            point2: (Force::from_newtons(50.0), Length::from_millimeters(70.0)),
        };
        assert!(
            matches!(
                s.solve(
                    &crate::test_support::music_wire(),
                    CurvatureCorrection::Bergstrasser
                ),
                Err(crate::SpringError::InconsistentInputs(ref m))
                    if m == "two load points must show increasing force with increasing length"
            ),
            "expected TwoLoad guard message"
        );
    }

    #[test]
    fn two_load_rejects_equal_forces() {
        // df = 0 exactly — pins the <= boundary on df (zero rate is not a valid spring).
        // Pin the exact message to distinguish from solve_forward's own guards.
        let s = TwoLoad {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            free_length: Length::from_millimeters(60.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            point1: (Force::from_newtons(30.0), Length::from_millimeters(70.0)),
            point2: (Force::from_newtons(30.0), Length::from_millimeters(80.0)),
        };
        assert!(
            matches!(
                s.solve(
                    &crate::test_support::music_wire(),
                    CurvatureCorrection::Bergstrasser
                ),
                Err(crate::SpringError::InconsistentInputs(ref m))
                    if m == "two load points must show increasing force with increasing length"
            ),
            "expected TwoLoad guard message"
        );
    }

    #[test]
    fn two_load_rejects_operating_length_below_free_length() {
        // free_length=75mm, point1=(30N,70mm), point2=(50N,80mm) → y1=−5mm.
        // Without the y1<0 guard: k=2000 N/m, F_i=40N (positive), so
        // solve_forward's initial-tension guard does NOT catch this; it returns Ok
        // with load_points[0].length silently misreported as 75mm, not 70mm.
        // The new guard must fire first.
        let s = TwoLoad {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            free_length: Length::from_millimeters(75.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            point1: (Force::from_newtons(30.0), Length::from_millimeters(70.0)),
            point2: (Force::from_newtons(50.0), Length::from_millimeters(80.0)),
        };
        assert!(
            matches!(
                s.solve(
                    &crate::test_support::music_wire(),
                    CurvatureCorrection::Bergstrasser
                ),
                Err(crate::SpringError::InconsistentInputs(ref m))
                    if m == "operating lengths must be at or above the free length"
            ),
            "expected TwoLoad y1<0 guard message"
        );
    }

    #[test]
    fn two_load_accepts_first_point_at_free_length() {
        // y1=0 boundary (strict < 0.0 guard must NOT fire here).
        // free_length=70mm, point1=(10N,70mm), point2=(30N,80mm):
        // y1=0, y2=10mm, k=(30−10)/0.010=2000 N/m, F_i=10−2000·0=10 N.
        // Both points round-trip to their given lengths.
        let s = TwoLoad {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            free_length: Length::from_millimeters(70.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            point1: (Force::from_newtons(10.0), Length::from_millimeters(70.0)),
            point2: (Force::from_newtons(30.0), Length::from_millimeters(80.0)),
        };
        let d = s
            .solve(
                &crate::test_support::music_wire(),
                CurvatureCorrection::Bergstrasser,
            )
            .unwrap();
        assert_relative_eq!(d.rate.newtons_per_meter(), 2000.0, max_relative = 1e-9);
        assert_relative_eq!(d.initial_tension.newtons(), 10.0, max_relative = 1e-9);
        assert_relative_eq!(
            d.load_points[0].length.millimeters(),
            70.0,
            max_relative = 1e-9
        );
        assert_relative_eq!(
            d.load_points[1].length.millimeters(),
            80.0,
            max_relative = 1e-9
        );
    }

    // --- RateBased scenario tests (SpringRate is in the test module's `use` above) ---

    #[test]
    fn rate_based_backs_out_active_coils() {
        // Target k=2000 N/m at d=2mm D=20mm → Na=10 (back-solved).
        let s = RateBased {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            rate: SpringRate::from_newtons_per_meter(2000.0),
            free_length: Length::from_millimeters(60.0),
            initial_tension: Force::from_newtons(10.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            loads: vec![Force::from_newtons(30.0)],
        };
        let d = s
            .solve(
                &crate::test_support::music_wire(),
                CurvatureCorrection::Bergstrasser,
            )
            .unwrap();
        assert_relative_eq!(d.active_coils, 10.0, max_relative = 1e-6);
        assert_relative_eq!(d.rate.newtons_per_meter(), 2000.0, max_relative = 1e-6);
    }

    #[test]
    fn rate_based_rejects_non_positive_rate() {
        // rate=0.0 fires the >0.0 branch; message pinned to catch mutants that let
        // bad values flow to solve_forward (which returns InconsistentInputs with a
        // different message).
        let s = RateBased {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            rate: SpringRate::from_newtons_per_meter(0.0),
            free_length: Length::from_millimeters(60.0),
            initial_tension: Force::from_newtons(10.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            loads: vec![Force::from_newtons(30.0)],
        };
        assert!(
            matches!(
                s.solve(
                    &crate::test_support::music_wire(),
                    CurvatureCorrection::Bergstrasser
                ),
                Err(crate::SpringError::InconsistentInputs(ref m))
                    if m == "required rate must be a positive finite number"
            ),
            "expected RateBased guard message"
        );
    }

    #[test]
    fn rate_based_rejects_non_finite_rate() {
        // rate=+inf passes >0.0 (true), so only is_finite() rejects it — exercises
        // that branch exclusively. NaN would also fail >0.0 so it wouldn't isolate
        // is_finite(); +infinity is the value that requires is_finite() to fire.
        // Pin the exact message to distinguish from solve_forward's own guards.
        let s = RateBased {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            rate: SpringRate::from_newtons_per_meter(f64::INFINITY),
            free_length: Length::from_millimeters(60.0),
            initial_tension: Force::from_newtons(10.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            loads: vec![Force::from_newtons(30.0)],
        };
        assert!(
            matches!(
                s.solve(
                    &crate::test_support::music_wire(),
                    CurvatureCorrection::Bergstrasser
                ),
                Err(crate::SpringError::InconsistentInputs(ref m))
                    if m == "required rate must be a positive finite number"
            ),
            "expected RateBased guard message"
        );
    }

    // --- Dimensional scenario tests ---

    #[test]
    fn dimensional_uses_outer_diameter() {
        // OD=22mm, d=2mm → mean=20mm → C=10.
        let s = Dimensional {
            wire_dia: Length::from_millimeters(2.0),
            outer_dia: Length::from_millimeters(22.0),
            active: 10.0,
            free_length: Length::from_millimeters(60.0),
            initial_tension: Force::from_newtons(10.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            loads: vec![Force::from_newtons(30.0)],
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

    #[test]
    fn dimensional_rejects_non_positive_outer() {
        // outer=0.0 fires the >0.0 branch; message pinned to catch mutants that let
        // bad values flow to solve_forward (which returns InconsistentInputs with a
        // different message).
        let s = Dimensional {
            wire_dia: Length::from_millimeters(2.0),
            outer_dia: Length::from_millimeters(0.0),
            active: 10.0,
            free_length: Length::from_millimeters(60.0),
            initial_tension: Force::from_newtons(10.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            loads: vec![Force::from_newtons(30.0)],
        };
        assert!(
            matches!(
                s.solve(
                    &crate::test_support::music_wire(),
                    CurvatureCorrection::Bergstrasser
                ),
                Err(crate::SpringError::InconsistentInputs(ref m))
                    if m == "outer diameter must be a positive finite number"
            ),
            "expected Dimensional guard message"
        );
    }

    #[test]
    fn dimensional_rejects_non_finite_outer() {
        // outer=+inf passes >0.0 (true), so only is_finite() rejects it — exercises
        // that branch exclusively. NaN would also fail >0.0 so it wouldn't isolate
        // is_finite(); +infinity is the value that requires is_finite() to fire.
        // Pin the exact message to distinguish from solve_forward's own guards.
        let s = Dimensional {
            wire_dia: Length::from_millimeters(2.0),
            outer_dia: Length::from_millimeters(f64::INFINITY),
            active: 10.0,
            free_length: Length::from_millimeters(60.0),
            initial_tension: Force::from_newtons(10.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            loads: vec![Force::from_newtons(30.0)],
        };
        assert!(
            matches!(
                s.solve(
                    &crate::test_support::music_wire(),
                    CurvatureCorrection::Bergstrasser
                ),
                Err(crate::SpringError::InconsistentInputs(ref m))
                    if m == "outer diameter must be a positive finite number"
            ),
            "expected Dimensional guard message"
        );
    }
}
