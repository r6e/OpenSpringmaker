//! Presenter (view-model) for the materials editor.
//!
//! Pure functions that turn `App` state into plain data describing *what* the
//! editor should show — which list rows and their actions, the edit-panel
//! field visibility, and the feedback line. No iced dependency, so each
//! decision (curated rows are clone-only, field visibility, error-over-status
//! precedence) is unit-testable without a renderer; `materials_view` renders
//! this data verbatim.

use crate::app::{App, EditTarget};
use crate::materials_form::coefficient_labels;

/// Provenance badge shown on a material list row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Badge {
    Curated,
    User,
}

/// One material row in the list, with the actions it offers.
#[derive(Debug)]
pub struct RowView {
    pub name: String,
    pub badge: Badge,
    /// Clone is always available (any material can seed a new user copy).
    pub can_clone: bool,
    /// Edit/remove apply to user materials only (curated are read-only).
    pub can_edit: bool,
    pub can_remove: bool,
}

/// Severity of the editor feedback line (the view shows error over status).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackKind {
    Error,
    Status,
}

/// The single feedback line to show, if any.
#[derive(Debug, Clone, PartialEq)]
pub struct Feedback {
    pub kind: FeedbackKind,
    pub text: String,
}

/// Presentation of the edit panel (absent when no material is being edited).
#[derive(Debug, Clone, PartialEq)]
pub struct EditPanelView {
    /// True when creating a new material ("New material"), false when editing.
    pub is_new: bool,
    /// Hint shown beside the coefficients input, e.g. "Coefficients (A …, m)".
    pub coefficient_hint: String,
    /// Endurance Ssa/Ssm/peened fields are shown only when enabled.
    pub show_endurance_fields: bool,
    /// Max-service-temperature field is shown only when enabled.
    pub show_max_temp_field: bool,
}

/// Rows for the materials list, in store order.
pub fn list_rows(app: &App) -> Vec<RowView> {
    app.materials
        .names()
        .iter()
        .map(|name| {
            let curated = app.materials.is_curated(name);
            RowView {
                name: name.to_string(),
                badge: if curated { Badge::Curated } else { Badge::User },
                can_clone: true,
                can_edit: !curated,
                can_remove: !curated,
            }
        })
        .collect()
}

/// The feedback line to display: an error takes priority over a success status
/// (mirrors the view's render order, so a lingering error can't be hidden).
pub fn feedback(app: &App) -> Option<Feedback> {
    if let Some(text) = &app.mat_error {
        Some(Feedback {
            kind: FeedbackKind::Error,
            text: text.clone(),
        })
    } else {
        app.mat_status.as_ref().map(|text| Feedback {
            kind: FeedbackKind::Status,
            text: text.clone(),
        })
    }
}

/// The edit-panel presentation, or `None` when the editor is closed.
pub fn edit_panel(app: &App) -> Option<EditPanelView> {
    app.editing.as_ref().map(|target| EditPanelView {
        is_new: matches!(target, EditTarget::New),
        coefficient_hint: format!(
            "Coefficients ({})",
            coefficient_labels(app.mat_form.mts_form).join(", ")
        ),
        show_endurance_fields: app.mat_form.has_endurance,
        show_max_temp_field: app.mat_form.has_max_temp,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Message;
    use springcore::{MaterialSet, MaterialStore, MtsForm};

    /// Hermetic App: curated-only store, no on-disk overlay (no filesystem IO).
    fn test_app() -> App {
        App::from_store(MaterialStore::new(MaterialSet::load_default()), Vec::new())
    }

    fn row<'a>(rows: &'a [RowView], name: &str) -> &'a RowView {
        rows.iter().find(|r| r.name == name).expect("row present")
    }

    #[test]
    fn curated_rows_are_clone_only() {
        let rows = list_rows(&test_app());
        let mw = row(&rows, "Music Wire");
        assert_eq!(mw.badge, Badge::Curated);
        assert!(mw.can_clone);
        assert!(!mw.can_edit, "curated must not be editable");
        assert!(!mw.can_remove, "curated must not be removable");
    }

    #[test]
    fn user_rows_offer_all_actions() {
        let mut a = test_app();
        a.update(Message::MatClone("Music Wire".into()));
        let rows = list_rows(&a);
        let copy = rows
            .iter()
            .find(|r| r.badge == Badge::User)
            .expect("a user material exists after clone");
        assert!(copy.can_clone && copy.can_edit && copy.can_remove);
    }

    #[test]
    fn feedback_is_none_when_clean() {
        assert_eq!(feedback(&test_app()), None);
    }

    #[test]
    fn feedback_prioritizes_error_over_status() {
        let mut a = test_app();
        a.mat_status = Some("saved".into());
        a.mat_error = Some("boom".into());
        let fb = feedback(&a).unwrap();
        assert_eq!(fb.kind, FeedbackKind::Error);
        assert_eq!(fb.text, "boom");
    }

    #[test]
    fn feedback_shows_status_when_no_error() {
        let mut a = test_app();
        a.mat_status = Some("saved entry".into());
        let fb = feedback(&a).unwrap();
        assert_eq!(fb.kind, FeedbackKind::Status);
        assert_eq!(fb.text, "saved entry");
    }

    #[test]
    fn edit_panel_absent_until_editing() {
        assert_eq!(edit_panel(&test_app()), None);
    }

    #[test]
    fn edit_panel_new_flag_and_coefficient_hint() {
        let mut a = test_app();
        a.update(Message::MatNew);
        let ep = edit_panel(&a).expect("editor open after New");
        assert!(ep.is_new);
        // Default form is PowerLaw -> its coefficient labels in the hint.
        assert_eq!(
            ep.coefficient_hint,
            format!(
                "Coefficients ({})",
                coefficient_labels(MtsForm::PowerLaw).join(", ")
            )
        );
    }

    #[test]
    fn coefficient_hint_tracks_selected_form() {
        let mut a = test_app();
        a.update(Message::MatNew);
        // Switch away from the PowerLaw default; the hint must follow the
        // current mts_form, not a value captured when the editor opened.
        a.update(Message::MatFormKind(MtsForm::Constant));
        let ep = edit_panel(&a).expect("editor open");
        // Hint reflects Constant's labels, not the PowerLaw defaults the editor
        // opened with — i.e. it is recomputed from the current form.
        assert_eq!(
            ep.coefficient_hint,
            format!(
                "Coefficients ({})",
                coefficient_labels(MtsForm::Constant).join(", ")
            )
        );
    }

    #[test]
    fn edit_panel_toggles_drive_field_visibility() {
        let mut a = test_app();
        a.update(Message::MatNew);
        assert!(!edit_panel(&a).unwrap().show_endurance_fields);
        assert!(!edit_panel(&a).unwrap().show_max_temp_field);
        a.update(Message::MatToggleEndurance(true));
        a.update(Message::MatToggleMaxTemp(true));
        let ep = edit_panel(&a).unwrap();
        assert!(ep.show_endurance_fields);
        assert!(ep.show_max_temp_field);
    }

    #[test]
    fn edit_panel_is_new_false_when_editing_existing() {
        let mut a = test_app();
        a.update(Message::MatClone("Music Wire".into()));
        // Clone opens the editor on the copy as an existing user material.
        assert!(!edit_panel(&a).unwrap().is_new);
    }
}
