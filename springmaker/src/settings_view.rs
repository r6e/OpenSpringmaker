//! Humble view for the Settings screen — renders SettingsViewModel only.
//! No logic or branching; all rendering decisions live in `settings_view_model`.

use iced::widget::{button, column, container, row, space, text};
use iced::{Background, Border, Color, Element, Font, Length};

use crate::app::{App, Message, Screen, C};
use crate::settings_view_model::{SettingsFeedbackKind, SettingsViewModel};
use crate::widgets::{
    nav_button_style, panel_container, section_divider, section_heading, SZ_BODY, SZ_LABEL,
    SZ_TITLE,
};

/// Style for a correction-option row: highlighted when selected, muted when not.
fn correction_option_style(
    selected: bool,
) -> impl Fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    move |_theme, status| {
        let is_hovered = matches!(status, iced::widget::button::Status::Hovered);
        let bg_color = if selected {
            Color {
                r: C::ACCENT.r * 0.15,
                g: C::ACCENT.g * 0.15,
                b: C::ACCENT.b * 0.15,
                a: 1.0,
            }
        } else if is_hovered {
            Color {
                r: C::RAISED.r + 0.05,
                g: C::RAISED.g + 0.05,
                b: C::RAISED.b + 0.05,
                a: 1.0,
            }
        } else {
            Color::TRANSPARENT
        };
        let border_color = if selected { C::ACCENT } else { C::LINE };
        iced::widget::button::Style {
            background: Some(Background::Color(bg_color)),
            text_color: if selected { C::ACCENT } else { C::TEXT },
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 4.0.into(),
            },
            shadow: Default::default(),
            snap: Default::default(),
        }
    }
}

/// Build the Settings screen.
pub(crate) fn view(app: &App) -> Element<'_, Message> {
    let vm = SettingsViewModel::from_app(app);

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

    // Build correction-option buttons. Each option emits SetCorrection on press;
    // the presenter's `selected` flag drives visual differentiation. Using
    // `button(text(label))` (rather than iced's `radio`) makes the label text a
    // first-class `Candidate::Text` widget, which the iced_test Simulator can
    // locate and click by label in headless tests.
    let mut options_col = column![
        section_heading("Curvature-correction factor"),
        section_divider(),
    ]
    .spacing(8);

    // Extract save_feedback before consuming vm.options into option_data.
    let save_feedback = vm.save_feedback;

    // Collect into owned tuples so no reference to `vm.options` escapes into
    // the element tree (Element<'_> must not borrow from the local ViewModel).
    let option_data: Vec<(springcore::CurvatureCorrection, String, bool)> = vm
        .options
        .into_iter()
        .map(|o| (o.value, o.label, o.selected))
        .collect();

    for (value, label, selected) in option_data {
        let label_text = text(label).size(SZ_BODY);
        let btn = button(label_text)
            .on_press(Message::SetCorrection(value))
            .style(correction_option_style(selected))
            .width(Length::Fill)
            .padding([8, 12]);
        options_col = options_col.push(btn);
    }

    let correction_panel: Element<'_, Message> = container(panel_container(options_col))
        .width(Length::Fill)
        .into();

    let mut content = column![header, section_divider(), correction_panel]
        .spacing(16)
        .max_width(800);

    // Surface a settings-save error below the correction panel (spec §5).
    // The in-memory preference still applies regardless of this status.
    if let Some(fb) = save_feedback {
        let color = match fb.kind {
            SettingsFeedbackKind::Error => C::DANGER,
        };
        content = content.push(text(fb.text).size(SZ_LABEL).color(color));
    }

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
