//! Layout for the OpenSpringmaker GUI. Pure view logic — no computation here.
//!
//! All business logic lives in `form` and `springcore`. This module only
//! assembles iced widgets from the current [`App`] state.

use iced::widget::{
    button, column, container, pick_list, radio, row, scrollable, text, text_input,
};
use iced::{Element, Length};

use crate::app::{App, Field, Message};
use crate::form::{ScenarioKind, ALL_SCENARIOS};
use springcore::UnitSystem;

// Static option lists for pick-lists backed by string keys.
const END_TYPES: &[&str] = &["plain", "plain_ground", "squared", "squared_ground"];
const FIXITIES: &[&str] = &["fixed_fixed", "fixed_pinned", "pinned_pinned", "fixed_free"];

/// Build the complete application UI.
pub fn view(app: &App) -> Element<'_, Message> {
    let header = build_header(app);
    let inputs = build_inputs(app);
    let outputs = build_outputs(app);
    let status = build_status(app);
    let actions = build_actions();

    let body = column![header, row![inputs, outputs].spacing(20), status, actions]
        .spacing(16)
        .padding(20);

    scrollable(container(body).width(Length::Fill)).into()
}

// --- header row -----------------------------------------------------------

fn build_header(app: &App) -> Element<'_, Message> {
    let material_names: Vec<String> = app
        .materials
        .names()
        .into_iter()
        .map(String::from)
        .collect();

    let mat_pick = pick_list(
        material_names,
        Some(app.form.material.clone()),
        Message::Material,
    );

    let scenario_pick = pick_list(ALL_SCENARIOS, Some(app.form.scenario), Message::Scenario);

    let unit_metric = radio(
        "Metric (mm / N)",
        UnitSystem::Metric,
        Some(app.form.unit_system),
        Message::Units,
    );
    let unit_us = radio(
        "US (in / lbf)",
        UnitSystem::Us,
        Some(app.form.unit_system),
        Message::Units,
    );

    // end_type and fixity are stored as string keys
    let end_pick = pick_list(END_TYPES, Some(app.form.end_type.as_str()), |s: &str| {
        Message::EndType(s.to_string())
    });
    let fix_pick = pick_list(FIXITIES, Some(app.form.fixity.as_str()), |s: &str| {
        Message::Fixity(s.to_string())
    });

    row![
        text("Material:"),
        mat_pick,
        text("Scenario:"),
        scenario_pick,
        unit_metric,
        unit_us,
        text("End Type:"),
        end_pick,
        text("Fixity:"),
        fix_pick,
    ]
    .spacing(8)
    .into()
}

// --- input column ---------------------------------------------------------

fn build_inputs(app: &App) -> Element<'_, Message> {
    let s = app.form.scenario;
    let us_label = unit_length_label(app.form.unit_system);
    let us_force = unit_force_label(app.form.unit_system);
    let us_rate = unit_rate_label(app.form.unit_system);

    // Fields shared by all scenarios
    let wire_field = labeled_input(
        &format!("Wire diameter ({us_label})"),
        &app.form.wire_dia,
        Field::WireDia,
    );
    let mean_field = labeled_input(
        &format!("Mean diameter ({us_label})"),
        &app.form.mean_dia,
        Field::MeanDia,
    );

    let mut col = column![wire_field, mean_field].spacing(8);

    match s {
        ScenarioKind::PowerUser => {
            col = col
                .push(labeled_input(
                    "Active coils",
                    &app.form.active,
                    Field::Active,
                ))
                .push(labeled_input(
                    &format!("Free length ({us_label})"),
                    &app.form.free_length,
                    Field::FreeLength,
                ))
                .push(labeled_input(
                    &format!("Loads ({us_force}), comma-separated"),
                    &app.form.loads,
                    Field::Loads,
                ));
        }
        ScenarioKind::TwoLoad => {
            col = col
                .push(labeled_input(
                    &format!("Force 1 ({us_force})"),
                    &app.form.force1,
                    Field::Force1,
                ))
                .push(labeled_input(
                    &format!("Length 1 ({us_label})"),
                    &app.form.length1,
                    Field::Length1,
                ))
                .push(labeled_input(
                    &format!("Force 2 ({us_force})"),
                    &app.form.force2,
                    Field::Force2,
                ))
                .push(labeled_input(
                    &format!("Length 2 ({us_label})"),
                    &app.form.length2,
                    Field::Length2,
                ));
        }
        ScenarioKind::RateBased => {
            col = col
                .push(labeled_input(
                    &format!("Spring rate ({us_rate})"),
                    &app.form.rate,
                    Field::Rate,
                ))
                .push(labeled_input(
                    &format!("Free length ({us_label})"),
                    &app.form.free_length,
                    Field::FreeLength,
                ))
                .push(labeled_input(
                    &format!("Loads ({us_force}), comma-separated"),
                    &app.form.loads,
                    Field::Loads,
                ));
        }
        ScenarioKind::Dimensional => {
            col = col
                .push(labeled_input(
                    &format!("Outer diameter ({us_label})"),
                    &app.form.outer_dia,
                    Field::OuterDia,
                ))
                .push(labeled_input(
                    "Active coils",
                    &app.form.active,
                    Field::Active,
                ))
                .push(labeled_input(
                    &format!("Free length ({us_label})"),
                    &app.form.free_length,
                    Field::FreeLength,
                ))
                .push(labeled_input(
                    &format!("Loads ({us_force}), comma-separated"),
                    &app.form.loads,
                    Field::Loads,
                ));
        }
    }

    // Fatigue inputs — always visible (leave blank to skip)
    col = col
        .push(text("--- Fatigue cycle (leave blank to skip) ---"))
        .push(labeled_input(
            &format!("Min cycle force ({us_force})"),
            &app.form.fatigue_min,
            Field::FatigueMin,
        ))
        .push(labeled_input(
            &format!("Max cycle force ({us_force})"),
            &app.form.fatigue_max,
            Field::FatigueMax,
        ));

    container(col.width(Length::Fill))
        .width(Length::FillPortion(1))
        .into()
}

fn labeled_input<'a>(label: &str, value: &str, field: Field) -> Element<'a, Message> {
    column![
        text(label.to_owned()),
        text_input("", value).on_input(move |s| Message::Field(field, s))
    ]
    .spacing(2)
    .into()
}

// --- output column --------------------------------------------------------

fn build_outputs(app: &App) -> Element<'_, Message> {
    let us = app.form.unit_system;
    let col = match &app.outcome {
        None => column![text("Enter design parameters above to see results.")],
        Some(out) => {
            let d = &out.design;

            let index_row = text(format!("Spring index C: {:.3}", d.index));
            let rate_row = text(format!(
                "Rate: {:.4} {}",
                display_rate(d.rate, us),
                unit_rate_label(us)
            ));
            let coils_row = text(format!(
                "Active coils: {:.3}  Total coils: {:.3}",
                d.active_coils, d.total_coils
            ));
            let lengths_row = text(format!(
                "Free length: {:.4} {}  Solid length: {:.4} {}",
                display_len(d.free_length, us),
                unit_length_label(us),
                display_len(d.solid_length, us),
                unit_length_label(us),
            ));
            let dia_row = text(format!(
                "OD: {:.4} {}  ID: {:.4} {}",
                display_len(d.outer_dia, us),
                unit_length_label(us),
                display_len(d.inner_dia, us),
                unit_length_label(us),
            ));
            let freq_row = text(format!(
                "Natural frequency: {:.2} Hz",
                d.natural_frequency.hertz()
            ));
            let buckle_row = text(if d.buckling_stable {
                "Buckling: stable".to_string()
            } else {
                "Buckling: UNSTABLE".to_string()
            });

            // Per-load table header + rows
            let load_header =
                text("Load pt  |  Force      |  Deflection  |  Length      |  Stress      |  %MTS");
            let mut load_rows: Vec<Element<'_, Message>> = vec![load_header.into()];
            for (i, lp) in d.load_points.iter().enumerate() {
                let row_text = text(format!(
                    "  {:>4}    |  {:>8.3} {} |  {:>8.4} {}  |  {:>8.4} {}  |  {:>8.3} MPa  |  {:>5.1}%",
                    i + 1,
                    display_force(lp.force, us),
                    unit_force_label(us),
                    display_len(lp.deflection, us),
                    unit_length_label(us),
                    display_len(lp.length, us),
                    unit_length_label(us),
                    lp.shear_stress.megapascals(),
                    lp.pct_mts * 100.0,
                ));
                load_rows.push(row_text.into());
            }

            // Fatigue
            let fatigue_section: Element<'_, Message> = match &out.fatigue {
                Some(fat) => column![
                    text("--- Fatigue Analysis ---"),
                    text(format!(
                        "Alternating stress: {:.2} MPa",
                        fat.alternating_stress.megapascals()
                    )),
                    text(format!(
                        "Mean stress: {:.2} MPa",
                        fat.mean_stress.megapascals()
                    )),
                    text(format!(
                        "Fully-reversed endurance (S''se): {:.2} MPa",
                        fat.fully_reversed_endurance.megapascals()
                    )),
                    text(format!(
                        "Goodman factor of safety: {:.3}",
                        fat.goodman_factor_of_safety
                    )),
                ]
                .spacing(4)
                .into(),
                None => text("No fatigue data for this material.").into(),
            };

            let mut col = column![
                index_row,
                rate_row,
                coils_row,
                lengths_row,
                dia_row,
                freq_row,
                buckle_row,
                text("--- Load Points ---"),
            ]
            .spacing(4);

            for lr in load_rows {
                col = col.push(lr);
            }
            col = col.push(fatigue_section);
            col
        }
    };

    container(col.spacing(4).width(Length::Fill))
        .width(Length::FillPortion(1))
        .into()
}

// --- status panel ---------------------------------------------------------

fn build_status(app: &App) -> Element<'_, Message> {
    use springcore::Severity;

    let mut col = column![text("--- Design Status ---")].spacing(4);

    if let Some(out) = &app.outcome {
        if out.status.messages.is_empty() {
            col = col.push(text("All checks passed."));
        } else {
            for msg in &out.status.messages {
                let prefix = match msg.severity {
                    Severity::Info => "[Info]",
                    Severity::Caution => "[Caution]",
                    Severity::Warning => "[WARNING]",
                };
                col = col.push(text(format!("{prefix} {}", msg.message)));
            }
        }
    }

    if let Some(err) = &app.error {
        col = col.push(text(format!("[Error] {err}")));
    }

    container(col).padding(8).into()
}

// --- action buttons -------------------------------------------------------

fn build_actions() -> Element<'static, Message> {
    row![
        button("Save design...").on_press(Message::Save),
        button("Load design...").on_press(Message::Load),
    ]
    .spacing(12)
    .into()
}

// --- unit display helpers -------------------------------------------------

fn unit_length_label(us: UnitSystem) -> &'static str {
    match us {
        UnitSystem::Metric => "mm",
        UnitSystem::Us => "in",
    }
}

fn unit_force_label(us: UnitSystem) -> &'static str {
    match us {
        UnitSystem::Metric => "N",
        UnitSystem::Us => "lbf",
    }
}

fn unit_rate_label(us: UnitSystem) -> &'static str {
    match us {
        UnitSystem::Metric => "N/m",
        UnitSystem::Us => "lbf/in",
    }
}

fn display_len(l: springcore::Length, us: UnitSystem) -> f64 {
    match us {
        UnitSystem::Metric => l.millimeters(),
        UnitSystem::Us => l.inches(),
    }
}

fn display_force(f: springcore::Force, us: UnitSystem) -> f64 {
    match us {
        UnitSystem::Metric => f.newtons(),
        UnitSystem::Us => f.pounds_force(),
    }
}

fn display_rate(r: springcore::SpringRate, us: UnitSystem) -> f64 {
    match us {
        UnitSystem::Metric => r.newtons_per_meter(),
        UnitSystem::Us => r.pounds_per_inch(),
    }
}
