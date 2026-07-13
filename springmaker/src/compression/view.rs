//! Layout for the OpenSpringmaker GUI. Pure view logic — no computation here.
//!
//! All business logic lives in `form` and `springcore`. This module only
//! assembles iced widgets from the current [`App`] state.

use iced::widget::{column, container, row, text};
use iced::{Element, Font, Length};

use crate::app::{App, Message, Palette};
use crate::compression::form::{Field, ALL_SCENARIOS};
use crate::compression::view_model::{
    fatigue_chart_data, inputs_view, results_view, FatigueView, MinWeightView, PopulatedResults,
    ResultsView,
};
use crate::picker::{find_by_key, KeyLabel, END_TYPES, FIXITIES};
use crate::presenter::{FieldDescriptor, LoadTable};
use crate::widgets::{
    divided_note, divided_result_section, emphasis_color, field_label, labeled_input,
    panel_container, render_governing_rate, results_empty, results_error, rows_section,
    section_divider, section_heading, styled_pick_list, visual_toggle, COL_PT, SP_LG, SP_MD,
    SP_ROW, SP_XS, SZ_CAPTION, SZ_LABEL,
};

// --------------------------------------------------------------------------
// Style helpers
// --------------------------------------------------------------------------

/// Stable widget id for a calculator field's text input. The inputs are empty by
/// default, so headless Simulator tests can't target them by text content and
/// select by this id instead. An explicit, exhaustive match (rather than a
/// `Debug`-derived string) keeps the ids a deliberate stable contract, avoids a
/// per-render allocation, and forces a choice when a `Field` is added. Single
/// source of truth shared by the view and the tests; each `Field` renders at
/// most one input per frame (the scenario-driven input set never repeats a field).
pub(crate) fn calc_field_id(field: Field) -> &'static str {
    match field {
        Field::WireDia => "calc-wire-dia",
        Field::MeanDia => "calc-mean-dia",
        Field::OuterDia => "calc-outer-dia",
        Field::Active => "calc-active",
        Field::FreeLength => "calc-free-length",
        Field::Rate => "calc-rate",
        Field::Loads => "calc-loads",
        Field::Force1 => "calc-force1",
        Field::Length1 => "calc-length1",
        Field::Force2 => "calc-force2",
        Field::Length2 => "calc-length2",
        Field::FatigueMin => "calc-fatigue-min",
        Field::FatigueMax => "calc-fatigue-max",
        Field::MaxForce => "calc-max-force",
        Field::IndexMin => "calc-index-min",
        Field::IndexMax => "calc-index-max",
        Field::MaxOuterDia => "calc-max-outer-dia",
        Field::CandidateDiameters => "calc-candidate-diameters",
        Field::ClashAllowance => "calc-clash-allowance",
    }
}

// --------------------------------------------------------------------------
// Design (left) panel
// --------------------------------------------------------------------------

pub(crate) fn design_panel(app: &App) -> Element<'_, Message> {
    let pal = app.pal();
    let selected_end = find_by_key(END_TYPES, &app.form.end_type).copied();
    let selected_fix = find_by_key(FIXITIES, &app.form.fixity).copied();

    // Setup group — two columns: material+scenario left, end_type+fixity right.
    let setup_col_a = column![
        crate::widgets::material_picker(app),
        column![
            field_label(pal, "Scenario"),
            styled_pick_list(
                pal,
                ALL_SCENARIOS,
                Some(app.form.scenario),
                Message::Scenario
            ),
        ]
        .spacing(SP_XS),
    ]
    .spacing(SP_MD)
    .width(Length::FillPortion(1));

    let setup_col_b = column![
        column![
            field_label(pal, "End type"),
            styled_pick_list(pal, END_TYPES, selected_end, |kl: KeyLabel| {
                Message::EndType(kl.key.to_string())
            }),
        ]
        .spacing(SP_XS),
        column![
            field_label(pal, "Fixity"),
            styled_pick_list(pal, FIXITIES, selected_fix, |kl: KeyLabel| {
                Message::Fixity(kl.key.to_string())
            }),
        ]
        .spacing(SP_XS),
    ]
    .spacing(SP_MD)
    .width(Length::FillPortion(1));

    let setup_row = row![setup_col_a, setup_col_b].spacing(SP_MD);

    let setup_group = column![section_heading(pal, "Setup"), setup_row,].spacing(SP_MD);

    let inputs_group = build_inputs_group(app);

    let inner = column![setup_group, section_divider(pal), inputs_group,].spacing(SP_LG);

    container(panel_container(pal, inner))
        .width(Length::FillPortion(1))
        .into()
}

fn build_inputs_group(app: &App) -> Element<'_, Message> {
    let pal = app.pal();
    // The presenter decides which fields appear for the scenario and their
    // unit-aware labels; the live value for each field is bound here from
    // `app.form` (iced's `text_input` borrows its value).
    let inputs = inputs_view(app);

    let mut col = column![section_heading(pal, "Inputs")].spacing(SP_MD);
    for fd in &inputs.primary {
        col = col.push(render_input(app, fd));
    }

    if !inputs.fatigue.is_empty() {
        col = col
            .push(section_divider(pal))
            .push(section_heading(pal, "Fatigue cycle (leave blank to skip)"));
        for fd in &inputs.fatigue {
            col = col.push(render_input(app, fd));
        }
    }

    col.into()
}

/// Render one descriptor as a labeled input, binding the live value from `app.form`.
fn render_input<'a>(app: &'a App, fd: &FieldDescriptor<Field>) -> Element<'a, Message> {
    let pal = app.pal();
    let field = fd.field;
    labeled_input(
        pal,
        &fd.label,
        field_value(&app.form, field),
        calc_field_id(field),
        move |s| Message::CompField(field, s),
    )
}

/// Map a [`Field`] to its current string value in the form state.
fn field_value(form: &crate::compression::form::FormState, field: Field) -> &str {
    match field {
        Field::WireDia => &form.wire_dia,
        Field::MeanDia => &form.mean_dia,
        Field::OuterDia => &form.outer_dia,
        Field::Active => &form.active,
        Field::FreeLength => &form.free_length,
        Field::Rate => &form.rate,
        Field::Loads => &form.loads,
        Field::Force1 => &form.force1,
        Field::Length1 => &form.length1,
        Field::Force2 => &form.force2,
        Field::Length2 => &form.length2,
        Field::FatigueMin => &form.fatigue_min,
        Field::FatigueMax => &form.fatigue_max,
        Field::MaxForce => &form.max_force,
        Field::IndexMin => &form.index_min,
        Field::IndexMax => &form.index_max,
        Field::MaxOuterDia => &form.max_outer_dia,
        Field::CandidateDiameters => &form.candidate_diameters,
        Field::ClashAllowance => &form.clash_allowance,
    }
}

// --------------------------------------------------------------------------
// Results (right) panel — renderers (data from view_model::results_view)
// --------------------------------------------------------------------------

fn render_load_table(pal: &'static Palette, lt: &LoadTable) -> Element<'static, Message> {
    let mut load_col = column![section_heading(pal, "Load points")].spacing(SP_XS);

    load_col = load_col.push(
        row![
            text("Pt")
                .size(SZ_CAPTION)
                .color(pal.muted)
                .width(Length::Fixed(COL_PT)),
            text("Force")
                .size(SZ_CAPTION)
                .color(pal.muted)
                .width(Length::FillPortion(2)),
            text("Deflection")
                .size(SZ_CAPTION)
                .color(pal.muted)
                .width(Length::FillPortion(2)),
            text("Length")
                .size(SZ_CAPTION)
                .color(pal.muted)
                .width(Length::FillPortion(2)),
            text(format!("Stress ({})", lt.stress_unit))
                .size(SZ_CAPTION)
                .color(pal.muted)
                .width(Length::FillPortion(2)),
            text("% MTS")
                .size(SZ_CAPTION)
                .color(pal.muted)
                .width(Length::FillPortion(1)),
        ]
        .spacing(SP_XS),
    );

    for lp in &lt.rows {
        let stress_color = emphasis_color(pal, lp.stress_emphasis);
        let load_row = row![
            text(lp.point.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(pal.muted)
                .width(Length::Fixed(COL_PT)),
            text(lp.force.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(pal.text)
                .width(Length::FillPortion(2)),
            text(lp.deflection.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(pal.text)
                .width(Length::FillPortion(2)),
            text(lp.length.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(pal.text)
                .width(Length::FillPortion(2)),
            text(lp.stress.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(stress_color)
                .width(Length::FillPortion(2)),
            text(lp.pct_mts.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(stress_color)
                .width(Length::FillPortion(1)),
        ]
        .spacing(SP_XS);
        load_col = load_col.push(load_row);
    }

    load_col.into()
}

// --------------------------------------------------------------------------
// Results (right) panel
// --------------------------------------------------------------------------

pub(crate) fn results_panel(app: &App) -> Element<'_, Message> {
    let pal = app.pal();
    let us = app.unit_system;

    let content: Element<'_, Message> = match results_view(app) {
        ResultsView::Error(msg) => results_error(pal, msg),
        ResultsView::Empty => results_empty(pal),
        ResultsView::Populated(p) => {
            // The results panel's shared visual slot: chart or orbitable 3D
            // scene, selected by `app.results_visual`. Each visual is pure
            // rendering of the design (no decision), built from the outcome
            // the Populated variant guarantees is present — and built ONLY in
            // its own arm, so exactly one load-deflection bitmap is
            // rasterized per render (orbit drags re-render every frame; an
            // eagerly-built chart would be thrown away each time).
            let outcome = app
                .outcome
                .as_ref()
                .expect("ResultsView::Populated implies app.outcome is Some");
            let visual: Element<'_, Message> = match app.results_visual {
                crate::app::VisualMode::Chart => crate::plot::chart_element(
                    pal,
                    crate::compression::plot_model::compression_chart(&outcome.design, us),
                ),
                crate::app::VisualMode::Spring3d => crate::viz::spring3d_element(
                    pal,
                    crate::compression::scene_model::compression_scene(&outcome.design),
                    crate::viz::sdf::compression_sdf(&outcome.design),
                    app.orbit,
                    app.zoom,
                    app.shader_available,
                ),
            };
            let toggle = visual_toggle(pal, app.results_visual);

            // The presenter decides whether a fatigue chart exists (it stays
            // hidden with the fatigue rows on min-weight runs); the view only
            // renders the data it hands back. Reuses the `outcome` binding
            // above rather than re-deriving it from `app.outcome`.
            let fatigue_chart =
                fatigue_chart_data(outcome, us).map(|d| crate::plot::chart_element(pal, d));

            render_populated(pal, &p, toggle, visual, fatigue_chart)
        }
    };

    container(panel_container(pal, content))
        .width(Length::FillPortion(1))
        .into()
}

/// Assemble the populated results column from the presenter data plus the
/// chart/3D toggle and the selected visual.
fn render_populated<'a>(
    pal: &'static Palette,
    p: &PopulatedResults,
    toggle: Element<'a, Message>,
    visual: Element<'a, Message>,
    fatigue_chart: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let mut col = column![
        section_heading(pal, "Results"),
        section_divider(pal),
        render_governing_rate(pal, "Spring rate", &p.governing_rate),
        section_divider(pal),
        rows_section(pal, "Geometry", &p.geometry),
        section_divider(pal),
        render_load_table(pal, &p.load_table),
        section_divider(pal),
        toggle,
        visual,
    ]
    .spacing(SP_ROW);

    match &p.fatigue {
        FatigueView::Hidden => {}
        FatigueView::Computed(rows) => {
            col = col.push(divided_result_section(pal, "Fatigue analysis", rows));
        }
        FatigueView::Note(msg) => {
            col = col.push(divided_note(pal, msg));
        }
    }
    if let Some(fc) = fatigue_chart {
        col = col.push(fc);
    }
    if let MinWeightView::Shown(rows) = &p.min_weight {
        col = col.push(divided_result_section(pal, "Min-weight optimisation", rows));
    }

    col.into()
}
