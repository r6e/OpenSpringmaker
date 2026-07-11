//! Materials editor screen — list panel + edit-form panel.
//!
//! Pure view logic. No computation here; all business logic lives in
//! `app.rs`, `materials_form.rs`, and `springcore`.

use iced::widget::{button, checkbox, column, container, row, scrollable, space, text, text_input};
use iced::{Background, Element, Font, Length};

use crate::app::{App, MatField, Message, Palette, Screen};
use crate::materials_view_model::{edit_panel, feedback, list_rows, Badge, FeedbackKind};
use crate::widgets::{
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
fn mat_text_input<'a>(
    pal: &'static Palette,
    placeholder: &str,
    value: &str,
    field: MatField,
) -> Element<'a, Message> {
    text_input(placeholder, value)
        .on_input(move |s| Message::MatField(field, s))
        .size(SZ_BODY)
        .font(Font::MONOSPACE)
        .style(text_input_style(pal))
        .into()
}

/// A labeled material text input: muted label above a mat_text_input.
fn labeled_mat_input<'a>(
    pal: &'static Palette,
    label: &str,
    value: &str,
    field: MatField,
) -> Element<'a, Message> {
    column![
        field_label(pal, label.to_owned()),
        mat_text_input(pal, "", value, field),
    ]
    .spacing(4)
    .into()
}

// --------------------------------------------------------------------------
// Sub-panels
// --------------------------------------------------------------------------

fn build_list_panel(app: &App) -> Element<'_, Message> {
    let pal = app.pal();
    let mut list_col = column![].spacing(6);
    // Rendering is driven entirely by the presenter (materials_view_model);
    // which actions a row offers is decided there and unit-tested.
    for r in list_rows(app) {
        let (badge_text, badge_color) = match r.badge {
            Badge::Curated => ("curated \u{1f512}", pal.muted),
            Badge::User => ("user", pal.accent),
        };
        let badge = text(badge_text).size(SZ_LABEL).color(badge_color);

        let mut btn_row = row![badge, space().width(Length::Fill)].spacing(6);
        if r.can_clone {
            btn_row = btn_row.push(
                button(text("Clone").size(SZ_LABEL).color(pal.text))
                    .on_press(Message::MatClone(r.name.clone()))
                    .style(ghost_button_style(pal)),
            );
        }
        if r.can_edit {
            btn_row = btn_row.push(
                button(text("Edit").size(SZ_LABEL).color(pal.text))
                    .on_press(Message::MatEdit(r.name.clone()))
                    .style(ghost_button_style(pal)),
            );
        }
        if r.can_remove {
            btn_row = btn_row.push(
                button(text("Remove").size(SZ_LABEL).color(pal.danger))
                    .on_press(Message::MatDelete(r.name.clone()))
                    .style(danger_button_style(pal)),
            );
        }

        let entry = column![
            mono_value(r.name, pal.text, SZ_BODY),
            btn_row,
            section_divider(pal),
        ]
        .spacing(4);

        list_col = list_col.push(entry);
    }

    let list_scroll = scrollable(list_col).height(Length::Fill);

    let new_btn = button(text("New").size(SZ_BODY).color(pal.ink))
        .on_press(Message::MatNew)
        .style(accent_button_style(pal));

    let persist_btn = button(text("Save to disk").size(SZ_BODY).color(pal.text))
        .on_press(Message::MatPersist)
        .style(ghost_button_style(pal));

    let footer = row![new_btn, persist_btn].spacing(10);

    let inner = column![
        section_heading(pal, "Materials"),
        section_divider(pal),
        list_scroll,
        footer,
    ]
    .spacing(10)
    .height(Length::Fill);

    container(panel_container(pal, inner))
        .width(Length::FillPortion(1))
        .height(Length::Fill)
        .into()
}

fn build_edit_panel(app: &App) -> Element<'_, Message> {
    let pal = app.pal();
    // The presenter decides whether the panel is shown and what it contains
    // (coefficient hint, section visibility, new-vs-editing); the values bound
    // to the inputs come from app.mat_form.
    let panel = match edit_panel(app) {
        None => {
            let hint = text("Select a material to edit, or New.")
                .size(SZ_BODY)
                .color(pal.muted);
            return container(panel_container(pal, hint))
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
    let endurance_toggle = checkbox(f.has_endurance)
        .label("Endurance data")
        .on_toggle(Message::MatToggleEndurance)
        .text_size(SZ_LABEL);

    let mut form_col = column![
        section_heading(pal, "Edit material"),
        section_divider(pal),
        labeled_mat_input(pal, "Name", &f.name, MatField::Name),
        labeled_mat_input(pal, "Specification", &f.specification, MatField::Spec),
        labeled_mat_input(pal, "Citations", &f.citations, MatField::Citations),
        column![
            field_label(pal, "MTS form"),
            styled_pick_list(pal, MTS_FORMS, Some(f.mts_form), Message::MatFormKind),
        ]
        .spacing(4),
        column![
            field_label(pal, "Units"),
            styled_pick_list(pal, STRENGTH_UNITS, Some(f.mts_units), Message::MatUnits),
        ]
        .spacing(4),
        labeled_mat_input(pal, coeff_hint, &f.coefficients, MatField::Coefficients),
        section_divider(pal),
        section_heading(pal, "Diameter range (mm)"),
        row![
            labeled_mat_input(pal, "Min", &f.valid_dia_min, MatField::ValidDiaMin),
            labeled_mat_input(pal, "Max", &f.valid_dia_max, MatField::ValidDiaMax),
        ]
        .spacing(10),
        section_divider(pal),
        section_heading(pal, "Elastic constants"),
        labeled_mat_input(
            pal,
            "Young's modulus (GPa)",
            &f.youngs_modulus,
            MatField::Youngs
        ),
        labeled_mat_input(
            pal,
            "Shear modulus (GPa)",
            &f.shear_modulus,
            MatField::Shear
        ),
        labeled_mat_input(pal, "Density (kg/m³)", &f.density, MatField::Density),
        section_divider(pal),
        section_heading(pal, "Allowable stress fractions"),
        labeled_mat_input(pal, "Torsion", &f.allowable_torsion, MatField::AllowTorsion),
        labeled_mat_input(
            pal,
            "End Torsion",
            &f.allowable_end_torsion,
            MatField::AllowEndTorsion,
        ),
        labeled_mat_input(pal, "Bending", &f.allowable_bending, MatField::AllowBending),
        labeled_mat_input(pal, "Set", &f.allowable_set, MatField::AllowSet),
        section_divider(pal),
        endurance_toggle,
    ]
    .spacing(10);

    if panel.show_endurance_fields {
        form_col = form_col
            .push(labeled_mat_input(
                pal,
                "Endurance Ssa (MPa)",
                &f.endurance_ssa,
                MatField::EnduranceSsa,
            ))
            .push(labeled_mat_input(
                pal,
                "Endurance Ssm (MPa)",
                &f.endurance_ssm,
                MatField::EnduranceSsm,
            ))
            .push(
                checkbox(f.endurance_peened)
                    .label("Shot-peened")
                    .on_toggle(Message::MatTogglePeened)
                    .text_size(SZ_LABEL),
            );
    }

    let max_temp_toggle = checkbox(f.has_max_temp)
        .label("Max service temperature (informational)")
        .on_toggle(Message::MatToggleMaxTemp)
        .text_size(SZ_LABEL);

    form_col = form_col.push(section_divider(pal)).push(max_temp_toggle);

    if panel.show_max_temp_field {
        form_col = form_col.push(labeled_mat_input(
            pal,
            "Max temp (°C)",
            &f.max_temp_c,
            MatField::MaxTemp,
        ));
    }

    // Action buttons
    let save_btn = button(text("Save entry").size(SZ_BODY).color(pal.ink))
        .on_press(Message::MatCommit)
        .style(accent_button_style(pal));

    let cancel_btn = button(text("Cancel").size(SZ_BODY).color(pal.text))
        .on_press(Message::MatCancel)
        .style(ghost_button_style(pal));

    let action_label = if panel.is_new {
        "New material"
    } else {
        "Editing"
    };
    let action_row = row![
        text(action_label).size(SZ_LABEL).color(pal.muted),
        space().width(Length::Fill),
        save_btn,
        cancel_btn,
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    form_col = form_col.push(section_divider(pal)).push(action_row);

    let scrolled = scrollable(form_col).height(Length::Fill);

    container(panel_container(pal, scrolled))
        .width(Length::FillPortion(1))
        .height(Length::Fill)
        .into()
}

// --------------------------------------------------------------------------
// Top-level view
// --------------------------------------------------------------------------

/// Build the materials editor screen.
pub(crate) fn view(app: &App) -> Element<'_, Message> {
    let pal = app.pal();
    let back_btn = button(text("\u{2190} Calculator").size(SZ_LABEL).color(pal.accent))
        .on_press(Message::NavigateTo(Screen::Calculator))
        .style(nav_button_style(pal));

    let title = text("Materials").size(SZ_TITLE).color(pal.text).font(Font {
        weight: iced::font::Weight::Semibold,
        ..Font::DEFAULT
    });

    let header = row![title, space().width(Length::Fill), back_btn]
        .spacing(16)
        .align_y(iced::Alignment::Center);

    let list_panel = build_list_panel(app);
    let edit_panel = build_edit_panel(app);

    let panels = row![list_panel, edit_panel]
        .spacing(16)
        .height(Length::Fill);

    let mut content = column![header, section_divider(pal)]
        .spacing(16)
        .max_width(1200)
        .height(Length::Fill);

    if let Some(fb) = feedback(app) {
        let color = match fb.kind {
            FeedbackKind::Error => pal.danger,
            FeedbackKind::Status => pal.success,
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
    .style(move |_theme| iced::widget::container::Style {
        background: Some(Background::Color(pal.ink)),
        ..Default::default()
    });

    root.into()
}
