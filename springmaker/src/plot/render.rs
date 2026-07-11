//! Humble renderer: ChartData → plotters → RGBA bitmap (+ ChartMapping).
//! Rendering approach (bitmap backend, bundled font) documented in Cargo.toml.

use super::mapping::ChartMapping;
use super::{
    chart_extent, ensure_font, to_rgb, ChartData, LineRole, MarkerKind, CHART_H, CHART_W, MARGIN,
    X_LABEL_AREA, Y_LABEL_AREA,
};
use crate::app::C;
use plotters::prelude::*;

fn line_style(role: LineRole) -> ShapeStyle {
    let (color, width) = match role {
        LineRole::Primary => (to_rgb(C::ACCENT), 2),
        LineRole::Member => (to_rgb(C::MUTED), 1),
        LineRole::Envelope => (to_rgb(C::WARN), 2),
        LineRole::LoadLine => (to_rgb(C::MUTED), 1),
    };
    ShapeStyle {
        color: color.to_rgba(),
        filled: false,
        stroke_width: width,
    }
}

fn marker_style(kind: MarkerKind) -> ShapeStyle {
    let color = match kind {
        MarkerKind::Operating => to_rgb(C::WARN),
        MarkerKind::Limit => to_rgb(C::DANGER),
    };
    ShapeStyle {
        color: color.to_rgba(),
        filled: true,
        stroke_width: 0,
    }
}

/// Render `data` to an RGBA bitmap. `None` iff the chart has no finite
/// positive extent (plotters must never see a non-finite range).
pub fn render_chart(data: &ChartData) -> Option<(Vec<u8>, ChartMapping)> {
    let (x_raw, y_raw) = chart_extent(data)?;
    // Headroom, with the legacy floor so tiny ranges don't degenerate, and a
    // ceiling so a near-f64::MAX finite extent doesn't overflow to +Inf under
    // the 1.1x multiply (plotters' cartesian range must stay finite).
    let x_max = (x_raw * 1.1).clamp(1e-9, f64::MAX);
    let y_max = (y_raw * 1.1).clamp(1e-9, f64::MAX);

    ensure_font();
    let mut rgb = vec![0u8; (CHART_W * CHART_H * 3) as usize];
    {
        let root = BitMapBackend::with_buffer(&mut rgb, (CHART_W, CHART_H)).into_drawing_area();
        root.fill(&to_rgb(C::PANEL)).expect("fill chart background");
        let mut chart = ChartBuilder::on(&root)
            .margin(MARGIN as i32)
            .x_label_area_size(X_LABEL_AREA as i32)
            .y_label_area_size(Y_LABEL_AREA as i32)
            .build_cartesian_2d(0.0..x_max, 0.0..y_max)
            .expect("chart axes");

        chart
            .configure_mesh()
            .light_line_style(ShapeStyle {
                color: to_rgb(C::LINE).to_rgba(),
                filled: false,
                stroke_width: 1,
            })
            .bold_line_style(ShapeStyle {
                color: to_rgb(C::RAISED).to_rgba(),
                filled: false,
                stroke_width: 1,
            })
            .axis_style(ShapeStyle {
                color: to_rgb(C::TEXT).to_rgba(),
                filled: false,
                stroke_width: 1,
            })
            .label_style(("sans-serif", 14).into_font().color(&to_rgb(C::MUTED)))
            .axis_desc_style(("sans-serif", 15).into_font().color(&to_rgb(C::TEXT)))
            .x_desc(data.x_axis.label)
            .y_desc(data.y_axis.label)
            .draw()
            .expect("mesh");

        let mut any_named = false;
        for line in &data.lines {
            let style = line_style(line.role);
            let pts: Vec<(f64, f64)> = line
                .points
                .iter()
                .copied()
                .filter(|&(x, y)| super::plottable(x, y))
                .collect();
            let series = chart
                .draw_series(LineSeries::new(pts, style))
                .expect("line");
            if let Some(name) = &line.name {
                any_named = true;
                series
                    .label(name.clone())
                    .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 16, y)], style));
            }
        }
        if any_named {
            chart
                .configure_series_labels()
                .background_style(to_rgb(C::PANEL).mix(0.9))
                .border_style(to_rgb(C::LINE))
                .label_font(("sans-serif", 13).into_font().color(&to_rgb(C::TEXT)))
                .draw()
                .expect("legend");
        }

        chart
            .draw_series(
                data.markers
                    .iter()
                    .filter(|m| super::plottable(m.x, m.y))
                    .map(|m| Circle::new((m.x, m.y), 5, marker_style(m.kind))),
            )
            .expect("markers");
        root.present().expect("present chart bitmap");
    }

    let mut rgba = Vec::with_capacity((CHART_W * CHART_H * 4) as usize);
    for px in rgb.chunks_exact(3) {
        rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
    }
    Some((rgba, ChartMapping { x_max, y_max }))
}

#[cfg(test)]
mod tests {
    use super::super::{AxisMeta, Line, Marker};
    use super::*;
    use approx::assert_relative_eq;

    fn simple_data(named: bool) -> ChartData {
        ChartData {
            x_axis: AxisMeta {
                label: "deflection (mm)",
                symbol: "y",
                unit: "mm",
            },
            y_axis: AxisMeta {
                label: "load (N)",
                symbol: "F",
                unit: "N",
            },
            lines: vec![Line {
                points: vec![(0.0, 0.0), (15.0, 30.0)],
                role: LineRole::Primary,
                name: named.then(|| "Assembly".to_string()),
            }],
            markers: vec![Marker {
                x: 7.5,
                y: 15.0,
                kind: MarkerKind::Operating,
            }],
        }
    }

    /// Negative coordinates are never drawn: axes are always 0-based, and
    /// plotters CLAMPS out-of-range coordinates onto the plot-area edge
    /// (`Rect::truncate`), so an unfiltered negative point would draw a
    /// fabricated on-edge point — not merely clip (defense in depth —
    /// presenters emit non-negative values from engine-guarded designs).
    /// Pinned by buffer equality: injecting a negative line point and a
    /// negative marker must not change a single pixel. The injected line
    /// point is deliberately NOT collinear with the fixture line: a
    /// collinear one would clamp onto the existing origin vertex and could
    /// retrace the clean path pixel-for-pixel, leaving the line half of
    /// this pin vacuous.
    #[test]
    fn render_chart_filters_negative_coordinates() {
        let clean = simple_data(false);
        let (clean_pixels, _) = render_chart(&clean).unwrap();

        let mut dirty = simple_data(false);
        dirty.lines[0].points.push((-5.0, 8.0));
        dirty.markers.push(Marker {
            x: -3.0,
            y: 8.0,
            kind: MarkerKind::Operating,
        });
        let (dirty_pixels, _) = render_chart(&dirty).unwrap();

        assert_eq!(
            clean_pixels, dirty_pixels,
            "negative-coordinate points/markers must be filtered, not drawn"
        );
    }

    #[test]
    fn render_chart_none_for_degenerate_data() {
        let d = ChartData {
            x_axis: AxisMeta {
                label: "x",
                symbol: "x",
                unit: "",
            },
            y_axis: AxisMeta {
                label: "y",
                symbol: "y",
                unit: "",
            },
            lines: vec![],
            markers: vec![],
        };
        assert!(render_chart(&d).is_none());
    }

    #[test]
    fn render_chart_returns_headroom_mapping_and_opaque_buffer() {
        let (pixels, mapping) = render_chart(&simple_data(false)).unwrap();
        assert_eq!(pixels.len(), (CHART_W * CHART_H * 4) as usize);
        assert!(pixels.chunks_exact(4).all(|p| p[3] == 255));
        assert_relative_eq!(mapping.x_max, 15.0 * 1.1, max_relative = 1e-12);
        assert_relative_eq!(mapping.y_max, 30.0 * 1.1, max_relative = 1e-12);
    }

    #[test]
    fn render_chart_clamps_headroom_for_near_max_extent() {
        // x_raw * 1.1 overflows to +Inf when x_raw sits near f64::MAX; the
        // headroom multiply must clamp back to a finite value so plotters'
        // cartesian range construction never sees an infinite bound.
        let d = ChartData {
            x_axis: AxisMeta {
                label: "x",
                symbol: "x",
                unit: "",
            },
            y_axis: AxisMeta {
                label: "y",
                symbol: "y",
                unit: "",
            },
            lines: vec![Line {
                points: vec![(0.0, 0.0), (f64::MAX, f64::MAX)],
                role: LineRole::Primary,
                name: None,
            }],
            markers: vec![],
        };
        let (_, mapping) = render_chart(&d).expect("a huge finite extent must still render");
        assert!(mapping.x_max.is_finite());
        assert!(mapping.y_max.is_finite());
    }

    #[test]
    fn render_chart_rasterizes_labels_in_y_band() {
        let (pixels, _) = render_chart(&simple_data(true)).unwrap();
        let bg = to_rgb(crate::app::C::PANEL);
        let differs = |col: u32, row: u32| {
            let i = ((row * CHART_W + col) * 4) as usize;
            pixels[i] != bg.0 || pixels[i + 1] != bg.1 || pixels[i + 2] != bg.2
        };
        assert!((0..Y_LABEL_AREA).any(|c| (0..CHART_H).any(|r| differs(c, r))));
    }
}
