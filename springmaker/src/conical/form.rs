//! Conical form state, parsing, and solve routing (PowerUser only).
//! iced-free per ADR 0008.

use springcore::conical::{solve_forward, ConicalDesign, ConicalInputs};
use springcore::units::{Force, Length};
use springcore::{
    parse_end_type, ConicalSpec, CurvatureCorrection, MaterialStore, Result, UnitSystem,
};

use crate::form_helpers::{
    fmt_len, fmt_loads, length_mm, loads_n, optional_non_negative_num, positive_num,
};

/// A form input field (one per text input; the end-type picker is separate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    WireDia,
    LargeMeanDia,
    SmallMeanDia,
    Active,
    FreeLength,
    Loads,
    Inactive,
}

/// Conical form state. `end_type` holds the persisted key
/// ("plain" | "plain_ground" | "squared" | "squared_ground").
#[derive(Debug, Clone)]
pub struct ConFormState {
    pub end_type: String,
    pub wire_dia: String,
    pub large_mean_dia: String,
    pub small_mean_dia: String,
    pub active: String,
    pub free_length: String,
    /// Comma-separated loads (compression's idiom).
    pub loads: String,
    /// Optional inactive-coil override; blank defers to the end-type's default.
    pub inactive: String,
}

impl Default for ConFormState {
    fn default() -> Self {
        Self {
            end_type: "squared_ground".into(),
            wire_dia: String::new(),
            large_mean_dia: String::new(),
            small_mean_dia: String::new(),
            active: String::new(),
            free_length: String::new(),
            loads: String::new(),
            inactive: String::new(),
        }
    }
}

impl ConFormState {
    /// Blank when every text input is empty (the end-type selector holds a
    /// default and does not count).
    pub fn is_blank(&self) -> bool {
        [
            &self.wire_dia,
            &self.large_mean_dia,
            &self.small_mean_dia,
            &self.active,
            &self.free_length,
            &self.loads,
        ]
        .iter()
        .all(|f| f.trim().is_empty())
    }
}

/// A successful conical solve.
#[derive(Debug, Clone)]
pub struct ConFormOutcome {
    pub design: ConicalDesign,
}

/// Parse the form and solve. Takes the app-global curvature correction (the
/// compression pattern — torsion's solver takes none; documented divergence).
pub fn parse_and_solve(
    form: &ConFormState,
    material_name: &str,
    us: UnitSystem,
    materials: &MaterialStore,
    correction: CurvatureCorrection,
) -> Result<ConFormOutcome> {
    let inputs = ConicalInputs {
        wire_dia: Length::from_millimeters(length_mm("wire diameter", &form.wire_dia, us)?),
        large_mean_dia: Length::from_millimeters(length_mm(
            "large mean diameter",
            &form.large_mean_dia,
            us,
        )?),
        small_mean_dia: Length::from_millimeters(length_mm(
            "small mean diameter",
            &form.small_mean_dia,
            us,
        )?),
        active_coils: positive_num("active coils", &form.active)?,
        free_length: Length::from_millimeters(length_mm("free length", &form.free_length, us)?),
        end_type: parse_end_type(&form.end_type)?,
        inactive_coils: optional_non_negative_num("inactive coils", &form.inactive)?,
    };
    let loads: Vec<Force> = loads_n(&form.loads, us)?
        .into_iter()
        .map(Force::from_newtons)
        .collect();
    let material = materials.get(material_name)?;
    let design = solve_forward(material, &inputs, &loads, correction)?;
    Ok(ConFormOutcome { design })
}

/// Build the persisted spec from the form (SI millimetres / newtons).
pub fn build_spec(form: &ConFormState, us: UnitSystem) -> Result<ConicalSpec> {
    Ok(ConicalSpec::PowerUser {
        end_type: form.end_type.clone(),
        wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
        large_mean_dia_mm: length_mm("large mean diameter", &form.large_mean_dia, us)?,
        small_mean_dia_mm: length_mm("small mean diameter", &form.small_mean_dia, us)?,
        active: positive_num("active coils", &form.active)?,
        free_length_mm: length_mm("free length", &form.free_length, us)?,
        loads_n: loads_n(&form.loads, us)?,
        inactive_coils: optional_non_negative_num("inactive coils", &form.inactive)?,
    })
}

/// Fill the form from a loaded spec (round-trips with `build_spec`).
pub fn populate_from_spec(form: &mut ConFormState, spec: &ConicalSpec, us: UnitSystem) {
    match spec {
        ConicalSpec::PowerUser {
            end_type,
            wire_dia_mm,
            large_mean_dia_mm,
            small_mean_dia_mm,
            active,
            free_length_mm,
            loads_n,
            inactive_coils,
        } => {
            form.end_type = end_type.clone();
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.large_mean_dia = fmt_len(*large_mean_dia_mm, us);
            form.small_mean_dia = fmt_len(*small_mean_dia_mm, us);
            form.active = active.to_string();
            form.free_length = fmt_len(*free_length_mm, us);
            form.loads = fmt_loads(loads_n, us);
            form.inactive = inactive_coils.map(|v| format!("{v}")).unwrap_or_default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use springcore::MaterialSet;

    fn metric_form() -> ConFormState {
        ConFormState {
            end_type: "squared_ground".into(),
            wire_dia: "2".into(),
            large_mean_dia: "20".into(),
            small_mean_dia: "12".into(),
            active: "10".into(),
            free_length: "60".into(),
            loads: "10, 25".into(),
            inactive: String::new(),
        }
    }

    fn store() -> MaterialStore {
        MaterialStore::new(MaterialSet::load_default())
    }

    #[test]
    fn golden_through_form_matches_direct_engine_solve() {
        let outcome = parse_and_solve(
            &metric_form(),
            "Music Wire",
            UnitSystem::Metric,
            &store(),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        // Direct engine solve with identical inputs.
        let materials = store();
        let material = materials.get("Music Wire").unwrap();
        let inputs = springcore::conical::ConicalInputs {
            wire_dia: springcore::units::Length::from_millimeters(2.0),
            large_mean_dia: springcore::units::Length::from_millimeters(20.0),
            small_mean_dia: springcore::units::Length::from_millimeters(12.0),
            active_coils: 10.0,
            free_length: springcore::units::Length::from_millimeters(60.0),
            end_type: springcore::EndType::SquaredGround,
            inactive_coils: None,
        };
        let direct = springcore::conical::solve_forward(
            material,
            &inputs,
            &[
                springcore::units::Force::from_newtons(10.0),
                springcore::units::Force::from_newtons(25.0),
            ],
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert_relative_eq!(
            outcome.design.rate.newtons_per_meter(),
            direct.rate.newtons_per_meter(),
            max_relative = 1e-12
        );
        assert_relative_eq!(
            outcome.design.load_points[0].shear_stress.pascals(),
            direct.load_points[0].shear_stress.pascals(),
            max_relative = 1e-12
        );
        assert_eq!(outcome.design.load_points.len(), 2);
    }

    #[test]
    fn correction_selection_changes_through_form_stress() {
        let mk = |corr| {
            parse_and_solve(
                &metric_form(),
                "Music Wire",
                UnitSystem::Metric,
                &store(),
                corr,
            )
            .unwrap()
            .design
            .load_points[0]
                .shear_stress
                .pascals()
        };
        let wahl = mk(CurvatureCorrection::Wahl);
        let berg = mk(CurvatureCorrection::Bergstrasser);
        assert!(wahl > berg, "Wahl exceeds Bergsträsser at C=10");
    }

    #[test]
    fn build_populate_round_trips_metric_and_us() {
        for us in [UnitSystem::Metric, UnitSystem::Us] {
            let mut original = metric_form();
            if us == UnitSystem::Us {
                // US displays inches; use plain numerics that parse either way.
                original.wire_dia = "0.08".into();
                original.large_mean_dia = "0.8".into();
                original.small_mean_dia = "0.5".into();
                original.free_length = "2.4".into();
                original.loads = "2, 5".into();
            }
            let spec = build_spec(&original, us).unwrap();
            let mut round = ConFormState::default();
            populate_from_spec(&mut round, &spec, us);
            let spec2 = build_spec(&round, us).unwrap();
            assert_eq!(spec, spec2, "spec round-trip must be lossless ({us:?})");
        }
    }

    #[test]
    fn is_blank_matrix() {
        assert!(ConFormState::default().is_blank());
        for field in [
            Field::WireDia,
            Field::LargeMeanDia,
            Field::SmallMeanDia,
            Field::Active,
            Field::FreeLength,
            Field::Loads,
        ] {
            let mut f = ConFormState::default();
            match field {
                Field::WireDia => f.wire_dia = "1".into(),
                Field::LargeMeanDia => f.large_mean_dia = "1".into(),
                Field::SmallMeanDia => f.small_mean_dia = "1".into(),
                Field::Active => f.active = "1".into(),
                Field::FreeLength => f.free_length = "1".into(),
                Field::Loads => f.loads = "1".into(),
                Field::Inactive => f.inactive = "1".into(),
            }
            assert!(!f.is_blank(), "{field:?} alone must trip is_blank");
        }
        // The end-type selector alone does NOT count.
        let f = ConFormState {
            end_type: "plain".into(),
            ..ConFormState::default()
        };
        assert!(f.is_blank());
    }

    #[test]
    fn inactive_alone_does_not_count_toward_is_blank() {
        // The optional inactive-coil override, like the end-type selector,
        // is not one of the required fields — filling it alone must not
        // clear the blank state (compression's `Field::Inactive` pattern).
        let f = ConFormState {
            inactive: "3".into(),
            ..ConFormState::default()
        };
        assert!(f.is_blank());
    }

    #[test]
    fn parse_errors_carry_field_prefixes() {
        type ErrorCase = (fn(&mut ConFormState), &'static str);
        let cases: &[ErrorCase] = &[
            (|f| f.wire_dia = "x".into(), "wire diameter"),
            (|f| f.large_mean_dia = "x".into(), "large mean diameter"),
            (|f| f.small_mean_dia = "x".into(), "small mean diameter"),
            (|f| f.active = "0".into(), "active coils"),
            (|f| f.free_length = "-1".into(), "free length"),
            (|f| f.loads = "10, x".into(), "load"),
        ];
        for (mutate, needle) in cases {
            let mut form = metric_form();
            mutate(&mut form);
            let err = parse_and_solve(
                &form,
                "Music Wire",
                UnitSystem::Metric,
                &store(),
                CurvatureCorrection::Bergstrasser,
            )
            .unwrap_err();
            assert!(
                err.to_string().contains(needle),
                "expected '{needle}' in: {err}"
            );
        }
    }

    #[test]
    fn conical_inactive_round_trips_and_solves() {
        let mut form = ConFormState {
            inactive: "4".into(), // end_type squared_ground, active 10 (metric_form)
            ..metric_form()
        };
        let spec = build_spec(&form, UnitSystem::Metric).unwrap();
        assert!(
            matches!(spec, ConicalSpec::PowerUser { inactive_coils: Some(v), .. } if (v - 4.0).abs() < 1e-9)
        );
        form.inactive.clear();
        populate_from_spec(&mut form, &spec, UnitSystem::Metric);
        assert_eq!(form.inactive, "4");
        let materials = store();
        let out = parse_and_solve(
            &form,
            "Music Wire",
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert_relative_eq!(out.design.total_coils, 14.0, max_relative = 1e-9); // active 10 + inactive 4
    }

    #[test]
    fn unknown_end_type_key_errors() {
        let mut form = metric_form();
        form.end_type = "SquaredGround".into(); // PascalCase is NOT a valid key
        let err = parse_and_solve(
            &form,
            "Music Wire",
            UnitSystem::Metric,
            &store(),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap_err();
        assert!(err.to_string().contains("unknown end_type"), "got: {err}");
    }
}
