//! Family-agnostic presenter vocabulary: the plain-data types a humble view
//! renders. iced-free, so every type is unit-testable without a renderer and
//! reusable by any spring family's presenter. Family-specific presenter
//! functions and result aggregates live in each family's `view_model`.

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
