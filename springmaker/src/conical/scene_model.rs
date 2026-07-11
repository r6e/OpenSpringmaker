//! Pure 3D scene presenter for the conical family: a linearly tapering helix
//! (large end down), same flattened-end-coil treatment as compression.

use crate::viz::{coil_height_fn, helix, stroke_for, Polyline3, SceneData, SceneRole};
use springcore::conical::ConicalDesign;

pub fn conical_scene(design: &ConicalDesign) -> SceneData {
    let r_large = design.inputs.large_mean_dia.millimeters() / 2.0;
    let r_small = design.inputs.small_mean_dia.millimeters() / 2.0;
    let wire = design.inputs.wire_dia.millimeters();
    let total = design.total_coils;
    let height = coil_height_fn(
        design.inputs.active_coils,
        total,
        design.pitch.millimeters(),
        wire,
    );
    let max_h = height(1.0);
    let extent = (2.0 * r_large).max(max_h);
    let points = helix(|t| r_large + (r_small - r_large) * t, height, total, 32);
    SceneData {
        polylines: vec![Polyline3 {
            points,
            role: SceneRole::Wire,
            stroke_px: stroke_for(wire, extent),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conical::form::ConFormState;
    use approx::assert_relative_eq;
    use springcore::{CurvatureCorrection, MaterialSet, MaterialStore, UnitSystem};

    fn store() -> MaterialStore {
        MaterialStore::new(MaterialSet::load_default())
    }

    /// Golden metric fixture from conical/plot_model.rs tests.
    fn design() -> springcore::conical::ConicalDesign {
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
    fn conical_scene_tapers_large_to_small() {
        let d = design(); // wire 2, large 20, small 12, active 10, free 60
        let s = conical_scene(&d);
        let line = &s.polylines[0];
        let first = line.points[0];
        let last = *line.points.last().unwrap();
        // Bottom = large end (10 mm), top = small end (6 mm) — engine-field pins.
        assert_relative_eq!(
            (first.0.powi(2) + first.2.powi(2)).sqrt(),
            d.inputs.large_mean_dia.millimeters() / 2.0,
            max_relative = 1e-9
        );
        assert_relative_eq!(
            (last.0.powi(2) + last.2.powi(2)).sqrt(),
            d.inputs.small_mean_dia.millimeters() / 2.0,
            max_relative = 1e-9
        );
    }
}
