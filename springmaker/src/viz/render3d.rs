//! Humble 3D renderer: SceneData + Orbit → plotters build_cartesian_3d →
//! RGBA bitmap (the shipped 760×300 pipeline shape). Axes and mesh are
//! deliberately suppressed — the scene reads as a clean engineering sketch.

use super::{scene_extent, Orbit, SceneData, SceneRole};
use crate::app::C;
use crate::plot::{ensure_font, to_rgb, CHART_H, CHART_W};
use plotters::prelude::*;

fn role_color(role: SceneRole) -> RGBColor {
    match role {
        SceneRole::Wire => to_rgb(C::ACCENT),
        SceneRole::Member => to_rgb(C::MUTED),
        SceneRole::Detail => to_rgb(C::WARN),
    }
}

/// Render the scene under the given orbit. `None` iff the scene has no
/// finite extent (degenerate — the caller shows the placeholder).
pub fn render_scene(scene: &SceneData, orbit: Orbit) -> Option<Vec<u8>> {
    let extent = scene_extent(scene)?;
    let r = extent.radial * 1.15;
    let y_pad = ((extent.y_max - extent.y_min) * 0.05).max(1e-9);
    let (y_lo, y_hi) = (extent.y_min - y_pad, extent.y_max + y_pad);

    ensure_font();
    let mut rgb = vec![0u8; (CHART_W * CHART_H * 3) as usize];
    {
        let root = BitMapBackend::with_buffer(&mut rgb, (CHART_W, CHART_H)).into_drawing_area();
        root.fill(&to_rgb(C::PANEL)).expect("fill scene background");
        let mut chart = ChartBuilder::on(&root)
            .margin(8)
            .build_cartesian_3d(-r..r, y_lo..y_hi, -r..r)
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
                .filter(|&(x, y, z)| x.is_finite() && y.is_finite() && z.is_finite())
                .collect();
            chart
                .draw_series(LineSeries::new(pts, style))
                .expect("scene polyline");
        }
        root.present().expect("present scene bitmap");
    }

    let mut rgba = Vec::with_capacity((CHART_W * CHART_H * 4) as usize);
    for px in rgb.chunks_exact(3) {
        rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
    }
    Some(rgba)
}

#[cfg(test)]
mod tests {
    use super::super::helix;
    use super::*;

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
}
