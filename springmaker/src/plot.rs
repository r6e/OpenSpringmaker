//! Live load-vs-deflection chart for the current design.

use crate::app::Message;
use plotters::prelude::*;
use plotters_iced::{Chart, ChartWidget};
use springcore::{SpringDesign, UnitSystem};

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

/// Returns `(x_max, y_max)` from `series`, considering only points where both
/// coordinates are finite. Returns `None` when the series has no finite points
/// or the resulting max is not finite and positive (i.e., the design is
/// degenerate and must not be passed to plotters).
pub fn finite_positive_extent(series: &[(f64, f64)]) -> Option<(f64, f64)> {
    let x_max = series
        .iter()
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .map(|&(x, _)| x)
        .fold(f64::NEG_INFINITY, f64::max);

    let y_max = series
        .iter()
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .map(|&(_, y)| y)
        .fold(f64::NEG_INFINITY, f64::max);

    if x_max.is_finite() && x_max > 0.0 && y_max.is_finite() && y_max > 0.0 {
        Some((x_max, y_max))
    } else {
        None
    }
}

/// Chart wrapper — renders the force–deflection line and operating-point markers.
pub struct ResultsChart<'a> {
    design: &'a SpringDesign,
    units: UnitSystem,
    /// Pre-computed, validated extent — always finite and positive.
    x_max: f64,
    y_max: f64,
}

impl<'a> Chart<Message> for ResultsChart<'a> {
    type State = ();

    fn build_chart<DB: DrawingBackend>(&self, _state: &Self::State, mut builder: ChartBuilder<DB>) {
        let series = force_deflection_series(self.design, self.units);

        // Apply floor so tiny-but-valid ranges don't produce a degenerate axis.
        let x_max = (self.x_max * 1.1).max(1e-9);
        let y_max = (self.y_max * 1.1).max(1e-9);

        let (x_label, y_label) = match self.units {
            UnitSystem::Metric => ("deflection (mm)", "load (N)"),
            UnitSystem::Us => ("deflection (in)", "load (lbf)"),
        };

        // Theme colours — keyed to the engineering-instrument dark palette.
        let accent_cyan = RGBColor(76, 194, 255);
        let amber = RGBColor(242, 181, 58);
        let mesh_light = RGBColor(42, 50, 61);
        let mesh_bold = RGBColor(58, 68, 82);
        let axis_color = RGBColor(230, 234, 240);
        let tick_label_color = RGBColor(138, 151, 166);

        let line_style = ShapeStyle {
            color: accent_cyan.to_rgba(),
            filled: false,
            stroke_width: 2,
        };

        let mut chart = builder
            .margin(24)
            .x_label_area_size(44)
            .y_label_area_size(64)
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
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .collect();

        chart
            .draw_series(LineSeries::new(finite_series, line_style))
            .expect("line");

        // Filter markers to finite coordinates only.
        let pts: Vec<(f64, f64)> = self
            .design
            .load_points
            .iter()
            .map(|lp| match self.units {
                UnitSystem::Metric => (lp.deflection.millimeters(), lp.force.newtons()),
                UnitSystem::Us => (lp.deflection.inches(), lp.force.pounds_force()),
            })
            .filter(|(x, y)| x.is_finite() && y.is_finite())
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
}

/// Build the chart widget element, sized to a fixed height.
///
/// When the design is degenerate (non-finite axis extent), returns a text
/// placeholder instead of constructing a `ChartWidget`. This prevents plotters
/// from receiving a non-finite range, which would cause an integer overflow panic.
pub fn results_chart(design: &SpringDesign, units: UnitSystem) -> iced::Element<'_, Message> {
    let series = force_deflection_series(design, units);
    match finite_positive_extent(&series) {
        None => iced::widget::text("Chart unavailable for this design (check inputs).").into(),
        Some((x_max, y_max)) => ChartWidget::new(ResultsChart {
            design,
            units,
            x_max,
            y_max,
        })
        .height(iced::Length::Fixed(300.0))
        .into(),
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
        .solve(&m)
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
