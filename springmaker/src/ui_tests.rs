//! End-to-end GUI tests driving the real view → message → update loop with
//! iced's headless `Simulator` (iced_test). These complement the presenter unit
//! tests (`view_model`) by exercising the actual widget tree: a click resolves
//! against the rendered layout, emits the wired `Message`, and we feed it back
//! through `App::update` exactly as the runtime would.
//!
//! Tests avoid the `Save design` / `Load design` buttons (which open native
//! `rfd` dialogs) and `Save to disk` (which writes the user overlay) — those
//! perform IO and can't run headlessly.

use crate::app::{App, Message, Screen, VisualMode};
use crate::compression::form::Field;
use crate::extension::form::{build_spec, ExtScenarioKind, Field as ExtField, HookMode};
use crate::extension::view_model::{ext_results_view, ExtResultsView};
use crate::plot::CHART_PLACEHOLDER;
use crate::viz::{Orbit, SCENE_PLACEHOLDER};
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

/// Apply an orbit drag delta (as `OrbitCanvas` would publish it — see
/// `viz::canvas3d`) and return the resulting committed orbit. Tests use the
/// RETURNED value for later equality checks rather than a hardcoded target
/// angle, since `Message::Orbit` carries a delta, not an absolute angle —
/// the achieved orbit depends on `app.orbit`'s value before the call.
fn drag_orbit(app: &mut App, dx: f32, dy: f32) -> Orbit {
    app.update(Message::Orbit(dx, dy));
    app.orbit
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
    app.update(Message::TorDiaPolicy(
        springcore::torsion::DiaPolicy::Compact,
    ));
    app.save_to(&path);
    let mut app2 = test_app();
    assert!(app2.load_from(&path));
    assert_eq!(app2.torsion.scenario, TorScenarioKind::MinWeight);
    assert!(app2.torsion.candidate_diameters.contains("1.5"));
    assert_eq!(
        app2.torsion.dia_policy,
        springcore::torsion::DiaPolicy::Compact,
        "policy round-trips through save/load"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn torsion_fatigue_e2e_rows_nodata_and_minweight_suppression() {
    use crate::torsion::form::{Field as TF, TorScenarioKind};
    // Rows render for a computed analysis.
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Torsion));
    type_into_tor(&mut app, TF::WireDia, "2");
    type_into_tor(&mut app, TF::MeanDia, "20");
    type_into_tor(&mut app, TF::BodyCoils, "5");
    type_into_tor(&mut app, TF::Leg1, "0");
    type_into_tor(&mut app, TF::Leg2, "0");
    type_into_tor(&mut app, TF::Moments, "1000");
    type_into_tor(&mut app, TF::FatigueMin, "100");
    type_into_tor(&mut app, TF::FatigueMax, "500");
    let out = app.tor_outcome.as_ref().expect("solves");
    assert!(matches!(
        out.fatigue,
        crate::torsion::form::TorFatigueStatus::Computed(_)
    ));
    assert!(shows(&app, "Gerber FOS"), "the fatigue rows render");

    // NoData note for a material without Table 10-10 data.
    app.update(Message::Material("Oil-Tempered Wire".into()));
    assert!(
        shows(&app, "No fatigue data for this material."),
        "the NoData note renders"
    );

    // Positive control: the fatigue inputs heading IS shown in PowerUser state
    // before the switch — ensures the later absence assert is a real suppression
    // test, not an incidental pass.
    assert!(
        shows(&app, "Fatigue cycle (leave blank to skip)"),
        "fatigue inputs heading must be visible in PowerUser before switching to MinWeight"
    );

    // MinWeight suppression: switch scenario, drive valid inputs so the solve
    // succeeds (field ids and values from the existing MinWeight E2E above),
    // keep fatigue fields filled, then assert both fatigue headings vanish while
    // the optimization section is present.
    app.update(Message::Material("Music Wire".into()));
    app.update(Message::TorFriction(
        springcore::torsion::FrictionModel::PureBending,
    ));
    app.update(Message::TorScenario(TorScenarioKind::MinWeight));
    type_into_tor(&mut app, TF::Rate, "8.875");
    type_into_tor(&mut app, TF::MaxMoment, "100");
    type_into_tor(&mut app, TF::CandidateDiameters, "1.5, 2, 2.5");
    let out = app.tor_outcome.as_ref().expect("MinWeight must solve");
    assert!(
        out.min_weight.is_some(),
        "the optimisation extra must be present"
    );
    assert!(
        shows(&app, "Min-weight optimisation"),
        "the min-weight section must render — confirms the solve succeeded"
    );
    assert!(
        !shows(&app, "Fatigue cycle (leave blank to skip)"),
        "fatigue inputs heading must be absent under MinWeight"
    );
    assert!(
        !shows(&app, "Fatigue analysis"),
        "the Fatigue analysis results heading must be absent under MinWeight"
    );
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

// ── Conical E2E ───────────────────────────────────────────────────────────────

/// Conical analog of `type_into_tor`: focus a conical field by its stable id
/// and type `text`, then apply the resulting messages. Mirrors `type_into_tor`
/// exactly (same idiom, adapted field type and id resolver).
fn type_into_con(app: &mut App, field: crate::conical::form::Field, text: &str) {
    let id = iced_test::core::widget::Id::from(crate::conical::view::con_field_id(field));
    let mut sim = ui(app);
    sim.click(id)
        .unwrap_or_else(|e| panic!("could not focus conical input for {field:?}: {e}"));
    sim.typewrite(text);
    for message in sim.into_messages() {
        app.update(message);
    }
}

#[test]
fn conical_e2e_solve_renders_results_and_footer() {
    use crate::conical::form::Field as CF;
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Conical));
    type_into_con(&mut app, CF::WireDia, "2");
    type_into_con(&mut app, CF::LargeMeanDia, "20");
    type_into_con(&mut app, CF::SmallMeanDia, "12");
    type_into_con(&mut app, CF::Active, "10");
    type_into_con(&mut app, CF::FreeLength, "60");
    type_into_con(&mut app, CF::Loads, "10, 25");
    assert!(app.con_outcome.is_some(), "solve must succeed");
    assert!(shows(&app, "Geometry"));
    assert!(shows(&app, "Large end OD"));
    assert!(
        shows(
            &app,
            "Linear-range model: progressive stiffening as coils bottom out is not modeled."
        ),
        "footer note must render in the results panel"
    );
}

/// The linear-model footer must NOT appear in the Empty state (no inputs).
/// Revert-probe: temporarily add `render_linear_model_footer()` to the Empty
/// arm → test FAILS → restore → green.
#[test]
fn conical_footer_absent_in_empty_state() {
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Conical));
    // No inputs: form is blank → Empty arm → no footer.
    assert!(
        app.con_outcome.is_none(),
        "pre-condition: no outcome without inputs"
    );
    assert!(
        !shows(
            &app,
            "Linear-range model: progressive stiffening as coils bottom out is not modeled."
        ),
        "linear-model footer must not appear in the Empty state"
    );
}

#[test]
fn conical_save_load_round_trip() {
    use crate::conical::form::Field as CF;
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Conical));
    type_into_con(&mut app, CF::WireDia, "2");
    type_into_con(&mut app, CF::LargeMeanDia, "20");
    type_into_con(&mut app, CF::SmallMeanDia, "12");
    type_into_con(&mut app, CF::Active, "10");
    type_into_con(&mut app, CF::FreeLength, "60");
    type_into_con(&mut app, CF::Loads, "10, 25");
    assert!(app.con_outcome.is_some(), "fixture must solve before save");

    // Mirror the torsion round-trip's temp-file + save_to/load_from idiom exactly.
    let dir = std::env::temp_dir().join(format!("osm_con_e2e_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("design.toml");
    app.save_to(&path);

    let mut app2 = test_app();
    assert!(app2.load_from(&path));
    assert_eq!(app2.family, Family::Conical);
    assert_eq!(app2.conical.large_mean_dia, "20");
    // A recompute after load must produce a Populated result.
    app2.recompute();
    assert!(
        app2.con_outcome.is_some(),
        "recompute after load must solve"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ── Assembly E2E ──────────────────────────────────────────────────────────────

/// Focus an assembly member field by its runtime-indexed widget id and type
/// `text`, then apply every resulting message.
///
/// Assembly member fields use runtime-generated `String` ids
/// (`asm_member_field_id(index, field)`) rather than `&'static str` ids.
/// The `labeled_input` `id` param accepts `impl Into<iced::widget::Id>`, and
/// `iced_core::widget::Id` implements `From<String>`, so `Id::from(String)` is
/// the correct construction — `Id::new` takes `&'static str` only.
fn type_into_asm_member(
    app: &mut App,
    index: usize,
    field: crate::assembly::form::MemberField,
    text: &str,
) {
    let id =
        iced_test::core::widget::Id::from(crate::assembly::view::asm_member_field_id(index, field));
    let mut sim = ui(app);
    sim.click(id)
        .unwrap_or_else(|e| panic!("member {index} field {field:?}: {e}"));
    sim.typewrite(text);
    for message in sim.into_messages() {
        app.update(message);
    }
}

/// Filling a two-member assembly via runtime-indexed widget IDs must solve,
/// render the Summary section and per-member headings, and allow removing a
/// member.
#[test]
fn assembly_e2e_dynamic_members_and_results() {
    use crate::assembly::form::MemberField as F;
    let mut app = test_app();
    app.update(Message::SelectFamily(springcore::Family::Assembly));

    // Member 0 is present by default.
    type_into_asm_member(&mut app, 0, F::WireDia, "2");
    type_into_asm_member(&mut app, 0, F::MeanDia, "20");
    type_into_asm_member(&mut app, 0, F::Active, "10");
    type_into_asm_member(&mut app, 0, F::FreeLength, "60");

    // Add member 1 — its indexed ids must resolve on the new row.
    app.update(Message::AsmMemberAdd);
    type_into_asm_member(&mut app, 1, F::WireDia, "1.5");
    type_into_asm_member(&mut app, 1, F::MeanDia, "16");
    type_into_asm_member(&mut app, 1, F::Active, "8");
    type_into_asm_member(&mut app, 1, F::FreeLength, "60");

    app.update(Message::AsmLoads("10, 25".into()));
    assert!(app.asm_outcome.is_some(), "two-member assembly must solve");

    // Summary section, assembly load table, and both member headings must be
    // present in the render.
    assert!(
        shows(&app, "Summary"),
        "populated results must show Summary"
    );
    assert!(
        shows(&app, "Assembly load points"),
        "assembly load table heading must render"
    );
    assert!(
        shows(&app, "Member 1 (Music Wire)"),
        "member 1 heading must render"
    );
    assert!(
        shows(&app, "Member 2 (Music Wire)"),
        "member 2 heading must render"
    );

    // Remove member 2 → back to a single-member form.
    app.update(Message::AsmMemberRemove(1));
    assert_eq!(app.assembly.members.len(), 1, "one member after remove");
}

/// In US mode, a member wire diameter that is out of range for its material
/// must produce an error that identifies the member and reports the measurement
/// in inches (not mm). Tests that the full dispatch path from `type_into_asm_member`
/// through `AsmField → recompute → format_error(US)` works end-to-end.
///
/// Note: `find`/`shows` performs exact text matching; member errors are
/// verified via `app.error` directly (the error text is long and its precise
/// phrasing is already pinned in `form_helpers` unit tests).
#[test]
fn assembly_us_member_diameter_error_renders_in_inches() {
    use crate::assembly::form::MemberField as F;
    let mut app = test_app();
    app.update(Message::Units(springcore::UnitSystem::Us));
    app.update(Message::SelectFamily(springcore::Family::Assembly));

    // 0.4 in ≈ 10.16 mm — outside Music Wire's valid range (max ≈ 0.256 in / 6.5 mm).
    type_into_asm_member(&mut app, 0, F::WireDia, "0.4");
    type_into_asm_member(&mut app, 0, F::MeanDia, "3.0");
    type_into_asm_member(&mut app, 0, F::Active, "10");
    type_into_asm_member(&mut app, 0, F::FreeLength, "8");
    app.update(Message::AsmLoads("2".into()));

    // The formatted error must name the member and render the diameter in inches.
    let err = app
        .error
        .as_deref()
        .expect("must produce an error for out-of-range diameter");
    assert!(
        err.contains("member 1: wire diameter"),
        "error must be scoped to member 1; got: {err:?}"
    );
    assert!(
        err.contains(" in "),
        "error must report diameter in inches (US mode); got: {err:?}"
    );
}

/// Fill a two-member assembly, save_to a temp file, load_from a fresh app,
/// and recompute — the family, member count, and a field value must be
/// restored, and the recompute must yield a solved `asm_outcome`.
#[test]
fn assembly_save_load_round_trip() {
    use crate::assembly::form::MemberField as F;
    let mut app = test_app();
    app.update(Message::SelectFamily(springcore::Family::Assembly));

    // Build a solved two-member assembly via Simulator clicks on indexed ids.
    type_into_asm_member(&mut app, 0, F::WireDia, "2");
    type_into_asm_member(&mut app, 0, F::MeanDia, "20");
    type_into_asm_member(&mut app, 0, F::Active, "10");
    type_into_asm_member(&mut app, 0, F::FreeLength, "60");
    app.update(Message::AsmMemberAdd);
    type_into_asm_member(&mut app, 1, F::WireDia, "1.5");
    type_into_asm_member(&mut app, 1, F::MeanDia, "16");
    type_into_asm_member(&mut app, 1, F::Active, "8");
    type_into_asm_member(&mut app, 1, F::FreeLength, "60");
    app.update(Message::AsmLoads("10, 25".into()));
    assert!(app.asm_outcome.is_some(), "must solve before save");

    // Mirror the conical save/load round-trip's temp-dir + save_to/load_from idiom.
    let dir = std::env::temp_dir().join(format!("osm_asm_e2e_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("design.toml");
    app.save_to(&path);
    assert!(
        app.action_error.is_none(),
        "save must succeed without error"
    );

    let mut app2 = test_app();
    assert!(app2.load_from(&path), "load_from must return true");
    assert_eq!(
        app2.family,
        springcore::Family::Assembly,
        "family restores to Assembly"
    );
    assert_eq!(
        app2.assembly.members.len(),
        2,
        "two members must be restored"
    );
    assert_eq!(
        app2.assembly.members[1].mean_dia, "16",
        "member 2 mean_dia must round-trip"
    );

    // Recompute on the loaded form must yield a solved assembly outcome.
    app2.recompute();
    assert!(
        app2.asm_outcome.is_some(),
        "recompute after load must produce an asm_outcome"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ── Chart + fatigue-chart wiring tests ──────────────────────────────────────

/// Compression: drive the same PowerUser design as
/// `typing_a_valid_power_user_design_renders_results` and confirm the chart
/// placeholder is absent once the design solves.
#[test]
fn compression_chart_renders_after_solve() {
    let mut app = test_app();
    type_into(&mut app, Field::WireDia, "2.0");
    type_into(&mut app, Field::MeanDia, "20.0");
    type_into(&mut app, Field::Active, "10");
    type_into(&mut app, Field::FreeLength, "60");
    type_into(&mut app, Field::Loads, "10, 30");

    assert!(
        shows(&app, "Spring rate"),
        "results must be Populated for the placeholder-absence check to be meaningful"
    );
    assert!(
        !shows(&app, CHART_PLACEHOLDER),
        "a solved compression design must render a real chart, not the placeholder"
    );
}

/// A solved design mutated post-solve into a degenerate state (zero rate —
/// the compression presenter suppresses both lines and markers, so
/// `chart_extent` is `None`) must fall back to the chart placeholder rather
/// than panic or keep showing a stale chart.
#[test]
fn degenerate_design_shows_chart_placeholder() {
    let mut app = test_app();
    type_into(&mut app, Field::WireDia, "2.0");
    type_into(&mut app, Field::MeanDia, "20.0");
    type_into(&mut app, Field::Active, "10");
    type_into(&mut app, Field::FreeLength, "60");
    type_into(&mut app, Field::Loads, "10, 30");

    assert!(
        !shows(&app, CHART_PLACEHOLDER),
        "sanity: the design must solve and render a real chart before mutation"
    );

    app.outcome.as_mut().unwrap().design.rate = springcore::SpringRate::from_newtons_per_meter(0.0);

    assert!(
        shows(&app, CHART_PLACEHOLDER),
        "a degenerate post-solve design must fall back to the chart placeholder"
    );
}

/// Extension: drive the same PowerUser design as `ext_solve_flow_renders_results`.
#[test]
fn ext_chart_renders_after_solve() {
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Extension));

    type_into_ext(&mut app, ExtField::WireDia, "2.0");
    type_into_ext(&mut app, ExtField::MeanDia, "20.0");
    type_into_ext(&mut app, ExtField::Active, "10");
    type_into_ext(&mut app, ExtField::FreeLength, "60");
    type_into_ext(&mut app, ExtField::InitialTension, "10");
    type_into_ext(&mut app, ExtField::Loads, "10, 30");

    assert!(
        matches!(ext_results_view(&app), ExtResultsView::Populated(_)),
        "results must be Populated for the placeholder-absence check to be meaningful"
    );
    assert!(
        !shows(&app, CHART_PLACEHOLDER),
        "a solved extension design must render a real chart, not the placeholder"
    );
}

/// Torsion: drive the same design as `torsion_family_solves_end_to_end`.
#[test]
fn torsion_chart_renders_after_solve() {
    use crate::torsion::form::Field as TF;
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Torsion));

    type_into_tor(&mut app, TF::WireDia, "2");
    type_into_tor(&mut app, TF::MeanDia, "20");
    type_into_tor(&mut app, TF::BodyCoils, "5");
    type_into_tor(&mut app, TF::Leg1, "0");
    type_into_tor(&mut app, TF::Leg2, "0");
    type_into_tor(&mut app, TF::Moments, "1000");

    assert!(app.tor_outcome.is_some(), "torsion design must solve");
    assert!(
        shows(&app, "Geometry"),
        "results must be Populated for the placeholder-absence check to be meaningful"
    );
    assert!(
        !shows(&app, CHART_PLACEHOLDER),
        "a solved torsion design must render a real chart, not the placeholder"
    );
}

/// Conical: drive the same design as `conical_e2e_solve_renders_results_and_footer`.
#[test]
fn conical_chart_renders_after_solve() {
    use crate::conical::form::Field as CF;
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Conical));

    type_into_con(&mut app, CF::WireDia, "2");
    type_into_con(&mut app, CF::LargeMeanDia, "20");
    type_into_con(&mut app, CF::SmallMeanDia, "12");
    type_into_con(&mut app, CF::Active, "10");
    type_into_con(&mut app, CF::FreeLength, "60");
    type_into_con(&mut app, CF::Loads, "10, 25");

    assert!(app.con_outcome.is_some(), "solve must succeed");
    assert!(
        shows(&app, "Geometry"),
        "results must be Populated for the placeholder-absence check to be meaningful"
    );
    assert!(
        !shows(&app, CHART_PLACEHOLDER),
        "a solved conical design must render a real chart, not the placeholder"
    );
}

/// Assembly: drive the same two-member design as
/// `assembly_e2e_dynamic_members_and_results`.
#[test]
fn assembly_chart_renders_after_solve() {
    use crate::assembly::form::MemberField as F;
    let mut app = test_app();
    app.update(Message::SelectFamily(springcore::Family::Assembly));

    type_into_asm_member(&mut app, 0, F::WireDia, "2");
    type_into_asm_member(&mut app, 0, F::MeanDia, "20");
    type_into_asm_member(&mut app, 0, F::Active, "10");
    type_into_asm_member(&mut app, 0, F::FreeLength, "60");
    app.update(Message::AsmMemberAdd);
    type_into_asm_member(&mut app, 1, F::WireDia, "1.5");
    type_into_asm_member(&mut app, 1, F::MeanDia, "16");
    type_into_asm_member(&mut app, 1, F::Active, "8");
    type_into_asm_member(&mut app, 1, F::FreeLength, "60");
    app.update(Message::AsmLoads("10, 25".into()));

    assert!(app.asm_outcome.is_some(), "two-member assembly must solve");
    assert!(
        shows(&app, "Summary"),
        "results must be Populated for the placeholder-absence check to be meaningful"
    );
    assert!(
        !shows(&app, CHART_PLACEHOLDER),
        "a solved assembly design must render a real chart, not the placeholder"
    );
}

/// Simulator-level pin for the compression fatigue chart's Computed gate
/// (the presenter decides the polarity; this pins that the Simulator's
/// wiring reacts to it): solving without cycle forces must show the Skipped
/// note; filling the cycle-force fields and re-solving must both compute the
/// fatigue rows (not fall through to NoData for the wrong reason) and clear
/// the note.
#[test]
fn fatigue_chart_only_when_computed() {
    let mut app = test_app();
    type_into(&mut app, Field::WireDia, "2.0");
    type_into(&mut app, Field::MeanDia, "20.0");
    type_into(&mut app, Field::Active, "10");
    type_into(&mut app, Field::FreeLength, "60");
    type_into(&mut app, Field::Loads, "10, 30");

    assert!(
        shows(&app, "Enter min and max cycle forces to compute fatigue."),
        "the Skipped note must show before cycle forces are entered"
    );

    // Default material is Music Wire, which has cited endurance data — filling
    // the cycle forces must reach Computed, not NoData (which would also clear
    // the Skipped note, masking a broken gate).
    type_into(&mut app, Field::FatigueMin, "10");
    type_into(&mut app, Field::FatigueMax, "30");

    assert!(
        shows(&app, "Goodman FOS"),
        "cycle forces must produce a Computed fatigue result, not NoData"
    );
    assert!(
        !shows(&app, "Enter min and max cycle forces to compute fatigue."),
        "the Skipped note must be gone once fatigue is Computed"
    );
}

// ── 3D visualization wiring pins ─────────────────────────────────────────────

/// Switching the shared visual slot between Chart and Spring3d must swap the
/// rendered pane without disturbing the solved results or falling back to
/// either placeholder in either direction.
///
/// Note: this does NOT assert that the toggle's own "Chart"/"3D" radio labels
/// render, despite that being queryable text on other controls in this app.
/// Unlike `settings_view`'s correction picker (which deliberately renders
/// `button(text(label))` — see its comment — specifically so
/// `iced_test::Simulator` can locate it), the family views' chart/3D toggle
/// uses iced's built-in `radio` widget directly. `iced_widget::radio::Radio`
/// draws its label directly in `draw()` with no child `Text` widget and no
/// `operate()` override, so it never feeds a `Candidate::Text` — the label is
/// structurally undiscoverable via `Simulator::find`/`shows`. (Verified: an
/// earlier version of this test asserted `shows(&app, "Chart")` and failed
/// even though the toggle renders correctly.) Making the toggle control
/// itself queryable would require swapping every family's `radio` for a
/// `button`-based look-alike — a real UI behavior change across five view
/// files, out of this task's scope and requiring its own review.
#[test]
fn visual_toggle_swaps_chart_for_3d() {
    let mut app = test_app();
    type_into(&mut app, Field::WireDia, "2.0");
    type_into(&mut app, Field::MeanDia, "20.0");
    type_into(&mut app, Field::Active, "10");
    type_into(&mut app, Field::FreeLength, "60");
    type_into(&mut app, Field::Loads, "10, 30");
    assert!(app.outcome.is_some(), "fixture must solve before toggling");

    // Switch to the 3D visual: neither placeholder appears, and the
    // populated-proof label survives the swap.
    app.update(Message::Visual(VisualMode::Spring3d));
    assert_eq!(app.results_visual, VisualMode::Spring3d);
    assert!(
        !shows(&app, CHART_PLACEHOLDER),
        "switching to 3D must not surface the chart placeholder"
    );
    assert!(
        !shows(&app, SCENE_PLACEHOLDER),
        "a solved design must render a real 3D scene, not the placeholder"
    );
    assert!(
        shows(&app, "Spring rate"),
        "the results panel must remain populated while the 3D visual is shown"
    );

    // Switch back to Chart: symmetric — no placeholder, results still shown.
    app.update(Message::Visual(VisualMode::Chart));
    assert_eq!(app.results_visual, VisualMode::Chart);
    assert!(
        !shows(&app, CHART_PLACEHOLDER),
        "switching back to Chart must render the real chart, not the placeholder"
    );
    assert!(
        shows(&app, "Spring rate"),
        "the results panel must remain populated after switching back to Chart"
    );
}

/// Dragging the 3D orbit while the Spring3d visual is active must update the
/// committed orbit angles (`Message::Orbit` is published by `OrbitCanvas`)
/// without disturbing the solved results or surfacing either placeholder —
/// `Message::Orbit` recomputes nothing (see `app.rs`'s `update`).
#[test]
fn orbit_message_rerenders_without_disturbing_results() {
    let mut app = test_app();
    type_into(&mut app, Field::WireDia, "2.0");
    type_into(&mut app, Field::MeanDia, "20.0");
    type_into(&mut app, Field::Active, "10");
    type_into(&mut app, Field::FreeLength, "60");
    type_into(&mut app, Field::Loads, "10, 30");
    assert!(app.outcome.is_some(), "fixture must solve before orbiting");

    app.update(Message::Visual(VisualMode::Spring3d));

    let before = app.orbit;
    let target = drag_orbit(&mut app, 40.0, 15.0);

    assert_ne!(
        target, before,
        "the committed orbit must update in response to the drag delta"
    );
    assert_eq!(
        app.orbit, target,
        "the committed orbit must update to the dragged angles"
    );
    assert!(
        shows(&app, "Spring rate"),
        "the results panel must remain populated after an orbit drag"
    );
    assert!(
        !shows(&app, CHART_PLACEHOLDER),
        "an orbit drag must not surface the chart placeholder"
    );
    assert!(
        !shows(&app, SCENE_PLACEHOLDER),
        "an orbit drag must not surface the 3D placeholder"
    );
}

/// Every family must render a real 3D scene (not the placeholder) once its
/// design solves and the user switches to the Spring3d visual — the same
/// non-vacuous double pin (populated-proof label + placeholder absence) as
/// the `*_chart_renders_after_solve` family, reusing each one's drive
/// sequence verbatim.
#[test]
fn every_family_renders_3d_after_solve() {
    // Compression.
    let mut app = test_app();
    type_into(&mut app, Field::WireDia, "2.0");
    type_into(&mut app, Field::MeanDia, "20.0");
    type_into(&mut app, Field::Active, "10");
    type_into(&mut app, Field::FreeLength, "60");
    type_into(&mut app, Field::Loads, "10, 30");
    app.update(Message::Visual(VisualMode::Spring3d));
    assert!(
        shows(&app, "Spring rate"),
        "compression: results must be Populated"
    );
    assert!(
        !shows(&app, SCENE_PLACEHOLDER),
        "compression: a solved design must render a real 3D scene"
    );

    // Extension.
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Extension));
    type_into_ext(&mut app, ExtField::WireDia, "2.0");
    type_into_ext(&mut app, ExtField::MeanDia, "20.0");
    type_into_ext(&mut app, ExtField::Active, "10");
    type_into_ext(&mut app, ExtField::FreeLength, "60");
    type_into_ext(&mut app, ExtField::InitialTension, "10");
    type_into_ext(&mut app, ExtField::Loads, "10, 30");
    app.update(Message::Visual(VisualMode::Spring3d));
    assert!(
        matches!(ext_results_view(&app), ExtResultsView::Populated(_)),
        "extension: results must be Populated"
    );
    assert!(
        !shows(&app, SCENE_PLACEHOLDER),
        "extension: a solved design must render a real 3D scene"
    );

    // Torsion.
    {
        use crate::torsion::form::Field as TF;
        let mut app = test_app();
        app.update(Message::SelectFamily(Family::Torsion));
        type_into_tor(&mut app, TF::WireDia, "2");
        type_into_tor(&mut app, TF::MeanDia, "20");
        type_into_tor(&mut app, TF::BodyCoils, "5");
        type_into_tor(&mut app, TF::Leg1, "0");
        type_into_tor(&mut app, TF::Leg2, "0");
        type_into_tor(&mut app, TF::Moments, "1000");
        app.update(Message::Visual(VisualMode::Spring3d));
        assert!(app.tor_outcome.is_some(), "torsion: design must solve");
        assert!(
            shows(&app, "Geometry"),
            "torsion: results must be Populated"
        );
        assert!(
            !shows(&app, SCENE_PLACEHOLDER),
            "torsion: a solved design must render a real 3D scene"
        );
    }

    // Conical.
    {
        use crate::conical::form::Field as CF;
        let mut app = test_app();
        app.update(Message::SelectFamily(Family::Conical));
        type_into_con(&mut app, CF::WireDia, "2");
        type_into_con(&mut app, CF::LargeMeanDia, "20");
        type_into_con(&mut app, CF::SmallMeanDia, "12");
        type_into_con(&mut app, CF::Active, "10");
        type_into_con(&mut app, CF::FreeLength, "60");
        type_into_con(&mut app, CF::Loads, "10, 25");
        app.update(Message::Visual(VisualMode::Spring3d));
        assert!(app.con_outcome.is_some(), "conical: solve must succeed");
        assert!(
            shows(&app, "Geometry"),
            "conical: results must be Populated"
        );
        assert!(
            !shows(&app, SCENE_PLACEHOLDER),
            "conical: a solved design must render a real 3D scene"
        );
    }

    // Assembly.
    {
        use crate::assembly::form::MemberField as F;
        let mut app = test_app();
        app.update(Message::SelectFamily(springcore::Family::Assembly));
        type_into_asm_member(&mut app, 0, F::WireDia, "2");
        type_into_asm_member(&mut app, 0, F::MeanDia, "20");
        type_into_asm_member(&mut app, 0, F::Active, "10");
        type_into_asm_member(&mut app, 0, F::FreeLength, "60");
        app.update(Message::AsmMemberAdd);
        type_into_asm_member(&mut app, 1, F::WireDia, "1.5");
        type_into_asm_member(&mut app, 1, F::MeanDia, "16");
        type_into_asm_member(&mut app, 1, F::Active, "8");
        type_into_asm_member(&mut app, 1, F::FreeLength, "60");
        app.update(Message::AsmLoads("10, 25".into()));
        app.update(Message::Visual(VisualMode::Spring3d));
        assert!(
            app.asm_outcome.is_some(),
            "assembly: two-member assembly must solve"
        );
        assert!(
            shows(&app, "Summary"),
            "assembly: results must be Populated"
        );
        assert!(
            !shows(&app, SCENE_PLACEHOLDER),
            "assembly: a solved design must render a real 3D scene"
        );
    }
}

/// When a compression design's chart becomes degenerate (rate=0, making
/// chart_extent None), the 3D scene remains valid (it derives only from
/// mean_dia, active_coils, total_coils, pitch, wire_dia; not rate). This
/// test discriminates the Spring3d arm dispatch: if it accidentally calls
/// chart_element instead of scene_element, we'll see the CHART_PLACEHOLDER
/// here. A correctly-dispatched Spring3d arm must call scene_element and
/// render the live 3D scene.
#[test]
fn spring3d_arm_dispatches_scene_not_chart() {
    let mut app = test_app();
    type_into(&mut app, Field::WireDia, "2.0");
    type_into(&mut app, Field::MeanDia, "20.0");
    type_into(&mut app, Field::Active, "10");
    type_into(&mut app, Field::FreeLength, "60");
    type_into(&mut app, Field::Loads, "10, 30");

    assert!(
        !shows(&app, CHART_PLACEHOLDER),
        "sanity: the design must solve and render a real chart before mutation"
    );

    // Mutate the rate to 0, making the chart degenerate while the 3D scene
    // remains valid. If the Spring3d arm calls chart_element by mistake,
    // the degenerate chart will cause CHART_PLACEHOLDER to appear here.
    app.outcome.as_mut().unwrap().design.rate = springcore::SpringRate::from_newtons_per_meter(0.0);

    app.update(Message::Visual(VisualMode::Spring3d));

    assert!(
        !shows(&app, SCENE_PLACEHOLDER),
        "the 3D scene is still valid; Spring3d must dispatch to scene_element, which renders it"
    );

    assert!(
        !shows(&app, CHART_PLACEHOLDER),
        "if Spring3d arm calls chart_element by mistake, the degenerate chart surfaces CHART_PLACEHOLDER here"
    );
}

/// Extension: degenerate chart (rate=0) must not surface placeholders in
/// Spring3d mode — scene dispatch and validity, per the compression template.
#[test]
fn extension_spring3d_arm_dispatches_scene_not_chart() {
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Extension));
    type_into_ext(&mut app, ExtField::WireDia, "2.0");
    type_into_ext(&mut app, ExtField::MeanDia, "20.0");
    type_into_ext(&mut app, ExtField::Active, "10");
    type_into_ext(&mut app, ExtField::FreeLength, "60");
    type_into_ext(&mut app, ExtField::InitialTension, "10");
    type_into_ext(&mut app, ExtField::Loads, "10, 30");

    assert!(
        !shows(&app, CHART_PLACEHOLDER),
        "sanity: the design must solve and render a real chart before mutation"
    );

    app.ext_outcome.as_mut().unwrap().design.rate =
        springcore::SpringRate::from_newtons_per_meter(0.0);

    app.update(Message::Visual(VisualMode::Spring3d));

    assert!(
        !shows(&app, SCENE_PLACEHOLDER),
        "extension: the 3D scene is still valid; Spring3d must dispatch to scene_element"
    );

    assert!(
        !shows(&app, CHART_PLACEHOLDER),
        "extension: if Spring3d arm calls chart_element by mistake, the degenerate chart surfaces CHART_PLACEHOLDER"
    );
}

/// Torsion: degenerate chart (rate=0) must not surface placeholders in
/// Spring3d mode — scene dispatch and validity, per the compression template.
#[test]
fn torsion_spring3d_arm_dispatches_scene_not_chart() {
    use crate::torsion::form::Field as TF;
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Torsion));
    type_into_tor(&mut app, TF::WireDia, "2");
    type_into_tor(&mut app, TF::MeanDia, "20");
    type_into_tor(&mut app, TF::BodyCoils, "5");
    type_into_tor(&mut app, TF::Leg1, "0");
    type_into_tor(&mut app, TF::Leg2, "0");
    type_into_tor(&mut app, TF::Moments, "1000");

    assert!(
        !shows(&app, CHART_PLACEHOLDER),
        "sanity: the design must solve and render a real chart before mutation"
    );

    app.tor_outcome.as_mut().unwrap().design.rate =
        springcore::units::AngularRate::from_newton_meters_per_radian(0.0);

    app.update(Message::Visual(VisualMode::Spring3d));

    assert!(
        !shows(&app, SCENE_PLACEHOLDER),
        "torsion: the 3D scene is still valid; Spring3d must dispatch to scene_element"
    );

    assert!(
        !shows(&app, CHART_PLACEHOLDER),
        "torsion: if Spring3d arm calls chart_element by mistake, the degenerate chart surfaces CHART_PLACEHOLDER"
    );
}

/// Conical: degenerate chart (rate=0) must not surface placeholders in
/// Spring3d mode — scene dispatch and validity, per the compression template.
#[test]
fn conical_spring3d_arm_dispatches_scene_not_chart() {
    use crate::conical::form::Field as CF;
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Conical));
    type_into_con(&mut app, CF::WireDia, "2");
    type_into_con(&mut app, CF::LargeMeanDia, "20");
    type_into_con(&mut app, CF::SmallMeanDia, "12");
    type_into_con(&mut app, CF::Active, "10");
    type_into_con(&mut app, CF::FreeLength, "60");
    type_into_con(&mut app, CF::Loads, "10, 25");

    assert!(
        !shows(&app, CHART_PLACEHOLDER),
        "sanity: the design must solve and render a real chart before mutation"
    );

    app.con_outcome.as_mut().unwrap().design.rate =
        springcore::SpringRate::from_newtons_per_meter(0.0);

    app.update(Message::Visual(VisualMode::Spring3d));

    assert!(
        !shows(&app, SCENE_PLACEHOLDER),
        "conical: the 3D scene is still valid; Spring3d must dispatch to scene_element"
    );

    assert!(
        !shows(&app, CHART_PLACEHOLDER),
        "conical: if Spring3d arm calls chart_element by mistake, the degenerate chart surfaces CHART_PLACEHOLDER"
    );
}

/// Assembly: degenerate chart (rate=0) must not surface placeholders in
/// Spring3d mode — scene dispatch and validity, per the compression template.
#[test]
fn assembly_spring3d_arm_dispatches_scene_not_chart() {
    use crate::assembly::form::MemberField as F;
    let mut app = test_app();
    app.update(Message::SelectFamily(springcore::Family::Assembly));
    type_into_asm_member(&mut app, 0, F::WireDia, "2");
    type_into_asm_member(&mut app, 0, F::MeanDia, "20");
    type_into_asm_member(&mut app, 0, F::Active, "10");
    type_into_asm_member(&mut app, 0, F::FreeLength, "60");
    app.update(Message::AsmMemberAdd);
    type_into_asm_member(&mut app, 1, F::WireDia, "1.5");
    type_into_asm_member(&mut app, 1, F::MeanDia, "16");
    type_into_asm_member(&mut app, 1, F::Active, "8");
    type_into_asm_member(&mut app, 1, F::FreeLength, "60");
    app.update(Message::AsmLoads("10, 25".into()));

    assert!(
        !shows(&app, CHART_PLACEHOLDER),
        "sanity: the design must solve and render a real chart before mutation"
    );

    app.asm_outcome.as_mut().unwrap().rate = springcore::SpringRate::from_newtons_per_meter(0.0);

    app.update(Message::Visual(VisualMode::Spring3d));

    assert!(
        !shows(&app, SCENE_PLACEHOLDER),
        "assembly: the 3D scene is still valid; Spring3d must dispatch to scene_element"
    );

    assert!(
        !shows(&app, CHART_PLACEHOLDER),
        "assembly: if Spring3d arm calls chart_element by mistake, the degenerate chart surfaces CHART_PLACEHOLDER"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// Stateful-UI regression pins, ported from the review panel's adversary probes
// (panel R1, finding 7). Cross-state interactions: mode × family × orbit ×
// units in ONE App instance — the class of bug that per-message unit tests
// can't catch (each of these composes several messages against shared state).
// ═════════════════════════════════════════════════════════════════════════════

/// Drive the compression fixture used by the shipped 3D pins.
fn probe_solve_compression(app: &mut App) {
    type_into(app, Field::WireDia, "2.0");
    type_into(app, Field::MeanDia, "20.0");
    type_into(app, Field::Active, "10");
    type_into(app, Field::FreeLength, "60");
    type_into(app, Field::Loads, "10, 30");
    assert!(app.outcome.is_some(), "compression fixture must solve");
}

/// Drive the extension fixture used by the shipped 3D pins.
fn probe_solve_extension(app: &mut App) {
    type_into_ext(app, ExtField::WireDia, "2.0");
    type_into_ext(app, ExtField::MeanDia, "20.0");
    type_into_ext(app, ExtField::Active, "10");
    type_into_ext(app, ExtField::FreeLength, "60");
    type_into_ext(app, ExtField::InitialTension, "10");
    type_into_ext(app, ExtField::Loads, "10, 30");
    assert!(app.ext_outcome.is_some(), "extension fixture must solve");
}

/// MUST-COVER: switching family tabs while already in Spring3d mode within
/// ONE App instance. Solve A → 3D → custom orbit → switch to B (blank) →
/// solve B → back to A. Orbit and mode are global by design and must survive
/// unchanged; each tab's visual must derive only from that tab's outcome.
#[test]
fn probe_family_tab_switch_while_in_3d_mode() {
    let mut app = test_app();
    probe_solve_compression(&mut app);
    app.update(Message::Visual(VisualMode::Spring3d));
    let custom = drag_orbit(&mut app, 20.0, -85.0);
    assert!(
        !shows(&app, SCENE_PLACEHOLDER),
        "A: solved 3D scene renders"
    );
    assert!(shows(&app, "Spring rate"), "A: populated results");

    // A → B with B's form blank: Empty arm, no visual, no stale A content.
    app.update(Message::SelectFamily(Family::Extension));
    assert!(
        app.outcome.is_none(),
        "A's outcome is cleared on tab switch"
    );
    assert!(
        shows(&app, "Enter design parameters to see results."),
        "B blank: Empty arm text"
    );
    assert!(
        !shows(&app, SCENE_PLACEHOLDER) && !shows(&app, CHART_PLACEHOLDER),
        "B blank: neither placeholder may leak into the Empty arm"
    );
    assert!(
        !shows(&app, "Spring rate"),
        "B blank: no stale populated panel from tab A"
    );
    assert_eq!(
        app.results_visual,
        VisualMode::Spring3d,
        "mode follows the user"
    );
    assert_eq!(app.orbit, custom, "orbit is untouched by the tab switch");

    // Solve B while already in 3D mode: B's own scene, same orbit.
    probe_solve_extension(&mut app);
    assert!(
        matches!(ext_results_view(&app), ExtResultsView::Populated(_)),
        "B: populated after solve"
    );
    assert!(
        !shows(&app, SCENE_PLACEHOLDER),
        "B: 3D scene renders on a tab entered already in Spring3d mode"
    );
    assert_eq!(
        app.orbit, custom,
        "solving B must not corrupt the shared orbit"
    );
    assert_eq!(app.results_visual, VisualMode::Spring3d);

    // B → A: A re-solves from its persisted form; 3D still correct.
    app.update(Message::SelectFamily(Family::Compression));
    assert!(app.outcome.is_some(), "A re-solves from its persisted form");
    assert!(
        app.ext_outcome.is_none(),
        "B's outcome cleared symmetrically"
    );
    assert!(shows(&app, "Spring rate"), "A: populated again");
    assert!(
        !shows(&app, SCENE_PLACEHOLDER),
        "A: 3D scene renders again after the round trip"
    );
    assert_eq!(
        app.orbit, custom,
        "orbit survives the full A→B→A round trip"
    );
    assert_eq!(app.results_visual, VisualMode::Spring3d);
}

/// Sweep all five family tabs in Spring3d mode with every non-compression
/// form blank: every tab renders the Empty arm (no visual, no placeholder,
/// no panic), and returning to the solved tab restores its 3D scene.
#[test]
fn probe_all_tabs_render_in_3d_mode_with_blank_forms() {
    let mut app = test_app();
    probe_solve_compression(&mut app);
    app.update(Message::Visual(VisualMode::Spring3d));
    for fam in [
        Family::Extension,
        Family::Torsion,
        Family::Conical,
        Family::Assembly,
    ] {
        app.update(Message::SelectFamily(fam));
        assert!(
            shows(&app, "Enter design parameters to see results."),
            "{fam:?}: blank tab shows the Empty arm in 3D mode"
        );
        assert!(
            !shows(&app, SCENE_PLACEHOLDER) && !shows(&app, CHART_PLACEHOLDER),
            "{fam:?}: no placeholder leaks on a blank tab in 3D mode"
        );
    }
    app.update(Message::SelectFamily(Family::Compression));
    assert!(app.outcome.is_some());
    assert!(!shows(&app, SCENE_PLACEHOLDER), "compression 3D restored");
}

/// Solve → 3D → corrupt an input (re-solve fails) → the Error arm must show,
/// with no stale scene and no placeholder; then in reverse for Chart mode.
/// Symmetric: the same sequence in Chart mode ends in the same Error arm.
#[test]
fn probe_resolve_to_invalid_in_3d_shows_error_not_stale_scene() {
    let mut app = test_app();
    probe_solve_compression(&mut app);
    app.update(Message::Visual(VisualMode::Spring3d));
    assert!(!shows(&app, SCENE_PLACEHOLDER));

    // Any 'x' inserted anywhere in "2.0" breaks the f64 parse.
    type_into(&mut app, Field::WireDia, "x");
    assert!(
        app.outcome.is_none(),
        "corrupted input must clear the outcome"
    );
    assert!(
        app.error.is_some(),
        "corrupted input must set the solve error"
    );
    assert!(
        !shows(&app, "Spring rate"),
        "3D mode: no stale populated panel after the failing re-solve"
    );
    assert!(
        !shows(&app, SCENE_PLACEHOLDER) && !shows(&app, CHART_PLACEHOLDER),
        "3D mode: the Error arm renders the message, not a placeholder"
    );

    // Same sequence in Chart mode (fresh app) must land in the same arm.
    let mut chart_app = test_app();
    probe_solve_compression(&mut chart_app);
    type_into(&mut chart_app, Field::WireDia, "x");
    assert_eq!(
        chart_app.outcome.is_none(),
        app.outcome.is_none(),
        "symmetry: both modes end with the same outcome state"
    );
    assert!(
        !shows(&chart_app, CHART_PLACEHOLDER) && !shows(&chart_app, SCENE_PLACEHOLDER),
        "Chart mode: the Error arm renders the message, not a placeholder"
    );
}

/// Orbit angles must survive a 3D → Chart → 3D round trip (committed angles
/// are App state, not canvas state), and rapid toggling must not drift state
/// or panic across renders.
#[test]
fn probe_orbit_survives_mode_roundtrip_and_rapid_toggle() {
    let mut app = test_app();
    probe_solve_compression(&mut app);
    app.update(Message::Visual(VisualMode::Spring3d));
    let custom = drag_orbit(&mut app, -290.0, 105.0);

    app.update(Message::Visual(VisualMode::Chart));
    assert!(
        !shows(&app, CHART_PLACEHOLDER),
        "chart renders mid-roundtrip"
    );
    app.update(Message::Visual(VisualMode::Spring3d));
    assert_eq!(app.orbit, custom, "orbit survives the mode round trip");
    assert!(
        !shows(&app, SCENE_PLACEHOLDER),
        "3D renders after round trip"
    );

    // Rapid toggling with a full render each time: no panic, no state drift.
    for _ in 0..4 {
        app.update(Message::Visual(VisualMode::Chart));
        assert!(shows(&app, "Spring rate"));
        app.update(Message::Visual(VisualMode::Spring3d));
        assert!(shows(&app, "Spring rate"));
    }
    assert_eq!(app.orbit, custom, "orbit untouched by rapid toggling");
    assert_eq!(app.results_visual, VisualMode::Spring3d);
    assert!(
        app.outcome.is_some(),
        "toggling never recomputes or clears results"
    );
}

/// US↔SI while in 3D mode: `Message::Units` recomputes with the form text
/// reinterpreted. Whatever arm results, the two visual modes must land in
/// the SAME arm (symmetry), the mode/orbit must be preserved, and flipping
/// back to Metric must restore the solved 3D scene.
#[test]
fn probe_units_toggle_while_in_3d_mode() {
    let mut threed = test_app();
    probe_solve_compression(&mut threed);
    threed.update(Message::Visual(VisualMode::Spring3d));
    let custom = drag_orbit(&mut threed, -50.0, 65.0);

    let mut chart = test_app();
    probe_solve_compression(&mut chart);

    threed.update(Message::Units(UnitSystem::Us));
    chart.update(Message::Units(UnitSystem::Us));

    assert_eq!(
        threed.outcome.is_some(),
        chart.outcome.is_some(),
        "symmetry: unit reinterpretation must resolve identically in both modes"
    );
    assert_eq!(
        threed.results_visual,
        VisualMode::Spring3d,
        "units must not reset the mode"
    );
    assert_eq!(threed.orbit, custom, "units must not reset the orbit");
    // Render both without panic; if unsolved, neither placeholder shows (Error arm).
    if threed.outcome.is_none() {
        assert!(
            !shows(&threed, SCENE_PLACEHOLDER) && !shows(&threed, CHART_PLACEHOLDER),
            "US reinterpretation failure renders the Error arm, not a placeholder"
        );
    } else {
        assert!(
            !shows(&threed, SCENE_PLACEHOLDER),
            "US-solved design renders 3D"
        );
    }

    threed.update(Message::Units(UnitSystem::Metric));
    assert!(
        threed.outcome.is_some(),
        "metric flip-back re-solves the same form"
    );
    assert!(
        !shows(&threed, SCENE_PLACEHOLDER),
        "3D restored after the unit round trip"
    );
    assert_eq!(threed.orbit, custom);
}

/// `Message::Orbit` arriving with no results at all (blank form): angles are
/// stored, nothing else moves, and the blank view still renders.
#[test]
fn probe_orbit_message_on_blank_form_is_harmless() {
    let mut app = test_app();
    app.update(Message::Visual(VisualMode::Spring3d));
    let before = app.orbit;
    let custom = drag_orbit(&mut app, 210.0, -165.0);
    assert_ne!(
        custom, before,
        "the stray orbit message must still change the committed orbit"
    );
    assert!(app.outcome.is_none() && app.error.is_none() && app.action_error.is_none());
    assert!(
        shows(&app, "Enter design parameters to see results."),
        "blank tab still renders the Empty arm after a stray orbit message"
    );
    assert!(!shows(&app, SCENE_PLACEHOLDER) && !shows(&app, CHART_PLACEHOLDER));
}

/// Assembly: topology and member-count changes while in 3D mode. A blank new
/// member fails the re-solve (Error arm — no stale scene); filling it
/// restores the 3D scene; topology flips re-render without disturbing mode.
#[test]
fn probe_assembly_member_and_topology_changes_in_3d_mode() {
    use crate::assembly::form::MemberField as F;
    let mut app = test_app();
    app.update(Message::SelectFamily(Family::Assembly));
    type_into_asm_member(&mut app, 0, F::WireDia, "2");
    type_into_asm_member(&mut app, 0, F::MeanDia, "20");
    type_into_asm_member(&mut app, 0, F::Active, "10");
    type_into_asm_member(&mut app, 0, F::FreeLength, "60");
    app.update(Message::AsmMemberAdd);
    type_into_asm_member(&mut app, 1, F::WireDia, "1.5");
    type_into_asm_member(&mut app, 1, F::MeanDia, "16");
    type_into_asm_member(&mut app, 1, F::Active, "8");
    type_into_asm_member(&mut app, 1, F::FreeLength, "60");
    app.update(Message::AsmLoads("10, 25".into()));
    assert!(app.asm_outcome.is_some(), "two-member assembly solves");
    app.update(Message::Visual(VisualMode::Spring3d));
    assert!(!shows(&app, SCENE_PLACEHOLDER), "assembly 3D renders");

    // Topology change while in 3D: re-solve, scene re-renders.
    app.update(Message::AsmTopology("series".into()));
    assert!(app.asm_outcome.is_some(), "series topology still solves");
    assert!(!shows(&app, SCENE_PLACEHOLDER), "series 3D renders");

    // Adding a blank member fails the re-solve: Error arm, no stale scene.
    app.update(Message::AsmMemberAdd);
    assert!(
        app.asm_outcome.is_none(),
        "blank third member fails the solve"
    );
    assert!(
        !shows(&app, "Summary"),
        "no stale populated assembly panel in 3D mode"
    );
    assert!(
        !shows(&app, SCENE_PLACEHOLDER) && !shows(&app, CHART_PLACEHOLDER),
        "assembly Error arm renders the message, not a placeholder"
    );

    // Filling the member restores the 3D scene; removing it works too.
    type_into_asm_member(&mut app, 2, F::WireDia, "1");
    type_into_asm_member(&mut app, 2, F::MeanDia, "12");
    type_into_asm_member(&mut app, 2, F::Active, "6");
    type_into_asm_member(&mut app, 2, F::FreeLength, "60");
    assert!(app.asm_outcome.is_some(), "filled third member solves");
    assert!(!shows(&app, SCENE_PLACEHOLDER), "three-member 3D renders");
    app.update(Message::AsmMemberRemove(2));
    assert!(
        app.asm_outcome.is_some(),
        "back to two members still solves"
    );
    assert!(
        !shows(&app, SCENE_PLACEHOLDER),
        "two-member 3D renders again"
    );
    assert_eq!(app.results_visual, VisualMode::Spring3d);
}

/// The toggle must swap ONLY the load-deflection slot: the fatigue analysis
/// (rows + chart region) renders in BOTH visual modes on the families that
/// have it (compression and torsion).
#[test]
fn probe_fatigue_region_renders_in_both_visual_modes() {
    // Compression.
    let mut app = test_app();
    probe_solve_compression(&mut app);
    type_into(&mut app, Field::FatigueMin, "10");
    type_into(&mut app, Field::FatigueMax, "30");
    assert!(
        shows(&app, "Goodman FOS"),
        "compression fatigue rows in Chart mode"
    );
    app.update(Message::Visual(VisualMode::Spring3d));
    assert!(
        shows(&app, "Goodman FOS"),
        "compression fatigue rows must survive the switch to 3D"
    );
    assert!(
        !shows(&app, CHART_PLACEHOLDER),
        "the fatigue chart (a chart!) must still render fine in 3D mode"
    );
    assert!(!shows(&app, SCENE_PLACEHOLDER));

    // Torsion.
    {
        use crate::torsion::form::Field as TF;
        let mut app = test_app();
        app.update(Message::SelectFamily(Family::Torsion));
        type_into_tor(&mut app, TF::WireDia, "2");
        type_into_tor(&mut app, TF::MeanDia, "20");
        type_into_tor(&mut app, TF::BodyCoils, "5");
        type_into_tor(&mut app, TF::Leg1, "0");
        type_into_tor(&mut app, TF::Leg2, "0");
        type_into_tor(&mut app, TF::Moments, "1000");
        type_into_tor(&mut app, TF::FatigueMin, "100");
        type_into_tor(&mut app, TF::FatigueMax, "500");
        assert!(
            shows(&app, "Gerber FOS"),
            "torsion fatigue rows in Chart mode"
        );
        app.update(Message::Visual(VisualMode::Spring3d));
        assert!(
            shows(&app, "Gerber FOS"),
            "torsion fatigue rows must survive the switch to 3D"
        );
        assert!(!shows(&app, CHART_PLACEHOLDER) && !shows(&app, SCENE_PLACEHOLDER));
    }
}

/// API-contract: `Message::Visual` must be a pure mode flip — it must not
/// clear `action_error` (no recompute) and must not clear a solve error.
#[test]
fn probe_visual_message_preserves_error_channels() {
    let mut app = test_app();
    probe_solve_compression(&mut app);
    app.action_error = Some("sentinel".into());
    app.update(Message::Visual(VisualMode::Spring3d));
    assert_eq!(
        app.action_error.as_deref(),
        Some("sentinel"),
        "Visual must not recompute (recompute clears action_error)"
    );
    // Solve error preserved across a mode flip, in both directions.
    let mut bad = test_app();
    type_into(&mut bad, Field::WireDia, "oops");
    assert!(bad.error.is_some());
    let err = bad.error.clone();
    bad.update(Message::Visual(VisualMode::Spring3d));
    assert_eq!(bad.error, err, "solve error survives Chart→3D");
    bad.update(Message::Visual(VisualMode::Chart));
    assert_eq!(bad.error, err, "solve error survives 3D→Chart");
}

/// Units flip in 3D mode with a design valid in BOTH systems: the outcome
/// re-solves under the new interpretation and the 3D scene re-renders from
/// the NEW design (mode and orbit untouched) — the stays-solved counterpart
/// to the Error-arm case in `probe_units_toggle_while_in_3d_mode`.
#[test]
fn probe_units_toggle_in_3d_stays_solved_when_valid_both_ways() {
    let mut app = test_app();
    // wire 0.2 (mm|in) is inside Music Wire's valid range both ways:
    // 0.2 mm >= 0.1 mm and 0.2 in = 5.08 mm <= 6.5 mm.
    type_into(&mut app, Field::WireDia, "0.2");
    type_into(&mut app, Field::MeanDia, "2");
    type_into(&mut app, Field::Active, "10");
    type_into(&mut app, Field::FreeLength, "6");
    type_into(&mut app, Field::Loads, "0.5, 1");
    assert!(app.outcome.is_some(), "metric interpretation must solve");
    app.update(Message::Visual(VisualMode::Spring3d));
    let custom = drag_orbit(&mut app, -20.0, -55.0);
    assert!(!shows(&app, SCENE_PLACEHOLDER));
    let metric_rate = app.outcome.as_ref().unwrap().design.rate;

    app.update(Message::Units(UnitSystem::Us));
    assert!(
        app.outcome.is_some(),
        "US interpretation must also solve: {:?}",
        app.error
    );
    assert_ne!(
        app.outcome.as_ref().unwrap().design.rate,
        metric_rate,
        "the US-reinterpreted design is a genuinely different design"
    );
    assert_eq!(app.results_visual, VisualMode::Spring3d);
    assert_eq!(app.orbit, custom);
    assert!(
        !shows(&app, SCENE_PLACEHOLDER),
        "the 3D scene re-renders from the re-solved US design"
    );
    assert!(shows(&app, "Spring rate"), "populated panel under US units");
}
