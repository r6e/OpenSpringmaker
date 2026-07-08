//! Assembly humble view (ADR 0008): renders presenter output, no logic.
//!
//! Skeleton: Task 2 adds the Populated results panel.

use iced::widget::{button, column, container, row, text};
use iced::{Element, Length};

use crate::app::{App, Message, C};
use crate::assembly::form::{AsmMemberForm, MemberField};
use crate::assembly::view_model::{asm_results_view, AsmResultsView};
use crate::picker::{find_by_key, KeyLabel, END_TYPES, FIXITIES, TOPOLOGIES};
use crate::widgets::{
    danger_button_style, field_label, ghost_button_style, labeled_input,
    material_picker_for_member, panel_container, results_empty, results_error, section_divider,
    section_heading, styled_pick_list, SZ_LABEL,
};

// --------------------------------------------------------------------------
// Design (left) panel
// --------------------------------------------------------------------------

pub(crate) fn design_panel(app: &App) -> Element<'_, Message> {
    let selected_topology = find_by_key(TOPOLOGIES, &app.assembly.topology).copied();
    let selected_fixity = find_by_key(FIXITIES, &app.assembly.fixity).copied();

    let setup_group = column![
        section_heading("Setup"),
        column![
            field_label("Topology"),
            styled_pick_list(TOPOLOGIES, selected_topology, |kl: KeyLabel| {
                Message::AsmTopology(kl.key.to_string())
            }),
        ]
        .spacing(4),
        column![
            field_label("Fixity"),
            styled_pick_list(FIXITIES, selected_fixity, |kl: KeyLabel| {
                Message::AsmFixity(kl.key.to_string())
            }),
        ]
        .spacing(4),
    ]
    .spacing(10);

    let loads_group = column![
        section_heading("Loads"),
        labeled_input("Loads", &app.assembly.loads, "asm-loads", |v| {
            Message::AsmLoads(v)
        }),
    ]
    .spacing(8);

    let mut members_col = column![section_heading("Members")].spacing(12);
    for index in 0..app.assembly.members.len() {
        let m = &app.assembly.members[index];
        members_col = members_col.push(member_card(app, index, m));
    }

    let add_btn = button(text("+ Add member").size(SZ_LABEL).color(C::TEXT))
        .style(ghost_button_style)
        .on_press(Message::AsmMemberAdd);

    let inner = column![
        setup_group,
        section_divider(),
        loads_group,
        section_divider(),
        members_col,
        add_btn,
    ]
    .spacing(16);

    container(panel_container(inner))
        .width(Length::FillPortion(1))
        .into()
}

// --------------------------------------------------------------------------
// Results (right) panel
// --------------------------------------------------------------------------

pub(crate) fn results_panel(app: &App) -> Element<'_, Message> {
    let view = asm_results_view(app);
    let inner: Element<'_, Message> = match view {
        AsmResultsView::Error(msg) => results_error(msg),
        AsmResultsView::Empty => results_empty(),
    };
    container(panel_container(inner))
        .width(Length::FillPortion(1))
        .into()
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

    let header_text = text(format!("Member {}", index + 1)).size(SZ_LABEL);
    let mut header = row![header_text].spacing(8);
    if app.assembly.members.len() > 1 {
        let remove_btn = button(text("Remove").size(SZ_LABEL).color(C::DANGER))
            .style(danger_button_style)
            .on_press(Message::AsmMemberRemove(index));
        header = header.push(remove_btn);
    }

    let selected_end = find_by_key(END_TYPES, &m.end_type).copied();

    column![
        header,
        material_picker_for_member(app, index),
        column![
            field_label("End type"),
            styled_pick_list(END_TYPES, selected_end, move |kl: KeyLabel| {
                Message::AsmMemberEndType(index, kl.key.to_string())
            }),
        ]
        .spacing(4),
        labeled_input(
            "Wire dia",
            &m.wire_dia,
            asm_member_field_id(index, F::WireDia),
            move |v| Message::AsmField(index, F::WireDia, v)
        ),
        labeled_input(
            "Mean dia",
            &m.mean_dia,
            asm_member_field_id(index, F::MeanDia),
            move |v| Message::AsmField(index, F::MeanDia, v)
        ),
        labeled_input(
            "Active coils",
            &m.active,
            asm_member_field_id(index, F::Active),
            move |v| Message::AsmField(index, F::Active, v)
        ),
        labeled_input(
            "Free length",
            &m.free_length,
            asm_member_field_id(index, F::FreeLength),
            move |v| Message::AsmField(index, F::FreeLength, v)
        ),
    ]
    .spacing(6)
    .padding(8)
    .into()
}
