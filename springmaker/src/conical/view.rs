//! Conical humble view (ADR 0008): renders presenter output, no logic.
//!
//! Mirrors the structure of `compression::view` and `torsion::view`.

use iced::widget::{column, container, text};
use iced::{Element, Length};

use crate::app::{App, Message, C};
use crate::conical::form::Field;
use crate::conical::view_model::{
    con_inputs_view, con_results_view, ConPopulatedResults, ConResultsView, CON_LINEAR_MODEL_NOTE,
};
use crate::picker::{find_by_key, END_TYPES};
use crate::presenter::LoadTable;
use crate::widgets::{
    field_label, labeled_input, material_picker, panel_container, render_governing_rate,
    results_empty, results_error, rows_section, section_divider, section_heading, styled_pick_list,
    visual_toggle, SZ_CAPTION, SZ_LABEL,
};
use iced::widget::row;
use iced::Font;

// --------------------------------------------------------------------------
// Design (left) panel
// --------------------------------------------------------------------------

pub(crate) fn design_panel(app: &App) -> Element<'_, Message> {
    use crate::picker::KeyLabel;

    let selected_end = find_by_key(END_TYPES, &app.conical.end_type).copied();

    let setup_group = column![
        section_heading("Setup"),
        material_picker(app),
        column![
            field_label("End type"),
            styled_pick_list(END_TYPES, selected_end, |kl: KeyLabel| {
                Message::ConEndType(kl.key.to_string())
            }),
        ]
        .spacing(4),
    ]
    .spacing(10);

    let inputs = con_inputs_view(app);
    let mut inputs_col = column![section_heading("Inputs")].spacing(12);
    for fd in &inputs {
        let field = fd.field;
        inputs_col = inputs_col.push(labeled_input(
            &fd.label,
            con_field_value(&app.conical, field),
            con_field_id(field),
            move |s| Message::ConField(field, s),
        ));
    }

    let inner = column![setup_group, section_divider(), inputs_col].spacing(16);

    container(panel_container(inner))
        .width(Length::FillPortion(1))
        .into()
}

/// Map a conical [`Field`] to its live string value in the form state.
fn con_field_value(form: &crate::conical::form::ConFormState, field: Field) -> &str {
    match field {
        Field::WireDia => &form.wire_dia,
        Field::LargeMeanDia => &form.large_mean_dia,
        Field::SmallMeanDia => &form.small_mean_dia,
        Field::Active => &form.active,
        Field::FreeLength => &form.free_length,
        Field::Loads => &form.loads,
    }
}

/// Stable widget ID for a conical field's text input.
pub(crate) fn con_field_id(field: Field) -> &'static str {
    match field {
        Field::WireDia => "con-wire-dia",
        Field::LargeMeanDia => "con-large-mean-dia",
        Field::SmallMeanDia => "con-small-mean-dia",
        Field::Active => "con-active",
        Field::FreeLength => "con-free-length",
        Field::Loads => "con-loads",
    }
}

// --------------------------------------------------------------------------
// Results (right) panel
// --------------------------------------------------------------------------

pub(crate) fn results_panel(app: &App) -> Element<'_, Message> {
    let us = app.unit_system;
    let content: Element<'_, Message> = match con_results_view(app) {
        ConResultsView::Error(msg) => results_error(msg),
        ConResultsView::Empty => results_empty(),
        ConResultsView::Populated(p) => {
            // The results panel's shared visual slot: chart or orbitable 3D
            // scene, selected by `app.results_visual`. Each visual is pure
            // rendering of the design (no decision), built from the outcome
            // the Populated variant guarantees is present — and built ONLY in
            // its own arm, so exactly one bitmap is rasterized per render
            // (orbit drags re-render every frame; an eagerly-built chart
            // would be thrown away each time).
            let outcome = app
                .con_outcome
                .as_ref()
                .expect("ConResultsView::Populated implies app.con_outcome is Some");
            let visual: Element<'_, Message> = match app.results_visual {
                crate::app::VisualMode::Chart => crate::plot::chart_element(
                    crate::conical::plot_model::conical_chart(&outcome.design, us),
                ),
                crate::app::VisualMode::Spring3d => crate::viz::scene_element(
                    crate::conical::scene_model::conical_scene(&outcome.design),
                    app.orbit,
                ),
            };
            let toggle = visual_toggle(app.results_visual);

            render_populated(&p, toggle, visual)
        }
    };

    container(panel_container(content))
        .width(Length::FillPortion(1))
        .into()
}

/// Render the populated conical results: hero rate → Geometry → load table →
/// chart/3D toggle → selected visual → footer note. Status is handled by the
/// calculator's shared status panel (as siblings do — see
/// `calculator::status_panel`).
fn render_populated<'a>(
    p: &ConPopulatedResults,
    toggle: Element<'a, Message>,
    visual: Element<'a, Message>,
) -> Element<'a, Message> {
    column![
        section_heading("Results"),
        section_divider(),
        render_governing_rate(&p.governing_rate),
        section_divider(),
        rows_section("Geometry", &p.geometry),
        section_divider(),
        render_con_load_table(&p.load_table),
        section_divider(),
        toggle,
        visual,
        render_linear_model_footer(),
    ]
    .spacing(6)
    .into()
}

/// The load-point table. Mirrors compression's `render_load_table` exactly.
fn render_con_load_table(lt: &LoadTable) -> Element<'static, Message> {
    let mut load_col = column![section_heading("Load points")].spacing(4);

    load_col = load_col.push(
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
            text(format!("Stress ({})", lt.stress_unit))
                .size(SZ_CAPTION)
                .color(C::MUTED)
                .width(Length::FillPortion(2)),
            text("%MTS")
                .size(SZ_CAPTION)
                .color(C::MUTED)
                .width(Length::FillPortion(1)),
        ]
        .spacing(4),
    );

    for lp in &lt.rows {
        let load_row = row![
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
            text(lp.stress.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(C::TEXT)
                .width(Length::FillPortion(2)),
            text(lp.pct_mts.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(C::TEXT)
                .width(Length::FillPortion(1)),
        ]
        .spacing(4);
        load_col = load_col.push(load_row);
    }

    load_col.into()
}

/// The always-present linear-model disclosure footer.
/// Mirrors compression's `render_fatigue` Note arm idiom exactly.
fn render_linear_model_footer() -> Element<'static, Message> {
    column![
        section_divider(),
        text(CON_LINEAR_MODEL_NOTE).size(SZ_LABEL).color(C::MUTED),
    ]
    .spacing(8)
    .into()
}
