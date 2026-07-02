use crate::preference_panels::sections::{
    SETTINGS_TARGET_EDITOR_TEXT_LAYOUT, SettingsHighlightState, bounded_settings_multiline_join,
    bounded_settings_text_edit_width, bounded_singleline_text_edit, guarded_f32_drag_value,
    settings_target_heading,
};
use eframe::egui;
use kuroya_core::{
    DEFAULT_EDITOR_LETTER_SPACING, DEFAULT_EDITOR_LINE_HEIGHT, EditorAccessibilitySupport,
    EditorPeekWidgetDefaultFocus, EditorSettings, EditorUnusualLineTerminators, EditorWordBreak,
    EditorWordWrap, EditorWordWrapOverride, EditorWrappingIndent, EditorWrappingStrategy,
    MAX_EDITOR_ACCESSIBILITY_PAGE_SIZE, MAX_EDITOR_LETTER_SPACING, MAX_EDITOR_LINE_HEIGHT,
    MAX_EDITOR_OVERVIEW_RULER_LANES, MAX_EDITOR_REVEAL_HORIZONTAL_RIGHT_PADDING,
    MAX_EDITOR_STOP_RENDERING_LINE_AFTER, MAX_EDITOR_TAB_INDEX, MAX_EDITOR_WORD_WRAP_COLUMN,
    MIN_EDITOR_ACCESSIBILITY_PAGE_SIZE, MIN_EDITOR_LETTER_SPACING, MIN_EDITOR_LINE_HEIGHT,
    MIN_EDITOR_OVERVIEW_RULER_LANES, MIN_EDITOR_REVEAL_HORIZONTAL_RIGHT_PADDING,
    MIN_EDITOR_STOP_RENDERING_LINE_AFTER, MIN_EDITOR_TAB_INDEX, MIN_EDITOR_WORD_WRAP_COLUMN,
};

mod display;

const MIN_SETTINGS_PANEL_FONT_SIZE: f32 = 10.0;
const MAX_SETTINGS_PANEL_FONT_SIZE: f32 = 28.0;
const DEFAULT_SETTINGS_PANEL_FONT_SIZE: f32 = 13.0;

pub(super) fn render_text_layout_settings_with_highlight(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    settings_target_heading(
        ui,
        highlight,
        SETTINGS_TARGET_EDITOR_TEXT_LAYOUT,
        "Text and Layout",
    );
    egui::Grid::new("settings_editor_text_grid")
        .num_columns(2)
        .spacing([18.0, 10.0])
        .show(ui, |ui| {
            ui.label("Editor font size");
            guarded_f32_drag_value(
                ui,
                &mut draft.font_size,
                0.25,
                MIN_SETTINGS_PANEL_FONT_SIZE..=MAX_SETTINGS_PANEL_FONT_SIZE,
                DEFAULT_SETTINGS_PANEL_FONT_SIZE,
            );
            ui.end_row();

            ui.label("Font family");
            bounded_singleline_text_edit(ui, &mut draft.font_family, 260.0);
            ui.end_row();

            ui.label("Font weight");
            bounded_singleline_text_edit(ui, &mut draft.font_weight, 120.0)
                .on_hover_text("Accepts normal, bold, or 1-1000");
            ui.end_row();

            ui.label("Letter spacing");
            guarded_f32_drag_value(
                ui,
                &mut draft.letter_spacing,
                0.1,
                MIN_EDITOR_LETTER_SPACING..=MAX_EDITOR_LETTER_SPACING,
                DEFAULT_EDITOR_LETTER_SPACING,
            );
            ui.end_row();

            ui.label("Font ligatures");
            bounded_singleline_text_edit(ui, &mut draft.font_ligatures, 260.0)
                .on_hover_text("Use true, false, or CSS font-feature-settings");
            ui.end_row();

            ui.label("Font variations");
            bounded_singleline_text_edit(ui, &mut draft.font_variations, 260.0)
                .on_hover_text("Use true, false, or CSS font-variation-settings");
            ui.end_row();

            ui.label("Automatic layout");
            ui.checkbox(&mut draft.automatic_layout, "Measure layout automatically");
            ui.end_row();

            ui.label("Render optimizations");
            ui.vertical(|ui| {
                ui.checkbox(&mut draft.disable_layer_hinting, "Disable layer hinting");
                ui.checkbox(
                    &mut draft.disable_monospace_optimizations,
                    "Disable monospace optimizations",
                );
            });
            ui.end_row();

            ui.label("Editor CSS class");
            bounded_singleline_text_edit(ui, &mut draft.extra_editor_class_name, 220.0);
            ui.end_row();

            ui.label("Variable line heights");
            ui.checkbox(
                &mut draft.allow_variable_line_heights,
                "Allow variable line heights",
            );
            ui.end_row();

            ui.label("Variable fonts");
            ui.vertical(|ui| {
                ui.checkbox(&mut draft.allow_variable_fonts, "Allow variable fonts");
                ui.checkbox(
                    &mut draft.allow_variable_fonts_in_accessibility_mode,
                    "Allow in accessibility mode",
                );
            });
            ui.end_row();

            ui.label("Accessibility support");
            editor_accessibility_support_combo(
                ui,
                "editor_accessibility_support",
                &mut draft.accessibility_support,
            );
            ui.end_row();

            ui.label("Accessibility page size");
            ui.add(
                egui::DragValue::new(&mut draft.accessibility_page_size)
                    .speed(10.0)
                    .range(MIN_EDITOR_ACCESSIBILITY_PAGE_SIZE..=MAX_EDITOR_ACCESSIBILITY_PAGE_SIZE),
            );
            ui.end_row();

            ui.label("ARIA label");
            bounded_singleline_text_edit(ui, &mut draft.aria_label, 260.0);
            ui.end_row();

            ui.label("ARIA required");
            ui.checkbox(
                &mut draft.aria_required,
                "Mark the editor textarea as required",
            );
            ui.end_row();

            ui.label("Screen reader suggestions");
            ui.checkbox(
                &mut draft.screen_reader_announce_inline_suggestion,
                "Announce inline suggestions",
            );
            ui.end_row();

            ui.label("Tab index");
            ui.add(
                egui::DragValue::new(&mut draft.tab_index)
                    .speed(1.0)
                    .range(MIN_EDITOR_TAB_INDEX..=MAX_EDITOR_TAB_INDEX),
            );
            ui.end_row();

            ui.label("Overflow widgets");
            ui.vertical(|ui| {
                ui.checkbox(&mut draft.allow_overflow, "Allow widget overflow");
                ui.checkbox(
                    &mut draft.fixed_overflow_widgets,
                    "Use fixed overflow widgets",
                );
            });
            ui.end_row();

            ui.label("Edit context");
            ui.vertical(|ui| {
                ui.checkbox(&mut draft.edit_context, "Use EditContext input");
                ui.checkbox(
                    &mut draft.render_rich_screen_reader_content,
                    "Render rich screen reader content",
                );
            });
            ui.end_row();

            ui.label("Whitespace delete");
            ui.checkbox(
                &mut draft.trim_whitespace_on_delete,
                "Trim indentation when deleting a newline",
            );
            ui.end_row();

            ui.label("Line terminators");
            editor_unusual_line_terminators_combo(
                ui,
                "editor_unusual_line_terminators",
                &mut draft.unusual_line_terminators,
            );
            ui.end_row();

            ui.label("Shadow DOM");
            ui.checkbox(&mut draft.use_shadow_dom, "Use Shadow DOM");
            ui.end_row();

            ui.label("Tab stops");
            ui.checkbox(&mut draft.use_tab_stops, "Insert and delete by tab stops");
            ui.end_row();

            ui.label("Tab width");
            ui.add(
                egui::DragValue::new(&mut draft.tab_width)
                    .speed(1.0)
                    .range(1..=12),
            );
            ui.end_row();

            ui.label("Insert spaces");
            ui.checkbox(&mut draft.insert_spaces, "Use spaces for Tab");
            ui.end_row();

            ui.label("Detect indentation");
            ui.checkbox(
                &mut draft.detect_indentation,
                "Use file indentation when detected",
            );
            ui.end_row();

            ui.label("Word separators");
            bounded_singleline_text_edit(ui, &mut draft.word_separators, 260.0)
                .on_hover_text("Characters that split words for navigation and selection");
            ui.end_row();

            ui.label("Word segmenter locales");
            render_string_list_input(ui, &mut draft.word_segmenter_locales, "ja\nzh-CN", 2);
            ui.end_row();

            ui.label("Line height");
            guarded_f32_drag_value(
                ui,
                &mut draft.line_height,
                0.5,
                MIN_EDITOR_LINE_HEIGHT..=MAX_EDITOR_LINE_HEIGHT,
                DEFAULT_EDITOR_LINE_HEIGHT,
            )
            .on_hover_text("Use 0 for automatic line height");
            ui.end_row();

            ui.label("Word wrap");
            editor_word_wrap_combo(ui, "editor_word_wrap", &mut draft.word_wrap);
            ui.end_row();

            ui.label("Wrap overrides");
            ui.horizontal(|ui| {
                editor_word_wrap_override_combo(
                    ui,
                    "editor_word_wrap_override1",
                    &mut draft.word_wrap_override1,
                );
                editor_word_wrap_override_combo(
                    ui,
                    "editor_word_wrap_override2",
                    &mut draft.word_wrap_override2,
                );
            });
            ui.end_row();

            ui.label("Wrap break after");
            bounded_singleline_text_edit(ui, &mut draft.word_wrap_break_after_characters, 260.0);
            ui.end_row();

            ui.label("Wrap break before");
            bounded_singleline_text_edit(ui, &mut draft.word_wrap_break_before_characters, 260.0);
            ui.end_row();

            ui.label("Wrap column");
            ui.add(
                egui::DragValue::new(&mut draft.word_wrap_column)
                    .speed(1.0)
                    .range(MIN_EDITOR_WORD_WRAP_COLUMN..=MAX_EDITOR_WORD_WRAP_COLUMN),
            )
            .on_hover_text("Column used by Word Wrap Column and Bounded modes");
            ui.end_row();

            ui.label("Wrapping indent");
            editor_wrapping_indent_combo(ui, "editor_wrapping_indent", &mut draft.wrapping_indent);
            ui.end_row();

            ui.label("Wrapping strategy");
            editor_wrapping_strategy_combo(
                ui,
                "editor_wrapping_strategy",
                &mut draft.wrapping_strategy,
            );
            ui.end_row();

            ui.label("Escaped line feeds");
            ui.checkbox(
                &mut draft.wrap_on_escaped_line_feeds,
                "Wrap on literal newline escapes",
            );
            ui.end_row();

            ui.label("Word break");
            editor_word_break_combo(ui, "editor_word_break", &mut draft.word_break);
            ui.end_row();

            ui.label("Reveal right padding");
            ui.add(
                egui::DragValue::new(&mut draft.reveal_horizontal_right_padding)
                    .speed(1.0)
                    .range(
                        MIN_EDITOR_REVEAL_HORIZONTAL_RIGHT_PADDING
                            ..=MAX_EDITOR_REVEAL_HORIZONTAL_RIGHT_PADDING,
                    ),
            );
            ui.end_row();

            ui.label("Rounded selection");
            ui.checkbox(&mut draft.rounded_selection, "Round selection corners");
            ui.end_row();

            ui.label("Overview ruler lanes");
            ui.add(
                egui::DragValue::new(&mut draft.overview_ruler_lanes)
                    .speed(1.0)
                    .range(MIN_EDITOR_OVERVIEW_RULER_LANES..=MAX_EDITOR_OVERVIEW_RULER_LANES),
            );
            ui.end_row();

            ui.label("Peek focus");
            editor_peek_widget_default_focus_combo(
                ui,
                "editor_peek_widget_default_focus",
                &mut draft.peek_widget_default_focus,
            );
            ui.end_row();

            ui.label("Placeholder");
            bounded_singleline_text_edit(ui, &mut draft.placeholder, 260.0);
            ui.end_row();

            ui.label("Definition links");
            ui.checkbox(
                &mut draft.definition_link_opens_in_peek,
                "Open definition links in Peek",
            );
            ui.end_row();

            ui.label("Long line limit");
            ui.add(
                egui::DragValue::new(&mut draft.stop_rendering_line_after)
                    .speed(100.0)
                    .range(
                        MIN_EDITOR_STOP_RENDERING_LINE_AFTER..=MAX_EDITOR_STOP_RENDERING_LINE_AFTER,
                    ),
            )
            .on_hover_text("Characters to render per line before stopping; use -1 for no limit");
            ui.end_row();
        });

    display::render_display_settings_with_highlight(ui, draft, highlight);
}

fn editor_word_wrap_combo(ui: &mut egui::Ui, id: impl std::hash::Hash, value: &mut EditorWordWrap) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_word_wrap_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorWordWrap::Off, "Off");
            ui.selectable_value(value, EditorWordWrap::On, "On");
            ui.selectable_value(value, EditorWordWrap::WordWrapColumn, "Column");
            ui.selectable_value(value, EditorWordWrap::Bounded, "Bounded");
        });
}

fn editor_word_wrap_label(mode: EditorWordWrap) -> &'static str {
    match mode {
        EditorWordWrap::Off => "Off",
        EditorWordWrap::On => "On",
        EditorWordWrap::WordWrapColumn => "Column",
        EditorWordWrap::Bounded => "Bounded",
    }
}

fn editor_word_wrap_override_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorWordWrapOverride,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_word_wrap_override_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorWordWrapOverride::Inherit, "Inherit");
            ui.selectable_value(value, EditorWordWrapOverride::Off, "Off");
            ui.selectable_value(value, EditorWordWrapOverride::On, "On");
        });
}

fn editor_word_wrap_override_label(mode: EditorWordWrapOverride) -> &'static str {
    match mode {
        EditorWordWrapOverride::Off => "Off",
        EditorWordWrapOverride::On => "On",
        EditorWordWrapOverride::Inherit => "Inherit",
    }
}

fn editor_accessibility_support_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorAccessibilitySupport,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_accessibility_support_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorAccessibilitySupport::Auto, "Auto");
            ui.selectable_value(value, EditorAccessibilitySupport::On, "On");
            ui.selectable_value(value, EditorAccessibilitySupport::Off, "Off");
        });
}

fn editor_accessibility_support_label(mode: EditorAccessibilitySupport) -> &'static str {
    match mode {
        EditorAccessibilitySupport::Auto => "Auto",
        EditorAccessibilitySupport::On => "On",
        EditorAccessibilitySupport::Off => "Off",
    }
}

fn editor_unusual_line_terminators_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorUnusualLineTerminators,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_unusual_line_terminators_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorUnusualLineTerminators::Prompt, "Prompt");
            ui.selectable_value(value, EditorUnusualLineTerminators::Auto, "Auto");
            ui.selectable_value(value, EditorUnusualLineTerminators::Off, "Off");
        });
}

fn editor_unusual_line_terminators_label(mode: EditorUnusualLineTerminators) -> &'static str {
    match mode {
        EditorUnusualLineTerminators::Auto => "Auto",
        EditorUnusualLineTerminators::Off => "Off",
        EditorUnusualLineTerminators::Prompt => "Prompt",
    }
}

fn editor_peek_widget_default_focus_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorPeekWidgetDefaultFocus,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_peek_widget_default_focus_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorPeekWidgetDefaultFocus::Tree, "Tree");
            ui.selectable_value(value, EditorPeekWidgetDefaultFocus::Editor, "Editor");
        });
}

fn editor_peek_widget_default_focus_label(mode: EditorPeekWidgetDefaultFocus) -> &'static str {
    match mode {
        EditorPeekWidgetDefaultFocus::Tree => "Tree",
        EditorPeekWidgetDefaultFocus::Editor => "Editor",
    }
}

fn editor_wrapping_indent_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorWrappingIndent,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_wrapping_indent_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorWrappingIndent::None, "None");
            ui.selectable_value(value, EditorWrappingIndent::Same, "Same");
            ui.selectable_value(value, EditorWrappingIndent::Indent, "Indent");
            ui.selectable_value(value, EditorWrappingIndent::DeepIndent, "Deep indent");
        });
}

fn editor_wrapping_indent_label(mode: EditorWrappingIndent) -> &'static str {
    match mode {
        EditorWrappingIndent::None => "None",
        EditorWrappingIndent::Same => "Same",
        EditorWrappingIndent::Indent => "Indent",
        EditorWrappingIndent::DeepIndent => "Deep indent",
    }
}

fn editor_wrapping_strategy_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorWrappingStrategy,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_wrapping_strategy_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorWrappingStrategy::Simple, "Simple");
            ui.selectable_value(value, EditorWrappingStrategy::Advanced, "Advanced");
        });
}

fn editor_wrapping_strategy_label(mode: EditorWrappingStrategy) -> &'static str {
    match mode {
        EditorWrappingStrategy::Simple => "Simple",
        EditorWrappingStrategy::Advanced => "Advanced",
    }
}

fn editor_word_break_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorWordBreak,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_word_break_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorWordBreak::Normal, "Normal");
            ui.selectable_value(value, EditorWordBreak::KeepAll, "Keep all");
        });
}

fn editor_word_break_label(mode: EditorWordBreak) -> &'static str {
    match mode {
        EditorWordBreak::Normal => "Normal",
        EditorWordBreak::KeepAll => "Keep all",
    }
}

fn render_string_list_input(
    ui: &mut egui::Ui,
    values: &mut Vec<String>,
    hint_text: &'static str,
    rows: usize,
) {
    let mut value = bounded_settings_multiline_join(values.iter().map(String::as_str));
    let response = ui.add_sized(
        [
            bounded_settings_text_edit_width(ui.available_width(), 320.0),
            (rows as f32 * 24.0).max(48.0),
        ],
        egui::TextEdit::multiline(&mut value)
            .desired_rows(rows)
            .hint_text(hint_text),
    );
    if response.changed() {
        *values = parse_string_list_input_value(&value);
    }
}

fn parse_string_list_input_value(value: &str) -> Vec<String> {
    value.lines().map(ToOwned::to_owned).collect()
}

#[cfg(test)]
mod tests {
    use super::{parse_string_list_input_value, render_string_list_input};
    use eframe::egui;

    #[test]
    fn string_list_parse_preserves_raw_lines_for_apply() {
        let values = parse_string_list_input_value(" ja \n\nzh-CN ");

        assert_eq!(values, [" ja ", "", "zh-CN "]);
    }

    #[test]
    fn string_list_render_keeps_raw_values_when_unchanged() {
        let ctx = egui::Context::default();
        let mut values = vec![" ja ".to_owned(), "".to_owned(), "zh-CN ".to_owned()];
        let original = values.clone();

        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                render_string_list_input(ui, &mut values, "ja\nzh-CN", 2);
            });
        });

        assert_eq!(values, original);
    }
}
