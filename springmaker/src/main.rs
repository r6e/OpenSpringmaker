mod app;
mod form;
mod view;

use app::App;

fn main() -> iced::Result {
    iced::run("OpenSpringmaker", App::update, App::view)
}
