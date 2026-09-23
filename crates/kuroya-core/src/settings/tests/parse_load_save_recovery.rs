use super::*;

mod recovery;

#[test]
fn partial_settings_toml_uses_defaults() {
    let settings: EditorSettings = toml::from_str("font_size = 15.0\n[theme]\nname = \"Custom\"\n")
        .expect("partial settings should load");
    assert_eq!(settings.schema_version, SETTINGS_SCHEMA_VERSION);
    assert_eq!(settings.font_size, 15.0);
    assert_eq!(settings.ui_font_size, 13.0);
    assert_eq!(settings.editor_font_path, None);
    assert_eq!(settings.ui_font_path, None);
    assert!(!settings.background_image_enabled);
    assert_eq!(settings.background_image_path, None);
    assert_eq!(
        settings.background_image_dim,
        DEFAULT_EDITOR_BACKGROUND_IMAGE_DIM
    );
    assert_eq!(
        settings.background_image_fit,
        EditorBackgroundImageFit::Cover
    );
    assert_eq!(
        settings.background_image_position,
        EditorBackgroundImagePosition::Center
    );
    assert!(settings.background_image_loop);
    assert_eq!(settings.font_family, DEFAULT_EDITOR_FONT_FAMILY);
    assert_eq!(settings.font_weight, DEFAULT_EDITOR_FONT_WEIGHT);
    assert_eq!(settings.font_ligatures, DEFAULT_EDITOR_FONT_LIGATURES);
    assert_eq!(settings.font_variations, DEFAULT_EDITOR_FONT_VARIATIONS);
    assert_eq!(settings.letter_spacing, DEFAULT_EDITOR_LETTER_SPACING);
    assert!(!settings.automatic_layout);
    assert!(!settings.disable_layer_hinting);
    assert!(!settings.disable_monospace_optimizations);
    assert!(settings.extra_editor_class_name.is_empty());
    assert!(settings.allow_variable_line_heights);
    assert!(settings.allow_variable_fonts);
    assert!(!settings.allow_variable_fonts_in_accessibility_mode);
    assert_eq!(
        settings.accessibility_support,
        EditorAccessibilitySupport::Auto
    );
    assert_eq!(
        settings.accessibility_page_size,
        DEFAULT_EDITOR_ACCESSIBILITY_PAGE_SIZE
    );
    assert_eq!(settings.aria_label, DEFAULT_EDITOR_ARIA_LABEL);
    assert!(!settings.aria_required);
    assert!(settings.screen_reader_announce_inline_suggestion);
    assert_eq!(settings.tab_index, DEFAULT_EDITOR_TAB_INDEX);
    assert!(!settings.read_only);
    assert!(settings.read_only_message.is_empty());
    assert!(!settings.dom_read_only);
    assert!(settings.edit_context);
    assert!(!settings.render_rich_screen_reader_content);
    assert!(!settings.trim_whitespace_on_delete);
    assert_eq!(
        settings.unusual_line_terminators,
        EditorUnusualLineTerminators::Prompt
    );
    assert!(settings.use_shadow_dom);
    assert!(settings.use_tab_stops);
    assert!(!settings.fixed_overflow_widgets);
    assert!(settings.allow_overflow);
    assert_eq!(settings.tab_width, 4);
    assert!(settings.insert_spaces);
    assert!(settings.detect_indentation);
    assert_eq!(settings.word_separators, DEFAULT_WORD_SEPARATORS);
    assert!(settings.word_segmenter_locales.is_empty());
    assert!(settings.auto_indent);
    assert!(settings.auto_closing_brackets);
    assert!(settings.auto_closing_quotes);
    assert_eq!(
        settings.experimental_gpu_acceleration,
        EditorExperimentalGpuAcceleration::Off
    );
    assert_eq!(
        settings.experimental_whitespace_rendering,
        EditorExperimentalWhitespaceRendering::Svg
    );
    assert_eq!(
        settings.auto_closing_comments,
        EditorAutoClosingStrategy::LanguageDefined
    );
    assert_eq!(
        settings.auto_closing_delete,
        EditorAutoClosingEditStrategy::Auto
    );
    assert_eq!(
        settings.auto_closing_overtype,
        EditorAutoClosingEditStrategy::Auto
    );
    assert!(settings.auto_surround);
    assert!(!settings.auto_indent_on_paste);
    assert!(settings.auto_indent_on_paste_within_string);
    assert!(!settings.sticky_tab_stops);
    assert!(!settings.linked_editing);
    assert!(!settings.rename_on_type);
    assert!(!settings.tab_focus_mode);
    assert!(!settings.vim_keybindings);
    assert!(!settings.quick_suggestions);
    assert_eq!(
        settings.quick_suggestions_delay_ms,
        DEFAULT_QUICK_SUGGESTIONS_DELAY_MS
    );
    assert!(settings.suggest_on_trigger_characters);
    assert!(settings.accept_suggestion_on_enter);
    assert!(!settings.accept_suggestion_on_tab);
    assert!(settings.accept_suggestion_on_commit_character);
    assert_eq!(settings.suggest_selection, EditorSuggestSelection::First);
    assert_eq!(
        settings.suggest_insert_mode,
        EditorSuggestInsertMode::Insert
    );
    assert!(settings.suggest_filter_graceful);
    assert!(!settings.suggest_snippets_prevent_quick_suggestions);
    assert!(!settings.suggest_locality_bonus);
    assert!(!settings.suggest_share_suggest_selections);
    assert_eq!(
        settings.suggest_selection_mode,
        EditorSuggestSelectionMode::Always
    );
    assert!(settings.suggest_show_icons);
    assert!(!settings.suggest_show_status_bar);
    assert!(!settings.suggest_preview);
    assert_eq!(
        settings.suggest_preview_mode,
        EditorSuggestPreviewMode::SubwordSmart
    );
    assert!(settings.suggest_show_inline_details);
    assert!(settings.suggest_show_methods);
    assert!(settings.suggest_show_functions);
    assert!(settings.suggest_show_constructors);
    assert!(settings.suggest_show_deprecated);
    assert!(settings.suggest_show_fields);
    assert!(settings.suggest_show_variables);
    assert!(settings.suggest_show_classes);
    assert!(settings.suggest_show_structs);
    assert!(settings.suggest_show_interfaces);
    assert!(settings.suggest_show_modules);
    assert!(settings.suggest_show_properties);
    assert!(settings.suggest_show_events);
    assert!(settings.suggest_show_operators);
    assert!(settings.suggest_show_units);
    assert!(settings.suggest_show_values);
    assert!(settings.suggest_show_constants);
    assert!(settings.suggest_show_enums);
    assert!(settings.suggest_show_enum_members);
    assert!(settings.suggest_show_keywords);
    assert!(settings.suggest_show_words);
    assert!(settings.suggest_show_colors);
    assert!(settings.suggest_show_files);
    assert!(settings.suggest_show_references);
    assert!(settings.suggest_show_customcolors);
    assert!(settings.suggest_show_folders);
    assert!(settings.suggest_show_type_parameters);
    assert!(settings.suggest_show_snippets);
    assert!(settings.suggest_show_users);
    assert!(settings.suggest_show_issues);
    assert!(settings.suggest_match_on_word_start_only);
    assert_eq!(settings.suggest_font_size, DEFAULT_SUGGEST_FONT_SIZE);
    assert_eq!(settings.suggest_line_height, DEFAULT_SUGGEST_LINE_HEIGHT);
    assert_eq!(settings.tab_completion, EditorTabCompletion::Off);
    assert_eq!(
        settings.snippet_suggestions,
        EditorSnippetSuggestions::Inline
    );
    assert!(settings.hover_enabled);
    assert_eq!(settings.hover_delay_ms, DEFAULT_HOVER_DELAY_MS);
    assert_eq!(
        settings.hover_hiding_delay_ms,
        DEFAULT_HOVER_HIDING_DELAY_MS
    );
    assert!(settings.hover_sticky);
    assert!(settings.hover_above);
    assert!(settings.hover_show_long_line_warning);
    assert!(settings.inline_suggest_enabled);
    assert_eq!(
        settings.inline_suggest_mode,
        EditorInlineSuggestMode::SubwordSmart
    );
    assert_eq!(
        settings.inline_suggest_show_toolbar,
        EditorInlineSuggestShowToolbar::OnHover
    );
    assert!(!settings.inline_suggest_keep_on_blur);
    assert_eq!(
        settings.inline_suggest_font_family,
        DEFAULT_INLINE_SUGGEST_FONT_FAMILY
    );
    assert!(settings.inline_suggest_syntax_highlighting_enabled);
    assert!(!settings.inline_suggest_suppress_suggestions);
    assert!(settings.inline_suggest_suppress_in_snippet_mode);
    assert_eq!(
        settings.inline_suggest_min_show_delay_ms,
        DEFAULT_INLINE_SUGGEST_MIN_SHOW_DELAY_MS
    );
    assert!(settings.inline_suggest_edits_enabled);
    assert!(!settings.inline_suggest_edits_show_collapsed);
    assert_eq!(
        settings.inline_suggest_edits_render_side_by_side,
        EditorInlineSuggestEditsRenderSideBySide::Auto
    );
    assert_eq!(
        settings.inline_suggest_edits_allow_code_shifting,
        EditorInlineSuggestEditsAllowCodeShifting::Always
    );
    assert!(settings.inline_suggest_edits_show_long_distance_hint);
    assert!(!settings.inline_suggest_trigger_command_on_provider_change);
    assert!(
        settings
            .inline_suggest_experimental_suppress_inline_suggestions
            .is_empty()
    );
    assert_eq!(
        settings.inline_suggest_experimental_show_on_suggest_conflict,
        EditorInlineSuggestShowOnSuggestConflict::Never
    );
    assert!(settings.inline_suggest_experimental_empty_response_information);
    assert!(!settings.inline_completions_accessibility_verbose);
    assert_eq!(settings.lightbulb, EditorLightbulbMode::OnCode);
    assert_eq!(
        settings.render_validation_decorations,
        EditorRenderValidationDecorations::Editable
    );
    assert!(settings.document_highlights_enabled);
    assert!(settings.code_lens);
    assert_eq!(
        settings.code_lens_font_family,
        DEFAULT_EDITOR_CODE_LENS_FONT_FAMILY
    );
    assert_eq!(
        settings.code_lens_font_size,
        DEFAULT_EDITOR_CODE_LENS_FONT_SIZE
    );
    assert_eq!(
        settings.goto_location_multiple_definitions,
        EditorGotoLocationMultiple::Peek
    );
    assert_eq!(
        settings.goto_location_multiple_type_definitions,
        EditorGotoLocationMultiple::Peek
    );
    assert_eq!(
        settings.goto_location_multiple_declarations,
        EditorGotoLocationMultiple::Peek
    );
    assert_eq!(
        settings.goto_location_multiple_implementations,
        EditorGotoLocationMultiple::Peek
    );
    assert_eq!(
        settings.goto_location_multiple_references,
        EditorGotoLocationMultiple::Peek
    );
    assert_eq!(
        settings.goto_location_multiple_tests,
        EditorGotoLocationMultiple::Peek
    );
    assert_eq!(
        settings.goto_location_alternative_definition_command,
        DEFAULT_GOTO_LOCATION_ALTERNATIVE_DEFINITION_COMMAND
    );
    assert_eq!(
        settings.goto_location_alternative_type_definition_command,
        DEFAULT_GOTO_LOCATION_ALTERNATIVE_TYPE_DEFINITION_COMMAND
    );
    assert_eq!(
        settings.goto_location_alternative_declaration_command,
        DEFAULT_GOTO_LOCATION_ALTERNATIVE_DECLARATION_COMMAND
    );
    assert_eq!(
        settings.goto_location_alternative_implementation_command,
        DEFAULT_GOTO_LOCATION_ALTERNATIVE_IMPLEMENTATION_COMMAND
    );
    assert_eq!(
        settings.goto_location_alternative_reference_command,
        DEFAULT_GOTO_LOCATION_ALTERNATIVE_REFERENCE_COMMAND
    );
    assert_eq!(
        settings.goto_location_alternative_tests_command,
        DEFAULT_GOTO_LOCATION_ALTERNATIVE_TESTS_COMMAND
    );
    assert_eq!(
        settings.peek_widget_default_focus,
        EditorPeekWidgetDefaultFocus::Tree
    );
    assert_eq!(settings.placeholder, DEFAULT_EDITOR_PLACEHOLDER);
    assert!(!settings.definition_link_opens_in_peek);
    assert!(settings.inlay_hints);
    assert_eq!(
        settings.inlay_hints_font_family,
        DEFAULT_EDITOR_INLAY_HINTS_FONT_FAMILY
    );
    assert_eq!(
        settings.inlay_hints_font_size,
        DEFAULT_EDITOR_INLAY_HINTS_FONT_SIZE
    );
    assert!(!settings.inlay_hints_padding);
    assert_eq!(
        settings.inlay_hints_maximum_length,
        DEFAULT_EDITOR_INLAY_HINTS_MAXIMUM_LENGTH
    );
    assert!(settings.parameter_hints_enabled);
    assert!(settings.parameter_hints_on_trigger_characters);
    assert!(settings.parameter_hints_cycle);
    assert!(settings.comments_insert_space);
    assert!(settings.comments_ignore_empty_lines);
    assert!(!settings.format_on_save);
    assert!(!settings.format_on_type);
    assert!(!settings.format_on_paste);
    assert!(settings.paste_as_enabled);
    assert_eq!(
        settings.paste_as_show_paste_selector,
        EditorPasteAsShowPasteSelector::AfterPaste
    );

    assert!(!settings.autosave);
    assert_eq!(settings.autosave_mode, EditorAutoSaveMode::AfterDelay);
    assert_eq!(settings.effective_autosave_mode(), EditorAutoSaveMode::Off);
    assert_eq!(settings.autosave_delay_ms, DEFAULT_AUTOSAVE_DELAY_MS);
    assert!(settings.scroll_beyond_last_line);
    assert_eq!(
        settings.scroll_beyond_last_column,
        DEFAULT_EDITOR_SCROLL_BEYOND_LAST_COLUMN
    );
    assert!(!settings.scroll_on_middle_click);
    assert!(settings.scroll_predominant_axis);
    assert!(!settings.inertial_scroll);
    assert_eq!(
        settings.mouse_wheel_scroll_sensitivity,
        DEFAULT_EDITOR_MOUSE_WHEEL_SCROLL_SENSITIVITY
    );
    assert_eq!(
        settings.fast_scroll_sensitivity,
        DEFAULT_EDITOR_FAST_SCROLL_SENSITIVITY
    );
    assert!(!settings.mouse_wheel_zoom);
    assert_eq!(
        settings.scrollbar_vertical,
        EditorScrollbarVisibility::Hidden
    );
    assert_eq!(
        settings.scrollbar_horizontal,
        EditorScrollbarVisibility::Hidden
    );
    assert_eq!(
        settings.scrollbar_vertical_scrollbar_size,
        DEFAULT_EDITOR_SCROLLBAR_VERTICAL_SCROLLBAR_SIZE
    );
    assert_eq!(
        settings.scrollbar_horizontal_scrollbar_size,
        DEFAULT_EDITOR_SCROLLBAR_HORIZONTAL_SCROLLBAR_SIZE
    );
    assert!(!settings.scrollbar_ignore_horizontal_scrollbar_in_content_height);
    assert_eq!(
        settings.explorer_scrollbar,
        EditorScrollbarVisibility::Hidden
    );
    assert_eq!(settings.padding_top, DEFAULT_EDITOR_PADDING_TOP);
    assert_eq!(settings.padding_bottom, DEFAULT_EDITOR_PADDING_BOTTOM);
    assert!(settings.links);
    assert!(settings.show_unused);
    assert!(settings.show_deprecated);
    assert!(settings.contextmenu);
    assert!(settings.color_decorators);
    assert_eq!(
        settings.color_decorators_activated_on,
        EditorColorDecoratorsActivatedOn::ClickAndHover
    );
    assert_eq!(
        settings.color_decorators_limit,
        DEFAULT_EDITOR_COLOR_DECORATORS_LIMIT
    );
    assert_eq!(
        settings.default_color_decorators,
        EditorDefaultColorDecorators::Auto
    );
    assert!(settings.sticky_scroll);
    assert_eq!(
        settings.sticky_scroll_max_line_count,
        DEFAULT_EDITOR_STICKY_SCROLL_MAX_LINE_COUNT
    );
    assert_eq!(
        settings.sticky_scroll_default_model,
        EditorStickyScrollDefaultModel::default()
    );
    assert!(settings.sticky_scroll_scroll_with_editor);
    assert_eq!(settings.line_height, DEFAULT_EDITOR_LINE_HEIGHT);
    assert!(!settings.minimap);
    assert_eq!(settings.minimap_side, EditorMinimapSide::default());
    assert_eq!(settings.minimap_autohide, EditorMinimapAutohide::default());
    assert_eq!(settings.minimap_size, EditorMinimapSize::default());
    assert_eq!(
        settings.minimap_show_slider,
        EditorMinimapShowSlider::default()
    );
    assert_eq!(settings.minimap_scale, DEFAULT_EDITOR_MINIMAP_SCALE);
    assert!(settings.minimap_render_characters);
    assert_eq!(
        settings.minimap_max_column,
        DEFAULT_EDITOR_MINIMAP_MAX_COLUMN
    );
    assert!(settings.minimap_show_region_section_headers);
    assert!(settings.minimap_show_mark_section_headers);
    assert_eq!(
        settings.minimap_mark_section_header_regex,
        DEFAULT_EDITOR_MINIMAP_MARK_SECTION_HEADER_REGEX
    );
    assert_eq!(
        settings.minimap_section_header_font_size,
        DEFAULT_EDITOR_MINIMAP_SECTION_HEADER_FONT_SIZE
    );
    assert_eq!(
        settings.minimap_section_header_letter_spacing,
        DEFAULT_EDITOR_MINIMAP_SECTION_HEADER_LETTER_SPACING
    );
    assert_eq!(
        settings.multi_cursor_modifier,
        EditorMultiCursorModifier::default()
    );
    assert!(settings.multi_cursor_merge_overlapping);
    assert_eq!(settings.multi_cursor_paste, EditorMultiCursorPaste::Spread);
    assert_eq!(
        settings.multi_cursor_limit,
        DEFAULT_EDITOR_MULTI_CURSOR_LIMIT
    );
    assert!(!settings.column_selection);
    assert_eq!(
        settings.mouse_middle_click_action,
        EditorMouseMiddleClickAction::Default
    );
    assert!(settings.empty_selection_clipboard);
    assert!(settings.selection_clipboard);
    assert!(settings.copy_with_syntax_highlighting);
    assert!(settings.double_click_selects_block);
    assert!(settings.drag_and_drop);
    assert!(settings.drop_into_editor_enabled);
    assert_eq!(
        settings.drop_into_editor_show_drop_selector,
        EditorDropIntoEditorShowDropSelector::AfterDrop
    );
    assert!(settings.glyph_margin);
    assert_eq!(settings.ruler_column, DEFAULT_EDITOR_RULER_COLUMN);
    assert!(settings.overview_ruler_border);
    assert_eq!(
        settings.overview_ruler_lanes,
        DEFAULT_EDITOR_OVERVIEW_RULER_LANES
    );
    assert!(!settings.hide_cursor_in_overview_ruler);
    assert!(settings.status_bar_visible);
    assert!(!settings.devtools_verbose_logging);
    assert!(!settings.devtools_profiling_enabled);
    assert_eq!(settings.window_zoom_level, DEFAULT_WINDOW_ZOOM_LEVEL);
    assert_eq!(settings.line_numbers, EditorLineNumbers::default());
    assert_eq!(
        settings.line_decorations_width,
        EditorLineDecorationsWidth::default()
    );
    assert_eq!(
        settings.line_numbers_min_chars,
        DEFAULT_EDITOR_LINE_NUMBERS_MIN_CHARS
    );
    assert_eq!(
        settings.select_on_line_numbers,
        DEFAULT_EDITOR_SELECT_ON_LINE_NUMBERS
    );
    assert_eq!(settings.word_wrap, EditorWordWrap::Off);
    assert_eq!(
        settings.word_wrap_override1,
        EditorWordWrapOverride::Inherit
    );
    assert_eq!(
        settings.word_wrap_override2,
        EditorWordWrapOverride::Inherit
    );
    assert_eq!(
        settings.word_wrap_break_after_characters,
        DEFAULT_EDITOR_WORD_WRAP_BREAK_AFTER_CHARACTERS
    );
    assert_eq!(
        settings.word_wrap_break_before_characters,
        DEFAULT_EDITOR_WORD_WRAP_BREAK_BEFORE_CHARACTERS
    );
    assert_eq!(settings.word_wrap_column, DEFAULT_EDITOR_WORD_WRAP_COLUMN);
    assert_eq!(settings.wrapping_indent, EditorWrappingIndent::Same);
    assert_eq!(settings.wrapping_strategy, EditorWrappingStrategy::Simple);
    assert!(!settings.wrap_on_escaped_line_feeds);
    assert_eq!(settings.word_break, EditorWordBreak::Normal);
    assert_eq!(
        settings.reveal_horizontal_right_padding,
        DEFAULT_EDITOR_REVEAL_HORIZONTAL_RIGHT_PADDING
    );
    assert!(settings.rounded_selection);
    assert_eq!(
        settings.stop_rendering_line_after,
        DEFAULT_EDITOR_STOP_RENDERING_LINE_AFTER
    );
    assert_eq!(
        settings.render_whitespace,
        EditorRenderWhitespace::default()
    );
    assert_eq!(
        settings.render_final_newline,
        EditorRenderFinalNewline::default()
    );
    assert!(!settings.render_control_characters);
    assert!(settings.unicode_highlight_ambiguous_characters);
    assert!(settings.unicode_highlight_invisible_characters);
    assert_eq!(
        settings.unicode_highlight_non_basic_ascii,
        EditorUnicodeHighlightNonBasicAscii::InUntrustedWorkspace
    );
    assert_eq!(
        settings.unicode_highlight_include_comments,
        EditorUnicodeHighlightScope::InUntrustedWorkspace
    );
    assert_eq!(
        settings.unicode_highlight_include_strings,
        EditorUnicodeHighlightScope::On
    );
    assert!(settings.unicode_highlight_allowed_characters.is_empty());
    assert_eq!(
        settings.unicode_highlight_allowed_locales,
        BTreeMap::from([("_os".to_owned(), true), ("_vscode".to_owned(), true)])
    );
    assert_eq!(
        settings.render_line_highlight,
        EditorRenderLineHighlight::default()
    );
    assert!(!settings.render_line_highlight_only_when_focus);
    assert!(settings.smart_select_select_leading_and_trailing_whitespace);
    assert!(settings.smart_select_select_subwords);
    assert_eq!(
        settings.find_seed_search_string_from_selection,
        DEFAULT_EDITOR_FIND_SEED_SEARCH_STRING_FROM_SELECTION
    );
    assert_eq!(
        settings.find_auto_find_in_selection,
        DEFAULT_EDITOR_FIND_AUTO_FIND_IN_SELECTION
    );
    assert_eq!(settings.find_on_type, DEFAULT_EDITOR_FIND_ON_TYPE);
    assert_eq!(
        settings.find_cursor_move_on_type,
        DEFAULT_EDITOR_FIND_CURSOR_MOVE_ON_TYPE
    );
    assert_eq!(settings.find_loop, DEFAULT_EDITOR_FIND_LOOP);
    assert_eq!(
        settings.find_close_on_result,
        DEFAULT_EDITOR_FIND_CLOSE_ON_RESULT
    );
    assert_eq!(
        settings.find_global_find_clipboard,
        DEFAULT_EDITOR_FIND_GLOBAL_FIND_CLIPBOARD
    );
    assert_eq!(
        settings.find_add_extra_space_on_top,
        DEFAULT_EDITOR_FIND_ADD_EXTRA_SPACE_ON_TOP
    );
    assert_eq!(settings.find_history, EditorFindHistory::Workspace);
    assert_eq!(settings.find_replace_history, EditorFindHistory::Workspace);
    assert!(settings.diff_ignore_trim_whitespace);
    assert_eq!(settings.diff_algorithm, DiffAlgorithm::Advanced);
    assert!(settings.diff_render_side_by_side);
    assert!(settings.diff_enable_split_view_resizing);
    assert_eq!(
        settings.diff_split_view_default_ratio,
        DEFAULT_DIFF_SPLIT_VIEW_DEFAULT_RATIO
    );
    assert_eq!(
        settings.diff_render_side_by_side_inline_breakpoint,
        DEFAULT_DIFF_RENDER_SIDE_BY_SIDE_INLINE_BREAKPOINT
    );
    assert!(settings.diff_use_inline_view_when_space_is_limited);
    assert!(!settings.diff_compact_mode);
    assert!(!settings.diff_original_editable);
    assert!(!settings.diff_code_lens);
    assert!(!settings.diff_accessibility_verbose);
    assert!(!settings.diff_hide_unchanged_regions);
    assert_eq!(settings.diff_context_lines, DEFAULT_DIFF_CONTEXT_LINES);
    assert_eq!(
        settings.diff_hide_unchanged_regions_minimum_line_count,
        DEFAULT_DIFF_HIDE_UNCHANGED_REGIONS_MINIMUM_LINE_COUNT
    );
    assert_eq!(
        settings.diff_hide_unchanged_regions_reveal_line_count,
        DEFAULT_DIFF_HIDE_UNCHANGED_REGIONS_REVEAL_LINE_COUNT
    );
    assert_eq!(
        settings.diff_max_computation_time_ms,
        DEFAULT_DIFF_MAX_COMPUTATION_TIME_MS
    );
    assert_eq!(
        settings.diff_max_file_size_mb,
        DEFAULT_DIFF_MAX_FILE_SIZE_MB
    );
    assert!(settings.diff_render_gutter_menu);
    assert!(settings.diff_render_indicators);
    assert!(settings.diff_render_margin_revert_icon);
    assert!(settings.diff_render_overview_ruler);
    assert!(!settings.diff_experimental_show_moves);
    assert!(settings.diff_experimental_show_empty_decorations);
    assert!(!settings.diff_experimental_use_true_inline_view);
    assert_eq!(settings.diff_word_wrap, DiffWordWrap::Inherit);
    assert!(!settings.diff_only_show_accessible_viewer);
    assert!(!settings.diff_is_in_embedded_editor);
    assert!(settings.git_enabled);
    assert_eq!(settings.git_add_ai_co_author, GitAddAiCoAuthor::Off);
    assert!(!settings.git_allow_force_push);
    assert!(!settings.git_allow_no_verify_commit);
    assert_eq!(
        settings.git_auto_repository_detection,
        GitAutoRepositoryDetection::True
    );
    assert_eq!(settings.git_autofetch, GitAutoFetch::False);
    assert_eq!(settings.git_autofetch_period, DEFAULT_GIT_AUTOFETCH_PERIOD);
    assert!(settings.git_autorefresh);
    assert!(!settings.git_auto_stash);
    assert!(settings.git_commands_to_log.is_empty());
    assert!(settings.git_confirm_force_push);
    assert!(settings.git_confirm_no_verify_commit);
    assert!(settings.git_confirm_sync);
    assert!(!settings.git_ignore_limit_warning);
    assert!(!settings.git_ignore_submodules);
    assert!(settings.git_ignored_repositories.is_empty());
    assert_eq!(
        settings.git_repository_scan_ignored_folders,
        vec!["node_modules".to_owned()]
    );
    assert_eq!(
        settings.git_open_repository_in_parent_folders,
        GitOpenRepositoryInParentFolders::Prompt
    );
    assert!(settings.git_detect_submodules);
    assert_eq!(
        settings.git_detect_submodules_limit,
        DEFAULT_GIT_DETECT_SUBMODULES_LIMIT
    );
    assert_eq!(
        settings.git_repository_scan_max_depth,
        DEFAULT_GIT_REPOSITORY_SCAN_MAX_DEPTH
    );
    assert!(!settings.git_detect_worktrees);
    assert_eq!(
        settings.git_detect_worktrees_limit,
        DEFAULT_GIT_DETECT_WORKTREES_LIMIT
    );
    assert!(settings.git_discard_untracked_changes_to_trash);
    assert!(!settings.git_diagnostics_commit_hook_enabled);
    assert_eq!(
        settings.git_diagnostics_commit_hook_sources.get("*"),
        Some(&"error".to_owned())
    );
    assert!(!settings.git_enable_commit_signing);
    assert!(settings.git_enable_status_bar_sync);
    assert!(!settings.git_fetch_on_pull);
    assert!(!settings.git_follow_tags_when_sync);
    assert!(!settings.git_ignore_legacy_warning);
    assert!(!settings.git_ignore_missing_git_warning);
    assert!(!settings.git_ignore_rebase_warning);
    assert!(!settings.git_ignore_windows_git27_warning);
    assert!(!settings.git_merge_editor);
    assert_eq!(settings.git_open_after_clone, GitOpenAfterClone::Prompt);
    assert!(settings.git_optimistic_update);
    assert!(settings.git_path.is_empty());
    assert_eq!(settings.git_post_commit_command, GitPostCommitCommand::None);
    assert!(!settings.git_prune_on_fetch);
    assert!(!settings.git_pull_before_checkout);
    assert!(settings.git_pull_tags);
    assert!(!settings.git_rebase_when_sync);
    assert!(!settings.git_remember_post_commit_command);
    assert!(!settings.git_replace_tags_when_pull);
    assert!(!settings.git_support_cancellation);
    assert!(settings.git_terminal_authentication);
    assert!(!settings.git_terminal_git_editor);
    assert!(settings.git_use_force_push_if_includes);
    assert!(settings.git_use_force_push_with_lease);
    assert!(settings.git_use_integrated_ask_pass);
    assert!(settings.git_worktree_include_files.is_empty());
    assert_eq!(
        settings.git_default_branch_name,
        DEFAULT_GIT_DEFAULT_BRANCH_NAME
    );
    assert_eq!(settings.git_default_clone_directory, None);
    assert_eq!(
        settings.git_similarity_threshold,
        DEFAULT_GIT_SIMILARITY_THRESHOLD
    );
    assert_eq!(settings.scm_default_view_mode, ScmDefaultViewMode::List);
    assert_eq!(
        settings.scm_default_view_sort_key,
        ScmDefaultViewSortKey::Path
    );
    assert!(settings.scm_auto_reveal);
    assert_eq!(settings.scm_count_badge, ScmCountBadge::All);
    assert_eq!(
        settings.scm_provider_count_badge,
        ScmProviderCountBadge::Hidden
    );
    assert!(!settings.scm_always_show_repositories);
    assert_eq!(
        settings.scm_repositories_visible,
        DEFAULT_SCM_REPOSITORIES_VISIBLE
    );
    assert!(settings.scm_compact_folders);
    assert!(!settings.scm_always_show_actions);
    assert!(settings.scm_show_action_button);
    assert!(settings.git_show_commit_input);
    assert!(!settings.git_show_push_success_notification);
    assert!(settings.git_use_editor_as_commit_input);
    assert!(!settings.git_verbose_commit);
    assert!(settings.git_show_action_button_commit);
    assert!(!settings.git_always_sign_off);
    assert!(settings.git_confirm_committed_delete);
    assert!(settings.git_confirm_empty_commits);
    assert!(settings.git_require_user_config);
    assert!(settings.git_show_progress);
    assert!(settings.git_show_reference_details);
    assert!(settings.git_timeline_show_author);
    assert!(!settings.git_timeline_show_uncommitted);
    assert_eq!(settings.git_timeline_date, GitTimelineDate::Committed);
    assert!(settings.git_show_inline_open_file_action);
    assert_eq!(settings.git_count_badge, GitCountBadge::All);
    assert_eq!(settings.git_untracked_changes, GitUntrackedChanges::Mixed);
    assert!(settings.git_open_diff_on_click);
    assert!(!settings.git_close_diff_on_operation);
    assert!(!settings.git_always_show_staged_changes_resource_group);
    assert_eq!(
        settings.git_checkout_type,
        [
            GitCheckoutType::Local,
            GitCheckoutType::Remote,
            GitCheckoutType::Tags
        ]
    );
    assert_eq!(
        settings.git_branch_sort_order,
        GitBranchSortOrder::CommitterDate
    );
    assert_eq!(settings.git_branch_prefix, DEFAULT_GIT_BRANCH_PREFIX);
    assert!(!settings.git_branch_random_name_enable);
    assert_eq!(
        settings.git_branch_random_name_dictionary,
        ["adjectives", "animals"]
    );
    assert_eq!(
        settings.git_branch_validation_regex,
        DEFAULT_GIT_BRANCH_VALIDATION_REGEX
    );
    assert_eq!(
        settings.git_branch_whitespace_char,
        DEFAULT_GIT_BRANCH_WHITESPACE_CHAR
    );
    assert!(settings.git_decorations_enabled);
    assert!(!settings.git_enable_smart_commit);
    assert!(settings.git_suggest_smart_commit);
    assert_eq!(
        settings.git_smart_commit_changes,
        GitSmartCommitChanges::All
    );
    assert_eq!(
        settings.git_prompt_to_save_files_before_commit,
        GitPromptToSaveFilesBeforeCommit::Always
    );
    assert_eq!(
        settings.git_prompt_to_save_files_before_stash,
        GitPromptToSaveFilesBeforeCommit::Always
    );
    assert!(settings.git_branch_protection.is_empty());
    assert_eq!(
        settings.git_branch_protection_prompt,
        GitBranchProtectionPrompt::AlwaysPrompt
    );
    assert_eq!(settings.git_status_limit, DEFAULT_GIT_STATUS_LIMIT);
    assert!(!settings.git_use_commit_input_as_stash_message);
    assert_eq!(
        settings.git_commit_short_hash_length,
        DEFAULT_GIT_COMMIT_SHORT_HASH_LENGTH
    );
    assert!(!settings.git_input_validation);
    assert_eq!(
        settings.git_input_validation_length,
        DEFAULT_GIT_INPUT_VALIDATION_LENGTH
    );
    assert_eq!(
        settings.git_input_validation_subject_length,
        GitInputValidationSubjectLength::default()
    );
    assert!(settings.git_blame_status_bar_item_enabled);
    assert!(!settings.git_blame_editor_decoration_enabled);
    assert!(!settings.git_blame_editor_decoration_disable_hover);
    assert!(!settings.git_blame_ignore_whitespace);
    assert_eq!(
        settings.git_blame_status_bar_item_template,
        DEFAULT_GIT_BLAME_STATUS_BAR_ITEM_TEMPLATE
    );
    assert_eq!(
        settings.git_blame_editor_decoration_template,
        DEFAULT_GIT_BLAME_EDITOR_DECORATION_TEMPLATE
    );
    assert!(settings.scm_show_input_action_button);
    assert_eq!(
        settings.scm_input_min_line_count,
        DEFAULT_SCM_INPUT_MIN_LINE_COUNT
    );
    assert_eq!(
        settings.scm_input_max_line_count,
        DEFAULT_SCM_INPUT_MAX_LINE_COUNT
    );
    assert_eq!(
        settings.scm_input_font_family,
        DEFAULT_SCM_INPUT_FONT_FAMILY
    );
    assert_eq!(settings.scm_input_font_size, DEFAULT_SCM_INPUT_FONT_SIZE);
    assert_eq!(settings.scm_diff_decorations, ScmDiffDecorations::All);
    assert_eq!(
        settings.scm_diff_decorations_gutter_action,
        ScmDiffDecorationsGutterAction::Diff
    );
    assert_eq!(
        settings.scm_diff_decorations_gutter_visibility,
        ScmDiffDecorationsGutterVisibility::Always
    );
    assert_eq!(
        settings.scm_diff_decorations_gutter_width,
        DEFAULT_SCM_DIFF_DECORATIONS_GUTTER_WIDTH
    );
    assert_eq!(
        settings.scm_diff_decorations_gutter_pattern,
        ScmDiffDecorationsGutterPattern::default()
    );
    assert_eq!(
        settings.scm_diff_decorations_ignore_trim_whitespace,
        ScmDiffDecorationsIgnoreTrimWhitespace::False
    );
    assert!(settings.scm_graph_page_on_scroll);
    assert_eq!(settings.scm_graph_page_size, DEFAULT_SCM_GRAPH_PAGE_SIZE);
    assert_eq!(settings.scm_graph_badges, ScmGraphBadges::Filter);
    assert!(settings.scm_graph_show_incoming_changes);
    assert!(settings.scm_graph_show_outgoing_changes);
    assert!(settings.bracket_pair_colorization);
    assert!(!settings.bracket_pair_colorization_independent_color_pool_per_bracket_type);
    assert_eq!(
        settings.bracket_pair_guides,
        EditorBracketPairGuideMode::Off
    );
    assert_eq!(
        settings.bracket_pair_guides_horizontal,
        EditorBracketPairGuideMode::Off
    );
    assert!(settings.highlight_active_bracket_pair);
    assert_eq!(settings.match_brackets, EditorMatchBrackets::Always);
    assert!(settings.folding);
    assert!(settings.folding_highlight);
    assert!(settings.folding_imports_by_default);
    assert_eq!(
        settings.folding_maximum_regions,
        DEFAULT_EDITOR_FOLDING_MAXIMUM_REGIONS
    );
    assert_eq!(settings.folding_strategy, EditorFoldingStrategy::default());
    assert!(!settings.unfold_on_click_after_end_of_line);
    assert_eq!(
        settings.show_folding_controls,
        EditorShowFoldingControls::default()
    );
    assert!(settings.indent_guides);
    assert_eq!(
        settings.highlight_active_indentation,
        EditorHighlightActiveIndentation::Focused
    );
    assert_eq!(settings.mouse_style, EditorMouseStyle::default());
    assert_eq!(
        settings.cursor_smooth_caret_animation,
        EditorCursorSmoothCaretAnimation::default()
    );
    assert_eq!(settings.cursor_style, EditorCursorStyle::default());
    assert_eq!(settings.overtype_cursor_style, EditorCursorStyle::Block);
    assert!(settings.overtype_on_paste);
    assert!(!settings.cursor_blinking);
    assert_eq!(settings.cursor_width, DEFAULT_EDITOR_CURSOR_WIDTH);
    assert_eq!(settings.cursor_height, DEFAULT_EDITOR_CURSOR_HEIGHT);
    assert_eq!(
        settings.cursor_surrounding_lines,
        DEFAULT_EDITOR_CURSOR_SURROUNDING_LINES
    );
    assert_eq!(
        settings.cursor_surrounding_lines_style,
        EditorCursorSurroundingLinesStyle::default()
    );
    assert_eq!(
        settings.terminal_scrollback_rows,
        DEFAULT_TERMINAL_SCROLLBACK_ROWS
    );
    assert_eq!(settings.terminal_shell_path, None);
    assert!(settings.terminal_shell_args.is_empty());
    assert_eq!(settings.terminal_cwd, None);
    assert_eq!(settings.terminal_split_cwd, TerminalSplitCwd::default());
    assert_eq!(settings.terminal_min_rows, DEFAULT_TERMINAL_MIN_ROWS);
    assert_eq!(settings.terminal_min_columns, DEFAULT_TERMINAL_MIN_COLUMNS);
    assert_eq!(settings.terminal_font_size, DEFAULT_TERMINAL_FONT_SIZE);
    assert_eq!(settings.terminal_line_height, DEFAULT_TERMINAL_LINE_HEIGHT);
    assert_eq!(
        settings.terminal_letter_spacing,
        DEFAULT_TERMINAL_LETTER_SPACING
    );
    assert_eq!(
        settings.terminal_cursor_style,
        TerminalCursorStyle::default()
    );
    assert_eq!(
        settings.terminal_cursor_width,
        DEFAULT_TERMINAL_CURSOR_WIDTH
    );
    assert!(!settings.terminal_cursor_blinking);
    assert_eq!(
        settings.terminal_cursor_style_inactive,
        TerminalInactiveCursorStyle::default()
    );
    assert!(settings.terminal_draw_bold_text_in_bright_colors);
    assert_eq!(
        settings.terminal_minimum_contrast_ratio,
        DEFAULT_TERMINAL_MINIMUM_CONTRAST_RATIO
    );
    assert_eq!(settings.terminal_enable_bell, DEFAULT_TERMINAL_ENABLE_BELL);
    assert_eq!(
        settings.terminal_bell_duration_ms,
        DEFAULT_TERMINAL_BELL_DURATION_MS
    );
    assert_eq!(
        settings.terminal_show_exit_alert,
        DEFAULT_TERMINAL_SHOW_EXIT_ALERT
    );
    assert_eq!(
        settings.terminal_hide_on_startup,
        TerminalHideOnStartup::default()
    );
    assert_eq!(
        settings.terminal_hide_on_last_closed,
        DEFAULT_TERMINAL_HIDE_ON_LAST_CLOSED
    );
    assert_eq!(
        settings.terminal_confirm_on_exit,
        TerminalConfirmOnExit::default()
    );
    assert_eq!(
        settings.terminal_confirm_on_kill,
        TerminalConfirmOnKill::default()
    );
    assert_eq!(
        settings.terminal_tabs_enabled,
        DEFAULT_TERMINAL_TABS_ENABLED
    );
    assert_eq!(
        settings.terminal_tabs_default_icon,
        DEFAULT_TERMINAL_TABS_DEFAULT_ICON
    );
    assert_eq!(settings.terminal_tabs_default_color, None);
    assert_eq!(
        settings.terminal_tabs_allow_agent_cli_title,
        DEFAULT_TERMINAL_TABS_ALLOW_AGENT_CLI_TITLE
    );
    assert_eq!(settings.terminal_tabs_title, DEFAULT_TERMINAL_TABS_TITLE);
    assert_eq!(
        settings.terminal_tabs_hide_condition,
        TerminalTabsHideCondition::default()
    );
    assert_eq!(
        settings.terminal_tabs_show_active_terminal,
        TerminalTabsShowActiveTerminal::default()
    );
    assert_eq!(
        settings.terminal_tabs_show_actions,
        TerminalTabsShowActions::default()
    );
    assert_eq!(
        settings.terminal_tabs_focus_mode,
        TerminalTabsFocusMode::default()
    );
    assert_eq!(
        settings.terminal_tabs_location,
        TerminalTabsLocation::default()
    );
    assert_eq!(
        settings.terminal_right_click_behavior,
        TerminalRightClickBehavior::default()
    );
    assert_eq!(
        settings.terminal_middle_click_behavior,
        TerminalMiddleClickBehavior::default()
    );
    assert_eq!(
        settings.terminal_alt_click_moves_cursor,
        DEFAULT_TERMINAL_ALT_CLICK_MOVES_CURSOR
    );
    assert_eq!(
        settings.terminal_copy_on_selection,
        DEFAULT_TERMINAL_COPY_ON_SELECTION
    );
    assert_eq!(
        settings.terminal_ignore_bracketed_paste_mode,
        DEFAULT_TERMINAL_IGNORE_BRACKETED_PASTE_MODE
    );
    assert_eq!(
        settings.terminal_enable_multi_line_paste_warning,
        TerminalMultiLinePasteWarning::default()
    );
    assert_eq!(
        settings.terminal_word_separators,
        DEFAULT_TERMINAL_WORD_SEPARATORS
    );
    assert_eq!(
        settings.terminal_mouse_wheel_scroll_sensitivity,
        DEFAULT_TERMINAL_MOUSE_WHEEL_SCROLL_SENSITIVITY
    );
    assert_eq!(
        settings.terminal_fast_scroll_sensitivity,
        DEFAULT_TERMINAL_FAST_SCROLL_SENSITIVITY
    );
    assert_eq!(
        settings.terminal_mouse_wheel_zoom,
        DEFAULT_TERMINAL_MOUSE_WHEEL_ZOOM
    );
    assert!(!settings.trim_trailing_whitespace);
    assert!(!settings.insert_final_newline);
    assert!(!settings.trim_final_newlines);
    assert_eq!(settings.theme.name, "Custom");
    assert_eq!(
        settings.theme.background,
        ThemeSettings::default().background
    );
    assert!(!settings.keymap.bindings.is_empty());
}

#[test]
fn legacy_selection_highlight_settings_are_ignored() {
    let settings: EditorSettings = toml::from_str(
        "selection_highlight = false\n\
         selection_highlight_max_length = 12\n\
         selection_highlight_multiline = true\n\
         occurrences_highlight = \"multiFile\"\n\
         occurrences_highlight_delay_ms = 175\n\
         render_line_highlight_only_when_focus = true\n",
    )
    .expect("legacy highlight settings should not block loading");

    assert!(settings.render_line_highlight_only_when_focus);
}

#[test]
fn zed_style_vim_mode_setting_alias_enables_vim_keybindings() {
    let settings: EditorSettings =
        toml::from_str("vim_mode = true\n").expect("vim_mode alias should load");

    assert!(settings.vim_keybindings);
}

#[test]
fn project_index_and_search_settings_load_from_toml() {
    let settings: EditorSettings = toml::from_str(
        "project_index_max_files = 120000\n\
         project_index_exclude_globs = [\"dist\", \"build\"]\n\
         project_search_exclude_globs = \"cache\"\n\
         project_search_max_file_size_mb = 8\n\
         project_search_max_results = 1234\n",
    )
    .expect("project search settings should load");

    assert_eq!(settings.project_index_max_files, 120_000);
    assert_eq!(settings.project_index_exclude_globs, ["dist", "build"]);
    assert_eq!(settings.project_search_exclude_globs, ["cache"]);
    assert_eq!(settings.project_search_max_file_size_mb, 8);
    assert_eq!(settings.project_search_max_results, 1234);
}

#[test]
fn editor_background_image_settings_parse_with_named_enum_values() {
    let settings: EditorSettings = toml::from_str(
        "background_image_enabled = true\n\
         background_image_path = \"assets/editor.png\"\n\
         background_image_scope = \"full_app\"\n\
         background_image_dim = 0.8\n\
         background_image_fit = \"contain\"\n\
         background_image_position = \"bottom\"\n\
         background_image_loop = false\n",
    )
    .expect("editor background image settings should load");

    assert!(settings.background_image_enabled);
    assert_eq!(
        settings.background_image_path.as_deref(),
        Some("assets/editor.png")
    );
    assert_eq!(
        settings.background_image_scope,
        EditorBackgroundImageScope::FullApp
    );
    assert_eq!(settings.background_image_dim, 0.8);
    assert_eq!(
        settings.background_image_fit,
        EditorBackgroundImageFit::Contain
    );
    assert_eq!(
        settings.background_image_position,
        EditorBackgroundImagePosition::Bottom
    );
    assert!(!settings.background_image_loop);
}

#[test]
fn editor_background_image_settings_save_and_reload_preserves_values() {
    let path = temp_settings_path("background-image-roundtrip");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();
    let settings = EditorSettings {
        background_image_enabled: true,
        background_image_path: Some("assets/editor.png".to_owned()),
        background_image_scope: EditorBackgroundImageScope::FullApp,
        background_image_dim: 0.6,
        background_image_fit: EditorBackgroundImageFit::Stretch,
        background_image_position: EditorBackgroundImagePosition::Top,
        background_image_loop: false,
        ..EditorSettings::default()
    };

    settings.save(&path).unwrap();

    let loaded = EditorSettings::load_or_create(&path).unwrap();
    assert_eq!(loaded, settings);
    assert_no_setting_temps(&path);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_index_and_search_settings_default_and_sanitize() {
    let defaults = EditorSettings::default();
    assert_eq!(
        defaults.project_index_exclude_globs,
        default_project_index_exclude_globs()
    );
    assert!(!defaults.project_index_include_hidden_dirs);
    assert_eq!(
        defaults.project_search_exclude_globs,
        default_project_search_exclude_globs()
    );

    let root = std::env::temp_dir().join(format!(
        "kuroya-project-settings-sanitize-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("settings.toml");
    std::fs::write(
        &path,
        "project_index_max_files = 0\n\
         project_index_exclude_globs = [\" target \", \"\", \"target\"]\n\
         project_search_exclude_globs = [\" cache \", \"cache\"]\n\
         project_search_max_file_size_mb = 0\n\
         project_search_max_results = 0\n",
    )
    .unwrap();

    let settings = EditorSettings::load_or_create(&path).unwrap();

    assert_eq!(
        settings.project_index_max_files,
        MIN_PROJECT_INDEX_MAX_FILES
    );
    assert_eq!(settings.project_index_exclude_globs, ["target"]);
    assert_eq!(settings.project_search_exclude_globs, ["cache"]);
    assert_eq!(
        settings.project_search_max_file_size_mb,
        MIN_PROJECT_SEARCH_MAX_FILE_SIZE_MB
    );
    assert_eq!(
        settings.project_search_max_results,
        MIN_PROJECT_SEARCH_MAX_RESULTS
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_index_include_hidden_dirs_setting_save_and_reload_preserves_value() {
    let path = temp_settings_path("project-index-hidden-dirs-roundtrip");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();
    assert!(!EditorSettings::default().project_index_include_hidden_dirs);
    let settings = EditorSettings {
        project_index_include_hidden_dirs: true,
        ..EditorSettings::default()
    };

    settings.save(&path).unwrap();

    let loaded = EditorSettings::load_or_create(&path).unwrap();
    assert_eq!(loaded, settings);
    assert!(loaded.project_index_include_hidden_dirs);
    assert_no_setting_temps(&path);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn built_in_themes_are_named_and_cyclable() {
    let expected_names: Vec<String> = [
        "Matte Dark",
        "Graphite",
        "Carbon Blue",
        "Soft Light",
        "Midnight Teal",
        "Forest Dusk",
        "Plum Dusk",
        "Warm Paper",
        "Clear Day",
        "Sage Light",
        "Sakura Milk",
        "Mochi Mint",
        "Sky Ribbon",
        "Starry Idol",
        "Peach Festival",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();

    let names = ThemeSettings::built_in_names();
    assert_eq!(names, expected_names);

    let mut unique_names = std::collections::HashSet::new();
    for name in &names {
        assert!(
            unique_names.insert(name.to_ascii_lowercase()),
            "duplicate theme name: {name}"
        );
        let theme = ThemeSettings::built_in_by_name(name).expect("theme name should resolve");
        assert_eq!(&theme.name, name);
        assert_eq!(
            ThemeSettings::built_in_by_name(&name.to_ascii_uppercase()).as_ref(),
            Some(&theme)
        );
    }

    for pair in names.windows(2) {
        assert_eq!(
            ThemeSettings::next_built_in_after(&pair[0]).name,
            pair[1].as_str()
        );
    }
    assert_eq!(
        ThemeSettings::next_built_in_after(names.last().unwrap()).name,
        names[0].as_str()
    );
    assert_eq!(
        ThemeSettings::next_built_in_after("Custom").name,
        names[0].as_str()
    );
}

#[test]
fn settings_save_replaces_existing_file_without_temp_files() {
    let path = temp_settings_path("replace");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();
    let first = EditorSettings {
        font_size: 14.0,
        ..EditorSettings::default()
    };
    let second = EditorSettings {
        font_size: 16.0,
        autosave: false,
        ..EditorSettings::default()
    };

    first.save(&path).unwrap();
    second.save(&path).unwrap();

    let loaded = EditorSettings::load_or_create(&path).unwrap();
    assert_eq!(loaded, second);
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .contains(&format!("schema_version = {SETTINGS_SCHEMA_VERSION}"))
    );
    assert_no_setting_temps(&path);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn settings_save_and_reload_preserves_enabled_vim_settings() {
    let path = temp_settings_path("vim-roundtrip");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();
    let settings = EditorSettings {
        vim_keybindings: true,
        vim: EditorVimSettings {
            disabled_bindings: vec!["Q".to_owned(), "<C-n>".to_owned()],
            key_overrides: vec![
                EditorVimKeyOverride {
                    before: "H".to_owned(),
                    after: "0".to_owned(),
                    command: None,
                },
                EditorVimKeyOverride {
                    before: "<Home>".to_owned(),
                    after: "<C-r>".to_owned(),
                    command: None,
                },
                EditorVimKeyOverride {
                    before: "K".to_owned(),
                    after: String::new(),
                    command: Some(Command::RequestHover),
                },
            ],
        },
        ..EditorSettings::default()
    };

    settings.save(&path).unwrap();

    let loaded = EditorSettings::load_or_create(&path).unwrap();
    assert_eq!(loaded.vim_keybindings, settings.vim_keybindings);
    assert_eq!(loaded.vim, settings.vim);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn settings_save_and_reload_preserves_disabled_vim_mode_with_custom_rows() {
    let path = temp_settings_path("vim-disabled-roundtrip");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();
    let settings = EditorSettings {
        vim_keybindings: false,
        vim: EditorVimSettings {
            disabled_bindings: vec!["Q".to_owned()],
            key_overrides: vec![EditorVimKeyOverride {
                before: "K".to_owned(),
                after: String::new(),
                command: Some(Command::RequestHover),
            }],
        },
        ..EditorSettings::default()
    };

    settings.save(&path).unwrap();

    let loaded = EditorSettings::load_or_create(&path).unwrap();
    assert!(!loaded.vim_keybindings);
    assert_eq!(loaded.vim, settings.vim);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn settings_parse_and_save_sanitize_stale_keymap_bindings() {
    let path = temp_settings_path("keymap-sanitize");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();
    let raw = EditorSettings {
        keymap: Keymap {
            bindings: vec![
                KeyBinding {
                    chord: " shift + control + p ".to_owned(),
                    command: Command::ToggleQuickOpen,
                },
                KeyBinding {
                    chord: "Ctrl+Shift+P".to_owned(),
                    command: Command::ToggleCommandPalette,
                },
                KeyBinding {
                    chord: "P".to_owned(),
                    command: Command::ToggleTerminal,
                },
            ],
        },
        ..EditorSettings::default()
    };
    let raw_text = toml::to_string_pretty(&raw).unwrap();

    let (settings, should_resave) = parse_settings_text(&raw_text).unwrap();

    assert!(should_resave);
    assert_eq!(
        settings.keymap.bindings,
        vec![KeyBinding {
            chord: "Ctrl+Shift+P".to_owned(),
            command: Command::ToggleQuickOpen,
        }]
    );

    raw.save(&path).unwrap();
    let (saved, should_resave_saved) =
        parse_settings_text(&fs::read_to_string(&path).unwrap()).unwrap();

    assert!(!should_resave_saved);
    assert_eq!(saved.keymap.bindings, settings.keymap.bindings);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn settings_migrates_schema_two_keymap_to_include_command_palette_shortcut() {
    let mut raw = EditorSettings {
        schema_version: 2,
        ..EditorSettings::default()
    };
    raw.keymap
        .bindings
        .retain(|binding| binding.command != Command::ToggleCommandPalette);
    assert!(
        !raw.keymap
            .bindings
            .iter()
            .any(|binding| binding.chord == "Ctrl+Shift+P")
    );

    let raw_text = toml::to_string_pretty(&raw).unwrap();
    let (settings, should_resave) = parse_settings_text(&raw_text).unwrap();

    assert!(should_resave);
    assert_eq!(settings.schema_version, SETTINGS_SCHEMA_VERSION);
    assert!(settings.keymap.bindings.iter().any(|binding| {
        binding.chord == "Ctrl+Shift+P" && binding.command == Command::ToggleCommandPalette
    }));
}

#[test]
fn settings_command_palette_keymap_migration_respects_custom_ctrl_shift_p_binding() {
    let mut raw = EditorSettings {
        schema_version: 2,
        ..EditorSettings::default()
    };
    raw.keymap
        .bindings
        .retain(|binding| binding.command != Command::ToggleCommandPalette);
    let terminal_binding = raw
        .keymap
        .bindings
        .iter_mut()
        .find(|binding| binding.command == Command::ToggleTerminal)
        .expect("default keymap includes terminal shortcut");
    terminal_binding.chord = "Ctrl+Shift+P".to_owned();

    let raw_text = toml::to_string_pretty(&raw).unwrap();
    let (settings, should_resave) = parse_settings_text(&raw_text).unwrap();

    assert!(should_resave);
    assert_eq!(settings.schema_version, SETTINGS_SCHEMA_VERSION);
    assert!(
        !settings
            .keymap
            .bindings
            .iter()
            .any(|binding| binding.command == Command::ToggleCommandPalette)
    );
    assert!(settings.keymap.bindings.iter().any(|binding| {
        binding.chord == "Ctrl+Shift+P" && binding.command == Command::ToggleTerminal
    }));
}

#[test]
fn settings_save_sanitizes_lsp_server_configs() {
    let path = temp_settings_path("lsp-sanitize");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();
    let raw = EditorSettings {
        lsp_servers: vec![
            crate::lsp::LspServerConfig {
                language: " Rust ".to_owned(),
                command: " rust-analyzer-custom ".to_owned(),
                args: vec![" --stdio ".to_owned()],
                extensions: vec![".rs".to_owned(), "rs".to_owned(), ".".to_owned()],
                root_markers: vec![" Cargo.toml ".to_owned()],
                enabled: true,
            },
            crate::lsp::LspServerConfig {
                language: "kuroya-empty".to_owned(),
                command: String::new(),
                args: Vec::new(),
                extensions: Vec::new(),
                root_markers: Vec::new(),
                enabled: true,
            },
            crate::lsp::LspServerConfig {
                language: " kuroya-test ".to_owned(),
                command: " kuroya-lsp ".to_owned(),
                args: vec![" --stdio ".to_owned()],
                extensions: vec![".kuroya".to_owned(), "kuroya".to_owned(), ".".to_owned()],
                root_markers: vec![" .kuroya-root ".to_owned()],
                enabled: true,
            },
            crate::lsp::LspServerConfig {
                language: "RUST".to_owned(),
                command: "rust-analyzer-final".to_owned(),
                args: Vec::new(),
                extensions: Vec::new(),
                root_markers: Vec::new(),
                enabled: true,
            },
        ],
        ..EditorSettings::default()
    };

    raw.save(&path).unwrap();
    let (saved, should_resave_saved) =
        parse_settings_text(&fs::read_to_string(&path).unwrap()).unwrap();

    assert!(!should_resave_saved);

    assert_eq!(
        saved.lsp_servers,
        vec![
            crate::lsp::LspServerConfig {
                language: "rust".to_owned(),
                command: "rust-analyzer-custom".to_owned(),
                args: vec!["--stdio".to_owned()],
                extensions: vec!["rs".to_owned()],
                root_markers: vec!["Cargo.toml".to_owned()],
                enabled: true,
            },
            crate::lsp::LspServerConfig {
                language: "kuroya-test".to_owned(),
                command: "kuroya-lsp".to_owned(),
                args: vec!["--stdio".to_owned()],
                extensions: vec!["kuroya".to_owned()],
                root_markers: vec![".kuroya-root".to_owned()],
                enabled: true,
            },
            crate::lsp::LspServerConfig {
                language: "rust".to_owned(),
                command: "rust-analyzer-final".to_owned(),
                args: Vec::new(),
                extensions: Vec::new(),
                root_markers: Vec::new(),
                enabled: true,
            },
        ]
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn schema_3_settings_materialize_builtin_lsp_servers() {
    let text = r#"
        schema_version = 3

        [[lsp_servers]]
        language = "rust"
        command = "rust-analyzer-custom"
        root_markers = ["Cargo.toml"]
    "#;

    let (settings, should_resave) = parse_settings_text(text).unwrap();

    assert!(should_resave);
    assert_eq!(settings.schema_version, SETTINGS_SCHEMA_VERSION);

    let servers = settings.lsp_servers;
    let defaults = crate::lsp::default_server_configs();

    assert_eq!(servers.len(), defaults.len());
    let rust = servers
        .iter()
        .find(|server| server.language == "rust")
        .expect("rust config should be present");
    assert_eq!(rust.command, "rust-analyzer-custom");
    assert_eq!(rust.root_markers, ["Cargo.toml"]);
    assert!(rust.enabled, "customized core server stays enabled");
    let python = servers
        .iter()
        .find(|server| server.language == "python")
        .expect("python config should be present");
    assert!(
        python.enabled,
        "python is a core language and stays enabled"
    );
    let java = servers
        .iter()
        .find(|server| server.language == "java")
        .expect("java config should be present");
    assert!(
        !java.enabled,
        "untouched non-core defaults materialize disabled"
    );
    for default in &defaults {
        if default.language == "rust" {
            continue;
        }
        assert!(
            servers.contains(default),
            "default for {} should be materialized",
            default.language
        );
    }
}

#[test]
fn schema_5_settings_trim_pristine_non_core_defaults_to_disabled() {
    let text = r#"
        schema_version = 4

        [[lsp_servers]]
        language = "java"
        command = "jdtls"
        root_markers = ["pom.xml", "build.gradle", "build.gradle.kts", ".project", ".git"]

        [[lsp_servers]]
        language = "dart"
        command = "my-dart-lsp"
        args = ["language-server"]
    "#;

    let (settings, should_resave) = parse_settings_text(text).unwrap();

    assert!(should_resave);
    let java = settings
        .lsp_servers
        .iter()
        .find(|server| server.language == "java")
        .expect("java config should be present");
    assert!(
        !java.enabled,
        "pristine non-core default is disabled by the migration"
    );
    let dart = settings
        .lsp_servers
        .iter()
        .find(|server| server.language == "dart")
        .expect("dart config should be present");
    assert!(dart.enabled, "customized entries keep their enabled state");
}

#[test]
fn schema_3_settings_without_lsp_servers_materialize_all_defaults() {
    let text = "schema_version = 3\n";

    let (settings, should_resave) = parse_settings_text(text).unwrap();

    assert!(should_resave);
    assert_eq!(settings.lsp_servers, crate::lsp::default_server_configs());
}

#[test]
fn settings_migrates_terminal_interaction_defaults_from_schema_one() {
    let text = r#"
        schema_version = 1
        terminal_tabs_focus_mode = "doubleClick"
        terminal_tabs_location = "right"
        terminal_copy_on_selection = false
    "#;

    let (settings, should_resave) = parse_settings_text(text).unwrap();

    assert!(should_resave);
    assert_eq!(settings.schema_version, SETTINGS_SCHEMA_VERSION);
    assert_eq!(
        settings.terminal_tabs_focus_mode,
        TerminalTabsFocusMode::SingleClick
    );
    assert_eq!(settings.terminal_tabs_location, TerminalTabsLocation::Top);
    assert_eq!(
        settings.terminal_copy_on_selection,
        DEFAULT_TERMINAL_COPY_ON_SELECTION
    );
}

#[test]
fn settings_plain_string_clean_fast_path_preserves_capacity_and_status() {
    let clean = "Consolas, 'Courier New', monospace";
    assert!(matches!(
        normalize_settings_plain_string_cow(clean, SETTINGS_STRING_MAX_CHARS, true),
        std::borrow::Cow::Borrowed(_)
    ));

    let mut value = String::with_capacity(128);
    value.push_str(clean);
    let capacity = value.capacity();

    assert!(!sanitize_settings_plain_string(&mut value));
    assert_eq!(value, clean);
    assert_eq!(value.capacity(), capacity);

    let mut optional_value = String::with_capacity(128);
    optional_value.push_str(clean);
    let optional_capacity = optional_value.capacity();
    let mut optional = Some(optional_value);

    assert!(!sanitize_settings_optional_string(&mut optional));
    let optional = optional.as_ref().unwrap();
    assert_eq!(optional, clean);
    assert_eq!(optional.capacity(), optional_capacity);
}

#[test]
fn settings_plain_string_owned_path_matches_sanitized_outputs() {
    let cases = [
        (" padded ", SETTINGS_STRING_MAX_CHARS, true, "padded"),
        (
            "prefix\u{202e}suffix",
            SETTINGS_STRING_MAX_CHARS,
            true,
            "prefixsuffix",
        ),
        (
            "prefix\u{0007}suffix",
            SETTINGS_STRING_MAX_CHARS,
            true,
            "prefixsuffix",
        ),
        ("abcd ef", 5, true, "abcd"),
        ("   ", SETTINGS_STRING_MAX_CHARS, true, ""),
    ];

    for (input, max_chars, trim_edges, expected) in cases {
        let normalized = normalize_settings_plain_string_cow(input, max_chars, trim_edges);
        assert!(matches!(normalized, std::borrow::Cow::Owned(_)));
        assert_eq!(normalized.as_ref(), expected);
        assert_eq!(
            normalize_settings_plain_string(input, max_chars, trim_edges),
            expected
        );
    }

    let mut value = " padded\u{202e} ".to_owned();
    assert!(sanitize_settings_plain_string(&mut value));
    assert_eq!(value, "padded");

    let mut optional = Some(" \u{202e} ".to_owned());
    assert!(sanitize_settings_optional_string(&mut optional));
    assert_eq!(optional, None);

    let mut empty_optional = Some(String::new());
    assert!(sanitize_settings_optional_string(&mut empty_optional));
    assert_eq!(empty_optional, None);
}

#[test]
fn settings_required_plain_strings_fall_back_when_empty_after_sanitize() {
    let clean = "Consolas, 'Courier New', monospace";
    let mut value = String::with_capacity(128);
    value.push_str(clean);
    let capacity = value.capacity();

    assert!(!sanitize_settings_plain_string_with_default(
        &mut value,
        DEFAULT_EDITOR_FONT_FAMILY
    ));
    assert_eq!(value, clean);
    assert_eq!(value.capacity(), capacity);

    let mut blank = " \u{202e}\t ".to_owned();
    assert!(sanitize_settings_plain_string_with_default(
        &mut blank,
        DEFAULT_EDITOR_FONT_FAMILY
    ));
    assert_eq!(blank, DEFAULT_EDITOR_FONT_FAMILY);
}

#[test]
fn settings_display_text_clean_fast_path_preserves_capacity_and_status() {
    let clean = "Editor Label 2";
    assert!(matches!(
        normalize_settings_display_text_cow(
            clean,
            SETTINGS_DISPLAY_TEXT_MAX_CHARS,
            Some("Fallback")
        ),
        std::borrow::Cow::Borrowed(_)
    ));

    let mut value = String::with_capacity(128);
    value.push_str(clean);
    let capacity = value.capacity();

    assert!(!sanitize_settings_display_string(
        &mut value,
        Some("Fallback")
    ));
    assert_eq!(value, clean);
    assert_eq!(value.capacity(), capacity);

    let mut optional_value = String::with_capacity(128);
    optional_value.push_str(clean);
    let optional_capacity = optional_value.capacity();
    let mut optional = Some(optional_value);

    assert!(!sanitize_settings_optional_display_string(&mut optional));
    let optional = optional.as_ref().unwrap();
    assert_eq!(optional, clean);
    assert_eq!(optional.capacity(), optional_capacity);
}

#[test]
fn settings_display_text_owned_path_matches_sanitized_outputs() {
    let cases = [
        (
            " padded ",
            SETTINGS_DISPLAY_TEXT_MAX_CHARS,
            Some("Fallback"),
            "padded",
        ),
        (
            "Alpha\u{0007}Beta",
            SETTINGS_DISPLAY_TEXT_MAX_CHARS,
            None,
            "Alpha Beta",
        ),
        (
            "Alpha\tBeta",
            SETTINGS_DISPLAY_TEXT_MAX_CHARS,
            None,
            "Alpha Beta",
        ),
        (
            "Alpha  Beta",
            SETTINGS_DISPLAY_TEXT_MAX_CHARS,
            None,
            "Alpha Beta",
        ),
        (
            "Alpha\u{202e}Beta",
            SETTINGS_DISPLAY_TEXT_MAX_CHARS,
            None,
            "Alpha Beta",
        ),
        (
            "   ",
            SETTINGS_DISPLAY_TEXT_MAX_CHARS,
            Some("Fallback"),
            "Fallback",
        ),
        ("abcdef", 5, None, "ab..."),
    ];

    for (input, max_chars, fallback, expected) in cases {
        let normalized = normalize_settings_display_text_cow(input, max_chars, fallback);
        assert!(matches!(normalized, std::borrow::Cow::Owned(_)));
        assert_eq!(normalized.as_ref(), expected);
        assert_eq!(
            normalize_settings_display_text(input, max_chars, fallback),
            expected
        );
    }

    let mut value = "Alpha\tBeta".to_owned();
    assert!(sanitize_settings_display_string(&mut value, None));
    assert_eq!(value, "Alpha Beta");

    let mut fallback_value = " \u{202e} ".to_owned();
    assert!(sanitize_settings_display_string(
        &mut fallback_value,
        Some("Fallback")
    ));
    assert_eq!(fallback_value, "Fallback");

    let mut optional = Some(" \t ".to_owned());
    assert!(sanitize_settings_optional_display_string(&mut optional));
    assert_eq!(optional, None);
}

#[test]
fn settings_parse_sanitizes_numeric_bounds_lists_maps_and_display_text() {
    let locales = (0..=SETTINGS_LIST_MAX_ITEMS)
        .map(|index| format!(r#"" locale-{index}\u202e ""#))
        .collect::<Vec<_>>()
        .join(", ");
    let locale_map = (0..=SETTINGS_MAP_MAX_ITEMS)
        .map(|index| format!(r#"" locale-{index}\u202e " = true"#))
        .collect::<Vec<_>>()
        .join(", ");
    let hook_sources = (0..=SETTINGS_MAP_MAX_ITEMS)
        .map(|index| format!(r#"" hook-{index} " = " warning\u202e ""#))
        .collect::<Vec<_>>()
        .join(", ");
    let theme_paths = (0..=SETTINGS_LIST_MAX_ITEMS)
        .map(|index| format!(r#"" theme-{index}.toml\u202e ""#))
        .collect::<Vec<_>>()
        .join(", ");
    let text = format!(
        r#"
                font_size = 1000.0
                terminal_scrollback_rows = 999999999
                terminal_min_rows = 0
                terminal_font_size = -1.0
                terminal_bell_duration_ms = 999999
                aria_label = " Editor\u202e\nLabel "
                terminal_tabs_title = " Term\u202e\tTitle "
                scm_input_min_line_count = 50
                scm_input_max_line_count = 1
                git_checkout_type = ["local", "local", "remote"]
                word_segmenter_locales = [{locales}]
                custom_theme_paths = [{theme_paths}]
                active_custom_theme_path = " theme-1.toml "
                unicode_highlight_allowed_locales = {{ {locale_map} }}
                git_diagnostics_commit_hook_sources = {{ {hook_sources} }}
            "#
    );

    let (settings, should_resave) = parse_settings_text(&text).unwrap();

    assert!(should_resave);
    assert_eq!(settings.font_size, MAX_EDITOR_FONT_SIZE);
    assert_eq!(
        settings.terminal_scrollback_rows,
        MAX_TERMINAL_SCROLLBACK_ROWS
    );
    assert_eq!(settings.terminal_min_rows, MIN_TERMINAL_MIN_ROWS);
    assert_eq!(settings.terminal_font_size, MIN_TERMINAL_FONT_SIZE);
    assert_eq!(
        settings.terminal_bell_duration_ms,
        MAX_TERMINAL_BELL_DURATION_MS
    );
    assert_eq!(settings.aria_label, "Editor Label");
    assert_eq!(settings.terminal_tabs_title, "Term Title");
    assert!(!settings.aria_label.chars().any(char::is_control));
    assert!(!settings.aria_label.chars().any(is_settings_format_control));
    assert_eq!(settings.scm_input_min_line_count, 50);
    assert_eq!(settings.scm_input_max_line_count, 50);
    assert_eq!(
        settings.git_checkout_type,
        vec![GitCheckoutType::Local, GitCheckoutType::Remote]
    );
    assert_eq!(
        settings.word_segmenter_locales.len(),
        SETTINGS_LIST_MAX_ITEMS
    );
    assert_eq!(settings.custom_theme_paths.len(), SETTINGS_LIST_MAX_ITEMS);
    assert_eq!(
        settings.active_custom_theme_path.as_deref(),
        Some("theme-1.toml")
    );
    assert!(
        settings
            .word_segmenter_locales
            .iter()
            .all(|locale| !locale.chars().any(is_settings_format_control))
    );
    assert_eq!(
        settings.unicode_highlight_allowed_locales.len(),
        SETTINGS_MAP_MAX_ITEMS
    );
    assert_eq!(
        settings.git_diagnostics_commit_hook_sources.len(),
        SETTINGS_MAP_MAX_ITEMS
    );
    assert!(
        settings
            .git_diagnostics_commit_hook_sources
            .values()
            .all(|value| !value.chars().any(is_settings_format_control))
    );
}

#[test]
fn settings_parse_clears_stale_active_custom_theme_path() {
    let (settings, should_resave) = parse_settings_text(
        r#"
            custom_theme_paths = ["theme-a.toml"]
            active_custom_theme_path = "theme-missing.toml"
        "#,
    )
    .unwrap();

    assert!(should_resave);
    assert_eq!(settings.custom_theme_paths, ["theme-a.toml"]);
    assert_eq!(settings.active_custom_theme_path, None);
}

#[test]
fn settings_parse_defaults_required_blank_text_fields() {
    let text = r#"
        font_family = " \u202e\t "
        minimap_mark_section_header_regex = " \u202e\t "
        terminal_tabs_default_icon = " \u202e\t "
        terminal_tabs_title = " \u202e\t "
    "#;

    let (settings, should_resave) = parse_settings_text(text).unwrap();

    assert!(should_resave);
    assert_eq!(settings.font_family, DEFAULT_EDITOR_FONT_FAMILY);
    assert_eq!(
        settings.minimap_mark_section_header_regex,
        DEFAULT_EDITOR_MINIMAP_MARK_SECTION_HEADER_REGEX
    );
    assert_eq!(
        settings.terminal_tabs_default_icon,
        DEFAULT_TERMINAL_TABS_DEFAULT_ICON
    );
    assert_eq!(settings.terminal_tabs_title, DEFAULT_TERMINAL_TABS_TITLE);
}

#[test]
fn settings_save_defaults_required_blank_text_fields() {
    let path = temp_settings_path("save-required-text-defaults");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();
    let settings = EditorSettings {
        font_family: " \u{202e}\t ".to_owned(),
        minimap_mark_section_header_regex: " \u{202e}\t ".to_owned(),
        terminal_tabs_default_icon: " \u{202e}\t ".to_owned(),
        terminal_tabs_title: " \u{202e}\t ".to_owned(),
        ..EditorSettings::default()
    };

    settings.save(&path).unwrap();
    let loaded = EditorSettings::load_or_create(&path).unwrap();

    assert_eq!(loaded.font_family, DEFAULT_EDITOR_FONT_FAMILY);
    assert_eq!(
        loaded.minimap_mark_section_header_regex,
        DEFAULT_EDITOR_MINIMAP_MARK_SECTION_HEADER_REGEX
    );
    assert_eq!(
        loaded.terminal_tabs_default_icon,
        DEFAULT_TERMINAL_TABS_DEFAULT_ICON
    );
    assert_eq!(loaded.terminal_tabs_title, DEFAULT_TERMINAL_TABS_TITLE);
    assert_no_setting_temps(&path);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn settings_string_maps_report_normalized_duplicate_keys() {
    let mut values = BTreeMap::from([
        ("hook".to_owned(), "warning".to_owned()),
        (" hook\u{202e} ".to_owned(), "error".to_owned()),
    ]);

    assert!(sanitize_settings_string_map(&mut values));
    assert_eq!(
        values,
        BTreeMap::from([("hook".to_owned(), "warning".to_owned())])
    );
}
