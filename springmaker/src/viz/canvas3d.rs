//! Humble orbit canvas: draws the pre-rendered scene bitmap and converts
//! mouse drags into `Message::Orbit` drag deltas (the committed angles are
//! App state, accumulated in `App::update` via `orbit_step` — they must
//! survive re-render, unlike chart hover). Drag TRACKING (the last cursor
//! position) is ephemeral canvas state; the canvas never reads the
//! committed orbit itself, so it cannot publish against a stale base.

use super::{Orbit, SceneData};
use crate::app::{Message, Palette};
use crate::plot::mapping::{draw_letterboxed_bitmap, letterbox};
use crate::plot::{CHART_H, CHART_W};
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Event, Frame, Geometry};
use iced::{Element, Length, Point, Rectangle, Renderer, Theme};

pub struct OrbitCanvas {
    pub handle: iced::widget::image::Handle,
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
                // Publish the raw delta, not an orbit computed here: this
                // struct is rebuilt from the App's committed orbit only when
                // `view()` re-runs, so a value read from `self` could be
                // stale for events arriving before that re-render. Deltas
                // compose correctly regardless — `App::update` accumulates
                // them one at a time via `orbit_step`.
                Some(canvas::Action::publish(Message::Orbit(
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
        let lb = letterbox(bounds.width, bounds.height);
        draw_letterboxed_bitmap(&mut frame, &lb, &self.handle);
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

/// Placeholder shown when the scene is degenerate from truly bad (non-finite)
/// geometry — the design's inputs need attention.
pub(crate) const SCENE_PLACEHOLDER: &str = "3D view unavailable for this design (check inputs).";

/// Placeholder shown when the scene is degenerate because the (otherwise
/// valid) coil count exceeds the renderer's `MAX_RENDER_TURNS` cap — the
/// design itself is fine, only the 3D render self-defended.
pub(crate) const SCENE_PLACEHOLDER_CAPPED: &str =
    "3D view unavailable: coil count exceeds the renderable 3D limit.";

/// Choose which placeholder wording applies to a degenerate scene (the
/// `render_scene` `None` path only). Data-driven by
/// [`super::coil_body_is_empty`]: an empty coil body is the capped/hostile
/// coil-count outcome (valid input past the render cap — for BOTH a single
/// family's capped body and an assembly's capped whole-scene composition),
/// while a non-empty body reaching `None` means the geometry itself is
/// non-finite (bad input).
///
/// This discriminator would also call an (unreachable in practice)
/// post-solve NaN-coil mutation "capped" rather than "check inputs" — a
/// non-finite coil count also empties the body. Accepted: every REACHABLE
/// path to an empty body today is the render cap, not a NaN coil count.
///
/// `pub(crate)` for `viz::spring3d_element`'s degenerate short-circuit,
/// which must pick the SAME wording this module's `None` path does.
pub(crate) fn placeholder_for(scene: &SceneData) -> &'static str {
    if super::coil_body_is_empty(scene) {
        SCENE_PLACEHOLDER_CAPPED
    } else {
        SCENE_PLACEHOLDER
    }
}

/// Build the 3D element: orbitable canvas, or the placeholder for a
/// degenerate scene.
///
/// Called from every family's results panel (compression, conical,
/// extension, torsion, assembly) via the shared `widgets::visual_toggle`
/// slot. `orbit` is only needed to render the bitmap up front — the
/// resulting canvas never reads it back (see the module doc).
pub fn scene_element(
    pal: &'static Palette,
    scene: SceneData,
    orbit: Orbit,
) -> Element<'static, Message> {
    match super::render3d::render_scene(pal, &scene, orbit) {
        None => crate::widgets::placeholder_text(pal, placeholder_for(&scene)),
        Some(pixels) => {
            let handle = iced::widget::image::Handle::from_rgba(CHART_W, CHART_H, pixels);
            Canvas::new(OrbitCanvas { handle })
                .width(Length::Fill)
                .height(Length::Fixed(CHART_H as f32))
                .into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viz::{close_wound_coil, coil_body_is_empty, Polyline3, SceneRole};

    fn scene_with_nan_points() -> SceneData {
        SceneData {
            polylines: vec![Polyline3 {
                points: vec![(0.0, f64::NAN, 0.0), (1.0, 2.0, 3.0)],
                role: SceneRole::Wire,
                stroke_px: 1,
            }],
        }
    }

    #[test]
    fn scene_element_picks_the_capped_wording_for_an_empty_body() {
        let scene = close_wound_coil(10.0, 2001.0, 2.0); // capped ⇒ empty body
        assert!(coil_body_is_empty(&scene)); // sanity: the discriminator
        assert_eq!(placeholder_for(&scene), SCENE_PLACEHOLDER_CAPPED);
    }

    #[test]
    fn scene_element_keeps_check_inputs_for_nonfinite_geometry() {
        let scene = scene_with_nan_points();
        assert_eq!(placeholder_for(&scene), SCENE_PLACEHOLDER);
    }
}
