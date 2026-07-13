//! Assembly humble view (ADR 0008): renders presenter output, no logic.

use iced::widget::{button, column, container, row, text};
use iced::{Element, Font, Length};

use crate::app::{App, Message, Palette};
use crate::assembly::form::{AsmMemberForm, MemberField};
use crate::assembly::view_model::{
    asm_results_view, AsmMemberResultView, AsmPopulatedResults, AsmResultsView,
};
use crate::picker::{find_by_key, KeyLabel, END_TYPES, FIXITIES, TOPOLOGIES};
use crate::presenter::LoadTable;
use crate::widgets::{
    danger_button_style, emphasis_color, field_label, ghost_button_style, labeled_input,
    material_picker_for_member, member_sub_card, panel_container, render_governing_rate,
    render_result_row, results_empty, results_error, rows_section, section_divider,
    section_heading, styled_pick_list, visual_toggle, COL_PT, SP_LG, SP_MD, SP_ROW, SP_SM, SP_XS,
    SZ_CAPTION, SZ_LABEL,
};

// --------------------------------------------------------------------------
// Design (left) panel
// --------------------------------------------------------------------------

pub(crate) fn design_panel(app: &App) -> Element<'_, Message> {
    let pal = app.pal();
    let selected_topology = find_by_key(TOPOLOGIES, &app.assembly.topology).copied();
    let selected_fixity = find_by_key(FIXITIES, &app.assembly.fixity).copied();

    let setup_group = column![
        section_heading(pal, "Setup"),
        column![
            field_label(pal, "Topology"),
            styled_pick_list(pal, TOPOLOGIES, selected_topology, |kl: KeyLabel| {
                Message::AsmTopology(kl.key.to_string())
            }),
        ]
        .spacing(SP_XS),
        column![
            field_label(pal, "Fixity"),
            styled_pick_list(pal, FIXITIES, selected_fixity, |kl: KeyLabel| {
                Message::AsmFixity(kl.key.to_string())
            }),
        ]
        .spacing(SP_XS),
    ]
    .spacing(SP_MD);

    let loads_group = column![
        section_heading(pal, "Loads"),
        labeled_input(pal, "Loads", &app.assembly.loads, "asm-loads", |v| {
            Message::AsmLoads(v)
        }),
    ]
    .spacing(SP_SM);

    let mut members_col = column![section_heading(pal, "Members")].spacing(SP_MD);
    for (index, m) in app.assembly.members.iter().enumerate() {
        members_col = members_col.push(member_card(app, index, m));
    }

    let add_btn = button(text("+ Add member").size(SZ_LABEL).color(pal.text))
        .style(ghost_button_style(pal))
        .on_press(Message::AsmMemberAdd);

    let inner = column![
        setup_group,
        section_divider(pal),
        loads_group,
        section_divider(pal),
        members_col,
        add_btn,
    ]
    .spacing(SP_LG);

    container(panel_container(pal, inner))
        .width(Length::FillPortion(1))
        .into()
}

// --------------------------------------------------------------------------
// Results (right) panel
// --------------------------------------------------------------------------

pub(crate) fn results_panel(app: &App) -> Element<'_, Message> {
    let pal = app.pal();
    let us = app.unit_system;
    let inner: Element<'_, Message> = match asm_results_view(app) {
        AsmResultsView::Error(msg) => results_error(pal, msg),
        AsmResultsView::Empty => results_empty(pal),
        AsmResultsView::Populated(p) => {
            // The results panel's shared visual slot (`results_visual_element`
            // — laziness preserved via closures, so exactly one bitmap is
            // rasterized per render: orbit drags re-render every frame, and
            // an eagerly-built chart would be thrown away each time). Built
            // from the outcome the Populated variant guarantees is present.
            // Unlike the other families, the assembly outcome IS the design
            // — no wrapper struct to unwrap first.
            let outcome = app
                .asm_outcome
                .as_ref()
                .expect("AsmResultsView::Populated implies app.asm_outcome is Some");
            let visual = crate::widgets::results_visual_element(
                pal,
                app,
                || {
                    crate::plot::chart_element(
                        pal,
                        crate::assembly::plot_model::assembly_chart(outcome, us),
                    )
                },
                || {
                    (
                        crate::assembly::scene_model::assembly_scene(outcome),
                        crate::viz::sdf::assembly_sdf(outcome),
                    )
                },
            );
            let toggle = visual_toggle(pal, app.results_visual);

            render_populated(pal, &p, toggle, visual)
        }
    };
    container(panel_container(pal, inner))
        .width(Length::FillPortion(1))
        .into()
}

// --------------------------------------------------------------------------
// Populated results rendering
// --------------------------------------------------------------------------

/// Render the populated assembly results: hero rate → Summary section →
/// assembly load table → chart/3D toggle + visual → per-member sections.
/// Status is handled by the calculator's shared status panel (not rendered
/// here — see ADR 0008).
fn render_populated<'a>(
    pal: &'static Palette,
    p: &AsmPopulatedResults,
    toggle: Element<'a, Message>,
    visual: Element<'a, Message>,
) -> Element<'a, Message> {
    let mut col = column![
        section_heading(pal, "Results"),
        section_divider(pal),
        render_governing_rate(pal, "Spring rate", &p.governing_rate),
        section_divider(pal),
        rows_section(pal, "Summary", &p.summary),
        section_divider(pal),
        render_asm_load_table(pal, &p.assembly_loads),
        section_divider(pal),
        toggle,
        visual,
    ]
    .spacing(SP_ROW);

    for member in &p.members {
        col = col.push(section_divider(pal));
        col = col.push(render_member_section(pal, member));
    }

    col.into()
}

/// Assembly-level load table: 4 columns (Pt / Force / Deflection / Length).
/// The assembly `LoadTable` has no stress — `stress_unit` is empty and the
/// stress/% MTS columns are omitted.
fn render_asm_load_table(pal: &'static Palette, lt: &LoadTable) -> Element<'static, Message> {
    let mut col = column![section_heading(pal, "Assembly load points")].spacing(SP_XS);

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
        ]
        .spacing(SP_XS),
    );

    for lp in &lt.rows {
        col = col.push(
            row![
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
            ]
            .spacing(SP_XS),
        );
    }

    col.into()
}

/// Per-member section: heading + geometry rows + 6-column load table.
///
/// Rows are pushed directly into the column (no empty section heading above
/// them) to avoid the blank-line visual artifact from `rows_section("")`.
fn render_member_section(
    pal: &'static Palette,
    m: &AsmMemberResultView,
) -> Element<'static, Message> {
    let heading = text(m.heading.clone()).size(SZ_LABEL).color(pal.text);

    let mut col = column![heading].spacing(SP_ROW);
    for r in &m.rows {
        col = col.push(render_result_row(pal, r));
    }
    if !m.loads.rows.is_empty() {
        col = col.push(render_member_load_table(pal, &m.loads));
    }

    member_sub_card(pal, col)
}

/// Per-member load table: 6 columns (Pt / Force / Deflection / Length /
/// Stress(unit) / % MTS). Mirrors `render_con_load_table` in the conical view.
fn render_member_load_table(pal: &'static Palette, lt: &LoadTable) -> Element<'static, Message> {
    let mut col = column![section_heading(pal, "Member load points")].spacing(SP_XS);

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
        col = col.push(
            row![
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
            .spacing(SP_XS),
        );
    }

    col.into()
}

// --------------------------------------------------------------------------
// Member field widget id
// --------------------------------------------------------------------------

/// Stable widget id for a member's text input. Runtime `String` — the
/// `labeled_input` `id` param was widened to `impl Into<iced::widget::Id>`
/// so existing `&'static str` callers are unaffected.
pub(crate) fn asm_member_field_id(index: usize, field: MemberField) -> String {
    use MemberField::*;
    let leaf = match field {
        WireDia => "wire-dia",
        MeanDia => "mean-dia",
        Active => "active",
        FreeLength => "free-length",
    };
    format!("asm-member-{index}-{leaf}")
}

// --------------------------------------------------------------------------
// Member card
// --------------------------------------------------------------------------

fn member_card<'a>(app: &'a App, index: usize, m: &'a AsmMemberForm) -> Element<'a, Message> {
    use MemberField as F;

    let pal = app.pal();
    let header_text = text(format!("Member {}", index + 1)).size(SZ_LABEL);
    let mut header = row![header_text].spacing(SP_SM);
    if app.assembly.members.len() > 1 {
        let remove_btn = button(text("Remove").size(SZ_LABEL).color(pal.danger))
            .style(danger_button_style(pal))
            .on_press(Message::AsmMemberRemove(index));
        header = header.push(remove_btn);
    }

    let selected_end = find_by_key(END_TYPES, &m.end_type).copied();

    column![
        header,
        material_picker_for_member(app, index),
        column![
            field_label(pal, "End type"),
            styled_pick_list(pal, END_TYPES, selected_end, move |kl: KeyLabel| {
                Message::AsmMemberEndType(index, kl.key.to_string())
            }),
        ]
        .spacing(SP_XS),
        labeled_input(
            pal,
            "Wire dia",
            &m.wire_dia,
            asm_member_field_id(index, F::WireDia),
            move |v| Message::AsmField(index, F::WireDia, v)
        ),
        labeled_input(
            pal,
            "Mean dia",
            &m.mean_dia,
            asm_member_field_id(index, F::MeanDia),
            move |v| Message::AsmField(index, F::MeanDia, v)
        ),
        labeled_input(
            pal,
            "Active coils",
            &m.active,
            asm_member_field_id(index, F::Active),
            move |v| Message::AsmField(index, F::Active, v)
        ),
        labeled_input(
            pal,
            "Free length",
            &m.free_length,
            asm_member_field_id(index, F::FreeLength),
            move |v| Message::AsmField(index, F::FreeLength, v)
        ),
    ]
    .spacing(SP_ROW)
    .padding(SP_SM)
    .into()
}
