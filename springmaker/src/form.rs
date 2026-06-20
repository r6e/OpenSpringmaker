//! Pure form-to-design logic. No iced dependency, so it is unit-testable.

// Items are consumed by the GUI (a later task); suppress premature dead-code warnings.
#![allow(dead_code)]

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
}

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
        }
    }
}

/// A solved form: the design plus its status and optional fatigue result.
#[derive(Debug, Clone)]
pub struct FormOutcome {
    pub design: SpringDesign,
    pub status: DesignStatus,
    pub fatigue: Option<FatigueResult>,
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
    })
}

/// Parse the form, solve the design, evaluate status, and (if a cycle and endurance
/// data are present) compute fatigue. Missing endurance data degrades to `None`.
pub fn parse_and_solve(form: &FormState, materials: &MaterialSet) -> Result<FormOutcome> {
    let saved = SavedDesign {
        material: form.material.clone(),
        unit_system: form.unit_system,
        scenario: build_spec(form)?,
    };
    let design = saved.solve(materials)?;
    let material = materials.get(&form.material)?;
    let status = evaluate_status(&design, material);

    let fatigue = if form.fatigue_min.trim().is_empty() || form.fatigue_max.trim().is_empty() {
        None
    } else {
        let fmin =
            Force::from_newtons(force_n("fatigue min", &form.fatigue_min, form.unit_system)?);
        let fmax =
            Force::from_newtons(force_n("fatigue max", &form.fatigue_max, form.unit_system)?);
        match analyze_fatigue(material, design.wire_dia, design.mean_dia, fmin, fmax) {
            Ok(r) => Some(r),
            Err(SpringError::NoFatigueData(_)) => None,
            Err(e) => return Err(e),
        }
    };

    Ok(FormOutcome {
        design,
        status,
        fatigue,
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
}
