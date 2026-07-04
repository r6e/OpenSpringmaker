# Torsion Spring Family GUI — Design

**Status:** Approved

## Goal

Bring the torsion spring family to the springmaker GUI, so a user can enter a
torsion design, solve it, view the results, and save/load it — reaching the same
feature bar as the compression and extension families. The torsion engine
(`springcore/src/torsion/`) already solves; this wires it into the GUI.

## Context

The torsion engine exposes a **single** scenario — `PowerUser` (geometry given →
performance computed) via `torsion::design::solve_forward`. Unlike compression
and extension (which have 5 input modes each: PowerUser / RateBased / Dimensional
/ TwoLoad / MinWeight), torsion has no optimization or alternate-input modes. So
this is a single-scenario family, closest in shape to how compression/extension
first shipped their PowerUser mode.

Torsion introduces domain quantities the GUI has not handled before: **moment**
(torque) loading instead of force, **angular** deflection and spring rate instead
of linear, straight **legs**, an optional **arbor** (mandrel) diameter, and a
**friction-model** choice that changes the rate formula. No torsion persistence
variant exists yet.

## Decisions (settled during brainstorming)

1. **Scope:** full parity including save/load — adds an additive, mutation-gated
   `DesignSpec::Torsion` variant in springcore.
2. **Friction model:** exposed as a **torsion-form selector** (pick-list, default
   `ShigleyFriction`), co-located with the family and persisted in the saved spec
   — mirroring how the Wahl/Bergsträsser curvature choice is surfaced.
3. **Angular units:** angular deflection and rate shown as **degrees primary with
   revolutions secondary**.
4. **Structure:** **YAGNI-flat** — no scenario-picker enum or widget (one mode); a
   single PowerUser form. The scenario scaffold is added later if torsion ever
   gains optimization modes.

## Non-goals

- No scenario picker / optimization modes (RateBased/MinWeight/etc.) for torsion.
- No fatigue analysis, end-type/fixity inputs, or close-wound modeling (the
  engine does not model these for torsion).
- No changes to compression or extension behavior. The one-way module boundary
  holds: `torsion/` never imports `compression/` or `extension/`.
- No new engine physics — the torsion formulas already exist and cite
  EN 13906-3 / Shigley in `springcore/src/torsion/mechanics.rs`.

## Architecture — two layers

### Layer 1 — springcore additions (additive, data/units only, mutation-gated to 0 survivors)

1. **`Family::Torsion`** — new variant on `springcore::Family`. Extends the
   family enum every exhaustive match sees.
2. **`DesignSpec::Torsion(TorsionSpec)`** — new persistence variant. `TorsionSpec`
   is a **struct** (not a scenario enum), since torsion is single-scenario. SI
   canonical:

   ```rust
   pub struct TorsionSpec {
       pub wire_dia_mm: f64,
       pub mean_dia_mm: f64,
       pub body_coils: f64,
       pub leg1_mm: f64,
       pub leg2_mm: f64,
       pub arbor_dia_mm: Option<f64>,   // only optional field; missing key -> None
       pub friction_model: FrictionModel,
       pub moments_nmm: Vec<f64>,        // each > 0
   }
   ```
3. **`FrictionModel` gains `Serialize`/`Deserialize`** (serde derive, tagged as a
   lowercase string, e.g. `"pure_bending"` / `"shigley_friction"`) so it persists
   in `TorsionSpec`.
4. **Units US support** (additive on `springcore/src/units.rs`): the module
   *already* provides `Moment::from_pound_force_inches`/`pound_force_inches`,
   `Angle::degrees`/`turns`, and `AngularRate::newton_meters_per_degree`/`_per_turn`
   (all unit types already derive `Serialize`/`Deserialize`). The **only** gap is
   US angular-rate read accessors: add `AngularRate::pound_force_inches_per_degree(self)`
   and `pound_force_inches_per_turn(self)` (the rate is engine *output*, so read
   accessors suffice), using the same NIST factor as the existing `Moment` US
   methods (`1 lbf·in = 4.4482216152605 N × 0.0254 m`).
5. **`reject_non_finite`** (the pre-deserialization guard in
   `springcore/src/persistence.rs`) extended to cover the `Torsion` variant —
   every `f64`, including inside `moments_nmm` and the `arbor_dia_mm` `Option`.

### Layer 2 — springmaker torsion family (NOT mutation-gated)

New `springmaker/src/torsion/`, mirroring `extension/`'s 4-file presenter/humble
split (ADR 0008):

| File | Responsibility | ADR 0008 |
|---|---|---|
| `mod.rs` | module declarations + overview docstring | metadata |
| `form.rs` | pure logic: `TorFormState`, `Field` enum, `parse_and_solve`, `build_spec`, `populate_from_spec`, `is_blank` | pure |
| `view_model.rs` | presenter: `tor_inputs_view`, `tor_results_view`, `tor_status_view` (no iced) | pure |
| `view.rs` | humble iced view: `design_panel`, `results_panel`, `tor_field_value`, `tor_field_id`, friction pick-list wiring | humble view |

Shared infrastructure reused unchanged: `form_helpers`, `presenter`, `widgets`.
No scenario-picker widget (single mode).

**Dispatch seams to extend** (each a new match arm, not a rewrite):

- `app.rs` state: `pub torsion: TorFormState`, `pub tor_outcome: Option<TorFormOutcome>`.
- `app.rs` `Message`: `TorField(torsion::form::Field, String)`,
  `TorFriction(springcore::FrictionModel)`.
- `app.rs` `update()`: route `TorField` → `set_tor_field()` + recompute;
  `TorFriction` → set `self.torsion.friction_model` + recompute.
- `app.rs` `set_tor_field()`: new setter, match on `torsion::form::Field`.
- `app.rs` `recompute()`: `Family::Torsion` → `torsion::form::parse_and_solve(...)`
  → set `self.tor_outcome`.
- `app.rs` `apply_saved()`: `DesignSpec::Torsion(spec)` → `family = Torsion` +
  `torsion::form::populate_from_spec(...)`.
- `calculator.rs` `view()`: `Family::Torsion` → torsion design + results panels.
- `calculator.rs` `status_panel()`: `Family::Torsion` → `tor_status_view(app)`.
- The family selector (`Message::SelectFamily`) lists three families by iterating
  `Family` variants; no per-family code needed beyond the enum variant.

## Inputs

Single PowerUser form — `TorFormState` with a `Field` enum of seven text fields
plus the friction selector:

| Field | Unit (metric / US) | Boundary helper | Notes |
|---|---|---|---|
| `wire_dia` | mm / in | `length_mm` | round wire `d` |
| `mean_dia` | mm / in | `length_mm` | coil centre-line `D` |
| `body_coils` | count (dimensionless) | `positive_num` | active body coils `N_b` |
| `leg1` | mm / in | `non_negative_length_mm` | first straight leg `L₁`; accepts 0 |
| `leg2` | mm / in | `non_negative_length_mm` | second straight leg `L₂`; accepts 0 |
| `arbor_dia` | mm / in | `length_mm` | **optional** (valid-empty); wind-up clearance check |
| `moments` | N·mm / lbf·in | new `moments_nmm` | comma-separated load table; each > 0; empty rejected at form boundary ("provide at least one applied moment") |
| `friction_model` | — | — (pick-list) | `PureBending` \| `ShigleyFriction` (default Shigley); own `Message`; persisted |

New form-boundary helpers in `form_helpers.rs`:

- `moment_nmm(s: &str, us: UnitSystem) -> Result<f64>` — parse one moment; **> 0**;
  US lbf·in → N·mm via `Moment`; metric N·mm passthrough; post-conversion finite
  check (same shape as `positive_force_n`).
- `moments_nmm(s: &str, us) -> Result<Vec<f64>>` — comma-separated `moment_nmm`.
- `fmt_moment(nmm: f64, us) -> String` — N·mm / lbf·in.

There are no index-bound, clash-allowance, or end-type inputs (torsion has none).

## Solve & results

`parse_and_solve(form: &TorFormState, material: &Material, us: UnitSystem) ->
Result<TorFormOutcome>` parses the fields → builds `TorsionInputs` + the moments
vector (as `Moment`) → calls `torsion::design::solve_forward(material, inputs,
&moments, form.friction_model)` → wraps the `TorsionDesign` (or the error) in
`TorFormOutcome`.

Results panel (`tor_results_view` presenter → humble `results_panel`):

- **Summary rows:** spring index `C`, active coils `Nₐ`, angular rate `k′` as
  **moment per degree (primary) + moment per revolution (secondary)** —
  N·mm/° · N·mm/rev metric, lbf·in/° · lbf·in/rev US.
- **Per applied moment** (one section/row per load point): applied moment `M`;
  angular deflection `θ` as **degrees (primary) + revolutions (secondary)**;
  inner-fiber bending stress `σᵢ` with **% of allowable** (the governing check);
  wound inner diameter (governs arbor clearance / over-wind) under load.
- **Advisories** (`tor_status_view`, from `DesignStatus`, non-fatal): overstress,
  arbor binding, over-wind collapse, spring index outside [4, 12].

## Persistence

`build_spec(form, us) -> Result<TorsionSpec>` (display → SI) and
`populate_from_spec(form, spec, us)` (SI → display) round-trip: `build_spec`
followed by `populate_from_spec` recovers the original form state. `arbor_dia`
empty ↔ `arbor_dia_mm: None`. `friction_model` persists directly.

TOML is family-tagged like the others (`DesignSpec` `#[serde(tag = "family")]`).
`reject_non_finite` guards the new variant before deserialization. A round-trip
test covers both `arbor_dia` states and both friction variants.

## Error handling — defense in depth

**Form boundary** (field-named errors, before the engine): `length_mm` /
`positive_num` / `moments_nmm` reject non-finite, non-positive, and out-of-domain
inputs with messages naming the field. `arbor_dia` empty → `None`; non-empty →
positive length. **Engine backstop:** `solve_forward` re-validates everything
(wire/mean/coils/legs/arbor/moments finiteness and sign, index ≤ 1, effective
coils finite) — the GUI catches most with friendlier messages; the engine is the
safety net. `format_error` renders any `SpringError` with lengths in the active
unit system.

`is_blank(form) -> bool` returns true only for a fully-untouched form: all seven
text fields empty. `arbor_dia` and `moments` **count when typed** (typing signals
intent — consistent with `max_outer_dia` / `loads` in the other families).
`friction_model` is excluded from the blank check (it always holds its default,
so it cannot distinguish a blank form). There are no pre-filled default text
fields to exclude (torsion has no index bounds or clash allowance). `is_blank`
drives the Empty-state suppression in `App::recompute` (an untouched form shows
no parse error).

## Units presentation

- **Moment:** metric N·mm, US lbf·in. Persistence canonical: `moments_nmm`
  (N·mm). Engine `Moment` built via `from_newton_millimeters`.
- **Angular deflection:** degrees primary (`Angle::degrees`) + revolutions
  secondary (`Angle::turns`). Unit-system-independent (degrees/rev are the same
  metric or US).
- **Angular rate:** moment per degree (primary) + moment per revolution
  (secondary), matching the deflection treatment. Metric N·mm/° · N·mm/rev (via
  `AngularRate::newton_meters_per_degree` / `newton_meters_per_turn` × 1000); US
  lbf·in/° · lbf·in/rev (via the new `AngularRate::pound_force_inches_per_degree` /
  `pound_force_inches_per_turn`).

## Testing & gates

**springcore (mutation-gated, literal 0 survivors via `cargo mutants --in-diff`):**

- `TorsionSpec` round-trip through `to_toml`/`from_toml`, covering both
  `arbor_dia` states (`None` / `Some`) and both friction variants.
- Non-finite rejection: `reject_non_finite` rejects a NaN/Inf in each `f64` of the
  `Torsion` variant, including inside `moments_nmm` and `arbor_dia_mm`.
- Unit tests for the new US conversions: `Moment` lbf·in round-trip; a known
  `AngularRate` read as lbf·in/° and lbf·in/rev against hand-computed values;
  `Angle::turns` (a known angle → revolutions).

**springmaker (NOT mutation-gated):**

- Presenter unit tests: `tor_results_view` (field values; degree + revolution
  formatting; moment and rate formatting in metric AND US); `tor_status_view`
  (each advisory surfaces); `tor_inputs_view` (field descriptors + unit-aware
  labels).
- `build_spec` → `populate_from_spec` round-trip (metric + US).
- `is_blank` invariant: untouched form is blank; typing any field (including
  `arbor_dia`) clears it; changing only `friction_model` does not.
- `parse_and_solve`: happy path + each field's error path (non-numeric, ≤ 0
  where required), moment ≤ 0, and `mean_dia ≤ wire_dia` (index ≤ 1).
- A headless **Simulator E2E** that enters a torsion design, solves it, switches
  between families, and saves/loads it (round-trip through `DesignSpec::Torsion`).

**Gates:** local CI-parity before push — `fmt --check`, `clippy -D warnings`,
`doc -D warnings`, `typos`, `cargo test --workspace`, and the springcore in-diff
mutation gate. Mandatory adversarial review panel: the three-reviewer floor
(general-code, architect, simplifier) **plus** a persistence/wire-format reviewer
(the new spec variant + serde `FrictionModel` + `reject_non_finite`) and the
input-domain adversary — cycled to convergence before push.

## Review plan

Split the work so each task is independently testable:

1. springcore units US additions (`Moment` / `AngularRate` / `Angle::turns`) + tests.
2. springcore persistence: `Family::Torsion`, `DesignSpec::Torsion(TorsionSpec)`,
   `FrictionModel` serde, `reject_non_finite`, round-trip + non-finite tests.
3. springmaker torsion `form.rs` (state, `Field`, `parse_and_solve`, `build_spec`,
   `populate_from_spec`, `is_blank`) + form-boundary helpers + tests.
4. springmaker torsion `view_model.rs` presenter + tests.
5. springmaker torsion `view.rs` humble view + `app.rs`/`calculator.rs` dispatch.
6. Simulator E2E + final whole-branch review.

## Confirmations

- **Moment unit default:** N·mm (metric) / lbf·in (US) — the natural spring-torque
  units, following the existing metric/US toggle. Flagged for confirmation at spec
  review.
