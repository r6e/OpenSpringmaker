//! Humble 2D-diagram canvas (ADR 0008): applies ONE affine (fit → zoom → pan)
//! to model-mm geometry and draws the wire silhouette and laid-out dimensions
//! with native iced `Frame`/`Path`/`Text`. The sole screen-space exception is
//! dimension-text size, held constant px per CAD convention. Scroll publishes
//! `DiagramZoom`; drag publishes `DiagramPan` — deltas, never absolute values
//! read back from `self` (the `OrbitCanvas` stale-base rule).

use crate::app::{Message, Palette};
use crate::diagram::{
    layout, project_silhouette, Bounds, DiagramView, DimLayers, LayoutedDim, Projected, P2,
};
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Event, Frame, Geometry, Path, Stroke, Text};
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

#[allow(dead_code)] // consumed by the results dispatch in Task 5
pub struct DiagramCanvas {
    projected: Projected,
    laid_out: Vec<LayoutedDim>,
    view: DiagramView,
    wire: Color,
    dim: Color,
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

        // Wire silhouette edges.
        for edge in &self.projected.edges {
            if edge.points.len() < 2 {
                continue;
            }
            let path = Path::new(|b| {
                b.move_to(t.point(edge.points[0]));
                for &p in &edge.points[1..] {
                    b.line_to(t.point(p));
                }
            });
            frame.stroke(
                &path,
                Stroke::default().with_color(self.wire).with_width(1.5),
            );
        }

        // Dimensions: lines, arcs, constant-px text. (`d.arrows` — arrowhead
        // anchor + direction — is laid out by `layout` but not yet drawn
        // here; the line + text already carry the callout unambiguously.)
        for d in &self.laid_out {
            for (a, b) in &d.lines {
                let seg = Path::line(t.point(*a), t.point(*b));
                frame.stroke(&seg, Stroke::default().with_color(self.dim).with_width(1.0));
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
                frame.stroke(&arc, Stroke::default().with_color(self.dim).with_width(1.0));
            }
            let (anchor, label) = &d.text;
            let (tx, ty) = t.apply(*anchor);
            frame.fill_text(Text {
                content: label.clone(),
                position: Point::new(tx, ty),
                color: self.dim,
                size: 12.0.into(), // constant px — the CAD text-size exception
                ..Text::default()
            });
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

/// Build the diagram element, or the shared placeholder for a degenerate scene.
#[allow(dead_code)] // consumed by the results dispatch in Task 5
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
            // `input.inset` (the torsion end-view) is drawn starting Task 9;
            // ignored here.
            Canvas::new(DiagramCanvas {
                projected,
                laid_out,
                view,
                wire: pal.ink,  // primary wire-stroke token (Palette in app.rs)
                dim: pal.muted, // muted dimension-line + text token
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
