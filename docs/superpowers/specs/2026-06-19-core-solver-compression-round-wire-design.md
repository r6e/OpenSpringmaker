# OpenSpringmaker — Core Solver + Compression Cylindrical Round-Wire Calculator

- **Status:** Approved (design), pending implementation plan
- **Date:** 2026-06-19
- **Sub-project:** first of the roadmap sub-projects in §1 (foundation)

## 1. Purpose & context

OpenSpringmaker is an open-source helical spring design application written in Rust
with an iced-rs GUI. It targets the reliability and accuracy that practicing spring
and mechanical engineers expect: every formula and constant is traceable to a cited,
rigorous source, and the engine is developed under strict test-driven discipline.

The full product spans many spring families (compression, extension, torsion, spiral
forms, garter, washers, beams, torsion bars, snap rings) plus a materials database,
fatigue analysis, plotting, CAD export, 3D visualization, and a design database. That
scope is decomposed into sequential sub-projects, each with its own spec → plan →
build cycle. **This document specifies sub-project 1 only.**

### Roadmap (for context; only sub-project 1 is specified here)

1. **Core solver + compression cylindrical round-wire calculator** ← this spec
2. Full editable materials database (multi-material, persisted)
3. Remaining compression variants (rectangular wire, conical, nested/parallel, series, variable pitch)
4. Extension and torsion springs
5. Plots library, 2D CAD (DXF) export, 3D visualization, reports
6. Spiral forms, washers, beams, torsion bars, snap rings, design database (customers/projects/versions)

## 2. Scope of sub-project 1

### In scope
- A calculation library (`springcore`) with:
  - Newtype unit quantities over canonical SI internals.
  - Curated material set loaded from a versioned data file with citations.
  - Cited governing formulas for cylindrical compression springs of round wire.
  - Four determined closed-form solve scenarios.
  - A numeric root-find kernel for target-stress design synthesis.
  - A minimum-weight constrained optimization solve.
  - Per-material fatigue analysis (modified-Goodman) with graceful degradation.
- An iced GUI (`springmaker`) with a single focused calculator form, US/metric
  toggle, a design-status (warnings) panel, and one live results plot.
- Single-design file save/load (human-readable TOML).
- Project scaffolding: dual MIT/Apache-2.0 license, formatting, linting, CI, docs, ADRs.

### Out of scope (later sub-projects)
- Any spring family other than cylindrical compression, round wire.
- Editable/persisted materials database; customers/projects/versions.
- Rectangular wire, conical, nested, series, variable-pitch compression springs.
- Fatigue *diagram* plot (numbers ship in v1; the visual diagram waits for the plotting cycle).
- DXF/CAD export, 3D visualization, multi-format reports.

### Non-negotiable constraints
- **No references to any commercial product or vendor** in any persisted file
  (spec, code, comments, commit messages, data files, docs). All functionality is
  described on its own terms and grounded in cited engineering literature.
- Every formula and constant carries an inline citation (source + equation/section).
- Strict TDD: tests written first, watched fail, then implemented.

## 3. Architecture

Cargo **workspace with two crates**:

- **`springcore`** — pure Rust, no GUI dependencies. Domain model, units, materials,
  formulas, scenarios, solver, fatigue. All TDD lives here. Public API is explicit;
  GUI must not reach into internals.
- **`springmaker`** — the iced GUI, depends on `springcore`.

### Decision A — unit typing: newtype quantities (chosen)
Lightweight newtypes (`Length`, `Force`, `Stress`, `SpringRate`, `Frequency`,
`Mass`, `Angle`, …) each wrap an `f64` stored in canonical **SI** (metre, newton,
pascal, N/m, hertz, kilogram, radian). Construction and read-out go through explicit
`from_*` / `to_*` methods named by unit. Conversion to/from display units (US
customary or metric-engineering) happens only at the UI boundary. Rationale: most of
the safety of compile-time dimensional analysis without the ergonomic friction that
`uom` introduces at iced text-entry sites and at native-unit material coefficient
evaluation.

### Decision B — plotting: `plotters` + `plotters-iced` (chosen)
Charts render into an iced canvas widget via `plotters-iced`, rather than
hand-rolling drawing on the raw iced canvas.

## 4. Domain model & cited formulas

All symbols in SI internally. `d` = wire diameter, `D` = mean coil diameter,
`C = D/d` = spring index, `Na` = active coils, `Nt` = total coils, `G` = shear
modulus, `E` = Young's modulus, `F` = axial force, `τ` = shear stress.

| Quantity | Formula | Citation |
|---|---|---|
| Spring index | `C = D/d` | Shigley §10-1 |
| Spring rate | `k = G·d⁴ / (8·D³·Na)` | Shigley Eq. 10-9; EN 13906-1 |
| Wahl correction factor | `Kw = (4C−1)/(4C−4) + 0.615/C` | Wahl (1963); Shigley Eq. 10-5 |
| Bergsträsser factor (alt.) | `Kb = (4C+2)/(4C−3)` | Shigley Eq. 10-6 |
| Corrected shear stress | `τ = Kw·8·F·D / (π·d³)` | Shigley Eq. 10-7 |
| Solid length (squared-ground) | `Ls = d·Nt` | Shigley Table 10-1 |
| Free length / coils per end type | see end-type table below | Shigley Table 10-1 |
| Natural frequency (both ends fixed) | `fn = (d / (2π·Na·D²))·√(G / (32·ρ))`, ρ = mass density | Shigley Eq. 10-25 |
| Critical buckling | deflection ratio vs slenderness, both end fixities | Shigley Eq. 10-11, Fig. 10-6 |
| % of MTS | `τ / Sut(d)` at each load condition | SMI Handbook |

### End-type table (Shigley Table 10-1)

| End condition | End coils `Ne` | Total coils `Nt` | Free length `L0` | Solid length `Ls` |
|---|---|---|---|---|
| Plain | 0 | `Na` | `p·Na + d` | `d·(Nt+1)` |
| Plain, ground | 1 | `Na + 1` | `p·Na` | `d·Nt` |
| Squared (closed) | 2 | `Na + 2` | `p·Na + 3d` | `d·(Nt+1)` |
| Squared & ground | 2 | `Na + 2` | `p·Na + 2d` | `d·Nt` |

(`p` = pitch.) End type is a first-class input; the active/total coil and length
relations are selected from this table.

## 5. Solver engine

Canonical internal units = SI. A `Scenario` trait with one implementation per solve
mode keeps each unit small and independently testable. Each scenario declares which
quantities are its inputs and computes the rest with explicit, cited equations.

### Determined closed-form scenarios
1. **Power User** — geometry fully specified (material, `d`, `D` or `C`, `Na`, end
   type, free length) plus load/length conditions; compute all performance outputs.
2. **Two Load** — two (load, length) operating points given; solve rate and free
   length, then geometry/performance.
3. **Rate Based** — desired spring rate plus index (or `d`); solve `Na` and the rest.
4. **Dimensional** — outer/inner diameter, free length, and solid constraints given;
   solve coil counts and performance.

Each is algebraically invertible with `C`/index fixed (e.g. `k = G·d⁴/(8·D³·Na)`
inverts directly for `Na`).

### Numeric root-find kernel
Bracketed **bisection with a Newton accelerator**, explicit absolute/relative
tolerance and iteration cap, returns a typed error on non-convergence or a bad
bracket. Needed because target-stress design synthesis ("size the spring so the
stress equals a target % of MTS at solid") has the diameter `d` on both sides:
allowable stress `Sut(d)` is itself a function of `d`. The kernel is a named,
separately-tested engine component.

### Minimum-weight optimization
Objective: minimize wire mass, figure of merit ∝ `d²·D·Nt` (Shigley Ch. 10 spring
design / figure-of-merit treatment). Subject to constraints: corrected stress ≤
allowable at the critical condition, spring index within an allowable range, no
buckling at max deflection, and fit (OD/ID) limits. Method: evaluate candidate wire
diameters (standard wire sizes plus continuous refinement), reject infeasible
designs, and select the minimum-mass feasible design. Returns the chosen design plus
which constraints were active/binding. The method and objective are documented and
cited in code.

All solver outputs return `Result` with typed, user-meaningful error variants
(inconsistent inputs, non-convergence, infeasible optimization, out-of-range material).

## 6. Materials data model & file schema

Curated set for v1, loaded from a **versioned TOML data file** (no recompile to add a
material). Initial materials: ASTM A228 music wire, ASTM A229 oil-tempered, ASTM A313
stainless type 302, ASTM A401 chrome-silicon.

**Critical correctness rule (unit-native coefficients):** minimum tensile strength
follows `Sut = A / d^m` (constant, binomial, or polynomial form). The coefficients
are unit-specific (US `kpsi·in^m` vs SI `MPa·mm^m` differ — Shigley Table 10-4). Each
material's MTS equation stores its **native unit**, is evaluated in native units, and
only the **scalar result** is converted to SI. Coefficients are never converted.

TOML entry fields:
- `name`, `specification`, `comments`
- `category` (round wire), `valid_diameter_range` (with unit)
- `mts_equation`: `{ form = "constant|binomial|polynomial", native_unit, coefficients[], diameter_unit }`
- `youngs_modulus_E`, `shear_modulus_G`, `density`
- `allowable_pct_torsion`, `allowable_pct_bending`, `allowable_pct_set`
- `endurance` (optional): cited fatigue data; absent ⇒ fatigue reports "no data"
- `citations`: source(s) for the above values

Every numeric value in the data file is accompanied by a citation field.

## 7. Fatigue model

**Per-material**, graceful degradation. Modified-Goodman criterion using cited
endurance data. **Zimmerli endurance constants apply only to steel spring wire**
(music / hard-drawn / oil-tempered) — materials lacking cited endurance data report
"no fatigue data available" rather than borrowing another material's constants.

Outputs: alternating stress `τa`, mean stress `τm`, fatigue factor of safety
(modified-Goodman), and an estimated cycle-life category. The visual fatigue diagram
is deferred to the plotting sub-project; v1 ships the numeric results plus the single
results plot (§8).

Materials with cited endurance data in v1: A228 music wire and A229 oil-tempered
(Zimmerli). A313 / A401 ship without endurance data unless a cited source is added,
and report "no fatigue data available."

## 8. GUI (iced)

Single focused calculator form:
- Material picker, scenario selector.
- Input fields, with fields **not owned by the active scenario** rendered read-only /
  dimmed (no silent ignoring). Editing such a field prompts switching to the scenario
  that owns it. The active inputs are visually distinguished.
- Live-computed outputs (recompute on input change).
- US/metric unit toggle (conversion at this boundary only).
- **Design Status panel**: warnings/cautions — stress > allowable, buckling risk at
  max deflection, expanded OD > hole diameter, spring index outside the recommended
  4–12 range, infeasible/inconsistent input.
- One live **results plot** (stress or load vs. deflection) via `plotters-iced`.
- Accessibility: keyboard navigation, labelled controls.

## 9. Persistence

Save/open a single design as human-readable **TOML**: scenario, input values with
their units, selected material reference, end type, and chosen display unit system.
No database, customers, projects, or versioning in v1.

## 10. Testing strategy (strict TDD)

- **Golden fixtures from published worked examples** — the accuracy contract.
  Enumerated targets (transcribe exact inputs/outputs from the sources during
  implementation):
  - Shigley, *Mechanical Engineering Design* (10th ed.), compression-spring worked
    examples (Ex. 10-1 design, Ex. 10-2 static, plus the fatigue examples 10-4/10-5).
  - SMI *Handbook of Spring Design* worked compression-spring example(s).
  - EN 13906-1 worked example (standard annex).
  Each fixture asserts engine output against the published numbers within a stated
  tolerance.
- **Property tests**: forward∘inverse round-trips for each determined scenario;
  unit-conversion invariants (`to_x(from_x(v)) == v` within tolerance; cross-unit
  consistency).
- **Solver tests**: root-find convergence on known roots, non-convergence and
  bad-bracket return typed errors; min-weight returns a feasible min-mass design and
  reports binding constraints; infeasible problems return a typed error.
- **Material tests**: MTS evaluated in native units then converted equals published
  strength at sample diameters; out-of-range diameter is rejected.
- **Fatigue tests**: modified-Goodman factor matches a worked example; materials
  without endurance data degrade gracefully.
- Tests written first, observed failing, then implemented.

## 11. Scaffolding

- Dual **MIT / Apache-2.0** license (Rust ecosystem norm).
- `rustfmt` (zero deviation) + `clippy` (strict defaults, curated allow-list),
  enforced in CI and pre-commit.
- **GitHub Actions CI**: fmt check, clippy, test, build.
- `.gitignore`, `README.md`, `ARCHITECTURE.md`, `CONTRIBUTING.md`.
- **ADRs** for significant decisions: scenario-based solver, SI-internal canonical
  units with newtype quantities, unit-native MTS coefficients, two-crate workspace.

## 12. References (citation hierarchy)

Primary standards: **EN 13906-1** (cylindrical helical compression springs);
**SMI *Handbook of Spring Design***. Academic backbone: **Shigley's *Mechanical
Engineering Design*** (Ch. 10); **A. M. Wahl, *Mechanical Springs* (2nd ed., 1963)**
(Wahl correction factor, curvature stress). Materials: **ASTM** specs (A228, A229,
A313, A401). Endurance data: **Zimmerli** (steel spring wire). Specific equation and
table numbers are cited inline at each formula and in §4.

## 13. Open items deferred (not blocking implementation)

- Exact transcription of golden-fixture numbers from each source (done during the
  TDD red phase, per fixture).
- Final curated clippy allow-list (curated during scaffolding).
- Endurance-data sourcing for stainless 302 / chrome-silicon (kept absent until a
  citation is found; fatigue degrades gracefully meanwhile).
