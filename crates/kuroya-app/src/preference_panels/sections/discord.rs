use super::{
    SETTINGS_TARGET_DISCORD, SettingsHighlightState, bounded_settings_singleline_input,
    settings_control_row, settings_target_block, settings_toggle_row,
};
use eframe::egui;
use kuroya_core::EditorSettings;

pub(super) fn render_discord_settings(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    settings_target_block(ui, highlight, SETTINGS_TARGET_DISCORD, |ui| {
        ui.set_width(ui.available_width());
        settings_toggle_row(
            ui,
            "Discord Rich Presence",
            "Show what you are editing on your Discord profile. Off by default; requires your own Discord application id and a running Discord client.",
            &mut draft.discord.presence_enabled,
        );
        settings_control_row(
            ui,
            "Application ID",
            "The application id from discord.com/developers/applications.",
            |ui| {
                let mut value = bounded_settings_singleline_input(&draft.discord.client_id);
                let input_width = discord_id_input_width(ui.available_width());
                let response = ui.add_sized(
                    [input_width, ui.spacing().interact_size.y],
                    egui::TextEdit::singleline(&mut value).hint_text("e.g. 1234567890123456789"),
                );
                if response.changed() {
                    draft.discord.client_id = value;
                }
            },
        );
        settings_toggle_row(
            ui,
            "Show edited file name",
            "Include the file name in the presence. Hidden lines are never sent to Discord.",
            &mut draft.discord.show_details,
        );
        settings_toggle_row(
            ui,
            "Show workspace name",
            "Include the workspace folder name in the presence.",
            &mut draft.discord.show_workspace,
        );
        settings_toggle_row(
            ui,
            "Show elapsed time",
            "Show how long the current workspace has been open.",
            &mut draft.discord.show_elapsed,
        );
    });
}

fn discord_id_input_width(width: f32) -> f32 {
    width.clamp(96.0, 280.0)
}
