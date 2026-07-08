//! Assembly presenter — pure data types + mapping from `App` state.
//! iced-free per ADR 0008. Task 2 replaces the `Empty` arm with `Populated`.

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
