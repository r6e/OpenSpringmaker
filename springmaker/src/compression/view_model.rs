//! Presenter (view-model) for the calculator screen.
//!
//! Pure functions that turn `App` state into plain data describing *what* the
//! results, status, and inputs areas should show. No iced dependency, so each
//! decision (which results mode, fatigue/min-weight gating, status suppression
//! and severity, the scenario-driven input field set) and every unit
//! conversion is unit-testable without a renderer; `view` renders this data
//! verbatim.

use crate::app::App;
use crate::compression::form::{FatigueStatus, Field, FormOutcome, ScenarioKind};
use crate::presenter::{
    append_status_messages, display_force, display_len, display_stress, fmt_row_value,
    overstress_emphasis, unit_force_label, unit_length_label, unit_rate_label, unit_stress_label,
    FieldDescriptor, GoverningRate, LoadRow, LoadTable, ResultRow, StatusLine,
};
use springcore::{BindingConstraint, Material, SpringDesign, UnitSystem};

// ── Results panel ───────────────────────────────────────────────────────────

/// Fatigue section state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FatigueView {
    /// Suppressed: a min-weight result occupies the panel instead.
    Hidden,
    /// Fatigue analysis succeeded; readout rows.
    Computed(Vec<ResultRow>),
    /// A muted note (`FATIGUE_NO_DATA` or `FATIGUE_SKIPPED`).
    Note(&'static str),
}

/// Shown when cycle forces were supplied but the material has no endurance data.
const FATIGUE_NO_DATA: &str = "No fatigue data for this material.";
/// Shown when the user left the cycle forces blank.
const FATIGUE_SKIPPED: &str = "Enter min and max cycle forces to compute fatigue.";

/// Min-weight optimisation section state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MinWeightView {
    /// Not a min-weight run; nothing shown.
    Hidden,
    /// Min-weight result readout rows.
    Shown(Vec<ResultRow>),
}

/// Everything the results panel shows when a design has been solved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopulatedResults {
    pub governing_rate: GoverningRate,
    pub geometry: Vec<ResultRow>,
    pub load_table: LoadTable,
    pub fatigue: FatigueView,
    pub min_weight: MinWeightView,
}

/// The three mutually-exclusive states of the results panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultsView {
    /// A solve was attempted and failed; the error message.
    Error(String),
    /// No solve yet and no error: the empty-state prompt.
    Empty,
    /// A solved design (boxed: the populated variant is far larger than the others).
    Populated(Box<PopulatedResults>),
}

/// Which results state to show. A present outcome wins over any stale error,
/// matching the prior inline order (outcome checked first).
pub fn results_view(app: &App) -> ResultsView {
    match &app.outcome {
        Some(out) => ResultsView::Populated(Box::new(populated_results(out, app))),
        None => match &app.error {
            Some(err) => ResultsView::Error(err.clone()),
            None => ResultsView::Empty,
        },
    }
}

fn populated_results(out: &FormOutcome, app: &App) -> PopulatedResults {
    let d = &out.design;
    let us = app.unit_system;
    // A present outcome means `app.material` already resolved during that
    // solve (the conical precedent); `.ok()` degrades gracefully rather than
    // panicking on the documented-unreachable race where it no longer does.
    let material = app.materials.get(&app.material).ok();
    PopulatedResults {
        governing_rate: GoverningRate::from_rate(d.rate, us),
        geometry: geometry_rows(d, us),
        load_table: load_table(d, us, material),
        fatigue: fatigue_view(out, us),
        min_weight: min_weight_view(out),
    }
}

fn geometry_rows(d: &SpringDesign, us: UnitSystem) -> Vec<ResultRow> {
    let len = unit_length_label(us);
    let buckling = if d.buckling_stable {
        ResultRow::new("Buckling", "Stable", "")
    } else {
        ResultRow::danger("Buckling", "UNSTABLE", "")
    };
    vec![
        ResultRow::new("Spring index", fmt_row_value(d.index, 3), ""),
        ResultRow::new("Active coils", fmt_row_value(d.active_coils, 3), ""),
        ResultRow::new("Total coils", fmt_row_value(d.total_coils, 3), ""),
        ResultRow::new(
            "Free length",
            fmt_row_value(display_len(d.free_length, us), 4),
            len,
        ),
        ResultRow::new(
            "Solid length",
            fmt_row_value(display_len(d.solid_length, us), 4),
            len,
        ),
        ResultRow::new(
            "Outer diameter",
            fmt_row_value(display_len(d.outer_dia, us), 4),
            len,
        ),
        ResultRow::new(
            "Inner diameter",
            fmt_row_value(display_len(d.inner_dia, us), 4),
            len,
        ),
        ResultRow::new(
            "Natural frequency",
            fmt_row_value(d.natural_frequency.hertz(), 2),
            "Hz",
        ),
        buckling,
    ]
}

fn load_table(d: &SpringDesign, us: UnitSystem, material: Option<&Material>) -> LoadTable {
    let rows = d
        .load_points
        .iter()
        .enumerate()
        .map(|(i, lp)| {
            let (stress_val, _) = display_stress(lp.shear_stress, us);
            LoadRow {
                point: format!("{}", i + 1),
                force: format!(
                    "{} {}",
                    fmt_row_value(display_force(lp.force, us), 3),
                    unit_force_label(us)
                ),
                deflection: format!(
                    "{} {}",
                    fmt_row_value(display_len(lp.deflection, us), 4),
                    unit_length_label(us)
                ),
                length: format!(
                    "{} {}",
                    fmt_row_value(display_len(lp.length, us), 4),
                    unit_length_label(us)
                ),
                stress: fmt_row_value(stress_val, 3),
                pct_mts: format!("{}%", fmt_row_value(lp.pct_mts * 100.0, 1)),
                stress_emphasis: overstress_emphasis(lp.pct_mts, material),
            }
        })
        .collect();
    LoadTable {
        stress_unit: unit_stress_label(us).to_string(),
        rows,
    }
}

fn fatigue_view(out: &FormOutcome, us: UnitSystem) -> FatigueView {
    if out.min_weight.is_some() {
        return FatigueView::Hidden;
    }
    match &out.fatigue {
        FatigueStatus::Computed(fat) => {
            let (alt_val, alt_lbl) = display_stress(fat.alternating_stress, us);
            let (mean_val, mean_lbl) = display_stress(fat.mean_stress, us);
            let (endurance_val, endurance_lbl) = display_stress(fat.fully_reversed_endurance, us);
            let (ssu_val, ssu_lbl) = display_stress(fat.ultimate_shear, us);
            FatigueView::Computed(vec![
                ResultRow::new("Alternating stress", fmt_row_value(alt_val, 2), alt_lbl),
                ResultRow::new("Mean stress", fmt_row_value(mean_val, 2), mean_lbl),
                ResultRow::new(
                    "Endurance (S\u{2032}\u{2032}se)",
                    fmt_row_value(endurance_val, 2),
                    endurance_lbl,
                ),
                ResultRow::new("Ultimate shear (Ssu)", fmt_row_value(ssu_val, 2), ssu_lbl),
                ResultRow::new(
                    "Goodman FOS",
                    fmt_row_value(fat.goodman_factor_of_safety, 3),
                    "",
                ),
            ])
        }
        FatigueStatus::NoData => FatigueView::Note(FATIGUE_NO_DATA),
        FatigueStatus::Skipped => FatigueView::Note(FATIGUE_SKIPPED),
    }
}

/// Goodman chart for the fatigue section — emitted exactly when the fatigue
/// rows are ([`FatigueView::Computed`]), so it inherits the min-weight Hidden
/// gate and the NoData/Skipped note gates from [`fatigue_view`]. A min-weight
/// run computes fatigue too; without this gate the chart would render orphaned
/// (no heading or rows) above the min-weight results.
pub fn fatigue_chart_data(out: &FormOutcome, us: UnitSystem) -> Option<crate::plot::ChartData> {
    match (fatigue_view(out, us), &out.fatigue) {
        (FatigueView::Computed(_), FatigueStatus::Computed(f)) => {
            Some(crate::compression::plot_model::goodman_chart(f, us))
        }
        _ => None,
    }
}

fn min_weight_view(out: &FormOutcome) -> MinWeightView {
    match &out.min_weight {
        Some(mw) => {
            let binding = match mw.binding {
                BindingConstraint::Stress => "stress",
                BindingConstraint::Index => "index",
                BindingConstraint::OuterDiameter => "outer diameter",
            };
            MinWeightView::Shown(vec![
                ResultRow::new("Wire mass", fmt_row_value(mw.mass_kg, 4), "kg"),
                ResultRow::new("Binding constraint", binding, ""),
            ])
        }
        None => MinWeightView::Hidden,
    }
}

// ── Status panel ────────────────────────────────────────────────────────────

/// Status lines to show: a save/load action error first (most recent), then
/// load warnings, then design messages. An empty vector means the status panel
/// is suppressed entirely.
pub fn status_view(app: &App) -> Vec<StatusLine> {
    let mut lines = crate::presenter::common_status_lines(app);
    if let Some(out) = &app.outcome {
        append_status_messages(&mut lines, &out.status.messages);
    }
    lines
}

// ── Inputs panel ────────────────────────────────────────────────────────────

/// The scenario-driven input fields: the primary set plus an optional fatigue
/// cycle set (empty for the min-weight scenario, which has no fatigue section).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputsView {
    pub primary: Vec<FieldDescriptor<Field>>,
    pub fatigue: Vec<FieldDescriptor<Field>>,
}

/// Which input fields to show for the current scenario, with unit-aware labels.
pub fn inputs_view(app: &App) -> InputsView {
    let f = &app.form;
    let us = app.unit_system;
    let len = unit_length_label(us);
    let force = unit_force_label(us);
    let rate = unit_rate_label(us);

    if f.scenario == ScenarioKind::MinWeight {
        return InputsView {
            primary: vec![
                FieldDescriptor::new(format!("Required rate ({rate})"), Field::Rate),
                FieldDescriptor::new(format!("Max force ({force})"), Field::MaxForce),
                FieldDescriptor::new("Index min", Field::IndexMin),
                FieldDescriptor::new("Index max", Field::IndexMax),
                FieldDescriptor::new(
                    format!("Max outer diameter ({len}, optional)"),
                    Field::MaxOuterDia,
                ),
                FieldDescriptor::new(
                    format!("Candidate wire diameters ({len}), comma-separated"),
                    Field::CandidateDiameters,
                ),
                FieldDescriptor::new("Clash allowance (fraction)", Field::ClashAllowance),
            ],
            fatigue: Vec::new(),
        };
    }

    let mut primary = vec![FieldDescriptor::new(
        format!("Wire diameter ({len})"),
        Field::WireDia,
    )];
    let mean = FieldDescriptor::new(format!("Mean diameter ({len})"), Field::MeanDia);
    let free_length = FieldDescriptor::new(format!("Free length ({len})"), Field::FreeLength);
    let loads = FieldDescriptor::new(format!("Loads ({force}), comma-separated"), Field::Loads);

    match f.scenario {
        ScenarioKind::PowerUser => {
            primary.push(mean);
            primary.push(FieldDescriptor::new("Active coils", Field::Active));
            primary.push(free_length);
            primary.push(loads);
        }
        ScenarioKind::TwoLoad => {
            primary.push(mean);
            primary.push(FieldDescriptor::new(
                format!("Force 1 ({force})"),
                Field::Force1,
            ));
            primary.push(FieldDescriptor::new(
                format!("Length 1 ({len})"),
                Field::Length1,
            ));
            primary.push(FieldDescriptor::new(
                format!("Force 2 ({force})"),
                Field::Force2,
            ));
            primary.push(FieldDescriptor::new(
                format!("Length 2 ({len})"),
                Field::Length2,
            ));
        }
        ScenarioKind::RateBased => {
            primary.push(mean);
            primary.push(FieldDescriptor::new(
                format!("Spring rate ({rate})"),
                Field::Rate,
            ));
            primary.push(free_length);
            primary.push(loads);
        }
        ScenarioKind::Dimensional => {
            primary.push(FieldDescriptor::new(
                format!("Outer diameter ({len})"),
                Field::OuterDia,
            ));
            primary.push(FieldDescriptor::new("Active coils", Field::Active));
            primary.push(free_length);
            primary.push(loads);
        }
        ScenarioKind::MinWeight => unreachable!("MinWeight handled by the early return above"),
    }

    let fatigue = vec![
        FieldDescriptor::new(format!("Min cycle force ({force})"), Field::FatigueMin),
        FieldDescriptor::new(format!("Max cycle force ({force})"), Field::FatigueMax),
    ];

    InputsView { primary, fatigue }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compression::form::FormState;
    use crate::presenter::{status_kind, Emphasis, StatusKind};
    use springcore::{LoadWarning, MaterialSet, MaterialStore, Severity, UnitSystem};

    fn store() -> MaterialStore {
        MaterialStore::new(MaterialSet::load_default())
    }

    /// Hermetic App: curated-only store, no on-disk overlay, no IO.
    fn app_with(form: FormState) -> App {
        let mut app = App::from_store(
            store(),
            Vec::new(),
            springcore::CurvatureCorrection::Bergstrasser,
        );
        app.form = form;
        app.recompute();
        app
    }

    fn rate_based_metric() -> FormState {
        FormState {
            scenario: ScenarioKind::RateBased,
            end_type: "squared_ground".into(),
            fixity: "fixed_fixed".into(),
            wire_dia: "2.0".into(),
            mean_dia: "20.0".into(),
            rate: "2.0".into(),
            free_length: "60.0".into(),
            loads: "10, 30".into(),
            fatigue_min: "10".into(),
            fatigue_max: "30".into(),
            ..Default::default()
        }
    }

    fn min_weight_metric() -> FormState {
        FormState {
            scenario: ScenarioKind::MinWeight,
            end_type: "squared_ground".into(),
            fixity: "fixed_fixed".into(),
            rate: "2".into(),
            max_force: "50".into(),
            index_min: "4".into(),
            index_max: "12".into(),
            max_outer_dia: "25".into(),
            candidate_diameters: "1.5, 2.5, 3".into(),
            clash_allowance: "0.15".into(),
            ..Default::default()
        }
    }

    fn fields(d: &[FieldDescriptor<Field>]) -> Vec<Field> {
        d.iter().map(|fd| fd.field).collect()
    }

    fn labels(d: &[FieldDescriptor<Field>]) -> Vec<&str> {
        d.iter().map(|fd| fd.label.as_str()).collect()
    }

    // ── results_view: the three mutually-exclusive modes ──

    #[test]
    fn results_view_is_empty_with_no_outcome_and_no_error() {
        let app = App::from_store(
            store(),
            Vec::new(),
            springcore::CurvatureCorrection::Bergstrasser,
        );
        assert_eq!(results_view(&app), ResultsView::Empty);
    }

    #[test]
    fn results_view_is_error_when_solve_fails() {
        let mut form = rate_based_metric();
        form.wire_dia = String::new(); // blank → parse failure
        let app = app_with(form);
        assert!(matches!(results_view(&app), ResultsView::Error(_)));
    }

    #[test]
    fn results_view_is_populated_after_a_successful_solve() {
        let app = app_with(rate_based_metric());
        assert!(matches!(results_view(&app), ResultsView::Populated(_)));
    }

    fn populated(app: &App) -> PopulatedResults {
        match results_view(app) {
            ResultsView::Populated(p) => *p,
            other => panic!("expected Populated, got {other:?}"),
        }
    }

    #[test]
    fn governing_rate_reads_in_n_per_mm() {
        let p = populated(&app_with(rate_based_metric()));
        assert_eq!(p.governing_rate.value, "2.0000");
        assert_eq!(p.governing_rate.unit, "N/mm");
    }

    #[test]
    fn geometry_buckling_emphasis_tracks_its_text() {
        let p = populated(&app_with(rate_based_metric()));
        let buckling = p
            .geometry
            .iter()
            .find(|r| r.label == "Buckling")
            .expect("a buckling row");
        // The danger color is shown exactly when the spring is unstable.
        assert_eq!(
            buckling.emphasis == Emphasis::Danger,
            buckling.value == "UNSTABLE"
        );
        assert_eq!(
            buckling.emphasis == Emphasis::Normal,
            buckling.value == "Stable"
        );
    }

    #[test]
    fn load_table_header_unit_and_row_count() {
        let app = app_with(rate_based_metric());
        let p = populated(&app);
        assert_eq!(p.load_table.stress_unit, "MPa");
        // One row per solved load point.
        let points = match &app.outcome {
            Some(o) => o.design.load_points.len(),
            None => unreachable!(),
        };
        assert_eq!(p.load_table.rows.len(), points);
        assert_eq!(p.load_table.rows[0].point, "1");
        assert!(p.load_table.rows[0].pct_mts.ends_with('%'));
        // The per-row stress value is the MPa magnitude at 3 decimals — pins the
        // conversion + precision, not just the header unit.
        let first_stress = match &app.outcome {
            Some(o) => display_stress(o.design.load_points[0].shear_stress, UnitSystem::Metric).0,
            None => unreachable!(),
        };
        assert_eq!(p.load_table.rows[0].stress, format!("{first_stress:.3}"));
    }

    #[test]
    fn overstressed_load_point_carries_danger_emphasis() {
        // Reuses the huge_finite_stress fixture: loads = "1e9" N drives pct_mts
        // far past 1.0.
        let form = FormState {
            loads: "1e9".into(),
            ..rate_based_metric()
        };
        let p = populated(&app_with(form));
        assert_eq!(p.load_table.rows[0].stress_emphasis, Emphasis::Danger);
    }

    #[test]
    fn normal_load_point_carries_normal_emphasis() {
        let p = populated(&app_with(rate_based_metric()));
        assert_eq!(p.load_table.rows[0].stress_emphasis, Emphasis::Normal);
    }

    /// Gap case: pct_mts (72.3%) sits between Music Wire's 45% allowable and
    /// 100% MTS. The engine's own status warning already calls this
    /// overstressed (`evaluate_status` fires at `pct_mts > allowable_pct_torsion`),
    /// but the old `pct_mts > 1.0` rule rendered it Normal — a load point the
    /// status panel flags as overstressed with no color in the load table.
    #[test]
    fn gap_case_overstressed_by_engine_carries_danger_emphasis() {
        let form = FormState {
            loads: "200".into(),
            ..rate_based_metric()
        };
        let p = populated(&app_with(form));
        assert_eq!(p.load_table.rows[0].stress_emphasis, Emphasis::Danger);
    }

    // ── fatigue gating ──

    #[test]
    fn fatigue_computed_for_material_with_endurance() {
        let p = populated(&app_with(rate_based_metric()));
        assert!(matches!(p.fatigue, FatigueView::Computed(_)));
    }

    #[test]
    fn fatigue_no_data_for_material_without_endurance() {
        let mut app = app_with(rate_based_metric());
        app.material = "Stainless 302".into(); // no cited endurance data
        app.recompute();
        let p = populated(&app);
        assert_eq!(p.fatigue, FatigueView::Note(FATIGUE_NO_DATA));
    }

    #[test]
    fn fatigue_skipped_when_cycle_forces_blank() {
        let mut form = rate_based_metric();
        form.fatigue_min = String::new();
        form.fatigue_max = String::new();
        let p = populated(&app_with(form));
        assert_eq!(p.fatigue, FatigueView::Note(FATIGUE_SKIPPED));
    }

    #[test]
    fn fatigue_hidden_for_min_weight_run() {
        let p = populated(&app_with(min_weight_metric()));
        assert_eq!(p.fatigue, FatigueView::Hidden);
    }

    #[test]
    fn fatigue_chart_suppressed_for_min_weight_even_when_computed() {
        // Regression guard: a min-weight solve with cycle forces
        // filled yields BOTH `min_weight: Some` and `fatigue: Computed`; the
        // chart must follow the rows' Hidden gate, not the raw fatigue status,
        // or it renders orphaned above the min-weight results.
        let mut form = min_weight_metric();
        form.fatigue_min = "10".into();
        form.fatigue_max = "30".into();
        let app = app_with(form);
        let out = app.outcome.as_ref().expect("min-weight solve succeeds");
        assert!(out.min_weight.is_some(), "fixture must be a min-weight run");
        assert!(
            matches!(out.fatigue, FatigueStatus::Computed(_)),
            "fixture must have computed fatigue underneath"
        );
        assert!(fatigue_chart_data(out, UnitSystem::Metric).is_none());
        // The rows agree: presenter hides the whole fatigue section.
        assert_eq!(populated(&app).fatigue, FatigueView::Hidden);
    }

    #[test]
    fn fatigue_chart_present_for_computed_non_min_weight() {
        let app = app_with(rate_based_metric());
        let out = app.outcome.as_ref().expect("solve succeeds");
        let data = fatigue_chart_data(out, UnitSystem::Metric)
            .expect("computed fatigue on a normal run yields a chart");
        assert_eq!(data.lines.len(), 2); // envelope + load line
    }

    // ── min-weight section ──

    #[test]
    fn min_weight_hidden_for_non_min_weight_scenario() {
        let p = populated(&app_with(rate_based_metric()));
        assert_eq!(p.min_weight, MinWeightView::Hidden);
    }

    #[test]
    fn min_weight_shown_with_mass_and_binding_rows() {
        let p = populated(&app_with(min_weight_metric()));
        let MinWeightView::Shown(rows) = &p.min_weight else {
            panic!("expected Shown");
        };
        assert_eq!(rows[0].label, "Wire mass");
        assert_eq!(rows[1].label, "Binding constraint");
        // The binding label is one of the three known constraints.
        assert!(["stress", "index", "outer diameter"].contains(&rows[1].value.as_str()));
    }

    // ── status panel ──

    #[test]
    fn status_kind_maps_each_severity() {
        assert_eq!(status_kind(Severity::Info), StatusKind::Info);
        assert_eq!(status_kind(Severity::Caution), StatusKind::Caution);
        assert_eq!(status_kind(Severity::Warning), StatusKind::DesignWarning);
    }

    #[test]
    fn status_suppressed_when_clean() {
        let app = App::from_store(
            store(),
            Vec::new(),
            springcore::CurvatureCorrection::Bergstrasser,
        );
        assert!(status_view(&app).is_empty());
    }

    #[test]
    fn status_surfaces_action_error() {
        let mut app = App::from_store(
            store(),
            Vec::new(),
            springcore::CurvatureCorrection::Bergstrasser,
        );
        app.action_error = Some("could not save design".into());
        let lines = status_view(&app);
        assert_eq!(lines[0].kind, StatusKind::ActionError);
        assert_eq!(lines[0].text, "could not save design");
    }

    #[test]
    fn action_error_precedes_load_warnings() {
        let mut app = App::from_store(
            store(),
            vec![LoadWarning {
                message: "overlay warning".into(),
            }],
            springcore::CurvatureCorrection::Bergstrasser,
        );
        app.action_error = Some("save failed".into());
        let lines = status_view(&app);
        assert_eq!(lines[0].kind, StatusKind::ActionError);
        assert!(lines.iter().any(|l| l.kind == StatusKind::LoadWarning));
    }

    #[test]
    fn status_surfaces_load_warnings() {
        let app = App::from_store(
            store(),
            vec![LoadWarning {
                message: "ignored a malformed overlay entry".into(),
            }],
            springcore::CurvatureCorrection::Bergstrasser,
        );
        let lines = status_view(&app);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].kind, StatusKind::LoadWarning);
        assert_eq!(lines[0].text, "ignored a malformed overlay entry");
    }

    #[test]
    fn status_shows_load_warnings_before_design_messages() {
        let mut app = app_with(rate_based_metric());
        app.load_warnings = vec![LoadWarning {
            message: "overlay warning".into(),
        }];
        let lines = status_view(&app);
        assert_eq!(lines[0].kind, StatusKind::LoadWarning);
        // Every load warning precedes every non-load-warning line.
        let first_non_warning = lines
            .iter()
            .position(|l| l.kind != StatusKind::LoadWarning)
            .unwrap_or(lines.len());
        assert!(lines[..first_non_warning]
            .iter()
            .all(|l| l.kind == StatusKind::LoadWarning));
    }

    // ── inputs: scenario-driven field set + unit-aware labels ──

    #[test]
    fn min_weight_inputs_have_no_fatigue_section() {
        let v = inputs_view(&app_with(min_weight_metric()));
        assert!(v.fatigue.is_empty());
        assert_eq!(
            fields(&v.primary),
            vec![
                Field::Rate,
                Field::MaxForce,
                Field::IndexMin,
                Field::IndexMax,
                Field::MaxOuterDia,
                Field::CandidateDiameters,
                Field::ClashAllowance,
            ]
        );
    }

    #[test]
    fn rate_based_inputs_have_fatigue_section() {
        let v = inputs_view(&app_with(rate_based_metric()));
        assert_eq!(
            fields(&v.primary),
            vec![
                Field::WireDia,
                Field::MeanDia,
                Field::Rate,
                Field::FreeLength,
                Field::Loads,
            ]
        );
        assert_eq!(
            fields(&v.fatigue),
            vec![Field::FatigueMin, Field::FatigueMax]
        );
    }

    #[test]
    fn dimensional_inputs_use_outer_diameter_not_mean() {
        let mut form = rate_based_metric();
        form.scenario = ScenarioKind::Dimensional;
        let v = inputs_view(&app_with(form));
        let fs = fields(&v.primary);
        assert!(fs.contains(&Field::OuterDia));
        assert!(!fs.contains(&Field::MeanDia));
    }

    #[test]
    fn input_labels_track_the_unit_system() {
        // App::from_store defaults to UnitSystem::Metric, so no override needed.
        let metric = app_with(min_weight_metric());
        assert!(labels(&inputs_view(&metric).primary).contains(&"Required rate (N/mm)"));

        let mut us = app_with(min_weight_metric());
        us.unit_system = UnitSystem::Us;
        assert!(labels(&inputs_view(&us).primary).contains(&"Required rate (lbf/in)"));
    }

    #[test]
    fn huge_finite_stress_renders_scientific_not_digit_wall() {
        // Mirror the existing load-table test fixture (rate_based_metric) exactly,
        // changing only the loads field to "1e9" N. Wahl: d=2mm, C=10, F=1e9 N
        // → τ ≈ 6e9 MPa — far above SCI_THRESHOLD (1e6), so fmt_row_value must
        // switch to scientific notation.
        let form = FormState {
            loads: "1e9".into(),
            ..rate_based_metric()
        };
        let app = app_with(form);
        let p = populated(&app);
        let row = &p.load_table.rows[0];
        let cell = &row.stress;
        assert!(
            cell.contains('e') && cell.len() < 12,
            "expected scientific notation, got '{cell}'"
        );
        // Sweep coverage: deflection cell must also render scientific for huge loads.
        let deflection = &row.deflection;
        assert!(
            deflection.split(' ').next().unwrap().contains('e'),
            "deflection cell must render scientific mantissa for huge load, got '{deflection}'"
        );
        // Sweep coverage: force cell must also render scientific for huge loads.
        let force = &row.force;
        assert!(
            force.split(' ').next().unwrap().contains('e'),
            "force cell must render scientific mantissa for huge load, got '{force}'"
        );
        // Sweep coverage: length cell must also render scientific for huge loads.
        let length = &row.length;
        assert!(
            length.split(' ').next().unwrap().contains('e'),
            "length cell must render scientific mantissa for huge load, got '{length}'"
        );
        // Sweep coverage: pct_mts is formatted as "{fmt_row_value(…)}%"; at the 1e9 N
        // fixture pct_mts ≈ 3.614e8% — strip the trailing '%' and assert scientific.
        // Probe (empirical): "3.614e8%"
        let pct_mts = &row.pct_mts;
        assert!(
            pct_mts.trim_end_matches('%').contains('e'),
            "pct_mts must render scientific for huge load, got '{pct_mts}'"
        );
    }
}
