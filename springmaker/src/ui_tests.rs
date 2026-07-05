//! End-to-end GUI tests driving the real view → message → update loop with
//! iced's headless `Simulator` (iced_test). These complement the presenter unit
//! tests (`view_model`) by exercising the actual widget tree: a click resolves
//! against the rendered layout, emits the wired `Message`, and we feed it back
//! through `App::update` exactly as the runtime would.
//!
//! Tests avoid the `Save design` / `Load design` buttons (which open native
//! `rfd` dialogs) and `Save to disk` (which writes the user overlay) — those
//! perform IO and can't run headlessly.

use crate::app::{App, Message, Screen};
use crate::compression::form::Field;
use crate::extension::form::{build_spec, ExtScenarioKind, Field as ExtField, HookMode};
use crate::extension::view_model::{ext_results_view, ExtResultsView};
use iced_test::core::Settings;
use iced_test::Simulator;
use springcore::{Family, MaterialSet, MaterialStore, UnitSystem};

/// A viewport large enough that no widget is clipped: as wide as the app's
/// 1200px design width and tall enough for the full materials edit-form
/// scrollable. `Simulator::click` only hits laid-out widgets, so the bottom
/// action buttons ("Save entry"/"Cancel") must fall within this height — they
/// sit below the default 1024x768 fold. 2400px clears the worst case (the
/// endurance-expanded edit form, ~1700px); bump it if the form grows taller.
const VIEWPORT: iced::Size = iced::Size {
    width: 1200.0,
    height: 2400.0,
};

/// Hermetic app: curated-only store, no on-disk overlay, fixed Bergsträsser correction.
fn test_app() -> App {
    App::from_store(
        MaterialStore::new(MaterialSet::load_default()),
        Vec::new(),
        springcore::CurvatureCorrection::Bergstrasser,
    )
}

fn ui(app: &App) -> Simulator<'_, Message> {
    Simulator::with_size(Settings::default(), VIEWPORT, app.view())
}

/// Render the app, click the widget matching `label`, and apply every message
/// the interaction produced — the headless equivalent of one runtime cycle.
fn click(app: &mut App, label: &str) {
    let mut sim = ui(app);
    sim.click(label)
        .unwrap_or_else(|_| panic!("no clickable widget matching {label:?}"));
    for message in sim.into_messages() {
        app.update(message);
    }
}

/// Whether a widget matching `label` is present in the current render.
fn shows(app: &App, label: &str) -> bool {
    ui(app).find(label).is_ok()
}

/// Focus a calculator field's input by its stable id and type `text` into it,
/// then apply the resulting messages. Focus is UI-internal (not in `App`), so
/// the focusing click and the `typewrite` must share one simulator instance.
fn type_into(app: &mut App, field: Field, text: &str) {
    let id = iced_test::core::widget::Id::from(crate::compression::view::calc_field_id(field));
    let mut sim = ui(app);
    sim.click(id)
        .unwrap_or_else(|e| panic!("could not focus input for field {field:?}: {e}"));
    sim.typewrite(text);
    for message in sim.into_messages() {
        app.update(message);
    }
}

/// Number of user (non-curated) materials currently in the store.
fn user_material_count(app: &App) -> usize {
    app.materials
        .names()
        .iter()
        .filter(|n| !app.materials.is_curated(n))
        .count()
}

#[test]
fn navigates_between_calculator_and_materials() {
    let mut app = test_app();
    assert_eq!(app.screen, Screen::Calculator);

    click(&mut app, "Materials →");
    assert_eq!(app.screen, Screen::Materials);

    click(&mut app, "← Calculator");
    assert_eq!(app.screen, Screen::Calculator);
}

#[test]
fn new_button_opens_editor_and_cancel_closes_it() {
    let mut app = test_app();
    click(&mut app, "Materials →");

    // Editor closed: the placeholder is shown, not the edit form.
    assert!(shows(&app, "Select a material to edit, or New."));

    click(&mut app, "New");
    assert!(shows(&app, "Edit material"), "the edit panel should open");
    assert!(shows(&app, "New material"));

    click(&mut app, "Cancel");
    assert!(
        shows(&app, "Select a material to edit, or New."),
        "Cancel should close the editor back to the placeholder"
    );
}

#[test]
fn clone_creates_an_editable_user_material() {
    let mut app = test_app();
    assert_eq!(user_material_count(&app), 0);

    click(&mut app, "Materials →");
    // Each curated row offers a "Clone" button; clone the first one.
    click(&mut app, "Clone");

    assert_eq!(
        user_material_count(&app),
        1,
        "cloning a curated material adds one user material"
    );
    // Clone opens the editor on the copy (an existing user material).
    assert!(shows(&app, "Editing"), "the clone opens in the editor");
    assert!(shows(&app, "user"), "a user-provenance badge now appears");
}

#[test]
fn remove_deletes_a_user_material() {
    let mut app = test_app();
    click(&mut app, "Materials →");
    click(&mut app, "Clone");
    assert_eq!(user_material_count(&app), 1);

    // Close the editor, then remove the user material via its row button.
    click(&mut app, "Cancel");
    click(&mut app, "Remove");
    assert_eq!(
        user_material_count(&app),
        0,
        "Remove deletes the user material"
    );
}

#[test]
fn typing_a_valid_power_user_design_renders_results() {
    let mut app = test_app();
    // Calculator opens on the empty PowerUser form → results show the prompt.
    assert!(shows(&app, "Enter design parameters to see results."));

    // Type a valid design field by field (each input targeted by its stable id).
    type_into(&mut app, Field::WireDia, "2.0");
    type_into(&mut app, Field::MeanDia, "20.0");
    type_into(&mut app, Field::Active, "10");
    type_into(&mut app, Field::FreeLength, "60");
    type_into(&mut app, Field::Loads, "10, 30");

    // typewrite accumulates the full string into the focused input (not just the
    // last keystroke) — assert the form captured each value verbatim.
    assert_eq!(app.form.wire_dia, "2.0");
    assert_eq!(app.form.mean_dia, "20.0");
    assert_eq!(app.form.loads, "10, 30");

    // The full input → Field → recompute → results-render path now produces a
    // solved design in the UI.
    assert!(
        shows(&app, "Spring rate"),
        "a valid design should render the results panel"
    );
    assert!(shows(&app, "Geometry"));
    assert!(!shows(&app, "Enter design parameters to see results."));
}

#[test]
fn settings_changes_correction_and_recomputes() {
    let mut app = test_app();
    // Enter a valid PowerUser design field by field.
    type_into(&mut app, Field::WireDia, "2.0");
    type_into(&mut app, Field::MeanDia, "20.0");
    type_into(&mut app, Field::Active, "10");
    type_into(&mut app, Field::FreeLength, "60");
    type_into(&mut app, Field::Loads, "10, 30");
    assert!(
        shows(&app, "Spring rate"),
        "results should render before navigating"
    );

    // Capture shear stress under the default Bergsträsser correction.
    let before = app.outcome.as_ref().unwrap().design.load_points[0]
        .shear_stress
        .pascals();

    // Navigate to Settings, select Wahl, then return to the Calculator.
    click(&mut app, "Settings \u{2192}");
    click(&mut app, "Wahl");
    assert_eq!(app.correction, springcore::CurvatureCorrection::Wahl);
    click(&mut app, "\u{2190} Calculator");

    // Recompute should have run on SetCorrection; Wahl factor exceeds Bergsträsser
    // at spring index C = mean_dia / wire_dia = 20 / 2 = 10.
    let after = app.outcome.as_ref().unwrap().design.load_points[0]
        .shear_stress
        .pascals();
    assert!(
        after > before,
        "Wahl raises body shear vs the Bergsträsser default at C=10"
    );
}

#[test]
fn save_entry_commits_a_clone_and_closes_the_editor() {
    let mut app = test_app();
    click(&mut app, "Materials →");
    // Clone copies a curated material's (valid) fields and opens the editor.
    click(&mut app, "Clone");
    assert!(shows(&app, "Editing"));

    // Saving the unchanged-but-valid clone commits it and closes the editor.
    click(&mut app, "Save entry");
    assert!(
        shows(&app, "Select a material to edit, or New."),
        "a successful save closes the editor"
    );
    assert!(shows(&app, "saved entry"), "a success status is shown");
    assert_eq!(
        user_material_count(&app),
        1,
        "the saved clone remains in the store"
    );
}

// ── Extension family Simulator tests ─────────────────────────────────────────

/// Focus an extension-spring calculator field's text input and type `text` into
/// it, then apply every resulting message. Mirrors `type_into`, resolving the
/// widget id through the view's `ext_field_id` so the test and the view share a
/// single source of truth for the id strings.
///
/// Note: `typewrite` APPENDS to the focused input's existing content — it does
/// not clear or replace it. Re-typing into a field already populated by a prior
/// call concatenates (e.g. two `"2"` calls yield `"22"`).
fn type_into_ext(app: &mut App, field: ExtField, text: &str) {
    let id = iced_test::core::widget::Id::from(crate::extension::view::ext_field_id(field));
    let mut sim = ui(app);
    sim.click(id)
        .unwrap_or_else(|e| panic!("could not focus ext input for {field:?}: {e}"));
    sim.typewrite(text);
    for message in sim.into_messages() {
        app.update(message);
    }
}

/// Selecting the Extension family, entering a complete PowerUser design, and
/// driving the update loop must produce a solved `ext_outcome` and render the
/// three-stress results panel (Geometry + load-point table).
#[test]
fn ext_solve_flow_renders_results() {
    let mut app = test_app();

    // Default family is Compression; switch to Extension via the family selector.
    app.update(Message::SelectFamily(Family::Extension));
    assert_eq!(app.family, Family::Extension);

    // Empty form starts in the "no results" state.
    assert!(shows(&app, "Enter design parameters to see results."));

    // Enter a valid PowerUser design field-by-field via stable widget IDs.
    type_into_ext(&mut app, ExtField::WireDia, "2.0");
    type_into_ext(&mut app, ExtField::MeanDia, "20.0");
    type_into_ext(&mut app, ExtField::Active, "10");
    type_into_ext(&mut app, ExtField::FreeLength, "60");
    type_into_ext(&mut app, ExtField::InitialTension, "10");
    type_into_ext(&mut app, ExtField::Loads, "10, 30");

    // The form state reflects the typed values.
    assert_eq!(app.extension.wire_dia, "2.0");
    assert_eq!(app.extension.mean_dia, "20.0");
    assert_eq!(app.extension.loads, "10, 30");

    // The full input → solve → render cycle produces a solved extension design.
    assert!(
        app.ext_outcome.is_some(),
        "a valid extension design must produce an ext_outcome"
    );
    assert!(
        matches!(ext_results_view(&app), ExtResultsView::Populated(_)),
        "ext_results_view must be Populated after a successful solve"
    );
    // Geometry section and load-point table are present in the rendered output.
    assert!(
        shows(&app, "Geometry"),
        "results must include the Geometry section"
    );
    assert!(
        !shows(&app, "Enter design parameters to see results."),
        "the empty-state prompt must not appear after a successful solve"
    );
}

/// Switching hook mode from Default to Custom must show the radius inputs and
/// require valid radii for the solve to succeed; switching back to Default must
/// hide the radius inputs and re-solve with machine-loop geometry.
#[test]
fn ext_hook_toggle_shows_radii_and_resolves() {
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Extension));

    // Enter valid geometry so the spring solves under any hook mode.
    type_into_ext(&mut app, ExtField::WireDia, "2.0");
    type_into_ext(&mut app, ExtField::MeanDia, "20.0");
    type_into_ext(&mut app, ExtField::Active, "10");
    type_into_ext(&mut app, ExtField::FreeLength, "60");
    type_into_ext(&mut app, ExtField::InitialTension, "5");
    type_into_ext(&mut app, ExtField::Loads, "50");

    // Default mode: solved and hook radius inputs are not rendered.
    assert_eq!(app.extension.hook_mode, HookMode::Default);
    assert!(
        app.ext_outcome.is_some(),
        "default hook mode must solve with valid geometry"
    );
    assert!(
        !shows(&app, "Hook radius r1 (mm)"),
        "hook radius inputs must not appear in Default mode"
    );

    // Switch to Custom hook mode; blank radii produce a parse error.
    app.update(Message::ExtHookMode(HookMode::Custom));
    assert_eq!(app.extension.hook_mode, HookMode::Custom);
    assert!(
        app.ext_outcome.is_none(),
        "blank custom radii must prevent a solve"
    );
    // The hook radius inputs now render in the widget tree.
    assert!(
        shows(&app, "Hook radius r1 (mm)"),
        "hook radius r1 input must appear in Custom mode"
    );

    // Enter valid custom radii; solve must succeed.
    type_into_ext(&mut app, ExtField::HookR1, "10.0");
    type_into_ext(&mut app, ExtField::HookR2, "5.0");
    assert!(
        app.ext_outcome.is_some(),
        "custom hook mode with valid radii must solve"
    );

    // Toggle back to Default; radius inputs must hide and solve must succeed.
    app.update(Message::ExtHookMode(HookMode::Default));
    assert_eq!(app.extension.hook_mode, HookMode::Default);
    assert!(
        !shows(&app, "Hook radius r1 (mm)"),
        "hook radius inputs must not appear after returning to Default mode"
    );
    assert!(
        app.ext_outcome.is_some(),
        "switching back to Default hook mode must re-solve successfully"
    );
}

/// Solving an extension design, saving it to a temp file, loading it into a
/// fresh App, and recomputing must reproduce the same family and a solved
/// ext_outcome (covers the persistence round-trip through `app.rs`).
///
/// Also verifies (item 1) that the Extension footer renders real Save/Load
/// buttons by asserting they are visible in the widget tree, and (item 8)
/// that the round-trip preserves field values exactly via spec equality.
///
/// Note: `Message::Save`/`Message::Load` dispatch native `rfd` dialogs that
/// cannot run headlessly. Save/load is tested directly via `app.save_to` /
/// `app.load_from`; the `shows()` check below confirms the buttons that wire
/// up those messages are present in the rendered tree.
#[test]
fn ext_save_load_round_trip() {
    let path = std::env::temp_dir().join(format!("osm_ext_{}.toml", std::process::id()));

    // Build and solve a valid extension design.
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Extension));

    // Item 1/2: Extension footer must render real Save/Load buttons.
    assert!(
        shows(&app, "Save design"),
        "Extension footer must show the Save button"
    );
    assert!(
        shows(&app, "Load design"),
        "Extension footer must show the Load button"
    );

    app.update(Message::ExtField(
        crate::extension::form::Field::WireDia,
        "2.0".into(),
    ));
    app.update(Message::ExtField(
        crate::extension::form::Field::MeanDia,
        "20.0".into(),
    ));
    app.update(Message::ExtField(
        crate::extension::form::Field::Active,
        "10".into(),
    ));
    app.update(Message::ExtField(
        crate::extension::form::Field::FreeLength,
        "60".into(),
    ));
    app.update(Message::ExtField(
        crate::extension::form::Field::InitialTension,
        "10".into(),
    ));
    app.update(Message::ExtField(
        crate::extension::form::Field::Loads,
        "10, 30".into(),
    ));
    assert!(app.ext_outcome.is_some(), "design must solve before save");

    // Item 8: capture the spec before saving for round-trip value equality.
    let spec_before = build_spec(&app.extension, UnitSystem::Metric)
        .expect("solved form must produce a valid spec");

    // Save to a process-unique temp path.
    app.save_to(&path);
    assert!(
        app.action_error.is_none(),
        "save must succeed without error"
    );
    assert!(path.exists(), "design file must be written to disk");

    // Load into a fresh app; verify the family and key form fields are populated.
    let mut app2 = test_app();
    let loaded = app2.load_from(&path);
    assert!(loaded, "load_from must return true on success");
    assert_eq!(
        app2.family,
        Family::Extension,
        "loaded family must be Extension"
    );
    assert!(
        !app2.extension.wire_dia.is_empty(),
        "wire_dia must be populated after load"
    );
    assert!(
        !app2.extension.mean_dia.is_empty(),
        "mean_dia must be populated after load"
    );

    // Recompute on the loaded form must yield a solved extension outcome.
    app2.recompute();
    assert!(
        app2.ext_outcome.is_some(),
        "recompute on the loaded extension form must produce an ext_outcome"
    );

    // Item 8: spec equality — the round-trip must preserve all field values exactly.
    let spec_after = build_spec(&app2.extension, UnitSystem::Metric)
        .expect("loaded form must produce a valid spec");
    assert_eq!(
        spec_before, spec_after,
        "save/load round-trip must preserve the full extension spec"
    );

    // Clean up — no temp files must remain in the repo or working directory.
    let _ = std::fs::remove_file(&path);
}

/// Custom-hook save/load round-trip through the full `App::save_to` →
/// `App::load_from` chain: the hook mode and both radii must survive, not just
/// the Default-hook path covered above.
#[test]
fn ext_save_load_round_trip_custom_hooks() {
    let path = std::env::temp_dir().join(format!("osm_ext_custom_{}.toml", std::process::id()));

    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Extension));
    for (field, value) in [
        (crate::extension::form::Field::WireDia, "2.0"),
        (crate::extension::form::Field::MeanDia, "20.0"),
        (crate::extension::form::Field::Active, "10"),
        (crate::extension::form::Field::FreeLength, "60"),
        (crate::extension::form::Field::InitialTension, "10"),
        (crate::extension::form::Field::Loads, "10, 30"),
    ] {
        app.update(Message::ExtField(field, value.into()));
    }

    // Switch to Custom hooks and enter explicit radii.
    app.update(Message::ExtHookMode(HookMode::Custom));
    app.update(Message::ExtField(
        crate::extension::form::Field::HookR1,
        "8".into(),
    ));
    app.update(Message::ExtField(
        crate::extension::form::Field::HookR2,
        "4".into(),
    ));
    assert!(
        app.ext_outcome.is_some(),
        "custom-hook design must solve before save"
    );

    let spec_before = build_spec(&app.extension, UnitSystem::Metric)
        .expect("solved custom-hook form must produce a valid spec");

    app.save_to(&path);
    assert!(
        app.action_error.is_none(),
        "save must succeed without error"
    );

    let mut app2 = test_app();
    assert!(
        app2.load_from(&path),
        "load_from must return true on success"
    );
    assert_eq!(app2.family, Family::Extension);
    assert_eq!(
        app2.extension.hook_mode,
        HookMode::Custom,
        "hook mode must be restored to Custom after load"
    );
    assert_eq!(app2.extension.hook_r1, "8", "hook r1 must be restored");
    assert_eq!(app2.extension.hook_r2, "4", "hook r2 must be restored");

    let spec_after = build_spec(&app2.extension, UnitSystem::Metric)
        .expect("loaded custom-hook form must produce a valid spec");
    assert_eq!(
        spec_before, spec_after,
        "custom-hook save/load round-trip must preserve the full spec"
    );

    let _ = std::fs::remove_file(&path);
}

// ── Torsion family Simulator tests ───────────────────────────────────────────

/// Torsion analog of `type_into_ext`: focus a torsion field by its stable id and
/// type `text`, then apply the resulting messages.
fn type_into_tor(app: &mut App, field: crate::torsion::form::Field, text: &str) {
    let id = iced_test::core::widget::Id::from(crate::torsion::view::tor_field_id(field));
    let mut sim = ui(app);
    sim.click(id)
        .unwrap_or_else(|e| panic!("could not focus torsion input for {field:?}: {e}"));
    sim.typewrite(text);
    for message in sim.into_messages() {
        app.update(message);
    }
}

#[test]
fn torsion_family_solves_end_to_end() {
    use crate::torsion::form::Field as TF;
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Torsion));
    assert_eq!(app.family, Family::Torsion);
    assert!(shows(&app, "Enter design parameters to see results."));

    type_into_tor(&mut app, TF::WireDia, "2");
    type_into_tor(&mut app, TF::MeanDia, "20");
    type_into_tor(&mut app, TF::BodyCoils, "5");
    type_into_tor(&mut app, TF::Leg1, "0");
    type_into_tor(&mut app, TF::Leg2, "0");
    type_into_tor(&mut app, TF::Moments, "1000");

    assert!(app.tor_outcome.is_some(), "torsion design must solve");
    assert!(app.error.is_none());
}

#[test]
fn torsion_save_load_round_trip() {
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Torsion));
    app.torsion = crate::torsion::form::TorFormState {
        wire_dia: "2".into(),
        mean_dia: "20".into(),
        body_coils: "5".into(),
        leg1: "0".into(),
        leg2: "0".into(),
        moments: "1000".into(),
        ..Default::default()
    };
    app.recompute();

    let dir = std::env::temp_dir().join(format!("osm_tor_e2e_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("design.toml");
    app.save_to(&path);

    let mut app2 = test_app();
    assert!(app2.load_from(&path));
    assert_eq!(app2.family, Family::Torsion);
    assert_eq!(app2.torsion.mean_dia, "20");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn torsion_scenario_switch_solves_each_mode() {
    use crate::torsion::form::{Field as TF, TorScenarioKind};
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Torsion));

    // RateBased through real widgets.
    app.update(Message::TorScenario(TorScenarioKind::RateBased));
    type_into_tor(&mut app, TF::WireDia, "2");
    type_into_tor(&mut app, TF::MeanDia, "20");
    type_into_tor(&mut app, TF::Rate, "8.875");
    type_into_tor(&mut app, TF::Leg1, "0");
    type_into_tor(&mut app, TF::Leg2, "0");
    type_into_tor(&mut app, TF::Moments, "1000");
    assert!(app.tor_outcome.is_some(), "RateBased must solve");

    // Dimensional: switch + fill its distinct field (shared fields carry over).
    app.update(Message::TorScenario(TorScenarioKind::Dimensional));
    type_into_tor(&mut app, TF::BodyCoils, "5");
    type_into_tor(&mut app, TF::OuterDia, "22");
    assert!(app.tor_outcome.is_some(), "Dimensional must solve");

    // TwoLoad: switch + the four point fields.
    app.update(Message::TorScenario(TorScenarioKind::TwoLoad));
    type_into_tor(&mut app, TF::Moment1, "508.5");
    type_into_tor(&mut app, TF::Angle1, "57.29578");
    type_into_tor(&mut app, TF::Moment2, "1017");
    type_into_tor(&mut app, TF::Angle2, "114.59156");
    assert!(app.tor_outcome.is_some(), "TwoLoad must solve");
    assert!(app.error.is_none());
}

#[test]
fn torsion_force_at_radius_e2e() {
    use crate::torsion::form::{Field as TF, MomentEntry};
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Torsion));
    type_into_tor(&mut app, TF::WireDia, "2");
    type_into_tor(&mut app, TF::MeanDia, "20");
    type_into_tor(&mut app, TF::BodyCoils, "5");
    type_into_tor(&mut app, TF::Leg1, "0");
    type_into_tor(&mut app, TF::Leg2, "0");
    app.update(Message::TorMomentEntry(MomentEntry::ForceAtRadius));
    type_into_tor(&mut app, TF::Forces, "10");
    type_into_tor(&mut app, TF::LoadRadius, "50");
    assert!(app.tor_outcome.is_some(), "F@r entry must solve");
}

#[test]
fn torsion_mode_save_load_round_trips() {
    use crate::torsion::form::{TorFormState, TorScenarioKind};
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Torsion));
    app.torsion = TorFormState {
        scenario: TorScenarioKind::RateBased,
        wire_dia: "2".into(),
        mean_dia: "20".into(),
        rate: "8.875".into(),
        leg1: "0".into(),
        leg2: "0".into(),
        moments: "1000".into(),
        ..TorFormState::default()
    };
    app.recompute();
    let dir = std::env::temp_dir().join(format!("osm_tor_modes_e2e_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("ratebased.toml");
    app.save_to(&path);
    let mut app2 = test_app();
    assert!(app2.load_from(&path));
    assert_eq!(app2.family, Family::Torsion);
    assert_eq!(app2.torsion.scenario, TorScenarioKind::RateBased);
    assert_eq!(app2.torsion.rate, "8.875");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn legacy_tagless_torsion_file_surfaces_clean_break_error() {
    // A file in the pre-migration flat layout must fail to load with the error in
    // `action_error` (status panel), leaving the current form untouched.
    let legacy = r#"
material = "Music Wire"
unit_system = "Metric"

[design]
family = "Torsion"
wire_dia_mm = 2.0
mean_dia_mm = 20.0
body_coils = 5.0
leg1_mm = 0.0
leg2_mm = 0.0
friction_model = "ShigleyFriction"
moments_nmm = [1000.0]
"#;
    let dir = std::env::temp_dir().join(format!("osm_tor_legacy_e2e_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("legacy.toml");
    std::fs::write(&path, legacy).unwrap();
    let mut app = test_app();
    assert!(!app.load_from(&path), "legacy file must fail to load");
    assert!(
        app.action_error
            .as_deref()
            .is_some_and(|m| m.contains("type")),
        "the clean-break error (missing `type` tag) must surface in action_error"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn torsion_min_weight_e2e_and_save_load() {
    use crate::torsion::form::{Field as TF, TorScenarioKind};
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Torsion));
    app.update(Message::TorScenario(TorScenarioKind::MinWeight));
    app.update(Message::TorFriction(
        springcore::torsion::FrictionModel::PureBending,
    ));
    type_into_tor(&mut app, TF::Rate, "8.875");
    type_into_tor(&mut app, TF::MaxMoment, "100");
    type_into_tor(&mut app, TF::Leg1, "0");
    type_into_tor(&mut app, TF::Leg2, "0");
    type_into_tor(&mut app, TF::CandidateDiameters, "1.5, 2, 2.5");
    let out = app.tor_outcome.as_ref().expect("MinWeight must solve");
    assert!(out.min_weight.is_some(), "the optimisation extra is filled");

    let dir = std::env::temp_dir().join(format!("osm_tor_mw_e2e_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("minweight.toml");
    app.save_to(&path);
    let mut app2 = test_app();
    assert!(app2.load_from(&path));
    assert_eq!(app2.torsion.scenario, TorScenarioKind::MinWeight);
    assert!(app2.torsion.candidate_diameters.contains("1.5"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ext_scenario_switch_solves_each_mode() {
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Extension));

    // RateBased: rate + free length + loads → solves to a standard design.
    app.update(Message::ExtScenario(ExtScenarioKind::RateBased));
    type_into_ext(&mut app, ExtField::WireDia, "2");
    type_into_ext(&mut app, ExtField::MeanDia, "20");
    type_into_ext(&mut app, ExtField::Rate, "2");
    type_into_ext(&mut app, ExtField::FreeLength, "100");
    type_into_ext(&mut app, ExtField::InitialTension, "5");
    type_into_ext(&mut app, ExtField::Loads, "10, 30");
    assert!(
        matches!(ext_results_view(&app), ExtResultsView::Populated(_)),
        "RateBased should render results"
    );

    // MinWeight: rate ("2") and initial_tension ("5") carry over from the RateBased
    // phase above. `type_into_ext`/typewrite APPENDS to existing field content, so
    // re-typing them here would produce "22"/"55" and break the solve — only supply
    // the MinWeight-specific fields (max_force, candidate_diameters) that were blank.
    app.update(Message::ExtScenario(ExtScenarioKind::MinWeight));
    type_into_ext(&mut app, ExtField::MaxForce, "50");
    type_into_ext(&mut app, ExtField::CandidateDiameters, "1.5, 2.0, 2.5");
    match ext_results_view(&app) {
        ExtResultsView::Populated(p) => {
            assert!(
                p.min_weight.is_some(),
                "MinWeight shows the optimisation section"
            );
        }
        other => panic!("expected Populated MinWeight results, got {other:?}"),
    }
}
