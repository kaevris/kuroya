use super::{
    SETTINGS_TARGET_GENERAL, SettingsHighlightState, guarded_f32_drag_value, settings_control_row,
    settings_target_block, settings_toggle_row,
};
use eframe::egui;
use kuroya_core::{
    DEFAULT_WINDOW_ZOOM_LEVEL, EditorSettings, MAX_WINDOW_ZOOM_LEVEL, MIN_WINDOW_ZOOM_LEVEL,
};

const MIN_SETTINGS_PANEL_UI_FONT_SIZE: f32 = 10.0;
const MAX_SETTINGS_PANEL_UI_FONT_SIZE: f32 = 24.0;
const DEFAULT_SETTINGS_PANEL_UI_FONT_SIZE: f32 = 13.0;

pub(super) fn render_general_settings_with_highlight(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    settings_target_block(ui, highlight, SETTINGS_TARGET_GENERAL, |ui| {
        render_general_settings_content(ui, draft);
    });
}

fn render_general_settings_content(ui: &mut egui::Ui, draft: &mut EditorSettings) {
    settings_control_row(
        ui,
        "Window zoom",
        "Scale the application interface without changing editor text size.",
        |ui| {
            guarded_f32_drag_value(
                ui,
                &mut draft.window_zoom_level,
                0.1,
                MIN_WINDOW_ZOOM_LEVEL..=MAX_WINDOW_ZOOM_LEVEL,
                DEFAULT_WINDOW_ZOOM_LEVEL,
            );
        },
    );
    settings_control_row(
        ui,
        "UI font size",
        "Set the text size used by menus, panels, and controls.",
        |ui| {
            guarded_f32_drag_value(
                ui,
                &mut draft.ui_font_size,
                0.25,
                MIN_SETTINGS_PANEL_UI_FONT_SIZE..=MAX_SETTINGS_PANEL_UI_FONT_SIZE,
                DEFAULT_SETTINGS_PANEL_UI_FONT_SIZE,
            );
        },
    );
    settings_toggle_row(
        ui,
        "Minimap",
        "Show a compact overview of the current file in the editor.",
        &mut draft.minimap,
    );
    settings_toggle_row(
        ui,
        "Smooth scroll",
        "Animate editor and panel scrolling.",
        &mut draft.smooth_scrolling,
    );
    settings_toggle_row(
        ui,
        "Scroll beyond last line",
        "Allow the final line to move above the bottom edge of the editor.",
        &mut draft.scroll_beyond_last_line,
    );
    settings_toggle_row(
        ui,
        "Status bar",
        "Show workspace and editor status at the bottom of the window.",
        &mut draft.status_bar_visible,
    );
}
