//! Pure chart presenter for the extension family (ADR 0008). The line makes
//! the initial-tension jump visible: no deflection until F exceeds Fi
//! (engine model: F = Fi + k·y).

use crate::plot::{
    convert_fd, force_deflection_axes, ChartData, Line, LineRole, Marker, MarkerKind,
};
use springcore::extension::ExtensionDesign;
use springcore::UnitSystem;

pub fn extension_chart(design: &ExtensionDesign, units: UnitSystem) -> ChartData {
    let (x_axis, y_axis) = force_deflection_axes(units);
    let fi = design.initial_tension.newtons();
    let k = design.rate.newtons_per_meter();
    let max_f = design
        .load_points
        .iter()
        .map(|lp| lp.force.newtons())
        .fold(0.0_f64, f64::max);

    let mut points = vec![(0.0, 0.0), convert_fd(0.0, fi, units)];
    if k.is_finite() && k > 0.0 && max_f.is_finite() && max_f > fi {
        points.push(convert_fd((max_f - fi) / k, max_f, units));
    }
    let lines = if fi.is_finite() && fi > 0.0 {
        vec![Line {
            points,
            role: LineRole::Primary,
            name: None,
        }]
    } else {
        vec![]
    };
    let markers = design
        .load_points
        .iter()
        .map(|lp| {
            let (x, y) = convert_fd(lp.deflection.meters(), lp.force.newtons(), units);
            Marker {
                x,
                y,
                kind: MarkerKind::Operating,
            }
        })
        .collect();
    ChartData {
        x_axis,
        y_axis,
        lines,
        markers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension::form::parse_and_solve;
    use approx::assert_relative_eq;
    use springcore::{CurvatureCorrection, MaterialSet, MaterialStore, UnitSystem};

    fn store() -> MaterialStore {
        MaterialStore::new(MaterialSet::load_default())
    }

    /// Mirror the golden metric fixture from extension/view_model.rs tests.
    fn design() -> ExtensionDesign {
        let materials = store();
        let form = crate::extension::form::ExtFormState {
            wire_dia: "2".to_string(),
            mean_dia: "20".to_string(),
            active: "10".to_string(),
            free_length: "100".to_string(),
            initial_tension: "5".to_string(),
            loads: "10, 30".to_string(),
            ..Default::default()
        };
        parse_and_solve(
            &form,
            "Music Wire",
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .unwrap()
        .design
    }

    /// Design with loads below initial tension to test two-point line.
    fn design_with_loads_below_fi() -> ExtensionDesign {
        let materials = store();
        let form = crate::extension::form::ExtFormState {
            wire_dia: "2".to_string(),
            mean_dia: "20".to_string(),
            active: "10".to_string(),
            free_length: "100".to_string(),
            initial_tension: "20".to_string(),
            loads: "10".to_string(),
            ..Default::default()
        };
        parse_and_solve(
            &form,
            "Music Wire",
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .unwrap()
        .design
    }

    #[test]
    fn line_jumps_to_initial_tension_then_climbs_to_max_load() {
        let d = design();
        let data = extension_chart(&d, UnitSystem::Metric);
        let pts = &data.lines[0].points;
        assert_eq!(pts.len(), 3);
        assert_relative_eq!(pts[0].0, 0.0, max_relative = 1e-12);
        assert_relative_eq!(pts[0].1, 0.0, max_relative = 1e-12);
        assert_relative_eq!(pts[1].0, 0.0, max_relative = 1e-12);
        // Fi intercept.
        assert_relative_eq!(pts[1].1, d.initial_tension.newtons(), max_relative = 1e-9);
        // Cross-check the third point against the ENGINE's solved load point
        // (not a recomputation of the presenter's own fold): the presenter's
        // (max_f - Fi)/k geometry must land exactly on the engine's solution.
        let max_lp = d
            .load_points
            .iter()
            .max_by(|a, b| a.force.newtons().total_cmp(&b.force.newtons()))
            .expect("fixture has load points");
        assert_relative_eq!(
            pts[2].0,
            max_lp.deflection.millimeters(),
            max_relative = 1e-9
        );
        assert_relative_eq!(pts[2].1, max_lp.force.newtons(), max_relative = 1e-9);
    }

    #[test]
    fn loads_below_initial_tension_yield_two_point_line() {
        // All loads ≤ Fi: no third segment; extent is Fi itself.
        let d = design_with_loads_below_fi();
        let data = extension_chart(&d, UnitSystem::Metric);
        let pts = &data.lines[0].points;
        assert_eq!(pts.len(), 2);
        // The line's extent is the fixture's Fi (20 N).
        assert_relative_eq!(pts[1].1, 20.0, max_relative = 1e-9);
    }

    #[test]
    fn invalid_initial_tension_yields_no_lines() {
        // Fi = 0 is not finite-positive: the presenter must emit no lines
        // (markers may remain — their validity does not hinge on Fi).
        let mut d = design();
        d.initial_tension = springcore::units::Force::from_newtons(0.0);
        let data = extension_chart(&d, UnitSystem::Metric);
        assert!(data.lines.is_empty());
    }
}
