//! Human-readable persistence of a single design. Stores the user's inputs
//! (not computed outputs); the design is recomputed on load.

use crate::design::SpringDesign;
use crate::end_type::EndType;
use crate::material::MaterialSet;
use crate::mechanics::EndFixity;
use crate::optimize::{solve_min_weight, MinWeightRequest};
use crate::scenario::{Dimensional, PowerUser, RateBased, Scenario, TwoLoad};
use crate::units::{Force, Length, SpringRate};
use crate::{Result, SpringError};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Display unit system chosen for a saved design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitSystem {
    Us,
    Metric,
}

/// Serializable scenario inputs (SI-friendly primitives; lengths in mm).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ScenarioSpec {
    PowerUser {
        end_type: String,
        fixity: String,
        wire_dia_mm: f64,
        mean_dia_mm: f64,
        active: f64,
        free_length_mm: f64,
        loads_n: Vec<f64>,
    },
    TwoLoad {
        end_type: String,
        fixity: String,
        wire_dia_mm: f64,
        mean_dia_mm: f64,
        force1_n: f64,
        length1_mm: f64,
        force2_n: f64,
        length2_mm: f64,
    },
    RateBased {
        end_type: String,
        fixity: String,
        wire_dia_mm: f64,
        mean_dia_mm: f64,
        rate_n_per_m: f64,
        free_length_mm: f64,
        loads_n: Vec<f64>,
    },
    Dimensional {
        end_type: String,
        fixity: String,
        wire_dia_mm: f64,
        outer_dia_mm: f64,
        active: f64,
        free_length_mm: f64,
        loads_n: Vec<f64>,
    },
    MinWeight {
        end_type: String,
        fixity: String,
        required_rate_n_per_m: f64,
        max_force_n: f64,
        index_min: f64,
        index_max: f64,
        max_outer_dia_mm: Option<f64>,
        candidate_diameters_mm: Vec<f64>,
        clash_allowance: f64,
    },
}

/// A persisted design: material, display units, and scenario inputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedDesign {
    pub material: String,
    pub unit_system: UnitSystem,
    pub scenario: ScenarioSpec,
}

fn parse_end_type(s: &str) -> Result<EndType> {
    Ok(match s {
        "plain" => EndType::Plain,
        "plain_ground" => EndType::PlainGround,
        "squared" => EndType::Squared,
        "squared_ground" => EndType::SquaredGround,
        other => return Err(SpringError::DataFile(format!("unknown end_type: {other}"))),
    })
}

fn parse_fixity(s: &str) -> Result<EndFixity> {
    Ok(match s {
        "fixed_fixed" => EndFixity::FixedFixed,
        "fixed_pinned" => EndFixity::FixedPinned,
        "pinned_pinned" => EndFixity::PinnedPinned,
        "fixed_free" => EndFixity::FixedFree,
        other => return Err(SpringError::DataFile(format!("unknown fixity: {other}"))),
    })
}

fn forces(v: &[f64]) -> Vec<Force> {
    v.iter().map(|&n| Force::from_newtons(n)).collect()
}

/// Build a [`MinWeightRequest`] from a [`ScenarioSpec::MinWeight`] variant.
///
/// Returns an error if `spec` is not the `MinWeight` variant.
pub fn min_weight_request_from_spec(spec: &ScenarioSpec) -> Result<MinWeightRequest> {
    match spec {
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
            // required_rate must be positive and finite; a rate of 0 or ∞ makes
            // active_coils_for_rate diverge (Na → ∞) which the optimizer cannot handle.
            if !required_rate_n_per_m.is_finite() || *required_rate_n_per_m <= 0.0 {
                return Err(SpringError::InconsistentInputs(
                    "required_rate must be a positive finite number (N/m)".into(),
                ));
            }
            // index_bounds must satisfy 0 < index_min < index_max.
            if !(*index_min > 0.0 && *index_min < *index_max) {
                return Err(SpringError::InconsistentInputs(format!(
                    "index bounds must satisfy 0 < index_min < index_max; \
                     got index_min={index_min}, index_max={index_max}"
                )));
            }
            // The optimizer's single-endpoint feasibility test in best_mean_dia is only
            // valid when τ(D) is monotonic increasing, which holds only for C ≥ C*
            // where C* = 1 + √3/2 ≈ 1.866 (minimum of the Wahl-corrected stress curve,
            // from d/dC[Kw·C] = 0 ⟹ 4C²−8C+1 = 0).
            let c_star = 1.0 + 3.0_f64.sqrt() / 2.0; // ≈ 1.866
            if *index_min < c_star {
                return Err(SpringError::InconsistentInputs(format!(
                    "index_min={index_min:.4} is below the Wahl monotonicity threshold \
                     C* = 1 + √3/2 ≈ {c_star:.4}; the optimizer requires index_min ≥ C* \
                     for correct feasibility detection"
                )));
            }
            Ok(MinWeightRequest {
                end_type: parse_end_type(end_type)?,
                fixity: parse_fixity(fixity)?,
                required_rate: SpringRate::from_newtons_per_meter(*required_rate_n_per_m),
                max_force: Force::from_newtons(*max_force_n),
                index_bounds: (*index_min, *index_max),
                max_outer_dia: max_outer_dia_mm.map(Length::from_millimeters),
                candidate_diameters: candidate_diameters_mm
                    .iter()
                    .map(|&d| Length::from_millimeters(d))
                    .collect(),
                clash_allowance: *clash_allowance,
            })
        }
        _ => Err(SpringError::InconsistentInputs(
            "min_weight_request_from_spec requires a MinWeight ScenarioSpec".into(),
        )),
    }
}

impl SavedDesign {
    /// Serialize this design to a TOML string.
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|e| SpringError::DataFile(e.to_string()))
    }

    /// Deserialize a design from a TOML string.
    pub fn from_toml(s: &str) -> Result<Self> {
        toml::from_str(s).map_err(|e| SpringError::DataFile(e.to_string()))
    }

    /// Write this design to a TOML file at `path`.
    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, self.to_toml()?).map_err(|e| SpringError::DataFile(e.to_string()))
    }

    /// Load and deserialize a design from the TOML file at `path`.
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).map_err(|e| SpringError::DataFile(e.to_string()))?;
        Self::from_toml(&text)
    }

    /// Re-compute the spring design from the stored scenario inputs and the given material set.
    pub fn solve(&self, materials: &MaterialSet) -> Result<SpringDesign> {
        let material = materials.get(&self.material)?;
        match &self.scenario {
            ScenarioSpec::PowerUser {
                end_type,
                fixity,
                wire_dia_mm,
                mean_dia_mm,
                active,
                free_length_mm,
                loads_n,
            } => PowerUser {
                end_type: parse_end_type(end_type)?,
                fixity: parse_fixity(fixity)?,
                wire_dia: Length::from_millimeters(*wire_dia_mm),
                mean_dia: Length::from_millimeters(*mean_dia_mm),
                active: *active,
                free_length: Length::from_millimeters(*free_length_mm),
                loads: forces(loads_n),
            }
            .solve(material),
            ScenarioSpec::TwoLoad {
                end_type,
                fixity,
                wire_dia_mm,
                mean_dia_mm,
                force1_n,
                length1_mm,
                force2_n,
                length2_mm,
            } => TwoLoad {
                end_type: parse_end_type(end_type)?,
                fixity: parse_fixity(fixity)?,
                wire_dia: Length::from_millimeters(*wire_dia_mm),
                mean_dia: Length::from_millimeters(*mean_dia_mm),
                point1: (
                    Force::from_newtons(*force1_n),
                    Length::from_millimeters(*length1_mm),
                ),
                point2: (
                    Force::from_newtons(*force2_n),
                    Length::from_millimeters(*length2_mm),
                ),
            }
            .solve(material),
            ScenarioSpec::RateBased {
                end_type,
                fixity,
                wire_dia_mm,
                mean_dia_mm,
                rate_n_per_m,
                free_length_mm,
                loads_n,
            } => RateBased {
                end_type: parse_end_type(end_type)?,
                fixity: parse_fixity(fixity)?,
                wire_dia: Length::from_millimeters(*wire_dia_mm),
                mean_dia: Length::from_millimeters(*mean_dia_mm),
                rate: SpringRate::from_newtons_per_meter(*rate_n_per_m),
                free_length: Length::from_millimeters(*free_length_mm),
                loads: forces(loads_n),
            }
            .solve(material),
            ScenarioSpec::Dimensional {
                end_type,
                fixity,
                wire_dia_mm,
                outer_dia_mm,
                active,
                free_length_mm,
                loads_n,
            } => Dimensional {
                end_type: parse_end_type(end_type)?,
                fixity: parse_fixity(fixity)?,
                wire_dia: Length::from_millimeters(*wire_dia_mm),
                outer_dia: Length::from_millimeters(*outer_dia_mm),
                active: *active,
                free_length: Length::from_millimeters(*free_length_mm),
                loads: forces(loads_n),
            }
            .solve(material),
            ScenarioSpec::MinWeight { .. } => {
                let req = min_weight_request_from_spec(&self.scenario)?;
                solve_min_weight(material, &req).map(|s| s.design)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::MaterialSet;
    use approx::assert_relative_eq;

    fn min_weight_spec(rate: f64, index_min: f64) -> ScenarioSpec {
        ScenarioSpec::MinWeight {
            end_type: "squared_ground".into(),
            fixity: "fixed_fixed".into(),
            required_rate_n_per_m: rate,
            max_force_n: 50.0,
            index_min,
            index_max: 12.0,
            max_outer_dia_mm: None,
            candidate_diameters_mm: vec![1.5, 2.0, 2.5, 3.0],
            clash_allowance: 0.15,
        }
    }

    #[test]
    fn min_weight_rate_zero_is_rejected() {
        let spec = min_weight_spec(0.0, 4.0);
        let err = min_weight_request_from_spec(&spec).unwrap_err();
        assert!(matches!(err, SpringError::InconsistentInputs(_)));
    }

    #[test]
    fn min_weight_rate_negative_is_rejected() {
        let spec = min_weight_spec(-100.0, 4.0);
        let err = min_weight_request_from_spec(&spec).unwrap_err();
        assert!(matches!(err, SpringError::InconsistentInputs(_)));
    }

    #[test]
    fn min_weight_index_min_below_c_star_is_rejected() {
        // C* = 1 + √3/2 ≈ 1.866; index_min=1.5 is below the threshold.
        let spec = min_weight_spec(2000.0, 1.5);
        let err = min_weight_request_from_spec(&spec).unwrap_err();
        assert!(matches!(err, SpringError::InconsistentInputs(_)));
    }

    #[test]
    fn min_weight_index_bounds_inverted_is_rejected() {
        // index_min > index_max — the ordering invariant is violated.
        let spec = ScenarioSpec::MinWeight {
            end_type: "squared_ground".into(),
            fixity: "fixed_fixed".into(),
            required_rate_n_per_m: 2000.0,
            max_force_n: 50.0,
            index_min: 12.0,
            index_max: 4.0,
            max_outer_dia_mm: None,
            candidate_diameters_mm: vec![2.0],
            clash_allowance: 0.15,
        };
        let err = min_weight_request_from_spec(&spec).unwrap_err();
        assert!(matches!(err, SpringError::InconsistentInputs(_)));
    }

    #[test]
    fn min_weight_valid_inputs_succeed() {
        let spec = min_weight_spec(2000.0, 4.0);
        assert!(min_weight_request_from_spec(&spec).is_ok());
    }

    #[test]
    fn min_weight_spec_roundtrips_and_solves() {
        let s = SavedDesign {
            material: "Music Wire".into(),
            unit_system: UnitSystem::Metric,
            scenario: ScenarioSpec::MinWeight {
                end_type: "squared_ground".into(),
                fixity: "fixed_fixed".into(),
                required_rate_n_per_m: 2000.0,
                max_force_n: 50.0,
                index_min: 4.0,
                index_max: 12.0,
                max_outer_dia_mm: None,
                candidate_diameters_mm: vec![1.5, 2.0, 2.5, 3.0],
                clash_allowance: 0.15,
            },
        };
        let parsed = SavedDesign::from_toml(&s.to_toml().unwrap()).unwrap();
        assert_eq!(s, parsed);
        let design = s.solve(&MaterialSet::load_default()).unwrap();
        assert!(design.buckling_stable);
    }

    fn sample() -> SavedDesign {
        SavedDesign {
            material: "Music Wire".into(),
            unit_system: UnitSystem::Metric,
            scenario: ScenarioSpec::RateBased {
                end_type: "squared_ground".into(),
                fixity: "fixed_fixed".into(),
                wire_dia_mm: 2.0,
                mean_dia_mm: 20.0,
                rate_n_per_m: 2000.0,
                free_length_mm: 60.0,
                loads_n: vec![10.0],
            },
        }
    }

    #[test]
    fn toml_roundtrip_is_lossless() {
        let original = sample();
        let text = original.to_toml().unwrap();
        let parsed = SavedDesign::from_toml(&text).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn solve_reproduces_design() {
        let set = MaterialSet::load_default();
        let design = sample().solve(&set).unwrap();
        assert_relative_eq!(design.rate.newtons_per_meter(), 2000.0, max_relative = 1e-6);
    }

    #[test]
    fn unknown_end_type_is_rejected() {
        let mut s = sample();
        if let ScenarioSpec::RateBased { end_type, .. } = &mut s.scenario {
            *end_type = "banana".into();
        }
        assert!(s.solve(&MaterialSet::load_default()).is_err());
    }

    #[test]
    fn file_roundtrip() {
        let mut path = std::env::temp_dir();
        path.push("openspringmaker_test_design.toml");
        sample().save(&path).unwrap();
        let loaded = SavedDesign::load(&path).unwrap();
        assert_eq!(sample(), loaded);
        let _ = std::fs::remove_file(&path);
    }
}
