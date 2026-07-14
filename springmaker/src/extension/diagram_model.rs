//! Pure 2D-diagram dimension presenter for the extension family (ADR 0008).
//! Anchors are in projection space `(axial, radial)` model mm. The free
//! length is anchored to the **inside-hooks span** (matches
//! `scene_model::extension_scene`'s hook geometry and the engine's
//! `free_length_from_geometry` relation), not `[0, l0]` like compression.

use crate::diagram::{common, DimKind, DimLayer, Dimension};
use springcore::extension::ExtensionDesign;

pub fn dimensions(design: &ExtensionDesign) -> Vec<Dimension> {
    let wire = design.wire_dia.millimeters();
    let r1 = design.hooks.r1.millimeters();
    let l0 = design.free_length.millimeters();
    let od = design.outer_dia.millimeters();
    let id = design.inner_dia.millimeters();
    let na = design.active_coils;
    // Body height from the engine's inside-hooks relation (matches scene_model).
    // body_h = body_coils·wire ≥ 0 for every solvable design (engine rejects
    // sub-close-wound free lengths), so no clamp needed.
    let body_h = l0 - 2.0 * (2.0 * r1 - wire) - wire;
    let bottom_inner = -2.0 * r1 + wire / 2.0;
    let top_inner = body_h + 2.0 * r1 - wire / 2.0;
    let fi = design.initial_tension.newtons();

    vec![
        // Free length = inside-hooks span (feature-anchored).
        Dimension {
            kind: DimKind::Linear {
                from: (bottom_inner, 0.0),
                to: (top_inner, 0.0),
            },
            layer: DimLayer::Lengths,
            value: l0,
            label: format!("L\u{2080} {}", common::mm(l0)),
            at: ((bottom_inner + top_inner) / 2.0, 0.0),
        },
        // Body length.
        common::axial_length(body_h, format!("body {}", common::mm(body_h))),
        // Hook opening = loop inside diameter. NOT a `common::diameter` fold:
        // its anchor is `(bottom_inner, r1)`, not `(bottom_inner, half)` —
        // `r1 != (2*r1 - wire)/2` for a nonzero wire.
        Dimension {
            kind: DimKind::Diameter {
                at_axial: bottom_inner,
                half: (2.0 * r1 - wire) / 2.0,
            },
            layer: DimLayer::Diameters,
            value: 2.0 * r1 - wire,
            label: format!("hook \u{2300}{}", common::mm(2.0 * r1 - wire)),
            at: (bottom_inner, r1),
        },
        common::diameter(body_h / 2.0, od, format!("OD {}", common::mm(od))),
        common::diameter(body_h / 2.0, id, format!("ID {}", common::mm(id))),
        common::wire_note(wire, (body_h / 2.0, od / 2.0)),
        common::coil_note(na, na, (body_h / 2.0, 0.0)), // extension body: active ≈ total
        Dimension {
            kind: DimKind::Note,
            layer: DimLayer::Coils,
            value: fi,
            label: format!("F\u{1d62} {}N", common::mm(fi)),
            at: (body_h / 2.0, 0.0),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram::project_silhouette;
    use crate::diagram::test_support::find;
    use crate::extension::form::{parse_and_solve, ExtFormState};
    use crate::extension::scene_model::extension_scene;
    use approx::assert_relative_eq;
    use springcore::{CurvatureCorrection, MaterialSet, MaterialStore, UnitSystem};

    fn design() -> springcore::extension::ExtensionDesign {
        let materials = MaterialStore::new(MaterialSet::load_default());
        let form = ExtFormState {
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            active: "10".into(),
            free_length: "100".into(),
            initial_tension: "5".into(),
            loads: "10, 30".into(),
            ..Default::default()
        };
        parse_and_solve(
            &form,
            "Music Wire",
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .unwrap()
        .design
    }

    #[test]
    fn free_length_spans_inside_hooks_matching_the_projection() {
        let d = design();
        let dims = dimensions(&d);
        let fl = find(&dims, "L\u{2080}");
        // (1) EXACT: presenter value == design free_length.
        assert_relative_eq!(fl.value, d.free_length.millimeters(), max_relative = 1e-9);
        // (2) EXACT (purely algebraic): the presenter's Linear span == free_length.
        //     |top_inner - bottom_inner| = body_h + 4·r1 - wire, and
        //     body_h = free_length - 4·r1 + wire, so the span is free_length exactly.
        if let crate::diagram::DimKind::Linear { from, to } = fl.kind {
            assert_relative_eq!(
                (to.0 - from.0).abs(),
                d.free_length.millimeters(),
                max_relative = 1e-9
            );
        } else {
            panic!("free length must be Linear");
        }
        // (3) DROP-Z ENVELOPE (sampling-approximate, NOT 1e-9): the projected
        //     outer axial span ≈ free_length + 2·wire (hook outer surfaces). The
        //     ±wire/2 perpendicular offset + arc sampling perturb each hook tip by
        //     ≤ wire/2, so use a wire-scale tolerance — mirror the compression idiom
        //     in diagram/geometry.rs::axial_span_matches_free_length (see the
        //     PROJECTION MODEL footer: the envelope only approaches the ideal to
        //     sampling resolution; exact ties are presenter-value/algebraic, above).
        let p = project_silhouette(&extension_scene(&d)).unwrap();
        let span = p.bounds.axial_max - p.bounds.axial_min;
        let ideal = d.free_length.millimeters() + 2.0 * d.wire_dia.millimeters();
        assert!(
            (span - ideal).abs() <= d.wire_dia.millimeters(),
            "projected outer axial span {span} not within a wire dia of free_length + 2·wire ({ideal})"
        );
    }

    #[test]
    fn hook_opening_and_initial_tension_present() {
        let d = design();
        let dims = dimensions(&d);
        let opening = find(&dims, "hook");
        assert_relative_eq!(
            opening.value,
            2.0 * d.hooks.r1.millimeters() - d.wire_dia.millimeters(),
            max_relative = 1e-9
        );
        let fi = find(&dims, "F\u{1d62}"); // Fᵢ initial tension
        assert_relative_eq!(fi.value, d.initial_tension.newtons(), max_relative = 1e-9);
        assert_eq!(fi.layer, crate::diagram::DimLayer::Coils);
    }

    /// Mirrors compression's `degenerate_design_yields_finite_labels_only`:
    /// a post-solve NaN on a field the presenter actually reads for a label
    /// (`free_length` flows into the L₀ callout) must not crash the
    /// presenter — labels stay finite-guarded (em dash, never "NaN").
    #[test]
    fn degenerate_design_yields_finite_labels_only() {
        let mut d = design();
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
