# Torsion GUI MinWeight Mode — Design

**Status:** Approved
**Scope:** springmaker torsion family + two ADDITIVE springcore changes (DiaPolicy
serde/Display/ALL const; the `TorsionSpec::MinWeight` persistence variant). Brings
`springcore::torsion::solve_min_weight` (PR #46) to the GUI as the fifth torsion
scenario — the extension-1c MinWeight-mode precedent. After this, torsion fatigue is
the last phase-1 deferred item.

## Decisions (settled during brainstorming)

1. **Pure parity fan-out** — extension's MinWeight GUI is the template at every layer
   (form fields incl. pre-filled index bounds, plain-`Option` outcome extra, the two
   result rows, the additive persistence variant, the non_exhaustive wildcard arm).
2. **The moment-entry (F@r) selector is HIDDEN for MinWeight** (like TwoLoad): the
   optimizer takes ONE `max_moment`, not the moments list the toggle feeds.
3. **Index bounds pre-fill "4"/"12"** (extension's exact defaults and torsion's
   engine caution range); `is_blank`'s MinWeight arm counts every displayed input
   EXCEPT the pre-filled bounds — torsion's first pre-filled fields, adopting
   extension's documented rule.

## Non-goals

- No torsion fatigue. No changes to the other four torsion scenarios' behavior.
- No compression/extension changes (the extension binding wildcard-arm note from
  PR #46's review stays a recorded follow-up, untouched here).
- No optimizer engine changes beyond the additive DiaPolicy derives.

## A. springcore: DiaPolicy GUI surface (additive; mutation-gated)

`springcore/src/torsion/optimize.rs` — the `FrictionModel` treatment:

```rust
#[non_exhaustive] // unchanged
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum DiaPolicy { #[default] MaxMargin, Compact }

impl std::fmt::Display for DiaPolicy {
    // "Max Margin" | "Compact"
}

/// All `DiaPolicy` variants in display order (pick-list source).
pub const ALL_DIA_POLICIES: &[DiaPolicy] = &[DiaPolicy::MaxMargin, DiaPolicy::Compact];
```

Re-export `ALL_DIA_POLICIES` from `torsion/mod.rs`. Serde strings are the PascalCase
variant names (`"MaxMargin"`/`"Compact"`), consistent with every persisted enum.
`TorBindingConstraint` is untouched (stringified presenter-side, never persisted).

## B. Scenario + form (springmaker/src/torsion/form.rs)

- `TorScenarioKind` gains `MinWeight` (Display "Min Weight"), fifth in
  `ALL_TOR_SCENARIOS`.
- `TorFormState` gains `pub max_moment: String, pub index_min: String, pub
  index_max: String, pub max_outer_dia: String, pub candidate_diameters: String,
  pub dia_policy: DiaPolicy`. The `#[derive(Default)]` becomes a manual `impl
  Default` pre-filling `index_min: "4".into(), index_max: "12".into()` (everything
  else as today) — extension's exact shape.
- `Field` gains `MaxMoment, IndexMin, IndexMax, MaxOuterDia, CandidateDiameters`;
  ids `tor-max-moment`, `tor-index-min`, `tor-index-max`, `tor-max-outer-dia`,
  `tor-candidate-diameters` via `tor_field_id`.
- Private `parse_candidate_diameters_mm(form, us) -> Result<Vec<f64>>` mirroring
  extension's (split ',', trim, skip empties, `length_mm("candidate diameter", …)`
  each, non-empty guard "provide at least one candidate diameter").
- **parse_and_solve MinWeight arm** builds `TorMinWeightRequest`:
  - `required_rate`: the existing `ang_rate_nmm_per_deg("rate", …)` → `/1000` →
    `AngularRate::from_newton_meters_per_degree` chain (RateBased's exact path);
  - `max_moment`: `moment_nmm("max moment", …)` → `Moment::from_newton_millimeters`;
  - `leg1/leg2`: `non_negative_length_mm`; `arbor_dia`: `parse_arbor`;
  - `friction_model`, `dia_policy` from the selectors;
  - `index_bounds`: `(num("index min", …)?, num("index max", …)?)` — plain finite
    parses; the ENGINE's `1 < c_min < c_max` guard is the validator (its message
    names the bounds; no duplicated form guard);
  - `max_outer_dia`: empty → None, else `length_mm`;
  - `candidate_diameters`: the shared parser, mapped to `Length::from_millimeters`.
  Calls `solve_min_weight(material, &req)`; `Infeasible`/`InconsistentInputs`
  surface via `format_error` unchanged.
- **Outcome**: `TorFormOutcome` gains `pub min_weight: Option<TorMinWeightExtra>`
  with `pub(crate) struct TorMinWeightExtra { pub binding: TorBindingConstraint,
  pub mass_kg: f64 }` (extension's plain-Option rationale: no fatigue section yet,
  an enum would be speculative). The four existing scenario arms set `min_weight:
  None`; the MinWeight arm fills it from the solution.
- **build_spec / populate_from_spec**: the `TorsionSpec::MinWeight` variant (section
  D) round-trips every field incl. `dia_policy` and the candidate list (candidates
  formatted as a comma-join of `fmt_len` per entry — extension's exact formatter
  shape for its candidate list);
  populate sets `scenario = MinWeight` and both selectors. All arms keep the
  established F@r resets.
- **is_blank MinWeight arm**: `all_empty(&[rate, max_moment, leg1, leg2, arbor_dia,
  max_outer_dia, candidate_diameters])` — index bounds EXCLUDED (pre-filled; comment
  carries extension's rationale). Selectors excluded as always. The cross-scenario
  invariant tests extend to the new arm and the prefill rule.

## C. Presenter + view

- `tor_inputs_view` MinWeight arm (unit-aware labels): Rate ({moment}/°) — reused
  descriptor; `Max moment ({moment})`; `Leg 1/Leg 2 ({len})`; `Arbor diameter
  ({len}, optional)`; `Index min` / `Index max` (unitless); `Max outer diameter
  ({len}, optional)`; `Candidate diameters ({len}), comma-separated`. No moments /
  forces / body-coils descriptors.
- New presenter fn `tor_min_weight_rows(out) -> Option<Vec<ResultRow>>` mirroring
  extension's: `None` unless `out.min_weight` is Some; rows `("Wire mass",
  format!("{:.4}", mass_kg), "kg")` and `("Binding constraint", <match>, "")` with
  the match "bending stress" / "index" / "outer diameter" and the documented
  `_ => "other"` wildcard (`TorBindingConstraint` is `#[non_exhaustive]` and
  springmaker is downstream). `TorPopulatedResults` gains `pub min_weight:
  Option<Vec<ResultRow>>`; the view renders it as an "Optimization" `rows_section`
  between the rate section and Geometry when present. Everything else (rate rows,
  geometry, the load table with the single max-moment point) renders unchanged.
- The winning wire/mean diameters already surface through the geometry/load rows
  (index, wound ID) — extension precedent: no dedicated "chosen diameter" row; the
  design's inputs are visible via populate-on-save or the load table. (Deliberate:
  parity over invention.)
- View chrome: the `DiaPolicy` pick-list (`field_label("Diameter policy")`,
  `styled_pick_list(ALL_DIA_POLICIES, Some(app.torsion.dia_policy),
  Message::TorDiaPolicy)`) renders in the Setup group ONLY when `scenario ==
  MinWeight`; the moment-entry selector's gate becomes `scenario != TwoLoad &&
  scenario != MinWeight`.

## D. Persistence (springcore; additive — no format break)

`TorsionSpec` gains:

```rust
    MinWeight {
        rate_nmm_per_deg: f64,
        max_moment_nmm: f64,
        leg1_mm: f64,
        leg2_mm: f64,
        arbor_dia_mm: Option<f64>,
        friction_model: crate::torsion::FrictionModel,
        dia_policy: crate::torsion::DiaPolicy,
        index_min: f64,
        index_max: f64,
        max_outer_dia_mm: Option<f64>,
        candidate_diameters_mm: Vec<f64>,
    },
```

Existing tagged files are unaffected (additive variant). The two `Option`s follow
the documented missing-key→None rule; every other field is required (missing →
DataFile error). `reject_non_finite`'s tree-walk covers the new floats and the Vec.
`solve_with_material`'s `Torsion(_)` arm already matches. The `deny_unknown_fields`
guardrail comment is unchanged and still true.

## E. app.rs

`Message::TorDiaPolicy(springcore::torsion::DiaPolicy)` + update arm (set + recompute
`true`); `set_tor_field` arms for the five new fields. Scenario switching, family
clearing, and save/load flow through the existing machinery.

## F. Testing & gates

- **springcore (mutation-gated 0 survivors in-diff):** DiaPolicy serde round-trip +
  Display strings + ALL const; `TorsionSpec::MinWeight` round-trip (both Options in
  None/Some states, both policies, both friction models across cases), missing
  required field → DataFile, non-finite rejection (a candidate list containing inf;
  a non-finite bound).
- **springmaker form:** MinWeight oracle through the REAL optimizer (golden
  geometry: rate ≈8.875 N·mm/°, max moment 100 N·mm, candidates "1.5, 2, 2.5" →
  smallest feasible d wins; `min_weight` Some with the closed-form mass;
  `load_points.len() == 1`; the fixture sets `FrictionModel::PureBending`
  EXPLICITLY — the recorded oracle rule: the form default is ShigleyFriction, whose
  denominator changes the mass); Infeasible surfaces through `format_error` (huge max
  moment); empty candidates → the form guard message; both DiaPolicy values solve
  (mass equal, D differs — the engine's policy-independence, observed through the
  form); build↔populate round-trips metric+US (candidates list, bounds, both
  selectors); is_blank: fresh MinWeight form blank (pre-filled bounds excluded),
  each displayed field trips it, editing ONLY a pre-filled bound does not
  (extension's rule, test-pinned); other scenarios still set `min_weight: None`.
- **presenter:** `tor_min_weight_rows` mapping incl. the wildcard-arm doc; inputs
  descriptor list per unit system; the Optimization section only for MinWeight.
- **E2E (Simulator):** drive MinWeight through real widgets → Populated with the
  "Wire mass" row; save/load round-trip restoring scenario + candidates + policy.
- **Gates:** local CI-parity set; springcore in-diff mutation → literal 0 survivors;
  final adversarial panel — floor 3 + MANDATORY input-domain adversary +
  persistence/wire-format reviewer (new persisted variant) — cycled to convergence.

## Task shape (for the plan)

1. springcore: DiaPolicy derives/Display/ALL + `TorsionSpec::MinWeight` variant +
   tests (mutation-gated) + the springmaker compile-keeper if any exhaustive match
   on `TorsionSpec` needs a new arm (populate_from_spec does — transitional arm per
   the established pattern).
2. springmaker: scenario variant + form fields/parsers + MinWeight solve arm +
   outcome extra + build/populate arms + is_blank (+ invariant updates).
3. Presenter + view + app wiring (rows, descriptors, DiaPolicy pick-list, selector
   gating) + E2E + full gate.
