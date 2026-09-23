use crate::{
    KuroyaApp,
    popup_buttons::{PopupButtonKind, popup_button, popup_button_enabled},
    save_lifecycle::{dirty_buffer_ids, dirty_buffer_save_block_reason},
    ui_text::count_label,
};
use eframe::egui::{self, Align, Context, Key, RichText};

pub(super) fn render_exit_confirm_guard(app: &mut KuroyaApp, ctx: &Context) {
    let dirty = dirty_buffer_ids(&app.buffers);
    let terminal_count = app.terminal_exit_confirmation_count();
    if dirty.is_empty() && terminal_count == 0 {
        app.exit_confirmed = true;
        app.pending_exit = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        return;
    }

    let changed_on_disk = app.observed_external_change_buffer_ids();
    let save_block = (!dirty.is_empty())
        .then(|| {
            dirty_buffer_save_block_reason(
                &dirty,
                &app.buffers,
                &changed_on_disk,
                &app.lossy_decoded_buffers,
                &app.binary_preview_buffers,
                "exiting",
            )
        })
        .flatten();
    let can_save = save_block.is_none();
    let mut save = false;
    let mut exit = false;
    let mut cancel = false;
    let restart = app.pending_update_install.is_some();
    let exit_label = exit_discard_button_label(dirty.len(), terminal_count, restart);

    egui::Window::new(exit_guard_window_title(restart))
        .max_size(crate::layout::popup_window_max_size_with_top_margin(
            ctx, 24.0,
        ))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .fixed_size([540.0, 188.0])
        .show(ctx, |ui| {
            ui.label(RichText::new(exit_confirmation_title(dirty.len(), terminal_count)).strong());
            if !dirty.is_empty() {
                ui.label(exit_unsaved_changes_summary(dirty.len()));
            }
            if terminal_count > 0 {
                ui.label(exit_terminal_summary(terminal_count));
            }
            if let Some(reason) = &save_block {
                ui.label(
                    RichText::new(reason)
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
                if popup_button(ui, exit_label, PopupButtonKind::Danger).clicked() {
                    exit = true;
                }
                if popup_button_enabled(
                    ui,
                    !dirty.is_empty() && can_save,
                    exit_save_button_label(dirty.len(), restart),
                    PopupButtonKind::Primary,
                )
                .clicked()
                {
                    save = true;
                }
            });
        });

    if cancel {
        app.pending_exit = None;
        app.status = exit_canceled_status(restart);
    } else if exit {
        app.exit_confirmed = true;
        app.pending_exit = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    } else if save {
        app.start_pending_exit_save();
    }
}

fn exit_guard_window_title(restart: bool) -> &'static str {
    if restart {
        "Restart Kuroya"
    } else {
        "Exit Kuroya"
    }
}

fn exit_confirmation_title(dirty_count: usize, terminal_count: usize) -> &'static str {
    match (dirty_count, terminal_count) {
        (0, 1) => "Active terminal",
        (0, _) => "Active terminals",
        (_, 0) => "Unsaved changes",
        (_, 1) => "Unsaved changes and active terminal",
        (_, _) => "Unsaved changes and active terminals",
    }
}

fn exit_unsaved_changes_summary(dirty_count: usize) -> String {
    let files = count_label(dirty_count, "file", "files");
    let verb = if dirty_count == 1 { "has" } else { "have" };
    format!("{files} {verb} unsaved changes.")
}

fn exit_terminal_summary(terminal_count: usize) -> String {
    let terminals = count_label(terminal_count, "terminal session", "terminal sessions");
    let verb = if terminal_count == 1 { "is" } else { "are" };
    format!("{terminals} {verb} still active.")
}

fn exit_save_button_label(dirty_count: usize, restart: bool) -> &'static str {
    match (dirty_count == 1, restart) {
        (true, false) => "Save and Exit",
        (false, false) => "Save All and Exit",
        (true, true) => "Save and Restart",
        (false, true) => "Save All and Restart",
    }
}

fn exit_discard_button_label(
    dirty_count: usize,
    terminal_count: usize,
    restart: bool,
) -> &'static str {
    match (dirty_count, terminal_count, restart) {
        (0, 1, false) => "Exit and Close Terminal",
        (0, _, false) => "Exit and Close Terminals",
        (_, 0, false) => "Discard and Exit",
        (_, 1, false) => "Discard and Close Terminal",
        (_, _, false) => "Discard and Close Terminals",
        (0, 1, true) => "Restart and Close Terminal",
        (0, _, true) => "Restart and Close Terminals",
        (_, 0, true) => "Discard and Restart",
        (_, 1, true) => "Discard and Restart",
        (_, _, true) => "Discard and Restart",
    }
}

fn exit_canceled_status(restart: bool) -> String {
    if restart {
        "Update restart canceled".to_owned()
    } else {
        "Exit canceled".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        exit_canceled_status, exit_confirmation_title, exit_discard_button_label,
        exit_guard_window_title, exit_save_button_label, exit_terminal_summary,
        exit_unsaved_changes_summary,
    };

    #[test]
    fn exit_guard_copy_uses_singular_and_plural_labels() {
        assert_eq!(exit_confirmation_title(0, 1), "Active terminal");
        assert_eq!(
            exit_confirmation_title(2, 1),
            "Unsaved changes and active terminal"
        );
        assert_eq!(
            exit_unsaved_changes_summary(1),
            "1 file has unsaved changes."
        );
        assert_eq!(
            exit_unsaved_changes_summary(2),
            "2 files have unsaved changes."
        );
        assert_eq!(
            exit_terminal_summary(1),
            "1 terminal session is still active."
        );
        assert_eq!(
            exit_terminal_summary(2),
            "2 terminal sessions are still active."
        );
        assert_eq!(exit_guard_window_title(false), "Exit Kuroya");
        assert_eq!(exit_save_button_label(1, false), "Save and Exit");
        assert_eq!(exit_save_button_label(2, false), "Save All and Exit");
        assert_eq!(
            exit_discard_button_label(0, 1, false),
            "Exit and Close Terminal"
        );
        assert_eq!(
            exit_discard_button_label(0, 2, false),
            "Exit and Close Terminals"
        );
        assert_eq!(exit_discard_button_label(1, 0, false), "Discard and Exit");
        assert_eq!(
            exit_discard_button_label(1, 1, false),
            "Discard and Close Terminal"
        );
        assert_eq!(
            exit_discard_button_label(2, 2, false),
            "Discard and Close Terminals"
        );
        assert_eq!(exit_canceled_status(false), "Exit canceled");
    }

    #[test]
    fn restart_guard_copy_uses_restart_labels() {
        assert_eq!(exit_guard_window_title(true), "Restart Kuroya");
        assert_eq!(exit_save_button_label(1, true), "Save and Restart");
        assert_eq!(exit_save_button_label(2, true), "Save All and Restart");
        assert_eq!(
            exit_discard_button_label(0, 1, true),
            "Restart and Close Terminal"
        );
        assert_eq!(
            exit_discard_button_label(0, 2, true),
            "Restart and Close Terminals"
        );
        assert_eq!(exit_discard_button_label(1, 0, true), "Discard and Restart");
        assert_eq!(exit_discard_button_label(1, 1, true), "Discard and Restart");
        assert_eq!(exit_discard_button_label(2, 2, true), "Discard and Restart");
        assert_eq!(exit_canceled_status(true), "Update restart canceled");
    }
}
