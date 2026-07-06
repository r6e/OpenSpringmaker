# Cross-Family Hardening — Design

**Status:** Approved
**Scope:** Clear the panel-recorded follow-up backlog accumulated across the
extension/torsion increments: compression fatigue guards (springcore,
mutation-gated), extension enum loudness + ADR 0013, a scientific-notation
display fallback for result rows, and the form_helpers extraction advisory.
Zero persistence surface (the only doc artifact is an ADR). One branch, one PR.

## Decisions (settled during brainstorming)

1. **Scientific fallback** (user decision): result-row values at or above 1e6
   display units render `{:.3e}`; below, they render fixed-point exactly as
   today. One shared presenter helper, swept across all three families.
2. **Loud compile-fail** (user decision): `ExtBindingConstraint` loses
   `#[non_exhaustive]` and the GUI wildcard arm dies — a new variant becomes a
   compile error, matching the compression `BindingConstraint` / `FrictionModel`
   precedent (PR #32). The convention is recorded as ADR 0013.
3. **Guard parity, not behavior parity**: compression's `analyze_fatigue`
   gains torsion's guard structure but KEEPS tolerating equal cycle forces —
   Goodman's reciprocal form is defined at τa = 0, unlike torsion's Gerber
   `nf = Sa/σa`. This is the documented divergence recorded on PR #54.
4. **Verified-current scope**: the recorded `.meters()` Dimensional drift is
   already fixed (all three families use `Length::from_millimeters(length_mm(…))`)
   and is NOT in scope. The equal-moments torsion divergence is NOT in scope
   (spec-mandated parity, physics-correct engine behavior).

## Non-goals

- No persistence changes of any kind.
- No small-value display handling (`0.00` for tiny nonzero values) — this
  fixes the recorded large-side exposure only.
- No form-level magnitude limits (rejected in brainstorming: arbitrary caps).
- No `HookSpec` change — it is constructed, never matched, so its
  `#[non_exhaustive]` is harmless and consistent with ADR 0013's rule.
- No torsion/extension engine changes beyond the shared-guard promotion.

## A. Compression fatigue guards (`springcore` — mutation-gated)

### A1. Promote the shared geometry guard

Move `validate_wire_mean_geometry` from `springcore/src/torsion/design.rs:73`
(pub(crate)) to `springcore/src/design.rs` (pub(crate)), messages VERBATIM.
Torsion imports it from the new location; its call sites and tests are
otherwise untouched. (DRY on second occurrence: compression's fatigue is the
second consumer.)

### A2. `analyze_fatigue` validation order (torsion's precedence, each message pinned)

Current state (`springcore/src/fatigue.rs:24-76`): ordering guard →
`NoFatigueData` → Ssm ≥ Ssu trap → compute → `Ok` with UNGUARDED outputs.
`spring_index(mean_dia, wire_dia)` divides by an unguarded wire diameter —
wire = 0 silently yields NaN stresses today.

New order:

1. `validate_wire_mean_geometry(wire_dia, mean_dia)` — geometry first
   (torsion's precedence rule; solve_forward's exact messages).
2. `NoFatigueData` check (existing, moves after geometry).
3. NEW input guard — both forces finite and ≥ 0:
   `"cycle forces must be finite and non-negative (the endurance data covers
   unidirectional compressive loads)"` (torsion's message shape, adapted;
   Zimmerli data per Shigley §10-9 is unidirectional).
4. Existing ordering guard (`max ≥ min`), message unchanged. NO equal-forces
   rejection (Decision 3).
5. Existing Ssm ≥ Ssu trap, message unchanged.
6. NEW output guard before `Ok` — torsion's 5-element finiteness check
   (`springcore/src/torsion/fatigue.rs:135-144`) transplanted:

```rust
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
```

Verified: no springmaker test asserts on compression fatigue error strings,
so no downstream breakage. The GUI's `compute_fatigue` already feeds
`non_negative_force_n`-parsed values; the engine guards are defense in depth.

## B. Extension enum loudness + ADR 0013

- `springcore/src/extension/optimize.rs:62`: remove `#[non_exhaustive]` from
  `ExtBindingConstraint`.
- `springmaker/src/extension/view_model.rs:115-123`: delete the
  `_ => "other"` arm and its comment — the match becomes exhaustive.
  MUST land in the same commit as the attribute removal (an exhaustive enum
  makes the wildcard an `unreachable_patterns` warning → `-D warnings` fails).
- `docs/adr/0013-public-enum-exhaustiveness-policy.md`: enums a downstream
  layer must exhaustively `match` (binding constraints, `FrictionModel`) carry
  NO `#[non_exhaustive]` — a new variant is a loud compile error at every
  match site. Display/iterate-only enums (`CycleLife`, `DiaPolicy`,
  `HookSpec`) keep `#[non_exhaustive]` + `ALL_*` consts. Context: the silent
  `_ => "other"` arm found in the extension GUI; alternatives considered
  (keep attribute + visible "unknown" label) and why loud won (PR #32
  precedent; springcore is workspace-internal and unpublished, so the
  technically-semver-major removal is free — noted in the ADR).

## C. Scientific display fallback (`springmaker` presenter)

In `springmaker/src/presenter.rs`:

```rust
/// Result-row values at/above this magnitude (in display units) render in
/// scientific notation; fixed-point below. Guards row layout against
/// huge-but-finite inputs that survive all engine finiteness checks.
pub(crate) const SCI_THRESHOLD: f64 = 1e6;

pub(crate) fn fmt_row_value(v: f64, decimals: usize) -> String {
    if v.abs() >= SCI_THRESHOLD {
        format!("{v:.3e}")
    } else {
        format!("{v:.decimals$}")
    }
}
```

Sweep: every numeric result-row `format!("{…:.N}")` site in
`compression/view_model.rs`, `extension/view_model.rs`,
`torsion/view_model.rs` (and any in `presenter.rs` itself) routes through
`fmt_row_value(value, N)`. Existing row-content tests stay green (all
fixtures sit far below the threshold).

## D. form_helpers extraction (`springmaker`)

Two private cores in `springmaker/src/form_helpers.rs`; every error message
preserved VERBATIM; the existing 15 tests in the file must stay green
UNTOUCHED (they pin the messages):

```rust
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

Wrappers collapse to 1–3 lines each: `length_mm`, `rate_npm`,
`ang_rate_nmm_per_deg`, `moment_nmm`, `positive_force_n` on the positive
core; `non_negative_length_mm`, `non_negative_force_n`,
`non_negative_moment_nmm` on the non-negative core. (Any helper whose shape
deviates on inspection — e.g. a different parse or message — stays unfactored
rather than force-fit.)

## E. Testing & gates

- **springcore (TDD, mutation-gated 0 in-diff survivors):** each new guard
  message pinned exactly; precedence tests (zero wire beats bad forces beats
  no-data — geometry first); equal-forces-still-Ok test (Decision 3, pins the
  divergence); huge-but-finite forces → the output guard's message; the moved
  `validate_wire_mean_geometry` covered via both torsion (existing tests keep
  passing) and the new compression callers.
- **Presenter:** boundary tests for `fmt_row_value` — just-below stays
  fixed-point (`999_999.99`), `1e6` and `1e300` go scientific, negative huge
  goes scientific, zero/normal values unchanged; one per-family row test with
  a huge finite stress asserting the scientific rendering reaches the row.
- **Form-level:** a compression fatigue test with a huge finite cycle force
  asserting the engine guard surfaces as a form error (mirrors torsion's).
- **Extension:** the existing binding-label presenter tests cover the
  de-wildcarded match; compilation IS the new-variant test.
- **form_helpers:** existing 15 tests green and untouched; no new behavior.
- **Gates:** local CI-parity (fmt, clippy `-D warnings`, doc `-D warnings`,
  bare typos, workspace tests) + in-diff mutation vs origin/main; final
  panel — floor 3 + MANDATORY input-domain adversary (guard-precedence ×
  sign × magnitude matrix across both engines and the presenter threshold);
  NO persistence reviewer (zero format surface — reason stated in briefs).

## Task shape (for the plan)

1. springcore: shared-guard promotion + compression fatigue guards (+tests,
   mutation gate) + extension enum loudness (attribute + wildcard arm, one
   commit) + ADR 0013.
2. springmaker: `fmt_row_value` + the three-family sweep + form_helpers
   extraction + presenter/form tests + full gate.
