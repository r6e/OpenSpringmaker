# Multi-Family GUI Scaffolding — Design (Spec 1a)

**Status:** Approved (design dialogue 2026-06-26)
**Scope:** `springmaker` GUI only. A behavior-preserving refactor — no engine changes, no new
spring family, no user-visible change, no persistence-format change.
**Sub-project:** 1a of the "bring families to the GUI" effort. 1b (extension family GUI, with the
`Family` enum + selector) is a separate spec that builds on this scaffolding.

## Goal

The springmaker GUI is currently compression-only: the Calculator screen's form, presenter, and
view are flat, compression-shaped modules at the crate root (`form.rs`, `view_model.rs`, `view.rs`).
Before a second family can be added, the compression Calculator GUI must be extracted into its own
module and the genuinely family-agnostic presenter vocabulary lifted into a shared module. This spec
does exactly that and nothing more — when it is done, the compression GUI behaves **identically**
(same widgets, same messages, same results, same persistence) and the codebase is shaped so the
extension family (spec 1b) plugs in as a sibling module.

## Non-goals (deferred to spec 1b)

- The `Family` enum (`Compression`/`Extension`/`Torsion`), the family selector widget, and the
  `App`-level family dispatch — they land in 1b, where they become live atomically with the second
  family (introducing them now, with only Compression wired and no selector, would create
  unconstructed enum variants that fail `clippy -D warnings`, forcing `#[allow(dead_code)]`
  scaffolding the project avoids).
- Any change to `SavedDesign` / persistence — the `family` tag is added (additive, defaulted) in 1b.
- Any extension/torsion GUI, engine change, or behavior change of any kind.

## Architecture

### New module layout (`springmaker/src/`)

```
compression/
  mod.rs          declares form, view_model, view as pub(crate) submodules (callers use full paths)
  form.rs         (moved from src/form.rs)        — compression FormState, ScenarioKind, parse/solve
  view_model.rs   (compression-specific presenter functions, moved from src/view_model.rs)
  view.rs         (moved from src/view.rs)         — compression Calculator widget tree
presenter.rs      (new) — family-agnostic presenter DATA vocabulary lifted out of view_model.rs
widgets.rs        (new) — family-agnostic iced WIDGET/STYLE kit lifted out of view.rs
```

`app.rs`, `materials_*`, `settings_*`, `plot.rs`, `main.rs`, `ui_tests.rs` stay at the crate root.
`main.rs` swaps `mod form; mod view_model; mod view;` for `mod compression; mod presenter; mod widgets;`.

> **Amendment (during planning):** `view.rs` turned out to export a cross-screen widget/style
> kit (`panel_container`, `section_heading`, the button styles, `SZ_*`, …) that `settings_view.rs`
> and `materials_view.rs` already consume. Moving `view.rs` into `compression/` would force those
> screens to depend on `compression`, violating strict module boundaries — so that kit is lifted
> into a new shared `widgets.rs`. This adds a third shared module beyond the originally-planned
> two; nothing else about the scope changes.

### The shared/compression split in the presenter

`view_model.rs` today mixes two concerns; the split is by dependency, not by guesswork:

- **Moves to `presenter.rs` (shared, family-agnostic)** — display-vocabulary types that carry no
  compression-specific dependency and that the extension presenter will reuse verbatim: `Emphasis`,
  `ResultRow`, `LoadRow`, `LoadTable`, `StatusKind`, `StatusLine`, `FieldDescriptor`. (The
  implementation plan finalizes the exact per-type list by checking each type's dependencies; the
  binding rule is "no compression-only coupling → shared.")
- **Stays in `compression/view_model.rs` (compression-specific)** — the presenter *functions*
  (`results_view`, `inputs_view`, `status_view`) that read the compression `FormState`, the
  unit-conversion helpers they use, and the compression **result aggregates** they assemble
  (`GoverningRate`, `FatigueView`, `MinWeightView`, `PopulatedResults`, `ResultsView`, `InputsView`).
  These are the compression *results vocabulary*: built from the shared primitives, named for the
  compression result set, and rebuilt per-family in 1b. (`min_weight_view` reads the compression
  `BindingConstraint`; the aggregate types themselves hold only `Vec<ResultRow>` and the other
  shared primitives, but they stay with the functions that produce them.)

The same split applies to the iced widget layer: the cross-screen kit (`panel_container`,
`styled_pick_list`, `text_input_style`, `field_label`, `mono_value`, `section_divider`,
`section_heading`, the four button styles, `SZ_LABEL/BODY/TITLE`) moves to `widgets.rs`; the
compression-only widgets (`KeyLabel`/`END_TYPES`/`FIXITIES`, `styled_text_input`, `calc_field_id`,
`SZ_CAPTION`, `SZ_HERO`, `view`) stay in `compression/view.rs`.

Dependency direction is one-way: `compression/{view_model,view}.rs` depend on `presenter.rs` and
`widgets.rs`; the shared modules depend only on the `app` shell (`Field`, `C`, `Message`), never on
`compression`.

### Call-site updates (mechanical)

- `app.rs::view()`: `Screen::Calculator => crate::view::view(self)` becomes
  `crate::compression::view::view(self)`.
- Every `use crate::form::…` / `crate::view_model::…` / `crate::view::…` across `app.rs`,
  `materials_view.rs`, `settings_view.rs`, and `ui_tests.rs` repoints to
  `crate::compression::{form,view_model,view}::…`, `crate::presenter::…`, or `crate::widgets::…`
  as appropriate. In particular, `settings_view.rs` and `materials_view.rs` repoint their shared
  widget-kit imports to `crate::widgets::…` (never to `crate::compression`).
- The compression presenter unit tests move with their code (they live in `#[cfg(test)] mod tests`
  inside `form.rs`/`view_model.rs`); the `ui_tests.rs` E2E tests stay put and only their `use`
  paths change.

## Data flow

Unchanged. The Calculator screen still flows form (raw strings) → `parse_and_solve` →
`FormOutcome` → presenter functions → view structs → iced widgets → `Message` → `App::update`. Only
the module paths of those pieces change.

## Error handling

No behavior change, so every error path (parse errors, solve errors, fatigue/no-data, dialog
errors) is preserved exactly. The refactor neither adds nor removes a single `Result` or message.

## Testing

The refactor is "done" when the existing suites pass with no behavioral diff — they are the safety
net:
- `springmaker`'s presenter unit tests (currently ~21 in `form.rs`, ~26 in `view_model.rs`) move
  with their code and must all pass unchanged.
- The headless `Simulator` E2E tests in `ui_tests.rs` (which drive the real
  view→message→update loop and click widgets by rendered text) must pass with only `use`-path
  edits — because the rendered widget tree is byte-identical, no test assertion changes.
- Full `cargo test --workspace` green.

No new tests are required (no new behavior); adding any would be over-build for a pure refactor.

## Global constraints

- MSRV 1.88; iced 0.14; dual MIT/Apache.
- ADR 0008 presenter / humble-view split is preserved (this refactor reinforces it — shared
  presenter vocabulary, compression presenter functions, humble compression view).
- `cargo fmt`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `RUSTDOCFLAGS="-D warnings" cargo doc`, repo-wide `typos`, `cargo deny check all` all green; no
  `#[allow(dead_code)]` or other lint-suppression scaffolding.
- No commercial-product/vendor references in any persisted file.
- Behavior-preserving: identical widgets, messages, results, and persistence; `springmaker` is not
  mutation-gated (GUI), so the Simulator E2E + presenter unit tests are the correctness bar.
- Mandatory adversarial multi-agent review panel before push, cycling to convergence.

## Deferred / open items (for spec 1b and beyond)

- `Family` enum + family selector widget + `App` family dispatch.
- `SavedDesign.family` tag (additive, serde-default `Compression`, no schema bump).
- Extension family GUI module (`extension/{form,view_model,view}.rs`) consuming `presenter.rs` and
  `widgets.rs`, including hook inputs, initial tension, the existing curvature-correction toggle, and
  three-stress results.
- Torsion family GUI (a later spec): moment/angle inputs, `FrictionModel` toggle.
- Generalizing any compression-flavored aggregate (e.g. a shared min-weight/binding view) only when
  extension actually needs it — not speculatively here.
