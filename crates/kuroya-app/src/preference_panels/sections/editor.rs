use eframe::egui;
use kuroya_core::EditorSettings;

use super::SettingsHighlightState;

mod code_view;
mod cursor;
mod language;
mod text_layout;
mod typing;

type EditorSettingsRenderStep<'a> = (
    &'a str,
    &'a dyn Fn(&mut egui::Ui, &mut EditorSettings, &mut SettingsHighlightState),
);

pub(super) fn render_editor_settings(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    #[cfg(debug_assertions)]
    let max_sub: usize = std::env::var("KUROYA_DEBUG_SUB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    #[cfg(not(debug_assertions))]
    let max_sub = 5usize;

    let steps: [EditorSettingsRenderStep; 5] = [
        ("text_layout", &|ui, d, h| {
            text_layout::render_text_layout_settings_with_highlight(ui, d, h)
        }),
        ("typing", &|ui, d, h| {
            typing::render_typing_settings_with_highlight(ui, d, h)
        }),
        ("language", &|ui, d, h| {
            language::render_language_settings_with_highlight(ui, d, h)
        }),
        ("cursor", &|ui, d, h| {
            cursor::render_cursor_settings_with_highlight(ui, d, h)
        }),
        ("code_view", &|ui, d, h| {
            code_view::render_code_view_settings_with_highlight(ui, d, h)
        }),
    ];
    for (index, (name, call)) in steps.iter().enumerate() {
        if index >= max_sub {
            break;
        }
        #[cfg(debug_assertions)]
        if std::env::var("KUROYA_DEBUG_SETTINGS").is_ok() {
            use std::fmt::Write as _;
            let mut line = String::new();
            let _ = writeln!(line, "SUB {},{}", name, index);
        }
        call(ui, draft, highlight);
    }
}

pub(super) fn render_source_control_settings(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    code_view::render_source_control_settings_with_highlight(ui, draft, highlight);
}
