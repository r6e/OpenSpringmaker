//! Extension-spring-specific mechanics: hook curvature factors and stresses,
//! and initial-tension deflection. Body rate/stress reuse `crate::mechanics`.

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
}
