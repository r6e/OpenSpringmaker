//! Humble hover canvas: draws the pre-rendered chart bitmap and a cursor
//! overlay. All coordinate decisions live in `mapping` (pure, tested); all
//! text decisions live in `hover_readout` (pure, tested). Hover state is
//! ephemeral — no Message is ever published; `Action::request_redraw()` only
//! repaints this canvas (plotters never re-rasterizes on mouse movement).

use super::mapping::{letterbox, ChartMapping};
use super::{hover_readout, ChartData, CHART_H, CHART_W};
use crate::app::{Message, C};
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Event, Frame, Geometry, Path, Stroke, Text};
use iced::{Element, Length, Point, Rectangle, Renderer, Size, Theme};

pub struct ChartCanvas {
    pub handle: iced::widget::image::Handle,
    pub mapping: ChartMapping,
    pub data: ChartData,
}

impl canvas::Program<Message> for ChartCanvas {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        event: &Event,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. })
            | Event::Mouse(mouse::Event::CursorLeft) => Some(canvas::Action::request_redraw()),
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let lb = letterbox(bounds.width, bounds.height);
        let (ox, oy) = (lb.offset_x, lb.offset_y);
        let (w, h) = (CHART_W as f32 * lb.scale, CHART_H as f32 * lb.scale);
        frame.draw_image(
            Rectangle::new(Point::new(ox, oy), Size::new(w, h)),
            &self.handle,
        );

        if let Some(pos) = cursor.position_in(bounds) {
            let (bx, by) = lb.widget_to_bitmap(pos.x, pos.y);
            if ChartMapping::in_plot_rect(bx, by) {
                self.draw_overlay(&mut frame, &lb, bx, by);
            }
        }
        vec![frame.into_geometry()]
    }
}

impl ChartCanvas {
    fn draw_overlay(&self, frame: &mut Frame, lb: &super::mapping::Letterbox, bx: f32, by: f32) {
        let (x0, y0, x1, y1) = ChartMapping::plot_rect();
        let stroke = Stroke::default().with_color(C::MUTED).with_width(1.0);
        // Crosshair clipped to the plot rect, in widget coordinates.
        let (cx, cy) = lb.bitmap_to_widget(bx, by);
        let (lx0, ly0) = lb.bitmap_to_widget(x0, y0);
        let (lx1, ly1) = lb.bitmap_to_widget(x1, y1);
        frame.stroke(
            &Path::line(Point::new(cx, ly0), Point::new(cx, ly1)),
            stroke,
        );
        frame.stroke(
            &Path::line(Point::new(lx0, cy), Point::new(lx1, cy)),
            stroke,
        );

        let (dx, dy) = self.mapping.pixel_to_data(bx, by);
        let content = hover_readout(&self.data, dx, dy);
        let (flip_x, flip_y) = ChartMapping::readout_flips(bx, by);
        // Approximate box metrics; the flip DECISION is the tested part.
        let box_w = 8.0 + content.len() as f32 * 7.0;
        let box_h = 20.0;
        let tx = if flip_x { cx - 10.0 - box_w } else { cx + 10.0 };
        let ty = if flip_y { cy + 10.0 } else { cy - 10.0 - box_h };
        frame.fill_rectangle(Point::new(tx, ty), Size::new(box_w, box_h), C::RAISED);
        frame.fill_text(Text {
            content,
            position: Point::new(tx + 4.0, ty + 3.0),
            color: C::TEXT,
            size: 13.0.into(),
            ..Text::default()
        });
    }
}

/// Build the chart element: hoverable canvas, or a text placeholder for a
/// degenerate design (plotters must never see a non-finite range).
pub fn chart_element(data: ChartData) -> Element<'static, Message> {
    match super::render::render_chart(&data) {
        None => iced::widget::text("Chart unavailable for this design (check inputs).").into(),
        Some((pixels, mapping)) => {
            let handle = iced::widget::image::Handle::from_rgba(CHART_W, CHART_H, pixels);
            Canvas::new(ChartCanvas {
                handle,
                mapping,
                data,
            })
            .width(Length::Fill)
            .height(Length::Fixed(CHART_H as f32))
            .into()
        }
    }
}
