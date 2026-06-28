//! Aggregate forward solve for extension springs: from fully-determined geometry
//! to a complete design. Hook-stress checks replace the solid-length / buckling
//! checks that belong only to compression springs.

use crate::extension::ends::HookEnds;
use crate::extension::mechanics::{deflection, hook_bending_stress, hook_torsion_stress};
use crate::material::Material;
use crate::mechanics::{corrected_shear_stress, spring_index, spring_rate};
use crate::units::{Force, Length, SpringRate, Stress};
use crate::{CurvatureCorrection, DesignStatus, Result, Severity, SpringError, StatusMessage};

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
    /// hook_torsion (τ_B) / (allowable_pct_end_torsion · Sut)
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
    /// Engineering status (overstress warnings, index caution) computed by
    /// `solve_forward`. Mirrors the torsion family's engine-computed status.
    pub status: DesignStatus,
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
    allowable_pct_end_torsion: f64,
    allowable_pct_bending: f64,
    correction: CurvatureCorrection,
) -> ExtLoadPoint {
    // Extension deflection y = max(0, (F − F_i) / k) (Shigley extension springs).
    let y = deflection(force, initial_tension, rate);
    // Extension springs lengthen under load: L = L0 + y.
    let length = Length::from_meters(free_length.meters() + y.meters());
    // Body shear stress with selected curvature correction (Shigley Eq. 10-7).
    let body_shear = corrected_shear_stress(force, mean_dia, wire_dia, correction.factor(index));
    // Hook stresses (Shigley extension spring hook curvature factors).
    let hook_bending = hook_bending_stress(force, mean_dia, wire_dia, hooks.r1);
    let hook_torsion = hook_torsion_stress(force, mean_dia, wire_dia, hooks.r2);

    let allow_torsion = allowable_pct_torsion * mts.pascals();
    let allow_end_torsion = allowable_pct_end_torsion * mts.pascals();
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
        // End hooks in torsion use the per-material end-hook allowable (Shigley
        // Table 10-7: 40% for carbon/low-alloy steel, 30% for stainless/nonferrous).
        pct_hook_torsion_allow: hook_torsion.pascals() / allow_end_torsion,
    }
}

/// Engineering checks for an extension design: per-load-point overstress on each
/// of the three stresses, plus the shared spring-index caution. Mirrors the
/// torsion `evaluate_status` precedent.
fn evaluate_status(index: f64, load_points: &[ExtLoadPoint]) -> DesignStatus {
    let mut messages = Vec::new();
    for (i, lp) in load_points.iter().enumerate() {
        if lp.pct_body_allow > 1.0 {
            messages.push(StatusMessage {
                severity: Severity::Warning,
                message: format!(
                    "load point {}: body shear stress is {:.0}% of allowable",
                    i + 1,
                    lp.pct_body_allow * 100.0
                ),
            });
        }
        if lp.pct_hook_bending_allow > 1.0 {
            messages.push(StatusMessage {
                severity: Severity::Warning,
                message: format!(
                    "load point {}: hook bending stress is {:.0}% of allowable",
                    i + 1,
                    lp.pct_hook_bending_allow * 100.0
                ),
            });
        }
        if lp.pct_hook_torsion_allow > 1.0 {
            messages.push(StatusMessage {
                severity: Severity::Warning,
                message: format!(
                    "load point {}: hook torsion stress is {:.0}% of allowable",
                    i + 1,
                    lp.pct_hook_torsion_allow * 100.0
                ),
            });
        }
    }
    if let Some(msg) = crate::design::index_caution(index) {
        messages.push(msg);
    }
    DesignStatus { messages }
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
    correction: CurvatureCorrection,
) -> Result<ExtensionDesign> {
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
    // Spring index must exceed 1 (mean_dia > wire_dia) for a physically valid spring.
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
    // Free length must be finite and positive; a non-finite L0 propagates into the
    // load-point length (L = L0 + y).
    if !(free_length.meters().is_finite() && free_length.meters() > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "free length must be a positive finite number".into(),
        ));
    }
    // Initial tension is a built-in preload; it must be finite and non-negative.
    // (`< 0.0` alone would admit NaN and +Inf, which then flow into deflection.)
    if !(initial_tension.newtons().is_finite() && initial_tension.newtons() >= 0.0) {
        return Err(SpringError::InconsistentInputs(
            "initial tension must be a non-negative finite number".into(),
        ));
    }
    // Every load must be finite and non-negative. A non-finite load makes stresses
    // and deflection NaN/Inf (a NaN deflection would be silently masked to zero by
    // the y clamp); a negative load drives the signed-force stress formulas to
    // negative stresses and %-allowable ratios that never exceed the limit, silently
    // hiding overstress. Zero is allowed (a valid free-state reference point).
    if loads
        .iter()
        .any(|f| !f.newtons().is_finite() || f.newtons() < 0.0)
    {
        return Err(SpringError::InconsistentInputs(
            "loads must be finite and non-negative".into(),
        ));
    }
    // Validate the wire diameter against the material's manufacturable range before
    // the hook-radius checks, so an out-of-range diameter surfaces as
    // DiameterOutOfRange (the most actionable error) rather than a hook-radius error
    // — matching the compression solver's ordering.
    let mts = material.min_tensile_strength(wire_dia)?;
    // Hook curvature index C1 = 2·r1/d must be finite and exceed 1; at C1 ≤ 1 the
    // bending factor (K_A) denominator 4·C1·(C1−1) goes to zero or negative, and a
    // non-finite r1 (or the `<= 1.0` comparison failing for NaN/+Inf) must not slip
    // through to produce NaN/Inf hook stress.
    let c1 = 2.0 * hooks.r1.meters() / wire_dia.meters();
    if !(c1.is_finite() && c1 > 1.0) {
        return Err(SpringError::InconsistentInputs(
            "hook bend radius r1 must exceed d/2 (curvature index C1 = 2·r1/d must exceed 1)"
                .into(),
        ));
    }
    // Hook curvature index C2 = 2·r2/d must be finite and exceed 1; at C2 ≤ 1 the
    // torsion factor (K_B) denominator (4·C2−4) goes to zero or negative.
    let c2 = 2.0 * hooks.r2.meters() / wire_dia.meters();
    if !(c2.is_finite() && c2 > 1.0) {
        return Err(SpringError::InconsistentInputs(
            "hook bend radius r2 must exceed d/2 (curvature index C2 = 2·r2/d must exceed 1)"
                .into(),
        ));
    }

    let index = spring_index(mean_dia, wire_dia);
    let rate = spring_rate(material.shear_modulus, wire_dia, mean_dia, active);

    let load_points: Vec<ExtLoadPoint> = loads
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
                material.allowable_pct_end_torsion,
                material.allowable_pct_bending,
                correction,
            )
        })
        .collect();

    let status = evaluate_status(index, &load_points);
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
        status,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension::ends::HookEnds;
    use crate::units::{Force, Length, Stress};
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
            crate::CurvatureCorrection::Bergstrasser,
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
        // OD = D + d = 22 mm; ID = D − d = 18 mm.
        assert_relative_eq!(d.outer_dia.millimeters(), 22.0, max_relative = 1e-9);
        assert_relative_eq!(d.inner_dia.millimeters(), 18.0, max_relative = 1e-9);
    }

    /// Zero initial tension is valid (coils separate immediately under any load);
    /// the guard rejects only strictly negative preload.
    #[test]
    fn accepts_zero_initial_tension() {
        let m = crate::test_support::music_wire();
        let d = solve_forward(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(60.0),
            Force::from_newtons(0.0),
            HookEnds::default_for(Length::from_millimeters(20.0)),
            &[Force::from_newtons(30.0)],
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        // With F_i = 0, deflection = F/k = 30/2000 = 15 mm.
        assert_relative_eq!(
            d.load_points[0].deflection.millimeters(),
            15.0,
            max_relative = 1e-9
        );
    }

    #[test]
    fn rejects_negative_initial_tension() {
        let m = crate::test_support::music_wire();
        let r = solve_forward(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(60.0),
            Force::from_newtons(-1.0),
            HookEnds::default_for(Length::from_millimeters(20.0)),
            &[Force::from_newtons(30.0)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    /// +Inf initial tension passes a bare `< 0.0` check yet flows into deflection.
    #[test]
    fn rejects_non_finite_initial_tension() {
        let m = crate::test_support::music_wire();
        let r = solve_forward(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(60.0),
            Force::from_newtons(f64::INFINITY),
            HookEnds::default_for(Length::from_millimeters(20.0)),
            &[Force::from_newtons(30.0)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    /// Non-finite free length propagates into the load-point length L = L0 + y.
    #[test]
    fn rejects_non_finite_free_length() {
        let m = crate::test_support::music_wire();
        let r = solve_forward(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(f64::INFINITY),
            Force::from_newtons(10.0),
            HookEnds::default_for(Length::from_millimeters(20.0)),
            &[Force::from_newtons(30.0)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    /// A NaN load would yield NaN stresses and a deflection silently masked to 0.
    #[test]
    fn rejects_non_finite_load() {
        let m = crate::test_support::music_wire();
        let r = solve_forward(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(60.0),
            Force::from_newtons(10.0),
            HookEnds::default_for(Length::from_millimeters(20.0)),
            &[Force::from_newtons(f64::NAN)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    /// A negative load drives the signed-force stress formulas to negative stresses
    /// and %-allowable ratios that never exceed the limit, hiding overstress.
    #[test]
    fn rejects_negative_load() {
        let m = crate::test_support::music_wire();
        let r = solve_forward(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(60.0),
            Force::from_newtons(10.0),
            HookEnds::default_for(Length::from_millimeters(20.0)),
            &[Force::from_newtons(-5.0)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(
            matches!(&r, Err(crate::SpringError::InconsistentInputs(m)) if m == "loads must be finite and non-negative"),
            "expected the loads guard, got {r:?}"
        );
    }

    /// A zero load is allowed: a valid free-state reference point (no force →
    /// no deflection, no stress). Pins the `< 0.0` (not `<= 0.0`) boundary.
    #[test]
    fn accepts_zero_load() {
        let m = crate::test_support::music_wire();
        let d = solve_forward(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(60.0),
            Force::from_newtons(10.0),
            HookEnds::default_for(Length::from_millimeters(20.0)),
            &[Force::from_newtons(0.0)],
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert_relative_eq!(d.load_points[0].deflection.millimeters(), 0.0);
        assert_relative_eq!(d.load_points[0].body_shear.pascals(), 0.0);
    }

    /// A non-finite hook radius makes C1 = 2·r1/d non-finite; `c1 <= 1.0` would not
    /// reject +Inf, so the finite-and-> 1 guard must catch it.
    #[test]
    fn rejects_non_finite_hook_radius() {
        let m = crate::test_support::music_wire();
        let r = solve_forward(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(60.0),
            Force::from_newtons(10.0),
            HookEnds {
                r1: Length::from_millimeters(f64::INFINITY),
                r2: Length::from_millimeters(5.0),
            },
            &[Force::from_newtons(30.0)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    /// active ≤ 0 would make spring_rate divide by zero/negative → Inf/negative
    /// rate silently flowing into deflection and stresses.
    #[test]
    fn rejects_non_positive_active() {
        let m = crate::test_support::music_wire();
        let r = solve_forward(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            0.0,
            Length::from_millimeters(60.0),
            Force::from_newtons(10.0),
            HookEnds::default_for(Length::from_millimeters(20.0)),
            &[Force::from_newtons(30.0)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    /// An out-of-range wire diameter must surface as DiameterOutOfRange even when a
    /// hook radius is ALSO invalid (C1 ≤ 1). Pins the `min_tensile_strength` ordering:
    /// if it were moved back after the hook-radius checks, this would return the hook
    /// error instead. d=10mm is out of range for music wire; r1=2mm → C1 = 0.4 ≤ 1.
    #[test]
    fn diameter_error_precedes_hook_error() {
        let m = crate::test_support::music_wire();
        let r = solve_forward(
            &m,
            Length::from_millimeters(10.0), // out of range for music wire
            Length::from_millimeters(80.0),
            10.0,
            Length::from_millimeters(60.0),
            Force::from_newtons(10.0),
            HookEnds {
                r1: Length::from_millimeters(2.0), // C1 = 2·2/10 = 0.4 ≤ 1 — also invalid
                r2: Length::from_millimeters(6.0),
            },
            &[Force::from_newtons(30.0)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(
            matches!(r, Err(crate::SpringError::DiameterOutOfRange { .. })),
            "out-of-range diameter must win over the hook-radius error, got {r:?}"
        );
    }

    /// wire_dia = 0 slips past the mean > wire check yet yields a zero rate
    /// (d⁴ = 0) → infinite deflection. Asserts the wire-specific message so the
    /// dedicated wire guard is exercised, not the downstream C1 = 2·r1/d guard
    /// (which would also reject d = 0 but with a misleading hook-radius message).
    #[test]
    fn rejects_zero_wire_dia() {
        let m = crate::test_support::music_wire();
        let r = solve_forward(
            &m,
            Length::from_millimeters(0.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(60.0),
            Force::from_newtons(10.0),
            HookEnds::default_for(Length::from_millimeters(20.0)),
            &[Force::from_newtons(30.0)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(
            matches!(&r, Err(crate::SpringError::InconsistentInputs(m)) if m == "wire diameter must be a positive finite number"),
            "expected the wire-diameter guard, got {r:?}"
        );
    }

    /// free_length = 0 is non-physical; `> 0.0` (not `>= 0.0`) must reject it.
    #[test]
    fn rejects_zero_free_length() {
        let m = crate::test_support::music_wire();
        let r = solve_forward(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(0.0),
            Force::from_newtons(10.0),
            HookEnds::default_for(Length::from_millimeters(20.0)),
            &[Force::from_newtons(30.0)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(
            matches!(&r, Err(crate::SpringError::InconsistentInputs(m)) if m == "free length must be a positive finite number"),
            "expected the free-length guard, got {r:?}"
        );
    }

    /// Non-finite mean_dia (+Inf) slips past `mean <= wire` yet yields a zero
    /// rate (k ∝ 1/dm³) → infinite deflection.
    #[test]
    fn rejects_non_finite_mean_dia() {
        let m = crate::test_support::music_wire();
        let r = solve_forward(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(f64::INFINITY),
            10.0,
            Length::from_millimeters(60.0),
            Force::from_newtons(10.0),
            HookEnds::default_for(Length::from_millimeters(20.0)),
            &[Force::from_newtons(30.0)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
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
            crate::CurvatureCorrection::Bergstrasser,
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
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    // ── status: clean baseline ───────────────────────────────────────────────
    #[test]
    fn clean_design_has_no_warnings() {
        // d=2mm D=20mm index=10 (in 4..=12 band), moderate load → no overstress.
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
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert!(!d.status.has_warnings(), "clean design must have no warnings: {:?}", d.status.messages);
        assert!(d.status.messages.is_empty(), "clean in-band design has an empty status");
    }

    // ── status: index caution (Severity::Caution) ────────────────────────────
    #[test]
    fn out_of_band_index_raises_caution() {
        // d=2mm D=40mm → index 20 (> 12). Load kept small so only the index caution fires.
        let m = crate::test_support::music_wire();
        let d = solve_forward(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(40.0),
            10.0,
            Length::from_millimeters(60.0),
            Force::from_newtons(1.0),
            HookEnds::default_for(Length::from_millimeters(40.0)),
            &[Force::from_newtons(2.0)],
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert!(
            d.status.messages.iter().any(|msg|
                msg.severity == crate::Severity::Caution && msg.message.contains("spring index")),
            "index 20 must raise a Caution, got: {:?}", d.status.messages
        );
    }

    // ── status: overstress per stress, load-point indexed (Severity::Warning) ──
    /// Build a design whose single load point overstresses ALL THREE stresses, then
    /// assert each stress produces a distinct indexed Warning naming that stress.
    fn overstressed() -> ExtensionDesign {
        // Music wire d=1mm D=8mm index=8; huge load → all three pct_* exceed 1.0.
        let m = crate::test_support::music_wire();
        solve_forward(
            &m,
            Length::from_millimeters(1.0),
            Length::from_millimeters(8.0),
            10.0,
            Length::from_millimeters(60.0),
            Force::from_newtons(0.0),
            HookEnds::default_for(Length::from_millimeters(8.0)),
            &[Force::from_newtons(500.0)],
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap()
    }

    #[test]
    fn body_overstress_raises_indexed_warning() {
        let d = overstressed();
        assert!(d.load_points[0].pct_body_allow > 1.0, "fixture must overstress body shear");
        assert!(
            d.status.messages.iter().any(|msg|
                msg.severity == crate::Severity::Warning
                && msg.message.contains("load point 1")
                && msg.message.contains("body shear")),
            "expected indexed body-shear warning, got: {:?}", d.status.messages
        );
    }

    #[test]
    fn hook_bending_overstress_raises_indexed_warning() {
        let d = overstressed();
        assert!(d.load_points[0].pct_hook_bending_allow > 1.0, "fixture must overstress hook bending");
        assert!(
            d.status.messages.iter().any(|msg|
                msg.severity == crate::Severity::Warning
                && msg.message.contains("load point 1")
                && msg.message.contains("hook bending")),
            "expected indexed hook-bending warning, got: {:?}", d.status.messages
        );
    }

    #[test]
    fn hook_torsion_overstress_raises_indexed_warning() {
        let d = overstressed();
        assert!(d.load_points[0].pct_hook_torsion_allow > 1.0, "fixture must overstress hook torsion");
        assert!(
            d.status.messages.iter().any(|msg|
                msg.severity == crate::Severity::Warning
                && msg.message.contains("load point 1")
                && msg.message.contains("hook torsion")),
            "expected indexed hook-torsion warning, got: {:?}", d.status.messages
        );
    }

    /// Boundary: pct == 1.0 exactly (at the allowable, not over it) must NOT warn.
    /// Kills the `> → >=` mutant on each of the three comparisons. Drives
    /// `evaluate_status` directly with a hand-built load point at the boundary.
    #[test]
    fn exactly_at_allowable_raises_no_overstress_warning() {
        let lp = ExtLoadPoint {
            force: Force::from_newtons(1.0),
            deflection: Length::from_meters(0.0),
            length: Length::from_meters(0.06),
            body_shear: Stress::from_pascals(1.0),
            hook_bending: Stress::from_pascals(1.0),
            hook_torsion: Stress::from_pascals(1.0),
            pct_body_allow: 1.0,
            pct_hook_bending_allow: 1.0,
            pct_hook_torsion_allow: 1.0,
        };
        // index 10 is in-band → no caution either, so a clean status is expected.
        let status = evaluate_status(10.0, std::slice::from_ref(&lp));
        assert!(!status.has_warnings(), "pct == 1.0 must not warn: {:?}", status.messages);
    }

    /// Default hooks give r2 = D/4; with d = D/2 → C2 = 2·(D/4)/(D/2) = 1 —
    /// torsion factor denominator hits zero. Uses an in-range wire diameter so the
    /// C2 guard fires (not the diameter-range check, which now precedes it).
    #[test]
    fn rejects_default_hooks_low_index_spring() {
        let m = crate::test_support::music_wire();
        // d = 5 mm (in range), D = 10 mm → index 2; default_for(10mm) → r1=5mm (C1=2), r2=2.5mm (C2=1).
        let r = solve_forward(
            &m,
            Length::from_millimeters(5.0),
            Length::from_millimeters(10.0),
            10.0,
            Length::from_millimeters(60.0),
            Force::from_newtons(10.0),
            HookEnds::default_for(Length::from_millimeters(10.0)),
            &[Force::from_newtons(30.0)],
            crate::CurvatureCorrection::Bergstrasser,
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    /// The selected correction factor governs extension body shear (only the
    /// body term; the hook σ_A/τ_B factors are independent).
    #[test]
    fn solve_forward_uses_selected_correction() {
        let m = crate::test_support::music_wire();
        let mk = |corr| {
            solve_forward(
                &m,
                Length::from_millimeters(2.0),
                Length::from_millimeters(20.0),
                10.0,
                Length::from_millimeters(60.0),
                Force::from_newtons(10.0),
                HookEnds::default_for(Length::from_millimeters(20.0)),
                &[Force::from_newtons(30.0)],
                corr,
            )
            .unwrap()
            .load_points[0]
                .body_shear
                .pascals()
        };
        assert_relative_eq!(
            mk(crate::CurvatureCorrection::Wahl) / mk(crate::CurvatureCorrection::Bergstrasser),
            crate::mechanics::wahl_factor(10.0) / crate::mechanics::bergstrasser_factor(10.0),
            max_relative = 1e-12
        );
    }

    /// Pin the pct-allowable denominator mapping: body shear uses
    /// `allowable_pct_torsion`; hook-torsion (τ_B) uses the lower
    /// `allowable_pct_end_torsion` (Shigley Table 10-7); hook-bending uses
    /// `allowable_pct_bending`. Swapping any of the three fractions fails this test.
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
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        let lp = &d.load_points[0];
        let mts = m.min_tensile_strength(wire_dia).unwrap();

        let expected_body = lp.body_shear.pascals() / (m.allowable_pct_torsion * mts.pascals());
        let expected_hook_torsion =
            lp.hook_torsion.pascals() / (m.allowable_pct_end_torsion * mts.pascals());
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
        // Sanity-check: the three allowable percentages differ, so the test is
        // discriminating — a swap of any denominator would change the values.
        assert_ne!(m.allowable_pct_torsion, m.allowable_pct_bending);
        assert_ne!(m.allowable_pct_torsion, m.allowable_pct_end_torsion);
    }
}
