//! Layout for the OpenSpringmaker GUI. Pure view logic — no computation here.
//!
//! All business logic lives in `form` and `springcore`. This module only
//! assembles iced widgets from the current [`App`] state.

use iced::widget::{column, container, row, text};
use iced::{Element, Font, Length};

use crate::app::{App, Message, C};
use crate::compression::form::{Field, ALL_SCENARIOS};
use crate::compression::view_model::{
    inputs_view, results_view, FatigueView, MinWeightView, PopulatedResults, ResultsView,
};
use crate::picker::{find_by_key, KeyLabel, END_TYPES};
use crate::presenter::{FieldDescriptor, LoadTable};
use crate::widgets::{
    divided_result_section, field_label, labeled_input, panel_container, render_governing_rate,
    results_empty, results_error, rows_section, section_divider, section_heading, styled_pick_list,
    SZ_CAPTION, SZ_LABEL,
};

// --------------------------------------------------------------------------
// Fixity pick-list items (compression-only; end-types are shared via picker)
// --------------------------------------------------------------------------

/// All fixity options in display order.
const FIXITIES: &[KeyLabel] = &[
    KeyLabel {
        key: "fixed_fixed",
        label: "Fixed-Fixed",
    },
    KeyLabel {
        key: "fixed_pinned",
        label: "Fixed-Pinned",
    },
    KeyLabel {
        key: "pinned_pinned",
        label: "Pinned-Pinned",
    },
    KeyLabel {
        key: "fixed_free",
        label: "Fixed-Free",
    },
];

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
    let selected_end = find_by_key(END_TYPES, &app.form.end_type).copied();
    let selected_fix = find_by_key(FIXITIES, &app.form.fixity).copied();

    // Setup group — two columns: material+scenario left, end_type+fixity right.
    let setup_col_a = column![
        crate::widgets::material_picker(app),
        column![
            field_label("Scenario"),
            styled_pick_list(ALL_SCENARIOS, Some(app.form.scenario), Message::Scenario),
        ]
        .spacing(4),
    ]
    .spacing(12)
    .width(Length::FillPortion(1));

    let setup_col_b = column![
        column![
            field_label("End type"),
            styled_pick_list(END_TYPES, selected_end, |kl: KeyLabel| {
                Message::EndType(kl.key.to_string())
            }),
        ]
        .spacing(4),
        column![
            field_label("Fixity"),
            styled_pick_list(FIXITIES, selected_fix, |kl: KeyLabel| {
                Message::Fixity(kl.key.to_string())
            }),
        ]
        .spacing(4),
    ]
    .spacing(12)
    .width(Length::FillPortion(1));

    let setup_row = row![setup_col_a, setup_col_b].spacing(12);

    let setup_group = column![section_heading("Setup"), setup_row,].spacing(10);

    let inputs_group = build_inputs_group(app);

    let inner = column![setup_group, section_divider(), inputs_group,].spacing(16);

    container(panel_container(inner))
        .width(Length::FillPortion(1))
        .into()
}

fn build_inputs_group(app: &App) -> Element<'_, Message> {
    // The presenter decides which fields appear for the scenario and their
    // unit-aware labels; the live value for each field is bound here from
    // `app.form` (iced's `text_input` borrows its value).
    let inputs = inputs_view(app);

    let mut col = column![section_heading("Inputs")].spacing(12);
    for fd in &inputs.primary {
        col = col.push(render_input(app, fd));
    }

    if !inputs.fatigue.is_empty() {
        col = col
            .push(section_divider())
            .push(section_heading("Fatigue cycle (leave blank to skip)"));
        for fd in &inputs.fatigue {
            col = col.push(render_input(app, fd));
        }
    }

    col.into()
}

/// Render one descriptor as a labeled input, binding the live value from `app.form`.
fn render_input<'a>(app: &'a App, fd: &FieldDescriptor<Field>) -> Element<'a, Message> {
    let field = fd.field;
    labeled_input(
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

fn render_load_table(lt: &LoadTable) -> Element<'static, Message> {
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

fn render_fatigue(fv: &FatigueView) -> Element<'static, Message> {
    match fv {
        FatigueView::Hidden => column![].into(),
        FatigueView::Computed(rows) => divided_result_section("Fatigue analysis", rows),
        FatigueView::Note(msg) => {
            column![section_divider(), text(*msg).size(SZ_LABEL).color(C::MUTED),]
                .spacing(8)
                .into()
        }
    }
}

fn render_min_weight(mv: &MinWeightView) -> Element<'static, Message> {
    match mv {
        MinWeightView::Hidden => column![].into(),
        MinWeightView::Shown(rows) => divided_result_section("Min-weight optimisation", rows),
    }
}

// --------------------------------------------------------------------------
// Results (right) panel
// --------------------------------------------------------------------------

pub(crate) fn results_panel(app: &App) -> Element<'_, Message> {
    let us = app.unit_system;

    let content: Element<'_, Message> = match results_view(app) {
        ResultsView::Error(msg) => results_error(msg),
        ResultsView::Empty => results_empty(),
        ResultsView::Populated(p) => {
            // The chart is pure rendering of the design (no decision); build it
            // from the outcome the Populated variant guarantees is present.
            let chart = app
                .outcome
                .as_ref()
                .map(|o| crate::plot::results_chart(&o.design, us))
                .expect("ResultsView::Populated implies app.outcome is Some");

            render_populated(&p, chart)
        }
    };

    container(panel_container(content))
        .width(Length::FillPortion(1))
        .into()
}

/// Assemble the populated results column from the presenter data plus the chart.
fn render_populated<'a>(p: &PopulatedResults, chart: Element<'a, Message>) -> Element<'a, Message> {
    column![
        section_heading("Results"),
        section_divider(),
        render_governing_rate(&p.governing_rate),
        section_divider(),
        rows_section("Geometry", &p.geometry),
        section_divider(),
        render_load_table(&p.load_table),
        render_fatigue(&p.fatigue),
        render_min_weight(&p.min_weight),
        section_divider(),
        chart,
    ]
    .spacing(6)
    .into()
}
