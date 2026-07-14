//! Pure 2D-diagram dimension presenter for the extension family (ADR 0008).
//! Anchors are in projection space `(axial, radial)` model mm. The free
//! length is anchored to the **drawn inside-hooks span** (matches
//! `scene_model::extension_scene`'s hook geometry), not `[0, l0]` like
//! compression. The L₀ callout's value is the design free length; its span is
//! the drawn hooks — the two coincide except in the close-wound clamp band
//! (see `dimensions`), where L₀ is a reference dim like the assembly overall.

use crate::diagram::{common, DimKind, DimLayer, Dimension};
use crate::viz::coil_render_height;
use crate::viz::sdf::extension_body_pitch_mm;
use springcore::extension::ExtensionDesign;

pub fn dimensions(design: &ExtensionDesign) -> Vec<Dimension> {
    let wire = design.wire_dia.millimeters();
    let r1 = design.hooks.r1.millimeters();
    let l0 = design.free_length.millimeters();
    let od = design.outer_dia.millimeters();
    let id = design.inner_dia.millimeters();
    let na = design.active_coils;
    // Body height AS DRAWN by extension_scene: it renders `na` turns at the
    // shared close-wound-clamped body pitch (`viz::sdf::extension_body_pitch_mm`,
    // via the same `coil_render_height` the other families anchor on). The raw
    // inside-hooks relation `l0 − 2·(2·r1 − wire) − wire` equals this only ABOVE
    // the rate-equivalent close-wound length (`free_length_from_geometry` at Na).
    // Between the PHYSICAL close-wound minimum (Nb = Na − G/E, what the engine
    // accepts down to) and that rate-equivalent length, the scene clamps the
    // body to close-wound while the raw relation keeps shrinking — so anchoring
    // on the raw relation floats the callouts off the drawn body. Anchor on the
    // drawn height instead, exactly as compression/conical/assembly do.
    let body_h = coil_render_height(na, na, extension_body_pitch_mm(design), wire);
    let bottom_inner = -2.0 * r1 + wire / 2.0;
    let top_inner = body_h + 2.0 * r1 - wire / 2.0;
    let fi = design.initial_tension.newtons();

    vec![
        // Free length: value is the design free length; the drawn span is the
        // inside-hooks span (bottom→top hook inner surfaces of the DRAWN body).
        // These coincide except in the close-wound clamp band, where this is a
        // reference dim (value = spec free length, span = the drawn hooks).
        Dimension {
            kind: DimKind::Linear {
                from: (bottom_inner, 0.0),
                to: (top_inner, 0.0),
            },
            layer: DimLayer::Lengths,
            value: l0,
            label: format!("L\u{2080} {}", common::mm(l0)),
            at: ((bottom_inner + top_inner) / 2.0, 0.0),
        },
        // Body length.
        common::axial_length(body_h, format!("body {}", common::mm(body_h))),
        // Hook opening = loop inside diameter. NOT a `common::diameter` fold:
        // its anchor is `(bottom_inner, r1)`, not `(bottom_inner, half)` —
        // `r1 != (2*r1 - wire)/2` for a nonzero wire.
        Dimension {
            kind: DimKind::Diameter {
                at_axial: bottom_inner,
                half: (2.0 * r1 - wire) / 2.0,
            },
            layer: DimLayer::Diameters,
            value: 2.0 * r1 - wire,
            label: format!("hook \u{2300}{}", common::mm(2.0 * r1 - wire)),
            at: (bottom_inner, r1),
        },
        common::diameter(body_h / 2.0, od, format!("OD {}", common::mm(od))),
        common::diameter(body_h / 2.0, id, format!("ID {}", common::mm(id))),
        common::wire_note(wire, (body_h / 2.0, od / 2.0)),
        common::coil_note(na, na, (body_h / 2.0, 0.0)), // extension body: active ≈ total
        Dimension {
            kind: DimKind::Note,
            layer: DimLayer::Coils,
            value: fi,
            label: format!("F\u{1d62} {}N", common::mm(fi)),
            at: (body_h / 2.0, 0.0),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram::project_silhouette;
    use crate::diagram::test_support::find;
    use crate::extension::form::{parse_and_solve, ExtFormState, HookMode};
    use crate::extension::scene_model::extension_scene;
    use approx::assert_relative_eq;
    use springcore::{CurvatureCorrection, MaterialSet, MaterialStore, UnitSystem};

    fn design() -> springcore::extension::ExtensionDesign {
        build(HookMode::Default, "10", "5", "100")
    }

    /// A non-default hook radius (r1 = 14, vs the default D/2 = 10) so the
    /// no-drift lock test exercises r1-generality — the whole
    /// presenter-matches-scene claim rests on `free_length_from_geometry` being
    /// general in r1, which a default-r1 fixture cannot show.
    fn custom_hook_design() -> springcore::extension::ExtensionDesign {
        build(HookMode::Custom, "14", "7", "100")
    }

    /// A free length inside the close-wound clamp band — above the PHYSICAL
    /// close-wound minimum the engine accepts down to (57.2134mm for this
    /// fixture, pinned in `springcore::extension::design`'s
    /// `accepts_free_length_just_above_close_wound_minimum`) but below the
    /// rate-equivalent close-wound length (58mm) where the scene clamps the body
    /// to close-wound. Here the drawn body ≠ the raw inside-hooks relation.
    fn clamp_band_design() -> springcore::extension::ExtensionDesign {
        build(HookMode::Default, "10", "5", "57.62")
    }

    fn build(
        hook_mode: HookMode,
        hook_r1: &str,
        hook_r2: &str,
        free_length: &str,
    ) -> springcore::extension::ExtensionDesign {
        let materials = MaterialStore::new(MaterialSet::load_default());
        let form = ExtFormState {
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            active: "10".into(),
            free_length: free_length.into(),
            initial_tension: "5".into(),
            loads: "10, 30".into(),
            hook_mode,
            hook_r1: hook_r1.into(),
            hook_r2: hook_r2.into(),
            ..Default::default()
        };
        parse_and_solve(
            &form,
            "Music Wire",
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .unwrap()
        .design
    }

    /// The y-center of the coil body AS DRAWN by `extension_scene` (polyline 0
    /// is the body; the two hooks are polylines 1–2) — INDEPENDENTLY sampled
    /// from the scene (via the shared `test_support::polyline_y_center`), not
    /// re-derived from `body_h`.
    fn drawn_body_center(d: &springcore::extension::ExtensionDesign) -> f64 {
        crate::diagram::test_support::polyline_y_center(&extension_scene(d).polylines[0])
    }

    #[test]
    fn free_length_spans_inside_hooks_matching_the_projection() {
        let d = design();
        let dims = dimensions(&d);
        let fl = find(&dims, "L\u{2080}");
        // (1) EXACT: presenter value == design free_length.
        assert_relative_eq!(fl.value, d.free_length.millimeters(), max_relative = 1e-9);
        // (2) EXACT above the clamp band: the presenter's Linear span == free_length.
        //     |top_inner - bottom_inner| = body_h + 4·r1 - wire. body_h is now the
        //     DRAWN height (coil_render_height over extension_body_pitch_mm); above
        //     the rate-equivalent close-wound length it COINCIDES with the raw
        //     relation free_length - 4·r1 + wire, so the span is free_length exactly.
        //     This fixture (free_length 100) sits there; inside the clamp band L₀ is
        //     a reference dim (span = drawn hooks > value) — see the clamp-band test.
        if let crate::diagram::DimKind::Linear { from, to } = fl.kind {
            assert_relative_eq!(
                (to.0 - from.0).abs(),
                d.free_length.millimeters(),
                max_relative = 1e-9
            );
        } else {
            panic!("free length must be Linear");
        }
        // (3) DROP-Z ENVELOPE (sampling-approximate, NOT 1e-9): the projected
        //     outer axial span ≈ free_length + 2·wire (hook outer surfaces). The
        //     ±wire/2 perpendicular offset + arc sampling perturb each hook tip by
        //     ≤ wire/2, so use a wire-scale tolerance — mirror the compression idiom
        //     in diagram/geometry.rs::axial_span_matches_free_length (see the
        //     PROJECTION MODEL footer: the envelope only approaches the ideal to
        //     sampling resolution; exact ties are presenter-value/algebraic, above).
        let p = project_silhouette(&extension_scene(&d)).unwrap();
        let span = p.bounds.axial_max - p.bounds.axial_min;
        let ideal = d.free_length.millimeters() + 2.0 * d.wire_dia.millimeters();
        assert!(
            (span - ideal).abs() <= d.wire_dia.millimeters(),
            "projected outer axial span {span} not within a wire dia of free_length + 2·wire ({ideal})"
        );
    }

    #[test]
    fn hook_opening_and_initial_tension_present() {
        let d = design();
        let dims = dimensions(&d);
        let opening = find(&dims, "hook");
        assert_relative_eq!(
            opening.value,
            2.0 * d.hooks.r1.millimeters() - d.wire_dia.millimeters(),
            max_relative = 1e-9
        );
        let fi = find(&dims, "F\u{1d62}"); // Fᵢ initial tension
        assert_relative_eq!(fi.value, d.initial_tension.newtons(), max_relative = 1e-9);
        assert_eq!(fi.layer, crate::diagram::DimLayer::Coils);
    }

    /// Mirrors compression's `degenerate_design_yields_finite_labels_only`:
    /// a post-solve NaN on a field the presenter actually reads for a label
    /// (`free_length` flows into the L₀ callout) must not crash the
    /// presenter — labels stay finite-guarded (em dash, never "NaN").
    #[test]
    fn degenerate_design_yields_finite_labels_only() {
        let mut d = design();
        d.free_length = springcore::units::Length::from_millimeters(f64::NAN);
        let dims = dimensions(&d);
        assert!(dims
            .iter()
            .all(|dm| dm.value.is_finite() || dm.label.contains('\u{2014}')));
        let fl = find(&dims, "L\u{2080}");
        assert!(!fl.value.is_finite());
        assert!(fl.label.contains('\u{2014}'));
    }

    /// The body OD/ID/wire/coil callouts sit at the axial center of the coil
    /// body AS DRAWN by `extension_scene`, for the default hook mode AND a
    /// non-default hook radius (r1-generality). Above the clamp band the drawn
    /// body equals the raw inside-hooks relation, so this also confirms the
    /// common case unchanged; the clamp-band test below covers where they
    /// diverge. Compared against the scene body's own y-center.
    #[test]
    fn body_callouts_track_the_drawn_body_across_hook_modes() {
        for d in [design(), custom_hook_design()] {
            let dims = dimensions(&d);
            let center = drawn_body_center(&d);
            // Substrings unique to the BODY callouts: "⌀"/"N" would collide via
            // `contains` with the "hook ⌀…" opening and the "Fᵢ …N" tension note.
            for label in ["OD", "ID", "wire", "active"] {
                assert_relative_eq!(find(&dims, label).at.0, center, max_relative = 1e-9);
            }
        }
    }

    /// The real drift the raw inside-hooks relation hides: in the close-wound
    /// clamp band (free length the engine ACCEPTS but where the scene clamps the
    /// body to close-wound), the callouts must track the DRAWN (clamped) body,
    /// not the raw `l0 − 2·(2·r1 − wire) − wire`. Anchoring on the drawn height
    /// (`coil_render_height` over `extension_body_pitch_mm`) fixes it. Compared
    /// against the scene body's own y-center; revert-probed RED on the raw
    /// relation, which drifts by (G/E)·wire/2 here.
    #[test]
    fn body_callouts_track_the_drawn_body_in_the_close_wound_clamp_band() {
        let d = clamp_band_design();
        let dims = dimensions(&d);
        let center = drawn_body_center(&d);
        // Precondition: the fixture really is in the clamp band — the drawn body
        // is close-wound (na·wire), strictly SHORTER than the raw relation, so a
        // raw anchor would genuinely drift (guards against a fixture that drifted
        // out of the band and made the test vacuous).
        let raw_body_h = d.free_length.millimeters()
            - 2.0 * (2.0 * d.hooks.r1.millimeters() - d.wire_dia.millimeters())
            - d.wire_dia.millimeters();
        assert_relative_eq!(
            2.0 * center,
            d.active_coils * d.wire_dia.millimeters(),
            max_relative = 1e-9
        );
        assert!(
            raw_body_h < 2.0 * center - 1e-6,
            "fixture left the clamp band: raw body_h {raw_body_h} not below drawn {}",
            2.0 * center
        );
        for label in ["OD", "ID", "wire", "active"] {
            assert_relative_eq!(find(&dims, label).at.0, center, max_relative = 1e-9);
        }
    }

    /// Parity with compression/conical (input-domain panel finding): the body
    /// callouts now route through `coil_render_height` via the derived
    /// `extension_body_pitch_mm` (which divides by `active_coils`), so a NaN in
    /// the coil-geometry fields must keep the BODY callouts (OD/ID/wire/coil, all
    /// at `body_h/2`) finite — the guards return 0.0. SCOPED to the body callouts:
    /// L₀/hook-opening anchor on `bottom_inner`/`top_inner`, which go non-finite
    /// under NaN `wire`/`r1` by design (the drop-z L₀ dim is dropped downstream by
    /// `layout`'s finiteness gate), so this must NOT assert those.
    #[test]
    fn degenerate_coil_geometry_yields_finite_body_callout_anchors() {
        use springcore::extension::ExtensionDesign;
        use springcore::units::Length;
        let check = |field: &str, mutate: fn(&mut ExtensionDesign)| {
            let mut d = design();
            mutate(&mut d);
            let dims = dimensions(&d);
            for label in ["OD", "ID", "wire", "active"] {
                let at = find(&dims, label).at;
                assert!(
                    at.0.is_finite() && at.1.is_finite(),
                    "NaN {field}: {label} anchor non-finite"
                );
            }
        };
        check("active", |d| d.active_coils = f64::NAN);
        check("wire", |d| d.wire_dia = Length::from_millimeters(f64::NAN));
    }
}
