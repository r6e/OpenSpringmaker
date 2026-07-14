mod app;
mod assembly;
mod calculator;
mod compression;
mod conical;
mod diagram;
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

/// One-shot boot probe: can wgpu produce ANY adapter on this machine? The
/// shaded 3D path needs a GPU adapter; where none exists (VMs, headless
/// sessions, exotic driver states) the app must degrade to the CPU
/// wireframe renderer up front rather than fail at first shaded render.
/// `request_adapter` is async only for WebGPU's sake — on native it
/// resolves immediately, so `pollster` drives it to completion right here.
/// `catch_unwind` covers `wgpu::Instance::default()`'s documented panic
/// (no enabled backend for the platform — unreachable with iced's default
/// features, but a probe whose entire job is graceful degradation must not
/// itself be able to crash the boot).
fn shader_probe() -> bool {
    std::panic::catch_unwind(|| {
        pollster::block_on(async {
            iced::wgpu::Instance::default()
                .request_adapter(&iced::wgpu::RequestAdapterOptions::default())
                .await
                .is_ok()
        })
    })
    .unwrap_or(false)
}

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
    // The probe runs ONCE, here — `App::from_store` deliberately defaults
    // `shader_available` to false (see the field's doc for the test-path
    // and snapshot hard-rule reasons).
    app.shader_available = shader_probe();
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

#[cfg(test)]
mod tests {
    /// The probe must COMPLETE on every machine — including GPU-less CI,
    /// where it reports `false`. The value itself is machine-dependent by
    /// nature, so this pins only the no-panic contract (the
    /// `catch_unwind`/`unwrap_or(false)` path) and prints the local verdict
    /// for the test log; it must never assert on the value.
    #[test]
    fn shader_probe_completes_without_panicking() {
        let available = super::shader_probe();
        println!("shader_probe() -> {available}");
    }
}
