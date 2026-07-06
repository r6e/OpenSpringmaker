# Cross-Family Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clear the panel-recorded cross-family backlog: compression fatigue guard parity, extension enum loudness (+ ADR 0013), a scientific-notation display fallback for result rows, and the form_helpers extraction.

**Architecture:** Two tasks. Task 1 hardens springcore (shared geometry guard promotion, compression `analyze_fatigue` guard restructure in torsion's precedence, `ExtBindingConstraint` loudness + the coupled GUI arm deletion, ADR 0013). Task 2 is springmaker-only (a `fmt_row_value` presenter helper swept across all numeric result-row format sites, plus the form_helpers two-core extraction).

**Tech Stack:** Rust workspace — `springcore` (pure engine, mutation-gated) + `springmaker` (iced 0.14 GUI, presenter pattern per ADR 0008).

## Global Constraints

- springcore is mutation-gated: `cargo mutants --in-diff` vs origin/main must end with literal 0 missed (survivors = failure). springmaker is NOT gated.
- Strict TDD: write the failing test, watch it fail, implement, watch it pass.
- Every existing error message and every message quoted in this plan is VERBATIM — do not reword.
- NO references to any commercial product or vendor in any file.
- MSRV 1.88; `cargo fmt` zero deviation; `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Commit DIRECTLY on `feat/cross-family-hardening`. NEVER push, NEVER create/edit PRs, NEVER run review panels or dispatch subagents, NEVER touch any file under `.git/` (e.g. REVIEW_CONVERGED_OK).
- Conventional commits; every commit message ends with the trailer:
  `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`
- Decision 3 (spec): equal NONZERO cycle forces stay ACCEPTED in compression (Goodman's reciprocal is finite at τa = 0) — the documented divergence from torsion's Gerber. Only the both-zero pair is rejected.

---

### Task 1: springcore hardening — shared geometry guard, compression fatigue guards, extension enum loudness, ADR 0013

**Files:**
- Modify: `springcore/src/design.rs` (receive `validate_wire_mean_geometry`)
- Modify: `springcore/src/torsion/design.rs` (remove the fn; import from new home)
- Modify: `springcore/src/torsion/fatigue.rs:9` (import path)
- Modify: `springcore/src/torsion/scenario.rs:6` (import path)
- Modify: `springcore/src/fatigue.rs` (guard restructure + tests)
- Modify: `springcore/src/extension/optimize.rs` (~line 60: attribute + doc comment)
- Modify: `springmaker/src/extension/view_model.rs:115-123` (delete wildcard arm — SAME commit as the attribute removal)
- Create: `docs/adr/0013-public-enum-exhaustiveness-policy.md`

**Interfaces:**
- Consumes: `SpringError::{InconsistentInputs, NoFatigueData}`, `crate::test_support::{music_wire, material}`, existing `corrected_shear_stress` / `spring_index` helpers.
- Produces: `pub(crate) fn validate_wire_mean_geometry(wire_dia: Length, mean_dia: Length) -> Result<()>` now living in `springcore/src/design.rs` (torsion keeps working; compression's `analyze_fatigue` calls it). `analyze_fatigue`'s signature is UNCHANGED. `ExtBindingConstraint` loses `#[non_exhaustive]`.

- [ ] **Step 1: Move `validate_wire_mean_geometry` to the shared module (pure refactor — existing torsion tests are the net)**

Cut the function (WITH its doc comment, if any, and these exact messages) from `springcore/src/torsion/design.rs:73-92` and paste it into `springcore/src/design.rs` (place it near the other shared cross-family helpers, e.g. beside `index_caution`):

```rust
pub(crate) fn validate_wire_mean_geometry(wire_dia: Length, mean_dia: Length) -> Result<()> {
    let d = wire_dia.meters();
    if !(d.is_finite() && d > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "wire diameter must be a positive finite number".into(),
        ));
    }
    let dm = mean_dia.meters();
    if !(dm.is_finite() && dm > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "mean diameter must be a positive finite number".into(),
        ));
    }
    if dm <= d {
        return Err(SpringError::InconsistentInputs(
            "mean diameter must exceed wire diameter (spring index must exceed 1)".into(),
        ));
    }
    Ok(())
}
```

Update the three torsion consumers to the canonical home:
- `springcore/src/torsion/design.rs`: add `use crate::design::validate_wire_mean_geometry;` (its own call at ~line 101 keeps compiling).
- `springcore/src/torsion/fatigue.rs:9`: `use crate::torsion::design::validate_wire_mean_geometry;` → `use crate::design::validate_wire_mean_geometry;`
- `springcore/src/torsion/scenario.rs:6`: remove `validate_wire_mean_geometry` from the `crate::torsion::design::{...}` list; add `use crate::design::validate_wire_mean_geometry;`

Check `springcore/src/design.rs` has `Length` and `SpringError`/`Result` in scope (it almost certainly does; add to existing `use` lines if not).

- [ ] **Step 2: Run the torsion suite to prove the move is behavior-neutral**

Run: `cargo test -p springcore torsion`
Expected: PASS (all existing torsion tests, including the geometry-guard message pins).

- [ ] **Step 3: Commit the move**

```bash
git add springcore/src/design.rs springcore/src/torsion/design.rs springcore/src/torsion/fatigue.rs springcore/src/torsion/scenario.rs
git commit -m "refactor(springcore): promote validate_wire_mean_geometry to the shared design module

Second consumer incoming (compression fatigue); messages verbatim, torsion
call sites re-pointed to the canonical home.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

- [ ] **Step 4: Write the failing compression guard tests**

Append to the `tests` module in `springcore/src/fatigue.rs` (conventions: `crate::test_support::music_wire()` fixture, `matches!` with message equality):

```rust
#[test]
fn geometry_guard_rejects_zero_wire_before_bad_forces() {
    // Precedence: geometry first — the negative force must NOT be the error.
    let m = crate::test_support::music_wire();
    let err = analyze_fatigue(
        &m,
        Length::from_millimeters(0.0),
        Length::from_millimeters(20.0),
        Force::from_newtons(-5.0),
        Force::from_newtons(30.0),
        crate::CurvatureCorrection::Bergstrasser,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        crate::SpringError::InconsistentInputs(ref msg)
            if msg == "wire diameter must be a positive finite number"
    ));
}

#[test]
fn no_data_beats_bad_forces() {
    // Precedence: data presence before input domain (torsion's order).
    let m = crate::test_support::material("Stainless 302");
    let err = analyze_fatigue(
        &m,
        Length::from_millimeters(2.0),
        Length::from_millimeters(20.0),
        Force::from_newtons(-5.0),
        Force::from_newtons(30.0),
        crate::CurvatureCorrection::Bergstrasser,
    )
    .unwrap_err();
    assert!(matches!(err, crate::SpringError::NoFatigueData(_)));
}

#[test]
fn rejects_negative_cycle_forces() {
    let m = crate::test_support::music_wire();
    let err = analyze_fatigue(
        &m,
        Length::from_millimeters(2.0),
        Length::from_millimeters(20.0),
        Force::from_newtons(-5.0),
        Force::from_newtons(30.0),
        crate::CurvatureCorrection::Bergstrasser,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        crate::SpringError::InconsistentInputs(ref msg)
            if msg == "cycle forces must be finite and non-negative (the endurance data \
                       covers unidirectional compressive loads)"
    ));
}

#[test]
fn rejects_non_finite_cycle_forces() {
    let m = crate::test_support::music_wire();
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let err = analyze_fatigue(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            Force::from_newtons(bad),
            Force::from_newtons(30.0),
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            crate::SpringError::InconsistentInputs(ref msg)
                if msg.starts_with("cycle forces must be finite and non-negative")
        ));
    }
}

#[test]
fn rejects_both_zero_cycle_forces() {
    // Both-zero previously returned Ok with nf = inf — the masquerade class
    // this guard kills. Equal NONZERO forces remain legal (see
    // `equal_forces_min_eq_max_is_accepted`).
    let m = crate::test_support::music_wire();
    let err = analyze_fatigue(
        &m,
        Length::from_millimeters(2.0),
        Length::from_millimeters(20.0),
        Force::from_newtons(0.0),
        Force::from_newtons(0.0),
        crate::CurvatureCorrection::Bergstrasser,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        crate::SpringError::InconsistentInputs(ref msg)
            if msg == "cycle forces must not both be zero (no load cycle to analyze)"
    ));
}

#[test]
fn huge_forces_trip_the_output_finiteness_guard() {
    // 1e305 N is finite and passes every input guard, but the corrected shear
    // stress overflows to inf — must surface as an error, never Ok(inf).
    let m = crate::test_support::music_wire();
    let err = analyze_fatigue(
        &m,
        Length::from_millimeters(2.0),
        Length::from_millimeters(20.0),
        Force::from_newtons(0.0),
        Force::from_newtons(1e305),
        crate::CurvatureCorrection::Bergstrasser,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        crate::SpringError::InconsistentInputs(ref msg)
            if msg == "fatigue analysis produced a non-finite result (inputs exceed the \
                       representable range)"
    ));
}
```

NOTE on the two long message asserts: Rust's string-literal line continuation (`\` + newline + indent) collapses to a single space — write the expected strings so they equal the implementation's exactly. If in doubt, use one long line.

- [ ] **Step 5: Run to verify the new tests fail**

Run: `cargo test -p springcore fatigue::tests`
Expected: the six new tests FAIL (`geometry_guard…` panics inside `spring_index` or returns Ok; `rejects_negative…` currently returns Ok; `rejects_both_zero…` currently returns Ok; `huge_forces…` currently returns Ok). The five pre-existing tests still pass.

- [ ] **Step 6: Restructure `analyze_fatigue` (springcore/src/fatigue.rs:24-76)**

Add the import near the top of the file:

```rust
use crate::design::validate_wire_mean_geometry;
```

Replace the function body's guard section (everything before the `let c = spring_index(...)` line) and add the output guard before `Ok(...)`:

```rust
pub fn analyze_fatigue(
    material: &Material,
    wire_dia: Length,
    mean_dia: Length,
    force_min: Force,
    force_max: Force,
    correction: CurvatureCorrection,
) -> Result<FatigueResult> {
    // Guard order mirrors torsion's `analyze_torsion_fatigue`: geometry → data
    // present → input domain → ordering → degenerate cycle → data trap →
    // compute → output finiteness.
    validate_wire_mean_geometry(wire_dia, mean_dia)?;
    let endurance = material
        .endurance
        .ok_or_else(|| SpringError::NoFatigueData(material.name.clone()))?;
    let lo = force_min.newtons();
    let hi = force_max.newtons();
    if !(lo.is_finite() && lo >= 0.0 && hi.is_finite() && hi >= 0.0) {
        return Err(SpringError::InconsistentInputs(
            "cycle forces must be finite and non-negative (the endurance data \
             covers unidirectional compressive loads)"
                .into(),
        ));
    }
    if hi < lo {
        return Err(SpringError::InconsistentInputs(
            "max cycle force must be at least the min cycle force".into(),
        ));
    }
    // Equal NONZERO forces are legal (τa = 0; Goodman's reciprocal form stays
    // finite) — the documented divergence from torsion's Gerber, which must
    // reject σa = 0. The both-zero pair, though, has no load cycle at all and
    // would produce nf = ∞; reject it precisely rather than letting the output
    // guard below mislabel zeros as "exceeding the representable range".
    if hi == 0.0 {
        return Err(SpringError::InconsistentInputs(
            "cycle forces must not both be zero (no load cycle to analyze)".into(),
        ));
    }

    let c = spring_index(mean_dia, wire_dia);
    let k = correction.factor(c);
    let fa = Force::from_newtons((hi - lo) / 2.0);
    let fm = Force::from_newtons((hi + lo) / 2.0);
    let tau_a = corrected_shear_stress(fa, mean_dia, wire_dia, k);
    let tau_m = corrected_shear_stress(fm, mean_dia, wire_dia, k);

    let sut = material.min_tensile_strength(wire_dia)?.pascals();
    let ssu = SHEAR_TO_TENSILE * sut;
    // Convert Zimmerli pulsating data to a fully-reversed endurance (Shigley Eq. 10-31):
    //   Sse = Ssa / (1 - Ssm/Ssu)
    // If Ssm ≥ Ssu the denominator is ≤ 0, producing ∞ or a negative Sse, which makes
    // the Goodman safety factor meaningless. Guard against this latent trap: with
    // bundled material data this should not occur (Ssm ≪ Ssu), but it would be a silent
    // trap for any future material whose endurance mean-stress meets/exceeds 0.67·Sut.
    if endurance.ssm.pascals() >= ssu {
        return Err(SpringError::InconsistentInputs(format!(
            "material '{}': endurance mean shear stress ({:.3} MPa) meets or exceeds \
             0.67·Sut = {:.3} MPa; cannot compute a valid fully-reversed endurance limit",
            material.name,
            endurance.ssm.pascals() / 1e6,
            ssu / 1e6,
        )));
    }
    let sse = endurance.ssa.pascals() / (1.0 - endurance.ssm.pascals() / ssu);
    // Goodman factor of safety: 1/nf = tau_a/Sse + tau_m/Ssu.
    let nf = 1.0 / (tau_a.pascals() / sse + tau_m.pascals() / ssu);

    // Belt-and-suspenders output guard (torsion's exact shape and message): a
    // finite-input overflow anywhere in the chain must never escape as Ok.
    if [tau_a.pascals(), tau_m.pascals(), sse, ssu, nf]
        .into_iter()
        .any(|v| !v.is_finite())
    {
        return Err(SpringError::InconsistentInputs(
            "fatigue analysis produced a non-finite result (inputs exceed the \
             representable range)"
                .into(),
        ));
    }

    Ok(FatigueResult {
        alternating_stress: tau_a,
        mean_stress: tau_m,
        fully_reversed_endurance: Stress::from_pascals(sse),
        ultimate_shear: Stress::from_pascals(ssu),
        goodman_factor_of_safety: nf,
    })
}
```

(The Ssm ≥ Ssu block, its comment, and the ordering-guard message are byte-identical to today's; only their position and the `lo`/`hi` bindings changed.)

Also update the now-inaccurate comment on the existing `equal_forces_min_eq_max_is_accepted` test (it claims "infinite safety factor"; for 20 N/20 N nf is finite):

```rust
    // Pins the `<` (strict) in the ordering guard: equal NONZERO forces (zero
    // alternating load) must be accepted — the spring cycles at a single load
    // point, a degenerate but valid Goodman case (τa = 0 → nf = Ssu/τm, finite).
    // A `<=` mutant would reject this. The both-zero pair IS rejected — see
    // `rejects_both_zero_cycle_forces`.
```

- [ ] **Step 7: Run to verify all fatigue tests pass**

Run: `cargo test -p springcore fatigue`
Expected: PASS — all pre-existing tests (including `equal_forces_min_eq_max_is_accepted` and `rejects_reversed_force_order`) plus the six new ones. Also run `cargo test -p springmaker compression` (the GUI's `compute_fatigue` routes engine errors verbatim; no test asserts on these strings — expect green).

- [ ] **Step 8: Commit the guards**

```bash
git add springcore/src/fatigue.rs
git commit -m "feat(springcore): compression fatigue guard parity — geometry, input domain, both-zero, output finiteness

Mirrors torsion's guard order; equal nonzero forces stay legal (Goodman
reciprocal, the documented divergence). Kills the Ok(inf) masquerade for
both-zero and overflow inputs.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

- [ ] **Step 9: Extension enum loudness (attribute + GUI arm, ONE commit)**

In `springcore/src/extension/optimize.rs` (~line 58-62), replace the enum's doc comment and remove the attribute:

```rust
/// Which limit determines the chosen extension design.
///
/// Deliberately NOT `#[non_exhaustive]`: the GUI matches this exhaustively,
/// so adding a binding limit is a loud compile-time break at every match
/// site (ADR 0013).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtBindingConstraint {
```

(The five variants and their doc comments are untouched.)

In `springmaker/src/extension/view_model.rs:115-123`, delete the wildcard arm and its comment so the match reads:

```rust
    let binding = match mw.binding {
        ExtBindingConstraint::BodyShear => "body shear",
        ExtBindingConstraint::HookBending => "hook bending",
        ExtBindingConstraint::HookTorsion => "hook torsion",
        ExtBindingConstraint::Index => "index",
        ExtBindingConstraint::OuterDiameter => "outer diameter",
    };
```

These MUST land together: with the attribute removed, the wildcard arm becomes an `unreachable_patterns` warning and fails `-D warnings`.

- [ ] **Step 10: Write ADR 0013**

Create `docs/adr/0013-public-enum-exhaustiveness-policy.md` (match the header style of `docs/adr/0008-*.md`):

```markdown
# 13. Public-enum exhaustiveness policy

Date: 2026-07-06

## Status

Accepted

## Context

springcore's public enums were inconsistent: compression's
`BindingConstraint` and torsion's `FrictionModel` deliberately omit
`#[non_exhaustive]` (a PR #32 scope decision) so that adding a variant is a
compile error at every downstream `match`, while extension's
`ExtBindingConstraint` carried the attribute — forcing the GUI to hold a
silent `_ => "other"` wildcard arm that would hide any future binding limit
at runtime instead of surfacing it at compile time.

## Decision

Enums that a downstream layer must exhaustively `match` on (binding
constraints, friction models) carry NO `#[non_exhaustive]`: a new variant is
a deliberate, loud compile-time break at every match site, and the wildcard
arm is forbidden.

Enums that downstream code only displays or iterates (e.g. `CycleLife`,
`DiaPolicy`, `HookSpec` — constructed or shown via `Display` + an `ALL_*`
const, never matched in the GUI) keep `#[non_exhaustive]`; extending them is
additive and silent by design.

`ExtBindingConstraint` loses the attribute and the GUI wildcard arm dies.

## Consequences

- Adding a binding-constraint variant now fails the workspace build until
  every match site handles it — the failure mode we want.
- Removing `#[non_exhaustive]` is technically a semver-major change;
  springcore is workspace-internal and unpublished, so this is free today.
  If springcore is ever published, match-surface enums are a deliberate
  major-version commitment.
- Alternative considered: keep the attribute and render a visible
  "unknown constraint" label from the wildcard. Rejected — it converts a
  compile-time signal into a runtime discovery.
```

- [ ] **Step 11: Verify the workspace compiles clean and extension tests pass**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo test -p springmaker extension && cargo test -p springcore extension`
Expected: clippy clean (no `unreachable_patterns`), all extension tests PASS (the existing binding-label presenter tests cover the de-wildcarded match).

- [ ] **Step 12: Commit the enum change + ADR**

```bash
git add springcore/src/extension/optimize.rs springmaker/src/extension/view_model.rs docs/adr/0013-public-enum-exhaustiveness-policy.md
git commit -m "refactor(extension): make ExtBindingConstraint loud — drop non_exhaustive + GUI wildcard (ADR 0013)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

- [ ] **Step 13: Mutation gate (springcore in-diff)**

```bash
git diff origin/main...HEAD > /tmp/hardening-t1.diff
cargo mutants --in-diff /tmp/hardening-t1.diff --package springcore
```
Expected: `N mutants tested: … 0 missed`. If any mutant survives, add a killing test (do NOT reclassify survivors as equivalent) and re-run to 0.

---

### Task 2: springmaker — fmt_row_value sweep + form_helpers extraction

**Files:**
- Modify: `springmaker/src/presenter.rs` (helper + const + tests; `GoverningRate::from_rate`)
- Modify: `springmaker/src/compression/view_model.rs` (format-site sweep + one huge-value row test)
- Modify: `springmaker/src/extension/view_model.rs` (sweep + test)
- Modify: `springmaker/src/torsion/view_model.rs` (sweep + test)
- Modify: `springmaker/src/form_helpers.rs` (two cores + wrapper collapse)
- Modify: `springmaker/src/compression/form.rs` (one form-level huge-force fatigue test)

**Interfaces:**
- Consumes: `ResultRow`, `display_stress`/`display_len`/… (presenter.rs), `num`/`positive_num`/`finite_or_err` (form_helpers.rs), the engine output guard from Task 1 (message `"fatigue analysis produced a non-finite result (inputs exceed the representable range)"`).
- Produces: `pub(crate) const SCI_THRESHOLD: f64 = 1e6;` and `pub(crate) fn fmt_row_value(v: f64, decimals: usize) -> String` in `springmaker/src/presenter.rs`; private `positive_to_si` / `non_negative_to_si` in form_helpers.rs (internal only).

- [ ] **Step 1: Write the failing `fmt_row_value` tests**

Append to the `tests` module in `springmaker/src/presenter.rs`:

```rust
#[test]
fn fmt_row_value_fixed_point_below_threshold() {
    assert_eq!(fmt_row_value(0.0, 2), "0.00");
    assert_eq!(fmt_row_value(1234.5678, 2), "1234.57");
    assert_eq!(fmt_row_value(999_999.99, 2), "999999.99");
    assert_eq!(fmt_row_value(-4.2, 3), "-4.200");
}

#[test]
fn fmt_row_value_scientific_at_and_above_threshold() {
    assert_eq!(fmt_row_value(1e6, 2), "1.000e6");
    assert_eq!(fmt_row_value(1e300, 2), "1.000e300");
    assert_eq!(fmt_row_value(-2.5e9, 2), "-2.500e9");
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p springmaker presenter::tests::fmt_row_value`
Expected: FAIL to compile — `fmt_row_value` not found.

- [ ] **Step 3: Implement the helper (spec §C exact code)**

Add to `springmaker/src/presenter.rs` (near `display_stress`):

```rust
/// Result-row values at/above this magnitude (in display units) render in
/// scientific notation; fixed-point below. Guards row layout against
/// huge-but-finite inputs that survive all engine finiteness checks.
pub(crate) const SCI_THRESHOLD: f64 = 1e6;

/// Format a numeric result-row value: fixed-point with `decimals` places
/// below [`SCI_THRESHOLD`], scientific (`{:.3e}`) at or above it.
pub(crate) fn fmt_row_value(v: f64, decimals: usize) -> String {
    if v.abs() >= SCI_THRESHOLD {
        format!("{v:.3e}")
    } else {
        format!("{v:.decimals$}")
    }
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p springmaker presenter::tests::fmt_row_value`
Expected: PASS (2 tests).

- [ ] **Step 5: Sweep every numeric result-row format site through the helper**

Transform rule — a site `format!("{expr:.N}")` or `format!("{:.N}", expr)` becomes `fmt_row_value(expr, N)`; a percent site `format!("{:.1}%", expr)` becomes `format!("{}%", fmt_row_value(expr, 1))`. Import `fmt_row_value` via each file's existing `crate::presenter::{...}` use list. Do NOT touch: integer point-number cells (`format!("{}", i + 1)`), unit-label formats (torsion view_model.rs `"{}/°"` / `"{}/rev"`), or any test-module code.

Production sites (line numbers as of branch start — re-locate by content if drifted):

`springmaker/src/compression/view_model.rs`: 98, 99, 100 (`{:.3}`); 103, 108, 113, 118 (`{:.4}`); 123 (`{:.2}`); 154 (`{stress_val:.3}`); 155 (`{:.1}%`); 176, 177, 180, 183 (`{…:.2}`); 186 (`{:.3}`); 205 (`{:.4}`).

`springmaker/src/extension/view_model.rs`: 71, 72, 73 (`{…:.3}`); 74, 75, 76 (`{:.1}%`); 125 (`{:.4}`); 159, 160 (`{:.3}`); 161, 164, 169, 174, 179 (`{:.4}`).

`springmaker/src/torsion/view_model.rs`: 50–54 (`{…:.2}`); 57 (`{:.3}`); 118 (`{stress_val:.3}`); 119 (`{:.1}%`); 168, 169 (`{:.3}`); 177, 186 (`{:.4}`); 228 (`{:.4}`).

`springmaker/src/presenter.rs`: `GoverningRate::from_rate` (~line 269): `value: format!("{:.4}", display_rate(rate, us))` → `value: fmt_row_value(display_rate(rate, us), 4)`.

- [ ] **Step 6: Run the full springmaker suite — existing row tests must stay green**

Run: `cargo test -p springmaker`
Expected: PASS. Every existing fixture value sits far below 1e6, so rendered strings are unchanged. Any failure here means a site was transformed wrong — fix the site, not the test.

- [ ] **Step 7: Write the per-family huge-value row tests**

One test per view_model test module, following each module's existing solved-outcome fixture conventions (each module already builds outcomes by parsing a form and solving — mirror the nearest existing load-table/fatigue-row test's setup). The load magnitude is chosen to be finite, solvable, and yield a stress far above 1e6 display units:

Compression (`springmaker/src/compression/view_model.rs` tests) — mirror the existing load-table test's form fixture, but set the load field to `"1e9"` (metric N; wire 2 mm / mean 20 mm class geometry → τ ≈ 6e9 MPa):

```rust
#[test]
fn huge_finite_stress_renders_scientific_not_digit_wall() {
    // (build the form + solve exactly like the existing load-table test,
    //  with loads = "1e9")
    // then, p = the populated results view:
    let cell = &p.load_table.rows[0].stress;
    assert!(
        cell.contains('e') && cell.len() < 12,
        "expected scientific notation, got '{cell}'"
    );
}
```

Extension: same shape — force field `"1e9"`, assert `rows[0].body_shear` contains `'e'` and is short.

Torsion: moment field `"1e9"` (N·mm), assert the load-table `stress` cell likewise.

If a chosen magnitude fails to solve in a family (an engine guard fires first), reduce it by decades until the solve succeeds while the display value stays ≥ 1e6 — the assertion is the contract, the input magnitude is not.

- [ ] **Step 8: Run the three new tests**

Run: `cargo test -p springmaker huge_finite_stress`
Expected: PASS ×3 (they pass immediately — the sweep in Step 5 is what they pin; they exist to catch a future un-sweep).

- [ ] **Step 9: Commit the presenter fallback**

```bash
git add springmaker/src/presenter.rs springmaker/src/compression/view_model.rs springmaker/src/extension/view_model.rs springmaker/src/torsion/view_model.rs
git commit -m "feat(gui): scientific-notation fallback for huge result-row values (SCI_THRESHOLD 1e6)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

- [ ] **Step 10: form_helpers extraction (refactor — the existing 15 tests are the net and must stay UNTOUCHED)**

Add the two private cores to `springmaker/src/form_helpers.rs` (after `finite_or_err`):

```rust
/// Shared core: parse a strictly-positive value, convert to SI via
/// `convert_us` (US) or pass through unchanged (metric), then
/// finiteness-check the converted result.
fn positive_to_si(
    field: &str,
    value: &str,
    us: UnitSystem,
    convert_us: impl Fn(f64) -> f64,
) -> Result<f64> {
    let v = positive_num(field, value)?;
    let v_si = match us {
        UnitSystem::Us => convert_us(v),
        UnitSystem::Metric => v,
    };
    finite_or_err(field, value, v_si)
}

/// Shared core: like `positive_to_si` but allows zero, rejecting negatives
/// with the "zero or greater" message.
fn non_negative_to_si(
    field: &str,
    value: &str,
    us: UnitSystem,
    convert_us: impl Fn(f64) -> f64,
) -> Result<f64> {
    let v = num(field, value)?;
    if v < 0.0 {
        return Err(SpringError::InconsistentInputs(format!(
            "{field} must be zero or greater"
        )));
    }
    let v_si = match us {
        UnitSystem::Us => convert_us(v),
        UnitSystem::Metric => v,
    };
    finite_or_err(field, value, v_si)
}
```

Collapse the wrappers, KEEPING every existing doc comment verbatim:

```rust
pub(crate) fn length_mm(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    // Lengths must be strictly positive — a zero-length dimension is unphysical.
    positive_to_si(field, value, us, |v| Length::from_inches(v).millimeters())
}

pub(crate) fn non_negative_length_mm(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    non_negative_to_si(field, value, us, |v| Length::from_inches(v).millimeters())
}

pub(crate) fn non_negative_force_n(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    non_negative_to_si(field, value, us, |v| Force::from_pounds_force(v).newtons())
}

pub(crate) fn non_negative_moment_nmm(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    non_negative_to_si(field, value, us, |v| {
        Moment::from_pound_force_inches(v).newton_millimeters()
    })
}

pub(crate) fn positive_force_n(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    positive_to_si(field, value, us, |v| Force::from_pounds_force(v).newtons())
}

pub(crate) fn ang_rate_nmm_per_deg(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    positive_to_si(field, value, us, |v| {
        Moment::from_pound_force_inches(v).newton_millimeters()
    })
}

pub(crate) fn moment_nmm(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    positive_to_si(field, value, us, |v| {
        Moment::from_pound_force_inches(v).newton_millimeters()
    })
}
```

`rate_npm` STAYS UNFACTORED (spec §D escape hatch — its metric arm scales by `MM_PER_M`, not an identity pass-through). Add one line above it:

```rust
// NOTE: deliberately not on `positive_to_si` — the metric arm scales
// (N/mm display → N/m stored); it is not an identity pass-through.
```

- [ ] **Step 11: Run the form_helpers tests + full workspace**

Run: `cargo test -p springmaker form_helpers && cargo test --workspace`
Expected: all 15 existing form_helpers tests PASS unmodified; full workspace green.

- [ ] **Step 12: Commit the extraction**

```bash
git add springmaker/src/form_helpers.rs
git commit -m "refactor(gui): extract positive_to_si / non_negative_to_si form-helper cores

rate_npm stays unfactored (metric arm scales, not identity). Messages
verbatim; the existing helper tests pass untouched.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

- [ ] **Step 13: Write the failing form-level test (compression, engine guard through the form)**

In `springmaker/src/compression/form.rs` tests, mirror the nearest existing fatigue form test's fixture (a valid metric solve), then set `fatigue_min = "0"` and `fatigue_max = "1e305"`:

```rust
#[test]
fn huge_finite_fatigue_force_surfaces_engine_guard_as_form_error() {
    // 1e305 N parses (finite), passes the non-negative helper, and overflows
    // the corrected shear stress inside the engine — the Task-1 output guard
    // must surface as a whole-form error, never Ok(inf) rows.
    // (build the valid metric fixture like the neighboring fatigue tests)
    form.fatigue_min = "0".into();
    form.fatigue_max = "1e305".into();
    let err = parse_and_solve(/* module conventions */).unwrap_err();
    assert!(
        err.to_string().contains("produced a non-finite result"),
        "got: {err}"
    );
}
```

(Adapt the `parse_and_solve` call signature to the module's existing test calls — copy a neighboring test's invocation verbatim.)

- [ ] **Step 14: Run it**

Run: `cargo test -p springmaker compression::form::tests::huge_finite_fatigue_force`
Expected: PASS against the Task-1 engine guard (it would have FAILED before Task 1 — the engine returned Ok with inf stresses; if run before Task 1 lands, it fails, which is its TDD provenance).

- [ ] **Step 15: Commit + full gate**

```bash
git add springmaker/src/compression/form.rs
git commit -m "test(gui): huge finite cycle force surfaces the fatigue output guard as a form error

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

Then the full local CI-parity gate, in order, each with unmasked exit codes:

```bash
cargo fmt --all && cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
typos
git diff origin/main...HEAD > /tmp/hardening-full.diff
cargo mutants --in-diff /tmp/hardening-full.diff --package springcore
```
Expected: all clean; mutation `0 missed`.
