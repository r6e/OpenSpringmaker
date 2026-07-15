# User-Specifiable Inactive-Coil Count Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user specify a helical spring's inactive (dead) coil count explicitly — decoupled from the end-type default — for Compression (all 5 input modes), Conical (PowerUser), and Assembly members.

**Architecture:** Additive closed-coil override. A new `inactive_coils: Option<f64>` (None = end-type default) threads through the geometry engine. The four `EndType` geometry methods are generalized to take an explicit inactive count `Ni`; `design::solve_forward` (the single choke point for all five compression modes and every assembly member) and `conical::solve_forward` resolve `Option → Ni` and pass it down. Extra dead coils beyond the end-type default add exactly `d` (wire diameter) each to free and solid length. Byte-identical at the default `Ni = Ne`; the `free(pitch=d) == solid` invariant is preserved for all four end types, so the existing `FreeLengthBelowMinimum` guard stays correct unchanged. Rate, stress, and natural frequency depend only on active coils and are invariant to `Ni`.

**Tech Stack:** Rust workspace — `springcore` (engine, mutation-gated CI) + `springmaker` (iced 0.14 GUI). Tests: `cargo test`, `approx::assert_relative_eq`. Approved design spec: `docs/superpowers/specs/2026-07-14-inactive-coils-design.md`.

## Global Constraints

- **Backward-compatible by construction:** at the default `Ni = end_type.end_coils()`, every geometry output is byte-identical; missing `inactive_coils` TOML key deserializes to `None` → default (no `#[serde(default)]`; the `arbor_dia_mm`/`max_outer_dia_mm` precedent).
- **Strict TDD:** write the failing test, watch it fail, implement minimally, watch it pass, commit. Run `cargo test -p springcore` / `-p springmaker` after every change.
- **The additive rule (verbatim):** with `Ne = end_type.end_coils()`, `Ni` = resolved inactive count, `d` = wire diameter, `p` = active pitch: `total = active + Ni`; `solid = d·(active+Ni)` (ground: PlainGround/SquaredGround) or `d·(active+Ni+1)` (non-ground: Plain/Squared); `free = base_free(p) + (Ni−Ne)·d`; `pitch_from_free_length` inverts on `free − (Ni−Ne)·d`.
- **Validation:** `inactive` must be finite and ≥ 0. Enforced in the engine (`design::solve_forward` + `conical::solve_forward` — the persistence path bypasses the GUI form) and at GUI entry (`optional_non_negative_num`). Over-specification (dead coils exceed achievable free length) is caught by the existing `FreeLengthBelowMinimum` guard; no new guard for that.
- **ADR 0008 humble-view/pure-presenter split:** the field's *label* (incl. the end-type default hint) and its *inputs-list membership* live in the `*_view_model` presenter; the empty→`None` *parse* lives in the family's `form.rs` (iced-free, but NOT the presenter). The `text_input` placeholder is hard-coded `""` in `widgets.rs:690` and is NOT presenter-reachable — surface the default hint in the `FieldDescriptor` label (the existing `"…(optional)"` idiom, `compression/view_model.rs:269`).
- **Renderer untouched:** it consumes `total_coils`/`active_coils` and already draws `(total−active)/2` dead coils per end (fractional-safe). No `viz/` changes.
- **No new `#[serde(deny_unknown_fields)]`** on the internally-tagged spec enums (`persistence.rs:80-90,119-123,253-256`). Append `inactive_coils` at the END of each spec variant (scalar-after-inline-array is TOML-valid — `clash_allowance` follows `candidate_diameters_mm` today; the ordering hazard is only `[[array-of-tables]]`, which this field is not).
- **No commercial vendor/product names** in any committed file.
- **Process:** never commit to `main` (work on `feat/inactive-coils`); mandatory adversarial multi-agent panel before push; `REVIEW_CONVERGED_OK` marker in its own Bash call; commit trailer `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.

---

## File Structure

**springcore (engine):**
- `src/end_type.rs` — add `resolve_inactive`; generalize `total_coils`/`solid_length`/`free_length`/`pitch_from_free_length` to take `inactive: f64`. (Task 1)
- `src/design.rs` — `solve_forward` gains `inactive: f64` param + the finite/≥0 guard; three call sites use it. (Task 1)
- `src/scenario.rs` — the four scenario structs gain `inactive_coils: Option<f64>`; each `solve()` resolves it. (Tasks 1 default, 2 real)
- `src/optimize.rs` — `MinWeightRequest` gains `inactive_coils`; `solve_min_weight` resolves once. (Tasks 1 default, 2 real)
- `src/conical/design.rs` — `ConicalInputs` gains `inactive_coils`; `conical::solve_forward` resolves it + guards. (Tasks 1 default, 4 real)
- `src/assembly/design.rs` — `AssemblyMember` gains `inactive_coils`; `solve_assembly` resolves per member. (Tasks 1 default, 5 real)
- `src/persistence.rs` — `inactive_coils: Option<f64>` on `ScenarioSpec` ×5, `ConicalSpec::PowerUser`, `AssemblyMemberSpec`; threaded through `solve_with_material` and `min_weight_request_from_spec`. (Tasks 2/4/5)

**springmaker (GUI):**
- `src/form_helpers.rs` — add shared `optional_non_negative_num`. (Task 3)
- `src/compression/{form.rs, view_model.rs, view.rs}`, `src/app.rs` — compression field. (Task 3)
- `src/conical/{form.rs, view_model.rs, view.rs}`, `src/app.rs` — conical field. (Task 4)
- `src/assembly/{form.rs, view.rs}`, `src/app.rs` — per-member field. (Task 5)

**Demo data (repo root):**
- `examples/compression_music_wire.toml`, `examples/conical_chrome_silicon.toml` — carry a non-default `inactive_coils` to showcase dead coils. (Task 6)

**Task dependency order:** 1 → 2 → 3; 4 and 5 depend only on Task 1 (independent spec fields) but execute after 3; 6 runs last (needs the compression + conical spec fields from Tasks 2 and 4). Each task ends green and independently reviewable.

---

## Task 1: Engine geometry generalization + inactive guard (springcore-only, byte-identical)

Generalize the geometry so it accepts an explicit inactive count, defaulting every caller to the end-type value so behavior is unchanged. This task is necessarily atomic: changing the `EndType` method signatures breaks every caller until updated, so all callers move together.

**Files:**
- Modify: `springcore/src/end_type.rs` (methods + `resolve_inactive` + in-file tests)
- Modify: `springcore/src/design.rs:71-155` (`solve_forward` param, guard, 3 call sites)
- Modify: `springcore/src/scenario.rs:32,86,128,164` (4 call sites → pass `end_coils()`)
- Modify: `springcore/src/optimize.rs:201,226` (2 call sites → pass `end_coils()`)
- Modify: `springcore/src/conical/design.rs:162,166-168,178-182` (3 method calls → pass `end_coils()`)
- Modify: `springcore/src/assembly/design.rs:137,175` (+ test call at `:458`) (→ pass `end_coils()`)

**Interfaces:**
- Produces: `EndType::resolve_inactive(self, inactive: Option<f64>) -> f64` (= `inactive.unwrap_or(self.end_coils())`); `EndType::{total_coils,solid_length,free_length,pitch_from_free_length}` now take a trailing `inactive: f64`; `design::solve_forward(..., active: f64, inactive: f64, free_length: Length, ...)` — `inactive` inserted immediately after `active`.
- Note: `active_coils(total)` is intentionally NOT generalized — its sole caller is a test (`end_type.rs:92`); no inactive-aware consumer exists (YAGNI). `conical::solve_forward`'s public signature does NOT change (it resolves internally); only its internal `EndType` method calls gain the arg.

- [ ] **Step 1: Add `resolve_inactive` + a test.** In `springcore/src/end_type.rs`, add inside `impl EndType` (after `end_coils`):

```rust
    /// Resolve an optional user-supplied inactive-coil count to a concrete value,
    /// defaulting to this end type's Shigley Table 10-1 count when unset. Single
    /// source of the "None = end-type default" rule for every family.
    pub fn resolve_inactive(self, inactive: Option<f64>) -> f64 {
        inactive.unwrap_or(self.end_coils())
    }
```

Add to the `tests` module:

```rust
    #[test]
    fn resolve_inactive_defaults_to_end_coils_else_passes_through() {
        assert_eq!(EndType::SquaredGround.resolve_inactive(None), 2.0);
        assert_eq!(EndType::Plain.resolve_inactive(None), 0.0);
        assert_eq!(EndType::PlainGround.resolve_inactive(None), 1.0);
        assert_eq!(EndType::SquaredGround.resolve_inactive(Some(3.5)), 3.5);
        assert_eq!(EndType::Plain.resolve_inactive(Some(0.0)), 0.0);
    }
```

- [ ] **Step 2: Run it — passes** (method + test added together): `cargo test -p springcore end_type::tests::resolve_inactive -- --nocapture`. Expected: PASS.

- [ ] **Step 3: Generalize the four geometry methods.** In `springcore/src/end_type.rs`, replace the method bodies so each takes a trailing `inactive: f64` and uses it instead of `self.end_coils()`:

```rust
    /// Total coils: Nt = Na + Ni (Ni = inactive count; Shigley Table 10-1 at the default).
    pub fn total_coils(self, active: f64, inactive: f64) -> f64 {
        active + inactive
    }

    /// Solid (fully compressed) length (Shigley Table 10-1, generalized to Ni).
    pub fn solid_length(self, wire_dia: Length, active: f64, inactive: f64) -> Length {
        let d = wire_dia.meters();
        let nt = self.total_coils(active, inactive);
        let ls = match self {
            // Ground ends: Ls = d * Nt
            Self::PlainGround | Self::SquaredGround => d * nt,
            // Non-ground ends: Ls = d * (Nt + 1)
            Self::Plain | Self::Squared => d * (nt + 1.0),
        };
        Length::from_meters(ls)
    }

    /// Free length from pitch: base per-end formula + (Ni − Ne)·d additive closed-coil term.
    pub fn free_length(self, wire_dia: Length, active: f64, pitch: Length, inactive: f64) -> Length {
        let d = wire_dia.meters();
        let p = pitch.meters();
        let base = match self {
            Self::Plain => p * active + d,
            Self::PlainGround => p * (active + 1.0),
            Self::Squared => p * active + 3.0 * d,
            Self::SquaredGround => p * active + 2.0 * d,
        };
        Length::from_meters(base + d * (inactive - self.end_coils()))
    }

    /// Pitch that yields a given free length (inverse of `free_length`, generalized to Ni).
    pub fn pitch_from_free_length(
        self,
        wire_dia: Length,
        active: f64,
        free_length: Length,
        inactive: f64,
    ) -> Length {
        let d = wire_dia.meters();
        // Subtract the additive closed-coil term, then invert the base per-end formula.
        let l0 = free_length.meters() - d * (inactive - self.end_coils());
        let p = match self {
            Self::Plain => (l0 - d) / active,
            Self::PlainGround => l0 / (active + 1.0),
            Self::Squared => (l0 - 3.0 * d) / active,
            Self::SquaredGround => (l0 - 2.0 * d) / active,
        };
        Length::from_meters(p)
    }
```

(The crate will not compile until Steps 5–9 update the callers — that is expected for this atomic task.)

- [ ] **Step 4: Update the in-file `end_type.rs` tests to pass `e.end_coils()`, keeping every asserted value verbatim.** These existing numeric asserts ARE the byte-identical proof. In each of `squared_ground_relations`, `plain_relations`, `plain_ground_free_length_uses_na_plus_one`, `squared_free_length_distinguishes_all_operators`, `plain_pitch_from_free_length`, `plain_ground_pitch_from_free_length`, `squared_pitch_from_free_length`, append `, e.end_coils()` to every `total_coils(...)`, `solid_length(...)`, `free_length(...)`, `pitch_from_free_length(...)` call. Example (in `squared_ground_relations`):

```rust
        assert_relative_eq!(e.total_coils(na, e.end_coils()), 10.0, max_relative = 1e-12);
        assert_relative_eq!(e.solid_length(d, na, e.end_coils()).millimeters(), 20.0, max_relative = 1e-12);
        assert_relative_eq!(e.free_length(d, na, p, e.end_coils()).millimeters(), 44.0, max_relative = 1e-12);
        assert_relative_eq!(e.pitch_from_free_length(d, na, l0, e.end_coils()).millimeters(), 5.0, max_relative = 1e-12);
```

Leave `active_coils(10.0)` at `:92` unchanged (not generalized).

- [ ] **Step 5: Update `design::solve_forward` — add param, guard, use at 3 sites.** In `springcore/src/design.rs`, change the signature (insert `inactive` after `active`):

```rust
pub fn solve_forward(
    material: &Material,
    end_type: EndType,
    fixity: EndFixity,
    wire_dia: Length,
    mean_dia: Length,
    active: f64,
    inactive: f64,
    free_length: Length,
    loads: &[Force],
    correction: CurvatureCorrection,
) -> Result<SpringDesign> {
```

Immediately after the existing active-coils guard (the block ending at `design.rs:112`), add:

```rust
    // Inactive (dead) coil count must be finite and non-negative. The persistence
    // path (`solve_with_material`) builds scenario structs straight from a loaded
    // TOML and bypasses the GUI form's parse guard, so a negative `inactive_coils`
    // would otherwise yield a silently-wrong total/solid geometry here.
    if !(inactive.is_finite() && inactive >= 0.0) {
        return Err(SpringError::InconsistentInputs(
            "inactive coils must be a finite number ≥ 0".into(),
        ));
    }
```

Update the three geometry lines (`design.rs:140,141,155`):

```rust
    let total_coils = end_type.total_coils(active, inactive);
    let solid_length = end_type.solid_length(wire_dia, active, inactive);
    // ... (free ≥ solid guard unchanged — solid_length now reflects inactive) ...
    let pitch = end_type.pitch_from_free_length(wire_dia, active, free_length, inactive);
```

- [ ] **Step 6: Update `scenario.rs` call sites (pass the default).** In `springcore/src/scenario.rs`, in each of the four `solve()` impls (`PowerUser` `:32`, `TwoLoad` `:86`, `RateBased` `:128`, `Dimensional` `:164`), insert `self.end_type.end_coils(),` as the argument immediately after the `active` argument in the `solve_forward(...)` call. Example (`PowerUser::solve`):

```rust
        solve_forward(
            material,
            self.end_type,
            self.fixity,
            self.wire_dia,
            self.mean_dia,
            self.active,
            self.end_type.end_coils(),
            self.free_length,
            &self.loads,
            correction,
        )
```

(`TwoLoad`/`RateBased` pass their derived `active` then `self.end_type.end_coils()`; `Dimensional` likewise.)

- [ ] **Step 7: Update `optimize.rs` call sites.** In `springcore/src/optimize.rs`: line 201 → `let solid = req.end_type.solid_length(d, active, req.end_type.end_coils());`. In the `solve_forward(...)` call (`:226-236`), insert `req.end_type.end_coils(),` after the `active` argument.

- [ ] **Step 8: Update `conical/design.rs` method calls.** In `springcore/src/conical/design.rs`: line 162 → `let total_coils = inputs.end_type.total_coils(inputs.active_coils, inputs.end_type.end_coils());`. Lines 166-168 → `let solid_length = inputs.end_type.solid_length(inputs.wire_dia, inputs.active_coils, inputs.end_type.end_coils());`. Lines 178-182 (the `pitch_from_free_length(...)` call) → append `, inputs.end_type.end_coils()` as the final arg. (`telescopes` at `:184` is unchanged.)

- [ ] **Step 9: Update `assembly/design.rs` call sites.** In `springcore/src/assembly/design.rs`, in both `solve_assembly` `solve_forward` calls (`:137`, `:175`) insert `m.end_type.end_coils(),` after the `m.active_coils` argument. Do the same for the test-only call at `:458` (`m.end_type.end_coils()`).

- [ ] **Step 10: Run the whole springcore suite — everything green, byte-identical.** Run: `cargo test -p springcore`. Expected: PASS, no numeric assertion changes (the values in Step 4 are the current values). If any pre-existing test's number moved, STOP — a formula generalization is wrong.

- [ ] **Step 11: Write the new proof tests.** Add to `springcore/src/end_type.rs` `tests`:

```rust
    /// Backward-compat lock: at inactive = end_coils(), every geometry output equals
    /// the pre-generalization value for all four end types. Fixture: d=2mm, Na=8, p=5mm.
    #[test]
    fn inactive_equals_end_coils_reproduces_baseline() {
        let d = Length::from_millimeters(2.0);
        let p = Length::from_millimeters(5.0);
        let na = 8.0;
        for e in [EndType::Plain, EndType::PlainGround, EndType::Squared, EndType::SquaredGround] {
            let ne = e.end_coils();
            assert_relative_eq!(e.total_coils(na, ne), na + ne, max_relative = 1e-12);
            // free(p=d) == solid holds for every end type at the default.
            let free_at_d = e.free_length(d, na, d, ne);
            assert_relative_eq!(
                free_at_d.meters(),
                e.solid_length(d, na, ne).meters(),
                max_relative = 1e-12
            );
        }
    }

    /// The free(pitch=d) == solid invariant is preserved for ALL inactive values
    /// (including fractional), for every end type. This is what keeps the
    /// FreeLengthBelowMinimum guard correct with zero change.
    #[test]
    fn free_at_solid_pitch_equals_solid_for_all_inactive() {
        let d = Length::from_millimeters(2.0);
        let na = 8.0;
        for e in [EndType::Plain, EndType::PlainGround, EndType::Squared, EndType::SquaredGround] {
            for ni in [0.0, 1.0, 2.0, 2.5, 4.0] {
                let free_at_d = e.free_length(d, na, d, ni).meters();
                let solid = e.solid_length(d, na, ni).meters();
                assert_relative_eq!(free_at_d, solid, max_relative = 1e-12);
            }
        }
    }

    /// Additive closed-coil term: each unit increase in inactive adds exactly d to
    /// both free length and solid length, for every end type.
    #[test]
    fn each_extra_inactive_coil_adds_one_wire_diameter() {
        let d = Length::from_millimeters(2.0);
        let p = Length::from_millimeters(5.0);
        let na = 8.0;
        for e in [EndType::Plain, EndType::PlainGround, EndType::Squared, EndType::SquaredGround] {
            let ne = e.end_coils();
            let free0 = e.free_length(d, na, p, ne).meters();
            let free1 = e.free_length(d, na, p, ne + 1.0).meters();
            let solid0 = e.solid_length(d, na, ne).meters();
            let solid1 = e.solid_length(d, na, ne + 1.0).meters();
            assert_relative_eq!(free1 - free0, d.meters(), max_relative = 1e-12);
            assert_relative_eq!(solid1 - solid0, d.meters(), max_relative = 1e-12);
        }
    }

    /// pitch_from_free_length inverts free_length under a non-default inactive count.
    #[test]
    fn pitch_inverts_free_length_under_nondefault_inactive() {
        let d = Length::from_millimeters(2.0);
        let na = 8.0;
        let p = Length::from_millimeters(5.0);
        for e in [EndType::Plain, EndType::PlainGround, EndType::Squared, EndType::SquaredGround] {
            let ni = e.end_coils() + 2.0;
            let l0 = e.free_length(d, na, p, ni);
            assert_relative_eq!(
                e.pitch_from_free_length(d, na, l0, ni).millimeters(),
                5.0,
                max_relative = 1e-12
            );
        }
    }
```

- [ ] **Step 12: Run the new tests.** Run: `cargo test -p springcore end_type`. Expected: PASS.

- [ ] **Step 13: Write the engine guard test.** Add to `springcore/src/design.rs` `tests` (mirror an existing `solve_forward` test's setup for the material/args; use `EndType::SquaredGround`, `EndFixity::FixedFixed`, `d=2mm`, `mean=20mm`, `active=10`, `free=60mm`, `loads=[10N]`, `CurvatureCorrection::Bergstrasser`, `crate::test_support::music_wire()`):

```rust
    #[test]
    fn negative_inactive_is_rejected() {
        let err = solve_forward(
            &crate::test_support::music_wire(),
            EndType::SquaredGround,
            EndFixity::FixedFixed,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            -1.0, // inactive
            Length::from_millimeters(60.0),
            &[Force::from_newtons(10.0)],
            CurvatureCorrection::Bergstrasser,
        );
        assert!(matches!(err, Err(SpringError::InconsistentInputs(m)) if m.contains("inactive")));
    }
```

- [ ] **Step 14: Run it.** Run: `cargo test -p springcore design::tests::negative_inactive_is_rejected`. Expected: PASS.

- [ ] **Step 15: Full gate + commit.** Run: `cargo test -p springcore && cargo clippy -p springcore -- -D warnings`. Expected: PASS.

```bash
git add springcore/src/end_type.rs springcore/src/design.rs springcore/src/scenario.rs springcore/src/optimize.rs springcore/src/conical/design.rs springcore/src/assembly/design.rs
git commit -m "feat(core): parameterize spring geometry on an explicit inactive-coil count

Generalize EndType::{total_coils,solid_length,free_length,pitch_from_free_length}
to take an explicit inactive count; add resolve_inactive (None → end-type default);
solve_forward gains an inactive param + a finite/≥0 guard. All callers pass the
end-type default, so behavior is byte-identical. Additive closed-coil model:
free/solid grow by (Ni−Ne)·d; free(p=d)==solid invariant preserved per end type.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Compression backend — thread `Option` through scenario, optimize, persistence (no GUI)

Add the `inactive_coils: Option<f64>` field to the compression engine request structs and specs, flip each family's default from `end_coils()` to `resolve_inactive(...)`, and thread it through the persistence mapping so a saved compression file solves with the override.

**Files:**
- Modify: `springcore/src/scenario.rs` (4 structs + 4 `solve()` + in-file test constructors)
- Modify: `springcore/src/optimize.rs` (`MinWeightRequest` + `solve_min_weight` + in-file test constructors)
- Modify: `springcore/src/persistence.rs` (`ScenarioSpec` ×5 + `solve_with_material` ×4 destructures + `min_weight_request_from_spec`)

**Interfaces:**
- Consumes (from Task 1): `EndType::resolve_inactive`, `solve_forward(..., active, inactive, free_length, ...)`.
- Produces: `scenario::{PowerUser,TwoLoad,RateBased,Dimensional}` each gain `pub inactive_coils: Option<f64>`; `optimize::MinWeightRequest` gains `pub inactive_coils: Option<f64>`; `persistence::ScenarioSpec::{PowerUser,TwoLoad,RateBased,Dimensional,MinWeight}` each gain a trailing `inactive_coils: Option<f64>`.

- [ ] **Step 1: Write a failing solve-override test.** Add to `springcore/src/scenario.rs` `tests`:

```rust
    /// A non-default inactive count adds coils to total and grows free/solid by d
    /// each, while rate is unchanged (rate depends only on active).
    #[test]
    fn power_user_honors_inactive_override() {
        let base = PowerUser {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            active: 10.0,
            inactive_coils: None,
            free_length: Length::from_millimeters(60.0),
            loads: vec![Force::from_newtons(10.0)],
        };
        let d0 = base.solve(&crate::test_support::music_wire(), CurvatureCorrection::Bergstrasser).unwrap();
        let bumped = PowerUser { inactive_coils: Some(3.0), ..base.clone() }; // default is 2 → +1 coil
        let d1 = bumped.solve(&crate::test_support::music_wire(), CurvatureCorrection::Bergstrasser).unwrap();
        assert_relative_eq!(d1.total_coils - d0.total_coils, 1.0, max_relative = 1e-9);
        assert_relative_eq!(d1.solid_length.meters() - d0.solid_length.meters(), 0.002, max_relative = 1e-9);
        assert_relative_eq!(d1.rate.newtons_per_meter(), d0.rate.newtons_per_meter(), max_relative = 1e-12);
    }
```

- [ ] **Step 2: Run it — fails to compile** (`inactive_coils` field missing). Run: `cargo test -p springcore scenario::tests::power_user_honors_inactive_override`. Expected: compile error `missing field inactive_coils` / `no field inactive_coils`.

- [ ] **Step 3: Add the field to all four scenario structs and resolve in `solve()`.** In `springcore/src/scenario.rs`, add `pub inactive_coils: Option<f64>,` to `PowerUser`, `TwoLoad`, `RateBased`, `Dimensional`. In each `solve()`, replace the Task-1 default argument `self.end_type.end_coils()` with `self.end_type.resolve_inactive(self.inactive_coils)`. Example (`PowerUser::solve`):

```rust
            self.active,
            self.end_type.resolve_inactive(self.inactive_coils),
            self.free_length,
```

- [ ] **Step 4: Fix the in-file `scenario.rs` test constructors.** Every existing `PowerUser {...}` / `TwoLoad {...}` / `RateBased {...}` / `Dimensional {...}` literal in the `tests` module (`power_user_passes_through`, `two_load_recovers_rate_and_free_length`, `two_load_rejects_inconsistent_points`, `rate_based_hits_target_rate`, `dimensional_uses_outer_diameter`, `dimensional_rejects_outer_equal_to_wire`, `dimensional_rejects_outer_less_than_wire`, `rate_based_rejects_non_positive_rate`, `two_load_rejects_non_finite_point`, `dimensional_rejects_non_finite_outer`, `dimensional_rejects_zero_outer`) gains `inactive_coils: None,`. (`TwoLoad` has no `end_coils`-dependent free path issue — `None` = default.)

- [ ] **Step 5: Run the override test + full scenario tests.** Run: `cargo test -p springcore scenario`. Expected: PASS.

- [ ] **Step 6: Write a failing fixed-free over-specification test.** (Over-specified dead coils push solid above the fixed free length → `FreeLengthBelowMinimum`. Must use a fixed-free mode — MinWeight derives free from solid so it can't trip this.) Add to `springcore/src/scenario.rs` `tests`:

```rust
    /// Enough extra dead coils push solid length above the user's fixed free length,
    /// tripping the existing FreeLengthBelowMinimum guard (no new guard needed).
    #[test]
    fn over_specified_inactive_trips_free_below_solid() {
        // SquaredGround, d=2mm, Na=10 → solid = d*(Na+Ni). At Ni=2, solid=24mm.
        // free=26mm leaves 2mm (1 extra coil) of headroom; Ni=4 → solid=28mm > 26mm.
        let over = PowerUser {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            active: 10.0,
            inactive_coils: Some(4.0),
            free_length: Length::from_millimeters(26.0),
            loads: vec![Force::from_newtons(5.0)],
        };
        assert!(matches!(
            over.solve(&crate::test_support::music_wire(), CurvatureCorrection::Bergstrasser),
            Err(SpringError::FreeLengthBelowMinimum { .. })
        ));
    }
```

- [ ] **Step 7: Run it.** Run: `cargo test -p springcore scenario::tests::over_specified_inactive_trips_free_below_solid`. Expected: PASS (the guard already exists; solid grew via Task 1's `solid_length`).

- [ ] **Step 8: Write a failing MinWeight test (mass-monotonic + field).** Add to `springcore/src/optimize.rs` `tests` (reuse an existing `MinWeightRequest {...}` fixture's numbers; add `inactive_coils`):

```rust
    /// Raising the inactive count never lowers the minimum achievable mass (each dead
    /// coil is extra wire). The winning (d, D, active) MAY shift, so assert on mass,
    /// not on unchanged geometry.
    #[test]
    fn min_weight_mass_is_nondecreasing_in_inactive() {
        let base = MinWeightRequest {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            required_rate: SpringRate::from_newtons_per_meter(2000.0),
            max_force: Force::from_newtons(50.0),
            index_bounds: (4.0, 12.0),
            max_outer_dia: None,
            candidate_diameters: vec![Length::from_millimeters(2.0), Length::from_millimeters(3.0)],
            clash_allowance: 0.15,
            inactive_coils: None,
        };
        let m0 = solve_min_weight(&crate::test_support::music_wire(), &base, CurvatureCorrection::Bergstrasser).unwrap().mass_kg;
        let bumped = MinWeightRequest { inactive_coils: Some(4.0), ..base.clone() };
        let m1 = solve_min_weight(&crate::test_support::music_wire(), &bumped, CurvatureCorrection::Bergstrasser).unwrap().mass_kg;
        assert!(m1 >= m0 - 1e-15, "mass must not decrease: m0={m0}, m1={m1}");
    }
```

- [ ] **Step 9: Run it — fails to compile** (`inactive_coils` missing on `MinWeightRequest`). Run: `cargo test -p springcore optimize::tests::min_weight_mass_is_nondecreasing_in_inactive`. Expected: compile error.

- [ ] **Step 10: Add the field to `MinWeightRequest` and resolve in `solve_min_weight`.** In `springcore/src/optimize.rs`: add `pub inactive_coils: Option<f64>,` to `MinWeightRequest` (after `clash_allowance`). Near the top of `solve_min_weight` (after the existing validation block, before the `for &d in ...` loop), add `let inactive = req.end_type.resolve_inactive(req.inactive_coils);`. Replace the two Task-1 defaults: line 201 → `let solid = req.end_type.solid_length(d, active, inactive);`; the `solve_forward` call → pass `inactive` (not `req.end_type.end_coils()`) after `active`.

- [ ] **Step 11: Fix in-file `optimize.rs` test constructors.** Add `inactive_coils: None,` to every `MinWeightRequest {...}` literal in the `tests` module (the fixtures at `:261,:282,:303,:364,:587,:611,:732,:776,:819` and any others the compiler flags).

- [ ] **Step 12: Run optimize tests.** Run: `cargo test -p springcore optimize`. Expected: PASS.

- [ ] **Step 13: Write a failing persistence round-trip test.** Add to `springcore/src/persistence.rs` `tests` (near the existing compression round-trip tests):

```rust
    #[test]
    fn power_user_inactive_coils_round_trips_and_defaults_when_absent() {
        // Present: Some(3.0) survives a save/load round trip.
        let spec = ScenarioSpec::PowerUser {
            end_type: "squared_ground".into(),
            fixity: "fixed_fixed".into(),
            wire_dia_mm: 2.0,
            mean_dia_mm: 20.0,
            active: 10.0,
            free_length_mm: 60.0,
            loads_n: vec![10.0, 30.0],
            inactive_coils: Some(3.0),
        };
        let toml = toml::to_string(&spec).unwrap();
        let back: ScenarioSpec = toml::from_str(&toml).unwrap();
        assert_eq!(back, spec);
        // Absent key → None (backward compatibility): a legacy body without the key.
        let legacy = "type = \"PowerUser\"\nend_type = \"squared_ground\"\nfixity = \"fixed_fixed\"\nwire_dia_mm = 2.0\nmean_dia_mm = 20.0\nactive = 10.0\nfree_length_mm = 60.0\nloads_n = [10.0, 30.0]\n";
        let loaded: ScenarioSpec = toml::from_str(legacy).unwrap();
        assert!(matches!(loaded, ScenarioSpec::PowerUser { inactive_coils: None, .. }));
    }
```

- [ ] **Step 14: Run it — fails to compile** (`inactive_coils` missing on the spec). Run: `cargo test -p springcore persistence::tests::power_user_inactive_coils_round_trips`. Expected: compile error.

- [ ] **Step 15: Add `inactive_coils` to the five `ScenarioSpec` variants.** In `springcore/src/persistence.rs`, append `inactive_coils: Option<f64>,` as the LAST field of each of `ScenarioSpec::{PowerUser,TwoLoad,RateBased,Dimensional,MinWeight}` (after `loads_n` / `clash_allowance`). Add a short doc line above the first one mirroring the `max_outer_dia_mm` note (missing key → `None`, no `#[serde(default)]`).

- [ ] **Step 16: Thread it through `solve_with_material` (4 arms) and `min_weight_request_from_spec`.** In `solve_with_material` (`persistence.rs:575-652`): add `inactive_coils,` to each of the `PowerUser`/`TwoLoad`/`RateBased`/`Dimensional` destructure patterns, and add `inactive_coils: *inactive_coils,` to each constructed `scenario::*` struct. (`MinWeight` uses `{ .. }` at `:653` — no change there.) In `min_weight_request_from_spec` (`:398-490`): add `inactive_coils,` to the `MinWeight` destructure; add a validation block mirroring `max_outer_dia_mm` (`if let Some(ni) = inactive_coils { if !(ni.is_finite() && *ni >= 0.0) { return Err(SpringError::InconsistentInputs("inactive_coils must be a finite number ≥ 0".into())); } }`); add `inactive_coils: *inactive_coils,` to the `MinWeightRequest {...}` construction.

- [ ] **Step 17: Fix any other `ScenarioSpec {...}` / `MinWeightRequest {...}` literals the compiler flags** (existing persistence test fixtures, e.g. `:704,:742,:759,:799,:837,:859,:886,:934`) with `inactive_coils: None,`.

- [ ] **Step 18: Run the full springcore suite + clippy — commit.** Run: `cargo test -p springcore && cargo clippy -p springcore -- -D warnings`. Expected: PASS.

```bash
git add springcore/src/scenario.rs springcore/src/optimize.rs springcore/src/persistence.rs
git commit -m "feat(core): thread inactive-coil override through compression backend

Add inactive_coils: Option<f64> to the four scenario structs, MinWeightRequest,
and the five ScenarioSpec variants; resolve via EndType::resolve_inactive at solve.
Persistence (solve_with_material, min_weight_request_from_spec) passes it through;
missing TOML key defaults to None → end-type value. Fixed-free modes trip the
existing FreeLengthBelowMinimum guard on over-specification; MinWeight mass is
non-decreasing in the count.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Compression GUI — expose the field in all five modes

Add the "Inactive coils" input to the compression form for every scenario, with an end-type default hint in its label, and confirm the results panel's total-coils row reflects it. Introduce the shared optional-number parse helper here (first of three family uses).

**Files:**
- Modify: `springmaker/src/form_helpers.rs` (add `optional_non_negative_num`)
- Modify: `springmaker/src/compression/form.rs` (`FormState`, `Field`, `Default`, `build_spec` ×5, `populate_from_spec`)
- Modify: `springmaker/src/compression/view_model.rs` (`inputs_view` descriptor per scenario)
- Modify: `springmaker/src/compression/view.rs` (`field_value`, `calc_field_id`)
- Modify: `springmaker/src/app.rs` (`set_field` arm for the new `Field`)

**Interfaces:**
- Consumes (from Task 2): `ScenarioSpec::*` now carry `inactive_coils`.
- Produces: `form_helpers::optional_non_negative_num(field: &str, value: &str) -> Result<Option<f64>>`; `compression::form::Field::Inactive`; `FormState.inactive: String`.

- [ ] **Step 1: Write a failing helper test.** Add to `springmaker/src/form_helpers.rs` `tests`:

```rust
    #[test]
    fn optional_non_negative_num_parses_blank_and_values() {
        assert_eq!(optional_non_negative_num("inactive coils", "   ").unwrap(), None);
        assert_eq!(optional_non_negative_num("inactive coils", "2").unwrap(), Some(2.0));
        assert_eq!(optional_non_negative_num("inactive coils", "1.5").unwrap(), Some(1.5));
        assert_eq!(optional_non_negative_num("inactive coils", "0").unwrap(), Some(0.0));
        assert!(optional_non_negative_num("inactive coils", "-1").is_err());
        assert!(optional_non_negative_num("inactive coils", "abc").is_err());
    }
```

- [ ] **Step 2: Run it — fails** (function missing). Run: `cargo test -p springmaker form_helpers::tests::optional_non_negative_num`. Expected: compile error.

- [ ] **Step 3: Implement the helper.** In `springmaker/src/form_helpers.rs`, add (next to `num`/`positive_num`):

```rust
/// Parse an optional non-negative count field: blank → None; else a finite ≥ 0
/// number. Shared by every family's optional inactive-coil input.
pub(crate) fn optional_non_negative_num(field: &str, value: &str) -> Result<Option<f64>> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    let v = num(field, value)?; // finite check
    if v < 0.0 {
        return Err(SpringError::InconsistentInputs(format!("{field} must be ≥ 0")));
    }
    Ok(Some(v))
}
```

- [ ] **Step 4: Run it.** Run: `cargo test -p springmaker form_helpers::tests::optional_non_negative_num`. Expected: PASS.

- [ ] **Step 5: Write a failing form↔spec round-trip test.** Add to `springmaker/src/compression/form.rs` `tests`:

```rust
    #[test]
    fn inactive_field_round_trips_through_spec() {
        let mut form = FormState { scenario: ScenarioKind::PowerUser, inactive: "3".into(), ..default_power_user_form() };
        let spec = build_spec(&form, UnitSystem::Metric).unwrap();
        assert!(matches!(spec, ScenarioSpec::PowerUser { inactive_coils: Some(v), .. } if (v - 3.0).abs() < 1e-9));
        form.inactive.clear();
        populate_from_spec(&mut form, &spec, UnitSystem::Metric);
        assert_eq!(form.inactive, "3"); // Some(3.0) → "3"
    }
```

(Use whatever the module's existing valid-form test constructor is; `default_power_user_form()` is a placeholder for the existing fixture helper — reuse the real one, e.g. a filled `FormState`.)

- [ ] **Step 6: Run it — fails to compile** (`inactive` field + spec field). Run: `cargo test -p springmaker compression::form::tests::inactive_field_round_trips`. Expected: compile error.

- [ ] **Step 7: Add the form field, enum variant, default.** In `springmaker/src/compression/form.rs`: add `pub inactive: String,` to `FormState` (`:83-108`); add `Inactive` to the `Field` enum (`:26-48`); add `inactive: String::new(),` to the `Default` impl (`:110-137`).

- [ ] **Step 8: Write `inactive_coils` in all five `build_spec` arms.** In `build_spec` (`:212-282`), add to each of the five `ScenarioSpec::*` constructions:

```rust
        inactive_coils: optional_non_negative_num("inactive coils", &form.inactive)?,
```

(Import `optional_non_negative_num` from `form_helpers` alongside the existing helpers.)

- [ ] **Step 9: Read it back in `populate_from_spec`.** In `populate_from_spec` (`:289-394`), in each arm's destructure add `inactive_coils,` and set (matching the count formatting Tasks 4/5 use):

```rust
            form.inactive = inactive_coils.map(|v| format!("{v}")).unwrap_or_default();
```

Rust's `Display` for `f64` prints an integer-valued value without a decimal point (`format!("{}", 3.0)` → `"3"`, `format!("{}", 1.5)` → `"1.5"`), which matches the existing `active` display idiom (conical `populate` uses `active.to_string()`).

- [ ] **Step 10: Run the round-trip test.** Run: `cargo test -p springmaker compression::form::tests::inactive_field_round_trips`. Expected: PASS. Adjust the expected string in Step 5 if the count formatter differs (`"3"` for `3.0`).

- [ ] **Step 11: Write a failing presenter test (descriptor + default hint).** Add to `springmaker/src/compression/view_model.rs` `tests`:

```rust
    #[test]
    fn inputs_view_includes_inactive_with_end_type_default_hint() {
        // Build an App in PowerUser with SquaredGround selected (default inactive = 2).
        let app = test_app_power_user_squared_ground(); // reuse existing test-app helper
        let inputs = inputs_view(&app);
        let fd = inputs.primary.iter().find(|f| matches!(f.field, Field::Inactive)).expect("inactive descriptor present");
        assert!(fd.label.contains("Inactive coils"));
        assert!(fd.label.contains("default 2"), "label was {:?}", fd.label);
    }
```

- [ ] **Step 12: Run it — fails.** Run: `cargo test -p springmaker compression::view_model::tests::inputs_view_includes_inactive`. Expected: FAIL (no descriptor).

- [ ] **Step 13: Add the descriptor to every scenario arm in `inputs_view`.** In `springmaker/src/compression/view_model.rs` `inputs_view` (`:254-343`), compute the end-type default hint once and push an `Inactive` descriptor in each of the five scenario arms' field lists:

```rust
    let inactive_label = match springcore::parse_end_type(&app.compression_form().end_type) {
        Ok(e) => format!("Inactive coils (default {:.0}, optional)", e.end_coils()),
        Err(_) => "Inactive coils (optional)".to_string(),
    };
    // ... in each scenario arm, alongside the other FieldDescriptor::new(...) pushes:
    FieldDescriptor::new(&inactive_label, Field::Inactive),
```

(Match the exact `&App` accessor the module already uses to reach the compression form and its `end_type` string — the map shows `app` state access in `inputs_view`; use the same path. `parse_end_type` is `springcore::parse_end_type`, already re-exported.)

- [ ] **Step 14: Wire the view plumbing.** In `springmaker/src/compression/view.rs`: add a `Field::Inactive => &form.inactive` arm to `field_value` (`:155-177`); add a `Field::Inactive => "..."` id to `calc_field_id` (`:35-57`, use a unique static id string e.g. `"comp-inactive"`). In `springmaker/src/app.rs`: add a `Field::Inactive => self.compression_form_mut().inactive = value,` (or the module's exact setter idiom) arm to `set_field` (`:1109-1131`).

- [ ] **Step 15: Run presenter + view tests.** Run: `cargo test -p springmaker compression`. Expected: PASS.

- [ ] **Step 16: Write a failing end-to-end total-coils test.** Add to `springmaker/src/compression/form.rs` `tests` (through the real solve path):

```rust
    #[test]
    fn inactive_override_shows_in_total_coils() {
        let materials = MaterialStore::new(MaterialSet::load_default());
        let form = FormState { scenario: ScenarioKind::PowerUser, inactive: "4".into(), ..power_user_squared_ground_form() }; // active=10, SquaredGround default 2
        let out = parse_and_solve(&form, "Music Wire", UnitSystem::Metric, &materials, CurvatureCorrection::Bergstrasser).unwrap();
        assert_relative_eq!(out.design.total_coils, 14.0, max_relative = 1e-9); // active 10 + inactive 4
    }
```

- [ ] **Step 17: Run it.** Run: `cargo test -p springmaker compression::form::tests::inactive_override_shows_in_total_coils`. Expected: PASS.

- [ ] **Step 18: Full gate + commit.** Run: `cargo test -p springmaker && cargo clippy -p springmaker -- -D warnings`. Expected: PASS.

```bash
git add springmaker/src/form_helpers.rs springmaker/src/compression/ springmaker/src/app.rs
git commit -m "feat(gui): inactive-coils input for all five compression modes

Optional 'Inactive coils' field (blank → end-type default) in every compression
scenario, with the end-type default surfaced in the FieldDescriptor label (the
text_input placeholder is not presenter-reachable). Adds the shared
optional_non_negative_num parse helper. Total-coils results row reflects the
override.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Conical — spec field, engine input field, guard, GUI (PowerUser)

**Files:**
- Modify: `springcore/src/persistence.rs` (`ConicalSpec::PowerUser` + field; conical round-trip test)
- Modify: `springcore/src/conical/design.rs` (`ConicalInputs` + field; resolve + guard in `conical::solve_forward`; in-file test constructors)
- Modify: `springmaker/src/conical/form.rs` (`ConFormState`, `Field`, `Default`, `build_spec`, `parse_and_solve` inline `ConicalInputs`, `populate_from_spec`)
- Modify: `springmaker/src/conical/view_model.rs` (`con_inputs_view` descriptor)
- Modify: `springmaker/src/conical/view.rs` (`con_field_value`, `con_field_id`)
- Modify: `springmaker/src/app.rs` (`set_con_field` arm)

**Interfaces:**
- Consumes (Task 1): `EndType::resolve_inactive`, generalized `EndType` methods.
- Produces: `ConicalInputs.inactive_coils: Option<f64>`; `ConicalSpec::PowerUser.inactive_coils: Option<f64>`; `conical::form::Field::Inactive`; `ConFormState.inactive: String`.

- [ ] **Step 1: Write a failing telescopes-invariance + additive test.** Add to `springcore/src/conical/design.rs` `tests` (reuse the `inputs(large_mm, small_mm)` fixture helper at `:350`):

```rust
    #[test]
    fn inactive_grows_geometry_but_not_telescopes() {
        let mut base = inputs(30.0, 18.0); // taper that telescopes; SquaredGround default 2
        base.inactive_coils = None;
        let mats = crate::test_support::music_wire();
        let d0 = solve_forward(&mats, &base, &[Force::from_newtons(20.0)], CurvatureCorrection::Bergstrasser).unwrap();
        let bumped = ConicalInputs { inactive_coils: Some(4.0), ..base.clone() }; // +2 coils
        let d1 = solve_forward(&mats, &bumped, &[Force::from_newtons(20.0)], CurvatureCorrection::Bergstrasser).unwrap();
        assert_eq!(d1.telescopes, d0.telescopes); // telescoping is axial-orthogonal to dead coils
        assert_relative_eq!(d1.total_coils - d0.total_coils, 2.0, max_relative = 1e-9);
        assert_relative_eq!(d1.solid_length.meters() - d0.solid_length.meters(), 0.004, max_relative = 1e-9); // 2 * d(2mm)
        assert_relative_eq!(d1.rate.newtons_per_meter(), d0.rate.newtons_per_meter(), max_relative = 1e-12);
    }

    #[test]
    fn conical_rejects_negative_inactive() {
        let bad = ConicalInputs { inactive_coils: Some(-1.0), ..inputs(30.0, 18.0) };
        assert!(matches!(
            solve_forward(&crate::test_support::music_wire(), &bad, &[Force::from_newtons(20.0)], CurvatureCorrection::Bergstrasser),
            Err(SpringError::InconsistentInputs(m)) if m.contains("inactive")
        ));
    }
```

- [ ] **Step 2: Run — fails to compile** (`inactive_coils` missing on `ConicalInputs`). Run: `cargo test -p springcore conical::design::tests::inactive_grows_geometry`. Expected: compile error.

- [ ] **Step 3: Add the field + resolve + guard.** In `springcore/src/conical/design.rs`: add `pub inactive_coils: Option<f64>,` to `ConicalInputs` (`:16-26`). In `conical::solve_forward`, after the existing `active_coils` guard (`:129`), add:

```rust
    let inactive = inputs.end_type.resolve_inactive(inputs.inactive_coils);
    if !(inactive.is_finite() && inactive >= 0.0) {
        return Err(SpringError::InconsistentInputs(
            "inactive coils must be a finite number ≥ 0".into(),
        ));
    }
```

Replace the three Task-1 defaults (`inputs.end_type.end_coils()`) at `:162`, `:166-168`, `:178-182` with `inactive`.

- [ ] **Step 4: Fix in-file `ConicalInputs` test constructors.** Add `inactive_coils: None,` to the fixture helper at `:351` and every explicit `ConicalInputs {...}` literal in the `tests` module (`:672,:710,:733,:777,:870,:909,:975`).

- [ ] **Step 5: Run conical engine tests.** Run: `cargo test -p springcore conical`. Expected: PASS.

- [ ] **Step 6: Add the spec field + round-trip test.** In `springcore/src/persistence.rs`, append `inactive_coils: Option<f64>,` as the last field of `ConicalSpec::PowerUser` (`:198-206`). Add a round-trip test mirroring Task 2 Step 13 but for `ConicalSpec::PowerUser` (present `Some(3.0)` survives; absent key → `None`). Fix any existing `ConicalSpec::PowerUser {...}` test literals (`:1957,:2011`) with `inactive_coils: None,`.

- [ ] **Step 7: Run persistence conical tests.** Run: `cargo test -p springcore persistence`. Expected: PASS.

- [ ] **Step 8: Write a failing GUI round-trip + solve test.** Add to `springmaker/src/conical/form.rs` `tests`:

```rust
    #[test]
    fn conical_inactive_round_trips_and_solves() {
        let mut form = ConFormState { inactive: "4".into(), ..valid_conical_form() }; // end_type squared_ground, active 8
        let spec = build_spec(&form, UnitSystem::Metric).unwrap();
        assert!(matches!(spec, ConicalSpec::PowerUser { inactive_coils: Some(v), .. } if (v - 4.0).abs() < 1e-9));
        form.inactive.clear();
        populate_from_spec(&mut form, &spec, UnitSystem::Metric);
        assert_eq!(form.inactive, "4");
        let materials = MaterialStore::new(MaterialSet::load_default());
        let out = parse_and_solve(&form, "Music Wire", UnitSystem::Metric, &materials, CurvatureCorrection::Bergstrasser).unwrap();
        assert_relative_eq!(out.design.total_coils, 12.0, max_relative = 1e-9); // active 8 + inactive 4
    }
```

- [ ] **Step 9: Run — fails to compile.** Run: `cargo test -p springmaker conical::form::tests::conical_inactive_round_trips`. Expected: compile error.

- [ ] **Step 10: Wire the conical form.** In `springmaker/src/conical/form.rs`: add `pub inactive: String,` to `ConFormState` (`:25-35`); add `Inactive` to `Field` (`:13-21`); `inactive: String::new(),` to `Default` (`:37-49`); in `build_spec` (`:109-119`) add `inactive_coils: optional_non_negative_num("inactive coils", &form.inactive)?,`; in `parse_and_solve`'s inline `ConicalInputs {...}` (`:83-98`) add `inactive_coils: optional_non_negative_num("inactive coils", &form.inactive)?,`; in `populate_from_spec` (`:122-142`) destructure `inactive_coils` and set `form.inactive = inactive_coils.map(|v| format!("{v}")).unwrap_or_default();`. Import `optional_non_negative_num`.

- [ ] **Step 11: Run the GUI round-trip test.** Run: `cargo test -p springmaker conical::form::tests::conical_inactive_round_trips`. Expected: PASS.

- [ ] **Step 12: Add the presenter descriptor + view plumbing.** In `springmaker/src/conical/view_model.rs` `con_inputs_view` (`:161-172`), push an `Inactive` `FieldDescriptor` with the end-type default hint (same pattern as Task 3 Step 13, using the conical form's `end_type`). Add a presenter test asserting the descriptor's label contains `"Inactive coils"` and `"default 2"` (SquaredGround). In `springmaker/src/conical/view.rs`: add `Field::Inactive` arms to `con_field_value` (`:68`) and `con_field_id` (`:80`, id `"con-inactive"`). In `springmaker/src/app.rs`: add the `Field::Inactive` arm to `set_con_field` (`:1189-1200`).

- [ ] **Step 13: Full gate + commit.** Run: `cargo test -p springcore -p springmaker && cargo clippy --workspace -- -D warnings`. Expected: PASS.

```bash
git add springcore/src/conical/design.rs springcore/src/persistence.rs springmaker/src/conical/ springmaker/src/app.rs
git commit -m "feat(conical): user-specifiable inactive-coil count

ConicalInputs + ConicalSpec::PowerUser gain inactive_coils: Option<f64>; conical
solve resolves + guards it and threads it through solid/free/pitch. Telescoping is
invariant to dead coils (geometry grows by (Ni−Ne)·d; rate unchanged). GUI PowerUser
form exposes the optional field with the end-type default hint.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Assembly — per-member inactive-coil count

Add a per-member "Inactive coils" input. Assembly's member solve routes through `design::solve_forward` (so the Task-1 engine guard already covers it — no assembly-specific guard). Assembly's presenter has no total-coils row; the override's effect surfaces in per-member solid length and the aggregate free/solid/travel math.

**Files:**
- Modify: `springcore/src/persistence.rs` (`AssemblyMemberSpec` + field; member round-trip test)
- Modify: `springcore/src/assembly/design.rs` (`AssemblyMember` + field; resolve per member at the two `solve_forward` calls; in-file test constructors)
- Modify: `springmaker/src/assembly/form.rs` (`AsmMemberForm`, `MemberField`, `blank`, `build_spec` member closure, `parse_and_solve` member closure, `populate_from_spec`)
- Modify: `springmaker/src/assembly/view.rs` (`member_card` row, `asm_member_field_id`)
- Modify: `springmaker/src/app.rs` (`Message::AsmField` inline arm for the new `MemberField`)

**Interfaces:**
- Consumes (Task 1): `EndType::resolve_inactive`, `design::solve_forward(..., active, inactive, ...)`, the engine guard.
- Produces: `AssemblyMember.inactive_coils: Option<f64>`; `AssemblyMemberSpec.inactive_coils: Option<f64>`; `assembly::form::MemberField::Inactive`; `AsmMemberForm.inactive: String`.

- [ ] **Step 1: Write a failing per-member effect test.** Add to `springcore/src/assembly/design.rs` `tests` (reuse `baseline_member()`/`soft_member()` helpers):

```rust
    /// A member's extra dead coils raise its solid length, which raises the nested
    /// assembly's solid length (nested solid = max member solid) and shrinks travel.
    #[test]
    fn member_inactive_raises_nested_solid_length() {
        let mats = MaterialStore::new(MaterialSet::load_default());
        let m0 = baseline_member(); // inactive_coils: None
        let inputs0 = AssemblyInputs { topology: Topology::Nested, members: vec![m0.clone(), soft_member()] };
        let d0 = solve_assembly(&mats, &inputs0, &[Force::from_newtons(30.0)], EndFixity::FixedFixed, CurvatureCorrection::Bergstrasser).unwrap();
        let m1 = AssemblyMember { inactive_coils: Some(m0.end_type.end_coils() + 2.0), ..baseline_member() };
        let inputs1 = AssemblyInputs { topology: Topology::Nested, members: vec![m1, soft_member()] };
        let d1 = solve_assembly(&mats, &inputs1, &[Force::from_newtons(30.0)], EndFixity::FixedFixed, CurvatureCorrection::Bergstrasser).unwrap();
        assert!(d1.solid_length.meters() > d0.solid_length.meters());
    }
```

(Choose loads/geometry so `baseline_member` is the governing (largest-solid) member; if `soft_member` governs, adjust so the bumped member governs — the point is the +2 coils move the aggregate.)

- [ ] **Step 2: Run — fails to compile** (`inactive_coils` missing on `AssemblyMember`). Run: `cargo test -p springcore assembly::design::tests::member_inactive_raises_nested_solid`. Expected: compile error.

- [ ] **Step 3: Add the field + resolve per member.** In `springcore/src/assembly/design.rs`: add `pub inactive_coils: Option<f64>,` to `AssemblyMember` (`:25-31`). At each of the two `crate::design::solve_forward(...)` calls in `solve_assembly` (`:137`, `:175`), replace the Task-1 default `m.end_type.end_coils()` with `m.end_type.resolve_inactive(m.inactive_coils)`.

- [ ] **Step 4: Fix in-file `AssemblyMember` test constructors.** Add `inactive_coils: None,` to `baseline_member` (`:414`), `soft_member` (`:426`), and every explicit `AssemblyMember {...}` literal in the `tests` module (`:609,:628,:682,:697,:725,:743,:776,:789,:799,:813,:820,:838,:962,:980,:1010,:1041,:1095,:1145`) and the test `solve_forward` at `:458` if it constructs one. (The compiler enumerates every site.)

- [ ] **Step 5: Run assembly engine tests.** Run: `cargo test -p springcore assembly`. Expected: PASS.

- [ ] **Step 6: Add the spec field + round-trip test.** In `springcore/src/persistence.rs`, append `pub inactive_coils: Option<f64>,` as the last field of `AssemblyMemberSpec` (`:237-244`). Add a round-trip test (member with `Some(3.0)` survives; a legacy member table without the key → `None`). Fix existing `AssemblyMemberSpec {...}` test literals (`:2060` and any others) with `inactive_coils: None,`.

- [ ] **Step 7: Run persistence tests.** Run: `cargo test -p springcore persistence`. Expected: PASS.

- [ ] **Step 8: Write a failing GUI member round-trip test.** Add to `springmaker/src/assembly/form.rs` `tests`:

```rust
    #[test]
    fn member_inactive_round_trips() {
        let mut form = valid_two_member_asm_form();
        form.members[0].inactive = "3".into();
        let spec = build_spec(&form, UnitSystem::Metric).unwrap();
        let AssemblySpec::PowerUser { members, .. } = &spec;
        assert!(matches!(members[0].inactive_coils, Some(v) if (v - 3.0).abs() < 1e-9));
        // Readback
        let mut form2 = AsmFormState::with_default_material("Music Wire");
        populate_from_spec(&mut form2, &spec, UnitSystem::Metric, "Music Wire");
        assert_eq!(form2.members[0].inactive, "3");
    }
```

- [ ] **Step 9: Run — fails to compile.** Run: `cargo test -p springmaker assembly::form::tests::member_inactive_round_trips`. Expected: compile error.

- [ ] **Step 10: Wire the assembly form.** In `springmaker/src/assembly/form.rs`: add `pub inactive: String,` to `AsmMemberForm` (`:23-31`); add `Inactive` to `MemberField` (`:14-20`); add `inactive: String::new(),` to `AsmMemberForm::blank` (`:34-44`); in `build_spec`'s member closure (`:143-159`) add `inactive_coils: optional_non_negative_num("inactive coils", &m.inactive)?,`; in `parse_and_solve`'s member closure (`:109-121`) add `inactive_coils: optional_non_negative_num("inactive coils", &m.inactive)?,`; in `populate_from_spec` member mapping (`:179-189`) add `inactive: member_spec.inactive_coils.map(|v| format!("{v}")).unwrap_or_default(),` (match the exact per-member field name used in that mapping). Import `optional_non_negative_num`.

- [ ] **Step 11: Run the GUI round-trip test.** Run: `cargo test -p springmaker assembly::form::tests::member_inactive_round_trips`. Expected: PASS.

- [ ] **Step 12: Wire the per-member view + message.** In `springmaker/src/assembly/view.rs`: add a `labeled_input` row for inactive to `member_card` (`:351-408`), alongside the existing rows, closure `move |v| Message::AsmField(index, MemberField::Inactive, v)`, label with the member's end-type default hint (parse `m.end_type`); add a `MemberField::Inactive` arm to `asm_member_field_id` (`:336-345`, id e.g. `format!("asm-{index}-inactive")`). In `springmaker/src/app.rs`: add a `MemberField::Inactive => m.inactive = value,` arm to the inline `Message::AsmField` match (`:776-781`).

- [ ] **Step 13: Full gate + commit.** Run: `cargo test -p springcore -p springmaker && cargo clippy --workspace -- -D warnings`. Expected: PASS.

```bash
git add springcore/src/assembly/design.rs springcore/src/persistence.rs springmaker/src/assembly/ springmaker/src/app.rs
git commit -m "feat(assembly): per-member inactive-coil count

AssemblyMember + AssemblyMemberSpec gain inactive_coils: Option<f64>; each member's
solve resolves it (the design::solve_forward guard already covers assembly). Extra
dead coils raise per-member solid length, flowing into the aggregate free/solid and
travel-limit math. Per-member GUI input with the end-type default hint.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Demo example files — showcase dead coils

Update one or two existing demo files to carry a non-default `inactive_coils`, so the live showcase visibly demonstrates extra dead coils (tight-wound end coils beyond the end-type default). Extension and torsion example files are out of scope (close-wound; no `inactive_coils` field) and MUST be left unchanged. Follow the established demo-file discipline: generate from `SavedDesign` structs, verify through the real load+solve path, remove the throwaway generator (files persist).

**Files:**
- Modify: `examples/compression_music_wire.toml` (Compression PowerUser — bump `inactive_coils` above the end-type default)
- Modify: `examples/conical_chrome_silicon.toml` (Conical PowerUser — same)
- (Throwaway, deleted after use) a `#[cfg(test)]` generator/verifier module in `springmaker` (a bin crate — no lib — so the generator must be an in-crate test, per the demo-file convention)

**Interfaces:**
- Consumes: `ScenarioSpec::PowerUser.inactive_coils` (Task 2), `ConicalSpec::PowerUser.inactive_coils` (Task 4); `springcore::{SavedDesign, DesignSpec, ...}`; the GUI load path `populate_from_spec` → `parse_and_solve`.

- [ ] **Step 1: Read the two current example files** (`examples/compression_music_wire.toml`, `examples/conical_chrome_silicon.toml`) to capture their exact current fields (material, unit_system, end_type, wire/mean dia, active, free_length, loads). Record the end type of each so you know its default inactive count (`end_coils()`): Plain 0 / PlainGround 1 / Squared|SquaredGround 2.

- [ ] **Step 2: Choose the dead-coil counts.** For each file, set `inactive_coils` to `end_type default + 2` (two extra dead coils — clearly visible in the render without dominating the spring). Confirm arithmetically that `free_length` still exceeds the new solid length: new solid = `d·(active + inactive)` (ground) or `d·(active + inactive + 1)` (non-ground); each extra dead coil adds one `d`. If headroom is tight, use `+1` instead. The verifier in Step 4 is the real check.

- [ ] **Step 3: Regenerate the files from `SavedDesign` structs.** Add a throwaway `#[cfg(test)]` module (e.g. in `springmaker/src/`) that constructs the two `SavedDesign` values with the chosen `inactive_coils: Some(...)` and calls `.save(path)` for each — do NOT hand-edit the TOML tags (the internally-tagged-enum traps: double `family`/`type`, ordering). Reuse the exact other field values read in Step 1 so only `inactive_coils` changes. Run it: `cargo test -p springmaker <generator_test_name> -- --ignored` (mark it `#[ignore]` so it only runs on demand).

- [ ] **Step 4: Verify via the real load+solve path — zero Warning-severity status.** In the same throwaway module, for each regenerated file: load it (`SavedDesign::load` or the app's load path), `populate_from_spec(&mut form, &spec, us)`, then `parse_and_solve(...)`, and assert the resulting status has NO `Severity::Warning` messages (compression → `FormOutcome.status`; conical → `springcore::conical::evaluate_status`). Assert `total_coils == active + inactive` for each. Run it and confirm PASS. (This is the same verification PR #71 used.)

- [ ] **Step 5: Remove the throwaway generator/verifier module.** Delete the `#[cfg(test)]` generator module; the two regenerated `examples/*.toml` files persist. Confirm the workspace still builds/tests: `cargo test --workspace`.

- [ ] **Step 6: Confirm the untouched files.** Verify `git diff --stat` shows ONLY `examples/compression_music_wire.toml` and `examples/conical_chrome_silicon.toml` changed under `examples/` (extension/torsion/assembly example files unchanged), plus the deleted generator.

- [ ] **Step 7: Commit.**

```bash
git add examples/compression_music_wire.toml examples/conical_chrome_silicon.toml
git commit -m "chore(examples): showcase dead coils in compression + conical demos

Set a non-default inactive_coils (end-type default + extra dead coils) on the
compression and conical demo springs, regenerated from SavedDesign structs and
verified warning-free through the real load+solve path. Extension/torsion demos
unchanged (close-wound; out of scope).

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Post-implementation (SDD skill, not a task)

- Whole-branch adversarial panel (min 3: general / architect / simplifier) + specialists (cross-family/stateful-UI parity reviewer per the memory; input-domain adversary; wire-format/persistence reviewer for the serde additions). Verify: rate/stress/frequency invariance across families; the `free(p=d)==solid` invariant; both light/dark palettes render the new field; no machine-dependent snapshots.
- Converge R1→Rn; fix-forward; then `REVIEW_CONVERGED_OK` marker in its own Bash call; then push + PR (lowercase-subject title per `pr-title.yml`).

## Notes for the implementer

- **`design::solve_forward` vs `conical::solve_forward` are different functions with the same name.** `design::solve_forward` takes an explicit `inactive: f64` argument (Task 1); `conical::solve_forward` takes `&ConicalInputs` and resolves `inputs.inactive_coils` internally (Task 4) — its public signature is unchanged, so `springmaker/src/conical/form.rs:104` is not edited.
- **Adding a field to a struct breaks every literal constructor.** Tasks 1–5 each list the known in-file test-constructor sites, but rely on the compiler to enumerate any missed one — `inactive_coils: None` (or `inactive: String::new()` for form structs) is always the byte-identical default.
- **Count formatting:** `format!("{v}")` on an integer-valued `f64` prints without a decimal (`3.0 → "3"`, `1.5 → "1.5"`), matching the existing `active` display idiom. Confirm against the module's formatter and adjust expected strings if a dedicated count formatter exists.
