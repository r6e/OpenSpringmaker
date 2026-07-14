//! Pure 2D-diagram dimension presenter for the conical family. Large end at
//! axial 0, small end at axial `free_length` (see conical scene_model).

use crate::diagram::{common, DimKind, DimLayer, Dimension};
use springcore::conical::ConicalDesign;

pub fn dimensions(design: &ConicalDesign) -> Vec<Dimension> {
    let l0 = design.inputs.free_length.millimeters();
    let large_od = design.large_outer_dia.millimeters();
    let large_id = design.large_inner_dia.millimeters();
    let small_od = design.small_outer_dia.millimeters();
    let small_id = design.small_inner_dia.millimeters();
    let wire = design.inputs.wire_dia.millimeters();
    let na = design.inputs.active_coils;
    let nt = design.total_coils;

    vec![
        common::free_length(l0),
        Dimension {
            kind: DimKind::Diameter {
                at_axial: 0.0,
                half: large_od / 2.0,
            },
            layer: DimLayer::Diameters,
            value: large_od,
            label: format!("large OD {}", common::mm(large_od)),
            at: (0.0, large_od / 2.0),
        },
        Dimension {
            kind: DimKind::Diameter {
                at_axial: 0.0,
                half: large_id / 2.0,
            },
            layer: DimLayer::Diameters,
            value: large_id,
            label: format!("large ID {}", common::mm(large_id)),
            at: (0.0, large_id / 2.0),
        },
        Dimension {
            kind: DimKind::Diameter {
                at_axial: l0,
                half: small_od / 2.0,
            },
            layer: DimLayer::Diameters,
            value: small_od,
            label: format!("small OD {}", common::mm(small_od)),
            at: (l0, small_od / 2.0),
        },
        Dimension {
            kind: DimKind::Diameter {
                at_axial: l0,
                half: small_id / 2.0,
            },
            layer: DimLayer::Diameters,
            value: small_id,
            label: format!("small ID {}", common::mm(small_id)),
            at: (l0, small_id / 2.0),
        },
        common::wire_note(wire, (l0 / 2.0, large_od / 2.0)),
        common::coil_note(na, nt, (l0 / 2.0, 0.0)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conical::form::ConFormState;
    use crate::conical::scene_model::conical_scene;
    use crate::diagram::{project_silhouette, DimKind, DimLayer};
    use approx::assert_relative_eq;
    use springcore::{CurvatureCorrection, MaterialSet, MaterialStore, UnitSystem};

    fn design() -> springcore::conical::ConicalDesign {
        let materials = MaterialStore::new(MaterialSet::load_default());
        let form = ConFormState {
            end_type: "squared_ground".into(),
            wire_dia: "2".into(),
            large_mean_dia: "20".into(),
            small_mean_dia: "12".into(),
            active: "10".into(),
            free_length: "60".into(),
            loads: "10, 25".into(),
        };
        crate::conical::form::parse_and_solve(
            &form,
            "Music Wire",
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .unwrap()
        .design
    }

    fn find(dims: &[Dimension], s: &str) -> Dimension {
        dims.iter()
            .find(|d| d.label.contains(s))
            .cloned()
            .unwrap_or_else(|| panic!("no dim containing {s}"))
    }

    #[test]
    fn large_and_small_od_anchor_to_the_projected_ends() {
        let d = design(); // large mean 20, small 12, wire 2, free 60, active 10 → total 12 (integer)
        let dims = dimensions(&d);
        let large = find(&dims, "large OD");
        let small = find(&dims, "small OD");
        // Presenter ↔ design: EXACT.
        assert_relative_eq!(
            large.value,
            d.large_outer_dia.millimeters(),
            max_relative = 1e-9
        );
        assert_relative_eq!(
            small.value,
            d.small_outer_dia.millimeters(),
            max_relative = 1e-9
        );
        // Anchors: large OD at the large end (axial 0), small OD at the free length; halves == OD/2.
        let DimKind::Diameter {
            at_axial: la,
            half: lhalf,
        } = large.kind
        else {
            panic!("large OD must be a Diameter")
        };
        let DimKind::Diameter {
            at_axial: sa,
            half: shalf,
        } = small.kind
        else {
            panic!("small OD must be a Diameter")
        };
        assert_relative_eq!(la, 0.0, epsilon = 1e-9);
        assert_relative_eq!(sa, d.inputs.free_length.millimeters(), max_relative = 1e-9);
        assert_relative_eq!(
            2.0 * lhalf,
            d.large_outer_dia.millimeters(),
            max_relative = 1e-9
        );
        assert_relative_eq!(
            2.0 * shalf,
            d.small_outer_dia.millimeters(),
            max_relative = 1e-9
        );
        assert_eq!(large.layer, DimLayer::Diameters);
        assert_eq!(small.layer, DimLayer::Diameters);
        // Mirror-drift vs geometry (EXACT, drop-z-robust): the silhouette
        // edge-MIDPOINT equals the centerline radius (the ±wire/2 offset cancels),
        // independent of the discrete perpendicular. At the first sample (large
        // end, θ=0) it is the large mean/2; at the last sample (small end,
        // θ=total·2π with integer total → cos≈1) the small mean/2. This is the
        // exact tie; the envelope PEAK is only sampling-approximate under drop-z.
        let p = project_silhouette(&conical_scene(&d)).unwrap();
        let last = p.edges[0].points.len() - 1;
        let mid = |i: usize| (p.edges[0].points[i].1 + p.edges[1].points[i].1) / 2.0;
        assert_relative_eq!(
            mid(0),
            d.inputs.large_mean_dia.millimeters() / 2.0,
            max_relative = 1e-9
        );
        assert_relative_eq!(
            mid(last),
            d.inputs.small_mean_dia.millimeters() / 2.0,
            max_relative = 1e-9
        );
    }

    #[test]
    fn free_length_and_coils_present() {
        let d = design();
        let dims = dimensions(&d);
        assert_relative_eq!(
            find(&dims, "L\u{2080}").value,
            d.inputs.free_length.millimeters(),
            max_relative = 1e-9
        );
        assert_eq!(find(&dims, "N").layer, DimLayer::Coils);
    }

    /// Mirrors compression's `degenerate_design_yields_finite_labels_only`:
    /// a post-solve NaN on a field the presenter actually reads for a label
    /// (`free_length` flows into the L₀ callout) must not crash the
    /// presenter — labels stay finite-guarded (em dash, never "NaN").
    #[test]
    fn degenerate_design_yields_finite_labels_only() {
        let mut d = design();
        d.inputs.free_length = springcore::units::Length::from_millimeters(f64::NAN);
        let dims = dimensions(&d);
        assert!(dims
            .iter()
            .all(|dm| dm.value.is_finite() || dm.label.contains('\u{2014}')));
        let fl = find(&dims, "L\u{2080}");
        assert!(!fl.value.is_finite());
        assert!(fl.label.contains('\u{2014}'));
    }
}
