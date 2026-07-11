//! Shared iced widget and style kit used by every screen (calculator, materials,
//! settings). Family- and screen-agnostic presentational helpers; depends only on
//! the app shell's color palette (`Palette`) and `Message`. Screen-specific widgets
//! live in that screen's own view module.

use iced::widget::{column, container, pick_list, radio, row, rule, text, text_input};
use iced::{Background, Border, Color, Element, Font, Length};

use crate::app::{App, Message, Palette, VisualMode};
use crate::presenter::{Emphasis, GoverningRate, ResultRow};

// --------------------------------------------------------------------------
// Font-size constants
// --------------------------------------------------------------------------

pub(crate) const SZ_LABEL: u32 = 13;
pub(crate) const SZ_BODY: u32 = 14;
pub(crate) const SZ_TITLE: u32 = 18;

pub(crate) fn panel_container<'a>(
    pal: &'static Palette,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(content)
        .padding(20)
        .style(move |_theme| iced::widget::container::Style {
            background: Some(Background::Color(pal.panel)),
            border: Border {
                color: pal.line,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .into()
}

pub(crate) fn styled_pick_list<'a, T, L>(
    pal: &'static Palette,
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
        .style(move |_theme, _status| iced::widget::pick_list::Style {
            text_color: pal.text,
            placeholder_color: pal.muted,
            handle_color: pal.muted,
            background: Background::Color(pal.raised),
            border: Border {
                color: pal.line,
                width: 1.0,
                radius: 4.0.into(),
            },
        })
        .menu_style(move |_theme| iced::widget::overlay::menu::Style {
            background: Background::Color(pal.panel),
            border: Border {
                color: pal.line,
                width: 1.0,
                radius: 4.0.into(),
            },
            text_color: pal.text,
            selected_text_color: pal.ink,
            selected_background: Background::Color(pal.accent),
            shadow: iced::Shadow::default(),
        })
        .into()
}

/// Shared text-input style used by both the calculator and materials editor.
pub(crate) fn text_input_style(
    pal: &'static Palette,
) -> impl Fn(&iced::Theme, iced::widget::text_input::Status) -> iced::widget::text_input::Style {
    move |_theme, status| {
        use iced::widget::text_input::Status;
        let focused = matches!(status, Status::Focused { .. });
        iced::widget::text_input::Style {
            background: Background::Color(pal.raised),
            border: Border {
                color: if focused { pal.accent } else { pal.line },
                width: if focused { 1.5 } else { 1.0 },
                radius: 4.0.into(),
            },
            icon: pal.muted,
            placeholder: pal.muted,
            value: pal.text,
            selection: Color {
                a: 0.3,
                ..pal.accent
            },
        }
    }
}

/// A field label in the muted color at 13px.
pub(crate) fn field_label(
    pal: &'static Palette,
    label: impl Into<String>,
) -> Element<'static, Message> {
    text(label.into()).size(SZ_LABEL).color(pal.muted).into()
}

/// The material selector (`Material` label + pick-list), shared by every family's
/// design panel. Reads only app-shell state (`app.materials`, `app.material`), so
/// it carries no family dependency.
pub(crate) fn material_picker(app: &App) -> Element<'_, Message> {
    let pal = app.pal();
    let material_names: Vec<String> = app
        .materials
        .names()
        .into_iter()
        .map(String::from)
        .collect();
    column![
        field_label(pal, "Material"),
        styled_pick_list(
            pal,
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

pub(crate) fn section_divider(pal: &'static Palette) -> Element<'static, Message> {
    rule::horizontal(1)
        .style(move |_theme| iced::widget::rule::Style {
            color: pal.line,
            radius: 0.0.into(),
            fill_mode: iced::widget::rule::FillMode::Full,
            snap: true,
        })
        .into()
}

pub(crate) fn section_heading(
    pal: &'static Palette,
    label: impl Into<String>,
) -> Element<'static, Message> {
    text(label.into())
        .size(SZ_LABEL)
        .color(pal.muted)
        .font(Font {
            weight: iced::font::Weight::Semibold,
            ..Font::DEFAULT
        })
        .into()
}

/// Ghost/outline button style (used for secondary actions).
pub(crate) fn ghost_button_style(
    pal: &'static Palette,
) -> impl Fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    move |_theme, status| {
        use iced::widget::button::Status;
        let border_color = if matches!(status, Status::Hovered) {
            pal.text
        } else {
            pal.line
        };
        iced::widget::button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            text_color: pal.text,
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

/// Danger/destructive ghost button style (remove actions).
pub(crate) fn danger_button_style(
    pal: &'static Palette,
) -> impl Fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    move |_theme, status| {
        use iced::widget::button::Status;
        let border_color = if matches!(status, Status::Hovered) {
            pal.danger
        } else {
            pal.line
        };
        iced::widget::button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            text_color: pal.danger,
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

/// Accent/primary filled button style (save/commit actions).
pub(crate) fn accent_button_style(
    pal: &'static Palette,
) -> impl Fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    move |_theme, status| {
        use iced::widget::button::Status;
        let bg = if matches!(status, Status::Hovered) {
            Color {
                r: pal.accent.r * 0.85,
                g: pal.accent.g * 0.85,
                b: pal.accent.b * 0.85,
                a: 1.0,
            }
        } else {
            pal.accent
        };
        iced::widget::button::Style {
            background: Some(Background::Color(bg)),
            text_color: pal.ink,
            border: Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            shadow: Default::default(),
            snap: Default::default(),
        }
    }
}

/// Accent-outline nav button style (navigation actions with accent color text/border).
pub(crate) fn nav_button_style(
    pal: &'static Palette,
) -> impl Fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    move |_theme, status| {
        use iced::widget::button::Status;
        let border_color = if matches!(status, Status::Hovered) {
            pal.accent
        } else {
            pal.line
        };
        iced::widget::button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            text_color: pal.accent,
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

// ── Results panel helpers ────────────────────────────────────────────────────

/// Caption / fine-print font size for table headers and small annotations.
pub(crate) const SZ_CAPTION: u32 = 11;

/// Hero font size for the governing-rate readout.
pub(crate) const SZ_HERO: u32 = 22;

/// Results panel for the empty (no input) state.
pub(crate) fn results_empty(pal: &'static Palette) -> Element<'static, Message> {
    column![
        section_heading(pal, "Results"),
        text("Enter design parameters to see results.")
            .size(SZ_BODY)
            .color(pal.muted),
    ]
    .spacing(12)
    .into()
}

/// Results panel for the error (solve failed) state.
pub(crate) fn results_error(pal: &'static Palette, msg: String) -> Element<'static, Message> {
    column![
        section_heading(pal, "Results"),
        text(msg).size(SZ_LABEL).color(pal.danger),
    ]
    .spacing(12)
    .into()
}

/// A muted label + mono value row with an explicit value color.
pub(crate) fn result_row_colored<'a>(
    pal: &'static Palette,
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
            .color(pal.muted)
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
    pal: &'static Palette,
    label: impl Into<String>,
    value: impl Into<String>,
    unit: impl Into<String>,
) -> Element<'a, Message> {
    result_row_colored(pal, label, value, unit, pal.text)
}

/// Render one result row, mapping the presenter's emphasis to a color.
pub(crate) fn render_result_row(pal: &'static Palette, r: &ResultRow) -> Element<'static, Message> {
    match r.emphasis {
        Emphasis::Normal => result_row(pal, r.label.clone(), r.value.clone(), r.unit.clone()),
        Emphasis::Danger => result_row_colored(
            pal,
            r.label.clone(),
            r.value.clone(),
            r.unit.clone(),
            pal.danger,
        ),
    }
}

/// A heading followed by result rows (spacing 6), as used by every readout section.
pub(crate) fn rows_section(
    pal: &'static Palette,
    heading: &str,
    rows: &[ResultRow],
) -> iced::widget::Column<'static, Message> {
    let mut col = column![section_heading(pal, heading)].spacing(6);
    for r in rows {
        col = col.push(render_result_row(pal, r));
    }
    col
}

/// A divider, a heading, then result rows (spacing 6) — the fatigue/min-weight
/// section shape. Built flat (not by wrapping `rows_section`) so the
/// divider→heading gap stays at the section's own spacing of 6.
pub(crate) fn divided_result_section(
    pal: &'static Palette,
    heading: &str,
    rows: &[ResultRow],
) -> Element<'static, Message> {
    let mut col = column![section_divider(pal), section_heading(pal, heading)].spacing(6);
    for r in rows {
        col = col.push(render_result_row(pal, r));
    }
    col.into()
}

/// The hero spring-rate readout widget.
pub(crate) fn render_governing_rate(
    pal: &'static Palette,
    label: &str,
    gr: &GoverningRate,
) -> Element<'static, Message> {
    let rate_label = text(label.to_owned()).size(SZ_LABEL).color(pal.muted);
    let rate_value = mono_value(format!("{} {}", gr.value, gr.unit), pal.accent, SZ_HERO);
    column![rate_label, rate_value].spacing(6).into()
}

/// The results panel's shared chart/3D toggle: identical in every family
/// (compression, conical, extension, torsion, assembly) — only the visual it
/// switches between differs, so it collapses to one call rather than five
/// byte-identical copies. `selected` is `app.results_visual`. `_pal` is
/// unused today (the radios carry no palette-dependent styling); Task 4
/// replaces them with the shared `segmented` widget, which needs it.
pub(crate) fn visual_toggle(
    _pal: &'static Palette,
    selected: VisualMode,
) -> Element<'static, Message> {
    row![
        radio("Chart", VisualMode::Chart, Some(selected), Message::Visual).text_size(SZ_LABEL),
        radio("3D", VisualMode::Spring3d, Some(selected), Message::Visual).text_size(SZ_LABEL),
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
    pal: &'static Palette,
    label: &str,
    value: &str,
    id: impl Into<iced::widget::Id>,
    on_input: impl Fn(String) -> Message + 'a,
) -> Element<'a, Message> {
    column![
        field_label(pal, label),
        text_input("", value)
            .id(id.into())
            .on_input(on_input)
            .size(SZ_BODY)
            .font(Font::MONOSPACE)
            .style(text_input_style(pal)),
    ]
    .spacing(4)
    .into()
}

/// Material picker scoped to a single assembly member.
///
/// Renders a `field_label("Material")` + pick-list for member at `index`.
/// The selected value is taken from `app.assembly.members[index].material`.
pub(crate) fn material_picker_for_member(app: &App, index: usize) -> Element<'_, Message> {
    let pal = app.pal();
    let names: Vec<String> = app
        .materials
        .names()
        .into_iter()
        .map(String::from)
        .collect();
    let selected = app.assembly.members.get(index).map(|m| m.material.clone());
    column![
        field_label(pal, "Material"),
        styled_pick_list(pal, names, selected, move |m| Message::AsmMemberMaterial(
            index, m
        )),
    ]
    .spacing(4)
    .into()
}
