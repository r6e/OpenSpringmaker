//! Torsion-spring mechanics: bending stress, active coils, angular rate, wind-up geometry.
//! All formulas cited inline (Shigley Ch. 10; EN 13906-3).

use crate::units::{Angle, AngularRate, Length, Moment, Stress};
use std::f64::consts::{PI, TAU};

/// Shigley's empirical per-turn rate denominator with inter-coil friction (Eq. 10-51).
const SHIGLEY_TURN_DENOM: f64 = 10.8;

/// Which angular-rate model the torsion solver uses. Selectable and (in a later GUI
/// phase) persisted, mirroring the shear-stress `CurvatureCorrection` precedent.
///
/// Deliberately NOT `#[non_exhaustive]`: `springcore` is an unpublished workspace crate
/// and the GUI will match this enum (variant → label), where a future variant should
/// force a compile error rather than a silent fallback (per the PR #32 scope decision).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum FrictionModel {
    /// Shigley Eq. 10-51 with empirical inter-coil friction (10.8 per turn). Default.
    #[default]
    ShigleyFriction,
    /// Pure-bending energy method (EN 13906-3; 64 per radian). No friction allowance.
    PureBending,
}

impl std::fmt::Display for FrictionModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            FrictionModel::ShigleyFriction => "Shigley (with friction)",
            FrictionModel::PureBending => "Pure bending (EN 13906-3)",
        })
    }
}

/// All friction models in pick-list display order.
pub const ALL_FRICTION_MODELS: &[FrictionModel] =
    &[FrictionModel::ShigleyFriction, FrictionModel::PureBending];

/// Inner-fiber bending stress-correction factor K_bi for round wire (Shigley Eq. 10-43):
/// `K_bi = (4C² − C − 1) / (4C(C − 1))`, where `C` is the spring index `D/d`. The inner
/// fiber carries the maximum bending stress and governs design.
pub fn kbi_factor(c: f64) -> f64 {
    (4.0 * c * c - c - 1.0) / (4.0 * c * (c - 1.0))
}

/// Nominal (uncorrected) bending stress `σ₀ = 32M/(πd³)` (Shigley Eq. 10-40 form).
pub fn bending_stress_nominal(moment: Moment, wire_dia: Length) -> Stress {
    let d = wire_dia.meters();
    Stress::from_pascals(32.0 * moment.newton_meters() / (PI * d.powi(3)))
}

/// Inner-fiber bending stress `σᵢ = K_bi · 32M/(πd³)` (Shigley Eq. 10-43), the critical
/// stress checked against the bending allowable.
pub fn bending_stress_inner(moment: Moment, mean_dia: Length, wire_dia: Length) -> Stress {
    let c = mean_dia.meters() / wire_dia.meters();
    Stress::from_pascals(kbi_factor(c) * bending_stress_nominal(moment, wire_dia).pascals())
}

/// Effective active coils including the straight-leg contribution (Shigley Eq. 10-50):
/// `Nₐ = N_b + (L₁ + L₂)/(3πD)`.
pub fn active_coils_with_legs(
    body_coils: f64,
    leg1: Length,
    leg2: Length,
    mean_dia: Length,
) -> f64 {
    let legs = leg1.meters() + leg2.meters();
    body_coils + legs / (3.0 * PI * mean_dia.meters())
}

/// Angular spring rate `k′ = M/θ` per radian.
///
/// - `PureBending` (EN 13906-3, energy method): `k′ = E·d⁴/(64·D·Nₐ)`.
/// - `ShigleyFriction` (Shigley Eq. 10-51): the 10.8-per-turn form with empirical
///   inter-coil friction, converted to per-radian: `k′ = E·d⁴/(2π·10.8·D·Nₐ)`.
pub fn angular_rate(
    youngs_modulus: Stress,
    wire_dia: Length,
    mean_dia: Length,
    active: f64,
    friction: FrictionModel,
) -> AngularRate {
    let e = youngs_modulus.pascals();
    let d = wire_dia.meters();
    let dm = mean_dia.meters();
    let denom_factor = match friction {
        FrictionModel::PureBending => 64.0,
        FrictionModel::ShigleyFriction => TAU * SHIGLEY_TURN_DENOM,
    };
    AngularRate::from_newton_meters_per_radian(e * d.powi(4) / (denom_factor * dm * active))
}

/// Wound-up mean diameter under load (Shigley Eq. 10-49): as the spring winds in the
/// load direction the body coils tighten, `D′ = D·N_b/(N_b + θ_turns)`.
pub fn wound_mean_diameter(mean_dia: Length, body_coils: f64, deflection: Angle) -> Length {
    let theta_turns = deflection.turns();
    Length::from_meters(mean_dia.meters() * body_coils / (body_coils + theta_turns))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{Angle, Length, Moment, Stress};
    use approx::assert_relative_eq;

    #[test]
    fn kbi_at_index_ten() {
        // K_bi = (4C²−C−1)/(4C(C−1)); C=10 → (400−10−1)/(360) = 389/360.
        assert_relative_eq!(kbi_factor(10.0), 389.0 / 360.0, max_relative = 1e-12);
    }

    #[test]
    fn nominal_bending_stress_value() {
        // σ₀ = 32M/(πd³); M=1 N·m, d=2 mm → 32/(π·8e-9) = 1.2732395447e9 Pa.
        let s = bending_stress_nominal(
            Moment::from_newton_meters(1.0),
            Length::from_millimeters(2.0),
        );
        assert_relative_eq!(s.pascals(), 1.2732395447351628e9, max_relative = 1e-9);
    }

    #[test]
    fn inner_bending_stress_applies_kbi() {
        // σᵢ = K_bi·σ₀; C=10 → 389/360 × 1.2732395447e9 = 1.375806...e9 Pa.
        let si = bending_stress_inner(
            Moment::from_newton_meters(1.0),
            Length::from_millimeters(20.0),
            Length::from_millimeters(2.0),
        );
        assert_relative_eq!(
            si.pascals(),
            (389.0 / 360.0) * 1.2732395447351628e9,
            max_relative = 1e-9
        );
    }

    #[test]
    fn active_coils_adds_leg_term() {
        // Na = N_b + (L1+L2)/(3πD); N_b=5, L1=L2=50 mm, D=20 mm.
        // (0.05+0.05)/(3π·0.02) = 0.1/0.1884955592 = 0.5305164769.
        let na = active_coils_with_legs(
            5.0,
            Length::from_millimeters(50.0),
            Length::from_millimeters(50.0),
            Length::from_millimeters(20.0),
        );
        assert_relative_eq!(na, 5.530516476972984, max_relative = 1e-12);
    }

    #[test]
    fn active_coils_body_only_when_no_legs() {
        let na = active_coils_with_legs(
            5.0,
            Length::from_meters(0.0),
            Length::from_meters(0.0),
            Length::from_millimeters(20.0),
        );
        assert_relative_eq!(na, 5.0, max_relative = 1e-12);
    }

    #[test]
    fn pure_bending_rate_value() {
        // k' = E·d⁴/(64·D·Na); E=203.4 GPa, d=2 mm, D=20 mm, Na=5.
        // = 203.4e9·1.6e-11/(64·0.02·5) = 3.2544/6.4 = 0.5085 N·m/rad (exact).
        let k = angular_rate(
            Stress::from_pascals(203.4e9),
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            5.0,
            FrictionModel::PureBending,
        );
        assert_relative_eq!(k.newton_meters_per_radian(), 0.5085, max_relative = 1e-12);
    }

    #[test]
    fn shigley_rate_is_softer_than_pure_bending() {
        // k' = E·d⁴/(2π·10.8·D·Na) = 3.2544/(67.85840132·0.02·5) = 3.2544/6.785840132
        //    = 0.47958689518357805 N·m/rad (softer than the 0.5085 pure-bending value).
        let k = angular_rate(
            Stress::from_pascals(203.4e9),
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            5.0,
            FrictionModel::ShigleyFriction,
        );
        assert_relative_eq!(
            k.newton_meters_per_radian(),
            0.47958689518357805,
            max_relative = 1e-9
        );
    }

    #[test]
    fn friction_model_default_is_shigley() {
        assert_eq!(FrictionModel::default(), FrictionModel::ShigleyFriction);
    }

    #[test]
    fn friction_model_display_names() {
        assert_eq!(
            FrictionModel::ShigleyFriction.to_string(),
            "Shigley (with friction)"
        );
        assert_eq!(
            FrictionModel::PureBending.to_string(),
            "Pure bending (EN 13906-3)"
        );
    }

    #[test]
    fn wound_diameter_shrinks_under_load() {
        // D' = D·N_b/(N_b + θ_turns); D=20 mm, N_b=5, θ=1 turn → 0.02·5/6 = 16.6667 mm.
        let dprime =
            wound_mean_diameter(Length::from_millimeters(20.0), 5.0, Angle::from_turns(1.0));
        assert_relative_eq!(dprime.millimeters(), 100.0 / 6.0, max_relative = 1e-12);
    }
}
