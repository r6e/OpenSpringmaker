//! Family-agnostic presenter vocabulary: the plain-data types a humble view
//! renders. iced-free, so every type is unit-testable without a renderer and
//! reusable by any spring family's presenter. Family-specific presenter
//! functions and result aggregates live in each family's `view_model`.

use crate::form_helpers::MM_PER_M;
use springcore::{
    Angle, AngularRate, Force, Length, Moment, Severity, SpringRate, StatusMessage, Stress,
    UnitSystem,
};

// ── Results panel ───────────────────────────────────────────────────────────

/// Emphasis for a result value; the view maps this to a color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Emphasis {
    Normal,
    Danger,
}

/// A muted-label + value(+unit) readout row, with value emphasis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResultRow {
    pub label: String,
    pub value: String,
    pub unit: String,
    pub emphasis: Emphasis,
}

impl ResultRow {
    pub(crate) fn new(
        label: impl Into<String>,
        value: impl Into<String>,
        unit: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            unit: unit.into(),
            emphasis: Emphasis::Normal,
        }
    }

    pub(crate) fn danger(
        label: impl Into<String>,
        value: impl Into<String>,
        unit: impl Into<String>,
    ) -> Self {
        Self {
            emphasis: Emphasis::Danger,
            ..Self::new(label, value, unit)
        }
    }
}

/// One row of the load-points table, all fields pre-formatted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoadRow {
    pub point: String,
    pub force: String,
    pub deflection: String,
    pub length: String,
    pub stress: String,
    pub pct_mts: String,
}

/// The load-points table: a stress-unit header label plus per-point rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoadTable {
    pub stress_unit: String,
    pub rows: Vec<LoadRow>,
}

// ── Status panel ────────────────────────────────────────────────────────────

/// Severity class of a status line; the view maps this to a prefix and color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatusKind {
    /// A failed save/load action (see `App::action_error`).
    ActionError,
    /// Startup material-load warning (can appear before any design is solved).
    LoadWarning,
    Info,
    Caution,
    /// A design-level warning (see `springcore::Severity::Warning`).
    DesignWarning,
}

/// One line in the status panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StatusLine {
    pub kind: StatusKind,
    pub text: String,
}

// ── Inputs panel ────────────────────────────────────────────────────────────

/// A labeled input descriptor, generic over the family's field enum. Each family
/// builds `FieldDescriptor<its Field>`; its humble view maps `field` to that
/// family's message variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FieldDescriptor<F> {
    pub label: String,
    pub field: F,
}

impl<F> FieldDescriptor<F> {
    pub(crate) fn new(label: impl Into<String>, field: F) -> Self {
        Self {
            label: label.into(),
            field,
        }
    }
}

// ── Unit labels and conversions ─────────────────────────────────────────────

/// Length unit label for the active unit system.
pub(crate) fn unit_length_label(us: UnitSystem) -> &'static str {
    match us {
        UnitSystem::Metric => "mm",
        UnitSystem::Us => "in",
    }
}

/// Force unit label for the active unit system.
pub(crate) fn unit_force_label(us: UnitSystem) -> &'static str {
    match us {
        UnitSystem::Metric => "N",
        UnitSystem::Us => "lbf",
    }
}

/// Spring-rate unit label for the active unit system.
pub(crate) fn unit_rate_label(us: UnitSystem) -> &'static str {
    match us {
        UnitSystem::Metric => "N/mm",
        UnitSystem::Us => "lbf/in",
    }
}

/// Stress unit label for the active unit system.
pub(crate) fn unit_stress_label(us: UnitSystem) -> &'static str {
    match us {
        UnitSystem::Metric => "MPa",
        UnitSystem::Us => "ksi",
    }
}

/// Length in the active unit system: mm (metric) or inches (US).
pub(crate) fn display_len(l: Length, us: UnitSystem) -> f64 {
    match us {
        UnitSystem::Metric => l.millimeters(),
        UnitSystem::Us => l.inches(),
    }
}

/// Force in the active unit system: N (metric) or lbf (US).
pub(crate) fn display_force(f: Force, us: UnitSystem) -> f64 {
    match us {
        UnitSystem::Metric => f.newtons(),
        UnitSystem::Us => f.pounds_force(),
    }
}

/// Spring rate in the active unit system: N/mm (metric) or lbf/in (US).
pub(crate) fn display_rate(r: SpringRate, us: UnitSystem) -> f64 {
    match us {
        // Display in N/mm (= N/m ÷ MM_PER_M) so rate is consistent with mm lengths and
        // the chart axes (deflection in mm, force in N → slope in N/mm).
        UnitSystem::Metric => r.newtons_per_meter() / MM_PER_M,
        UnitSystem::Us => r.pounds_per_inch(),
    }
}

/// Stress `(value, label)` in the active unit system: MPa (metric) or ksi (US).
pub(crate) fn display_stress(s: Stress, us: UnitSystem) -> (f64, &'static str) {
    let value = match us {
        UnitSystem::Metric => s.megapascals(),
        UnitSystem::Us => s.psi() / 1000.0,
    };
    (value, unit_stress_label(us))
}

/// Moment unit label for the active unit system.
pub(crate) fn unit_moment_label(us: UnitSystem) -> &'static str {
    match us {
        UnitSystem::Metric => "N·mm",
        UnitSystem::Us => "lbf·in",
    }
}

/// Moment in the active unit system: N·mm (metric) or lbf·in (US).
pub(crate) fn display_moment(m: Moment, us: UnitSystem) -> f64 {
    match us {
        UnitSystem::Metric => m.newton_millimeters(),
        UnitSystem::Us => m.pound_force_inches(),
    }
}

/// Angular deflection in degrees (unit-system independent).
pub(crate) fn display_angle_degrees(a: Angle) -> f64 {
    a.degrees()
}

/// Angular deflection in revolutions / turns (unit-system independent).
pub(crate) fn display_angle_turns(a: Angle) -> f64 {
    a.turns()
}

/// Angular rate as moment per degree: N·mm/° (metric) or lbf·in/° (US).
pub(crate) fn display_ang_rate_per_deg(r: AngularRate, us: UnitSystem) -> f64 {
    match us {
        UnitSystem::Metric => r.newton_meters_per_degree() * MM_PER_M,
        UnitSystem::Us => r.pound_force_inches_per_degree(),
    }
}

/// Angular rate as moment per revolution: N·mm/rev (metric) or lbf·in/rev (US).
pub(crate) fn display_ang_rate_per_turn(r: AngularRate, us: UnitSystem) -> f64 {
    match us {
        UnitSystem::Metric => r.newton_meters_per_turn() * MM_PER_M,
        UnitSystem::Us => r.pound_force_inches_per_turn(),
    }
}

/// Map a design-message severity to its status-line class. Shared by every family.
pub(crate) fn status_kind(severity: Severity) -> StatusKind {
    match severity {
        Severity::Info => StatusKind::Info,
        Severity::Caution => StatusKind::Caution,
        Severity::Warning => StatusKind::DesignWarning,
    }
}

/// Shared status prefix: the save/load action error (if any) then material-load
/// warnings, in that order. Every family's status view opens with this before
/// appending its own design messages.
pub(crate) fn common_status_lines(app: &crate::app::App) -> Vec<StatusLine> {
    let mut lines = Vec::new();
    if let Some(text) = &app.action_error {
        lines.push(StatusLine {
            kind: StatusKind::ActionError,
            text: text.clone(),
        });
    }
    for warn in &app.load_warnings {
        lines.push(StatusLine {
            kind: StatusKind::LoadWarning,
            text: warn.message.clone(),
        });
    }
    lines
}

// ── Hero rate readout ────────────────────────────────────────────────────────

/// The hero spring-rate readout (label is constant in the view).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GoverningRate {
    pub value: String,
    pub unit: String,
}

impl GoverningRate {
    /// Build from a `SpringRate`, formatting to 4 decimal places in the active unit system.
    pub(crate) fn from_rate(rate: SpringRate, us: UnitSystem) -> Self {
        Self {
            value: format!("{:.4}", display_rate(rate, us)),
            unit: unit_rate_label(us).to_string(),
        }
    }
}

/// Append design-message status lines, mapping severity to [`StatusKind`].
///
/// Called at the end of every family's status-view function to add
/// engine-level messages after the shared action-error / load-warning prefix.
pub(crate) fn append_status_messages(lines: &mut Vec<StatusLine>, messages: &[StatusMessage]) {
    for msg in messages {
        lines.push(StatusLine {
            kind: status_kind(msg.severity),
            text: msg.message.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use springcore::{Angle, AngularRate, Force, Length, Moment, SpringRate, Stress, UnitSystem};

    // ── Unit conversions (the surface of the prior 1000× magnitude bug) ──

    #[test]
    fn length_conversion_matches_unit_system() {
        let one_mm = Length::from_millimeters(1.0);
        assert_relative_eq!(display_len(one_mm, UnitSystem::Metric), 1.0);
        assert_relative_eq!(
            display_len(one_mm, UnitSystem::Us),
            1.0 / 25.4,
            epsilon = 1e-9
        );
    }

    #[test]
    fn force_conversion_matches_unit_system() {
        // Each side built from its own native constructor, so a metric↔US
        // accessor swap is caught (not just a tautology against the impl).
        assert_relative_eq!(
            display_force(Force::from_newtons(10.0), UnitSystem::Metric),
            10.0
        );
        assert_relative_eq!(
            display_force(Force::from_pounds_force(7.0), UnitSystem::Us),
            7.0,
            epsilon = 1e-9
        );
    }

    #[test]
    fn rate_is_displayed_in_per_mm_not_per_meter() {
        // 2000 N/m stored must read as 2 N/mm — the magnitude that bit us before.
        assert_relative_eq!(
            display_rate(
                SpringRate::from_newtons_per_meter(2000.0),
                UnitSystem::Metric
            ),
            2.0
        );
        assert_relative_eq!(
            display_rate(SpringRate::from_pounds_per_inch(5.0), UnitSystem::Us),
            5.0,
            epsilon = 1e-9
        );
    }

    #[test]
    fn moment_conversion_matches_unit_system() {
        assert_relative_eq!(
            display_moment(Moment::from_newton_millimeters(100.0), UnitSystem::Metric),
            100.0
        );
        assert_relative_eq!(
            display_moment(Moment::from_pound_force_inches(1.0), UnitSystem::Us),
            1.0,
            epsilon = 1e-9
        );
    }

    #[test]
    fn angle_degrees_and_turns() {
        assert_relative_eq!(
            display_angle_degrees(Angle::from_degrees(90.0)),
            90.0,
            epsilon = 1e-9
        );
        assert_relative_eq!(
            display_angle_turns(Angle::from_turns(0.25)),
            0.25,
            epsilon = 1e-9
        );
    }

    #[test]
    fn ang_rate_per_deg_magnitude_pins_mm_per_m_scale() {
        // 1 N·m/° stored in SI must display as 1000 N·mm/° metric (same * MM_PER_M
        // pattern as display_rate). A dropped constant would produce 1.0 or 0.001.
        let r = AngularRate::from_newton_meters_per_degree(1.0);
        assert_relative_eq!(
            display_ang_rate_per_deg(r, UnitSystem::Metric),
            1000.0,
            epsilon = 1e-9
        );
        // US: no scale factor — must equal the native accessor value.
        assert_relative_eq!(
            display_ang_rate_per_deg(r, UnitSystem::Us),
            r.pound_force_inches_per_degree(),
            epsilon = 1e-9
        );
    }

    #[test]
    fn ang_rate_per_turn_magnitude_pins_mm_per_m_scale() {
        // Same contract: 1 N·m/turn → 1000 N·mm/turn metric.
        let r = AngularRate::from_newton_meters_per_turn(1.0);
        assert_relative_eq!(
            display_ang_rate_per_turn(r, UnitSystem::Metric),
            1000.0,
            epsilon = 1e-9
        );
        // US: no scale factor.
        assert_relative_eq!(
            display_ang_rate_per_turn(r, UnitSystem::Us),
            r.pound_force_inches_per_turn(),
            epsilon = 1e-9
        );
    }

    #[test]
    fn stress_conversion_carries_the_right_label() {
        let (v_metric, l_metric) =
            display_stress(Stress::from_megapascals(500.0), UnitSystem::Metric);
        assert_relative_eq!(v_metric, 500.0);
        assert_eq!(l_metric, "MPa");
        // 2000 psi = 2 ksi (independent magnitude, not a restatement of psi()/1000).
        let (v_us, l_us) = display_stress(Stress::from_psi(2000.0), UnitSystem::Us);
        assert_relative_eq!(v_us, 2.0, epsilon = 1e-9);
        assert_eq!(l_us, "ksi");
    }
}
