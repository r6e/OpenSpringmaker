//! Pure 2D-diagram presenter for assemblies: per-member OD/wire dims anchored to
//! each member body, plus overall free/solid reference dims and a stage summary.
//! Series drawn span includes schematic gaps, so overall free length is a
//! reference dim (value from design.free_length), not a full-span anchor.
use crate::diagram::{common, DimKind, DimLayer, Dimension};
use crate::viz::coil_render_height;
use springcore::assembly::{AssemblyDesign, Topology};
use springcore::SpringDesign;

/// The axial height a member's coil body renders to — what `assembly_scene`
/// stacks and centers members by. Anchor callouts on THIS, not `free_length`:
/// the two diverge for every end type except `SquaredGround` (by `wire` for
/// Plain/Squared ends, `pitch − wire` for PlainGround), so anchoring on free
/// length walks the callout off the drawn body — and, accumulated over Series
/// members, onto a neighbour.
fn member_rendered_height(d: &SpringDesign) -> f64 {
    coil_render_height(
        d.active_coils,
        d.total_coils,
        d.pitch.millimeters(),
        d.wire_dia.millimeters(),
    )
}

pub fn dimensions(design: &AssemblyDesign) -> Vec<Dimension> {
    let l0 = design.free_length.millimeters();
    let ls = design.solid_length.millimeters();
    let mut dims = vec![
        // Overall free length (reference; series includes schematic gaps).
        common::free_length(l0),
        common::axial_length(ls, format!("L\u{209B} {}", common::mm(ls))),
    ];
    // Per-member OD/wire notes anchored to each drawn body. Series stacks
    // members with the same `2 × max wire` gap the scene draws, and centers each
    // on its rendered height, so callout stations sit on the drawn bodies.
    let gap = super::scene_model::series_stack_gap(design);
    let mut axial = 0.0;
    for (i, m) in design.members.iter().enumerate() {
        let od = m.design.outer_dia.millimeters();
        let wire = m.design.wire_dia.millimeters();
        let member_h = member_rendered_height(&m.design);
        let station = match design.topology {
            Topology::Nested => member_h / 2.0,
            Topology::Series => axial + member_h / 2.0,
        };
        dims.push(common::diameter(
            station,
            od,
            format!("m{} OD {}", i + 1, common::mm(od)),
        ));
        dims.push(common::wire_note(wire, (station, od / 2.0)));
        if design.topology == Topology::Series {
            axial += member_h + gap;
        }
    }
    // Stage summary. This is a caption (stage count + topology), NOT a feature
    // callout on a drawn body, so it anchors on `free_length/2` like the L₀
    // reference dim — the "anchor on the drawn body" rule does not apply. For a
    // Series stack with gaps the drawn span exceeds `l0`, so this sits slightly
    // below the drawn center by design; do not "fix" it onto rendered height.
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
    use crate::diagram::test_support::find;
    use crate::diagram::DimLayer;
    use approx::assert_relative_eq;
    use springcore::{CurvatureCorrection, MaterialSet, MaterialStore, UnitSystem};

    /// Member geometry as `(wire_dia, mean_dia, active, free_length)` mm strings.
    type MemberSpec = (&'static str, &'static str, &'static str, &'static str);
    const TWO_MEMBERS: &[MemberSpec] = &[("2", "20", "10", "60"), ("1.5", "16", "8", "60")];
    /// Every user-selectable member end type — the alignment tests sweep all of
    /// them, since only `squared_ground` has rendered height == free length.
    const ALL_END_TYPES: [&str; 4] = ["squared_ground", "squared", "plain", "plain_ground"];

    /// Solve an assembly with the given topology, a shared per-member `end_type`,
    /// and explicit member specs — so a test can exercise every user-selectable
    /// end type, not just the `blank` default (`squared_ground`).
    fn build(topology: &str, end_type: &str, members: &[MemberSpec]) -> AssemblyDesign {
        let mut f = AsmFormState::with_default_material("Music Wire");
        f.topology = topology.into();
        f.loads = "10, 25".into();
        f.members.clear();
        for &(wire_dia, mean_dia, active, free_length) in members {
            f.members.push(AsmMemberForm {
                wire_dia: wire_dia.into(),
                mean_dia: mean_dia.into(),
                active: active.into(),
                free_length: free_length.into(),
                end_type: end_type.into(),
                ..AsmMemberForm::blank("Music Wire")
            });
        }
        parse_and_solve(
            &f,
            UnitSystem::Metric,
            &MaterialStore::new(MaterialSet::load_default()),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap()
    }

    fn two_member(topology: &str) -> AssemblyDesign {
        build(topology, "squared_ground", TWO_MEMBERS)
    }

    /// Assert every member's OD callout sits at the axial center of that member
    /// as ACTUALLY drawn by `assembly_scene` — the annotation-tracks-geometry
    /// invariant, checked against the real scene rather than a re-derived
    /// formula. The tolerance is tight because presenter and scene both evaluate
    /// the same `coil_height_fn(..)(1.0)` height (the presenter via
    /// `coil_render_height`, the scene via its helix's last point), so they agree
    /// to fp; a stale `free_length` anchor (the pre-fix bug) drifts by `wire`
    /// (Plain/Squared) or `pitch − wire` (PlainGround) per member and blows past
    /// the tolerance for every non-`SquaredGround` end.
    fn assert_callouts_track_drawn_bodies(d: &AssemblyDesign) {
        let dims = dimensions(d);
        let scene = crate::assembly::scene_model::assembly_scene(d);
        assert_eq!(
            scene.polylines.len(),
            d.members.len(),
            "one drawn body per member"
        );
        for (i, poly) in scene.polylines.iter().enumerate() {
            let drawn_center = crate::diagram::test_support::polyline_y_center(poly);
            let label = format!("m{} OD", i + 1);
            let at_axial = match find(&dims, &label).kind {
                DimKind::Diameter { at_axial, .. } => at_axial,
                other => panic!("{label} should be a diameter dim, got {other:?}"),
            };
            assert_relative_eq!(at_axial, drawn_center, max_relative = 1e-9);
        }
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

    /// Every user-selectable end type — not just the fixture's default
    /// `squared_ground`, the one type where rendered height happens to equal free
    /// length. This is the guard against anchoring callouts on `free_length`:
    /// under the old code `plain_ground` drifts ~3.5 mm on member 2, past any
    /// alignment tolerance. Series exercises gap + accumulation (member 2's
    /// station rides on member 1's height plus the gap).
    #[test]
    fn series_member_stations_align_with_the_drawn_scene() {
        let gap = crate::assembly::scene_model::series_stack_gap(&two_member("series"));
        assert!(gap > 0.0, "fixture has nonzero wire → nonzero stacking gap");
        for end_type in ALL_END_TYPES {
            assert_callouts_track_drawn_bodies(&build("series", end_type, TWO_MEMBERS));
        }
    }

    /// N > 2 accumulation: with three stacked `plain_ground` members (the worst
    /// drift end), the third member's station rides on two prior heights + two
    /// gaps; it must still land on the drawn body.
    #[test]
    fn series_three_member_stations_accumulate_onto_the_drawn_bodies() {
        let three: &[MemberSpec] = &[
            ("2", "20", "10", "60"),
            ("1.5", "16", "8", "50"),
            ("2.5", "24", "6", "45"),
        ];
        assert_callouts_track_drawn_bodies(&build("series", "plain_ground", three));
    }

    /// Nested twin of the alignment invariant: members are concentric (all share
    /// `y_base = 0`, no gap), but each is drawn to its OWN rendered height, so the
    /// callout must center on that height — not the shared `free_length / 2`, and
    /// not accumulate a gap. Checked against the drawn scene across all end types.
    #[test]
    fn nested_member_stations_align_with_the_drawn_scene() {
        for end_type in ALL_END_TYPES {
            assert_callouts_track_drawn_bodies(&build("nested", end_type, TWO_MEMBERS));
        }
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

    /// The exact path Copilot flagged: `assembly_scene` ignores `free_length`,
    /// so a design with a non-finite `free_length` still projects a renderable
    /// scene, yet `dimensions` anchors the overall L₀ callout at
    /// `(free_length/2, 0)` — a non-finite coordinate. `layout` must drop such
    /// a dim so no NaN reaches the canvas `Path`. Assert on the OUTPUT: every
    /// coordinate of every laid-out dim is finite (not merely that the input
    /// was filtered).
    #[test]
    fn degenerate_free_length_never_reaches_layout_with_non_finite_geometry() {
        use crate::diagram::geometry::finite2;
        use crate::diagram::test_support::layouted_dim_is_finite;
        use crate::diagram::{layout, Bounds, DimLayers};

        let mut d = two_member("nested");
        d.free_length = springcore::units::Length::from_millimeters(f64::NAN);
        let dims = dimensions(&d);
        // Precondition: the presenter really does emit a non-finite anchor for
        // the degenerate field (otherwise this test would pass vacuously).
        assert!(
            dims.iter().any(|dm| !finite2(dm.at)),
            "degenerate free_length should produce a non-finite dim anchor"
        );

        let bounds = Bounds {
            axial_min: 0.0,
            axial_max: 60.0,
            radial_min: -11.0,
            radial_max: 11.0,
        };
        let out = layout(&dims, &bounds, DimLayers::default());
        assert!(
            out.iter().all(layouted_dim_is_finite),
            "no laid-out dim may carry a non-finite coordinate to the canvas"
        );
    }

    /// Parity with compression/conical (input-domain panel finding): each
    /// member's OD/wire station routes through `member_rendered_height` →
    /// `coil_render_height`, and Series ACCUMULATES member heights + gaps
    /// (`axial += member_h + gap`). A NaN in one member's coil geometry must keep
    /// EVERY member's station finite — `coil_render_height` guards → 0.0 and
    /// `series_stack_gap`'s max-fold ignores the NaN wire — so a poisoned member
    /// can't drop the whole downstream stack's callouts. Swept over both
    /// topologies (Series exercises the accumulator).
    #[test]
    fn degenerate_member_coil_geometry_yields_finite_station_anchors() {
        use springcore::units::Length;
        let check = |field: &str, topology: &str, mutate: fn(&mut SpringDesign)| {
            let mut d = build(topology, "squared_ground", TWO_MEMBERS);
            mutate(&mut d.members[0].design);
            let dims = dimensions(&d);
            for i in 0..d.members.len() {
                let at = find(&dims, &format!("m{} OD", i + 1)).at;
                assert!(
                    at.0.is_finite() && at.1.is_finite(),
                    "NaN {field} on member 0 ({topology}): m{} OD anchor non-finite",
                    i + 1
                );
            }
        };
        // All FOUR independent coil-geometry fields member_rendered_height reads
        // (member is a full SpringDesign, unlike extension's na==total derived
        // path) — sweep ALL producers of the coil_render_height sink, matching
        // compression/conical: total/pitch hit coil_body_hostile clauses that
        // active/wire don't (total-range and pitch-finiteness respectively).
        for topology in ["series", "nested"] {
            check("active", topology, |m| m.active_coils = f64::NAN);
            check("total", topology, |m| m.total_coils = f64::NAN);
            check("pitch", topology, |m| {
                m.pitch = Length::from_millimeters(f64::NAN)
            });
            check("wire", topology, |m| {
                m.wire_dia = Length::from_millimeters(f64::NAN)
            });
        }
    }
}
