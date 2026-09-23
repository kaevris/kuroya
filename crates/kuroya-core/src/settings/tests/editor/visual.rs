use super::*;

#[test]
fn editor_visual_settings_parse_vs_code_style_values() {
    let settings: EditorSettings = toml::from_str(
            "font_family = \"Cascadia Code, Consolas, monospace\"\n\
             font_weight = \"600\"\n\
             font_ligatures = true\n\
             font_variations = \"true\"\n\
             letter_spacing = 1.5\n\
             automatic_layout = true\n\
             disable_layer_hinting = true\n\
             disable_monospace_optimizations = true\n\
             extra_editor_class_name = \"workbench-editor\"\n\
             allow_variable_line_heights = false\n\
             allow_variable_fonts = false\n\
             allow_variable_fonts_in_accessibility_mode = true\n\
             accessibility_support = \"on\"\n\
             accessibility_page_size = 250\n\
             aria_label = \"Source editor\"\n\
             aria_required = true\n\
             screen_reader_announce_inline_suggestion = false\n\
             tab_index = -1\n\
             read_only = true\n\
             read_only_message = \"Generated file\"\n\
             dom_read_only = true\n\
             edit_context = false\n\
             render_rich_screen_reader_content = true\n\
             trim_whitespace_on_delete = true\n\
             unusual_line_terminators = \"auto\"\n\
             use_shadow_dom = false\n\
             use_tab_stops = false\n\
             fixed_overflow_widgets = true\n\
             allow_overflow = false\n\
             line_numbers = \"relative\"\n\
             render_whitespace = \"all\"\n\
             render_final_newline = \"dimmed\"\n\
             render_line_highlight = \"all\"\n\
             render_line_highlight_only_when_focus = true\n\
             smart_select_select_leading_and_trailing_whitespace = false\n\
             smart_select_select_subwords = false\n\
             find_seed_search_string_from_selection = \"selection\"\n\
             find_auto_find_in_selection = \"multiline\"\n\
             find_on_type = false\n\
             find_cursor_move_on_type = false\n\
             find_loop = false\n\
             find_close_on_result = true\n\
             find_global_find_clipboard = true\n\
             find_add_extra_space_on_top = false\n\
             find_history = \"never\"\n\
             find_replace_history = \"never\"\n\
             auto_closing_comments = \"beforeWhitespace\"\n\
             auto_closing_delete = \"never\"\n\
             auto_closing_overtype = \"always\"\n\
             auto_indent_on_paste = true\n\
             auto_indent_on_paste_within_string = false\n\
             sticky_tab_stops = true\n\
             linked_editing = true\n\
             rename_on_type = true\n\
             tab_focus_mode = true\n\
             vim_keybindings = true\n\
             quick_suggestions_delay_ms = 25\n\
             accept_suggestion_on_commit_character = false\n\
             suggest_selection = \"recentlyUsedByPrefix\"\n\
             suggest_insert_mode = \"replace\"\n\
             suggest_filter_graceful = false\n\
             suggest_snippets_prevent_quick_suggestions = true\n\
             suggest_locality_bonus = true\n\
             suggest_share_suggest_selections = true\n\
             suggest_selection_mode = \"whenQuickSuggestion\"\n\
             suggest_show_icons = false\n\
             suggest_show_status_bar = true\n\
             suggest_preview = true\n\
             suggest_preview_mode = \"prefix\"\n\
             suggest_show_inline_details = false\n\
             suggest_show_methods = false\n\
             suggest_show_functions = false\n\
             suggest_show_constructors = false\n\
             suggest_show_deprecated = false\n\
             suggest_show_fields = false\n\
             suggest_show_variables = false\n\
             suggest_show_classes = false\n\
             suggest_show_structs = false\n\
             suggest_show_interfaces = false\n\
             suggest_show_modules = false\n\
             suggest_show_properties = false\n\
             suggest_show_events = false\n\
             suggest_show_operators = false\n\
             suggest_show_units = false\n\
             suggest_show_values = false\n\
             suggest_show_constants = false\n\
             suggest_show_enums = false\n\
             suggest_show_enum_members = false\n\
             suggest_show_keywords = false\n\
             suggest_show_words = false\n\
             suggest_show_colors = false\n\
             suggest_show_files = false\n\
             suggest_show_references = false\n\
             suggest_show_customcolors = false\n\
             suggest_show_folders = false\n\
             suggest_show_type_parameters = false\n\
             suggest_show_snippets = false\n\
             suggest_show_users = false\n\
             suggest_show_issues = false\n\
             suggest_match_on_word_start_only = false\n\
             suggest_font_size = 15\n\
             suggest_line_height = 24\n\
             tab_completion = \"onlySnippets\"\n\
             snippet_suggestions = \"top\"\n\
             hover_delay_ms = 450\n\
             hover_hiding_delay_ms = 900\n\
             hover_sticky = false\n\
             hover_above = false\n\
             hover_show_long_line_warning = false\n\
             inline_suggest_enabled = false\n\
             inline_suggest_mode = \"prefix\"\n\
             inline_suggest_show_toolbar = \"never\"\n\
             inline_suggest_keep_on_blur = true\n\
             inline_suggest_font_family = \"JetBrains Mono\"\n\
             inline_suggest_syntax_highlighting_enabled = false\n\
             inline_suggest_suppress_suggestions = true\n\
             inline_suggest_suppress_in_snippet_mode = false\n\
             inline_suggest_min_show_delay_ms = 125\n\
             inline_suggest_edits_enabled = false\n\
             inline_suggest_edits_show_collapsed = true\n\
             inline_suggest_edits_render_side_by_side = \"never\"\n\
             inline_suggest_edits_allow_code_shifting = \"horizontal\"\n\
             inline_suggest_edits_show_long_distance_hint = false\n\
             inline_suggest_trigger_command_on_provider_change = true\n\
             inline_suggest_experimental_suppress_inline_suggestions = \"ext.one,ext.two\"\n\
             inline_suggest_experimental_show_on_suggest_conflict = \"whenSuggestListIsIncomplete\"\n\
             inline_suggest_experimental_empty_response_information = false\n\
             inline_completions_accessibility_verbose = true\n\
             comments_insert_space = false\n\
             comments_ignore_empty_lines = false\n\
             paste_as_enabled = false\n\
             paste_as_show_paste_selector = \"never\"\n\
             format_on_type = true\n\
             double_click_selects_block = false\n\
             drag_and_drop = false\n\
             drop_into_editor_enabled = false\n\
             drop_into_editor_show_drop_selector = \"never\"\n\
             diff_ignore_trim_whitespace = false\n\
             diff_algorithm = \"legacy\"\n\
             diff_render_side_by_side = false\n\
             diff_enable_split_view_resizing = false\n\
             diff_split_view_default_ratio = 0.35\n\
             diff_render_side_by_side_inline_breakpoint = 720\n\
             diff_use_inline_view_when_space_is_limited = false\n\
             diff_compact_mode = true\n\
             diff_original_editable = true\n\
             diff_code_lens = true\n\
             diff_accessibility_verbose = true\n\
             diff_hide_unchanged_regions = false\n\
             diff_context_lines = 1\n\
             diff_hide_unchanged_regions_minimum_line_count = 9\n\
             diff_hide_unchanged_regions_reveal_line_count = 15\n\
             diff_max_computation_time_ms = 2500\n\
             diff_max_file_size_mb = 12\n\
             diff_render_gutter_menu = false\n\
             diff_render_indicators = false\n\
             diff_render_margin_revert_icon = false\n\
             diff_render_overview_ruler = false\n\
             diff_experimental_show_moves = true\n\
             diff_experimental_show_empty_decorations = false\n\
             diff_experimental_use_true_inline_view = true\n\
             diff_word_wrap = \"off\"\n\
             diff_only_show_accessible_viewer = true\n\
             diff_is_in_embedded_editor = true\n\
             git_enabled = false\n\
             git_add_ai_co_author = \"all\"\n\
             git_allow_force_push = true\n\
             git_allow_no_verify_commit = true\n\
             git_auto_repository_detection = \"subFolders\"\n\
             git_autofetch = \"all\"\n\
             git_autofetch_period = 90\n\
             git_autorefresh = false\n\
             git_auto_stash = true\n\
             git_commands_to_log = [\"fetch\", \"pull\"]\n\
             git_confirm_force_push = false\n\
             git_confirm_no_verify_commit = false\n\
             git_confirm_sync = false\n\
             git_ignore_limit_warning = true\n\
             git_ignore_submodules = true\n\
             git_ignored_repositories = [\"C:/repo/ignored\", \"../other\"]\n\
             git_repository_scan_ignored_folders = [\"node_modules\", \"dist\"]\n\
             git_open_repository_in_parent_folders = \"never\"\n\
             git_detect_submodules = false\n\
             git_detect_submodules_limit = 3\n\
             git_repository_scan_max_depth = 4\n\
             git_detect_worktrees = true\n\
             git_detect_worktrees_limit = 7\n\
             git_discard_untracked_changes_to_trash = false\n\
             git_diagnostics_commit_hook_enabled = true\n\
             git_diagnostics_commit_hook_sources = { \"*\" = \"warning\", rust = \"error\" }\n\
             git_enable_commit_signing = true\n\
             git_enable_status_bar_sync = false\n\
             git_fetch_on_pull = true\n\
             git_follow_tags_when_sync = true\n\
             git_ignore_legacy_warning = true\n\
             git_ignore_missing_git_warning = true\n\
             git_ignore_rebase_warning = true\n\
             git_ignore_windows_git27_warning = true\n\
             git_merge_editor = true\n\
             git_open_after_clone = \"alwaysNewWindow\"\n\
             git_optimistic_update = false\n\
             git_path = [\"C:/Git/bin/git.exe\", \"D:/Git/bin/git.exe\"]\n\
             git_post_commit_command = \"sync\"\n\
             git_prune_on_fetch = true\n\
             git_pull_before_checkout = true\n\
             git_pull_tags = false\n\
             git_rebase_when_sync = true\n\
             git_remember_post_commit_command = true\n\
             git_replace_tags_when_pull = true\n\
             git_support_cancellation = true\n\
             git_terminal_authentication = false\n\
             git_terminal_git_editor = true\n\
             git_use_force_push_if_includes = false\n\
             git_use_force_push_with_lease = false\n\
             git_use_integrated_ask_pass = false\n\
             git_worktree_include_files = [\"packages/app\"]\n\
             git_default_branch_name = \"trunk\"\n\
             git_default_clone_directory = \"C:/src\"\n\
             git_similarity_threshold = 80\n\
             scm_default_view_mode = \"tree\"\n\
             scm_default_view_sort_key = \"status\"\n\
             scm_auto_reveal = false\n\
             scm_count_badge = \"off\"\n\
             scm_provider_count_badge = \"visible\"\n\
             scm_always_show_repositories = true\n\
             scm_repositories_visible = 2\n\
             scm_compact_folders = false\n\
             scm_always_show_actions = true\n\
             scm_show_action_button = false\n\
             git_show_commit_input = false\n\
             git_show_push_success_notification = true\n\
             git_use_editor_as_commit_input = false\n\
             git_verbose_commit = true\n\
             git_show_action_button_commit = false\n\
             git_always_sign_off = true\n\
             git_confirm_committed_delete = false\n\
             git_confirm_empty_commits = false\n\
             git_require_user_config = false\n\
             git_show_progress = false\n\
             git_show_reference_details = false\n\
             git_timeline_show_author = false\n\
             git_timeline_show_uncommitted = true\n\
             git_timeline_date = \"authored\"\n\
             git_show_inline_open_file_action = false\n\
             git_count_badge = \"tracked\"\n\
             git_untracked_changes = \"separate\"\n\
             git_open_diff_on_click = false\n\
             git_close_diff_on_operation = true\n\
             git_always_show_staged_changes_resource_group = true\n\
             git_checkout_type = [\"remote\", \"tags\"]\n\
             git_branch_sort_order = \"alphabetically\"\n\
             git_branch_prefix = \"feature/\"\n\
             git_branch_random_name_enable = true\n\
             git_branch_random_name_dictionary = [\"colors\", \"numbers\"]\n\
             git_branch_validation_regex = \"^feature/\"\n\
             git_branch_whitespace_char = \"_\"\n\
             git_decorations_enabled = false\n\
             git_enable_smart_commit = true\n\
             git_suggest_smart_commit = false\n\
             git_smart_commit_changes = \"tracked\"\n\
             git_prompt_to_save_files_before_commit = \"staged\"\n\
             git_prompt_to_save_files_before_stash = \"never\"\n\
             git_branch_protection = [\"main\", \"release/*\"]\n\
             git_branch_protection_prompt = \"alwaysCommitToNewBranch\"\n\
             git_status_limit = 250\n\
             git_use_commit_input_as_stash_message = true\n\
             git_commit_short_hash_length = 12\n\
             git_input_validation = true\n\
             git_input_validation_length = 80\n\
             git_input_validation_subject_length = \"inherit\"\n\
             git_blame_status_bar_item_enabled = false\n\
             git_blame_editor_decoration_enabled = true\n\
             git_blame_editor_decoration_disable_hover = true\n\
             git_blame_ignore_whitespace = true\n\
             git_blame_status_bar_item_template = \"${subject} - ${authorName}\"\n\
             git_blame_editor_decoration_template = \"${hash}: ${subject}\"\n\
             scm_show_input_action_button = false\n\
             scm_input_min_line_count = 2\n\
             scm_input_max_line_count = 8\n\
             scm_input_font_family = \"editor\"\n\
             scm_input_font_size = 15.0\n\
             scm_diff_decorations = \"minimap\"\n\
             scm_diff_decorations_gutter_action = \"none\"\n\
             scm_diff_decorations_gutter_visibility = \"hover\"\n\
             scm_diff_decorations_gutter_width = 5\n\
             scm_diff_decorations_gutter_pattern = { added = true, modified = false }\n\
             scm_diff_decorations_ignore_trim_whitespace = \"inherit\"\n\
             scm_graph_page_on_scroll = false\n\
             scm_graph_page_size = 125\n\
             scm_graph_badges = \"all\"\n\
             scm_graph_show_incoming_changes = false\n\
             scm_graph_show_outgoing_changes = false\n\
             bracket_pair_colorization = false\n\
             bracket_pair_colorization_independent_color_pool_per_bracket_type = true\n\
             bracket_pair_guides = true\n\
             bracket_pair_guides_horizontal = \"active\"\n\
             highlight_active_bracket_pair = false\n\
             match_brackets = \"near\"\n\
             folding = false\n\
             folding_highlight = false\n\
             folding_imports_by_default = false\n\
             folding_maximum_regions = 123\n\
             folding_strategy = \"indentation\"\n\
             unfold_on_click_after_end_of_line = true\n\
             show_folding_controls = \"never\"\n\
             mouse_style = \"copy\"\n\
             cursor_smooth_caret_animation = \"explicit\"\n\
             cursor_style = \"line-thin\"\n\
             overtype_cursor_style = \"block-outline\"\n\
             overtype_on_paste = false\n\
             cursor_blinking = true\n\
             cursor_width = 4.0\n\
             cursor_height = 18\n\
             cursor_surrounding_lines = 3\n\
             cursor_surrounding_lines_style = \"all\"\n\
             line_height = 22.0\n\
             scroll_beyond_last_line = false\n\
             scroll_beyond_last_column = 12\n\
             scroll_on_middle_click = true\n\
             scroll_predominant_axis = false\n\
             inertial_scroll = true\n\
             mouse_wheel_scroll_sensitivity = 2.5\n\
             fast_scroll_sensitivity = 9.0\n\
             mouse_wheel_zoom = true\n\
             scrollbar_vertical = \"visible\"\n\
             scrollbar_horizontal = \"hidden\"\n\
             scrollbar_vertical_scrollbar_size = 18\n\
             scrollbar_horizontal_scrollbar_size = 16\n\
             scrollbar_ignore_horizontal_scrollbar_in_content_height = true\n\
             padding_top = 12\n\
             padding_bottom = 24\n\
             links = false\n\
             show_unused = false\n\
             show_deprecated = false\n\
             contextmenu = false\n\
             color_decorators = false\n\
             color_decorators_activated_on = \"hover\"\n\
             color_decorators_limit = 42\n\
             default_color_decorators = \"never\"\n\
             sticky_scroll = false\n\
             sticky_scroll_max_line_count = 8\n\
             sticky_scroll_default_model = \"indentationModel\"\n\
             sticky_scroll_scroll_with_editor = false\n\
             minimap = false\n\
             minimap_side = \"left\"\n\
             minimap_autohide = \"scroll\"\n\
             minimap_size = \"fit\"\n\
             minimap_show_slider = \"always\"\n\
             minimap_scale = 3\n\
             minimap_render_characters = false\n\
             minimap_max_column = 80\n\
             minimap_show_region_section_headers = false\n\
             minimap_show_mark_section_headers = false\n\
             minimap_mark_section_header_regex = \"MARK: (?<label>.*)\"\n\
             minimap_section_header_font_size = 12.0\n\
             minimap_section_header_letter_spacing = 2.0\n\
             multi_cursor_modifier = \"ctrlCmd\"\n\
             multi_cursor_merge_overlapping = false\n\
             multi_cursor_paste = \"full\"\n\
             multi_cursor_limit = 200\n\
             column_selection = true\n\
             mouse_middle_click_action = \"openLink\"\n\
             empty_selection_clipboard = false\n\
             selection_clipboard = false\n\
             copy_with_syntax_highlighting = false\n\
             glyph_margin = false\n\
             ruler_column = 100\n\
             overview_ruler_border = false\n\
             overview_ruler_lanes = 2\n\
             hide_cursor_in_overview_ruler = true\n\
             status_bar_visible = false\n\
             devtools_verbose_logging = true\n\
             devtools_profiling_enabled = true\n\
             line_decorations_width = 12.5\n\
             line_numbers_min_chars = 8\n\
             select_on_line_numbers = false\n\
             word_wrap = \"bounded\"\n\
             word_wrap_override1 = \"off\"\n\
             word_wrap_override2 = \"on\"\n\
             word_wrap_break_after_characters = \" ,;\"\n\
             word_wrap_break_before_characters = \"([{\"\n\
             word_wrap_column = 96\n\
             wrapping_indent = \"deepIndent\"\n\
             wrapping_strategy = \"advanced\"\n\
             wrap_on_escaped_line_feeds = true\n\
             word_break = \"keepAll\"\n\
             reveal_horizontal_right_padding = 30\n\
             rounded_selection = false\n\
             stop_rendering_line_after = -1\n\
             autosave_mode = \"onFocusChange\"\n\
             autosave_delay_ms = 1500\n\
             window_zoom_level = 1.5\n\
             render_control_characters = true\n\
             unicode_highlight_ambiguous_characters = false\n\
             unicode_highlight_invisible_characters = false\n\
             unicode_highlight_non_basic_ascii = \"on\"\n\
             unicode_highlight_include_comments = true\n\
             unicode_highlight_include_strings = \"inUntrustedWorkspace\"\n\
             unicode_highlight_allowed_characters = { \"Α\" = true, \"ß\" = false }\n\
             unicode_highlight_allowed_locales = { \"_os\" = false, \"ja\" = true }\n\
             indent_guides = false\n\
             highlight_active_indentation = \"always\"\n\
             insert_spaces = false\n\
             detect_indentation = false\n\
             word_separators = \".\"\n\
             word_segmenter_locales = [\"ja\", \"zh-CN\"]\n\
             auto_indent = false\n\
             auto_closing_brackets = false\n\
             auto_closing_quotes = false\n\
             experimental_gpu_acceleration = \"on\"\n\
             experimental_whitespace_rendering = \"font\"\n\
             auto_surround = false\n\
             quick_suggestions = true\n\
             suggest_on_trigger_characters = false\n\
             accept_suggestion_on_enter = false\n\
             accept_suggestion_on_tab = true\n\
             hover_enabled = false\n\
             lightbulb = \"on\"\n\
             render_validation_decorations = \"off\"\n\
             document_highlights_enabled = false\n\
             code_lens = false\n\
             code_lens_font_family = \"Cascadia Code\"\n\
             code_lens_font_size = 11\n\
             goto_location_multiple_definitions = \"gotoAndPeek\"\n\
             goto_location_multiple_type_definitions = \"goto\"\n\
             goto_location_multiple_declarations = \"peek\"\n\
             goto_location_multiple_implementations = \"gotoAndPeek\"\n\
             goto_location_multiple_references = \"goto\"\n\
             goto_location_multiple_tests = \"peek\"\n\
             goto_location_alternative_definition_command = \"editor.action.peekDefinition\"\n\
             goto_location_alternative_type_definition_command = \"editor.action.peekTypeDefinition\"\n\
             goto_location_alternative_declaration_command = \"editor.action.peekDeclaration\"\n\
             goto_location_alternative_implementation_command = \"editor.action.peekImplementation\"\n\
             goto_location_alternative_reference_command = \"editor.action.referenceSearch.trigger\"\n\
             goto_location_alternative_tests_command = \"editor.action.goToReferences\"\n\
             peek_widget_default_focus = \"editor\"\n\
             placeholder = \"Type here\"\n\
             definition_link_opens_in_peek = true\n\
             inlay_hints = false\n\
             inlay_hints_font_family = \"Cascadia Code\"\n\
             inlay_hints_font_size = 13\n\
             inlay_hints_padding = true\n\
             inlay_hints_maximum_length = 25\n\
             parameter_hints_enabled = false\n\
             parameter_hints_on_trigger_characters = false\n\
             parameter_hints_cycle = false\n\
             format_on_save = true\n\
             format_on_paste = true\n\
             trim_trailing_whitespace = true\n\
             insert_final_newline = true\n\
             trim_final_newlines = true\n",
        )
        .expect("editor visual settings should load");

    assert_eq!(settings.font_family, "Cascadia Code, Consolas, monospace");
    assert_eq!(settings.font_weight, "600");
    assert_eq!(settings.font_ligatures, EDITOR_FONT_LIGATURES_ON);
    assert_eq!(settings.font_variations, EDITOR_FONT_VARIATIONS_TRANSLATE);
    assert_eq!(settings.letter_spacing, 1.5);
    assert!(settings.automatic_layout);
    assert!(settings.disable_layer_hinting);
    assert!(settings.disable_monospace_optimizations);
    assert_eq!(settings.extra_editor_class_name, "workbench-editor");
    assert!(!settings.allow_variable_line_heights);
    assert!(!settings.allow_variable_fonts);
    assert!(settings.allow_variable_fonts_in_accessibility_mode);
    assert_eq!(
        settings.accessibility_support,
        EditorAccessibilitySupport::On
    );
    assert_eq!(settings.accessibility_page_size, 250);
    assert_eq!(settings.aria_label, "Source editor");
    assert!(settings.aria_required);
    assert!(!settings.screen_reader_announce_inline_suggestion);
    assert_eq!(settings.tab_index, -1);
    assert!(settings.read_only);
    assert_eq!(settings.read_only_message, "Generated file");
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
    assert_eq!(settings.line_numbers, EditorLineNumbers::Relative);
    assert_eq!(settings.render_whitespace, EditorRenderWhitespace::All);
    assert_eq!(
        settings.render_final_newline,
        EditorRenderFinalNewline::Dimmed
    );
    assert_eq!(
        settings.render_line_highlight,
        EditorRenderLineHighlight::All
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
        EditorAutoClosingStrategy::BeforeWhitespace
    );
    assert_eq!(
        settings.auto_closing_delete,
        EditorAutoClosingEditStrategy::Never
    );
    assert_eq!(
        settings.auto_closing_overtype,
        EditorAutoClosingEditStrategy::Always
    );
    assert!(settings.auto_indent_on_paste);
    assert!(!settings.auto_indent_on_paste_within_string);
    assert!(settings.sticky_tab_stops);
    assert!(settings.linked_editing);
    assert!(settings.rename_on_type);
    assert!(settings.tab_focus_mode);
    assert!(settings.vim_keybindings);
    assert_eq!(settings.quick_suggestions_delay_ms, 25);
    assert!(!settings.accept_suggestion_on_commit_character);
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
    assert_eq!(settings.inline_suggest_font_family, "JetBrains Mono");
    assert!(!settings.inline_suggest_syntax_highlighting_enabled);
    assert!(settings.inline_suggest_suppress_suggestions);
    assert!(!settings.inline_suggest_suppress_in_snippet_mode);
    assert_eq!(settings.inline_suggest_min_show_delay_ms, 125);
    assert!(!settings.inline_suggest_edits_enabled);
    assert!(settings.inline_suggest_edits_show_collapsed);
    assert_eq!(
        settings.inline_suggest_edits_render_side_by_side,
        EditorInlineSuggestEditsRenderSideBySide::Never
    );
    assert_eq!(
        settings.inline_suggest_edits_allow_code_shifting,
        EditorInlineSuggestEditsAllowCodeShifting::Horizontal
    );
    assert!(!settings.inline_suggest_edits_show_long_distance_hint);
    assert!(settings.inline_suggest_trigger_command_on_provider_change);
    assert_eq!(
        settings.inline_suggest_experimental_suppress_inline_suggestions,
        "ext.one,ext.two"
    );
    assert_eq!(
        settings.inline_suggest_experimental_show_on_suggest_conflict,
        EditorInlineSuggestShowOnSuggestConflict::WhenSuggestListIsIncomplete
    );
    assert!(!settings.inline_suggest_experimental_empty_response_information);
    assert!(settings.inline_completions_accessibility_verbose);
    assert!(!settings.comments_insert_space);
    assert!(!settings.comments_ignore_empty_lines);
    assert!(!settings.paste_as_enabled);
    assert_eq!(
        settings.paste_as_show_paste_selector,
        EditorPasteAsShowPasteSelector::Never
    );
    assert!(settings.format_on_type);
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
    assert!(!settings.diff_hide_unchanged_regions);
    assert_eq!(settings.diff_context_lines, 1);
    assert_eq!(settings.diff_hide_unchanged_regions_minimum_line_count, 9);
    assert_eq!(settings.diff_hide_unchanged_regions_reveal_line_count, 15);
    assert_eq!(settings.diff_max_computation_time_ms, 2500);
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
    assert_eq!(settings.git_add_ai_co_author, GitAddAiCoAuthor::All);
    assert!(settings.git_allow_force_push);
    assert!(settings.git_allow_no_verify_commit);
    assert_eq!(
        settings.git_auto_repository_detection,
        GitAutoRepositoryDetection::SubFolders
    );
    assert_eq!(settings.git_autofetch, GitAutoFetch::All);
    assert_eq!(settings.git_autofetch_period, 90);
    assert!(!settings.git_autorefresh);
    assert!(settings.git_auto_stash);
    assert_eq!(settings.git_commands_to_log, ["fetch", "pull"]);
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
        GitOpenRepositoryInParentFolders::Never
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
    assert_eq!(
        settings.git_diagnostics_commit_hook_sources.get("rust"),
        Some(&"error".to_owned())
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
        GitOpenAfterClone::AlwaysNewWindow
    );
    assert!(!settings.git_optimistic_update);
    assert_eq!(
        settings.git_path,
        ["C:/Git/bin/git.exe", "D:/Git/bin/git.exe"]
    );
    assert_eq!(settings.git_post_commit_command, GitPostCommitCommand::Sync);
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
    assert_eq!(settings.git_worktree_include_files, ["packages/app"]);
    assert_eq!(settings.git_default_branch_name, "trunk");
    assert_eq!(
        settings.git_default_clone_directory,
        Some("C:/src".to_owned())
    );
    assert_eq!(settings.git_similarity_threshold, 80);
    assert_eq!(settings.scm_default_view_mode, ScmDefaultViewMode::Tree);
    assert_eq!(
        settings.scm_default_view_sort_key,
        ScmDefaultViewSortKey::Status
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
    assert_eq!(settings.git_timeline_date, GitTimelineDate::Authored);
    assert!(!settings.git_show_inline_open_file_action);
    assert_eq!(settings.git_count_badge, GitCountBadge::Tracked);
    assert_eq!(
        settings.git_untracked_changes,
        GitUntrackedChanges::Separate
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
        GitBranchSortOrder::Alphabetically
    );
    assert_eq!(settings.git_branch_prefix, "feature/");
    assert!(settings.git_branch_random_name_enable);
    assert_eq!(
        settings.git_branch_random_name_dictionary,
        ["colors", "numbers"]
    );
    assert_eq!(settings.git_branch_validation_regex, "^feature/");
    assert_eq!(settings.git_branch_whitespace_char, "_");
    assert!(!settings.git_decorations_enabled);
    assert!(settings.git_enable_smart_commit);
    assert!(!settings.git_suggest_smart_commit);
    assert_eq!(
        settings.git_smart_commit_changes,
        GitSmartCommitChanges::Tracked
    );
    assert_eq!(
        settings.git_prompt_to_save_files_before_commit,
        GitPromptToSaveFilesBeforeCommit::Staged
    );
    assert_eq!(
        settings.git_prompt_to_save_files_before_stash,
        GitPromptToSaveFilesBeforeCommit::Never
    );
    assert_eq!(settings.git_branch_protection, ["main", "release/*"]);
    assert_eq!(
        settings.git_branch_protection_prompt,
        GitBranchProtectionPrompt::AlwaysCommitToNewBranch
    );
    assert_eq!(settings.git_status_limit, 250);
    assert!(settings.git_use_commit_input_as_stash_message);
    assert_eq!(settings.git_commit_short_hash_length, 12);
    assert!(settings.git_input_validation);
    assert_eq!(settings.git_input_validation_length, 80);
    assert_eq!(
        settings.git_input_validation_subject_length,
        GitInputValidationSubjectLength::Inherit
    );
    assert!(!settings.git_blame_status_bar_item_enabled);
    assert!(settings.git_blame_editor_decoration_enabled);
    assert!(settings.git_blame_editor_decoration_disable_hover);
    assert!(settings.git_blame_ignore_whitespace);
    assert_eq!(
        settings.git_blame_status_bar_item_template,
        "${subject} - ${authorName}"
    );
    assert_eq!(
        settings.git_blame_editor_decoration_template,
        "${hash}: ${subject}"
    );
    assert!(!settings.scm_show_input_action_button);
    assert_eq!(settings.scm_input_min_line_count, 2);
    assert_eq!(settings.scm_input_max_line_count, 8);
    assert_eq!(settings.scm_input_font_family, "editor");
    assert_eq!(settings.scm_input_font_size, 15.0);
    assert_eq!(settings.scm_diff_decorations, ScmDiffDecorations::Minimap);
    assert_eq!(
        settings.scm_diff_decorations_gutter_action,
        ScmDiffDecorationsGutterAction::None
    );
    assert_eq!(
        settings.scm_diff_decorations_gutter_visibility,
        ScmDiffDecorationsGutterVisibility::Hover
    );
    assert_eq!(settings.scm_diff_decorations_gutter_width, 5);
    assert_eq!(
        settings.scm_diff_decorations_gutter_pattern,
        ScmDiffDecorationsGutterPattern {
            added: true,
            modified: false
        }
    );
    assert_eq!(
        settings.scm_diff_decorations_ignore_trim_whitespace,
        ScmDiffDecorationsIgnoreTrimWhitespace::Inherit
    );
    assert!(!settings.scm_graph_page_on_scroll);
    assert_eq!(settings.scm_graph_page_size, 125);
    assert_eq!(settings.scm_graph_badges, ScmGraphBadges::All);
    assert!(!settings.scm_graph_show_incoming_changes);
    assert!(!settings.scm_graph_show_outgoing_changes);
    assert!(!settings.bracket_pair_colorization);
    assert!(settings.bracket_pair_colorization_independent_color_pool_per_bracket_type);
    assert_eq!(settings.bracket_pair_guides, EditorBracketPairGuideMode::On);
    assert_eq!(
        settings.bracket_pair_guides_horizontal,
        EditorBracketPairGuideMode::Active
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
    assert_eq!(settings.cursor_surrounding_lines, 3);
    assert_eq!(
        settings.cursor_surrounding_lines_style,
        EditorCursorSurroundingLinesStyle::All
    );
    assert_eq!(settings.line_height, 22.0);
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
        EditorScrollbarVisibility::Hidden
    );
    assert_eq!(settings.scrollbar_vertical_scrollbar_size, 18);
    assert_eq!(settings.scrollbar_horizontal_scrollbar_size, 16);
    assert!(settings.scrollbar_ignore_horizontal_scrollbar_in_content_height);
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
    assert!(!settings.minimap);
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
    assert_eq!(
        settings.line_decorations_width,
        EditorLineDecorationsWidth::Pixels(12.5)
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
    assert_eq!(settings.autosave_mode, EditorAutoSaveMode::OnFocusChange);
    assert_eq!(settings.autosave_delay_ms, 1500);
    assert_eq!(settings.window_zoom_level, 1.5);
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
        BTreeMap::from([("Α".to_owned(), true), ("ß".to_owned(), false)])
    );
    assert_eq!(
        settings.unicode_highlight_allowed_locales,
        BTreeMap::from([("_os".to_owned(), false), ("ja".to_owned(), true)])
    );
    assert!(!settings.indent_guides);
    assert_eq!(
        settings.highlight_active_indentation,
        EditorHighlightActiveIndentation::Always
    );
    assert!(!settings.insert_spaces);
    assert!(!settings.detect_indentation);
    assert_eq!(settings.word_separators, ".");
    assert_eq!(settings.word_segmenter_locales, ["ja", "zh-CN"]);
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
    assert!(!settings.hover_enabled);
    assert_eq!(settings.lightbulb, EditorLightbulbMode::On);
    assert_eq!(
        settings.render_validation_decorations,
        EditorRenderValidationDecorations::Off
    );
    assert!(!settings.document_highlights_enabled);
    assert!(!settings.code_lens);
    assert_eq!(settings.code_lens_font_family, "Cascadia Code");
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
        EditorGotoLocationMultiple::Peek
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
        EditorGotoLocationMultiple::Peek
    );
    assert_eq!(
        settings.goto_location_alternative_definition_command,
        "editor.action.peekDefinition"
    );
    assert_eq!(
        settings.goto_location_alternative_type_definition_command,
        "editor.action.peekTypeDefinition"
    );
    assert_eq!(
        settings.goto_location_alternative_declaration_command,
        "editor.action.peekDeclaration"
    );
    assert_eq!(
        settings.goto_location_alternative_implementation_command,
        "editor.action.peekImplementation"
    );
    assert_eq!(
        settings.goto_location_alternative_reference_command,
        "editor.action.referenceSearch.trigger"
    );
    assert_eq!(
        settings.goto_location_alternative_tests_command,
        "editor.action.goToReferences"
    );
    assert_eq!(
        settings.peek_widget_default_focus,
        EditorPeekWidgetDefaultFocus::Editor
    );
    assert_eq!(settings.placeholder, "Type here");
    assert!(settings.definition_link_opens_in_peek);
    assert!(!settings.inlay_hints);
    assert_eq!(settings.inlay_hints_font_family, "Cascadia Code");
    assert_eq!(settings.inlay_hints_font_size, 13);
    assert!(settings.inlay_hints_padding);
    assert_eq!(settings.inlay_hints_maximum_length, 25);
    assert!(!settings.parameter_hints_enabled);
    assert!(!settings.parameter_hints_on_trigger_characters);
    assert!(!settings.parameter_hints_cycle);
    assert!(settings.format_on_save);
    assert!(settings.format_on_paste);
    assert!(settings.trim_trailing_whitespace);
    assert!(settings.insert_final_newline);
    assert!(settings.trim_final_newlines);
}
