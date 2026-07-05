# Torsion GUI Input Modes — Design

**Status:** Approved
**Scope:** springmaker torsion family + the springcore persistence migration it requires.
Brings the phase-2 engine scenarios (RateBased, Dimensional, TwoLoad — PR #44) and the
force-at-radius entry convenience to the GUI, fanning out the proven scenario-picker
pattern (extension spec-1c precedent). Torsion reaches full input-mode parity with its
engine.

## Decisions (settled during brainstorming)

1. **Clean-break persistence migration:** `TorsionSpec` becomes a `#[serde(tag = "type")]`
   enum. Old tag-less torsion files (written by the PR #43 single-scenario GUI) STOP
   loading, failing with a clear `DataFile` error — no legacy fallback. Chosen over a
   back-compat untagged fallback; pre-1.0, torsion save/load is days old. A test pins
   that the legacy flat layout errors rather than silently parsing as the wrong shape.
2. **Force-at-radius is an entry toggle, not persisted:** available in the three
   moments-list scenarios (PowerUser, RateBased, Dimensional); converts F@r → moments at
   the form boundary via the cited `moment_from_force_at_radius`; the spec stores derived
   `moments_nmm` only (unchanged shape per variant). Reloading shows Direct mode with the
   derived moments. TwoLoad is excluded (paired-point entry; the toggle doesn't fit).
3. **No cross-family generic scenario-form abstraction** — the per-family module pattern
   stands; extension's 1c threading is the template.

## Non-goals

- No torsion MinWeight (no engine optimizer yet), no fatigue.
- No back-compat for tag-less torsion files (decision 1). Compression/extension
  persistence untouched.
- No changes to compression/extension behavior; one-way module boundary holds.

## A. Scenario picker

`torsion/form.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TorScenarioKind {
    #[default]
    PowerUser,
    RateBased,
    Dimensional,
    TwoLoad,
}
pub const ALL_TOR_SCENARIOS: &[TorScenarioKind] = &[/* display order as declared */];
impl Display { "Power User" | "Rate Based" | "Dimensional" | "Two Load" }
```

`TorFormState` gains `pub scenario: TorScenarioKind`. `app.rs` gains
`Message::TorScenario(TorScenarioKind)` (sets `scenario`, recomputes). The design panel
renders the pick-list (`ALL_TOR_SCENARIOS`, `Message::TorScenario`) above the friction
selector — the same chrome as extension. Threaded through every match, kept exhaustive:
`is_blank` / `parse_and_solve` / `build_spec` / `populate_from_spec` / `tor_inputs_view`
/ `tor_field_id` / `tor_field_value` / `set_tor_field`.

## B. Per-scenario forms

Shared-field convention (extension precedent): new fields reused across scenarios.

| Scenario | Displayed inputs (beyond the shared friction selector) |
|---|---|
| PowerUser | wire_dia, mean_dia, body_coils, leg1, leg2, arbor_dia (optional), moment entry (see C) |
| RateBased | wire_dia, mean_dia, **rate**, leg1, leg2, arbor_dia (optional), moment entry |
| Dimensional | wire_dia, **outer_dia**, body_coils, leg1, leg2, arbor_dia (optional), moment entry |
| TwoLoad | wire_dia, mean_dia, leg1, leg2, arbor_dia (optional), **moment1, angle1, moment2, angle2** |

New `Field` variants: `Rate`, `OuterDia`, `Moment1`, `Angle1`, `Moment2`, `Angle2`,
`Forces`, `LoadRadius`. New stable widget ids (`tor-rate`, `tor-outer-dia`,
`tor-moment1`, `tor-angle1`, `tor-moment2`, `tor-angle2`, `tor-forces`,
`tor-load-radius`) via `tor_field_id` — the single id source the Simulator tests share.

**Units and helpers (form boundary — new in `form_helpers.rs`):**

- `ang_rate_nmm_per_deg(field, value, us) -> Result<f64>`: strictly positive; metric
  input N·mm/°, US input lbf·in/° (converted via
  `Moment::from_pound_force_inches(v).newton_millimeters()`); post-conversion finite
  check. Canonical form: N·mm/°. Engine value built as
  `AngularRate::from_newton_meters_per_degree(nmm_per_deg / 1000.0)` (existing
  constructor; no new units API).
- `angle_deg(field, value) -> Result<f64>`: any FINITE number (no sign constraint —
  TwoLoad is offset-tolerant; angles are degrees in both unit systems). Engine value:
  `Angle::from_degrees(v)`.
- `fmt_ang_rate_nmm_per_deg`, `fmt_angle_deg` formatters for `populate_from_spec`.
- Forces for F@r reuse `positive_force_n` per element (moments must be > 0 and radius
  > 0 ⇒ forces strictly positive); `load_radius` uses `length_mm`.

**Dimensional boundary guard (owed from phase 2):** a form-level
`dimensional_mean_check(wire_dia_mm, outer_dia_mm)` rejecting `outer ≤ wire` with
"outer diameter must be greater than wire diameter" — mirroring extension's
`dimensional_mean_mm` — applied in BOTH `parse_and_solve` and `build_spec` (never
persist a spec the engine would reject on index grounds the user can't see). The
engine's own OD guard + mean/index guards remain the backstop.

**parse_and_solve** builds the matching engine scenario
(`springcore::torsion::{PowerUser, RateBased, Dimensional, TwoLoad}`) and calls
`.solve(material, form.friction_model)`. TwoLoad points:
`(Moment::from_newton_millimeters(m1), Angle::from_degrees(a1))` etc., in input order.
Engine guards (degenerate points, positive slope, leg/na attribution, derived body
coils) surface through `format_error` as-is — the form adds no duplicate slope logic.

## C. Force-at-radius entry toggle

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MomentEntry {
    #[default]
    Direct,        // "Moments (N·mm)" — the existing comma list
    ForceAtRadius, // "Force @ radius" — forces comma list + one load radius
}
```

`TorFormState` gains `moment_entry: MomentEntry`, `forces: String`,
`load_radius: String`; `app.rs` gains `Message::TorMomentEntry(MomentEntry)`. The view
shows the selector for PowerUser/RateBased/Dimensional only; in `ForceAtRadius` mode
the moments input is replaced by the forces list + load radius fields. Parsing (shared
by `parse_and_solve` and `build_spec`): in Direct mode, `parse_moments_nmm_nonempty`
as today; in F@r mode, each force (strictly positive) × the radius →
`moment_from_force_at_radius(force, radius).newton_millimeters()`, with the same
non-empty-list guard ("provide at least one applied force"). The derived moments then
flow identically everywhere — `build_spec` persists `moments_nmm`; the toggle itself is
NOT persisted (decision 2), and `populate_from_spec` always sets `Direct` and clears
`forces`/`load_radius` (no stale-field leak, same rule as extension's hook radii).

## D. Persistence migration (springcore, mutation-gated to 0 survivors)

`TorsionSpec` (flat struct) is REPLACED by:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TorsionSpec {
    PowerUser {
        wire_dia_mm: f64, mean_dia_mm: f64, body_coils: f64,
        leg1_mm: f64, leg2_mm: f64, arbor_dia_mm: Option<f64>,
        friction_model: FrictionModel, moments_nmm: Vec<f64>,
    },
    RateBased {
        wire_dia_mm: f64, mean_dia_mm: f64, rate_nmm_per_deg: f64,
        leg1_mm: f64, leg2_mm: f64, arbor_dia_mm: Option<f64>,
        friction_model: FrictionModel, moments_nmm: Vec<f64>,
    },
    Dimensional {
        wire_dia_mm: f64, outer_dia_mm: f64, body_coils: f64,
        leg1_mm: f64, leg2_mm: f64, arbor_dia_mm: Option<f64>,
        friction_model: FrictionModel, moments_nmm: Vec<f64>,
    },
    TwoLoad {
        wire_dia_mm: f64, mean_dia_mm: f64,
        leg1_mm: f64, leg2_mm: f64, arbor_dia_mm: Option<f64>,
        friction_model: FrictionModel,
        moment1_nmm: f64, angle1_deg: f64, moment2_nmm: f64, angle2_deg: f64,
    },
}
```

- The PowerUser variant's field set is byte-compatible with the old flat struct EXCEPT
  the now-required `type = "PowerUser"` tag — which is the clean break: a tag-less file
  fails serde's tag resolution and surfaces as `SpringError::DataFile`. A test feeds the
  exact legacy flat TOML and asserts the error (never a silent wrong-shape parse).
- `rate_nmm_per_deg` follows the family's mm/N·mm storage flavor and the degree-primary
  UI (documented at the field; conversion exact via existing constructors).
- `arbor_dia_mm` stays the only `Option` per variant (same missing-key→None note).
- Guardrail comments updated: the struct→enum migration is now EXECUTED (comment
  records it and the clean-break decision); the `deny_unknown_fields` warning carries
  over to the enum (variants are still flattened under `DesignSpec`'s `family` tag —
  same landmine).
- `reject_non_finite` is the generic tree-walk — tests only (per-variant non-finite,
  including the TwoLoad angle fields, which may legitimately be NEGATIVE but never
  non-finite).
- `SavedDesign::solve_with_material`'s Torsion arm (returns `InconsistentInputs`)
  updates mechanically for the enum; its test stays.

## E. is_blank invariant

Per scenario, the arm lists EVERY displayed text input (torsion has no pre-filled text
defaults); typing any — including the optional `arbor_dia` and TwoLoad's four point
fields — clears blank. The moment-entry fields count only in their active entry mode:
`moments` in Direct mode; `forces`/`load_radius` in ForceAtRadius mode (mirroring
extension's `hook_mode`-gated blank term — final-review refinement). Selectors
(`scenario`, `friction_model`, `moment_entry`) are excluded: they always hold a
default and cannot distinguish an untouched form. Invariant tests mirror extension's
(untouched-per-scenario blank; each field-class trips it; selector changes alone do
not; cross-family outcome clearing already covered).

## F. View & presenter

- `tor_inputs_view` returns the per-scenario `FieldDescriptor` list with unit-aware
  labels (`Rate (N·mm/°)` / `(lbf·in/°)`; `Angle 1 (°)`; `Forces (N), comma-separated`;
  `Load radius (mm)`). The moment-entry selector + friction pick-list render in the
  design panel chrome (view.rs), not in the descriptor list — same split as extension's
  hook-mode toggle.
- Results/status panels are UNCHANGED — every scenario yields the same `TorsionDesign`,
  and `tor_results_view` already renders it (RateBased/TwoLoad users see the derived
  body coils in the existing geometry rows; no new result rows).

## G. Testing & gates

**springcore (mutation-gated, literal 0 survivors in-diff):** per-variant round-trips
(all four, arbor None/Some where shaped, both friction models on at least one variant);
per-variant non-finite rejection incl. a negative-but-finite TwoLoad angle accepted and
a non-finite one rejected; the LEGACY tag-less flat TOML → `DataFile` error test;
`solve_with_material` Torsion-arm test updated.

**springmaker:** per-scenario `parse_and_solve` oracles on the engine's golden geometry
(RateBased: rate entered as the metric display value ≈ 8.875 N·mm/° — exactly
`0.5085 N·m/rad × 1000 × π/180` — must derive body coils 5.0 and round-trip the rate;
Dimensional OD 22 ≡ mean 20; TwoLoad two points on the oracle line + the
offset-shifted pair solving identically); F@r ≡ Direct equivalence (10 N @ 50 mm ≡
"500" N·mm); `build_spec` ↔ `populate_from_spec` round-trips metric + US per scenario
(F@r forms round-trip to Direct with derived moments; stale `forces`/`load_radius`
cleared); `dimensional_mean_check` at both call sites (parse + build, metric + US
boundary); `is_blank` invariant per scenario; error paths (outer ≤ wire named error;
TwoLoad degenerate points surfacing the engine's named messages through
`format_error`); `tor_inputs_view` descriptor/label tests per scenario + unit system;
Simulator E2E: drive each scenario through real widgets to Populated, toggle F@r and
solve, save/load round-trip per scenario, and a legacy-file load surfacing the
clean-break error in `action_error`.

**Gates:** local CI-parity set + in-diff mutation; mandatory adversarial panel —
floor three (general-code, architect, simplifier) + MANDATORY input-domain adversary +
persistence/wire-format reviewer (the migration) — cycled to convergence before push.

## Task shape (for the plan)

1. springcore: `TorsionSpec` enum migration + tests (round-trips, legacy clean-break,
   non-finite, solve_with_material) — isolated so the format change reviews alone.
2. springmaker: scenario enum + picker + RateBased (form/helpers/presenter/dispatch).
3. Dimensional (+ `dimensional_mean_check` both sites) and TwoLoad (fields, angles).
4. Force-at-radius toggle (MomentEntry, fields, conversion, view).
5. Simulator E2E per mode + legacy-error E2E + final whole-branch review.
