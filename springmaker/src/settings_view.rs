//! Humble view for the Settings screen — renders SettingsViewModel only.
//! No logic or branching; all rendering decisions live in `settings_view_model`.

use iced::widget::{button, column, container, row, space, text};
use iced::{Background, Border, Color, Element, Font, Length};

use crate::app::{App, Message, Palette, Screen};
use crate::settings_view_model::{SettingsFeedbackKind, SettingsViewModel};
use crate::widgets::{
    nav_button_style, panel_container, section_divider, section_heading, SP_LG, SP_MD, SP_SM,
    SP_XL, SZ_BODY, SZ_LABEL, SZ_TITLE,
};

/// Style for a correction-option row: highlighted when selected, muted when not.
fn correction_option_style(
    pal: &'static Palette,
    selected: bool,
) -> impl Fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    move |_theme, status| {
        let is_hovered = matches!(status, iced::widget::button::Status::Hovered);
        let bg_color = if selected {
            Color {
                r: pal.accent.r * 0.15,
                g: pal.accent.g * 0.15,
                b: pal.accent.b * 0.15,
                a: 1.0,
            }
        } else if is_hovered {
            Color {
                r: pal.raised.r + 0.05,
                g: pal.raised.g + 0.05,
                b: pal.raised.b + 0.05,
                a: 1.0,
            }
        } else {
            Color::TRANSPARENT
        };
        let border_color = if selected { pal.accent } else { pal.line };
        iced::widget::button::Style {
            background: Some(Background::Color(bg_color)),
            text_color: if selected { pal.accent } else { pal.text },
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
    let pal = app.pal();
    let vm = SettingsViewModel::from_app(app);

    let back_btn = button(text("\u{2190} Calculator").size(SZ_LABEL).color(pal.accent))
        .on_press(Message::NavigateTo(Screen::Calculator))
        .style(nav_button_style(pal));

    let title = text("Settings").size(SZ_TITLE).color(pal.text).font(Font {
        weight: iced::font::Weight::Semibold,
        ..Font::DEFAULT
    });

    let header = row![title, space().width(Length::Fill), back_btn]
        .spacing(SP_LG)
        .align_y(iced::Alignment::Center);

    // Build correction-option buttons. Each option emits SetCorrection on press;
    // the presenter's `selected` flag drives visual differentiation. Using
    // `button(text(label))` (rather than iced's `radio`) makes the label text a
    // first-class `Candidate::Text` widget, which the iced_test Simulator can
    // locate and click by label in headless tests.
    let mut options_col = column![
        section_heading(pal, "Curvature-correction factor"),
        section_divider(pal),
    ]
    .spacing(SP_SM);

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
            .style(correction_option_style(pal, selected))
            .width(Length::Fill)
            .padding([SP_SM, SP_MD]);
        options_col = options_col.push(btn);
    }

    let correction_panel: Element<'_, Message> = container(panel_container(pal, options_col))
        .width(Length::Fill)
        .into();

    let mut content = column![header, section_divider(pal), correction_panel]
        .spacing(SP_LG)
        .max_width(800);

    // Surface a settings-save error below the correction panel (spec §5).
    // The in-memory preference still applies regardless of this status.
    if let Some(fb) = save_feedback {
        let color = match fb.kind {
            SettingsFeedbackKind::Error => pal.danger,
        };
        content = content.push(text(fb.text).size(SZ_LABEL).color(color));
    }

    let root = container(
        container(content)
            .padding(SP_XL)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_theme| iced::widget::container::Style {
        background: Some(Background::Color(pal.ink)),
        ..Default::default()
    });

    root.into()
}
