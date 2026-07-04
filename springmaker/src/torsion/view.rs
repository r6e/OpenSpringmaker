//! Humble iced view for the torsion spring calculator.
//!
//! All business logic lives in `form` and `view_model`. This module assembles
//! iced widgets from the current [`App`] state, delegating data decisions to
//! the presenter layer (ADR 0008).

use iced::widget::{column, container, row, text};
use iced::{Element, Font, Length};

use crate::app::{App, Message, C};
use crate::presenter::Emphasis;
use crate::torsion::form::{Field, TorFormState, TorScenarioKind};
use crate::torsion::form::{ALL_MOMENT_ENTRIES, ALL_TOR_SCENARIOS};
use crate::torsion::view_model::{tor_inputs_view, tor_results_view, TorLoadTable, TorResultsView};
use crate::widgets::{
    field_label, labeled_input, material_picker, panel_container, render_result_row, results_empty,
    results_error, rows_section, section_divider, section_heading, styled_pick_list, SZ_CAPTION,
    SZ_LABEL,
};

// --------------------------------------------------------------------------
// Design (left) panel
// --------------------------------------------------------------------------

pub(crate) fn design_panel(app: &App) -> Element<'_, Message> {
    // Setup group — material selector, scenario pick-list, optional moment-entry
    // pick-list (hidden for TwoLoad), and friction model pick-list.
    let scenario_col = column![
        field_label("Input mode"),
        styled_pick_list(
            ALL_TOR_SCENARIOS,
            Some(app.torsion.scenario),
            Message::TorScenario,
        ),
    ]
    .spacing(4);

    let mut setup_group =
        column![section_heading("Setup"), material_picker(app), scenario_col].spacing(10);

    if app.torsion.scenario != TorScenarioKind::TwoLoad {
        setup_group = setup_group.push(
            column![
                field_label("Moment entry"),
                styled_pick_list(
                    ALL_MOMENT_ENTRIES,
                    Some(app.torsion.moment_entry),
                    Message::TorMomentEntry,
                ),
            ]
            .spacing(4),
        );
    }

    setup_group = setup_group.push(
        column![
            field_label("Friction model"),
            styled_pick_list(
                springcore::torsion::ALL_FRICTION_MODELS,
                Some(app.torsion.friction_model),
                Message::TorFriction,
            ),
        ]
        .spacing(4),
    );

    // Inputs group — driven by the presenter's field list.
    let inputs = tor_inputs_view(app);
    let mut inputs_col = column![section_heading("Inputs")].spacing(12);
    for fd in &inputs {
        let field = fd.field;
        inputs_col = inputs_col.push(labeled_input(
            &fd.label,
            tor_field_value(&app.torsion, field),
            tor_field_id(field),
            move |s| Message::TorField(field, s),
        ));
    }

    let inner = column![setup_group, section_divider(), inputs_col].spacing(16);

    container(panel_container(inner))
        .width(Length::FillPortion(1))
        .into()
}

/// Map a torsion [`Field`] to its live string value in the form state.
fn tor_field_value(form: &TorFormState, field: Field) -> &str {
    match field {
        Field::WireDia => &form.wire_dia,
        Field::MeanDia => &form.mean_dia,
        Field::OuterDia => &form.outer_dia,
        Field::BodyCoils => &form.body_coils,
        Field::Rate => &form.rate,
        Field::Leg1 => &form.leg1,
        Field::Leg2 => &form.leg2,
        Field::ArborDia => &form.arbor_dia,
        Field::Moments => &form.moments,
        Field::Moment1 => &form.moment1,
        Field::Angle1 => &form.angle1,
        Field::Moment2 => &form.moment2,
        Field::Angle2 => &form.angle2,
        Field::Forces => &form.forces,
        Field::LoadRadius => &form.load_radius,
    }
}

/// Stable widget ID for a torsion field's text input. Single source of truth shared
/// by the view and Simulator tests, which resolve widget ids through this fn.
/// An explicit, exhaustive match avoids `Debug`-derived strings and forces a
/// deliberate choice when a `Field` variant is added.
pub(crate) fn tor_field_id(field: Field) -> &'static str {
    match field {
        Field::WireDia => "tor-wire-dia",
        Field::MeanDia => "tor-mean-dia",
        Field::OuterDia => "tor-outer-dia",
        Field::BodyCoils => "tor-body-coils",
        Field::Rate => "tor-rate",
        Field::Leg1 => "tor-leg1",
        Field::Leg2 => "tor-leg2",
        Field::ArborDia => "tor-arbor-dia",
        Field::Moments => "tor-moments",
        Field::Moment1 => "tor-moment1",
        Field::Angle1 => "tor-angle1",
        Field::Moment2 => "tor-moment2",
        Field::Angle2 => "tor-angle2",
        Field::Forces => "tor-forces",
        Field::LoadRadius => "tor-load-radius",
    }
}

// --------------------------------------------------------------------------
// Results (right) panel — renderers
// --------------------------------------------------------------------------

fn render_tor_load_table(lt: &TorLoadTable) -> Element<'static, Message> {
    let mut col = column![section_heading("Load points")].spacing(4);

    // Header row.
    col = col.push(
        row![
            text("Pt")
                .size(SZ_CAPTION)
                .color(C::MUTED)
                .width(Length::Fixed(24.0)),
            text("Moment")
                .size(SZ_CAPTION)
                .color(C::MUTED)
                .width(Length::FillPortion(3)),
            text("Deflection")
                .size(SZ_CAPTION)
                .color(C::MUTED)
                .width(Length::FillPortion(3)),
            text(format!("Stress ({})", lt.stress_unit))
                .size(SZ_CAPTION)
                .color(C::MUTED)
                .width(Length::FillPortion(2)),
            text("% Allow")
                .size(SZ_CAPTION)
                .color(C::MUTED)
                .width(Length::FillPortion(2)),
            text("Wound ID")
                .size(SZ_CAPTION)
                .color(C::MUTED)
                .width(Length::FillPortion(2)),
        ]
        .spacing(4),
    );

    // Data rows — stress and % allow are colored by emphasis.
    for lp in &lt.rows {
        let stress_color = match lp.stress_emphasis {
            Emphasis::Normal => C::TEXT,
            Emphasis::Danger => C::DANGER,
        };

        let data_row = row![
            text(lp.point.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(C::MUTED)
                .width(Length::Fixed(24.0)),
            text(lp.moment.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(C::TEXT)
                .width(Length::FillPortion(3)),
            text(lp.deflection.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(C::TEXT)
                .width(Length::FillPortion(3)),
            text(lp.stress.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(stress_color)
                .width(Length::FillPortion(2)),
            text(lp.pct_allow.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(stress_color)
                .width(Length::FillPortion(2)),
            text(lp.wound_inner.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(C::TEXT)
                .width(Length::FillPortion(2)),
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
    let content: Element<'_, Message> = match tor_results_view(app) {
        TorResultsView::Error(msg) => results_error(msg),
        TorResultsView::Empty => results_empty(),
        TorResultsView::Populated(p) => {
            // Angular rate section — two ResultRows (per-degree and per-revolution).
            let mut rate_col = column![section_heading("Angular rate")].spacing(6);
            rate_col = rate_col.push(render_result_row(&p.rate_per_deg));
            rate_col = rate_col.push(render_result_row(&p.rate_per_turn));

            column![
                section_heading("Results"),
                section_divider(),
                rate_col,
                section_divider(),
                rows_section("Geometry", &p.geometry),
                section_divider(),
                render_tor_load_table(&p.load_table),
            ]
            .spacing(6)
            .into()
        }
    };

    container(panel_container(content))
        .width(Length::FillPortion(1))
        .into()
}
