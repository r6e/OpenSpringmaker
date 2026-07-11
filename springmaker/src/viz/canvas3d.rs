//! Humble orbit canvas: draws the pre-rendered scene bitmap and converts
//! mouse drags into `Message::Orbit` (committed angles are App state — they
//! must survive re-render, unlike chart hover). Drag TRACKING (the last
//! cursor position) is ephemeral canvas state.

use super::{orbit_step, Orbit, SceneData};
use crate::app::Message;
use crate::plot::mapping::letterbox;
use crate::plot::{CHART_H, CHART_W};
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Event, Frame, Geometry};
use iced::{Element, Length, Point, Rectangle, Renderer, Size, Theme};

pub struct OrbitCanvas {
    pub handle: iced::widget::image::Handle,
    pub orbit: Orbit,
}

#[derive(Default)]
pub struct DragState {
    last: Option<Point>,
}

impl canvas::Program<Message> for OrbitCanvas {
    type State = DragState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                state.last = cursor.position_in(bounds);
                None
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let pos = cursor.position_in(bounds)?;
                let last = state.last?;
                state.last = Some(pos);
                let next = orbit_step(self.orbit, pos.x - last.x, pos.y - last.y);
                Some(canvas::Action::publish(Message::Orbit(next)))
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
        let lb = letterbox(bounds.width, bounds.height);
        let (w, h) = (CHART_W as f32 * lb.scale, CHART_H as f32 * lb.scale);
        frame.draw_image(
            Rectangle::new(Point::new(lb.offset_x, lb.offset_y), Size::new(w, h)),
            &self.handle,
        );
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
            // Cursor is outside the canvas entirely — the same "no special
            // affordance" case `canvas::Program`'s own default impl, and every
            // draggable iced widget (slider, text_input), report by falling
            // back to `default()`. `Interaction::Idle` is a different state
            // (over the widget with no affordance to show), which never
            // applies here — `Grab` already covers "over, not dragging".
            mouse::Interaction::default()
        }
    }
}

/// Placeholder shown when the scene is degenerate (same pattern as the chart).
pub(crate) const SCENE_PLACEHOLDER: &str = "3D view unavailable for this design (check inputs).";

/// Build the 3D element: orbitable canvas, or the placeholder for a
/// degenerate scene.
///
/// Called from every family's results panel: compression and conical
/// (Task 4), extension and torsion (Task 5), assembly (Task 6). This body is
/// what keeps
/// `render_scene`, `OrbitCanvas`, `SCENE_PLACEHOLDER` (and, via the trait
/// impl, `orbit_step`) alive in the bin target.
pub fn scene_element(scene: SceneData, orbit: Orbit) -> Element<'static, Message> {
    match super::render3d::render_scene(&scene, orbit) {
        None => iced::widget::text(SCENE_PLACEHOLDER).into(),
        Some(pixels) => {
            let handle = iced::widget::image::Handle::from_rgba(CHART_W, CHART_H, pixels);
            Canvas::new(OrbitCanvas { handle, orbit })
                .width(Length::Fill)
                .height(Length::Fixed(CHART_H as f32))
                .into()
        }
    }
}
