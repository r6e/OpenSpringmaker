//! Application state, messages, and update/view glue for the iced GUI.

use crate::compression::form::{parse_and_solve, Field, FormOutcome, FormState, ScenarioKind};
use crate::conical::form::{ConFormOutcome, ConFormState};
use crate::extension::form::{ExtFormOutcome, ExtFormState};
use crate::form_helpers::format_error;
use crate::materials_form::{build_draft, populate_from_material, MaterialsFormState};
use crate::settings::ThemePref;
use iced::{Color, Theme};
use springcore::{
    CurvatureCorrection, Family, LoadWarning, MaterialStore, MtsForm, SavedDesign, StrengthUnits,
    UnitSystem,
};

// --------------------------------------------------------------------------
// Design tokens — single source of truth for colours used in view.rs
// --------------------------------------------------------------------------

/// One resolved color palette. PR 2 adds `LIGHT`; views resolve the active
/// palette once per view build via [`App::pal`] and pass it down — theme
/// switches re-run `view()`, so build-time resolution stays correct.
pub struct Palette {
    /// App background — near-black ink.
    pub ink: Color,
    /// Card/panel surface.
    pub panel: Color,
    /// Raised input field surface.
    pub raised: Color,
    /// Hairline border / divider.
    pub line: Color,
    /// Primary text.
    pub text: Color,
    /// Muted / secondary labels.
    pub muted: Color,
    /// Accent — active controls, focus, governing result.
    pub accent: Color,
    /// Selected-option background tint. Palette-owned: a ×0.15 dark-tint is a
    /// dark-theme assumption; LIGHT defines its own pale tint.
    pub accent_tint: Color,
    /// Hovered-option background. Palette-owned: +0.05 lightens on dark; LIGHT
    /// darkens instead.
    pub hover: Color,
    /// Caution / warning indicator.
    pub warn: Color,
    /// Danger / error indicator.
    pub danger: Color,
    /// Success / healthy indicator.
    pub success: Color,
}

/// The engineering-instrument dark palette (the shipped identity).
///
/// `static`, not `const`: `resolved_palette`/`pal`/`theme` identify the active
/// palette by `std::ptr::eq` against `&DARK`/`&LIGHT`. A `const`'s value is
/// copied at each use site, so two `&DARK` expressions in different places
/// are not guaranteed to share an address; a `static` has exactly one
/// program-wide location, which `ptr::eq` can rely on.
pub static DARK: Palette = Palette {
    ink: Color {
        r: 0.055,
        g: 0.067,
        b: 0.086,
        a: 1.0,
    },
    panel: Color {
        r: 0.090,
        g: 0.110,
        b: 0.141,
        a: 1.0,
    },
    raised: Color {
        r: 0.122,
        g: 0.149,
        b: 0.188,
        a: 1.0,
    },
    line: Color {
        r: 0.165,
        g: 0.196,
        b: 0.239,
        a: 1.0,
    },
    text: Color {
        r: 0.902,
        g: 0.918,
        b: 0.941,
        a: 1.0,
    },
    muted: Color {
        r: 0.541,
        g: 0.592,
        b: 0.651,
        a: 1.0,
    },
    accent: Color {
        r: 0.298,
        g: 0.761,
        b: 1.0,
        a: 1.0,
    },
    accent_tint: Color {
        r: 0.298 * 0.15,
        g: 0.761 * 0.15,
        b: 1.0 * 0.15,
        a: 1.0,
    },
    hover: Color {
        r: 0.122 + 0.05,
        g: 0.149 + 0.05,
        b: 0.188 + 0.05,
        a: 1.0,
    },
    warn: Color {
        r: 0.949,
        g: 0.710,
        b: 0.227,
        a: 1.0,
    },
    danger: Color {
        r: 1.0,
        g: 0.420,
        b: 0.420,
        a: 1.0,
    },
    success: Color {
        r: 0.31,
        g: 0.78,
        b: 0.47,
        a: 1.0,
    },
};

/// The paper-white light palette — the dark theme's mirror, not an inversion.
/// `static` for the same address-identity reason as [`DARK`].
pub static LIGHT: Palette = Palette {
    ink: Color {
        r: 0.965,
        g: 0.960,
        b: 0.950,
        a: 1.0,
    },
    panel: Color {
        r: 0.925,
        g: 0.920,
        b: 0.908,
        a: 1.0,
    },
    raised: Color {
        r: 0.885,
        g: 0.880,
        b: 0.868,
        a: 1.0,
    },
    line: Color {
        r: 0.780,
        g: 0.775,
        b: 0.760,
        a: 1.0,
    },
    text: Color {
        r: 0.100,
        g: 0.110,
        b: 0.130,
        a: 1.0,
    },
    muted: Color {
        r: 0.320,
        g: 0.340,
        b: 0.380,
        a: 1.0,
    },
    accent: Color {
        r: 0.000,
        g: 0.350,
        b: 0.620,
        a: 1.0,
    },
    warn: Color {
        r: 0.520,
        g: 0.340,
        b: 0.000,
        a: 1.0,
    },
    danger: Color {
        r: 0.760,
        g: 0.100,
        b: 0.100,
        a: 1.0,
    },
    success: Color {
        r: 0.050,
        g: 0.450,
        b: 0.220,
        a: 1.0,
    },
    accent_tint: Color {
        r: 0.850,
        g: 0.910,
        b: 0.970,
        a: 1.0,
    },
    hover: Color {
        r: 0.885 - 0.05,
        g: 0.880 - 0.05,
        b: 0.868 - 0.05,
        a: 1.0,
    },
};

/// The palette for a pref × OS-mode pair. `Mode::None` (OS reported nothing)
/// resolves to DARK — the shipped identity.
fn resolved_palette(pref: ThemePref, system: iced::theme::Mode) -> &'static Palette {
    match pref {
        ThemePref::Dark => &DARK,
        ThemePref::Light => &LIGHT,
        ThemePref::System => match system {
            iced::theme::Mode::Light => &LIGHT,
            iced::theme::Mode::Dark | iced::theme::Mode::None => &DARK,
        },
    }
}

// --------------------------------------------------------------------------
// Screen routing
// --------------------------------------------------------------------------

/// Top-level navigation screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Calculator,
    Materials,
    /// Settings screen — curvature-correction and theme preferences.
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
    AllowEndTorsion,
    AllowBending,
    AllowSet,
    EnduranceSsa,
    EnduranceSsm,
    MaxTemp,
}

/// Which visual occupies the results panel's shared slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VisualMode {
    #[default]
    Chart,
    Spring3d,
    Diagram,
}

/// All UI events.
#[derive(Debug, Clone)]
pub enum Message {
    // Calculator screen — compression
    CompField(Field, String),
    Material(String),
    Scenario(ScenarioKind),
    Units(UnitSystem),
    EndType(String),
    Fixity(String),
    Save,
    Load,
    // Calculator screen — family selector
    SelectFamily(Family),
    // Calculator screen — extension
    ExtField(crate::extension::form::Field, String),
    ExtHookMode(crate::extension::form::HookMode),
    ExtScenario(crate::extension::form::ExtScenarioKind),
    // Calculator screen — torsion
    TorField(crate::torsion::form::Field, String),
    TorFriction(springcore::torsion::FrictionModel),
    TorScenario(crate::torsion::form::TorScenarioKind),
    TorMomentEntry(crate::torsion::form::MomentEntry),
    TorDiaPolicy(springcore::torsion::DiaPolicy),
    TorCycleLife(springcore::torsion::CycleLife),
    // Calculator screen — conical
    ConField(crate::conical::form::Field, String),
    ConEndType(String),
    // Calculator screen — assembly
    AsmTopology(String),
    AsmFixity(String),
    AsmLoads(String),
    AsmField(usize, crate::assembly::form::MemberField, String),
    AsmMemberMaterial(usize, String),
    AsmMemberEndType(usize, String),
    AsmMemberAdd,
    AsmMemberRemove(usize),
    // Results panel — 3D visualization (shared across families; orbit and
    // visual-mode choice persist across family tabs, unlike per-family form
    // state). `Orbit` carries a per-drag-event delta (dx, dy) in pixels,
    // published by `OrbitCanvas::update`: publishing the raw delta (rather
    // than an orbit computed against the canvas's own possibly-stale
    // `Orbit`) means the single accumulation point is `App::update`, so
    // coalesced drag events compose instead of dropping intermediate steps.
    // `Visual` is constructed by the shared `widgets::visual_toggle` segmented
    // control used by every family (compression, conical, extension, torsion,
    // assembly).
    Orbit(f32, f32),
    // Shaded-3D zoom: `shader3d::SpringShader::update` publishes this from
    // a wheel-scroll event on the shader widget, mirroring `Orbit`'s
    // "publish the raw delta, accumulate in App::update" discipline — the
    // single accumulation point is the `Message::Zoom` arm below, via
    // `crate::viz::zoom_step`.
    Zoom(f32),
    Visual(VisualMode),
    /// 2D-diagram wheel-zoom delta (published by `DiagramCanvas::update`),
    /// accumulated by the `DiagramZoom` arm via `diagram::zoom_step`.
    DiagramZoom(f32),
    /// 2D-diagram drag-pan delta (dx, dy) in px, accumulated via `diagram::pan_step`.
    DiagramPan(f32, f32),
    /// Toggle one 2D-diagram dimension layer.
    #[allow(dead_code)] // constructed by the layer-toggle row in Task 5
    DiagramLayer(crate::diagram::DimLayer),
    // Settings screen: emitted by the correction option buttons in settings_view.
    SetCorrection(CurvatureCorrection),
    // Settings screen: theme preference (System/Light/Dark) picker.
    // Constructed by settings_view's theme options (Task 4).
    ThemePref(ThemePref),
    // Emitted when the OS reports (or changes) its light/dark preference;
    // only affects rendering when `theme_pref` is `System`. Constructed by
    // the OS-theme subscription (`App::subscription`, Task 5).
    SystemTheme(iced::theme::Mode),
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
    /// Active spring family (Compression | Extension | Torsion).
    pub family: Family,
    pub form: FormState,
    /// Extension PowerUser form inputs.
    pub extension: ExtFormState,
    /// Solved extension outcome; `None` until a valid extension form is solved.
    pub ext_outcome: Option<ExtFormOutcome>,
    /// Torsion PowerUser form inputs.
    pub torsion: crate::torsion::form::TorFormState,
    /// Solved torsion outcome; `None` until a valid torsion form is solved.
    pub tor_outcome: Option<crate::torsion::form::TorFormOutcome>,
    /// Conical PowerUser form inputs.
    pub conical: ConFormState,
    /// Solved conical outcome; `None` until a valid conical form is solved.
    pub con_outcome: Option<ConFormOutcome>,
    /// Assembly form inputs (dynamic member list).
    pub assembly: crate::assembly::form::AsmFormState,
    /// Solved assembly outcome; `None` until a valid assembly form is solved.
    pub asm_outcome: Option<springcore::assembly::AssemblyDesign>,
    /// Selected material name (shared across families). Lifted out of `FormState`.
    pub material: String,
    /// Active unit system (shared across families). Lifted out of `FormState`.
    pub unit_system: UnitSystem,
    pub materials: MaterialStore,
    pub load_warnings: Vec<LoadWarning>,
    pub outcome: Option<FormOutcome>,
    /// Solve error for the active family: set/cleared by [`App::recompute`].
    /// Exclusive with that family's outcome field — a present outcome means the
    /// solve succeeded and `error` is `None`. Shared by all four families
    /// (Compression, Extension, Torsion, Conical). Surfaced in the results panel
    /// only when the active family's outcome is `None`.
    pub error: Option<String>,
    /// Save/load action error. Independent of `outcome`/`error` so a failed save
    /// or load is surfaced (in the status panel) without wiping the design the
    /// user is looking at. Cleared on the next save/load attempt and on recompute.
    pub action_error: Option<String>,
    /// Committed 3D orbit angles for the results panel's `Spring3d` visual.
    /// Shared across families so the orientation follows the user.
    pub orbit: crate::viz::Orbit,
    /// Committed multiplicative zoom for the shaded 3D camera (1.0 = the
    /// fit-to-extent framing; clamped to `viz::ZOOM_MIN..=ZOOM_MAX`). The
    /// `Message::Zoom` arm is the single writer, via `viz::zoom_step`, so
    /// its non-finite guard keeps this finite by induction — the same
    /// argument as `orbit`. Shared across families like `orbit`.
    pub zoom: f32,
    /// Whether a GPU adapter exists for the shaded 3D path (`main.rs`'s
    /// boot-time `shader_probe` sets it once, mirroring how `settings_path`
    /// and `theme_pref` are wired post-construction). `from_store` ALWAYS
    /// defaults this to `false`, for two load-bearing reasons: (a) probing
    /// is machine-dependent, so a probing constructor would make every
    /// headless-Simulator test's widget tree vary with the host GPU; (b)
    /// HARD RULE — no snapshot test may ever hash a render of an app with
    /// `shader_available = true` (GPU `Shader` pixels are adapter/driver-
    /// specific; see `ui_tests::snapshot_hash`'s doc). Defaulting false
    /// keeps the entire suite on the deterministic CPU wireframe path.
    pub shader_available: bool,
    /// Which visual (chart or 3D) occupies the results panel's shared slot.
    pub results_visual: VisualMode,
    /// Committed 2D-diagram view transform (zoom/pan). The `DiagramZoom`/
    /// `DiagramPan` arms are the single writers, via `diagram::zoom_step`/
    /// `pan_step`, mirroring `orbit`/`zoom` above.
    pub diagram_view: crate::diagram::DiagramView,
    /// Which 2D-diagram dimension layers are shown; toggled per-layer by
    /// `Message::DiagramLayer`.
    pub diagram_layers: crate::diagram::DimLayers,
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
    /// Theme preference (System/Light/Dark); persisted via
    /// [`crate::settings::AppSettings`]. `from_store` always defaults this to
    /// [`ThemePref::default`] — `main.rs`'s `initial_app` assigns the loaded
    /// value afterward, mirroring how `settings_path` is wired post-construction.
    pub theme_pref: ThemePref,
    /// The OS-reported light/dark mode, as last observed. `Mode::None` (no
    /// report yet) resolves to DARK — see [`resolved_palette`].
    pub system_mode: iced::theme::Mode,
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
            family: Family::default(),
            form: FormState::default(),
            extension: ExtFormState::default(),
            ext_outcome: None,
            torsion: crate::torsion::form::TorFormState::default(),
            tor_outcome: None,
            conical: ConFormState::default(),
            con_outcome: None,
            assembly: crate::assembly::form::AsmFormState::with_default_material("Music Wire"),
            asm_outcome: None,
            material: "Music Wire".into(),
            unit_system: UnitSystem::Metric,
            materials,
            load_warnings,
            outcome: None,
            error: None,
            action_error: None,
            orbit: crate::viz::Orbit::default(),
            zoom: 1.0,
            shader_available: false,
            results_visual: VisualMode::default(),
            diagram_view: crate::diagram::DiagramView::default(),
            diagram_layers: crate::diagram::DimLayers::default(),
            screen: Screen::Calculator,
            mat_form: MaterialsFormState::default(),
            editing: None,
            mat_error: None,
            mat_status: None,
            correction,
            settings_path: None,
            settings_error: None,
            theme_pref: ThemePref::default(),
            system_mode: iced::theme::Mode::default(),
        }
    }
}

impl Default for App {
    fn default() -> Self {
        let (materials, load_warnings) = MaterialStore::load();
        Self::from_store(materials, load_warnings, CurvatureCorrection::default())
    }
}

/// Assign `value` to `slot` only if it differs; returns whether it changed.
/// Mirrors the update contract: `false` ⇒ no mutation ⇒ no recompute ⇒
/// `action_error` preserved (the pick_list no-op guard).
fn set_if_changed<T: PartialEq>(slot: &mut T, value: T) -> bool {
    let changed = *slot != value;
    if changed {
        *slot = value;
    }
    changed
}

impl App {
    /// Re-solve from the current form, storing either an outcome or an error string.
    pub fn recompute(&mut self) {
        // A form edit (or successful load / return to the calculator) dismisses a
        // stale save/load error.
        self.action_error = None;
        match self.family {
            Family::Compression => {
                // Stale extension/torsion/conical/assembly outcomes from a prior solve are no longer active.
                self.ext_outcome = None;
                self.tor_outcome = None;
                self.con_outcome = None;
                self.asm_outcome = None;
                // If the user has entered none of the active scenario's required
                // inputs (e.g. switched families on an untouched form), treat this
                // as the initial state rather than surfacing a parse error. Once any
                // required field is filled the form is no longer blank and parse
                // feedback shows — see `FormState::is_blank`.
                if self.form.is_blank() {
                    self.error = None;
                    self.outcome = None;
                    return;
                }
                match parse_and_solve(
                    &self.form,
                    &self.material,
                    self.unit_system,
                    &self.materials,
                    self.correction,
                ) {
                    Ok(out) => {
                        self.outcome = Some(out);
                        self.error = None;
                    }
                    Err(e) => {
                        self.outcome = None;
                        self.error = Some(format_error(&e, self.unit_system));
                    }
                }
            }
            Family::Extension => {
                // Stale compression/torsion/conical/assembly outcomes from a prior solve are no longer active.
                self.outcome = None;
                self.tor_outcome = None;
                self.con_outcome = None;
                self.asm_outcome = None;
                // If the user has entered none of the PowerUser required inputs
                // (e.g. switched families on an untouched form), treat this as the
                // initial state rather than surfacing a parse error. Once any
                // required field is filled, parse feedback shows — see
                // `ExtFormState::is_blank`.
                if self.extension.is_blank() {
                    self.error = None;
                    self.ext_outcome = None;
                    return;
                }
                match crate::extension::form::parse_and_solve(
                    &self.extension,
                    &self.material,
                    self.unit_system,
                    &self.materials,
                    self.correction,
                ) {
                    Ok(out) => {
                        self.ext_outcome = Some(out);
                        self.error = None;
                    }
                    Err(e) => {
                        self.ext_outcome = None;
                        self.error = Some(format_error(&e, self.unit_system));
                    }
                }
            }
            Family::Torsion => {
                // Stale compression/extension/conical/assembly outcomes from a prior solve are no longer active.
                self.outcome = None;
                self.ext_outcome = None;
                self.con_outcome = None;
                self.asm_outcome = None;
                if self.torsion.is_blank() {
                    self.error = None;
                    self.tor_outcome = None;
                    return;
                }
                match crate::torsion::form::parse_and_solve(
                    &self.torsion,
                    &self.material,
                    self.unit_system,
                    &self.materials,
                ) {
                    Ok(out) => {
                        self.tor_outcome = Some(out);
                        self.error = None;
                    }
                    Err(e) => {
                        self.tor_outcome = None;
                        self.error = Some(format_error(&e, self.unit_system));
                    }
                }
            }
            Family::Conical => {
                // Stale compression/extension/torsion/assembly outcomes from a prior solve are no longer active.
                self.outcome = None;
                self.ext_outcome = None;
                self.tor_outcome = None;
                self.asm_outcome = None;
                if self.conical.is_blank() {
                    self.error = None;
                    self.con_outcome = None;
                    return;
                }
                match crate::conical::form::parse_and_solve(
                    &self.conical,
                    &self.material,
                    self.unit_system,
                    &self.materials,
                    self.correction,
                ) {
                    Ok(out) => {
                        self.con_outcome = Some(out);
                        self.error = None;
                    }
                    Err(e) => {
                        self.con_outcome = None;
                        self.error = Some(format_error(&e, self.unit_system));
                    }
                }
            }
            Family::Assembly => {
                // Stale outcomes from other families are no longer active.
                self.outcome = None;
                self.ext_outcome = None;
                self.tor_outcome = None;
                self.con_outcome = None;
                if self.assembly.is_blank() {
                    self.error = None;
                    self.asm_outcome = None;
                    return;
                }
                match crate::assembly::form::parse_and_solve(
                    &self.assembly,
                    self.unit_system,
                    &self.materials,
                    self.correction,
                ) {
                    Ok(out) => {
                        self.asm_outcome = Some(out);
                        self.error = None;
                    }
                    Err(e) => {
                        self.asm_outcome = None;
                        self.error = Some(format_error(&e, self.unit_system));
                    }
                }
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

    /// Persist the current preferences (`correction` + `theme_pref`) to
    /// `settings_path`, surfacing any write failure via `settings_error`.
    /// Shared by every preference message so there is exactly one save path —
    /// a save attempted from one preference's message always carries the
    /// other's current value along with it.
    fn persist_settings(&mut self) {
        // Persist only when a real settings path is configured (None in all
        // test-constructed apps, so tests never touch the real filesystem).
        let save_result = self.settings_path.as_ref().map(|p| {
            crate::settings::AppSettings {
                curvature_correction: self.correction,
                theme_pref: self.theme_pref,
            }
            .save_to(p)
        });
        match save_result {
            Some(Err(e)) => self.settings_error = Some(format!("could not save settings: {e}")),
            // Ok(()) or no path configured: clear any stale error.
            _ => self.settings_error = None,
        }
    }

    /// Process a UI event, updating state and re-solving the design where needed.
    pub fn update(&mut self, message: Message) {
        let should_recompute = match message {
            Message::CompField(field, value) => {
                self.set_field(field, value);
                true
            }
            Message::SelectFamily(fam) => {
                self.family = fam;
                true
            }
            Message::ExtField(f, v) => {
                self.set_ext_field(f, v);
                true
            }
            Message::ExtHookMode(m) => {
                self.extension.hook_mode = m;
                true
            }
            // A pick_list dispatches `on_select` unconditionally even when
            // re-selecting the current value (iced's pick_list.rs), so this
            // must not blindly recompute — that would clear a pending
            // `action_error` though nothing changed. `set_if_changed` is the
            // shared no-op guard for every pick_list-backed handler below.
            Message::ExtScenario(s) => set_if_changed(&mut self.extension.scenario, s),
            Message::TorField(f, v) => {
                self.set_tor_field(f, v);
                true
            }
            // Same pick_list no-op contract as `ExtScenario` above for the
            // remaining torsion pick_list handlers.
            Message::TorFriction(m) => set_if_changed(&mut self.torsion.friction_model, m),
            Message::TorScenario(s) => set_if_changed(&mut self.torsion.scenario, s),
            Message::TorMomentEntry(m) => set_if_changed(&mut self.torsion.moment_entry, m),
            Message::TorDiaPolicy(p) => set_if_changed(&mut self.torsion.dia_policy, p),
            Message::TorCycleLife(l) => set_if_changed(&mut self.torsion.cycle_life, l),
            Message::ConField(f, v) => {
                self.set_con_field(f, v);
                true
            }
            // Same pick_list no-op contract as `ExtScenario` above.
            Message::ConEndType(e) => set_if_changed(&mut self.conical.end_type, e),
            Message::AsmTopology(t) => set_if_changed(&mut self.assembly.topology, t),
            Message::AsmFixity(f) => set_if_changed(&mut self.assembly.fixity, f),
            Message::AsmLoads(v) => {
                self.assembly.loads = v;
                true
            }
            Message::AsmField(i, field, v) => {
                // Return whether a field was actually written: an out-of-bounds
                // index (not UI-reachable, but defensively possible via direct
                // message dispatch) is a no-op and must NOT trigger a recompute
                // that would clear `action_error`.
                if let Some(m) = self.assembly.members.get_mut(i) {
                    use crate::assembly::form::MemberField as F;
                    match field {
                        F::WireDia => m.wire_dia = v,
                        F::MeanDia => m.mean_dia = v,
                        F::Active => m.active = v,
                        F::FreeLength => m.free_length = v,
                    }
                    true
                } else {
                    false
                }
            }
            // Bounds-checked per the `AsmField` precedent above, AND value-checked
            // per the pick_list no-op contract (`ExtScenario` above): re-selecting
            // a member's CURRENT material/end type is also a no-op.
            Message::AsmMemberMaterial(i, mat) => self
                .assembly
                .members
                .get_mut(i)
                .is_some_and(|m| set_if_changed(&mut m.material, mat)),
            Message::AsmMemberEndType(i, et) => self
                .assembly
                .members
                .get_mut(i)
                .is_some_and(|m| set_if_changed(&mut m.end_type, et)),
            Message::AsmMemberAdd => {
                self.assembly
                    .members
                    .push(crate::assembly::form::AsmMemberForm::blank(&self.material));
                true
            }
            Message::AsmMemberRemove(i) => {
                // A no-op removal (min-one floor or out-of-bounds index) must not
                // recompute — that would clear `action_error` though nothing changed.
                let len = self.assembly.members.len();
                if len > 1 && i < len {
                    self.assembly.members.remove(i);
                    true
                } else {
                    false
                }
            }
            // Same pick_list no-op contract as `ExtScenario` above.
            Message::Material(m) => set_if_changed(&mut self.material, m),
            Message::Scenario(s) => set_if_changed(&mut self.form.scenario, s),
            Message::Units(u) => {
                self.unit_system = u;
                true
            }
            Message::EndType(e) => set_if_changed(&mut self.form.end_type, e),
            Message::Fixity(f) => set_if_changed(&mut self.form.fixity, f),
            // Save never mutates the form, so it must not recompute — that would
            // clear the `action_error` a failed save just set.
            Message::Save => {
                self.save_dialog();
                false
            }
            // Load recomputes only on success (apply_saved mutates the form).
            Message::Load => self.load_dialog(),

            // ── Results panel: 3D visualization ────────────────────────────
            // Orbiting and switching visuals touch neither the solved outcome
            // nor `action_error` — same non-recompute shape as `Message::Save`.
            // `orbit_step` is the single consumer of a raw drag delta, so its
            // non-finite guard keeps `self.orbit` finite by induction — no
            // other code path can write to `self.orbit`.
            Message::Orbit(dx, dy) => {
                self.orbit = crate::viz::orbit_step(self.orbit, dx, dy);
                false
            }
            // Same non-recompute shape as `Orbit`: `zoom_step` is the single
            // consumer of a raw wheel delta, so its non-finite guard and
            // clamp keep `self.zoom` finite and in-bounds by induction — no
            // other code path can write to `self.zoom`.
            Message::Zoom(delta) => {
                self.zoom = crate::viz::zoom_step(self.zoom, delta);
                false
            }
            Message::Visual(v) => {
                self.results_visual = v;
                false
            }
            // Same non-recompute shape as `Zoom`/`Orbit`: `zoom_step`/`pan_step`
            // are the single writers (finiteness-guarded), so the view stays valid
            // by induction. Layer toggles are pure view state.
            Message::DiagramZoom(delta) => {
                self.diagram_view = crate::diagram::zoom_step(self.diagram_view, delta);
                false
            }
            Message::DiagramPan(dx, dy) => {
                self.diagram_view = crate::diagram::pan_step(self.diagram_view, dx, dy);
                false
            }
            Message::DiagramLayer(layer) => {
                let l = &mut self.diagram_layers;
                match layer {
                    crate::diagram::DimLayer::Lengths => l.lengths = !l.lengths,
                    crate::diagram::DimLayer::Diameters => l.diameters = !l.diameters,
                    crate::diagram::DimLayer::Coils => l.coils = !l.coils,
                }
                false
            }

            // ── Settings ────────────────────────────────────────────────────
            Message::SetCorrection(c) => {
                self.correction = c;
                self.persist_settings();
                true
            }
            Message::ThemePref(p) => {
                // Deliberate parity with SetCorrection: the VM's `clickable`
                // flag is the single owner of when a same-value click can
                // happen (the retry case), so this arm always writes and saves.
                self.theme_pref = p;
                self.persist_settings();
                false
            }
            Message::SystemTheme(mode) => {
                self.system_mode = mode;
                false
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
            // Same pick_list no-op contract as `ExtScenario` above, applied to
            // `mat_error` rather than `action_error`: re-selecting the
            // materials-editor pick_list's CURRENT value must not clear a
            // pending error, but a genuine change still must (deliberate
            // policy for real edits, unchanged).
            Message::MatFormKind(k) => {
                if set_if_changed(&mut self.mat_form.mts_form, k) {
                    self.mat_error = None;
                }
                false
            }
            Message::MatUnits(u) => {
                if set_if_changed(&mut self.mat_form.mts_units, u) {
                    self.mat_error = None;
                }
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
                                    if self.material == *orig && new_name != *orig {
                                        self.material = new_name;
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
                        if self.material == name {
                            if let Some(first) =
                                self.materials.names().first().map(|s| s.to_string())
                            {
                                self.material = first;
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
            Screen::Calculator => crate::calculator::view(self),
            Screen::Materials => crate::materials_view::view(self),
            Screen::Settings => crate::settings_view::view(self),
        }
    }

    /// Supply the custom theme (dark or light, per `resolved_palette`) to the
    /// iced application builder.
    pub fn theme(&self) -> Theme {
        let pal = self.pal();
        let name = if std::ptr::eq(pal, &LIGHT) {
            "OpenSpringmaker Light"
        } else {
            "OpenSpringmaker Dark"
        };
        Theme::custom(
            name.to_string(),
            iced::theme::Palette {
                background: pal.ink,
                text: pal.text,
                primary: pal.accent,
                success: pal.success,
                warning: pal.warn,
                danger: pal.danger,
            },
        )
    }

    /// The active palette, resolved from `theme_pref` × `system_mode`.
    pub(crate) fn pal(&self) -> &'static Palette {
        resolved_palette(self.theme_pref, self.system_mode)
    }

    /// Supply the OS-theme subscription to the iced application builder.
    /// Live theme-change events flow in as `Message::SystemTheme`; `update`'s
    /// `SystemTheme` arm only affects rendering when `theme_pref` is `System`
    /// (see `resolved_palette`). A thin humble shell over
    /// `iced::system::theme_changes()` — the Simulator can't drive a real OS
    /// subscription, so `ui_tests` pins the downstream rendering effect by
    /// dispatching `Message::SystemTheme` directly instead (OrbitCanvas
    /// discipline).
    pub fn subscription(&self) -> iced::Subscription<Message> {
        iced::system::theme_changes().map(Message::SystemTheme)
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
            Field::Inactive => f.inactive = value,
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

    fn set_ext_field(&mut self, field: crate::extension::form::Field, value: String) {
        use crate::extension::form::Field as EF;
        let f = &mut self.extension;
        match field {
            EF::WireDia => f.wire_dia = value,
            EF::MeanDia => f.mean_dia = value,
            EF::OuterDia => f.outer_dia = value,
            EF::Active => f.active = value,
            EF::FreeLength => f.free_length = value,
            EF::InitialTension => f.initial_tension = value,
            EF::Loads => f.loads = value,
            EF::Rate => f.rate = value,
            EF::HookR1 => f.hook_r1 = value,
            EF::HookR2 => f.hook_r2 = value,
            EF::Force1 => f.force1 = value,
            EF::Length1 => f.length1 = value,
            EF::Force2 => f.force2 = value,
            EF::Length2 => f.length2 = value,
            EF::MaxForce => f.max_force = value,
            EF::CandidateDiameters => f.candidate_diameters = value,
            EF::IndexMin => f.index_min = value,
            EF::IndexMax => f.index_max = value,
            EF::MaxOuterDia => f.max_outer_dia = value,
        }
    }

    fn set_tor_field(&mut self, field: crate::torsion::form::Field, value: String) {
        use crate::torsion::form::Field as TF;
        let f = &mut self.torsion;
        match field {
            TF::WireDia => f.wire_dia = value,
            TF::MeanDia => f.mean_dia = value,
            TF::OuterDia => f.outer_dia = value,
            TF::BodyCoils => f.body_coils = value,
            TF::Rate => f.rate = value,
            TF::Leg1 => f.leg1 = value,
            TF::Leg2 => f.leg2 = value,
            TF::ArborDia => f.arbor_dia = value,
            TF::Moments => f.moments = value,
            TF::Moment1 => f.moment1 = value,
            TF::Angle1 => f.angle1 = value,
            TF::Moment2 => f.moment2 = value,
            TF::Angle2 => f.angle2 = value,
            TF::Forces => f.forces = value,
            TF::LoadRadius => f.load_radius = value,
            TF::MaxMoment => f.max_moment = value,
            TF::IndexMin => f.index_min = value,
            TF::IndexMax => f.index_max = value,
            TF::MaxOuterDia => f.max_outer_dia = value,
            TF::CandidateDiameters => f.candidate_diameters = value,
            TF::FatigueMin => f.fatigue_min = value,
            TF::FatigueMax => f.fatigue_max = value,
        }
    }

    fn set_con_field(&mut self, field: crate::conical::form::Field, value: String) {
        use crate::conical::form::Field as CF;
        let f = &mut self.conical;
        match field {
            CF::WireDia => f.wire_dia = value,
            CF::LargeMeanDia => f.large_mean_dia = value,
            CF::SmallMeanDia => f.small_mean_dia = value,
            CF::Active => f.active = value,
            CF::FreeLength => f.free_length = value,
            CF::Loads => f.loads = value,
            CF::Inactive => f.inactive = value,
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
            MatField::AllowEndTorsion => f.allowable_end_torsion = value,
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
    pub(crate) fn save_to(&mut self, path: &std::path::Path) {
        self.action_error = None;
        let design = match self.family {
            Family::Compression => {
                match crate::compression::form::build_spec(&self.form, self.unit_system) {
                    Ok(s) => springcore::DesignSpec::Compression(s),
                    Err(e) => {
                        self.action_error = Some(e.to_string());
                        return;
                    }
                }
            }
            Family::Extension => {
                match crate::extension::form::build_spec(&self.extension, self.unit_system) {
                    Ok(e) => springcore::DesignSpec::Extension(e),
                    Err(e) => {
                        self.action_error = Some(e.to_string());
                        return;
                    }
                }
            }
            Family::Torsion => {
                match crate::torsion::form::build_spec(&self.torsion, self.unit_system) {
                    Ok(s) => springcore::DesignSpec::Torsion(s),
                    Err(e) => {
                        self.action_error = Some(e.to_string());
                        return;
                    }
                }
            }
            Family::Conical => {
                match crate::conical::form::build_spec(&self.conical, self.unit_system) {
                    Ok(s) => springcore::DesignSpec::Conical(s),
                    Err(e) => {
                        self.action_error = Some(e.to_string());
                        return;
                    }
                }
            }
            Family::Assembly => {
                match crate::assembly::form::build_spec(&self.assembly, self.unit_system) {
                    Ok(s) => springcore::DesignSpec::Assembly(s),
                    Err(e) => {
                        self.action_error = Some(e.to_string());
                        return;
                    }
                }
            }
        };
        let saved = SavedDesign {
            material: self.material.clone(),
            unit_system: self.unit_system,
            design,
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
    pub(crate) fn load_from(&mut self, path: &std::path::Path) -> bool {
        self.action_error = None;
        match SavedDesign::load(path) {
            Ok(saved) => self.apply_saved(saved),
            Err(e) => {
                self.action_error = Some(e.to_string());
                false
            }
        }
    }

    /// Apply a loaded design. Returns `false` when the design's family has no
    /// GUI yet (nothing applied, `action_error` set) so `load_from` can skip
    /// the recompute that would wipe the error — currently the rectangular
    /// placeholder (engine shipped, GUI in a later increment). The signature
    /// is RETAINED permanently so each family placeholder (see the conical
    /// Decision-5 reversal note) does not flip it again. The `()`↔`bool`
    /// pendulum ends here.
    fn apply_saved(&mut self, saved: SavedDesign) -> bool {
        if matches!(saved.design, springcore::DesignSpec::Rectangular(_)) {
            self.action_error = Some(
                "rectangular designs are not supported by this build yet (the rectangular \
                 GUI ships in a later increment)"
                    .into(),
            );
            return false;
        }
        self.material = saved.material;
        self.unit_system = saved.unit_system;
        match saved.design {
            springcore::DesignSpec::Compression(spec) => {
                self.family = Family::Compression;
                crate::compression::form::populate_from_spec(
                    &mut self.form,
                    &spec,
                    self.unit_system,
                );
            }
            springcore::DesignSpec::Extension(spec) => {
                self.family = Family::Extension;
                crate::extension::form::populate_from_spec(
                    &mut self.extension,
                    &spec,
                    self.unit_system,
                );
            }
            springcore::DesignSpec::Torsion(spec) => {
                self.family = Family::Torsion;
                crate::torsion::form::populate_from_spec(
                    &mut self.torsion,
                    &spec,
                    self.unit_system,
                );
            }
            springcore::DesignSpec::Conical(spec) => {
                self.family = Family::Conical;
                crate::conical::form::populate_from_spec(
                    &mut self.conical,
                    &spec,
                    self.unit_system,
                );
            }
            springcore::DesignSpec::Assembly(spec) => {
                self.family = Family::Assembly;
                crate::assembly::form::populate_from_spec(
                    &mut self.assembly,
                    &spec,
                    self.unit_system,
                    &self.material,
                );
            }
            springcore::DesignSpec::Rectangular(_) => unreachable!("handled above"),
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_dark_matches_the_legacy_c_tokens() {
        // Pin the exact legacy values so the C→Palette migration is provably
        // color-identical (any drift = silent restyle).
        assert_eq!(
            DARK.ink,
            Color {
                r: 0.055,
                g: 0.067,
                b: 0.086,
                a: 1.0
            }
        );
        assert_eq!(
            DARK.accent,
            Color {
                r: 0.298,
                g: 0.761,
                b: 1.0,
                a: 1.0
            }
        );
        assert_eq!(
            DARK.danger,
            Color {
                r: 1.0,
                g: 0.420,
                b: 0.420,
                a: 1.0
            }
        );
        assert_eq!(
            DARK.success,
            Color {
                r: 0.31,
                g: 0.78,
                b: 0.47,
                a: 1.0
            }
        );
    }

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

    /// A path that makes `save_to` fail deterministically WITHOUT touching the
    /// filesystem: an empty path has no parent, so `save_to`'s `parent()` guard
    /// errors before any temp file is written. (A relative name like
    /// `"x\0.toml"` would have parent `""` = cwd, leaking a `.settings.*.tmp`
    /// there before the rename fails — not hermetic.)
    fn unwritable_settings_path() -> std::path::PathBuf {
        std::path::PathBuf::new()
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

    /// An `App` with a writable temp-dir settings path — unlike `test_app`
    /// (non-persisting: `settings_path` is always `None`), this variant is for
    /// tests that assert on the actual persisted file. Follows the temp-dir
    /// idiom of `settings_correction_reclick_retries_after_a_failed_save` in
    /// ui_tests.rs: process id + thread id keep the directory unique across
    /// parallel test threads.
    fn test_app_with_writable_settings() -> App {
        let mut app = test_app();
        let dir = std::env::temp_dir().join(format!(
            "osm-app-theme-pref-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("create a writable temp dir");
        app.settings_path = Some(dir.join("settings.toml"));
        app
    }

    #[test]
    fn resolved_palette_covers_the_pref_by_mode_matrix() {
        use iced::theme::Mode::*;
        let cases: [(ThemePref, iced::theme::Mode, &Palette); 9] = [
            (ThemePref::Dark, None, &DARK),
            (ThemePref::Dark, Light, &DARK),
            (ThemePref::Dark, Dark, &DARK),
            (ThemePref::Light, None, &LIGHT),
            (ThemePref::Light, Light, &LIGHT),
            (ThemePref::Light, Dark, &LIGHT),
            (ThemePref::System, None, &DARK),
            (ThemePref::System, Light, &LIGHT),
            (ThemePref::System, Dark, &DARK),
        ];
        for (pref, mode, want) in cases {
            assert!(
                std::ptr::eq(resolved_palette(pref, mode), want),
                "{pref:?} × {mode:?}"
            );
        }
    }

    #[test]
    fn theme_pref_message_persists_and_switches_the_palette() {
        let mut app = test_app_with_writable_settings();
        app.update(Message::ThemePref(ThemePref::Light));
        assert!(std::ptr::eq(app.pal(), &LIGHT));
        let path = app.settings_path.clone().unwrap();
        let (saved, _) = crate::settings::load_from(&path);
        assert_eq!(saved.theme_pref, ThemePref::Light);
        assert_eq!(
            saved.curvature_correction, app.correction,
            "one save path carries BOTH prefs"
        );
        // Clean up the temp dir `test_app_with_writable_settings` created.
        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    /// `Message::SystemTheme` actually applies the reported mode to
    /// `system_mode` (and so, under the default `System` pref, to the
    /// resolved palette) — not just that it leaves other channels alone
    /// (`theme_messages_do_not_recompute_or_touch_error_channels`, below,
    /// only makes negative assertions and would stay green even if the
    /// handler dropped the mode on the floor).
    #[test]
    fn system_theme_message_switches_the_palette_under_system_pref() {
        let mut app = test_app();
        assert_eq!(app.theme_pref, ThemePref::System);
        assert!(std::ptr::eq(app.pal(), &DARK), "no OS report yet ⇒ DARK");

        app.update(Message::SystemTheme(iced::theme::Mode::Light));

        assert!(
            std::ptr::eq(app.pal(), &LIGHT),
            "System pref must follow a reported OS-Light switch"
        );
    }

    /// API-contract: `Message::ThemePref` and `Message::SystemTheme` are pure
    /// preference flips — neither recomputes (no `action_error` clear) nor
    /// disturbs a solve error, mirroring `probe_visual_message_preserves_error_channels`
    /// in ui_tests.rs (same shape, applied to the theme messages).
    #[test]
    fn theme_messages_do_not_recompute_or_touch_error_channels() {
        // A solved outcome plus a sentinel action_error, exactly like
        // `probe_visual_message_preserves_error_channels`'s first half: both
        // channels must survive a non-recomputing message untouched.
        let mut app = solved_app();
        let rate_before = app.outcome.as_ref().unwrap().design.rate;
        app.action_error = Some("sentinel".into());
        app.update(Message::ThemePref(ThemePref::Light));
        assert_eq!(
            app.action_error.as_deref(),
            Some("sentinel"),
            "ThemePref must not recompute (recompute clears action_error)"
        );
        assert_eq!(
            app.outcome.as_ref().map(|o| o.design.rate),
            Some(rate_before),
            "ThemePref must not disturb a solved outcome"
        );
        app.update(Message::SystemTheme(iced::theme::Mode::Dark));
        assert_eq!(
            app.action_error.as_deref(),
            Some("sentinel"),
            "SystemTheme must not recompute (recompute clears action_error)"
        );
        assert_eq!(
            app.outcome.as_ref().map(|o| o.design.rate),
            Some(rate_before),
            "SystemTheme must not disturb a solved outcome"
        );

        // A solve error survives both messages, in both directions.
        let mut bad = test_app();
        bad.form.scenario = crate::compression::form::ScenarioKind::RateBased;
        bad.form.wire_dia = "oops".into();
        bad.recompute();
        assert!(bad.error.is_some(), "fixture must have a solve error");
        let err = bad.error.clone();
        bad.update(Message::ThemePref(ThemePref::Dark));
        assert_eq!(bad.error, err, "solve error survives a ThemePref flip");
        bad.update(Message::SystemTheme(iced::theme::Mode::Light));
        assert_eq!(bad.error, err, "solve error survives a SystemTheme flip");
    }

    #[test]
    fn orbit_message_composes_the_delta_via_orbit_step_without_recompute() {
        let mut app = test_app();
        // Seed an action error; a non-recompute message must not clear it.
        app.action_error = Some("sentinel".into());
        let before = app.orbit;
        app.update(Message::Orbit(5.0, -3.0));
        assert_eq!(
            app.orbit,
            crate::viz::orbit_step(before, 5.0, -3.0),
            "App::update must delegate to orbit_step, not duplicate its math"
        );
        assert_eq!(app.action_error.as_deref(), Some("sentinel"));
    }

    /// `Message::Zoom` mirrors `Message::Orbit`'s update-boundary contract:
    /// delegate to `viz::zoom_step` (never duplicate its math), recompute
    /// nothing — both the save/load `action_error` and the solve `error`
    /// sentinels must survive (recompute clears `action_error`; a solve
    /// clears/overwrites `error`).
    #[test]
    fn zoom_message_composes_the_delta_via_zoom_step_without_recompute() {
        let mut app = test_app();
        app.action_error = Some("sentinel".into());
        app.error = Some("solve sentinel".into());
        let before = app.zoom;
        app.update(Message::Zoom(2.0));
        assert_eq!(
            app.zoom,
            crate::viz::zoom_step(before, 2.0),
            "App::update must delegate to zoom_step, not duplicate its math"
        );
        assert_eq!(app.action_error.as_deref(), Some("sentinel"));
        assert_eq!(app.error.as_deref(), Some("solve sentinel"));
    }

    /// `App.zoom` starts at 1.0 (unmagnified) and, because the arm routes
    /// every delta through `zoom_step`, extreme deltas land EXACTLY on the
    /// shared clamp bounds — the orbit pitch-clamp precedent.
    #[test]
    fn zoom_message_defaults_to_one_and_clamps_at_exact_bounds() {
        let mut app = test_app();
        assert_eq!(app.zoom, 1.0, "from_store must initialize zoom to 1.0");
        app.update(Message::Zoom(1000.0));
        assert_eq!(app.zoom, crate::viz::ZOOM_MAX);
        app.update(Message::Zoom(-1000.0));
        assert_eq!(app.zoom, crate::viz::ZOOM_MIN);
    }

    /// A non-finite wheel delta must leave the committed zoom unchanged —
    /// `zoom_step`'s guard is the single writer's induction step keeping
    /// `self.zoom` finite forever (the orbit NaN-delta precedent).
    #[test]
    fn zoom_message_ignores_a_non_finite_delta() {
        let mut app = test_app();
        app.update(Message::Zoom(0.5));
        let before = app.zoom;
        app.update(Message::Zoom(f32::NAN));
        assert_eq!(app.zoom, before);
        app.update(Message::Zoom(f32::INFINITY));
        assert_eq!(app.zoom, before);
    }

    /// Wheel events publish per-event DELTAS (`SpringShader::update`), so —
    /// exactly like the orbit compose pin — two sequential deltas must land
    /// where one combined delta would (`zoom_step` is multiplicative:
    /// e^0.1 · e^0.1 = e^0.2), so coalesced scroll events never drop steps.
    #[test]
    fn zoom_message_composes_across_repeated_updates() {
        use approx::assert_relative_eq;

        let mut sequential = test_app();
        sequential.update(Message::Zoom(1.0));
        sequential.update(Message::Zoom(1.0));

        let mut combined = test_app();
        combined.update(Message::Zoom(2.0));

        assert_relative_eq!(sequential.zoom, combined.zoom, max_relative = 1e-6);
    }

    /// `from_store` (every test-constructed app) must default
    /// `shader_available` to FALSE — the deterministic wireframe path. See
    /// the field's doc for the hard rule this anchors.
    #[test]
    fn from_store_defaults_shader_available_to_false() {
        assert!(!test_app().shader_available);
    }

    /// The regression this fix targets: publishing per-event DELTAS (rather
    /// than an absolute orbit computed against a possibly-stale base) means
    /// repeated `Message::Orbit` updates must compose additively — applying
    /// two small deltas in sequence must land (within float rounding) where
    /// one combined delta would, so coalesced drag events never drop
    /// intermediate steps.
    #[test]
    fn orbit_message_composes_across_repeated_updates() {
        use approx::assert_relative_eq;

        let mut sequential = test_app();
        sequential.update(Message::Orbit(10.0, 4.0));
        sequential.update(Message::Orbit(10.0, 4.0));

        let mut combined = test_app();
        combined.update(Message::Orbit(20.0, 8.0));

        assert_relative_eq!(
            sequential.orbit.yaw,
            combined.orbit.yaw,
            max_relative = 1e-5
        );
        assert_relative_eq!(
            sequential.orbit.pitch,
            combined.orbit.pitch,
            max_relative = 1e-5
        );
    }

    #[test]
    fn visual_mode_message_toggles_and_defaults_to_chart() {
        let mut app = test_app();
        assert_eq!(app.results_visual, VisualMode::Chart);
        app.update(Message::Visual(VisualMode::Spring3d));
        assert_eq!(app.results_visual, VisualMode::Spring3d);
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
        app.form.scenario = crate::compression::form::ScenarioKind::PowerUser;
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
        assert_eq!(app.material, "Music Wire");
    }

    #[test]
    fn default_app_loads_material_store_with_curated() {
        let app = App::default();
        assert!(app.materials.names().contains(&"Music Wire"));
    }

    #[test]
    fn recompute_produces_outcome_for_valid_form() {
        let mut app = App::default();
        app.form.scenario = crate::compression::form::ScenarioKind::RateBased;
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
        app.form.scenario = crate::compression::form::ScenarioKind::RateBased;
        app.form.wire_dia = "oops".into();
        app.recompute();
        assert!(app.outcome.is_none());
        assert!(app.error.is_some());
    }

    /// A hermetic app with a valid rate-based design already solved.
    fn solved_app() -> App {
        let mut app = test_app();
        app.form.scenario = crate::compression::form::ScenarioKind::RateBased;
        app.form.wire_dia = "2.0".into();
        app.form.mean_dia = "20.0".into();
        app.form.rate = "2.0".into();
        app.form.free_length = "60".into();
        app.form.loads = "10, 30".into();
        app.recompute();
        assert!(app.outcome.is_some(), "fixture should solve");
        app
    }

    /// A path every OS rejects (embedded NUL), so both `save_to` (write→rename)
    /// and `load_from` (read) fail deterministically with the same `InvalidInput`
    /// error on every platform — the NUL is rejected at the path→CString/wide-string
    /// conversion before any syscall (as `unreadable_path_yields_warning` also relies
    /// on). Rooted in `temp_dir()`, not a bare relative name, so the `.materials.*.tmp`
    /// a failing `save_to` writes before its rename fails lands in the system temp
    /// dir, never the repo tree (`atomic_write` cleans it up; rooting it here keeps
    /// even a SIGKILL-orphaned temp out of the working tree).
    ///
    /// Why not an empty path like `unwritable_settings_path`? That helper is save-only
    /// (it fails at `save_to`'s no-parent guard). Here an empty path reads as `NotFound`,
    /// not `InvalidInput` — breaking the same-error guarantee, and silently hollowing out
    /// the load test if `SavedDesign::load` ever treats `NotFound` as benign (as
    /// `MaterialStore::from_overlay_file` does). The NUL also exercises `atomic_write`'s
    /// rename-fail cleanup, which the empty-path guard would skip.
    fn io_failing_path() -> std::path::PathBuf {
        std::env::temp_dir().join("invalid\0path.toml")
    }

    #[test]
    fn save_failure_surfaces_action_error_and_preserves_outcome() {
        let mut app = solved_app();
        app.save_to(&io_failing_path());
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
        let before_material = app.material.clone();
        let mutated = app.load_from(&io_failing_path());
        assert!(!mutated, "a failed load reports no form mutation");
        assert!(app.action_error.is_some(), "load failure must be surfaced");
        assert!(
            app.outcome.is_some(),
            "a failed load must not clear results"
        );
        assert_eq!(
            app.material, before_material,
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
        app.update(Message::MatField(MatField::AllowEndTorsion, "0.40".into()));
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
        a.material = copy_name.clone(); // calculator selects it too
        a.update(Message::MatDelete(copy_name.clone()));
        assert!(a.mat_error.is_none());
        assert!(!a.materials.names().contains(&copy_name.as_str()));
        // Editor closed and calculator selection moved to a valid material.
        assert!(a.editing.is_none());
        assert_ne!(a.material, copy_name);
        assert!(a.materials.names().contains(&a.material.as_str()));
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
        a.material = orig.clone(); // calculator selects it
                                   // Rename it via the editor and commit.
        a.update(Message::MatField(MatField::Name, "Renamed Wire".into()));
        a.update(Message::MatCommit);
        assert!(a.mat_error.is_none());
        assert!(a.materials.names().contains(&"Renamed Wire"));
        assert!(!a.materials.names().contains(&orig.as_str()));
        // The calculator selection followed the rename (no stale MaterialNotFound).
        assert_eq!(a.material, "Renamed Wire");
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
            a.materials.names().contains(&a.material.as_str()),
            "INV1 violated: material '{}' is not in the store",
            a.material
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

    /// R1/R2 regression: Compression arm must also have a blank-form guard.
    ///
    /// Before this fix, `SelectFamily(Compression)` on a blank form called
    /// `parse_and_solve` immediately, producing "wire diameter required" on a
    /// form the user never touched.
    #[test]
    fn select_compression_on_blank_form_shows_no_error() {
        use springcore::Family;

        let mut app = test_app();
        // Switch to Extension first (triggering recompute with blank guard).
        app.update(Message::SelectFamily(Family::Extension));
        // Switch back to Compression — Compression arm must apply the same guard.
        app.update(Message::SelectFamily(Family::Compression));

        assert!(
            app.error.is_none(),
            "blank compression form must not produce a parse error after SelectFamily"
        );
        assert!(
            app.outcome.is_none(),
            "blank compression form must not produce an outcome"
        );
    }

    /// A partially-filled Dimensional form (outer diameter entered, wire diameter
    /// still blank) must NOT be suppressed as "blank" — Dimensional reads `outer_dia`,
    /// not `mean_dia`, so the form carries real input and the missing wire diameter
    /// must surface as a parse error. Regression: the old guard checked `mean_dia`
    /// (which Dimensional never fills), so any Dimensional form with a blank wire was
    /// wrongly treated as blank and left in the Empty state.
    #[test]
    fn partially_filled_dimensional_form_surfaces_parse_error() {
        use crate::compression::form::ScenarioKind;
        let mut app = test_app();
        app.form.scenario = ScenarioKind::Dimensional;
        app.form.outer_dia = "30".into();
        app.recompute();
        assert!(
            app.error.is_some(),
            "a Dimensional form with input but a missing wire diameter must show a parse error, not stay Empty"
        );
        assert!(app.outcome.is_none());
    }

    /// A partially-filled extension form (free length entered, geometry blank) must
    /// surface a parse error rather than staying Empty. Regression: the old guard
    /// only checked wire/mean/active, so entering free length or initial tension
    /// first was wrongly suppressed.
    #[test]
    fn partially_filled_extension_form_surfaces_parse_error() {
        use springcore::Family;
        let mut app = test_app();
        app.update(Message::SelectFamily(Family::Extension));
        app.extension.free_length = "60".into();
        app.recompute();
        assert!(
            app.error.is_some(),
            "an extension form with input but missing geometry must show a parse error, not stay Empty"
        );
        assert!(app.ext_outcome.is_none());
    }

    // ── Conical family: cross-family outcome clearing ────────────────────────

    /// Switching to the Conical family must clear stale outcomes from every
    /// other family so the results panel can never show residual data.
    #[test]
    fn switching_to_conical_clears_other_family_outcomes() {
        use crate::extension::form::{parse_and_solve as ext_parse_and_solve, ExtFormState};
        use crate::torsion::form::{parse_and_solve as tor_parse_and_solve, TorFormState};
        use springcore::{CurvatureCorrection, Family, UnitSystem};

        let mut app = solved_app();
        assert!(app.outcome.is_some(), "pre-condition: compression solved");

        // Inject a real extension outcome directly (recompute would clobber outcome).
        let ext_form = ExtFormState {
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            active: "10".into(),
            free_length: "100".into(),
            initial_tension: "5".into(),
            loads: "50".into(),
            ..ExtFormState::default()
        };
        let ext_out = ext_parse_and_solve(
            &ext_form,
            "Music Wire",
            UnitSystem::Metric,
            &app.materials,
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        app.ext_outcome = Some(ext_out);
        assert!(
            app.ext_outcome.is_some(),
            "pre-condition: ext_outcome must be Some before switching"
        );

        // Inject a real torsion outcome directly.
        let tor_form = TorFormState {
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            body_coils: "5".into(),
            leg1: "0".into(),
            leg2: "0".into(),
            moments: "1000".into(),
            ..TorFormState::default()
        };
        let tor_out =
            tor_parse_and_solve(&tor_form, "Music Wire", UnitSystem::Metric, &app.materials)
                .unwrap();
        app.tor_outcome = Some(tor_out);
        assert!(
            app.tor_outcome.is_some(),
            "pre-condition: tor_outcome must be Some before switching"
        );

        // Switch to Conical — the Conical arm of recompute() clears all three.
        app.update(Message::SelectFamily(Family::Conical));

        assert!(
            app.outcome.is_none(),
            "compression outcome must be None after switching to Conical"
        );
        assert!(
            app.ext_outcome.is_none(),
            "ext_outcome must be None after switching to Conical"
        );
        assert!(
            app.tor_outcome.is_none(),
            "tor_outcome must be None after switching to Conical"
        );
    }

    /// Switching away from the Conical family must clear the conical outcome so
    /// the results panel can never show stale data when another family is active.
    #[test]
    fn switching_away_from_conical_clears_con_outcome() {
        use springcore::Family;
        let mut app = test_app();
        app.update(Message::SelectFamily(Family::Conical));
        app.conical.wire_dia = "2".into();
        app.conical.large_mean_dia = "20".into();
        app.conical.small_mean_dia = "12".into();
        app.conical.active = "10".into();
        app.conical.free_length = "60".into();
        app.conical.loads = "10".into();
        app.recompute();
        assert!(app.con_outcome.is_some(), "fixture should solve");
        app.update(Message::SelectFamily(Family::Compression));
        assert!(
            app.con_outcome.is_none(),
            "switching away from Conical must clear conical outcome"
        );
    }

    /// Switching to Extension must clear a primed con_outcome so the conical
    /// results panel can never show stale data when Extension is active.
    /// Revert-probe: comment out `self.con_outcome = None` in the Extension arm
    /// → this test FAILS → restore → green.
    #[test]
    fn switching_to_extension_clears_con_outcome() {
        use crate::conical::form::{parse_and_solve as con_parse_and_solve, ConFormState};
        use crate::extension::form::ExtFormState;
        use springcore::{CurvatureCorrection, Family, UnitSystem};

        let mut app = test_app();

        // Prime a real conical outcome by solving directly.
        let con_form = ConFormState {
            end_type: "squared_ground".into(),
            wire_dia: "2".into(),
            large_mean_dia: "20".into(),
            small_mean_dia: "12".into(),
            active: "10".into(),
            free_length: "60".into(),
            loads: "10".into(),
            inactive: String::new(),
        };
        let con_out = con_parse_and_solve(
            &con_form,
            "Music Wire",
            UnitSystem::Metric,
            &app.materials,
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        app.con_outcome = Some(con_out);
        assert!(
            app.con_outcome.is_some(),
            "pre-condition: con_outcome must be Some before switching"
        );

        // Switch to Extension with a valid form so the Extension arm runs through
        // to the parse_and_solve path (not the blank-guard early return).
        app.extension = ExtFormState {
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            active: "10".into(),
            free_length: "100".into(),
            initial_tension: "5".into(),
            loads: "50".into(),
            ..ExtFormState::default()
        };
        app.update(Message::SelectFamily(Family::Extension));

        assert!(
            app.con_outcome.is_none(),
            "switching to Extension must clear con_outcome"
        );
    }

    /// Switching to Torsion must clear a primed con_outcome so the conical
    /// results panel can never show stale data when Torsion is active.
    /// Revert-probe: comment out `self.con_outcome = None` in the Torsion arm
    /// → this test FAILS → restore → green.
    #[test]
    fn switching_to_torsion_clears_con_outcome() {
        use crate::conical::form::{parse_and_solve as con_parse_and_solve, ConFormState};
        use crate::torsion::form::TorFormState;
        use springcore::{CurvatureCorrection, Family, UnitSystem};

        let mut app = test_app();

        // Prime a real conical outcome by solving directly.
        let con_form = ConFormState {
            end_type: "squared_ground".into(),
            wire_dia: "2".into(),
            large_mean_dia: "20".into(),
            small_mean_dia: "12".into(),
            active: "10".into(),
            free_length: "60".into(),
            loads: "10".into(),
            inactive: String::new(),
        };
        let con_out = con_parse_and_solve(
            &con_form,
            "Music Wire",
            UnitSystem::Metric,
            &app.materials,
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        app.con_outcome = Some(con_out);
        assert!(
            app.con_outcome.is_some(),
            "pre-condition: con_outcome must be Some before switching"
        );

        // Switch to Torsion with a valid form so the Torsion arm runs through
        // to the parse_and_solve path (not the blank-guard early return).
        app.torsion = TorFormState {
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            body_coils: "5".into(),
            leg1: "0".into(),
            leg2: "0".into(),
            moments: "1000".into(),
            ..TorFormState::default()
        };
        app.update(Message::SelectFamily(Family::Torsion));

        assert!(
            app.con_outcome.is_none(),
            "switching to Torsion must clear con_outcome"
        );
    }

    // ── Conical family: apply_saved integration test ─────────────────────────

    #[test]
    fn loading_a_conical_design_populates_the_conical_form() {
        let mut app = test_app();
        app.apply_saved(springcore::SavedDesign {
            material: "Music Wire".to_string(),
            unit_system: springcore::UnitSystem::Metric,
            design: springcore::DesignSpec::Conical(springcore::ConicalSpec::PowerUser {
                end_type: "squared_ground".to_string(),
                wire_dia_mm: 2.0,
                large_mean_dia_mm: 20.0,
                small_mean_dia_mm: 12.0,
                active: 10.0,
                free_length_mm: 60.0,
                loads_n: vec![10.0],
                inactive_coils: None,
            }),
        });
        assert_eq!(app.family, springcore::Family::Conical);
        assert_eq!(app.conical.wire_dia, "2");
        assert_eq!(app.conical.large_mean_dia, "20");
        assert!(app.action_error.is_none());
    }

    // ── Assembly family: apply_saved integration test ─────────────────────────

    #[test]
    fn loading_an_assembly_design_populates_the_assembly_form() {
        let mut app = test_app();
        let applied = app.apply_saved(springcore::SavedDesign {
            material: "Music Wire".to_string(),
            unit_system: springcore::UnitSystem::Metric,
            design: springcore::DesignSpec::Assembly(springcore::AssemblySpec::PowerUser {
                topology: "nested".into(),
                fixity: "fixed_fixed".into(),
                loads_n: vec![10.0],
                members: vec![springcore::AssemblyMemberSpec {
                    material_name: "Music Wire".into(),
                    end_type: "squared_ground".into(),
                    wire_dia_mm: 2.0,
                    mean_dia_mm: 20.0,
                    active: 10.0,
                    free_length_mm: 60.0,
                }],
            }),
        });
        assert!(applied);
        assert_eq!(app.family, springcore::Family::Assembly);
        assert_eq!(app.assembly.members.len(), 1);
        assert_eq!(app.assembly.members[0].wire_dia, "2");
        assert!(app.action_error.is_none());
    }

    // ── Rectangular placeholder: apply_saved rejects and preserves state ──────

    /// The rectangular family persists but has no GUI yet (assembly-pattern
    /// placeholder). `apply_saved` must reject BEFORE any mutation and return
    /// `false` so `load_from`'s caller skips the recompute that would wipe the
    /// error — the load-path invariant from the conical/assembly increments.
    #[test]
    fn loading_a_rectangular_design_rejects_and_preserves_form() {
        let mut app = test_app();
        // Pre-seed values that differ from the design's `material` ("Music
        // Wire") and `unit_system` (Metric), so the unchanged assertions can't
        // pass vacuously if `apply_saved` mutates before returning false.
        app.material = "Chrome-Vanadium".to_string();
        app.unit_system = springcore::UnitSystem::Us;
        let seeded_family = app.family;

        let applied = app.apply_saved(springcore::SavedDesign {
            material: "Music Wire".to_string(),
            unit_system: springcore::UnitSystem::Metric,
            design: springcore::DesignSpec::Rectangular(springcore::RectangularSpec::PowerUser {
                end_type: "squared_ground".to_string(),
                wire_axial_mm: 3.0,
                wire_radial_mm: 2.0,
                mean_dia_mm: 30.0,
                active: 8.0,
                free_length_mm: 40.0,
                loads_n: vec![10.0],
            }),
        });

        assert!(
            !applied,
            "apply_saved must return false for a rectangular design"
        );
        assert!(
            app.action_error
                .as_deref()
                .is_some_and(|m| m.contains("rectangular designs are not supported")),
            "action_error must contain the rejection message, got: {:?}",
            app.action_error
        );
        assert_eq!(
            app.material, "Chrome-Vanadium",
            "material must be unchanged (reject must happen before any mutation)"
        );
        assert_eq!(
            app.unit_system,
            springcore::UnitSystem::Us,
            "unit_system must be unchanged"
        );
        assert_eq!(app.family, seeded_family, "family must be unchanged");
    }

    /// AsmMemberRemove with an out-of-bounds or at-boundary index must be a no-op,
    /// not a Vec::remove panic — AND side-effect-free: a no-op must not trigger the
    /// recompute that clears `action_error` (nothing changed, so nothing to resolve).
    /// Revert-probe (panic): remove the `i < len` guard → this test panics.
    /// Revert-probe (side effect): make the no-op arm return `true` → recompute clears
    /// `action_error` → the preservation asserts fail → restore → green.
    #[test]
    fn asm_member_remove_oob_is_noop() {
        use crate::assembly::form::AsmMemberForm;
        let mut app = test_app();
        // Seed three members (default starts with one blank; push two more).
        app.assembly
            .members
            .push(AsmMemberForm::blank("Music Wire"));
        app.assembly
            .members
            .push(AsmMemberForm::blank("Music Wire"));
        assert_eq!(app.assembly.members.len(), 3);

        // Seed a status message a prior failed save would have set. A genuine no-op
        // must leave it intact (only a real state change recomputes and clears it).
        app.action_error = Some("prior status".to_string());

        // OOB index (5 on a 3-element vec) must be a no-op, not a panic.
        app.update(Message::AsmMemberRemove(5));
        assert_eq!(
            app.assembly.members.len(),
            3,
            "OOB AsmMemberRemove(5) must not change length"
        );
        assert_eq!(
            app.action_error.as_deref(),
            Some("prior status"),
            "a no-op OOB removal must not recompute and clear action_error"
        );

        // Exact boundary: i == len (3) must also be a no-op.
        app.update(Message::AsmMemberRemove(3));
        assert_eq!(
            app.assembly.members.len(),
            3,
            "AsmMemberRemove(i==len) must be a no-op"
        );
        assert_eq!(
            app.action_error.as_deref(),
            Some("prior status"),
            "a boundary no-op removal must not clear action_error"
        );
    }

    /// The three member-attribute arms (`AsmField`/`AsmMemberMaterial`/
    /// `AsmMemberEndType`) share the no-op-is-side-effect-free contract: an
    /// out-of-bounds index writes nothing, so it must not recompute and clear
    /// `action_error`. Locks the sibling sweep alongside `AsmMemberRemove`.
    /// Revert-probe: make any of the three arms return `true` on the `None`
    /// branch → recompute clears action_error → the matching assert fails.
    #[test]
    fn asm_member_attribute_oob_is_side_effect_free() {
        use crate::assembly::form::MemberField;
        let mut app = test_app();
        assert_eq!(app.assembly.members.len(), 1); // default: one blank member
        let oob = 7; // well past the single member

        app.action_error = Some("prior status".to_string());
        app.update(Message::AsmField(
            oob,
            MemberField::WireDia,
            "3".to_string(),
        ));
        app.update(Message::AsmMemberMaterial(oob, "Stainless 302".to_string()));
        app.update(Message::AsmMemberEndType(oob, "closed_ground".to_string()));

        assert_eq!(
            app.assembly.members.len(),
            1,
            "OOB member-attribute writes must not alter the member list"
        );
        assert_eq!(
            app.action_error.as_deref(),
            Some("prior status"),
            "OOB member-attribute writes are no-ops and must not clear action_error"
        );
    }

    /// One named (label, dispatch) pair for
    /// `pick_list_reselect_current_value_preserves_action_error` — named alias
    /// keeps clippy's `type_complexity` lint quiet on the `Vec` below.
    type NamedDispatch = (&'static str, Box<dyn Fn(&mut App)>);

    /// Panel R2 item 1: iced's `pick_list` dispatches `on_select`
    /// unconditionally, even when re-selecting the value that's ALREADY
    /// selected (vendored `pick_list.rs`) — so every pick_list-backed handler
    /// must guard at the UPDATE boundary (this file's own `AsmField`/
    /// `AsmMemberRemove` precedent above), not rely on the view-level
    /// (`segmented`) guard alone. Exercises all 15 affected handlers: firing
    /// each with its OWN current value must be a no-op, so a pending
    /// `action_error` must survive every one of them.
    /// Revert-probe: revert any ONE handler to its old unconditional-`true`
    /// shape (e.g. `Message::Material(m) => { self.material = m; true }`) →
    /// that handler's iteration clears `action_error` → the assert for that
    /// name fails → restore → green.
    #[test]
    fn pick_list_reselect_current_value_preserves_action_error() {
        let mut app = test_app();
        // A valid (non-OOB) member index for AsmMemberMaterial/AsmMemberEndType.
        assert_eq!(app.assembly.members.len(), 1);

        let dispatchers: Vec<NamedDispatch> = vec![
            (
                "Material",
                Box::new(|a: &mut App| a.update(Message::Material(a.material.clone()))),
            ),
            (
                "Scenario",
                Box::new(|a: &mut App| a.update(Message::Scenario(a.form.scenario))),
            ),
            (
                "EndType",
                Box::new(|a: &mut App| a.update(Message::EndType(a.form.end_type.clone()))),
            ),
            (
                "Fixity",
                Box::new(|a: &mut App| a.update(Message::Fixity(a.form.fixity.clone()))),
            ),
            (
                "ExtScenario",
                Box::new(|a: &mut App| a.update(Message::ExtScenario(a.extension.scenario))),
            ),
            (
                "TorFriction",
                Box::new(|a: &mut App| a.update(Message::TorFriction(a.torsion.friction_model))),
            ),
            (
                "TorScenario",
                Box::new(|a: &mut App| a.update(Message::TorScenario(a.torsion.scenario))),
            ),
            (
                "TorMomentEntry",
                Box::new(|a: &mut App| a.update(Message::TorMomentEntry(a.torsion.moment_entry))),
            ),
            (
                "TorDiaPolicy",
                Box::new(|a: &mut App| a.update(Message::TorDiaPolicy(a.torsion.dia_policy))),
            ),
            (
                "TorCycleLife",
                Box::new(|a: &mut App| a.update(Message::TorCycleLife(a.torsion.cycle_life))),
            ),
            (
                "ConEndType",
                Box::new(|a: &mut App| a.update(Message::ConEndType(a.conical.end_type.clone()))),
            ),
            (
                "AsmTopology",
                Box::new(|a: &mut App| a.update(Message::AsmTopology(a.assembly.topology.clone()))),
            ),
            (
                "AsmFixity",
                Box::new(|a: &mut App| a.update(Message::AsmFixity(a.assembly.fixity.clone()))),
            ),
            (
                "AsmMemberMaterial",
                Box::new(|a: &mut App| {
                    let mat = a.assembly.members[0].material.clone();
                    a.update(Message::AsmMemberMaterial(0, mat));
                }),
            ),
            (
                "AsmMemberEndType",
                Box::new(|a: &mut App| {
                    let et = a.assembly.members[0].end_type.clone();
                    a.update(Message::AsmMemberEndType(0, et));
                }),
            ),
        ];

        for (name, dispatch) in dispatchers {
            app.action_error = Some("sentinel".to_string());
            dispatch(&mut app);
            assert_eq!(
                app.action_error.as_deref(),
                Some("sentinel"),
                "re-selecting {name}'s current value must be a no-op and must not clear action_error"
            );
        }
    }

    /// Panel R3 item 1 (adversary, reproduced): `MatFormKind`/`MatUnits` were
    /// missed by the R2 sweep above — both unconditionally cleared
    /// `mat_error`, so re-selecting the materials-editor pick_list's CURRENT
    /// value wiped a pending error banner though nothing changed. Same no-op
    /// contract as `pick_list_reselect_current_value_preserves_action_error`
    /// above, but for `mat_error` on these two handlers — AND a genuine
    /// change must still clear `mat_error` (the deliberate policy for real
    /// edits, unchanged).
    /// Revert-probe: drop either guard back to an unconditional `self.mat_error
    /// = None` → that handler's same-value assertion fails → restore → green.
    #[test]
    fn mat_form_pick_lists_guard_mat_error_on_same_value_only() {
        let mut app = test_app();

        // Same-value dispatch must NOT clear a pending mat_error.
        app.mat_error = Some("sentinel".to_string());
        app.update(Message::MatFormKind(app.mat_form.mts_form));
        assert_eq!(
            app.mat_error.as_deref(),
            Some("sentinel"),
            "re-selecting MatFormKind's current value must not clear mat_error"
        );

        app.mat_error = Some("sentinel".to_string());
        app.update(Message::MatUnits(app.mat_form.mts_units));
        assert_eq!(
            app.mat_error.as_deref(),
            Some("sentinel"),
            "re-selecting MatUnits's current value must not clear mat_error"
        );

        // A REAL change must still clear mat_error (deliberate policy preserved).
        app.mat_error = Some("sentinel".to_string());
        app.update(Message::MatFormKind(MtsForm::Constant));
        assert!(
            app.mat_error.is_none(),
            "a genuine MatFormKind change must still clear mat_error"
        );
        assert_eq!(app.mat_form.mts_form, MtsForm::Constant);

        app.mat_error = Some("sentinel".to_string());
        app.update(Message::MatUnits(StrengthUnits::UsKpsiInch));
        assert!(
            app.mat_error.is_none(),
            "a genuine MatUnits change must still clear mat_error"
        );
        assert_eq!(app.mat_form.mts_units, StrengthUnits::UsKpsiInch);
    }

    /// Switching away from Assembly must clear the asm_outcome so the results
    /// panel can never show stale data when another family is active.
    /// Revert-probe: comment out `self.asm_outcome = None` in a non-Assembly arm
    /// (Compression, Extension, Torsion, or Conical) → this test FAILS → restore → green.
    #[test]
    fn switching_away_from_assembly_clears_asm_outcome() {
        use crate::assembly::form::{parse_and_solve as asm_parse_and_solve, AsmFormState};
        use springcore::{CurvatureCorrection, Family, UnitSystem};

        let mut app = test_app();

        // Prime a real assembly outcome by solving directly.
        let asm_form = AsmFormState {
            topology: "nested".into(),
            fixity: "fixed_fixed".into(),
            loads: "10".into(),
            members: vec![crate::assembly::form::AsmMemberForm {
                material: "Music Wire".into(),
                end_type: "squared_ground".into(),
                wire_dia: "2".into(),
                mean_dia: "20".into(),
                active: "10".into(),
                free_length: "60".into(),
            }],
        };
        let asm_out = asm_parse_and_solve(
            &asm_form,
            UnitSystem::Metric,
            &app.materials,
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        app.asm_outcome = Some(asm_out);
        assert!(
            app.asm_outcome.is_some(),
            "pre-condition: asm_outcome must be Some before switching"
        );

        // Switch to Compression — the Compression arm of recompute() must clear asm_outcome.
        app.update(Message::SelectFamily(Family::Compression));

        assert!(
            app.asm_outcome.is_none(),
            "switching away from Assembly must clear asm_outcome"
        );
    }

    #[test]
    fn palette_dark_derived_fields_match_the_legacy_runtime_math() {
        assert_eq!(
            DARK.accent_tint,
            Color {
                r: DARK.accent.r * 0.15,
                g: DARK.accent.g * 0.15,
                b: DARK.accent.b * 0.15,
                a: 1.0
            }
        );
        assert_eq!(
            DARK.hover,
            Color {
                r: DARK.raised.r + 0.05,
                g: DARK.raised.g + 0.05,
                b: DARK.raised.b + 0.05,
                a: 1.0
            }
        );
    }

    // ----------------------------------------------------------------------
    // WCAG 2.x contrast gate — machine-checked, not eyeballed.
    // ----------------------------------------------------------------------

    fn srgb_lin(c: f32) -> f64 {
        let c = c as f64;
        if c <= 0.040_45 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    fn luminance(c: Color) -> f64 {
        0.2126 * srgb_lin(c.r) + 0.7152 * srgb_lin(c.g) + 0.0722 * srgb_lin(c.b)
    }
    fn contrast(a: Color, b: Color) -> f64 {
        let (l1, l2) = (
            luminance(a).max(luminance(b)),
            luminance(a).min(luminance(b)),
        );
        (l1 + 0.05) / (l2 + 0.05)
    }

    /// Adjudication (carried from the Task 2 review): `danger`/`warn`/`success`
    /// are not checked against `LIGHT.hover` here because that pairing never
    /// occurs in the UI — `pal.hover` is used exactly once, as the segmented
    /// control's unselected-option *hover* background (`widgets.rs`'s
    /// `segmented_style`), whose text color in that branch is always
    /// `pal.text`, never a status color. So the fg×bg matrix below (which
    /// deliberately omits `hover` as a background) covers every real
    /// status-color-on-background pairing that actually renders.
    #[test]
    fn light_palette_meets_wcag_aa_on_both_surfaces() {
        // Body text sizes here are 11-14px — AA small-text threshold 4.5:1.
        for bg in [LIGHT.ink, LIGHT.panel, LIGHT.raised] {
            for fg in [
                LIGHT.text,
                LIGHT.muted,
                LIGHT.accent,
                LIGHT.danger,
                LIGHT.warn,
                LIGHT.success,
            ] {
                assert!(
                    contrast(fg, bg) >= 4.5,
                    "LIGHT fg {fg:?} on bg {bg:?} = {:.2}, needs 4.5",
                    contrast(fg, bg)
                );
            }
        }
        // Selected-option text is accent-on-accent_tint (segmented_style).
        assert!(contrast(LIGHT.accent, LIGHT.accent_tint) >= 4.5);
        // Hovered-unselected-option text is text-on-hover (segmented_style).
        assert!(contrast(LIGHT.text, LIGHT.hover) >= 4.5);
        // Structural sanity: light surfaces order light→dark as ink ≥ panel ≥ raised > hover.
        assert!(luminance(LIGHT.ink) > luminance(LIGHT.panel));
        assert!(luminance(LIGHT.panel) > luminance(LIGHT.raised));
        assert!(luminance(LIGHT.raised) > luminance(LIGHT.hover));
        // The hairline must remain visible but not text-strong.
        assert!(contrast(LIGHT.line, LIGHT.panel) >= 1.2);
    }

    #[test]
    fn dark_palette_meets_the_same_bar() {
        // Measured: every DARK fg/bg pairing already clears AA 4.5 — worst is
        // DARK.muted on DARK.raised at 5.12:1 (see task-2-report.md for the
        // full matrix). DARK is the shipped, frozen identity; this test pins
        // the bar to 4.5 (not the measured floor) so a future regression that
        // erodes the margin is still caught.
        const DARK_FLOOR: f64 = 4.5;
        for bg in [DARK.ink, DARK.panel, DARK.raised] {
            for fg in [
                DARK.text,
                DARK.muted,
                DARK.accent,
                DARK.danger,
                DARK.warn,
                DARK.success,
            ] {
                assert!(
                    contrast(fg, bg) >= DARK_FLOOR,
                    "DARK fg {fg:?} on {bg:?} = {:.2}",
                    contrast(fg, bg)
                );
            }
        }
        // Selected-option text is accent-on-accent_tint (segmented_style).
        assert!(contrast(DARK.accent, DARK.accent_tint) >= 4.5);
        // Hovered-unselected-option text is text-on-hover (segmented_style).
        assert!(contrast(DARK.text, DARK.hover) >= 4.5);
    }

    #[test]
    fn subscription_wires_at_least_one_recipe() {
        let app = test_app();
        assert_ne!(
            app.subscription().units(),
            0,
            "App::subscription must wire the OS theme_changes recipe"
        );
    }
}
