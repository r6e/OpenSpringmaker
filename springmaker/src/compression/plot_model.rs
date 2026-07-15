//! Pure chart presenter for the compression family (ADR 0008).

use crate::plot::{
    round_wire_force_deflection, shear_stress_axes, stress_display, ChartData, Line, LineRole,
    Marker, MarkerKind,
};
use springcore::{SpringDesign, UnitSystem};

pub fn compression_chart(design: &SpringDesign, units: UnitSystem) -> ChartData {
    round_wire_force_deflection(design.rate, &design.load_points, &design.at_solid, units)
}

/// Goodman fatigue diagram (Shigley §10-9): straight envelope (0,Se)→(Ssu,0),
/// load line from the origin through the operating point to the envelope.
/// All values come from `FatigueResult`; no engineering formula is re-derived
/// here — the intersection is line/line geometry on the engine's numbers.
pub fn goodman_chart(f: &springcore::FatigueResult, units: UnitSystem) -> ChartData {
    let (x_axis, y_axis) = shear_stress_axes(units);
    let se = stress_display(f.fully_reversed_endurance, units);
    let ssu = stress_display(f.ultimate_shear, units);
    let ta = stress_display(f.alternating_stress, units);
    let tm = stress_display(f.mean_stress, units);
    // Defense in depth: the engine's output guard makes these finite-positive,
    // but the presenter re-checks before building geometry.
    if !(se.is_finite()
        && se > 0.0
        && ssu.is_finite()
        && ssu > 0.0
        && ta.is_finite()
        && tm.is_finite())
    {
        return ChartData {
            x_axis,
            y_axis,
            lines: vec![],
            markers: vec![],
        };
    }

    let envelope = Line {
        points: vec![(0.0, se), (ssu, 0.0)],
        role: LineRole::Envelope,
        name: None,
    };
    // Load line endpoint on the envelope: vertical when τm = 0, else
    // x* = Se·Ssu/(Se + r·Ssu) with r = τa/τm.
    let (lx, ly) = if tm <= 0.0 {
        (0.0, se)
    } else {
        let r = ta / tm;
        let x_star = se * ssu / (se + r * ssu);
        (x_star, r * x_star)
    };
    let load_line = Line {
        points: vec![(0.0, 0.0), (lx, ly)],
        role: LineRole::LoadLine,
        name: None,
    };

    ChartData {
        x_axis,
        y_axis,
        lines: vec![envelope, load_line],
        markers: vec![
            Marker {
                x: tm,
                y: ta,
                kind: MarkerKind::Operating,
            },
            Marker {
                x: lx,
                y: ly,
                kind: MarkerKind::Limit,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use springcore::units::{Force, Length, Stress};
    use springcore::{EndFixity, EndType, MaterialSet, PowerUser, Scenario};

    fn design() -> SpringDesign {
        let m = MaterialSet::load_default()
            .get("Music Wire")
            .unwrap()
            .clone();
        PowerUser {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            active: 10.0,
            inactive_coils: None,
            free_length: Length::from_millimeters(60.0),
            loads: vec![Force::from_newtons(10.0), Force::from_newtons(30.0)],
        }
        .solve(&m, springcore::CurvatureCorrection::Bergstrasser)
        .unwrap()
    }

    #[test]
    fn line_is_origin_to_max_load_and_markers_are_operating_points() {
        let d = compression_chart(&design(), UnitSystem::Metric);
        assert_eq!(d.lines.len(), 1);
        assert_eq!(d.lines[0].role, LineRole::Primary);
        let pts = &d.lines[0].points;
        assert_relative_eq!(pts[0].0, 0.0, max_relative = 1e-12);
        assert_relative_eq!(pts[0].1, 0.0, max_relative = 1e-12);
        // 30 N / 2000 N/m = 15 mm (the legacy plot.rs golden).
        assert_relative_eq!(pts[1].0, 15.0, max_relative = 1e-6);
        assert_relative_eq!(pts[1].1, 30.0, max_relative = 1e-6);
        assert_eq!(d.markers.len(), 2);
        assert!(d.markers.iter().all(|m| m.kind == MarkerKind::Operating));
        assert_eq!(d.x_axis.label, "deflection (mm)");
        assert_eq!(d.y_axis.unit, "N");
    }

    #[test]
    fn us_units_convert_axes_and_points() {
        let d = compression_chart(&design(), UnitSystem::Us);
        assert_eq!(d.x_axis.label, "deflection (in)");
        assert_eq!(d.y_axis.unit, "lbf");
        // 30 N = 6.7443 lbf; 15 mm = 0.59055 in.
        let last = *d.lines[0].points.last().unwrap();
        assert_relative_eq!(last.0, 0.5905511811, max_relative = 1e-6);
        assert_relative_eq!(last.1, 6.744268797, max_relative = 1e-4);
    }

    #[test]
    fn zero_rate_design_yields_degenerate_chart() {
        // Presenter must not panic and must yield no positive extent when the
        // rate is non-positive (defense in depth; the engine guards this).
        let mut d = design();
        d.rate = springcore::SpringRate::from_newtons_per_meter(0.0);
        let data = compression_chart(&d, UnitSystem::Metric);
        assert!(crate::plot::chart_extent(&data).is_none());
    }

    fn fatigue_result() -> springcore::FatigueResult {
        // Synthetic but physically-shaped values; the presenter is pure geometry.
        springcore::FatigueResult {
            alternating_stress: Stress::from_megapascals(100.0),
            mean_stress: Stress::from_megapascals(200.0),
            fully_reversed_endurance: Stress::from_megapascals(310.0),
            ultimate_shear: Stress::from_megapascals(1130.0),
            goodman_factor_of_safety: 2.0,
        }
    }

    #[test]
    fn goodman_axes_are_labeled_as_shear_stress() {
        // Compression's FatigueResult quantities are torsional shear, not
        // normal stress — the axes must read τ, not σ.
        let d = goodman_chart(&fatigue_result(), UnitSystem::Metric);
        assert_eq!(d.x_axis.symbol, "τm");
        assert_eq!(d.x_axis.label, "mean shear stress (MPa)");
        assert_eq!(d.y_axis.symbol, "τa");
        assert_eq!(d.y_axis.label, "alternating shear stress (MPa)");
    }

    #[test]
    fn goodman_us_axes_and_envelope_are_ksi_not_psi() {
        // App-wide US stress convention is ksi (presenter::display_stress),
        // not psi — pin both the axis units/labels and a converted value
        // against the engine accessor, not a hand constant.
        let d = goodman_chart(&fatigue_result(), UnitSystem::Us);
        assert_eq!(d.x_axis.unit, "ksi");
        assert_eq!(d.x_axis.label, "mean shear stress (ksi)");
        assert_eq!(d.y_axis.unit, "ksi");
        assert_eq!(d.y_axis.label, "alternating shear stress (ksi)");
        let env = d
            .lines
            .iter()
            .find(|l| l.role == LineRole::Envelope)
            .unwrap();
        let se_ksi = Stress::from_megapascals(310.0).psi() / 1000.0;
        assert_relative_eq!(env.points[0].1, se_ksi, max_relative = 1e-9); // (0, Se) in ksi
    }

    #[test]
    fn goodman_envelope_runs_se_to_ssu() {
        let d = goodman_chart(&fatigue_result(), UnitSystem::Metric);
        let env = d
            .lines
            .iter()
            .find(|l| l.role == LineRole::Envelope)
            .unwrap();
        assert_eq!(d.lines.len(), 2);
        assert_eq!(d.markers.len(), 2);
        assert_eq!(env.points.len(), 2);
        assert_relative_eq!(env.points[0].0, 0.0, max_relative = 1e-12);
        assert_relative_eq!(env.points[0].1, 310.0, max_relative = 1e-9); // (0, Se)
        assert_relative_eq!(env.points[1].0, 1130.0, max_relative = 1e-9); // (Ssu, 0)
        assert_relative_eq!(env.points[1].1, 0.0, max_relative = 1e-12);
    }

    #[test]
    fn goodman_load_line_ends_on_the_envelope() {
        // Shigley §10-9: load line y = r·x, r = τa/τm, meets x/Ssu + y/Se = 1 at
        // x* = Se·Ssu/(Se + r·Ssu).
        let f = fatigue_result();
        let d = goodman_chart(&f, UnitSystem::Metric);
        assert_eq!(d.lines.len(), 2);
        assert_eq!(d.markers.len(), 2);
        let ll = d
            .lines
            .iter()
            .find(|l| l.role == LineRole::LoadLine)
            .unwrap();
        let r: f64 = 100.0 / 200.0;
        let x_star: f64 = 310.0 * 1130.0 / (310.0 + r * 1130.0);
        let end = *ll.points.last().unwrap();
        assert_relative_eq!(end.0, x_star, max_relative = 1e-9);
        assert_relative_eq!(end.1, r * x_star, max_relative = 1e-9);
        // Marker check: Operating at (τm, τa); Limit at the envelope intersection.
        assert!(d.markers.iter().any(|m| m.kind == MarkerKind::Operating
            && (m.x - 200.0).abs() < 1e-9
            && (m.y - 100.0).abs() < 1e-9));
        assert!(d
            .markers
            .iter()
            .any(|m| m.kind == MarkerKind::Limit && (m.x - x_star).abs() < 1e-6));
    }

    #[test]
    fn goodman_zero_mean_stress_is_a_vertical_load_line() {
        let mut f = fatigue_result();
        f.mean_stress = Stress::from_megapascals(0.0);
        let d = goodman_chart(&f, UnitSystem::Metric);
        assert_eq!(d.lines.len(), 2);
        assert_eq!(d.markers.len(), 2);
        let ll = d
            .lines
            .iter()
            .find(|l| l.role == LineRole::LoadLine)
            .unwrap();
        let end = *ll.points.last().unwrap();
        assert_relative_eq!(end.0, 0.0, max_relative = 1e-12);
        assert_relative_eq!(end.1, 310.0, max_relative = 1e-9); // straight up to Se
    }

    #[test]
    fn goodman_zero_alternating_stress_load_line_runs_to_ssu() {
        // r = 0 (equal cycle forces — a legal engine output): the load line
        // lies on the x-axis and meets the envelope at (Ssu, 0).
        let mut f = fatigue_result();
        f.alternating_stress = Stress::from_megapascals(0.0);
        let d = goodman_chart(&f, UnitSystem::Metric);
        assert_eq!(d.lines.len(), 2);
        assert_eq!(d.markers.len(), 2);
        let ll = d
            .lines
            .iter()
            .find(|l| l.role == LineRole::LoadLine)
            .unwrap();
        let end = *ll.points.last().unwrap();
        assert_relative_eq!(end.0, 1130.0, max_relative = 1e-9); // x* = Se·Ssu/Se = Ssu
        assert_relative_eq!(end.1, 0.0, max_relative = 1e-12);
        assert!(d.markers.iter().any(|m| m.kind == MarkerKind::Operating
            && (m.x - 200.0).abs() < 1e-9
            && m.y.abs() < 1e-12));
        assert!(d.markers.iter().any(|m| m.kind == MarkerKind::Limit
            && (m.x - 1130.0).abs() < 1e-9
            && m.y.abs() < 1e-12));
    }

    #[test]
    fn goodman_zero_endurance_is_degenerate() {
        let mut f = fatigue_result();
        f.fully_reversed_endurance = Stress::from_megapascals(0.0);
        let d = goodman_chart(&f, UnitSystem::Metric);
        assert_eq!(d.lines.len(), 0);
        assert_eq!(d.markers.len(), 0);
        assert!(crate::plot::chart_extent(&d).is_none());
    }
}
