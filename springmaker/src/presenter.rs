//! Family-agnostic presenter vocabulary: the plain-data types a humble view
//! renders. iced-free, so every type is unit-testable without a renderer and
//! reusable by any spring family's presenter. Family-specific presenter
//! functions and result aggregates live in each family's `view_model`.

use crate::form_helpers::MM_PER_M;
use springcore::{Force, Length, SpringRate, Stress, UnitSystem};

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

// ── Hero rate readout ────────────────────────────────────────────────────────

/// The hero spring-rate readout (label is constant in the view).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GoverningRate {
    pub value: String,
    pub unit: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use springcore::{Force, Length, SpringRate, Stress, UnitSystem};

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
