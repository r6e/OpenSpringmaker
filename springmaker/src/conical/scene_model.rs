//! Pure 3D scene presenter for the conical family: a linearly tapering helix
//! (large end down), same flattened-end-coil treatment as compression.

use crate::viz::{scene_from_radius, SceneData};
use springcore::conical::ConicalDesign;

pub fn conical_scene(design: &ConicalDesign) -> SceneData {
    let r_large = design.inputs.large_mean_dia.millimeters() / 2.0;
    let r_small = design.inputs.small_mean_dia.millimeters() / 2.0;
    scene_from_radius(
        |t| r_large + (r_small - r_large) * t,
        r_large,
        design.inputs.active_coils,
        design.total_coils,
        design.pitch.millimeters(),
        design.inputs.wire_dia.millimeters(),
    )
}

// No degenerate-design test here: NaN gating is shared (`viz::scene_extent`)
// and covered by compression's degenerate test plus viz's own tests.
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
            inactive: String::new(),
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

    /// Symmetry pin (panel-R2 test matrix): a coil count past the helix
    /// render cap forwards the sampler's empty body unchanged — extent
    /// `None`, placeholder — at THIS family's scene level, not just inside
    /// the shared sampler.
    #[test]
    fn capped_coils_yield_degenerate_scene() {
        let mut d = design();
        d.inputs.active_coils = 2001.0;
        d.total_coils = 2003.0;
        let s = conical_scene(&d);
        assert!(crate::viz::scene_extent(&s).is_none());
    }
}
