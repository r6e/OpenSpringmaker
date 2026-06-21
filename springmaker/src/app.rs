//! Application state, messages, and update/view glue for the iced GUI.

use crate::form::{format_error, parse_and_solve, FormOutcome, FormState, ScenarioKind};
use crate::view;
use iced::theme::Palette;
use iced::{Color, Theme};
use springcore::{LoadWarning, MaterialStore, SavedDesign, UnitSystem};

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
    Field(Field, String),
    Material(String),
    Scenario(ScenarioKind),
    Units(UnitSystem),
    EndType(String),
    Fixity(String),
    Save,
    Load,
}

/// Top-level application state.
pub struct App {
    pub form: FormState,
    pub materials: MaterialStore,
    pub load_warnings: Vec<LoadWarning>,
    pub outcome: Option<FormOutcome>,
    pub error: Option<String>,
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
        };
        if should_recompute {
            self.recompute();
        }
    }

    /// Render the current application state as an iced element.
    pub fn view(&self) -> iced::Element<'_, Message> {
        view::view(self)
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
}
