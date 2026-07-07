//! Human-readable persistence of a single design. Stores the user's inputs
//! (not computed outputs); the design is recomputed on load.

use crate::assembly::Topology;
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
        // The only optional field: serde maps a missing key to `None` for `Option` types
        // (no `#[serde(default)]` needed), so unlike every other (required) payload field a
        // missing or misspelled `max_outer_dia_mm` deserializes to `None` rather than erroring.
        max_outer_dia_mm: Option<f64>,
        candidate_diameters_mm: Vec<f64>,
        clash_allowance: f64,
    },
}

// `#[serde(deny_unknown_fields)]` is intentionally NOT applied to the
// family/type/mode-tagged enums here: serde rejects that attribute on
// internally-tagged enums. Forward-compat safety is structural instead — every
// payload field is required (none carry `#[serde(default)]`), so a misspelled
// key surfaces as a "missing field" error on the correctly-spelled field rather
// than silently defaulting. The one exception is `Option` fields
// (`max_outer_dia_mm`): serde implicitly maps a missing key to `None` without
// `#[serde(default)]`, so a missing or misspelled `max_outer_dia_mm` deserializes
// to `None` rather than erroring — see the field-level note at each site. (A
// genuinely unknown extra key is ignored, which is the desired
// additive-forward-compatibility behavior.)
/// A design's family-tagged scenario inputs. `family` is the discriminant tag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "family")]
pub enum DesignSpec {
    Compression(ScenarioSpec),
    Extension(ExtScenarioSpec),
    Torsion(TorsionSpec),
    Conical(ConicalSpec),
    /// Assembly of N cylindrical round-wire compression springs.
    ///
    /// **Decision-2 semantic**: `SavedDesign.material` is the top-level active
    /// picker state (what the GUI had selected when the design was saved); each
    /// `AssemblyMemberSpec.material_name` governs its member's actual solve.
    /// The two fields are intentionally independent — the file-level material is
    /// NOT rewritten to match members, and they may differ.
    Assembly(AssemblySpec),
}

/// Torsion scenario inputs (SI millimetres / newton-millimetres, as stored).
/// One variant per input mode, `type`-tagged — MIGRATED from the original flat
/// single-scenario struct (a conscious clean break: tag-less files written by the
/// single-scenario GUI no longer load; `legacy_tagless_torsion_file_fails_cleanly`
/// pins that they error rather than parse as the wrong shape).
//
// GUARDRAIL: Do NOT add `#[serde(deny_unknown_fields)]` here. The enum is
// flattened under `DesignSpec`'s `#[serde(tag = "family")]` internally-tagged
// enum; serde rejects `deny_unknown_fields` in that position because the injected
// `family` discriminant would be treated as an unknown field, breaking
// deserialization of every torsion TOML file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TorsionSpec {
    PowerUser {
        wire_dia_mm: f64,
        mean_dia_mm: f64,
        body_coils: f64,
        leg1_mm: f64,
        leg2_mm: f64,
        // The only optional field: the `toml` deserializer maps a missing key to
        // `None` for `Option` types (no `#[serde(default)]` needed), so a missing
        // or misspelled `arbor_dia_mm` deserializes to `None` rather than erroring.
        arbor_dia_mm: Option<f64>,
        friction_model: crate::torsion::FrictionModel,
        moments_nmm: Vec<f64>,
    },
    RateBased {
        wire_dia_mm: f64,
        mean_dia_mm: f64,
        /// Required angular rate in N·mm per degree — the family's mm/N·mm storage
        /// flavor and the degree-primary UI unit (exact conversion to the engine's
        /// N·m/rad via `AngularRate::from_newton_meters_per_degree(v / 1000.0)`).
        rate_nmm_per_deg: f64,
        leg1_mm: f64,
        leg2_mm: f64,
        arbor_dia_mm: Option<f64>,
        friction_model: crate::torsion::FrictionModel,
        moments_nmm: Vec<f64>,
    },
    Dimensional {
        wire_dia_mm: f64,
        outer_dia_mm: f64,
        body_coils: f64,
        leg1_mm: f64,
        leg2_mm: f64,
        arbor_dia_mm: Option<f64>,
        friction_model: crate::torsion::FrictionModel,
        moments_nmm: Vec<f64>,
    },
    TwoLoad {
        wire_dia_mm: f64,
        mean_dia_mm: f64,
        leg1_mm: f64,
        leg2_mm: f64,
        arbor_dia_mm: Option<f64>,
        friction_model: crate::torsion::FrictionModel,
        /// Two measured operating points. Angles are degrees and may be NEGATIVE
        /// (the engine's TwoLoad is offset-tolerant) but never non-finite.
        moment1_nmm: f64,
        angle1_deg: f64,
        moment2_nmm: f64,
        angle2_deg: f64,
    },
    MinWeight {
        /// Required angular rate in N·mm per degree (the family's storage flavor).
        rate_nmm_per_deg: f64,
        max_moment_nmm: f64,
        leg1_mm: f64,
        leg2_mm: f64,
        arbor_dia_mm: Option<f64>,
        friction_model: crate::torsion::FrictionModel,
        dia_policy: crate::torsion::DiaPolicy,
        index_min: f64,
        index_max: f64,
        /// Optional outer-diameter cap; missing key → None (documented rule).
        max_outer_dia_mm: Option<f64>,
        candidate_diameters_mm: Vec<f64>,
    },
}

/// Conical compression scenarios (v1: direct geometry only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ConicalSpec {
    PowerUser {
        end_type: String,
        wire_dia_mm: f64,
        large_mean_dia_mm: f64,
        small_mean_dia_mm: f64,
        active: f64,
        free_length_mm: f64,
        loads_n: Vec<f64>,
    },
}

/// One member in a persisted assembly. All fields are required — no
/// `#[serde(default)]` — so a misspelled key surfaces as "missing field"
/// rather than silently defaulting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssemblyMemberSpec {
    pub material_name: String,
    pub end_type: String,
    pub wire_dia_mm: f64,
    pub mean_dia_mm: f64,
    pub active: f64,
    pub free_length_mm: f64,
}

/// Assembly scenario inputs.  `type`-tagged for forward-compat additive growth.
///
/// Declare `loads_n` **before** `members`: TOML `to_string_pretty` emits fields
/// in declaration order and cannot write scalar key-values after an array-of-tables
/// block (`[[design.members]]`) at the same level.  Named-field construction in
/// calling code is unaffected by this ordering.
//
// GUARDRAIL: Do NOT add `#[serde(deny_unknown_fields)]` here. The enum is
// flattened under `DesignSpec`'s `#[serde(tag = "family")]` internally-tagged
// enum; serde rejects `deny_unknown_fields` in that position (same rule as
// TorsionSpec's guardrail comment).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AssemblySpec {
    PowerUser {
        topology: String,
        fixity: String,
        /// Declare before `members` — TOML cannot emit scalars after an
        /// array-of-tables block at the same table level.
        loads_n: Vec<f64>,
        members: Vec<AssemblyMemberSpec>,
    },
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
    RateBased {
        wire_dia_mm: f64,
        mean_dia_mm: f64,
        rate_n_per_m: f64,
        free_length_mm: f64,
        initial_tension_n: f64,
        hooks: HookSpecSpec,
        loads_n: Vec<f64>,
    },
    Dimensional {
        wire_dia_mm: f64,
        outer_dia_mm: f64,
        active: f64,
        free_length_mm: f64,
        initial_tension_n: f64,
        hooks: HookSpecSpec,
        loads_n: Vec<f64>,
    },
    TwoLoad {
        wire_dia_mm: f64,
        mean_dia_mm: f64,
        free_length_mm: f64,
        hooks: HookSpecSpec,
        force1_n: f64,
        length1_mm: f64,
        force2_n: f64,
        length2_mm: f64,
    },
    MinWeight {
        required_rate_n_per_m: f64,
        max_force_n: f64,
        initial_tension_n: f64,
        hooks: HookSpecSpec,
        index_min: f64,
        index_max: f64,
        // The only optional field: serde maps a missing key to `None` for `Option` types
        // (no `#[serde(default)]` needed), so unlike every other (required) payload field a
        // missing or misspelled `max_outer_dia_mm` deserializes to `None` rather than erroring.
        max_outer_dia_mm: Option<f64>,
        candidate_diameters_mm: Vec<f64>,
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

/// Parse a persisted end-type key ("plain" | "plain_ground" | "squared" | "squared_ground").
pub fn parse_end_type(s: &str) -> Result<EndType> {
    Ok(match s {
        "plain" => EndType::Plain,
        "plain_ground" => EndType::PlainGround,
        "squared" => EndType::Squared,
        "squared_ground" => EndType::SquaredGround,
        other => return Err(SpringError::DataFile(format!("unknown end_type: {other}"))),
    })
}

/// Parse a persisted topology key ("nested" | "series").
///
/// The `topology` field of [`AssemblySpec`] stores a raw `String`; this
/// function is called at solve/GUI time (not at deserialize) so that unknown
/// values survive the TOML round-trip and produce a clear error at the point
/// of use.
pub fn parse_topology(s: &str) -> Result<Topology> {
    Ok(match s {
        "nested" => Topology::Nested,
        "series" => Topology::Series,
        other => return Err(SpringError::DataFile(format!("unknown topology: {other}"))),
    })
}

/// Parse a persisted fixity key ("fixed_fixed" | "fixed_pinned" | "pinned_pinned" | "fixed_free").
///
/// Promoted to `pub` so the assembly GUI increment can call it without
/// duplicating the match — same rationale as [`parse_end_type`].
pub fn parse_fixity(s: &str) -> Result<EndFixity> {
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
            DesignSpec::Torsion(_) => Err(SpringError::InconsistentInputs(
                "SavedDesign::solve handles compression designs; torsion designs are solved \
                 via the torsion scenario"
                    .into(),
            )),
            DesignSpec::Conical(_) => Err(SpringError::InconsistentInputs(
                "SavedDesign::solve handles compression designs; conical designs are solved \
                 via the conical scenario"
                    .into(),
            )),
            DesignSpec::Assembly(_) => Err(SpringError::InconsistentInputs(
                "SavedDesign::solve handles compression designs; assembly designs are solved \
                 via the assembly scenario"
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
    fn ext_dimensional_round_trips_through_toml() {
        let saved = SavedDesign {
            material: "Music Wire".into(),
            unit_system: UnitSystem::Metric,
            design: DesignSpec::Extension(ExtScenarioSpec::Dimensional {
                wire_dia_mm: 2.0,
                outer_dia_mm: 22.0,
                active: 10.0,
                free_length_mm: 100.0,
                initial_tension_n: 5.0,
                hooks: HookSpecSpec::Custom {
                    r1_mm: 8.0,
                    r2_mm: 4.0,
                },
                loads_n: vec![10.0, 30.0],
            }),
        };
        let back = SavedDesign::from_toml(&saved.to_toml().unwrap()).unwrap();
        assert_eq!(saved, back);
    }

    #[test]
    fn ext_twoload_round_trips_through_toml() {
        let saved = SavedDesign {
            material: "Music Wire".into(),
            unit_system: UnitSystem::Metric,
            design: DesignSpec::Extension(ExtScenarioSpec::TwoLoad {
                wire_dia_mm: 2.0,
                mean_dia_mm: 20.0,
                free_length_mm: 100.0,
                hooks: HookSpecSpec::Default,
                force1_n: 10.0,
                length1_mm: 110.0,
                force2_n: 30.0,
                length2_mm: 130.0,
            }),
        };
        let back = SavedDesign::from_toml(&saved.to_toml().unwrap()).unwrap();
        assert_eq!(saved, back);
    }

    #[test]
    fn ext_minweight_round_trips_both_max_outer_dia_states() {
        for max_od in [None, Some(30.0)] {
            let saved = SavedDesign {
                material: "Music Wire".into(),
                unit_system: UnitSystem::Metric,
                design: DesignSpec::Extension(ExtScenarioSpec::MinWeight {
                    required_rate_n_per_m: 2000.0,
                    max_force_n: 50.0,
                    initial_tension_n: 5.0,
                    hooks: HookSpecSpec::Default,
                    index_min: 4.0,
                    index_max: 12.0,
                    max_outer_dia_mm: max_od,
                    candidate_diameters_mm: vec![1.5, 2.0, 2.5],
                }),
            };
            let back = SavedDesign::from_toml(&saved.to_toml().unwrap()).unwrap();
            assert_eq!(saved, back);
        }
    }

    #[test]
    fn ext_ratebased_round_trips_through_toml() {
        let saved = SavedDesign {
            material: "Music Wire".into(),
            unit_system: UnitSystem::Metric,
            design: DesignSpec::Extension(ExtScenarioSpec::RateBased {
                wire_dia_mm: 2.0,
                mean_dia_mm: 20.0,
                rate_n_per_m: 2000.0,
                free_length_mm: 100.0,
                initial_tension_n: 5.0,
                hooks: HookSpecSpec::Default,
                loads_n: vec![10.0, 30.0],
            }),
        };
        let toml = saved.to_toml().unwrap();
        let back = SavedDesign::from_toml(&toml).unwrap();
        assert_eq!(saved, back);
    }

    #[test]
    fn from_toml_rejects_non_finite_ratebased_rate() {
        let toml = r#"
material = "Music Wire"
unit_system = "Metric"
[design]
family = "Extension"
type = "RateBased"
wire_dia_mm = 2.0
mean_dia_mm = 20.0
rate_n_per_m = inf
free_length_mm = 100.0
initial_tension_n = 5.0
loads_n = [10.0, 30.0]
[design.hooks]
mode = "Default"
"#;
        assert!(matches!(
            SavedDesign::from_toml(toml),
            Err(SpringError::DataFile(_))
        ));
    }

    #[test]
    fn from_toml_rejects_non_finite_dimensional_float() {
        let toml = r#"
material = "Music Wire"
unit_system = "Metric"
[design]
family = "Extension"
type = "Dimensional"
wire_dia_mm = 2.0
outer_dia_mm = inf
active = 10.0
free_length_mm = 100.0
initial_tension_n = 5.0
loads_n = [10.0, 30.0]
[design.hooks]
mode = "Default"
"#;
        assert!(matches!(
            SavedDesign::from_toml(toml),
            Err(SpringError::DataFile(_))
        ));
    }

    #[test]
    fn from_toml_rejects_non_finite_twoload_float() {
        let toml = r#"
material = "Music Wire"
unit_system = "Metric"
[design]
family = "Extension"
type = "TwoLoad"
wire_dia_mm = 2.0
mean_dia_mm = 20.0
free_length_mm = 100.0
force1_n = nan
length1_mm = 110.0
force2_n = 30.0
length2_mm = 130.0
[design.hooks]
mode = "Default"
"#;
        assert!(matches!(
            SavedDesign::from_toml(toml),
            Err(SpringError::DataFile(_))
        ));
    }

    #[test]
    fn from_toml_rejects_non_finite_minweight_float() {
        let toml = r#"
material = "Music Wire"
unit_system = "Metric"
[design]
family = "Extension"
type = "MinWeight"
required_rate_n_per_m = 2000.0
max_force_n = -inf
initial_tension_n = 5.0
index_min = 4.0
index_max = 12.0
candidate_diameters_mm = [1.5, 2.0]
[design.hooks]
mode = "Default"
"#;
        assert!(matches!(
            SavedDesign::from_toml(toml),
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

    #[test]
    fn solve_with_material_rejects_torsion_design() {
        use crate::torsion::FrictionModel;
        let set = MaterialSet::load_default();
        let m = set.get("Music Wire").unwrap();
        let saved = SavedDesign {
            material: "Music Wire".into(),
            unit_system: UnitSystem::Metric,
            design: DesignSpec::Torsion(TorsionSpec::PowerUser {
                wire_dia_mm: 2.0,
                mean_dia_mm: 20.0,
                body_coils: 5.0,
                leg1_mm: 50.0,
                leg2_mm: 50.0,
                arbor_dia_mm: Some(10.0),
                friction_model: FrictionModel::ShigleyFriction,
                moments_nmm: vec![100.0, 250.0],
            }),
        };
        let err = saved
            .solve_with_material(m, CurvatureCorrection::Bergstrasser)
            .unwrap_err();
        assert!(matches!(err, SpringError::InconsistentInputs(_)));
        // Pin the error content to kill mutation survivors on the Torsion arm.
        let msg = err.to_string();
        assert!(
            msg.contains("torsion"),
            "error must mention 'torsion', got: {msg}"
        );
    }

    // -----------------------------------------------------------------------
    // Task 2: Torsion persistence
    // -----------------------------------------------------------------------

    #[test]
    fn torsion_round_trips_both_arbor_states_and_friction_models() {
        use crate::torsion::FrictionModel;
        for arbor in [None, Some(10.0)] {
            for friction in [FrictionModel::ShigleyFriction, FrictionModel::PureBending] {
                let saved = SavedDesign {
                    material: "Music Wire".into(),
                    unit_system: UnitSystem::Metric,
                    design: DesignSpec::Torsion(TorsionSpec::PowerUser {
                        wire_dia_mm: 2.0,
                        mean_dia_mm: 20.0,
                        body_coils: 5.0,
                        leg1_mm: 50.0,
                        leg2_mm: 50.0,
                        arbor_dia_mm: arbor,
                        friction_model: friction,
                        moments_nmm: vec![100.0, 250.0],
                    }),
                };
                let back = SavedDesign::from_toml(&saved.to_toml().unwrap()).unwrap();
                assert_eq!(saved, back);
            }
        }
    }

    /// Base valid torsion TOML fixture used as the "good" baseline for the
    /// reject-non-finite and reject-unknown-variant tests below.  Asserting
    /// `Ok` on this string first ensures the negative tests can't rot into a
    /// vacuous pass if field names drift.
    const VALID_TORSION_TOML: &str = r#"
material = "Music Wire"
unit_system = "Metric"

[design]
family = "Torsion"
type = "PowerUser"
wire_dia_mm = 2.0
mean_dia_mm = 20.0
body_coils = 5.0
leg1_mm = 0.0
leg2_mm = 0.0
friction_model = "ShigleyFriction"
moments_nmm = [100.0, 250.0]
"#;

    #[test]
    fn from_toml_rejects_non_finite_torsion_moment() {
        // Anchor: the same fixture with finite values must parse Ok so the
        // test can't rot into a vacuous pass if field names drift.
        assert!(
            SavedDesign::from_toml(VALID_TORSION_TOML).is_ok(),
            "base torsion fixture must parse Ok"
        );

        // reject_non_finite must reject an inf inside the moments array.
        let toml = r#"
material = "Music Wire"
unit_system = "Metric"

[design]
family = "Torsion"
type = "PowerUser"
wire_dia_mm = 2.0
mean_dia_mm = 20.0
body_coils = 5.0
leg1_mm = 0.0
leg2_mm = 0.0
friction_model = "ShigleyFriction"
moments_nmm = [100.0, inf]
"#;
        assert!(matches!(
            SavedDesign::from_toml(toml),
            Err(crate::SpringError::DataFile(_))
        ));
    }

    #[test]
    fn from_toml_rejects_non_finite_torsion_arbor() {
        // reject_non_finite must reject inf in the optional scalar arbor_dia_mm.
        // Parity with the array case: reject_non_finite recurses into Table
        // values, so a non-finite Option<f64> scalar is caught at the same
        // boundary.
        let toml = r#"
material = "Music Wire"
unit_system = "Metric"

[design]
family = "Torsion"
type = "PowerUser"
wire_dia_mm = 2.0
mean_dia_mm = 20.0
body_coils = 5.0
leg1_mm = 0.0
leg2_mm = 0.0
arbor_dia_mm = inf
friction_model = "ShigleyFriction"
moments_nmm = [100.0, 250.0]
"#;
        assert!(matches!(
            SavedDesign::from_toml(toml),
            Err(crate::SpringError::DataFile(_))
        ));
    }

    #[test]
    fn from_toml_rejects_unknown_friction_model() {
        // A torsion TOML identical to the valid fixture but with a misspelled
        // friction_model; serde's "unknown variant" error must map to DataFile
        // rather than silently defaulting.
        let toml = r#"
material = "Music Wire"
unit_system = "Metric"

[design]
family = "Torsion"
type = "PowerUser"
wire_dia_mm = 2.0
mean_dia_mm = 20.0
body_coils = 5.0
leg1_mm = 0.0
leg2_mm = 0.0
friction_model = "Bogus"
moments_nmm = [100.0, 250.0]
"#;
        assert!(matches!(
            SavedDesign::from_toml(toml),
            Err(crate::SpringError::DataFile(_))
        ));
    }

    // -----------------------------------------------------------------------
    // Task 1: TorsionSpec tagged-enum — new variant round-trips and guards
    // -----------------------------------------------------------------------

    #[test]
    fn torsion_ratebased_dimensional_twoload_round_trip() {
        use crate::torsion::FrictionModel;
        for design in [
            DesignSpec::Torsion(TorsionSpec::RateBased {
                wire_dia_mm: 2.0,
                mean_dia_mm: 20.0,
                rate_nmm_per_deg: 8.875,
                leg1_mm: 10.0,
                leg2_mm: 0.0,
                arbor_dia_mm: Some(10.0),
                friction_model: FrictionModel::PureBending,
                moments_nmm: vec![1000.0],
            }),
            DesignSpec::Torsion(TorsionSpec::Dimensional {
                wire_dia_mm: 2.0,
                outer_dia_mm: 22.0,
                body_coils: 5.0,
                leg1_mm: 0.0,
                leg2_mm: 0.0,
                arbor_dia_mm: None,
                friction_model: FrictionModel::ShigleyFriction,
                moments_nmm: vec![100.0, 250.0],
            }),
            DesignSpec::Torsion(TorsionSpec::TwoLoad {
                wire_dia_mm: 2.0,
                mean_dia_mm: 20.0,
                leg1_mm: 0.0,
                leg2_mm: 0.0,
                arbor_dia_mm: None,
                friction_model: FrictionModel::ShigleyFriction,
                moment1_nmm: 508.5,
                angle1_deg: -10.0, // negative-but-finite angle is legal (offset-tolerant)
                moment2_nmm: 1017.0,
                angle2_deg: 47.29578,
            }),
        ] {
            let saved = SavedDesign {
                material: "Music Wire".into(),
                unit_system: UnitSystem::Metric,
                design,
            };
            let back = SavedDesign::from_toml(&saved.to_toml().unwrap()).unwrap();
            assert_eq!(saved, back);
        }
    }

    #[test]
    fn legacy_tagless_torsion_file_fails_cleanly() {
        // The exact flat layout the single-scenario GUI wrote (NO `type` key). The
        // clean-break decision: it must ERROR (DataFile, naming the missing tag), never
        // silently parse as some variant.
        let legacy = r#"
material = "Music Wire"
unit_system = "Metric"

[design]
family = "Torsion"
wire_dia_mm = 2.0
mean_dia_mm = 20.0
body_coils = 5.0
leg1_mm = 0.0
leg2_mm = 0.0
friction_model = "ShigleyFriction"
moments_nmm = [1000.0]
"#;
        match SavedDesign::from_toml(legacy) {
            Err(SpringError::DataFile(msg)) => assert!(
                msg.contains("type"),
                "clean-break error should name the missing `type` tag; got: {msg}"
            ),
            other => panic!("legacy tag-less torsion file must fail to load, got {other:?}"),
        }
    }

    #[test]
    fn from_toml_rejects_non_finite_twoload_angle() {
        // Angles may be negative (offset-tolerant) but never non-finite; the generic
        // reject_non_finite tree-walk must cover the new angle fields.
        let toml = r#"
material = "Music Wire"
unit_system = "Metric"

[design]
family = "Torsion"
type = "TwoLoad"
wire_dia_mm = 2.0
mean_dia_mm = 20.0
leg1_mm = 0.0
leg2_mm = 0.0
friction_model = "ShigleyFriction"
moment1_nmm = 508.5
angle1_deg = inf
moment2_nmm = 1017.0
angle2_deg = 114.59156
"#;
        assert!(matches!(
            SavedDesign::from_toml(toml),
            Err(SpringError::DataFile(_))
        ));
    }

    #[test]
    fn torsion_min_weight_round_trips_both_options_and_policies() {
        use crate::torsion::{DiaPolicy, FrictionModel};
        for design in [
            DesignSpec::Torsion(TorsionSpec::MinWeight {
                rate_nmm_per_deg: 8.875,
                max_moment_nmm: 100.0,
                leg1_mm: 10.0,
                leg2_mm: 0.0,
                arbor_dia_mm: Some(10.0),
                friction_model: FrictionModel::PureBending,
                dia_policy: DiaPolicy::MaxMargin,
                index_min: 4.0,
                index_max: 12.0,
                max_outer_dia_mm: Some(30.0),
                candidate_diameters_mm: vec![1.5, 2.0, 2.5],
            }),
            DesignSpec::Torsion(TorsionSpec::MinWeight {
                rate_nmm_per_deg: 8.875,
                max_moment_nmm: 100.0,
                leg1_mm: 0.0,
                leg2_mm: 0.0,
                arbor_dia_mm: None,
                friction_model: FrictionModel::ShigleyFriction,
                dia_policy: DiaPolicy::Compact,
                index_min: 4.0,
                index_max: 12.0,
                max_outer_dia_mm: None,
                candidate_diameters_mm: vec![2.0],
            }),
        ] {
            let saved = SavedDesign {
                material: "Music Wire".into(),
                unit_system: UnitSystem::Metric,
                design,
            };
            let back = SavedDesign::from_toml(&saved.to_toml().unwrap()).unwrap();
            assert_eq!(saved, back);
        }
    }

    #[test]
    fn torsion_min_weight_missing_required_field_errors() {
        // dia_policy omitted → DataFile (only the two *_mm Options may be absent).
        let toml = r#"
material = "Music Wire"
unit_system = "Metric"

[design]
family = "Torsion"
type = "MinWeight"
rate_nmm_per_deg = 8.875
max_moment_nmm = 100.0
leg1_mm = 0.0
leg2_mm = 0.0
friction_model = "PureBending"
index_min = 4.0
index_max = 12.0
candidate_diameters_mm = [2.0]
"#;
        assert!(matches!(
            SavedDesign::from_toml(toml),
            Err(SpringError::DataFile(_))
        ));
    }

    #[test]
    fn torsion_min_weight_rejects_non_finite_candidate_and_bound() {
        // Two complete fixtures: a non-finite Vec ENTRY and a non-finite scalar bound —
        // both must trip reject_non_finite's tree-walk.
        const NON_FINITE_CANDIDATE: &str = r#"
material = "Music Wire"
unit_system = "Metric"

[design]
family = "Torsion"
type = "MinWeight"
rate_nmm_per_deg = 8.875
max_moment_nmm = 100.0
leg1_mm = 0.0
leg2_mm = 0.0
friction_model = "PureBending"
dia_policy = "MaxMargin"
index_min = 4.0
index_max = 12.0
candidate_diameters_mm = [2.0, inf]
"#;
        const NON_FINITE_BOUND: &str = r#"
material = "Music Wire"
unit_system = "Metric"

[design]
family = "Torsion"
type = "MinWeight"
rate_nmm_per_deg = 8.875
max_moment_nmm = 100.0
leg1_mm = 0.0
leg2_mm = 0.0
friction_model = "PureBending"
dia_policy = "MaxMargin"
index_min = inf
index_max = 12.0
candidate_diameters_mm = [2.0]
"#;
        for (name, toml) in [
            ("candidate", NON_FINITE_CANDIDATE),
            ("bound", NON_FINITE_BOUND),
        ] {
            assert!(
                matches!(SavedDesign::from_toml(toml), Err(SpringError::DataFile(_))),
                "non-finite {name} must be rejected"
            );
        }
    }

    #[test]
    fn from_toml_rejects_non_finite_torsion_ratebased_rate() {
        let toml = r#"
material = "Music Wire"
unit_system = "Metric"

[design]
family = "Torsion"
type = "RateBased"
wire_dia_mm = 2.0
mean_dia_mm = 20.0
rate_nmm_per_deg = inf
leg1_mm = 0.0
leg2_mm = 0.0
friction_model = "PureBending"
moments_nmm = [1000.0]
"#;
        assert!(matches!(
            SavedDesign::from_toml(toml),
            Err(SpringError::DataFile(_))
        ));
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

    // -----------------------------------------------------------------------
    // Task 2 (conical): ConicalSpec persistence tests
    // -----------------------------------------------------------------------

    // Base TOML for conical raw-string tests — layout verified against actual
    // `to_toml()` output so the negative tests below can't pass vacuously on
    // a layout mistake.
    const VALID_CONICAL_TOML: &str = r#"material = "Music Wire"
unit_system = "Metric"

[design]
family = "Conical"
type = "PowerUser"
end_type = "squared_ground"
wire_dia_mm = 2.0
large_mean_dia_mm = 20.0
small_mean_dia_mm = 12.0
active = 10.0
free_length_mm = 60.0
loads_n = [10.0]
"#;

    #[test]
    fn conical_round_trips_through_toml() {
        let saved = SavedDesign {
            material: "Music Wire".to_string(),
            unit_system: UnitSystem::Metric,
            design: DesignSpec::Conical(ConicalSpec::PowerUser {
                end_type: "squared_ground".to_string(),
                wire_dia_mm: 2.0,
                large_mean_dia_mm: 20.0,
                small_mean_dia_mm: 12.0,
                active: 10.0,
                free_length_mm: 60.0,
                loads_n: vec![10.0, 25.0],
            }),
        };
        let toml = saved.to_toml().unwrap();
        let back = SavedDesign::from_toml(&toml).unwrap();
        assert_eq!(back, saved);
    }

    #[test]
    fn from_toml_rejects_non_finite_conical_float() {
        // Anchor: the base TOML must parse Ok, proving the layout is correct and
        // the Err below is caused by `inf`, not a layout mistake.
        assert!(
            SavedDesign::from_toml(VALID_CONICAL_TOML).is_ok(),
            "base VALID_CONICAL_TOML must parse Ok"
        );
        // Mutate one float to `inf` — reject_non_finite must catch it.
        let toml =
            VALID_CONICAL_TOML.replace("large_mean_dia_mm = 20.0", "large_mean_dia_mm = inf");
        assert!(matches!(
            SavedDesign::from_toml(&toml),
            Err(SpringError::DataFile(_))
        ));
    }

    #[test]
    fn from_toml_rejects_unknown_conical_type() {
        // Anchor: the base TOML must parse Ok, proving the layout is correct and
        // the Err below is caused by the bad type tag, not a layout mistake.
        assert!(
            SavedDesign::from_toml(VALID_CONICAL_TOML).is_ok(),
            "base VALID_CONICAL_TOML must parse Ok"
        );
        // Mutate the type tag — the serde internally-tagged enum must reject unknown variants.
        let toml = VALID_CONICAL_TOML.replace("type = \"PowerUser\"", "type = \"PowerUsr\"");
        assert!(matches!(
            SavedDesign::from_toml(&toml),
            Err(SpringError::DataFile(_))
        ));
    }

    #[test]
    fn solve_with_material_rejects_conical_design() {
        let m = crate::test_support::music_wire();
        let saved = SavedDesign {
            material: "Music Wire".to_string(),
            unit_system: UnitSystem::Metric,
            design: DesignSpec::Conical(ConicalSpec::PowerUser {
                end_type: "squared_ground".to_string(),
                wire_dia_mm: 2.0,
                large_mean_dia_mm: 20.0,
                small_mean_dia_mm: 12.0,
                active: 10.0,
                free_length_mm: 60.0,
                loads_n: vec![10.0],
            }),
        };
        let err = saved
            .solve_with_material(&m, CurvatureCorrection::Bergstrasser)
            .unwrap_err();
        assert!(matches!(
            err,
            SpringError::InconsistentInputs(ref msg)
                if msg == "SavedDesign::solve handles compression designs; conical designs are \
                           solved via the conical scenario"
        ));
    }

    // -----------------------------------------------------------------------
    // Task 2 (assembly): AssemblySpec persistence tests
    // -----------------------------------------------------------------------

    // Base TOML for assembly raw-string tests — layout verified against actual
    // `to_toml()` output so the negative tests below can't pass vacuously on
    // a layout mistake.  Scalars precede `[[design.members]]` (the array-of-tables
    // block), matching the field declaration order (`loads_n` before `members`).
    const VALID_ASSEMBLY_TOML: &str = r#"material = "Music Wire"
unit_system = "Metric"

[design]
family = "Assembly"
type = "PowerUser"
topology = "nested"
fixity = "fixed_fixed"
loads_n = [10.0, 25.0]

[[design.members]]
material_name = "Music Wire"
end_type = "squared_ground"
wire_dia_mm = 2.0
mean_dia_mm = 20.0
active = 10.0
free_length_mm = 60.0
"#;

    fn member_spec() -> AssemblyMemberSpec {
        AssemblyMemberSpec {
            material_name: "Music Wire".to_string(),
            end_type: "squared_ground".to_string(),
            wire_dia_mm: 2.0,
            mean_dia_mm: 20.0,
            active: 10.0,
            free_length_mm: 60.0,
        }
    }

    #[test]
    fn assembly_round_trips_one_and_three_members() {
        // Anchor: VALID_ASSEMBLY_TOML must parse Ok so the round-trip tests
        // can't pass vacuously on a layout mismatch.
        assert!(
            SavedDesign::from_toml(VALID_ASSEMBLY_TOML).is_ok(),
            "base VALID_ASSEMBLY_TOML must parse Ok"
        );

        // 1-member round-trip.
        let saved1 = SavedDesign {
            material: "Music Wire".to_string(),
            unit_system: UnitSystem::Metric,
            design: DesignSpec::Assembly(AssemblySpec::PowerUser {
                topology: "nested".to_string(),
                fixity: "fixed_fixed".to_string(),
                loads_n: vec![10.0, 25.0],
                members: vec![member_spec()],
            }),
        };
        let toml1 = saved1.to_toml().unwrap();
        let back1 = SavedDesign::from_toml(&toml1).unwrap();
        assert_eq!(back1, saved1);

        // 3-member round-trip.
        let saved3 = SavedDesign {
            material: "Music Wire".to_string(),
            unit_system: UnitSystem::Metric,
            design: DesignSpec::Assembly(AssemblySpec::PowerUser {
                topology: "series".to_string(),
                fixity: "pinned_pinned".to_string(),
                loads_n: vec![50.0],
                members: vec![member_spec(), member_spec(), member_spec()],
            }),
        };
        let toml3 = saved3.to_toml().unwrap();
        let back3 = SavedDesign::from_toml(&toml3).unwrap();
        assert_eq!(back3, saved3);
    }

    #[test]
    fn from_toml_rejects_missing_field_inside_a_member() {
        // Anchor: the base TOML must parse Ok.
        assert!(
            SavedDesign::from_toml(VALID_ASSEMBLY_TOML).is_ok(),
            "base VALID_ASSEMBLY_TOML must parse Ok"
        );
        // Rename a required member key — serde must reject with "missing field".
        let toml = VALID_ASSEMBLY_TOML.replace("wire_dia_mm = 2.0", "wire_diam = 2.0");
        assert!(
            matches!(SavedDesign::from_toml(&toml), Err(SpringError::DataFile(_))),
            "misspelled member field must be rejected"
        );
    }

    #[test]
    fn from_toml_rejects_non_finite_inside_a_member_and_in_loads() {
        // Anchor: the base TOML must parse Ok.
        assert!(
            SavedDesign::from_toml(VALID_ASSEMBLY_TOML).is_ok(),
            "base VALID_ASSEMBLY_TOML must parse Ok"
        );
        // Non-finite inside the member block.
        let toml_member_inf = VALID_ASSEMBLY_TOML.replace("wire_dia_mm = 2.0", "wire_dia_mm = inf");
        assert!(
            matches!(
                SavedDesign::from_toml(&toml_member_inf),
                Err(SpringError::DataFile(_))
            ),
            "inf inside member must be rejected"
        );
        // Non-finite in loads_n.
        let toml_loads_nan =
            VALID_ASSEMBLY_TOML.replace("loads_n = [10.0, 25.0]", "loads_n = [10.0, nan]");
        assert!(
            matches!(
                SavedDesign::from_toml(&toml_loads_nan),
                Err(SpringError::DataFile(_))
            ),
            "nan in loads_n must be rejected"
        );
    }

    #[test]
    fn from_toml_rejects_unknown_topology() {
        // parse_topology is pinned directly — topology is a raw String in the
        // spec struct, so rejection happens at solve/GUI time, not at deserialize.
        assert!(matches!(
            parse_topology("stacked"),
            Err(SpringError::DataFile(ref m)) if m == "unknown topology: stacked"
        ));
        assert!(matches!(
            parse_topology("nested"),
            Ok(crate::assembly::Topology::Nested)
        ));
        assert!(matches!(
            parse_topology("series"),
            Ok(crate::assembly::Topology::Series)
        ));
    }

    #[test]
    fn solve_with_material_rejects_assembly_design() {
        let m = crate::test_support::music_wire();
        let saved = SavedDesign {
            material: "Music Wire".to_string(),
            unit_system: UnitSystem::Metric,
            design: DesignSpec::Assembly(AssemblySpec::PowerUser {
                topology: "nested".to_string(),
                fixity: "fixed_fixed".to_string(),
                loads_n: vec![10.0],
                members: vec![member_spec()],
            }),
        };
        let err = saved
            .solve_with_material(&m, CurvatureCorrection::Bergstrasser)
            .unwrap_err();
        assert!(matches!(
            err,
            SpringError::InconsistentInputs(ref msg)
                if msg == "SavedDesign::solve handles compression designs; assembly designs \
                           are solved via the assembly scenario"
        ));
    }

    #[test]
    fn top_level_material_differs_from_members_and_still_parses() {
        // Decision-2 semantic: SavedDesign.material = top-level active picker
        // state; member material_name governs the solve. The file-level material
        // is NOT rewritten to match members — they are intentionally independent.
        let saved = SavedDesign {
            material: "Chrome-Vanadium".to_string(),
            unit_system: UnitSystem::Metric,
            design: DesignSpec::Assembly(AssemblySpec::PowerUser {
                topology: "nested".to_string(),
                fixity: "fixed_fixed".to_string(),
                loads_n: vec![10.0],
                members: vec![AssemblyMemberSpec {
                    material_name: "Music Wire".to_string(),
                    end_type: "plain".to_string(),
                    wire_dia_mm: 2.0,
                    mean_dia_mm: 20.0,
                    active: 10.0,
                    free_length_mm: 60.0,
                }],
            }),
        };
        let toml = saved.to_toml().unwrap();
        let back = SavedDesign::from_toml(&toml).unwrap();
        assert_eq!(back, saved);
        // Confirm the materials are genuinely different.
        assert_ne!(back.material, "Music Wire");
        if let DesignSpec::Assembly(AssemblySpec::PowerUser { members, .. }) = &back.design {
            assert_eq!(members[0].material_name, "Music Wire");
        } else {
            panic!("expected Assembly variant");
        }
    }
}
