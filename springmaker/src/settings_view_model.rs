//! Pure presenter for the Settings screen (no iced). Decides what the settings
//! UI shows; `settings_view` renders it. (Humble-view standard, ADR 0008.)

use crate::app::App;
use crate::settings::ThemePref;
use springcore::CurvatureCorrection;

/// One selectable correction option as the view should render it.
pub struct CorrectionOption {
    pub value: CurvatureCorrection,
    pub label: String,
    pub selected: bool,
    /// Whether the view should attach a click handler — see [`clickable`].
    pub clickable: bool,
}

/// One selectable theme-preference option as the view should render it.
pub struct ThemeOption {
    pub value: ThemePref,
    pub label: String,
    pub selected: bool,
    /// Whether the view should attach a click handler — see [`clickable`].
    pub clickable: bool,
}

/// Severity of a save-error feedback line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsFeedbackKind {
    Error,
}

/// A settings-screen feedback line (save error), if any.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsFeedback {
    pub kind: SettingsFeedbackKind,
    pub text: String,
}

/// Rendering decisions for the Settings screen.
pub struct SettingsViewModel {
    pub options: Vec<CorrectionOption>,
    pub theme_options: Vec<ThemeOption>,
    /// Non-`None` when the last settings-save attempt failed.
    pub save_feedback: Option<SettingsFeedback>,
}

/// Whether an option should respond to clicks. Identical rule for every
/// option kind in this screen: an unselected option is always clickable; the
/// currently-selected option becomes clickable too, but ONLY while a save is
/// failing — the one-click retry affordance. This is the single place that
/// decision is made (Task 4): it used to be a view-side `if` (see
/// `settings_view`'s former guard comment); computing it here instead makes
/// the view a true humble view (no logic or branching of its own).
fn clickable(selected: bool, save_feedback_pending: bool) -> bool {
    !selected || save_feedback_pending
}

impl SettingsViewModel {
    /// Build the view model from the currently-active correction and theme
    /// preference.
    pub fn from_app(app: &App) -> Self {
        let active = app.correction;
        let active_theme = app.theme_pref;
        let save_feedback_pending = app.settings_error.is_some();
        let mk = |value, label: &str| {
            let selected = value == active;
            CorrectionOption {
                value,
                label: label.to_string(),
                selected,
                clickable: clickable(selected, save_feedback_pending),
            }
        };
        let mk_theme = |value, label: &str| {
            let selected = value == active_theme;
            ThemeOption {
                value,
                label: label.to_string(),
                selected,
                clickable: clickable(selected, save_feedback_pending),
            }
        };
        let save_feedback = app.settings_error.as_ref().map(|text| SettingsFeedback {
            kind: SettingsFeedbackKind::Error,
            text: text.clone(),
        });
        Self {
            options: vec![
                mk(
                    CurvatureCorrection::Bergstrasser,
                    "Bergsträsser (EN 13906-1 / Shigley default)",
                ),
                mk(CurvatureCorrection::Wahl, "Wahl"),
            ],
            theme_options: vec![
                mk_theme(ThemePref::System, "System"),
                mk_theme(ThemePref::Light, "Light"),
                mk_theme(ThemePref::Dark, "Dark"),
            ],
            save_feedback,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use springcore::{CurvatureCorrection, MaterialSet, MaterialStore};

    fn test_app_with_correction(correction: CurvatureCorrection) -> crate::app::App {
        let mut app = crate::app::App::from_store(
            MaterialStore::new(MaterialSet::load_default()),
            Vec::new(),
            correction,
        );
        app.settings_error = None;
        app
    }

    #[test]
    fn marks_the_active_correction_selected() {
        let app = test_app_with_correction(CurvatureCorrection::Wahl);
        let vm = SettingsViewModel::from_app(&app);
        let wahl = vm
            .options
            .iter()
            .find(|o| o.value == CurvatureCorrection::Wahl)
            .unwrap();
        let berg = vm
            .options
            .iter()
            .find(|o| o.value == CurvatureCorrection::Bergstrasser)
            .unwrap();
        assert!(wahl.selected);
        assert!(!berg.selected);
        // Bergsträsser is presented as the recommended/standard default.
        assert!(berg.label.contains("Bergsträsser"));
        assert!(wahl.label.contains("Wahl"));
    }

    #[test]
    fn offers_exactly_the_two_factors() {
        let app = test_app_with_correction(CurvatureCorrection::Bergstrasser);
        let vm = SettingsViewModel::from_app(&app);
        assert_eq!(vm.options.len(), 2);
    }

    #[test]
    fn save_feedback_is_none_when_no_error() {
        let app = test_app_with_correction(CurvatureCorrection::Bergstrasser);
        let vm = SettingsViewModel::from_app(&app);
        assert!(vm.save_feedback.is_none());
    }

    #[test]
    fn save_feedback_carries_error_text() {
        let mut app = test_app_with_correction(CurvatureCorrection::Bergstrasser);
        app.settings_error = Some("disk full".into());
        let vm = SettingsViewModel::from_app(&app);
        let fb = vm.save_feedback.unwrap();
        assert_eq!(fb.kind, SettingsFeedbackKind::Error);
        assert_eq!(fb.text, "disk full");
    }

    // ── Task 4: theme picker + ViewModel-owned clickability ────────────────

    #[test]
    fn theme_options_marks_the_active_pref_selected() {
        use crate::settings::ThemePref;
        let mut app = test_app_with_correction(CurvatureCorrection::Bergstrasser);
        app.theme_pref = ThemePref::Light;
        let vm = SettingsViewModel::from_app(&app);
        let light = vm
            .theme_options
            .iter()
            .find(|o| o.value == ThemePref::Light)
            .unwrap();
        let system = vm
            .theme_options
            .iter()
            .find(|o| o.value == ThemePref::System)
            .unwrap();
        assert!(light.selected);
        assert!(!system.selected);
    }

    #[test]
    fn theme_options_offers_exactly_three_prefs_with_exact_labels() {
        let app = test_app_with_correction(CurvatureCorrection::Bergstrasser);
        let vm = SettingsViewModel::from_app(&app);
        assert_eq!(vm.theme_options.len(), 3);
        let labels: Vec<&str> = vm.theme_options.iter().map(|o| o.label.as_str()).collect();
        // Copy canon: exact labels, System/Light/Dark order.
        assert_eq!(labels, vec!["System", "Light", "Dark"]);
    }

    /// Clickable rule table (shared by both option kinds — panel Task 4):
    /// selected + no error ⇒ false; selected + error ⇒ true (retry
    /// affordance); unselected ⇒ true (always).
    #[test]
    fn clickable_rule_table_for_correction_options() {
        let app = test_app_with_correction(CurvatureCorrection::Bergstrasser);
        let vm = SettingsViewModel::from_app(&app);
        let selected = vm
            .options
            .iter()
            .find(|o| o.value == CurvatureCorrection::Bergstrasser)
            .unwrap();
        let unselected = vm
            .options
            .iter()
            .find(|o| o.value == CurvatureCorrection::Wahl)
            .unwrap();
        assert!(
            !selected.clickable,
            "selected + no error must not be clickable"
        );
        assert!(unselected.clickable, "unselected must always be clickable");

        let mut app_err = test_app_with_correction(CurvatureCorrection::Bergstrasser);
        app_err.settings_error = Some("disk full".into());
        let vm_err = SettingsViewModel::from_app(&app_err);
        let selected_err = vm_err
            .options
            .iter()
            .find(|o| o.value == CurvatureCorrection::Bergstrasser)
            .unwrap();
        let unselected_err = vm_err
            .options
            .iter()
            .find(|o| o.value == CurvatureCorrection::Wahl)
            .unwrap();
        assert!(
            selected_err.clickable,
            "selected + error must become clickable (retry)"
        );
        assert!(
            unselected_err.clickable,
            "unselected must always be clickable"
        );
    }

    #[test]
    fn clickable_rule_table_for_theme_options() {
        use crate::settings::ThemePref;
        let mut app = test_app_with_correction(CurvatureCorrection::Bergstrasser);
        app.theme_pref = ThemePref::System;
        let vm = SettingsViewModel::from_app(&app);
        let selected = vm
            .theme_options
            .iter()
            .find(|o| o.value == ThemePref::System)
            .unwrap();
        let unselected = vm
            .theme_options
            .iter()
            .find(|o| o.value == ThemePref::Light)
            .unwrap();
        assert!(
            !selected.clickable,
            "selected + no error must not be clickable"
        );
        assert!(unselected.clickable, "unselected must always be clickable");

        app.settings_error = Some("disk full".into());
        let vm_err = SettingsViewModel::from_app(&app);
        let selected_err = vm_err
            .theme_options
            .iter()
            .find(|o| o.value == ThemePref::System)
            .unwrap();
        let unselected_err = vm_err
            .theme_options
            .iter()
            .find(|o| o.value == ThemePref::Light)
            .unwrap();
        assert!(
            selected_err.clickable,
            "selected + error must become clickable (retry)"
        );
        assert!(
            unselected_err.clickable,
            "unselected must always be clickable"
        );
    }
}
