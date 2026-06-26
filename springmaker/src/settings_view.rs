//! Humble view for the Settings screen — renders SettingsViewModel only.
//! No logic or branching; all rendering decisions live in `settings_view_model`.

use iced::widget::{button, column, container, radio, row, space, text};
use iced::{Background, Element, Font, Length};

use crate::app::{App, Message, Screen, C};
use crate::settings_view_model::SettingsViewModel;
use crate::view::{
    nav_button_style, panel_container, section_divider, section_heading, SZ_BODY, SZ_LABEL,
    SZ_TITLE,
};

/// Build the Settings screen.
pub(crate) fn view(app: &App) -> Element<'_, Message> {
    let vm = SettingsViewModel::from_correction(app.correction);

    let back_btn = button(text("\u{2190} Calculator").size(SZ_LABEL).color(C::ACCENT))
        .on_press(Message::NavigateTo(Screen::Calculator))
        .style(nav_button_style);

    let title = text("Settings").size(SZ_TITLE).color(C::TEXT).font(Font {
        weight: iced::font::Weight::Semibold,
        ..Font::DEFAULT
    });

    let header = row![title, space().width(Length::Fill), back_btn]
        .spacing(16)
        .align_y(iced::Alignment::Center);

    // Build radio group. Each option carries a `selected` bool from the presenter;
    // passing `opt.selected.then_some(opt.value)` to iced's radio means exactly the
    // active option sees `Some(value)` (renders selected), the rest see `None`.
    let mut options_col = column![
        section_heading("Curvature-correction factor"),
        section_divider(),
    ]
    .spacing(10);

    for opt in &vm.options {
        options_col = options_col.push(
            radio(
                opt.label.as_str(),
                opt.value,
                opt.selected.then_some(opt.value),
                Message::SetCorrection,
            )
            .text_size(SZ_BODY),
        );
    }

    let correction_panel: Element<'_, Message> = container(panel_container(options_col))
        .width(Length::Fill)
        .into();

    let content = column![header, section_divider(), correction_panel]
        .spacing(16)
        .max_width(800);

    let root = container(
        container(content)
            .padding(24)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_theme| iced::widget::container::Style {
        background: Some(Background::Color(C::INK)),
        ..Default::default()
    });

    root.into()
}
