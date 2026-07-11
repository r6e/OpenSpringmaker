//! Pure chart presenter for the compression family (ADR 0008).

use crate::plot::{round_wire_force_deflection, ChartData};
use springcore::{SpringDesign, UnitSystem};

pub fn compression_chart(design: &SpringDesign, units: UnitSystem) -> ChartData {
    round_wire_force_deflection(design.rate, &design.load_points, &design.at_solid, units)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plot::{LineRole, MarkerKind};
    use approx::assert_relative_eq;
    use springcore::units::{Force, Length};
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
}
