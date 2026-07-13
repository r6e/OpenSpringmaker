//! Pure 3D scene presenter for the extension family: a coil body rendered
//! AT the design's specified free length (wave-2 V5 — body pitch from the
//! shared `viz::sdf::extension_body_pitch_mm`, clamped close-wound from
//! below) plus two representative hook arcs (spec-documented
//! simplification — arcs, not exact hook developments), each attached
//! exactly at its body endpoint.

use crate::viz::sdf::extension_body_pitch_mm;
use crate::viz::{coil_body_is_empty, scene_from_radius, Polyline3, SceneData, SceneRole};
use springcore::extension::ExtensionDesign;
use std::f64::consts::{PI, TAU};

/// Sample a representative hook arc (spec-documented simplification — arcs,
/// not exact hook developments) attached at `(attach_angle, attach_h)` on a
/// coil of radius `coil_r`, with hook radius `hook_r`. `sign` picks the loop
/// direction (+1 loops toward +y, −1 toward −y); `arc(0)` is exactly the body
/// endpoint `(coil_r·cos φ, attach_h, coil_r·sin φ)`.
///
/// **The loop HANGS axially outward (wave-2 V4 fix).** The loop circle lies
/// in the vertical plane at the attach azimuth, centered at
/// `(coil_r, attach_h + sign·hook_r)` — one hook radius BEYOND the attach
/// along the axis — so the whole 1.5π sweep stays on the outward side of
/// the attach: radial `coil_r + hook_r·sin θ`, axial
/// `attach_h + sign·hook_r·(1 − cos θ)`. Its centerline extreme sits
/// `2·hook_r` beyond the attach, i.e. the loop's INNER surface reaches
/// `2·hook_r − d` past the body face — exactly the per-end hook allowance
/// in Shigley's inside-hooks free length (Fig. 10-7b / Eq. 10-39,
/// `L0 = 2(D − d) + (Nb + 1)d` for standard `r = D/2` loops). The pre-fix
/// loop was vertically CENTERED on the attach point instead, so its final
/// quarter-sweep climbed `hook_r` INTO the coil bore (user finding V4,
/// "hooks curl into the body") and its axial reach matched no citable
/// free-length convention. The transition at the attach keeps the
/// documented sharp-corner limitation (the loop plane is vertical, the
/// coil tangent azimuthal — identical in both geometry paths).
///
/// **Curl handedness (user finding V6 fix).** The `+ hook_r·sin θ` sign is
/// the loop's in-plane curl direction, per Shigley Fig. 10-6(a)/10-7(b):
/// from the attach the wire departs radially OUTWARD (the max-bending
/// point A sits on the outermost side at mid-loop height), sweeps up over
/// the top, and the free tip ends on the INNER side curling back toward
/// the body — the open quarter of the sweep faces the body on the inner
/// side ("Gap" in Fig. 10-7b). The V4 fix shipped the mirror image
/// (`− hook_r·sin θ`: inward departure, outward tip) — positionally
/// correct but curling the wrong way, invisible to the cross-path
/// agreement test because BOTH paths carried the same mirror. Kept in
/// lockstep with `viz::sdf::hook_torus_part` (its `y_rotation` bakes the
/// same handedness); the handedness pins in both test modules are the
/// asymmetric-sample guards.
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
            let radial = coil_r + hook_r * theta.sin();
            (
                radial * attach_angle.cos(),
                attach_h + sign * hook_r * (1.0 - theta.cos()),
                radial * attach_angle.sin(),
            )
        })
        .collect()
}

pub fn extension_scene(design: &ExtensionDesign) -> SceneData {
    let r = design.mean_dia.millimeters() / 2.0;
    let wire = design.wire_dia.millimeters();
    let turns = design.active_coils;
    // Body pitch renders the SPECIFIED free length (wave-2 V5) — shared
    // with `extension_sdf` so the two geometry paths cannot drift; clamped
    // close-wound from below (defense in depth, decision point e).
    let pitch = extension_body_pitch_mm(design);
    let mut scene = scene_from_radius(|_| r, r, turns, turns, pitch, wire);
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
    let body_h = turns * pitch;
    let end_angle = turns * TAU;
    // BOTH loops use r1 — the loop's mean bend radius, the one the engine's
    // free-length relation (`free_length_from_geometry`) models; r2 is the
    // hook's side-bend torsion radius, not a loop radius (wave-2 V5).
    let hook_r = design.hooks.r1.millimeters();
    scene.polylines.push(Polyline3 {
        points: hook_arc(0.0, 0.0, r, hook_r, -1.0),
        role: SceneRole::Detail,
        stroke_px: stroke,
    });
    scene.polylines.push(Polyline3 {
        points: hook_arc(end_angle, body_h, r, hook_r, 1.0),
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

    /// (min y, max y) over a polyline's points — the hook-span pins fold the
    /// y-extremes four ways otherwise (simplifier nit).
    fn y_extremes(line: &Polyline3) -> (f64, f64) {
        line.points
            .iter()
            .map(|p| p.1)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), y| {
                (lo.min(y), hi.max(y))
            })
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
    fn extension_scene_spans_the_free_length_with_continuous_hooks() {
        let d = design(); // the plot_model fixture: wire 2, mean 20, active 10, Fi 5
        let s = extension_scene(&d);
        // Body + two hooks.
        assert_eq!(s.polylines.len(), 3);
        let body = &s.polylines[0];
        assert_eq!(body.role, SceneRole::Wire);
        // Wave-2 V5: the body renders AT the specified free length, not
        // close-wound. Body centerline height = L0 − 2·(2·r1 − d) − d
        // (the engine's inside-hooks relation, `free_length_from_geometry`,
        // solved for the stretched body): 100 − 2·18 − 2 = 62 mm.
        let last = *body.points.last().unwrap();
        let wire = d.wire_dia.millimeters();
        let r1 = d.hooks.r1.millimeters();
        let expected_body_h = d.free_length.millimeters() - 2.0 * (2.0 * r1 - wire) - wire;
        assert_relative_eq!(last.1, expected_body_h, max_relative = 1e-9);
        assert_relative_eq!(last.1, 62.0, max_relative = 1e-9);
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

    /// Regression (wave-2 V4, user-reported "hooks curl INTO the body"):
    /// the representative hook loop must HANG axially outward from its
    /// attach point — bottom loop entirely at `y <= 0`, top loop entirely
    /// at `y >= body_top` — with the loop's centerline extreme exactly
    /// `2·hook_r` beyond the attach (inner surface `2·hook_r − d` beyond
    /// the body face: Shigley Fig. 10-7b / Eq. 10-39's per-end hook
    /// allowance). The pre-fix loop was vertically CENTERED on the attach
    /// point, so its final quarter-sweep climbed `hook_r` INTO the coil
    /// bore.
    #[test]
    fn hook_arcs_hang_axially_outward_never_into_the_body_span() {
        let d = design();
        let s = extension_scene(&d);
        let body_top = s.polylines[0].points.last().unwrap().1;
        // BOTH loops use r1 — the loop's mean bend radius. r2 is the hook's
        // side-bend radius (torsion at point B), not a loop radius; the
        // engine's free-length relation (`free_length_from_geometry`)
        // models both end loops by `d_loop = 2·r1` (wave-2 V5).
        let r1 = d.hooks.r1.millimeters();
        let bottom = &s.polylines[1];
        let top = &s.polylines[2];
        for p in &bottom.points {
            assert!(p.1 <= 1e-9, "bottom hook enters the body span: y={}", p.1);
        }
        for p in &top.points {
            assert!(
                p.1 >= body_top - 1e-9,
                "top hook dips into the body span: y={} (body_top={body_top})",
                p.1
            );
        }
        let (bottom_min, _) = y_extremes(bottom);
        let (_, top_max) = y_extremes(top);
        assert_relative_eq!(bottom_min, -2.0 * r1, max_relative = 1e-9);
        assert_relative_eq!(top_max, body_top + 2.0 * r1, max_relative = 1e-9);
    }

    /// Handedness pin (user finding V6: hooks curled 180° mirrored; the
    /// centerline-agreement test only enforces cross-path CONSISTENCY, so a
    /// consistent mirror survived it). Shigley Fig. 10-6(a)/10-7(b): from
    /// the attach the loop departs radially OUTWARD, sweeps up over the
    /// top, and its free tip ends on the INNER side curling back toward
    /// the body (gap between tip and body). The mirrored curl departs
    /// inward and tips outward — asymmetric samples fail under it.
    #[test]
    fn hook_arcs_depart_radially_outward_and_tip_radially_inward() {
        let d = design();
        let s = extension_scene(&d);
        let coil_r = d.mean_dia.millimeters() / 2.0;
        for hook in [&s.polylines[1], &s.polylines[2]] {
            let radial = |p: &(f64, f64, f64)| p.0.hypot(p.2);
            // Just past the attach (sample 1 of 24 — θ ≈ 0.2 rad): outward.
            let early = radial(&hook.points[1]);
            assert!(
                early > coil_r + 1e-9,
                "hook departs the attach radially inward (mirrored curl): {early} <= {coil_r}"
            );
            // The free tip (θ = 1.5π): on the inner side, toward the axis.
            let tip = radial(hook.points.last().unwrap());
            assert!(
                tip < coil_r - 1e-9,
                "hook tip points radially outward (mirrored curl): {tip} >= {coil_r}"
            );
        }
    }

    /// Wave-2 V5 (user directive: render at the specified length): the
    /// rendered INSIDE-HOOKS span — from the bottom loop's inner surface to
    /// the top loop's inner surface — must equal the design's free length,
    /// matching the engine's own inside-hooks definition
    /// (`free_length_from_geometry`, Shigley Eq. 10-39 / Fig. 10-7b).
    #[test]
    fn rendered_inside_hook_span_equals_the_specified_free_length() {
        let d = design();
        let s = extension_scene(&d);
        let wire_r = d.wire_dia.millimeters() / 2.0;
        let (bottom_min, _) = y_extremes(&s.polylines[1]);
        let (_, top_max) = y_extremes(&s.polylines[2]);
        let inside_span = (top_max - wire_r) - (bottom_min + wire_r);
        assert_relative_eq!(
            inside_span,
            d.free_length.millimeters(),
            max_relative = 1e-9
        );
    }

    /// Defense in depth (wave-2 V5 decision point e): the engine rejects a
    /// free length below the close-wound minimum, so this is unreachable
    /// through a real solve — but the BUILDER must never inter-penetrate
    /// coils regardless of upstream validation. A post-solve-mutated free
    /// length below the minimum clamps the body to close-wound (pitch =
    /// wire), never tighter.
    #[test]
    fn free_length_below_minimum_clamps_the_body_to_close_wound() {
        let mut d = design();
        d.free_length = springcore::units::Length::from_millimeters(10.0); // far below minimum
        let s = extension_scene(&d);
        let last = *s.polylines[0].points.last().unwrap();
        assert_relative_eq!(
            last.1,
            d.active_coils * d.wire_dia.millimeters(), // close-wound floor
            max_relative = 1e-9
        );
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

    /// NaN ACTIVE COILS (unlike the NaN mean_dia above, which only poisons
    /// coordinates) used to reach `coil_height_fn`'s `clamp(0.0, active)`
    /// via stroke sizing before helix's turns guard could fire — a panic,
    /// not a degenerate scene. Must degrade to the placeholder instead.
    #[test]
    fn nan_active_coils_yield_degenerate_scene_not_panic() {
        let mut d = design();
        d.active_coils = f64::NAN;
        let s = extension_scene(&d);
        assert!(crate::viz::scene_extent(&s).is_none());
    }

    /// Sibling parity with `extension_sdf` (R2 input-domain F3): ZERO active
    /// coils drives the derived pitch to `stretch / 0 = inf`, poisoning the
    /// body heights (`0 × inf = NaN`) while the bottom hook stays fully
    /// finite — pre-fix this rendered a disembodied floating hook arc, the
    /// exact artifact the empty-body bail documents preventing, where the
    /// SDF path correctly degrades to the default scene.
    #[test]
    fn zero_active_coils_yield_degenerate_scene_not_floating_hooks() {
        let mut d = design();
        d.active_coils = 0.0;
        let s = extension_scene(&d);
        assert!(crate::viz::scene_extent(&s).is_none());
    }

    /// Sibling parity with `extension_sdf`'s `geometry_hostile(&[pitch])`
    /// gate (R2 input-domain F3): an INFINITE free length (post-solve
    /// mutation) makes the derived pitch infinite; the body's points go
    /// NaN/inf (filtered downstream) but the bottom hook stays finite —
    /// pre-fix: a floating hook arc instead of the placeholder.
    #[test]
    fn infinite_free_length_yields_degenerate_scene_not_floating_hooks() {
        let mut d = design();
        d.free_length = springcore::units::Length::from_millimeters(f64::INFINITY);
        let s = extension_scene(&d);
        assert!(crate::viz::scene_extent(&s).is_none());
    }
}
