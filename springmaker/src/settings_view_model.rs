//! Pure presenter for the Settings screen (no iced). Decides what the settings
//! UI shows; `settings_view` renders it. (Humble-view standard, ADR 0008.)

use crate::app::App;
use springcore::CurvatureCorrection;

/// One selectable correction option as the view should render it.
pub struct CorrectionOption {
    pub value: CurvatureCorrection,
    pub label: String,
    pub selected: bool,
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
    /// Non-`None` when the last settings-save attempt failed.
    pub save_feedback: Option<SettingsFeedback>,
}

impl SettingsViewModel {
    /// Build the view model from the currently-active correction.
    pub fn from_app(app: &App) -> Self {
        let active = app.correction;
        let mk = |value, label: &str| CorrectionOption {
            value,
            label: label.to_string(),
            selected: value == active,
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
}
