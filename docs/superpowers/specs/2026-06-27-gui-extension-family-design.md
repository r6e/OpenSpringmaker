# Extension Family in the GUI — Design (Spec 1b)

**Status:** Approved (design dialogue 2026-06-27)
**Scope:** Bring the **extension** spring family to the `springmaker` GUI as a second
family, proving the multi-family architecture end-to-end on one input mode
(**PowerUser**). Spans `springmaker` (family machinery + extension GUI module) and
`springcore` (extension design status + persistence refactor).
**Sub-project:** 1b of the "bring families to the GUI" effort, built on the 1a scaffolding
(`presenter.rs`, `widgets.rs`, `compression/` module). The remaining extension input modes
(TwoLoad / RateBased / Dimensional / MinWeight) are spec **1c**.

## Goal

After 1a the GUI is structurally per-family but still only renders compression. 1b makes
**Extension** a real, selectable family: a family selector on the Calculator screen, an
`extension/` GUI module that drives the existing `springcore` extension engine, three-stress
hook results, engineering status, and save/load — all the way through, for the PowerUser
scenario. Doing one mode end-to-end proves the family-scoped state/message/dispatch pattern
(and the persistence shape) before 1c fans the pattern out to the other four modes
mechanically.

## Non-goals (deferred)

- **Extension TwoLoad / RateBased / Dimensional / MinWeight** input modes in the GUI — spec 1c.
  (The engine already supports them; 1b only wires PowerUser.)
- **Torsion** in the GUI — a later spec.
- **Extension fatigue** in the GUI — not in the engine's extension surface; out of scope.
- Any change to compression *behavior* (the compression refactors here are behavior-preserving).

## Architecture

### Module layout

```
springcore/src/
  extension/design.rs   — ExtensionDesign gains `status: DesignStatus` (computed in solve_forward)
  persistence.rs        — Family enum; SavedDesign scenario field becomes a family-tagged enum;
                          ExtScenarioSpec (PowerUser); solve dispatch on family
springmaker/src/
  app.rs        — Family selector state; Message Comp*/Ext*/SelectFamily; App { family,
                  material, unit_system, compression, extension }; view()/recompute()/save/load dispatch
  presenter.rs  — FieldDescriptor<F> becomes generic over the family's field type
  widgets.rs    — reused unchanged (family selector uses the existing styled_pick_list)
  compression/  — Field moves in as compression::form::Field; FieldDescriptor<Field>;
                  material/unit_system read from App, not FormState (behavior-preserving)
  extension/
    mod.rs
    form.rs       — extension::form::Field, ExtFormState (PowerUser + hook mode), parse_and_solve,
                    build_spec / populate_from_spec (persistence)
    view_model.rs — ExtensionDesign → geometry ResultRows + three-stress ExtLoadTable + status lines
    view.rs       — humble PowerUser view (inputs + hook toggle + three-stress results)
```

### Family-scoped state, messages, dispatch

- **`Family`** lives in `springcore` (single source of truth, like `UnitSystem`): `enum Family
  { Compression, Extension }`, `#[derive(Default)]` with `Compression` default, serde-serializable.
  The GUI uses `springcore::Family`.
- **`App`** gains `family: Family`, and holds **both** families' form state always-constructed —
  `compression: compression::form::FormState`, `extension: extension::form::FormState` — so
  switching families preserves each one's in-progress input (no dead-variant problem; both wired).
  `material: String` and `unit_system: UnitSystem` are **lifted out of `FormState` to `App`**
  (shared across families, matching where `SavedDesign` already keeps them).
- **Messages** (in `app.rs`): `SelectFamily(Family)`; `CompField(compression::form::Field, String)`
  (the existing `Field`, moved into the compression module and the variant renamed); `ExtField(
  extension::form::Field, String)`. `Material`, `Units`, `Save`, `Load`, navigation, and the
  materials-editor messages stay shared.
- **`presenter::FieldDescriptor<F>`** becomes generic over the family's field enum: each family's
  `inputs_view` builds `FieldDescriptor<its Field>`, and its humble view maps that descriptor to
  its own message variant. The shared `presenter` stays family-agnostic.
- **Dispatch:** `App::view()`'s `Screen::Calculator` arm and `App::recompute()` match on `family`
  and call the active family's `view`/`parse_and_solve`. The **family selector** is a segmented
  `styled_pick_list` (`widgets.rs`) at the top of the Calculator screen.

This is the principled continuation of 1a's per-family split: shared primitives in
`presenter`/`widgets`, each family self-contained in its module, the shell (`app.rs`) routing.

## Extension form → solve (PowerUser)

`ExtFormState` holds the PowerUser inputs as strings: `wire_dia`, `mean_dia`, `active`,
`free_length`, `initial_tension`, `loads`, plus a **hook mode** — `Default` | `Custom { r1, r2 }`
(mirroring the engine's `HookSpec`). `parse_and_solve(form, material, unit_system, correction)`:

1. Parses the numeric inputs in the active unit system.
2. Resolves the hook mode: `Default` → `HookEnds::default_for(mean_dia)` (r1 = D/2, r2 = D/4);
   `Custom` → the parsed `r1`, `r2`.
3. Builds `extension::scenario::PowerUser { wire_dia, mean_dia, active, free_length,
   initial_tension, hooks, loads }` and calls `.solve(material, correction)`.

`correction` is the existing global `App.correction` (curvature-correction setting) — reused, no
new control. The result is an `ExtensionDesign` wrapped in an extension `FormOutcome` (design +
the same error-vs-outcome exclusivity the compression form uses).

## Results & status (three-stress)

The extension presenter (`extension/view_model.rs`) maps `ExtensionDesign` to view data:

- **Geometry rows** (shared `ResultRow`): spring index, active coils, rate, free length, outer
  diameter, inner diameter, initial tension.
- **Load-point table** — extension-specific (per 1a's "aggregates are family-specific"): a new
  `ExtLoadTable` / `ExtLoadRow` in `extension/view_model.rs` with **three** stress columns —
  **body shear**, **hook bending**, **hook torsion** — each with its `%`-of-allowable, formatted
  from `ExtLoadPoint.{body_shear, hook_bending, hook_torsion}` and the three `pct_*_allow`. Built
  from shared formatting primitives where they apply.
- **Status lines** (shared `StatusKind` / `StatusLine`): rendered from the engine-computed
  `ExtensionDesign.status` (below), using the same severity→kind mapping compression uses.

## Engine change: `ExtensionDesign.status` (springcore)

For family symmetry, status is computed in the **engine**, not the GUI. `ExtensionDesign` gains
`status: DesignStatus` (the crate-root type), populated by `solve_forward` via a new
`evaluate_status`, mirroring the **torsion** `evaluate_status` precedent:

- **Overstress** (`Severity::Warning`), per load point and per stress: when any of
  `pct_body_allow`, `pct_hook_bending_allow`, `pct_hook_torsion_allow` exceeds `1.0`, emit a
  load-point-indexed warning naming the stress (body shear / hook bending / hook torsion) that
  exceeded its allowable.
- **Index caution** (`Severity::Caution`): reuse the shared `crate::design::index_caution(index)`
  helper (already shared by compression and torsion) for the recommended 4–12 band.

All existing `ExtensionDesign` producers (the four scenarios and the min-weight optimizer
`ExtMinWeightSolution.design`) get the field for free since it is set in `solve_forward`. Both
golden/oracle and status-branch tests cover it; mutation gate applies.

## Persistence (springcore): clean family-tagged scenario

`SavedDesign`'s scenario field becomes **family-tagged**, replacing the bare `scenario:
ScenarioSpec`:

```rust
pub struct SavedDesign {
    pub material: String,
    pub unit_system: UnitSystem,
    pub design: DesignSpec,
}

#[serde(tag = "family")]
pub enum DesignSpec {
    Compression(ScenarioSpec),       // the existing 5 compression variants, unchanged
    Extension(ExtScenarioSpec),      // new
}

#[serde(tag = "type")]
pub enum ExtScenarioSpec {
    PowerUser {
        wire_dia_mm: f64, mean_dia_mm: f64, active: f64, free_length_mm: f64,
        initial_tension_n: f64, hooks: HookSpecSpec, loads_n: Vec<f64>,
    },
    // 1c: TwoLoad / RateBased / Dimensional / MinWeight
}

#[serde(tag = "mode")]
pub enum HookSpecSpec { Default, Custom { r1_mm: f64, r2_mm: f64 } }
```

`Family` is the discriminant tag. **Back-compat:** this changes the on-disk shape, so save files
written before 1b will not deserialize. This is a deliberate, **surfaced** break — a load failure
returns `SpringError::DataFile` and the app already shows load errors in the status panel (no
silent data loss); the user re-saves. (This overrides the 1a "additive, no schema bump" note,
chosen for a clean schema in design review.)

**`SavedDesign::solve*` are compression-only (amended during 1b implementation).** They dispatch
on the `DesignSpec` variant: `Compression(spec)` solves as today and yields a compression
`SpringDesign`; an `Extension(_)` design returns `SpringError::InconsistentInputs` rather than
solving through `SavedDesign`. The original design routed extension through
`extension::scenario::PowerUser` here too, but that proved redundant: the return type
`Result<SpringDesign>` is the *compression* design, and the GUI already re-solves extension
designs through the form. So `SavedDesign` serializes extension inputs but does not solve them,
and the method keeps its name (the `SpringDesign` return type already signals the restriction).
The GUI round-trips spec↔form and re-solves through the form: `populate_from_spec` (`ExtScenarioSpec`
→ FormState) then `extension::form::parse_and_solve` on recompute. `extension::form::build_spec`
returns an `ExtScenarioSpec` that `App::save_to` wraps in `DesignSpec::Extension` — mirroring
compression's `ScenarioSpec` → `DesignSpec::Compression`; `App::save_to`/`apply_saved` dispatch on
`family`. On load, `SavedDesign::from_toml` rejects any non-finite (`inf`/`nan`) float before it
reaches a `DesignSpec`, and `SavedDesign::save` writes atomically (temp file + rename).

## Data flow

```
family selector → App.family
[Extension] ExtField msgs → App.extension (ExtFormState, strings)
recompute → extension::form::parse_and_solve(form, App.material, App.unit_system, App.correction)
          → extension::scenario::PowerUser.solve → ExtensionDesign (with status)
          → extension FormOutcome
view → extension::view_model (geometry rows + ExtLoadTable + status lines)
     → extension::view (humble iced widgets, shared widgets.rs kit)
save → build_spec → SavedDesign{ material, unit_system, design: DesignSpec::Extension(...) } → TOML
load → SavedDesign → populate_from_spec → ExtFormState → recompute
```

## Error handling

- Reuse the existing form error model: `parse_and_solve` returns `Result`; the extension
  `FormOutcome` keeps the error-vs-outcome exclusivity (a present outcome means the solve
  succeeded). Parse errors and `SpringError`s surface in the results/status panels exactly as
  compression's do (shared `format_error` reused or mirrored).
- Hook `Custom` radii, initial tension, and all numerics are validated at parse (non-finite /
  non-positive rejected with a field-named message); the engine's own input guards remain the
  defense-in-depth backstop.
- Persistence load failures (including pre-1b files) surface as `DataFile` errors in the status
  panel.

## Testing

Mirror compression's coverage; strict TDD.

- **springcore:**
  - `ExtensionDesign.status`: golden/oracle status for a clean design; one test per status branch
    (each overstress stress, load-point indexing, index caution); existing extension scenario and
    optimizer tests updated for the new field.
  - Persistence: extension PowerUser round-trip (`SavedDesign` → TOML → `SavedDesign`, identical);
    `DesignSpec` family tagging; `HookSpecSpec` Default and Custom round-trips; `SavedDesign::solve`
    extension dispatch; a pre-1b compression TOML produces a clear `DataFile` load error (the
    surfaced break is asserted, not silent).
  - `cargo mutants --in-diff` on `springcore`, literal 0 survivors.
- **springmaker:**
  - Extension presenter unit tests: PowerUser `inputs_view` field set (incl. hook-mode
    conditional fields), three-stress `ExtLoadTable` rows, status-line derivation from
    `ExtensionDesign.status`, hook-toggle resolution (Default vs Custom), unit-aware labels.
  - Compression presenter tests still pass unchanged after the `Field` move + material/units lift
    (behavior-preserving).
  - Headless `Simulator` E2E: select Extension → enter PowerUser inputs → solve → assert the
    three-stress results render; toggle hook mode; save→load round-trip through the UI.

## Global constraints

- MSRV 1.88; iced 0.14; dual MIT/Apache; **SI canonical** in the engine, convert at the boundary.
- ADR 0008 presenter / humble-view split preserved for the new extension screen.
- Every engine formula already cited in `springcore`; status thresholds cite Shigley allowables
  (already encoded in the `pct_*_allow` fields and `Material.allowable_pct_*`).
- `cargo fmt`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `RUSTDOCFLAGS="-D warnings" cargo doc`, repo-wide `typos`, `cargo deny check all`,
  `cargo test --workspace`, and `cargo mutants --in-diff` (springcore) all green before push.
- No commercial-product/vendor references in any persisted file.
- No `#[allow(dead_code)]` or lint-suppression scaffolding; internal enums stay exhaustive
  (no `#[non_exhaustive]` on GUI-matched enums). Mandatory adversarial multi-agent review panel
  before push, cycling to convergence.

## Deferred / open items (for 1c and beyond)

- Extension TwoLoad / RateBased / Dimensional / MinWeight GUI modes (engine ready), incl. an
  extension scenario sub-picker and the `ExtScenarioSpec` 1c variants.
- Torsion family in the GUI.
- Re-evaluate whether the unit-conversion helpers (now in `compression/view_model.rs`) should be
  lifted into a shared module once the extension presenter needs the same mm/in, MPa/ksi
  conversions — lift, don't duplicate.
