//! 2D engineering-diagram visual mode (ADR 0008): pure projection
//! (`geometry`) + pure layout (`layout`) feeding the humble `canvas`.
pub mod geometry;
pub mod layout;

// Re-exports consumed by a later diagram task (layout + humble canvas); Task 1
// ships the projection API ahead of its first caller.
#[allow(unused_imports)]
pub use geometry::{project_silhouette, Bounds, Edge2, Projected, P2};
// Re-export consumed by the humble canvas in Task 4.
#[allow(unused_imports)]
pub use layout::{layout, LayoutedDim};

/// Which toggleable layer a dimension belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // consumed by the layout engine in Task 3
pub enum DimLayer {
    Lengths,
    Diameters,
    Coils,
}

/// The geometric primitive a dimension draws as. Coordinates are model mm in
/// projection space `(axial, radial)`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)] // consumed by the layout engine in Task 3
pub enum DimKind {
    /// Distance between two anchor points (measured along the line joining them).
    Linear { from: P2, to: P2 },
    /// A diameter across the envelope at axial station `at_axial`, full span `2*half`.
    Diameter { at_axial: f64, half: f64 },
    /// Angular measurement: `sweep_deg` from `start_deg`, drawn at arc `radius`.
    Angular {
        vertex: P2,
        start_deg: f64,
        sweep_deg: f64,
        radius: f64,
    },
    /// Text-only annotation placed at `at` by the layout engine (no line).
    Note,
}

/// One callout: geometry (`kind`), which layer it toggles with, the numeric
/// `value` from the design field (the label source), the formatted `label`,
/// and a reference anchor `at` on the geometry (leader origin for `Note`s).
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)] // consumed by the layout engine in Task 3
pub struct Dimension {
    pub kind: DimKind,
    pub layer: DimLayer,
    pub value: f64,
    pub label: String,
    pub at: P2,
}

/// Which dimension layers are currently shown (app state; toggled in the UI).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // consumed by the UI toggle in Task 5
pub struct DimLayers {
    pub lengths: bool,
    pub diameters: bool,
    pub coils: bool,
}

impl Default for DimLayers {
    fn default() -> Self {
        Self {
            lengths: true,
            diameters: true,
            coils: true,
        }
    }
}

impl DimLayers {
    /// Whether a dimension's layer is currently visible.
    #[allow(dead_code)] // consumed by the layout engine in Task 3
    pub fn shows(&self, layer: DimLayer) -> bool {
        match layer {
            DimLayer::Lengths => self.lengths,
            DimLayer::Diameters => self.diameters,
            DimLayer::Coils => self.coils,
        }
    }
}
