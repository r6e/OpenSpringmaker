//! Human-readable persistence of a single design. Stores the user's inputs
//! (not computed outputs); the design is recomputed on load.

use crate::design::SpringDesign;
use crate::end_type::EndType;
use crate::material::{Material, MaterialSet};
use crate::mechanics::EndFixity;
use crate::optimize::{solve_min_weight, MinWeightRequest};
use crate::scenario::{Dimensional, PowerUser, RateBased, Scenario, TwoLoad};
use crate::units::{Force, Length, SpringRate};
use crate::{CurvatureCorrection, Result, SpringError};
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

// `#[serde(deny_unknown_fields)]` is intentionally NOT applied to the
// family/type/mode-tagged enums below: serde rejects that attribute on
// internally-tagged enums. Forward-compat safety is structural instead — every
// payload field is required (none carry `#[serde(default)]`), so a misspelled
// key surfaces as a "missing field" error on the correctly-spelled field rather
// than silently defaulting. (A genuinely unknown extra key is ignored, which is
// the desired additive-forward-compatibility behavior.)
/// A design's family-tagged scenario inputs. `family` is the discriminant tag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "family")]
pub enum DesignSpec {
    Compression(ScenarioSpec),
    Extension(ExtScenarioSpec),
}

/// Extension scenario inputs (SI millimetres / newtons, as stored). 1c adds the other modes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ExtScenarioSpec {
    PowerUser {
        wire_dia_mm: f64,
        mean_dia_mm: f64,
        active: f64,
        free_length_mm: f64,
        initial_tension_n: f64,
        hooks: HookSpecSpec,
        loads_n: Vec<f64>,
    },
}

/// Persisted hook geometry mode (mirrors engine `HookSpec`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode")]
pub enum HookSpecSpec {
    Default,
    Custom { r1_mm: f64, r2_mm: f64 },
}

/// A persisted design: material, display units, and family-tagged design inputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedDesign {
    pub material: String,
    pub unit_system: UnitSystem,
    pub design: DesignSpec,
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

/// Returns `true` when `v` is both finite and strictly positive.
///
/// Used to validate max_force_n, candidate diameters, and max_outer_dia_mm — all
/// of which must be > 0. **Not** used for clash_allowance, which permits zero (≥ 0).
fn finite_positive(v: f64) -> bool {
    v.is_finite() && v > 0.0
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
            // index_bounds must satisfy 0 < index_min < index_max, and both must be finite
            // (TOML permits inf/nan literals which would cause optimizer divergence).
            if !(*index_min > 0.0
                && *index_min < *index_max
                && index_min.is_finite()
                && index_max.is_finite())
            {
                return Err(SpringError::InconsistentInputs(format!(
                    "index bounds must satisfy 0 < index_min < index_max with both finite; \
                     got index_min={index_min}, index_max={index_max}"
                )));
            }
            // The optimizer's single-endpoint feasibility test in best_mean_dia is only
            // valid when τ(D) is monotonic increasing, which holds only for C ≥ C*
            // where C* = 1 + √3/2 ≈ 1.866 (minimum of the Wahl-corrected stress curve,
            // from d/dC[Kw·C] = 0 ⟹ 4C²−8C+1 = 0). Shared with `solve_min_weight`'s own
            // SI-request guard so the floor is defined once (see [`crate::optimize::min_spring_index`]).
            let c_star = crate::optimize::min_spring_index();
            if *index_min < c_star {
                return Err(SpringError::InconsistentInputs(format!(
                    "index_min={index_min:.4} is below the Wahl monotonicity threshold \
                     C* = 1 + √3/2 ≈ {c_star:.4}; the optimizer requires index_min ≥ C* \
                     for correct feasibility detection"
                )));
            }
            // max_force must be finite and strictly positive; zero or negative
            // force is unphysical for a compression-spring max-load constraint.
            if !finite_positive(*max_force_n) {
                return Err(SpringError::InconsistentInputs(
                    "max_force_n must be a positive finite number (N)".into(),
                ));
            }
            // clash_allowance must be finite and non-negative; a fraction of
            // solid length, so negative values are unphysical.
            if !clash_allowance.is_finite() || *clash_allowance < 0.0 {
                return Err(SpringError::InconsistentInputs(
                    "clash_allowance must be a finite number ≥ 0".into(),
                ));
            }
            // candidate_diameters_mm must have at least one entry, and every
            // entry must be finite and strictly positive.
            if candidate_diameters_mm.is_empty() {
                return Err(SpringError::InconsistentInputs(
                    "candidate_diameters_mm must contain at least one diameter".into(),
                ));
            }
            for &d in candidate_diameters_mm {
                if !finite_positive(d) {
                    return Err(SpringError::InconsistentInputs(format!(
                        "every candidate diameter must be a positive finite number (mm); \
                         got {d}"
                    )));
                }
            }
            // max_outer_dia_mm, when supplied, must be finite and strictly positive.
            if let Some(ood) = max_outer_dia_mm {
                if !finite_positive(*ood) {
                    return Err(SpringError::InconsistentInputs(format!(
                        "max_outer_dia_mm must be a positive finite number (mm); got {ood}"
                    )));
                }
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

/// Reject any non-finite float (`inf`/`nan`) anywhere in a parsed TOML tree.
///
/// TOML 1.1 admits `inf`/`nan` float literals; no design field may hold one, so
/// a single recursive scan guards every current and future float field uniformly
/// — across both families and at any nesting depth.
fn reject_non_finite(value: &toml::Value) -> Result<()> {
    match value {
        toml::Value::Float(f) if !f.is_finite() => Err(SpringError::DataFile(
            "design file contains a non-finite number (inf/nan)".into(),
        )),
        toml::Value::Array(items) => items.iter().try_for_each(reject_non_finite),
        toml::Value::Table(table) => table.values().try_for_each(reject_non_finite),
        _ => Ok(()),
    }
}

impl SavedDesign {
    /// Serialize this design to a TOML string.
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|e| SpringError::DataFile(e.to_string()))
    }

    /// Deserialize a design from a TOML string.
    ///
    /// TOML 1.1 accepts `inf`/`nan` as float literals, so a hand-edited or
    /// machine-generated file could carry a non-finite number into any float
    /// field. Reject those at this boundary (defense in depth) before the value
    /// reaches a `DesignSpec`; the GUI form-parse layer rejects them again on
    /// recompute, but deserialization must never yield a non-finite design input.
    pub fn from_toml(s: &str) -> Result<Self> {
        let value: toml::Value =
            toml::from_str(s).map_err(|e| SpringError::DataFile(e.to_string()))?;
        reject_non_finite(&value)?;
        toml::from_str(s).map_err(|e| SpringError::DataFile(e.to_string()))
    }

    /// Write this design to a TOML file at `path`.
    ///
    /// Writes atomically (temp file + rename) via the shared atomic-write helper,
    /// matching the material-store and settings save paths, so a crash mid-write
    /// cannot corrupt an existing saved design.
    pub fn save(&self, path: &Path) -> Result<()> {
        crate::material_persist::atomic_write(path, &self.to_toml()?)
            .map_err(|e| SpringError::DataFile(e.to_string()))
    }

    /// Load and deserialize a design from the TOML file at `path`.
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).map_err(|e| SpringError::DataFile(e.to_string()))?;
        Self::from_toml(&text)
    }

    /// Re-compute the spring design from the stored scenario inputs and an already-resolved
    /// material reference. Callers that hold a `MaterialStore` (or any other lookup source)
    /// can call `.get(name)?` themselves and pass the result here.
    ///
    /// **Compression-only.** Extension designs are solved via their scenario in the GUI,
    /// not through `SavedDesign`. Passing an extension `DesignSpec` returns
    /// `SpringError::InconsistentInputs`.
    ///
    /// `material` must be the one named by `self.material`; otherwise the computed design
    /// and the design's recorded material name would silently disagree. The mismatch is
    /// rejected at runtime (not merely `debug_assert!`) since this is a public API.
    pub fn solve_with_material(
        &self,
        material: &Material,
        correction: CurvatureCorrection,
    ) -> Result<SpringDesign> {
        if self.material != material.name {
            return Err(SpringError::InconsistentInputs(format!(
                "solve_with_material: material '{}' does not match SavedDesign.material '{}'",
                material.name, self.material
            )));
        }
        match &self.design {
            DesignSpec::Compression(scenario) => match scenario {
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
                .solve(material, correction),
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
                .solve(material, correction),
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
                .solve(material, correction),
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
                .solve(material, correction),
                ScenarioSpec::MinWeight { .. } => {
                    let req = min_weight_request_from_spec(scenario)?;
                    solve_min_weight(material, &req, correction).map(|s| s.design)
                }
            },
            DesignSpec::Extension(_) => Err(SpringError::InconsistentInputs(
                "SavedDesign::solve handles compression designs; extension designs are solved \
                 via the extension scenario"
                    .into(),
            )),
        }
    }

    /// Re-compute the spring design from the stored scenario inputs and the given material set.
    pub fn solve(
        &self,
        materials: &MaterialSet,
        correction: CurvatureCorrection,
    ) -> Result<SpringDesign> {
        let material = materials.get(&self.material)?;
        self.solve_with_material(material, correction)
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
    fn min_weight_index_max_inf_is_rejected() {
        let spec = ScenarioSpec::MinWeight {
            end_type: "squared_ground".into(),
            fixity: "fixed_fixed".into(),
            required_rate_n_per_m: 2000.0,
            max_force_n: 50.0,
            index_min: 4.0,
            index_max: f64::INFINITY,
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

    // Pins `index_min > 0.0` (strict): zero must be rejected by the bounds guard
    // (not the c_star guard). Under a `> 0.0` → `>= 0.0` mutant, index_min=0.0
    // would pass the bounds check and fall through to the c_star check, producing
    // a different error message.  Asserting the exact message pins the guard.
    #[test]
    fn min_weight_index_min_zero_is_rejected() {
        let spec = min_weight_spec(2000.0, 0.0);
        let err = min_weight_request_from_spec(&spec).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("0 < index_min < index_max"),
            "error should name the bounds constraint, got: {msg}"
        );
    }

    // Pins `index_min < index_max` (strict): equal bounds must be rejected.
    // A `<` → `<=` mutant would accept index_min == index_max.
    #[test]
    fn min_weight_index_min_eq_max_is_rejected() {
        let spec = ScenarioSpec::MinWeight {
            end_type: "squared_ground".into(),
            fixity: "fixed_fixed".into(),
            required_rate_n_per_m: 2000.0,
            max_force_n: 50.0,
            index_min: 8.0,
            index_max: 8.0,
            max_outer_dia_mm: None,
            candidate_diameters_mm: vec![2.0],
            clash_allowance: 0.15,
        };
        let err = min_weight_request_from_spec(&spec).unwrap_err();
        assert!(matches!(err, SpringError::InconsistentInputs(_)));
    }

    // Pins both the c_star computation (`/ 2.0` must not become `% 2.0`) and the
    // strict `< c_star` comparison (`<` must not become `<=`).
    //
    // c_star = 1 + √3/2 ≈ 1.8660. A `% 2.0` mutant gives √3 % 2 = √3 ≈ 1.7321,
    // making mutant c_star ≈ 2.7321. An index_min between the two (e.g. 2.0) is
    // accepted by the real code but rejected by the `%` mutant.
    #[test]
    fn min_weight_index_min_above_c_star_is_accepted() {
        // 2.0 > real c_star (≈1.866) so real code accepts; mutant c_star ≈ 2.732
        // would reject this, killing that mutant.
        let spec = min_weight_spec(2000.0, 2.0);
        assert!(
            min_weight_request_from_spec(&spec).is_ok(),
            "index_min=2.0 should be accepted (above C* ≈ 1.866)"
        );
    }

    // c_star exactly: pins `<` vs `<=`. index_min == c_star must be accepted by the
    // real `<` guard (since c_star < c_star is false → no error) but rejected by a
    // `<=` mutant (c_star <= c_star is true → error).
    #[test]
    fn min_weight_index_min_exactly_c_star_is_accepted() {
        let c_star = 1.0 + 3.0_f64.sqrt() / 2.0; // ≈ 1.8660
        let spec = ScenarioSpec::MinWeight {
            end_type: "squared_ground".into(),
            fixity: "fixed_fixed".into(),
            required_rate_n_per_m: 2000.0,
            max_force_n: 50.0,
            index_min: c_star,
            index_max: 12.0,
            max_outer_dia_mm: None,
            candidate_diameters_mm: vec![2.0],
            clash_allowance: 0.15,
        };
        assert!(
            min_weight_request_from_spec(&spec).is_ok(),
            "index_min exactly at C* ≈ {c_star:.4} must be accepted"
        );
    }

    #[test]
    fn min_weight_spec_roundtrips_and_solves() {
        let s = SavedDesign {
            material: "Music Wire".into(),
            unit_system: UnitSystem::Metric,
            design: DesignSpec::Compression(ScenarioSpec::MinWeight {
                end_type: "squared_ground".into(),
                fixity: "fixed_fixed".into(),
                required_rate_n_per_m: 2000.0,
                max_force_n: 50.0,
                index_min: 4.0,
                index_max: 12.0,
                max_outer_dia_mm: None,
                candidate_diameters_mm: vec![1.5, 2.0, 2.5, 3.0],
                clash_allowance: 0.15,
            }),
        };
        let parsed = SavedDesign::from_toml(&s.to_toml().unwrap()).unwrap();
        assert_eq!(s, parsed);
        let design = s
            .solve(
                &MaterialSet::load_default(),
                CurvatureCorrection::Bergstrasser,
            )
            .unwrap();
        assert!(design.buckling_stable);
    }

    fn sample() -> SavedDesign {
        SavedDesign {
            material: "Music Wire".into(),
            unit_system: UnitSystem::Metric,
            design: DesignSpec::Compression(ScenarioSpec::RateBased {
                end_type: "squared_ground".into(),
                fixity: "fixed_fixed".into(),
                wire_dia_mm: 2.0,
                mean_dia_mm: 20.0,
                rate_n_per_m: 2000.0,
                free_length_mm: 60.0,
                loads_n: vec![10.0],
            }),
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
        let design = sample()
            .solve(&set, CurvatureCorrection::Bergstrasser)
            .unwrap();
        assert_relative_eq!(design.rate.newtons_per_meter(), 2000.0, max_relative = 1e-6);
    }

    #[test]
    fn solve_with_material_rejects_mismatched_material() {
        // sample().material == "Music Wire"; passing a different material errors.
        let set = MaterialSet::load_default();
        let wrong = set.get("Stainless 302").unwrap();
        assert!(matches!(
            sample().solve_with_material(wrong, CurvatureCorrection::Bergstrasser),
            Err(SpringError::InconsistentInputs(_))
        ));
        // The matching material still solves.
        let right = set.get("Music Wire").unwrap();
        assert!(sample()
            .solve_with_material(right, CurvatureCorrection::Bergstrasser)
            .is_ok());
    }

    #[test]
    fn unknown_end_type_is_rejected() {
        let mut s = sample();
        if let DesignSpec::Compression(ScenarioSpec::RateBased { end_type, .. }) = &mut s.design {
            *end_type = "banana".into();
        }
        assert!(s
            .solve(
                &MaterialSet::load_default(),
                CurvatureCorrection::Bergstrasser
            )
            .is_err());
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

    // -----------------------------------------------------------------------
    // Task 8: family-tagged DesignSpec round-trips
    // -----------------------------------------------------------------------

    fn ext_power_user_saved() -> SavedDesign {
        SavedDesign {
            material: "Music Wire".into(),
            unit_system: UnitSystem::Metric,
            design: DesignSpec::Extension(ExtScenarioSpec::PowerUser {
                wire_dia_mm: 2.0,
                mean_dia_mm: 20.0,
                active: 10.0,
                free_length_mm: 60.0,
                initial_tension_n: 10.0,
                hooks: HookSpecSpec::Default,
                loads_n: vec![10.0, 30.0],
            }),
        }
    }

    #[test]
    fn extension_power_user_round_trips_through_toml_text() {
        let v = ext_power_user_saved();
        let text = v.to_toml().unwrap();
        let back = SavedDesign::from_toml(&text).unwrap();
        assert_eq!(v, back);
        assert_eq!(text, back.to_toml().unwrap()); // stable serialization, no tag collision
    }

    #[test]
    fn hook_spec_custom_round_trips() {
        let mut v = ext_power_user_saved();
        if let DesignSpec::Extension(ExtScenarioSpec::PowerUser { hooks, .. }) = &mut v.design {
            *hooks = HookSpecSpec::Custom {
                r1_mm: 10.0,
                r2_mm: 5.0,
            };
        }
        let back = SavedDesign::from_toml(&v.to_toml().unwrap()).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn compression_design_still_round_trips_under_design_tag() {
        let v = SavedDesign {
            material: "Music Wire".into(),
            unit_system: UnitSystem::Metric,
            design: DesignSpec::Compression(min_weight_spec(2000.0, 4.0)),
        };
        let back = SavedDesign::from_toml(&v.to_toml().unwrap()).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn pre_1b_scenario_shape_fails_with_data_file_error() {
        let pre_1b = "material = \"Music Wire\"\nunit_system = \"Metric\"\n[scenario]\ntype = \"PowerUser\"\nend_type = \"squared_ground\"\nfixity = \"fixed_fixed\"\nwire_dia_mm = 2.0\nmean_dia_mm = 20.0\nactive = 10.0\nfree_length_mm = 60.0\nloads_n = [10.0, 30.0]\n";
        assert!(matches!(
            SavedDesign::from_toml(pre_1b),
            Err(SpringError::DataFile(_))
        ));
    }

    // -----------------------------------------------------------------------
    // Non-finite rejection at the deserialization boundary (TOML 1.1 inf/nan)
    // -----------------------------------------------------------------------

    #[test]
    fn from_toml_rejects_non_finite_extension_floats() {
        // A structurally valid extension file with Custom hooks...
        let base = "material = \"Music Wire\"\nunit_system = \"Metric\"\n\
                    [design]\nfamily = \"Extension\"\ntype = \"PowerUser\"\n\
                    wire_dia_mm = 2.0\nmean_dia_mm = 20.0\nactive = 10.0\n\
                    free_length_mm = 60.0\ninitial_tension_n = 10.0\nloads_n = [10.0, 30.0]\n\
                    [design.hooks]\nmode = \"Custom\"\nr1_mm = 10.0\nr2_mm = 5.0\n";
        assert!(
            SavedDesign::from_toml(base).is_ok(),
            "base fixture must deserialize cleanly"
        );
        // ...but every non-finite substitution is rejected as DataFile. The three
        // sites exercise a scalar in [design], an element of an array, and a
        // scalar in the [design.hooks] subtable (table + array + nested recursion).
        for (from, to) in [
            ("active = 10.0", "active = inf"),
            ("loads_n = [10.0, 30.0]", "loads_n = [nan, 30.0]"),
            ("r2_mm = 5.0", "r2_mm = -inf"),
        ] {
            let mutated = base.replace(from, to);
            assert_ne!(mutated, base, "substitution '{from}' must apply");
            assert!(
                matches!(
                    SavedDesign::from_toml(&mutated),
                    Err(SpringError::DataFile(_))
                ),
                "non-finite via '{to}' must be rejected"
            );
        }
    }

    #[test]
    fn from_toml_rejects_non_finite_compression_float() {
        let base = "material = \"Music Wire\"\nunit_system = \"Metric\"\n\
                    [design]\nfamily = \"Compression\"\ntype = \"PowerUser\"\n\
                    end_type = \"squared_ground\"\nfixity = \"fixed_fixed\"\n\
                    wire_dia_mm = 2.0\nmean_dia_mm = 20.0\nactive = 10.0\n\
                    free_length_mm = 60.0\nloads_n = [10.0, 30.0]\n";
        assert!(
            SavedDesign::from_toml(base).is_ok(),
            "base fixture must deserialize cleanly"
        );
        let mutated = base.replace("wire_dia_mm = 2.0", "wire_dia_mm = nan");
        assert!(matches!(
            SavedDesign::from_toml(&mutated),
            Err(SpringError::DataFile(_))
        ));
    }

    // -----------------------------------------------------------------------
    // Atomic save (temp file + rename) — disk round-trip and error path
    // -----------------------------------------------------------------------

    #[test]
    fn save_writes_atomically_and_round_trips_through_disk() {
        // Unique per-process+thread path so the round-trip genuinely depends on
        // save() writing (and so concurrent test threads don't collide).
        let path = std::env::temp_dir().join(format!(
            "osm_design_save_{}_{:?}.toml",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&path);
        let original = ext_power_user_saved();
        original.save(&path).unwrap();
        let loaded = SavedDesign::load(&path).unwrap();
        assert_eq!(original, loaded);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_to_path_with_missing_parent_errors() {
        let path = std::env::temp_dir()
            .join("osm_nonexistent_subdir_zzz")
            .join("design.toml");
        assert!(matches!(
            ext_power_user_saved().save(&path),
            Err(SpringError::DataFile(_))
        ));
    }

    #[test]
    fn solve_with_material_rejects_extension_design() {
        let set = MaterialSet::load_default();
        let m = set.get("Music Wire").unwrap();
        let err = ext_power_user_saved()
            .solve_with_material(m, CurvatureCorrection::Bergstrasser)
            .unwrap_err();
        assert!(matches!(err, SpringError::InconsistentInputs(_)));
        // Pin the error content to kill mutation survivors on the Extension arm.
        let msg = err.to_string();
        assert!(
            msg.contains("extension"),
            "error must mention 'extension', got: {msg}"
        );
    }

    // -----------------------------------------------------------------------
    // Fix 4: validation helpers
    // -----------------------------------------------------------------------

    /// Returns a fully-valid MinWeight ScenarioSpec with configurable fields.
    /// All parameters correspond to the ScenarioSpec::MinWeight variant fields.
    fn mw_spec(
        max_force_n: f64,
        clash_allowance: f64,
        candidate_diameters_mm: Vec<f64>,
        max_outer_dia_mm: Option<f64>,
    ) -> ScenarioSpec {
        ScenarioSpec::MinWeight {
            end_type: "squared_ground".into(),
            fixity: "fixed_fixed".into(),
            required_rate_n_per_m: 2000.0,
            max_force_n,
            index_min: 4.0,
            index_max: 12.0,
            max_outer_dia_mm,
            candidate_diameters_mm,
            clash_allowance,
        }
    }

    // -----------------------------------------------------------------------
    // Fix 4: max_force_n validation
    // -----------------------------------------------------------------------

    // Pins `> 0.0` (strict): zero max_force must be rejected.
    #[test]
    fn max_force_zero_is_rejected() {
        let spec = mw_spec(0.0, 0.15, vec![2.0], None);
        let err = min_weight_request_from_spec(&spec).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("max_force_n must be a positive finite number"),
            "expected max_force_n message, got: {msg}"
        );
    }

    // Pins `> 0.0` (strict): value just above zero must be accepted.
    #[test]
    fn max_force_tiny_positive_is_accepted() {
        let spec = mw_spec(f64::MIN_POSITIVE, 0.15, vec![2.0], None);
        assert!(
            min_weight_request_from_spec(&spec).is_ok(),
            "tiny positive max_force_n must be accepted"
        );
    }

    // Pins `is_finite()`: infinity must be rejected.
    #[test]
    fn max_force_inf_is_rejected() {
        let spec = mw_spec(f64::INFINITY, 0.15, vec![2.0], None);
        let err = min_weight_request_from_spec(&spec).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("max_force_n must be a positive finite number"),
            "expected max_force_n message, got: {msg}"
        );
    }

    // Pins `is_finite()`: NaN must be rejected.
    #[test]
    fn max_force_nan_is_rejected() {
        let spec = mw_spec(f64::NAN, 0.15, vec![2.0], None);
        let err = min_weight_request_from_spec(&spec).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("max_force_n must be a positive finite number"),
            "expected max_force_n message, got: {msg}"
        );
    }

    // -----------------------------------------------------------------------
    // Fix 4: clash_allowance validation
    // -----------------------------------------------------------------------

    // Pins `>= 0.0`: zero clash must be accepted (zero = no extra margin).
    #[test]
    fn clash_allowance_zero_is_accepted() {
        let spec = mw_spec(50.0, 0.0, vec![2.0], None);
        assert!(
            min_weight_request_from_spec(&spec).is_ok(),
            "clash_allowance=0.0 must be accepted"
        );
    }

    // Pins `>= 0.0` (strict lower): negative clash must be rejected.
    #[test]
    fn clash_allowance_negative_is_rejected() {
        let spec = mw_spec(50.0, -f64::EPSILON, vec![2.0], None);
        let err = min_weight_request_from_spec(&spec).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("clash_allowance must be a finite number"),
            "expected clash_allowance message, got: {msg}"
        );
    }

    // Pins `is_finite()`: infinity clash must be rejected.
    #[test]
    fn clash_allowance_inf_is_rejected() {
        let spec = mw_spec(50.0, f64::INFINITY, vec![2.0], None);
        let err = min_weight_request_from_spec(&spec).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("clash_allowance must be a finite number"),
            "expected clash_allowance message, got: {msg}"
        );
    }

    // -----------------------------------------------------------------------
    // Fix 4: candidate_diameters_mm validation
    // -----------------------------------------------------------------------

    // Pins `is_empty()` guard: empty list must be rejected.
    #[test]
    fn candidate_diameters_empty_is_rejected() {
        let spec = mw_spec(50.0, 0.15, vec![], None);
        let err = min_weight_request_from_spec(&spec).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("candidate_diameters_mm must contain at least one diameter"),
            "expected empty-candidates message, got: {msg}"
        );
    }

    // Pins per-entry `> 0.0`: a zero diameter in the list must be rejected.
    #[test]
    fn candidate_diameters_zero_entry_is_rejected() {
        let spec = mw_spec(50.0, 0.15, vec![2.0, 0.0], None);
        let err = min_weight_request_from_spec(&spec).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("every candidate diameter must be a positive finite number"),
            "expected per-entry positive message, got: {msg}"
        );
    }

    // Pins per-entry `> 0.0`: a negative diameter must be rejected.
    #[test]
    fn candidate_diameters_negative_entry_is_rejected() {
        let spec = mw_spec(50.0, 0.15, vec![2.0, -1.0], None);
        let err = min_weight_request_from_spec(&spec).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("every candidate diameter must be a positive finite number"),
            "expected per-entry negative message, got: {msg}"
        );
    }

    // Pins per-entry `is_finite()`: infinity in the list must be rejected.
    #[test]
    fn candidate_diameters_inf_entry_is_rejected() {
        let spec = mw_spec(50.0, 0.15, vec![2.0, f64::INFINITY], None);
        let err = min_weight_request_from_spec(&spec).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("every candidate diameter must be a positive finite number"),
            "expected per-entry finite message, got: {msg}"
        );
    }

    // A single-element list with a valid diameter must be accepted.
    #[test]
    fn candidate_diameters_single_valid_is_accepted() {
        let spec = mw_spec(50.0, 0.15, vec![2.0], None);
        assert!(
            min_weight_request_from_spec(&spec).is_ok(),
            "single valid candidate diameter must be accepted"
        );
    }

    // -----------------------------------------------------------------------
    // Fix 4: max_outer_dia_mm validation
    // -----------------------------------------------------------------------

    // Some(positive) is accepted.
    #[test]
    fn max_outer_dia_some_positive_is_accepted() {
        let spec = mw_spec(50.0, 0.15, vec![2.0], Some(25.0));
        assert!(
            min_weight_request_from_spec(&spec).is_ok(),
            "Some(25.0) max_outer_dia_mm must be accepted"
        );
    }

    // Pins `> 0.0`: Some(0.0) must be rejected.
    #[test]
    fn max_outer_dia_zero_is_rejected() {
        let spec = mw_spec(50.0, 0.15, vec![2.0], Some(0.0));
        let err = min_weight_request_from_spec(&spec).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("max_outer_dia_mm must be a positive finite number"),
            "expected max_outer_dia_mm message, got: {msg}"
        );
    }

    // Pins `> 0.0`: Some(negative) must be rejected.
    #[test]
    fn max_outer_dia_negative_is_rejected() {
        let spec = mw_spec(50.0, 0.15, vec![2.0], Some(-1.0));
        let err = min_weight_request_from_spec(&spec).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("max_outer_dia_mm must be a positive finite number"),
            "expected max_outer_dia_mm message, got: {msg}"
        );
    }

    // Pins `is_finite()`: Some(inf) must be rejected.
    #[test]
    fn max_outer_dia_inf_is_rejected() {
        let spec = mw_spec(50.0, 0.15, vec![2.0], Some(f64::INFINITY));
        let err = min_weight_request_from_spec(&spec).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("max_outer_dia_mm must be a positive finite number"),
            "expected max_outer_dia_mm message, got: {msg}"
        );
    }
}
