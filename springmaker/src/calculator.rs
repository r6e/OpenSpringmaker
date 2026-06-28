//! Family-agnostic Calculator screen shell.
//!
//! Provides the chrome (header, status panel, footer) that wraps any spring
//! family's design and results panels. Currently wires directly to the
//! compression family; Task 6 swaps the inner panels for a family-dispatched call.

use iced::widget::{button, column, container, radio, row, scrollable, space, text};
use iced::{Background, Element, Font, Length};

use crate::app::{App, Message, Screen, C};
use crate::compression::view_model::status_view;
use crate::presenter::{StatusKind, StatusLine};
use crate::widgets::{
    accent_button_style, ghost_button_style, nav_button_style, panel_container, section_divider,
    section_heading, SZ_BODY, SZ_LABEL, SZ_TITLE,
};
use springcore::UnitSystem;

/// Build the complete Calculator screen UI.
pub(crate) fn view(app: &App) -> Element<'_, Message> {
    let header = header(app);
    let left = crate::compression::view::design_panel(app);
    let right = crate::compression::view::results_panel(app);
    let status = status_panel(app);
    let footer = footer();

    let header_divider = section_divider();

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
    .style(|_theme| iced::widget::container::Style {
        background: Some(Background::Color(C::INK)),
        ..Default::default()
    });

    root.into()
}

// --------------------------------------------------------------------------
// Header
// --------------------------------------------------------------------------

fn header(app: &App) -> Element<'_, Message> {
    let app_name = text("OpenSpringmaker")
        .size(SZ_TITLE)
        .color(C::ACCENT)
        .font(Font {
            weight: iced::font::Weight::Semibold,
            ..Font::DEFAULT
        });

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

    let materials_btn = button(text("Materials →").size(SZ_LABEL).color(C::ACCENT))
        .on_press(Message::NavigateTo(Screen::Materials))
        .style(nav_button_style);

    let settings_btn = button(text("Settings →").size(SZ_LABEL).color(C::ACCENT))
        .on_press(Message::NavigateTo(Screen::Settings))
        .style(nav_button_style);

    row![
        app_name,
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
    // The presenter decides suppression, ordering (load warnings first), and
    // each line's severity class; the view maps that class to prefix and color.
    let lines = status_view(app);
    if lines.is_empty() {
        return column![].into();
    }

    // Neutral heading: this panel carries both startup material-load warnings
    // (which can appear before any design is computed) and design-status messages.
    let mut col = column![section_heading("Status")].spacing(6);
    for line in &lines {
        col = col.push(render_status_line(line));
    }

    panel_container(col)
}

fn render_status_line(line: &StatusLine) -> Element<'static, Message> {
    let (prefix, color) = match line.kind {
        StatusKind::ActionError => ("Error:", C::DANGER),
        StatusKind::LoadWarning => ("Warning:", C::WARN),
        StatusKind::Info => ("Info:", C::MUTED),
        StatusKind::Caution => ("Caution:", C::WARN),
        StatusKind::DesignWarning => ("Warning:", C::DANGER),
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

fn footer() -> Element<'static, Message> {
    let save_btn = button(text("Save design").size(SZ_BODY).color(C::INK))
        .on_press(Message::Save)
        .style(accent_button_style);

    let load_btn = button(text("Load design").size(SZ_BODY).color(C::TEXT))
        .on_press(Message::Load)
        .style(ghost_button_style);

    row![save_btn, load_btn].spacing(12).into()
}
