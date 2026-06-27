//! Static forward solver for helical torsion springs. Validates geometry, computes the
//! angular rate, per-moment load points (deflection, bending stress, wound geometry),
//! and an engineering status. Formulas cited via `super::mechanics`.

use crate::design::{DesignStatus, Severity, StatusMessage};
use crate::material::Material;
use crate::torsion::mechanics::{
    active_coils_with_legs, angular_rate, bending_stress_inner, bending_stress_nominal,
    wound_mean_diameter, FrictionModel,
};
use crate::units::{Angle, AngularRate, Length, Moment, Stress};
use crate::{Result, SpringError};

/// Recommended spring-index band (SMI Handbook; Shigley §10-2), shared across families.
const INDEX_MIN: f64 = 4.0;
const INDEX_MAX: f64 = 12.0;

/// Geometry of a torsion spring. The legs are loaded so the coils wind tighter.
#[derive(Debug, Clone)]
pub struct TorsionInputs {
    /// Wire diameter `d`.
    pub wire_dia: Length,
    /// Mean coil diameter `D`.
    pub mean_dia: Length,
    /// Body (active) coil count `N_b`, excluding the leg contribution.
    pub body_coils: f64,
    /// First straight-leg length `L₁`.
    pub leg1: Length,
    /// Second straight-leg length `L₂`.
    pub leg2: Length,
    /// Optional arbor (mandrel) diameter; when set, enables the wind-up clearance check.
    pub arbor_dia: Option<Length>,
}

/// One operating point: an applied moment and the resulting response.
#[derive(Debug, Clone)]
pub struct TorsionLoadPoint {
    /// Applied moment `M`.
    pub moment: Moment,
    /// Angular deflection `θ = M/k′`.
    pub deflection: Angle,
    /// Inner-fiber bending stress `σᵢ` (critical).
    pub stress_inner: Stress,
    /// Nominal bending stress `σ₀` (reference).
    pub stress_nominal: Stress,
    /// `σᵢ` as a fraction of the bending allowable (`allowable_pct_bending · MTS`).
    pub pct_bending_allow: f64,
    /// Wound-up mean diameter `D′` under this load.
    pub wound_mean_dia: Length,
    /// Wound-up inner diameter `D′ − d` under this load.
    pub wound_inner_dia: Length,
}

/// A fully solved torsion-spring design.
#[derive(Debug, Clone)]
pub struct TorsionDesign {
    /// The geometry that produced this design.
    pub inputs: TorsionInputs,
    /// Spring index `C = D/d`.
    pub index: f64,
    /// Effective active coils `Nₐ` (body + leg contribution).
    pub active_coils: f64,
    /// Angular rate `k′` (per radian).
    pub rate: AngularRate,
    /// One entry per applied moment.
    pub load_points: Vec<TorsionLoadPoint>,
    /// Engineering advisories (overstress, arbor binding, index range).
    pub status: DesignStatus,
}

/// Solve a torsion spring statically for one or more applied moments.
pub fn solve_forward(
    material: &Material,
    inputs: TorsionInputs,
    moments: &[Moment],
    friction: FrictionModel,
) -> Result<TorsionDesign> {
    let d = inputs.wire_dia.meters();
    if !(d.is_finite() && d > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "wire diameter must be a positive finite number".into(),
        ));
    }
    let dm = inputs.mean_dia.meters();
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
    if !(inputs.body_coils.is_finite() && inputs.body_coils > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "body coils must be a positive finite number".into(),
        ));
    }
    for leg in [inputs.leg1.meters(), inputs.leg2.meters()] {
        if !(leg.is_finite() && leg >= 0.0) {
            return Err(SpringError::InconsistentInputs(
                "leg lengths must be finite and non-negative".into(),
            ));
        }
    }
    if let Some(arbor) = inputs.arbor_dia {
        let a = arbor.meters();
        if !(a.is_finite() && a > 0.0) {
            return Err(SpringError::InconsistentInputs(
                "arbor diameter must be a positive finite number".into(),
            ));
        }
    }
    if moments.is_empty() {
        return Err(SpringError::InconsistentInputs(
            "at least one applied moment is required".into(),
        ));
    }
    for m in moments {
        let mv = m.newton_meters();
        if !(mv.is_finite() && mv > 0.0) {
            return Err(SpringError::InconsistentInputs(
                "applied moment must be a positive finite number (load winds the coils tighter)"
                    .into(),
            ));
        }
    }
    // Validate the wire diameter against the material range (surfaces DiameterOutOfRange).
    let mts = material.min_tensile_strength(inputs.wire_dia)?.pascals();
    let allowable = material.allowable_pct_bending * mts;

    let index = dm / d;
    let active =
        active_coils_with_legs(inputs.body_coils, inputs.leg1, inputs.leg2, inputs.mean_dia);
    let rate = angular_rate(
        material.youngs_modulus,
        inputs.wire_dia,
        inputs.mean_dia,
        active,
        friction,
    );

    let load_points: Vec<TorsionLoadPoint> = moments
        .iter()
        .map(|&moment| {
            let deflection =
                Angle::from_radians(moment.newton_meters() / rate.newton_meters_per_radian());
            let stress_inner = bending_stress_inner(moment, inputs.mean_dia, inputs.wire_dia);
            let stress_nominal = bending_stress_nominal(moment, inputs.wire_dia);
            let wound_mean_dia =
                wound_mean_diameter(inputs.mean_dia, inputs.body_coils, deflection);
            let wound_inner_dia = Length::from_meters(wound_mean_dia.meters() - d);
            TorsionLoadPoint {
                moment,
                deflection,
                stress_inner,
                stress_nominal,
                pct_bending_allow: stress_inner.pascals() / allowable,
                wound_mean_dia,
                wound_inner_dia,
            }
        })
        .collect();

    let status = evaluate_status(index, &load_points, inputs.arbor_dia);
    Ok(TorsionDesign {
        inputs,
        index,
        active_coils: active,
        rate,
        load_points,
        status,
    })
}

/// Engineering checks: overstress (inner-fiber), arbor binding under wind-up, index range.
fn evaluate_status(
    index: f64,
    load_points: &[TorsionLoadPoint],
    arbor_dia: Option<Length>,
) -> DesignStatus {
    let mut messages = Vec::new();

    if load_points.iter().any(|lp| lp.pct_bending_allow > 1.0) {
        messages.push(StatusMessage {
            severity: Severity::Warning,
            message: "inner-fiber bending stress exceeds the allowable".into(),
        });
    }
    if let Some(arbor) = arbor_dia {
        if load_points
            .iter()
            .any(|lp| lp.wound_inner_dia.meters() <= arbor.meters())
        {
            messages.push(StatusMessage {
                severity: Severity::Warning,
                message: "spring winds down onto the arbor (inner diameter binds)".into(),
            });
        }
    }
    if !(INDEX_MIN..=INDEX_MAX).contains(&index) {
        messages.push(StatusMessage {
            severity: Severity::Caution,
            message: format!(
                "spring index {index:.2} is outside the recommended range {INDEX_MIN}–{INDEX_MAX}"
            ),
        });
    }

    DesignStatus { messages }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::Severity;
    use crate::torsion::FrictionModel;
    use crate::units::{Angle, Length, Moment, Stress};
    use approx::assert_relative_eq;

    fn inputs() -> TorsionInputs {
        // d=2 mm, D=20 mm (C=10), N_b=5, no legs (Na=5), no arbor.
        TorsionInputs {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            body_coils: 5.0,
            leg1: Length::from_meters(0.0),
            leg2: Length::from_meters(0.0),
            arbor_dia: None,
        }
    }

    #[test]
    fn pure_bending_design_oracle() {
        // Music Wire E=203.4 GPa; PureBending k'=0.5085 N·m/rad (Task 2 oracle).
        // M=1 N·m → θ=1/0.5085=1.96656834 rad; θ_turns=0.31298907.
        // σᵢ = 389/360 × 1.2732395447e9 = 1.375806e9 Pa.
        // D' = 0.02·5/(5+0.31298907) = 0.0188218 m → inner' = 16.821797 mm.
        let m = crate::test_support::music_wire();
        let d = solve_forward(
            &m,
            inputs(),
            &[Moment::from_newton_meters(1.0)],
            FrictionModel::PureBending,
        )
        .unwrap();
        assert_relative_eq!(d.index, 10.0, max_relative = 1e-12);
        assert_relative_eq!(d.active_coils, 5.0, max_relative = 1e-12);
        assert_relative_eq!(
            d.rate.newton_meters_per_radian(),
            0.5085,
            max_relative = 1e-9
        );
        let lp = &d.load_points[0];
        assert_relative_eq!(
            lp.deflection.radians(),
            1.9665683382497539,
            max_relative = 1e-9
        );
        assert_relative_eq!(
            lp.stress_inner.pascals(),
            (389.0 / 360.0) * 1.2732395447351628e9,
            max_relative = 1e-9
        );
        assert_relative_eq!(
            lp.stress_nominal.pascals(),
            1.2732395447351628e9,
            max_relative = 1e-9
        );
        assert_relative_eq!(
            lp.wound_inner_dia.millimeters(),
            16.821797,
            max_relative = 1e-4
        );
    }

    #[test]
    fn shigley_rate_matches_oracle() {
        let m = crate::test_support::music_wire();
        let d = solve_forward(
            &m,
            inputs(),
            &[Moment::from_newton_meters(1.0)],
            FrictionModel::ShigleyFriction,
        )
        .unwrap();
        assert_relative_eq!(
            d.rate.newton_meters_per_radian(),
            0.47958689518357805,
            max_relative = 1e-9
        );
    }

    #[test]
    fn pct_bending_allow_is_sigma_i_over_allowable() {
        // pct = σᵢ / (allowable_pct_bending · MTS(d)). Music Wire: pct_bending=0.75,
        // MTS(2mm)=2211·2^(−0.145) MPa = 2211·0.904181 = 1999.14 MPa.
        let m = crate::test_support::music_wire();
        let d = solve_forward(
            &m,
            inputs(),
            &[Moment::from_newton_meters(1.0)],
            FrictionModel::PureBending,
        )
        .unwrap();
        let mts = m
            .min_tensile_strength(Length::from_millimeters(2.0))
            .unwrap()
            .pascals();
        let expected = ((389.0 / 360.0) * 1.2732395447351628e9) / (0.75 * mts);
        assert_relative_eq!(
            d.load_points[0].pct_bending_allow,
            expected,
            max_relative = 1e-9
        );
    }

    #[test]
    fn overstress_raises_warning() {
        // A large moment drives σᵢ past the allowable → Warning status.
        let m = crate::test_support::music_wire();
        let d = solve_forward(
            &m,
            inputs(),
            &[Moment::from_newton_meters(50.0)],
            FrictionModel::PureBending,
        )
        .unwrap();
        assert!(d.status.has_warnings());
    }

    #[test]
    fn arbor_binding_raises_warning() {
        // Arbor diameter larger than the wound inner diameter → binding Warning.
        let mut i = inputs();
        i.arbor_dia = Some(Length::from_millimeters(19.0)); // inner' ≈ 16.8 mm < 19 mm
        let m = crate::test_support::music_wire();
        let d = solve_forward(
            &m,
            i,
            &[Moment::from_newton_meters(1.0)],
            FrictionModel::PureBending,
        )
        .unwrap();
        assert!(d.status.has_warnings());
    }

    #[test]
    fn arbor_clear_no_binding_warning() {
        let mut i = inputs();
        i.arbor_dia = Some(Length::from_millimeters(10.0)); // inner' ≈ 16.8 mm > 10 mm
        let m = crate::test_support::music_wire();
        let d = solve_forward(
            &m,
            i,
            &[Moment::from_newton_meters(1.0)],
            FrictionModel::PureBending,
        )
        .unwrap();
        // No binding warning (a low moment also keeps stress under allowable).
        assert!(!d.status.has_warnings());
    }

    #[test]
    fn rejects_non_positive_wire_dia() {
        let mut i = inputs();
        i.wire_dia = Length::from_meters(0.0);
        let m = crate::test_support::music_wire();
        let r = solve_forward(
            &m,
            i,
            &[Moment::from_newton_meters(1.0)],
            FrictionModel::PureBending,
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    #[test]
    fn rejects_index_at_or_below_one() {
        let mut i = inputs();
        i.mean_dia = Length::from_millimeters(2.0); // D == d → C = 1
        let m = crate::test_support::music_wire();
        let r = solve_forward(
            &m,
            i,
            &[Moment::from_newton_meters(1.0)],
            FrictionModel::PureBending,
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    #[test]
    fn rejects_non_positive_body_coils() {
        let mut i = inputs();
        i.body_coils = 0.0;
        let m = crate::test_support::music_wire();
        let r = solve_forward(
            &m,
            i,
            &[Moment::from_newton_meters(1.0)],
            FrictionModel::PureBending,
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    #[test]
    fn rejects_negative_leg() {
        let mut i = inputs();
        i.leg1 = Length::from_millimeters(-1.0);
        let m = crate::test_support::music_wire();
        let r = solve_forward(
            &m,
            i,
            &[Moment::from_newton_meters(1.0)],
            FrictionModel::PureBending,
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    #[test]
    fn rejects_non_positive_moment() {
        let m = crate::test_support::music_wire();
        let r = solve_forward(
            &m,
            inputs(),
            &[Moment::from_newton_meters(0.0)],
            FrictionModel::PureBending,
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    #[test]
    fn rejects_empty_moments() {
        let m = crate::test_support::music_wire();
        let r = solve_forward(&m, inputs(), &[], FrictionModel::PureBending);
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    #[test]
    fn rejects_non_positive_arbor() {
        let mut i = inputs();
        i.arbor_dia = Some(Length::from_meters(0.0));
        let m = crate::test_support::music_wire();
        let r = solve_forward(
            &m,
            i,
            &[Moment::from_newton_meters(1.0)],
            FrictionModel::PureBending,
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    #[test]
    fn rejects_infinite_mean_dia() {
        // Kills: `&& → ||` mutant (line 85:25) — infinite dm passes `is_finite() || dm > 0`
        // but fails the original `is_finite() && dm > 0`, so the mutant propagates NaN into
        // the computation and returns a bogus Ok instead of rejecting.
        let mut i = inputs();
        i.mean_dia = Length::from_meters(f64::INFINITY);
        let m = crate::test_support::music_wire();
        let r = solve_forward(
            &m,
            i,
            &[Moment::from_newton_meters(1.0)],
            FrictionModel::PureBending,
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    #[test]
    fn rejects_zero_mean_dia_with_message() {
        // Kills: `> → >=` mutant (line 85:31) — dm=0 passes the mutant guard but hits the
        // subsequent `dm <= d` guard, emitting a different error message. Asserting the exact
        // message text distinguishes the two paths.
        let mut i = inputs();
        i.mean_dia = Length::from_meters(0.0);
        let m = crate::test_support::music_wire();
        match solve_forward(
            &m,
            i,
            &[Moment::from_newton_meters(1.0)],
            FrictionModel::PureBending,
        ) {
            Err(crate::SpringError::InconsistentInputs(msg)) => {
                assert!(msg.contains("positive finite"), "unexpected message: {msg}");
            }
            other => panic!("expected InconsistentInputs, got {other:?}"),
        }
    }

    #[test]
    fn index_below_min_emits_caution() {
        // C = D/d = 11 mm / 3 mm ≈ 3.67 < INDEX_MIN (4.0) → Caution advisory.
        // Kills: `delete !` mutant (line 202:8) — inverted guard would suppress the message.
        let mut i = inputs();
        i.wire_dia = Length::from_millimeters(3.0);
        i.mean_dia = Length::from_millimeters(11.0);
        let m = crate::test_support::music_wire();
        let d = solve_forward(
            &m,
            i,
            &[Moment::from_newton_meters(0.1)],
            FrictionModel::PureBending,
        )
        .unwrap();
        assert!(d
            .status
            .messages
            .iter()
            .any(|msg| msg.severity == Severity::Caution));
    }

    #[test]
    fn index_above_max_emits_caution() {
        // C = D/d = 26 mm / 2 mm = 13.0 > INDEX_MAX (12.0) → Caution advisory.
        // Kills: `delete !` mutant (line 202:8) — inverted guard would suppress the message.
        let mut i = inputs();
        i.mean_dia = Length::from_millimeters(26.0);
        let m = crate::test_support::music_wire();
        let d = solve_forward(
            &m,
            i,
            &[Moment::from_newton_meters(0.1)],
            FrictionModel::PureBending,
        )
        .unwrap();
        assert!(d
            .status
            .messages
            .iter()
            .any(|msg| msg.severity == Severity::Caution));
    }

    #[test]
    fn evaluate_status_at_exact_allowable_no_warning() {
        // pct = 1.0 exactly (spring is at the bending allowable, not over it) must NOT
        // trigger an overstress Warning. Kills: `> → >=` mutant (line 185:57).
        let lp = TorsionLoadPoint {
            moment: Moment::from_newton_meters(1.0),
            deflection: Angle::from_radians(1.0),
            stress_inner: Stress::from_pascals(1.0),
            stress_nominal: Stress::from_pascals(1.0),
            pct_bending_allow: 1.0,
            wound_mean_dia: Length::from_meters(0.02),
            wound_inner_dia: Length::from_meters(0.018),
        };
        // index=10.0 (in range → no Caution), no arbor (no binding check).
        let status = evaluate_status(10.0, std::slice::from_ref(&lp), None);
        assert!(!status.has_warnings());
    }
}
