# Conical GUI Family — Design

**Status:** Approved
**Scope:** The fourth GUI family tab — conical compression springs, PowerUser
scenario only (matching the merged engine, PR #57). springcore surface:
`Family::Conical` (enum + `ALL_FAMILIES` + `Display`) and ONE extended status
message (both mutation-gated). ZERO persisted-format changes (`ConicalSpec`
shipped with the engine; nothing here touches serialization) — the final
panel carries NO persistence reviewer, with this reason stated in the briefs.
Completes the conical increment; the compression-variants sequence continues
with assemblies afterward.

## Decisions (settled during brainstorming)

1. **Telescoping message extension** (user decision): the ENGINE's Info
   message gains the stress caveat (one message + its pinned test; the engine
   owns the message, every consumer benefits). New verbatim string in §A.
2. **Linear-model note = results-panel footer** (user decision): a muted,
   always-present line at the bottom of the conical results panel (the
   compression fatigue-note widget idiom). Verbatim string in §C.
3. **Correction threading**: conical's `parse_and_solve` takes the app-global
   `CurvatureCorrection` (the compression pattern) — a DOCUMENTED divergence
   from the torsion template, whose solver takes none.
4. **Single-scenario shape**: no scenario picker, no arm matrices — `is_blank`
   is a flat all-fields check; `populate_from_spec` has one arm.
5. **The placeholder dies**: `apply_saved`'s conical early-reject and its
   `unreachable!()` arm are deleted, replaced by the real populate arm. The
   bool return stays (all arms now return `true`); the two wholesale-reject
   regression tests are REPLACED by positive load→populate→recompute tests
   (the load-path invariant they pinned — action_error surviving recompute —
   no longer applies to conical, which now recomputes like any family).

## A. springcore (mutation-gated 0 in-diff survivors)

- `springcore/src/family.rs`: `Conical` variant added to `Family` (after
  `Torsion`), `ALL_FAMILIES` gains it (display order: after Torsion),
  `Display` renders exactly `"Conical"`. Exact-string + ALL-content tests per
  the family-enum test pattern already in the file.
- `springcore/src/conical/design.rs` (`evaluate_status`): the telescoping
  Info message becomes, VERBATIM:
  `"coils telescope (per-coil radial step ≥ wire diameter); the reported
  solid length is conservative — the true solid height is lower and the
  reported at-solid stress is correspondingly understated"`.
  The existing pinned status test updates to the new string.

## B. Form (`springmaker/src/conical/form.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    WireDia,
    LargeMeanDia,
    SmallMeanDia,
    Active,
    FreeLength,
    Loads,
}

#[derive(Debug, Clone)]
pub struct ConFormState {
    /// End-type key ("plain" | "plain_ground" | "squared" | "squared_ground"),
    /// the compression picker convention.
    pub end_type: String,
    pub wire_dia: String,
    pub large_mean_dia: String,
    pub small_mean_dia: String,
    pub active: String,
    pub free_length: String,
    /// Comma-separated loads (compression's loads_n idiom).
    pub loads: String,
}
```

Manual `Default` with `end_type: "squared_ground"` and empty strings.

- `is_blank`: all six text fields empty-trimmed (`end_type` excluded —
  default-holding selector).
- `pub struct ConFormOutcome { pub design: springcore::conical::ConicalDesign }`.
- `parse_and_solve(form, material_name, us, materials, correction) ->
  Result<ConFormOutcome>`: parse fields via the established helpers
  (`length_mm` for the four lengths, `positive_num` for active coils,
  `loads_n` for the list), resolve `EndType` from the key (the
  compression-path treatment — verify at plan time where the string→enum
  conversion lives for the GUI solve path and mirror it), build
  `ConicalInputs`, call `springcore::conical::solve_forward(material,
  &inputs, &loads, correction)`. Field-name prefixes in parse errors:
  "wire diameter", "large mean diameter", "small mean diameter",
  "active coils", "free length", "load".
- `build_spec(form, us) -> Result<ConicalSpec>` →
  `ConicalSpec::PowerUser { ... }` (mm/N via the helpers).
- `populate_from_spec(form, spec, us)`: one arm; `fmt_len`/`fmt_loads`/
  `format!("{active}")` round-trip.

## C. Presenter + view (ADR 0008)

`view_model.rs`:

```rust
pub enum ConResultsView {
    Error(String),
    Empty,
    Populated(Box<ConPopulatedResults>),
}

pub struct ConPopulatedResults {
    pub governing_rate: GoverningRate,
    pub geometry: Vec<ResultRow>,
    pub load_table: LoadTable,
}
```

> **Implementation note (panel finding):** The `pub status: Vec<StatusLine>` field originally
> specified here was dropped. §D specifies a single shared `status_panel` across all families —
> status renders once via that shared path, not via a field on `ConPopulatedResults`.
> Including the field in the struct would have been dead data never read by `render_populated`.

Geometry rows (labels exact; every numeric via `fmt_row_value`; lengths via
`display_len` + unit label, indices/coils unitless):

| Label | Source | Decimals |
|---|---|---|
| Large end OD | large_outer_dia | 4 |
| Large end ID | large_inner_dia | 4 |
| Small end OD | small_outer_dia | 4 |
| Small end ID | small_inner_dia | 4 |
| Index (large end) | index_large | 3 |
| Index (small end) | index_small | 3 |
| Taper per coil | taper_per_coil | 4 |
| Total coils | total_coils | 3 |
| Pitch | pitch | 4 |
| Solid length (conservative) | solid_length | 4 |

The load table reuses the shared `LoadTable`/`LoadRow` (per-load force,
deflection, length, stress at the governing coil, %MTS) with the at-solid
treatment mirroring compression's. Status lines flow from
`springcore::conical::evaluate_status` through the shared status plumbing.

`view.rs`:
- Setup group: material picker + end-type picker (compression's
  `find_by_key`/`styled_pick_list` idiom emitting the String key). No
  scenario, friction, or hook chrome.
- Inputs group: the six-field descriptor loop; widget ids `con-wire-dia`,
  `con-large-mean-dia`, `con-small-mean-dia`, `con-active`,
  `con-free-length`, `con-loads`.
- Results panel: hero rate → Geometry section → Load table → status, and the
  LINEAR-MODEL FOOTER, always present when the panel renders (muted, the
  compression fatigue-note widget idiom), VERBATIM:
  `"Linear-range model: progressive stiffening as coils bottom out is not
  modeled."`

## D. App wiring

- `App` gains `conical: ConFormState` and
  `con_outcome: Option<ConFormOutcome>`.
- `Message::ConField(Field, String)` + `set_con_field`.
- The four wildcard-free `Family` matches gain conical arms: the calculator
  design/results dispatch (calculator.rs ×2), `recompute` (blank-clears +
  `parse_and_solve` with `self.correction`), and `save_to`
  (`DesignSpec::Conical(build_spec(...)?)`).
- `apply_saved`: early-reject + `unreachable!()` deleted; real arm sets
  `Family::Conical` + populates; returns `true` like the siblings.

## E. Testing & gates

- **springcore:** Family Display/ALL exact-string tests; the new telescoping
  message pinned. Mutation in-diff 0 survivors.
- **Form:** build/populate round-trip in BOTH unit systems; is_blank matrix
  (each field individually trips it; end_type alone does not); parse errors
  surface with the right field prefixes; a solve golden through the form
  (metric taper case — assert rate/stress against direct engine values);
  the correction selector changes the through-form stress (compression's
  test pattern).
- **Presenter:** the geometry-row table (labels/values/units exact);
  Error/Empty/Populated mapping; huge-value scientific rendering on a stress
  cell (hardening standard); telescoping Info passthrough with the NEW
  message; the linear-model footer present in Populated, absent in Empty.
- **E2E:** family-switch → type the six fields → results render (heading +
  a geometry row + the footer); save/load round-trip through a real TOML
  file (replacing the two placeholder tests) asserting the conical form
  populates, the family switches, and recompute yields results.
- **Gates:** local CI-parity + in-diff mutation; final panel — floor 3 +
  MANDATORY input-domain adversary (form-domain × family-switch × load-path
  matrix incl. stale-state across tab switches); NO persistence reviewer
  (zero format surface — `ConicalSpec` untouched; reason stated in briefs).

## Task shape (for the plan)

1. springcore Family surface + telescoping-message extension + the form
   layer + ALL app dispatch arms (the enum-crosses-crate wiring lands with
   its consumers) + form tests + mutation gate.
2. Presenter/view (geometry table, load table, footer, pickers, ids) +
   presenter tests + E2E + placeholder-test replacement + full gate.
