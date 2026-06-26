//! Application state, messages, and update/view glue for the iced GUI.

use crate::form::{format_error, parse_and_solve, FormOutcome, FormState, ScenarioKind};
use crate::materials_form::{build_draft, populate_from_material, MaterialsFormState};
use iced::theme::Palette;
use iced::{Color, Theme};
use springcore::{
    CurvatureCorrection, LoadWarning, MaterialStore, MtsForm, SavedDesign, StrengthUnits,
    UnitSystem,
};

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
    /// Settings screen — curvature-correction preference.
    Settings,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    // Settings screen: emitted by the correction radio group in settings_view.
    SetCorrection(CurvatureCorrection),
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
    /// Solve error: set/cleared by [`App::recompute`] as an exclusive pair with
    /// `outcome` (a present `outcome` means the solve succeeded). Surfaced in the
    /// results panel only when `outcome` is `None`.
    pub error: Option<String>,
    /// Save/load action error. Independent of `outcome`/`error` so a failed save
    /// or load is surfaced (in the status panel) without wiping the design the
    /// user is looking at. Cleared on the next save/load attempt and on recompute.
    pub action_error: Option<String>,
    // Screen routing
    pub screen: Screen,
    // Materials editor
    pub mat_form: MaterialsFormState,
    pub editing: Option<EditTarget>,
    pub mat_error: Option<String>,
    pub mat_status: Option<String>,
    /// Curvature-correction factor applied to all solve paths; persisted via
    /// [`crate::settings::AppSettings`].
    pub correction: CurvatureCorrection,
    /// Path to persist settings on [`Message::SetCorrection`]. `None` means do
    /// not write to the filesystem (all test-constructed apps use `None`).
    pub settings_path: Option<std::path::PathBuf>,
    /// Last settings-save error, if any. Separate from `action_error` because
    /// `recompute()` clears `action_error` but must not clobber this status.
    pub settings_error: Option<String>,
}

impl App {
    /// Build an `App` around a given store.
    ///
    /// `correction` is injected by the caller so that `from_store` performs no
    /// filesystem I/O; the running app passes the value loaded from
    /// [`crate::settings::AppSettings`], while tests pass a known hermetic value.
    pub(crate) fn from_store(
        materials: MaterialStore,
        load_warnings: Vec<LoadWarning>,
        correction: CurvatureCorrection,
    ) -> Self {
        Self {
            form: FormState::default(),
            materials,
            load_warnings,
            outcome: None,
            error: None,
            action_error: None,
            screen: Screen::Calculator,
            mat_form: MaterialsFormState::default(),
            editing: None,
            mat_error: None,
            mat_status: None,
            correction,
            settings_path: None,
            settings_error: None,
        }
    }
}

impl Default for App {
    fn default() -> Self {
        let (materials, load_warnings) = MaterialStore::load();
        Self::from_store(materials, load_warnings, CurvatureCorrection::default())
    }
}

impl App {
    /// Re-solve from the current form, storing either an outcome or an error string.
    pub fn recompute(&mut self) {
        // A form edit (or successful load / return to the calculator) dismisses a
        // stale save/load error.
        self.action_error = None;
        match parse_and_solve(&self.form, &self.materials, self.correction) {
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

    /// Set a materials-editor error and clear any stale success status, so a
    /// prior "saved"/"cloned" message can't linger after a failed action.
    fn set_mat_error(&mut self, msg: impl Into<String>) {
        self.mat_error = Some(msg.into());
        self.mat_status = None;
    }

    /// Set a materials-editor success status and clear any stale error, so a
    /// prior error can't linger after a successful action (the view shows error
    /// over status).
    fn set_mat_status(&mut self, msg: impl Into<String>) {
        self.mat_status = Some(msg.into());
        self.mat_error = None;
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
            // Save never mutates the form, so it must not recompute — that would
            // clear the `action_error` a failed save just set.
            Message::Save => {
                self.save_dialog();
                false
            }
            // Load recomputes only on success (apply_saved mutates the form).
            Message::Load => self.load_dialog(),

            // ── Settings ────────────────────────────────────────────────────
            Message::SetCorrection(c) => {
                self.correction = c;
                // Persist only when a real settings path is configured (None in all
                // test-constructed apps, so tests never touch the real filesystem).
                let save_result = self.settings_path.as_ref().map(|p| {
                    crate::settings::AppSettings {
                        curvature_correction: c,
                    }
                    .save_to(p)
                });
                match save_result {
                    Some(Err(e)) => self.settings_error = Some(e.to_string()),
                    // Ok(()) or no path configured: clear any stale error.
                    _ => self.settings_error = None,
                }
                true
            }

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
                    self.set_mat_error("curated materials are read-only");
                } else {
                    match self.materials.get(&name) {
                        Ok(m) => {
                            populate_from_material(&mut self.mat_form, m);
                            self.editing = Some(EditTarget::Existing(name));
                            self.mat_error = None;
                            self.mat_status = None;
                        }
                        Err(e) => self.set_mat_error(format!("{e}")),
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
                                    self.set_mat_status("cloned");
                                }
                                Err(e) => self.set_mat_error(format!("{e}")),
                            },
                            Err(e) => self.set_mat_error(format!("{e}")),
                        }
                    }
                    Err(e) => self.set_mat_error(format!("{e}")),
                }
                false
            }
            Message::MatCommit => {
                match build_draft(&self.mat_form).and_then(|d| d.build()) {
                    Ok(m) => {
                        let new_name = m.name.clone();
                        let target = self.editing.clone();
                        let res = match &target {
                            Some(EditTarget::New) => self.materials.add(m),
                            Some(EditTarget::Existing(orig)) => self.materials.update(orig, m),
                            None => return,
                        };
                        match res {
                            Ok(()) => {
                                // If editing renamed the material the calculator had
                                // selected, follow the rename so the selection stays valid.
                                if let Some(EditTarget::Existing(orig)) = &target {
                                    if self.form.material == *orig && new_name != *orig {
                                        self.form.material = new_name;
                                    }
                                }
                                self.editing = None;
                                self.set_mat_status("saved entry");
                            }
                            Err(e) => self.set_mat_error(format!("{e}")),
                        }
                    }
                    Err(e) => self.set_mat_error(format!("{e}")),
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
                        self.set_mat_status(format!("deleted '{name}'"));
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
                    Err(e) => self.set_mat_error(format!("{e}")),
                }
                false
            }
            Message::MatPersist => {
                match self.materials.save() {
                    Ok(()) => self.set_mat_status("saved to disk"),
                    Err(e) => self.set_mat_error(format!("{e}")),
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
            Screen::Settings => crate::settings_view::view(self),
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
                warning: C::WARN,
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
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("design", &["toml"])
            .save_file()
        {
            self.save_to(&path);
        }
    }

    /// Build and write the current design to `path`, recording any failure in
    /// `action_error` (not `error`) so a failed save leaves the displayed design
    /// intact. The leading clear makes a successful save dismiss a prior failure.
    fn save_to(&mut self, path: &std::path::Path) {
        self.action_error = None;
        let spec = match crate::form::build_spec(&self.form) {
            Ok(s) => s,
            Err(e) => {
                self.action_error = Some(e.to_string());
                return;
            }
        };
        let saved = SavedDesign {
            material: self.form.material.clone(),
            unit_system: self.form.unit_system,
            scenario: spec,
        };
        if let Err(e) = saved.save(path) {
            self.action_error = Some(e.to_string());
        }
    }

    /// Returns `true` if the form was mutated (successful load), `false` otherwise.
    fn load_dialog(&mut self) -> bool {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("design", &["toml"])
            .pick_file()
        {
            return self.load_from(&path);
        }
        false
    }

    /// Load a design from `path` into the form. On failure, records it in
    /// `action_error` and returns `false`, leaving the current form untouched.
    /// Returns `true` (form mutated) on success.
    fn load_from(&mut self, path: &std::path::Path) -> bool {
        self.action_error = None;
        match SavedDesign::load(path) {
            Ok(saved) => {
                self.apply_saved(saved);
                true
            }
            Err(e) => {
                self.action_error = Some(e.to_string());
                false
            }
        }
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
        // No filesystem IO: a curated-only store, no on-disk user overlay, and a
        // fixed hermetic correction value (Bergsträsser) rather than reading settings.
        App::from_store(
            MaterialStore::new(springcore::MaterialSet::load_default()),
            Vec::new(),
            springcore::CurvatureCorrection::Bergstrasser,
        )
    }

    /// An unwritable path (embedded NUL) that causes save to fail deterministically
    /// without touching the real filesystem.
    fn unwritable_settings_path() -> std::path::PathBuf {
        std::path::PathBuf::from("invalid\0settings.toml")
    }

    #[test]
    fn set_correction_without_settings_path_does_not_write_fs() {
        // settings_path is None in all test-constructed apps → no FS access.
        let mut app = test_app();
        assert!(
            app.settings_path.is_none(),
            "test apps must be non-persisting"
        );
        app.update(Message::SetCorrection(
            springcore::CurvatureCorrection::Wahl,
        ));
        // In-memory preference updated.
        assert_eq!(app.correction, springcore::CurvatureCorrection::Wahl);
        // No save attempted → no error surfaced.
        assert!(app.settings_error.is_none());
    }

    #[test]
    fn set_correction_surfaces_save_error_and_still_updates_correction() {
        let mut app = test_app();
        // Point to an unwritable path so the save attempt fails deterministically.
        app.settings_path = Some(unwritable_settings_path());
        app.update(Message::SetCorrection(
            springcore::CurvatureCorrection::Wahl,
        ));
        // In-memory preference still updated even on write failure.
        assert_eq!(app.correction, springcore::CurvatureCorrection::Wahl);
        // The error is surfaced via the dedicated channel (not action_error,
        // which recompute() clears).
        assert!(
            app.settings_error.is_some(),
            "settings-save failure must be surfaced"
        );
    }

    #[test]
    fn set_correction_clears_stale_save_error_on_success() {
        let mut app = test_app();
        // Prime a stale error, then fire without a settings path (no save → clear).
        app.settings_error = Some("stale error".into());
        app.update(Message::SetCorrection(
            springcore::CurvatureCorrection::Bergstrasser,
        ));
        assert!(
            app.settings_error.is_none(),
            "a no-save SetCorrection must clear a stale settings error"
        );
    }

    #[test]
    fn changing_correction_recomputes_with_new_factor() {
        let mut app = App::from_store(
            MaterialStore::new(springcore::MaterialSet::load_default()),
            Vec::new(),
            springcore::CurvatureCorrection::Bergstrasser,
        );
        // PowerUser design with spring index C = mean_dia / wire_dia = 20 / 2 = 10.
        app.form.scenario = crate::form::ScenarioKind::PowerUser;
        app.form.wire_dia = "2.0".into();
        app.form.mean_dia = "20.0".into();
        app.form.active = "10".into();
        app.form.free_length = "60".into();
        app.form.loads = "10, 30".into();
        app.update(Message::SetCorrection(
            springcore::CurvatureCorrection::Bergstrasser,
        ));
        let berg = app.outcome.as_ref().unwrap().design.load_points[0]
            .shear_stress
            .pascals();
        app.update(Message::SetCorrection(
            springcore::CurvatureCorrection::Wahl,
        ));
        let wahl = app.outcome.as_ref().unwrap().design.load_points[0]
            .shear_stress
            .pascals();
        assert!(wahl > berg, "Wahl factor exceeds Bergsträsser at C=10");
        assert_eq!(app.correction, springcore::CurvatureCorrection::Wahl);
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

    /// A hermetic app with a valid rate-based design already solved.
    fn solved_app() -> App {
        let mut app = test_app();
        app.form.scenario = crate::form::ScenarioKind::RateBased;
        app.form.wire_dia = "2.0".into();
        app.form.mean_dia = "20.0".into();
        app.form.rate = "2.0".into();
        app.form.free_length = "60".into();
        app.form.loads = "10, 30".into();
        app.recompute();
        assert!(app.outcome.is_some(), "fixture should solve");
        app
    }

    /// A path every OS rejects (embedded NUL), so save/load IO fails
    /// deterministically without touching the real filesystem.
    fn unwritable_path() -> std::path::PathBuf {
        std::path::PathBuf::from("invalid\0path.toml")
    }

    #[test]
    fn save_failure_surfaces_action_error_and_preserves_outcome() {
        let mut app = solved_app();
        app.save_to(&unwritable_path());
        // The displayed design is untouched — a failed save must not wipe it.
        assert!(
            app.outcome.is_some(),
            "a failed save must not clear results"
        );
        assert!(
            app.error.is_none(),
            "the solve-error channel is not used for IO failures"
        );
        // ...and the failure is recorded for the status panel.
        assert!(app.action_error.is_some(), "save failure must be surfaced");
    }

    #[test]
    fn load_failure_surfaces_action_error_and_preserves_form() {
        let mut app = solved_app();
        let before_material = app.form.material.clone();
        let mutated = app.load_from(&unwritable_path());
        assert!(!mutated, "a failed load reports no form mutation");
        assert!(app.action_error.is_some(), "load failure must be surfaced");
        assert!(
            app.outcome.is_some(),
            "a failed load must not clear results"
        );
        assert_eq!(
            app.form.material, before_material,
            "the form is untouched on a failed load"
        );
    }

    #[test]
    fn recompute_clears_stale_action_error() {
        let mut app = solved_app();
        app.action_error = Some("stale save failure".into());
        app.recompute(); // stands in for a subsequent form edit
        assert!(
            app.action_error.is_none(),
            "editing the form dismisses a stale action error"
        );
    }

    #[test]
    fn successful_save_clears_a_prior_action_error() {
        let mut app = solved_app();
        app.action_error = Some("a previous save failure".into());
        // A genuine successful write (unique temp path) must dismiss the prior
        // banner via the leading clear — Save never recomputes, so nothing else
        // would clear it.
        let path = std::env::temp_dir().join(format!("osm_save_ok_{}.toml", std::process::id()));
        app.save_to(&path);
        assert!(
            app.action_error.is_none(),
            "a successful save dismisses a stale action error"
        );
        assert!(path.exists(), "the design was actually written");
        let _ = std::fs::remove_file(&path);
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

    #[test]
    fn successful_action_clears_stale_error() {
        // The view prioritizes mat_error over mat_status, so a successful action
        // must clear a prior error (regression guard for MatPersist/clone/etc).
        let mut a = test_app();
        a.mat_error = Some("stale error from a prior failed action".into());
        a.update(Message::MatClone("Music Wire".into())); // succeeds, in-memory
        assert!(a.mat_error.is_none(), "success must clear a stale error");
        assert!(a.mat_status.is_some());
    }

    #[test]
    fn renaming_selected_material_follows_the_rename() {
        let mut a = test_app();
        a.update(Message::MatClone("Music Wire".into()));
        let orig = editing_name(&a); // the editor is open on the clone
        a.form.material = orig.clone(); // calculator selects it
                                        // Rename it via the editor and commit.
        a.update(Message::MatField(MatField::Name, "Renamed Wire".into()));
        a.update(Message::MatCommit);
        assert!(a.mat_error.is_none());
        assert!(a.materials.names().contains(&"Renamed Wire"));
        assert!(!a.materials.names().contains(&orig.as_str()));
        // The calculator selection followed the rename (no stale MaterialNotFound).
        assert_eq!(a.form.material, "Renamed Wire");
    }

    // ── Cross-state invariants ──────────────────────────────────────────────
    //
    // The class of bugs surfaced in review (delete/rename of the edited or
    // calculator-selected material; stale error/status) all violate one of two
    // invariants that must hold after EVERY `update`. Driving a representative
    // message sequence and checking after each step turns "did we think of case
    // X?" into "the invariant fails on any unhandled case".

    /// INV1: the calculator's selected material always names one that exists in
    /// the store (holds for `Message::Material` carrying a valid name, which the
    /// view's pick list guarantees — it only offers names from the store). INV2:
    /// editor error and success status are never shown together (the view
    /// prioritizes error, so a lingering error would mask a success).
    fn assert_editor_invariants(a: &App) {
        assert!(
            a.materials.names().contains(&a.form.material.as_str()),
            "INV1 violated: form.material '{}' is not in the store",
            a.form.material
        );
        assert!(
            !(a.mat_error.is_some() && a.mat_status.is_some()),
            "INV2 violated: mat_error and mat_status are both set"
        );
    }

    #[test]
    fn editor_message_sequence_preserves_invariants() {
        let mut a = test_app();
        assert_editor_invariants(&a);

        macro_rules! step {
            ($msg:expr) => {{
                a.update($msg);
                assert_editor_invariants(&a);
            }};
        }

        step!(Message::NavigateTo(Screen::Materials));
        step!(Message::MatClone("Music Wire".into())); // adds "(copy)", opens editor
        let copy = editing_name(&a);
        step!(Message::Material(copy.clone())); // calculator selects the user copy
        step!(Message::MatEdit(copy.clone()));
        step!(Message::MatField(MatField::Name, "Renamed".into()));
        step!(Message::MatCommit); // rename the SELECTED material -> selection must follow
        step!(Message::Material("Renamed".into()));
        step!(Message::MatDelete("Renamed".into())); // delete the SELECTED material -> fallback
        step!(Message::MatNew);
        step!(Message::MatField(
            MatField::Coefficients,
            "not-a-number".into()
        ));
        step!(Message::MatCommit); // invalid -> error set, status must be clear
        step!(Message::MatCancel);
        step!(Message::NavigateTo(Screen::Calculator));
    }
}
