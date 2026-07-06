//! Conical presenters (ADR 0008). Task 1 ships the inputs descriptors and the
//! Empty/Error results states; the Populated arm lands with the full results
//! panel in the next task.

use crate::app::App;
use crate::presenter::{unit_force_label, unit_length_label, FieldDescriptor, StatusLine};

use super::form::Field;

/// Conical results panel state (Populated arrives in Task 2).
#[derive(Debug, Clone, PartialEq)]
pub enum ConResultsView {
    Error(String),
    Empty,
}

/// Results-panel state from app state.
pub fn con_results_view(app: &App) -> ConResultsView {
    if let Some(err) = &app.error {
        return ConResultsView::Error(err.clone());
    }
    ConResultsView::Empty
}

/// The six labeled inputs, in display order.
pub fn con_inputs_view(app: &App) -> Vec<FieldDescriptor<Field>> {
    let len = unit_length_label(app.unit_system);
    let force = unit_force_label(app.unit_system);
    vec![
        FieldDescriptor::new(format!("Wire diameter ({len})"), Field::WireDia),
        FieldDescriptor::new(format!("Large mean diameter ({len})"), Field::LargeMeanDia),
        FieldDescriptor::new(format!("Small mean diameter ({len})"), Field::SmallMeanDia),
        FieldDescriptor::new("Active coils".to_string(), Field::Active),
        FieldDescriptor::new(format!("Free length ({len})"), Field::FreeLength),
        FieldDescriptor::new(format!("Loads ({force}, comma-separated)"), Field::Loads),
    ]
}

/// Status lines (shared prefix + design messages arrive with Populated in Task 2).
pub fn con_status_view(app: &App) -> Vec<StatusLine> {
    crate::presenter::common_status_lines(app)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use springcore::{CurvatureCorrection, Family, MaterialSet, MaterialStore};

    fn store() -> MaterialStore {
        MaterialStore::new(MaterialSet::load_default())
    }

    fn fresh_app() -> App {
        App::from_store(store(), Vec::new(), CurvatureCorrection::Bergstrasser)
    }

    fn fresh_app_conical() -> App {
        let mut app = fresh_app();
        app.family = Family::Conical;
        app
    }

    #[test]
    fn con_results_view_error_when_error_set() {
        let mut app = fresh_app_conical();
        app.error = Some("bad input".to_string());
        assert!(matches!(con_results_view(&app), ConResultsView::Error(_)));
    }

    #[test]
    fn con_results_view_empty_on_fresh_conical() {
        let app = fresh_app_conical();
        assert_eq!(con_results_view(&app), ConResultsView::Empty);
    }
}
