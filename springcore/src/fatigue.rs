//! Fatigue analysis for compression springs (Shigley §10-9). Uses cited
//! per-material endurance data (Zimmerli); materials without it degrade
//! gracefully by returning `NoFatigueData`.

use crate::design::validate_wire_mean_geometry;
use crate::material::Material;
use crate::mechanics::{corrected_shear_stress, spring_index};
use crate::units::{Force, Length, Stress};
use crate::{CurvatureCorrection, Result, SpringError};

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
    correction: CurvatureCorrection,
) -> Result<FatigueResult> {
    // Guard order mirrors torsion's `analyze_torsion_fatigue`: geometry → data
    // present → input domain → ordering → degenerate cycle → data trap →
    // compute → output finiteness.
    validate_wire_mean_geometry(wire_dia, mean_dia)?;
    let endurance = material
        .endurance
        .ok_or_else(|| SpringError::NoFatigueData(material.name.clone()))?;
    let lo = force_min.newtons();
    let hi = force_max.newtons();
    if !(lo.is_finite() && lo >= 0.0 && hi.is_finite() && hi >= 0.0) {
        return Err(SpringError::InconsistentInputs(
            "cycle forces must be finite and non-negative (the endurance data \
             covers unidirectional compressive loads)"
                .into(),
        ));
    }
    if hi < lo {
        return Err(SpringError::InconsistentInputs(
            "max cycle force must be at least the min cycle force".into(),
        ));
    }
    // Equal NONZERO forces are legal (τa = 0; Goodman's reciprocal form stays
    // finite) — the documented divergence from torsion's Gerber, which must
    // reject σa = 0. The both-zero pair, though, has no load cycle at all and
    // would produce nf = ∞; reject it precisely rather than letting the output
    // guard below mislabel zeros as "exceeding the representable range".
    if hi == 0.0 {
        return Err(SpringError::InconsistentInputs(
            "cycle forces must not both be zero (no load cycle to analyze)".into(),
        ));
    }

    let c = spring_index(mean_dia, wire_dia);
    let k = correction.factor(c);
    let fa = Force::from_newtons((hi - lo) / 2.0);
    let fm = Force::from_newtons((hi + lo) / 2.0);
    let tau_a = corrected_shear_stress(fa, mean_dia, wire_dia, k);
    let tau_m = corrected_shear_stress(fm, mean_dia, wire_dia, k);

    let sut = material.min_tensile_strength(wire_dia)?.pascals();
    let ssu = SHEAR_TO_TENSILE * sut;
    // Convert Zimmerli pulsating data to a fully-reversed endurance (Shigley Eq. 10-31):
    //   Sse = Ssa / (1 - Ssm/Ssu)
    // If Ssm ≥ Ssu the denominator is ≤ 0, producing ∞ or a negative Sse, which makes
    // the Goodman safety factor meaningless. Guard against this latent trap: with
    // bundled material data this should not occur (Ssm ≪ Ssu), but it would be a silent
    // trap for any future material whose endurance mean-stress meets/exceeds 0.67·Sut.
    if endurance.ssm.pascals() >= ssu {
        return Err(SpringError::InconsistentInputs(format!(
            "material '{}': endurance mean shear stress ({:.3} MPa) meets or exceeds \
             0.67·Sut = {:.3} MPa; cannot compute a valid fully-reversed endurance limit",
            material.name,
            endurance.ssm.pascals() / 1e6,
            ssu / 1e6,
        )));
    }
    let sse = endurance.ssa.pascals() / (1.0 - endurance.ssm.pascals() / ssu);
    // Goodman factor of safety: 1/nf = tau_a/Sse + tau_m/Ssu.
    let nf = 1.0 / (tau_a.pascals() / sse + tau_m.pascals() / ssu);

    // Belt-and-suspenders output guard (torsion's exact shape and message): a
    // finite-input overflow anywhere in the chain must never escape as Ok.
    if [tau_a.pascals(), tau_m.pascals(), sse, ssu, nf]
        .into_iter()
        .any(|v| !v.is_finite())
    {
        return Err(SpringError::InconsistentInputs(
            "fatigue analysis produced a non-finite result (inputs exceed the \
             representable range)"
                .into(),
        ));
    }

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
    use crate::units::{Force, Length};
    use approx::assert_relative_eq;
    use std::f64::consts::PI;

    #[test]
    fn goodman_safety_factor_music_wire() {
        let m = crate::test_support::music_wire();
        let r = analyze_fatigue(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            Force::from_newtons(10.0),
            Force::from_newtons(30.0),
            crate::CurvatureCorrection::Bergstrasser,
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
        let m = crate::test_support::material("Stainless 302");
        let err = analyze_fatigue(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            Force::from_newtons(10.0),
            Force::from_newtons(30.0),
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap_err();
        assert!(matches!(err, crate::SpringError::NoFatigueData(_)));
    }

    #[test]
    fn rejects_reversed_force_order() {
        let m = crate::test_support::music_wire();
        let err = analyze_fatigue(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            Force::from_newtons(30.0),
            Force::from_newtons(10.0),
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap_err();
        assert!(matches!(err, crate::SpringError::InconsistentInputs(_)));
    }

    #[test]
    fn analyze_fatigue_uses_selected_correction() {
        let m = crate::test_support::music_wire();
        let mk = |corr| {
            analyze_fatigue(
                &m,
                Length::from_millimeters(2.0),
                Length::from_millimeters(20.0),
                Force::from_newtons(10.0),
                Force::from_newtons(30.0),
                corr,
            )
            .unwrap()
            .goodman_factor_of_safety
        };
        // Wahl > Bergsträsser at C=10 → higher stress → lower factor of safety, so they differ.
        assert!(
            mk(crate::CurvatureCorrection::Wahl) < mk(crate::CurvatureCorrection::Bergstrasser)
        );
    }

    #[test]
    fn geometry_guard_rejects_zero_wire_before_bad_forces() {
        // Precedence: geometry first — the negative force must NOT be the error.
        let m = crate::test_support::music_wire();
        let err = analyze_fatigue(
            &m,
            Length::from_millimeters(0.0),
            Length::from_millimeters(20.0),
            Force::from_newtons(-5.0),
            Force::from_newtons(30.0),
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            crate::SpringError::InconsistentInputs(ref msg)
                if msg == "wire diameter must be a positive finite number"
        ));
    }

    #[test]
    fn no_data_beats_bad_forces() {
        // Precedence: data presence before input domain (torsion's order).
        let m = crate::test_support::material("Stainless 302");
        let err = analyze_fatigue(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            Force::from_newtons(-5.0),
            Force::from_newtons(30.0),
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap_err();
        assert!(matches!(err, crate::SpringError::NoFatigueData(_)));
    }

    #[test]
    fn rejects_negative_cycle_forces() {
        let m = crate::test_support::music_wire();
        let err = analyze_fatigue(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            Force::from_newtons(-5.0),
            Force::from_newtons(30.0),
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            crate::SpringError::InconsistentInputs(ref msg)
                if msg == "cycle forces must be finite and non-negative (the endurance data \
                           covers unidirectional compressive loads)"
        ));
    }

    #[test]
    fn rejects_non_finite_cycle_forces() {
        let m = crate::test_support::music_wire();
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let err = analyze_fatigue(
                &m,
                Length::from_millimeters(2.0),
                Length::from_millimeters(20.0),
                Force::from_newtons(bad),
                Force::from_newtons(30.0),
                crate::CurvatureCorrection::Bergstrasser,
            )
            .unwrap_err();
            assert!(matches!(
                err,
                crate::SpringError::InconsistentInputs(ref msg)
                    if msg.starts_with("cycle forces must be finite and non-negative")
            ));
        }
    }

    #[test]
    fn rejects_both_zero_cycle_forces() {
        // Both-zero previously returned Ok with nf = inf — the masquerade class
        // this guard kills. Equal NONZERO forces remain legal (see
        // `equal_forces_min_eq_max_is_accepted`).
        let m = crate::test_support::music_wire();
        let err = analyze_fatigue(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            Force::from_newtons(0.0),
            Force::from_newtons(0.0),
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            crate::SpringError::InconsistentInputs(ref msg)
                if msg == "cycle forces must not both be zero (no load cycle to analyze)"
        ));
    }

    #[test]
    fn huge_forces_trip_the_output_finiteness_guard() {
        // 1e305 N is finite and passes every input guard, but the corrected shear
        // stress overflows to inf — must surface as an error, never Ok(inf).
        let m = crate::test_support::music_wire();
        let err = analyze_fatigue(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            Force::from_newtons(0.0),
            Force::from_newtons(1e305),
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            crate::SpringError::InconsistentInputs(ref msg)
                if msg == "fatigue analysis produced a non-finite result (inputs exceed the \
                           representable range)"
        ));
    }

    // Pins the `<` (strict) in the ordering guard: equal NONZERO forces (zero
    // alternating load) must be accepted — the spring cycles at a single load
    // point, a degenerate but valid Goodman case (τa = 0 → nf = Ssu/τm, finite).
    // A `<=` mutant would reject this. The both-zero pair IS rejected — see
    // `rejects_both_zero_cycle_forces`.
    #[test]
    fn equal_forces_min_eq_max_is_accepted() {
        let m = crate::test_support::music_wire();
        let result = analyze_fatigue(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            Force::from_newtons(20.0),
            Force::from_newtons(20.0),
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(
            result.is_ok(),
            "equal forces should be accepted: {result:?}"
        );
        // Alternating stress is zero → τa = 0, nf = Ssu/τm (finite, not infinite).
        let r = result.unwrap();
        assert_relative_eq!(
            r.alternating_stress.pascals(),
            0.0,
            max_relative = 1e-9,
            epsilon = 1e-6
        );
    }
}
