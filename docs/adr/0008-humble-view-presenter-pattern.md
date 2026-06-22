# ADR 0008: Humble-view presenter pattern for all GUI screens

**Status:** Accepted

## Context

The GUI crate (`springmaker`) uses iced 0.13, whose API offers no test
harness for view code (the `Simulator` that would let a test drive widgets and
assert on output arrives only in iced 0.14, which we cannot adopt yet — see
[ADR 0007](0007-accept-transitive-lru-advisory.md), blocked by
`plotters-iced`). As a result, every decision a `view` function makes — which
section to show, which fields a row offers, how a value is converted and
formatted, how a severity maps to a color — was previously expressed inline in
widget-assembly code that no test could reach.

That gap was not theoretical. The materials editor (PR #8) shipped a set of
view-layer decisions (curated rows must be clone-only; endurance/max-temp
fields show only when enabled; an error must take priority over a stale
success) that were correct but guarded only by reviewer vigilance, and the
calculator screen carried the same risk in higher-stakes places: unit
conversion (the surface of the earlier 1000× spring-rate magnitude bug),
fatigue/min-weight section gating, and status-panel suppression and severity
mapping.

PR #10 extracted the materials editor's decisions into a pure presenter
(`materials_view_model`) rendered by a humble `materials_view`, making each
decision a plain function over `App` state that a unit test exercises without a
renderer. That worked, so we are standardizing it.

## Decision

**Every GUI screen separates a pure presenter (view-model) from a humble
view.**

- The **presenter** (a `*_view_model` module — `view_model` for the calculator,
  `materials_view_model` for the editor) is pure: it takes `&App` (or a piece of
  app state) and returns plain data — structs and enums describing *what* to
  show. It has **no iced dependency**. It owns every correctness-bearing
  decision: which mode/section/fields appear, value and unit conversions,
  pre-formatted display strings, and classification enums (e.g. severity →
  `StatusKind`, value emphasis → `Emphasis`).
- The **view** (the matching view module — `view` for the calculator,
  `materials_view` for the editor) is humble: it maps the presenter's data to
  iced widgets and nothing else. It owns only **cosmetic** concerns — colors,
  fonts, widths, spacing, layout — plus the glue iced forces into the view
  (message closures, and binding a `text_input`'s borrowed value from `app`
  state, since iced borrows it for the widget's lifetime).

The dividing principle: **if getting it wrong is a correctness bug, it belongs
in the presenter and gets a test; if it is purely visual, it stays in the
view.**

Presenters are unit-tested hermetically (`App::from_store(...)`, never
`App::default()`, which performs filesystem IO). Tests assert the returned data
directly — no renderer involved.

**Naming:** a presenter entry point is named for the screen area it describes,
suffixed `_view` — `results_view`, `status_view`, `inputs_view`. Only the
entry-point functions and the data types the view consumes are `pub`; helpers
and conversions stay private to the presenter module. (The materials editor,
written before this convention, uses `list_rows` / `feedback` / `edit_panel`;
it is grandfathered, and new screens follow the `_view` form.)

**File layout.** A new screen's pair is `<screen>_view.rs` (humble view) and
`<screen>_view_model.rs` (presenter) — as the materials editor already is
(`materials_view.rs` / `materials_view_model.rs`). The calculator is the
exception: its presenter is `view_model.rs`, and its humble view is `view.rs`,
which also hosts the **shared style toolkit** (`panel_container`,
`section_heading`, `section_divider`, the button/input styles, `mono_value`,
`result_row`) that *every* screen imports — so it keeps a generic name rather
than `calculator_view.rs`. Splitting that toolkit into its own module and
renaming the calculator pair to `calculator_view{,_model}.rs` is a worthwhile
future cleanup, not a blocker for this convention.

This applies to existing screens (calculator `view` / `view_model`, materials
editor `materials_view` / `materials_view_model`) and to every screen added
going forward.

## Consequences

- View decisions become testable today, without waiting for the iced 0.14
  `Simulator`. When that upgrade lands, presenter tests remain valid and
  `Simulator` tests can layer on top for true click-through coverage.
- A small amount of boilerplate per screen: plain-data structs/enums and a
  field→value binding map in the view (because the presenter cannot hold values
  that iced will borrow). Accepted as the cost of testability.
- Two-way reach is forbidden: the presenter must not build widgets, and the
  view must not make decisions. Reviews check this boundary.
- The pattern is recorded in [`docs/REVIEW_CHECKLIST.md`](../REVIEW_CHECKLIST.md);
  new screens that bypass it are a review finding.

## Alternatives considered

- **Wait for iced 0.14 `Simulator`.** Rejected: the upgrade is blocked
  (`plotters-iced`), and even once available, end-to-end widget tests are
  slower and coarser than pure-function assertions on decisions. The presenter
  pattern is complementary, not redundant.
- **Test view functions by inspecting the returned `Element`.** Rejected:
  iced's `Element` is an opaque widget tree with no public structure to assert
  on; there is nothing a test can read back.
- **Leave decisions inline and rely on review.** Rejected: that is the exact
  failure mode this ADR exists to remove — correct-but-untested view logic that
  regresses silently.
