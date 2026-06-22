//! Application state, messages, and update/view glue for the iced GUI.

use crate::form::{format_error, parse_and_solve, FormOutcome, FormState, ScenarioKind};
use crate::materials_form::{build_draft, populate_from_material, MaterialsFormState};
use iced::theme::Palette;
use iced::{Color, Theme};
use springcore::{LoadWarning, MaterialStore, MtsForm, SavedDesign, StrengthUnits, UnitSystem};

// --------------------------------------------------------------------------
// Design tokens — single source of truth for colours used in view.rs
// --------------------------------------------------------------------------

/// Global colour constants for the engineering-instrument palette.
pub struct C;

impl C {
    /// App background — near-black ink.
    pub const INK: Color = Color {
        r: 0.055,
        g: 0.067,
        b: 0.086,
        a: 1.0,
    };
    /// Card/panel surface.
    pub const PANEL: Color = Color {
        r: 0.090,
        g: 0.110,
        b: 0.141,
        a: 1.0,
    };
    /// Raised input field surface.
    pub const RAISED: Color = Color {
        r: 0.122,
        g: 0.149,
        b: 0.188,
        a: 1.0,
    };
    /// Hairline border / divider.
    pub const LINE: Color = Color {
        r: 0.165,
        g: 0.196,
        b: 0.239,
        a: 1.0,
    };
    /// Primary text.
    pub const TEXT: Color = Color {
        r: 0.902,
        g: 0.918,
        b: 0.941,
        a: 1.0,
    };
    /// Muted / secondary labels.
    pub const MUTED: Color = Color {
        r: 0.541,
        g: 0.592,
        b: 0.651,
        a: 1.0,
    };
    /// Accent — active controls, focus, governing result.
    pub const ACCENT: Color = Color {
        r: 0.298,
        g: 0.761,
        b: 1.0,
        a: 1.0,
    };
    /// Caution / warning indicator.
    pub const WARN: Color = Color {
        r: 0.949,
        g: 0.710,
        b: 0.227,
        a: 1.0,
    };
    /// Danger / error indicator.
    pub const DANGER: Color = Color {
        r: 1.0,
        g: 0.420,
        b: 0.420,
        a: 1.0,
    };
    /// Success / healthy indicator.
    pub const SUCCESS: Color = Color {
        r: 0.31,
        g: 0.78,
        b: 0.47,
        a: 1.0,
    };
}

// --------------------------------------------------------------------------
// Screen routing
// --------------------------------------------------------------------------

/// Top-level navigation screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Calculator,
    Materials,
}

// --------------------------------------------------------------------------
// Materials editor state types
// --------------------------------------------------------------------------

/// Whether the editor is creating a new material or editing an existing one.
#[derive(Debug, Clone, PartialEq)]
pub enum EditTarget {
    New,
    Existing(String),
}

/// Which text field a [`Message::MatField`] targets in the material editor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MatField {
    Name,
    Spec,
    Citations,
    Coefficients,
    ValidDiaMin,
    ValidDiaMax,
    Youngs,
    Shear,
    Density,
    AllowTorsion,
    AllowBending,
    AllowSet,
    EnduranceSsa,
    EnduranceSsm,
    MaxTemp,
}

// --------------------------------------------------------------------------
// Calculator field enum
// --------------------------------------------------------------------------

/// Which text field a [`Message::Field`] targets.
#[derive(Debug, Clone, Copy)]
pub enum Field {
    WireDia,
    MeanDia,
    OuterDia,
    Active,
    FreeLength,
    Rate,
    Loads,
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

/// All UI events.
#[derive(Debug, Clone)]
pub enum Message {
    // Calculator screen
    Field(Field, String),
    Material(String),
    Scenario(ScenarioKind),
    Units(UnitSystem),
    EndType(String),
    Fixity(String),
    Save,
    Load,
    // Navigation and materials-editor variants.
    NavigateTo(Screen),
    MatField(MatField, String),
    MatFormKind(MtsForm),
    MatUnits(StrengthUnits),
    MatToggleEndurance(bool),
    MatTogglePeened(bool),
    MatToggleMaxTemp(bool),
    MatNew,
    MatClone(String),
    MatEdit(String),
    MatCommit,
    MatCancel,
    MatDelete(String),
    MatPersist,
}

/// Top-level application state.
pub struct App {
    pub form: FormState,
    pub materials: MaterialStore,
    pub load_warnings: Vec<LoadWarning>,
    pub outcome: Option<FormOutcome>,
    pub error: Option<String>,
    // Screen routing
    pub screen: Screen,
    // Materials editor
    pub mat_form: MaterialsFormState,
    pub editing: Option<EditTarget>,
    pub mat_error: Option<String>,
    pub mat_status: Option<String>,
}

impl Default for App {
    fn default() -> Self {
        let (materials, load_warnings) = MaterialStore::load();
        Self {
            form: FormState::default(),
            materials,
            load_warnings,
            outcome: None,
            error: None,
            screen: Screen::Calculator,
            mat_form: MaterialsFormState::default(),
            editing: None,
            mat_error: None,
            mat_status: None,
        }
    }
}

impl App {
    /// Re-solve from the current form, storing either an outcome or an error string.
    pub fn recompute(&mut self) {
        match parse_and_solve(&self.form, &self.materials) {
            Ok(out) => {
                self.outcome = Some(out);
                self.error = None;
            }
            Err(e) => {
                self.outcome = None;
                self.error = Some(format_error(&e, self.form.unit_system));
            }
        }
    }

    /// Process a UI event, updating state and re-solving the design where needed.
    pub fn update(&mut self, message: Message) {
        let should_recompute = match message {
            Message::Field(field, value) => {
                self.set_field(field, value);
                true
            }
            Message::Material(m) => {
                self.form.material = m;
                true
            }
            Message::Scenario(s) => {
                self.form.scenario = s;
                true
            }
            Message::Units(u) => {
                self.form.unit_system = u;
                true
            }
            Message::EndType(e) => {
                self.form.end_type = e;
                true
            }
            Message::Fixity(f) => {
                self.form.fixity = f;
                true
            }
            // Save never mutates the form; preserve any error from the dialog.
            Message::Save => {
                self.save_dialog();
                false
            }
            // Load recomputes only on success (apply_saved mutates the form).
            Message::Load => self.load_dialog(),

            // ── Navigation ──────────────────────────────────────────────────
            Message::NavigateTo(s) => {
                self.screen = s;
                self.mat_error = None;
                self.mat_status = None;
                // Returning to the calculator: re-solve in case the selected
                // material was edited in the editor (stale outcome otherwise).
                matches!(s, Screen::Calculator)
            }

            // ── Materials editor ─────────────────────────────────────────────
            Message::MatField(f, v) => {
                self.set_mat_field(f, v);
                self.mat_error = None;
                false
            }
            Message::MatFormKind(k) => {
                self.mat_form.mts_form = k;
                self.mat_error = None;
                false
            }
            Message::MatUnits(u) => {
                self.mat_form.mts_units = u;
                self.mat_error = None;
                false
            }
            Message::MatToggleEndurance(b) => {
                self.mat_form.has_endurance = b;
                self.mat_error = None;
                false
            }
            Message::MatTogglePeened(b) => {
                self.mat_form.endurance_peened = b;
                self.mat_error = None;
                false
            }
            Message::MatToggleMaxTemp(b) => {
                self.mat_form.has_max_temp = b;
                self.mat_error = None;
                false
            }
            Message::MatNew => {
                self.mat_form = MaterialsFormState::default();
                self.editing = Some(EditTarget::New);
                self.mat_error = None;
                self.mat_status = None;
                false
            }
            Message::MatEdit(name) => {
                if self.materials.is_curated(&name) {
                    self.mat_error = Some("curated materials are read-only".into());
                } else {
                    match self.materials.get(&name) {
                        Ok(m) => {
                            populate_from_material(&mut self.mat_form, m);
                            self.editing = Some(EditTarget::Existing(name));
                            self.mat_error = None;
                            self.mat_status = None;
                        }
                        Err(e) => self.mat_error = Some(format!("{e}")),
                    }
                }
                false
            }
            Message::MatClone(name) => {
                // Clone adds the "(copy)" immediately and opens it for editing
                // (an instant copy you then refine); cancelling leaves the copy,
                // which the user can Remove — unlike New, which adds only on commit.
                match self.materials.clone_material(&name) {
                    Ok(copy) => {
                        let copy_name = copy.name.clone();
                        match self.materials.add(copy) {
                            Ok(()) => match self.materials.get(&copy_name) {
                                Ok(m) => {
                                    populate_from_material(&mut self.mat_form, m);
                                    self.editing = Some(EditTarget::Existing(copy_name));
                                    self.mat_status = Some("cloned".into());
                                    self.mat_error = None;
                                }
                                Err(e) => self.mat_error = Some(format!("{e}")),
                            },
                            Err(e) => self.mat_error = Some(format!("{e}")),
                        }
                    }
                    Err(e) => self.mat_error = Some(format!("{e}")),
                }
                false
            }
            Message::MatCommit => {
                match build_draft(&self.mat_form).and_then(|d| d.build()) {
                    Ok(m) => {
                        let res = match &self.editing {
                            Some(EditTarget::New) => self.materials.add(m),
                            Some(EditTarget::Existing(orig)) => {
                                let orig = orig.clone();
                                self.materials.update(&orig, m)
                            }
                            None => return,
                        };
                        match res {
                            Ok(()) => {
                                self.editing = None;
                                self.mat_error = None;
                                self.mat_status = Some("saved entry".into());
                            }
                            Err(e) => self.mat_error = Some(format!("{e}")),
                        }
                    }
                    Err(e) => self.mat_error = Some(format!("{e}")),
                }
                false
            }
            Message::MatCancel => {
                self.editing = None;
                self.mat_error = None;
                self.mat_status = None;
                false
            }
            Message::MatDelete(name) => {
                match self.materials.remove(&name) {
                    Ok(()) => {
                        self.mat_error = None;
                        self.mat_status = Some(format!("deleted '{name}'"));
                        // Close the editor if it was editing the deleted material.
                        if matches!(&self.editing, Some(EditTarget::Existing(n)) if *n == name) {
                            self.editing = None;
                        }
                        // If the calculator had it selected, fall back to a valid
                        // remaining material so navigating back doesn't error.
                        if self.form.material == name {
                            if let Some(first) =
                                self.materials.names().first().map(|s| s.to_string())
                            {
                                self.form.material = first;
                            }
                        }
                    }
                    Err(e) => self.mat_error = Some(format!("{e}")),
                }
                false
            }
            Message::MatPersist => {
                match self.materials.save() {
                    Ok(()) => self.mat_status = Some("saved to disk".into()),
                    Err(e) => self.mat_error = Some(format!("{e}")),
                }
                false
            }
        };
        if should_recompute {
            self.recompute();
        }
    }

    /// Render the current application state as an iced element.
    pub fn view(&self) -> iced::Element<'_, Message> {
        match self.screen {
            Screen::Calculator => crate::view::view(self),
            Screen::Materials => crate::materials_view::view(self),
        }
    }

    /// Supply the custom dark theme to the iced application builder.
    pub fn theme(&self) -> Theme {
        Theme::custom(
            "OpenSpringmaker".to_string(),
            Palette {
                background: C::INK,
                text: C::TEXT,
                primary: C::ACCENT,
                success: C::SUCCESS,
                danger: C::DANGER,
            },
        )
    }

    fn set_field(&mut self, field: Field, value: String) {
        let f = &mut self.form;
        match field {
            Field::WireDia => f.wire_dia = value,
            Field::MeanDia => f.mean_dia = value,
            Field::OuterDia => f.outer_dia = value,
            Field::Active => f.active = value,
            Field::FreeLength => f.free_length = value,
            Field::Rate => f.rate = value,
            Field::Loads => f.loads = value,
            Field::Force1 => f.force1 = value,
            Field::Length1 => f.length1 = value,
            Field::Force2 => f.force2 = value,
            Field::Length2 => f.length2 = value,
            Field::FatigueMin => f.fatigue_min = value,
            Field::FatigueMax => f.fatigue_max = value,
            Field::MaxForce => f.max_force = value,
            Field::IndexMin => f.index_min = value,
            Field::IndexMax => f.index_max = value,
            Field::MaxOuterDia => f.max_outer_dia = value,
            Field::CandidateDiameters => f.candidate_diameters = value,
            Field::ClashAllowance => f.clash_allowance = value,
        }
    }

    fn set_mat_field(&mut self, field: MatField, value: String) {
        let f = &mut self.mat_form;
        match field {
            MatField::Name => f.name = value,
            MatField::Spec => f.specification = value,
            MatField::Citations => f.citations = value,
            MatField::Coefficients => f.coefficients = value,
            MatField::ValidDiaMin => f.valid_dia_min = value,
            MatField::ValidDiaMax => f.valid_dia_max = value,
            MatField::Youngs => f.youngs_modulus = value,
            MatField::Shear => f.shear_modulus = value,
            MatField::Density => f.density = value,
            MatField::AllowTorsion => f.allowable_torsion = value,
            MatField::AllowBending => f.allowable_bending = value,
            MatField::AllowSet => f.allowable_set = value,
            MatField::EnduranceSsa => f.endurance_ssa = value,
            MatField::EnduranceSsm => f.endurance_ssm = value,
            MatField::MaxTemp => f.max_temp_c = value,
        }
    }

    fn save_dialog(&mut self) {
        let spec = match crate::form::build_spec(&self.form) {
            Ok(s) => s,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("design", &["toml"])
            .save_file()
        {
            let saved = SavedDesign {
                material: self.form.material.clone(),
                unit_system: self.form.unit_system,
                scenario: spec,
            };
            if let Err(e) = saved.save(&path) {
                self.error = Some(e.to_string());
            }
        }
    }

    /// Returns `true` if the form was mutated (successful load), `false` otherwise.
    fn load_dialog(&mut self) -> bool {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("design", &["toml"])
            .pick_file()
        {
            match SavedDesign::load(&path) {
                Ok(saved) => {
                    self.apply_saved(saved);
                    return true;
                }
                Err(e) => {
                    self.error = Some(e.to_string());
                }
            }
        }
        false
    }

    fn apply_saved(&mut self, saved: SavedDesign) {
        self.form.material = saved.material;
        self.form.unit_system = saved.unit_system;
        crate::form::populate_from_spec(&mut self.form, &saved.scenario);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An `App` with a curated-only store (no on-disk user overlay), so the
    /// materials-CRUD tests are hermetic regardless of the developer's saved
    /// materials. `App::default()` loads the real overlay from the OS config dir.
    fn test_app() -> App {
        App {
            materials: MaterialStore::new(springcore::MaterialSet::load_default()),
            ..App::default()
        }
    }

    #[test]
    fn default_app_has_no_outcome_until_filled() {
        let app = App::default();
        assert!(app.outcome.is_none());
        assert_eq!(app.form.material, "Music Wire");
    }

    #[test]
    fn default_app_loads_material_store_with_curated() {
        let app = App::default();
        assert!(app.materials.names().contains(&"Music Wire"));
    }

    #[test]
    fn recompute_produces_outcome_for_valid_form() {
        let mut app = App::default();
        app.form.scenario = crate::form::ScenarioKind::RateBased;
        app.form.wire_dia = "2.0".into();
        app.form.mean_dia = "20.0".into();
        app.form.rate = "2.0".into(); // 2 N/mm = 2000 N/m (internal)
        app.form.free_length = "60".into();
        app.form.loads = "10, 30".into();
        app.recompute();
        assert!(app.error.is_none());
        assert!(app.outcome.is_some());
    }

    #[test]
    fn recompute_sets_error_for_invalid_form() {
        let mut app = App::default();
        app.form.scenario = crate::form::ScenarioKind::RateBased;
        app.form.wire_dia = "oops".into();
        app.recompute();
        assert!(app.outcome.is_none());
        assert!(app.error.is_some());
    }

    // ── Materials CRUD tests ──────────────────────────────────────────────────

    fn fill_valid_power_law(app: &mut App) {
        app.update(Message::MatField(MatField::Name, "New Wire".into()));
        app.update(Message::MatField(MatField::Spec, "x".into()));
        app.update(Message::MatField(MatField::Citations, "x".into()));
        app.update(Message::MatField(
            MatField::Coefficients,
            "2000, 0.15".into(),
        ));
        app.update(Message::MatField(MatField::ValidDiaMin, "0.5".into()));
        app.update(Message::MatField(MatField::ValidDiaMax, "6".into()));
        app.update(Message::MatField(MatField::Youngs, "200".into()));
        app.update(Message::MatField(MatField::Shear, "79".into()));
        app.update(Message::MatField(MatField::Density, "7850".into()));
        app.update(Message::MatField(MatField::AllowTorsion, "0.45".into()));
        app.update(Message::MatField(MatField::AllowBending, "0.75".into()));
        app.update(Message::MatField(MatField::AllowSet, "0.6".into()));
    }

    /// The user material the editor opened after a clone (deterministic,
    /// regardless of any pre-existing user overlay).
    fn editing_name(a: &App) -> String {
        match &a.editing {
            Some(EditTarget::Existing(n)) => n.clone(),
            other => panic!("expected an Existing edit target, got {other:?}"),
        }
    }

    #[test]
    fn add_user_material_via_messages() {
        let mut a = test_app();
        a.update(Message::MatNew);
        fill_valid_power_law(&mut a);
        a.update(Message::MatCommit);
        assert!(a.mat_error.is_none());
        assert!(a.materials.names().contains(&"New Wire"));
        assert!(!a.materials.is_curated("New Wire"));
        assert!(a.editing.is_none());
    }

    #[test]
    fn commit_invalid_sets_error_not_panic() {
        let mut a = test_app();
        a.update(Message::MatNew);
        // power_law needs 2 coefficients; supply only 1
        a.update(Message::MatField(MatField::Coefficients, "2000".into()));
        a.update(Message::MatCommit);
        assert!(a.mat_error.is_some());
        // The editor stays open after a failed commit so the user can fix input.
        assert!(a.editing.is_some());
    }

    #[test]
    fn editing_curated_is_rejected() {
        let mut a = test_app();
        a.update(Message::MatEdit("Music Wire".into()));
        assert!(a.mat_error.is_some());
        assert!(a.editing.is_none());
    }

    #[test]
    fn delete_curated_is_rejected() {
        let mut a = test_app();
        a.update(Message::MatDelete("Music Wire".into()));
        assert!(a.mat_error.is_some());
        assert!(a.materials.names().contains(&"Music Wire"));
    }

    #[test]
    fn clone_creates_user_copy() {
        let mut a = test_app();
        a.update(Message::MatClone("Music Wire".into()));
        assert!(a.materials.names().iter().any(|n| n.contains("(copy)")));
    }

    #[test]
    fn navigate_switches_screen() {
        let mut a = test_app();
        a.update(Message::NavigateTo(Screen::Materials));
        assert_eq!(a.screen, Screen::Materials);
    }

    #[test]
    fn navigate_clears_materials_feedback() {
        let mut a = test_app();
        a.update(Message::MatDelete("Music Wire".into())); // sets mat_error (curated)
        assert!(a.mat_error.is_some());
        a.update(Message::NavigateTo(Screen::Calculator));
        assert!(a.mat_error.is_none() && a.mat_status.is_none());
    }

    #[test]
    fn edit_then_commit_updates_user_material() {
        let mut a = test_app();
        a.update(Message::MatClone("Music Wire".into()));
        let copy_name = editing_name(&a);
        a.update(Message::MatEdit(copy_name.clone()));
        a.update(Message::MatField(MatField::Density, "8000".into()));
        a.update(Message::MatCommit);
        assert!(a.mat_error.is_none());
        assert!(a.editing.is_none()); // editor closes on success
                                      // The edited value is persisted in the store.
        let updated = a.materials.get(&copy_name).unwrap();
        assert!((updated.density.kg_per_m3() - 8000.0).abs() < 1e-6);
    }

    #[test]
    fn deleting_active_and_selected_material_resets_state() {
        let mut a = test_app();
        a.update(Message::MatClone("Music Wire".into()));
        let copy_name = editing_name(&a); // editor is open on the clone
        a.form.material = copy_name.clone(); // calculator selects it too
        a.update(Message::MatDelete(copy_name.clone()));
        assert!(a.mat_error.is_none());
        assert!(!a.materials.names().contains(&copy_name.as_str()));
        // Editor closed and calculator selection moved to a valid material.
        assert!(a.editing.is_none());
        assert_ne!(a.form.material, copy_name);
        assert!(a.materials.names().contains(&a.form.material.as_str()));
    }
}
