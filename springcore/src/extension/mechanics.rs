//! Extension-spring-specific mechanics: hook curvature factors and stresses,
//! and initial-tension deflection. Body rate/stress reuse `crate::mechanics`.

use crate::units::{Force, Length, Stress};
use std::f64::consts::PI;

/// Hook bending curvature factor at point A (Shigley, extension springs):
/// (K)_A = (4·C1² − C1 − 1) / (4·C1·(C1 − 1)), with C1 = 2·r1/d.
pub fn hook_bending_factor(c1: f64) -> f64 {
    (4.0 * c1 * c1 - c1 - 1.0) / (4.0 * c1 * (c1 - 1.0))
}

/// Hook torsion curvature factor at point B (Shigley, extension springs):
/// (K)_B = (4·C2 − 1) / (4·C2 − 4), with C2 = 2·r2/d.
pub fn hook_torsion_factor(c2: f64) -> f64 {
    (4.0 * c2 - 1.0) / (4.0 * c2 - 4.0)
}

/// Hook bending stress at point A (Shigley): σ_A = F[(K)_A·16D/(πd³) + 4/(πd²)].
pub fn hook_bending_stress(force: Force, mean_dia: Length, wire_dia: Length, r1: Length) -> Stress {
    let (f, d, dia) = (force.newtons(), wire_dia.meters(), mean_dia.meters());
    let c1 = 2.0 * r1.meters() / d;
    let ka = hook_bending_factor(c1);
    let sigma = f * (ka * 16.0 * dia / (PI * d.powi(3)) + 4.0 / (PI * d * d));
    Stress::from_pascals(sigma)
}

/// Hook torsional stress at point B (Shigley): τ_B = (K)_B·8FD/(πd³).
pub fn hook_torsion_stress(force: Force, mean_dia: Length, wire_dia: Length, r2: Length) -> Stress {
    let (f, d, dia) = (force.newtons(), wire_dia.meters(), mean_dia.meters());
    let c2 = 2.0 * r2.meters() / d;
    let kb = hook_torsion_factor(c2);
    Stress::from_pascals(kb * 8.0 * f * dia / (PI * d.powi(3)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn hook_bending_factor_c1_10() {
        // (4·100 − 10 − 1)/(4·10·9) = 389/360.
        assert_relative_eq!(
            hook_bending_factor(10.0),
            389.0 / 360.0,
            max_relative = 1e-12
        );
    }

    #[test]
    fn hook_torsion_factor_c2_5() {
        // (20 − 1)/(20 − 4) = 19/16.
        assert_relative_eq!(hook_torsion_factor(5.0), 19.0 / 16.0, max_relative = 1e-12);
    }

    #[test]
    fn hook_bending_stress_matches_hand_calc() {
        let s = hook_bending_stress(
            Force::from_newtons(100.0),
            Length::from_millimeters(20.0),
            Length::from_millimeters(2.0),
            Length::from_millimeters(10.0),
        );
        // σ_A = 100·[ (389/360)·16·0.02/(π·0.002³) + 4/(π·0.002²) ] ≈ 1.4076e9 Pa.
        assert_relative_eq!(s.pascals(), 1.40765e9, max_relative = 1e-4);
    }

    #[test]
    fn hook_torsion_stress_matches_hand_calc() {
        let s = hook_torsion_stress(
            Force::from_newtons(100.0),
            Length::from_millimeters(20.0),
            Length::from_millimeters(2.0),
            Length::from_millimeters(5.0),
        );
        // τ_B = (19/16)·8·100·0.02/(π·0.002³) ≈ 7.560e8 Pa.
        assert_relative_eq!(s.pascals(), 7.5599e8, max_relative = 1e-4);
    }
}
