//! 2D engineering-diagram visual mode (ADR 0008): pure projection
//! (`geometry`) + pure layout (`layout`) feeding the humble `canvas`.
pub mod canvas;
pub mod common;
pub mod geometry;
pub mod layout;

use crate::viz::SceneData;

// Re-export consumed by the humble canvas (`canvas::diagram_element`).
pub use geometry::{bounds_of, project_silhouette, Bounds, Edge2, Projected, P2};
// Re-export consumed by the humble canvas (`canvas::diagram_element`).
pub use layout::{layout, LayoutedDim};
// Re-export consumed by the results dispatch in Task 5.
#[allow(unused_imports)]
pub use canvas::{diagram_element, DiagramCanvas};

/// Which toggleable layer a dimension belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DimLayer {
    Lengths,
    Diameters,
    Coils,
}

/// The geometric primitive a dimension draws as. Coordinates are model mm in
/// projection space `(axial, radial)`.
#[derive(Debug, Clone, Copy, PartialEq)]
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
pub struct Dimension {
    pub kind: DimKind,
    pub layer: DimLayer,
    pub value: f64,
    pub label: String,
    pub at: P2,
}

/// Which dimension layers are currently shown (app state; toggled in the UI).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub fn shows(&self, layer: DimLayer) -> bool {
        match layer {
            DimLayer::Lengths => self.lengths,
            DimLayer::Diameters => self.diameters,
            DimLayer::Coils => self.coils,
        }
    }
}

/// View transform for the diagram (app state). `zoom` multiplies the
/// fit-to-canvas baseline; `pan` translates in screen px. Default = fit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiagramView {
    pub zoom: f32,
    pub pan: iced::Vector,
}

impl Default for DiagramView {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: iced::Vector::ZERO,
        }
    }
}

/// Zoom clamp bounds (mirrors the 3D `viz::zoom_step` discipline: a single
/// writer, finiteness-guarded, clamped).
const ZOOM_MIN: f32 = 0.2;
const ZOOM_MAX: f32 = 8.0;

/// Single writer for `DiagramView::zoom`. A non-finite delta is a no-op; the
/// result is clamped so no other code can push zoom out of range.
pub fn zoom_step(view: DiagramView, delta: f32) -> DiagramView {
    if !delta.is_finite() {
        return view;
    }
    let zoom = (view.zoom * (1.0 + delta * 0.1)).clamp(ZOOM_MIN, ZOOM_MAX);
    DiagramView { zoom, ..view }
}

/// Single writer for `DiagramView::pan`. Non-finite deltas are no-ops.
pub fn pan_step(view: DiagramView, dx: f32, dy: f32) -> DiagramView {
    if !dx.is_finite() || !dy.is_finite() {
        return view;
    }
    DiagramView {
        pan: view.pan + iced::Vector::new(dx, dy),
        ..view
    }
}

/// Optional secondary end-on projection (torsion legs). Empty for other
/// families.
pub struct Inset {
    pub edges: Vec<Edge2>,
    pub dims: Vec<Dimension>,
}

/// Everything the diagram needs for one family, built lazily by the caller.
pub struct DiagramInput {
    pub scene: SceneData,
    pub dims: Vec<Dimension>,
    pub inset: Option<Inset>,
}

impl DiagramInput {
    pub fn new(scene: SceneData, dims: Vec<Dimension>) -> Self {
        Self {
            scene,
            dims,
            inset: None,
        }
    }
    pub fn with_inset(mut self, inset: Inset) -> Self {
        self.inset = Some(inset);
        self
    }
}
