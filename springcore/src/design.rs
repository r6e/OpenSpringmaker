//! Aggregate forward solve: from fully-determined geometry to a complete design,
//! plus engineering status checks. Formula sources cited at each call site.

use crate::end_type::EndType;
use crate::material::Material;
use crate::mechanics::{
    corrected_shear_stress, is_buckling_stable, natural_frequency, spring_index, spring_rate,
    EndFixity,
};
use crate::units::{Force, Frequency, Length, SpringRate, Stress};
use crate::{Result, SpringError};

/// State of the spring at one axial load.
#[derive(Debug, Clone, Copy)]
pub struct LoadPoint {
    pub force: Force,
    pub deflection: Length,
    pub length: Length,
    pub shear_stress: Stress,
    pub pct_mts: f64,
}

/// A fully computed compression-spring design.
#[derive(Debug, Clone)]
pub struct SpringDesign {
    pub wire_dia: Length,
    pub mean_dia: Length,
    pub index: f64,
    pub active_coils: f64,
    pub total_coils: f64,
    pub rate: SpringRate,
    pub free_length: Length,
    pub solid_length: Length,
    pub pitch: Length,
    pub outer_dia: Length,
    pub inner_dia: Length,
    pub min_tensile_strength: Stress,
    pub natural_frequency: Frequency,
    pub buckling_stable: bool,
    pub load_points: Vec<LoadPoint>,
    pub at_solid: LoadPoint,
    pub end_type: EndType,
}

#[allow(clippy::too_many_arguments)]
fn load_point(
    force: Force,
    rate: SpringRate,
    free_length: Length,
    mean_dia: Length,
    wire_dia: Length,
    index: f64,
    mts: Stress,
    correction: crate::CurvatureCorrection,
) -> LoadPoint {
    // Deflection y = F/k (Shigley Eq. 10-9 rearranged).
    let y = force.newtons() / rate.newtons_per_meter();
    let length = Length::from_meters(free_length.meters() - y);
    let stress = corrected_shear_stress(force, mean_dia, wire_dia, correction.factor(index));
    LoadPoint {
        force,
        deflection: Length::from_meters(y),
        length,
        shear_stress: stress,
        pct_mts: stress.pascals() / mts.pascals(),
    }
}

/// Compute a complete design from determined geometry plus operating loads.
#[allow(clippy::too_many_arguments)]
pub fn solve_forward(
    material: &Material,
    end_type: EndType,
    fixity: EndFixity,
    wire_dia: Length,
    mean_dia: Length,
    active: f64,
    free_length: Length,
    loads: &[Force],
    correction: crate::CurvatureCorrection,
) -> Result<SpringDesign> {
    // Wire diameter must be finite and positive; a zero/non-finite d gives a zero
    // or non-finite rate (k ∝ d⁴) that would silently flow into deflection/stresses.
    if !(wire_dia.meters().is_finite() && wire_dia.meters() > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "wire diameter must be a positive finite number".into(),
        ));
    }
    // Mean diameter must be finite; a non-finite dm gives a zero or non-finite
    // rate (k ∝ 1/dm³). Positivity is enforced by the mean > wire check below
    // (wire is already finite-positive), but that relational check admits
    // +Inf/NaN, so guard finiteness explicitly here.
    if !mean_dia.meters().is_finite() {
        return Err(SpringError::InconsistentInputs(
            "mean diameter must be finite".into(),
        ));
    }
    // A spring index ≤ 1 is physically meaningless; mean diameter must exceed wire diameter
    // (SMI Handbook; Shigley §10-1). This also guards the Dimensional scenario when
    // outer_dia ≤ wire_dia, which would produce a non-positive mean diameter.
    if mean_dia.meters() <= wire_dia.meters() {
        return Err(SpringError::InconsistentInputs(
            "mean diameter must be greater than wire diameter (spring index must exceed 1)".into(),
        ));
    }
    // Active coils must be finite and positive; k ∝ 1/Na, so Na ≤ 0 yields a
    // non-finite or negative rate.
    if !(active.is_finite() && active > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "active coils must be a positive finite number".into(),
        ));
    }
    // Free length must be finite and positive; a non-finite L0 propagates into pitch
    // and buckling and the per-load lengths.
    if !(free_length.meters().is_finite() && free_length.meters() > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "free length must be a positive finite number".into(),
        ));
    }
    // Every load must be finite and non-negative. A non-finite load makes deflection
    // and stresses NaN/Inf; a negative load drives the signed-force stress formulas
    // to negative stresses and %-allowable ratios that never exceed the limit,
    // silently hiding overstress. Zero is allowed (a valid free-state reference point).
    if loads
        .iter()
        .any(|f| !f.newtons().is_finite() || f.newtons() < 0.0)
    {
        return Err(SpringError::InconsistentInputs(
            "loads must be finite and non-negative".into(),
        ));
    }

    // Validate the wire diameter against the material's manufacturable range before
    // any geometry-derived check, so an out-of-range diameter surfaces as
    // DiameterOutOfRange rather than a downstream geometry error.
    let mts = material.min_tensile_strength(wire_dia)?;

    let index = spring_index(mean_dia, wire_dia);
    let rate = spring_rate(material.shear_modulus, wire_dia, mean_dia, active);
    let total_coils = end_type.total_coils(active);
    let solid_length = end_type.solid_length(wire_dia, active);
    // Free length cannot be below solid length — the coils physically cannot
    // compress past solid. Beyond being impossible geometry, it makes the
    // solid force F = k·(L0 − Ls) negative, which yields negative stress and a
    // negative %-allowable that silently never trips the set/overstress check.
    if free_length.meters() < solid_length.meters() {
        return Err(SpringError::InconsistentInputs(
            "free length must be at least the solid length".into(),
        ));
    }
    let pitch = end_type.pitch_from_free_length(wire_dia, active, free_length);
    let nat_freq = natural_frequency(
        wire_dia,
        mean_dia,
        active,
        material.shear_modulus,
        material.density,
    );
    let stable = is_buckling_stable(
        free_length,
        mean_dia,
        material.youngs_modulus,
        material.shear_modulus,
        fixity,
    );

    let load_points = loads
        .iter()
        .map(|&f| {
            load_point(
                f,
                rate,
                free_length,
                mean_dia,
                wire_dia,
                index,
                mts,
                correction,
            )
        })
        .collect();

    // Force required to reach solid: F = k * (L0 - Ls).
    let solid_force = Force::from_newtons(
        rate.newtons_per_meter() * (free_length.meters() - solid_length.meters()),
    );
    let at_solid = load_point(
        solid_force,
        rate,
        free_length,
        mean_dia,
        wire_dia,
        index,
        mts,
        correction,
    );

    Ok(SpringDesign {
        wire_dia,
        mean_dia,
        index,
        active_coils: active,
        total_coils,
        rate,
        free_length,
        solid_length,
        pitch,
        outer_dia: Length::from_meters(mean_dia.meters() + wire_dia.meters()),
        inner_dia: Length::from_meters(mean_dia.meters() - wire_dia.meters()),
        min_tensile_strength: mts,
        natural_frequency: nat_freq,
        buckling_stable: stable,
        load_points,
        at_solid,
        end_type,
    })
}

/// Severity of a design-status message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Caution,
    Warning,
}

/// One status/advisory message about a design.
#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub severity: Severity,
    pub message: String,
}

/// Collected status messages for a design.
#[derive(Debug, Clone, Default)]
pub struct DesignStatus {
    pub messages: Vec<StatusMessage>,
}

impl DesignStatus {
    /// Returns `true` if any message has [`Severity::Warning`] severity.
    pub fn has_warnings(&self) -> bool {
        self.messages
            .iter()
            .any(|m| m.severity == Severity::Warning)
    }
}

/// Recommended spring-index bounds (SMI Handbook; Shigley §10-2 guidance).
const INDEX_MIN: f64 = 4.0;
const INDEX_MAX: f64 = 12.0;

/// Apply engineering checks to a computed design.
pub fn evaluate_status(design: &SpringDesign, material: &Material) -> DesignStatus {
    let mut messages = Vec::new();

    // Spring index outside the practical manufacturing range (SMI; Shigley §10-2).
    if design.index < INDEX_MIN || design.index > INDEX_MAX {
        messages.push(StatusMessage {
            severity: Severity::Caution,
            message: format!(
                "spring index {:.2} is outside the recommended range {INDEX_MIN}–{INDEX_MAX}",
                design.index
            ),
        });
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

    // Buckling (Shigley Eq. 10-10 absolute-stability criterion).
    if !design.buckling_stable {
        messages.push(StatusMessage {
            severity: Severity::Warning,
            message: "free length exceeds the absolute-stability limit; buckling possible".into(),
        });
    }

    // Free length shorter than solid length is physically invalid.
    if design.free_length.meters() < design.solid_length.meters() {
        messages.push(StatusMessage {
            severity: Severity::Warning,
            message: "free length is less than solid length".into(),
        });
    }

    DesignStatus { messages }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mechanics::EndFixity;
    use crate::units::{Force, Length};
    use approx::assert_relative_eq;

    /// Build a clean, warning-free baseline design.
    /// d=2mm, D=20mm -> C=10, Na=10. G=80 GPa -> k=2000 N/m.
    /// Solid: Nt=12, Ls=24mm. Free=60mm. load=[10N].
    fn baseline_design() -> (SpringDesign, crate::material::Material) {
        let m = crate::test_support::music_wire();
        let design = solve_forward(
            &m,
            EndType::SquaredGround,
            EndFixity::FixedFixed,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(60.0),
            &[Force::from_newtons(10.0)],
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        (design, m)
    }

    fn has_message(status: &DesignStatus, needle: &str) -> bool {
        status.messages.iter().any(|m| m.message.contains(needle))
    }

    // ── Arithmetic: load_point (line 56) ─────────────────────────────────────

    #[test]
    fn load_point_deflection_and_length_are_exact() {
        // y = F/k = 10/2000 = 0.005 m = 5 mm.
        // length = free_length - y = 60 - 5 = 55 mm.
        // Kills: `- → +` (y=0 → length=65), `- → /` (y=0 → length=12000).
        let (design, _) = baseline_design();
        let lp = &design.load_points[0];
        assert_relative_eq!(lp.deflection.millimeters(), 5.0, max_relative = 1e-9);
        assert_relative_eq!(lp.length.millimeters(), 55.0, max_relative = 1e-9);
    }

    // ── Arithmetic: solve_forward solid_force (lines 107) ────────────────────

    #[test]
    fn at_solid_force_is_exact() {
        // solid_force = k * (L0 - Ls) = 2000 * (0.060 - 0.024) = 2000 * 0.036 = 72 N.
        // Kills `* → +` (2000.036), `* → /` (55555), `- → +` (168 N), `- → /` (5000 N).
        let (design, _) = baseline_design();
        assert_relative_eq!(design.at_solid.force.newtons(), 72.0, max_relative = 1e-9);
        // at_solid is itself a load_point, so deflection and length must also be consistent.
        // deflection = 72/2000 = 36 mm; length = 60 - 36 = 24 mm (== solid_length).
        assert_relative_eq!(
            design.at_solid.deflection.millimeters(),
            36.0,
            max_relative = 1e-9
        );
        assert_relative_eq!(
            design.at_solid.length.millimeters(),
            24.0,
            max_relative = 1e-9
        );
    }

    // ── Arithmetic: outer_dia and inner_dia (lines 129-130) ──────────────────

    #[test]
    fn outer_and_inner_dia_are_exact() {
        // OD = D + d = 20 + 2 = 22 mm. Kills `+ → -` (18), `+ → *` (40).
        // ID = D - d = 20 - 2 = 18 mm. Kills `- → +` (22), `- → /` (10).
        let (design, _) = baseline_design();
        assert_relative_eq!(design.outer_dia.millimeters(), 22.0, max_relative = 1e-9);
        assert_relative_eq!(design.inner_dia.millimeters(), 18.0, max_relative = 1e-9);
    }

    // ── has_warnings on clean design (line 164) ───────────────────────────────

    #[test]
    fn has_warnings_is_false_on_clean_design() {
        // Kills: `has_warnings → true`.
        let (design, m) = baseline_design();
        let status = evaluate_status(&design, &m);
        assert!(
            !status.has_warnings(),
            "clean design should have no warnings"
        );
    }

    // ── evaluate_status: index boundaries (line 179) ─────────────────────────

    #[test]
    fn forward_solve_clean_case() {
        let m = crate::test_support::music_wire();
        // d=2mm, D=20mm -> C=10, Na=10. G=80 GPa -> k = 2000 N/m.
        let design = solve_forward(
            &m,
            EndType::SquaredGround,
            EndFixity::FixedFixed,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(60.0),
            &[Force::from_newtons(10.0)],
            crate::CurvatureCorrection::Wahl,
        )
        .unwrap();
        assert_relative_eq!(design.index, 10.0, max_relative = 1e-12);
        assert_relative_eq!(design.rate.newtons_per_meter(), 2000.0, max_relative = 1e-9);
        assert_relative_eq!(design.total_coils, 12.0, max_relative = 1e-12);
        // Solid length = d*Nt = 2*12 = 24 mm
        assert_relative_eq!(design.solid_length.millimeters(), 24.0, max_relative = 1e-9);
        // Load 10 N -> deflection 10/2000 = 0.005 m = 5 mm
        let lp = &design.load_points[0];
        assert_relative_eq!(lp.deflection.millimeters(), 5.0, max_relative = 1e-9);
        // stress = Kw*8FD/(pi d^3), Kw = wahl(10)
        let kw = 39.0 / 36.0 + 0.0615;
        let expected = kw * 8.0 * 10.0 * 0.020 / (std::f64::consts::PI * 0.002_f64.powi(3));
        assert_relative_eq!(lp.shear_stress.pascals(), expected, max_relative = 1e-9);
    }

    #[test]
    fn index_at_min_boundary_produces_no_index_message() {
        // index == INDEX_MIN (4.0) → no message. Kills `< → <=` and `< → ==`.
        let (mut design, m) = baseline_design();
        design.index = INDEX_MIN; // exactly 4.0
        let status = evaluate_status(&design, &m);
        assert!(
            !has_message(&status, "index"),
            "index at exact minimum should not trigger caution"
        );
    }

    #[test]
    fn index_at_max_boundary_produces_no_index_message() {
        // index == INDEX_MAX (12.0) → no message. Kills `> → >=` and `> → ==`.
        let (mut design, m) = baseline_design();
        design.index = INDEX_MAX; // exactly 12.0
        let status = evaluate_status(&design, &m);
        assert!(
            !has_message(&status, "index"),
            "index at exact maximum should not trigger caution"
        );
    }

    #[test]
    fn index_below_min_produces_index_message() {
        // index < INDEX_MIN → message present. Kills `< → >`.
        let (mut design, m) = baseline_design();
        design.index = INDEX_MIN - 0.01;
        let status = evaluate_status(&design, &m);
        assert!(
            has_message(&status, "index"),
            "index below minimum should trigger caution"
        );
    }

    #[test]
    fn index_above_max_produces_index_message() {
        // index > INDEX_MAX → message present.
        let (mut design, m) = baseline_design();
        design.index = INDEX_MAX + 0.01;
        let status = evaluate_status(&design, &m);
        assert!(
            has_message(&status, "index"),
            "index above maximum should trigger caution"
        );
    }

    // ── evaluate_status: load pct_mts boundary (line 192) ────────────────────

    #[test]
    fn load_stress_exactly_at_allowable_produces_no_stress_message() {
        // pct_mts == allowable → no "load point" warning. Kills `> → >=` and `> → ==`.
        let (mut design, mut m) = baseline_design();
        m.allowable_pct_torsion = 0.50;
        design.load_points[0].pct_mts = 0.50; // exactly equal
        let status = evaluate_status(&design, &m);
        assert!(
            !has_message(&status, "load point"),
            "stress exactly at allowable should not trigger warning"
        );
    }

    #[test]
    fn load_stress_above_allowable_produces_stress_message() {
        // pct_mts > allowable → warning present. Kills `> → <` and `> → ==`.
        let (mut design, mut m) = baseline_design();
        m.allowable_pct_torsion = 0.50;
        design.load_points[0].pct_mts = 0.51;
        let status = evaluate_status(&design, &m);
        assert!(
            has_message(&status, "load point"),
            "stress above allowable should trigger warning"
        );
    }

    #[test]
    fn load_stress_below_allowable_produces_no_stress_message() {
        // pct_mts < allowable → no warning. Sanity check.
        let (mut design, mut m) = baseline_design();
        m.allowable_pct_torsion = 0.50;
        design.load_points[0].pct_mts = 0.49;
        let status = evaluate_status(&design, &m);
        assert!(
            !has_message(&status, "load point"),
            "stress below allowable should not trigger warning"
        );
    }

    // ── evaluate_status: at_solid boundary (line 206) ────────────────────────

    #[test]
    fn at_solid_stress_exactly_at_set_allowable_produces_no_solid_message() {
        // at_solid.pct_mts == allowable_pct_set → no "stress at solid" warning.
        // Kills `> → >=` and `> → ==`.
        let (mut design, mut m) = baseline_design();
        m.allowable_pct_set = 0.60;
        design.at_solid.pct_mts = 0.60; // exactly equal
        let status = evaluate_status(&design, &m);
        assert!(
            !has_message(&status, "stress at solid"),
            "at-solid stress exactly at set allowable should not trigger warning"
        );
    }

    #[test]
    fn at_solid_stress_above_set_allowable_produces_solid_message() {
        // at_solid.pct_mts > allowable_pct_set → warning present.
        // Kills `> → <` and `> → ==`.
        let (mut design, mut m) = baseline_design();
        m.allowable_pct_set = 0.60;
        design.at_solid.pct_mts = 0.61;
        let status = evaluate_status(&design, &m);
        assert!(
            has_message(&status, "stress at solid"),
            "at-solid stress above set allowable should trigger warning"
        );
    }

    #[test]
    fn at_solid_stress_below_set_allowable_produces_no_solid_message() {
        // Sanity check.
        let (mut design, mut m) = baseline_design();
        m.allowable_pct_set = 0.60;
        design.at_solid.pct_mts = 0.59;
        let status = evaluate_status(&design, &m);
        assert!(
            !has_message(&status, "stress at solid"),
            "at-solid stress below set allowable should not trigger warning"
        );
    }

    // ── evaluate_status: buckling (line 218) ─────────────────────────────────

    #[test]
    fn buckling_unstable_produces_buckling_message() {
        // !stable == true → message. Kills `delete !` (would require stable=true to trigger).
        let (mut design, m) = baseline_design();
        design.buckling_stable = false;
        let status = evaluate_status(&design, &m);
        assert!(
            has_message(&status, "buckling"),
            "unstable design should trigger buckling warning"
        );
    }

    #[test]
    fn buckling_stable_produces_no_buckling_message() {
        // stable=true → no message. Complements the above to fully pin the `!`.
        let (mut design, m) = baseline_design();
        design.buckling_stable = true;
        let status = evaluate_status(&design, &m);
        assert!(
            !has_message(&status, "buckling"),
            "stable design should not trigger buckling warning"
        );
    }

    // ── evaluate_status: free_length < solid_length (line 226) ───────────────

    #[test]
    fn free_length_equal_to_solid_length_produces_no_length_message() {
        // free_length == solid_length → no "less than solid" warning.
        // Kills `< → <=` and `< → ==`.
        let (mut design, m) = baseline_design();
        design.free_length = design.solid_length; // exact bit-copy
        let status = evaluate_status(&design, &m);
        assert!(
            !has_message(&status, "less than solid"),
            "free == solid should not trigger length warning"
        );
    }

    #[test]
    fn free_length_below_solid_length_produces_length_message() {
        // free_length < solid_length → warning present. Kills `< → >` and `< → ==`.
        let (mut design, m) = baseline_design();
        // Subtract 1 mm from free_length so it's below solid_length (24 mm baseline).
        design.free_length = Length::from_meters(design.solid_length.meters() - 0.001);
        let status = evaluate_status(&design, &m);
        assert!(
            has_message(&status, "less than solid"),
            "free < solid should trigger length warning"
        );
    }

    #[test]
    fn free_length_above_solid_length_produces_no_length_message() {
        // Sanity check.
        let (mut design, m) = baseline_design();
        design.free_length = Length::from_meters(design.solid_length.meters() + 0.001);
        let status = evaluate_status(&design, &m);
        assert!(
            !has_message(&status, "less than solid"),
            "free > solid should not trigger length warning"
        );
    }

    // ── Legacy tests kept for coverage breadth ────────────────────────────────

    #[test]
    fn status_flags_low_index() {
        let m = crate::test_support::music_wire();
        // C = 16/2 = 8 is fine; make C=3 (D=6mm,d=2mm) to trigger low-index caution.
        let design = solve_forward(
            &m,
            EndType::SquaredGround,
            EndFixity::FixedFixed,
            Length::from_millimeters(2.0),
            Length::from_millimeters(6.0),
            10.0,
            Length::from_millimeters(60.0),
            &[Force::from_newtons(10.0)],
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        let status = evaluate_status(&design, &m);
        assert!(status
            .messages
            .iter()
            .any(|msg| msg.message.contains("index")));
    }

    // ── solve_forward: mean_dia ≤ wire_dia guard (lines 79-86) ─────────────────

    /// mean == wire is rejected (spring index == 1; coil cannot close).
    /// Kills: `<=` → `<` (would accept equal) and `<=` → `==` (would miss mean < wire).
    #[test]
    fn solve_forward_rejects_mean_equal_to_wire() {
        let m = crate::test_support::music_wire();
        let result = solve_forward(
            &m,
            EndType::SquaredGround,
            EndFixity::FixedFixed,
            Length::from_millimeters(2.0),
            Length::from_millimeters(2.0), // mean == wire → rejected
            10.0,
            Length::from_millimeters(60.0),
            &[Force::from_newtons(10.0)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(
            matches!(result, Err(crate::SpringError::InconsistentInputs(_))),
            "mean == wire must return InconsistentInputs"
        );
    }

    /// active ≤ 0 makes spring_rate divide by zero/negative → Inf/negative rate
    /// silently flowing into deflection and stresses.
    #[test]
    fn solve_forward_rejects_non_positive_active() {
        let m = crate::test_support::music_wire();
        let result = solve_forward(
            &m,
            EndType::SquaredGround,
            EndFixity::FixedFixed,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            0.0, // active = 0 → rejected
            Length::from_millimeters(60.0),
            &[Force::from_newtons(10.0)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(
            matches!(result, Err(crate::SpringError::InconsistentInputs(_))),
            "active <= 0 must return InconsistentInputs"
        );
    }

    /// wire_dia = 0 slips past the mean > wire check yet yields a zero rate
    /// (d⁴ = 0) → infinite deflection.
    #[test]
    fn solve_forward_rejects_zero_wire_dia() {
        let m = crate::test_support::music_wire();
        let result = solve_forward(
            &m,
            EndType::SquaredGround,
            EndFixity::FixedFixed,
            Length::from_millimeters(0.0), // wire = 0 → rejected
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(60.0),
            &[Force::from_newtons(10.0)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(
            matches!(result, Err(crate::SpringError::InconsistentInputs(_))),
            "wire_dia <= 0 must return InconsistentInputs"
        );
    }

    /// Non-finite mean_dia (+Inf) slips past `mean <= wire` (Inf <= wire is false)
    /// yet yields a zero rate (k ∝ 1/dm³) → infinite deflection.
    #[test]
    fn solve_forward_rejects_non_finite_mean_dia() {
        let m = crate::test_support::music_wire();
        let result = solve_forward(
            &m,
            EndType::SquaredGround,
            EndFixity::FixedFixed,
            Length::from_millimeters(2.0),
            Length::from_millimeters(f64::INFINITY), // mean = +Inf → rejected
            10.0,
            Length::from_millimeters(60.0),
            &[Force::from_newtons(10.0)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(
            matches!(result, Err(crate::SpringError::InconsistentInputs(_))),
            "non-finite mean_dia must return InconsistentInputs"
        );
    }

    /// free_length = 0 is non-physical; `> 0.0` (not `>= 0.0`) must reject it.
    #[test]
    fn solve_forward_rejects_zero_free_length() {
        let m = crate::test_support::music_wire();
        let result = solve_forward(
            &m,
            EndType::SquaredGround,
            EndFixity::FixedFixed,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(0.0), // free length = 0 → rejected
            &[Force::from_newtons(10.0)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(
            matches!(&result, Err(crate::SpringError::InconsistentInputs(m)) if m == "free length must be a positive finite number"),
            "expected the free-length guard, got {result:?}"
        );
    }

    /// Non-finite free length propagates into pitch/buckling/per-load lengths.
    #[test]
    fn solve_forward_rejects_non_finite_free_length() {
        let m = crate::test_support::music_wire();
        let result = solve_forward(
            &m,
            EndType::SquaredGround,
            EndFixity::FixedFixed,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(f64::INFINITY), // free length = +Inf → rejected
            &[Force::from_newtons(10.0)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(
            matches!(result, Err(crate::SpringError::InconsistentInputs(_))),
            "non-finite free_length must return InconsistentInputs"
        );
    }

    /// A NaN load would yield NaN deflection and stresses.
    #[test]
    fn solve_forward_rejects_non_finite_load() {
        let m = crate::test_support::music_wire();
        let result = solve_forward(
            &m,
            EndType::SquaredGround,
            EndFixity::FixedFixed,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(60.0),
            &[Force::from_newtons(f64::NAN)], // NaN load → rejected
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(
            matches!(result, Err(crate::SpringError::InconsistentInputs(_))),
            "non-finite load must return InconsistentInputs"
        );
    }

    /// free_length below solid length is impossible geometry and makes the solid
    /// force k·(L0−Ls) negative — negative stress and a %-allowable that can't trip
    /// the set check. SquaredGround d=2,Na=10 → Ls = d(Na+2) = 24mm; 20 < 24.
    #[test]
    fn solve_forward_rejects_free_length_below_solid() {
        let m = crate::test_support::music_wire();
        let result = solve_forward(
            &m,
            EndType::SquaredGround,
            EndFixity::FixedFixed,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(20.0), // L0 = 20mm < Ls = 24mm → rejected
            &[Force::from_newtons(10.0)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(
            matches!(&result, Err(crate::SpringError::InconsistentInputs(m)) if m == "free length must be at least the solid length"),
            "free_length < solid_length must be rejected, got {result:?}"
        );
    }

    /// An out-of-range wire diameter must surface as DiameterOutOfRange even when
    /// the geometry is ALSO invalid (free < solid). Pins the `mts` check ordering:
    /// if `min_tensile_strength` were moved back after the solid-length guard, this
    /// would return the geometry error instead. d=10mm is out of range for music
    /// wire AND makes Ls = 10(10+2) = 120mm > free = 50mm.
    #[test]
    fn solve_forward_diameter_error_precedes_geometry_error() {
        let m = crate::test_support::music_wire();
        let result = solve_forward(
            &m,
            EndType::SquaredGround,
            EndFixity::FixedFixed,
            Length::from_millimeters(10.0), // out of range for music wire
            Length::from_millimeters(80.0),
            10.0,
            Length::from_millimeters(50.0), // < solid (120mm) — also invalid
            &[Force::from_newtons(10.0)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(
            matches!(result, Err(crate::SpringError::DiameterOutOfRange { .. })),
            "out-of-range diameter must win over the geometry error, got {result:?}"
        );
    }

    /// free_length == solid length is the boundary: a (degenerate, zero-travel)
    /// design that must be ACCEPTED, with solid force k·(L0−Ls) = 0. Pins the
    /// `<` (not `<=`) boundary of the solid-length guard.
    #[test]
    fn solve_forward_accepts_free_length_equal_solid() {
        let m = crate::test_support::music_wire();
        let design = solve_forward(
            &m,
            EndType::SquaredGround,
            EndFixity::FixedFixed,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(24.0), // L0 == Ls = d(Na+2) = 24mm
            &[Force::from_newtons(10.0)],
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert_relative_eq!(design.at_solid.force.newtons(), 0.0);
    }

    /// A negative load yields negative stresses and %-allowable ratios that never
    /// exceed the limit, silently hiding overstress.
    #[test]
    fn solve_forward_rejects_negative_load() {
        let m = crate::test_support::music_wire();
        let result = solve_forward(
            &m,
            EndType::SquaredGround,
            EndFixity::FixedFixed,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(60.0),
            &[Force::from_newtons(-5.0)], // negative load → rejected
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(
            matches!(&result, Err(crate::SpringError::InconsistentInputs(m)) if m == "loads must be finite and non-negative"),
            "expected the loads guard, got {result:?}"
        );
    }

    /// A zero load is allowed (a valid free-state point). Pins the `< 0.0`
    /// (not `<= 0.0`) boundary.
    #[test]
    fn solve_forward_accepts_zero_load() {
        let m = crate::test_support::music_wire();
        let design = solve_forward(
            &m,
            EndType::SquaredGround,
            EndFixity::FixedFixed,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(60.0),
            &[Force::from_newtons(0.0)],
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert_relative_eq!(design.load_points[0].deflection.millimeters(), 0.0);
        assert_relative_eq!(design.load_points[0].shear_stress.pascals(), 0.0);
    }

    /// The selected correction factor governs the body shear: the same geometry
    /// solved with Wahl vs Bergsträsser yields the two factors' stress ratio.
    #[test]
    fn solve_forward_uses_selected_correction() {
        let m = crate::test_support::music_wire();
        let mk = |corr| {
            solve_forward(
                &m,
                EndType::SquaredGround,
                EndFixity::FixedFixed,
                Length::from_millimeters(2.0),
                Length::from_millimeters(20.0),
                10.0,
                Length::from_millimeters(60.0),
                &[Force::from_newtons(30.0)],
                corr,
            )
            .unwrap()
            .load_points[0]
                .shear_stress
                .pascals()
        };
        let wahl = mk(crate::CurvatureCorrection::Wahl);
        let berg = mk(crate::CurvatureCorrection::Bergstrasser);
        // C = 10 → Kw/Kb = (39/36+0.0615)/(42/37); stresses share base 8FD/πd³.
        assert_relative_eq!(
            wahl / berg,
            crate::mechanics::wahl_factor(10.0) / crate::mechanics::bergstrasser_factor(10.0),
            max_relative = 1e-12
        );
    }

    /// mean < wire is also rejected.
    /// Kills: `<=` → `<` would still accept if mean < wire (not applicable here — belt-and-suspenders).
    #[test]
    fn solve_forward_rejects_mean_less_than_wire() {
        let m = crate::test_support::music_wire();
        let result = solve_forward(
            &m,
            EndType::SquaredGround,
            EndFixity::FixedFixed,
            Length::from_millimeters(5.0),
            Length::from_millimeters(3.0), // mean < wire → rejected
            10.0,
            Length::from_millimeters(60.0),
            &[Force::from_newtons(10.0)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(
            matches!(result, Err(crate::SpringError::InconsistentInputs(_))),
            "mean < wire must return InconsistentInputs"
        );
    }

    /// mean just above wire (by 1 µm) is accepted — pins the `<=` vs `<` boundary.
    /// Kills: `<=` → `<` would accept equal (but we check the just-above case too).
    #[test]
    fn solve_forward_accepts_mean_just_above_wire() {
        let m = crate::test_support::music_wire();
        // wire = 2 mm, mean = 2.001 mm → index ≈ 1.0005 (accepted by guard, though low).
        let result = solve_forward(
            &m,
            EndType::SquaredGround,
            EndFixity::FixedFixed,
            Length::from_millimeters(2.0),
            Length::from_millimeters(2.001), // mean just above wire → accepted
            10.0,
            Length::from_millimeters(60.0),
            &[Force::from_newtons(1.0)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(
            result.is_ok(),
            "mean just above wire must be accepted by the guard"
        );
    }

    #[test]
    fn status_flags_overstress_at_solid() {
        let m = crate::test_support::music_wire();
        // Very stiff, large deflection to solid -> overstress.
        let design = solve_forward(
            &m,
            EndType::SquaredGround,
            EndFixity::FixedFixed,
            Length::from_millimeters(1.0),
            Length::from_millimeters(8.0),
            6.0,
            Length::from_millimeters(60.0),
            &[Force::from_newtons(5.0)],
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        let status = evaluate_status(&design, &m);
        assert!(status.has_warnings());
    }
}
