//! Form-to-draft logic for the material editor.
//!
//! Converts between [`MaterialsFormState`] (raw `String` fields, as entered
//! by the user) and [`MaterialDraft`] (validated, typed DTO).  No iced
//! imports — this module is pure logic.

// The public API is not yet wired into the GUI layer (future tasks).
#![allow(dead_code)]

use springcore::{
    EnduranceDraft, Material, MaterialDraft, MtsForm, Result, SpringError, StrengthUnits,
};

// ── Form state ────────────────────────────────────────────────────────────────

/// Raw string state for the material-editor form.
///
/// Every numeric field is stored as a `String` so the UI can display partial
/// input without forcing valid numbers at each keystroke.  Call
/// [`build_draft`] to parse and validate.
#[derive(Debug, Clone)]
pub struct MaterialsFormState {
    /// Material name (the unique key; must not collide with a curated name).
    pub name: String,
    /// Specification / standard designation (e.g. `"ASTM A228"`).
    pub specification: String,
    /// Source citations for the material's constants.
    pub citations: String,
    /// Minimum-tensile-strength equation form.
    pub mts_form: MtsForm,
    /// Unit system the strength coefficients are expressed in.
    pub mts_units: StrengthUnits,
    /// Comma-separated coefficients, e.g. `"2000, 0.15"`.
    pub coefficients: String,
    /// Valid wire-diameter minimum, in mm.
    pub valid_dia_min: String,
    /// Valid wire-diameter maximum, in mm.
    pub valid_dia_max: String,
    /// Young's modulus, in GPa.
    pub youngs_modulus: String,
    /// Shear modulus, in GPa.
    pub shear_modulus: String,
    /// Density, in kg/m³.
    pub density: String,
    /// Allowable torsional stress as a fraction of MTS (e.g. `"0.45"`).
    pub allowable_torsion: String,
    /// Allowable bending stress as a fraction of MTS (e.g. `"0.75"`).
    pub allowable_bending: String,
    /// Allowable stress before permanent set (e.g. `"0.60"`).
    pub allowable_set: String,
    /// Whether to include Zimmerli endurance data.
    pub has_endurance: bool,
    /// Alternating shear endurance strength, in MPa.
    pub endurance_ssa: String,
    /// Mean shear endurance strength, in MPa.
    pub endurance_ssm: String,
    /// Whether the endurance data is for a shot-peened wire.
    pub endurance_peened: bool,
    /// Whether a maximum service temperature is specified.
    pub has_max_temp: bool,
    /// Maximum service temperature, in °C.
    pub max_temp_c: String,
}

impl Default for MaterialsFormState {
    fn default() -> Self {
        Self {
            name: String::new(),
            specification: String::new(),
            citations: String::new(),
            mts_form: MtsForm::PowerLaw,
            mts_units: StrengthUnits::SiMpaMm,
            coefficients: String::new(),
            valid_dia_min: String::new(),
            valid_dia_max: String::new(),
            youngs_modulus: String::new(),
            shear_modulus: String::new(),
            density: String::new(),
            allowable_torsion: String::new(),
            allowable_bending: String::new(),
            allowable_set: String::new(),
            has_endurance: false,
            endurance_ssa: String::new(),
            endurance_ssm: String::new(),
            endurance_peened: false,
            has_max_temp: false,
            max_temp_c: String::new(),
        }
    }
}

// ── Parse helpers ─────────────────────────────────────────────────────────────

/// Parse a single numeric field; return `InconsistentInputs` on failure.
///
/// Rejects empty strings, non-numeric text, and non-finite values (±∞, NaN).
fn num(field: &str, value: &str) -> Result<f64> {
    let v = value.trim().parse::<f64>().map_err(|_| {
        SpringError::InconsistentInputs(format!("{field} is not a number: '{value}'"))
    })?;
    if !v.is_finite() {
        return Err(SpringError::InconsistentInputs(format!(
            "{field} must be a finite number: '{value}'"
        )));
    }
    Ok(v)
}

/// Parse a comma-separated list of coefficients.
///
/// Returns an error if any token is non-numeric or the list is empty.
fn parse_coefficients(value: &str) -> Result<Vec<f64>> {
    let tokens: Vec<&str> = value.split(',').map(str::trim).collect();
    let mut out = Vec::with_capacity(tokens.len());
    for token in &tokens {
        if token.is_empty() {
            return Err(SpringError::InconsistentInputs(
                "coefficients: empty token in list".to_string(),
            ));
        }
        out.push(num("coefficient", token)?);
    }
    // `split(',')` always yields >= 1 token, and an empty input yields a single
    // "" token caught above, so `out` is guaranteed non-empty here. The required
    // coefficient *count* per form is enforced later by `MaterialDraft::build`.
    Ok(out)
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Parse a [`MaterialsFormState`] into a [`MaterialDraft`].
///
/// Only field-level parsing and finiteness are checked here.  Count, range,
/// and physical-validity checks are delegated to [`MaterialDraft::build`].
pub fn build_draft(form: &MaterialsFormState) -> Result<MaterialDraft> {
    let mts_coefficients = parse_coefficients(&form.coefficients)?;

    let endurance = if form.has_endurance {
        Some(EnduranceDraft {
            ssa_mpa: num("endurance_ssa", &form.endurance_ssa)?,
            ssm_mpa: num("endurance_ssm", &form.endurance_ssm)?,
            peened: form.endurance_peened,
        })
    } else {
        None
    };

    let max_service_temp_c = if form.has_max_temp {
        Some(num("max_temp_c", &form.max_temp_c)?)
    } else {
        None
    };

    Ok(MaterialDraft {
        name: form.name.clone(),
        specification: form.specification.clone(),
        citations: form.citations.clone(),
        mts_form: form.mts_form,
        mts_units: form.mts_units,
        mts_coefficients,
        valid_dia_min_mm: num("valid_dia_min", &form.valid_dia_min)?,
        valid_dia_max_mm: num("valid_dia_max", &form.valid_dia_max)?,
        youngs_modulus_gpa: num("youngs_modulus", &form.youngs_modulus)?,
        shear_modulus_gpa: num("shear_modulus", &form.shear_modulus)?,
        density_kg_per_m3: num("density", &form.density)?,
        allowable_pct_torsion: num("allowable_torsion", &form.allowable_torsion)?,
        allowable_pct_bending: num("allowable_bending", &form.allowable_bending)?,
        allowable_pct_set: num("allowable_set", &form.allowable_set)?,
        endurance,
        max_service_temp_c,
    })
}

/// Populate a [`MaterialsFormState`] from an existing [`Material`].
///
/// Uses [`Material::to_draft`] as the canonical source of truth so the
/// round-trip `populate_from_material` → [`build_draft`] → `build()` is
/// always consistent with the material's internal representation.
pub fn populate_from_material(form: &mut MaterialsFormState, m: &Material) {
    let d = m.to_draft();

    form.name = d.name;
    form.specification = d.specification;
    form.citations = d.citations;
    form.mts_form = d.mts_form;
    form.mts_units = d.mts_units;
    form.coefficients = d
        .mts_coefficients
        .iter()
        .map(|x| format!("{x}"))
        .collect::<Vec<_>>()
        .join(", ");
    form.valid_dia_min = format!("{}", d.valid_dia_min_mm);
    form.valid_dia_max = format!("{}", d.valid_dia_max_mm);
    form.youngs_modulus = format!("{}", d.youngs_modulus_gpa);
    form.shear_modulus = format!("{}", d.shear_modulus_gpa);
    form.density = format!("{}", d.density_kg_per_m3);
    form.allowable_torsion = format!("{}", d.allowable_pct_torsion);
    form.allowable_bending = format!("{}", d.allowable_pct_bending);
    form.allowable_set = format!("{}", d.allowable_pct_set);

    match d.endurance {
        Some(e) => {
            form.has_endurance = true;
            form.endurance_ssa = format!("{}", e.ssa_mpa);
            form.endurance_ssm = format!("{}", e.ssm_mpa);
            form.endurance_peened = e.peened;
        }
        None => {
            form.has_endurance = false;
            form.endurance_ssa = String::new();
            form.endurance_ssm = String::new();
            form.endurance_peened = false;
        }
    }

    match d.max_service_temp_c {
        Some(t) => {
            form.has_max_temp = true;
            form.max_temp_c = format!("{t}");
        }
        None => {
            form.has_max_temp = false;
            form.max_temp_c = String::new();
        }
    }
}

/// Advisory hint labels for the comma-separated `coefficients` input of the
/// given form, in order. These describe what each value means; they do NOT
/// constrain the input widget (coefficients are entered as one comma-separated
/// string). `Polynomial` accepts any number of coefficients (>= 1, ascending
/// powers), so its labels are illustrative.
pub fn coefficient_labels(form: MtsForm) -> &'static [&'static str] {
    match form {
        MtsForm::Constant => &["UTS"],
        MtsForm::PowerLaw => &["A (strength·dia^m)", "m"],
        MtsForm::Polynomial => &["c0", "c1", "c2, … (ascending powers)"],
        MtsForm::Rational => &["P0", "P1", "P2", "P3", "P4"],
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use springcore::MaterialSet;

    fn power_law_form() -> MaterialsFormState {
        MaterialsFormState {
            name: "Test Wire".to_string(),
            specification: "ASTM A228".to_string(),
            citations: "Shigley Table 10-4".to_string(),
            coefficients: "2000, 0.15".to_string(),
            valid_dia_min: "0.5".to_string(),
            valid_dia_max: "6.0".to_string(),
            youngs_modulus: "200".to_string(),
            shear_modulus: "79".to_string(),
            density: "7850".to_string(),
            allowable_torsion: "0.45".to_string(),
            allowable_bending: "0.75".to_string(),
            allowable_set: "0.6".to_string(),
            ..MaterialsFormState::default()
        }
    }

    #[test]
    fn build_draft_parses_power_law() {
        let d = build_draft(&power_law_form()).unwrap();
        assert_eq!(d.mts_coefficients, vec![2000.0, 0.15]);
        assert!(d.build().is_ok());
    }

    #[test]
    fn build_draft_rejects_nonnumeric_coefficient() {
        let mut f = power_law_form();
        f.coefficients = "2000, abc".into();
        assert!(build_draft(&f).is_err());
    }

    #[test]
    fn build_draft_rejects_empty_coefficients() {
        let mut f = power_law_form();
        f.coefficients = String::new();
        assert!(build_draft(&f).is_err());
    }

    #[test]
    fn build_draft_rejects_trailing_comma_coefficients() {
        let mut f = power_law_form();
        f.coefficients = "2000, 0.15,".into();
        assert!(build_draft(&f).is_err());
    }

    #[test]
    fn build_draft_rejects_empty_modulus() {
        let mut f = power_law_form();
        f.youngs_modulus = "".into();
        assert!(build_draft(&f).is_err());
    }

    #[test]
    fn build_draft_includes_endurance_when_enabled() {
        let mut f = power_law_form();
        f.has_endurance = true;
        f.endurance_ssa = "241".into();
        f.endurance_ssm = "379".into();
        f.endurance_peened = true;
        let d = build_draft(&f).unwrap();
        let e = d.endurance.unwrap();
        assert_eq!(e.ssa_mpa, 241.0);
        assert!(e.peened);
    }

    #[test]
    fn populate_round_trips_via_to_draft() {
        let set = MaterialSet::load_default();
        let mut f = MaterialsFormState::default();
        populate_from_material(&mut f, set.get("Music Wire").unwrap());
        assert_eq!(f.name, "Music Wire");
        assert_eq!(f.mts_form, MtsForm::PowerLaw);
        assert!(build_draft(&f).unwrap().build().is_ok());
    }

    #[test]
    fn coefficient_labels_match_form() {
        assert_eq!(coefficient_labels(MtsForm::Rational).len(), 5);
        assert_eq!(coefficient_labels(MtsForm::PowerLaw).len(), 2);
        assert_eq!(coefficient_labels(MtsForm::Constant).len(), 1);
    }
}
