# Materials Editor GUI (Sub-project 2, PR c) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a GUI materials editor to `springmaker` — list/add/clone/edit/remove/save user materials backed by `MaterialStore`, with the calculator reading the merged curated+user set.

**Architecture:** A small public `MaterialDraft` construction API in `springcore` (the GUI cannot build a `Material` directly — `mts` is `pub(crate)`), reusing the existing `try_from_raw` validation. `springmaker` switches `App.materials` to `MaterialStore`, gains a `Screen` route, a pure `materials_form.rs` (form↔draft logic, no iced, mirroring `form.rs`), and a `materials_view.rs` editor screen (mirroring `view.rs`).

**Tech Stack:** Rust 2021 (MSRV 1.86), iced 0.13 (free-function API), springcore engine.

## Global Constraints

- No reference to any commercial spring-design product/vendor in any persisted file.
- Maximum type strictness; clippy `-D warnings` clean; `cargo fmt` clean.
- TDD: write the failing test first. Pure logic (Tasks 1, 3) and state logic (Task 4) are unit-tested; iced view code (Task 5) is verified by compile + the logic beneath it.
- `springcore` has no GUI deps; the editor's public API lives in `springcore`, the iced code in `springmaker`.
- The user overlay is untrusted on load but the editor's own inputs are validated before `build()`; all construction goes through `MaterialDraft::build` → `try_from_raw` (finiteness, positivity, coeff-count, range, allowable ∈ (0,1]).
- Persistence is via `MaterialStore::save()` (atomic, OS config dir).

---

### Task 1: Public `MaterialDraft` construction API (springcore)

**Files:**
- Modify: `springcore/src/material.rs` (add `MaterialDraft`, `EnduranceDraft`, `Material::to_draft`, `MaterialDraft::build`; near `RawMaterial`/`try_from_raw`).
- Modify: `springcore/src/lib.rs` (re-export `MaterialDraft`, `EnduranceDraft`).

**Interfaces:**
- Consumes: existing `pub(crate) fn Material::try_from_raw(RawMaterial) -> Result<Material>`, `pub(crate) fn Material::to_raw(&self) -> RawMaterial`, `RawMaterial`, `RawEndurance`.
- Produces (public):
  ```rust
  pub struct MaterialDraft {
      pub name: String,
      pub specification: String,
      pub citations: String,
      pub mts_form: MtsForm,            // pub enum already
      pub mts_units: StrengthUnits,     // pub enum already
      pub mts_coefficients: Vec<f64>,
      pub valid_dia_min_mm: f64,
      pub valid_dia_max_mm: f64,
      pub youngs_modulus_gpa: f64,
      pub shear_modulus_gpa: f64,
      pub density_kg_per_m3: f64,
      pub allowable_pct_torsion: f64,
      pub allowable_pct_bending: f64,
      pub allowable_pct_set: f64,
      pub endurance: Option<EnduranceDraft>,
      pub max_service_temp_c: Option<f64>,
  }
  pub struct EnduranceDraft { pub ssa_mpa: f64, pub ssm_mpa: f64, pub peened: bool }
  impl MaterialDraft { pub fn build(&self) -> Result<Material>; }   // validates via try_from_raw
  impl Material { pub fn to_draft(&self) -> MaterialDraft; }        // for edit/clone population
  ```
  Both derive `Debug, Clone, PartialEq`. `build` maps the draft into a `RawMaterial` (using the existing pub(crate) `mts_form_str`/`strength_units_str` to stringify the enums) and calls `try_from_raw`. `to_draft` calls `self.to_raw()` and maps back (parsing the strings via the existing pub(crate) helpers — these always succeed for a valid `Material`).

- [ ] **Step 1: Write failing tests** in `material.rs` `#[cfg(test)] mod tests`:
```rust
fn good_draft() -> MaterialDraft {
    MaterialDraft {
        name: "My Wire".into(), specification: "synthetic".into(), citations: "synthetic".into(),
        mts_form: MtsForm::PowerLaw, mts_units: StrengthUnits::SiMpaMm,
        mts_coefficients: vec![2000.0, 0.15],
        valid_dia_min_mm: 0.5, valid_dia_max_mm: 6.0,
        youngs_modulus_gpa: 200.0, shear_modulus_gpa: 79.0, density_kg_per_m3: 7850.0,
        allowable_pct_torsion: 0.45, allowable_pct_bending: 0.75, allowable_pct_set: 0.6,
        endurance: None, max_service_temp_c: Some(120.0),
    }
}
#[test] fn draft_builds_valid_material() {
    let m = good_draft().build().unwrap();
    assert_eq!(m.name, "My Wire");
    assert_relative_eq!(
        m.min_tensile_strength(Length::from_millimeters(1.0)).unwrap().megapascals(),
        2000.0, max_relative = 1e-9); // 2000/1^0.15 = 2000
}
#[test] fn draft_build_rejects_bad_coeff_count() {
    let mut d = good_draft(); d.mts_coefficients = vec![2000.0]; // power_law needs 2
    assert!(matches!(d.build(), Err(SpringError::DataFile(_))));
}
#[test] fn draft_build_rejects_allowable_over_one() {
    let mut d = good_draft(); d.allowable_pct_torsion = 1.5;
    assert!(matches!(d.build(), Err(SpringError::DataFile(_))));
}
#[test] fn to_draft_round_trips_through_build() {
    let set = MaterialSet::load_default();
    let original = set.get("Music Wire").unwrap();
    let rebuilt = original.to_draft().build().unwrap();
    let d = Length::from_millimeters(2.0);
    assert_relative_eq!(
        rebuilt.min_tensile_strength(d).unwrap().pascals(),
        original.min_tensile_strength(d).unwrap().pascals(), max_relative = 1e-12);
    assert_eq!(rebuilt.name, original.name);
    assert_relative_eq!(rebuilt.youngs_modulus.pascals(), original.youngs_modulus.pascals(), max_relative = 1e-12);
}
#[test] fn to_draft_preserves_polynomial_phosphor_bronze() {
    let set = MaterialSet::load_default();
    let m = set.get("Phosphor Bronze").unwrap();
    assert_eq!(m.to_draft().mts_form, MtsForm::Polynomial);
    assert_eq!(m.to_draft().mts_coefficients.len(), 3);
}
```
- [ ] **Step 2: Run** `cargo test -p springcore --lib draft` → FAIL (MaterialDraft undefined).
- [ ] **Step 3: Implement** `MaterialDraft`, `EnduranceDraft`, `build`, `to_draft` in `material.rs`; re-export both from `lib.rs`. `build` constructs a `RawMaterial { mts_form: mts_form_str(self.mts_form).into(), mts_units: strength_units_str(self.mts_units).into(), endurance: self.endurance.as_ref().map(|e| RawEndurance{ssa_mpa:e.ssa_mpa, ssm_mpa:e.ssm_mpa, peened:e.peened}), .. }` then `Material::try_from_raw(raw)`. `to_draft` uses `self.to_raw()` then maps (parse form/units strings via `mts_form_from_str`/`strength_units_from_str`, `.expect("valid material")`).
- [ ] **Step 4: Run** `cargo test -p springcore --lib draft` → PASS. Then `cargo clippy -p springcore --all-targets -- -D warnings` clean.
- [ ] **Step 5: Commit** `feat(material): public MaterialDraft construction API for the GUI editor`.

---

### Task 2: Switch `springmaker` to `MaterialStore` + surface load warnings

**Files:**
- Modify: `springmaker/src/app.rs` (imports; `App.materials` type; `App::default`; add `load_warnings`).
- Modify: `springmaker/src/form.rs` (`parse_and_solve` signature `&MaterialSet` → `&MaterialStore`; test fixtures).
- Modify: `springmaker/src/view.rs` (surface `load_warnings` in the status area; picker already calls `.names()`).

**Interfaces:**
- Consumes: `springcore::{MaterialStore, LoadWarning}`; `MaterialStore::load() -> (MaterialStore, Vec<LoadWarning>)`, `.names() -> Vec<&str>`, `.get(name) -> Result<&Material>`.
- Produces: `App.materials: MaterialStore`, `App.load_warnings: Vec<LoadWarning>`; `pub fn parse_and_solve(form: &FormState, materials: &MaterialStore) -> Result<FormOutcome>`.

- [ ] **Step 1: Update test fixtures first** — in `form.rs` tests replace `let set = MaterialSet::load_default();` with `let set = MaterialStore::new(MaterialSet::load_default());` and import `MaterialStore`. In `app.rs` tests, `App::default()` already builds the store. Add a test asserting the merged store is used:
```rust
#[test]
fn parse_and_solve_accepts_material_store() {
    let store = MaterialStore::new(MaterialSet::load_default());
    let form = rate_based_metric();
    assert!(parse_and_solve(&form, &store).is_ok());
}
```
- [ ] **Step 2: Run** `cargo test -p springmaker` → FAIL (type mismatch: parse_and_solve wants &MaterialSet).
- [ ] **Step 3: Implement** — `app.rs`: `use springcore::{LoadWarning, MaterialStore, SavedDesign, UnitSystem};`; `App { ..., materials: MaterialStore, load_warnings: Vec<LoadWarning>, .. }`; in `App::default()` `let (materials, load_warnings) = MaterialStore::load();`. `form.rs`: change `parse_and_solve` param to `&MaterialStore` (body unchanged — `.get()` is identical). `view.rs`: in `build_status_panel` (or a new banner above it) render each `app.load_warnings[i].message` as a `C::WARN` caption row when non-empty.
- [ ] **Step 4: Run** `cargo test -p springmaker` → PASS; `cargo clippy -p springmaker --all-targets -- -D warnings` clean; `cargo build -p springmaker`.
- [ ] **Step 5: Commit** `feat(gui): back the calculator with MaterialStore and surface load warnings`.

---

### Task 3: `materials_form.rs` — pure form↔draft logic (no iced)

**Files:**
- Create: `springmaker/src/materials_form.rs`.
- Modify: `springmaker/src/main.rs` (add `mod materials_form;`).

**Interfaces:**
- Consumes: `springcore::{MaterialDraft, EnduranceDraft, MtsForm, StrengthUnits, Material, SpringError, Result}`.
- Produces:
  ```rust
  pub struct MaterialsFormState {       // all String inputs + the two enums
      pub name: String, pub specification: String, pub citations: String,
      pub mts_form: MtsForm, pub mts_units: StrengthUnits,
      pub coefficients: String,         // comma-separated, e.g. "2000, 0.15"
      pub valid_dia_min: String, pub valid_dia_max: String,   // mm
      pub youngs_modulus: String, pub shear_modulus: String,  // GPa
      pub density: String,              // kg/m3
      pub allowable_torsion: String, pub allowable_bending: String, pub allowable_set: String,
      pub has_endurance: bool, pub endurance_ssa: String, pub endurance_ssm: String, pub endurance_peened: bool,
      pub has_max_temp: bool, pub max_temp_c: String,
  }
  impl Default for MaterialsFormState { ... blank, mts_form=PowerLaw, mts_units=SiMpaMm ... }
  pub fn build_draft(form: &MaterialsFormState) -> Result<MaterialDraft>;
  pub fn populate_from_material(form: &mut MaterialsFormState, m: &Material);  // via m.to_draft()
  pub fn coefficient_labels(form: MtsForm) -> &'static [&'static str];  // form-aware field hints
  ```
  `build_draft` parses numbers (reuse the `num`/`positive_num` pattern from `form.rs:166`, returning `SpringError::InconsistentInputs`), splits `coefficients` on commas into `Vec<f64>`, and assembles a `MaterialDraft` (field validation like coeff-count/range/allowable happens in `MaterialDraft::build`, NOT here — `build_draft` only parses text into the draft). `coefficient_labels` returns e.g. `["A (MPa·mm^m)", "m"]` for PowerLaw, `["P0","P1","P2","P3","P4"]` for Rational, `["UTS"]` for Constant, `["c0","c1","c2,…"]` for Polynomial — drives the view's hint text.

- [ ] **Step 1: Write failing tests** in `materials_form.rs`:
```rust
fn power_law_form() -> MaterialsFormState {
    let mut f = MaterialsFormState::default();
    f.name = "Test".into(); f.specification = "x".into(); f.citations = "x".into();
    f.coefficients = "2000, 0.15".into();
    f.valid_dia_min = "0.5".into(); f.valid_dia_max = "6.0".into();
    f.youngs_modulus = "200".into(); f.shear_modulus = "79".into(); f.density = "7850".into();
    f.allowable_torsion = "0.45".into(); f.allowable_bending = "0.75".into(); f.allowable_set = "0.6".into();
    f
}
#[test] fn build_draft_parses_power_law() {
    let d = build_draft(&power_law_form()).unwrap();
    assert_eq!(d.mts_coefficients, vec![2000.0, 0.15]);
    assert!(d.build().is_ok());
}
#[test] fn build_draft_rejects_nonnumeric_coefficient() {
    let mut f = power_law_form(); f.coefficients = "2000, abc".into();
    assert!(build_draft(&f).is_err());
}
#[test] fn build_draft_rejects_nonnumeric_modulus() {
    let mut f = power_law_form(); f.youngs_modulus = "".into();
    assert!(build_draft(&f).is_err());
}
#[test] fn populate_round_trips_via_to_draft() {
    let set = MaterialSet::load_default(); // import springcore::MaterialSet in test
    let mut f = MaterialsFormState::default();
    populate_from_material(&mut f, set.get("Music Wire").unwrap());
    assert_eq!(f.name, "Music Wire");
    assert_eq!(f.mts_form, MtsForm::PowerLaw);
    let d = build_draft(&f).unwrap();
    assert!(d.build().is_ok());
}
#[test] fn coefficient_labels_match_form() {
    assert_eq!(coefficient_labels(MtsForm::Rational).len(), 5);
    assert_eq!(coefficient_labels(MtsForm::PowerLaw).len(), 2);
}
```
- [ ] **Step 2: Run** `cargo test -p springmaker materials_form` → FAIL.
- [ ] **Step 3: Implement** `materials_form.rs` + `mod materials_form;` in `main.rs`.
- [ ] **Step 4: Run** tests → PASS; clippy clean.
- [ ] **Step 5: Commit** `feat(gui): pure materials form-to-draft logic with validation`.

---

### Task 4: `app.rs` — Screen routing + Materials CRUD state & messages

**Files:**
- Modify: `springmaker/src/app.rs` (add `Screen`, `MaterialEdit` state, `Message` variants, `update` arms).

**Interfaces:**
- Consumes: `materials_form::{MaterialsFormState, build_draft, populate_from_material}`; `MaterialStore::{add, update, remove, clone_material, is_curated, get, save}`.
- Produces:
  ```rust
  #[derive(Clone, Copy, PartialEq)] pub enum Screen { Calculator, Materials }
  // App gains: pub screen: Screen, pub mat_form: MaterialsFormState,
  //            pub editing: Option<EditTarget>, pub mat_error: Option<String>, pub mat_status: Option<String>
  pub enum EditTarget { New, Existing(String) }   // Existing = the original name being edited
  // Message gains:
  //   NavigateTo(Screen), MatField(MatField, String), MatFormKind(MtsForm), MatUnits(StrengthUnits),
  //   MatToggleEndurance(bool), MatTogglePeened(bool), MatToggleMaxTemp(bool),
  //   MatNew, MatClone(String), MatEdit(String), MatCommit, MatCancel, MatDelete(String), MatPersist
  ```
  where `MatField` is an enum of editable text fields (Name, Spec, Citations, Coefficients, ValidDiaMin/Max, Youngs, Shear, Density, AllowTorsion/Bending/Set, EnduranceSsa/Ssm, MaxTemp). `update` handlers:
  - `NavigateTo(s)` → `self.screen = s`.
  - `MatField/MatFormKind/...` → mutate `self.mat_form`; clear `self.mat_error`.
  - `MatNew` → `self.mat_form = Default`, `self.editing = Some(EditTarget::New)`.
  - `MatEdit(name)` → if `is_curated(name)` set `mat_error = "curated materials are read-only"`, else `populate_from_material(&mut self.mat_form, store.get(name)?)`, `editing = Some(Existing(name))`.
  - `MatClone(name)` → `let copy = store.clone_material(name)?; let new = copy.name.clone(); store.add(copy)?; populate from store.get(&new); editing = Some(Existing(new))`; `mat_status = "cloned"`.
  - `MatCommit` → `let draft = build_draft(&self.mat_form)?; let m = draft.build()?;` then for `New` `store.add(m)?`, for `Existing(orig)` `store.update(orig, m)?`; on Ok `editing = None`, recompute picker; on Err set `mat_error`.
  - `MatDelete(name)` → `store.remove(name)?` (errors on curated) → set `mat_error` or clear.
  - `MatPersist` → `store.save()?` → `mat_status = "saved"` or `mat_error`.

- [ ] **Step 1: Write failing tests** (`app.rs` tests, no iced render):
```rust
#[test] fn add_user_material_via_state() {
    let mut app = App::default();
    app.update(Message::MatNew);
    // fill a valid power-law form
    app.mat_form.name = "New Wire".into(); app.mat_form.specification="x".into(); app.mat_form.citations="x".into();
    app.mat_form.coefficients = "2000, 0.15".into();
    app.mat_form.valid_dia_min="0.5".into(); app.mat_form.valid_dia_max="6".into();
    app.mat_form.youngs_modulus="200".into(); app.mat_form.shear_modulus="79".into(); app.mat_form.density="7850".into();
    app.mat_form.allowable_torsion="0.45".into(); app.mat_form.allowable_bending="0.75".into(); app.mat_form.allowable_set="0.6".into();
    app.update(Message::MatCommit);
    assert!(app.mat_error.is_none());
    assert!(app.materials.names().contains(&"New Wire"));
    assert!(!app.materials.is_curated("New Wire"));
}
#[test] fn editing_curated_is_rejected() {
    let mut app = App::default();
    app.update(Message::MatEdit("Music Wire".into()));
    assert!(app.mat_error.is_some());
}
#[test] fn delete_curated_is_rejected() {
    let mut app = App::default();
    app.update(Message::MatDelete("Music Wire".into()));
    assert!(app.mat_error.is_some());
    assert!(app.materials.names().contains(&"Music Wire"));
}
#[test] fn navigate_switches_screen() {
    let mut app = App::default();
    app.update(Message::NavigateTo(Screen::Materials));
    assert_eq!(app.screen, Screen::Materials);
}
```
- [ ] **Step 2: Run** `cargo test -p springmaker --lib` → FAIL.
- [ ] **Step 3: Implement** the enums, state fields, Message variants, and `update` arms. (`update` already returns/handles recompute; CRUD arms set `should_recompute=false` except `MatCommit` which may change the active material list → recompute the calculator picker is harmless; keep `false` for editor-only actions.)
- [ ] **Step 4: Run** tests → PASS; clippy clean.
- [ ] **Step 5: Commit** `feat(gui): materials screen state, CRUD messages, and update logic`.

---

### Task 5: `materials_view.rs` — the editor screen (iced) + navigation

**Files:**
- Create: `springmaker/src/materials_view.rs`.
- Modify: `springmaker/src/main.rs` (`mod materials_view;`).
- Modify: `springmaker/src/app.rs` (`App::view` routes by `self.screen`).
- Modify: `springmaker/src/view.rs` (header gains a "Materials"/"Calculator" nav button emitting `Message::NavigateTo`).

**Interfaces:**
- Consumes: the `view.rs` design tokens/helpers pattern (replicate the needed helpers locally or make the shared ones `pub(crate)` in `view.rs` and reuse — prefer making `panel_container`, `styled_text_input`-equivalent, `field_label`, `section_heading`, `section_divider`, `C` reusable). `app::{App, Message, Screen, MatField, EditTarget}`; `materials_form::coefficient_labels`; `MaterialStore::{names, is_curated, get}`.
- Produces: `pub fn view(app: &App) -> iced::Element<'_, Message>`.

**Layout (mirror `view.rs` structure and tokens):**
- Header with title + a nav button: on Calculator screen show "Materials →" (`NavigateTo(Materials)`); on Materials screen show "← Calculator" (`NavigateTo(Calculator)`).
- Master-detail in a `row!`:
  - **List panel** (`panel_container`): scrollable column of materials. Each row: name (mono), a badge — "curated" (`C::MUTED`) with a lock glyph, or "user" (`C::ACCENT`); buttons: Edit + Clone for all, Remove for user only (curated rows show no Remove). Buttons emit `MatEdit/MatClone/MatDelete(name)`. A "New" button (`MatNew`) and a "Save to disk" button (`MatPersist`) at the bottom.
  - **Edit panel** (`panel_container`, shown when `app.editing.is_some()`): the form. Fields via labeled inputs bound to `MatField(...)`: name, specification, citations; an MTS-form `pick_list` (`MatFormKind`) over `MtsForm` variants and a units `pick_list` (`MatUnits`); a coefficients text input whose label/hint comes from `coefficient_labels(app.mat_form.mts_form)`; valid dia min/max; E, G, density; allowable torsion/bending/set; an "endurance" checkbox (`MatToggleEndurance`) revealing ssa/ssm/peened; a "max service temperature" checkbox (`MatToggleMaxTemp`, labelled *informational*) revealing the °C input. Commit (`MatCommit`) + Cancel (`MatCancel`) buttons. Show `app.mat_error` in `C::DANGER`, `app.mat_status` in `C::SUCCESS`.

- [ ] **Step 1:** Add `mod materials_view;` to `main.rs`; make the shared `view.rs` helpers + `C` reusable (`pub(crate)`), or duplicate minimally. Route `App::view`:
```rust
pub fn view(&self) -> iced::Element<'_, Message> {
    match self.screen {
        Screen::Calculator => view::view(self),
        Screen::Materials => materials_view::view(self),
    }
}
```
- [ ] **Step 2:** Implement `materials_view::view` per the layout above, reusing tokens/helpers. Add the header nav button in `view.rs::build_header`. MTS-form/units `pick_list`s require `MtsForm`/`StrengthUnits` to impl `Display` + `Clone` + `PartialEq` — add `impl std::fmt::Display` and any missing `#[derive]` in `springcore::material.rs` (e.g. PowerLaw → "Power law") and re-export if needed.
- [ ] **Step 3:** `cargo build -p springmaker` (iced views aren't unit-tested; compilation + the Task 3/4 logic tests are the gate). Manually sanity-check: `cargo run -p springmaker`, navigate to Materials, add/clone/edit/remove/save.
- [ ] **Step 4:** `cargo clippy -p springmaker --all-targets -- -D warnings` clean; `cargo fmt --all --check`; `cargo test --workspace` (all prior tests still green).
- [ ] **Step 5: Commit** `feat(gui): materials editor screen with list, edit form, and navigation`.

---

## Final verification (controller)

- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check` all clean.
- Mutation gate on changed `springcore` logic (`MaterialDraft::build`/`to_draft`) via `cargo mutants --in-diff` — drive to 0 missed (the GUI/view code in springmaker isn't under the mutation gate, which targets engine logic).
- Run the mandatory multi-reviewer panel (general/security, architect, simplifier; add a GUI-state reviewer for the CRUD/update flow and a deep-impact reviewer for the `MaterialSet`→`MaterialStore` signature change). Cycle to convergence. Do not push without convergence.
- Manual smoke test of the editor (add/clone/edit/remove/save round-trip; curated read-only enforced; load-warning surfaced).

## Spec coverage check (§8)
- List with curated/user badge + read-only lock → Task 5 list panel.
- Add / Clone / Edit (user) / Remove (user) / Save → Tasks 4 (logic) + 5 (UI).
- Edit form with MTS-form-specific coefficient fields, native units, dia range, E/G/density, allowable %s, optional endurance, optional informational max temp, citations → Tasks 3 (fields) + 5 (rendering).
- Live validation with clear messages → `build_draft`→`build` errors surfaced as `mat_error` (Tasks 3/4/5).
- Calculator picker reads merged store; startup LoadWarning shown → Task 2.
- Pure form logic separated from iced → Task 3 (`materials_form.rs`).
