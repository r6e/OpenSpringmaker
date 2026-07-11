//! Pure 3D scene presenter for the extension family: close-wound body
//! (built via the shared `close_wound_coil` helper) plus two representative
//! hook arcs (spec-documented simplification — arcs, not exact hook
//! developments), each attached exactly at its body endpoint.

use crate::viz::{close_wound_coil, coil_body_is_empty, Polyline3, SceneData, SceneRole};
use springcore::extension::ExtensionDesign;
use std::f64::consts::{PI, TAU};

/// Sample a representative hook arc (spec-documented simplification — arcs,
/// not exact hook developments) attached at `(attach_angle, attach_h)` on a
/// coil of radius `coil_r`, with hook radius `hook_r`. `sign` picks the loop
/// direction (+1 loops toward +y, −1 toward −y); `arc(0)` is exactly the body
/// endpoint `(coil_r·cos φ, attach_h, coil_r·sin φ)`.
fn hook_arc(
    attach_angle: f64,
    attach_h: f64,
    coil_r: f64,
    hook_r: f64,
    sign: f64,
) -> Vec<(f64, f64, f64)> {
    const SAMPLES: usize = 24;
    (0..=SAMPLES)
        .map(|i| {
            let theta = i as f64 / SAMPLES as f64 * (1.5 * PI);
            let radial = coil_r - hook_r + hook_r * theta.cos();
            (
                radial * attach_angle.cos(),
                attach_h + sign * hook_r * theta.sin(),
                radial * attach_angle.sin(),
            )
        })
        .collect()
}

pub fn extension_scene(design: &ExtensionDesign) -> SceneData {
    let r = design.mean_dia.millimeters() / 2.0;
    let wire = design.wire_dia.millimeters();
    let turns = design.active_coils;
    let mut scene = close_wound_coil(r, turns, wire);
    // Capped/hostile coil count → empty body: the hooks are positioned from
    // radius and coil count alone, so building them anyway would render two
    // floating arcs around a missing body (finite extent — the placeholder
    // would NOT fire). Return the bodyless scene — extent None → placeholder.
    if coil_body_is_empty(&scene) {
        return scene;
    }
    // Decision: size the hook strokes off the body-only stroke rather than
    // recomputing against the full body+hooks extent (see task report) —
    // the difference is cosmetic and stroke_for clamps to [1, 8] px anyway.
    let stroke = scene.polylines[0].stroke_px;
    let body_h = turns * wire;
    let end_angle = turns * TAU;
    scene.polylines.push(Polyline3 {
        points: hook_arc(0.0, 0.0, r, design.hooks.r1.millimeters(), -1.0),
        role: SceneRole::Detail,
        stroke_px: stroke,
    });
    scene.polylines.push(Polyline3 {
        points: hook_arc(end_angle, body_h, r, design.hooks.r2.millimeters(), 1.0),
        role: SceneRole::Detail,
        stroke_px: stroke,
    });
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension::form::{parse_and_solve, ExtFormState};
    use approx::assert_relative_eq;
    use springcore::{CurvatureCorrection, MaterialSet, MaterialStore, UnitSystem};

    fn store() -> MaterialStore {
        MaterialStore::new(MaterialSet::load_default())
    }

    /// Golden fixture mirrored from `extension/plot_model.rs`'s `solved`
    /// fixture: wire 2mm, mean 20mm, active 10 coils, free 100mm, Fi 5N,
    /// loads 10/30N — default hook mode (r1 = D/2 = 10mm, r2 = D/4 = 5mm).
    fn design() -> ExtensionDesign {
        let materials = store();
        let form = ExtFormState {
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

    #[test]
    fn extension_scene_is_close_wound_with_continuous_hooks() {
        let d = design(); // the plot_model fixture: wire 2, mean 20, active 10, Fi 5
        let s = extension_scene(&d);
        // Body + two hooks.
        assert_eq!(s.polylines.len(), 3);
        let body = &s.polylines[0];
        assert_eq!(body.role, SceneRole::Wire);
        // Close-wound: body height = active × wire dia (engine-field pin).
        let last = *body.points.last().unwrap();
        assert_relative_eq!(
            last.1,
            d.active_coils * d.wire_dia.millimeters(),
            max_relative = 1e-9
        );
        // Hook continuity: each hook's first point == its body endpoint (1e-9).
        let bottom = &s.polylines[1];
        let top = &s.polylines[2];
        assert_eq!(bottom.role, SceneRole::Detail);
        let b0 = bottom.points[0];
        let body0 = body.points[0];
        assert_relative_eq!(b0.0, body0.0, max_relative = 1e-9);
        assert_relative_eq!(b0.1, body0.1, epsilon = 1e-9);
        assert_relative_eq!(b0.2, body0.2, epsilon = 1e-9);
        let t0 = top.points[0];
        assert_relative_eq!(t0.0, last.0, max_relative = 1e-9);
        assert_relative_eq!(t0.1, last.1, max_relative = 1e-9);
        assert_relative_eq!(t0.2, last.2, epsilon = 1e-9);
    }

    /// An active-coil count past the helix render cap (`MAX_RENDER_TURNS`)
    /// is VALID form input — active "2001" with free length "5000" solves —
    /// but the capped sampler returns an empty body. Both hooks are
    /// positioned from radius and coil count alone (independent of the body
    /// points), so without an empty-body bail the scene keeps finite hook
    /// points and renders two disembodied arcs; it must instead degrade to
    /// extent-`None` (the placeholder).
    #[test]
    fn capped_active_coils_yield_degenerate_scene_not_floating_hooks() {
        let materials = store();
        let form = ExtFormState {
            wire_dia: "2".to_string(),
            mean_dia: "20".to_string(),
            active: "2001".to_string(),
            free_length: "5000".to_string(),
            initial_tension: "5".to_string(),
            loads: "10, 30".to_string(),
            ..Default::default()
        };
        let d = parse_and_solve(
            &form,
            "Music Wire",
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .unwrap()
        .design;
        let s = extension_scene(&d);
        assert!(
            crate::viz::scene_extent(&s).is_none(),
            "an empty capped body must not leave floating hook arcs behind"
        );
    }

    /// Post-solve-mutation degenerate fixture (spec §Degenerate handling,
    /// "the chart precedent"): a NaN solved field must yield a scene with no
    /// finite extent, not a partially-broken scene reaching the renderer.
    /// Extension builds hooks (family-specific `Detail` geometry) outside
    /// the shared `scene_from_radius` path, so this isn't covered by
    /// compression's degenerate test alone.
    #[test]
    fn degenerate_design_yields_empty_scene() {
        let mut d = design();
        d.mean_dia = springcore::units::Length::from_millimeters(f64::NAN);
        let s = extension_scene(&d);
        assert!(crate::viz::scene_extent(&s).is_none());
    }
}
