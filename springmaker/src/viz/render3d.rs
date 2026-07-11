//! Humble 3D renderer: SceneData + Orbit → plotters build_cartesian_3d →
//! RGBA bitmap (the shipped 760×300 pipeline shape). Axes and mesh are
//! deliberately suppressed — the scene reads as a clean engineering sketch.

use super::{finite3, scene_extent, Orbit, SceneData, SceneExtent, SceneRole};
use crate::app::C;
use crate::plot::{ensure_font, rgb_to_rgba, to_rgb, CHART_H, CHART_W};
use plotters::prelude::*;

fn role_color(role: SceneRole) -> RGBColor {
    match role {
        SceneRole::Wire => to_rgb(C::ACCENT),
        SceneRole::Member => to_rgb(C::MUTED),
        SceneRole::Detail => to_rgb(C::WARN),
    }
}

/// Frame a scene's bounding extent into plotters axis ranges. plotters'
/// `Cartesian3d` normalizes EVERY axis onto the same logical length
/// (vendored plotters-0.3.7 `cartesian3d.rs:50-51,107-113`), so handing x/z
/// a `±radial` range while y gets a separately-padded data span would
/// squash the aspect ratio onto a cube (the standard fixture: 23mm radial
/// vs ~60mm axial rendered squashed 2.6×). Framing all three axes with ONE
/// physical half-span `s = max(padded radial, padded y half-span)` keeps
/// millimetres-per-unit equal on every axis (spec: "Coordinates stay in
/// true mm (aspect-honest)"); x/z stay symmetric about the origin, y stays
/// centered on the data's own midpoint.
///
/// `None` when a computed bound OR SPAN is non-finite — e.g. a near-
/// `f64::MAX` y span overflows the padding arithmetic to infinity, and
/// plotters accepts an infinite range without complaint, silently rendering
/// garbage pixels instead of panicking (the latent twin of
/// `plot::render::render_chart`'s guarded headroom overflow). The spans are
/// checked separately because all four bounds can be individually finite
/// (±~9e307) while `hi − lo` still overflows — plotters then maps every
/// point to 0: a blank canvas instead of the placeholder.
pub(crate) fn frame_ranges(extent: &SceneExtent) -> Option<((f64, f64), (f64, f64))> {
    let r = extent.radial * 1.15;
    let y_pad = ((extent.y_max - extent.y_min) * 0.05).max(1e-9);
    let y_mid = (extent.y_max + extent.y_min) / 2.0;
    let y_half = (extent.y_max - extent.y_min) / 2.0 + y_pad;
    let s = r.max(y_half);
    let (x_lo, x_hi) = (-s, s);
    let (y_lo, y_hi) = (y_mid - s, y_mid + s);
    [x_lo, x_hi, y_lo, y_hi, x_hi - x_lo, y_hi - y_lo]
        .into_iter()
        .all(f64::is_finite)
        .then_some(((x_lo, x_hi), (y_lo, y_hi)))
}

/// Render the scene under the given orbit. `None` iff the scene has no
/// finite extent, or the framed ranges overflow (degenerate — the caller
/// shows the placeholder).
pub fn render_scene(scene: &SceneData, orbit: Orbit) -> Option<Vec<u8>> {
    let extent = scene_extent(scene)?;
    let ((x_lo, x_hi), (y_lo, y_hi)) = frame_ranges(&extent)?;

    ensure_font();
    let mut rgb = vec![0u8; (CHART_W * CHART_H * 3) as usize];
    {
        let root = BitMapBackend::with_buffer(&mut rgb, (CHART_W, CHART_H)).into_drawing_area();
        root.fill(&to_rgb(C::PANEL)).expect("fill scene background");
        let mut chart = ChartBuilder::on(&root)
            .margin(8)
            .build_cartesian_3d(x_lo..x_hi, y_lo..y_hi, x_lo..x_hi)
            .expect("scene axes");
        chart.with_projection(|mut pb| {
            pb.yaw = f64::from(orbit.yaw);
            pb.pitch = f64::from(orbit.pitch);
            pb.scale = 0.9;
            pb.into_matrix()
        });
        // No configure_axes(): a bare projected scene, no grid/tick noise.

        for line in &scene.polylines {
            let style = ShapeStyle {
                color: role_color(line.role).to_rgba(),
                filled: false,
                stroke_width: line.stroke_px,
            };
            let pts: Vec<(f64, f64, f64)> = line
                .points
                .iter()
                .copied()
                .filter(|&p| finite3(p))
                .collect();
            chart
                .draw_series(LineSeries::new(pts, style))
                .expect("scene polyline");
        }
        root.present().expect("present scene bitmap");
    }

    Some(rgb_to_rgba(&rgb))
}

#[cfg(test)]
mod tests {
    use super::super::helix;
    use super::*;
    use approx::assert_relative_eq;

    fn test_scene() -> SceneData {
        SceneData {
            polylines: vec![super::super::Polyline3 {
                points: helix(|_| 10.0, |t| t * 50.0, 6.0, 32),
                role: SceneRole::Wire,
                stroke_px: 3,
            }],
        }
    }

    #[test]
    fn render_scene_none_for_degenerate() {
        assert!(render_scene(&SceneData { polylines: vec![] }, Orbit::default()).is_none());
    }

    #[test]
    fn render_scene_draws_content_over_background() {
        let pixels = render_scene(&test_scene(), Orbit::default()).unwrap();
        assert_eq!(pixels.len(), (CHART_W * CHART_H * 4) as usize);
        assert!(pixels.chunks_exact(4).all(|p| p[3] == 255));
        let bg = to_rgb(crate::app::C::PANEL);
        assert!(
            pixels
                .chunks_exact(4)
                .any(|p| p[0] != bg.0 || p[1] != bg.1 || p[2] != bg.2),
            "the helix must draw over the panel background"
        );
    }

    #[test]
    fn orbit_changes_the_rendered_image() {
        // The projection is load-bearing: two different orbits must not produce
        // identical bitmaps (this is the drag-to-orbit contract at the pixel level).
        let a = render_scene(
            &test_scene(),
            Orbit {
                yaw: 0.2,
                pitch: 0.1,
            },
        )
        .unwrap();
        let b = render_scene(
            &test_scene(),
            Orbit {
                yaw: 1.2,
                pitch: 0.6,
            },
        )
        .unwrap();
        assert_ne!(a, b, "different orbit angles must change the projection");
    }

    #[test]
    fn non_finite_points_are_filtered() {
        let mut scene = test_scene();
        scene.polylines[0].points.push((f64::NAN, 10.0, 0.0));
        let clean = render_scene(&test_scene(), Orbit::default()).unwrap();
        let dirty = render_scene(&scene, Orbit::default()).unwrap();
        assert_eq!(
            clean, dirty,
            "non-finite points must be filtered, not drawn"
        );
    }

    #[test]
    fn frame_ranges_tall_spring_y_drives_the_shared_half_span() {
        // 23mm radial, 60mm axial — the standard fixture the aspect bug was
        // proven against (per-axis framing would render this 2.6x squashed).
        let extent = SceneExtent {
            radial: 23.0,
            y_min: 0.0,
            y_max: 60.0,
        };
        let ((x_lo, x_hi), (y_lo, y_hi)) = frame_ranges(&extent).unwrap();
        // y's padded half-span (33.0) exceeds the padded radial (26.45), so
        // it drives s — x/z get the SAME half-span as y, not `±radial`.
        assert_relative_eq!(x_lo, -33.0, max_relative = 1e-12);
        assert_relative_eq!(x_hi, 33.0, max_relative = 1e-12);
        assert_relative_eq!(y_lo, -3.0, max_relative = 1e-12); // y_min - pad(3.0)
        assert_relative_eq!(y_hi, 63.0, max_relative = 1e-12); // y_max + pad(3.0)
        assert_relative_eq!(
            x_hi - x_lo,
            y_hi - y_lo,
            max_relative = 1e-12,
            epsilon = 1e-9
        ); // equal mm-per-unit on every axis
    }

    #[test]
    fn frame_ranges_wide_spring_radial_drives_the_shared_half_span() {
        // 50mm radial, 10mm axial — the opposite regime: radial dominates.
        let extent = SceneExtent {
            radial: 50.0,
            y_min: 0.0,
            y_max: 10.0,
        };
        let ((x_lo, x_hi), (y_lo, y_hi)) = frame_ranges(&extent).unwrap();
        assert_relative_eq!(x_lo, -57.5, max_relative = 1e-12); // radial * 1.15
        assert_relative_eq!(x_hi, 57.5, max_relative = 1e-12);
        assert_relative_eq!(
            x_hi - x_lo,
            y_hi - y_lo,
            max_relative = 1e-12,
            epsilon = 1e-9
        );
    }

    #[test]
    fn frame_ranges_centers_y_on_the_data_midpoint() {
        // y spans [10, 50] — midpoint 30, not 0 — must stay centered even
        // though the shared half-span is driven by the (much larger) radial.
        let extent = SceneExtent {
            radial: 100.0,
            y_min: 10.0,
            y_max: 50.0,
        };
        let ((_, _), (y_lo, y_hi)) = frame_ranges(&extent).unwrap();
        assert_relative_eq!((y_lo + y_hi) / 2.0, 30.0, max_relative = 1e-9);
    }

    #[test]
    fn frame_ranges_none_for_overflowing_y_span() {
        // y_max - y_min alone overflows f64 (±~1e308 exceeds f64::MAX) —
        // frame_ranges must bail, never hand plotters an infinite bound.
        let extent = SceneExtent {
            radial: 1.0,
            y_min: -1e308,
            y_max: 1e308,
        };
        assert!(frame_ranges(&extent).is_none());
    }

    #[test]
    fn frame_ranges_none_for_finite_bounds_with_overflowing_span() {
        // All four BOUNDS are finite (±9.35e307) but the SPAN x_hi − x_lo
        // (= 2s ≈ 1.87e308) overflows f64 — plotters would accept the
        // ranges and map every point to 0: a silently blank canvas instead
        // of the placeholder. The span check must return None.
        let extent = SceneExtent {
            radial: 1.0,
            y_min: -8.5e307,
            y_max: 8.5e307,
        };
        assert!(frame_ranges(&extent).is_none());
    }

    #[test]
    fn frame_ranges_none_for_overflowing_radial() {
        let extent = SceneExtent {
            radial: f64::MAX,
            y_min: 0.0,
            y_max: 1.0,
        };
        assert!(frame_ranges(&extent).is_none());
    }

    #[test]
    fn render_scene_none_for_near_max_extent() {
        // All-finite points whose y span alone overflows f64 — the latent
        // twin of `render_chart_clamps_headroom_for_near_max_extent`, except
        // 3D bails to None rather than clamping (see `frame_ranges` doc).
        let scene = SceneData {
            polylines: vec![super::super::Polyline3 {
                points: vec![(1.0, -1e308, 0.0), (1.0, 1e308, 0.0)],
                role: SceneRole::Wire,
                stroke_px: 1,
            }],
        };
        assert!(render_scene(&scene, Orbit::default()).is_none());
    }
}
