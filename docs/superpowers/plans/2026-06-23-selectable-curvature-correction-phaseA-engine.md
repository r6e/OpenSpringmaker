# Selectable Curvature-Correction — Phase A (Engine) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the body-shear curvature-correction factor a caller-supplied `CurvatureCorrection { Wahl, Bergstrasser }` (default Bergsträsser) threaded through every static and fatigue body-shear path in `springcore`, and tighten the golden tests to each source's own convention. (No GUI/settings — that is Phase B.)

**Architecture:** `springcore` is a pure library with no global state, so the factor is an explicit parameter. A new `CurvatureCorrection` enum lives in `mechanics.rs` next to `wahl_factor`/`bergstrasser_factor` and dispatches to them via `factor(index)`. It is threaded through compression `solve_forward`/`load_point`, the `Scenario` trait + 4 impls, `SavedDesign::solve*`, the min-weight optimizer, extension `solve_forward`/`ext_load_point` + its `Scenario`, and `analyze_fatigue`. Hook curvature factors (σ_A/τ_B) are a different formula family and are untouched.

**Tech Stack:** Rust (workspace crate `springcore`), serde, `approx` for test asserts, `cargo test`/`clippy`/`mutants`.

## Global Constraints

- MSRV 1.88; dual MIT/Apache; SI canonical internally.
- Every formula/constant cited inline (Wahl: Shigley Eq. 10-5 / EN 13906-1 NOTE; Bergsträsser: Shigley Eq. 10-6 / EN 13906-1 Formula (1)).
- Strict TDD; `cargo fmt`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `RUSTDOCFLAGS=-D warnings cargo doc`, repo-wide `typos`, `cargo deny check all`, `cargo mutants --in-diff` (springcore) all green before push.
- Default factor is **Bergsträsser** (EN 13906-1 / Shigley primary).
- No commercial-product/vendor references in any persisted file.
- Branch `feat/selectable-curvature-correction`, already rebased onto post-#28 main.
- Phase A is `springcore`-only: do NOT touch `springmaker` (that is Phase B).

---

## File Structure

- `springcore/src/mechanics.rs` — **(modify)** add `CurvatureCorrection` enum + `factor()` + unit tests, beside the existing factor functions.
- `springcore/src/lib.rs` — **(modify)** re-export `CurvatureCorrection` from the crate root.
- `springcore/src/design.rs` — **(modify)** compression `solve_forward` + `load_point` take `correction`.
- `springcore/src/scenario.rs` — **(modify)** `Scenario::solve` + the 4 impls take `correction`; update tests.
- `springcore/src/optimize.rs` — **(modify)** `solve_min_weight` + `best_mean_dia` take `correction`; update tests.
- `springcore/src/persistence.rs` — **(modify)** `SavedDesign::solve` + `solve_with_material` take `correction`; update tests.
- `springcore/src/extension/design.rs` — **(modify)** extension `solve_forward` + `ext_load_point` take `correction`; update tests.
- `springcore/src/extension/scenario.rs` — **(modify)** extension `Scenario::solve` + `PowerUser` impl take `correction`; update tests.
- `springcore/src/fatigue.rs` — **(modify)** `analyze_fatigue` takes `correction`; update tests.
- `springcore/tests/golden.rs` — **(modify)** pass each golden's source convention; tighten body-shear tolerances.

---

## Task 1: `CurvatureCorrection` enum

**Files:**
- Modify: `springcore/src/mechanics.rs`
- Modify: `springcore/src/lib.rs`

**Interfaces:**
- Produces: `pub enum CurvatureCorrection { Wahl, Bergstrasser }` (in `mechanics`, re-exported at crate root) with `impl Default` (Bergsträsser), `serde::{Serialize, Deserialize}`, and `pub fn factor(self, index: f64) -> f64`. Later tasks call `correction.factor(index)` where they currently call `wahl_factor(index)` / `bergstrasser_factor(c)`.

- [ ] **Step 1: Write the failing tests** in `springcore/src/mechanics.rs`, inside the existing `#[cfg(test)] mod tests` block (add after `bergstrasser_factor_c10`). Also add `CurvatureCorrection` to the `use super::{...}` import list at the top of the test module.

```rust
    #[test]
    fn correction_factor_dispatches() {
        // factor() returns exactly the chosen closed-form factor.
        for &c in &[5.0_f64, 8.0, 10.8] {
            assert_relative_eq!(
                CurvatureCorrection::Wahl.factor(c),
                wahl_factor(c),
                max_relative = 1e-12
            );
            assert_relative_eq!(
                CurvatureCorrection::Bergstrasser.factor(c),
                bergstrasser_factor(c),
                max_relative = 1e-12
            );
        }
    }

    #[test]
    fn correction_default_is_bergstrasser() {
        // EN 13906-1 / Shigley name Bergsträsser the primary factor.
        assert_eq!(CurvatureCorrection::default(), CurvatureCorrection::Bergstrasser);
    }

    #[test]
    fn correction_serde_round_trips() {
        // Persisted in Phase B's settings file; lowercase token form.
        let json = serde_json::to_string(&CurvatureCorrection::Wahl).unwrap();
        assert_eq!(json, "\"wahl\"");
        let back: CurvatureCorrection = serde_json::from_str(&json).unwrap();
        assert_eq!(back, CurvatureCorrection::Wahl);
    }
```

- [ ] **Step 2: Verify the tests fail to compile** (the type does not exist yet)

Run: `cargo test -p springcore --lib correction_ 2>&1 | tail -5`
Expected: compile error `cannot find ... CurvatureCorrection`.

- [ ] **Step 3: Add the enum + impl** in `springcore/src/mechanics.rs`, immediately after `bergstrasser_factor` (around line 21). Add `use serde::{Deserialize, Serialize};` to the file's imports (top of file, with the existing `use` lines).

```rust
/// Wire-curvature stress-correction model for body torsional shear. Both are
/// accepted; EN 13906-1:2013 Formula (1) and Shigley name Bergsträsser primary
/// (the default), with Wahl the documented alternative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CurvatureCorrection {
    /// Wahl factor (Shigley Eq. 10-5; EN 13906-1 NOTE alternative).
    Wahl,
    /// Bergsträsser factor (Shigley Eq. 10-6; EN 13906-1 Formula (1)).
    #[default]
    Bergstrasser,
}

impl CurvatureCorrection {
    /// The chosen curvature-correction factor at spring index `index`.
    pub fn factor(self, index: f64) -> f64 {
        match self {
            Self::Wahl => wahl_factor(index),
            Self::Bergstrasser => bergstrasser_factor(index),
        }
    }
}
```

- [ ] **Step 4: Re-export from the crate root** — in `springcore/src/lib.rs`, change the mechanics re-export line (currently `pub use mechanics::EndFixity;`, ~line 42) to:

```rust
pub use mechanics::{CurvatureCorrection, EndFixity};
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p springcore --lib correction_ 2>&1 | tail -5`
Expected: `test result: ok. 3 passed`.

- [ ] **Step 6: Run fmt + clippy**

Run: `cargo fmt && cargo clippy -p springcore --all-targets -- -D warnings 2>&1 | tail -2`
Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add springcore/src/mechanics.rs springcore/src/lib.rs
git commit -m "feat(springcore): add CurvatureCorrection { Wahl, Bergstrasser } factor enum"
```

---

## Task 2: Thread correction through the compression path

This is one atomic unit: changing `solve_forward` and the `Scenario` trait cascades to all compression callers, so they must change together to compile.

**Files:**
- Modify: `springcore/src/design.rs` (`solve_forward`, `load_point`)
- Modify: `springcore/src/scenario.rs` (`Scenario` trait + `PowerUser`/`TwoLoad`/`RateBased`/`Dimensional` impls + tests)
- Modify: `springcore/src/optimize.rs` (`solve_min_weight`, `best_mean_dia` + tests)
- Modify: `springcore/src/persistence.rs` (`SavedDesign::solve`, `solve_with_material` + tests)
- Modify: `springcore/tests/golden.rs` (compression goldens)

**Interfaces:**
- Consumes: `CurvatureCorrection` (Task 1).
- Produces:
  - `pub fn solve_forward(material, end_type, fixity, wire_dia, mean_dia, active, free_length, loads, correction: CurvatureCorrection) -> Result<SpringDesign>` (new last param).
  - `Scenario::solve(&self, material: &Material, correction: CurvatureCorrection) -> Result<SpringDesign>`.
  - `pub fn solve_min_weight(material, req, correction: CurvatureCorrection) -> Result<MinWeightSolution>`.
  - `SavedDesign::solve(&self, materials, correction)` and `solve_with_material(&self, material, correction)`.

- [ ] **Step 1: Write the failing behavior test** in `springcore/src/design.rs` `#[cfg(test)] mod tests`. Import `CurvatureCorrection` via `crate::CurvatureCorrection`.

```rust
    /// The selected correction factor governs the body shear: the same geometry
    /// solved with Wahl vs Bergsträsser yields the two factors' stress ratio.
    #[test]
    fn solve_forward_uses_selected_correction() {
        let m = crate::test_support::music_wire();
        let mk = |corr| {
            solve_forward(
                &m,
                EndType::SquaredGround,
                EndFixity::FixedFixed,
                Length::from_millimeters(2.0),
                Length::from_millimeters(20.0),
                10.0,
                Length::from_millimeters(60.0),
                &[Force::from_newtons(30.0)],
                corr,
            )
            .unwrap()
            .load_points[0]
            .shear_stress
            .pascals()
        };
        let wahl = mk(crate::CurvatureCorrection::Wahl);
        let berg = mk(crate::CurvatureCorrection::Bergstrasser);
        // C = 10 → Kw/Kb = (39/36+0.0615)/(42/37); stresses share base 8FD/πd³.
        assert_relative_eq!(
            wahl / berg,
            crate::mechanics::wahl_factor(10.0) / crate::mechanics::bergstrasser_factor(10.0),
            max_relative = 1e-12
        );
    }
```

- [ ] **Step 2: Run it to confirm it fails to compile** (`solve_forward` takes 8 args today)

Run: `cargo test -p springcore --lib solve_forward_uses_selected_correction 2>&1 | tail -5`
Expected: compile error about argument count / `corr`.

- [ ] **Step 3: Add the param to `load_point` and `solve_forward`** in `springcore/src/design.rs`.

In `load_point` (currently ends its signature with `mts: Stress,`), add a final param and use it:

```rust
fn load_point(
    force: Force,
    rate: SpringRate,
    free_length: Length,
    mean_dia: Length,
    wire_dia: Length,
    index: f64,
    mts: Stress,
    correction: crate::CurvatureCorrection,
) -> LoadPoint {
    // Deflection y = F/k (Shigley Eq. 10-9 rearranged).
    let y = force.newtons() / rate.newtons_per_meter();
    let length = Length::from_meters(free_length.meters() - y);
    let stress = corrected_shear_stress(force, mean_dia, wire_dia, correction.factor(index));
    LoadPoint {
        force,
        deflection: Length::from_meters(y),
        length,
        shear_stress: stress,
        pct_mts: stress.pascals() / mts.pascals(),
    }
}
```

Remove the now-unused `wahl_factor` import from `design.rs` (the `use crate::mechanics::{...}` line) since `load_point` no longer calls it directly.

In `solve_forward`, add `correction: crate::CurvatureCorrection` as the final parameter (after `loads: &[Force]`), and pass `correction` to the two `load_point(...)` calls (the `.map(|&f| load_point(f, rate, free_length, mean_dia, wire_dia, index, mts))` and the `at_solid` call).

- [ ] **Step 4: Thread through the `Scenario` trait + impls** in `springcore/src/scenario.rs`. Change the trait method and every impl to take `correction` and forward it. Add `use crate::CurvatureCorrection;` to the file imports.

Trait:
```rust
    fn solve(&self, material: &Material, correction: CurvatureCorrection) -> Result<SpringDesign>;
```
In each of the 4 impls (`PowerUser`, `TwoLoad`, `RateBased`, `Dimensional`), change the signature to `fn solve(&self, material: &Material, correction: CurvatureCorrection) -> Result<SpringDesign>` and pass `correction` as the new final argument of the `solve_forward(...)` call.

- [ ] **Step 5: Thread through the optimizer** in `springcore/src/optimize.rs`. Replace the `wahl_factor` import with `CurvatureCorrection`, give `solve_min_weight` and `best_mean_dia` a `correction` param, and use it:

`best_mean_dia(material, d, max_force, index_bounds, correction)` — change the stress line (currently `corrected_shear_stress(max_force, dm, d, wahl_factor(c)).pascals()`) to:
```rust
        corrected_shear_stress(max_force, dm, d, correction.factor(c)).pascals()
```
`solve_min_weight(material, req, correction: CurvatureCorrection)` — pass `correction` to the `best_mean_dia(...)` call and to the `solve_forward(...)` call (its new final arg).

- [ ] **Step 6: Thread through persistence** in `springcore/src/persistence.rs`. `solve_with_material` and `solve` gain a `correction` param; forward it to each scenario `.solve(material, correction)` call and to the `solve_min_weight(material, &req, correction)` call.

```rust
    pub fn solve_with_material(&self, material: &Material, correction: CurvatureCorrection) -> Result<SpringDesign> {
        // ...unchanged material-name guard...
        match &self.scenario {
            // each `... }.solve(material)` becomes `.solve(material, correction)`
            // the MinWeight arm's `solve_min_weight(material, &req)` becomes
            //   `solve_min_weight(material, &req, correction)`
        }
    }

    pub fn solve(&self, materials: &MaterialSet, correction: CurvatureCorrection) -> Result<SpringDesign> {
        let material = materials.get(&self.material)?;
        self.solve_with_material(material, correction)
    }
```
Add `use crate::CurvatureCorrection;` (or `crate::mechanics::CurvatureCorrection`) to the imports.

- [ ] **Step 7: Update all compression tests to pass a correction.** In `scenario.rs`, `optimize.rs`, `persistence.rs`, and `design.rs` test modules, add the correction argument to every `.solve(...)`, `solve_forward(...)`, `solve_min_weight(...)`, and `SavedDesign::solve*` call. Use `CurvatureCorrection::Bergstrasser` (the default/standard) except where a test pins Wahl-specific values — for those, pass `CurvatureCorrection::Wahl` so the existing expected numbers stay valid. (The pre-existing `corrected_stress_c10` mechanics test is unaffected — it calls `corrected_shear_stress` directly.)

- [ ] **Step 8: Update the compression goldens** in `springcore/tests/golden.rs`:
  - `comprehensive_spring_design_compression`: its source publishes a **Wahl** factor (`Ka = 1.221`), so pass `springcore::CurvatureCorrection::Wahl` to `.solve(...)`. The 3% shear tolerance can stay (it also absorbs source rounding), but the factor is now its source's convention.
  - `shigley_10_1_compression`: Shigley uses **Bergsträsser** (`K_B = 1.124`), so pass `springcore::CurvatureCorrection::Bergstrasser`. Tighten the corrected-shear assertion from `max_relative = 0.02` to `max_relative = 3e-3` (now exact up to Shigley's 3-figure rounding of K_B) and update the comment to drop the "Wahl vs Bergsträsser" note.
  - `pipeline_rate_based_music_wire`: pass `springcore::CurvatureCorrection::Bergstrasser` to `saved.solve(&set, ...)`. (Its `analyze_fatigue` call is updated in Task 4.)

- [ ] **Step 9: Run the full springcore suite + clippy**

Run: `cargo test -p springcore 2>&1 | grep "test result:"` then `cargo clippy -p springcore --all-targets -- -D warnings 2>&1 | tail -2`
Expected: all green; the new `solve_forward_uses_selected_correction` passes; the Shigley compression golden passes at the tightened tolerance.

- [ ] **Step 10: Commit**

```bash
git add springcore/src/design.rs springcore/src/scenario.rs springcore/src/optimize.rs springcore/src/persistence.rs springcore/tests/golden.rs
git commit -m "feat(springcore): thread CurvatureCorrection through the compression path"
```

---

## Task 3: Thread correction through the extension path

**Files:**
- Modify: `springcore/src/extension/design.rs` (`solve_forward`, `ext_load_point` + tests)
- Modify: `springcore/src/extension/scenario.rs` (`Scenario` trait + `PowerUser` impl + tests)
- Modify: `springcore/tests/golden.rs` (`extension_shigley_worked_example`)

**Interfaces:**
- Consumes: `CurvatureCorrection` (Task 1).
- Produces: extension `solve_forward(.., loads, correction)` and extension `Scenario::solve(&self, material, correction)`.

- [ ] **Step 1: Write the failing behavior test** in `springcore/src/extension/design.rs` `#[cfg(test)] mod tests`:

```rust
    /// The selected correction factor governs extension body shear (only the
    /// body term; the hook σ_A/τ_B factors are independent).
    #[test]
    fn solve_forward_uses_selected_correction() {
        let m = crate::test_support::music_wire();
        let mk = |corr| {
            solve_forward(
                &m,
                Length::from_millimeters(2.0),
                Length::from_millimeters(20.0),
                10.0,
                Length::from_millimeters(60.0),
                Force::from_newtons(10.0),
                HookEnds::default_for(Length::from_millimeters(20.0)),
                &[Force::from_newtons(30.0)],
                corr,
            )
            .unwrap()
            .load_points[0]
            .body_shear
            .pascals()
        };
        assert_relative_eq!(
            mk(crate::CurvatureCorrection::Wahl) / mk(crate::CurvatureCorrection::Bergstrasser),
            crate::mechanics::wahl_factor(10.0) / crate::mechanics::bergstrasser_factor(10.0),
            max_relative = 1e-12
        );
    }
```

- [ ] **Step 2: Run it to confirm a compile failure**

Run: `cargo test -p springcore --lib extension::design::tests::solve_forward_uses_selected_correction 2>&1 | tail -5`
Expected: compile error about argument count.

- [ ] **Step 3: Add the param to `ext_load_point` and extension `solve_forward`** in `springcore/src/extension/design.rs`. Give `ext_load_point` a final `correction: crate::CurvatureCorrection` param and change its body-shear line (currently `corrected_shear_stress(force, mean_dia, wire_dia, wahl_factor(index))`) to use `correction.factor(index)`. Remove the now-unused `wahl_factor` from the `use crate::mechanics::{...}` import. Add `correction: crate::CurvatureCorrection` as the final param of `solve_forward`, and pass it into the `ext_load_point(...)` call inside the `loads.iter().map(...)`.

- [ ] **Step 4: Thread through the extension `Scenario`** in `springcore/src/extension/scenario.rs`: trait method and the `PowerUser` impl become `fn solve(&self, material: &Material, correction: crate::CurvatureCorrection) -> Result<ExtensionDesign>`, passing `correction` as the final arg of `solve_forward(...)`.

- [ ] **Step 5: Update extension tests** in `extension/design.rs` and `extension/scenario.rs`: add the correction argument to every `solve_forward(...)` / `.solve(...)` call. Use `CurvatureCorrection::Bergstrasser` except where a test pins a Wahl-specific number (none currently assert body-shear magnitude except the golden, so Bergsträsser is fine for the unit tests).

- [ ] **Step 6: Update the extension golden** in `springcore/tests/golden.rs`. `extension_shigley_worked_example` (Shigley 10-6 uses Bergsträsser K_B = 1.234 for the body shear): pass `springcore::CurvatureCorrection::Bergstrasser` to `.solve(...)`, tighten the body-shear assertion from `max_relative = 0.02` to `max_relative = 3e-3`, and update the comment to drop the "Wahl vs Bergsträsser" note. (σ_A/τ_B/k assertions are unchanged.)

- [ ] **Step 7: Run the suite + clippy**

Run: `cargo test -p springcore 2>&1 | grep "test result:"` then `cargo clippy -p springcore --all-targets -- -D warnings 2>&1 | tail -2`
Expected: all green; the extension golden passes at the tightened tolerance.

- [ ] **Step 8: Commit**

```bash
git add springcore/src/extension/design.rs springcore/src/extension/scenario.rs springcore/tests/golden.rs
git commit -m "feat(springcore): thread CurvatureCorrection through the extension path"
```

---

## Task 4: Thread correction through fatigue

**Files:**
- Modify: `springcore/src/fatigue.rs` (`analyze_fatigue` + tests)
- Modify: `springcore/tests/golden.rs` (`pipeline_rate_based_music_wire` fatigue call)

**Interfaces:**
- Consumes: `CurvatureCorrection` (Task 1).
- Produces: `pub fn analyze_fatigue(material, wire_dia, mean_dia, force_min, force_max, correction: CurvatureCorrection) -> Result<FatigueResult>`.

- [ ] **Step 1: Write the failing behavior test** in `springcore/src/fatigue.rs` `#[cfg(test)] mod tests` (mirror the existing fatigue test setup; use the same material/geometry an existing test uses). Assert the selection changes the result — e.g. the Goodman factor of safety differs between Wahl and Bergsträsser for a fixed cycle:

```rust
    #[test]
    fn analyze_fatigue_uses_selected_correction() {
        let m = crate::test_support::music_wire();
        let mk = |corr| {
            analyze_fatigue(
                &m,
                Length::from_millimeters(2.0),
                Length::from_millimeters(20.0),
                Force::from_newtons(10.0),
                Force::from_newtons(30.0),
                corr,
            )
            .unwrap()
            .goodman_factor_of_safety
        };
        // Wahl > Bergsträsser at C=10 → higher stress → lower factor of safety, so they differ.
        assert!(mk(crate::CurvatureCorrection::Wahl) < mk(crate::CurvatureCorrection::Bergstrasser));
    }
```
(If `test_support::music_wire()` lacks endurance data, use the material/geometry the existing passing fatigue test in this module already uses — check the module before writing.)

- [ ] **Step 2: Run it to confirm a compile failure**

Run: `cargo test -p springcore --lib fatigue::tests::analyze_fatigue_uses_selected_correction 2>&1 | tail -5`
Expected: compile error about argument count.

- [ ] **Step 3: Add the param** in `springcore/src/fatigue.rs`. Add `correction: crate::CurvatureCorrection` as the final parameter of `analyze_fatigue`, and replace `let kb = bergstrasser_factor(c);` with `let kb = correction.factor(c);` (rename the binding if clearer, e.g. `let k = correction.factor(c);` and update the two `corrected_shear_stress(.., kb)` calls). Remove the now-unused `bergstrasser_factor` import if nothing else in the file uses it.

- [ ] **Step 4: Update fatigue tests + the pipeline golden.** In `fatigue.rs` tests, add the correction arg to existing `analyze_fatigue(...)` calls (use `CurvatureCorrection::Bergstrasser` to preserve current expected values, since fatigue used Bergsträsser before). In `springcore/tests/golden.rs`, update the `analyze_fatigue(...)` call in `pipeline_rate_based_music_wire` to pass `springcore::CurvatureCorrection::Bergstrasser`.

- [ ] **Step 5: Run the full suite + clippy**

Run: `cargo test -p springcore 2>&1 | grep "test result:"` then `cargo clippy -p springcore --all-targets -- -D warnings 2>&1 | tail -2`
Expected: all green.

- [ ] **Step 6: Commit**

```bash
git add springcore/src/fatigue.rs springcore/tests/golden.rs
git commit -m "feat(springcore): thread CurvatureCorrection through fatigue analysis"
```

---

## Final verification (before opening the PR)

- [ ] Full gate: `cargo test --workspace`, `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps`, `typos`, `cargo deny check all`.
- [ ] Mutation: `git diff main...HEAD -- springcore/ > /tmp/cc.diff && cargo mutants --in-diff /tmp/cc.diff -p springcore` — 0 survivors (the new `factor()` dispatch and each threaded call are covered by the per-task behavior tests; add a pinning test if any survive).
- [ ] Confirm `springmaker` is untouched (`git diff --stat main...HEAD` shows only `springcore/` + docs).

**Note:** `springmaker` will not compile against the changed `springcore` signatures until Phase B updates its `solve`/`analyze_fatigue`/`solve_min_weight` call sites. Phase A is `springcore`-only; the workspace `cargo build`/`cargo test --workspace` will therefore fail in `springmaker` until Phase B. **Resolve this by doing Phase B immediately after Phase A on the same branch (recommended), or expect the workspace build to break between the two.** If the two must be separate green PRs, the call-site updates in `springmaker` belong in whichever PR keeps `main` green — i.e. Phase A and Phase B should land together, or Phase A must include the minimal `springmaker` call-site updates to keep the workspace compiling.

---

## Self-review notes

- **Spec coverage:** enum + default + serde (Task 1); thread through compression `solve_forward`/`Scenario`/optimizer/persistence (Task 2), extension (Task 3), fatigue (Task 4); golden tightening (Tasks 2,3) — all spec §3.1 / §6 items covered. Settings/GUI (spec §3.2/§3.3) are explicitly Phase B.
- **Workspace-compile caveat:** changing public `springcore` signatures breaks `springmaker`'s call sites. This is called out above. The cleanest execution is Phase A + Phase B on one branch / one PR pair landed together; otherwise Phase A alone leaves `cargo test --workspace` red in `springmaker`. **Decide this before executing** (see Open Question).

## Resolved: A + B land together as ONE PR

**Decision (human):** Phase A stays strictly `springcore`-only; Phases A and B execute on the **same branch** (`feat/selectable-curvature-correction`) and ship as **one PR**, so `main` only ever sees the final green state. The workspace (`cargo test --workspace`) is expected to be red on the intermediate Phase-A commits (because `springmaker` still calls the old signatures) and goes green once Phase B updates those call sites — this is fine because nothing is pushed/merged until the combined tip is green. The springmaker call-site updates therefore live in **Phase B**, not here. Run the full `--workspace` gate only at the end of Phase B, not at the end of Phase A.
