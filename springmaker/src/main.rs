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
    app.theme_pref = settings.theme_pref;
    app
}

/// Boot the app AND seed `system_mode` with the OS's current theme, so a
/// `ThemePref::System` user gets the right palette on the very first frame
/// rather than only after their next OS theme change (which is all the
/// `subscription` alone would catch). `iced::system::theme()` resolves the
/// query as a one-shot `Task`; `iced::application`'s boot closure accepts
/// `(State, Task<Message>)` (`IntoBoot` in the vendored `iced::application`
/// source), so returning the seed task here dispatches it through the normal
/// update loop before the first `view()`.
fn boot() -> (App, iced::Task<app::Message>) {
    (
        initial_app(),
        iced::system::theme().map(app::Message::SystemTheme),
    )
}

fn main() -> iced::Result {
    iced::application(boot, App::update, App::view)
        .title("OpenSpringmaker")
        .theme(App::theme)
        .subscription(App::subscription)
        .window(window::Settings {
            size: Size::new(1200.0, 820.0),
            min_size: Some(Size::new(720.0, 600.0)),
            ..Default::default()
        })
        .run()
}
