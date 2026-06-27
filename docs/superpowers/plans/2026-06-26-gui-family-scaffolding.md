# Multi-Family GUI Scaffolding Implementation Plan (Spec 1a)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract the compression-only Calculator GUI into a `compression/` module and lift the genuinely family-agnostic vocabulary into shared crate-root modules, with zero behavior change, so a second spring family can be added as a sibling in spec 1b.

**Architecture:** Three extractions, each independently compilable and testable. (1) Move the shared *data* vocabulary (`ResultRow`, `StatusLine`, … 7 types) out of `view_model.rs` into a new iced-free `presenter.rs`. (2) Move the shared *widget/style* kit (`panel_container`, `section_heading`, button styles, `SZ_*`, … 14 items) out of `view.rs` into a new `widgets.rs` — forced because `settings_view.rs` and `materials_view.rs` currently reach into `view.rs` for these helpers and must not depend on `compression`. (3) Move the three compression files (`form.rs`, `view_model.rs`, `view.rs`) into a `compression/` module and rewire every call site. The existing presenter unit tests and headless `Simulator` E2E tests are the correctness net: a behavior-preserving refactor leaves the rendered widget tree byte-identical, so no test assertion changes.

**Tech Stack:** Rust (workspace crate `springmaker`), iced 0.14, `iced_test::Simulator` for E2E.

> **Deviation from spec 1a (discovered during planning):** The spec named two shared
> modules (`compression/` + `presenter.rs`). Planning revealed `view.rs` also exports a
> cross-screen widget/style kit consumed by `settings_view.rs` and `materials_view.rs`.
> Moving `view.rs` into `compression/` without extracting that kit would make the settings
> and materials screens import from `compression`, violating the strict-module-boundaries
> rule. This plan therefore adds a third shared module, `widgets.rs` (Task 2). The spec has
> been amended to match. No other scope change.

## Global Constraints

Copied from the spec; every task's requirements implicitly include these.

- **Behavior-preserving:** identical widgets, messages, results, and persistence. No `Result`, message, widget, or rendered string added or removed. The rendered widget tree must stay byte-identical so `ui_tests.rs` assertions pass unchanged.
- **No persistence-format change:** `SavedDesign`/`settings` serialization is untouched.
- **No `Family` enum, selector, or dispatch** — deferred to spec 1b. Do not add unconstructed enum variants or `#[allow(dead_code)]` scaffolding.
- MSRV 1.88; iced 0.14; dual MIT/Apache.
- ADR 0008 presenter / humble-view split preserved (shared presenter vocabulary, compression presenter functions, humble compression view).
- Gate green before any push: `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`, repo-wide `typos`, `cargo deny check all`, `cargo test --workspace`. No `#[allow(dead_code)]` or other lint suppression.
- No commercial-product/vendor references in any persisted file (academic/standard citations are fine).
- Moves are **verbatim**: when a step says "move item X", the body is transplanted unchanged. Only module location, `use` paths, and the explicitly-named visibility widenings change. Do not refactor, rename, reformat, or "improve" moved code.

## File Structure (end state, `springmaker/src/`)

| File | Responsibility | Change |
| --- | --- | --- |
| `presenter.rs` | Family-agnostic **data** vocabulary the view renders: `Emphasis`, `ResultRow`, `LoadRow`, `LoadTable`, `StatusKind`, `StatusLine`, `FieldDescriptor`. iced-free. Depends only on `crate::app::Field`. | **New** (Task 1) |
| `widgets.rs` | Family-agnostic **iced widget/style** kit shared across screens: `SZ_LABEL/BODY/TITLE`, `panel_container`, `styled_pick_list`, `text_input_style`, `field_label`, `mono_value`, `section_divider`, `section_heading`, `ghost_/danger_/accent_/nav_button_style`. Depends only on `crate::app::{C, Message}` + iced. | **New** (Task 2) |
| `compression/mod.rs` | Declares the compression submodules. | **New** (Task 3) |
| `compression/form.rs` | Compression form state, scenario parsing, solve (`FormState`, `ScenarioKind`, `build_spec`, `parse_and_solve`, …). | **Moved** from `form.rs` (Task 3), unchanged |
| `compression/view_model.rs` | Compression presenter **functions** (`results_view`, `status_view`, `inputs_view`) + compression result aggregates (`GoverningRate`, `FatigueView`, `MinWeightView`, `PopulatedResults`, `ResultsView`, `InputsView`) + unit-conversion helpers. | **Moved** from `view_model.rs`, minus the 7 shared types (Task 1) |
| `compression/view.rs` | Compression humble view: `view()`, `calc_field_id`, `KeyLabel`/`END_TYPES`/`FIXITIES`/`find_by_key`, `styled_text_input`, `SZ_CAPTION`, `SZ_HERO`. | **Moved** from `view.rs`, minus the 14 shared widget items (Task 2) |
| `app.rs` | Shell: `App`, `Message`, `Field`, `C`, `Screen`, `update`, `view` dispatch. | Import + dispatch repoint (Task 3) |
| `main.rs` | Module declarations + bootstrap. | `mod` list swapped (Tasks 1–3) |
| `materials_view.rs`, `settings_view.rs` | Materials/settings screens. Stay at crate root. | Widget-kit import repoint `crate::view::` → `crate::widgets::` (Task 2) |
| `ui_tests.rs` | Headless Simulator E2E. Stays at crate root. | `crate::view::calc_field_id` → `crate::compression::view::calc_field_id` (Task 3) |

---

### Task 1: Extract shared data vocabulary into `presenter.rs`

Move the 7 family-agnostic display-data types out of `view_model.rs` into a new iced-free `presenter.rs` at the crate root. `view_model.rs` stays at the crate root for now (it moves in Task 3); it imports the moved types from `presenter`. The constructors `ResultRow::{new, danger}` and `FieldDescriptor::new` (currently module-private) become `pub(crate)` so the presenter functions, which remain in `view_model.rs`, can still call them across the module boundary.

**Files:**
- Create: `springmaker/src/presenter.rs`
- Modify: `springmaker/src/main.rs` (add `mod presenter;`)
- Modify: `springmaker/src/view_model.rs` (remove the 7 types + their `impl` blocks; add `use crate::presenter::{…};`)
- Modify: `springmaker/src/view.rs` (split its `use crate::view_model::{…}` so the 7 moved types come from `crate::presenter`)
- Test: existing tests in `springmaker/src/view_model.rs` (`#[cfg(test)] mod tests`) — must pass unchanged

**Interfaces:**
- Produces (in `crate::presenter`): `Emphasis` (enum `Normal`/`Danger`), `ResultRow { label, value, unit: String, emphasis: Emphasis }` with `pub(crate) fn new(label, value, unit)` and `pub(crate) fn danger(label, value, unit)`, `LoadRow { point, force, deflection, length, stress, pct_mts: String }`, `LoadTable { stress_unit: String, rows: Vec<LoadRow> }`, `StatusKind` (enum `ActionError`/`LoadWarning`/`Info`/`Caution`/`DesignWarning`), `StatusLine { kind: StatusKind, text: String }`, `FieldDescriptor { label: String, field: crate::app::Field }` with `pub(crate) fn new(label, field)`.
- Consumes: `crate::app::Field` (for `FieldDescriptor`).

- [ ] **Step 1: Run the baseline tests so you know they pass before the move**

Run: `cargo test --workspace`
Expected: PASS (this is the green baseline the refactor must preserve).

- [ ] **Step 2: Create `presenter.rs` with the 7 moved types**

Create `springmaker/src/presenter.rs`. Transplant **verbatim** from `view_model.rs`: the `Emphasis` enum (currently ~lines 88–93), the `ResultRow` struct **and** its `impl ResultRow` block (~95–120), `LoadRow` (~129–138), `LoadTable` (~140–145), the `StatusKind` enum **and** its doc comments (~344–355), `StatusLine` (~357–362), the `FieldDescriptor` struct **and** its `impl FieldDescriptor` block (~405–419). Keep all derives and doc comments exactly. Add the module header and the one import:

```rust
//! Family-agnostic presenter vocabulary: the plain-data types a humble view
//! renders. iced-free, so every type is unit-testable without a renderer and
//! reusable by any spring family's presenter. Family-specific presenter
//! functions and result aggregates live in each family's `view_model`.

use crate::app::Field;
```

Change the two constructor `impl` blocks so the methods are `pub(crate)`:

```rust
impl ResultRow {
    pub(crate) fn new(label: impl Into<String>, value: impl Into<String>, unit: impl Into<String>) -> Self {
        Self { label: label.into(), value: value.into(), unit: unit.into(), emphasis: Emphasis::Normal }
    }

    pub(crate) fn danger(label: impl Into<String>, value: impl Into<String>, unit: impl Into<String>) -> Self {
        Self { emphasis: Emphasis::Danger, ..Self::new(label, value, unit) }
    }
}
```
```rust
impl FieldDescriptor {
    pub(crate) fn new(label: impl Into<String>, field: Field) -> Self {
        Self { label: label.into(), field }
    }
}
```

- [ ] **Step 3: Register the module**

In `springmaker/src/main.rs`, add `mod presenter;` in alphabetical position (between `mod plot;` and `mod settings;`):

```rust
mod plot;
mod presenter;
mod settings;
```

- [ ] **Step 4: Remove the moved types from `view_model.rs` and import them instead**

Delete the 7 type definitions and their two `impl` blocks from `view_model.rs`. At the top of `view_model.rs`, below the existing `use crate::form::…;` line, add:

```rust
use crate::presenter::{
    Emphasis, FieldDescriptor, LoadRow, LoadTable, ResultRow, StatusKind, StatusLine,
};
```

The `#[cfg(test)] mod tests` block uses `use super::*;`, which re-exports these module-level imports into the test module — so the tests reference `Emphasis`, `StatusKind`, `FieldDescriptor`, etc. with no change. Do not touch the test module.

- [ ] **Step 5: Repoint `view.rs`'s import**

`view.rs` currently has one `use crate::view_model::{ inputs_view, results_view, status_view, Emphasis, FatigueView, FieldDescriptor, GoverningRate, LoadTable, MinWeightView, PopulatedResults, ResultRow, ResultsView, StatusKind, StatusLine };`. Split it: the 7 moved types come from `presenter`; the rest stay in `view_model`:

```rust
use crate::presenter::{
    Emphasis, FieldDescriptor, LoadTable, ResultRow, StatusKind, StatusLine,
};
use crate::view_model::{
    inputs_view, results_view, status_view, FatigueView, GoverningRate, MinWeightView,
    PopulatedResults, ResultsView,
};
```

(`LoadRow` is not named in `view.rs` — it is reached through `LoadTable.rows` field access, which needs no import.)

- [ ] **Step 6: Verify it compiles and all tests pass**

Run: `cargo test --workspace`
Expected: PASS — identical test count and results to Step 1. If `clippy` later flags an unused import in the split `use` blocks, remove only the genuinely-unused name.

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add springmaker/src/presenter.rs springmaker/src/main.rs springmaker/src/view_model.rs springmaker/src/view.rs
git commit -m "refactor(gui): extract shared presenter data vocabulary into presenter.rs"
```

---

### Task 2: Extract shared widget/style kit into `widgets.rs`

Move the 14 cross-screen iced helpers out of `view.rs` into a new `widgets.rs` at the crate root, then repoint `settings_view.rs` and `materials_view.rs` (and `view.rs` itself) to import them from `crate::widgets`. This removes the boundary violation where the settings and materials screens reach into the calculator view for shared styling. `view.rs` stays at the crate root for now (it moves in Task 3). Compression-only helpers (`calc_field_id`, `KeyLabel`/`END_TYPES`/`FIXITIES`/`find_by_key`, `styled_text_input`, `SZ_CAPTION`, `SZ_HERO`, `view`) stay in `view.rs`.

**Files:**
- Create: `springmaker/src/widgets.rs`
- Modify: `springmaker/src/main.rs` (add `mod widgets;`)
- Modify: `springmaker/src/view.rs` (remove the 14 items; add `use crate::widgets::{…};`)
- Modify: `springmaker/src/settings_view.rs` (`use crate::view::{…}` → `use crate::widgets::{…}`)
- Modify: `springmaker/src/materials_view.rs` (`use crate::view::{…}` → `use crate::widgets::{…}`)
- Test: existing presenter/Simulator tests — must pass unchanged

**Interfaces:**
- Produces (in `crate::widgets`, all `pub(crate)`): consts `SZ_LABEL: u32 = 13`, `SZ_BODY: u32 = 14`, `SZ_TITLE: u32 = 18`; fns `panel_container`, `styled_pick_list`, `text_input_style`, `field_label`, `mono_value`, `section_divider`, `section_heading`, `ghost_button_style`, `danger_button_style`, `accent_button_style`, `nav_button_style` (signatures identical to their current `view.rs` definitions).
- Consumes: `crate::app::{C, Message}` and iced widget/type imports.

- [ ] **Step 1: Create `widgets.rs` and transplant the 14 shared items verbatim**

Create `springmaker/src/widgets.rs`. Move **verbatim** from `view.rs`: the three `pub(crate) const SZ_LABEL/SZ_BODY/SZ_TITLE` (~lines 24–26) and the `pub(crate) fn` bodies for `panel_container`, `styled_pick_list`, `text_input_style`, `field_label`, `mono_value`, `section_divider`, `section_heading`, `ghost_button_style`, `danger_button_style`, `accent_button_style`, `nav_button_style`. Keep every attribute, generic bound, and doc comment. Add the module header and the imports these bodies need (derive the exact iced import set from the moved bodies — they reference `container`, `pick_list`, `text`, the `text_input` style type, `button`, `Background`, `Border`, `Color`, `Length`, `Font`, `Element`, and `crate::app::{C, Message}`):

```rust
//! Shared iced widget and style kit used by every screen (calculator, materials,
//! settings). Family- and screen-agnostic presentational helpers; depends only on
//! the app shell's color palette (`C`) and `Message`. Screen-specific widgets live
//! in that screen's own view module.
```

- [ ] **Step 2: Register the module**

In `springmaker/src/main.rs`, add `mod widgets;` in alphabetical position (after `mod settings_view_model;`, before `#[cfg(test)] mod ui_tests;`):

```rust
mod settings_view_model;
mod widgets;

#[cfg(test)]
mod ui_tests;
```

- [ ] **Step 3: Remove the moved items from `view.rs` and import them**

Delete the 14 moved definitions from `view.rs`. Add an import for the subset `view.rs` actually uses (let the compiler tell you which — `view.rs` renders the calculator, so it needs at least `SZ_BODY`, `SZ_LABEL`, `SZ_TITLE`, `panel_container`, `styled_pick_list`, `text_input_style`, `field_label`, `mono_value`, `section_divider`, `section_heading`, `nav_button_style`, and any button style it renders):

```rust
use crate::widgets::{
    accent_button_style, danger_button_style, field_label, ghost_button_style, mono_value,
    nav_button_style, panel_container, section_divider, section_heading, styled_pick_list,
    text_input_style, SZ_BODY, SZ_LABEL, SZ_TITLE,
};
```

Keep `SZ_CAPTION`, `SZ_HERO`, `KeyLabel`, `END_TYPES`, `FIXITIES`, `find_by_key`, `styled_text_input`, `calc_field_id`, and `view` in `view.rs`. `styled_text_input` calls `text_input_style` (now imported) and `calc_field_id` (local) — both resolve.

- [ ] **Step 4: Repoint `settings_view.rs`**

Change its import from `crate::view` to `crate::widgets` (same names):

```rust
use crate::widgets::{
    nav_button_style, panel_container, section_divider, section_heading, SZ_BODY, SZ_LABEL,
    SZ_TITLE,
};
```

- [ ] **Step 5: Repoint `materials_view.rs`**

Change its import from `crate::view` to `crate::widgets` (same names):

```rust
use crate::widgets::{
    accent_button_style, danger_button_style, field_label, ghost_button_style, mono_value,
    nav_button_style, panel_container, section_divider, section_heading, styled_pick_list,
    text_input_style, SZ_BODY, SZ_LABEL, SZ_TITLE,
};
```

- [ ] **Step 6: Verify compile, lint, and tests**

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean (no unused imports, no boundary errors).

Run: `cargo test --workspace`
Expected: PASS — identical results to Task 1's green state.

- [ ] **Step 7: Commit**

```bash
git add springmaker/src/widgets.rs springmaker/src/main.rs springmaker/src/view.rs springmaker/src/settings_view.rs springmaker/src/materials_view.rs
git commit -m "refactor(gui): extract shared widget/style kit into widgets.rs"
```

---

### Task 3: Move the compression trio into a `compression/` module and rewire call sites

Relocate `form.rs`, `view_model.rs`, and `view.rs` into a new `compression/` module, add `compression/mod.rs`, swap the `main.rs` declarations, and repoint every remaining `crate::form::` / `crate::view_model::` / `crate::view::` reference. `presenter.rs` and `widgets.rs` stay at the crate root, so their import paths do **not** change.

**Files:**
- Create: `springmaker/src/compression/mod.rs`
- Move (git mv): `springmaker/src/form.rs` → `springmaker/src/compression/form.rs`; `springmaker/src/view_model.rs` → `springmaker/src/compression/view_model.rs`; `springmaker/src/view.rs` → `springmaker/src/compression/view.rs`
- Modify: `springmaker/src/main.rs` (remove `mod form; mod view; mod view_model;`, add `mod compression;`)
- Modify: `springmaker/src/compression/view_model.rs` and `compression/view.rs` (intra-module `crate::form`/`crate::view_model` paths → `crate::compression::…`)
- Modify: `springmaker/src/app.rs` (5 `crate::form::` refs → `crate::compression::form::`; dispatch `crate::view::view` → `crate::compression::view::view`)
- Modify: `springmaker/src/ui_tests.rs` (`crate::view::calc_field_id` → `crate::compression::view::calc_field_id`)
- Test: full `cargo test --workspace`, including `ui_tests.rs` Simulator E2E

**Interfaces:**
- Produces: `crate::compression::form::{…}`, `crate::compression::view_model::{…}`, `crate::compression::view::{view, calc_field_id}` — same public items as before, new paths.
- Consumes: `crate::presenter::{…}` and `crate::widgets::{…}` (unchanged paths from Tasks 1–2), `crate::app::{App, Field, Message, C}`.

- [ ] **Step 1: Move the three files with git (preserves history)**

```bash
cd springmaker/src
mkdir compression
git mv form.rs compression/form.rs
git mv view_model.rs compression/view_model.rs
git mv view.rs compression/view.rs
cd ../..
```

- [ ] **Step 2: Create `compression/mod.rs`**

Create `springmaker/src/compression/mod.rs`:

```rust
//! Compression-spring calculator screen: form (input parsing and solving),
//! view-model (the compression presenter functions and result aggregates), and
//! view (the humble iced widget tree). Mirrors the engine's per-family layout;
//! shared vocabulary lives in `crate::presenter` and `crate::widgets`.

pub(crate) mod form;
pub(crate) mod view;
pub(crate) mod view_model;
```

- [ ] **Step 3: Swap the `main.rs` module declarations**

In `springmaker/src/main.rs`, remove `mod form;`, `mod view;`, `mod view_model;` and add `mod compression;` (alphabetical, after `mod app;`). End state:

```rust
mod app;
mod compression;
mod materials_form;
mod materials_view;
mod materials_view_model;
mod plot;
mod presenter;
mod settings;
mod settings_view;
mod settings_view_model;
mod widgets;

#[cfg(test)]
mod ui_tests;
```

- [ ] **Step 4: Repoint intra-module paths inside the moved files**

In `compression/view_model.rs`: change `use crate::form::{FatigueStatus, FormOutcome, ScenarioKind};` → `use crate::compression::form::{FatigueStatus, FormOutcome, ScenarioKind};`, and in its test module `use crate::form::FormState;` → `use crate::compression::form::FormState;`. (`use crate::app::{App, Field};` and `use crate::presenter::{…};` are unchanged.)

In `compression/view.rs`: change `use crate::form::ALL_SCENARIOS;` → `use crate::compression::form::ALL_SCENARIOS;`, `use crate::view_model::{…};` → `use crate::compression::view_model::{…};`, and the inline `fn field_value(form: &crate::form::FormState, …)` → `&crate::compression::form::FormState`. (`use crate::widgets::{…};` and `use crate::presenter::{…};` are unchanged.)

- [ ] **Step 5: Repoint `app.rs`**

In `springmaker/src/app.rs`:
- Line ~3: `use crate::form::{format_error, parse_and_solve, FormOutcome, FormState, ScenarioKind};` → `use crate::compression::form::{format_error, parse_and_solve, FormOutcome, FormState, ScenarioKind};`
- Dispatch (~516): `Screen::Calculator => crate::view::view(self),` → `Screen::Calculator => crate::compression::view::view(self),`
- `crate::form::build_spec` (~598) → `crate::compression::form::build_spec`
- `crate::form::populate_from_spec` (~646) → `crate::compression::form::populate_from_spec`
- The four `crate::form::ScenarioKind` refs in app.rs tests (~733, 771, 785, 795) → `crate::compression::form::ScenarioKind`

- [ ] **Step 6: Repoint `ui_tests.rs`**

In `springmaker/src/ui_tests.rs` (~line 60): `crate::view::calc_field_id(field)` → `crate::compression::view::calc_field_id(field)`.

- [ ] **Step 7: Verify the full gate**

Run: `rg 'crate::(form|view_model|view)::' springmaker/src`
Expected: no matches (every reference repointed).

Run: `cargo test --workspace`
Expected: PASS — same test count and results as before the refactor; `ui_tests.rs` Simulator E2E green (proves the rendered widget tree is unchanged).

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings && RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add -A springmaker/src
git commit -m "refactor(gui): move compression calculator into compression/ module"
```

---

## Final verification (before review/push)

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] `cargo test --workspace` green (test count unchanged from the pre-refactor baseline)
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` clean
- [ ] `typos` clean; `cargo deny check all` clean
- [ ] `rg 'crate::(form|view_model|view)::' springmaker/src` returns nothing
- [ ] No `#[allow(dead_code)]` or new lint suppressions added; no `Family` enum introduced
- [ ] Mandatory adversarial multi-agent review panel run to convergence (general, architect, simplifier, + an input-domain/boundary lens for module-boundary correctness), then push with permission

## Self-Review (plan vs. spec)

1. **Spec coverage:** compression/ module → Task 3; presenter.rs shared data vocabulary → Task 1; compression presenter functions stay in compression/view_model.rs → Tasks 1 & 3; app.rs Calculator dispatch → Task 3 Step 5; main.rs mod swap → Tasks 1–3; ui_tests import update → Task 3 Step 6; behavior-preserving (tests pass unchanged) → every task's verify step. The `widgets.rs` extraction (Task 2) is the planned, spec-amended addition for the cross-screen kit the spec's "call-site updates" section implied but mistargeted. `Family` enum / persistence tag explicitly **not** built. No gaps.
2. **Placeholder scan:** none — every step names exact files, items, paths, and commands; moved bodies are "verbatim transplant" with the new glue (imports, mod lines, visibility) shown in full.
3. **Type consistency:** the 7 presenter types and their `pub(crate)` constructors are named identically in Task 1's Produces block, the `view_model.rs`/`view.rs` import lists, and Task 3's path repoints; the 14 widget items match across Task 2's Produces block and the three consumer import lists; `crate::compression::{form,view_model,view}` paths are consistent across mod.rs, app.rs, ui_tests.rs, and the intra-module repoints.
