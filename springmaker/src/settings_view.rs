//! Humble view for the Settings screen — renders SettingsViewModel only.
//! No logic or branching; all rendering decisions live in `settings_view_model`.

use iced::widget::{button, column, container, row, space, text};
use iced::{Element, Font, Length};

use crate::app::{App, Message, Palette, Screen};
use crate::settings_view_model::{SettingsFeedbackKind, SettingsViewModel};
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

    // Extract save_feedback before consuming vm.options/theme_options.
    let save_feedback = vm.save_feedback;

    // Build correction-option buttons. Each option emits SetCorrection on
    // press; the presenter's `selected`/`clickable` flags drive visual
    // differentiation and click-handling via the shared `option_button`
    // (Task 4). Full-width rows (rather than the shared `segmented` row
    // widget) because option labels are long prose ("Bergsträsser (EN
    // 13906-1 / Shigley default)"), not short chips.
    let mut options_col = column![
        section_heading(pal, "Curvature-correction factor"),
        section_divider(pal),
    ]
    .spacing(SP_SM);
    for o in vm.options {
        options_col = options_col.push(option_button(
            pal,
            o.label,
            o.selected,
            o.clickable,
            Message::SetCorrection(o.value),
        ));
    }
    let correction_panel: Element<'_, Message> = container(panel_container(pal, options_col))
        .width(Length::Fill)
        .into();

    // Theme-preference picker (System/Light/Dark): same prose-button pattern
    // and the same shared `option_button` as the correction group above.
    let mut theme_col = column![section_heading(pal, "Theme"), section_divider(pal)].spacing(SP_SM);
    for o in vm.theme_options {
        theme_col = theme_col.push(option_button(
            pal,
            o.label,
            o.selected,
            o.clickable,
            Message::ThemePref(o.value),
        ));
    }
    let theme_panel: Element<'_, Message> = container(panel_container(pal, theme_col))
        .width(Length::Fill)
        .into();

    let mut content =
        column![header, section_divider(pal), correction_panel, theme_panel].spacing(SP_LG);

    // Surface a settings-save error below the panels (spec §5). The
    // in-memory preference still applies regardless of this status.
    if let Some(fb) = save_feedback {
        let color = match fb.kind {
            SettingsFeedbackKind::Error => pal.danger,
        };
        content = content.push(text(fb.text).size(SZ_LABEL).color(color));
    }

    screen_shell(pal, content, true)
}
