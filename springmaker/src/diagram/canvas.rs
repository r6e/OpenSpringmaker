//! Humble 2D-diagram canvas (ADR 0008): applies ONE affine (fit → zoom → pan)
//! to model-mm geometry and draws the wire silhouette and laid-out dimensions
//! with native iced `Frame`/`Path`/`Text`. The sole screen-space exception is
//! dimension-text size, held constant px per CAD convention. Scroll publishes
//! `DiagramZoom`; drag publishes `DiagramPan` — deltas, never absolute values
//! read back from `self` (the `OrbitCanvas` stale-base rule).

use std::f64::consts::PI;

use crate::app::{Message, Palette};
use crate::diagram::{
    bounds_of, layout, project_silhouette, Bounds, DiagramView, DimLayers, Edge2, LayoutedDim,
    Projected, P2,
};
use iced::alignment::Vertical;
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Event, Frame, Geometry, Path, Stroke, Text};
use iced::widget::text::Alignment as TextAlignment;
use iced::{Color, Element, Length, Point, Rectangle, Renderer, Theme, Vector};

/// A uniform-scale affine from model mm `(axial, radial)` to screen px.
pub struct Transform {
    pub scale: f32,
    pub offset: Vector, // screen-space translation (fit-center + pan)
}

impl Transform {
    pub fn apply(&self, p: P2) -> (f32, f32) {
        (
            p.0 as f32 * self.scale + self.offset.x,
            // radial grows downward on screen; keep axial left→right.
            -(p.1 as f32) * self.scale + self.offset.y,
        )
    }

    fn point(&self, p: P2) -> Point {
        let (x, y) = self.apply(p);
        Point::new(x, y)
    }
}

/// Fit `bounds` (plus a dimension-ladder margin) into `w × h`, centered, then
/// apply the view's zoom (about the center) and pan. Uniform scale preserves
/// true proportions.
pub fn fit_transform(bounds: &Bounds, w: f32, h: f32, view: DiagramView) -> Transform {
    const MARGIN: f32 = 40.0; // room for ladders/text around the envelope
    let span_a = (bounds.axial_max - bounds.axial_min).max(1e-6) as f32;
    let span_r = (bounds.radial_max - bounds.radial_min).max(1e-6) as f32;
    let sx = (w - 2.0 * MARGIN) / span_a;
    let sy = (h - 2.0 * MARGIN) / span_r;
    let scale = sx.min(sy).max(1e-6) * view.zoom;
    let cx = ((bounds.axial_min + bounds.axial_max) / 2.0) as f32;
    let cy = ((bounds.radial_min + bounds.radial_max) / 2.0) as f32;
    // Place the model center at the canvas center, then pan.
    let offset = Vector::new(
        w / 2.0 - cx * scale + view.pan.x,
        h / 2.0 + cy * scale + view.pan.y,
    );
    Transform { scale, offset }
}

/// Bounding envelope over an inset's edges (model mm) — a thin wrapper around
/// the shared `geometry::bounds_of`, the pure, testable seam (ADR 0008)
/// behind the corner `fit_transform` in `draw`. `None` when no finite point
/// exists (an empty or fully degenerate inset), so the caller can skip
/// drawing it rather than fit a transform to garbage.
fn inset_bounds(edges: &[Edge2]) -> Option<Bounds> {
    bounds_of(edges)
}

/// A corner sub-rectangle for the inset, anchored bottom-right, sized as a
/// fixed fraction of the canvas with a fixed margin. `None` when the canvas
/// is too small for the box to clear `fit_transform`'s own internal margin
/// (which would otherwise degenerate to a near-zero scale) — the caller
/// skips drawing the inset rather than render a garbled sliver.
fn inset_sub_rect(canvas_w: f32, canvas_h: f32) -> Option<Rectangle> {
    const FRACTION: f32 = 0.34;
    const MARGIN: f32 = 8.0;
    // Must clear `fit_transform`'s own 2×40px internal margin with headroom.
    const MIN_SIZE: f32 = 90.0;
    let width = canvas_w * FRACTION;
    let height = canvas_h * FRACTION;
    if width < MIN_SIZE || height < MIN_SIZE {
        return None;
    }
    let x = canvas_w - width - MARGIN;
    let y = canvas_h - height - MARGIN;
    if x < 0.0 || y < 0.0 {
        return None;
    }
    Some(Rectangle {
        x,
        y,
        width,
        height,
    })
}

/// The inset's projected edges and laid-out dims, prepared once off the
/// frame (`diagram_element`) so `draw` only applies a transform and strokes.
struct PreppedInset {
    edges: Vec<Edge2>,
    laid_out: Vec<LayoutedDim>,
    bounds: Bounds,
}

pub struct DiagramCanvas {
    projected: Projected,
    laid_out: Vec<LayoutedDim>,
    view: DiagramView,
    wire: Color,
    dim: Color,
    inset: Option<PreppedInset>,
}

/// Stroke a set of edges (plain polylines, e.g. a wire silhouette or the
/// inset's leg edges) under `t` into `frame` using `color`. Shared by the
/// main silhouette and the corner inset (Task 9).
fn draw_edges(frame: &mut Frame, t: &Transform, edges: &[Edge2], color: Color) {
    for edge in edges {
        if edge.points.len() < 2 {
            continue;
        }
        let path = Path::new(|b| {
            b.move_to(t.point(edge.points[0]));
            for &p in &edge.points[1..] {
                b.line_to(t.point(p));
            }
        });
        frame.stroke(&path, Stroke::default().with_color(color).with_width(1.5));
    }
}

/// Stroke one set of laid-out dims (lines, arc, arrowheads, constant-px
/// centered text) under `t` into `frame` using `color`. Shared by the main
/// side elevation and the corner inset (Task 9) so the CAD-callout drawing
/// logic lives in exactly one place.
fn draw_dims(frame: &mut Frame, t: &Transform, laid_out: &[LayoutedDim], color: Color) {
    const ARROW_LEN: f32 = 7.0; // screen px, constant regardless of zoom
    const ARROW_HALF: f64 = 0.42; // ~24° half-angle
    for d in laid_out {
        for (a, b) in &d.lines {
            let seg = Path::line(t.point(*a), t.point(*b));
            frame.stroke(&seg, Stroke::default().with_color(color).with_width(1.0));
        }
        if let Some((vertex, radius, start_deg, sweep_deg)) = d.arc {
            let arc = Path::new(|bld| {
                let steps = 24;
                for i in 0..=steps {
                    let a = (start_deg + sweep_deg * i as f64 / steps as f64).to_radians();
                    let p = (vertex.0 + radius * a.cos(), vertex.1 + radius * a.sin());
                    if i == 0 {
                        bld.move_to(t.point(p));
                    } else {
                        bld.line_to(t.point(p));
                    }
                }
            });
            frame.stroke(&arc, Stroke::default().with_color(color).with_width(1.0));
        }
        for (anchor, dir) in &d.arrows {
            let tip = t.point(*anchor);
            // Model→screen direction: uniform positive scale keeps the
            // angle, the y-flip negates the sin component.
            let screen_dir = (-dir.sin()).atan2(dir.cos());
            for barb in [screen_dir + PI - ARROW_HALF, screen_dir + PI + ARROW_HALF] {
                let end = Point::new(
                    tip.x + ARROW_LEN * barb.cos() as f32,
                    tip.y + ARROW_LEN * barb.sin() as f32,
                );
                frame.stroke(
                    &Path::line(tip, end),
                    Stroke::default().with_color(color).with_width(1.0),
                );
            }
        }
        let (anchor, label) = &d.text;
        let (tx, ty) = t.apply(*anchor);
        frame.fill_text(Text {
            content: label.clone(),
            position: Point::new(tx, ty),
            color,
            size: 12.0.into(), // constant px — the CAD text-size exception
            align_x: TextAlignment::Center,
            align_y: Vertical::Center,
            ..Text::default()
        });
    }
}

#[derive(Default)]
pub struct DragState {
    last: Option<Point>,
}

impl canvas::Program<Message> for DiagramCanvas {
    type State = DragState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                cursor.position_in(bounds)?; // only when the cursor is over us
                let d = match delta {
                    mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. } => {
                        *y
                    }
                };
                Some(canvas::Action::publish(Message::DiagramZoom(d)).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                state.last = cursor.position_in(bounds);
                None
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let pos = cursor.position_in(bounds)?;
                let last = state.last?;
                state.last = Some(pos);
                Some(canvas::Action::publish(Message::DiagramPan(
                    pos.x - last.x,
                    pos.y - last.y,
                )))
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Mouse(mouse::Event::CursorLeft) => {
                state.last = None;
                None
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let t = fit_transform(
            &self.projected.bounds,
            bounds.width,
            bounds.height,
            self.view,
        );

        draw_edges(&mut frame, &t, &self.projected.edges, self.wire);
        draw_dims(&mut frame, &t, &self.laid_out, self.dim);

        // End-on inset (torsion legs; Task 9): a bordered box anchored in the
        // bottom-right corner, drawn last so it sits atop the main content.
        // Its transform uses the DEFAULT view (no zoom/pan) — the inset is a
        // fixed reference, not a viewport onto the main scene — then is
        // offset into the corner sub-rectangle.
        if let Some(prepped) = &self.inset {
            if let Some(rect) = inset_sub_rect(bounds.width, bounds.height) {
                let mut it = fit_transform(
                    &prepped.bounds,
                    rect.width,
                    rect.height,
                    DiagramView::default(),
                );
                it.offset += Vector::new(rect.x, rect.y);

                frame.stroke(
                    &Path::rectangle(Point::new(rect.x, rect.y), rect.size()),
                    Stroke::default().with_color(self.dim).with_width(1.0),
                );
                draw_edges(&mut frame, &it, &prepped.edges, self.wire);
                draw_dims(&mut frame, &it, &prepped.laid_out, self.dim);
            }
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.last.is_some() {
            mouse::Interaction::Grabbing
        } else if cursor.position_in(bounds).is_some() {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::default()
        }
    }
}

/// Prepare the torsion end-view inset: laid out once here, off the frame,
/// against its OWN bounds — never the main scene's. `None` (no border, no
/// content) when there is no inset, or when its edges carry no finite point
/// (`inset_bounds` → `None`); `draw` never sees it either way.
fn prep_inset(inset: Option<crate::diagram::Inset>, layers: DimLayers) -> Option<PreppedInset> {
    inset.and_then(|i| {
        inset_bounds(&i.edges).map(|bounds| PreppedInset {
            laid_out: layout(&i.dims, &bounds, layers),
            edges: i.edges,
            bounds,
        })
    })
}

/// Build the diagram element, or the shared placeholder for a degenerate scene.
pub fn diagram_element(
    pal: &'static Palette,
    input: crate::diagram::DiagramInput,
    view: DiagramView,
    layers: DimLayers,
) -> Element<'static, Message> {
    match project_silhouette(&input.scene) {
        None => crate::widgets::placeholder_text(
            pal,
            crate::viz::canvas3d::placeholder_for(&input.scene),
        ),
        Some(projected) => {
            let laid_out = layout(&input.dims, &projected.bounds, layers);
            let inset = prep_inset(input.inset, layers);
            Canvas::new(DiagramCanvas {
                projected,
                laid_out,
                view,
                wire: pal.ink,  // primary wire-stroke token (Palette in app.rs)
                dim: pal.muted, // muted dimension-line + text token
                inset,
            })
            .width(Length::Fill)
            .height(Length::Fixed(crate::plot::CHART_H as f32))
            .into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram::{zoom_step, DiagramView};
    use crate::torsion::form::{parse_and_solve, TorFormState};
    use springcore::{MaterialSet, MaterialStore, UnitSystem};

    /// Fixture mirroring `torsion::diagram_model`'s own tests: body 5.25
    /// coils (legs 90° apart), legs 15mm/10mm — nonzero everything so the
    /// inset edges/dims are non-trivial.
    fn torsion_design() -> springcore::torsion::TorsionDesign {
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
    fn prep_inset_retains_a_valid_inset_and_drops_absent_or_degenerate_ones() {
        let design = torsion_design();
        let (_dims, inset) = crate::torsion::diagram_model::diagram(&design);
        // A real torsion inset: retained, and its contents actually survive
        // (a regression to always-`None` would fail this, not just the
        // element-construction-only smoke test).
        let prepped = prep_inset(Some(inset), DimLayers::default())
            .expect("a valid inset with finite edges must be retained");
        assert!(
            !prepped.edges.is_empty(),
            "retained inset must keep its edges"
        );
        assert!(
            !prepped.laid_out.is_empty(),
            "retained inset must keep its laid-out dims"
        );

        // No inset at all.
        assert!(prep_inset(None, DimLayers::default()).is_none());

        // An inset with no edges: `inset_bounds` → `None`, so the whole
        // inset is dropped rather than fit a transform to garbage.
        let empty = crate::diagram::Inset {
            edges: vec![],
            dims: vec![],
        };
        assert!(prep_inset(Some(empty), DimLayers::default()).is_none());
    }

    #[test]
    fn inset_bounds_encloses_the_leg_tips_and_is_none_for_a_degenerate_inset() {
        let design = torsion_design();
        let (_dims, inset) = crate::torsion::diagram_model::diagram(&design);
        let b = inset_bounds(&inset.edges).expect("finite inset edges must produce bounds");
        assert!(b.axial_max > b.axial_min);
        assert!(b.radial_max > b.radial_min);
        // The bounds enclose every point of every edge actually drawn (the
        // leg tips in particular — the whole point of the inset).
        for edge in &inset.edges {
            for &(a, r) in &edge.points {
                assert!(a >= b.axial_min - 1e-9 && a <= b.axial_max + 1e-9);
                assert!(r >= b.radial_min - 1e-9 && r <= b.radial_max + 1e-9);
            }
        }

        // Empty edges: no finite point exists anywhere.
        assert!(inset_bounds(&[]).is_none());
        // All-non-finite points: skipped by the finiteness guard, leaving no
        // finite point either.
        let degenerate = vec![Edge2 {
            points: vec![(f64::NAN, f64::NAN), (f64::INFINITY, f64::NEG_INFINITY)],
            role: crate::viz::SceneRole::Detail,
        }];
        assert!(inset_bounds(&degenerate).is_none());
    }

    #[test]
    fn diagram_element_with_inset_builds_without_panicking() {
        let design = torsion_design();
        let (dims, inset) = crate::torsion::diagram_model::diagram(&design);
        let scene = crate::torsion::scene_model::torsion_scene(&design);
        let input = crate::diagram::DiagramInput::new(scene, dims).with_inset(inset);
        // The "inset element is produced when present" pin: no pixel/geometry
        // snapshot (machine-dependent), just that construction doesn't panic.
        let _element = diagram_element(
            &crate::app::DARK,
            input,
            DiagramView::default(),
            DimLayers::default(),
        );
    }

    #[test]
    fn zoom_step_clamps_and_ignores_non_finite() {
        let v = DiagramView::default();
        assert_eq!(zoom_step(v, f32::NAN).zoom, 1.0);
        // Repeated zoom-in saturates at the max, never past it.
        let mut z = v;
        for _ in 0..200 {
            z = zoom_step(z, 1.0);
        }
        assert!(z.zoom <= 8.0 + 1e-6);
        let mut zo = v;
        for _ in 0..200 {
            zo = zoom_step(zo, -1.0);
        }
        assert!(zo.zoom >= 0.2 - 1e-6);
    }

    #[test]
    fn fit_transform_centers_and_scales_bounds_into_the_canvas() {
        // A 60×22 model box fit into a 600×300 canvas (with margin). ASYMMETRIC
        // radial bounds (0..22, so cy = 11 ≠ 0) — a symmetric box would make the
        // `cy * scale` term in `offset.y` vanish and could not catch a dropped
        // y-flip or a wrong `offset.y` sign.
        let b = crate::diagram::Bounds {
            axial_min: 0.0,
            axial_max: 60.0,
            radial_min: 0.0,
            radial_max: 22.0,
        };
        let t = fit_transform(&b, 600.0, 300.0, DiagramView::default());

        // The model center (cx=30, cy=11) maps to the canvas center.
        let (mx, my) = t.apply((30.0, 11.0));
        assert!(
            (mx - 300.0).abs() < 1.0 && (my - 150.0).abs() < 1.0,
            "center mapped to ({mx}, {my}), expected ~(300, 150)"
        );
        assert!(t.scale > 0.0);

        // The top-right envelope corner (60, 22) pins three things at once:
        // axial 60 > cx 30 ⇒ x RIGHT of center (> 300); radial 22 > cy 11 AND
        // radial grows DOWNWARD on screen ⇒ the corner sits HIGHER = SMALLER y
        // (< 150). Off-center on both axes, so it survives even if only one of
        // scale/flip/offset were wrong.
        let (crx, cry) = t.apply((60.0, 22.0));
        assert!(
            crx > 300.0,
            "top-right corner x {crx} must be right of center"
        );
        assert!(cry < 150.0, "top-right corner y {cry} must be above center");
        let dist1 = ((crx - 300.0).powi(2) + (cry - 150.0).powi(2)).sqrt();

        // Zoom pivots on the center: at zoom 2 the center still maps to the
        // canvas center, but the same corner sits FURTHER from it.
        let tz = fit_transform(
            &b,
            600.0,
            300.0,
            DiagramView {
                zoom: 2.0,
                pan: iced::Vector::ZERO,
            },
        );
        let (zmx, zmy) = tz.apply((30.0, 11.0));
        assert!(
            (zmx - 300.0).abs() < 1.0 && (zmy - 150.0).abs() < 1.0,
            "zoom must pivot on center; got ({zmx}, {zmy})"
        );
        assert!(tz.scale > 0.0);
        let (zcx, zcy) = tz.apply((60.0, 22.0));
        let dist2 = ((zcx - 300.0).powi(2) + (zcy - 150.0).powi(2)).sqrt();
        assert!(
            dist2 > dist1,
            "zoomed corner distance {dist2} must exceed unzoomed {dist1}"
        );
    }
}
