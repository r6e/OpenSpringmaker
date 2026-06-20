mod app;
mod form;
mod plot;
mod view;

use app::App;
use iced::window;
use iced::Size;

fn main() -> iced::Result {
    iced::application("OpenSpringmaker", App::update, App::view)
        .theme(App::theme)
        .window(window::Settings {
            size: Size::new(1200.0, 820.0),
            min_size: Some(Size::new(720.0, 600.0)),
            ..Default::default()
        })
        .run()
}
