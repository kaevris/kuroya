use super::capture::{handle_vim_key_capture, store_vim_key_capture_state, vim_key_capture_state};
use crate::preference_panels::sections::{
    SETTINGS_TARGET_VIM_KEYBINDINGS, SettingsHighlightState, settings_target_block,
};
use crate::ui_switch::ui_switch;
use eframe::egui;
use kuroya_core::EditorSettings;

mod builtin_rows;
mod custom_rows;
mod widgets;

pub(super) fn render_vim_settings(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    let mut capture_state = vim_key_capture_state(ui.ctx());
    if handle_vim_key_capture(ui.ctx(), &mut draft.vim, &mut capture_state) {
        capture_state.lock_controls_for_frame();
    }

    settings_target_block(ui, highlight, SETTINGS_TARGET_VIM_KEYBINDINGS, |ui| {
        ui.label(egui::RichText::new("Keybindings").strong());
        egui::Grid::new("settings_vim_keybindings_grid")
            .num_columns(2)
            .spacing([18.0, 10.0])
            .show(ui, |ui| {
                ui.label("Mode");
                ui_switch(ui, &mut draft.vim_keybindings);
                ui.end_row();

                ui.label("Custom bindings");
                custom_rows::render_custom_bindings(ui, &mut draft.vim, &mut capture_state);
                ui.end_row();

                ui.label("Built-in bindings");
                builtin_rows::render_builtin_bindings(ui, &mut draft.vim, &mut capture_state);
                ui.end_row();
            });
    });

    capture_state.clear_frame_controls_lock();
    store_vim_key_capture_state(ui.ctx(), capture_state);
}

#[cfg(test)]
pub(super) fn vim_capture_hint_text(
    state: &super::capture::VimKeyCaptureState,
    target: super::capture::VimKeyCaptureTarget,
) -> &'static str {
    widgets::vim_capture_hint_text(state, target)
}
