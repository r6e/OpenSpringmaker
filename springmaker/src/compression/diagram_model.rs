//! Pure 2D-diagram dimension presenter for the compression family (ADR 0008).
//! Anchors are in projection space `(axial, radial)` model mm. The L₀ reference
//! dim spans `[0, free_length]`; the diameter/wire/coil callouts anchor on the
//! axial center of the coil body AS DRAWN — the rendered coil height, which
//! diverges from free_length for every end type except SquaredGround (see
//! `dimensions`). The radial envelope is ±OD/2 (see scene_model). The
//! mirror-drift equality against the drawn geometry is asserted in tests.

use crate::diagram::{common, Dimension};
use crate::viz::coil_render_height;
use springcore::SpringDesign;

pub fn dimensions(design: &SpringDesign) -> Vec<Dimension> {
    let l0 = design.free_length.millimeters();
    let ls = design.solid_length.millimeters();
    let od = design.outer_dia.millimeters();
    let id = design.inner_dia.millimeters();
    let wire = design.wire_dia.millimeters();
    let na = design.active_coils;
    let nt = design.total_coils;
    // Axial station for the diameter/wire/coil callouts: the center of the coil
    // body AS DRAWN. `compression_scene` draws the helix to `coil_render_height`
    // (coil_height_fn(..)(1.0)); that equals free_length/2 only for
    // SquaredGround ends. For Squared/Plain/PlainGround, free_length exceeds the
    // rendered body (by wire or pitch−wire), so anchoring on l0/2 floats the
    // callouts off the drawn coils. free_length remains the L₀ reference dim.
    let mid = coil_render_height(na, nt, design.pitch.millimeters(), wire) / 2.0;

    vec![
        common::free_length(l0),
        common::axial_length(ls, format!("L\u{209B} {}", common::mm(ls))), // Lₛ (reference)
        common::diameter(mid, od, format!("OD {}", common::mm(od))),
        common::diameter(mid, id, format!("ID {}", common::mm(id))),
        common::wire_note(wire, (mid, od / 2.0)),
        common::coil_note(na, nt, (mid, 0.0)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compression::scene_model::compression_scene;
    use crate::diagram::{project_silhouette, DimKind, DimLayer};
    use approx::assert_relative_eq;
    use springcore::units::{Force, Length};
    use springcore::{EndFixity, EndType, MaterialSet, PowerUser, Scenario, SpringDesign};

    /// Every user-selectable compression end type. Only SquaredGround has
    /// rendered coil height == free_length, so the drift sweep must exercise
    /// all four — a SquaredGround-only fixture passes the anchor check
    /// vacuously (assembly PR lesson).
    const ALL_END_TYPES: [EndType; 4] = [
        EndType::SquaredGround,
        EndType::Squared,
        EndType::Plain,
        EndType::PlainGround,
    ];

    fn build(end_type: EndType) -> SpringDesign {
        let m = MaterialSet::load_default()
            .get("Music Wire")
            .unwrap()
            .clone();
        PowerUser {
            end_type,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            active: 10.0,
            free_length: Length::from_millimeters(60.0),
            loads: vec![Force::from_newtons(10.0), Force::from_newtons(30.0)],
        }
        .solve(&m, springcore::CurvatureCorrection::Bergstrasser)
        .unwrap()
    }

    fn design() -> SpringDesign {
        build(EndType::SquaredGround)
    }

    /// The axial center of the coil body AS DRAWN by `compression_scene` — the
    /// helix's INDEPENDENTLY sampled y-center (via the shared
    /// `test_support::polyline_y_center`), not a re-derived `coil_render_height`,
    /// so the mirror-drift test pins presenter-tracks-geometry rather than
    /// presenter-calls-the-same-helper.
    fn drawn_body_center(d: &SpringDesign) -> f64 {
        crate::diagram::test_support::polyline_y_center(&compression_scene(d).polylines[0])
    }

    fn find(dims: &[Dimension], label_starts: &str) -> Dimension {
        dims.iter()
            .find(|d| d.label.starts_with(label_starts))
            .unwrap_or_else(|| {
                panic!(
                    "no dimension labeled {label_starts}: {:?}",
                    dims.iter().map(|d| &d.label).collect::<Vec<_>>()
                )
            })
            .clone()
    }

    #[test]
    fn free_length_dimension_matches_the_projected_body_span() {
        let d = design();
        let dims = dimensions(&d);
        let fl = find(&dims, "L\u{2080}"); // "L₀"
        assert_eq!(fl.layer, DimLayer::Lengths);
        // Presenter ↔ design: EXACT (the callout number is the design field).
        assert_relative_eq!(fl.value, d.free_length.millimeters(), max_relative = 1e-9);
        // Presenter anchor span == the labeled value: EXACT (the drawn dimension
        // line length equals its own number).
        let DimKind::Linear { from, to } = fl.kind else {
            panic!("free length must be a Linear dim")
        };
        assert_relative_eq!(
            (to.0 - from.0).abs(),
            d.free_length.millimeters(),
            max_relative = 1e-9
        );
        // Mirror-drift vs geometry: the projected silhouette's axial span matches
        // free_length to within a wire diameter (the drop-z offset perturbs the
        // ends by ≤ wire/2 each — the envelope peak is sampling-approximate, see
        // Task 1's PROJECTION MODEL note).
        let b = project_silhouette(&compression_scene(&d)).unwrap().bounds;
        let axial = b.axial_max - b.axial_min;
        assert!((axial - d.free_length.millimeters()).abs() <= d.wire_dia.millimeters());
    }

    #[test]
    fn outer_diameter_dimension_matches_the_design_field() {
        let d = design();
        let dims = dimensions(&d);
        let od = find(&dims, "OD");
        assert_eq!(od.layer, DimLayer::Diameters);
        assert_relative_eq!(od.value, d.outer_dia.millimeters(), max_relative = 1e-9);
        if let DimKind::Diameter { half, .. } = od.kind {
            assert_relative_eq!(2.0 * half, d.outer_dia.millimeters(), max_relative = 1e-9);
        } else {
            panic!("OD must be a Diameter dim");
        }
    }

    #[test]
    fn inner_diameter_dimension_matches_id() {
        let d = design();
        let id = find(&dimensions(&d), "ID");
        // Mirror-drift pin: the callout number IS the design field (EXACT).
        assert_relative_eq!(id.value, d.inner_dia.millimeters(), max_relative = 1e-9);
        if let DimKind::Diameter { half, .. } = id.kind {
            assert_relative_eq!(2.0 * half, d.inner_dia.millimeters(), max_relative = 1e-9);
        } else {
            panic!("ID must be a Diameter dim");
        }
    }

    #[test]
    fn wire_and_coil_and_solid_callouts_are_present_and_layered() {
        let d = design();
        let dims = dimensions(&d);
        assert_eq!(find(&dims, "\u{2300}").layer, DimLayer::Diameters); // wire ⌀
                                                                        // Coil-count note carries total & active, in the Coils layer.
        let coils = find(&dims, "N");
        assert_eq!(coils.layer, DimLayer::Coils);
        assert_eq!(coils.kind, DimKind::Note);
        // Solid length is a Lengths reference dim.
        assert_relative_eq!(
            find(&dims, "L\u{209B}").value,
            d.solid_length.millimeters(),
            max_relative = 1e-9
        );
    }

    #[test]
    fn degenerate_design_yields_finite_labels_only() {
        let mut d = design();
        // Mutate a field the presenter actually reads so the em-dash fallback in
        // `mm()` is genuinely exercised (defense in depth): free_length flows into
        // the L₀ callout's value and label.
        d.free_length = Length::from_millimeters(f64::NAN);
        // A NaN field must not crash the presenter; labels stay finite-guarded.
        let dims = dimensions(&d);
        assert!(dims
            .iter()
            .all(|dm| dm.value.is_finite() || dm.label.contains('\u{2014}')));
        // The L₀ callout renders the em dash "—", never "NaN".
        let fl = find(&dims, "L\u{2080}"); // "L₀"
        assert!(!fl.value.is_finite());
        assert!(fl.label.contains('\u{2014}'));
    }

    /// The diameter/wire/coil callouts anchor on the axial center of the coil
    /// body AS DRAWN by `compression_scene`, across EVERY end type — not on
    /// free_length/2, which drifts from the rendered body for all ends but
    /// SquaredGround (~(pitch−wire)/2 for PlainGround). Every callout's axial
    /// anchor (`at.0`) must equal the scene helix's own y-center to fp.
    #[test]
    fn diameter_callouts_track_the_drawn_coil_body_across_end_types() {
        for end_type in ALL_END_TYPES {
            let d = build(end_type);
            let dims = dimensions(&d);
            let center = drawn_body_center(&d);
            for label in ["OD", "ID", "\u{2300}", "N"] {
                assert_relative_eq!(find(&dims, label).at.0, center, max_relative = 1e-9);
            }
        }
    }

    /// The callouts moved off `free_length` onto the rendered coil height, which
    /// reads active/total/pitch/wire. A NaN in ANY of those must keep every
    /// anchor finite — `coil_render_height` guards active/total/pitch via
    /// `coil_body_hostile` and NaN/inf `wire` (plus finite overflow) via its
    /// result finiteness check, so `mid` stays finite. Mirrors the assembly
    /// degenerate discipline (NaN a field the anchor actually reads), swept over
    /// every such field so the doc's "wire" claim is exercised, not just asserted.
    #[test]
    fn degenerate_coil_geometry_yields_finite_callout_anchors() {
        let check = |field: &str, mutate: fn(&mut SpringDesign)| {
            let mut d = design();
            mutate(&mut d);
            let dims = dimensions(&d);
            for label in ["OD", "ID", "\u{2300}", "N"] {
                let at = find(&dims, label).at;
                assert!(
                    at.0.is_finite() && at.1.is_finite(),
                    "NaN {field}: {label} anchor non-finite"
                );
            }
        };
        check("active", |d| d.active_coils = f64::NAN);
        check("total", |d| d.total_coils = f64::NAN);
        check("pitch", |d| d.pitch = Length::from_millimeters(f64::NAN));
        check("wire", |d| d.wire_dia = Length::from_millimeters(f64::NAN));
    }
}
