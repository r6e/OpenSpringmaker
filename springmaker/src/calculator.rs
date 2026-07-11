//! Family-agnostic Calculator screen shell.
//!
//! Provides the chrome (header, status panel, footer) that wraps any spring
//! family's design and results panels. The active family is dispatched via
//! [`App::family`]; the inner panels are supplied by the family's own view module.

use iced::widget::{button, column, container, radio, row, scrollable, space, text};
use iced::{Background, Element, Font, Length};

use crate::app::{App, Message, Palette, Screen};
use crate::presenter::{StatusKind, StatusLine};
use crate::widgets::{
    accent_button_style, ghost_button_style, nav_button_style, panel_container, section_divider,
    section_heading, styled_pick_list, SZ_BODY, SZ_LABEL, SZ_TITLE,
};
use springcore::{Family, UnitSystem, ALL_FAMILIES};

/// Build the complete Calculator screen UI.
pub(crate) fn view(app: &App) -> Element<'_, Message> {
    let pal = app.pal();
    let header = header(app);
    let (left, right) = match app.family {
        Family::Compression => (
            crate::compression::view::design_panel(app),
            crate::compression::view::results_panel(app),
        ),
        Family::Extension => (
            crate::extension::view::design_panel(app),
            crate::extension::view::results_panel(app),
        ),
        Family::Torsion => (
            crate::torsion::view::design_panel(app),
            crate::torsion::view::results_panel(app),
        ),
        Family::Conical => (
            crate::conical::view::design_panel(app),
            crate::conical::view::results_panel(app),
        ),
        Family::Assembly => (
            crate::assembly::view::design_panel(app),
            crate::assembly::view::results_panel(app),
        ),
    };
    let status = status_panel(app);
    let footer = footer(pal);

    let header_divider = section_divider(pal);

    let content = column![
        header,
        header_divider,
        row![left, right].spacing(16),
        status,
        footer,
    ]
    .spacing(16)
    .max_width(1200);

    let root = container(scrollable(
        container(content).padding(24).width(Length::Fill),
    ))
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_theme| iced::widget::container::Style {
        background: Some(Background::Color(pal.ink)),
        ..Default::default()
    });

    root.into()
}

// --------------------------------------------------------------------------
// Header
// --------------------------------------------------------------------------

fn header(app: &App) -> Element<'_, Message> {
    let pal = app.pal();
    let app_name = text("OpenSpringmaker")
        .size(SZ_TITLE)
        .color(pal.accent)
        .font(Font {
            weight: iced::font::Weight::Semibold,
            ..Font::DEFAULT
        });

    let family_selector = container(styled_pick_list(
        pal,
        ALL_FAMILIES.to_vec(),
        Some(app.family),
        Message::SelectFamily,
    ))
    .width(Length::Fixed(180.0));

    let unit_metric = radio(
        "Metric (mm, N)",
        UnitSystem::Metric,
        Some(app.unit_system),
        Message::Units,
    )
    .text_size(SZ_LABEL);

    let unit_us = radio(
        "US (in, lbf)",
        UnitSystem::Us,
        Some(app.unit_system),
        Message::Units,
    )
    .text_size(SZ_LABEL);

    let materials_btn = button(text("Materials →").size(SZ_LABEL).color(pal.accent))
        .on_press(Message::NavigateTo(Screen::Materials))
        .style(nav_button_style(pal));

    let settings_btn = button(text("Settings →").size(SZ_LABEL).color(pal.accent))
        .on_press(Message::NavigateTo(Screen::Settings))
        .style(nav_button_style(pal));

    row![
        app_name,
        space().width(Length::Fixed(160.0)),
        family_selector,
        space().width(Length::Fill),
        materials_btn,
        settings_btn,
        unit_metric,
        unit_us,
    ]
    .spacing(16)
    .align_y(iced::Alignment::Center)
    .into()
}

// --------------------------------------------------------------------------
// Status panel
// --------------------------------------------------------------------------

fn status_panel(app: &App) -> Element<'_, Message> {
    let pal = app.pal();
    let lines = match app.family {
        Family::Compression => crate::compression::view_model::status_view(app),
        Family::Extension => crate::extension::view_model::ext_status_view(app),
        Family::Torsion => crate::torsion::view_model::tor_status_view(app),
        Family::Conical => crate::conical::view_model::con_status_view(app),
        Family::Assembly => crate::assembly::view_model::asm_status_view(app),
    };

    if lines.is_empty() {
        return column![].into();
    }

    let mut col = column![section_heading(pal, "Status")].spacing(6);
    for line in &lines {
        col = col.push(render_status_line(pal, line));
    }

    panel_container(pal, col)
}

fn render_status_line(pal: &'static Palette, line: &StatusLine) -> Element<'static, Message> {
    let (prefix, color) = match line.kind {
        StatusKind::ActionError => ("Error:", pal.danger),
        StatusKind::LoadWarning => ("Warning:", pal.warn),
        StatusKind::Info => ("Info:", pal.muted),
        StatusKind::Caution => ("Caution:", pal.warn),
        StatusKind::DesignWarning => ("Warning:", pal.danger),
    };
    row![
        text(prefix)
            .size(SZ_LABEL)
            .color(color)
            .width(Length::Fixed(72.0)),
        text(line.text.clone()).size(SZ_LABEL).color(color),
    ]
    .spacing(8)
    .into()
}

// --------------------------------------------------------------------------
// Footer
// --------------------------------------------------------------------------

fn footer(pal: &'static Palette) -> Element<'static, Message> {
    let save_btn = button(text("Save design").size(SZ_BODY).color(pal.ink))
        .on_press(Message::Save)
        .style(accent_button_style(pal));

    let load_btn = button(text("Load design").size(SZ_BODY).color(pal.text))
        .on_press(Message::Load)
        .style(ghost_button_style(pal));

    row![save_btn, load_btn].spacing(12).into()
}
