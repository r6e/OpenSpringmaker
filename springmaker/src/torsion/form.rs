//! Pure torsion form-to-design logic. No iced dependency.
use crate::form_helpers::{
    fmt_len, fmt_moments, length_mm, moments_nmm, non_negative_length_mm, positive_num,
};
use springcore::torsion::{FrictionModel, PowerUser, Scenario, TorsionDesign};
use springcore::units::{Length, Moment};
use springcore::{Material, MaterialStore, Result, TorsionSpec, UnitSystem};

/// Which torsion text field a `Message::TorField` targets.
// Variants are constructed by `torsion::view_model::tor_inputs_view` (Task 4) and
// `torsion::view` (Task 5). No dead_code annotation is needed: the variants are
// referenced in `tor_inputs_view`'s body and test code, keeping them "live."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    WireDia,
    MeanDia,
    BodyCoils,
    Leg1,
    Leg2,
    ArborDia,
    Moments,
}

/// Torsion form inputs as raw strings, plus the friction-model selector.
#[derive(Debug, Clone, Default)]
pub struct TorFormState {
    pub wire_dia: String,
    pub mean_dia: String,
    pub body_coils: String,
    pub leg1: String,
    pub leg2: String,
    pub arbor_dia: String,
    pub moments: String,
    pub friction_model: FrictionModel,
}

impl TorFormState {
    /// Whether the user has entered none of the input fields. Drives the
    /// "untouched form" suppression in `App::recompute`. All seven text fields
    /// count; `arbor_dia` and `moments` count when typed (typing signals intent,
    /// matching the `max_outer_dia`/`loads` convention). `friction_model` is
    /// excluded — it always holds a default and cannot distinguish a blank form.
    pub fn is_blank(&self) -> bool {
        [
            &self.wire_dia,
            &self.mean_dia,
            &self.body_coils,
            &self.leg1,
            &self.leg2,
            &self.arbor_dia,
            &self.moments,
        ]
        .iter()
        .all(|f| f.trim().is_empty())
    }
}

/// A solved torsion form: the design (which carries engine-computed status).
#[derive(Debug, Clone)]
pub struct TorFormOutcome {
    // Read by `torsion::view_model::tor_status_view` (now wired in Task 4) and by
    // tests; the `#[cfg_attr]` guard is no longer needed because `tor_status_view`
    // references `out.design` in non-test builds via `calculator::status_panel`.
    pub design: TorsionDesign,
}

/// Parse the comma-separated moment list into SI newton-millimetres, rejecting an
/// empty list at the form boundary. Shared by `parse_and_solve` and `build_spec` so
/// neither a vacuous solve nor an unsolvable persisted spec can slip past the engine
/// backstop. Mirrors extension's `parse_candidate_diameters_mm`.
fn parse_moments_nmm_nonempty(form: &TorFormState, us: UnitSystem) -> Result<Vec<f64>> {
    let moments = moments_nmm(&form.moments, us)?;
    if moments.is_empty() {
        return Err(springcore::SpringError::InconsistentInputs(
            "provide at least one applied moment".into(),
        ));
    }
    Ok(moments)
}

/// Parse the optional arbor field: empty → None; non-empty → a positive length.
fn parse_arbor(form: &TorFormState, us: UnitSystem) -> Result<Option<Length>> {
    if form.arbor_dia.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(Length::from_millimeters(length_mm(
            "arbor diameter",
            &form.arbor_dia,
            us,
        )?)))
    }
}

/// Parse the form, build the PowerUser scenario, and solve. The engine's own
/// input guards remain the defense-in-depth backstop.
pub fn parse_and_solve(
    form: &TorFormState,
    material_name: &str,
    us: UnitSystem,
    materials: &MaterialStore,
) -> Result<TorFormOutcome> {
    let material: &Material = materials.get(material_name)?;
    let scenario = PowerUser {
        wire_dia: Length::from_millimeters(length_mm("wire diameter", &form.wire_dia, us)?),
        mean_dia: Length::from_millimeters(length_mm("mean diameter", &form.mean_dia, us)?),
        body_coils: positive_num("body coils", &form.body_coils)?,
        leg1: Length::from_millimeters(non_negative_length_mm("leg 1", &form.leg1, us)?),
        leg2: Length::from_millimeters(non_negative_length_mm("leg 2", &form.leg2, us)?),
        arbor_dia: parse_arbor(form, us)?,
        moments: parse_moments_nmm_nonempty(form, us)?
            .into_iter()
            .map(Moment::from_newton_millimeters)
            .collect(),
    };
    Ok(TorFormOutcome {
        design: scenario.solve(material, form.friction_model)?,
    })
}

/// Parse `form` into a persisted `TorsionSpec` (SI mm / N·mm). The caller wraps it
/// in `DesignSpec::Torsion`. Round-trips with `populate_from_spec`.
pub fn build_spec(form: &TorFormState, us: UnitSystem) -> Result<TorsionSpec> {
    let arbor_dia_mm = if form.arbor_dia.trim().is_empty() {
        None
    } else {
        Some(length_mm("arbor diameter", &form.arbor_dia, us)?)
    };
    Ok(TorsionSpec::PowerUser {
        wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
        mean_dia_mm: length_mm("mean diameter", &form.mean_dia, us)?,
        body_coils: positive_num("body coils", &form.body_coils)?,
        leg1_mm: non_negative_length_mm("leg 1", &form.leg1, us)?,
        leg2_mm: non_negative_length_mm("leg 2", &form.leg2, us)?,
        arbor_dia_mm,
        friction_model: form.friction_model,
        moments_nmm: parse_moments_nmm_nonempty(form, us)?,
    })
}

/// Write a persisted `TorsionSpec` back into `form`, converting SI to display
/// units. After this call, `build_spec(form, us)` reproduces the spec.
pub fn populate_from_spec(form: &mut TorFormState, spec: &TorsionSpec, us: UnitSystem) {
    match spec {
        TorsionSpec::PowerUser {
            wire_dia_mm,
            mean_dia_mm,
            body_coils,
            leg1_mm,
            leg2_mm,
            arbor_dia_mm,
            friction_model,
            moments_nmm,
        } => {
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.mean_dia = fmt_len(*mean_dia_mm, us);
            form.body_coils = format!("{body_coils}");
            form.leg1 = fmt_len(*leg1_mm, us);
            form.leg2 = fmt_len(*leg2_mm, us);
            form.arbor_dia = match arbor_dia_mm {
                Some(v) => fmt_len(*v, us),
                None => String::new(),
            };
            form.friction_model = *friction_model;
            form.moments = fmt_moments(moments_nmm, us);
        }
        // Tasks 2–4 replace these arms with full per-scenario population (scenario
        // kind + mode-specific fields). Until then only PowerUser specs exist on
        // disk — nothing writes the other tags before those tasks land.
        TorsionSpec::RateBased {
            wire_dia_mm,
            leg1_mm,
            leg2_mm,
            arbor_dia_mm,
            friction_model,
            moments_nmm,
            ..
        }
        | TorsionSpec::Dimensional {
            wire_dia_mm,
            leg1_mm,
            leg2_mm,
            arbor_dia_mm,
            friction_model,
            moments_nmm,
            ..
        } => {
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.leg1 = fmt_len(*leg1_mm, us);
            form.leg2 = fmt_len(*leg2_mm, us);
            form.arbor_dia = match arbor_dia_mm {
                Some(v) => fmt_len(*v, us),
                None => String::new(),
            };
            form.friction_model = *friction_model;
            form.moments = fmt_moments(moments_nmm, us);
        }
        TorsionSpec::TwoLoad {
            wire_dia_mm,
            mean_dia_mm,
            leg1_mm,
            leg2_mm,
            arbor_dia_mm,
            friction_model,
            ..
        } => {
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.mean_dia = fmt_len(*mean_dia_mm, us);
            form.leg1 = fmt_len(*leg1_mm, us);
            form.leg2 = fmt_len(*leg2_mm, us);
            form.arbor_dia = match arbor_dia_mm {
                Some(v) => fmt_len(*v, us),
                None => String::new(),
            };
            form.friction_model = *friction_model;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use springcore::{MaterialSet, MaterialStore, UnitSystem};

    fn store() -> MaterialStore {
        MaterialStore::new(MaterialSet::load_default())
    }

    fn metric_form() -> TorFormState {
        TorFormState {
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            body_coils: "5".into(),
            leg1: "0".into(),
            leg2: "0".into(),
            moments: "1000".into(),
            ..TorFormState::default()
        }
    }

    #[test]
    fn is_blank_true_until_a_field_is_filled() {
        let mut f = TorFormState::default();
        assert!(f.is_blank());
        f.wire_dia = "2".into();
        assert!(!f.is_blank());
    }

    #[test]
    fn changing_only_friction_model_stays_blank() {
        let f = TorFormState {
            friction_model: FrictionModel::PureBending,
            ..TorFormState::default()
        };
        assert!(
            f.is_blank(),
            "friction model default cannot distinguish blank"
        );
    }

    #[test]
    fn typing_arbor_or_moments_clears_blank() {
        let f = TorFormState {
            arbor_dia: "10".into(),
            ..TorFormState::default()
        };
        assert!(
            !f.is_blank(),
            "arbor is optional but typing it signals intent"
        );
        let g = TorFormState {
            moments: "500".into(),
            ..TorFormState::default()
        };
        assert!(!g.is_blank());
    }

    #[test]
    fn metric_power_user_solves_with_index_ten() {
        let out =
            parse_and_solve(&metric_form(), "Music Wire", UnitSystem::Metric, &store()).unwrap();
        assert_relative_eq!(out.design.index, 10.0, max_relative = 1e-9);
        assert_eq!(out.design.load_points.len(), 1);
    }

    #[test]
    fn blank_wire_dia_errors() {
        let f = TorFormState {
            wire_dia: String::new(),
            ..metric_form()
        };
        assert!(parse_and_solve(&f, "Music Wire", UnitSystem::Metric, &store()).is_err());
    }

    #[test]
    fn non_positive_moment_errors() {
        let f = TorFormState {
            moments: "0".into(),
            ..metric_form()
        };
        assert!(parse_and_solve(&f, "Music Wire", UnitSystem::Metric, &store()).is_err());
    }

    #[test]
    fn mean_at_or_below_wire_errors() {
        let f = TorFormState {
            mean_dia: "2".into(), // C = 1
            ..metric_form()
        };
        assert!(parse_and_solve(&f, "Music Wire", UnitSystem::Metric, &store()).is_err());
    }

    #[test]
    fn build_spec_populate_round_trips_metric_and_us() {
        for us in [UnitSystem::Metric, UnitSystem::Us] {
            let form = TorFormState {
                wire_dia: "2".into(),
                mean_dia: "20".into(),
                body_coils: "5".into(),
                leg1: "10".into(),
                leg2: "10".into(),
                arbor_dia: "10".into(),
                moments: "100, 250".into(),
                friction_model: FrictionModel::PureBending,
            };
            let spec = build_spec(&form, us).unwrap();
            let mut form2 = TorFormState::default();
            populate_from_spec(&mut form2, &spec, us);
            assert_eq!(build_spec(&form2, us).unwrap(), spec);
            assert_eq!(form2.friction_model, FrictionModel::PureBending);
        }
    }

    #[test]
    fn empty_arbor_round_trips_as_none() {
        let spec = build_spec(&metric_form(), UnitSystem::Metric).unwrap();
        let arbor = match &spec {
            springcore::TorsionSpec::PowerUser { arbor_dia_mm, .. } => *arbor_dia_mm,
            _ => panic!("build_spec must produce a PowerUser spec"),
        };
        assert_eq!(arbor, None);
    }

    #[test]
    fn empty_moments_parse_and_solve_errors() {
        // Everything filled except `moments`: the form-boundary guard must reject
        // the empty list before it reaches the engine, with a field-named message.
        let f = TorFormState {
            moments: String::new(),
            ..metric_form()
        };
        let err = parse_and_solve(&f, "Music Wire", UnitSystem::Metric, &store())
            .expect_err("empty moments must be rejected at the form boundary");
        // Assert the form-guard message specifically ("provide …"), which the
        // engine backstop ("… is required") does not emit — so this test proves
        // the form-boundary guard fires, not merely the engine's own check.
        assert!(
            err.to_string()
                .contains("provide at least one applied moment"),
            "expected the form-boundary moment guard message; got: {err}"
        );
    }

    #[test]
    fn empty_moments_build_spec_errors() {
        let f = TorFormState {
            moments: String::new(),
            ..metric_form()
        };
        let err = build_spec(&f, UnitSystem::Metric)
            .expect_err("empty moments must not persist an unsolvable spec");
        assert!(
            err.to_string()
                .contains("provide at least one applied moment"),
            "expected the form-boundary moment guard message; got: {err}"
        );
    }
}
