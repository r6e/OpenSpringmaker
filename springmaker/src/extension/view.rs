//! Humble iced view for the extension spring calculator.
//!
//! All business logic lives in `form` and `view_model`. This module assembles
//! iced widgets from the current [`App`] state, delegating data decisions to
//! the presenter layer (ADR 0008).

use iced::widget::{column, container, radio, row, text};
use iced::{Element, Font, Length};

use crate::app::{App, Message, C};
use crate::extension::form::{ExtFormState, Field, HookMode, ALL_EXT_SCENARIOS};
use crate::extension::view_model::{
    ext_inputs_view, ext_results_view, ExtLoadTable, ExtResultsView,
};
use crate::presenter::unit_length_label;
use crate::widgets::{
    field_label, labeled_input, panel_container, render_governing_rate, results_empty,
    results_error, rows_section, section_divider, section_heading, styled_pick_list, SZ_CAPTION,
    SZ_LABEL,
};

// --------------------------------------------------------------------------
// Design (left) panel
// --------------------------------------------------------------------------

pub(crate) fn design_panel(app: &App) -> Element<'_, Message> {
    // Setup group — material + scenario picker (no end-type/fixity for extension springs).
    let setup_group = column![
        section_heading("Setup"),
        crate::widgets::material_picker(app),
        column![
            field_label("Scenario"),
            styled_pick_list(
                ALL_EXT_SCENARIOS,
                Some(app.extension.scenario),
                Message::ExtScenario
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

    // Hooks group — mode toggle + optional custom-radius inputs.
    let hooks_group = hooks_group(app);

    let inner = column![
        setup_group,
        section_divider(),
        inputs_col,
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
        Field::Rate => &form.rate,
        Field::HookR1 => &form.hook_r1,
        Field::HookR2 => &form.hook_r2,
    }
}

/// Stable widget ID for a calculator extension field's text input. The inputs
/// are empty by default, so headless Simulator tests can't target them by text
/// content and select by this id instead. An explicit, exhaustive match (rather
/// than a `Debug`-derived string) keeps the ids a deliberate stable contract,
/// avoids a per-render allocation, and forces a choice when a `Field` is added.
/// Single source of truth shared by the view and the tests, which resolve their
/// target inputs through this function (see `type_into_ext`) rather than
/// hardcoding the strings, so the two cannot drift. Each `Field` renders at most
/// one input per frame.
pub(crate) fn ext_field_id(field: Field) -> &'static str {
    match field {
        Field::WireDia => "ext-wire-dia",
        Field::MeanDia => "ext-mean-dia",
        Field::Active => "ext-active",
        Field::FreeLength => "ext-free-length",
        Field::InitialTension => "ext-initial-tension",
        Field::Loads => "ext-loads",
        Field::Rate => "ext-rate",
        Field::HookR1 => "ext-hook-r1",
        Field::HookR2 => "ext-hook-r2",
    }
}

// --------------------------------------------------------------------------
// Results (right) panel — renderers
// --------------------------------------------------------------------------

fn render_ext_load_table(lt: &ExtLoadTable) -> Element<'static, Message> {
    let mut col = column![section_heading("Load points")].spacing(4);

    col = col.push(
        row![
            text("Pt")
                .size(SZ_CAPTION)
                .color(C::MUTED)
                .width(Length::Fixed(24.0)),
            text("Force")
                .size(SZ_CAPTION)
                .color(C::MUTED)
                .width(Length::FillPortion(2)),
            text("Deflection")
                .size(SZ_CAPTION)
                .color(C::MUTED)
                .width(Length::FillPortion(2)),
            text("Length")
                .size(SZ_CAPTION)
                .color(C::MUTED)
                .width(Length::FillPortion(2)),
            text(format!("Body \u{03c4} ({})", lt.stress_unit))
                .size(SZ_CAPTION)
                .color(C::MUTED)
                .width(Length::FillPortion(2)),
            text(format!("Hook \u{03c3} ({})", lt.stress_unit))
                .size(SZ_CAPTION)
                .color(C::MUTED)
                .width(Length::FillPortion(2)),
            text(format!("Hook \u{03c4} ({})", lt.stress_unit))
                .size(SZ_CAPTION)
                .color(C::MUTED)
                .width(Length::FillPortion(2)),
            text("%\u{03c4}_body")
                .size(SZ_CAPTION)
                .color(C::MUTED)
                .width(Length::FillPortion(1)),
            text("%\u{03c3}")
                .size(SZ_CAPTION)
                .color(C::MUTED)
                .width(Length::FillPortion(1)),
            text("%\u{03c4}_hook")
                .size(SZ_CAPTION)
                .color(C::MUTED)
                .width(Length::FillPortion(1)),
        ]
        .spacing(4),
    );

    for lp in &lt.rows {
        let data_row = row![
            text(lp.point.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(C::MUTED)
                .width(Length::Fixed(24.0)),
            text(lp.force.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(C::TEXT)
                .width(Length::FillPortion(2)),
            text(lp.deflection.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(C::TEXT)
                .width(Length::FillPortion(2)),
            text(lp.length.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(C::TEXT)
                .width(Length::FillPortion(2)),
            text(lp.body_shear.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(C::TEXT)
                .width(Length::FillPortion(2)),
            text(lp.hook_bending.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(C::TEXT)
                .width(Length::FillPortion(2)),
            text(lp.hook_torsion.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(C::TEXT)
                .width(Length::FillPortion(2)),
            text(lp.pct_body.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(C::TEXT)
                .width(Length::FillPortion(1)),
            text(lp.pct_bending.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(C::TEXT)
                .width(Length::FillPortion(1)),
            text(lp.pct_torsion.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(C::TEXT)
                .width(Length::FillPortion(1)),
        ]
        .spacing(4);
        col = col.push(data_row);
    }

    col.into()
}

// --------------------------------------------------------------------------
// Results (right) panel
// --------------------------------------------------------------------------

pub(crate) fn results_panel(app: &App) -> Element<'_, Message> {
    let content: Element<'_, Message> = match ext_results_view(app) {
        ExtResultsView::Error(msg) => results_error(msg),
        ExtResultsView::Empty => results_empty(),
        ExtResultsView::Populated(p) => column![
            section_heading("Results"),
            section_divider(),
            render_governing_rate(&p.governing_rate),
            section_divider(),
            rows_section("Geometry", &p.geometry),
            section_divider(),
            render_ext_load_table(&p.load_table),
        ]
        .spacing(6)
        .into(),
    };

    container(panel_container(content))
        .width(Length::FillPortion(1))
        .into()
}
