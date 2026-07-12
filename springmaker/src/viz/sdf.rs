//! Pure SDF (signed-distance-function) core — the Rust mirror of the WGSL
//! ray-marching fragment shader (Task 5 mirrors this file function-for-
//! function). ADR 0008 purity is BINDING here: no `iced`, no `wgpu`, no
//! rendering types — plain `f64` math and plain data only.
//!
//! **Conservativeness invariant.** Every distance function below must
//! never OVER-estimate the true distance from a point to the surface it
//! describes — sphere-tracing correctness depends on it (an overestimate
//! lets the marcher step past a thin feature and miss the intersection
//! entirely). Every STANDALONE primitive here (capsule, torus arc, profile)
//! is proven EXACT, which trivially satisfies the invariant; the CSG
//! combinator [`cut_plane`] is conservative-but-not-exact near the cut seam
//! (see its doc). Each function's doc comment carries its own argument.

use std::f64::consts::TAU;

/// A point or vector in millimetres, in whatever local frame the caller
/// established (world frame for scene composition; a part's own local
/// frame for primitive evaluation).
pub(crate) type Vec3 = [f64; 3];

fn vsub(u: Vec3, v: Vec3) -> Vec3 {
    [u[0] - v[0], u[1] - v[1], u[2] - v[2]]
}

fn vadd(u: Vec3, v: Vec3) -> Vec3 {
    [u[0] + v[0], u[1] + v[1], u[2] + v[2]]
}

fn vscale(u: Vec3, s: f64) -> Vec3 {
    [u[0] * s, u[1] * s, u[2] * s]
}

fn vdot(u: Vec3, v: Vec3) -> f64 {
    u[0] * v[0] + u[1] * v[1] + u[2] * v[2]
}

fn vlen(u: Vec3) -> f64 {
    vdot(u, u).sqrt()
}

/// Cross-section shape swept along a wire's centerline, evaluated in the
/// wire's local (radial, axial) plane by [`sd_profile_2d`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)] // consumed by Task 2 (rectangular-family scenes) and Task 5 (WGSL mirror)
pub(crate) enum Profile {
    Circle { radius_mm: f64 },
    Rectangle { half_w_mm: f64, half_h_mm: f64 },
}

/// Forward-ready per-part material description (design doc §Decisions 2):
/// a future material-DB swaps values in without touching the contract.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Appearance {
    pub base_color: [f32; 3],
    pub metallic: f32,
    pub roughness: f32,
}

/// Default spring-steel look shared by every family's primary wire.
#[allow(dead_code)] // consumed by Task 2 (SdfScene part construction)
pub(crate) fn steel() -> Appearance {
    Appearance {
        base_color: [0.62, 0.64, 0.67],
        metallic: 0.9,
        roughness: 0.35,
    }
}

/// Hue-shift table backing [`member_appearance`]: differentiates each
/// assembly member's tint while holding metallic/roughness fixed, wrapping
/// every [`MEMBER_HUE_TABLE_LEN`] members (mirrors `SceneRole::Member`'s
/// two-way wireframe alternation upstream, extended to N tints for the
/// shaded view). Dead until Task 2 wires assembly member coloring.
#[allow(dead_code)] // consumed by Task 2 via member_appearance
const MEMBER_HUE_SHIFTS: [[f32; 3]; 4] = [
    [0.0, 0.0, 0.0],
    [0.05, -0.03, -0.05],
    [-0.05, 0.03, 0.05],
    [0.08, 0.04, -0.06],
];

/// Documented length of [`MEMBER_HUE_SHIFTS`] — exported so callers (and
/// this module's own property test) read the real table length rather
/// than a copied literal.
#[allow(dead_code)] // consumed by Task 2 via member_appearance
pub(crate) const MEMBER_HUE_TABLE_LEN: usize = MEMBER_HUE_SHIFTS.len();

/// Steel tinted by `index`'s hue shift, wrapping every
/// [`MEMBER_HUE_TABLE_LEN`] members.
#[allow(dead_code)] // consumed by Task 2 (assembly member coloring)
pub(crate) fn member_appearance(index: usize) -> Appearance {
    let base = steel();
    let shift = MEMBER_HUE_SHIFTS[index % MEMBER_HUE_TABLE_LEN];
    Appearance {
        base_color: [
            (base.base_color[0] + shift[0]).clamp(0.0, 1.0),
            (base.base_color[1] + shift[1]).clamp(0.0, 1.0),
            (base.base_color[2] + shift[2]).clamp(0.0, 1.0),
        ],
        ..base
    }
}

/// Exact distance to a capsule (round-swept segment): the standard
/// nearest-point-on-segment construction — `h` clamps to `[0, 1]` so
/// beyond the endpoints the closest point locks to `a` or `b`, giving the
/// hemispherical end caps for free. Exact everywhere, hence trivially
/// conservative (never overestimates) — sphere-tracing may rely on it
/// without slack.
#[allow(dead_code)] // consumed by Task 2 (torsion legs) and Task 5 (WGSL mirror)
pub(crate) fn sd_capsule(p: Vec3, a: Vec3, b: Vec3, radius_mm: f64) -> f64 {
    let seg = vsub(b, a);
    let from_a = vsub(p, a);
    let seg_len_sq = vdot(seg, seg);
    let h = if seg_len_sq > 0.0 {
        (vdot(from_a, seg) / seg_len_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let closest = vadd(a, vscale(seg, h));
    vlen(vsub(p, closest)) - radius_mm
}

/// Exact distance to a torus ARC in its local frame (axis = local Y; the
/// ring lies in the local XZ plane at radius `major_r`; the arc sweeps
/// azimuth `0 -> sweep`, capped at both ends).
///
/// **Why this is exact, not merely conservative.** The squared distance
/// from `p` to the full ring's point at azimuth θ is
/// `R_p² + major_r² + p.y² - 2·major_r·R_p·cos(θ - φ_p)`, where `R_p` is
/// `p`'s radial distance from the axis and `φ_p` its azimuth — a cosine in
/// θ shifted by `φ_p`, unimodal with its single minimum at `θ = φ_p`.
/// Inside the sweep this is exactly the classic closed-form torus SDF
/// (`length(q) - minor_r`, `q = (hypot(x,z) - major_r, y)`). Outside the
/// sweep, the restricted minimum over `θ ∈ [0, sweep]` is achieved at
/// whichever endpoint (θ=0 or θ=sweep) is ANGULARLY nearer `φ_p` — and
/// that ordering always coincides with EUCLIDEAN-nearer: the squared-
/// distance difference between the two endpoints collapses to
/// `2·major_r·R_p·[cos(sweep - φ_p) - cos(φ_p)]`, the identical cosine
/// comparison that decides angular nearness, scaled by a non-negative
/// constant. So `min(|p - C0|, |p - C1|) - minor_r` (`C0`, `C1` the
/// endpoint centerline points) is the EXACT distance to the capped arc's
/// centerline — clamped exactly like [`sd_capsule`]'s segment clamp, not
/// an approximation.
#[allow(dead_code)] // consumed by Task 2 (extension hooks) and Task 5 (WGSL mirror)
pub(crate) fn sd_torus_arc(p_local: Vec3, major_r: f64, minor_r: f64, sweep: f64) -> f64 {
    let azimuth = p_local[2].atan2(p_local[0]).rem_euclid(TAU);
    if (0.0..=sweep).contains(&azimuth) {
        let radial = p_local[0].hypot(p_local[2]) - major_r;
        return radial.hypot(p_local[1]) - minor_r;
    }
    let endpoint = |angle: f64| -> Vec3 { [major_r * angle.cos(), 0.0, major_r * angle.sin()] };
    let dist_to_start = vlen(vsub(p_local, endpoint(0.0)));
    let dist_to_end = vlen(vsub(p_local, endpoint(sweep)));
    dist_to_start.min(dist_to_end) - minor_r
}

/// Distance in the wire's local cross-section plane (radial x axial).
/// `Circle` is exact (`hypot(d_radial, d_axial) - radius_mm`); `Rectangle`
/// is the classic 2D box SDF (`length(max(q, 0)) + min(max(q.x, q.y), 0)`,
/// `q = (|d_radial| - half_w, |d_axial| - half_h)`) — also exact, the
/// standard construction used throughout SDF literature.
#[allow(dead_code)] // consumed by Task 2 (rectangular-family scenes) and Task 5 (WGSL mirror)
pub(crate) fn sd_profile_2d(d_radial: f64, d_axial: f64, profile: Profile) -> f64 {
    match profile {
        Profile::Circle { radius_mm } => d_radial.hypot(d_axial) - radius_mm,
        Profile::Rectangle {
            half_w_mm,
            half_h_mm,
        } => {
            let qx = d_radial.abs() - half_w_mm;
            let qy = d_axial.abs() - half_h_mm;
            let outside = qx.max(0.0).hypot(qy.max(0.0));
            let inside = qx.max(qy).min(0.0);
            outside + inside
        }
    }
}

/// Half-space intersection: `max(d, dot(p - plane_point, plane_normal))`.
/// `d` is the base shape's signed distance at `p`; the plane's own signed
/// distance is positive on the side `plane_normal` points to (the side cut
/// AWAY) and negative on the kept side. `max` is the standard CSG
/// intersection combinator — CONSERVATIVE but not exact: near the cut seam
/// (where the base surface meets the plane, e.g. a ground end's rim) it
/// UNDER-estimates the true distance to the composed boundary, which is the
/// safe direction for sphere tracing. Away from the seam it is exact.
#[allow(dead_code)] // consumed by Task 2 (ground-end cuts) and Task 5 (WGSL mirror)
pub(crate) fn cut_plane(d: f64, p: Vec3, plane_point: Vec3, plane_normal: Vec3) -> f64 {
    d.max(vdot(vsub(p, plane_point), plane_normal))
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-6;

    #[test]
    fn capsule_surface_points_are_zero() {
        let (a, b, r) = ([0.0, 0.0, 0.0], [0.0, 10.0, 0.0], 2.0);
        // On the barrel:
        assert!(sd_capsule([2.0, 5.0, 0.0], a, b, r).abs() < EPS);
        // On the end sphere:
        assert!(sd_capsule([0.0, -2.0, 0.0], a, b, r).abs() < EPS);
        // Inside negative / outside positive:
        assert!(sd_capsule([0.0, 5.0, 0.0], a, b, r) < 0.0);
        assert!(sd_capsule([5.0, 5.0, 0.0], a, b, r) > 0.0);
        // Exact distance outside: 3 mm radial => 1.0
        assert!((sd_capsule([5.0, 5.0, 0.0], a, b, r) - 3.0).abs() < EPS);
    }

    #[test]
    fn torus_arc_matches_full_torus_inside_sweep_and_caps_beyond() {
        let (maj, min) = (10.0, 1.5);
        let ang = std::f64::consts::FRAC_PI_4;
        let sweep = 1.5 * std::f64::consts::PI;
        // Full-circle point at angle pi/4 (inside a 1.5*pi sweep): classic
        // torus distance (center of the tube ring is -minor_r deep).
        let on_ring = [maj * ang.cos(), 0.0, maj * ang.sin()];
        assert!(
            (sd_torus_arc(on_ring, maj, min, sweep).abs() - min).abs() < EPS
                || (sd_torus_arc(on_ring, maj, min, sweep) + min).abs() < EPS
        );
        // Beyond the sweep: nearest endpoint-circle governs (distance from
        // the angular gap) and must be strictly outside.
        let past = [
            maj * (1.6 * std::f64::consts::PI).cos(),
            0.0,
            maj * (1.6 * std::f64::consts::PI).sin(),
        ];
        assert!(sd_torus_arc(past, maj, min, sweep) > 0.0);
    }

    #[test]
    fn torus_arc_is_continuous_at_the_sweep_boundary() {
        // Mirror-drift insurance (design doc's named #1 risk): the
        // in-sweep branch (classic torus formula) and the out-of-sweep
        // branch (nearest-endpoint) are both proven exact, so they must
        // agree AT the boundary itself. A WGSL port that breaks this would
        // show as a visible seam on the rendered hook.
        let (maj, min) = (10.0, 1.5);
        let sweep = 1.5 * std::f64::consts::PI;
        let on_boundary = [maj * sweep.cos(), 0.3, maj * sweep.sin()];
        let just_past_angle = sweep + 1e-9;
        let just_past = [
            maj * just_past_angle.cos(),
            0.3,
            maj * just_past_angle.sin(),
        ];
        let inside_value = sd_torus_arc(on_boundary, maj, min, sweep);
        let outside_value = sd_torus_arc(just_past, maj, min, sweep);
        assert!((inside_value - outside_value).abs() < 1e-6);
    }

    #[test]
    fn profile_circle_vs_rectangle() {
        assert!((sd_profile_2d(3.0, 4.0, Profile::Circle { radius_mm: 5.0 })).abs() < EPS); // 3-4-5
        let r = Profile::Rectangle {
            half_w_mm: 2.0,
            half_h_mm: 1.0,
        };
        assert!((sd_profile_2d(3.0, 0.0, r) - 1.0).abs() < EPS); // 1 beyond half_w
        assert!(sd_profile_2d(0.0, 0.0, r) < 0.0); // center inside
        assert!((sd_profile_2d(3.0, 2.0, r) - std::f64::consts::SQRT_2).abs() < EPS);
        // corner
    }

    #[test]
    fn plane_cut_is_exact_halfspace() {
        // A point 2mm above the plane keeps its (smaller) base distance
        // replaced by 2.0 (the plane is the nearer surface there).
        assert!(
            (cut_plane(-5.0, [0.0, 2.0, 0.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]) - 2.0).abs() < EPS
        );
        // Below the plane but NEARER it than the base surface: the plane
        // governs the reported depth (max(-5.0, -3.0) == -3.0).
        assert!(
            (cut_plane(-5.0, [0.0, -3.0, 0.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]) + 3.0).abs() < EPS
        );
        // Below the plane by MORE than the base depth: the base distance
        // wins, since it is the nearer surface (max(-5.0, -10.0) == -5.0).
        assert!(
            (cut_plane(-5.0, [0.0, -10.0, 0.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]) + 5.0).abs()
                < EPS
        );
    }

    #[test]
    fn appearances_are_sane() {
        let s = steel();
        assert!(s.metallic > 0.5 && s.roughness < 0.6);
        assert_ne!(
            member_appearance(0).base_color,
            member_appearance(1).base_color
        );
        // The hue table wraps: member N gets member 0's appearance again.
        assert_eq!(
            member_appearance(0).base_color,
            member_appearance(MEMBER_HUE_TABLE_LEN).base_color
        );
    }
}
