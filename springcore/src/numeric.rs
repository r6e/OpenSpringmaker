//! Derivative-free bracketed root finder. Uses the Illinois variant of the
//! false-position (regula falsi) method, which is guaranteed to keep the root
//! bracketed and converges superlinearly.
//!
//! Reference: Dowell, M. & Jarratt, P. (1971), "A modified regula falsi method
//! for computing the root of an equation," BIT 11(2), 168–174.

use crate::error::{Result, SpringError};

/// Convergence configuration for [`find_root_bracketed`].
#[derive(Debug, Clone, Copy)]
pub struct SolveConfig {
    /// Stop when the bracket width is below this.
    pub x_tol: f64,
    /// Stop when |f(c)| is below this.
    pub f_tol: f64,
    /// Maximum iterations before reporting non-convergence.
    pub max_iter: u32,
}

impl Default for SolveConfig {
    fn default() -> Self {
        Self {
            x_tol: 1e-12,
            f_tol: 1e-12,
            max_iter: 200,
        }
    }
}

/// Find a root of `f` within `[lo, hi]`, which must bracket a sign change.
pub fn find_root_bracketed<F: Fn(f64) -> f64>(
    f: F,
    lo: f64,
    hi: f64,
    cfg: SolveConfig,
) -> Result<f64> {
    let (mut a, mut b) = (lo, hi);
    let (mut fa, mut fb) = (f(a), f(b));
    if fa == 0.0 {
        return Ok(a);
    }
    if fb == 0.0 {
        return Ok(b);
    }
    if fa.signum() == fb.signum() {
        return Err(SpringError::InvalidBracket);
    }
    // side tracks which endpoint was retained last, for the Illinois halving.
    let mut side: i8 = 0;
    for _ in 0..cfg.max_iter {
        let c = (a * fb - b * fa) / (fb - fa);
        let fc = f(c);
        if fc.abs() < cfg.f_tol || (b - a).abs() < cfg.x_tol {
            return Ok(c);
        }
        if fc.signum() == fb.signum() {
            b = c;
            fb = fc;
            if side == -1 {
                fa *= 0.5;
            }
            side = -1;
        } else {
            a = c;
            fa = fc;
            if side == 1 {
                fb *= 0.5;
            }
            side = 1;
        }
    }
    Err(SpringError::NonConvergence {
        iterations: cfg.max_iter,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SpringError;
    use approx::assert_relative_eq;

    #[test]
    fn finds_sqrt2() {
        let r = find_root_bracketed(|x| x * x - 2.0, 0.0, 2.0, SolveConfig::default()).unwrap();
        assert_relative_eq!(r, std::f64::consts::SQRT_2, max_relative = 1e-10);
    }

    #[test]
    fn finds_cube_root() {
        let r =
            find_root_bracketed(|x| x * x * x - 27.0, 0.0, 10.0, SolveConfig::default()).unwrap();
        assert_relative_eq!(r, 3.0, max_relative = 1e-10);
    }

    #[test]
    fn rejects_bracket_without_sign_change() {
        let err =
            find_root_bracketed(|x| x * x - 2.0, 2.0, 3.0, SolveConfig::default()).unwrap_err();
        assert_eq!(err, SpringError::InvalidBracket);
    }

    #[test]
    fn reports_non_convergence() {
        let cfg = SolveConfig {
            x_tol: 1e-18,
            f_tol: 1e-18,
            max_iter: 1,
        };
        let err = find_root_bracketed(|x| x * x * x - 2.0, 0.0, 2.0, cfg).unwrap_err();
        assert_eq!(err, SpringError::NonConvergence { iterations: 1 });
    }

    #[test]
    fn detects_root_at_endpoint() {
        let r = find_root_bracketed(|x| x - 1.0, 1.0, 5.0, SolveConfig::default()).unwrap();
        assert_relative_eq!(r, 1.0, max_relative = 1e-12);
    }
}
