# Extension-Spring Minimum-Weight Optimization — Design

**Status:** Approved (design dialogue 2026-06-26)
**Scope:** Primarily the `springcore` engine (one new module). During implementation
the scope expanded to a minimal `springmaker` ripple: the new per-material
`allowable_pct_end_torsion` field round-trips through the materials editor, so the GUI
form/view/state were updated to preserve it (the engine work would otherwise drop the
value on save). No new GUI screens.
**Phase:** Extension Phase 3 (follows Phase 1 PowerUser + Phase 2 input modes).

## Goal

Add a minimum-weight constrained optimizer for **extension springs**, mirroring the
existing compression optimizer (`springcore/src/optimize.rs`). Given a required spring
rate, a maximum operating force, an index range, candidate wire diameters, and hook
geometry, return the lightest feasible design whose three governing stresses — body
shear, hook bending (σ_A), and hook torsion (τ_B) — all stay within their material
allowables.

## Background and key results

### Why this is tractable (it mirrors compression with two twists)

The compression optimizer's core, `best_mean_dia`, finds the **largest feasible mean
diameter** for each wire size, because for a fixed required rate a larger `D` means
fewer active coils (`Na = G·d⁴ / (8·D³·k)`) and therefore less wire — so minimum weight
pushes `D` up until a constraint binds. The binding constraint is the body shear stress
(monotonic in the index `C = D/d` above the index floor) or the index ceiling.

Extension differs in exactly two ways:

1. **Three stresses instead of one.** The body shear `τ` is joined by the hook bending
   stress `σ_A` and the hook torsion stress `τ_B`. With the standard scaling hooks
   (`r1 = D/2`, `r2 = D/4`, i.e. `HookEnds::default_for`), the hook curvature indices are
   `C1 = 2·r1/d = C` and `C2 = 2·r2/d = C/2` — so all three stresses are monotone
   increasing in `D` over the working index range. Each therefore imposes an **upper
   bound** on `D`, and the largest feasible `D` is the **minimum of the three
   per-stress roots**, the index ceiling, and any outer-diameter cap. With user-supplied
   **fixed** hook radii the curvature factors `K_A`, `K_B` become constants but each
   stress is still linear-and-increasing in `D`, so the same upper-bound structure holds.

2. **No buckling.** An extension spring is loaded in tension and does not buckle, so the
   compression optimizer's buckling-stability gate is dropped. There is likewise no
   solid-length / clash-allowance term.

### Initial tension and free length are passthrough, not optimization variables

The mass objective depends only on the geometry (`d`, `D`, `Na`, hook size). The active
coils come from the **required rate** (`Na = G·d⁴ / (8·D³·k)`), independent of the
initial tension `F_i`. The three stresses are evaluated at `max_force`, also independent
of `F_i`. Therefore `F_i` does not affect the chosen geometry, the mass, or the binding
constraint — it only shifts the deflection threshold and the reported free length. `F_i`
is carried as a **passthrough input** (validated `≥ 0`, finite) that flows into the final
`solve_forward` call so the returned `ExtensionDesign` reports deflection and free length
correctly. The free length is **derived geometrically** from the chosen `D`, `d`, `Na`,
and hook size (§5).

### Governing formulas (all cited)

- **Body shear** (existing `mechanics::corrected_shear_stress`): τ = K(C)·8·F·D/(π·d³),
  with the selectable curvature factor `K` (Wahl, Shigley Eq. 10-5 / Bergsträsser,
  Shigley Eq. 10-6). `C = D/d`.
- **Hook bending** (existing `extension::mechanics::hook_bending_stress`, Shigley
  extension-spring hook stresses): σ_A = F·[ K_A·16·D/(π·d³) + 4/(π·d²) ],
  K_A = (4·C1² − C1 − 1) / (4·C1·(C1 − 1)), C1 = 2·r1/d.
- **Hook torsion** (existing `extension::mechanics::hook_torsion_stress`): τ_B = K_B·8·F·D/(π·d³),
  K_B = (4·C2 − 1)/(4·C2 − 4), C2 = 2·r2/d.
- **Rate → active coils** (existing `mechanics::active_coils_for_rate`): inverse of
  k = G·d⁴ / (8·D³·Na).
- **Free length** (Shigley extension-spring free-length relation, generalized — §5):
  L₀ = 2·(d_loop − d) + (Na + 1)·d, with d_loop = 2·r1 (the hook-loop mean diameter;
  d_loop = D for the default hook).
- **Developed wire length** (Acxess Spring, *Calculate Length of Coiled Spring Wire*:
  `Li = π·D·(N + 2)`, the `+2` being the two cross-over/side hooks): modeled here as a
  body term plus a per-hook loop term (§4).

## Public API

New file `springcore/src/extension/optimize.rs`; types re-exported from
`springcore/src/extension/mod.rs`.

```rust
/// How the hook geometry is determined during the search.
pub enum HookSpec {
    /// Standard machine loops that scale with the mean diameter: r1 = D/2, r2 = D/4
    /// (matches `HookEnds::default_for`).
    Default,
    /// Fixed absolute bend radii, independent of D.
    Fixed { r1: Length, r2: Length },
}

/// Which limit determines the chosen extension design.
pub enum ExtBindingConstraint { BodyShear, HookBending, HookTorsion, Index, OuterDiameter }

/// A minimum-weight extension-spring problem.
pub struct ExtMinWeightRequest {
    pub required_rate: SpringRate,
    pub max_force: Force,
    /// Built-in preload. Passthrough: validated (>= 0, finite) and reported, but it does
    /// not affect the mass, the stresses, or the binding constraint.
    pub initial_tension: Force,
    pub hooks: HookSpec,
    pub index_bounds: (f64, f64),
    pub max_outer_dia: Option<Length>,
    pub candidate_diameters: Vec<Length>,
}

/// The chosen design and why it is limited.
pub struct ExtMinWeightSolution {
    pub design: ExtensionDesign,
    pub binding: ExtBindingConstraint,
    pub mass_kg: f64,
}

pub fn solve_min_weight(
    material: &Material,
    req: &ExtMinWeightRequest,
    correction: CurvatureCorrection,
) -> Result<ExtMinWeightSolution>;
```

`HookSpec` resolves to a concrete `HookEnds` for any given `D`:
`Default → HookEnds::default_for(D)`; `Fixed { r1, r2 } → HookEnds { r1, r2 }`.

## Algorithm

### `best_mean_dia(material, d, max_force, bounds, hooks, correction) -> Option<(Length, ExtBindingConstraint)>`

1. `allow_torsion = allowable_pct_torsion · MTS(d)` (body shear, 45%);
   `allow_end_torsion = allowable_pct_end_torsion · MTS(d)` (end-hook τ_B, per-material —
   Shigley Table 10-7: 40% carbon/low-alloy steel, 30% stainless/nonferrous);
   `allow_bending = allowable_pct_bending · MTS(d)` (75%). (Returns `None`
   if `MTS(d)` is unavailable — wire out of manufacturable range.)
2. Bracket: `dm_lo = c_min·d`, `dm_hi = c_max·d`.
3. For each of the three stresses `s(D)` — body shear (`τ`, allow_torsion), hook bending
   (`σ_A`, allow_bending), hook torsion (`τ_B`, allow_end_torsion) — compute its upper bound:
   - if `s(dm_lo) > allowable` → the wire overstresses even at the smallest index →
     return `None` (candidate infeasible);
   - else if `s(dm_hi) ≤ allowable` → the stress does not bind → bound = `dm_hi`;
   - else bracket-root-find `D ∈ [dm_lo, dm_hi]` with `s(D) = allowable` (existing
     `numeric::find_root_bracketed`).
4. Collect four candidate bounds — the three stress bounds plus `(dm_hi, Index)` — and
   take the one with the **smallest `D`**; its label is the binding constraint. When no
   stress reaches its allowable within the index range, the limit is the index ceiling,
   labelled `Index`; the exact-`dm_hi` equality tie is immaterial (identical geometry,
   only the reported label could differ).
5. Return `(D, binding)`.

Each stress is monotone increasing in `D` over `[dm_lo, dm_hi]` provided the index floor
`c_min` is at or above the **binding** per-factor turning point. The body shear's U-shape
has a minimum at `C* ≈ 1.866` (Wahl) / `≈ 1.718` (Bergsträsser), but the tightest
precondition is the **hook torsion**: its factor `K_B·C2` uses `C2 = C/2` and is minimised
where `4·C2² − 8·C2 + 1 = 0`, i.e. `C2 = 1 − √3/2` is rejected and `C2 = 1 + √3/2 ≈ 1.866`
is the relevant root, giving the spring-index floor `C = 2 + √3 ≈ 3.732`. Below it the
hook-torsion factor is non-monotone (and has a pole at `C = 2` for default hooks, where
`4·C2 − 4 → 0`). The optimizer therefore rejects any request with `c_min < 2 + √3` as an
input-contract violation; the single-endpoint feasibility test in step 3 is valid only
above this floor.

### `solve_min_weight`

For each candidate `d`:
1. `best_mean_dia(...)` → `(D, binding)`; skip the candidate on `None`.
2. **Outer-diameter cap** (identical to compression): if `D + d > max_outer_dia`, set
   `D = max_outer_dia − d`; skip if the capped index `< c_min`; set
   `binding = OuterDiameter`.
3. `Na = active_coils_for_rate(G, d, D, required_rate)`; skip if non-finite or `< 1`.
4. `free_length = derive_free_length(D, d, Na, hooks(D))` (§5).
5. `design = extension::design::solve_forward(material, d, D, Na, free_length,
   initial_tension, hooks(D), &[max_force], correction)?`.
6. `mass = wire_mass(material, d, D, Na, hooks(D))` (§4); keep the minimum-mass solution.

Return the lightest solution, or `SpringError::Infeasible` if no candidate survives.

## §4 — Mass objective (body + hooks)

`mass = ρ · (π·d²/4) · L_wire`, where the developed wire length splits into the body and
the two hooks:

```
L_wire = π·D·Na  +  2·(π·d_loop)          // body coils + two hook loops
d_loop = 2·r1                              // hook-loop mean diameter (= D for default hooks)
```

This is the Acxess Spring developed-length model `Li = π·D·(N + 2)` (each hook ≈ one mean
coil) generalized so a **fixed** hook of radius `r1` contributes a loop of its own mean
diameter `d_loop = 2·r1` rather than `D`. For the default hook (`r1 = D/2`),
`d_loop = D` and `L_wire = π·D·(Na + 2)` exactly recovers the cited formula. The hook-loop
term is an explicit, documented engineering **approximation** (the spring's true hook arc
length depends on the specific loop style); it is the only term in the design without a
closed-form textbook citation, and is retained per the design decision (option a) with
this Acxess Spring basis. `r2` (the side-bend radius) governs τ_B only and does not enter
the wire-length model.

## §5 — Free length (geometric, one formula for both hook modes)

`derive_free_length(D, d, Na, hooks) = 2·(d_loop − d) + (Na + 1)·d`, with `d_loop = 2·r1`.

This is Shigley's standard extension-spring free-length relation
`L₀ = 2·(D − d) + (Nb + 1)·d` (two end loops of mean diameter `D` plus the close-wound
body), generalized so the end-loop term uses the actual hook-loop mean diameter
`d_loop = 2·r1`. For the default hook (`r1 = D/2`) it reduces to the textbook form. Body
coils `Nb` are taken equal to the active coils `Na` (close-wound extension body). `r2`
does not affect the loop length.

The derived free length is reported in the returned `ExtensionDesign` and feeds the
deflection/length computation in `solve_forward`; it does **not** constrain the
optimization.

## Initial tension, edges, and errors

- **`initial_tension`**: passthrough. Validated `≥ 0` and finite (the existing
  `solve_forward` guard already enforces this); flows only into the final solve.
  A manufacturable-preferred-`F_i` band advisory (Shigley's preferred uncorrected
  torsional-stress range) is **out of scope** for this phase.
- **Input validation** (mirrors compression): `required_rate` finite `> 0`; `max_force`
  finite `> 0`; `index_bounds` finite with `c_min ≥ 2 + √3 ≈ 3.732` (the hook-torsion
  monotonicity turning point, NOT compression's body-shear `C* ≈ 1.866`) and
  `c_min < c_max`; `candidate_diameters` non-empty; `max_outer_dia` (if present)
  finite `> 0`; for `HookSpec::Fixed`, `r1`/`r2` finite `> 0` (and the resulting `C1`,
  `C2 > 1`, surfaced by `solve_forward`'s hook guards). Malformed requests return
  `SpringError::InconsistentInputs` (bad input), distinct from `Infeasible` (no
  candidate survives). The redundant `0 < c_min` check is intentionally omitted: the
  floor strictly implies it.
- **Infeasibility**: a candidate is skipped when any stress overstresses at `c_min`,
  when `Na < 1` or non-finite, when the OD cap forces the index below `c_min`, or when
  the per-candidate `solve_forward` rejects the resulting geometry (e.g. fixed hooks
  with `C2 ≤ 1`, or `free_length ≤ 0`) — every such case `continue`s to the next
  candidate rather than aborting the search. `solve_min_weight` returns
  `SpringError::Infeasible` when no candidate survives.

## Testing

Mirror the compression optimizer's mutation-pinned test style:

- **Per-binding tests**, one for each `ExtBindingConstraint` variant — `BodyShear`,
  `HookBending`, `HookTorsion`, `Index`, `OuterDiameter` — constructed so the named
  constraint is the smallest upper bound. (The hook-bending and hook-torsion binding
  cases are the new behavior compression cannot exercise.)
- **Exact pins** for `mean_dia`, `mass_kg`, and `free_length` from an independent
  computation (Python), so arithmetic mutations in the stress roots, the mass formula,
  and the free-length formula change the result.
- **Infeasibility**: overstress at `c_min`; OD cap below `c_min`; `Na < 1`.
- **Boundary**: OD cap exactly at `c_min`; `Na` exactly `1`; OD exactly equal to
  `max_outer_dia` (cap must not fire — strict `>`), paralleling the compression suite.
- **F_i passthrough**: two requests differing only in `initial_tension` yield identical
  `mass_kg`, `binding`, `d`, and `D`, but different reported free length / deflection —
  proving `F_i` is outside the optimization.
- **Fixed vs default hooks**: a `HookSpec::Fixed { r1: D/2, r2: D/4 }` request reproduces
  the `HookSpec::Default` result for the same `D`; a different fixed `r1` changes mass and
  free length as the model predicts.
- **Golden**: validate against a worked Shigley extension example if one fits the
  optimizer's inputs; otherwise an independent hand/Python calculation serves as the
  oracle (as the compression optimizer does).
- Gate: `cargo mutants --in-diff` (springcore) → 0 survivors, plus the full
  fmt/clippy/doc/typos/deny suite.

## Out of scope (future phases)

- `springmaker` GUI for extension min-weight (a later GUI phase).
- Manufacturable-preferred-initial-tension advisory.
- Hook **fatigue** (needs bending-endurance data) and body fatigue in the optimizer.
- Non-default loop *styles* beyond the radius-parameterized loop model (e.g. extended
  hooks, V-hooks) — the current model parameterizes the loop by `r1` only.

## Sources

- Shigley's *Mechanical Engineering Design* (10th ed.), extension-spring sections —
  hook bending/torsion stresses, curvature factors, and the free-length relation.
  (Local copy: see project reference notes.)
- Acxess Spring, *Calculate Length of Coiled Spring Wire*
  (`https://www.acxesspring.com/calculate-spring-wire-length.html`) — developed wire
  length `Li = π·D·(N + 2)` for extension springs with cross-over/side hooks.
