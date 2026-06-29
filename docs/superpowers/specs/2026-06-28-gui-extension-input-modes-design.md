# Extension Input Modes in the GUI — Design (Spec 1c)

**Status:** Approved (design dialogue 2026-06-28)
**Scope:** Fan the **extension** family's remaining four input modes — **TwoLoad**,
**RateBased**, **Dimensional**, **MinWeight** — out across the `springmaker` GUI, mirroring
the compression scenario-picker pattern. Spans `springmaker` (extension scenario machinery)
and `springcore` (the four new `ExtScenarioSpec` persistence variants). No new engine solver
work: the engine scenarios and the min-weight optimizer already exist.
**Sub-project:** 1c of the "bring families to the GUI" effort, built on 1b (which shipped
extension **PowerUser** end-to-end: family dispatch, family-scoped state/messages,
family-tagged persistence, three-stress hook results, hook-geometry modes).

## Goal

After 1b the extension family is live but renders a single input mode (PowerUser). Compression
already offers all five modes through a scenario sub-picker; 1c brings extension to parity by
adding the same picker and the four remaining modes. The three **forward** modes reuse 1b's
results panel unchanged (they all yield a standard `ExtensionDesign`); **MinWeight** adds a
small family-local optimization-result section (binding constraint + mass). The work is a
faithful mirror of the compression machinery so the two families stay diff-able.

## Non-goals (deferred)

- **Torsion** input modes in the GUI — a later spec.
- **Extension fatigue** in the GUI — not in the engine's extension surface; out of scope.
- A **shared** (cross-family) min-weight / binding results aggregate — the binding enums differ
  per family (see below); generalizing now would be speculative coupling (1a's deferral stands).
- Any change to compression *behavior*.

## Architecture

### Module layout

```
springcore/src/
  persistence.rs        — ExtScenarioSpec gains TwoLoad / RateBased / Dimensional / MinWeight
                          variants (SI mm/N), mirroring the compression ScenarioSpec variants
springmaker/src/
  extension/
    form.rs       — ExtScenarioKind enum + ALL_EXT_SCENARIOS; ExtFormState gains `scenario`
                    and the per-mode fields; Field gains the new variants; parse_and_solve,
                    build_spec, populate_from_spec, is_blank all branch per scenario;
                    ExtFormOutcome gains `min_weight: Option<ExtMinWeightExtra>`
    view_model.rs — inputs_view branches per scenario; new MinWeight results section
                    (binding + mass) built from shared widgets primitives
    view.rs       — scenario styled_pick_list in the Setup group; ext_field_id gains the
                    new field ids (shared with the Simulator tests, per the 1b contract)
  app.rs          — Message::ExtScenario(ExtScenarioKind); recompute unchanged otherwise
```

No change to `presenter.rs` or `widgets.rs` — the generic `FieldDescriptor<F>` and the shared
widget kit already carry everything 1c needs.

### Scenario enum + selector

- **`ExtScenarioKind { PowerUser, TwoLoad, RateBased, Dimensional, MinWeight }`** in
  `extension/form.rs`, `#[derive(Default)]` with `PowerUser` default, plus `ALL_EXT_SCENARIOS`
  and a `Display` impl ("Power User", "Two Load", "Rate Based", "Dimensional", "Min Weight").
  This is the extension family's **own** enum, *not* compression's `ScenarioKind` — the one-way
  module boundary forbids `extension` importing `compression`, and the per-mode field sets and
  solve paths genuinely differ (see TwoLoad below).
- **`Message::ExtScenario(ExtScenarioKind)`** in `app.rs` sets `self.extension.scenario` and
  recomputes — exactly parallel to compression's `Message::Scenario`.
- The selector is a `styled_pick_list(ALL_EXT_SCENARIOS, Some(scenario), Message::ExtScenario)`
  in the extension design panel's **Setup** group, alongside the material picker — the same
  placement compression uses.

## Form state & per-mode field sets

`ExtFormState` gains `scenario: ExtScenarioKind` and these string fields (reusing existing ones
where the meaning matches): `force1`, `length1`, `force2`, `length2` (TwoLoad); `rate`
(RateBased rate **and** MinWeight required rate, one field reused as compression does);
`outer_dia` (Dimensional); `max_force`, `candidate_diameters` (MinWeight); `index_min`,
`index_max` (MinWeight, **pre-filled** "4" / "12"); `max_outer_dia` (MinWeight, optional).
Existing fields (`wire_dia`, `mean_dia`, `active`, `free_length`, `initial_tension`, `loads`,
`hook_mode`, `hook_r1`, `hook_r2`) are reused. `Field` gains: `Force1`, `Length1`, `Force2`,
`Length2`, `Rate`, `OuterDia`, `MaxForce`, `CandidateDiameters`, `IndexMin`, `IndexMax`,
`MaxOuterDia`.

`inputs_view` branches on `scenario`. The hook-geometry group renders for **every** mode (all
modes carry hooks). The per-mode primary input fields:

| Mode | Primary input fields (in order) |
|------|---------------------------------|
| PowerUser | wire dia, mean dia, active, free length, initial tension, loads |
| TwoLoad | wire dia, mean dia, free length, force 1, length 1, force 2, length 2 |
| RateBased | wire dia, mean dia, spring rate, free length, initial tension, loads |
| Dimensional | wire dia, **outer dia**, active, free length, initial tension, loads |
| MinWeight | required rate, max force, initial tension, index min, index max, max outer dia (optional), candidate diameters |

**Initial tension shows for every mode except TwoLoad** — TwoLoad derives the initial tension
(Fᵢ = F₁ − k·y₁) from the two load points, so it is a *result*, not an input.

**TwoLoad asymmetry vs compression (call-out):** extension TwoLoad **requires** `free_length`,
unlike compression's TwoLoad. The deflections are anchored as y = L − L₀, so the free length is
needed to convert the two operating lengths into deflections; both operating lengths must be ≥
the free length. A reviewer diffing the two families will see this difference — it is intended
and engine-mandated, not an oversight.

## Form → solve

`parse_and_solve` branches on `scenario`. The forward modes build the matching
`springcore::extension::scenario` struct from the parsed form and call `.solve(material,
correction)`, each yielding a standard `ExtensionDesign` (the 1b results panel renders it
unchanged):

- **TwoLoad** → `TwoLoad { wire_dia, mean_dia, free_length, hooks, point1: (F₁, L₁),
  point2: (F₂, L₂) }`
- **RateBased** → `RateBased { wire_dia, mean_dia, rate, free_length, initial_tension, hooks,
  loads }`
- **Dimensional** → `Dimensional { wire_dia, outer_dia, active, free_length, initial_tension,
  hooks, loads }`

Hooks for the forward modes resolve through the existing `resolve_hooks` (Default →
`HookEnds::default_for(mean_dia)`; Custom → parsed radii).

**MinWeight** builds an `ExtMinWeightRequest { required_rate, max_force, initial_tension, hooks,
index_bounds: (index_min, index_max), max_outer_dia, candidate_diameters }` and calls
`springcore::solve_min_weight(material, &req, correction)`. Its `hooks` is a **`HookSpec`** (the
optimizer abstraction: `Default` defers r1=D/2, r2=D/4 to each candidate diameter, or
`Fixed { r1, r2 }`), resolved by a new `resolve_hooks_spec(form) -> Result<HookSpec>` (no
`mean_dia` needed — D is what the optimizer varies). `ExtMinWeightRequest` has public fields and
is reachable (with `solve_min_weight`, `ExtMinWeightSolution`, `ExtBindingConstraint`, `HookSpec`)
via `springcore::extension`, and `solve_min_weight` self-validates (index floor, finiteness,
feasibility) and returns `Result`, so the form constructs the request directly and surfaces the
optimizer's error — no separate validating constructor is needed.

`ExtFormOutcome` gains `min_weight: Option<ExtMinWeightExtra>` (parallel to compression's
`FormOutcome.min_weight: Option<MinWeightExtra>`). On a MinWeight solve the outcome is
`{ design: solution.design, min_weight: Some(ExtMinWeightExtra { binding, mass_kg }) }`; every
other mode leaves it `None`.

## MinWeight results (family-local)

`ExtMinWeightSolution { design: ExtensionDesign, binding: ExtBindingConstraint, mass_kg: f64 }`.
When `min_weight` is `Some`, `extension/view_model.rs` prepends an **Optimization** section to
the results: the binding constraint (one of **BodyShear / HookBending / HookTorsion / Index /
OuterDiameter**, formatted as a human label) and the achieved **mass**, followed by the standard
geometry rows + three-stress hook table for the winning `ExtensionDesign`. The section is built
from the shared `widgets.rs` primitives (`divided_result_section`, `rows_section`) but the
binding enum is extension-specific (compression's binding variants differ), so the shaping stays
in the extension presenter — **not** lifted into a shared aggregate (1a's "generalize only when
needed" deferral still holds: the two binding enums are not unifiable without speculative
coupling).

## Persistence (springcore)

`ExtScenarioSpec` gains four variants (SI mm/N on disk), mirroring the compression
`ScenarioSpec` shapes but carrying `hooks: HookSpecSpec` where compression carries
`end_type`/`fixity`:

```rust
#[serde(tag = "type")]
pub enum ExtScenarioSpec {
    PowerUser { /* unchanged (1b) */ },
    TwoLoad {
        wire_dia_mm: f64, mean_dia_mm: f64, free_length_mm: f64, hooks: HookSpecSpec,
        force1_n: f64, length1_mm: f64, force2_n: f64, length2_mm: f64,
    },
    RateBased {
        wire_dia_mm: f64, mean_dia_mm: f64, rate_n_per_m: f64, free_length_mm: f64,
        initial_tension_n: f64, hooks: HookSpecSpec, loads_n: Vec<f64>,
    },
    Dimensional {
        wire_dia_mm: f64, outer_dia_mm: f64, active: f64, free_length_mm: f64,
        initial_tension_n: f64, hooks: HookSpecSpec, loads_n: Vec<f64>,
    },
    MinWeight {
        required_rate_n_per_m: f64, max_force_n: f64, initial_tension_n: f64,
        hooks: HookSpecSpec, index_min: f64, index_max: f64,
        max_outer_dia_mm: Option<f64>, candidate_diameters_mm: Vec<f64>,
    },
}
```

`build_spec` and `populate_from_spec` become per-scenario matches producing/consuming the right
variant; both round-trip losslessly (incl. `max_outer_dia_mm` `None` and `Some`). Extension has
no clash-allowance (it is not a stacked compression spring — no solid-height clash), so
MinWeight carries no clash field. `SavedDesign::from_toml` already rejects non-finite floats
before any `DesignSpec`; the four new variants inherit that guard. Save remains atomic (temp +
rename). `SavedDesign::solve*` stays compression-only by design (1b); the GUI re-solves
extension through `populate_from_spec` → `parse_and_solve`, so save→load→re-solve covers every
mode.

## Data flow

```
extension scenario pick-list → Message::ExtScenario → App.extension.scenario
ExtField msgs → App.extension (per-mode strings)
recompute → extension::form::parse_and_solve(form, App.material, App.unit_system, App.correction)
   forward → extension::scenario::{TwoLoad|RateBased|Dimensional}.solve → ExtensionDesign
   minweight→ ExtMinWeightRequest → solve_min_weight → ExtMinWeightSolution
   → ExtFormOutcome { design, min_weight }
view → extension::view_model (Optimization section if min_weight; geometry + three-stress table)
     → extension::view (humble iced widgets, shared widgets.rs kit)
save → build_spec(scenario) → DesignSpec::Extension(<variant>) → TOML
load → populate_from_spec(scenario) → ExtFormState → recompute
```

## Error handling

- Reuse 1b's form error model: `parse_and_solve` returns `Result`; `ExtFormOutcome` keeps the
  outcome-vs-error exclusivity. Parse/`SpringError`s surface in the results/status panels.
- **MinWeight:** empty `candidate_diameters` → field-named parse error (mirror compression).
  Index band below the engine floor (extension's is `2 + √3 ≈ 3.732`, stricter than
  compression's `1.866`, from the hook-torsion factor pole) or `index_min ≥ index_max` →
  surfaced from `solve_min_weight`. An infeasible request (no candidate satisfies the
  constraints) → surfaced optimizer error.
- **TwoLoad:** equal operating lengths (y₂ = y₁ → divide-by-zero) and operating lengths below
  the free length are rejected by the engine and surfaced.
- All numerics validated at parse (non-finite / out-of-domain rejected with a field-named
  message); the engine guards remain the defense-in-depth backstop.

## Testing

Mirror compression's coverage; strict TDD.

- **springcore:**
  - Persistence round-trip per new variant (`SavedDesign` → TOML → `SavedDesign`, identical),
    incl. `HookSpecSpec` Default/Custom and `max_outer_dia_mm` `None`/`Some`.
  - Non-finite rejection covers a float in each new variant.
  - `cargo mutants --in-diff` on `springcore`, literal **0 survivors**.
- **springmaker:**
  - `inputs_view` field-set test per scenario (incl. the initial-tension-absent TwoLoad case and
    the MinWeight optional `max_outer_dia`).
  - `build_spec` / `populate_from_spec` round-trip per scenario.
  - `is_blank` per scenario (required fields trip it; pre-filled `index_min`/`index_max` and the
    optional `max_outer_dia` do **not**) — closing the same blank-form trap fixed in 1b.
  - `parse_and_solve` per forward mode (asserting a reasonable rate) and MinWeight (asserting the
    binding constraint and a positive mass).
  - Headless `Simulator` E2E: switch the scenario pick-list across modes, enter inputs, solve,
    and assert the right inputs render and results appear; one MinWeight run asserts the
    Optimization section renders.
  - Widget ids for the new fields go through `ext_field_id` (shared with the tests, per the 1b
    single-source-of-truth contract).

## Global constraints

- MSRV 1.88; iced 0.14; dual MIT/Apache; **SI canonical** in the engine, convert at the boundary.
- ADR 0008 presenter / humble-view split preserved.
- Every engine formula already cited in `springcore`; no new formulas (engine scenarios and the
  optimizer already exist and are cited).
- `cargo fmt`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `RUSTDOCFLAGS="-D warnings" cargo doc`, repo-wide `typos`, `cargo deny check all`,
  `cargo test --workspace`, and `cargo mutants --in-diff` (springcore) all green before push.
- No commercial-product/vendor references in any persisted file.
- No `#[allow(dead_code)]` or lint-suppression scaffolding; GUI-matched enums stay exhaustive.
  Mandatory adversarial multi-agent review panel before push, cycling to convergence.

## Deferred / open items (for later specs)

- Torsion family input modes in the GUI.
- If a third family later needs the same scenario-picker plumbing, re-evaluate lifting a shared
  scenario-picker helper then — not now (two families do not justify the abstraction, and their
  field sets/solve paths differ).
