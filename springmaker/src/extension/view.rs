//! Humble iced view for the extension spring calculator.
//!
//! All business logic lives in `form` and `view_model`. This module assembles
//! iced widgets from the current [`App`] state, delegating data decisions to
//! the presenter layer (ADR 0008).

use iced::widget::{column, container, radio, text};
use iced::{Element, Length};

use crate::app::{App, Message, C};
use crate::extension::form::{ExtFormState, Field, HookMode};
use crate::extension::view_model::{ext_inputs_view, ext_results_view, ExtResultsView};
use crate::presenter::unit_length_label;
use crate::widgets::{
    labeled_input, panel_container, render_governing_rate, rows_section, section_divider,
    section_heading, styled_pick_list, SZ_BODY, SZ_LABEL,
};

// --------------------------------------------------------------------------
// Design (left) panel
// --------------------------------------------------------------------------

pub(crate) fn design_panel(app: &App) -> Element<'_, Message> {
    let material_names: Vec<String> = app
        .materials
        .names()
        .into_iter()
        .map(String::from)
        .collect();

    // Setup group — material only (no end-type/fixity for extension springs).
    let setup_group = column![
        section_heading("Setup"),
        column![
            crate::widgets::field_label("Material"),
            styled_pick_list(
                material_names,
                Some(app.material.clone()),
                Message::Material
            ),
        ]
        .spacing(4),
    ]
    .spacing(10);

    // Inputs group — driven by the presenter's field list.
    let inputs = ext_inputs_view(app);
    let mut inputs_col = column![section_heading("Inputs")].spacing(12);
    for fd in &inputs {
        let field = fd.field;
        inputs_col = inputs_col.push(labeled_input(
            &fd.label,
            ext_field_value(&app.extension, field),
            ext_field_id(field),
            move |s| Message::ExtField(field, s),
        ));
    }
    let inputs_group = inputs_col;

    // Hooks group — mode toggle + optional custom-radius inputs.
    let hooks_group = hooks_group(app);

    let inner = column![
        setup_group,
        section_divider(),
        inputs_group,
        section_divider(),
        hooks_group,
    ]
    .spacing(16);

    container(panel_container(inner))
        .width(Length::FillPortion(1))
        .into()
}

fn hooks_group(app: &App) -> Element<'_, Message> {
    let len_label = unit_length_label(app.unit_system);
    let mode = app.extension.hook_mode;

    let default_radio = radio(
        "Default (machine loops)",
        HookMode::Default,
        Some(mode),
        Message::ExtHookMode,
    )
    .text_size(SZ_LABEL);

    let custom_radio = radio(
        "Custom radii",
        HookMode::Custom,
        Some(mode),
        Message::ExtHookMode,
    )
    .text_size(SZ_LABEL);

    let mut col = column![
        section_heading("Hook geometry"),
        default_radio,
        custom_radio,
    ]
    .spacing(8);

    if mode == HookMode::Custom {
        col = col
            .push(labeled_input(
                &format!("Hook radius r1 ({len_label})"),
                &app.extension.hook_r1,
                ext_field_id(Field::HookR1),
                |s| Message::ExtField(Field::HookR1, s),
            ))
            .push(labeled_input(
                &format!("Hook radius r2 ({len_label})"),
                &app.extension.hook_r2,
                ext_field_id(Field::HookR2),
                |s| Message::ExtField(Field::HookR2, s),
            ));
    }

    col.into()
}

/// Map an extension [`Field`] to its live string value in the form state.
fn ext_field_value(form: &ExtFormState, field: Field) -> &str {
    match field {
        Field::WireDia => &form.wire_dia,
        Field::MeanDia => &form.mean_dia,
        Field::Active => &form.active,
        Field::FreeLength => &form.free_length,
        Field::InitialTension => &form.initial_tension,
        Field::Loads => &form.loads,
        Field::HookR1 => &form.hook_r1,
        Field::HookR2 => &form.hook_r2,
    }
}

/// Stable widget ID for a calculator extension field's text input.
///
/// IDs are a deliberate stable contract: single source of truth shared by the
/// view and tests; each `Field` renders at most one input per frame.
fn ext_field_id(field: Field) -> &'static str {
    match field {
        Field::WireDia => "ext-wire-dia",
        Field::MeanDia => "ext-mean-dia",
        Field::Active => "ext-active",
        Field::FreeLength => "ext-free-length",
        Field::InitialTension => "ext-initial-tension",
        Field::Loads => "ext-loads",
        Field::HookR1 => "ext-hook-r1",
        Field::HookR2 => "ext-hook-r2",
    }
}

// --------------------------------------------------------------------------
// Results (right) panel
// --------------------------------------------------------------------------

pub(crate) fn results_panel(app: &App) -> Element<'_, Message> {
    let content: Element<'_, Message> = match ext_results_view(app) {
        ExtResultsView::Error(msg) => column![
            section_heading("Results"),
            text(msg).size(SZ_LABEL).color(C::DANGER),
        ]
        .spacing(12)
        .into(),
        ExtResultsView::Empty => column![
            section_heading("Results"),
            text("Enter design parameters to see results.")
                .size(SZ_BODY)
                .color(C::MUTED),
        ]
        .spacing(12)
        .into(),
        ExtResultsView::Populated(p) => column![
            section_heading("Results"),
            section_divider(),
            render_governing_rate(&p.governing_rate),
            section_divider(),
            rows_section("Geometry", &p.geometry),
        ]
        .spacing(6)
        .into(),
    };

    container(panel_container(content))
        .width(Length::FillPortion(1))
        .into()
}
