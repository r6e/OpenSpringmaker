//! Pure 2D-diagram dimension presenter for the compression family (ADR 0008).
//! Anchors are in projection space `(axial, radial)` model mm; axial spans
//! `[0, free_length]` and the radial envelope is ±OD/2 (see scene_model). Each
//! feature dimension is anchored to that geometry and labeled from the design
//! field — the mirror-drift equality is asserted in tests.

use crate::diagram::{DimKind, DimLayer, Dimension};
use springcore::SpringDesign;

/// Format a millimetre value, or an em dash for a non-finite field so no NaN/inf
/// ever reaches a label (defense in depth; the engine rejects these upstream).
#[allow(dead_code)] // consumed by the layout engine in Task 3
fn mm(v: f64) -> String {
    if v.is_finite() {
        format!("{v:.1}")
    } else {
        "\u{2014}".into() // em dash
    }
}

#[allow(dead_code)] // consumed by the layout engine in Task 3
pub fn dimensions(design: &SpringDesign) -> Vec<Dimension> {
    let l0 = design.free_length.millimeters();
    let ls = design.solid_length.millimeters();
    let od = design.outer_dia.millimeters();
    let id = design.inner_dia.millimeters();
    let wire = design.wire_dia.millimeters();
    let na = design.active_coils;
    let nt = design.total_coils;
    let mid = l0 / 2.0; // an axial station for the diameter callouts

    vec![
        Dimension {
            kind: DimKind::Linear {
                from: (0.0, 0.0),
                to: (l0, 0.0),
            },
            layer: DimLayer::Lengths,
            value: l0,
            label: format!("L\u{2080} {}", mm(l0)), // L₀
            at: (mid, 0.0),
        },
        Dimension {
            kind: DimKind::Linear {
                from: (0.0, 0.0),
                to: (ls, 0.0),
            },
            layer: DimLayer::Lengths,
            value: ls,
            label: format!("L\u{209B} {}", mm(ls)), // Lₛ (reference)
            at: (ls / 2.0, 0.0),
        },
        Dimension {
            kind: DimKind::Diameter {
                at_axial: mid,
                half: od / 2.0,
            },
            layer: DimLayer::Diameters,
            value: od,
            label: format!("OD {}", mm(od)),
            at: (mid, od / 2.0),
        },
        Dimension {
            kind: DimKind::Diameter {
                at_axial: mid,
                half: id / 2.0,
            },
            layer: DimLayer::Diameters,
            value: id,
            label: format!("ID {}", mm(id)),
            at: (mid, id / 2.0),
        },
        Dimension {
            kind: DimKind::Note,
            layer: DimLayer::Diameters,
            value: wire,
            label: format!("\u{2300}{} wire", mm(wire)), // ⌀2.0 wire
            at: (mid, od / 2.0),
        },
        Dimension {
            kind: DimKind::Note,
            layer: DimLayer::Coils,
            value: na,
            label: format!("N {} active / {} total", mm(na), mm(nt)),
            at: (mid, 0.0),
        },
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

    fn design() -> SpringDesign {
        let m = MaterialSet::load_default()
            .get("Music Wire")
            .unwrap()
            .clone();
        PowerUser {
            end_type: EndType::SquaredGround,
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
}
