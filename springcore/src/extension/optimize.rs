//! Minimum-weight constrained optimization for extension springs.
//! Figure of merit = wire mass. For a given wire size the optimum mean diameter
//! is the largest that keeps all three stresses — body shear, hook bending (σ_A),
//! and hook torsion (τ_B) — within allowable, so it lies on the binding stress,
//! the index ceiling, or the outer-diameter cap. Mirrors `crate::optimize`
//! (compression) without buckling/solid-length (an extension spring is loaded in
//! tension). See `docs/superpowers/specs/2026-06-26-extension-min-weight-design.md`.

use crate::extension::design::{solve_forward, ExtensionDesign};
use crate::extension::ends::HookEnds;
use crate::extension::mechanics::{
    free_length_from_geometry, hook_bending_stress, hook_torsion_stress,
};
use crate::material::Material;
use crate::mechanics::{active_coils_for_rate, corrected_shear_stress};
use crate::numeric::{find_root_bracketed, SolveConfig};
use crate::units::{Force, Length, SpringRate};
use crate::CurvatureCorrection;
use crate::{Result, SpringError};
use std::f64::consts::PI;

/// Smallest spring index for which all three extension stresses (body shear, hook
/// bending σ_A, hook torsion τ_B) are monotone increasing in the mean diameter, so the
/// single-endpoint feasibility test in `best_mean_dia` is valid. It is the hook-torsion
/// factor turning point: `K_B(C2)·C2` is minimised where `4·C2² − 8·C2 + 1 = 0` with
/// `C2 = C/2`, giving `C = 2 + √3 ≈ 3.732`. Below this the factor is non-monotone (and has
/// a pole at `C = 2` for default hooks, where `4·C2 − 4 → 0`), so a request whose `c_min`
/// sits below it can drive the root finder onto the pole discontinuity and report a wildly
/// overstressed design as feasible. This floor is conservative for fixed hooks (whose
/// factors are monotone for all `C > 1`) but a single floor correct for every hook type is
/// the right input contract here. Cf. Shigley Eq. 10-37.
fn min_spring_index() -> f64 {
    2.0 + 3.0_f64.sqrt() // 2 + √3
}

/// How the hook geometry is determined during the search.
///
/// Non-exhaustive: future hook styles (e.g. V-hooks, extended hooks) may be added.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum HookSpec {
    /// Standard machine loops that scale with the mean diameter: r1 = D/2, r2 = D/4.
    Default,
    /// Fixed absolute bend radii, independent of D.
    Fixed { r1: Length, r2: Length },
}

impl HookSpec {
    /// Resolve the concrete hook radii for a given mean diameter.
    pub(crate) fn resolve(self, mean_dia: Length) -> HookEnds {
        match self {
            HookSpec::Default => HookEnds::default_for(mean_dia),
            HookSpec::Fixed { r1, r2 } => HookEnds { r1, r2 },
        }
    }
}

/// Which limit determines the chosen extension design.
///
/// Non-exhaustive: future binding limits (e.g. fatigue) may be added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExtBindingConstraint {
    /// Body coil shear stress τ reached its allowable (the largest D where the body
    /// shear is within `allowable_pct_torsion · MTS`).
    BodyShear,
    /// Hook bending stress σ_A reached its allowable (`allowable_pct_bending · MTS`).
    HookBending,
    /// End-hook torsion stress τ_B reached its allowable (`allowable_pct_end_torsion ·
    /// MTS`, the per-material end-hook limit; Shigley Table 10-7).
    HookTorsion,
    /// No stress bound first; the index ceiling `c_max` capped the mean diameter.
    Index,
    /// The outer-diameter cap `max_outer_dia` capped the mean diameter below every
    /// stress and index limit.
    OuterDiameter,
}

/// A minimum-weight extension-spring problem.
#[derive(Debug, Clone)]
pub struct ExtMinWeightRequest {
    /// Required spring rate k; fixes the active coils for each (d, D) via
    /// `Na = G·d⁴ / (8·D³·k)`. Must be finite and > 0.
    pub required_rate: SpringRate,
    /// Maximum operating force at which all three stresses are evaluated against their
    /// allowables. Must be finite and > 0.
    pub max_force: Force,
    /// Built-in preload. Passthrough: validated (>= 0, finite) and reported, but it
    /// does not affect the mass, the stresses, or the binding constraint.
    pub initial_tension: Force,
    /// How the hook geometry is chosen for each candidate (scaling or fixed radii).
    pub hooks: HookSpec,
    /// Allowed spring-index range `(c_min, c_max)`. `c_min` must be ≥ the monotonicity
    /// floor `2 + √3` (see `min_spring_index`) and strictly below `c_max`.
    pub index_bounds: (f64, f64),
    /// Optional cap on the outer diameter `D + d`; caps the mean diameter if a candidate
    /// would exceed it. Must be finite and > 0 when present.
    pub max_outer_dia: Option<Length>,
    /// Wire diameters to search; the lightest feasible one wins. Must be non-empty.
    pub candidate_diameters: Vec<Length>,
}

/// The chosen design and why it is limited.
#[derive(Debug, Clone)]
pub struct ExtMinWeightSolution {
    pub design: ExtensionDesign,
    pub binding: ExtBindingConstraint,
    pub mass_kg: f64,
}

/// Wire mass of a design: rho * (pi·d²/4) * L_wire, with developed wire length
/// L_wire = pi·D·Na (body) + 2·(pi·d_loop) (two hook loops), d_loop = 2·r1.
/// This is the Acxess Spring developed-length model `Li = pi·D·(N + 2)` (each
/// machine hook ≈ one mean coil) generalized so a fixed hook of radius r1
/// contributes a loop of its own mean diameter d_loop = 2·r1.
fn wire_mass(
    material: &Material,
    wire_dia: Length,
    mean_dia: Length,
    active: f64,
    hooks: HookEnds,
) -> f64 {
    let d = wire_dia.meters();
    let dm = mean_dia.meters();
    let d_loop = 2.0 * hooks.r1.meters();
    let l_wire = PI * dm * active + 2.0 * PI * d_loop;
    material.density.kg_per_m3() * (PI / 4.0) * d.powi(2) * l_wire
}

/// Largest feasible mean diameter for a wire size, and which limit binds.
///
/// Each of the three stresses (body shear, hook bending σ_A, hook torsion τ_B)
/// is monotone increasing in D over [c_min·d, c_max·d] when c_min is at or above
/// the per-factor turning point (compression documents the shear's U-shape with a
/// minimum at C* ≈ 1.866 for Wahl / ≈ 1.718 for Bergsträsser; the hook factors are
/// likewise monotone above their turning points. The tightest precondition is the
/// hook torsion, whose factor uses C2 = C/2, so its `K_B·C2` turns at C2 ≈ 1.866,
/// i.e. spring index C ≈ 3.73 — the default index floor (C ≥ 4) clears all three).
/// Under that assumption each stress imposes an upper bound on D via a single
/// bracketed root, and the feasible D is the minimum of the three bounds and the
/// index ceiling. The single-endpoint feasibility test (`stress(dm_lo) > allowable
/// → infeasible`) is valid only under this monotonicity.
fn best_mean_dia(
    material: &Material,
    d: Length,
    max_force: Force,
    bounds: (f64, f64),
    hooks: HookSpec,
    correction: CurvatureCorrection,
) -> Option<(Length, ExtBindingConstraint)> {
    let (c_min, c_max) = bounds;
    let mts = material.min_tensile_strength(d).ok()?.pascals();
    let allow_torsion = material.allowable_pct_torsion * mts;
    // End hooks in torsion (τ_B) use the per-material end-hook allowable (Shigley
    // Table 10-7: 40% for carbon/low-alloy steel, 30% for stainless/nonferrous).
    let allow_end_torsion = material.allowable_pct_end_torsion * mts;
    let allow_bending = material.allowable_pct_bending * mts;
    let dm_lo = c_min * d.meters();
    let dm_hi = c_max * d.meters();

    // Stress closures as functions of the mean diameter (in metres). Hooks are
    // resolved per D so default hooks scale (r1 = D/2, r2 = D/4).
    let body = |dm_m: f64| {
        let dm = Length::from_meters(dm_m);
        let c = dm_m / d.meters();
        corrected_shear_stress(max_force, dm, d, correction.factor(c)).pascals()
    };
    let bending = |dm_m: f64| {
        let dm = Length::from_meters(dm_m);
        hook_bending_stress(max_force, dm, d, hooks.resolve(dm).r1).pascals()
    };
    let torsion = |dm_m: f64| {
        let dm = Length::from_meters(dm_m);
        hook_torsion_stress(max_force, dm, d, hooks.resolve(dm).r2).pascals()
    };

    // Per-stress upper bound on D: None if it overstresses even at the smallest
    // index (candidate infeasible); (dm_hi, Index) if it never reaches allowable
    // (the index ceiling limits, not this stress); else the bracketed root where
    // stress == allowable, labeled with this stress.
    let bound_for = |stress: &dyn Fn(f64) -> f64,
                     allowable: f64,
                     label: ExtBindingConstraint|
     -> Option<(f64, ExtBindingConstraint)> {
        if stress(dm_lo) - allowable > 0.0 {
            return None;
        }
        if stress(dm_hi) - allowable <= 0.0 {
            // This stress never reaches allowable across the bracket, so it does not
            // bind — the index ceiling (dm_hi = c_max·d) is the limit, hence `Index`
            // rather than `label`.
            return Some((dm_hi, ExtBindingConstraint::Index));
        }
        let root = find_root_bracketed(
            |dm| stress(dm) - allowable,
            dm_lo,
            dm_hi,
            SolveConfig::default(),
        )
        .ok()?;
        Some((root, label))
    };

    let candidates = [
        bound_for(&body, allow_torsion, ExtBindingConstraint::BodyShear)?,
        bound_for(&bending, allow_bending, ExtBindingConstraint::HookBending)?,
        bound_for(
            &torsion,
            allow_end_torsion,
            ExtBindingConstraint::HookTorsion,
        )?,
    ];
    // The smallest upper bound binds. `total_cmp` is panic-free (the values are
    // finite here — dm_lo/dm_hi are finite and any root lies between them — so it
    // agrees with the usual ordering, but it avoids an `expect` on a NaN edge).
    let (dm, binding) = candidates.into_iter().min_by(|a, b| a.0.total_cmp(&b.0))?;
    Some((Length::from_meters(dm), binding))
}

/// Solve the minimum-weight extension-spring problem.
pub fn solve_min_weight(
    material: &Material,
    req: &ExtMinWeightRequest,
    correction: CurvatureCorrection,
) -> Result<ExtMinWeightSolution> {
    // Input validation (bad inputs → InconsistentInputs, NOT Infeasible — mirrors the
    // compression `min_weight_request_from_spec` contract). These reject malformed
    // requests up front rather than letting non-finite/degenerate values poison the
    // search (a zero rate diverges Na → ∞; a sub-floor c_min breaks the monotonicity
    // precondition `best_mean_dia` relies on, see [`min_spring_index`]).
    if !(req.required_rate.newtons_per_meter().is_finite()
        && req.required_rate.newtons_per_meter() > 0.0)
    {
        return Err(SpringError::InconsistentInputs(
            "required rate must be a positive finite number (N/m)".into(),
        ));
    }
    if !(req.max_force.newtons().is_finite() && req.max_force.newtons() > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "max force must be a positive finite number (N)".into(),
        ));
    }
    if req.candidate_diameters.is_empty() {
        return Err(SpringError::InconsistentInputs(
            "candidate_diameters must contain at least one diameter".into(),
        ));
    }
    let (c_min, c_max) = req.index_bounds;
    // 0 < c_min is intentionally NOT checked separately: the floor (c_min ≥ 2 + √3 ≈
    // 3.732) strictly implies it, so a redundant positivity guard would be an unkillable
    // equivalent mutant. The floor still rejects every c_min ≤ 0.
    if !(c_min.is_finite() && c_max.is_finite() && c_min < c_max && c_min >= min_spring_index()) {
        return Err(SpringError::InconsistentInputs(format!(
            "index bounds must satisfy {min:.4} ≤ c_min < c_max with both finite \
             (c_min floor = 2 + √3, the hook-torsion monotonicity turning point); \
             got c_min={c_min}, c_max={c_max}",
            min = min_spring_index()
        )));
    }
    if let Some(od_max) = req.max_outer_dia {
        if !(od_max.meters().is_finite() && od_max.meters() > 0.0) {
            return Err(SpringError::InconsistentInputs(
                "max_outer_dia must be a positive finite number".into(),
            ));
        }
    }

    let mut best: Option<ExtMinWeightSolution> = None;

    for &d in &req.candidate_diameters {
        let Some((mut mean, mut binding)) = best_mean_dia(
            material,
            d,
            req.max_force,
            req.index_bounds,
            req.hooks,
            correction,
        ) else {
            continue;
        };
        // Optional outer-diameter cap (mirrors compression).
        if let Some(od_max) = req.max_outer_dia {
            if mean.meters() + d.meters() > od_max.meters() {
                let capped = od_max.meters() - d.meters();
                if capped / d.meters() < c_min {
                    continue; // capping would push the index below the floor
                }
                mean = Length::from_meters(capped);
                binding = ExtBindingConstraint::OuterDiameter;
            }
        }
        let active = active_coils_for_rate(material.shear_modulus, d, mean, req.required_rate);
        if !active.is_finite() || active < 1.0 {
            continue; // non-finite or fewer than one active coil is unphysical
        }
        let hooks = req.hooks.resolve(mean);
        let free_length = free_length_from_geometry(d, active, hooks);
        let Ok(design) = solve_forward(
            material,
            d,
            mean,
            active,
            free_length,
            req.initial_tension,
            hooks,
            &[req.max_force],
            correction,
        ) else {
            continue; // candidate produces an invalid forward design (e.g. C2 ≤ 1 for
                      // fixed hooks, or free_length ≤ 0); skip it like every other guard
                      // rather than aborting the whole solve and discarding lighter finds
        };
        let mass = wire_mass(material, d, mean, active, hooks);
        if best.as_ref().map(|b| mass < b.mass_kg).unwrap_or(true) {
            best = Some(ExtMinWeightSolution {
                design,
                binding,
                mass_kg: mass,
            });
        }
    }

    best.ok_or_else(|| {
        SpringError::Infeasible("no candidate diameter satisfies the constraints".into())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{Force, Length, SpringRate};
    use approx::assert_relative_eq;

    /// Default-hook request over a set of candidate wire diameters (mm).
    fn base_request(candidates: Vec<f64>) -> ExtMinWeightRequest {
        ExtMinWeightRequest {
            required_rate: SpringRate::from_newtons_per_meter(2000.0),
            max_force: Force::from_newtons(50.0),
            initial_tension: Force::from_newtons(10.0),
            hooks: HookSpec::Default,
            index_bounds: (4.0, 12.0),
            max_outer_dia: None,
            candidate_diameters: candidates
                .into_iter()
                .map(Length::from_millimeters)
                .collect(),
        }
    }

    /// Low-force request (F = 5 N) where the index ceiling binds for every candidate.
    fn low_force_request(candidates: Vec<f64>) -> ExtMinWeightRequest {
        let mut req = base_request(candidates);
        req.max_force = Force::from_newtons(5.0);
        req
    }

    #[test]
    fn solution_is_feasible() {
        let m = crate::test_support::music_wire();
        let sol = solve_min_weight(
            &m,
            &base_request(vec![1.5, 2.0, 2.5, 3.0]),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        // Rate met.
        assert_relative_eq!(
            sol.design.rate.newtons_per_meter(),
            2000.0,
            max_relative = 1e-6
        );
        // All three stresses within allowable at the operating load.
        let lp = &sol.design.load_points[0];
        assert!(lp.pct_body_allow <= 1.0 + 1e-6);
        assert!(lp.pct_hook_bending_allow <= 1.0 + 1e-6);
        assert!(lp.pct_hook_torsion_allow <= 1.0 + 1e-6);
        // Index within bounds; positive mass.
        assert!(sol.design.index >= 4.0 - 1e-9 && sol.design.index <= 12.0 + 1e-9);
        assert!(sol.mass_kg > 0.0);
    }

    #[test]
    fn picks_global_minimum_over_candidates() {
        let m = crate::test_support::music_wire();
        let candidates = vec![1.5, 2.0, 2.5, 3.0];
        let per: Vec<f64> = candidates
            .iter()
            .filter_map(|&d| {
                solve_min_weight(
                    &m,
                    &base_request(vec![d]),
                    CurvatureCorrection::Bergstrasser,
                )
                .ok()
                .map(|s| s.mass_kg)
            })
            .collect();
        let best = solve_min_weight(
            &m,
            &base_request(candidates),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        let min = per.iter().cloned().fold(f64::INFINITY, f64::min);
        assert_relative_eq!(best.mass_kg, min, max_relative = 1e-9);
    }

    #[test]
    fn infeasible_when_outer_diameter_too_small() {
        let m = crate::test_support::music_wire();
        let mut req = base_request(vec![1.5, 2.0, 2.5]);
        req.max_outer_dia = Some(Length::from_millimeters(3.0)); // forces index < 4
        assert!(matches!(
            solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser),
            Err(SpringError::Infeasible(_))
        ));
    }

    // ── Group 1: input validation + index floor ───────────────────────────────

    /// At the exact floor c_min = 2 + √3, the request is accepted (the rest of the
    /// request is feasible), pinning the `>=` boundary against a `>` mutant.
    #[test]
    fn index_floor_exactly_at_min_is_accepted() {
        let m = crate::test_support::music_wire();
        let mut req = base_request(vec![1.5, 2.0, 2.5, 3.0]);
        req.index_bounds = (2.0 + 3.0_f64.sqrt(), 12.0);
        assert!(
            solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser).is_ok(),
            "c_min exactly at the floor (2 + √3) must be accepted"
        );
    }

    /// Just below the floor is rejected. `floor - 1e-9` (≈3.7320508) lands in the
    /// `[2·√3, 2+√3)` gap so it also kills the `+`→`*` mutant on `2.0 + √3`
    /// (`2·√3 ≈ 3.464`, which would accept it).
    #[test]
    fn index_below_floor_is_rejected() {
        let m = crate::test_support::music_wire();
        let mut req = base_request(vec![2.0]);
        req.index_bounds = ((2.0 + 3.0_f64.sqrt()) - 1e-9, 12.0);
        assert!(matches!(
            solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser),
            Err(SpringError::InconsistentInputs(_))
        ));
    }

    /// CRITICAL regression: default hooks with c_min = 1.5 (below the C = 2 pole of
    /// the hook-torsion factor) used to converge onto the pole and return a design
    /// overstressed by ~10¹¹× labelled feasible. It must now be rejected as bad input.
    #[test]
    fn low_index_default_hooks_rejected_not_overstressed() {
        let m = crate::test_support::music_wire();
        let req = ExtMinWeightRequest {
            required_rate: SpringRate::from_newtons_per_meter(2000.0),
            max_force: Force::from_newtons(300.0),
            initial_tension: Force::from_newtons(0.0),
            hooks: HookSpec::Default,
            index_bounds: (1.5, 12.0),
            max_outer_dia: None,
            candidate_diameters: vec![Length::from_millimeters(2.0)],
        };
        assert!(
            matches!(
                solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser),
                Err(SpringError::InconsistentInputs(_))
            ),
            "sub-pole c_min must be rejected as bad input, never returned as a feasible design"
        );
    }

    #[test]
    fn inverted_index_bounds_rejected() {
        let m = crate::test_support::music_wire();
        let mut req = base_request(vec![2.0]);
        req.index_bounds = (12.0, 4.0);
        assert!(matches!(
            solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser),
            Err(SpringError::InconsistentInputs(_))
        ));
    }

    /// c_min == c_max is degenerate; pins the strict `<` against a `<=` mutant.
    #[test]
    fn equal_index_bounds_rejected() {
        let m = crate::test_support::music_wire();
        let mut req = base_request(vec![2.0]);
        req.index_bounds = (5.0, 5.0);
        assert!(matches!(
            solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser),
            Err(SpringError::InconsistentInputs(_))
        ));
    }

    /// A +Inf bound is the discriminator for the finiteness conjunct: ordering
    /// (`c_min < +Inf`) and the floor both pass, so only the finiteness check stands
    /// between it and acceptance (NaN would be caught downstream by the ordering test).
    #[test]
    fn non_finite_index_bound_rejected() {
        let m = crate::test_support::music_wire();
        let mut req = base_request(vec![2.0]);
        req.index_bounds = (4.0, f64::INFINITY);
        assert!(matches!(
            solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser),
            Err(SpringError::InconsistentInputs(_))
        ));
    }

    #[test]
    fn empty_candidates_rejected() {
        let m = crate::test_support::music_wire();
        let req = base_request(vec![]);
        assert!(matches!(
            solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser),
            Err(SpringError::InconsistentInputs(_))
        ));
    }

    #[test]
    fn non_positive_max_force_rejected() {
        let m = crate::test_support::music_wire();
        let mut req = base_request(vec![2.0]);
        req.max_force = Force::from_newtons(0.0);
        assert!(matches!(
            solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser),
            Err(SpringError::InconsistentInputs(_))
        ));
    }

    #[test]
    fn non_positive_required_rate_rejected() {
        let m = crate::test_support::music_wire();
        let mut req = base_request(vec![2.0]);
        req.required_rate = SpringRate::from_newtons_per_meter(0.0);
        assert!(matches!(
            solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser),
            Err(SpringError::InconsistentInputs(_))
        ));
    }

    #[test]
    fn non_positive_max_outer_dia_rejected() {
        let m = crate::test_support::music_wire();
        let mut req = base_request(vec![2.0]);
        req.max_outer_dia = Some(Length::from_millimeters(0.0));
        assert!(matches!(
            solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser),
            Err(SpringError::InconsistentInputs(_))
        ));
    }

    // ── Group 5: public-contract feasibility sweep ─────────────────────────────

    /// Defense in depth: across a grid of VALID requests, every design the optimizer
    /// returns as `Ok` must be feasible — all three %-allowable ratios within 1.0 at
    /// the operating load. This is the property the pole bug violated; a test (not a
    /// runtime re-check, which Group 1 makes unreachable dead code) keeps it
    /// mutation-clean while asserting the contract directly.
    #[test]
    fn returned_designs_are_always_feasible() {
        let m = crate::test_support::music_wire();
        let diameters = [2.0_f64, 3.0, 4.0];
        let c_mins = [4.0_f64, 6.0, 8.0];
        let forces = [20.0_f64, 100.0];
        let hook_specs = [
            HookSpec::Default,
            HookSpec::Fixed {
                r1: Length::from_millimeters(20.0),
                r2: Length::from_millimeters(8.0),
            },
            HookSpec::Fixed {
                r1: Length::from_millimeters(30.0),
                r2: Length::from_millimeters(5.0),
            },
        ];
        let corrections = [CurvatureCorrection::Wahl, CurvatureCorrection::Bergstrasser];

        let mut feasible_count = 0;
        for &d in &diameters {
            for &c_min in &c_mins {
                for &f in &forces {
                    for &hooks in &hook_specs {
                        for &corr in &corrections {
                            let mut req = base_request(vec![d]);
                            req.index_bounds = (c_min, 12.0);
                            req.max_force = Force::from_newtons(f);
                            req.hooks = hooks;
                            let Ok(sol) = solve_min_weight(&m, &req, corr) else {
                                continue; // infeasible combos are allowed; only Ok must be feasible
                            };
                            let lp = &sol.design.load_points[0];
                            assert!(
                                lp.pct_body_allow <= 1.0 + 1e-6
                                    && lp.pct_hook_bending_allow <= 1.0 + 1e-6
                                    && lp.pct_hook_torsion_allow <= 1.0 + 1e-6,
                                "infeasible design returned for d={d} c_min={c_min} F={f} \
                                 hooks={hooks:?} corr={corr:?}: body={}, bending={}, torsion={}",
                                lp.pct_body_allow,
                                lp.pct_hook_bending_allow,
                                lp.pct_hook_torsion_allow,
                            );
                            feasible_count += 1;
                        }
                    }
                }
            }
        }
        // Guard against the grid degenerating to all-infeasible (which would make the
        // assertion vacuous).
        assert!(
            feasible_count > 0,
            "expected at least one feasible design across the sweep"
        );
    }

    // ── per-binding discrimination ────────────────────────────────────────────

    #[test]
    fn low_force_binds_index() {
        let m = crate::test_support::music_wire();
        let req = low_force_request(vec![3.0]); // F far below any stress limit at c_max
        let sol = solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser).unwrap();
        assert_eq!(sol.binding, ExtBindingConstraint::Index);
        // Index ceiling ⇒ mean diameter = c_max · d = 12 · 3 mm = 36 mm.
        assert_relative_eq!(sol.design.mean_dia.millimeters(), 36.0, max_relative = 1e-9);
        let lp = &sol.design.load_points[0];
        assert!(lp.pct_body_allow <= 1.0 + 1e-6);
        assert!(lp.pct_hook_bending_allow <= 1.0 + 1e-6);
        assert!(lp.pct_hook_torsion_allow <= 1.0 + 1e-6);
    }

    /// Group 2: a candidate whose forward design is invalid (fixed hooks giving
    /// C2 = 2·r2/d ≤ 1) must be skipped, not abort the whole solve. d=6 mm yields
    /// C2 = 2·2/6 = 0.667 ≤ 1 (solve_forward Errs) but best_mean_dia passes it
    /// (negative K_B ⇒ torsion never binds); d=2 mm is valid and lighter, and must win.
    #[test]
    fn fixed_hooks_bad_candidate_is_skipped_not_aborted() {
        let m = crate::test_support::music_wire();
        let mut req = base_request(vec![2.0, 6.0]); // 2 mm valid+lighter; 6 mm ⇒ C2 ≤ 1
        req.hooks = HookSpec::Fixed {
            r1: Length::from_millimeters(30.0), // large ⇒ C1 ≫ 1, valid bending factor
            r2: Length::from_millimeters(2.0),
        };
        let sol = solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser)
            .expect("the valid 2 mm candidate must be returned, not aborted by the 6 mm one");
        assert_relative_eq!(sol.design.wire_dia.millimeters(), 2.0, max_relative = 1e-9);
    }

    // ── exact arithmetic pins (mass & free_length) ────────────────────────────
    //
    // d=3 mm, D=36 mm (c_max=12, Index binding), k=2000 N/m, default hooks (r1=D/2).
    // Python recomputation from cited formulas:
    //   G=80e9;  rho=7850;  d=3e-3;  D=36e-3;  k=2000;  r1=D/2;  d_loop=2*r1=D
    //   Na     = G*d^4/(8*D^3*k)                = 8.680555…
    //   L0     = 2*(d_loop − d) + (Na+1)*d      = 95.041666… mm
    //   L_wire = π·D·Na + 2·π·d_loop            = 1.207942375… m
    //   mass   = ρ·(π/4)·d²·L_wire              = 6.702676583381559e-02 kg
    #[test]
    fn index_binding_mass_and_free_length_exact() {
        let m = crate::test_support::music_wire();
        let req = low_force_request(vec![3.0]);
        let sol = solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser).unwrap();
        // Kills: arithmetic mutations in wire_mass (factor swaps, powi, PI) and
        //        in free_length_from_geometry (coefficient swaps, wrong d_loop).
        assert_relative_eq!(sol.mass_kg, 6.702_676_583_381_559e-2, max_relative = 1e-6);
        assert_relative_eq!(
            sol.design.free_length.millimeters(),
            95.041_666_666_666_66,
            max_relative = 1e-6
        );
    }

    #[test]
    fn hook_bending_binds() {
        let m = crate::test_support::music_wire();
        // Default hooks; F=200 N pushes σ_A past its allowable before the index ceiling.
        // At c_max=12: σ_A ≈ 1475 MPa > allow_bending ≈ 1412 MPa → root-find.
        let mut req = base_request(vec![3.0]);
        req.max_force = Force::from_newtons(200.0);
        let sol = solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser).unwrap();
        assert_eq!(sol.binding, ExtBindingConstraint::HookBending);
        let lp = &sol.design.load_points[0];
        // Binding stress is ≈ 1.0 at the root; remaining stresses within allowable.
        assert!(lp.pct_hook_bending_allow > 0.99 && lp.pct_hook_bending_allow <= 1.0 + 1e-6);
        assert!(lp.pct_body_allow <= 1.0 + 1e-6);
        assert!(lp.pct_hook_torsion_allow <= 1.0 + 1e-6);
    }

    #[test]
    fn hook_torsion_binds() {
        let m = crate::test_support::music_wire();
        // Large r1 drops σ_A; tiny r2 gives C2=4/3, K_B≈3.25 → τ_B binds.
        // d=3 mm: C2 = 2·r2/d = 2·2/3 = 1.33 > 1 (factor valid).
        let mut req = base_request(vec![3.0]);
        req.max_force = Force::from_newtons(80.0);
        req.hooks = HookSpec::Fixed {
            r1: Length::from_millimeters(30.0),
            r2: Length::from_millimeters(2.0),
        };
        let sol = solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser).unwrap();
        assert_eq!(sol.binding, ExtBindingConstraint::HookTorsion);
        let lp = &sol.design.load_points[0];
        assert!(lp.pct_hook_torsion_allow > 0.99 && lp.pct_hook_torsion_allow <= 1.0 + 1e-6);
        assert!(lp.pct_body_allow <= 1.0 + 1e-6);
        assert!(lp.pct_hook_bending_allow <= 1.0 + 1e-6);
    }

    #[test]
    fn body_shear_binds() {
        let m = crate::test_support::music_wire();
        // Very large fixed hooks (r1=r2=100 mm) push both hook K-factors toward 1,
        // leaving the body shear with the highest normalised stress → it binds first.
        let mut req = base_request(vec![3.0]);
        req.max_force = Force::from_newtons(500.0);
        req.hooks = HookSpec::Fixed {
            r1: Length::from_millimeters(100.0),
            r2: Length::from_millimeters(100.0),
        };
        let sol = solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser).unwrap();
        assert_eq!(sol.binding, ExtBindingConstraint::BodyShear);
        let lp = &sol.design.load_points[0];
        assert!(lp.pct_body_allow > 0.99 && lp.pct_body_allow <= 1.0 + 1e-6);
        assert!(lp.pct_hook_bending_allow <= 1.0 + 1e-6);
        assert!(lp.pct_hook_torsion_allow <= 1.0 + 1e-6);
    }

    #[test]
    fn od_cap_binds() {
        let m = crate::test_support::music_wire();
        // Without cap: d=3 mm, F=5 N → Index binding, D=36 mm, OD=39 mm.
        // od_max=35 mm < 39 mm → cap fires → D=32 mm, binding=OuterDiameter.
        let mut req = low_force_request(vec![3.0]);
        req.max_outer_dia = Some(Length::from_millimeters(35.0));
        let sol = solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser).unwrap();
        assert_eq!(sol.binding, ExtBindingConstraint::OuterDiameter);
        // Capped mean = od_max − d = 35 − 3 = 32 mm exactly.
        assert_relative_eq!(sol.design.mean_dia.millimeters(), 32.0, max_relative = 1e-9);
        let lp = &sol.design.load_points[0];
        assert!(lp.pct_body_allow <= 1.0 + 1e-6);
        assert!(lp.pct_hook_bending_allow <= 1.0 + 1e-6);
        assert!(lp.pct_hook_torsion_allow <= 1.0 + 1e-6);
    }

    // ── F_i passthrough ───────────────────────────────────────────────────────

    #[test]
    fn initial_tension_is_passthrough_only() {
        let m = crate::test_support::music_wire();
        let mut lo = base_request(vec![2.0, 2.5, 3.0]);
        lo.initial_tension = Force::from_newtons(0.0);
        let mut hi = lo.clone();
        hi.initial_tension = Force::from_newtons(20.0);
        let a = solve_min_weight(&m, &lo, CurvatureCorrection::Bergstrasser).unwrap();
        let b = solve_min_weight(&m, &hi, CurvatureCorrection::Bergstrasser).unwrap();
        // Mass, binding, and geometry are identical regardless of F_i.
        assert_relative_eq!(a.mass_kg, b.mass_kg, max_relative = 1e-12);
        assert_eq!(a.binding, b.binding);
        assert_relative_eq!(
            a.design.wire_dia.millimeters(),
            b.design.wire_dia.millimeters(),
            max_relative = 1e-12
        );
        assert_relative_eq!(
            a.design.mean_dia.millimeters(),
            b.design.mean_dia.millimeters(),
            max_relative = 1e-12
        );
        // But the reported preload differs (proving it flows into the final design).
        assert_relative_eq!(a.design.initial_tension.newtons(), 0.0, epsilon = 1e-15);
        assert_relative_eq!(
            b.design.initial_tension.newtons(),
            20.0,
            max_relative = 1e-12
        );
    }

    // ── hook-spec coverage ────────────────────────────────────────────────────

    #[test]
    fn fixed_hooks_reproduce_default_at_same_radii() {
        let m = crate::test_support::music_wire();
        // Single candidate so both runs pick the same D; low force keeps Index binding.
        let def = low_force_request(vec![3.0]); // Index binding ⇒ D=36 mm
        let d_sol = solve_min_weight(&m, &def, CurvatureCorrection::Bergstrasser).unwrap();
        // At D=36 mm the default hook resolves to r1=18 mm, r2=9 mm.
        let mut fixed = def.clone();
        fixed.hooks = HookSpec::Fixed {
            r1: Length::from_millimeters(18.0),
            r2: Length::from_millimeters(9.0),
        };
        let f_sol = solve_min_weight(&m, &fixed, CurvatureCorrection::Bergstrasser).unwrap();
        assert_relative_eq!(d_sol.mass_kg, f_sol.mass_kg, max_relative = 1e-9);
        assert_relative_eq!(
            d_sol.design.free_length.millimeters(),
            f_sol.design.free_length.millimeters(),
            max_relative = 1e-9
        );
    }

    #[test]
    fn larger_fixed_loop_increases_mass_and_free_length() {
        let m = crate::test_support::music_wire();
        // Low force ⇒ Index binding in both cases ⇒ same D=36 mm.
        // Larger r1 means a bigger loop diameter (d_loop=2·r1), more wire, longer spring.
        let mut small = low_force_request(vec![3.0]);
        small.hooks = HookSpec::Fixed {
            r1: Length::from_millimeters(18.0),
            r2: Length::from_millimeters(9.0),
        };
        let mut large = small.clone();
        large.hooks = HookSpec::Fixed {
            r1: Length::from_millimeters(30.0), // bigger loop ⇒ more wire, longer L0
            r2: Length::from_millimeters(9.0),
        };
        let s = solve_min_weight(&m, &small, CurvatureCorrection::Bergstrasser).unwrap();
        let l = solve_min_weight(&m, &large, CurvatureCorrection::Bergstrasser).unwrap();
        assert!(l.mass_kg > s.mass_kg);
        assert!(l.design.free_length.millimeters() > s.design.free_length.millimeters());
    }

    // ── OD cap boundary: strict > (line 194) ─────────────────────────────────
    //
    // `if mean.meters() + d.meters() > od_max.meters()`
    // Mutant ">" → ">=" also caps when OD == od_max, wrongly switching binding from
    // Index to OuterDiameter even though no constraint is violated.
    //
    // d=3 mm, c_max=8, index binding ⇒ D=24 mm, OD=27 mm.
    // IEEE 754: 24e-3 + 3e-3 == 27e-3 exactly (verified in Python).
    // rate=58593.75 N/m: Na = G*(3e-3)^4/(8*(24e-3)^3*58593.75) = 1.0 exactly,
    // so the active-coils guard also passes at its exact boundary.
    // Original (>): 0.027 > 0.027 = false → no cap → Index binding.
    // Mutant (>=): 0.027 >= 0.027 = true → cap fires → OuterDiameter binding.
    #[test]
    fn od_cap_does_not_fire_when_od_equals_od_max_exactly() {
        let m = crate::test_support::music_wire();
        let req = ExtMinWeightRequest {
            required_rate: SpringRate::from_newtons_per_meter(58_593.75),
            max_force: Force::from_newtons(5.0),
            initial_tension: Force::from_newtons(0.0),
            hooks: HookSpec::Default,
            index_bounds: (4.0, 8.0),
            max_outer_dia: Some(Length::from_millimeters(27.0)), // OD == od_max at D=24mm
            candidate_diameters: vec![Length::from_millimeters(3.0)],
        };
        let sol = solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser).unwrap();
        assert_eq!(
            sol.binding,
            ExtBindingConstraint::Index,
            "OD == od_max must not fire the cap (strict >); binding must stay Index"
        );
    }

    // ── c_min floor after OD cap: strict < (line 196) ────────────────────────
    //
    // `if capped / d.meters() < c_min { continue; }`
    // Mutant "<" → "<=" rejects when capped/d == c_min exactly.
    //
    // d=3 mm, c_min=4, od_max=15 mm: capped = 15e-3 − 3e-3 = 12e-3,
    // c_cap = 12e-3/3e-3 = 4.0 == c_min exactly (IEEE 754 verified).
    // Original (<): 4.0 < 4.0 = false → candidate survives → OuterDiameter at D=12mm.
    // Mutant (<=): 4.0 <= 4.0 = true → candidate skipped → Infeasible.
    #[test]
    fn od_cap_at_c_min_boundary_is_not_rejected() {
        let m = crate::test_support::music_wire();
        let mut req = low_force_request(vec![3.0]);
        // od_max = (c_min+1)·d = 5·3 = 15 mm → capped D = 12 mm → c_cap = 4 = c_min.
        req.max_outer_dia = Some(Length::from_millimeters(15.0));
        let sol = solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser).unwrap();
        assert_eq!(
            sol.binding,
            ExtBindingConstraint::OuterDiameter,
            "c_cap == c_min must not be rejected (strict <); should yield OuterDiameter"
        );
        // Capped mean = od_max − d = 15 − 3 = 12 mm exactly.
        assert_relative_eq!(sol.design.mean_dia.millimeters(), 12.0, max_relative = 1e-9);
    }

    // ── active-coils guard: || vs && and < boundary (line 204) ───────────────
    //
    // `if !active.is_finite() || active < 1.0 { continue; }`
    //
    // Mutant "||" → "&&" skips only when BOTH conditions hold (never, since a
    // finite value makes !is_finite() false). So active=0.868 (finite, <1) would
    // NOT be skipped — solve_forward would run and return a design.
    // Mutant "< → ==" only skips active==1.0 exactly, missing active=0.868.
    // Both mutants cause Ok where the original gives Infeasible.
    //
    // d=3 mm, c_max=12, k=20000 N/m: Na=G*d^4/(8*D^3*k)=0.868 < 1.0 → skip.
    #[test]
    fn active_coils_below_one_rejects_candidate() {
        let m = crate::test_support::music_wire();
        let req = ExtMinWeightRequest {
            required_rate: SpringRate::from_newtons_per_meter(20_000.0),
            max_force: Force::from_newtons(5.0),
            initial_tension: Force::from_newtons(0.0),
            hooks: HookSpec::Default,
            index_bounds: (4.0, 12.0),
            max_outer_dia: None,
            candidate_diameters: vec![Length::from_millimeters(3.0)],
        };
        assert!(
            matches!(
                solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser),
                Err(SpringError::Infeasible(_))
            ),
            "Na=0.868 < 1.0 must be skipped; no feasible candidate → Infeasible"
        );
    }

    // Mutant "< → <=" would also reject active==1.0, turning a valid design infeasible.
    //
    // d=3 mm, c_max=8, k=58593.75 N/m: Na=G*d^4/(8*(24e-3)^3*58593.75)=1.0 exactly.
    // Original (<): 1.0 < 1.0 = false → candidate survives.
    // Mutant (<=): 1.0 <= 1.0 = true → skip → Infeasible.
    #[test]
    fn active_coils_exactly_one_is_not_rejected() {
        let m = crate::test_support::music_wire();
        let req = ExtMinWeightRequest {
            required_rate: SpringRate::from_newtons_per_meter(58_593.75),
            max_force: Force::from_newtons(5.0),
            initial_tension: Force::from_newtons(0.0),
            hooks: HookSpec::Default,
            index_bounds: (4.0, 8.0),
            max_outer_dia: None,
            candidate_diameters: vec![Length::from_millimeters(3.0)],
        };
        assert!(
            solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser).is_ok(),
            "Na=1.0 exactly must not be skipped (strict <); design must succeed"
        );
    }

    // ── mass comparison: strict < (line 221) ─────────────────────────────────
    //
    // `if best.as_ref().map(|b| mass < b.mass_kg).unwrap_or(true)`
    // Mutant "< → ==" updates only when mass == previous best (never for distinct
    // masses), so the first candidate always wins regardless of weight.
    //
    // Candidates ordered heavier-first ([3.0, 2.0] mm); d=2mm is lighter.
    // Original: 2mm mass < 3mm mass → update → 2mm wins.
    // Mutant: 2mm mass != 3mm mass → no update → 3mm wins (wrong).
    //
    // mass(d=2mm, D=24mm) ≈ 1.448e-2 kg << mass(d=3mm, D=36mm) ≈ 6.703e-2 kg.
    #[test]
    fn mass_comparison_selects_lighter_design() {
        let m = crate::test_support::music_wire();
        // d=3 mm first (heavier), d=2 mm second (lighter) → 2 mm must win.
        let req = low_force_request(vec![3.0, 2.0]);
        let sol = solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser).unwrap();
        assert_relative_eq!(sol.design.wire_dia.millimeters(), 2.0, max_relative = 1e-9);
        // Mass must match the single-candidate 2mm solution exactly.
        let mut req2 = req.clone();
        req2.candidate_diameters = vec![Length::from_millimeters(2.0)];
        let sol2 = solve_min_weight(&m, &req2, CurvatureCorrection::Bergstrasser).unwrap();
        assert_relative_eq!(sol.mass_kg, sol2.mass_kg, max_relative = 1e-12);
    }
}
