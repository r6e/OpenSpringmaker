//! Pure chart presenter for the assembly family (ADR 0008). Composite k_total
//! line + one line per member (slope kᵢ, clipped to the chart extent) — nested
//! members visibly stack; series members visibly soften. Travel limit marked.

use crate::plot::{
    convert_fd, force_deflection_axes, ChartData, Line, LineRole, Marker, MarkerKind,
};
use springcore::assembly::AssemblyDesign;
use springcore::UnitSystem;

pub fn assembly_chart(design: &AssemblyDesign, units: UnitSystem) -> ChartData {
    let (x_axis, y_axis) = force_deflection_axes(units);
    let k_total = design.rate.newtons_per_meter();
    let rate_ok = k_total.is_finite() && k_total > 0.0;

    // Extent: max operating deflection and the travel limit (spec extent rule).
    let x_max_m = design
        .load_points
        .iter()
        .map(|lp| lp.deflection.meters())
        .chain(std::iter::once(design.travel_limit_deflection.meters()))
        .filter(|v| v.is_finite())
        .fold(0.0_f64, f64::max);

    let mut lines = Vec::with_capacity(1 + design.members.len());
    if rate_ok && x_max_m > 0.0 {
        let f_cap_n = k_total * x_max_m;
        lines.push(Line {
            points: vec![(0.0, 0.0), convert_fd(x_max_m, f_cap_n, units)],
            role: LineRole::Primary,
            name: Some("Assembly".to_string()),
        });
        for (i, member) in design.members.iter().enumerate() {
            let k_i = member.design.rate.newtons_per_meter();
            if !(k_i.is_finite() && k_i > 0.0) {
                continue;
            }
            // Clip: end at whichever the member line hits first — the chart's
            // right edge (x_max) or its top edge (f_cap).
            let end_x_m = x_max_m.min(f_cap_n / k_i);
            lines.push(Line {
                points: vec![(0.0, 0.0), convert_fd(end_x_m, k_i * end_x_m, units)],
                role: LineRole::Member,
                name: Some(format!("Member {}", i + 1)),
            });
        }
    }

    // Markers are gated on the SAME rate validity as the lines: a degenerate
    // rate means the operating/travel-limit state derived from it is no
    // longer meaningful, so markers are suppressed too — the
    // round_wire_force_deflection / torsion_chart convention (`plot/mod.rs`,
    // `torsion/plot_model.rs`) — keeping `chart_extent` `None` and the whole
    // design out of plotters rather than showing orphaned markers on an empty
    // chart.
    let markers = if rate_ok {
        let mut markers: Vec<Marker> = design
            .load_points
            .iter()
            .map(|lp| {
                let (x, y) = convert_fd(lp.deflection.meters(), lp.force.newtons(), units);
                Marker {
                    x,
                    y,
                    kind: MarkerKind::Operating,
                }
            })
            .collect();
        let (tx, ty) = convert_fd(
            design.travel_limit_deflection.meters(),
            design.travel_limit_force.newtons(),
            units,
        );
        markers.push(Marker {
            x: tx,
            y: ty,
            kind: MarkerKind::Limit,
        });
        markers
    } else {
        vec![]
    };

    ChartData {
        x_axis,
        y_axis,
        lines,
        markers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::form::{parse_and_solve, AsmFormState, AsmMemberForm};
    use approx::assert_relative_eq;
    use springcore::{CurvatureCorrection, MaterialSet, MaterialStore, SpringRate};

    fn store() -> MaterialStore {
        MaterialStore::new(MaterialSet::load_default())
    }

    /// Two-member metric fixture (wire=2/1.5mm, mean=20/16mm, active=10/8
    /// coils, free=60mm each, loads=[10N, 25N]) — mirrors `two_member_form` in
    /// `assembly/view_model.rs` tests, parameterized on topology.
    fn two_member_form(topology: &str) -> AsmFormState {
        let mut f = AsmFormState::with_default_material("Music Wire");
        f.topology = topology.to_string();
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

    fn solve(form: &AsmFormState) -> AssemblyDesign {
        parse_and_solve(
            form,
            UnitSystem::Metric,
            &store(),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap()
    }

    /// Nested topology: k_total = Σkᵢ, so every member rate is strictly below
    /// the composite rate.
    fn nested_two_member_design() -> AssemblyDesign {
        solve(&two_member_form("nested"))
    }

    /// Series topology: 1/k_total = Σ(1/kᵢ), so every member rate is strictly
    /// above the composite rate (harmonic mean < any component).
    fn series_two_member_design() -> AssemblyDesign {
        solve(&two_member_form("series"))
    }

    #[test]
    fn nested_members_sit_below_composite() {
        let d = nested_two_member_design();
        let data = assembly_chart(&d, UnitSystem::Metric);
        // 1 composite + N members, composite first.
        assert_eq!(data.lines.len(), 1 + d.members.len());
        assert_eq!(data.lines[0].role, LineRole::Primary);
        assert_eq!(data.lines[0].name.as_deref(), Some("Assembly"));
        assert_eq!(data.lines[1].name.as_deref(), Some("Member 1"));
        // Nested: every member rate < composite rate ⇒ member end-y < composite end-y at shared x.
        let comp_end = *data.lines[0].points.last().unwrap();
        for member_line in &data.lines[1..] {
            let end = *member_line.points.last().unwrap();
            assert!(
                end.1 <= comp_end.1 + 1e-9,
                "nested member lines sit at/below the composite"
            );
            assert_eq!(member_line.role, LineRole::Member);
        }
    }

    #[test]
    fn series_members_are_clipped_to_chart_extent() {
        let d = series_two_member_design();
        let data = assembly_chart(&d, UnitSystem::Metric);
        assert_eq!(data.lines.len(), 1 + d.members.len());
        let comp_end = *data.lines[0].points.last().unwrap();
        for member_line in &data.lines[1..] {
            let end = *member_line.points.last().unwrap();
            // Series members are STIFFER than the composite; their lines clip at the
            // composite y extent instead of overshooting the chart.
            assert!(end.1 <= comp_end.1 * (1.0 + 1e-9));
            assert!(end.0 <= comp_end.0 * (1.0 + 1e-9));
        }
    }

    #[test]
    fn travel_limit_is_the_single_limit_marker() {
        let d = nested_two_member_design();
        let data = assembly_chart(&d, UnitSystem::Metric);
        // One Operating marker per load point plus exactly one Limit marker —
        // asserted by length (not just filtered count) so a presenter bug that
        // drops/duplicates Operating markers can't hide behind the filter.
        assert_eq!(data.markers.len(), d.load_points.len() + 1);
        let limits: Vec<_> = data
            .markers
            .iter()
            .filter(|m| m.kind == MarkerKind::Limit)
            .collect();
        assert_eq!(limits.len(), 1);
        assert_relative_eq!(
            limits[0].x,
            d.travel_limit_deflection.millimeters(),
            max_relative = 1e-9
        );
        assert_relative_eq!(
            limits[0].y,
            d.travel_limit_force.newtons(),
            max_relative = 1e-9
        );
    }

    /// A degenerate (zero) rate must suppress BOTH lines and markers — the
    /// documented convention shared with `round_wire_force_deflection` and
    /// `torsion_chart`. `chart_extent` must return `None` so the design falls
    /// through to the placeholder rather than rendering orphaned markers on
    /// an axis-less chart.
    #[test]
    fn invalid_rate_yields_degenerate_chart() {
        let mut d = nested_two_member_design();
        d.rate = SpringRate::from_newtons_per_meter(0.0);
        let data = assembly_chart(&d, UnitSystem::Metric);
        assert!(data.lines.is_empty());
        assert!(data.markers.is_empty());
        assert!(crate::plot::chart_extent(&data).is_none());
    }
}
