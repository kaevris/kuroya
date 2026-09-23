use crate::preference_panels::sections::{
    SETTINGS_TARGET_TERMINAL_CURSOR, SettingsHighlightState, guarded_f32_drag_value,
    settings_target_heading,
};
use crate::ui_switch::ui_switch;
use eframe::egui;
use kuroya_core::{
    DEFAULT_TERMINAL_CURSOR_WIDTH, EditorSettings, MAX_TERMINAL_CURSOR_WIDTH,
    MIN_TERMINAL_CURSOR_WIDTH, TerminalCursorStyle, TerminalInactiveCursorStyle,
};

#[cfg(test)]
pub(super) fn render_cursor_settings(ui: &mut egui::Ui, draft: &mut EditorSettings) {
    let mut highlight = SettingsHighlightState::disabled();
    render_cursor_settings_with_highlight(ui, draft, &mut highlight);
}

pub(super) fn render_cursor_settings_with_highlight(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    ui.add_space(12.0);
    settings_target_heading(ui, highlight, SETTINGS_TARGET_TERMINAL_CURSOR, "Cursor");
    egui::Grid::new("settings_terminal_cursor_grid")
        .num_columns(2)
        .spacing([18.0, 10.0])
        .show(ui, |ui| {
            ui.label("Cursor style");
            terminal_cursor_style_combo(
                ui,
                "terminal_cursor_style",
                &mut draft.terminal_cursor_style,
            );
            ui.end_row();

            ui.label("Cursor width");
            guarded_f32_drag_value(
                ui,
                &mut draft.terminal_cursor_width,
                0.25,
                MIN_TERMINAL_CURSOR_WIDTH..=MAX_TERMINAL_CURSOR_WIDTH,
                DEFAULT_TERMINAL_CURSOR_WIDTH,
            );
            ui.end_row();

            ui.label("Inactive cursor");
            inactive_terminal_cursor_style_combo(
                ui,
                "terminal_cursor_style_inactive",
                &mut draft.terminal_cursor_style_inactive,
            );
            ui.end_row();

            ui.label("Cursor blinking");
            ui_switch(ui, &mut draft.terminal_cursor_blinking).on_hover_text("Blink when focused");
            ui.end_row();
        });
}

fn terminal_cursor_style_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut TerminalCursorStyle,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(terminal_cursor_style_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, TerminalCursorStyle::Block, "Block");
            ui.selectable_value(value, TerminalCursorStyle::Line, "Line");
            ui.selectable_value(value, TerminalCursorStyle::Underline, "Underline");
        });
}

fn inactive_terminal_cursor_style_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut TerminalInactiveCursorStyle,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(inactive_terminal_cursor_style_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, TerminalInactiveCursorStyle::Outline, "Outline");
            ui.selectable_value(value, TerminalInactiveCursorStyle::Block, "Block");
            ui.selectable_value(value, TerminalInactiveCursorStyle::Line, "Line");
            ui.selectable_value(value, TerminalInactiveCursorStyle::Underline, "Underline");
            ui.selectable_value(value, TerminalInactiveCursorStyle::None, "None");
        });
}

fn terminal_cursor_style_label(style: TerminalCursorStyle) -> &'static str {
    match style {
        TerminalCursorStyle::Block => "Block",
        TerminalCursorStyle::Line => "Line",
        TerminalCursorStyle::Underline => "Underline",
    }
}

fn inactive_terminal_cursor_style_label(style: TerminalInactiveCursorStyle) -> &'static str {
    match style {
        TerminalInactiveCursorStyle::Outline => "Outline",
        TerminalInactiveCursorStyle::Block => "Block",
        TerminalInactiveCursorStyle::Line => "Line",
        TerminalInactiveCursorStyle::Underline => "Underline",
        TerminalInactiveCursorStyle::None => "None",
    }
}

#[cfg(test)]
mod tests {
    use super::render_cursor_settings;
    use eframe::egui;
    use kuroya_core::EditorSettings;

    #[test]
    fn terminal_cursor_render_preserves_non_finite_width_draft() {
        let ctx = egui::Context::default();
        let mut draft = EditorSettings {
            terminal_cursor_width: f32::NAN,
            ..EditorSettings::default()
        };

        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                render_cursor_settings(ui, &mut draft);
            });
        });

        assert!(draft.terminal_cursor_width.is_nan());
    }
}
