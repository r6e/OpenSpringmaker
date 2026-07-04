//! Pure presenter (view-model) for the torsion spring calculator screen.
//!
//! No iced dependency — every decision (which results mode, unit conversions,
//! status severity mapping) is unit-testable without a renderer. Mirrors the
//! structure of `extension::view_model`.

use crate::app::App;
use crate::presenter::{
    append_status_messages, display_ang_rate_per_deg, display_ang_rate_per_turn,
    display_angle_degrees, display_angle_turns, display_len, display_moment, display_stress,
    unit_length_label, unit_moment_label, unit_stress_label, Emphasis, FieldDescriptor, ResultRow,
    StatusLine,
};
use crate::torsion::form::{Field, TorScenarioKind};
use springcore::torsion::TorsionDesign;

// ── Torsion load-point table ─────────────────────────────────────────────────

/// One row of the torsion load-points table, all fields pre-formatted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TorLoadRow {
    /// Point index label ("1", "2", …).
    pub point: String,
    /// Applied moment with unit, e.g. "1000.000 N·mm".
    pub moment: String,
    /// Angular deflection in degrees and revolutions, e.g. "112.87° (0.3135 rev)".
    pub deflection: String,
    /// Inner-fiber bending stress magnitude (e.g. "1375.806").
    pub stress: String,
    /// Inner-fiber stress as % of allowable (e.g. "91.7%").
    pub pct_allow: String,
    /// Wound-up inner diameter with unit (e.g. "16.8218 mm").
    pub wound_inner: String,
    /// Danger when `pct_bending_allow > 1.0`; the view maps this to a danger color.
    pub stress_emphasis: Emphasis,
}

/// Stress-unit header label plus per-point rows for the torsion load-points table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TorLoadTable {
    pub stress_unit: String,
    pub rows: Vec<TorLoadRow>,
}

/// Build a [`TorLoadTable`] from a solved torsion design.
fn tor_load_table(d: &TorsionDesign, us: springcore::UnitSystem) -> TorLoadTable {
    let rows = d
        .load_points
        .iter()
        .enumerate()
        .map(|(i, lp)| {
            let (stress_val, _) = display_stress(lp.stress_inner, us);
            let stress_emphasis = if lp.pct_bending_allow > 1.0 {
                Emphasis::Danger
            } else {
                Emphasis::Normal
            };
            TorLoadRow {
                point: format!("{}", i + 1),
                moment: format!(
                    "{:.3} {}",
                    display_moment(lp.moment, us),
                    unit_moment_label(us)
                ),
                deflection: format!(
                    "{:.2}° ({:.4} rev)",
                    display_angle_degrees(lp.deflection),
                    display_angle_turns(lp.deflection)
                ),
                stress: format!("{stress_val:.3}"),
                pct_allow: format!("{:.1}%", lp.pct_bending_allow * 100.0),
                wound_inner: format!(
                    "{:.4} {}",
                    display_len(lp.wound_inner_dia, us),
                    unit_length_label(us)
                ),
                stress_emphasis,
            }
        })
        .collect();
    TorLoadTable {
        stress_unit: unit_stress_label(us).to_string(),
        rows,
    }
}

// ── Results panel ─────────────────────────────────────────────────────────────

/// The three mutually-exclusive states of the torsion results panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TorResultsView {
    /// A parse/solve error.
    Error(String),
    /// Inputs are empty or invalid; nothing to show.
    Empty,
    /// A solved design with geometry ready to render.
    Populated(Box<TorPopulatedResults>),
}

/// Everything the torsion results panel shows when a design is solved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TorPopulatedResults {
    /// Angular rate expressed as moment per degree (one ResultRow).
    pub rate_per_deg: ResultRow,
    /// Angular rate expressed as moment per revolution (one ResultRow).
    pub rate_per_turn: ResultRow,
    /// Geometry summary rows: spring index and effective active coils.
    pub geometry: Vec<ResultRow>,
    /// Per-moment load-point table.
    pub load_table: TorLoadTable,
}

/// Geometry summary rows: spring index and effective active coils.
fn geometry_rows(d: &TorsionDesign) -> Vec<ResultRow> {
    vec![
        ResultRow::new("Spring index", format!("{:.3}", d.index), ""),
        ResultRow::new("Active coils", format!("{:.3}", d.active_coils), ""),
    ]
}

/// Build the angular-rate-per-degree result row.
fn rate_per_deg_row(d: &TorsionDesign, us: springcore::UnitSystem) -> ResultRow {
    ResultRow::new(
        "Angular rate",
        format!("{:.4}", display_ang_rate_per_deg(d.rate, us)),
        format!("{}/°", unit_moment_label(us)),
    )
}

/// Build the angular-rate-per-revolution result row.
fn rate_per_turn_row(d: &TorsionDesign, us: springcore::UnitSystem) -> ResultRow {
    ResultRow::new(
        "Angular rate",
        format!("{:.4}", display_ang_rate_per_turn(d.rate, us)),
        format!("{}/rev", unit_moment_label(us)),
    )
}

/// Build the torsion results panel view model from app state.
///
/// A solved outcome takes priority over an error string (the two are mutually
/// exclusive after any recompute); blank state with neither is Empty.
pub fn tor_results_view(app: &App) -> TorResultsView {
    match &app.tor_outcome {
        Some(out) => {
            let us = app.unit_system;
            TorResultsView::Populated(Box::new(TorPopulatedResults {
                rate_per_deg: rate_per_deg_row(&out.design, us),
                rate_per_turn: rate_per_turn_row(&out.design, us),
                geometry: geometry_rows(&out.design),
                load_table: tor_load_table(&out.design, us),
            }))
        }
        None => match &app.error {
            Some(err) => TorResultsView::Error(err.clone()),
            None => TorResultsView::Empty,
        },
    }
}

// ── Status panel ──────────────────────────────────────────────────────────────

/// Status lines for the torsion family: mirrors `ext_status_view`.
pub fn tor_status_view(app: &App) -> Vec<StatusLine> {
    let mut lines = crate::presenter::common_status_lines(app);
    if let Some(out) = &app.tor_outcome {
        append_status_messages(&mut lines, &out.design.status.messages);
    }
    lines
}

// ── Inputs panel ──────────────────────────────────────────────────────────────

/// The torsion input fields with unit-aware labels, varying by active scenario.
///
/// The friction-model and scenario pick-lists are rendered separately in the view.
pub fn tor_inputs_view(app: &App) -> Vec<FieldDescriptor<Field>> {
    let us = app.unit_system;
    let len = unit_length_label(us);
    let moment = unit_moment_label(us);
    match app.torsion.scenario {
        TorScenarioKind::RateBased => vec![
            FieldDescriptor::new(format!("Wire diameter ({len})"), Field::WireDia),
            FieldDescriptor::new(format!("Mean diameter ({len})"), Field::MeanDia),
            FieldDescriptor::new(format!("Rate ({moment}/°)"), Field::Rate),
            FieldDescriptor::new(format!("Leg 1 ({len})"), Field::Leg1),
            FieldDescriptor::new(format!("Leg 2 ({len})"), Field::Leg2),
            FieldDescriptor::new(format!("Arbor diameter ({len}, optional)"), Field::ArborDia),
            FieldDescriptor::new(
                format!("Moments ({moment}), comma-separated"),
                Field::Moments,
            ),
        ],
        TorScenarioKind::PowerUser => vec![
            FieldDescriptor::new(format!("Wire diameter ({len})"), Field::WireDia),
            FieldDescriptor::new(format!("Mean diameter ({len})"), Field::MeanDia),
            FieldDescriptor::new("Body coils".to_string(), Field::BodyCoils),
            FieldDescriptor::new(format!("Leg 1 ({len})"), Field::Leg1),
            FieldDescriptor::new(format!("Leg 2 ({len})"), Field::Leg2),
            FieldDescriptor::new(format!("Arbor diameter ({len}, optional)"), Field::ArborDia),
            FieldDescriptor::new(
                format!("Moments ({moment}), comma-separated"),
                Field::Moments,
            ),
        ],
        TorScenarioKind::Dimensional => vec![
            FieldDescriptor::new(format!("Wire diameter ({len})"), Field::WireDia),
            FieldDescriptor::new(format!("Outer diameter ({len})"), Field::OuterDia),
            FieldDescriptor::new("Body coils".to_string(), Field::BodyCoils),
            FieldDescriptor::new(format!("Leg 1 ({len})"), Field::Leg1),
            FieldDescriptor::new(format!("Leg 2 ({len})"), Field::Leg2),
            FieldDescriptor::new(format!("Arbor diameter ({len}, optional)"), Field::ArborDia),
            FieldDescriptor::new(
                format!("Moments ({moment}), comma-separated"),
                Field::Moments,
            ),
        ],
        TorScenarioKind::TwoLoad => vec![
            FieldDescriptor::new(format!("Wire diameter ({len})"), Field::WireDia),
            FieldDescriptor::new(format!("Mean diameter ({len})"), Field::MeanDia),
            FieldDescriptor::new(format!("Leg 1 ({len})"), Field::Leg1),
            FieldDescriptor::new(format!("Leg 2 ({len})"), Field::Leg2),
            FieldDescriptor::new(format!("Arbor diameter ({len}, optional)"), Field::ArborDia),
            FieldDescriptor::new(format!("Moment 1 ({moment})"), Field::Moment1),
            FieldDescriptor::new("Angle 1 (°)".to_string(), Field::Angle1),
            FieldDescriptor::new(format!("Moment 2 ({moment})"), Field::Moment2),
            FieldDescriptor::new("Angle 2 (°)".to_string(), Field::Angle2),
        ],
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::presenter::StatusKind;
    use crate::torsion::form::TorFormState;
    use springcore::{CurvatureCorrection, Family, MaterialSet, MaterialStore, UnitSystem};

    fn store() -> MaterialStore {
        MaterialStore::new(MaterialSet::load_default())
    }

    fn fresh_app() -> App {
        App::from_store(store(), Vec::new(), CurvatureCorrection::Bergstrasser)
    }

    fn fresh_app_torsion() -> App {
        let mut app = fresh_app();
        app.family = Family::Torsion;
        app
    }

    fn metric_form() -> TorFormState {
        TorFormState {
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            body_coils: "5".into(),
            leg1: "0".into(),
            leg2: "0".into(),
            moments: "1000".into(),
            ..TorFormState::default()
        }
    }

    /// Two-moment metric form to exercise the multi-row load table.
    fn two_moment_metric_form() -> TorFormState {
        TorFormState {
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            body_coils: "5".into(),
            leg1: "0".into(),
            leg2: "0".into(),
            moments: "500, 1000".into(),
            ..TorFormState::default()
        }
    }

    /// Form with a very large moment to drive σᵢ past the allowable.
    fn overstressed_form() -> TorFormState {
        TorFormState {
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            body_coils: "5".into(),
            leg1: "0".into(),
            leg2: "0".into(),
            moments: "50000".into(), // 50 N·m — well past bending allowable
            ..TorFormState::default()
        }
    }

    fn app_with_tor(form: TorFormState) -> App {
        let mut app = App::from_store(store(), Vec::new(), CurvatureCorrection::Bergstrasser);
        app.family = Family::Torsion;
        app.torsion = form;
        app.recompute();
        app
    }

    fn tor_populated(app: &App) -> TorPopulatedResults {
        match tor_results_view(app) {
            TorResultsView::Populated(p) => *p,
            other => panic!("expected Populated, got {other:?}"),
        }
    }

    // ── results panel ────────────────────────────────────────────────────────

    #[test]
    fn results_empty_then_populated() {
        let app = fresh_app();
        assert_eq!(tor_results_view(&app), TorResultsView::Empty);
        let solved = app_with_tor(metric_form());
        assert!(matches!(
            tor_results_view(&solved),
            TorResultsView::Populated(_)
        ));
    }

    #[test]
    fn results_view_error_when_error_set() {
        let mut app = fresh_app();
        app.error = Some("bad input".to_string());
        assert!(matches!(tor_results_view(&app), TorResultsView::Error(_)));
    }

    #[test]
    fn results_view_empty_on_fresh_torsion_family() {
        let mut app = fresh_app_torsion();
        app.recompute();
        assert_eq!(tor_results_view(&app), TorResultsView::Empty);
    }

    // ── load-point table ─────────────────────────────────────────────────────

    #[test]
    fn deflection_row_shows_degrees_and_revolutions() {
        let p = tor_populated(&app_with_tor(metric_form()));
        let row0 = &p.load_table.rows[0];
        assert!(
            row0.deflection.contains('°') && row0.deflection.contains("rev"),
            "deflection must show both ° and rev: got '{}'",
            row0.deflection
        );
    }

    #[test]
    fn load_table_has_correct_point_count() {
        let p = tor_populated(&app_with_tor(two_moment_metric_form()));
        assert_eq!(p.load_table.rows.len(), 2);
        assert_eq!(p.load_table.rows[0].point, "1");
        assert_eq!(p.load_table.rows[1].point, "2");
    }

    #[test]
    fn load_table_stress_unit_is_mpa_for_metric() {
        let p = tor_populated(&app_with_tor(metric_form()));
        assert_eq!(p.load_table.stress_unit, "MPa");
    }

    #[test]
    fn load_table_moment_contains_unit_label_metric() {
        let p = tor_populated(&app_with_tor(metric_form()));
        assert!(
            p.load_table.rows[0].moment.contains("N·mm"),
            "moment cell must contain 'N·mm'; got '{}'",
            p.load_table.rows[0].moment
        );
    }

    #[test]
    fn stress_emphasis_is_danger_when_overstressed() {
        let p = tor_populated(&app_with_tor(overstressed_form()));
        assert_eq!(
            p.load_table.rows[0].stress_emphasis,
            Emphasis::Danger,
            "overstressed row must be Danger"
        );
    }

    #[test]
    fn stress_emphasis_is_normal_when_safe() {
        let p = tor_populated(&app_with_tor(metric_form()));
        assert_eq!(
            p.load_table.rows[0].stress_emphasis,
            Emphasis::Normal,
            "safe row must be Normal"
        );
    }

    #[test]
    fn pct_allow_ends_with_percent() {
        let p = tor_populated(&app_with_tor(metric_form()));
        assert!(p.load_table.rows[0].pct_allow.ends_with('%'));
    }

    #[test]
    fn wound_inner_contains_length_unit_metric() {
        let p = tor_populated(&app_with_tor(metric_form()));
        assert!(
            p.load_table.rows[0].wound_inner.contains("mm"),
            "wound_inner must contain 'mm'; got '{}'",
            p.load_table.rows[0].wound_inner
        );
    }

    // ── summary / geometry rows ───────────────────────────────────────────────

    #[test]
    fn geometry_rows_has_spring_index_and_active_coils() {
        let p = tor_populated(&app_with_tor(metric_form()));
        let labels: Vec<&str> = p.geometry.iter().map(|r| r.label.as_str()).collect();
        assert!(
            labels.contains(&"Spring index"),
            "must include 'Spring index'"
        );
        assert!(
            labels.contains(&"Active coils"),
            "must include 'Active coils'"
        );
    }

    #[test]
    fn rate_rows_have_angular_rate_label_and_correct_units() {
        let p = tor_populated(&app_with_tor(metric_form()));
        assert_eq!(p.rate_per_deg.label, "Angular rate");
        assert_eq!(p.rate_per_turn.label, "Angular rate");
        assert!(
            p.rate_per_deg.unit.contains("N·mm") && p.rate_per_deg.unit.contains('°'),
            "per-deg unit should contain 'N·mm' and '°'; got '{}'",
            p.rate_per_deg.unit
        );
        assert!(
            p.rate_per_turn.unit.contains("N·mm") && p.rate_per_turn.unit.contains("rev"),
            "per-rev unit should contain 'N·mm' and 'rev'; got '{}'",
            p.rate_per_turn.unit
        );
    }

    #[test]
    fn rate_per_turn_is_360x_rate_per_deg() {
        let p = tor_populated(&app_with_tor(metric_form()));
        let per_deg: f64 = p.rate_per_deg.value.parse().expect("parseable f64");
        let per_turn: f64 = p.rate_per_turn.value.parse().expect("parseable f64");
        assert!(per_deg > 0.0 && per_turn > 0.0, "rates must be positive");
        // Both values are formatted to 4 decimal places independently; the ratio
        // per_turn/per_deg must be 360 to within the combined rounding error
        // (≤ 0.5×10⁻⁴×360 + 0.5×10⁻⁴ ≈ 0.018 in the worst case).
        let ratio = per_turn / per_deg;
        assert!(
            (ratio - 360.0).abs() < 0.05,
            "per_turn / per_deg must be ≈ 360; got ratio = {ratio}"
        );
    }

    #[test]
    fn geometry_index_value_is_ten_for_c10_spring() {
        // d=2 mm, D=20 mm → C=10.
        let p = tor_populated(&app_with_tor(metric_form()));
        let idx_row = p
            .geometry
            .iter()
            .find(|r| r.label == "Spring index")
            .expect("Spring index row must exist");
        let idx: f64 = idx_row.value.parse().expect("parseable f64");
        use approx::assert_relative_eq;
        assert_relative_eq!(idx, 10.0, max_relative = 1e-3);
    }

    // ── US-unit load table ────────────────────────────────────────────────────

    #[test]
    fn load_table_stress_unit_is_ksi_for_us() {
        let mut app = app_with_tor(TorFormState {
            wire_dia: "0.0787".into(), // ≈ 2 mm in inches
            mean_dia: "0.787".into(),  // ≈ 20 mm in inches
            body_coils: "5".into(),
            leg1: "0".into(),
            leg2: "0".into(),
            moments: "8.851".into(), // ≈ 1000 N·mm in lbf·in
            ..TorFormState::default()
        });
        app.unit_system = UnitSystem::Us;
        app.recompute();
        let p = tor_populated(&app);
        assert_eq!(p.load_table.stress_unit, "ksi");
    }

    // ── status panel ─────────────────────────────────────────────────────────

    #[test]
    fn status_empty_for_fresh_app() {
        let app = fresh_app();
        assert!(tor_status_view(&app).is_empty());
    }

    #[test]
    fn status_surfaces_action_error() {
        let mut app = fresh_app();
        app.action_error = Some("test error".to_string());
        let lines = tor_status_view(&app);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].kind, StatusKind::ActionError);
    }

    #[test]
    fn status_surfaces_design_warnings() {
        // overstressed_form triggers an overstress Warning from the engine.
        let app = app_with_tor(overstressed_form());
        let lines = tor_status_view(&app);
        assert!(
            lines.iter().any(|l| l.kind == StatusKind::DesignWarning),
            "overstressed design must produce a DesignWarning status line"
        );
    }

    // ── inputs panel ─────────────────────────────────────────────────────────

    #[test]
    fn inputs_view_has_seven_unit_aware_fields() {
        let app = fresh_app_torsion();
        let fields = tor_inputs_view(&app);
        assert_eq!(fields.len(), 7);
        assert_eq!(fields[0].field, Field::WireDia);
        assert!(
            fields[0].label.contains("mm"),
            "wire dia label must contain 'mm'; got '{}'",
            fields[0].label
        );
        assert!(
            fields.iter().any(|f| f.label.contains("N·mm")),
            "moment field label must contain 'N·mm'"
        );
    }

    #[test]
    fn inputs_view_field_order_is_canonical() {
        let app = fresh_app_torsion();
        let fields = tor_inputs_view(&app);
        let kinds: Vec<Field> = fields.iter().map(|fd| fd.field).collect();
        assert_eq!(
            kinds,
            vec![
                Field::WireDia,
                Field::MeanDia,
                Field::BodyCoils,
                Field::Leg1,
                Field::Leg2,
                Field::ArborDia,
                Field::Moments,
            ]
        );
    }

    #[test]
    fn inputs_view_labels_contain_unit_us() {
        let mut app = fresh_app_torsion();
        app.unit_system = UnitSystem::Us;
        let fields = tor_inputs_view(&app);
        assert!(
            fields[0].label.contains("in"),
            "wire dia label must mention 'in' for US; got '{}'",
            fields[0].label
        );
        assert!(
            fields.iter().any(|f| f.label.contains("lbf·in")),
            "moment field label must mention 'lbf·in' for US"
        );
    }

    #[test]
    fn inputs_view_last_field_is_moments() {
        let app = fresh_app_torsion();
        let fields = tor_inputs_view(&app);
        assert_eq!(fields.last().expect("non-empty").field, Field::Moments);
    }

    #[test]
    fn ratebased_inputs_view_contains_rate_not_body_coils() {
        use crate::torsion::form::TorScenarioKind;
        let mut app = fresh_app_torsion();
        app.torsion.scenario = TorScenarioKind::RateBased;
        let fields = tor_inputs_view(&app);
        let kinds: Vec<Field> = fields.iter().map(|fd| fd.field).collect();
        assert!(
            kinds.contains(&Field::Rate),
            "RateBased inputs must contain Field::Rate; got {kinds:?}"
        );
        assert!(
            !kinds.contains(&Field::BodyCoils),
            "RateBased inputs must NOT contain Field::BodyCoils; got {kinds:?}"
        );
    }

    #[test]
    fn ratebased_rate_field_label_contains_per_degree() {
        use crate::torsion::form::TorScenarioKind;
        let mut app = fresh_app_torsion();
        app.torsion.scenario = TorScenarioKind::RateBased;
        let fields = tor_inputs_view(&app);
        let rate_fd = fields
            .iter()
            .find(|fd| fd.field == Field::Rate)
            .expect("Field::Rate must be present in RateBased inputs");
        assert!(
            rate_fd.label.contains("/°"),
            "rate field label must contain '/°'; got '{}'",
            rate_fd.label
        );
    }

    #[test]
    fn dimensional_inputs_view_has_outer_dia_not_mean_dia() {
        use crate::torsion::form::TorScenarioKind;
        let mut app = fresh_app_torsion();
        app.torsion.scenario = TorScenarioKind::Dimensional;
        let fields = tor_inputs_view(&app);
        let kinds: Vec<Field> = fields.iter().map(|fd| fd.field).collect();
        assert_eq!(
            fields.len(),
            7,
            "Dimensional must have 7 fields; got {kinds:?}"
        );
        assert!(
            kinds.contains(&Field::OuterDia),
            "Dimensional inputs must contain Field::OuterDia; got {kinds:?}"
        );
        assert!(
            !kinds.contains(&Field::MeanDia),
            "Dimensional inputs must NOT contain Field::MeanDia; got {kinds:?}"
        );
    }

    #[test]
    fn twoload_inputs_view_has_point_fields_not_moments() {
        use crate::torsion::form::TorScenarioKind;
        let mut app = fresh_app_torsion();
        app.torsion.scenario = TorScenarioKind::TwoLoad;
        let fields = tor_inputs_view(&app);
        let kinds: Vec<Field> = fields.iter().map(|fd| fd.field).collect();
        assert_eq!(fields.len(), 9, "TwoLoad must have 9 fields; got {kinds:?}");
        for required in [Field::Moment1, Field::Angle1, Field::Moment2, Field::Angle2] {
            assert!(
                kinds.contains(&required),
                "TwoLoad inputs must contain {required:?}; got {kinds:?}"
            );
        }
        assert!(
            !kinds.contains(&Field::Moments),
            "TwoLoad inputs must NOT contain Field::Moments; got {kinds:?}"
        );
        let angle_fd = fields
            .iter()
            .find(|fd| fd.field == Field::Angle1)
            .expect("Field::Angle1 must be present");
        assert!(
            angle_fd.label.contains('°'),
            "angle field label must contain '°'; got '{}'",
            angle_fd.label
        );
    }

    // ── cross-family outcome clearing ────────────────────────────────────────

    /// Switching to Torsion clears any stale Compression and Extension outcomes.
    /// Pins the `self.outcome = None` and `self.ext_outcome = None` lines in
    /// the Torsion arm of `recompute()` so deleting either makes this test fail.
    #[test]
    fn switch_to_torsion_clears_other_family_outcomes() {
        use crate::app::Message;
        use crate::extension::form::{parse_and_solve as ext_parse_and_solve, ExtFormState};
        use springcore::Family;

        let mut app = fresh_app();

        // Produce a real compression outcome via form fields + recompute.
        app.form.scenario = crate::compression::form::ScenarioKind::RateBased;
        app.form.wire_dia = "2.0".into();
        app.form.mean_dia = "20.0".into();
        app.form.rate = "2.0".into();
        app.form.free_length = "60".into();
        app.form.loads = "10, 30".into();
        app.recompute();
        assert!(
            app.outcome.is_some(),
            "pre-condition: compression outcome must be Some before switching"
        );

        // Inject a real extension outcome directly (recompute would clobber outcome).
        let ext_form = ExtFormState {
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            active: "10".into(),
            free_length: "100".into(),
            initial_tension: "5".into(),
            loads: "50".into(),
            ..ExtFormState::default()
        };
        let ext_out = ext_parse_and_solve(
            &ext_form,
            "Music Wire",
            UnitSystem::Metric,
            &store(),
            CurvatureCorrection::default(),
        )
        .unwrap();
        app.ext_outcome = Some(ext_out);
        assert!(
            app.ext_outcome.is_some(),
            "pre-condition: ext_outcome must be Some before switching"
        );

        // Switch to Torsion — the Torsion arm of recompute() clears both.
        app.update(Message::SelectFamily(Family::Torsion));

        assert!(
            app.outcome.is_none(),
            "compression outcome must be None after switching to Torsion"
        );
        assert!(
            app.ext_outcome.is_none(),
            "ext_outcome must be None after switching to Torsion"
        );
    }

    /// Switching away from Torsion (to Compression or Extension) clears
    /// `tor_outcome`.  Pins every `self.tor_outcome = None` line in the
    /// Compression and Extension arms of `recompute()`.
    #[test]
    fn switch_away_from_torsion_clears_tor_outcome() {
        use crate::app::Message;
        use springcore::Family;

        // Part 1: switching to Compression clears tor_outcome.
        let mut app = app_with_tor(metric_form());
        assert!(
            app.tor_outcome.is_some(),
            "pre-condition: tor_outcome must be Some before switching to Compression"
        );
        app.update(Message::SelectFamily(Family::Compression));
        assert!(
            app.tor_outcome.is_none(),
            "tor_outcome must be None after switching to Compression"
        );

        // Part 2: switching to Extension clears tor_outcome (fresh solve).
        let mut app2 = app_with_tor(metric_form());
        assert!(
            app2.tor_outcome.is_some(),
            "pre-condition: tor_outcome must be Some before switching to Extension"
        );
        app2.update(Message::SelectFamily(Family::Extension));
        assert!(
            app2.tor_outcome.is_none(),
            "tor_outcome must be None after switching to Extension"
        );
    }
}
