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

// --------------------------------------------------------------------------
// Newtype for pick-list items that show a human-readable label while
// storing the snake_case key in the form.
// --------------------------------------------------------------------------

/// A (key, label) pair for end-type and fixity pick-lists.
///
/// The `Display` impl renders the human-readable label; the key is used to
/// store the value in [`FormState`] and round-trip through save/load.
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

    // end_type and fixity are stored as snake_case keys; pick-lists show human labels.
    let selected_end = find_by_key(END_TYPES, &app.form.end_type).copied();
    let selected_fix = find_by_key(FIXITIES, &app.form.fixity).copied();
    let end_pick = pick_list(END_TYPES, selected_end, |kl: KeyLabel| {
        Message::EndType(kl.key.to_string())
    });
    let fix_pick = pick_list(FIXITIES, selected_fix, |kl: KeyLabel| {
        Message::Fixity(kl.key.to_string())
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

    // Wire diameter is shared by all scenarios.
    let wire_field = labeled_input(
        &format!("Wire diameter ({us_label})"),
        &app.form.wire_dia,
        Field::WireDia,
    );

    // Mean diameter is owned by PowerUser / TwoLoad / RateBased.
    // Dimensional owns outer_dia instead; rendering mean_dia there would silently
    // discard edits since build_spec never reads it for that variant.
    let mean_field = labeled_input(
        &format!("Mean diameter ({us_label})"),
        &app.form.mean_dia,
        Field::MeanDia,
    );

    let mut col = column![wire_field].spacing(8);

    match s {
        ScenarioKind::PowerUser => {
            col = col
                .push(mean_field)
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
                .push(mean_field)
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
                .push(mean_field)
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
            let stress_unit = if us == UnitSystem::Metric {
                "MPa"
            } else {
                "ksi"
            };
            let load_header = text(format!(
                "Load pt  |  Force      |  Deflection  |  Length      |  Stress ({stress_unit})  |  %MTS"
            ));
            let mut load_rows: Vec<Element<'_, Message>> = vec![load_header.into()];
            for (i, lp) in d.load_points.iter().enumerate() {
                let (stress_val, stress_lbl) = display_stress(lp.shear_stress, us);
                let row_text = text(format!(
                    "  {:>4}    |  {:>8.3} {} |  {:>8.4} {}  |  {:>8.4} {}  |  {:>8.3} {}  |  {:>5.1}%",
                    i + 1,
                    display_force(lp.force, us),
                    unit_force_label(us),
                    display_len(lp.deflection, us),
                    unit_length_label(us),
                    display_len(lp.length, us),
                    unit_length_label(us),
                    stress_val,
                    stress_lbl,
                    lp.pct_mts * 100.0,
                ));
                load_rows.push(row_text.into());
            }

            // Fatigue
            let fatigue_section: Element<'_, Message> = match &out.fatigue {
                Some(fat) => {
                    let (alt_val, alt_lbl) = display_stress(fat.alternating_stress, us);
                    let (mean_val, mean_lbl) = display_stress(fat.mean_stress, us);
                    let (endur_val, endur_lbl) = display_stress(fat.fully_reversed_endurance, us);
                    let (ssu_val, ssu_lbl) = display_stress(fat.ultimate_shear, us);
                    column![
                        text("--- Fatigue Analysis ---"),
                        text(format!("Alternating stress: {alt_val:.2} {alt_lbl}")),
                        text(format!("Mean stress: {mean_val:.2} {mean_lbl}")),
                        text(format!(
                            "Fully-reversed endurance (S\u{2032}\u{2032}se): {endur_val:.2} {endur_lbl}"
                        )),
                        text(format!("Ultimate shear strength (Ssu): {ssu_val:.2} {ssu_lbl}")),
                        text(format!(
                            "Goodman factor of safety: {:.3}",
                            fat.goodman_factor_of_safety
                        )),
                    ]
                    .spacing(4)
                    .into()
                }
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
            col = col.push(crate::plot::results_chart(d, us));
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

/// Returns `(value, label)` for a stress in the active unit system.
/// Metric → MPa; US → ksi.
fn display_stress(s: springcore::Stress, us: UnitSystem) -> (f64, &'static str) {
    match us {
        UnitSystem::Metric => (s.megapascals(), "MPa"),
        UnitSystem::Us => (s.psi() / 1000.0, "ksi"),
    }
}
