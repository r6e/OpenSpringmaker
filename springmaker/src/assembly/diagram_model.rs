//! Pure 2D-diagram presenter for assemblies: per-member OD/wire dims anchored to
//! each member body, plus overall free/solid reference dims and a stage summary.
//! Series drawn span includes schematic gaps, so overall free length is a
//! reference dim (value from design.free_length), not a full-span anchor.
use crate::diagram::{common, DimKind, DimLayer, Dimension};
use springcore::assembly::{AssemblyDesign, Topology};

pub fn dimensions(design: &AssemblyDesign) -> Vec<Dimension> {
    let l0 = design.free_length.millimeters();
    let ls = design.solid_length.millimeters();
    let mut dims = vec![
        // Overall free length (reference; series includes schematic gaps).
        common::free_length(l0),
        Dimension {
            kind: DimKind::Linear {
                from: (0.0, 0.0),
                to: (ls, 0.0),
            },
            layer: DimLayer::Lengths,
            value: ls,
            label: format!("L\u{209B} {}", common::mm(ls)),
            at: (ls / 2.0, 0.0),
        },
    ];
    // Per-member OD/wire notes.
    let mut axial = 0.0;
    for (i, m) in design.members.iter().enumerate() {
        let od = m.design.outer_dia.millimeters();
        let wire = m.design.wire_dia.millimeters();
        let member_h = m.design.free_length.millimeters();
        let station = match design.topology {
            Topology::Nested => member_h / 2.0,
            Topology::Series => axial + member_h / 2.0,
        };
        dims.push(Dimension {
            kind: DimKind::Diameter {
                at_axial: station,
                half: od / 2.0,
            },
            layer: DimLayer::Diameters,
            value: od,
            label: format!("m{} OD {}", i + 1, common::mm(od)),
            at: (station, od / 2.0),
        });
        dims.push(common::wire_note(wire, (station, od / 2.0)));
        if design.topology == Topology::Series {
            axial += member_h; // (gap is cosmetic; per-member stations approximate)
        }
    }
    // Stage summary.
    let topo = match design.topology {
        Topology::Nested => "nested",
        Topology::Series => "series",
    };
    dims.push(Dimension {
        kind: DimKind::Note,
        layer: DimLayer::Coils,
        value: design.members.len() as f64,
        label: format!("{} stage {}", design.members.len(), topo),
        at: (l0 / 2.0, 0.0),
    });
    dims
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::form::{parse_and_solve, AsmFormState, AsmMemberForm};
    use crate::diagram::DimLayer;
    use approx::assert_relative_eq;
    use springcore::{CurvatureCorrection, MaterialSet, MaterialStore, UnitSystem};

    fn two_member(topology: &str) -> springcore::assembly::AssemblyDesign {
        let mut f = AsmFormState::with_default_material("Music Wire");
        f.topology = topology.into();
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
        parse_and_solve(
            &f,
            UnitSystem::Metric,
            &MaterialStore::new(MaterialSet::load_default()),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap()
    }

    fn find(dims: &[Dimension], s: &str) -> Dimension {
        dims.iter()
            .find(|d| d.label.contains(s))
            .cloned()
            .unwrap_or_else(|| panic!("no dim {s}"))
    }

    #[test]
    fn per_member_od_and_overall_free_length_present() {
        let d = two_member("nested");
        let dims = dimensions(&d);
        // Each member's OD appears (envelope OD = member 0's 22, inner member 17.5).
        assert!(dims.iter().filter(|x| x.label.contains("OD")).count() >= 2);
        let overall = find(&dims, "L\u{2080}");
        assert_relative_eq!(
            overall.value,
            d.free_length.millimeters(),
            max_relative = 1e-9
        );
        assert_eq!(overall.layer, DimLayer::Lengths);
    }

    #[test]
    fn series_reports_stage_summary() {
        let d = two_member("series");
        let dims = dimensions(&d);
        let stages = find(&dims, "stage");
        assert_eq!(stages.layer, DimLayer::Coils);
    }

    /// Mirrors compression's `degenerate_design_yields_finite_labels_only`:
    /// a post-solve NaN on a field the presenter actually reads for a label
    /// (`free_length` flows into the overall L₀ callout) must not crash the
    /// presenter — labels stay finite-guarded (em dash, never "NaN").
    #[test]
    fn degenerate_design_yields_finite_labels_only() {
        let mut d = two_member("nested");
        d.free_length = springcore::units::Length::from_millimeters(f64::NAN);
        let dims = dimensions(&d);
        assert!(dims
            .iter()
            .all(|dm| dm.value.is_finite() || dm.label.contains('\u{2014}')));
        let fl = find(&dims, "L\u{2080}");
        assert!(!fl.value.is_finite());
        assert!(fl.label.contains('\u{2014}'));
    }
}
