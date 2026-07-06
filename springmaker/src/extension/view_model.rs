//! Pure presenter (view-model) for the extension spring calculator screen.
//!
//! No iced dependency — every decision (which results mode, unit conversions,
//! status severity mapping) is unit-testable without a renderer.
use crate::app::App;
use crate::extension::form::{ExtScenarioKind, Field};
use crate::presenter::{
    append_status_messages, display_force, display_len, display_rate, display_stress,
    unit_force_label, unit_length_label, unit_rate_label, unit_stress_label, FieldDescriptor,
    GoverningRate, ResultRow, StatusLine,
};
use springcore::extension::{ExtBindingConstraint, ExtensionDesign};

// ── Extension load-point table ───────────────────────────────────────────────

/// One row of the extension load-points table, all fields pre-formatted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExtLoadRow {
    pub point: String,
    pub force: String,
    pub deflection: String,
    pub length: String,
    /// Body shear stress magnitude (e.g. "412.345").
    pub body_shear: String,
    /// Hook bending stress magnitude (e.g. "567.890").
    pub hook_bending: String,
    /// Hook torsion stress magnitude (e.g. "234.567").
    pub hook_torsion: String,
    /// body_shear as % of allowable (e.g. "72.3%").
    pub pct_body: String,
    /// hook_bending as % of allowable (e.g. "85.1%").
    pub pct_bending: String,
    /// hook_torsion as % of allowable (e.g. "61.4%").
    pub pct_torsion: String,
}

/// Stress-unit header label plus per-point rows for the extension load-points table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExtLoadTable {
    pub stress_unit: String,
    pub rows: Vec<ExtLoadRow>,
}

/// Build an [`ExtLoadTable`] from a solved extension design.
fn ext_load_table(d: &ExtensionDesign, us: springcore::UnitSystem) -> ExtLoadTable {
    let rows = d
        .load_points
        .iter()
        .enumerate()
        .map(|(i, lp)| {
            let (body_val, _) = display_stress(lp.body_shear, us);
            let (bending_val, _) = display_stress(lp.hook_bending, us);
            let (torsion_val, _) = display_stress(lp.hook_torsion, us);
            ExtLoadRow {
                point: format!("{}", i + 1),
                force: format!(
                    "{:.3} {}",
                    display_force(lp.force, us),
                    unit_force_label(us)
                ),
                deflection: format!(
                    "{:.4} {}",
                    display_len(lp.deflection, us),
                    unit_length_label(us)
                ),
                length: format!(
                    "{:.4} {}",
                    display_len(lp.length, us),
                    unit_length_label(us)
                ),
                body_shear: format!("{body_val:.3}"),
                hook_bending: format!("{bending_val:.3}"),
                hook_torsion: format!("{torsion_val:.3}"),
                pct_body: format!("{:.1}%", lp.pct_body_allow * 100.0),
                pct_bending: format!("{:.1}%", lp.pct_hook_bending_allow * 100.0),
                pct_torsion: format!("{:.1}%", lp.pct_hook_torsion_allow * 100.0),
            }
        })
        .collect();
    ExtLoadTable {
        stress_unit: unit_stress_label(us).to_string(),
        rows,
    }
}

// ── Results panel ────────────────────────────────────────────────────────────

/// The three mutually-exclusive states of the extension results panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtResultsView {
    /// A parse/solve error.
    Error(String),
    /// Inputs are empty or invalid; nothing to show.
    Empty,
    /// A solved design with geometry ready to render.
    Populated(Box<ExtPopulatedResults>),
}

/// Everything the extension results panel shows when a design is solved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtPopulatedResults {
    pub governing_rate: GoverningRate,
    pub geometry: Vec<ResultRow>,
    pub load_table: ExtLoadTable,
    /// Min-weight optimisation rows when the active outcome is a MinWeight solve. A plain Option
    /// (vs compression's MinWeightView enum) suffices because extension has no fatigue section
    /// for the enum to also gate.
    pub min_weight: Option<Vec<ResultRow>>,
}

/// Min-weight optimisation result rows, or `None` when the active outcome is not
/// a Min-Weight solve.
fn ext_min_weight_rows(out: &crate::extension::form::ExtFormOutcome) -> Option<Vec<ResultRow>> {
    let mw = out.min_weight.as_ref()?;
    let binding = match mw.binding {
        ExtBindingConstraint::BodyShear => "body shear",
        ExtBindingConstraint::HookBending => "hook bending",
        ExtBindingConstraint::HookTorsion => "hook torsion",
        ExtBindingConstraint::Index => "index",
        ExtBindingConstraint::OuterDiameter => "outer diameter",
    };
    Some(vec![
        ResultRow::new("Wire mass", format!("{:.4}", mw.mass_kg), "kg"),
        ResultRow::new("Binding constraint", binding, ""),
    ])
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
            ExtResultsView::Populated(Box::new(ExtPopulatedResults {
                governing_rate: GoverningRate::from_rate(out.design.rate, us),
                geometry: geometry_rows(&out.design, us),
                load_table: ext_load_table(&out.design, us),
                min_weight: ext_min_weight_rows(out),
            }))
        }
        None => match &app.error {
            Some(err) => ExtResultsView::Error(err.clone()),
            None => ExtResultsView::Empty,
        },
    }
}

/// Geometry result rows for a solved extension design.
pub(crate) fn geometry_rows(d: &ExtensionDesign, us: springcore::UnitSystem) -> Vec<ResultRow> {
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
    let mut lines = crate::presenter::common_status_lines(app);
    if let Some(out) = &app.ext_outcome {
        append_status_messages(&mut lines, &out.design.status.messages);
    }
    lines
}

// ── Inputs panel ─────────────────────────────────────────────────────────────

/// The scenario-aware input fields with unit-aware labels.
///
/// Hook fields are rendered separately in the view via the mode toggle.
pub fn ext_inputs_view(app: &App) -> Vec<FieldDescriptor<Field>> {
    let us = app.unit_system;
    let len = unit_length_label(us);
    let force = unit_force_label(us);
    let rate = unit_rate_label(us);
    if app.extension.scenario == ExtScenarioKind::MinWeight {
        return vec![
            FieldDescriptor::new(format!("Required rate ({rate})"), Field::Rate),
            FieldDescriptor::new(format!("Max force ({force})"), Field::MaxForce),
            FieldDescriptor::new(format!("Initial tension ({force})"), Field::InitialTension),
            FieldDescriptor::new("Index min".to_string(), Field::IndexMin),
            FieldDescriptor::new("Index max".to_string(), Field::IndexMax),
            FieldDescriptor::new(
                format!("Max outer diameter ({len}, optional)"),
                Field::MaxOuterDia,
            ),
            FieldDescriptor::new(
                format!("Candidate wire diameters ({len}), comma-separated"),
                Field::CandidateDiameters,
            ),
        ];
    }
    let wire = FieldDescriptor::new(format!("Wire diameter ({len})"), Field::WireDia);
    let mean = FieldDescriptor::new(format!("Mean diameter ({len})"), Field::MeanDia);
    let free_length = FieldDescriptor::new(format!("Free length ({len})"), Field::FreeLength);
    let initial_tension =
        FieldDescriptor::new(format!("Initial tension ({force})"), Field::InitialTension);
    let loads = FieldDescriptor::new(format!("Loads ({force}), comma-separated"), Field::Loads);
    match app.extension.scenario {
        ExtScenarioKind::PowerUser => vec![
            wire,
            mean,
            FieldDescriptor::new("Active coils".to_string(), Field::Active),
            free_length,
            initial_tension,
            loads,
        ],
        ExtScenarioKind::RateBased => vec![
            wire,
            mean,
            FieldDescriptor::new(format!("Spring rate ({rate})"), Field::Rate),
            free_length,
            initial_tension,
            loads,
        ],
        ExtScenarioKind::Dimensional => vec![
            wire,
            FieldDescriptor::new(format!("Outer diameter ({len})"), Field::OuterDia),
            FieldDescriptor::new("Active coils".to_string(), Field::Active),
            free_length,
            initial_tension,
            loads,
        ],
        ExtScenarioKind::TwoLoad => vec![
            wire,
            mean,
            free_length,
            FieldDescriptor::new(format!("Force 1 ({force})"), Field::Force1),
            FieldDescriptor::new(format!("Length 1 ({len})"), Field::Length1),
            FieldDescriptor::new(format!("Force 2 ({force})"), Field::Force2),
            FieldDescriptor::new(format!("Length 2 ({len})"), Field::Length2),
        ],
        ExtScenarioKind::MinWeight => unreachable!("MinWeight handled by the early return above"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::extension::form::{parse_and_solve, ExtFormState};
    use crate::presenter::StatusKind;
    use springcore::{CurvatureCorrection, Family, MaterialSet, MaterialStore, UnitSystem};

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

    /// Two-load metric form for load-table tests.
    fn power_user_metric() -> ExtFormState {
        ExtFormState {
            wire_dia: "2".to_string(),
            mean_dia: "20".to_string(),
            active: "10".to_string(),
            free_length: "100".to_string(),
            initial_tension: "5".to_string(),
            loads: "10, 30".to_string(),
            ..ExtFormState::default()
        }
    }

    /// Build a hermetic App already switched to Extension and solved.
    fn app_with_ext(form: ExtFormState) -> App {
        let mut app = App::from_store(store(), Vec::new(), CurvatureCorrection::Bergstrasser);
        app.family = Family::Extension;
        app.extension = form;
        app.recompute();
        app
    }

    /// Unwrap an [`ExtResultsView::Populated`], panicking on anything else.
    fn ext_populated(app: &App) -> ExtPopulatedResults {
        match ext_results_view(app) {
            ExtResultsView::Populated(p) => *p,
            other => panic!("expected Populated, got {other:?}"),
        }
    }

    // ── load-point table ──

    #[test]
    fn ext_load_table_has_three_stress_columns_per_point() {
        let p = ext_populated(&app_with_ext(power_user_metric()));
        assert_eq!(p.load_table.stress_unit, "MPa");
        assert_eq!(p.load_table.rows.len(), 2);
        let r0 = &p.load_table.rows[0];
        assert_eq!(r0.point, "1");
        assert!(r0.pct_body.ends_with('%'));
        assert!(r0.pct_bending.ends_with('%'));
        assert!(r0.pct_torsion.ends_with('%'));
        // distinct stress columns (hook bending typically ≠ body shear).
        assert_ne!(r0.body_shear, r0.hook_bending);
    }

    /// Pin each hook-stress cell against its source field to catch column-swap mutants.
    ///
    /// `assert_ne!` alone is swap-blind (both `(A,B)` and `(B,A)` pass `A≠B`).
    /// Comparing each cell to the formatter output of the matching engine field
    /// catches a mutant that swaps `hook_bending` and `hook_torsion` columns.
    #[test]
    fn ext_load_table_pins_hook_bending_and_torsion_cells() {
        let app = app_with_ext(power_user_metric());
        let p = ext_populated(&app);
        let r0 = &p.load_table.rows[0];

        let out = app.ext_outcome.as_ref().expect("must be solved");
        let lp0 = &out.design.load_points[0];

        let (expected_bending, _) = display_stress(lp0.hook_bending, UnitSystem::Metric);
        let (expected_torsion, _) = display_stress(lp0.hook_torsion, UnitSystem::Metric);

        assert_eq!(
            r0.hook_bending,
            format!("{expected_bending:.3}"),
            "hook_bending column must map to hook_bending stress"
        );
        assert_eq!(
            r0.hook_torsion,
            format!("{expected_torsion:.3}"),
            "hook_torsion column must map to hook_torsion stress"
        );
        // Distinctness check: the two stresses differ for this geometry.
        assert_ne!(
            r0.hook_bending, r0.hook_torsion,
            "hook bending and torsion stresses are distinct for this design point"
        );
    }

    // ── inputs view ──

    #[test]
    fn inputs_view_twoload_has_no_initial_tension() {
        let mut app = fresh_app();
        app.extension.scenario = crate::extension::form::ExtScenarioKind::TwoLoad;
        let kinds: Vec<Field> = ext_inputs_view(&app).iter().map(|fd| fd.field).collect();
        assert!(
            kinds.contains(&Field::Force1)
                && kinds.contains(&Field::Length1)
                && kinds.contains(&Field::Force2)
                && kinds.contains(&Field::Length2),
            "TwoLoad inputs view must contain all four load-point fields"
        );
        assert!(
            !kinds.contains(&Field::InitialTension),
            "TwoLoad derives initial tension; it is not an input"
        );
    }

    #[test]
    fn inputs_view_ratebased_shows_rate_not_active() {
        let mut app = fresh_app();
        app.extension.scenario = crate::extension::form::ExtScenarioKind::RateBased;
        let fields = ext_inputs_view(&app);
        let kinds: Vec<Field> = fields.iter().map(|fd| fd.field).collect();
        assert!(
            kinds.contains(&Field::Rate),
            "RateBased shows the rate field"
        );
        assert!(
            !kinds.contains(&Field::Active),
            "RateBased has no active-coils field"
        );
    }

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

    // ── MinWeight results view ──

    #[test]
    fn minweight_results_include_optimisation_section() {
        let materials = store();
        let mut app = fresh_app();
        let out = parse_and_solve(
            &ExtFormState {
                scenario: crate::extension::form::ExtScenarioKind::MinWeight,
                rate: "2".into(),
                max_force: "50".into(),
                initial_tension: "5".into(),
                candidate_diameters: "1.5, 2.0, 2.5".into(),
                ..ExtFormState::default()
            },
            "Music Wire",
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .unwrap();
        app.family = Family::Extension;
        app.ext_outcome = Some(out);
        let p = match ext_results_view(&app) {
            ExtResultsView::Populated(p) => *p,
            other => panic!("expected Populated, got {other:?}"),
        };
        let rows = p
            .min_weight
            .expect("MinWeight outcome shows the optimisation section");
        assert!(rows.iter().any(|r| r.label == "Wire mass"));
        assert!(rows.iter().any(|r| r.label == "Binding constraint"));
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

        // Switch to Compression — recompute() Compression arm clears ext_outcome.
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
        assert!(
            app.outcome.is_some(),
            "pre-condition: compression must be solved"
        );

        // Switch to Extension — recompute() Compression arm must clear outcome.
        app.update(Message::SelectFamily(Family::Extension));

        assert!(
            app.outcome.is_none(),
            "compression outcome must be None after switching to Extension"
        );
    }
}
