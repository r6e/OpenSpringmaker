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

#[cfg(test)]
use super::zoom_step;
use super::{camera_uniforms, fallback_camera, sdf, wheel_lines, Orbit};

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
        .replace(
            "{{HELIX_WINDING_SUBDIVISIONS}}",
            &sdf::HELIX_WINDING_SUBDIVISIONS.to_string(),
        )
}

/// Per-drag ephemeral canvas state — mirrors `canvas3d::DragState` exactly
/// (last cursor position, `None` when no drag is in progress).
#[derive(Debug, Default, PartialEq)]
pub(crate) struct DragState {
    last: Option<Point>,
}

/// The `shader::Program` for the shaded 3D view: carries the already-packed
/// scene/background data plus the RAW camera inputs (computed upstream by
/// `viz::spring3d_element`, its single production constructor) and mirrors
/// `OrbitCanvas`'s drag-to-`Message::Orbit` discipline, plus
/// wheel-to-`Message::Zoom`.
///
/// **No pre-packed camera field (review finding 1 fix).** The camera is
/// built fresh in `Program::draw` (below) every frame from `extent_mm`/`y_mid_mm`/
/// `orbit`/`zoom` here PLUS the widget's live layout `Rectangle` — the only
/// input `camera_uniforms` needs that isn't already known at `view()` time.
/// Baking a camera here (at a nominal aspect) would drift from the panel's
/// actual on-screen aspect ratio whenever it differs from that nominal
/// value, which is every window width other than the one nominal value was
/// tuned for.
pub(crate) struct SpringShader {
    pub uniforms: Vec<f32>,
    pub extent_mm: f64,
    pub y_mid_mm: f64,
    pub orbit: Orbit,
    pub zoom: f32,
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
                // Publish the RAW line-equivalent count — `zoom_step`'s own
                // `ZOOM_SENSITIVITY` is the single rate applied to it
                // (review finding 2: a second scaling factor here used to
                // compound with that rate). `Pixels` converts to the same
                // line-equivalent unit via `WHEEL_PIXELS_PER_LINE` (shared with
                // the 2D diagram canvas through `wheel_lines`).
                let lines = wheel_lines(delta);
                // `.and_capture()` (review finding 3): without it the outer
                // results-panel `scrollable` ALSO scrolls on every
                // wheel-zoom tick, since an `Ignored` status bubbles the
                // event past this widget.
                Some(Action::publish(Message::Zoom(lines)).and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &DragState,
        _cursor: mouse::Cursor,
        bounds: Rectangle,
    ) -> SpringPrimitive {
        // The widget's LIVE on-screen aspect ratio (review finding 1) — not
        // a nominal stand-in baked in at `view()` time — so the fitted
        // camera tracks the panel's actual shape every frame, including
        // across a live window resize.
        let aspect = bounds.width / bounds.height;
        // `spring3d_element`'s representability probe already rejected a
        // persistently-hostile extent/y_mid/zoom/orbit before this widget
        // was ever built, so `None` here is not expected in production —
        // but `draw` cannot itself return an `Option` (the `shader::
        // Program` trait requires a concrete `Primitive` every frame), so a
        // known-safe fallback camera (review finding 5) stands in rather
        // than an `unwrap` panic on the unreachable-in-practice case.
        let camera = camera_uniforms(self.extent_mm, self.y_mid_mm, self.orbit, self.zoom, aspect)
            .unwrap_or_else(fallback_camera);
        SpringPrimitive {
            uniforms: self.uniforms.clone(),
            camera,
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
        // The sRGB-encode flag rides in the camera uniform (wave-2 V1/V2):
        // the PIPELINE learned the real target format in `Pipeline::new`;
        // the primitive itself carries no format knowledge.
        let camera_floats = camera_buffer_floats(&self.camera, self.bg, pipeline.needs_encode);
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

/// `camera`(32) + `bg`(4) + `flags`(4) floats, in that order — the layout
/// `Camera`'s WGSL struct (`view_proj: mat4x4, inv_view_proj: mat4x4,
/// bg: vec4, flags: vec4`) expects. `flags.x` carries the runtime
/// sRGB-encode flag ([`needs_srgb_encode`], wave-2 V1/V2); `.yzw` pad the
/// slot to a full `vec4` (WGSL uniform structs align vector members to 16
/// bytes, and a lone trailing `f32` would leave the CPU/GPU sizes
/// disagreeing).
const CAMERA_UNIFORM_FLOATS: usize = 32 + 4 + 4;

/// Whether the fragment shader must apply the sRGB OETF itself (wave-2
/// V1/V2 fix): TRUE for a non-sRGB render-target format, whose hardware
/// stores shader output RAW — without the encode, the linear-light values
/// this pipeline composes in display ~2.2x too dark (the user-reported
/// black spring on a black background: `iced_wgpu`'s compositor PREFERS an
/// sRGB swapchain format under gamma correction, but falls back to
/// whatever the surface offers first, and the user's Metal surface handed
/// the pipeline a non-sRGB format). FALSE for an sRGB-format target, which
/// encodes in hardware on store — encoding in the shader too would
/// double-encode and wash the image out. With the flag, BOTH format
/// classes render identically.
fn needs_srgb_encode(format: wgpu::TextureFormat) -> bool {
    !format.is_srgb()
}

/// Assemble the camera uniform buffer's exact float layout
/// ([`CAMERA_UNIFORM_FLOATS`]): `camera`(32) + `bg`(4) + `flags`(4), with
/// `flags.x` the [`needs_srgb_encode`] verdict as `1.0`/`0.0`. Pure — the
/// headless-pinnable half of `SpringPrimitive::prepare`'s buffer write.
fn camera_buffer_floats(camera: &[f32; 32], bg: [f32; 4], needs_encode: bool) -> Vec<f32> {
    let mut floats = Vec::with_capacity(CAMERA_UNIFORM_FLOATS);
    floats.extend_from_slice(camera);
    floats.extend_from_slice(&bg);
    floats.extend_from_slice(&[f32::from(needs_encode), 0.0, 0.0, 0.0]);
    floats
}

/// The exact float count `sdf::scene_uniforms` always returns (see its
/// doc) — aliased from `sdf.rs`'s own hoisted `SCENE_UNIFORM_FLOATS`
/// (simplifier F3) rather than re-derived, so this buffer's size can never
/// silently drift from the layout `scene_uniforms`/`unpack_scene` share.
const SCENE_STORAGE_FLOATS: usize = sdf::SCENE_UNIFORM_FLOATS;

/// The shared GPU state for every [`SpringPrimitive`] instance: the render
/// pipeline, its fixed-size uniform/storage buffers (sized once from the
/// shared `sdf.rs` constants — they never need to grow), and the bind group
/// tying them to the shader's `@group(0)` bindings.
///
/// **One shaded 3D widget per frame (review F5).** `iced` caches exactly one
/// `SpringPipeline` per `Primitive` TYPE (not per widget instance), so its
/// single camera/scene buffer pair is implicitly shared — fine today since
/// the results panel shows at most one `Spring3d` slot at a time (`app::
/// VisualMode` is a single, app-wide choice), but a future layout showing
/// two shaded widgets simultaneously (e.g. a side-by-side comparison view)
/// would need per-instance buffers, not this shared pair.
pub(crate) struct SpringPipeline {
    render_pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    scene_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Whether the fragment shader must apply the sRGB OETF itself —
    /// derived once from the REAL target format `Pipeline::new` receives
    /// ([`needs_srgb_encode`], wave-2 V1/V2) and forwarded to the shader
    /// through the camera uniform's `flags.x` on every `prepare`.
    needs_encode: bool,
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
            needs_encode: needs_srgb_encode(format),
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
            extent_mm: 80.0,
            y_mid_mm: 40.0,
            orbit: Orbit::default(),
            zoom: 1.0,
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

    /// Like [`expect_published`] but for the wheel-zoom case (review finding
    /// 3), which — unlike the drag arms above — must ALSO capture the event
    /// (`Status::Captured`) so the outer results-panel `scrollable` doesn't
    /// ALSO scroll on every wheel-zoom tick. Asserts the status explicitly
    /// rather than discarding it.
    fn expect_published_and_captured(action: Option<Action<Message>>) -> Message {
        let (message, _redraw, status) = action.expect("expected a published Action").into_inner();
        assert_eq!(
            status,
            iced::event::Status::Captured,
            "wheel-zoom must capture the event, or the outer scrollable also scrolls mid-zoom"
        );
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
    fn wheel_scrolled_lines_publishes_the_raw_line_count_and_captures() {
        // Review finding 2: the widget must publish the RAW line-equivalent
        // count — `zoom_step`'s own `ZOOM_SENSITIVITY` is the ONLY rate
        // applied. A pre-normalizing ×0.1 here (the pre-fix shape) would
        // compound with that rate, needing ~139 notches for a 1x -> 4x zoom
        // instead of the documented ~15.
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
        match expect_published_and_captured(action) {
            Message::Zoom(delta) => assert!((delta - 2.0).abs() < 1e-6, "delta={delta}"),
            other => panic!("expected Message::Zoom, got {other:?}"),
        }
    }

    #[test]
    fn wheel_scrolled_pixels_publishes_the_line_equivalent_count_and_captures() {
        let program = shader_fixture();
        let mut state = DragState::default();
        let action = program.update(
            &mut state,
            &Event::Mouse(MouseEvent::WheelScrolled {
                delta: ScrollDelta::Pixels { x: 0.0, y: 32.0 },
            }),
            bounds(),
            Cursor::Available(Point::new(50.0, 50.0)),
        );
        match expect_published_and_captured(action) {
            // 32px / WHEEL_PIXELS_PER_LINE(16) = 2.0 lines — the same
            // line-equivalent unit `ScrollDelta::Lines` reports natively.
            Message::Zoom(delta) => assert!((delta - 2.0).abs() < 1e-6, "delta={delta}"),
            other => panic!("expected Message::Zoom, got {other:?}"),
        }
    }

    #[test]
    fn wheel_zoom_composes_through_zoom_step_to_the_documented_per_line_rate() {
        // Review finding 2 (composed-pipeline regression): dispatch what the
        // WIDGET actually publishes for a single-line scroll tick straight
        // through `zoom_step` (the app-level accumulator) and confirm the
        // resulting change matches `ZOOM_SENSITIVITY`'s documented ~10% per
        // line — not the pre-fix ~1% (two compounded ×0.1 factors).
        let program = shader_fixture();
        let mut state = DragState::default();
        let action = program.update(
            &mut state,
            &Event::Mouse(MouseEvent::WheelScrolled {
                delta: ScrollDelta::Lines { x: 0.0, y: 1.0 },
            }),
            bounds(),
            Cursor::Available(Point::new(50.0, 50.0)),
        );
        let published = match expect_published_and_captured(action) {
            Message::Zoom(delta) => delta,
            other => panic!("expected Message::Zoom, got {other:?}"),
        };
        let zoomed = zoom_step(1.0, published);
        assert!(
            (zoomed - std::f32::consts::E.powf(0.1)).abs() < 1e-6,
            "single-line zoom_step result {zoomed}, expected e^0.1 ({})",
            std::f32::consts::E.powf(0.1)
        );
        assert!(
            (0.10..0.12).contains(&(zoomed - 1.0)),
            "a single wheel line must change zoom by ~10%, got {}%",
            (zoomed - 1.0) * 100.0
        );
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
    fn draw_clones_the_packed_uniforms_and_background() {
        let program = shader_fixture();
        let state = DragState::default();
        let primitive = program.draw(&state, Cursor::Unavailable, bounds());
        assert_eq!(primitive.uniforms, program.uniforms);
        assert_eq!(primitive.bg, program.bg);
    }

    /// Review finding 1: the camera must be rebuilt every frame from the
    /// widget's LIVE layout `Rectangle`, not pre-baked at a nominal aspect —
    /// two different bounds must therefore produce DIFFERENT camera arrays,
    /// each matching `camera_uniforms` called directly with that bounds'
    /// own aspect ratio (the pure fn is the oracle). Before this fix
    /// `SpringPrimitive::camera` was just `self.camera`, a value fixed at
    /// `view()` time — this test is RED against that shape (both bounds
    /// would report the identical stale camera).
    #[test]
    fn draw_builds_the_camera_from_the_live_bounds_aspect_every_frame() {
        let program = shader_fixture();
        let state = DragState::default();
        // Two very different aspect ratios: wide (4:3) and tall (3:4).
        let wide = Rectangle::new(Point::new(0.0, 0.0), Size::new(400.0, 300.0));
        let tall = Rectangle::new(Point::new(0.0, 0.0), Size::new(300.0, 400.0));

        let primitive_wide = program.draw(&state, Cursor::Unavailable, wide);
        let primitive_tall = program.draw(&state, Cursor::Unavailable, tall);

        assert_ne!(
            primitive_wide.camera, primitive_tall.camera,
            "two widget bounds with different aspect ratios must produce different \
             cameras — an unchanging camera means `draw` is still ignoring live bounds"
        );

        let expected_wide = camera_uniforms(
            program.extent_mm,
            program.y_mid_mm,
            program.orbit,
            program.zoom,
            wide.width / wide.height,
        )
        .expect("nominal fixture inputs always produce a camera");
        let expected_tall = camera_uniforms(
            program.extent_mm,
            program.y_mid_mm,
            program.orbit,
            program.zoom,
            tall.width / tall.height,
        )
        .expect("nominal fixture inputs always produce a camera");
        assert_eq!(primitive_wide.camera, expected_wide);
        assert_eq!(primitive_tall.camera, expected_tall);
    }

    /// Review finding 5: if the per-frame `camera_uniforms` call ever DOES
    /// return `None` (e.g. a persistently-hostile `zoom`/`orbit` that
    /// slipped past `spring3d_element`'s upstream representability probe —
    /// defense in depth, not a reachable production path today), `draw`
    /// must fall back to the documented safe default camera rather than
    /// panicking on an `unwrap` or propagating a poisoned value.
    #[test]
    fn draw_falls_back_to_the_safe_camera_when_camera_uniforms_returns_none() {
        let mut program = shader_fixture();
        program.zoom = f32::NAN; // camera_uniforms(..) is None for any aspect
        let state = DragState::default();
        let primitive = program.draw(&state, Cursor::Unavailable, bounds());
        assert_eq!(primitive.camera, fallback_camera());
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

    // ------------------------------------------------------------------
    // Wave-2 V1/V2: runtime sRGB-encode flag — the pipeline learns the real
    // target format in `Pipeline::new` and the shader applies the sRGB OETF
    // at its exit iff the target is NOT an sRGB-format texture. These pin
    // everything pinnable headless: the flag derivation, the packed camera
    // buffer layout, and the WGSL's OETF presence (the naga test validates
    // the shader itself). What they CANNOT pin: actual on-screen pixels —
    // that verification is human-only, on a real GPU.
    // ------------------------------------------------------------------

    #[test]
    fn needs_srgb_encode_matches_the_target_format_class() {
        // Non-sRGB targets store shader output raw — the shader must encode.
        assert!(needs_srgb_encode(wgpu::TextureFormat::Bgra8Unorm));
        assert!(needs_srgb_encode(wgpu::TextureFormat::Rgba8Unorm));
        // sRGB targets encode in hardware on store — the shader must NOT
        // (double-encoding would wash the image out).
        assert!(!needs_srgb_encode(wgpu::TextureFormat::Bgra8UnormSrgb));
        assert!(!needs_srgb_encode(wgpu::TextureFormat::Rgba8UnormSrgb));
    }

    #[test]
    fn camera_buffer_floats_packs_camera_bg_then_encode_flag() {
        let camera: [f32; 32] = std::array::from_fn(|i| i as f32);
        let bg = [0.1, 0.2, 0.3, 1.0];
        let floats = camera_buffer_floats(&camera, bg, true);
        assert_eq!(floats.len(), CAMERA_UNIFORM_FLOATS);
        assert_eq!(&floats[0..32], &camera);
        assert_eq!(&floats[32..36], &bg);
        assert_eq!(floats[36], 1.0, "flags.x must carry needs_encode");
        assert_eq!(&floats[37..40], &[0.0, 0.0, 0.0], "flags.yzw are padding");
        let floats_srgb = camera_buffer_floats(&camera, bg, false);
        assert_eq!(floats_srgb[36], 0.0);
        assert_eq!(floats_srgb.len(), CAMERA_UNIFORM_FLOATS);
    }

    #[test]
    fn instantiate_wgsl_carries_the_srgb_oetf() {
        let src = instantiate_wgsl();
        for needle in ["0.0031308", "12.92", "1.055", "flags"] {
            assert!(
                src.contains(needle),
                "instantiated WGSL is missing the sRGB-OETF marker {needle:?}"
            );
        }
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
        assert!(
            src.contains(&format!(
                "HELIX_WINDING_SUBDIVISIONS: u32 = {}u",
                sdf::HELIX_WINDING_SUBDIVISIONS
            )),
            "HELIX_WINDING_SUBDIVISIONS not substituted in instantiated WGSL"
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
        // `Capabilities::default()` (review F4), not `::all()`: the shader
        // uses none of the optional GPU capabilities (`::all()` gates —
        // push constants, multiview, ray tracing, etc.) the default set
        // excludes, and it validates cleanly, so the gate stays as close as
        // possible to the real `wgpu::Device` capabilities this shader
        // actually needs at runtime rather than a maximally permissive stand-in.
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("WGSL validation error:\n{}", e.emit_to_string(&src)));
    }
}
