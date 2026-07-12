//! Humble view for the Settings screen — renders SettingsViewModel only.
//! All BEHAVIORAL decisions (option lists, selection, clickability) live in
//! `settings_view_model`; like every humble view in this app (ADR 0008, the
//! `Emphasis`→color precedent), mapping a semantic kind to a palette color at
//! render time is the view's job — the ViewModel stays iced-free.

use iced::widget::{button, column, container, row, space, text};
use iced::{Element, Font, Length};

use crate::app::{App, Message, Palette, Screen};
use crate::settings_view_model::{SettingOption, SettingsFeedbackKind, SettingsViewModel};
use crate::widgets::{
    nav_button_style, panel_container, screen_shell, section_divider, section_heading,
    segmented_style, SP_LG, SP_MD, SP_SM, SZ_BODY, SZ_LABEL, SZ_TITLE,
};

/// Render one option button: labels are real `text()` children (so the
/// Simulator can find/click them), styled by the shared `segmented_style`,
/// and given `.on_press` only when the ViewModel marks the option
/// `clickable`. Shared by the correction group and the theme group — the two
/// are structurally identical prose-button rows, differing only in the
/// emitted `Message` (Task 4).
fn option_button<'a>(
    pal: &'static Palette,
    label: String,
    selected: bool,
    clickable: bool,
    msg: Message,
) -> Element<'a, Message> {
    let label_text = text(label).size(SZ_BODY);
    let mut btn = button(label_text)
        .style(segmented_style(pal, selected))
        .width(Length::Fill)
        .padding([SP_SM, SP_MD]);
    if clickable {
        btn = btn.on_press(msg);
    }
    btn.into()
}

/// Build one settings panel: a heading, a divider, and one `option_button`
/// row per option, each emitting `to_msg(option.value)` on press. Generic
/// over the option's value type so the correction group and the theme group
/// — structurally identical prose-button panels differing only in which
/// `Message` variant a click emits — share this single builder (Task 4/panel
/// item 5). `to_msg` is a tuple-variant constructor (`Message::SetCorrection`,
/// `Message::ThemePref`) coerced to a plain `fn` pointer.
fn option_panel<'a, T: Copy>(
    pal: &'static Palette,
    heading: &str,
    options: Vec<SettingOption<T>>,
    to_msg: fn(T) -> Message,
) -> Element<'a, Message> {
    let mut col = column![section_heading(pal, heading), section_divider(pal)].spacing(SP_SM);
    for o in options {
        col = col.push(option_button(
            pal,
            o.label,
            o.selected,
            o.clickable,
            to_msg(o.value),
        ));
    }
    container(panel_container(pal, col))
        .width(Length::Fill)
        .into()
}

/// Build the Settings screen.
pub(crate) fn view(app: &App) -> Element<'_, Message> {
    let pal = app.pal();
    let vm = SettingsViewModel::from_app(app);

    let back_btn = button(text("\u{2190} Calculator").size(SZ_LABEL).color(pal.accent))
        .on_press(Message::NavigateTo(Screen::Calculator))
        .style(nav_button_style(pal));

    let title = text("Settings").size(SZ_TITLE).color(pal.text).font(Font {
        weight: iced::font::Weight::Semibold,
        ..Font::DEFAULT
    });

    let header = row![title, space().width(Length::Fill), back_btn]
        .spacing(SP_LG)
        .align_y(iced::Alignment::Center);

    // Correction-option panel: each option emits SetCorrection on press; the
    // presenter's `selected`/`clickable` flags drive visual differentiation
    // and click-handling via the shared `option_button`/`option_panel`
    // (Task 4/panel item 5). Full-width rows (rather than the shared
    // `segmented` row widget) because option labels are long prose
    // ("Bergsträsser (EN 13906-1 / Shigley default)"), not short chips.
    let correction_panel = option_panel(
        pal,
        "Curvature-correction factor",
        vm.options,
        Message::SetCorrection,
    );

    // Theme-preference picker (System/Light/Dark): same prose-button pattern
    // and the same shared `option_panel` as the correction group above.
    let theme_panel = option_panel(pal, "Theme", vm.theme_options, Message::ThemePref);

    let mut content =
        column![header, section_divider(pal), correction_panel, theme_panel].spacing(SP_LG);

    // Surface a settings-save error below the panels (spec §5). The
    // in-memory preference still applies regardless of this status.
    // `vm.save_feedback` is a disjoint field from `vm.options`/`vm.theme_options`
    // above, so this partial move needs no pre-extraction.
    if let Some(fb) = vm.save_feedback {
        let color = match fb.kind {
            SettingsFeedbackKind::Error => pal.danger,
        };
        content = content.push(text(fb.text).size(SZ_LABEL).color(color));
    }

    screen_shell(pal, content, true)
}
