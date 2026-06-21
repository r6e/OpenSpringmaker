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

    // --- f_tol convergence path ---
    // f(x) = x - 5.0 on [4, 7]: false position reaches the exact root in one
    // iteration (fc = 0.0). With the correct `<`, `0.0 < f_tol` → converge.
    // Under the `< → ==` mutant at col 21, `0.0 == 1e-12` is false, so it
    // loops forever and hits NonConvergence (max_iter = 2). Test kills it.
    #[test]
    fn converges_via_f_tol_kills_eq_mutant() {
        let cfg = SolveConfig {
            f_tol: 1e-12,
            x_tol: 1e-30, // practically never triggers
            max_iter: 2,
        };
        let r = find_root_bracketed(|x| x - 5.0, 4.0, 7.0, cfg).unwrap();
        assert_relative_eq!(r, 5.0, max_relative = 1e-10);
    }

    // --- `< → <=` boundary test: kills both remaining <= mutants ---
    // f(x) = x - 5.0 on [4, 7] with f_tol = 0.0 and x_tol = 3.0.
    //
    // On iteration 1: c = (4·2 - 7·(-1))/(2-(-1)) = 15/3 = 5.0 (exact),
    // fc = 0.0 (exact). The convergence check evaluates:
    //   - f_tol clause: `|fc| < 0.0` = `0.0 < 0.0` → false (correct `<`).
    //     `<=` mutant: `0.0 <= 0.0` → true → returns Ok early.
    //   - x_tol clause: `|(b-a)| < 3.0` = `3.0 < 3.0` → false (correct `<`).
    //     `<=` mutant: `3.0 <= 3.0` → true → returns Ok early.
    //
    // Correct code exhausts max_iter=1 → NonConvergence. Each `<=` mutant
    // triggers its clause and returns Ok instead. Test asserts Err, killing both.
    #[test]
    fn strict_tolerance_boundaries_do_not_converge_early() {
        let cfg = SolveConfig {
            f_tol: 0.0, // makes fc.abs() == f_tol exactly (both are 0.0)
            x_tol: 3.0, // makes (b-a).abs() == x_tol exactly (7.0-4.0 = 3.0)
            max_iter: 1,
        };
        let result = find_root_bracketed(|x| x - 5.0, 4.0, 7.0, cfg);
        assert_eq!(
            result.unwrap_err(),
            SpringError::NonConvergence { iterations: 1 }
        );
    }

    // --- x_tol convergence path: b - a vs b + a vs b / a ---
    // Use a nonlinear function so neither endpoint is an exact root.
    // f_tol = 0.0 so f_tol branch is NEVER taken (fc.abs() >= 0.0 always).
    // Root at x = 3 for f(x) = x^3 - 27 on [2, 4].
    //   Correct code: `(b - a).abs() < x_tol = 1e-4` eventually fires → Ok.
    //   `b - a → b + a` mutant: (b+a).abs() ≈ 6.0 >> 1e-4 → never fires → NonConvergence.
    //   `b - a → b / a` mutant: (b/a).abs() ≈ 1.0 >> 1e-4 → never fires → NonConvergence.
    //   `< → ==` mutant: exact bracket == x_tol almost never → NonConvergence.
    #[test]
    fn converges_via_x_tol_kills_bracket_mutants() {
        let cfg = SolveConfig {
            f_tol: 0.0, // disable f_tol path
            x_tol: 1e-4,
            max_iter: 200,
        };
        let r = find_root_bracketed(|x| x * x * x - 27.0, 2.0, 4.0, cfg).unwrap();
        assert_relative_eq!(r, 3.0, max_relative = 1e-3);
    }

    // --- Illinois halving: fb-path stagnation (fb *= 0.5, lines 67-70) ---
    // f(x) = 1 - x^9 on [0, 2]: root at x = 1. f(0) = 1, f(2) = -511.
    // False position repeatedly produces c near 0 (same sign as fa = f(0) > 0),
    // so a keeps moving and b = 2 stays fixed — the a-side stagnation case.
    // Illinois halves fb after two consecutive a-side updates, avoiding the fix.
    //
    // Mutants killed (lines 67-70):
    //   `side == 1` → `side != 1`: fb halved at wrong times → scrambled convergence.
    //   `fb *= 0.5` → `fb += 0.5`: additive instead of halving.
    //   `fb *= 0.5` → `fb /= 0.5`: doubling instead of halving.
    //   `side = 1` → `side = -1` (line 70): side never becomes 1, fb halving
    //   never fires → pure regula falsi → needs far more iterations.
    // max_iter = 50 is sufficient with correct Illinois but not without it.
    #[test]
    fn illinois_fb_halving_enables_convergence() {
        let cfg = SolveConfig {
            f_tol: 1e-10,
            x_tol: 1e-10,
            max_iter: 50,
        };
        let r = find_root_bracketed(|x| 1.0 - x.powi(9), 0.0, 2.0, cfg).unwrap();
        assert_relative_eq!(r, 1.0, max_relative = 1e-8);
    }

    // --- Illinois halving: fa-path stagnation (fa *= 0.5, lines 60-63) ---
    // Same function x^9 - 1, but with lo and hi swapped so a = 2, b = 0.
    // f(2) = 511 (large), f(0) = -1 (small). False position produces c near 0
    // (same sign as fb = f(0) < 0), so b keeps moving and a = 2 stays fixed —
    // the b-side stagnation case. Illinois halves fa after two consecutive
    // b-side updates (lines 60-61).
    //
    // Mutants killed (lines 60-63):
    //   `side == -1` → `side != -1` (line 60): fa halved when side is 0 or 1
    //   (wrong moments) → scrambled convergence.
    //   `delete -` at col 24 (line 60 → `if side == 1`): fa halved when side is 1
    //   instead of -1; the b-retention path never triggers fa halving → stagnation.
    //   `fa *= 0.5` → `fa += 0.5` (line 61): additive instead of halving.
    //   `fa *= 0.5` → `fa /= 0.5` (line 61): doubling instead of halving.
    //   `delete -` at col 20 (line 63 → `side = 1`): side never becomes -1, so
    //   `if side == -1` never fires → fa never halved → pure regula falsi stagnation.
    #[test]
    fn illinois_fa_halving_enables_convergence() {
        let cfg = SolveConfig {
            f_tol: 1e-10,
            x_tol: 1e-10,
            max_iter: 50,
        };
        // Reversed bracket: a = 2.0 (f = 511, large), b = 0.0 (f = -1, small).
        // False position stagnates with a fixed; Illinois fa halving rescues it.
        let r = find_root_bracketed(|x| x.powi(9) - 1.0, 2.0, 0.0, cfg).unwrap();
        assert_relative_eq!(r, 1.0, max_relative = 1e-8);
    }
}
