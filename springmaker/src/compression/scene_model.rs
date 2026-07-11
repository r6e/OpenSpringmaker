//! Pure 3D scene presenter for the compression family (ADR 0008): a constant-
//! radius helix over the solved coil counts, dead end coils flattened to
//! wire pitch (data-driven from total−active — correct for every EndType).

use crate::viz::{coil_height_fn, helix, stroke_for, Polyline3, SceneData, SceneRole};
use springcore::SpringDesign;

pub fn compression_scene(design: &SpringDesign) -> SceneData {
    let r = design.mean_dia.millimeters() / 2.0;
    let wire = design.wire_dia.millimeters();
    let total = design.total_coils;
    let height = coil_height_fn(design.active_coils, total, design.pitch.millimeters(), wire);
    let max_h = height(1.0);
    let extent = (2.0 * r).max(max_h);
    let points = helix(|_| r, height, total, 32);
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
}
