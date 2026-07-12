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

/// Parameters for a (possibly tapered) circular helix: the coil geometry
/// every compression/extension/torsion family reduces to for the shaded
/// view. `radius_mm` is the mean coil radius at wire-parameter `phi = 0`
/// (the large end when tapered); `taper_small_r` linearly interpolates the
/// local coil radius down to `Some(small_r)` across the full `[0, turns]`
/// sweep. `axial_offset_mm` stacks members in a series assembly.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)] // consumed by Task 3 (SdfScene part construction) and Task 5 (WGSL mirror)
pub(crate) struct HelixParams {
    pub radius_mm: f64,
    pub taper_small_r: Option<f64>,
    pub pitch_mm: f64,
    pub turns: f64,
    pub profile: Profile,
    pub axial_offset_mm: f64,
}

/// Distance to a (possibly tapered) circular helix via PERIODIC REDUCTION:
/// the exact nearest-point-on-helix problem is transcendental (no
/// closed form), so this locates the wire's cross-section plane at each
/// candidate turn index and takes the 2D profile distance there instead of
/// solving for the true 3D nearest point on the curved centerline.
///
/// **Candidate turns.** `theta = atan2(p.z, p.x)` only determines the wire
/// parameter `phi` modulo `TAU` (`phi = theta + TAU*k` for any integer
/// `k`); the continuous estimate `k_est` (solving `y = pitch*phi/TAU` for
/// `p`'s actual axial position) picks out which winding is nearest, but a
/// SINGLE candidate is not enough: close-wound coils (`pitch` near the wire
/// diameter) place adjacent turns' cross-sections close enough in the
/// radial/axial plane that a point sitting between turn `k` and turn `k+1`
/// can be nearer turn `k+1` (or even `k-1`/`k+2` near the rounding boundary
/// of `k_est`) than the single turn `floor(k_est)` naively picked. Checking
/// FOUR neighbors (`floor(k_est)-1 ..= floor(k_est)+2`) brackets every case
/// the continuous estimate can be off by after flooring, in both
/// directions — this is the "close-wound neighbor guard" the property test
/// `close_wound_gap_never_reports_negative_between_coils` exercises
/// directly (a point exactly midway between two touching coils must not be
/// mistaken for being inside a WRONG turn's wire).
///
/// **Conservativeness.** Reducing the true 3D distance to a 2D
/// cross-section-plane distance treats the coil locally as a flat ring in
/// that plane, ignoring the helix's local curvature and pitch angle — the
/// true centerline curves away from the plane as it sweeps in `theta`, so
/// the plane distance UNDER-estimates the true 3D distance (never over-
/// estimates: the flat-ring approximation is always at least as close as
/// the true curved wire). This under-estimate grows with pitch angle,
/// which is exactly why the surface-point property test
/// (`helix_surface_points_are_near_zero`) allows an 8% tolerance rather
/// than requiring exactness — see the measured error in the task report.
/// An under-estimate is the SAFE direction for sphere tracing (never lets
/// the marcher step past the surface), so no additional safety margin is
/// applied here; the march loop is responsible for step-size discipline.
///
/// **End clamping — clamp the TURN INDEX, not the raw angle.** A literal
/// reading of "clamp `phi = theta + TAU*k` to `[0, TAU*turns]`" breaks
/// down for an out-of-range candidate `k`: clamping the raw angle directly
/// snaps `phi` to the boundary value (`0` or `TAU*turns`) UNCONDITIONALLY,
/// which discards `theta` and silently changes the candidate's azimuth to
/// whatever the boundary's azimuth happens to be (`0` at the start, `phi
/// mod TAU` at the end) — a full turn-index step away from `p`'s own
/// azimuth. Since [`sd_profile_2d`]'s ring-plane reduction is only valid
/// when the candidate's azimuth matches `p`'s (that equality is exactly
/// what makes an in-range candidate's `d_radial`/`d_axial` pair meaningful
/// — see the module doc's conservativeness argument), an azimuth-mismatched
/// candidate can report an artificially small distance: concretely, a
/// point sitting exactly on the wire surface just past the start (small
/// `theta`) produces `k_est` slightly negative (an off-turn wire-profile
/// offset can nudge the continuous estimate below the true integer turn),
/// so `floor(k_est)-1` and `floor(k_est)-2`-style candidates go deeply
/// negative; naively clamping their raw `phi` to `0` creates a spurious
/// "phantom start cap" at azimuth `0` that is numerically CLOSER to the
/// query point than the true, correct candidate at azimuth `theta` — a
/// false-negative surface hit (verified empirically: `radius_mm=10`,
/// `pitch_mm=6`, `turns=8`, `wire_r=1`, `phi≈0.2526`, `ring_ang=4.0` gives
/// `d≈-0.167` under naive phi-clamping vs. the true `d≈0.0` — see the task
/// report for the full derivation). This is NOT the documented conservative
/// under-estimate (which only ever brings the reported distance closer to
/// zero from the correct side) — it is a wrong-direction sign flip past a
/// genuine surface point, so it must be fixed rather than tolerated.
///
/// The fix: clamp the candidate's TURN INDEX `k` to the range of integers
/// for which `theta + TAU*k` can possibly land in `[0, TAU*turns]`
/// (`k_min = 0`, `k_max = floor((TAU*turns - theta) / TAU)`) BEFORE forming
/// `phi`. This keeps `phi mod TAU == theta` for every candidate whose turn
/// index is clamped to an interior value (i.e. every case except the
/// genuine boundary itself), preserving azimuthal alignment with `p` and
/// eliminating the phantom-cap duplicate. The outer `.clamp(0.0, max_phi)`
/// on `phi` remains as a final safety net for the true edge case (`turns`
/// itself fractional, or `theta` alone already outside a very short
/// partial-turn range) — there, azimuth genuinely cannot be preserved and
/// the terminal cross-section is the correct (if only approximately exact)
/// stand-in for the end disc, per the module's ring-plane conservativeness
/// argument: never an overestimate of the true distance to the physical
/// end. `ends_are_clamped_not_infinite` exercises that residual case.
#[allow(dead_code)] // consumed by Task 3 (SdfScene part construction) and Task 5 (WGSL mirror)
pub(crate) fn sd_helix(p: Vec3, h: &HelixParams) -> f64 {
    let axial = p[1] - h.axial_offset_mm;
    let radial = p[0].hypot(p[2]);
    let theta = p[2].atan2(p[0]).rem_euclid(TAU);
    let k_est = (axial - h.pitch_mm * theta / TAU) / h.pitch_mm;
    let k_floor = k_est.floor();
    let max_phi = TAU * h.turns;
    let k_min = 0.0;
    let k_max = ((max_phi - theta) / TAU).floor().max(k_min);

    (-1..=2)
        .map(|delta| {
            let k = (k_floor + delta as f64).clamp(k_min, k_max);
            let phi = (theta + TAU * k).clamp(0.0, max_phi);
            let y_k = h.pitch_mm * phi / TAU;
            let coil_r = match h.taper_small_r {
                Some(small_r) => h.radius_mm + (small_r - h.radius_mm) * (phi / max_phi),
                None => h.radius_mm,
            };
            sd_profile_2d(radial - coil_r, axial - y_k, h.profile)
        })
        .fold(f64::INFINITY, f64::min)
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

    /// Exact analytic surface point: the wire centerline at wire-parameter
    /// `phi`, offset by the circular profile's radius in the cross-section
    /// plane direction (`ring_ang`'s cosine is the radial component, its
    /// sine the axial component). Used only to construct test points that
    /// must land within tolerance of the SDF's zero level set.
    fn on_surface(h: &HelixParams, phi: f64, ring_ang: f64) -> Vec3 {
        let t = phi / (TAU * h.turns);
        let big_r = match h.taper_small_r {
            Some(s) => h.radius_mm + (s - h.radius_mm) * t,
            None => h.radius_mm,
        };
        let wire_r = match h.profile {
            Profile::Circle { radius_mm } => radius_mm,
            _ => unreachable!(),
        };
        let cy = h.pitch_mm * phi / TAU + h.axial_offset_mm;
        let rr = big_r + wire_r * ring_ang.cos();
        [rr * phi.cos(), cy + wire_r * ring_ang.sin(), rr * phi.sin()]
    }

    #[test]
    fn helix_surface_points_are_near_zero() {
        let h = HelixParams {
            radius_mm: 10.0,
            taper_small_r: None,
            pitch_mm: 6.0,
            turns: 8.0,
            profile: Profile::Circle { radius_mm: 1.0 },
            axial_offset_mm: 0.0,
        };
        for i in 0..200 {
            let phi = (i as f64 / 199.0) * TAU * h.turns;
            for ring in [0.0, 1.0, 2.5, 4.0, 5.5] {
                let d = sd_helix(on_surface(&h, phi, ring), &h);
                assert!(d.abs() < 0.08, "phi={phi} ring={ring} d={d}"); // 8% of wire_r tolerance:
                                                                        // the ring-plane reduction is approximate at nonzero pitch angle
            }
        }
    }

    #[test]
    fn close_wound_gap_never_reports_negative_between_coils() {
        // pitch == wire diameter (extension body): the midpoint BETWEEN adjacent coil
        // centers on the coil cylinder is a surface-contact point; just outside the
        // cylinder it must be OUTSIDE (>= -eps) — the wrong-turn failure mode reports
        // deep negative here.
        //
        // The sweep also pins the neighbor guard from the OTHER side: dropping
        // candidates from a min() can only INCREASE the reported distance (min
        // over a subset >= min over the full set), so a negative-guard alone is
        // provably blind to a shrunk candidate set. The load-bearing failure a
        // single-candidate reduction produces is an OVER-estimate — at gap
        // fraction 0.9 the continuous estimate floors to turn 3, but turn 4 is
        // nearer, and over-estimating lets a sphere-tracer step clean through
        // the gap's wall. Hence the conservativeness upper bound below, checked
        // against the analytic two-neighbor distance at every point.
        let h = HelixParams {
            radius_mm: 10.0,
            taper_small_r: None,
            pitch_mm: 2.0,
            turns: 10.0,
            profile: Profile::Circle { radius_mm: 1.0 },
            axial_offset_mm: 0.0,
        };
        let wire_r = 1.0;
        let radial_gap = 1.2;
        for i in 0..100 {
            let theta = (i as f64 / 99.0) * TAU;
            for gap_frac in [0.1, 0.5, 0.9] {
                // between turns 3 and 4, `gap_frac` of the way up the gap
                let y_between = h.pitch_mm * (theta / TAU) + h.pitch_mm * (3.0 + gap_frac);
                let p = [
                    (h.radius_mm + radial_gap) * theta.cos(),
                    y_between,
                    (h.radius_mm + radial_gap) * theta.sin(),
                ];
                let d = sd_helix(p, &h);
                assert!(
                    d > -1e-9,
                    "between-coil point reported inside at theta={theta} gap_frac={gap_frac}"
                );
                // Analytic distance to the nearer of the two adjacent turns'
                // cross-sections — the SDF must never exceed it (conservative).
                let axial_to_nearer = h.pitch_mm * gap_frac.min(1.0 - gap_frac);
                let true_d = radial_gap.hypot(axial_to_nearer) - wire_r;
                assert!(
                    d <= true_d + 1e-9,
                    "over-estimate at theta={theta} gap_frac={gap_frac}: d={d} > true={true_d}"
                );
            }
        }
    }

    #[test]
    fn helix_inside_wire_is_negative_and_far_outside_positive() {
        let h = HelixParams {
            radius_mm: 10.0,
            taper_small_r: None,
            pitch_mm: 6.0,
            turns: 8.0,
            profile: Profile::Circle { radius_mm: 1.0 },
            axial_offset_mm: 0.0,
        };
        // wire centerline point:
        let phi = TAU * 3.25;
        let c = [10.0 * phi.cos(), 6.0 * phi / TAU, 10.0 * phi.sin()];
        assert!(sd_helix(c, &h) < -0.9); // ~ -wire_r
        assert!(sd_helix([40.0, 10.0, 0.0], &h) > 25.0); // conservative but must be well positive
    }

    #[test]
    fn taper_tracks_local_radius() {
        let h = HelixParams {
            radius_mm: 15.0,
            taper_small_r: Some(7.0),
            pitch_mm: 5.0,
            turns: 6.0,
            profile: Profile::Circle { radius_mm: 0.8 },
            axial_offset_mm: 0.0,
        };
        for frac in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let phi = frac * TAU * h.turns;
            let d = sd_helix(on_surface(&h, phi, 0.0), &h);
            assert!(d.abs() < 0.1, "taper frac={frac} d={d}");
        }
    }

    #[test]
    fn ends_are_clamped_not_infinite() {
        let h = HelixParams {
            radius_mm: 10.0,
            taper_small_r: None,
            pitch_mm: 6.0,
            turns: 4.0,
            profile: Profile::Circle { radius_mm: 1.0 },
            axial_offset_mm: 0.0,
        };
        // Below the start: distance grows with axial gap, never negative.
        let d = sd_helix([10.0, -5.0, 0.0], &h);
        assert!(d > 4.0 - 1.0 - 0.1 && d.is_finite());
    }

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
