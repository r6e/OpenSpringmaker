//! Minimum-weight constrained optimization for compression springs.
//! Figure of merit = wire mass (Shigley §10-11). The optimum mean diameter for a
//! given wire size lies on the binding stress or index constraint.

use crate::design::{solve_forward, SpringDesign};
use crate::end_type::EndType;
use crate::material::Material;
use crate::mechanics::{
    active_coils_for_rate, corrected_shear_stress, is_buckling_stable, EndFixity,
};
use crate::numeric::{find_root_bracketed, SolveConfig};
use crate::units::{Force, Length, SpringRate};
use crate::CurvatureCorrection;
use crate::{Result, SpringError};
use std::f64::consts::PI;

/// Smallest spring index for which the Wahl/Bergsträsser-corrected shear stress is
/// monotone increasing in the mean diameter, so the single-endpoint feasibility test in
/// [`best_mean_dia`] is valid. The corrected stress `τ(C) = Kw(C)·C` is U-shaped: solving
/// `d/dC[Kw·C] = 0` gives `4C² − 8C + 1 = 0`, whose relevant root is `C* = 1 + √3/2 ≈
/// 1.866` (the minimum). Below `C*` the stress decreases with `C`, so a request whose
/// `c_min` sits there violates the monotonicity precondition and can make `best_mean_dia`
/// accept an overstressed design. Bergsträsser's `K_B·C` turns slightly lower (≈ 1.718),
/// so this floor is conservative for both correction factors. Cf. Shigley Eq. 10-5/10-6.
pub(crate) fn min_spring_index() -> f64 {
    1.0 + 3.0_f64.sqrt() / 2.0 // 1 + √3/2 ≈ 1.866
}

/// Which constraint limits the chosen design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingConstraint {
    Stress,
    Index,
    OuterDiameter,
}

/// A minimum-weight design problem.
#[derive(Debug, Clone)]
pub struct MinWeightRequest {
    pub end_type: EndType,
    pub fixity: EndFixity,
    pub required_rate: SpringRate,
    pub max_force: Force,
    pub index_bounds: (f64, f64),
    pub max_outer_dia: Option<Length>,
    pub candidate_diameters: Vec<Length>,
    /// Fractional clearance kept before solid at max force (SMI ~0.10–0.15).
    pub clash_allowance: f64,
    /// Override for the inactive-coil count; `None` resolves to the end type's
    /// Shigley Table 10-1 default via [`EndType::resolve_inactive`].
    pub inactive_coils: Option<f64>,
}

/// The chosen design and why it is limited.
#[derive(Debug, Clone)]
pub struct MinWeightSolution {
    pub design: SpringDesign,
    pub binding: BindingConstraint,
    pub mass_kg: f64,
}

/// Wire mass of a design: rho * (pi^2/4) * d^2 * D * Nt (wire length ~ pi*D*Nt).
fn wire_mass(material: &Material, wire_dia: Length, mean_dia: Length, total_coils: f64) -> f64 {
    let d = wire_dia.meters();
    let dm = mean_dia.meters();
    material.density.kg_per_m3() * (PI * PI / 4.0) * d.powi(2) * dm * total_coils
}

/// Largest feasible mean diameter for a wire size, and which limit binds.
fn best_mean_dia(
    material: &Material,
    d: Length,
    max_force: Force,
    bounds: (f64, f64),
    correction: CurvatureCorrection,
) -> Option<(Length, BindingConstraint)> {
    let (c_min, c_max) = bounds;
    let allowable =
        material.allowable_pct_torsion * material.min_tensile_strength(d).ok()?.pascals();
    // τ(C) = Kw(C)·C with Kw = (4C−1)/(4C−4) + 0.615/C is NOT globally monotonic.
    // Solving dτ/dC = 0 gives 4C²−8C+1 = 0 → C* = 1 + √3/2 ≈ 1.866; τ is
    // U-shaped with a minimum at C* and is monotonic increasing only for C ≥ 1.866.
    // Bergsträsser's K_B·C turns at C ≈ 1.718 (below the enforced index floor),
    // so the monotonicity assumption holds for both factors — the floor is
    // correction-agnostic and conservative.
    // The single-endpoint feasibility test below (`stress_at(dm_lo) > allowable → None`)
    // is therefore only valid when c_min ≥ 1.866, which Fix 2 (min_weight_request_from_spec)
    // enforces at the entry point.
    let stress_at = |dm_m: f64| {
        let dm = Length::from_meters(dm_m);
        let c = dm_m / d.meters();
        corrected_shear_stress(max_force, dm, d, correction.factor(c)).pascals()
    };
    let dm_lo = c_min * d.meters();
    let dm_hi = c_max * d.meters();
    // If even the smallest index overstresses, this wire size is infeasible.
    if stress_at(dm_lo) - allowable > 0.0 {
        return None;
    }
    // If the largest index is still under allowable, the index ceiling binds.
    if stress_at(dm_hi) - allowable <= 0.0 {
        return Some((Length::from_meters(dm_hi), BindingConstraint::Index));
    }
    // Otherwise the stress limit binds; solve for the mean diameter at allowable.
    let root = find_root_bracketed(
        |dm| stress_at(dm) - allowable,
        dm_lo,
        dm_hi,
        SolveConfig::default(),
    )
    .ok()?;
    Some((Length::from_meters(root), BindingConstraint::Stress))
}

/// Solve the minimum-weight problem.
pub fn solve_min_weight(
    material: &Material,
    req: &MinWeightRequest,
    correction: CurvatureCorrection,
) -> Result<MinWeightSolution> {
    // Input validation (bad inputs → InconsistentInputs, NOT Infeasible). `solve_min_weight`
    // is a public entry point: the scenario path validates raw TOML in
    // `min_weight_request_from_spec`, but a direct caller bypasses that, so the SI request
    // is validated here too (defense in depth, mirroring the extension optimizer). These
    // reject malformed requests up front rather than letting non-finite/degenerate values
    // poison the search (a zero rate diverges Na → ∞; a sub-floor c_min breaks the
    // monotonicity precondition `best_mean_dia` relies on, see [`min_spring_index`]).
    let rate = req.required_rate.newtons_per_meter();
    if !(rate.is_finite() && rate > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "required rate must be a positive finite number (N/m)".into(),
        ));
    }
    let force = req.max_force.newtons();
    if !(force.is_finite() && force > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "max force must be a positive finite number (N)".into(),
        ));
    }
    if !(req.clash_allowance.is_finite() && req.clash_allowance >= 0.0) {
        return Err(SpringError::InconsistentInputs(
            "clash allowance must be a finite number ≥ 0".into(),
        ));
    }
    if req.candidate_diameters.is_empty() {
        return Err(SpringError::InconsistentInputs(
            "candidate_diameters must contain at least one diameter".into(),
        ));
    }
    // Reject non-finite/zero/negative diameters explicitly rather than silently skipping
    // them downstream, so a malformed candidate list is InconsistentInputs.
    if req.candidate_diameters.iter().any(|d| {
        let m = d.meters();
        !(m.is_finite() && m > 0.0)
    }) {
        return Err(SpringError::InconsistentInputs(
            "candidate diameters must be finite and positive".into(),
        ));
    }
    let (c_min, c_max) = req.index_bounds;
    let c_floor = min_spring_index();
    // 0 < c_min is intentionally NOT checked separately: the floor (c_min ≥ 1 + √3/2 ≈
    // 1.866) strictly implies it, so a redundant positivity guard would be an unkillable
    // equivalent mutant. The floor still rejects every c_min ≤ 0.
    if !(c_min.is_finite() && c_max.is_finite() && c_min < c_max && c_min >= c_floor) {
        return Err(SpringError::InconsistentInputs(format!(
            "index bounds must satisfy {c_floor:.4} ≤ c_min < c_max with both finite \
             (c_min floor = 1 + √3/2, the Wahl/Bergsträsser monotonicity turning point); \
             got c_min={c_min}, c_max={c_max}"
        )));
    }
    if let Some(od_max) = req.max_outer_dia {
        let od = od_max.meters();
        if !(od.is_finite() && od > 0.0) {
            return Err(SpringError::InconsistentInputs(
                "max_outer_dia must be a positive finite number".into(),
            ));
        }
    }

    let inactive = req.end_type.resolve_inactive(req.inactive_coils);

    let mut best: Option<MinWeightSolution> = None;

    for &d in &req.candidate_diameters {
        let Some((mut mean, mut binding)) =
            best_mean_dia(material, d, req.max_force, req.index_bounds, correction)
        else {
            continue;
        };
        // Apply an optional outer-diameter cap.
        if let Some(od_max) = req.max_outer_dia {
            if mean.meters() + d.meters() > od_max.meters() {
                let capped = od_max.meters() - d.meters();
                if capped / d.meters() < c_min {
                    continue; // capping would push index below the floor
                }
                mean = Length::from_meters(capped);
                binding = BindingConstraint::OuterDiameter;
            }
        }
        let active = active_coils_for_rate(material.shear_modulus, d, mean, req.required_rate);
        if !active.is_finite() || active < 1.0 {
            continue; // non-finite or fewer than one active coil is unphysical
        }
        let solid = req.end_type.solid_length(d, active, inactive);
        let travel = req.max_force.newtons() / req.required_rate.newtons_per_meter();
        let free_length =
            Length::from_meters(solid.meters() + travel * (1.0 + req.clash_allowance));
        // Reject buckling-prone geometry (spec §5 constraint; Shigley Eq. 10-10).
        // The optimizer is biased toward large mean diameters, which is exactly the
        // slender regime most likely to buckle, so this check is load-bearing.
        if !is_buckling_stable(
            free_length,
            mean,
            material.youngs_modulus,
            material.shear_modulus,
            req.fixity,
        ) {
            continue;
        }
        // `?` is safe here: every solve_forward error mode is already pre-filtered by the
        // guards above — wire is finite-positive (candidate validation), mean ≥ c_min·d > d
        // (c_min ≥ floor > 1), active ≥ 1, free_length = solid + positive travel > solid, and
        // d is in the material range (best_mean_dia's min_tensile_strength(d) already
        // succeeded). So this never aborts a valid solve. (Unlike the extension optimizer,
        // which uses skip-on-Err because its fixed-hook C2 ≤ 1 case is a live per-candidate
        // failure; compression has no such per-candidate error mode, so adding a skip branch
        // here would be unreachable dead code. If a future per-candidate error mode is added,
        // switch this to a skip-on-Err `let Ok(_) = … else { continue }`.)
        let design = solve_forward(
            material,
            req.end_type,
            req.fixity,
            d,
            mean,
            active,
            inactive,
            free_length,
            &[req.max_force],
            correction,
        )?;
        let mass = wire_mass(material, d, mean, design.total_coils);
        if best.as_ref().map(|b| mass < b.mass_kg).unwrap_or(true) {
            best = Some(MinWeightSolution {
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
    use crate::mechanics::EndFixity;
    use crate::units::{Force, Length, SpringRate};
    use approx::assert_relative_eq;

    fn base_request(candidates: Vec<f64>) -> MinWeightRequest {
        MinWeightRequest {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            required_rate: SpringRate::from_newtons_per_meter(2000.0),
            max_force: Force::from_newtons(50.0),
            index_bounds: (4.0, 12.0),
            max_outer_dia: None,
            candidate_diameters: candidates
                .into_iter()
                .map(Length::from_millimeters)
                .collect(),
            clash_allowance: 0.15,
            inactive_coils: None,
        }
    }

    /// Build a request for stress-binding tests.
    ///
    /// d=3 mm, F=300 N, rate=5 000 N/m: stress at c_max=12 (1140 MPa) exceeds
    /// the Music Wire allowable (≈848 MPa), so `best_mean_dia` must root-find
    /// the mean diameter where stress == allowable (BindingConstraint::Stress).
    fn stress_binding_request(candidates: Vec<f64>) -> MinWeightRequest {
        MinWeightRequest {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            required_rate: SpringRate::from_newtons_per_meter(5_000.0),
            max_force: Force::from_newtons(300.0),
            index_bounds: (4.0, 12.0),
            max_outer_dia: None,
            candidate_diameters: candidates
                .into_iter()
                .map(Length::from_millimeters)
                .collect(),
            clash_allowance: 0.15,
            inactive_coils: None,
        }
    }

    /// Build a request for index-binding tests.
    ///
    /// d=3 mm, F=50 N, rate=5 000 N/m: stress at c_max=12 (190 MPa) is well
    /// within the Music Wire allowable (≈848 MPa), so `best_mean_dia` returns
    /// dm_hi with BindingConstraint::Index.
    fn index_binding_request(candidates: Vec<f64>) -> MinWeightRequest {
        MinWeightRequest {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            required_rate: SpringRate::from_newtons_per_meter(5_000.0),
            max_force: Force::from_newtons(50.0),
            index_bounds: (4.0, 12.0),
            max_outer_dia: None,
            candidate_diameters: candidates
                .into_iter()
                .map(Length::from_millimeters)
                .collect(),
            clash_allowance: 0.15,
            inactive_coils: None,
        }
    }

    // ── wire_mass arithmetic ──────────────────────────────────────────────────
    //
    // mass = rho * (pi^2 / 4) * d^2 * D * Nt
    // d=2 mm, D=20 mm, Nt=12, rho=7850 kg/m^3 (Music Wire)
    // = 7850 * (pi^2/4) * 4e-6 * 0.020 * 12 = 1.85943…e-2 kg
    //
    // Kills: replace wire_mass -> 1.0; all * ↔ +/÷ swaps in the formula;
    // the PI*PI ÷ 4 term; the d.powi(2) term.
    #[test]
    fn wire_mass_exact_value() {
        let m = crate::test_support::music_wire();
        // wire_mass is private, but solve_min_weight exposes mass_kg.
        // Construct a request where the geometry is fully determined so we can
        // predict mass to high precision:
        // d=3mm, F=50N, rate=5000N/m → Index binding at c=12 → dm=36mm.
        // Na = G*d^4/(8*dm^3*rate) = 80e9*(3e-3)^4/(8*(36e-3)^3*5000) = 3.4722...
        // Nt = Na + 2 (SquaredGround) = 5.4722...
        // mass = 7850 * (pi^2/4) * (3e-3)^2 * 36e-3 * 5.4722...
        //      = 7850 * 2.4674e0 * 9e-6 * 36e-3 * 5.4722...
        //      ≈ 3.4341e-2 kg
        let sol = solve_min_weight(
            &m,
            &index_binding_request(vec![3.0]),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        // Computed by independent Python simulation from the formula:
        // rho*(pi^2/4)*d^2*D*Nt = 7850*(pi^2/4)*(0.003)^2*0.036*5.4722...
        assert_relative_eq!(sol.mass_kg, 3.434_141_188_364_544e-2, max_relative = 1e-8);
    }

    // ── allowable = pct * MTS (line 62: * mutant) ────────────────────────────
    //
    // allowable = allowable_pct_torsion * MTS(d).
    // Mutant "* → +" yields allowable = pct + MTS ≈ MTS (two billion Pa), making
    // virtually any design feasible. This test exercises the infeasible branch
    // (stress_at(dm_lo) > allowable) which only fires with the *real* product.
    //
    // Music Wire d=2 mm: allowable_real ≈ 899 MPa.
    // At F=300 N, c_min=4: stress ≈ 1072 MPa > 899 MPa → returns None.
    // With + mutant: allowable_mutated ≈ 2000 MPa, 1072 < 2000 → feasible → Some.
    #[test]
    fn allowable_is_pct_times_mts_not_pct_plus_mts() {
        let m = crate::test_support::music_wire();
        // F=300 N stresses the 2 mm wire at c_min past its real allowable,
        // so the single candidate must be rejected → Infeasible.
        let req = MinWeightRequest {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            required_rate: SpringRate::from_newtons_per_meter(5_000.0),
            max_force: Force::from_newtons(300.0),
            index_bounds: (4.0, 12.0),
            max_outer_dia: None,
            candidate_diameters: vec![Length::from_millimeters(2.0)],
            clash_allowance: 0.15,
            inactive_coils: None,
        };
        assert!(
            matches!(
                solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser),
                Err(SpringError::Infeasible(_))
            ),
            "d=2mm at F=300N must be infeasible (stress_at_c_min > 0.45*MTS)"
        );
    }

    // ── best_mean_dia: feasibility comparison (line 77: > mutants) ───────────
    //
    // `if stress_at(dm_lo) - allowable > 0.0 { return None; }`
    // Mutant ">" → "==" passes the equality case through and finds a root
    // (which would be exactly at dm_lo), returning Some instead of None.
    // Mutant ">" → ">=" would reject the case where stress == allowable exactly.
    //
    // Pin the boundary: construct a request where the 2 mm wire at c_min is
    // exactly the borderline case by checking the sign.
    // The existing `allowable_is_pct_times_mts_not_pct_plus_mts` already kills
    // the `> → ==` branch (stress > allowable → returns None, not Some).
    // We add a complementary test: infeasible must NOT fire when stress < allowable.
    #[test]
    fn feasibility_check_does_not_reject_below_allowable() {
        let m = crate::test_support::music_wire();
        // d=3mm at F=50N: stress at c_min (≈79 MPa) << allowable (≈848 MPa).
        // Must succeed (not return None from the first guard).
        let sol = solve_min_weight(
            &m,
            &index_binding_request(vec![3.0]),
            CurvatureCorrection::Bergstrasser,
        );
        assert!(
            sol.is_ok(),
            "d=3mm at F=50N must be feasible (stress_at_c_min << allowable)"
        );
    }

    // ── best_mean_dia: index ceiling (line 81-82: <= mutants, return dm_hi) ──
    //
    // `if stress_at(dm_hi) - allowable <= 0.0 { return Some(dm_hi, Index); }`
    // Mutant "<=" → "<" would miss the equality case.
    // Mutant returns wrong BindingConstraint if swapped.
    //
    // d=3mm, F=50N: stress at c_max=12 (190 MPa) < allowable (848 MPa) → Index.
    // The solution binding must be Index, and the mean diameter must be
    // c_max*d = 12*3 = 36 mm.
    #[test]
    fn stress_within_allowable_at_cmax_binds_index() {
        let m = crate::test_support::music_wire();
        let sol = solve_min_weight(
            &m,
            &index_binding_request(vec![3.0]),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert_eq!(
            sol.binding,
            BindingConstraint::Index,
            "stress at c_max << allowable must bind the index ceiling"
        );
        // Mean diameter must be c_max * d = 12 * 3 mm = 36 mm.
        assert_relative_eq!(sol.design.mean_dia.millimeters(), 36.0, max_relative = 1e-9);
        assert_relative_eq!(sol.design.index, 12.0, max_relative = 1e-9);
    }

    // ── best_mean_dia: root-find branch (lines 85-92: stress binds) ──────────
    //
    // d=3mm, F=300N: stress at c_min (477 MPa) < allowable (848 MPa) but stress
    // at c_max (1140 MPa) > allowable → bisection returns the stress-limiting dm.
    // Mutants in lines 86 (- → +/÷) change the function passed to find_root and
    // return the wrong mean diameter; the binding must be Stress.
    #[test]
    fn stress_exceeding_allowable_at_cmax_binds_stress() {
        let m = crate::test_support::music_wire();
        let sol = solve_min_weight(
            &m,
            &stress_binding_request(vec![3.0]),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert_eq!(
            sol.binding,
            BindingConstraint::Stress,
            "stress at c_max > allowable must bind the stress limit"
        );
        // Mean diameter must be strictly inside (c_min*d, c_max*d).
        let d = sol.design.wire_dia.millimeters();
        let dm = sol.design.mean_dia.millimeters();
        assert!(
            dm > 4.0 * d + 1e-6 && dm < 12.0 * d - 1e-6,
            "stress-binding dm={dm:.4}mm must be strictly inside index bounds"
        );
    }

    // Pin the exact stress-binding mean diameter and mass so that arithmetic
    // mutations (+ ↔ -, * ↔ ÷) in the root residual change the result.
    //
    // Python simulation (independent, same formula):
    //   d=3mm, F=300N, Music Wire: allowable=848.435 MPa
    //   Bisection root: dm ≈ 25.5924019 mm  (c ≈ 8.5308)
    //   na = G*d^4/(8*dm^3*rate) ≈ 9.6646  (rate=5000 N/m)
    //   Nt = 11.6646 (SquaredGround)
    //   mass = 7850*(pi^2/4)*(0.003)^2*0.025592*11.6646 ≈ 5.2039e-2 kg
    #[test]
    fn stress_binding_mean_dia_and_mass_are_exact() {
        let m = crate::test_support::music_wire();
        // The exact dm and mass were computed with Wahl factor; pass Wahl to keep the pin valid.
        let sol = solve_min_weight(
            &m,
            &stress_binding_request(vec![3.0]),
            CurvatureCorrection::Wahl,
        )
        .unwrap();
        assert_relative_eq!(
            sol.design.mean_dia.millimeters(),
            25.592_401_871_8,
            max_relative = 1e-6
        );
        assert_relative_eq!(sol.mass_kg, 5.203_926_481_489_774e-2, max_relative = 1e-6);
    }

    // ── solve_min_weight: OD cap (lines 108-114) ─────────────────────────────
    //
    // Without cap, d=3mm F=300N: stress-binding dm≈25.59mm, OD≈28.59mm.
    // With od_max=28mm: OD without cap (28.59mm) > 28mm → cap fires.
    //   capped_dm = 28 - 3 = 25mm, c=8.33 ≥ c_min=4 → valid.
    // Binding must switch to OuterDiameter; OD of the design must equal od_max.
    //
    // Kills line 108 "+" → "-" (OD check wrong), ">" → ">=" (off-by-one),
    // line 109 "-" → "+" or "÷" (wrong capped mean), line 114 (wrong binding).
    #[test]
    fn outer_diameter_cap_binds_and_od_equals_od_max() {
        let m = crate::test_support::music_wire();
        let mut req = stress_binding_request(vec![3.0]);
        // od_max=28mm: uncapped OD≈28.59mm fires the cap; capped_dm=25mm (c=8.33 ≥ 4).
        req.max_outer_dia = Some(Length::from_millimeters(28.0));
        let sol = solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser).unwrap();
        assert_eq!(
            sol.binding,
            BindingConstraint::OuterDiameter,
            "OD cap must set binding=OuterDiameter"
        );
        // OD = mean + d; with capped_dm=25mm, d=3mm → OD=28mm exactly.
        assert_relative_eq!(
            sol.design.outer_dia.millimeters(),
            28.0,
            max_relative = 1e-9
        );
        // Mean diameter must equal od_max - d = 25 mm.
        assert_relative_eq!(sol.design.mean_dia.millimeters(), 25.0, max_relative = 1e-9);
    }

    // Pin the OD-capped mass so that mutations in the capped-mean arithmetic
    // (lines 109-110) and OD comparison (line 108) change the result.
    //
    // Python simulation: d=3mm, dm=25mm, na=G*d^4/(8*dm^3*rate)=10.3680
    //   Nt=12.3680, mass=7850*(pi^2/4)*(0.003)^2*0.025*12.3680 ≈ 5.3900e-2 kg
    #[test]
    fn outer_diameter_cap_mass_is_exact() {
        let m = crate::test_support::music_wire();
        let mut req = stress_binding_request(vec![3.0]);
        req.max_outer_dia = Some(Length::from_millimeters(28.0));
        let sol = solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser).unwrap();
        assert_relative_eq!(sol.mass_kg, 5.390_032_768_742_724e-2, max_relative = 1e-6);
    }

    // ── OD cap: capped index < c_min → candidate skipped (line 110) ─────────
    //
    // When capped_dm / d < c_min, the candidate is skipped.
    // Kills "< → ==", "< → >", "< → <=", "÷ → %", "÷ → *" on line 110.
    //
    // d=3mm, c_min=4: min valid dm = 4*3 = 12mm; OD_min = 15mm.
    // With od_max=14.9mm: capped_dm=11.9mm, c=3.967 < 4 → skip.
    // All candidates rejected → Infeasible.
    #[test]
    fn outer_diameter_cap_below_min_index_skips_candidate() {
        let m = crate::test_support::music_wire();
        let mut req = stress_binding_request(vec![3.0]);
        // od_max=14.9mm forces capped index=3.967 < c_min=4.0 → skip.
        req.max_outer_dia = Some(Length::from_millimeters(14.9));
        assert!(
            matches!(
                solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser),
                Err(SpringError::Infeasible(_))
            ),
            "OD cap that violates min-index must reject the candidate"
        );
    }

    // Complement: OD cap that still leaves c_cap > c_min must be accepted by the
    // index check.  The test `outer_diameter_cap_binds_and_od_equals_od_max` already
    // covers this (c_cap=8.33 >> c_min=4), so no additional test is needed here.
    //
    // Note: OD cap exactly at c_cap == c_min (od_max = (c_min+1)*d = 15mm for d=3mm)
    // leaves the candidate viable per the index check but results in a geometrically
    // infeasible spring (na >> 1 at rate=5000 N/m → free_length >> crit → buckles →
    // continue → Infeasible).  The buckling skip is orthogonal to the index skip.

    // ── active coils guard: || vs && and < boundary (line 118) ───────────────
    //
    // `if !active.is_finite() || active < 1.0 { continue; }`
    // Mutant "||" → "&&" would skip only when BOTH conditions hold; a finite
    // active < 1.0 would then NOT be skipped, allowing the infeasible design
    // to propagate into solve_forward.
    // Mutant "< → ==" or "< → <=" would keep designs with active < 1.
    //
    // d=2mm, F=50N, rate=15 000 N/m: Index binding at dm=24mm.
    //   na = G*d^4/(8*dm^3*rate) = 80e9*(2e-3)^4/(8*(24e-3)^3*15000) ≈ 0.772 < 1.0 → skip.
    // With only d=2mm as candidate → Infeasible.
    #[test]
    fn active_coils_below_one_rejects_candidate() {
        let m = crate::test_support::music_wire();
        // d=2mm, rate=15 000 N/m: Index binding at dm=24mm gives na≈0.772 < 1.0 → skip.
        let req = MinWeightRequest {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            required_rate: SpringRate::from_newtons_per_meter(15_000.0),
            max_force: Force::from_newtons(50.0),
            index_bounds: (4.0, 12.0),
            max_outer_dia: None,
            candidate_diameters: vec![Length::from_millimeters(2.0)],
            clash_allowance: 0.15,
            inactive_coils: None,
        };
        assert!(
            matches!(
                solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser),
                Err(SpringError::Infeasible(_))
            ),
            "d=2mm at rate=15000 N/m has active ≈ 0.772 < 1.0 and must be skipped"
        );
    }

    // Complement: d=3mm at rate=15 000 N/m has na≈1.157 >= 1.0 and must succeed.
    // Kills "||" → "&&" indirectly: both conditions false for this case, same as original.
    #[test]
    fn active_coils_at_least_one_is_not_rejected() {
        let m = crate::test_support::music_wire();
        let req = MinWeightRequest {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            required_rate: SpringRate::from_newtons_per_meter(15_000.0),
            max_force: Force::from_newtons(50.0),
            index_bounds: (4.0, 12.0),
            max_outer_dia: None,
            candidate_diameters: vec![Length::from_millimeters(3.0)],
            clash_allowance: 0.15,
            inactive_coils: None,
        };
        assert!(
            solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser).is_ok(),
            "d=3mm at rate=15000 N/m must have active >= 1.0 and succeed"
        );
    }

    // ── free_length arithmetic (line 124) ────────────────────────────────────
    //
    // `free_length = solid + travel * (1 + clash_allowance)`
    // Mutants on line 124: "+" → "-" on clash term, "+" → "*", "*" → "÷".
    //
    // Pin the free_length value via design.free_length.
    // d=3mm, F=50N, rate=5000N/m, index binding at dm=36mm:
    //   na = G*(3e-3)^4/(8*(36e-3)^3*5000) = 3.4722
    //   Nt = 5.4722 (SquaredGround: Nt=Na+2)
    //   solid = d*Nt = 3e-3*5.4722 = 16.4167mm
    //   travel = 50/5000 = 10mm
    //   free = 16.4167 + 10*(1+0.15) = 16.4167 + 11.5 = 27.9167mm
    //
    // Kills "+→-" (27.9167 → 25.9167), "+→*" (27.9167 → 17.5167), "*→÷" (17.4667).
    #[test]
    fn free_length_includes_clash_allowance() {
        let m = crate::test_support::music_wire();
        let sol = solve_min_weight(
            &m,
            &index_binding_request(vec![3.0]),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        // travel=10mm; with clash=0.15: extra = 10*0.15=1.5mm, so free = solid+11.5mm.
        // without clash: free would be solid+10 = 26.4167mm.
        // Exact value from Python simulation: 27.91666667mm.
        assert_relative_eq!(
            sol.design.free_length.millimeters(),
            27.916_666_67,
            max_relative = 1e-6
        );
        // Also verify the design itself captures the correct solid length.
        // solid = d * Nt = 3mm * 5.4722 = 16.4167mm
        assert_relative_eq!(
            sol.design.solid_length.millimeters(),
            16.416_666_67,
            max_relative = 1e-6
        );
    }

    // ── mass selection: `mass < best.mass_kg` (line 148) ─────────────────────
    //
    // Mutants "< → ==" (keeps first, ignores ties), "< → <=" (always replaces).
    // With two candidates where d=2mm is lighter than d=3mm at the same load,
    // the final binding=Index mass must match the 2mm solution exactly.
    //
    // F=50N, rate=5000N/m: both 2mm and 3mm are Index-binding; 2mm is lighter
    // because it needs a smaller dm (24mm vs 36mm) and has fewer coils.
    // Python: mass_2mm ≈ 8.023e-3 kg, mass_3mm ≈ 3.434e-2 kg.
    #[test]
    fn mass_comparison_selects_lighter_design() {
        let m = crate::test_support::music_wire();
        let sol = solve_min_weight(
            &m,
            &index_binding_request(vec![2.0, 3.0]),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        // The 2mm candidate must win.
        assert_relative_eq!(sol.design.wire_dia.millimeters(), 2.0, max_relative = 1e-9);
        // The mass must equal the single-candidate 2mm solution exactly.
        let sol_single = solve_min_weight(
            &m,
            &index_binding_request(vec![2.0]),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert_relative_eq!(sol.mass_kg, sol_single.mass_kg, max_relative = 1e-12);
    }

    // Pin the exact 2mm index-binding mass for arithmetic mutation coverage.
    //
    // Python: d=2mm, dm=24mm (c=12), na=G*d^4/(8*dm^3*rate)=2.3148
    //   Nt=4.3148, mass=7850*(pi^2/4)*(0.002)^2*0.024*4.3148 ≈ 8.023e-3 kg
    #[test]
    fn index_binding_mass_exact_value() {
        let m = crate::test_support::music_wire();
        let sol = solve_min_weight(
            &m,
            &index_binding_request(vec![2.0]),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert_relative_eq!(sol.mass_kg, 8.023_107_534e-3, max_relative = 1e-5);
    }

    // ── OD comparison boundary: > vs >= (line 108) ───────────────────────────
    //
    // `if mean.meters() + d.meters() > od_max.meters()`
    // Mutant ">" → ">=" also fires when mean + d == od_max (exact FP equality),
    // changing binding from Index to OuterDiameter even though OD was not exceeded.
    //
    // d=3mm, c_max=8: dm_hi = 8*(3/1000) = 0.024, OD = 0.024 + 0.003 = 0.027.
    // 24e-3 + 3e-3 == 27.0/1000.0 in IEEE 754 (verified: same f64 bit pattern).
    // Original (>): 0.027 > 0.027 = false → no cap → Index binding.
    // Mutant (>=): 0.027 >= 0.027 = true → cap fires: capped_dm = 27-3 = 24mm (same
    //   as dm_hi), but binding wrongly becomes OuterDiameter.
    //
    // rate=58593.75 N/m: G*(3e-3)^4/(8*(24e-3)^3*58593.75) = 1.0 exactly in Rust,
    // so active coils passes the >=1.0 guard. F=50N: stress<<allowable → Index binding.
    #[test]
    fn od_cap_does_not_fire_when_od_equals_od_max_exactly() {
        let m = crate::test_support::music_wire();
        // Index binding at d=3mm, c_max=8: OD = 27mm (FP-exact equal to od_max).
        // The cap must NOT fire; binding must be Index, not OuterDiameter.
        let req = MinWeightRequest {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            required_rate: SpringRate::from_newtons_per_meter(58_593.75),
            max_force: Force::from_newtons(50.0),
            index_bounds: (4.0, 8.0),
            max_outer_dia: Some(Length::from_millimeters(27.0)),
            candidate_diameters: vec![Length::from_millimeters(3.0)],
            clash_allowance: 0.15,
            inactive_coils: None,
        };
        let sol = solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser).unwrap();
        // Binding must stay Index (uncapped OD == od_max, strict > means cap doesn't fire).
        assert_eq!(
            sol.binding,
            BindingConstraint::Index,
            "OD == od_max (exact FP) must NOT trigger the cap (strict > comparison)"
        );
        // Mean diameter must be c_max*d = 24mm (unchanged from best_mean_dia).
        assert_relative_eq!(sol.design.mean_dia.millimeters(), 24.0, max_relative = 1e-9);
    }

    // ── OD cap: c_cap == c_min boundary (line 110: < → == and < → <=) ────────
    // AND active coils == 1.0 (line 118: < → <=) ──────────────────────────────
    //
    // `if capped / d.meters() < c_min { continue; }` (line 110)
    // Mutants "< → ==" and "< → <=" both skip when c_cap == c_min EXACTLY in FP.
    //
    // `if !active.is_finite() || active < 1.0 { continue; }` (line 118)
    // Mutant "< → <=" skips when active == 1.0 exactly in FP.
    //
    // A single scenario covers both:
    // d=3mm, od_max=15mm, rate=468750 N/m, F=300N:
    //   best_mean_dia: Stress binding at dm≈25.6mm, OD≈28.6mm > 15mm → cap fires.
    //   capped = od_max - d = 15-3 = 12mm, c_cap = 12/3 = 4.0 = c_min (IEEE 754 exact).
    //   na = G*(3e-3)^4 / (8*(12e-3)^3 * 468750) = 6.48/6.48 = 1.0 (IEEE 754 exact).
    //   Solid = 15mm, free ≈ 9.74mm, crit ≈ 62.1mm → buckling stable.
    //
    // Original: c_cap < c_min? 4.0 < 4.0 = false → proceeds; na < 1.0? 1.0 < 1.0 = false → proceeds.
    // Mutants: < → == or <= fire on c_cap==c_min; < → <= fires on na==1.0. All return Infeasible.
    #[test]
    fn od_cap_at_c_min_boundary_and_active_exactly_one_are_not_rejected() {
        let m = crate::test_support::music_wire();
        // d=3mm, od_max=15mm: capped_dm=12mm, c_cap = (15-3)/3 = 4.0 = c_min (FP exact).
        // rate=468750 N/m: na = G*(3e-3)^4/(8*(12e-3)^3*468750) = 6.48/6.48 = 1.0 (FP exact).
        let req = MinWeightRequest {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            required_rate: SpringRate::from_newtons_per_meter(468_750.0),
            max_force: Force::from_newtons(300.0),
            index_bounds: (4.0, 12.0),
            max_outer_dia: Some(Length::from_millimeters(15.0)),
            candidate_diameters: vec![Length::from_millimeters(3.0)],
            clash_allowance: 0.15,
            inactive_coils: None,
        };
        let sol = solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser);
        assert!(
            sol.is_ok(),
            "c_cap == c_min (exact FP) and active == 1.0 (exact FP) must NOT be rejected"
        );
        let sol = sol.unwrap();
        // Binding must be OuterDiameter (cap fired at od_max=15mm).
        assert_eq!(
            sol.binding,
            BindingConstraint::OuterDiameter,
            "OD cap at c_min boundary must set binding=OuterDiameter"
        );
        // Active coils must be exactly 1.0 (IEEE 754 exact cancellation).
        assert_relative_eq!(sol.design.active_coils, 1.0, max_relative = 1e-12);
        // OD = capped_dm + d = 12 + 3 = 15mm = od_max.
        assert_relative_eq!(
            sol.design.outer_dia.millimeters(),
            15.0,
            max_relative = 1e-9
        );
    }

    // Complement for active == 1.0 without OD cap:
    // d=3mm, c_max=8, rate=58593.75 N/m, F=50N:
    //   G*(3e-3)^4/(8*(24e-3)^3*58593.75) = 1.0 exactly in Rust (verified: same
    //   f64 bit pattern as 1.0).
    // Original: 1.0 < 1.0 = false → proceeds.
    // Mutant (<= ): 1.0 <= 1.0 = true → skipped → Infeasible.
    #[test]
    fn active_coils_exactly_one_is_not_rejected() {
        let m = crate::test_support::music_wire();
        // Index binding at c_max=8 with d=3mm: dm=24mm.
        // na = G*(3e-3)^4/(8*(24e-3)^3*58593.75) = 1.0 exactly in Rust IEEE 754.
        let req = MinWeightRequest {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            required_rate: SpringRate::from_newtons_per_meter(58_593.75),
            max_force: Force::from_newtons(50.0),
            index_bounds: (4.0, 8.0),
            max_outer_dia: None,
            candidate_diameters: vec![Length::from_millimeters(3.0)],
            clash_allowance: 0.15,
            inactive_coils: None,
        };
        let sol = solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser).unwrap();
        // na must be exactly 1.0 (IEEE 754: numerator and denominator converge to same bits).
        assert_relative_eq!(sol.design.active_coils, 1.0, max_relative = 1e-12);
    }

    // ── mass selection: heavy-first candidate order (line 148: < → ==) ────────
    //
    // `if best.as_ref().map(|b| mass < b.mass_kg).unwrap_or(true)`
    // Mutant "< → ==" never updates best after the first candidate (mass == inf is false,
    // and then subsequent masses are never == to the first mass_kg).
    // When candidates are [heavy_first, light_second]:
    //   Original: picks heavy first (None → Some), then light replaces (light < heavy).
    //   Mutant ==: picks heavy first, then light == heavy? No → never replaces. Returns heavy.
    //
    // d=3mm (heavy, mass≈3.43e-2 kg) before d=2mm (light, mass≈8.02e-3 kg), both Index-binding.
    #[test]
    fn mass_comparison_selects_lighter_even_when_heavy_comes_first() {
        let m = crate::test_support::music_wire();
        // Reversed candidate order: 3mm (heavy) → 2mm (light).
        // < → == mutant: picks 3mm first, then 2mm == 3mm? No → returns 3mm (WRONG).
        // Original <: picks 3mm first, then 2mm < 3mm → replaces. Returns 2mm (CORRECT).
        let sol = solve_min_weight(
            &m,
            &index_binding_request(vec![3.0, 2.0]),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert_relative_eq!(sol.design.wire_dia.millimeters(), 2.0, max_relative = 1e-9,);
        let sol_single = solve_min_weight(
            &m,
            &index_binding_request(vec![2.0]),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert_relative_eq!(sol.mass_kg, sol_single.mass_kg, max_relative = 1e-12);
    }

    // ── existing tests retained ───────────────────────────────────────────────

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
        // Stress at the operating load within allowable.
        let allowable = m.allowable_pct_torsion;
        assert!(sol.design.load_points[0].pct_mts <= allowable + 1e-6);
        // Index within bounds.
        assert!(sol.design.index >= 4.0 - 1e-9 && sol.design.index <= 12.0 + 1e-9);
        assert!(sol.mass_kg > 0.0);
    }

    #[test]
    fn picks_global_minimum_over_candidates() {
        let m = crate::test_support::music_wire();
        let candidates = vec![1.5, 2.0, 2.5, 3.0];
        // Per-candidate mass via the same function restricted to one diameter.
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
    fn solution_does_not_buckle() {
        let m = crate::test_support::music_wire();
        let sol = solve_min_weight(
            &m,
            &base_request(vec![1.5, 2.0, 2.5, 3.0]),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert!(sol.design.buckling_stable);
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

    // ── direct-call input validation ─────────────────────────────────────────
    //
    // solve_min_weight is a public entry point; a direct caller bypasses the
    // scenario-path validation in min_weight_request_from_spec. These pin each
    // up-front guard so malformed inputs return InconsistentInputs (not a
    // misleading Infeasible, and never a silently-overstressed design). Each is
    // a distinct guard so the in-diff mutation gate can kill the new comparisons.

    fn assert_inconsistent(req: &MinWeightRequest) {
        let m = crate::test_support::music_wire();
        assert!(
            matches!(
                solve_min_weight(&m, req, CurvatureCorrection::Bergstrasser),
                Err(SpringError::InconsistentInputs(_))
            ),
            "expected InconsistentInputs"
        );
    }

    fn assert_accepted(req: &MinWeightRequest) {
        let m = crate::test_support::music_wire();
        assert!(
            solve_min_weight(&m, req, CurvatureCorrection::Bergstrasser).is_ok(),
            "expected Ok"
        );
    }

    #[test]
    fn zero_rate_rejected() {
        let mut req = base_request(vec![2.0]);
        req.required_rate = SpringRate::from_newtons_per_meter(0.0);
        assert_inconsistent(&req);
    }

    #[test]
    fn non_finite_rate_rejected() {
        let mut req = base_request(vec![2.0]);
        req.required_rate = SpringRate::from_newtons_per_meter(f64::NAN);
        assert_inconsistent(&req);
    }

    #[test]
    fn zero_force_rejected() {
        let mut req = base_request(vec![2.0]);
        req.max_force = Force::from_newtons(0.0);
        assert_inconsistent(&req);
    }

    #[test]
    fn non_finite_force_rejected() {
        let mut req = base_request(vec![2.0]);
        req.max_force = Force::from_newtons(f64::INFINITY);
        assert_inconsistent(&req);
    }

    #[test]
    fn negative_clash_allowance_rejected() {
        let mut req = base_request(vec![2.0]);
        req.clash_allowance = -0.1;
        assert_inconsistent(&req);
    }

    #[test]
    fn non_finite_clash_allowance_rejected() {
        let mut req = base_request(vec![2.0]);
        req.clash_allowance = f64::NAN;
        assert_inconsistent(&req);
    }

    #[test]
    fn clash_allowance_zero_is_accepted() {
        // Pins the `>= 0` boundary so a `>` mutant on the clash guard dies.
        let mut req = base_request(vec![2.0]);
        req.clash_allowance = 0.0;
        assert_accepted(&req);
    }

    #[test]
    fn empty_candidates_rejected() {
        let req = base_request(vec![]);
        assert_inconsistent(&req);
    }

    #[test]
    fn non_finite_candidate_diameter_rejected() {
        let mut req = base_request(vec![2.0]);
        req.candidate_diameters
            .push(Length::from_millimeters(f64::NAN));
        assert_inconsistent(&req);
    }

    #[test]
    fn non_positive_candidate_diameter_rejected() {
        let req = base_request(vec![0.0]);
        assert_inconsistent(&req);
    }

    #[test]
    fn non_finite_index_bound_rejected() {
        let mut req = base_request(vec![2.0]);
        req.index_bounds = (4.0, f64::INFINITY);
        assert_inconsistent(&req);
    }

    #[test]
    fn inverted_index_bounds_rejected() {
        let mut req = base_request(vec![2.0]);
        req.index_bounds = (12.0, 4.0); // c_min > c_max
        assert_inconsistent(&req);
    }

    #[test]
    fn equal_index_bounds_rejected() {
        // Pins the strict `c_min < c_max` boundary: a degenerate range where
        // c_min == c_max must be rejected (kills the `< → <=` mutant).
        let mut req = base_request(vec![2.0]);
        req.index_bounds = (4.0, 4.0);
        assert_inconsistent(&req);
    }

    #[test]
    fn sub_floor_index_min_rejected() {
        // c_min below the Wahl monotonicity floor (1 + √3/2 ≈ 1.866) would let
        // best_mean_dia's single-endpoint feasibility test accept an overstressed
        // design; the floor guard rejects it. (Same bug class as the extension pole.)
        let mut req = base_request(vec![2.0]);
        req.index_bounds = (1.5, 12.0);
        assert_inconsistent(&req);
    }

    #[test]
    fn index_min_exactly_at_floor_is_accepted() {
        // Pins the `c_min >= floor` boundary so a `>` mutant on the floor guard dies.
        let mut req = base_request(vec![2.0]);
        req.index_bounds = (min_spring_index(), 12.0);
        assert_accepted(&req);
    }

    #[test]
    fn non_finite_max_outer_dia_rejected() {
        let mut req = base_request(vec![2.0]);
        req.max_outer_dia = Some(Length::from_millimeters(f64::NAN));
        assert_inconsistent(&req);
    }

    #[test]
    fn non_positive_max_outer_dia_rejected() {
        let mut req = base_request(vec![2.0]);
        req.max_outer_dia = Some(Length::from_millimeters(0.0));
        assert_inconsistent(&req);
    }

    /// Raising the inactive count never lowers the minimum achievable mass (each dead
    /// coil is extra wire). The winning (d, D, active) MAY shift, so assert on mass,
    /// not on unchanged geometry.
    #[test]
    fn min_weight_mass_is_nondecreasing_in_inactive() {
        let base = MinWeightRequest {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            required_rate: SpringRate::from_newtons_per_meter(2000.0),
            max_force: Force::from_newtons(50.0),
            index_bounds: (4.0, 12.0),
            max_outer_dia: None,
            candidate_diameters: vec![Length::from_millimeters(2.0), Length::from_millimeters(3.0)],
            clash_allowance: 0.15,
            inactive_coils: None,
        };
        let m0 = solve_min_weight(
            &crate::test_support::music_wire(),
            &base,
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap()
        .mass_kg;
        let bumped = MinWeightRequest {
            inactive_coils: Some(4.0),
            ..base.clone()
        };
        let m1 = solve_min_weight(
            &crate::test_support::music_wire(),
            &bumped,
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap()
        .mass_kg;
        assert!(m1 >= m0 - 1e-15, "mass must not decrease: m0={m0}, m1={m1}");
    }

    /// `min_weight_mass_is_nondecreasing_in_inactive` alone can't distinguish "the
    /// override is wired" from "the override is silently ignored" — if
    /// `solve_min_weight` fell back to `req.end_type.end_coils()` instead of
    /// resolving `req.inactive_coils`, both sides of that test would be identical
    /// and the `>=` bound would still pass. Pin the wiring directly: at a fixed,
    /// Index-binding geometry (d, D, Na are independent of the inactive count —
    /// `best_mean_dia` and `active_coils_for_rate` never see it), total_coils must
    /// grow by EXACTLY the override delta, and mass must scale with total_coils.
    #[test]
    fn min_weight_inactive_override_adds_exact_dead_coils_to_total() {
        let m = crate::test_support::music_wire();
        let base = index_binding_request(vec![3.0]); // inactive_coils: None -> SquaredGround default 2.0
        let sol0 = solve_min_weight(&m, &base, CurvatureCorrection::Bergstrasser).unwrap();
        let mut bumped = base;
        bumped.inactive_coils = Some(5.0); // +3.0 dead coils over the 2.0 default
        let sol1 = solve_min_weight(&m, &bumped, CurvatureCorrection::Bergstrasser).unwrap();
        // Geometry (d, D, Na) is unaffected by the inactive count.
        assert_relative_eq!(
            sol0.design.wire_dia.millimeters(),
            sol1.design.wire_dia.millimeters(),
            max_relative = 1e-12
        );
        assert_relative_eq!(
            sol0.design.mean_dia.millimeters(),
            sol1.design.mean_dia.millimeters(),
            max_relative = 1e-12
        );
        assert_relative_eq!(
            sol0.design.active_coils,
            sol1.design.active_coils,
            max_relative = 1e-12
        );
        // Total coils must grow by exactly the override delta (3.0).
        assert_relative_eq!(
            sol1.design.total_coils - sol0.design.total_coils,
            3.0,
            max_relative = 1e-9
        );
        // Mass is proportional to total_coils at fixed (d, D).
        assert_relative_eq!(
            sol1.mass_kg / sol0.mass_kg,
            sol1.design.total_coils / sol0.design.total_coils,
            max_relative = 1e-9
        );
    }

    #[test]
    fn min_spring_index_is_wahl_turning_point() {
        // C* is the minimum of the Wahl-corrected τ(C)·C curve, i.e. the root of
        // d/dC[Kw·C] = 0 ⟹ 4C² − 8C + 1 = 0. Verify the property directly (not just
        // by recomputing the literal), so the test pins the mathematical claim.
        let c = min_spring_index();
        assert_relative_eq!(4.0 * c * c - 8.0 * c + 1.0, 0.0, epsilon = 1e-12);
        // And confirm it is the upper root (≈1.866), not the lower one (≈0.134).
        assert!(c > 1.0);
    }
}
