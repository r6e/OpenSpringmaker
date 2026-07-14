//! Pure 2D-diagram presenter for torsion: body dims in the side elevation, plus
//! an end-on inset (project x,z) for the legs and their included angle — a side
//! elevation cannot show cross-section-plane legs.
use crate::diagram::{common, DimKind, DimLayer, Dimension, Edge2, Inset};
use springcore::torsion::TorsionDesign;
use std::f64::consts::TAU;

pub fn diagram(design: &TorsionDesign) -> (Vec<Dimension>, Inset) {
    let wire = design.inputs.wire_dia.millimeters();
    let r = design.inputs.mean_dia.millimeters() / 2.0;
    let nb = design.inputs.body_coils;
    let body_h = nb * wire;
    let od = design.inputs.mean_dia.millimeters() + wire;
    let id = design.inputs.mean_dia.millimeters() - wire;
    let l1 = design.inputs.leg1.millimeters();
    let l2 = design.inputs.leg2.millimeters();

    let side = vec![
        Dimension {
            kind: DimKind::Linear {
                from: (0.0, 0.0),
                to: (body_h, 0.0),
            },
            layer: DimLayer::Lengths,
            value: body_h,
            label: format!("body {}", common::mm(body_h)),
            at: (body_h / 2.0, 0.0),
        },
        Dimension {
            kind: DimKind::Diameter {
                at_axial: body_h / 2.0,
                half: od / 2.0,
            },
            layer: DimLayer::Diameters,
            value: od,
            label: format!("OD {}", common::mm(od)),
            at: (body_h / 2.0, od / 2.0),
        },
        Dimension {
            kind: DimKind::Diameter {
                at_axial: body_h / 2.0,
                half: id / 2.0,
            },
            layer: DimLayer::Diameters,
            value: id,
            label: format!("ID {}", common::mm(id)),
            at: (body_h / 2.0, id / 2.0),
        },
        common::wire_note(wire, (body_h / 2.0, od / 2.0)),
        common::coil_note(nb, nb, (body_h / 2.0, 0.0)),
    ];

    // End-on inset: coil circle + two legs at azimuths 0 and nb*TAU (x,z plane).
    let end_angle = nb * TAU;
    let leg_dir = |az: f64| (az.cos(), az.sin());
    let (d1x, d1y) = leg_dir(0.0);
    let (d2x, d2y) = leg_dir(end_angle);
    // Legs start at the coil radius, extend outward by their length (true length).
    let leg1_edge = Edge2 {
        points: vec![(r, 0.0), ((r + l1) * d1x, (r + l1) * d1y)],
        role: crate::viz::SceneRole::Detail,
    };
    let leg2_edge = Edge2 {
        points: vec![(r * d2x, r * d2y), ((r + l2) * d2x, (r + l2) * d2y)],
        role: crate::viz::SceneRole::Detail,
    };
    // Included leg angle from the drawn leg directions (fractional turn → degrees).
    let included = end_angle.to_degrees().rem_euclid(360.0);
    let inset_dims = vec![
        Dimension {
            kind: DimKind::Linear {
                from: (r, 0.0),
                to: ((r + l1) * d1x, (r + l1) * d1y),
            },
            layer: DimLayer::Lengths,
            value: l1,
            label: format!("L\u{2081} {}", common::mm(l1)),
            at: (r + l1 / 2.0, 0.0),
        },
        Dimension {
            kind: DimKind::Linear {
                from: (r * d2x, r * d2y),
                to: ((r + l2) * d2x, (r + l2) * d2y),
            },
            layer: DimLayer::Lengths,
            value: l2,
            label: format!("L\u{2082} {}", common::mm(l2)),
            at: ((r + l2 / 2.0) * d2x, (r + l2 / 2.0) * d2y),
        },
        Dimension {
            kind: DimKind::Angular {
                vertex: (0.0, 0.0),
                start_deg: 0.0,
                sweep_deg: included,
                radius: r + 4.0,
            },
            layer: DimLayer::Coils,
            value: included,
            label: format!("{included:.0}\u{00b0}"),
            at: (0.0, 0.0),
        },
    ];
    (
        side,
        Inset {
            edges: vec![leg1_edge, leg2_edge],
            dims: inset_dims,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::torsion::form::{parse_and_solve, TorFormState};
    use approx::assert_relative_eq;
    use springcore::{MaterialSet, MaterialStore, UnitSystem};

    fn design() -> springcore::torsion::TorsionDesign {
        let materials = MaterialStore::new(MaterialSet::load_default());
        let form = TorFormState {
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            body_coils: "5.25".into(),
            leg1: "15".into(),
            leg2: "10".into(),
            moments: "500, 1000".into(),
            ..Default::default()
        };
        parse_and_solve(&form, "Music Wire", UnitSystem::Metric, &materials)
            .unwrap()
            .design
    }

    #[test]
    fn body_dims_present_in_side_elevation() {
        let d = design();
        let (dims, _inset) = diagram(&d);
        let body = dims.iter().find(|x| x.label.contains("body")).unwrap();
        // `millimeters_body()` in the plan is shorthand; the real expression is
        // body_coils * wire_dia (there is no such method on `TorsionInputs`).
        assert_relative_eq!(
            body.value,
            d.inputs.body_coils * d.inputs.wire_dia.millimeters(),
            max_relative = 1e-9
        );
    }

    #[test]
    fn inset_carries_leg_lengths_and_the_angular_leg_angle() {
        let d = design(); // body_coils 5.25 → legs 0.25 turn = 90° apart
        let (_dims, inset) = diagram(&d);
        let leg_angle = inset
            .dims
            .iter()
            .find(|x| matches!(x.kind, DimKind::Angular { .. }))
            .unwrap();
        assert_relative_eq!(leg_angle.value, 90.0, max_relative = 1e-6);
        // Both leg lengths are represented (true length in the end-view plane).
        assert!(inset.dims.iter().any(|x| (x.value - 15.0).abs() < 1e-6));
        assert!(inset.dims.iter().any(|x| (x.value - 10.0).abs() < 1e-6));
        assert!(!inset.edges.is_empty());
    }

    /// Mirrors compression's `degenerate_design_yields_finite_labels_only`.
    /// Unlike the other families, torsion's inset Angular label is built
    /// here (not in a `dimensions()` fn), and its side dims are the FIRST
    /// tuple element returned by `diagram()`. `body_coils` NaN would empty
    /// the 3D scene, but `diagram()` itself is pure geometry/labels and has
    /// no such short-circuit — a non-finite `mean_dia` poisons OD/ID (both
    /// derived from `mean_dia ± wire`) while `body` (from `body_coils *
    /// wire`) stays finite, so this targets the side-elevation OD/ID
    /// callouts specifically.
    #[test]
    fn degenerate_design_yields_finite_side_labels_only() {
        let mut d = design();
        d.inputs.mean_dia = springcore::units::Length::from_millimeters(f64::NAN);
        let (side, _inset) = diagram(&d);
        assert!(side
            .iter()
            .all(|dm| dm.value.is_finite() || dm.label.contains('\u{2014}')));
        let od = side.iter().find(|x| x.label.contains("OD")).unwrap();
        assert!(!od.value.is_finite());
        assert!(od.label.contains('\u{2014}'));
    }
}
