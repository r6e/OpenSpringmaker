//! Pure chart presenter for the torsion family (ADR 0008): moment vs angle.
//! The line is the ideal rate line θ = M/k′; markers read the engine's SOLVED
//! `deflection` field rather than recomputing M/k. With the current linear
//! engine those coincide, but the presenter must not bake that in — a test
//! pins that the field is read, not derived.

use crate::plot::{AxisMeta, ChartData, Line, LineRole, Marker, MarkerKind};
use springcore::torsion::TorsionDesign;
use springcore::UnitSystem;

pub fn torsion_chart(design: &TorsionDesign, units: UnitSystem) -> ChartData {
    let x_axis = AxisMeta {
        label: "angle (deg)",
        symbol: "θ",
        unit: "°",
    };
    let y_axis = match units {
        UnitSystem::Metric => AxisMeta {
            label: "moment (N·mm)",
            symbol: "M",
            unit: "N·mm",
        },
        UnitSystem::Us => AxisMeta {
            label: "moment (lbf·in)",
            symbol: "M",
            unit: "lbf·in",
        },
    };
    let display_moment = |m: springcore::units::Moment| match units {
        UnitSystem::Metric => m.newton_millimeters(),
        UnitSystem::Us => m.pound_force_inches(),
    };

    let max_m_nm = design
        .load_points
        .iter()
        .map(|lp| lp.moment.newton_meters())
        .fold(0.0_f64, f64::max);
    let k = design.rate.newton_meters_per_degree();
    let rate_ok = k.is_finite() && k > 0.0;
    let lines = if rate_ok && max_m_nm.is_finite() && max_m_nm > 0.0 {
        let theta = max_m_nm / k;
        let max_m = design
            .load_points
            .iter()
            .map(|lp| display_moment(lp.moment))
            .fold(0.0_f64, f64::max);
        vec![Line {
            points: vec![(0.0, 0.0), (theta, max_m)],
            role: LineRole::Primary,
            name: None,
        }]
    } else {
        vec![]
    };
    // A non-positive or non-finite rate makes the whole design degenerate, not
    // just the line: the solved deflections came from that rate, so markers are
    // suppressed too, keeping `chart_extent` None and the chart out of plotters
    // (round_wire_force_deflection convention).
    let markers = if rate_ok {
        design
            .load_points
            .iter()
            .map(|lp| Marker {
                x: lp.deflection.degrees(),
                y: display_moment(lp.moment),
                kind: MarkerKind::Operating,
            })
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
    use crate::app::App;
    use crate::torsion::form::TorFormState;
    use approx::assert_relative_eq;
    use springcore::{CurvatureCorrection, Family, MaterialSet, MaterialStore, UnitSystem};

    fn store() -> MaterialStore {
        MaterialStore::new(MaterialSet::load_default())
    }

    /// Mirror the golden metric fixture from torsion/view_model.rs tests.
    fn design() -> TorsionDesign {
        let mut app = App::from_store(store(), Vec::new(), CurvatureCorrection::Bergstrasser);
        app.family = Family::Torsion;
        app.torsion = TorFormState {
            wire_dia: "2".to_string(),
            mean_dia: "20".to_string(),
            body_coils: "5".to_string(),
            leg1: "0".to_string(),
            leg2: "0".to_string(),
            moments: "500, 1000".to_string(),
            ..Default::default()
        };
        app.recompute();
        app.tor_outcome.expect("torsion solve must succeed").design
    }

    #[test]
    fn ideal_line_ends_at_max_moment_over_rate() {
        let d = design();
        let data = torsion_chart(&d, UnitSystem::Metric);
        let max_m_nm = d
            .load_points
            .iter()
            .map(|lp| lp.moment.newton_meters())
            .fold(0.0_f64, f64::max);
        let theta_deg = max_m_nm / d.rate.newton_meters_per_degree();
        let last = *data.lines[0].points.last().unwrap();
        assert_relative_eq!(last.0, theta_deg, max_relative = 1e-9);
        // Pin the display value against the ENGINE's max-moment load point,
        // not a recomputation of the presenter's own fold.
        let max_lp = d
            .load_points
            .iter()
            .max_by(|a, b| {
                a.moment
                    .newton_meters()
                    .total_cmp(&b.moment.newton_meters())
            })
            .expect("fixture has load points");
        assert_relative_eq!(
            last.1,
            max_lp.moment.newton_millimeters(),
            max_relative = 1e-9
        );
        assert_eq!(data.x_axis.unit, "°");
        assert_eq!(data.y_axis.unit, "N·mm");
    }

    #[test]
    fn markers_use_solved_deflections_not_the_ideal_line() {
        // The linear engine puts every solved point exactly on M = k′·θ, so a
        // presenter that recomputed markers from the rate would pass a plain
        // field comparison. Mutate the solved deflection off the ideal line:
        // the marker must follow the FIELD, proving it is read, not derived.
        let mut d = design();
        let nudged =
            springcore::units::Angle::from_degrees(d.load_points[0].deflection.degrees() + 3.0);
        d.load_points[0].deflection = nudged;
        let data = torsion_chart(&d, UnitSystem::Metric);
        assert_relative_eq!(data.markers[0].x, nudged.degrees(), max_relative = 1e-12);
        for (m, lp) in data.markers.iter().zip(&d.load_points) {
            assert_relative_eq!(m.x, lp.deflection.degrees(), max_relative = 1e-12);
        }
    }

    #[test]
    fn invalid_rate_yields_degenerate_chart() {
        // Zero rate invalidates the whole design: no line, no markers, and
        // chart_extent must be None so the placeholder path is taken.
        let mut d = design();
        d.rate = springcore::units::AngularRate::from_newton_meters_per_radian(0.0);
        let data = torsion_chart(&d, UnitSystem::Metric);
        assert!(data.lines.is_empty());
        assert!(data.markers.is_empty());
        assert!(crate::plot::chart_extent(&data).is_none());
    }

    #[test]
    fn us_units_convert_moment_axis() {
        let d = design();
        let data = torsion_chart(&d, UnitSystem::Us);
        assert_eq!(data.y_axis.unit, "lbf·in");
        let last = *data.lines[0].points.last().unwrap();
        let max_lbf_in = d
            .load_points
            .iter()
            .map(|lp| lp.moment.pound_force_inches())
            .fold(0.0_f64, f64::max);
        assert_relative_eq!(last.1, max_lbf_in, max_relative = 1e-9);
    }
}
