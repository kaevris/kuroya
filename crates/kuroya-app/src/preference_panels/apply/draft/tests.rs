use super::apply_settings_panel_draft;
use kuroya_core::{
    Command, DiffAlgorithm, DiffWordWrap, EDITOR_FONT_LIGATURES_ON,
    EDITOR_FONT_VARIATIONS_TRANSLATE, EditorAccessibilitySupport, EditorAutoSaveMode,
    EditorBracketPairGuideMode, EditorColorDecoratorsActivatedOn, EditorCursorSmoothCaretAnimation,
    EditorCursorStyle, EditorCursorSurroundingLinesStyle, EditorDefaultColorDecorators,
    EditorDropIntoEditorShowDropSelector, EditorExperimentalGpuAcceleration,
    EditorExperimentalWhitespaceRendering, EditorFindAutoFindInSelection, EditorFindHistory,
    EditorFindSeedSearchStringFromSelection, EditorFoldingStrategy, EditorGotoLocationMultiple,
    EditorHighlightActiveIndentation, EditorInlineSuggestMode, EditorInlineSuggestShowToolbar,
    EditorLightbulbMode, EditorLineDecorationsWidth, EditorLineNumbers, EditorMatchBrackets,
    EditorMinimapAutohide, EditorMinimapShowSlider, EditorMinimapSide, EditorMinimapSize,
    EditorMouseMiddleClickAction, EditorMouseStyle, EditorMultiCursorModifier,
    EditorMultiCursorPaste, EditorPasteAsShowPasteSelector, EditorPeekWidgetDefaultFocus,
    EditorRenderLineHighlight, EditorRenderValidationDecorations, EditorRenderWhitespace,
    EditorScrollbarVisibility, EditorSettings, EditorShowFoldingControls, EditorSnippetSuggestions,
    EditorStickyScrollDefaultModel, EditorSuggestInsertMode, EditorSuggestPreviewMode,
    EditorSuggestSelection, EditorSuggestSelectionMode, EditorTabCompletion,
    EditorUnicodeHighlightNonBasicAscii, EditorUnicodeHighlightScope, EditorUnusualLineTerminators,
    EditorVimKeyOverride, EditorVimSettings, EditorWordBreak, EditorWordWrap,
    EditorWordWrapOverride, EditorWrappingIndent, EditorWrappingStrategy, GitCheckoutType,
    GitCountBadge, LspServerConfig, ScmCountBadge, ScmProviderCountBadge, TerminalConfirmOnExit,
    TerminalConfirmOnKill, TerminalCursorStyle, TerminalHideOnStartup, TerminalInactiveCursorStyle,
    TerminalMiddleClickBehavior, TerminalMultiLinePasteWarning, TerminalRightClickBehavior,
    TerminalSplitCwd, TerminalTabsFocusMode, TerminalTabsHideCondition, TerminalTabsLocation,
    TerminalTabsShowActions, TerminalTabsShowActiveTerminal, ThemeSettings,
};
use serde_json::{Map, Value};
use std::collections::BTreeSet;

#[test]
fn draft_apply_sanitizes_vim_binding_rows() {
    let mut settings = EditorSettings::default();
    let draft = EditorSettings {
        vim_keybindings: true,
        vim: EditorVimSettings {
            disabled_bindings: vec![
                " x ".to_owned(),
                "x".to_owned(),
                String::new(),
                " <C-n> ".to_owned(),
                "<Nope>".to_owned(),
            ],
            key_overrides: vec![
                EditorVimKeyOverride {
                    before: " K ".to_owned(),
                    after: String::new(),
                    command: Some(Command::RequestHover),
                },
                EditorVimKeyOverride {
                    before: "H".to_owned(),
                    after: " 0 ".to_owned(),
                    command: None,
                },
                EditorVimKeyOverride {
                    before: String::new(),
                    after: "gg".to_owned(),
                    command: None,
                },
                EditorVimKeyOverride {
                    before: " <Home> ".to_owned(),
                    after: " <C-r> ".to_owned(),
                    command: None,
                },
                EditorVimKeyOverride {
                    before: "L".to_owned(),
                    after: "<Left>".to_owned(),
                    command: None,
                },
            ],
        },
        ..EditorSettings::default()
    };

    apply_settings_panel_draft(&mut settings, &draft, "", "");

    assert!(settings.vim_keybindings);
    assert_eq!(settings.vim.disabled_bindings, ["x", "<C-n>"]);
    assert_eq!(
        settings.vim.key_overrides,
        [
            EditorVimKeyOverride {
                before: "K".to_owned(),
                after: String::new(),
                command: Some(Command::RequestHover),
            },
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
        ]
    );
}

#[test]
fn draft_apply_covers_every_settings_panel_field() {
    let default_values = serialized_settings_fields(&EditorSettings::default());
    let draft = draft_apply_copy_fixture();
    let draft_values = serialized_settings_fields(&draft);
    let mut settings = EditorSettings::default();

    apply_settings_panel_draft(
        &mut settings,
        &draft,
        " fonts/editor.ttf ",
        " fonts/ui.ttf ",
    );

    let applied_values = serialized_settings_fields(&settings);
    let intentionally_not_panel_applied = draft_apply_intentionally_excluded_fields();
    let intentionally_normalized = draft_apply_intentionally_normalized_fields();
    let mut unchanged_fixture_fields = Vec::new();
    let mut missing_exact_copies = Vec::new();
    let mut missing_normalized_copies = Vec::new();

    for (field, draft_value) in &draft_values {
        let field = field.as_str();
        if intentionally_not_panel_applied.contains(field) {
            continue;
        }

        let default_value = default_values
            .get(field)
            .expect("serialized settings fields should match");
        let applied_value = applied_values
            .get(field)
            .expect("serialized settings fields should match");

        if default_value == draft_value {
            unchanged_fixture_fields.push(field.to_owned());
        }

        if intentionally_normalized.contains(field) {
            if applied_value == default_value {
                missing_normalized_copies.push(field.to_owned());
            }
        } else if applied_value != draft_value {
            missing_exact_copies.push(format!(
                "{field}: expected {draft_value:?}, got {applied_value:?}"
            ));
        }
    }

    assert!(
        unchanged_fixture_fields.is_empty(),
        "draft apply coverage fixture leaves fields at their default values: {unchanged_fixture_fields:?}"
    );
    assert!(
        missing_exact_copies.is_empty(),
        "settings fields missing exact draft apply: {missing_exact_copies:?}"
    );
    assert!(
        missing_normalized_copies.is_empty(),
        "normalized settings fields appear not to be draft-applied: {missing_normalized_copies:?}"
    );
}

fn draft_apply_copy_fixture() -> EditorSettings {
    EditorSettings {
        font_size: 15.0,
        ui_font_size: 14.0,
        lsp_suggest_missing_servers: true,
        background_image_enabled: true,
        background_image_path: Some("C:/images/background.png".to_owned()),
        background_image_scope: kuroya_core::EditorBackgroundImageScope::FullApp,
        background_image_dim: 0.42,
        background_image_fit: kuroya_core::EditorBackgroundImageFit::Contain,
        background_image_position: kuroya_core::EditorBackgroundImagePosition::Bottom,
        background_image_loop: false,
        editor_font_path: Some("fonts/editor.ttf".to_owned()),
        ui_font_path: Some("fonts/ui.ttf".to_owned()),
        font_family: " Cascadia Code ".to_owned(),
        font_weight: "600".to_owned(),
        font_ligatures: "true".to_owned(),
        font_variations: "true".to_owned(),
        letter_spacing: 1.25,
        automatic_layout: true,
        disable_layer_hinting: true,
        disable_monospace_optimizations: true,
        extra_editor_class_name: " workbench-editor ".to_owned(),
        allow_variable_line_heights: false,
        allow_variable_fonts: false,
        allow_variable_fonts_in_accessibility_mode: true,
        accessibility_support: EditorAccessibilitySupport::On,
        accessibility_page_size: 250,
        aria_label: " Source editor ".to_owned(),
        aria_required: true,
        screen_reader_announce_inline_suggestion: false,
        tab_index: -1,
        read_only: true,
        read_only_message: " Generated file ".to_owned(),
        dom_read_only: true,
        edit_context: false,
        render_rich_screen_reader_content: true,
        trim_whitespace_on_delete: true,
        unusual_line_terminators: EditorUnusualLineTerminators::Auto,
        use_shadow_dom: false,
        use_tab_stops: false,
        fixed_overflow_widgets: true,
        allow_overflow: false,
        tab_width: 2,
        insert_spaces: false,
        detect_indentation: false,
        word_separators: ".".to_owned(),
        word_segmenter_locales: vec![" ja ".to_owned(), "".to_owned(), "zh-CN".to_owned()],
        auto_indent: false,
        auto_closing_brackets: false,
        auto_closing_quotes: false,
        experimental_gpu_acceleration: EditorExperimentalGpuAcceleration::On,
        experimental_whitespace_rendering: EditorExperimentalWhitespaceRendering::Font,
        auto_closing_comments: kuroya_core::EditorAutoClosingStrategy::BeforeWhitespace,
        auto_closing_delete: kuroya_core::EditorAutoClosingEditStrategy::Never,
        auto_closing_overtype: kuroya_core::EditorAutoClosingEditStrategy::Always,
        auto_surround: false,
        auto_indent_on_paste: true,
        auto_indent_on_paste_within_string: false,
        sticky_tab_stops: true,
        linked_editing: true,
        rename_on_type: true,
        tab_focus_mode: true,
        vim_keybindings: true,
        vim: EditorVimSettings {
            disabled_bindings: vec!["<C-n>".to_owned()],
            key_overrides: vec![EditorVimKeyOverride {
                before: "K".to_owned(),
                after: String::new(),
                command: Some(Command::RequestHover),
            }],
        },
        discord: kuroya_core::DiscordSettings {
            presence_enabled: true,
            client_id: "1552355674346754098".to_owned(),
            show_details: false,
            show_workspace: false,
            show_elapsed: false,
        },
        quick_suggestions: true,
        quick_suggestions_delay_ms: 25,
        suggest_on_trigger_characters: false,
        accept_suggestion_on_enter: false,
        accept_suggestion_on_tab: true,
        accept_suggestion_on_commit_character: false,
        suggest_selection: EditorSuggestSelection::RecentlyUsedByPrefix,
        suggest_insert_mode: EditorSuggestInsertMode::Replace,
        suggest_filter_graceful: false,
        suggest_snippets_prevent_quick_suggestions: true,
        suggest_locality_bonus: true,
        suggest_share_suggest_selections: true,
        suggest_selection_mode: EditorSuggestSelectionMode::WhenQuickSuggestion,
        suggest_show_icons: false,
        suggest_show_status_bar: true,
        suggest_preview: true,
        suggest_preview_mode: EditorSuggestPreviewMode::Prefix,
        suggest_show_inline_details: false,
        suggest_show_methods: false,
        suggest_show_functions: false,
        suggest_show_constructors: false,
        suggest_show_deprecated: false,
        suggest_show_fields: false,
        suggest_show_variables: false,
        suggest_show_classes: false,
        suggest_show_structs: false,
        suggest_show_interfaces: false,
        suggest_show_modules: false,
        suggest_show_properties: false,
        suggest_show_events: false,
        suggest_show_operators: false,
        suggest_show_units: false,
        suggest_show_values: false,
        suggest_show_constants: false,
        suggest_show_enums: false,
        suggest_show_enum_members: false,
        suggest_show_keywords: false,
        suggest_show_words: false,
        suggest_show_colors: false,
        suggest_show_files: false,
        suggest_show_references: false,
        suggest_show_customcolors: false,
        suggest_show_folders: false,
        suggest_show_type_parameters: false,
        suggest_show_snippets: false,
        suggest_show_users: false,
        suggest_show_issues: false,
        suggest_match_on_word_start_only: false,
        suggest_font_size: 15,
        suggest_line_height: 24,
        tab_completion: EditorTabCompletion::OnlySnippets,
        snippet_suggestions: EditorSnippetSuggestions::Top,
        hover_enabled: false,
        hover_delay_ms: 450,
        hover_hiding_delay_ms: 900,
        hover_sticky: false,
        hover_above: false,
        hover_show_long_line_warning: false,
        inline_suggest_enabled: false,
        inline_suggest_mode: EditorInlineSuggestMode::Prefix,
        inline_suggest_show_toolbar: EditorInlineSuggestShowToolbar::Never,
        inline_suggest_keep_on_blur: true,
        inline_suggest_font_family: " JetBrains Mono ".to_owned(),
        inline_suggest_syntax_highlighting_enabled: false,
        inline_suggest_suppress_suggestions: true,
        inline_suggest_suppress_in_snippet_mode: false,
        inline_suggest_min_show_delay_ms: 125,
        inline_suggest_edits_enabled: false,
        inline_suggest_edits_show_collapsed: true,
        inline_suggest_edits_render_side_by_side:
            kuroya_core::EditorInlineSuggestEditsRenderSideBySide::Never,
        inline_suggest_edits_allow_code_shifting:
            kuroya_core::EditorInlineSuggestEditsAllowCodeShifting::Horizontal,
        inline_suggest_edits_show_long_distance_hint: false,
        inline_suggest_trigger_command_on_provider_change: true,
        inline_suggest_experimental_suppress_inline_suggestions: " ext.one,ext.two ".to_owned(),
        inline_suggest_experimental_show_on_suggest_conflict:
            kuroya_core::EditorInlineSuggestShowOnSuggestConflict::WhenSuggestListIsIncomplete,
        inline_suggest_experimental_empty_response_information: false,
        inline_completions_accessibility_verbose: true,
        lightbulb: EditorLightbulbMode::On,
        render_validation_decorations: EditorRenderValidationDecorations::Off,
        document_highlights_enabled: false,
        code_lens: false,
        code_lens_font_family: " Cascadia Code ".to_owned(),
        code_lens_font_size: 11,
        goto_location_multiple_definitions: EditorGotoLocationMultiple::GotoAndPeek,
        goto_location_multiple_type_definitions: EditorGotoLocationMultiple::Goto,
        goto_location_multiple_declarations: EditorGotoLocationMultiple::GotoAndPeek,
        goto_location_multiple_implementations: EditorGotoLocationMultiple::GotoAndPeek,
        goto_location_multiple_references: EditorGotoLocationMultiple::Goto,
        goto_location_multiple_tests: EditorGotoLocationMultiple::Goto,
        goto_location_alternative_definition_command: " editor.action.peekDefinition ".to_owned(),
        goto_location_alternative_type_definition_command: " editor.action.peekTypeDefinition "
            .to_owned(),
        goto_location_alternative_declaration_command: " editor.action.peekDeclaration ".to_owned(),
        goto_location_alternative_implementation_command: " editor.action.peekImplementation "
            .to_owned(),
        goto_location_alternative_reference_command: " editor.action.referenceSearch.trigger "
            .to_owned(),
        goto_location_alternative_tests_command: " editor.action.goToReferences ".to_owned(),
        peek_widget_default_focus: EditorPeekWidgetDefaultFocus::Editor,
        placeholder: " Type here ".to_owned(),
        definition_link_opens_in_peek: true,
        inlay_hints: false,
        inlay_hints_font_family: " Cascadia Code ".to_owned(),
        inlay_hints_font_size: 13,
        inlay_hints_padding: true,
        inlay_hints_maximum_length: 25,
        parameter_hints_enabled: false,
        parameter_hints_on_trigger_characters: false,
        parameter_hints_cycle: false,
        comments_insert_space: false,
        comments_ignore_empty_lines: false,
        format_on_save: true,
        format_on_type: true,
        format_on_paste: true,
        paste_as_enabled: false,
        paste_as_show_paste_selector: EditorPasteAsShowPasteSelector::Never,

        autosave: true,
        autosave_mode: EditorAutoSaveMode::OnFocusChange,
        autosave_delay_ms: 1_500,
        smooth_scrolling: false,
        scroll_beyond_last_line: false,
        scroll_beyond_last_column: 12,
        scroll_on_middle_click: true,
        scroll_predominant_axis: false,
        inertial_scroll: true,
        mouse_wheel_scroll_sensitivity: 2.5,
        fast_scroll_sensitivity: 9.0,
        mouse_wheel_zoom: true,
        scrollbar_vertical: EditorScrollbarVisibility::Visible,
        scrollbar_horizontal: EditorScrollbarVisibility::Visible,
        scrollbar_vertical_scrollbar_size: 18,
        scrollbar_horizontal_scrollbar_size: 16,
        scrollbar_ignore_horizontal_scrollbar_in_content_height: true,
        explorer_scrollbar: EditorScrollbarVisibility::Visible,
        padding_top: 12,
        padding_bottom: 24,
        links: false,
        show_unused: false,
        show_deprecated: false,
        contextmenu: false,
        color_decorators: false,
        color_decorators_activated_on: EditorColorDecoratorsActivatedOn::Hover,
        color_decorators_limit: 42,
        default_color_decorators: EditorDefaultColorDecorators::Never,
        sticky_scroll: false,
        sticky_scroll_max_line_count: 8,
        sticky_scroll_default_model: EditorStickyScrollDefaultModel::IndentationModel,
        sticky_scroll_scroll_with_editor: false,
        line_height: 1.7,
        minimap: true,
        minimap_side: EditorMinimapSide::Left,
        minimap_autohide: EditorMinimapAutohide::Scroll,
        minimap_size: EditorMinimapSize::Fit,
        minimap_show_slider: EditorMinimapShowSlider::Always,
        minimap_scale: 3,
        minimap_render_characters: false,
        minimap_max_column: 80,
        minimap_show_region_section_headers: false,
        minimap_show_mark_section_headers: false,
        minimap_mark_section_header_regex: "MARK: (?<label>.*)".to_owned(),
        minimap_section_header_font_size: 12.0,
        minimap_section_header_letter_spacing: 2.0,
        multi_cursor_modifier: EditorMultiCursorModifier::CtrlCmd,
        multi_cursor_merge_overlapping: false,
        multi_cursor_paste: EditorMultiCursorPaste::Full,
        multi_cursor_limit: 200,
        column_selection: true,
        mouse_middle_click_action: EditorMouseMiddleClickAction::OpenLink,
        empty_selection_clipboard: false,
        selection_clipboard: false,
        copy_with_syntax_highlighting: false,
        double_click_selects_block: false,
        drag_and_drop: false,
        drop_into_editor_enabled: false,
        drop_into_editor_show_drop_selector: EditorDropIntoEditorShowDropSelector::Never,
        glyph_margin: false,
        ruler_column: 100,
        overview_ruler_border: false,
        overview_ruler_lanes: 2,
        hide_cursor_in_overview_ruler: true,
        status_bar_visible: false,
        devtools_verbose_logging: true,
        devtools_profiling_enabled: true,
        plugins: kuroya_core::settings::PluginSettings {
            enabled: false,
            disabled_ids: vec!["legacy.plugin".to_owned()],
        },
        lsp_servers: vec![LspServerConfig {
            language: "rust".to_owned(),
            command: "rust-analyzer".to_owned(),
            args: vec!["--log-file".to_owned(), "rust-analyzer.log".to_owned()],
            extensions: vec!["rs".to_owned()],
            root_markers: vec!["Cargo.toml".to_owned()],
            enabled: true,
        }],
        window_zoom_level: 1.25,
        line_numbers: EditorLineNumbers::Relative,
        line_decorations_width: EditorLineDecorationsWidth::Pixels(16.0),
        line_numbers_min_chars: 8,
        select_on_line_numbers: false,
        word_wrap: EditorWordWrap::Bounded,
        word_wrap_override1: EditorWordWrapOverride::Off,
        word_wrap_override2: EditorWordWrapOverride::On,
        word_wrap_break_after_characters: " ,;".to_owned(),
        word_wrap_break_before_characters: "([{".to_owned(),
        word_wrap_column: 96,
        wrapping_indent: EditorWrappingIndent::DeepIndent,
        wrapping_strategy: EditorWrappingStrategy::Advanced,
        wrap_on_escaped_line_feeds: true,
        word_break: EditorWordBreak::KeepAll,
        reveal_horizontal_right_padding: 30,
        rounded_selection: false,
        stop_rendering_line_after: -1,
        render_whitespace: EditorRenderWhitespace::All,
        render_final_newline: kuroya_core::EditorRenderFinalNewline::Off,
        render_control_characters: true,
        unicode_highlight_ambiguous_characters: false,
        unicode_highlight_invisible_characters: false,
        unicode_highlight_non_basic_ascii: EditorUnicodeHighlightNonBasicAscii::On,
        unicode_highlight_include_comments: EditorUnicodeHighlightScope::On,
        unicode_highlight_include_strings: EditorUnicodeHighlightScope::InUntrustedWorkspace,
        unicode_highlight_allowed_characters: std::collections::BTreeMap::from([
            ("Α".to_owned(), true),
            ("ß".to_owned(), false),
        ]),
        unicode_highlight_allowed_locales: std::collections::BTreeMap::from([
            ("_os".to_owned(), false),
            ("ja".to_owned(), true),
        ]),
        render_line_highlight: EditorRenderLineHighlight::None,
        cursor_surrounding_lines: 3,
        cursor_surrounding_lines_style: EditorCursorSurroundingLinesStyle::All,
        render_line_highlight_only_when_focus: true,
        smart_select_select_leading_and_trailing_whitespace: false,
        smart_select_select_subwords: false,
        find_seed_search_string_from_selection: EditorFindSeedSearchStringFromSelection::Selection,
        find_auto_find_in_selection: EditorFindAutoFindInSelection::Multiline,
        find_on_type: false,
        find_cursor_move_on_type: false,
        find_loop: false,
        find_close_on_result: true,
        find_global_find_clipboard: true,
        find_add_extra_space_on_top: false,
        find_history: EditorFindHistory::Never,
        find_replace_history: EditorFindHistory::Never,
        diff_ignore_trim_whitespace: false,
        diff_algorithm: DiffAlgorithm::Legacy,
        diff_render_side_by_side: false,
        diff_enable_split_view_resizing: false,
        diff_split_view_default_ratio: 0.35,
        diff_render_side_by_side_inline_breakpoint: 720,
        diff_use_inline_view_when_space_is_limited: false,
        diff_compact_mode: true,
        diff_original_editable: true,
        diff_code_lens: true,
        diff_accessibility_verbose: true,
        diff_hide_unchanged_regions: true,
        diff_context_lines: 1,
        diff_hide_unchanged_regions_minimum_line_count: 9,
        diff_hide_unchanged_regions_reveal_line_count: 15,
        diff_max_computation_time_ms: 2_500,
        diff_max_file_size_mb: 12,
        diff_render_gutter_menu: false,
        diff_render_indicators: false,
        diff_render_margin_revert_icon: false,
        diff_render_overview_ruler: false,
        diff_experimental_show_moves: true,
        diff_experimental_show_empty_decorations: false,
        diff_experimental_use_true_inline_view: true,
        diff_word_wrap: DiffWordWrap::Off,
        diff_only_show_accessible_viewer: true,
        diff_is_in_embedded_editor: true,
        project_index_max_files: 25_000,
        project_index_exclude_globs: vec!["dist".to_owned(), "cache".to_owned()],
        project_index_include_hidden_dirs: true,
        project_search_exclude_globs: vec!["generated".to_owned(), "tmp".to_owned()],
        project_search_max_file_size_mb: 8,
        project_search_max_results: 1_234,
        git_enabled: false,
        git_add_ai_co_author: kuroya_core::GitAddAiCoAuthor::All,
        git_allow_force_push: true,
        git_allow_no_verify_commit: true,
        git_auto_repository_detection: kuroya_core::GitAutoRepositoryDetection::SubFolders,
        git_autofetch: kuroya_core::GitAutoFetch::All,
        git_autofetch_period: 90,
        git_autorefresh: false,
        git_auto_stash: true,
        git_commands_to_log: vec![" fetch ".to_owned(), "".to_owned(), "pull".to_owned()],
        git_confirm_force_push: false,
        git_confirm_no_verify_commit: false,
        git_confirm_sync: false,
        git_ignore_limit_warning: true,
        git_ignore_submodules: true,
        git_ignored_repositories: vec!["C:/repo/ignored".to_owned(), "../other".to_owned()],
        git_repository_scan_ignored_folders: vec!["node_modules".to_owned(), "dist".to_owned()],
        git_open_repository_in_parent_folders: kuroya_core::GitOpenRepositoryInParentFolders::Never,
        git_detect_submodules: false,
        git_detect_submodules_limit: 3,
        git_repository_scan_max_depth: 4,
        git_detect_worktrees: true,
        git_detect_worktrees_limit: 7,
        git_discard_untracked_changes_to_trash: false,
        git_diagnostics_commit_hook_enabled: true,
        git_diagnostics_commit_hook_sources: std::collections::BTreeMap::from([(
            "*".to_owned(),
            "warning".to_owned(),
        )]),
        git_enable_commit_signing: true,
        git_enable_status_bar_sync: false,
        git_fetch_on_pull: true,
        git_follow_tags_when_sync: true,
        git_ignore_legacy_warning: true,
        git_ignore_missing_git_warning: true,
        git_ignore_rebase_warning: true,
        git_ignore_windows_git27_warning: true,
        git_merge_editor: true,
        git_open_after_clone: kuroya_core::GitOpenAfterClone::AlwaysNewWindow,
        git_optimistic_update: false,
        git_path: vec![" C:/Git/bin/git.exe ".to_owned(), "".to_owned()],
        git_post_commit_command: kuroya_core::GitPostCommitCommand::Sync,
        git_prune_on_fetch: true,
        git_pull_before_checkout: true,
        git_pull_tags: false,
        git_rebase_when_sync: true,
        git_remember_post_commit_command: true,
        git_replace_tags_when_pull: true,
        git_support_cancellation: true,
        git_terminal_authentication: false,
        git_terminal_git_editor: true,
        git_use_force_push_if_includes: false,
        git_use_force_push_with_lease: false,
        git_use_integrated_ask_pass: false,
        git_worktree_include_files: vec![" packages/app ".to_owned(), "".to_owned()],
        git_default_branch_name: " trunk ".to_owned(),
        git_default_clone_directory: Some(" C:/src ".to_owned()),
        git_similarity_threshold: 80,
        scm_default_view_mode: kuroya_core::ScmDefaultViewMode::Tree,
        scm_default_view_sort_key: kuroya_core::ScmDefaultViewSortKey::Status,
        scm_auto_reveal: false,
        scm_count_badge: ScmCountBadge::Off,
        scm_provider_count_badge: ScmProviderCountBadge::Visible,
        scm_always_show_repositories: true,
        scm_repositories_visible: 2,
        scm_compact_folders: false,
        scm_always_show_actions: true,
        scm_show_action_button: false,
        git_show_commit_input: false,
        git_show_push_success_notification: true,
        git_use_editor_as_commit_input: false,
        git_verbose_commit: true,
        git_show_action_button_commit: false,
        git_always_sign_off: true,
        git_confirm_committed_delete: false,
        git_confirm_empty_commits: false,
        git_require_user_config: false,
        git_show_progress: false,
        git_show_reference_details: false,
        git_timeline_show_author: false,
        git_timeline_show_uncommitted: true,
        git_timeline_date: kuroya_core::GitTimelineDate::Authored,
        git_show_inline_open_file_action: false,
        git_count_badge: GitCountBadge::Tracked,
        git_untracked_changes: kuroya_core::GitUntrackedChanges::Separate,
        git_open_diff_on_click: false,
        git_close_diff_on_operation: true,
        git_always_show_staged_changes_resource_group: true,
        git_checkout_type: vec![
            GitCheckoutType::Tags,
            GitCheckoutType::Remote,
            GitCheckoutType::Remote,
        ],
        git_branch_sort_order: kuroya_core::GitBranchSortOrder::Alphabetically,
        git_branch_prefix: " feature/ ".to_owned(),
        git_branch_random_name_enable: true,
        git_branch_random_name_dictionary: vec![
            " colors ".to_owned(),
            "".to_owned(),
            "numbers".to_owned(),
        ],
        git_branch_validation_regex: " ^feature/ ".to_owned(),
        git_branch_whitespace_char: " _ ".to_owned(),
        git_decorations_enabled: false,
        git_enable_smart_commit: true,
        git_suggest_smart_commit: false,
        git_smart_commit_changes: kuroya_core::GitSmartCommitChanges::Tracked,
        git_prompt_to_save_files_before_commit:
            kuroya_core::GitPromptToSaveFilesBeforeCommit::Staged,
        git_prompt_to_save_files_before_stash: kuroya_core::GitPromptToSaveFilesBeforeCommit::Never,
        git_branch_protection: vec![" main ".to_owned(), "release/*".to_owned(), "".to_owned()],
        git_branch_protection_prompt:
            kuroya_core::GitBranchProtectionPrompt::AlwaysCommitToNewBranch,
        git_status_limit: 250,
        git_use_commit_input_as_stash_message: true,
        git_commit_short_hash_length: 12,
        git_input_validation: true,
        git_input_validation_length: 80,
        git_input_validation_subject_length: kuroya_core::GitInputValidationSubjectLength::Inherit,
        git_blame_status_bar_item_enabled: false,
        git_blame_editor_decoration_enabled: true,
        git_blame_editor_decoration_disable_hover: true,
        git_blame_ignore_whitespace: true,
        git_blame_status_bar_item_template: " ${subject} - ${authorName} ".to_owned(),
        git_blame_editor_decoration_template: " ${hash}: ${subject} ".to_owned(),
        scm_show_input_action_button: false,
        scm_input_min_line_count: 2,
        scm_input_max_line_count: 8,
        scm_input_font_family: "editor".to_owned(),
        scm_input_font_size: 15.0,
        scm_diff_decorations: kuroya_core::ScmDiffDecorations::Minimap,
        scm_diff_decorations_gutter_action: kuroya_core::ScmDiffDecorationsGutterAction::None,
        scm_diff_decorations_gutter_visibility:
            kuroya_core::ScmDiffDecorationsGutterVisibility::Hover,
        scm_diff_decorations_gutter_width: 5,
        scm_diff_decorations_gutter_pattern: kuroya_core::ScmDiffDecorationsGutterPattern {
            added: true,
            modified: false,
        },
        scm_diff_decorations_ignore_trim_whitespace:
            kuroya_core::ScmDiffDecorationsIgnoreTrimWhitespace::Inherit,
        scm_graph_page_on_scroll: false,
        scm_graph_page_size: 125,
        scm_graph_badges: kuroya_core::ScmGraphBadges::All,
        scm_graph_show_incoming_changes: false,
        scm_graph_show_outgoing_changes: false,
        bracket_pair_colorization: false,
        bracket_pair_colorization_independent_color_pool_per_bracket_type: true,
        bracket_pair_guides: EditorBracketPairGuideMode::On,
        bracket_pair_guides_horizontal: EditorBracketPairGuideMode::On,
        highlight_active_bracket_pair: false,
        match_brackets: EditorMatchBrackets::Near,
        folding: false,
        folding_highlight: false,
        folding_imports_by_default: false,
        folding_maximum_regions: 123,
        folding_strategy: EditorFoldingStrategy::Indentation,
        unfold_on_click_after_end_of_line: true,
        show_folding_controls: EditorShowFoldingControls::Never,
        indent_guides: false,
        highlight_active_indentation: EditorHighlightActiveIndentation::Always,
        mouse_style: EditorMouseStyle::Copy,
        cursor_smooth_caret_animation: EditorCursorSmoothCaretAnimation::Explicit,
        cursor_style: EditorCursorStyle::LineThin,
        overtype_cursor_style: EditorCursorStyle::BlockOutline,
        overtype_on_paste: false,
        cursor_blinking: true,
        cursor_width: 4.0,
        cursor_height: 18,
        terminal_scrollback_rows: 2_000,
        terminal_shell_path: Some(" pwsh.exe ".to_owned()),
        terminal_shell_args: vec![" -NoLogo ".to_owned(), "".to_owned()],
        terminal_cwd: Some(" tools ".to_owned()),
        terminal_split_cwd: TerminalSplitCwd::WorkspaceRoot,
        terminal_min_rows: 9,
        terminal_min_columns: 60,
        terminal_font_size: 15.0,
        terminal_line_height: 1.5,
        terminal_letter_spacing: 0.5,
        terminal_cursor_style: TerminalCursorStyle::Underline,
        terminal_cursor_width: 3.0,
        terminal_cursor_blinking: true,
        terminal_cursor_style_inactive: TerminalInactiveCursorStyle::Line,
        terminal_draw_bold_text_in_bright_colors: false,
        terminal_minimum_contrast_ratio: 3.5,
        terminal_enable_bell: true,
        terminal_bell_duration_ms: 300,
        terminal_show_exit_alert: false,
        terminal_hide_on_startup: TerminalHideOnStartup::Always,
        terminal_hide_on_last_closed: false,
        terminal_confirm_on_exit: TerminalConfirmOnExit::Always,
        terminal_confirm_on_kill: TerminalConfirmOnKill::Panel,
        terminal_tabs_enabled: false,
        terminal_tabs_default_icon: " code ".to_owned(),
        terminal_tabs_default_color: Some(" terminal.ansiBlue ".to_owned()),
        terminal_tabs_allow_agent_cli_title: false,
        terminal_tabs_title: " ${process} - ${cwd} ".to_owned(),
        terminal_tabs_hide_condition: TerminalTabsHideCondition::Never,
        terminal_tabs_show_active_terminal: TerminalTabsShowActiveTerminal::Always,
        terminal_tabs_show_actions: TerminalTabsShowActions::Never,
        terminal_tabs_focus_mode: TerminalTabsFocusMode::DoubleClick,
        terminal_tabs_location: TerminalTabsLocation::Left,
        terminal_right_click_behavior: TerminalRightClickBehavior::Paste,
        terminal_middle_click_behavior: TerminalMiddleClickBehavior::Paste,
        terminal_alt_click_moves_cursor: false,
        terminal_copy_on_selection: false,
        terminal_ignore_bracketed_paste_mode: true,
        terminal_enable_multi_line_paste_warning: TerminalMultiLinePasteWarning::Always,
        terminal_word_separators: ":".to_owned(),
        terminal_mouse_wheel_scroll_sensitivity: 2.0,
        terminal_fast_scroll_sensitivity: 8.0,
        terminal_mouse_wheel_zoom: true,
        trim_trailing_whitespace: true,
        insert_final_newline: true,
        trim_final_newlines: true,
        theme: ThemeSettings {
            name: "Panel Theme".to_owned(),
            background: [1, 2, 3],
            panel: [4, 5, 6],
            ..ThemeSettings::default()
        },
        custom_theme_paths: vec!["themes/live.toml".to_owned()],
        active_custom_theme_path: Some("themes/live.toml".to_owned()),
        ..EditorSettings::default()
    }
}

#[test]
fn draft_apply_copies_word_separators() {
    let mut settings = EditorSettings::default();
    let draft = draft_apply_copy_fixture();

    apply_settings_panel_draft(
        &mut settings,
        &draft,
        " fonts/editor.ttf ",
        " fonts/ui.ttf ",
    );

    assert_eq!(settings.font_size, 15.0);
    assert_eq!(settings.ui_font_size, 14.0);
    assert!(settings.background_image_enabled);
    assert_eq!(
        settings.background_image_path.as_deref(),
        Some("C:/images/background.png")
    );
    assert_eq!(settings.background_image_dim, 0.42);
    assert_eq!(
        settings.background_image_scope,
        kuroya_core::EditorBackgroundImageScope::FullApp
    );
    assert_eq!(
        settings.background_image_fit,
        kuroya_core::EditorBackgroundImageFit::Contain
    );
    assert_eq!(
        settings.background_image_position,
        kuroya_core::EditorBackgroundImagePosition::Bottom
    );
    assert!(!settings.background_image_loop);
    assert_eq!(settings.font_family, " Cascadia Code ");
    assert_eq!(settings.font_weight, "600");
    assert_eq!(settings.font_ligatures, EDITOR_FONT_LIGATURES_ON);
    assert_eq!(settings.font_variations, EDITOR_FONT_VARIATIONS_TRANSLATE);
    assert_eq!(settings.letter_spacing, 1.25);
    assert!(settings.automatic_layout);
    assert!(settings.disable_layer_hinting);
    assert!(settings.disable_monospace_optimizations);
    assert_eq!(settings.extra_editor_class_name, " workbench-editor ");
    assert_eq!(
        settings.editor_font_path.as_deref(),
        Some("fonts/editor.ttf")
    );
    assert_eq!(settings.ui_font_path.as_deref(), Some("fonts/ui.ttf"));
    assert!(!settings.allow_variable_line_heights);
    assert!(!settings.allow_variable_fonts);
    assert!(settings.allow_variable_fonts_in_accessibility_mode);
    assert_eq!(
        settings.accessibility_support,
        EditorAccessibilitySupport::On
    );
    assert_eq!(settings.accessibility_page_size, 250);
    assert_eq!(settings.aria_label, " Source editor ");
    assert!(settings.aria_required);
    assert!(!settings.screen_reader_announce_inline_suggestion);
    assert_eq!(settings.tab_index, -1);
    assert!(settings.read_only);
    assert_eq!(settings.read_only_message, " Generated file ");
    assert!(settings.dom_read_only);
    assert!(!settings.edit_context);
    assert!(settings.render_rich_screen_reader_content);
    assert!(settings.trim_whitespace_on_delete);
    assert_eq!(
        settings.unusual_line_terminators,
        EditorUnusualLineTerminators::Auto
    );
    assert!(!settings.use_shadow_dom);
    assert!(!settings.use_tab_stops);
    assert!(settings.fixed_overflow_widgets);
    assert!(!settings.allow_overflow);
    assert_eq!(settings.tab_width, 2);
    assert!(!settings.insert_spaces);
    assert!(!settings.detect_indentation);
    assert_eq!(settings.word_separators, ".");
    assert_eq!(
        settings.word_segmenter_locales,
        [" ja ".to_owned(), "zh-CN".to_owned()]
    );
    assert!(!settings.auto_indent);
    assert!(!settings.auto_closing_brackets);
    assert!(!settings.auto_closing_quotes);
    assert_eq!(
        settings.experimental_gpu_acceleration,
        EditorExperimentalGpuAcceleration::On
    );
    assert_eq!(
        settings.experimental_whitespace_rendering,
        EditorExperimentalWhitespaceRendering::Font
    );
    assert!(!settings.auto_surround);
    assert!(settings.quick_suggestions);
    assert!(!settings.suggest_on_trigger_characters);
    assert!(!settings.accept_suggestion_on_enter);
    assert!(settings.accept_suggestion_on_tab);
    assert_eq!(
        settings.suggest_selection,
        EditorSuggestSelection::RecentlyUsedByPrefix
    );
    assert_eq!(
        settings.suggest_insert_mode,
        EditorSuggestInsertMode::Replace
    );
    assert!(!settings.suggest_filter_graceful);
    assert!(settings.suggest_snippets_prevent_quick_suggestions);
    assert!(settings.suggest_locality_bonus);
    assert!(settings.suggest_share_suggest_selections);
    assert_eq!(
        settings.suggest_selection_mode,
        EditorSuggestSelectionMode::WhenQuickSuggestion
    );
    assert!(!settings.suggest_show_icons);
    assert!(settings.suggest_show_status_bar);
    assert!(settings.suggest_preview);
    assert_eq!(
        settings.suggest_preview_mode,
        EditorSuggestPreviewMode::Prefix
    );
    assert!(!settings.suggest_show_inline_details);
    assert!(!settings.suggest_show_methods);
    assert!(!settings.suggest_show_functions);
    assert!(!settings.suggest_show_constructors);
    assert!(!settings.suggest_show_deprecated);
    assert!(!settings.suggest_show_fields);
    assert!(!settings.suggest_show_variables);
    assert!(!settings.suggest_show_classes);
    assert!(!settings.suggest_show_structs);
    assert!(!settings.suggest_show_interfaces);
    assert!(!settings.suggest_show_modules);
    assert!(!settings.suggest_show_properties);
    assert!(!settings.suggest_show_events);
    assert!(!settings.suggest_show_operators);
    assert!(!settings.suggest_show_units);
    assert!(!settings.suggest_show_values);
    assert!(!settings.suggest_show_constants);
    assert!(!settings.suggest_show_enums);
    assert!(!settings.suggest_show_enum_members);
    assert!(!settings.suggest_show_keywords);
    assert!(!settings.suggest_show_words);
    assert!(!settings.suggest_show_colors);
    assert!(!settings.suggest_show_files);
    assert!(!settings.suggest_show_references);
    assert!(!settings.suggest_show_customcolors);
    assert!(!settings.suggest_show_folders);
    assert!(!settings.suggest_show_type_parameters);
    assert!(!settings.suggest_show_snippets);
    assert!(!settings.suggest_show_users);
    assert!(!settings.suggest_show_issues);
    assert!(!settings.suggest_match_on_word_start_only);
    assert_eq!(settings.suggest_font_size, 15);
    assert_eq!(settings.suggest_line_height, 24);
    assert_eq!(settings.tab_completion, EditorTabCompletion::OnlySnippets);
    assert_eq!(settings.snippet_suggestions, EditorSnippetSuggestions::Top);
    assert!(!settings.hover_enabled);
    assert_eq!(settings.hover_delay_ms, 450);
    assert_eq!(settings.hover_hiding_delay_ms, 900);
    assert!(!settings.hover_sticky);
    assert!(!settings.hover_above);
    assert!(!settings.hover_show_long_line_warning);
    assert!(!settings.inline_suggest_enabled);
    assert_eq!(
        settings.inline_suggest_mode,
        EditorInlineSuggestMode::Prefix
    );
    assert_eq!(
        settings.inline_suggest_show_toolbar,
        EditorInlineSuggestShowToolbar::Never
    );
    assert!(settings.inline_suggest_keep_on_blur);
    assert_eq!(settings.inline_suggest_font_family, " JetBrains Mono ");
    assert!(!settings.inline_suggest_syntax_highlighting_enabled);
    assert!(settings.inline_suggest_suppress_suggestions);
    assert!(!settings.inline_suggest_suppress_in_snippet_mode);
    assert_eq!(settings.inline_suggest_min_show_delay_ms, 125);
    assert!(!settings.inline_suggest_edits_enabled);
    assert!(settings.inline_suggest_edits_show_collapsed);
    assert_eq!(
        settings.inline_suggest_edits_render_side_by_side,
        kuroya_core::EditorInlineSuggestEditsRenderSideBySide::Never
    );
    assert_eq!(
        settings.inline_suggest_edits_allow_code_shifting,
        kuroya_core::EditorInlineSuggestEditsAllowCodeShifting::Horizontal
    );
    assert!(!settings.inline_suggest_edits_show_long_distance_hint);
    assert!(settings.inline_suggest_trigger_command_on_provider_change);
    assert_eq!(
        settings.inline_suggest_experimental_suppress_inline_suggestions,
        "ext.one,ext.two"
    );
    assert_eq!(
        settings.inline_suggest_experimental_show_on_suggest_conflict,
        kuroya_core::EditorInlineSuggestShowOnSuggestConflict::WhenSuggestListIsIncomplete
    );
    assert!(!settings.inline_suggest_experimental_empty_response_information);
    assert!(settings.inline_completions_accessibility_verbose);
    assert_eq!(settings.lightbulb, EditorLightbulbMode::On);
    assert_eq!(
        settings.render_validation_decorations,
        EditorRenderValidationDecorations::Off
    );
    assert!(!settings.document_highlights_enabled);
    assert!(!settings.code_lens);
    assert_eq!(settings.code_lens_font_family, " Cascadia Code ");
    assert_eq!(settings.code_lens_font_size, 11);
    assert_eq!(
        settings.goto_location_multiple_definitions,
        EditorGotoLocationMultiple::GotoAndPeek
    );
    assert_eq!(
        settings.goto_location_multiple_type_definitions,
        EditorGotoLocationMultiple::Goto
    );
    assert_eq!(
        settings.goto_location_multiple_declarations,
        EditorGotoLocationMultiple::GotoAndPeek
    );
    assert_eq!(
        settings.goto_location_multiple_implementations,
        EditorGotoLocationMultiple::GotoAndPeek
    );
    assert_eq!(
        settings.goto_location_multiple_references,
        EditorGotoLocationMultiple::Goto
    );
    assert_eq!(
        settings.goto_location_multiple_tests,
        EditorGotoLocationMultiple::Goto
    );
    assert_eq!(
        settings.goto_location_alternative_definition_command,
        " editor.action.peekDefinition "
    );
    assert_eq!(
        settings.goto_location_alternative_type_definition_command,
        " editor.action.peekTypeDefinition "
    );
    assert_eq!(
        settings.goto_location_alternative_declaration_command,
        " editor.action.peekDeclaration "
    );
    assert_eq!(
        settings.goto_location_alternative_implementation_command,
        " editor.action.peekImplementation "
    );
    assert_eq!(
        settings.goto_location_alternative_reference_command,
        " editor.action.referenceSearch.trigger "
    );
    assert_eq!(
        settings.goto_location_alternative_tests_command,
        " editor.action.goToReferences "
    );
    assert_eq!(
        settings.peek_widget_default_focus,
        EditorPeekWidgetDefaultFocus::Editor
    );
    assert_eq!(settings.placeholder, " Type here ");
    assert!(settings.definition_link_opens_in_peek);
    assert!(!settings.inlay_hints);
    assert_eq!(settings.inlay_hints_font_family, " Cascadia Code ");
    assert_eq!(settings.inlay_hints_font_size, 13);
    assert!(settings.inlay_hints_padding);
    assert_eq!(settings.inlay_hints_maximum_length, 25);
    assert!(!settings.parameter_hints_enabled);
    assert!(!settings.parameter_hints_on_trigger_characters);
    assert!(!settings.parameter_hints_cycle);
    assert!(settings.format_on_save);
    assert!(settings.format_on_paste);
    assert!(settings.autosave);
    assert_eq!(settings.autosave_mode, EditorAutoSaveMode::OnFocusChange);
    assert_eq!(settings.autosave_delay_ms, 1_500);
    assert!(!settings.smooth_scrolling);
    assert!(!settings.scroll_beyond_last_line);
    assert_eq!(settings.scroll_beyond_last_column, 12);
    assert!(settings.scroll_on_middle_click);
    assert!(!settings.scroll_predominant_axis);
    assert!(settings.inertial_scroll);
    assert_eq!(settings.mouse_wheel_scroll_sensitivity, 2.5);
    assert_eq!(settings.fast_scroll_sensitivity, 9.0);
    assert!(settings.mouse_wheel_zoom);
    assert_eq!(
        settings.scrollbar_vertical,
        EditorScrollbarVisibility::Visible
    );
    assert_eq!(
        settings.scrollbar_horizontal,
        EditorScrollbarVisibility::Visible
    );
    assert_eq!(settings.scrollbar_vertical_scrollbar_size, 18);
    assert_eq!(settings.scrollbar_horizontal_scrollbar_size, 16);
    assert!(settings.scrollbar_ignore_horizontal_scrollbar_in_content_height);
    assert_eq!(
        settings.explorer_scrollbar,
        EditorScrollbarVisibility::Visible
    );
    assert_eq!(settings.padding_top, 12);
    assert_eq!(settings.padding_bottom, 24);
    assert!(!settings.links);
    assert!(!settings.show_unused);
    assert!(!settings.show_deprecated);
    assert!(!settings.contextmenu);
    assert!(!settings.color_decorators);
    assert_eq!(
        settings.color_decorators_activated_on,
        EditorColorDecoratorsActivatedOn::Hover
    );
    assert_eq!(settings.color_decorators_limit, 42);
    assert_eq!(
        settings.default_color_decorators,
        EditorDefaultColorDecorators::Never
    );
    assert!(!settings.sticky_scroll);
    assert_eq!(settings.sticky_scroll_max_line_count, 8);
    assert_eq!(
        settings.sticky_scroll_default_model,
        EditorStickyScrollDefaultModel::IndentationModel
    );
    assert!(!settings.sticky_scroll_scroll_with_editor);
    assert_eq!(settings.line_height, 1.7);
    assert!(settings.minimap);
    assert_eq!(settings.minimap_side, EditorMinimapSide::Left);
    assert_eq!(settings.minimap_autohide, EditorMinimapAutohide::Scroll);
    assert_eq!(settings.minimap_size, EditorMinimapSize::Fit);
    assert_eq!(
        settings.minimap_show_slider,
        EditorMinimapShowSlider::Always
    );
    assert_eq!(settings.minimap_scale, 3);
    assert!(!settings.minimap_render_characters);
    assert_eq!(settings.minimap_max_column, 80);
    assert!(!settings.minimap_show_region_section_headers);
    assert!(!settings.minimap_show_mark_section_headers);
    assert_eq!(
        settings.minimap_mark_section_header_regex,
        "MARK: (?<label>.*)"
    );
    assert_eq!(settings.minimap_section_header_font_size, 12.0);
    assert_eq!(settings.minimap_section_header_letter_spacing, 2.0);
    assert_eq!(
        settings.multi_cursor_modifier,
        EditorMultiCursorModifier::CtrlCmd
    );
    assert!(!settings.multi_cursor_merge_overlapping);
    assert_eq!(settings.multi_cursor_paste, EditorMultiCursorPaste::Full);
    assert_eq!(settings.multi_cursor_limit, 200);
    assert!(settings.column_selection);
    assert_eq!(
        settings.mouse_middle_click_action,
        EditorMouseMiddleClickAction::OpenLink
    );
    assert!(!settings.empty_selection_clipboard);
    assert!(!settings.selection_clipboard);
    assert!(!settings.copy_with_syntax_highlighting);
    assert!(!settings.glyph_margin);
    assert_eq!(settings.ruler_column, 100);
    assert!(!settings.overview_ruler_border);
    assert_eq!(settings.overview_ruler_lanes, 2);
    assert!(settings.hide_cursor_in_overview_ruler);
    assert!(!settings.status_bar_visible);
    assert!(settings.devtools_verbose_logging);
    assert!(settings.devtools_profiling_enabled);
    assert!(!settings.plugins.enabled);
    assert_eq!(settings.plugins.disabled_ids, ["legacy.plugin"]);
    assert_eq!(settings.window_zoom_level, 1.25);
    assert_eq!(settings.line_numbers, EditorLineNumbers::Relative);
    assert_eq!(
        settings.line_decorations_width,
        EditorLineDecorationsWidth::Pixels(16.0)
    );
    assert_eq!(settings.line_numbers_min_chars, 8);
    assert!(!settings.select_on_line_numbers);
    assert_eq!(settings.word_wrap, EditorWordWrap::Bounded);
    assert_eq!(settings.word_wrap_override1, EditorWordWrapOverride::Off);
    assert_eq!(settings.word_wrap_override2, EditorWordWrapOverride::On);
    assert_eq!(settings.word_wrap_break_after_characters, " ,;");
    assert_eq!(settings.word_wrap_break_before_characters, "([{");
    assert_eq!(settings.word_wrap_column, 96);
    assert_eq!(settings.wrapping_indent, EditorWrappingIndent::DeepIndent);
    assert_eq!(settings.wrapping_strategy, EditorWrappingStrategy::Advanced);
    assert!(settings.wrap_on_escaped_line_feeds);
    assert_eq!(settings.word_break, EditorWordBreak::KeepAll);
    assert_eq!(settings.reveal_horizontal_right_padding, 30);
    assert!(!settings.rounded_selection);
    assert_eq!(settings.stop_rendering_line_after, -1);
    assert_eq!(settings.render_whitespace, EditorRenderWhitespace::All);
    assert_eq!(
        settings.render_final_newline,
        kuroya_core::EditorRenderFinalNewline::Off
    );
    assert!(settings.render_control_characters);
    assert!(!settings.unicode_highlight_ambiguous_characters);
    assert!(!settings.unicode_highlight_invisible_characters);
    assert_eq!(
        settings.unicode_highlight_non_basic_ascii,
        EditorUnicodeHighlightNonBasicAscii::On
    );
    assert_eq!(
        settings.unicode_highlight_include_comments,
        EditorUnicodeHighlightScope::On
    );
    assert_eq!(
        settings.unicode_highlight_include_strings,
        EditorUnicodeHighlightScope::InUntrustedWorkspace
    );
    assert_eq!(
        settings.unicode_highlight_allowed_characters,
        std::collections::BTreeMap::from([("Α".to_owned(), true), ("ß".to_owned(), false)])
    );
    assert_eq!(
        settings.unicode_highlight_allowed_locales,
        std::collections::BTreeMap::from([("_os".to_owned(), false), ("ja".to_owned(), true)])
    );
    assert_eq!(
        settings.render_line_highlight,
        EditorRenderLineHighlight::None
    );
    assert_eq!(settings.cursor_surrounding_lines, 3);
    assert_eq!(
        settings.cursor_surrounding_lines_style,
        EditorCursorSurroundingLinesStyle::All
    );
    assert!(settings.render_line_highlight_only_when_focus);
    assert!(!settings.smart_select_select_leading_and_trailing_whitespace);
    assert!(!settings.smart_select_select_subwords);
    assert_eq!(
        settings.find_seed_search_string_from_selection,
        EditorFindSeedSearchStringFromSelection::Selection
    );
    assert_eq!(
        settings.find_auto_find_in_selection,
        EditorFindAutoFindInSelection::Multiline
    );
    assert!(!settings.find_on_type);
    assert!(!settings.find_cursor_move_on_type);
    assert!(!settings.find_loop);
    assert!(settings.find_close_on_result);
    assert!(settings.find_global_find_clipboard);
    assert!(!settings.find_add_extra_space_on_top);
    assert_eq!(settings.find_history, EditorFindHistory::Never);
    assert_eq!(settings.find_replace_history, EditorFindHistory::Never);
    assert_eq!(
        settings.auto_closing_comments,
        kuroya_core::EditorAutoClosingStrategy::BeforeWhitespace
    );
    assert_eq!(
        settings.auto_closing_delete,
        kuroya_core::EditorAutoClosingEditStrategy::Never
    );
    assert_eq!(
        settings.auto_closing_overtype,
        kuroya_core::EditorAutoClosingEditStrategy::Always
    );
    assert!(settings.auto_indent_on_paste);
    assert!(!settings.auto_indent_on_paste_within_string);
    assert!(settings.sticky_tab_stops);
    assert!(settings.linked_editing);
    assert!(settings.rename_on_type);
    assert!(settings.tab_focus_mode);
    assert_eq!(settings.quick_suggestions_delay_ms, 25);
    assert!(!settings.accept_suggestion_on_commit_character);
    assert!(!settings.comments_insert_space);
    assert!(!settings.comments_ignore_empty_lines);
    assert!(settings.format_on_type);
    assert!(!settings.paste_as_enabled);
    assert_eq!(
        settings.paste_as_show_paste_selector,
        EditorPasteAsShowPasteSelector::Never
    );
    assert!(!settings.double_click_selects_block);
    assert!(!settings.drag_and_drop);
    assert!(!settings.drop_into_editor_enabled);
    assert_eq!(
        settings.drop_into_editor_show_drop_selector,
        EditorDropIntoEditorShowDropSelector::Never
    );
    assert!(!settings.diff_ignore_trim_whitespace);
    assert_eq!(settings.diff_algorithm, DiffAlgorithm::Legacy);
    assert!(!settings.diff_render_side_by_side);
    assert!(!settings.diff_enable_split_view_resizing);
    assert_eq!(settings.diff_split_view_default_ratio, 0.35);
    assert_eq!(settings.diff_render_side_by_side_inline_breakpoint, 720);
    assert!(!settings.diff_use_inline_view_when_space_is_limited);
    assert!(settings.diff_compact_mode);
    assert!(settings.diff_original_editable);
    assert!(settings.diff_code_lens);
    assert!(settings.diff_accessibility_verbose);
    assert!(settings.diff_hide_unchanged_regions);
    assert_eq!(settings.diff_context_lines, 1);
    assert_eq!(settings.diff_hide_unchanged_regions_minimum_line_count, 9);
    assert_eq!(settings.diff_hide_unchanged_regions_reveal_line_count, 15);
    assert_eq!(settings.diff_max_computation_time_ms, 2_500);
    assert_eq!(settings.diff_max_file_size_mb, 12);
    assert!(!settings.diff_render_gutter_menu);
    assert!(!settings.diff_render_indicators);
    assert!(!settings.diff_render_margin_revert_icon);
    assert!(!settings.diff_render_overview_ruler);
    assert!(settings.diff_experimental_show_moves);
    assert!(!settings.diff_experimental_show_empty_decorations);
    assert!(settings.diff_experimental_use_true_inline_view);
    assert_eq!(settings.diff_word_wrap, DiffWordWrap::Off);
    assert!(settings.diff_only_show_accessible_viewer);
    assert!(settings.diff_is_in_embedded_editor);
    assert!(!settings.git_enabled);
    assert_eq!(
        settings.git_add_ai_co_author,
        kuroya_core::GitAddAiCoAuthor::All
    );
    assert!(settings.git_allow_force_push);
    assert!(settings.git_allow_no_verify_commit);
    assert_eq!(
        settings.git_auto_repository_detection,
        kuroya_core::GitAutoRepositoryDetection::SubFolders
    );
    assert_eq!(settings.git_autofetch, kuroya_core::GitAutoFetch::All);
    assert_eq!(settings.git_autofetch_period, 90);
    assert!(!settings.git_autorefresh);
    assert!(settings.git_auto_stash);
    assert_eq!(
        settings.git_commands_to_log,
        [" fetch ".to_owned(), "pull".to_owned()]
    );
    assert!(!settings.git_confirm_force_push);
    assert!(!settings.git_confirm_no_verify_commit);
    assert!(!settings.git_confirm_sync);
    assert!(settings.git_ignore_limit_warning);
    assert!(settings.git_ignore_submodules);
    assert_eq!(
        settings.git_ignored_repositories,
        vec!["C:/repo/ignored".to_owned(), "../other".to_owned()]
    );
    assert_eq!(
        settings.git_repository_scan_ignored_folders,
        vec!["node_modules".to_owned(), "dist".to_owned()]
    );
    assert_eq!(
        settings.git_open_repository_in_parent_folders,
        kuroya_core::GitOpenRepositoryInParentFolders::Never
    );
    assert!(!settings.git_detect_submodules);
    assert_eq!(settings.git_detect_submodules_limit, 3);
    assert_eq!(settings.git_repository_scan_max_depth, 4);
    assert!(settings.git_detect_worktrees);
    assert_eq!(settings.git_detect_worktrees_limit, 7);
    assert!(!settings.git_discard_untracked_changes_to_trash);
    assert!(settings.git_diagnostics_commit_hook_enabled);
    assert_eq!(
        settings.git_diagnostics_commit_hook_sources.get("*"),
        Some(&"warning".to_owned())
    );
    assert!(settings.git_enable_commit_signing);
    assert!(!settings.git_enable_status_bar_sync);
    assert!(settings.git_fetch_on_pull);
    assert!(settings.git_follow_tags_when_sync);
    assert!(settings.git_ignore_legacy_warning);
    assert!(settings.git_ignore_missing_git_warning);
    assert!(settings.git_ignore_rebase_warning);
    assert!(settings.git_ignore_windows_git27_warning);
    assert!(settings.git_merge_editor);
    assert_eq!(
        settings.git_open_after_clone,
        kuroya_core::GitOpenAfterClone::AlwaysNewWindow
    );
    assert!(!settings.git_optimistic_update);
    assert_eq!(settings.git_path, [" C:/Git/bin/git.exe ".to_owned()]);
    assert_eq!(
        settings.git_post_commit_command,
        kuroya_core::GitPostCommitCommand::Sync
    );
    assert!(settings.git_prune_on_fetch);
    assert!(settings.git_pull_before_checkout);
    assert!(!settings.git_pull_tags);
    assert!(settings.git_rebase_when_sync);
    assert!(settings.git_remember_post_commit_command);
    assert!(settings.git_replace_tags_when_pull);
    assert!(settings.git_support_cancellation);
    assert!(!settings.git_terminal_authentication);
    assert!(settings.git_terminal_git_editor);
    assert!(!settings.git_use_force_push_if_includes);
    assert!(!settings.git_use_force_push_with_lease);
    assert!(!settings.git_use_integrated_ask_pass);
    assert_eq!(
        settings.git_worktree_include_files,
        [" packages/app ".to_owned()]
    );
    assert_eq!(settings.git_default_branch_name, " trunk ");
    assert_eq!(
        settings.git_default_clone_directory,
        Some(" C:/src ".to_owned())
    );
    assert_eq!(settings.git_similarity_threshold, 80);
    assert_eq!(
        settings.scm_default_view_mode,
        kuroya_core::ScmDefaultViewMode::Tree
    );
    assert_eq!(
        settings.scm_default_view_sort_key,
        kuroya_core::ScmDefaultViewSortKey::Status
    );
    assert!(!settings.scm_auto_reveal);
    assert_eq!(settings.scm_count_badge, ScmCountBadge::Off);
    assert_eq!(
        settings.scm_provider_count_badge,
        ScmProviderCountBadge::Visible
    );
    assert!(settings.scm_always_show_repositories);
    assert_eq!(settings.scm_repositories_visible, 2);
    assert!(!settings.scm_compact_folders);
    assert!(settings.scm_always_show_actions);
    assert!(!settings.scm_show_action_button);
    assert!(!settings.git_show_commit_input);
    assert!(settings.git_show_push_success_notification);
    assert!(!settings.git_use_editor_as_commit_input);
    assert!(settings.git_verbose_commit);
    assert!(!settings.git_show_action_button_commit);
    assert!(settings.git_always_sign_off);
    assert!(!settings.git_confirm_committed_delete);
    assert!(!settings.git_confirm_empty_commits);
    assert!(!settings.git_require_user_config);
    assert!(!settings.git_show_progress);
    assert!(!settings.git_show_reference_details);
    assert!(!settings.git_timeline_show_author);
    assert!(settings.git_timeline_show_uncommitted);
    assert_eq!(
        settings.git_timeline_date,
        kuroya_core::GitTimelineDate::Authored
    );
    assert!(!settings.git_show_inline_open_file_action);
    assert_eq!(settings.git_count_badge, GitCountBadge::Tracked);
    assert_eq!(
        settings.git_untracked_changes,
        kuroya_core::GitUntrackedChanges::Separate
    );
    assert!(!settings.git_open_diff_on_click);
    assert!(settings.git_close_diff_on_operation);
    assert!(settings.git_always_show_staged_changes_resource_group);
    assert_eq!(
        settings.git_checkout_type,
        [GitCheckoutType::Remote, GitCheckoutType::Tags]
    );
    assert_eq!(
        settings.git_branch_sort_order,
        kuroya_core::GitBranchSortOrder::Alphabetically
    );
    assert_eq!(settings.git_branch_prefix, " feature/ ");
    assert!(settings.git_branch_random_name_enable);
    assert_eq!(
        settings.git_branch_random_name_dictionary,
        [" colors ".to_owned(), "numbers".to_owned()]
    );
    assert_eq!(settings.git_branch_validation_regex, " ^feature/ ");
    assert_eq!(settings.git_branch_whitespace_char, " _ ");
    assert!(!settings.git_decorations_enabled);
    assert!(settings.git_enable_smart_commit);
    assert!(!settings.git_suggest_smart_commit);
    assert_eq!(
        settings.git_smart_commit_changes,
        kuroya_core::GitSmartCommitChanges::Tracked
    );
    assert_eq!(
        settings.git_prompt_to_save_files_before_commit,
        kuroya_core::GitPromptToSaveFilesBeforeCommit::Staged
    );
    assert_eq!(
        settings.git_prompt_to_save_files_before_stash,
        kuroya_core::GitPromptToSaveFilesBeforeCommit::Never
    );
    assert_eq!(
        settings.git_branch_protection,
        [" main ".to_owned(), "release/*".to_owned()]
    );
    assert_eq!(
        settings.git_branch_protection_prompt,
        kuroya_core::GitBranchProtectionPrompt::AlwaysCommitToNewBranch
    );
    assert_eq!(settings.git_status_limit, 250);
    assert!(settings.git_use_commit_input_as_stash_message);
    assert_eq!(settings.git_commit_short_hash_length, 12);
    assert!(settings.git_input_validation);
    assert_eq!(settings.git_input_validation_length, 80);
    assert_eq!(
        settings.git_input_validation_subject_length,
        kuroya_core::GitInputValidationSubjectLength::Inherit
    );
    assert!(!settings.git_blame_status_bar_item_enabled);
    assert!(settings.git_blame_editor_decoration_enabled);
    assert!(settings.git_blame_editor_decoration_disable_hover);
    assert!(settings.git_blame_ignore_whitespace);
    assert_eq!(
        settings.git_blame_status_bar_item_template,
        " ${subject} - ${authorName} "
    );
    assert_eq!(
        settings.git_blame_editor_decoration_template,
        " ${hash}: ${subject} "
    );
    assert!(!settings.scm_show_input_action_button);
    assert_eq!(settings.scm_input_min_line_count, 2);
    assert_eq!(settings.scm_input_max_line_count, 8);
    assert_eq!(settings.scm_input_font_family, "editor");
    assert_eq!(settings.scm_input_font_size, 15.0);
    assert_eq!(
        settings.scm_diff_decorations,
        kuroya_core::ScmDiffDecorations::Minimap
    );
    assert_eq!(
        settings.scm_diff_decorations_gutter_action,
        kuroya_core::ScmDiffDecorationsGutterAction::None
    );
    assert_eq!(
        settings.scm_diff_decorations_gutter_visibility,
        kuroya_core::ScmDiffDecorationsGutterVisibility::Hover
    );
    assert_eq!(settings.scm_diff_decorations_gutter_width, 5);
    assert_eq!(
        settings.scm_diff_decorations_gutter_pattern,
        kuroya_core::ScmDiffDecorationsGutterPattern {
            added: true,
            modified: false
        }
    );
    assert_eq!(
        settings.scm_diff_decorations_ignore_trim_whitespace,
        kuroya_core::ScmDiffDecorationsIgnoreTrimWhitespace::Inherit
    );
    assert!(!settings.scm_graph_page_on_scroll);
    assert_eq!(settings.scm_graph_page_size, 125);
    assert_eq!(settings.scm_graph_badges, kuroya_core::ScmGraphBadges::All);
    assert!(!settings.scm_graph_show_incoming_changes);
    assert!(!settings.scm_graph_show_outgoing_changes);
    assert!(!settings.bracket_pair_colorization);
    assert!(settings.bracket_pair_colorization_independent_color_pool_per_bracket_type);
    assert_eq!(settings.bracket_pair_guides, EditorBracketPairGuideMode::On);
    assert_eq!(
        settings.bracket_pair_guides_horizontal,
        EditorBracketPairGuideMode::On
    );
    assert!(!settings.highlight_active_bracket_pair);
    assert_eq!(settings.match_brackets, EditorMatchBrackets::Near);
    assert!(!settings.folding);
    assert!(!settings.folding_highlight);
    assert!(!settings.folding_imports_by_default);
    assert_eq!(settings.folding_maximum_regions, 123);
    assert_eq!(
        settings.folding_strategy,
        EditorFoldingStrategy::Indentation
    );
    assert!(settings.unfold_on_click_after_end_of_line);
    assert_eq!(
        settings.show_folding_controls,
        EditorShowFoldingControls::Never
    );
    assert!(!settings.indent_guides);
    assert_eq!(
        settings.highlight_active_indentation,
        EditorHighlightActiveIndentation::Always
    );
    assert_eq!(settings.mouse_style, EditorMouseStyle::Copy);
    assert_eq!(
        settings.cursor_smooth_caret_animation,
        EditorCursorSmoothCaretAnimation::Explicit
    );
    assert_eq!(settings.cursor_style, EditorCursorStyle::LineThin);
    assert_eq!(
        settings.overtype_cursor_style,
        EditorCursorStyle::BlockOutline
    );
    assert!(!settings.overtype_on_paste);
    assert!(settings.cursor_blinking);
    assert_eq!(settings.cursor_width, 4.0);
    assert_eq!(settings.cursor_height, 18);
    assert_eq!(settings.terminal_scrollback_rows, 2_000);
    assert_eq!(settings.terminal_shell_path.as_deref(), Some(" pwsh.exe "));
    assert_eq!(settings.terminal_shell_args, [" -NoLogo ".to_owned()]);
    assert_eq!(settings.terminal_cwd.as_deref(), Some(" tools "));
    assert_eq!(settings.terminal_split_cwd, TerminalSplitCwd::WorkspaceRoot);
    assert_eq!(settings.terminal_min_rows, 9);
    assert_eq!(settings.terminal_min_columns, 60);
    assert_eq!(settings.terminal_font_size, 15.0);
    assert_eq!(settings.terminal_line_height, 1.5);
    assert_eq!(settings.terminal_letter_spacing, 0.5);
    assert_eq!(
        settings.terminal_cursor_style,
        TerminalCursorStyle::Underline
    );
    assert_eq!(settings.terminal_cursor_width, 3.0);
    assert!(settings.terminal_cursor_blinking);
    assert_eq!(
        settings.terminal_cursor_style_inactive,
        TerminalInactiveCursorStyle::Line
    );
    assert!(!settings.terminal_draw_bold_text_in_bright_colors);
    assert_eq!(settings.terminal_minimum_contrast_ratio, 3.5);
    assert!(settings.terminal_enable_bell);
    assert_eq!(settings.terminal_bell_duration_ms, 300);
    assert!(!settings.terminal_show_exit_alert);
    assert_eq!(
        settings.terminal_hide_on_startup,
        TerminalHideOnStartup::Always
    );
    assert!(!settings.terminal_hide_on_last_closed);
    assert_eq!(
        settings.terminal_confirm_on_exit,
        TerminalConfirmOnExit::Always
    );
    assert_eq!(
        settings.terminal_confirm_on_kill,
        TerminalConfirmOnKill::Panel
    );
    assert!(!settings.terminal_tabs_enabled);
    assert_eq!(settings.terminal_tabs_default_icon, " code ");
    assert_eq!(
        settings.terminal_tabs_default_color.as_deref(),
        Some(" terminal.ansiBlue ")
    );
    assert!(!settings.terminal_tabs_allow_agent_cli_title);
    assert_eq!(settings.terminal_tabs_title, " ${process} - ${cwd} ");
    assert_eq!(
        settings.terminal_tabs_hide_condition,
        TerminalTabsHideCondition::Never
    );
    assert_eq!(
        settings.terminal_tabs_show_active_terminal,
        TerminalTabsShowActiveTerminal::Always
    );
    assert_eq!(
        settings.terminal_tabs_show_actions,
        TerminalTabsShowActions::Never
    );
    assert_eq!(
        settings.terminal_tabs_focus_mode,
        TerminalTabsFocusMode::DoubleClick
    );
    assert_eq!(settings.terminal_tabs_location, TerminalTabsLocation::Left);
    assert_eq!(
        settings.terminal_right_click_behavior,
        TerminalRightClickBehavior::Paste
    );
    assert_eq!(
        settings.terminal_middle_click_behavior,
        TerminalMiddleClickBehavior::Paste
    );
    assert!(!settings.terminal_alt_click_moves_cursor);
    assert!(!settings.terminal_copy_on_selection);
    assert!(settings.terminal_ignore_bracketed_paste_mode);
    assert_eq!(
        settings.terminal_enable_multi_line_paste_warning,
        TerminalMultiLinePasteWarning::Always
    );
    assert_eq!(settings.terminal_word_separators, ":");
    assert_eq!(settings.terminal_mouse_wheel_scroll_sensitivity, 2.0);
    assert_eq!(settings.terminal_fast_scroll_sensitivity, 8.0);
    assert!(settings.terminal_mouse_wheel_zoom);
    assert!(settings.trim_trailing_whitespace);
    assert!(settings.insert_final_newline);
    assert!(settings.trim_final_newlines);
}

#[test]
fn apply_settings_panel_draft_rejects_terminal_cwd_control_characters() {
    let mut settings = EditorSettings {
        terminal_cwd: Some("workspace".to_owned()),
        ..EditorSettings::default()
    };
    let draft = EditorSettings {
        terminal_cwd: Some("tools\nbad".to_owned()),
        ..EditorSettings::default()
    };

    apply_settings_panel_draft(&mut settings, &draft, "", "");

    assert_eq!(settings.terminal_cwd, None);
}

#[test]
fn apply_settings_panel_draft_preserves_lsp_server_enabled_flag() {
    let mut settings = EditorSettings::default();
    let draft = EditorSettings {
        lsp_servers: vec![LspServerConfig {
            language: "rust".to_owned(),
            command: "rust-analyzer".to_owned(),
            args: Vec::new(),
            extensions: Vec::new(),
            root_markers: vec!["Cargo.toml".to_owned()],
            enabled: false,
        }],
        ..EditorSettings::default()
    };

    apply_settings_panel_draft(&mut settings, &draft, "", "");

    assert_eq!(settings.lsp_servers.len(), 1);
    assert!(
        !settings.lsp_servers[0].enabled,
        "disabling a server must survive apply"
    );
    assert!(
        settings
            .lsp_server_configs()
            .iter()
            .all(|server| server.language != "rust"),
        "disabled servers must not reach the LSP runtime"
    );
}

#[test]
fn apply_settings_panel_draft_normalizes_lsp_server_configs() {
    let mut settings = EditorSettings {
        lsp_servers: vec![LspServerConfig {
            language: "old".to_owned(),
            command: "old-lsp".to_owned(),
            args: Vec::new(),
            extensions: Vec::new(),
            root_markers: Vec::new(),
            enabled: true,
        }],
        ..EditorSettings::default()
    };
    let draft = EditorSettings {
        lsp_servers: vec![
            LspServerConfig {
                language: " typescript\u{200b} ".to_owned(),
                command: " typescript-language-server ".to_owned(),
                args: vec![" --stdio ".to_owned()],
                extensions: vec![" .ts ".to_owned(), ".tsx".to_owned()],
                root_markers: vec![" package.json ".to_owned()],
                enabled: true,
            },
            LspServerConfig {
                language: " ".to_owned(),
                command: "missing-language-lsp".to_owned(),
                args: Vec::new(),
                extensions: Vec::new(),
                root_markers: Vec::new(),
                enabled: true,
            },
            LspServerConfig {
                language: " Python ".to_owned(),
                command: " pyright-langserver ".to_owned(),
                args: vec![" --stdio ".to_owned(), "\u{202e}".to_owned()],
                extensions: vec![".py".to_owned(), "py".to_owned(), ".".to_owned()],
                root_markers: vec![
                    " pyproject.toml ".to_owned(),
                    "pyproject.toml".to_owned(),
                    " .git ".to_owned(),
                ],
                enabled: true,
            },
            LspServerConfig {
                language: "typescript".to_owned(),
                command: "custom-ts-lsp".to_owned(),
                args: vec![" --stdio ".to_owned(), "x\nbad".to_owned()],
                extensions: vec![" .ts ".to_owned(), ".mts".to_owned()],
                root_markers: vec![" .git ".to_owned()],
                enabled: true,
            },
        ],
        ..EditorSettings::default()
    };

    apply_settings_panel_draft(&mut settings, &draft, "", "");

    assert_eq!(settings.lsp_servers.len(), 3);
    let typescript = settings
        .lsp_servers
        .iter()
        .find(|server| server.command == "typescript-language-server")
        .expect("typescript server should be retained");
    let custom_typescript = settings
        .lsp_servers
        .iter()
        .find(|server| server.command == "custom-ts-lsp")
        .expect("second typescript server should be retained");
    let python = settings
        .lsp_servers
        .iter()
        .find(|server| server.language == "python")
        .expect("python server should be retained");

    assert_eq!(typescript.language, "typescript");
    assert_eq!(typescript.args, ["--stdio"]);
    assert_eq!(typescript.extensions, ["ts", "tsx"]);
    assert_eq!(typescript.root_markers, ["package.json"]);
    assert_eq!(custom_typescript.language, "typescript");
    assert_eq!(custom_typescript.args, ["--stdio", "x bad"]);
    assert_eq!(custom_typescript.extensions, ["ts", "mts"]);
    assert_eq!(custom_typescript.root_markers, [".git"]);
    assert_eq!(python.command, "pyright-langserver");
    assert_eq!(python.args, ["--stdio"]);
    assert_eq!(python.extensions, ["py"]);
    assert_eq!(python.root_markers, ["pyproject.toml", ".git"]);
}

#[test]
fn apply_settings_panel_draft_normalizes_custom_theme_paths() {
    let mut settings = EditorSettings {
        custom_theme_paths: vec!["old.toml".to_owned()],
        active_custom_theme_path: Some("old.toml".to_owned()),
        ..EditorSettings::default()
    };
    let draft = EditorSettings {
        custom_theme_paths: vec![
            " .kuroya/themes/night.toml ".to_owned(),
            "".to_owned(),
            "\u{202e}".to_owned(),
            "themes/day.toml".to_owned(),
        ],
        ..EditorSettings::default()
    };

    apply_settings_panel_draft(&mut settings, &draft, "", "");

    assert_eq!(
        settings.custom_theme_paths,
        [".kuroya/themes/night.toml", "themes/day.toml"]
    );
    assert_eq!(settings.active_custom_theme_path, None);
}

#[test]
fn apply_settings_panel_draft_copies_theme_selection() {
    let mut settings = EditorSettings::default();
    let draft = EditorSettings {
        custom_theme_paths: vec!["themes/live.toml".to_owned()],
        active_custom_theme_path: Some("themes/live.toml".to_owned()),
        theme: ThemeSettings {
            name: "Live Theme".to_owned(),
            background: [1, 2, 3],
            panel: [4, 5, 6],
            panel_alt: [7, 8, 9],
            text: [10, 11, 12],
            muted_text: [13, 14, 15],
            accent: [16, 17, 18],
            selection: None,
            warning: [19, 20, 21],
            error: [22, 23, 24],
        },
        ..EditorSettings::default()
    };

    apply_settings_panel_draft(&mut settings, &draft, "", "");

    assert_eq!(settings.theme, draft.theme);
    assert_eq!(
        settings.active_custom_theme_path.as_deref(),
        Some("themes/live.toml")
    );
}

#[test]
fn apply_settings_panel_draft_caps_custom_theme_paths_and_preserves_active_match() {
    let active_path = "theme-42.toml".to_owned();
    let mut settings = EditorSettings {
        active_custom_theme_path: Some(active_path.clone()),
        ..EditorSettings::default()
    };
    let mut paths = (0..300)
        .map(|index| format!(" theme-{index}.toml "))
        .collect::<Vec<_>>();
    paths.push("theme-42.toml".to_owned());
    paths.push("theme-42.toml".to_owned());
    paths.push("x".repeat(9_000));
    let draft = EditorSettings {
        custom_theme_paths: paths,
        ..EditorSettings::default()
    };

    apply_settings_panel_draft(&mut settings, &draft, "", "");

    assert_eq!(settings.custom_theme_paths.len(), 256);
    assert!(
        settings
            .custom_theme_paths
            .iter()
            .all(|path| path.chars().count() <= 4096)
    );
    assert_eq!(settings.active_custom_theme_path, Some(active_path));
}

#[test]
fn apply_settings_panel_draft_falls_back_for_blank_required_text_settings() {
    let mut settings = EditorSettings::default();
    let draft = EditorSettings {
        font_family: " \t ".to_owned(),
        aria_label: " \t ".to_owned(),
        minimap_mark_section_header_regex: " \t ".to_owned(),
        git_default_branch_name: " \t ".to_owned(),
        scm_input_font_family: " \t ".to_owned(),
        ..EditorSettings::default()
    };

    apply_settings_panel_draft(&mut settings, &draft, "", "");

    assert_eq!(
        settings.font_family,
        kuroya_core::DEFAULT_EDITOR_FONT_FAMILY
    );
    assert_eq!(settings.aria_label, kuroya_core::DEFAULT_EDITOR_ARIA_LABEL);
    assert_eq!(
        settings.minimap_mark_section_header_regex,
        kuroya_core::DEFAULT_EDITOR_MINIMAP_MARK_SECTION_HEADER_REGEX
    );
    assert_eq!(
        settings.git_default_branch_name,
        kuroya_core::DEFAULT_GIT_DEFAULT_BRANCH_NAME
    );
    assert_eq!(
        settings.scm_input_font_family,
        kuroya_core::DEFAULT_SCM_INPUT_FONT_FAMILY
    );
}

#[test]
fn apply_settings_panel_draft_sanitizes_hidden_controls_in_text_settings() {
    let mut settings = EditorSettings::default();
    let draft = EditorSettings {
        font_family: "Jet\u{202e}Brains\u{200b} Mono".to_owned(),
        read_only_message: "Read\u{2028}only".to_owned(),
        word_segmenter_locales: vec![
            " en\u{200f}US ".to_owned(),
            "\u{202e}".to_owned(),
            "x".repeat(9_000),
        ],
        git_branch_prefix: " feature\u{feff}/ ".to_owned(),
        ..EditorSettings::default()
    };

    apply_settings_panel_draft(&mut settings, &draft, "", "");

    assert_eq!(settings.font_family, "JetBrains Mono");
    assert_eq!(settings.read_only_message, "Read only");
    assert_eq!(settings.word_segmenter_locales.len(), 2);
    assert_eq!(settings.word_segmenter_locales[0], " enUS ");
    assert!(settings.word_segmenter_locales[1].chars().count() <= 8_192);
    assert_eq!(settings.git_branch_prefix, " feature/ ");
}

fn serialized_settings_fields(settings: &EditorSettings) -> Map<String, Value> {
    match serde_json::to_value(settings).expect("settings should serialize") {
        Value::Object(fields) => fields,
        other => panic!("settings should serialize as an object, got {other:?}"),
    }
}

fn draft_apply_intentionally_excluded_fields() -> BTreeSet<&'static str> {
    BTreeSet::from(["keymap", "schema_version", "updates_github_repository"])
}

fn draft_apply_intentionally_normalized_fields() -> BTreeSet<&'static str> {
    BTreeSet::from([
        "font_ligatures",
        "font_variations",
        "git_branch_protection",
        "git_branch_random_name_dictionary",
        "git_checkout_type",
        "git_commands_to_log",
        "git_path",
        "git_worktree_include_files",
        "inline_suggest_experimental_suppress_inline_suggestions",
        "terminal_shell_args",
        "word_segmenter_locales",
    ])
}
