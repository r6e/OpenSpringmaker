# Torsion Phase 2 — Engine Input Modes — Design

**Status:** Approved
**Scope:** `springcore` engine only (no `springmaker` GUI, no persistence change). Extends
`springcore/src/torsion/` with three scenarios and two mechanics helpers.
**Phase:** Torsion Phase 2 — the "additional input modes" increment deferred by the
phase-1 spec (`2026-06-26-torsion-springs-phase1-design.md`). The GUI (scenario picker,
form threading, `TorsionSpec` struct→tagged-enum migration) is the follow-up phase,
mirroring how extension shipped engine input modes first and the GUI picker after
(spec 1c precedent).

## Goal

Let a torsion design be specified three more ways — by required angular rate
(**RateBased**), by outer diameter (**Dimensional**), and by two measured operating
points (**TwoLoad**) — plus a cited **force-on-a-leg** helper (`M = F·r`) the GUI will
later expose as a moment-entry convenience. Every mode derives exactly one thing and
delegates to the proven `solve_forward`, so validation, load points, advisories, and
results stay single-sourced.

## Decisions (settled during brainstorming)

1. **Engine-first split:** this spec is springcore-only; the GUI gets its own follow-up
   spec (extension precedent: engine phase 2 → GUI 1c). Isolates the risky persistence
   migration in the GUI phase.
2. **Force-on-a-leg is a helper, not a scenario:** one cited mechanics function
   `moment_from_force_at_radius`. The phase-1 spec calls it a "convenience *input*";
   as a helper it is orthogonal — the GUI phase can offer force@radius entry in ANY
   scenario rather than adding a near-duplicate `LegForce` mode.
3. **Scenario-structs-delegate pattern** (not a generic pre-resolution layer): each
   mode is a struct implementing the existing `Scenario` trait, deriving its one
   quantity, then calling `solve_forward`. Matches all three sibling families; the
   derivations are one line each, so an abstraction layer would be over-engineering.
4. **TwoLoad is offset-tolerant by design** (see the TwoLoad section).

## Non-goals / out of scope (later phases)

- **GUI**: scenario picker, per-mode forms, presenter rows, and the documented
  `TorsionSpec` struct→`#[serde(tag = "type")]`-enum TOML migration — the follow-up
  spec. No persistence file changes in this phase.
- **Minimum-weight optimizer** for torsion; **fatigue** (alternating-moment Goodman).
- Rectangular wire, double torsion, pitch/helix-angle effects.

## Architecture

Two touched files, both `springcore/src/torsion/`:

### 1. `mechanics.rs` — two additive helpers (cited)

```rust
/// Effective active coils that produce angular rate `k'` (the `angular_rate`
/// formula inverted): `Nₐ = E·d⁴ / (denom · D · k')`, where `denom` is 64
/// (PureBending, EN 13906-3 energy method) or 2π·10.8 (ShigleyFriction,
/// Shigley Eq. 10-51). Shared by the RateBased and TwoLoad scenarios.
pub fn active_coils_for_rate(
    youngs_modulus: Stress,
    wire_dia: Length,
    mean_dia: Length,
    rate: AngularRate,
    friction: FrictionModel,
) -> f64
```

```rust
/// Moment produced by a force applied at a radius (elementary statics,
/// `M = F·r`; the torsion-spring loading model of Shigley Ch. 10 — a load on
/// a leg at a moment arm). The GUI exposes this as a force-at-radius
/// moment-entry convenience.
pub fn moment_from_force_at_radius(force: Force, radius: Length) -> Moment
```

Both are pure, mutation-gated to 0 survivors. `active_coils_for_rate` and
`angular_rate` are exact inverses — pinned by a round-trip test.

### 2. `scenario.rs` — three new scenarios implementing the existing `Scenario` trait

Each struct mirrors `PowerUser`'s field set except for its one derived quantity,
derives it, validates the derivation, and delegates to `solve_forward` — which
re-validates everything (two-layer defense, as everywhere in the engine).

**RateBased** — geometry + required rate given; body coils derived.

```rust
pub struct RateBased {
    pub wire_dia: Length,
    pub mean_dia: Length,
    /// Required angular rate `k'`; body coils are derived from it.
    pub rate: AngularRate,
    pub leg1: Length,
    pub leg2: Length,
    pub arbor_dia: Option<Length>,
    pub moments: Vec<Moment>,
}
```

Derivation: `Nₐ = active_coils_for_rate(E, d, D, k', friction)`, then
`N_b = Nₐ − (L₁+L₂)/(3πD)` (Shigley Eq. 10-50 leg contribution, subtracted).
Validation before delegation:
- `k'` must be positive and finite → `InconsistentInputs("rate must be a positive
  finite number")`.
- Derived `N_b` must be > 0 → `InconsistentInputs("leg contribution alone meets or
  exceeds the active coils the required rate allows (body coils would be ≤ 0)")`.
- Everything else (wire/mean/legs/arbor/moments) is validated by `solve_forward`.

Round-trip property: the solved design's `rate` reproduces the requested rate (the
derivation and `solve_forward`'s forward computation are inverses).

**Dimensional** — outer diameter given instead of mean.

```rust
pub struct Dimensional {
    pub wire_dia: Length,
    /// Coil outer diameter; mean is derived as `OD − d`.
    pub outer_dia: Length,
    pub body_coils: f64,
    pub leg1: Length,
    pub leg2: Length,
    pub arbor_dia: Option<Length>,
    pub moments: Vec<Moment>,
}
```

Derivation: `mean = outer_dia − wire_dia`. Scenario-level guard (sibling parity with
compression/extension's identical guard and message, per final-review decision):
`outer_dia` must be a positive finite number —
`InconsistentInputs("outer diameter must be a positive finite number")`. The derived
`mean ≤ 0` and `mean ≤ d` cases still delegate to `solve_forward` (covers every
positive-finite `OD ≤ 2d` input). (The GUI phase adds the field-named `outer > wire`
error at the form boundary, exactly like extension's `dimensional_mean_mm`.)

**TwoLoad** — two measured operating points given; rate, then body coils, derived.

```rust
pub struct TwoLoad {
    pub wire_dia: Length,
    pub mean_dia: Length,
    pub leg1: Length,
    pub leg2: Length,
    pub arbor_dia: Option<Length>,
    /// Two measured (moment, angle) operating points.
    pub point1: (Moment, Angle),
    pub point2: (Moment, Angle),
}
```

Derivation: `k' = (M₂ − M₁) / (θ₂ − θ₁)` (slope), then body coils exactly as
RateBased (shared inversion + leg subtraction). The design's load points are the two
input moments `[M₁, M₂]`, in input order.

**Offset-tolerant by design:** the static model is linear through the free position
(`M = k'·θ_from_free`), so a constant zero-reference offset in the *measured* angles
cancels in the slope. Result deflections come from `solve_forward` as `M/k'` — the
true from-free values — regardless of where the user's protractor zero was. This is
documented on the struct: measured angles need not be referenced to the free position;
only their difference matters.

Validation before delegation:
- Points distinct in both coordinates: `θ₂ ≠ θ₁` → `InconsistentInputs("the two
  operating points must have different angles")`; `M₂ ≠ M₁` → `…("different
  moments")`.
- Slope positive and finite → `InconsistentInputs("the two operating points must
  define a positive finite rate (larger moment at larger angle)")` — rejects
  opposite-sign ordering and overflow.
- Both moments must be > 0 — enforced by `solve_forward`'s existing moment guard.
- Derived `N_b > 0` — same error as RateBased.

### Module exports

`torsion/mod.rs` re-exports the new scenarios and helpers alongside the existing ones:
`pub use scenario::{Dimensional, PowerUser, RateBased, Scenario, TwoLoad};`
`pub use mechanics::{…, active_coils_for_rate, moment_from_force_at_radius};`
(Names collide with the compression/extension scenarios only across modules — each
family's scenarios already live in their own namespace; no ambiguity.)

## Error handling

All derivation-layer failures are `SpringError::InconsistentInputs` with named causes,
listed per scenario above. Delegation to `solve_forward` supplies the second layer
(geometry, moments, material range, finiteness of derived `Nₐ`). No panics on any
input; non-finite inputs are rejected either by the derivation guards (rate, slope) or
by `solve_forward`'s existing finiteness checks.

**Error-precedence requirement:** a derivation that *consumes* geometry (RateBased and
TwoLoad divide by `d⁴`/`D`) must validate that geometry first — wire and mean positive,
finite, `mean > wire`, with the same messages `solve_forward` uses — so a degenerate
input (e.g. `wire_dia = 0` in RateBased) surfaces the geometry error rather than a
misleading derived-body-coils error. A test pins this precedence.

## Testing (TDD; springcore mutation-gated to literal 0 survivors)

**Oracles** (reuse the phase-1 oracle geometry — d=2 mm, D=20 mm, Music Wire
E=203.4 GPa; Nₐ=5 → k'=0.5085 N·m/rad PureBending):

- `active_coils_for_rate(…, 0.5085 N·m/rad, PureBending) == 5.0` (exact inversion) and
  the round-trip `active_coils_for_rate(angular_rate(Na)) == Na` for both friction
  models.
- RateBased with `rate = 0.5085` and no legs → solved design has `body_coils == 5.0`
  and `rate == 0.5085` (round-trip).
- RateBased with legs (L₁=L₂=50 mm) → `N_b == 5.0 − 0.5305164769…` (pins the
  subtraction; value from the phase-1 `active_coils_adds_leg_term` oracle).
- TwoLoad through two points on the k'=0.5085 line — e.g. (0.5085 N·m, 1 rad) and
  (1.017 N·m, 2 rad) → derived rate 0.5085, `body_coils == 5.0`; a second case with
  both angles offset by +0.3 rad derives the SAME design (pins offset tolerance).
- Dimensional with `outer = 22 mm`, `wire = 2 mm` ≡ PowerUser with `mean = 20 mm`
  (identical rate and stress — pins the subtraction).
- `moment_from_force_at_radius(10 N, 50 mm) == 0.5 N·m` (exact).

**Input-domain rejections:** rate ≤ 0 / non-finite; legs non-finite or large enough to
overflow `leg_term` to +Inf (e.g. `3.4e307 m` — finite-but-overflowing, pinning the
leg-attributed message); na-before-leg precedence (tiny rate + huge leg → rate message
surfaces first); legs so long the derived `N_b ≤ 0` (exact boundary: legs chosen so
`N_b == 0`); TwoLoad `θ₂ == θ₁`, `M₂ == M₁`, opposite-sign slope (larger moment at
smaller angle), non-finite point values; Dimensional `outer_dia` zero / negative /
non-finite (OD guard, sibling parity) and `OD == 2d` / `OD < 2d` (positive-finite ODs,
rejected via delegation); zero/negative moments (via delegation). Both friction models
exercised where the friction model enters the derivation.

**Gates:** the standard local CI-parity set (fmt/clippy -D/doc -D/typos/tests) plus
`cargo mutants --in-diff` vs origin/main — literal 0 survivors on every changed
springcore line. Mandatory adversarial review panel (≥3 reviewers + the input-domain
adversary) cycled to convergence before push.

## Follow-up (the GUI phase spec, not this one)

Scenario picker + per-mode forms + presenter rows; the `TorsionSpec`
struct→tagged-enum migration (a conscious TOML format change — old single-scenario
files lack the `type` tag; the migration strategy, e.g. untagged-fallback
deserialization to the PowerUser variant, is that spec's decision); the
force-at-radius moment-entry toggle wired to `moment_from_force_at_radius`; and
updating the `TorsionSpec` guardrail comment in `persistence.rs` (which documents the
struct→enum migration as a conscious format change) as part of executing that migration.
