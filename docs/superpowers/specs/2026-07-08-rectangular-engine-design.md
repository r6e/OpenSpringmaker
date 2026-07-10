# Rectangular-Wire Compression Springs — Engine Design

**Status:** Draft (awaiting user review)
**Scope:** `springcore` engine for a NEW family — rectangular- (and square-)
wire helical compression springs, general aspect ratio, torsion-of-rectangular-
bar model — plus additive persistence (`DesignSpec::Rectangular`) and the
minimal springmaker placeholder arms the workspace needs to compile. The GUI
family tab is the follow-up increment with its own spec. Third variant of
roadmap sub-project 3 (compression variants; sequence: conical → assemblies →
**rectangular** → variable pitch [parked, no in-house source]).

## Decisions (settled during brainstorming)

1. **General rectangular scope** (user decision): handle any aspect ratio
   `b/c ≥ 1`, not square-only. The section is oriented `b = max(axial, radial)`,
   `c = min(axial, radial)` (Shigley's convention — `b` is the longer side), so
   `b/c ≥ 1` always. Square (`axial == radial`) is the degenerate case and its
   cross-check against the AF Stress Manual is the anchor golden.
2. **Stress = Wahl|Bergsträsser correction on the straight-bar torsion**
   (user decision): `τ = K(C)·τ₀`, `τ₀ = T/(α·b·c²)`, `T = P·D/2`, spring index
   `C = D/b`, `K` = the existing selectable `CurvatureCorrection` factor. Shigley
   §3-14 gives only straight-bar torsion; the curvature term reuses the round-
   wire correction — documented as the standard engineering approximation. For a
   square section this reproduces AF Stress Manual Eq. 1-84 exactly.
3. **Orientation-explicit inputs** (user decision): two labeled wire dimensions,
   `wire_axial` (along the coil axis → sets solid length) and `wire_radial` (in
   the radial direction → sets OD/ID spread). The torsion model sees only `b/c`;
   the geometry outputs need the orientation. Swapping the two dimensions is an
   invariant: same `b/c` ⇒ identical rate/stress, but flipped solid length/OD/ID.
4. **Standard family rigor** (user decision): engine PR (mutation-gated, goldens
   + provenance test + input-domain guards) then GUI PR, each through the full
   adversarial panel to convergence.
5. **α/β from Shigley's discrete table**: linear interpolation across the
   tabulated `b/c` points [1.00 … 10]; clamp to the `b/c = 10` entry above that
   (real wire rarely exceeds ~10:1; clamping is conservative on BOTH rate and
   stress — see §A). The `b/c → ∞` limit (α = β = 1/3) is the documented
   asymptote, not a table point we interpolate to.

## Sources

- **Rate**: Shigley 10th ed. §3-14, Eq. 3-41 (angle of twist of a rectangular
  bar, `θ = T·l / (β·b·c³·G)`) assembled with close-coiled helix geometry
  (`T = P·D/2`, wire length `l = π·D·n`, `δ = θ·D/2`):
  **`k = 4·β·b·c³·G / (π·D³·n)`**. The in-code comment carries the derivation.
- **Stress**: Shigley §3-14, Eq. 3-40 (max shear in a rectangular bar,
  `τ₀ = T/(α·b·c²)`, α from the Table) × the selectable curvature correction
  (Wahl Eq. 10-4 / Bergsträsser Eq. 10-5 — the existing `CurvatureCorrection`
  surface), at index `C = D/b`.
- **α, β vs b/c**: Shigley §3-14 Table (footnoted to Timoshenko, *Strength of
  Materials*, Part I, 3rd ed., 1955, p. 290):

  | b/c | 1.00 | 1.50 | 1.75 | 2.00 | 2.50 | 3.00 | 4.00 | 6.00 | 8.00 | 10 | ∞ |
  |-----|------|------|------|------|------|------|------|------|------|----|---|
  | α   |0.208 |0.231 |0.239 |0.246 |0.258 |0.267 |0.282 |0.299 |0.307 |0.313|0.333|
  | β   |0.141 |0.196 |0.214 |0.228 |0.249 |0.263 |0.281 |0.299 |0.307 |0.313|0.333|

- **Cross-check (the anchor)**: **Air Force Stress Analysis Manual (AFFDL,
  Oct 1986) §1.5.4.2** square-wire formulas — max shear Eq. 1-84
  `f_smax = (4.80·P·r/b³)·[(4m−1)/(4m−4) + 0.615/m]`, `m = 2r/b`, and deflection
  Eq. 1-90 `δ = 44.5·P·r³·n/(G·b⁴)`. Our assembly reproduces these for the
  square case: `1/α = 1/0.208 = 4.808 ≈ 4.80`; `2π/β = 2π/0.141 = 44.56 ≈ 44.5`;
  and `(4m−1)/(4m−4)+0.615/m` IS the Wahl round-wire correction with `C = m`.
  Two independent authoritative sources agreeing to 3 sig figs.
- **Solid length / end types, index caution band, MTS, allowables, buckling**:
  reuse the existing cited compression surfaces unchanged.

## A. Engine (`springcore/src/rectangular/`, mutation-gated 0 in-diff survivors)

`mod.rs` (module docs stating the torsion model, the Wahl-correction
approximation, and the α/β clamp) + `design.rs`. Re-exports:
`RectangularInputs`, `RectangularDesign`, `solve_forward`, `evaluate_status`.

```rust
/// Inputs for a rectangular- (or square-) wire helical compression spring.
#[derive(Debug, Clone)]
pub struct RectangularInputs {
    /// Wire dimension along the coil axis. Sets solid length (n_total × axial).
    pub wire_axial: Length,
    /// Wire dimension in the radial direction. Sets OD (D + radial) / ID (D − radial).
    pub wire_radial: Length,
    pub mean_dia: Length,
    pub active_coils: f64,
    pub free_length: Length,
    pub end_type: EndType,
}

/// A solved rectangular-wire design (torsion-of-rectangular-bar model).
#[derive(Debug, Clone)]
pub struct RectangularDesign {
    pub inputs: RectangularInputs,
    pub outer_dia: Length,   // D + wire_radial
    pub inner_dia: Length,   // D − wire_radial
    /// b/c = max(axial,radial) / min(axial,radial) ≥ 1.
    pub aspect_ratio: f64,
    /// Interpolated Shigley §3-14 coefficients at this aspect ratio.
    pub alpha: f64,          // stress   (Eq. 3-40)
    pub beta: f64,           // twist    (Eq. 3-41)
    /// Spring index C = D / b (b = larger wire side), the stress-governing index.
    pub index: f64,
    /// k = 4·β·b·c³·G / (π·D³·n).
    pub rate: SpringRate,
    /// Solid length = n_total × wire_axial (the existing EndType total-coils).
    pub solid_length: Length,
    pub total_coils: f64,
    pub pitch: Length,
    /// True when b/c exceeds 10 and the coefficients are clamped (Info status).
    pub aspect_clamped: bool,
    pub min_tensile_strength: Stress,
    pub load_points: Vec<LoadPoint>,
    pub at_solid: LoadPoint,
    // Buckling mirrors compression's fields (finalized at plan time against the
    // current compression SpringDesign buckling surface).
}

pub fn solve_forward(
    material: &Material,
    inputs: &RectangularInputs,
    loads: &[Force],
    correction: CurvatureCorrection,
) -> Result<RectangularDesign>
```

`LoadPoint` is REUSED from `crate::design` (force, deflection, length, stress,
pct_mts). The per-load stress is `K(C)·F·D / (2·α·b·c²)` — a new rectangular
shear helper `rect_corrected_shear_stress(F, D, b, c, α, K)` in `design.rs`
(the square case must equal AF Eq. 1-84). `at_solid` = the linear extrapolation
`F = k·(free_length − solid_length)` evaluated like compression's.

**α/β interpolation** — a pure helper `rect_torsion_coeffs(aspect: f64) -> (f64, f64)`:
the Shigley Table as a `const` array of `(b/c, α, β)` rows [1.00 … 10]; linear
interpolation between adjacent rows; `aspect ≤ 1.0` → the b/c=1 row (the
`b = max, c = min` orientation guarantees `aspect ≥ 1`, so `< 1` is unreachable
but clamped defensively); `aspect ≥ 10` → the b/c=10 row with `aspect_clamped =
true`. Clamping at 10 is conservative: α(10)=0.313 < 0.333 ⇒ higher τ; β(10)=0.313
< 0.333 ⇒ lower k. The interpolation is a first-class mutation-gated unit
(pins at every table row + a between-rows midpoint + both clamp sides).

**Validation order (family precedence — geometry → derived → loads → compute →
output; each message pinned):**

1. `wire_axial` positive finite: `"wire axial dimension must be a positive finite number"`.
2. `wire_radial` positive finite: `"wire radial dimension must be a positive finite number"`.
3. `mean_dia` positive finite and exceeding the LARGER wire side (this single
   guard secures both `index C = D/b > 1` and `inner_dia = D − radial > 0`, since
   `b ≥ radial`): `"mean diameter must be a positive finite number"` /
   `"mean diameter must exceed the larger wire dimension (spring index must exceed 1)"`.
4. Active coils positive finite (compression's message shape, copied verbatim at plan time).
5. Free length positive finite and > solid length (compression's messages, verbatim).
6. Loads: each finite and ≥ 0 (compression's engine treatment mirrored).
7. **Output guard before `Ok`** (the hardening standard — and the conical/
   compression empty-loads+overflow regression class): `rate`, every load point's
   stress/deflection, and **`at_solid`'s stress AND deflection** all finite, else
   `"rectangular solve produced a non-finite result (inputs exceed the representable range)"`.
   Explicitly cover `loads = &[]` + overflow-magnitude dimensions (the NaN escape
   conical/compression both had).

**`evaluate_status`:** index caution on `C = D/b` (the shared 4–12 band; message
shape mirrors `index_caution`); per-load-point overstress warnings (%MTS,
load-point-indexed); solid-stress warning (compression's pattern); buckling
(reuse compression's criterion + message); aspect-clamp Info when
`aspect_clamped`: `"wire aspect ratio exceeds 10:1; the torsion coefficients are
clamped to the 10:1 tabulated values (conservative — the true section is stiffer
and slightly less stressed)"`.

## B. Persistence (additive; persistence reviewer on the panel)

`springcore/src/persistence.rs`:

```rust
pub enum DesignSpec {
    Compression(ScenarioSpec),
    Extension(ExtScenarioSpec),
    Torsion(TorsionSpec),
    Conical(ConicalSpec),
    Rectangular(RectangularSpec),   // NEW
}

/// Rectangular-wire scenarios (v1: direct geometry only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RectangularSpec {
    PowerUser {
        end_type: String,
        wire_axial_mm: f64,
        wire_radial_mm: f64,
        mean_dia_mm: f64,
        active: f64,
        free_length_mm: f64,
        loads_n: Vec<f64>,
    },
}
```

Same conventions as `ConicalSpec`: internally tagged, every field required (no
`serde(default)`), non-finite floats rejected on load via the existing
`reject_non_finite` treatment, `SavedDesign::solve_with_material` gains the
`Rectangular` dispatch arm. Round-trip + reject tests per the established set.

## C. springmaker placeholder arms (compile-wiring only)

Per the enum-crosses-crate lesson: the new `DesignSpec::Rectangular` variant
breaks springmaker's exhaustive matches. This increment adds the MINIMAL arms:
loading a rectangular design surfaces a clean action error
`"rectangular-wire designs are not supported by this build yet (GUI ships in the
next increment)"` — exact wording finalized at plan time against the existing
load-dispatch error path; no family tab, no form, no presenter. The GUI
increment (own spec) replaces the placeholders. `Family::Rectangular` is added
to the `Family` enum + `ALL_FAMILIES` + `Display` ("Rectangular"), mutation-gated
with exact-string tests, and wired into springmaker's four wildcard-free `Family`
matches (placeholder arms only this increment).

## D. Testing (mutation-gated)

- **Square cross-check (the anchor golden):** with `wire_axial == wire_radial`
  (square, b/c=1), the solver's rate and per-load stress MATCH the AF Stress
  Manual Eq. 1-84 / 1-90 values computed independently in-test
  (`k = G·b⁴/(44.5·r³·n)`, `τ = 4.80·(P·r/b³)·Wahl(m=2r/b)`) at
  `assert_relative_eq` ~1e-3 (the AF coefficients are 3-sig-fig rounded, so the
  tolerance reflects the source, not our arithmetic). Pins the whole assembly
  against an independent authority.
- **Rectangular hand-computed golden (b/c = 2):** rate and stress vs values hand-
  computed from Shigley α=0.246, β=0.228 at 1e-12 (our own arithmetic, exact).
- **α/β interpolation:** every table row reproduced exactly; a between-rows
  midpoint (e.g. b/c=1.25 → linear mean of the 1.00 and 1.50 rows); `aspect < 1`
  clamps to row 1; `aspect ≥ 10` clamps to row 10 with `aspect_clamped = true`.
- **Orientation invariant:** swapping `wire_axial` and `wire_radial` leaves
  `aspect_ratio`, `rate`, and every load-point stress IDENTICAL (torsion sees
  only b/c), while `solid_length`, `outer_dia`, `inner_dia` change. Pin both the
  invariance and the divergence.
- **Governing dimension:** a case where using the smaller side as `b` would
  differ measurably — confirms `b = max`, `c = min` (kills an axial↔radial or
  b↔c swap mutant in the torsion formula).
- **Correction selectability:** Wahl vs Bergsträsser produce different stresses
  at the same inputs (the compression test pattern).
- **Provenance test** (the Shigley / EN provenance-test precedent, PR #28):
  a dedicated test asserting the square case reproduces BOTH AF magic numbers
  (4.80 within 3 sig figs, 44.5 within 3 sig figs) from the Shigley α=0.208,
  β=0.141 rows — locking the source chain in code.
- **Guard matrix:** every message pinned exactly; precedence (bad axial beats
  bad radial beats bad mean beats bad ordering beats bad loads); the 1e305-load
  output-guard case; NaN/∞ inputs per guard; **empty-loads + overflow-dimension
  NaN regression** (the conical/compression class; verify `at_solid.deflection`
  is guarded).
- **Status set:** index caution, overstress at a load point, solid stress,
  buckling, aspect-clamp Info present/absent (both sides of b/c=10).
- **Persistence:** round-trip through TOML; reject non-finite fields; reject
  unknown `type`; the springmaker placeholder error surfaces on loading a
  Rectangular file.
- **Gates:** local CI-parity (fmt, clippy `-D warnings`, doc `-D warnings`, bare
  typos, workspace tests) + in-diff mutation 0 survivors; final panel — floor 3
  + MANDATORY input-domain adversary (guard precedence × aspect × magnitude
  matrix; the b/c=10 clamp boundary; the square corner) + persistence reviewer
  (persisted format touched) + a **wire-format/cross-validation reviewer** that
  independently verifies the α/β Table transcription against Shigley §3-14 and
  the square cross-check against AF §1.5.4.2.

## Global constraints

Mutation-gated literal 0 in-diff survivors vs origin/main for the springcore
surface; springmaker not gated; TDD; every source-derived formula and message
carries its citation in-code; no vendor / commercial-product names in persisted files;
MSRV 1.88; conventional commits + `Co-Authored-By: Claude Fable 5` trailer;
applications recover gracefully / every function validates its inputs / defense
in depth.

## Task shape (for the plan)

1. `springcore/src/rectangular/` — inputs/design/solve/status + the
   `rect_torsion_coeffs` interpolation + the `rect_corrected_shear_stress`
   helper + the full engine test set (square anchor, rectangular golden,
   interpolation pins, orientation invariant, provenance, guard matrix, status)
   + mutation gate.
2. Persistence (`RectangularSpec`, `DesignSpec::Rectangular`, dispatch,
   round-trip + reject tests) + `Family::Rectangular` (+ ALL_FAMILIES + Display)
   + the springmaker placeholder arms (four `Family` matches + load-dispatch
   error) + full gate.
