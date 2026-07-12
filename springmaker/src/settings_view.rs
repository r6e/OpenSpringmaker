//! Humble view for the Settings screen — renders SettingsViewModel only.
//! No logic or branching; all rendering decisions live in `settings_view_model`.

use iced::widget::{button, column, container, row, space, text};
use iced::{Element, Font, Length};

use crate::app::{App, Message, Screen};
use crate::settings_view_model::{SettingsFeedbackKind, SettingsViewModel};
use crate::widgets::{
    nav_button_style, panel_container, screen_shell, section_divider, section_heading,
    segmented_style, SP_LG, SP_MD, SP_SM, SZ_BODY, SZ_LABEL, SZ_TITLE,
};

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

    // Build correction-option buttons. Each option emits SetCorrection on press;
    // the presenter's `selected` flag drives visual differentiation via the
    // shared `segmented_style` (Task 4). Full-width rows (rather than the
    // shared `segmented` row widget) because option labels are long prose
    // ("Bergsträsser (EN 13906-1 / Shigley default)"), not short chips.
    let mut options_col = column![
        section_heading(pal, "Curvature-correction factor"),
        section_divider(pal),
    ]
    .spacing(SP_SM);

    // Extract save_feedback before consuming vm.options into option_data.
    let save_feedback = vm.save_feedback;

    // Collect into owned tuples so no reference to `vm.options` escapes into
    // the element tree (Element<'_> must not borrow from the local ViewModel).
    let option_data: Vec<(springcore::CurvatureCorrection, String, bool)> = vm
        .options
        .into_iter()
        .map(|o| (o.value, o.label, o.selected))
        .collect();

    for (value, label, selected) in option_data {
        let label_text = text(label).size(SZ_BODY);
        let mut btn = button(label_text)
            .style(segmented_style(pal, selected))
            .width(Length::Fill)
            .padding([SP_SM, SP_MD]);
        // The already-selected option normally gets no `.on_press` (same no-op
        // guard as `widgets::segmented`): re-clicking it would dispatch
        // `SetCorrection` with the same value for no visible reason. BUT
        // `SetCorrection` also performs a real file write (`AppSettings::save_to`),
        // and a FAILED write leaves the selected option as the one the user needs
        // to retry — with no `.on_press`, a failed save could never be retried
        // from this screen. So the no-op guard is relaxed specifically when a
        // save is currently failing, keeping the selected button live for a
        // one-click retry. Read via `save_feedback` (the ViewModel's own
        // rendering of `app.settings_error`) rather than `app` directly — this
        // is a humble view (ADR 0008): it may branch on presenter output, not
        // reach past it into model state it's meant to be insulated from.
        if !selected || save_feedback.is_some() {
            btn = btn.on_press(Message::SetCorrection(value));
        }
        options_col = options_col.push(btn);
    }

    let correction_panel: Element<'_, Message> = container(panel_container(pal, options_col))
        .width(Length::Fill)
        .into();

    let mut content = column![header, section_divider(pal), correction_panel].spacing(SP_LG);

    // Surface a settings-save error below the correction panel (spec §5).
    // The in-memory preference still applies regardless of this status.
    if let Some(fb) = save_feedback {
        let color = match fb.kind {
            SettingsFeedbackKind::Error => pal.danger,
        };
        content = content.push(text(fb.text).size(SZ_LABEL).color(color));
    }

    screen_shell(pal, content, true)
}
