# Assembly GUI Family Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The sixth GUI family tab — assemblies — with a dynamic member-list form, per-member material pickers, member-error re-localization, and the topology-rejection pin end-to-end.

**Architecture:** Task 1 lands the springcore surface (`Family::Assembly` + the `SpringError::Member` variant + `format_error` recursion), the full form layer with the dynamic member list, all app dispatch arms, and a minimal Empty/Error results skeleton. Task 2 fleshes the presenter (assembly + per-member result sections), the full view, and E2E with real Simulator clicks on runtime-indexed widget ids.

**Tech Stack:** Rust workspace — springcore (mutation-gated) + springmaker (iced 0.14, ADR 0008).

## Global Constraints

- springcore mutation-gated: `cargo mutants --in-diff` vs origin/main ends `0 missed`. springmaker NOT gated.
- Strict TDD; every message/string quoted here is VERBATIM (especially the `Member` Display byte-identity and `format_error` output).
- NO references to the commercial inspiration product/vendor (tooling trailers exempt).
- MSRV 1.88; fmt zero deviation; clippy `-D warnings`; ADR 0008 (no iced imports in form.rs/view_model.rs).
- Commit DIRECTLY on `feat/gui-assembly` — NOT a side branch (verify `git branch --show-current` before the first commit; state it in the report). NEVER push/PR/panel/marker; NEVER touch `.git/`.
- Conventional commits; trailer `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.
- Do NOT change `AssemblySpec`/`AssemblyMemberSpec` (persisted format is frozen). Spec: docs/superpowers/specs/2026-07-07-gui-assembly-design.md (Decisions 1-5 bind).

---

### Task 1: springcore surface + form layer + app wiring (+ minimal skeleton)

**Files:**
- Modify: `springcore/src/family.rs` (variant + ALL + Display + test)
- Modify: `springcore/src/error.rs` (the `Member` variant + Display)
- Modify: `springcore/src/assembly/design.rs` (`member_error` + the `msg()` test-helper arm)
- Modify: `springmaker/src/form_helpers.rs` (`format_error` recursion)
- Modify: `springmaker/src/widgets.rs` (widen `labeled_input`'s `id` bound; `material_picker_for` variant)
- Modify: `springmaker/src/picker.rs` (hoist `FIXITIES`)
- Modify: `springmaker/src/compression/view.rs` (drop local `FIXITIES`, import from picker)
- Create: `springmaker/src/assembly/mod.rs`, `form.rs`, `view.rs` (skeleton), `view_model.rs` (skeleton)
- Modify: springmaker module root (`mod assembly;` beside `mod conical;`)
- Modify: `springmaker/src/app.rs` (state, messages, dispatch arms, apply_saved, placeholder-test replacement)
- Modify: `springmaker/src/calculator.rs` (two Family arms)

**Interfaces produced (Task 2 relies on these exact names):** `AsmFormState`, `AsmMemberForm`, `MemberField`, `parse_and_solve(form, us, materials, correction) -> Result<AssemblyDesign>`, `build_spec`, `populate_from_spec`, `AsmFormState::is_blank`; `App.assembly`/`App.asm_outcome: Option<AssemblyDesign>`; `Message::Asm*`; skeleton `assembly::view::{design_panel, results_panel, asm_member_field_id}` and `assembly::view_model::{AsmResultsView, asm_results_view, asm_status_view}`; the hoisted `picker::FIXITIES`.

- [ ] **Step 1: `Family::Assembly` (TDD)**

Add the failing test to `springcore/src/family.rs` tests (mirror `torsion_display_and_in_all_families`):

```rust
    #[test]
    fn assembly_display_and_in_all_families() {
        assert_eq!(Family::Assembly.to_string(), "Assembly");
        assert!(ALL_FAMILIES.contains(&Family::Assembly));
    }
```

Run `cargo test -p springcore family` → FAIL. Implement: `Assembly` after `Conical` in the enum, `Family::Assembly => "Assembly"` in Display, and `Family::Assembly` appended to `ALL_FAMILIES`. (springmaker will not compile until Step 8; run only `cargo test -p springcore` this step.)

- [ ] **Step 2: `SpringError::Member` variant (TDD)**

Add to `springcore/src/error.rs` tests:

```rust
    #[test]
    fn member_display_is_byte_identical_to_the_old_flatten() {
        // InconsistentInputs source: the RAW inner message, no doubled prefix.
        let e = SpringError::Member {
            index: 1,
            source: Box::new(SpringError::InconsistentInputs(
                "mean diameter must be greater than wire diameter".into(),
            )),
        };
        assert_eq!(
            e.to_string(),
            "member 2: mean diameter must be greater than wire diameter"
        );
        // Non-InconsistentInputs source flattens via its own Display.
        let e = SpringError::Member {
            index: 0,
            source: Box::new(SpringError::MaterialNotFound("Unobtainium".into())),
        };
        assert_eq!(e.to_string(), "member 1: material not found: Unobtainium");
    }
```

Run → FAIL (variant missing). Implement — add the variant after `DataFile`:

```rust
    /// A member-scoped error from an assembly solve. Preserves the underlying
    /// error's structure (so a UI layer can re-localize it, e.g. a member's
    /// `DiameterOutOfRange` in the active unit system) plus the 1-based member
    /// attribution.
    Member {
        index: usize,
        source: Box<SpringError>,
    },
```

and the Display arm (byte-identical to the pre-existing `member_error` flatten — the `InconsistentInputs` source contributes its RAW inner string, not its own `Display`):

```rust
            Self::Member { index, source } => {
                let inner = match source.as_ref() {
                    SpringError::InconsistentInputs(m) => m.clone(),
                    other => other.to_string(),
                };
                write!(f, "member {}: {inner}", index + 1)
            }
```

- [ ] **Step 3: `member_error` returns the variant + engine test update (TDD-adjacent)**

In `springcore/src/assembly/design.rs`, replace `member_error` (currently flattens to `InconsistentInputs`):

```rust
/// Wrap a member-level error with its 1-based attribution, preserving the
/// inner error's structure (a UI layer re-localizes `DiameterOutOfRange`).
/// The `Member` variant's `Display` reproduces the previous flattened string
/// byte-for-byte, so error *messages* are unchanged; only the *structure* is
/// richer.
fn member_error(index: usize, err: SpringError) -> SpringError {
    SpringError::Member {
        index,
        source: Box::new(err),
    }
}
```

The assembly test module's `msg()` helper currently matches only `InconsistentInputs` and would panic on a member error. Extend it to flatten a `Member` too (so `member_errors_carry_the_member_prefix`'s asserted strings stay UNCHANGED):

```rust
    fn msg(result: crate::Result<AssemblyDesign>) -> String {
        match result {
            Err(crate::SpringError::InconsistentInputs(m)) => m,
            Err(crate::SpringError::Member { index, source }) => {
                let inner = match *source {
                    crate::SpringError::InconsistentInputs(m) => m,
                    other => other.to_string(),
                };
                format!("member {}: {inner}", index + 1)
            }
            other => panic!("expected InconsistentInputs or Member, got {other:?}"),
        }
    }
```

Run `cargo test -p springcore assembly` → the existing `member_errors_carry_the_member_prefix` and `guards_pin_messages` still PASS (byte-identical strings; assembly-level guards remain `InconsistentInputs`). Add a test pinning the STRUCTURE (so a future regression flattening back to `InconsistentInputs` is caught):

```rust
    #[test]
    fn member_error_preserves_structure_for_relocalization() {
        // A member DiameterOutOfRange must arrive as Member{ DiameterOutOfRange },
        // not a flattened InconsistentInputs — the GUI relies on the structure.
        let fat = AssemblyMember {
            wire_dia: Length::from_millimeters(10.0),
            mean_dia: Length::from_millimeters(80.0),
            free_length: Length::from_millimeters(200.0),
            ..baseline_member()
        };
        let err = solve(Topology::Series, vec![fat], &[10.0]).unwrap_err();
        assert!(matches!(
            err,
            crate::SpringError::Member { index: 0, ref source }
                if matches!(**source, crate::SpringError::DiameterOutOfRange { .. })
        ));
    }
```

- [ ] **Step 4: `format_error` recursion (TDD)**

`springmaker/src/form_helpers.rs` tests (mirror the file's conventions):

```rust
    #[test]
    fn format_error_relocalizes_a_member_diameter_error() {
        let inner = SpringError::DiameterOutOfRange {
            diameter_m: 0.010,
            min_m: 0.0002,
            max_m: 0.0064,
        };
        let e = SpringError::Member { index: 0, source: Box::new(inner) };
        // US: inches, member-prefixed.
        let us = format_error(&e, UnitSystem::Us);
        assert!(us.starts_with("member 1: wire diameter") && us.contains(" in "), "got: {us}");
        // Metric: mm.
        let m = format_error(&e, UnitSystem::Metric);
        assert!(m.starts_with("member 1: wire diameter") && m.contains(" mm "), "got: {m}");
        // An InconsistentInputs member source: raw message, no doubled prefix.
        let e = SpringError::Member {
            index: 1,
            source: Box::new(SpringError::InconsistentInputs("mean diameter must be greater".into())),
        };
        assert_eq!(format_error(&e, UnitSystem::Us), "member 2: mean diameter must be greater");
    }
```

Run → FAIL. Implement — add the `Member` arm BEFORE the `other =>` fallthrough in `format_error`:

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

- [ ] **Step 5: Commit the springcore + error surface**

```bash
git add springcore/src/family.rs springcore/src/error.rs springcore/src/assembly/design.rs springmaker/src/form_helpers.rs
git commit -m "feat(springcore): Family::Assembly + structured SpringError::Member for member re-localization

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

(springmaker may not fully compile until the app arms land — the `format_error` change alone compiles; the `Family::Assembly` breakage is resolved in Step 8. If `cargo build -p springmaker` fails here solely on the exhaustive `Family` matches, that is expected; commit anyway — the workspace gate runs at Step 9.)

- [ ] **Step 6: hoist `FIXITIES` + widen `labeled_input`**

In `springmaker/src/picker.rs`, add after `END_TYPES`:

```rust
/// All end-fixity options in display order (buckling boundary condition).
pub(crate) const FIXITIES: &[KeyLabel] = &[
    KeyLabel { key: "fixed_fixed", label: "Fixed-Fixed" },
    KeyLabel { key: "fixed_pinned", label: "Fixed-Pinned" },
    KeyLabel { key: "pinned_pinned", label: "Pinned-Pinned" },
    KeyLabel { key: "fixed_free", label: "Fixed-Free" },
];

/// Assembly topologies (keys match `springcore::parse_topology`).
pub(crate) const TOPOLOGIES: &[KeyLabel] = &[
    KeyLabel { key: "nested", label: "Nested" },
    KeyLabel { key: "series", label: "Series" },
];
```

In `springmaker/src/compression/view.rs`: delete the local `const FIXITIES` (lines ~27-44) and add `FIXITIES` to the `use crate::picker::{…}` import. Run `cargo test -p springmaker compression` (once Step 8 makes the crate compile) to confirm the compression fixity picker is unchanged.

Widen `labeled_input`'s `id` param so member fields can pass a runtime `String`. `iced::widget::text_input::Id` has `From<&'static str>` AND `From<String>`, so `impl Into<text_input::Id>` accepts both — existing `&'static str` callers are unchanged:

```rust
pub(crate) fn labeled_input<'a>(
    label: &str,
    value: &str,
    id: impl Into<iced::widget::text_input::Id>,
    on_input: impl Fn(String) -> Message + 'a,
) -> Element<'a, Message> {
    column![
        field_label(label),
        text_input("", value)
            .id(id.into())
            .on_input(on_input)
            .size(SZ_BODY)
            .font(Font::MONOSPACE)
            .style(text_input_style),
    ]
    .spacing(4)
    .into()
}
```

(Verify the `text_input` import path in widgets.rs; adjust `iced::widget::text_input::Id` to the crate's actual alias if it re-imports `text_input` directly.)

Add a per-member material picker beside `material_picker` in `widgets.rs`:

```rust
/// A material pick-list bound to member `index`, emitting `AsmMemberMaterial`.
pub(crate) fn material_picker_for_member(app: &App, index: usize) -> Element<'_, Message> {
    let names: Vec<String> = app.materials.names().into_iter().map(String::from).collect();
    let selected = app.assembly.members.get(index).map(|m| m.material.clone());
    column![
        field_label("Material"),
        styled_pick_list(names, selected, move |m| Message::AsmMemberMaterial(index, m)),
    ]
    .spacing(4)
    .into()
}
```

- [ ] **Step 7: the form layer (TDD — `springmaker/src/assembly/form.rs`)**

Create `springmaker/src/assembly/mod.rs`:

```rust
//! Assembly compression-spring family — GUI layer (nested/series, dynamic
//! member list). Humble view / pure presenter per ADR 0008.

pub mod form;
pub mod view;
pub mod view_model;
```

Declare `mod assembly;` beside `mod conical;` in the springmaker module root.

Write `form.rs` — test module first (red), then implementation:

```rust
//! Assembly form state, dynamic member list, parsing, and solve routing.
//! iced-free per ADR 0008.

use springcore::assembly::{solve_assembly, AssemblyDesign, AssemblyInputs, AssemblyMember};
use springcore::units::{Force, Length};
use springcore::{
    parse_end_type, parse_fixity, parse_topology, AssemblyMemberSpec, AssemblySpec,
    CurvatureCorrection, MaterialSet, Result, UnitSystem,
};

use crate::form_helpers::{length_mm, loads_n, positive_num};

/// One member's editable text field.
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
    pub material: String,
    pub end_type: String,
    pub wire_dia: String,
    pub mean_dia: String,
    pub active: String,
    pub free_length: String,
}

impl AsmMemberForm {
    /// A blank member card seeded with the given default material.
    pub fn blank(default_material: &str) -> Self {
        Self {
            material: default_material.to_string(),
            end_type: "squared_ground".into(),
            wire_dia: String::new(),
            mean_dia: String::new(),
            active: String::new(),
            free_length: String::new(),
        }
    }

    fn is_blank(&self) -> bool {
        [&self.wire_dia, &self.mean_dia, &self.active, &self.free_length]
            .iter()
            .all(|f| f.trim().is_empty())
    }
}

/// Assembly form state.
#[derive(Debug, Clone)]
pub struct AsmFormState {
    pub topology: String,
    pub fixity: String,
    pub loads: String,
    pub members: Vec<AsmMemberForm>,
}

impl AsmFormState {
    /// A fresh form opens with one blank member (the min-one floor).
    pub fn with_default_material(default_material: &str) -> Self {
        Self {
            topology: "nested".into(),
            fixity: "fixed_fixed".into(),
            loads: String::new(),
            members: vec![AsmMemberForm::blank(default_material)],
        }
    }

    pub fn is_blank(&self) -> bool {
        self.loads.trim().is_empty() && self.members.iter().all(AsmMemberForm::is_blank)
    }
}

/// Parse the whole form and solve. Wires `parse_topology`/`parse_fixity`
/// (the topology-rejection pin lands here) and threads the app-global
/// curvature correction (the compression pattern).
pub fn parse_and_solve(
    form: &AsmFormState,
    us: UnitSystem,
    materials: &MaterialSet,
    correction: CurvatureCorrection,
) -> Result<AssemblyDesign> {
    let topology = parse_topology(&form.topology)?;
    let fixity = parse_fixity(&form.fixity)?;
    let mut members = Vec::with_capacity(form.members.len());
    for m in &form.members {
        members.push(AssemblyMember {
            material_name: m.material.clone(),
            wire_dia: Length::from_millimeters(length_mm("wire diameter", &m.wire_dia, us)?),
            mean_dia: Length::from_millimeters(length_mm("mean diameter", &m.mean_dia, us)?),
            active_coils: positive_num("active coils", &m.active)?,
            free_length: Length::from_millimeters(length_mm("free length", &m.free_length, us)?),
            end_type: parse_end_type(&m.end_type)?,
        });
    }
    let loads: Vec<Force> = loads_n(&form.loads, us)?.into_iter().map(Force::from_newtons).collect();
    solve_assembly(materials, &AssemblyInputs { topology, members }, &loads, fixity, correction)
}

/// Build the persisted spec from the form.
pub fn build_spec(form: &AsmFormState, us: UnitSystem) -> Result<AssemblySpec> {
    let members = form
        .members
        .iter()
        .map(|m| {
            Ok(AssemblyMemberSpec {
                material_name: m.material.clone(),
                end_type: m.end_type.clone(),
                wire_dia_mm: length_mm("wire diameter", &m.wire_dia, us)?,
                mean_dia_mm: length_mm("mean diameter", &m.mean_dia, us)?,
                active: positive_num("active coils", &m.active)?,
                free_length_mm: length_mm("free length", &m.free_length, us)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(AssemblySpec::PowerUser {
        topology: form.topology.clone(),
        fixity: form.fixity.clone(),
        loads_n: loads_n(&form.loads, us)?,
        members,
    })
}

/// Fill the form from a loaded spec (round-trips with `build_spec`).
pub fn populate_from_spec(form: &mut AsmFormState, spec: &AssemblySpec, us: UnitSystem) {
    let AssemblySpec::PowerUser { topology, fixity, loads_n, members } = spec;
    form.topology = topology.clone();
    form.fixity = fixity.clone();
    form.loads = crate::form_helpers::fmt_loads(loads_n, us);
    form.members = members
        .iter()
        .map(|m| AsmMemberForm {
            material: m.material_name.clone(),
            end_type: m.end_type.clone(),
            wire_dia: crate::form_helpers::fmt_len(m.wire_dia_mm, us),
            mean_dia: crate::form_helpers::fmt_len(m.mean_dia_mm, us),
            active: format!("{}", m.active),
            free_length: crate::form_helpers::fmt_len(m.free_length_mm, us),
        })
        .collect();
}
```

Test module (mirror torsion/conical form-test conventions; `MaterialSet::load_default()` for the hermetic store — verify the sibling idiom and match it):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> MaterialSet { MaterialSet::load_default() }

    fn two_member_form() -> AsmFormState {
        let mut f = AsmFormState::with_default_material("Music Wire");
        f.loads = "10, 25".into();
        f.members[0] = AsmMemberForm { wire_dia: "2".into(), mean_dia: "20".into(), active: "10".into(), free_length: "60".into(), ..AsmMemberForm::blank("Music Wire") };
        f.members.push(AsmMemberForm { wire_dia: "1.5".into(), mean_dia: "16".into(), active: "8".into(), free_length: "60".into(), ..AsmMemberForm::blank("Music Wire") });
        f
    }

    #[test]
    fn golden_through_form_matches_direct_solve() {
        let out = parse_and_solve(&two_member_form(), UnitSystem::Metric, &store(), CurvatureCorrection::Bergstrasser).unwrap();
        assert_eq!(out.members.len(), 2);
        assert_eq!(out.topology, springcore::assembly::Topology::Nested);
        // combined nested rate = k1 + k2 (both members solved)
        let k: f64 = out.members.iter().map(|m| m.design.rate.newtons_per_meter()).sum();
        approx::assert_relative_eq!(out.rate.newtons_per_meter(), k, max_relative = 1e-12);
    }

    #[test]
    fn topology_rejection_is_end_to_end() {
        // THE engine-panel carry-forward: a bad topology (from a loaded file)
        // now rejects through parse_and_solve, not just parse_topology.
        let mut f = two_member_form();
        f.topology = "stacked".into();
        let err = parse_and_solve(&f, UnitSystem::Metric, &store(), CurvatureCorrection::Bergstrasser).unwrap_err();
        assert!(err.to_string().contains("unknown topology: stacked"), "got: {err}");
    }

    #[test]
    fn build_populate_round_trips() {
        for us in [UnitSystem::Metric, UnitSystem::Us] {
            let mut f = two_member_form();
            if us == UnitSystem::Us {
                for m in &mut f.members { m.wire_dia = "0.08".into(); m.mean_dia = "0.8".into(); m.free_length = "2.4".into(); }
                f.loads = "2, 5".into();
            }
            let spec = build_spec(&f, us).unwrap();
            let mut round = AsmFormState::with_default_material("Music Wire");
            populate_from_spec(&mut round, &spec, us);
            assert_eq!(build_spec(&round, us).unwrap(), spec, "round-trip lossless ({us:?})");
        }
    }

    #[test]
    fn is_blank_default_and_member_material_governs() {
        assert!(AsmFormState::with_default_material("Music Wire").is_blank());
        // Decision-2: a member's own material name is what solves.
        let mut f = two_member_form();
        f.members[1].material = "Stainless 302".into();
        let out = parse_and_solve(&f, UnitSystem::Metric, &store(), CurvatureCorrection::Bergstrasser).unwrap();
        assert_eq!(out.members[1].material_name, "Stainless 302");
    }

    #[test]
    fn member_diameter_error_is_member_scoped() {
        let mut f = two_member_form();
        f.members[1].wire_dia = "10".into(); // out of range for music wire
        f.members[1].mean_dia = "80".into();
        f.members[1].free_length = "200".into();
        let err = parse_and_solve(&f, UnitSystem::Metric, &store(), CurvatureCorrection::Bergstrasser).unwrap_err();
        assert!(matches!(err, springcore::SpringError::Member { index: 1, .. }), "got: {err:?}");
    }
}
```

- [ ] **Step 8: app wiring + minimal skeletons**

Create `springmaker/src/assembly/view_model.rs` (skeleton, iced-free — mirror conical Task-1 skeleton):

```rust
use crate::app::App;
use crate::presenter::StatusLine;

/// Assembly results panel state (Populated arrives in Task 2).
#[derive(Debug, Clone, PartialEq)]
pub enum AsmResultsView {
    Error(String),
    Empty,
}

/// Outcome-first ordering (the conical ordering-trap lesson): a solved outcome
/// wins over any stale error string. Task 2 replaces the `Some(_) => Empty`
/// arm with the real `Populated`.
pub fn asm_results_view(app: &App) -> AsmResultsView {
    match &app.asm_outcome {
        Some(_) => AsmResultsView::Empty,
        None => match &app.error {
            Some(e) => AsmResultsView::Error(e.clone()),
            None => AsmResultsView::Empty,
        },
    }
}

pub fn asm_status_view(app: &App) -> Vec<StatusLine> {
    crate::presenter::common_status_lines(app)
}
```

(Import only what the skeleton uses — the skeleton must compile clean under `-D warnings`. Task 2 adds the `ResultRow`/`LoadTable`/etc. imports with the Populated shapes.)

Create `springmaker/src/assembly/view.rs` (skeleton) with `design_panel(app) -> Element<'_, Message>` rendering the setup group (topology + fixity pickers via `picker::{TOPOLOGIES, FIXITIES}` — define a `TOPOLOGIES: &[KeyLabel]` const in picker.rs with keys "nested"/"series", labels "Nested"/"Series"), the assembly loads field, and the member-card list; `results_panel(app)` matching the skeleton `AsmResultsView::{Error, Empty}`; and:

```rust
pub(crate) fn asm_member_field_id(index: usize, field: crate::assembly::form::MemberField) -> String {
    use crate::assembly::form::MemberField::*;
    let leaf = match field {
        WireDia => "wire-dia",
        MeanDia => "mean-dia",
        Active => "active",
        FreeLength => "free-length",
    };
    format!("asm-member-{index}-{leaf}")
}
```

The member-card builder (loop `app.assembly.members` via `column.push`, the load-table render precedent):

```rust
fn member_card<'a>(app: &'a App, index: usize, m: &'a crate::assembly::form::AsmMemberForm) -> Element<'a, Message> {
    use crate::assembly::form::MemberField as F;
    let mut header = row![text(format!("Member {}", index + 1)).size(SZ_LABEL)].spacing(8);
    if app.assembly.members.len() > 1 {
        header = header.push(styled_button("Remove", Message::AsmMemberRemove(index)));
    }
    column![
        header,
        material_picker_for_member(app, index),
        // end-type picker: styled_pick_list(END_TYPES, selected, move |kl| Message::AsmMemberEndType(index, kl.key.to_string()))
        labeled_input("Wire dia", &m.wire_dia, asm_member_field_id(index, F::WireDia), move |v| Message::AsmField(index, F::WireDia, v)),
        labeled_input("Mean dia", &m.mean_dia, asm_member_field_id(index, F::MeanDia), move |v| Message::AsmField(index, F::MeanDia, v)),
        labeled_input("Active coils", &m.active, asm_member_field_id(index, F::Active), move |v| Message::AsmField(index, F::Active, v)),
        labeled_input("Free length", &m.free_length, asm_member_field_id(index, F::FreeLength), move |v| Message::AsmField(index, F::FreeLength, v)),
    ]
    .spacing(6)
    .padding(8)
    .into()
}
```

(Adapt `styled_button`/card-border helpers to the actual widget names; wrap the card in the codebase's bordered-container style if one exists. Transcribe from the load-table render for the `column.push` loop over `members`.)

In `springmaker/src/app.rs`:
- State: `pub assembly: crate::assembly::form::AsmFormState` (initialized `AsmFormState::with_default_material(&self.material)` in the constructor beside the siblings — verify the constructor's material-default availability), `pub asm_outcome: Option<springcore::assembly::AssemblyDesign>`.
- Messages: `AsmTopology(String)`, `AsmFixity(String)`, `AsmLoads(String)`, `AsmField(usize, crate::assembly::form::MemberField, String)`, `AsmMemberMaterial(usize, String)`, `AsmMemberEndType(usize, String)`, `AsmMemberAdd`, `AsmMemberRemove(usize)`. Update arms: the scalar setters assign + `true`; `AsmField`/`AsmMemberMaterial`/`AsmMemberEndType` index into `self.assembly.members` (bounds-guarded) + `true`; `AsmMemberAdd => { self.assembly.members.push(AsmMemberForm::blank(&self.material)); true }`; `AsmMemberRemove(i) => { if self.assembly.members.len() > 1 { self.assembly.members.remove(i); } true }` (the min-one floor).
- `recompute()` conical-shaped Assembly arm: clear stale outcome/error, `if self.assembly.is_blank() { return; }`, `parse_and_solve(&self.assembly, self.unit_system, &self.materials, self.correction)` → store `asm_outcome` or `self.error`.
- `save_to()`: `DesignSpec::Assembly(crate::assembly::form::build_spec(&self.assembly, self.unit_system)?)`.
- `apply_saved()`: DELETE the `matches!(…Assembly…) { … return false; }` early-reject block AND the `DesignSpec::Assembly(_) => unreachable!(...)` arm. Add the real arm:

```rust
            springcore::DesignSpec::Assembly(spec) => {
                self.family = Family::Assembly;
                crate::assembly::form::populate_from_spec(&mut self.assembly, &spec, self.unit_system);
            }
```

  `apply_saved` KEEPS `-> bool` (now every arm returns `true`). Update its doc comment to the permanent-signal rationale:

```rust
    /// Apply a loaded design. Returns `false` when the design's family has no
    /// GUI yet (nothing applied, `action_error` set) so `load_from` can skip
    /// the recompute that would wipe the error. Always `true` today — every
    /// family populates — but the signature is RETAINED permanently so the
    /// next family placeholder (see the conical Decision-5 reversal note) does
    /// not flip it again. The `()`↔`bool` pendulum ends here.
```

- `calculator.rs`: the two Family arms (design/results dispatch) → `crate::assembly::view::{design_panel, results_panel}` and the status arm → `crate::assembly::view_model::asm_status_view`.
- PLACEHOLDER TESTS (app.rs): DELETE the two assembly placeholder tests (`…not supported…` and the load-survives-recompute one). REPLACE with a positive load test:

```rust
    #[test]
    fn loading_an_assembly_design_populates_the_assembly_form() {
        let mut app = test_app();
        let applied = app.apply_saved(springcore::SavedDesign {
            material: "Music Wire".to_string(),
            unit_system: springcore::UnitSystem::Metric,
            design: springcore::DesignSpec::Assembly(springcore::AssemblySpec::PowerUser {
                topology: "nested".into(),
                fixity: "fixed_fixed".into(),
                loads_n: vec![10.0],
                members: vec![springcore::AssemblyMemberSpec {
                    material_name: "Music Wire".into(),
                    end_type: "squared_ground".into(),
                    wire_dia_mm: 2.0, mean_dia_mm: 20.0, active: 10.0, free_length_mm: 60.0,
                }],
            }),
        });
        assert!(applied);
        assert_eq!(app.family, springcore::Family::Assembly);
        assert_eq!(app.assembly.members.len(), 1);
        assert_eq!(app.assembly.members[0].wire_dia, "2");
        assert!(app.action_error.is_none());
    }
```

(Export `AssemblyMemberSpec`/`AssemblySpec`/`Topology` from springcore's crate root as needed — verify they're already `pub use`d from the engine PR; add if missing.)

- [ ] **Step 9: workspace green + commit**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all && cargo fmt --all --check` → all green.

```bash
git add springmaker/src/assembly springmaker/src/app.rs springmaker/src/calculator.rs springmaker/src/picker.rs springmaker/src/widgets.rs springmaker/src/compression/view.rs <module-root-file>
git commit -m "feat(gui): assembly family — dynamic member form, app dispatch, minimal panels; placeholder retired

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

- [ ] **Step 10: mutation gate**

```bash
git diff origin/main...HEAD > /tmp/gui-asm-t1.diff
cargo mutants --in-diff /tmp/gui-asm-t1.diff --package springcore
```
Expected `0 missed` (the springcore surface: `Family::Assembly`, the `Member` variant + its Display, `member_error`). Kill survivors with tests.

---

### Task 2: full presenter + view + E2E

**Files:**
- Modify: `springmaker/src/assembly/view_model.rs` (Populated + summary + per-member + status)
- Modify: `springmaker/src/assembly/view.rs` (populated rendering)
- Modify: `springmaker/src/ui_tests.rs` (E2E + save/load)

**Interfaces:**
- Consumes: Task 1's surface; `crate::presenter::{ResultRow, LoadTable, LoadRow, GoverningRate, fmt_row_value, display_len, display_stress, unit_length_label, unit_stress_label, append_status_messages, common_status_lines}`; `springcore::assembly::{evaluate_status, AssemblyDesign, MemberResult, Topology}`.
- Produces: `AsmResultsView::Populated(Box<AsmPopulatedResults>)`, `AsmPopulatedResults`, `AsmMemberResultView`.

- [ ] **Step 1: presenter (TDD)**

Extend `view_model.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum AsmResultsView {
    Error(String),
    Empty,
    Populated(Box<AsmPopulatedResults>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AsmPopulatedResults {
    pub governing_rate: GoverningRate,
    pub summary: Vec<ResultRow>,
    pub assembly_loads: LoadTable,
    pub members: Vec<AsmMemberResultView>,
    pub status: Vec<StatusLine>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AsmMemberResultView {
    pub heading: String,          // "Member N (Music Wire)"
    pub rows: Vec<ResultRow>,     // share %, rate, index, buckling flag
    pub loads: LoadTable,         // per-member per-load stress/%MTS
}
```

`asm_results_view` gains the `Populated` arm (outcome-first, per the conical ordering-trap lesson) building from `out: &AssemblyDesign`:
- `governing_rate: GoverningRate::from_rate(out.rate, us)`.
- `summary` rows (labels/decimals EXACT; lengths via `display_len` + unit label; every numeric through `fmt_row_value`):
  | Label | Source | fmt |
  |---|---|---|
  | Topology | `out.topology` → "Nested"/"Series" | text, unit "" |
  | Free length | `out.free_length` | `fmt_row_value(display_len(…), 4)` + len unit |
  | Solid length | `out.solid_length` | 4 + len unit |
  | Travel limit | `out.travel_limit_deflection` | 4 + len unit |
  | Travel-limit force | `out.travel_limit_force` | `fmt_row_value(display_force(…), 3)` + force unit |
  | Limited by | `out.limiting_member` | `format!("member {}", out.limiting_member + 1)`, unit "" |
- `assembly_loads`: `LoadTable` from `out.load_points` (force/deflection/length value+unit cells through `fmt_row_value`; there is NO per-load stress at the assembly level — the assembly load table shows force/deflection/length only, mirroring the AssemblyLoadPoint fields; document that the stress lives in the per-member tables).
- `members`: for each `MemberResult`, `heading = format!("Member {} ({})", i+1, mr.material_name)`; `rows` = share % (`fmt_row_value(mr.share_fraction * 100.0, 1)` + "%"), member rate, member spring index, buckling ("stable"/"buckling risk" from `mr.design.buckling_stable`); `loads` = the compression LoadTable idiom over `mr.design.load_points` (force/deflection/length/stress/%MTS — the full per-member load table).
- `status`: `common_status_lines(app)` then `append_status_messages(&mut lines, &evaluate_status(out, &app.materials).messages)`.

Presenter tests (build the outcome via `parse_and_solve` on a fixture form; mirror sibling presenter-test conventions):
- `summary_and_member_rows_exact`: a nested two-member metric fixture → assert the six summary labels in order + spot values (Topology "Nested"; Limited by "member N"); assert `members.len() == 2` and member 1's heading `"Member 1 (Music Wire)"` and its share row present.
- `results_view_tristate`: blank → Empty; app with an error → Error; solved → Populated.
- `huge_finite_load_member_stress_is_scientific`: loads "1e9" → a member load-table stress cell contains 'e'.
- `member_prefixed_status_passthrough`: a clearance-interfering nested pair (per the engine's clearance fixture) → a status line whose text contains the exact engine clearance message ("nested interference"); a member overstress → "member N: load point".
- `limiting_member_callout`: assert the "Limited by" summary row reads `"member {out.limiting_member+1}"`.

- [ ] **Step 2: view (populated rendering)**

`results_panel`'s `Populated` arm: hero rate → "Summary" section (`out.summary` rows) → assembly Load table → a per-member section for each `AsmMemberResultView` (heading + `rows` + its `loads` table) → statuses via the shared status panel. All value cells through `fmt_row_value` (already applied in the presenter). Mirror the compression/conical results panel structure; the per-member sections loop via `column.push`.

- [ ] **Step 3: E2E (`springmaker/src/ui_tests.rs`) — real Simulator clicks on dynamic ids**

Add a member-field typer using the runtime id (iced 0.14 resolves `widget::Id::new(String)`):

```rust
fn type_into_asm_member(app: &mut App, index: usize, field: crate::assembly::form::MemberField, text: &str) {
    let id = iced_test::core::widget::Id::new(crate::assembly::view::asm_member_field_id(index, field));
    let mut sim = ui(app);
    sim.click(id).unwrap_or_else(|e| panic!("member {index} field {field:?}: {e}"));
    sim.typewrite(text);
    for message in sim.into_messages() { app.update(message); }
}
```

Tests:

```rust
#[test]
fn assembly_e2e_dynamic_members_and_results() {
    use crate::assembly::form::MemberField as F;
    let mut app = test_app();
    app.update(Message::SelectFamily(springcore::Family::Assembly));
    // Member 0 (present by default).
    type_into_asm_member(&mut app, 0, F::WireDia, "2");
    type_into_asm_member(&mut app, 0, F::MeanDia, "20");
    type_into_asm_member(&mut app, 0, F::Active, "10");
    type_into_asm_member(&mut app, 0, F::FreeLength, "60");
    // Add member 1 and fill it (indexed ids must resolve on the new row).
    app.update(Message::AsmMemberAdd);
    type_into_asm_member(&mut app, 1, F::WireDia, "1.5");
    type_into_asm_member(&mut app, 1, F::MeanDia, "16");
    type_into_asm_member(&mut app, 1, F::Active, "8");
    type_into_asm_member(&mut app, 1, F::FreeLength, "60");
    app.update(Message::AsmLoads("10, 25".into()));
    assert!(app.asm_outcome.is_some(), "two-member assembly solves");
    assert!(shows(&app, "Summary"));
    assert!(shows(&app, "Member 1 (Music Wire)"));
    assert!(shows(&app, "Member 2 (Music Wire)"));
    // Remove member 2 → back to one member.
    app.update(Message::AsmMemberRemove(1));
    assert_eq!(app.assembly.members.len(), 1);
}

#[test]
fn assembly_us_member_diameter_error_renders_in_inches() {
    use crate::assembly::form::MemberField as F;
    let mut app = test_app();
    app.update(Message::Units(springcore::UnitSystem::Us)); // verify the actual unit-toggle message
    app.update(Message::SelectFamily(springcore::Family::Assembly));
    type_into_asm_member(&mut app, 0, F::WireDia, "0.4"); // ~10mm, out of range for music wire
    type_into_asm_member(&mut app, 0, F::MeanDia, "3.0");
    type_into_asm_member(&mut app, 0, F::Active, "10");
    type_into_asm_member(&mut app, 0, F::FreeLength, "8");
    app.update(Message::AsmLoads("2".into()));
    assert!(shows(&app, "member 1: wire diameter") && shows(&app, " in "), "inch-formatted member error");
}

#[test]
fn assembly_save_load_round_trip() {
    // Fill a two-member assembly, save_to a temp file, fresh app, load_from,
    // assert family switched + members repopulated + recompute yields results.
    // Mirror the conical save/load E2E's temp-file idiom exactly.
}
```

(Complete the round-trip body from the nearest sibling save/load E2E. Verify the unit-toggle message name and the material-selection message against the current harness; adapt `shows(&app, " in ")` if the assertion needs the full inch-formatted substring.)

- [ ] **Step 4: full gate + commit**

```bash
cargo fmt --all && cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
typos
git diff origin/main...HEAD > /tmp/gui-asm-full.diff
cargo mutants --in-diff /tmp/gui-asm-full.diff --package springcore
```
All clean; `0 missed`.

```bash
git add springmaker/src/assembly springmaker/src/ui_tests.rs
git commit -m "feat(gui): assembly results — summary, per-member tables, dynamic-member E2E, member error re-localization

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```
