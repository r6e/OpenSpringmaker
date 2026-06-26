//! Pure presenter for the Settings screen (no iced). Decides what the settings
//! UI shows; `settings_view` renders it. (Humble-view standard, ADR 0008.)

use springcore::CurvatureCorrection;

/// One selectable correction option as the view should render it.
pub struct CorrectionOption {
    pub value: CurvatureCorrection,
    pub label: String,
    pub selected: bool,
}

/// Rendering decisions for the Settings screen.
pub struct SettingsViewModel {
    pub options: Vec<CorrectionOption>,
}

impl SettingsViewModel {
    /// Build the view model from the currently-active correction.
    pub fn from_correction(active: CurvatureCorrection) -> Self {
        let mk = |value, label: &str| CorrectionOption {
            value,
            label: label.to_string(),
            selected: value == active,
        };
        Self {
            options: vec![
                mk(
                    CurvatureCorrection::Bergstrasser,
                    "Bergsträsser (EN 13906-1 / Shigley default)",
                ),
                mk(CurvatureCorrection::Wahl, "Wahl"),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use springcore::CurvatureCorrection;

    #[test]
    fn marks_the_active_correction_selected() {
        let vm = SettingsViewModel::from_correction(CurvatureCorrection::Wahl);
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
        let vm = SettingsViewModel::from_correction(CurvatureCorrection::Bergstrasser);
        assert_eq!(vm.options.len(), 2);
    }
}
