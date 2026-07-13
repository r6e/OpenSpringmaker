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

use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// Maximum number of [`SdfPart`]s one packed scene may carry — the shared
/// budget [`scene_uniforms`]'s buffer sizes itself around (Task 5
/// substitutes this literal into the WGSL source, so both sides read the
/// identical bound from this one constant).
///
/// **Why 48, not the original design's 24.** Every helical body
/// reconstructs to THREE `Helix` segments (dead/active/dead — see
/// [`helical_body_parts`]'s doc), discovered after the original budget was
/// set assuming one segment per body. The single-spring families top out
/// low regardless (compression/conical: 3 parts + 2 cuts; extension/
/// torsion: 3 body segments + 2 hook/leg parts = 5) — the assembly family
/// is the only one that scales with a user-controlled count, and NO cap on
/// the number of assembly members exists anywhere in the app
/// (`Message::AsmMemberAdd` in `app.rs` pushes unconditionally; `AsmFormState`
/// and `AssemblyInputs.members` are plain `Vec`s with no length check
/// anywhere in `form.rs`/`solve_assembly`). A finite `MAX_PARTS` is
/// therefore a genuine, unavoidable representability boundary, not a value
/// that can be sized to "fit the real member cap" — there isn't one — the
/// same shape as `MAX_RENDER_TURNS` capping the wireframe path today. 48 =
/// 16 members × 3 segments each, comfortably above a naive 8-member
/// assumption; the buffer this sizes
/// (`4 + MAX_PARTS·FLOATS_PER_PART + MAX_CUTS·FLOATS_PER_CUT` floats, here
/// 804, ~3.2KB) is cheap enough to be generous with. Past it,
/// [`scene_uniforms`] returns `None` and the caller falls back to the
/// wireframe RENDER (the same orbitable polyline scene the non-shaded path
/// always shows — proven by probe: an over-budget but otherwise valid
/// design still has a representable wireframe scene, so it renders that,
/// NOT the degenerate placeholder text) — never truncation.
pub(crate) const MAX_PARTS: usize = 48;
/// Maximum number of [`GroundPlane`] cuts one packed scene may carry. Every
/// current family emits at most 2 (one ground-flattened end each side);
/// double that for headroom.
pub(crate) const MAX_CUTS: usize = 4;
/// Sphere-march iteration cap — Task 5's WGSL fragment shader loop bound.
pub(crate) const MARCH_MAX_STEPS: u32 = 160;
/// Sphere-march step-size safety factor (steps `SAFETY × |sdf|`, not the
/// full reported distance, guarding against the [`sd_helix`]/[`cut_plane`]
/// conservative-but-inexact regions overshooting past a thin feature).
pub(crate) const MARCH_SAFETY: f32 = 0.8;
/// Sphere-march surface epsilon, mm-scale.
pub(crate) const MARCH_EPS: f32 = 1e-3;
/// Fixed per-part float stride in the packed uniform buffer — see
/// [`scene_uniforms`]'s doc for the exact per-`SdfPart`-kind layout within
/// one `FLOATS_PER_PART`-wide slot.
pub(crate) const FLOATS_PER_PART: usize = 16;
/// Fixed per-cut float stride — see [`scene_uniforms`]'s doc.
pub(crate) const FLOATS_PER_CUT: usize = 8;
/// Float offset of the cut-slot region within [`scene_uniforms`]'s packed
/// buffer: the 4-float header plus every part slot (simplifier F3 — hoisted
/// so `scene_uniforms`/`unpack_scene`/the mixed-scene test fixture below
/// all read the identical value instead of re-deriving it).
pub(crate) const CUTS_BASE_FLOATS: usize = 4 + MAX_PARTS * FLOATS_PER_PART;
/// Total float length of [`scene_uniforms`]'s packed buffer — the 4-float
/// header, every part slot, then every cut slot (simplifier F3 — hoisted and
/// aliased at every spelling: `scene_uniforms`, `unpack_scene`,
/// `shader3d::SCENE_STORAGE_FLOATS`, and the test fixtures that build a
/// buffer of this exact size).
pub(crate) const SCENE_UNIFORM_FLOATS: usize = CUTS_BASE_FLOATS + MAX_CUTS * FLOATS_PER_CUT;
/// Sentinel packed into a Helix slot's `taper_small_r` float to mean
/// `None` (no taper) — see [`scene_uniforms`]'s doc for the decode
/// contract. **Negative, not `NaN`.** `taper_small_r` is a physical
/// radius (always `>= 0.0` when `Some`), so any negative value is
/// unambiguously "absent"; unpacking it is then the ORDINARY comparison
/// `< 0.0`, not a NaN-testing predicate. This matters because WGSL (the
/// WebGPU Shading Language Task 5's fragment shader is written in) permits
/// implementations to compile under a finite-math-only assumption — no
/// NaN exists — under which `isNan()` and NaN-involving comparisons may
/// legally fold to a constant `false` at compile time
/// (<https://www.w3.org/TR/WGSL/#floating-point-evaluation>). A `NaN`
/// sentinel would therefore risk silently misdecoding every tapered vs.
/// untapered helix on a conforming implementation that takes that
/// license; an ordinary negative-vs-nonnegative comparison carries no such
/// risk in either Rust or WGSL.
const NO_TAPER_SENTINEL: f32 = -1.0;

/// A point or vector in millimetres, in whatever local frame the caller
/// established (world frame for scene composition; a part's own local
/// frame for primitive evaluation).
type Vec3 = [f64; 3];

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
pub(crate) enum Profile {
    Circle {
        radius_mm: f64,
    },
    #[allow(dead_code)]
    // consumed by the rectangular family's shaded view (not yet built) and Task 5 (WGSL mirror)
    Rectangle {
        half_w_mm: f64,
        half_h_mm: f64,
    },
}

/// Forward-ready per-part material description (design doc §Decisions 2):
/// a future material-DB swaps values in without touching the contract.
///
/// **Color convention (review F1-extension fix).** `base_color` here (and in
/// [`steel`]/the member hue table) is authored in ordinary sRGB, the same
/// display-color convention every other color in this app uses (`crate::
/// app::Palette`'s tokens included) — these were eyeballed as on-screen
/// colors, not measured radiometric values. [`pack_appearance`] linearizes
/// it at PACK TIME, right before it reaches the shaded shader's per-part
/// slot, for the same reason `viz::bg_rgba` linearizes the background: the
/// fragment shader writes to an sRGB-format target, so every color the
/// shader consumes for its (linear-space) shading math must already be
/// linear. `metallic`/`roughness` are plain material scalars, not colors —
/// they pack and unpack unchanged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Appearance {
    pub base_color: [f32; 3],
    pub metallic: f32,
    pub roughness: f32,
}

/// Default spring-steel look shared by every family's primary wire.
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
/// shaded view).
const MEMBER_HUE_SHIFTS: [[f32; 3]; 4] = [
    [0.0, 0.0, 0.0],
    [0.05, -0.03, -0.05],
    [-0.05, 0.03, 0.05],
    [0.08, 0.04, -0.06],
];

/// Documented length of [`MEMBER_HUE_SHIFTS`] — exported so callers (and
/// this module's own property test) read the real table length rather
/// than a copied literal.
const MEMBER_HUE_TABLE_LEN: usize = MEMBER_HUE_SHIFTS.len();

/// Steel tinted by `index`'s hue shift, wrapping every
/// [`MEMBER_HUE_TABLE_LEN`] members.
fn member_appearance(index: usize) -> Appearance {
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
fn sd_capsule(p: Vec3, a: Vec3, b: Vec3, radius_mm: f64) -> f64 {
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
fn sd_torus_arc(p_local: Vec3, major_r: f64, minor_r: f64, sweep: f64) -> f64 {
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
fn sd_profile_2d(d_radial: f64, d_axial: f64, profile: Profile) -> f64 {
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
/// `phase_rad` sets the starting azimuth: wire parameter `phi` sits at
/// world azimuth `phi + phase_rad`, i.e. the whole helix is the
/// `phase_rad = 0` helix rigidly rotated about Y
/// (`helix_phase_rotates_the_whole_helix_rigidly`) — this is what lets
/// [`helical_body_parts`]'s segments stay azimuthally CONTINUOUS across
/// fractional dead/active coil splits (review F2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HelixParams {
    pub radius_mm: f64,
    pub taper_small_r: Option<f64>,
    pub pitch_mm: f64,
    pub turns: f64,
    pub profile: Profile,
    pub axial_offset_mm: f64,
    pub phase_rad: f64,
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
///
/// **Phase (review-F2 fix).** `phase_rad` rotates the whole helix rigidly
/// about Y: wire parameter `phi` sits at world azimuth `phi + phase_rad`,
/// so `phi ≡ theta_world - phase_rad (mod TAU)` and the reduction's
/// candidates are `phi_c = (theta_world - phase_rad) + TAU*k`. That is ONE
/// substitution — mapping the query azimuth into the helix's own phase
/// frame up front — after which every downstream expression (the window
/// vertex `k*`, the covering clamp, the per-winding bound) is unchanged and
/// phase-consistent, because they all consume `theta` only through `phi_c`.
/// The terminal caps use the true world azimuth of each end
/// (`phi_end + phase_rad`). For `phase_rad = 0` the substitution is the
/// identity (bit-for-bit: `rem_euclid` of an unshifted angle), so every
/// phase-0 property below — conservativeness sweeps included — carries over
/// unchanged; `helix_phase_rotates_the_whole_helix_rigidly` pins the
/// general case by rotation equivariance.
///
/// **Precondition: `h.pitch_mm > 0.0` (review finding 6, input-domain F-C).**
/// [`candidate_window_vertex`]'s `k_est = (axial - s*theta) / pitch_mm`
/// divides by `pitch_mm` — at exactly `0.0` this is `NaN`, and EVERY
/// ring-plane candidate this fn computes below inherits that `NaN` (`best =
/// best.min(d)` with a `NaN` `d` — Rust's `f64::min` silently keeps `best`
/// unchanged rather than propagating the `NaN`, per IEEE-754 min semantics
/// — so the candidates are effectively DROPPED, not merely wrong), leaving
/// only the two terminal caps to report distance. The probed brief geometry
/// then over-reports by ~20mm INSIDE material — a conservativeness
/// violation, not just an under-shoot. No current family builder can
/// actually construct a zero-pitch `HelixParams` (`compression_sdf`/
/// `conical_sdf` both reject `pitch <= 0.0` before building one; audited),
/// but this function does not itself defend against the precondition —
/// callers must.
fn sd_helix(p: Vec3, h: &HelixParams) -> f64 {
    let axial = p[1] - h.axial_offset_mm;
    let radial = p[0].hypot(p[2]);
    let theta = (p[2].atan2(p[0]) - h.phase_rad).rem_euclid(TAU);
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

    // Finding-3 terminal caps: exact distance to both end centerline points
    // (at their TRUE world azimuth phi_end + phase_rad).
    for phi_end in [0.0, max_phi] {
        let coil_r = h.radius_mm + g * phi_end;
        let az = phi_end + h.phase_rad;
        let end = [
            coil_r * az.cos(),
            s * phi_end + h.axial_offset_mm,
            coil_r * az.sin(),
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
fn cut_plane(d: f64, p: Vec3, plane_point: Vec3, plane_normal: Vec3) -> f64 {
    d.max(vdot(vsub(p, plane_point), plane_normal))
}

/// One renderable primitive in a composed scene. `Helix` covers every
/// family's coil body (possibly tapered); `TorusArc` covers extension hooks
/// (a hook loop lies in a fixed vertical azimuthal plane, oriented into that
/// plane by `(y_rotation, tilt)` — see [`part_distance`]'s doc for the exact
/// frame convention); `Capsule` covers torsion legs (straight tangential
/// segments).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SdfPart {
    Helix(HelixParams),
    TorusArc {
        center: Vec3,
        y_rotation: f64,
        tilt: f64,
        major_r: f64,
        minor_r: f64,
        sweep: f64,
    },
    Capsule {
        a: Vec3,
        b: Vec3,
        radius_mm: f64,
    },
}

/// A half-space cut applied to the WHOLE scene (every part, not just one) —
/// see [`sdf_eval`]. `point`/`normal` feed [`cut_plane`] directly.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GroundPlane {
    pub point: Vec3,
    pub normal: Vec3,
}

/// One scene part paired with its shading appearance (per-part, so an
/// assembly's members can each carry a distinct [`member_appearance`]).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScenePart {
    pub shape: SdfPart,
    pub appearance: Appearance,
}

/// A composed scene: the union of every part, intersected with every ground
/// cut. `Default` (empty parts, empty cuts) is the degenerate/placeholder
/// sentinel every family builder returns for a hostile or empty design —
/// [`scene_extent_mm`] reports `None` for it, driving the existing
/// wireframe-style placeholder.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SdfScene {
    pub parts: Vec<ScenePart>,
    pub cuts: Vec<GroundPlane>,
}

/// Rotate a vector about the world Y axis by `angle` (`x' = x·cosθ +
/// z·sinθ`, `z' = -x·sinθ + z·cosθ`, `y` fixed).
fn rotate_y(v: Vec3, angle: f64) -> Vec3 {
    let (s, c) = angle.sin_cos();
    [v[0] * c + v[2] * s, v[1], -v[0] * s + v[2] * c]
}

/// Rotate a vector about the world X axis by `angle` (`y' = y·cosθ -
/// z·sinθ`, `z' = y·sinθ + z·cosθ`, `x` fixed).
fn rotate_x(v: Vec3, angle: f64) -> Vec3 {
    let (s, c) = angle.sin_cos();
    [v[0], v[1] * c - v[2] * s, v[1] * s + v[2] * c]
}

/// Distance from world-frame `p` to one scene part.
///
/// **`TorusArc`'s world→local transform.** The part's local frame starts
/// from the canonical torus frame (axis = world Y, ring in the world XZ
/// plane, sweep from local +X) and applies, IN ORDER, a `tilt` rotation
/// about world X then a `y_rotation` about world Y — so world→local undoes
/// them in reverse: undo `y_rotation` first, then undo `tilt`. This
/// two-angle (yaw, tilt) parametrization is enough to place extension
/// hooks: a hook's loop lies in the VERTICAL half-plane at its attach
/// azimuth `a` (world Y and the radial direction `(cos a, 0, sin a)` span
/// it), so `y_rotation = -a` swings the canonical horizontal ring's local
/// +X direction onto that radial direction, and `tilt = ∓π/2` (sign per
/// loop direction) tips the ring's axis from vertical (world Y) into the
/// tangential direction, landing the ring itself in the vertical
/// half-plane. See `extension_sdf`'s `hook_torus_part` for the concrete
/// angles; test (b)'s centerline-agreement check is the empirical proof
/// (the revert-probe drops the hooks entirely and watches it fail).
fn part_distance(shape: &SdfPart, p: Vec3) -> f64 {
    match shape {
        SdfPart::Helix(h) => sd_helix(p, h),
        SdfPart::TorusArc {
            center,
            y_rotation,
            tilt,
            major_r,
            minor_r,
            sweep,
        } => {
            let local = rotate_x(rotate_y(vsub(p, *center), -y_rotation), -tilt);
            sd_torus_arc(local, *major_r, *minor_r, *sweep)
        }
        SdfPart::Capsule { a, b, radius_mm } => sd_capsule(p, *a, *b, *radius_mm),
    }
}

/// Union over every part (the nearest surface wins) plus which part index
/// won — the appearance lookup key for shading. An empty scene reports
/// `f64::INFINITY` at index 0 (never reached in practice: callers check
/// [`scene_extent_mm`] first, the same discipline as the wireframe's
/// `scene_extent`/placeholder gate).
#[allow(dead_code)] // CPU-side reference: exercised by the test suite (WGSL cross-checks)
fn sdf_eval_part(scene: &SdfScene, p: Vec3) -> (f64, usize) {
    let mut best = f64::INFINITY;
    let mut best_index = 0usize;
    for (index, part) in scene.parts.iter().enumerate() {
        let d = part_distance(&part.shape, p);
        if d < best {
            best = d;
            best_index = index;
        }
    }
    (best, best_index)
}

/// Full scene distance: the part union, then every ground cut folded in via
/// [`cut_plane`] (sequential `max` — associative/commutative, so cut order
/// never matters).
#[allow(dead_code)] // CPU-side reference: exercised by the test suite (WGSL cross-checks)
fn sdf_eval(scene: &SdfScene, p: Vec3) -> f64 {
    let (mut d, _) = sdf_eval_part(scene, p);
    for cut in &scene.cuts {
        d = cut_plane(d, p, cut.point, cut.normal);
    }
    d
}

/// Coarse spatial reach of a scene — `None` for an empty (degenerate) scene,
/// driving the existing placeholder exactly like `viz::scene_extent`'s
/// `None` does for the wireframe path. Otherwise `(extent, y_mid)`: `extent`
/// is the larger of "twice the farthest radial reach from the Y axis" and
/// "the full axial (Y) span", mirroring `scene_from_radius`'s own extent
/// formula; `y_mid` is the TRUE midpoint of that Y span, `(y_min + y_max) /
/// 2`. Per-part bounding reach is CONSERVATIVE (an over-estimate of the
/// part's true footprint, e.g. a torus arc's full major+minor radius even
/// though the arc doesn't sweep the full circle, or the Helix arm's
/// centerline range padded by the wire radius on both ends to cover the
/// terminal cap's overhang — see [`sd_helix`]'s terminal-cap treatment) —
/// camera fitting (Task 4) tolerates slack; only the distance FUNCTIONS
/// themselves (not this bound) carry the sphere-tracing conservativeness
/// contract.
///
/// **Why `y_mid`, not an assumed `extent / 2`.** A body-only scene
/// (compression/conical: dead/active coils flattened symmetrically at both
/// ends) has `y_min == 0`, so `y_mid == extent / 2` coincidentally — but
/// extension's asymmetric hook radii (`r1 = D/2`, `r2 = D/4` by spec
/// default) push the true Y span well off that assumption (the bottom hook
/// dips below `y = 0`); a camera built from `extent / 2` alone would target
/// the WRONG point and crop or off-center the fitted view. Callers MUST use
/// the returned `y_mid`, not re-derive it from `extent`.
pub(crate) fn scene_extent_mm(scene: &SdfScene) -> Option<(f64, f64)> {
    if scene.parts.is_empty() {
        return None;
    }
    let mut radial: f64 = 0.0;
    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;
    let mut expand = |center_radial: f64, reach: f64, y_lo: f64, y_hi: f64| {
        radial = radial.max(center_radial + reach);
        y_min = y_min.min(y_lo);
        y_max = y_max.max(y_hi);
    };
    for part in &scene.parts {
        match &part.shape {
            SdfPart::Helix(h) => {
                let wire_r = profile_cap_radius(h.profile);
                let big_r = h.radius_mm.max(h.taper_small_r.unwrap_or(h.radius_mm));
                let y0 = h.axial_offset_mm;
                let y1 = h.axial_offset_mm + h.pitch_mm * h.turns;
                // Pad the centerline range by the terminal cap radius: the
                // wire's rounded end can sit up to `wire_r` beyond the
                // centerline endpoint in ANY direction (including Y), so a
                // centerline-only range under-covers by up to one wire
                // radius at each open end (review finding 4).
                expand(
                    0.0,
                    big_r + wire_r,
                    y0.min(y1) - wire_r,
                    y0.max(y1) + wire_r,
                );
            }
            SdfPart::TorusArc {
                center,
                major_r,
                minor_r,
                ..
            } => {
                let center_radial = center[0].hypot(center[2]);
                let reach = major_r + minor_r;
                expand(center_radial, reach, center[1] - reach, center[1] + reach);
            }
            SdfPart::Capsule { a, b, radius_mm } => {
                let reach_radial = a[0].hypot(a[2]).max(b[0].hypot(b[2]));
                expand(
                    reach_radial,
                    *radius_mm,
                    a[1].min(b[1]) - radius_mm,
                    a[1].max(b[1]) + radius_mm,
                );
            }
        }
    }
    let extent = (2.0 * radial).max(y_max - y_min);
    let y_mid = (y_min + y_max) / 2.0;
    (radial.is_finite() && radial > 0.0 && extent.is_finite() && extent > 0.0 && y_mid.is_finite())
        .then_some((extent, y_mid))
}

/// Whether a coil count is too hostile to render — mirrors
/// `scene_from_radius`'s guard (non-finite/negative `active`, or `total`
/// outside `[0, MAX_RENDER_TURNS]`), plus `total <= 0.0`: unlike the
/// wireframe (whose radius closures are constant per family), a tapered
/// [`helical_body_parts`] reconstruction divides by `total` to place each
/// segment's start/end fraction, so a zero total must bail here rather than
/// produce a NaN taper endpoint.
fn coils_hostile(active: f64, total: f64) -> bool {
    !active.is_finite()
        || active < 0.0
        || !(0.0..=super::MAX_RENDER_TURNS).contains(&total)
        || total <= 0.0
}

/// Whether any of a design's own solved geometry fields (radius/wire/pitch —
/// independent of coil counts) are unusable. A non-finite value here would
/// otherwise poison every `HelixParams` field silently (NaN propagating
/// through the taper/pitch arithmetic) rather than failing loud; checked
/// ALONGSIDE `coils_hostile` so a builder bails to `SdfScene::default()`
/// before constructing a single part, matching the wireframe's degenerate
/// fixtures (a NaN `mean_dia`/`wire_dia`) one-for-one rather than relying on
/// `scene_extent_mm`'s downstream filtering.
fn geometry_hostile(values: &[f64]) -> bool {
    values.iter().any(|v| !v.is_finite())
}

/// Reconstructs the dead_lo/active/dead_hi `Helix` parts that reproduce
/// `viz::coil_height_fn`'s piecewise dead-coil flattening for one coil body
/// — shared by `compression_sdf`, `conical_sdf`, and (per member)
/// `assembly_sdf`. `radius_at(t)` gives the body's mean coil radius at
/// coil-count fraction `t ∈ [0, 1]` of the FULL `total` sweep (constant for
/// compression/assembly members, linear taper for conical) — the same
/// closure shape `scene_from_radius`'s callers pass. `axial_shift` positions
/// the body's base at a nonzero height (assembly's series stacking); every
/// part gets `appearance`.
///
/// **Why 3 parts, not 1.** `coil_height_fn` flattens dead end coils to wire
/// pitch while active coils run at the solved pitch — a piecewise height no
/// single `HelixParams` (one `pitch_mm`) can express. Each segment below is
/// an independent `HelixParams`, sized from `dead_per_end =
/// ((total - active) / 2).max(0)`; a zero-turn segment is skipped (`Plain`
/// ends: `dead_per_end == 0`, collapsing to the single active segment).
///
/// **Phase continuity (review-F2 fix).** Each segment's `phase_rad` is
/// `TAU ×` the cumulative turns wound before it (dead-lo: `0`; active:
/// `TAU·dead_per_end`; dead-hi: `TAU·(dead_per_end + active)`), so every
/// segment STARTS at the exact world azimuth where the previous one ended —
/// one continuous wire for ANY dead/active split, fractional counts
/// included (`PlainGround`'s `dead_per_end = 0.5`, a fractional `active`
/// like `7.5`). The phase-less reconstruction restarted every segment at
/// world azimuth 0 — exact only for integer counts, with a real seam (two
/// dangling wire ends, interpenetrating coils) otherwise;
/// `plain_ground_segment_joins_have_no_azimuthal_seam` and
/// `fractional_active_squared_top_join_has_no_azimuthal_seam` pin the fix.
fn helical_body_parts(
    radius_at: impl Fn(f64) -> f64,
    active: f64,
    total: f64,
    pitch_mm: f64,
    wire_mm: f64,
    axial_shift: f64,
    appearance: Appearance,
) -> Vec<ScenePart> {
    let wire_r = wire_mm / 2.0;
    let dead_per_end = ((total - active) / 2.0).max(0.0);
    let t1 = dead_per_end / total;
    let t2 = (dead_per_end + active) / total;
    let mut parts = Vec::with_capacity(3);
    let mut push_segment = |turns: f64, pitch: f64, r0: f64, r1: f64, offset: f64, phase: f64| {
        if turns <= 0.0 {
            return;
        }
        parts.push(ScenePart {
            shape: SdfPart::Helix(HelixParams {
                radius_mm: r0,
                taper_small_r: Some(r1),
                pitch_mm: pitch,
                turns,
                profile: Profile::Circle { radius_mm: wire_r },
                axial_offset_mm: offset,
                phase_rad: phase,
            }),
            appearance,
        });
    };
    push_segment(
        dead_per_end,
        wire_mm,
        radius_at(0.0),
        radius_at(t1),
        axial_shift,
        0.0,
    );
    push_segment(
        active,
        pitch_mm,
        radius_at(t1),
        radius_at(t2),
        axial_shift + dead_per_end * wire_mm,
        TAU * dead_per_end,
    );
    push_segment(
        dead_per_end,
        wire_mm,
        radius_at(t2),
        radius_at(1.0),
        axial_shift + dead_per_end * wire_mm + active * pitch_mm,
        TAU * (dead_per_end + active),
    );
    parts
}

/// Whether `end_type` machines a flat, perpendicular ground face — the
/// `*Ground` `EndType` variants (`PlainGround`, `SquaredGround`);
/// `Plain`/`Squared` end coils are NOT ground flat (the wire tip still
/// follows the helix pitch), so no cut plane applies to them.
fn is_ground_end(end_type: springcore::EndType) -> bool {
    matches!(
        end_type,
        springcore::EndType::PlainGround | springcore::EndType::SquaredGround
    )
}

/// One [`GroundPlane`] per ground end (bottom + top), positioned at the
/// TRUE ground faces — `y = 0` and `y = total_height` (review F1; the
/// previous inset by `wire_r` sat one wire radius too deep and shaved real
/// coil material). The geometry: `coil_height_fn`'s centerline spans
/// `[0, H]` with `H` = dead coils at wire pitch + active at solved pitch —
/// for SquaredGround that is exactly Shigley's `L0`; the un-ground wire
/// (the Squared blank) fills `[-wire_r, H + wire_r]`, and grinding removes
/// half a wire diameter of material per end (Shigley Table 10-1: a ground
/// end carries `d/2` less material per end), leaving flat faces at the
/// centerline extremes and preserving face-to-face = `L0`. Solid-length
/// cross-check: at solid the same faces sit at `{0, Nt·d}`, reproducing
/// `Ls = d·Nt` exactly. `total_height` comes from the SAME
/// `viz::coil_height_fn` the wireframe uses. Empty for non-ground
/// `end_type`s.
fn ground_cuts(
    end_type: springcore::EndType,
    active: f64,
    total: f64,
    pitch_mm: f64,
    wire_mm: f64,
) -> Vec<GroundPlane> {
    if !is_ground_end(end_type) {
        return Vec::new();
    }
    let total_height = crate::viz::coil_height_fn(active, total, pitch_mm, wire_mm)(1.0);
    vec![
        GroundPlane {
            point: [0.0, 0.0, 0.0],
            normal: [0.0, -1.0, 0.0],
        },
        GroundPlane {
            point: [0.0, total_height, 0.0],
            normal: [0.0, 1.0, 0.0],
        },
    ]
}

/// The five `SpringDesign` fields every single-radius coil body needs
/// (`active`/`total` coils, mean radius, wire, pitch), pre-validated against
/// the coils/geometry/pitch hostility gate — shared by `compression_sdf` and
/// `assembly_sdf`'s per-member loop (simplifier F2; every assembly member IS
/// a `SpringDesign`, so it needs the identical reader and gate).
struct CoilGeom {
    active: f64,
    total: f64,
    r: f64,
    wire: f64,
    pitch: f64,
}

/// Reads [`CoilGeom`] from a `SpringDesign`'s solved fields, or `None` if
/// the coil counts/geometry are hostile — INCLUDING `pitch <= 0.0` (review
/// finding 6). `pitch` is a real, independent parameter for every caller of
/// this reader (unlike extension/torsion's close-wound body, whose
/// `pitch_mm = wire` is always positive by construction, so those families
/// don't go through this reader at all) — see [`sd_helix`]'s doc for why
/// `pitch_mm == 0.0` is a genuine precondition violation, not merely an
/// unusual value. `geometry_hostile` alone doesn't catch it (`0.0` is
/// finite), so it's checked separately.
fn coil_geom(design: &springcore::SpringDesign) -> Option<CoilGeom> {
    let active = design.active_coils;
    let total = design.total_coils;
    let r = design.mean_dia.millimeters() / 2.0;
    let wire = design.wire_dia.millimeters();
    let pitch = design.pitch.millimeters();
    if coils_hostile(active, total) || geometry_hostile(&[r, wire, pitch]) || pitch <= 0.0 {
        return None;
    }
    Some(CoilGeom {
        active,
        total,
        r,
        wire,
        pitch,
    })
}

/// Compression family SDF scene: reuses `SpringDesign`'s SOLVED fields
/// (`mean_dia`, `wire_dia`, `pitch`, `active_coils`, `total_coils`,
/// `end_type`) via [`coil_geom`] — the same fields
/// `compression::scene_model::compression_scene` reads — so both geometry
/// paths render the SAME spring. See [`helical_body_parts`] for why a
/// ground-ended design needs 3 `Helix` parts, not the single helix a first
/// glance suggests.
pub(crate) fn compression_sdf(d: &springcore::SpringDesign) -> SdfScene {
    let Some(g) = coil_geom(d) else {
        return SdfScene::default();
    };
    SdfScene {
        parts: helical_body_parts(|_| g.r, g.active, g.total, g.pitch, g.wire, 0.0, steel()),
        cuts: ground_cuts(d.end_type, g.active, g.total, g.pitch, g.wire),
    }
}

/// Conical family SDF scene: linear taper from `large_mean_dia` to
/// `small_mean_dia` across the FULL total-coil sweep (matching
/// `conical::scene_model::conical_scene`'s `radius_at`), same dead-coil
/// reconstruction and ground cuts as `compression_sdf` — including the same
/// `pitch <= 0.0` hostility check (review finding 6; see
/// `compression_sdf`'s doc).
pub(crate) fn conical_sdf(d: &springcore::conical::ConicalDesign) -> SdfScene {
    let active = d.inputs.active_coils;
    let total = d.total_coils;
    let r_large = d.inputs.large_mean_dia.millimeters() / 2.0;
    let r_small = d.inputs.small_mean_dia.millimeters() / 2.0;
    let wire = d.inputs.wire_dia.millimeters();
    let pitch = d.pitch.millimeters();
    if coils_hostile(active, total)
        || geometry_hostile(&[r_large, r_small, wire, pitch])
        || pitch <= 0.0
    {
        return SdfScene::default();
    }
    let radius_at = |t: f64| r_large + (r_small - r_large) * t;
    SdfScene {
        parts: helical_body_parts(radius_at, active, total, pitch, wire, 0.0, steel()),
        cuts: ground_cuts(d.inputs.end_type, active, total, pitch, wire),
    }
}

/// One extension hook as a `TorusArc` part — see [`part_distance`]'s doc for
/// the `(y_rotation, tilt)` frame convention. `attach_angle`/`attach_h` pin
/// the loop's start to the coil body's endpoint (matching
/// `extension::scene_model::hook_arc`'s `arc(0)` continuity guarantee);
/// `sign` picks the loop direction (that same function's `sign` parameter:
/// -1 for the bottom hook toward -y, +1 for the top hook toward +y); `sweep
/// = 1.5π` matches `hook_arc`'s `SAMPLES` range.
fn hook_torus_part(
    attach_angle: f64,
    attach_h: f64,
    coil_r: f64,
    hook_r: f64,
    sign: f64,
    wire_r: f64,
    appearance: Appearance,
) -> ScenePart {
    let center_r = coil_r - hook_r;
    ScenePart {
        shape: SdfPart::TorusArc {
            center: [
                center_r * attach_angle.cos(),
                attach_h,
                center_r * attach_angle.sin(),
            ],
            y_rotation: -attach_angle,
            tilt: -sign * FRAC_PI_2,
            major_r: hook_r,
            minor_r: wire_r,
            sweep: 1.5 * PI,
        },
        appearance,
    }
}

/// Extension family SDF scene: a close-wound `Helix` body (no dead coils —
/// `active_coils` IS the full turn count, matching
/// `extension::scene_model::extension_scene`'s `close_wound_coil` call)
/// plus the two hook `TorusArc`s, each attached exactly at its body
/// endpoint.
pub(crate) fn extension_sdf(d: &springcore::extension::ExtensionDesign) -> SdfScene {
    let turns = d.active_coils;
    let r = d.mean_dia.millimeters() / 2.0;
    let wire = d.wire_dia.millimeters();
    if coils_hostile(turns, turns) || geometry_hostile(&[r, wire]) {
        return SdfScene::default();
    }
    let wire_r = wire / 2.0;
    let body_h = turns * wire;
    let end_angle = turns * TAU;
    let appearance = steel();
    SdfScene {
        parts: vec![
            ScenePart {
                shape: SdfPart::Helix(HelixParams {
                    radius_mm: r,
                    taper_small_r: None,
                    pitch_mm: wire,
                    turns,
                    profile: Profile::Circle { radius_mm: wire_r },
                    axial_offset_mm: 0.0,
                    phase_rad: 0.0,
                }),
                appearance,
            },
            hook_torus_part(
                0.0,
                0.0,
                r,
                d.hooks.r1.millimeters(),
                -1.0,
                wire_r,
                appearance,
            ),
            hook_torus_part(
                end_angle,
                body_h,
                r,
                d.hooks.r2.millimeters(),
                1.0,
                wire_r,
                appearance,
            ),
        ],
        cuts: Vec::new(),
    }
}

/// Torsion family SDF scene: a close-wound `Helix` body plus two straight
/// `Capsule` legs, tangential at each body endpoint (mirrors
/// `torsion::scene_model::torsion_scene`'s `leg` closure: tangent at wire
/// parameter `phi` is `(-sin phi, cos phi)`, legs stay at the endpoint's
/// height).
pub(crate) fn torsion_sdf(d: &springcore::torsion::TorsionDesign) -> SdfScene {
    let turns = d.inputs.body_coils;
    let r = d.inputs.mean_dia.millimeters() / 2.0;
    let wire = d.inputs.wire_dia.millimeters();
    if coils_hostile(turns, turns) || geometry_hostile(&[r, wire]) {
        return SdfScene::default();
    }
    let wire_r = wire / 2.0;
    let end_angle = turns * TAU;
    let start = [r, 0.0, 0.0];
    let end = [r * end_angle.cos(), turns * wire, r * end_angle.sin()];
    let l1 = d.inputs.leg1.millimeters();
    let l2 = d.inputs.leg2.millimeters();
    // Tangent at phi=0 is (-sin 0, cos 0) = (0, 1); leg1's sign is -1 (mirrors
    // torsion_scene's leg closure exactly).
    let leg1_end = [start[0], start[1], start[2] - l1];
    let (s2, c2) = end_angle.sin_cos();
    let leg2_end = [end[0] + l2 * -s2, end[1], end[2] + l2 * c2];
    let appearance = steel();
    SdfScene {
        parts: vec![
            ScenePart {
                shape: SdfPart::Helix(HelixParams {
                    radius_mm: r,
                    taper_small_r: None,
                    pitch_mm: wire,
                    turns,
                    profile: Profile::Circle { radius_mm: wire_r },
                    axial_offset_mm: 0.0,
                    phase_rad: 0.0,
                }),
                appearance,
            },
            ScenePart {
                shape: SdfPart::Capsule {
                    a: start,
                    b: leg1_end,
                    radius_mm: wire_r,
                },
                appearance,
            },
            ScenePart {
                shape: SdfPart::Capsule {
                    a: end,
                    b: leg2_end,
                    radius_mm: wire_r,
                },
                appearance,
            },
        ],
        cuts: Vec::new(),
    }
}

/// Assembly family SDF scene: each member's own coil body (via [`coil_geom`]
/// and [`helical_body_parts`] — every member IS a `SpringDesign`, so this
/// reuses `compression_sdf`'s exact reconstruction AND hostility gate,
/// `pitch <= 0.0` included per review finding 6) tinted by
/// [`member_appearance`], Nested concentric (every member at axial offset 0)
/// or Series stacked with the SAME running-offset/gap the wireframe uses
/// (`assembly::scene_model::assembly_scene`'s `2 × max member wire dia`).
///
/// No ground cuts: [`cut_plane`]'s half-space is GLOBAL to the whole scene
/// (`sdf_eval` applies every cut to the combined union), so a per-member
/// ground plane would also clip every OTHER member sharing the scene —
/// wrong for Series' stacked members sitting above it. Any hostile/capped
/// member degrades the WHOLE scene (simpler than the wireframe's nuanced
/// per-topology partial-cascade semantics, but the same "an empty/hostile
/// member misrepresents the design" spirit `assembly_scene` documents).
pub(crate) fn assembly_sdf(d: &springcore::assembly::AssemblyDesign) -> SdfScene {
    if d.members.is_empty() {
        return SdfScene::default();
    }
    let gap = 2.0
        * d.members
            .iter()
            .map(|m| m.design.wire_dia.millimeters())
            .fold(0.0_f64, f64::max);
    let mut parts = Vec::new();
    let mut y_base = 0.0_f64;
    for (index, member) in d.members.iter().enumerate() {
        let Some(g) = coil_geom(&member.design) else {
            return SdfScene::default();
        };
        parts.extend(helical_body_parts(
            |_| g.r,
            g.active,
            g.total,
            g.pitch,
            g.wire,
            y_base,
            member_appearance(index),
        ));
        if d.topology == springcore::assembly::Topology::Series {
            let height = crate::viz::coil_height_fn(g.active, g.total, g.pitch, g.wire)(1.0);
            y_base += height + gap;
        }
    }
    SdfScene {
        parts,
        cuts: Vec::new(),
    }
}

/// sRGB EOTF (decode: encoded -> linear) for one channel — mirrors
/// `iced_core::Color::into_linear`'s private `linear_component` helper
/// exactly (same 0.04045 threshold, same piecewise formula:
/// <https://en.wikipedia.org/wiki/SRGB#The_reverse_transformation>).
/// Hand-rolled here rather than routed through `iced::Color` — ADR 0008
/// purity forbids `iced` types in this module — but it MUST NOT
/// independently drift from iced's own conversion, since `viz::bg_rgba`
/// linearizes the very same sRGB-authoring convention (via the real
/// `iced::Color::into_linear`) for the background these packed part colors
/// share a shader with.
fn srgb_to_linear(u: f32) -> f32 {
    if u < 0.04045 {
        u / 12.92
    } else {
        ((u + 0.055) / 1.055).powf(2.4)
    }
}

/// Pack `appearance`'s 5 floats (`base_color[0..3]`, `metallic`,
/// `roughness`, already `f32`) into `slot[0..5]` — linearizing `base_color`
/// via [`srgb_to_linear`] per [`Appearance`]'s documented convention;
/// `metallic`/`roughness` are scalars, not colors, and pack unchanged.
fn pack_appearance(slot: &mut [f32], appearance: Appearance) {
    slot[0] = srgb_to_linear(appearance.base_color[0]);
    slot[1] = srgb_to_linear(appearance.base_color[1]);
    slot[2] = srgb_to_linear(appearance.base_color[2]);
    slot[3] = appearance.metallic;
    slot[4] = appearance.roughness;
}

/// Pack one [`ScenePart`] into a `FLOATS_PER_PART`-wide slot — see
/// [`scene_uniforms`]'s doc for the binding per-kind layout table.
fn pack_part(slot: &mut [f32], part: &ScenePart) {
    match &part.shape {
        SdfPart::Helix(h) => {
            slot[0] = 0.0;
            slot[1] = h.radius_mm as f32;
            // Negative sentinel for `None` — see `NO_TAPER_SENTINEL`'s doc.
            slot[2] = h.taper_small_r.map_or(NO_TAPER_SENTINEL, |v| v as f32);
            slot[3] = h.pitch_mm as f32;
            slot[4] = h.turns as f32;
            let (profile_kind, dim0, dim1) = match h.profile {
                Profile::Circle { radius_mm } => (0.0, radius_mm as f32, 0.0),
                Profile::Rectangle {
                    half_w_mm,
                    half_h_mm,
                } => (1.0, half_w_mm as f32, half_h_mm as f32),
            };
            slot[5] = profile_kind;
            slot[6] = dim0;
            slot[7] = dim1;
            slot[8] = h.axial_offset_mm as f32;
            slot[9] = h.phase_rad as f32;
            pack_appearance(&mut slot[10..15], part.appearance);
        }
        SdfPart::TorusArc {
            center,
            y_rotation,
            tilt,
            major_r,
            minor_r,
            sweep,
        } => {
            slot[0] = 1.0;
            slot[1] = center[0] as f32;
            slot[2] = center[1] as f32;
            slot[3] = center[2] as f32;
            slot[4] = *y_rotation as f32;
            slot[5] = *tilt as f32;
            slot[6] = *major_r as f32;
            slot[7] = *minor_r as f32;
            slot[8] = *sweep as f32;
            pack_appearance(&mut slot[9..14], part.appearance);
        }
        SdfPart::Capsule { a, b, radius_mm } => {
            slot[0] = 2.0;
            slot[1] = a[0] as f32;
            slot[2] = a[1] as f32;
            slot[3] = a[2] as f32;
            slot[4] = b[0] as f32;
            slot[5] = b[1] as f32;
            slot[6] = b[2] as f32;
            slot[7] = *radius_mm as f32;
            pack_appearance(&mut slot[8..13], part.appearance);
        }
    }
}

/// Pack one [`GroundPlane`] into a `FLOATS_PER_CUT`-wide slot: `[0..3]`
/// point, `[3..6]` normal, `[6..8]` pad (`0.0`, via the caller's zero-filled
/// buffer).
fn pack_cut(slot: &mut [f32], cut: &GroundPlane) {
    slot[0] = cut.point[0] as f32;
    slot[1] = cut.point[1] as f32;
    slot[2] = cut.point[2] as f32;
    slot[3] = cut.normal[0] as f32;
    slot[4] = cut.normal[1] as f32;
    slot[5] = cut.normal[2] as f32;
}

/// Fixed-stride packing of an [`SdfScene`] into a flat `f32` buffer for the
/// WGSL uniform (Task 5 substitutes [`MAX_PARTS`]/[`FLOATS_PER_PART`]/
/// [`MAX_CUTS`]/[`FLOATS_PER_CUT`] into the shader source, so both sides
/// read the identical layout from these constants — the mirror-drift
/// discipline). `None` — a representability guard, never truncation — iff
/// `scene.parts.len() > MAX_PARTS` or `scene.cuts.len() > MAX_CUTS`.
///
/// **Buffer shape**, always exactly
/// `4 + MAX_PARTS·FLOATS_PER_PART + MAX_CUTS·FLOATS_PER_CUT` floats (fixed
/// regardless of how many parts/cuts the scene actually has — the WGSL
/// uniform is a fixed-size array; unused slots past `n_parts`/`n_cuts` are
/// zero-filled):
/// - `[0]` = `parts.len()` as `f32`, `[1]` = `cuts.len()` as `f32`, `[2..4)`
///   padding (`0.0`) — a fixed 4-float header so both sides can index the
///   part/cut arrays at a compile-time-known offset.
/// - `[4 .. 4 + MAX_PARTS·FLOATS_PER_PART)`: one `FLOATS_PER_PART`-float
///   block per part SLOT. Slot layout, `kind = block[0]` (`0.0` Helix,
///   `1.0` TorusArc, `2.0` Capsule):
///   - **Helix** (15 of 16 floats used, 1 pad): `[1]` radius_mm, `[2]`
///     taper_small_r (negative [`NO_TAPER_SENTINEL`] encodes `None` — no
///     separate presence flag needed since a real radius can never be
///     negative; **WGSL-side decode contract: `taper < 0.0` means no
///     taper** — an ordinary numeric comparison, not `isNan()`/a NaN
///     comparison, which WGSL implementations may legally constant-fold
///     under a finite-math assumption; see the note below and
///     [`NO_TAPER_SENTINEL`]'s doc), `[3]` pitch_mm, `[4]` turns, `[5]`
///     profile_kind (`0.0`
///     Circle / `1.0` Rectangle), `[6]` profile_dim0 (Circle: radius_mm;
///     Rectangle: half_w_mm), `[7]` profile_dim1 (Circle: unused `0.0`;
///     Rectangle: half_h_mm), `[8]` axial_offset_mm, `[9]` phase_rad,
///     `[10..13)` appearance.base_color, `[13]` metallic, `[14]` roughness,
///     `[15]` pad.
///   - **TorusArc** (14 of 16 used, 2 pad): `[1..4)` center, `[4]`
///     y_rotation, `[5]` tilt, `[6]` major_r, `[7]` minor_r, `[8]` sweep,
///     `[9..12)` appearance.base_color, `[12]` metallic, `[13]` roughness,
///     `[14..16)` pad.
///   - **Capsule** (13 of 16 used, 3 pad): `[1..4)` a, `[4..7)` b, `[7]`
///     radius_mm, `[8..11)` appearance.base_color, `[11]` metallic, `[12]`
///     roughness, `[13..16)` pad.
/// - `[4 + MAX_PARTS·FLOATS_PER_PART ..)`: one `FLOATS_PER_CUT`-float
///   (`GroundPlane`) block per cut SLOT — see [`pack_cut`].
///
/// **Output-side finiteness sweep (review finding 5 fix — the
/// subset-guard-bypass class).** Every family builder rejects non-finite
/// `f64` geometry via `geometry_hostile` upstream, but that guard runs
/// BEFORE the `as f32` narrowing cast below — a form-reachable value that is
/// a perfectly finite `f64` (e.g. an extreme `free_length`/`active_coils`
/// solve landing a huge but finite radius, `1e40`) can still overflow to
/// `f32::INFINITY` on packing, which `geometry_hostile`'s f64 check can
/// never see. Without a check on the FINAL packed buffer, that `inf` would
/// reach the shader as a "representable" (`Some`) uniform — `use_shaded`
/// would pick the shaded path and render garbage, not the documented
/// wireframe fallback. So: after packing, this function sweeps every float
/// in `out` and returns `None` if ANY of them is non-finite — cheap (one
/// linear pass over a buffer already fully materialized) and catches every
/// overflow source at once (radius, pitch, turns, taper, appearance —
/// whichever field the hostile input lands in) without needing a
/// per-field `f32`-range precheck upstream. This SUPERSEDES the prior
/// per-field design (a directly-constructed `Some(f64::NAN)` taper used to
/// round-trip as `Some(NaN)` rather than `None` — see
/// `scene_uniforms_some_nan_taper_now_fails_the_finiteness_sweep`): a
/// poisoned buffer reaching the GPU renders silently-wrong pixels, which is
/// worse than the visible, documented `None` -> wireframe fallback.
/// `pack_part` itself is UNCHANGED — it still packs `NaN as f32` like any
/// other value with no special-casing, so `unpack_scene`'s own decode
/// contract (`NO_TAPER_SENTINEL`'s `< 0.0` — an ordinary comparison, false
/// for `NaN`, not a NaN-testing one WGSL might constant-fold away) is still
/// exercisable directly against a hand-packed buffer.
///
/// Every geometry value packs `as f32` (mm-scale doubles lose ~1e-7
/// relative precision, well under any rendering-visible threshold);
/// `Appearance`'s fields are already `f32`.
pub(crate) fn scene_uniforms(scene: &SdfScene) -> Option<Vec<f32>> {
    if scene.parts.len() > MAX_PARTS || scene.cuts.len() > MAX_CUTS {
        return None;
    }
    let mut out = vec![0.0f32; SCENE_UNIFORM_FLOATS];
    out[0] = scene.parts.len() as f32;
    out[1] = scene.cuts.len() as f32;
    for (i, part) in scene.parts.iter().enumerate() {
        let base = 4 + i * FLOATS_PER_PART;
        pack_part(&mut out[base..base + FLOATS_PER_PART], part);
    }
    for (i, cut) in scene.cuts.iter().enumerate() {
        let base = CUTS_BASE_FLOATS + i * FLOATS_PER_CUT;
        pack_cut(&mut out[base..base + FLOATS_PER_CUT], cut);
    }
    out.iter().all(|v| v.is_finite()).then_some(out)
}

/// Inverse sRGB EOTF (linear -> encoded), test-only: undoes
/// [`srgb_to_linear`] so [`unpack_appearance`]/[`unpack_scene`] stays a
/// genuine round-trip inverse of [`pack_appearance`]/[`scene_uniforms`]
/// rather than a partial one that goes blind to the pack-time
/// linearization. Standard piecewise sRGB encode (threshold 0.0031308,
/// the linear-side value corresponding to the decode's 0.04045 —
/// <https://en.wikipedia.org/wiki/SRGB#The_forward_transformation>).
#[cfg(test)]
fn linear_to_srgb(u: f32) -> f32 {
    if u <= 0.003_130_8 {
        u * 12.92
    } else {
        1.055 * u.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
fn unpack_appearance(slot: &[f32]) -> Appearance {
    Appearance {
        base_color: [
            linear_to_srgb(slot[0]),
            linear_to_srgb(slot[1]),
            linear_to_srgb(slot[2]),
        ],
        metallic: slot[3],
        roughness: slot[4],
    }
}

/// Inverse of [`pack_part`] — test-only (never reachable from production
/// code, which only ever needs the forward pack). `None` for an
/// unrecognized `kind` (defensive against a malformed buffer, e.g. a
/// revert-probed transposition landing a non-integer kind float).
#[cfg(test)]
fn unpack_part(slot: &[f32]) -> Option<ScenePart> {
    let kind = slot[0] as u8;
    let (shape, appearance_start) = match kind {
        0 => (
            SdfPart::Helix(HelixParams {
                radius_mm: f64::from(slot[1]),
                taper_small_r: if slot[2] < 0.0 {
                    None
                } else {
                    Some(f64::from(slot[2]))
                },
                pitch_mm: f64::from(slot[3]),
                turns: f64::from(slot[4]),
                profile: if slot[5] as u8 == 0 {
                    Profile::Circle {
                        radius_mm: f64::from(slot[6]),
                    }
                } else {
                    Profile::Rectangle {
                        half_w_mm: f64::from(slot[6]),
                        half_h_mm: f64::from(slot[7]),
                    }
                },
                axial_offset_mm: f64::from(slot[8]),
                phase_rad: f64::from(slot[9]),
            }),
            10,
        ),
        1 => (
            SdfPart::TorusArc {
                center: [f64::from(slot[1]), f64::from(slot[2]), f64::from(slot[3])],
                y_rotation: f64::from(slot[4]),
                tilt: f64::from(slot[5]),
                major_r: f64::from(slot[6]),
                minor_r: f64::from(slot[7]),
                sweep: f64::from(slot[8]),
            },
            9,
        ),
        2 => (
            SdfPart::Capsule {
                a: [f64::from(slot[1]), f64::from(slot[2]), f64::from(slot[3])],
                b: [f64::from(slot[4]), f64::from(slot[5]), f64::from(slot[6])],
                radius_mm: f64::from(slot[7]),
            },
            8,
        ),
        _ => return None,
    };
    Some(ScenePart {
        shape,
        appearance: unpack_appearance(&slot[appearance_start..appearance_start + 5]),
    })
}

#[cfg(test)]
fn unpack_cut(slot: &[f32]) -> GroundPlane {
    GroundPlane {
        point: [f64::from(slot[0]), f64::from(slot[1]), f64::from(slot[2])],
        normal: [f64::from(slot[3]), f64::from(slot[4]), f64::from(slot[5])],
    }
}

/// Inverse of [`scene_uniforms`] — test-only round-trip check (never
/// reachable from production code). `None` for a malformed buffer (wrong
/// total length, or a header claiming more parts/cuts than the fixed
/// budget provides slots for).
#[cfg(test)]
pub(crate) fn unpack_scene(u: &[f32]) -> Option<SdfScene> {
    if u.len() != SCENE_UNIFORM_FLOATS {
        return None;
    }
    let n_parts = u[0] as usize;
    let n_cuts = u[1] as usize;
    if n_parts > MAX_PARTS || n_cuts > MAX_CUTS {
        return None;
    }
    let mut parts = Vec::with_capacity(n_parts);
    for i in 0..n_parts {
        let base = 4 + i * FLOATS_PER_PART;
        parts.push(unpack_part(&u[base..base + FLOATS_PER_PART])?);
    }
    let mut cuts = Vec::with_capacity(n_cuts);
    for i in 0..n_cuts {
        let base = CUTS_BASE_FLOATS + i * FLOATS_PER_CUT;
        cuts.push(unpack_cut(&u[base..base + FLOATS_PER_CUT]));
    }
    Some(SdfScene { parts, cuts })
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

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
        let az = phi + h.phase_rad; // world azimuth of wire parameter phi
        [rr * az.cos(), cy + wire_r * ring_ang.sin(), rr * az.sin()]
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
            phase_rad: 0.0,
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
            phase_rad: 0.0,
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

    /// Golden-section search for the minimizer of `f` over `[lo, hi]`
    /// (assumed unimodal on that bracket) — `iters` shrink steps, each
    /// cutting the bracket by the golden ratio. Returns the ARGUMENT at the
    /// minimum, not the minimum value itself (callers re-apply `f`, or
    /// whatever derived quantity they need, at the returned point).
    /// Simplifier F6: the one golden-section implementation shared by every
    /// test below that needs an independent numeric minimum to check a
    /// closed-form claim against (previously duplicated verbatim).
    fn golden_min(f: impl Fn(f64) -> f64, mut lo: f64, mut hi: f64, iters: u32) -> f64 {
        let inv_gold = (5.0_f64.sqrt() - 1.0) / 2.0;
        let mut mid_lo = hi - inv_gold * (hi - lo);
        let mut mid_hi = lo + inv_gold * (hi - lo);
        let (mut f_lo, mut f_hi) = (f(mid_lo), f(mid_hi));
        for _ in 0..iters {
            if f_lo < f_hi {
                hi = mid_hi;
                mid_hi = mid_lo;
                f_hi = f_lo;
                mid_lo = hi - inv_gold * (hi - lo);
                f_lo = f(mid_lo);
            } else {
                lo = mid_lo;
                mid_lo = mid_hi;
                f_lo = f_hi;
                mid_hi = lo + inv_gold * (hi - lo);
                f_hi = f(mid_hi);
            }
        }
        if f_lo < f_hi {
            mid_lo
        } else {
            mid_hi
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
            let az = phi + h.phase_rad; // world azimuth of wire parameter phi
            let c = [
                big_r * az.cos(),
                h.pitch_mm * phi / TAU + h.axial_offset_mm,
                big_r * az.sin(),
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
        let lo = max_phi * f64::from(best_i.saturating_sub(1)) / f64::from(n);
        let hi = max_phi * f64::from((best_i + 1).min(n)) / f64::from(n);
        let phi_min = golden_min(dist_sq, lo, hi, 120);
        dist_sq(phi_min).sqrt() - wire_r
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
            phase_rad: 0.0,
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
            phase_rad: 0.0,
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
            phase_rad: 0.0,
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
            phase_rad: 0.0,
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

    #[test]
    fn helix_phase_rotates_the_whole_helix_rigidly() {
        // Review F2 semantics pin: a phase-φ helix is the phase-0 helix
        // rigidly rotated about Y, nothing else. `rotate_y(p, φ)` maps a
        // point at azimuth θ to azimuth θ - φ (see rotate_y's doc), exactly
        // undoing the phase — so sd(p, phase=φ) must equal
        // sd(rotate_y(p, φ), phase=0) for EVERY geometry and query. φ = 0 is
        // the identity, so this also pins phase-0 back-compat: every
        // existing phase-0 property (conservativeness sweeps included)
        // transfers to any phase by rotation invariance of distances.
        let mut rng = TestRng(0x9e37_79b9_7f4a_7c15);
        for _ in 0..400 {
            let radius_mm = rng.range(5.0, 20.0);
            let h0 = HelixParams {
                radius_mm,
                taper_small_r: Some(radius_mm * rng.range(0.3, 1.0)),
                pitch_mm: rng.range(1.0, 10.0),
                turns: rng.range(0.4, 9.0),
                profile: Profile::Circle { radius_mm: 0.8 },
                axial_offset_mm: rng.range(-5.0, 5.0),
                phase_rad: 0.0,
            };
            let phase = rng.range(-TAU, TAU);
            let hp = HelixParams {
                phase_rad: phase,
                ..h0
            };
            let p = [
                rng.range(-25.0, 25.0),
                rng.range(-10.0, 60.0),
                rng.range(-25.0, 25.0),
            ];
            let a = sd_helix(p, &hp);
            let b = sd_helix(rotate_y(p, phase), &h0);
            assert!(
                (a - b).abs() < 1e-9,
                "phase is not a rigid rotation: phase={phase} p={p:?} \
                 phased={a} rotated-unphased={b}"
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
            let k_star_star = golden_min(v_of_k, k_star - 5.0, k_star + 5.0, 200);

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
                phase_rad: 0.0,
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
            phase_rad: 0.0,
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
            phase_rad: 0.0,
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
            phase_rad: 0.0,
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
            phase_rad: 0.0,
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

    /// Review F1-extension fix: `pack_appearance` must linearize
    /// `base_color` (sRGB-authored, per `Appearance`'s doc) before it
    /// reaches the shader's per-part slot — the same reason `viz::bg_rgba`
    /// linearizes the background. Expected values are independently
    /// hand-computed (Python) via the standard sRGB EOTF
    /// (<https://en.wikipedia.org/wiki/SRGB#The_reverse_transformation>),
    /// not derived from this crate's own conversion — a genuine external
    /// pin, not a tautology against the code under test. `metallic`/
    /// `roughness` are plain scalars and must pack UNCHANGED.
    #[test]
    fn pack_appearance_linearizes_base_color_but_not_metallic_or_roughness() {
        let mut slot = [0.0f32; 5];
        pack_appearance(&mut slot, steel());
        // steel().base_color = [0.62, 0.64, 0.67] -> sRGB EOTF (Python):
        assert_relative_eq!(slot[0], 0.342_391_64, max_relative = 1e-5);
        assert_relative_eq!(slot[1], 0.367_246_46, max_relative = 1e-5);
        assert_relative_eq!(slot[2], 0.406_448_3, max_relative = 1e-5);
        // Not a no-op: the packed value must differ meaningfully from the
        // raw authored component (rules out a mutant that skips the
        // conversion entirely).
        for (packed, raw) in slot[0..3].iter().zip(steel().base_color) {
            assert!(
                (packed - raw).abs() > 0.1,
                "packed={packed} looks unlinearized (raw={raw})"
            );
        }
        assert_eq!(slot[3], steel().metallic);
        assert_eq!(slot[4], steel().roughness);
    }

    // --- Task 3: SdfScene composition, family builders, degenerate discipline ---

    /// Whether a wireframe centerline point is within reach of ANY of a
    /// scene's ground cuts — used to exclude that sliver of points from the
    /// strict centerline-agreement checks below. Grinding is a REAL
    /// geometric correction the wireframe does NOT model (its
    /// `coil_height_fn` draws the identical squared-but-un-ground
    /// centerline for every end type, starting the very first dead coil's
    /// centerline at `y = 0`; Shigley's own free-length/solid-length
    /// formulas put exactly one wire diameter LESS material in a
    /// SquaredGround end than a Squared one — half a wire diameter shaved
    /// from each end's outer face). "Within reach" is `wire_r`, not `0` —
    /// a centerline point up to a full wire radius on the KEPT side still
    /// has its own local material partially eaten by the cut (the cut
    /// plane is tangent to the wire's OWN cross-section there), so its
    /// SDF value degrades from the full `-wire_r` towards `0` even though
    /// it isn't literally past the plane. Concretely this excludes the
    /// OUTER HALF-TURN of the dead coil adjoining each ground end: the cut
    /// sits at the centerline extreme (`y = 0` at the bottom, review F1),
    /// so of that coil's full `[0, 2·wire_r]` centerline span only
    /// `[0, wire_r)` is within one wire radius of it — 16 of the 32
    /// samples per turn, pinned exactly by the agreement tests below
    /// (review F3). So near a
    /// ground end, the SDF (which DOES cut it) is MORE accurate than the
    /// wireframe, not less — those points are SUPPOSED to read closer to
    /// `0` than `-wire_r`. `ground_cut_flattens_material_below_the_
    /// squared_ground_end` pins that behavior directly; this helper just
    /// keeps the broader agreement checks honest about the region where
    /// the two paths deliberately diverge.
    fn within_ground_cut_reach(scene: &SdfScene, p: (f64, f64, f64), wire_r: f64) -> bool {
        scene.cuts.iter().any(|cut| {
            let v = [p.0 - cut.point[0], p.1 - cut.point[1], p.2 - cut.point[2]];
            let dist = v[0] * cut.normal[0] + v[1] * cut.normal[1] + v[2] * cut.normal[2];
            dist > -wire_r
        })
    }

    #[test]
    fn sdf_eval_part_unions_via_min_and_reports_the_winning_index() {
        let scene = SdfScene {
            parts: vec![
                ScenePart {
                    shape: SdfPart::Capsule {
                        a: [0.0, 0.0, 0.0],
                        b: [0.0, 0.0, 0.0],
                        radius_mm: 1.0,
                    },
                    appearance: steel(),
                },
                ScenePart {
                    shape: SdfPart::Capsule {
                        a: [3.0, 0.0, 0.0],
                        b: [3.0, 0.0, 0.0],
                        radius_mm: 1.0,
                    },
                    appearance: steel(),
                },
            ],
            cuts: Vec::new(),
        };
        let p = [2.0, 0.0, 0.0];
        // Distance to part 0 (point at the origin, r=1): |p-0|-1 = 2-1 = 1.0
        // Distance to part 1 (point at (3,0,0), r=1): |p-(3,0,0)|-1 = 1-1 = 0.0
        let (d, winning_index) = sdf_eval_part(&scene, p);
        assert_relative_eq!(d, 0.0, epsilon = 1e-9);
        assert_eq!(winning_index, 1); // the NEARER part wins, proving a true min
        assert_relative_eq!(sdf_eval(&scene, p), 0.0, epsilon = 1e-9); // no cuts: same as the union
    }

    #[test]
    fn sdf_eval_folds_in_every_cut_via_max() {
        let scene = SdfScene {
            parts: vec![ScenePart {
                shape: SdfPart::Capsule {
                    a: [0.0, 0.0, 0.0],
                    b: [0.0, 0.0, 0.0],
                    radius_mm: 1.0,
                },
                appearance: steel(),
            }],
            cuts: vec![GroundPlane {
                point: [0.0, 0.0, 0.0],
                normal: [1.0, 0.0, 0.0],
            }],
        };
        let p = [2.0, 0.0, 0.0];
        // Union alone: |p-0|-1 = 1.0. The cut's own distance:
        // dot(p-0, (1,0,0)) = 2.0 — larger, so it governs (max).
        assert_relative_eq!(sdf_eval(&scene, p), 2.0, epsilon = 1e-9);
    }

    // ------------------------------------------------------------------
    // Review finding 7 (spec §Testing, architect F3): "numeric-gradient
    // normals unit-length and outward" — the spec-named test this branch
    // was missing entirely.
    // ------------------------------------------------------------------

    /// Central-difference gradient of `sdf_eval` at `p`, step `h` along
    /// each world axis — the numeric stand-in for the WGSL fragment
    /// shader's own gradient-based surface normal (it computes the
    /// identical central difference against the mirrored WGSL `sdf_eval`).
    fn numeric_gradient(scene: &SdfScene, p: Vec3, h: f64) -> Vec3 {
        let d = |q: Vec3| sdf_eval(scene, q);
        let axis = |i: usize| {
            let mut lo = p;
            let mut hi = p;
            lo[i] -= h;
            hi[i] += h;
            (d(hi) - d(lo)) / (2.0 * h)
        };
        [axis(0), axis(1), axis(2)]
    }

    /// `h = 2 * MARCH_EPS` (as `f64`) per the finding's prescribed step —
    /// small enough to approximate the true local gradient, large enough
    /// to stay well clear of `f64` cancellation error.
    fn gradient_step() -> f64 {
        2.0 * f64::from(MARCH_EPS)
    }

    /// Shared assertion: the numeric gradient at `p` has near-unit length
    /// (loose band — `sdf_eval`'s Helix arm is CONSERVATIVE, not exact; see
    /// `sd_helix`'s doc for its measured ~0.7%-of-wire-radius typical error,
    /// worse near steep-pitch/seam regions — so the gradient magnitude can
    /// run somewhat off 1.0 even at a true surface point, hence 0.5 as the
    /// lower band rather than requiring near-exact unit length) and points
    /// generally OUTWARD (positive dot with the analytically-known outward
    /// direction at `p` — sign alone, `outward` need not be normalized).
    fn assert_gradient_unit_and_outward(scene: &SdfScene, p: Vec3, outward: Vec3, label: &str) {
        let grad = numeric_gradient(scene, p, gradient_step());
        let mag = vlen(grad);
        assert!(
            (0.5..=1.05).contains(&mag),
            "{label}: |grad|={mag} at {p:?} (grad={grad:?}), expected in [0.5, 1.05]"
        );
        assert!(
            vdot(grad, outward) > 0.0,
            "{label}: grad={grad:?} at {p:?} does not point outward (outward={outward:?})"
        );
    }

    #[test]
    fn numeric_gradient_normals_are_unit_length_and_outward() {
        // Helix: a point on the tube surface at ring_ang=0 (pure RADIAL
        // offset from the centerline, no axial component), where the
        // outward direction is — to the loose tolerance above — the world
        // radial direction at that azimuth (the small pitch-angle tilt of
        // the true local outward normal is within the conservativeness
        // slack this test already tolerates).
        let helix = HelixParams {
            radius_mm: 10.0,
            taper_small_r: None,
            pitch_mm: 6.0,
            turns: 8.0,
            profile: Profile::Circle { radius_mm: 1.0 },
            axial_offset_mm: 0.0,
            phase_rad: 0.0,
        };
        let helix_scene = SdfScene {
            parts: vec![ScenePart {
                shape: SdfPart::Helix(helix),
                appearance: steel(),
            }],
            cuts: Vec::new(),
        };
        let phi = 3.0 * TAU + 0.7;
        let p_helix = on_surface(&helix, phi, 0.0);
        let az = phi; // phase_rad = 0.0
        assert_gradient_unit_and_outward(&helix_scene, p_helix, [az.cos(), 0.0, az.sin()], "helix");

        // Torus arc: an interior point (well clear of the sweep's end
        // caps), pure radial offset (ring_ang=0) in the LOCAL XZ plane —
        // this part's identity (tilt=0, y_rotation=0) transform makes local
        // == world, so the outward direction is the plain world-radial one.
        let torus_scene = SdfScene {
            parts: vec![ScenePart {
                shape: SdfPart::TorusArc {
                    center: [0.0, 0.0, 0.0],
                    y_rotation: 0.0,
                    tilt: 0.0,
                    major_r: 10.0,
                    minor_r: 2.0,
                    sweep: 1.5 * PI,
                },
                appearance: steel(),
            }],
            cuts: Vec::new(),
        };
        let theta = std::f64::consts::FRAC_PI_2; // interior of [0, 1.5*PI]
        let p_torus = [(10.0 + 2.0) * theta.cos(), 0.0, (10.0 + 2.0) * theta.sin()];
        assert_gradient_unit_and_outward(
            &torus_scene,
            p_torus,
            [theta.cos(), 0.0, theta.sin()],
            "torus arc",
        );

        // Capsule: a shaft point (well clear of the hemispherical end
        // caps), where the outward direction is radial from the segment's
        // axis (here the world Y axis).
        let capsule_scene = SdfScene {
            parts: vec![ScenePart {
                shape: SdfPart::Capsule {
                    a: [0.0, 0.0, 0.0],
                    b: [0.0, 20.0, 0.0],
                    radius_mm: 2.0,
                },
                appearance: steel(),
            }],
            cuts: Vec::new(),
        };
        let p_capsule = [2.0, 10.0, 0.0];
        assert_gradient_unit_and_outward(&capsule_scene, p_capsule, [1.0, 0.0, 0.0], "capsule");

        // Composed scene (the compression golden fixture: 3 Helix segments
        // + 2 ground cuts) at a point on the ACTIVE segment's surface, away
        // from any segment join or cut — `sdf_eval`'s union-then-cut
        // pipeline in full, not a single bare part.
        let d = compression_fixture();
        let composed = compression_sdf(&d);
        let r = d.mean_dia.millimeters() / 2.0;
        let active_helix = HelixParams {
            radius_mm: r,
            taper_small_r: None,
            pitch_mm: d.pitch.millimeters(),
            turns: 1.0, // unused by on_surface beyond scaling phi -> t; phi is absolute below
            profile: Profile::Circle {
                radius_mm: d.wire_dia.millimeters() / 2.0,
            },
            axial_offset_mm: d.wire_dia.millimeters(), // dead_per_end(1) * wire_mm — see helical_body_parts
            phase_rad: TAU,                            // dead_per_end(1) turns wound before it
        };
        let phi_active = TAU * 4.0 + 0.3; // well inside the active segment's sweep, away from either join
        let p_composed = on_surface(&active_helix, phi_active, 0.0);
        let az_composed = phi_active + active_helix.phase_rad;
        assert_gradient_unit_and_outward(
            &composed,
            p_composed,
            [az_composed.cos(), 0.0, az_composed.sin()],
            "composed scene (compression fixture, active segment)",
        );

        // NEAR (not on) a ground cut: just above the bottom cut plane
        // (y=0), at the flattened dead coil's own azimuth/radius — deep
        // inside the wire tube's OWN distance (very negative), so
        // `cut_plane`'s `max` combinator is governed by the PLANE's
        // distance instead, and the outward direction is exactly the
        // plane's own normal (downward, away from the kept material above
        // it).
        let p_near_cut = [r, 0.05, 0.0];
        assert_gradient_unit_and_outward(
            &composed,
            p_near_cut,
            [0.0, -1.0, 0.0],
            "near a ground cut (compression fixture)",
        );
    }

    #[test]
    fn scene_extent_mm_is_none_for_the_default_empty_scene() {
        assert_eq!(scene_extent_mm(&SdfScene::default()), None);
    }

    #[test]
    fn scene_extent_mm_is_some_positive_finite_for_a_populated_scene() {
        let scene = SdfScene {
            parts: vec![ScenePart {
                shape: SdfPart::Capsule {
                    a: [0.0, 0.0, 0.0],
                    b: [0.0, 50.0, 0.0],
                    radius_mm: 2.0,
                },
                appearance: steel(),
            }],
            cuts: Vec::new(),
        };
        let (extent, y_mid) = scene_extent_mm(&scene).expect("a populated scene has finite extent");
        assert!(extent > 0.0 && extent.is_finite());
        assert!(y_mid.is_finite());
        // A symmetric capsule from y=0 to y=50 (no radial dominance: radial
        // reach is just the 2mm capsule radius) has its true midpoint at 25.
        assert_relative_eq!(y_mid, 25.0, max_relative = 1e-9);
    }

    #[test]
    fn scene_extent_mm_pads_the_helix_y_range_by_the_wire_radius() {
        // Review finding 4 (second clause): the doc calls the per-part bound
        // "conservative" but the Helix arm only ever measured the CENTERLINE
        // y-range, silently under-covering the terminal cap's up-to-`wire_r`
        // overhang at each open end. A single untapered Helix, wire_r=1.5,
        // pitch*turns=48 (centerline span [0, 48]): the true (padded) y-span
        // is [−1.5, 49.5] = 51mm, 3mm more than the centerline-only 48mm.
        let scene = SdfScene {
            parts: vec![ScenePart {
                shape: SdfPart::Helix(HelixParams {
                    radius_mm: 5.0, // small, so the y-span (not 2*radial) governs extent
                    taper_small_r: None,
                    pitch_mm: 6.0,
                    turns: 8.0,
                    profile: Profile::Circle { radius_mm: 1.5 },
                    axial_offset_mm: 0.0,
                    phase_rad: 0.0,
                }),
                appearance: steel(),
            }],
            cuts: Vec::new(),
        };
        let (extent, y_mid) = scene_extent_mm(&scene).expect("populated scene");
        assert_relative_eq!(extent, 51.0, max_relative = 1e-9);
        // A single segment pads symmetrically at both ends, so the midpoint
        // is unaffected by the padding itself (24.0, same as the unpadded
        // centerline midpoint) — only `extent` grows.
        assert_relative_eq!(y_mid, 24.0, max_relative = 1e-9);
    }

    #[test]
    fn scene_extent_mm_reports_the_true_asymmetric_y_midpoint_for_the_extension_hook_scene() {
        // Review finding 4 (worked fixture): the extension body's bottom hook
        // dips below y=0 (the body's own centerline start) while the top
        // hook's overhang past the body's end is generally a DIFFERENT
        // amount (different hook radius) — so the scene's true y-midpoint is
        // NOT `extent/2`, the value every caller silently assumed before
        // this fix. This is the regression the golden fixture exposes: a
        // camera built from `(extent, extent/2)` instead of the ACTUAL
        // `(extent, y_mid)` this function now returns would wrongly center
        // the fitted view.
        let d = extension_fixture();
        let scene = extension_sdf(&d);
        let (extent, y_mid) = scene_extent_mm(&scene).expect("extension scene has finite extent");
        assert!(
            (y_mid - extent / 2.0).abs() > 0.5,
            "extent={extent} y_mid={y_mid}: the asymmetric hook geometry must shift the true \
             midpoint measurably away from extent/2, or this fixture doesn't exercise the bug"
        );
    }

    #[test]
    fn extension_hook_scene_camera_contains_all_six_true_bounding_sphere_poles() {
        // Review finding 4 (worked fixture, end-to-end): feed the REAL
        // extension fixture's `(extent, y_mid)` straight into
        // `camera_uniforms` and confirm the true (y_min-aware) bounding
        // sphere's six world-axis poles all project inside NDC — the
        // concrete case the general parametric frustum test in
        // `viz::tests` exercises abstractly (an arbitrary off-center
        // `y_mid`), now pinned against the actual asymmetric hook geometry
        // that motivated the fix.
        let d = extension_fixture();
        let scene = extension_sdf(&d);
        let (extent, y_mid) = scene_extent_mm(&scene).expect("extension scene has finite extent");
        let center = [0.0, y_mid, 0.0];
        let true_radius = extent / std::f64::consts::SQRT_2;
        let orbit = crate::viz::Orbit {
            yaw: 0.6,
            pitch: -0.25,
        };
        for aspect in [1.777_f32, 0.5625_f32] {
            let camera = crate::viz::camera_uniforms(extent, y_mid, orbit, 1.0, aspect)
                .expect("finite, sane inputs always produce a camera");
            let view_proj: [f64; 16] = std::array::from_fn(|i| f64::from(camera[i]));
            // Column-major 4x4 * homogeneous vec4, matching camera_uniforms's
            // documented WGSL-mirroring layout.
            let apply = |m: &[f64; 16], v: [f64; 4]| -> [f64; 4] {
                std::array::from_fn(|row| (0..4).map(|col| m[col * 4 + row] * v[col]).sum::<f64>())
            };
            for axis in 0..3 {
                for sign in [-1.0, 1.0] {
                    let mut pole = center;
                    pole[axis] += sign * true_radius;
                    let clip = apply(&view_proj, [pole[0], pole[1], pole[2], 1.0]);
                    assert!(
                        clip[3] > 0.0,
                        "aspect {aspect}: pole {pole:?} behind the camera"
                    );
                    let ndc_x = clip[0] / clip[3];
                    let ndc_y = clip[1] / clip[3];
                    assert!(
                        ndc_x.abs() <= 1.0 + 1e-6 && ndc_y.abs() <= 1.0 + 1e-6,
                        "aspect {aspect}: pole {pole:?} outside NDC (x={ndc_x} y={ndc_y})"
                    );
                }
            }
        }
    }

    /// Solve the standard compression geometry (wire 2mm, mean 20mm, free
    /// 60mm, loads 10/30N) for any end type / active count — the golden
    /// fixture below plus the F2 seam fixtures (PlainGround's half-integer
    /// dead coils, fractional active) parameterize this.
    fn solved_compression(end_type: springcore::EndType, active: f64) -> springcore::SpringDesign {
        use springcore::units::{Force, Length};
        use springcore::{EndFixity, MaterialSet, PowerUser, Scenario};
        let m = MaterialSet::load_default()
            .get("Music Wire")
            .unwrap()
            .clone();
        PowerUser {
            end_type,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            active,
            free_length: Length::from_millimeters(60.0),
            loads: vec![Force::from_newtons(10.0), Force::from_newtons(30.0)],
        }
        .solve(&m, springcore::CurvatureCorrection::Bergstrasser)
        .unwrap()
    }

    /// Golden fixture mirrored from `compression::scene_model`'s own test
    /// (wire 2mm, mean 20mm, active 10 coils, free 60mm, SquaredGround).
    fn compression_fixture() -> springcore::SpringDesign {
        solved_compression(springcore::EndType::SquaredGround, 10.0)
    }

    #[test]
    fn compression_sdf_part_and_cut_counts_reflect_dead_coil_flattening() {
        let d = compression_fixture();
        let scene = compression_sdf(&d);
        // dead_per_end = (total-active)/2 = (12-10)/2 = 1 (a nonzero, INTEGER
        // dead coil per end for SquaredGround) — coil_height_fn's piecewise
        // dead/active/dead height cannot be expressed by ONE HelixParams
        // (single pitch_mm), so this is 3 Helix segments, not the naive "1"
        // a first glance at the design suggests (see helical_body_parts's
        // doc; flagged against task-3-brief's "compression: 1 helix + cuts"
        // wording in the task report).
        assert_eq!(scene.parts.len(), 3);
        assert_eq!(scene.cuts.len(), 2); // one ground plane per end (SquaredGround)
    }

    #[test]
    fn compression_sdf_centerline_agrees_with_the_wireframe_scene() {
        use crate::compression::scene_model::compression_scene;
        let d = compression_fixture();
        let scene = compression_sdf(&d);
        let wire_r = d.wire_dia.millimeters() / 2.0;
        let wireframe = compression_scene(&d);
        let mut excluded = 0usize;
        for p in &wireframe.polylines[0].points {
            if within_ground_cut_reach(&scene, *p, wire_r) {
                excluded += 1;
                continue; // see within_ground_cut_reach's doc: the ground cut sliver
            }
            let dist = sdf_eval(&scene, [p.0, p.1, p.2]);
            assert!(
                (dist + wire_r).abs() < 0.1 * wire_r,
                "point {p:?}: sdf={dist}, expected ~{}",
                -wire_r
            );
        }
        // Review F3: pin the exclusion's EXACT size so a mispositioned plane
        // cannot silently widen its own blind spot. With the planes at the
        // true faces (y = 0 and y = H), the wire_r reach covers the OUTER
        // HALF-TURN of each flattened end coil: 16 of the 32 samples per
        // turn, at each end — 32 of the fixture's 385 points.
        assert_eq!(excluded, 32);
    }

    #[test]
    fn compression_sdf_degenerate_design_yields_default() {
        let mut d = compression_fixture();
        d.mean_dia = springcore::units::Length::from_millimeters(f64::NAN);
        assert_eq!(compression_sdf(&d), SdfScene::default());
    }

    #[test]
    fn compression_sdf_zero_pitch_yields_default() {
        // Review finding 6 (input-domain F-C): `sd_helix`'s `k_est =
        // (axial - s*theta) / pitch_mm` divides by pitch — at pitch=0 this
        // is NaN, and `best.min(NaN)` (Rust `f64::min`'s NaN-ignoring
        // semantics) silently drops every ring-plane candidate, leaving only
        // the terminal caps to report distance — a helix that over-reports
        // by ~20mm INSIDE material at the probed brief geometry. Unreachable
        // via the current builders (audited — the solver never lands
        // exactly on pitch=0), but the hostility guard didn't encode it, and
        // rectangular wire (coming next) reuses this same gate. `pitch_mm`
        // is a real, independent parameter here (unlike extension/torsion's
        // close-wound `pitch_mm = wire`, always positive by construction),
        // so the guard belongs in THIS builder.
        let mut d = compression_fixture();
        d.pitch = springcore::units::Length::from_millimeters(0.0);
        assert_eq!(compression_sdf(&d), SdfScene::default());
    }

    #[test]
    fn compression_sdf_scene_extent_keeps_the_true_y_midpoint_near_extent_over_two() {
        // Regression (review finding 4): compression's coil body pads
        // symmetrically at both ends (same wire radius throughout), so its
        // TRUE y-midpoint stays at the unpadded centerline's middle
        // (`total_height / 2`) regardless of the wire-radius padding fix.
        // When the y-span governs `extent` (as it does for this fixture),
        // that padding adds a full `wire_r` to `extent` on top (`y_max -
        // y_min` grows by `2 * wire_r`, `extent / 2` by `wire_r`) WITHOUT
        // moving `y_mid` at all — so `y_mid` and `extent / 2` differ by
        // exactly `wire_r` here, not zero. That's still a tiny, harmless
        // offset (well inside the camera's fit slack) — utterly unlike the
        // extension hook scene's double-digit-millimeter divergence from
        // its ASYMMETRIC (r1 != r2) hook radii. This pins that the
        // symmetric case stays near-centered, not exactly at `extent/2`.
        let d = compression_fixture();
        let scene = compression_sdf(&d);
        let (extent, y_mid) = scene_extent_mm(&scene).expect("compression scene has finite extent");
        let wire_r = d.wire_dia.millimeters() / 2.0;
        assert_relative_eq!(y_mid, extent / 2.0 - wire_r, max_relative = 1e-9);
    }

    #[test]
    fn compression_sdf_capped_coils_yield_default() {
        let mut d = compression_fixture();
        d.active_coils = 2001.0;
        d.total_coils = 2003.0;
        assert_eq!(compression_sdf(&d), SdfScene::default());
    }

    #[test]
    fn ground_cut_flattens_material_below_the_squared_ground_end() {
        let d = compression_fixture();
        let scene = compression_sdf(&d);
        let r = d.mean_dia.millimeters() / 2.0;
        let p = [r, -2.0, 0.0]; // near the terminal cap, but below the ground face
        let (base_d, _) = sdf_eval_part(&scene, p);
        // Review F1: the bottom face sits at y = 0 (the centerline's own
        // start — grinding shaves the material's [-wire_r, 0) overhang), so
        // the plane's signed distance is dot(p - [0,0,0], [0,-1,0]) = -p[1].
        let plane_distance = -p[1];
        let full = sdf_eval(&scene, p);
        assert!(full >= plane_distance - 1e-9);
        assert_relative_eq!(full, plane_distance, max_relative = 1e-9);
        assert!(
            full > base_d + 1e-6,
            "the ground cut must push the reported distance OUTWARD beyond the \
             raw wire surface (base={base_d}, cut-adjusted={full}) — otherwise \
             the flattening has no effect"
        );
    }

    #[test]
    fn ground_cuts_match_planes_derived_from_engine_fields() {
        // Review F3: `within_ground_cut_reach` reads `scene.cuts` — the
        // implementation's OWN output — so a mispositioned plane would
        // auto-exclude its own damage from the centerline checks (exactly
        // what let the F1 inset slip through review). This pins BOTH planes
        // (position AND normal — the top plane was previously entirely
        // unpinned) against values derived here, in-test, from ENGINE fields
        // only. F1's geometry: the centerline spans [0, H] (H = dead coils
        // at wire pitch + active at solved pitch = the SquaredGround free
        // length); the un-ground material spans [-d/2, H + d/2]; grinding
        // removes d/2 of material per end, leaving faces at y = 0 and y = H
        // (face-to-face = L0; at solid the same faces reproduce Ls = d·Nt).
        let d = compression_fixture();
        let scene = compression_sdf(&d);
        let wire = d.wire_dia.millimeters();
        let dead = d.total_coils - d.active_coils;
        let top_face = dead * wire + d.active_coils * d.pitch.millimeters();
        assert_eq!(scene.cuts.len(), 2);
        let bottom = &scene.cuts[0];
        assert_eq!(bottom.normal, [0.0, -1.0, 0.0]);
        assert_relative_eq!(bottom.point[0], 0.0, epsilon = 1e-9);
        assert_relative_eq!(bottom.point[1], 0.0, epsilon = 1e-9);
        assert_relative_eq!(bottom.point[2], 0.0, epsilon = 1e-9);
        let top = &scene.cuts[1];
        assert_eq!(top.normal, [0.0, 1.0, 0.0]);
        assert_relative_eq!(top.point[0], 0.0, epsilon = 1e-9);
        assert_relative_eq!(top.point[1], top_face, epsilon = 1e-9);
        assert_relative_eq!(top.point[2], 0.0, epsilon = 1e-9);
    }

    /// Sample the continuous wireframe centerline in a ±0.2-turn
    /// neighborhood of `join_turn` (a body-segment boundary) and assert the
    /// SDF reads ≈ -wire_r there — the review-F2 seam probe: a wrongly
    /// phased segment leaves the true centerline empty on its side of the join
    /// (its coil restarts azimuthally elsewhere), so the SDF reads far
    /// above -wire_r. Points within a ground cut's reach are skipped (same
    /// contract as the centerline-agreement tests); returns how many points
    /// were actually checked so callers can assert the exclusion did not
    /// swallow the probe.
    fn checked_seamless_join_points(
        scene: &SdfScene,
        join_turn: f64,
        coil_r: f64,
        total: f64,
        height: &dyn Fn(f64) -> f64,
        wire_r: f64,
    ) -> usize {
        let mut checked = 0usize;
        for delta in [-0.2, -0.15, -0.1, -0.05, 0.05, 0.1, 0.15, 0.2] {
            let turn = join_turn + delta;
            let ang = turn * TAU;
            let p = (coil_r * ang.cos(), height(turn / total), coil_r * ang.sin());
            if within_ground_cut_reach(scene, p, wire_r) {
                continue;
            }
            checked += 1;
            let dist = sdf_eval(scene, [p.0, p.1, p.2]);
            assert!(
                (dist + wire_r).abs() < 0.1 * wire_r,
                "azimuthal seam at join turn {join_turn} (delta {delta}): \
                 sdf={dist}, expected ~{}",
                -wire_r
            );
        }
        checked
    }

    #[test]
    fn plain_ground_segment_joins_have_no_azimuthal_seam() {
        // Review F2 (UI-reachable): PlainGround — a pick-list option — has
        // dead_per_end = 0.5, so the active segment starts half a turn in:
        // at azimuth π, exactly where the dead segment ends. A phase-less
        // reconstruction restarted it at azimuth 0 (two dangling wire ends
        // + interpenetrating coils). The true continuous centerline across
        // BOTH joins must read ≈ -wire_r.
        let d = solved_compression(springcore::EndType::PlainGround, 10.0);
        let scene = compression_sdf(&d);
        let wire = d.wire_dia.millimeters();
        let wire_r = wire / 2.0;
        let coil_r = d.mean_dia.millimeters() / 2.0;
        let (active, total) = (d.active_coils, d.total_coils);
        let dead_per_end = (total - active) / 2.0;
        assert_relative_eq!(dead_per_end, 0.5, epsilon = 1e-12); // the case under test
        let height = crate::viz::coil_height_fn(active, total, d.pitch.millimeters(), wire);
        let mut checked = 0usize;
        for join in [dead_per_end, dead_per_end + active] {
            checked += checked_seamless_join_points(&scene, join, coil_r, total, &height, wire_r);
        }
        // The ground-cut exclusion swallows the face-side half of each
        // neighborhood; the 4 body-side points per join must remain.
        assert!(
            checked >= 8,
            "exclusion swallowed the seam probes: {checked}"
        );
    }

    #[test]
    fn fractional_active_squared_top_join_has_no_azimuthal_seam() {
        // Review F2: a fractional active count ("7.5" is valid form input)
        // makes the TOP dead segment start at azimuth frac(active)·TAU even
        // for Squared ends (integer dead_per_end = 1) — the phase-less
        // reconstruction seamed it at azimuth 0.
        let d = solved_compression(springcore::EndType::Squared, 7.5);
        let scene = compression_sdf(&d);
        let wire = d.wire_dia.millimeters();
        let wire_r = wire / 2.0;
        let coil_r = d.mean_dia.millimeters() / 2.0;
        let (active, total) = (d.active_coils, d.total_coils);
        let dead_per_end = (total - active) / 2.0;
        let height = crate::viz::coil_height_fn(active, total, d.pitch.millimeters(), wire);
        let top_join = dead_per_end + active; // 8.5 turns: azimuth π, not 0
        let checked =
            checked_seamless_join_points(&scene, top_join, coil_r, total, &height, wire_r);
        assert_eq!(checked, 8); // Squared has no cuts — nothing may be skipped
    }

    /// Golden fixture mirrored from `conical::scene_model`'s own test (wire
    /// 2mm, large 20mm, small 12mm, active 10 coils, free 60mm,
    /// squared_ground).
    fn conical_fixture() -> springcore::conical::ConicalDesign {
        use springcore::{CurvatureCorrection, MaterialSet, MaterialStore, UnitSystem};
        let materials = MaterialStore::new(MaterialSet::load_default());
        let form = crate::conical::form::ConFormState {
            end_type: "squared_ground".into(),
            wire_dia: "2".into(),
            large_mean_dia: "20".into(),
            small_mean_dia: "12".into(),
            active: "10".into(),
            free_length: "60".into(),
            loads: "10, 25".into(),
        };
        crate::conical::form::parse_and_solve(
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
    fn conical_sdf_part_and_cut_counts_reflect_dead_coil_flattening() {
        let d = conical_fixture();
        let scene = conical_sdf(&d);
        // Same piecewise-flattening reconstruction as compression_sdf (see
        // that test's comment): 3 Helix segments, 2 ground cuts.
        assert_eq!(scene.parts.len(), 3);
        assert_eq!(scene.cuts.len(), 2);
    }

    #[test]
    fn conical_sdf_centerline_agrees_with_the_wireframe_scene() {
        use crate::conical::scene_model::conical_scene;
        let d = conical_fixture();
        let scene = conical_sdf(&d);
        let wire_r = d.inputs.wire_dia.millimeters() / 2.0;
        let wireframe = conical_scene(&d);
        let mut excluded = 0usize;
        for p in &wireframe.polylines[0].points {
            if within_ground_cut_reach(&scene, *p, wire_r) {
                excluded += 1;
                continue; // see within_ground_cut_reach's doc: the ground cut sliver
            }
            let dist = sdf_eval(&scene, [p.0, p.1, p.2]);
            assert!(
                (dist + wire_r).abs() < 0.1 * wire_r,
                "point {p:?}: sdf={dist}, expected ~{}",
                -wire_r
            );
        }
        // Review F3: same exact-exclusion pin as the compression twin (the
        // conical fixture shares its coil counts and heights): 16 points per
        // end = 32 of 385.
        assert_eq!(excluded, 32);
    }

    #[test]
    fn conical_sdf_degenerate_design_yields_default() {
        let mut d = conical_fixture();
        d.inputs.large_mean_dia = springcore::units::Length::from_millimeters(f64::NAN);
        assert_eq!(conical_sdf(&d), SdfScene::default());
    }

    #[test]
    fn conical_sdf_zero_pitch_yields_default() {
        // Review finding 6 — see `compression_sdf_zero_pitch_yields_default`
        // for the `sd_helix` mechanism; conical shares the same real,
        // independent `pitch_mm` parameter (a linear taper, not a
        // close-wound `pitch_mm = wire`).
        let mut d = conical_fixture();
        d.pitch = springcore::units::Length::from_millimeters(0.0);
        assert_eq!(conical_sdf(&d), SdfScene::default());
    }

    #[test]
    fn conical_sdf_capped_coils_yield_default() {
        let mut d = conical_fixture();
        d.inputs.active_coils = 2001.0;
        d.total_coils = 2003.0;
        assert_eq!(conical_sdf(&d), SdfScene::default());
    }

    /// Golden fixture mirrored from `extension::scene_model`'s own test
    /// (wire 2mm, mean 20mm, active 10 coils, free 100mm, Fi 5N).
    fn extension_fixture() -> springcore::extension::ExtensionDesign {
        use crate::extension::form::{parse_and_solve, ExtFormState};
        use springcore::{CurvatureCorrection, MaterialSet, MaterialStore, UnitSystem};
        let materials = MaterialStore::new(MaterialSet::load_default());
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
    fn extension_sdf_part_count_is_body_plus_two_hooks() {
        let d = extension_fixture();
        let scene = extension_sdf(&d);
        assert_eq!(scene.parts.len(), 3); // 1 helix + 2 torus-arc hooks
        assert!(scene.cuts.is_empty());
    }

    #[test]
    fn extension_sdf_centerline_agrees_with_the_wireframe_scene_including_hooks() {
        use crate::extension::scene_model::extension_scene;
        let d = extension_fixture();
        let scene = extension_sdf(&d);
        let wire_r = d.wire_dia.millimeters() / 2.0;
        let wireframe = extension_scene(&d);
        for line in &wireframe.polylines {
            for p in &line.points {
                let dist = sdf_eval(&scene, [p.0, p.1, p.2]);
                assert!(
                    (dist + wire_r).abs() < 0.1 * wire_r,
                    "point {p:?}: sdf={dist}, expected ~{}",
                    -wire_r
                );
            }
        }
    }

    #[test]
    fn extension_sdf_degenerate_design_yields_default() {
        let mut d = extension_fixture();
        d.mean_dia = springcore::units::Length::from_millimeters(f64::NAN);
        assert_eq!(extension_sdf(&d), SdfScene::default());
    }

    #[test]
    fn extension_sdf_capped_active_coils_yield_default() {
        let mut d = extension_fixture();
        d.active_coils = 2001.0;
        assert_eq!(extension_sdf(&d), SdfScene::default());
    }

    /// Golden fixture mirrored from `torsion::scene_model`'s own test (wire
    /// 2mm, mean 20mm, body 5 coils, legs 15mm/10mm).
    fn torsion_fixture() -> springcore::torsion::TorsionDesign {
        use crate::torsion::form::{parse_and_solve, TorFormState};
        use springcore::{MaterialSet, MaterialStore, UnitSystem};
        let materials = MaterialStore::new(MaterialSet::load_default());
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
    fn torsion_sdf_part_count_is_body_plus_two_legs() {
        let d = torsion_fixture();
        let scene = torsion_sdf(&d);
        assert_eq!(scene.parts.len(), 3); // 1 helix + 2 capsule legs
        assert!(scene.cuts.is_empty());
    }

    #[test]
    fn torsion_sdf_centerline_agrees_with_the_wireframe_scene_including_legs() {
        use crate::torsion::scene_model::torsion_scene;
        let d = torsion_fixture();
        let scene = torsion_sdf(&d);
        let wire_r = d.inputs.wire_dia.millimeters() / 2.0;
        let wireframe = torsion_scene(&d);
        for line in &wireframe.polylines {
            for p in &line.points {
                let dist = sdf_eval(&scene, [p.0, p.1, p.2]);
                assert!(
                    (dist + wire_r).abs() < 0.1 * wire_r,
                    "point {p:?}: sdf={dist}, expected ~{}",
                    -wire_r
                );
            }
        }
    }

    #[test]
    fn torsion_sdf_degenerate_design_yields_default() {
        let mut d = torsion_fixture();
        d.inputs.mean_dia = springcore::units::Length::from_millimeters(f64::NAN);
        assert_eq!(torsion_sdf(&d), SdfScene::default());
    }

    #[test]
    fn torsion_sdf_capped_body_coils_yield_default() {
        let mut d = torsion_fixture();
        d.inputs.body_coils = 2001.0;
        assert_eq!(torsion_sdf(&d), SdfScene::default());
    }

    /// One assembly member's form fixture at the given wire/mean diameter,
    /// active-coil count, and free length (mm strings), SquaredGround end
    /// type (`AsmMemberForm::blank`'s default) — simplifier F7, shared by
    /// every multi-member `assembly_sdf` fixture below to kill the repeated
    /// `AsmMemberForm { .., ..blank(..) }` literal.
    fn member_form(
        wire_dia: &str,
        mean_dia: &str,
        active: &str,
        free_length: &str,
    ) -> crate::assembly::form::AsmMemberForm {
        use crate::assembly::form::AsmMemberForm;
        AsmMemberForm {
            wire_dia: wire_dia.into(),
            mean_dia: mean_dia.into(),
            active: active.into(),
            free_length: free_length.into(),
            ..AsmMemberForm::blank("Music Wire")
        }
    }

    /// Two-member fixture mirrored from `assembly::scene_model`'s own test
    /// (wire 2/1.5mm, mean 20/16mm, active 10/8 coils, free 60mm each,
    /// SquaredGround by default — `AsmMemberForm::blank`'s default end type).
    fn two_member_assembly_fixture(topology: &str) -> springcore::assembly::AssemblyDesign {
        use crate::assembly::form::{parse_and_solve, AsmFormState};
        use springcore::{CurvatureCorrection, MaterialSet, MaterialStore, UnitSystem};
        let materials = MaterialStore::new(MaterialSet::load_default());
        let mut f = AsmFormState::with_default_material("Music Wire");
        f.topology = topology.to_string();
        f.loads = "10, 25".into();
        f.members[0] = member_form("2", "20", "10", "60");
        f.members.push(member_form("1.5", "16", "8", "60"));
        parse_and_solve(
            &f,
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap()
    }

    #[test]
    fn assembly_sdf_nested_has_distinct_member_appearances() {
        let d = two_member_assembly_fixture("nested");
        let scene = assembly_sdf(&d);
        // Both members default to SquaredGround (AsmMemberForm::blank), each
        // with a nonzero, INTEGER dead_per_end, so each reconstructs to 3
        // Helix segments (see helical_body_parts's doc) — 2 members x 3 = 6
        // parts, not the "N member helices" a first glance at the brief's
        // wording suggests (same piecewise-flattening reason as
        // compression_sdf; flagged in the task report).
        assert_eq!(scene.parts.len(), 6);
        assert!(scene.cuts.is_empty());
        let appearance0 = scene.parts[0].appearance;
        let appearance1 = scene.parts[3].appearance;
        assert_ne!(appearance0.base_color, appearance1.base_color);
    }

    #[test]
    fn assembly_sdf_centerline_agrees_with_the_wireframe_scene() {
        use crate::assembly::scene_model::assembly_scene;
        let d = two_member_assembly_fixture("series");
        let scene = assembly_sdf(&d);
        let wireframe = assembly_scene(&d);
        for (line, member) in wireframe.polylines.iter().zip(&d.members) {
            let wire_r = member.design.wire_dia.millimeters() / 2.0;
            for p in &line.points {
                let dist = sdf_eval(&scene, [p.0, p.1, p.2]);
                assert!(
                    (dist + wire_r).abs() < 0.1 * wire_r,
                    "point {p:?}: sdf={dist}, expected ~{}",
                    -wire_r
                );
            }
        }
    }

    #[test]
    fn assembly_sdf_empty_members_yield_default() {
        let mut d = two_member_assembly_fixture("nested");
        d.members.clear();
        assert_eq!(assembly_sdf(&d), SdfScene::default());
    }

    #[test]
    fn assembly_sdf_member_zero_pitch_yields_default() {
        // Review finding 6, sibling-family sweep: every assembly member IS
        // a `SpringDesign` (the same `coil_geom` reader `compression_sdf`
        // uses — see that function's doc), so it has the exact same real,
        // independent `pitch_mm` gap `sd_helix`'s doc documents.
        let mut d = two_member_assembly_fixture("nested");
        d.members[0].design.pitch = springcore::units::Length::from_millimeters(0.0);
        assert_eq!(assembly_sdf(&d), SdfScene::default());
    }

    #[test]
    fn assembly_sdf_capped_member_yields_default() {
        use crate::assembly::form::{parse_and_solve, AsmFormState};
        use springcore::{CurvatureCorrection, MaterialSet, MaterialStore, UnitSystem};
        let materials = MaterialStore::new(MaterialSet::load_default());
        let mut f = AsmFormState::with_default_material("Music Wire");
        f.topology = "series".to_string();
        f.loads = "10, 25".into();
        f.members[0] = member_form("2", "20", "10", "60");
        f.members.push(member_form("1.5", "16", "2001", "5000"));
        let d = parse_and_solve(
            &f,
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert_eq!(assembly_sdf(&d), SdfScene::default());
    }

    // ------------------------------------------------------------------
    // Task 4: uniform packing representability + round-trip
    // ------------------------------------------------------------------

    fn trivial_capsule_part() -> ScenePart {
        ScenePart {
            shape: SdfPart::Capsule {
                a: [0.0, 0.0, 0.0],
                b: [1.0, 0.0, 0.0],
                radius_mm: 0.5,
            },
            appearance: steel(),
        }
    }

    #[test]
    fn scene_uniforms_is_some_at_exactly_the_max_parts_and_cuts_budget() {
        let scene = SdfScene {
            parts: (0..MAX_PARTS).map(|_| trivial_capsule_part()).collect(),
            cuts: (0..MAX_CUTS)
                .map(|_| GroundPlane {
                    point: [0.0, 0.0, 0.0],
                    normal: [0.0, 1.0, 0.0],
                })
                .collect(),
        };
        assert!(scene_uniforms(&scene).is_some());
    }

    #[test]
    fn scene_uniforms_is_none_one_part_past_the_budget() {
        let scene = SdfScene {
            parts: (0..=MAX_PARTS).map(|_| trivial_capsule_part()).collect(),
            cuts: Vec::new(),
        };
        assert!(scene_uniforms(&scene).is_none());
    }

    #[test]
    fn scene_uniforms_is_none_one_cut_past_the_budget() {
        let scene = SdfScene {
            parts: vec![trivial_capsule_part()],
            cuts: (0..=MAX_CUTS)
                .map(|_| GroundPlane {
                    point: [0.0, 0.0, 0.0],
                    normal: [0.0, 1.0, 0.0],
                })
                .collect(),
        };
        assert!(scene_uniforms(&scene).is_none());
    }

    #[test]
    fn scene_uniforms_representable_for_every_single_body_family_fixture() {
        assert!(scene_uniforms(&compression_sdf(&compression_fixture())).is_some());
        assert!(scene_uniforms(&conical_sdf(&conical_fixture())).is_some());
        assert!(scene_uniforms(&extension_sdf(&extension_fixture())).is_some());
        assert!(scene_uniforms(&torsion_sdf(&torsion_fixture())).is_some());
        assert!(scene_uniforms(&assembly_sdf(&two_member_assembly_fixture("nested"))).is_some());
    }

    /// `n` identical members (mirrors `two_member_assembly_fixture`'s member
    /// 0: wire 2mm, mean 20mm, active 10 coils, free 60mm, SquaredGround —
    /// a nonzero INTEGER dead-per-end, so every member reconstructs to the
    /// full 3-`Helix`-segment budget) — used to stress `scene_uniforms`'s
    /// representability guard at a REPRESENTATIVE large member count. No
    /// real cap on assembly member count exists anywhere in the app (see
    /// `MAX_PARTS`'s doc), so "the member cap" a first glance at the task
    /// brief assumed does not exist; this fixture instead demonstrates the
    /// guard at a generously large, still entirely plausible member count.
    fn n_member_assembly_fixture(n: usize) -> springcore::assembly::AssemblyDesign {
        use crate::assembly::form::{parse_and_solve, AsmFormState};
        use springcore::{CurvatureCorrection, MaterialSet, MaterialStore, UnitSystem};
        let materials = MaterialStore::new(MaterialSet::load_default());
        let mut f = AsmFormState::with_default_material("Music Wire");
        f.topology = "nested".to_string();
        f.loads = "10, 25".into();
        f.members = (0..n).map(|_| member_form("2", "20", "10", "60")).collect();
        parse_and_solve(
            &f,
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap()
    }

    #[test]
    fn scene_uniforms_representable_for_a_generous_sixteen_member_assembly() {
        // 16 members x 3 Helix segments each = 48 = MAX_PARTS exactly.
        let d = n_member_assembly_fixture(16);
        let scene = assembly_sdf(&d);
        assert_eq!(scene.parts.len(), 48);
        assert!(scene_uniforms(&scene).is_some());
    }

    #[test]
    fn scene_uniforms_none_for_an_assembly_one_member_past_the_budget() {
        // 17 members x 3 = 51 > MAX_PARTS: gracefully degrades to the
        // wireframe render rather than truncating or panicking.
        let d = n_member_assembly_fixture(17);
        let scene = assembly_sdf(&d);
        assert_eq!(scene.parts.len(), 51);
        assert!(scene_uniforms(&scene).is_none());
    }

    /// A single-part scene with the given Helix radius — the adversary's
    /// probe geometry for the finiteness-sweep tests below (the exact field
    /// doesn't matter; `radius_mm` packs straight into a slot with no
    /// intervening arithmetic, so it's the most direct route to an
    /// overflowing `as f32` cast).
    fn huge_radius_scene(radius_mm: f64) -> SdfScene {
        SdfScene {
            parts: vec![ScenePart {
                shape: SdfPart::Helix(HelixParams {
                    radius_mm,
                    taper_small_r: None,
                    pitch_mm: 2.0,
                    turns: 4.0,
                    profile: Profile::Circle { radius_mm: 1.0 },
                    axial_offset_mm: 0.0,
                    phase_rad: 0.0,
                }),
                appearance: steel(),
            }],
            cuts: Vec::new(),
        }
    }

    #[test]
    fn scene_uniforms_representable_at_1e38_radius() {
        // Review finding 5 (input-domain F-A): 1e38 is still safely inside
        // f32's finite range (max ~3.4028e38) — the finiteness sweep must
        // NOT reject a merely-huge-but-representable value, or it would
        // start rejecting legitimate (if extreme) designs.
        assert!(scene_uniforms(&huge_radius_scene(1e38)).is_some());
    }

    #[test]
    fn scene_uniforms_none_for_geometry_that_overflows_f32_on_packing() {
        // Review finding 5 (input-domain F-A/F-B, the subset-guard-bypass
        // class): the adversary's exact probe values. `geometry_hostile`'s
        // f64 finiteness check passes all of these (they ARE finite f64s),
        // but `1e40 as f32` overflows to `inf` — without a sweep over the
        // FINAL packed buffer, that `inf` would reach the shader as a
        // "valid" (`Some`) uniform with `use_shaded = true`, rendering
        // garbage instead of the documented wireframe fallback.
        for radius in [1e40, 1e155, 1e300] {
            assert!(
                scene_uniforms(&huge_radius_scene(radius)).is_none(),
                "radius={radius} must fail the finiteness sweep"
            );
        }
    }

    #[test]
    fn scene_uniforms_round_trips_a_mixed_three_part_scene_with_cuts() {
        let scene = SdfScene {
            parts: vec![
                ScenePart {
                    shape: SdfPart::Helix(HelixParams {
                        radius_mm: 12.5,
                        taper_small_r: Some(7.25),
                        pitch_mm: 3.1,
                        turns: 6.5,
                        profile: Profile::Rectangle {
                            half_w_mm: 0.6,
                            half_h_mm: 0.4,
                        },
                        axial_offset_mm: 4.0,
                        phase_rad: 1.2,
                    }),
                    appearance: steel(),
                },
                ScenePart {
                    shape: SdfPart::TorusArc {
                        center: [3.0, 5.0, -2.0],
                        y_rotation: 0.7,
                        tilt: -0.3,
                        major_r: 4.0,
                        minor_r: 0.9,
                        sweep: 4.5,
                    },
                    appearance: member_appearance(1),
                },
                ScenePart {
                    shape: SdfPart::Capsule {
                        a: [1.0, 2.0, 3.0],
                        b: [4.0, 5.0, 6.0],
                        radius_mm: 1.1,
                    },
                    appearance: member_appearance(2),
                },
            ],
            cuts: vec![
                GroundPlane {
                    point: [0.0, 0.0, 0.0],
                    normal: [0.0, -1.0, 0.0],
                },
                GroundPlane {
                    point: [0.0, 40.0, 0.0],
                    normal: [0.0, 1.0, 0.0],
                },
            ],
        };
        let packed = scene_uniforms(&scene).unwrap();
        assert_eq!(packed.len(), SCENE_UNIFORM_FLOATS);
        let round_tripped = unpack_scene(&packed).unwrap();
        assert_eq!(round_tripped.parts.len(), 3);
        assert_eq!(round_tripped.cuts.len(), 2);

        match (&scene.parts[0].shape, &round_tripped.parts[0].shape) {
            (SdfPart::Helix(a), SdfPart::Helix(b)) => {
                assert_relative_eq!(a.radius_mm, b.radius_mm, max_relative = 1e-6);
                assert_relative_eq!(
                    a.taper_small_r.unwrap(),
                    b.taper_small_r.unwrap(),
                    max_relative = 1e-6
                );
                assert_relative_eq!(a.pitch_mm, b.pitch_mm, max_relative = 1e-6);
                assert_relative_eq!(a.turns, b.turns, max_relative = 1e-6);
                assert_relative_eq!(a.axial_offset_mm, b.axial_offset_mm, max_relative = 1e-6);
                assert_relative_eq!(a.phase_rad, b.phase_rad, max_relative = 1e-6);
                match (a.profile, b.profile) {
                    (
                        Profile::Rectangle {
                            half_w_mm: aw,
                            half_h_mm: ah,
                        },
                        Profile::Rectangle {
                            half_w_mm: bw,
                            half_h_mm: bh,
                        },
                    ) => {
                        assert_relative_eq!(aw, bw, max_relative = 1e-6);
                        assert_relative_eq!(ah, bh, max_relative = 1e-6);
                    }
                    _ => panic!("profile kind did not round-trip"),
                }
            }
            _ => panic!("part 0 kind did not round-trip"),
        }
        match (&scene.parts[1].shape, &round_tripped.parts[1].shape) {
            (
                SdfPart::TorusArc {
                    center: ca,
                    y_rotation: ya,
                    tilt: ta,
                    major_r: ma,
                    minor_r: mia,
                    sweep: sa,
                },
                SdfPart::TorusArc {
                    center: cb,
                    y_rotation: yb,
                    tilt: tb,
                    major_r: mb,
                    minor_r: mib,
                    sweep: sb,
                },
            ) => {
                for i in 0..3 {
                    assert_relative_eq!(ca[i], cb[i], max_relative = 1e-6);
                }
                assert_relative_eq!(ya, yb, max_relative = 1e-6);
                assert_relative_eq!(ta, tb, max_relative = 1e-6);
                assert_relative_eq!(ma, mb, max_relative = 1e-6);
                assert_relative_eq!(mia, mib, max_relative = 1e-6);
                assert_relative_eq!(sa, sb, max_relative = 1e-6);
            }
            _ => panic!("part 1 kind did not round-trip"),
        }
        match (&scene.parts[2].shape, &round_tripped.parts[2].shape) {
            (
                SdfPart::Capsule {
                    a: orig_end0,
                    b: orig_end1,
                    radius_mm: ra,
                },
                SdfPart::Capsule {
                    a: rt_end0,
                    b: rt_end1,
                    radius_mm: rb,
                },
            ) => {
                for i in 0..3 {
                    assert_relative_eq!(orig_end0[i], rt_end0[i], max_relative = 1e-6);
                    assert_relative_eq!(orig_end1[i], rt_end1[i], max_relative = 1e-6);
                }
                assert_relative_eq!(ra, rb, max_relative = 1e-6);
            }
            _ => panic!("part 2 kind did not round-trip"),
        }
        for (orig, rt) in scene.parts.iter().zip(&round_tripped.parts) {
            // `base_color` round-trips through pack-time linearization
            // (srgb_to_linear) and the test-only inverse (linear_to_srgb),
            // so — like every OTHER field in this test that crosses the
            // f64/f32 packed boundary — it needs an epsilon, not bit-exact
            // equality; `metallic`/`roughness` are untouched scalars and
            // stay exact.
            for i in 0..3 {
                assert_relative_eq!(
                    orig.appearance.base_color[i],
                    rt.appearance.base_color[i],
                    max_relative = 1e-4
                );
            }
            assert_eq!(orig.appearance.metallic, rt.appearance.metallic);
            assert_eq!(orig.appearance.roughness, rt.appearance.roughness);
        }
        for (orig, rt) in scene.cuts.iter().zip(&round_tripped.cuts) {
            for i in 0..3 {
                assert_relative_eq!(orig.point[i], rt.point[i], max_relative = 1e-6);
                assert_relative_eq!(orig.normal[i], rt.normal[i], max_relative = 1e-6);
            }
        }
    }

    /// F1 (review fix): the common, real-world case — a `None`-taper
    /// Helix scene must round-trip to EXACTLY `None`, not merely "some
    /// falsy-ish value". Pins the negative-sentinel decode
    /// (`NO_TAPER_SENTINEL` packed for `None`, `< 0.0` unpacked back to
    /// `None`) directly, independent of the mixed-scene round-trip test
    /// above. A mutated pack default of `0.0` instead of `-1.0` fails this
    /// test: `0.0 < 0.0` is false, so it would unpack as `Some(0.0)`
    /// instead of `None` (verified by inspection — `0.0` is not negative,
    /// so the `unpack_part` decode branch is not taken).
    #[test]
    fn scene_uniforms_round_trips_none_taper_to_none_exactly() {
        let scene = SdfScene {
            parts: vec![ScenePart {
                shape: SdfPart::Helix(HelixParams {
                    radius_mm: 10.0,
                    taper_small_r: None,
                    pitch_mm: 2.0,
                    turns: 4.0,
                    profile: Profile::Circle { radius_mm: 1.0 },
                    axial_offset_mm: 0.0,
                    phase_rad: 0.0,
                }),
                appearance: steel(),
            }],
            cuts: Vec::new(),
        };
        let packed = scene_uniforms(&scene).unwrap();
        let round_tripped = unpack_scene(&packed).unwrap();
        match &round_tripped.parts[0].shape {
            SdfPart::Helix(h) => assert_eq!(h.taper_small_r, None),
            _ => panic!("expected a Helix part"),
        }
    }

    #[test]
    fn scene_uniforms_some_nan_taper_now_fails_the_finiteness_sweep() {
        // Superseded by review finding 5's blanket finiteness sweep. A
        // directly-constructed `Some(f64::NAN)` — never produced by any real
        // family builder, which rejects non-finite geometry via
        // `geometry_hostile` upstream — packs a `NaN` float into the
        // buffer, and `scene_uniforms` now rejects ANY non-finite packed
        // float wholesale rather than letting it through to poison the
        // shader (see `scene_uniforms`'s doc: a poisoned buffer renders
        // GARBAGE, not the documented wireframe fallback — strictly worse
        // than surfacing the bug via `None`).
        let scene = SdfScene {
            parts: vec![ScenePart {
                shape: SdfPart::Helix(HelixParams {
                    radius_mm: 10.0,
                    taper_small_r: Some(f64::NAN),
                    pitch_mm: 2.0,
                    turns: 4.0,
                    profile: Profile::Circle { radius_mm: 1.0 },
                    axial_offset_mm: 0.0,
                    phase_rad: 0.0,
                }),
                appearance: steel(),
            }],
            cuts: Vec::new(),
        };
        assert!(scene_uniforms(&scene).is_none());
    }

    #[test]
    fn unpack_scene_still_decodes_a_hand_packed_nan_taper_via_ordinary_comparison() {
        // `unpack_scene`'s own decode contract, independent of
        // `scene_uniforms`'s gate above: a HAND-PACKED buffer (bypassing
        // `scene_uniforms` entirely) with a `NaN` in the taper slot still
        // decodes as `Some(NaN)`, not `None` — `NO_TAPER_SENTINEL`'s `< 0.0`
        // decode is an ORDINARY numeric comparison (false for `NaN`), not a
        // NaN-testing one, which matters because WGSL may legally
        // constant-fold `isNan()`-shaped comparisons under a finite-math
        // assumption (see `scene_uniforms`'s doc). This is a property of the
        // decode primitive itself, not of whether a NaN can reach it in
        // practice (it can't, post finding-5 — see the test above).
        let mut packed = vec![0.0f32; SCENE_UNIFORM_FLOATS];
        packed[0] = 1.0; // one part
        packed[4] = 0.0; // kind = Helix
        packed[4 + 1] = 10.0; // radius_mm
        packed[4 + 2] = f32::NAN; // taper_small_r
        packed[4 + 3] = 2.0; // pitch_mm
        packed[4 + 4] = 4.0; // turns
        packed[4 + 6] = 1.0; // profile_dim0 (radius_mm)
        let round_tripped = unpack_scene(&packed).expect("well-formed header/counts");
        match &round_tripped.parts[0].shape {
            SdfPart::Helix(h) => assert!(h.taper_small_r.is_some_and(|v| v.is_nan())),
            _ => panic!("expected a Helix part"),
        }
    }
}
