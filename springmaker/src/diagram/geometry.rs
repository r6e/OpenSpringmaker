//! Family-agnostic projection of a 3D `SceneData` to the 2D side elevation:
//! `axial = y`, `radial = x` (drop `z`), then each polyline's centerline is
//! offset ±wire/2 perpendicular to its local 2D tangent to draw the wire's two
//! silhouette edges (the crossing double-strand look). All coordinates are
//! model mm; the humble canvas applies the only affine.
use crate::viz::{self, SceneData};

pub type P2 = (f64, f64);

/// Whether both coordinates of a projected point are finite. Shared by
/// `bounds_of` (the silhouette/inset bounds) and `layout::geometry_is_finite`
/// (the dimension guard) so the two finiteness checks cannot drift apart —
/// the 2D analogue of `viz::finite3` (and the reason `plot::plottable` exists).
pub(crate) fn finite2(p: P2) -> bool {
    p.0.is_finite() && p.1.is_finite()
}

pub struct Edge2 {
    pub points: Vec<P2>,
    // Deliberate public API surface: carried through from the 3D scene's
    // `SceneRole` for future role-based rendering (e.g. dashed envelope
    // lines); the canvas does not yet discriminate on it.
    #[allow(dead_code)]
    pub role: viz::SceneRole,
}

pub struct Bounds {
    pub axial_min: f64,
    pub axial_max: f64,
    pub radial_min: f64,
    pub radial_max: f64,
}

pub struct Projected {
    pub edges: Vec<Edge2>,
    pub bounds: Bounds,
}

/// Project a 3D point to `(axial, radial)`: axial = y (the spring axis, drawn
/// horizontal), radial = x. z is dropped (orthographic side elevation).
fn to_2d(p: (f64, f64, f64)) -> P2 {
    (p.1, p.0)
}

/// Unit perpendicular to the local tangent at centerline index `i`, using the
/// adjacent segment(s). Perpendicular of tangent (tx, ty) is (-ty, tx).
/// Returns (0, 0) for a zero-length tangent (coincident points).
fn normal_at(pts: &[P2], i: usize) -> P2 {
    let n = pts.len();
    let (a, b) = if i == 0 {
        (pts[0], pts[1])
    } else if i == n - 1 {
        (pts[n - 2], pts[n - 1])
    } else {
        (pts[i - 1], pts[i + 1])
    };
    let (tx, ty) = (b.0 - a.0, b.1 - a.1);
    let len = tx.hypot(ty);
    if len == 0.0 {
        (0.0, 0.0)
    } else {
        (-ty / len, tx / len)
    }
}

/// Bounding envelope over a set of edges (model mm), skipping non-finite
/// points; `None` when no finite point exists. Shared by `project_silhouette`
/// (the main silhouette) and the canvas's corner inset (Task 9).
pub fn bounds_of(edges: &[Edge2]) -> Option<Bounds> {
    let mut b = Bounds {
        axial_min: f64::INFINITY,
        axial_max: f64::NEG_INFINITY,
        radial_min: f64::INFINITY,
        radial_max: f64::NEG_INFINITY,
    };
    for (a, r) in edges.iter().flat_map(|e| e.points.iter().copied()) {
        if !finite2((a, r)) {
            continue;
        }
        b.axial_min = b.axial_min.min(a);
        b.axial_max = b.axial_max.max(a);
        b.radial_min = b.radial_min.min(r);
        b.radial_max = b.radial_max.max(r);
    }
    (b.axial_min.is_finite() && b.axial_max.is_finite()).then_some(b)
}

pub fn project_silhouette(scene: &SceneData) -> Option<Projected> {
    // Gate on the same finiteness contract the renderer uses.
    viz::scene_extent(scene)?;

    let mut edges = Vec::with_capacity(scene.polylines.len() * 2);
    for line in &scene.polylines {
        // Project, dropping any non-finite point. `scene_extent`/`bounds_of`
        // only veto an *entirely* non-finite scene; a mixed one could otherwise
        // carry a NaN/inf point straight into `draw_edges`'s `Path` (which can
        // panic the tessellator) — the silhouette-side parity of `layout`'s
        // dimension finiteness guard. Filtering here also stops one bad point
        // poisoning its neighbours' `normal_at` tangents. Unreachable through a
        // real solve (every family finiteness-guards its scene fields), so this
        // is defense in depth; for an all-finite body it is a no-op.
        let center: Vec<P2> = line
            .points
            .iter()
            .copied()
            .map(to_2d)
            .filter(|&p| finite2(p))
            .collect();
        let wire_r = line.wire_mm / 2.0;
        // A non-finite gauge or a body left with fewer than two finite points
        // (also the empty/capped case) cannot draw a silhouette — skip it.
        if !wire_r.is_finite() || center.len() < 2 {
            continue;
        }
        let mut outer = Vec::with_capacity(center.len());
        let mut inner = Vec::with_capacity(center.len());
        for i in 0..center.len() {
            let (nx, ny) = normal_at(&center, i);
            let (cx, cy) = center[i];
            outer.push((cx + nx * wire_r, cy + ny * wire_r));
            inner.push((cx - nx * wire_r, cy - ny * wire_r));
        }
        edges.push(Edge2 {
            points: outer,
            role: line.role,
        });
        edges.push(Edge2 {
            points: inner,
            role: line.role,
        });
    }

    let bounds = bounds_of(&edges)?;
    Some(Projected { edges, bounds })
}

/// Drop every non-finite point from each edge, then drop any edge left with
/// fewer than two points. The built-edge analogue of `project_silhouette`'s
/// centerline filter, for `Edge2`s constructed OUTSIDE the projection (the
/// torsion end-on inset legs, built in `torsion::diagram_model`): `bounds_of`
/// only *skips* non-finite points when sizing the box, it does not remove them
/// from the point vectors that `draw_edges` later strokes, so a mixed inset
/// could carry a NaN/inf straight into a `Path`. Runs in `prep_inset` so the
/// inset gets the same finiteness parity the main silhouette has. No-op for a
/// finite inset (unreachable through a real solve; defense in depth).
pub(crate) fn retain_finite_edges(edges: Vec<Edge2>) -> Vec<Edge2> {
    edges
        .into_iter()
        .filter_map(|e| {
            let points: Vec<P2> = e.points.into_iter().filter(|&p| finite2(p)).collect();
            (points.len() >= 2).then_some(Edge2 {
                points,
                role: e.role,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compression::scene_model::compression_scene;
    use approx::assert_relative_eq;
    use springcore::units::{Force, Length};
    use springcore::{EndFixity, EndType, MaterialSet, PowerUser, Scenario};

    fn compression_design() -> springcore::SpringDesign {
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
            inactive_coils: None,
            free_length: Length::from_millimeters(60.0),
            loads: vec![Force::from_newtons(10.0), Force::from_newtons(30.0)],
        }
        .solve(&m, springcore::CurvatureCorrection::Bergstrasser)
        .unwrap()
    }

    #[test]
    fn projects_two_silhouette_edges_per_wire_polyline() {
        let scene = compression_scene(&compression_design());
        let p = project_silhouette(&scene).expect("finite scene projects");
        // One body Wire polyline → two silhouette edges (outer + inner),
        // index-aligned with the centerline.
        assert_eq!(p.edges.len(), 2);
        assert!(p
            .edges
            .iter()
            .all(|e| e.points.len() == scene.polylines[0].points.len()));
    }

    #[test]
    fn silhouette_edge_separation_equals_the_true_wire_diameter() {
        // The point of the crossing double-strand choice: the perpendicular gap
        // between the two silhouette edges is the TRUE wire diameter in model mm
        // (a real geometric width, not a clamped stroke). EXACT at every sample —
        // this is the honest-thickness invariant that makes OD/ID fall out.
        let d = compression_design(); // wire 2mm
        let p = project_silhouette(&compression_scene(&d)).unwrap();
        let (outer, inner) = (&p.edges[0], &p.edges[1]);
        assert_eq!(outer.points.len(), inner.points.len());
        for (a, b) in outer.points.iter().zip(&inner.points) {
            assert_relative_eq!(
                (a.0 - b.0).hypot(a.1 - b.1),
                d.wire_dia.millimeters(),
                max_relative = 1e-9
            );
        }
    }

    #[test]
    fn silhouette_midpoint_at_the_axial_start_is_the_mean_radius() {
        // Drop-z side elevation: radial = x. At the first sample (θ=0) the
        // centerline sits at radial = mean/2 exactly, and the two silhouette
        // points straddle it, so their midpoint == mean/2. EXACT — ties the
        // drawn geometry to the mean-diameter field regardless of the discrete
        // perpendicular's direction.
        let d = compression_design(); // mean 20mm
        let p = project_silhouette(&compression_scene(&d)).unwrap();
        let mid = (p.edges[0].points[0].1 + p.edges[1].points[0].1) / 2.0;
        assert_relative_eq!(mid, d.mean_dia.millimeters() / 2.0, max_relative = 1e-9);
    }

    #[test]
    fn axial_span_matches_free_length() {
        // axial = y ∈ [0, H] with H = free_length (compression scene_model). The
        // silhouette offset perturbs each end by at most wire/2, so the edge
        // bounds' axial span sits within a wire diameter of the true free length.
        let d = compression_design();
        let p = project_silhouette(&compression_scene(&d)).unwrap();
        let span = p.bounds.axial_max - p.bounds.axial_min;
        assert!(
            (span - d.free_length.millimeters()).abs() <= d.wire_dia.millimeters(),
            "axial span {span} not within one wire dia of free length {}",
            d.free_length.millimeters()
        );
    }

    #[test]
    fn degenerate_scene_projects_to_none() {
        let mut d = compression_design();
        d.mean_dia = Length::from_millimeters(f64::NAN);
        let scene = compression_scene(&d);
        assert!(project_silhouette(&scene).is_none());
    }

    fn polyline(points: Vec<(f64, f64, f64)>, wire_mm: f64) -> crate::viz::Polyline3 {
        crate::viz::Polyline3 {
            points,
            role: crate::viz::SceneRole::Wire,
            stroke_px: 1,
            wire_mm,
        }
    }

    #[test]
    fn mixed_scene_drops_non_finite_points_but_keeps_the_polyline() {
        // A finite body plus a polyline carrying one NaN point (finite gauge):
        // `scene_extent`/`bounds_of` still return `Some` off the finite points,
        // so without the per-point filter the NaN would reach `draw_edges`'s
        // `Path`. The bad point must be dropped, the two surviving points still
        // drawn, and EVERY emitted edge coordinate finite.
        let scene = crate::viz::SceneData {
            polylines: vec![
                polyline(
                    vec![(0.0, 0.0, 0.0), (2.0, 5.0, 0.0), (0.0, 10.0, 0.0)],
                    2.0,
                ),
                polyline(
                    vec![(0.0, 0.0, 0.0), (f64::NAN, 5.0, 0.0), (0.0, 10.0, 0.0)],
                    2.0,
                ),
            ],
        };
        let p = project_silhouette(&scene).expect("the finite body keeps the scene renderable");
        // Both polylines contribute (2 edges each): the degenerate one is
        // compacted to its 2 finite points, not discarded wholesale.
        assert_eq!(p.edges.len(), 4);
        assert!(
            p.edges
                .iter()
                .flat_map(|e| &e.points)
                .all(|&pt| finite2(pt)),
            "every silhouette edge point must be finite"
        );
    }

    #[test]
    fn mixed_scene_skips_a_non_finite_wire_gauge_polyline() {
        // A finite body plus a polyline whose wire gauge is NaN: its offset
        // points would all be NaN, so the whole polyline is skipped. The scene
        // still renders off the finite body, with only finite edges.
        let scene = crate::viz::SceneData {
            polylines: vec![
                polyline(
                    vec![(0.0, 0.0, 0.0), (2.0, 5.0, 0.0), (0.0, 10.0, 0.0)],
                    2.0,
                ),
                polyline(
                    vec![(0.0, 0.0, 0.0), (2.0, 5.0, 0.0), (0.0, 10.0, 0.0)],
                    f64::NAN,
                ),
            ],
        };
        let p = project_silhouette(&scene).expect("the finite body keeps the scene renderable");
        assert_eq!(
            p.edges.len(),
            2,
            "the NaN-gauge polyline must contribute nothing"
        );
        assert!(p
            .edges
            .iter()
            .flat_map(|e| &e.points)
            .all(|&pt| finite2(pt)));
    }

    #[test]
    fn retain_finite_edges_strips_non_finite_points_and_drops_thin_edges() {
        let edge = |pts: Vec<P2>| Edge2 {
            points: pts,
            role: crate::viz::SceneRole::Detail,
        };
        let edges = vec![
            // One NaN point dropped; the two finite points survive.
            edge(vec![(0.0, 0.0), (f64::NAN, 1.0), (2.0, 2.0)]),
            // Reduced below two finite points → the whole edge is dropped.
            edge(vec![(f64::INFINITY, 0.0), (1.0, 1.0)]),
            // Fully finite → unchanged.
            edge(vec![(3.0, 3.0), (4.0, 4.0)]),
        ];
        let out = retain_finite_edges(edges);
        assert_eq!(out.len(), 2, "the thin (one-finite-point) edge is dropped");
        assert_eq!(
            out[0].points.len(),
            2,
            "the NaN point is removed, not the edge"
        );
        assert!(out.iter().flat_map(|e| &e.points).all(|&p| finite2(p)));
    }
}

// PROJECTION MODEL (locked): drop-z side elevation — `radial = x`, `axial = y`,
// z discarded. This is what makes the crossing double-strand look: radial
// oscillates ±R along the axis, so adjacent coils' sine ribbons overlap and
// their edges cross (the textbook interlocking-X look, for the true-scale case
// diameter > pitch). Do NOT switch to `radial = hypot(x,z)` (the developed/
// envelope model) — it draws a flat constant-radius band with no visible coils.
// Consequence: the OUTER silhouette envelope only approaches OD/2 to sampling
// resolution (~3% at 32 samples/turn), so OD/ID are NOT verified from the
// geometry envelope here — they are verified in Task 2 as `presenter-half ==
// design OD/2` (exact). The EXACT geometry ties in Task 1 are edge-separation
// == wire_dia and edge-midpoint == mean/2.
