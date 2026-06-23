# OpenSpringmaker — Extension Springs

- **Status:** Approved (design), pending implementation plan
- **Date:** 2026-06-22
- **Sub-project:** 3 of the roadmap (builds on the compression solver + materials
  database, both merged to `main`)

## 1. Purpose & context

The engine and GUI today model **helical compression springs** only: `scenario.rs`
(PowerUser / TwoLoad / RateBased / Dimensional input modes), `SpringDesign`
(with buckling and solid length), `mechanics.rs`, `end_type.rs`, the calculator
screen, and the force–deflection chart all assume axial compression.

This sub-project adds a second spring family — **helical extension springs** — at
full parity with compression: all four input modes, min-weight optimization,
static stress analysis (body **and** hook), and fatigue (body **and** hook). The
two families share `units` and the materials database; they meet only at those
shared pieces and at a GUI spring-type switch.

Non-negotiable project constraints carry over: **no references to any commercial
product or vendor** in persisted files; **every formula and constant cited
inline**; **strict TDD**; **professional-grade accuracy**; **modern UI/UX**; SI
canonical internally; dual MIT/Apache licensing; MSRV 1.88.

## 2. Scope

### In scope
- An `extension` family in `springcore` mirroring the compression architecture:
  an `extension::Scenario` trait, the **four input modes** (PowerUser, TwoLoad,
  RateBased, Dimensional), and an **`ExtensionDesign`** output type.
- **Initial tension** `F_i` as a first-class input, with an advisory when it
  falls outside the recommended τ_i band for the index.
- **Static stresses:** body torsional shear, plus the two **hook stresses** —
  bending at A (`σ_A`) and torsion at B (`τ_B`) — using curvature factors over
  the bend radii `r₁, r₂` (inputs, defaulting to `D/2`, `D/4`). Each reported
  with %-allowable.
- **Min-weight optimization** for extension, honoring rate, max load, index
  bounds, OD limit, and body + hook stress limits.
- **Fatigue:** body fatigue (reusing the existing torsional method/data) and
  **hook fatigue** (a bending criterion at the hook — see §7 data dependency).
- A **GUI** spring-type selector (Compression | Extension) that swaps the
  scenario set, inputs, solver, and results; a new `extension_view_model`
  presenter; reuse of the force–deflection chart and materials UI.
- **Persistence:** save/load round-trips designs of either family.
- Tests: golden worked example, per-scenario, min-weight, fatigue, presenter
  unit tests, and Simulator E2E.

### Out of scope (later cycles)
- **Torsion springs** (a separate future family — moment/angle, bending).
- Hook geometries beyond the standard machine loop/half-loop parameterized by
  `r₁, r₂` (e.g. extended hooks, side loops) as distinct presets.
- Set/creep, temperature derating, DXF/plot export, reports.

## 3. Architecture (Approach A — parallel modules)

### springcore
New `extension` module; compression files stay flat (deliberate asymmetry to
avoid churning working code):

```
springcore/src/extension/
  mod.rs        — public surface for the family
  scenario.rs   — extension::Scenario trait + PowerUser/TwoLoad/RateBased/Dimensional
  design.rs     — ExtensionDesign (solved output) + solve_forward
  mechanics.rs  — rate, initial tension, body shear, hook σ_A/τ_B, curvature factors
  fatigue.rs    — body + hook fatigue
  optimize.rs   — extension min-weight
  ends.rs       — HookType / hook geometry
```

`lib.rs` gains `pub mod extension;`. The family's types are namespaced
(`extension::PowerUser`, `extension::Scenario`, `ExtensionDesign`) so they do not
collide with compression's. Shared and reused unchanged: `units`, `material` +
`MaterialStore`, `numeric`; `error` gains a few extension-specific `SpringError`
variants. `ExtensionDesign` carries initial tension, deflection-onset, body and
hook stresses, and **no** buckling/solid-length; extension ends are hooks
(`HookType`), not compression `EndType`/`EndFixity`.

### springmaker
- `app.rs`: `SpringType { Compression, Extension }`; a separate
  `ExtensionFormState`, an `ExtField` enum, an `ExtScenario` kind, and a
  `Message::ExtField(..)` variant — the two families never share a form/enum.
  `update`/`recompute` branch on `spring_type`.
- `extension_view_model.rs`: pure presenter mirroring `view_model.rs`
  (`inputs_view`, `results_view`, status) over `ExtensionDesign`.
- The calculator view renders the active family's presenter output; decisions
  stay in the presenters (humble-view standard, ADR 0008).
- `persistence.rs` / `ScenarioSpec`: a spring-type tag + the extension scenario.

## 4. Mechanics & formulas

**Sources:** Shigley's *Mechanical Engineering Design* (Budynas & Nisbett),
extension-spring sections, for rate, initial tension, hook stresses, curvature
factors, and recommended maximum stresses; cross-checked against SMI *Handbook
of Spring Design* and Wahl, *Mechanical Springs*. Every formula is cited inline
in the code.

### Rate & deflection
- `k = G·d⁴ / (8·D³·Na)`, with `Na = Nb` (close-wound body coils); `G` = shear
  modulus, `d` = wire diameter, `D` = mean coil diameter.
- **Initial tension** `F_i` (input): coils separate only when `F > F_i`, so
  deflection `y = (F − F_i)/k` for `F > F_i`, else `0`. Length at load
  `L = L0 + y`. An advisory flags `F_i` outside Shigley's recommended initial
  coil-stress (τ_i) band for the spring index.

### Stresses
- **Body** (torsional shear, actual force F): `τ = K_W·(8·F·D)/(π·d³)`, reusing
  the existing Wahl/Bergsträsser correction.
- **Hook bending at A:** `σ_A = F·[(K)_A·(16D)/(π·d³) + 4/(π·d²)]`,
  `(K)_A = (4C₁²−C₁−1)/(4C₁(C₁−1))`, `C₁ = 2r₁/d`.
- **Hook torsion at B:** `τ_B = (K)_B·(8·F·D)/(π·d³)`,
  `(K)_B = (4C₂−1)/(4C₂−4)`, `C₂ = 2r₂/d`.
- `r₁, r₂` are inputs defaulting to `D/2`, `D/4`. Each stress is reported with
  %-allowable: body τ and hook τ_B against the material's torsion fraction,
  σ_A against the bending fraction (per Shigley's recommended extension max
  stresses).

### Input modes
- **PowerUser:** d, D, Na, L0, F_i, r₁/r₂, loads → all of the above.
- **TwoLoad:** two (force, length) points solve **both** k and F_i:
  `k = (F₂−F₁)/(y₂−y₁)`, `F_i = F₁ − k·y₁`.
- **RateBased:** required rate + F_i + L0 + loads → back out Na, then outputs.
- **Dimensional:** outer diameter + Na + L0 + F_i + loads → `D = OD − d`, then
  outputs.
- **MinWeight:** minimize wire mass subject to required rate, max load, index
  bounds, OD limit, and **body + hook** stresses within allowable — the
  compression optimizer's structure with hook constraints added.

### Fatigue
- **Body:** torsional fatigue reusing the existing compression method (Goodman on
  body shear with the material's Zimmerli-type endurance data already in the DB).
- **Hook:** the critical, less-standardized site — bending fatigue at A (and
  torsion at B) via a Goodman/Sines criterion. Requires a **bending** endurance
  basis for the hook, which the current (torsional) DB does not carry. The
  method is cited from Shigley; the per-material endurance values are a data
  dependency (§7) and are sequenced last (§8) so they do not block the rest.

## 5. GUI

- **Spring-type selector** (Compression | Extension) at the top of the Setup
  group — the most fundamental choice, gating the scenario list, inputs,
  end/hook config, solver, and results.
- **State:** separate `ExtensionFormState` / `ExtField` / `ExtScenario`;
  `spring_type` selects the active family; `update`/`recompute` branch on it.
- **Presenter:** `extension_view_model.rs` (pure, no iced), mirroring
  `view_model.rs`.
- **Results panel (extension):** governing rate (hero) + initial tension;
  geometry (body length, free length, OD/ID, coils — no buckling/solid-length);
  load-points table (force, deflection, length, body τ, hook σ_A, hook τ_B, each
  with %-allowable); fatigue section (body FOS + hook FOS); the **reused**
  force–deflection chart, whose extension series is offset by F_i (deflection
  begins once `F > F_i`), so `plot.rs` gains an extension series variant.
- **Persistence:** spring-type-tagged `ScenarioSpec`; either family round-trips.

## 6. Testing

Mirrors the compression family's rigor (testing trophy: mostly integration, some
unit, few e2e):
- **Golden tests:** an authoritative Shigley extension worked example validating
  rate, body τ, and hook σ_A/τ_B against published results; formulas cross-checked
  against a second source (Wahl/SMI) with an independent oracle point
  (data-correctness contract).
- **Per-scenario tests:** TwoLoad recovers both k and F_i; RateBased backs out
  Na; Dimensional applies `D = OD − d`; PowerUser forward solve; save/load
  round-trip.
- **Min-weight tests:** optimizer honors rate + body/hook stress limits + bounds.
- **Fatigue tests:** body fatigue matches the existing method; hook fatigue per
  the chosen criterion.
- **Presenter unit tests** (`extension_view_model`): hermetic (`App::from_store`),
  inputs_view per scenario, results_view over `ExtensionDesign`, status.
- **Simulator E2E:** spring-type selector swap, extension input→solve→results
  (extension inputs get stable widget ids; headless via `ICED_TEST_BACKEND=tiny-skia`).

## 7. Sourcing & data dependencies

- **Formulas / curvature factors / rate / initial-tension band** — fully citable
  from Shigley (+ Wahl/SMI). No blocker.
- **Recommended max stresses** (extension body/hook allowable fractions) — from
  Shigley's table. If they differ from the values already in the material DB,
  that is a small, citable materials addition.
- **Two items may need authoritative figures provided by the maintainer**, as the
  materials data and the EN 13906-1 example did:
  1. **Golden worked-example numbers** — a specific Shigley extension example's
     given inputs and published k / σ_A / τ_B (confirm edition/example, or
     provide the figures).
  2. **Hook-fatigue endurance data** — the bending-endurance basis per material;
     not standardized the way Zimmerli torsional data is. Sourced from
     Shigley/MH/SMI.

## 8. Sequencing (de-risking the data dependencies)

The implementation plan orders work so nothing blocks on data not yet in hand:

1. **Extension static core** — `extension` module scaffolding, mechanics
   (rate, F_i, body τ, hook σ_A/τ_B), `ExtensionDesign`, and the **PowerUser**
   mode, with the golden worked example.
2. **Remaining input modes** — TwoLoad, RateBased, Dimensional.
3. **Min-weight** optimization (body + hook constraints).
4. **GUI** — spring-type selector, `ExtensionFormState`/`ExtField`,
   `extension_view_model`, results panel, chart variant, persistence, Simulator
   tests. (May interleave earlier so each mode is usable as it lands.)
5. **Body fatigue** — reuses existing endurance data.
6. **Hook fatigue** — last, so it can wait on the bending-endurance data without
   holding up the rest.

Each numbered item is one or more bite-sized PRs, each independently reviewable
and shippable, per the project's small-PR + mandatory-review workflow.

## 9. Global constraints (carry-over)

- No commercial-product/vendor references in any persisted file (legal).
- Every formula/constant cited inline.
- Strict TDD; ≥80% coverage target; cargo fmt/clippy `-D warnings`; `cargo doc
  -D warnings`; repo-wide `typos`; `cargo deny check all`; `cargo mutants
  --in-diff` (springcore) — all green before each push.
- SI canonical internally; US-customary at the UI boundary only.
- Humble-view presenter standard (ADR 0008) for all new GUI.
- Always branch + PR; mandatory adversarial review panel to convergence before
  every push; never push without permission.
