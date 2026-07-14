//! Family-agnostic projection of a 3D `SceneData` to the 2D side elevation:
//! `axial = y`, `radial = x` (drop `z`), then each polyline's centerline is
//! offset ±wire/2 perpendicular to its local 2D tangent to draw the wire's two
//! silhouette edges (the crossing double-strand look). All coordinates are
//! model mm; the humble canvas applies the only affine.
use crate::viz::{self, SceneData};

pub type P2 = (f64, f64);

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

pub fn project_silhouette(scene: &SceneData) -> Option<Projected> {
    // Gate on the same finiteness contract the renderer uses.
    viz::scene_extent(scene)?;

    let mut edges = Vec::with_capacity(scene.polylines.len() * 2);
    for line in &scene.polylines {
        // Skip empty (capped) bodies; scene_extent already vetoed a fully
        // degenerate scene, but a mixed scene could carry an empty polyline.
        if line.points.len() < 2 {
            continue;
        }
        let center: Vec<P2> = line.points.iter().copied().map(to_2d).collect();
        let wire_r = line.wire_mm / 2.0;
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

    let mut b = Bounds {
        axial_min: f64::INFINITY,
        axial_max: f64::NEG_INFINITY,
        radial_min: f64::INFINITY,
        radial_max: f64::NEG_INFINITY,
    };
    for (a, r) in edges.iter().flat_map(|e| e.points.iter().copied()) {
        if !a.is_finite() || !r.is_finite() {
            continue;
        }
        b.axial_min = b.axial_min.min(a);
        b.axial_max = b.axial_max.max(a);
        b.radial_min = b.radial_min.min(r);
        b.radial_max = b.radial_max.max(r);
    }
    (b.axial_min.is_finite() && b.axial_max.is_finite()).then_some(Projected { edges, bounds: b })
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
