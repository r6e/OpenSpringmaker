//! Rectangular-wire solve: inputs → complete design + status checks.
//! Formula sources cited at each site; see `mod.rs` for the model's scope and
//! deliberate omissions.

use crate::design::{index_caution_labeled, DesignStatus, LoadPoint, Severity, StatusMessage};
use crate::end_type::EndType;
use crate::material::Material;
use crate::mechanics::spring_index;
use crate::units::{Force, Length, SpringRate, Stress};
use crate::{CurvatureCorrection, Result, SpringError};

/// Inputs for a rectangular- (or square-) wire helical compression spring.
#[derive(Debug, Clone)]
pub struct RectangularInputs {
    /// Wire dimension along the coil axis. Sets solid length (n_total × axial).
    pub wire_axial: Length,
    /// Wire dimension in the radial direction. Sets OD (D + radial) / ID (D − radial).
    pub wire_radial: Length,
    pub mean_dia: Length,
    pub active_coils: f64,
    pub free_length: Length,
    pub end_type: EndType,
}

/// A solved rectangular-wire design (torsion-of-rectangular-bar model).
#[derive(Debug, Clone)]
pub struct RectangularDesign {
    pub inputs: RectangularInputs,
    pub outer_dia: Length,
    pub inner_dia: Length,
    /// b/c = max(axial, radial) / min(axial, radial) ≥ 1.
    pub aspect_ratio: f64,
    /// Interpolated Shigley §3-14 coefficients at this aspect ratio.
    pub alpha: f64,
    pub beta: f64,
    /// Spring index C = D / b (b = larger wire side); the stress-governing index.
    pub index: f64,
    /// k = 4·β·b·c³·G / (π·D³·n).
    pub rate: SpringRate,
    /// Solid length = n_total × wire_axial.
    pub solid_length: Length,
    pub total_coils: f64,
    pub pitch: Length,
    /// True when b/c exceeds 10 and the coefficients are clamped (Info status).
    pub aspect_clamped: bool,
    pub min_tensile_strength: Stress,
    pub load_points: Vec<LoadPoint>,
    pub at_solid: LoadPoint,
}

/// The largest tabulated side ratio (the final `RECT_TORSION_TABLE` row's
/// ratio). Above it the coefficients are clamped to that row (and
/// `solve_forward` flags `aspect_clamped`) — a single constant so the clamp
/// boundary and the table's extent cannot decouple.
const MAX_TABULATED_ASPECT: f64 = 10.0;

/// Shigley 10th ed. §3-14 torsion-of-rectangular-bar coefficients vs the side
/// ratio b/c (b = longer side). α governs max shear (Eq. 3-40, τ₀ = T/(α·b·c²));
/// β governs angle of twist (Eq. 3-41, θ = T·l/(β·b·c³·G)). Footnoted in Shigley
/// to Timoshenko, *Strength of Materials*, Part I, 3rd ed. (1955), p. 290.
/// The b/c → ∞ limit is α = β = 1/3 (documented asymptote, not a table row).
///
/// Every row below was verified cell-by-cell against the printed table (both the
/// α and β rows, all ten b/c columns plus the ∞ = 1/3 column). This transcription
/// is the load-bearing constant for all non-square results — the square case
/// (b/c = 1) is independently anchored to the AF manual, but rows b/c > 1 inherit
/// correctness from the formula structure *and* these numbers, so they are pinned
/// against the source here, not merely against the code (which mutation testing
/// alone cannot validate).
const RECT_TORSION_TABLE: &[(f64, f64, f64)] = &[
    (1.00, 0.208, 0.141),
    (1.50, 0.231, 0.196),
    (1.75, 0.239, 0.214),
    (2.00, 0.246, 0.228),
    (2.50, 0.258, 0.249),
    (3.00, 0.267, 0.263),
    (4.00, 0.282, 0.281),
    (6.00, 0.299, 0.299),
    (8.00, 0.307, 0.307),
    (MAX_TABULATED_ASPECT, 0.313, 0.313),
];

/// Linearly interpolate (α, β) at side ratio `aspect` (b/c ≥ 1). Clamps to the
/// first row at/below 1.0 (unreachable — orientation guarantees b/c ≥ 1 — but
/// defensive) and to the last row at/above 10.0 (conservative: α, β < 1/3 ⇒
/// higher stress, lower rate; the caller flags `aspect_clamped`).
fn rect_torsion_coeffs(aspect: f64) -> (f64, f64) {
    let first = RECT_TORSION_TABLE[0];
    let last = RECT_TORSION_TABLE[RECT_TORSION_TABLE.len() - 1];
    if aspect <= first.0 {
        return (first.1, first.2);
    }
    if aspect >= last.0 {
        return (last.1, last.2);
    }
    let hi = RECT_TORSION_TABLE
        .iter()
        .position(|&(r, _, _)| r >= aspect)
        .unwrap();
    let (r0, a0, b0) = RECT_TORSION_TABLE[hi - 1];
    let (r1, a1, b1) = RECT_TORSION_TABLE[hi];
    let t = (aspect - r0) / (r1 - r0);
    (a0 + t * (a1 - a0), b0 + t * (b1 - b0))
}

/// Rectangular-wire helical rate. Shigley §3-14 Eq. 3-41 (angle of twist of a
/// rectangular bar, θ = T·l/(β·b·c³·G)) assembled with close-coiled helix
/// geometry: torque T = P·D/2, wire length l = π·D·n, axial deflection δ = θ·D/2
/// ⟹ **k = 4·β·b·c³·G / (π·D³·n)**. For a square section (b = c = a, β = 0.141)
/// this equals the AF Stress Manual §1.5.4.2 Eq. 1-90 rate k = G·a⁴/(44.5·r³·n)
/// (2π/0.141 = 44.56 ≈ 44.5).
fn rectangular_rate(
    shear_modulus: Stress,
    b: Length,
    c: Length,
    mean_dia: Length,
    beta: f64,
    active: f64,
) -> SpringRate {
    let g = shear_modulus.pascals();
    let bb = b.meters();
    let cc = c.meters();
    let dm = mean_dia.meters();
    SpringRate::from_newtons_per_meter(
        4.0 * beta * bb * cc.powi(3) * g / (std::f64::consts::PI * dm.powi(3) * active),
    )
}

/// Rectangular-wire corrected max shear stress. Straight-bar torsion
/// (Shigley §3-14 Eq. 3-40, τ₀ = T/(α·b·c²), T = F·D/2) times the selectable
/// curvature correction K(C), C = D/b. Square (b = c, α = 0.208): reduces to
/// AF §1.5.4.2 Eq. 1-84, K·4.80·F·r/b³ (1/0.208 = 4.808 ≈ 4.80).
fn rect_corrected_shear_stress(
    force: Force,
    mean_dia: Length,
    b: Length,
    c: Length,
    alpha: f64,
    factor: f64,
) -> Stress {
    let f = force.newtons();
    let dm = mean_dia.meters();
    let bb = b.meters();
    let cc = c.meters();
    Stress::from_pascals(factor * f * dm / (2.0 * alpha * bb * cc * cc))
}

/// A rectangular load point (mirrors `crate::design::load_point`, swapping the
/// round-wire stress for the rectangular one). Deflection y = F/k.
#[allow(clippy::too_many_arguments)]
fn rect_load_point(
    force: Force,
    rate: SpringRate,
    free_length: Length,
    mean_dia: Length,
    b: Length,
    c: Length,
    alpha: f64,
    index: f64,
    mts: Stress,
    correction: CurvatureCorrection,
) -> LoadPoint {
    let y = force.newtons() / rate.newtons_per_meter();
    let stress =
        rect_corrected_shear_stress(force, mean_dia, b, c, alpha, correction.factor(index));
    LoadPoint {
        force,
        deflection: Length::from_meters(y),
        length: Length::from_meters(free_length.meters() - y),
        shear_stress: stress,
        pct_mts: stress.pascals() / mts.pascals(),
    }
}

/// Compute a complete rectangular-wire design from geometry plus loads.
pub fn solve_forward(
    material: &Material,
    inputs: &RectangularInputs,
    loads: &[Force],
    correction: CurvatureCorrection,
) -> Result<RectangularDesign> {
    let axial = inputs.wire_axial.meters();
    if !(axial.is_finite() && axial > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "wire axial dimension must be a positive finite number".into(),
        ));
    }
    let radial = inputs.wire_radial.meters();
    if !(radial.is_finite() && radial > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "wire radial dimension must be a positive finite number".into(),
        ));
    }
    // Orient the section: b = longer side, c = shorter (Shigley's convention).
    let (b_len, c_len) = if axial >= radial {
        (inputs.wire_axial, inputs.wire_radial)
    } else {
        (inputs.wire_radial, inputs.wire_axial)
    };
    let b = b_len.meters();
    let c = c_len.meters();

    let dm = inputs.mean_dia.meters();
    if !(dm.is_finite() && dm > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "mean diameter must be a positive finite number".into(),
        ));
    }
    // The single mean > b guard secures both the spring index (C = D/b > 1) and
    // a positive inner diameter (D − radial > 0, since b ≥ radial).
    if dm <= b {
        return Err(SpringError::InconsistentInputs(
            "mean diameter must exceed the larger wire dimension (spring index must exceed 1)"
                .into(),
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

    // Manufacturable-range check on the larger side, before geometry-derived
    // checks (compression's precedence: DiameterOutOfRange beats free<solid).
    let mts = material.min_tensile_strength(b_len)?;

    let aspect = b / c;
    let (alpha, beta) = rect_torsion_coeffs(aspect);
    let aspect_clamped = aspect > MAX_TABULATED_ASPECT;
    let index = spring_index(inputs.mean_dia, b_len);
    let rate = rectangular_rate(
        material.shear_modulus,
        b_len,
        c_len,
        inputs.mean_dia,
        beta,
        inputs.active_coils,
    );
    let total_coils = inputs.end_type.total_coils(inputs.active_coils);
    // Solid length stacks the AXIAL wire dimension (Shigley Table 10-1 form).
    let solid_length = inputs
        .end_type
        .solid_length(inputs.wire_axial, inputs.active_coils);
    if l0 < solid_length.meters() {
        return Err(SpringError::InconsistentInputs(
            "free length must be at least the solid length".into(),
        ));
    }
    let pitch = inputs.end_type.pitch_from_free_length(
        inputs.wire_axial,
        inputs.active_coils,
        inputs.free_length,
    );

    let load_points: Vec<LoadPoint> = loads
        .iter()
        .map(|&f| {
            rect_load_point(
                f,
                rate,
                inputs.free_length,
                inputs.mean_dia,
                b_len,
                c_len,
                alpha,
                index,
                mts,
                correction,
            )
        })
        .collect();
    let solid_force = Force::from_newtons(rate.newtons_per_meter() * (l0 - solid_length.meters()));
    let at_solid = rect_load_point(
        solid_force,
        rate,
        inputs.free_length,
        inputs.mean_dia,
        b_len,
        c_len,
        alpha,
        index,
        mts,
        correction,
    );

    // Output-finiteness guard (the cross-family hardening standard): a
    // finite-input overflow anywhere in the chain must never escape as Ok.
    // `at_solid.deflection` is included: with no load points the per-load chain
    // is vacuous, and a diameter overflow makes rate → 0 and
    // at_solid.deflection = 0/0 = NaN — it must be caught here.
    if [
        rate.newtons_per_meter(),
        at_solid.shear_stress.pascals(),
        at_solid.deflection.meters(),
    ]
    .into_iter()
    .chain(
        load_points
            .iter()
            .flat_map(|lp| [lp.shear_stress.pascals(), lp.deflection.meters()]),
    )
    .any(|v| !v.is_finite())
    {
        return Err(SpringError::InconsistentInputs(
            "rectangular solve produced a non-finite result (inputs exceed the representable range)"
                .into(),
        ));
    }

    Ok(RectangularDesign {
        outer_dia: Length::from_meters(dm + radial),
        inner_dia: Length::from_meters(dm - radial),
        aspect_ratio: aspect,
        alpha,
        beta,
        index,
        rate,
        solid_length,
        total_coils,
        pitch,
        aspect_clamped,
        min_tensile_strength: mts,
        load_points,
        at_solid,
        inputs: inputs.clone(),
    })
}

/// Apply engineering checks to a computed rectangular-wire design.
pub fn evaluate_status(design: &RectangularDesign, material: &Material) -> DesignStatus {
    let mut messages = Vec::new();

    // Spring index against the shared 4–12 band (SMI; Shigley §10-2), C = D/b.
    if let Some(msg) = index_caution_labeled("spring index", design.index) {
        messages.push(msg);
    }

    // Operating stress above the allowable fraction of MTS (SMI design stress).
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

    // Stress at solid above the set-allowable fraction (SMI).
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

    if design.aspect_clamped {
        messages.push(StatusMessage {
            severity: Severity::Info,
            message: "wire aspect ratio exceeds 10:1; the torsion coefficients are clamped to the \
                      10:1 tabulated values (conservative — the true section is stiffer and \
                      slightly less stressed)"
                .into(),
        });
    }

    DesignStatus { messages }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mechanics::{bergstrasser_factor, wahl_factor};
    use approx::assert_relative_eq;

    fn music_wire() -> Material {
        crate::test_support::music_wire()
    }

    /// Baseline: axial=radial=3mm (square), mean 30mm (C=10), Na 8, L0 40mm.
    fn square_inputs() -> RectangularInputs {
        RectangularInputs {
            wire_axial: Length::from_millimeters(3.0),
            wire_radial: Length::from_millimeters(3.0),
            mean_dia: Length::from_millimeters(30.0),
            active_coils: 8.0,
            free_length: Length::from_millimeters(40.0),
            end_type: EndType::SquaredGround,
        }
    }

    fn solve_inputs(i: &RectangularInputs) -> Result<RectangularDesign> {
        solve_forward(
            &music_wire(),
            i,
            &[Force::from_newtons(10.0)],
            CurvatureCorrection::Bergstrasser,
        )
    }

    // ── α/β interpolation ────────────────────────────────────────────────────

    #[test]
    fn rect_torsion_coeffs_pins_the_table() {
        assert_eq!(rect_torsion_coeffs(1.0), (0.208, 0.141));
        assert_eq!(rect_torsion_coeffs(2.0), (0.246, 0.228));
        assert_eq!(rect_torsion_coeffs(10.0), (0.313, 0.313));
        // Between rows: b/c = 1.25 is the linear mean of the 1.00 and 1.50 rows.
        let (a, b) = rect_torsion_coeffs(1.25);
        assert_relative_eq!(a, (0.208 + 0.231) / 2.0, max_relative = 1e-12);
        assert_relative_eq!(b, (0.141 + 0.196) / 2.0, max_relative = 1e-12);
        // Clamp below 1.0 → first row; at/above 10.0 → last row.
        assert_eq!(rect_torsion_coeffs(0.5), (0.208, 0.141));
        assert_eq!(rect_torsion_coeffs(20.0), (0.313, 0.313));
    }

    // ── Square cross-check against the AF Stress Manual (the anchor) ─────────

    #[test]
    fn square_rate_matches_af_stress_manual() {
        // AF Eq. 1-90: δ = 44.5·P·r³·n/(G·b⁴) ⟹ k = G·b⁴/(44.5·r³·n).
        let g = music_wire().shear_modulus.pascals();
        let b = 0.003_f64; // 3 mm
        let r = 0.015_f64; // mean radius = D/2 = 15 mm
        let n = 8.0;
        let af = g * b.powi(4) / (44.5 * r.powi(3) * n);
        let ours = rectangular_rate(
            music_wire().shear_modulus,
            Length::from_millimeters(3.0),
            Length::from_millimeters(3.0),
            Length::from_millimeters(30.0),
            0.141,
            8.0,
        );
        // AF's 44.5 is a 3-sig-fig rounding of 2π/0.141 = 44.56, ~0.14% off — the
        // tolerance reflects the source's rounding, not our arithmetic.
        assert_relative_eq!(ours.newtons_per_meter(), af, max_relative = 3e-3);
    }

    #[test]
    fn square_stress_matches_af_eq_1_84() {
        // AF Eq. 1-84: f_smax = (4.80·P·r/b³)·Wahl(m), m = 2r/b.
        let f = Force::from_newtons(50.0);
        let b = 0.003_f64;
        let r = 0.015_f64;
        let m = 2.0 * r / b;
        let af = wahl_factor(m) * 4.80 * (50.0 * r / b.powi(3));
        let ours = rect_corrected_shear_stress(
            f,
            Length::from_millimeters(30.0),
            Length::from_millimeters(3.0),
            Length::from_millimeters(3.0),
            0.208,
            wahl_factor(m),
        );
        // AF's 4.80 is a 3-sig-fig rounding of 1/0.208 = 4.808, ~0.16% off.
        assert_relative_eq!(ours.pascals(), af, max_relative = 3e-3);
    }

    /// Provenance: the square case recovers BOTH AF magic numbers from the
    /// table's b/c = 1 row — through `rect_torsion_coeffs`, so a mistranscribed
    /// first row breaks this AF derivation, not just the literal-pinning test.
    /// Locks the source chain in code.
    #[test]
    fn provenance_square_reproduces_af_coefficients() {
        let (alpha, beta) = rect_torsion_coeffs(1.0);
        // Rate coefficient: 2π/β should be ~44.5 (AF Eq. 1-90).
        assert_relative_eq!(2.0 * std::f64::consts::PI / beta, 44.5, max_relative = 2e-3);
        // Stress coefficient: 1/α should be ~4.80 (AF Eq. 1-84).
        assert_relative_eq!(1.0 / alpha, 4.80, max_relative = 2e-3);
    }

    // ── Rectangular golden (b/c = 2), hand-computed from Shigley α/β ─────────

    #[test]
    fn rectangular_golden_b_over_c_2() {
        let i = RectangularInputs {
            wire_axial: Length::from_millimeters(4.0),
            wire_radial: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(40.0),
            active_coils: 8.0,
            free_length: Length::from_millimeters(60.0),
            end_type: EndType::SquaredGround,
        };
        let d = solve_inputs(&i).unwrap();
        assert_relative_eq!(d.aspect_ratio, 2.0, max_relative = 1e-12);
        assert_eq!((d.alpha, d.beta), (0.246, 0.228));
        // Rate: k = 4·β·b·c³·G/(π·D³·n), b=4mm, c=2mm, D=40mm, n=8.
        let g = music_wire().shear_modulus.pascals();
        let k_expected = 4.0 * 0.228 * 0.004 * 0.002_f64.powi(3) * g
            / (std::f64::consts::PI * 0.040_f64.powi(3) * 8.0);
        assert_relative_eq!(d.rate.newtons_per_meter(), k_expected, max_relative = 1e-12);
        // Stress at 10 N: Kb(D/b)·F·D/(2·α·b·c²), C = 40/4 = 10.
        let kb = bergstrasser_factor(10.0);
        let tau = kb * 10.0 * 0.040 / (2.0 * 0.246 * 0.004 * 0.002_f64.powi(2));
        assert_relative_eq!(
            d.load_points[0].shear_stress.pascals(),
            tau,
            max_relative = 1e-12
        );
        assert_relative_eq!(d.index, 10.0, max_relative = 1e-12);
    }

    // ── Orientation invariant: swap axial↔radial ────────────────────────────

    #[test]
    fn orientation_swap_is_invariant_for_torsion_but_flips_geometry() {
        let base = RectangularInputs {
            wire_axial: Length::from_millimeters(4.0),
            wire_radial: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(40.0),
            active_coils: 8.0,
            free_length: Length::from_millimeters(60.0),
            end_type: EndType::SquaredGround,
        };
        let mut swapped = base.clone();
        swapped.wire_axial = Length::from_millimeters(2.0);
        swapped.wire_radial = Length::from_millimeters(4.0);
        let a = solve_inputs(&base).unwrap();
        let b = solve_inputs(&swapped).unwrap();
        // Torsion sees only b/c → identical.
        assert_relative_eq!(a.aspect_ratio, b.aspect_ratio, max_relative = 1e-12);
        assert_relative_eq!(
            a.rate.newtons_per_meter(),
            b.rate.newtons_per_meter(),
            max_relative = 1e-12
        );
        assert_relative_eq!(
            a.load_points[0].shear_stress.pascals(),
            b.load_points[0].shear_stress.pascals(),
            max_relative = 1e-12
        );
        // Geometry flips: solid length (axial) and OD/ID (radial) differ.
        assert!((a.solid_length.meters() - b.solid_length.meters()).abs() > 1e-6);
        assert!((a.outer_dia.meters() - b.outer_dia.meters()).abs() > 1e-6);
    }

    #[test]
    fn selected_correction_governs_stress() {
        let mk = |corr| {
            solve_forward(
                &music_wire(),
                &square_inputs(),
                &[Force::from_newtons(30.0)],
                corr,
            )
            .unwrap()
            .load_points[0]
                .shear_stress
                .pascals()
        };
        let wahl = mk(CurvatureCorrection::Wahl);
        let berg = mk(CurvatureCorrection::Bergstrasser);
        // C = 30/3 = 10.
        assert_relative_eq!(
            wahl / berg,
            wahl_factor(10.0) / bergstrasser_factor(10.0),
            max_relative = 1e-12
        );
    }

    // ── Derived geometry ─────────────────────────────────────────────────────

    #[test]
    fn derived_geometry_is_exact() {
        let d = solve_inputs(&square_inputs()).unwrap();
        assert_relative_eq!(d.outer_dia.millimeters(), 33.0, max_relative = 1e-9);
        assert_relative_eq!(d.inner_dia.millimeters(), 27.0, max_relative = 1e-9);
        assert_relative_eq!(d.index, 10.0, max_relative = 1e-12);
        assert_relative_eq!(d.aspect_ratio, 1.0, max_relative = 1e-12);
        // SquaredGround: Nt = Na + 2 = 10; Ls = axial·Nt = 3·10 = 30 mm.
        assert_relative_eq!(d.total_coils, 10.0, max_relative = 1e-12);
        assert_relative_eq!(d.solid_length.millimeters(), 30.0, max_relative = 1e-9);
        assert!(!d.aspect_clamped);
    }

    /// Load-point deflection/length/pct_mts pinned at a NON-zero load (zero load
    /// can't distinguish `F/k` from `F%k`, `free−y` from `free+y`, etc.).
    /// Kills the `rect_load_point` arithmetic mutants (/→%, −→+, /→*).
    #[test]
    fn load_point_values_are_exact() {
        let d = solve_forward(
            &music_wire(),
            &square_inputs(),
            &[Force::from_newtons(20.0)],
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        let lp = &d.load_points[0];
        let y = 20.0 / d.rate.newtons_per_meter();
        assert_relative_eq!(lp.deflection.meters(), y, max_relative = 1e-12);
        assert_relative_eq!(lp.length.meters(), 0.040 - y, max_relative = 1e-9);
        assert_relative_eq!(
            lp.pct_mts,
            lp.shear_stress.pascals() / d.min_tensile_strength.pascals(),
            max_relative = 1e-12
        );
        // Sanity: 20 N on a ~5.4 kN/m spring deflects a few millimetres, well
        // under the free length (so length stays positive — distinguishes −/+).
        assert!(lp.deflection.millimeters() > 0.0 && lp.length.millimeters() < 40.0);
    }

    // ── Guard matrix ─────────────────────────────────────────────────────────

    fn msg(result: Result<RectangularDesign>) -> String {
        match result {
            Err(SpringError::InconsistentInputs(m)) => m,
            other => panic!("expected InconsistentInputs, got {other:?}"),
        }
    }

    #[test]
    fn guards_pin_messages_and_precedence() {
        let base = square_inputs();

        // Axial guard first — even with every other input bad.
        let mut i = base.clone();
        i.wire_axial = Length::from_millimeters(0.0);
        i.wire_radial = Length::from_millimeters(-1.0);
        i.mean_dia = Length::from_millimeters(-1.0);
        assert_eq!(
            msg(solve_inputs(&i)),
            "wire axial dimension must be a positive finite number"
        );

        // Radial before mean.
        let mut i = base.clone();
        i.wire_radial = Length::from_millimeters(f64::NAN);
        i.mean_dia = Length::from_millimeters(-1.0);
        assert_eq!(
            msg(solve_inputs(&i)),
            "wire radial dimension must be a positive finite number"
        );

        // Mean positive-finite before mean>b.
        let mut i = base.clone();
        i.mean_dia = Length::from_millimeters(f64::INFINITY);
        assert_eq!(
            msg(solve_inputs(&i)),
            "mean diameter must be a positive finite number"
        );

        // Mean below the larger side (b = 3mm here). Strictly below — the
        // `== b` boundary case is owned exclusively by
        // `mean_equal_to_larger_side_rejected` (conical's non-overlapping
        // matrix/boundary split).
        let mut i = base.clone();
        i.mean_dia = Length::from_millimeters(2.9); // < b
        assert_eq!(
            msg(solve_inputs(&i)),
            "mean diameter must exceed the larger wire dimension (spring index must exceed 1)"
        );

        // Active coils, free length, loads — compression's exact messages.
        let mut i = base.clone();
        i.active_coils = 0.0;
        assert_eq!(
            msg(solve_inputs(&i)),
            "active coils must be a positive finite number"
        );
        let mut i = base.clone();
        i.free_length = Length::from_millimeters(0.0);
        assert_eq!(
            msg(solve_inputs(&i)),
            "free length must be a positive finite number"
        );
        let mut i = base.clone();
        i.free_length = Length::from_millimeters(20.0); // < Ls = 30 mm
        assert_eq!(
            msg(solve_inputs(&i)),
            "free length must be at least the solid length"
        );
        assert_eq!(
            msg(solve_forward(
                &music_wire(),
                &base,
                &[Force::from_newtons(-5.0)],
                CurvatureCorrection::Bergstrasser
            )),
            "loads must be finite and non-negative"
        );
    }

    /// mean exactly equal to b is rejected (kills `<=` → `<`). With b=3mm and
    /// mean=3mm, C=1; if the guard were `<` the next check (active, valid) would
    /// pass and it would wrongly succeed.
    #[test]
    fn mean_equal_to_larger_side_rejected() {
        let mut i = square_inputs();
        i.mean_dia = Length::from_millimeters(3.0);
        assert_eq!(
            msg(solve_inputs(&i)),
            "mean diameter must exceed the larger wire dimension (spring index must exceed 1)"
        );
    }

    /// mean = 0 gives the "positive finite" message, NOT the "must exceed" one
    /// (kills `dm > 0.0` → `dm >= 0.0`, which would fall through to the mean>b
    /// guard and report the wrong message).
    #[test]
    fn mean_zero_is_positive_finite_message() {
        let mut i = square_inputs();
        i.mean_dia = Length::from_millimeters(0.0);
        assert_eq!(
            msg(solve_inputs(&i)),
            "mean diameter must be a positive finite number"
        );
    }

    /// Zero radial rejected (kills the radial `> 0.0` → `>= 0.0` — the guard
    /// matrix's radial case uses NaN, which only pins the `is_finite` half).
    /// The axial twin needs no dedicated test: the matrix's axial-first case
    /// sets axial = 0 with radial/mean also bad, so under the `>=` mutant it
    /// falls through to the radial guard and fails the pinned-message assert
    /// (conical's first-guard pattern).
    #[test]
    fn wire_radial_zero_rejected() {
        let mut i = square_inputs();
        i.wire_radial = Length::from_millimeters(0.0);
        assert_eq!(
            msg(solve_inputs(&i)),
            "wire radial dimension must be a positive finite number"
        );
    }

    /// Zero load accepted; free == solid accepted (degenerate zero-travel).
    #[test]
    fn zero_load_and_free_equals_solid_accepted() {
        // Zero load.
        let d = solve_forward(
            &music_wire(),
            &square_inputs(),
            &[Force::from_newtons(0.0)],
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert_relative_eq!(d.load_points[0].deflection.millimeters(), 0.0);
        assert_relative_eq!(d.load_points[0].shear_stress.pascals(), 0.0);
        // free == solid (Ls = 30 mm).
        let mut i = square_inputs();
        i.free_length = Length::from_millimeters(30.0);
        let r = solve_inputs(&i);
        assert!(r.is_ok(), "free == solid must be accepted, got {r:?}");
        assert_relative_eq!(r.unwrap().at_solid.force.newtons(), 0.0);
    }

    #[test]
    fn diameter_range_error_precedes_solid_length_guard() {
        // An out-of-range larger side surfaces as DiameterOutOfRange even when
        // free < solid would also fail (compression's precedence).
        let i = RectangularInputs {
            wire_axial: Length::from_millimeters(10.0), // out of range for music wire
            wire_radial: Length::from_millimeters(8.0),
            mean_dia: Length::from_millimeters(80.0),
            active_coils: 10.0,
            free_length: Length::from_millimeters(50.0), // < Ls
            end_type: EndType::SquaredGround,
        };
        assert!(
            matches!(
                solve_inputs(&i),
                Err(SpringError::DiameterOutOfRange { .. })
            ),
            "got {:?}",
            solve_inputs(&i)
        );
    }

    #[test]
    fn huge_finite_load_trips_the_output_guard() {
        let result = solve_forward(
            &music_wire(),
            &square_inputs(),
            &[Force::from_newtons(1e305)],
            CurvatureCorrection::Bergstrasser,
        );
        assert_eq!(
            msg(result),
            "rectangular solve produced a non-finite result (inputs exceed the representable range)"
        );
    }

    #[test]
    fn empty_loads_with_overflow_dimensions_trip_the_output_guard() {
        // rate denominator overflows → rate = 0.0 (finite); with no load points the
        // per-load checks are vacuous, so at_solid.deflection = 0/0 = NaN must be
        // caught by the at_solid.deflection element of the guard.
        let i = RectangularInputs {
            wire_axial: Length::from_millimeters(3.0),
            wire_radial: Length::from_millimeters(3.0),
            mean_dia: Length::from_millimeters(1e200),
            active_coils: 8.0,
            free_length: Length::from_millimeters(1e201),
            end_type: EndType::SquaredGround,
        };
        let result = solve_forward(&music_wire(), &i, &[], CurvatureCorrection::Bergstrasser);
        assert_eq!(
            msg(result),
            "rectangular solve produced a non-finite result (inputs exceed the representable range)"
        );
    }

    // ── evaluate_status ──────────────────────────────────────────────────────

    fn has_message(status: &DesignStatus, needle: &str) -> bool {
        status.messages.iter().any(|m| m.message.contains(needle))
    }

    #[test]
    fn index_caution_fires_out_of_band() {
        // C = 30/3 = 10 → in band; C small → caution. Use mean 9mm, b 3mm → C=3.
        let mut i = square_inputs();
        i.mean_dia = Length::from_millimeters(9.0); // C = 3 (< 4 → caution)
        let d = solve_inputs(&i).unwrap();
        let s = evaluate_status(&d, &music_wire());
        assert!(has_message(&s, "spring index"));
        // Clean case C = 10 → no caution.
        let clean = solve_inputs(&square_inputs()).unwrap();
        assert!(!has_message(
            &evaluate_status(&clean, &music_wire()),
            "spring index"
        ));
    }

    #[test]
    fn overstress_and_solid_warnings_fire() {
        let i = RectangularInputs {
            wire_axial: Length::from_millimeters(1.0),
            wire_radial: Length::from_millimeters(1.0),
            mean_dia: Length::from_millimeters(8.0),
            active_coils: 6.0,
            free_length: Length::from_millimeters(40.0),
            end_type: EndType::SquaredGround,
        };
        let d = solve_forward(
            &music_wire(),
            &i,
            &[Force::from_newtons(60.0)],
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        let s = evaluate_status(&d, &music_wire());
        assert!(has_message(&s, "load point 1 stress"));
        assert!(has_message(&s, "stress at solid"));
        assert!(s.has_warnings());
    }

    #[test]
    fn aspect_clamp_info_present_only_above_10() {
        // b/c = 12.5 (axial 2.5mm, radial 0.2mm) → clamped. In-range larger side;
        // mean 25mm keeps C = D/b = 10 in band.
        let i = RectangularInputs {
            wire_axial: Length::from_millimeters(2.5),
            wire_radial: Length::from_millimeters(0.2),
            mean_dia: Length::from_millimeters(25.0), // C = 10
            active_coils: 8.0,
            free_length: Length::from_millimeters(40.0),
            end_type: EndType::SquaredGround,
        };
        let d = solve_inputs(&i).unwrap();
        assert!(d.aspect_clamped);
        let s = evaluate_status(&d, &music_wire());
        assert!(s.messages.iter().any(|m| m.severity == Severity::Info
            && m.message
                == "wire aspect ratio exceeds 10:1; the torsion coefficients are clamped to the \
                    10:1 tabulated values (conservative — the true section is stiffer and \
                    slightly less stressed)"));
        // Exactly at 10 (axial 2.0, radial 0.2) → NOT clamped (aspect > 10.0 is strict).
        let mut at10 = i.clone();
        at10.wire_axial = Length::from_millimeters(2.0);
        let d10 = solve_inputs(&at10).unwrap();
        assert!(!d10.aspect_clamped);
        assert!(!has_message(
            &evaluate_status(&d10, &music_wire()),
            "aspect ratio exceeds"
        ));
    }

    /// Stress exactly at the allowable fraction raises NO warning (kills `>` → `>=`).
    #[test]
    fn load_stress_exactly_at_allowable_no_warning() {
        let mut d = solve_inputs(&square_inputs()).unwrap();
        let mut m2 = music_wire();
        m2.allowable_pct_torsion = 0.50;
        d.load_points[0].pct_mts = 0.50;
        let s = evaluate_status(&d, &m2);
        assert!(!s.messages.iter().any(|m| m.message.contains("load point")));
    }

    /// At-solid stress exactly at the set allowable raises NO warning (kills `>` → `>=`).
    #[test]
    fn at_solid_exactly_at_set_allowable_no_warning() {
        let mut d = solve_inputs(&square_inputs()).unwrap();
        let mut m2 = music_wire();
        m2.allowable_pct_set = 0.60;
        d.at_solid.pct_mts = 0.60;
        let s = evaluate_status(&d, &m2);
        assert!(!s
            .messages
            .iter()
            .any(|m| m.message.contains("stress at solid")));
    }
}
