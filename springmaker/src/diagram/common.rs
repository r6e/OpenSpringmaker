//! Shared dimension-presenter helpers used by the helical families.
use crate::diagram::{DimKind, DimLayer, Dimension, P2};

/// U+2014 EM DASH — the label placeholder for any non-finite value, so no
/// `NaN`/`inf` ever reaches a rendered label. Shared by [`mm`] and [`degrees`].
const EM_DASH: &str = "\u{2014}";

/// Format a finite scalar to one decimal place; em dash for non-finite (so no
/// `NaN`/`inf` ever reaches a label). Named for its commonest use — millimetre
/// callouts — but the same one-decimal, non-finite-guarded formatting also
/// backs dimensionless coil counts and forces (N).
pub fn mm(v: f64) -> String {
    if v.is_finite() {
        format!("{v:.1}")
    } else {
        EM_DASH.into()
    }
}

/// Format a finite angle as whole degrees with a `°` suffix; em dash for
/// non-finite (the same "no NaN/inf in labels" discipline as [`mm`], at the
/// zero-decimal precision that reads best for an angle). The lengths helper
/// [`mm`] uses one decimal, so degrees gets its own guarded formatter.
pub fn degrees(v: f64) -> String {
    if v.is_finite() {
        format!("{v:.0}\u{00b0}")
    } else {
        EM_DASH.into()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mm_formats_finite_and_em_dashes_non_finite() {
        assert_eq!(mm(2.0), "2.0");
        assert_eq!(mm(f64::NAN), "\u{2014}");
        assert_eq!(mm(f64::INFINITY), "\u{2014}");
    }

    #[test]
    fn degrees_formats_whole_degrees_and_em_dashes_non_finite() {
        assert_eq!(degrees(90.0), "90\u{00b0}");
        assert_eq!(degrees(0.0), "0\u{00b0}");
        assert_eq!(degrees(f64::NAN), "\u{2014}");
        assert_eq!(degrees(f64::INFINITY), "\u{2014}");
        assert_eq!(degrees(f64::NEG_INFINITY), "\u{2014}");
    }
}
