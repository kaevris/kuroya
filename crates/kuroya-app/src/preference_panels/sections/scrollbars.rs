use crate::preference_panels::sections::{
    SETTINGS_TARGET_SCROLLBARS, SettingsHighlightState, settings_target_block,
};
use crate::ui_switch::ui_switch;
use eframe::egui;
use kuroya_core::{
    EditorScrollbarVisibility, EditorSettings, MAX_EDITOR_SCROLLBAR_SIZE, MIN_EDITOR_SCROLLBAR_SIZE,
};

pub(super) fn render_scrollbar_settings_with_highlight(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    settings_target_block(ui, highlight, SETTINGS_TARGET_SCROLLBARS, |ui| {
        egui::Grid::new("settings_scrollbars_grid")
            .num_columns(2)
            .spacing([18.0, 10.0])
            .show(ui, |ui| {
                ui.label("Editor vertical");
                scrollbar_visibility_combo(
                    ui,
                    "settings_scrollbars_editor_vertical",
                    &mut draft.scrollbar_vertical,
                );
                ui.end_row();

                ui.label("Editor horizontal");
                scrollbar_visibility_combo(
                    ui,
                    "settings_scrollbars_editor_horizontal",
                    &mut draft.scrollbar_horizontal,
                );
                ui.end_row();

                ui.label("Explorer");
                scrollbar_visibility_combo(
                    ui,
                    "settings_scrollbars_explorer",
                    &mut draft.explorer_scrollbar,
                );
                ui.end_row();

                ui.label("Vertical size");
                ui.add(
                    egui::DragValue::new(&mut draft.scrollbar_vertical_scrollbar_size)
                        .speed(1.0)
                        .range(MIN_EDITOR_SCROLLBAR_SIZE..=MAX_EDITOR_SCROLLBAR_SIZE),
                );
                ui.end_row();

                ui.label("Horizontal size");
                ui.add(
                    egui::DragValue::new(&mut draft.scrollbar_horizontal_scrollbar_size)
                        .speed(1.0)
                        .range(MIN_EDITOR_SCROLLBAR_SIZE..=MAX_EDITOR_SCROLLBAR_SIZE),
                );
                ui.end_row();

                ui.label("Horizontal layout");
                ui_switch(
                    ui,
                    &mut draft.scrollbar_ignore_horizontal_scrollbar_in_content_height,
                )
                .on_hover_text("Do not reserve editor height");
                ui.end_row();
            });
    });
}

fn scrollbar_visibility_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorScrollbarVisibility,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(scrollbar_visibility_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorScrollbarVisibility::Auto, "Auto");
            ui.selectable_value(value, EditorScrollbarVisibility::Visible, "Visible");
            ui.selectable_value(value, EditorScrollbarVisibility::Hidden, "Hidden");
        });
}

fn scrollbar_visibility_label(mode: EditorScrollbarVisibility) -> &'static str {
    match mode {
        EditorScrollbarVisibility::Auto => "Auto",
        EditorScrollbarVisibility::Visible => "Visible",
        EditorScrollbarVisibility::Hidden => "Hidden",
    }
}
