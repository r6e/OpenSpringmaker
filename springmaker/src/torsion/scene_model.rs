//! Pure 3D scene presenter for the torsion family: close-wound body (built
//! via the shared `close_wound_coil` helper) plus two straight tangential
//! legs at the solved lengths.

use crate::viz::{close_wound_coil, coil_body_is_empty, Polyline3, SceneData, SceneRole};
use springcore::torsion::TorsionDesign;
use std::f64::consts::TAU;

pub fn torsion_scene(design: &TorsionDesign) -> SceneData {
    let r = design.inputs.mean_dia.millimeters() / 2.0;
    let wire = design.inputs.wire_dia.millimeters();
    let turns = design.inputs.body_coils;
    let mut scene = close_wound_coil(r, turns, wire);
    // Capped/hostile coil count → empty body: the legs below attach at the
    // body's endpoints (`points[0]`/`.last()`), which would panic on an
    // empty Vec. Return the bodyless scene — extent None → placeholder.
    if coil_body_is_empty(&scene) {
        return scene;
    }
    // Decision: size the leg strokes off the body-only stroke rather than
    // recomputing against the full body+legs extent (see task report) — the
    // difference is cosmetic and stroke_for clamps to [1, 8] px anyway.
    let stroke = scene.polylines[0].stroke_px;
    let start = scene.polylines[0].points[0];
    let end = *scene.polylines[0].points.last().unwrap();
    let l1 = design.inputs.leg1.millimeters();
    let l2 = design.inputs.leg2.millimeters();
    // Tangent at angle φ is (−sin φ, cos φ); φ = 0 at the start.
    let end_angle = turns * TAU;
    let leg = |p: (f64, f64, f64), tangent: (f64, f64), len: f64, sign: f64| {
        vec![
            p,
            (
                p.0 + sign * len * tangent.0,
                p.1,
                p.2 + sign * len * tangent.1,
            ),
        ]
    };
    scene.polylines.push(Polyline3 {
        // Tangent at the start angle φ = 0 is (−sin 0, cos 0) = (0, 1).
        points: leg(start, (0.0, 1.0), l1, -1.0),
        role: SceneRole::Detail,
        stroke_px: stroke,
    });
    scene.polylines.push(Polyline3 {
        points: leg(end, (-end_angle.sin(), end_angle.cos()), l2, 1.0),
        role: SceneRole::Detail,
        stroke_px: stroke,
    });
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::torsion::form::{parse_and_solve, TorFormState};
    use approx::assert_relative_eq;
    use springcore::{MaterialSet, MaterialStore, UnitSystem};

    fn store() -> MaterialStore {
        MaterialStore::new(MaterialSet::load_default())
    }

    /// Golden fixture mirrored from `torsion/view_model.rs`'s tests (wire
    /// 2mm, mean 20mm, body 5 coils), with nonzero leg lengths (15mm/10mm)
    /// so the leg-length and tangency assertions are non-trivial.
    fn design() -> TorsionDesign {
        let materials = store();
        let form = TorFormState {
            wire_dia: "2".to_string(),
            mean_dia: "20".to_string(),
            body_coils: "5".to_string(),
            leg1: "15".to_string(),
            leg2: "10".to_string(),
            moments: "500, 1000".to_string(),
            ..Default::default()
        };
        parse_and_solve(&form, "Music Wire", UnitSystem::Metric, &materials)
            .unwrap()
            .design
    }

    #[test]
    fn torsion_scene_has_tangential_legs_of_solved_length() {
        let d = design();
        let s = torsion_scene(&d);
        assert_eq!(s.polylines.len(), 3); // body + 2 legs
        let body = &s.polylines[0];
        let leg1 = &s.polylines[1];
        let leg2 = &s.polylines[2];
        assert_eq!(leg1.role, SceneRole::Detail);
        assert_eq!(leg2.role, SceneRole::Detail);
        // Legs attach at the body endpoints.
        assert_eq!(leg1.points[0], body.points[0]);
        assert_eq!(leg2.points[0], *body.points.last().unwrap());
        // Leg lengths are the ENGINE's inputs (straight segments).
        let len = |l: &Polyline3| {
            let a = l.points[0];
            let b = l.points[1];
            ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2) + (b.2 - a.2).powi(2)).sqrt()
        };
        assert_relative_eq!(len(leg1), d.inputs.leg1.millimeters(), max_relative = 1e-9);
        assert_relative_eq!(len(leg2), d.inputs.leg2.millimeters(), max_relative = 1e-9);
        // Legs are tangential: perpendicular to the radial direction at
        // attach, on BOTH legs — a mutant swapping leg2's tangent to
        // (cos φ, -sin φ) (a 90° error) would still pass every other assert
        // here, so this must check leg2 too, not just leg1.
        let radial_dot_tangent = |l: &Polyline3| {
            let a = l.points[0];
            let b = l.points[1];
            let dir = (b.0 - a.0, b.2 - a.2);
            let radial = (a.0, a.2);
            dir.0 * radial.0 + dir.1 * radial.1
        };
        assert_relative_eq!(radial_dot_tangent(leg1), 0.0, epsilon = 1e-6);
        assert_relative_eq!(radial_dot_tangent(leg2), 0.0, epsilon = 1e-6);
    }

    /// A body-coil count past the helix render cap (`MAX_RENDER_TURNS`) is
    /// VALID form input — "2001" solves fine — but the capped sampler
    /// returns an empty body. The scene must degrade to extent-`None` (the
    /// placeholder), not panic indexing `points[0]` on the empty body
    /// polyline to attach the legs.
    #[test]
    fn capped_body_coils_yield_degenerate_scene() {
        let materials = store();
        let form = TorFormState {
            wire_dia: "2".to_string(),
            mean_dia: "20".to_string(),
            body_coils: "2001".to_string(),
            leg1: "15".to_string(),
            leg2: "10".to_string(),
            moments: "500, 1000".to_string(),
            ..Default::default()
        };
        let d = parse_and_solve(&form, "Music Wire", UnitSystem::Metric, &materials)
            .unwrap()
            .design;
        let s = torsion_scene(&d);
        assert!(crate::viz::scene_extent(&s).is_none());
    }

    /// Post-solve-mutation degenerate fixture (spec §Degenerate handling,
    /// "the chart precedent"): torsion builds legs (family-specific `Detail`
    /// geometry) outside the shared `scene_from_radius` path, so this isn't
    /// covered by compression's degenerate test alone.
    #[test]
    fn degenerate_design_yields_empty_scene() {
        let mut d = design();
        d.inputs.mean_dia = springcore::units::Length::from_millimeters(f64::NAN);
        let s = torsion_scene(&d);
        assert!(crate::viz::scene_extent(&s).is_none());
    }

    /// NaN BODY COILS (unlike the NaN mean_dia above, which only poisons
    /// coordinates) used to reach `coil_height_fn`'s `clamp(0.0, active)`
    /// via stroke sizing before helix's turns guard could fire — a panic,
    /// not a degenerate scene. Must degrade to the placeholder instead.
    #[test]
    fn nan_body_coils_yield_degenerate_scene_not_panic() {
        let mut d = design();
        d.inputs.body_coils = f64::NAN;
        let s = torsion_scene(&d);
        assert!(crate::viz::scene_extent(&s).is_none());
    }
}
