use crate::{
    KuroyaApp, keybinding_input::capture_keybinding_input,
    keybindings_panel_actions::PendingKeybindingsPanelActions,
};
use eframe::egui::{self, Context};
use std::time::Instant;

mod actions_guard;
mod buttons;
mod cache;
mod controls;
mod item;
mod query;
mod rows;

use actions_guard::guard_keybindings_panel_actions;
use cache::cached_keybinding_items;
pub(in crate::keybindings_panel) use item::KeybindingPanelItem;
use query::sanitize_keybindings_query;

pub(in crate::keybindings_panel) const KEYBINDING_TEXT_MAX_CHARS: usize = 96;

impl KuroyaApp {
    pub(crate) fn render_keybindings_panel(&mut self, ctx: &Context) {
        self.finish_expired_keybinding_escape_capture(Instant::now());
        if let Some(remaining) = self.keybinding_escape_cancel_remaining(Instant::now()) {
            ctx.request_repaint_after(remaining);
        }

        if sanitize_keybindings_query(&mut self.keybindings_query) {
            self.keybindings_selected = 0;
        }
        let query = self.keybindings_query.trim();
        let items = cached_keybinding_items(ctx, &self.settings.keymap.bindings, query);
        crate::ui_state::clamp_selection(&mut self.keybindings_selected, items.len());
        let capturing = self.keybinding_capture_command.is_some();
        let mut actions = PendingKeybindingsPanelActions {
            captured: capturing.then(|| capture_keybinding_input(ctx)).flatten(),
            ..Default::default()
        };

        egui::Window::new("Keyboard Shortcuts")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 96.0,
            ))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 72.0])
            .fixed_size([620.0, 440.0])
            .show(ctx, |ui| {
                let selection_changed =
                    controls::render_keybinding_controls(self, ui, &items, capturing, &mut actions);
                buttons::render_keybinding_buttons(self, ui, &items, capturing, &mut actions);
                ui.separator();

                rows::render_keybinding_rows(
                    ui,
                    &items,
                    &self.keybindings_query,
                    &mut self.keybindings_selected,
                    capturing,
                    self.settings.ui_font_size,
                    selection_changed,
                    &mut actions,
                );
            });

        guard_keybindings_panel_actions(&mut actions, &items);
        self.apply_keybindings_panel_actions(actions);
    }
}

#[cfg(test)]
mod tests;
