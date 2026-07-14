//! Pure 3D scene presenter for the assembly family: each member's solved
//! design rendered from its own geometry via the shared `scene_from_radius`
//! coil-body helper — concentric for Nested (roles alternate Wire/Member by
//! index for visual distinction; 3D has no composite line to fall back on),
//! stacked axially with a gap of `2 × max member wire dia` for Series.
//!
//! Decision (task report): each member's stroke stays sized against ITS OWN
//! body extent (`scene_from_radius`'s own scaling), not the merged assembly
//! scene's — the natural per-member helper output, and it differentiates a
//! thick outer member from a thin inner one honestly rather than flattening
//! both to one shared scale.

use crate::viz::{coil_body_is_empty, scene_from_radius, SceneData, SceneRole};
use springcore::assembly::{AssemblyDesign, Topology};

/// Axial gap inserted between stacked Series members: `2 × max member wire dia`.
/// The single source of truth for the stacking pitch — both the 3D scene
/// (`assembly_scene`) and the 2D-diagram presenter (`diagram_model`) advance
/// members by this exact gap, so callouts line up with the drawn bodies.
pub(crate) fn series_stack_gap(design: &AssemblyDesign) -> f64 {
    2.0 * design
        .members
        .iter()
        .map(|m| m.design.wire_dia.millimeters())
        .fold(0.0_f64, f64::max)
}

pub fn assembly_scene(design: &AssemblyDesign) -> SceneData {
    let gap = series_stack_gap(design);

    let mut polylines = Vec::with_capacity(design.members.len());
    let mut y_base = 0.0_f64;
    for (i, m) in design.members.iter().enumerate() {
        let d = &m.design;
        let r = d.mean_dia.millimeters() / 2.0;
        // Each member's body via the shared coil-body helper — one source of
        // truth for the helix/pitch/stroke construction every family shares.
        let member_scene = scene_from_radius(
            |_| r,
            r,
            d.active_coils,
            d.total_coils,
            d.pitch.millimeters(),
            d.wire_dia.millimeters(),
        );
        // Contrast with the NaN-cascade semantics pinned in the tests below:
        // NaN in COIL COUNTS (active_coils, total_coils) or PITCH routes
        // through `scene_from_radius`'s entry guard (empty body →
        // whole-scene bail via `coil_body_is_empty` — the pitch term is the
        // R2 sibling-parity gate matching `assembly_sdf`'s whole-scene
        // verdict for a hostile member). Only NaN in the remaining NON-COIL
        // fields (mean_dia → radius, wire_dia → height/stroke) produces the
        // contributing-points cascade: a NaN-poisoned member still
        // CONTRIBUTES points (non-finite, filtered per point by `finite3`)
        // because the solver output was already poisoned upstream, and a
        // partial render is the accepted trade-off. A CAPPED member is
        // different — its input is VALID and solvable, and the sampler
        // returned an EMPTY body; rendering the assembly minus one member
        // would silently misrepresent the design. Honest bail instead: any
        // empty member body degrades the WHOLE scene (empty SceneData →
        // extent None → placeholder).
        if coil_body_is_empty(&member_scene) {
            return SceneData {
                polylines: Vec::new(),
            };
        }
        let mut line = member_scene
            .polylines
            .into_iter()
            .next()
            .expect("scene_from_radius always returns exactly one polyline");
        // Roles alternate by index — a documented pragmatic choice: 3D has no
        // composite line to render, so alternating Wire/Member is the visual
        // stand-in for "these are distinct members" (Nested and Series alike).
        line.role = if i % 2 == 0 {
            SceneRole::Wire
        } else {
            SceneRole::Member
        };
        if design.topology == Topology::Series {
            // Stack axially: this member starts where the running offset
            // left off, then the offset advances by this member's own solved
            // height plus the shared gap for the next member.
            let top = line.points.last().map_or(0.0, |p| p.1);
            for p in &mut line.points {
                p.1 += y_base;
            }
            y_base += top + gap;
        }
        polylines.push(line);
    }
    SceneData { polylines }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::form::{parse_and_solve, AsmFormState, AsmMemberForm};
    use crate::viz::scene_extent;
    use approx::assert_relative_eq;
    use springcore::{CurvatureCorrection, MaterialSet, MaterialStore, UnitSystem};

    fn store() -> MaterialStore {
        MaterialStore::new(MaterialSet::load_default())
    }

    /// Two-member metric fixture (wire=2/1.5mm, mean=20/16mm, active=10/8
    /// coils, free=60mm each, loads=[10N, 25N]) — mirrors `two_member_form`
    /// in `assembly/plot_model.rs`'s tests, parameterized on topology.
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

    fn nested_two_member_design() -> AssemblyDesign {
        solve(&two_member_form("nested"))
    }

    fn series_two_member_design() -> AssemblyDesign {
        solve(&two_member_form("series"))
    }

    #[test]
    fn nested_members_are_concentric_from_member_geometry() {
        let d = nested_two_member_design();
        let s = assembly_scene(&d);
        assert_eq!(s.polylines.len(), d.members.len());
        for (line, member) in s.polylines.iter().zip(&d.members) {
            let p = line.points[0];
            assert_relative_eq!(
                (p.0.powi(2) + p.2.powi(2)).sqrt(),
                member.design.mean_dia.millimeters() / 2.0,
                max_relative = 1e-9
            );
            // All start at y = 0 (shared axis, shared base).
            assert_relative_eq!(p.1, 0.0, epsilon = 1e-12);
        }
        assert_eq!(s.polylines.len(), 2); // length assert before the zip above proves nothing dropped
    }

    #[test]
    fn series_members_stack_without_overlap() {
        let d = series_two_member_design();
        let s = assembly_scene(&d);
        assert_eq!(s.polylines.len(), 2);
        let first_top = s.polylines[0]
            .points
            .iter()
            .map(|p| p.1)
            .fold(f64::NEG_INFINITY, f64::max);
        let second_bottom = s.polylines[1]
            .points
            .iter()
            .map(|p| p.1)
            .fold(f64::INFINITY, f64::min);
        assert!(
            second_bottom > first_top,
            "series member 2 must start above member 1's top (gap): {second_bottom} vs {first_top}"
        );
    }

    #[test]
    fn roles_alternate_wire_and_member_by_index() {
        let d = nested_two_member_design();
        let s = assembly_scene(&d);
        assert_eq!(s.polylines[0].role, SceneRole::Wire);
        assert_eq!(s.polylines[1].role, SceneRole::Member);
    }

    /// A member coil count past the helix render cap (`MAX_RENDER_TURNS`)
    /// is VALID form input — active "2001" with free length "5000" solves —
    /// but that member's capped body comes back EMPTY. Panel-R2 contract:
    /// any empty member body makes the WHOLE scene degenerate (extent
    /// `None` → placeholder) rather than silently rendering the assembly
    /// minus one member, which would misrepresent the design.
    #[test]
    fn capped_member_makes_the_whole_scene_degenerate() {
        let mut form = two_member_form("series");
        form.members[1].active = "2001".into();
        form.members[1].free_length = "5000".into();
        let d = solve(&form);
        let s = assembly_scene(&d);
        assert!(
            scene_extent(&s).is_none(),
            "a capped (empty) member must degrade the whole scene, not vanish from it"
        );
    }

    /// Nested has no cross-member coupling (every polyline is independent),
    /// so one member's corrupted radius (NaN x/z) must not take down the
    /// other member's otherwise-finite polyline.
    #[test]
    fn nested_single_degenerate_member_does_not_take_down_the_scene() {
        let mut d = nested_two_member_design();
        d.members[0].design.mean_dia = springcore::units::Length::from_millimeters(f64::NAN);
        let s = assembly_scene(&d);
        assert!(
            scene_extent(&s).is_some(),
            "member 2's polyline is still finite even though member 1's is degenerate"
        );
    }

    /// Series stacks via a running y-offset derived from each member's own
    /// solved height; a NaN first-member height (via `wire_dia`, which feeds
    /// `coil_height_fn` without tripping the entry guard) poisons every
    /// member after it (NaN + gap = NaN), so a broken FIRST member yields a
    /// fully degenerate (whole-scene placeholder) result. Documented
    /// trade-off of the shared running-offset accumulator (task report).
    #[test]
    fn series_degenerate_first_member_cascades_through_the_axial_offset() {
        let mut d = series_two_member_design();
        d.members[0].design.wire_dia = springcore::units::Length::from_millimeters(f64::NAN);
        let s = assembly_scene(&d);
        assert!(
            scene_extent(&s).is_none(),
            "a NaN first-member height must poison every later member's offset"
        );
    }

    /// R2 sibling parity: a NaN member PITCH now routes through
    /// `scene_from_radius`'s entry guard (non-finite pitch → empty body →
    /// the capped-member whole-scene bail) — the SAME verdict
    /// `assembly_sdf` reaches for any hostile member, instead of the
    /// wireframe's former partial-render cascade for this field.
    #[test]
    fn nan_pitch_member_degrades_the_whole_scene_matching_the_sdf_path() {
        let mut d = series_two_member_design();
        d.members[1].design.pitch = springcore::units::Length::from_millimeters(f64::NAN);
        let s = assembly_scene(&d);
        assert!(
            scene_extent(&s).is_none(),
            "a NaN-pitch member must degrade the whole scene, matching assembly_sdf"
        );
    }

    /// Companion to the FIRST-member cascade above: when a MIDDLE member is
    /// poisoned instead, the members BEFORE it are untouched (the running
    /// y-offset accumulator only starts poisoning once it reaches the broken
    /// member), so the scene is NOT wholly degenerate — `scene_extent` is
    /// still `Some`, derived from the surviving earlier member. The poisoned
    /// member's own points, and every member after it (whose offset now
    /// inherits the NaN), are individually non-finite and get filtered by
    /// the shared `finite3` check rather than taking down the whole scene.
    /// Documented accepted cascade semantics (task report / spec §Degenerate
    /// handling's "partial-render" case).
    #[test]
    fn series_degenerate_middle_member_erases_only_itself_and_later_members() {
        let mut form = two_member_form("series");
        form.members.push(AsmMemberForm {
            wire_dia: "1".into(),
            mean_dia: "12".into(),
            active: "6".into(),
            free_length: "60".into(),
            ..AsmMemberForm::blank("Music Wire")
        });
        let mut d = solve(&form);
        d.members[1].design.wire_dia = springcore::units::Length::from_millimeters(f64::NAN);
        let s = assembly_scene(&d);

        assert!(
            scene_extent(&s).is_some(),
            "member 0 (before the poisoned member) stays finite, so the scene is not wholly degenerate"
        );
        let all_finite =
            |line: &crate::viz::Polyline3| line.points.iter().copied().all(crate::viz::finite3);
        assert!(
            all_finite(&s.polylines[0]),
            "member 0, before the poisoned member, must stay fully finite"
        );
        assert!(
            s.polylines[1].points.iter().all(|p| !p.1.is_finite()),
            "the poisoned member's own points must be non-finite (filtered, not just member 0)"
        );
        assert!(
            s.polylines[2].points.iter().all(|p| !p.1.is_finite()),
            "a member AFTER the poisoned one must be erased too via the cascading y-offset"
        );
    }
}
