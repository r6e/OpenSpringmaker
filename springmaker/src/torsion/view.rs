//! Humble iced view for the torsion spring calculator.
//!
//! All business logic lives in `form` and `view_model`. This module assembles
//! iced widgets from the current [`App`] state, delegating data decisions to
//! the presenter layer (ADR 0008).

use iced::widget::{column, container, row, text};
use iced::{Element, Font, Length};

use crate::app::{App, Message, Palette};
use crate::presenter::Emphasis;
use crate::torsion::form::{Field, TorFormState, TorScenarioKind};
use crate::torsion::form::{ALL_MOMENT_ENTRIES, ALL_TOR_SCENARIOS};
use crate::torsion::view_model::{
    tor_fatigue_chart_data, tor_fatigue_inputs_view, tor_inputs_view, tor_results_view,
    TorFatigueView, TorLoadTable, TorResultsView,
};
use crate::widgets::{
    divided_result_section, field_label, labeled_input, material_picker, panel_container,
    render_result_row, results_empty, results_error, rows_section, section_divider,
    section_heading, styled_pick_list, visual_toggle, COL_PT, SP_LG, SP_MD, SP_ROW, SP_SM, SP_XS,
    SZ_CAPTION, SZ_LABEL,
};

// --------------------------------------------------------------------------
// Design (left) panel
// --------------------------------------------------------------------------

pub(crate) fn design_panel(app: &App) -> Element<'_, Message> {
    let pal = app.pal();
    // Setup group — material selector, scenario pick-list, optional moment-entry
    // pick-list (hidden for TwoLoad), and friction model pick-list.
    let scenario_col = column![
        field_label(pal, "Input mode"),
        styled_pick_list(
            pal,
            ALL_TOR_SCENARIOS,
            Some(app.torsion.scenario),
            Message::TorScenario,
        ),
    ]
    .spacing(SP_XS);

    let mut setup_group = column![
        section_heading(pal, "Setup"),
        material_picker(app),
        scenario_col
    ]
    .spacing(SP_MD);

    if app.torsion.scenario != TorScenarioKind::TwoLoad
        && app.torsion.scenario != TorScenarioKind::MinWeight
    {
        setup_group = setup_group.push(
            column![
                field_label(pal, "Moment entry"),
                styled_pick_list(
                    pal,
                    ALL_MOMENT_ENTRIES,
                    Some(app.torsion.moment_entry),
                    Message::TorMomentEntry,
                ),
            ]
            .spacing(SP_XS),
        );
    }

    if app.torsion.scenario == TorScenarioKind::MinWeight {
        setup_group = setup_group.push(
            column![
                field_label(pal, "Diameter policy"),
                styled_pick_list(
                    pal,
                    springcore::torsion::ALL_DIA_POLICIES,
                    Some(app.torsion.dia_policy),
                    Message::TorDiaPolicy,
                ),
            ]
            .spacing(SP_XS),
        );
    }

    setup_group = setup_group.push(
        column![
            field_label(pal, "Friction model"),
            styled_pick_list(
                pal,
                springcore::torsion::ALL_FRICTION_MODELS,
                Some(app.torsion.friction_model),
                Message::TorFriction,
            ),
        ]
        .spacing(SP_XS),
    );

    setup_group = setup_group.push(
        column![
            field_label(pal, "Cycle life"),
            styled_pick_list(
                pal,
                springcore::torsion::ALL_CYCLE_LIVES,
                Some(app.torsion.cycle_life),
                Message::TorCycleLife,
            ),
        ]
        .spacing(SP_XS),
    );

    // Inputs group — driven by the presenter's field list.
    let inputs = tor_inputs_view(app);
    let mut inputs_col = column![section_heading(pal, "Inputs")].spacing(SP_MD);
    for fd in &inputs {
        let field = fd.field;
        inputs_col = inputs_col.push(labeled_input(
            pal,
            &fd.label,
            tor_field_value(&app.torsion, field),
            tor_field_id(field),
            move |s| Message::TorField(field, s),
        ));
    }

    // Fatigue cycle group — a separate presenter list, empty for MinWeight.
    let fatigue_inputs = tor_fatigue_inputs_view(app);
    if !fatigue_inputs.is_empty() {
        inputs_col = inputs_col
            .push(section_divider(pal))
            .push(section_heading(pal, "Fatigue cycle (leave blank to skip)"));
        for fd in &fatigue_inputs {
            let field = fd.field;
            inputs_col = inputs_col.push(labeled_input(
                pal,
                &fd.label,
                tor_field_value(&app.torsion, field),
                tor_field_id(field),
                move |s| Message::TorField(field, s),
            ));
        }
    }

    let inner = column![setup_group, section_divider(pal), inputs_col].spacing(SP_LG);

    container(panel_container(pal, inner))
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
        Field::MaxMoment => &form.max_moment,
        Field::IndexMin => &form.index_min,
        Field::IndexMax => &form.index_max,
        Field::MaxOuterDia => &form.max_outer_dia,
        Field::CandidateDiameters => &form.candidate_diameters,
        Field::FatigueMin => &form.fatigue_min,
        Field::FatigueMax => &form.fatigue_max,
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
        Field::MaxMoment => "tor-max-moment",
        Field::IndexMin => "tor-index-min",
        Field::IndexMax => "tor-index-max",
        Field::MaxOuterDia => "tor-max-outer-dia",
        Field::CandidateDiameters => "tor-candidate-diameters",
        Field::FatigueMin => "tor-fatigue-min",
        Field::FatigueMax => "tor-fatigue-max",
    }
}

// --------------------------------------------------------------------------
// Results (right) panel — renderers
// --------------------------------------------------------------------------

fn render_tor_load_table(pal: &'static Palette, lt: &TorLoadTable) -> Element<'static, Message> {
    let mut col = column![section_heading(pal, "Load points")].spacing(SP_XS);

    // Header row.
    col = col.push(
        row![
            text("Pt")
                .size(SZ_CAPTION)
                .color(pal.muted)
                .width(Length::Fixed(COL_PT)),
            text("Moment")
                .size(SZ_CAPTION)
                .color(pal.muted)
                .width(Length::FillPortion(3)),
            text("Deflection")
                .size(SZ_CAPTION)
                .color(pal.muted)
                .width(Length::FillPortion(3)),
            text(format!("Stress ({})", lt.stress_unit))
                .size(SZ_CAPTION)
                .color(pal.muted)
                .width(Length::FillPortion(2)),
            text("% Allow")
                .size(SZ_CAPTION)
                .color(pal.muted)
                .width(Length::FillPortion(2)),
            text("Wound ID")
                .size(SZ_CAPTION)
                .color(pal.muted)
                .width(Length::FillPortion(2)),
        ]
        .spacing(SP_XS),
    );

    // Data rows — stress and % allow are colored by emphasis.
    for lp in &lt.rows {
        let stress_color = match lp.stress_emphasis {
            Emphasis::Normal => pal.text,
            Emphasis::Danger => pal.danger,
        };

        let data_row = row![
            text(lp.point.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(pal.muted)
                .width(Length::Fixed(COL_PT)),
            text(lp.moment.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(pal.text)
                .width(Length::FillPortion(3)),
            text(lp.deflection.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(pal.text)
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
                .color(pal.text)
                .width(Length::FillPortion(2)),
        ]
        .spacing(SP_XS);

        col = col.push(data_row);
    }

    col.into()
}

// --------------------------------------------------------------------------
// Results (right) panel
// --------------------------------------------------------------------------

pub(crate) fn results_panel(app: &App) -> Element<'_, Message> {
    let pal = app.pal();
    let us = app.unit_system;
    let content: Element<'_, Message> = match tor_results_view(app) {
        TorResultsView::Error(msg) => results_error(pal, msg),
        TorResultsView::Empty => results_empty(pal),
        TorResultsView::Populated(p) => {
            // The results panel's shared visual slot: chart or orbitable 3D
            // scene, selected by `app.results_visual`. Each visual is pure
            // rendering of the design (no decision), built from the outcome
            // the Populated variant guarantees is present — and built ONLY in
            // its own arm, so exactly one load-deflection bitmap is
            // rasterized per render (orbit drags re-render every frame; an
            // eagerly-built chart would be thrown away each time).
            let outcome = app
                .tor_outcome
                .as_ref()
                .expect("TorResultsView::Populated implies app.tor_outcome is Some");
            let visual: Element<'_, Message> = match app.results_visual {
                crate::app::VisualMode::Chart => crate::plot::chart_element(
                    pal,
                    crate::torsion::plot_model::torsion_chart(&outcome.design, us),
                ),
                crate::app::VisualMode::Spring3d => crate::viz::scene_element(
                    pal,
                    crate::torsion::scene_model::torsion_scene(&outcome.design),
                    app.orbit,
                ),
            };
            let toggle = visual_toggle(pal, app.results_visual);

            // The presenter decides whether a fatigue chart exists (it stays
            // hidden with the fatigue rows on min-weight runs); the view only
            // renders the data it hands back. Reuses the `outcome` binding
            // above rather than re-deriving it from `app.tor_outcome`.
            let fatigue_chart =
                tor_fatigue_chart_data(outcome, us).map(|d| crate::plot::chart_element(pal, d));

            // Angular rate section — two ResultRows (per-degree and per-revolution).
            let mut rate_col = column![section_heading(pal, "Angular rate")].spacing(SP_ROW);
            rate_col = rate_col.push(render_result_row(pal, &p.rate_per_deg));
            rate_col = rate_col.push(render_result_row(pal, &p.rate_per_turn));

            let mut col = column![
                section_heading(pal, "Results"),
                section_divider(pal),
                rate_col,
                section_divider(pal),
                rows_section(pal, "Geometry", &p.geometry),
                section_divider(pal),
                render_tor_load_table(pal, &p.load_table),
                section_divider(pal),
                toggle,
                visual,
            ]
            .spacing(SP_ROW);

            if let Some(rows) = &p.min_weight {
                col = col.push(divided_result_section(pal, "Min-weight optimisation", rows));
            }

            match &p.fatigue {
                TorFatigueView::Hidden => {}
                TorFatigueView::Computed(rows) => {
                    col = col.push(divided_result_section(pal, "Fatigue analysis", rows));
                }
                TorFatigueView::Note(msg) => {
                    col = col.push(
                        column![
                            section_divider(pal),
                            text(*msg).size(SZ_LABEL).color(pal.muted),
                        ]
                        .spacing(SP_SM),
                    );
                }
            }
            // Directly beneath the fatigue rows; None whenever they are not
            // Computed (the presenter gates both together).
            if let Some(fc) = fatigue_chart {
                col = col.push(fc);
            }

            col.into()
        }
    };

    container(panel_container(pal, content))
        .width(Length::FillPortion(1))
        .into()
}
