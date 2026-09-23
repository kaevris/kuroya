use crate::preference_panels::sections::{
    SETTINGS_TARGET_EDITOR_CURSOR, SettingsHighlightState, guarded_f32_drag_value,
    settings_target_heading,
};
use crate::ui_switch::ui_switch;
use eframe::egui;
use kuroya_core::{
    DEFAULT_EDITOR_CURSOR_WIDTH, EditorCursorSmoothCaretAnimation, EditorCursorStyle,
    EditorCursorSurroundingLinesStyle, EditorMouseStyle, EditorRenderLineHighlight, EditorSettings,
    MAX_EDITOR_CURSOR_HEIGHT, MAX_EDITOR_CURSOR_SURROUNDING_LINES, MAX_EDITOR_CURSOR_WIDTH,
    MIN_EDITOR_CURSOR_HEIGHT, MIN_EDITOR_CURSOR_WIDTH,
};

pub(super) fn render_cursor_settings_with_highlight(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    ui.add_space(12.0);
    settings_target_heading(
        ui,
        highlight,
        SETTINGS_TARGET_EDITOR_CURSOR,
        "Cursor and Highlight",
    );
    egui::Grid::new("settings_editor_cursor_grid")
        .num_columns(2)
        .spacing([18.0, 10.0])
        .show(ui, |ui| {
            ui.label("Mouse pointer");
            editor_mouse_style_combo(ui, "editor_mouse_style", &mut draft.mouse_style);
            ui.end_row();

            ui.label("Smooth caret");
            editor_cursor_smooth_caret_animation_combo(
                ui,
                "editor_cursor_smooth_caret_animation",
                &mut draft.cursor_smooth_caret_animation,
            );
            ui.end_row();

            ui.label("Cursor style");
            editor_cursor_style_combo(ui, "editor_cursor_style", &mut draft.cursor_style);
            ui.end_row();

            ui.label("Overtype cursor");
            editor_cursor_style_combo(
                ui,
                "editor_overtype_cursor_style",
                &mut draft.overtype_cursor_style,
            );
            ui.end_row();

            ui.label("Overtype paste");
            ui_switch(ui, &mut draft.overtype_on_paste)
                .on_hover_text("Paste overwrites in overtype mode");
            ui.end_row();

            ui.label("Cursor width");
            guarded_f32_drag_value(
                ui,
                &mut draft.cursor_width,
                0.25,
                MIN_EDITOR_CURSOR_WIDTH..=MAX_EDITOR_CURSOR_WIDTH,
                DEFAULT_EDITOR_CURSOR_WIDTH,
            );
            ui.end_row();

            ui.label("Cursor height");
            ui.add(
                egui::DragValue::new(&mut draft.cursor_height)
                    .speed(1.0)
                    .range(MIN_EDITOR_CURSOR_HEIGHT..=MAX_EDITOR_CURSOR_HEIGHT),
            )
            .on_hover_text("0 follows the line height.");
            ui.end_row();

            ui.label("Cursor blinking");
            ui_switch(ui, &mut draft.cursor_blinking);
            ui.end_row();

            ui.label("Surrounding lines");
            ui.add(
                egui::DragValue::new(&mut draft.cursor_surrounding_lines)
                    .speed(1.0)
                    .range(0..=MAX_EDITOR_CURSOR_SURROUNDING_LINES),
            )
            .on_hover_text("Minimum visible lines to keep above and below the cursor.");
            ui.end_row();

            ui.label("Surrounding style");
            editor_cursor_surrounding_lines_style_combo(
                ui,
                "editor_cursor_surrounding_lines_style",
                &mut draft.cursor_surrounding_lines_style,
            );
            ui.end_row();

            ui.label("Line highlight");
            editor_render_line_highlight_combo(
                ui,
                "editor_render_line_highlight",
                &mut draft.render_line_highlight,
            );
            ui.end_row();

            ui.label("Highlight on focus");
            ui_switch(ui, &mut draft.render_line_highlight_only_when_focus)
                .on_hover_text("Only highlight cursor line in the focused pane");
            ui.end_row();
        });
}

fn editor_mouse_style_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorMouseStyle,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_mouse_style_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorMouseStyle::Text, "Text");
            ui.selectable_value(value, EditorMouseStyle::SystemDefault, "Default");
            ui.selectable_value(value, EditorMouseStyle::Copy, "Copy");
        });
}

fn editor_mouse_style_label(style: EditorMouseStyle) -> &'static str {
    match style {
        EditorMouseStyle::Text => "Text",
        EditorMouseStyle::SystemDefault => "Default",
        EditorMouseStyle::Copy => "Copy",
    }
}

fn editor_cursor_smooth_caret_animation_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorCursorSmoothCaretAnimation,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_cursor_smooth_caret_animation_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorCursorSmoothCaretAnimation::Off, "Off");
            ui.selectable_value(
                value,
                EditorCursorSmoothCaretAnimation::Explicit,
                "Explicit",
            );
            ui.selectable_value(value, EditorCursorSmoothCaretAnimation::On, "On");
        });
}

fn editor_cursor_smooth_caret_animation_label(
    mode: EditorCursorSmoothCaretAnimation,
) -> &'static str {
    match mode {
        EditorCursorSmoothCaretAnimation::Off => "Off",
        EditorCursorSmoothCaretAnimation::Explicit => "Explicit",
        EditorCursorSmoothCaretAnimation::On => "On",
    }
}

fn editor_cursor_surrounding_lines_style_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorCursorSurroundingLinesStyle,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_cursor_surrounding_lines_style_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorCursorSurroundingLinesStyle::Default, "Default");
            ui.selectable_value(value, EditorCursorSurroundingLinesStyle::All, "All");
        });
}

fn editor_cursor_surrounding_lines_style_label(
    mode: EditorCursorSurroundingLinesStyle,
) -> &'static str {
    match mode {
        EditorCursorSurroundingLinesStyle::Default => "Default",
        EditorCursorSurroundingLinesStyle::All => "All",
    }
}

fn editor_render_line_highlight_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorRenderLineHighlight,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_render_line_highlight_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorRenderLineHighlight::None, "None");
            ui.selectable_value(value, EditorRenderLineHighlight::Gutter, "Gutter");
            ui.selectable_value(value, EditorRenderLineHighlight::Line, "Line");
            ui.selectable_value(value, EditorRenderLineHighlight::All, "All");
        });
}

fn editor_render_line_highlight_label(mode: EditorRenderLineHighlight) -> &'static str {
    match mode {
        EditorRenderLineHighlight::None => "None",
        EditorRenderLineHighlight::Gutter => "Gutter",
        EditorRenderLineHighlight::Line => "Line",
        EditorRenderLineHighlight::All => "All",
    }
}

fn editor_cursor_style_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorCursorStyle,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_cursor_style_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorCursorStyle::Line, "Line");
            ui.selectable_value(value, EditorCursorStyle::Block, "Block");
            ui.selectable_value(value, EditorCursorStyle::Underline, "Underline");
            ui.selectable_value(value, EditorCursorStyle::LineThin, "Line thin");
            ui.selectable_value(value, EditorCursorStyle::BlockOutline, "Block outline");
            ui.selectable_value(value, EditorCursorStyle::UnderlineThin, "Underline thin");
        });
}

fn editor_cursor_style_label(style: EditorCursorStyle) -> &'static str {
    match style {
        EditorCursorStyle::Line => "Line",
        EditorCursorStyle::Block => "Block",
        EditorCursorStyle::Underline => "Underline",
        EditorCursorStyle::LineThin => "Line thin",
        EditorCursorStyle::BlockOutline => "Block outline",
        EditorCursorStyle::UnderlineThin => "Underline thin",
    }
}
