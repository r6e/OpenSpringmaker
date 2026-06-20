//! Layout for the OpenSpringmaker GUI. Pure view logic — no computation here.
//!
//! All business logic lives in `form` and `springcore`. This module only
//! assembles iced widgets from the current [`App`] state.

use iced::widget::{
    button, column, container, horizontal_rule, horizontal_space, pick_list, radio, row,
    scrollable, text, text_input,
};
use iced::{Background, Border, Color, Element, Font, Length};

use crate::app::{App, Field, Message, C};
use crate::form::{ScenarioKind, ALL_SCENARIOS};
use springcore::BindingConstraint;
use springcore::UnitSystem;

// --------------------------------------------------------------------------
// KeyLabel newtype for pick-list items
// --------------------------------------------------------------------------

/// A (key, label) pair for end-type and fixity pick-lists.
///
/// The `Display` impl renders the human-readable label; the key is used to
/// store the value in `FormState` and round-trip through save/load.
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

// --------------------------------------------------------------------------
// Style helpers
// --------------------------------------------------------------------------

fn panel_container<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(20)
        .style(|_theme| iced::widget::container::Style {
            background: Some(Background::Color(C::PANEL)),
            border: Border {
                color: C::LINE,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn styled_pick_list<'a, T, L>(
    options: L,
    selected: Option<T>,
    on_select: impl Fn(T) -> Message + 'a,
) -> Element<'a, Message>
where
    T: ToString + PartialEq + Clone + 'a,
    L: std::borrow::Borrow<[T]> + 'a,
{
    pick_list(options, selected, on_select)
        .width(Length::Fill)
        .text_size(14)
        .style(|_theme, _status| iced::widget::pick_list::Style {
            text_color: C::TEXT,
            placeholder_color: C::MUTED,
            handle_color: C::MUTED,
            background: Background::Color(C::RAISED),
            border: Border {
                color: C::LINE,
                width: 1.0,
                radius: 4.0.into(),
            },
        })
        .menu_style(|_theme| iced::widget::overlay::menu::Style {
            background: Background::Color(C::PANEL),
            border: Border {
                color: C::LINE,
                width: 1.0,
                radius: 4.0.into(),
            },
            text_color: C::TEXT,
            selected_text_color: C::INK,
            selected_background: Background::Color(C::ACCENT),
        })
        .into()
}

fn styled_text_input<'a>(placeholder: &str, value: &str, field: Field) -> Element<'a, Message> {
    text_input(placeholder, value)
        .on_input(move |s| Message::Field(field, s))
        .size(14)
        .font(Font::MONOSPACE)
        .style(|_theme, status| {
            use iced::widget::text_input::Status;
            let focused = matches!(status, Status::Focused);
            iced::widget::text_input::Style {
                background: Background::Color(C::RAISED),
                border: Border {
                    color: if focused { C::ACCENT } else { C::LINE },
                    width: if focused { 1.5 } else { 1.0 },
                    radius: 4.0.into(),
                },
                icon: C::MUTED,
                placeholder: C::MUTED,
                value: C::TEXT,
                selection: Color {
                    a: 0.3,
                    ..C::ACCENT
                },
            }
        })
        .into()
}

/// A field label in the muted color at 13px.
fn field_label(label: impl Into<String>) -> Element<'static, Message> {
    text(label.into()).size(13).color(C::MUTED).into()
}

/// A labeled input: muted label above a styled text_input.
fn labeled_input<'a>(label: &str, value: &str, field: Field) -> Element<'a, Message> {
    column![
        field_label(label.to_owned()),
        styled_text_input("", value, field),
    ]
    .spacing(4)
    .into()
}

/// A mono-spaced data value with color control.
fn mono_value(value: impl Into<String>, color: Color, size: u16) -> Element<'static, Message> {
    text(value.into())
        .font(Font::MONOSPACE)
        .size(size)
        .color(color)
        .into()
}

/// A muted label + mono value row, used in results readouts.
fn result_row<'a>(
    label: impl Into<String>,
    value: impl Into<String>,
    unit: impl Into<String>,
) -> Element<'a, Message> {
    row![
        text(label.into())
            .size(13)
            .color(C::MUTED)
            .width(Length::FillPortion(2)),
        text(format!("{} {}", value.into(), unit.into()))
            .font(Font::MONOSPACE)
            .size(14)
            .color(C::TEXT)
            .width(Length::FillPortion(3)),
    ]
    .spacing(8)
    .into()
}

fn section_divider() -> Element<'static, Message> {
    horizontal_rule(1)
        .style(|_theme| iced::widget::rule::Style {
            color: C::LINE,
            width: 1,
            radius: 0.0.into(),
            fill_mode: iced::widget::rule::FillMode::Full,
        })
        .into()
}

fn section_heading(label: impl Into<String>) -> Element<'static, Message> {
    text(label.into())
        .size(13)
        .color(C::MUTED)
        .font(Font {
            weight: iced::font::Weight::Semibold,
            ..Font::DEFAULT
        })
        .into()
}

// --------------------------------------------------------------------------
// Top-level view
// --------------------------------------------------------------------------

/// Build the complete application UI.
pub fn view(app: &App) -> Element<'_, Message> {
    let header = build_header(app);
    let left = build_design_panel(app);
    let right = build_results_panel(app);
    let status = build_status_panel(app);
    let footer = build_footer();

    let header_divider = section_divider();

    let content = column![
        header,
        header_divider,
        row![left, right].spacing(16),
        status,
        footer,
    ]
    .spacing(16)
    .max_width(1200);

    let root = container(scrollable(
        container(content).padding(24).width(Length::Fill),
    ))
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_theme| iced::widget::container::Style {
        background: Some(Background::Color(C::INK)),
        ..Default::default()
    });

    root.into()
}

// --------------------------------------------------------------------------
// Header
// --------------------------------------------------------------------------

fn build_header(app: &App) -> Element<'_, Message> {
    let app_name = text("OpenSpringmaker")
        .size(18)
        .color(C::ACCENT)
        .font(Font {
            weight: iced::font::Weight::Semibold,
            ..Font::DEFAULT
        });

    let unit_metric = radio(
        "Metric (mm / N)",
        UnitSystem::Metric,
        Some(app.form.unit_system),
        Message::Units,
    )
    .text_size(13);

    let unit_us = radio(
        "US (in / lbf)",
        UnitSystem::Us,
        Some(app.form.unit_system),
        Message::Units,
    )
    .text_size(13);

    row![app_name, horizontal_space(), unit_metric, unit_us,]
        .spacing(16)
        .align_y(iced::Alignment::Center)
        .into()
}

// --------------------------------------------------------------------------
// Design (left) panel
// --------------------------------------------------------------------------

fn build_design_panel(app: &App) -> Element<'_, Message> {
    let material_names: Vec<String> = app
        .materials
        .names()
        .into_iter()
        .map(String::from)
        .collect();

    let selected_end = find_by_key(END_TYPES, &app.form.end_type).copied();
    let selected_fix = find_by_key(FIXITIES, &app.form.fixity).copied();

    // Setup group — two columns: material+scenario left, end_type+fixity right.
    let setup_col_a = column![
        column![
            field_label("Material"),
            styled_pick_list(
                material_names,
                Some(app.form.material.clone()),
                Message::Material,
            ),
        ]
        .spacing(4),
        column![
            field_label("Scenario"),
            styled_pick_list(ALL_SCENARIOS, Some(app.form.scenario), Message::Scenario),
        ]
        .spacing(4),
    ]
    .spacing(12)
    .width(Length::FillPortion(1));

    let setup_col_b = column![
        column![
            field_label("End type"),
            styled_pick_list(END_TYPES, selected_end, |kl: KeyLabel| {
                Message::EndType(kl.key.to_string())
            }),
        ]
        .spacing(4),
        column![
            field_label("Fixity"),
            styled_pick_list(FIXITIES, selected_fix, |kl: KeyLabel| {
                Message::Fixity(kl.key.to_string())
            }),
        ]
        .spacing(4),
    ]
    .spacing(12)
    .width(Length::FillPortion(1));

    let setup_row = row![setup_col_a, setup_col_b].spacing(12);

    let setup_group = column![section_heading("Setup"), setup_row,].spacing(10);

    let inputs_group = build_inputs_group(app);

    let inner = column![setup_group, section_divider(), inputs_group,].spacing(16);

    container(panel_container(inner))
        .width(Length::FillPortion(1))
        .into()
}

fn build_inputs_group(app: &App) -> Element<'_, Message> {
    let s = app.form.scenario;
    let us_label = unit_length_label(app.form.unit_system);
    let us_force = unit_force_label(app.form.unit_system);
    let us_rate = unit_rate_label(app.form.unit_system);

    let heading = section_heading("Inputs");

    let col: iced::widget::Column<'_, Message> = if s == ScenarioKind::MinWeight {
        column![
            heading,
            labeled_input(
                &format!("Required rate ({us_rate})"),
                &app.form.rate,
                Field::Rate
            ),
            labeled_input(
                &format!("Max force ({us_force})"),
                &app.form.max_force,
                Field::MaxForce
            ),
            labeled_input("Index min", &app.form.index_min, Field::IndexMin),
            labeled_input("Index max", &app.form.index_max, Field::IndexMax),
            labeled_input(
                &format!("Max outer diameter ({us_label}, optional)"),
                &app.form.max_outer_dia,
                Field::MaxOuterDia,
            ),
            labeled_input(
                &format!("Candidate wire diameters ({us_label}), comma-separated"),
                &app.form.candidate_diameters,
                Field::CandidateDiameters,
            ),
            labeled_input(
                "Clash allowance (fraction)",
                &app.form.clash_allowance,
                Field::ClashAllowance
            ),
        ]
        .spacing(12)
    } else {
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

        let mut col = column![heading, wire_field].spacing(12);

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
            ScenarioKind::MinWeight => unreachable!("MinWeight handled by early-return guard"),
        }

        col = col
            .push(section_divider())
            .push(section_heading("Fatigue cycle (leave blank to skip)"))
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

        col
    };

    col.into()
}

// --------------------------------------------------------------------------
// Results (right) panel
// --------------------------------------------------------------------------

fn build_results_panel(app: &App) -> Element<'_, Message> {
    let us = app.form.unit_system;

    let content: Element<'_, Message> = match &app.outcome {
        None => {
            if let Some(err) = &app.error {
                column![
                    section_heading("Results"),
                    text(err.as_str()).size(13).color(C::DANGER),
                ]
                .spacing(12)
                .into()
            } else {
                column![
                    section_heading("Results"),
                    text("Enter design parameters to see results.")
                        .size(14)
                        .color(C::MUTED),
                ]
                .spacing(12)
                .into()
            }
        }
        Some(out) => {
            let d = &out.design;

            // Governing value — spring rate, large mono in accent
            let rate_label = text("Spring rate").size(13).color(C::MUTED);
            let rate_value = mono_value(
                format!("{:.4} {}", display_rate(d.rate, us), unit_rate_label(us)),
                C::ACCENT,
                22,
            );

            let buckling_color = if d.buckling_stable {
                C::TEXT
            } else {
                C::DANGER
            };
            let buckling_text = if d.buckling_stable {
                "Stable".to_string()
            } else {
                "UNSTABLE".to_string()
            };

            let stress_unit = if us == UnitSystem::Metric {
                "MPa"
            } else {
                "ksi"
            };

            // Per-load table
            let load_section_heading = section_heading("Load points");
            let mut load_col = column![load_section_heading].spacing(4);

            // Column header
            load_col = load_col.push(
                row![
                    text("Pt")
                        .size(11)
                        .color(C::MUTED)
                        .width(Length::Fixed(24.0)),
                    text("Force")
                        .size(11)
                        .color(C::MUTED)
                        .width(Length::FillPortion(2)),
                    text("Deflection")
                        .size(11)
                        .color(C::MUTED)
                        .width(Length::FillPortion(2)),
                    text("Length")
                        .size(11)
                        .color(C::MUTED)
                        .width(Length::FillPortion(2)),
                    text(format!("Stress ({stress_unit})"))
                        .size(11)
                        .color(C::MUTED)
                        .width(Length::FillPortion(2)),
                    text("%MTS")
                        .size(11)
                        .color(C::MUTED)
                        .width(Length::FillPortion(1)),
                ]
                .spacing(4),
            );

            for (i, lp) in d.load_points.iter().enumerate() {
                let (stress_val, _stress_lbl) = display_stress(lp.shear_stress, us);
                let load_row = row![
                    text(format!("{}", i + 1))
                        .font(Font::MONOSPACE)
                        .size(13)
                        .color(C::MUTED)
                        .width(Length::Fixed(24.0)),
                    text(format!(
                        "{:.3} {}",
                        display_force(lp.force, us),
                        unit_force_label(us)
                    ))
                    .font(Font::MONOSPACE)
                    .size(13)
                    .color(C::TEXT)
                    .width(Length::FillPortion(2)),
                    text(format!(
                        "{:.4} {}",
                        display_len(lp.deflection, us),
                        unit_length_label(us)
                    ))
                    .font(Font::MONOSPACE)
                    .size(13)
                    .color(C::TEXT)
                    .width(Length::FillPortion(2)),
                    text(format!(
                        "{:.4} {}",
                        display_len(lp.length, us),
                        unit_length_label(us)
                    ))
                    .font(Font::MONOSPACE)
                    .size(13)
                    .color(C::TEXT)
                    .width(Length::FillPortion(2)),
                    text(format!("{stress_val:.3}"))
                        .font(Font::MONOSPACE)
                        .size(13)
                        .color(C::TEXT)
                        .width(Length::FillPortion(2)),
                    text(format!("{:.1}%", lp.pct_mts * 100.0))
                        .font(Font::MONOSPACE)
                        .size(13)
                        .color(C::TEXT)
                        .width(Length::FillPortion(1)),
                ]
                .spacing(4);
                load_col = load_col.push(load_row);
            }

            // Fatigue section
            let fatigue_section: Element<'_, Message> = if out.min_weight.is_none() {
                match &out.fatigue {
                    Some(fat) => {
                        let (alt_val, alt_lbl) = display_stress(fat.alternating_stress, us);
                        let (mean_val, mean_lbl) = display_stress(fat.mean_stress, us);
                        let (endurance_val, endurance_lbl) =
                            display_stress(fat.fully_reversed_endurance, us);
                        let (ssu_val, ssu_lbl) = display_stress(fat.ultimate_shear, us);
                        column![
                            section_divider(),
                            section_heading("Fatigue analysis"),
                            result_row("Alternating stress", format!("{alt_val:.2}"), alt_lbl),
                            result_row("Mean stress", format!("{mean_val:.2}"), mean_lbl),
                            result_row(
                                "Endurance (S\u{2032}\u{2032}se)",
                                format!("{endurance_val:.2}"),
                                endurance_lbl,
                            ),
                            result_row("Ultimate shear (Ssu)", format!("{ssu_val:.2}"), ssu_lbl),
                            result_row(
                                "Goodman FOS",
                                format!("{:.3}", fat.goodman_factor_of_safety),
                                "",
                            ),
                        ]
                        .spacing(6)
                        .into()
                    }
                    None => column![
                        section_divider(),
                        text("No fatigue data for this material.")
                            .size(13)
                            .color(C::MUTED),
                    ]
                    .spacing(8)
                    .into(),
                }
            } else {
                column![].into()
            };

            // Min weight section
            let min_weight_section: Element<'_, Message> = if let Some(mw) = &out.min_weight {
                let binding_label = match mw.binding {
                    BindingConstraint::Stress => "stress",
                    BindingConstraint::Index => "index",
                    BindingConstraint::OuterDiameter => "outer diameter",
                };
                column![
                    section_divider(),
                    section_heading("Min-weight optimisation"),
                    result_row("Wire mass", format!("{:.4}", mw.mass_kg), "kg"),
                    result_row("Binding constraint", binding_label, ""),
                ]
                .spacing(6)
                .into()
            } else {
                column![].into()
            };

            // Chart
            let chart = crate::plot::results_chart(d, us);

            column![
                section_heading("Results"),
                section_divider(),
                // Governing value
                rate_label,
                rate_value,
                section_divider(),
                // Key geometry
                section_heading("Geometry"),
                result_row("Spring index", format!("{:.3}", d.index), ""),
                result_row("Active coils", format!("{:.3}", d.active_coils), ""),
                result_row("Total coils", format!("{:.3}", d.total_coils), ""),
                result_row(
                    "Free length",
                    format!("{:.4}", display_len(d.free_length, us)),
                    unit_length_label(us),
                ),
                result_row(
                    "Solid length",
                    format!("{:.4}", display_len(d.solid_length, us)),
                    unit_length_label(us),
                ),
                result_row(
                    "Outer diameter",
                    format!("{:.4}", display_len(d.outer_dia, us)),
                    unit_length_label(us),
                ),
                result_row(
                    "Inner diameter",
                    format!("{:.4}", display_len(d.inner_dia, us)),
                    unit_length_label(us),
                ),
                result_row(
                    "Natural frequency",
                    format!("{:.2}", d.natural_frequency.hertz()),
                    "Hz"
                ),
                row![
                    text("Buckling")
                        .size(13)
                        .color(C::MUTED)
                        .width(Length::FillPortion(2)),
                    text(buckling_text)
                        .font(Font::MONOSPACE)
                        .size(14)
                        .color(buckling_color)
                        .width(Length::FillPortion(3)),
                ]
                .spacing(8),
                section_divider(),
                load_col,
                fatigue_section,
                min_weight_section,
                section_divider(),
                chart,
            ]
            .spacing(6)
            .into()
        }
    };

    container(panel_container(content))
        .width(Length::FillPortion(1))
        .into()
}

// --------------------------------------------------------------------------
// Status panel
// --------------------------------------------------------------------------

fn build_status_panel(app: &App) -> Element<'_, Message> {
    use springcore::Severity;

    // Only render when there's something to say.
    let has_messages = app
        .outcome
        .as_ref()
        .map(|o| !o.status.messages.is_empty())
        .unwrap_or(false);

    if !has_messages {
        return column![].into();
    }

    let mut col = column![section_heading("Design status")].spacing(6);

    if let Some(out) = &app.outcome {
        for msg in &out.status.messages {
            let (prefix, color) = match msg.severity {
                Severity::Info => ("Info:", C::MUTED),
                Severity::Caution => ("Caution:", C::WARN),
                Severity::Warning => ("Warning:", C::DANGER),
            };
            let status_row = row![
                text(prefix)
                    .size(13)
                    .color(color)
                    .width(Length::Fixed(72.0)),
                text(msg.message.as_str()).size(13).color(color),
            ]
            .spacing(8);
            col = col.push(status_row);
        }
    }

    panel_container(col)
}

// --------------------------------------------------------------------------
// Footer
// --------------------------------------------------------------------------

fn build_footer() -> Element<'static, Message> {
    let save_btn = button(text("Save design").size(14).color(C::INK))
        .on_press(Message::Save)
        .style(|_theme, status| {
            use iced::widget::button::Status;
            let base_bg = if matches!(status, Status::Hovered) {
                Color {
                    r: C::ACCENT.r * 0.85,
                    g: C::ACCENT.g * 0.85,
                    b: C::ACCENT.b * 0.85,
                    a: 1.0,
                }
            } else {
                C::ACCENT
            };
            iced::widget::button::Style {
                background: Some(Background::Color(base_bg)),
                text_color: C::INK,
                border: Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                shadow: Default::default(),
            }
        });

    let load_btn = button(text("Load design").size(14).color(C::TEXT))
        .on_press(Message::Load)
        .style(|_theme, status| {
            use iced::widget::button::Status;
            let border_color = if matches!(status, Status::Hovered) {
                C::TEXT
            } else {
                C::LINE
            };
            iced::widget::button::Style {
                background: Some(Background::Color(Color::TRANSPARENT)),
                text_color: C::TEXT,
                border: Border {
                    color: border_color,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                shadow: Default::default(),
            }
        });

    row![save_btn, load_btn].spacing(12).into()
}

// --------------------------------------------------------------------------
// Unit display helpers
// --------------------------------------------------------------------------

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
