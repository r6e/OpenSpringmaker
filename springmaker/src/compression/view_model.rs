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
use crate::presenter::{FieldDescriptor, LoadRow, LoadTable, ResultRow, StatusKind, StatusLine};
use springcore::{BindingConstraint, Severity, SpringDesign, UnitSystem};

// ── Unit labels and conversions ─────────────────────────────────────────────

/// Length unit label for the active unit system.
fn unit_length_label(us: UnitSystem) -> &'static str {
    match us {
        UnitSystem::Metric => "mm",
        UnitSystem::Us => "in",
    }
}

/// Force unit label for the active unit system.
fn unit_force_label(us: UnitSystem) -> &'static str {
    match us {
        UnitSystem::Metric => "N",
        UnitSystem::Us => "lbf",
    }
}

/// Spring-rate unit label for the active unit system.
fn unit_rate_label(us: UnitSystem) -> &'static str {
    match us {
        UnitSystem::Metric => "N/mm",
        UnitSystem::Us => "lbf/in",
    }
}

/// Stress unit label for the active unit system.
fn unit_stress_label(us: UnitSystem) -> &'static str {
    match us {
        UnitSystem::Metric => "MPa",
        UnitSystem::Us => "ksi",
    }
}

/// Length in the active unit system: mm (metric) or inches (US).
fn display_len(l: springcore::Length, us: UnitSystem) -> f64 {
    match us {
        UnitSystem::Metric => l.millimeters(),
        UnitSystem::Us => l.inches(),
    }
}

/// Force in the active unit system: N (metric) or lbf (US).
fn display_force(f: springcore::Force, us: UnitSystem) -> f64 {
    match us {
        UnitSystem::Metric => f.newtons(),
        UnitSystem::Us => f.pounds_force(),
    }
}

/// Conversion factor: N/mm displayed ↔ N/m stored internally.
const MM_PER_M: f64 = 1000.0;

/// Spring rate in the active unit system: N/mm (metric) or lbf/in (US).
fn display_rate(r: springcore::SpringRate, us: UnitSystem) -> f64 {
    match us {
        // Display in N/mm (= N/m ÷ MM_PER_M) so rate is consistent with mm lengths and
        // the chart axes (deflection in mm, force in N → slope in N/mm).
        UnitSystem::Metric => r.newtons_per_meter() / MM_PER_M,
        UnitSystem::Us => r.pounds_per_inch(),
    }
}

/// Stress `(value, label)` in the active unit system: MPa (metric) or ksi (US).
fn display_stress(s: springcore::Stress, us: UnitSystem) -> (f64, &'static str) {
    let value = match us {
        UnitSystem::Metric => s.megapascals(),
        UnitSystem::Us => s.psi() / 1000.0,
    };
    (value, unit_stress_label(us))
}

// ── Results panel ───────────────────────────────────────────────────────────

/// The hero spring-rate readout (label is constant in the view).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoverningRate {
    pub value: String,
    pub unit: String,
}

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
        Some(out) => ResultsView::Populated(Box::new(populated_results(out, app.unit_system))),
        None => match &app.error {
            Some(err) => ResultsView::Error(err.clone()),
            None => ResultsView::Empty,
        },
    }
}

fn populated_results(out: &FormOutcome, us: UnitSystem) -> PopulatedResults {
    let d = &out.design;
    PopulatedResults {
        governing_rate: GoverningRate {
            value: format!("{:.4}", display_rate(d.rate, us)),
            unit: unit_rate_label(us).to_string(),
        },
        geometry: geometry_rows(d, us),
        load_table: load_table(d, us),
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
        ResultRow::new("Spring index", format!("{:.3}", d.index), ""),
        ResultRow::new("Active coils", format!("{:.3}", d.active_coils), ""),
        ResultRow::new("Total coils", format!("{:.3}", d.total_coils), ""),
        ResultRow::new(
            "Free length",
            format!("{:.4}", display_len(d.free_length, us)),
            len,
        ),
        ResultRow::new(
            "Solid length",
            format!("{:.4}", display_len(d.solid_length, us)),
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
            "Natural frequency",
            format!("{:.2}", d.natural_frequency.hertz()),
            "Hz",
        ),
        buckling,
    ]
}

fn load_table(d: &SpringDesign, us: UnitSystem) -> LoadTable {
    let rows = d
        .load_points
        .iter()
        .enumerate()
        .map(|(i, lp)| {
            let (stress_val, _) = display_stress(lp.shear_stress, us);
            LoadRow {
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
                stress: format!("{stress_val:.3}"),
                pct_mts: format!("{:.1}%", lp.pct_mts * 100.0),
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
                ResultRow::new("Alternating stress", format!("{alt_val:.2}"), alt_lbl),
                ResultRow::new("Mean stress", format!("{mean_val:.2}"), mean_lbl),
                ResultRow::new(
                    "Endurance (S\u{2032}\u{2032}se)",
                    format!("{endurance_val:.2}"),
                    endurance_lbl,
                ),
                ResultRow::new("Ultimate shear (Ssu)", format!("{ssu_val:.2}"), ssu_lbl),
                ResultRow::new(
                    "Goodman FOS",
                    format!("{:.3}", fat.goodman_factor_of_safety),
                    "",
                ),
            ])
        }
        FatigueStatus::NoData => FatigueView::Note(FATIGUE_NO_DATA),
        FatigueStatus::Skipped => FatigueView::Note(FATIGUE_SKIPPED),
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
                ResultRow::new("Wire mass", format!("{:.4}", mw.mass_kg), "kg"),
                ResultRow::new("Binding constraint", binding, ""),
            ])
        }
        None => MinWeightView::Hidden,
    }
}

// ── Status panel ────────────────────────────────────────────────────────────

/// Status class for a design message's severity.
fn status_kind(severity: Severity) -> StatusKind {
    match severity {
        Severity::Info => StatusKind::Info,
        Severity::Caution => StatusKind::Caution,
        Severity::Warning => StatusKind::DesignWarning,
    }
}

/// Status lines to show: a save/load action error first (most recent), then
/// load warnings, then design messages. An empty vector means the status panel
/// is suppressed entirely.
pub fn status_view(app: &App) -> Vec<StatusLine> {
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
    if let Some(out) = &app.outcome {
        for msg in &out.status.messages {
            lines.push(StatusLine {
                kind: status_kind(msg.severity),
                text: msg.message.clone(),
            });
        }
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
    use crate::presenter::Emphasis;
    use approx::assert_relative_eq;
    use springcore::{
        Force, Length, LoadWarning, MaterialSet, MaterialStore, SpringRate, Stress, UnitSystem,
    };

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

    // ── Unit conversions (the surface of the prior 1000× magnitude bug) ──

    #[test]
    fn length_conversion_matches_unit_system() {
        let one_mm = Length::from_millimeters(1.0);
        assert_relative_eq!(display_len(one_mm, UnitSystem::Metric), 1.0);
        assert_relative_eq!(
            display_len(one_mm, UnitSystem::Us),
            1.0 / 25.4,
            epsilon = 1e-9
        );
    }

    #[test]
    fn force_conversion_matches_unit_system() {
        // Each side built from its own native constructor, so a metric↔US
        // accessor swap is caught (not just a tautology against the impl).
        assert_relative_eq!(
            display_force(Force::from_newtons(10.0), UnitSystem::Metric),
            10.0
        );
        assert_relative_eq!(
            display_force(Force::from_pounds_force(7.0), UnitSystem::Us),
            7.0,
            epsilon = 1e-9
        );
    }

    #[test]
    fn rate_is_displayed_in_per_mm_not_per_meter() {
        // 2000 N/m stored must read as 2 N/mm — the magnitude that bit us before.
        assert_relative_eq!(
            display_rate(
                SpringRate::from_newtons_per_meter(2000.0),
                UnitSystem::Metric
            ),
            2.0
        );
        assert_relative_eq!(
            display_rate(SpringRate::from_pounds_per_inch(5.0), UnitSystem::Us),
            5.0,
            epsilon = 1e-9
        );
    }

    #[test]
    fn stress_conversion_carries_the_right_label() {
        let (v_metric, l_metric) =
            display_stress(Stress::from_megapascals(500.0), UnitSystem::Metric);
        assert_relative_eq!(v_metric, 500.0);
        assert_eq!(l_metric, "MPa");
        // 2000 psi = 2 ksi (independent magnitude, not a restatement of psi()/1000).
        let (v_us, l_us) = display_stress(Stress::from_psi(2000.0), UnitSystem::Us);
        assert_relative_eq!(v_us, 2.0, epsilon = 1e-9);
        assert_eq!(l_us, "ksi");
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
}
