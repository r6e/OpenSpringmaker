//! Pure chart presenter for the torsion family (ADR 0008): moment vs angle.
//! The line is the ideal rate line θ = M/k′; markers read the engine's SOLVED
//! `deflection` field rather than recomputing M/k. With the current linear
//! engine those coincide, but the presenter must not bake that in — a test
//! pins that the field is read, not derived.

use crate::plot::{
    stress_axes, stress_display, AxisMeta, ChartData, Line, LineRole, Marker, MarkerKind,
};
use springcore::torsion::TorsionDesign;
use springcore::UnitSystem;

pub fn torsion_chart(design: &TorsionDesign, units: UnitSystem) -> ChartData {
    let x_axis = AxisMeta {
        label: "angle (deg)",
        symbol: "θ",
        unit: "°",
    };
    let y_axis = match units {
        UnitSystem::Metric => AxisMeta {
            label: "moment (N·mm)",
            symbol: "M",
            unit: "N·mm",
        },
        UnitSystem::Us => AxisMeta {
            label: "moment (lbf·in)",
            symbol: "M",
            unit: "lbf·in",
        },
    };
    let display_moment = |m: springcore::units::Moment| match units {
        UnitSystem::Metric => m.newton_millimeters(),
        UnitSystem::Us => m.pound_force_inches(),
    };

    let max_m_nm = design
        .load_points
        .iter()
        .map(|lp| lp.moment.newton_meters())
        .fold(0.0_f64, f64::max);
    let k = design.rate.newton_meters_per_degree();
    let rate_ok = k.is_finite() && k > 0.0;
    let lines = if rate_ok && max_m_nm.is_finite() && max_m_nm > 0.0 {
        let theta = max_m_nm / k;
        let max_m = design
            .load_points
            .iter()
            .map(|lp| display_moment(lp.moment))
            .fold(0.0_f64, f64::max);
        vec![Line {
            points: vec![(0.0, 0.0), (theta, max_m)],
            role: LineRole::Primary,
            name: None,
        }]
    } else {
        vec![]
    };
    // A non-positive or non-finite rate makes the whole design degenerate, not
    // just the line: the solved deflections came from that rate, so markers are
    // suppressed too, keeping `chart_extent` None and the chart out of plotters
    // (round_wire_force_deflection convention).
    let markers = if rate_ok {
        design
            .load_points
            .iter()
            .map(|lp| Marker {
                x: lp.deflection.degrees(),
                y: display_moment(lp.moment),
                kind: MarkerKind::Operating,
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

/// Gerber fatigue diagram (Shigley §10-12): parabolic envelope
/// σa = Se·(1 − (σm/Sut)²) sampled as a 65-point polyline, load line
/// r = σa/σm from the origin to the engine's strength amplitude (Sa,
/// Eq. 10-59). The GUI samples the cited criterion for display only; the
/// engine remains the factor-of-safety authority.
pub fn gerber_chart(f: &springcore::torsion::TorFatigueResult, units: UnitSystem) -> ChartData {
    let (x_axis, y_axis) = stress_axes(units);
    let se = stress_display(f.fully_reversed_endurance, units);
    let sut = stress_display(f.ultimate_tensile, units);
    let sa_op = stress_display(f.alternating_stress, units);
    let sm_op = stress_display(f.mean_stress, units);
    let sa_strength = stress_display(f.strength_amplitude, units);
    if !(se.is_finite()
        && se > 0.0
        && sut.is_finite()
        && sut > 0.0
        && sa_op.is_finite()
        && sa_op > 0.0
        && sm_op.is_finite()
        && sm_op > 0.0)
    {
        return ChartData {
            x_axis,
            y_axis,
            lines: vec![],
            markers: vec![],
        };
    }

    const SAMPLES: usize = 65;
    let envelope_pts: Vec<(f64, f64)> = (0..SAMPLES)
        .map(|i| {
            let sm = sut * i as f64 / (SAMPLES - 1) as f64;
            (sm, se * (1.0 - (sm / sut).powi(2)))
        })
        .collect();
    let r = sa_op / sm_op; // engine rejects σm = 0 (strictly increasing moments)
    let sm_star = sa_strength / r;
    ChartData {
        x_axis,
        y_axis,
        lines: vec![
            Line {
                points: envelope_pts,
                role: LineRole::Envelope,
                name: None,
            },
            Line {
                points: vec![(0.0, 0.0), (sm_star, sa_strength)],
                role: LineRole::LoadLine,
                name: None,
            },
        ],
        markers: vec![
            Marker {
                x: sm_op,
                y: sa_op,
                kind: MarkerKind::Operating,
            },
            Marker {
                x: sm_star,
                y: sa_strength,
                kind: MarkerKind::Limit,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::torsion::form::TorFormState;
    use approx::assert_relative_eq;
    use springcore::units::Stress;
    use springcore::{CurvatureCorrection, Family, MaterialSet, MaterialStore, UnitSystem};

    fn store() -> MaterialStore {
        MaterialStore::new(MaterialSet::load_default())
    }

    /// Mirror the golden metric fixture from torsion/view_model.rs tests.
    fn design() -> TorsionDesign {
        let mut app = App::from_store(store(), Vec::new(), CurvatureCorrection::Bergstrasser);
        app.family = Family::Torsion;
        app.torsion = TorFormState {
            wire_dia: "2".to_string(),
            mean_dia: "20".to_string(),
            body_coils: "5".to_string(),
            leg1: "0".to_string(),
            leg2: "0".to_string(),
            moments: "500, 1000".to_string(),
            ..Default::default()
        };
        app.recompute();
        app.tor_outcome.expect("torsion solve must succeed").design
    }

    #[test]
    fn ideal_line_ends_at_max_moment_over_rate() {
        let d = design();
        let data = torsion_chart(&d, UnitSystem::Metric);
        let max_m_nm = d
            .load_points
            .iter()
            .map(|lp| lp.moment.newton_meters())
            .fold(0.0_f64, f64::max);
        let theta_deg = max_m_nm / d.rate.newton_meters_per_degree();
        let last = *data.lines[0].points.last().unwrap();
        assert_relative_eq!(last.0, theta_deg, max_relative = 1e-9);
        // Pin the display value against the ENGINE's max-moment load point,
        // not a recomputation of the presenter's own fold.
        let max_lp = d
            .load_points
            .iter()
            .max_by(|a, b| {
                a.moment
                    .newton_meters()
                    .total_cmp(&b.moment.newton_meters())
            })
            .expect("fixture has load points");
        assert_relative_eq!(
            last.1,
            max_lp.moment.newton_millimeters(),
            max_relative = 1e-9
        );
        assert_eq!(data.x_axis.unit, "°");
        assert_eq!(data.y_axis.unit, "N·mm");
    }

    #[test]
    fn markers_use_solved_deflections_not_the_ideal_line() {
        // The linear engine puts every solved point exactly on M = k′·θ, so a
        // presenter that recomputed markers from the rate would pass a plain
        // field comparison. Mutate the solved deflection off the ideal line:
        // the marker must follow the FIELD, proving it is read, not derived.
        let mut d = design();
        let nudged =
            springcore::units::Angle::from_degrees(d.load_points[0].deflection.degrees() + 3.0);
        d.load_points[0].deflection = nudged;
        let data = torsion_chart(&d, UnitSystem::Metric);
        assert_relative_eq!(data.markers[0].x, nudged.degrees(), max_relative = 1e-12);
        for (m, lp) in data.markers.iter().zip(&d.load_points) {
            assert_relative_eq!(m.x, lp.deflection.degrees(), max_relative = 1e-12);
        }
    }

    #[test]
    fn invalid_rate_yields_degenerate_chart() {
        // Zero rate invalidates the whole design: no line, no markers, and
        // chart_extent must be None so the placeholder path is taken.
        let mut d = design();
        d.rate = springcore::units::AngularRate::from_newton_meters_per_radian(0.0);
        let data = torsion_chart(&d, UnitSystem::Metric);
        assert!(data.lines.is_empty());
        assert!(data.markers.is_empty());
        assert!(crate::plot::chart_extent(&data).is_none());
    }

    #[test]
    fn us_units_convert_moment_axis() {
        let d = design();
        let data = torsion_chart(&d, UnitSystem::Us);
        assert_eq!(data.y_axis.unit, "lbf·in");
        let last = *data.lines[0].points.last().unwrap();
        let max_lbf_in = d
            .load_points
            .iter()
            .map(|lp| lp.moment.pound_force_inches())
            .fold(0.0_f64, f64::max);
        assert_relative_eq!(last.1, max_lbf_in, max_relative = 1e-9);
    }

    fn tor_fatigue_result() -> springcore::torsion::TorFatigueResult {
        springcore::torsion::TorFatigueResult {
            alternating_stress: Stress::from_megapascals(300.0),
            mean_stress: Stress::from_megapascals(300.0),
            fully_reversed_endurance: Stress::from_megapascals(500.0),
            ultimate_tensile: Stress::from_megapascals(1600.0),
            strength_amplitude: Stress::from_megapascals(480.0),
            gerber_factor_of_safety: 1.6,
        }
    }

    #[test]
    fn gerber_envelope_is_the_sampled_parabola() {
        // σa = Se·(1 − (σm/Sut)²), Shigley Eq. 10-58 family (§10-12).
        let d = gerber_chart(&tor_fatigue_result(), UnitSystem::Metric);
        let env = d
            .lines
            .iter()
            .find(|l| l.role == LineRole::Envelope)
            .unwrap();
        assert_eq!(d.lines.len(), 2);
        assert_eq!(d.markers.len(), 2);
        assert_eq!(env.points.len(), 65);
        assert_relative_eq!(env.points[0].1, 500.0, max_relative = 1e-9); // (0, Se)
        let last = *env.points.last().unwrap();
        assert_relative_eq!(last.0, 1600.0, max_relative = 1e-9); // (Sut, 0)
        assert!(last.1.abs() < 1e-9);
        // Midpoint sanity: at σm = Sut/2, σa = Se·(1 − 1/4).
        let mid = env.points[32];
        assert_relative_eq!(mid.0, 800.0, max_relative = 1e-9);
        assert_relative_eq!(mid.1, 375.0, max_relative = 1e-9);
    }

    #[test]
    fn gerber_markers_show_operating_and_strength_amplitude() {
        // Limit marker: (Sm*, Sa) with Sm* = Sa/r, r = σa/σm (load line), Sa from
        // the ENGINE's strength_amplitude — not re-derived here.
        let d = gerber_chart(&tor_fatigue_result(), UnitSystem::Metric);
        assert_eq!(d.lines.len(), 2);
        assert_eq!(d.markers.len(), 2);
        assert!(d.markers.iter().any(|m| m.kind == MarkerKind::Operating
            && (m.x - 300.0).abs() < 1e-9
            && (m.y - 300.0).abs() < 1e-9));
        let sm_star: f64 = 480.0 / (300.0_f64 / 300.0); // = 480.0
        assert!(d.markers.iter().any(|m| m.kind == MarkerKind::Limit
            && (m.x - sm_star).abs() < 1e-9
            && (m.y - 480.0).abs() < 1e-9));
    }

    #[test]
    fn gerber_zero_endurance_is_degenerate() {
        let mut f = tor_fatigue_result();
        f.fully_reversed_endurance = Stress::from_megapascals(0.0);
        let d = gerber_chart(&f, UnitSystem::Metric);
        assert_eq!(d.lines.len(), 0);
        assert_eq!(d.markers.len(), 0);
        assert!(crate::plot::chart_extent(&d).is_none());
    }
}
