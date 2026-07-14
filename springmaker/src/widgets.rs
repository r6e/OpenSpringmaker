//! Shared iced widget and style kit used by every screen (calculator, materials,
//! settings). Family- and screen-agnostic presentational helpers; depends only on
//! the app shell's color palette (`Palette`) and `Message`. Screen-specific widgets
//! live in that screen's own view module.

use iced::widget::{button, column, container, pick_list, row, rule, scrollable, text, text_input};
use iced::{Background, Border, Color, Element, Font, Length};

use crate::app::{App, Message, Palette, VisualMode};
use crate::presenter::{Emphasis, GoverningRate, ResultRow};

// --------------------------------------------------------------------------
// Font-size constants
// --------------------------------------------------------------------------

pub(crate) const SZ_LABEL: u32 = 13;
pub(crate) const SZ_BODY: u32 = 14;
pub(crate) const SZ_TITLE: u32 = 18;

// --------------------------------------------------------------------------
// Spacing tokens
// --------------------------------------------------------------------------

/// Extra-small spacing unit (4px).
pub(crate) const SP_XS: f32 = 4.0;

/// Row spacing for results panels (6px) — deliberate token for results-row rhythm.
pub(crate) const SP_ROW: f32 = 6.0;

/// Small spacing unit (8px).
pub(crate) const SP_SM: f32 = 8.0;

/// Medium spacing unit (12px) — note: 10→12 value change during visual refresh.
pub(crate) const SP_MD: f32 = 12.0;

/// Large spacing unit (16px).
pub(crate) const SP_LG: f32 = 16.0;

/// Panel padding (20px).
pub(crate) const PANEL_PAD: f32 = 20.0;

/// Extra-large spacing unit (24px).
pub(crate) const SP_XL: f32 = 24.0;

/// Max content width for the screen shell (1200px) — the cap applies to the
/// CONTENT, not the padded box around it (see `screen_shell`).
pub(crate) const CONTENT_MAX_W: f32 = 1200.0;

// --------------------------------------------------------------------------
// Fixed-width columns
// --------------------------------------------------------------------------

/// Pt column width (24.0).
pub(crate) const COL_PT: f32 = 24.0;

/// Status prefix column width (72.0).
pub(crate) const COL_STATUS_PREFIX: f32 = 72.0;

/// Header gap width (160.0).
pub(crate) const HEADER_GAP: f32 = 160.0;

pub(crate) fn panel_container<'a>(
    pal: &'static Palette,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(content)
        .padding(PANEL_PAD)
        .style(move |_theme| iced::widget::container::Style {
            background: Some(Background::Color(pal.panel)),
            border: Border {
                color: pal.line,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .into()
}

/// A member-level sub-card: a `raised`-background bordered box (radius 4,
/// padding `SP_SM`) — one step lighter and tighter than [`panel_container`]'s
/// larger `panel`-background parent panels. Groups one assembly member's
/// results visually within the shared results panel.
pub(crate) fn member_sub_card<'a>(
    pal: &'static Palette,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(content)
        .padding(SP_SM)
        .style(move |_theme| iced::widget::container::Style {
            background: Some(Background::Color(pal.raised)),
            border: Border {
                color: pal.line,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        })
        .into()
}

/// Shared root chrome for every screen (calculator, materials, settings):
/// padding `SP_XL`, a `CONTENT_MAX_W` max content width, and the app's `ink`
/// background — collapses the three near-identical root-container copies
/// that used to duplicate this (and let Settings drift to its own 800px
/// cap) into one definition. `scroll` wraps the padded content in an outer
/// `scrollable`; materials passes `false` since its list/edit panels already
/// scroll internally and an outer scrollable would fight them over height.
///
/// The width cap is nested INSIDE the padding (an inner container caps
/// `content` at `CONTENT_MAX_W`; the outer container only pads, uncapped) —
/// not one container doing both. A single `container(content).padding(SP_XL)
/// .max_width(CONTENT_MAX_W)` would apply the 1200px cap to the padded box
/// as a whole (iced's `Limits::max_width` binds before padding is
/// subtracted), so content itself would max out at `CONTENT_MAX_W - 2 *
/// SP_XL` (1152px) instead of the pre-refresh 1200px. Nesting restores the
/// original two-container split (content had its own `.max_width(1200)`;
/// the wrapping container only padded, uncapped) that predates this shell's
/// collapse of the three screens' near-identical root chrome.
pub(crate) fn screen_shell<'a>(
    pal: &'static Palette,
    content: impl Into<Element<'a, Message>>,
    scroll: bool,
) -> Element<'a, Message> {
    let capped = container(content)
        .max_width(CONTENT_MAX_W)
        .width(Length::Fill);

    let padded = container(capped).padding(SP_XL).width(Length::Fill);

    let inner: Element<'a, Message> = if scroll {
        scrollable(padded).into()
    } else {
        padded.height(Length::Fill).into()
    };

    container(inner)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme| iced::widget::container::Style {
            background: Some(Background::Color(pal.ink)),
            ..Default::default()
        })
        .into()
}

/// A muted placeholder message shown when a chart or 3D scene cannot be
/// rendered for the current design. Extracted so `chart_element` and
/// `scene_element`'s `None` arms (previously each built a bare, unstyled
/// `text()` inline) render identically styled, state-aware wording.
pub(crate) fn placeholder_text(pal: &'static Palette, msg: &str) -> Element<'static, Message> {
    text(msg.to_string()).size(SZ_BODY).color(pal.muted).into()
}

pub(crate) fn styled_pick_list<'a, T, L>(
    pal: &'static Palette,
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
        .text_size(SZ_BODY)
        .style(move |_theme, _status| iced::widget::pick_list::Style {
            text_color: pal.text,
            placeholder_color: pal.muted,
            handle_color: pal.muted,
            background: Background::Color(pal.raised),
            border: Border {
                color: pal.line,
                width: 1.0,
                radius: 4.0.into(),
            },
        })
        .menu_style(move |_theme| iced::widget::overlay::menu::Style {
            background: Background::Color(pal.panel),
            border: Border {
                color: pal.line,
                width: 1.0,
                radius: 4.0.into(),
            },
            text_color: pal.text,
            selected_text_color: pal.ink,
            selected_background: Background::Color(pal.accent),
            shadow: iced::Shadow::default(),
        })
        .into()
}

/// Shared text-input style used by both the calculator and materials editor.
pub(crate) fn text_input_style(
    pal: &'static Palette,
) -> impl Fn(&iced::Theme, iced::widget::text_input::Status) -> iced::widget::text_input::Style {
    move |_theme, status| {
        use iced::widget::text_input::Status;
        let focused = matches!(status, Status::Focused { .. });
        iced::widget::text_input::Style {
            background: Background::Color(pal.raised),
            border: Border {
                color: if focused { pal.accent } else { pal.line },
                width: if focused { 1.5 } else { 1.0 },
                radius: 4.0.into(),
            },
            icon: pal.muted,
            placeholder: pal.muted,
            value: pal.text,
            // Selection alpha 0.3: direction-safe (works on both dark and light themes).
            selection: Color {
                a: 0.3,
                ..pal.accent
            },
        }
    }
}

/// A field label in the muted color at 13px.
pub(crate) fn field_label(
    pal: &'static Palette,
    label: impl Into<String>,
) -> Element<'static, Message> {
    text(label.into()).size(SZ_LABEL).color(pal.muted).into()
}

/// The material selector (`Material` label + pick-list), shared by every family's
/// design panel. Reads only app-shell state (`app.materials`, `app.material`), so
/// it carries no family dependency.
pub(crate) fn material_picker(app: &App) -> Element<'_, Message> {
    let pal = app.pal();
    let material_names: Vec<String> = app
        .materials
        .names()
        .into_iter()
        .map(String::from)
        .collect();
    column![
        field_label(pal, "Material"),
        styled_pick_list(
            pal,
            material_names,
            Some(app.material.clone()),
            Message::Material
        ),
    ]
    .spacing(SP_XS)
    .into()
}

/// A mono-spaced data value with color control.
pub(crate) fn mono_value(
    value: impl Into<String>,
    color: Color,
    size: u32,
) -> Element<'static, Message> {
    text(value.into())
        .font(Font::MONOSPACE)
        .size(size)
        .color(color)
        .into()
}

pub(crate) fn section_divider(pal: &'static Palette) -> Element<'static, Message> {
    rule::horizontal(1)
        .style(move |_theme| iced::widget::rule::Style {
            color: pal.line,
            radius: 0.0.into(),
            fill_mode: iced::widget::rule::FillMode::Full,
            snap: true,
        })
        .into()
}

pub(crate) fn section_heading(
    pal: &'static Palette,
    label: impl Into<String>,
) -> Element<'static, Message> {
    text(label.into())
        .size(SZ_LABEL)
        .color(pal.muted)
        .font(Font {
            weight: iced::font::Weight::Semibold,
            ..Font::DEFAULT
        })
        .into()
}

/// Shared outline-button style factory: transparent background, `color`
/// text, and a border that's `color` on hover, `pal.line` otherwise — the
/// shape common to `ghost_button_style`, `danger_button_style`, and
/// `nav_button_style` (each just names a different palette color).
fn outline_button_style(
    pal: &'static Palette,
    color: Color,
) -> impl Fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    move |_theme, status| {
        use iced::widget::button::Status;
        let border_color = if matches!(status, Status::Hovered) {
            color
        } else {
            pal.line
        };
        iced::widget::button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            text_color: color,
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 4.0.into(),
            },
            shadow: Default::default(),
            snap: Default::default(),
        }
    }
}

/// Ghost/outline button style (used for secondary actions).
pub(crate) fn ghost_button_style(
    pal: &'static Palette,
) -> impl Fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    outline_button_style(pal, pal.text)
}

/// Danger/destructive ghost button style (remove actions).
pub(crate) fn danger_button_style(
    pal: &'static Palette,
) -> impl Fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    outline_button_style(pal, pal.danger)
}

/// Accent/primary filled button style (save/commit actions).
pub(crate) fn accent_button_style(
    pal: &'static Palette,
) -> impl Fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    move |_theme, status| {
        use iced::widget::button::Status;
        let bg = if matches!(status, Status::Hovered) {
            // Darken by ×0.85 on hover: direction-safe (works on both dark and light themes).
            Color {
                r: pal.accent.r * 0.85,
                g: pal.accent.g * 0.85,
                b: pal.accent.b * 0.85,
                a: 1.0,
            }
        } else {
            pal.accent
        };
        iced::widget::button::Style {
            background: Some(Background::Color(bg)),
            text_color: pal.ink,
            border: Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            shadow: Default::default(),
            snap: Default::default(),
        }
    }
}

/// Accent-outline nav button style (navigation actions with accent color text/border).
pub(crate) fn nav_button_style(
    pal: &'static Palette,
) -> impl Fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    outline_button_style(pal, pal.accent)
}

// ── Results panel helpers ────────────────────────────────────────────────────

/// Caption / fine-print font size for table headers and small annotations.
pub(crate) const SZ_CAPTION: u32 = 11;

/// Hero font size for the governing-rate readout.
pub(crate) const SZ_HERO: u32 = 22;

/// Results panel for the empty (no input) state.
pub(crate) fn results_empty(pal: &'static Palette) -> Element<'static, Message> {
    column![
        section_heading(pal, "Results"),
        text("Enter design parameters to see results.")
            .size(SZ_BODY)
            .color(pal.muted),
    ]
    .spacing(SP_MD)
    .into()
}

/// Results panel for the error (solve failed) state.
pub(crate) fn results_error(pal: &'static Palette, msg: String) -> Element<'static, Message> {
    column![
        section_heading(pal, "Results"),
        text(msg).size(SZ_LABEL).color(pal.danger),
    ]
    .spacing(SP_MD)
    .into()
}

/// A muted label + mono value row with an explicit value color.
fn result_row_colored<'a>(
    pal: &'static Palette,
    label: impl Into<String>,
    value: impl Into<String>,
    unit: impl Into<String>,
    value_color: Color,
) -> Element<'a, Message> {
    let value = value.into();
    let unit = unit.into();
    let display = if unit.is_empty() {
        value
    } else {
        format!("{value} {unit}")
    };
    row![
        text(label.into())
            .size(SZ_LABEL)
            .color(pal.muted)
            .width(Length::FillPortion(2)),
        text(display)
            .font(Font::MONOSPACE)
            .size(SZ_BODY)
            .color(value_color)
            .width(Length::FillPortion(3)),
    ]
    .spacing(SP_SM)
    .into()
}

/// Maps the presenter's emphasis to its rendered color: `Normal` is plain
/// text, `Danger` is the danger color. Shared by every load-table row across
/// all five families (each used to carry its own copy of this match).
pub(crate) fn emphasis_color(pal: &'static Palette, e: Emphasis) -> Color {
    match e {
        Emphasis::Normal => pal.text,
        Emphasis::Danger => pal.danger,
    }
}

/// Render one result row, mapping the presenter's emphasis to a color.
pub(crate) fn render_result_row(pal: &'static Palette, r: &ResultRow) -> Element<'static, Message> {
    result_row_colored(
        pal,
        r.label.clone(),
        r.value.clone(),
        r.unit.clone(),
        emphasis_color(pal, r.emphasis),
    )
}

/// A heading followed by result rows (spacing 6), as used by every readout section.
pub(crate) fn rows_section(
    pal: &'static Palette,
    heading: &str,
    rows: &[ResultRow],
) -> iced::widget::Column<'static, Message> {
    let mut col = column![section_heading(pal, heading)].spacing(SP_ROW);
    for r in rows {
        col = col.push(render_result_row(pal, r));
    }
    col
}

/// A divider, a heading, then result rows (spacing 6) — the fatigue/min-weight
/// section shape. Built flat (not by wrapping `rows_section`) so the
/// divider→heading gap stays at the section's own spacing of 6.
pub(crate) fn divided_result_section(
    pal: &'static Palette,
    heading: &str,
    rows: &[ResultRow],
) -> Element<'static, Message> {
    let mut col = column![section_divider(pal), section_heading(pal, heading)].spacing(SP_ROW);
    for r in rows {
        col = col.push(render_result_row(pal, r));
    }
    col.into()
}

/// A divider followed by one muted `SZ_LABEL` line of prose — the shared
/// shape of compression's and torsion's fatigue `Note` arm and conical's
/// always-present linear-model disclosure footer.
pub(crate) fn divided_note(pal: &'static Palette, msg: &str) -> Element<'static, Message> {
    column![
        section_divider(pal),
        text(msg.to_string()).size(SZ_LABEL).color(pal.muted)
    ]
    .spacing(SP_SM)
    .into()
}

/// The hero spring-rate readout widget.
pub(crate) fn render_governing_rate(
    pal: &'static Palette,
    label: &str,
    gr: &GoverningRate,
) -> Element<'static, Message> {
    let rate_label = text(label.to_owned()).size(SZ_LABEL).color(pal.muted);
    let rate_value = mono_value(format!("{} {}", gr.value, gr.unit), pal.accent, SZ_HERO);
    column![rate_label, rate_value].spacing(SP_ROW).into()
}

/// Style for a `segmented` option button: highlighted when selected, muted
/// otherwise. Shared by every one-of-N chooser in the app (chart/3D toggle,
/// units, hook mode, settings' correction picker) so the highlight look is
/// defined exactly once.
pub(crate) fn segmented_style(
    pal: &'static Palette,
    selected: bool,
) -> impl Fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    move |_theme, status| {
        let is_hovered = matches!(status, iced::widget::button::Status::Hovered);
        let bg = if selected {
            pal.accent_tint
        } else if is_hovered {
            pal.hover
        } else {
            Color::TRANSPARENT
        };
        iced::widget::button::Style {
            background: Some(Background::Color(bg)),
            text_color: if selected { pal.accent } else { pal.text },
            border: Border {
                color: if selected { pal.accent } else { pal.line },
                width: 1.0,
                radius: 4.0.into(),
            },
            shadow: Default::default(),
            snap: Default::default(),
        }
    }
}

/// One-of-N chooser rendered as a row of styled buttons (labels are real
/// `text()` children, so the Simulator can find and click them — iced's
/// built-in single-select widget draws its label directly with no child
/// `Text` widget, so it never feeds a `Candidate::Text` and is structurally
/// invisible to `Simulator::find`).
///
/// The already-selected option gets no `.on_press`: clicking it would still
/// dispatch the same-valued message, and every consumer's handler
/// unconditionally returns `true` from `App::update`, which triggers
/// `recompute()` — clearing any pending `action_error` though nothing
/// actually changed (the documented no-op invariant at app.rs:621-622). A
/// button without `.on_press` renders with `Status::Disabled`, but
/// `segmented_style` only branches on `Hovered`, so the selected option's
/// look is unaffected.
pub(crate) fn segmented<'a, T: PartialEq + Copy + 'a>(
    pal: &'static Palette,
    options: &[(&'static str, T)],
    selected: T,
    on_pick: impl Fn(T) -> Message + 'a,
) -> Element<'a, Message> {
    let mut r = row![].spacing(SP_XS);
    for (label, value) in options {
        let is_selected = *value == selected;
        let mut btn = button(text(*label).size(SZ_LABEL))
            .style(segmented_style(pal, is_selected))
            .padding([SP_XS, SP_MD]);
        if !is_selected {
            btn = btn.on_press(on_pick(*value));
        }
        r = r.push(btn);
    }
    r.into()
}

/// The results panel's shared chart/3D toggle: identical in every family
/// (compression, conical, extension, torsion, assembly) — only the visual it
/// switches between differs, so it collapses to one call rather than five
/// byte-identical copies. `selected` is `app.results_visual`.
pub(crate) fn visual_toggle(
    pal: &'static Palette,
    selected: VisualMode,
) -> Element<'static, Message> {
    segmented(
        pal,
        &[
            ("Chart", VisualMode::Chart),
            ("3D", VisualMode::Spring3d),
            ("2D", VisualMode::Diagram),
        ],
        selected,
        Message::Visual,
    )
}

/// The results panel's shared visual slot: chart, orbitable 3D scene, or 2D
/// engineering diagram, selected by `app.results_visual` — identical dispatch
/// in every family (compression, conical, extension, torsion, assembly),
/// collapsing five byte-identical `match` arms into one call (simplifier F1).
/// `chart`/`wire3d`/`sdf3d`/`diagram` are `FnOnce` so only the geometry
/// actually needed is BUILT per render — laziness preserved: an eagerly-built
/// chart or scene would be thrown away every frame a DIFFERENT visual is
/// active (orbit drags re-render every frame while the shaded/wireframe path
/// is showing).
///
/// The wireframe and SDF scenes are separate closures because `sdf3d` is
/// invoked ONLY when a GPU adapter is present (`app.shader_available`): on a
/// GPU-less machine the shaded path is unreachable, so building the SDF scene
/// would allocate geometry `spring3d_element` immediately discards (Copilot
/// perf note). An empty [`crate::viz::sdf::SdfScene`] is passed instead.
pub(crate) fn results_visual_element<'a>(
    pal: &'static Palette,
    app: &App,
    chart: impl FnOnce() -> Element<'a, Message>,
    wire3d: impl FnOnce() -> crate::viz::SceneData,
    sdf3d: impl FnOnce() -> crate::viz::sdf::SdfScene,
    diagram: impl FnOnce() -> crate::diagram::DiagramInput,
) -> Element<'a, Message> {
    match app.results_visual {
        VisualMode::Chart => chart(),
        VisualMode::Spring3d => {
            let scene = wire3d();
            let sdf_scene = if app.shader_available {
                sdf3d()
            } else {
                crate::viz::sdf::SdfScene::default()
            };
            crate::viz::spring3d_element(
                pal,
                scene,
                sdf_scene,
                app.orbit,
                app.zoom,
                app.shader_available,
            )
        }
        VisualMode::Diagram => {
            crate::diagram::diagram_element(pal, diagram(), app.diagram_view, app.diagram_layers)
        }
    }
}

/// The 2D-diagram layer-toggle row, gated to Diagram mode — `None` in
/// Chart/Spring3d so every family can push it unconditionally without
/// repeating the `app.results_visual == VisualMode::Diagram` gate.
/// `app.diagram_layers` is a single global `App` field (not reset on family
/// switch), so this must be reachable from all five families — otherwise a
/// layer hidden on one family's diagram (e.g. Coils, which carries the
/// torsion inset's leg-angle dim) stays hidden with no affordance to restore
/// it after switching to a family whose view never rendered the toggle.
pub(crate) fn diagram_layer_controls(
    pal: &'static Palette,
    app: &App,
) -> Option<Element<'static, Message>> {
    (app.results_visual == VisualMode::Diagram)
        .then(|| diagram_layer_toggle(pal, app.diagram_layers))
}

/// The 2D-diagram layer toggles (lengths / diameters / coils). Rendered above
/// the canvas in Diagram mode only. Each button flips exactly its own group —
/// unlike `segmented`'s single-select `on_press` omission on the selected
/// option, every chip here always carries `on_press` because toggling the
/// already-on layer off is the whole point of a multi-select toggle group.
pub(crate) fn diagram_layer_toggle(
    pal: &'static Palette,
    layers: crate::diagram::DimLayers,
) -> Element<'static, Message> {
    use crate::diagram::DimLayer;
    let chip = |label: &'static str, on: bool, layer: DimLayer| {
        button(text(label).size(SZ_LABEL))
            .style(segmented_style(pal, on))
            .padding([SP_XS, SP_MD])
            .on_press(Message::DiagramLayer(layer))
    };
    row![
        chip("Lengths", layers.lengths, DimLayer::Lengths),
        chip("Diameters", layers.diameters, DimLayer::Diameters),
        chip("Coils", layers.coils, DimLayer::Coils),
    ]
    .spacing(SP_XS)
    .into()
}

// ── Labeled input ────────────────────────────────────────────────────────────

/// A labeled text input: muted label above a styled monospace input. The caller
/// supplies the widget id and the message constructor, so all families reuse this.
/// `id` accepts any type that converts to [`iced::widget::Id`]: `&'static str`,
/// `String`, or a pre-built `Id`. Existing `&'static str` callers are unaffected.
pub(crate) fn labeled_input<'a>(
    pal: &'static Palette,
    label: &str,
    value: &str,
    id: impl Into<iced::widget::Id>,
    on_input: impl Fn(String) -> Message + 'a,
) -> Element<'a, Message> {
    column![
        field_label(pal, label),
        text_input("", value)
            .id(id.into())
            .on_input(on_input)
            .size(SZ_BODY)
            .font(Font::MONOSPACE)
            .style(text_input_style(pal)),
    ]
    .spacing(SP_XS)
    .into()
}

/// Material picker scoped to a single assembly member.
///
/// Renders a `field_label("Material")` + pick-list for member at `index`.
/// The selected value is taken from `app.assembly.members[index].material`.
pub(crate) fn material_picker_for_member(app: &App, index: usize) -> Element<'_, Message> {
    let pal = app.pal();
    let names: Vec<String> = app
        .materials
        .names()
        .into_iter()
        .map(String::from)
        .collect();
    let selected = app.assembly.members.get(index).map(|m| m.material.clone());
    column![
        field_label(pal, "Material"),
        styled_pick_list(pal, names, selected, move |m| Message::AsmMemberMaterial(
            index, m
        )),
    ]
    .spacing(SP_XS)
    .into()
}
