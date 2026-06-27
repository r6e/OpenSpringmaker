# Helical Torsion Springs — Phase 1 Design

**Status:** Approved (design dialogue 2026-06-26)
**Scope:** `springcore` engine only (no `springmaker` GUI). One new `torsion/` module + three new shared units.
**Phase:** Torsion Phase 1 — static core + a single `PowerUser` scenario. Third spring family, after compression and extension.

## Goal

Add helical **torsion springs** (round wire) as a new spring family, mirroring the
compression and extension families. A torsion spring is loaded by a **moment** `M` and
deflects through an **angle** `θ`; the wire is stressed in **bending** (not torsional
shear). Phase 1 delivers the pure engine: new units (moment, angle, angular rate),
torsion mechanics (rate, bending stress, active coils including legs, wind-up geometry),
a static forward solver, and one fully-specified `PowerUser` scenario that takes geometry
plus an applied moment and returns rate, deflection, stresses, wound-up diameter, and an
engineering status.

## Non-goals / out of scope (later phases)

- **GUI** (`springmaker`): a torsion scenario tab, presenter, results panel, and
  persistence round-trip — a later phase, mirroring how extension shipped engine-first.
- **Additional input modes** (TwoLoad / RateBased / Dimensional analogues, and a
  force-on-a-leg-at-radius convenience input that derives `M = F·r`) — later phase.
- **Minimum-weight optimizer** for torsion — later phase.
- **Fatigue** for torsion (alternating-moment Goodman) — later phase; Phase 1 is static.
- **Rectangular wire**, double torsion springs, and pitch/helix-angle effects.

## Architecture (parallel module, mirrors `extension/`)

New `springcore/src/torsion/` module, registered from `lib.rs`, reusing the shared
`units`, `material`, `numeric`, and crate-root `DesignStatus`/`Severity` infrastructure:

```
springcore/src/torsion/
  mod.rs        — module doc + public re-exports
  mechanics.rs  — K_bi factor, inner-fiber & nominal bending stress, active_coils_with_legs,
                  angular_rate (per FrictionModel), wound_mean_diameter, FrictionModel enum
  design.rs     — TorsionInputs, TorsionLoadPoint, TorsionDesign, solve_forward, status checks
  scenario.rs   — PowerUser scenario → TorsionInputs → solve
```

Compression stays flat in `src/`; extension and now torsion are self-contained modules
(the established asymmetry — new families do not churn working compression code).

### New shared units (`springcore/src/units.rs`)

Three new `si_quantity!` types, **radian-canonical** internally (consistent with the
SI-canonical rule), each with US/shop conversions for a later GUI:

- `Moment` — SI N·m; `from_/to`: newton_meters, newton_millimeters, pound_force_inches.
- `Angle` — SI radians; `from_/to`: radians, degrees, turns (1 turn = 2π rad).
- `AngularRate` — SI N·m per radian; `from_/to`: per_radian, per_degree, per_turn
  (per-turn = per-radian × 2π).

## Mechanics and formulas (cited inline at each call site)

All symbols: `d` wire dia, `D` mean coil dia, `C = D/d` spring index, `N_b` body coils,
`L₁,L₂` straight-leg lengths, `E` Young's modulus, `M` applied moment, `θ` angular
deflection.

- **Effective active coils** (legs bend too): `Nₐ = N_b + (L₁+L₂)/(3πD)` — Shigley
  Eq. 10-50.
- **Angular rate**, per radian, selectable by `FrictionModel`:
  - `ShigleyFriction`: `k′ = E·d⁴ / (2π · 10.8 · D · Nₐ)`
    (Shigley Eq. 10-51, the 10.8-per-turn form with its empirical inter-coil
    friction allowance; ÷2π converts per-turn → per-radian).
  - `PureBending`: `k′ = E·d⁴ / (64 · D · Nₐ)`
    (energy method, EN 13906-3 form; equals the friction-free 10.2-per-turn value
    since 64/2π ≈ 10.19).
- **Deflection:** `θ = M / k′` (radians).
- **Inner-fiber bending stress (critical):** `σᵢ = K_bi · 32M/(πd³)` with the
  inner-fiber correction `K_bi = (4C² − C − 1) / (4C(C − 1))` — Shigley Eq. 10-43.
- **Nominal bending stress (reference):** `σ₀ = 32M/(πd³)` (reported alongside σᵢ).
- **Wound-up geometry** (the spring tightens as it winds): `D′ = D·N_b/(N_b + θ_turns)`,
  inner diameter `inner′ = D′ − d` — Shigley Eq. 10-49, where `θ_turns = θ/2π`.

### `FrictionModel`

```rust
/// Which angular-rate model the torsion solver uses. Selectable and (in a later GUI
/// phase) persisted, mirroring the shear-stress `CurvatureCorrection` precedent.
///
/// Deliberately NOT `#[non_exhaustive]`: `springcore` is an unpublished workspace crate,
/// and this enum will be matched in the GUI (variant → label) where exhaustiveness
/// checking is wanted so a future variant forces a compile error rather than a silent
/// fallback. (Per the PR #32 scope decision.)
pub enum FrictionModel {
    /// Shigley Eq. 10-51 with empirical inter-coil friction (10.8 per turn). Default.
    ShigleyFriction,
    /// Pure-bending energy method (EN 13906-3; 64 per radian). No friction allowance.
    PureBending,
}
```

`solve_forward` takes a `FrictionModel` parameter (as compression/extension take
`CurvatureCorrection`). Both models are golden-/oracle-tested.

## Data types and solver flow

```rust
pub struct TorsionInputs {
    pub wire_dia: Length,
    pub mean_dia: Length,        // PowerUser may give OD; scenario converts D = OD − d
    pub body_coils: f64,         // N_b
    pub leg1: Length,            // L₁
    pub leg2: Length,            // L₂
    pub arbor_dia: Option<Length>, // optional; enables the wind-up clearance check
}

pub struct TorsionLoadPoint {
    pub moment: Moment,          // applied M
    pub deflection: Angle,       // θ = M/k′
    pub stress_inner: Stress,    // σᵢ (critical)
    pub stress_nominal: Stress,  // σ₀
    pub pct_bending_allow: f64,  // σᵢ / (allowable_pct_bending · MTS)
    pub wound_mean_dia: Length,  // D′
    pub wound_inner_dia: Length, // inner′
}

pub struct TorsionDesign {
    pub inputs: TorsionInputs,
    pub index: f64,              // C
    pub active_coils: f64,       // Nₐ
    pub rate: AngularRate,       // k′
    pub load_points: Vec<TorsionLoadPoint>,
    pub status: DesignStatus,
}

pub fn solve_forward(
    material: &Material,
    inputs: TorsionInputs,
    moments: &[Moment],
    friction: FrictionModel,
) -> Result<TorsionDesign>;
```

**Flow:** validate inputs → compute `C`, `Nₐ`, `k′` → for each moment compute the load
point → run status checks → return. `PowerUser::solve` builds `TorsionInputs` (resolving
OD→D if given) and calls `solve_forward` with the chosen `FrictionModel` and the single
applied moment.

**Loading-direction assumption (documented):** the legs are loaded so the coils **wind
tighter** (the only correct loading sense for a torsion spring — winding to open is weaker
and not supported). `M`, `θ`, and σᵢ are taken in that sense; a negative/zero moment is
rejected as `InconsistentInputs`.

## Error handling

Reuse `SpringError`. `solve_forward` validates up front and returns
`InconsistentInputs` for: non-finite or non-positive `d`, `D`, `N_b`; negative or
non-finite `L₁`/`L₂`; spring index `C ≤ 1` (`D ≤ d`); non-finite or non-positive applied
moment; and (when supplied) non-finite or non-positive `arbor_dia`. `min_tensile_strength(d)`
surfaces `DiameterOutOfRange` for an out-of-range wire size. Defense in depth: every
public entry validates its own inputs.

## Status checks (`DesignStatus`)

- **Overstress** (`Severity::Warning`): σᵢ > `allowable_pct_bending · MTS`. Reuses the
  existing `Material.allowable_pct_bending` field — **no new material field needed**
  (the extension hook-bending check already uses it).
- **Arbor binding** (`Severity::Warning`): when `arbor_dia` is supplied and the wound
  inner diameter `inner′ ≤ arbor_dia` at any load point (the spring tightens onto the
  arbor).
- **Index out of range** (`Severity::Caution`): `C` outside the recommended 4–12 band,
  reusing the same guidance as compression/extension.

## Testing strategy (strict TDD)

- **Golden (ShigleyFriction):** the Shigley Ch. 10 worked torsion example — input values
  and expected `k′`, σᵢ, `θ` transcribed from the text and independently hand-verified,
  pinned as exact assertions (same approach as the compression golden oracle).
- **Oracle (PureBending):** an independently hand-derived case for the `64·D·Nₐ` rate
  (no published example needs to match), pinned to high precision.
- **Mechanics unit tests:** `K_bi` exact value at a known `C`; `Nₐ` with non-zero legs
  vs. body-only; `wound_mean_diameter` at a known θ; both friction denominators distinct.
- **Unit round-trips:** `Moment` (N·m / N·mm / lbf·in), `Angle` (rad / deg / turn),
  `AngularRate` (per-rad / per-deg / per-turn).
- **Input validation:** one test per guard (each returning `InconsistentInputs`), plus the
  arbor-binding and overstress status checks.
- **Mutation gate:** `cargo mutants --in-diff` on `springcore`, literal 0 survivors
  (proven-equivalent mutants excluded only with a formal per-entry argument).

## Global constraints

- MSRV 1.88; dual MIT/Apache; **SI canonical internally** (radian-canonical angle/rate),
  convert at the boundary.
- Every formula cited inline (Shigley Ch. 10 torsion-spring sections with equation
  numbers; EN 13906-3 for the pure-bending rate; design spec path).
- No commercial-product/vendor references in any persisted file (technical citations such
  as Shigley and EN standards are permitted; the prohibition is the commercial
  inspiration product/vendor).
- Strict TDD; `cargo fmt`, `cargo clippy --workspace --all-targets --all-features -D
  warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc`, repo-wide `typos`, `cargo deny check
  all`, and `cargo mutants --in-diff` (springcore) all green before push.
- Mandatory adversarial multi-agent review panel before every push, cycling to
  convergence.

## Deferred / open items (noted for later phases, not Phase 1)

- GUI family (scenario tab, presenter per ADR 0008, results, persistence) — the
  `FrictionModel` selection is persisted there, mirroring the curvature-correction toggle.
- Force-on-a-leg input (`M = F·r`) and the TwoLoad/RateBased/Dimensional analogues.
- Torsion minimum-weight optimizer and torsion fatigue.
