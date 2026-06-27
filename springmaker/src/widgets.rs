//! Shared iced widget and style kit used by every screen (calculator, materials,
//! settings). Family- and screen-agnostic presentational helpers; depends only on
//! the app shell's color palette (`C`) and `Message`. Screen-specific widgets live
//! in that screen's own view module.

use iced::widget::{container, pick_list, rule, text};
use iced::{Background, Border, Color, Element, Font, Length};

use crate::app::{Message, C};

// --------------------------------------------------------------------------
// Font-size constants
// --------------------------------------------------------------------------

pub(crate) const SZ_LABEL: u32 = 13;
pub(crate) const SZ_BODY: u32 = 14;
pub(crate) const SZ_TITLE: u32 = 18;

pub(crate) fn panel_container<'a>(
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(content)
        .padding(20)
        .style(|_theme| iced::widget::container::Style {
            background: Some(Background::Color(C::PANEL)),
            border: Border {
                color: C::LINE,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .into()
}

pub(crate) fn styled_pick_list<'a, T, L>(
    options: L,
    selected: Option<T>,
    on_select: impl Fn(T) -> Message + 'a,
) -> Element<'a, Message>
where
    T: ToString + PartialEq + Clone + 'a,
    L: std::borrow::Borrow<[T]> + 'a,
{
    pick_list(options, selected, on_select)
        .width(Length::Fill)
        .text_size(SZ_BODY)
        .style(|_theme, _status| iced::widget::pick_list::Style {
            text_color: C::TEXT,
            placeholder_color: C::MUTED,
            handle_color: C::MUTED,
            background: Background::Color(C::RAISED),
            border: Border {
                color: C::LINE,
                width: 1.0,
                radius: 4.0.into(),
            },
        })
        .menu_style(|_theme| iced::widget::overlay::menu::Style {
            background: Background::Color(C::PANEL),
            border: Border {
                color: C::LINE,
                width: 1.0,
                radius: 4.0.into(),
            },
            text_color: C::TEXT,
            selected_text_color: C::INK,
            selected_background: Background::Color(C::ACCENT),
            shadow: iced::Shadow::default(),
        })
        .into()
}

/// Shared text-input style used by both the calculator and materials editor.
pub(crate) fn text_input_style(
    _theme: &iced::Theme,
    status: iced::widget::text_input::Status,
) -> iced::widget::text_input::Style {
    use iced::widget::text_input::Status;
    let focused = matches!(status, Status::Focused { .. });
    iced::widget::text_input::Style {
        background: Background::Color(C::RAISED),
        border: Border {
            color: if focused { C::ACCENT } else { C::LINE },
            width: if focused { 1.5 } else { 1.0 },
            radius: 4.0.into(),
        },
        icon: C::MUTED,
        placeholder: C::MUTED,
        value: C::TEXT,
        selection: Color {
            a: 0.3,
            ..C::ACCENT
        },
    }
}

/// A field label in the muted color at 13px.
pub(crate) fn field_label(label: impl Into<String>) -> Element<'static, Message> {
    text(label.into()).size(SZ_LABEL).color(C::MUTED).into()
}

/// A mono-spaced data value with color control.
pub(crate) fn mono_value(
    value: impl Into<String>,
    color: Color,
    size: u32,
) -> Element<'static, Message> {
    text(value.into())
        .font(Font::MONOSPACE)
        .size(size)
        .color(color)
        .into()
}

pub(crate) fn section_divider() -> Element<'static, Message> {
    rule::horizontal(1)
        .style(|_theme| iced::widget::rule::Style {
            color: C::LINE,
            radius: 0.0.into(),
            fill_mode: iced::widget::rule::FillMode::Full,
            snap: true,
        })
        .into()
}

pub(crate) fn section_heading(label: impl Into<String>) -> Element<'static, Message> {
    text(label.into())
        .size(SZ_LABEL)
        .color(C::MUTED)
        .font(Font {
            weight: iced::font::Weight::Semibold,
            ..Font::DEFAULT
        })
        .into()
}

/// Ghost/outline button style (used for secondary actions).
pub(crate) fn ghost_button_style(
    _theme: &iced::Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    use iced::widget::button::Status;
    let border_color = if matches!(status, Status::Hovered) {
        C::TEXT
    } else {
        C::LINE
    };
    iced::widget::button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: C::TEXT,
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 4.0.into(),
        },
        shadow: Default::default(),
        snap: Default::default(),
    }
}

/// Danger/destructive ghost button style (remove actions).
pub(crate) fn danger_button_style(
    _theme: &iced::Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    use iced::widget::button::Status;
    let border_color = if matches!(status, Status::Hovered) {
        C::DANGER
    } else {
        C::LINE
    };
    iced::widget::button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: C::DANGER,
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 4.0.into(),
        },
        shadow: Default::default(),
        snap: Default::default(),
    }
}

/// Accent/primary filled button style (save/commit actions).
pub(crate) fn accent_button_style(
    _theme: &iced::Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    use iced::widget::button::Status;
    let bg = if matches!(status, Status::Hovered) {
        Color {
            r: C::ACCENT.r * 0.85,
            g: C::ACCENT.g * 0.85,
            b: C::ACCENT.b * 0.85,
            a: 1.0,
        }
    } else {
        C::ACCENT
    };
    iced::widget::button::Style {
        background: Some(Background::Color(bg)),
        text_color: C::INK,
        border: Border {
            radius: 4.0.into(),
            ..Default::default()
        },
        shadow: Default::default(),
        snap: Default::default(),
    }
}

/// Accent-outline nav button style (navigation actions with accent color text/border).
pub(crate) fn nav_button_style(
    _theme: &iced::Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    use iced::widget::button::Status;
    let border_color = if matches!(status, Status::Hovered) {
        C::ACCENT
    } else {
        C::LINE
    };
    iced::widget::button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: C::ACCENT,
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 4.0.into(),
        },
        shadow: Default::default(),
        snap: Default::default(),
    }
}
