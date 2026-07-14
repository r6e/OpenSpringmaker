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
        common::axial_length(body_h, format!("body {}", common::mm(body_h))),
        common::diameter(body_h / 2.0, od, format!("OD {}", common::mm(od))),
        common::diameter(body_h / 2.0, id, format!("ID {}", common::mm(id))),
        common::wire_note(wire, (body_h / 2.0, od / 2.0)),
        common::coil_note(nb, nb, (body_h / 2.0, 0.0)),
    ];

    // End-on inset: coil circle + two legs at azimuths 0 and nb*TAU (x,z plane).
    let end_angle = nb * TAU;
    let leg_dir = |az: f64| (az.cos(), az.sin());
    let (d1x, d1y) = leg_dir(0.0);
    let (d2x, d2y) = leg_dir(end_angle);
    // Legs start at the coil radius, extend outward by their length (true length).
    let leg1_start = (r, 0.0);
    let leg1_end = ((r + l1) * d1x, (r + l1) * d1y);
    let leg2_start = (r * d2x, r * d2y);
    let leg2_end = ((r + l2) * d2x, (r + l2) * d2y);
    let leg1_edge = Edge2 {
        points: vec![leg1_start, leg1_end],
        role: crate::viz::SceneRole::Detail,
    };
    let leg2_edge = Edge2 {
        points: vec![leg2_start, leg2_end],
        role: crate::viz::SceneRole::Detail,
    };
    // Included leg angle from the drawn leg directions (fractional turn → degrees).
    let included = end_angle.to_degrees().rem_euclid(360.0);
    // Leg lengths are `Note`s, not `Linear`: legs are off-axis and `Linear` is
    // axial-only (see `DimKind::Linear`). The leg edge is already stroked here,
    // so a `Note` at the leg midpoint honestly labels the drawn length.
    let inset_dims = vec![
        Dimension {
            kind: DimKind::Note,
            layer: DimLayer::Lengths,
            value: l1,
            label: format!("L\u{2081} {}", common::mm(l1)),
            at: (r + l1 / 2.0, 0.0),
        },
        Dimension {
            kind: DimKind::Note,
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

    /// Locks the fix: both leg callouts must be `Note`, not the axial-only
    /// `Linear` (see `DimKind::Linear`) — else the ladder layout foreshortens a
    /// non-axial leg. Reverting either leg to `Linear` fails the `assert_eq!`.
    /// Also locks each note's anchor to the midpoint of its DRAWN leg edge (the
    /// annotate-rendered-geometry discipline), so the label can't drift off the
    /// leg it labels.
    #[test]
    fn leg_length_callouts_are_notes_so_no_line_can_foreshorten_the_leg() {
        let (_side, inset) = diagram(&design());
        // edges[0]/[1] are the drawn leg1/leg2 edges; L₁/L₂ pair with them in order.
        for (label, edge) in ["L\u{2081}", "L\u{2082}"].iter().zip(&inset.edges) {
            let dm = inset.dims.iter().find(|x| x.label.contains(label)).unwrap();
            assert_eq!(
                dm.kind,
                DimKind::Note,
                "{label} leg length must be a text Note (no foreshortening line)"
            );
            // The note text sits on the drawn leg: at == midpoint of the edge.
            let (p0, p1) = (edge.points[0], edge.points[1]);
            let mid = ((p0.0 + p1.0) / 2.0, (p0.1 + p1.1) / 2.0);
            assert_relative_eq!(dm.at.0, mid.0, max_relative = 1e-9);
            assert_relative_eq!(dm.at.1, mid.1, max_relative = 1e-9);
        }
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

    /// Lock (not fix): the side-elevation body callouts (OD/ID/wire/coil) sit at
    /// the axial center of the body AS DRAWN — and the main side elevation
    /// projects the BODY-ONLY scene (`torsion_body_scene`, the R1 leg-leak fix),
    /// not the body+legs `torsion_scene`. Torsion does NOT drift like
    /// compression/conical: the body is close-wound, so the presenter's
    /// `body_h = nb·wire` equals the rendered height `coil_height_fn(nb,nb,
    /// wire,wire)(1.0)` exactly. Compared against the body scene's own
    /// independently sampled y-center, so a re-derivation drift would fail it.
    #[test]
    fn side_body_callouts_track_the_drawn_body_only_scene() {
        let d = design();
        let (side, _inset) = diagram(&d);
        let body = &crate::torsion::scene_model::torsion_body_scene(&d).polylines[0];
        let center = crate::diagram::test_support::polyline_y_center(body);
        assert!(center > 0.0, "fixture must have a nonzero body height");
        // "OD"/"ID"/"wire"/"active" are each unique among the side dims.
        for label in ["OD", "ID", "wire", "active"] {
            let dm = side.iter().find(|x| x.label.contains(label)).unwrap();
            assert_relative_eq!(dm.at.0, center, max_relative = 1e-9);
        }
    }
}
