//! Aggregate forward solve: from fully-determined geometry to a complete design,
//! plus engineering status checks. Formula sources cited at each call site.

use crate::end_type::EndType;
use crate::material::Material;
use crate::mechanics::{
    corrected_shear_stress, is_buckling_stable, natural_frequency, spring_index, spring_rate,
    wahl_factor, EndFixity,
};
use crate::units::{Force, Frequency, Length, SpringRate, Stress};
use crate::Result;

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

fn load_point(
    force: Force,
    rate: SpringRate,
    free_length: Length,
    mean_dia: Length,
    wire_dia: Length,
    index: f64,
    mts: Stress,
) -> LoadPoint {
    // Deflection y = F/k (Shigley Eq. 10-9 rearranged).
    let y = force.newtons() / rate.newtons_per_meter();
    let length = Length::from_meters(free_length.meters() - y);
    let stress = corrected_shear_stress(force, mean_dia, wire_dia, wahl_factor(index));
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
) -> Result<SpringDesign> {
    let index = spring_index(mean_dia, wire_dia);
    let rate = spring_rate(material.shear_modulus, wire_dia, mean_dia, active);
    let total_coils = end_type.total_coils(active);
    let solid_length = end_type.solid_length(wire_dia, active);
    let pitch = end_type.pitch_from_free_length(wire_dia, active, free_length);
    let mts = material.min_tensile_strength(wire_dia)?;
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
        .map(|&f| load_point(f, rate, free_length, mean_dia, wire_dia, index, mts))
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
        )
        .unwrap();
        let status = evaluate_status(&design, &m);
        assert!(status
            .messages
            .iter()
            .any(|msg| msg.message.contains("index")));
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
        )
        .unwrap();
        let status = evaluate_status(&design, &m);
        assert!(status.has_warnings());
    }
}
