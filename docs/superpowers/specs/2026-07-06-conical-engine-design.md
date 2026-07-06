# Conical Compression Springs — Engine Design

**Status:** Approved
**Scope:** `springcore` engine for a NEW family — round-wire conical compression
springs, linear taper, LINEAR-RANGE model — plus additive persistence
(`DesignSpec::Conical`) and the minimal springmaker placeholder arms the
workspace needs to compile. The GUI family tab is the follow-up increment with
its own spec. First increment of roadmap sub-project 3 (compression variants;
sequence settled in brainstorming: conical → assemblies → rectangular
[source-gated] → variable pitch [parked pending a source]).

## Decisions (settled during brainstorming)

1. **New family module** (user decision): `springcore/src/conical/` with its
   own `ConicalDesign`, the extension/torsion pattern — NOT a scenario inside
   compression (would fork `SpringDesign`/status logic) and NOT a body-shape
   engine dispatch (invasive generality nothing else needs).
2. **Linear range only**: all coils active. The progressive-rate regime (the
   largest coil bottoms first and the spring stiffens) is contact-progression
   physics with no in-house citation — out of scope, documented in module docs;
   the GUI increment surfaces the limitation as a static note.
3. **Honest omissions** (each documented in module docs, none fabricated):
   natural frequency (the cylindrical surge formula does not apply to a
   tapered body; no cited replacement in-house), buckling (no cited conical
   criterion; conical springs are inherently more stable), fatigue (deferred;
   `analyze_fatigue` at the large end is the natural follow-up), telescoped
   solid height (no citable formula in-house → conservative non-telescoping
   solid length + an Info status when the geometry telescopes).
4. **Equality is legal**: `large_mean_dia == small_mean_dia` is the
   zero-taper case — the formula reduces exactly to the cylindrical rate, and
   that identity IS the golden oracle.

## Sources

- **Rate**: Shigley 10th ed., Prob. 10-29 (p. 556): coil radius
  `R(θ) = R₁ + (R₂−R₁)·θ/(2πNa)`, Castigliano →
  `k = d⁴G / [16·Na·(R₂+R₁)(R₂²+R₁²)]`. The in-code comment carries the
  derivation sketch (per-turn compliance ∝ R³; the closed-form mean of R³
  over a linear taper is `(R₁+R₂)(R₁²+R₂²)/4`).
- **Stress**: Shigley Eq. 10-2 with the selectable correction (Wahl Eq. 10-4 /
  Bergsträsser Eq. 10-5 — the existing `CurvatureCorrection` surface),
  evaluated at the LARGEST coil (`D_large`, local index `C_large = D_large/d`)
  — the governing cross-section, since torsional moment grows with R.
- **Solid length / end types**: the existing cited `EndType` helpers, applied
  conservatively (non-telescoping stack).
- Everything else (index caution band, MTS, allowables) reuses the existing
  cited compression surfaces unchanged.

## A. Engine (`springcore/src/conical/`, mutation-gated 0 in-diff survivors)

`mod.rs` (module docs stating the linear-range model + omissions) +
`design.rs`. Re-exports: `ConicalInputs`, `ConicalDesign`, `solve_forward`,
`evaluate_status`.

```rust
/// Inputs for a round-wire conical compression spring (linear taper).
#[derive(Debug, Clone)]
pub struct ConicalInputs {
    pub wire_dia: Length,
    /// Mean diameter at the large end (the governing coil for stress).
    pub large_mean_dia: Length,
    /// Mean diameter at the small end. May equal `large_mean_dia`
    /// (zero taper — the cylindrical identity case).
    pub small_mean_dia: Length,
    pub active_coils: f64,
    pub free_length: Length,
    pub end_type: EndType,
}

/// A solved conical design (linear-range model).
#[derive(Debug, Clone)]
pub struct ConicalDesign {
    pub inputs: ConicalInputs,
    pub large_outer_dia: Length,
    pub large_inner_dia: Length,
    pub small_outer_dia: Length,
    pub small_inner_dia: Length,
    /// Local spring index at each end; the large end governs stress, the
    /// small end is the manufacturability floor. Both get index cautions.
    pub index_large: f64,
    pub index_small: f64,
    /// Diametral taper per active coil: (D_large − D_small) / Na.
    pub taper_per_coil: Length,
    pub total_coils: f64,
    /// Linear-range rate, Prob. 10-29. In diameters:
    /// k = G·d⁴ / (2·Na·(D_large + D_small)·(D_large² + D_small²)).
    pub rate: SpringRate,
    /// Conservative non-telescoping solid length (existing EndType formula).
    pub solid_length: Length,
    /// True when the per-coil mean-radius step ≥ wire dia —
    /// (D_large − D_small) / (2·Na) ≥ d — i.e. coils nest and the true solid
    /// height is LOWER than `solid_length` (geometric condition; Info status).
    pub telescopes: bool,
    pub pitch: Length,
    pub min_tensile_strength: Stress,
    pub load_points: Vec<LoadPoint>,
    pub at_solid: LoadPoint,
}

pub fn solve_forward(
    material: &Material,
    inputs: &ConicalInputs,
    loads: &[Force],
    correction: CurvatureCorrection,
) -> Result<ConicalDesign>
```

`LoadPoint` is REUSED from `crate::design` (force, deflection, length,
stress at the governing coil via the existing `corrected_shear_stress(F,
D_large, d, K)`, pct_mts). `at_solid` = the linear extrapolation
`F = k·(free_length − solid_length)` evaluated like compression's.

**Validation order (family precedence — geometry → derived → loads →
compute → output; each message pinned):**

1. Wire dia positive finite — the shared message verbatim:
   `"wire diameter must be a positive finite number"`.
2. Each end's mean dia positive finite and exceeding the wire dia, with
   END-NAMED messages (two guards, small end first):
   `"small-end mean diameter must be a positive finite number"` /
   `"small-end mean diameter must exceed wire diameter (spring index must
   exceed 1)"`, and the large-end twins.
3. Ordering: `large_mean_dia ≥ small_mean_dia`, equality legal:
   `"large-end mean diameter must be at least the small-end mean diameter"`.
4. Active coils positive finite (compression's message shape, verified at
   plan time and copied verbatim).
5. Free length positive finite and > solid length (compression's messages,
   verified at plan time and copied verbatim).
6. Loads: each finite and ≥ 0 (compression's engine treatment mirrored).
7. Output guard before `Ok` (the hardening standard): rate, every load
   point's stress/deflection, and **at_solid's stress and deflection** all
   finite, else `"conical solve produced a non-finite result (inputs exceed
   the representable range)"` (the established message shape). **Final panel
   (2026-07-06) found the empty-loads NaN escape**: with `loads = &[]` and
   huge-but-finite diameters (≥ ~1e152 mm) the rate denominator overflows to
   Inf → rate = 0.0 (finite), the per-load chain is vacuous, and
   at_solid.deflection = 0/0 = NaN slipped the original guard (which omitted
   at_solid.deflection). Fixed in-branch: `at_solid.deflection.meters()`
   added to the guarded array. The compression sibling hole (no output guard
   at all) was also found and fixed in-branch (sweep-sibling standard; both
   fixes in the same commit).

**`evaluate_status`:** index caution at BOTH ends with end-labeled messages
(the shared 4–12 band; message shape mirrors `index_caution` with a
`"small-end"`/`"large-end"` prefix); per-load-point overstress warnings
(existing pattern, %MTS at the governing coil, load-point-indexed); solid
stress warning (compression's pattern); telescoping Info:
`"coils telescope (per-coil radial step ≥ wire diameter); the reported solid
length is conservative — the true solid height is lower"`.
NO linear-model status line — that is a static GUI note (Decision 2).

## B. Persistence (additive; persistence reviewer on the panel)

`springcore/src/persistence.rs`:

```rust
pub enum DesignSpec {
    Compression(ScenarioSpec),
    Extension(ExtScenarioSpec),
    Torsion(TorsionSpec),
    Conical(ConicalSpec),   // NEW
}

/// Conical scenarios (v1: direct geometry only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ConicalSpec {
    PowerUser {
        end_type: String,
        wire_dia_mm: f64,
        large_mean_dia_mm: f64,
        small_mean_dia_mm: f64,
        active: f64,
        free_length_mm: f64,
        loads_n: Vec<f64>,
    },
}
```

Same conventions as `TorsionSpec`: internally tagged, every field required
(no `serde(default)`), non-finite floats rejected on load via the existing
`reject_non_finite` treatment, `SavedDesign::solve_with_material` gains the
`Conical` dispatch arm. Round-trip + reject tests per the established set.

## C. springmaker placeholder arms (compile-wiring only)

Per the enum-crosses-crate lesson: the new `DesignSpec::Conical` variant
breaks springmaker's exhaustive matches. This increment adds the MINIMAL
arms: loading a conical design surfaces a clean action error
`"conical designs are not supported by this build yet (GUI ships in the next
increment)"` — exact wording finalized at plan time against how the existing
load-dispatch error path renders; no family tab, no form, no presenter.
The GUI increment (own spec) replaces the placeholders.

## D. Testing (mutation-gated)

- **Zero-taper identity (the golden):** with `large == small`, the conical
  solver's rate, per-load stresses/deflections, solid length, and at_solid
  MATCH the existing compression `solve_forward` on the equivalent cylinder
  (assert_relative_eq at 1e-12 — the formulas are algebraically identical
  but computed differently, so bit-equality is not required). This pins the
  integral against the independently validated cylindrical engine.
- **Integral self-consistency:** an in-test numerical integration of
  `R(θ)³` over the taper (Simpson) matches the closed-form mean
  `(R₁+R₂)(R₁²+R₂²)/4` at ~1e-12 for a non-trivial taper — pins the algebra
  the identity test can't see (it only exercises zero taper).
- **Governing-coil choice:** a tapered case where stress computed at
  `D_small` would differ measurably — the load-point stress must equal
  `corrected_shear_stress(F, D_large, d, K)` (kills a large↔small swap
  mutant).
- **Correction selectability:** Wahl vs Bergsträsser produce different
  stresses at the same inputs (the compression test pattern).
- **Telescoping flag:** both sides of the `(D_large − D_small)/(2·Na) = d`
  boundary, including exactly-at (≥ semantics pinned).
- **Guard matrix:** every message pinned exactly; precedence (bad wire beats
  bad mean beats bad ordering beats bad loads); zero-taper accepted; the
  1e305-load output-guard case; NaN/∞ inputs per guard; **empty-loads +
  overflow-diameter NaN regression** (final panel finding; also the twin
  in compression — see §A item 7).
- **Status set:** both index cautions (end-labeled), overstress at a load
  point, solid stress, telescoping Info present/absent.
- **Persistence:** round-trip through TOML; reject non-finite fields;
  reject unknown `type`; the springmaker placeholder error surfaces on
  loading a Conical file.
- **Gates:** local CI-parity (fmt, clippy `-D warnings`, doc `-D warnings`,
  bare typos, workspace tests) + in-diff mutation 0 survivors; final panel —
  floor 3 + MANDATORY input-domain adversary (guard precedence × taper ×
  magnitude matrix; the telescoping boundary; the zero-taper corner) +
  persistence reviewer (persisted format touched).

## Task shape (for the plan)

1. `springcore/src/conical/` — inputs/design/solve/status + the full engine
   test set + mutation gate.
2. Persistence (`ConicalSpec`, `DesignSpec::Conical`, dispatch, round-trip +
   reject tests) + the springmaker placeholder arms + full gate.
