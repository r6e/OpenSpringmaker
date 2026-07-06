//! Conical solve: inputs → complete linear-range design + status checks.
//! Formula sources cited at each site; see the module docs for the model's
//! scope and deliberate omissions.

use crate::design::{
    index_caution_labeled, load_point, DesignStatus, LoadPoint, Severity, StatusMessage,
};
use crate::end_type::EndType;
use crate::material::Material;
use crate::mechanics::spring_index;
use crate::units::{Force, Length, SpringRate, Stress};
use crate::{CurvatureCorrection, Result, SpringError};

/// Inputs for a round-wire conical compression spring (linear taper).
#[derive(Debug, Clone)]
pub struct ConicalInputs {
    pub wire_dia: Length,
    /// Mean diameter at the large end (the governing coil for stress).
    pub large_mean_dia: Length,
    /// Mean diameter at the small end. May equal `large_mean_dia`
    /// (zero taper — the cylindrical identity case).
    pub small_mean_dia: Length,
    pub active_coils: f64,
    pub free_length: Length,
    pub end_type: EndType,
}

/// A solved conical design (linear-range model).
#[derive(Debug, Clone)]
pub struct ConicalDesign {
    pub inputs: ConicalInputs,
    pub large_outer_dia: Length,
    pub large_inner_dia: Length,
    pub small_outer_dia: Length,
    pub small_inner_dia: Length,
    /// Local spring index at each end; the large end governs stress, the
    /// small end is the manufacturability floor. Both get index cautions.
    pub index_large: f64,
    pub index_small: f64,
    /// Diametral taper per active coil: (D_large − D_small) / Na.
    pub taper_per_coil: Length,
    pub total_coils: f64,
    /// Linear-range rate (Shigley Prob. 10-29, in diameters):
    /// k = G·d⁴ / (2·Na·(D_large + D_small)·(D_large² + D_small²)).
    pub rate: SpringRate,
    /// Conservative non-telescoping solid length (EndType formula,
    /// Shigley Table 10-1).
    pub solid_length: Length,
    /// True when the per-coil mean-radius step ≥ wire dia —
    /// (D_large − D_small)/(2·Na) ≥ d — i.e. coils nest and the true solid
    /// height is LOWER than `solid_length` (purely geometric condition).
    pub telescopes: bool,
    pub pitch: Length,
    pub min_tensile_strength: Stress,
    pub load_points: Vec<LoadPoint>,
    pub at_solid: LoadPoint,
}

/// Linear-range conical rate (Shigley 10th ed., Prob. 10-29).
///
/// Derivation sketch: a cylindrical turn of mean radius R has per-turn
/// compliance ∝ R³ (Eq. 10-9 with Na = 1). For the linear taper
/// R(t) = R_s + (R_l − R_s)·t over t ∈ [0, 1], Castigliano sums the per-turn
/// compliances: mean(R³) = ∫₀¹ R(t)³ dt = (R_l + R_s)(R_l² + R_s²)/4, giving
/// k = d⁴G / [16·Na·(R_l + R_s)(R_l² + R_s²)] — Prob. 10-29's stated result —
/// or in mean diameters (R = D/2):
/// k = G·d⁴ / (2·Na·(D_l + D_s)·(D_l² + D_s²)).
/// At D_l = D_s = D this reduces exactly to Eq. 10-9, k = G·d⁴/(8·D³·Na).
fn conical_rate(
    shear_modulus: Stress,
    wire_dia: Length,
    large: Length,
    small: Length,
    active: f64,
) -> SpringRate {
    let g = shear_modulus.pascals();
    let d = wire_dia.meters();
    let dl = large.meters();
    let ds = small.meters();
    SpringRate::from_newtons_per_meter(
        g * d.powi(4) / (2.0 * active * (dl + ds) * (dl * dl + ds * ds)),
    )
}

/// Compute a complete linear-range conical design from geometry plus loads.
pub fn solve_forward(
    material: &Material,
    inputs: &ConicalInputs,
    loads: &[Force],
    correction: CurvatureCorrection,
) -> Result<ConicalDesign> {
    let d = inputs.wire_dia.meters();
    if !(d.is_finite() && d > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "wire diameter must be a positive finite number".into(),
        ));
    }
    // End diameters: each finite-positive and exceeding the wire, small end
    // first, then the taper ordering. End-named messages — conical has two
    // mean diameters, so the shared unlabeled message would be ambiguous.
    let ds = inputs.small_mean_dia.meters();
    if !(ds.is_finite() && ds > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "small-end mean diameter must be a positive finite number".into(),
        ));
    }
    if ds <= d {
        return Err(SpringError::InconsistentInputs(
            "small-end mean diameter must exceed wire diameter (spring index must exceed 1)".into(),
        ));
    }
    let dl = inputs.large_mean_dia.meters();
    if !(dl.is_finite() && dl > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "large-end mean diameter must be a positive finite number".into(),
        ));
    }
    if dl <= d {
        return Err(SpringError::InconsistentInputs(
            "large-end mean diameter must exceed wire diameter (spring index must exceed 1)".into(),
        ));
    }
    // Equality is legal — zero taper is the cylindrical identity case.
    if dl < ds {
        return Err(SpringError::InconsistentInputs(
            "large-end mean diameter must be at least the small-end mean diameter".into(),
        ));
    }
    if !(inputs.active_coils.is_finite() && inputs.active_coils > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "active coils must be a positive finite number".into(),
        ));
    }
    let l0 = inputs.free_length.meters();
    if !(l0.is_finite() && l0 > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "free length must be a positive finite number".into(),
        ));
    }
    if loads
        .iter()
        .any(|f| !f.newtons().is_finite() || f.newtons() < 0.0)
    {
        return Err(SpringError::InconsistentInputs(
            "loads must be finite and non-negative".into(),
        ));
    }

    // Manufacturable-range check before geometry-derived checks (compression's
    // precedence: DiameterOutOfRange beats the solid-length guard).
    let mts = material.min_tensile_strength(inputs.wire_dia)?;

    let index_large = spring_index(inputs.large_mean_dia, inputs.wire_dia);
    let index_small = spring_index(inputs.small_mean_dia, inputs.wire_dia);
    let rate = conical_rate(
        material.shear_modulus,
        inputs.wire_dia,
        inputs.large_mean_dia,
        inputs.small_mean_dia,
        inputs.active_coils,
    );
    let total_coils = inputs.end_type.total_coils(inputs.active_coils);
    // Conservative non-telescoping solid stack (Shigley Table 10-1); when the
    // geometry telescopes the true solid height is lower — flagged, not modeled
    // (no cited telescoped-height formula in-house).
    let solid_length = inputs
        .end_type
        .solid_length(inputs.wire_dia, inputs.active_coils);
    if l0 < solid_length.meters() {
        return Err(SpringError::InconsistentInputs(
            "free length must be at least the solid length".into(),
        ));
    }
    let pitch = inputs.end_type.pitch_from_free_length(
        inputs.wire_dia,
        inputs.active_coils,
        inputs.free_length,
    );
    // Geometric nesting condition: per-coil mean-radius step ≥ wire diameter.
    let telescopes = (dl - ds) / (2.0 * inputs.active_coils) >= d;

    // Stress is governed by the LARGEST coil: the torsional moment F·R is
    // maximal at R_large (Prob. 10-29's premise), evaluated at the local index.
    let load_points: Vec<LoadPoint> = loads
        .iter()
        .map(|&f| {
            load_point(
                f,
                rate,
                inputs.free_length,
                inputs.large_mean_dia,
                inputs.wire_dia,
                index_large,
                mts,
                correction,
            )
        })
        .collect();
    let solid_force = Force::from_newtons(rate.newtons_per_meter() * (l0 - solid_length.meters()));
    let at_solid = load_point(
        solid_force,
        rate,
        inputs.free_length,
        inputs.large_mean_dia,
        inputs.wire_dia,
        index_large,
        mts,
        correction,
    );

    // Output-finiteness guard (the cross-family hardening standard): a
    // finite-input overflow anywhere in the chain must never escape as Ok.
    if [rate.newtons_per_meter(), at_solid.shear_stress.pascals()]
        .into_iter()
        .chain(
            load_points
                .iter()
                .flat_map(|lp| [lp.shear_stress.pascals(), lp.deflection.meters()]),
        )
        .any(|v| !v.is_finite())
    {
        return Err(SpringError::InconsistentInputs(
            "conical solve produced a non-finite result (inputs exceed the representable range)"
                .into(),
        ));
    }

    Ok(ConicalDesign {
        large_outer_dia: Length::from_meters(dl + d),
        large_inner_dia: Length::from_meters(dl - d),
        small_outer_dia: Length::from_meters(ds + d),
        small_inner_dia: Length::from_meters(ds - d),
        index_large,
        index_small,
        taper_per_coil: Length::from_meters((dl - ds) / inputs.active_coils),
        total_coils,
        rate,
        solid_length,
        telescopes,
        pitch,
        min_tensile_strength: mts,
        load_points,
        at_solid,
        inputs: inputs.clone(),
    })
}

/// Apply engineering checks to a computed conical design.
pub fn evaluate_status(design: &ConicalDesign, material: &Material) -> DesignStatus {
    let mut messages = Vec::new();

    // Both ends' local indices against the shared 4–12 band (SMI; Shigley §10-2).
    if let Some(msg) = index_caution_labeled("small-end spring index", design.index_small) {
        messages.push(msg);
    }
    if let Some(msg) = index_caution_labeled("large-end spring index", design.index_large) {
        messages.push(msg);
    }

    // Operating stress above the allowable fraction of MTS (SMI design stress),
    // evaluated at the governing large-end coil.
    let allowable = material.allowable_pct_torsion;
    for (i, lp) in design.load_points.iter().enumerate() {
        if lp.pct_mts > allowable {
            messages.push(StatusMessage {
                severity: Severity::Warning,
                message: format!(
                    "load point {} stress is {:.1}% of MTS, above the allowable {:.0}%",
                    i + 1,
                    lp.pct_mts * 100.0,
                    allowable * 100.0
                ),
            });
        }
    }

    // Stress at (conservative) solid above the set-allowable fraction (SMI).
    if design.at_solid.pct_mts > material.allowable_pct_set {
        messages.push(StatusMessage {
            severity: Severity::Warning,
            message: format!(
                "stress at solid is {:.1}% of MTS, above the set allowable {:.0}%",
                design.at_solid.pct_mts * 100.0,
                material.allowable_pct_set * 100.0
            ),
        });
    }

    if design.telescopes {
        messages.push(StatusMessage {
            severity: Severity::Info,
            message: "coils telescope (per-coil radial step ≥ wire diameter); the reported \
                      solid length is conservative — the true solid height is lower"
                .into(),
        });
    }

    DesignStatus { messages }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::end_type::EndType;
    use crate::mechanics::EndFixity;
    use crate::units::{Force, Length};
    use approx::assert_relative_eq;

    /// Tapered baseline: d=2mm, D_large=30mm (C_l=15… deliberately NOT — keep
    /// indices in-band: D_large=20mm (C_l=10), D_small=12mm (C_s=6), Na=10,
    /// L0=60mm, SquaredGround, load 10 N.
    fn inputs(large_mm: f64, small_mm: f64) -> ConicalInputs {
        ConicalInputs {
            wire_dia: Length::from_millimeters(2.0),
            large_mean_dia: Length::from_millimeters(large_mm),
            small_mean_dia: Length::from_millimeters(small_mm),
            active_coils: 10.0,
            free_length: Length::from_millimeters(60.0),
            end_type: EndType::SquaredGround,
        }
    }

    fn solve(large_mm: f64, small_mm: f64) -> crate::Result<ConicalDesign> {
        let m = crate::test_support::music_wire();
        solve_forward(
            &m,
            &inputs(large_mm, small_mm),
            &[Force::from_newtons(10.0)],
            crate::CurvatureCorrection::Bergstrasser,
        )
    }

    // ── The golden: zero taper reduces exactly to the cylindrical solver ────

    #[test]
    fn zero_taper_matches_cylindrical_solver() {
        let m = crate::test_support::music_wire();
        let conical = solve(20.0, 20.0).unwrap();
        let cyl = crate::design::solve_forward(
            &m,
            EndType::SquaredGround,
            EndFixity::FixedFixed, // fixity only affects buckling, which conical omits
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(60.0),
            &[Force::from_newtons(10.0)],
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert_relative_eq!(
            conical.rate.newtons_per_meter(),
            cyl.rate.newtons_per_meter(),
            max_relative = 1e-12
        );
        assert_relative_eq!(
            conical.load_points[0].shear_stress.pascals(),
            cyl.load_points[0].shear_stress.pascals(),
            max_relative = 1e-12
        );
        assert_relative_eq!(
            conical.load_points[0].deflection.meters(),
            cyl.load_points[0].deflection.meters(),
            max_relative = 1e-12
        );
        assert_relative_eq!(
            conical.solid_length.meters(),
            cyl.solid_length.meters(),
            max_relative = 1e-12
        );
        assert_relative_eq!(
            conical.at_solid.force.newtons(),
            cyl.at_solid.force.newtons(),
            max_relative = 1e-12
        );
        assert!(!conical.telescopes);
    }

    // ── Integral self-consistency: pins the taper algebra ───────────────────

    #[test]
    fn rate_matches_simpson_integration_of_r_cubed() {
        // Per-turn compliance ∝ R³; k = G·d⁴ / (64 · Na · mean(R³)) where
        // mean(R³) over the linear taper R(t) = R_s + (R_l − R_s)·t, t ∈ [0,1].
        // Closed form: mean(R³) = (R_l + R_s)(R_l² + R_s²)/4.
        // Simpson with 10_000 intervals nails the cubic exactly (Simpson is
        // exact for cubics; the fine grid just guards the arithmetic).
        let d: f64 = 0.002;
        let r_l: f64 = 0.010; // 20 mm large mean dia
        let r_s: f64 = 0.006; // 12 mm small mean dia
        let na = 10.0;
        let g = 80.0e9_f64; // any value; cancels in the comparison structure
        let n = 10_000usize;
        let h = 1.0 / n as f64;
        let f = |t: f64| (r_s + (r_l - r_s) * t).powi(3);
        let mut simpson = f(0.0) + f(1.0);
        for i in 1..n {
            simpson += f(i as f64 * h) * if i % 2 == 0 { 2.0 } else { 4.0 };
        }
        let mean_r3 = simpson * h / 3.0;
        let k_integral = g * d.powi(4) / (64.0 * na * mean_r3);

        let m = crate::test_support::music_wire();
        // music_wire G is NOT 80 GPa necessarily — recompute with the real G:
        let g_real = m.shear_modulus.pascals();
        let k_expected = g_real * d.powi(4) / (64.0 * na * mean_r3);
        let design = solve(20.0, 12.0).unwrap();
        assert_relative_eq!(
            design.rate.newtons_per_meter(),
            k_expected,
            max_relative = 1e-12
        );
        // And the two G-scaled forms agree structurally (guards the test itself).
        assert_relative_eq!(k_integral * g_real / g, k_expected, max_relative = 1e-12);
    }

    // ── Stress governed by the LARGE end (kills a large↔small swap) ─────────

    #[test]
    fn stress_is_computed_at_the_large_end() {
        let design = solve(20.0, 12.0).unwrap();
        let c_large = 20.0 / 2.0; // C_l = 10
        let k_b = crate::mechanics::bergstrasser_factor(c_large);
        let expected = crate::mechanics::corrected_shear_stress(
            Force::from_newtons(10.0),
            Length::from_millimeters(20.0),
            Length::from_millimeters(2.0),
            k_b,
        );
        assert_relative_eq!(
            design.load_points[0].shear_stress.pascals(),
            expected.pascals(),
            max_relative = 1e-12
        );
        // A small-end evaluation would differ by ~40% — assert inequality to
        // make the kill explicit.
        let k_b_small = crate::mechanics::bergstrasser_factor(12.0 / 2.0);
        let at_small = crate::mechanics::corrected_shear_stress(
            Force::from_newtons(10.0),
            Length::from_millimeters(12.0),
            Length::from_millimeters(2.0),
            k_b_small,
        );
        assert!((design.load_points[0].shear_stress.pascals() - at_small.pascals()).abs() > 1.0);
    }

    #[test]
    fn selected_correction_governs_stress() {
        let m = crate::test_support::music_wire();
        let mk = |corr| {
            solve_forward(&m, &inputs(20.0, 12.0), &[Force::from_newtons(30.0)], corr)
                .unwrap()
                .load_points[0]
                .shear_stress
                .pascals()
        };
        let wahl = mk(crate::CurvatureCorrection::Wahl);
        let berg = mk(crate::CurvatureCorrection::Bergstrasser);
        assert_relative_eq!(
            wahl / berg,
            crate::mechanics::wahl_factor(10.0) / crate::mechanics::bergstrasser_factor(10.0),
            max_relative = 1e-12
        );
    }

    // ── Telescoping boundary: (D_l − D_s)/(2·Na) ≥ d, ≥ semantics pinned ────

    #[test]
    fn telescoping_flag_boundary() {
        // d = 2 mm, Na = 10 → boundary at D_l − D_s = 2·Na·d = 40 mm.
        // Indices stay legal: D_s = 52 mm (C_s = 26 — caution, fine), D_l = 92.
        let exactly_at = solve(92.0, 52.0).unwrap();
        assert!(exactly_at.telescopes, "exactly-at boundary is ≥ → true");
        let just_below = solve(91.9, 52.0).unwrap();
        assert!(!just_below.telescopes, "just below the boundary → false");
        let above = solve(100.0, 52.0).unwrap();
        assert!(above.telescopes);
    }

    // ── Derived geometry ─────────────────────────────────────────────────────

    #[test]
    fn derived_diameters_indices_and_taper_are_exact() {
        let design = solve(20.0, 12.0).unwrap();
        assert_relative_eq!(
            design.large_outer_dia.millimeters(),
            22.0,
            max_relative = 1e-9
        );
        assert_relative_eq!(
            design.large_inner_dia.millimeters(),
            18.0,
            max_relative = 1e-9
        );
        assert_relative_eq!(
            design.small_outer_dia.millimeters(),
            14.0,
            max_relative = 1e-9
        );
        assert_relative_eq!(
            design.small_inner_dia.millimeters(),
            10.0,
            max_relative = 1e-9
        );
        assert_relative_eq!(design.index_large, 10.0, max_relative = 1e-12);
        assert_relative_eq!(design.index_small, 6.0, max_relative = 1e-12);
        // Diametral taper per active coil: (20 − 12)/10 = 0.8 mm.
        assert_relative_eq!(
            design.taper_per_coil.millimeters(),
            0.8,
            max_relative = 1e-9
        );
        // SquaredGround: Nt = Na + 2 = 12; Ls = d·Nt = 24 mm (conservative).
        assert_relative_eq!(design.total_coils, 12.0, max_relative = 1e-12);
        assert_relative_eq!(design.solid_length.millimeters(), 24.0, max_relative = 1e-9);
    }

    // ── Guard matrix: every message pinned; precedence pinned ───────────────

    fn msg(result: crate::Result<ConicalDesign>) -> String {
        match result {
            Err(crate::SpringError::InconsistentInputs(m)) => m,
            other => panic!("expected InconsistentInputs, got {other:?}"),
        }
    }

    #[test]
    fn guards_pin_messages_and_precedence() {
        let m = crate::test_support::music_wire();
        let base = inputs(20.0, 12.0);
        let loads = [Force::from_newtons(10.0)];
        let corr = crate::CurvatureCorrection::Bergstrasser;
        let run = |i: &ConicalInputs| solve_forward(&m, i, &loads, corr);

        // Wire guard first — even with every other input bad.
        let mut i = base.clone();
        i.wire_dia = Length::from_millimeters(0.0);
        i.small_mean_dia = Length::from_millimeters(-1.0);
        i.active_coils = -1.0;
        assert_eq!(
            msg(run(&i)),
            "wire diameter must be a positive finite number"
        );

        // Small end before large end before ordering.
        let mut i = base.clone();
        i.small_mean_dia = Length::from_millimeters(f64::NAN);
        i.large_mean_dia = Length::from_millimeters(-5.0);
        assert_eq!(
            msg(run(&i)),
            "small-end mean diameter must be a positive finite number"
        );
        let mut i = base.clone();
        i.small_mean_dia = Length::from_millimeters(1.0); // ≤ wire (2 mm)
        assert_eq!(
            msg(run(&i)),
            "small-end mean diameter must exceed wire diameter (spring index must exceed 1)"
        );
        let mut i = base.clone();
        i.large_mean_dia = Length::from_millimeters(f64::INFINITY);
        assert_eq!(
            msg(run(&i)),
            "large-end mean diameter must be a positive finite number"
        );
        let mut i = base.clone();
        i.large_mean_dia = Length::from_millimeters(1.5); // > wire? NO — 1.5 < 2
        assert_eq!(
            msg(run(&i)),
            "large-end mean diameter must exceed wire diameter (spring index must exceed 1)"
        );
        // Ordering: both ends individually valid, but large < small.
        let mut i = base.clone();
        i.large_mean_dia = Length::from_millimeters(12.0);
        i.small_mean_dia = Length::from_millimeters(20.0);
        assert_eq!(
            msg(run(&i)),
            "large-end mean diameter must be at least the small-end mean diameter"
        );

        // Active coils, free length, loads — compression's exact messages.
        let mut i = base.clone();
        i.active_coils = 0.0;
        assert_eq!(
            msg(run(&i)),
            "active coils must be a positive finite number"
        );
        let mut i = base.clone();
        i.free_length = Length::from_millimeters(0.0);
        assert_eq!(msg(run(&i)), "free length must be a positive finite number");
        let mut i = base.clone();
        i.free_length = Length::from_millimeters(20.0); // < Ls = 24 mm
        assert_eq!(
            msg(run(&i)),
            "free length must be at least the solid length"
        );
        let bad_loads = [Force::from_newtons(-5.0)];
        assert_eq!(
            msg(solve_forward(&m, &base, &bad_loads, corr)),
            "loads must be finite and non-negative"
        );
    }

    #[test]
    fn equal_end_diameters_are_accepted() {
        // Zero taper is the identity case — the ordering guard is ≥, not >.
        assert!(solve(20.0, 20.0).is_ok());
    }

    #[test]
    fn diameter_range_error_precedes_solid_length_guard() {
        // Mirrors compression's precedence test: an out-of-range wire diameter
        // surfaces as DiameterOutOfRange even when free < solid would also fail.
        let m = crate::test_support::music_wire();
        let i = ConicalInputs {
            wire_dia: Length::from_millimeters(10.0), // out of range for music wire
            large_mean_dia: Length::from_millimeters(80.0),
            small_mean_dia: Length::from_millimeters(60.0),
            active_coils: 10.0,
            free_length: Length::from_millimeters(50.0), // < Ls = 120 mm
            end_type: EndType::SquaredGround,
        };
        let result = solve_forward(
            &m,
            &i,
            &[Force::from_newtons(10.0)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(
            matches!(result, Err(crate::SpringError::DiameterOutOfRange { .. })),
            "got {result:?}"
        );
    }

    #[test]
    fn huge_finite_load_trips_the_output_guard() {
        let m = crate::test_support::music_wire();
        let result = solve_forward(
            &m,
            &inputs(20.0, 12.0),
            &[Force::from_newtons(1e305)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert_eq!(
            msg(result),
            "conical solve produced a non-finite result (inputs exceed the representable range)"
        );
    }

    // ── evaluate_status ──────────────────────────────────────────────────────

    fn has_message(status: &crate::design::DesignStatus, needle: &str) -> bool {
        status.messages.iter().any(|m| m.message.contains(needle))
    }

    #[test]
    fn end_labeled_index_cautions() {
        let m = crate::test_support::music_wire();
        // C_s = 6/2 = 3 (below 4 → caution), C_l = 20/2 = 10 (in band).
        let design = solve(20.0, 6.0).unwrap();
        let status = evaluate_status(&design, &m);
        assert!(has_message(&status, "small-end spring index"));
        assert!(!has_message(&status, "large-end spring index"));
        // Clean case: both in band → neither message.
        let clean = solve(20.0, 12.0).unwrap();
        let status = evaluate_status(&clean, &m);
        assert!(!has_message(&status, "spring index"));
    }

    #[test]
    fn overstress_and_solid_warnings_fire() {
        let m = crate::test_support::music_wire();
        // Small stiff spring, heavy load → overstress at the load point and at solid.
        let i = ConicalInputs {
            wire_dia: Length::from_millimeters(1.0),
            large_mean_dia: Length::from_millimeters(8.0),
            small_mean_dia: Length::from_millimeters(6.0),
            active_coils: 6.0,
            free_length: Length::from_millimeters(60.0),
            end_type: EndType::SquaredGround,
        };
        let design = solve_forward(
            &m,
            &i,
            &[Force::from_newtons(50.0)],
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        let status = evaluate_status(&design, &m);
        assert!(has_message(&status, "load point 1 stress"));
        assert!(has_message(&status, "stress at solid"));
        assert!(status.has_warnings());
    }

    #[test]
    fn telescoping_info_present_only_when_telescoping() {
        let m = crate::test_support::music_wire();
        let tele = solve(92.0, 52.0).unwrap();
        let status = evaluate_status(&tele, &m);
        assert!(has_message(&status, "coils telescope"));
        // And it is Info, not a warning.
        assert!(status
            .messages
            .iter()
            .any(|msg| msg.message.contains("coils telescope")
                && msg.severity == crate::design::Severity::Info));
        let flat = solve(20.0, 12.0).unwrap();
        let status = evaluate_status(&flat, &m);
        assert!(!has_message(&status, "coils telescope"));
    }
}
