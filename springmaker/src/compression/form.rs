//! Pure form-to-design logic. No iced dependency, so it is unit-testable.

use crate::form_helpers::{
    fmt_force, fmt_len, fmt_loads, fmt_rate, length_mm, loads_n, non_negative_force_n, num,
    optional_non_negative_num, positive_force_n, positive_num, rate_npm,
};
use springcore::units::Force;
use springcore::UnitSystem;
use springcore::{
    analyze_fatigue, evaluate_status, DesignStatus, FatigueResult, MaterialStore, Result,
    SavedDesign, ScenarioSpec, SpringDesign, SpringError,
};

/// Three-state fatigue result distinguishing "not attempted" from "no data".
#[derive(Debug, Clone)]
pub enum FatigueStatus {
    /// User left min/max cycle forces blank; fatigue was not attempted.
    Skipped,
    /// Cycle forces were supplied but the material has no cited endurance data.
    NoData,
    /// Fatigue analysis succeeded.
    Computed(FatigueResult),
}

/// Which text field a [`crate::app::Message::CompField`] targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    WireDia,
    MeanDia,
    OuterDia,
    Active,
    FreeLength,
    Rate,
    Loads,
    Inactive,
    Force1,
    Length1,
    Force2,
    Length2,
    FatigueMin,
    FatigueMax,
    // Min Weight fields
    MaxForce,
    IndexMin,
    IndexMax,
    MaxOuterDia,
    CandidateDiameters,
    ClashAllowance,
}

/// Which scenario the form is editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScenarioKind {
    #[default]
    PowerUser,
    TwoLoad,
    RateBased,
    Dimensional,
    MinWeight,
}

impl std::fmt::Display for ScenarioKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScenarioKind::PowerUser => write!(f, "Power User"),
            ScenarioKind::TwoLoad => write!(f, "Two Load"),
            ScenarioKind::RateBased => write!(f, "Rate Based"),
            ScenarioKind::Dimensional => write!(f, "Dimensional"),
            ScenarioKind::MinWeight => write!(f, "Min Weight"),
        }
    }
}

/// All `ScenarioKind` variants in display order.
pub const ALL_SCENARIOS: &[ScenarioKind] = &[
    ScenarioKind::PowerUser,
    ScenarioKind::TwoLoad,
    ScenarioKind::RateBased,
    ScenarioKind::Dimensional,
    ScenarioKind::MinWeight,
];

/// All editable form fields (as raw strings, mirroring the UI).
#[derive(Debug, Clone)]
pub struct FormState {
    pub scenario: ScenarioKind,
    pub end_type: String,
    pub fixity: String,
    pub wire_dia: String,
    pub mean_dia: String,
    pub outer_dia: String,
    pub active: String,
    pub free_length: String,
    pub rate: String,
    pub loads: String,
    pub inactive: String,
    pub force1: String,
    pub length1: String,
    pub force2: String,
    pub length2: String,
    pub fatigue_min: String,
    pub fatigue_max: String,
    // Min Weight scenario fields
    pub max_force: String,
    pub index_min: String,
    pub index_max: String,
    pub max_outer_dia: String,
    pub candidate_diameters: String,
    pub clash_allowance: String,
}

impl Default for FormState {
    fn default() -> Self {
        Self {
            scenario: ScenarioKind::default(),
            end_type: "squared_ground".into(),
            fixity: "fixed_fixed".into(),
            wire_dia: String::new(),
            mean_dia: String::new(),
            outer_dia: String::new(),
            active: String::new(),
            free_length: String::new(),
            rate: String::new(),
            loads: String::new(),
            inactive: String::new(),
            force1: String::new(),
            length1: String::new(),
            force2: String::new(),
            length2: String::new(),
            fatigue_min: String::new(),
            fatigue_max: String::new(),
            max_force: String::new(),
            index_min: "4".into(),
            index_max: "12".into(),
            max_outer_dia: String::new(),
            candidate_diameters: String::new(),
            clash_allowance: "0.15".into(),
        }
    }
}

impl FormState {
    /// Whether the user has entered none of the active scenario's primary input
    /// fields.
    ///
    /// Drives the "untouched form" suppression in `App::recompute`: an entirely
    /// empty form (e.g. just after a family/scenario switch) stays in the initial
    /// Empty state instead of surfacing a parse error, but once ANY of those fields
    /// is filled the form is no longer blank and parse feedback shows. Each scenario
    /// lists the input fields it actually reads — Dimensional uses `outer_dia` (not
    /// `mean_dia`), MinWeight optimises over `candidate_diameters` — so a
    /// partially-filled form is never mistaken for blank. The valid-empty inputs still
    /// count when entered — `loads` in the forward modes, the optional `max_outer_dia` in
    /// MinWeight — because typing either signals intent (matching the extension family).
    /// Only the pre-filled defaults (`index_min`, `index_max`, `clash_allowance`) are
    /// excluded, as they are not user-entered design intent.
    pub fn is_blank(&self) -> bool {
        let all_empty = |fields: &[&String]| fields.iter().all(|f| f.trim().is_empty());
        match self.scenario {
            ScenarioKind::PowerUser => all_empty(&[
                &self.wire_dia,
                &self.mean_dia,
                &self.active,
                &self.free_length,
                &self.loads,
            ]),
            ScenarioKind::TwoLoad => all_empty(&[
                &self.wire_dia,
                &self.mean_dia,
                &self.force1,
                &self.length1,
                &self.force2,
                &self.length2,
            ]),
            ScenarioKind::RateBased => all_empty(&[
                &self.wire_dia,
                &self.mean_dia,
                &self.rate,
                &self.free_length,
                &self.loads,
            ]),
            ScenarioKind::Dimensional => all_empty(&[
                &self.wire_dia,
                &self.outer_dia,
                &self.active,
                &self.free_length,
                &self.loads,
            ]),
            ScenarioKind::MinWeight => all_empty(&[
                &self.rate,
                &self.max_force,
                &self.max_outer_dia,
                &self.candidate_diameters,
            ]),
        }
    }
}

/// Extra outputs produced only by the Min Weight optimisation path.
#[derive(Debug, Clone)]
pub struct MinWeightExtra {
    pub binding: springcore::BindingConstraint,
    pub mass_kg: f64,
}

/// A solved form: the design plus its status and fatigue result.
#[derive(Debug, Clone)]
pub struct FormOutcome {
    pub design: SpringDesign,
    pub status: DesignStatus,
    pub fatigue: FatigueStatus,
    pub min_weight: Option<MinWeightExtra>,
}

pub fn build_spec(form: &FormState, us: UnitSystem) -> Result<ScenarioSpec> {
    Ok(match form.scenario {
        ScenarioKind::PowerUser => ScenarioSpec::PowerUser {
            end_type: form.end_type.clone(),
            fixity: form.fixity.clone(),
            wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
            mean_dia_mm: length_mm("mean diameter", &form.mean_dia, us)?,
            active: positive_num("active coils", &form.active)?,
            free_length_mm: length_mm("free length", &form.free_length, us)?,
            loads_n: loads_n(&form.loads, us)?,
            inactive_coils: optional_non_negative_num("inactive coils", &form.inactive)?,
        },
        ScenarioKind::TwoLoad => ScenarioSpec::TwoLoad {
            end_type: form.end_type.clone(),
            fixity: form.fixity.clone(),
            wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
            mean_dia_mm: length_mm("mean diameter", &form.mean_dia, us)?,
            force1_n: non_negative_force_n("force 1", &form.force1, us)?,
            length1_mm: length_mm("length 1", &form.length1, us)?,
            force2_n: non_negative_force_n("force 2", &form.force2, us)?,
            length2_mm: length_mm("length 2", &form.length2, us)?,
            inactive_coils: optional_non_negative_num("inactive coils", &form.inactive)?,
        },
        ScenarioKind::RateBased => ScenarioSpec::RateBased {
            end_type: form.end_type.clone(),
            fixity: form.fixity.clone(),
            wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
            mean_dia_mm: length_mm("mean diameter", &form.mean_dia, us)?,
            rate_n_per_m: rate_npm("rate", &form.rate, us)?,
            free_length_mm: length_mm("free length", &form.free_length, us)?,
            loads_n: loads_n(&form.loads, us)?,
            inactive_coils: optional_non_negative_num("inactive coils", &form.inactive)?,
        },
        ScenarioKind::Dimensional => ScenarioSpec::Dimensional {
            end_type: form.end_type.clone(),
            fixity: form.fixity.clone(),
            wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
            outer_dia_mm: length_mm("outer diameter", &form.outer_dia, us)?,
            active: positive_num("active coils", &form.active)?,
            free_length_mm: length_mm("free length", &form.free_length, us)?,
            loads_n: loads_n(&form.loads, us)?,
            inactive_coils: optional_non_negative_num("inactive coils", &form.inactive)?,
        },
        ScenarioKind::MinWeight => {
            let diameters_mm: Vec<f64> = form
                .candidate_diameters
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| length_mm("candidate diameter", s, us))
                .collect::<Result<_>>()?;
            if diameters_mm.is_empty() {
                return Err(SpringError::InconsistentInputs(
                    "provide at least one candidate wire diameter".into(),
                ));
            }
            let max_outer_dia_mm = if form.max_outer_dia.trim().is_empty() {
                None
            } else {
                Some(length_mm("max outer diameter", &form.max_outer_dia, us)?)
            };
            ScenarioSpec::MinWeight {
                end_type: form.end_type.clone(),
                fixity: form.fixity.clone(),
                required_rate_n_per_m: rate_npm("rate", &form.rate, us)?,
                max_force_n: positive_force_n("max force", &form.max_force, us)?,
                index_min: positive_num("index min", &form.index_min)?,
                index_max: positive_num("index max", &form.index_max)?,
                max_outer_dia_mm,
                candidate_diameters_mm: diameters_mm,
                clash_allowance: num("clash allowance", &form.clash_allowance)?,
                inactive_coils: optional_non_negative_num("inactive coils", &form.inactive)?,
            }
        }
    })
}

/// Write a `ScenarioSpec`'s fields back into `form`, converting SI-stored
/// mm/N values to display units per `us`.
///
/// After calling this, `build_spec(form, us)` should reproduce a spec
/// equal to `spec` (round-trip invariant).
pub fn populate_from_spec(form: &mut FormState, spec: &ScenarioSpec, us: UnitSystem) {
    match spec {
        ScenarioSpec::PowerUser {
            end_type,
            fixity,
            wire_dia_mm,
            mean_dia_mm,
            active,
            free_length_mm,
            loads_n,
            inactive_coils,
        } => {
            form.scenario = ScenarioKind::PowerUser;
            form.end_type = end_type.clone();
            form.fixity = fixity.clone();
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.mean_dia = fmt_len(*mean_dia_mm, us);
            form.active = format!("{active}");
            form.free_length = fmt_len(*free_length_mm, us);
            form.loads = fmt_loads(loads_n, us);
            form.inactive = inactive_coils.map(|v| format!("{v}")).unwrap_or_default();
        }
        ScenarioSpec::TwoLoad {
            end_type,
            fixity,
            wire_dia_mm,
            mean_dia_mm,
            force1_n,
            length1_mm,
            force2_n,
            length2_mm,
            inactive_coils,
        } => {
            form.scenario = ScenarioKind::TwoLoad;
            form.end_type = end_type.clone();
            form.fixity = fixity.clone();
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.mean_dia = fmt_len(*mean_dia_mm, us);
            form.force1 = fmt_force(*force1_n, us);
            form.length1 = fmt_len(*length1_mm, us);
            form.force2 = fmt_force(*force2_n, us);
            form.length2 = fmt_len(*length2_mm, us);
            form.inactive = inactive_coils.map(|v| format!("{v}")).unwrap_or_default();
        }
        ScenarioSpec::RateBased {
            end_type,
            fixity,
            wire_dia_mm,
            mean_dia_mm,
            rate_n_per_m,
            free_length_mm,
            loads_n,
            inactive_coils,
        } => {
            form.scenario = ScenarioKind::RateBased;
            form.end_type = end_type.clone();
            form.fixity = fixity.clone();
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.mean_dia = fmt_len(*mean_dia_mm, us);
            form.rate = fmt_rate(*rate_n_per_m, us);
            form.free_length = fmt_len(*free_length_mm, us);
            form.loads = fmt_loads(loads_n, us);
            form.inactive = inactive_coils.map(|v| format!("{v}")).unwrap_or_default();
        }
        ScenarioSpec::Dimensional {
            end_type,
            fixity,
            wire_dia_mm,
            outer_dia_mm,
            active,
            free_length_mm,
            loads_n,
            inactive_coils,
        } => {
            form.scenario = ScenarioKind::Dimensional;
            form.end_type = end_type.clone();
            form.fixity = fixity.clone();
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.outer_dia = fmt_len(*outer_dia_mm, us);
            form.active = format!("{active}");
            form.free_length = fmt_len(*free_length_mm, us);
            form.loads = fmt_loads(loads_n, us);
            form.inactive = inactive_coils.map(|v| format!("{v}")).unwrap_or_default();
        }
        ScenarioSpec::MinWeight {
            end_type,
            fixity,
            required_rate_n_per_m,
            max_force_n,
            index_min,
            index_max,
            max_outer_dia_mm,
            candidate_diameters_mm,
            clash_allowance,
            inactive_coils,
        } => {
            form.scenario = ScenarioKind::MinWeight;
            form.end_type = end_type.clone();
            form.fixity = fixity.clone();
            form.rate = fmt_rate(*required_rate_n_per_m, us);
            form.max_force = fmt_force(*max_force_n, us);
            form.index_min = format!("{index_min}");
            form.index_max = format!("{index_max}");
            form.max_outer_dia = max_outer_dia_mm
                .map(|mm| fmt_len(mm, us))
                .unwrap_or_default();
            form.candidate_diameters = candidate_diameters_mm
                .iter()
                .map(|&mm| fmt_len(mm, us))
                .collect::<Vec<_>>()
                .join(", ");
            form.clash_allowance = format!("{clash_allowance}");
            form.inactive = inactive_coils.map(|v| format!("{v}")).unwrap_or_default();
        }
    }
}

fn compute_fatigue(
    form: &FormState,
    material: &springcore::Material,
    design: &SpringDesign,
    correction: springcore::CurvatureCorrection,
    us: UnitSystem,
) -> Result<FatigueStatus> {
    if form.fatigue_min.trim().is_empty() || form.fatigue_max.trim().is_empty() {
        return Ok(FatigueStatus::Skipped);
    }
    let fmin = Force::from_newtons(non_negative_force_n("fatigue min", &form.fatigue_min, us)?);
    let fmax = Force::from_newtons(non_negative_force_n("fatigue max", &form.fatigue_max, us)?);
    match analyze_fatigue(
        material,
        design.wire_dia,
        design.mean_dia,
        fmin,
        fmax,
        correction,
    ) {
        Ok(r) => Ok(FatigueStatus::Computed(r)),
        Err(SpringError::NoFatigueData(_)) => Ok(FatigueStatus::NoData),
        Err(e) => Err(e),
    }
}

/// Parse the form, solve the design, evaluate status, and (if cycle forces are
/// supplied) compute fatigue. Blank cycle-force fields yield [`FatigueStatus::Skipped`];
/// cycle forces supplied but no endurance data in the material yield
/// [`FatigueStatus::NoData`].
pub fn parse_and_solve(
    form: &FormState,
    material_name: &str,
    us: UnitSystem,
    materials: &MaterialStore,
    correction: springcore::CurvatureCorrection,
) -> Result<FormOutcome> {
    if form.scenario == ScenarioKind::MinWeight {
        let material = materials.get(material_name)?;
        let spec = build_spec(form, us)?;
        let req = springcore::min_weight_request_from_spec(&spec)?;
        let sol = springcore::solve_min_weight(material, &req, correction)?;
        let status = evaluate_status(&sol.design, material);
        let fatigue = compute_fatigue(form, material, &sol.design, correction, us)?;
        return Ok(FormOutcome {
            design: sol.design,
            status,
            fatigue,
            min_weight: Some(MinWeightExtra {
                binding: sol.binding,
                mass_kg: sol.mass_kg,
            }),
        });
    }

    // Preserve the original error precedence for the calculator paths: parse the
    // form first, then resolve the material (both unreachable failures in the GUI,
    // since the picker only offers valid names, but keeps behavior identical).
    let spec = build_spec(form, us)?;
    let material = materials.get(material_name)?;
    let saved = SavedDesign {
        material: material_name.to_string(),
        unit_system: us,
        design: springcore::DesignSpec::Compression(spec),
    };
    let design = saved.solve_with_material(material, correction)?;
    let status = evaluate_status(&design, material);
    let fatigue = compute_fatigue(form, material, &design, correction, us)?;

    Ok(FormOutcome {
        design,
        status,
        fatigue,
        min_weight: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::form_helpers::format_error;
    use approx::assert_relative_eq;
    use springcore::{MaterialSet, MaterialStore};

    fn default_store() -> MaterialStore {
        MaterialStore::new(MaterialSet::load_default())
    }

    fn rate_based_metric() -> FormState {
        FormState {
            scenario: ScenarioKind::RateBased,
            end_type: "squared_ground".into(),
            fixity: "fixed_fixed".into(),
            wire_dia: "2.0".into(),
            mean_dia: "20.0".into(),
            rate: "2.0".into(), // 2 N/mm = 2000 N/m (internal)
            free_length: "60.0".into(),
            loads: "10, 30".into(),
            fatigue_min: "10".into(),
            fatigue_max: "30".into(),
            ..Default::default()
        }
    }

    /// A valid PowerUser form (SquaredGround end type, so the end-type default
    /// inactive-coil count is 2; active = 10 for a round total_coils == 12 at
    /// the default).
    fn power_user_squared_ground_form() -> FormState {
        FormState {
            scenario: ScenarioKind::PowerUser,
            end_type: "squared_ground".into(),
            fixity: "fixed_fixed".into(),
            wire_dia: "2.0".into(),
            mean_dia: "20.0".into(),
            active: "10".into(),
            free_length: "60.0".into(),
            loads: "10, 30".into(),
            ..Default::default()
        }
    }

    #[test]
    fn is_blank_true_for_untouched_form_false_once_filled() {
        let mut f = FormState::default();
        assert!(f.is_blank(), "a default PowerUser form is blank");
        f.wire_dia = "2".into();
        assert!(!f.is_blank(), "filling a required field clears blank");
    }

    #[test]
    fn is_blank_dimensional_uses_outer_dia_not_mean_dia() {
        let mut f = FormState {
            scenario: ScenarioKind::Dimensional,
            ..FormState::default()
        };
        assert!(f.is_blank(), "an untouched Dimensional form is blank");
        // mean_dia is not a Dimensional input — filling it must not clear blank.
        f.mean_dia = "20".into();
        assert!(
            f.is_blank(),
            "mean_dia is not a Dimensional input; the form stays blank"
        );
        // outer_dia IS a Dimensional input.
        f.outer_dia = "30".into();
        assert!(
            !f.is_blank(),
            "outer_dia is a Dimensional input; the form is no longer blank"
        );
    }

    #[test]
    fn is_blank_minweight_ignores_prefilled_defaults_but_not_required_rate() {
        let mut f = FormState {
            scenario: ScenarioKind::MinWeight,
            ..FormState::default()
        };
        // index_min/index_max/clash_allowance default non-empty but are not user
        // input, so a default MinWeight form is still blank.
        assert!(f.is_blank(), "a default MinWeight form is blank");
        // ...but the required rate IS user input — and it is the first field shown,
        // so filling only it must clear blank.
        f.rate = "2".into();
        assert!(
            !f.is_blank(),
            "filling the required MinWeight rate clears blank"
        );
    }

    #[test]
    fn is_blank_minweight_typing_max_outer_dia_clears_blank() {
        let mut f = FormState {
            scenario: ScenarioKind::MinWeight,
            ..FormState::default()
        };
        // max_outer_dia is an optional cap, but — like `loads` — typing it signals intent,
        // so it must clear blank. It was previously (wrongly) excluded.
        f.max_outer_dia = "30".into();
        assert!(
            !f.is_blank(),
            "typing the optional max_outer_dia signals intent and clears blank"
        );
    }

    #[test]
    fn inactive_alone_does_not_count_toward_is_blank() {
        // The optional inactive-coil override, like the end-type selector, is not
        // one of PowerUser's required fields — filling it alone must not clear
        // the blank state (mirrors conical's `inactive_alone_does_not_count_toward_is_blank`).
        let f = FormState {
            inactive: "3".into(),
            ..FormState::default()
        };
        assert!(f.is_blank());
    }

    #[test]
    fn solves_rate_based_metric() {
        let set = default_store();
        let out = parse_and_solve(
            &rate_based_metric(),
            "Music Wire",
            springcore::UnitSystem::Metric,
            &set,
            springcore::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert_relative_eq!(
            out.design.rate.newtons_per_meter(),
            2000.0,
            max_relative = 1e-6
        );
        assert_eq!(out.design.load_points.len(), 2);
        assert!(matches!(out.fatigue, FatigueStatus::Computed(_)));
    }

    #[test]
    fn us_units_are_converted() {
        let set = default_store();
        let mut form = rate_based_metric();
        form.wire_dia = "0.08".into(); // inches
        form.mean_dia = "0.8".into();
        form.rate = "10".into(); // lbf/in
        form.free_length = "2.0".into();
        form.loads = "2".into();
        form.fatigue_min = "1".into();
        form.fatigue_max = "2".into();
        let out = parse_and_solve(
            &form,
            "Music Wire",
            springcore::UnitSystem::Us,
            &set,
            springcore::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert_relative_eq!(out.design.wire_dia.inches(), 0.08, max_relative = 1e-9);
    }

    /// Rate field "2" in N/mm (metric) must store 2000 N/m internally.
    /// Rate field "10" lbf/in (US) must remain unchanged via SpringRate conversion.
    #[test]
    fn rate_conversion_direction() {
        let set = default_store();

        // Metric: typing "2" into an N/mm-labeled field → 2000 N/m stored
        let metric_form = rate_based_metric(); // rate = "2.0" N/mm
        let out = parse_and_solve(
            &metric_form,
            "Music Wire",
            springcore::UnitSystem::Metric,
            &set,
            springcore::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert_relative_eq!(
            out.design.rate.newtons_per_meter(),
            2000.0,
            max_relative = 1e-6,
        );

        // US: lbf/in parse is unchanged — SpringRate conversion handles it
        let mut us_form = rate_based_metric();
        us_form.wire_dia = "0.08".into();
        us_form.mean_dia = "0.8".into();
        us_form.rate = "10".into(); // 10 lbf/in
        us_form.free_length = "2.0".into();
        us_form.loads = "2".into();
        us_form.fatigue_min = "1".into();
        us_form.fatigue_max = "2".into();
        let us_out = parse_and_solve(
            &us_form,
            "Music Wire",
            springcore::UnitSystem::Us,
            &set,
            springcore::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        // 10 lbf/in ≈ 1751.27 N/m
        assert_relative_eq!(
            us_out.design.rate.pounds_per_inch(),
            10.0,
            max_relative = 1e-6,
        );
    }

    #[test]
    fn bad_number_is_an_error() {
        let set = default_store();
        let mut form = rate_based_metric();
        form.wire_dia = "abc".into();
        assert!(parse_and_solve(
            &form,
            "Music Wire",
            springcore::UnitSystem::Metric,
            &set,
            springcore::CurvatureCorrection::Bergstrasser
        )
        .is_err());
    }

    #[test]
    fn zero_wire_diameter_is_an_error() {
        let set = default_store();
        let mut form = rate_based_metric();
        form.wire_dia = "0".into();
        let err = parse_and_solve(
            &form,
            "Music Wire",
            springcore::UnitSystem::Metric,
            &set,
            springcore::CurvatureCorrection::Bergstrasser,
        )
        .unwrap_err();
        assert!(matches!(err, SpringError::InconsistentInputs(_)));
    }

    #[test]
    fn negative_wire_diameter_is_an_error() {
        let set = default_store();
        let mut form = rate_based_metric();
        form.wire_dia = "-1.0".into();
        let err = parse_and_solve(
            &form,
            "Music Wire",
            springcore::UnitSystem::Metric,
            &set,
            springcore::CurvatureCorrection::Bergstrasser,
        )
        .unwrap_err();
        assert!(matches!(err, SpringError::InconsistentInputs(_)));
    }

    #[test]
    fn zero_load_is_accepted() {
        // A zero operating load is physically valid (unloaded check).
        let set = default_store();
        let mut form = rate_based_metric();
        form.loads = "0".into();
        // Should not error on the zero load itself (may still fail for other reasons,
        // but the error must not be about the zero load value).
        let result = parse_and_solve(
            &form,
            "Music Wire",
            springcore::UnitSystem::Metric,
            &set,
            springcore::CurvatureCorrection::Bergstrasser,
        );
        if let Err(SpringError::InconsistentInputs(msg)) = &result {
            assert!(
                !msg.contains("load must be zero or greater"),
                "zero load must be accepted; got: {msg}"
            );
        }
    }

    #[test]
    fn negative_load_is_rejected_by_positivity_guard() {
        // Negative loads are unphysical; the non_negative_force_n guard must reject them.
        let set = default_store();
        let mut form = rate_based_metric();
        form.loads = "-5".into();
        let err = parse_and_solve(
            &form,
            "Music Wire",
            springcore::UnitSystem::Metric,
            &set,
            springcore::CurvatureCorrection::Bergstrasser,
        )
        .unwrap_err();
        match &err {
            SpringError::InconsistentInputs(msg) => {
                assert!(
                    msg.contains("must be zero or greater"),
                    "expected 'must be zero or greater' in error; got: {msg}"
                );
            }
            other => panic!("expected InconsistentInputs, got: {other}"),
        }
    }

    #[test]
    fn wire_dia_zero_triggers_greater_than_zero_error() {
        // Exercises the positivity guard on a dimensional field; wire_dia = "0"
        // must produce an error mentioning "greater than zero".
        let set = default_store();
        let mut form = rate_based_metric();
        form.wire_dia = "0".into();
        let err = parse_and_solve(
            &form,
            "Music Wire",
            springcore::UnitSystem::Metric,
            &set,
            springcore::CurvatureCorrection::Bergstrasser,
        )
        .unwrap_err();
        match &err {
            SpringError::InconsistentInputs(msg) => {
                assert!(
                    msg.contains("greater than zero"),
                    "expected 'greater than zero' in error; got: {msg}"
                );
            }
            other => panic!("expected InconsistentInputs, got: {other}"),
        }
    }

    #[test]
    fn nan_in_field_is_an_error() {
        // Rust's f64 parse accepts "nan"; the form layer must reject it.
        let set = default_store();
        let mut form = rate_based_metric();
        form.wire_dia = "nan".into();
        let err = parse_and_solve(
            &form,
            "Music Wire",
            springcore::UnitSystem::Metric,
            &set,
            springcore::CurvatureCorrection::Bergstrasser,
        )
        .unwrap_err();
        assert!(matches!(err, SpringError::InconsistentInputs(_)));
    }

    #[test]
    fn inf_in_field_is_an_error() {
        // Rust's f64 parse accepts "inf"; the form layer must reject it.
        let set = default_store();
        let mut form = rate_based_metric();
        form.wire_dia = "inf".into();
        let err = parse_and_solve(
            &form,
            "Music Wire",
            springcore::UnitSystem::Metric,
            &set,
            springcore::CurvatureCorrection::Bergstrasser,
        )
        .unwrap_err();
        assert!(matches!(err, SpringError::InconsistentInputs(_)));
    }

    #[test]
    fn fatigue_no_data_for_material_without_endurance() {
        // Stainless 302 has no cited endurance data; when cycle forces are
        // supplied the result must be NoData, not Skipped.
        let set = default_store();
        let form = rate_based_metric();
        // rate_based_metric already sets fatigue_min/max, so forces are present.
        let out = parse_and_solve(
            &form,
            "Stainless 302",
            springcore::UnitSystem::Metric,
            &set,
            springcore::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert!(
            matches!(out.fatigue, FatigueStatus::NoData),
            "expected NoData for Stainless 302, got: {:?}",
            out.fatigue,
        );
    }

    #[test]
    fn fatigue_skipped_when_cycle_forces_blank() {
        // Leaving both cycle-force fields blank must yield Skipped, not NoData.
        let set = default_store();
        let mut form = rate_based_metric();
        form.fatigue_min = String::new();
        form.fatigue_max = String::new();
        let out = parse_and_solve(
            &form,
            "Music Wire",
            springcore::UnitSystem::Metric,
            &set,
            springcore::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert!(
            matches!(out.fatigue, FatigueStatus::Skipped),
            "expected Skipped when cycle forces are blank, got: {:?}",
            out.fatigue,
        );
    }

    #[test]
    fn solves_min_weight_with_extras() {
        let set = default_store();
        let form = FormState {
            scenario: ScenarioKind::MinWeight,
            end_type: "squared_ground".into(),
            fixity: "fixed_fixed".into(),
            rate: "2".into(), // 2 N/mm = 2000 N/m (internal)
            max_force: "50".into(),
            index_min: "4".into(),
            index_max: "12".into(),
            candidate_diameters: "1.5, 2.0, 2.5, 3.0".into(),
            clash_allowance: "0.15".into(),
            ..Default::default()
        };
        let out = parse_and_solve(
            &form,
            "Music Wire",
            springcore::UnitSystem::Metric,
            &set,
            springcore::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert!(out.min_weight.is_some());
        assert!(out.min_weight.unwrap().mass_kg > 0.0);
        assert!(out.design.buckling_stable);
    }

    #[test]
    fn build_spec_populate_round_trip_metric() {
        // A metric form produces spec1; populate_from_spec writes it back into
        // another form; building that second form gives spec2 == spec1.
        let us = springcore::UnitSystem::Metric;
        let form = rate_based_metric();
        let spec1 = build_spec(&form, us).unwrap();

        let mut form2 = FormState::default();
        populate_from_spec(&mut form2, &spec1, us);

        let spec2 = build_spec(&form2, us).unwrap();
        assert_eq!(spec1, spec2);
    }

    #[test]
    fn inactive_field_round_trips_through_spec() {
        let mut form = FormState {
            scenario: ScenarioKind::PowerUser,
            inactive: "3".into(),
            ..power_user_squared_ground_form()
        };
        let spec = build_spec(&form, UnitSystem::Metric).unwrap();
        assert!(
            matches!(spec, ScenarioSpec::PowerUser { inactive_coils: Some(v), .. } if (v - 3.0).abs() < 1e-9)
        );
        form.inactive.clear();
        populate_from_spec(&mut form, &spec, UnitSystem::Metric);
        assert_eq!(form.inactive, "3"); // Some(3.0) → "3"
    }

    #[test]
    fn inactive_override_shows_in_total_coils() {
        let materials = MaterialStore::new(MaterialSet::load_default());
        let form = FormState {
            scenario: ScenarioKind::PowerUser,
            inactive: "4".into(),
            ..power_user_squared_ground_form() // active=10, SquaredGround default 2
        };
        let out = parse_and_solve(
            &form,
            "Music Wire",
            UnitSystem::Metric,
            &materials,
            springcore::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert_relative_eq!(out.design.total_coils, 14.0, max_relative = 1e-9); // active 10 + inactive 4
    }

    fn min_weight_metric() -> FormState {
        FormState {
            scenario: ScenarioKind::MinWeight,
            end_type: "squared_ground".into(),
            fixity: "fixed_fixed".into(),
            rate: "2".into(), // 2 N/mm = 2000 N/m (internal)
            max_force: "50".into(),
            index_min: "4".into(),
            index_max: "12".into(),
            max_outer_dia: "25".into(),
            candidate_diameters: "1.5, 2.5, 3".into(),
            clash_allowance: "0.15".into(),
            ..Default::default()
        }
    }

    #[test]
    fn build_spec_populate_round_trip_min_weight_metric() {
        // MinWeight round-trip: spec1 → populate_from_spec → spec2 must equal spec1.
        let us = springcore::UnitSystem::Metric;
        let form = min_weight_metric();
        let spec1 = build_spec(&form, us).unwrap();

        let mut form2 = FormState::default();
        populate_from_spec(&mut form2, &spec1, us);

        let spec2 = build_spec(&form2, us).unwrap();
        assert_eq!(spec1, spec2);
    }

    #[test]
    fn empty_candidate_diameters_is_an_error() {
        let set = default_store();
        let mut form = min_weight_metric();
        form.candidate_diameters = String::new();
        assert!(parse_and_solve(
            &form,
            "Music Wire",
            springcore::UnitSystem::Metric,
            &set,
            springcore::CurvatureCorrection::Bergstrasser
        )
        .is_err());
    }

    #[test]
    fn min_weight_binding_constraint_is_valid() {
        use springcore::BindingConstraint;
        let set = default_store();
        let out = parse_and_solve(
            &min_weight_metric(),
            "Music Wire",
            springcore::UnitSystem::Metric,
            &set,
            springcore::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        let mw = out.min_weight.unwrap();
        assert!(matches!(
            mw.binding,
            BindingConstraint::Stress | BindingConstraint::Index | BindingConstraint::OuterDiameter
        ));
    }

    // --- format_error tests --------------------------------------------------

    /// A US form whose wire diameter is below the material's valid range
    /// produces a DiameterOutOfRange error. format_error(…, Us) must render
    /// inches (not metres) and must not contain " m" as a unit suffix.
    #[test]
    fn format_error_us_uses_inches() {
        let set = default_store();
        // Music Wire valid range: 0.10 mm – 6.5 mm (≈ 0.00394 – 0.256 in).
        // 0.001 in ≈ 0.0254 mm — well below 0.10 mm minimum.
        let form = FormState {
            scenario: ScenarioKind::RateBased,
            end_type: "squared_ground".into(),
            fixity: "fixed_fixed".into(),
            wire_dia: "0.001".into(), // 0.001 in — below valid minimum
            mean_dia: "0.8".into(),
            rate: "10".into(),
            free_length: "2.0".into(),
            loads: "2".into(),
            ..Default::default()
        };
        let err = parse_and_solve(
            &form,
            "Music Wire",
            springcore::UnitSystem::Us,
            &set,
            springcore::CurvatureCorrection::Bergstrasser,
        )
        .unwrap_err();
        assert!(
            matches!(err, SpringError::DiameterOutOfRange { .. }),
            "expected DiameterOutOfRange, got: {err}"
        );
        let msg = format_error(&err, springcore::UnitSystem::Us);
        // Must contain the inch unit token.
        assert!(
            msg.contains(" in"),
            "US error message must contain ' in': {msg}"
        );
        // Must not contain a millimetre or bare-metre token.
        assert!(
            !msg.contains("mm"),
            "US error message must not contain 'mm': {msg}"
        );
        assert!(
            !msg.contains(" m ") && !msg.ends_with(" m"),
            "US error message must not contain a bare metre suffix: {msg}"
        );
        // The metric variant must contain "mm" and not "in".
        let msg_metric = format_error(&err, springcore::UnitSystem::Metric);
        assert!(
            msg_metric.contains("mm"),
            "metric error message must contain 'mm': {msg_metric}"
        );
        assert!(
            !msg_metric.contains(" in"),
            "metric error message must not contain ' in': {msg_metric}"
        );
    }

    /// A metric form whose wire diameter is above the valid range produces a
    /// DiameterOutOfRange error. format_error(…, Metric) must render mm.
    #[test]
    fn parse_and_solve_accepts_material_store() {
        let store = springcore::MaterialStore::new(MaterialSet::load_default());
        let form = rate_based_metric();
        assert!(parse_and_solve(
            &form,
            "Music Wire",
            springcore::UnitSystem::Metric,
            &store,
            springcore::CurvatureCorrection::Bergstrasser
        )
        .is_ok());
    }

    #[test]
    fn format_error_metric_uses_mm() {
        let set = default_store();
        // Music Wire valid max: 6.5 mm. Use 100 mm — far out of range.
        let form = FormState {
            scenario: ScenarioKind::RateBased,
            end_type: "squared_ground".into(),
            fixity: "fixed_fixed".into(),
            wire_dia: "100.0".into(), // 100 mm — above valid max
            mean_dia: "200.0".into(),
            rate: "2.0".into(), // 2 N/mm = 2000 N/m (internal)
            free_length: "60.0".into(),
            loads: "10".into(),
            ..Default::default()
        };
        let err = parse_and_solve(
            &form,
            "Music Wire",
            springcore::UnitSystem::Metric,
            &set,
            springcore::CurvatureCorrection::Bergstrasser,
        )
        .unwrap_err();
        assert!(
            matches!(err, SpringError::DiameterOutOfRange { .. }),
            "expected DiameterOutOfRange, got: {err}"
        );
        let msg = format_error(&err, springcore::UnitSystem::Metric);
        assert!(
            msg.contains("mm"),
            "metric error message must contain 'mm': {msg}"
        );
        assert!(
            !msg.contains(" in"),
            "metric error message must not contain 'in': {msg}"
        );
    }

    #[test]
    fn huge_finite_fatigue_force_surfaces_engine_guard_as_form_error() {
        // 1e305 N parses (finite), passes the non-negative helper, and overflows
        // the corrected shear stress inside the engine — the Task-1 output guard
        // must surface as a whole-form error, never Ok(inf) rows.
        let set = default_store();
        let mut form = rate_based_metric();
        form.fatigue_min = "0".into();
        form.fatigue_max = "1e305".into();
        let err = parse_and_solve(
            &form,
            "Music Wire",
            springcore::UnitSystem::Metric,
            &set,
            springcore::CurvatureCorrection::Bergstrasser,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("produced a non-finite result"),
            "got: {err}"
        );
    }
}
