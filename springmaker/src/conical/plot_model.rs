//! Pure chart presenter for the conical family (ADR 0008).

use crate::plot::ChartData;
use springcore::conical::ConicalDesign;
use springcore::UnitSystem;

pub fn conical_chart(design: &ConicalDesign, units: UnitSystem) -> ChartData {
    crate::plot::round_wire_force_deflection(
        design.rate,
        &design.load_points,
        &design.at_solid,
        units,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conical::form::ConFormState;
    use crate::plot::{LineRole, MarkerKind};
    use approx::assert_relative_eq;
    use springcore::{CurvatureCorrection, MaterialSet, MaterialStore};

    fn store() -> MaterialStore {
        MaterialStore::new(MaterialSet::load_default())
    }

    /// Golden metric fixture from conical/view_model.rs tests.
    fn design() -> ConicalDesign {
        let materials = store();
        let form = ConFormState {
            end_type: "squared_ground".into(),
            wire_dia: "2".into(),
            large_mean_dia: "20".into(),
            small_mean_dia: "12".into(),
            active: "10".into(),
            free_length: "60".into(),
            loads: "10, 25".into(),
        };
        crate::conical::form::parse_and_solve(
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
    fn line_is_origin_to_max_load_and_markers_are_operating_points() {
        let d = design();
        let data = conical_chart(&d, UnitSystem::Metric);
        assert_eq!(data.lines.len(), 1);
        assert_eq!(data.lines[0].role, LineRole::Primary);
        let pts = &data.lines[0].points;
        assert_relative_eq!(pts[0].0, 0.0, max_relative = 1e-12);
        assert_relative_eq!(pts[0].1, 0.0, max_relative = 1e-12);
        // Last point should be at max load.
        let max_f = d
            .load_points
            .iter()
            .map(|lp| lp.force.newtons())
            .fold(0.0_f64, f64::max);
        assert_relative_eq!(pts[pts.len() - 1].1, max_f, max_relative = 1e-6);
        assert_eq!(data.markers.len(), 2);
        assert!(data.markers.iter().all(|m| m.kind == MarkerKind::Operating));
        assert_eq!(data.x_axis.label, "deflection (mm)");
        assert_eq!(data.y_axis.unit, "N");
    }
}
