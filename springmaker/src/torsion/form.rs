//! Pure torsion form-to-design logic. No iced dependency.
use crate::form_helpers::{
    ang_rate_nmm_per_deg, angle_deg, finite_or_err, fmt_ang_rate_nmm_per_deg, fmt_angle_deg,
    fmt_len, fmt_moment, fmt_moments, length_mm, moment_nmm, moments_nmm, non_negative_length_mm,
    positive_force_n, positive_num,
};
use springcore::torsion::{FrictionModel, PowerUser, Scenario, TorsionDesign};
use springcore::units::{Angle, AngularRate, Force, Length, Moment};
use springcore::{Material, MaterialStore, Result, TorsionSpec, UnitSystem};

/// Which torsion input scenario is active. The torsion family's own enum — the
/// module boundary forbids importing the sibling families' kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TorScenarioKind {
    #[default]
    PowerUser,
    RateBased,
    Dimensional,
    TwoLoad,
}

/// All `TorScenarioKind` variants in display order.
pub const ALL_TOR_SCENARIOS: &[TorScenarioKind] = &[
    TorScenarioKind::PowerUser,
    TorScenarioKind::RateBased,
    TorScenarioKind::Dimensional,
    TorScenarioKind::TwoLoad,
];

impl std::fmt::Display for TorScenarioKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            TorScenarioKind::PowerUser => "Power User",
            TorScenarioKind::RateBased => "Rate Based",
            TorScenarioKind::Dimensional => "Dimensional",
            TorScenarioKind::TwoLoad => "Two Load",
        })
    }
}

/// How applied moments are entered: directly, or as forces on a leg at one radius
/// (`M = F·r`, converted at the form boundary — the choice is NOT persisted; specs
/// always store the derived moments).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MomentEntry {
    #[default]
    Direct,
    ForceAtRadius,
}

/// All `MomentEntry` variants in display order.
pub const ALL_MOMENT_ENTRIES: &[MomentEntry] = &[MomentEntry::Direct, MomentEntry::ForceAtRadius];

impl std::fmt::Display for MomentEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            MomentEntry::Direct => "Moments",
            MomentEntry::ForceAtRadius => "Force @ radius",
        })
    }
}

/// Which torsion text field a `Message::TorField` targets.
// Variants are constructed by `torsion::view_model::tor_inputs_view` and
// `torsion::view`. No dead_code annotation is needed: the variants are referenced in
// `tor_inputs_view`'s body and test code, keeping them "live."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    WireDia,
    MeanDia,
    OuterDia,
    BodyCoils,
    Rate,
    Leg1,
    Leg2,
    ArborDia,
    Moments,
    Moment1,
    Angle1,
    Moment2,
    Angle2,
    /// Forces comma-separated (F@r mode only).
    Forces,
    /// Load radius (F@r mode only).
    LoadRadius,
}

/// Torsion form inputs as raw strings, plus the friction-model selector.
#[derive(Debug, Clone, Default)]
pub struct TorFormState {
    pub scenario: TorScenarioKind,
    /// Whether moments are entered directly or derived from F·r. NOT persisted.
    pub moment_entry: MomentEntry,
    pub wire_dia: String,
    pub mean_dia: String,
    /// Outer diameter (Dimensional mode only): wire + 2 × mean_radius.
    pub outer_dia: String,
    pub body_coils: String,
    pub rate: String,
    pub leg1: String,
    pub leg2: String,
    pub arbor_dia: String,
    pub moments: String,
    /// Applied forces, comma-separated (F@r entry mode only).
    pub forces: String,
    /// Load radius for F@r moment derivation (F@r entry mode only).
    pub load_radius: String,
    /// First operating-point moment (TwoLoad mode).
    pub moment1: String,
    /// First operating-point angle in degrees (TwoLoad mode).
    pub angle1: String,
    /// Second operating-point moment (TwoLoad mode).
    pub moment2: String,
    /// Second operating-point angle in degrees (TwoLoad mode).
    pub angle2: String,
    pub friction_model: FrictionModel,
}

impl TorFormState {
    /// Whether the user has entered none of the input fields. Drives the
    /// "untouched form" suppression in `App::recompute`. `friction_model` and
    /// `scenario` are excluded — they always hold defaults and cannot distinguish
    /// a blank form.
    pub fn is_blank(&self) -> bool {
        let all_empty = |fields: &[&String]| fields.iter().all(|f| f.trim().is_empty());
        // Only the ACTIVE entry mode's fields signal intent — a hidden field left over
        // from a toggled-away mode must not un-blank a visually empty form (mirrors
        // extension's hook_mode-gated hooks_blank term).
        let moment_entry_blank = match self.moment_entry {
            MomentEntry::Direct => self.moments.trim().is_empty(),
            MomentEntry::ForceAtRadius => {
                self.forces.trim().is_empty() && self.load_radius.trim().is_empty()
            }
        };
        match self.scenario {
            TorScenarioKind::PowerUser => {
                moment_entry_blank
                    && all_empty(&[
                        &self.wire_dia,
                        &self.mean_dia,
                        &self.body_coils,
                        &self.leg1,
                        &self.leg2,
                        &self.arbor_dia,
                    ])
            }
            TorScenarioKind::RateBased => {
                moment_entry_blank
                    && all_empty(&[
                        &self.wire_dia,
                        &self.mean_dia,
                        &self.rate,
                        &self.leg1,
                        &self.leg2,
                        &self.arbor_dia,
                    ])
            }
            TorScenarioKind::Dimensional => {
                moment_entry_blank
                    && all_empty(&[
                        &self.wire_dia,
                        &self.outer_dia,
                        &self.body_coils,
                        &self.leg1,
                        &self.leg2,
                        &self.arbor_dia,
                    ])
            }
            TorScenarioKind::TwoLoad => all_empty(&[
                &self.wire_dia,
                &self.mean_dia,
                &self.leg1,
                &self.leg2,
                &self.arbor_dia,
                &self.moment1,
                &self.angle1,
                &self.moment2,
                &self.angle2,
            ]),
        }
    }
}

/// A solved torsion form: the design (which carries engine-computed status).
#[derive(Debug, Clone)]
pub struct TorFormOutcome {
    // Read by `torsion::view_model::tor_status_view` and by tests;
    // the `#[cfg_attr]` guard is no longer needed because `tor_status_view` references
    // `out.design` in non-test builds via `calculator::status_panel`.
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

/// The applied-moment list per the active entry mode: Direct parses the moments
/// field; ForceAtRadius derives each moment as `M = F·r` (engine helper, cited)
/// from strictly-positive forces at one strictly-positive load radius. Both modes
/// reject an empty list at the form boundary.
fn parse_applied_moments_nmm(form: &TorFormState, us: UnitSystem) -> Result<Vec<f64>> {
    match form.moment_entry {
        MomentEntry::Direct => parse_moments_nmm_nonempty(form, us),
        MomentEntry::ForceAtRadius => {
            let radius_mm = length_mm("load radius", &form.load_radius, us)?;
            let moments: Vec<f64> = form
                .forces
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| {
                    let force_n = positive_force_n("force", s, us)?;
                    let nmm = springcore::torsion::moment_from_force_at_radius(
                        Force::from_newtons(force_n),
                        Length::from_millimeters(radius_mm),
                    )
                    .newton_millimeters();
                    // Attribute the overflow to the PRODUCT, not the force alone: a
                    // moderate force at a huge radius overflows too, and reducing
                    // either input fixes it. `s` still names the offending entry.
                    finite_or_err("force × load radius", s, nmm)
                })
                .collect::<Result<_>>()?;
            if moments.is_empty() {
                return Err(springcore::SpringError::InconsistentInputs(
                    "provide at least one applied force".into(),
                ));
            }
            Ok(moments)
        }
    }
}

/// Reject at the form boundary when outer diameter ≤ wire diameter (mean ≤ 0).
/// Mirrors extension's `dimensional_mean_mm` guard — the engine's OD/index guards
/// remain the defense-in-depth backstop.
fn dimensional_mean_check(wire_dia_mm: f64, outer_dia_mm: f64) -> Result<()> {
    if outer_dia_mm - wire_dia_mm <= 0.0 {
        return Err(springcore::SpringError::InconsistentInputs(
            "outer diameter must be greater than wire diameter".into(),
        ));
    }
    Ok(())
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

/// Like `parse_arbor` but returns millimetres for `build_spec`.
fn parse_arbor_mm(form: &TorFormState, us: UnitSystem) -> Result<Option<f64>> {
    if form.arbor_dia.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(length_mm("arbor diameter", &form.arbor_dia, us)?))
    }
}

/// Parse the form, build the active scenario, and solve. The engine's own
/// input guards remain the defense-in-depth backstop.
pub fn parse_and_solve(
    form: &TorFormState,
    material_name: &str,
    us: UnitSystem,
    materials: &MaterialStore,
) -> Result<TorFormOutcome> {
    let material: &Material = materials.get(material_name)?;
    match form.scenario {
        TorScenarioKind::PowerUser => {
            let scenario = PowerUser {
                wire_dia: Length::from_millimeters(length_mm("wire diameter", &form.wire_dia, us)?),
                mean_dia: Length::from_millimeters(length_mm("mean diameter", &form.mean_dia, us)?),
                body_coils: positive_num("body coils", &form.body_coils)?,
                leg1: Length::from_millimeters(non_negative_length_mm("leg 1", &form.leg1, us)?),
                leg2: Length::from_millimeters(non_negative_length_mm("leg 2", &form.leg2, us)?),
                arbor_dia: parse_arbor(form, us)?,
                moments: parse_applied_moments_nmm(form, us)?
                    .into_iter()
                    .map(Moment::from_newton_millimeters)
                    .collect(),
            };
            Ok(TorFormOutcome {
                design: scenario.solve(material, form.friction_model)?,
            })
        }
        TorScenarioKind::RateBased => {
            let scenario = springcore::torsion::RateBased {
                wire_dia: Length::from_millimeters(length_mm("wire diameter", &form.wire_dia, us)?),
                mean_dia: Length::from_millimeters(length_mm("mean diameter", &form.mean_dia, us)?),
                rate: AngularRate::from_newton_meters_per_degree(
                    ang_rate_nmm_per_deg("rate", &form.rate, us)? / 1000.0,
                ),
                leg1: Length::from_millimeters(non_negative_length_mm("leg 1", &form.leg1, us)?),
                leg2: Length::from_millimeters(non_negative_length_mm("leg 2", &form.leg2, us)?),
                arbor_dia: parse_arbor(form, us)?,
                moments: parse_applied_moments_nmm(form, us)?
                    .into_iter()
                    .map(Moment::from_newton_millimeters)
                    .collect(),
            };
            Ok(TorFormOutcome {
                design: scenario.solve(material, form.friction_model)?,
            })
        }
        TorScenarioKind::Dimensional => {
            let wire_dia_mm = length_mm("wire diameter", &form.wire_dia, us)?;
            let outer_dia_mm = length_mm("outer diameter", &form.outer_dia, us)?;
            dimensional_mean_check(wire_dia_mm, outer_dia_mm)?;
            let scenario = springcore::torsion::Dimensional {
                wire_dia: Length::from_millimeters(wire_dia_mm),
                outer_dia: Length::from_millimeters(outer_dia_mm),
                body_coils: positive_num("body coils", &form.body_coils)?,
                leg1: Length::from_millimeters(non_negative_length_mm("leg 1", &form.leg1, us)?),
                leg2: Length::from_millimeters(non_negative_length_mm("leg 2", &form.leg2, us)?),
                arbor_dia: parse_arbor(form, us)?,
                moments: parse_applied_moments_nmm(form, us)?
                    .into_iter()
                    .map(Moment::from_newton_millimeters)
                    .collect(),
            };
            Ok(TorFormOutcome {
                design: scenario.solve(material, form.friction_model)?,
            })
        }
        TorScenarioKind::TwoLoad => {
            let scenario = springcore::torsion::TwoLoad {
                wire_dia: Length::from_millimeters(length_mm("wire diameter", &form.wire_dia, us)?),
                mean_dia: Length::from_millimeters(length_mm("mean diameter", &form.mean_dia, us)?),
                leg1: Length::from_millimeters(non_negative_length_mm("leg 1", &form.leg1, us)?),
                leg2: Length::from_millimeters(non_negative_length_mm("leg 2", &form.leg2, us)?),
                arbor_dia: parse_arbor(form, us)?,
                point1: (
                    Moment::from_newton_millimeters(moment_nmm("moment 1", &form.moment1, us)?),
                    Angle::from_degrees(angle_deg("angle 1", &form.angle1)?),
                ),
                point2: (
                    Moment::from_newton_millimeters(moment_nmm("moment 2", &form.moment2, us)?),
                    Angle::from_degrees(angle_deg("angle 2", &form.angle2)?),
                ),
            };
            Ok(TorFormOutcome {
                design: scenario.solve(material, form.friction_model)?,
            })
        }
    }
}

/// Parse `form` into a persisted `TorsionSpec` (SI mm / N·mm). The caller wraps it
/// in `DesignSpec::Torsion`. Round-trips with `populate_from_spec`.
pub fn build_spec(form: &TorFormState, us: UnitSystem) -> Result<TorsionSpec> {
    match form.scenario {
        TorScenarioKind::PowerUser => Ok(TorsionSpec::PowerUser {
            wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
            mean_dia_mm: length_mm("mean diameter", &form.mean_dia, us)?,
            body_coils: positive_num("body coils", &form.body_coils)?,
            leg1_mm: non_negative_length_mm("leg 1", &form.leg1, us)?,
            leg2_mm: non_negative_length_mm("leg 2", &form.leg2, us)?,
            arbor_dia_mm: parse_arbor_mm(form, us)?,
            friction_model: form.friction_model,
            moments_nmm: parse_applied_moments_nmm(form, us)?,
        }),
        TorScenarioKind::RateBased => Ok(TorsionSpec::RateBased {
            wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
            mean_dia_mm: length_mm("mean diameter", &form.mean_dia, us)?,
            rate_nmm_per_deg: ang_rate_nmm_per_deg("rate", &form.rate, us)?,
            leg1_mm: non_negative_length_mm("leg 1", &form.leg1, us)?,
            leg2_mm: non_negative_length_mm("leg 2", &form.leg2, us)?,
            arbor_dia_mm: parse_arbor_mm(form, us)?,
            friction_model: form.friction_model,
            moments_nmm: parse_applied_moments_nmm(form, us)?,
        }),
        TorScenarioKind::Dimensional => {
            let wire_dia_mm = length_mm("wire diameter", &form.wire_dia, us)?;
            let outer_dia_mm = length_mm("outer diameter", &form.outer_dia, us)?;
            dimensional_mean_check(wire_dia_mm, outer_dia_mm)?;
            Ok(TorsionSpec::Dimensional {
                wire_dia_mm,
                outer_dia_mm,
                body_coils: positive_num("body coils", &form.body_coils)?,
                leg1_mm: non_negative_length_mm("leg 1", &form.leg1, us)?,
                leg2_mm: non_negative_length_mm("leg 2", &form.leg2, us)?,
                arbor_dia_mm: parse_arbor_mm(form, us)?,
                friction_model: form.friction_model,
                moments_nmm: parse_applied_moments_nmm(form, us)?,
            })
        }
        TorScenarioKind::TwoLoad => Ok(TorsionSpec::TwoLoad {
            wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
            mean_dia_mm: length_mm("mean diameter", &form.mean_dia, us)?,
            leg1_mm: non_negative_length_mm("leg 1", &form.leg1, us)?,
            leg2_mm: non_negative_length_mm("leg 2", &form.leg2, us)?,
            arbor_dia_mm: parse_arbor_mm(form, us)?,
            friction_model: form.friction_model,
            moment1_nmm: moment_nmm("moment 1", &form.moment1, us)?,
            angle1_deg: angle_deg("angle 1", &form.angle1)?,
            moment2_nmm: moment_nmm("moment 2", &form.moment2, us)?,
            angle2_deg: angle_deg("angle 2", &form.angle2)?,
        }),
    }
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
            form.scenario = TorScenarioKind::PowerUser;
            form.moment_entry = MomentEntry::Direct;
            form.forces = String::new();
            form.load_radius = String::new();
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
        TorsionSpec::RateBased {
            wire_dia_mm,
            mean_dia_mm,
            rate_nmm_per_deg,
            leg1_mm,
            leg2_mm,
            arbor_dia_mm,
            friction_model,
            moments_nmm,
        } => {
            form.scenario = TorScenarioKind::RateBased;
            form.moment_entry = MomentEntry::Direct;
            form.forces = String::new();
            form.load_radius = String::new();
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.mean_dia = fmt_len(*mean_dia_mm, us);
            form.rate = fmt_ang_rate_nmm_per_deg(*rate_nmm_per_deg, us);
            form.leg1 = fmt_len(*leg1_mm, us);
            form.leg2 = fmt_len(*leg2_mm, us);
            form.arbor_dia = match arbor_dia_mm {
                Some(v) => fmt_len(*v, us),
                None => String::new(),
            };
            form.friction_model = *friction_model;
            form.moments = fmt_moments(moments_nmm, us);
        }
        TorsionSpec::Dimensional {
            wire_dia_mm,
            outer_dia_mm,
            body_coils,
            leg1_mm,
            leg2_mm,
            arbor_dia_mm,
            friction_model,
            moments_nmm,
        } => {
            form.scenario = TorScenarioKind::Dimensional;
            form.moment_entry = MomentEntry::Direct;
            form.forces = String::new();
            form.load_radius = String::new();
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.outer_dia = fmt_len(*outer_dia_mm, us);
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
        TorsionSpec::TwoLoad {
            wire_dia_mm,
            mean_dia_mm,
            leg1_mm,
            leg2_mm,
            arbor_dia_mm,
            friction_model,
            moment1_nmm,
            angle1_deg,
            moment2_nmm,
            angle2_deg,
        } => {
            form.scenario = TorScenarioKind::TwoLoad;
            form.moment_entry = MomentEntry::Direct;
            form.forces = String::new();
            form.load_radius = String::new();
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.mean_dia = fmt_len(*mean_dia_mm, us);
            form.leg1 = fmt_len(*leg1_mm, us);
            form.leg2 = fmt_len(*leg2_mm, us);
            form.arbor_dia = match arbor_dia_mm {
                Some(v) => fmt_len(*v, us),
                None => String::new(),
            };
            form.friction_model = *friction_model;
            form.moment1 = fmt_moment(*moment1_nmm, us);
            form.angle1 = fmt_angle_deg(*angle1_deg);
            form.moment2 = fmt_moment(*moment2_nmm, us);
            form.angle2 = fmt_angle_deg(*angle2_deg);
        }
        // Task 2 replaces this arm with full MinWeight population (scenario kind,
        // optimizer fields, both selectors). Until then nothing writes this tag.
        TorsionSpec::MinWeight {
            rate_nmm_per_deg,
            leg1_mm,
            leg2_mm,
            arbor_dia_mm,
            friction_model,
            ..
        } => {
            form.rate = fmt_ang_rate_nmm_per_deg(*rate_nmm_per_deg, us);
            form.leg1 = fmt_len(*leg1_mm, us);
            form.leg2 = fmt_len(*leg2_mm, us);
            form.arbor_dia = match arbor_dia_mm {
                Some(v) => fmt_len(*v, us),
                None => String::new(),
            };
            form.friction_model = *friction_model;
            form.moment_entry = MomentEntry::Direct;
            form.forces = String::new();
            form.load_radius = String::new();
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
                ..TorFormState::default()
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

    #[test]
    fn force_at_radius_equals_direct_moments() {
        // 10 N @ 50 mm ≡ 500 N·mm — identical solve AND identical persisted spec.
        let far = TorFormState {
            moment_entry: MomentEntry::ForceAtRadius,
            forces: "10".into(),
            load_radius: "50".into(),
            moments: String::new(),
            ..metric_form()
        };
        let direct = TorFormState {
            moments: "500".into(),
            ..metric_form()
        };
        let out_far = parse_and_solve(&far, "Music Wire", UnitSystem::Metric, &store()).unwrap();
        let out_direct =
            parse_and_solve(&direct, "Music Wire", UnitSystem::Metric, &store()).unwrap();
        assert_relative_eq!(
            out_far.design.load_points[0].moment.newton_millimeters(),
            out_direct.design.load_points[0].moment.newton_millimeters(),
            max_relative = 1e-12
        );
        assert_eq!(
            build_spec(&far, UnitSystem::Metric).unwrap(),
            build_spec(&direct, UnitSystem::Metric).unwrap(),
            "F@r persists the derived moments — specs must be identical"
        );
    }

    #[test]
    fn force_at_radius_empty_forces_rejected() {
        let f = TorFormState {
            moment_entry: MomentEntry::ForceAtRadius,
            forces: String::new(),
            load_radius: "50".into(),
            moments: String::new(),
            ..metric_form()
        };
        let err = parse_and_solve(&f, "Music Wire", UnitSystem::Metric, &store()).unwrap_err();
        assert!(
            err.to_string()
                .contains("provide at least one applied force"),
            "expected the F@r non-empty guard; got: {err}"
        );
    }

    #[test]
    fn populate_resets_moment_entry_and_clears_far_fields() {
        let far = TorFormState {
            moment_entry: MomentEntry::ForceAtRadius,
            forces: "10".into(),
            load_radius: "50".into(),
            moments: String::new(),
            ..metric_form()
        };
        let spec = build_spec(&far, UnitSystem::Metric).unwrap();
        let mut form2 = far.clone();
        populate_from_spec(&mut form2, &spec, UnitSystem::Metric);
        assert_eq!(form2.moment_entry, MomentEntry::Direct);
        assert!(form2.forces.is_empty() && form2.load_radius.is_empty());
        assert_eq!(form2.moments, "500", "derived moments shown in Direct mode");
    }

    #[test]
    fn is_blank_ignores_hidden_far_fields_after_toggle_back() {
        // Toggle to F@r, type a force, toggle back to Direct: the hidden forces field
        // must not un-blank the visually empty Direct form.
        let mut f = TorFormState {
            moment_entry: MomentEntry::ForceAtRadius,
            forces: "10".into(),
            ..TorFormState::default()
        };
        assert!(
            !f.is_blank(),
            "typed force in the ACTIVE mode signals intent"
        );
        f.moment_entry = MomentEntry::Direct;
        assert!(
            f.is_blank(),
            "a hidden F@r field must not un-blank a visually empty Direct form"
        );
    }

    #[test]
    fn far_moment_overflow_is_rejected_and_names_the_product() {
        // The derived moment F·r can overflow from EITHER a huge force at a small
        // radius or a moderate force at a huge radius — the guard must catch both
        // before the engine and attribute the overflow to the product ("force ×
        // load radius"), not the force alone, since reducing either input fixes it.
        for (forces, radius) in [("1e308", "50"), ("100", "1e307")] {
            let f = TorFormState {
                moment_entry: MomentEntry::ForceAtRadius,
                forces: forces.into(),
                load_radius: radius.into(),
                moments: String::new(),
                ..metric_form()
            };
            let err = parse_and_solve(&f, "Music Wire", UnitSystem::Metric, &store())
                .expect_err("overflow moment must be caught at form boundary");
            let msg = err.to_string();
            assert!(
                msg.contains("overflow") && msg.contains("force × load radius"),
                "expected product-attributed overflow for F={forces}, r={radius}; got: {msg}"
            );
        }
    }

    #[test]
    fn is_blank_trips_on_far_fields_but_not_selector() {
        let mut f = TorFormState {
            moment_entry: MomentEntry::ForceAtRadius,
            ..TorFormState::default()
        };
        assert!(f.is_blank(), "selector alone cannot distinguish blank");
        f.forces = "10".into();
        assert!(!f.is_blank(), "typing a force clears blank");
    }

    fn ratebased_metric_form() -> TorFormState {
        // Rate chosen so PureBending with d=2mm, D=20mm, Na=5 gives exactly
        // 0.5085 N·m/rad: E_MusicWire * d^4 / (64 * D * 5) = 0.5085 N·m/rad,
        // which is 0.5085 * 1000 * π/180 N·mm/°. Must specify PureBending to
        // reproduce body_coils=5 — the default (ShigleyFriction) uses a different
        // denominator (2π·10.8 ≈ 67.86 vs 64) and would yield ~4.71 coils.
        TorFormState {
            scenario: TorScenarioKind::RateBased,
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            rate: format!("{}", 0.5085_f64 * 1000.0 * std::f64::consts::PI / 180.0),
            leg1: "0".into(),
            leg2: "0".into(),
            moments: "1000".into(),
            friction_model: FrictionModel::PureBending,
            ..TorFormState::default()
        }
    }

    #[test]
    fn ratebased_derives_body_coils_and_round_trips_rate() {
        let out = parse_and_solve(
            &ratebased_metric_form(),
            "Music Wire",
            UnitSystem::Metric,
            &store(),
        )
        .expect("RateBased should solve");
        assert_relative_eq!(out.design.inputs.body_coils, 5.0, max_relative = 1e-9);
        assert_relative_eq!(
            out.design.rate.newton_meters_per_radian(),
            0.5085,
            max_relative = 1e-9
        );
    }

    #[test]
    fn ratebased_build_spec_populate_round_trip() {
        for us in [UnitSystem::Metric, UnitSystem::Us] {
            let spec = build_spec(&ratebased_metric_form(), us).unwrap();
            let mut form2 = TorFormState::default();
            populate_from_spec(&mut form2, &spec, us);
            assert_eq!(form2.scenario, TorScenarioKind::RateBased);
            assert_eq!(build_spec(&form2, us).unwrap(), spec);
        }
    }

    #[test]
    fn is_blank_ratebased_trips_on_rate() {
        let mut f = TorFormState {
            scenario: TorScenarioKind::RateBased,
            ..TorFormState::default()
        };
        assert!(f.is_blank(), "untouched RateBased form is blank");
        f.rate = "8.9".into();
        assert!(!f.is_blank(), "entering the rate clears blank");
    }

    #[test]
    fn dimensional_matches_power_user_geometry() {
        let form = TorFormState {
            scenario: TorScenarioKind::Dimensional,
            wire_dia: "2".into(),
            outer_dia: "22".into(), // mean = 20
            body_coils: "5".into(),
            leg1: "0".into(),
            leg2: "0".into(),
            moments: "1000".into(),
            ..TorFormState::default()
        };
        let out = parse_and_solve(&form, "Music Wire", UnitSystem::Metric, &store()).unwrap();
        assert_relative_eq!(out.design.index, 10.0, max_relative = 1e-9);
    }

    #[test]
    fn dimensional_outer_not_greater_than_wire_rejected_both_sites() {
        // The owed field-named boundary error, at BOTH call sites, metric and US.
        for us in [UnitSystem::Metric, UnitSystem::Us] {
            let form = TorFormState {
                scenario: TorScenarioKind::Dimensional,
                wire_dia: "2".into(),
                outer_dia: "2".into(), // mean = 0
                body_coils: "5".into(),
                leg1: "0".into(),
                leg2: "0".into(),
                moments: "1000".into(),
                ..TorFormState::default()
            };
            for err in [
                parse_and_solve(&form, "Music Wire", us, &store()).unwrap_err(),
                build_spec(&form, us).unwrap_err(),
            ] {
                assert!(
                    err.to_string()
                        .contains("outer diameter must be greater than wire diameter"),
                    "expected the form-boundary outer>wire error ({us:?}); got: {err}"
                );
            }
        }
    }

    fn twoload_metric_form() -> TorFormState {
        // Two points on the oracle k' = 0.5085 N·m/rad line, in display units:
        // (508.5 N·mm, 1 rad = 57.29578°), (1017 N·mm, 2 rad = 114.59156°).
        TorFormState {
            scenario: TorScenarioKind::TwoLoad,
            friction_model: FrictionModel::PureBending, // 0.5085 N·m/rad is the PureBending oracle
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            leg1: "0".into(),
            leg2: "0".into(),
            moment1: "508.5".into(),
            angle1: format!("{}", 180.0_f64 / std::f64::consts::PI),
            moment2: "1017".into(),
            angle2: format!("{}", 2.0_f64 * 180.0 / std::f64::consts::PI),
            ..TorFormState::default()
        }
    }

    #[test]
    fn twoload_derives_rate_and_body_coils_from_points() {
        let out = parse_and_solve(
            &twoload_metric_form(),
            "Music Wire",
            UnitSystem::Metric,
            &store(),
        )
        .expect("TwoLoad should solve");
        assert_relative_eq!(
            out.design.rate.newton_meters_per_radian(),
            0.5085,
            max_relative = 1e-9
        );
        assert_relative_eq!(out.design.inputs.body_coils, 5.0, max_relative = 1e-9);
        assert_eq!(out.design.load_points.len(), 2);
    }

    #[test]
    fn twoload_degenerate_points_surface_engine_message() {
        let form = TorFormState {
            angle2: twoload_metric_form().angle1.clone(), // same angle both points
            ..twoload_metric_form()
        };
        let err = parse_and_solve(&form, "Music Wire", UnitSystem::Metric, &store()).unwrap_err();
        assert!(
            err.to_string().contains("different angles"),
            "engine degenerate-point message must surface; got: {err}"
        );
    }

    #[test]
    fn dimensional_and_twoload_round_trip_and_blank() {
        for us in [UnitSystem::Metric, UnitSystem::Us] {
            for form in [
                TorFormState {
                    scenario: TorScenarioKind::Dimensional,
                    wire_dia: "2".into(),
                    outer_dia: "22".into(),
                    body_coils: "5".into(),
                    leg1: "0".into(),
                    leg2: "0".into(),
                    moments: "1000".into(),
                    ..TorFormState::default()
                },
                twoload_metric_form(),
            ] {
                let spec = build_spec(&form, us).unwrap();
                let mut form2 = TorFormState::default();
                populate_from_spec(&mut form2, &spec, us);
                assert_eq!(form2.scenario, form.scenario);
                assert_eq!(build_spec(&form2, us).unwrap(), spec);
            }
        }
        let mut blank = TorFormState {
            scenario: TorScenarioKind::TwoLoad,
            ..TorFormState::default()
        };
        assert!(blank.is_blank());
        blank.angle1 = "-10".into(); // negative angle is a legal, intent-signaling entry
        assert!(
            !blank.is_blank(),
            "typing a TwoLoad point field clears blank"
        );
    }
}
