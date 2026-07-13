//! Extension-spring-specific mechanics: hook curvature factors and stresses,
//! and initial-tension deflection. Body rate/stress reuse `crate::mechanics`.

use crate::extension::ends::HookEnds;
use crate::units::{Force, Length, SpringRate, Stress};
use std::f64::consts::PI;

/// Close-wound minimum free length: the shortest inside-hooks free length
/// the spring can physically be wound to — the close-wound BODY plus both
/// hook allowances. Composed from the two cited Shigley relations rather
/// than a new formula: the solver's `active` is the RATE-equivalent count
/// `Na = Nb + G/E` (Eq. 10-40 — ordinary twisted end loops add ~G/E
/// equivalent turns of hook compliance), so the physical body coils are
/// `Nb = active − G/E`, and the close-wound inside-hooks length of that
/// body is [`free_length_from_geometry`] at `Nb` (Eq. 10-39 generalized to
/// the loop diameter). Using `active` directly would OVER-state the
/// minimum by `(G/E)·d` and falsely reject textbook designs — Shigley's
/// own Example 10-6 (`Na = 12.574`, `L0 = 0.817 in` computed from
/// `Nb = 12.17`) sits inside that band.
///
/// Errors when the derived quantities are nonphysical (R2 input-domain F1;
/// guards cover every derived value): a hostile-but-validated modulus ratio
/// (e.g. E = 0.001 GPa → G/E = 80 000, or a modulus that overflowed to
/// +Inf Pa in the GPa→Pa conversion) drives `Nb` non-positive, NaN, or
/// ±Inf — the old unguarded value made the minimum hugely negative, so the
/// caller's `<` reject NEVER fired and impossible free lengths solved.
/// `Nb` must be a positive finite count and the composed minimum finite.
pub fn min_free_length(
    wire_dia: Length,
    active: f64,
    hooks: HookEnds,
    shear_modulus: Stress,
    youngs_modulus: Stress,
) -> crate::Result<Length> {
    let body_coils = active - shear_modulus.pascals() / youngs_modulus.pascals();
    if !(body_coils.is_finite() && body_coils > 0.0) {
        return Err(crate::SpringError::InconsistentInputs(
            "computed body coil count (active coils minus the G/E hook-compliance \
             turns) is not a positive finite number — increase the active coil \
             count, or check the shear and Young's moduli"
                .into(),
        ));
    }
    let min = free_length_from_geometry(wire_dia, body_coils, hooks);
    if !min.meters().is_finite() {
        return Err(crate::SpringError::InconsistentInputs(
            "computed close-wound minimum free length is not finite \
             (check wire diameter and hook bend radius r1)"
                .into(),
        ));
    }
    Ok(min)
}

/// Hook bending curvature factor at point A (Shigley, extension springs):
/// (K)_A = (4·C1² − C1 − 1) / (4·C1·(C1 − 1)), with C1 = 2·r1/d.
///
/// Precondition: `c1` must exceed 1.0. The factor diverges (denominator → 0)
/// as C1 → 1. The caller is responsible for ensuring C1 > 1 before invoking.
pub fn hook_bending_factor(c1: f64) -> f64 {
    (4.0 * c1 * c1 - c1 - 1.0) / (4.0 * c1 * (c1 - 1.0))
}

/// Hook torsion curvature factor at point B (Shigley, extension springs):
/// (K)_B = (4·C2 − 1) / (4·C2 − 4), with C2 = 2·r2/d.
///
/// Precondition: `c2` must exceed 1.0. The factor diverges (denominator → 0)
/// as C2 → 1. The caller is responsible for ensuring C2 > 1 before invoking.
pub fn hook_torsion_factor(c2: f64) -> f64 {
    (4.0 * c2 - 1.0) / (4.0 * c2 - 4.0)
}

/// Hook bending stress at point A (Shigley): σ_A = F[(K)_A·16D/(πd³) + 4/(πd²)].
pub fn hook_bending_stress(force: Force, mean_dia: Length, wire_dia: Length, r1: Length) -> Stress {
    let (f, d, dia) = (force.newtons(), wire_dia.meters(), mean_dia.meters());
    let c1 = 2.0 * r1.meters() / d;
    let ka = hook_bending_factor(c1);
    let sigma = f * (ka * 16.0 * dia / (PI * d.powi(3)) + 4.0 / (PI * d * d));
    Stress::from_pascals(sigma)
}

/// Extension deflection at a load: y = max(0, F − F_i)/k. Coils stay closed
/// (no deflection) until the force exceeds the built-in initial tension (Shigley).
///
/// Precondition: `force`, `initial_tension`, and `rate` are finite (and rate > 0).
/// `solve_forward` enforces this at the family boundary. The precondition — not the
/// clamp — is what guarantees correctness: were `net` ever NaN, `net.max(0.0)` would
/// silently yield 0.0 (Rust's `f64::max` returns the non-NaN operand), masking it.
/// Because `net` is always finite here, the `max(0.0)` only ever rounds a valid
/// negative net up to zero. Do not call with non-finite arguments.
pub fn deflection(force: Force, initial_tension: Force, rate: SpringRate) -> Length {
    let net = force.newtons() - initial_tension.newtons();
    Length::from_meters(net.max(0.0) / rate.newtons_per_meter())
}

/// Extension-spring free length from geometry (Shigley extension free-length
/// relation, generalized to the hook-loop diameter). The end loop is modeled
/// by its mean diameter `d_loop = 2·r1` (default hook `r1 = D/2` ⇒ `d_loop = D`),
/// so `L₀ = 2·(d_loop − d) + (N + 1)·d`. `r2` governs torsion only and is not
/// used.
///
/// The `active` parameter is the coil count `N` in the `(N + 1)·d` body term;
/// which count to pass depends on the purpose (Shigley Eq. 10-39/10-40):
/// - pass the solver's rate-equivalent active turns `Na` for the
///   RATE-EQUIVALENT close-wound length (the renderer's
///   `viz::sdf::extension_body_pitch_mm` does this: it draws `Na` body turns);
/// - pass the PHYSICAL body turns `Nb = Na − G/E` for the physical close-wound
///   MINIMUM (`min_free_length` does this — un-folding Eq. 10-40's
///   hook-compliance turns, so the minimum is not over-stated by `(G/E)·d` and
///   Shigley's own Example 10-6 is not falsely rejected).
///
/// So this fn does NOT itself assume `Nb = Na`; the caller chooses.
pub fn free_length_from_geometry(wire_dia: Length, active: f64, hooks: HookEnds) -> Length {
    let d = wire_dia.meters();
    let d_loop = 2.0 * hooks.r1.meters();
    Length::from_meters(2.0 * (d_loop - d) + (active + 1.0) * d)
}

/// Hook torsional stress at point B (Shigley): τ_B = (K)_B·8FD/(πd³).
pub fn hook_torsion_stress(force: Force, mean_dia: Length, wire_dia: Length, r2: Length) -> Stress {
    let (f, d, dia) = (force.newtons(), wire_dia.meters(), mean_dia.meters());
    let c2 = 2.0 * r2.meters() / d;
    let kb = hook_torsion_factor(c2);
    Stress::from_pascals(kb * 8.0 * f * dia / (PI * d.powi(3)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::SpringRate;
    use approx::assert_relative_eq;

    #[test]
    fn hook_bending_factor_c1_10() {
        // (4·100 − 10 − 1)/(4·10·9) = 389/360.
        assert_relative_eq!(
            hook_bending_factor(10.0),
            389.0 / 360.0,
            max_relative = 1e-12
        );
    }

    #[test]
    fn hook_torsion_factor_c2_5() {
        // (20 − 1)/(20 − 4) = 19/16.
        assert_relative_eq!(hook_torsion_factor(5.0), 19.0 / 16.0, max_relative = 1e-12);
    }

    #[test]
    fn hook_bending_stress_matches_hand_calc() {
        let s = hook_bending_stress(
            Force::from_newtons(100.0),
            Length::from_millimeters(20.0),
            Length::from_millimeters(2.0),
            Length::from_millimeters(10.0),
        );
        // σ_A = 100·[ (389/360)·16·0.02/(π·0.002³) + 4/(π·0.002²) ] ≈ 1.4076e9 Pa.
        assert_relative_eq!(s.pascals(), 1.40765e9, max_relative = 1e-4);
    }

    #[test]
    fn hook_torsion_stress_matches_hand_calc() {
        let s = hook_torsion_stress(
            Force::from_newtons(100.0),
            Length::from_millimeters(20.0),
            Length::from_millimeters(2.0),
            Length::from_millimeters(5.0),
        );
        // τ_B = (19/16)·8·100·0.02/(π·0.002³) ≈ 7.560e8 Pa.
        assert_relative_eq!(s.pascals(), 7.5599e8, max_relative = 1e-4);
    }

    #[test]
    fn deflection_zero_below_initial_tension() {
        let y = deflection(
            Force::from_newtons(5.0),
            Force::from_newtons(10.0),
            SpringRate::from_newtons_per_meter(2000.0),
        );
        assert_relative_eq!(y.meters(), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn deflection_above_initial_tension() {
        // (30 − 10)/2000 = 0.01 m = 10 mm.
        let y = deflection(
            Force::from_newtons(30.0),
            Force::from_newtons(10.0),
            SpringRate::from_newtons_per_meter(2000.0),
        );
        assert_relative_eq!(y.millimeters(), 10.0, max_relative = 1e-9);
    }

    #[test]
    fn free_length_default_hook_matches_shigley_form() {
        // Default hook: r1 = D/2 ⇒ d_loop = D. D=20mm, d=2mm, Na=10.
        // L0 = 2(D − d) + (Na + 1)d = 2(18mm) + 11·2mm = 36 + 22 = 58 mm.
        let hooks = HookEnds::default_for(Length::from_millimeters(20.0));
        let l0 = free_length_from_geometry(Length::from_millimeters(2.0), 10.0, hooks);
        assert_relative_eq!(l0.millimeters(), 58.0, max_relative = 1e-12);
    }

    #[test]
    fn free_length_fixed_hook_uses_loop_diameter() {
        // Fixed hook r1 = 6mm ⇒ d_loop = 12mm. d=2mm, Na=10.
        // L0 = 2(12 − 2) + 11·2 = 20 + 22 = 42 mm. (r2 does not affect length.)
        let hooks = HookEnds {
            r1: Length::from_millimeters(6.0),
            r2: Length::from_millimeters(3.0),
        };
        let l0 = free_length_from_geometry(Length::from_millimeters(2.0), 10.0, hooks);
        assert_relative_eq!(l0.millimeters(), 42.0, max_relative = 1e-12);
    }

    #[test]
    fn min_free_length_subtracts_hook_compliance_turns() {
        // d=2mm, r1=10mm, Na=10, G=80 GPa, E=203.4 GPa (Music Wire values):
        // Nb = 10 − 80/203.4 = 9.60668633235005 body coils (Eq. 10-40), so
        // L0_min = 2·(20 − 2) + (Nb + 1)·2 = 57.2133726647001 mm — hand
        // literal, NOT recomputed through the function under test.
        let hooks = HookEnds {
            r1: Length::from_millimeters(10.0),
            r2: Length::from_millimeters(5.0),
        };
        let min = min_free_length(
            Length::from_millimeters(2.0),
            10.0,
            hooks,
            crate::units::Stress::from_pascals(80.0e9),
            crate::units::Stress::from_pascals(203.4e9),
        )
        .unwrap();
        assert_relative_eq!(min.millimeters(), 57.2133726647001, max_relative = 1e-12);
    }

    fn r1_10mm_hooks() -> HookEnds {
        HookEnds {
            r1: Length::from_millimeters(10.0),
            r2: Length::from_millimeters(5.0),
        }
    }

    /// R2 input-domain F1: a hostile modulus ratio (G/E ≥ active) drives the
    /// body coil count non-positive; the old unguarded value made the
    /// minimum hugely negative and neutralized the solver's reject. Exactly
    /// zero body coils (active == G/E) is nonphysical too — pins `>`, not
    /// `>=`.
    #[test]
    fn min_free_length_rejects_non_positive_body_coils() {
        // G/E = 80 000 ≫ active = 10.
        let r = min_free_length(
            Length::from_millimeters(2.0),
            10.0,
            r1_10mm_hooks(),
            crate::units::Stress::from_pascals(80.0e9),
            crate::units::Stress::from_pascals(1.0e6), // E = 0.001 GPa
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
        // Boundary: G/E == active exactly → Nb == 0 → still nonphysical.
        let r = min_free_length(
            Length::from_millimeters(2.0),
            10.0,
            r1_10mm_hooks(),
            crate::units::Stress::from_pascals(80.0e9),
            crate::units::Stress::from_pascals(8.0e9), // G/E = 10 = active
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    /// R3 stateful-UI F1: for a low active-coil count (active ∈ (0, G/E) with
    /// normal steel) the body-coil guard fires because the ACTIVE COUNT — not
    /// the material — sits below the G/E hook-compliance turns. The message
    /// must name the active-coil cause (and its remedy) rather than misdirect
    /// the user to the (valid) material. Music Wire G/E ≈ 80/203.4 ≈ 0.393;
    /// active = 0.2 < G/E ⇒ Nb = 0.2 − 0.393 < 0.
    #[test]
    fn min_free_length_message_names_active_count_not_only_the_material() {
        let err = min_free_length(
            Length::from_millimeters(2.0),
            0.2,
            r1_10mm_hooks(),
            crate::units::Stress::from_pascals(80.0e9),
            crate::units::Stress::from_pascals(203.4e9),
        )
        .unwrap_err();
        let crate::SpringError::InconsistentInputs(msg) = err else {
            panic!("expected InconsistentInputs, got {err:?}");
        };
        // Leads with the active-coil cause and its remedy — the misdirection fix.
        assert!(
            msg.contains("increase the active coil count"),
            "message must name the active-coil cause; got: {msg}"
        );
        // Still names the moduli as the OTHER reachable cause, without
        // asserting the material is at fault.
        assert!(
            msg.contains("shear and Young's moduli"),
            "message must still name the moduli as a possible cause; got: {msg}"
        );
    }

    /// R2 input-domain F1 twin: a modulus that overflowed to +Inf Pa in the
    /// GPa→Pa conversion (validated finite in GPa) makes G/E = +Inf
    /// (Nb = −Inf) or, with both infinite, NaN — neither may pass the
    /// body-coil guard (`<` comparisons are false for NaN).
    #[test]
    fn min_free_length_rejects_non_finite_body_coils() {
        for (g, e) in [(f64::INFINITY, 203.4e9), (f64::INFINITY, f64::INFINITY)] {
            let r = min_free_length(
                Length::from_millimeters(2.0),
                10.0,
                r1_10mm_hooks(),
                crate::units::Stress::from_pascals(g),
                crate::units::Stress::from_pascals(e),
            );
            assert!(
                matches!(r, Err(crate::SpringError::InconsistentInputs(_))),
                "G={g}, E={e} must reject"
            );
        }
    }

    /// Output-side guard (guards cover EVERY derived value): a positive
    /// finite body coil count can still compose a non-finite minimum when
    /// the geometry itself overflows (2·(2·r1 − d) alone exceeds f64::MAX).
    #[test]
    fn min_free_length_rejects_non_finite_composed_minimum() {
        let hooks = HookEnds {
            r1: Length::from_meters(1.0e308),
            r2: Length::from_meters(5.0),
        };
        let r = min_free_length(
            Length::from_meters(1.0e308),
            10.0,
            hooks,
            crate::units::Stress::from_pascals(80.0e9),
            crate::units::Stress::from_pascals(203.4e9),
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }
}
