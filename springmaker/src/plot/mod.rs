//! Live load-vs-deflection chart for the current design.
//!
//! The chart is drawn by `plotters` into an in-memory RGB bitmap and shown via
//! iced's `image` widget. This keeps the (well-established) `plotters` drawing
//! code without depending on any iced-coupled backend, so the chart is never
//! blocked by a `plotters-iced` version lag. Text is rendered with a bundled
//! font (`ab_glyph`), so there is no runtime system-font lookup.

use plotters::prelude::*;
use plotters::style::{register_font, FontStyle};
use springcore::UnitSystem;

pub mod canvas;
pub mod mapping;
pub mod render;

pub use canvas::chart_element;

/// Fixed render resolution for the chart bitmap; iced scales it to fit the panel.
pub(crate) const CHART_W: u32 = 760;
pub(crate) const CHART_H: u32 = 300;
/// Top/bottom margin and bottom x-axis label band width.
pub(crate) const MARGIN: u32 = 24;
pub(crate) const X_LABEL_AREA: u32 = 44;
/// Width of the left y-axis label band. Plot geometry sits right of it
/// (margin + this), so this column range contains only axis text — relied on by
/// the render test to confirm the bundled font rasterizes.
pub(crate) const Y_LABEL_AREA: u32 = 64;

/// Bundled font (DejaVu Sans, permissive license — see assets/LICENSE-DejaVu.txt),
/// registered under the "sans-serif" family so the chart needs no system fonts.
static FONT_BYTES: &[u8] = include_bytes!("../../assets/DejaVuSans.ttf");

/// Register the bundled font once (ab_glyph has no built-in fonts).
pub(crate) fn ensure_font() {
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
pub(crate) fn to_rgb(c: iced::Color) -> RGBColor {
    let ch = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    RGBColor(ch(c.r), ch(c.g), ch(c.b))
}

// ── ChartData: the pure contract between family presenters and the chart core ──

/// Axis metadata that a family presenter fills.
///
/// # Fields
/// - `label`: plotters axis description including unit (e.g., "deflection (mm)")
/// - `symbol`: hover readout symbol (e.g., "y")
/// - `unit`: hover readout unit (e.g., "mm")
pub struct AxisMeta {
    pub label: &'static str,
    pub symbol: &'static str,
    pub unit: &'static str,
}

/// Line role; maps to stroke style in the renderer only.
///
/// # Variants
/// - `Primary`: the family's main line
/// - `Member`: an assembly member overlay
/// - `Envelope`: a fatigue failure envelope
/// - `LoadLine`: a fatigue load line from the origin
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
// Envelope/LoadLine constructed from Tasks 7-8 (assembly legend uses Member
// from Task 7; fatigue uses Envelope/LoadLine from Task 8); remove then
#[allow(dead_code)]
pub enum LineRole {
    Primary,
    Member,
    Envelope,
    LoadLine,
}

/// A polyline.
///
/// # Fields
/// - `points`: vertices of the line
/// - `role`: stroke style (Primary, Member, Envelope, LoadLine)
/// - `name`: put in legend if `Some`
pub struct Line {
    pub points: Vec<(f64, f64)>,
    pub role: LineRole,
    pub name: Option<String>,
}

/// Marker kind; determines visual representation.
///
/// # Variants
/// - `Operating`: an operating point
/// - `Limit`: a limit point (travel limit, fatigue strength amplitude)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MarkerKind {
    Operating,
    // Consumed once a family emits limit markers (travel-limit/fatigue-amplitude, later tasks); remove then.
    #[allow(dead_code)]
    Limit,
}

/// A point marker.
pub struct Marker {
    pub x: f64,
    pub y: f64,
    pub kind: MarkerKind,
}

/// The pure contract between family presenters and the chart core.
pub struct ChartData {
    pub x_axis: AxisMeta,
    pub y_axis: AxisMeta,
    pub lines: Vec<Line>,
    pub markers: Vec<Marker>,
}

/// Finite-positive extent across every line point and marker. `None` means the
/// chart is degenerate and must not reach plotters (non-finite ranges panic).
pub fn chart_extent(data: &ChartData) -> Option<(f64, f64)> {
    let pts = data
        .lines
        .iter()
        .flat_map(|l| l.points.iter().copied())
        .chain(data.markers.iter().map(|m| (m.x, m.y)));
    let (x_max, y_max) = pts.filter(|(x, y)| x.is_finite() && y.is_finite()).fold(
        (f64::NEG_INFINITY, f64::NEG_INFINITY),
        |(xm, ym), (x, y)| (xm.max(x), ym.max(y)),
    );
    (x_max.is_finite() && x_max > 0.0 && y_max.is_finite() && y_max > 0.0).then_some((x_max, y_max))
}

/// Hover readout line, e.g. `y = 12.30 mm · F = 45.60 N`.
pub fn hover_readout(data: &ChartData, x: f64, y: f64) -> String {
    format!(
        "{} = {} {} · {} = {} {}",
        data.x_axis.symbol,
        crate::presenter::fmt_row_value(x, 2),
        data.x_axis.unit,
        data.y_axis.symbol,
        crate::presenter::fmt_row_value(y, 2),
        data.y_axis.unit,
    )
}

/// Force–deflection ChartData for the round-wire linear families
/// (compression, conical): one Primary line origin→max load, Operating
/// markers at the load points; extent falls back to the at-solid point when
/// no loads exist (compression-family precedent).
///
/// A non-positive or non-finite rate makes the whole design degenerate (not
/// just the line): the operating-point markers are derived from a spring
/// rate that is no longer valid, so they are suppressed along with the line,
/// keeping `chart_extent` `None` and the chart out of plotters entirely.
pub fn round_wire_force_deflection(
    rate: springcore::SpringRate,
    load_points: &[springcore::LoadPoint],
    at_solid: &springcore::LoadPoint,
    units: UnitSystem,
) -> ChartData {
    let (x_axis, y_axis) = force_deflection_axes(units);
    let max_force_n = if load_points.is_empty() {
        at_solid.force.newtons()
    } else {
        load_points
            .iter()
            .map(|lp| lp.force.newtons())
            .fold(0.0_f64, f64::max)
    };
    let k = rate.newtons_per_meter();
    let rate_ok = k.is_finite() && k > 0.0;
    let lines = if rate_ok && max_force_n.is_finite() {
        let max_defl_m = max_force_n / k;
        vec![Line {
            points: vec![(0.0, 0.0), convert_fd(max_defl_m, max_force_n, units)],
            role: LineRole::Primary,
            name: None,
        }]
    } else {
        vec![]
    };
    let markers = if rate_ok {
        load_points
            .iter()
            .map(|lp| {
                let (x, y) = convert_fd(lp.deflection.meters(), lp.force.newtons(), units);
                Marker {
                    x,
                    y,
                    kind: MarkerKind::Operating,
                }
            })
            .collect()
    } else {
        vec![]
    };
    ChartData {
        x_axis,
        y_axis,
        lines,
        markers,
    }
}

pub(crate) fn force_deflection_axes(units: UnitSystem) -> (AxisMeta, AxisMeta) {
    match units {
        UnitSystem::Metric => (
            AxisMeta {
                label: "deflection (mm)",
                symbol: "y",
                unit: "mm",
            },
            AxisMeta {
                label: "load (N)",
                symbol: "F",
                unit: "N",
            },
        ),
        UnitSystem::Us => (
            AxisMeta {
                label: "deflection (in)",
                symbol: "y",
                unit: "in",
            },
            AxisMeta {
                label: "load (lbf)",
                symbol: "F",
                unit: "lbf",
            },
        ),
    }
}

pub(crate) fn convert_fd(defl_m: f64, force_n: f64, units: UnitSystem) -> (f64, f64) {
    match units {
        UnitSystem::Metric => (
            springcore::units::Length::from_meters(defl_m).millimeters(),
            force_n,
        ),
        UnitSystem::Us => (
            springcore::units::Length::from_meters(defl_m).inches(),
            springcore::units::Force::from_newtons(force_n).pounds_force(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    fn axis(sym: &'static str, unit: &'static str) -> AxisMeta {
        AxisMeta {
            label: "test",
            symbol: sym,
            unit,
        }
    }

    fn data_with(lines: Vec<Line>, markers: Vec<Marker>) -> ChartData {
        ChartData {
            x_axis: axis("y", "mm"),
            y_axis: axis("F", "N"),
            lines,
            markers,
        }
    }

    #[test]
    fn chart_extent_spans_all_lines_and_markers() {
        let d = data_with(
            vec![
                Line {
                    points: vec![(0.0, 0.0), (10.0, 5.0)],
                    role: LineRole::Primary,
                    name: None,
                },
                Line {
                    points: vec![(0.0, 0.0), (4.0, 20.0)],
                    role: LineRole::Member,
                    name: None,
                },
            ],
            vec![Marker {
                x: 12.0,
                y: 8.0,
                kind: MarkerKind::Limit,
            }],
        );
        let (x, y) = chart_extent(&d).unwrap();
        assert_relative_eq!(x, 12.0, max_relative = 1e-12); // marker wins x
        assert_relative_eq!(y, 20.0, max_relative = 1e-12); // member line wins y
    }

    #[test]
    fn chart_extent_ignores_non_finite_and_requires_positive() {
        let d = data_with(
            vec![Line {
                points: vec![(0.0, 0.0), (f64::INFINITY, 5.0)],
                role: LineRole::Primary,
                name: None,
            }],
            vec![],
        );
        assert!(chart_extent(&d).is_none()); // only finite point is the origin → no positive extent
    }

    #[test]
    fn hover_readout_formats_both_axes() {
        let d = data_with(vec![], vec![]);
        assert_eq!(hover_readout(&d, 12.3, 45.6), "y = 12.30 mm · F = 45.60 N");
    }
}
