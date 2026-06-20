//! Fatigue analysis for compression springs (Shigley §10-9). Uses cited
//! per-material endurance data (Zimmerli); materials without it degrade
//! gracefully by returning `NoFatigueData`.

use crate::material::Material;
use crate::mechanics::{bergstrasser_factor, corrected_shear_stress, spring_index};
use crate::units::{Force, Length, Stress};
use crate::{Result, SpringError};

/// Ratio of ultimate shear strength to ultimate tensile strength (Shigley Eq. 10-30).
const SHEAR_TO_TENSILE: f64 = 0.67;

/// Result of a fatigue analysis over one load cycle.
#[derive(Debug, Clone, Copy)]
pub struct FatigueResult {
    pub alternating_stress: Stress,
    pub mean_stress: Stress,
    pub fully_reversed_endurance: Stress,
    pub ultimate_shear: Stress,
    pub goodman_factor_of_safety: f64,
}

/// Analyze fatigue for a spring cycling between `force_min` and `force_max`.
pub fn analyze_fatigue(
    material: &Material,
    wire_dia: Length,
    mean_dia: Length,
    force_min: Force,
    force_max: Force,
) -> Result<FatigueResult> {
    if force_max.newtons() < force_min.newtons() {
        return Err(SpringError::InconsistentInputs(
            "max cycle force must be at least the min cycle force".into(),
        ));
    }
    let endurance = material
        .endurance
        .ok_or_else(|| SpringError::NoFatigueData(material.name.clone()))?;

    let c = spring_index(mean_dia, wire_dia);
    let kb = bergstrasser_factor(c);
    let fa = Force::from_newtons((force_max.newtons() - force_min.newtons()) / 2.0);
    let fm = Force::from_newtons((force_max.newtons() + force_min.newtons()) / 2.0);
    let tau_a = corrected_shear_stress(fa, mean_dia, wire_dia, kb);
    let tau_m = corrected_shear_stress(fm, mean_dia, wire_dia, kb);

    let sut = material.min_tensile_strength(wire_dia)?.pascals();
    let ssu = SHEAR_TO_TENSILE * sut;
    // Convert Zimmerli pulsating data to a fully-reversed endurance (Shigley Eq. 10-31).
    let sse = endurance.ssa.pascals() / (1.0 - endurance.ssm.pascals() / ssu);
    // Goodman factor of safety: 1/nf = tau_a/Sse + tau_m/Ssu.
    let nf = 1.0 / (tau_a.pascals() / sse + tau_m.pascals() / ssu);

    Ok(FatigueResult {
        alternating_stress: tau_a,
        mean_stress: tau_m,
        fully_reversed_endurance: Stress::from_pascals(sse),
        ultimate_shear: Stress::from_pascals(ssu),
        goodman_factor_of_safety: nf,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::{Material, MaterialSet};
    use crate::units::{Force, Length};
    use approx::assert_relative_eq;
    use std::f64::consts::PI;

    fn mat(name: &str) -> Material {
        MaterialSet::load_default().get(name).unwrap().clone()
    }

    #[test]
    fn goodman_safety_factor_music_wire() {
        let m = mat("Music Wire");
        let r = analyze_fatigue(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            Force::from_newtons(10.0),
            Force::from_newtons(30.0),
        )
        .unwrap();
        // Independent re-derivation per Shigley §10-9.
        let kb = 42.0 / 37.0; // Bergsträsser at C=10
        let d3 = 0.002_f64.powi(3);
        let ta = kb * 8.0 * 10.0 * 0.020 / (PI * d3); // Fa = 10 N
        let tm = kb * 8.0 * 20.0 * 0.020 / (PI * d3); // Fm = 20 N
        let sut = 2211.0e6 / 2.0_f64.powf(0.145);
        let ssu = 0.67 * sut;
        let sse = 241.0e6 / (1.0 - 379.0e6 / ssu);
        let nf = 1.0 / (ta / sse + tm / ssu);
        assert_relative_eq!(r.alternating_stress.pascals(), ta, max_relative = 1e-9);
        assert_relative_eq!(r.mean_stress.pascals(), tm, max_relative = 1e-9);
        assert_relative_eq!(r.goodman_factor_of_safety, nf, max_relative = 1e-9);
    }

    #[test]
    fn missing_endurance_degrades_gracefully() {
        let m = mat("Stainless 302");
        let err = analyze_fatigue(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            Force::from_newtons(10.0),
            Force::from_newtons(30.0),
        )
        .unwrap_err();
        assert!(matches!(err, crate::SpringError::NoFatigueData(_)));
    }

    #[test]
    fn rejects_reversed_force_order() {
        let m = mat("Music Wire");
        let err = analyze_fatigue(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            Force::from_newtons(30.0),
            Force::from_newtons(10.0),
        )
        .unwrap_err();
        assert!(matches!(err, crate::SpringError::InconsistentInputs(_)));
    }
}
