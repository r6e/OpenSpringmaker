//! Assembly presenter — pure data types + mapping from `App` state.
//! iced-free per ADR 0008.

use crate::app::App;
use crate::presenter::{
    append_status_messages, display_force, display_len, display_rate, display_stress,
    fmt_row_value, overstress_emphasis, unit_force_label, unit_length_label, unit_rate_label,
    unit_stress_label, Emphasis, GoverningRate, LoadRow, LoadTable, ResultRow, StatusLine,
};
use springcore::assembly::{evaluate_status, AssemblyDesign, MemberResult, Topology};
use springcore::{Material, MaterialStore, SpringDesign, UnitSystem};

// ── Populated results ─────────────────────────────────────────────────────────

/// Everything the assembly results panel shows when a design is solved.
#[derive(Debug, Clone, PartialEq)]
pub struct AsmPopulatedResults {
    pub governing_rate: GoverningRate,
    pub summary: Vec<ResultRow>,
    /// Assembly-level load table: force / deflection / length only — no shear
    /// stress (stress lives in the per-member tables).
    pub assembly_loads: LoadTable,
    pub members: Vec<AsmMemberResultView>,
}

/// One member's display data in the populated results.
#[derive(Debug, Clone, PartialEq)]
pub struct AsmMemberResultView {
    /// `"Member N (Music Wire)"` heading.
    pub heading: String,
    /// Share %, rate, spring index, buckling flag.
    pub rows: Vec<ResultRow>,
    /// Per-member load table: full force / deflection / length / stress / % MTS.
    pub loads: LoadTable,
}

// ── Results view enum ─────────────────────────────────────────────────────────

/// The three mutually-exclusive states of the assembly results panel.
#[derive(Debug, Clone, PartialEq)]
pub enum AsmResultsView {
    /// A parse/solve error.
    Error(String),
    /// Inputs are empty or invalid; nothing to show.
    Empty,
    /// A solved design with results ready to render.
    Populated(Box<AsmPopulatedResults>),
}

/// Build the assembly results panel view model from app state.
///
/// Outcome-first ordering (the conical ordering-trap lesson): a solved outcome
/// wins over any stale error string. Blank state with neither is Empty.
pub fn asm_results_view(app: &App) -> AsmResultsView {
    match &app.asm_outcome {
        Some(out) => AsmResultsView::Populated(Box::new(asm_populated_results(out, app))),
        None => match &app.error {
            Some(e) => AsmResultsView::Error(e.clone()),
            None => AsmResultsView::Empty,
        },
    }
}

fn asm_populated_results(out: &AssemblyDesign, app: &App) -> AsmPopulatedResults {
    let us = app.unit_system;
    AsmPopulatedResults {
        governing_rate: GoverningRate::from_rate(out.rate, us),
        summary: asm_summary_rows(out, us),
        assembly_loads: asm_assembly_load_table(out, us),
        members: out
            .members
            .iter()
            .enumerate()
            .map(|(i, mr)| asm_member_result_view(i, mr, us, &app.materials))
            .collect(),
    }
}

/// The six assembly-level summary rows (labels exact per spec).
fn asm_summary_rows(out: &AssemblyDesign, us: UnitSystem) -> Vec<ResultRow> {
    let len = unit_length_label(us);
    let force = unit_force_label(us);
    let topology_text = match out.topology {
        Topology::Nested => "Nested",
        Topology::Series => "Series",
    };
    vec![
        ResultRow::new("Topology", topology_text, ""),
        ResultRow::new(
            "Free length",
            fmt_row_value(display_len(out.free_length, us), 4),
            len,
        ),
        ResultRow::new(
            "Solid length",
            fmt_row_value(display_len(out.solid_length, us), 4),
            len,
        ),
        ResultRow::new(
            "Travel limit",
            fmt_row_value(display_len(out.travel_limit_deflection, us), 4),
            len,
        ),
        ResultRow::new(
            "Travel-limit force",
            fmt_row_value(display_force(out.travel_limit_force, us), 3),
            force,
        ),
        ResultRow::new(
            "Limited by",
            format!("member {}", out.limiting_member + 1),
            "",
        ),
    ]
}

/// Assembly-level load table: force / deflection / length only.
///
/// `AssemblyLoadPoint` carries no per-load shear stress — stress lives in the
/// per-member load tables. The `stress` and `pct_mts` fields are left empty;
/// the view renders a 4-column table (Pt / Force / Deflection / Length) rather
/// than the 6-column per-member table.
fn asm_assembly_load_table(out: &AssemblyDesign, us: UnitSystem) -> LoadTable {
    let rows = out
        .load_points
        .iter()
        .enumerate()
        .map(|(i, lp)| LoadRow {
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
            stress: String::new(),
            pct_mts: String::new(),
            stress_emphasis: Emphasis::Normal,
        })
        .collect();
    // stress_unit="" signals to the view that the stress columns are absent.
    LoadTable {
        stress_unit: String::new(),
        rows,
    }
}

fn asm_member_result_view(
    i: usize,
    mr: &MemberResult,
    us: UnitSystem,
    materials: &MaterialStore,
) -> AsmMemberResultView {
    let buckling_row = if mr.design.buckling_stable {
        ResultRow::new("Buckling", "stable", "")
    } else {
        ResultRow::danger("Buckling", "buckling risk", "")
    };
    // Each member carries its own material (Decision 2); a present outcome
    // means it already resolved during that solve (the conical precedent),
    // so `.ok()` degrades gracefully rather than panicking on the
    // documented-unreachable race where it no longer does.
    let material = materials.get(&mr.material_name).ok();
    AsmMemberResultView {
        heading: format!("Member {} ({})", i + 1, mr.material_name),
        rows: vec![
            ResultRow::new("Share", fmt_row_value(mr.share_fraction * 100.0, 1), "%"),
            ResultRow::new(
                "Spring rate",
                fmt_row_value(display_rate(mr.design.rate, us), 4),
                unit_rate_label(us),
            ),
            ResultRow::new("Spring index", fmt_row_value(mr.design.index, 3), ""),
            buckling_row,
        ],
        loads: member_load_table(&mr.design, us, material),
    }
}

/// Per-member load table: full 6-column force / deflection / length / stress /
/// % MTS. Mirrors `con_load_table` in the conical presenter.
fn member_load_table(d: &SpringDesign, us: UnitSystem, material: Option<&Material>) -> LoadTable {
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

// ── Status ────────────────────────────────────────────────────────────────────

/// Status lines for the calculator's shared status panel. Single source of truth
/// for assembly status; status is NOT carried on the populated-results struct —
/// it renders only through the shared panel (ADR 0008).
pub fn asm_status_view(app: &App) -> Vec<StatusLine> {
    let mut lines = crate::presenter::common_status_lines(app);
    if let Some(out) = &app.asm_outcome {
        append_status_messages(&mut lines, &evaluate_status(out, &app.materials).messages);
    }
    lines
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::form::{AsmFormState, AsmMemberForm};
    use springcore::{CurvatureCorrection, Family, MaterialSet, MaterialStore};

    fn store() -> MaterialStore {
        MaterialStore::new(MaterialSet::load_default())
    }

    fn fresh_asm_app() -> App {
        let mut app = App::from_store(store(), Vec::new(), CurvatureCorrection::Bergstrasser);
        app.family = Family::Assembly;
        app
    }

    /// Nested two-member metric fixture: wire=2/1.5mm, mean=20/16mm,
    /// active=10/8 coils, free=60mm each, loads=[10N, 25N].
    fn two_member_form() -> AsmFormState {
        let mut f = AsmFormState::with_default_material("Music Wire");
        f.loads = "10, 25".into();
        f.members[0] = AsmMemberForm {
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            active: "10".into(),
            free_length: "60".into(),
            ..AsmMemberForm::blank("Music Wire")
        };
        f.members.push(AsmMemberForm {
            wire_dia: "1.5".into(),
            mean_dia: "16".into(),
            active: "8".into(),
            free_length: "60".into(),
            ..AsmMemberForm::blank("Music Wire")
        });
        f
    }

    fn solved_asm_app() -> App {
        let mut app = fresh_asm_app();
        app.assembly = two_member_form();
        app.recompute();
        app
    }

    fn asm_populated(app: &App) -> AsmPopulatedResults {
        match asm_results_view(app) {
            AsmResultsView::Populated(p) => *p,
            other => panic!("expected Populated, got {other:?}"),
        }
    }

    // ── summary_and_member_rows_exact ─────────────────────────────────────────

    #[test]
    fn summary_and_member_rows_exact() {
        // Two-member nested metric fixture → six summary labels + spot values.
        let app = solved_asm_app();
        let p = asm_populated(&app);

        // All six labels in order.
        let labels: Vec<&str> = p.summary.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "Topology",
                "Free length",
                "Solid length",
                "Travel limit",
                "Travel-limit force",
                "Limited by",
            ]
        );

        // Topology is "Nested" for the default fixture.
        let topo = p.summary.iter().find(|r| r.label == "Topology").unwrap();
        assert_eq!(topo.value, "Nested");
        assert_eq!(topo.unit, "");

        // "Limited by" has the 1-indexed member format and empty unit.
        let limited_by = p.summary.iter().find(|r| r.label == "Limited by").unwrap();
        assert!(
            limited_by.value.starts_with("member "),
            "Limited by must be 'member N', got {:?}",
            limited_by.value
        );
        assert_eq!(limited_by.unit, "");

        // Two members present.
        assert_eq!(p.members.len(), 2);

        // Member 1 heading verbatim.
        assert_eq!(p.members[0].heading, "Member 1 (Music Wire)");

        // Member 1 has a "Share" row with "%" unit.
        let share = p.members[0].rows.iter().find(|r| r.label == "Share");
        assert!(share.is_some(), "member 1 must have a Share row");
        assert_eq!(share.unwrap().unit, "%");

        // Assembly-level load table: 2 rows, no stress at assembly level.
        assert_eq!(p.assembly_loads.rows.len(), 2, "two load points → two rows");
        assert_eq!(
            p.assembly_loads.stress_unit, "",
            "assembly table must carry empty stress_unit (no stress at assembly level)"
        );
        assert!(
            p.assembly_loads.rows[0].stress.is_empty(),
            "assembly LoadRow stress cell must be empty"
        );
        assert!(
            p.assembly_loads.rows[0].pct_mts.is_empty(),
            "assembly LoadRow pct_mts cell must be empty"
        );
        // Metric fixture → force unit is N.
        assert!(
            p.assembly_loads.rows[0].force.ends_with(" N"),
            "metric assembly force cell must end with ' N', got {:?}",
            p.assembly_loads.rows[0].force
        );
    }

    // ── results_view_tristate ─────────────────────────────────────────────────

    #[test]
    fn results_view_tristate() {
        // Blank form → Empty.
        let blank = fresh_asm_app();
        assert_eq!(asm_results_view(&blank), AsmResultsView::Empty);

        // Error set (no outcome) → Error.
        let mut err_app = fresh_asm_app();
        err_app.error = Some("bad input".to_string());
        assert!(matches!(
            asm_results_view(&err_app),
            AsmResultsView::Error(_)
        ));

        // Solved → Populated.
        let solved = solved_asm_app();
        assert!(matches!(
            asm_results_view(&solved),
            AsmResultsView::Populated(_)
        ));
    }

    // ── huge_finite_load_member_stress_is_scientific ──────────────────────────

    #[test]
    fn huge_finite_load_member_stress_is_scientific() {
        // loads = "1e9" N: member shear stress is far above SCI_THRESHOLD (1e6 MPa/Pa),
        // so fmt_row_value switches to scientific notation in the per-member stress cell.
        let mut app = fresh_asm_app();
        app.assembly = AsmFormState {
            loads: "1e9".into(),
            ..two_member_form()
        };
        app.recompute();

        assert!(
            app.asm_outcome.is_some(),
            "must solve even with a huge load"
        );
        let p = asm_populated(&app);
        let has_sci = p.members.iter().any(|m| {
            m.loads
                .rows
                .iter()
                .any(|row| row.stress.contains('e') && row.stress.len() < 12)
        });
        assert!(
            has_sci,
            "at least one member stress cell must use scientific notation for 1e9 N; got: {:?}",
            p.members
                .iter()
                .map(|m| m.loads.rows.iter().map(|r| &r.stress).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        );
    }

    // ── stress_emphasis ────────────────────────────────────────────────────────

    #[test]
    fn overstressed_member_load_point_carries_danger_emphasis() {
        // Reuses the huge_finite_load fixture: loads = "1e9" N drives every
        // member's pct_mts far past 1.0.
        let mut app = fresh_asm_app();
        app.assembly = AsmFormState {
            loads: "1e9".into(),
            ..two_member_form()
        };
        app.recompute();
        let p = asm_populated(&app);
        assert_eq!(p.members[0].loads.rows[0].stress_emphasis, Emphasis::Danger);
    }

    #[test]
    fn normal_member_load_point_carries_normal_emphasis() {
        let p = asm_populated(&solved_asm_app());
        assert_eq!(p.members[0].loads.rows[0].stress_emphasis, Emphasis::Normal);
    }

    /// Gap case: member 1's pct_mts (56.8%) sits between Music Wire's 45%
    /// allowable and 100% MTS (member 0 stays at 40.8%, still below allowable
    /// at this load — Nested topology splits load by rate fraction, so
    /// members land at different stresses for the same assembly load). The
    /// engine's own status warning already calls member 1 overstressed
    /// (`evaluate_status` fires at `pct_mts > allowable_pct_torsion`), but the
    /// old `pct_mts > 1.0` rule rendered it Normal.
    #[test]
    fn gap_case_overstressed_by_engine_carries_danger_emphasis() {
        let mut app = fresh_asm_app();
        app.assembly = AsmFormState {
            loads: "200".into(),
            ..two_member_form()
        };
        app.recompute();
        let p = asm_populated(&app);
        assert_eq!(p.members[1].loads.rows[0].stress_emphasis, Emphasis::Danger);
    }

    #[test]
    fn assembly_level_empty_stress_row_carries_normal_emphasis() {
        // Assembly-level rows have no per-load stress (String::new()); the
        // presenter must set Normal rather than leave emphasis undetermined.
        let p = asm_populated(&solved_asm_app());
        assert_eq!(p.assembly_loads.rows[0].stress_emphasis, Emphasis::Normal);
    }

    // ── member_prefixed_status_passthrough ────────────────────────────────────

    #[test]
    fn member_prefixed_status_passthrough() {
        // Part 1: clearance-interfering nested pair → "nested interference".
        // Outer: D=20mm, d=2mm; Inner: D=16mm, d=2mm (OD_inner=18mm = ID_outer → ≥ boundary).
        let mut app = fresh_asm_app();
        let mut f = AsmFormState::with_default_material("Music Wire");
        f.loads = "10".into();
        f.topology = "nested".into();
        f.members[0] = AsmMemberForm {
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            active: "10".into(),
            free_length: "60".into(),
            ..AsmMemberForm::blank("Music Wire")
        };
        f.members.push(AsmMemberForm {
            wire_dia: "2".into(),
            mean_dia: "16".into(),
            active: "10".into(),
            free_length: "60".into(),
            ..AsmMemberForm::blank("Music Wire")
        });
        app.assembly = f;
        app.recompute();
        assert!(app.asm_outcome.is_some(), "clearance fixture must solve");

        let lines = asm_status_view(&app);
        assert!(
            lines.iter().any(|l| l.text.contains("nested interference")),
            "must contain 'nested interference'; got: {lines:?}"
        );

        // Part 2: member overstress → "member 2: load point" in status.
        // Thin wire (d=0.5mm, D=20mm, Na=6) in series at 40N → stress >> torsion allowable.
        let mut app2 = fresh_asm_app();
        let mut f2 = AsmFormState::with_default_material("Music Wire");
        f2.loads = "40".into();
        f2.topology = "series".into();
        f2.members[0] = AsmMemberForm {
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            active: "10".into(),
            free_length: "60".into(),
            ..AsmMemberForm::blank("Music Wire")
        };
        f2.members.push(AsmMemberForm {
            wire_dia: "0.5".into(),
            mean_dia: "20".into(),
            active: "6".into(),
            free_length: "30".into(),
            ..AsmMemberForm::blank("Music Wire")
        });
        app2.assembly = f2;
        app2.recompute();
        assert!(app2.asm_outcome.is_some(), "overstress fixture must solve");

        let lines2 = asm_status_view(&app2);
        assert!(
            lines2
                .iter()
                .any(|l| l.text.contains("member 2: load point")),
            "must contain 'member 2: load point'; got: {lines2:?}"
        );
    }

    // ── limiting_member_callout ───────────────────────────────────────────────

    #[test]
    fn limiting_member_callout() {
        // The "Limited by" summary row must read "member {out.limiting_member + 1}".
        let app = solved_asm_app();
        let p = asm_populated(&app);
        let out = app.asm_outcome.as_ref().unwrap();
        let limited_by = p.summary.iter().find(|r| r.label == "Limited by").unwrap();
        assert_eq!(
            limited_by.value,
            format!("member {}", out.limiting_member + 1),
            "Limited by must be 1-indexed"
        );
    }
}
