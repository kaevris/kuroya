#[cfg(test)]
use crate::settings_form::optional_setting_path_from_input;
use kuroya_core::{EditorSettings, clamp_autosave_delay_ms, clamp_window_zoom_level};

mod editor;
mod terminal;

use editor::apply_editor_settings_draft;
use terminal::apply_terminal_settings_draft;

#[cfg(test)]
pub(super) fn apply_settings_panel_draft(
    settings: &mut EditorSettings,
    draft: &EditorSettings,
    editor_font_path: &str,
    ui_font_path: &str,
) {
    apply_settings_panel_draft_with_font_paths(
        settings,
        draft,
        optional_setting_path_from_input(editor_font_path),
        optional_setting_path_from_input(ui_font_path),
    );
}

pub(super) fn apply_settings_panel_draft_with_font_paths(
    settings: &mut EditorSettings,
    draft: &EditorSettings,
    editor_font_path: Option<String>,
    ui_font_path: Option<String>,
) {
    apply_editor_settings_draft(settings, draft);
    settings.autosave = draft.autosave;
    settings.autosave_mode = draft.autosave_mode;
    settings.autosave_delay_ms = clamp_autosave_delay_ms(draft.autosave_delay_ms);
    settings.status_bar_visible = draft.status_bar_visible;
    settings.devtools_verbose_logging = draft.devtools_verbose_logging;
    settings.devtools_profiling_enabled = draft.devtools_profiling_enabled;
    settings.plugins = draft.plugins.clone();
    settings.discord = draft.discord.clone();
    settings.window_zoom_level = clamp_window_zoom_level(draft.window_zoom_level);
    apply_terminal_settings_draft(settings, draft);
    settings.trim_trailing_whitespace = draft.trim_trailing_whitespace;
    settings.insert_final_newline = draft.insert_final_newline;
    settings.trim_final_newlines = draft.trim_final_newlines;
    settings.editor_font_path = editor_font_path;
    settings.ui_font_path = ui_font_path;
}

#[cfg(test)]
mod tests;
