use crate::preference_panels::sections::{
    SETTINGS_TARGET_FILES_SAVE_ACTIONS, SETTINGS_TARGET_FILES_SAVE_CLEANUP, SettingsHighlightState,
    settings_target_heading,
};
use eframe::egui;
use kuroya_core::{
    EditorAutoSaveMode, EditorSettings, MAX_AUTOSAVE_DELAY_MS, MIN_AUTOSAVE_DELAY_MS,
};

pub(super) fn render_files_settings_with_highlight(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    settings_target_heading(
        ui,
        highlight,
        SETTINGS_TARGET_FILES_SAVE_ACTIONS,
        "Save Actions",
    );
    egui::Grid::new("settings_files_save_actions_grid")
        .num_columns(2)
        .spacing([18.0, 10.0])
        .show(ui, |ui| {
            ui.label("Format on save");
            ui.checkbox(&mut draft.format_on_save, "Format before writing files");
            ui.end_row();

            ui.label("Autosave");
            autosave_mode_combo(ui, "settings_files_autosave_mode", draft);
            ui.end_row();

            let autosave_after_delay =
                draft.effective_autosave_mode() == EditorAutoSaveMode::AfterDelay;
            ui.add_enabled_ui(autosave_after_delay, |ui| {
                ui.label("Autosave delay");
            });
            ui.add_enabled_ui(autosave_after_delay, |ui| {
                ui.add(
                    egui::DragValue::new(&mut draft.autosave_delay_ms)
                        .speed(250.0)
                        .suffix(" ms")
                        .range(MIN_AUTOSAVE_DELAY_MS..=MAX_AUTOSAVE_DELAY_MS),
                );
            });
            ui.end_row();
        });

    ui.add_space(12.0);
    settings_target_heading(
        ui,
        highlight,
        SETTINGS_TARGET_FILES_SAVE_CLEANUP,
        "Save Cleanup",
    );
    egui::Grid::new("settings_files_save_cleanup_grid")
        .num_columns(2)
        .spacing([18.0, 10.0])
        .show(ui, |ui| {
            ui.label("Trim trailing whitespace");
            ui.checkbox(
                &mut draft.trim_trailing_whitespace,
                "Remove trailing spaces and tabs on save",
            );
            ui.end_row();

            ui.label("Insert final newline");
            ui.checkbox(&mut draft.insert_final_newline, "End files with a newline");
            ui.end_row();

            ui.label("Trim final newlines");
            ui.checkbox(
                &mut draft.trim_final_newlines,
                "Keep one final newline at most",
            );
            ui.end_row();
        });
}

fn autosave_mode_combo(ui: &mut egui::Ui, id: &'static str, draft: &mut EditorSettings) {
    let mut mode = draft.effective_autosave_mode();
    egui::ComboBox::from_id_salt(id)
        .selected_text(autosave_mode_label(mode))
        .show_ui(ui, |ui| {
            for candidate in [
                EditorAutoSaveMode::Off,
                EditorAutoSaveMode::AfterDelay,
                EditorAutoSaveMode::OnFocusChange,
                EditorAutoSaveMode::OnWindowChange,
            ] {
                ui.selectable_value(&mut mode, candidate, autosave_mode_label(candidate));
            }
        });

    draft.autosave = mode != EditorAutoSaveMode::Off;
    draft.autosave_mode = mode;
}

fn autosave_mode_label(mode: EditorAutoSaveMode) -> &'static str {
    match mode {
        EditorAutoSaveMode::Off => "Off",
        EditorAutoSaveMode::AfterDelay => "After delay",
        EditorAutoSaveMode::OnFocusChange => "On focus change",
        EditorAutoSaveMode::OnWindowChange => "On window change",
    }
}

#[cfg(test)]
mod tests {
    use super::autosave_mode_label;
    use kuroya_core::EditorAutoSaveMode;

    #[test]
    fn autosave_mode_label_names_modes() {
        assert_eq!(autosave_mode_label(EditorAutoSaveMode::Off), "Off");
        assert_eq!(
            autosave_mode_label(EditorAutoSaveMode::AfterDelay),
            "After delay"
        );
    }
}
