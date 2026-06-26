# Extension-Spring Minimum-Weight Optimization — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a minimum-weight constrained optimizer for extension springs that picks the lightest feasible design whose body shear, hook bending (σ_A), and hook torsion (τ_B) stresses all stay within allowables — mirroring the compression optimizer.

**Architecture:** A new `springcore/src/extension/optimize.rs` module mirrors `springcore/src/optimize.rs`. For each candidate wire diameter, `best_mean_dia` finds the largest feasible mean diameter as the minimum of three per-stress upper-bound roots, the index ceiling, and an optional OD cap (no buckling — extension is loaded in tension). `solve_min_weight` derives active coils from the required rate, derives the free length from geometry, solves, and keeps the minimum-mass candidate. A small geometry helper for the free length lives in `springcore/src/extension/mechanics.rs`.

**Tech Stack:** Rust (workspace crate `springcore`), `approx` for test asserts, `cargo test`/`clippy`/`mutants`. All formulas reuse existing engine functions.

## Global Constraints

- MSRV 1.88; dual MIT/Apache; SI canonical internally.
- Every formula cited inline (Shigley extension sections; Acxess Spring wire length; design spec `docs/superpowers/specs/2026-06-26-extension-min-weight-design.md`).
- Strict TDD; `cargo fmt`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc`, repo-wide `typos`, `cargo deny check all`, `cargo mutants --in-diff` (springcore) all green before push.
- No commercial-product/vendor references in any persisted file.
- Engine-only: `springcore` only; do NOT touch `springmaker` (extension GUI is a later phase).
- `initial_tension` (F_i) is a passthrough input: validated, reported, but it does NOT affect mass, stresses, or the binding constraint.
- Branch off current `main` (#27/#28/#29/#30 all merged). (Branch `feat/extension-min-weight` already created; the design spec is committed there.)

## Existing functions this plan reuses (do not reimplement)

- `crate::mechanics::corrected_shear_stress(force: Force, mean_dia: Length, wire_dia: Length, factor: f64) -> Stress` — body shear τ.
- `crate::extension::mechanics::hook_bending_stress(force: Force, mean_dia: Length, wire_dia: Length, r1: Length) -> Stress` — σ_A.
- `crate::extension::mechanics::hook_torsion_stress(force: Force, mean_dia: Length, wire_dia: Length, r2: Length) -> Stress` — τ_B.
- `crate::mechanics::active_coils_for_rate(shear_modulus: Stress, wire_dia: Length, mean_dia: Length, rate: SpringRate) -> f64`.
- `crate::numeric::{find_root_bracketed, SolveConfig}` — `find_root_bracketed(|f64| -> f64, lo, hi, SolveConfig) -> Result<f64>`.
- `crate::extension::design::solve_forward(material, wire_dia, mean_dia, active, free_length, initial_tension, hooks, loads, correction) -> Result<ExtensionDesign>`.
- `crate::extension::ends::HookEnds { r1: Length, r2: Length }` and `HookEnds::default_for(mean_dia) -> Self` (r1 = D/2, r2 = D/4).
- `material.min_tensile_strength(d) -> Result<Stress>`, `material.allowable_pct_torsion: f64`, `material.allowable_pct_bending: f64`, `material.shear_modulus: Stress`, `material.density: MassDensity` with `.kg_per_m3()`.
- `crate::test_support::music_wire() -> Material` (test oracle material).

---

## File Structure

- `springcore/src/extension/mechanics.rs` — **(modify)** add `free_length_from_geometry` + its tests.
- `springcore/src/extension/optimize.rs` — **(create)** the optimizer: `HookSpec`, `ExtBindingConstraint`, `ExtMinWeightRequest`, `ExtMinWeightSolution`, private `wire_mass`/`best_mean_dia`, public `solve_min_weight`, and all optimizer tests.
- `springcore/src/extension/mod.rs` — **(modify)** declare `mod optimize;` and re-export the public types.

---

## Task 1: Free-length-from-geometry helper

Derives the extension free length from the chosen geometry so the optimizer can report a complete design. Shigley's standard extension free length `L₀ = 2·(D − d) + (Nb + 1)·d`, generalized so the end loop uses the actual hook-loop mean diameter `d_loop = 2·r1` (default hook `r1 = D/2` ⇒ `d_loop = D`, recovering the textbook form). Body coils `Nb` are taken equal to active coils `Na` (close-wound body).

**Files:**
- Modify: `springcore/src/extension/mechanics.rs`

**Interfaces:**
- Consumes: `HookEnds`, `Length`.
- Produces: `pub fn free_length_from_geometry(wire_dia: Length, active: f64, hooks: HookEnds) -> Length`.

- [ ] **Step 1: Add the import.** At the top of `springcore/src/extension/mechanics.rs`, change `use crate::units::{Force, Length, SpringRate, Stress};` to also bring in the hook type:
  add a separate line `use crate::extension::ends::HookEnds;` below the existing `use` lines (keep stdlib/external/internal grouping; this is an internal import).

- [ ] **Step 2: Write the failing test** in the `#[cfg(test)] mod tests` block of `springcore/src/extension/mechanics.rs`:

```rust
    #[test]
    fn free_length_default_hook_matches_shigley_form() {
        // Default hook: r1 = D/2 ⇒ d_loop = D. D=20mm, d=2mm, Na=10.
        // L0 = 2(D − d) + (Na + 1)d = 2(18mm) + 11·2mm = 36 + 22 = 58 mm.
        let hooks = HookEnds::default_for(Length::from_millimeters(20.0));
        let l0 = free_length_from_geometry(Length::from_millimeters(2.0), 10.0, hooks);
        assert_relative_eq!(l0.millimeters(), 58.0, max_relative = 1e-12);
    }

    #[test]
    fn free_length_fixed_hook_uses_loop_diameter() {
        // Fixed hook r1 = 6mm ⇒ d_loop = 12mm. d=2mm, Na=10.
        // L0 = 2(12 − 2) + 11·2 = 20 + 22 = 42 mm. (r2 does not affect length.)
        let hooks = HookEnds {
            r1: Length::from_millimeters(6.0),
            r2: Length::from_millimeters(3.0),
        };
        let l0 = free_length_from_geometry(Length::from_millimeters(2.0), 10.0, hooks);
        assert_relative_eq!(l0.millimeters(), 42.0, max_relative = 1e-12);
    }
```

- [ ] **Step 3: Run the tests — expect compile failure** (`free_length_from_geometry` undefined).

Run: `cargo test -p springcore --lib extension::mechanics::tests::free_length 2>&1 | tail -5`
Expected: compile error `cannot find function ... free_length_from_geometry`.

- [ ] **Step 4: Implement the helper** in `springcore/src/extension/mechanics.rs` (after the hook-stress functions, before the test module):

```rust
/// Extension-spring free length from geometry (Shigley extension free-length
/// relation, generalized to the hook-loop diameter). The end loop is modeled
/// by its mean diameter `d_loop = 2·r1` (default hook `r1 = D/2` ⇒ `d_loop = D`),
/// so `L₀ = 2·(d_loop − d) + (Na + 1)·d`. Body coils are taken equal to the
/// active coils (close-wound body); `r2` governs torsion only and is not used.
pub fn free_length_from_geometry(wire_dia: Length, active: f64, hooks: HookEnds) -> Length {
    let d = wire_dia.meters();
    let d_loop = 2.0 * hooks.r1.meters();
    Length::from_meters(2.0 * (d_loop - d) + (active + 1.0) * d)
}
```

- [ ] **Step 5: Run the tests — expect pass.**

Run: `cargo test -p springcore --lib extension::mechanics::tests::free_length 2>&1 | tail -5`
Expected: `test result: ok. 2 passed`.

- [ ] **Step 6: fmt + clippy.**

Run: `cargo fmt && cargo clippy -p springcore --all-targets -- -D warnings 2>&1 | tail -2`
Expected: clean.

- [ ] **Step 7: Commit.**

```bash
git add springcore/src/extension/mechanics.rs
git commit -m "feat(extension): free-length-from-geometry helper for the optimizer"
```

---

## Task 2: Optimizer module — types, mass, binding search, driver

Builds the whole optimizer end-to-end with structural tests (feasibility, rate met, global minimum, infeasibility). The exact-binding-discrimination tests, exact-value pins, boundary cases, and mutation hardening come in Task 3 (they need the built optimizer to calibrate inputs).

**Files:**
- Create: `springcore/src/extension/optimize.rs`
- Modify: `springcore/src/extension/mod.rs`

**Interfaces:**
- Consumes: every function in "Existing functions this plan reuses", plus `free_length_from_geometry` (Task 1).
- Produces:
  - `pub enum HookSpec { Default, Fixed { r1: Length, r2: Length } }` with `pub fn resolve(self, mean_dia: Length) -> HookEnds`.
  - `pub enum ExtBindingConstraint { BodyShear, HookBending, HookTorsion, Index, OuterDiameter }` (derives `Debug, Clone, Copy, PartialEq, Eq`).
  - `pub struct ExtMinWeightRequest { required_rate: SpringRate, max_force: Force, initial_tension: Force, hooks: HookSpec, index_bounds: (f64, f64), max_outer_dia: Option<Length>, candidate_diameters: Vec<Length> }` (derives `Debug, Clone`).
  - `pub struct ExtMinWeightSolution { design: ExtensionDesign, binding: ExtBindingConstraint, mass_kg: f64 }` (derives `Debug, Clone`).
  - `pub fn solve_min_weight(material: &Material, req: &ExtMinWeightRequest, correction: CurvatureCorrection) -> Result<ExtMinWeightSolution>`.

- [ ] **Step 1: Create the module file** `springcore/src/extension/optimize.rs` with the header, imports, and public types:

```rust
//! Minimum-weight constrained optimization for extension springs.
//! Figure of merit = wire mass. For a given wire size the optimum mean diameter
//! is the largest that keeps all three stresses — body shear, hook bending (σ_A),
//! and hook torsion (τ_B) — within allowable, so it lies on the binding stress,
//! the index ceiling, or the outer-diameter cap. Mirrors `crate::optimize`
//! (compression) without buckling/solid-length (an extension spring is loaded in
//! tension). See `docs/superpowers/specs/2026-06-26-extension-min-weight-design.md`.

use crate::extension::design::{solve_forward, ExtensionDesign};
use crate::extension::ends::HookEnds;
use crate::extension::mechanics::{
    free_length_from_geometry, hook_bending_stress, hook_torsion_stress,
};
use crate::material::Material;
use crate::mechanics::{active_coils_for_rate, corrected_shear_stress};
use crate::numeric::{find_root_bracketed, SolveConfig};
use crate::units::{Force, Length, SpringRate};
use crate::CurvatureCorrection;
use crate::{Result, SpringError};
use std::f64::consts::PI;

/// How the hook geometry is determined during the search.
#[derive(Debug, Clone, Copy)]
pub enum HookSpec {
    /// Standard machine loops that scale with the mean diameter: r1 = D/2, r2 = D/4.
    Default,
    /// Fixed absolute bend radii, independent of D.
    Fixed { r1: Length, r2: Length },
}

impl HookSpec {
    /// Resolve the concrete hook radii for a given mean diameter.
    pub fn resolve(self, mean_dia: Length) -> HookEnds {
        match self {
            HookSpec::Default => HookEnds::default_for(mean_dia),
            HookSpec::Fixed { r1, r2 } => HookEnds { r1, r2 },
        }
    }
}

/// Which limit determines the chosen extension design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtBindingConstraint {
    BodyShear,
    HookBending,
    HookTorsion,
    Index,
    OuterDiameter,
}

/// A minimum-weight extension-spring problem.
#[derive(Debug, Clone)]
pub struct ExtMinWeightRequest {
    pub required_rate: SpringRate,
    pub max_force: Force,
    /// Built-in preload. Passthrough: validated (>= 0, finite) and reported, but it
    /// does not affect the mass, the stresses, or the binding constraint.
    pub initial_tension: Force,
    pub hooks: HookSpec,
    pub index_bounds: (f64, f64),
    pub max_outer_dia: Option<Length>,
    pub candidate_diameters: Vec<Length>,
}

/// The chosen design and why it is limited.
#[derive(Debug, Clone)]
pub struct ExtMinWeightSolution {
    pub design: ExtensionDesign,
    pub binding: ExtBindingConstraint,
    pub mass_kg: f64,
}
```

- [ ] **Step 2: Add the mass helper** to `optimize.rs`:

```rust
/// Wire mass of a design: rho * (pi·d²/4) * L_wire, with developed wire length
/// L_wire = pi·D·Na (body) + 2·(pi·d_loop) (two hook loops), d_loop = 2·r1.
/// This is the Acxess Spring developed-length model `Li = pi·D·(N + 2)` (each
/// machine hook ≈ one mean coil) generalized so a fixed hook of radius r1
/// contributes a loop of its own mean diameter d_loop = 2·r1.
fn wire_mass(material: &Material, wire_dia: Length, mean_dia: Length, active: f64, hooks: HookEnds) -> f64 {
    let d = wire_dia.meters();
    let dm = mean_dia.meters();
    let d_loop = 2.0 * hooks.r1.meters();
    let l_wire = PI * dm * active + 2.0 * PI * d_loop;
    material.density.kg_per_m3() * (PI / 4.0) * d.powi(2) * l_wire
}
```

- [ ] **Step 3: Add the three-stress binding search** to `optimize.rs`:

```rust
/// Largest feasible mean diameter for a wire size, and which limit binds.
///
/// Each of the three stresses (body shear, hook bending σ_A, hook torsion τ_B)
/// is monotone increasing in D over [c_min·d, c_max·d] when c_min is at or above
/// the per-factor turning point (compression documents the shear's U-shape with a
/// minimum at C* ≈ 1.866 for Wahl / ≈ 1.718 for Bergsträsser; the hook factors are
/// likewise monotone for the practical C ≥ 4 range — the index floor enforces it).
/// Under that assumption each stress imposes an upper bound on D via a single
/// bracketed root, and the feasible D is the minimum of the three bounds and the
/// index ceiling. The single-endpoint feasibility test (`stress(dm_lo) > allowable
/// → infeasible`) is valid only under this monotonicity.
fn best_mean_dia(
    material: &Material,
    d: Length,
    max_force: Force,
    bounds: (f64, f64),
    hooks: HookSpec,
    correction: CurvatureCorrection,
) -> Option<(Length, ExtBindingConstraint)> {
    let (c_min, c_max) = bounds;
    let mts = material.min_tensile_strength(d).ok()?.pascals();
    let allow_torsion = material.allowable_pct_torsion * mts;
    let allow_bending = material.allowable_pct_bending * mts;
    let dm_lo = c_min * d.meters();
    let dm_hi = c_max * d.meters();

    // Stress closures as functions of the mean diameter (in metres). Hooks are
    // resolved per D so default hooks scale (r1 = D/2, r2 = D/4).
    let body = |dm_m: f64| {
        let dm = Length::from_meters(dm_m);
        let c = dm_m / d.meters();
        corrected_shear_stress(max_force, dm, d, correction.factor(c)).pascals()
    };
    let bending = |dm_m: f64| {
        let dm = Length::from_meters(dm_m);
        hook_bending_stress(max_force, dm, d, hooks.resolve(dm).r1).pascals()
    };
    let torsion = |dm_m: f64| {
        let dm = Length::from_meters(dm_m);
        hook_torsion_stress(max_force, dm, d, hooks.resolve(dm).r2).pascals()
    };

    // Per-stress upper bound on D: None if it overstresses even at the smallest
    // index (candidate infeasible); (dm_hi, Index) if it never reaches allowable
    // (the index ceiling limits, not this stress); else the bracketed root where
    // stress == allowable, labeled with this stress.
    let bound_for = |stress: &dyn Fn(f64) -> f64,
                     allowable: f64,
                     label: ExtBindingConstraint|
     -> Option<(f64, ExtBindingConstraint)> {
        if stress(dm_lo) - allowable > 0.0 {
            return None;
        }
        if stress(dm_hi) - allowable <= 0.0 {
            return Some((dm_hi, ExtBindingConstraint::Index));
        }
        let root = find_root_bracketed(
            |dm| stress(dm) - allowable,
            dm_lo,
            dm_hi,
            SolveConfig::default(),
        )
        .ok()?;
        Some((root, label))
    };

    let candidates = [
        bound_for(&body, allow_torsion, ExtBindingConstraint::BodyShear)?,
        bound_for(&bending, allow_bending, ExtBindingConstraint::HookBending)?,
        bound_for(&torsion, allow_torsion, ExtBindingConstraint::HookTorsion)?,
    ];
    // The smallest upper bound binds. (partial_cmp is safe: all values are finite
    // here — dm_lo/dm_hi are finite and any root lies between them.)
    let (dm, binding) = candidates
        .into_iter()
        .min_by(|a, b| a.0.partial_cmp(&b.0).expect("finite mean-diameter bounds"))?;
    Some((Length::from_meters(dm), binding))
}
```

- [ ] **Step 4: Add the driver** `solve_min_weight` to `optimize.rs`:

```rust
/// Solve the minimum-weight extension-spring problem.
pub fn solve_min_weight(
    material: &Material,
    req: &ExtMinWeightRequest,
    correction: CurvatureCorrection,
) -> Result<ExtMinWeightSolution> {
    let (c_min, _c_max) = req.index_bounds;
    let mut best: Option<ExtMinWeightSolution> = None;

    for &d in &req.candidate_diameters {
        let Some((mut mean, mut binding)) =
            best_mean_dia(material, d, req.max_force, req.index_bounds, req.hooks, correction)
        else {
            continue;
        };
        // Optional outer-diameter cap (mirrors compression).
        if let Some(od_max) = req.max_outer_dia {
            if mean.meters() + d.meters() > od_max.meters() {
                let capped = od_max.meters() - d.meters();
                if capped / d.meters() < c_min {
                    continue; // capping would push the index below the floor
                }
                mean = Length::from_meters(capped);
                binding = ExtBindingConstraint::OuterDiameter;
            }
        }
        let active = active_coils_for_rate(material.shear_modulus, d, mean, req.required_rate);
        if !active.is_finite() || active < 1.0 {
            continue; // non-finite or fewer than one active coil is unphysical
        }
        let hooks = req.hooks.resolve(mean);
        let free_length = free_length_from_geometry(d, active, hooks);
        let design = solve_forward(
            material,
            d,
            mean,
            active,
            free_length,
            req.initial_tension,
            hooks,
            &[req.max_force],
            correction,
        )?;
        let mass = wire_mass(material, d, mean, active, hooks);
        if best.as_ref().map(|b| mass < b.mass_kg).unwrap_or(true) {
            best = Some(ExtMinWeightSolution {
                design,
                binding,
                mass_kg: mass,
            });
        }
    }

    best.ok_or_else(|| {
        SpringError::Infeasible("no candidate diameter satisfies the constraints".into())
    })
}
```

- [ ] **Step 5: Wire the module in** `springcore/src/extension/mod.rs`. Add `mod optimize;` with the other `mod` declarations, and extend the re-exports. After this task the public surface is:

```rust
pub use optimize::{
    solve_min_weight, ExtBindingConstraint, ExtMinWeightRequest, ExtMinWeightSolution, HookSpec,
};
```

(Place this `pub use` next to the existing `pub use design::...` / `pub use scenario::...` lines, alphabetically by module is fine.)

- [ ] **Step 6: Write the structural tests** in a `#[cfg(test)] mod tests` block at the bottom of `optimize.rs`. These assert feasibility and the global-minimum property without depending on which specific constraint binds (those discrimination tests are Task 3):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{Force, Length, SpringRate};
    use approx::assert_relative_eq;

    /// Default-hook request over a set of candidate wire diameters (mm).
    fn base_request(candidates: Vec<f64>) -> ExtMinWeightRequest {
        ExtMinWeightRequest {
            required_rate: SpringRate::from_newtons_per_meter(2000.0),
            max_force: Force::from_newtons(50.0),
            initial_tension: Force::from_newtons(10.0),
            hooks: HookSpec::Default,
            index_bounds: (4.0, 12.0),
            max_outer_dia: None,
            candidate_diameters: candidates
                .into_iter()
                .map(Length::from_millimeters)
                .collect(),
        }
    }

    #[test]
    fn solution_is_feasible() {
        let m = crate::test_support::music_wire();
        let sol = solve_min_weight(
            &m,
            &base_request(vec![1.5, 2.0, 2.5, 3.0]),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        // Rate met.
        assert_relative_eq!(sol.design.rate.newtons_per_meter(), 2000.0, max_relative = 1e-6);
        // All three stresses within allowable at the operating load.
        let lp = &sol.design.load_points[0];
        assert!(lp.pct_body_allow <= 1.0 + 1e-6);
        assert!(lp.pct_hook_bending_allow <= 1.0 + 1e-6);
        assert!(lp.pct_hook_torsion_allow <= 1.0 + 1e-6);
        // Index within bounds; positive mass.
        assert!(sol.design.index >= 4.0 - 1e-9 && sol.design.index <= 12.0 + 1e-9);
        assert!(sol.mass_kg > 0.0);
    }

    #[test]
    fn picks_global_minimum_over_candidates() {
        let m = crate::test_support::music_wire();
        let candidates = vec![1.5, 2.0, 2.5, 3.0];
        let per: Vec<f64> = candidates
            .iter()
            .filter_map(|&d| {
                solve_min_weight(&m, &base_request(vec![d]), CurvatureCorrection::Bergstrasser)
                    .ok()
                    .map(|s| s.mass_kg)
            })
            .collect();
        let best = solve_min_weight(
            &m,
            &base_request(candidates),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        let min = per.iter().cloned().fold(f64::INFINITY, f64::min);
        assert_relative_eq!(best.mass_kg, min, max_relative = 1e-9);
    }

    #[test]
    fn infeasible_when_outer_diameter_too_small() {
        let m = crate::test_support::music_wire();
        let mut req = base_request(vec![1.5, 2.0, 2.5]);
        req.max_outer_dia = Some(Length::from_millimeters(3.0)); // forces index < 4
        assert!(matches!(
            solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser),
            Err(SpringError::Infeasible(_))
        ));
    }
}
```

- [ ] **Step 7: Run the tests — expect pass.**

Run: `cargo test -p springcore --lib extension::optimize 2>&1 | tail -6`
Expected: `test result: ok. 3 passed` (plus the whole springcore suite stays green: `cargo test -p springcore 2>&1 | tail -3`).

- [ ] **Step 8: fmt + clippy.**

Run: `cargo fmt && cargo clippy -p springcore --all-targets -- -D warnings 2>&1 | tail -2`
Expected: clean.

- [ ] **Step 9: Commit.**

```bash
git add springcore/src/extension/optimize.rs springcore/src/extension/mod.rs
git commit -m "feat(extension): minimum-weight optimizer (three-stress binding search)"
```

---

## Task 3: Binding discrimination, exact pins, boundaries, F_i passthrough, hooks, mutation

Adds the tests that pin behavior precisely. The exact request inputs that make a *specific* constraint bind, and the exact `mean_dia`/`mass_kg`/`free_length` values, are obtained by the implementer using the optimizer built in Task 2 plus an independent recomputation (a short Python script from the cited formulas, exactly as the compression optimizer's exact-value tests were derived). This task adds tests only (plus any pinning assertion needed to kill a surviving mutant); it changes no production logic except to add a pinning assertion if Task 2's code has an unpinned mutant.

**Files:**
- Modify: `springcore/src/extension/optimize.rs` (test module only, unless a mutant requires a pinning tweak).

**Calibration method (read first).** To make a chosen constraint bind, exploit the stress ordering: with default hooks the hook bending σ_A is typically the highest stress and the body shear the lowest, so:
- **HookBending binding:** default hooks, a load high enough that σ_A reaches allowable before the index ceiling (raise `max_force` / shrink the index ceiling).
- **HookTorsion binding:** default hooks but enlarge `r1` via `HookSpec::Fixed { r1: large, r2: small }` so σ_A drops below τ_B while τ_B still binds before the index ceiling.
- **BodyShear binding:** `HookSpec::Fixed` with large `r1` and large `r2` so both hook stresses fall below the body shear, which then binds.
- **Index binding:** a low `max_force` so no stress reaches allowable within the index range (bound = `dm_hi`).
- **OuterDiameter binding:** any feasible request plus a `max_outer_dia` below the uncapped OD (as compression's OD-cap test does).

For each, construct the request, run `solve_min_weight`, and assert `sol.binding == <variant>`. Obtain the exact `mean_dia`/`mass_kg`/`free_length` pins from an independent Python recomputation of the cited formulas at the solved geometry.

- [ ] **Step 1: Write the five per-binding tests.** One test per `ExtBindingConstraint` variant asserting `sol.binding`. Use the calibration method above; verify each by running the test and confirming the intended variant binds (adjust inputs until it does). Each test must also assert the design is feasible (all three `pct_*_allow <= 1.0 + 1e-6`). Example skeleton for the index-binding case (low force):

```rust
    #[test]
    fn low_force_binds_index() {
        let m = crate::test_support::music_wire();
        let mut req = base_request(vec![3.0]);
        req.max_force = Force::from_newtons(5.0); // far below any stress limit at c_max
        let sol = solve_min_weight(&m, &req, CurvatureCorrection::Bergstrasser).unwrap();
        assert_eq!(sol.binding, ExtBindingConstraint::Index);
        // Index ceiling ⇒ mean diameter = c_max · d = 12 · 3 mm = 36 mm.
        assert_relative_eq!(sol.design.mean_dia.millimeters(), 36.0, max_relative = 1e-9);
    }
```

Write `body_shear_binds`, `hook_bending_binds`, `hook_torsion_binds`, and `od_cap_binds` analogously, each asserting the variant and (where the geometry is determined, e.g. index/OD cap) the exact mean diameter.

- [ ] **Step 2: Write the exact-value pins.** For one fully-determined geometry (e.g. the index-binding `low_force_binds_index` case, where `mean_dia = c_max·d`), pin `mass_kg` and `free_length` to independently-computed values so arithmetic mutations in `wire_mass` and `free_length_from_geometry` change the result. Compute the expected numbers in Python from the formulas:

```
Na = G·d⁴ / (8·D³·k);  d_loop = 2·r1 = D (default);  Nb = Na
L0 = 2·(d_loop − d) + (Na + 1)·d
L_wire = π·D·Na + 2·π·d_loop
mass = ρ·(π·d²/4)·L_wire
```

Add the assertions to the relevant test (or a dedicated `index_binding_mass_and_free_length_exact`), with a comment showing the Python-computed values and inputs, mirroring `optimize.rs`'s compression exact-value tests. Use `max_relative = 1e-6`.

- [ ] **Step 3: Write the F_i passthrough test** — the spec's defining invariant:

```rust
    #[test]
    fn initial_tension_is_passthrough_only() {
        let m = crate::test_support::music_wire();
        let mut lo = base_request(vec![2.0, 2.5, 3.0]);
        lo.initial_tension = Force::from_newtons(0.0);
        let mut hi = lo.clone();
        hi.initial_tension = Force::from_newtons(20.0);
        let a = solve_min_weight(&m, &lo, CurvatureCorrection::Bergstrasser).unwrap();
        let b = solve_min_weight(&m, &hi, CurvatureCorrection::Bergstrasser).unwrap();
        // Mass, binding, and geometry are identical regardless of F_i.
        assert_relative_eq!(a.mass_kg, b.mass_kg, max_relative = 1e-12);
        assert_eq!(a.binding, b.binding);
        assert_relative_eq!(a.design.wire_dia.millimeters(), b.design.wire_dia.millimeters(), max_relative = 1e-12);
        assert_relative_eq!(a.design.mean_dia.millimeters(), b.design.mean_dia.millimeters(), max_relative = 1e-12);
        // But the reported preload differs (proving it flows into the final design).
        assert_relative_eq!(a.design.initial_tension.newtons(), 0.0, max_relative = 1e-12);
        assert_relative_eq!(b.design.initial_tension.newtons(), 20.0, max_relative = 1e-12);
    }
```

- [ ] **Step 4: Write the hook-spec tests** — fixed hooks reproduce default at the same radii, and a different `r1` changes mass + free length:

```rust
    #[test]
    fn fixed_hooks_reproduce_default_at_same_radii() {
        let m = crate::test_support::music_wire();
        // Single candidate so both runs pick the same D; compare full result.
        let mut def = base_request(vec![3.0]);
        def.max_force = Force::from_newtons(5.0); // index-binding ⇒ D = 36mm deterministically
        let d_sol = solve_min_weight(&m, &def, CurvatureCorrection::Bergstrasser).unwrap();
        // At D = 36mm the default hook is r1 = 18mm, r2 = 9mm. Pin fixed hooks to those.
        let mut fixed = def.clone();
        fixed.hooks = HookSpec::Fixed {
            r1: Length::from_millimeters(18.0),
            r2: Length::from_millimeters(9.0),
        };
        let f_sol = solve_min_weight(&m, &fixed, CurvatureCorrection::Bergstrasser).unwrap();
        assert_relative_eq!(d_sol.mass_kg, f_sol.mass_kg, max_relative = 1e-9);
        assert_relative_eq!(
            d_sol.design.free_length.millimeters(),
            f_sol.design.free_length.millimeters(),
            max_relative = 1e-9
        );
    }

    #[test]
    fn larger_fixed_loop_increases_mass_and_free_length() {
        let m = crate::test_support::music_wire();
        let mut small = base_request(vec![3.0]);
        small.max_force = Force::from_newtons(5.0);
        small.hooks = HookSpec::Fixed {
            r1: Length::from_millimeters(18.0),
            r2: Length::from_millimeters(9.0),
        };
        let mut large = small.clone();
        large.hooks = HookSpec::Fixed {
            r1: Length::from_millimeters(30.0), // bigger loop ⇒ more wire, longer L0
            r2: Length::from_millimeters(9.0),
        };
        let s = solve_min_weight(&m, &small, CurvatureCorrection::Bergstrasser).unwrap();
        let l = solve_min_weight(&m, &large, CurvatureCorrection::Bergstrasser).unwrap();
        assert!(l.mass_kg > s.mass_kg);
        assert!(l.design.free_length.millimeters() > s.design.free_length.millimeters());
    }
```

(Note: a larger fixed `r1` lowers σ_A, so confirm the `large` case still binds and stays feasible; if the larger loop changes the binding, keep `max_force` low enough that index binds in both so `D` is identical and only the hook term differs.)

- [ ] **Step 5: Run the full optimizer test set — expect pass.**

Run: `cargo test -p springcore --lib extension::optimize 2>&1 | tail -8`
Expected: all green.

- [ ] **Step 6: Mutation hardening.** Generate the branch diff and run in-diff mutation testing scoped to springcore:

Run:
```bash
git add -A && git commit -m "test(extension): pin min-weight bindings, mass, free length, F_i passthrough"
git diff main...HEAD -- springcore/ > /tmp/extmw.diff
cargo mutants --in-diff /tmp/extmw.diff -p springcore 2>&1 | tail -15
```
Expected: `0 survivors`. For any survivor, add a targeted pinning assertion (an exact-value pin on the affected arithmetic, or a boundary test for a `<`/`<=`/`>` comparison — e.g. an OD-exactly-equal-to-`max_outer_dia` test to pin the strict `>` in the cap, and an `active`-exactly-`1.0` test to pin the `< 1.0` guard, mirroring the compression optimizer's boundary tests), then `git commit --amend` and re-run until clean.

- [ ] **Step 7: fmt + clippy + final commit (if not already amended).**

Run: `cargo fmt && cargo clippy -p springcore --all-targets -- -D warnings 2>&1 | tail -2`
Expected: clean. Ensure all work is committed.

---

## Final verification (before opening the PR)

- [ ] Full gate: `cargo test --workspace`; `cargo fmt --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`; `typos`; `cargo deny check all`.
- [ ] Mutation: `git diff main...HEAD -- springcore/ > /tmp/extmw.diff && cargo mutants --in-diff /tmp/extmw.diff -p springcore` — 0 survivors.
- [ ] If `typos` flags "Acxess" (the cited vendor name) or "Bergsträsser", add it to the repo `typos` allow-list (`_typos.toml`) rather than altering the citation/term.
- [ ] Confirm `springmaker` is untouched (`git diff --stat main...HEAD` shows only `springcore/extension/` + the spec/plan docs).

---

## Self-review notes

- **Spec coverage:** Algorithm/three-stress binding search (Task 2 Step 3); mass model §4 (Task 2 Step 2, cited Acxess Spring); free-length §5 (Task 1); F_i passthrough (Task 3 Step 3); `ExtBindingConstraint` five variants (Task 2 Step 1 + Task 3 Step 1 per-binding tests); HookSpec Default/Fixed (Task 2 Step 1 + Task 3 Step 4); OD cap + active guard + Infeasible (Task 2 Step 4); input validation is delegated to `solve_forward`'s existing guards (finite/positive wire, mean>wire, free_length>0, F_i≥0, loads≥0, hook C1/C2>1) plus the optimizer's own `active < 1`, `c_min` floor, and OD-cap checks. Testing §6 (Tasks 2–3).
- **Placeholder scan:** Task 1 and Task 2 carry complete code. Task 3's per-binding inputs and exact pins are intentionally calibrated by the implementer against the built optimizer + an independent Python recomputation — the same method the compression optimizer's exact-value tests used; the method, the binding-discrimination knobs, and one fully-worked skeleton are given, so there is no hidden "fill in details" step.
- **Type consistency:** `solve_min_weight`, `best_mean_dia`, `wire_mass`, `free_length_from_geometry`, `HookSpec::resolve`, and the four public types use one set of signatures across Tasks 1–3 and match the existing reused functions' signatures verbatim.

## Out of scope (this plan)

- `springmaker` GUI for extension min-weight.
- Manufacturable-preferred-initial-tension advisory; hook/body fatigue in the optimizer; loop *styles* beyond the radius-parameterized loop (per the spec's Out-of-Scope section).
