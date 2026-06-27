//! Family-agnostic presenter vocabulary: the plain-data types a humble view
//! renders. iced-free, so every type is unit-testable without a renderer and
//! reusable by any spring family's presenter. Family-specific presenter
//! functions and result aggregates live in each family's `view_model`.

use crate::app::Field;

// ── Results panel ───────────────────────────────────────────────────────────

/// Emphasis for a result value; the view maps this to a color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Emphasis {
    Normal,
    Danger,
}

/// A muted-label + value(+unit) readout row, with value emphasis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultRow {
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
pub struct LoadRow {
    pub point: String,
    pub force: String,
    pub deflection: String,
    pub length: String,
    pub stress: String,
    pub pct_mts: String,
}

/// The load-points table: a stress-unit header label plus per-point rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadTable {
    pub stress_unit: String,
    pub rows: Vec<LoadRow>,
}

// ── Status panel ────────────────────────────────────────────────────────────

/// Severity class of a status line; the view maps this to a prefix and color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    /// A failed save/load action ([`App::action_error`]).
    ActionError,
    /// Startup material-load warning (can appear before any design is solved).
    LoadWarning,
    Info,
    Caution,
    /// A design-level warning ([`Severity::Warning`]).
    DesignWarning,
}

/// One line in the status panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusLine {
    pub kind: StatusKind,
    pub text: String,
}

// ── Inputs panel ────────────────────────────────────────────────────────────

/// One input field: its label (with embedded unit) and the [`Field`] the view
/// binds it to. The current value is read from `app.form` by the view (iced's
/// `text_input` borrows its value, which must outlive this owned descriptor).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDescriptor {
    pub label: String,
    pub field: Field,
}

impl FieldDescriptor {
    pub(crate) fn new(label: impl Into<String>, field: Field) -> Self {
        Self {
            label: label.into(),
            field,
        }
    }
}
