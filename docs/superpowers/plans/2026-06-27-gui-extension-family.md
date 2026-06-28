# Extension Family in the GUI (Spec 1b) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make **Extension** a real, selectable spring family in the `springmaker` GUI for the **PowerUser** scenario — family selector, an `extension/` GUI module driving the existing engine, three-stress hook results, engine-computed status, and save/load — proving the multi-family state/message/dispatch/persistence pattern end-to-end on one input mode.

**Architecture:** A walking-skeleton vertical slice. First three behavior-preserving compression refactors (lift `material`/`unit_system` to `App`; move `Field` into `compression::form` + make `presenter::FieldDescriptor` generic; extract the shared calculator chrome to `calculator.rs`), then a shared-helpers lift (numeric-parse/display/conversion/render helpers move to shared modules so the second family consumes, never copies). Then the `springcore::Family` enum + selector + `App`-level dispatch land atomically with a thin extension module (no dead variants). Later tasks thicken the already-referenced extension code (three-stress table), add the family-tagged persistence schema, and an end-to-end Simulator test.

**Tech Stack:** Rust (workspace: `springcore` pure engine + `springmaker` iced 0.14 GUI), serde + toml persistence, `approx` for float asserts, `cargo mutants` gate on `springcore`.

## Global Constraints

Every task's requirements implicitly include this section. Values are copied verbatim from the spec.

- MSRV 1.88; iced 0.14; dual MIT/Apache; **SI canonical** in the engine, convert at the boundary.
- ADR 0008 presenter / humble-view split preserved for the new extension screen (pure presenter in `view_model.rs`, humble iced in `view.rs`).
- Every engine formula already cited in `springcore`; status thresholds cite Shigley allowables (already encoded in the `pct_*_allow` fields and `Material.allowable_pct_*`).
- Before every push: `cargo fmt`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc`, repo-wide `typos`, `cargo deny check all`, `cargo test --workspace`, and `cargo mutants --in-diff` on `springcore` (literal **0** survivors) all green.
- No commercial-product/vendor references in any persisted file.
- No `#[allow(dead_code)]` or lint-suppression scaffolding; internal enums stay exhaustive (no `#[non_exhaustive]` on GUI-matched enums).
- **DRY on the second occurrence — lift, don't duplicate.** Shared logic blocks (numeric parsing, unit conversion/labels, output-row rendering) are extracted to shared modules and consumed by both families; only family-specific *glue* (a family's own field enum → its own message variant) stays per-family.
- `springmaker` is not mutation-gated (GUI); its correctness bar is the presenter unit tests + the headless `Simulator` E2E tests. `springcore` **is** mutation-gated.
- `typos` flags a standalone `mis` token — write "mistargeted", never "mis-targeted".
- Mandatory adversarial multi-agent review panel before push, cycling to convergence.

## Design decisions locked before planning (rationale for reviewers)

These refine the spec; they were settled during planning and the spec is amended to match.

1. **`SavedDesign::solve*` stays compression-only.** The spec (§Persistence) said `solve*` dispatches on family and *solves* extension. But `solve_with_material` returns `Result<SpringDesign>` and extension solves to `ExtensionDesign` — a different type — so uniform engine-side solve dispatch would need a `FamilyDesign` return sum type that **only tests would exercise** (the GUI re-solves extension via its scenario, never via `SavedDesign`). Per YAGNI, `DesignSpec` is **serialization-only**; `solve_with_material` keeps returning `SpringDesign`, matches `DesignSpec::Compression(s)` as today, and returns a documented `SpringError::InconsistentInputs` for `DesignSpec::Extension(_)` (semantically honest: `SpringDesign` *is* the compression output type). The extension GUI form solves by building `extension::scenario::PowerUser` and calling `.solve()` directly.
2. **Two typed outcome fields**, not a `FamilyOutcome` enum: `App.outcome: Option<FormOutcome>` (compression, unchanged) + `App.ext_outcome: Option<ExtFormOutcome>`. Keeps compression's ~15 `app.outcome` readers untouched; each family module stays self-contained. The inactive family's stale outcome is never rendered because `view()` dispatches on `app.family` and `SelectFamily` recomputes.
3. **Shared calculator chrome is extracted** (`calculator.rs`) before the skeleton — header/units/nav/status/footer are already family-agnostic; extension is the second occurrence, so DRY mandates extraction.
4. **Shared helpers are lifted before the second family exists (Task 5).** The numeric-parse helpers, unit-conversion/label helpers, and output-row render helpers are genuinely-shared logic blocks. Per the spec's deferred-items note ("lift, don't duplicate") and DRY-on-second-occurrence, they move to shared modules in Task 5 (compression-only, behavior-preserving) so Task 6's extension module consumes them. Family-specific *glue* (`field_value`/`calc_field_id` — keyed by each family's own `Field` enum) stays per-family: it is not a duplicated logic block.
5. **Persistence is one late task** (after `ExtFormState` exists), constructing *both* `DesignSpec` variants at once — so there is never a single-variant enum nor a dead variant at any commit. Compression keeps `SavedDesign.scenario: ScenarioSpec` until that task; the skeleton solves extension via the scenario directly (no `build_spec` needed for solving).
6. **Walking skeleton**: `Family` enum + selector + dispatch + a thin (but rendering + solving) extension module land atomically in Task 6, because `-D warnings` + no-`allow(dead_code)` means `#[cfg(test)]`-only references do not keep production functions alive.

---

## File Structure

**springcore/src/**
- `extension/design.rs` — `ExtensionDesign` gains `status: DesignStatus`; new private `evaluate_status(index, &[ExtLoadPoint])`, called in `solve_forward`. (Task 1)
- `family.rs` *(new)* — `pub enum Family { Compression, Extension }` (`Default` = `Compression`, serde). `pub mod family;` + `pub use family::{Family, ALL_FAMILIES};` in `lib.rs`. (Task 6)
- `persistence.rs` — `SavedDesign.scenario: ScenarioSpec` → `design: DesignSpec`; new `DesignSpec`, `ExtScenarioSpec`, `HookSpecSpec`; `solve_with_material` matches `DesignSpec`. (Task 8)

**springmaker/src/**
- `form_helpers.rs` *(new)* — shared `pub(crate)` numeric-parse + display + `format_error` helpers lifted out of `compression/form.rs`. (Task 5)
- `presenter.rs` — `FieldDescriptor` → generic `FieldDescriptor<F>` (Task 3); shared `status_kind`, the `GoverningRate` primitive, and the unit-conversion/label helpers (`display_len`/`unit_*_label`/…) lifted in (Tasks 5, 6).
- `widgets.rs` — gains the shared output-row render helpers (`result_row`*, `render_result_row`, `rows_section`, `divided_result_section`, `render_governing_rate`) and a message-parameterized `labeled_input`. (Task 5)
- `app.rs` — `material`/`unit_system` lifted to `App`; `Field` moved out; `family`, `extension`, `ext_outcome` added; `Message` gains `SelectFamily`/`ExtField`/`ExtHookMode` (and `Field`→`CompField`); `update`/`recompute`/`view`/`save_to`/`apply_saved` dispatch on `family`. (Tasks 2, 3, 6, 8)
- `calculator.rs` *(new)* — shared Calculator-screen shell: header (units radios, nav), family selector, status panel, footer; dispatches the design+results body to the active family. (Tasks 4, 6)
- `compression/{form,view_model,view}.rs` — `Field` becomes `compression::form::Field`; presenter/view read `unit_system`/`material` from `App`; chrome → `calculator.rs`; shared helpers → `form_helpers`/`presenter`/`widgets`. (Tasks 2, 3, 4, 5)
- `extension/mod.rs` *(new)* — `pub(crate) mod form; pub(crate) mod view_model; pub(crate) mod view;`. (Task 6)
- `extension/form.rs` *(new)* — `Field`, `HookMode`, `ExtFormState`, `ExtFormOutcome`, `parse_and_solve` (consuming shared helpers), and (Task 8) `build_spec`/`populate_from_spec`. (Tasks 6, 8)
- `extension/view_model.rs` *(new)* — `ExtensionDesign` → geometry `ResultRow`s + status lines (Task 6); three-stress `ExtLoadTable`/`ExtLoadRow` (Task 7).
- `extension/view.rs` *(new)* — humble PowerUser view: inputs + hook toggle + governing rate + geometry + status (Task 6); three-stress table (Task 7).
- `ui_tests.rs` — extension E2E flows. (Task 9)
- `main.rs` — add `mod form_helpers; mod calculator; mod extension;`. (Tasks 4, 5, 6)

---

## Task 1: Engine — `ExtensionDesign.status` (springcore, mutation-gated)

**Files:**
- Modify: `springcore/src/extension/design.rs` (struct at 30-44; `solve_forward` construction at 210-223; new `evaluate_status`; tests in `mod tests`)

**Interfaces:**
- Consumes: `crate::design::{DesignStatus, Severity, StatusMessage, index_caution}` (all in-crate; `index_caution` is `pub(crate)`), `ExtLoadPoint` (this file).
- Produces: `ExtensionDesign.status: DesignStatus` — read by `springmaker` Task 6.

This mirrors the **torsion** `evaluate_status` precedent in `springcore/src/torsion/design.rs`. The status carries: a per-load-point **overstress** `Warning` for each of the three stresses that exceeds its allowable (`pct_* > 1.0`), plus the shared **index caution** via `index_caution(index)`.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `springcore/src/extension/design.rs`. A helper builds a known-clean design and known-overstressed designs. The clean baseline `forward_solve_basic_design` already exists — reuse `crate::test_support::music_wire()`. Boundary pins mirror torsion (kill `>`→`>=` and the format-string mutants).

```rust
    // ── status: clean baseline ───────────────────────────────────────────────
    #[test]
    fn clean_design_has_no_warnings() {
        // d=2mm D=20mm index=10 (in 4..=12 band), moderate load → no overstress.
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
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert!(!d.status.has_warnings(), "clean design must have no warnings: {:?}", d.status.messages);
        assert!(d.status.messages.is_empty(), "clean in-band design has an empty status");
    }

    // ── status: index caution (Severity::Caution) ────────────────────────────
    #[test]
    fn out_of_band_index_raises_caution() {
        // d=2mm D=40mm → index 20 (> 12). Load kept small so only the index caution fires.
        let m = crate::test_support::music_wire();
        let d = solve_forward(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(40.0),
            10.0,
            Length::from_millimeters(60.0),
            Force::from_newtons(1.0),
            HookEnds::default_for(Length::from_millimeters(40.0)),
            &[Force::from_newtons(2.0)],
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert!(
            d.status.messages.iter().any(|msg|
                msg.severity == crate::Severity::Caution && msg.message.contains("spring index")),
            "index 20 must raise a Caution, got: {:?}", d.status.messages
        );
    }

    // ── status: overstress per stress, load-point indexed (Severity::Warning) ──
    /// Build a design whose single load point overstresses ALL THREE stresses, then
    /// assert each stress produces a distinct indexed Warning naming that stress.
    fn overstressed() -> ExtensionDesign {
        // Music wire d=1mm D=8mm index=8; huge load → all three pct_* exceed 1.0.
        let m = crate::test_support::music_wire();
        solve_forward(
            &m,
            Length::from_millimeters(1.0),
            Length::from_millimeters(8.0),
            10.0,
            Length::from_millimeters(60.0),
            Force::from_newtons(0.0),
            HookEnds::default_for(Length::from_millimeters(8.0)),
            &[Force::from_newtons(500.0)],
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap()
    }

    #[test]
    fn body_overstress_raises_indexed_warning() {
        let d = overstressed();
        assert!(d.load_points[0].pct_body_allow > 1.0, "fixture must overstress body shear");
        assert!(
            d.status.messages.iter().any(|msg|
                msg.severity == crate::Severity::Warning
                && msg.message.contains("load point 1")
                && msg.message.contains("body shear")),
            "expected indexed body-shear warning, got: {:?}", d.status.messages
        );
    }

    #[test]
    fn hook_bending_overstress_raises_indexed_warning() {
        let d = overstressed();
        assert!(d.load_points[0].pct_hook_bending_allow > 1.0, "fixture must overstress hook bending");
        assert!(
            d.status.messages.iter().any(|msg|
                msg.severity == crate::Severity::Warning
                && msg.message.contains("load point 1")
                && msg.message.contains("hook bending")),
            "expected indexed hook-bending warning, got: {:?}", d.status.messages
        );
    }

    #[test]
    fn hook_torsion_overstress_raises_indexed_warning() {
        let d = overstressed();
        assert!(d.load_points[0].pct_hook_torsion_allow > 1.0, "fixture must overstress hook torsion");
        assert!(
            d.status.messages.iter().any(|msg|
                msg.severity == crate::Severity::Warning
                && msg.message.contains("load point 1")
                && msg.message.contains("hook torsion")),
            "expected indexed hook-torsion warning, got: {:?}", d.status.messages
        );
    }

    /// Boundary: pct == 1.0 exactly (at the allowable, not over it) must NOT warn.
    /// Kills the `> → >=` mutant on each of the three comparisons. Drives
    /// `evaluate_status` directly with a hand-built load point at the boundary.
    #[test]
    fn exactly_at_allowable_raises_no_overstress_warning() {
        let lp = ExtLoadPoint {
            force: Force::from_newtons(1.0),
            deflection: Length::from_meters(0.0),
            length: Length::from_meters(0.06),
            body_shear: Stress::from_pascals(1.0),
            hook_bending: Stress::from_pascals(1.0),
            hook_torsion: Stress::from_pascals(1.0),
            pct_body_allow: 1.0,
            pct_hook_bending_allow: 1.0,
            pct_hook_torsion_allow: 1.0,
        };
        // index 10 is in-band → no caution either, so a clean status is expected.
        let status = evaluate_status(10.0, std::slice::from_ref(&lp));
        assert!(!status.has_warnings(), "pct == 1.0 must not warn: {:?}", status.messages);
    }
```

Add `use crate::units::Stress;` to the test module imports (the boundary test constructs `Stress`). `Length`/`Force` are already imported there.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p springcore --lib extension::design`
Expected: FAIL — `status` field and `evaluate_status` do not exist (compile errors).

- [ ] **Step 3: Add the field, the function, and populate it**

In the `ExtensionDesign` struct (after `load_points`, line 43), add:

```rust
    /// Engineering status (overstress warnings, index caution) computed by
    /// `solve_forward`. Mirrors the torsion family's engine-computed status.
    pub status: DesignStatus,
```

Change the file's `use crate::{...}` line (10) to `use crate::{CurvatureCorrection, DesignStatus, Result, Severity, SpringError, StatusMessage};` (these are crate-root re-exports).

Add the private function just above `solve_forward`:

```rust
/// Engineering checks for an extension design: per-load-point overstress on each
/// of the three stresses, plus the shared spring-index caution. Mirrors the
/// torsion `evaluate_status` precedent.
fn evaluate_status(index: f64, load_points: &[ExtLoadPoint]) -> DesignStatus {
    let mut messages = Vec::new();
    for (i, lp) in load_points.iter().enumerate() {
        if lp.pct_body_allow > 1.0 {
            messages.push(StatusMessage {
                severity: Severity::Warning,
                message: format!(
                    "load point {}: body shear stress is {:.0}% of allowable",
                    i + 1,
                    lp.pct_body_allow * 100.0
                ),
            });
        }
        if lp.pct_hook_bending_allow > 1.0 {
            messages.push(StatusMessage {
                severity: Severity::Warning,
                message: format!(
                    "load point {}: hook bending stress is {:.0}% of allowable",
                    i + 1,
                    lp.pct_hook_bending_allow * 100.0
                ),
            });
        }
        if lp.pct_hook_torsion_allow > 1.0 {
            messages.push(StatusMessage {
                severity: Severity::Warning,
                message: format!(
                    "load point {}: hook torsion stress is {:.0}% of allowable",
                    i + 1,
                    lp.pct_hook_torsion_allow * 100.0
                ),
            });
        }
    }
    if let Some(msg) = crate::design::index_caution(index) {
        messages.push(msg);
    }
    DesignStatus { messages }
}
```

In `solve_forward`, after `load_points` is built (line 208) and before the `Ok(ExtensionDesign { … })` literal, add `let status = evaluate_status(index, &load_points);`, and add `status,` to the struct literal (after `load_points,`).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p springcore --lib extension::design`
Expected: PASS — new tests plus the existing `solve_forward` tests (which get `status` for free).

- [ ] **Step 5: Confirm no other `ExtensionDesign` literal needs the field**

Run: `cargo build -p springcore 2>&1 | head -30`
Expected: clean build. (All producers route through `solve_forward`; none constructs the struct literal directly.)

- [ ] **Step 6: Mutation-gate the new code**

Run: `git add -A && git diff --cached origin/main > /tmp/t1.diff && cargo mutants --in-diff /tmp/t1.diff -p springcore`
Expected: **0 survivors**. A survivor on a `format!`/severity line → add a content assertion; on a comparison line → add a boundary pin like `exactly_at_allowable_raises_no_overstress_warning`.

- [ ] **Step 7: Commit**

```bash
git add springcore/src/extension/design.rs
git commit -m "feat(extension): engine-computed DesignStatus (overstress + index caution)"
```

---

## Task 2: Compression refactor — lift `material` + `unit_system` to `App` (springmaker, behavior-preserving)

**Files:**
- Modify: `springmaker/src/compression/form.rs` (`FormState` 91-117 + `Default` 119-148; `build_spec` 241; `populate_from_spec` 356; `compute_fatigue` 465; `parse_and_solve` 502; `#[cfg(test)]` `FormState{…}` literals)
- Modify: `springmaker/src/compression/view_model.rs` (`results_view` 144; `inputs_view` 341; reads of `app.form.unit_system`)
- Modify: `springmaker/src/compression/view.rs` (reads of `app.form.unit_system`, `app.form.material`)
- Modify: `springmaker/src/app.rs` (`App` struct, `from_store`, `recompute`, `update`, `save_to`, `apply_saved`, materials-editor arms, tests)
- Modify: `springmaker/src/ui_tests.rs` (only if it constructs `FormState` with these fields or reads `app.form.material/unit_system`)

**Interfaces:**
- Produces: `App.material: String`, `App.unit_system: UnitSystem`; `FormState` no longer has `material`/`unit_system`; `parse_and_solve(form, material_name: &str, unit_system: UnitSystem, materials, correction)`; `build_spec(form, unit_system)`; `populate_from_spec(form, spec, unit_system)`; `compute_fatigue(form, material, design, correction, unit_system)`.

Pure behavior-preserving move: the values that today live on `FormState` move to `App`; every function that read `form.unit_system`/`form.material` now receives them as parameters. The existing test suites are the safety net.

- [ ] **Step 1: Move the fields to `App`, update `from_store`/`Default`**

In `app.rs` `App` (200-229) add near `form`:

```rust
    /// Selected material name (shared across families). Lifted out of `FormState`.
    pub material: String,
    /// Active unit system (shared across families). Lifted out of `FormState`.
    pub unit_system: UnitSystem,
```

In `from_store` (242-257) initialize `material: "Music Wire".into(), unit_system: UnitSystem::Metric,`. Remove `material`/`unit_system` from `compression::form::FormState` (struct 91-117 and `Default` 119-148).

- [ ] **Step 2: Repoint the readers (form.rs)**

- `pub fn build_spec(form: &FormState, us: UnitSystem) -> Result<ScenarioSpec>` — delete `let us = form.unit_system;`, use the parameter.
- `pub fn populate_from_spec(form: &mut FormState, spec: &ScenarioSpec, us: UnitSystem)` — delete `let us = form.unit_system;`.
- `fn compute_fatigue(form, material, design, correction, us: UnitSystem)` — replace `form.unit_system` with `us` (3 sites).
- `pub fn parse_and_solve(form: &FormState, material_name: &str, us: UnitSystem, materials: &MaterialStore, correction) -> Result<FormOutcome>` — replace `form.material` with `material_name` (508, 529, 531), `form.unit_system` with `us` (533), pass `us` to `build_spec`/`compute_fatigue`. The throwaway `SavedDesign { material: material_name.to_string(), unit_system: us, scenario: spec }` keeps its current shape (Task 8 changes `scenario`→`design`).

Update every `#[cfg(test)]` `FormState { material: …, unit_system: …, … }` literal (e.g. `rate_based_metric` 558, `min_weight_metric` 837, the inline forms in `format_error_*`) to drop those two fields and pass them at the call site:

```rust
let out = parse_and_solve(&form, "Music Wire", UnitSystem::Metric, &set, CurvatureCorrection::Bergstrasser).unwrap();
```

For the round-trip tests, thread the same `us` through both `build_spec(&form, us)` and `populate_from_spec(&mut form2, &spec1, us)`.

- [ ] **Step 3: Repoint the readers (view_model.rs, view.rs)**

`view_model.rs`: `results_view`/`inputs_view`/`status_view` take `&App`; replace `app.form.unit_system` with `app.unit_system` (146, 343-345). `view.rs`: replace `app.form.unit_system` (242, 251, 533) with `app.unit_system`, and `app.form.material` (298) with `app.material`.

- [ ] **Step 4: Repoint `app.rs` update/recompute/save/load + materials editor**

- `recompute` (270): `parse_and_solve(&self.form, &self.material, self.unit_system, &self.materials, self.correction)`; `format_error(&e, self.unit_system)`.
- `update`: `Message::Material(m) => { self.material = m; … }`; `Message::Units(u) => { self.unit_system = u; … }`.
- Materials-editor arms reading/writing `self.form.material` (rename-follow 460-462; delete-fallback 490-495) → `self.material`.
- `save_to` (598): `material: self.material.clone(), unit_system: self.unit_system,` and `build_spec(&self.form, self.unit_system)`.
- `apply_saved` (645): `self.material = saved.material; self.unit_system = saved.unit_system; populate_from_spec(&mut self.form, &saved.scenario, self.unit_system);`.

Update `app.rs` tests: every `app.form.material` → `app.material` (761, 834, 843, 989-996, 1015-1023, 1041, `assert_editor_invariants`), and any `app.form.unit_system` → `app.unit_system`.

- [ ] **Step 5: Repoint `view_model.rs` tests + `ui_tests.rs`**

In `view_model.rs` tests, `app_with(form)` must also set `app.material`/`app.unit_system` from the test's intent before `recompute`. Update `input_labels_track_the_unit_system` to set `app.unit_system`. In `ui_tests.rs`, if any flow reads `app.form.material`/`unit_system` or constructs `FormState` with them, repoint to `app.material`/`app.unit_system` (most flows select via rendered widgets and are unaffected).

- [ ] **Step 6: Run the full springmaker suite**

Run: `cargo test -p springmaker`
Expected: PASS — identical behavior, no assertion changes beyond the field relocation.

- [ ] **Step 7: Lint + commit**

Run: `cargo fmt && cargo clippy -p springmaker --all-targets -- -D warnings`
Expected: clean.

```bash
git add springmaker/src
git commit -m "refactor(gui): lift material + unit_system from FormState to App"
```

---

## Task 3: Compression refactor — `Field` → `compression::form::Field`; generic `presenter::FieldDescriptor<F>`

**Files:**
- Modify: `springmaker/src/app.rs` (`Field` enum 144-166 — move out; `Message::Field` type; `set_field`)
- Modify: `springmaker/src/compression/form.rs` (declare `Field` here)
- Modify: `springmaker/src/presenter.rs` (`FieldDescriptor` → `FieldDescriptor<F>`)
- Modify: `springmaker/src/compression/view_model.rs` (`InputsView`, `inputs_view` → `FieldDescriptor<Field>`)
- Modify: `springmaker/src/compression/view.rs` (`render_input`, `field_value`, `calc_field_id`, `styled_text_input` → `compression::form::Field`)

**Interfaces:**
- Produces: `compression::form::Field` (the existing 19 variants, unchanged); `presenter::FieldDescriptor<F> { pub label: String, pub field: F }` with `pub(crate) fn new(label: impl Into<String>, field: F) -> Self`. Compression uses `FieldDescriptor<Field>`.

Generic-izing `FieldDescriptor` is what lets the extension family build `FieldDescriptor<extension::form::Field>` from the *same* shared presenter type (Task 6). Move `Field` into `compression::form` so it is family-owned.

- [ ] **Step 1: Move `Field` into `compression::form`**

Cut the `Field` enum (app.rs 143-166) into `compression/form.rs` (near `ScenarioKind`), keeping `#[derive(Debug, Clone, Copy, PartialEq, Eq)]` and all 19 variants. In `app.rs`, add `Field` to the existing `use crate::compression::form::{…}` import. `Message::Field(Field, String)` refers to the imported type; `set_field` (539) matches the same variants.

- [ ] **Step 2: Make `FieldDescriptor` generic**

In `presenter.rs`, replace the struct + impl:

```rust
/// A labeled input descriptor, generic over the family's field enum. Each family
/// builds `FieldDescriptor<its Field>`; its humble view maps `field` to that
/// family's message variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FieldDescriptor<F> {
    pub label: String,
    pub field: F,
}

impl<F> FieldDescriptor<F> {
    pub(crate) fn new(label: impl Into<String>, field: F) -> Self {
        Self { label: label.into(), field }
    }
}
```

Remove `use crate::app::Field;` from `presenter.rs`.

- [ ] **Step 3: Repoint compression presenter + view**

- `view_model.rs`: `use crate::compression::form::Field;`; `InputsView { primary: Vec<FieldDescriptor<Field>>, fatigue: Vec<FieldDescriptor<Field>> }`; `inputs_view` returns those. `fields`/`labels` test helpers take `&[FieldDescriptor<Field>]`.
- `view.rs`: import `Field` from `compression::form`; `render_input<'a>(app, fd: &FieldDescriptor<Field>)`, `field_value(form, field: Field)`, `calc_field_id(field: Field)`, `styled_text_input(…, field: Field)`.

- [ ] **Step 4: Run the suite + lint**

Run: `cargo test -p springmaker && cargo clippy -p springmaker --all-targets -- -D warnings`
Expected: PASS, clean (rendered tree + messages byte-identical).

- [ ] **Step 5: Commit**

```bash
git add springmaker/src
git commit -m "refactor(gui): move Field into compression::form; make FieldDescriptor generic"
```

---

## Task 4: Shell extraction — shared calculator chrome → `calculator.rs` (behavior-preserving)

**Files:**
- Create: `springmaker/src/calculator.rs`
- Modify: `springmaker/src/compression/view.rs` (expose `design_panel`/`results_panel`; move chrome out)
- Modify: `springmaker/src/app.rs` (`view()` Calculator arm)
- Modify: `springmaker/src/main.rs` (`mod calculator;`)

**Interfaces:**
- Produces: `calculator::view(app: &App) -> Element<'_, Message>` (full Calculator chrome embedding the compression panels); `compression::view::design_panel(app)`/`results_panel(app)` (renamed from `build_design_panel`/`build_results_panel`, `pub(crate)`).

The header (units radios + nav), status panel, and footer (Save/Load) are family-agnostic; extract them so Task 6 can wrap either family's panels in identical chrome. Still **compression-only** — no `Family` enum yet.

- [ ] **Step 1: Create `calculator.rs` with the chrome**

Move `build_header`, `build_status_panel`, `render_status_line`, `build_footer` from `compression/view.rs` into `calculator.rs` (rename `build_header`→`header`, `build_status_panel`→`status_panel`, `build_footer`→`footer`). `status_panel` keeps `use crate::compression::view_model::status_view;` for now (Task 6 swaps it for a family-dispatched call). Assemble the screen exactly as `compression::view::view` does today (203-224), but `left`/`right` call `crate::compression::view::design_panel(app)`/`results_panel(app)`. Only `view` is `pub(crate)`. Bring the needed imports (iced widgets, `widgets::*`, `C`, `Message`, `UnitSystem`).

- [ ] **Step 2: Trim `compression/view.rs`**

Delete the moved functions. Rename `build_design_panel`→`design_panel`, `build_results_panel`→`results_panel`, both `pub(crate)`. Remove the now-unused top-level `view`. Drop imports that became unused (let `clippy -D warnings`/`cargo build` flag them).

- [ ] **Step 3: Repoint `app.rs` and `main.rs`**

`app.rs` `view()` (516-522): `Screen::Calculator => crate::calculator::view(self),`. `main.rs`: add `mod calculator;`.

- [ ] **Step 4: Run the suite + lint**

Run: `cargo test -p springmaker && cargo clippy -p springmaker --all-targets -- -D warnings && RUSTDOCFLAGS="-D warnings" cargo doc -p springmaker --no-deps`
Expected: PASS, clean. The `ui_tests.rs` Simulator drives the same rendered tree.

- [ ] **Step 5: Commit**

```bash
git add springmaker/src
git commit -m "refactor(gui): extract shared calculator shell from compression view"
```

---

## Task 5: Lift shared helpers to shared modules (springmaker, behavior-preserving)

**Files:**
- Create: `springmaker/src/form_helpers.rs`
- Modify: `springmaker/src/presenter.rs` (add the unit-conversion/label helpers + `GoverningRate`)
- Modify: `springmaker/src/widgets.rs` (add the output-row render helpers + message-parameterized `labeled_input`)
- Modify: `springmaker/src/compression/form.rs` (consume `form_helpers`; remove the moved fns)
- Modify: `springmaker/src/compression/view_model.rs` (consume the presenter helpers; remove the moved fns/`GoverningRate`)
- Modify: `springmaker/src/compression/view.rs` (consume the widgets render helpers; remove the moved fns)
- Modify: `springmaker/src/app.rs` (repoint `format_error` import); `main.rs` (`mod form_helpers;`)

**Interfaces:**
- Produces:
  - `form_helpers` (`pub(crate)`): `num`, `positive_num`, `length_mm`, `non_negative_force_n`, `positive_force_n`, `rate_npm`, `loads_n`, `fmt_len`, `fmt_force`, `fmt_rate`, `fmt_loads`, `pub fn format_error(&SpringError, UnitSystem) -> String`, `const MM_PER_M`.
  - `presenter` additions (`pub(crate)`): `unit_length_label`, `unit_force_label`, `unit_rate_label`, `unit_stress_label`, `display_len`, `display_force`, `display_rate`, `display_stress`, `const MM_PER_M`, and `struct GoverningRate { pub value: String, pub unit: String }` (moved from `compression::view_model`).
  - `widgets` additions (`pub(crate)`): `result_row`, `result_row_colored`, `render_result_row`, `rows_section`, `divided_result_section`, `render_governing_rate(&GoverningRate)`, and `labeled_input(label: &str, value: &str, id: &'static str, on_input: impl Fn(String) -> Message + 'static)`.

This is the DRY lift the spec mandates ("lift, don't duplicate") so Task 6's extension module consumes shared logic instead of copying it. It is **compression-only and behavior-preserving** — the existing suite is the net. Family-specific glue (`field_value`, `calc_field_id`, `render_input`) stays in `compression/view.rs`: those are keyed by the compression `Field` enum and emit `Message::Field`, so they are not shared logic.

- [ ] **Step 1: Lift the form helpers**

Create `springmaker/src/form_helpers.rs`. Move verbatim from `compression/form.rs`: `const MM_PER_M` (line 4), `num`, `positive_num`, `length_mm`, `non_negative_force_n`, `positive_force_n`, `rate_npm`, `loads_n`, `fmt_len`, `fmt_force`, `fmt_rate`, `fmt_loads`, and `pub fn format_error`. Mark each `pub(crate)` (keep `format_error` `pub` — it is referenced across modules). Add the module's imports (`springcore::units::{Force, Length, SpringRate}`, `springcore::UnitSystem`, `springcore::{Result, SpringError}`). In `compression/form.rs`, delete those items and add `use crate::form_helpers::{num, positive_num, length_mm, non_negative_force_n, positive_force_n, rate_npm, loads_n, fmt_len, fmt_force, fmt_rate, fmt_loads, format_error};` (the `pub use` re-export is not needed — `app.rs` imports `format_error` directly from `form_helpers` in Step 4). `main.rs`: add `mod form_helpers;`.

- [ ] **Step 2: Lift the presenter conversion helpers + `GoverningRate`**

In `presenter.rs`, add (the small unit-label/conversion fns, `pub(crate)`, moved from `compression/view_model.rs` lines 17-85): `unit_length_label`, `unit_force_label`, `unit_rate_label`, `unit_stress_label`, `display_len`, `display_force`, `display_rate`, `display_stress`, `const MM_PER_M`. Also move the `GoverningRate` struct (view_model 90-94) here (it is a trivial shared display primitive, like `ResultRow`). Add `use springcore::{Force, Length, SpringRate, Stress, UnitSystem};` as needed. **Move the conversion unit tests** (`length_conversion_matches_unit_system`, `force_conversion_matches_unit_system`, `rate_is_displayed_in_per_mm_not_per_meter`, `stress_conversion_carries_the_right_label`, view_model 503-556) into `presenter.rs`'s `#[cfg(test)] mod tests`, since they test the moved helpers. In `compression/view_model.rs`, delete the moved fns + `GoverningRate` + those tests, and `use crate::presenter::{… GoverningRate, display_len, display_force, display_rate, display_stress, unit_length_label, unit_force_label, unit_rate_label, unit_stress_label};`.

- [ ] **Step 3: Lift the view render helpers + generic `labeled_input`**

In `widgets.rs`, add (moved from `compression/view.rs`): `result_row_colored`, `result_row`, `render_result_row`, `rows_section`, `divided_result_section`, `render_governing_rate`. These reference `Message`, `C`, `SZ_*`, and `presenter::{Emphasis, ResultRow, GoverningRate}` — `widgets.rs` already imports `crate::app::{C, Message}`; add the `presenter` imports. Add a **message-parameterized** `labeled_input`:

```rust
/// A labeled text input: muted label above a styled monospace input. The caller
/// supplies the stable widget id and the message constructor, so both families
/// reuse this (compression emits `Message::Field`, extension `Message::ExtField`).
pub(crate) fn labeled_input<'a>(
    label: &str,
    value: &str,
    id: &'static str,
    on_input: impl Fn(String) -> Message + 'static,
) -> Element<'a, Message> {
    iced::widget::column![
        field_label(label.to_owned()),
        iced::widget::text_input("", value)
            .id(id)
            .on_input(on_input)
            .size(SZ_BODY)
            .font(iced::Font::MONOSPACE)
            .style(text_input_style),
    ]
    .spacing(4)
    .into()
}
```

In `compression/view.rs`: delete `result_row*`, `render_result_row`, `rows_section`, `divided_result_section`, `render_governing_rate`, the old `labeled_input`, and `styled_text_input` (folded into `widgets::labeled_input`). Update `render_input` to call the shared helper:

```rust
fn render_input<'a>(app: &'a App, fd: &FieldDescriptor<Field>) -> Element<'a, Message> {
    let field = fd.field;
    crate::widgets::labeled_input(&fd.label, field_value(&app.form, field), calc_field_id(field), move |s| Message::Field(field, s))
}
```

`field_value` and `calc_field_id` stay in `compression/view.rs`. Import the moved render helpers from `crate::widgets`. Use `crate::presenter::GoverningRate` where `render_governing_rate`/`PopulatedResults` reference it.

- [ ] **Step 4: Repoint `app.rs` + remaining imports**

`app.rs`: `use crate::form_helpers::format_error;` (it currently imports `format_error` from `crate::compression::form`; move it). `recompute` already calls `format_error(&e, self.unit_system)` (Task 2) — only the import path changes.

- [ ] **Step 5: Run the suite + lints**

Run: `cargo test -p springmaker`
Expected: PASS — the moved conversion tests now live in `presenter`; all compression behavior unchanged.
Run: `cargo clippy -p springmaker --all-targets -- -D warnings && RUSTDOCFLAGS="-D warnings" cargo doc -p springmaker --no-deps`
Expected: clean (no orphaned/dead helpers).

- [ ] **Step 6: Commit**

```bash
git add springmaker/src
git commit -m "refactor(gui): lift shared form/presenter/widget helpers to shared modules"
```

---

## Task 6: Walking skeleton — `Family` + extension PowerUser wired in (springcore + springmaker)

**Files:**
- Create: `springcore/src/family.rs`; modify `springcore/src/lib.rs` (`pub mod family; pub use family::{Family, ALL_FAMILIES};`)
- Create: `springmaker/src/extension/{mod,form,view_model,view}.rs`; modify `main.rs` (`mod extension;`)
- Modify: `springmaker/src/presenter.rs` (add shared `status_kind`)
- Modify: `springmaker/src/compression/view_model.rs` (use the shared `status_kind`)
- Modify: `springmaker/src/app.rs` (`family`, `extension`, `ext_outcome`; `Message::{SelectFamily, ExtField, ExtHookMode}`, rename `Field`→`CompField`; `update`/`recompute` dispatch)
- Modify: `springmaker/src/calculator.rs` (family selector; family-dispatched body + status)

**Interfaces:**
- Consumes: `ExtensionDesign.status` (Task 1); `extension::scenario::PowerUser`, `HookEnds`, `Force`, `Length` (springcore); generic `FieldDescriptor<F>` (Task 3); shell (Task 4); shared helpers `form_helpers::*`, `presenter::{display_*, unit_*_label, GoverningRate, status_kind}`, `widgets::{labeled_input, rows_section, render_governing_rate, …}` (Task 5).
- Produces: `springcore::{Family, ALL_FAMILIES}`; `extension::form::{Field, HookMode, ExtFormState, ExtFormOutcome, parse_and_solve}`; `extension::view_model::{ext_results_view, ext_status_view, ext_inputs_view, ExtResultsView, ExtPopulatedResults}`; `extension::view::{design_panel, results_panel}`; `App.{family, extension, ext_outcome}`; `Message::{SelectFamily, CompField, ExtField, ExtHookMode}`; `presenter::status_kind`.

After this task the user can pick **Extension**, enter PowerUser inputs (incl. hooks), solve, and see governing rate + geometry + status. The three-stress load table is Task 7.

- [ ] **Step 1 (springcore): add `Family`**

Create `springcore/src/family.rs`:

```rust
//! Spring family discriminant — single source of truth (like `UnitSystem`).
use serde::{Deserialize, Serialize};

/// Which spring family a design belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Family {
    #[default]
    Compression,
    Extension,
}

impl std::fmt::Display for Family {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Family::Compression => "Compression",
            Family::Extension => "Extension",
        })
    }
}

/// All families in selector display order.
pub const ALL_FAMILIES: &[Family] = &[Family::Compression, Family::Extension];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_is_compression() { assert_eq!(Family::default(), Family::Compression); }
    #[test]
    fn display_names_match_serde_tags() {
        assert_eq!(Family::Compression.to_string(), "Compression");
        assert_eq!(Family::Extension.to_string(), "Extension");
    }
}
```

Add to `lib.rs`: `pub mod family;` and `pub use family::{Family, ALL_FAMILIES};`.
Run: `cargo test -p springcore --lib family` → PASS.

- [ ] **Step 2 (springmaker): extension form model + solve**

Create `extension/mod.rs`:

```rust
//! Extension-spring Calculator GUI: form (strings→solve), presenter, humble view.
pub(crate) mod form;
pub(crate) mod view;
pub(crate) mod view_model;
```

Create `extension/form.rs` — **consume the shared `form_helpers`** (do not re-define parse helpers):

```rust
//! Pure extension form-to-design logic. No iced dependency.
use crate::form_helpers::{length_mm, loads_n, non_negative_force_n, positive_num};
use springcore::extension::{ExtensionDesign, HookEnds, PowerUser, Scenario};
use springcore::units::{Force, Length};
use springcore::{CurvatureCorrection, Material, MaterialStore, Result, UnitSystem};

/// Which extension text field a `Message::ExtField` targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    WireDia, MeanDia, Active, FreeLength, InitialTension, Loads, HookR1, HookR2,
}

/// Hook geometry mode (mirrors engine `HookSpec`): standard machine loops
/// (r1 = D/2, r2 = D/4) or user-specified absolute radii.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HookMode { #[default] Default, Custom }

/// Extension PowerUser inputs as raw strings (+ hook mode and custom radii).
#[derive(Debug, Clone)]
pub struct ExtFormState {
    pub wire_dia: String,
    pub mean_dia: String,
    pub active: String,
    pub free_length: String,
    pub initial_tension: String,
    pub loads: String,
    pub hook_mode: HookMode,
    pub hook_r1: String,
    pub hook_r2: String,
}

impl Default for ExtFormState {
    fn default() -> Self {
        Self {
            wire_dia: String::new(), mean_dia: String::new(), active: String::new(),
            free_length: String::new(), initial_tension: String::new(), loads: String::new(),
            hook_mode: HookMode::Default, hook_r1: String::new(), hook_r2: String::new(),
        }
    }
}

/// A solved extension form: the design (which carries engine-computed status).
#[derive(Debug, Clone)]
pub struct ExtFormOutcome { pub design: ExtensionDesign }

fn resolve_hooks(form: &ExtFormState, mean_dia_mm: f64, us: UnitSystem) -> Result<HookEnds> {
    match form.hook_mode {
        HookMode::Default => Ok(HookEnds::default_for(Length::from_millimeters(mean_dia_mm))),
        HookMode::Custom => Ok(HookEnds {
            r1: Length::from_millimeters(length_mm("hook radius r1", &form.hook_r1, us)?),
            r2: Length::from_millimeters(length_mm("hook radius r2", &form.hook_r2, us)?),
        }),
    }
}

/// Parse the form, resolve hooks, build `PowerUser`, and solve. The engine's own
/// input guards remain the defense-in-depth backstop.
pub fn parse_and_solve(
    form: &ExtFormState,
    material_name: &str,
    us: UnitSystem,
    materials: &MaterialStore,
    correction: CurvatureCorrection,
) -> Result<ExtFormOutcome> {
    let material: &Material = materials.get(material_name)?;
    let mean_dia_mm = length_mm("mean diameter", &form.mean_dia, us)?;
    let hooks = resolve_hooks(form, mean_dia_mm, us)?;
    let scenario = PowerUser {
        wire_dia: Length::from_millimeters(length_mm("wire diameter", &form.wire_dia, us)?),
        mean_dia: Length::from_millimeters(mean_dia_mm),
        active: positive_num("active coils", &form.active)?,
        free_length: Length::from_millimeters(length_mm("free length", &form.free_length, us)?),
        initial_tension: Force::from_newtons(non_negative_force_n("initial tension", &form.initial_tension, us)?),
        hooks,
        loads: loads_n(&form.loads, us)?.into_iter().map(Force::from_newtons).collect(),
    };
    let design = scenario.solve(material, correction)?;
    Ok(ExtFormOutcome { design })
}
```

(`format_error` is consumed directly from `crate::form_helpers::format_error` at the call site in `app.rs` recompute — no extension wrapper needed.) Add `#[cfg(test)]` unit tests: a metric PowerUser solves (rate ≈ 2000 N/m for d=2 D=20 active=10); US conversion works; `HookMode::Default` vs `Custom` resolve to different `HookEnds` (Default r1=D/2; Custom parsed); blank/`nan`/`inf`/negative numeric fields error; a `Custom` hook with blank r1 errors. Use `MaterialStore::new(MaterialSet::load_default())`.

- [ ] **Step 3 (springmaker): shared `status_kind` + extension presenter (geometry + status)**

In `presenter.rs`, add and have `compression/view_model.rs` consume it (deleting its private copy 294-300):

```rust
use springcore::Severity;

/// Map a design-message severity to its status-line class. Shared by every family.
pub(crate) fn status_kind(severity: Severity) -> StatusKind {
    match severity {
        Severity::Info => StatusKind::Info,
        Severity::Caution => StatusKind::Caution,
        Severity::Warning => StatusKind::DesignWarning,
    }
}
```

Create `extension/view_model.rs` (ADR 0008 pure presenter), consuming the shared `presenter` conversion helpers + `GoverningRate`:

```rust
use crate::app::App;
use crate::extension::form::Field;
use crate::presenter::{
    display_force, display_len, display_rate, status_kind, unit_force_label, unit_length_label,
    unit_rate_label, FieldDescriptor, GoverningRate, ResultRow, StatusKind, StatusLine,
};
use springcore::{ExtensionDesign, UnitSystem};

/// The three mutually-exclusive states of the extension results panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtResultsView { Error(String), Empty, Populated(Box<ExtPopulatedResults>) }

/// Everything the extension results panel shows when a design is solved.
/// (load_table is added in Task 7.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtPopulatedResults {
    pub governing_rate: GoverningRate,
    pub geometry: Vec<ResultRow>,
}
```

Implement:
- `ext_results_view(app) -> ExtResultsView` — reads `app.ext_outcome`/`app.error` exactly like compression `results_view` reads `app.outcome`/`app.error`.
- `geometry_rows(&ExtensionDesign, us) -> Vec<ResultRow>` — spring index `{:.3}`, active coils `{:.3}`, rate `{:.4}` (via `display_rate`, unit `unit_rate_label`), free length, outer diameter, inner diameter (via `display_len`, unit `unit_length_label`), initial tension (via `display_force`, unit `unit_force_label`).
- `ext_status_view(app) -> Vec<StatusLine>` — mirror compression `status_view`: `action_error` first, then `load_warnings`, then design messages from `app.ext_outcome.as_ref().map(|o| &o.design.status)`, mapping severity via the shared `status_kind`.
- `ext_inputs_view(app) -> Vec<FieldDescriptor<Field>>` — the PowerUser fields with unit-aware labels:
  `Wire diameter (mm|in)`→`WireDia`, `Mean diameter (mm|in)`→`MeanDia`, `Active coils`→`Active`, `Free length (mm|in)`→`FreeLength`, `Initial tension (N|lbf)`→`InitialTension`, `Loads (N|lbf), comma-separated`→`Loads`. (Hook fields render via the view's toggle, not here.)

Presenter unit tests: field set + unit-aware labels; geometry row labels/count; status-line derivation (build an overstressed design through `parse_and_solve`, assert a `DesignWarning` line); `ext_results_view` Empty/Error/Populated.

- [ ] **Step 4 (springmaker): extension humble view**

Create `extension/view.rs` mirroring `compression/view.rs`'s structure, consuming the shared `widgets` render helpers. Provide `pub(crate) fn design_panel(app) -> Element<'_, Message>` and `pub(crate) fn results_panel(app) -> Element<'_, Message>`.

- `design_panel`: a "Setup" group with the **Material** pick list (`styled_pick_list`, bind `app.material`, `Message::Material`) — no end-type/fixity. An "Inputs" group from `ext_inputs_view(app)`, each rendered via `widgets::labeled_input(&fd.label, ext_field_value(&app.extension, fd.field), ext_field_id(fd.field), move |s| Message::ExtField(fd.field, s))`. Define module-private `ext_field_value(form, Field) -> &str` and `ext_field_id(Field) -> &'static str` (the family glue) with ids `ext-wire-dia`, `ext-mean-dia`, `ext-active`, `ext-free-length`, `ext-initial-tension`, `ext-loads`, `ext-hook-r1`, `ext-hook-r2`. A **Hooks** group: a `radio` pair Default/Custom emitting `Message::ExtHookMode(HookMode)`, and when `app.extension.hook_mode == HookMode::Custom`, two `labeled_input`s for `HookR1`/`HookR2` (label `Hook radius r1/r2 (mm|in)`).
- `results_panel`: `match ext_results_view(app)` → Error/Empty/Populated (mirror compression `build_results_panel`). Populated renders `render_governing_rate(&p.governing_rate)`, `section_divider()`, `rows_section("Geometry", &p.geometry)`. No chart, no fatigue, no min-weight. (Load table = Task 7.)

- [ ] **Step 5 (springmaker): wire `App` + messages + dispatch**

`app.rs`:
- `App` gains `pub family: springcore::Family,`, `pub extension: crate::extension::form::ExtFormState,`, `pub ext_outcome: Option<crate::extension::form::ExtFormOutcome>,`. Initialize in `from_store`: `family: Family::default(), extension: ExtFormState::default(), ext_outcome: None,`.
- `Message`: rename `Field(Field, String)` → `CompField(crate::compression::form::Field, String)`; add `SelectFamily(springcore::Family)`, `ExtField(crate::extension::form::Field, String)`, `ExtHookMode(crate::extension::form::HookMode)`. Update the dispatch arm: `Message::CompField(field, value) => { self.set_field(field, value); true }`.
- Add `set_ext_field(&mut self, field, value)` matching the 8 extension fields onto `self.extension.*`.
- `update` arms: `Message::SelectFamily(fam) => { self.family = fam; true }`; `Message::ExtField(f, v) => { self.set_ext_field(f, v); true }`; `Message::ExtHookMode(m) => { self.extension.hook_mode = m; true }`.
- `recompute` dispatches on family:

```rust
pub fn recompute(&mut self) {
    self.action_error = None;
    match self.family {
        Family::Compression => match parse_and_solve(&self.form, &self.material, self.unit_system, &self.materials, self.correction) {
            Ok(out) => { self.outcome = Some(out); self.error = None; }
            Err(e) => { self.outcome = None; self.error = Some(crate::form_helpers::format_error(&e, self.unit_system)); }
        },
        Family::Extension => match crate::extension::form::parse_and_solve(&self.extension, &self.material, self.unit_system, &self.materials, self.correction) {
            Ok(out) => { self.ext_outcome = Some(out); self.error = None; }
            Err(e) => { self.ext_outcome = None; self.error = Some(crate::form_helpers::format_error(&e, self.unit_system)); }
        },
    }
}
```

`calculator.rs`:
- Add the **family selector** `styled_pick_list(springcore::ALL_FAMILIES.to_vec(), Some(app.family), Message::SelectFamily)` into the header row, before the units radios.
- Dispatch body + status on `app.family`:

```rust
let (left, right) = match app.family {
    Family::Compression => (crate::compression::view::design_panel(app), crate::compression::view::results_panel(app)),
    Family::Extension => (crate::extension::view::design_panel(app), crate::extension::view::results_panel(app)),
};
```
- `status_panel`: `let lines = match app.family { Family::Compression => crate::compression::view_model::status_view(app), Family::Extension => crate::extension::view_model::ext_status_view(app) };` (replace the direct `status_view` import).

`main.rs`: add `mod extension;`.

- [ ] **Step 6: Run the workspace suite + lints + mutation**

Run: `cargo test --workspace`
Expected: PASS (compression unchanged; new extension form/presenter tests green).
Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings && RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
Expected: clean — no dead code (every new fn is referenced by the dispatch).
Run: `git add -A && git diff --cached origin/main > /tmp/t6.diff && cargo mutants --in-diff /tmp/t6.diff -p springcore`
Expected: **0 survivors** (only springcore change is `family.rs`, pinned by the Step-1 test).

- [ ] **Step 7: Commit**

```bash
git add springcore/src springmaker/src
git commit -m "feat(extension): selectable extension family with PowerUser solve, geometry + status"
```

---

## Task 7: Extension three-stress hook load table (springmaker)

**Files:**
- Modify: `springmaker/src/extension/view_model.rs` (`ExtLoadTable`/`ExtLoadRow`; add to `ExtPopulatedResults`)
- Modify: `springmaker/src/extension/view.rs` (render the table in `results_panel`)

**Interfaces:**
- Consumes: `ExtLoadPoint.{force, deflection, length, body_shear, hook_bending, hook_torsion, pct_body_allow, pct_hook_bending_allow, pct_hook_torsion_allow}`; shared `presenter::{display_stress, unit_stress_label, display_force, display_len, unit_force_label, unit_length_label}`.
- Produces: `ExtLoadTable { stress_unit: String, rows: Vec<ExtLoadRow> }`, `ExtLoadRow { point, force, deflection, length, body_shear, hook_bending, hook_torsion, pct_body, pct_bending, pct_torsion }` (all `String`); `ExtPopulatedResults.load_table`.

- [ ] **Step 1: Write the failing presenter test**

In `extension/view_model.rs` tests, after a successful `parse_and_solve` (d=2 D=20 active=10 loads "10, 30"):

```rust
    #[test]
    fn ext_load_table_has_three_stress_columns_per_point() {
        let p = ext_populated(&app_with_ext(power_user_metric()));
        assert_eq!(p.load_table.stress_unit, "MPa");
        assert_eq!(p.load_table.rows.len(), 2);
        let r0 = &p.load_table.rows[0];
        assert_eq!(r0.point, "1");
        assert!(r0.pct_body.ends_with('%'));
        assert!(r0.pct_bending.ends_with('%'));
        assert!(r0.pct_torsion.ends_with('%'));
        // distinct stress columns (hook bending typically ≠ body shear).
        assert_ne!(r0.body_shear, r0.hook_bending);
    }
```

Provide `app_with_ext`/`ext_populated`/`power_user_metric` helpers analogous to compression's `app_with`/`populated`/`rate_based_metric` (set `app.family = Family::Extension`, `app.extension = …`, `app.material`/`unit_system`, then `recompute`).

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p springmaker --lib extension::view_model`
Expected: FAIL — `load_table`/`ExtLoadTable` missing.

- [ ] **Step 3: Implement the table**

Add `ExtLoadTable`/`ExtLoadRow` and `ext_load_table(d: &ExtensionDesign, us) -> ExtLoadTable` building one `ExtLoadRow` per `load_point`: force `{:.3}` + `unit_force_label`, deflection/length `{:.4}` + `unit_length_label`, each stress via `display_stress` (`{:.3}`), each `pct_*` as `format!("{:.1}%", pct * 100.0)`, `stress_unit: unit_stress_label(us)`. Add `load_table: ext_load_table(d, us)` to `ExtPopulatedResults` in `ext_results_view`'s populated branch.

- [ ] **Step 4: Render it**

In `extension/view.rs` `results_panel` Populated branch, insert `render_ext_load_table(&p.load_table)` after the geometry section (mirror compression's `render_load_table`, header columns: `Pt | Force | Deflection | Length | Body τ (unit) | Hook σ (unit) | Hook τ (unit) | %τ | %σ | %τ_end`). Use `FillPortion` widths analogous to compression; identical per-cell mono styling.

- [ ] **Step 5: Run + lint**

Run: `cargo test -p springmaker && cargo clippy -p springmaker --all-targets -- -D warnings`
Expected: PASS, clean.

- [ ] **Step 6: Commit**

```bash
git add springmaker/src/extension
git commit -m "feat(extension): three-stress hook load-point table in results"
```

---

## Task 8: Persistence — family-tagged `DesignSpec` (springcore, mutation-gated) + GUI save/load dispatch

**Files:**
- Modify: `springcore/src/persistence.rs` (`SavedDesign`; new `DesignSpec`/`ExtScenarioSpec`/`HookSpecSpec`; `solve_with_material`; tests)
- Modify: `springmaker/src/compression/form.rs` (`parse_and_solve` throwaway `SavedDesign`)
- Modify: `springmaker/src/extension/form.rs` (`build_spec`/`populate_from_spec`)
- Modify: `springmaker/src/app.rs` (`save_to`/`apply_saved` dispatch on `family`)

**Interfaces:**
- Produces: `SavedDesign { material: String, unit_system: UnitSystem, design: DesignSpec }`; `DesignSpec { Compression(ScenarioSpec), Extension(ExtScenarioSpec) }` (`#[serde(tag = "family")]`); `ExtScenarioSpec::PowerUser { wire_dia_mm, mean_dia_mm, active, free_length_mm, initial_tension_n, hooks: HookSpecSpec, loads_n: Vec<f64> }` (`#[serde(tag = "type")]`); `HookSpecSpec { Default, Custom { r1_mm, r2_mm } }` (`#[serde(tag = "mode")]`); `extension::form::{build_spec(form, us) -> Result<DesignSpec>, populate_from_spec(form, &ExtScenarioSpec, us)}`.

The nested internally-tagged schema is **verified to round-trip through TOML text** (a scratch test confirmed `family` wrapping `type`/`mode` serializes/deserializes, and a pre-1b `[scenario]` shape fails). Both `DesignSpec` variants land at once — no dead variant.

- [ ] **Step 1: Write the failing persistence tests (string→struct→string)**

In `persistence.rs` `mod tests` — the real verification is **round-trip through TOML text**, not struct equality (a tag collision passes `assert_eq` yet corrupts on disk):

```rust
    fn ext_power_user_saved() -> SavedDesign {
        SavedDesign {
            material: "Music Wire".into(),
            unit_system: UnitSystem::Metric,
            design: DesignSpec::Extension(ExtScenarioSpec::PowerUser {
                wire_dia_mm: 2.0, mean_dia_mm: 20.0, active: 10.0, free_length_mm: 60.0,
                initial_tension_n: 10.0, hooks: HookSpecSpec::Default, loads_n: vec![10.0, 30.0],
            }),
        }
    }

    #[test]
    fn extension_power_user_round_trips_through_toml_text() {
        let v = ext_power_user_saved();
        let text = v.to_toml().unwrap();
        let back = SavedDesign::from_toml(&text).unwrap();
        assert_eq!(v, back);
        assert_eq!(text, back.to_toml().unwrap()); // stable serialization, no tag collision
    }

    #[test]
    fn hook_spec_custom_round_trips() {
        let mut v = ext_power_user_saved();
        if let DesignSpec::Extension(ExtScenarioSpec::PowerUser { hooks, .. }) = &mut v.design {
            *hooks = HookSpecSpec::Custom { r1_mm: 10.0, r2_mm: 5.0 };
        }
        let back = SavedDesign::from_toml(&v.to_toml().unwrap()).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn compression_design_still_round_trips_under_design_tag() {
        let v = SavedDesign {
            material: "Music Wire".into(), unit_system: UnitSystem::Metric,
            design: DesignSpec::Compression(min_weight_spec(2000.0, 4.0)),
        };
        let back = SavedDesign::from_toml(&v.to_toml().unwrap()).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn pre_1b_scenario_shape_fails_with_data_file_error() {
        let pre_1b = "material = \"Music Wire\"\nunit_system = \"Metric\"\n[scenario]\ntype = \"PowerUser\"\nend_type = \"squared_ground\"\nfixity = \"fixed_fixed\"\nwire_dia_mm = 2.0\nmean_dia_mm = 20.0\nactive = 10.0\nfree_length_mm = 60.0\nloads_n = [10.0, 30.0]\n";
        assert!(matches!(SavedDesign::from_toml(pre_1b), Err(SpringError::DataFile(_))));
    }

    #[test]
    fn solve_with_material_rejects_extension_design() {
        let set = MaterialSet::load_default();
        let m = set.get("Music Wire").unwrap();
        let err = ext_power_user_saved().solve_with_material(m, CurvatureCorrection::Bergstrasser).unwrap_err();
        assert!(matches!(err, SpringError::InconsistentInputs(_)));
    }
```

Confirm `from_toml` maps `toml::de::Error` to `SpringError::DataFile` (it does). The `pre_1b` test pins this.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p springcore --lib persistence`
Expected: FAIL — `DesignSpec`/`ExtScenarioSpec`/`HookSpecSpec` and `SavedDesign.design` do not exist.

- [ ] **Step 3: Implement the schema**

In `persistence.rs`:

```rust
/// A design's family-tagged scenario inputs. `Family` is the discriminant tag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "family")]
pub enum DesignSpec {
    Compression(ScenarioSpec),
    Extension(ExtScenarioSpec),
}

/// Extension scenario inputs (SI millimetres / newtons, as stored). 1c adds the other modes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ExtScenarioSpec {
    PowerUser {
        wire_dia_mm: f64, mean_dia_mm: f64, active: f64, free_length_mm: f64,
        initial_tension_n: f64, hooks: HookSpecSpec, loads_n: Vec<f64>,
    },
}

/// Persisted hook geometry mode (mirrors engine `HookSpec`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode")]
pub enum HookSpecSpec { Default, Custom { r1_mm: f64, r2_mm: f64 } }
```

Change `SavedDesign`: `scenario: ScenarioSpec` → `design: DesignSpec`; add `#[derive(PartialEq)]` to `SavedDesign` if not present. Update `solve_with_material` (251) — wrap the existing match in `match &self.design`:

```rust
match &self.design {
    DesignSpec::Compression(scenario) => match scenario {
        // …the existing PowerUser/TwoLoad/RateBased/Dimensional/MinWeight arms, reading `scenario`…
        ScenarioSpec::MinWeight { .. } => {
            let req = min_weight_request_from_spec(scenario)?;
            solve_min_weight(material, &req, correction).map(|s| s.design)
        }
        // …others unchanged…
    },
    DesignSpec::Extension(_) => Err(SpringError::InconsistentInputs(
        "SavedDesign::solve handles compression designs; extension designs are solved \
         via the extension scenario".into(),
    )),
}
```

Keep the `self.material != material.name` guard at the top, unchanged. Add a doc line to `solve_with_material` noting it is compression-only.

- [ ] **Step 4: Update the GUI construction sites**

- `compression/form.rs` `parse_and_solve` (530): `SavedDesign { material: material_name.to_string(), unit_system: us, design: springcore::DesignSpec::Compression(spec) }`.
- `app.rs` `save_to` (607): dispatch on `self.family`:

```rust
fn save_to(&mut self, path: &std::path::Path) {
    self.action_error = None;
    let design = match self.family {
        Family::Compression => match crate::compression::form::build_spec(&self.form, self.unit_system) {
            Ok(s) => springcore::DesignSpec::Compression(s),
            Err(e) => { self.action_error = Some(e.to_string()); return; }
        },
        Family::Extension => match crate::extension::form::build_spec(&self.extension, self.unit_system) {
            Ok(d) => d,
            Err(e) => { self.action_error = Some(e.to_string()); return; }
        },
    };
    let saved = SavedDesign { material: self.material.clone(), unit_system: self.unit_system, design };
    if let Err(e) = saved.save(path) { self.action_error = Some(e.to_string()); }
}
```

- `app.rs` `apply_saved` (645): set `self.material`/`self.unit_system`, then dispatch + switch family:

```rust
fn apply_saved(&mut self, saved: SavedDesign) {
    self.material = saved.material;
    self.unit_system = saved.unit_system;
    match saved.design {
        springcore::DesignSpec::Compression(spec) => {
            self.family = Family::Compression;
            crate::compression::form::populate_from_spec(&mut self.form, &spec, self.unit_system);
        }
        springcore::DesignSpec::Extension(spec) => {
            self.family = Family::Extension;
            crate::extension::form::populate_from_spec(&mut self.extension, &spec, self.unit_system);
        }
    }
}
```

- [ ] **Step 5: Implement extension `build_spec`/`populate_from_spec` (consume shared display helpers)**

In `extension/form.rs` add (consuming `form_helpers::{fmt_len, fmt_force, fmt_loads}` — do not re-define):

```rust
use crate::form_helpers::{fmt_force, fmt_len, fmt_loads};
use springcore::{DesignSpec, ExtScenarioSpec, HookSpecSpec};

/// FormState → persisted `DesignSpec::Extension` (SI mm/N). Round-trips with `populate_from_spec`.
pub fn build_spec(form: &ExtFormState, us: UnitSystem) -> Result<DesignSpec> {
    let mean_dia_mm = length_mm("mean diameter", &form.mean_dia, us)?;
    let hooks = match form.hook_mode {
        HookMode::Default => HookSpecSpec::Default,
        HookMode::Custom => HookSpecSpec::Custom {
            r1_mm: length_mm("hook radius r1", &form.hook_r1, us)?,
            r2_mm: length_mm("hook radius r2", &form.hook_r2, us)?,
        },
    };
    Ok(DesignSpec::Extension(ExtScenarioSpec::PowerUser {
        wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
        mean_dia_mm,
        active: positive_num("active coils", &form.active)?,
        free_length_mm: length_mm("free length", &form.free_length, us)?,
        initial_tension_n: non_negative_force_n("initial tension", &form.initial_tension, us)?,
        hooks,
        loads_n: loads_n(&form.loads, us)?,
    }))
}

/// Write a persisted `ExtScenarioSpec` back into `form`, converting SI mm/N to display
/// units. After this, `build_spec(form, us)` reproduces the spec.
pub fn populate_from_spec(form: &mut ExtFormState, spec: &ExtScenarioSpec, us: UnitSystem) {
    let ExtScenarioSpec::PowerUser {
        wire_dia_mm, mean_dia_mm, active, free_length_mm, initial_tension_n, hooks, loads_n,
    } = spec;
    form.wire_dia = fmt_len(*wire_dia_mm, us);
    form.mean_dia = fmt_len(*mean_dia_mm, us);
    form.active = format!("{active}");
    form.free_length = fmt_len(*free_length_mm, us);
    form.initial_tension = fmt_force(*initial_tension_n, us);
    form.loads = fmt_loads(loads_n, us);
    match hooks {
        HookSpecSpec::Default => { form.hook_mode = HookMode::Default; }
        HookSpecSpec::Custom { r1_mm, r2_mm } => {
            form.hook_mode = HookMode::Custom;
            form.hook_r1 = fmt_len(*r1_mm, us);
            form.hook_r2 = fmt_len(*r2_mm, us);
        }
    }
}
```

Add a `build_spec_populate_round_trip` test (Default + Custom hook modes): `build_spec(&form, us)` → extract the `ExtScenarioSpec` → `populate_from_spec(&mut form2, spec, us)` → `build_spec(&form2, us)` equals the first.

- [ ] **Step 6: Run workspace suite + mutation gate**

Run: `cargo test --workspace`
Expected: PASS (existing persistence tests updated for `design`; new extension round-trips green; compression GUI behaviorally unchanged).
Run: `git add -A && git diff --cached origin/main > /tmp/t8.diff && cargo mutants --in-diff /tmp/t8.diff -p springcore`
Expected: **0 survivors**. A survivor on the `Extension(_)` error arm → assert the error message content.

- [ ] **Step 7: Commit**

```bash
git add springcore/src springmaker/src
git commit -m "feat(persistence): family-tagged DesignSpec; extension save/load round-trip"
```

---

## Task 9: End-to-end Simulator coverage (springmaker)

**Files:**
- Modify: `springmaker/src/ui_tests.rs`

**Interfaces:**
- Consumes: the headless `Simulator` harness already used for compression E2E (drives the real view→message→update loop; selects widgets by rendered text or widget id).

- [ ] **Step 1: Extension solve flow**

Add a test that constructs the hermetic test `App`; selects **Extension** (`Message::SelectFamily(Family::Extension)` or clicking the rendered "Extension" option); enters PowerUser inputs by widget id (`ext-wire-dia`="2.0", `ext-mean-dia`="20.0", `ext-active`="10", `ext-free-length`="60", `ext-initial-tension`="10", `ext-loads`="10, 30"); drives the update loop; asserts the rendered results contain the three-stress table (assert the "Body"/"Hook" headers and a `%` cell, or `app.ext_outcome.is_some()` and `ext_results_view(&app)` is `Populated`). Follow the existing compression Simulator tests' construction exactly.

- [ ] **Step 2: Hook-toggle flow**

A test that switches `hook_mode` to `Custom` (`Message::ExtHookMode(HookMode::Custom)`), asserts `ext-hook-r1`/`ext-hook-r2` now render, enters valid radii, re-solves (`app.ext_outcome.is_some()`); toggles back to `Default` and re-solves.

- [ ] **Step 3: Save→load round-trip flow**

A test that solves an extension design, `app.save_to(&unique_temp_path)`, constructs a fresh `App`, `app2.load_from(&that_path)`, and asserts `app2.family == Family::Extension`, the extension form fields are populated, and a recompute yields `ext_outcome.is_some()`. Use the `std::env::temp_dir().join(format!("osm_ext_{}.toml", std::process::id()))` + cleanup pattern from the compression `successful_save_clears_a_prior_action_error` test.

- [ ] **Step 4: Run + lint**

Run: `cargo test -p springmaker` (and the `ui_tests` module specifically)
Expected: PASS.
Run: `cargo clippy -p springmaker --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add springmaker/src/ui_tests.rs
git commit -m "test(gui): extension family E2E — solve, hook toggle, save/load round-trip"
```

---

## Final pre-push gate (after all tasks + review convergence)

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
typos
cargo deny check all
cargo test --workspace
git diff origin/main > /tmp/branch.diff && cargo mutants --in-diff /tmp/branch.diff -p springcore   # literal 0 survivors
```

Then update the spec's persistence section to reflect decision #1 (the `solve*` extension-dispatch downgrade) before opening the PR, and run the mandatory adversarial review panel (≥3 reviewers + a MANDATORY input-domain adversary + a persistence/wire-format reviewer for the schema break) to convergence.

---

## Self-Review

**Spec coverage:**
- Family enum in springcore + selector + dispatch → Task 6. ✓
- `App { family, material, unit_system, compression, extension }` → Tasks 2 (material/units), 6 (family/extension). ✓
- Messages `SelectFamily`/`CompField`/`ExtField` (+ `ExtHookMode`) → Task 6. ✓
- Generic `FieldDescriptor<F>` → Task 3. ✓
- `ExtFormState` PowerUser + hook mode; `parse_and_solve` via scenario → Task 6. ✓
- Three-stress `ExtLoadTable` → Task 7. ✓
- Status lines from engine-computed `ExtensionDesign.status` → Task 1 (engine) + Task 6 (presenter). ✓
- `ExtensionDesign.status` mirroring torsion → Task 1. ✓
- Family-tagged `DesignSpec`/`ExtScenarioSpec`/`HookSpecSpec`; back-compat break surfaced as `DataFile` → Task 8. ✓
- `build_spec`/`populate_from_spec`; `save_to`/`apply_saved` dispatch on family → Task 8. ✓
- Error model (parse `Result`, error-vs-outcome exclusivity, `format_error` reuse) → Tasks 5, 6. ✓
- **"Lift, don't duplicate" (spec deferred-items) → Task 5** (shared helpers lifted before the second family consumes them). ✓
- Testing: engine golden+branch+mutation (Task 1), persistence round-trip-through-text + break (Task 8), presenter unit tests (Tasks 6,7), Simulator E2E (Task 9). ✓
- **Spec deviation surfaced:** decision #1 (engine-side extension solve dispatch dropped; `solve*` compression-only) — flag when presenting; amend spec before push.

**Placeholder scan:** No TBD/"add validation"/"similar to Task N". Each step shows code or an exact mechanical edit with file:line anchors. The iced view tasks (6, 7) reference `compression/view.rs` as a named template and specify concrete deltas (panels, ids, columns, messages) rather than re-pasting boilerplate; all *new* logic carries full code; all *shared* logic is lifted (Task 5) and imported, never re-pasted.

**Type consistency:** `parse_and_solve(form, material_name, us, materials, correction)` matches across Tasks 2/6. `FieldDescriptor<F>::new` matches Tasks 3/6/7. Shared helper signatures (`length_mm`, `non_negative_force_n`, `loads_n`, `fmt_len`, `fmt_force`, `fmt_loads`, `display_*`, `unit_*_label`, `status_kind`, `GoverningRate`, `labeled_input`) are defined once in Task 5 and consumed by name in Tasks 6/7/8. `DesignSpec`/`ExtScenarioSpec`/`HookSpecSpec` field names match between schema (Task 8 Step 3), GUI `build_spec`/`populate_from_spec` (Task 8 Step 5), and the verified scratch round-trip. `ExtFormOutcome { design }`, `ExtensionDesign.status`, `ext_outcome` consistent across Tasks 1/6/7/8.
```
