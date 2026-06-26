# Extension Springs Phase 2 — Input Modes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the remaining three extension-spring input modes — `TwoLoad`, `RateBased`, `Dimensional` — as `extension::Scenario` implementations that derive the geometry and delegate to the existing (tested) extension `solve_forward`, bringing the extension family to input-mode parity with compression.

**Architecture:** Each mode is a struct + `Scenario` impl in `springcore/src/extension/scenario.rs`, alongside the existing `PowerUser`. They mirror the compression scenarios in `springcore/src/scenario.rs` but adapted for extension: they carry `initial_tension` (F_i) and `hooks` instead of `end_type`/`fixity`, and there is no buckling/solid-length. All delegate to `extension::design::solve_forward(.., correction)`.

**Tech Stack:** Rust (workspace crate `springcore`), `approx` for test asserts, `cargo test`/`clippy`/`mutants`.

## Global Constraints

- MSRV 1.88; dual MIT/Apache; SI canonical internally.
- Every formula cited inline (Shigley extension sections; spec §4).
- Strict TDD; `cargo fmt`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `RUSTDOCFLAGS=-D warnings cargo doc`, repo-wide `typos`, `cargo deny check all`, `cargo mutants --in-diff` (springcore) all green before push.
- No commercial-product/vendor references in any persisted file.
- Engine-only: `springcore`-only; do NOT touch `springmaker` (extension GUI is a later phase).
- Every scenario `solve` takes the `correction: CurvatureCorrection` parameter (the signature already established by `PowerUser`).
- Branch off current `main` (#27/#28/#29 all merged).

## Resolved design decision — extension `TwoLoad` takes `free_length`

Compression `TwoLoad` solves only the rate `k` from two (force, length) points and *derives* the free length. Extension must solve **both** `k` and the initial tension `F_i`. From two points (F₁,L₁),(F₂,L₂) with deflection from free length y = L − L₀:
- `k = (F₂−F₁)/(y₂−y₁) = (F₂−F₁)/(L₂−L₁)` (L₀ cancels in the difference).
- `F_i = F₁ − k·y₁`, which needs `y₁ = L₁ − L₀` — i.e. **L₀ must be known**.

Two points alone leave `F_i` and `L₀` coupled (one equation, two unknowns), so extension `TwoLoad` takes `free_length` (L₀) as an explicit input (the user can measure the free length). This matches the spec §4 formula `F_i = F₁ − k·y₁` (which presumes y₁ is known). `solve_forward`'s existing `F_i ≥ 0` guard rejects point/length combinations that imply a negative initial tension.

---

## File Structure

- `springcore/src/extension/scenario.rs` — **(modify)** add `TwoLoad`, `RateBased`, `Dimensional` structs + `Scenario` impls + tests, beside the existing `PowerUser`.
- `springcore/src/extension/mod.rs` — **(modify)** re-export the three new scenario types.

(No `solve_forward`, golden, or `springmaker` changes — these are thin closed-form wrappers over the already-tested solver.)

---

## Task 1: Extension `TwoLoad` scenario

**Files:**
- Modify: `springcore/src/extension/scenario.rs`
- Modify: `springcore/src/extension/mod.rs`

**Interfaces:**
- Consumes: `extension::design::solve_forward(material, wire_dia, mean_dia, active, free_length, initial_tension, hooks, loads, correction) -> Result<ExtensionDesign>`; `crate::mechanics::active_coils_for_rate(shear_modulus: Stress, wire_dia, mean_dia, rate: SpringRate) -> f64`; `HookEnds`; `CurvatureCorrection`.
- Produces: `pub struct TwoLoad { wire_dia, mean_dia, free_length, hooks, point1: (Force, Length), point2: (Force, Length) }` implementing `extension::Scenario`.

- [ ] **Step 1: Write the failing test** in `springcore/src/extension/scenario.rs` `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn two_load_recovers_rate_and_initial_tension() {
        // Music wire, d=2mm D=20mm → k=2000 N/m. Choose F_i=10 N, L0=60mm.
        // At y=10mm (L=70mm): F = F_i + k·y = 10 + 2000·0.010 = 30 N.
        // At y=20mm (L=80mm): F = 10 + 2000·0.020 = 50 N.
        let s = TwoLoad {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            free_length: Length::from_millimeters(60.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            point1: (Force::from_newtons(30.0), Length::from_millimeters(70.0)),
            point2: (Force::from_newtons(50.0), Length::from_millimeters(80.0)),
        };
        let d = s
            .solve(&crate::test_support::music_wire(), CurvatureCorrection::Bergstrasser)
            .unwrap();
        assert_relative_eq!(d.rate.newtons_per_meter(), 2000.0, max_relative = 1e-9);
        assert_relative_eq!(d.initial_tension.newtons(), 10.0, max_relative = 1e-9);
        // The first operating point round-trips to its given length.
        assert_relative_eq!(d.load_points[0].length.millimeters(), 70.0, max_relative = 1e-9);
    }

    #[test]
    fn two_load_rejects_non_increasing_points() {
        // More force at a shorter length is impossible for an extension spring.
        let s = TwoLoad {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            free_length: Length::from_millimeters(60.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            point1: (Force::from_newtons(50.0), Length::from_millimeters(70.0)),
            point2: (Force::from_newtons(30.0), Length::from_millimeters(80.0)),
        };
        assert!(matches!(
            s.solve(&crate::test_support::music_wire(), CurvatureCorrection::Bergstrasser),
            Err(crate::SpringError::InconsistentInputs(_))
        ));
    }
```

Add the imports the tests need to the test module (`SpringRate` is not needed in tests; `CurvatureCorrection`, `HookEnds`, `Force`, `Length` already imported via the existing test module — verify and add any missing).

- [ ] **Step 2: Run the tests — expect a compile failure** (`TwoLoad` undefined)

Run: `cargo test -p springcore --lib extension::scenario::tests::two_load 2>&1 | tail -5`
Expected: compile error `cannot find ... TwoLoad`.

- [ ] **Step 3: Implement `TwoLoad`** in `springcore/src/extension/scenario.rs` (after `PowerUser`). Add `use crate::mechanics::active_coils_for_rate;` and `use crate::units::SpringRate;` to the file's imports (with the existing `use` lines), and `use crate::SpringError;`.

```rust
/// Two (force, length) operating points; solve the rate AND the initial tension.
/// Free length is given so the per-point deflections y = L − L0 are known (see
/// the plan's "Resolved design decision"). Shigley extension relations:
/// k = (F2−F1)/(y2−y1), F_i = F1 − k·y1.
#[derive(Debug, Clone)]
pub struct TwoLoad {
    pub wire_dia: Length,
    pub mean_dia: Length,
    pub free_length: Length,
    pub hooks: HookEnds,
    pub point1: (Force, Length),
    pub point2: (Force, Length),
}

impl Scenario for TwoLoad {
    fn solve(
        &self,
        material: &Material,
        correction: CurvatureCorrection,
    ) -> Result<ExtensionDesign> {
        let (f1, l1) = self.point1;
        let (f2, l2) = self.point2;
        // Deflection from the free length (an extension spring lengthens under load).
        let y1 = l1.meters() - self.free_length.meters();
        let y2 = l2.meters() - self.free_length.meters();
        let df = f2.newtons() - f1.newtons();
        let dy = y2 - y1;
        // A valid extension pair has more force at the greater length.
        if !(df.is_finite() && dy.is_finite()) || df <= 0.0 || dy <= 0.0 {
            return Err(SpringError::InconsistentInputs(
                "two load points must show increasing force with increasing length".into(),
            ));
        }
        let rate = SpringRate::from_newtons_per_meter(df / dy);
        let initial_tension = Force::from_newtons(f1.newtons() - rate.newtons_per_meter() * y1);
        let active =
            active_coils_for_rate(material.shear_modulus, self.wire_dia, self.mean_dia, rate);
        solve_forward(
            material,
            self.wire_dia,
            self.mean_dia,
            active,
            self.free_length,
            initial_tension,
            self.hooks,
            &[f1, f2],
            correction,
        )
    }
}
```

- [ ] **Step 4: Re-export** in `springcore/src/extension/mod.rs` — change `pub use scenario::{PowerUser, Scenario};` to `pub use scenario::{PowerUser, Scenario, TwoLoad};`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p springcore --lib extension::scenario::tests::two_load 2>&1 | tail -5`
Expected: `test result: ok. 2 passed`.

- [ ] **Step 6: fmt + clippy**

Run: `cargo fmt && cargo clippy -p springcore --all-targets -- -D warnings 2>&1 | tail -2`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add springcore/src/extension/scenario.rs springcore/src/extension/mod.rs
git commit -m "feat(extension): TwoLoad scenario (solves rate and initial tension)"
```

---

## Task 2: Extension `RateBased` scenario

**Files:**
- Modify: `springcore/src/extension/scenario.rs`
- Modify: `springcore/src/extension/mod.rs`

**Interfaces:**
- Consumes: `solve_forward`; `active_coils_for_rate`; `SpringRate`; `HookEnds`; `CurvatureCorrection`.
- Produces: `pub struct RateBased { wire_dia, mean_dia, rate: SpringRate, free_length, initial_tension, hooks, loads: Vec<Force> }` implementing `extension::Scenario`.

- [ ] **Step 1: Write the failing test** in the `scenario.rs` test module:

```rust
    #[test]
    fn rate_based_backs_out_active_coils() {
        // Target k=2000 N/m at d=2mm D=20mm → Na=10 (back-solved).
        let s = RateBased {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            rate: SpringRate::from_newtons_per_meter(2000.0),
            free_length: Length::from_millimeters(60.0),
            initial_tension: Force::from_newtons(10.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            loads: vec![Force::from_newtons(30.0)],
        };
        let d = s
            .solve(&crate::test_support::music_wire(), CurvatureCorrection::Bergstrasser)
            .unwrap();
        assert_relative_eq!(d.active_coils, 10.0, max_relative = 1e-6);
        assert_relative_eq!(d.rate.newtons_per_meter(), 2000.0, max_relative = 1e-6);
    }

    #[test]
    fn rate_based_rejects_non_positive_rate() {
        let s = RateBased {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            rate: SpringRate::from_newtons_per_meter(0.0),
            free_length: Length::from_millimeters(60.0),
            initial_tension: Force::from_newtons(10.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            loads: vec![Force::from_newtons(30.0)],
        };
        assert!(matches!(
            s.solve(&crate::test_support::music_wire(), CurvatureCorrection::Bergstrasser),
            Err(crate::SpringError::InconsistentInputs(_))
        ));
    }
```

- [ ] **Step 2: Run — expect compile failure**

Run: `cargo test -p springcore --lib extension::scenario::tests::rate_based 2>&1 | tail -5`
Expected: `cannot find ... RateBased`.

- [ ] **Step 3: Implement `RateBased`** in `scenario.rs`:

```rust
/// Required rate given; back out the active coils, then solve. Mirrors the
/// compression `RateBased` (plus initial tension / hooks).
#[derive(Debug, Clone)]
pub struct RateBased {
    pub wire_dia: Length,
    pub mean_dia: Length,
    pub rate: SpringRate,
    pub free_length: Length,
    pub initial_tension: Force,
    pub hooks: HookEnds,
    pub loads: Vec<Force>,
}

impl Scenario for RateBased {
    fn solve(
        &self,
        material: &Material,
        correction: CurvatureCorrection,
    ) -> Result<ExtensionDesign> {
        // Validate the target rate here so a non-positive/non-finite value gives a
        // rate-specific message rather than the derived "active coils" error.
        if !(self.rate.newtons_per_meter().is_finite() && self.rate.newtons_per_meter() > 0.0) {
            return Err(SpringError::InconsistentInputs(
                "required rate must be a positive finite number".into(),
            ));
        }
        let active =
            active_coils_for_rate(material.shear_modulus, self.wire_dia, self.mean_dia, self.rate);
        solve_forward(
            material,
            self.wire_dia,
            self.mean_dia,
            active,
            self.free_length,
            self.initial_tension,
            self.hooks,
            &self.loads,
            correction,
        )
    }
}
```

- [ ] **Step 4: Re-export** — extend the `extension/mod.rs` `pub use scenario::{...}` to include `RateBased`.

- [ ] **Step 5: Run the tests — expect pass**

Run: `cargo test -p springcore --lib extension::scenario::tests::rate_based 2>&1 | tail -5`
Expected: `test result: ok. 2 passed`.

- [ ] **Step 6: fmt + clippy** (`cargo fmt && cargo clippy -p springcore --all-targets -- -D warnings`) — clean.

- [ ] **Step 7: Commit**

```bash
git add springcore/src/extension/scenario.rs springcore/src/extension/mod.rs
git commit -m "feat(extension): RateBased scenario (back-solves active coils)"
```

---

## Task 3: Extension `Dimensional` scenario

**Files:**
- Modify: `springcore/src/extension/scenario.rs`
- Modify: `springcore/src/extension/mod.rs`

**Interfaces:**
- Consumes: `solve_forward`; `HookEnds`; `CurvatureCorrection`.
- Produces: `pub struct Dimensional { wire_dia, outer_dia, active: f64, free_length, initial_tension, hooks, loads: Vec<Force> }` implementing `extension::Scenario`.

- [ ] **Step 1: Write the failing test** in the `scenario.rs` test module:

```rust
    #[test]
    fn dimensional_uses_outer_diameter() {
        // OD=22mm, d=2mm → mean=20mm → C=10.
        let s = Dimensional {
            wire_dia: Length::from_millimeters(2.0),
            outer_dia: Length::from_millimeters(22.0),
            active: 10.0,
            free_length: Length::from_millimeters(60.0),
            initial_tension: Force::from_newtons(10.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            loads: vec![Force::from_newtons(30.0)],
        };
        let d = s
            .solve(&crate::test_support::music_wire(), CurvatureCorrection::Bergstrasser)
            .unwrap();
        assert_relative_eq!(d.index, 10.0, max_relative = 1e-9);
        assert_relative_eq!(d.mean_dia.millimeters(), 20.0, max_relative = 1e-9);
    }

    #[test]
    fn dimensional_rejects_non_positive_outer() {
        let s = Dimensional {
            wire_dia: Length::from_millimeters(2.0),
            outer_dia: Length::from_millimeters(0.0),
            active: 10.0,
            free_length: Length::from_millimeters(60.0),
            initial_tension: Force::from_newtons(10.0),
            hooks: HookEnds::default_for(Length::from_millimeters(20.0)),
            loads: vec![Force::from_newtons(30.0)],
        };
        assert!(matches!(
            s.solve(&crate::test_support::music_wire(), CurvatureCorrection::Bergstrasser),
            Err(crate::SpringError::InconsistentInputs(_))
        ));
    }
```

- [ ] **Step 2: Run — expect compile failure**

Run: `cargo test -p springcore --lib extension::scenario::tests::dimensional 2>&1 | tail -5`
Expected: `cannot find ... Dimensional`.

- [ ] **Step 3: Implement `Dimensional`** in `scenario.rs`:

```rust
/// Outer diameter given; derive the mean diameter (D = OD − d), then solve.
/// Mirrors the compression `Dimensional` (plus initial tension / hooks).
#[derive(Debug, Clone)]
pub struct Dimensional {
    pub wire_dia: Length,
    pub outer_dia: Length,
    pub active: f64,
    pub free_length: Length,
    pub initial_tension: Force,
    pub hooks: HookEnds,
    pub loads: Vec<Force>,
}

impl Scenario for Dimensional {
    fn solve(
        &self,
        material: &Material,
        correction: CurvatureCorrection,
    ) -> Result<ExtensionDesign> {
        // Validate the outer diameter here so a non-finite/non-positive value gives
        // a clear message rather than a derived "mean diameter" error.
        if !(self.outer_dia.meters().is_finite() && self.outer_dia.meters() > 0.0) {
            return Err(SpringError::InconsistentInputs(
                "outer diameter must be a positive finite number".into(),
            ));
        }
        let mean = Length::from_meters(self.outer_dia.meters() - self.wire_dia.meters());
        solve_forward(
            material,
            self.wire_dia,
            mean,
            self.active,
            self.free_length,
            self.initial_tension,
            self.hooks,
            &self.loads,
            correction,
        )
    }
}
```

- [ ] **Step 4: Re-export** — extend the `extension/mod.rs` `pub use scenario::{...}` to include `Dimensional`. Final line: `pub use scenario::{Dimensional, PowerUser, RateBased, Scenario, TwoLoad};`.

- [ ] **Step 5: Run the tests — expect pass**

Run: `cargo test -p springcore --lib extension::scenario::tests::dimensional 2>&1 | tail -5`
Expected: `test result: ok. 2 passed`.

- [ ] **Step 6: fmt + clippy** — clean.

- [ ] **Step 7: Commit**

```bash
git add springcore/src/extension/scenario.rs springcore/src/extension/mod.rs
git commit -m "feat(extension): Dimensional scenario (derives mean from outer diameter)"
```

---

## Final verification (before opening the PR)

- [ ] Full gate: `cargo test --workspace`; `cargo fmt --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps`; `typos`; `cargo deny check all`.
- [ ] Mutation: `git diff main...HEAD -- springcore/ > /tmp/ext2.diff && cargo mutants --in-diff /tmp/ext2.diff -p springcore` — 0 survivors (the new closed-form derivations — `k`/`F_i`/`mean`/the guards — are pinned by the per-scenario tests; add a pinning assertion if any survive).
- [ ] Confirm `springmaker` is untouched (`git diff --stat main...HEAD` shows only `springcore/extension/` + this plan doc).

---

## Self-review notes

- **Spec coverage (§4 input modes):** TwoLoad (Task 1), RateBased (Task 2), Dimensional (Task 3) — all three remaining input modes from spec §2/§4. PowerUser shipped in Phase 1. **Min-weight (spec §4 MinWeight, §8 item 3) is intentionally NOT in this plan** — see "Out of scope" below.
- **Placeholder scan:** every step has concrete code/commands; no TBD/TODO.
- **Type consistency:** all three impls use the exact `solve_forward(material, wire_dia, mean_dia, active, free_length, initial_tension, hooks, loads, correction)` signature and `Scenario::solve(&self, material, correction)`; `active_coils_for_rate(shear_modulus, wire_dia, mean_dia, rate)`; field names match across structs and tests.

## Out of scope — extension min-weight is Phase 3 (separate plan)

Extension min-weight (spec §4 MinWeight / §8 item 3) is deliberately deferred to its own plan because it is materially more complex and under-specified than the compression optimizer this plan's scenarios mirror:

- Compression `best_mean_dia` finds the binding mean diameter against a SINGLE stress (body shear, monotonic in C). Extension must honor **three** stresses simultaneously — body shear, hook bending σ_A, and hook torsion τ_B — so the feasible mean diameter is the minimum over three per-constraint roots (plus the index ceiling and OD cap). That multi-constraint binding search is a real algorithm the spec only states as a goal.
- It must also decide how `free_length` (Shigley Eq. 10-39 from geometry), `initial_tension`, and `hooks` (default-vs-given, and whether hook radii scale with D) feed the optimizer, none of which the spec pins down.

That algorithm warrants its own focused plan (and a short design confirmation), so it does not ride along under-designed here.
