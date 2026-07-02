use crate::preference_panels::sections::{
    SETTINGS_TARGET_EDITOR_LANGUAGE, SettingsHighlightState, bounded_singleline_text_edit,
    bounded_singleline_text_edit_with_hint, settings_target_heading,
};
use eframe::egui;
use kuroya_core::{
    EditorInlineSuggestEditsAllowCodeShifting, EditorInlineSuggestEditsRenderSideBySide,
    EditorInlineSuggestMode, EditorInlineSuggestShowOnSuggestConflict,
    EditorInlineSuggestShowToolbar, EditorSettings, EditorSnippetSuggestions,
    EditorSuggestInsertMode, EditorSuggestPreviewMode, EditorSuggestSelection,
    EditorSuggestSelectionMode, EditorTabCompletion, MAX_INLINE_SUGGEST_MIN_SHOW_DELAY_MS,
    MAX_QUICK_SUGGESTIONS_DELAY_MS, MAX_SUGGEST_FONT_SIZE, MAX_SUGGEST_LINE_HEIGHT,
    MIN_INLINE_SUGGEST_MIN_SHOW_DELAY_MS, MIN_QUICK_SUGGESTIONS_DELAY_MS, MIN_SUGGEST_FONT_SIZE,
    MIN_SUGGEST_LINE_HEIGHT,
};

pub(super) fn render_language_settings_with_highlight(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    ui.add_space(12.0);
    settings_target_heading(
        ui,
        highlight,
        SETTINGS_TARGET_EDITOR_LANGUAGE,
        "Language Features",
    );
    egui::Grid::new("settings_editor_language_grid")
        .num_columns(2)
        .spacing([18.0, 10.0])
        .show(ui, |ui| {
            ui.label("Quick suggestions");
            ui.checkbox(&mut draft.quick_suggestions, "Suggest while typing words");
            ui.end_row();

            ui.label("Quick suggestion delay");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut draft.quick_suggestions_delay_ms)
                        .speed(10.0)
                        .range(MIN_QUICK_SUGGESTIONS_DELAY_MS..=MAX_QUICK_SUGGESTIONS_DELAY_MS),
                );
                ui.label("ms");
            })
            .response
            .on_hover_text("Delay before quick suggestions are shown while typing");
            ui.end_row();

            ui.label("Trigger characters");
            ui.checkbox(
                &mut draft.suggest_on_trigger_characters,
                "Suggest after trigger characters",
            );
            ui.end_row();

            ui.label("Accept on Enter");
            ui.checkbox(
                &mut draft.accept_suggestion_on_enter,
                "Apply selected suggestion with Enter",
            );
            ui.end_row();

            ui.label("Accept on Tab");
            ui.checkbox(
                &mut draft.accept_suggestion_on_tab,
                "Apply selected suggestion with Tab",
            );
            ui.end_row();

            ui.label("Accept on commit char");
            ui.checkbox(
                &mut draft.accept_suggestion_on_commit_character,
                "Apply suggestions when provider-defined commit characters are typed",
            );
            ui.end_row();

            ui.label("Suggest selection");
            editor_suggest_selection_combo(
                ui,
                "editor_suggest_selection",
                &mut draft.suggest_selection,
            );
            ui.end_row();

            ui.label("Suggest insert mode");
            editor_suggest_insert_mode_combo(
                ui,
                "editor_suggest_insert_mode",
                &mut draft.suggest_insert_mode,
            );
            ui.end_row();

            ui.label("Suggest font size");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut draft.suggest_font_size)
                        .speed(1.0)
                        .range(MIN_SUGGEST_FONT_SIZE..=MAX_SUGGEST_FONT_SIZE),
                );
                ui.label("px");
            })
            .response
            .on_hover_text("Use 0 to follow the editor font size");
            ui.end_row();

            ui.label("Suggest line height");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut draft.suggest_line_height)
                        .speed(1.0)
                        .range(MIN_SUGGEST_LINE_HEIGHT..=MAX_SUGGEST_LINE_HEIGHT),
                );
                ui.label("px");
            })
            .response
            .on_hover_text("Use 0 to follow the editor line height");
            ui.end_row();

            ui.label("Tab completion");
            editor_tab_completion_combo(ui, "editor_tab_completion", &mut draft.tab_completion);
            ui.end_row();

            ui.label("Snippet suggestions");
            editor_snippet_suggestions_combo(
                ui,
                "editor_snippet_suggestions",
                &mut draft.snippet_suggestions,
            );
            ui.end_row();

            ui.label("Suggest selection mode");
            editor_suggest_selection_mode_combo(
                ui,
                "editor_suggest_selection_mode",
                &mut draft.suggest_selection_mode,
            );
            ui.end_row();

            ui.label("Suggest preview mode");
            editor_suggest_preview_mode_combo(
                ui,
                "editor_suggest_preview_mode",
                &mut draft.suggest_preview_mode,
            );
            ui.end_row();

            ui.label("Suggest behavior");
            ui.vertical(|ui| {
                ui.checkbox(
                    &mut draft.suggest_filter_graceful,
                    "Graceful fuzzy filtering",
                );
                ui.checkbox(
                    &mut draft.suggest_snippets_prevent_quick_suggestions,
                    "Snippets block quick suggestions",
                );
                ui.checkbox(&mut draft.suggest_locality_bonus, "Prefer nearby words");
                ui.checkbox(
                    &mut draft.suggest_share_suggest_selections,
                    "Share remembered selections",
                );
                ui.checkbox(&mut draft.suggest_preview, "Preview suggestion edits");
                ui.checkbox(
                    &mut draft.suggest_match_on_word_start_only,
                    "Match only on word start",
                );
                ui.checkbox(&mut draft.show_unused, "Fade unused code");
                ui.checkbox(&mut draft.show_deprecated, "Strike deprecated symbols");
            });
            ui.end_row();

            ui.label("Suggest widget");
            ui.vertical(|ui| {
                ui.checkbox(&mut draft.suggest_show_icons, "Show suggestion icons");
                ui.checkbox(&mut draft.suggest_show_status_bar, "Show status bar");
                ui.checkbox(
                    &mut draft.suggest_show_inline_details,
                    "Show details inline",
                );
            });
            ui.end_row();

            ui.label("Suggest item kinds");
            ui.vertical(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut draft.suggest_show_methods, "Methods");
                    ui.checkbox(&mut draft.suggest_show_functions, "Functions");
                    ui.checkbox(&mut draft.suggest_show_constructors, "Constructors");
                    ui.checkbox(&mut draft.suggest_show_fields, "Fields");
                    ui.checkbox(&mut draft.suggest_show_variables, "Variables");
                    ui.checkbox(&mut draft.suggest_show_classes, "Classes");
                    ui.checkbox(&mut draft.suggest_show_structs, "Structs");
                    ui.checkbox(&mut draft.suggest_show_interfaces, "Interfaces");
                    ui.checkbox(&mut draft.suggest_show_modules, "Modules");
                    ui.checkbox(&mut draft.suggest_show_properties, "Properties");
                });
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut draft.suggest_show_events, "Events");
                    ui.checkbox(&mut draft.suggest_show_operators, "Operators");
                    ui.checkbox(&mut draft.suggest_show_units, "Units");
                    ui.checkbox(&mut draft.suggest_show_values, "Values");
                    ui.checkbox(&mut draft.suggest_show_constants, "Constants");
                    ui.checkbox(&mut draft.suggest_show_enums, "Enums");
                    ui.checkbox(&mut draft.suggest_show_enum_members, "Enum members");
                    ui.checkbox(&mut draft.suggest_show_keywords, "Keywords");
                    ui.checkbox(&mut draft.suggest_show_words, "Words");
                    ui.checkbox(&mut draft.suggest_show_colors, "Colors");
                });
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut draft.suggest_show_files, "Files");
                    ui.checkbox(&mut draft.suggest_show_references, "References");
                    ui.checkbox(&mut draft.suggest_show_customcolors, "Custom colors");
                    ui.checkbox(&mut draft.suggest_show_folders, "Folders");
                    ui.checkbox(&mut draft.suggest_show_type_parameters, "Type parameters");
                    ui.checkbox(&mut draft.suggest_show_snippets, "Snippets");
                    ui.checkbox(&mut draft.suggest_show_users, "Users");
                    ui.checkbox(&mut draft.suggest_show_issues, "Issues");
                    ui.checkbox(&mut draft.suggest_show_deprecated, "Deprecated");
                });
            });
            ui.end_row();

            ui.label("Inline suggestions");
            ui.checkbox(
                &mut draft.inline_suggest_enabled,
                "Show automatic inline suggestions",
            );
            ui.end_row();

            ui.label("Inline suggest mode");
            editor_inline_suggest_mode_combo(
                ui,
                "editor_inline_suggest_mode",
                &mut draft.inline_suggest_mode,
            );
            ui.end_row();

            ui.label("Inline toolbar");
            editor_inline_suggest_toolbar_combo(
                ui,
                "editor_inline_suggest_toolbar",
                &mut draft.inline_suggest_show_toolbar,
            );
            ui.end_row();

            ui.label("Inline suggest delay");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut draft.inline_suggest_min_show_delay_ms)
                        .speed(25.0)
                        .range(
                            MIN_INLINE_SUGGEST_MIN_SHOW_DELAY_MS
                                ..=MAX_INLINE_SUGGEST_MIN_SHOW_DELAY_MS,
                        ),
                );
                ui.label("ms");
            });
            ui.end_row();

            ui.label("Inline suggest font");
            bounded_singleline_text_edit_with_hint(
                ui,
                &mut draft.inline_suggest_font_family,
                220.0,
                Some("default"),
            )
            .on_hover_text("Use default to follow the editor font");
            ui.end_row();

            ui.label("Inline suggest details");
            ui.vertical(|ui| {
                ui.checkbox(
                    &mut draft.inline_suggest_syntax_highlighting_enabled,
                    "Syntax highlight inline suggestions",
                );
                ui.checkbox(
                    &mut draft.inline_suggest_suppress_suggestions,
                    "Suppress suggest widget when inline suggestions are available",
                );
                ui.checkbox(
                    &mut draft.inline_suggest_suppress_in_snippet_mode,
                    "Suppress inline suggestions in snippet mode",
                );
                ui.checkbox(&mut draft.inline_suggest_keep_on_blur, "Keep on blur");
                ui.checkbox(
                    &mut draft.inline_suggest_trigger_command_on_provider_change,
                    "Trigger command on provider change",
                );
                ui.checkbox(
                    &mut draft.inline_completions_accessibility_verbose,
                    "Verbose screen reader hint",
                );
            });
            ui.end_row();

            ui.label("Inline suggest edits");
            ui.vertical(|ui| {
                ui.checkbox(
                    &mut draft.inline_suggest_edits_enabled,
                    "Enable edit suggestions",
                );
                ui.checkbox(
                    &mut draft.inline_suggest_edits_show_collapsed,
                    "Show collapsed edit suggestions",
                );
                ui.checkbox(
                    &mut draft.inline_suggest_edits_show_long_distance_hint,
                    "Show long-distance hints",
                );
                ui.horizontal(|ui| {
                    ui.label("Code shifting");
                    editor_inline_suggest_edits_allow_code_shifting_combo(
                        ui,
                        "editor_inline_suggest_edits_allow_code_shifting",
                        &mut draft.inline_suggest_edits_allow_code_shifting,
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Side by side");
                    editor_inline_suggest_edits_render_side_by_side_combo(
                        ui,
                        "editor_inline_suggest_edits_render_side_by_side",
                        &mut draft.inline_suggest_edits_render_side_by_side,
                    );
                });
            });
            ui.end_row();

            ui.label("Inline suggest experiments");
            ui.vertical(|ui| {
                bounded_singleline_text_edit(
                    ui,
                    &mut draft.inline_suggest_experimental_suppress_inline_suggestions,
                    220.0,
                )
                .on_hover_text("Extension IDs to suppress inline suggestions for");
                ui.horizontal(|ui| {
                    ui.label("Conflict");
                    editor_inline_suggest_show_on_suggest_conflict_combo(
                        ui,
                        "editor_inline_suggest_experimental_show_on_suggest_conflict",
                        &mut draft.inline_suggest_experimental_show_on_suggest_conflict,
                    );
                });
                ui.checkbox(
                    &mut draft.inline_suggest_experimental_empty_response_information,
                    "Empty response information",
                );
            });
            ui.end_row();

            ui.label("Smart select");
            ui.vertical(|ui| {
                ui.checkbox(
                    &mut draft.smart_select_select_leading_and_trailing_whitespace,
                    "Select leading and trailing whitespace",
                );
                ui.checkbox(&mut draft.smart_select_select_subwords, "Select subwords");
            });
            ui.end_row();
        });
}

fn editor_suggest_selection_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorSuggestSelection,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_suggest_selection_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorSuggestSelection::First, "First");
            ui.selectable_value(value, EditorSuggestSelection::RecentlyUsed, "Recently used");
            ui.selectable_value(
                value,
                EditorSuggestSelection::RecentlyUsedByPrefix,
                "Recently used by prefix",
            );
        });
}

fn editor_suggest_selection_label(mode: EditorSuggestSelection) -> &'static str {
    match mode {
        EditorSuggestSelection::First => "First",
        EditorSuggestSelection::RecentlyUsed => "Recently used",
        EditorSuggestSelection::RecentlyUsedByPrefix => "Recently used by prefix",
    }
}

fn editor_suggest_insert_mode_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorSuggestInsertMode,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_suggest_insert_mode_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorSuggestInsertMode::Insert, "Insert");
            ui.selectable_value(value, EditorSuggestInsertMode::Replace, "Replace");
        });
}

fn editor_suggest_insert_mode_label(mode: EditorSuggestInsertMode) -> &'static str {
    match mode {
        EditorSuggestInsertMode::Insert => "Insert",
        EditorSuggestInsertMode::Replace => "Replace",
    }
}

fn editor_suggest_selection_mode_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorSuggestSelectionMode,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_suggest_selection_mode_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorSuggestSelectionMode::Always, "Always");
            ui.selectable_value(value, EditorSuggestSelectionMode::Never, "Never");
            ui.selectable_value(
                value,
                EditorSuggestSelectionMode::WhenTriggerCharacter,
                "Trigger character",
            );
            ui.selectable_value(
                value,
                EditorSuggestSelectionMode::WhenQuickSuggestion,
                "Quick suggestion",
            );
        });
}

fn editor_suggest_selection_mode_label(mode: EditorSuggestSelectionMode) -> &'static str {
    match mode {
        EditorSuggestSelectionMode::Always => "Always",
        EditorSuggestSelectionMode::Never => "Never",
        EditorSuggestSelectionMode::WhenTriggerCharacter => "Trigger character",
        EditorSuggestSelectionMode::WhenQuickSuggestion => "Quick suggestion",
    }
}

fn editor_suggest_preview_mode_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorSuggestPreviewMode,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_suggest_preview_mode_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorSuggestPreviewMode::Prefix, "Prefix");
            ui.selectable_value(value, EditorSuggestPreviewMode::Subword, "Subword");
            ui.selectable_value(
                value,
                EditorSuggestPreviewMode::SubwordSmart,
                "Subword smart",
            );
        });
}

fn editor_suggest_preview_mode_label(mode: EditorSuggestPreviewMode) -> &'static str {
    match mode {
        EditorSuggestPreviewMode::Prefix => "Prefix",
        EditorSuggestPreviewMode::Subword => "Subword",
        EditorSuggestPreviewMode::SubwordSmart => "Subword smart",
    }
}

fn editor_tab_completion_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorTabCompletion,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_tab_completion_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorTabCompletion::On, "On");
            ui.selectable_value(value, EditorTabCompletion::Off, "Off");
            ui.selectable_value(value, EditorTabCompletion::OnlySnippets, "Only snippets");
        });
}

fn editor_tab_completion_label(mode: EditorTabCompletion) -> &'static str {
    match mode {
        EditorTabCompletion::On => "On",
        EditorTabCompletion::Off => "Off",
        EditorTabCompletion::OnlySnippets => "Only snippets",
    }
}

fn editor_snippet_suggestions_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorSnippetSuggestions,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_snippet_suggestions_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorSnippetSuggestions::Top, "Top");
            ui.selectable_value(value, EditorSnippetSuggestions::Bottom, "Bottom");
            ui.selectable_value(value, EditorSnippetSuggestions::Inline, "Inline");
            ui.selectable_value(value, EditorSnippetSuggestions::None, "None");
        });
}

fn editor_snippet_suggestions_label(mode: EditorSnippetSuggestions) -> &'static str {
    match mode {
        EditorSnippetSuggestions::Top => "Top",
        EditorSnippetSuggestions::Bottom => "Bottom",
        EditorSnippetSuggestions::Inline => "Inline",
        EditorSnippetSuggestions::None => "None",
    }
}

fn editor_inline_suggest_mode_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorInlineSuggestMode,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_inline_suggest_mode_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorInlineSuggestMode::Prefix, "Prefix");
            ui.selectable_value(value, EditorInlineSuggestMode::Subword, "Subword");
            ui.selectable_value(
                value,
                EditorInlineSuggestMode::SubwordSmart,
                "Subword smart",
            );
        });
}

fn editor_inline_suggest_mode_label(mode: EditorInlineSuggestMode) -> &'static str {
    match mode {
        EditorInlineSuggestMode::Prefix => "Prefix",
        EditorInlineSuggestMode::Subword => "Subword",
        EditorInlineSuggestMode::SubwordSmart => "Subword smart",
    }
}

fn editor_inline_suggest_toolbar_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorInlineSuggestShowToolbar,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_inline_suggest_toolbar_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorInlineSuggestShowToolbar::Always, "Always");
            ui.selectable_value(value, EditorInlineSuggestShowToolbar::OnHover, "On hover");
            ui.selectable_value(value, EditorInlineSuggestShowToolbar::Never, "Never");
        });
}

fn editor_inline_suggest_toolbar_label(mode: EditorInlineSuggestShowToolbar) -> &'static str {
    match mode {
        EditorInlineSuggestShowToolbar::Always => "Always",
        EditorInlineSuggestShowToolbar::OnHover => "On hover",
        EditorInlineSuggestShowToolbar::Never => "Never",
    }
}

fn editor_inline_suggest_edits_allow_code_shifting_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorInlineSuggestEditsAllowCodeShifting,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_inline_suggest_edits_allow_code_shifting_label(
            *value,
        ))
        .show_ui(ui, |ui| {
            ui.selectable_value(
                value,
                EditorInlineSuggestEditsAllowCodeShifting::Always,
                "Always",
            );
            ui.selectable_value(
                value,
                EditorInlineSuggestEditsAllowCodeShifting::Horizontal,
                "Horizontal",
            );
            ui.selectable_value(
                value,
                EditorInlineSuggestEditsAllowCodeShifting::Never,
                "Never",
            );
        });
}

fn editor_inline_suggest_edits_allow_code_shifting_label(
    mode: EditorInlineSuggestEditsAllowCodeShifting,
) -> &'static str {
    match mode {
        EditorInlineSuggestEditsAllowCodeShifting::Always => "Always",
        EditorInlineSuggestEditsAllowCodeShifting::Horizontal => "Horizontal",
        EditorInlineSuggestEditsAllowCodeShifting::Never => "Never",
    }
}

fn editor_inline_suggest_edits_render_side_by_side_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorInlineSuggestEditsRenderSideBySide,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_inline_suggest_edits_render_side_by_side_label(
            *value,
        ))
        .show_ui(ui, |ui| {
            ui.selectable_value(
                value,
                EditorInlineSuggestEditsRenderSideBySide::Auto,
                "Auto",
            );
            ui.selectable_value(
                value,
                EditorInlineSuggestEditsRenderSideBySide::Never,
                "Never",
            );
        });
}

fn editor_inline_suggest_edits_render_side_by_side_label(
    mode: EditorInlineSuggestEditsRenderSideBySide,
) -> &'static str {
    match mode {
        EditorInlineSuggestEditsRenderSideBySide::Auto => "Auto",
        EditorInlineSuggestEditsRenderSideBySide::Never => "Never",
    }
}

fn editor_inline_suggest_show_on_suggest_conflict_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorInlineSuggestShowOnSuggestConflict,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_inline_suggest_show_on_suggest_conflict_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(
                value,
                EditorInlineSuggestShowOnSuggestConflict::Always,
                "Always",
            );
            ui.selectable_value(
                value,
                EditorInlineSuggestShowOnSuggestConflict::Never,
                "Never",
            );
            ui.selectable_value(
                value,
                EditorInlineSuggestShowOnSuggestConflict::WhenSuggestListIsIncomplete,
                "Incomplete suggest list",
            );
        });
}

fn editor_inline_suggest_show_on_suggest_conflict_label(
    mode: EditorInlineSuggestShowOnSuggestConflict,
) -> &'static str {
    match mode {
        EditorInlineSuggestShowOnSuggestConflict::Always => "Always",
        EditorInlineSuggestShowOnSuggestConflict::Never => "Never",
        EditorInlineSuggestShowOnSuggestConflict::WhenSuggestListIsIncomplete => {
            "Incomplete suggest list"
        }
    }
}
