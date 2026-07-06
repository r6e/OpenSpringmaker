//! Fatigue analysis for helical torsion springs (Shigley §10-12): the wire cycles
//! in BENDING, so the compression module's Sines/Zimmerli shear data does not
//! apply. Uses the Associated Spring R = 0 repeated-bending strengths (Table
//! 10-10, stored per material as fractions of Sut) with the GERBER criterion the
//! source prescribes: Se from Eq. 10-58, strength amplitude Sa from Eq. 10-59
//! along the load line r = Ma/Mm, and nf = Sa/σa (Eq. 10-60).

use crate::material::Material;
use crate::torsion::design::validate_wire_mean_geometry;
use crate::torsion::mechanics::bending_stress_inner;
use crate::units::{Length, Moment, Stress};
use crate::{Result, SpringError};

/// Cycle-life class for Table 10-10's two data columns.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CycleLife {
    /// 10⁵ cycles.
    HundredThousand,
    /// 10⁶ cycles (default — conservative, the worked example's column).
    #[default]
    Million,
}

/// Torsion-spring fatigue analysis result (Shigley §10-12, Gerber).
#[derive(Debug, Clone, Copy)]
pub struct TorFatigueResult {
    /// σa = K_bi·32·Ma/(π·d³) (Eq. 10-44 at the alternating moment).
    pub alternating_stress: Stress,
    /// σm at the mean moment.
    pub mean_stress: Stress,
    /// Fully-reversed endurance Se (Eq. 10-58, the Gerber R = 0 conversion of Sr).
    pub fully_reversed_endurance: Stress,
    /// Sut(d) — the Gerber ultimate (bending: TENSILE, unlike compression's shear).
    pub ultimate_tensile: Stress,
    /// Gerber strength amplitude Sa (Eq. 10-59, load line r = Ma/Mm).
    pub strength_amplitude: Stress,
    /// nf = Sa/σa (Eq. 10-60).
    pub gerber_factor_of_safety: f64,
}

/// Analyze fatigue for a torsion spring cycling between `moment_min` and
/// `moment_max` (both winding the coil tighter — the R = 0 data's domain).
pub fn analyze_torsion_fatigue(
    material: &Material,
    wire_dia: Length,
    mean_dia: Length,
    moment_min: Moment,
    moment_max: Moment,
    life: CycleLife,
) -> Result<TorFatigueResult> {
    // 1. Geometry first (error precedence; solve_forward's exact messages).
    validate_wire_mean_geometry(wire_dia, mean_dia)?;
    // 2. Data presence (compression's degradation path).
    let bf = material
        .bending_fatigue
        .ok_or_else(|| SpringError::NoFatigueData(material.name.clone()))?;
    // 3–5. The moment pair. Non-negative + finite (R = 0 domain), ordered, and
    // strictly differing: Gerber's nf = Sa/σa divides by σa (Eq. 10-60), so a zero
    // alternating moment must be a named error, not an ∞/NaN — unlike compression's
    // reciprocal Goodman form, which tolerates τa = 0.
    let (lo, hi) = (moment_min.newton_meters(), moment_max.newton_meters());
    if !(lo.is_finite() && lo >= 0.0 && hi.is_finite() && hi >= 0.0) {
        return Err(SpringError::InconsistentInputs(
            "cycle moments must be finite and non-negative \
             (the R = 0 bending data covers unidirectional winding loads)"
                .into(),
        ));
    }
    if hi < lo {
        return Err(SpringError::InconsistentInputs(
            "max cycle moment must be at least the min cycle moment".into(),
        ));
    }
    if hi == lo {
        return Err(SpringError::InconsistentInputs(
            "cycle moments must differ (a zero alternating moment has no fatigue amplitude)".into(),
        ));
    }

    let ma = Moment::from_newton_meters((hi - lo) / 2.0);
    let mm = Moment::from_newton_meters((hi + lo) / 2.0);
    // σ via the cited inner-fiber helper (Ki = Eq. 10-43 = kbi_factor; the source
    // prescribes Ki — no selectable correction in bending).
    let sigma_a = bending_stress_inner(ma, mean_dia, wire_dia);
    let sigma_m = bending_stress_inner(mm, mean_dia, wire_dia);

    let sut = material.min_tensile_strength(wire_dia)?;
    let sut_pa = sut.pascals();
    let pct = match life {
        CycleLife::HundredThousand => bf.sr_pct_1e5,
        CycleLife::Million => bf.sr_pct_1e6,
    };
    let sr = pct * sut_pa;
    // 6. Eq. 10-58's denominator 1 − (Sr/2/Sut)² is ≤ 0 iff Sr ≥ 2·Sut — impossible
    // for Table 10-10 fractions (≤ 0.64) but REACHABLE through user-overlay
    // materials with absurd fractions; a silent negative/∞ Se would poison nf.
    if sr / 2.0 >= sut_pa {
        return Err(SpringError::InconsistentInputs(
            "bending-fatigue strength must lie below twice the tensile strength \
             (Eq. 10-58's denominator would be non-positive)"
                .into(),
        ));
    }
    let se = (sr / 2.0) / (1.0 - (sr / (2.0 * sut_pa)).powi(2));
    // Load-line slope r = Ma/Mm (Mm > 0: guard 5 excluded the both-zero pair).
    let r = ma.newton_meters() / mm.newton_meters();
    // Eq. 10-59: Sa = (r²·Sut²)/(2·Se) · (−1 + √(1 + (2·Se/(r·Sut))²)).
    let sa = (r * r * sut_pa * sut_pa) / (2.0 * se)
        * (-1.0 + (1.0 + (2.0 * se / (r * sut_pa)).powi(2)).sqrt());
    let nf = sa / sigma_a.pascals();

    Ok(TorFatigueResult {
        alternating_stress: sigma_a,
        mean_stress: sigma_m,
        fully_reversed_endurance: Stress::from_pascals(se),
        ultimate_tensile: sut,
        strength_amplitude: Stress::from_pascals(sa),
        gerber_factor_of_safety: nf,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::music_wire;
    use crate::units::{Length, Moment};
    use approx::assert_relative_eq;

    /// Pa per psi (exact: 4.4482216152605 N/lbf ÷ 0.00064516 m²/in²).
    const PSI: f64 = 6894.757293168361;

    fn draft_from_material(m: &crate::material::Material) -> crate::material::MaterialDraft {
        m.to_draft()
    }

    fn golden(life: CycleLife) -> TorFatigueResult {
        // Shigley Example 10-8(c): music wire, d = 0.072 in, D = 0.5218 in,
        // M cycles 1 → 5 lbf·in.
        analyze_torsion_fatigue(
            &music_wire(),
            Length::from_inches(0.072),
            Length::from_inches(0.5218),
            Moment::from_pound_force_inches(1.0),
            Moment::from_pound_force_inches(5.0),
            life,
        )
        .expect("the worked example is feasible")
    }

    #[test]
    fn shigley_example_10_8c_golden() {
        let r = golden(CycleLife::Million);
        // Textbook-rounded chain at 5e-3 relative (the book rounds intermediates):
        assert_relative_eq!(
            r.alternating_stress.pascals() / PSI,
            60_857.0,
            max_relative = 5e-3
        );
        assert_relative_eq!(r.mean_stress.pascals() / PSI, 91_286.0, max_relative = 5e-3);
        assert_relative_eq!(
            r.fully_reversed_endurance.pascals() / PSI,
            78_510.0,
            max_relative = 5e-3
        );
        assert_relative_eq!(
            r.strength_amplitude.pascals() / PSI,
            68_850.0,
            max_relative = 5e-3
        );
        assert_relative_eq!(r.gerber_factor_of_safety, 1.13, max_relative = 5e-3);
        // Full-precision self-consistency (pins the algebra tighter than the
        // rounded oracle): nf ≡ Sa/σa; σm/σa ≡ Mm/Ma = 3/2.
        assert_relative_eq!(
            r.gerber_factor_of_safety,
            r.strength_amplitude.pascals() / r.alternating_stress.pascals(),
            max_relative = 1e-12
        );
        assert_relative_eq!(
            r.mean_stress.pascals() / r.alternating_stress.pascals(),
            1.5,
            max_relative = 1e-12
        );
    }

    #[test]
    fn hundred_thousand_life_gives_strictly_higher_margin() {
        // Sr fraction 0.53 vs 0.50 (Music Wire) → higher Se, Sa, nf at 10⁵. The
        // ratio Se(1e5)-vs-Se(1e6) pins BOTH columns (kills a column-swap mutant:
        // swapped columns would invert the inequality).
        let m6 = golden(CycleLife::Million);
        let m5 = golden(CycleLife::HundredThousand);
        assert!(m5.fully_reversed_endurance.pascals() > m6.fully_reversed_endurance.pascals());
        assert!(m5.strength_amplitude.pascals() > m6.strength_amplitude.pascals());
        assert!(m5.gerber_factor_of_safety > m6.gerber_factor_of_safety);
        // Stresses are life-independent.
        assert_relative_eq!(
            m5.alternating_stress.pascals(),
            m6.alternating_stress.pascals(),
            max_relative = 1e-12
        );
    }

    #[test]
    fn chrome_vanadium_column_is_used() {
        // 0.55/0.53 (Table 10-10 "A230 and A232" column): Sr at Million must be
        // exactly 0.53·Sut(d) — pins the per-material lookup, not just Music Wire's.
        let set = crate::MaterialSet::load_default();
        let cv = set.get("Chrome-Vanadium").unwrap();
        let d = Length::from_inches(0.072);
        let r = analyze_torsion_fatigue(
            cv,
            d,
            Length::from_inches(0.5218),
            Moment::from_pound_force_inches(1.0),
            Moment::from_pound_force_inches(5.0),
            CycleLife::Million,
        )
        .expect("feasible");
        let sut = cv.min_tensile_strength(d).unwrap().pascals();
        let sr = 0.53 * sut;
        let expected_se = (sr / 2.0) / (1.0 - (sr / 2.0 / sut).powi(2));
        assert_relative_eq!(
            r.fully_reversed_endurance.pascals(),
            expected_se,
            max_relative = 1e-12
        );
    }

    #[test]
    fn material_without_data_degrades_to_no_fatigue_data() {
        let set = crate::MaterialSet::load_default();
        let otw = set.get("Oil-Tempered Wire").unwrap(); // A229: deliberately data-less
        let err = analyze_torsion_fatigue(
            otw,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            Moment::from_newton_millimeters(100.0),
            Moment::from_newton_millimeters(500.0),
            CycleLife::Million,
        )
        .expect_err("no Table 10-10 grade match");
        match err {
            crate::SpringError::NoFatigueData(name) => assert_eq!(name, "Oil-Tempered Wire"),
            other => panic!("expected NoFatigueData, got {other:?}"),
        }
    }

    #[test]
    fn guards_fire_in_order_with_pinned_messages() {
        let m = music_wire();
        let (d, dm) = (
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
        );
        let mm = Moment::from_newton_millimeters;
        // Geometry precedence: wire = 0 beats bad moments.
        let err = analyze_torsion_fatigue(
            &m,
            Length::from_meters(0.0),
            dm,
            mm(-1.0),
            mm(-2.0),
            CycleLife::Million,
        )
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("wire diameter must be a positive finite number"));
        // Non-negative + finite (the R = 0 domain), covering NaN/Inf/negative:
        for (lo, hi) in [(-1.0, 500.0), (f64::NAN, 500.0), (100.0, f64::INFINITY)] {
            let err =
                analyze_torsion_fatigue(&m, d, dm, mm(lo), mm(hi), CycleLife::Million).unwrap_err();
            assert!(
                err.to_string().contains(
                    "cycle moments must be finite and non-negative \
                     (the R = 0 bending data covers unidirectional winding loads)"
                ),
                "({lo},{hi}): {err}"
            );
        }
        // Order: max ≥ min.
        let err = analyze_torsion_fatigue(&m, d, dm, mm(500.0), mm(100.0), CycleLife::Million)
            .unwrap_err();
        assert!(err
            .to_string()
            .contains("max cycle moment must be at least the min cycle moment"));
        // Equal (incl. both-zero) → the Gerber-amplitude guard.
        for v in [300.0, 0.0] {
            let err =
                analyze_torsion_fatigue(&m, d, dm, mm(v), mm(v), CycleLife::Million).unwrap_err();
            assert!(
                err.to_string().contains(
                    "cycle moments must differ (a zero alternating moment has no fatigue amplitude)"
                ),
                "v={v}: {err}"
            );
        }
    }

    #[test]
    fn absurd_user_material_fraction_trips_the_eq_10_58_trap() {
        // REACHABLE via user-overlay materials: an Sr fraction ≥ 2 makes Eq. 10-58's
        // denominator ≤ 0. Build such a material through the draft path and assert
        // the named trap instead of a silent negative/∞ Se.
        let set = crate::MaterialSet::load_default();
        let mut d = draft_from_material(set.get("Music Wire").unwrap());
        d.name = "Absurd".into();
        d.bending_fatigue = Some(crate::BendingFatigueDraft {
            sr_pct_1e5: 2.5,
            sr_pct_1e6: 2.5,
            peened: false,
        });
        let absurd = d.build().unwrap();
        let err = analyze_torsion_fatigue(
            &absurd,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            Moment::from_newton_millimeters(100.0),
            Moment::from_newton_millimeters(500.0),
            CycleLife::Million,
        )
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("bending-fatigue strength must lie below twice the tensile strength"),
            "{err}"
        );
    }
}
