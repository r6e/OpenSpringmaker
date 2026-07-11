//! Shared iced widget and style kit used by every screen (calculator, materials,
//! settings). Family- and screen-agnostic presentational helpers; depends only on
//! the app shell's color palette (`C`) and `Message`. Screen-specific widgets live
//! in that screen's own view module.

use iced::widget::{column, container, pick_list, radio, row, rule, text, text_input};
use iced::{Background, Border, Color, Element, Font, Length};

use crate::app::{App, Message, VisualMode, C};
use crate::presenter::{Emphasis, GoverningRate, ResultRow};

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

/// The material selector (`Material` label + pick-list), shared by every family's
/// design panel. Reads only app-shell state (`app.materials`, `app.material`), so
/// it carries no family dependency.
pub(crate) fn material_picker(app: &App) -> Element<'_, Message> {
    let material_names: Vec<String> = app
        .materials
        .names()
        .into_iter()
        .map(String::from)
        .collect();
    column![
        field_label("Material"),
        styled_pick_list(
            material_names,
            Some(app.material.clone()),
            Message::Material
        ),
    ]
    .spacing(4)
    .into()
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

// ── Results panel helpers ────────────────────────────────────────────────────

/// Caption / fine-print font size for table headers and small annotations.
pub(crate) const SZ_CAPTION: u32 = 11;

/// Hero font size for the governing-rate readout.
pub(crate) const SZ_HERO: u32 = 22;

/// Results panel for the empty (no input) state.
pub(crate) fn results_empty() -> Element<'static, Message> {
    column![
        section_heading("Results"),
        text("Enter design parameters to see results.")
            .size(SZ_BODY)
            .color(C::MUTED),
    ]
    .spacing(12)
    .into()
}

/// Results panel for the error (solve failed) state.
pub(crate) fn results_error(msg: String) -> Element<'static, Message> {
    column![
        section_heading("Results"),
        text(msg).size(SZ_LABEL).color(C::DANGER),
    ]
    .spacing(12)
    .into()
}

/// A muted label + mono value row with an explicit value color.
pub(crate) fn result_row_colored<'a>(
    label: impl Into<String>,
    value: impl Into<String>,
    unit: impl Into<String>,
    value_color: Color,
) -> Element<'a, Message> {
    let value = value.into();
    let unit = unit.into();
    let display = if unit.is_empty() {
        value
    } else {
        format!("{value} {unit}")
    };
    row![
        text(label.into())
            .size(SZ_LABEL)
            .color(C::MUTED)
            .width(Length::FillPortion(2)),
        text(display)
            .font(Font::MONOSPACE)
            .size(SZ_BODY)
            .color(value_color)
            .width(Length::FillPortion(3)),
    ]
    .spacing(8)
    .into()
}

/// A muted label + mono value row in standard text color, used in results readouts.
pub(crate) fn result_row<'a>(
    label: impl Into<String>,
    value: impl Into<String>,
    unit: impl Into<String>,
) -> Element<'a, Message> {
    result_row_colored(label, value, unit, C::TEXT)
}

/// Render one result row, mapping the presenter's emphasis to a color.
pub(crate) fn render_result_row(r: &ResultRow) -> Element<'static, Message> {
    match r.emphasis {
        Emphasis::Normal => result_row(r.label.clone(), r.value.clone(), r.unit.clone()),
        Emphasis::Danger => {
            result_row_colored(r.label.clone(), r.value.clone(), r.unit.clone(), C::DANGER)
        }
    }
}

/// A heading followed by result rows (spacing 6), as used by every readout section.
pub(crate) fn rows_section(
    heading: &str,
    rows: &[ResultRow],
) -> iced::widget::Column<'static, Message> {
    let mut col = column![section_heading(heading)].spacing(6);
    for r in rows {
        col = col.push(render_result_row(r));
    }
    col
}

/// A divider, a heading, then result rows (spacing 6) — the fatigue/min-weight
/// section shape. Built flat (not by wrapping `rows_section`) so the
/// divider→heading gap stays at the section's own spacing of 6.
pub(crate) fn divided_result_section(
    heading: &str,
    rows: &[ResultRow],
) -> Element<'static, Message> {
    let mut col = column![section_divider(), section_heading(heading)].spacing(6);
    for r in rows {
        col = col.push(render_result_row(r));
    }
    col.into()
}

/// The hero spring-rate readout widget.
pub(crate) fn render_governing_rate(gr: &GoverningRate) -> Element<'static, Message> {
    let rate_label = text("Spring rate").size(SZ_LABEL).color(C::MUTED);
    let rate_value = mono_value(format!("{} {}", gr.value, gr.unit), C::ACCENT, SZ_HERO);
    column![rate_label, rate_value].spacing(6).into()
}

/// The results panel's shared chart/3D toggle: identical in every family
/// (compression, conical, extension, torsion, assembly) — only the visual it
/// switches between differs, so it collapses to one call rather than five
/// byte-identical copies. `selected` is `app.results_visual`.
pub(crate) fn visual_toggle(selected: VisualMode) -> Element<'static, Message> {
    row![
        radio("Chart", VisualMode::Chart, Some(selected), Message::Visual).text_size(SZ_LABEL),
        radio(
            "3D",
            VisualMode::Spring3d,
            Some(selected),
            Message::Visual
        )
        .text_size(SZ_LABEL),
    ]
    .spacing(12)
    .into()
}

// ── Labeled input ────────────────────────────────────────────────────────────

/// A labeled text input: muted label above a styled monospace input. The caller
/// supplies the widget id and the message constructor, so all families reuse this.
/// `id` accepts any type that converts to [`iced::widget::Id`]: `&'static str`,
/// `String`, or a pre-built `Id`. Existing `&'static str` callers are unaffected.
pub(crate) fn labeled_input<'a>(
    label: &str,
    value: &str,
    id: impl Into<iced::widget::Id>,
    on_input: impl Fn(String) -> Message + 'a,
) -> Element<'a, Message> {
    column![
        field_label(label),
        text_input("", value)
            .id(id.into())
            .on_input(on_input)
            .size(SZ_BODY)
            .font(Font::MONOSPACE)
            .style(text_input_style),
    ]
    .spacing(4)
    .into()
}

/// Material picker scoped to a single assembly member.
///
/// Renders a `field_label("Material")` + pick-list for member at `index`.
/// The selected value is taken from `app.assembly.members[index].material`.
pub(crate) fn material_picker_for_member(app: &App, index: usize) -> Element<'_, Message> {
    let names: Vec<String> = app
        .materials
        .names()
        .into_iter()
        .map(String::from)
        .collect();
    let selected = app.assembly.members.get(index).map(|m| m.material.clone());
    column![
        field_label("Material"),
        styled_pick_list(names, selected, move |m| Message::AsmMemberMaterial(
            index, m
        )),
    ]
    .spacing(4)
    .into()
}
