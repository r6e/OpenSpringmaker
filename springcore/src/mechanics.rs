//! Governing mechanics for cylindrical helical compression springs of round wire.
//! Each formula cites its source.

use crate::units::{Force, Frequency, Length, MassDensity, SpringRate, Stress};
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

/// Spring index C = D/d (Shigley Eq. 10-1).
pub fn spring_index(mean_dia: Length, wire_dia: Length) -> f64 {
    mean_dia.meters() / wire_dia.meters()
}

/// Wahl curvature-and-shear correction factor (Wahl 1963; Shigley Eq. 10-5):
/// Kw = (4C-1)/(4C-4) + 0.615/C. EN 13906-1:2013 lists this as the alternative
/// to Bergsträsser in the NOTE under Formula (1).
pub fn wahl_factor(index: f64) -> f64 {
    (4.0 * index - 1.0) / (4.0 * index - 4.0) + 0.615 / index
}

/// Bergsträsser correction factor (Shigley Eq. 10-6): Kb = (4C+2)/(4C-3).
/// This is EN 13906-1:2013 Formula (1), `k = (w+0.5)/(w−0.75)`, written over the
/// common denominator (multiply numerator and denominator by 4); the standard's
/// primary stress-correction factor.
pub fn bergstrasser_factor(index: f64) -> f64 {
    (4.0 * index + 2.0) / (4.0 * index - 3.0)
}

/// Wire-curvature stress-correction model for body torsional shear. Both are
/// accepted; EN 13906-1:2013 Formula (1) and Shigley name Bergsträsser primary
/// (the default), with Wahl the documented alternative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CurvatureCorrection {
    /// Wahl factor (Shigley Eq. 10-5; EN 13906-1 NOTE alternative).
    Wahl,
    /// Bergsträsser factor (Shigley Eq. 10-6; EN 13906-1 Formula (1)).
    #[default]
    Bergstrasser,
}

impl CurvatureCorrection {
    /// The chosen curvature-correction factor at spring index `index`.
    pub fn factor(self, index: f64) -> f64 {
        match self {
            Self::Wahl => wahl_factor(index),
            Self::Bergstrasser => bergstrasser_factor(index),
        }
    }
}

/// Spring rate k = G d^4 / (8 D^3 Na) (Shigley Eq. 10-9; EN 13906-1:2013 Formula (6)).
pub fn spring_rate(
    shear_modulus: Stress,
    wire_dia: Length,
    mean_dia: Length,
    active: f64,
) -> SpringRate {
    let g = shear_modulus.pascals();
    let d = wire_dia.meters();
    let dm = mean_dia.meters();
    SpringRate::from_newtons_per_meter(g * d.powi(4) / (8.0 * dm.powi(3) * active))
}

/// Active coils required for a target rate (inverse of `spring_rate`;
/// EN 13906-1:2013 Formula (11): n = G d^4 s / (8 D^3 F)).
pub fn active_coils_for_rate(
    shear_modulus: Stress,
    wire_dia: Length,
    mean_dia: Length,
    rate: SpringRate,
) -> f64 {
    let g = shear_modulus.pascals();
    let d = wire_dia.meters();
    let dm = mean_dia.meters();
    g * d.powi(4) / (8.0 * dm.powi(3) * rate.newtons_per_meter())
}

/// Corrected shear stress tau = K * 8 F D / (pi d^3) (Shigley Eq. 10-7;
/// EN 13906-1:2013 uncorrected Formula (7) τ = 8 D F / (π d^3), corrected via
/// Formula (9) τk = k·τ). `factor` is the chosen correction factor (Wahl or
/// Bergsträsser).
pub fn corrected_shear_stress(
    force: Force,
    mean_dia: Length,
    wire_dia: Length,
    factor: f64,
) -> Stress {
    let f = force.newtons();
    let dm = mean_dia.meters();
    let d = wire_dia.meters();
    Stress::from_pascals(factor * 8.0 * f * dm / (PI * d.powi(3)))
}

/// Fundamental natural frequency of a compression spring with both ends against
/// fixed/parallel plates (Shigley Eq. 10-25):
/// fn = (d / (2*pi*Na*D^2)) * sqrt(G / (32*rho)), rho = mass density.
/// Note: this formula does not take an EndFixity parameter.
pub fn natural_frequency(
    wire_dia: Length,
    mean_dia: Length,
    active: f64,
    shear_modulus: Stress,
    density: MassDensity,
) -> Frequency {
    let d = wire_dia.meters();
    let dm = mean_dia.meters();
    let g = shear_modulus.pascals();
    let rho = density.kg_per_m3();
    let hz = (d / (2.0 * PI * active * dm.powi(2))) * (g / (32.0 * rho)).sqrt();
    Frequency::from_hertz(hz)
}

/// End-condition constant alpha for buckling (Shigley Table 10-2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EndFixity {
    /// Both ends squared and ground, between parallel plates.
    FixedFixed,
    /// One end fixed, the other pivoted.
    FixedPinned,
    /// Both ends pivoted.
    PinnedPinned,
    /// One end fixed, the other free.
    FixedFree,
}

impl EndFixity {
    /// End-condition constant α used in buckling calculations (Shigley Table 10-2).
    pub fn alpha(self) -> f64 {
        match self {
            Self::FixedFixed => 0.5,
            Self::FixedPinned => 0.707,
            Self::PinnedPinned => 1.0,
            Self::FixedFree => 2.0,
        }
    }
}

/// Critical free length for absolute stability (Shigley Eq. 10-10):
/// L0_cr = (pi*D/alpha) * sqrt(2(E-G)/(2G+E)).
/// A spring with L0 below this cannot buckle at any deflection (conservative; the
/// deflection-ratio refinement of Eq. 10-11 is deferred — see ADR 0005).
pub fn critical_free_length(
    mean_dia: Length,
    youngs_modulus: Stress,
    shear_modulus: Stress,
    fixity: EndFixity,
) -> Length {
    let dm = mean_dia.meters();
    let e = youngs_modulus.pascals();
    let g = shear_modulus.pascals();
    let l0 = (PI * dm / fixity.alpha()) * (2.0 * (e - g) / (2.0 * g + e)).sqrt();
    Length::from_meters(l0)
}

/// True when the spring's free length is at or below the absolute-stability limit.
pub fn is_buckling_stable(
    free_length: Length,
    mean_dia: Length,
    youngs_modulus: Stress,
    shear_modulus: Stress,
    fixity: EndFixity,
) -> bool {
    free_length.meters()
        <= critical_free_length(mean_dia, youngs_modulus, shear_modulus, fixity).meters()
}

#[cfg(test)]
mod tests {
    use super::{
        active_coils_for_rate, bergstrasser_factor, corrected_shear_stress, critical_free_length,
        is_buckling_stable, natural_frequency, spring_index, spring_rate, wahl_factor,
        CurvatureCorrection, EndFixity,
    };
    use crate::units::{Force, Length, MassDensity, SpringRate, Stress};
    use approx::assert_relative_eq;

    #[test]
    fn index_is_d_over_d() {
        let c = spring_index(
            Length::from_millimeters(10.0),
            Length::from_millimeters(1.0),
        );
        assert_relative_eq!(c, 10.0, max_relative = 1e-12);
    }

    #[test]
    fn wahl_factor_c10() {
        // Kw = (4C-1)/(4C-4) + 0.615/C; C=10 -> 39/36 + 0.0615
        assert_relative_eq!(
            wahl_factor(10.0),
            39.0 / 36.0 + 0.0615,
            max_relative = 1e-12
        );
    }

    #[test]
    fn bergstrasser_factor_c10() {
        // Kb = (4C+2)/(4C-3); C=10 -> 42/37
        assert_relative_eq!(bergstrasser_factor(10.0), 42.0 / 37.0, max_relative = 1e-12);
    }

    #[test]
    fn correction_factor_dispatches() {
        // factor() returns exactly the chosen closed-form factor.
        for &c in &[5.0_f64, 8.0, 10.8] {
            assert_relative_eq!(
                CurvatureCorrection::Wahl.factor(c),
                wahl_factor(c),
                max_relative = 1e-12
            );
            assert_relative_eq!(
                CurvatureCorrection::Bergstrasser.factor(c),
                bergstrasser_factor(c),
                max_relative = 1e-12
            );
        }
    }

    #[test]
    fn correction_default_is_bergstrasser() {
        // EN 13906-1 / Shigley name Bergsträsser the primary factor.
        assert_eq!(
            CurvatureCorrection::default(),
            CurvatureCorrection::Bergstrasser
        );
    }

    #[test]
    fn correction_serde_round_trips() {
        // Persisted in Phase B's settings file; lowercase token form.
        let json = serde_json::to_string(&CurvatureCorrection::Wahl).unwrap();
        assert_eq!(json, "\"wahl\"");
        let back: CurvatureCorrection = serde_json::from_str(&json).unwrap();
        assert_eq!(back, CurvatureCorrection::Wahl);
    }

    /// Provenance: `bergstrasser_factor` is exactly EN 13906-1:2013 Formula (1),
    /// `k = (w+0.5)/(w−0.75)` — our `(4w+2)/(4w−3)` is the same identity over a
    /// common denominator. Asserting against the standard's printed form (a
    /// genuinely distinct expression) catches a drift either way. (Wahl, the EN
    /// NOTE alternative, has no distinct closed form to cross-check — it is pinned
    /// numerically by `wahl_factor_c10`.)
    #[test]
    fn en_13906_1_bergstrasser_provenance() {
        for &w in &[5.0_f64, 8.0, 10.8, 12.0] {
            assert_relative_eq!(
                bergstrasser_factor(w),
                (w + 0.5) / (w - 0.75),
                max_relative = 1e-12
            );
        }
    }

    #[test]
    fn rate_clean_case() {
        // k = G d^4 / (8 D^3 Na); G=80e9, d=1mm, D=10mm, Na=10 -> exactly 1000 N/m
        let k = spring_rate(
            Stress::from_pascals(80.0e9),
            Length::from_millimeters(1.0),
            Length::from_millimeters(10.0),
            10.0,
        );
        assert_relative_eq!(k.newtons_per_meter(), 1000.0, max_relative = 1e-12);
    }

    #[test]
    fn active_coils_inverts_rate() {
        let na = active_coils_for_rate(
            Stress::from_pascals(80.0e9),
            Length::from_millimeters(1.0),
            Length::from_millimeters(10.0),
            SpringRate::from_newtons_per_meter(1000.0),
        );
        assert_relative_eq!(na, 10.0, max_relative = 1e-12);
    }

    #[test]
    fn corrected_stress_c10() {
        // tau = Kw * 8 F D / (pi d^3); F=10 N, D=10mm, d=1mm, Kw=39/36+0.0615
        let kw = 39.0 / 36.0 + 0.0615;
        let s = corrected_shear_stress(
            Force::from_newtons(10.0),
            Length::from_millimeters(10.0),
            Length::from_millimeters(1.0),
            kw,
        );
        let expected = kw * 8.0 * 10.0 * 0.010 / (std::f64::consts::PI * 0.001_f64.powi(3));
        assert_relative_eq!(s.pascals(), expected, max_relative = 1e-12);
    }

    #[test]
    fn natural_frequency_case() {
        // fn = (d/(2*pi*Na*D^2)) * sqrt(G/(32*rho))
        let f = natural_frequency(
            Length::from_millimeters(1.0),
            Length::from_millimeters(10.0),
            10.0,
            Stress::from_pascals(80.0e9),
            MassDensity::from_kg_per_m3(7850.0),
        );
        let expected = (0.001 / (2.0 * std::f64::consts::PI * 10.0 * 0.010_f64.powi(2)))
            * (80.0e9_f64 / (32.0 * 7850.0)).sqrt();
        assert_relative_eq!(f.hertz(), expected, max_relative = 1e-12);
    }

    #[test]
    fn buckling_critical_length() {
        // L0_cr = (pi D / alpha) * sqrt(2(E-G)/(2G+E)); E=200e9, G=80e9, fixed-fixed alpha=0.5
        let l = critical_free_length(
            Length::from_millimeters(10.0),
            Stress::from_pascals(200.0e9),
            Stress::from_pascals(80.0e9),
            EndFixity::FixedFixed,
        );
        let expected = (std::f64::consts::PI * 0.010 / 0.5)
            * (2.0_f64 * (200.0e9 - 80.0e9) / (2.0_f64 * 80.0e9 + 200.0e9)).sqrt();
        assert_relative_eq!(l.meters(), expected, max_relative = 1e-12);
        // A spring shorter than critical is stable; far longer is not.
        assert!(is_buckling_stable(
            Length::from_meters(expected * 0.5),
            Length::from_millimeters(10.0),
            Stress::from_pascals(200.0e9),
            Stress::from_pascals(80.0e9),
            EndFixity::FixedFixed
        ));
        assert!(!is_buckling_stable(
            Length::from_meters(expected * 2.0),
            Length::from_millimeters(10.0),
            Stress::from_pascals(200.0e9),
            Stress::from_pascals(80.0e9),
            EndFixity::FixedFixed
        ));
    }
}
