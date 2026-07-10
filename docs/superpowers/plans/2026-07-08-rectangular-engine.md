# Rectangular-Wire Compression Engine — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `springcore` engine for rectangular- (and square-) wire helical compression springs — general aspect ratio, torsion-of-rectangular-bar model — plus additive persistence and the minimal springmaker placeholder arms.

**Architecture:** A new `springcore/src/rectangular/` module (mirrors `conical/` exactly: `mod.rs` docs + `design.rs` with `RectangularInputs`/`RectangularDesign`/`solve_forward`/`evaluate_status`). The model is pure Shigley §3-14 (torsion of a rectangular bar) assembled with close-coiled helix geometry, cross-checked against the Air Force Stress Analysis Manual §1.5.4.2 square-wire formulas. Persistence gains `DesignSpec::Rectangular`; the `Family` enum gains `Rectangular`; springmaker gets placeholder arms (GUI is a later increment).

**Tech Stack:** Rust, `springcore` (pure SI), `approx` for golden tolerances, `cargo-mutants` in-diff gate.

**Precedent to read first:** `springcore/src/conical/design.rs` is the structural template for Task 1 — the guard order, `load_point` reuse, `evaluate_status` shape, and the test layout all transfer directly. `springcore/src/persistence.rs` `ConicalSpec` (line ~190) + its round-trip/reject tests (~1903) are the template for Task 2.

## Global Constraints

- springcore surface mutation-gated to **literal 0 in-diff survivors** vs origin/main (`cargo mutants --in-diff`); springmaker not gated.
- **TDD** throughout: failing test → run red → minimal code → run green → commit.
- Every source-derived formula and message carries its citation in-code (Shigley §3-14; AF Stress Manual §1.5.4.2 for the square cross-check; Timoshenko 1955 for the α/β table).
- No vendor / commercial-product names in any persisted file or doc.
- MSRV 1.88; conventional commits ending `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.
- Commit directly on `feat/rectangular-engine` — **verify `git branch --show-current` before the first commit** (a prior increment strayed onto a side branch). NEVER push/PR/panel/marker; the controller verifies and gates.

**Spec refinement (resolved in this plan):** the spec's `evaluate_status` mentioned "reuse compression's buckling," but the coil-column buckling criterion needs an `EndFixity` input that `RectangularInputs` does not carry — exactly why the conical sibling omitted buckling. This plan **omits buckling in the engine v1** (documented in `mod.rs` as a deliberate omission, conical-style: "buckling assessment needs an end-fixity input; deferred to the GUI increment"). `RectangularInputs` carries no `fixity`. No other spec deviation.

---

### Task 1: `springcore/src/rectangular/` — engine

**Files:**
- Create: `springcore/src/rectangular/mod.rs`
- Create: `springcore/src/rectangular/design.rs`
- Modify: `springcore/src/lib.rs` (add `pub mod rectangular;` + re-exports, mirroring the `conical` line)

**Interfaces:**
- Consumes (all existing): `crate::design::{load_point, LoadPoint, index_caution_labeled, DesignStatus, StatusMessage, Severity}`, `crate::end_type::EndType`, `crate::material::Material`, `crate::mechanics::spring_index`, `crate::units::{Force, Length, SpringRate, Stress}`, `crate::{CurvatureCorrection, Result, SpringError}`. `CurvatureCorrection::factor(index)` returns the Wahl/Bergsträsser factor.
- Produces: `RectangularInputs`, `RectangularDesign`, `solve_forward`, `evaluate_status`.

- [ ] **Step 1: Module skeleton + docs.** Create `mod.rs` with module docs stating the model (Shigley §3-14 torsion-of-rectangular-bar + helix geometry; Wahl/Bergsträsser curvature correction as the standard round-wire→rectangular approximation; α/β from Shigley's table linearly interpolated and clamped above b/c=10) and the deliberate omissions (buckling — needs an end-fixity input, deferred to the GUI increment; natural frequency — no cited rectangular-wire surge formula in-house). `mod.rs` body: `mod design; pub use design::{RectangularInputs, RectangularDesign, solve_forward, evaluate_status};`. Add `pub mod rectangular;` to `lib.rs` next to `pub mod conical;`. Run `cargo build -p springcore`. Commit.

- [ ] **Step 2 (TDD): α/β interpolation — write the failing test.** In `design.rs` `#[cfg(test)]`, write `rect_torsion_coeffs_pins_the_table`: assert `rect_torsion_coeffs(1.0) == (0.208, 0.141)`, `(2.0) == (0.246, 0.228)`, `(10.0) == (0.313, 0.313)` (exact, `assert_eq!` on the table rows); a between-rows midpoint `rect_torsion_coeffs(1.25)` equals the linear mean of the 1.00 and 1.50 rows `((0.208+0.231)/2, (0.141+0.196)/2)` via `assert_relative_eq!` 1e-12; `rect_torsion_coeffs(0.5)` clamps to the 1.0 row; `rect_torsion_coeffs(20.0)` clamps to the 10.0 row. Run: `cargo test -p springcore rect_torsion_coeffs` → FAIL (fn not defined).

- [ ] **Step 3: Implement `rect_torsion_coeffs`.** In `design.rs`:

```rust
/// Shigley 10th ed. §3-14 torsion-of-rectangular-bar coefficients vs the side
/// ratio b/c (b = longer side). α governs max shear (Eq. 3-40, τ₀ = T/(α·b·c²));
/// β governs angle of twist (Eq. 3-41, θ = T·l/(β·b·c³·G)). Footnoted in Shigley
/// to Timoshenko, *Strength of Materials*, Part I, 3rd ed. (1955), p. 290.
/// The b/c → ∞ limit is α = β = 1/3 (documented asymptote, not a table row).
const RECT_TORSION_TABLE: &[(f64, f64, f64)] = &[
    (1.00, 0.208, 0.141),
    (1.50, 0.231, 0.196),
    (1.75, 0.239, 0.214),
    (2.00, 0.246, 0.228),
    (2.50, 0.258, 0.249),
    (3.00, 0.267, 0.263),
    (4.00, 0.282, 0.281),
    (6.00, 0.299, 0.299),
    (8.00, 0.307, 0.307),
    (10.00, 0.313, 0.313),
];

/// Linearly interpolate (α, β) at side ratio `aspect` (b/c ≥ 1). Clamps to the
/// first row below 1.0 (unreachable — orientation guarantees b/c ≥ 1 — but
/// defensive) and to the last row above 10.0 (conservative: α, β < 1/3 ⇒ higher
/// stress, lower rate; the caller flags `aspect_clamped`).
fn rect_torsion_coeffs(aspect: f64) -> (f64, f64) {
    let first = RECT_TORSION_TABLE[0];
    let last = RECT_TORSION_TABLE[RECT_TORSION_TABLE.len() - 1];
    if aspect <= first.0 {
        return (first.1, first.2);
    }
    if aspect >= last.0 {
        return (last.1, last.2);
    }
    let hi = RECT_TORSION_TABLE
        .iter()
        .position(|&(r, _, _)| r >= aspect)
        .unwrap();
    let (r0, a0, b0) = RECT_TORSION_TABLE[hi - 1];
    let (r1, a1, b1) = RECT_TORSION_TABLE[hi];
    let t = (aspect - r0) / (r1 - r0);
    (a0 + t * (a1 - a0), b0 + t * (b1 - b0))
}
```

Run: `cargo test -p springcore rect_torsion_coeffs` → PASS. Commit.

- [ ] **Step 4 (TDD): rate — write the failing square cross-check against the helper directly.** Test the `rectangular_rate` HELPER in isolation (it exists before `solve_forward`, keeping the red→green order): `square_rate_matches_af_stress_manual` calls `rectangular_rate(G, 3mm, 3mm, 30mm, 0.141, 8.0)` and asserts it equals the AF Eq. 1-90 form `G·b⁴/(44.5·r³·n)` (r = 15mm, b = 3mm) via `assert_relative_eq!` `max_relative = 1e-3` (AF's 44.5 is a 3-sig-fig rounding of 2π/0.141 = 44.56). Use `crate::test_support::music_wire().shear_modulus` for G. Run → FAIL (fn not defined).

- [ ] **Step 5: Implement `rectangular_rate`.** In `design.rs`:

```rust
/// Rectangular-wire helical rate. Shigley §3-14 Eq. 3-41 (angle of twist of a
/// rectangular bar, θ = T·l/(β·b·c³·G)) assembled with close-coiled helix
/// geometry: torque T = P·D/2, wire length l = π·D·n, axial deflection δ = θ·D/2
/// ⟹ **k = 4·β·b·c³·G / (π·D³·n)**. For a square section (b = c = a, β = 0.141)
/// this equals the AF Stress Manual §1.5.4.2 Eq. 1-90 rate k = G·a⁴/(44.5·r³·n)
/// (2π/0.141 = 44.56 ≈ 44.5).
fn rectangular_rate(shear_modulus: Stress, b: Length, c: Length, mean_dia: Length, beta: f64, active: f64) -> SpringRate {
    let g = shear_modulus.pascals();
    let bb = b.meters();
    let cc = c.meters();
    let dm = mean_dia.meters();
    SpringRate::from_newtons_per_meter(4.0 * beta * bb * cc.powi(3) * g / (std::f64::consts::PI * dm.powi(3) * active))
}
```

Run → PASS. Commit.

- [ ] **Step 6: Add `RectangularInputs`/`RectangularDesign` structs.** Per the spec §A — `RectangularInputs { wire_axial, wire_radial, mean_dia, active_coils, free_length, end_type }`; `RectangularDesign { inputs, outer_dia, inner_dia, aspect_ratio, alpha, beta, index, rate, solid_length, total_coils, pitch, aspect_clamped, min_tensile_strength, load_points, at_solid }` — NO buckling fields. `cargo build -p springcore` (structs unused-warnings are fine until `solve_forward`; add `#[allow(dead_code)]` on the private helpers if clippy complains before Step 9). Commit.

- [ ] **Step 7a (TDD): stress helper — write the failing square cross-check against the helper directly.** Test `rect_corrected_shear_stress` in isolation: `square_stress_matches_af_eq_1_84` calls `rect_corrected_shear_stress(F=50N, D=30mm, b=3mm, c=3mm, α=0.208, factor=wahl_factor(2r/b))` and asserts it equals the AF Eq. 1-84 form `wahl_factor(m)·4.80·F·r/b³`, `m = 2r/b`, `assert_relative_eq!` 1e-3. Run → FAIL (fn not defined).

- [ ] **Step 7b: Implement `rect_corrected_shear_stress` + `rect_load_point`.** (`rect_load_point` has no isolated test here — it's exercised through `solve_forward` from Step 8; its round-wire twin `load_point` is likewise only covered via solvers.)

```rust
/// Rectangular-wire corrected max shear stress. Straight-bar torsion
/// (Shigley §3-14 Eq. 3-40, τ₀ = T/(α·b·c²), T = F·D/2) times the selectable
/// curvature correction K(C), C = D/b. Square (b = c, α = 0.208): reduces to
/// AF §1.5.4.2 Eq. 1-84, K·4.80·F·r/b³ (1/0.208 = 4.808 ≈ 4.80).
fn rect_corrected_shear_stress(force: Force, mean_dia: Length, b: Length, c: Length, alpha: f64, factor: f64) -> Stress {
    let f = force.newtons();
    let dm = mean_dia.meters();
    let bb = b.meters();
    let cc = c.meters();
    Stress::from_pascals(factor * f * dm / (2.0 * alpha * bb * cc * cc))
}

/// A rectangular load point (mirrors `crate::design::load_point`, swapping the
/// round-wire stress for the rectangular one). Deflection y = F/k.
#[allow(clippy::too_many_arguments)]
fn rect_load_point(force: Force, rate: SpringRate, free_length: Length, mean_dia: Length, b: Length, c: Length, alpha: f64, index: f64, mts: Stress, correction: CurvatureCorrection) -> LoadPoint {
    let y = force.newtons() / rate.newtons_per_meter();
    let stress = rect_corrected_shear_stress(force, mean_dia, b, c, alpha, correction.factor(index));
    LoadPoint {
        force,
        deflection: Length::from_meters(y),
        length: Length::from_meters(free_length.meters() - y),
        shear_stress: stress,
        pct_mts: stress.pascals() / mts.pascals(),
    }
}
```

Run → PASS. Commit.

- [ ] **Step 8 (TDD): `solve_forward` — write the guard-matrix + orientation tests first, then implement.** Mirror `conical/design.rs::solve_forward` structure exactly. Guard order (each message pinned in a `guards_pin_messages_and_precedence` test modeled on conical's):
  1. `wire_axial` finite > 0 → `"wire axial dimension must be a positive finite number"`.
  2. `wire_radial` finite > 0 → `"wire radial dimension must be a positive finite number"`.
  3. `mean_dia` finite > 0 → `"mean diameter must be a positive finite number"`; then `mean_dia > b` (b = larger side) → `"mean diameter must exceed the larger wire dimension (spring index must exceed 1)"`.
  4. `active_coils` finite > 0 → `"active coils must be a positive finite number"` (verbatim from compression).
  5. `mts = material.min_tensile_strength(...)?` on the LARGER side (governing manufacturability) — placed before geometry-derived checks (conical's `DiameterOutOfRange`-precedence).
  6. Compute `b = max(axial,radial)`, `c = min(...)`, `aspect = b/c`, `(alpha, beta) = rect_torsion_coeffs(aspect)`, `aspect_clamped = aspect > 10.0`, `index = D/b`, `rate`, `total_coils = end_type.total_coils(active)`, `solid_length = end_type.solid_length(<axial>, active)` — **note: solid length uses the AXIAL wire dimension, not `b`** (add an `EndType` note; if `solid_length` hardcodes a single wire dim, pass `wire_axial`).
  7. `free_length` finite > 0 → `"free length must be a positive finite number"`; then `≥ solid_length` → `"free length must be at least the solid length"` (verbatim from compression).
  8. loads finite ≥ 0 → `"loads must be finite and non-negative"`.
  9. Build `load_points` via `rect_load_point`, `at_solid` at `F = k·(L0 − Ls)`.
  10. Output guard (verbatim conical shape, `at_solid.deflection` included): `"rectangular solve produced a non-finite result (inputs exceed the representable range)"`.
  Also write `orientation_swap_is_invariant`: swapping `wire_axial`↔`wire_radial` leaves `aspect_ratio`, `rate`, `load_points[0].shear_stress` identical (1e-12) but flips `solid_length` and `outer_dia`/`inner_dia`. And `mean_must_exceed_larger_side` reachability. Run the new tests → FAIL.

- [ ] **Step 9: Implement `solve_forward`.** Follow conical's body; `outer_dia = D + wire_radial`, `inner_dia = D − wire_radial`, `pitch = end_type.pitch_from_free_length(wire_axial, active, free_length)`. Run all Step-8 tests → PASS. Commit.

- [ ] **Step 10 (TDD): rectangular golden + provenance + correction selectability.** `rectangular_golden_b_over_c_2`: axial=4mm, radial=2mm (b/c=2, α=0.246, β=0.228), mean 40mm, hand-compute `k = 4·0.228·0.004·0.002³·G/(π·0.040³·8)` and the load-point stress `Kb(D/b)·F·D/(2·0.246·0.004·0.002²)`, assert 1e-12. `provenance_square_reproduces_af_coefficients`: assert the square case's rate coefficient recovers 44.5 (±0.1) and the stress coefficient recovers 4.80 (±0.01) from α=0.208, β=0.141. `selected_correction_governs_stress`: Wahl/Bergsträsser ratio equals `wahl_factor(C)/bergstrasser_factor(C)` (conical's pattern). Run → implement nothing new (formulas already there) → PASS. Commit.

- [ ] **Step 11 (TDD): `evaluate_status`.** Mirror conical's `evaluate_status`: `index_caution_labeled("spring index", design.index)`; per-load overstress warnings (`"load point {} stress is {:.1}% of MTS, above the allowable {:.0}%"`, verbatim); solid-stress warning (`"stress at solid is {:.1}% of MTS, above the set allowable {:.0}%"`, verbatim); aspect-clamp Info when `aspect_clamped`: `"wire aspect ratio exceeds 10:1; the torsion coefficients are clamped to the 10:1 tabulated values (conservative — the true section is stiffer and slightly less stressed)"`. Tests (conical's shape): index caution fires/not; overstress + solid warnings fire; aspect-clamp Info present only above b/c=10 (both sides of the boundary, full pinned string). Run red → implement → green. Commit.

- [ ] **Step 12 (TDD): mutation-gate killer tests.** Add the boundary/equality kills modeled on conical's `#[cfg(test)]` mutation section: `wire_axial`/`wire_radial` zero rejected; `mean == b` rejected (kills `>` → `>=`); zero load accepted; `free == solid` accepted (kills `<` → `<=`); load/solid stress exactly at allowable → no warning (kills `>` → `>=`); `empty_loads_with_overflow_dimensions_trip_the_output_guard` (the NaN regression class — `loads=&[]`, `mean_dia = 1e200`, assert the output-guard message); huge-finite-load output guard. Run → PASS.

- [ ] **Step 13: Mutation gate.** `git diff origin/main...HEAD > /tmp/rect.diff && cargo mutants --in-diff /tmp/rect.diff --package springcore`. Any survivor → add a killing test → re-run until **0 missed**. Run the full local gate (fmt, clippy `-D warnings`, doc `-D warnings`, typos, `cargo test --workspace`). Commit.

---

### Task 2: Persistence + springmaker load rejection (assembly pattern)

> **AMENDED 2026-07-09 (user decision):** Task 2 follows the **assembly placeholder pattern**
> (`d0a36fc`), not the conical one originally written here. `Family::Rectangular` is **deferred to
> the GUI increment** (it lands when the family becomes selectable, exactly as `Family::Assembly`
> did in `c5b16ef`). This increment adds only the persistence variant + a load-time rejection in
> springmaker — no picker entry, no placeholder panels, no `family.rs` change.

**Files:**
- Modify: `springcore/src/persistence.rs` (`DesignSpec::Rectangular`, `RectangularSpec`, the `SavedDesign::solve_with_material` arm)
- Modify: `springmaker/src/app.rs` (`apply_saved` rejects rectangular designs before mutating, returns `false`; `unreachable!` arm in the family match below the early reject)

**Interfaces:**
- Consumes: `RectangularSpec` (new), the existing persistence validation helpers.
- Produces: `DesignSpec::Rectangular(RectangularSpec)`.

- [ ] **Step 1 (TDD): `RectangularSpec` round-trip + reject.** In `persistence.rs` tests, model on the `ConicalSpec` set (~line 1903): a `RectangularSpec::PowerUser { end_type, wire_axial_mm, wire_radial_mm, mean_dia_mm, active, free_length_mm, loads_n }` round-trips through TOML (`family = "Rectangular"`); a non-finite field is rejected on load (existing non-finite validation treatment); an unknown `type` tag is rejected; `solve_with_material` rejects a rectangular design with the "solved via the rectangular scenario" message. Run red.

- [ ] **Step 2: Add `RectangularSpec` + `DesignSpec::Rectangular` + dispatch.** Add the `#[serde(tag = "type")]` enum (per spec §B, every field required), the `DesignSpec::Rectangular(RectangularSpec)` variant, and the `SavedDesign::solve_with_material` arm mirroring the other non-compression families: `DesignSpec::Rectangular(_) => Err(SpringError::InconsistentInputs("SavedDesign::solve handles compression designs; rectangular designs are solved via the rectangular scenario".into()))`. Wire the non-finite validation the other specs get. Run persistence tests → green. Commit.

- [ ] **Step 3 (TDD): springmaker load rejection.** The new `DesignSpec::Rectangular` breaks springmaker's exhaustive `apply_saved` match. Mirror `d0a36fc` exactly: early `matches!` reject setting `action_error = "rectangular designs are not supported by this build yet (the rectangular GUI ships in a later increment)"` and returning `false` (suppresses the caller's recompute); `unreachable!("handled above")` arm in the design match. Add a springmaker test mirroring `loading_an_assembly_design_rejects_and_preserves_form` (pre-seed differing material/unit_system, assert unchanged + error surfaced + `false` return). Run red → implement → green.

- [ ] **Step 4: Full gate.** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`, `typos`, and the in-diff mutation gate on the springcore surface (RectangularSpec + dispatch): **0 missed**. Commit.

---

## Final whole-branch review (after both tasks)

Floor 3 + **MANDATORY input-domain adversary** (guard precedence × aspect × magnitude matrix; the b/c=10 clamp boundary; the square corner; the empty-loads+overflow NaN case) + **persistence reviewer** (persisted format touched) + a **wire-format/cross-validation reviewer** that independently verifies the `RECT_TORSION_TABLE` transcription against Shigley §3-14 and the square cross-check against AF §1.5.4.2. Cycle to convergence; controller full gate; marker → push → PR for the user's merge.
