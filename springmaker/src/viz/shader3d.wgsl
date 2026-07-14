// Shaded-3D ray-marching fragment shader — the WGSL MIRROR of
// springmaker/src/viz/sdf.rs, function-for-function. sdf.rs's doc comments
// are the authoritative spec for every formula below; this file must not
// drift from them (design doc's named #1 risk). Double-curly-brace
// PLACEHOLDER tokens below (MAX_PARTS, MAX_CUTS, ...) are substituted by
// `shader3d::instantiate_wgsl` from the SAME `pub(crate) const`s sdf.rs
// defines, so both sides read one shared source of truth — never re-typed
// literals.
//
// Pure mathematical constants (PI/TAU) are hardcoded directly rather than
// placeholder-substituted: there is no Rust "source of truth" instance for
// them to drift from (unlike MAX_PARTS/MARCH_EPS/etc., which are genuine
// app-tunable values sdf.rs owns).
const PI: f32 = 3.14159265358979323846;
const TAU: f32 = 6.28318530717958647692;

// Sphere-tracing "no candidate yet" sentinel — a large FINITE value (not
// true IEEE infinity: WGSL has no infinity literal, and const-evaluated
// division by zero is a shader-creation error), always overwritten by at
// least the two unconditional terminal-cap candidates in `sd_helix`, or by
// the march loop's first step. Mirrors `sd_helix`'s `best = f64::INFINITY`.
const SDF_INF: f32 = 1e20;

// -----------------------------------------------------------------------
// Shared budget/march constants — substituted from sdf.rs's own
// `pub(crate) const`s (MAX_PARTS, MAX_CUTS, FLOATS_PER_PART, FLOATS_PER_CUT,
// MARCH_MAX_STEPS, MARCH_SAFETY, MARCH_EPS). See sdf.rs's `scene_uniforms`
// doc for the exact buffer layout these size.
// -----------------------------------------------------------------------
const MAX_PARTS: u32 = {{MAX_PARTS}}u;
const MAX_CUTS: u32 = {{MAX_CUTS}}u;
const FLOATS_PER_PART: u32 = {{FLOATS_PER_PART}}u;
const FLOATS_PER_CUT: u32 = {{FLOATS_PER_CUT}}u;
const MARCH_MAX_STEPS: u32 = {{MARCH_MAX_STEPS}}u;
const MARCH_SAFETY: f32 = {{MARCH_SAFETY}};
const MARCH_EPS: f32 = {{MARCH_EPS}};
// Central-difference normal-gradient offset — brief: "normals via
// central-difference gradient (h = 2×EPS)".
const NORMAL_H: f32 = 2.0 * MARCH_EPS;
// Per-winding azimuth sub-range subdivisions in sd_helix — substituted from
// sdf.rs's HELIX_WINDING_SUBDIVISIONS (see its doc + sd_helix's per-winding
// bound section). Must match the Rust side exactly or the two SDFs drift.
const HELIX_WINDING_SUBDIVISIONS: u32 = {{HELIX_WINDING_SUBDIVISIONS}}u;

// `rem_euclid` (Rust's `f64::rem_euclid`, always non-negative for m > 0 —
// our only use is `m = TAU`) — WGSL has no builtin equivalent.
fn rem_euclid(x: f32, m: f32) -> f32 {
    return x - m * floor(x / m);
}

// -----------------------------------------------------------------------
// Primitive distance functions — mirrors sd_capsule / sd_torus_arc /
// sd_profile_2d / cut_plane in sdf.rs exactly (all proven EXACT there; see
// each Rust fn's doc for the conservativeness argument, unchanged by the
// port). NAMING NOTE: `seg`, not the two-letter b-minus-a abbreviation the
// repo's typos gate rejects (sdf.rs's own naming note, task-1 brief).
// -----------------------------------------------------------------------

fn sd_capsule(p: vec3<f32>, a: vec3<f32>, b: vec3<f32>, radius_mm: f32) -> f32 {
    let seg = b - a;
    let from_a = p - a;
    let seg_len_sq = dot(seg, seg);
    var h: f32 = 0.0;
    if (seg_len_sq > 0.0) {
        h = clamp(dot(from_a, seg) / seg_len_sq, 0.0, 1.0);
    }
    let closest = a + seg * h;
    return length(p - closest) - radius_mm;
}

fn torus_arc_endpoint(major_r: f32, angle: f32) -> vec3<f32> {
    return vec3<f32>(major_r * cos(angle), 0.0, major_r * sin(angle));
}

fn sd_torus_arc(p_local: vec3<f32>, major_r: f32, minor_r: f32, sweep: f32) -> f32 {
    let azimuth = rem_euclid(atan2(p_local.z, p_local.x), TAU);
    if (azimuth >= 0.0 && azimuth <= sweep) {
        let radial = length(vec2<f32>(p_local.x, p_local.z)) - major_r;
        return length(vec2<f32>(radial, p_local.y)) - minor_r;
    }
    let dist_to_start = length(p_local - torus_arc_endpoint(major_r, 0.0));
    let dist_to_end = length(p_local - torus_arc_endpoint(major_r, sweep));
    return min(dist_to_start, dist_to_end) - minor_r;
}

// `profile_kind`: 0.0 = Circle (dim0 = radius_mm, dim1 unused), 1.0 =
// Rectangle (dim0 = half_w_mm, dim1 = half_h_mm) — mirrors `Profile`'s two
// variants (sdf.rs `sd_profile_2d`).
fn sd_profile_2d(d_radial: f32, d_axial: f32, profile_kind: f32, dim0: f32, dim1: f32) -> f32 {
    if (profile_kind < 0.5) {
        return length(vec2<f32>(d_radial, d_axial)) - dim0;
    }
    let qx = abs(d_radial) - dim0;
    let qy = abs(d_axial) - dim1;
    let outside = length(vec2<f32>(max(qx, 0.0), max(qy, 0.0)));
    let inside = min(max(qx, qy), 0.0);
    return outside + inside;
}

fn profile_cap_radius(profile_kind: f32, dim0: f32, dim1: f32) -> f32 {
    if (profile_kind < 0.5) {
        return dim0;
    }
    return length(vec2<f32>(dim0, dim1));
}

// Mirrors `winding_distance`: folds the squared azimuth term into the
// circular-profile distance; rectangles drop it (sdf.rs doc: no rigorous
// conservativeness argument for the box reduction, not currently rendered).
fn winding_distance(d_radial: f32, d_axial: f32, azimuth_sq: f32, profile_kind: f32, dim0: f32, dim1: f32) -> f32 {
    if (profile_kind < 0.5) {
        return sqrt(d_radial * d_radial + d_axial * d_axial + azimuth_sq) - dim0;
    }
    return sd_profile_2d(d_radial, d_axial, profile_kind, dim0, dim1);
}

// Mirrors `jordan_chord_coeff` in sdf.rs: the tightest chord constant
// (1 - cos u_max)/u_max^2 valid across |u| <= u_max, computed in the
// numerically stable 0.5*sinc^2(u_max/2) form (1 - cos cancels near zero),
// guarded at u_max -> 0 (exact small-angle limit 0.5).
fn jordan_chord_coeff(u_max: f32) -> f32 {
    if (u_max < 1e-6) {
        return 0.5;
    }
    let half = u_max * 0.5;
    let sinc = sin(half) / half;
    return 0.5 * sinc * sinc;
}

fn cut_plane(d: f32, p: vec3<f32>, plane_point: vec3<f32>, plane_normal: vec3<f32>) -> f32 {
    return max(d, dot(p - plane_point, plane_normal));
}

fn rotate_y(v: vec3<f32>, angle: f32) -> vec3<f32> {
    let s = sin(angle);
    let c = cos(angle);
    return vec3<f32>(v.x * c + v.z * s, v.y, -v.x * s + v.z * c);
}

fn rotate_x(v: vec3<f32>, angle: f32) -> vec3<f32> {
    let s = sin(angle);
    let c = cos(angle);
    return vec3<f32>(v.x, v.y * c - v.z * s, v.y * s + v.z * c);
}

// -----------------------------------------------------------------------
// Helix SDF — mirrors sd_helix / candidate_window_vertex exactly, including
// every review-hardened fix documented on sd_helix in sdf.rs: the k-index
// clamp BEFORE forming phi (not a raw-phi clamp), the per-winding
// Jordan-chord contraction (not a constant cos-alpha rescale), the
// candidate_window_vertex anchor, the sub-turn terminal caps, the
// subdivided per-winding lower bound (wave-2 R2 — HELIX_WINDING_SUBDIVISIONS
// pieces, each with its own local Jordan constant and local coil radius;
// min over pieces == min over the whole winding, tightening the boundary/
// seam regime the earlier frozen-azimuth form left under-tight), and
// phase_rad threaded through both the phase-frame theta AND the terminal caps.
// -----------------------------------------------------------------------

struct HelixParams {
    radius_mm: f32,
    // Negative = None (NO_TAPER_SENTINEL's decode contract, sdf.rs): an
    // ORDINARY numeric comparison (`< 0.0`), never `isNan()` — WGSL permits
    // finite-math-only implementations that may constant-fold NaN
    // comparisons away (w3.org/TR/WGSL/#floating-point-evaluation).
    taper_small_r: f32,
    pitch_mm: f32,
    turns: f32,
    profile_kind: f32,
    dim0: f32,
    dim1: f32,
    axial_offset_mm: f32,
    phase_rad: f32,
};

// Mirrors `candidate_window_vertex` — the closed-form vertex proven (sd_helix's
// doc) to be simultaneously the coarse ring-plane objective's vertex k* and
// the Jordan-refined joint objective's true vertex k**.
fn candidate_window_vertex(radial: f32, axial: f32, theta: f32, radius_mm: f32, pitch_mm: f32, g: f32) -> f32 {
    let s = pitch_mm / TAU;
    let k_est = (axial - s * theta) / pitch_mm;
    let dr_turn = g * TAU;
    return (pitch_mm * pitch_mm * k_est + dr_turn * (radial - radius_mm - theta * g))
        / (dr_turn * dr_turn + pitch_mm * pitch_mm);
}

fn sd_helix(p: vec3<f32>, h: HelixParams) -> f32 {
    let axial = p.y - h.axial_offset_mm;
    let radial = length(vec2<f32>(p.x, p.z));
    // Phase-frame substitution (review-F2 fix): map the query azimuth into
    // the helix's own phase frame ONCE, up front — every downstream
    // expression (window vertex, covering clamp, per-winding bound) then
    // consumes `theta` exactly as the phase-0 formulation does.
    let theta = rem_euclid(atan2(p.z, p.x) - h.phase_rad, TAU);
    let max_phi = TAU * h.turns;
    let s = h.pitch_mm / TAU;
    let small_r = select(h.radius_mm, h.taper_small_r, h.taper_small_r >= 0.0);
    let g = (small_r - h.radius_mm) / max_phi;
    let r_min = min(h.radius_mm, small_r);

    let k_star = candidate_window_vertex(radial, axial, theta, h.radius_mm, h.pitch_mm, g);

    // Covering-winding range + window anchor clamp (sd_helix's doc).
    let k_cov_lo = ceil((-PI - theta) / TAU);
    let k_cov_hi = floor((max_phi + PI - theta) / TAU);
    let anchor = clamp(floor(k_star), k_cov_lo, max(k_cov_hi - 2.0, k_cov_lo));

    let planar_sq = g * g + s * s;

    var best: f32 = SDF_INF;
    for (var delta: i32 = -1; delta <= 2; delta = delta + 1) {
        let phi_c = theta + TAU * (anchor + f32(delta));
        let u_lo = max(-PI, -phi_c);
        let u_hi = min(PI, max_phi - phi_c);
        if (u_lo > u_hi) {
            continue;
        }
        let a = radial - (h.radius_mm + g * phi_c);
        let b = axial - s * phi_c;
        // Subdivide the winding's real sub-range into HELIX_WINDING_SUBDIVISIONS
        // pieces, each bounded with its OWN local Jordan chord constant and
        // local minimum coil radius; keep the min over pieces (min over a
        // partition == min over the whole range — see sd_helix's per-winding
        // bound doc in sdf.rs for the validity proof and why this subsumes the
        // old interior-Jordan + boundary frozen-azimuth max-of-two).
        let span = (u_hi - u_lo) / f32(HELIX_WINDING_SUBDIVISIONS);
        for (var i: u32 = 0u; i < HELIX_WINDING_SUBDIVISIONS; i = i + 1u) {
            let lo = u_lo + span * f32(i);
            let hi = lo + span;
            let r_local = max(min(h.radius_mm + g * (phi_c + lo), h.radius_mm + g * (phi_c + hi)), r_min);
            let chord_sq = 2.0 * radial * r_local * jordan_chord_coeff(max(abs(lo), abs(hi)));
            let denom = planar_sq + chord_sq;
            var u: f32 = clamp(0.0, lo, hi);
            if (denom > 0.0) {
                u = clamp((a * g + b * s) / denom, lo, hi);
            }
            best = min(best, winding_distance(a - g * u, b - s * u, chord_sq * u * u, h.profile_kind, h.dim0, h.dim1));
        }
    }

    // Finding-3 terminal caps: exact distance to both end centerline points,
    // at their TRUE world azimuth `phi_end + phase_rad`. Fixed 2-iteration
    // loop (phi_end in {0, max_phi}) — unconditional min-participants, so
    // they can never break the conservativeness bound.
    for (var i: i32 = 0; i < 2; i = i + 1) {
        var phi_end: f32 = 0.0;
        if (i == 1) {
            phi_end = max_phi;
        }
        let coil_r = h.radius_mm + g * phi_end;
        let az = phi_end + h.phase_rad;
        let end = vec3<f32>(coil_r * cos(az), s * phi_end + h.axial_offset_mm, coil_r * sin(az));
        best = min(best, length(p - end) - profile_cap_radius(h.profile_kind, h.dim0, h.dim1));
    }
    return best;
}

// -----------------------------------------------------------------------
// Scene composition — mirrors part_distance / sdf_eval_part / sdf_eval,
// reading the packed buffer per the FLOATS_PER_PART/FLOATS_PER_CUT layout
// table documented on sdf.rs's `scene_uniforms`. Appearance is captured
// alongside distance in the SAME per-part scan (rather than a separate
// index-based lookup pass, as sdf.rs's Rust split does) — an adaptation for
// GPU locality, not a change in the union-selection semantics: the nearest
// part still wins via the same running `<` comparison.
// -----------------------------------------------------------------------

@group(0) @binding(1) var<storage, read> scene: array<f32>;

struct PartHit {
    dist: f32,
    base_color: vec3<f32>,
    metallic: f32,
    roughness: f32,
};

// `base` is the part's slot offset (`4u + i * FLOATS_PER_PART`). Slot
// layout per kind — see sdf.rs `scene_uniforms`'s doc table:
//   Helix (kind 0):    [1] radius_mm [2] taper_small_r [3] pitch_mm
//                       [4] turns [5] profile_kind [6] dim0 [7] dim1
//                       [8] axial_offset_mm [9] phase_rad
//                       [10..13) base_color [13] metallic [14] roughness
//   TorusArc (kind 1): [1..4) center [4] y_rotation [5] tilt [6] major_r
//                       [7] minor_r [8] sweep
//                       [9..12) base_color [12] metallic [13] roughness
//   Capsule (kind 2):  [1..4) a [4..7) b [7] radius_mm
//                       [8..11) base_color [11] metallic [12] roughness
// base_color is already LINEAR here (sdf.rs's `pack_appearance` linearizes
// the sRGB-authored `Appearance::base_color` at pack time — see that fn's
// doc); this shader consumes every base_color slot as-is, no further
// conversion needed before it's used in `shade`'s linear-space lighting math.
fn part_hit_at(base: u32, p: vec3<f32>) -> PartHit {
    let kind = scene[base];
    var out: PartHit;
    if (kind < 0.5) {
        let h = HelixParams(
            scene[base + 1u],
            scene[base + 2u],
            scene[base + 3u],
            scene[base + 4u],
            scene[base + 5u],
            scene[base + 6u],
            scene[base + 7u],
            scene[base + 8u],
            scene[base + 9u],
        );
        out.dist = sd_helix(p, h);
        out.base_color = vec3<f32>(scene[base + 10u], scene[base + 11u], scene[base + 12u]);
        out.metallic = scene[base + 13u];
        out.roughness = scene[base + 14u];
    } else if (kind < 1.5) {
        let center = vec3<f32>(scene[base + 1u], scene[base + 2u], scene[base + 3u]);
        let y_rotation = scene[base + 4u];
        let tilt = scene[base + 5u];
        let major_r = scene[base + 6u];
        let minor_r = scene[base + 7u];
        let sweep = scene[base + 8u];
        let local = rotate_x(rotate_y(p - center, -y_rotation), -tilt);
        out.dist = sd_torus_arc(local, major_r, minor_r, sweep);
        out.base_color = vec3<f32>(scene[base + 9u], scene[base + 10u], scene[base + 11u]);
        out.metallic = scene[base + 12u];
        out.roughness = scene[base + 13u];
    } else {
        let a = vec3<f32>(scene[base + 1u], scene[base + 2u], scene[base + 3u]);
        let b = vec3<f32>(scene[base + 4u], scene[base + 5u], scene[base + 6u]);
        let radius_mm = scene[base + 7u];
        out.dist = sd_capsule(p, a, b, radius_mm);
        out.base_color = vec3<f32>(scene[base + 8u], scene[base + 9u], scene[base + 10u]);
        out.metallic = scene[base + 11u];
        out.roughness = scene[base + 12u];
    }
    return out;
}

// Mirrors `sdf_eval_part`: union over every part (nearest wins), reporting
// the winning appearance alongside the distance. `scene[0]` is `n_parts`.
fn scene_eval_part(p: vec3<f32>) -> PartHit {
    var best: PartHit;
    best.dist = SDF_INF;
    best.base_color = vec3<f32>(0.0, 0.0, 0.0);
    best.metallic = 0.0;
    best.roughness = 0.0;
    let n_parts = scene[0];
    for (var i: u32 = 0u; i < MAX_PARTS; i = i + 1u) {
        if (f32(i) >= n_parts) {
            break;
        }
        let hit = part_hit_at(4u + i * FLOATS_PER_PART, p);
        if (hit.dist < best.dist) {
            best = hit;
        }
    }
    return best;
}

// Mirrors `sdf_eval`: the part union, then every ground cut folded in via
// `cut_plane` (sequential max — order never matters). `scene[1]` is
// `n_cuts`; the cuts block starts right after the fixed part block.
fn scene_eval(p: vec3<f32>) -> f32 {
    var d = scene_eval_part(p).dist;
    let n_cuts = scene[1];
    let cuts_base = 4u + MAX_PARTS * FLOATS_PER_PART;
    for (var i: u32 = 0u; i < MAX_CUTS; i = i + 1u) {
        if (f32(i) >= n_cuts) {
            break;
        }
        let base = cuts_base + i * FLOATS_PER_CUT;
        let plane_point = vec3<f32>(scene[base], scene[base + 1u], scene[base + 2u]);
        let plane_normal = vec3<f32>(scene[base + 3u], scene[base + 4u], scene[base + 5u]);
        d = cut_plane(d, p, plane_point, plane_normal);
    }
    return d;
}

// -----------------------------------------------------------------------
// Camera + vertex shader — a fullscreen triangle; each fragment reconstructs
// its own world-space ray by unprojecting its NDC (x, y) at the near (z=0)
// and far (z=1) planes through the inverse view-projection matrix (no
// separately stored eye position — Task 4's `camera_uniforms` doc; both
// unprojected points lie ON the eye-to-pixel ray).
// -----------------------------------------------------------------------

struct Camera {
    // Unused by the fragment shader's own math (the ray is reconstructed
    // from `inv_view_proj` alone) but kept for byte-layout parity with the
    // 32-float `camera_uniforms` array Task 4 defines (view_proj +
    // inv_view_proj back to back).
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    bg: vec4<f32>,
    // flags.x: the runtime sRGB-encode verdict (wave-2 V1/V2) —
    // `needs_srgb_encode(format)` from SpringPipeline::new, 1.0 when the
    // render target is NOT an sRGB-format texture (its hardware stores
    // shader output raw, so fs_main must apply the OETF itself), 0.0 when
    // it IS (hardware encodes on store; encoding here too would
    // double-encode). flags.yzw: padding to a full vec4.
    flags: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;

struct VOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VOut {
    // Classic "big triangle" fullscreen trick: three vertices at NDC
    // (-1,-1), (3,-1), (-1,3) — one triangle that fully covers the
    // [-1,1]^2 NDC square after clipping, no vertex buffer needed.
    let ndc = vec2<f32>(
        f32((vertex_index << 1u) & 2u) * 2.0 - 1.0,
        f32(vertex_index & 2u) * 2.0 - 1.0,
    );
    var out: VOut;
    out.clip_position = vec4<f32>(ndc, 0.0, 1.0);
    out.ndc = ndc;
    return out;
}

fn unproject(ndc_xy: vec2<f32>, ndc_z: f32, inv_view_proj: mat4x4<f32>) -> vec3<f32> {
    let clip = vec4<f32>(ndc_xy, ndc_z, 1.0);
    let world = inv_view_proj * clip;
    return world.xyz / world.w;
}

// -----------------------------------------------------------------------
// Shading: Blinn-Phong with a camera-fixed key light, bg-luminance ambient,
// metallic-tinted specular, and 5-tap distance-field ambient occlusion.
// -----------------------------------------------------------------------

// IQ's classic 5-tap distance-field AO, rescaled x10 for this app's mm-scale
// geometry (typical wire radii ~0.25-3mm) rather than IQ's unit-scale scenes.
fn occlusion(p: vec3<f32>, n: vec3<f32>) -> f32 {
    var occ: f32 = 0.0;
    var scale: f32 = 1.0;
    for (var i: i32 = 0; i < 5; i = i + 1) {
        let fi = f32(i);
        let h = 0.1 + 0.2 * fi * fi;
        let d = scene_eval(p + n * h);
        occ = occ + (h - d) * scale;
        scale = scale * 0.7;
    }
    return clamp(1.0 - occ, 0.0, 1.0);
}

fn estimate_normal(p: vec3<f32>) -> vec3<f32> {
    let dx = vec3<f32>(NORMAL_H, 0.0, 0.0);
    let dy = vec3<f32>(0.0, NORMAL_H, 0.0);
    let dz = vec3<f32>(0.0, 0.0, NORMAL_H);
    let nx = scene_eval(p + dx) - scene_eval(p - dx);
    let ny = scene_eval(p + dy) - scene_eval(p - dy);
    let nz = scene_eval(p + dz) - scene_eval(p - dz);
    return normalize(vec3<f32>(nx, ny, nz));
}

fn shade(p: vec3<f32>, n: vec3<f32>, hit: PartHit, ray_dir: vec3<f32>) -> vec3<f32> {
    let view_dir = -ray_dir;

    // Reconstruct the camera's world-space basis from inv_view_proj (no
    // separately stored eye position, per Task 4's camera_uniforms doc): the
    // screen-center ray gives `forward`; `right`/`up` are re-derived against
    // world-up EXACTLY as `mat4_look_at` (viz/mod.rs) built them.
    let inv_vp = camera.inv_view_proj;
    let center_near = unproject(vec2<f32>(0.0, 0.0), 0.0, inv_vp);
    let center_far = unproject(vec2<f32>(0.0, 0.0), 1.0, inv_vp);
    let cam_forward = normalize(center_far - center_near);
    let world_up = vec3<f32>(0.0, 1.0, 0.0);
    let cam_right = normalize(cross(cam_forward, world_up));
    let cam_up = cross(cam_right, cam_forward);

    // Key light, fixed in CAMERA space (brief: dir normalize(-0.4, 0.7,
    // 0.6)): components read as (right, up, toward-camera) — a camera's own
    // local +Z axis in world terms is -forward (view space looks down -Z).
    let light_dir = normalize(cam_right * -0.4 + cam_up * 0.7 + (-cam_forward) * 0.6);

    let base_color = hit.base_color;
    let metallic = hit.metallic;
    let roughness = hit.roughness;

    let n_dot_l = max(dot(n, light_dir), 0.0);
    let half_dir = normalize(light_dir + view_dir);
    let shininess = mix(8.0, 128.0, 1.0 - roughness);
    let spec_strength = pow(max(dot(n, half_dir), 0.0), shininess);
    // Dielectric F0 ~0.04, lerped toward the base color as metallic -> 1
    // (brief: "metallic lerps specular color toward base_color").
    let spec_color = mix(vec3<f32>(0.04, 0.04, 0.04), base_color, metallic);

    let luminance = dot(camera.bg.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let ambient = luminance * 0.35;

    let ao = occlusion(p, n);
    let diffuse = base_color * n_dot_l * (1.0 - metallic);
    let color = (diffuse + spec_color * spec_strength + base_color * ambient) * ao;
    return color;
}

// Inverse sRGB EOTF (linear -> encoded) — mirrors sdf.rs's
// `linear_to_srgb` token-for-token (threshold 0.0031308, the linear-side
// value of the decode's 0.04045). Applied by `encode_output` at fs_main's
// exits iff `camera.flags.x` says the render target is NOT an sRGB-format
// texture (wave-2 V1/V2: on such a target the hardware stores shader
// output raw, and un-encoded linear light displays ~2.2x too dark — the
// user-reported black spring on a black background).
fn linear_to_srgb(u: f32) -> f32 {
    if (u <= 0.0031308) {
        return u * 12.92;
    }
    return 1.055 * pow(u, 1.0 / 2.4) - 0.055;
}

// The pipeline's single output-encode gate: every fs_main return value
// funnels through here so no exit path can forget the format-dependent
// encode. Alpha is coverage, not light — never gamma-encoded.
fn encode_output(color: vec4<f32>) -> vec4<f32> {
    if (camera.flags.x > 0.5) {
        return vec4<f32>(
            linear_to_srgb(color.r),
            linear_to_srgb(color.g),
            linear_to_srgb(color.b),
            color.a,
        );
    }
    return color;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let inv_vp = camera.inv_view_proj;
    let ray_origin = unproject(in.ndc, 0.0, inv_vp);
    let ray_far = unproject(in.ndc, 1.0, inv_vp);
    let ray_dir = normalize(ray_far - ray_origin);

    var hit = false;
    var p = ray_origin;
    var t: f32 = 0.0;
    // Compute far-clip distance to prevent f32 overflow on miss rays.
    let t_max = length(ray_far - ray_origin);
    for (var i: u32 = 0u; i < MARCH_MAX_STEPS; i = i + 1u) {
        p = ray_origin + ray_dir * t;
        let d = scene_eval(p);
        if (d < MARCH_EPS) {
            hit = true;
            break;
        }
        t = t + d * MARCH_SAFETY;
        // Bail if marched past far-clip boundary.
        if (t > t_max) { break; }
    }

    if (!hit) {
        return encode_output(camera.bg);
    }

    let part_hit = scene_eval_part(p);
    let normal = estimate_normal(p);
    return encode_output(vec4<f32>(shade(p, normal, part_hit, ray_dir), 1.0));
}
