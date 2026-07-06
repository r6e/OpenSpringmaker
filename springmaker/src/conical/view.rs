//! Conical humble view (ADR 0008): renders presenter output, no logic.

// Task 1 skeleton: setup group (material + end-type pickers) + the six-field
// inputs group; the results panel renders Empty/Error only. Task 2 adds the
// populated results (geometry, load table, footer).

use iced::widget::{column, container};
use iced::{Element, Length};

use crate::app::{App, Message};
use crate::conical::form::Field;
use crate::conical::view_model::{con_inputs_view, con_results_view, ConResultsView};
use crate::picker::{find_by_key, END_TYPES};
use crate::widgets::{
    field_label, labeled_input, material_picker, panel_container, results_empty, results_error,
    section_divider, section_heading, styled_pick_list,
};

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
    let content: Element<'_, Message> = match con_results_view(app) {
        ConResultsView::Error(msg) => results_error(msg),
        ConResultsView::Empty => results_empty(),
    };

    container(panel_container(content))
        .width(Length::FillPortion(1))
        .into()
}
