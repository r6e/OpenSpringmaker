# Torsion GUI Fatigue Section — Design

**Status:** Approved
**Scope:** springmaker torsion family + one ADDITIVE springcore change (CycleLife
Display + `ALL_CYCLE_LIVES`; NO serde — nothing fatigue-related persists). The FINAL
torsion increment: after this, the family is complete against the original design.
Template: compression's fatigue GUI, mirrored at every layer.

## Decisions (settled during brainstorming)

1. **Compression parity at every layer**: optional-empty fatigue inputs; the
   three-state status (`Skipped`/`NoData`/`Computed`); the presenter view enum with
   the two muted notes; fatigue computed from the SOLVED design's geometry;
   **nothing persisted** (fields ephemeral; `populate_from_spec` clears them).
2. **All five scenarios** get the fatigue inputs uniformly — using
   `design.inputs.wire_dia/mean_dia` post-solve makes derived-geometry scenarios
   (RateBased, Dimensional, TwoLoad, MinWeight) work identically to PowerUser.
3. **Hidden under MinWeight**: compression suppresses the fatigue section when a
   min-weight result occupies the panel; torsion mirrors (the MinWeight scenario
   shows its Optimization section; the fatigue inputs still parse — status is
   computed — but the section yields to the optimizer readout).
4. **CycleLife stays unpersisted**, so the springcore surface is Display + the ALL
   const only (the DiaPolicy serde half is NOT needed — compression persists no
   fatigue state and neither do we).

## Non-goals

- No persistence changes of any kind (no `TorsionSpec` fields, no CycleLife serde) —
  the first torsion increment with zero format surface.
- No compression/extension changes. The recorded compression follow-ups (missing
  geometry + derived-finiteness guards in its `analyze_fatigue`) stay recorded — not
  this branch.
- No materials-editor changes.

## A. springcore (additive; mutation-gated)

`springcore/src/torsion/fatigue.rs`:

```rust
impl std::fmt::Display for CycleLife {
    // "10⁵ cycles" | "10⁶ cycles"
}

/// All `CycleLife` variants in display order (pick-list source).
pub const ALL_CYCLE_LIVES: &[CycleLife] = &[CycleLife::HundredThousand, CycleLife::Million];
```

Re-export `ALL_CYCLE_LIVES` from `torsion/mod.rs`. (`CycleLife` keeps
`#[non_exhaustive]` and its `Million` default; NO serde derives.)

## B. Form (`springmaker/src/torsion/form.rs`)

- `TorFormState` gains `pub fatigue_min: String, pub fatigue_max: String` (both
  `String::new()` in the manual Default) and `pub cycle_life: CycleLife`.
- `Field` gains `FatigueMin, FatigueMax`; ids `tor-fatigue-min`, `tor-fatigue-max`.
- New form-helper `non_negative_moment_nmm(field, value, us) -> Result<f64>`
  (mirrors `non_negative_force_n`: ≥ 0 allowed — fatigue-min may be 0, the exact
  R = 0 case; the existing `moment_nmm` requires > 0 and stays untouched).
- Three-state status + outcome field (compression's exact shape):

```rust
/// Three-state fatigue result distinguishing "not attempted" from "no data".
#[derive(Debug, Clone)]
pub(crate) enum TorFatigueStatus {
    /// User left min/max cycle moments blank; fatigue was not attempted.
    Skipped,
    /// Cycle moments supplied but the material has no bending-fatigue data.
    NoData,
    /// Fatigue analysis succeeded.
    Computed(TorFatigueResult),
}

pub struct TorFormOutcome {
    pub design: TorsionDesign,
    pub(crate) min_weight: Option<TorMinWeightExtra>,
    pub(crate) fatigue: TorFatigueStatus,
}
```

- `compute_tor_fatigue(form, material, design: &TorsionDesign, us) ->
  Result<TorFatigueStatus>` mirroring compression's `compute_fatigue`: both fields
  blank-trimmed → `Skipped`; parse via `non_negative_moment_nmm("fatigue min"/"
  fatigue max", …)`; call `analyze_torsion_fatigue(material,
  design.inputs.wire_dia, design.inputs.mean_dia, m_min, m_max, form.cycle_life)`;
  `Err(NoFatigueData(_))` → `Ok(NoData)`; other errors propagate (surface via
  `format_error` like any parse/solve error). EVERY scenario arm calls it after its
  solve (the MinWeight arm too — the status is computed even though the section
  hides; one code path, no scenario special-casing).
- `is_blank`: `fatigue_min`/`fatigue_max` join ALL FIVE arms (typed-optional
  signals intent — torsion's established rule; compression's treatment verified at
  plan time and mirrored if it differs, with the divergence documented).
  `cycle_life` excluded (default-holding selector).
- `populate_from_spec`: every arm clears both fatigue fields and resets
  `cycle_life = CycleLife::default()` (the established stale-field rule — nothing
  is persisted, so a loaded design starts fatigue-clean).

## C. Presenter + view

`view_model.rs`:

```rust
/// Fatigue section state (compression's shape).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TorFatigueView {
    /// Suppressed: a min-weight result occupies the panel instead.
    Hidden,
    /// Fatigue analysis succeeded; readout rows.
    Computed(Vec<ResultRow>),
    /// A muted note (no data / not attempted).
    Note(&'static str),
}

const TOR_FATIGUE_NO_DATA: &str = "No fatigue data for this material.";
const TOR_FATIGUE_SKIPPED: &str = "Enter min and max cycle moments to compute fatigue.";
```

`tor_fatigue_view(out, us) -> TorFatigueView`: `Hidden` when `out.min_weight` is
`Some`; else map the status — `Computed` rows (unit-aware via `display_stress`):

| Label | Value | Unit |
|---|---|---|
| Alternating stress | `{:.2}` | stress unit |
| Mean stress | `{:.2}` | stress unit |
| Endurance (Se) | `{:.2}` | stress unit |
| Ultimate tensile (Sut) | `{:.2}` | stress unit |
| Strength amplitude (Sa) | `{:.2}` | stress unit |
| Gerber FOS | `{:.3}` | — |

`TorPopulatedResults` gains `pub fatigue: TorFatigueView`; `tor_results_view` fills
it. `tor_inputs_view`: every scenario arm appends `Fatigue min moment ({moment},
optional)` → `FatigueMin` and `Fatigue max moment ({moment}, optional)` →
`FatigueMax` (after the scenario's existing fields).

`view.rs`: the `Cycle life` pick-list (`ALL_CYCLE_LIVES`, `Message::TorCycleLife`)
joins the Setup group beside the friction selector (all scenarios — it gates only
the fatigue computation); the results panel renders the fatigue state after the
existing sections: `Computed` → `divided_result_section("Fatigue", rows)`;
`Note(s)` → the muted-note widget compression's fatigue section uses (mirror its
exact widget helper); `Hidden` → nothing.

## D. app.rs

`Message::TorCycleLife(springcore::torsion::CycleLife)` + update arm (set +
recompute `true`); `set_tor_field`/`tor_field_value`/`tor_field_id` arms for the
two fields.

## E. Testing & gates

- **springcore (mutation-gated 0 survivors):** Display strings exact + ALL const
  content (the FrictionModel-test pattern).
- **Form:** the golden THROUGH the form (US units, PowerUser with Example 10-8(c)'s
  geometry as direct inputs, fatigue 1→5 lbf·in, Million → `Computed` with
  `gerber_factor_of_safety ≈ 1.13` at 5e-3); Skipped when both fields are blank
  (the default) AND when exactly one is filled — compression's `||` check treats
  any blank side as not-attempted; mirror it and test both one-sided cases;
  NoData with Oil-Tempered (fields filled); a parse
  error (negative fatigue_min) propagates as Err; the life selector changes Se
  through the form (HundredThousand > Million endurance on Music Wire); is_blank:
  the two fields trip it in every scenario arm; populate clears both + resets the
  selector; the derived-geometry path (RateBased solves, fatigue computed on the
  DERIVED wire/mean — assert Computed).
- **Presenter:** view-state mapping (Hidden under MinWeight even with fields
  filled; Computed rows' labels/units; both notes); descriptor additions per
  scenario (count bumps by 2 everywhere).
- **E2E:** drive PowerUser + fatigue moments through widgets → the Fatigue rows
  render (`shows` a row label); the NoData note with Oil-Tempered; the MinWeight
  suppression (fatigue filled + MinWeight scenario → Optimization section present,
  Fatigue absent).
- **Gates:** local CI-parity + in-diff mutation (springcore = the Display/const
  only); final panel — floor 3 + MANDATORY input-domain adversary (the
  three-state × scenario × selector matrix); **NO persistence reviewer** (zero
  format surface — a first for this family, stated in the panel brief).

## Task shape (for the plan)

1. springcore Display/ALL (+tests, mutation gate) + form layer (fields, helper,
   status, compute, outcome field with ALL construction sites updated, is_blank,
   populate clears) + form tests.
2. Presenter/view/app (view enum, rows, notes, pick-list, gating, ids) + presenter
   tests + E2E + full gate.
