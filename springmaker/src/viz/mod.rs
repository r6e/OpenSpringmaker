//! 3D spring visualization core: the pure `SceneData` contract (family
//! presenters → renderer), the shared helix sampler every family
//! parameterizes, and the orbit math. The humble renderer/canvas live in
//! `render3d`/`canvas3d` (Tasks 2–3).
//!
//! Accepted limitations (spec §Per-family geometry): the wire renders as a
//! stroked polyline (round cross-section for every family — rectangular wire
//! would need a mesh renderer); hooks are representative arcs, not exact hook
//! developments. Scene coordinates are true millimetres; y is the spring axis.

use std::f64::consts::TAU;

/// Stroke/color role of one polyline (mapped to palette tokens in the
/// renderer only). `Detail` = hooks and legs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // consumed from Task 2 (renderer) / Task 3 (canvas) / Tasks 4-6 (presenters); remove this allow then
pub enum SceneRole {
    Wire,
    Member,
    Detail,
}

/// One 3D polyline in true millimetres; y is the spring axis.
#[allow(dead_code)] // consumed from Task 2 (renderer) / Task 3 (canvas) / Tasks 4-6 (presenters); remove this allow then
pub struct Polyline3 {
    pub points: Vec<(f64, f64, f64)>,
    pub role: SceneRole,
    /// Stroke width in pixels (from `stroke_for`).
    pub stroke_px: u32,
}

/// The pure contract between family scene presenters and the 3D renderer.
pub struct SceneData {
    pub polylines: Vec<Polyline3>,
}

/// Sample a helix: `radius_at`/`height_at` are functions of t ∈ [0, 1] along
/// the wire; the angle sweeps `turns · 2π`. Returns `turns × samples_per_turn
/// + 1` points (inclusive endpoint).
#[allow(dead_code)] // consumed from Task 2 (renderer) / Task 3 (canvas) / Tasks 4-6 (presenters); remove this allow then
pub fn helix(
    radius_at: impl Fn(f64) -> f64,
    height_at: impl Fn(f64) -> f64,
    turns: f64,
    samples_per_turn: usize,
) -> Vec<(f64, f64, f64)> {
    let n = ((turns * samples_per_turn as f64).ceil() as usize).max(2);
    (0..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let angle = t * turns * TAU;
            let r = radius_at(t);
            (r * angle.cos(), height_at(t), r * angle.sin())
        })
        .collect()
}

/// Piecewise axial-height function for a coil body whose dead end coils
/// (total − active, split evenly) are flattened to wire-diameter pitch —
/// driven by the SOLVED coil counts, so every `EndType` renders correctly
/// without matching on it (plain ends have total == active ⇒ no flattening).
/// `t` spans the TOTAL coil count.
#[allow(dead_code)] // consumed from Task 2 (renderer) / Task 3 (canvas) / Tasks 4-6 (presenters); remove this allow then
pub fn coil_height_fn(active: f64, total: f64, pitch_mm: f64, wire_mm: f64) -> impl Fn(f64) -> f64 {
    let dead_per_end = ((total - active) / 2.0).max(0.0);
    move |t: f64| {
        let turn = t * total;
        let dead_lo = turn.min(dead_per_end);
        let active_span = (turn - dead_per_end).clamp(0.0, active);
        let dead_hi = (turn - dead_per_end - active).max(0.0);
        dead_lo * wire_mm + active_span * pitch_mm + dead_hi * wire_mm
    }
}

/// Wire-diameter → stroke width: proportional to the wire's share of the
/// scene's largest dimension, clamped to a legible pixel range.
#[allow(dead_code)] // consumed from Task 2 (renderer) / Task 3 (canvas) / Tasks 4-6 (presenters); remove this allow then
pub fn stroke_for(wire_mm: f64, extent_mm: f64) -> u32 {
    ((wire_mm / extent_mm) * 250.0).clamp(1.0, 8.0) as u32
}

/// 3D bounding extent: max radial distance from the y axis plus the axial
/// span. `None` when no finite point exists (degenerate scene — must not
/// reach the renderer). Coordinates are SIGNED (x/z span ±R); only
/// finiteness is filtered, unlike the 2D chart's non-negative rule.
#[allow(dead_code)] // consumed from Task 2 (renderer) / Task 3 (canvas) / Tasks 4-6 (presenters); remove this allow then
pub struct SceneExtent {
    pub radial: f64,
    pub y_min: f64,
    pub y_max: f64,
}

#[allow(dead_code)] // consumed from Task 2 (renderer) / Task 3 (canvas) / Tasks 4-6 (presenters); remove this allow then
pub fn scene_extent(scene: &SceneData) -> Option<SceneExtent> {
    let mut radial = f64::NEG_INFINITY;
    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;
    for p in scene.polylines.iter().flat_map(|l| l.points.iter()) {
        let (x, y, z) = *p;
        if !(x.is_finite() && y.is_finite() && z.is_finite()) {
            continue;
        }
        radial = radial.max((x * x + z * z).sqrt());
        y_min = y_min.min(y);
        y_max = y_max.max(y);
    }
    (radial.is_finite() && radial > 0.0 && y_min.is_finite() && y_max.is_finite()).then_some(
        SceneExtent {
            radial,
            y_min,
            y_max,
        },
    )
}

/// Committed orbit angles (radians). Global App state — the orientation
/// follows the user across family tabs. Defaults to a three-quarter view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Orbit {
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for Orbit {
    fn default() -> Self {
        Self {
            yaw: 0.9,
            pitch: 0.25,
        }
    }
}

/// Drag sensitivity in radians per pixel.
const ORBIT_SENSITIVITY: f32 = 0.01;
/// Pitch clamp — stops short of the poles so the projection never flips.
const PITCH_LIMIT: f32 = 1.4;

/// Apply a drag delta: yaw wraps into (-π, π], pitch clamps to ±`PITCH_LIMIT`.
#[allow(dead_code)] // consumed from Task 2 (renderer) / Task 3 (canvas) / Tasks 4-6 (presenters); remove this allow then
pub fn orbit_step(current: Orbit, dx: f32, dy: f32) -> Orbit {
    use std::f32::consts::{PI, TAU};
    let mut yaw = current.yaw + dx * ORBIT_SENSITIVITY;
    yaw = yaw.rem_euclid(TAU);
    if yaw > PI {
        yaw -= TAU;
    }
    Orbit {
        yaw,
        pitch: (current.pitch + dy * ORBIT_SENSITIVITY).clamp(-PITCH_LIMIT, PITCH_LIMIT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn helix_endpoint_count_radius_and_height() {
        let pts = helix(|_| 10.0, |t| t * 50.0, 5.0, 32);
        assert_eq!(pts.len(), 5 * 32 + 1); // inclusive endpoint
        let first = pts[0];
        let last = *pts.last().unwrap();
        // Radius holds at both ends: x² + z² = R².
        assert_relative_eq!(
            (first.0.powi(2) + first.2.powi(2)).sqrt(),
            10.0,
            max_relative = 1e-12
        );
        assert_relative_eq!(
            (last.0.powi(2) + last.2.powi(2)).sqrt(),
            10.0,
            max_relative = 1e-12
        );
        // Height integrates 0 → 50.
        assert_relative_eq!(first.1, 0.0, max_relative = 1e-12);
        assert_relative_eq!(last.1, 50.0, max_relative = 1e-12);
        // 5 whole turns: end angle ≡ start angle (x > 0, z ≈ 0).
        assert_relative_eq!(last.2, 0.0, epsilon = 1e-9);
    }

    #[test]
    fn coil_height_flattens_dead_end_coils() {
        // total 10, active 8 → 1 dead coil per end at wire pitch (1 mm); active
        // span at solved pitch 5 mm. Total height = 1 + 8*5 + 1 = 42 mm.
        let h = coil_height_fn(8.0, 10.0, 5.0, 1.0);
        assert_relative_eq!(h(0.0), 0.0, max_relative = 1e-12);
        assert_relative_eq!(h(1.0), 42.0, max_relative = 1e-12);
        // After the first dead coil (t = 1/10): height = 1 mm (wire pitch).
        assert_relative_eq!(h(0.1), 1.0, max_relative = 1e-9);
        // Mid-span (t = 0.5, i.e. dead + 4 active turns): 1 + 4*5 = 21 mm.
        assert_relative_eq!(h(0.5), 21.0, max_relative = 1e-9);
        // Plain ends (total == active): pure linear ramp.
        let plain = coil_height_fn(10.0, 10.0, 5.0, 1.0);
        assert_relative_eq!(plain(0.5), 25.0, max_relative = 1e-9);
    }

    #[test]
    fn stroke_for_clamps_to_legible_range() {
        assert_eq!(stroke_for(2.0, 50.0), 8); // (2/50)*250 = 10 → clamped to 8
        assert_eq!(stroke_for(0.1, 500.0), 1); // 0.05 → clamped to 1
        assert_eq!(stroke_for(1.0, 50.0), 5); // exactly 5, unclamped
    }

    #[test]
    fn scene_extent_spans_all_polylines_and_requires_content() {
        let scene = SceneData {
            polylines: vec![
                Polyline3 {
                    points: vec![(10.0, 0.0, 0.0), (-10.0, 40.0, 3.0)],
                    role: SceneRole::Wire,
                    stroke_px: 2,
                },
                Polyline3 {
                    points: vec![(0.0, -5.0, 12.0)],
                    role: SceneRole::Detail,
                    stroke_px: 1,
                },
            ],
        };
        let e = scene_extent(&scene).unwrap();
        assert_relative_eq!(e.radial, 12.0, max_relative = 1e-12); // max sqrt(x²+z²)
        assert_relative_eq!(e.y_min, -5.0, max_relative = 1e-12);
        assert_relative_eq!(e.y_max, 40.0, max_relative = 1e-12);
        // Empty and non-finite-only scenes are degenerate.
        assert!(scene_extent(&SceneData { polylines: vec![] }).is_none());
        let bad = SceneData {
            polylines: vec![Polyline3 {
                points: vec![(f64::NAN, 0.0, 0.0)],
                role: SceneRole::Wire,
                stroke_px: 1,
            }],
        };
        assert!(scene_extent(&bad).is_none());
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn orbit_step_applies_sensitivity_clamps_pitch_and_wraps_yaw() {
        let o = Orbit {
            yaw: 0.0,
            pitch: 0.0,
        };
        let stepped = orbit_step(o, 100.0, 50.0);
        assert_relative_eq!(stepped.yaw, 1.0, max_relative = 1e-6); // 100 px * 0.01
        assert_relative_eq!(stepped.pitch, 0.5, max_relative = 1e-6); // 50 px * 0.01
                                                                      // Pitch clamps at ±1.4.
        let pinned = orbit_step(
            Orbit {
                yaw: 0.0,
                pitch: 1.35,
            },
            0.0,
            100.0,
        );
        assert_relative_eq!(pinned.pitch, 1.4, max_relative = 1e-6);
        // Yaw wraps into (-π, π].
        let wrapped = orbit_step(
            Orbit {
                yaw: 3.14,
                pitch: 0.0,
            },
            10.0,
            0.0,
        );
        assert!(wrapped.yaw <= std::f32::consts::PI && wrapped.yaw > -std::f32::consts::PI);
        assert_relative_eq!(
            wrapped.yaw,
            3.24 - std::f32::consts::TAU,
            max_relative = 1e-4
        );
    }
}
