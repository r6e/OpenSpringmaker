mod app;
mod assembly;
mod calculator;
mod compression;
mod conical;
mod extension;
mod form_helpers;
mod materials_form;
mod materials_view;
mod materials_view_model;
mod picker;
mod plot;
mod presenter;
mod settings;
mod settings_view;
mod settings_view_model;
mod torsion;
mod viz;
mod widgets;

#[cfg(test)]
mod ui_tests;

use app::App;
use iced::window;
use iced::Size;
use springcore::{LoadWarning, MaterialStore};

fn initial_app() -> App {
    let (settings, settings_warning) = settings::AppSettings::load();
    let (materials, mut load_warnings) = MaterialStore::load();
    // Surface a settings-load problem (unreadable/malformed file → reset to
    // defaults) in the same startup status channel as material-load warnings, so a
    // silently-reset preference is visible rather than hidden.
    if let Some(message) = settings_warning {
        load_warnings.push(LoadWarning { message });
    }
    let mut app = App::from_store(materials, load_warnings, settings.curvature_correction);
    // Wire up the real settings path so preference changes are persisted.
    // None if the platform config dir is unavailable (settings_path() returns None).
    app.settings_path = settings::settings_path();
    app
}

fn main() -> iced::Result {
    iced::application(initial_app, App::update, App::view)
        .title("OpenSpringmaker")
        .theme(App::theme)
        .window(window::Settings {
            size: Size::new(1200.0, 820.0),
            min_size: Some(Size::new(720.0, 600.0)),
            ..Default::default()
        })
        .run()
}
