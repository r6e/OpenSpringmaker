//! Pure form-to-design logic. No iced dependency, so it is unit-testable.

use springcore::units::{Force, Length, SpringRate};
use springcore::{
    analyze_fatigue, evaluate_status, DesignStatus, FatigueResult, MaterialSet, Result,
    SavedDesign, ScenarioSpec, SpringDesign, SpringError, UnitSystem,
};

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
    pub material: String,
    pub unit_system: UnitSystem,
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
            material: "Music Wire".into(),
            unit_system: UnitSystem::Metric,
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

/// Extra outputs produced only by the Min Weight optimisation path.
#[derive(Debug, Clone)]
pub struct MinWeightExtra {
    pub binding: springcore::BindingConstraint,
    pub mass_kg: f64,
}

/// A solved form: the design plus its status and optional fatigue result.
#[derive(Debug, Clone)]
pub struct FormOutcome {
    pub design: SpringDesign,
    pub status: DesignStatus,
    pub fatigue: Option<FatigueResult>,
    pub min_weight: Option<MinWeightExtra>,
}

fn num(field: &str, value: &str) -> Result<f64> {
    value
        .trim()
        .parse::<f64>()
        .map_err(|_| SpringError::InconsistentInputs(format!("{field} is not a number: '{value}'")))
}

fn length_mm(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    let v = num(field, value)?;
    Ok(match us {
        UnitSystem::Us => Length::from_inches(v).millimeters(),
        UnitSystem::Metric => Length::from_millimeters(v).millimeters(),
    })
}

fn force_n(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    let v = num(field, value)?;
    Ok(match us {
        UnitSystem::Us => Force::from_pounds_force(v).newtons(),
        UnitSystem::Metric => Force::from_newtons(v).newtons(),
    })
}

fn rate_npm(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    let v = num(field, value)?;
    Ok(match us {
        UnitSystem::Us => SpringRate::from_pounds_per_inch(v).newtons_per_meter(),
        UnitSystem::Metric => SpringRate::from_newtons_per_meter(v).newtons_per_meter(),
    })
}

fn loads_n(value: &str, us: UnitSystem) -> Result<Vec<f64>> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| force_n("load", s, us))
        .collect()
}

fn build_spec(form: &FormState) -> Result<ScenarioSpec> {
    let us = form.unit_system;
    Ok(match form.scenario {
        ScenarioKind::PowerUser => ScenarioSpec::PowerUser {
            end_type: form.end_type.clone(),
            fixity: form.fixity.clone(),
            wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
            mean_dia_mm: length_mm("mean diameter", &form.mean_dia, us)?,
            active: num("active coils", &form.active)?,
            free_length_mm: length_mm("free length", &form.free_length, us)?,
            loads_n: loads_n(&form.loads, us)?,
        },
        ScenarioKind::TwoLoad => ScenarioSpec::TwoLoad {
            end_type: form.end_type.clone(),
            fixity: form.fixity.clone(),
            wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
            mean_dia_mm: length_mm("mean diameter", &form.mean_dia, us)?,
            force1_n: force_n("force 1", &form.force1, us)?,
            length1_mm: length_mm("length 1", &form.length1, us)?,
            force2_n: force_n("force 2", &form.force2, us)?,
            length2_mm: length_mm("length 2", &form.length2, us)?,
        },
        ScenarioKind::RateBased => ScenarioSpec::RateBased {
            end_type: form.end_type.clone(),
            fixity: form.fixity.clone(),
            wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
            mean_dia_mm: length_mm("mean diameter", &form.mean_dia, us)?,
            rate_n_per_m: rate_npm("rate", &form.rate, us)?,
            free_length_mm: length_mm("free length", &form.free_length, us)?,
            loads_n: loads_n(&form.loads, us)?,
        },
        ScenarioKind::Dimensional => ScenarioSpec::Dimensional {
            end_type: form.end_type.clone(),
            fixity: form.fixity.clone(),
            wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
            outer_dia_mm: length_mm("outer diameter", &form.outer_dia, us)?,
            active: num("active coils", &form.active)?,
            free_length_mm: length_mm("free length", &form.free_length, us)?,
            loads_n: loads_n(&form.loads, us)?,
        },
        ScenarioKind::MinWeight => {
            let us = form.unit_system;
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
                max_force_n: force_n("max force", &form.max_force, us)?,
                index_min: num("index min", &form.index_min)?,
                index_max: num("index max", &form.index_max)?,
                max_outer_dia_mm,
                candidate_diameters_mm: diameters_mm,
                clash_allowance: num("clash allowance", &form.clash_allowance)?,
            }
        }
    })
}

/// Public wrapper around `build_spec` used by `app.rs` for save dialogs.
pub fn build_spec_public(form: &FormState) -> Result<ScenarioSpec> {
    build_spec(form)
}

/// Write a `ScenarioSpec`'s fields back into `form`, converting SI-stored
/// mm/N values to display units per `form.unit_system`.
///
/// After calling this, `build_spec_public(form)` should reproduce a spec
/// equal to `spec` (round-trip invariant).
pub fn populate_from_spec(form: &mut FormState, spec: &ScenarioSpec) {
    let us = form.unit_system;

    /// Convert mm (SI internal) → display string.
    fn fmt_len(mm: f64, us: UnitSystem) -> String {
        match us {
            UnitSystem::Metric => format!("{mm}"),
            UnitSystem::Us => {
                use springcore::units::Length;
                format!("{}", Length::from_millimeters(mm).inches())
            }
        }
    }

    /// Convert N → display string.
    fn fmt_force(n: f64, us: UnitSystem) -> String {
        match us {
            UnitSystem::Metric => format!("{n}"),
            UnitSystem::Us => {
                use springcore::units::Force;
                format!("{}", Force::from_newtons(n).pounds_force())
            }
        }
    }

    /// Convert N/m → display string.
    fn fmt_rate(npm: f64, us: UnitSystem) -> String {
        match us {
            UnitSystem::Metric => format!("{npm}"),
            UnitSystem::Us => {
                use springcore::units::SpringRate;
                format!(
                    "{}",
                    SpringRate::from_newtons_per_meter(npm).pounds_per_inch()
                )
            }
        }
    }

    /// Join a slice of newtons values → comma-separated display string.
    fn fmt_loads(loads: &[f64], us: UnitSystem) -> String {
        loads
            .iter()
            .map(|&n| fmt_force(n, us))
            .collect::<Vec<_>>()
            .join(", ")
    }

    match spec {
        ScenarioSpec::PowerUser {
            end_type,
            fixity,
            wire_dia_mm,
            mean_dia_mm,
            active,
            free_length_mm,
            loads_n,
        } => {
            form.scenario = ScenarioKind::PowerUser;
            form.end_type = end_type.clone();
            form.fixity = fixity.clone();
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.mean_dia = fmt_len(*mean_dia_mm, us);
            form.active = format!("{active}");
            form.free_length = fmt_len(*free_length_mm, us);
            form.loads = fmt_loads(loads_n, us);
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
        }
        ScenarioSpec::RateBased {
            end_type,
            fixity,
            wire_dia_mm,
            mean_dia_mm,
            rate_n_per_m,
            free_length_mm,
            loads_n,
        } => {
            form.scenario = ScenarioKind::RateBased;
            form.end_type = end_type.clone();
            form.fixity = fixity.clone();
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.mean_dia = fmt_len(*mean_dia_mm, us);
            form.rate = fmt_rate(*rate_n_per_m, us);
            form.free_length = fmt_len(*free_length_mm, us);
            form.loads = fmt_loads(loads_n, us);
        }
        ScenarioSpec::Dimensional {
            end_type,
            fixity,
            wire_dia_mm,
            outer_dia_mm,
            active,
            free_length_mm,
            loads_n,
        } => {
            form.scenario = ScenarioKind::Dimensional;
            form.end_type = end_type.clone();
            form.fixity = fixity.clone();
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.outer_dia = fmt_len(*outer_dia_mm, us);
            form.active = format!("{active}");
            form.free_length = fmt_len(*free_length_mm, us);
            form.loads = fmt_loads(loads_n, us);
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
        }
    }
}

fn compute_fatigue(
    form: &FormState,
    material: &springcore::Material,
    design: &SpringDesign,
) -> Result<Option<FatigueResult>> {
    if form.fatigue_min.trim().is_empty() || form.fatigue_max.trim().is_empty() {
        return Ok(None);
    }
    let fmin = Force::from_newtons(force_n("fatigue min", &form.fatigue_min, form.unit_system)?);
    let fmax = Force::from_newtons(force_n("fatigue max", &form.fatigue_max, form.unit_system)?);
    match analyze_fatigue(material, design.wire_dia, design.mean_dia, fmin, fmax) {
        Ok(r) => Ok(Some(r)),
        Err(SpringError::NoFatigueData(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

fn build_min_weight_request(form: &FormState) -> Result<springcore::MinWeightRequest> {
    let us = form.unit_system;
    let diameters: Vec<Length> = form
        .candidate_diameters
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| length_mm("candidate diameter", s, us).map(Length::from_millimeters))
        .collect::<Result<_>>()?;
    if diameters.is_empty() {
        return Err(SpringError::InconsistentInputs(
            "provide at least one candidate wire diameter".into(),
        ));
    }
    let max_outer_dia = if form.max_outer_dia.trim().is_empty() {
        None
    } else {
        Some(Length::from_millimeters(length_mm(
            "max outer diameter",
            &form.max_outer_dia,
            us,
        )?))
    };
    Ok(springcore::MinWeightRequest {
        end_type: springcore::persistence_parse_end_type(&form.end_type)?,
        fixity: springcore::persistence_parse_fixity(&form.fixity)?,
        required_rate: SpringRate::from_newtons_per_meter(rate_npm("rate", &form.rate, us)?),
        max_force: Force::from_newtons(force_n("max force", &form.max_force, us)?),
        index_bounds: (
            num("index min", &form.index_min)?,
            num("index max", &form.index_max)?,
        ),
        max_outer_dia,
        candidate_diameters: diameters,
        clash_allowance: num("clash allowance", &form.clash_allowance)?,
    })
}

/// Parse the form, solve the design, evaluate status, and (if a cycle and endurance
/// data are present) compute fatigue. Missing endurance data degrades to `None`.
pub fn parse_and_solve(form: &FormState, materials: &MaterialSet) -> Result<FormOutcome> {
    if form.scenario == ScenarioKind::MinWeight {
        let material = materials.get(&form.material)?;
        let req = build_min_weight_request(form)?;
        let sol = springcore::solve_min_weight(material, &req)?;
        let status = evaluate_status(&sol.design, material);
        let fatigue = compute_fatigue(form, material, &sol.design)?;
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

    let saved = SavedDesign {
        material: form.material.clone(),
        unit_system: form.unit_system,
        scenario: build_spec(form)?,
    };
    let design = saved.solve(materials)?;
    let material = materials.get(&form.material)?;
    let status = evaluate_status(&design, material);
    let fatigue = compute_fatigue(form, material, &design)?;

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
    use approx::assert_relative_eq;
    use springcore::MaterialSet;

    fn rate_based_metric() -> FormState {
        FormState {
            material: "Music Wire".into(),
            unit_system: springcore::UnitSystem::Metric,
            scenario: ScenarioKind::RateBased,
            end_type: "squared_ground".into(),
            fixity: "fixed_fixed".into(),
            wire_dia: "2.0".into(),
            mean_dia: "20.0".into(),
            rate: "2000.0".into(),
            free_length: "60.0".into(),
            loads: "10, 30".into(),
            fatigue_min: "10".into(),
            fatigue_max: "30".into(),
            ..Default::default()
        }
    }

    #[test]
    fn solves_rate_based_metric() {
        let set = MaterialSet::load_default();
        let out = parse_and_solve(&rate_based_metric(), &set).unwrap();
        assert_relative_eq!(
            out.design.rate.newtons_per_meter(),
            2000.0,
            max_relative = 1e-6
        );
        assert_eq!(out.design.load_points.len(), 2);
        assert!(out.fatigue.is_some());
    }

    #[test]
    fn us_units_are_converted() {
        let set = MaterialSet::load_default();
        let mut form = rate_based_metric();
        form.unit_system = springcore::UnitSystem::Us;
        form.wire_dia = "0.08".into(); // inches
        form.mean_dia = "0.8".into();
        form.rate = "10".into(); // lbf/in
        form.free_length = "2.0".into();
        form.loads = "2".into();
        form.fatigue_min = "1".into();
        form.fatigue_max = "2".into();
        let out = parse_and_solve(&form, &set).unwrap();
        assert_relative_eq!(out.design.wire_dia.inches(), 0.08, max_relative = 1e-9);
    }

    #[test]
    fn bad_number_is_an_error() {
        let set = MaterialSet::load_default();
        let mut form = rate_based_metric();
        form.wire_dia = "abc".into();
        assert!(parse_and_solve(&form, &set).is_err());
    }

    #[test]
    fn fatigue_absent_for_material_without_endurance() {
        let set = MaterialSet::load_default();
        let mut form = rate_based_metric();
        form.material = "Stainless 302".into();
        let out = parse_and_solve(&form, &set).unwrap();
        assert!(out.fatigue.is_none());
    }

    #[test]
    fn solves_min_weight_with_extras() {
        let set = MaterialSet::load_default();
        let form = FormState {
            material: "Music Wire".into(),
            unit_system: springcore::UnitSystem::Metric,
            scenario: ScenarioKind::MinWeight,
            end_type: "squared_ground".into(),
            fixity: "fixed_fixed".into(),
            rate: "2000".into(),
            max_force: "50".into(),
            index_min: "4".into(),
            index_max: "12".into(),
            candidate_diameters: "1.5, 2.0, 2.5, 3.0".into(),
            clash_allowance: "0.15".into(),
            ..Default::default()
        };
        let out = parse_and_solve(&form, &set).unwrap();
        assert!(out.min_weight.is_some());
        assert!(out.min_weight.unwrap().mass_kg > 0.0);
        assert!(out.design.buckling_stable);
    }

    #[test]
    fn build_spec_public_populate_round_trip_metric() {
        // A metric form produces spec1; populate_from_spec writes it back into
        // another form; building that second form gives spec2 == spec1.
        let form = rate_based_metric();
        let spec1 = build_spec_public(&form).unwrap();

        let mut form2 = FormState {
            unit_system: springcore::UnitSystem::Metric,
            ..FormState::default()
        };
        populate_from_spec(&mut form2, &spec1);

        let spec2 = build_spec_public(&form2).unwrap();
        assert_eq!(spec1, spec2);
    }
}
