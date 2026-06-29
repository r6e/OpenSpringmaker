//! Pure extension form-to-design logic. No iced dependency.
use crate::form_helpers::{
    fmt_force, fmt_len, fmt_loads, fmt_rate, length_mm, loads_n, non_negative_force_n,
    positive_force_n, positive_num, rate_npm,
};
use springcore::extension::{
    solve_min_weight, Dimensional, ExtBindingConstraint, ExtMinWeightRequest, ExtensionDesign,
    HookEnds, HookSpec, PowerUser, RateBased, Scenario, TwoLoad,
};
use springcore::units::{Force, Length, SpringRate};
use springcore::{
    CurvatureCorrection, ExtScenarioSpec, HookSpecSpec, Material, MaterialStore, Result, UnitSystem,
};

/// Which extension input scenario is active. The extension family's own enum
/// (not compression's `ScenarioKind`) — the module boundary forbids importing
/// compression, and the per-mode field sets and solve paths differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExtScenarioKind {
    #[default]
    PowerUser,
    RateBased,
    Dimensional,
    TwoLoad,
    MinWeight,
}

/// All `ExtScenarioKind` variants in display order.
pub const ALL_EXT_SCENARIOS: &[ExtScenarioKind] = &[
    ExtScenarioKind::PowerUser,
    ExtScenarioKind::RateBased,
    ExtScenarioKind::Dimensional,
    ExtScenarioKind::TwoLoad,
    ExtScenarioKind::MinWeight,
];

impl std::fmt::Display for ExtScenarioKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtScenarioKind::PowerUser => write!(f, "Power User"),
            ExtScenarioKind::RateBased => write!(f, "Rate Based"),
            ExtScenarioKind::Dimensional => write!(f, "Dimensional"),
            ExtScenarioKind::TwoLoad => write!(f, "Two Load"),
            ExtScenarioKind::MinWeight => write!(f, "Min Weight"),
        }
    }
}

/// Which extension text field a `Message::ExtField` targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    WireDia,
    MeanDia,
    OuterDia,
    Active,
    FreeLength,
    InitialTension,
    Loads,
    Rate,
    HookR1,
    HookR2,
    Force1,
    Length1,
    Force2,
    Length2,
    MaxForce,
    CandidateDiameters,
    IndexMin,
    IndexMax,
    MaxOuterDia,
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

/// Extension form inputs as raw strings (+ scenario, hook mode and custom radii).
#[derive(Debug, Clone)]
pub struct ExtFormState {
    pub scenario: ExtScenarioKind,
    pub wire_dia: String,
    pub mean_dia: String,
    pub outer_dia: String,
    pub active: String,
    pub free_length: String,
    pub initial_tension: String,
    pub loads: String,
    pub rate: String,
    pub hook_mode: HookMode,
    pub hook_r1: String,
    pub hook_r2: String,
    pub force1: String,
    pub length1: String,
    pub force2: String,
    pub length2: String,
    pub max_force: String,
    pub candidate_diameters: String,
    pub index_min: String,
    pub index_max: String,
    pub max_outer_dia: String,
}

impl Default for ExtFormState {
    fn default() -> Self {
        Self {
            scenario: ExtScenarioKind::default(),
            wire_dia: String::new(),
            mean_dia: String::new(),
            outer_dia: String::new(),
            active: String::new(),
            free_length: String::new(),
            initial_tension: String::new(),
            loads: String::new(),
            rate: String::new(),
            hook_mode: HookMode::Default,
            hook_r1: String::new(),
            hook_r2: String::new(),
            force1: String::new(),
            length1: String::new(),
            force2: String::new(),
            length2: String::new(),
            max_force: String::new(),
            candidate_diameters: String::new(),
            index_min: "4".into(),
            index_max: "12".into(),
            max_outer_dia: String::new(),
        }
    }
}

impl ExtFormState {
    /// Whether the user has entered none of the active scenario's primary input
    /// fields. Drives the "untouched form" suppression in `App::recompute`: the
    /// results panel stays in its initial Empty state until ANY of those fields is
    /// filled, after which parse feedback shows. The list is the fields whose presence
    /// means the user has begun a design — `loads` is included even though an empty
    /// loads list is itself valid (typing into it still signals intent). In Default
    /// hook mode the radii are auto-resolved from the mean diameter and are not input;
    /// in Custom mode they are user input and count toward blankness. Mirrors
    /// `compression::form::FormState::is_blank`.
    pub fn is_blank(&self) -> bool {
        let all_empty = |fields: &[&String]| fields.iter().all(|f| f.trim().is_empty());
        let hooks_blank = match self.hook_mode {
            HookMode::Default => true,
            HookMode::Custom => self.hook_r1.trim().is_empty() && self.hook_r2.trim().is_empty(),
        };
        let core_blank = match self.scenario {
            ExtScenarioKind::PowerUser => all_empty(&[
                &self.wire_dia,
                &self.mean_dia,
                &self.active,
                &self.free_length,
                &self.initial_tension,
                &self.loads,
            ]),
            ExtScenarioKind::RateBased => all_empty(&[
                &self.wire_dia,
                &self.mean_dia,
                &self.rate,
                &self.free_length,
                &self.initial_tension,
                &self.loads,
            ]),
            ExtScenarioKind::Dimensional => all_empty(&[
                &self.wire_dia,
                &self.outer_dia,
                &self.active,
                &self.free_length,
                &self.initial_tension,
                &self.loads,
            ]),
            ExtScenarioKind::TwoLoad => all_empty(&[
                &self.wire_dia,
                &self.mean_dia,
                &self.free_length,
                &self.force1,
                &self.length1,
                &self.force2,
                &self.length2,
            ]),
            // Every displayed MinWeight input EXCEPT the pre-filled index bounds
            // (`index_min`/`index_max` default to "4"/"12", so they do not signal that
            // the user has begun). `rate`, `max_force`, `initial_tension`, and
            // `candidate_diameters` are required; `max_outer_dia` is optional (valid-empty)
            // but still counts, since typing it signals intent — the same reason `loads`
            // counts in the forward modes.
            ExtScenarioKind::MinWeight => all_empty(&[
                &self.rate,
                &self.max_force,
                &self.initial_tension,
                &self.max_outer_dia,
                &self.candidate_diameters,
            ]),
        };
        core_blank && hooks_blank
    }
}

/// Extra outputs produced only by the extension Min-Weight optimisation path.
#[derive(Debug, Clone)]
pub(crate) struct ExtMinWeightExtra {
    pub binding: ExtBindingConstraint,
    pub mass_kg: f64,
}

/// A solved extension form: the design (which carries engine-computed status),
/// plus optimisation extras when the Min-Weight path produced it.
#[derive(Debug, Clone)]
pub struct ExtFormOutcome {
    pub design: ExtensionDesign,
    pub min_weight: Option<ExtMinWeightExtra>,
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

/// Mean coil diameter for the Dimensional scenario (`outer − wire`), rejecting a
/// non-positive result with a field-named error. Dimensional is the only scenario
/// whose mean diameter is *derived* by subtraction rather than entered directly, so an
/// outer diameter at or below the wire diameter yields `mean ≤ 0` — which feeds
/// non-positive default hook radii (`HookEnds::default_for`) into the solve (negative for
/// `outer < wire`, zero at `outer == wire`). The engine independently
/// rejects the broader `mean ≤ wire` (spring index ≤ 1) at solve time ("spring index must
/// exceed 1"), so `parse_and_solve` was already backstopped; validating here surfaces the
/// `mean ≤ 0` case at the form boundary against the field the user actually typed, and
/// stops `build_spec` — which never solves — from persisting that non-positive-radii case.
/// (The narrower `0 < mean ≤ wire` band still reaches the engine's index check, the same
/// as every other scenario whose build_spec does not re-derive a spring index.)
fn dimensional_mean_mm(wire_dia_mm: f64, outer_dia_mm: f64) -> Result<f64> {
    let mean_dia_mm = outer_dia_mm - wire_dia_mm;
    if mean_dia_mm <= 0.0 {
        return Err(springcore::SpringError::InconsistentInputs(
            "outer diameter must be greater than wire diameter".into(),
        ));
    }
    Ok(mean_dia_mm)
}

/// Resolve the form's hook mode into the optimiser's `HookSpec` (scaling Default,
/// or fixed radii). No mean diameter is needed — the optimiser varies D per candidate.
fn resolve_hooks_spec(form: &ExtFormState, us: UnitSystem) -> Result<HookSpec> {
    Ok(match form.hook_mode {
        HookMode::Default => HookSpec::Default,
        HookMode::Custom => HookSpec::Fixed {
            r1: Length::from_millimeters(length_mm("hook radius r1", &form.hook_r1, us)?),
            r2: Length::from_millimeters(length_mm("hook radius r2", &form.hook_r2, us)?),
        },
    })
}

/// Parse the comma-separated candidate-diameter list into SI millimetres, rejecting an empty list.
/// Shared by the MinWeight `parse_and_solve` and `build_spec` arms.
fn parse_candidate_diameters_mm(form: &ExtFormState, us: UnitSystem) -> Result<Vec<f64>> {
    let diameters: Vec<f64> = form
        .candidate_diameters
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| length_mm("candidate diameter", s, us))
        .collect::<Result<_>>()?;
    if diameters.is_empty() {
        return Err(springcore::SpringError::InconsistentInputs(
            "provide at least one candidate wire diameter".into(),
        ));
    }
    Ok(diameters)
}

/// Parse the form, resolve hooks, build the active scenario, and solve. The engine's
/// own input guards remain the defense-in-depth backstop.
pub fn parse_and_solve(
    form: &ExtFormState,
    material_name: &str,
    us: UnitSystem,
    materials: &MaterialStore,
    correction: CurvatureCorrection,
) -> Result<ExtFormOutcome> {
    let material: &Material = materials.get(material_name)?;
    match form.scenario {
        ExtScenarioKind::PowerUser => {
            let mean_dia_mm = length_mm("mean diameter", &form.mean_dia, us)?;
            let hooks = resolve_hooks(form, mean_dia_mm, us)?;
            let scenario = PowerUser {
                wire_dia: Length::from_millimeters(length_mm("wire diameter", &form.wire_dia, us)?),
                mean_dia: Length::from_millimeters(mean_dia_mm),
                active: positive_num("active coils", &form.active)?,
                free_length: Length::from_millimeters(length_mm(
                    "free length",
                    &form.free_length,
                    us,
                )?),
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
            Ok(ExtFormOutcome {
                design: scenario.solve(material, correction)?,
                min_weight: None,
            })
        }
        ExtScenarioKind::RateBased => {
            let mean_dia_mm = length_mm("mean diameter", &form.mean_dia, us)?;
            let hooks = resolve_hooks(form, mean_dia_mm, us)?;
            let scenario = RateBased {
                wire_dia: Length::from_millimeters(length_mm("wire diameter", &form.wire_dia, us)?),
                mean_dia: Length::from_millimeters(mean_dia_mm),
                rate: SpringRate::from_newtons_per_meter(rate_npm("spring rate", &form.rate, us)?),
                free_length: Length::from_millimeters(length_mm(
                    "free length",
                    &form.free_length,
                    us,
                )?),
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
            Ok(ExtFormOutcome {
                design: scenario.solve(material, correction)?,
                min_weight: None,
            })
        }
        ExtScenarioKind::Dimensional => {
            let wire_dia_mm = length_mm("wire diameter", &form.wire_dia, us)?;
            let outer_dia_mm = length_mm("outer diameter", &form.outer_dia, us)?;
            let mean_dia_mm = dimensional_mean_mm(wire_dia_mm, outer_dia_mm)?;
            let hooks = resolve_hooks(form, mean_dia_mm, us)?;
            let scenario = Dimensional {
                wire_dia: Length::from_millimeters(wire_dia_mm),
                outer_dia: Length::from_millimeters(outer_dia_mm),
                active: positive_num("active coils", &form.active)?,
                free_length: Length::from_millimeters(length_mm(
                    "free length",
                    &form.free_length,
                    us,
                )?),
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
            Ok(ExtFormOutcome {
                design: scenario.solve(material, correction)?,
                min_weight: None,
            })
        }
        ExtScenarioKind::TwoLoad => {
            let mean_dia_mm = length_mm("mean diameter", &form.mean_dia, us)?;
            let hooks = resolve_hooks(form, mean_dia_mm, us)?;
            let scenario = TwoLoad {
                wire_dia: Length::from_millimeters(length_mm("wire diameter", &form.wire_dia, us)?),
                mean_dia: Length::from_millimeters(mean_dia_mm),
                free_length: Length::from_millimeters(length_mm(
                    "free length",
                    &form.free_length,
                    us,
                )?),
                hooks,
                point1: (
                    Force::from_newtons(non_negative_force_n("force 1", &form.force1, us)?),
                    Length::from_millimeters(length_mm("length 1", &form.length1, us)?),
                ),
                point2: (
                    Force::from_newtons(non_negative_force_n("force 2", &form.force2, us)?),
                    Length::from_millimeters(length_mm("length 2", &form.length2, us)?),
                ),
            };
            Ok(ExtFormOutcome {
                design: scenario.solve(material, correction)?,
                min_weight: None,
            })
        }
        ExtScenarioKind::MinWeight => {
            let candidate_diameters: Vec<Length> = parse_candidate_diameters_mm(form, us)?
                .into_iter()
                .map(Length::from_millimeters)
                .collect();
            let max_outer_dia = if form.max_outer_dia.trim().is_empty() {
                None
            } else {
                Some(Length::from_millimeters(length_mm(
                    "max outer diameter",
                    &form.max_outer_dia,
                    us,
                )?))
            };
            let req = ExtMinWeightRequest {
                required_rate: SpringRate::from_newtons_per_meter(rate_npm(
                    "required rate",
                    &form.rate,
                    us,
                )?),
                max_force: Force::from_newtons(positive_force_n("max force", &form.max_force, us)?),
                initial_tension: Force::from_newtons(non_negative_force_n(
                    "initial tension",
                    &form.initial_tension,
                    us,
                )?),
                hooks: resolve_hooks_spec(form, us)?,
                index_bounds: (
                    positive_num("index min", &form.index_min)?,
                    positive_num("index max", &form.index_max)?,
                ),
                max_outer_dia,
                candidate_diameters,
            };
            let sol = solve_min_weight(material, &req, correction)?;
            Ok(ExtFormOutcome {
                design: sol.design,
                min_weight: Some(ExtMinWeightExtra {
                    binding: sol.binding,
                    mass_kg: sol.mass_kg,
                }),
            })
        }
    }
}

/// Build the persisted hook spec from the form's hook mode (shared by every scenario).
fn build_hooks_spec(form: &ExtFormState, us: UnitSystem) -> Result<HookSpecSpec> {
    Ok(match form.hook_mode {
        HookMode::Default => HookSpecSpec::Default,
        HookMode::Custom => HookSpecSpec::Custom {
            r1_mm: length_mm("hook radius r1", &form.hook_r1, us)?,
            r2_mm: length_mm("hook radius r2", &form.hook_r2, us)?,
        },
    })
}

/// Parse `form` into a persisted `ExtScenarioSpec` (SI mm/N). The caller wraps it
/// in `DesignSpec::Extension` — mirroring `compression::form::build_spec`, which
/// returns a `ScenarioSpec` the caller wraps in `DesignSpec::Compression`.
/// Round-trips with `populate_from_spec`.
pub fn build_spec(form: &ExtFormState, us: UnitSystem) -> Result<ExtScenarioSpec> {
    match form.scenario {
        ExtScenarioKind::PowerUser => {
            let mean_dia_mm = length_mm("mean diameter", &form.mean_dia, us)?;
            Ok(ExtScenarioSpec::PowerUser {
                wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
                mean_dia_mm,
                active: positive_num("active coils", &form.active)?,
                free_length_mm: length_mm("free length", &form.free_length, us)?,
                initial_tension_n: non_negative_force_n(
                    "initial tension",
                    &form.initial_tension,
                    us,
                )?,
                hooks: build_hooks_spec(form, us)?,
                loads_n: loads_n(&form.loads, us)?,
            })
        }
        ExtScenarioKind::RateBased => Ok(ExtScenarioSpec::RateBased {
            wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
            mean_dia_mm: length_mm("mean diameter", &form.mean_dia, us)?,
            rate_n_per_m: rate_npm("spring rate", &form.rate, us)?,
            free_length_mm: length_mm("free length", &form.free_length, us)?,
            initial_tension_n: non_negative_force_n("initial tension", &form.initial_tension, us)?,
            hooks: build_hooks_spec(form, us)?,
            loads_n: loads_n(&form.loads, us)?,
        }),
        ExtScenarioKind::Dimensional => {
            let wire_dia_mm = length_mm("wire diameter", &form.wire_dia, us)?;
            let outer_dia_mm = length_mm("outer diameter", &form.outer_dia, us)?;
            dimensional_mean_mm(wire_dia_mm, outer_dia_mm)?; // reject outer ≤ wire before persisting
            Ok(ExtScenarioSpec::Dimensional {
                wire_dia_mm,
                outer_dia_mm,
                active: positive_num("active coils", &form.active)?,
                free_length_mm: length_mm("free length", &form.free_length, us)?,
                initial_tension_n: non_negative_force_n(
                    "initial tension",
                    &form.initial_tension,
                    us,
                )?,
                hooks: build_hooks_spec(form, us)?,
                loads_n: loads_n(&form.loads, us)?,
            })
        }
        ExtScenarioKind::TwoLoad => Ok(ExtScenarioSpec::TwoLoad {
            wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
            mean_dia_mm: length_mm("mean diameter", &form.mean_dia, us)?,
            free_length_mm: length_mm("free length", &form.free_length, us)?,
            hooks: build_hooks_spec(form, us)?,
            force1_n: non_negative_force_n("force 1", &form.force1, us)?,
            length1_mm: length_mm("length 1", &form.length1, us)?,
            force2_n: non_negative_force_n("force 2", &form.force2, us)?,
            length2_mm: length_mm("length 2", &form.length2, us)?,
        }),
        ExtScenarioKind::MinWeight => {
            let candidate_diameters_mm = parse_candidate_diameters_mm(form, us)?;
            let max_outer_dia_mm = if form.max_outer_dia.trim().is_empty() {
                None
            } else {
                Some(length_mm("max outer diameter", &form.max_outer_dia, us)?)
            };
            Ok(ExtScenarioSpec::MinWeight {
                required_rate_n_per_m: rate_npm("required rate", &form.rate, us)?,
                max_force_n: positive_force_n("max force", &form.max_force, us)?,
                initial_tension_n: non_negative_force_n(
                    "initial tension",
                    &form.initial_tension,
                    us,
                )?,
                hooks: build_hooks_spec(form, us)?,
                index_min: positive_num("index min", &form.index_min)?,
                index_max: positive_num("index max", &form.index_max)?,
                max_outer_dia_mm,
                candidate_diameters_mm,
            })
        }
    }
}

/// Apply a persisted hook spec back onto the form (shared by every scenario).
fn apply_hooks_spec(form: &mut ExtFormState, hooks: &HookSpecSpec, us: UnitSystem) {
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

/// Write a persisted `ExtScenarioSpec` back into `form`, converting SI mm/N to display
/// units. After this call, `build_spec(form, us)` reproduces the spec.
pub fn populate_from_spec(form: &mut ExtFormState, spec: &ExtScenarioSpec, us: UnitSystem) {
    match spec {
        ExtScenarioSpec::PowerUser {
            wire_dia_mm,
            mean_dia_mm,
            active,
            free_length_mm,
            initial_tension_n,
            hooks,
            loads_n,
        } => {
            form.scenario = ExtScenarioKind::PowerUser;
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.mean_dia = fmt_len(*mean_dia_mm, us);
            form.active = format!("{active}");
            form.free_length = fmt_len(*free_length_mm, us);
            form.initial_tension = fmt_force(*initial_tension_n, us);
            form.loads = fmt_loads(loads_n, us);
            apply_hooks_spec(form, hooks, us);
        }
        ExtScenarioSpec::RateBased {
            wire_dia_mm,
            mean_dia_mm,
            rate_n_per_m,
            free_length_mm,
            initial_tension_n,
            hooks,
            loads_n,
        } => {
            form.scenario = ExtScenarioKind::RateBased;
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.mean_dia = fmt_len(*mean_dia_mm, us);
            form.rate = fmt_rate(*rate_n_per_m, us);
            form.free_length = fmt_len(*free_length_mm, us);
            form.initial_tension = fmt_force(*initial_tension_n, us);
            form.loads = fmt_loads(loads_n, us);
            apply_hooks_spec(form, hooks, us);
        }
        ExtScenarioSpec::Dimensional {
            wire_dia_mm,
            outer_dia_mm,
            active,
            free_length_mm,
            initial_tension_n,
            hooks,
            loads_n,
        } => {
            form.scenario = ExtScenarioKind::Dimensional;
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.outer_dia = fmt_len(*outer_dia_mm, us);
            form.active = format!("{active}");
            form.free_length = fmt_len(*free_length_mm, us);
            form.initial_tension = fmt_force(*initial_tension_n, us);
            form.loads = fmt_loads(loads_n, us);
            apply_hooks_spec(form, hooks, us);
        }
        ExtScenarioSpec::TwoLoad {
            wire_dia_mm,
            mean_dia_mm,
            free_length_mm,
            hooks,
            force1_n,
            length1_mm,
            force2_n,
            length2_mm,
        } => {
            form.scenario = ExtScenarioKind::TwoLoad;
            form.wire_dia = fmt_len(*wire_dia_mm, us);
            form.mean_dia = fmt_len(*mean_dia_mm, us);
            form.free_length = fmt_len(*free_length_mm, us);
            form.force1 = fmt_force(*force1_n, us);
            form.length1 = fmt_len(*length1_mm, us);
            form.force2 = fmt_force(*force2_n, us);
            form.length2 = fmt_len(*length2_mm, us);
            apply_hooks_spec(form, hooks, us);
        }
        ExtScenarioSpec::MinWeight {
            required_rate_n_per_m,
            max_force_n,
            initial_tension_n,
            hooks,
            index_min,
            index_max,
            max_outer_dia_mm,
            candidate_diameters_mm,
        } => {
            form.scenario = ExtScenarioKind::MinWeight;
            form.rate = fmt_rate(*required_rate_n_per_m, us);
            form.max_force = fmt_force(*max_force_n, us);
            form.initial_tension = fmt_force(*initial_tension_n, us);
            form.index_min = format!("{index_min}");
            form.index_max = format!("{index_max}");
            form.max_outer_dia = match max_outer_dia_mm {
                Some(v) => fmt_len(*v, us),
                None => String::new(),
            };
            form.candidate_diameters = candidate_diameters_mm
                .iter()
                .map(|&d| fmt_len(d, us))
                .collect::<Vec<_>>()
                .join(", ");
            apply_hooks_spec(form, hooks, us);
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
            ..ExtFormState::default()
        }
    }

    #[test]
    fn is_blank_true_until_any_required_field_filled() {
        let mut f = ExtFormState::default();
        assert!(f.is_blank(), "a default extension form is blank");
        // initial_tension is a required PowerUser input — filling it clears blank,
        // even though wire/mean/active are still empty.
        f.initial_tension = "5".into();
        assert!(
            !f.is_blank(),
            "filling initial tension (a required input) clears blank"
        );
    }

    #[test]
    fn is_blank_custom_hook_radius_counts_as_input() {
        let mut f = ExtFormState {
            hook_mode: HookMode::Custom,
            ..ExtFormState::default()
        };
        assert!(f.is_blank(), "an untouched Custom-hook form is blank");
        // In Custom mode a hook radius is required input, so entering one (even with
        // the core geometry still empty) must clear blank.
        f.hook_r1 = "8".into();
        assert!(
            !f.is_blank(),
            "a Custom-mode hook radius is required input; the form is no longer blank"
        );
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

    fn ratebased_metric_form() -> ExtFormState {
        ExtFormState {
            scenario: ExtScenarioKind::RateBased,
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            rate: "2".into(), // 2 N/mm
            free_length: "100".into(),
            initial_tension: "5".into(),
            loads: "10, 30".into(),
            ..ExtFormState::default()
        }
    }

    #[test]
    fn ratebased_solves_and_rate_matches_input() {
        let materials = default_materials();
        let out = parse_and_solve(
            &ratebased_metric_form(),
            default_material_name(),
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .expect("RateBased should solve");
        // The solved rate must reproduce the 2 N/mm = 2000 N/m input.
        assert_relative_eq!(out.design.rate.newtons_per_meter(), 2000.0, epsilon = 1.0);
    }

    #[test]
    fn ratebased_build_spec_populate_round_trip() {
        let us = UnitSystem::Metric;
        let form = ratebased_metric_form();
        let spec = build_spec(&form, us).unwrap();
        let mut form2 = ExtFormState::default();
        populate_from_spec(&mut form2, &spec, us);
        assert_eq!(form2.scenario, ExtScenarioKind::RateBased);
        assert_eq!(build_spec(&form2, us).unwrap(), spec);
    }

    #[test]
    fn is_blank_ratebased_trips_on_rate() {
        let mut f = ExtFormState {
            scenario: ExtScenarioKind::RateBased,
            ..ExtFormState::default()
        };
        assert!(f.is_blank(), "untouched RateBased form is blank");
        f.rate = "2".into();
        assert!(!f.is_blank(), "entering the rate clears blank");
    }

    #[test]
    fn dimensional_solves_with_outer_dia() {
        let materials = default_materials();
        let form = ExtFormState {
            scenario: ExtScenarioKind::Dimensional,
            wire_dia: "2".into(),
            outer_dia: "22".into(), // mean = 20
            active: "10".into(),
            free_length: "100".into(),
            initial_tension: "5".into(),
            loads: "10, 30".into(),
            ..ExtFormState::default()
        };
        let out = parse_and_solve(
            &form,
            default_material_name(),
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .expect("Dimensional should solve");
        // mean = OD - d = 20 mm → same geometry as the D=20 PowerUser case.
        assert_relative_eq!(out.design.outer_dia.millimeters(), 22.0, epsilon = 1e-6);
    }

    #[test]
    fn dimensional_round_trip_and_blank() {
        let us = UnitSystem::Metric;
        let form = ExtFormState {
            scenario: ExtScenarioKind::Dimensional,
            wire_dia: "2".into(),
            outer_dia: "22".into(),
            active: "10".into(),
            free_length: "100".into(),
            initial_tension: "5".into(),
            loads: "10".into(),
            ..ExtFormState::default()
        };
        let spec = build_spec(&form, us).unwrap();
        let mut form2 = ExtFormState::default();
        populate_from_spec(&mut form2, &spec, us);
        assert_eq!(form2.scenario, ExtScenarioKind::Dimensional);
        assert_eq!(build_spec(&form2, us).unwrap(), spec);

        let mut blank = ExtFormState {
            scenario: ExtScenarioKind::Dimensional,
            ..ExtFormState::default()
        };
        assert!(blank.is_blank());
        blank.outer_dia = "22".into();
        assert!(!blank.is_blank(), "Dimensional blank check uses outer_dia");
    }

    #[test]
    fn dimensional_parse_rejects_outer_not_greater_than_wire() {
        // Outer ≤ wire gives a mean diameter ≤ 0, which would feed negative default
        // hook radii into the solve. The form must reject this at its boundary with a
        // field-named "outer diameter" error — not defer to the engine's later
        // "mean diameter must be greater than wire diameter" backstop.
        let materials = default_materials();
        let form = ExtFormState {
            scenario: ExtScenarioKind::Dimensional,
            wire_dia: "5".into(),
            outer_dia: "5".into(), // mean = 0; every other field valid
            active: "10".into(),
            free_length: "100".into(),
            initial_tension: "5".into(),
            loads: "10".into(),
            ..ExtFormState::default()
        };
        let msg = parse_and_solve(
            &form,
            default_material_name(),
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .expect_err("outer ≤ wire must be rejected")
        .to_string();
        assert!(
            msg.contains("outer diameter") && msg.contains("greater than wire diameter"),
            "expected the form-boundary outer>wire error, got: {msg}"
        );
    }

    #[test]
    fn dimensional_build_spec_rejects_outer_not_greater_than_wire() {
        // build_spec must not persist an unbuildable Dimensional spec: outer ≤ wire
        // yields mean ≤ 0 (negative default hook radii), so reject it at build time
        // rather than write a spec that can never be loaded and solved.
        let us = UnitSystem::Metric;
        let form = ExtFormState {
            scenario: ExtScenarioKind::Dimensional,
            wire_dia: "5".into(),
            outer_dia: "4".into(), // mean = -1; every other field valid
            active: "10".into(),
            free_length: "100".into(),
            initial_tension: "5".into(),
            loads: "10".into(),
            ..ExtFormState::default()
        };
        let msg = build_spec(&form, us)
            .expect_err("outer ≤ wire must be rejected at build time")
            .to_string();
        assert!(
            msg.contains("outer diameter") && msg.contains("greater than wire diameter"),
            "expected the outer>wire build error, got: {msg}"
        );
    }

    #[test]
    fn dimensional_build_spec_rejects_outer_eq_wire_us() {
        // US units at the equality boundary (mean = 0): the guard subtracts AFTER the
        // ×25.4 conversion, so equal inch operands give a bit-identical SI mean of 0 and
        // must fire. Pins both the US conversion path and the mean == 0 boundary at the
        // build_spec site (the Metric tests cover mean = 0 / mean < 0 in inches' absence).
        let us = UnitSystem::Us;
        let form = ExtFormState {
            scenario: ExtScenarioKind::Dimensional,
            wire_dia: "0.2".into(),
            outer_dia: "0.2".into(), // mean = 0 in (→ 0 mm); every other field valid
            active: "10".into(),
            free_length: "4".into(),
            initial_tension: "1".into(),
            loads: "2".into(),
            ..ExtFormState::default()
        };
        let msg = build_spec(&form, us)
            .expect_err("US outer == wire must be rejected at build time")
            .to_string();
        assert!(
            msg.contains("outer diameter") && msg.contains("greater than wire diameter"),
            "expected the outer>wire build error, got: {msg}"
        );
    }

    fn twoload_metric_form() -> ExtFormState {
        // Two points 20 mm apart with a 20 N force delta → k = 1 N/mm = 1000 N/m.
        ExtFormState {
            scenario: ExtScenarioKind::TwoLoad,
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            free_length: "100".into(),
            force1: "10".into(),
            length1: "110".into(),
            force2: "30".into(),
            length2: "130".into(),
            ..ExtFormState::default()
        }
    }

    #[test]
    fn twoload_derives_rate_from_two_points() {
        let materials = default_materials();
        let out = parse_and_solve(
            &twoload_metric_form(),
            default_material_name(),
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .expect("TwoLoad should solve");
        // k = (30-10)/(130-110) mm = 1 N/mm = 1000 N/m.
        assert_relative_eq!(out.design.rate.newtons_per_meter(), 1000.0, epsilon = 1.0);
    }

    #[test]
    fn twoload_round_trip_and_blank_ignores_initial_tension() {
        let us = UnitSystem::Metric;
        let form = twoload_metric_form();
        let spec = build_spec(&form, us).unwrap();
        let mut form2 = ExtFormState::default();
        populate_from_spec(&mut form2, &spec, us);
        assert_eq!(form2.scenario, ExtScenarioKind::TwoLoad);
        assert_eq!(build_spec(&form2, us).unwrap(), spec);

        // initial_tension is NOT a TwoLoad input — filling it must not clear blank.
        let mut blank = ExtFormState {
            scenario: ExtScenarioKind::TwoLoad,
            initial_tension: "5".into(),
            ..ExtFormState::default()
        };
        assert!(blank.is_blank(), "initial tension is not a TwoLoad input");
        blank.force1 = "10".into();
        assert!(!blank.is_blank(), "a load point clears blank");
    }

    /// `build_spec` → extract `ExtScenarioSpec` → `populate_from_spec` → `build_spec` again
    /// must produce identical specs. Verified for both Default and Custom hook modes.
    #[test]
    fn build_spec_populate_round_trip() {
        let us = UnitSystem::Metric;

        // Default hooks
        let form = metric_form();
        let spec1 = build_spec(&form, us).unwrap();
        let mut form2 = ExtFormState::default();
        populate_from_spec(&mut form2, &spec1, us);
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
        let mut form4 = ExtFormState::default();
        populate_from_spec(&mut form4, &spec3, us);
        let spec4 = build_spec(&form4, us).unwrap();
        assert_eq!(spec3, spec4, "custom hooks: round-trip must be lossless");
    }

    fn minweight_metric_form() -> ExtFormState {
        ExtFormState {
            scenario: ExtScenarioKind::MinWeight,
            rate: "2".into(), // 2 N/mm required rate
            max_force: "50".into(),
            initial_tension: "5".into(),
            candidate_diameters: "1.5, 2.0, 2.5".into(),
            ..ExtFormState::default() // index_min="4", index_max="12" by default
        }
    }

    #[test]
    fn minweight_solves_with_binding_and_positive_mass() {
        let materials = default_materials();
        let out = parse_and_solve(
            &minweight_metric_form(),
            default_material_name(),
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .expect("MinWeight should solve");
        let mw = out.min_weight.expect("MinWeight path sets min_weight");
        assert!(mw.mass_kg > 0.0, "optimised wire mass is positive");
    }

    #[test]
    fn minweight_empty_candidates_errors() {
        let materials = default_materials();
        let form = ExtFormState {
            candidate_diameters: String::new(),
            ..minweight_metric_form()
        };
        assert!(parse_and_solve(
            &form,
            default_material_name(),
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .is_err());
    }

    #[test]
    fn minweight_round_trip_and_blank_ignores_prefilled_defaults() {
        let us = UnitSystem::Metric;
        let spec = build_spec(&minweight_metric_form(), us).unwrap();
        let mut form2 = ExtFormState::default();
        populate_from_spec(&mut form2, &spec, us);
        assert_eq!(form2.scenario, ExtScenarioKind::MinWeight);
        assert_eq!(build_spec(&form2, us).unwrap(), spec);

        // A default MinWeight form (index_min/max pre-filled) is still blank.
        let f = ExtFormState {
            scenario: ExtScenarioKind::MinWeight,
            ..ExtFormState::default()
        };
        assert!(
            f.is_blank(),
            "pre-filled index defaults do not count as input"
        );
    }

    /// MinWeight renders `initial_tension` and `max_outer_dia` as inputs with no
    /// pre-filled default, so typing either must clear blank and surface parse
    /// feedback — the other scenarios include every displayed input in their blank
    /// check, and these two were the only displayed MinWeight inputs omitted
    /// (`index_min`/`index_max` stay out because they ARE pre-filled defaults).
    #[test]
    fn minweight_blank_trips_on_initial_tension_or_max_outer_dia() {
        let only_initial_tension = ExtFormState {
            scenario: ExtScenarioKind::MinWeight,
            initial_tension: "5".into(),
            ..ExtFormState::default()
        };
        assert!(
            !only_initial_tension.is_blank(),
            "entering initial tension clears blank (it is a MinWeight input)"
        );

        let only_max_outer_dia = ExtFormState {
            scenario: ExtScenarioKind::MinWeight,
            max_outer_dia: "20".into(),
            ..ExtFormState::default()
        };
        assert!(
            !only_max_outer_dia.is_blank(),
            "entering a max outer diameter clears blank (it is a MinWeight input)"
        );
    }

    /// MinWeight `max_force` must be strictly positive: it is the design force the
    /// optimiser sizes against, so `0` is meaningless. The form boundary must reject
    /// it directly — matching compression's MinWeight form and the solver's own
    /// validation — rather than deferring to the engine with a misleading
    /// "zero or greater" message. `build_spec` does not call the solver, so this
    /// test pins the form-helper boundary, not the engine fallback.
    #[test]
    fn minweight_zero_max_force_rejected_at_form_boundary() {
        let us = UnitSystem::Metric;
        let form = ExtFormState {
            max_force: "0".into(),
            ..minweight_metric_form()
        };
        let msg = build_spec(&form, us)
            .expect_err("zero max force must be rejected at the form boundary")
            .to_string();
        assert!(
            msg.contains("max force") && msg.contains("greater than zero"),
            "expected field-named strictly-positive error, got: {msg}"
        );
    }

    /// Companion to the `build_spec` boundary test, pinning the OTHER call site:
    /// `parse_and_solve` must also reject `max_force = 0` at the form helper, BEFORE
    /// the solver. The engine independently rejects 0 with a different message
    /// ("positive finite number"), so asserting the form-layer "greater than zero"
    /// message proves the rejection came from `positive_force_n` here — catching a
    /// revert of this site that the engine's own guard would otherwise mask.
    #[test]
    fn minweight_zero_max_force_rejected_in_parse_and_solve() {
        let materials = default_materials();
        let form = ExtFormState {
            max_force: "0".into(),
            ..minweight_metric_form()
        };
        let msg = parse_and_solve(
            &form,
            default_material_name(),
            UnitSystem::Metric,
            &materials,
            CurvatureCorrection::default(),
        )
        .expect_err("zero max force must be rejected before the solver")
        .to_string();
        assert!(
            msg.contains("max force") && msg.contains("greater than zero"),
            "expected the form-helper error, not the engine fallback, got: {msg}"
        );
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
        let default_spec = build_spec(&metric_form(), us).unwrap();
        populate_from_spec(&mut form, &default_spec, us);

        assert_eq!(
            form.hook_mode,
            HookMode::Default,
            "mode must switch to Default"
        );
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
