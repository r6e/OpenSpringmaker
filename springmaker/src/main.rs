mod app;
mod form;
mod materials_form;
mod materials_view;
mod materials_view_model;
mod plot;
mod settings;
mod settings_view;
mod settings_view_model;
mod view;
mod view_model;

#[cfg(test)]
mod ui_tests;

use app::App;
use iced::window;
use iced::Size;
use springcore::MaterialStore;

fn initial_app() -> App {
    let settings = settings::AppSettings::load();
    let (materials, load_warnings) = MaterialStore::load();
    App::from_store(materials, load_warnings, settings.curvature_correction)
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
