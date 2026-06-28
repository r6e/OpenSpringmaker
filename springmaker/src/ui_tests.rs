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
use iced_test::core::Settings;
use iced_test::Simulator;
use springcore::{MaterialSet, MaterialStore};

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
