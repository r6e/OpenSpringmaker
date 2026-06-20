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
    material.density.kg_per_m3() * (PI * PI / 4.0) * d * d * dm * total_coils
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
    // Shear stress at max force as a function of mean diameter (monotonic increasing).
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
        if active < 1.0 {
            continue; // fewer than one active coil is unphysical
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
    use crate::material::{Material, MaterialSet};
    use crate::mechanics::EndFixity;
    use crate::units::{Force, Length, SpringRate};
    use approx::assert_relative_eq;

    fn music_wire() -> Material {
        MaterialSet::load_default()
            .get("Music Wire")
            .unwrap()
            .clone()
    }

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

    #[test]
    fn solution_is_feasible() {
        let m = music_wire();
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
        let m = music_wire();
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
        let m = music_wire();
        let sol = solve_min_weight(&m, &base_request(vec![1.5, 2.0, 2.5, 3.0])).unwrap();
        assert!(sol.design.buckling_stable);
    }

    #[test]
    fn infeasible_when_outer_diameter_too_small() {
        let m = music_wire();
        let mut req = base_request(vec![1.5, 2.0, 2.5]);
        req.max_outer_dia = Some(Length::from_millimeters(3.0)); // forces index < 4
        assert!(matches!(
            solve_min_weight(&m, &req),
            Err(SpringError::Infeasible(_))
        ));
    }
}
