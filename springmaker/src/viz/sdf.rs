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

use std::f64::consts::{PI, TAU};

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
/// the exact nearest-point-on-helix problem is transcendental (no closed
/// form), so this evaluates a closed-form LOWER BOUND of the distance on
/// each of four candidate windings and takes the minimum. All symbols
/// below use the query's cylindrical frame: `radial`/`axial` its
/// coordinates, `theta` its azimuth, `phi` the wire parameter, `s =
/// pitch/TAU` and `g = dR/dphi` the axial and radial centerline slopes.
///
/// **Candidate windings (finding-2 fix: taper-aware anchor).** `theta`
/// determines `phi` modulo `TAU` (`phi_c = theta + TAU*k`); which winding
/// `k` is nearest is decided by the per-candidate squared centerline
/// distance `(radial - R(k))^2 + (axial - y(k))^2`, QUADRATIC in `k` when
/// the taper is linear. Its closed-form minimizer
/// `k* = (pitch^2*k_est + dR_turn*(radial - radius_mm - theta*g)) /
/// (dR_turn^2 + pitch^2)` (with `dR_turn` the signed radius change per
/// turn) anchors the window `floor(k*)-1 ..= floor(k*)+2`; it reduces to
/// the pitch-only estimate `k_est` exactly when `dR_turn = 0`
/// (`taper_with_zero_delta_reduces_to_untapered`). Anchoring at `k_est`
/// alone drifts by `~dR*(R_local - radial)/(dR^2 + pitch^2)` turns and can
/// miss the true winding entirely on a steep cone
/// (`taper_window_reaches_the_small_end_winding` pins the reviewer's 109%
/// over-estimate).
///
/// **R2 finding — does `k*` also bracket the Jordan-refined vertex?** `k*`
/// is the vertex of the coarse ring-plane objective `d^2(k) = a(k)^2 +
/// b(k)^2` (fixing `u = 0`), not of the Jordan-refined per-winding value
/// this function actually evaluates, `V(k) = min_u f_k(u)` with `f_k(u) =
/// (a(k) - g*u)^2 + (b(k) - s*u)^2 + c^2*u^2` (`c^2`, defined below in the
/// per-winding bound, `= 4*radial*R_min/PI^2`). `f_k(u)` is a JOINT
/// quadratic in `(k, u)`, and in general a joint quadratic's
/// profile-minimized vertex (`k**`, from `V(k)`) need not coincide with a
/// fixed-slice vertex (`k*`, from `V`'s `u = 0` cross-section). Solving
/// `f_k(u)`'s 2x2 stationarity system (`d f/dk = 0`, `d f/du = 0`) proves
/// they coincide here anyway: `a(k, u)` and `b(k, u)` depend on `(k, u)`
/// only through the combined phase `TAU*k + u` (because `dR_turn = TAU*g`
/// and `pitch = TAU*s` — the per-turn and per-radian centerline slopes are
/// the same rate measured two ways), so `d f/du = 0` reduces to
/// `c^2*u** = 0` at the joint stationary point — forcing `u** = 0` and
/// collapsing the joint vertex onto the exact `u = 0` slice `k*` already
/// comes from. Concretely, both vertices reduce to the identical closed
/// form `(g*A + s*B) / (TAU*(g^2 + s^2))` (`A, B` the `phi_c = theta`,
/// i.e. `k = 0`, values of `a(k), b(k)`): `c^2` shifts `V(k)`'s curvature
/// and minimum value but never its argmin location. This is an EXACT
/// identity for every geometry, not merely "close enough"
/// (`ring_plane_vertex_matches_jordan_refined_vertex` pins `k* == k**` to
/// `1e-6` against an independent numeric argmin of `V(k)` — built from the
/// raw `f_k(u)` formula and golden-section refined, never touching the
/// closed form — over 1500 randomized interior/high-radial/high-taper
/// geometries; reverting the anchor to the taper-blind `k_est` reproduces
/// a real divergence in that same test, confirming it has teeth). The
/// identity relies on `c^2` being CONSTANT in `k` (the part's global
/// `R_min`, not a per-winding radius) — a future per-winding tightening of
/// the Jordan bound would break the cancellation and require re-deriving
/// `k**` from the 2x2 system directly rather than reusing `k*`.
///
/// So the window, anchored at `floor(k*)`, is PROVEN to bracket `V(k)`'s
/// true vertex `k**` exactly; combined with the per-winding Jordan bound
/// making every evaluated winding conservative (below), the reported
/// minimum is conservative wherever the window covers the true winner. The
/// four-wide (not two-wide) span still matters for a separate reason:
/// `floor` rounding near a half-integer vertex can put the true winner one
/// slot off center, which the close-wound rounding case exercises directly
/// (`close_wound_gap_never_reports_negative_between_coils`).
///
/// The window anchor is clamped so the four candidates always sweep the
/// COVERING windings — those whose intersection with the real sweep
/// `[0, max_phi]` is non-empty: winding `k` covers `phi in [phi_c - PI,
/// phi_c + PI]`, so `k in [ceil((-PI - theta)/TAU),
/// floor((max_phi + PI - theta)/TAU)]`. Note this includes "virtual"
/// boundary windings whose reference `phi_c` lies OUTSIDE the sweep while
/// part of their half-turn neighborhood is real; skipping them made
/// beyond-the-end queries at far azimuths over-estimate (the nearest real
/// arc was in no candidate's frame). Candidates whose real sub-range is
/// empty are skipped.
///
/// **Per-winding lower bound (finding-1 fix).** The exact squared
/// centerline distance within winding `k`, parametrized by the offset `u =
/// phi - phi_c` from the same-azimuth reference, is
///
/// ```text
/// |p - C(phi_c + u)|^2
///   = (a - g*u)^2 + (b - s*u)^2 + 2*radial*R(phi)*(1 - cos u)
/// ```
///
/// with `a = radial - R(phi_c)`, `b = axial - y(phi_c)`. The previous
/// formulation evaluated only `u = 0` (the ring plane): that is the
/// distance to ONE azimuthal cross-section — a SUBSET of the swept solid —
/// so it can only OVER-estimate the true distance, which is the unsafe
/// direction for sphere tracing (reviewer's counterexample: `R=10`,
/// `pitch=6`, `wire_r=1`, `p=(10,3,0)` reported 2.000000 vs. true
/// 1.986414; a marcher can step through a coil gap's wall). The reviewer's
/// proposed constant rescale `d*cos(alpha)` with `tan(alpha) =
/// pitch/(TAU*R_min)` is the first-order version of the fix but is NOT a
/// true bound — measured violations: 5.4e-7 at the counterexample itself,
/// 6e-2 at exterior steep-pitch points, 4.4e-1 on the coil axis — so the
/// rigorous form is used instead:
///
/// * Jordan's chord inequality `1 - cos u >= 2*u^2/PI^2` (valid `|u| <=
///   PI`) and `R(phi) >= R_min` (the part's smallest coil radius) bound
///   the azimuth term below by `c^2*u^2` with `c^2 = 4*radial*R_min/PI^2`.
///   The remaining expression is QUADRATIC in `u`; its vertex `u0 =
///   (a*g + b*s)/(g^2 + s^2 + c^2)`, clamped to both the Jordan range
///   `[-PI, PI]` and the winding's real sub-range, gives the constrained
///   minimum. Evaluating the profile at the shifted in-plane offsets
///   `(a - g*u0, b - s*u0)` (plus the `c^2*u0^2` azimuth term for circular
///   profiles) therefore never exceeds the true centerline distance minus
///   the wire radius — the exact tube distance — and the rendered solid is
///   a subset of the tube, so the bound is conservative for it a fortiori.
/// * For a virtual boundary winding the real sub-range excludes `u = 0`,
///   and the azimuth term is instead bounded by its EXACT value at the
///   sub-range endpoint nearest zero (`2*radial*R_min*(1 - cos u_nz)`,
///   monotone in `|u|` on `[-PI, PI]`), added to the constrained planar
///   minimum. This keeps end-region queries tight (exact at the terminal
///   cross-sections) where the Jordan bound alone is loose mid-winding.
///
/// **Terminal caps (finding-3 fix).** For `turns < 1` a query azimuth can
/// lie outside the swept arc for EVERY winding; the old raw-`phi` clamp
/// then snapped a candidate to the boundary cross-section at the WRONG
/// azimuth — a phantom: `turns=0.5`, `p=(0,3,-10)` reported -1.0 where the
/// true exterior distance is 13.14. The [`sd_torus_arc`] endpoint trick
/// closes it exactly: the distance to each terminal centerline point,
/// minus the profile's cap radius, joins the candidate minimum
/// unconditionally. These are plain min-participants over real surface
/// features, so they never break the bound
/// (`sub_turn_end_reports_exterior_distance_not_phantom`).
///
/// **Conservativeness contract.** For circular profiles the reported value
/// never exceeds the true tube distance (`helix_never_over_estimates_the_
/// true_wire_distance` checks grids on the brief and steep-pitch
/// geometries against an independent 1-D minimization; a 2600-point
/// randomized sweep across three geometries measured zero violations —
/// see the task report). Under-estimation fattens the level set slightly
/// instead: measured `max |d|` on exact surface points is 7.4e-3 (0.7% of
/// the wire radius) for the brief geometry, within the 8% test tolerance;
/// at a 46.7-degree pitch angle the worst near-surface under-shoot
/// measured -0.081. Inside the wire the value stays negative (sign
/// structure exact); depth may exceed the true depth, which is also the
/// safe direction. `Rectangle` profiles keep the ring-plane evaluation at
/// the shifted offset WITHOUT a rigorous conservativeness argument (the
/// box is not rotation-invariant about the helix tangent); they are not
/// rendered by any current scene — revisit alongside the rectangular
/// family's shaded view.
#[allow(dead_code)] // consumed by Task 3 (SdfScene part construction) and Task 5 (WGSL mirror)
pub(crate) fn sd_helix(p: Vec3, h: &HelixParams) -> f64 {
    let axial = p[1] - h.axial_offset_mm;
    let radial = p[0].hypot(p[2]);
    let theta = p[2].atan2(p[0]).rem_euclid(TAU);
    let max_phi = TAU * h.turns;
    let s = h.pitch_mm / TAU;
    let small_r = h.taper_small_r.unwrap_or(h.radius_mm);
    let g = (small_r - h.radius_mm) / max_phi;
    let r_min = h.radius_mm.min(small_r);

    // Finding-2 / R2 anchor: closed-form quadratic minimizer over the turn
    // index — proven in the doc above to equal BOTH the ring-plane vertex
    // k* and the Jordan-refined joint vertex k**.
    let k_star = candidate_window_vertex(radial, axial, theta, h.radius_mm, h.pitch_mm, g);

    // Covering windings and the window anchor clamp (see doc).
    let k_cov_lo = ((-PI - theta) / TAU).ceil();
    let k_cov_hi = ((max_phi + PI - theta) / TAU).floor();
    let anchor = k_star
        .floor()
        .clamp(k_cov_lo, (k_cov_hi - 2.0).max(k_cov_lo));

    let chord_sq = 4.0 * radial * r_min / (PI * PI);
    let planar_sq = g * g + s * s;

    let mut best = f64::INFINITY;
    for delta in -1..=2 {
        let phi_c = theta + TAU * (anchor + f64::from(delta));
        let u_lo = (-PI).max(-phi_c);
        let u_hi = PI.min(max_phi - phi_c);
        if u_lo > u_hi {
            continue;
        }
        let a = radial - (h.radius_mm + g * phi_c);
        let b = axial - s * phi_c;
        let d = if u_lo <= 0.0 && 0.0 <= u_hi {
            // Same-azimuth winding: coupled Jordan-chord quadratic.
            let denom = planar_sq + chord_sq;
            let u = if denom > 0.0 {
                ((a * g + b * s) / denom).clamp(u_lo, u_hi)
            } else {
                0.0
            };
            winding_distance(a - g * u, b - s * u, chord_sq * u * u, h.profile)
        } else {
            // Virtual boundary winding: exact azimuth chord at the real
            // sub-range endpoint nearest zero + constrained planar minimum.
            let u_nz = if u_lo > 0.0 { u_lo } else { u_hi };
            let azimuth_sq = 2.0 * radial * r_min * (1.0 - u_nz.cos());
            let u = if planar_sq > 0.0 {
                ((a * g + b * s) / planar_sq).clamp(u_lo, u_hi)
            } else {
                u_nz
            };
            winding_distance(a - g * u, b - s * u, azimuth_sq, h.profile)
        };
        best = best.min(d);
    }

    // Finding-3 terminal caps: exact distance to both end centerline points.
    for phi_end in [0.0, max_phi] {
        let coil_r = h.radius_mm + g * phi_end;
        let end = [
            coil_r * phi_end.cos(),
            s * phi_end + h.axial_offset_mm,
            coil_r * phi_end.sin(),
        ];
        best = best.min(vlen(vsub(p, end)) - profile_cap_radius(h.profile));
    }
    best
}

/// Closed-form vertex used to anchor [`sd_helix`]'s candidate window —
/// proven in that function's doc to be simultaneously `k*` (the coarse
/// ring-plane objective's vertex, `u = 0` fixed) and `k**` (the true vertex
/// of the Jordan-refined joint objective `V(k) = min_u f_k(u)`), so one
/// closed form serves both roles. `radial`/`axial`/`theta` are the query's
/// cylindrical coordinates, `g = dR/dphi` the signed radial centerline
/// slope; `radius_mm`/`pitch_mm` are the part's large-end radius and pitch.
fn candidate_window_vertex(
    radial: f64,
    axial: f64,
    theta: f64,
    radius_mm: f64,
    pitch_mm: f64,
    g: f64,
) -> f64 {
    let s = pitch_mm / TAU;
    let k_est = (axial - s * theta) / pitch_mm;
    let dr_turn = g * TAU;
    (pitch_mm * pitch_mm * k_est + dr_turn * (radial - radius_mm - theta * g))
        / (dr_turn * dr_turn + pitch_mm * pitch_mm)
}

/// Per-winding candidate distance at the shifted in-plane offsets, with the
/// squared azimuth (out-of-plane) term folded in for circular profiles —
/// see [`sd_helix`]'s derivation. Rectangles drop the azimuth term (any
/// dropped non-negative term keeps the value a lower bound of the
/// centerline distance, but the box reduction itself carries no rigorous
/// conservativeness argument — documented on [`sd_helix`]).
fn winding_distance(d_radial: f64, d_axial: f64, azimuth_sq: f64, profile: Profile) -> f64 {
    match profile {
        Profile::Circle { radius_mm } => {
            (d_radial * d_radial + d_axial * d_axial + azimuth_sq).sqrt() - radius_mm
        }
        rectangle => sd_profile_2d(d_radial, d_axial, rectangle),
    }
}

/// Radius of the sphere used for the terminal-cap candidates: the wire
/// radius for circles, the circumscribed radius for rectangles (the
/// enclosing sphere is nearer than the terminal cross-section, keeping the
/// cap candidate on the conservative side).
fn profile_cap_radius(profile: Profile) -> f64 {
    match profile {
        Profile::Circle { radius_mm } => radius_mm,
        Profile::Rectangle {
            half_w_mm,
            half_h_mm,
        } => half_w_mm.hypot(half_h_mm),
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

    /// Independent ground truth for the conservativeness tests: 1-D numeric
    /// minimization of the point-to-centerline distance over the full wire
    /// parameter range (dense sample, then golden-section refinement), minus
    /// the wire radius — the exact signed distance to the wire tube. The
    /// rendered solid (ring-plane discs swept along the centerline) is a
    /// subset of the tube, so `sd_helix <= tube distance` is the strictly
    /// stronger conservativeness statement.
    ///
    /// `pre_scan_n` is the dense-sample count used only to locate the right
    /// winding's basin before golden-section refinement takes over; the
    /// refinement supplies the actual precision, so a coarser scan (used by
    /// the bulk randomized sweep, for speed) is exact as long as it is fine
    /// enough to not skip past a whole winding (basins are `TAU` wide).
    fn true_helix_distance(p: Vec3, h: &HelixParams, pre_scan_n: u32) -> f64 {
        let wire_r = match h.profile {
            Profile::Circle { radius_mm } => radius_mm,
            _ => unreachable!(),
        };
        let max_phi = TAU * h.turns;
        let dist_sq = |phi: f64| -> f64 {
            let t = phi / max_phi;
            let big_r = match h.taper_small_r {
                Some(s) => h.radius_mm + (s - h.radius_mm) * t,
                None => h.radius_mm,
            };
            let c = [
                big_r * phi.cos(),
                h.pitch_mm * phi / TAU + h.axial_offset_mm,
                big_r * phi.sin(),
            ];
            let d = vsub(p, c);
            vdot(d, d)
        };
        let n = pre_scan_n;
        let mut best_i = 0_u32;
        let mut best = f64::INFINITY;
        for i in 0..=n {
            let d = dist_sq(max_phi * f64::from(i) / f64::from(n));
            if d < best {
                best = d;
                best_i = i;
            }
        }
        let mut lo = max_phi * f64::from(best_i.saturating_sub(1)) / f64::from(n);
        let mut hi = max_phi * f64::from((best_i + 1).min(n)) / f64::from(n);
        let inv_gold = (5.0_f64.sqrt() - 1.0) / 2.0;
        let mut mid_lo = hi - inv_gold * (hi - lo);
        let mut mid_hi = lo + inv_gold * (hi - lo);
        let (mut f_lo, mut f_hi) = (dist_sq(mid_lo), dist_sq(mid_hi));
        for _ in 0..120 {
            if f_lo < f_hi {
                hi = mid_hi;
                mid_hi = mid_lo;
                f_hi = f_lo;
                mid_lo = hi - inv_gold * (hi - lo);
                f_lo = dist_sq(mid_lo);
            } else {
                lo = mid_lo;
                mid_lo = mid_hi;
                f_lo = f_hi;
                mid_hi = lo + inv_gold * (hi - lo);
                f_hi = dist_sq(mid_hi);
            }
        }
        f_lo.min(f_hi).sqrt() - wire_r
    }

    #[test]
    fn helix_never_over_estimates_the_true_wire_distance() {
        // Review finding 1 (critical): a ring-plane candidate measures the
        // distance to a SUBSET of the swept solid (one azimuthal cross-
        // section), so it can only OVER-estimate the true distance — the
        // unsafe direction for sphere tracing. Reviewer's counterexample:
        // R=10, pitch=6, wire_r=1, p=(10, 3, 0) mid-gap on the coil cylinder
        // reported 2.000000 where the true tube distance is 1.986414.
        let brief = HelixParams {
            radius_mm: 10.0,
            taper_small_r: None,
            pitch_mm: 6.0,
            turns: 8.0,
            profile: Profile::Circle { radius_mm: 1.0 },
            axial_offset_mm: 0.0,
        };
        // (radial, y, azimuth) probes: the counterexample, the coil axis,
        // on-cylinder mid-gaps at other azimuths, bore and exterior points,
        // and points below the start.
        let brief_grid: [(f64, f64, f64); 10] = [
            (10.0, 3.0, 0.0), // the reviewer's counterexample
            (0.0, 3.0, 0.0),  // coil axis, mid-gap height
            (10.0, 27.0, 5.5),
            (12.0, 3.0, 0.0),
            (11.5, 15.0, 1.0),
            (13.0, 21.0, 2.5),
            (8.0, 9.0, 4.0),
            (14.0, 40.0, 0.7),
            (10.0, -4.0, 0.0),
            (2.9, -4.6, 5.2),
        ];
        // Steep-pitch case from the review brief: pitch 20, mean dia 6.
        let steep = HelixParams {
            radius_mm: 3.0,
            taper_small_r: None,
            pitch_mm: 20.0,
            turns: 5.0,
            profile: Profile::Circle { radius_mm: 0.5 },
            axial_offset_mm: 0.0,
        };
        let steep_grid: [(f64, f64, f64); 8] = [
            (3.0, 5.0, 0.0),  // on-cylinder mid-gap
            (0.0, 30.0, 0.0), // axis
            (4.0, 10.0, 0.0),
            (4.5, 5.0, 0.0),
            (5.0, 30.0, 1.0),
            (6.0, 50.0, 2.5),
            (4.2, 70.0, 4.0),
            (1.5, 45.0, 2.0),
        ];
        for (h, grid) in [(&brief, &brief_grid[..]), (&steep, &steep_grid[..])] {
            for &(radial, y, azimuth) in grid {
                let p = [radial * azimuth.cos(), y, radial * azimuth.sin()];
                let d = sd_helix(p, h);
                let true_d = true_helix_distance(p, h, 200_000);
                assert!(
                    d <= true_d + 1e-9,
                    "over-estimate at radial={radial} y={y} azimuth={azimuth} \
                     (pitch={}): d={d} > true={true_d}",
                    h.pitch_mm
                );
            }
        }
    }

    #[test]
    fn taper_window_reaches_the_small_end_winding() {
        // Review finding 2: with a linear taper the per-candidate squared
        // distance is quadratic in the turn index, and its minimizer drifts
        // from the pitch-only estimate by ~dR*(R_local - radial)/(dR^2 +
        // pitch^2) turns. Reviewer's conical counterexample: large end 40,
        // small end 10, 5 turns, wire 1, pitch 1.2, query on the axis at
        // y=1.2 — the pitch-only window stops at turn 3 (d=21.13, a 109%
        // over-estimate); the true nearest winding is the small end
        // (turn 5, ring-plane d=10.0923).
        let h = HelixParams {
            radius_mm: 40.0,
            taper_small_r: Some(10.0),
            pitch_mm: 1.2,
            turns: 5.0,
            profile: Profile::Circle { radius_mm: 1.0 },
            axial_offset_mm: 0.0,
        };
        let p = [0.0, 1.2, 0.0];
        // Distance to the turn-5 (small-end) ring-plane candidate: coil
        // radius 10 at height 6.0, query on the axis at height 1.2.
        let turn5 = 10.0_f64.hypot(1.2 - 6.0) - 1.0;
        let d = sd_helix(p, &h);
        assert!(
            d <= turn5 + 1e-9,
            "window missed the small-end winding: d={d} > turn-5 candidate={turn5}"
        );
    }

    #[test]
    fn taper_with_zero_delta_reduces_to_untapered() {
        // The finding-2 anchor k* must reduce algebraically to k_est when
        // the radius change per turn is zero; observable as bit-identical
        // distances between `taper_small_r: Some(radius_mm)` and `None`.
        let tapered = HelixParams {
            radius_mm: 10.0,
            taper_small_r: Some(10.0),
            pitch_mm: 6.0,
            turns: 8.0,
            profile: Profile::Circle { radius_mm: 1.0 },
            axial_offset_mm: 0.0,
        };
        let plain = HelixParams {
            taper_small_r: None,
            ..tapered
        };
        for i in 0..50 {
            let azimuth = f64::from(i) * 0.37;
            let p = [
                11.0 * azimuth.cos(),
                3.0 + f64::from(i) * 0.9,
                11.0 * azimuth.sin(),
            ];
            let (a, b) = (sd_helix(p, &tapered), sd_helix(p, &plain));
            assert!(
                (a - b).abs() < 1e-12,
                "zero-delta taper diverged at i={i}: {a} vs {b}"
            );
        }
    }

    /// Deterministic xorshift64 PRNG for reproducible randomized test
    /// sweeps — a pure-`f64`-math file pulls in no RNG crate dependency for
    /// test-only sampling. Returns values in `[0, 1)`.
    struct TestRng(u64);
    impl TestRng {
        fn next_unit(&mut self) -> f64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
        fn range(&mut self, lo: f64, hi: f64) -> f64 {
            lo + self.next_unit() * (hi - lo)
        }
    }

    #[test]
    fn ring_plane_vertex_matches_jordan_refined_vertex() {
        // R2 review: `k*` (`sd_helix`'s window anchor) is the vertex of the
        // coarse ring-plane objective `d^2(k)` (fixing `u = 0`) — is it also
        // the vertex `k**` of the Jordan-refined joint objective `V(k) =
        // min_u f_k(u)` the rest of the function actually evaluates? The doc
        // above proves `k* == k**` algebraically (the `c^2*u^2` azimuth term
        // shifts `V(k)`'s curvature but never its argmin). This test checks
        // that identity against a GENUINELY INDEPENDENT ground truth: `V(k)`
        // built straight from the raw joint quadratic `f_k(u)` and minimized
        // over `u` in closed form (a *different*, standalone expression from
        // `candidate_window_vertex`'s), then the resulting 1-D profile's own
        // vertex located by golden-section search — never calling
        // `candidate_window_vertex` for anything but the value under test.
        // No dense pre-scan is needed first (unlike `true_helix_distance`):
        // `V(k)` is a single convex parabola in `k` by construction (see the
        // doc's derivation), so it has exactly one basin and golden section
        // finds it directly from a wide bracket.
        let mut rng = TestRng(0x5eed_1234_dead_beef);
        for _ in 0..1500 {
            // Interior / high-radial (near R) / high-taper combinations —
            // the regime the R2 review flagged as the divergence risk.
            let radius_mm = rng.range(5.0, 50.0);
            let taper_small_r = radius_mm * rng.range(0.05, 1.0); // up to 95% taper
            let pitch_mm = rng.range(0.5, 15.0);
            let turns = rng.range(1.0, 9.0);
            let r_min = radius_mm.min(taper_small_r);
            let radial = r_min * rng.range(0.3, 1.7); // near-cylinder, off-axis
            let axial = rng.range(-1.0, 1.0) * pitch_mm * turns;
            let theta = rng.range(0.0, TAU);

            let max_phi = TAU * turns;
            let s = pitch_mm / TAU;
            let g = (taper_small_r - radius_mm) / max_phi;
            let c_sq = 4.0 * radial * r_min / (PI * PI);

            let k_star = candidate_window_vertex(radial, axial, theta, radius_mm, pitch_mm, g);

            // Independent V(k): raw f_k(u) minimized over u in closed form.
            let v_of_k = |k: f64| -> f64 {
                let phi_c = theta + TAU * k;
                let a = radial - (radius_mm + g * phi_c);
                let b = axial - s * phi_c;
                let denom = g * g + s * s + c_sq;
                let u = if denom > 0.0 {
                    (a * g + b * s) / denom
                } else {
                    0.0
                };
                (a - g * u).powi(2) + (b - s * u).powi(2) + c_sq * u * u
            };

            // Golden-section search for V(k)'s vertex over a bracket wide
            // enough to contain it regardless of where k_star lands.
            let (mut lo, mut hi) = (k_star - 5.0, k_star + 5.0);
            let inv_gold = (5.0_f64.sqrt() - 1.0) / 2.0;
            let mut mid_lo = hi - inv_gold * (hi - lo);
            let mut mid_hi = lo + inv_gold * (hi - lo);
            let (mut f_lo, mut f_hi) = (v_of_k(mid_lo), v_of_k(mid_hi));
            for _ in 0..200 {
                if f_lo < f_hi {
                    hi = mid_hi;
                    mid_hi = mid_lo;
                    f_hi = f_lo;
                    mid_lo = hi - inv_gold * (hi - lo);
                    f_lo = v_of_k(mid_lo);
                } else {
                    lo = mid_lo;
                    mid_lo = mid_hi;
                    f_lo = f_hi;
                    mid_hi = lo + inv_gold * (hi - lo);
                    f_hi = v_of_k(mid_hi);
                }
            }
            let k_star_star = if f_lo < f_hi { mid_lo } else { mid_hi };

            assert!(
                (k_star - k_star_star).abs() < 1e-6,
                "k* and k** diverged: k*={k_star} k**~{k_star_star} \
                 (radius={radius_mm} taper_small_r={taper_small_r} pitch={pitch_mm} \
                 turns={turns} radial={radial} axial={axial} theta={theta})"
            );
        }
    }

    #[test]
    fn broadened_conservativeness_sweep_interior_high_radial_high_taper() {
        // Broadens the finding-1 conservativeness sweep with 2000 points
        // deliberately targeted at the R2-flagged risk regime: query points
        // near the coil cylinder ("high-radial"), on steeply tapered cones
        // ("high-taper"), sampled along the actual swept body ("interior")
        // rather than only at a handful of hand-picked grid points.
        let mut rng = TestRng(0xc0ff_ee12_3456_789a);
        for _ in 0..2000 {
            let radius_mm = rng.range(5.0, 50.0);
            let taper_small_r = radius_mm * rng.range(0.05, 1.0);
            let pitch_mm = rng.range(0.5, 12.0);
            let turns = rng.range(1.0, 8.0);
            // Wire radius kept plausible relative to pitch and coil radius
            // (form-reachable geometry, not degenerate self-overlap).
            let r_min = radius_mm.min(taper_small_r);
            let wire_r = rng.range(0.1, (pitch_mm * 0.45).min(r_min * 0.4).max(0.1001));
            let h = HelixParams {
                radius_mm,
                taper_small_r: Some(taper_small_r),
                pitch_mm,
                turns,
                profile: Profile::Circle { radius_mm: wire_r },
                axial_offset_mm: 0.0,
            };

            // Query near a random point along the actual sweep: pick a wire
            // parameter, then jitter radially (biased near the local coil
            // radius — high-radial) and axially (spanning inside the wire,
            // in the gap, and a bit beyond).
            let max_phi = TAU * turns;
            let phi_query = rng.range(0.0, max_phi);
            let t = phi_query / max_phi;
            let local_r = radius_mm + (taper_small_r - radius_mm) * t;
            let local_y = pitch_mm * phi_query / TAU;
            let radial = (local_r * rng.range(0.5, 1.5)).max(0.0);
            let theta = phi_query.rem_euclid(TAU) + rng.range(-0.5, 0.5);
            let axial = local_y + rng.range(-2.0, 2.0) * pitch_mm;

            let p = [radial * theta.cos(), axial, radial * theta.sin()];
            let d = sd_helix(p, &h);
            let true_d = true_helix_distance(p, &h, 4000);
            assert!(
                d <= true_d + 1e-9,
                "over-estimate: d={d} > true={true_d} (radius={radius_mm} \
                 taper_small_r={taper_small_r} pitch={pitch_mm} turns={turns} \
                 p={p:?})"
            );
        }
    }

    #[test]
    fn sub_turn_end_reports_exterior_distance_not_phantom() {
        // Review finding 3: for turns < 1 the residual raw-phi clamp snaps a
        // candidate to the terminal cross-section at the WRONG azimuth.
        // Reviewer's counterexample: turns=0.5 (sweep 0..pi), query at
        // azimuth 3*pi/2, p=(0, 3, -10): the broken clamp reports -1.0
        // (a phantom "inside"), the true exterior distance is ~13.14 (the
        // terminal centerline point is (-10, 3, 0)).
        let h = HelixParams {
            radius_mm: 10.0,
            taper_small_r: None,
            pitch_mm: 6.0,
            turns: 0.5,
            profile: Profile::Circle { radius_mm: 1.0 },
            axial_offset_mm: 0.0,
        };
        let p = [0.0, 3.0, -10.0];
        let d = sd_helix(p, &h);
        assert!(d > 12.0, "sub-turn phantom: d={d}, expected ~13.14");
        assert!(
            d <= true_helix_distance(p, &h, 200_000) + 1e-9,
            "sub-turn end over-estimates: d={d}"
        );
        // Surface sweep sanity for the sub-turn body itself.
        for i in 0..200 {
            let phi = (f64::from(i) / 199.0) * TAU * h.turns;
            for ring in [0.0, 1.0, 2.5, 4.0, 5.5] {
                let d = sd_helix(on_surface(&h, phi, ring), &h);
                assert!(
                    d.abs() < 0.08,
                    "sub-turn surface phi={phi} ring={ring} d={d}"
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
