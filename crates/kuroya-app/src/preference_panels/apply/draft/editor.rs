use crate::editor_vim_key_events::sanitize_vim_settings_for_runtime;
use kuroya_core::{
    EditorSettings, GitInputValidationSubjectLength, LspServerConfig, clamp_diff_context_lines,
    clamp_diff_hide_unchanged_regions_minimum_line_count,
    clamp_diff_hide_unchanged_regions_reveal_line_count, clamp_diff_max_computation_time_ms,
    clamp_diff_max_file_size_mb, clamp_diff_render_side_by_side_inline_breakpoint,
    clamp_diff_split_view_default_ratio, clamp_editor_accessibility_page_size,
    clamp_editor_background_image_dim, clamp_editor_code_lens_font_size,
    clamp_editor_color_decorators_limit, clamp_editor_cursor_height,
    clamp_editor_cursor_surrounding_lines, clamp_editor_cursor_width,
    clamp_editor_folding_maximum_regions, clamp_editor_inlay_hints_font_size,
    clamp_editor_inlay_hints_maximum_length, clamp_editor_letter_spacing, clamp_editor_line_height,
    clamp_editor_line_numbers_min_chars, clamp_editor_minimap_max_column,
    clamp_editor_minimap_scale, clamp_editor_minimap_section_header_font_size,
    clamp_editor_minimap_section_header_letter_spacing, clamp_editor_multi_cursor_limit,
    clamp_editor_overview_ruler_lanes, clamp_editor_padding,
    clamp_editor_reveal_horizontal_right_padding, clamp_editor_ruler_column,
    clamp_editor_scroll_beyond_last_column, clamp_editor_scroll_sensitivity,
    clamp_editor_scrollbar_size, clamp_editor_sticky_scroll_max_line_count,
    clamp_editor_stop_rendering_line_after, clamp_editor_tab_index, clamp_editor_word_wrap_column,
    clamp_git_autofetch_period, clamp_git_commit_short_hash_length,
    clamp_git_detect_submodules_limit, clamp_git_detect_worktrees_limit,
    clamp_git_input_validation_length, clamp_git_repository_scan_max_depth,
    clamp_git_similarity_threshold, clamp_git_status_limit, clamp_hover_delay_ms,
    clamp_hover_hiding_delay_ms, clamp_inline_suggest_min_show_delay_ms,
    clamp_project_index_max_files, clamp_project_search_max_file_size_mb,
    clamp_project_search_max_results, clamp_quick_suggestions_delay_ms,
    clamp_scm_diff_decorations_gutter_width, clamp_scm_graph_page_size, clamp_scm_input_font_size,
    clamp_scm_input_line_count, clamp_scm_repositories_visible, clamp_suggest_font_size,
    clamp_suggest_line_height, normalize_editor_font_ligatures, normalize_editor_font_variations,
    sanitize_editor_font_weight,
};
use std::collections::BTreeMap;

const MIN_SETTINGS_PANEL_FONT_SIZE: f32 = 10.0;
const MAX_SETTINGS_PANEL_FONT_SIZE: f32 = 28.0;
const DEFAULT_SETTINGS_PANEL_FONT_SIZE: f32 = 13.0;
const MIN_SETTINGS_PANEL_UI_FONT_SIZE: f32 = 10.0;
const MAX_SETTINGS_PANEL_UI_FONT_SIZE: f32 = 24.0;
const DEFAULT_SETTINGS_PANEL_UI_FONT_SIZE: f32 = 13.0;
const MAX_SETTINGS_TEXT_CHARS: usize = 8_192;
const MAX_CUSTOM_THEME_PATHS: usize = 256;
const MAX_CUSTOM_THEME_PATH_CHARS: usize = 4096;

pub(super) fn apply_editor_settings_draft(settings: &mut EditorSettings, draft: &EditorSettings) {
    let current_theme = settings.theme.clone();
    let current_active_custom_theme_path = settings.active_custom_theme_path.clone();
    settings.font_size = clamp_finite_f32(
        draft.font_size,
        MIN_SETTINGS_PANEL_FONT_SIZE,
        MAX_SETTINGS_PANEL_FONT_SIZE,
        DEFAULT_SETTINGS_PANEL_FONT_SIZE,
    );
    settings.ui_font_size = clamp_finite_f32(
        draft.ui_font_size,
        MIN_SETTINGS_PANEL_UI_FONT_SIZE,
        MAX_SETTINGS_PANEL_UI_FONT_SIZE,
        DEFAULT_SETTINGS_PANEL_UI_FONT_SIZE,
    );
    settings.background_image_enabled = draft.background_image_enabled;
    settings.background_image_path = draft.background_image_path.clone();
    settings.background_image_scope = draft.background_image_scope;
    settings.background_image_dim = clamp_editor_background_image_dim(draft.background_image_dim);
    settings.background_image_fit = draft.background_image_fit;
    settings.background_image_position = draft.background_image_position;
    settings.background_image_loop = draft.background_image_loop;
    settings.font_family =
        raw_setting_text_or_default(&draft.font_family, kuroya_core::DEFAULT_EDITOR_FONT_FAMILY);
    settings.font_weight = sanitize_editor_font_weight(&draft.font_weight);
    settings.font_ligatures = normalize_editor_font_ligatures(&draft.font_ligatures);
    settings.font_variations = normalize_editor_font_variations(&draft.font_variations);
    settings.theme = draft.theme.clone();
    settings.active_custom_theme_path = draft.active_custom_theme_path.clone().or_else(|| {
        if draft.theme == current_theme {
            current_active_custom_theme_path
        } else {
            None
        }
    });
    settings.letter_spacing = clamp_editor_letter_spacing(draft.letter_spacing);
    settings.automatic_layout = draft.automatic_layout;
    settings.disable_layer_hinting = draft.disable_layer_hinting;
    settings.disable_monospace_optimizations = draft.disable_monospace_optimizations;
    settings.extra_editor_class_name = raw_non_empty_setting_text(&draft.extra_editor_class_name);
    settings.allow_variable_line_heights = draft.allow_variable_line_heights;
    settings.allow_variable_fonts = draft.allow_variable_fonts;
    settings.allow_variable_fonts_in_accessibility_mode =
        draft.allow_variable_fonts_in_accessibility_mode;
    settings.accessibility_support = draft.accessibility_support;
    settings.accessibility_page_size =
        clamp_editor_accessibility_page_size(draft.accessibility_page_size);
    settings.aria_label =
        raw_setting_text_or_default(&draft.aria_label, kuroya_core::DEFAULT_EDITOR_ARIA_LABEL);
    settings.aria_required = draft.aria_required;
    settings.screen_reader_announce_inline_suggestion =
        draft.screen_reader_announce_inline_suggestion;
    settings.tab_index = clamp_editor_tab_index(draft.tab_index);
    settings.read_only = draft.read_only;
    settings.read_only_message = raw_non_empty_setting_text(&draft.read_only_message);
    settings.dom_read_only = draft.dom_read_only;
    settings.edit_context = draft.edit_context;
    settings.render_rich_screen_reader_content = draft.render_rich_screen_reader_content;
    settings.trim_whitespace_on_delete = draft.trim_whitespace_on_delete;
    settings.unusual_line_terminators = draft.unusual_line_terminators;
    settings.use_shadow_dom = draft.use_shadow_dom;
    settings.use_tab_stops = draft.use_tab_stops;
    settings.fixed_overflow_widgets = draft.fixed_overflow_widgets;
    settings.allow_overflow = draft.allow_overflow;
    settings.tab_width = draft.tab_width.clamp(1, 12);
    settings.insert_spaces = draft.insert_spaces;
    settings.detect_indentation = draft.detect_indentation;
    settings.word_separators = normalized_setting_text(&draft.word_separators);
    settings.word_segmenter_locales = raw_non_empty_string_list(&draft.word_segmenter_locales);
    settings.auto_indent = draft.auto_indent;
    settings.auto_closing_brackets = draft.auto_closing_brackets;
    settings.auto_closing_quotes = draft.auto_closing_quotes;
    settings.experimental_gpu_acceleration = draft.experimental_gpu_acceleration;
    settings.experimental_whitespace_rendering = draft.experimental_whitespace_rendering;
    settings.auto_closing_comments = draft.auto_closing_comments;
    settings.auto_closing_delete = draft.auto_closing_delete;
    settings.auto_closing_overtype = draft.auto_closing_overtype;
    settings.auto_surround = draft.auto_surround;
    settings.auto_indent_on_paste = draft.auto_indent_on_paste;
    settings.auto_indent_on_paste_within_string = draft.auto_indent_on_paste_within_string;
    settings.sticky_tab_stops = draft.sticky_tab_stops;
    settings.linked_editing = draft.linked_editing;
    settings.rename_on_type = draft.rename_on_type;
    settings.tab_focus_mode = draft.tab_focus_mode;
    settings.vim_keybindings = draft.vim_keybindings;
    settings.vim = draft.vim.clone();
    settings.vim.sanitize();
    sanitize_vim_settings_for_runtime(&mut settings.vim);
    settings.quick_suggestions = draft.quick_suggestions;
    settings.quick_suggestions_delay_ms =
        clamp_quick_suggestions_delay_ms(draft.quick_suggestions_delay_ms);
    settings.suggest_on_trigger_characters = draft.suggest_on_trigger_characters;
    settings.accept_suggestion_on_enter = draft.accept_suggestion_on_enter;
    settings.accept_suggestion_on_tab = draft.accept_suggestion_on_tab;
    settings.accept_suggestion_on_commit_character = draft.accept_suggestion_on_commit_character;
    settings.suggest_selection = draft.suggest_selection;
    settings.suggest_insert_mode = draft.suggest_insert_mode;
    settings.suggest_filter_graceful = draft.suggest_filter_graceful;
    settings.suggest_snippets_prevent_quick_suggestions =
        draft.suggest_snippets_prevent_quick_suggestions;
    settings.suggest_locality_bonus = draft.suggest_locality_bonus;
    settings.suggest_share_suggest_selections = draft.suggest_share_suggest_selections;
    settings.suggest_selection_mode = draft.suggest_selection_mode;
    settings.suggest_show_icons = draft.suggest_show_icons;
    settings.suggest_show_status_bar = draft.suggest_show_status_bar;
    settings.suggest_preview = draft.suggest_preview;
    settings.suggest_preview_mode = draft.suggest_preview_mode;
    settings.suggest_show_inline_details = draft.suggest_show_inline_details;
    settings.suggest_show_methods = draft.suggest_show_methods;
    settings.suggest_show_functions = draft.suggest_show_functions;
    settings.suggest_show_constructors = draft.suggest_show_constructors;
    settings.suggest_show_deprecated = draft.suggest_show_deprecated;
    settings.suggest_show_fields = draft.suggest_show_fields;
    settings.suggest_show_variables = draft.suggest_show_variables;
    settings.suggest_show_classes = draft.suggest_show_classes;
    settings.suggest_show_structs = draft.suggest_show_structs;
    settings.suggest_show_interfaces = draft.suggest_show_interfaces;
    settings.suggest_show_modules = draft.suggest_show_modules;
    settings.suggest_show_properties = draft.suggest_show_properties;
    settings.suggest_show_events = draft.suggest_show_events;
    settings.suggest_show_operators = draft.suggest_show_operators;
    settings.suggest_show_units = draft.suggest_show_units;
    settings.suggest_show_values = draft.suggest_show_values;
    settings.suggest_show_constants = draft.suggest_show_constants;
    settings.suggest_show_enums = draft.suggest_show_enums;
    settings.suggest_show_enum_members = draft.suggest_show_enum_members;
    settings.suggest_show_keywords = draft.suggest_show_keywords;
    settings.suggest_show_words = draft.suggest_show_words;
    settings.suggest_show_colors = draft.suggest_show_colors;
    settings.suggest_show_files = draft.suggest_show_files;
    settings.suggest_show_references = draft.suggest_show_references;
    settings.suggest_show_customcolors = draft.suggest_show_customcolors;
    settings.suggest_show_folders = draft.suggest_show_folders;
    settings.suggest_show_type_parameters = draft.suggest_show_type_parameters;
    settings.suggest_show_snippets = draft.suggest_show_snippets;
    settings.suggest_show_users = draft.suggest_show_users;
    settings.suggest_show_issues = draft.suggest_show_issues;
    settings.suggest_match_on_word_start_only = draft.suggest_match_on_word_start_only;
    settings.suggest_font_size = clamp_suggest_font_size(draft.suggest_font_size);
    settings.suggest_line_height = clamp_suggest_line_height(draft.suggest_line_height);
    settings.tab_completion = draft.tab_completion;
    settings.snippet_suggestions = draft.snippet_suggestions;
    settings.hover_enabled = draft.hover_enabled;
    settings.hover_delay_ms = clamp_hover_delay_ms(draft.hover_delay_ms);
    settings.hover_hiding_delay_ms = clamp_hover_hiding_delay_ms(draft.hover_hiding_delay_ms);
    settings.hover_sticky = draft.hover_sticky;
    settings.hover_above = draft.hover_above;
    settings.hover_show_long_line_warning = draft.hover_show_long_line_warning;
    settings.inline_suggest_enabled = draft.inline_suggest_enabled;
    settings.inline_suggest_mode = draft.inline_suggest_mode;
    settings.inline_suggest_show_toolbar = draft.inline_suggest_show_toolbar;
    settings.inline_suggest_keep_on_blur = draft.inline_suggest_keep_on_blur;
    settings.inline_suggest_font_family =
        raw_non_empty_setting_text(&draft.inline_suggest_font_family);
    settings.inline_suggest_syntax_highlighting_enabled =
        draft.inline_suggest_syntax_highlighting_enabled;
    settings.inline_suggest_suppress_suggestions = draft.inline_suggest_suppress_suggestions;
    settings.inline_suggest_suppress_in_snippet_mode =
        draft.inline_suggest_suppress_in_snippet_mode;
    settings.inline_suggest_min_show_delay_ms =
        clamp_inline_suggest_min_show_delay_ms(draft.inline_suggest_min_show_delay_ms);
    settings.inline_suggest_edits_enabled = draft.inline_suggest_edits_enabled;
    settings.inline_suggest_edits_show_collapsed = draft.inline_suggest_edits_show_collapsed;
    settings.inline_suggest_edits_render_side_by_side =
        draft.inline_suggest_edits_render_side_by_side;
    settings.inline_suggest_edits_allow_code_shifting =
        draft.inline_suggest_edits_allow_code_shifting;
    settings.inline_suggest_edits_show_long_distance_hint =
        draft.inline_suggest_edits_show_long_distance_hint;
    settings.inline_suggest_trigger_command_on_provider_change =
        draft.inline_suggest_trigger_command_on_provider_change;
    settings.inline_suggest_experimental_suppress_inline_suggestions =
        normalized_trimmed_setting_text(
            &draft.inline_suggest_experimental_suppress_inline_suggestions,
        );
    settings.inline_suggest_experimental_show_on_suggest_conflict =
        draft.inline_suggest_experimental_show_on_suggest_conflict;
    settings.inline_suggest_experimental_empty_response_information =
        draft.inline_suggest_experimental_empty_response_information;
    settings.inline_completions_accessibility_verbose =
        draft.inline_completions_accessibility_verbose;
    settings.lightbulb = draft.lightbulb;
    settings.render_validation_decorations = draft.render_validation_decorations;
    settings.document_highlights_enabled = draft.document_highlights_enabled;
    settings.code_lens = draft.code_lens;
    settings.code_lens_font_family = raw_non_empty_setting_text(&draft.code_lens_font_family);
    settings.code_lens_font_size = clamp_editor_code_lens_font_size(draft.code_lens_font_size);
    settings.goto_location_multiple_definitions = draft.goto_location_multiple_definitions;
    settings.goto_location_multiple_type_definitions =
        draft.goto_location_multiple_type_definitions;
    settings.goto_location_multiple_declarations = draft.goto_location_multiple_declarations;
    settings.goto_location_multiple_implementations = draft.goto_location_multiple_implementations;
    settings.goto_location_multiple_references = draft.goto_location_multiple_references;
    settings.goto_location_multiple_tests = draft.goto_location_multiple_tests;
    settings.goto_location_alternative_definition_command =
        raw_non_empty_setting_text(&draft.goto_location_alternative_definition_command);
    settings.goto_location_alternative_type_definition_command =
        raw_non_empty_setting_text(&draft.goto_location_alternative_type_definition_command);
    settings.goto_location_alternative_declaration_command =
        raw_non_empty_setting_text(&draft.goto_location_alternative_declaration_command);
    settings.goto_location_alternative_implementation_command =
        raw_non_empty_setting_text(&draft.goto_location_alternative_implementation_command);
    settings.goto_location_alternative_reference_command =
        raw_non_empty_setting_text(&draft.goto_location_alternative_reference_command);
    settings.goto_location_alternative_tests_command =
        raw_non_empty_setting_text(&draft.goto_location_alternative_tests_command);
    settings.peek_widget_default_focus = draft.peek_widget_default_focus;
    settings.placeholder = raw_non_empty_setting_text(&draft.placeholder);
    settings.definition_link_opens_in_peek = draft.definition_link_opens_in_peek;
    settings.inlay_hints = draft.inlay_hints;
    settings.inlay_hints_font_family = raw_non_empty_setting_text(&draft.inlay_hints_font_family);
    settings.inlay_hints_font_size =
        clamp_editor_inlay_hints_font_size(draft.inlay_hints_font_size);
    settings.inlay_hints_padding = draft.inlay_hints_padding;
    settings.inlay_hints_maximum_length =
        clamp_editor_inlay_hints_maximum_length(draft.inlay_hints_maximum_length);
    settings.parameter_hints_enabled = draft.parameter_hints_enabled;
    settings.parameter_hints_on_trigger_characters = draft.parameter_hints_on_trigger_characters;
    settings.parameter_hints_cycle = draft.parameter_hints_cycle;
    settings.lsp_servers = normalized_lsp_server_configs(&draft.lsp_servers);
    settings.lsp_suggest_missing_servers = draft.lsp_suggest_missing_servers;
    settings.comments_insert_space = draft.comments_insert_space;
    settings.comments_ignore_empty_lines = draft.comments_ignore_empty_lines;
    settings.format_on_save = draft.format_on_save;
    settings.format_on_type = draft.format_on_type;
    settings.format_on_paste = draft.format_on_paste;
    settings.paste_as_enabled = draft.paste_as_enabled;
    settings.paste_as_show_paste_selector = draft.paste_as_show_paste_selector;
    settings.smooth_scrolling = draft.smooth_scrolling;
    settings.scroll_beyond_last_line = draft.scroll_beyond_last_line;
    settings.scroll_beyond_last_column =
        clamp_editor_scroll_beyond_last_column(draft.scroll_beyond_last_column);
    settings.scroll_on_middle_click = draft.scroll_on_middle_click;
    settings.scroll_predominant_axis = draft.scroll_predominant_axis;
    settings.inertial_scroll = draft.inertial_scroll;
    settings.mouse_wheel_scroll_sensitivity = clamp_editor_scroll_sensitivity(
        draft.mouse_wheel_scroll_sensitivity,
        kuroya_core::DEFAULT_EDITOR_MOUSE_WHEEL_SCROLL_SENSITIVITY,
    );
    settings.fast_scroll_sensitivity = clamp_editor_scroll_sensitivity(
        draft.fast_scroll_sensitivity,
        kuroya_core::DEFAULT_EDITOR_FAST_SCROLL_SENSITIVITY,
    );
    settings.mouse_wheel_zoom = draft.mouse_wheel_zoom;
    settings.scrollbar_vertical = draft.scrollbar_vertical;
    settings.scrollbar_horizontal = draft.scrollbar_horizontal;
    settings.scrollbar_vertical_scrollbar_size =
        clamp_editor_scrollbar_size(draft.scrollbar_vertical_scrollbar_size);
    settings.scrollbar_horizontal_scrollbar_size =
        clamp_editor_scrollbar_size(draft.scrollbar_horizontal_scrollbar_size);
    settings.scrollbar_ignore_horizontal_scrollbar_in_content_height =
        draft.scrollbar_ignore_horizontal_scrollbar_in_content_height;
    settings.explorer_scrollbar = draft.explorer_scrollbar;
    settings.padding_top = clamp_editor_padding(draft.padding_top);
    settings.padding_bottom = clamp_editor_padding(draft.padding_bottom);
    settings.links = draft.links;
    settings.show_unused = draft.show_unused;
    settings.show_deprecated = draft.show_deprecated;
    settings.contextmenu = draft.contextmenu;
    settings.color_decorators = draft.color_decorators;
    settings.color_decorators_activated_on = draft.color_decorators_activated_on;
    settings.color_decorators_limit =
        clamp_editor_color_decorators_limit(draft.color_decorators_limit);
    settings.default_color_decorators = draft.default_color_decorators;
    settings.sticky_scroll = draft.sticky_scroll;
    settings.sticky_scroll_max_line_count =
        clamp_editor_sticky_scroll_max_line_count(draft.sticky_scroll_max_line_count);
    settings.sticky_scroll_default_model = draft.sticky_scroll_default_model;
    settings.sticky_scroll_scroll_with_editor = draft.sticky_scroll_scroll_with_editor;
    settings.line_height = clamp_editor_line_height(draft.line_height);
    settings.minimap = draft.minimap;
    settings.minimap_side = draft.minimap_side;
    settings.minimap_autohide = draft.minimap_autohide;
    settings.minimap_size = draft.minimap_size;
    settings.minimap_show_slider = draft.minimap_show_slider;
    settings.minimap_scale = clamp_editor_minimap_scale(draft.minimap_scale);
    settings.minimap_render_characters = draft.minimap_render_characters;
    settings.minimap_max_column = clamp_editor_minimap_max_column(draft.minimap_max_column);
    settings.minimap_show_region_section_headers = draft.minimap_show_region_section_headers;
    settings.minimap_show_mark_section_headers = draft.minimap_show_mark_section_headers;
    settings.minimap_mark_section_header_regex = raw_setting_text_or_default(
        &draft.minimap_mark_section_header_regex,
        kuroya_core::DEFAULT_EDITOR_MINIMAP_MARK_SECTION_HEADER_REGEX,
    );
    settings.minimap_section_header_font_size =
        clamp_editor_minimap_section_header_font_size(draft.minimap_section_header_font_size);
    settings.minimap_section_header_letter_spacing =
        clamp_editor_minimap_section_header_letter_spacing(
            draft.minimap_section_header_letter_spacing,
        );
    settings.multi_cursor_modifier = draft.multi_cursor_modifier;
    settings.multi_cursor_merge_overlapping = draft.multi_cursor_merge_overlapping;
    settings.multi_cursor_paste = draft.multi_cursor_paste;
    settings.multi_cursor_limit = clamp_editor_multi_cursor_limit(draft.multi_cursor_limit);
    settings.column_selection = draft.column_selection;
    settings.mouse_middle_click_action = draft.mouse_middle_click_action;
    settings.empty_selection_clipboard = draft.empty_selection_clipboard;
    settings.selection_clipboard = draft.selection_clipboard;
    settings.copy_with_syntax_highlighting = draft.copy_with_syntax_highlighting;
    settings.double_click_selects_block = draft.double_click_selects_block;
    settings.drag_and_drop = draft.drag_and_drop;
    settings.drop_into_editor_enabled = draft.drop_into_editor_enabled;
    settings.drop_into_editor_show_drop_selector = draft.drop_into_editor_show_drop_selector;
    settings.glyph_margin = draft.glyph_margin;
    settings.ruler_column = clamp_editor_ruler_column(draft.ruler_column);
    settings.overview_ruler_border = draft.overview_ruler_border;
    settings.overview_ruler_lanes = clamp_editor_overview_ruler_lanes(draft.overview_ruler_lanes);
    settings.hide_cursor_in_overview_ruler = draft.hide_cursor_in_overview_ruler;
    settings.line_numbers = draft.line_numbers;
    settings.line_decorations_width = draft.line_decorations_width.clamped();
    settings.line_numbers_min_chars =
        clamp_editor_line_numbers_min_chars(draft.line_numbers_min_chars);
    settings.select_on_line_numbers = draft.select_on_line_numbers;
    settings.word_wrap = draft.word_wrap;
    settings.word_wrap_override1 = draft.word_wrap_override1;
    settings.word_wrap_override2 = draft.word_wrap_override2;
    settings.word_wrap_break_after_characters =
        normalized_break_character_text(&draft.word_wrap_break_after_characters);
    settings.word_wrap_break_before_characters =
        normalized_break_character_text(&draft.word_wrap_break_before_characters);
    settings.word_wrap_column = clamp_editor_word_wrap_column(draft.word_wrap_column);
    settings.wrapping_indent = draft.wrapping_indent;
    settings.wrapping_strategy = draft.wrapping_strategy;
    settings.wrap_on_escaped_line_feeds = draft.wrap_on_escaped_line_feeds;
    settings.word_break = draft.word_break;
    settings.reveal_horizontal_right_padding =
        clamp_editor_reveal_horizontal_right_padding(draft.reveal_horizontal_right_padding);
    settings.rounded_selection = draft.rounded_selection;
    settings.stop_rendering_line_after =
        clamp_editor_stop_rendering_line_after(draft.stop_rendering_line_after);
    settings.render_whitespace = draft.render_whitespace;
    settings.render_final_newline = draft.render_final_newline;
    settings.render_control_characters = draft.render_control_characters;
    settings.unicode_highlight_ambiguous_characters = draft.unicode_highlight_ambiguous_characters;
    settings.unicode_highlight_invisible_characters = draft.unicode_highlight_invisible_characters;
    settings.unicode_highlight_non_basic_ascii = draft.unicode_highlight_non_basic_ascii;
    settings.unicode_highlight_include_comments = draft.unicode_highlight_include_comments;
    settings.unicode_highlight_include_strings = draft.unicode_highlight_include_strings;
    settings.unicode_highlight_allowed_characters =
        raw_non_empty_bool_map(&draft.unicode_highlight_allowed_characters);
    settings.unicode_highlight_allowed_locales =
        raw_non_empty_bool_map(&draft.unicode_highlight_allowed_locales);
    settings.render_line_highlight = draft.render_line_highlight;
    settings.render_line_highlight_only_when_focus = draft.render_line_highlight_only_when_focus;
    settings.smart_select_select_leading_and_trailing_whitespace =
        draft.smart_select_select_leading_and_trailing_whitespace;
    settings.smart_select_select_subwords = draft.smart_select_select_subwords;
    settings.find_seed_search_string_from_selection = draft.find_seed_search_string_from_selection;
    settings.find_auto_find_in_selection = draft.find_auto_find_in_selection;
    settings.find_on_type = draft.find_on_type;
    settings.find_cursor_move_on_type = draft.find_cursor_move_on_type;
    settings.find_loop = draft.find_loop;
    settings.find_close_on_result = draft.find_close_on_result;
    settings.find_global_find_clipboard = draft.find_global_find_clipboard;
    settings.find_add_extra_space_on_top = draft.find_add_extra_space_on_top;
    settings.find_history = draft.find_history;
    settings.find_replace_history = draft.find_replace_history;
    settings.diff_ignore_trim_whitespace = draft.diff_ignore_trim_whitespace;
    settings.diff_algorithm = draft.diff_algorithm;
    settings.diff_render_side_by_side = draft.diff_render_side_by_side;
    settings.diff_enable_split_view_resizing = draft.diff_enable_split_view_resizing;
    settings.diff_split_view_default_ratio =
        clamp_diff_split_view_default_ratio(draft.diff_split_view_default_ratio);
    settings.diff_render_side_by_side_inline_breakpoint =
        clamp_diff_render_side_by_side_inline_breakpoint(
            draft.diff_render_side_by_side_inline_breakpoint,
        );
    settings.diff_use_inline_view_when_space_is_limited =
        draft.diff_use_inline_view_when_space_is_limited;
    settings.diff_compact_mode = draft.diff_compact_mode;
    settings.diff_original_editable = draft.diff_original_editable;
    settings.diff_code_lens = draft.diff_code_lens;
    settings.diff_accessibility_verbose = draft.diff_accessibility_verbose;
    settings.diff_hide_unchanged_regions = draft.diff_hide_unchanged_regions;
    settings.diff_context_lines = clamp_diff_context_lines(draft.diff_context_lines);
    settings.diff_hide_unchanged_regions_minimum_line_count =
        clamp_diff_hide_unchanged_regions_minimum_line_count(
            draft.diff_hide_unchanged_regions_minimum_line_count,
        );
    settings.diff_hide_unchanged_regions_reveal_line_count =
        clamp_diff_hide_unchanged_regions_reveal_line_count(
            draft.diff_hide_unchanged_regions_reveal_line_count,
        );
    settings.diff_max_computation_time_ms =
        clamp_diff_max_computation_time_ms(draft.diff_max_computation_time_ms);
    settings.diff_max_file_size_mb = clamp_diff_max_file_size_mb(draft.diff_max_file_size_mb);
    settings.diff_render_gutter_menu = draft.diff_render_gutter_menu;
    settings.diff_render_indicators = draft.diff_render_indicators;
    settings.diff_render_margin_revert_icon = draft.diff_render_margin_revert_icon;
    settings.diff_render_overview_ruler = draft.diff_render_overview_ruler;
    settings.diff_experimental_show_moves = draft.diff_experimental_show_moves;
    settings.diff_experimental_show_empty_decorations =
        draft.diff_experimental_show_empty_decorations;
    settings.diff_experimental_use_true_inline_view = draft.diff_experimental_use_true_inline_view;
    settings.diff_word_wrap = draft.diff_word_wrap;
    settings.diff_only_show_accessible_viewer = draft.diff_only_show_accessible_viewer;
    settings.diff_is_in_embedded_editor = draft.diff_is_in_embedded_editor;
    settings.project_index_max_files = clamp_project_index_max_files(draft.project_index_max_files);
    settings.project_index_exclude_globs =
        raw_non_empty_string_list(&draft.project_index_exclude_globs);
    settings.project_index_include_hidden_dirs = draft.project_index_include_hidden_dirs;
    settings.project_search_exclude_globs =
        raw_non_empty_string_list(&draft.project_search_exclude_globs);
    settings.project_search_max_file_size_mb =
        clamp_project_search_max_file_size_mb(draft.project_search_max_file_size_mb);
    settings.project_search_max_results =
        clamp_project_search_max_results(draft.project_search_max_results);
    settings.git_enabled = draft.git_enabled;
    settings.git_add_ai_co_author = draft.git_add_ai_co_author;
    settings.git_allow_force_push = draft.git_allow_force_push;
    settings.git_allow_no_verify_commit = draft.git_allow_no_verify_commit;
    settings.git_auto_repository_detection = draft.git_auto_repository_detection;
    settings.git_autofetch = draft.git_autofetch;
    settings.git_autofetch_period = clamp_git_autofetch_period(draft.git_autofetch_period);
    settings.git_autorefresh = draft.git_autorefresh;
    settings.git_auto_stash = draft.git_auto_stash;
    settings.git_commands_to_log = raw_non_empty_string_list(&draft.git_commands_to_log);
    settings.git_confirm_force_push = draft.git_confirm_force_push;
    settings.git_confirm_no_verify_commit = draft.git_confirm_no_verify_commit;
    settings.git_confirm_sync = draft.git_confirm_sync;
    settings.git_ignore_limit_warning = draft.git_ignore_limit_warning;
    settings.git_ignore_submodules = draft.git_ignore_submodules;
    settings.git_ignored_repositories = raw_non_empty_string_list(&draft.git_ignored_repositories);
    settings.git_repository_scan_ignored_folders =
        raw_non_empty_string_list(&draft.git_repository_scan_ignored_folders);
    settings.git_open_repository_in_parent_folders = draft.git_open_repository_in_parent_folders;
    settings.git_detect_submodules = draft.git_detect_submodules;
    settings.git_detect_submodules_limit =
        clamp_git_detect_submodules_limit(draft.git_detect_submodules_limit);
    settings.git_repository_scan_max_depth =
        clamp_git_repository_scan_max_depth(draft.git_repository_scan_max_depth);
    settings.git_detect_worktrees = draft.git_detect_worktrees;
    settings.git_detect_worktrees_limit =
        clamp_git_detect_worktrees_limit(draft.git_detect_worktrees_limit);
    settings.git_discard_untracked_changes_to_trash = draft.git_discard_untracked_changes_to_trash;
    settings.git_diagnostics_commit_hook_enabled = draft.git_diagnostics_commit_hook_enabled;
    settings.git_diagnostics_commit_hook_sources =
        raw_non_empty_string_map(&draft.git_diagnostics_commit_hook_sources);
    settings.git_enable_commit_signing = draft.git_enable_commit_signing;
    settings.git_enable_status_bar_sync = draft.git_enable_status_bar_sync;
    settings.git_fetch_on_pull = draft.git_fetch_on_pull;
    settings.git_follow_tags_when_sync = draft.git_follow_tags_when_sync;
    settings.git_ignore_legacy_warning = draft.git_ignore_legacy_warning;
    settings.git_ignore_missing_git_warning = draft.git_ignore_missing_git_warning;
    settings.git_ignore_rebase_warning = draft.git_ignore_rebase_warning;
    settings.git_ignore_windows_git27_warning = draft.git_ignore_windows_git27_warning;
    settings.git_merge_editor = draft.git_merge_editor;
    settings.git_open_after_clone = draft.git_open_after_clone;
    settings.git_optimistic_update = draft.git_optimistic_update;
    settings.git_path = raw_non_empty_string_list(&draft.git_path);
    settings.git_post_commit_command = draft.git_post_commit_command;
    settings.git_prune_on_fetch = draft.git_prune_on_fetch;
    settings.git_pull_before_checkout = draft.git_pull_before_checkout;
    settings.git_pull_tags = draft.git_pull_tags;
    settings.git_rebase_when_sync = draft.git_rebase_when_sync;
    settings.git_remember_post_commit_command = draft.git_remember_post_commit_command;
    settings.git_replace_tags_when_pull = draft.git_replace_tags_when_pull;
    settings.git_support_cancellation = draft.git_support_cancellation;
    settings.git_terminal_authentication = draft.git_terminal_authentication;
    settings.git_terminal_git_editor = draft.git_terminal_git_editor;
    settings.git_use_force_push_if_includes = draft.git_use_force_push_if_includes;
    settings.git_use_force_push_with_lease = draft.git_use_force_push_with_lease;
    settings.git_use_integrated_ask_pass = draft.git_use_integrated_ask_pass;
    settings.git_worktree_include_files =
        raw_non_empty_string_list(&draft.git_worktree_include_files);
    settings.git_default_branch_name = raw_setting_text_or_default(
        &draft.git_default_branch_name,
        kuroya_core::DEFAULT_GIT_DEFAULT_BRANCH_NAME,
    );
    settings.git_default_clone_directory = draft
        .git_default_clone_directory
        .as_deref()
        .and_then(raw_optional_setting_text);
    settings.git_similarity_threshold =
        clamp_git_similarity_threshold(draft.git_similarity_threshold);
    settings.scm_default_view_mode = draft.scm_default_view_mode;
    settings.scm_default_view_sort_key = draft.scm_default_view_sort_key;
    settings.scm_auto_reveal = draft.scm_auto_reveal;
    settings.scm_count_badge = draft.scm_count_badge;
    settings.scm_provider_count_badge = draft.scm_provider_count_badge;
    settings.scm_always_show_repositories = draft.scm_always_show_repositories;
    settings.scm_repositories_visible =
        clamp_scm_repositories_visible(draft.scm_repositories_visible);
    settings.scm_compact_folders = draft.scm_compact_folders;
    settings.scm_always_show_actions = draft.scm_always_show_actions;
    settings.scm_show_action_button = draft.scm_show_action_button;
    settings.git_show_commit_input = draft.git_show_commit_input;
    settings.git_show_push_success_notification = draft.git_show_push_success_notification;
    settings.git_use_editor_as_commit_input = draft.git_use_editor_as_commit_input;
    settings.git_verbose_commit = draft.git_verbose_commit;
    settings.git_show_action_button_commit = draft.git_show_action_button_commit;
    settings.git_always_sign_off = draft.git_always_sign_off;
    settings.git_confirm_committed_delete = draft.git_confirm_committed_delete;
    settings.git_confirm_empty_commits = draft.git_confirm_empty_commits;
    settings.git_require_user_config = draft.git_require_user_config;
    settings.git_show_progress = draft.git_show_progress;
    settings.git_show_reference_details = draft.git_show_reference_details;
    settings.git_timeline_show_author = draft.git_timeline_show_author;
    settings.git_timeline_show_uncommitted = draft.git_timeline_show_uncommitted;
    settings.git_timeline_date = draft.git_timeline_date;
    settings.git_show_inline_open_file_action = draft.git_show_inline_open_file_action;
    settings.git_count_badge = draft.git_count_badge;
    settings.git_untracked_changes = draft.git_untracked_changes;
    settings.git_open_diff_on_click = draft.git_open_diff_on_click;
    settings.git_close_diff_on_operation = draft.git_close_diff_on_operation;
    settings.git_always_show_staged_changes_resource_group =
        draft.git_always_show_staged_changes_resource_group;
    settings.git_checkout_type = normalized_git_checkout_types(&draft.git_checkout_type);
    settings.git_branch_sort_order = draft.git_branch_sort_order;
    settings.git_branch_prefix = raw_non_empty_setting_text(&draft.git_branch_prefix);
    settings.git_branch_random_name_enable = draft.git_branch_random_name_enable;
    settings.git_branch_random_name_dictionary =
        raw_non_empty_string_list(&draft.git_branch_random_name_dictionary);
    settings.git_branch_validation_regex =
        raw_non_empty_setting_text(&draft.git_branch_validation_regex);
    settings.git_branch_whitespace_char =
        raw_non_empty_setting_text(&draft.git_branch_whitespace_char);
    settings.git_decorations_enabled = draft.git_decorations_enabled;
    settings.git_enable_smart_commit = draft.git_enable_smart_commit;
    settings.git_suggest_smart_commit = draft.git_suggest_smart_commit;
    settings.git_smart_commit_changes = draft.git_smart_commit_changes;
    settings.git_prompt_to_save_files_before_commit = draft.git_prompt_to_save_files_before_commit;
    settings.git_prompt_to_save_files_before_stash = draft.git_prompt_to_save_files_before_stash;
    settings.git_branch_protection = raw_non_empty_string_list(&draft.git_branch_protection);
    settings.git_branch_protection_prompt = draft.git_branch_protection_prompt;
    settings.git_status_limit = clamp_git_status_limit(draft.git_status_limit);
    settings.git_use_commit_input_as_stash_message = draft.git_use_commit_input_as_stash_message;
    settings.git_commit_short_hash_length =
        clamp_git_commit_short_hash_length(draft.git_commit_short_hash_length);
    settings.git_input_validation = draft.git_input_validation;
    settings.git_input_validation_length =
        clamp_git_input_validation_length(draft.git_input_validation_length);
    settings.git_input_validation_subject_length =
        clamp_git_input_validation_subject_length(draft.git_input_validation_subject_length);
    settings.git_blame_status_bar_item_enabled = draft.git_blame_status_bar_item_enabled;
    settings.git_blame_editor_decoration_enabled = draft.git_blame_editor_decoration_enabled;
    settings.git_blame_editor_decoration_disable_hover =
        draft.git_blame_editor_decoration_disable_hover;
    settings.git_blame_ignore_whitespace = draft.git_blame_ignore_whitespace;
    settings.git_blame_status_bar_item_template =
        raw_non_empty_setting_text(&draft.git_blame_status_bar_item_template);
    settings.git_blame_editor_decoration_template =
        raw_non_empty_setting_text(&draft.git_blame_editor_decoration_template);
    settings.scm_show_input_action_button = draft.scm_show_input_action_button;
    settings.scm_input_min_line_count = clamp_scm_input_line_count(draft.scm_input_min_line_count);
    settings.scm_input_max_line_count = clamp_scm_input_line_count(draft.scm_input_max_line_count)
        .max(settings.scm_input_min_line_count);
    settings.scm_input_font_family = raw_setting_text_or_default(
        &draft.scm_input_font_family,
        kuroya_core::DEFAULT_SCM_INPUT_FONT_FAMILY,
    );
    settings.scm_input_font_size = clamp_scm_input_font_size(draft.scm_input_font_size);
    settings.scm_diff_decorations = draft.scm_diff_decorations;
    settings.scm_diff_decorations_gutter_action = draft.scm_diff_decorations_gutter_action;
    settings.scm_diff_decorations_gutter_visibility = draft.scm_diff_decorations_gutter_visibility;
    settings.scm_diff_decorations_gutter_width =
        clamp_scm_diff_decorations_gutter_width(draft.scm_diff_decorations_gutter_width);
    settings.scm_diff_decorations_gutter_pattern = draft.scm_diff_decorations_gutter_pattern;
    settings.scm_diff_decorations_ignore_trim_whitespace =
        draft.scm_diff_decorations_ignore_trim_whitespace;
    settings.scm_graph_page_on_scroll = draft.scm_graph_page_on_scroll;
    settings.scm_graph_page_size = clamp_scm_graph_page_size(draft.scm_graph_page_size);
    settings.scm_graph_badges = draft.scm_graph_badges;
    settings.scm_graph_show_incoming_changes = draft.scm_graph_show_incoming_changes;
    settings.scm_graph_show_outgoing_changes = draft.scm_graph_show_outgoing_changes;
    settings.bracket_pair_colorization = draft.bracket_pair_colorization;
    settings.bracket_pair_colorization_independent_color_pool_per_bracket_type =
        draft.bracket_pair_colorization_independent_color_pool_per_bracket_type;
    settings.bracket_pair_guides = draft.bracket_pair_guides;
    settings.bracket_pair_guides_horizontal = draft.bracket_pair_guides_horizontal;
    settings.highlight_active_bracket_pair = draft.highlight_active_bracket_pair;
    settings.match_brackets = draft.match_brackets;
    settings.folding = draft.folding;
    settings.folding_highlight = draft.folding_highlight;
    settings.folding_imports_by_default = draft.folding_imports_by_default;
    settings.folding_maximum_regions =
        clamp_editor_folding_maximum_regions(draft.folding_maximum_regions);
    settings.folding_strategy = draft.folding_strategy;
    settings.unfold_on_click_after_end_of_line = draft.unfold_on_click_after_end_of_line;
    settings.show_folding_controls = draft.show_folding_controls;
    settings.indent_guides = draft.indent_guides;
    settings.highlight_active_indentation = draft.highlight_active_indentation;
    settings.mouse_style = draft.mouse_style;
    settings.cursor_smooth_caret_animation = draft.cursor_smooth_caret_animation;
    settings.cursor_style = draft.cursor_style;
    settings.overtype_cursor_style = draft.overtype_cursor_style;
    settings.overtype_on_paste = draft.overtype_on_paste;
    settings.cursor_blinking = draft.cursor_blinking;
    settings.cursor_width = clamp_editor_cursor_width(draft.cursor_width);
    settings.cursor_height = clamp_editor_cursor_height(draft.cursor_height);
    settings.cursor_surrounding_lines =
        clamp_editor_cursor_surrounding_lines(draft.cursor_surrounding_lines);
    settings.cursor_surrounding_lines_style = draft.cursor_surrounding_lines_style;
    settings.custom_theme_paths = normalized_custom_theme_paths(&draft.custom_theme_paths);
    if let Some(active_path) = settings.active_custom_theme_path.as_deref()
        && !settings
            .custom_theme_paths
            .iter()
            .any(|path| path == active_path)
    {
        settings.active_custom_theme_path = None;
    }
}

fn normalized_git_checkout_types(
    checkout_types: &[kuroya_core::GitCheckoutType],
) -> Vec<kuroya_core::GitCheckoutType> {
    [
        kuroya_core::GitCheckoutType::Local,
        kuroya_core::GitCheckoutType::Remote,
        kuroya_core::GitCheckoutType::Tags,
    ]
    .into_iter()
    .filter(|kind| checkout_types.contains(kind))
    .collect()
}

fn normalized_lsp_server_configs(servers: &[LspServerConfig]) -> Vec<LspServerConfig> {
    let mut normalized = Vec::new();
    for server in servers {
        let mut language = normalized_trimmed_setting_text(&server.language);
        language.make_ascii_lowercase();
        let command = normalized_trimmed_setting_text(&server.command);
        if language.is_empty() || command.is_empty() {
            continue;
        }

        let config = LspServerConfig {
            language,
            command,
            args: normalized_trimmed_non_empty_string_list(&server.args, false),
            extensions: normalized_lsp_extensions(&server.extensions),
            root_markers: normalized_trimmed_non_empty_string_list(&server.root_markers, true),
            enabled: server.enabled,
        };

        if !normalized.contains(&config) {
            normalized.push(config);
        }
    }
    normalized
}

fn normalized_lsp_extensions(values: &[String]) -> Vec<String> {
    let normalized = normalized_trimmed_non_empty_string_list(values, true);
    let mut output = Vec::with_capacity(normalized.len());
    for extension in normalized {
        let extension = extension.trim_start_matches('.').trim().to_owned();
        if extension.is_empty() || output.contains(&extension) {
            continue;
        }
        output.push(extension);
    }
    output
}

fn normalized_custom_theme_paths(values: &[String]) -> Vec<String> {
    let mut normalized = Vec::with_capacity(values.len().min(MAX_CUSTOM_THEME_PATHS));
    for value in values.iter().take(MAX_CUSTOM_THEME_PATHS) {
        let item = normalized_setting_text(value);
        let item = item
            .chars()
            .take(MAX_CUSTOM_THEME_PATH_CHARS)
            .collect::<String>()
            .trim()
            .to_owned();
        if item.is_empty() || normalized.iter().any(|existing| existing == &item) {
            continue;
        }
        normalized.push(item);
    }
    normalized
}

fn raw_non_empty_string_list(values: &[String]) -> Vec<String> {
    values
        .iter()
        .filter_map(|value| normalized_non_empty_setting_text(value))
        .collect()
}

fn normalized_trimmed_non_empty_string_list(values: &[String], dedupe: bool) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values {
        let item = normalized_trimmed_setting_text(value);
        if item.is_empty() {
            continue;
        }
        if dedupe && normalized.iter().any(|existing| existing == &item) {
            continue;
        }
        normalized.push(item);
    }
    normalized
}

fn raw_non_empty_string_map(values: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    values
        .iter()
        .filter_map(|(key, value)| {
            let key = normalized_non_empty_setting_text(key)?;
            let value = normalized_non_empty_setting_text(value)?;
            Some((key, value))
        })
        .collect()
}

fn raw_non_empty_bool_map(values: &BTreeMap<String, bool>) -> BTreeMap<String, bool> {
    values
        .iter()
        .filter_map(|(key, value)| {
            let key = normalized_non_empty_setting_text(key)?;
            Some((key, *value))
        })
        .collect()
}

fn raw_setting_text_or_default(value: &str, fallback: &str) -> String {
    normalized_non_empty_setting_text(value).unwrap_or_else(|| fallback.to_owned())
}

fn raw_non_empty_setting_text(value: &str) -> String {
    normalized_non_empty_setting_text(value).unwrap_or_default()
}

fn raw_optional_setting_text(value: &str) -> Option<String> {
    normalized_non_empty_setting_text(value)
}

fn normalized_non_empty_setting_text(value: &str) -> Option<String> {
    let normalized = normalized_setting_text(value);
    (!normalized.trim().is_empty()).then_some(normalized)
}

fn normalized_trimmed_setting_text(value: &str) -> String {
    normalized_setting_text(value).trim().to_owned()
}

fn normalized_break_character_text(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len().min(MAX_SETTINGS_TEXT_CHARS));
    for ch in value.chars().take(MAX_SETTINGS_TEXT_CHARS) {
        if is_hidden_setting_format_control(ch) {
            continue;
        }

        normalized.push(
            if matches!(ch, '\r' | '\n' | '\u{2028}' | '\u{2029}')
                || (ch.is_control() && ch != '\t')
            {
                ' '
            } else {
                ch
            },
        );
    }
    normalized
}

fn normalized_setting_text(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len().min(MAX_SETTINGS_TEXT_CHARS));
    for ch in value.chars().take(MAX_SETTINGS_TEXT_CHARS) {
        if is_hidden_setting_format_control(ch) {
            continue;
        }

        normalized.push(
            if ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}') {
                ' '
            } else {
                ch
            },
        );
    }
    normalized
}

fn is_hidden_setting_format_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{061C}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FEFF}'
    )
}

fn clamp_git_input_validation_subject_length(
    value: GitInputValidationSubjectLength,
) -> GitInputValidationSubjectLength {
    match value {
        GitInputValidationSubjectLength::Inherit => GitInputValidationSubjectLength::Inherit,
        GitInputValidationSubjectLength::Chars(length) => {
            GitInputValidationSubjectLength::Chars(clamp_git_input_validation_length(length))
        }
    }
}

fn clamp_finite_f32(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_SETTINGS_TEXT_CHARS, apply_editor_settings_draft};
    use kuroya_core::EditorSettings;
    use std::collections::BTreeMap;

    #[test]
    fn editor_draft_apply_sanitizes_display_text_maps_and_lists() {
        let mut settings = EditorSettings::default();
        let draft = EditorSettings {
            word_separators: format!(".\u{202e}\n{}", "x".repeat(MAX_SETTINGS_TEXT_CHARS + 8)),
            inline_suggest_experimental_suppress_inline_suggestions: " ext.one\u{202e}\n,ext.two "
                .to_owned(),
            word_wrap_break_after_characters: " ,;\u{2066}\n".to_owned(),
            word_wrap_break_before_characters: "([{\u{200b}\r".to_owned(),
            unicode_highlight_allowed_characters: BTreeMap::from([
                ("\u{202e}".to_owned(), true),
                ("\u{0391}\u{202e}".to_owned(), false),
            ]),
            unicode_highlight_allowed_locales: BTreeMap::from([
                ("\u{200b}\t".to_owned(), true),
                ("ja\u{2066}".to_owned(), false),
            ]),
            git_diagnostics_commit_hook_sources: BTreeMap::from([
                ("\u{202e}".to_owned(), "error".to_owned()),
                ("rust\u{202e}\n".to_owned(), "warning\tbad".to_owned()),
            ]),
            git_branch_protection: vec![
                " main\u{202e}\n".to_owned(),
                "\u{202e}\t".to_owned(),
                "release/*".to_owned(),
            ],
            ..EditorSettings::default()
        };

        apply_editor_settings_draft(&mut settings, &draft);

        assert!(settings.word_separators.chars().count() <= MAX_SETTINGS_TEXT_CHARS);
        assert_eq!(
            settings.inline_suggest_experimental_suppress_inline_suggestions,
            "ext.one ,ext.two"
        );
        assert_eq!(settings.word_wrap_break_after_characters, " ,; ");
        assert_eq!(settings.word_wrap_break_before_characters, "([{ ");
        assert_eq!(
            settings.unicode_highlight_allowed_characters,
            BTreeMap::from([("\u{0391}".to_owned(), false)])
        );
        assert_eq!(
            settings.unicode_highlight_allowed_locales,
            BTreeMap::from([("ja".to_owned(), false)])
        );
        assert_eq!(
            settings.git_diagnostics_commit_hook_sources,
            BTreeMap::from([("rust ".to_owned(), "warning bad".to_owned())])
        );
        assert_eq!(
            settings.git_branch_protection,
            [" main ".to_owned(), "release/*".to_owned()]
        );

        for value in [
            settings.word_separators.as_str(),
            settings
                .inline_suggest_experimental_suppress_inline_suggestions
                .as_str(),
            settings.word_wrap_break_after_characters.as_str(),
            settings.word_wrap_break_before_characters.as_str(),
            settings.git_branch_protection[0].as_str(),
        ] {
            assert!(!value.chars().any(is_unsafe_display_char), "{value:?}");
        }
    }

    #[test]
    fn editor_draft_apply_preserves_default_break_character_settings() {
        let current = EditorSettings::default();
        let mut settings = current.clone();

        apply_editor_settings_draft(&mut settings, &current);

        assert_eq!(
            settings.word_wrap_break_after_characters,
            current.word_wrap_break_after_characters
        );
        assert_eq!(
            settings.word_wrap_break_before_characters,
            current.word_wrap_break_before_characters
        );
    }

    fn is_unsafe_display_char(ch: char) -> bool {
        ch.is_control()
            || matches!(
                ch,
                '\u{061c}'
                    | '\u{200b}'..='\u{200f}'
                    | '\u{202a}'..='\u{202e}'
                    | '\u{2066}'..='\u{2069}'
                    | '\u{feff}'
            )
    }
}
