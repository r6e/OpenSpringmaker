//! 3D spring visualization core: the pure `SceneData` contract (family
//! presenters → renderer), the shared helix sampler every family
//! parameterizes, and the orbit math. The humble renderer/canvas live in
//! `render3d`/`canvas3d`.
//!
//! Accepted limitations (spec §Per-family geometry): the wire renders as a
//! stroked polyline (round cross-section for every family — rectangular wire
//! would need a mesh renderer); hooks are representative arcs, not exact hook
//! developments; no self-intersection/clearance rendering beyond what true
//! geometry shows. Scene coordinates are true millimetres; y is the spring axis.

use std::f64::consts::TAU;

pub mod canvas3d;
pub mod render3d;
pub mod sdf;
pub mod shader3d;

pub use canvas3d::scene_element;
#[cfg(test)]
pub(crate) use canvas3d::{SCENE_PLACEHOLDER, SCENE_PLACEHOLDER_CAPPED};

/// Stroke/color role of one polyline (mapped to palette tokens in the
/// renderer only). `Detail` = hooks and legs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneRole {
    Wire,
    /// Alternates with `Wire` by member index; constructed by
    /// `assembly::scene_model`.
    Member,
    /// Hooks (extension) and legs (torsion); constructed by
    /// `extension::scene_model`/`torsion::scene_model`.
    Detail,
}

/// One 3D polyline in true millimetres; y is the spring axis.
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

/// Hard cap on renderable turns. Coil counts are form-controlled with only
/// positive+finite validation upstream, so the viz layer must self-defend:
/// beyond this, additional turns add no visible detail at the shipped
/// 760×300 scene resolution, while an uncapped hostile value (a huge finite
/// turns count, or `inf`) would try to allocate an enormous or overflowing
/// point buffer inside `view()` — an application-killing panic on every
/// affected family, since they all share this sampler.
const MAX_RENDER_TURNS: f64 = 2_000.0;

/// Sample a helix: `radius_at`/`height_at` are functions of t ∈ [0, 1] along
/// the wire; the angle sweeps `turns · 2π`. Returns
/// `⌈turns × samples_per_turn⌉ + 1` points (inclusive endpoint), with a floor
/// of 3 points so tiny turn counts still draw a visible arc.
///
/// Hostile `turns` (non-finite, negative, or beyond [`MAX_RENDER_TURNS`])
/// returns an empty `Vec` instead of sampling, so the existing
/// degenerate-scene discipline (`scene_extent` returning `None`, the caller
/// showing the placeholder) fires rather than the allocation overflowing or
/// blowing up the frame budget. Mirrors `scene_from_radius`'s entry guard so
/// a direct caller gets the same contract.
pub fn helix(
    radius_at: impl Fn(f64) -> f64,
    height_at: impl Fn(f64) -> f64,
    turns: f64,
    samples_per_turn: usize,
) -> Vec<(f64, f64, f64)> {
    if !(0.0..=MAX_RENDER_TURNS).contains(&turns) {
        return Vec::new();
    }
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
/// scene's largest dimension, clamped to a legible pixel range. A non-finite
/// ratio (zero or non-finite `extent_mm`) floors to 1 rather than propagating
/// NaN/inf through `as u32` (which would silently truncate to 0).
pub fn stroke_for(wire_mm: f64, extent_mm: f64) -> u32 {
    let ratio = wire_mm / extent_mm;
    if !ratio.is_finite() {
        return 1;
    }
    (ratio * 250.0).clamp(1.0, 8.0) as u32
}

/// Helix sampling density shared by every family scene.
const SAMPLES_PER_TURN: usize = 32;

/// Build the standard one-wire coil scene every helical family shares:
/// `coil_height_fn` over the solved coil counts (dead end coils flattened to
/// wire pitch), a [`SAMPLES_PER_TURN`]-per-turn helix with the given radius
/// profile, and a stroke sized against the scene extent. `max_radius_mm` is
/// the largest value `radius_at` attains (the closure hides it, so callers
/// pass it explicitly); extent = max(2·max_radius, total height).
///
/// Hostile coil counts (non-finite or negative `active`/`total`, or `total`
/// past [`MAX_RENDER_TURNS`]) return the degenerate empty-body scene
/// (`scene_extent` → `None`, the caller shows the placeholder). The guard
/// lives HERE and not only in [`helix`] because the unconditional
/// stroke-sizing `height(1.0)` call below runs BEFORE helix's turns guard —
/// and `coil_height_fn`'s `clamp(0.0, active)` panics when `active` is NaN
/// or negative (std clamp: "min > max, or either was NaN"). Helix keeps its
/// own guard as defense in depth.
pub fn scene_from_radius(
    radius_at: impl Fn(f64) -> f64,
    max_radius_mm: f64,
    active: f64,
    total: f64,
    pitch_mm: f64,
    wire_mm: f64,
) -> SceneData {
    // The range check rejects a NaN/±inf total too (`contains` is false for
    // all three); active needs its own finiteness test since `+inf < 0.0`
    // and `NaN < 0.0` are both false.
    let coils_hostile =
        !active.is_finite() || active < 0.0 || !(0.0..=MAX_RENDER_TURNS).contains(&total);
    if coils_hostile {
        // Same shape as the normal path (exactly one Wire polyline, here
        // with no points) so the documented one-polyline invariant holds
        // for every caller and `coil_body_is_empty` reads it uniformly.
        return SceneData {
            polylines: vec![Polyline3 {
                points: Vec::new(),
                role: SceneRole::Wire,
                stroke_px: 1,
            }],
        };
    }
    let height = coil_height_fn(active, total, pitch_mm, wire_mm);
    let extent = (2.0 * max_radius_mm).max(height(1.0));
    let points = helix(radius_at, height, total, SAMPLES_PER_TURN);
    SceneData {
        polylines: vec![Polyline3 {
            points,
            role: SceneRole::Wire,
            stroke_px: stroke_for(wire_mm, extent),
        }],
    }
}

/// Close-wound coil body shared by extension and torsion: pitch = wire
/// diameter collapses `coil_height_fn` to a linear close-wound ramp (no dead
/// coils, since active == total for a close-wound body). A thin wrapper over
/// `scene_from_radius` with a constant radius and active == total == `turns`,
/// hosting the explanation once instead of at each call site.
pub fn close_wound_coil(radius_mm: f64, turns: f64, wire_mm: f64) -> SceneData {
    scene_from_radius(|_| radius_mm, radius_mm, turns, turns, wire_mm, wire_mm)
}

/// Whether a scene's coil BODY — `polylines[0]`, the helix polyline that
/// `scene_from_radius` (and thus `close_wound_coil`) always puts first —
/// came back EMPTY: the capped/hostile-turns outcome of [`helix`]'s guard.
/// Every family presenter that builds ON the body (attaching `Detail`
/// hooks/legs, or composing members) MUST check this before touching the
/// body's points; otherwise it indexes into an empty Vec (panic) or renders
/// floating details around a missing body. Centralized so a future
/// Detail-building family cannot reintroduce the gap.
pub(crate) fn coil_body_is_empty(scene: &SceneData) -> bool {
    scene
        .polylines
        .first()
        .is_none_or(|body| body.points.is_empty())
}

/// 3D bounding extent: max radial distance from the y axis plus the axial
/// span. `None` when no finite point exists (degenerate scene — must not
/// reach the renderer). Coordinates are SIGNED (x/z span ±R); only
/// finiteness is filtered, unlike the 2D chart's non-negative rule.
pub struct SceneExtent {
    pub radial: f64,
    pub y_min: f64,
    pub y_max: f64,
}

/// Whether all three coordinates of a 3D point are finite — shared by
/// `scene_extent` and the renderer's point filter (`render3d`) so the two
/// cannot drift apart (mirrors why `plot::plottable` exists).
pub(crate) fn finite3(p: (f64, f64, f64)) -> bool {
    p.0.is_finite() && p.1.is_finite() && p.2.is_finite()
}

pub fn scene_extent(scene: &SceneData) -> Option<SceneExtent> {
    let mut radial = f64::NEG_INFINITY;
    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;
    for p in scene.polylines.iter().flat_map(|l| l.points.iter()) {
        if !finite3(*p) {
            continue;
        }
        let (x, y, z) = *p;
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
/// A non-finite delta (NaN/inf, e.g. a degenerate cursor-position subtraction)
/// leaves `current` unchanged rather than poisoning the committed orbit with
/// NaN, which would propagate forever (NaN + x = NaN on every future drag).
pub fn orbit_step(current: Orbit, dx: f32, dy: f32) -> Orbit {
    if !dx.is_finite() || !dy.is_finite() {
        return current;
    }
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

/// Multiplicative zoom bounds — the shaded-3D camera's `App.zoom` clamp
/// (spec §Interaction: "`App.zoom: f32` (default 1.0, clamped to
/// `[0.3, 4.0]`)"); shared by [`zoom_step`] (the app-level accumulator)
/// and [`camera_uniforms`]'s own defensive clamp.
pub(crate) const ZOOM_MIN: f32 = 0.3;
pub(crate) const ZOOM_MAX: f32 = 4.0;

/// Zoom sensitivity: e-folding rate per unit of `scroll_delta`. Chosen so a
/// typical single wheel/trackpad tick (`scroll_delta` ~1-3) changes zoom by
/// a comfortable ~10-35%.
const ZOOM_SENSITIVITY: f32 = 0.1;

/// Apply a scroll-wheel delta multiplicatively: `current * e^(delta ×
/// SENSITIVITY)` — exponential rather than `current * (1 + delta ×
/// SENSITIVITY)` so the pre-clamp result is ALWAYS strictly positive
/// regardless of `delta`'s sign or magnitude (no "clamped but still
/// negative" edge case to separately guard), then clamped to [`ZOOM_MIN`],
/// [`ZOOM_MAX`]. A non-finite delta (NaN/inf, e.g. a degenerate scroll
/// event) leaves `current` unchanged, mirroring [`orbit_step`]'s guard.
pub(crate) fn zoom_step(current: f32, scroll_delta: f32) -> f32 {
    if !scroll_delta.is_finite() {
        return current;
    }
    (current * (scroll_delta * ZOOM_SENSITIVITY).exp()).clamp(ZOOM_MIN, ZOOM_MAX)
}

/// A 4x4 matrix stored COLUMN-MAJOR (`m[col*4 + row]`) — the layout WGSL's
/// `mat4x4<f32>` expects when built from a flat float array (Task 5 mirrors
/// this exactly): column `c` occupies `m[c*4 .. c*4+4]`.
type Mat4 = [f64; 16];

const fn mat4_identity() -> Mat4 {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

/// `a * b` (matrix product; applying the RESULT to a vector is equivalent
/// to applying `b` first, then `a`).
fn mat4_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [0.0; 16];
    for col in 0..4 {
        for row in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += a[k * 4 + row] * b[col * 4 + k];
            }
            out[col * 4 + row] = sum;
        }
    }
    out
}

/// `m * v` (homogeneous column vector) — test-only (the frustum-containment
/// pin projects world points through `view_proj` directly; production code
/// never needs a raw matrix-vector product).
#[cfg(test)]
fn mat4_mul_vec4(m: &Mat4, v: [f64; 4]) -> [f64; 4] {
    let mut out = [0.0; 4];
    for row in 0..4 {
        let mut sum = 0.0;
        for col in 0..4 {
            sum += m[col * 4 + row] * v[col];
        }
        out[row] = sum;
    }
    out
}

/// Right-handed look-at view matrix (world → view space; camera looks down
/// its own -Z, +X right, +Y up) — the standard construction. `up` need not
/// be exactly orthogonal to `target - eye` (re-orthogonalized via the cross
/// products below) but must not be parallel to it; never happens here
/// since [`PITCH_LIMIT`] (1.4 rad, 80.2°) keeps the view direction's angle
/// to world Y strictly short of 90°.
fn mat4_look_at(eye: [f64; 3], target: [f64; 3], up: [f64; 3]) -> Mat4 {
    let sub = |p: [f64; 3], q: [f64; 3]| [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
    let dot = |p: [f64; 3], q: [f64; 3]| p[0] * q[0] + p[1] * q[1] + p[2] * q[2];
    let cross = |p: [f64; 3], q: [f64; 3]| {
        [
            p[1] * q[2] - p[2] * q[1],
            p[2] * q[0] - p[0] * q[2],
            p[0] * q[1] - p[1] * q[0],
        ]
    };
    let normalize = |v: [f64; 3]| {
        let len = dot(v, v).sqrt();
        [v[0] / len, v[1] / len, v[2] / len]
    };
    let forward = normalize(sub(target, eye));
    let right = normalize(cross(forward, up));
    let true_up = cross(right, forward);
    let mut m = mat4_identity();
    m[0] = right[0];
    m[1] = true_up[0];
    m[2] = -forward[0];
    m[4] = right[1];
    m[5] = true_up[1];
    m[6] = -forward[1];
    m[8] = right[2];
    m[9] = true_up[2];
    m[10] = -forward[2];
    m[12] = -dot(right, eye);
    m[13] = -dot(true_up, eye);
    m[14] = dot(forward, eye);
    m[15] = 1.0;
    m
}

/// Right-handed perspective projection, wgpu/D3D NDC-depth convention
/// (`z_ndc ∈ [0, 1]`, near → 0, far → 1 — NOT OpenGL's `[-1, 1]`, since
/// Task 5's actual pipeline runs through `wgpu`).
fn mat4_perspective(fov_y_rad: f64, aspect: f64, near: f64, far: f64) -> Mat4 {
    let f_cot = 1.0 / (fov_y_rad / 2.0).tan();
    let mut m = [0.0; 16];
    m[0] = f_cot / aspect;
    m[5] = f_cot;
    m[10] = far / (near - far);
    m[11] = -1.0;
    m[14] = (near * far) / (near - far);
    m
}

/// General 4x4 matrix inverse via the cofactor/adjugate expansion (the
/// classic, widely-published closed-form construction — not specific to
/// any one library). A near-zero determinant (a degenerate camera
/// configuration: `near == far`, or a zero-volume frustum) is unreachable
/// via [`camera_uniforms`]'s own inputs (FOV/near/far are all derived
/// strictly positive with `near < far` by construction) but falls back to
/// the identity rather than dividing by ~zero, as defense in depth.
fn mat4_invert(m: &Mat4) -> Mat4 {
    let mut inv = [0.0; 16];
    inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15]
        + m[9] * m[7] * m[14]
        + m[13] * m[6] * m[11]
        - m[13] * m[7] * m[10];
    inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15]
        - m[8] * m[7] * m[14]
        - m[12] * m[6] * m[11]
        + m[12] * m[7] * m[10];
    inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15]
        + m[8] * m[7] * m[13]
        + m[12] * m[5] * m[11]
        - m[12] * m[7] * m[9];
    inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14]
        - m[8] * m[6] * m[13]
        - m[12] * m[5] * m[10]
        + m[12] * m[6] * m[9];
    inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15]
        - m[9] * m[3] * m[14]
        - m[13] * m[2] * m[11]
        + m[13] * m[3] * m[10];
    inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15]
        + m[8] * m[3] * m[14]
        + m[12] * m[2] * m[11]
        - m[12] * m[3] * m[10];
    inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15]
        - m[8] * m[3] * m[13]
        - m[12] * m[1] * m[11]
        + m[12] * m[3] * m[9];
    inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14]
        + m[8] * m[2] * m[13]
        + m[12] * m[1] * m[10]
        - m[12] * m[2] * m[9];
    inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15]
        + m[5] * m[3] * m[14]
        + m[13] * m[2] * m[7]
        - m[13] * m[3] * m[6];
    inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15]
        - m[4] * m[3] * m[14]
        - m[12] * m[2] * m[7]
        + m[12] * m[3] * m[6];
    inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15]
        + m[4] * m[3] * m[13]
        + m[12] * m[1] * m[7]
        - m[12] * m[3] * m[5];
    inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14]
        - m[4] * m[2] * m[13]
        - m[12] * m[1] * m[6]
        + m[12] * m[2] * m[5];
    inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11]
        - m[5] * m[3] * m[10]
        - m[9] * m[2] * m[7]
        + m[9] * m[3] * m[6];
    inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11]
        + m[4] * m[3] * m[10]
        + m[8] * m[2] * m[7]
        - m[8] * m[3] * m[6];
    inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11]
        - m[4] * m[3] * m[9]
        - m[8] * m[1] * m[7]
        + m[8] * m[3] * m[5];
    inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10]
        + m[4] * m[2] * m[9]
        + m[8] * m[1] * m[6]
        - m[8] * m[2] * m[5];

    let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
    if det.abs() < 1e-12 {
        return mat4_identity();
    }
    let inv_det = 1.0 / det;
    inv.map(|x| x * inv_det)
}

fn mat4_to_f32(m: &Mat4) -> [f32; 16] {
    std::array::from_fn(|i| m[i] as f32)
}

/// Vertical field of view, fixed (no user FOV control — spec §Architecture:
/// "45° vertical FOV").
const FOV_Y_RAD: f64 = 45.0 * std::f64::consts::PI / 180.0;

/// Fractions of `extent_mm` used for the near/far clip planes — generous
/// enough that the fit-to-extent bounding sphere stays inside `[near, far]`
/// across the FULL [`ZOOM_MIN`], [`ZOOM_MAX`] range (worst-case eye
/// distance runs roughly `extent_mm × [0.33, 4.4]` — see [`fit_distance`]'s
/// derivation and its doc) without either plane clipping real geometry in
/// the common (zoom ≈ 1) case.
const NEAR_FRACTION: f64 = 0.001;
const FAR_FRACTION: f64 = 20.0;

/// Small multiplicative margin on the fit-to-extent bounding-sphere radius
/// so a pole landing exactly tangent to the frustum wall clears
/// floating-point rounding (`|NDC| <= 1.0`) instead of landing a hair past
/// it.
const FIT_MARGIN: f64 = 1.02;

/// Bounding-sphere radius that safely contains the shape [`sdf::
/// scene_extent_mm`] actually bounds: a cylinder of radius `extent_mm/2`
/// spanning `y ∈ [0, extent_mm]` (both hold because
/// `extent_mm = max(2·r_max, y_span)` — see that function's doc), centered
/// at `(0, extent_mm/2, 0)`. The worst-case corner (full radial reach AND a
/// full axial half-span SIMULTANEOUSLY, e.g. `(extent_mm/2, 0, 0)`) sits at
/// `extent_mm/sqrt(2)` from that center — NOT `extent_mm/2`, which would
/// under-cover a wide, flat spring's outer coil by a factor of `sqrt(2)`.
fn fit_sphere_radius(extent_mm: f64) -> f64 {
    (extent_mm / std::f64::consts::SQRT_2) * FIT_MARGIN
}

/// Eye-to-target distance that fits [`fit_sphere_radius`]'s bounding sphere
/// exactly inside a [`FOV_Y_RAD`]-tall, `aspect`-wide perspective frustum:
/// the sphere's angular half-size as seen from the eye is `asin(R /
/// distance)`, so `distance = R / sin(theta)` where `theta` is the MORE
/// restrictive of the vertical and horizontal half-fields-of-view
/// (whichever is narrower — vertical for `aspect >= 1` landscape,
/// horizontal for `aspect < 1` portrait; `tan(fov_x/2) = aspect ×
/// tan(fov_y/2)` is the standard aspect-derived horizontal FOV).
fn fit_distance(extent_mm: f64, aspect: f64) -> f64 {
    let radius = fit_sphere_radius(extent_mm);
    let half_v = FOV_Y_RAD / 2.0;
    let half_h = (aspect * half_v.tan()).atan();
    let half_theta = half_v.min(half_h);
    radius / half_theta.sin()
}

/// World-space eye position for the given orbit/zoom/extent: spherical
/// coordinates about the scene's target `(0, extent_mm/2, 0)` (see
/// [`fit_sphere_radius`]'s doc for why that height, not the origin) at
/// [`fit_distance`] divided by `zoom` (`zoom > 1` moves the eye closer —
/// magnified). `yaw` rotates about world Y, `pitch` tilts toward/away from
/// it — the same [`Orbit`] angles the wireframe path already carries.
fn eye_position(orbit: Orbit, zoom: f32, extent_mm: f64, aspect: f64) -> [f64; 3] {
    let distance = fit_distance(extent_mm, aspect) / f64::from(zoom);
    let yaw = f64::from(orbit.yaw);
    let pitch = f64::from(orbit.pitch);
    let dir = [
        pitch.cos() * yaw.sin(),
        pitch.sin(),
        pitch.cos() * yaw.cos(),
    ];
    [
        distance * dir[0],
        extent_mm / 2.0 + distance * dir[1],
        distance * dir[2],
    ]
}

/// View-projection matrix + its inverse for the shaded-3D camera, packed
/// into a fixed 32-`f32` array — Task 5's WGSL uniform reads this layout
/// bit-for-bit: `[0..16)` = `view_proj` (world → clip, column-major, the
/// standard WGSL `mat4x4<f32>` layout — column `c`'s four floats are
/// `[c*4 .. c*4+4)`), `[16..32)` = its inverse. Camera: perspective, 45°
/// vertical FOV ([`FOV_Y_RAD`]), looking at `(0, extent_mm/2, 0)` from
/// [`eye_position`] (fit-to-extent distance divided by `zoom`), world-Y up,
/// near/far derived from `extent_mm` ([`NEAR_FRACTION`]/[`FAR_FRACTION`]).
///
/// **No separate stored eye position.** Task 5's vertex shader reconstructs
/// each pixel's world-space ray directly from the INVERSE block by
/// unprojecting that pixel's near- and far-plane NDC points (standard
/// technique — both lie ON the eye-to-pixel ray, so their difference gives
/// the ray direction without the eye ever needing to be a separately stored
/// value — see the plan's Task 5 entry, "pass NDC ray through the inverse
/// view-proj"). This task's own "pinned eye distance" tests exercise
/// [`eye_position`]/[`fit_distance`] directly as private helpers instead —
/// Task 5 should NOT hunt for a third stored field; there isn't one.
///
/// A non-finite or non-positive `extent_mm`/`aspect` falls back to `1.0`
/// (defense in depth — a pure fn reachable directly by tests/future
/// callers, mirroring [`stroke_for`]'s non-finite guard); `zoom` is clamped
/// to [`ZOOM_MIN`]/[`ZOOM_MAX`] even though the committed `App.zoom` state
/// is expected to already be clamped by [`zoom_step`].
pub(crate) fn camera_uniforms(extent_mm: f64, orbit: Orbit, zoom: f32, aspect: f32) -> [f32; 32] {
    let extent = if extent_mm.is_finite() && extent_mm > 0.0 {
        extent_mm
    } else {
        1.0
    };
    let aspect = if aspect.is_finite() && aspect > 0.0 {
        f64::from(aspect)
    } else {
        1.0
    };
    let zoom = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
    let eye = eye_position(orbit, zoom, extent, aspect);
    let target = [0.0, extent / 2.0, 0.0];
    let up = [0.0, 1.0, 0.0];
    let near = extent * NEAR_FRACTION;
    let far = (extent * FAR_FRACTION).max(near * 2.0);
    let view = mat4_look_at(eye, target, up);
    let proj = mat4_perspective(FOV_Y_RAD, aspect, near, far);
    let view_proj = mat4_mul(&proj, &view);
    let inv_view_proj = mat4_invert(&view_proj);
    let mut out = [0.0f32; 32];
    out[0..16].copy_from_slice(&mat4_to_f32(&view_proj));
    out[16..32].copy_from_slice(&mat4_to_f32(&inv_view_proj));
    out
}

/// Pure chooser for the results panel's 3D slot: the shaded path runs IFF
/// a GPU adapter was found by the boot probe (`App.shader_available`) AND
/// the scene is representable in the packed uniform budget
/// ([`sdf::scene_uniforms`] returned `Some`). Everything else — no
/// adapter, over-budget scene — falls back to the wireframe path.
pub(crate) fn use_shaded(shader_available: bool, uniforms: Option<&Vec<f32>>) -> bool {
    shader_available && uniforms.is_some()
}

/// The nominal camera aspect for the shaded view: the same 760×300 frame
/// the wireframe bitmap pipeline rasterizes at (`plot::CHART_W`/`CHART_H`).
///
/// **Accepted limitation (nominal, not live, bounds).** `shader::Program::
/// draw` and `shader::Primitive::prepare` DO receive the widget's real
/// layout `Rectangle` every frame (`SpringShader::draw`/`SpringPrimitive::
/// prepare` in `shader3d.rs` both take a `bounds` parameter today — unused,
/// hence the leading underscore on each). The actual constraint is that
/// `SpringShader` carries a camera PRE-PACKED at `view()` time: this
/// function's caller, `spring3d_element`, builds `camera_uniforms` once from
/// `chart_aspect`'s nominal ratio before the shader widget ever sees a real
/// bounds value, and `draw` only clones that already-packed camera through
/// (Task 5's contract). Threading live bounds into the camera would need a
/// `SpringShader` field/flow change to carry them forward to where the
/// camera is built — deferred, not attempted here. The widget fills the
/// same Fill×300 slot as the wireframe canvas; at panel widths other than
/// 760 px the horizontal field of view is proportionally wider or narrower
/// than exact. Fit-framing still holds for anything at least as wide as it
/// is tall: [`fit_distance`] takes the MORE restrictive of the vertical and
/// aspect-derived horizontal half-FOV, and the vertical (fixed) one governs
/// for all aspects ≥ 1, so a wider-than-nominal panel only adds margin.
fn chart_aspect() -> f32 {
    crate::plot::CHART_W as f32 / crate::plot::CHART_H as f32
}

/// The shaded path's clear color: the same panel surface the wireframe
/// bitmap fills, as the LINEAR RGBA floats the WGSL `Camera.bg` slot
/// expects.
///
/// **Linear, not raw sRGB (review F1 fix).** `Palette`'s tokens (like every
/// other color authored in this app) are ordinary sRGB display values, but
/// this app's `iced` compositor picks an sRGB-FORMAT swapchain texture
/// whenever gamma correction is enabled (the default: `iced_wgpu`'s
/// `Compositor::new` calls `formats.find(TextureFormat::is_srgb)` under
/// `color::GAMMA_CORRECTION`, which is `true` unless the `web-colors`
/// feature is on — not enabled here) — writing to such a target, the
/// hardware itself re-encodes whatever the fragment shader returns from
/// linear to sRGB on store. `into_linear` (the same conversion every other
/// `iced` rendering pipeline applies before handing a `Color` to the GPU)
/// undoes the authoring convention so the shader's `ambient = luminance(bg)`
/// term operates on genuinely linear light, and the raw clear color the GPU
/// eventually stores back matches the authored panel color instead of being
/// double-encoded.
fn bg_rgba(pal: &crate::app::Palette) -> [f32; 4] {
    pal.panel.into_linear()
}

/// The results panel's shared 3D element: shaded ray-marched view when
/// [`use_shaded`] says so, the existing wireframe canvas otherwise.
///
/// **Degenerate scenes short-circuit to the placeholder BEFORE choosing.**
/// An empty [`sdf::SdfScene`] is "representable" to `scene_uniforms` (zero
/// parts pack fine and render pure background), so without this gate a
/// degenerate design on the shaded path would show an empty background
/// where the wireframe path shows the placeholder. Both geometry paths'
/// degenerate verdicts are checked — the WIREFRAME's via the same
/// `scene_extent` + `frame_ranges` tests `render_scene` itself performs,
/// the SDF's via [`sdf::scene_extent_mm`] — and EITHER being degenerate
/// shows the placeholder. The two verdicts agree for every real design
/// (the family SDF builders and scene presenters share the same hostility
/// rules: non-finite geometry and hostile coil counts empty both); if they
/// ever disagree, the wireframe scene's verdict governs the WORDING via
/// [`canvas3d`]'s `placeholder_for`, per the shipped placeholder contract.
///
/// The shaded widget matches the wireframe slot's sizing (Fill × the
/// bitmap height) and builds its camera at the nominal [`chart_aspect`]
/// (see that doc for the accepted live-bounds limitation).
pub(crate) fn spring3d_element(
    pal: &'static crate::app::Palette,
    scene: SceneData,
    sdf_scene: sdf::SdfScene,
    orbit: Orbit,
    zoom: f32,
    shader_available: bool,
) -> iced::Element<'static, crate::app::Message> {
    let wireframe_frames = scene_extent(&scene)
        .as_ref()
        .and_then(render3d::frame_ranges)
        .is_some();
    let (true, Some(extent_mm)) = (wireframe_frames, sdf::scene_extent_mm(&sdf_scene)) else {
        return crate::widgets::placeholder_text(pal, canvas3d::placeholder_for(&scene));
    };
    let uniforms = sdf::scene_uniforms(&sdf_scene);
    if !use_shaded(shader_available, uniforms.as_ref()) {
        return scene_element(pal, scene, orbit);
    }
    let Some(uniforms) = uniforms else {
        // Unreachable: `use_shaded` is true only when `uniforms` is `Some`.
        // A typed fallback to the same wireframe path rather than an
        // `unwrap` — the two branches are behaviorally identical, so this
        // cannot drift from the chooser's verdict.
        return scene_element(pal, scene, orbit);
    };
    let camera = camera_uniforms(extent_mm, orbit, zoom, chart_aspect());
    let program = shader3d::SpringShader {
        uniforms,
        camera,
        bg: bg_rgba(pal),
    };
    iced::widget::shader::Shader::new(program)
        .width(iced::Length::Fill)
        .height(iced::Length::Fixed(crate::plot::CHART_H as f32))
        .into()
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
    fn helix_caps_hostile_turns_to_empty() {
        // Non-finite turns: NaN and inf both must empty out rather than
        // overflow the `as usize` cast into a capacity-overflow panic.
        assert!(helix(|_| 1.0, |_| 1.0, f64::NAN, 32).is_empty());
        assert!(helix(|_| 1.0, |_| 1.0, f64::INFINITY, 32).is_empty());
        // A huge but finite turns count (form validation only checks
        // positive+finite) must also empty out, not allocate ~768MB/frame.
        assert!(helix(|_| 1.0, |_| 1.0, 1e18, 32).is_empty());
        // Negative turns never panic (the float→usize cast saturates to 0,
        // yielding a 3-point arc), but a helix with negative turns is
        // meaningless — treat it as degenerate like scene_from_radius's
        // entry guard does, so the two guards stay symmetric.
        assert!(helix(|_| 1.0, |_| 1.0, -5.0, 32).is_empty());
    }

    #[test]
    fn helix_renders_exactly_at_the_cap_and_empties_just_above() {
        let at_cap = helix(|_| 1.0, |_| 1.0, MAX_RENDER_TURNS, 32);
        assert_eq!(at_cap.len(), (MAX_RENDER_TURNS * 32.0) as usize + 1); // ≈64k points, ~1.5MB
        let just_over = helix(|_| 1.0, |_| 1.0, MAX_RENDER_TURNS + 1.0, 32);
        assert!(just_over.is_empty());
    }

    #[test]
    fn scene_from_radius_hostile_turns_yields_a_degenerate_scene() {
        // End-to-end: the sampler cap must reach through `scene_from_radius`
        // (and therefore every family calling it) so `scene_extent` sees
        // `None` and the shipped placeholder fires, instead of the `view()`
        // capacity-overflow panic the adversary proved (form `active=1e18,
        // free=1e19` solves, then the scene build panics).
        let scene = scene_from_radius(|_| 10.0, 10.0, 1e18, 1e18, 5.0, 2.0);
        assert!(scene_extent(&scene).is_none());
    }

    #[test]
    fn scene_from_radius_non_finite_or_negative_coils_yield_a_degenerate_scene() {
        // A NaN or negative active count reaches `coil_height_fn`'s
        // `clamp(0.0, active)` through the unconditional stroke-sizing
        // `height(1.0)` call BEFORE helix's turns guard can fire — std clamp
        // PANICS when max is NaN or min > max ("min > max, or either was
        // NaN"). The entry guard must return the degenerate scene instead.
        let nan_active = scene_from_radius(|_| 10.0, 10.0, f64::NAN, 10.0, 5.0, 2.0);
        assert!(scene_extent(&nan_active).is_none());
        let negative_active = scene_from_radius(|_| 10.0, 10.0, -1.0, 1.0, 5.0, 2.0);
        assert!(scene_extent(&negative_active).is_none());
        let nan_total = scene_from_radius(|_| 10.0, 10.0, 10.0, f64::NAN, 5.0, 2.0);
        assert!(scene_extent(&nan_total).is_none());
        let negative_total = scene_from_radius(|_| 10.0, 10.0, 10.0, -2.0, 5.0, 2.0);
        assert!(scene_extent(&negative_total).is_none());
    }

    #[test]
    fn close_wound_coil_nan_turns_yield_a_degenerate_scene() {
        // `close_wound_coil` passes `turns` as BOTH active and total, so a
        // NaN reaches the clamp-panic path exactly like the case above —
        // this is the extension/torsion families' route into the guard.
        assert!(scene_extent(&close_wound_coil(10.0, f64::NAN, 2.0)).is_none());
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
    fn stroke_for_zero_or_non_finite_extent_floors_to_one() {
        // Zero extent: ratio is +inf (not NaN, since wire_mm > 0), but is still
        // non-finite and must floor to 1 rather than clamp to the top of the range.
        assert_eq!(stroke_for(2.0, 0.0), 1);
        // A NaN extent propagates NaN through the ratio; `NaN.clamp(1.0, 8.0)` is
        // a no-op (NaN compares false to both bounds), and `NaN as u32` truncates
        // to 0 — silently violating the documented [1, 8] range.
        assert_eq!(stroke_for(2.0, f64::NAN), 1);
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

    #[test]
    fn orbit_step_ignores_non_finite_deltas() {
        let current = Orbit {
            yaw: 0.3,
            pitch: -0.2,
        };
        assert_eq!(orbit_step(current, f32::NAN, 1.0), current);
        assert_eq!(orbit_step(current, 1.0, f32::NAN), current);
        assert_eq!(orbit_step(current, f32::INFINITY, 0.0), current);
        assert_eq!(orbit_step(current, 0.0, f32::INFINITY), current);
    }

    // ------------------------------------------------------------------
    // Task 4: zoom step
    // ------------------------------------------------------------------

    #[test]
    fn zoom_step_scales_multiplicatively_and_clamps_at_exact_bounds() {
        assert_relative_eq!(zoom_step(1.0, 0.0), 1.0, max_relative = 1e-6); // no-op scroll
        let up = zoom_step(1.0, 1.0);
        assert_relative_eq!(up, ZOOM_SENSITIVITY.exp(), max_relative = 1e-6);
        let down = zoom_step(up, -1.0);
        assert_relative_eq!(down, 1.0, max_relative = 1e-6); // exactly undoes the prior step
                                                             // Clamps at EXACTLY the bounds for extreme deltas (orbit_step precedent).
        assert_eq!(zoom_step(1.0, 1000.0), ZOOM_MAX);
        assert_eq!(zoom_step(1.0, -1000.0), ZOOM_MIN);
    }

    #[test]
    fn zoom_step_ignores_non_finite_deltas() {
        assert_eq!(zoom_step(1.5, f32::NAN), 1.5);
        assert_eq!(zoom_step(1.5, f32::INFINITY), 1.5);
        assert_eq!(zoom_step(1.5, f32::NEG_INFINITY), 1.5);
    }

    // ------------------------------------------------------------------
    // Task 6: shaded/wireframe chooser
    // ------------------------------------------------------------------

    /// The full 2×2 truth table: the shaded path runs IFF a GPU adapter was
    /// probed at boot AND the scene packed into the uniform budget. Any
    /// other combination — no adapter, unrepresentable scene, or both —
    /// falls back to the wireframe path.
    #[test]
    fn use_shaded_requires_adapter_and_representable_scene() {
        let packed = vec![0.0f32; 4];
        assert!(!use_shaded(false, None));
        assert!(!use_shaded(false, Some(&packed)));
        assert!(!use_shaded(true, None));
        assert!(use_shaded(true, Some(&packed)));
    }

    // ------------------------------------------------------------------
    // Review F1 fix: gamma — the shaded background must be LINEAR, not raw
    // sRGB, since the shader writes to an sRGB-format compositor target
    // (hardware re-encodes linear -> sRGB on store).
    // ------------------------------------------------------------------

    #[test]
    fn bg_rgba_returns_the_panel_color_linearized() {
        // Neither DARK's nor LIGHT's `panel` sits at a 0/1 fixed point of the
        // sRGB EOTF, so this pin has real teeth: a regression back to raw
        // components would fail it (revert-probed below in the fix commit).
        for pal in [&crate::app::DARK, &crate::app::LIGHT] {
            assert_eq!(bg_rgba(pal), pal.panel.into_linear());
        }
    }

    // ------------------------------------------------------------------
    // Task 4: camera math
    // ------------------------------------------------------------------

    #[test]
    fn fit_distance_matches_the_independently_computed_value_at_two_aspects() {
        // extent=100mm; hand-computed (Python) via
        // R = 100/sqrt(2)*1.02 = 72.12489168102783,
        // half_v = 22.5 deg exactly.
        // aspect=1.0: half_h == half_v (symmetric) -> theta = 22.5 deg.
        assert_relative_eq!(
            fit_distance(100.0, 1.0),
            188.47142463230244,
            max_relative = 1e-9
        );
        // aspect=2.0 (wide): half_h = atan(2*tan(22.5deg)) = 39.639...deg >
        // half_v, so vertical still governs -> same distance as aspect=1.0.
        assert_relative_eq!(
            fit_distance(100.0, 2.0),
            188.47142463230244,
            max_relative = 1e-9
        );
        // aspect=0.5 (tall/portrait): half_h = atan(0.5*tan(22.5deg)) =
        // 11.7...deg < half_v, so horizontal governs -> a LARGER distance.
        assert_relative_eq!(
            fit_distance(100.0, 0.5),
            355.6401434198882,
            max_relative = 1e-9
        );
    }

    #[test]
    fn eye_position_matches_the_independently_computed_pinned_vector() {
        // extent=60mm, aspect=16/9, zoom=1.5, yaw=0.3, pitch=0.2 rad (as
        // ACTUALLY stored — `Orbit`'s fields are `f32`, so the literals
        // below round to the nearest f32 before this function ever sees
        // them: yaw = 0.300000011920928..., pitch = 0.200000002980232...) —
        // hand-computed (Python, independent of this module's Rust code,
        // starting from those SAME f32-rounded values) via the documented
        // formula: eye = (0, extent/2, 0) + (fit_distance(extent, aspect) /
        // zoom) * (cos(pitch)sin(yaw), sin(pitch), cos(pitch)cos(yaw)).
        let orbit = Orbit {
            yaw: 0.3,
            pitch: 0.2,
        };
        let eye = eye_position(orbit, 1.5, 60.0, 16.0 / 9.0);
        assert_relative_eq!(eye[0], 21.834752933693835, max_relative = 1e-9);
        assert_relative_eq!(eye[1], 44.97739694247343, max_relative = 1e-9);
        assert_relative_eq!(eye[2], 70.5858173404607, max_relative = 1e-9);
    }

    #[test]
    fn eye_position_distance_from_target_is_fit_distance_over_zoom() {
        let orbit = Orbit {
            yaw: 0.4,
            pitch: -0.35,
        };
        let (extent, aspect, zoom) = (80.0, 1.6, 2.0);
        let eye = eye_position(orbit, zoom, extent, aspect);
        let target = [0.0, extent / 2.0, 0.0];
        let dist = ((eye[0] - target[0]).powi(2)
            + (eye[1] - target[1]).powi(2)
            + (eye[2] - target[2]).powi(2))
        .sqrt();
        assert_relative_eq!(
            dist,
            fit_distance(extent, aspect) / f64::from(zoom),
            max_relative = 1e-9
        );
    }

    #[test]
    fn mat4_mul_with_identity_is_a_no_op() {
        let m = mat4_look_at([10.0, 5.0, 3.0], [0.0, 2.0, 0.0], [0.0, 1.0, 0.0]);
        let id = mat4_identity();
        let product = mat4_mul(&m, &id);
        for i in 0..16 {
            assert_relative_eq!(product[i], m[i], max_relative = 1e-9);
        }
    }

    #[test]
    fn camera_uniforms_matrix_times_its_inverse_is_the_identity() {
        let out = camera_uniforms(
            120.0,
            Orbit {
                yaw: 0.9,
                pitch: 0.25,
            },
            1.0,
            1.5,
        );
        let view_proj: Mat4 = std::array::from_fn(|i| f64::from(out[i]));
        let inv_view_proj: Mat4 = std::array::from_fn(|i| f64::from(out[16 + i]));
        let product = mat4_mul(&view_proj, &inv_view_proj);
        let identity = mat4_identity();
        for i in 0..16 {
            assert!(
                (product[i] - identity[i]).abs() < 1e-2,
                "index {i}: {} vs identity {}",
                product[i],
                identity[i]
            );
        }
    }

    #[test]
    fn camera_uniforms_keeps_the_extent_sphere_inside_the_frustum_at_two_aspects() {
        // Project the fit-to-extent bounding sphere's 6 world-axis poles
        // (the TRUE, unpadded sphere the scene actually needs contained —
        // extent/sqrt(2), not fit_sphere_radius's padded value) through the
        // forward view-proj matrix at two very different aspect ratios
        // (wide and tall) and confirm every pole's NDC x/y stays within
        // [-1, 1] (a tiny epsilon absorbs f32 rounding at the tangent
        // boundary).
        let extent = 90.0;
        let orbit = Orbit {
            yaw: 0.5,
            pitch: -0.3,
        };
        let center = [0.0, extent / 2.0, 0.0];
        let true_radius = extent / std::f64::consts::SQRT_2;
        for aspect in [1.777_f32, 0.5625_f32] {
            let out = camera_uniforms(extent, orbit, 1.0, aspect);
            let view_proj: Mat4 = std::array::from_fn(|i| f64::from(out[i]));
            for axis in 0..3 {
                for sign in [-1.0, 1.0] {
                    let mut pole = center;
                    pole[axis] += sign * true_radius;
                    let clip = mat4_mul_vec4(&view_proj, [pole[0], pole[1], pole[2], 1.0]);
                    assert!(
                        clip[3] > 0.0,
                        "aspect {aspect}: pole {pole:?} landed behind the camera (w={})",
                        clip[3]
                    );
                    let ndc_x = clip[0] / clip[3];
                    let ndc_y = clip[1] / clip[3];
                    assert!(
                        ndc_x.abs() <= 1.0 + 1e-6,
                        "aspect {aspect}: pole {pole:?} ndc_x={ndc_x}"
                    );
                    assert!(
                        ndc_y.abs() <= 1.0 + 1e-6,
                        "aspect {aspect}: pole {pole:?} ndc_y={ndc_y}"
                    );
                }
            }
        }
    }

    #[test]
    fn camera_uniforms_non_finite_extent_and_aspect_do_not_produce_nan() {
        let out = camera_uniforms(f64::NAN, Orbit::default(), 1.0, f32::INFINITY);
        assert!(out.iter().all(|v| v.is_finite()), "{out:?}");
        let out_neg = camera_uniforms(-5.0, Orbit::default(), 1.0, -1.0);
        assert!(out_neg.iter().all(|v| v.is_finite()), "{out_neg:?}");
    }
}
