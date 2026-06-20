//! Live load-vs-deflection chart for the current design.

use crate::app::Message;
use plotters::prelude::*;
use plotters_iced::{Chart, ChartWidget};
use springcore::{SpringDesign, UnitSystem};

/// Force–deflection points (deflection x, force y) in the display unit system.
///
/// The spring rate is constant, so the line is linear; two endpoints suffice.
/// The maximum operating load (or solid load, whichever is larger) sets the extent.
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
    let max_defl_m = if rate > 0.0 { max_force_n / rate } else { 0.0 };

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

/// Chart wrapper — renders the force–deflection line and operating-point markers.
pub struct ResultsChart<'a> {
    pub design: &'a SpringDesign,
    pub units: UnitSystem,
}

impl<'a> Chart<Message> for ResultsChart<'a> {
    type State = ();

    fn build_chart<DB: DrawingBackend>(&self, _state: &Self::State, mut builder: ChartBuilder<DB>) {
        let series = force_deflection_series(self.design, self.units);
        let x_max = series.iter().map(|p| p.0).fold(0.0_f64, f64::max).max(1e-9);
        let y_max = series.iter().map(|p| p.1).fold(0.0_f64, f64::max).max(1e-9);

        let (x_label, y_label) = match self.units {
            UnitSystem::Metric => ("deflection (mm)", "load (N)"),
            UnitSystem::Us => ("deflection (in)", "load (lbf)"),
        };

        let mut chart = builder
            .margin(20)
            .x_label_area_size(30)
            .y_label_area_size(50)
            .build_cartesian_2d(0.0..x_max * 1.1, 0.0..y_max * 1.1)
            .expect("chart axes");

        chart
            .configure_mesh()
            .x_desc(x_label)
            .y_desc(y_label)
            .draw()
            .expect("mesh");

        chart
            .draw_series(LineSeries::new(series.iter().copied(), &BLUE))
            .expect("line");

        let pts: Vec<(f64, f64)> = self
            .design
            .load_points
            .iter()
            .map(|lp| match self.units {
                UnitSystem::Metric => (lp.deflection.millimeters(), lp.force.newtons()),
                UnitSystem::Us => (lp.deflection.inches(), lp.force.pounds_force()),
            })
            .collect();

        chart
            .draw_series(
                pts.iter()
                    .map(|&(x, y)| Circle::new((x, y), 4, RED.filled())),
            )
            .expect("markers");
    }
}

/// Build the chart widget element, sized to a fixed height.
pub fn results_chart(design: &SpringDesign, units: UnitSystem) -> iced::Element<'_, Message> {
    ChartWidget::new(ResultsChart { design, units })
        .height(iced::Length::Fixed(300.0))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use springcore::mechanics::EndFixity;
    use springcore::units::{Force, Length};
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
}
