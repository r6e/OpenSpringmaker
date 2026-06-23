//! Live load-vs-deflection chart for the current design.
//!
//! The chart is drawn by `plotters` into an in-memory RGB bitmap and shown via
//! iced's `image` widget. This keeps the (well-established) `plotters` drawing
//! code without depending on any iced-coupled backend, so the chart is never
//! blocked by a `plotters-iced` version lag. Text is rendered with a bundled
//! font (`ab_glyph`), so there is no runtime system-font lookup.

use crate::app::{Message, C};
use plotters::prelude::*;
use plotters::style::{register_font, FontStyle};
use springcore::{SpringDesign, UnitSystem};

/// Fixed render resolution for the chart bitmap; iced scales it to fit the panel.
const CHART_W: u32 = 760;
const CHART_H: u32 = 300;
/// Width of the left y-axis label band. Plot geometry sits right of it
/// (margin + this), so this column range contains only axis text — relied on by
/// the render test to confirm the bundled font rasterizes.
const Y_LABEL_AREA_SIZE: u32 = 64;

/// Bundled font (DejaVu Sans, permissive license — see assets/LICENSE-DejaVu.txt),
/// registered under the "sans-serif" family so the chart needs no system fonts.
static FONT_BYTES: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");

/// Register the bundled font once (ab_glyph has no built-in fonts).
fn ensure_font() {
    use std::sync::Once;
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        // `InvalidFont` is not `Debug`, so map to a plain message.
        if register_font("sans-serif", FontStyle::Normal, FONT_BYTES).is_err() {
            panic!("the bundled DejaVu Sans font failed to register (invalid TTF)");
        }
    });
}

/// Convert an iced palette colour to a plotters `RGBColor`.
///
/// Each channel is multiplied by 255 and rounded; the result is clamped to
/// the valid `u8` range so values from the token struct (which are guaranteed
/// to be in [0, 1]) round-trip cleanly.
fn to_rgb(c: iced::Color) -> RGBColor {
    let ch = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    RGBColor(ch(c.r), ch(c.g), ch(c.b))
}

/// Force–deflection points (deflection x, force y) in the display unit system.
///
/// The spring rate is constant, so the line is linear; two endpoints suffice.
/// The maximum operating load sets the extent; if no operating loads are present,
/// the solid load is used as a fallback. Points with non-finite coordinates are
/// never emitted; callers should treat an all-zero result as degenerate.
pub fn force_deflection_series(design: &SpringDesign, units: UnitSystem) -> Vec<(f64, f64)> {
    let max_force_n = if design.load_points.is_empty() {
        design.at_solid.force.newtons()
    } else {
        design
            .load_points
            .iter()
            .map(|lp| lp.force.newtons())
            .fold(0.0_f64, f64::max)
    };

    let rate = design.rate.newtons_per_meter();

    // Guard: a non-positive or non-finite rate produces a non-finite deflection.
    // Return only the origin so `finite_positive_extent` sees no finite extent.
    if !rate.is_finite() || rate <= 0.0 {
        return vec![(0.0, 0.0)];
    }

    let max_defl_m = max_force_n / rate;

    // Guard: non-finite deflection means an extreme/degenerate design.
    if !max_defl_m.is_finite() || !max_force_n.is_finite() {
        return vec![(0.0, 0.0)];
    }

    let convert = |defl_m: f64, force_n: f64| match units {
        UnitSystem::Metric => (
            springcore::units::Length::from_meters(defl_m).millimeters(),
            force_n,
        ),
        UnitSystem::Us => (
            springcore::units::Length::from_meters(defl_m).inches(),
            springcore::units::Force::from_newtons(force_n).pounds_force(),
        ),
    };

    vec![convert(0.0, 0.0), convert(max_defl_m, max_force_n)]
}

/// Whether both coordinates of a point are finite (NaN/∞ points are never drawn).
fn finite_xy(x: f64, y: f64) -> bool {
    x.is_finite() && y.is_finite()
}

/// Returns `(x_max, y_max)` from `series`, considering only points where both
/// coordinates are finite. Returns `None` when the series has no finite points
/// or the resulting max is not finite and positive (i.e., the design is
/// degenerate and must not be passed to plotters).
pub fn finite_positive_extent(series: &[(f64, f64)]) -> Option<(f64, f64)> {
    let (x_max, y_max) = series.iter().filter(|(x, y)| finite_xy(*x, *y)).fold(
        (f64::NEG_INFINITY, f64::NEG_INFINITY),
        |(xm, ym), &(x, y)| (xm.max(x), ym.max(y)),
    );

    if x_max.is_finite() && x_max > 0.0 && y_max.is_finite() && y_max > 0.0 {
        Some((x_max, y_max))
    } else {
        None
    }
}

/// Draw the mesh, force–deflection line, and operating-point markers into the
/// chart builder. `x_max`/`y_max` are the pre-validated finite, positive extents.
fn draw_chart<DB: DrawingBackend>(
    mut builder: ChartBuilder<DB>,
    design: &SpringDesign,
    units: UnitSystem,
    x_max: f64,
    y_max: f64,
) {
    let series = force_deflection_series(design, units);

    // Apply floor so tiny-but-valid ranges don't produce a degenerate axis.
    let x_max = (x_max * 1.1).max(1e-9);
    let y_max = (y_max * 1.1).max(1e-9);

    let (x_label, y_label) = match units {
        UnitSystem::Metric => ("deflection (mm)", "load (N)"),
        UnitSystem::Us => ("deflection (in)", "load (lbf)"),
    };

    // Theme colours derived from the shared palette tokens so the chart
    // stays in sync with the application palette automatically.
    let accent_cyan = to_rgb(C::ACCENT);
    let amber = to_rgb(C::WARN);
    let mesh_light = to_rgb(C::LINE);
    let mesh_bold = to_rgb(C::RAISED);
    let axis_color = to_rgb(C::TEXT);
    let tick_label_color = to_rgb(C::MUTED);

    let line_style = ShapeStyle {
        color: accent_cyan.to_rgba(),
        filled: false,
        stroke_width: 2,
    };

    let mut chart = builder
        .margin(24)
        .x_label_area_size(44)
        .y_label_area_size(Y_LABEL_AREA_SIZE as i32)
        .build_cartesian_2d(0.0..x_max, 0.0..y_max)
        .expect("chart axes");

    chart
        .configure_mesh()
        .light_line_style(ShapeStyle {
            color: mesh_light.to_rgba(),
            filled: false,
            stroke_width: 1,
        })
        .bold_line_style(ShapeStyle {
            color: mesh_bold.to_rgba(),
            filled: false,
            stroke_width: 1,
        })
        .axis_style(ShapeStyle {
            color: axis_color.to_rgba(),
            filled: false,
            stroke_width: 1,
        })
        .label_style(("sans-serif", 14).into_font().color(&tick_label_color))
        .axis_desc_style(("sans-serif", 15).into_font().color(&axis_color))
        .x_desc(x_label)
        .y_desc(y_label)
        .draw()
        .expect("mesh");

    // Only emit finite series points to plotters.
    let finite_series: Vec<(f64, f64)> = series
        .iter()
        .copied()
        .filter(|(x, y)| finite_xy(*x, *y))
        .collect();

    chart
        .draw_series(LineSeries::new(finite_series, line_style))
        .expect("line");

    // Filter markers to finite coordinates only.
    let pts: Vec<(f64, f64)> = design
        .load_points
        .iter()
        .map(|lp| match units {
            UnitSystem::Metric => (lp.deflection.millimeters(), lp.force.newtons()),
            UnitSystem::Us => (lp.deflection.inches(), lp.force.pounds_force()),
        })
        .filter(|(x, y)| finite_xy(*x, *y))
        .collect();

    let marker_style = ShapeStyle {
        color: amber.to_rgba(),
        filled: true,
        stroke_width: 0,
    };

    chart
        .draw_series(
            pts.iter()
                .map(|&(x, y)| Circle::new((x, y), 5, marker_style)),
        )
        .expect("markers");
}

/// Render the chart into an `CHART_W`×`CHART_H` RGBA pixel buffer.
fn render_rgba(design: &SpringDesign, units: UnitSystem, x_max: f64, y_max: f64) -> Vec<u8> {
    ensure_font();

    let mut rgb = vec![0u8; (CHART_W * CHART_H * 3) as usize];
    {
        let root = BitMapBackend::with_buffer(&mut rgb, (CHART_W, CHART_H)).into_drawing_area();
        root.fill(&to_rgb(C::PANEL)).expect("fill chart background");
        draw_chart(ChartBuilder::on(&root), design, units, x_max, y_max);
        root.present().expect("present chart bitmap");
    }

    // The bitmap backend produced RGB; iced's image widget expects RGBA.
    let mut rgba = Vec::with_capacity((CHART_W * CHART_H * 4) as usize);
    for px in rgb.chunks_exact(3) {
        rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
    }
    rgba
}

/// Build the chart element, sized to a fixed height.
///
/// When the design is degenerate (non-finite axis extent), returns a text
/// placeholder instead of rendering. This keeps plotters from receiving a
/// non-finite range, which would cause an integer-overflow panic.
pub fn results_chart(design: &SpringDesign, units: UnitSystem) -> iced::Element<'_, Message> {
    let series = force_deflection_series(design, units);
    match finite_positive_extent(&series) {
        None => iced::widget::text("Chart unavailable for this design (check inputs).").into(),
        Some((x_max, y_max)) => {
            let pixels = render_rgba(design, units, x_max, y_max);
            let handle = iced::widget::image::Handle::from_rgba(CHART_W, CHART_H, pixels);
            iced::widget::image(handle)
                .width(iced::Length::Fill)
                .height(iced::Length::Fixed(CHART_H as f32))
                .into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use springcore::units::{Force, Length};
    use springcore::EndFixity;
    use springcore::{EndType, MaterialSet, PowerUser, Scenario, UnitSystem};

    fn design() -> springcore::SpringDesign {
        let m = MaterialSet::load_default()
            .get("Music Wire")
            .unwrap()
            .clone();
        PowerUser {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            active: 10.0,
            free_length: Length::from_millimeters(60.0),
            loads: vec![Force::from_newtons(10.0), Force::from_newtons(30.0)],
        }
        // Deliberate: Bergsträsser is the default; this test exercises geometry, not correction choice.
        .solve(&m, springcore::CurvatureCorrection::Bergstrasser)
        .unwrap()
    }

    #[test]
    fn series_starts_at_origin_and_is_linear() {
        let s = force_deflection_series(&design(), UnitSystem::Metric);
        assert!(s.len() >= 2);
        assert_relative_eq!(s[0].0, 0.0, max_relative = 1e-12);
        assert_relative_eq!(s[0].1, 0.0, max_relative = 1e-12);
        // Last point: at 30 N, deflection = 30 N / 2000 N/m = 0.015 m = 15 mm
        let last = s.last().unwrap();
        assert_relative_eq!(last.0, 15.0, max_relative = 1e-6);
        assert_relative_eq!(last.1, 30.0, max_relative = 1e-6);
    }

    #[test]
    fn series_is_all_finite_for_normal_design() {
        let s = force_deflection_series(&design(), UnitSystem::Metric);
        for (x, y) in &s {
            assert!(x.is_finite(), "x coordinate must be finite: {x}");
            assert!(y.is_finite(), "y coordinate must be finite: {y}");
        }
    }

    #[test]
    fn finite_positive_extent_returns_none_for_inf_point() {
        let series = vec![(0.0_f64, 0.0_f64), (f64::INFINITY, 100.0)];
        assert!(finite_positive_extent(&series).is_none());
    }

    #[test]
    fn finite_positive_extent_returns_none_for_all_zero() {
        // A series with only the origin has no positive extent.
        let series = vec![(0.0_f64, 0.0_f64)];
        assert!(finite_positive_extent(&series).is_none());
    }

    #[test]
    fn finite_positive_extent_returns_some_for_valid_series() {
        let series = vec![(0.0_f64, 0.0_f64), (15.0, 30.0)];
        let extent = finite_positive_extent(&series);
        assert!(extent.is_some());
        let (x_max, y_max) = extent.unwrap();
        assert_relative_eq!(x_max, 15.0, max_relative = 1e-12);
        assert_relative_eq!(y_max, 30.0, max_relative = 1e-12);
    }

    #[test]
    fn render_rgba_produces_full_opaque_buffer_with_content() {
        let d = design();
        let series = force_deflection_series(&d, UnitSystem::Metric);
        let (x_max, y_max) = finite_positive_extent(&series).expect("valid extent");
        let pixels = render_rgba(&d, UnitSystem::Metric, x_max, y_max);

        // Correct RGBA buffer size, fully opaque.
        assert_eq!(pixels.len(), (CHART_W * CHART_H * 4) as usize);
        assert!(pixels.chunks_exact(4).all(|p| p[3] == 255));

        let bg = to_rgb(C::PANEL);
        let differs = |col: u32, row: u32| {
            let i = ((row * CHART_W + col) * 4) as usize;
            pixels[i] != bg.0 || pixels[i + 1] != bg.1 || pixels[i + 2] != bg.2
        };

        // Something was drawn over the panel background (axes/line/labels).
        assert!(
            (0..CHART_W).any(|c| (0..CHART_H).any(|r| differs(c, r))),
            "chart must draw content over the background"
        );

        // Glyphs specifically: the left y-label band holds only axis text — the
        // plot line, mesh, and markers all sit right of the y-axis (margin +
        // Y_LABEL_AREA_SIZE). Content there proves the bundled font actually
        // rasterized, not merely that lines drew. Sharing the const (not a
        // hardcoded width) keeps this guard valid if the label area is resized.
        assert!(
            (0..Y_LABEL_AREA_SIZE).any(|c| (0..CHART_H).any(|r| differs(c, r))),
            "axis labels (bundled font) must rasterize in the y-label band"
        );
    }

    #[test]
    fn finite_positive_extent_ignores_non_finite_points() {
        // Mixed: one finite point and one inf point — inf is filtered, finite wins.
        let series = vec![(5.0_f64, 10.0_f64), (f64::INFINITY, f64::INFINITY)];
        let extent = finite_positive_extent(&series);
        assert!(extent.is_some());
        let (x_max, y_max) = extent.unwrap();
        assert_relative_eq!(x_max, 5.0, max_relative = 1e-12);
        assert_relative_eq!(y_max, 10.0, max_relative = 1e-12);
    }
}
