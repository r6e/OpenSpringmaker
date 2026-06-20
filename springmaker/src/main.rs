mod app;
mod form;
mod plot;
mod view;

use app::App;

fn main() -> iced::Result {
    iced::run("OpenSpringmaker", App::update, App::view)
}
