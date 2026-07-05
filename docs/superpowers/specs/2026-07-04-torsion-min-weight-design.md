# Torsion Minimum-Weight Optimizer — Design

**Status:** Approved
**Scope:** `springcore` engine only (no `springmaker` GUI, no persistence change). One new
module `springcore/src/torsion/optimize.rs` + re-exports.
**Phase:** the second-to-last torsion phase-1 deferred item (torsion fatigue remains).
Engine-first per the established split; the GUI MinWeight mode (+ the additive
`TorsionSpec::MinWeight` persistence variant) is the follow-up spec — the extension
optimizer precedent (engine PR first, GUI mode in spec 1c after).

## Goal

Given a required angular rate, a maximum applied moment, spring-index bounds, optional
outer-diameter cap, fixed leg lengths, a friction model, and candidate wire diameters,
pick the **lightest** torsion design whose inner-fiber bending stress at the max moment
stays within the material's bending allowable — reporting which constraint bound the
design and the wire mass.

## The structural insight (what makes torsion different)

At a fixed rate k′ and wire diameter d, the body wire length is **independent of the
mean diameter D**:

- Nₐ = E·d⁴/(denom·D·k′) (the PR-44 inversion; denom = 64 PureBending / 2π·10.8
  ShigleyFriction), and the leg term is (L₁+L₂)/(3πD), so
  π·D·N_b = π·D·Nₐ − π·D·(L₁+L₂)/(3πD) = **π·E·d⁴/(denom·k′) − (L₁+L₂)/3**.
- Total wire length (body + straight legs) = π·E·d⁴/(denom·k′) + **⅔(L₁+L₂)**.

So **mass is a strictly increasing function of d alone** — the optimizer needs no
root-finding and no per-D mass search (unlike the siblings, whose mass falls with D):
the lightest feasible candidate diameter wins outright, and D is chosen by *policy*
(decision 1). Two more D-independence consequences:

- **N_b > 0 feasibility is D-independent:** Nₐ > legterm ⟺ E·d⁴/(denom·k′) > (L₁+L₂)/(3π).
- **σᵢ = K_bi(C)·32·M/(π·d³)** with K_bi = (4C²−C−1)/(4C(C−1)) monotone **decreasing**
  for all C > 1 (toward 1 as C→∞), so stress feasibility over a D-interval is decided
  by ONE evaluation at the interval's ceiling, and the stress-governed D (Compact
  policy) comes from inverting K_bi — a **closed-form quadratic in C**:
  `K_bi(C) = t ⟺ C²(4−4t) + C(4t−1) − 1 = 0` with t = allow·π·d³/(32·M).

## Decisions (settled during brainstorming)

1. **`DiaPolicy` is a request field** (user decision — over the YAGNI single-policy
   alternative): mass is D-indifferent, so the caller chooses.
   - `MaxMargin` (the `#[default]`): D = min(c_max·d, OD − d) — minimum stress,
     maximum margin; binding reads `Index` or `OuterDiameter` (whichever ceiling set D).
   - `Compact`: D = max(c_min·d, D_stress) where σᵢ(D_stress) == allowable (the K_bi
     inversion) — the smallest coil that passes; binding reads `BendingStress` (stress
     governed) or `Index` (the c_min floor governed, stress fine there). A Compact D
     exceeding the ceiling (c_max·d / OD − d) means the candidate is infeasible.
2. **Legs are request fields** (`leg1`, `leg2`: non-negative finite). They enter BOTH
   the N_b derivation (the D-independent feasibility above) and the mass (the ⅔ term +
   the legs' own straight length) — a contribution no sibling optimizer has.
3. **`friction_model` is a request field** — the denom changes Nₐ and therefore the
   mass itself, not just the reported rate.
4. **`arbor_dia` is a passthrough `Option`** — validated (positive finite when present)
   and handed to `solve_forward`, whose arbor-binding/over-wind advisories ride along
   in the returned design's status. It is NOT a hard optimizer constraint (advisory-only
   in the engine, kept advisory here).

## API (`springcore/src/torsion/optimize.rs`)

```rust
/// How the winning candidate's mean diameter is chosen — torsion mass is
/// D-independent at fixed rate and wire, so D is policy, not optimization.
#[non_exhaustive] // sibling parity (HookSpec precedent): variants may be added
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiaPolicy {
    /// Largest allowed D: minimum bending stress, maximum margin (default).
    #[default]
    MaxMargin,
    /// Smallest D that satisfies the stress allowable: the most compact coil.
    Compact,
}

/// Which constraint bound the chosen design.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TorBindingConstraint {
    /// σᵢ reached the bending allowable (Compact policy, stress-governed D).
    BendingStress,
    /// A spring-index bound set D (c_max ceiling under MaxMargin; c_min floor
    /// under Compact when stress is satisfied there).
    Index,
    /// The outer-diameter cap set D (MaxMargin with OD − d < c_max·d).
    OuterDiameter,
}

/// A minimum-weight torsion-spring problem.
#[derive(Debug, Clone)]
pub struct TorMinWeightRequest {
    /// Required angular rate k′; fixes Nₐ per (d, D) via `active_coils_for_rate`.
    pub required_rate: AngularRate,
    /// Maximum applied moment; σᵢ is evaluated here and it becomes the design's
    /// single load point. Must be finite and > 0.
    pub max_moment: Moment,
    /// Straight-leg lengths (finite, ≥ 0): enter the body-coil derivation AND the mass.
    pub leg1: Length,
    pub leg2: Length,
    /// Rate model (changes the Nₐ denominator, hence the mass).
    pub friction_model: FrictionModel,
    /// Mean-diameter selection policy (see `DiaPolicy`).
    pub dia_policy: DiaPolicy,
    /// Allowed spring-index range (c_min, c_max): both finite, 1 < c_min < c_max.
    /// NOTE: no `2 + √3` floor here — that sibling floor exists for the extension/
    /// compression stress factors' turning points; torsion's K_bi is monotone
    /// decreasing for ALL C > 1, so C > 1 is the only monotonicity requirement.
    pub index_bounds: (f64, f64),
    /// Optional outer-diameter cap (D + d ≤ this); finite and > 0 when present.
    pub max_outer_dia: Option<Length>,
    /// Optional arbor passthrough; advisory warnings surface in the returned design.
    pub arbor_dia: Option<Length>,
    /// Wire diameters to search; the lightest feasible one wins. Non-empty, all
    /// finite and > 0.
    pub candidate_diameters: Vec<Length>,
}

/// The chosen design, why it is limited, and its wire mass.
#[derive(Debug, Clone)]
pub struct TorMinWeightSolution {
    pub design: TorsionDesign,
    pub binding: TorBindingConstraint,
    pub mass_kg: f64,
}

pub fn solve_min_weight(
    material: &Material,
    req: &TorMinWeightRequest,
) -> Result<TorMinWeightSolution>
```

Re-exports from `torsion/mod.rs`: `DiaPolicy`, `TorBindingConstraint`,
`TorMinWeightRequest`, `TorMinWeightSolution`, `solve_min_weight` (module added as
`mod optimize;`).

## Algorithm (per candidate d, all analytic)

1. **N_b feasibility (D-independent):** skip the candidate unless
   `E·d⁴/(denom·k′) > (L₁+L₂)/(3π)` (else body coils would be ≤ 0 at every D).
2. **D interval:** `dm_lo = c_min·d`; `dm_hi = min(c_max·d, OD − d if capped)`. Skip if
   `dm_hi < dm_lo` (the cap pushes the index below the floor — sibling behavior).
3. **Stress feasibility (one evaluation):** σᵢ at `dm_hi` (the minimum-stress point,
   K_bi decreasing) via the existing `bending_stress_inner`; if it exceeds
   `allowable_pct_bending · MTS(d)`, the candidate is infeasible at every allowed D —
   skip. (MTS via `min_tensile_strength(d)`; out-of-range diameters skip like siblings.)
4. **Choose D per policy:**
   - `MaxMargin`: D = `dm_hi`; binding = `OuterDiameter` if the OD cap set `dm_hi`,
     else `Index`.
   - `Compact`: t = allow·π·d³/(32·M). If `K_bi(dm_lo/d) ≤ t`, stress is satisfied at
     the floor: D = `dm_lo`, binding = `Index`. Else D = `C_stress·d` from the
     closed-form quadratic `C²(4−4t) + C(4t−1) − 1 = 0`, binding = `BendingStress`.
     For t > 1 the quadratic has exactly one root in C > 1 (K_bi is a decreasing
     bijection (1,∞)→(1,∞)); t ≤ 1 CANNOT reach this branch — K_bi > 1 for every
     finite C, so step 3 already skipped such candidates (document this at the code,
     no special-case branch). (D ≤ `dm_hi` is guaranteed by step 3's check.)
5. **Derive and verify:** `body_coils` via `active_coils_for_rate` minus the leg term
   (the PR-44 pipeline); `solve_forward(material, inputs, &[max_moment], friction)` —
   the full engine backstop (index caution, arbor advisories, overstress warning
   cannot fire for a feasible candidate by construction, but the backstop stays). A
   candidate whose forward solve errors is skipped, not fatal (sibling behavior).
6. **Mass from the ACTUAL chosen geometry** (single-sourced with the engine, identical
   to the closed form): `mass = ρ · (π/4)·d² · (π·D·N_b + L₁ + L₂)` via a
   `wire_mass(material, wire_dia, mean_dia, body_coils, leg1, leg2)` helper.
7. **Keep the lightest** across candidates (strictly increasing in d, but the loop
   keeps the min-mass winner like the siblings — robust to unsorted lists). No
   feasible candidate → `SpringError::Infeasible` with a message naming the tried
   candidate count.

## Validation (up-front; malformed = `InconsistentInputs`, empty feasible set = `Infeasible`)

Mirrors extension's block: rate positive finite; max moment positive finite; legs
finite ≥ 0; index bounds finite with `1 < c_min < c_max` (the deliberate absence of the
sibling `2+√3` floor documented at the check); OD/arbor positive finite when present;
candidates non-empty, all positive finite.

## Testing (mutation-gated, literal 0 survivors in-diff)

- **Golden oracle:** k′ = 0.5085 N·m/rad (PureBending), M small, candidates
  {1.5, 2.0, 2.5} mm, no legs → smallest feasible d wins; mass hand-computed from
  ρ·(π/4)d²·π·E·d⁴/(64·k′) and asserted.
- **D-independence pinned:** same request under both policies → same winning d, same
  mass (1e-9), different D (MaxMargin ≥ Compact).
- **Compact inversion oracle:** a stress-governed case where the closed-form C_stress
  is hand-computed from the quadratic and the design's σᵢ at max moment lands on the
  allowable (relative 1e-6); binding = `BendingStress`.
- **Per-policy binding attribution:** MaxMargin with/without a governing OD cap →
  `Index` / `OuterDiameter`; Compact stress-governed / floor-governed →
  `BendingStress` / `Index`.
- **Leg physics:** the ⅔ relationship pinned — same request with legs totalling L vs
  without differs in mass by exactly ρ·(π/4)d²·(⅔·L): the legs add L of straight wire
  but shorten the body by L/3 (the leg term's coil-equivalent). A candidate made
  N_b-infeasible by long legs is skipped while a larger candidate wins.
- **Friction models:** both models on one request → different Nₐ hence different mass
  (ratio = denom ratio, pinned).
- **Bounds:** index floor/ceiling boundary candidates; OD cap below c_min·d + d skips;
  out-of-material-range candidate skips (not fatal).
- **Error split:** each malformed-request case → `InconsistentInputs`; all-infeasible →
  `Infeasible`. Arbor passthrough: an arbor that binds surfaces the engine's advisory
  in the returned design's status.
- Gates: local CI-parity + `cargo mutants --in-diff` vs origin/main → 0 survivors;
  final adversarial panel (floor 3 + MANDATORY input-domain adversary; numerical-
  correctness attention on the closed-form inversion) to convergence.

## Non-goals

- No GUI, no persistence (`TorsionSpec::MinWeight` variant), no `is_blank`/form work —
  the follow-up GUI spec.
- No torsion fatigue (the last deferred item).
- No multi-objective optimization (envelope, resonance) and no non-round wire.
