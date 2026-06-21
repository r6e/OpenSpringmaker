//! Minimum-weight constrained optimization for compression springs.
//! Figure of merit = wire mass (Shigley §10-11). The optimum mean diameter for a
//! given wire size lies on the binding stress or index constraint.

use crate::design::{solve_forward, SpringDesign};
use crate::end_type::EndType;
use crate::material::Material;
use crate::mechanics::{
    active_coils_for_rate, corrected_shear_stress, is_buckling_stable, wahl_factor, EndFixity,
};
use crate::numeric::{find_root_bracketed, SolveConfig};
use crate::units::{Force, Length, SpringRate};
use crate::{Result, SpringError};
use std::f64::consts::PI;

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
) -> Option<(Length, BindingConstraint)> {
    let (c_min, c_max) = bounds;
    let allowable =
        material.allowable_pct_torsion * material.min_tensile_strength(d).ok()?.pascals();
    // τ(C) = Kw(C)·C with Kw = (4C−1)/(4C−4) + 0.615/C is NOT globally monotonic.
    // Solving dτ/dC = 0 gives 4C²−8C+1 = 0 → C* = 1 + √3/2 ≈ 1.866; τ is
    // U-shaped with a minimum at C* and is monotonic increasing only for C ≥ 1.866.
    // The single-endpoint feasibility test below (`stress_at(dm_lo) > allowable → None`)
    // is therefore only valid when c_min ≥ 1.866, which Fix 2 (min_weight_request_from_spec)
    // enforces at the entry point.
    let stress_at = |dm_m: f64| {
        let dm = Length::from_meters(dm_m);
        let c = dm_m / d.meters();
        corrected_shear_stress(max_force, dm, d, wahl_factor(c)).pascals()
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
pub fn solve_min_weight(material: &Material, req: &MinWeightRequest) -> Result<MinWeightSolution> {
    let (c_min, _c_max) = req.index_bounds;
    let mut best: Option<MinWeightSolution> = None;

    for &d in &req.candidate_diameters {
        let Some((mut mean, mut binding)) =
            best_mean_dia(material, d, req.max_force, req.index_bounds)
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
        let solid = req.end_type.solid_length(d, active);
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
        let design = solve_forward(
            material,
            req.end_type,
            req.fixity,
            d,
            mean,
            active,
            free_length,
            &[req.max_force],
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
        let sol = solve_min_weight(&m, &index_binding_request(vec![3.0])).unwrap();
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
        };
        assert!(
            matches!(solve_min_weight(&m, &req), Err(SpringError::Infeasible(_))),
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
        let sol = solve_min_weight(&m, &index_binding_request(vec![3.0]));
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
        let sol = solve_min_weight(&m, &index_binding_request(vec![3.0])).unwrap();
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
        let sol = solve_min_weight(&m, &stress_binding_request(vec![3.0])).unwrap();
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
        let sol = solve_min_weight(&m, &stress_binding_request(vec![3.0])).unwrap();
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
        let sol = solve_min_weight(&m, &req).unwrap();
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
        let sol = solve_min_weight(&m, &req).unwrap();
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
            matches!(solve_min_weight(&m, &req), Err(SpringError::Infeasible(_))),
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
        };
        assert!(
            matches!(solve_min_weight(&m, &req), Err(SpringError::Infeasible(_))),
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
        };
        assert!(
            solve_min_weight(&m, &req).is_ok(),
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
        let sol = solve_min_weight(&m, &index_binding_request(vec![3.0])).unwrap();
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
        let sol = solve_min_weight(&m, &index_binding_request(vec![2.0, 3.0])).unwrap();
        // The 2mm candidate must win.
        assert_relative_eq!(sol.design.wire_dia.millimeters(), 2.0, max_relative = 1e-9);
        // The mass must equal the single-candidate 2mm solution exactly.
        let sol_single = solve_min_weight(&m, &index_binding_request(vec![2.0])).unwrap();
        assert_relative_eq!(sol.mass_kg, sol_single.mass_kg, max_relative = 1e-12);
    }

    // Pin the exact 2mm index-binding mass for arithmetic mutation coverage.
    //
    // Python: d=2mm, dm=24mm (c=12), na=G*d^4/(8*dm^3*rate)=2.3148
    //   Nt=4.3148, mass=7850*(pi^2/4)*(0.002)^2*0.024*4.3148 ≈ 8.023e-3 kg
    #[test]
    fn index_binding_mass_exact_value() {
        let m = crate::test_support::music_wire();
        let sol = solve_min_weight(&m, &index_binding_request(vec![2.0])).unwrap();
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
        };
        let sol = solve_min_weight(&m, &req).unwrap();
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
        };
        let sol = solve_min_weight(&m, &req);
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
        };
        let sol = solve_min_weight(&m, &req).unwrap();
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
        let sol = solve_min_weight(&m, &index_binding_request(vec![3.0, 2.0])).unwrap();
        assert_relative_eq!(sol.design.wire_dia.millimeters(), 2.0, max_relative = 1e-9,);
        let sol_single = solve_min_weight(&m, &index_binding_request(vec![2.0])).unwrap();
        assert_relative_eq!(sol.mass_kg, sol_single.mass_kg, max_relative = 1e-12);
    }

    // ── existing tests retained ───────────────────────────────────────────────

    #[test]
    fn solution_is_feasible() {
        let m = crate::test_support::music_wire();
        let sol = solve_min_weight(&m, &base_request(vec![1.5, 2.0, 2.5, 3.0])).unwrap();
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
                solve_min_weight(&m, &base_request(vec![d]))
                    .ok()
                    .map(|s| s.mass_kg)
            })
            .collect();
        let best = solve_min_weight(&m, &base_request(candidates)).unwrap();
        let min = per.iter().cloned().fold(f64::INFINITY, f64::min);
        assert_relative_eq!(best.mass_kg, min, max_relative = 1e-9);
    }

    #[test]
    fn solution_does_not_buckle() {
        let m = crate::test_support::music_wire();
        let sol = solve_min_weight(&m, &base_request(vec![1.5, 2.0, 2.5, 3.0])).unwrap();
        assert!(sol.design.buckling_stable);
    }

    #[test]
    fn infeasible_when_outer_diameter_too_small() {
        let m = crate::test_support::music_wire();
        let mut req = base_request(vec![1.5, 2.0, 2.5]);
        req.max_outer_dia = Some(Length::from_millimeters(3.0)); // forces index < 4
        assert!(matches!(
            solve_min_weight(&m, &req),
            Err(SpringError::Infeasible(_))
        ));
    }
}
