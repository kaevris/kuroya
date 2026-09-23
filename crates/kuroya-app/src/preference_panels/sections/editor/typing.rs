use crate::preference_panels::sections::{
    SETTINGS_TARGET_EDITOR_TYPING, SettingsHighlightState, bounded_singleline_text_edit,
    settings_target_heading,
};
use crate::ui_switch::ui_switch;
use eframe::egui;
use kuroya_core::{
    EditorAutoClosingEditStrategy, EditorAutoClosingStrategy, EditorDropIntoEditorShowDropSelector,
    EditorMouseMiddleClickAction, EditorMultiCursorModifier, EditorMultiCursorPaste,
    EditorPasteAsShowPasteSelector, EditorSettings, MAX_EDITOR_MULTI_CURSOR_LIMIT,
    MIN_EDITOR_MULTI_CURSOR_LIMIT, clamp_editor_multi_cursor_limit,
};

pub(super) fn render_typing_settings_with_highlight(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    ui.add_space(12.0);
    settings_target_heading(
        ui,
        highlight,
        SETTINGS_TARGET_EDITOR_TYPING,
        "Typing Assistance",
    );
    egui::Grid::new("settings_editor_typing_grid")
        .num_columns(2)
        .spacing([18.0, 10.0])
        .show(ui, |ui| {
            ui.label("Auto indent");
            ui_switch(ui, &mut draft.auto_indent).on_hover_text("Indent new lines from context");
            ui.end_row();

            ui.label("Auto close brackets");
            ui_switch(ui, &mut draft.auto_closing_brackets)
                .on_hover_text("Insert matching brackets");
            ui.end_row();

            ui.label("Auto close quotes");
            ui_switch(ui, &mut draft.auto_closing_quotes).on_hover_text("Insert matching quotes");
            ui.end_row();

            ui.label("Auto close comments");
            editor_auto_closing_strategy_combo(
                ui,
                "editor_auto_closing_comments",
                &mut draft.auto_closing_comments,
            );
            ui.end_row();

            ui.label("Auto close delete");
            editor_auto_closing_edit_strategy_combo(
                ui,
                "editor_auto_closing_delete",
                &mut draft.auto_closing_delete,
            );
            ui.end_row();

            ui.label("Auto close overtype");
            editor_auto_closing_edit_strategy_combo(
                ui,
                "editor_auto_closing_overtype",
                &mut draft.auto_closing_overtype,
            );
            ui.end_row();

            ui.label("Auto surround");
            ui_switch(ui, &mut draft.auto_surround).on_hover_text("Wrap selections with pairs");
            ui.end_row();

            ui.label("Indent pasted text");
            ui_switch(ui, &mut draft.auto_indent_on_paste);
            ui.end_row();

            ui.label("Indent paste in strings");
            ui_switch(ui, &mut draft.auto_indent_on_paste_within_string);
            ui.end_row();

            ui.label("Format on paste");
            ui_switch(ui, &mut draft.format_on_paste);
            ui.end_row();

            ui.label("Paste as");
            ui_switch(ui, &mut draft.paste_as_enabled)
                .on_hover_text("Enable paste transformations");
            ui.end_row();

            ui.label("Paste selector");
            editor_paste_as_show_paste_selector_combo(
                ui,
                "editor_paste_as_show_paste_selector",
                &mut draft.paste_as_show_paste_selector,
            );
            ui.end_row();

            ui.label("Format on type");
            ui_switch(ui, &mut draft.format_on_type).on_hover_text("Format the line after typing");
            ui.end_row();

            ui.label("Sticky tab stops");
            ui_switch(ui, &mut draft.sticky_tab_stops)
                .on_hover_text("Make selection snap to tab stops when using spaces");
            ui.end_row();

            ui.label("Linked editing");
            ui_switch(ui, &mut draft.linked_editing).on_hover_text("Edit linked symbols together");
            ui.end_row();

            ui.label("Rename on type");
            ui_switch(ui, &mut draft.rename_on_type)
                .on_hover_text("Rename linked symbols while typing");
            ui.end_row();

            ui.label("Tab focus mode");
            ui_switch(ui, &mut draft.tab_focus_mode).on_hover_text("Move focus with Tab");
            ui.end_row();

            ui.label("Read-only");
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let r = ui_switch(ui, &mut draft.read_only);
                    ui.label(
                        egui::RichText::new("Open editors as read-only")
                            .small()
                            .weak(),
                    );
                    r
                });
                ui.horizontal(|ui| {
                    let r = ui_switch(ui, &mut draft.dom_read_only);
                    ui.label(
                        egui::RichText::new("Use DOM read-only input")
                            .small()
                            .weak(),
                    );
                    r
                });
            });
            ui.end_row();

            ui.label("Read-only message");
            bounded_singleline_text_edit(ui, &mut draft.read_only_message, 260.0);
            ui.end_row();

            ui.label("Comment spacing");
            ui_switch(ui, &mut draft.comments_insert_space)
                .on_hover_text("Insert a space after line comment tokens");
            ui.end_row();

            ui.label("Ignore empty comments");
            ui_switch(ui, &mut draft.comments_ignore_empty_lines)
                .on_hover_text("Skip empty lines when toggling comments");
            ui.end_row();

            ui.label("Select blocks");
            ui_switch(ui, &mut draft.double_click_selects_block)
                .on_hover_text("Double-clicking beside brackets selects block content");
            ui.end_row();

            ui.label("Drag and drop");
            ui_switch(ui, &mut draft.drag_and_drop)
                .on_hover_text("Allow moving selections with drag and drop");
            ui.end_row();

            ui.label("Drop into editor");
            ui_switch(ui, &mut draft.drop_into_editor_enabled)
                .on_hover_text("Enable external drop handling");
            ui.end_row();

            ui.label("Drop selector");
            editor_drop_into_editor_show_drop_selector_combo(
                ui,
                "editor_drop_into_editor_show_drop_selector",
                &mut draft.drop_into_editor_show_drop_selector,
            );
            ui.end_row();

            ui.label("Multi cursor modifier");
            editor_multi_cursor_modifier_combo(
                ui,
                "editor_multi_cursor_modifier",
                &mut draft.multi_cursor_modifier,
            );
            ui.end_row();

            ui.label("Multi cursor behavior");
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let r = ui_switch(ui, &mut draft.multi_cursor_merge_overlapping);
                    ui.label(
                        egui::RichText::new("Merge overlapping cursors")
                            .small()
                            .weak(),
                    );
                    r
                });
                ui.horizontal(|ui| {
                    let r = ui_switch(ui, &mut draft.column_selection);
                    ui.label(
                        egui::RichText::new("Enable column selection")
                            .small()
                            .weak(),
                    );
                    r
                });
            });
            ui.end_row();

            ui.label("Multi cursor paste");
            editor_multi_cursor_paste_combo(
                ui,
                "editor_multi_cursor_paste",
                &mut draft.multi_cursor_paste,
            );
            ui.end_row();

            ui.label("Multi cursor limit");
            editor_multi_cursor_limit_drag_value(ui, &mut draft.multi_cursor_limit);
            ui.end_row();

            ui.label("Middle click");
            editor_mouse_middle_click_action_combo(
                ui,
                "editor_mouse_middle_click_action",
                &mut draft.mouse_middle_click_action,
            );
            ui.end_row();

            ui.label("Empty selection clipboard");
            ui_switch(ui, &mut draft.empty_selection_clipboard)
                .on_hover_text("Copy or cut the current line when nothing is selected");
            ui.end_row();

            ui.label("Selection clipboard");
            ui_switch(ui, &mut draft.selection_clipboard)
                .on_hover_text("Support the primary selection clipboard");
            ui.end_row();

            ui.label("Copy highlighting");
            ui_switch(ui, &mut draft.copy_with_syntax_highlighting)
                .on_hover_text("Copy text with syntax highlighting");
            ui.end_row();
        });
}

fn editor_multi_cursor_limit_drag_value(ui: &mut egui::Ui, value: &mut usize) {
    let mut clamped_value = clamp_editor_multi_cursor_limit(*value);
    let response = ui.add(
        egui::DragValue::new(&mut clamped_value)
            .speed(100.0)
            .range(MIN_EDITOR_MULTI_CURSOR_LIMIT..=MAX_EDITOR_MULTI_CURSOR_LIMIT),
    );

    if response.changed() {
        *value = clamped_value;
    }
}

fn editor_auto_closing_strategy_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorAutoClosingStrategy,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_auto_closing_strategy_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorAutoClosingStrategy::Always, "Always");
            ui.selectable_value(
                value,
                EditorAutoClosingStrategy::LanguageDefined,
                "Language defined",
            );
            ui.selectable_value(
                value,
                EditorAutoClosingStrategy::BeforeWhitespace,
                "Before whitespace",
            );
            ui.selectable_value(value, EditorAutoClosingStrategy::Never, "Never");
        });
}

fn editor_auto_closing_strategy_label(mode: EditorAutoClosingStrategy) -> &'static str {
    match mode {
        EditorAutoClosingStrategy::Always => "Always",
        EditorAutoClosingStrategy::LanguageDefined => "Language defined",
        EditorAutoClosingStrategy::BeforeWhitespace => "Before whitespace",
        EditorAutoClosingStrategy::Never => "Never",
    }
}

fn editor_auto_closing_edit_strategy_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorAutoClosingEditStrategy,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_auto_closing_edit_strategy_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorAutoClosingEditStrategy::Always, "Always");
            ui.selectable_value(value, EditorAutoClosingEditStrategy::Auto, "Auto");
            ui.selectable_value(value, EditorAutoClosingEditStrategy::Never, "Never");
        });
}

fn editor_auto_closing_edit_strategy_label(mode: EditorAutoClosingEditStrategy) -> &'static str {
    match mode {
        EditorAutoClosingEditStrategy::Always => "Always",
        EditorAutoClosingEditStrategy::Auto => "Auto",
        EditorAutoClosingEditStrategy::Never => "Never",
    }
}

fn editor_paste_as_show_paste_selector_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorPasteAsShowPasteSelector,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_paste_as_show_paste_selector_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(
                value,
                EditorPasteAsShowPasteSelector::AfterPaste,
                "After paste",
            );
            ui.selectable_value(value, EditorPasteAsShowPasteSelector::Never, "Never");
        });
}

fn editor_paste_as_show_paste_selector_label(mode: EditorPasteAsShowPasteSelector) -> &'static str {
    match mode {
        EditorPasteAsShowPasteSelector::AfterPaste => "After paste",
        EditorPasteAsShowPasteSelector::Never => "Never",
    }
}

fn editor_drop_into_editor_show_drop_selector_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorDropIntoEditorShowDropSelector,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_drop_into_editor_show_drop_selector_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(
                value,
                EditorDropIntoEditorShowDropSelector::AfterDrop,
                "After drop",
            );
            ui.selectable_value(value, EditorDropIntoEditorShowDropSelector::Never, "Never");
        });
}

fn editor_drop_into_editor_show_drop_selector_label(
    mode: EditorDropIntoEditorShowDropSelector,
) -> &'static str {
    match mode {
        EditorDropIntoEditorShowDropSelector::AfterDrop => "After drop",
        EditorDropIntoEditorShowDropSelector::Never => "Never",
    }
}

fn editor_multi_cursor_modifier_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorMultiCursorModifier,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_multi_cursor_modifier_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorMultiCursorModifier::Alt, "Alt");
            ui.selectable_value(value, EditorMultiCursorModifier::CtrlCmd, "Ctrl/Cmd");
        });
}

fn editor_multi_cursor_modifier_label(mode: EditorMultiCursorModifier) -> &'static str {
    match mode {
        EditorMultiCursorModifier::Alt => "Alt",
        EditorMultiCursorModifier::CtrlCmd => "Ctrl/Cmd",
    }
}

fn editor_multi_cursor_paste_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorMultiCursorPaste,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_multi_cursor_paste_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorMultiCursorPaste::Spread, "Spread");
            ui.selectable_value(value, EditorMultiCursorPaste::Full, "Full");
        });
}

fn editor_multi_cursor_paste_label(mode: EditorMultiCursorPaste) -> &'static str {
    match mode {
        EditorMultiCursorPaste::Spread => "Spread",
        EditorMultiCursorPaste::Full => "Full",
    }
}

fn editor_mouse_middle_click_action_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorMouseMiddleClickAction,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_mouse_middle_click_action_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorMouseMiddleClickAction::Default, "Default");
            ui.selectable_value(value, EditorMouseMiddleClickAction::OpenLink, "Open link");
            ui.selectable_value(
                value,
                EditorMouseMiddleClickAction::CtrlLeftClick,
                "Ctrl left click",
            );
        });
}

fn editor_mouse_middle_click_action_label(mode: EditorMouseMiddleClickAction) -> &'static str {
    match mode {
        EditorMouseMiddleClickAction::Default => "Default",
        EditorMouseMiddleClickAction::OpenLink => "Open link",
        EditorMouseMiddleClickAction::CtrlLeftClick => "Ctrl left click",
    }
}

#[cfg(test)]
mod tests {
    use super::editor_multi_cursor_limit_drag_value;
    use eframe::egui;
    use kuroya_core::MAX_EDITOR_MULTI_CURSOR_LIMIT;

    #[test]
    fn multi_cursor_limit_render_preserves_raw_out_of_range_draft() {
        let ctx = egui::Context::default();
        let mut value = usize::MAX;

        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                editor_multi_cursor_limit_drag_value(ui, &mut value);
            });
        });

        assert_eq!(value, usize::MAX);
        assert_ne!(value, MAX_EDITOR_MULTI_CURSOR_LIMIT);
    }
}
