//! Pure dimension layout (ADR 0008): given the model-mm `Bounds` of the drawn
//! geometry and the callouts, place dimension lines, extension lines,
//! arrowheads, and text anchors — all in model mm. Linear length dims stack in
//! ladders below the envelope; diameter dims span the envelope and sit to its
//! right; angular dims carry an arc. The humble canvas applies the only affine.

use crate::diagram::{Bounds, DimKind, DimLayers, Dimension, P2};

/// A dimension resolved to drawable primitives, all in model mm. Consumed by
/// the humble canvas, which applies the single affine to screen space.
pub struct LayoutedDim {
    pub lines: Vec<(P2, P2)>,
    pub arrows: Vec<(P2, f64)>,
    pub arc: Option<(P2, f64, f64, f64)>,
    pub text: (P2, String),
}

/// Radial gap between successive ladder rungs (model mm, scaled by the view).
const RUNG_STEP: f64 = 6.0;
/// Gap from the envelope to the first ladder rung.
const LADDER_GAP: f64 = 8.0;

fn arrow_dir(from: P2, to: P2) -> f64 {
    (to.1 - from.1).atan2(to.0 - from.0)
}

/// Two arrowheads at the ends of a dimension line, each pointing inward toward
/// the opposite end (the standard "arrows meeting in the middle" convention).
fn end_arrows(a: P2, b: P2) -> Vec<(P2, f64)> {
    vec![(a, arrow_dir(b, a)), (b, arrow_dir(a, b))]
}

/// Place every visible dimension's drawable primitives in model mm. Purely
/// geometric: no frame/screen coordinates enter here (ADR 0008) — the humble
/// canvas applies the single affine afterward.
pub fn layout(dims: &[Dimension], bounds: &Bounds, active: DimLayers) -> Vec<LayoutedDim> {
    let mut out = Vec::new();
    let mut length_rung = 0usize; // ladder index for axial length dims
    let mut diameter_rung = 0usize;

    for d in dims.iter().filter(|d| active.shows(d.layer)) {
        match d.kind {
            DimKind::Linear { from, to } => {
                // Drop the dimension line onto a ladder rung below the envelope.
                let r = bounds.radial_min - LADDER_GAP - RUNG_STEP * length_rung as f64;
                length_rung += 1;
                let a = (from.0, r);
                let b = (to.0, r);
                out.push(LayoutedDim {
                    lines: vec![
                        (a, b),    // dimension line
                        (from, a), // extension line from geometry
                        (to, b),   // extension line from geometry
                    ],
                    arrows: end_arrows(a, b),
                    arc: None,
                    text: (((a.0 + b.0) / 2.0, r), d.label.clone()),
                });
            }
            DimKind::Diameter { at_axial, half } => {
                // Full radial span at the station; text parked to the right.
                let a = (at_axial, -half);
                let b = (at_axial, half);
                let text_x = bounds.axial_max + LADDER_GAP + RUNG_STEP * diameter_rung as f64;
                diameter_rung += 1;
                out.push(LayoutedDim {
                    lines: vec![(a, b), (b, (text_x, half))],
                    arrows: end_arrows(a, b),
                    arc: None,
                    text: ((text_x, half), d.label.clone()),
                });
            }
            DimKind::Angular {
                vertex,
                start_deg,
                sweep_deg,
                radius,
            } => {
                let mid = (start_deg + sweep_deg / 2.0).to_radians();
                let text_at = (vertex.0 + radius * mid.cos(), vertex.1 + radius * mid.sin());
                out.push(LayoutedDim {
                    lines: Vec::new(),
                    arrows: Vec::new(),
                    arc: Some((vertex, radius, start_deg, sweep_deg)),
                    text: (text_at, d.label.clone()),
                });
            }
            DimKind::Note => {
                out.push(LayoutedDim {
                    lines: Vec::new(),
                    arrows: Vec::new(),
                    arc: None,
                    text: (d.at, d.label.clone()),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram::{Bounds, DimKind, DimLayer, DimLayers, Dimension};
    use approx::assert_relative_eq;

    fn bounds() -> Bounds {
        Bounds {
            axial_min: 0.0,
            axial_max: 60.0,
            radial_min: -11.0,
            radial_max: 11.0,
        }
    }

    fn linear(label: &str, layer: DimLayer, to: f64) -> Dimension {
        Dimension {
            kind: DimKind::Linear {
                from: (0.0, 0.0),
                to: (to, 0.0),
            },
            layer,
            value: to,
            label: label.into(),
            at: (to / 2.0, 0.0),
        }
    }

    #[test]
    fn hidden_layers_produce_no_layouted_dims() {
        let dims = vec![linear("L\u{2080}", DimLayer::Lengths, 60.0)];
        let out = layout(
            &dims,
            &bounds(),
            DimLayers {
                lengths: false,
                diameters: true,
                coils: true,
            },
        );
        assert!(out.is_empty());
    }

    #[test]
    fn linear_length_dims_stack_in_ladders_clear_of_the_envelope() {
        let dims = vec![
            linear("L\u{2080}", DimLayer::Lengths, 60.0),
            linear("L\u{209B}", DimLayer::Lengths, 26.0),
        ];
        let out = layout(&dims, &bounds(), DimLayers::default());
        assert_eq!(out.len(), 2);
        // Length ladders sit below the envelope (radial < radial_min) and at
        // increasing offsets so they never overlap.
        let rungs: Vec<f64> = out.iter().map(|d| d.text.0 .1).collect();
        assert!(rungs.iter().all(|&r| r < bounds().radial_min));
        assert!(
            (rungs[0] - rungs[1]).abs() > 1e-6,
            "ladder rungs must differ"
        );
    }

    #[test]
    fn diameter_dims_span_the_full_envelope_and_place_text_to_the_side() {
        // half = 8.0 is deliberately distinct from bounds().radial_max (11.0) so
        // the span assertion proves layout() uses the dim's own `half` (design
        // OD/2), not the projected envelope bounds.
        let dims = vec![Dimension {
            kind: DimKind::Diameter {
                at_axial: 30.0,
                half: 8.0,
            },
            layer: DimLayer::Diameters,
            value: 16.0,
            label: "OD 16.0".into(),
            at: (30.0, 8.0),
        }];
        let out = layout(&dims, &bounds(), DimLayers::default());
        assert_eq!(out.len(), 1);
        // The diameter line spans -half..+half in radial at its axial station.
        let spans_half = out[0].lines.iter().any(|(a, b)| {
            (a.1 - (-8.0)).abs() < 1e-6 && (b.1 - 8.0).abs() < 1e-6 && (a.0 - b.0).abs() < 1e-6
        });
        assert!(
            spans_half,
            "diameter line must span the dim's own half, not the envelope"
        );
        // Text is parked to the side of the envelope, past its axial extent.
        assert!(
            out[0].text.0 .0 > bounds().axial_max,
            "diameter text must be placed to the side of the envelope"
        );
    }

    #[test]
    fn angular_dims_carry_an_arc() {
        let dims = vec![Dimension {
            kind: DimKind::Angular {
                vertex: (0.0, 0.0),
                start_deg: 0.0,
                sweep_deg: 90.0,
                radius: 8.0,
            },
            layer: DimLayer::Coils,
            value: 90.0,
            label: "90\u{00b0}".into(),
            at: (0.0, 0.0),
        }];
        let out = layout(&dims, &bounds(), DimLayers::default());
        let (_, r, _, sweep) = out[0].arc.expect("angular dim carries an arc");
        assert_relative_eq!(r, 8.0, max_relative = 1e-9);
        assert_relative_eq!(sweep, 90.0, max_relative = 1e-9);
    }

    #[test]
    fn note_dims_are_text_only_at_their_anchor() {
        let dims = vec![Dimension {
            kind: DimKind::Note,
            layer: DimLayer::Coils,
            value: 10.0,
            label: "N 10 active / 12 total".into(),
            at: (25.0, 3.0),
        }];
        let out = layout(&dims, &bounds(), DimLayers::default());
        assert_eq!(out.len(), 1);
        // A note draws no geometry — only its text, parked at the dim's anchor.
        assert!(out[0].lines.is_empty());
        assert!(out[0].arrows.is_empty());
        assert!(out[0].arc.is_none());
        assert_eq!(out[0].text.0, (25.0, 3.0));
        assert_eq!(out[0].text.1, "N 10 active / 12 total");
    }

    #[test]
    fn all_layers_off_produces_no_layouted_dims() {
        let dims = vec![
            linear("L\u{2080}", DimLayer::Lengths, 60.0),
            Dimension {
                kind: DimKind::Diameter {
                    at_axial: 30.0,
                    half: 8.0,
                },
                layer: DimLayer::Diameters,
                value: 16.0,
                label: "OD 16.0".into(),
                at: (30.0, 8.0),
            },
            Dimension {
                kind: DimKind::Note,
                layer: DimLayer::Coils,
                value: 10.0,
                label: "N 10".into(),
                at: (25.0, 0.0),
            },
        ];
        let out = layout(
            &dims,
            &bounds(),
            DimLayers {
                lengths: false,
                diameters: false,
                coils: false,
            },
        );
        assert!(out.is_empty());
    }
}
