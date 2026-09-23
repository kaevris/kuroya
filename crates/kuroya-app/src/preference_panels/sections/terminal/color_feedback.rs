use crate::preference_panels::sections::{
    SETTINGS_TARGET_TERMINAL_COLOR, SettingsHighlightState, bounded_settings_singleline_input,
    bounded_settings_text_edit_width, guarded_f32_drag_value, settings_target_heading,
};
use crate::ui_switch::ui_switch;
use eframe::egui;
use kuroya_core::{
    DEFAULT_TERMINAL_MINIMUM_CONTRAST_RATIO, EditorSettings, MAX_TERMINAL_BELL_DURATION_MS,
    MAX_TERMINAL_MINIMUM_CONTRAST_RATIO, MIN_TERMINAL_BELL_DURATION_MS,
    MIN_TERMINAL_MINIMUM_CONTRAST_RATIO,
};

#[cfg(test)]
pub(super) fn render_color_feedback_settings(ui: &mut egui::Ui, draft: &mut EditorSettings) {
    let mut highlight = SettingsHighlightState::disabled();
    render_color_feedback_settings_with_highlight(ui, draft, &mut highlight);
}

pub(super) fn render_color_feedback_settings_with_highlight(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    ui.add_space(12.0);
    settings_target_heading(
        ui,
        highlight,
        SETTINGS_TARGET_TERMINAL_COLOR,
        "Color and Feedback",
    );
    egui::Grid::new("settings_terminal_color_grid")
        .num_columns(2)
        .spacing([18.0, 10.0])
        .show(ui, |ui| {
            ui.label("Bold ANSI colors");
            ui_switch(ui, &mut draft.terminal_draw_bold_text_in_bright_colors)
                .on_hover_text("Use bright colors");
            ui.end_row();

            ui.label("Minimum contrast");
            guarded_f32_drag_value(
                ui,
                &mut draft.terminal_minimum_contrast_ratio,
                0.25,
                MIN_TERMINAL_MINIMUM_CONTRAST_RATIO..=MAX_TERMINAL_MINIMUM_CONTRAST_RATIO,
                DEFAULT_TERMINAL_MINIMUM_CONTRAST_RATIO,
            );
            ui.end_row();

            ui.label("Tab icon color");
            let mut tab_color = bounded_settings_singleline_input(
                draft
                    .terminal_tabs_default_color
                    .as_deref()
                    .unwrap_or_default(),
            );
            let color_response = ui.add(
                egui::TextEdit::singleline(&mut tab_color)
                    .hint_text("terminal.ansiBlue or #3b78ff")
                    .desired_width(bounded_settings_text_edit_width(
                        ui.available_width(),
                        260.0,
                    )),
            );
            if color_response.changed() {
                let trimmed = tab_color.trim();
                draft.terminal_tabs_default_color =
                    (!trimmed.is_empty()).then(|| trimmed.to_owned());
            }
            color_response.on_hover_text("Theme color ID or #RRGGBB value for terminal tab icons.");
            ui.end_row();

            ui.label("Visual bell");
            ui_switch(ui, &mut draft.terminal_enable_bell).on_hover_text("Flash on bell");
            ui.end_row();

            ui.label("Bell duration");
            ui.add(
                egui::DragValue::new(&mut draft.terminal_bell_duration_ms)
                    .speed(50.0)
                    .range(MIN_TERMINAL_BELL_DURATION_MS..=MAX_TERMINAL_BELL_DURATION_MS),
            );
            ui.end_row();

            ui.label("Exit alert");
            ui_switch(ui, &mut draft.terminal_show_exit_alert)
                .on_hover_text("Show non-zero exit message");
            ui.end_row();
        });
}

#[cfg(test)]
mod tests {
    use super::render_color_feedback_settings;
    use eframe::egui;
    use kuroya_core::EditorSettings;

    #[test]
    fn terminal_color_render_preserves_non_finite_contrast_draft() {
        let ctx = egui::Context::default();
        let mut draft = EditorSettings {
            terminal_minimum_contrast_ratio: f32::NAN,
            ..EditorSettings::default()
        };

        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                render_color_feedback_settings(ui, &mut draft);
            });
        });

        assert!(draft.terminal_minimum_contrast_ratio.is_nan());
    }
}
