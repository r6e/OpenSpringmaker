//! Pure chart presenter for the torsion family (ADR 0008): moment vs angle.
//! The line is the IDEAL rate line θ = M/k′; markers use the solved,
//! friction-adjusted deflections and may legitimately sit off the line.

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
    let lines = if k.is_finite() && k > 0.0 && max_m_nm.is_finite() && max_m_nm > 0.0 {
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
    let markers = design
        .load_points
        .iter()
        .map(|lp| Marker {
            x: lp.deflection.degrees(),
            y: display_moment(lp.moment),
            kind: MarkerKind::Operating,
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
        assert_relative_eq!(last.1, max_m_nm * 1000.0, max_relative = 1e-9); // display N·mm
        assert_eq!(data.x_axis.unit, "°");
        assert_eq!(data.y_axis.unit, "N·mm");
    }

    #[test]
    fn markers_use_solved_deflections_not_the_ideal_line() {
        // Friction-adjusted load points may sit off the ideal line — markers must
        // come from the SOLVED deflection field, not from M/k.
        let d = design();
        let data = torsion_chart(&d, UnitSystem::Metric);
        for (m, lp) in data.markers.iter().zip(&d.load_points) {
            assert_relative_eq!(m.x, lp.deflection.degrees(), max_relative = 1e-12);
        }
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
