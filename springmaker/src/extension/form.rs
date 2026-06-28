//! Pure extension form-to-design logic. No iced dependency.
use crate::form_helpers::{
    fmt_force, fmt_len, fmt_loads, length_mm, loads_n, non_negative_force_n, positive_num,
};
use springcore::extension::{ExtensionDesign, HookEnds, PowerUser, Scenario};
use springcore::units::{Force, Length};
use springcore::{
    CurvatureCorrection, DesignSpec, ExtScenarioSpec, HookSpecSpec, Material, MaterialStore,
    Result, UnitSystem,
};

/// Which extension text field a `Message::ExtField` targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    WireDia,
    MeanDia,
    Active,
    FreeLength,
    InitialTension,
    Loads,
    HookR1,
    HookR2,
}

/// Hook geometry mode: standard machine loops (r1 = D/2, r2 = D/4) or
/// user-specified absolute radii.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HookMode {
    #[default]
    Default,
    Custom,
}

impl std::fmt::Display for HookMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            HookMode::Default => "Default (machine loops)",
            HookMode::Custom => "Custom radii",
        })
    }
}

/// Extension PowerUser inputs as raw strings (+ hook mode and custom radii).
#[derive(Debug, Clone)]
pub struct ExtFormState {
    pub wire_dia: String,
    pub mean_dia: String,
    pub active: String,
    pub free_length: String,
    pub initial_tension: String,
    pub loads: String,
    pub hook_mode: HookMode,
    pub hook_r1: String,
    pub hook_r2: String,
}

impl Default for ExtFormState {
    fn default() -> Self {
        Self {
            wire_dia: String::new(),
            mean_dia: String::new(),
            active: String::new(),
            free_length: String::new(),
            initial_tension: String::new(),
            loads: String::new(),
            hook_mode: HookMode::Default,
            hook_r1: String::new(),
            hook_r2: String::new(),
        }
    }
}

/// A solved extension form: the design (which carries engine-computed status).
#[derive(Debug, Clone)]
pub struct ExtFormOutcome {
    pub design: ExtensionDesign,
}

fn resolve_hooks(form: &ExtFormState, mean_dia_mm: f64, us: UnitSystem) -> Result<HookEnds> {
    match form.hook_mode {
        HookMode::Default => Ok(HookEnds::default_for(Length::from_millimeters(mean_dia_mm))),
        HookMode::Custom => Ok(HookEnds {
            r1: Length::from_millimeters(length_mm("hook radius r1", &form.hook_r1, us)?),
            r2: Length::from_millimeters(length_mm("hook radius r2", &form.hook_r2, us)?),
        }),
    }
}

/// Parse the form, resolve hooks, build `PowerUser`, and solve. The engine's
/// own input guards remain the defense-in-depth backstop.
pub fn parse_and_solve(
    form: &ExtFormState,
    material_name: &str,
    us: UnitSystem,
    materials: &MaterialStore,
    correction: CurvatureCorrection,
) -> Result<ExtFormOutcome> {
    let material: &Material = materials.get(material_name)?;
    let mean_dia_mm = length_mm("mean diameter", &form.mean_dia, us)?;
    let hooks = resolve_hooks(form, mean_dia_mm, us)?;
    let scenario = PowerUser {
        wire_dia: Length::from_millimeters(length_mm("wire diameter", &form.wire_dia, us)?),
        mean_dia: Length::from_millimeters(mean_dia_mm),
        active: positive_num("active coils", &form.active)?,
        free_length: Length::from_millimeters(length_mm("free length", &form.free_length, us)?),
        initial_tension: Force::from_newtons(non_negative_force_n(
            "initial tension",
            &form.initial_tension,
            us,
        )?),
        hooks,
        loads: loads_n(&form.loads, us)?
            .into_iter()
            .map(Force::from_newtons)
            .collect(),
    };
    let design = scenario.solve(material, correction)?;
    Ok(ExtFormOutcome { design })
}

/// Parse `form` into a persisted `DesignSpec::Extension` (SI mm/N). Round-trips with
/// `populate_from_spec`.
pub fn build_spec(form: &ExtFormState, us: UnitSystem) -> Result<DesignSpec> {
    let mean_dia_mm = length_mm("mean diameter", &form.mean_dia, us)?;
    let hooks = match form.hook_mode {
        HookMode::Default => HookSpecSpec::Default,
        HookMode::Custom => HookSpecSpec::Custom {
            r1_mm: length_mm("hook radius r1", &form.hook_r1, us)?,
            r2_mm: length_mm("hook radius r2", &form.hook_r2, us)?,
        },
    };
    Ok(DesignSpec::Extension(ExtScenarioSpec::PowerUser {
        wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
        mean_dia_mm,
        active: positive_num("active coils", &form.active)?,
        free_length_mm: length_mm("free length", &form.free_length, us)?,
        initial_tension_n: non_negative_force_n("initial tension", &form.initial_tension, us)?,
        hooks,
        loads_n: loads_n(&form.loads, us)?,
    }))
}

/// Write a persisted `ExtScenarioSpec` back into `form`, converting SI mm/N to display
/// units. After this call, `build_spec(form, us)` reproduces the spec.
pub fn populate_from_spec(form: &mut ExtFormState, spec: &ExtScenarioSpec, us: UnitSystem) {
    let ExtScenarioSpec::PowerUser {
        wire_dia_mm,
        mean_dia_mm,
        active,
        free_length_mm,
        initial_tension_n,
        hooks,
        loads_n,
    } = spec;
    form.wire_dia = fmt_len(*wire_dia_mm, us);
    form.mean_dia = fmt_len(*mean_dia_mm, us);
    form.active = format!("{active}");
    form.free_length = fmt_len(*free_length_mm, us);
    form.initial_tension = fmt_force(*initial_tension_n, us);
    form.loads = fmt_loads(loads_n, us);
    match hooks {
        HookSpecSpec::Default => {
            form.hook_mode = HookMode::Default;
            form.hook_r1 = String::new();
            form.hook_r2 = String::new();
        }
        HookSpecSpec::Custom { r1_mm, r2_mm } => {
            form.hook_mode = HookMode::Custom;
            form.hook_r1 = fmt_len(*r1_mm, us);
            form.hook_r2 = fmt_len(*r2_mm, us);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use springcore::material_store::MaterialStore;
    use springcore::{CurvatureCorrection, MaterialSet, UnitSystem};

    fn default_materials() -> MaterialStore {
        MaterialStore::new(MaterialSet::load_default())
    }

    fn default_material_name() -> &'static str {
        "Music Wire"
    }

    /// Build a typical metric PowerUser form: d=2mm, D=20mm, n=10, L0=100mm, Fi=5N, P=50N.
    fn metric_form() -> ExtFormState {
        ExtFormState {
            wire_dia: "2".to_string(),
            mean_dia: "20".to_string(),
            active: "10".to_string(),
            free_length: "100".to_string(),
            initial_tension: "5".to_string(),
            loads: "50".to_string(),
            hook_mode: HookMode::Default,
            hook_r1: String::new(),
            hook_r2: String::new(),
        }
    }

    #[test]
    fn metric_poweruser_solves_and_rate_is_reasonable() {
        let materials = default_materials();
        let out = parse_and_solve(
            &metric_form(),
            default_material_name(),
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .expect("should solve");
        // rate = G * d^4 / (8 * D^3 * n) ≈ 2000 N/m for this geometry
        let rate_npm = out.design.rate.newtons_per_meter();
        assert_relative_eq!(rate_npm, 2000.0, epsilon = 500.0);
    }

    #[test]
    fn us_unit_conversion_gives_same_rate() {
        let materials = default_materials();
        // Same geometry in inches: d=2/25.4, D=20/25.4, L0=100/25.4
        let form_us = ExtFormState {
            wire_dia: format!("{}", 2.0_f64 / 25.4),
            mean_dia: format!("{}", 20.0_f64 / 25.4),
            active: "10".to_string(),
            free_length: format!("{}", 100.0_f64 / 25.4),
            initial_tension: format!("{}", 5.0_f64 / 4.448_222),
            loads: format!("{}", 50.0_f64 / 4.448_222),
            ..ExtFormState::default()
        };
        let out_metric = parse_and_solve(
            &metric_form(),
            default_material_name(),
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .unwrap();
        let out_us = parse_and_solve(
            &form_us,
            default_material_name(),
            UnitSystem::Us,
            &materials,
            CurvatureCorrection::default(),
        )
        .unwrap();
        assert_relative_eq!(
            out_metric.design.rate.newtons_per_meter(),
            out_us.design.rate.newtons_per_meter(),
            epsilon = 1.0 // N/m tolerance after double inch→mm conversion
        );
    }

    #[test]
    fn default_hook_mode_uses_d_over_2_and_d_over_4() {
        let materials = default_materials();
        let out = parse_and_solve(
            &metric_form(),
            default_material_name(),
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .unwrap();
        // HookEnds::default_for(D=20mm) → r1=10mm, r2=5mm
        assert_relative_eq!(out.design.hooks.r1.millimeters(), 10.0, epsilon = 1e-9);
        assert_relative_eq!(out.design.hooks.r2.millimeters(), 5.0, epsilon = 1e-9);
    }

    #[test]
    fn custom_hook_mode_parses_supplied_radii() {
        let materials = default_materials();
        let form = ExtFormState {
            hook_mode: HookMode::Custom,
            hook_r1: "8".to_string(),
            hook_r2: "4".to_string(),
            ..metric_form()
        };
        let out = parse_and_solve(
            &form,
            default_material_name(),
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .unwrap();
        assert_relative_eq!(out.design.hooks.r1.millimeters(), 8.0, epsilon = 1e-9);
        assert_relative_eq!(out.design.hooks.r2.millimeters(), 4.0, epsilon = 1e-9);
    }

    #[test]
    fn blank_wire_dia_errors() {
        let materials = default_materials();
        let form = ExtFormState {
            wire_dia: String::new(),
            ..metric_form()
        };
        assert!(parse_and_solve(
            &form,
            default_material_name(),
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default()
        )
        .is_err());
    }

    #[test]
    fn nan_mean_dia_errors() {
        let materials = default_materials();
        let form = ExtFormState {
            mean_dia: "nan".to_string(),
            ..metric_form()
        };
        assert!(parse_and_solve(
            &form,
            default_material_name(),
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default()
        )
        .is_err());
    }

    #[test]
    fn inf_free_length_errors() {
        let materials = default_materials();
        let form = ExtFormState {
            free_length: "inf".to_string(),
            ..metric_form()
        };
        assert!(parse_and_solve(
            &form,
            default_material_name(),
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default()
        )
        .is_err());
    }

    #[test]
    fn negative_active_coils_errors() {
        let materials = default_materials();
        let form = ExtFormState {
            active: "-5".to_string(),
            ..metric_form()
        };
        assert!(parse_and_solve(
            &form,
            default_material_name(),
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default()
        )
        .is_err());
    }

    /// Blank `loads` is explicitly allowed: extension springs have geometry and
    /// rate regardless of applied loads, so a zero-load form is a valid query
    /// for spring properties (index, rate, dimensions) without stress analysis.
    #[test]
    fn blank_loads_returns_ok_with_empty_load_points() {
        let materials = default_materials();
        let form = ExtFormState {
            loads: String::new(),
            ..metric_form()
        };
        let out = parse_and_solve(
            &form,
            default_material_name(),
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .expect("blank loads should be allowed");
        assert!(
            out.design.load_points.is_empty(),
            "no load points when loads field is blank"
        );
    }

    #[test]
    fn negative_initial_tension_errors() {
        let materials = default_materials();
        let form = ExtFormState {
            initial_tension: "-5".to_string(),
            ..metric_form()
        };
        assert!(
            parse_and_solve(
                &form,
                default_material_name(),
                UnitSystem::Metric,
                &materials,
                CurvatureCorrection::Bergstrasser,
            )
            .is_err(),
            "negative initial_tension must be rejected via non_negative_force_n"
        );
    }

    #[test]
    fn custom_hook_with_blank_r1_errors() {
        let materials = default_materials();
        let form = ExtFormState {
            hook_mode: HookMode::Custom,
            hook_r1: String::new(),
            hook_r2: "4".to_string(),
            ..metric_form()
        };
        assert!(parse_and_solve(
            &form,
            default_material_name(),
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default()
        )
        .is_err());
    }

    /// `build_spec` → extract `ExtScenarioSpec` → `populate_from_spec` → `build_spec` again
    /// must produce identical specs. Verified for both Default and Custom hook modes.
    #[test]
    fn build_spec_populate_round_trip() {
        let us = UnitSystem::Metric;

        // Default hooks
        let form = metric_form();
        let spec1 = build_spec(&form, us).unwrap();
        let inner = match &spec1 {
            springcore::DesignSpec::Extension(s) => s,
            _ => panic!("expected Extension"),
        };
        let mut form2 = ExtFormState::default();
        populate_from_spec(&mut form2, inner, us);
        let spec2 = build_spec(&form2, us).unwrap();
        assert_eq!(spec1, spec2, "default hooks: round-trip must be lossless");

        // Custom hooks
        let form_custom = ExtFormState {
            hook_mode: HookMode::Custom,
            hook_r1: "8".to_string(),
            hook_r2: "4".to_string(),
            ..metric_form()
        };
        let spec3 = build_spec(&form_custom, us).unwrap();
        let inner3 = match &spec3 {
            springcore::DesignSpec::Extension(s) => s,
            _ => panic!("expected Extension"),
        };
        let mut form4 = ExtFormState::default();
        populate_from_spec(&mut form4, inner3, us);
        let spec4 = build_spec(&form4, us).unwrap();
        assert_eq!(spec3, spec4, "custom hooks: round-trip must be lossless");
    }

    /// Loading a Default-hook spec onto a form that previously held Custom radii
    /// must clear `hook_r1` / `hook_r2` so stale values cannot leak to the user
    /// if they later toggle back to Custom mode.
    #[test]
    fn default_hook_load_clears_stale_radii() {
        let us = UnitSystem::Metric;

        // Start from a Custom-hook form with non-empty radii.
        let mut form = ExtFormState {
            hook_mode: HookMode::Custom,
            hook_r1: "8".to_string(),
            hook_r2: "4".to_string(),
            ..metric_form()
        };

        // Build a Default-hook spec and populate it over the stale Custom form.
        let default_spec_inner = match &build_spec(&metric_form(), us).unwrap() {
            springcore::DesignSpec::Extension(s) => s.clone(),
            _ => panic!("expected Extension"),
        };
        populate_from_spec(&mut form, &default_spec_inner, us);

        assert_eq!(form.hook_mode, HookMode::Default, "mode must switch to Default");
        assert!(
            form.hook_r1.is_empty(),
            "hook_r1 must be cleared when loading a Default-hook spec"
        );
        assert!(
            form.hook_r2.is_empty(),
            "hook_r2 must be cleared when loading a Default-hook spec"
        );
    }
}
