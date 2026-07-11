//! Pure 3D scene presenter for the compression family (ADR 0008): a constant-
//! radius helix over the solved coil counts, dead end coils flattened to
//! wire pitch (data-driven from total−active — correct for every EndType).

use crate::viz::{scene_from_radius, SceneData};
use springcore::SpringDesign;

pub fn compression_scene(design: &SpringDesign) -> SceneData {
    let r = design.mean_dia.millimeters() / 2.0;
    scene_from_radius(
        |_| r,
        r,
        design.active_coils,
        design.total_coils,
        design.pitch.millimeters(),
        design.wire_dia.millimeters(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viz::SceneRole;
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
    fn compression_scene_matches_solved_geometry() {
        let d = design(); // wire 2, mean 20, active 10, free 60, SquaredGround
        let s = compression_scene(&d);
        assert_eq!(s.polylines.len(), 1);
        let line = &s.polylines[0];
        assert_eq!(line.role, SceneRole::Wire);
        // Radius = mean/2 = 10 mm at both ends.
        let first = line.points[0];
        let last = *line.points.last().unwrap();
        assert_relative_eq!(
            (first.0.powi(2) + first.2.powi(2)).sqrt(),
            10.0,
            max_relative = 1e-9
        );
        assert_relative_eq!(
            (last.0.powi(2) + last.2.powi(2)).sqrt(),
            10.0,
            max_relative = 1e-9
        );
        // Total height = dead coils at wire pitch + active at solved pitch —
        // pinned against ENGINE fields, not a re-derivation:
        let dead = d.total_coils - d.active_coils; // 2 for SquaredGround
        let expected_h = dead * d.wire_dia.millimeters() + d.active_coils * d.pitch.millimeters();
        assert_relative_eq!(last.1, expected_h, max_relative = 1e-9);
        // Point count: total_coils × 32 + 1.
        assert_eq!(
            line.points.len(),
            (d.total_coils * 32.0).ceil() as usize + 1
        );
    }

    #[test]
    fn degenerate_design_yields_empty_scene() {
        let mut d = design();
        d.mean_dia = springcore::units::Length::from_millimeters(f64::NAN);
        let s = compression_scene(&d);
        assert!(crate::viz::scene_extent(&s).is_none());
    }

    /// Symmetry pin (panel-R2 test matrix): a coil count past the helix
    /// render cap forwards the sampler's empty body unchanged — extent
    /// `None`, placeholder — at THIS family's scene level, not just inside
    /// the shared sampler.
    #[test]
    fn capped_coils_yield_degenerate_scene() {
        let mut d = design();
        d.active_coils = 2001.0;
        d.total_coils = 2003.0;
        let s = compression_scene(&d);
        assert!(crate::viz::scene_extent(&s).is_none());
    }
}
