# Extension Springs — Phase 1 Implementation Plan (static core + PowerUser)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the static engine core for helical **extension** springs in `springcore` — rate, initial-tension deflection, body shear stress, and the two critical hook stresses — exposed through a `PowerUser` input mode and validated by a golden worked example.

**Architecture:** A new `springcore/src/extension/` module (Approach A from the spec), parallel to the flat compression files, with its own `Scenario` trait and an `ExtensionDesign` output type. It **reuses** the compression mechanics that are identical (`spring_rate`, `corrected_shear_stress`, `wahl_factor`, `spring_index`, `Material`, `units`) and adds only the extension-specific math (hook curvature factors, hook stresses, initial-tension deflection). No compression code changes.

**Tech Stack:** Rust 2021, MSRV 1.88; `springcore` (no GUI deps); `approx` for float asserts (dev-dep); formulas from Shigley's *Mechanical Engineering Design* (cross-checked vs SMI / Wahl).

**Scope of THIS plan:** Phase 1 only (spec §8 item 1). Phases 2–6 (other input modes, min-weight, GUI, body fatigue, hook fatigue) each get their own plan, written when their turn comes. Phase 1 ships working, testable extension statics behind `PowerUser`.

## Global Constraints

- No commercial-product/vendor references in any persisted file (legal).
- Every formula/constant cited inline (Shigley equation/section, verbatim where possible).
- Strict TDD: write the failing test, watch it fail, implement minimally, watch it pass, commit.
- SI canonical internally; the engine takes/returns `units` newtypes (`Length`, `Force`, `Stress`, `SpringRate`).
- Pre-push gates all green: `cargo test --workspace`, `cargo clippy --all-targets --all-features -D warnings`, `cargo fmt --check`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features`, repo-wide `typos`, `cargo deny --all-features check all`, `cargo mutants --in-diff <diff> --package springcore` (0 missed).
- Mandatory adversarial review panel to convergence before every push; branch + PR; never push without permission.
- Each task is its own commit; the whole phase is one PR (`feat/extension-springs`, off `main`).

## Reference patterns (existing compression code to mirror — do NOT modify it)

- `springcore/src/mechanics.rs` — `spring_rate(shear_modulus: Stress, wire_dia: Length, mean_dia: Length, active: f64) -> SpringRate`, `spring_index(mean_dia, wire_dia) -> f64`, `wahl_factor(index: f64) -> f64`, `corrected_shear_stress(force: Force, mean_dia: Length, wire_dia: Length, factor: f64) -> Stress`. **Reuse these directly** (`crate::mechanics::…`).
- `springcore/src/design.rs` — `SpringDesign`, `LoadPoint`, `solve_forward(...)`. Mirror the *shape*; extension has its own type.
- `springcore/src/scenario.rs` — `Scenario` trait + `PowerUser`. Mirror for `extension`.
- `springcore/src/material.rs` — `Material` fields used here (verified): `shear_modulus: Stress`, `min_tensile_strength(wire_dia: Length) -> Result<Stress>`, and the allowable fractions `allowable_pct_torsion: f64`, `allowable_pct_bending: f64`. `crate::test_support::music_wire()` (a `pub(crate) fn`, in-crate unit tests only) is the test fixture.
- `springcore/src/units.rs` — `Length::{from_meters,from_millimeters,meters,millimeters}`, `Force::{from_newtons,newtons}`, `Stress::{from_pascals,pascals,from_megapascals,megapascals}`, `SpringRate::{from_newtons_per_meter,newtons_per_meter}`.
- `springcore/tests/golden.rs` — golden-test style (transcribed worked example + `assert_relative_eq!`).
- `springcore/src/error.rs` — `SpringError::InconsistentInputs(String)`.

## File Structure

- Create `springcore/src/extension/mod.rs` — module surface; `pub mod`s + re-exports.
- Create `springcore/src/extension/mechanics.rs` — hook curvature factors, hook stresses, initial-tension deflection. Unit tests inline.
- Create `springcore/src/extension/ends.rs` — `HookEnds { r1: Length, r2: Length }` with `default_for(mean_dia)`. Unit tests inline.
- Create `springcore/src/extension/design.rs` — `ExtensionDesign`, `ExtLoadPoint`, `solve_forward(...)`. Unit tests inline.
- Create `springcore/src/extension/scenario.rs` — `extension::Scenario` trait + `PowerUser`. Unit tests inline.
- Modify `springcore/src/lib.rs` — add `pub mod extension;` (Task 1) and re-exports (Task 5).
- Modify `springcore/tests/golden.rs` — add the extension golden test (Task 6).

---

### Task 1: Module scaffolding + hook curvature factors

**Files:**
- Create: `springcore/src/extension/mod.rs`, `springcore/src/extension/mechanics.rs`
- Modify: `springcore/src/lib.rs` (add `pub mod extension;`)

**Interfaces — Produces:**
- `extension::mechanics::hook_bending_factor(c1: f64) -> f64`
- `extension::mechanics::hook_torsion_factor(c2: f64) -> f64`

These are the curvature corrections at the hook bends. Source: Shigley extension-spring section.
- `(K)_A = (4·C1² − C1 − 1) / (4·C1·(C1 − 1))`, where `C1 = 2·r1/d`.
- `(K)_B = (4·C2 − 1) / (4·C2 − 4)`, where `C2 = 2·r2/d`.

- [ ] **Step 1: Write failing tests** in `springcore/src/extension/mechanics.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn hook_bending_factor_c1_10() {
        // (4·100 − 10 − 1)/(4·10·9) = 389/360.
        assert_relative_eq!(hook_bending_factor(10.0), 389.0 / 360.0, max_relative = 1e-12);
    }

    #[test]
    fn hook_torsion_factor_c2_5() {
        // (20 − 1)/(20 − 4) = 19/16.
        assert_relative_eq!(hook_torsion_factor(5.0), 19.0 / 16.0, max_relative = 1e-12);
    }
}
```

- [ ] **Step 2: Run, verify fail**
Run: `cargo test -p springcore extension::mechanics 2>&1 | tail`
Expected: FAIL — module/functions not found.

- [ ] **Step 3: Implement** — create `extension/mod.rs`:
```rust
//! Helical extension springs (round wire). Parallel to the compression family;
//! reuses `units`, `material`, and the identical `mechanics::spring_rate` /
//! `corrected_shear_stress`. Formula sources cited at each call site.

pub mod mechanics;
```
Add `pub mod extension;` to `springcore/src/lib.rs` (alongside the other `pub mod` lines). Create `extension/mechanics.rs`:
```rust
//! Extension-spring-specific mechanics: hook curvature factors and stresses,
//! and initial-tension deflection. Body rate/stress reuse `crate::mechanics`.

/// Hook bending curvature factor at point A (Shigley, extension springs):
/// (K)_A = (4·C1² − C1 − 1) / (4·C1·(C1 − 1)), with C1 = 2·r1/d.
pub fn hook_bending_factor(c1: f64) -> f64 {
    (4.0 * c1 * c1 - c1 - 1.0) / (4.0 * c1 * (c1 - 1.0))
}

/// Hook torsion curvature factor at point B (Shigley, extension springs):
/// (K)_B = (4·C2 − 1) / (4·C2 − 4), with C2 = 2·r2/d.
pub fn hook_torsion_factor(c2: f64) -> f64 {
    (4.0 * c2 - 1.0) / (4.0 * c2 - 4.0)
}
```

- [ ] **Step 4: Run, verify pass**
Run: `cargo test -p springcore extension::mechanics 2>&1 | tail`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**
```bash
git add springcore/src/extension/mod.rs springcore/src/extension/mechanics.rs springcore/src/lib.rs
git commit -m "feat(extension): module scaffolding + hook curvature factors"
```

---

### Task 2: Hook stresses (bending σ_A, torsion τ_B)

**Files:** Modify `springcore/src/extension/mechanics.rs`

**Interfaces — Consumes:** `hook_bending_factor`, `hook_torsion_factor` (Task 1); `crate::units::{Force, Length, Stress}`.
**Produces:**
- `hook_bending_stress(force: Force, mean_dia: Length, wire_dia: Length, r1: Length) -> Stress`
- `hook_torsion_stress(force: Force, mean_dia: Length, wire_dia: Length, r2: Length) -> Stress`

Formulas (Shigley, F = actual force, all SI):
- `σ_A = F·[ (K)_A·(16·D)/(π·d³) + 4/(π·d²) ]`, `C1 = 2·r1/d`.
- `τ_B = (K)_B·(8·F·D)/(π·d³)`, `C2 = 2·r2/d`.

- [ ] **Step 1: Write failing tests** (append to the `tests` mod). Oracle values hand-computed for F=100 N, D=20 mm, d=2 mm, r1=10 mm (C1=10, Ka=389/360), r2=5 mm (C2=5, Kb=19/16); **verify on first run**:
```rust
    #[test]
    fn hook_bending_stress_matches_hand_calc() {
        let s = hook_bending_stress(
            Force::from_newtons(100.0),
            Length::from_millimeters(20.0),
            Length::from_millimeters(2.0),
            Length::from_millimeters(10.0),
        );
        // σ_A = 100·[ (389/360)·16·0.02/(π·0.002³) + 4/(π·0.002²) ] ≈ 1.4076e9 Pa.
        assert_relative_eq!(s.pascals(), 1.40765e9, max_relative = 1e-4);
    }

    #[test]
    fn hook_torsion_stress_matches_hand_calc() {
        let s = hook_torsion_stress(
            Force::from_newtons(100.0),
            Length::from_millimeters(20.0),
            Length::from_millimeters(2.0),
            Length::from_millimeters(5.0),
        );
        // τ_B = (19/16)·8·100·0.02/(π·0.002³) ≈ 7.560e8 Pa.
        assert_relative_eq!(s.pascals(), 7.5599e8, max_relative = 1e-4);
    }
```
Add `use crate::units::{Force, Length, Stress};` at the top of the file (and `use std::f64::consts::PI;`).

- [ ] **Step 2: Run, verify fail.** `cargo test -p springcore extension::mechanics 2>&1 | tail` → FAIL (fns not found). If the *oracle* is what's wrong (not the impl), recompute from the formula and correct the literal — do not loosen tolerance below 1e-4.

- [ ] **Step 3: Implement** in `extension/mechanics.rs`:
```rust
use crate::units::{Force, Length, Stress};
use std::f64::consts::PI;

/// Hook bending stress at point A (Shigley): σ_A = F[(K)_A·16D/(πd³) + 4/(πd²)].
pub fn hook_bending_stress(force: Force, mean_dia: Length, wire_dia: Length, r1: Length) -> Stress {
    let (f, d, dia) = (force.newtons(), wire_dia.meters(), mean_dia.meters());
    let c1 = 2.0 * r1.meters() / d;
    let ka = hook_bending_factor(c1);
    let sigma = f * (ka * 16.0 * dia / (PI * d.powi(3)) + 4.0 / (PI * d * d));
    Stress::from_pascals(sigma)
}

/// Hook torsional stress at point B (Shigley): τ_B = (K)_B·8FD/(πd³).
pub fn hook_torsion_stress(force: Force, mean_dia: Length, wire_dia: Length, r2: Length) -> Stress {
    let (f, d, dia) = (force.newtons(), wire_dia.meters(), mean_dia.meters());
    let c2 = 2.0 * r2.meters() / d;
    let kb = hook_torsion_factor(c2);
    Stress::from_pascals(kb * 8.0 * f * dia / (PI * d.powi(3)))
}
```

- [ ] **Step 4: Run, verify pass.** `cargo test -p springcore extension::mechanics` → PASS (4 tests).
- [ ] **Step 5: Commit**
```bash
git add springcore/src/extension/mechanics.rs
git commit -m "feat(extension): hook bending and torsion stresses"
```

---

### Task 3: Initial-tension deflection + `HookEnds`

**Files:** Modify `springcore/src/extension/mechanics.rs`; Create `springcore/src/extension/ends.rs`; Modify `extension/mod.rs` (add `pub mod ends;`).

**Interfaces — Produces:**
- `extension::mechanics::deflection(force: Force, initial_tension: Force, rate: SpringRate) -> Length` — `y = max(0, (F − F_i)/k)` (coils don't separate until F exceeds initial tension; Shigley).
- `extension::ends::HookEnds { pub r1: Length, pub r2: Length }` with `HookEnds::default_for(mean_dia: Length) -> Self` → `r1 = D/2`, `r2 = D/4` (spec default).

- [ ] **Step 1: Write failing tests.** In `extension/mechanics.rs` tests:
```rust
    #[test]
    fn deflection_zero_below_initial_tension() {
        let y = deflection(
            Force::from_newtons(5.0),
            Force::from_newtons(10.0),
            SpringRate::from_newtons_per_meter(2000.0),
        );
        assert_relative_eq!(y.meters(), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn deflection_above_initial_tension() {
        // (30 − 10)/2000 = 0.01 m = 10 mm.
        let y = deflection(
            Force::from_newtons(30.0),
            Force::from_newtons(10.0),
            SpringRate::from_newtons_per_meter(2000.0),
        );
        assert_relative_eq!(y.millimeters(), 10.0, max_relative = 1e-9);
    }
```
In a new `extension/ends.rs` test:
```rust
    #[test]
    fn default_hook_radii_are_half_and_quarter_mean() {
        let h = HookEnds::default_for(Length::from_millimeters(20.0));
        assert_relative_eq!(h.r1.millimeters(), 10.0, max_relative = 1e-12);
        assert_relative_eq!(h.r2.millimeters(), 5.0, max_relative = 1e-12);
    }
```

- [ ] **Step 2: Run, verify fail.** `cargo test -p springcore extension:: 2>&1 | tail` → FAIL.

- [ ] **Step 3: Implement.** Add `SpringRate` to the `units` import in `mechanics.rs`, then:
```rust
/// Extension deflection at a load: y = max(0, (F − F_i)/k). Coils stay closed
/// (no deflection) until the force exceeds the built-in initial tension (Shigley).
pub fn deflection(force: Force, initial_tension: Force, rate: crate::units::SpringRate) -> Length {
    let net = force.newtons() - initial_tension.newtons();
    Length::from_meters((net.max(0.0)) / rate.newtons_per_meter())
}
```
Create `extension/ends.rs`:
```rust
//! Hook/loop end geometry for extension springs.

use crate::units::Length;

/// Mean bend radii of the two hook curvatures: r1 at the hook (point A, bending)
/// and r2 at the side bend (point B, torsion).
#[derive(Debug, Clone, Copy)]
pub struct HookEnds {
    pub r1: Length,
    pub r2: Length,
}

impl HookEnds {
    /// Standard machine-loop defaults: r1 = D/2, r2 = D/4 (spec default).
    pub fn default_for(mean_dia: Length) -> Self {
        Self {
            r1: Length::from_meters(mean_dia.meters() / 2.0),
            r2: Length::from_meters(mean_dia.meters() / 4.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    // (test from Step 1)
}
```
Add `pub mod ends;` to `extension/mod.rs`.

- [ ] **Step 4: Run, verify pass.** `cargo test -p springcore extension::` → PASS.
- [ ] **Step 5: Commit**
```bash
git add springcore/src/extension/mechanics.rs springcore/src/extension/ends.rs springcore/src/extension/mod.rs
git commit -m "feat(extension): initial-tension deflection + HookEnds defaults"
```

---

### Task 4: `ExtensionDesign` + `solve_forward`

**Files:** Create `springcore/src/extension/design.rs`; Modify `extension/mod.rs` (add `pub mod design;`).

**Interfaces — Consumes:** `crate::mechanics::{spring_rate, spring_index, wahl_factor, corrected_shear_stress}`; `extension::mechanics::{deflection, hook_bending_stress, hook_torsion_stress}`; `extension::ends::HookEnds`; `Material`; `units`; `SpringError`.
**Produces:**
```rust
pub struct ExtLoadPoint {
    pub force: Force,
    pub deflection: Length,
    pub length: Length,
    pub body_shear: Stress,
    pub hook_bending: Stress,   // σ_A
    pub hook_torsion: Stress,   // τ_B
    pub pct_body_allow: f64,    // body_shear / (allowable_torsion · Sut)
    pub pct_hook_bending_allow: f64, // hook_bending / (allowable_bending · Sut)
    pub pct_hook_torsion_allow: f64, // hook_torsion / (allowable_torsion · Sut)
}
pub struct ExtensionDesign {
    pub wire_dia: Length, pub mean_dia: Length, pub index: f64,
    pub active_coils: f64, pub rate: SpringRate,
    pub free_length: Length, pub initial_tension: Force,
    pub outer_dia: Length, pub inner_dia: Length,
    pub min_tensile_strength: Stress, pub hooks: HookEnds,
    pub load_points: Vec<ExtLoadPoint>,
}
#[allow(clippy::too_many_arguments)]
pub fn solve_forward(
    material: &Material, wire_dia: Length, mean_dia: Length, active: f64,
    free_length: Length, initial_tension: Force, hooks: HookEnds, loads: &[Force],
) -> Result<ExtensionDesign>
```

`solve_forward` logic (mirror `design::solve_forward` shape):
1. Validate `mean_dia > wire_dia` → else `SpringError::InconsistentInputs("mean diameter must exceed wire diameter (spring index must exceed 1)")`; validate `initial_tension >= 0` → else `InconsistentInputs("initial tension must be non-negative")`.
2. `index = spring_index(mean_dia, wire_dia)`; `rate = spring_rate(material.shear_modulus, wire_dia, mean_dia, active)`; `mts = material.min_tensile_strength(wire_dia)?`.
3. Each load → `ExtLoadPoint`: `y = deflection(f, initial_tension, rate)`; `length = free_length + y`; `body_shear = corrected_shear_stress(f, mean_dia, wire_dia, wahl_factor(index))`; `hook_bending = hook_bending_stress(f, mean_dia, wire_dia, hooks.r1)`; `hook_torsion = hook_torsion_stress(f, mean_dia, wire_dia, hooks.r2)`; the three `pct_*` = stress.pascals() / (fraction · mts.pascals()) using `material.allowable_pct_torsion` (body shear & hook torsion τ_B) and `material.allowable_pct_bending` (hook bending σ_A).
4. `outer_dia = mean+wire`, `inner_dia = mean−wire`.

- [ ] **Step 1: Write failing test** in `extension/design.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{Force, Length};
    use crate::extension::ends::HookEnds;
    use approx::assert_relative_eq;

    #[test]
    fn forward_solve_basic_design() {
        let m = crate::test_support::music_wire();
        let d = solve_forward(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(60.0),
            Force::from_newtons(10.0),
            HookEnds::default_for(Length::from_millimeters(20.0)),
            &[Force::from_newtons(30.0)],
        )
        .unwrap();
        assert_relative_eq!(d.rate.newtons_per_meter(), 2000.0, max_relative = 1e-9);
        // y = (30 − 10)/2000 = 10 mm; length = 60 + 10 = 70 mm.
        assert_relative_eq!(d.load_points[0].deflection.millimeters(), 10.0, max_relative = 1e-9);
        assert_relative_eq!(d.load_points[0].length.millimeters(), 70.0, max_relative = 1e-9);
        assert!(d.load_points[0].hook_bending.pascals() > d.load_points[0].body_shear.pascals());
    }

    #[test]
    fn rejects_mean_not_exceeding_wire() {
        let m = crate::test_support::music_wire();
        let r = solve_forward(
            &m, Length::from_millimeters(5.0), Length::from_millimeters(5.0), 10.0,
            Length::from_millimeters(60.0), Force::from_newtons(10.0),
            HookEnds::default_for(Length::from_millimeters(5.0)), &[Force::from_newtons(30.0)],
        );
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }
}
```

- [ ] **Step 2: Run, verify fail.** `cargo test -p springcore extension::design 2>&1 | tail` → FAIL.
- [ ] **Step 3: Implement** `extension/design.rs` per the logic above (mirror `design.rs`); add `pub mod design;` to `extension/mod.rs`.
- [ ] **Step 4: Run, verify pass.** `cargo test -p springcore extension::design` → PASS.
- [ ] **Step 5: Commit**
```bash
git add springcore/src/extension/design.rs springcore/src/extension/mod.rs
git commit -m "feat(extension): ExtensionDesign + forward solve"
```

---

### Task 5: `extension::Scenario` trait + `PowerUser` + re-exports

**Files:** Create `springcore/src/extension/scenario.rs`; Modify `extension/mod.rs` (add `pub mod scenario;` + re-exports); Modify `springcore/src/lib.rs` (re-export the family).

**Interfaces — Produces:**
```rust
pub trait Scenario { fn solve(&self, material: &Material) -> Result<ExtensionDesign>; }
pub struct PowerUser {
    pub wire_dia: Length, pub mean_dia: Length, pub active: f64,
    pub free_length: Length, pub initial_tension: Force,
    pub hooks: HookEnds, pub loads: Vec<Force>,
}
```
`PowerUser::solve` delegates to `design::solve_forward(material, wire_dia, mean_dia, active, free_length, initial_tension, hooks, &loads)`. Mirror `scenario.rs`'s `PowerUser` exactly (minus end_type/fixity, plus initial_tension/hooks).

`extension/mod.rs` re-exports: `pub use scenario::{Scenario, PowerUser}; pub use design::{ExtensionDesign, ExtLoadPoint}; pub use ends::HookEnds;`.
`lib.rs`: keep `pub mod extension;` (no flat re-export of names that would collide with compression — consumers use `springcore::extension::…`).

- [ ] **Step 1: Write failing test** in `extension/scenario.rs` (mirror compression's `power_user_passes_through`):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension::ends::HookEnds;
    use crate::units::{Force, Length};
    use approx::assert_relative_eq;

    #[test]
    fn power_user_solves() {
        let s = PowerUser {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            active: 10.0,
            free_length: Length::from_millimeters(60.0),
            initial_tension: Force::from_newtons(10.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            loads: vec![Force::from_newtons(30.0)],
        };
        let d = s.solve(&crate::test_support::music_wire()).unwrap();
        assert_relative_eq!(d.rate.newtons_per_meter(), 2000.0, max_relative = 1e-9);
        assert_relative_eq!(d.load_points[0].length.millimeters(), 70.0, max_relative = 1e-9);
    }
}
```

- [ ] **Step 2: Run, verify fail.** `cargo test -p springcore extension::scenario 2>&1 | tail` → FAIL.
- [ ] **Step 3: Implement** `extension/scenario.rs` (mirror `scenario.rs` `PowerUser`); wire `extension/mod.rs` re-exports.
- [ ] **Step 4: Run, verify pass.** `cargo test -p springcore extension::` → PASS (whole module).
- [ ] **Step 5: Commit**
```bash
git add springcore/src/extension/scenario.rs springcore/src/extension/mod.rs springcore/src/lib.rs
git commit -m "feat(extension): Scenario trait + PowerUser mode"
```

---

### Task 6: Golden worked-example test

**Files:** Modify `springcore/tests/golden.rs`

**DATA DEPENDENCY (spec §7):** the published inputs + results of a specific Shigley extension-spring worked example. **Before implementing, confirm the example’s figures with the maintainer** (edition + example number, given d/D/Na/L0/F_i/r1/r2 and published k, σ_A, τ_B). If not yet available, implement the test `#[ignore = "awaiting Shigley extension worked-example values — see PR body"]` with the structure below and TODO placeholders, mirroring the EN 13906-1 release-blocker pattern, so the scaffolding lands and is filled when data arrives.

**Interfaces — Consumes:** `springcore::extension::{PowerUser, Scenario, HookEnds}`, `MaterialSet`, `units`.

- [ ] **Step 1: Write the golden test** (transcribe the confirmed example; placeholders shown):
```rust
#[test]
fn extension_shigley_worked_example() {
    let set = MaterialSet::load_default();
    let material = set.get(/* material from the example */ "Music Wire").unwrap();
    let s = springcore::extension::PowerUser {
        wire_dia: Length::from_millimeters(/* d */ 0.0),
        mean_dia: Length::from_millimeters(/* D */ 0.0),
        active: /* Na */ 0.0,
        free_length: Length::from_millimeters(/* L0 */ 0.0),
        initial_tension: Force::from_newtons(/* F_i */ 0.0),
        hooks: springcore::extension::HookEnds {
            r1: Length::from_millimeters(/* r1 */ 0.0),
            r2: Length::from_millimeters(/* r2 */ 0.0),
        },
        loads: vec![Force::from_newtons(/* F */ 0.0)],
    };
    let d = s.solve(material).unwrap();
    assert_relative_eq!(d.rate.newtons_per_meter(), /* k */ 0.0, max_relative = 0.03);
    assert_relative_eq!(d.load_points[0].hook_bending.megapascals(), /* σ_A */ 0.0, max_relative = 0.03);
    assert_relative_eq!(d.load_points[0].hook_torsion.megapascals(), /* τ_B */ 0.0, max_relative = 0.03);
}
```

- [ ] **Step 2: Run.** If values are filled: `cargo test -p springcore --test golden extension 2>&1 | tail` → PASS. If `#[ignore]`d: `cargo test -p springcore --test golden 2>&1 | grep ignored` → shows 1 ignored.
- [ ] **Step 3: Commit**
```bash
git add springcore/tests/golden.rs
git commit -m "test(extension): golden worked-example (Shigley)"
```

---

## Self-Review (against the spec)

- **Spec coverage (Phase 1 / §8 item 1):** rate ✓ (Task 4 via reused `spring_rate`), initial tension ✓ (Task 3), body shear ✓ (Task 4 via reused `corrected_shear_stress`), hook σ_A/τ_B ✓ (Tasks 1–2), %-allowable mapping ✓ (Task 4), PowerUser ✓ (Task 5), golden ✓ (Task 6). Other input modes / min-weight / GUI / fatigue are explicitly later phases.
- **Placeholder scan:** the only placeholders are in Task 6, which is gated on the §7 data dependency and explicitly handled (confirm-or-`#[ignore]`), mirroring the EN 13906-1 precedent — not a plan gap.
- **Type consistency:** field/fn names are consistent across tasks (`hooks: HookEnds`, `ExtLoadPoint.hook_bending/hook_torsion/body_shear`, `solve_forward` arg order identical in Tasks 4 & 5). `Material` allowable-fraction names verified against `material.rs`: `allowable_pct_torsion` / `allowable_pct_bending`.

## Next phases (own plans, per spec §8)

2. Other input modes (TwoLoad — recovers k *and* F_i; RateBased; Dimensional).
3. Extension min-weight optimization (body + hook constraints).
4. GUI: spring-type selector, `ExtensionFormState`/`ExtField`, `extension_view_model`, results panel, chart variant, persistence, Simulator tests.
5. Body fatigue (reuses existing endurance data).
6. Hook fatigue (last — needs bending-endurance data, spec §7).
