//! Humble iced view for the extension spring calculator.
//!
//! All business logic lives in `form` and `view_model`. This module assembles
//! iced widgets from the current [`App`] state, delegating data decisions to
//! the presenter layer (ADR 0008).

use iced::widget::{column, container, row, text};
use iced::{Element, Font, Length};

use crate::app::{App, Message, Palette};
use crate::extension::form::{ExtFormState, Field, HookMode, ALL_EXT_SCENARIOS};
use crate::extension::view_model::{
    ext_inputs_view, ext_results_view, ExtLoadTable, ExtPopulatedResults, ExtResultsView,
};
use crate::presenter::unit_length_label;
use crate::widgets::{
    divided_result_section, emphasis_color, field_label, labeled_input, panel_container,
    render_governing_rate, results_empty, results_error, rows_section, section_divider,
    section_heading, segmented, styled_pick_list, visual_toggle, COL_PT, SP_LG, SP_MD, SP_ROW,
    SP_SM, SP_XS, SZ_CAPTION, SZ_LABEL,
};

// --------------------------------------------------------------------------
// Design (left) panel
// --------------------------------------------------------------------------

pub(crate) fn design_panel(app: &App) -> Element<'_, Message> {
    let pal = app.pal();
    // Setup group — material + scenario picker (no end-type/fixity for extension springs).
    let setup_group = column![
        section_heading(pal, "Setup"),
        crate::widgets::material_picker(app),
        column![
            field_label(pal, "Scenario"),
            styled_pick_list(
                pal,
                ALL_EXT_SCENARIOS,
                Some(app.extension.scenario),
                Message::ExtScenario
            ),
        ]
        .spacing(SP_XS),
    ]
    .spacing(SP_MD);

    // Inputs group — driven by the presenter's field list.
    let inputs = ext_inputs_view(app);
    let mut inputs_col = column![section_heading(pal, "Inputs")].spacing(SP_MD);
    for fd in &inputs {
        let field = fd.field;
        inputs_col = inputs_col.push(labeled_input(
            pal,
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
        section_divider(pal),
        inputs_col,
        section_divider(pal),
        hooks_group,
    ]
    .spacing(SP_LG);

    container(panel_container(pal, inner))
        .width(Length::FillPortion(1))
        .into()
}

fn hooks_group(app: &App) -> Element<'_, Message> {
    let pal = app.pal();
    let len_label = unit_length_label(app.unit_system);
    let mode = app.extension.hook_mode;

    let mode_toggle = segmented(
        pal,
        &[
            ("Default (machine loops)", HookMode::Default),
            ("Custom radii", HookMode::Custom),
        ],
        mode,
        Message::ExtHookMode,
    );

    let mut col = column![section_heading(pal, "Hook geometry"), mode_toggle].spacing(SP_SM);

    if mode == HookMode::Custom {
        col = col
            .push(labeled_input(
                pal,
                &format!("Hook radius r1 ({len_label})"),
                &app.extension.hook_r1,
                ext_field_id(Field::HookR1),
                |s| Message::ExtField(Field::HookR1, s),
            ))
            .push(labeled_input(
                pal,
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
        Field::OuterDia => &form.outer_dia,
        Field::Active => &form.active,
        Field::FreeLength => &form.free_length,
        Field::InitialTension => &form.initial_tension,
        Field::Loads => &form.loads,
        Field::Rate => &form.rate,
        Field::HookR1 => &form.hook_r1,
        Field::HookR2 => &form.hook_r2,
        Field::Force1 => &form.force1,
        Field::Length1 => &form.length1,
        Field::Force2 => &form.force2,
        Field::Length2 => &form.length2,
        Field::MaxForce => &form.max_force,
        Field::CandidateDiameters => &form.candidate_diameters,
        Field::IndexMin => &form.index_min,
        Field::IndexMax => &form.index_max,
        Field::MaxOuterDia => &form.max_outer_dia,
    }
}

/// Stable widget ID for a calculator extension field's text input. The inputs
/// are empty by default, so headless Simulator tests can't target them by text
/// content and select by this id instead. An explicit, exhaustive match (rather
/// than a `Debug`-derived string) keeps the ids a deliberate stable contract,
/// avoids a per-render allocation, and forces a choice when a `Field` is added.
/// Single source of truth shared by the view and Simulator tests, which resolve
/// their target inputs through this function (see `type_into_ext`) rather than
/// hardcoding the strings, so the two cannot drift. Each `Field` renders at most
/// one input per frame.
pub(crate) fn ext_field_id(field: Field) -> &'static str {
    match field {
        Field::WireDia => "ext-wire-dia",
        Field::MeanDia => "ext-mean-dia",
        Field::OuterDia => "ext-outer-dia",
        Field::Active => "ext-active",
        Field::FreeLength => "ext-free-length",
        Field::InitialTension => "ext-initial-tension",
        Field::Loads => "ext-loads",
        Field::Rate => "ext-rate",
        Field::HookR1 => "ext-hook-r1",
        Field::HookR2 => "ext-hook-r2",
        Field::Force1 => "ext-force1",
        Field::Length1 => "ext-length1",
        Field::Force2 => "ext-force2",
        Field::Length2 => "ext-length2",
        Field::MaxForce => "ext-max-force",
        Field::CandidateDiameters => "ext-candidate-diameters",
        Field::IndexMin => "ext-index-min",
        Field::IndexMax => "ext-index-max",
        Field::MaxOuterDia => "ext-max-outer-dia",
    }
}

// --------------------------------------------------------------------------
// Results (right) panel — renderers
// --------------------------------------------------------------------------

fn render_ext_load_table(pal: &'static Palette, lt: &ExtLoadTable) -> Element<'static, Message> {
    let mut col = column![section_heading(pal, "Load points")].spacing(SP_XS);

    col = col.push(
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
            text(format!("Body \u{03c4} ({})", lt.stress_unit))
                .size(SZ_CAPTION)
                .color(pal.muted)
                .width(Length::FillPortion(2)),
            text(format!("Hook \u{03c3} ({})", lt.stress_unit))
                .size(SZ_CAPTION)
                .color(pal.muted)
                .width(Length::FillPortion(2)),
            text(format!("Hook \u{03c4} ({})", lt.stress_unit))
                .size(SZ_CAPTION)
                .color(pal.muted)
                .width(Length::FillPortion(2)),
            text("% \u{03c4}_body")
                .size(SZ_CAPTION)
                .color(pal.muted)
                .width(Length::FillPortion(1)),
            text("% \u{03c3}")
                .size(SZ_CAPTION)
                .color(pal.muted)
                .width(Length::FillPortion(1)),
            text("% \u{03c4}_hook")
                .size(SZ_CAPTION)
                .color(pal.muted)
                .width(Length::FillPortion(1)),
        ]
        .spacing(SP_XS),
    );

    for lp in &lt.rows {
        let body_color = emphasis_color(pal, lp.body_emphasis);
        let bending_color = emphasis_color(pal, lp.bending_emphasis);
        let torsion_color = emphasis_color(pal, lp.torsion_emphasis);
        let data_row = row![
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
            text(lp.body_shear.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(body_color)
                .width(Length::FillPortion(2)),
            text(lp.hook_bending.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(bending_color)
                .width(Length::FillPortion(2)),
            text(lp.hook_torsion.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(torsion_color)
                .width(Length::FillPortion(2)),
            text(lp.pct_body.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(body_color)
                .width(Length::FillPortion(1)),
            text(lp.pct_bending.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(bending_color)
                .width(Length::FillPortion(1)),
            text(lp.pct_torsion.clone())
                .font(Font::MONOSPACE)
                .size(SZ_LABEL)
                .color(torsion_color)
                .width(Length::FillPortion(1)),
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
    let content: Element<'_, Message> = match ext_results_view(app) {
        ExtResultsView::Error(msg) => results_error(pal, msg),
        ExtResultsView::Empty => results_empty(pal),
        ExtResultsView::Populated(p) => {
            // The results panel's shared visual slot (see
            // `results_visual_element`'s doc for the one-bitmap-per-render
            // laziness rationale). Built from the outcome the Populated
            // variant guarantees is present.
            let outcome = app
                .ext_outcome
                .as_ref()
                .expect("ExtResultsView::Populated implies app.ext_outcome is Some");
            let visual = crate::widgets::results_visual_element(
                pal,
                app,
                || {
                    crate::plot::chart_element(
                        pal,
                        crate::extension::plot_model::extension_chart(&outcome.design, us),
                    )
                },
                || crate::extension::scene_model::extension_scene(&outcome.design),
                || crate::viz::sdf::extension_sdf(&outcome.design),
                || {
                    crate::diagram::DiagramInput::new(
                        crate::extension::scene_model::extension_scene(&outcome.design),
                        crate::extension::diagram_model::dimensions(&outcome.design),
                    )
                },
            );
            let toggle = visual_toggle(pal, app.results_visual);

            render_populated(pal, &p, toggle, visual)
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
    p: &ExtPopulatedResults,
    toggle: Element<'a, Message>,
    visual: Element<'a, Message>,
) -> Element<'a, Message> {
    let mut col = column![
        section_heading(pal, "Results"),
        section_divider(pal),
        render_governing_rate(pal, "Spring rate", &p.governing_rate),
        section_divider(pal),
        rows_section(pal, "Geometry", &p.geometry),
        section_divider(pal),
        render_ext_load_table(pal, &p.load_table),
        section_divider(pal),
        toggle,
        visual,
    ]
    .spacing(SP_ROW);
    if let Some(rows) = &p.min_weight {
        col = col.push(divided_result_section(pal, "Min-weight optimisation", rows));
    }
    col.into()
}
