//! Aggregate forward solve for extension springs: from fully-determined geometry
//! to a complete design. Hook-stress checks replace the solid-length / buckling
//! checks that belong only to compression springs.

use crate::extension::ends::HookEnds;
use crate::extension::mechanics::{deflection, hook_bending_stress, hook_torsion_stress};
use crate::material::Material;
use crate::mechanics::{corrected_shear_stress, spring_index, spring_rate, wahl_factor};
use crate::units::{Force, Length, SpringRate, Stress};
use crate::{Result, SpringError};

/// State of an extension spring at one axial load.
#[derive(Debug, Clone, Copy)]
pub struct ExtLoadPoint {
    pub force: Force,
    pub deflection: Length,
    pub length: Length,
    pub body_shear: Stress,
    pub hook_bending: Stress,
    pub hook_torsion: Stress,
    /// body_shear / (allowable_pct_torsion · Sut)
    pub pct_body_allow: f64,
    /// hook_bending (σ_A) / (allowable_pct_bending · Sut)
    pub pct_hook_bending_allow: f64,
    /// hook_torsion (τ_B) / (allowable_pct_torsion · Sut)
    pub pct_hook_torsion_allow: f64,
}

/// A fully computed extension-spring design.
#[derive(Debug, Clone)]
pub struct ExtensionDesign {
    pub wire_dia: Length,
    pub mean_dia: Length,
    pub index: f64,
    pub active_coils: f64,
    pub rate: SpringRate,
    pub free_length: Length,
    pub initial_tension: Force,
    pub outer_dia: Length,
    pub inner_dia: Length,
    pub min_tensile_strength: Stress,
    pub hooks: HookEnds,
    pub load_points: Vec<ExtLoadPoint>,
}

#[allow(clippy::too_many_arguments)]
fn ext_load_point(
    force: Force,
    initial_tension: Force,
    rate: SpringRate,
    free_length: Length,
    mean_dia: Length,
    wire_dia: Length,
    index: f64,
    hooks: HookEnds,
    mts: Stress,
    allowable_pct_torsion: f64,
    allowable_pct_bending: f64,
) -> ExtLoadPoint {
    // Extension deflection y = max(0, (F − F_i) / k) (Shigley extension springs).
    let y = deflection(force, initial_tension, rate);
    // Extension springs lengthen under load: L = L0 + y.
    let length = Length::from_meters(free_length.meters() + y.meters());
    // Body shear stress with Wahl correction (Shigley Eq. 10-7).
    let body_shear = corrected_shear_stress(force, mean_dia, wire_dia, wahl_factor(index));
    // Hook stresses (Shigley extension spring hook curvature factors).
    let hook_bending = hook_bending_stress(force, mean_dia, wire_dia, hooks.r1);
    let hook_torsion = hook_torsion_stress(force, mean_dia, wire_dia, hooks.r2);

    let allow_torsion = allowable_pct_torsion * mts.pascals();
    let allow_bending = allowable_pct_bending * mts.pascals();

    ExtLoadPoint {
        force,
        deflection: y,
        length,
        body_shear,
        hook_bending,
        hook_torsion,
        pct_body_allow: body_shear.pascals() / allow_torsion,
        pct_hook_bending_allow: hook_bending.pascals() / allow_bending,
        pct_hook_torsion_allow: hook_torsion.pascals() / allow_torsion,
    }
}

/// Compute a complete extension-spring design from determined geometry plus operating loads.
#[allow(clippy::too_many_arguments)]
pub fn solve_forward(
    material: &Material,
    wire_dia: Length,
    mean_dia: Length,
    active: f64,
    free_length: Length,
    initial_tension: Force,
    hooks: HookEnds,
    loads: &[Force],
) -> Result<ExtensionDesign> {
    // Spring index must exceed 1 (mean_dia > wire_dia) for a physically valid spring.
    if mean_dia.meters() <= wire_dia.meters() {
        return Err(SpringError::InconsistentInputs(
            "mean diameter must exceed wire diameter (spring index must exceed 1)".into(),
        ));
    }
    // Initial tension is a built-in preload; negative values are physically meaningless.
    if initial_tension.newtons() < 0.0 {
        return Err(SpringError::InconsistentInputs(
            "initial tension must be non-negative".into(),
        ));
    }
    // Hook curvature index C1 = 2·r1/d must exceed 1; at C1 ≤ 1 the bending
    // factor (K_A) denominator 4·C1·(C1−1) goes to zero or negative.
    let c1 = 2.0 * hooks.r1.meters() / wire_dia.meters();
    if c1 <= 1.0 {
        return Err(SpringError::InconsistentInputs(
            "hook bend radius r1 must exceed d/2 (curvature index C1 = 2·r1/d must exceed 1)"
                .into(),
        ));
    }
    // Hook curvature index C2 = 2·r2/d must exceed 1; at C2 ≤ 1 the torsion
    // factor (K_B) denominator (4·C2−4) goes to zero or negative.
    let c2 = 2.0 * hooks.r2.meters() / wire_dia.meters();
    if c2 <= 1.0 {
        return Err(SpringError::InconsistentInputs(
            "hook bend radius r2 must exceed d/2 (curvature index C2 = 2·r2/d must exceed 1)"
                .into(),
        ));
    }

    let index = spring_index(mean_dia, wire_dia);
    let rate = spring_rate(material.shear_modulus, wire_dia, mean_dia, active);
    let mts = material.min_tensile_strength(wire_dia)?;

    let load_points = loads
        .iter()
        .map(|&f| {
            ext_load_point(
                f,
                initial_tension,
                rate,
                free_length,
                mean_dia,
                wire_dia,
                index,
                hooks,
                mts,
                material.allowable_pct_torsion,
                material.allowable_pct_bending,
            )
        })
        .collect();

    Ok(ExtensionDesign {
        wire_dia,
        mean_dia,
        index,
        active_coils: active,
        rate,
        free_length,
        initial_tension,
        outer_dia: Length::from_meters(mean_dia.meters() + wire_dia.meters()),
        inner_dia: Length::from_meters(mean_dia.meters() - wire_dia.meters()),
        min_tensile_strength: mts,
        hooks,
        load_points,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension::ends::HookEnds;
    use crate::units::{Force, Length};
    use approx::assert_relative_eq;

    #[test]
    fn forward_solve_basic_design() {
        let m = crate::test_support::music_wire();
        let d = solve_forward(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(60.0),
            Force::from_newtons(10.0),
            HookEnds::default_for(Length::from_millimeters(20.0)),
            &[Force::from_newtons(30.0)],
        )
        .unwrap();
        assert_relative_eq!(d.rate.newtons_per_meter(), 2000.0, max_relative = 1e-9);
        // y = (30 − 10)/2000 = 10 mm; length = 60 + 10 = 70 mm.
        assert_relative_eq!(
            d.load_points[0].deflection.millimeters(),
            10.0,
            max_relative = 1e-9
        );
        assert_relative_eq!(
            d.load_points[0].length.millimeters(),
            70.0,
            max_relative = 1e-9
        );
        assert!(d.load_points[0].hook_bending.pascals() > d.load_points[0].body_shear.pascals());
    }

    #[test]
    fn rejects_mean_not_exceeding_wire() {
        let m = crate::test_support::music_wire();
        let r = solve_forward(
            &m,
            Length::from_millimeters(5.0),
            Length::from_millimeters(5.0),
            10.0,
            Length::from_millimeters(60.0),
            Force::from_newtons(10.0),
            HookEnds::default_for(Length::from_millimeters(5.0)),
            &[Force::from_newtons(30.0)],
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    /// C1 = 2·r1/d = 2·(2mm)/(4mm) = 1 — bending factor denominator hits zero.
    #[test]
    fn rejects_hook_r1_too_tight() {
        let m = crate::test_support::music_wire();
        // d = 4 mm, r1 = d/2 = 2 mm → C1 = 1.0; r2 = 3 mm → C2 = 1.5 (valid).
        let r = solve_forward(
            &m,
            Length::from_millimeters(4.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(60.0),
            Force::from_newtons(10.0),
            HookEnds {
                r1: Length::from_millimeters(2.0),
                r2: Length::from_millimeters(3.0),
            },
            &[Force::from_newtons(30.0)],
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    /// Default hooks give r2 = D/4; with d = D/2 → C2 = 2·(D/4)/(D/2) = 1 —
    /// torsion factor denominator hits zero.
    #[test]
    fn rejects_default_hooks_low_index_spring() {
        let m = crate::test_support::music_wire();
        // d = 10 mm, D = 20 mm → index 2; default_for(20mm) → r1=10mm (C1=2), r2=5mm (C2=1).
        let r = solve_forward(
            &m,
            Length::from_millimeters(10.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(60.0),
            Force::from_newtons(10.0),
            HookEnds::default_for(Length::from_millimeters(20.0)),
            &[Force::from_newtons(30.0)],
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    /// Pin the pct-allowable denominator mapping: body and hook-torsion use
    /// `allowable_pct_torsion`; hook-bending uses `allowable_pct_bending`.
    /// Swapping the two fractions would make this test fail.
    #[test]
    fn pct_allowable_fractions_use_correct_denominators() {
        let m = crate::test_support::music_wire();
        let wire_dia = Length::from_millimeters(2.0);
        let d = solve_forward(
            &m,
            wire_dia,
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(60.0),
            Force::from_newtons(10.0),
            HookEnds::default_for(Length::from_millimeters(20.0)),
            &[Force::from_newtons(30.0)],
        )
        .unwrap();
        let lp = &d.load_points[0];
        let mts = m.min_tensile_strength(wire_dia).unwrap();

        let expected_body = lp.body_shear.pascals() / (m.allowable_pct_torsion * mts.pascals());
        let expected_hook_torsion =
            lp.hook_torsion.pascals() / (m.allowable_pct_torsion * mts.pascals());
        let expected_hook_bending =
            lp.hook_bending.pascals() / (m.allowable_pct_bending * mts.pascals());

        assert_relative_eq!(lp.pct_body_allow, expected_body, max_relative = 1e-12);
        assert_relative_eq!(
            lp.pct_hook_torsion_allow,
            expected_hook_torsion,
            max_relative = 1e-12
        );
        assert_relative_eq!(
            lp.pct_hook_bending_allow,
            expected_hook_bending,
            max_relative = 1e-12
        );
        // Sanity-check: the two allowable percentages differ, so the test is
        // discriminating — a swap of the torsion/bending denominator would change
        // the values.
        assert_ne!(m.allowable_pct_torsion, m.allowable_pct_bending);
    }
}
