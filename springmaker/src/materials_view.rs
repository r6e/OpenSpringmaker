//! Materials editor screen — list panel + edit-form panel.
//!
//! Pure view logic. No computation here; all business logic lives in
//! `app.rs`, `materials_form.rs`, and `springcore`.

use iced::widget::{
    button, checkbox, column, container, horizontal_space, row, scrollable, text, text_input,
};
use iced::{Background, Element, Font, Length};

use crate::app::{App, MatField, Message, Screen, C};
use crate::materials_view_model::{edit_panel, feedback, list_rows, Badge, FeedbackKind};
use crate::view::{
    accent_button_style, danger_button_style, field_label, ghost_button_style, mono_value,
    nav_button_style, panel_container, section_divider, section_heading, styled_pick_list,
    text_input_style, SZ_BODY, SZ_LABEL, SZ_TITLE,
};
use springcore::{MtsForm, StrengthUnits};

// --------------------------------------------------------------------------
// Local helpers
// --------------------------------------------------------------------------

/// Styled text input bound to a [`MatField`], mirroring `styled_text_input`
/// in `view.rs` but emitting [`Message::MatField`].
fn mat_text_input<'a>(placeholder: &str, value: &str, field: MatField) -> Element<'a, Message> {
    text_input(placeholder, value)
        .on_input(move |s| Message::MatField(field, s))
        .size(SZ_BODY)
        .font(Font::MONOSPACE)
        .style(text_input_style)
        .into()
}

/// A labeled material text input: muted label above a mat_text_input.
fn labeled_mat_input<'a>(label: &str, value: &str, field: MatField) -> Element<'a, Message> {
    column![
        field_label(label.to_owned()),
        mat_text_input("", value, field),
    ]
    .spacing(4)
    .into()
}

// --------------------------------------------------------------------------
// Sub-panels
// --------------------------------------------------------------------------

fn build_list_panel(app: &App) -> Element<'_, Message> {
    let mut list_col = column![].spacing(6);
    // Rendering is driven entirely by the presenter (materials_view_model);
    // which actions a row offers is decided there and unit-tested.
    for r in list_rows(app) {
        let (badge_text, badge_color) = match r.badge {
            Badge::Curated => ("curated \u{1f512}", C::MUTED),
            Badge::User => ("user", C::ACCENT),
        };
        let badge = text(badge_text).size(SZ_LABEL).color(badge_color);

        let mut btn_row = row![badge, horizontal_space()].spacing(6);
        if r.can_clone {
            btn_row = btn_row.push(
                button(text("Clone").size(SZ_LABEL).color(C::TEXT))
                    .on_press(Message::MatClone(r.name.clone()))
                    .style(ghost_button_style),
            );
        }
        if r.can_edit {
            btn_row = btn_row.push(
                button(text("Edit").size(SZ_LABEL).color(C::TEXT))
                    .on_press(Message::MatEdit(r.name.clone()))
                    .style(ghost_button_style),
            );
        }
        if r.can_remove {
            btn_row = btn_row.push(
                button(text("Remove").size(SZ_LABEL).color(C::DANGER))
                    .on_press(Message::MatDelete(r.name.clone()))
                    .style(danger_button_style),
            );
        }

        let entry = column![
            mono_value(r.name, C::TEXT, SZ_BODY),
            btn_row,
            section_divider(),
        ]
        .spacing(4);

        list_col = list_col.push(entry);
    }

    let list_scroll = scrollable(list_col).height(Length::Fill);

    let new_btn = button(text("New").size(SZ_BODY).color(C::INK))
        .on_press(Message::MatNew)
        .style(accent_button_style);

    let persist_btn = button(text("Save to disk").size(SZ_BODY).color(C::TEXT))
        .on_press(Message::MatPersist)
        .style(ghost_button_style);

    let footer = row![new_btn, persist_btn].spacing(10);

    let inner = column![
        section_heading("Materials"),
        section_divider(),
        list_scroll,
        footer,
    ]
    .spacing(10)
    .height(Length::Fill);

    container(panel_container(inner))
        .width(Length::FillPortion(1))
        .height(Length::Fill)
        .into()
}

fn build_edit_panel(app: &App) -> Element<'_, Message> {
    // The presenter decides whether the panel is shown and what it contains
    // (coefficient hint, section visibility, new-vs-editing); the values bound
    // to the inputs come from app.mat_form.
    let panel = match edit_panel(app) {
        None => {
            let hint = text("Select a material to edit, or New.")
                .size(SZ_BODY)
                .color(C::MUTED);
            return container(panel_container(hint))
                .width(Length::FillPortion(1))
                .height(Length::Fill)
                .into();
        }
        Some(p) => p,
    };

    let f = &app.mat_form;

    // MTS form options
    const MTS_FORMS: &[MtsForm] = &[
        MtsForm::Constant,
        MtsForm::PowerLaw,
        MtsForm::Polynomial,
        MtsForm::Rational,
    ];

    // StrengthUnits options
    const STRENGTH_UNITS: &[StrengthUnits] = &[StrengthUnits::SiMpaMm, StrengthUnits::UsKpsiInch];

    let coeff_hint = panel.coefficient_hint.as_str();

    // Endurance section
    let endurance_toggle = checkbox("Endurance data", f.has_endurance)
        .on_toggle(Message::MatToggleEndurance)
        .text_size(SZ_LABEL);

    let mut form_col = column![
        section_heading("Edit material"),
        section_divider(),
        labeled_mat_input("Name", &f.name, MatField::Name),
        labeled_mat_input("Specification", &f.specification, MatField::Spec),
        labeled_mat_input("Citations", &f.citations, MatField::Citations),
        column![
            field_label("MTS form"),
            styled_pick_list(MTS_FORMS, Some(f.mts_form), Message::MatFormKind),
        ]
        .spacing(4),
        column![
            field_label("Units"),
            styled_pick_list(STRENGTH_UNITS, Some(f.mts_units), Message::MatUnits),
        ]
        .spacing(4),
        labeled_mat_input(coeff_hint, &f.coefficients, MatField::Coefficients),
        section_divider(),
        section_heading("Diameter range (mm)"),
        row![
            labeled_mat_input("Min", &f.valid_dia_min, MatField::ValidDiaMin),
            labeled_mat_input("Max", &f.valid_dia_max, MatField::ValidDiaMax),
        ]
        .spacing(10),
        section_divider(),
        section_heading("Elastic constants"),
        labeled_mat_input("Young's modulus (GPa)", &f.youngs_modulus, MatField::Youngs),
        labeled_mat_input("Shear modulus (GPa)", &f.shear_modulus, MatField::Shear),
        labeled_mat_input("Density (kg/m³)", &f.density, MatField::Density),
        section_divider(),
        section_heading("Allowable stress fractions"),
        labeled_mat_input("Torsion", &f.allowable_torsion, MatField::AllowTorsion),
        labeled_mat_input("Bending", &f.allowable_bending, MatField::AllowBending),
        labeled_mat_input("Set", &f.allowable_set, MatField::AllowSet),
        section_divider(),
        endurance_toggle,
    ]
    .spacing(10);

    if panel.show_endurance_fields {
        form_col = form_col
            .push(labeled_mat_input(
                "Endurance Ssa (MPa)",
                &f.endurance_ssa,
                MatField::EnduranceSsa,
            ))
            .push(labeled_mat_input(
                "Endurance Ssm (MPa)",
                &f.endurance_ssm,
                MatField::EnduranceSsm,
            ))
            .push(
                checkbox("Shot-peened", f.endurance_peened)
                    .on_toggle(Message::MatTogglePeened)
                    .text_size(SZ_LABEL),
            );
    }

    let max_temp_toggle = checkbox("Max service temperature (informational)", f.has_max_temp)
        .on_toggle(Message::MatToggleMaxTemp)
        .text_size(SZ_LABEL);

    form_col = form_col.push(section_divider()).push(max_temp_toggle);

    if panel.show_max_temp_field {
        form_col = form_col.push(labeled_mat_input(
            "Max temp (°C)",
            &f.max_temp_c,
            MatField::MaxTemp,
        ));
    }

    // Action buttons
    let save_btn = button(text("Save entry").size(SZ_BODY).color(C::INK))
        .on_press(Message::MatCommit)
        .style(accent_button_style);

    let cancel_btn = button(text("Cancel").size(SZ_BODY).color(C::TEXT))
        .on_press(Message::MatCancel)
        .style(ghost_button_style);

    let action_label = if panel.is_new {
        "New material"
    } else {
        "Editing"
    };
    let action_row = row![
        text(action_label).size(SZ_LABEL).color(C::MUTED),
        horizontal_space(),
        save_btn,
        cancel_btn,
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    form_col = form_col.push(section_divider()).push(action_row);

    let scrolled = scrollable(form_col).height(Length::Fill);

    container(panel_container(scrolled))
        .width(Length::FillPortion(1))
        .height(Length::Fill)
        .into()
}

// --------------------------------------------------------------------------
// Top-level view
// --------------------------------------------------------------------------

/// Build the materials editor screen.
pub(crate) fn view(app: &App) -> Element<'_, Message> {
    let back_btn = button(text("\u{2190} Calculator").size(SZ_LABEL).color(C::ACCENT))
        .on_press(Message::NavigateTo(Screen::Calculator))
        .style(nav_button_style);

    let title = text("Materials").size(SZ_TITLE).color(C::TEXT).font(Font {
        weight: iced::font::Weight::Semibold,
        ..Font::DEFAULT
    });

    let header = row![title, horizontal_space(), back_btn]
        .spacing(16)
        .align_y(iced::Alignment::Center);

    let list_panel = build_list_panel(app);
    let edit_panel = build_edit_panel(app);

    let panels = row![list_panel, edit_panel]
        .spacing(16)
        .height(Length::Fill);

    let mut content = column![header, section_divider()]
        .spacing(16)
        .max_width(1200)
        .height(Length::Fill);

    if let Some(fb) = feedback(app) {
        let color = match fb.kind {
            FeedbackKind::Error => C::DANGER,
            FeedbackKind::Status => C::SUCCESS,
        };
        content = content.push(text(fb.text).size(SZ_LABEL).color(color));
    }

    let content = content.push(panels);

    let root = container(
        container(content)
            .padding(24)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_theme| iced::widget::container::Style {
        background: Some(Background::Color(C::INK)),
        ..Default::default()
    });

    root.into()
}
