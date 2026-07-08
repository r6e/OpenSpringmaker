# Assembly GUI Family — Design

**Status:** Approved
**Scope:** The SIXTH GUI family tab — assemblies (the engine merged as PR #59).
Completes the assembly increment. Carries two firsts (a dynamic member-list
widget; per-member material pickers) and two engine-panel carry-forward
obligations (the end-to-end topology-rejection pin; the member
`DiameterOutOfRange` re-localization). Two mutation-gated springcore changes
beyond the `Family` enum: (1) the structured `SpringError::Member` variant
enabling the re-localization; and (2) `solve_assembly`/`evaluate_status`
retyped `&MaterialSet` → `&MaterialStore` — a correctness fix (resolves
user-overlay member materials that `MaterialSet` cannot see) and a consistency
fix (all four sibling `parse_and_solve` functions already took `&MaterialStore`).
Two SDD tasks, mirroring the conical GUI split.

## Decisions (settled during brainstorming)

1. **Stacked member cards** (user): each member is a full-width bordered card
   stacking its own fields; fits the narrow left input panel and reads
   clearly at 3+ members. NOT compact horizontal rows (pickers need width).
2. **The bool pendulum ends here** (user): `apply_saved -> bool` STAYS
   permanently with a doc note — it returns `false` for a family with no GUI
   yet; always `true` today, retained so the next family placeholder
   (rectangular wire, roadmap 5-6) does not flip it a fourth time. (It went
   `()` at conical → `bool` at the assembly engine → permanent here.)
3. **Re-localize member `DiameterOutOfRange`** (user, the carry-forward):
   introduce `SpringError::Member { index, source }` (§A) so the GUI's
   `format_error` can render a member's out-of-range wire diameter in the
   active unit system (inches for US). Fixes localization uniformly for every
   member error, not just diameter range.
4. **Dynamic member ids are E2E-testable** (verified, not a user choice):
   iced 0.14 has `impl From<String> for text_input::Id` and
   `widget::Id::new(impl Into<Cow<'static,str>>)`, and the Simulator resolves
   a runtime-built id — so member fields get indexed ids
   (`asm-member-{i}-wire-dia`) and REAL Simulator-click E2E, via a runtime-id
   `labeled_input` variant (§C). No message-dispatch fallback needed.
5. **Zero persisted-format change**: `AssemblySpec` shipped with the engine
   and is untouched; `SpringError` is not persisted. The topology-rejection
   pin is a FORM-TEST obligation (the GUI's `parse_and_solve` wires
   `parse_topology`), not a format change. The final panel carries NO
   persistence reviewer — reason stated in the briefs.

## A. springcore (mutation-gated 0 in-diff survivors)

### A1. `Family::Assembly`

`springcore/src/family.rs`: `Assembly` after `Conical` in the enum,
`ALL_FAMILIES` (after Conical), `Display` → exactly `"Assembly"`. The
exact-string + ALL-content test per the file's pattern. This breaks the four
wildcard-free springmaker `Family` matches — wired in Task 1 (§E).

### A2. Structured member error + re-localization

`springcore/src/error.rs`: add

```rust
/// A member-scoped error from an assembly solve, preserving the underlying
/// error's structure (so a UI layer can re-localize it) plus the 1-based
/// member attribution.
Member { index: usize, source: Box<SpringError> },
```

Its `Display` reproduces today's flattened string BYTE-IDENTICALLY (so every
existing engine string contract holds): for an `InconsistentInputs(m)` source
it renders the raw `m` (no doubled `"inconsistent inputs:"` prefix); any other
source flattens via its own `Display`:

```rust
Self::Member { index, source } => {
    let inner = match source.as_ref() {
        SpringError::InconsistentInputs(m) => m.clone(),
        other => other.to_string(),
    };
    write!(f, "member {}: {inner}", index + 1)
}
```

`springcore/src/assembly/design.rs` `member_error` (currently flattens to
`InconsistentInputs("member N: …")`) becomes:

```rust
fn member_error(index: usize, err: SpringError) -> SpringError {
    SpringError::Member { index, source: Box::new(err) }
}
```

Engine test updates (Task 1, same commit): the `msg()` helper in the assembly
test module gains a `Member` arm returning the flattened string (byte-
identical, so `member_errors_carry_the_member_prefix`'s asserted strings are
UNCHANGED); any assertion that `matches!(…InconsistentInputs)` on a member
error switches to the `Member` variant. Assembly-LEVEL guards (empty members,
loads, nested-free-length, output) stay `InconsistentInputs` — unchanged.

`springmaker/src/form_helpers.rs` `format_error` gains the recursion that
delivers the localization (its `DiameterOutOfRange` arm already renders the
active unit system):

```rust
SpringError::Member { index, source } => {
    let inner = match source.as_ref() {
        SpringError::DiameterOutOfRange { .. } => format_error(source, units),
        SpringError::InconsistentInputs(m) => m.clone(),
        other => other.to_string(),
    };
    format!("member {}: {inner}", index + 1)
}
```

## B. Form (`springmaker/src/assembly/form.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberField {
    WireDia,
    MeanDia,
    Active,
    FreeLength,
}

/// One member's form inputs (all strings; material/end-type via pickers).
#[derive(Debug, Clone)]
pub struct AsmMemberForm {
    pub material: String,   // a material name, the app's default at add-time
    pub end_type: String,   // key: "squared_ground" default
    pub wire_dia: String,
    pub mean_dia: String,
    pub active: String,
    pub free_length: String,
}

#[derive(Debug, Clone)]
pub struct AsmFormState {
    pub topology: String,   // "nested" | "series"
    pub fixity: String,     // "fixed_fixed" default
    pub loads: String,      // comma-separated
    pub members: Vec<AsmMemberForm>,
}
```

`Default`: topology `"nested"`, fixity `"fixed_fixed"`, empty loads, ONE
default member (a blank card — the form opens ready to fill, min one member).

- `is_blank`: true when every member's text fields AND loads are empty (the
  default-holding topology/fixity/material/end-type selectors excluded). Used
  by `recompute` to suppress solving an untouched form.
- New members seed `material` from the app's active material name at add
  time (the add-member handler passes it in), `end_type` `"squared_ground"`,
  and empty text fields.
- `parse_and_solve(form, us, materials, correction) -> Result<AssemblyDesign>`:
  `parse_topology(&form.topology)?` and `parse_fixity(&form.fixity)?` (THE
  topology-pin obligation — a bad topology from a loaded file now errors
  here), build `Vec<AssemblyMember>` (each: `length_mm`/`positive_num`
  helpers, `parse_end_type(&m.end_type)?`, `material_name: m.material.clone()`),
  `loads_n(&form.loads, us)`, then `solve_assembly(materials, &inputs, &loads,
  fixity, correction)`. Member field-error prefixes are the ENGINE's job
  (§A2) — the GUI passes member material names straight through.
- `build_spec` → `AssemblySpec::PowerUser { topology, fixity, loads_n,
  members: Vec<AssemblyMemberSpec> }` (field order per the engine's
  TOML-forced shape). `populate_from_spec`: rebuild the member Vec from the
  spec (round-trips).

Messages (app.rs): `AsmTopology(String)`, `AsmFixity(String)`,
`AsmLoads(String)`, `AsmField(usize, MemberField, String)`,
`AsmMemberMaterial(usize, String)`, `AsmMemberEndType(usize, String)`,
`AsmMemberAdd`, `AsmMemberRemove(usize)`. Add/remove mutate `self.assembly.
members` and recompute; remove guards the min-one-member floor (removing the
last member is a no-op OR disabled in the view — the view hides Remove when
`len == 1`).

## C. Widgets

- **`FIXITIES` hoists** to `picker.rs` as `pub(crate) const FIXITIES:
  &[KeyLabel]` (second consumer, the END_TYPES precedent); compression's
  reference updates.
- **Runtime-id input helper**: a `labeled_input` variant accepting `impl
  Into<std::borrow::Cow<'static, str>>` for the id (iced supports it), so
  member fields get `format!("asm-member-{i}-wire-dia")` ids. The existing
  `&'static str` helper stays for the fixed-id families; the new one is used
  by the member cards. (Consolidate if the `&'static str` sites trivially
  accept the broader bound — decide at plan time; do not churn all families.)
- **Member-card builder** (view.rs): loops `members` into stacked
  `bordered_card`-style columns via `column.push()` (the load-table render
  precedent for variable-length children). Each card: a header row with
  "Member N" + a Remove button (hidden when `len == 1`); a per-member material
  picker (the app-global `material_picker` idiom, parameterized by index,
  emitting `AsmMemberMaterial(i, key)`); an end-type picker
  (`AsmMemberEndType(i, key)`); the four runtime-id text fields. Below the
  list: an "Add member" button (`AsmMemberAdd`).

## D. Presenter + view (ADR 0008)

`view_model.rs`:

```rust
pub enum AsmResultsView { Error(String), Empty, Populated(Box<AsmPopulatedResults>) }

pub struct AsmPopulatedResults {
    pub governing_rate: GoverningRate,       // combined rate hero
    pub summary: Vec<ResultRow>,             // topology, free/solid length,
                                             // travel-limit deflection+force,
                                             // "limited by member N"
    pub assembly_loads: LoadTable,           // assembly-level per-load state
    pub members: Vec<AsmMemberResultView>,   // one per member
}

> **Implementation note (panel finding):** A `pub status: Vec<StatusLine>` field was dropped before implementation. §D specifies a single shared `status_panel` across all families — status renders once via the shared path (`asm_status_view`), not via a field on `AsmPopulatedResults`. The field would have been dead data never read by `render_populated`.

pub struct AsmMemberResultView {
    pub heading: String,        // "Member N (Music Wire)"
    pub rows: Vec<ResultRow>,   // share %, rate, index, buckling flag
    pub loads: LoadTable,       // per-member per-load stress/%MTS
}
```

`con_results_view`-style ordering: `asm_outcome` FIRST (the conical
ordering-trap lesson), `app.error` in the `None` fallback. Every numeric via
`fmt_row_value`. `summary` rows: topology label; free length; solid length
(nested = max member, series = sum — both exact per the engine, no
conservative caveat); travel-limit deflection + force; the limiting-member
callout ("Travel limited by member N"). Statuses flow from
`springcore::assembly::evaluate_status(&out, materials)` through the shared
status plumbing — the member-prefixed overstress/clearance/buckling/travel
messages pass straight through.

`view.rs` results panel: hero rate → Summary section → assembly Load table →
per-member sections (each: heading, member rows, member load table) →
statuses (shared status panel). Setup group: topology picker + fixity picker
(from `picker.rs`) + assembly loads field; then the member-card list (§C).

## E. App wiring

- `App` gains `assembly: AsmFormState` + `asm_outcome:
  Option<AssemblyDesign>` — `solve_assembly` returns `AssemblyDesign`
  directly, so it is stored unwrapped and the presenter reads it as
  `&AssemblyDesign` (no `…FormOutcome` wrapper — the conical `ConFormOutcome`
  existed only to carry a `.design`; here that indirection is unnecessary).
- The four `Family` matches gain Assembly arms: calculator design/results
  dispatch (×2), `recompute` (blank-clear + `parse_and_solve` with
  `self.correction`), `save_to` (`DesignSpec::Assembly(build_spec(...)?)`).
- `apply_saved`: the Assembly early-reject arm (currently `return false`)
  DELETED; a real arm sets `Family::Assembly` + `populate_from_spec`; the
  `unreachable!()` arm removed. `apply_saved` KEEPS `-> bool`, now always
  `true`, with the Decision-2 doc note (permanent-signal rationale). The two
  placeholder tests (`…not supported…`, the load-survives-recompute one) are
  REPLACED by positive load→populate→recompute tests.
- Message arms + the per-member setters; add/remove handlers.

## F. Testing & gates

- **springcore (mutation-gated):** `Family::Assembly` Display/ALL exact; the
  `SpringError::Member` Display byte-identity (a test pinning
  `Member{InconsistentInputs("x")}.to_string() == "member 1: x"` and a
  non-InconsistentInputs source flattening); the assembly engine's updated
  member-error tests still pin the same strings.
- **format_error:** a `Member{DiameterOutOfRange}` renders inches under US and
  mm under Metric (both pinned); a `Member{InconsistentInputs}` renders
  `"member N: {m}"` with no doubled prefix.
- **Form:** build/populate round-trip (both units, 1 and 3 members); is_blank
  (each member field + loads trip it; selectors don't); THE TOPOLOGY PIN — a
  loaded `AssemblySpec` with `topology = "stacked"` errors through
  `parse_and_solve` with `"unknown topology: stacked"` (the engine-panel
  carry-forward, now end-to-end); a through-form golden vs a direct
  `solve_assembly`; add/remove mutate the member Vec; per-member material
  resolution.
- **Presenter:** summary + per-member row tables (labels/values/units exact);
  Error/Empty/Populated mapping (outcome-first); huge-value scientific on a
  member stress cell; the member-prefixed status passthrough (clearance
  warning present with the exact engine message); limiting-member callout.
- **E2E (real Simulator clicks, incl. dynamic ids):** family-switch → fill
  member 1 → Add member → fill member 2 (indexed ids resolve) → results
  render (combined rate + a per-member section); Remove member 2 → results
  reflect one member; save/load round-trip through a real TOML (replacing the
  placeholder tests); a US-unit member with an out-of-range wire diameter →
  the results/error surface shows the inch-formatted member message.
- **Gates:** local CI-parity + in-diff mutation (the Family enum + the
  `SpringError::Member` variant + `member_error` are the springcore surface);
  final panel — floor 3 + MANDATORY input-domain adversary (the member ×
  topology × add/remove × unit-toggle matrix; the dynamic-id resolution; the
  re-localization path); NO persistence reviewer (zero format surface —
  `AssemblySpec` untouched, `SpringError` not persisted; the topology pin is a
  form test; reason stated in the briefs).

## Task shape (for the plan)

1. springcore (`Family::Assembly` + the `SpringError::Member` variant +
   `member_error` + `format_error` recursion + engine member-error test
   updates) + the form layer (state, dynamic add/remove, parse_and_solve
   wiring parse_topology/parse_fixity, build/populate) + the `FIXITIES` hoist
   + the runtime-id input helper + the member-card view + ALL app dispatch
   arms + minimal results skeleton (Empty/Error) + form tests + mutation gate.
2. Full presenter/view (summary, assembly + per-member load tables, statuses)
   + presenter tests + E2E (dynamic Simulator clicks, save/load, the
   re-localized member error) + the placeholder-test replacement + full gate.
