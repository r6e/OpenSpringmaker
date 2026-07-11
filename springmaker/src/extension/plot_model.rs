//! Pure chart presenter for the extension family (ADR 0008). The line makes
//! the initial-tension jump visible: no deflection until F exceeds Fi
//! (engine model: F = Fi + k·y). Fi = 0 is a valid engine design (coils
//! separate immediately under any load — `accepts_zero_initial_tension`):
//! the line then degenerates to a plain rate line, origin to max load, with
//! no vertical jump segment.

use crate::plot::{convert_fd, force_deflection_axes, operating_marker, ChartData, Line, LineRole};
use springcore::extension::ExtensionDesign;
use springcore::UnitSystem;

pub fn extension_chart(design: &ExtensionDesign, units: UnitSystem) -> ChartData {
    let (x_axis, y_axis) = force_deflection_axes(units);
    let fi = design.initial_tension.newtons();
    let k = design.rate.newtons_per_meter();
    let k_ok = k.is_finite() && k > 0.0;
    let fi_ok = fi.is_finite() && fi >= 0.0;
    let valid = k_ok && fi_ok;
    let max_f = design
        .load_points
        .iter()
        .map(|lp| lp.force.newtons())
        .fold(0.0_f64, f64::max);

    let lines = if !valid {
        vec![]
    } else if fi > 0.0 {
        let mut points = vec![(0.0, 0.0), convert_fd(0.0, fi, units)];
        if max_f.is_finite() && max_f > fi {
            points.push(convert_fd((max_f - fi) / k, max_f, units));
        }
        vec![Line {
            points,
            role: LineRole::Primary,
            name: None,
        }]
    } else if max_f.is_finite() && max_f > 0.0 {
        // Fi == 0: no preload jump — a plain rate line. The engine's
        // deflection is exactly F/k at Fi = 0, so this coincides with the
        // solved load points (`accepts_zero_initial_tension`).
        vec![Line {
            points: vec![(0.0, 0.0), convert_fd(max_f / k, max_f, units)],
            role: LineRole::Primary,
            name: None,
        }]
    } else {
        vec![]
    };

    // A degenerate rate or Fi makes the whole design degenerate, not just the
    // line: the solved deflections came from that rate/Fi, so markers are
    // suppressed too, keeping `chart_extent` `None` and the chart out of
    // plotters entirely (`round_wire_force_deflection` convention).
    let markers = if valid {
        design
            .load_points
            .iter()
            .map(|lp| operating_marker(lp.deflection.meters(), lp.force.newtons(), units))
            .collect()
    } else {
        vec![]
    };
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

    /// Design with Fi = 0 (engine-valid: `accepts_zero_initial_tension`) to
    /// test the plain-rate-line shape — no jump segment.
    fn design_with_zero_initial_tension() -> ExtensionDesign {
        let materials = store();
        let form = crate::extension::form::ExtFormState {
            wire_dia: "2".to_string(),
            mean_dia: "20".to_string(),
            active: "10".to_string(),
            free_length: "100".to_string(),
            initial_tension: "0".to_string(),
            loads: "30".to_string(),
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
        // The vertical Fi jump must actually be renderable: chart_extent
        // sees x = 0 (not degenerate) and y = Fi.
        let (x, y) =
            crate::plot::chart_extent(&data).expect("the Fi jump must produce a renderable extent");
        assert_relative_eq!(x, 0.0, max_relative = 1e-12);
        assert_relative_eq!(y, 20.0, max_relative = 1e-9);
    }

    #[test]
    fn zero_initial_tension_draws_plain_rate_line() {
        // Fi = 0 is engine-valid (`accepts_zero_initial_tension`): no jump
        // segment, just a two-point origin -> max-load line, with markers
        // present (their validity hinges on the rate, not Fi).
        let d = design_with_zero_initial_tension();
        let data = extension_chart(&d, UnitSystem::Metric);
        assert_eq!(data.lines.len(), 1);
        let pts = &data.lines[0].points;
        assert_eq!(pts.len(), 2);
        assert_relative_eq!(pts[0].0, 0.0, max_relative = 1e-12);
        assert_relative_eq!(pts[0].1, 0.0, max_relative = 1e-12);
        // Cross-check against the ENGINE's own solved load point, not a
        // recomputation of the presenter's fold.
        let max_lp = d
            .load_points
            .iter()
            .max_by(|a, b| a.force.newtons().total_cmp(&b.force.newtons()))
            .expect("fixture has load points");
        assert_relative_eq!(
            pts[1].0,
            max_lp.deflection.millimeters(),
            max_relative = 1e-9
        );
        assert_relative_eq!(pts[1].1, max_lp.force.newtons(), max_relative = 1e-9);
        assert_eq!(data.markers.len(), d.load_points.len());
    }

    #[test]
    fn nan_initial_tension_yields_no_lines_or_markers() {
        // A NaN Fi is genuinely invalid (never engine-reachable, but the
        // presenter defends in depth): no lines, and markers are suppressed
        // too since the whole design is degenerate.
        let mut d = design();
        d.initial_tension = springcore::units::Force::from_newtons(f64::NAN);
        let data = extension_chart(&d, UnitSystem::Metric);
        assert!(data.lines.is_empty());
        assert!(data.markers.is_empty());
    }

    #[test]
    fn invalid_rate_suppresses_lines_and_markers() {
        // A degenerate rate invalidates the whole design (the
        // `round_wire_force_deflection` convention): no lines, no markers,
        // and chart_extent must be None so the placeholder path is taken.
        let mut d = design();
        d.rate = springcore::SpringRate::from_newtons_per_meter(0.0);
        let data = extension_chart(&d, UnitSystem::Metric);
        assert!(data.lines.is_empty());
        assert!(data.markers.is_empty());
        assert!(crate::plot::chart_extent(&data).is_none());
    }
}
