#[cfg(test)]
use crate::path_display::{DISPLAY_PATH_LABEL_MAX_CHARS, sanitized_display_label};
use crate::{
    KuroyaApp,
    popup_buttons::{PopupButtonKind, popup_button, popup_button_enabled},
    save_lifecycle::protected_preview_save_block_reason_for_buffer,
};
use eframe::egui::{self, Align, Context, Key, RichText};

impl KuroyaApp {
    pub(crate) fn render_unsaved_close(&mut self, ctx: &Context) {
        let Some(id) = self.dirty_close_buffer else {
            return;
        };
        let Some(buffer) = self.buffer(id) else {
            self.dirty_close_buffer = None;
            self.begin_next_pending_close();
            return;
        };

        let label = self.buffer_label_for(buffer);
        let save_block = protected_preview_save_block_reason_for_buffer(
            buffer,
            &self.lossy_decoded_buffers,
            &self.binary_preview_buffers,
        );
        let mut save = false;
        let mut discard = false;
        let mut cancel = false;
        let mut window_open = true;

        egui::Window::new("Unsaved Changes")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 24.0,
            ))
            .open(&mut window_open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([500.0, 156.0])
            .show(ctx, |ui| {
                ui.label(RichText::new(label).strong());
                ui.label("Save changes before closing?");
                if let Some(reason) = save_block {
                    ui.label(
                        RichText::new(close_guard_save_block_message(reason))
                            .small()
                            .color(ui.visuals().warn_fg_color),
                    );
                }

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    cancel = true;
                }

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if popup_button(ui, "Cancel", PopupButtonKind::Secondary).clicked() {
                        cancel = true;
                    }
                    if popup_button(
                        ui,
                        close_guard_discard_button_label(),
                        PopupButtonKind::Danger,
                    )
                    .clicked()
                    {
                        discard = true;
                    }
                    if popup_button_enabled(
                        ui,
                        save_block.is_none(),
                        close_guard_save_button_label(),
                        PopupButtonKind::Primary,
                    )
                    .clicked()
                    {
                        save = true;
                    }
                });
            });

        if cancel || !window_open {
            self.dirty_close_buffer = None;
            self.pending_close_buffers.clear();
            self.status = "Close canceled".to_owned();
        } else if discard {
            self.dirty_close_buffer = None;
            self.clear_deferred_save_work(id);
            self.force_close_buffer(id);
            self.begin_next_pending_close();
        } else if save {
            self.dirty_close_buffer = None;
            self.close_after_save = Some(id);
            self.spawn_save(id);
        }
    }
}

#[cfg(test)]
fn close_guard_display_label(label: &str) -> String {
    sanitized_display_label(label, DISPLAY_PATH_LABEL_MAX_CHARS, "Untitled")
}

fn close_guard_save_button_label() -> &'static str {
    "Save and Close"
}

fn close_guard_discard_button_label() -> &'static str {
    "Discard and Close"
}

fn close_guard_save_block_message(reason: &str) -> String {
    format!("Cannot save before closing; {reason}.")
}

#[cfg(test)]
mod tests {
    use super::{
        DISPLAY_PATH_LABEL_MAX_CHARS, close_guard_discard_button_label, close_guard_display_label,
        close_guard_save_block_message, close_guard_save_button_label,
    };

    #[test]
    fn close_guard_display_label_sanitizes_controls_bidi_and_bounds_length() {
        let raw = format!(
            "alpha\n{}\u{202e}omega.rs",
            "very-long-component-".repeat(16)
        );

        let label = close_guard_display_label(&raw);

        assert!(label.starts_with("alpha "));
        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));
        assert!(label.contains("..."));
        assert!(label.chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS);
    }

    #[test]
    fn close_guard_display_label_falls_back_for_blank_control_text() {
        assert_eq!(close_guard_display_label("\n\u{202e}\u{0007}"), "Untitled");
    }

    #[test]
    fn close_guard_copy_uses_close_action_labels_and_save_block_message() {
        assert_eq!(close_guard_save_button_label(), "Save and Close");
        assert_eq!(close_guard_discard_button_label(), "Discard and Close");
        assert_eq!(
            close_guard_save_block_message("buffer is read-only"),
            "Cannot save before closing; buffer is read-only."
        );
    }
}
