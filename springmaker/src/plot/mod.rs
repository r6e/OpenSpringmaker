//! The family-agnostic chart core (ADR 0008). Each family's pure presenter
//! (`plot_model.rs`) turns a solved design into a [`ChartData`]; this module
//! renders that data ([`render`]) into a bitmap and drives the interactive
//! hover widget ([`canvas`]) that displays it, with the pixel↔data affine
//! math ([`mapping`]) shared between the two so they cannot drift apart.
//!
//! The chart is drawn by `plotters` into an in-memory RGB bitmap, displayed
//! by the hover canvas (`ChartCanvas`) via `Frame::draw_image`. This keeps
//! the (well-established) `plotters` drawing code without depending on any
//! iced-coupled backend, so the chart is never blocked by a `plotters-iced`
//! version lag. Text is rendered with a bundled font (`ab_glyph`), so there
//! is no runtime system-font lookup.

use plotters::prelude::*;
use plotters::style::{register_font, FontStyle};
use springcore::UnitSystem;

pub mod canvas;
pub mod mapping;
pub mod render;

pub use canvas::chart_element;
#[cfg(test)]
pub(crate) use canvas::CHART_PLACEHOLDER;

/// Fixed render resolution for the chart bitmap; iced scales it to fit the panel.
pub(crate) const CHART_W: u32 = 760;
pub(crate) const CHART_H: u32 = 300;
/// Top/bottom margin surrounding the plot area.
pub(crate) const MARGIN: u32 = 24;
/// Height of the bottom x-axis label band.
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

/// Convert a tightly-packed RGB buffer (as `plotters`' `BitMapBackend` fills
/// it — it has no RGBA mode) to RGBA with a fully-opaque alpha channel, as
/// iced's image widget expects. Shared by the 2D chart and 3D scene
/// renderers so the conversion cannot drift between them.
pub(crate) fn rgb_to_rgba(rgb: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(rgb.len() / 3 * 4);
    for px in rgb.chunks_exact(3) {
        rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
    }
    rgba
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

/// Whether a point may be drawn: finite AND non-negative in both coordinates.
/// Chart axes are always 0-based, so a negative coordinate would render
/// outside the plot area; the renderer and `chart_extent` share this exact
/// predicate so a point is either visible and counted, or neither (defense in
/// depth — presenters emit non-negative values from engine-guarded designs).
pub(crate) fn plottable(x: f64, y: f64) -> bool {
    x.is_finite() && y.is_finite() && x >= 0.0 && y >= 0.0
}

/// Extent across every line point and marker: y must be finite-positive; x
/// must be finite and non-negative. x == 0 is deliberately allowed — it is
/// how the extension family's initial-tension jump (a vertical line at
/// x = 0 from (0, 0) to (0, Fi)) renders when every load is at or below Fi.
/// Points that are not [`plottable`] (non-finite or negative in either
/// coordinate) are ignored, exactly as the renderer ignores them.
/// `None` means the chart is degenerate and must not reach plotters
/// (non-finite ranges panic).
pub fn chart_extent(data: &ChartData) -> Option<(f64, f64)> {
    let pts = data
        .lines
        .iter()
        .flat_map(|l| l.points.iter().copied())
        .chain(data.markers.iter().map(|m| (m.x, m.y)));
    let (x_max, y_max) = pts.filter(|&(x, y)| plottable(x, y)).fold(
        (f64::NEG_INFINITY, f64::NEG_INFINITY),
        |(xm, ym), (x, y)| (xm.max(x), ym.max(y)),
    );
    (x_max.is_finite() && x_max >= 0.0 && y_max.is_finite() && y_max > 0.0)
        .then_some((x_max, y_max))
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
            .map(|lp| operating_marker(lp.deflection.meters(), lp.force.newtons(), units))
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

/// Build an `Operating` marker from a deflection (meters) and force
/// (newtons), converting both to display units. Shared by the round-wire,
/// extension, and assembly presenters, which all plot force vs. linear
/// deflection; torsion plots angle vs. moment and builds its markers
/// directly.
pub(crate) fn operating_marker(defl_m: f64, force_n: f64, units: UnitSystem) -> Marker {
    let (x, y) = convert_fd(defl_m, force_n, units);
    Marker {
        x,
        y,
        kind: MarkerKind::Operating,
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

/// Axes for the Goodman diagram (compression + torsion families): the
/// plotted quantities (`FatigueResult::mean_stress`/`alternating_stress`,
/// derived from torsional shear) are SHEAR stresses, not normal — τ, not σ.
pub(crate) fn shear_stress_axes(units: UnitSystem) -> (AxisMeta, AxisMeta) {
    match units {
        UnitSystem::Metric => (
            AxisMeta {
                label: "mean shear stress (MPa)",
                symbol: "τm",
                unit: "MPa",
            },
            AxisMeta {
                label: "alternating shear stress (MPa)",
                symbol: "τa",
                unit: "MPa",
            },
        ),
        UnitSystem::Us => (
            AxisMeta {
                label: "mean shear stress (ksi)",
                symbol: "τm",
                unit: "ksi",
            },
            AxisMeta {
                label: "alternating shear stress (ksi)",
                symbol: "τa",
                unit: "ksi",
            },
        ),
    }
}

/// Axes for the Gerber diagram (torsion family's bending fatigue check):
/// normal stress — σ.
pub(crate) fn normal_stress_axes(units: UnitSystem) -> (AxisMeta, AxisMeta) {
    match units {
        UnitSystem::Metric => (
            AxisMeta {
                label: "mean stress (MPa)",
                symbol: "σm",
                unit: "MPa",
            },
            AxisMeta {
                label: "alternating stress (MPa)",
                symbol: "σa",
                unit: "MPa",
            },
        ),
        UnitSystem::Us => (
            AxisMeta {
                label: "mean stress (ksi)",
                symbol: "σm",
                unit: "ksi",
            },
            AxisMeta {
                label: "alternating stress (ksi)",
                symbol: "σa",
                unit: "ksi",
            },
        ),
    }
}

/// Stress in the active unit system: MPa (metric) or ksi (US) — matches the
/// app-wide display convention (`presenter::display_stress`).
pub(crate) fn stress_display(s: springcore::units::Stress, units: UnitSystem) -> f64 {
    match units {
        UnitSystem::Metric => s.megapascals(),
        UnitSystem::Us => s.psi() / 1000.0,
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

    /// Negative coordinates are excluded by the same `plottable` predicate the
    /// renderer uses: a chart whose only positive-y content sits at a negative
    /// coordinate is degenerate (the renderer would not draw that point, so
    /// the extent must not count it either).
    #[test]
    fn chart_extent_ignores_negative_coordinates() {
        let d = data_with(
            vec![Line {
                points: vec![(0.0, 0.0), (-4.0, 30.0)],
                role: LineRole::Primary,
                name: None,
            }],
            vec![Marker {
                x: 5.0,
                y: -2.0,
                kind: MarkerKind::Operating,
            }],
        );
        assert!(chart_extent(&d).is_none());
    }

    #[test]
    fn chart_extent_allows_zero_x_when_y_positive() {
        // The extension family's Fi jump: x stays at 0, y is the (positive)
        // Fi value — must be renderable, not treated as degenerate.
        let d = data_with(
            vec![Line {
                points: vec![(0.0, 0.0), (0.0, 20.0)],
                role: LineRole::Primary,
                name: None,
            }],
            vec![],
        );
        let (x, y) = chart_extent(&d).expect("x = 0, y > 0 must be a valid extent");
        assert_relative_eq!(x, 0.0, max_relative = 1e-12);
        assert_relative_eq!(y, 20.0, max_relative = 1e-12);
    }

    #[test]
    fn round_wire_force_deflection_falls_back_to_at_solid_with_no_loads() {
        // Empty load_points: the line's extent must come from at_solid (the
        // compression-family precedent documented on the function), and no
        // operating markers exist since there are no load points to mark.
        use springcore::units::{Force, Length, Stress};
        use springcore::{LoadPoint, SpringRate};

        let at_solid = LoadPoint {
            force: Force::from_newtons(50.0),
            deflection: Length::from_millimeters(25.0),
            length: Length::from_millimeters(35.0),
            shear_stress: Stress::from_megapascals(400.0),
            pct_mts: 50.0,
        };
        let rate = SpringRate::from_newtons_per_meter(2000.0);
        let data = round_wire_force_deflection(rate, &[], &at_solid, UnitSystem::Metric);

        assert!(data.markers.is_empty());
        assert_eq!(data.lines.len(), 1);
        let pts = &data.lines[0].points;
        assert_eq!(pts.len(), 2);
        assert_relative_eq!(pts[0].0, 0.0, max_relative = 1e-12);
        assert_relative_eq!(pts[0].1, 0.0, max_relative = 1e-12);
        assert_relative_eq!(pts[1].0, 25.0, max_relative = 1e-9); // at_solid deflection, mm
        assert_relative_eq!(pts[1].1, 50.0, max_relative = 1e-9); // at_solid force, N

        let (x, y) = chart_extent(&data).expect("at_solid fallback must be a renderable extent");
        assert_relative_eq!(x, 25.0, max_relative = 1e-9);
        assert_relative_eq!(y, 50.0, max_relative = 1e-9);
    }

    #[test]
    fn hover_readout_formats_both_axes() {
        let d = data_with(vec![], vec![]);
        assert_eq!(hover_readout(&d, 12.3, 45.6), "y = 12.30 mm · F = 45.60 N");
    }
}
