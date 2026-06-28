//! Pure presenter (view-model) for the extension spring calculator screen.
//!
//! No iced dependency — every decision (which results mode, unit conversions,
//! status severity mapping) is unit-testable without a renderer.
use crate::app::App;
use crate::extension::form::Field;
use crate::presenter::{
    display_force, display_len, display_rate, status_kind, unit_force_label, unit_length_label,
    unit_rate_label, FieldDescriptor, GoverningRate, ResultRow, StatusKind, StatusLine,
};
use springcore::extension::ExtensionDesign;

// ── Results panel ────────────────────────────────────────────────────────────

/// The three mutually-exclusive states of the extension results panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtResultsView {
    /// Inputs are empty or invalid; nothing to show.
    Empty,
    /// A parse/solve error.
    Error(String),
    /// A solved design with geometry ready to render.
    Populated(Box<ExtPopulatedResults>),
}

/// Everything the extension results panel shows when a design is solved.
/// (load_table is added in Task 7.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtPopulatedResults {
    pub governing_rate: GoverningRate,
    pub geometry: Vec<ResultRow>,
}

/// Build the extension results panel view model from app state.
///
/// Mirrors the compression `results_view` logic: a solved outcome takes
/// priority over an error string (the two are mutually exclusive after any
/// recompute); blank state with neither is Empty.
pub fn ext_results_view(app: &App) -> ExtResultsView {
    match &app.ext_outcome {
        Some(out) => {
            let us = app.unit_system;
            let rate = display_rate(out.design.rate, us);
            let governing_rate = GoverningRate {
                value: format!("{rate:.4}"),
                unit: unit_rate_label(us).to_string(),
            };
            ExtResultsView::Populated(Box::new(ExtPopulatedResults {
                governing_rate,
                geometry: geometry_rows(&out.design, us),
            }))
        }
        None => match &app.error {
            Some(err) => ExtResultsView::Error(err.clone()),
            None => ExtResultsView::Empty,
        },
    }
}

/// Geometry result rows for a solved extension design.
pub fn geometry_rows(d: &ExtensionDesign, us: springcore::UnitSystem) -> Vec<ResultRow> {
    let len = unit_length_label(us);
    let force = unit_force_label(us);
    let rate = unit_rate_label(us);
    vec![
        ResultRow::new("Spring index", format!("{:.3}", d.index), ""),
        ResultRow::new("Active coils", format!("{:.3}", d.active_coils), ""),
        ResultRow::new("Rate", format!("{:.4}", display_rate(d.rate, us)), rate),
        ResultRow::new(
            "Free length",
            format!("{:.4}", display_len(d.free_length, us)),
            len,
        ),
        ResultRow::new(
            "Outer diameter",
            format!("{:.4}", display_len(d.outer_dia, us)),
            len,
        ),
        ResultRow::new(
            "Inner diameter",
            format!("{:.4}", display_len(d.inner_dia, us)),
            len,
        ),
        ResultRow::new(
            "Initial tension",
            format!("{:.4}", display_force(d.initial_tension, us)),
            force,
        ),
    ]
}

// ── Status panel ─────────────────────────────────────────────────────────────

/// Status lines for the extension family: mirrors compression `status_view`.
pub fn ext_status_view(app: &App) -> Vec<StatusLine> {
    let mut lines = Vec::new();
    if let Some(text) = &app.action_error {
        lines.push(StatusLine {
            kind: StatusKind::ActionError,
            text: text.clone(),
        });
    }
    for warn in &app.load_warnings {
        lines.push(StatusLine {
            kind: StatusKind::LoadWarning,
            text: warn.message.clone(),
        });
    }
    if let Some(out) = &app.ext_outcome {
        for msg in &out.design.status.messages {
            lines.push(StatusLine {
                kind: status_kind(msg.severity),
                text: msg.message.clone(),
            });
        }
    }
    lines
}

// ── Inputs panel ─────────────────────────────────────────────────────────────

/// The PowerUser input fields with unit-aware labels.
///
/// Hook fields are rendered separately in the view via the mode toggle.
pub fn ext_inputs_view(app: &App) -> Vec<FieldDescriptor<Field>> {
    let us = app.unit_system;
    let len = unit_length_label(us);
    let force = unit_force_label(us);
    vec![
        FieldDescriptor::new(format!("Wire diameter ({len})"), Field::WireDia),
        FieldDescriptor::new(format!("Mean diameter ({len})"), Field::MeanDia),
        FieldDescriptor::new("Active coils".to_string(), Field::Active),
        FieldDescriptor::new(format!("Free length ({len})"), Field::FreeLength),
        FieldDescriptor::new(format!("Initial tension ({force})"), Field::InitialTension),
        FieldDescriptor::new(format!("Loads ({force}), comma-separated"), Field::Loads),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::extension::form::{parse_and_solve, ExtFormState};
    use springcore::{CurvatureCorrection, MaterialSet, MaterialStore, UnitSystem};

    fn store() -> MaterialStore {
        MaterialStore::new(MaterialSet::load_default())
    }

    fn fresh_app() -> App {
        App::from_store(store(), Vec::new(), CurvatureCorrection::Bergstrasser)
    }

    fn metric_form() -> ExtFormState {
        ExtFormState {
            wire_dia: "2".to_string(),
            mean_dia: "20".to_string(),
            active: "10".to_string(),
            free_length: "100".to_string(),
            initial_tension: "5".to_string(),
            loads: "50".to_string(),
            ..ExtFormState::default()
        }
    }

    // ── inputs view ──

    #[test]
    fn inputs_view_has_six_fields_metric() {
        let app = fresh_app();
        let fields = ext_inputs_view(&app);
        assert_eq!(fields.len(), 6);
        assert_eq!(fields[0].field, Field::WireDia);
        assert_eq!(fields[5].field, Field::Loads);
    }

    #[test]
    fn inputs_view_labels_contain_unit_metric() {
        let app = fresh_app();
        let fields = ext_inputs_view(&app);
        assert!(
            fields[0].label.contains("mm"),
            "wire dia label should mention mm"
        );
        assert!(
            fields[4].label.contains("N"),
            "initial tension label should mention N"
        );
    }

    #[test]
    fn inputs_view_labels_contain_unit_us() {
        let mut app = fresh_app();
        app.unit_system = UnitSystem::Us;
        let fields = ext_inputs_view(&app);
        assert!(
            fields[0].label.contains("in"),
            "wire dia label should mention in"
        );
        assert!(
            fields[4].label.contains("lbf"),
            "initial tension label should mention lbf"
        );
    }

    // ── geometry rows ──

    #[test]
    fn geometry_rows_has_seven_entries() {
        let materials = store();
        let out = parse_and_solve(
            &metric_form(),
            "Music Wire",
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .unwrap();
        let rows = geometry_rows(&out.design, UnitSystem::Metric);
        assert_eq!(rows.len(), 7);
    }

    #[test]
    fn geometry_rows_labels() {
        let materials = store();
        let out = parse_and_solve(
            &metric_form(),
            "Music Wire",
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .unwrap();
        let rows = geometry_rows(&out.design, UnitSystem::Metric);
        let labels: Vec<&str> = rows.iter().map(|r| r.label.as_str()).collect();
        assert!(labels.contains(&"Spring index"));
        assert!(labels.contains(&"Active coils"));
        assert!(labels.contains(&"Rate"));
        assert!(labels.contains(&"Free length"));
        assert!(labels.contains(&"Outer diameter"));
        assert!(labels.contains(&"Inner diameter"));
        assert!(labels.contains(&"Initial tension"));
    }

    // ── status view ──

    #[test]
    fn status_empty_for_fresh_app() {
        let app = fresh_app();
        assert!(ext_status_view(&app).is_empty());
    }

    #[test]
    fn status_surfaces_action_error() {
        let mut app = fresh_app();
        app.action_error = Some("test error".to_string());
        let lines = ext_status_view(&app);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].kind, StatusKind::ActionError);
    }

    // ── results view ──

    #[test]
    fn results_view_empty_with_no_outcome() {
        let app = fresh_app();
        assert_eq!(ext_results_view(&app), ExtResultsView::Empty);
    }

    #[test]
    fn results_view_error_when_error_set() {
        let mut app = fresh_app();
        app.error = Some("bad input".to_string());
        assert!(matches!(ext_results_view(&app), ExtResultsView::Error(_)));
    }

    #[test]
    fn results_view_populated_after_solve() {
        let materials = store();
        let mut app = fresh_app();
        let out = parse_and_solve(
            &metric_form(),
            "Music Wire",
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .unwrap();
        app.ext_outcome = Some(out);
        assert!(matches!(
            ext_results_view(&app),
            ExtResultsView::Populated(_)
        ));
    }

    // ── Regression tests for review-panel findings ──

    /// R1 finding H2: selecting Extension on a fresh (all-blank) form used to
    /// immediately parse the blank state, produce a "wire diameter required"
    /// error, and land in `Error` rather than `Empty`.
    ///
    /// The blank-form guard in `recompute()` must intercept this path.
    #[test]
    fn select_extension_on_blank_form_shows_empty_not_error() {
        use crate::app::Message;
        use springcore::Family;

        // App starts as Compression; extension form is all-blank by default.
        let mut app = fresh_app();
        app.update(Message::SelectFamily(Family::Extension));

        assert!(
            app.error.is_none(),
            "blank extension form must not produce a parse error"
        );
        assert_eq!(
            ext_results_view(&app),
            ExtResultsView::Empty,
            "blank extension form must show Empty, not Error"
        );
    }

    /// R1 finding M1: after switching from Extension back to Compression,
    /// `ext_outcome` must be cleared so the stale solved design can't
    /// resurface if the user switches back without re-entering data.
    #[test]
    fn switch_to_compression_clears_ext_outcome() {
        use crate::app::Message;
        use springcore::Family;

        let materials = store();
        let mut app = fresh_app();

        // Manually inject a solved extension outcome while on Extension.
        let out = parse_and_solve(
            &metric_form(),
            "Music Wire",
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .unwrap();
        app.family = Family::Extension;
        app.ext_outcome = Some(out);

        // Switch to Compression — recompute() Extension arm must clear ext_outcome.
        app.update(Message::SelectFamily(Family::Compression));

        assert!(
            app.ext_outcome.is_none(),
            "ext_outcome must be None after switching to Compression"
        );
    }

    /// R1 finding M1 (other direction): after switching from Compression to
    /// Extension, `outcome` (the compression result) must be cleared.
    #[test]
    fn switch_to_extension_clears_compression_outcome() {
        use crate::app::Message;
        use springcore::Family;

        let mut app = fresh_app();

        // Produce a real compression outcome via the existing test-visible path.
        app.form.scenario = crate::compression::form::ScenarioKind::RateBased;
        app.form.wire_dia = "2.0".into();
        app.form.mean_dia = "20.0".into();
        app.form.rate = "2.0".into();
        app.form.free_length = "60".into();
        app.form.loads = "10, 30".into();
        app.recompute();
        assert!(app.outcome.is_some(), "pre-condition: compression must be solved");

        // Switch to Extension — recompute() Compression arm must clear outcome.
        app.update(Message::SelectFamily(Family::Extension));

        assert!(
            app.outcome.is_none(),
            "compression outcome must be None after switching to Extension"
        );
    }
}
