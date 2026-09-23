mod results;

use crate::{
    KuroyaApp,
    app_update_overlays::PopupDismissalGuard,
    command_palette_items::{
        MAX_COMMAND_PALETTE_QUERY_MEMORY, MAX_COMMAND_PALETTE_RECENT_COMMANDS,
        record_command_palette_query_memory, record_recent_palette_command,
        sanitize_command_palette_query_input_in_place,
    },
};
use eframe::egui::{self, Context, TextEdit};
pub(crate) use results::CommandPaletteResultsCache;

impl KuroyaApp {
    pub(crate) fn render_command_palette(&mut self, ctx: &Context) {
        let mut command_to_run = None;
        let dismissal = PopupDismissalGuard::capture(ctx);

        let window_response = egui::Window::new("Command Palette")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 96.0,
            ))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 72.0])
            .fixed_size([620.0, 380.0])
            .show(ctx, |ui| {
                let response = ui.add(
                    TextEdit::singleline(&mut self.command_query)
                        .hint_text("Run command")
                        .desired_width(f32::INFINITY),
                );
                response.request_focus();
                let query_changed = response.changed();
                if query_changed {
                    sanitize_command_palette_query_input_in_place(&mut self.command_query);
                    self.command_selected = 0;
                }

                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    self.close_command_palette();
                    return;
                }

                command_to_run = self.render_command_palette_results(ui, query_changed);
            });

        if window_response
            .as_ref()
            .is_some_and(|response| dismissal.clicked_outside(ctx, response.response.rect))
        {
            self.close_command_palette();
            return;
        }

        if let Some(command) = command_to_run {
            record_command_palette_query_memory(
                &mut self.command_query_memory,
                &self.command_query,
                &command,
                MAX_COMMAND_PALETTE_QUERY_MEMORY,
            );
            record_recent_palette_command(
                &mut self.command_recent,
                command.clone(),
                MAX_COMMAND_PALETTE_RECENT_COMMANDS,
            );
            self.close_command_palette();
            self.command_bus.push(command);
        }
    }
}
