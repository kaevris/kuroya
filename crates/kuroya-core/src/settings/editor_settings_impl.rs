use super::*;

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            font_size: 13.0,
            ui_font_size: 13.0,
            editor_font_path: None,
            ui_font_path: None,
            font_family: DEFAULT_EDITOR_FONT_FAMILY.to_owned(),
            font_weight: DEFAULT_EDITOR_FONT_WEIGHT.to_owned(),
            font_ligatures: DEFAULT_EDITOR_FONT_LIGATURES.to_owned(),
            font_variations: DEFAULT_EDITOR_FONT_VARIATIONS.to_owned(),
            letter_spacing: DEFAULT_EDITOR_LETTER_SPACING,
            automatic_layout: false,
            disable_layer_hinting: false,
            disable_monospace_optimizations: false,
            extra_editor_class_name: String::new(),
            allow_variable_line_heights: true,
            allow_variable_fonts: true,
            allow_variable_fonts_in_accessibility_mode: false,
            accessibility_support: EditorAccessibilitySupport::default(),
            accessibility_page_size: DEFAULT_EDITOR_ACCESSIBILITY_PAGE_SIZE,
            aria_label: DEFAULT_EDITOR_ARIA_LABEL.to_owned(),
            aria_required: false,
            screen_reader_announce_inline_suggestion: true,
            tab_index: DEFAULT_EDITOR_TAB_INDEX,
            read_only: false,
            read_only_message: String::new(),
            dom_read_only: false,
            edit_context: true,
            render_rich_screen_reader_content: false,
            trim_whitespace_on_delete: false,
            unusual_line_terminators: EditorUnusualLineTerminators::default(),
            use_shadow_dom: true,
            use_tab_stops: true,
            fixed_overflow_widgets: false,
            allow_overflow: true,
            tab_width: 4,
            insert_spaces: true,
            detect_indentation: true,
            word_separators: DEFAULT_WORD_SEPARATORS.to_owned(),
            word_segmenter_locales: Vec::new(),
            auto_indent: true,
            auto_closing_brackets: true,
            auto_closing_quotes: true,
            experimental_gpu_acceleration: EditorExperimentalGpuAcceleration::default(),
            experimental_whitespace_rendering: EditorExperimentalWhitespaceRendering::default(),
            auto_closing_comments: EditorAutoClosingStrategy::default(),
            auto_closing_delete: EditorAutoClosingEditStrategy::default(),
            auto_closing_overtype: EditorAutoClosingEditStrategy::default(),
            auto_surround: true,
            auto_indent_on_paste: false,
            auto_indent_on_paste_within_string: true,
            sticky_tab_stops: false,
            linked_editing: false,
            rename_on_type: false,
            tab_focus_mode: false,
            vim_keybindings: false,
            vim: EditorVimSettings::default(),
            discord: DiscordSettings::default(),
            quick_suggestions: false,
            quick_suggestions_delay_ms: DEFAULT_QUICK_SUGGESTIONS_DELAY_MS,
            suggest_on_trigger_characters: true,
            accept_suggestion_on_enter: true,
            accept_suggestion_on_tab: false,
            accept_suggestion_on_commit_character: true,
            suggest_selection: EditorSuggestSelection::default(),
            suggest_insert_mode: EditorSuggestInsertMode::default(),
            suggest_filter_graceful: true,
            suggest_snippets_prevent_quick_suggestions: false,
            suggest_locality_bonus: false,
            suggest_share_suggest_selections: false,
            suggest_selection_mode: EditorSuggestSelectionMode::default(),
            suggest_show_icons: true,
            suggest_show_status_bar: false,
            suggest_preview: false,
            suggest_preview_mode: EditorSuggestPreviewMode::default(),
            suggest_show_inline_details: true,
            suggest_show_methods: true,
            suggest_show_functions: true,
            suggest_show_constructors: true,
            suggest_show_deprecated: true,
            suggest_show_fields: true,
            suggest_show_variables: true,
            suggest_show_classes: true,
            suggest_show_structs: true,
            suggest_show_interfaces: true,
            suggest_show_modules: true,
            suggest_show_properties: true,
            suggest_show_events: true,
            suggest_show_operators: true,
            suggest_show_units: true,
            suggest_show_values: true,
            suggest_show_constants: true,
            suggest_show_enums: true,
            suggest_show_enum_members: true,
            suggest_show_keywords: true,
            suggest_show_words: true,
            suggest_show_colors: true,
            suggest_show_files: true,
            suggest_show_references: true,
            suggest_show_customcolors: true,
            suggest_show_folders: true,
            suggest_show_type_parameters: true,
            suggest_show_snippets: true,
            suggest_show_users: true,
            suggest_show_issues: true,
            suggest_match_on_word_start_only: true,
            suggest_font_size: DEFAULT_SUGGEST_FONT_SIZE,
            suggest_line_height: DEFAULT_SUGGEST_LINE_HEIGHT,
            tab_completion: EditorTabCompletion::default(),
            snippet_suggestions: EditorSnippetSuggestions::default(),
            hover_enabled: true,
            hover_delay_ms: DEFAULT_HOVER_DELAY_MS,
            hover_hiding_delay_ms: DEFAULT_HOVER_HIDING_DELAY_MS,
            hover_sticky: true,
            hover_above: true,
            hover_show_long_line_warning: true,
            inline_suggest_enabled: true,
            inline_suggest_mode: EditorInlineSuggestMode::default(),
            inline_suggest_show_toolbar: EditorInlineSuggestShowToolbar::default(),
            inline_suggest_keep_on_blur: false,
            inline_suggest_font_family: DEFAULT_INLINE_SUGGEST_FONT_FAMILY.to_owned(),
            inline_suggest_syntax_highlighting_enabled: true,
            inline_suggest_suppress_suggestions: false,
            inline_suggest_suppress_in_snippet_mode: true,
            inline_suggest_min_show_delay_ms: DEFAULT_INLINE_SUGGEST_MIN_SHOW_DELAY_MS,
            inline_suggest_edits_enabled: true,
            inline_suggest_edits_show_collapsed: false,
            inline_suggest_edits_render_side_by_side:
                EditorInlineSuggestEditsRenderSideBySide::default(),
            inline_suggest_edits_allow_code_shifting:
                EditorInlineSuggestEditsAllowCodeShifting::default(),
            inline_suggest_edits_show_long_distance_hint: true,
            inline_suggest_trigger_command_on_provider_change: false,
            inline_suggest_experimental_suppress_inline_suggestions: String::new(),
            inline_suggest_experimental_show_on_suggest_conflict:
                EditorInlineSuggestShowOnSuggestConflict::default(),
            inline_suggest_experimental_empty_response_information: true,
            inline_completions_accessibility_verbose: false,
            lightbulb: EditorLightbulbMode::default(),
            render_validation_decorations: EditorRenderValidationDecorations::default(),
            document_highlights_enabled: true,
            code_lens: true,
            code_lens_font_family: DEFAULT_EDITOR_CODE_LENS_FONT_FAMILY.to_owned(),
            code_lens_font_size: DEFAULT_EDITOR_CODE_LENS_FONT_SIZE,
            goto_location_multiple_definitions: EditorGotoLocationMultiple::default(),
            goto_location_multiple_type_definitions: EditorGotoLocationMultiple::default(),
            goto_location_multiple_declarations: EditorGotoLocationMultiple::default(),
            goto_location_multiple_implementations: EditorGotoLocationMultiple::default(),
            goto_location_multiple_references: EditorGotoLocationMultiple::default(),
            goto_location_multiple_tests: EditorGotoLocationMultiple::default(),
            goto_location_alternative_definition_command:
                DEFAULT_GOTO_LOCATION_ALTERNATIVE_DEFINITION_COMMAND.to_owned(),
            goto_location_alternative_type_definition_command:
                DEFAULT_GOTO_LOCATION_ALTERNATIVE_TYPE_DEFINITION_COMMAND.to_owned(),
            goto_location_alternative_declaration_command:
                DEFAULT_GOTO_LOCATION_ALTERNATIVE_DECLARATION_COMMAND.to_owned(),
            goto_location_alternative_implementation_command:
                DEFAULT_GOTO_LOCATION_ALTERNATIVE_IMPLEMENTATION_COMMAND.to_owned(),
            goto_location_alternative_reference_command:
                DEFAULT_GOTO_LOCATION_ALTERNATIVE_REFERENCE_COMMAND.to_owned(),
            goto_location_alternative_tests_command:
                DEFAULT_GOTO_LOCATION_ALTERNATIVE_TESTS_COMMAND.to_owned(),
            peek_widget_default_focus: EditorPeekWidgetDefaultFocus::default(),
            placeholder: DEFAULT_EDITOR_PLACEHOLDER.to_owned(),
            definition_link_opens_in_peek: false,
            inlay_hints: true,
            inlay_hints_font_family: DEFAULT_EDITOR_INLAY_HINTS_FONT_FAMILY.to_owned(),
            inlay_hints_font_size: DEFAULT_EDITOR_INLAY_HINTS_FONT_SIZE,
            inlay_hints_padding: false,
            inlay_hints_maximum_length: DEFAULT_EDITOR_INLAY_HINTS_MAXIMUM_LENGTH,
            parameter_hints_enabled: true,
            parameter_hints_on_trigger_characters: true,
            parameter_hints_cycle: true,
            comments_insert_space: true,
            comments_ignore_empty_lines: true,
            format_on_save: false,
            format_on_type: false,
            format_on_paste: false,
            paste_as_enabled: true,
            paste_as_show_paste_selector: EditorPasteAsShowPasteSelector::default(),

            autosave: false,
            autosave_mode: EditorAutoSaveMode::AfterDelay,
            autosave_delay_ms: DEFAULT_AUTOSAVE_DELAY_MS,
            smooth_scrolling: true,
            scroll_beyond_last_line: true,
            scroll_beyond_last_column: DEFAULT_EDITOR_SCROLL_BEYOND_LAST_COLUMN,
            scroll_on_middle_click: false,
            scroll_predominant_axis: true,
            inertial_scroll: false,
            mouse_wheel_scroll_sensitivity: DEFAULT_EDITOR_MOUSE_WHEEL_SCROLL_SENSITIVITY,
            fast_scroll_sensitivity: DEFAULT_EDITOR_FAST_SCROLL_SENSITIVITY,
            mouse_wheel_zoom: false,
            scrollbar_vertical: EditorScrollbarVisibility::Hidden,
            scrollbar_horizontal: EditorScrollbarVisibility::Hidden,
            scrollbar_vertical_scrollbar_size: DEFAULT_EDITOR_SCROLLBAR_VERTICAL_SCROLLBAR_SIZE,
            scrollbar_horizontal_scrollbar_size: DEFAULT_EDITOR_SCROLLBAR_HORIZONTAL_SCROLLBAR_SIZE,
            scrollbar_ignore_horizontal_scrollbar_in_content_height: false,
            explorer_scrollbar: EditorScrollbarVisibility::Hidden,
            padding_top: DEFAULT_EDITOR_PADDING_TOP,
            padding_bottom: DEFAULT_EDITOR_PADDING_BOTTOM,
            links: true,
            show_unused: true,
            show_deprecated: true,
            contextmenu: true,
            color_decorators: true,
            color_decorators_activated_on: EditorColorDecoratorsActivatedOn::default(),
            color_decorators_limit: DEFAULT_EDITOR_COLOR_DECORATORS_LIMIT,
            default_color_decorators: EditorDefaultColorDecorators::default(),
            sticky_scroll: true,
            sticky_scroll_max_line_count: DEFAULT_EDITOR_STICKY_SCROLL_MAX_LINE_COUNT,
            sticky_scroll_default_model: EditorStickyScrollDefaultModel::default(),
            sticky_scroll_scroll_with_editor: true,
            line_height: DEFAULT_EDITOR_LINE_HEIGHT,
            minimap: false,
            minimap_side: EditorMinimapSide::default(),
            minimap_autohide: EditorMinimapAutohide::default(),
            minimap_size: EditorMinimapSize::default(),
            minimap_show_slider: EditorMinimapShowSlider::default(),
            minimap_scale: DEFAULT_EDITOR_MINIMAP_SCALE,
            minimap_render_characters: true,
            minimap_max_column: DEFAULT_EDITOR_MINIMAP_MAX_COLUMN,
            minimap_show_region_section_headers: true,
            minimap_show_mark_section_headers: true,
            minimap_mark_section_header_regex: DEFAULT_EDITOR_MINIMAP_MARK_SECTION_HEADER_REGEX
                .to_owned(),
            minimap_section_header_font_size: DEFAULT_EDITOR_MINIMAP_SECTION_HEADER_FONT_SIZE,
            minimap_section_header_letter_spacing:
                DEFAULT_EDITOR_MINIMAP_SECTION_HEADER_LETTER_SPACING,
            multi_cursor_modifier: EditorMultiCursorModifier::default(),
            multi_cursor_merge_overlapping: true,
            multi_cursor_paste: EditorMultiCursorPaste::default(),
            multi_cursor_limit: DEFAULT_EDITOR_MULTI_CURSOR_LIMIT,
            column_selection: false,
            mouse_middle_click_action: EditorMouseMiddleClickAction::default(),
            empty_selection_clipboard: true,
            selection_clipboard: true,
            copy_with_syntax_highlighting: true,
            double_click_selects_block: true,
            drag_and_drop: true,
            drop_into_editor_enabled: true,
            drop_into_editor_show_drop_selector: EditorDropIntoEditorShowDropSelector::default(),
            glyph_margin: true,
            ruler_column: DEFAULT_EDITOR_RULER_COLUMN,
            overview_ruler_border: true,
            overview_ruler_lanes: DEFAULT_EDITOR_OVERVIEW_RULER_LANES,
            hide_cursor_in_overview_ruler: false,
            status_bar_visible: true,
            devtools_verbose_logging: false,
            devtools_profiling_enabled: false,
            lsp_servers: default_server_configs(),
            lsp_suggest_missing_servers: false,
            window_zoom_level: DEFAULT_WINDOW_ZOOM_LEVEL,
            line_numbers: EditorLineNumbers::default(),
            line_decorations_width: EditorLineDecorationsWidth::default(),
            line_numbers_min_chars: DEFAULT_EDITOR_LINE_NUMBERS_MIN_CHARS,
            select_on_line_numbers: DEFAULT_EDITOR_SELECT_ON_LINE_NUMBERS,
            word_wrap: EditorWordWrap::default(),
            word_wrap_override1: EditorWordWrapOverride::default(),
            word_wrap_override2: EditorWordWrapOverride::default(),
            word_wrap_break_after_characters: DEFAULT_EDITOR_WORD_WRAP_BREAK_AFTER_CHARACTERS
                .to_owned(),
            word_wrap_break_before_characters: DEFAULT_EDITOR_WORD_WRAP_BREAK_BEFORE_CHARACTERS
                .to_owned(),
            word_wrap_column: DEFAULT_EDITOR_WORD_WRAP_COLUMN,
            wrapping_indent: EditorWrappingIndent::default(),
            wrapping_strategy: EditorWrappingStrategy::default(),
            wrap_on_escaped_line_feeds: false,
            word_break: EditorWordBreak::default(),
            reveal_horizontal_right_padding: DEFAULT_EDITOR_REVEAL_HORIZONTAL_RIGHT_PADDING,
            rounded_selection: true,
            stop_rendering_line_after: DEFAULT_EDITOR_STOP_RENDERING_LINE_AFTER,
            render_whitespace: EditorRenderWhitespace::default(),
            render_final_newline: EditorRenderFinalNewline::default(),
            render_control_characters: false,
            unicode_highlight_ambiguous_characters: true,
            unicode_highlight_invisible_characters: true,
            unicode_highlight_non_basic_ascii: EditorUnicodeHighlightNonBasicAscii::default(),
            unicode_highlight_include_comments: EditorUnicodeHighlightScope::InUntrustedWorkspace,
            unicode_highlight_include_strings: EditorUnicodeHighlightScope::On,
            unicode_highlight_allowed_characters: BTreeMap::new(),
            unicode_highlight_allowed_locales: BTreeMap::from([
                ("_os".to_owned(), true),
                ("_vscode".to_owned(), true),
            ]),
            render_line_highlight: EditorRenderLineHighlight::default(),
            render_line_highlight_only_when_focus: false,
            smart_select_select_leading_and_trailing_whitespace: true,
            smart_select_select_subwords: true,
            find_seed_search_string_from_selection:
                DEFAULT_EDITOR_FIND_SEED_SEARCH_STRING_FROM_SELECTION,
            find_auto_find_in_selection: DEFAULT_EDITOR_FIND_AUTO_FIND_IN_SELECTION,
            find_on_type: DEFAULT_EDITOR_FIND_ON_TYPE,
            find_cursor_move_on_type: DEFAULT_EDITOR_FIND_CURSOR_MOVE_ON_TYPE,
            find_loop: DEFAULT_EDITOR_FIND_LOOP,
            find_close_on_result: DEFAULT_EDITOR_FIND_CLOSE_ON_RESULT,
            find_global_find_clipboard: DEFAULT_EDITOR_FIND_GLOBAL_FIND_CLIPBOARD,
            find_add_extra_space_on_top: DEFAULT_EDITOR_FIND_ADD_EXTRA_SPACE_ON_TOP,
            find_history: EditorFindHistory::default(),
            find_replace_history: EditorFindHistory::default(),
            diff_ignore_trim_whitespace: true,
            diff_algorithm: DiffAlgorithm::default(),
            diff_render_side_by_side: true,
            diff_enable_split_view_resizing: true,
            diff_split_view_default_ratio: DEFAULT_DIFF_SPLIT_VIEW_DEFAULT_RATIO,
            diff_render_side_by_side_inline_breakpoint:
                DEFAULT_DIFF_RENDER_SIDE_BY_SIDE_INLINE_BREAKPOINT,
            diff_use_inline_view_when_space_is_limited: true,
            diff_compact_mode: false,
            diff_original_editable: false,
            diff_code_lens: false,
            diff_accessibility_verbose: false,
            diff_hide_unchanged_regions: false,
            diff_context_lines: DEFAULT_DIFF_CONTEXT_LINES,
            diff_hide_unchanged_regions_minimum_line_count:
                DEFAULT_DIFF_HIDE_UNCHANGED_REGIONS_MINIMUM_LINE_COUNT,
            diff_hide_unchanged_regions_reveal_line_count:
                DEFAULT_DIFF_HIDE_UNCHANGED_REGIONS_REVEAL_LINE_COUNT,
            diff_max_computation_time_ms: DEFAULT_DIFF_MAX_COMPUTATION_TIME_MS,
            diff_max_file_size_mb: DEFAULT_DIFF_MAX_FILE_SIZE_MB,
            diff_render_gutter_menu: true,
            diff_render_indicators: true,
            diff_render_margin_revert_icon: true,
            diff_render_overview_ruler: true,
            diff_experimental_show_moves: false,
            diff_experimental_show_empty_decorations: true,
            diff_experimental_use_true_inline_view: false,
            diff_word_wrap: DiffWordWrap::default(),
            diff_only_show_accessible_viewer: false,
            diff_is_in_embedded_editor: false,
            background_image_enabled: false,
            background_image_path: None,
            background_image_scope: EditorBackgroundImageScope::default(),
            background_image_dim: DEFAULT_EDITOR_BACKGROUND_IMAGE_DIM,
            background_image_fit: EditorBackgroundImageFit::default(),
            background_image_position: EditorBackgroundImagePosition::default(),
            background_image_loop: true,
            project_index_max_files: DEFAULT_PROJECT_INDEX_MAX_FILES,
            project_index_exclude_globs: default_project_index_exclude_globs(),
            project_index_include_hidden_dirs: false,
            project_search_exclude_globs: default_project_search_exclude_globs(),
            project_search_max_file_size_mb: DEFAULT_PROJECT_SEARCH_MAX_FILE_SIZE_MB,
            project_search_max_results: DEFAULT_PROJECT_SEARCH_MAX_RESULTS,
            git_enabled: true,
            git_add_ai_co_author: GitAddAiCoAuthor::default(),
            git_allow_force_push: false,
            git_allow_no_verify_commit: false,
            git_auto_repository_detection: GitAutoRepositoryDetection::default(),
            git_autofetch: GitAutoFetch::default(),
            git_autofetch_period: DEFAULT_GIT_AUTOFETCH_PERIOD,
            git_autorefresh: true,
            git_auto_stash: false,
            git_commands_to_log: Vec::new(),
            git_confirm_force_push: true,
            git_confirm_no_verify_commit: true,
            git_confirm_sync: true,
            git_ignore_limit_warning: false,
            git_ignore_submodules: false,
            git_ignored_repositories: Vec::new(),
            git_repository_scan_ignored_folders: vec!["node_modules".to_owned()],
            git_open_repository_in_parent_folders: GitOpenRepositoryInParentFolders::default(),
            git_detect_submodules: true,
            git_detect_submodules_limit: DEFAULT_GIT_DETECT_SUBMODULES_LIMIT,
            git_repository_scan_max_depth: DEFAULT_GIT_REPOSITORY_SCAN_MAX_DEPTH,
            git_detect_worktrees: false,
            git_detect_worktrees_limit: DEFAULT_GIT_DETECT_WORKTREES_LIMIT,
            git_discard_untracked_changes_to_trash: true,
            git_diagnostics_commit_hook_enabled: false,
            git_diagnostics_commit_hook_sources: BTreeMap::from([(
                "*".to_owned(),
                "error".to_owned(),
            )]),
            git_enable_commit_signing: false,
            git_enable_status_bar_sync: true,
            git_fetch_on_pull: false,
            git_follow_tags_when_sync: false,
            git_ignore_legacy_warning: false,
            git_ignore_missing_git_warning: false,
            git_ignore_rebase_warning: false,
            git_ignore_windows_git27_warning: false,
            git_merge_editor: false,
            git_open_after_clone: GitOpenAfterClone::default(),
            git_optimistic_update: true,
            git_path: Vec::new(),
            git_post_commit_command: GitPostCommitCommand::default(),
            git_prune_on_fetch: false,
            git_pull_before_checkout: false,
            git_pull_tags: true,
            git_rebase_when_sync: false,
            git_remember_post_commit_command: false,
            git_replace_tags_when_pull: false,
            git_support_cancellation: false,
            git_terminal_authentication: true,
            git_terminal_git_editor: false,
            git_use_force_push_if_includes: true,
            git_use_force_push_with_lease: true,
            git_use_integrated_ask_pass: true,
            git_worktree_include_files: Vec::new(),
            git_default_branch_name: DEFAULT_GIT_DEFAULT_BRANCH_NAME.to_owned(),
            git_default_clone_directory: None,
            git_similarity_threshold: DEFAULT_GIT_SIMILARITY_THRESHOLD,
            scm_default_view_mode: ScmDefaultViewMode::default(),
            scm_default_view_sort_key: ScmDefaultViewSortKey::default(),
            scm_auto_reveal: DEFAULT_SCM_AUTO_REVEAL,
            scm_count_badge: ScmCountBadge::default(),
            scm_provider_count_badge: ScmProviderCountBadge::default(),
            scm_always_show_repositories: false,
            scm_repositories_visible: DEFAULT_SCM_REPOSITORIES_VISIBLE,
            scm_compact_folders: true,
            scm_always_show_actions: false,
            scm_show_action_button: true,
            git_show_commit_input: true,
            git_show_push_success_notification: false,
            git_use_editor_as_commit_input: true,
            git_verbose_commit: false,
            git_show_action_button_commit: true,
            git_always_sign_off: false,
            git_confirm_committed_delete: true,
            git_confirm_empty_commits: true,
            git_require_user_config: true,
            git_show_progress: true,
            git_show_reference_details: true,
            git_timeline_show_author: true,
            git_timeline_show_uncommitted: false,
            git_timeline_date: GitTimelineDate::default(),
            git_show_inline_open_file_action: true,
            git_count_badge: GitCountBadge::default(),
            git_untracked_changes: GitUntrackedChanges::default(),
            git_open_diff_on_click: true,
            git_close_diff_on_operation: false,
            git_always_show_staged_changes_resource_group: false,
            git_checkout_type: vec![
                GitCheckoutType::Local,
                GitCheckoutType::Remote,
                GitCheckoutType::Tags,
            ],
            git_branch_sort_order: GitBranchSortOrder::default(),
            git_branch_prefix: DEFAULT_GIT_BRANCH_PREFIX.to_owned(),
            git_branch_random_name_enable: false,
            git_branch_random_name_dictionary: vec!["adjectives".to_owned(), "animals".to_owned()],
            git_branch_validation_regex: DEFAULT_GIT_BRANCH_VALIDATION_REGEX.to_owned(),
            git_branch_whitespace_char: DEFAULT_GIT_BRANCH_WHITESPACE_CHAR.to_owned(),
            git_decorations_enabled: true,
            git_enable_smart_commit: false,
            git_suggest_smart_commit: true,
            git_smart_commit_changes: GitSmartCommitChanges::default(),
            git_prompt_to_save_files_before_commit: GitPromptToSaveFilesBeforeCommit::default(),
            git_prompt_to_save_files_before_stash: GitPromptToSaveFilesBeforeCommit::default(),
            git_branch_protection: Vec::new(),
            git_branch_protection_prompt: GitBranchProtectionPrompt::default(),
            git_status_limit: DEFAULT_GIT_STATUS_LIMIT,
            git_use_commit_input_as_stash_message: false,
            git_commit_short_hash_length: DEFAULT_GIT_COMMIT_SHORT_HASH_LENGTH,
            git_input_validation: false,
            git_input_validation_length: DEFAULT_GIT_INPUT_VALIDATION_LENGTH,
            git_input_validation_subject_length: GitInputValidationSubjectLength::default(),
            git_blame_status_bar_item_enabled: true,
            git_blame_editor_decoration_enabled: false,
            git_blame_editor_decoration_disable_hover: false,
            git_blame_ignore_whitespace: false,
            git_blame_status_bar_item_template: DEFAULT_GIT_BLAME_STATUS_BAR_ITEM_TEMPLATE
                .to_owned(),
            git_blame_editor_decoration_template: DEFAULT_GIT_BLAME_EDITOR_DECORATION_TEMPLATE
                .to_owned(),
            scm_show_input_action_button: true,
            scm_input_min_line_count: DEFAULT_SCM_INPUT_MIN_LINE_COUNT,
            scm_input_max_line_count: DEFAULT_SCM_INPUT_MAX_LINE_COUNT,
            scm_input_font_family: DEFAULT_SCM_INPUT_FONT_FAMILY.to_owned(),
            scm_input_font_size: DEFAULT_SCM_INPUT_FONT_SIZE,
            scm_diff_decorations: ScmDiffDecorations::default(),
            scm_diff_decorations_gutter_action: ScmDiffDecorationsGutterAction::default(),
            scm_diff_decorations_gutter_visibility: ScmDiffDecorationsGutterVisibility::default(),
            scm_diff_decorations_gutter_width: DEFAULT_SCM_DIFF_DECORATIONS_GUTTER_WIDTH,
            scm_diff_decorations_gutter_pattern: ScmDiffDecorationsGutterPattern::default(),
            scm_diff_decorations_ignore_trim_whitespace:
                ScmDiffDecorationsIgnoreTrimWhitespace::default(),
            scm_graph_page_on_scroll: DEFAULT_SCM_GRAPH_PAGE_ON_SCROLL,
            scm_graph_page_size: DEFAULT_SCM_GRAPH_PAGE_SIZE,
            scm_graph_badges: ScmGraphBadges::default(),
            scm_graph_show_incoming_changes: true,
            scm_graph_show_outgoing_changes: true,
            bracket_pair_colorization: true,
            bracket_pair_colorization_independent_color_pool_per_bracket_type: false,
            bracket_pair_guides: EditorBracketPairGuideMode::default(),
            bracket_pair_guides_horizontal: EditorBracketPairGuideMode::Off,
            highlight_active_bracket_pair: true,
            match_brackets: EditorMatchBrackets::default(),
            folding: true,
            folding_highlight: true,
            folding_imports_by_default: true,
            folding_maximum_regions: DEFAULT_EDITOR_FOLDING_MAXIMUM_REGIONS,
            folding_strategy: EditorFoldingStrategy::default(),
            unfold_on_click_after_end_of_line: false,
            show_folding_controls: EditorShowFoldingControls::default(),
            indent_guides: true,
            highlight_active_indentation: EditorHighlightActiveIndentation::default(),
            mouse_style: EditorMouseStyle::default(),
            cursor_smooth_caret_animation: EditorCursorSmoothCaretAnimation::default(),
            cursor_style: EditorCursorStyle::default(),
            overtype_cursor_style: EditorCursorStyle::Block,
            overtype_on_paste: true,
            cursor_blinking: false,
            cursor_width: DEFAULT_EDITOR_CURSOR_WIDTH,
            cursor_height: DEFAULT_EDITOR_CURSOR_HEIGHT,
            cursor_surrounding_lines: DEFAULT_EDITOR_CURSOR_SURROUNDING_LINES,
            cursor_surrounding_lines_style: EditorCursorSurroundingLinesStyle::default(),
            terminal_scrollback_rows: DEFAULT_TERMINAL_SCROLLBACK_ROWS,
            terminal_shell_path: None,
            terminal_shell_args: Vec::new(),
            terminal_cwd: None,
            terminal_split_cwd: TerminalSplitCwd::default(),
            terminal_min_rows: DEFAULT_TERMINAL_MIN_ROWS,
            terminal_min_columns: DEFAULT_TERMINAL_MIN_COLUMNS,
            terminal_font_size: DEFAULT_TERMINAL_FONT_SIZE,
            terminal_line_height: DEFAULT_TERMINAL_LINE_HEIGHT,
            terminal_letter_spacing: DEFAULT_TERMINAL_LETTER_SPACING,
            terminal_cursor_style: TerminalCursorStyle::default(),
            terminal_cursor_width: DEFAULT_TERMINAL_CURSOR_WIDTH,
            terminal_cursor_blinking: false,
            terminal_cursor_style_inactive: TerminalInactiveCursorStyle::default(),
            terminal_draw_bold_text_in_bright_colors: true,
            terminal_minimum_contrast_ratio: DEFAULT_TERMINAL_MINIMUM_CONTRAST_RATIO,
            terminal_enable_bell: DEFAULT_TERMINAL_ENABLE_BELL,
            terminal_bell_duration_ms: DEFAULT_TERMINAL_BELL_DURATION_MS,
            terminal_show_exit_alert: DEFAULT_TERMINAL_SHOW_EXIT_ALERT,
            terminal_hide_on_startup: TerminalHideOnStartup::default(),
            terminal_hide_on_last_closed: DEFAULT_TERMINAL_HIDE_ON_LAST_CLOSED,
            terminal_confirm_on_exit: TerminalConfirmOnExit::default(),
            terminal_confirm_on_kill: TerminalConfirmOnKill::default(),
            terminal_tabs_enabled: DEFAULT_TERMINAL_TABS_ENABLED,
            terminal_tabs_default_icon: DEFAULT_TERMINAL_TABS_DEFAULT_ICON.to_owned(),
            terminal_tabs_default_color: None,
            terminal_tabs_allow_agent_cli_title: DEFAULT_TERMINAL_TABS_ALLOW_AGENT_CLI_TITLE,
            terminal_tabs_title: DEFAULT_TERMINAL_TABS_TITLE.to_owned(),
            terminal_tabs_hide_condition: TerminalTabsHideCondition::default(),
            terminal_tabs_show_active_terminal: TerminalTabsShowActiveTerminal::default(),
            terminal_tabs_show_actions: TerminalTabsShowActions::default(),
            terminal_tabs_focus_mode: TerminalTabsFocusMode::default(),
            terminal_tabs_location: TerminalTabsLocation::default(),
            terminal_right_click_behavior: TerminalRightClickBehavior::default(),
            terminal_middle_click_behavior: TerminalMiddleClickBehavior::default(),
            terminal_alt_click_moves_cursor: DEFAULT_TERMINAL_ALT_CLICK_MOVES_CURSOR,
            terminal_copy_on_selection: DEFAULT_TERMINAL_COPY_ON_SELECTION,
            terminal_ignore_bracketed_paste_mode: DEFAULT_TERMINAL_IGNORE_BRACKETED_PASTE_MODE,
            terminal_enable_multi_line_paste_warning: TerminalMultiLinePasteWarning::default(),
            terminal_word_separators: DEFAULT_TERMINAL_WORD_SEPARATORS.to_owned(),
            terminal_mouse_wheel_scroll_sensitivity:
                DEFAULT_TERMINAL_MOUSE_WHEEL_SCROLL_SENSITIVITY,
            terminal_fast_scroll_sensitivity: DEFAULT_TERMINAL_FAST_SCROLL_SENSITIVITY,
            terminal_mouse_wheel_zoom: DEFAULT_TERMINAL_MOUSE_WHEEL_ZOOM,
            keymap: Keymap::default(),
            trim_trailing_whitespace: false,
            insert_final_newline: false,
            trim_final_newlines: false,
            updates_github_repository: String::new(),
            theme: ThemeSettings::default(),
            custom_theme_paths: Vec::new(),
            active_custom_theme_path: None,
            plugins: PluginSettings::default(),
        }
    }
}

impl EditorSettings {
    pub fn effective_autosave_mode(&self) -> EditorAutoSaveMode {
        if self.autosave {
            self.autosave_mode
        } else {
            EditorAutoSaveMode::Off
        }
    }

    pub fn lsp_server_configs(&self) -> Vec<LspServerConfig> {
        effective_lsp_server_configs(&self.lsp_servers)
            .into_iter()
            .filter(|server| server.enabled)
            .collect()
    }

    pub(super) fn sanitize(&mut self) -> bool {
        let mut changed = false;

        macro_rules! field {
            ($field:ident, $value:expr $(,)?) => {{
                let value = $value;
                changed |= replace_if_changed(&mut self.$field, value);
            }};
        }

        field!(font_size, clamp_editor_font_size(self.font_size, 13.0),);
        field!(
            ui_font_size,
            clamp_editor_font_size(self.ui_font_size, 13.0),
        );
        changed |= sanitize_settings_optional_string(&mut self.editor_font_path);
        changed |= sanitize_settings_optional_string(&mut self.ui_font_path);
        changed |= sanitize_settings_plain_string_with_default(
            &mut self.font_family,
            DEFAULT_EDITOR_FONT_FAMILY,
        );
        field!(font_weight, sanitize_editor_font_weight(&self.font_weight),);
        changed |= sanitize_settings_plain_string(&mut self.font_ligatures);
        changed |= sanitize_settings_plain_string(&mut self.font_variations);
        field!(
            letter_spacing,
            clamp_editor_letter_spacing(self.letter_spacing),
        );
        changed |= sanitize_settings_plain_string(&mut self.extra_editor_class_name);
        field!(
            accessibility_page_size,
            clamp_editor_accessibility_page_size(self.accessibility_page_size),
        );
        changed |=
            sanitize_settings_display_string(&mut self.aria_label, Some(DEFAULT_EDITOR_ARIA_LABEL));
        field!(tab_index, clamp_editor_tab_index(self.tab_index));
        changed |= sanitize_settings_display_string(&mut self.read_only_message, None);
        changed |= sanitize_settings_plain_string(&mut self.word_separators);
        changed |= sanitize_settings_string_list(
            &mut self.word_segmenter_locales,
            SETTINGS_LIST_MAX_ITEMS,
            SETTINGS_MAP_KEY_MAX_CHARS,
            true,
        );
        changed |= self.vim.sanitize();
        changed |= self.discord.sanitize();

        field!(
            quick_suggestions_delay_ms,
            clamp_quick_suggestions_delay_ms(self.quick_suggestions_delay_ms),
        );
        field!(
            suggest_font_size,
            clamp_suggest_font_size(self.suggest_font_size),
        );
        field!(
            suggest_line_height,
            clamp_suggest_line_height(self.suggest_line_height),
        );
        field!(hover_delay_ms, clamp_hover_delay_ms(self.hover_delay_ms),);
        field!(
            hover_hiding_delay_ms,
            clamp_hover_hiding_delay_ms(self.hover_hiding_delay_ms),
        );
        changed |= sanitize_settings_plain_string(&mut self.inline_suggest_font_family);
        field!(
            inline_suggest_min_show_delay_ms,
            clamp_inline_suggest_min_show_delay_ms(self.inline_suggest_min_show_delay_ms),
        );
        changed |= sanitize_settings_plain_string(
            &mut self.inline_suggest_experimental_suppress_inline_suggestions,
        );
        changed |= sanitize_settings_plain_string(&mut self.code_lens_font_family);
        field!(
            code_lens_font_size,
            clamp_editor_code_lens_font_size(self.code_lens_font_size),
        );
        changed |=
            sanitize_settings_plain_string(&mut self.goto_location_alternative_definition_command);
        changed |= sanitize_settings_plain_string(
            &mut self.goto_location_alternative_type_definition_command,
        );
        changed |=
            sanitize_settings_plain_string(&mut self.goto_location_alternative_declaration_command);
        changed |= sanitize_settings_plain_string(
            &mut self.goto_location_alternative_implementation_command,
        );
        changed |=
            sanitize_settings_plain_string(&mut self.goto_location_alternative_reference_command);
        changed |=
            sanitize_settings_plain_string(&mut self.goto_location_alternative_tests_command);
        changed |= sanitize_settings_display_string(&mut self.placeholder, None);
        changed |= sanitize_settings_plain_string(&mut self.inlay_hints_font_family);
        field!(
            inlay_hints_font_size,
            clamp_editor_inlay_hints_font_size(self.inlay_hints_font_size),
        );
        field!(
            inlay_hints_maximum_length,
            clamp_editor_inlay_hints_maximum_length(self.inlay_hints_maximum_length),
        );
        field!(
            autosave_delay_ms,
            clamp_autosave_delay_ms(self.autosave_delay_ms),
        );

        field!(
            scroll_beyond_last_column,
            clamp_editor_scroll_beyond_last_column(self.scroll_beyond_last_column),
        );
        field!(
            mouse_wheel_scroll_sensitivity,
            clamp_editor_scroll_sensitivity(
                self.mouse_wheel_scroll_sensitivity,
                DEFAULT_EDITOR_MOUSE_WHEEL_SCROLL_SENSITIVITY,
            ),
        );
        field!(
            fast_scroll_sensitivity,
            clamp_editor_scroll_sensitivity(
                self.fast_scroll_sensitivity,
                DEFAULT_EDITOR_FAST_SCROLL_SENSITIVITY,
            ),
        );
        field!(
            scrollbar_vertical_scrollbar_size,
            clamp_editor_scrollbar_size(self.scrollbar_vertical_scrollbar_size),
        );
        field!(
            scrollbar_horizontal_scrollbar_size,
            clamp_editor_scrollbar_size(self.scrollbar_horizontal_scrollbar_size),
        );
        field!(padding_top, clamp_editor_padding(self.padding_top),);
        field!(padding_bottom, clamp_editor_padding(self.padding_bottom),);
        field!(
            color_decorators_limit,
            clamp_editor_color_decorators_limit(self.color_decorators_limit),
        );
        field!(
            sticky_scroll_max_line_count,
            clamp_editor_sticky_scroll_max_line_count(self.sticky_scroll_max_line_count),
        );
        field!(line_height, clamp_editor_line_height(self.line_height),);
        field!(
            minimap_scale,
            clamp_editor_minimap_scale(self.minimap_scale),
        );
        field!(
            minimap_max_column,
            clamp_editor_minimap_max_column(self.minimap_max_column),
        );
        changed |= sanitize_settings_plain_string_with_default(
            &mut self.minimap_mark_section_header_regex,
            DEFAULT_EDITOR_MINIMAP_MARK_SECTION_HEADER_REGEX,
        );
        field!(
            minimap_section_header_font_size,
            clamp_editor_minimap_section_header_font_size(self.minimap_section_header_font_size),
        );
        field!(
            minimap_section_header_letter_spacing,
            clamp_editor_minimap_section_header_letter_spacing(
                self.minimap_section_header_letter_spacing,
            ),
        );
        field!(
            multi_cursor_limit,
            clamp_editor_multi_cursor_limit(self.multi_cursor_limit),
        );
        field!(ruler_column, clamp_editor_ruler_column(self.ruler_column),);
        field!(
            overview_ruler_lanes,
            clamp_editor_overview_ruler_lanes(self.overview_ruler_lanes),
        );
        field!(
            window_zoom_level,
            clamp_window_zoom_level(self.window_zoom_level),
        );
        changed |= sanitize_lsp_server_configs(&mut self.lsp_servers);
        field!(
            line_decorations_width,
            self.line_decorations_width.clamped(),
        );
        field!(
            line_numbers_min_chars,
            clamp_editor_line_numbers_min_chars(self.line_numbers_min_chars),
        );
        field!(
            word_wrap_column,
            clamp_editor_word_wrap_column(self.word_wrap_column),
        );
        field!(
            reveal_horizontal_right_padding,
            clamp_editor_reveal_horizontal_right_padding(self.reveal_horizontal_right_padding),
        );
        field!(
            stop_rendering_line_after,
            clamp_editor_stop_rendering_line_after(self.stop_rendering_line_after),
        );
        changed |= sanitize_settings_bool_map(&mut self.unicode_highlight_allowed_characters);
        changed |= sanitize_settings_bool_map(&mut self.unicode_highlight_allowed_locales);

        field!(
            diff_context_lines,
            crate::git::clamp_diff_context_lines(self.diff_context_lines),
        );
        field!(
            diff_hide_unchanged_regions_minimum_line_count,
            crate::git::clamp_diff_hide_unchanged_regions_minimum_line_count(
                self.diff_hide_unchanged_regions_minimum_line_count,
            ),
        );
        field!(
            diff_hide_unchanged_regions_reveal_line_count,
            crate::git::clamp_diff_hide_unchanged_regions_reveal_line_count(
                self.diff_hide_unchanged_regions_reveal_line_count,
            ),
        );
        field!(
            diff_max_computation_time_ms,
            crate::git::clamp_diff_max_computation_time_ms(self.diff_max_computation_time_ms),
        );
        field!(
            diff_max_file_size_mb,
            crate::git::clamp_diff_max_file_size_mb(self.diff_max_file_size_mb),
        );
        field!(
            diff_split_view_default_ratio,
            clamp_diff_split_view_default_ratio(self.diff_split_view_default_ratio),
        );
        field!(
            diff_render_side_by_side_inline_breakpoint,
            clamp_diff_render_side_by_side_inline_breakpoint(
                self.diff_render_side_by_side_inline_breakpoint,
            ),
        );
        changed |= sanitize_settings_optional_string(&mut self.background_image_path);
        field!(
            background_image_dim,
            clamp_editor_background_image_dim(self.background_image_dim),
        );
        field!(
            project_index_max_files,
            clamp_project_index_max_files(self.project_index_max_files),
        );
        changed |= sanitize_settings_string_list(
            &mut self.project_index_exclude_globs,
            SETTINGS_LIST_MAX_ITEMS,
            SETTINGS_MAP_KEY_MAX_CHARS,
            true,
        );
        changed |= sanitize_settings_string_list(
            &mut self.project_search_exclude_globs,
            SETTINGS_LIST_MAX_ITEMS,
            SETTINGS_MAP_KEY_MAX_CHARS,
            true,
        );
        field!(
            project_search_max_file_size_mb,
            clamp_project_search_max_file_size_mb(self.project_search_max_file_size_mb),
        );
        field!(
            project_search_max_results,
            clamp_project_search_max_results(self.project_search_max_results),
        );

        changed |= sanitize_settings_string_list(
            &mut self.git_commands_to_log,
            SETTINGS_LIST_MAX_ITEMS,
            SETTINGS_MAP_KEY_MAX_CHARS,
            true,
        );
        changed |= sanitize_settings_string_list(
            &mut self.git_ignored_repositories,
            SETTINGS_LIST_MAX_ITEMS,
            SETTINGS_STRING_MAX_CHARS,
            true,
        );
        changed |= sanitize_settings_string_list(
            &mut self.git_repository_scan_ignored_folders,
            SETTINGS_LIST_MAX_ITEMS,
            SETTINGS_STRING_MAX_CHARS,
            true,
        );
        field!(
            git_detect_submodules_limit,
            crate::git::clamp_git_detect_submodules_limit(self.git_detect_submodules_limit),
        );
        field!(
            git_repository_scan_max_depth,
            clamp_git_repository_scan_max_depth(self.git_repository_scan_max_depth),
        );
        field!(
            git_detect_worktrees_limit,
            clamp_git_detect_worktrees_limit(self.git_detect_worktrees_limit),
        );
        changed |= sanitize_settings_string_map(&mut self.git_diagnostics_commit_hook_sources);
        changed |= sanitize_settings_string_list(
            &mut self.git_path,
            SETTINGS_LIST_MAX_ITEMS,
            SETTINGS_STRING_MAX_CHARS,
            true,
        );
        changed |= sanitize_settings_string_list(
            &mut self.git_worktree_include_files,
            SETTINGS_LIST_MAX_ITEMS,
            SETTINGS_STRING_MAX_CHARS,
            true,
        );
        changed |= sanitize_settings_plain_string(&mut self.git_default_branch_name);
        changed |= sanitize_settings_optional_string(&mut self.git_default_clone_directory);
        field!(
            git_similarity_threshold,
            crate::git::clamp_git_similarity_threshold(self.git_similarity_threshold),
        );
        field!(
            scm_repositories_visible,
            clamp_scm_repositories_visible(self.scm_repositories_visible),
        );
        changed |=
            sanitize_settings_enum_list(&mut self.git_checkout_type, SETTINGS_LIST_MAX_ITEMS);
        changed |= sanitize_settings_plain_string(&mut self.git_branch_prefix);
        changed |= sanitize_settings_string_list(
            &mut self.git_branch_random_name_dictionary,
            SETTINGS_LIST_MAX_ITEMS,
            SETTINGS_MAP_KEY_MAX_CHARS,
            true,
        );
        changed |= sanitize_settings_plain_string(&mut self.git_branch_validation_regex);
        changed |= sanitize_settings_plain_string(&mut self.git_branch_whitespace_char);
        changed |= sanitize_settings_string_list(
            &mut self.git_branch_protection,
            SETTINGS_LIST_MAX_ITEMS,
            SETTINGS_STRING_MAX_CHARS,
            true,
        );
        field!(
            git_status_limit,
            crate::git::clamp_git_status_limit(self.git_status_limit),
        );
        field!(
            git_commit_short_hash_length,
            crate::git::clamp_git_commit_short_hash_length(self.git_commit_short_hash_length),
        );
        field!(
            git_input_validation_length,
            clamp_git_input_validation_length(self.git_input_validation_length),
        );
        changed |=
            sanitize_settings_display_string(&mut self.git_blame_status_bar_item_template, None);
        changed |=
            sanitize_settings_display_string(&mut self.git_blame_editor_decoration_template, None);
        field!(
            scm_input_min_line_count,
            clamp_scm_input_line_count(self.scm_input_min_line_count),
        );
        field!(
            scm_input_max_line_count,
            clamp_scm_input_line_count(self.scm_input_max_line_count),
        );
        if self.scm_input_min_line_count > self.scm_input_max_line_count {
            self.scm_input_max_line_count = self.scm_input_min_line_count;
            changed = true;
        }
        changed |= sanitize_settings_plain_string(&mut self.scm_input_font_family);
        field!(
            scm_input_font_size,
            clamp_scm_input_font_size(self.scm_input_font_size),
        );
        field!(
            scm_diff_decorations_gutter_width,
            clamp_scm_diff_decorations_gutter_width(self.scm_diff_decorations_gutter_width),
        );
        field!(
            scm_graph_page_size,
            clamp_scm_graph_page_size(self.scm_graph_page_size),
        );
        field!(
            folding_maximum_regions,
            clamp_editor_folding_maximum_regions(self.folding_maximum_regions),
        );
        field!(cursor_width, clamp_editor_cursor_width(self.cursor_width),);
        field!(
            cursor_height,
            clamp_editor_cursor_height(self.cursor_height),
        );
        field!(
            cursor_surrounding_lines,
            clamp_editor_cursor_surrounding_lines(self.cursor_surrounding_lines),
        );

        field!(
            terminal_scrollback_rows,
            clamp_terminal_scrollback_rows(self.terminal_scrollback_rows),
        );
        changed |= sanitize_settings_optional_string(&mut self.terminal_shell_path);
        changed |= sanitize_settings_string_list(
            &mut self.terminal_shell_args,
            SETTINGS_LIST_MAX_ITEMS,
            SETTINGS_STRING_MAX_CHARS,
            false,
        );
        changed |= sanitize_settings_optional_string(&mut self.terminal_cwd);
        field!(
            terminal_min_rows,
            clamp_terminal_min_rows(self.terminal_min_rows),
        );
        field!(
            terminal_min_columns,
            clamp_terminal_min_columns(self.terminal_min_columns),
        );
        field!(
            terminal_font_size,
            clamp_terminal_font_size(self.terminal_font_size),
        );
        field!(
            terminal_line_height,
            clamp_terminal_line_height(self.terminal_line_height),
        );
        field!(
            terminal_letter_spacing,
            clamp_terminal_letter_spacing(self.terminal_letter_spacing),
        );
        field!(
            terminal_cursor_width,
            clamp_terminal_cursor_width(self.terminal_cursor_width),
        );
        field!(
            terminal_minimum_contrast_ratio,
            clamp_terminal_minimum_contrast_ratio(self.terminal_minimum_contrast_ratio),
        );
        field!(
            terminal_bell_duration_ms,
            clamp_terminal_bell_duration_ms(self.terminal_bell_duration_ms),
        );
        changed |= sanitize_settings_display_string(
            &mut self.terminal_tabs_default_icon,
            Some(DEFAULT_TERMINAL_TABS_DEFAULT_ICON),
        );
        changed |= sanitize_settings_optional_display_string(&mut self.terminal_tabs_default_color);
        changed |= sanitize_settings_display_string(
            &mut self.terminal_tabs_title,
            Some(DEFAULT_TERMINAL_TABS_TITLE),
        );
        field!(
            terminal_mouse_wheel_scroll_sensitivity,
            clamp_terminal_scroll_sensitivity(
                self.terminal_mouse_wheel_scroll_sensitivity,
                DEFAULT_TERMINAL_MOUSE_WHEEL_SCROLL_SENSITIVITY,
            ),
        );
        field!(
            terminal_fast_scroll_sensitivity,
            clamp_terminal_scroll_sensitivity(
                self.terminal_fast_scroll_sensitivity,
                DEFAULT_TERMINAL_FAST_SCROLL_SENSITIVITY,
            ),
        );

        changed |= self.keymap.sanitize() > 0;
        changed |= sanitize_settings_plain_string(&mut self.updates_github_repository);
        changed |= sanitize_settings_string_list(
            &mut self.custom_theme_paths,
            SETTINGS_LIST_MAX_ITEMS,
            SETTINGS_STRING_MAX_CHARS,
            true,
        );
        changed |= sanitize_settings_optional_string(&mut self.active_custom_theme_path);
        if let Some(active_path) = self.active_custom_theme_path.as_deref()
            && !self
                .custom_theme_paths
                .iter()
                .any(|path| path == active_path)
        {
            self.active_custom_theme_path = None;
            changed = true;
        }

        changed |= self.plugins.sanitize();

        changed
    }

    pub fn load_or_create(path: &Path) -> anyhow::Result<Self> {
        let text = match read_settings_text_with_limit(path) {
            Ok(text) => text,
            Err(error) if settings_read_error_is_not_found(&error) => {
                return Self::create_default_settings(path);
            }
            Err(error) => return Err(error),
        };
        let (settings, should_save_migration) = parse_settings_text_with_known_recovery(&text)?;
        if should_save_migration {
            settings.save(path)?;
        }
        Ok(settings)
    }

    pub fn load_or_create_with_recovery(path: &Path) -> anyhow::Result<EditorSettingsLoad> {
        Self::load_or_create_with_recovery_and_quarantine(path, quarantine_corrupt_settings)
    }

    pub(super) fn load_or_create_with_recovery_and_quarantine(
        path: &Path,
        mut quarantine: impl FnMut(&Path) -> anyhow::Result<PathBuf>,
    ) -> anyhow::Result<EditorSettingsLoad> {
        let text = match read_settings_text_with_limit(path) {
            Ok(text) => text,
            Err(error) if settings_read_error_is_not_found(&error) => {
                return Self::create_default_settings_load(path);
            }
            Err(_) => return Self::recover_default_settings_with(path, &mut quarantine),
        };
        let loaded = parse_settings_text_with_known_recovery(&text);

        match loaded {
            Ok((settings, should_save_migration)) => {
                if should_save_migration {
                    settings.save(path)?;
                }
                Ok(EditorSettingsLoad {
                    settings,
                    quarantined_path: None,
                    future_schema_version: None,
                })
            }
            Err(_) => {
                if let Some(version) = settings_schema_version_newer_than_supported(&text) {
                    return Ok(EditorSettingsLoad {
                        settings: Self::default(),
                        quarantined_path: None,
                        future_schema_version: Some(version),
                    });
                }
                Self::recover_default_settings_with(path, quarantine)
            }
        }
    }

    fn create_default_settings(path: &Path) -> anyhow::Result<Self> {
        let settings = Self::default();
        settings.save(path)?;
        Ok(settings)
    }

    fn create_default_settings_load(path: &Path) -> anyhow::Result<EditorSettingsLoad> {
        Ok(EditorSettingsLoad {
            settings: Self::create_default_settings(path)?,
            quarantined_path: None,
            future_schema_version: None,
        })
    }

    pub(super) fn recover_default_settings_with(
        path: &Path,
        quarantine: impl FnOnce(&Path) -> anyhow::Result<PathBuf>,
    ) -> anyhow::Result<EditorSettingsLoad> {
        let settings = Self::default();
        let quarantined_path = match quarantine(path) {
            Ok(quarantined_path) => {
                settings.save(path)?;
                Some(quarantined_path)
            }
            Err(quarantine_error) => {
                if let Err(save_error) = settings.save(path) {
                    anyhow::bail!(
                        "could not recover corrupt settings: quarantine failed ({quarantine_error}); writing defaults failed ({save_error})"
                    );
                }
                None
            }
        };
        Ok(EditorSettingsLoad {
            settings,
            quarantined_path,
            future_schema_version: None,
        })
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut settings = self.clone();
        settings.schema_version = SETTINGS_SCHEMA_VERSION;
        settings.sanitize();
        let text = toml::to_string_pretty(&settings)?;
        atomic_write(path, text.as_bytes())?;
        Ok(())
    }

    pub(super) fn apply_migrations(&mut self, source_version: u32) -> bool {
        let mut changed = false;
        if source_version < 2 {
            if self.terminal_tabs_focus_mode == TerminalTabsFocusMode::DoubleClick {
                self.terminal_tabs_focus_mode = TerminalTabsFocusMode::SingleClick;
                changed = true;
            }
            if self.terminal_tabs_location == TerminalTabsLocation::Right {
                self.terminal_tabs_location = TerminalTabsLocation::Top;
                changed = true;
            }
            if !self.terminal_copy_on_selection {
                self.terminal_copy_on_selection = DEFAULT_TERMINAL_COPY_ON_SELECTION;
                changed = true;
            }
        }
        if source_version < 3 {
            changed |= self.keymap.ensure_default_command_palette_binding();
        }
        if source_version < 4 {
            self.lsp_servers = merge_lsp_server_configs_with_defaults(&self.lsp_servers);
            changed = true;
        }
        if source_version < 5 {
            for server in &mut self.lsp_servers {
                if let Some(default) = default_lsp_server_for_language(&server.language)
                    && server.command == default.command
                    && server.args == default.args
                    && server.extensions == default.extensions
                    && server.root_markers == default.root_markers
                {
                    server.enabled = default.enabled;
                }
            }
            changed = true;
        }
        self.schema_version = SETTINGS_SCHEMA_VERSION;
        changed || source_version < SETTINGS_SCHEMA_VERSION
    }
}

impl EditorVimSettings {
    pub fn sanitize(&mut self) -> bool {
        let mut changed = sanitize_settings_string_list(
            &mut self.disabled_bindings,
            SETTINGS_LIST_MAX_ITEMS,
            SETTINGS_MAP_KEY_MAX_CHARS,
            true,
        );

        let original = std::mem::take(&mut self.key_overrides);
        let mut normalized = Vec::with_capacity(original.len().min(SETTINGS_LIST_MAX_ITEMS));
        let mut seen = Vec::new();
        changed |= original.len() > SETTINGS_LIST_MAX_ITEMS;

        for mut binding in original {
            if normalized.len() >= SETTINGS_LIST_MAX_ITEMS {
                changed = true;
                continue;
            }

            let original_binding = binding.clone();
            binding.before =
                normalize_settings_plain_string(&binding.before, SETTINGS_MAP_KEY_MAX_CHARS, true);
            binding.after =
                normalize_settings_plain_string(&binding.after, SETTINGS_MAP_KEY_MAX_CHARS, true);
            let command_changed = binding
                .command
                .as_mut()
                .map(|command| command.normalize_keymap_metadata())
                .unwrap_or(false);
            if binding
                .command
                .as_ref()
                .is_some_and(|command| !command.is_stable_keymap_command())
            {
                changed = true;
                continue;
            }
            if binding.before.is_empty() || (binding.after.is_empty() && binding.command.is_none())
            {
                changed = true;
                continue;
            }
            if seen.iter().any(|before: &String| before == &binding.before) {
                changed = true;
                continue;
            }

            changed |= command_changed || binding != original_binding;
            seen.push(binding.before.clone());
            normalized.push(binding);
        }

        self.key_overrides = normalized;
        changed
    }
}
