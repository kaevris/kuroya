use crate::{
    KuroyaApp,
    popup_buttons::{PopupButtonKind, popup_button},
    ui_icon_shapes::draw_icon,
    ui_icons::IconKind,
    workspace_guard_runtime::workspace_guard_display_path,
};
use eframe::egui::{self, Align, Context, Key, RichText, Sense, Vec2};
use std::path::PathBuf;

pub(super) fn render_workspace_trust_prompt(app: &mut KuroyaApp, ctx: &Context, root: PathBuf) {
    let mut trust = false;
    let mut keep_restricted = false;

    egui::Window::new("Workspace Trust")
        .max_size(crate::layout::popup_window_max_size_with_top_margin(
            ctx, 24.0,
        ))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .fixed_size([520.0, 248.0])
        .show(ctx, |ui| {
            let visuals = ui.visuals().clone();
            let accent = visuals.selection.bg_fill;

            ui.add_space(6.0);

            ui.horizontal(|ui| {
                let (badge_rect, _) = ui.allocate_exact_size(Vec2::splat(38.0), Sense::hover());
                let badge_rect = badge_rect.shrink(3.0);
                ui.painter()
                    .rect_filled(badge_rect, 10.0, accent.gamma_multiply(0.30));
                draw_icon(
                    ui,
                    badge_rect,
                    IconKind::Diagnostics,
                    visuals.strong_text_color(),
                );

                ui.add_space(6.0);
                ui.label(RichText::new("Trust this workspace?").heading());
            });

            ui.add_space(10.0);
            ui.label(RichText::new("Restricted mode").strong());
            ui.label(RichText::new(format!(
                "You are working in {}.",
                workspace_guard_display_path(&root)
            )));

            ui.add_space(10.0);
            ui.label(
                RichText::new(
                    "While restricted, these stay disabled until the workspace is trusted:",
                )
                .weak(),
            );
            ui.label(RichText::new("LSP  •  Workspace tasks  •  Plugins").strong());

            ui.add_space(12.0);
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if popup_button(ui, "Keep Restricted", PopupButtonKind::Secondary).clicked() {
                    keep_restricted = true;
                }
                if popup_button(ui, "Trust Workspace", PopupButtonKind::Primary).clicked() {
                    trust = true;
                }
            });

            if ui.input(|input| input.key_pressed(Key::Escape)) {
                keep_restricted = true;
            }
        });

    if trust {
        app.trust_current_workspace();
    } else if keep_restricted {
        app.keep_workspace_restricted();
    }
}
