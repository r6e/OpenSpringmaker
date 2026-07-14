//! Shared dimension-presenter helpers used by the helical families.
use crate::diagram::{DimKind, DimLayer, Dimension, P2};

/// Format a millimetre value; em dash for non-finite (no NaN/inf label).
pub fn mm(v: f64) -> String {
    if v.is_finite() {
        format!("{v:.1}")
    } else {
        "\u{2014}".into()
    }
}

/// Coil-count note: "N {active} active / {total} total" in the Coils layer.
pub fn coil_note(active: f64, total: f64, at: P2) -> Dimension {
    Dimension {
        kind: DimKind::Note,
        layer: DimLayer::Coils,
        value: active,
        label: format!("N {} active / {} total", mm(active), mm(total)),
        at,
    }
}

/// Wire-diameter note in the Diameters layer.
///
/// SYMBOL-FIRST label ("⌀2.0 wire"): compression's landed test searches
/// `find(&dims, "⌀")` with `starts_with`, so the diameter symbol MUST lead.
/// This also unifies the label with conical (whose previous "wire ⌀" drift is
/// untested and folds into this shared convention).
pub fn wire_note(wire: f64, at: P2) -> Dimension {
    Dimension {
        kind: DimKind::Note,
        layer: DimLayer::Diameters,
        value: wire,
        label: format!("\u{2300}{} wire", mm(wire)),
        at,
    }
}

/// A diameter callout spanning ±value/2 at an axial station, in the
/// Diameters layer.
pub fn diameter(at_axial: f64, value: f64, label: String) -> Dimension {
    Dimension {
        kind: DimKind::Diameter {
            at_axial,
            half: value / 2.0,
        },
        layer: DimLayer::Diameters,
        value,
        label,
        at: (at_axial, value / 2.0),
    }
}

/// An axial length dim over `[0, to]` in the Lengths layer, labelled at its
/// midpoint.
pub fn axial_length(to: f64, label: String) -> Dimension {
    Dimension {
        kind: DimKind::Linear {
            from: (0.0, 0.0),
            to: (to, 0.0),
        },
        layer: DimLayer::Lengths,
        value: to,
        label,
        at: (to / 2.0, 0.0),
    }
}

/// A free-length linear dimension along the axis, `[0, l0]`.
pub fn free_length(l0: f64) -> Dimension {
    axial_length(l0, format!("L\u{2080} {}", mm(l0)))
}
