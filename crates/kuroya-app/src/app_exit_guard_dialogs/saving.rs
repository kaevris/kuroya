use crate::{
    KuroyaApp,
    popup_buttons::{PopupButtonKind, popup_button},
    save_lifecycle::has_active_save_work,
    transient_state::PendingExit,
    ui_text::count_label,
};
use eframe::egui::{self, Align, Context, Key, RichText};
use kuroya_core::{BufferId, TextBuffer};

pub(super) fn render_exit_saving_guard(app: &mut KuroyaApp, ctx: &Context) {
    app.advance_pending_exit_after_save();
    let remaining = match app.pending_exit.as_ref() {
        Some(PendingExit::Saving { ids }) => pending_exit_remaining_saves(app, ids),
        _ => {
            if app.exit_confirmed {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            return;
        }
    };
    let mut cancel = false;
    let restart = app.pending_update_install.is_some();

    egui::Window::new(exit_saving_window_title(restart))
        .max_size(crate::layout::popup_window_max_size_with_top_margin(
            ctx, 24.0,
        ))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .fixed_size([520.0, 132.0])
        .show(ctx, |ui| {
            ui.label(RichText::new(exit_saving_title(restart)).strong());
            ui.label(exit_saving_body(remaining));

            if ui.input(|input| input.key_pressed(Key::Escape)) {
                cancel = true;
            }

            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if popup_button(ui, "Cancel", PopupButtonKind::Secondary).clicked() {
                    cancel = true;
                }
            });
        });

    if cancel {
        app.pending_exit = None;
        app.status = exit_saving_canceled_status(restart);
    }
}

fn exit_saving_window_title(restart: bool) -> &'static str {
    if restart {
        "Restart Kuroya"
    } else {
        "Exit Kuroya"
    }
}

fn exit_saving_title(restart: bool) -> &'static str {
    if restart {
        "Saving before restart"
    } else {
        "Saving before exit"
    }
}

fn exit_saving_canceled_status(restart: bool) -> String {
    if restart {
        "Update restart canceled; in-flight saves will still finish".to_owned()
    } else {
        "Exit canceled; in-flight saves will still finish".to_owned()
    }
}

fn pending_exit_remaining_saves(app: &KuroyaApp, ids: &[BufferId]) -> usize {
    ids.iter()
        .filter(|id| {
            has_active_save_work(
                **id,
                &app.in_flight_saves,
                &app.queued_save_paths,
                &app.pending_format_on_save,
            ) || app.buffer(**id).is_some_and(TextBuffer::is_dirty)
        })
        .count()
}

fn exit_saving_body(remaining: usize) -> String {
    format!("Saving {}.", count_label(remaining, "file", "files"))
}

#[cfg(test)]
mod tests {
    use super::{
        exit_saving_body, exit_saving_canceled_status, exit_saving_title, exit_saving_window_title,
    };

    #[test]
    fn exit_saving_body_uses_file_count_labels() {
        assert_eq!(exit_saving_body(1), "Saving 1 file.");
        assert_eq!(exit_saving_body(2), "Saving 2 files.");
    }

    #[test]
    fn exit_saving_copy_switches_for_restart() {
        assert_eq!(exit_saving_window_title(false), "Exit Kuroya");
        assert_eq!(exit_saving_window_title(true), "Restart Kuroya");
        assert_eq!(exit_saving_title(false), "Saving before exit");
        assert_eq!(exit_saving_title(true), "Saving before restart");
        assert_eq!(
            exit_saving_canceled_status(false),
            "Exit canceled; in-flight saves will still finish"
        );
        assert_eq!(
            exit_saving_canceled_status(true),
            "Update restart canceled; in-flight saves will still finish"
        );
    }
}
