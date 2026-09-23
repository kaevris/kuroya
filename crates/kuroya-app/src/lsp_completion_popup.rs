mod content;
mod passthrough;

use crate::KuroyaApp;
use content::CompletionPopupAction;
use eframe::egui::{self, Context};

impl KuroyaApp {
    pub(crate) fn render_completion_popup(&mut self, ctx: &Context) {
        if self.apply_completion_passthrough_input(ctx) {
            return;
        }

        let mut action = None;

        egui::Window::new("Completions")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 140.0,
            ))
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 116.0])
            .default_size([720.0, 320.0])
            .show(ctx, |ui| {
                action = self.render_completion_popup_content(ui);
            });

        match action {
            Some(CompletionPopupAction::Close) => {
                self.clear_completion_popup_state();
                self.status = "Closed completions".to_owned();
            }
            Some(CompletionPopupAction::Apply { item, commit_text }) => {
                self.apply_completion_item_with_commit(*item, commit_text);
            }
            None => {}
        }
    }
}
