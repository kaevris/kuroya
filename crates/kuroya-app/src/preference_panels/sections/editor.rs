use eframe::egui;
use kuroya_core::EditorSettings;

use super::SettingsHighlightState;

mod code_view;
mod cursor;
mod language;
mod text_layout;
mod typing;

pub(super) fn render_editor_settings(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    text_layout::render_text_layout_settings_with_highlight(ui, draft, highlight);
    typing::render_typing_settings_with_highlight(ui, draft, highlight);
    language::render_language_settings_with_highlight(ui, draft, highlight);
    cursor::render_cursor_settings_with_highlight(ui, draft, highlight);
    code_view::render_code_view_settings_with_highlight(ui, draft, highlight);
}

pub(super) fn render_source_control_settings(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    code_view::render_source_control_settings_with_highlight(ui, draft, highlight);
}
