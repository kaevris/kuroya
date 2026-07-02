use super::{
    SETTINGS_TARGET_GENERAL, SettingsHighlightState, guarded_f32_drag_value, settings_target_block,
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
    egui::Grid::new("settings_general_window_grid")
        .num_columns(2)
        .spacing([18.0, 10.0])
        .show(ui, |ui| {
            ui.label("Window zoom");
            guarded_f32_drag_value(
                ui,
                &mut draft.window_zoom_level,
                0.1,
                MIN_WINDOW_ZOOM_LEVEL..=MAX_WINDOW_ZOOM_LEVEL,
                DEFAULT_WINDOW_ZOOM_LEVEL,
            );
            ui.end_row();

            ui.label("UI font size");
            guarded_f32_drag_value(
                ui,
                &mut draft.ui_font_size,
                0.25,
                MIN_SETTINGS_PANEL_UI_FONT_SIZE..=MAX_SETTINGS_PANEL_UI_FONT_SIZE,
                DEFAULT_SETTINGS_PANEL_UI_FONT_SIZE,
            );
            ui.end_row();
        });

    ui.checkbox(&mut draft.minimap, "Minimap");
    ui.checkbox(&mut draft.smooth_scrolling, "Smooth scroll");
    ui.checkbox(
        &mut draft.scroll_beyond_last_line,
        "Scroll beyond last line",
    );
    ui.checkbox(&mut draft.status_bar_visible, "Status bar");
}
