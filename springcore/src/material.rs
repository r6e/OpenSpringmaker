//! Spring-wire materials. Strength is defined by a diameter-dependent equation
//! whose coefficients live in their native unit system (Shigley Table 10-4);
//! only the scalar result is converted to SI (see ADR 0003).

use crate::error::{Result, SpringError};
use crate::units::{Length, MassDensity, Stress, Temperature};
use serde::{Deserialize, Serialize};

const PSI_PER_KPSI: f64 = 1000.0;

/// Functional form of the minimum-tensile-strength equation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtsForm {
    /// Sut = c0 (constant).
    Constant,
    /// Sut = A / d^m, coefficients = [A, m] (Shigley Eq. 10-14).
    PowerLaw,
    /// Sut = sum_i c_i d^i, coefficients = [c0, c1, ...].
    Polynomial,
    /// Sut = (P0*d^P4 + P1) / (P2*d^P4 + P3), coefficients = [P0, P1, P2, P3, P4].
    /// A 5-parameter rational curve fit. The denominator is guarded against zero.
    Rational,
}

/// Native unit system of an MTS equation's coefficients.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrengthUnits {
    /// A in kpsi·inᵐ, diameter in inches, result in kpsi.
    UsKpsiInch,
    /// A in MPa·mmᵐ, diameter in mm, result in MPa.
    SiMpaMm,
}

impl std::fmt::Display for MtsForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            MtsForm::Constant => "Constant",
            MtsForm::PowerLaw => "Power law",
            MtsForm::Polynomial => "Polynomial",
            MtsForm::Rational => "Rational",
        })
    }
}

impl std::fmt::Display for StrengthUnits {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            StrengthUnits::UsKpsiInch => "US (kpsi, in)",
            StrengthUnits::SiMpaMm => "SI (MPa, mm)",
        })
    }
}

impl StrengthUnits {
    /// Express a diameter in this system's native length unit.
    pub fn length_native(self, d: Length) -> f64 {
        match self {
            Self::UsKpsiInch => d.inches(),
            Self::SiMpaMm => d.millimeters(),
        }
    }

    /// Convert a native strength scalar to SI stress.
    pub fn stress_from_native(self, value: f64) -> Stress {
        match self {
            Self::UsKpsiInch => Stress::from_psi(value * PSI_PER_KPSI),
            Self::SiMpaMm => Stress::from_megapascals(value),
        }
    }
}

/// Diameter-dependent minimum tensile strength.
#[derive(Debug, Clone)]
pub struct MtsEquation {
    pub form: MtsForm,
    pub units: StrengthUnits,
    pub coefficients: Vec<f64>,
    pub valid_dia_min: Length,
    pub valid_dia_max: Length,
}

impl MtsEquation {
    /// Minimum tensile strength at diameter `d`, in SI.
    pub fn evaluate(&self, d: Length) -> Result<Stress> {
        if d.meters() < self.valid_dia_min.meters() || d.meters() > self.valid_dia_max.meters() {
            return Err(SpringError::DiameterOutOfRange {
                diameter_m: d.meters(),
                min_m: self.valid_dia_min.meters(),
                max_m: self.valid_dia_max.meters(),
            });
        }
        let dn = self.units.length_native(d);
        let c = &self.coefficients;
        let raw = match self.form {
            MtsForm::Constant => c[0],
            MtsForm::PowerLaw => c[0] / dn.powf(c[1]),
            MtsForm::Polynomial => c
                .iter()
                .enumerate()
                .map(|(i, ci)| ci * dn.powi(i as i32))
                .sum::<f64>(),
            MtsForm::Rational => {
                let p = dn.powf(c[4]);
                let denominator = c[2] * p + c[3];
                let numerator = c[0] * p + c[1];
                let value = numerator / denominator;
                if !value.is_finite() {
                    return Err(SpringError::InconsistentInputs(format!(
                        "rational MTS form produced a non-finite result at diameter {} m",
                        d.meters()
                    )));
                }
                value
            }
        };
        Ok(self.units.stress_from_native(raw))
    }
}

/// Cited endurance data (Zimmerli; steel spring wire only).
#[derive(Debug, Clone, Copy)]
pub struct Endurance {
    /// Alternating shear endurance strength.
    pub ssa: Stress,
    /// Mean shear endurance strength.
    pub ssm: Stress,
    /// Whether the data is for shot-peened springs.
    pub peened: bool,
}

/// A mutable editing DTO for constructing or modifying a [`Material`].
///
/// Mirrors the fields of [`RawMaterial`] but exposes typed enums for
/// `mts_form` and `mts_units` instead of raw strings. Call [`MaterialDraft::build`]
/// to validate and convert to a [`Material`].
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialDraft {
    pub name: String,
    pub specification: String,
    pub citations: String,
    pub mts_form: MtsForm,
    pub mts_units: StrengthUnits,
    pub mts_coefficients: Vec<f64>,
    pub valid_dia_min_mm: f64,
    pub valid_dia_max_mm: f64,
    pub youngs_modulus_gpa: f64,
    pub shear_modulus_gpa: f64,
    pub density_kg_per_m3: f64,
    pub allowable_pct_torsion: f64,
    pub allowable_pct_bending: f64,
    pub allowable_pct_set: f64,
    pub endurance: Option<EnduranceDraft>,
    pub max_service_temp_c: Option<f64>,
}

/// Editable endurance data within a [`MaterialDraft`].
#[derive(Debug, Clone, PartialEq)]
pub struct EnduranceDraft {
    pub ssa_mpa: f64,
    pub ssm_mpa: f64,
    pub peened: bool,
}

impl MaterialDraft {
    /// Validate this draft and build a [`Material`].
    ///
    /// Returns `SpringError::DataFile` on invalid data (wrong coefficient
    /// count, out-of-range allowable fractions, etc.).
    pub fn build(&self) -> crate::error::Result<Material> {
        let raw = RawMaterial {
            name: self.name.clone(),
            specification: self.specification.clone(),
            citations: self.citations.clone(),
            mts_form: mts_form_str(self.mts_form).to_string(),
            mts_units: strength_units_str(self.mts_units).to_string(),
            mts_coefficients: self.mts_coefficients.clone(),
            valid_dia_min_mm: self.valid_dia_min_mm,
            valid_dia_max_mm: self.valid_dia_max_mm,
            youngs_modulus_gpa: self.youngs_modulus_gpa,
            shear_modulus_gpa: self.shear_modulus_gpa,
            density_kg_per_m3: self.density_kg_per_m3,
            allowable_pct_torsion: self.allowable_pct_torsion,
            allowable_pct_bending: self.allowable_pct_bending,
            allowable_pct_set: self.allowable_pct_set,
            endurance: self.endurance.as_ref().map(|e| RawEndurance {
                ssa_mpa: e.ssa_mpa,
                ssm_mpa: e.ssm_mpa,
                peened: e.peened,
            }),
            max_service_temp_c: self.max_service_temp_c,
        };
        Material::try_from_raw(raw)
    }
}

/// A spring-wire material.
#[derive(Debug, Clone)]
pub struct Material {
    pub name: String,
    pub specification: String,
    pub(crate) mts: MtsEquation,
    pub youngs_modulus: Stress,
    pub shear_modulus: Stress,
    pub density: MassDensity,
    /// Allowable torsional stress as a fraction of MTS; applies to torsionally
    /// loaded spring types (e.g. helical compression/extension).
    pub allowable_pct_torsion: f64,
    /// Allowable bending stress as a fraction of MTS; applies to bending-loaded
    /// spring types (e.g. torsion, flat). Retained here for future sub-projects.
    pub allowable_pct_bending: f64,
    /// Allowable stress before permanent set, as a fraction of MTS.
    pub allowable_pct_set: f64,
    pub endurance: Option<Endurance>,
    pub citations: String,
    /// Maximum service temperature, if specified. Informational only — NOT used
    /// in any calculation (no derating). Displayed with its citation.
    pub max_service_temperature: Option<Temperature>,
}

impl Material {
    /// Minimum tensile strength at wire diameter `d`, in SI (pascals).
    pub fn min_tensile_strength(&self, d: Length) -> Result<Stress> {
        self.mts.evaluate(d)
    }

    /// Convert this material to an editable [`MaterialDraft`].
    ///
    /// The draft can be modified and then rebuilt via [`MaterialDraft::build`].
    /// Panics if the material's internal form/units strings are somehow
    /// inconsistent (impossible for materials that have been validated).
    pub fn to_draft(&self) -> MaterialDraft {
        let raw = self.to_raw();
        let mts_form =
            mts_form_from_str(&raw.mts_form).expect("valid material has parseable mts_form");
        let mts_units = strength_units_from_str(&raw.mts_units)
            .expect("valid material has parseable mts_units");
        MaterialDraft {
            name: raw.name,
            specification: raw.specification,
            citations: raw.citations,
            mts_form,
            mts_units,
            mts_coefficients: raw.mts_coefficients,
            valid_dia_min_mm: raw.valid_dia_min_mm,
            valid_dia_max_mm: raw.valid_dia_max_mm,
            youngs_modulus_gpa: raw.youngs_modulus_gpa,
            shear_modulus_gpa: raw.shear_modulus_gpa,
            density_kg_per_m3: raw.density_kg_per_m3,
            allowable_pct_torsion: raw.allowable_pct_torsion,
            allowable_pct_bending: raw.allowable_pct_bending,
            allowable_pct_set: raw.allowable_pct_set,
            endurance: raw.endurance.map(|e| EnduranceDraft {
                ssa_mpa: e.ssa_mpa,
                ssm_mpa: e.ssm_mpa,
                peened: e.peened,
            }),
            max_service_temp_c: raw.max_service_temp_c,
        }
    }
}

/// An immutable, named collection of materials.
#[derive(Debug, Clone)]
pub struct MaterialSet {
    materials: Vec<Material>,
}

impl MaterialSet {
    /// Parse a TOML document containing `[[material]]` entries.
    ///
    /// Returns `SpringError::DataFile` on malformed input (unknown form/units,
    /// wrong coefficient count, inverted diameter range, or TOML syntax errors).
    pub fn from_toml_str(s: &str) -> Result<Self> {
        let raw: RawDoc = toml::from_str(s).map_err(|e| SpringError::DataFile(e.to_string()))?;
        let materials = raw
            .material
            .into_iter()
            .map(Material::try_from_raw)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { materials })
    }

    /// Load the curated material set bundled with the crate.
    pub fn load_default() -> Self {
        Self::from_toml_str(include_str!("../data/materials.toml"))
            .expect("bundled materials.toml is valid")
    }

    /// Look up a material by name; returns an error if not found.
    pub fn get(&self, name: &str) -> Result<&Material> {
        self.materials
            .iter()
            .find(|m| m.name == name)
            .ok_or_else(|| SpringError::MaterialNotFound(name.to_string()))
    }

    /// Return the names of all materials in insertion order.
    pub fn names(&self) -> Vec<&str> {
        self.materials.iter().map(|m| m.name.as_str()).collect()
    }
}

// --- TOML deserialization layer (native, human-readable units) ---

#[derive(Deserialize, Serialize)]
pub(crate) struct RawDoc {
    pub(crate) material: Vec<RawMaterial>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct RawMaterial {
    pub(crate) name: String,
    pub(crate) specification: String,
    pub(crate) citations: String,
    pub(crate) mts_form: String,
    pub(crate) mts_units: String,
    pub(crate) mts_coefficients: Vec<f64>,
    pub(crate) valid_dia_min_mm: f64,
    pub(crate) valid_dia_max_mm: f64,
    pub(crate) youngs_modulus_gpa: f64,
    pub(crate) shear_modulus_gpa: f64,
    pub(crate) density_kg_per_m3: f64,
    pub(crate) allowable_pct_torsion: f64,
    pub(crate) allowable_pct_bending: f64,
    pub(crate) allowable_pct_set: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) endurance: Option<RawEndurance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) max_service_temp_c: Option<f64>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct RawEndurance {
    pub(crate) ssa_mpa: f64,
    pub(crate) ssm_mpa: f64,
    pub(crate) peened: bool,
}

/// Stable string key for an MTS form (used in TOML).
pub(crate) fn mts_form_str(form: MtsForm) -> &'static str {
    match form {
        MtsForm::Constant => "constant",
        MtsForm::PowerLaw => "power_law",
        MtsForm::Polynomial => "polynomial",
        MtsForm::Rational => "rational",
    }
}

fn mts_form_from_str(s: &str) -> Result<MtsForm> {
    Ok(match s {
        "constant" => MtsForm::Constant,
        "power_law" => MtsForm::PowerLaw,
        "polynomial" => MtsForm::Polynomial,
        "rational" => MtsForm::Rational,
        other => return Err(SpringError::DataFile(format!("unknown mts_form: {other}"))),
    })
}

/// Stable string key for a strength unit system (used in TOML).
pub(crate) fn strength_units_str(units: StrengthUnits) -> &'static str {
    match units {
        StrengthUnits::UsKpsiInch => "us_kpsi_inch",
        StrengthUnits::SiMpaMm => "si_mpa_mm",
    }
}

fn strength_units_from_str(s: &str) -> Result<StrengthUnits> {
    Ok(match s {
        "us_kpsi_inch" => StrengthUnits::UsKpsiInch,
        "si_mpa_mm" => StrengthUnits::SiMpaMm,
        other => return Err(SpringError::DataFile(format!("unknown mts_units: {other}"))),
    })
}

/// Number of coefficients each form requires. Polynomial requires >= 1
/// (checked separately as a minimum).
fn coefficients_ok(form: MtsForm, n: usize) -> bool {
    match form {
        MtsForm::Constant => n == 1,
        MtsForm::PowerLaw => n == 2,
        MtsForm::Rational => n == 5,
        MtsForm::Polynomial => n >= 1,
    }
}

impl Material {
    pub(crate) fn try_from_raw(r: RawMaterial) -> Result<Self> {
        let form = mts_form_from_str(&r.mts_form)?;
        let units = strength_units_from_str(&r.mts_units)?;
        if !coefficients_ok(form, r.mts_coefficients.len()) {
            return Err(SpringError::DataFile(format!(
                "material '{}': {} coefficients for form {}",
                r.name,
                r.mts_coefficients.len(),
                r.mts_form
            )));
        }
        if r.valid_dia_min_mm > r.valid_dia_max_mm {
            return Err(SpringError::DataFile(format!(
                "material '{}': valid_dia_min_mm > valid_dia_max_mm",
                r.name
            )));
        }
        // Untrusted overlay input: reject non-finite numbers (the toml crate
        // accepts nan/inf literals) so they can never poison a downstream f64
        // calculation. Finiteness is checked before positivity because
        // `NaN <= 0.0` is false and would otherwise slip past the `<= 0.0` guards.
        let finite_fields = [
            r.youngs_modulus_gpa,
            r.shear_modulus_gpa,
            r.density_kg_per_m3,
            r.valid_dia_min_mm,
            r.valid_dia_max_mm,
            r.allowable_pct_torsion,
            r.allowable_pct_bending,
            r.allowable_pct_set,
        ];
        let coeff_finite = r.mts_coefficients.iter().copied();
        let optional_finite = r
            .max_service_temp_c
            .into_iter()
            .chain(r.endurance.iter().flat_map(|e| [e.ssa_mpa, e.ssm_mpa]));
        if finite_fields
            .into_iter()
            .chain(coeff_finite)
            .chain(optional_finite)
            .any(|x| !x.is_finite())
        {
            return Err(SpringError::DataFile(format!(
                "material '{}': non-finite numeric field",
                r.name
            )));
        }
        if r.youngs_modulus_gpa <= 0.0 {
            return Err(SpringError::DataFile(format!(
                "material '{}': youngs_modulus_gpa must be > 0",
                r.name
            )));
        }
        if r.shear_modulus_gpa <= 0.0 {
            return Err(SpringError::DataFile(format!(
                "material '{}': shear_modulus_gpa must be > 0",
                r.name
            )));
        }
        if r.density_kg_per_m3 <= 0.0 {
            return Err(SpringError::DataFile(format!(
                "material '{}': density_kg_per_m3 must be > 0",
                r.name
            )));
        }
        if r.valid_dia_min_mm <= 0.0 {
            return Err(SpringError::DataFile(format!(
                "material '{}': valid_dia_min_mm must be > 0",
                r.name
            )));
        }
        // Allowable stresses are fractions of MTS used as safety thresholds:
        // design.rs warns when operating stress exceeds them, and optimize.rs caps
        // feasible stress at allowable_pct_torsion * MTS. An untrusted overlay value
        // outside (0, 1] would silently suppress overstress warnings or let the
        // optimizer accept designs stressed beyond ultimate, so reject it. (Finiteness
        // is already established above, so these comparisons cannot see NaN.)
        for (label, pct) in [
            ("allowable_pct_torsion", r.allowable_pct_torsion),
            ("allowable_pct_bending", r.allowable_pct_bending),
            ("allowable_pct_set", r.allowable_pct_set),
        ] {
            if pct <= 0.0 || pct > 1.0 {
                return Err(SpringError::DataFile(format!(
                    "material '{}': {label} must be in (0, 1]",
                    r.name
                )));
            }
        }
        Ok(Material {
            name: r.name,
            specification: r.specification,
            mts: MtsEquation {
                form,
                units,
                coefficients: r.mts_coefficients,
                valid_dia_min: Length::from_millimeters(r.valid_dia_min_mm),
                valid_dia_max: Length::from_millimeters(r.valid_dia_max_mm),
            },
            youngs_modulus: Stress::from_pascals(r.youngs_modulus_gpa * 1.0e9),
            shear_modulus: Stress::from_pascals(r.shear_modulus_gpa * 1.0e9),
            density: MassDensity::from_kg_per_m3(r.density_kg_per_m3),
            allowable_pct_torsion: r.allowable_pct_torsion,
            allowable_pct_bending: r.allowable_pct_bending,
            allowable_pct_set: r.allowable_pct_set,
            endurance: r.endurance.map(|e| Endurance {
                ssa: Stress::from_megapascals(e.ssa_mpa),
                ssm: Stress::from_megapascals(e.ssm_mpa),
                peened: e.peened,
            }),
            max_service_temperature: r.max_service_temp_c.map(Temperature::from_celsius),
            citations: r.citations,
        })
    }

    /// Convert back to the serializable raw form (for the user overlay file).
    pub(crate) fn to_raw(&self) -> RawMaterial {
        RawMaterial {
            name: self.name.clone(),
            specification: self.specification.clone(),
            citations: self.citations.clone(),
            mts_form: mts_form_str(self.mts.form).to_string(),
            mts_units: strength_units_str(self.mts.units).to_string(),
            mts_coefficients: self.mts.coefficients.clone(),
            valid_dia_min_mm: self.mts.valid_dia_min.millimeters(),
            valid_dia_max_mm: self.mts.valid_dia_max.millimeters(),
            youngs_modulus_gpa: self.youngs_modulus.pascals() / 1.0e9,
            shear_modulus_gpa: self.shear_modulus.pascals() / 1.0e9,
            density_kg_per_m3: self.density.kg_per_m3(),
            allowable_pct_torsion: self.allowable_pct_torsion,
            allowable_pct_bending: self.allowable_pct_bending,
            allowable_pct_set: self.allowable_pct_set,
            endurance: self.endurance.map(|e| RawEndurance {
                ssa_mpa: e.ssa.megapascals(),
                ssm_mpa: e.ssm.megapascals(),
                peened: e.peened,
            }),
            max_service_temp_c: self.max_service_temperature.map(|t| t.celsius()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SpringError;
    use crate::units::Length;
    use approx::assert_relative_eq;

    const SAMPLE: &str = r#"
[[material]]
name = "Test Music Wire"
specification = "ASTM A228"
citations = "Shigley Table 10-4 (A, m); Table 10-5 (E, G)"
mts_form = "power_law"
mts_units = "si_mpa_mm"
mts_coefficients = [2211.0, 0.145]
valid_dia_min_mm = 0.10
valid_dia_max_mm = 6.5
youngs_modulus_gpa = 203.4
shear_modulus_gpa = 80.0
density_kg_per_m3 = 7850.0
allowable_pct_torsion = 0.45
allowable_pct_bending = 0.75
allowable_pct_set = 0.60
[material.endurance]
ssa_mpa = 241.0
ssm_mpa = 379.0
peened = false
"#;

    #[test]
    fn power_law_mts_si_native() {
        let set = MaterialSet::from_toml_str(SAMPLE).unwrap();
        let m = set.get("Test Music Wire").unwrap();
        // Sut = 2211 / d^0.145, d in mm. At d=1mm -> 2211 MPa.
        assert_relative_eq!(
            m.min_tensile_strength(Length::from_millimeters(1.0))
                .unwrap()
                .megapascals(),
            2211.0,
            max_relative = 1e-9
        );
        // At d=2mm -> 2211 / 2^0.145
        let expected = 2211.0 / 2.0_f64.powf(0.145);
        assert_relative_eq!(
            m.min_tensile_strength(Length::from_millimeters(2.0))
                .unwrap()
                .megapascals(),
            expected,
            max_relative = 1e-9
        );
    }

    #[test]
    fn us_native_units_not_converted_as_coefficients() {
        let us = r#"
[[material]]
name = "US Music Wire"
specification = "ASTM A228"
citations = "Shigley Table 10-4"
mts_form = "power_law"
mts_units = "us_kpsi_inch"
mts_coefficients = [201.0, 0.145]
valid_dia_min_mm = 2.54
valid_dia_max_mm = 25.4
youngs_modulus_gpa = 203.4
shear_modulus_gpa = 80.0
density_kg_per_m3 = 7850.0
allowable_pct_torsion = 0.45
allowable_pct_bending = 0.75
allowable_pct_set = 0.60
"#;
        let set = MaterialSet::from_toml_str(us).unwrap();
        let m = set.get("US Music Wire").unwrap();
        // d = 0.2 in. Sut = 201 / 0.2^0.145 kpsi, evaluated in inches.
        let d = Length::from_inches(0.2);
        let expected_kpsi = 201.0 / 0.2_f64.powf(0.145);
        let got = m.min_tensile_strength(d).unwrap();
        assert_relative_eq!(got.psi() / 1000.0, expected_kpsi, max_relative = 1e-9);
    }

    #[test]
    fn out_of_range_diameter_rejected() {
        let set = MaterialSet::from_toml_str(SAMPLE).unwrap();
        let m = set.get("Test Music Wire").unwrap();
        let err = m
            .min_tensile_strength(Length::from_millimeters(10.0))
            .unwrap_err();
        assert!(matches!(err, SpringError::DiameterOutOfRange { .. }));
    }

    // Pins the strict `<`/`>` boundary checks. The min and max boundary diameters
    // themselves must be accepted (i.e., `d < min` and `d > max` rejects, not
    // `d <= min` or `d >= max`). A `<`→`<=` mutant would reject d=min_dia; a
    // `>`→`>=` mutant would reject d=max_dia.
    #[test]
    fn boundary_diameters_are_accepted() {
        let set = MaterialSet::from_toml_str(SAMPLE).unwrap();
        let m = set.get("Test Music Wire").unwrap();
        // valid_dia_min_mm = 0.10 — must be Ok
        assert!(
            m.min_tensile_strength(Length::from_millimeters(0.10))
                .is_ok(),
            "min boundary diameter must be accepted"
        );
        // valid_dia_max_mm = 6.5 — must be Ok
        assert!(
            m.min_tensile_strength(Length::from_millimeters(6.5))
                .is_ok(),
            "max boundary diameter must be accepted"
        );
        // Just outside the boundaries — must be rejected
        assert!(
            m.min_tensile_strength(Length::from_millimeters(0.09))
                .is_err(),
            "just below min must be rejected"
        );
        assert!(
            m.min_tensile_strength(Length::from_millimeters(6.51))
                .is_err(),
            "just above max must be rejected"
        );
    }

    // Polynomial MTS form: Sut = c0 + c1*d + c2*d^2 (d in mm, Sut in MPa).
    // Coefficients [2000.0, -5.0, 0.5] → at d=4 mm:
    //   correct:  2000 + (-5)*4 + 0.5*16 = 2000 - 20 + 8 = 1988 MPa
    //   *→+:      2000 + (-5)+4 + 0.5+16 = trivially wrong
    //   *→/:      2000 + (-5)/4 + 0.5/16 = also wrong
    // Pins both polynomial-multiply mutants.
    const POLY_SAMPLE: &str = r#"
[[material]]
name = "Poly Test Wire"
specification = "ASTM A999"
citations = "synthetic test coefficients"
mts_form = "polynomial"
mts_units = "si_mpa_mm"
mts_coefficients = [2000.0, -5.0, 0.5]
valid_dia_min_mm = 1.0
valid_dia_max_mm = 10.0
youngs_modulus_gpa = 200.0
shear_modulus_gpa = 78.0
density_kg_per_m3 = 7850.0
allowable_pct_torsion = 0.45
allowable_pct_bending = 0.75
allowable_pct_set = 0.60
"#;

    #[test]
    fn polynomial_mts_evaluates_correctly() {
        let set = MaterialSet::from_toml_str(POLY_SAMPLE).unwrap();
        let m = set.get("Poly Test Wire").unwrap();
        // d = 4 mm: Sut = 2000 + (-5)*4 + 0.5*16 = 1988 MPa
        let expected = 2000.0 + (-5.0) * 4.0 + 0.5 * 16.0;
        assert_relative_eq!(
            m.min_tensile_strength(Length::from_millimeters(4.0))
                .unwrap()
                .megapascals(),
            expected,
            max_relative = 1e-9
        );
        // d = 3 mm: Sut = 2000 + (-5)*3 + 0.5*9 = 1989.5 MPa (differs from d=4 value)
        let expected3 = 2000.0 + (-5.0) * 3.0 + 0.5 * 9.0;
        assert_relative_eq!(
            m.min_tensile_strength(Length::from_millimeters(3.0))
                .unwrap()
                .megapascals(),
            expected3,
            max_relative = 1e-9
        );
    }

    #[test]
    fn endurance_optional() {
        let set = MaterialSet::from_toml_str(SAMPLE).unwrap();
        let m = set.get("Test Music Wire").unwrap();
        assert!(m.endurance.is_some());
    }

    #[test]
    fn missing_material_errors() {
        let set = MaterialSet::from_toml_str(SAMPLE).unwrap();
        assert_eq!(
            set.get("nope").unwrap_err(),
            SpringError::MaterialNotFound("nope".into())
        );
    }

    #[test]
    fn default_set_loads_seven_materials_with_music_wire() {
        let set = MaterialSet::load_default();
        assert!(set.names().contains(&"Music Wire"));
        // Music wire at 1 mm -> 2211 MPa (Shigley Table 10-4).
        let m = set.get("Music Wire").unwrap();
        assert_relative_eq!(
            m.min_tensile_strength(Length::from_millimeters(1.0))
                .unwrap()
                .megapascals(),
            2211.0,
            max_relative = 1e-9
        );
        // 4 from sub-project 1 + 3 from PR (b).
        assert_eq!(set.names().len(), 7);
    }

    // Rational MTS form: Sut = (P0*d^P4 + P1) / (P2*d^P4 + P3), d in mm, MPa.
    // Coeffs [1000, 500, 0.0, 2.0, 1.0] => Sut = (1000*d + 500) / (0 + 2) = 500*d + 250.
    // At d=3 mm -> 1750 MPa; at d=5 mm -> 2750 MPa (distinct).
    const RATIONAL_SAMPLE: &str = r#"
[[material]]
name = "Rational Test Wire"
specification = "synthetic"
citations = "synthetic test coefficients"
mts_form = "rational"
mts_units = "si_mpa_mm"
mts_coefficients = [1000.0, 500.0, 0.0, 2.0, 1.0]
valid_dia_min_mm = 1.0
valid_dia_max_mm = 10.0
youngs_modulus_gpa = 200.0
shear_modulus_gpa = 78.0
density_kg_per_m3 = 7850.0
allowable_pct_torsion = 0.45
allowable_pct_bending = 0.75
allowable_pct_set = 0.60
"#;

    #[test]
    fn rational_mts_evaluates_correctly() {
        let set = MaterialSet::from_toml_str(RATIONAL_SAMPLE).unwrap();
        let m = set.get("Rational Test Wire").unwrap();
        assert_relative_eq!(
            m.min_tensile_strength(Length::from_millimeters(3.0))
                .unwrap()
                .megapascals(),
            500.0 * 3.0 + 250.0,
            max_relative = 1e-9
        );
        assert_relative_eq!(
            m.min_tensile_strength(Length::from_millimeters(5.0))
                .unwrap()
                .megapascals(),
            500.0 * 5.0 + 250.0,
            max_relative = 1e-9
        );
    }

    #[test]
    fn rational_denominator_zero_is_rejected() {
        // [P0,P1,P2,P3,P4] = [1, 1, 1, -2, 1] => denom = d - 2; at d=2 -> 0.
        let toml = r#"
[[material]]
name = "Bad Rational"
specification = "synthetic"
citations = "synthetic"
mts_form = "rational"
mts_units = "si_mpa_mm"
mts_coefficients = [1.0, 1.0, 1.0, -2.0, 1.0]
valid_dia_min_mm = 1.0
valid_dia_max_mm = 10.0
youngs_modulus_gpa = 200.0
shear_modulus_gpa = 78.0
density_kg_per_m3 = 7850.0
allowable_pct_torsion = 0.45
allowable_pct_bending = 0.75
allowable_pct_set = 0.60
"#;
        let set = MaterialSet::from_toml_str(toml).unwrap();
        let m = set.get("Bad Rational").unwrap();
        let err = m
            .min_tensile_strength(Length::from_millimeters(2.0))
            .unwrap_err();
        assert!(matches!(err, SpringError::InconsistentInputs(_)));
    }

    #[test]
    fn unknown_mts_form_is_data_error_not_panic() {
        let toml = r#"
[[material]]
name = "Bad Form"
specification = "x"
citations = "x"
mts_form = "banana"
mts_units = "si_mpa_mm"
mts_coefficients = [1.0]
valid_dia_min_mm = 1.0
valid_dia_max_mm = 10.0
youngs_modulus_gpa = 200.0
shear_modulus_gpa = 78.0
density_kg_per_m3 = 7850.0
allowable_pct_torsion = 0.45
allowable_pct_bending = 0.75
allowable_pct_set = 0.60
"#;
        let err = MaterialSet::from_toml_str(toml).unwrap_err();
        assert!(matches!(err, SpringError::DataFile(_)));
    }

    #[test]
    fn unknown_mts_units_is_data_error() {
        let toml = r#"
[[material]]
name = "Bad Units"
specification = "x"
citations = "x"
mts_form = "constant"
mts_units = "furlongs"
mts_coefficients = [1.0]
valid_dia_min_mm = 1.0
valid_dia_max_mm = 10.0
youngs_modulus_gpa = 200.0
shear_modulus_gpa = 78.0
density_kg_per_m3 = 7850.0
allowable_pct_torsion = 0.45
allowable_pct_bending = 0.75
allowable_pct_set = 0.60
"#;
        assert!(matches!(
            MaterialSet::from_toml_str(toml).unwrap_err(),
            SpringError::DataFile(_)
        ));
    }

    #[test]
    fn wrong_coefficient_count_is_data_error() {
        // power_law needs exactly 2 coefficients; give 1.
        let toml = r#"
[[material]]
name = "Bad Coeffs"
specification = "x"
citations = "x"
mts_form = "power_law"
mts_units = "si_mpa_mm"
mts_coefficients = [2211.0]
valid_dia_min_mm = 1.0
valid_dia_max_mm = 10.0
youngs_modulus_gpa = 200.0
shear_modulus_gpa = 78.0
density_kg_per_m3 = 7850.0
allowable_pct_torsion = 0.45
allowable_pct_bending = 0.75
allowable_pct_set = 0.60
"#;
        assert!(matches!(
            MaterialSet::from_toml_str(toml).unwrap_err(),
            SpringError::DataFile(_)
        ));
    }

    #[test]
    fn inverted_diameter_range_is_rejected() {
        let toml = r#"
[[material]]
name = "Range Test"
specification = "synthetic"
citations = "synthetic"
mts_form = "constant"
mts_units = "si_mpa_mm"
mts_coefficients = [1500.0]
valid_dia_min_mm = 10.0
valid_dia_max_mm = 1.0
youngs_modulus_gpa = 200.0
shear_modulus_gpa = 78.0
density_kg_per_m3 = 7850.0
allowable_pct_torsion = 0.45
allowable_pct_bending = 0.75
allowable_pct_set = 0.60
"#;
        let err = MaterialSet::from_toml_str(toml).unwrap_err();
        assert!(matches!(err, SpringError::DataFile(_)));
    }

    #[test]
    fn equal_diameter_range_is_accepted() {
        let toml = r#"
[[material]]
name = "Range Test"
specification = "synthetic"
citations = "synthetic"
mts_form = "constant"
mts_units = "si_mpa_mm"
mts_coefficients = [1500.0]
valid_dia_min_mm = 5.0
valid_dia_max_mm = 5.0
youngs_modulus_gpa = 200.0
shear_modulus_gpa = 78.0
density_kg_per_m3 = 7850.0
allowable_pct_torsion = 0.45
allowable_pct_bending = 0.75
allowable_pct_set = 0.60
"#;
        assert!(MaterialSet::from_toml_str(toml).is_ok());
    }

    // --- FIX 1: finiteness + positivity validation on untrusted overlay input ---

    // A known-good RawMaterial fixture; tests mutate one field each.
    fn good_raw() -> RawMaterial {
        RawMaterial {
            name: "Fixture Wire".into(),
            specification: "synthetic".into(),
            citations: "synthetic".into(),
            mts_form: "power_law".into(),
            mts_units: "si_mpa_mm".into(),
            mts_coefficients: vec![2211.0, 0.145],
            valid_dia_min_mm: 0.10,
            valid_dia_max_mm: 6.5,
            youngs_modulus_gpa: 203.4,
            shear_modulus_gpa: 80.0,
            density_kg_per_m3: 7850.0,
            allowable_pct_torsion: 0.45,
            allowable_pct_bending: 0.75,
            allowable_pct_set: 0.60,
            endurance: Some(RawEndurance {
                ssa_mpa: 241.0,
                ssm_mpa: 379.0,
                peened: false,
            }),
            max_service_temp_c: Some(120.0),
        }
    }

    fn assert_data_err(r: RawMaterial) {
        assert!(matches!(
            Material::try_from_raw(r),
            Err(SpringError::DataFile(_))
        ));
    }

    #[test]
    fn good_fixture_parses_ok() {
        assert!(Material::try_from_raw(good_raw()).is_ok());
    }

    #[test]
    fn non_finite_coefficient_is_rejected() {
        let mut r = good_raw();
        r.mts_coefficients = vec![f64::NAN, 0.145];
        assert_data_err(r);
    }

    #[test]
    fn infinite_youngs_modulus_is_rejected() {
        let mut r = good_raw();
        r.youngs_modulus_gpa = f64::INFINITY;
        assert_data_err(r);
    }

    #[test]
    fn non_finite_shear_modulus_is_rejected() {
        let mut r = good_raw();
        r.shear_modulus_gpa = f64::NAN;
        assert_data_err(r);
    }

    #[test]
    fn non_finite_density_is_rejected() {
        let mut r = good_raw();
        r.density_kg_per_m3 = f64::INFINITY;
        assert_data_err(r);
    }

    #[test]
    fn non_finite_valid_dia_min_is_rejected() {
        let mut r = good_raw();
        r.valid_dia_min_mm = f64::NAN;
        assert_data_err(r);
    }

    #[test]
    fn non_finite_valid_dia_max_is_rejected() {
        let mut r = good_raw();
        r.valid_dia_max_mm = f64::INFINITY;
        assert_data_err(r);
    }

    #[test]
    fn non_finite_allowable_pct_torsion_is_rejected() {
        let mut r = good_raw();
        r.allowable_pct_torsion = f64::NAN;
        assert_data_err(r);
    }

    #[test]
    fn non_finite_allowable_pct_bending_is_rejected() {
        let mut r = good_raw();
        r.allowable_pct_bending = f64::INFINITY;
        assert_data_err(r);
    }

    #[test]
    fn non_finite_allowable_pct_set_is_rejected() {
        let mut r = good_raw();
        r.allowable_pct_set = f64::NEG_INFINITY;
        assert_data_err(r);
    }

    #[test]
    fn non_finite_max_service_temp_is_rejected() {
        let mut r = good_raw();
        r.max_service_temp_c = Some(f64::NAN);
        assert_data_err(r);
    }

    #[test]
    fn non_finite_endurance_ssa_is_rejected() {
        let mut r = good_raw();
        r.endurance = Some(RawEndurance {
            ssa_mpa: f64::INFINITY,
            ssm_mpa: 379.0,
            peened: false,
        });
        assert_data_err(r);
    }

    #[test]
    fn non_finite_endurance_ssm_is_rejected() {
        let mut r = good_raw();
        r.endurance = Some(RawEndurance {
            ssa_mpa: 241.0,
            ssm_mpa: f64::NAN,
            peened: false,
        });
        assert_data_err(r);
    }

    // Positivity: each field gets BOTH a 0.0 test (pins `<=` vs `<`) and a
    // negative test (pins `<=` vs `==`), so the full operator-swap mutant set dies.
    #[test]
    fn zero_youngs_modulus_is_rejected() {
        let mut r = good_raw();
        r.youngs_modulus_gpa = 0.0;
        assert_data_err(r);
    }

    #[test]
    fn negative_youngs_modulus_is_rejected() {
        let mut r = good_raw();
        r.youngs_modulus_gpa = -1.0;
        assert_data_err(r);
    }

    #[test]
    fn zero_shear_modulus_is_rejected() {
        let mut r = good_raw();
        r.shear_modulus_gpa = 0.0;
        assert_data_err(r);
    }

    #[test]
    fn negative_shear_modulus_is_rejected() {
        let mut r = good_raw();
        r.shear_modulus_gpa = -1.0;
        assert_data_err(r);
    }

    #[test]
    fn zero_density_is_rejected() {
        let mut r = good_raw();
        r.density_kg_per_m3 = 0.0;
        assert_data_err(r);
    }

    #[test]
    fn negative_density_is_rejected() {
        let mut r = good_raw();
        r.density_kg_per_m3 = -1.0;
        assert_data_err(r);
    }

    #[test]
    fn zero_valid_dia_min_is_rejected() {
        let mut r = good_raw();
        r.valid_dia_min_mm = 0.0;
        assert_data_err(r);
    }

    #[test]
    fn negative_valid_dia_min_is_rejected() {
        let mut r = good_raw();
        r.valid_dia_min_mm = -1.0;
        assert_data_err(r);
    }

    // Boundary: a tiny positive valid_dia_min_mm must be ACCEPTED, pinning the
    // `<` vs `<=` distinction on the positivity guard.
    #[test]
    fn tiny_positive_valid_dia_min_is_accepted() {
        let mut r = good_raw();
        r.valid_dia_min_mm = 1e-6;
        assert!(Material::try_from_raw(r).is_ok());
    }

    // Allowable-stress fractions must lie in (0, 1]: a value > 1.0 from an
    // untrusted overlay would silently suppress overstress warnings and let the
    // optimizer accept designs stressed beyond ultimate; <= 0.0 is nonsensical.
    #[test]
    fn zero_allowable_pct_torsion_is_rejected() {
        let mut r = good_raw();
        r.allowable_pct_torsion = 0.0;
        assert_data_err(r);
    }

    #[test]
    fn negative_allowable_pct_torsion_is_rejected() {
        let mut r = good_raw();
        r.allowable_pct_torsion = -0.1;
        assert_data_err(r);
    }

    #[test]
    fn over_one_allowable_pct_torsion_is_rejected() {
        let mut r = good_raw();
        r.allowable_pct_torsion = 1.5;
        assert_data_err(r);
    }

    // Boundary: exactly 1.0 (allow up to ultimate) is ACCEPTED, pinning `>` vs `>=`.
    #[test]
    fn allowable_pct_torsion_of_one_is_accepted() {
        let mut r = good_raw();
        r.allowable_pct_torsion = 1.0;
        assert!(Material::try_from_raw(r).is_ok());
    }

    #[test]
    fn over_one_allowable_pct_bending_is_rejected() {
        let mut r = good_raw();
        r.allowable_pct_bending = 1.5;
        assert_data_err(r);
    }

    #[test]
    fn over_one_allowable_pct_set_is_rejected() {
        let mut r = good_raw();
        r.allowable_pct_set = 1.5;
        assert_data_err(r);
    }

    // --- MaterialDraft API tests ---

    fn good_draft() -> MaterialDraft {
        MaterialDraft {
            name: "Draft Wire".into(),
            specification: "ASTM A999".into(),
            citations: "synthetic".into(),
            mts_form: MtsForm::PowerLaw,
            mts_units: StrengthUnits::SiMpaMm,
            mts_coefficients: vec![2000.0, 0.15],
            valid_dia_min_mm: 0.5,
            valid_dia_max_mm: 6.0,
            youngs_modulus_gpa: 200.0,
            shear_modulus_gpa: 79.0,
            density_kg_per_m3: 7850.0,
            allowable_pct_torsion: 0.45,
            allowable_pct_bending: 0.75,
            allowable_pct_set: 0.60,
            endurance: None,
            max_service_temp_c: Some(120.0),
        }
    }

    #[test]
    fn draft_builds_valid_material() {
        let draft = good_draft();
        let mat = draft.build().unwrap();
        // PowerLaw: Sut = 2000 / d^0.15 MPa. At d=1mm -> 2000/1^0.15 = 2000 MPa.
        assert_relative_eq!(
            mat.min_tensile_strength(Length::from_millimeters(1.0))
                .unwrap()
                .megapascals(),
            2000.0,
            max_relative = 1e-9
        );
    }

    #[test]
    fn draft_build_rejects_bad_coeff_count() {
        let mut draft = good_draft();
        draft.mts_coefficients = vec![2000.0]; // PowerLaw needs 2
        let err = draft.build().unwrap_err();
        assert!(matches!(err, SpringError::DataFile(_)));
    }

    #[test]
    fn draft_build_rejects_allowable_over_one() {
        let mut draft = good_draft();
        draft.allowable_pct_torsion = 1.5;
        let err = draft.build().unwrap_err();
        assert!(matches!(err, SpringError::DataFile(_)));
    }

    #[test]
    fn to_draft_round_trips_through_build() {
        let set = MaterialSet::load_default();
        let mw = set.get("Music Wire").unwrap();
        let draft = mw.to_draft();
        let rebuilt = draft.build().unwrap();

        assert_eq!(rebuilt.name, mw.name);
        assert_relative_eq!(
            rebuilt
                .min_tensile_strength(Length::from_millimeters(2.0))
                .unwrap()
                .megapascals(),
            mw.min_tensile_strength(Length::from_millimeters(2.0))
                .unwrap()
                .megapascals(),
            max_relative = 1e-12
        );
        assert_relative_eq!(
            rebuilt.youngs_modulus.pascals(),
            mw.youngs_modulus.pascals(),
            max_relative = 1e-12
        );
    }

    #[test]
    fn to_draft_preserves_polynomial_phosphor_bronze() {
        let set = MaterialSet::load_default();
        let pb = set.get("Phosphor Bronze").unwrap();
        let draft = pb.to_draft();
        assert_eq!(draft.mts_form, MtsForm::Polynomial);
        assert_eq!(draft.mts_coefficients.len(), 3);
    }

    #[test]
    fn draft_build_preserves_endurance() {
        // Guards the build() endurance->RawEndurance mapping: a dropped or
        // mis-mapped field would otherwise go undetected.
        let mut d = good_draft();
        d.endurance = Some(EnduranceDraft {
            ssa_mpa: 241.0,
            ssm_mpa: 379.0,
            peened: true,
        });
        let m = d.build().unwrap();
        let e = m.endurance.expect("endurance preserved through build");
        assert_relative_eq!(e.ssa.megapascals(), 241.0, max_relative = 1e-9);
        assert_relative_eq!(e.ssm.megapascals(), 379.0, max_relative = 1e-9);
        assert!(e.peened);
        // And it survives a full to_draft round-trip.
        assert_eq!(m.to_draft().endurance, d.endurance);
    }

    #[test]
    fn max_service_temperature_parses_when_present_and_absent() {
        // Present:
        let with_temp = r#"
[[material]]
name = "Temp Wire"
specification = "synthetic"
citations = "synthetic"
mts_form = "constant"
mts_units = "si_mpa_mm"
mts_coefficients = [1500.0]
valid_dia_min_mm = 1.0
valid_dia_max_mm = 10.0
youngs_modulus_gpa = 200.0
shear_modulus_gpa = 78.0
density_kg_per_m3 = 7850.0
allowable_pct_torsion = 0.45
allowable_pct_bending = 0.75
allowable_pct_set = 0.60
max_service_temp_c = 120.0
"#;
        let m = MaterialSet::from_toml_str(with_temp).unwrap();
        let mat = m.get("Temp Wire").unwrap();
        assert_relative_eq!(
            mat.max_service_temperature.unwrap().celsius(),
            120.0,
            max_relative = 1e-12
        );
        // Absent -> None (the existing SAMPLE has no max_service_temp_c).
        let s = MaterialSet::from_toml_str(SAMPLE).unwrap();
        assert!(s
            .get("Test Music Wire")
            .unwrap()
            .max_service_temperature
            .is_none());
    }
}
