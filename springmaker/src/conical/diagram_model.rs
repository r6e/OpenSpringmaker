//! Pure 2D-diagram dimension presenter for the conical family. Large end at
//! axial 0, small end at the TOP of the coil body AS DRAWN — the rendered coil
//! height `conical_scene` tapers to, which equals `free_length` only for
//! SquaredGround ends (see `dimensions`). `free_length` stays the L₀ reference.

use crate::diagram::{common, Dimension};
use crate::viz::coil_render_height;
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
    // The small end sits at the top of the coil body AS DRAWN: `conical_scene`
    // tapers the helix from the large end (axial 0, its t=0 point) up to the
    // rendered coil height `coil_height_fn(..)(1.0)`. That top equals
    // free_length only for SquaredGround; for Squared/Plain/PlainGround ends
    // free_length exceeds the rendered body, so anchoring the small-end
    // callouts on free_length floats them past the drawn taper.
    let top = coil_render_height(na, nt, design.pitch.millimeters(), wire);

    vec![
        common::free_length(l0),
        common::diameter(0.0, large_od, format!("large OD {}", common::mm(large_od))),
        common::diameter(0.0, large_id, format!("large ID {}", common::mm(large_id))),
        common::diameter(top, small_od, format!("small OD {}", common::mm(small_od))),
        common::diameter(top, small_id, format!("small ID {}", common::mm(small_id))),
        common::wire_note(wire, (top / 2.0, large_od / 2.0)),
        common::coil_note(na, nt, (top / 2.0, 0.0)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conical::form::ConFormState;
    use crate::conical::scene_model::conical_scene;
    use crate::diagram::test_support::find;
    use crate::diagram::{project_silhouette, DimKind, DimLayer};
    use approx::assert_relative_eq;
    use springcore::{CurvatureCorrection, MaterialSet, MaterialStore, UnitSystem};

    /// Every user-selectable conical end type. Only SquaredGround has rendered
    /// coil height == free_length, so the taper-anchor sweep must exercise all
    /// four; a SquaredGround-only fixture passes the small-end check vacuously.
    const ALL_END_TYPES: [&str; 4] = ["squared_ground", "squared", "plain", "plain_ground"];

    fn build(end_type: &str) -> springcore::conical::ConicalDesign {
        let materials = MaterialStore::new(MaterialSet::load_default());
        let form = ConFormState {
            end_type: end_type.into(),
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

    fn design() -> springcore::conical::ConicalDesign {
        build("squared_ground")
    }

    /// (large-end y, small-end y) of the coil body AS DRAWN by `conical_scene` —
    /// the helix's first and last INDEPENDENTLY sampled y coordinates, so the
    /// anchor checks pin presenter-tracks-geometry, not the same helper twice.
    fn drawn_taper_ends(d: &springcore::conical::ConicalDesign) -> (f64, f64) {
        let body = &conical_scene(d).polylines[0];
        (body.points[0].1, body.points.last().unwrap().1)
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
        // Axial anchors track the DRAWN taper endpoints (the scene helix's
        // first/last y), NOT free_length — for SquaredGround the two coincide,
        // but the honest invariant is "on the drawn body". The end-type sweep
        // below exercises the ends where they diverge.
        let (large_y, small_y) = drawn_taper_ends(&d);
        assert_relative_eq!(la, large_y, epsilon = 1e-9);
        assert_relative_eq!(sa, small_y, max_relative = 1e-9);
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

    /// The small-end callouts anchor on the DRAWN taper top (rendered coil
    /// height), across EVERY end type — not on free_length, which exceeds the
    /// rendered body for all ends but SquaredGround. Large-end callouts stay at
    /// the taper base (axial 0); the wire/coil notes sit at the drawn
    /// mid-height. Compared against the scene helix's own endpoints.
    #[test]
    fn small_end_callouts_track_the_drawn_taper_across_end_types() {
        for end_type in ALL_END_TYPES {
            let d = build(end_type);
            let dims = dimensions(&d);
            let (large_y, small_y) = drawn_taper_ends(&d);
            assert_relative_eq!(find(&dims, "large OD").at.0, large_y, epsilon = 1e-9);
            assert_relative_eq!(find(&dims, "large ID").at.0, large_y, epsilon = 1e-9);
            assert_relative_eq!(find(&dims, "small OD").at.0, small_y, max_relative = 1e-9);
            assert_relative_eq!(find(&dims, "small ID").at.0, small_y, max_relative = 1e-9);
            assert_relative_eq!(
                find(&dims, "\u{2300}").at.0,
                small_y / 2.0,
                max_relative = 1e-9
            );
            assert_relative_eq!(find(&dims, "N").at.0, small_y / 2.0, max_relative = 1e-9);
        }
    }

    /// The small-end anchor moved off `free_length` onto the rendered coil
    /// height (active/total/pitch/wire). A NaN in one of THOSE fields must not
    /// produce a non-finite anchor: `coil_render_height`'s shared guard returns
    /// 0.0. Mirrors the assembly degenerate discipline (NaN a field the anchor
    /// actually reads).
    #[test]
    fn degenerate_coil_count_yields_finite_callout_anchors() {
        let mut d = design();
        d.inputs.active_coils = f64::NAN;
        let dims = dimensions(&d);
        for label in ["small OD", "small ID", "\u{2300}", "N"] {
            let at = find(&dims, label).at;
            assert!(
                at.0.is_finite() && at.1.is_finite(),
                "{label} anchor non-finite"
            );
        }
    }
}
