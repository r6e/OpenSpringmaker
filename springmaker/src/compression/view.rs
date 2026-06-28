//! Layout for the OpenSpringmaker GUI. Pure view logic — no computation here.
//!
//! All business logic lives in `form` and `springcore`. This module only
//! assembles iced widgets from the current [`App`] state.

use iced::widget::{button, column, container, radio, row, scrollable, space, text, text_input};
use iced::{Background, Color, Element, Font, Length};

use crate::app::{App, Message, C};
use crate::compression::form::{Field, ALL_SCENARIOS};
use crate::compression::view_model::{
    inputs_view, results_view, status_view, FatigueView, GoverningRate, MinWeightView,
    PopulatedResults, ResultsView,
};
use crate::presenter::{Emphasis, FieldDescriptor, LoadTable, ResultRow, StatusKind, StatusLine};
use crate::widgets::{
    accent_button_style, field_label, ghost_button_style, mono_value, nav_button_style,
    panel_container, section_divider, section_heading, styled_pick_list, text_input_style, SZ_BODY,
    SZ_LABEL, SZ_TITLE,
};
use springcore::UnitSystem;

// --------------------------------------------------------------------------
// Font-size constants
// --------------------------------------------------------------------------

const SZ_CAPTION: u32 = 11;
const SZ_HERO: u32 = 22;

// --------------------------------------------------------------------------
// KeyLabel newtype for pick-list items
// --------------------------------------------------------------------------

/// A (key, label) pair for end-type and fixity pick-lists.
///
/// The `Display` impl renders the human-readable label; the key is used to
/// store the value in `FormState` and round-trip through save/load.
#[derive(Clone, Copy, PartialEq, Eq)]
struct KeyLabel {
    key: &'static str,
    label: &'static str,
}

impl std::fmt::Display for KeyLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label)
    }
}

/// All end-type options in display order.
const END_TYPES: &[KeyLabel] = &[
    KeyLabel {
        key: "plain",
        label: "Plain",
    },
    KeyLabel {
        key: "plain_ground",
        label: "Plain ground",
    },
    KeyLabel {
        key: "squared",
        label: "Squared",
    },
    KeyLabel {
        key: "squared_ground",
        label: "Squared and ground",
    },
];

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

/// Find a `KeyLabel` by its stored key string. Returns `None` if the key is
/// unrecognised (e.g. a future format loaded into an older binary).
fn find_by_key<'a>(options: &'a [KeyLabel], key: &str) -> Option<&'a KeyLabel> {
    options.iter().find(|kl| kl.key == key)
}

// --------------------------------------------------------------------------
// Style helpers
// --------------------------------------------------------------------------

fn styled_text_input<'a>(placeholder: &str, value: &str, field: Field) -> Element<'a, Message> {
    text_input(placeholder, value)
        .id(calc_field_id(field))
        .on_input(move |s| Message::Field(field, s))
        .size(SZ_BODY)
        .font(Font::MONOSPACE)
        .style(text_input_style)
        .into()
}

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

/// A labeled input: muted label above a styled text_input.
fn labeled_input<'a>(label: &str, value: &str, field: Field) -> Element<'a, Message> {
    column![
        field_label(label.to_owned()),
        styled_text_input("", value, field),
    ]
    .spacing(4)
    .into()
}

/// A muted label + mono value row with an explicit value color.
fn result_row_colored<'a>(
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
fn result_row<'a>(
    label: impl Into<String>,
    value: impl Into<String>,
    unit: impl Into<String>,
) -> Element<'a, Message> {
    result_row_colored(label, value, unit, C::TEXT)
}

// --------------------------------------------------------------------------
// Top-level view
// --------------------------------------------------------------------------

/// Build the complete application UI.
pub fn view(app: &App) -> Element<'_, Message> {
    let header = build_header(app);
    let left = build_design_panel(app);
    let right = build_results_panel(app);
    let status = build_status_panel(app);
    let footer = build_footer();

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

fn build_header(app: &App) -> Element<'_, Message> {
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
        .on_press(Message::NavigateTo(crate::app::Screen::Materials))
        .style(nav_button_style);

    let settings_btn = button(text("Settings →").size(SZ_LABEL).color(C::ACCENT))
        .on_press(Message::NavigateTo(crate::app::Screen::Settings))
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
// Design (left) panel
// --------------------------------------------------------------------------

fn build_design_panel(app: &App) -> Element<'_, Message> {
    let material_names: Vec<String> = app
        .materials
        .names()
        .into_iter()
        .map(String::from)
        .collect();

    let selected_end = find_by_key(END_TYPES, &app.form.end_type).copied();
    let selected_fix = find_by_key(FIXITIES, &app.form.fixity).copied();

    // Setup group — two columns: material+scenario left, end_type+fixity right.
    let setup_col_a = column![
        column![
            field_label("Material"),
            styled_pick_list(
                material_names,
                Some(app.material.clone()),
                Message::Material,
            ),
        ]
        .spacing(4),
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
    labeled_input(&fd.label, field_value(&app.form, fd.field), fd.field)
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

/// Render one result row, mapping the presenter's emphasis to a color.
fn render_result_row(r: &ResultRow) -> Element<'static, Message> {
    match r.emphasis {
        Emphasis::Normal => result_row(r.label.clone(), r.value.clone(), r.unit.clone()),
        Emphasis::Danger => {
            result_row_colored(r.label.clone(), r.value.clone(), r.unit.clone(), C::DANGER)
        }
    }
}

/// A heading followed by result rows (spacing 6), as used by every readout section.
fn rows_section(heading: &str, rows: &[ResultRow]) -> iced::widget::Column<'static, Message> {
    let mut col = column![section_heading(heading)].spacing(6);
    for r in rows {
        col = col.push(render_result_row(r));
    }
    col
}

/// A divider, a heading, then result rows (spacing 6) — the fatigue/min-weight
/// section shape. Built flat (not by wrapping `rows_section`) so the
/// divider→heading gap stays at the section's own spacing of 6.
fn divided_result_section(heading: &str, rows: &[ResultRow]) -> Element<'static, Message> {
    let mut col = column![section_divider(), section_heading(heading)].spacing(6);
    for r in rows {
        col = col.push(render_result_row(r));
    }
    col.into()
}

fn render_governing_rate(gr: &GoverningRate) -> Element<'static, Message> {
    let rate_label = text("Spring rate").size(SZ_LABEL).color(C::MUTED);
    let rate_value = mono_value(format!("{} {}", gr.value, gr.unit), C::ACCENT, SZ_HERO);
    column![rate_label, rate_value].spacing(6).into()
}

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

fn build_results_panel(app: &App) -> Element<'_, Message> {
    let us = app.unit_system;

    let content: Element<'_, Message> = match results_view(app) {
        ResultsView::Error(msg) => column![
            section_heading("Results"),
            text(msg).size(SZ_LABEL).color(C::DANGER),
        ]
        .spacing(12)
        .into(),
        ResultsView::Empty => column![
            section_heading("Results"),
            text("Enter design parameters to see results.")
                .size(SZ_BODY)
                .color(C::MUTED),
        ]
        .spacing(12)
        .into(),
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

// --------------------------------------------------------------------------
// Status panel
// --------------------------------------------------------------------------

fn build_status_panel(app: &App) -> Element<'_, Message> {
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

fn build_footer() -> Element<'static, Message> {
    let save_btn = button(text("Save design").size(SZ_BODY).color(C::INK))
        .on_press(Message::Save)
        .style(accent_button_style);

    let load_btn = button(text("Load design").size(SZ_BODY).color(C::TEXT))
        .on_press(Message::Load)
        .style(ghost_button_style);

    row![save_btn, load_btn].spacing(12).into()
}
