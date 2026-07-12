//! Conical presenters (ADR 0008).
//!
//! Pure functions mapping `App` state to plain data (iced-free). Mirrors the
//! structure of `compression::view_model` and `torsion::view_model`.

use crate::app::App;
use crate::presenter::{
    append_status_messages, display_force, display_len, display_stress, fmt_row_value,
    overstress_emphasis, resolved_material, unit_force_label, unit_length_label, unit_stress_label,
    FieldDescriptor, GoverningRate, LoadRow, LoadTable, ResultRow, StatusLine,
};
use springcore::Material;

use super::form::Field;

// ── Footer constant ────────────────────────────────────────────────────────────

/// The always-present linear-model disclosure (spec Decision 2 of the engine
/// increment, placed here per the GUI spec).
pub const CON_LINEAR_MODEL_NOTE: &str =
    "Linear-range model: progressive stiffening as coils bottom out is not modeled.";

// ── Populated results ──────────────────────────────────────────────────────────

/// Everything the conical results panel shows when a design is solved.
#[derive(Debug, Clone, PartialEq)]
pub struct ConPopulatedResults {
    pub governing_rate: GoverningRate,
    pub geometry: Vec<ResultRow>,
    pub load_table: LoadTable,
}

/// The three mutually-exclusive states of the conical results panel.
#[derive(Debug, Clone, PartialEq)]
pub enum ConResultsView {
    /// A parse/solve error.
    Error(String),
    /// Inputs are empty or invalid; nothing to show.
    Empty,
    /// A solved design with results ready to render.
    Populated(Box<ConPopulatedResults>),
}

/// Build the conical results panel view model from app state.
///
/// A solved outcome takes priority over an error string (mutually exclusive
/// after any recompute); blank state with neither is Empty. Mirrors
/// `tor_results_view`'s outcome-first ordering.
pub fn con_results_view(app: &App) -> ConResultsView {
    match &app.con_outcome {
        Some(out) => ConResultsView::Populated(Box::new(con_populated_results(out, app))),
        None => match &app.error {
            Some(err) => ConResultsView::Error(err.clone()),
            None => ConResultsView::Empty,
        },
    }
}

/// Build [`ConPopulatedResults`] from a solved outcome.
fn con_populated_results(out: &super::form::ConFormOutcome, app: &App) -> ConPopulatedResults {
    let d = &out.design;
    let us = app.unit_system;
    let material = resolved_material(app);
    ConPopulatedResults {
        governing_rate: GoverningRate::from_rate(d.rate, us),
        geometry: con_geometry_rows(d, us),
        load_table: con_load_table(d, us, material),
    }
}

/// The 10-row geometry table (labels/decimals exact per spec §C).
fn con_geometry_rows(
    d: &springcore::conical::ConicalDesign,
    us: springcore::UnitSystem,
) -> Vec<ResultRow> {
    let len = unit_length_label(us);
    vec![
        ResultRow::new(
            "Large end OD",
            fmt_row_value(display_len(d.large_outer_dia, us), 4),
            len,
        ),
        ResultRow::new(
            "Large end ID",
            fmt_row_value(display_len(d.large_inner_dia, us), 4),
            len,
        ),
        ResultRow::new(
            "Small end OD",
            fmt_row_value(display_len(d.small_outer_dia, us), 4),
            len,
        ),
        ResultRow::new(
            "Small end ID",
            fmt_row_value(display_len(d.small_inner_dia, us), 4),
            len,
        ),
        ResultRow::new("Index (large end)", fmt_row_value(d.index_large, 3), ""),
        ResultRow::new("Index (small end)", fmt_row_value(d.index_small, 3), ""),
        ResultRow::new(
            "Taper per coil",
            fmt_row_value(display_len(d.taper_per_coil, us), 4),
            len,
        ),
        ResultRow::new("Total coils", fmt_row_value(d.total_coils, 3), ""),
        ResultRow::new("Pitch", fmt_row_value(display_len(d.pitch, us), 4), len),
        ResultRow::new(
            "Solid length (conservative)",
            fmt_row_value(display_len(d.solid_length, us), 4),
            len,
        ),
    ]
}

/// Build the load-point table. Mirrors compression's `load_table` construction.
/// No at-solid row: at-solid surfaces via the status warnings and the
/// conservative solid-length geometry row.
fn con_load_table(
    d: &springcore::conical::ConicalDesign,
    us: springcore::UnitSystem,
    material: Option<&Material>,
) -> LoadTable {
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

// ── Inputs panel ──────────────────────────────────────────────────────────────

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

// ── Status panel ──────────────────────────────────────────────────────────────

/// Status lines: shared prefix + design messages from `evaluate_status`.
///
/// `evaluate_status` is a free function in springcore (conical is unique; the
/// compression and torsion families bake status into the design struct). A
/// present `con_outcome` means the material already resolved during solve, so
/// an `Err` from the store here means a race that cannot occur in practice —
/// design messages are simply skipped on Err rather than panicking.
pub fn con_status_view(app: &App) -> Vec<StatusLine> {
    let mut lines = crate::presenter::common_status_lines(app);
    if let Some(out) = &app.con_outcome {
        if let Ok(material) = app.materials.get(&app.material) {
            let status = springcore::conical::evaluate_status(&out.design, material);
            append_status_messages(&mut lines, &status.messages);
        }
    }
    lines
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conical::form::ConFormState;
    use crate::presenter::Emphasis;
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

    /// The golden metric fixture: wire=2mm, large_mean=20mm, small_mean=12mm,
    /// active=10, free_length=60mm, loads=[10N, 25N].
    fn metric_form() -> ConFormState {
        ConFormState {
            end_type: "squared_ground".into(),
            wire_dia: "2".into(),
            large_mean_dia: "20".into(),
            small_mean_dia: "12".into(),
            active: "10".into(),
            free_length: "60".into(),
            loads: "10, 25".into(),
        }
    }

    fn solved_metric_app() -> App {
        let mut app = fresh_app_conical();
        app.conical = metric_form();
        app.recompute();
        app
    }

    fn con_populated(app: &App) -> ConPopulatedResults {
        match con_results_view(app) {
            ConResultsView::Populated(p) => *p,
            other => panic!("expected Populated, got {other:?}"),
        }
    }

    // ── geometry_rows_exact ─────────────────────────────────────────────────

    #[test]
    fn geometry_rows_exact() {
        // Golden fixture: wire=2, large_mean=20, small_mean=12, Na=10 →
        //   large_outer_dia = 22 mm, index_large = 10, taper_per_coil = 0.8 mm,
        //   solid_length = 24 mm (conservative, 2-wire-dia/coil at smallest end).
        let app = solved_metric_app();
        let p = con_populated(&app);

        // All 10 labels in order.
        let labels: Vec<&str> = p.geometry.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "Large end OD",
                "Large end ID",
                "Small end OD",
                "Small end ID",
                "Index (large end)",
                "Index (small end)",
                "Taper per coil",
                "Total coils",
                "Pitch",
                "Solid length (conservative)",
            ]
        );

        // Spot-check key values.
        let large_od = p
            .geometry
            .iter()
            .find(|r| r.label == "Large end OD")
            .unwrap();
        assert_eq!(large_od.value, "22.0000", "Large end OD must be 22.0000 mm");

        let index_large = p
            .geometry
            .iter()
            .find(|r| r.label == "Index (large end)")
            .unwrap();
        assert_eq!(
            index_large.value, "10.000",
            "Index (large end) must be 10.000"
        );

        let taper = p
            .geometry
            .iter()
            .find(|r| r.label == "Taper per coil")
            .unwrap();
        assert_eq!(taper.value, "0.8000", "Taper per coil must be 0.8000 mm");

        let solid = p
            .geometry
            .iter()
            .find(|r| r.label == "Solid length (conservative)")
            .unwrap();
        assert_eq!(solid.value, "24.0000", "Solid length must be 24.0000 mm");
    }

    // ── results_view_maps_error_empty_populated ─────────────────────────────

    #[test]
    fn results_view_maps_error_empty_populated() {
        // Blank form → Empty.
        let blank = fresh_app_conical();
        assert_eq!(con_results_view(&blank), ConResultsView::Empty);

        // Error set → Error.
        let mut err_app = fresh_app_conical();
        err_app.error = Some("bad".to_string());
        assert!(matches!(
            con_results_view(&err_app),
            ConResultsView::Error(_)
        ));

        // Solved → Populated.
        let solved = solved_metric_app();
        assert!(matches!(
            con_results_view(&solved),
            ConResultsView::Populated(_)
        ));
    }

    // ── huge_finite_load_renders_scientific ─────────────────────────────────

    #[test]
    fn huge_finite_load_renders_scientific() {
        // loads="1e9" N: shear stress will be far above SCI_THRESHOLD (1e6 MPa),
        // so fmt_row_value must switch to scientific notation in the stress cell.
        let mut app = fresh_app_conical();
        app.conical = ConFormState {
            loads: "1e9".into(),
            ..metric_form()
        };
        app.recompute();

        assert!(app.con_outcome.is_some(), "must solve even with huge load");
        let p = con_populated(&app);
        let stress_cell = &p.load_table.rows[0].stress;
        assert!(
            stress_cell.contains('e') && stress_cell.len() < 12,
            "expected scientific notation in stress cell, got '{stress_cell}'"
        );
    }

    // ── stress_emphasis ──────────────────────────────────────────────────────

    #[test]
    fn overstressed_load_point_carries_danger_emphasis() {
        // Reuses the huge_finite_load fixture: loads="1e9" N drives pct_mts far
        // past 1.0.
        let mut app = fresh_app_conical();
        app.conical = ConFormState {
            loads: "1e9".into(),
            ..metric_form()
        };
        app.recompute();
        let p = con_populated(&app);
        assert_eq!(p.load_table.rows[0].stress_emphasis, Emphasis::Danger);
    }

    #[test]
    fn normal_load_point_carries_normal_emphasis() {
        let p = con_populated(&solved_metric_app());
        assert_eq!(p.load_table.rows[0].stress_emphasis, Emphasis::Normal);
    }

    /// Gap case: pct_mts (72.3%) sits between Music Wire's 45% allowable and
    /// 100% MTS. The engine's own status warning already calls this
    /// overstressed (`evaluate_status` fires at `pct_mts > allowable_pct_torsion`),
    /// but the old `pct_mts > 1.0` rule rendered it Normal.
    #[test]
    fn gap_case_overstressed_by_engine_carries_danger_emphasis() {
        let mut app = fresh_app_conical();
        app.conical = ConFormState {
            loads: "200".into(),
            ..metric_form()
        };
        app.recompute();
        let p = con_populated(&app);
        assert_eq!(p.load_table.rows[0].stress_emphasis, Emphasis::Danger);
    }

    // ── telescoping_message_passes_through ──────────────────────────────────

    #[test]
    fn telescoping_message_passes_through() {
        // large_mean=92, small_mean=52, wire=2, Na=10 — per-coil radial step
        // (92-52)/2/(10*1) = 2 mm ≥ wire_dia=2 mm → telescoping flag.
        let mut app = fresh_app_conical();
        app.conical = ConFormState {
            wire_dia: "2".into(),
            large_mean_dia: "92".into(),
            small_mean_dia: "52".into(),
            active: "10".into(),
            free_length: "200".into(),
            loads: "10".into(),
            ..ConFormState::default()
        };
        app.recompute();
        assert!(app.con_outcome.is_some(), "telescoping fixture must solve");

        let lines = con_status_view(&app);
        let telescoping_msg = "coils telescope (per-coil radial step \u{2265} wire diameter); \
the reported solid length is conservative \u{2014} the true solid height is lower \
and the reported at-solid stress is correspondingly understated";
        assert!(
            lines.iter().any(|l| l.text == telescoping_msg),
            "telescoping status message must pass through; got: {lines:?}"
        );
    }

    // ── geometry_pitch_sci_notation ─────────────────────────────────────────

    /// Pin that the Pitch geometry cell uses scientific notation for a
    /// huge-but-finite free_length (≥ SCI_THRESHOLD after display_len scaling).
    ///
    /// Revert-probe: swap fmt_row_value for raw fixed-point in the Pitch row
    /// → this test FAILS → restore → green.
    #[test]
    fn geometry_pitch_renders_scientific_for_huge_free_length() {
        // free_length=1e299 mm → pitch ≈ 1e299/10 = 1e298 mm, far above SCI_THRESHOLD=1e6.
        let mut app = fresh_app_conical();
        app.conical = ConFormState {
            free_length: "1e299".into(),
            ..metric_form()
        };
        app.recompute();

        assert!(
            app.con_outcome.is_some(),
            "huge finite free_length must still solve (not a physics error)"
        );
        let p = con_populated(&app);
        let pitch_row = p
            .geometry
            .iter()
            .find(|r| r.label == "Pitch")
            .expect("Pitch row must be present");
        assert!(
            pitch_row.value.contains('e'),
            "Pitch must render in scientific notation for huge free_length; got '{}'",
            pitch_row.value
        );
    }

    // ── footer_constant_is_exact ────────────────────────────────────────────

    #[test]
    fn footer_constant_is_exact() {
        assert_eq!(
            CON_LINEAR_MODEL_NOTE,
            "Linear-range model: progressive stiffening as coils bottom out is not modeled."
        );
    }
}
