use crate::{
    KuroyaApp,
    popup_buttons::{PopupButtonKind, popup_button},
    workspace_guard_runtime::workspace_guard_display_path,
};
use eframe::egui::{self, Align, Context, Key, RichText};
use std::path::PathBuf;

pub(super) fn render_workspace_switch_home_directory_guard(
    app: &mut KuroyaApp,
    ctx: &Context,
    target: PathBuf,
) {
    let mut open = false;
    let mut cancel = false;

    egui::Window::new("Open Home Directory")
        .max_size(crate::layout::popup_window_max_size_with_top_margin(
            ctx, 24.0,
        ))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .fixed_size([520.0, 164.0])
        .show(ctx, |ui| {
            ui.label(
                RichText::new(format!("Open {}", workspace_guard_display_path(&target))).strong(),
            );
            ui.label(
                "Opening your home directory can make indexing and searching slow. Open anyway?",
            );

            if ui.input(|input| input.key_pressed(Key::Escape)) {
                cancel = true;
            }

            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if popup_button(ui, "Cancel", PopupButtonKind::Secondary).clicked() {
                    cancel = true;
                }
                if popup_button(ui, "Open", PopupButtonKind::Primary).clicked() {
                    open = true;
                }
            });
        });

    if cancel {
        app.pending_workspace_switch = None;
        app.status = "Workspace switch canceled".to_owned();
    } else if open {
        app.pending_workspace_switch = None;
        app.open_workspace_now(target);
    }
}
