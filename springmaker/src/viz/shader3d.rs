//! Humble shaded-3D shader: the iced `shader::Program`/`Primitive`/`Pipeline`
//! plumbing around the WGSL ray-marcher in `shader3d.wgsl`. ADR 0008 humble
//! discipline is BINDING here — no distance-function math, no camera math,
//! no packing math belongs in this file; it only substitutes the shared
//! constants from `sdf.rs` into the WGSL template and moves already-packed
//! bytes to/from the GPU. The WGSL itself is the mirror of `sdf.rs`
//! (function-for-function); this file is the plumbing that gets it there.

use iced::mouse;
use iced::wgpu;
use iced::widget::shader::{self, Action};
use iced::{Point, Rectangle};

use crate::app::Message;

use super::sdf;

/// Raw WGSL source, unmodified — [`instantiate_wgsl`] substitutes every
/// `{{PLACEHOLDER}}` token below before it reaches `wgpu::ShaderSource::Wgsl`.
const WGSL_TEMPLATE: &str = include_str!("shader3d.wgsl");

/// Substitute every shared numeric constant from `sdf.rs` into the WGSL
/// template. Chained `str::replace` calls, deliberately NOT `format!`: the
/// shader source is full of bare `{`/`}` braces (block scopes, struct
/// bodies) that `format!`'s own brace-escaping rules would misparse (and
/// `format!` needs a literal format string in the first place, which
/// `include_str!`'s runtime `&str` can never be). Every substituted value is
/// read directly from `sdf.rs`'s own `pub(crate) const`s — never re-typed —
/// so the two sides cannot drift apart on a shared numeric budget.
pub(crate) fn instantiate_wgsl() -> String {
    WGSL_TEMPLATE
        .replace("{{MAX_PARTS}}", &sdf::MAX_PARTS.to_string())
        .replace("{{MAX_CUTS}}", &sdf::MAX_CUTS.to_string())
        .replace("{{FLOATS_PER_PART}}", &sdf::FLOATS_PER_PART.to_string())
        .replace("{{FLOATS_PER_CUT}}", &sdf::FLOATS_PER_CUT.to_string())
        .replace("{{MARCH_MAX_STEPS}}", &sdf::MARCH_MAX_STEPS.to_string())
        .replace("{{MARCH_SAFETY}}", &sdf::MARCH_SAFETY.to_string())
        .replace("{{MARCH_EPS}}", &sdf::MARCH_EPS.to_string())
}

/// Per-drag ephemeral canvas state — mirrors `canvas3d::DragState` exactly
/// (last cursor position, `None` when no drag is in progress).
#[derive(Debug, Default, PartialEq)]
pub(crate) struct DragState {
    last: Option<Point>,
}

/// The `shader::Program` for the shaded 3D view: carries the already-packed
/// scene/camera/background data (computed upstream by `viz::
/// spring3d_element`, its single production constructor) and mirrors
/// `OrbitCanvas`'s drag-to-`Message::Orbit` discipline, plus
/// wheel-to-`Message::Zoom`.
pub(crate) struct SpringShader {
    pub uniforms: Vec<f32>,
    pub camera: [f32; 32],
    pub bg: [f32; 4],
}

impl shader::Program<Message> for SpringShader {
    type State = DragState;
    type Primitive = SpringPrimitive;

    fn update(
        &self,
        state: &mut DragState,
        event: &iced::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                state.last = cursor.position_in(bounds);
                None
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let pos = cursor.position_in(bounds)?;
                let last = state.last?;
                state.last = Some(pos);
                Some(Action::publish(Message::Orbit(
                    pos.x - last.x,
                    pos.y - last.y,
                )))
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | iced::Event::Mouse(mouse::Event::CursorLeft) => {
                state.last = None;
                None
            }
            iced::Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let normalized = match *delta {
                    mouse::ScrollDelta::Lines { y, .. } => y * 0.1,
                    mouse::ScrollDelta::Pixels { y, .. } => y * 0.002,
                };
                Some(Action::publish(Message::Zoom(normalized)))
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &DragState,
        _cursor: mouse::Cursor,
        _bounds: Rectangle,
    ) -> SpringPrimitive {
        SpringPrimitive {
            uniforms: self.uniforms.clone(),
            camera: self.camera,
            bg: self.bg,
        }
    }
}

/// One frame's packed data, cloned out of [`SpringShader`] by `draw` — the
/// `iced_wgpu::Primitive` half of the pair.
#[derive(Debug)]
pub(crate) struct SpringPrimitive {
    uniforms: Vec<f32>,
    camera: [f32; 32],
    bg: [f32; 4],
}

/// Concatenate an `f32` slice into little-endian bytes for a `wgpu` buffer
/// write — no `bytemuck` dependency needed for a one-off flat conversion.
fn floats_to_le_bytes(data: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(data.len() * 4);
    for f in data {
        bytes.extend_from_slice(&f.to_le_bytes());
    }
    bytes
}

impl shader::Primitive for SpringPrimitive {
    type Pipeline = SpringPipeline;

    fn prepare(
        &self,
        pipeline: &mut SpringPipeline,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        _viewport: &shader::Viewport,
    ) {
        let mut camera_floats = Vec::with_capacity(CAMERA_UNIFORM_FLOATS);
        camera_floats.extend_from_slice(&self.camera);
        camera_floats.extend_from_slice(&self.bg);
        queue.write_buffer(
            &pipeline.camera_buffer,
            0,
            &floats_to_le_bytes(&camera_floats),
        );
        queue.write_buffer(
            &pipeline.scene_buffer,
            0,
            &floats_to_le_bytes(&self.uniforms),
        );
    }

    fn draw(&self, pipeline: &SpringPipeline, render_pass: &mut wgpu::RenderPass<'_>) -> bool {
        render_pass.set_pipeline(&pipeline.render_pipeline);
        render_pass.set_bind_group(0, &pipeline.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
        true
    }
}

/// `camera`(32) + `bg`(4) floats, in that order — the layout `Camera`'s WGSL
/// struct (`view_proj: mat4x4, inv_view_proj: mat4x4, bg: vec4`) expects.
const CAMERA_UNIFORM_FLOATS: usize = 32 + 4;

/// `4` header floats + `MAX_PARTS` part slots + `MAX_CUTS` cut slots — the
/// exact float count `sdf::scene_uniforms` always returns (see its doc);
/// derived from the shared `sdf.rs` constants, never a separate literal.
const SCENE_STORAGE_FLOATS: usize =
    4 + sdf::MAX_PARTS * sdf::FLOATS_PER_PART + sdf::MAX_CUTS * sdf::FLOATS_PER_CUT;

/// The shared GPU state for every [`SpringPrimitive`] instance: the render
/// pipeline, its fixed-size uniform/storage buffers (sized once from the
/// shared `sdf.rs` constants — they never need to grow), and the bind group
/// tying them to the shader's `@group(0)` bindings.
pub(crate) struct SpringPipeline {
    render_pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    scene_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl shader::Pipeline for SpringPipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("spring-shader-module"),
            source: wgpu::ShaderSource::Wgsl(instantiate_wgsl().into()),
        });

        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("spring-shader-camera-buffer"),
            size: (CAMERA_UNIFORM_FLOATS * 4) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let scene_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("spring-shader-scene-buffer"),
            size: (SCENE_STORAGE_FLOATS * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("spring-shader-bind-group-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("spring-shader-bind-group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: scene_buffer.as_entire_binding(),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("spring-shader-pipeline-layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("spring-shader-render-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        Self {
            render_pipeline,
            camera_buffer,
            scene_buffer,
            bind_group,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::mouse::{Button, Cursor, Event as MouseEvent, ScrollDelta};
    use iced::widget::shader::Program;
    use iced::{Event, Point, Size};

    fn bounds() -> Rectangle {
        Rectangle::new(Point::new(0.0, 0.0), Size::new(200.0, 150.0))
    }

    fn shader_fixture() -> SpringShader {
        SpringShader {
            uniforms: vec![0.0; SCENE_STORAGE_FLOATS],
            camera: [0.0; 32],
            bg: [0.1, 0.2, 0.3, 1.0],
        }
    }

    /// Unwrap a published `Action` down to its `Message`. `Action::publish`
    /// leaves its OWN `redraw_request` at the default `Wait` (verified
    /// against `iced_widget::shader::Action::publish`'s source — its doc's
    /// "publishing a message always produces a redraw" guarantee comes from
    /// `Shell::publish` itself reacting to the returned message, a
    /// runtime-level effect this unit test has no `Shell` to observe), so
    /// this helper only asserts the brief's actual per-event-mapping
    /// contract: every arm below reaches the app via `publish` (never
    /// `capture`), i.e. a `Some(message)` is always present alongside it.
    fn expect_published(action: Option<Action<Message>>) -> Message {
        let (message, _redraw, _status) = action.expect("expected a published Action").into_inner();
        message.expect("expected a published Message")
    }

    // ------------------------------------------------------------------
    // Drag lifecycle — mirrors OrbitCanvas::update's discipline exactly.
    // ------------------------------------------------------------------

    #[test]
    fn button_press_in_bounds_starts_a_drag_without_publishing() {
        let program = shader_fixture();
        let mut state = DragState::default();
        let action = program.update(
            &mut state,
            &Event::Mouse(MouseEvent::ButtonPressed(Button::Left)),
            bounds(),
            Cursor::Available(Point::new(10.0, 20.0)),
        );
        assert!(action.is_none());
        assert_eq!(state.last, Some(Point::new(10.0, 20.0)));
    }

    #[test]
    fn button_press_outside_bounds_does_not_start_a_drag() {
        let program = shader_fixture();
        let mut state = DragState::default();
        let action = program.update(
            &mut state,
            &Event::Mouse(MouseEvent::ButtonPressed(Button::Left)),
            bounds(),
            Cursor::Available(Point::new(-5.0, -5.0)),
        );
        assert!(action.is_none());
        assert_eq!(state.last, None);
    }

    #[test]
    fn cursor_moved_during_a_drag_publishes_the_raw_delta_and_updates_last() {
        let program = shader_fixture();
        let mut state = DragState {
            last: Some(Point::new(10.0, 10.0)),
        };
        let action = program.update(
            &mut state,
            &Event::Mouse(MouseEvent::CursorMoved {
                position: Point::new(14.0, 6.0),
            }),
            bounds(),
            Cursor::Available(Point::new(14.0, 6.0)),
        );
        match expect_published(action) {
            Message::Orbit(dx, dy) => {
                assert!((dx - 4.0).abs() < 1e-6, "dx={dx}");
                assert!((dy - (-4.0)).abs() < 1e-6, "dy={dy}");
            }
            other => panic!("expected Message::Orbit, got {other:?}"),
        }
        assert_eq!(state.last, Some(Point::new(14.0, 6.0)));
    }

    /// Regression shape mirrored from `orbit_message_composes_across_repeated_updates`
    /// (app.rs): two sequential small moves must publish two deltas that sum
    /// to the one big delta — the drag tracks the LAST position, not a
    /// stale base, so coalesced drag events never drop intermediate steps.
    #[test]
    fn repeated_cursor_moves_publish_deltas_that_compose_additively() {
        let program = shader_fixture();
        let mut state = DragState {
            last: Some(Point::new(0.0, 0.0)),
        };
        let first = program.update(
            &mut state,
            &Event::Mouse(MouseEvent::CursorMoved {
                position: Point::new(3.0, 2.0),
            }),
            bounds(),
            Cursor::Available(Point::new(3.0, 2.0)),
        );
        let second = program.update(
            &mut state,
            &Event::Mouse(MouseEvent::CursorMoved {
                position: Point::new(7.0, 5.0),
            }),
            bounds(),
            Cursor::Available(Point::new(7.0, 5.0)),
        );
        let (dx1, dy1) = match expect_published(first) {
            Message::Orbit(dx, dy) => (dx, dy),
            other => panic!("expected Message::Orbit, got {other:?}"),
        };
        let (dx2, dy2) = match expect_published(second) {
            Message::Orbit(dx, dy) => (dx, dy),
            other => panic!("expected Message::Orbit, got {other:?}"),
        };
        assert!(((dx1 + dx2) - 7.0).abs() < 1e-6);
        assert!(((dy1 + dy2) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn cursor_moved_without_a_prior_press_publishes_nothing() {
        let program = shader_fixture();
        let mut state = DragState::default();
        let action = program.update(
            &mut state,
            &Event::Mouse(MouseEvent::CursorMoved {
                position: Point::new(1.0, 1.0),
            }),
            bounds(),
            Cursor::Available(Point::new(1.0, 1.0)),
        );
        assert!(action.is_none());
    }

    #[test]
    fn button_released_and_cursor_left_both_end_the_drag() {
        let program = shader_fixture();
        for event in [
            Event::Mouse(MouseEvent::ButtonReleased(Button::Left)),
            Event::Mouse(MouseEvent::CursorLeft),
        ] {
            let mut state = DragState {
                last: Some(Point::new(5.0, 5.0)),
            };
            let action = program.update(&mut state, &event, bounds(), Cursor::Unavailable);
            assert!(action.is_none());
            assert_eq!(state.last, None);
        }
    }

    // ------------------------------------------------------------------
    // Wheel zoom — no OrbitCanvas precedent (new in this task); gated on
    // the cursor being over the widget, symmetric with the press-in-bounds
    // discipline above.
    // ------------------------------------------------------------------

    #[test]
    fn wheel_scrolled_lines_publishes_a_scaled_zoom_delta() {
        let program = shader_fixture();
        let mut state = DragState::default();
        let action = program.update(
            &mut state,
            &Event::Mouse(MouseEvent::WheelScrolled {
                delta: ScrollDelta::Lines { x: 0.0, y: 2.0 },
            }),
            bounds(),
            Cursor::Available(Point::new(50.0, 50.0)),
        );
        match expect_published(action) {
            Message::Zoom(delta) => assert!((delta - 0.2).abs() < 1e-6, "delta={delta}"),
            other => panic!("expected Message::Zoom, got {other:?}"),
        }
    }

    #[test]
    fn wheel_scrolled_pixels_publishes_a_scaled_zoom_delta() {
        let program = shader_fixture();
        let mut state = DragState::default();
        let action = program.update(
            &mut state,
            &Event::Mouse(MouseEvent::WheelScrolled {
                delta: ScrollDelta::Pixels { x: 0.0, y: 100.0 },
            }),
            bounds(),
            Cursor::Available(Point::new(50.0, 50.0)),
        );
        match expect_published(action) {
            Message::Zoom(delta) => assert!((delta - 0.2).abs() < 1e-6, "delta={delta}"),
            other => panic!("expected Message::Zoom, got {other:?}"),
        }
    }

    #[test]
    fn wheel_scrolled_outside_bounds_publishes_nothing() {
        let program = shader_fixture();
        let mut state = DragState::default();
        let action = program.update(
            &mut state,
            &Event::Mouse(MouseEvent::WheelScrolled {
                delta: ScrollDelta::Lines { x: 0.0, y: 2.0 },
            }),
            bounds(),
            Cursor::Unavailable,
        );
        assert!(action.is_none());
    }

    // ------------------------------------------------------------------
    // draw()
    // ------------------------------------------------------------------

    #[test]
    fn draw_clones_the_packed_uniforms_camera_and_background() {
        let program = shader_fixture();
        let state = DragState::default();
        let primitive = program.draw(&state, Cursor::Unavailable, bounds());
        assert_eq!(primitive.uniforms, program.uniforms);
        assert_eq!(primitive.camera, program.camera);
        assert_eq!(primitive.bg, program.bg);
    }

    // ------------------------------------------------------------------
    // instantiate_wgsl — the mirror-drift gates (no GPU).
    // ------------------------------------------------------------------

    #[test]
    fn instantiate_wgsl_leaves_no_placeholders_unsubstituted() {
        let src = instantiate_wgsl();
        assert!(
            !src.contains("{{"),
            "unsubstituted {{PLACEHOLDER}} remains in:\n{src}"
        );
    }

    #[test]
    fn instantiate_wgsl_pins_the_shared_drift_constants() {
        let src = instantiate_wgsl();
        assert!(
            src.contains(&sdf::MARCH_MAX_STEPS.to_string()),
            "MARCH_MAX_STEPS not found in instantiated WGSL"
        );
        assert!(
            src.contains(&sdf::MAX_PARTS.to_string()),
            "MAX_PARTS not found in instantiated WGSL"
        );
    }

    /// The strongest no-GPU shader gate: parse the instantiated WGSL with
    /// naga's own WGSL front-end and run it through naga's validator — this
    /// catches type errors, undeclared bindings, and structural mistakes
    /// (e.g. a wrongly-transcribed slot index producing an out-of-range constant
    /// expression) without ever touching a GPU adapter.
    #[test]
    fn instantiate_wgsl_parses_and_validates_via_naga() {
        let src = instantiate_wgsl();
        let module = naga::front::wgsl::parse_str(&src)
            .unwrap_or_else(|e| panic!("WGSL parse error:\n{}", e.emit_to_string(&src)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("WGSL validation error:\n{}", e.emit_to_string(&src)));
    }
}
