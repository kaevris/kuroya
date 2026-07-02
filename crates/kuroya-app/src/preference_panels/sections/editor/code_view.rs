use crate::preference_panels::sections::{
    SETTINGS_TARGET_EDITOR_CODE_VIEW, SETTINGS_TARGET_EDITOR_DIFF,
    SETTINGS_TARGET_EDITOR_SOURCE_CONTROL, SettingsHighlightState,
    bounded_settings_text_edit_width, bounded_singleline_text_edit,
    bounded_singleline_text_edit_with_hint, guarded_f32_drag_value, settings_target_heading,
};
use eframe::egui;
use kuroya_core::{
    DEFAULT_DIFF_SPLIT_VIEW_DEFAULT_RATIO, DEFAULT_EDITOR_MINIMAP_SECTION_HEADER_FONT_SIZE,
    DEFAULT_EDITOR_MINIMAP_SECTION_HEADER_LETTER_SPACING, DEFAULT_SCM_INPUT_FONT_SIZE,
    EditorHighlightActiveIndentation, EditorSettings, MAX_DIFF_CONTEXT_LINES,
    MAX_DIFF_HIDE_UNCHANGED_REGIONS_MINIMUM_LINE_COUNT,
    MAX_DIFF_HIDE_UNCHANGED_REGIONS_REVEAL_LINE_COUNT, MAX_DIFF_MAX_COMPUTATION_TIME_MS,
    MAX_DIFF_MAX_FILE_SIZE_MB, MAX_DIFF_RENDER_SIDE_BY_SIDE_INLINE_BREAKPOINT,
    MAX_DIFF_SPLIT_VIEW_DEFAULT_RATIO, MAX_EDITOR_FOLDING_MAXIMUM_REGIONS,
    MAX_EDITOR_MINIMAP_MAX_COLUMN, MAX_EDITOR_MINIMAP_SCALE,
    MAX_EDITOR_MINIMAP_SECTION_HEADER_FONT_SIZE, MAX_EDITOR_MINIMAP_SECTION_HEADER_LETTER_SPACING,
    MAX_EDITOR_RULER_COLUMN, MAX_EDITOR_STICKY_SCROLL_MAX_LINE_COUNT, MAX_GIT_AUTOFETCH_PERIOD,
    MAX_GIT_COMMIT_SHORT_HASH_LENGTH, MAX_GIT_DETECT_SUBMODULES_LIMIT,
    MAX_GIT_DETECT_WORKTREES_LIMIT, MAX_GIT_INPUT_VALIDATION_LENGTH,
    MAX_GIT_REPOSITORY_SCAN_MAX_DEPTH, MAX_GIT_SIMILARITY_THRESHOLD, MAX_GIT_STATUS_LIMIT,
    MAX_SCM_DIFF_DECORATIONS_GUTTER_WIDTH, MAX_SCM_GRAPH_PAGE_SIZE, MAX_SCM_INPUT_FONT_SIZE,
    MAX_SCM_INPUT_LINE_COUNT, MAX_SCM_REPOSITORIES_VISIBLE, MIN_DIFF_CONTEXT_LINES,
    MIN_DIFF_HIDE_UNCHANGED_REGIONS_MINIMUM_LINE_COUNT,
    MIN_DIFF_HIDE_UNCHANGED_REGIONS_REVEAL_LINE_COUNT, MIN_DIFF_MAX_COMPUTATION_TIME_MS,
    MIN_DIFF_MAX_FILE_SIZE_MB, MIN_DIFF_RENDER_SIDE_BY_SIDE_INLINE_BREAKPOINT,
    MIN_DIFF_SPLIT_VIEW_DEFAULT_RATIO, MIN_EDITOR_FOLDING_MAXIMUM_REGIONS,
    MIN_EDITOR_MINIMAP_MAX_COLUMN, MIN_EDITOR_MINIMAP_SCALE,
    MIN_EDITOR_MINIMAP_SECTION_HEADER_FONT_SIZE, MIN_EDITOR_MINIMAP_SECTION_HEADER_LETTER_SPACING,
    MIN_EDITOR_STICKY_SCROLL_MAX_LINE_COUNT, MIN_GIT_AUTOFETCH_PERIOD,
    MIN_GIT_COMMIT_SHORT_HASH_LENGTH, MIN_GIT_DETECT_SUBMODULES_LIMIT,
    MIN_GIT_DETECT_WORKTREES_LIMIT, MIN_GIT_INPUT_VALIDATION_LENGTH,
    MIN_GIT_REPOSITORY_SCAN_MAX_DEPTH, MIN_GIT_SIMILARITY_THRESHOLD, MIN_GIT_STATUS_LIMIT,
    MIN_SCM_DIFF_DECORATIONS_GUTTER_WIDTH, MIN_SCM_GRAPH_PAGE_SIZE, MIN_SCM_INPUT_FONT_SIZE,
    MIN_SCM_INPUT_LINE_COUNT, MIN_SCM_REPOSITORIES_VISIBLE,
};

mod editor_combos;
mod git_scm_combos;

use editor_combos::{
    diff_algorithm_combo, diff_word_wrap_combo, editor_bracket_pair_guide_mode_combo,
    editor_find_auto_find_in_selection_combo, editor_find_history_combo,
    editor_find_seed_search_string_from_selection_combo, editor_folding_strategy_combo,
    editor_highlight_active_indentation_combo, editor_match_brackets_combo,
    editor_minimap_autohide_combo, editor_minimap_show_slider_combo, editor_minimap_side_combo,
    editor_minimap_size_combo, editor_show_folding_controls_combo,
    editor_sticky_scroll_default_model_combo,
};
#[cfg(test)]
use git_scm_combos::parse_string_list_input_value;
use git_scm_combos::{
    git_add_ai_co_author_combo, git_auto_repository_detection_combo, git_autofetch_combo,
    git_branch_protection_prompt_combo, git_branch_sort_order_combo, git_count_badge_combo,
    git_open_after_clone_combo, git_open_repository_in_parent_folders_combo,
    git_post_commit_command_combo, git_prompt_to_save_files_combo, git_smart_commit_changes_combo,
    git_timeline_date_combo, git_untracked_changes_combo, render_git_branch_protection_input,
    render_git_checkout_type, render_git_ignored_repositories_input,
    render_git_input_validation_subject_length, render_git_repository_scan_ignored_folders_input,
    render_optional_string_input, render_string_list_input, render_string_map_input,
    scm_count_badge_combo, scm_default_view_mode_combo, scm_default_view_sort_key_combo,
    scm_diff_decorations_combo, scm_diff_decorations_gutter_action_combo,
    scm_diff_decorations_gutter_visibility_combo,
    scm_diff_decorations_ignore_trim_whitespace_combo, scm_graph_badges_combo,
    scm_provider_count_badge_combo,
};

pub(super) fn render_code_view_settings_with_highlight(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    ui.add_space(12.0);
    settings_target_heading(ui, highlight, SETTINGS_TARGET_EDITOR_CODE_VIEW, "Code View");
    egui::Grid::new("settings_editor_code_view_grid")
        .num_columns(2)
        .spacing([18.0, 10.0])
        .show(ui, |ui| {
            ui.label("Folding");
            ui.checkbox(&mut draft.folding, "Show folding controls");
            ui.end_row();

            ui.label("Folding controls");
            editor_show_folding_controls_combo(
                ui,
                "editor_show_folding_controls",
                &mut draft.show_folding_controls,
            );
            ui.end_row();

            ui.label("Folding highlight");
            ui.checkbox(&mut draft.folding_highlight, "Highlight folded regions");
            ui.end_row();

            ui.label("Fold imports");
            ui.checkbox(
                &mut draft.folding_imports_by_default,
                "Fold import regions when folding data loads",
            );
            ui.end_row();

            ui.label("Folding strategy");
            editor_folding_strategy_combo(
                ui,
                "editor_folding_strategy",
                &mut draft.folding_strategy,
            );
            ui.end_row();

            ui.label("Maximum fold regions");
            ui.add(
                egui::DragValue::new(&mut draft.folding_maximum_regions)
                    .speed(100.0)
                    .range(MIN_EDITOR_FOLDING_MAXIMUM_REGIONS..=MAX_EDITOR_FOLDING_MAXIMUM_REGIONS),
            )
            .on_hover_text("Maximum number of foldable regions tracked by the editor");
            ui.end_row();

            ui.label("Unfold after line end");
            ui.checkbox(
                &mut draft.unfold_on_click_after_end_of_line,
                "Unfold by clicking after a folded line",
            );
            ui.end_row();

            ui.label("Sticky scroll");
            ui.checkbox(&mut draft.sticky_scroll, "Pin containing scope");
            ui.end_row();

            ui.label("Sticky scroll lines");
            ui.add(
                egui::DragValue::new(&mut draft.sticky_scroll_max_line_count)
                    .speed(1.0)
                    .range(
                        MIN_EDITOR_STICKY_SCROLL_MAX_LINE_COUNT
                            ..=MAX_EDITOR_STICKY_SCROLL_MAX_LINE_COUNT,
                    ),
            );
            ui.end_row();

            ui.label("Sticky scroll model");
            editor_sticky_scroll_default_model_combo(
                ui,
                "editor_sticky_scroll_default_model",
                &mut draft.sticky_scroll_default_model,
            );
            ui.end_row();

            ui.label("Sticky scroll with editor");
            ui.checkbox(
                &mut draft.sticky_scroll_scroll_with_editor,
                "Scroll sticky headers horizontally",
            );
            ui.end_row();

            ui.label("Glyph margin");
            ui.checkbox(&mut draft.glyph_margin, "Show gutter markers");
            ui.end_row();

            ui.label("Indent guides");
            ui.checkbox(&mut draft.indent_guides, "Show indentation guides");
            ui.end_row();

            ui.label("Active indent guide");
            let mut active_indentation = if draft.indent_guides {
                draft.highlight_active_indentation
            } else {
                EditorHighlightActiveIndentation::Off
            };
            ui.add_enabled_ui(draft.indent_guides, |ui| {
                editor_highlight_active_indentation_combo(
                    ui,
                    "editor_highlight_active_indentation",
                    &mut active_indentation,
                );
            });
            if draft.indent_guides {
                draft.highlight_active_indentation = active_indentation;
            }
            ui.end_row();

            ui.label("Bracket colorization");
            ui.vertical(|ui| {
                ui.checkbox(&mut draft.bracket_pair_colorization, "Color bracket pairs");
                ui.checkbox(
                    &mut draft.bracket_pair_colorization_independent_color_pool_per_bracket_type,
                    "Use independent color pools per bracket type",
                );
            });
            ui.end_row();

            ui.label("Bracket pair guides");
            editor_bracket_pair_guide_mode_combo(
                ui,
                "editor_bracket_pair_guides",
                &mut draft.bracket_pair_guides,
            );
            ui.end_row();

            ui.label("Horizontal bracket guides");
            editor_bracket_pair_guide_mode_combo(
                ui,
                "editor_bracket_pair_guides_horizontal",
                &mut draft.bracket_pair_guides_horizontal,
            );
            ui.end_row();

            ui.label("Active bracket guide");
            ui.checkbox(
                &mut draft.highlight_active_bracket_pair,
                "Highlight the active bracket pair guide",
            );
            ui.end_row();

            ui.label("Match brackets");
            editor_match_brackets_combo(ui, "editor_match_brackets", &mut draft.match_brackets);
            ui.end_row();

            ui.label("Find from selection");
            editor_find_seed_search_string_from_selection_combo(
                ui,
                "editor_find_seed_search_string_from_selection",
                &mut draft.find_seed_search_string_from_selection,
            );
            ui.end_row();

            ui.label("Auto find in selection");
            editor_find_auto_find_in_selection_combo(
                ui,
                "editor_find_auto_find_in_selection",
                &mut draft.find_auto_find_in_selection,
            );
            ui.end_row();

            ui.label("Find on type");
            ui.checkbox(
                &mut draft.find_on_type,
                "Search while typing in the Find box",
            );
            ui.end_row();

            ui.label("Cursor move on type");
            ui.checkbox(
                &mut draft.find_cursor_move_on_type,
                "Move the editor cursor while typing in the Find box",
            );
            ui.end_row();

            ui.label("Close on result");
            ui.checkbox(
                &mut draft.find_close_on_result,
                "Close the Find panel after a result is selected",
            );
            ui.end_row();

            ui.label("Find loop");
            ui.checkbox(
                &mut draft.find_loop,
                "Wrap find navigation at the first and last match",
            );
            ui.end_row();

            ui.label("Global find clipboard");
            ui.checkbox(
                &mut draft.find_global_find_clipboard,
                "Use the shared macOS Find clipboard",
            );
            ui.end_row();

            ui.label("Find top space");
            ui.checkbox(
                &mut draft.find_add_extra_space_on_top,
                "Add scroll space above the first line while finding",
            );
            ui.end_row();

            ui.label("Find history");
            editor_find_history_combo(ui, "editor_find_history", &mut draft.find_history);
            ui.end_row();

            ui.label("Replace history");
            editor_find_history_combo(
                ui,
                "editor_find_replace_history",
                &mut draft.find_replace_history,
            );
            ui.end_row();

            ui.label("Column ruler");
            ui.add(
                egui::DragValue::new(&mut draft.ruler_column)
                    .speed(1.0)
                    .range(0..=MAX_EDITOR_RULER_COLUMN),
            )
            .on_hover_text("Use 0 to hide the ruler");
            ui.end_row();

            ui.label("Minimap side");
            editor_minimap_side_combo(ui, "editor_minimap_side", &mut draft.minimap_side);
            ui.end_row();

            ui.label("Minimap autohide");
            editor_minimap_autohide_combo(
                ui,
                "editor_minimap_autohide",
                &mut draft.minimap_autohide,
            );
            ui.end_row();

            ui.label("Minimap size");
            editor_minimap_size_combo(ui, "editor_minimap_size", &mut draft.minimap_size);
            ui.end_row();

            ui.label("Minimap slider");
            editor_minimap_show_slider_combo(
                ui,
                "editor_minimap_show_slider",
                &mut draft.minimap_show_slider,
            );
            ui.end_row();

            ui.label("Minimap scale");
            ui.add(
                egui::DragValue::new(&mut draft.minimap_scale)
                    .speed(1.0)
                    .range(MIN_EDITOR_MINIMAP_SCALE..=MAX_EDITOR_MINIMAP_SCALE),
            );
            ui.end_row();

            ui.label("Minimap characters");
            ui.checkbox(&mut draft.minimap_render_characters, "Render characters");
            ui.end_row();

            ui.label("Minimap max column");
            ui.add(
                egui::DragValue::new(&mut draft.minimap_max_column)
                    .speed(1.0)
                    .range(MIN_EDITOR_MINIMAP_MAX_COLUMN..=MAX_EDITOR_MINIMAP_MAX_COLUMN),
            );
            ui.end_row();

            ui.label("Region section headers");
            ui.checkbox(
                &mut draft.minimap_show_region_section_headers,
                "Show named regions in minimap",
            );
            ui.end_row();

            ui.label("MARK section headers");
            ui.checkbox(
                &mut draft.minimap_show_mark_section_headers,
                "Show MARK comments in minimap",
            );
            ui.end_row();

            ui.label("MARK regex");
            bounded_singleline_text_edit(ui, &mut draft.minimap_mark_section_header_regex, 260.0);
            ui.end_row();

            ui.label("Section header font");
            guarded_f32_drag_value(
                ui,
                &mut draft.minimap_section_header_font_size,
                0.5,
                MIN_EDITOR_MINIMAP_SECTION_HEADER_FONT_SIZE
                    ..=MAX_EDITOR_MINIMAP_SECTION_HEADER_FONT_SIZE,
                DEFAULT_EDITOR_MINIMAP_SECTION_HEADER_FONT_SIZE,
            );
            ui.end_row();

            ui.label("Section header spacing");
            guarded_f32_drag_value(
                ui,
                &mut draft.minimap_section_header_letter_spacing,
                0.25,
                MIN_EDITOR_MINIMAP_SECTION_HEADER_LETTER_SPACING
                    ..=MAX_EDITOR_MINIMAP_SECTION_HEADER_LETTER_SPACING,
                DEFAULT_EDITOR_MINIMAP_SECTION_HEADER_LETTER_SPACING,
            );
            ui.end_row();

            ui.label("Overview ruler border");
            ui.checkbox(
                &mut draft.overview_ruler_border,
                "Draw a border around the overview strip",
            );
            ui.end_row();

            ui.label("Overview cursor marker");
            ui.checkbox(
                &mut draft.hide_cursor_in_overview_ruler,
                "Hide the cursor marker in the overview strip",
            );
            ui.end_row();
        });

    ui.add_space(12.0);
    settings_target_heading(ui, highlight, SETTINGS_TARGET_EDITOR_DIFF, "Diff Editor");
    egui::Grid::new("settings_editor_diff_grid")
        .num_columns(2)
        .spacing([18.0, 10.0])
        .show(ui, |ui| {
            ui.label("Ignore trim whitespace");
            ui.checkbox(
                &mut draft.diff_ignore_trim_whitespace,
                "Hide leading and trailing whitespace-only changes",
            );
            ui.end_row();

            ui.label("Algorithm");
            diff_algorithm_combo(ui, "diff_algorithm", &mut draft.diff_algorithm);
            ui.end_row();

            ui.label("Side by side");
            ui.checkbox(
                &mut draft.diff_render_side_by_side,
                "Open supported diffs side by side",
            );
            ui.end_row();

            ui.label("Resizable split");
            ui.checkbox(
                &mut draft.diff_enable_split_view_resizing,
                "Allow resizing side-by-side diff panes",
            );
            ui.end_row();

            ui.label("Split ratio");
            guarded_f32_drag_value(
                ui,
                &mut draft.diff_split_view_default_ratio,
                0.05,
                MIN_DIFF_SPLIT_VIEW_DEFAULT_RATIO..=MAX_DIFF_SPLIT_VIEW_DEFAULT_RATIO,
                DEFAULT_DIFF_SPLIT_VIEW_DEFAULT_RATIO,
            )
            .on_hover_text("Initial width ratio for the left side of side-by-side diffs");
            ui.end_row();

            ui.label("Inline when narrow");
            ui.checkbox(
                &mut draft.diff_use_inline_view_when_space_is_limited,
                "Use inline view below the breakpoint",
            );
            ui.end_row();

            ui.label("Inline breakpoint");
            ui.add(
                egui::DragValue::new(&mut draft.diff_render_side_by_side_inline_breakpoint)
                    .range(
                        MIN_DIFF_RENDER_SIDE_BY_SIDE_INLINE_BREAKPOINT
                            ..=MAX_DIFF_RENDER_SIDE_BY_SIDE_INLINE_BREAKPOINT,
                    )
                    .speed(10)
                    .suffix(" px"),
            );
            ui.end_row();

            ui.label("Compact mode");
            ui.checkbox(
                &mut draft.diff_compact_mode,
                "Optimize diff controls for small panes",
            );
            ui.end_row();

            ui.label("Editable original");
            ui.checkbox(
                &mut draft.diff_original_editable,
                "Allow editing the original side of supported diffs",
            );
            ui.end_row();

            ui.label("Code lens");
            ui.checkbox(
                &mut draft.diff_code_lens,
                "Show hunk actions on diff headers",
            );
            ui.end_row();

            ui.label("Verbose accessibility");
            ui.checkbox(
                &mut draft.diff_accessibility_verbose,
                "Use verbose accessibility labels in diffs",
            );
            ui.end_row();

            ui.label("Hide unchanged regions");
            ui.checkbox(
                &mut draft.diff_hide_unchanged_regions,
                "Collapse unchanged lines outside diff hunks",
            );
            ui.end_row();

            ui.label("Word wrap");
            diff_word_wrap_combo(ui, "diff_word_wrap", &mut draft.diff_word_wrap);
            ui.end_row();

            ui.label("Accessible viewer");
            ui.checkbox(
                &mut draft.diff_only_show_accessible_viewer,
                "Open diffs in the accessible diff viewer",
            );
            ui.end_row();

            ui.label("Context lines");
            ui.add(
                egui::DragValue::new(&mut draft.diff_context_lines)
                    .speed(1.0)
                    .range(MIN_DIFF_CONTEXT_LINES..=MAX_DIFF_CONTEXT_LINES),
            )
            .on_hover_text("Unchanged lines kept around each diff hunk");
            ui.end_row();

            ui.label("Minimum hidden lines");
            ui.add(
                egui::DragValue::new(&mut draft.diff_hide_unchanged_regions_minimum_line_count)
                    .speed(1.0)
                    .range(
                        MIN_DIFF_HIDE_UNCHANGED_REGIONS_MINIMUM_LINE_COUNT
                            ..=MAX_DIFF_HIDE_UNCHANGED_REGIONS_MINIMUM_LINE_COUNT,
                    ),
            )
            .on_hover_text(
                "Small unchanged gaps below this line count stay visible between diff hunks",
            );
            ui.end_row();

            ui.label("Reveal lines");
            ui.add(
                egui::DragValue::new(&mut draft.diff_hide_unchanged_regions_reveal_line_count)
                    .speed(1.0)
                    .range(
                        MIN_DIFF_HIDE_UNCHANGED_REGIONS_REVEAL_LINE_COUNT
                            ..=MAX_DIFF_HIDE_UNCHANGED_REGIONS_REVEAL_LINE_COUNT,
                    ),
            )
            .on_hover_text("Unchanged lines to reveal around hidden diff regions");
            ui.end_row();

            ui.label("Max computation time");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut draft.diff_max_computation_time_ms)
                        .speed(100.0)
                        .range(MIN_DIFF_MAX_COMPUTATION_TIME_MS..=MAX_DIFF_MAX_COMPUTATION_TIME_MS),
                );
                ui.label("ms");
            })
            .response
            .on_hover_text("Maximum diff computation time. Use 0 for no time limit.");
            ui.end_row();

            ui.label("Max file size");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut draft.diff_max_file_size_mb)
                        .speed(1.0)
                        .range(MIN_DIFF_MAX_FILE_SIZE_MB..=MAX_DIFF_MAX_FILE_SIZE_MB),
                );
                ui.label("MB");
            })
            .response
            .on_hover_text("Maximum file size supported by the diff editor. Use 0 for no limit.");
            ui.end_row();

            ui.label("Gutter actions");
            ui.checkbox(
                &mut draft.diff_render_gutter_menu,
                "Show stage and revert controls in diff gutters",
            );
            ui.end_row();

            ui.label("Revert icon");
            ui.checkbox(
                &mut draft.diff_render_margin_revert_icon,
                "Show the discard/revert control in diff gutters",
            );
            ui.end_row();

            ui.label("Indicators");
            ui.checkbox(
                &mut draft.diff_render_indicators,
                "Show +/- markers for added and removed lines",
            );
            ui.end_row();

            ui.label("Moved code");
            ui.checkbox(
                &mut draft.diff_experimental_show_moves,
                "Mark moved lines in patch diffs",
            );
            ui.end_row();

            ui.label("Empty decorations");
            ui.checkbox(
                &mut draft.diff_experimental_show_empty_decorations,
                "Mark empty added and removed diff lines",
            );
            ui.end_row();

            ui.label("True inline view");
            ui.checkbox(
                &mut draft.diff_experimental_use_true_inline_view,
                "Use the experimental inline diff layout",
            );
            ui.end_row();

            ui.label("Overview ruler");
            ui.checkbox(
                &mut draft.diff_render_overview_ruler,
                "Show diff markers on the editor overview strip",
            );
            ui.end_row();

            ui.label("Embedded editor");
            ui.checkbox(
                &mut draft.diff_is_in_embedded_editor,
                "Use embedded-editor diff behavior",
            );
            ui.end_row();
        });
}

pub(super) fn render_source_control_settings_with_highlight(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    settings_target_heading(
        ui,
        highlight,
        SETTINGS_TARGET_EDITOR_SOURCE_CONTROL,
        "Source Control",
    );
    egui::Grid::new("settings_editor_scm_decorations_grid")
        .num_columns(2)
        .spacing([18.0, 10.0])
        .show(ui, |ui| {
            ui.label("Git enabled");
            ui.checkbox(&mut draft.git_enabled, "Enable Git source control features");
            ui.end_row();

            ui.label("Git path");
            render_string_list_input(
                ui,
                &mut draft.git_path,
                "C:/Program Files/Git/bin/git.exe",
                2,
            );
            ui.end_row();

            ui.label("AI co-author");
            git_add_ai_co_author_combo(ui, "git_add_ai_co_author", &mut draft.git_add_ai_co_author);
            ui.end_row();

            ui.label("Git autorefresh");
            ui.checkbox(
                &mut draft.git_autorefresh,
                "Automatically refresh Git changes after workspace file changes",
            );
            ui.end_row();

            ui.label("Auto fetch");
            git_autofetch_combo(ui, "git_autofetch", &mut draft.git_autofetch);
            ui.end_row();

            ui.label("Fetch period");
            ui.add(
                egui::DragValue::new(&mut draft.git_autofetch_period)
                    .speed(10.0)
                    .range(MIN_GIT_AUTOFETCH_PERIOD..=MAX_GIT_AUTOFETCH_PERIOD),
            )
            .on_hover_text("Seconds between automatic Git fetches");
            ui.end_row();

            ui.label("Repository detection");
            git_auto_repository_detection_combo(
                ui,
                "git_auto_repository_detection",
                &mut draft.git_auto_repository_detection,
            );
            ui.end_row();

            ui.label("Ignore limit warning");
            ui.checkbox(
                &mut draft.git_ignore_limit_warning,
                "Hide warning when Git changes hit the status limit",
            );
            ui.end_row();

            ui.label("Ignore submodules");
            ui.checkbox(
                &mut draft.git_ignore_submodules,
                "Ignore modifications to Git submodules",
            );
            ui.end_row();

            ui.label("Ignored repositories");
            render_git_ignored_repositories_input(ui, draft);
            ui.end_row();

            ui.label("Scan ignored folders");
            render_git_repository_scan_ignored_folders_input(ui, draft);
            ui.end_row();

            ui.label("Scan max depth");
            ui.add(
                egui::DragValue::new(&mut draft.git_repository_scan_max_depth)
                    .speed(1.0)
                    .range(MIN_GIT_REPOSITORY_SCAN_MAX_DEPTH..=MAX_GIT_REPOSITORY_SCAN_MAX_DEPTH),
            )
            .on_hover_text("Maximum folder depth used when detecting Git repositories");
            ui.end_row();

            ui.label("Parent repositories");
            git_open_repository_in_parent_folders_combo(
                ui,
                "git_open_repository_in_parent_folders",
                &mut draft.git_open_repository_in_parent_folders,
            );
            ui.end_row();

            ui.label("Detect submodules");
            ui.checkbox(
                &mut draft.git_detect_submodules,
                "Automatically detect Git submodule changes",
            );
            ui.end_row();

            ui.label("Submodule detect limit");
            ui.add(
                egui::DragValue::new(&mut draft.git_detect_submodules_limit)
                    .speed(1.0)
                    .range(MIN_GIT_DETECT_SUBMODULES_LIMIT..=MAX_GIT_DETECT_SUBMODULES_LIMIT),
            )
            .on_hover_text("Maximum number of Git submodules scanned for Source Control");
            ui.end_row();

            ui.label("Detect worktrees");
            ui.checkbox(
                &mut draft.git_detect_worktrees,
                "Detect Git worktree repositories",
            );
            ui.end_row();

            ui.label("Worktree detect limit");
            ui.add(
                egui::DragValue::new(&mut draft.git_detect_worktrees_limit)
                    .speed(1.0)
                    .range(MIN_GIT_DETECT_WORKTREES_LIMIT..=MAX_GIT_DETECT_WORKTREES_LIMIT),
            )
            .on_hover_text("Maximum number of Git worktrees scanned for Source Control");
            ui.end_row();

            ui.label("Worktree include files");
            render_string_list_input(
                ui,
                &mut draft.git_worktree_include_files,
                "packages/app\napps/web",
                3,
            );
            ui.end_row();

            ui.label("Scan repositories");
            render_string_list_input(ui, &mut draft.git_scan_repositories, "../repo\nC:/repo", 3);
            ui.end_row();

            ui.label("Clone directory");
            render_optional_string_input(
                ui,
                &mut draft.git_default_clone_directory,
                "C:/Users/name/source",
            );
            ui.end_row();

            ui.label("Open after clone");
            git_open_after_clone_combo(ui, "git_open_after_clone", &mut draft.git_open_after_clone);
            ui.end_row();

            ui.label("Similarity threshold");
            ui.add(
                egui::DragValue::new(&mut draft.git_similarity_threshold)
                    .speed(1.0)
                    .range(MIN_GIT_SIMILARITY_THRESHOLD..=MAX_GIT_SIMILARITY_THRESHOLD),
            )
            .on_hover_text("Similarity percent required for Git rename detection");
            ui.end_row();

            ui.label("Default view");
            scm_default_view_mode_combo(
                ui,
                "scm_default_view_mode",
                &mut draft.scm_default_view_mode,
            );
            ui.end_row();

            ui.label("Default sort");
            scm_default_view_sort_key_combo(
                ui,
                "scm_default_view_sort_key",
                &mut draft.scm_default_view_sort_key,
            );
            ui.end_row();

            ui.label("Auto reveal");
            ui.checkbox(
                &mut draft.scm_auto_reveal,
                "Select the active changed file in Source Control",
            );
            ui.end_row();

            ui.label("Count badge");
            scm_count_badge_combo(ui, "scm_count_badge", &mut draft.scm_count_badge);
            ui.end_row();

            ui.label("Provider count badge");
            scm_provider_count_badge_combo(
                ui,
                "scm_provider_count_badge",
                &mut draft.scm_provider_count_badge,
            );
            ui.end_row();

            ui.label("Repositories");
            ui.checkbox(
                &mut draft.scm_always_show_repositories,
                "Always show the Source Control repositories section",
            );
            ui.end_row();

            ui.label("Visible repositories");
            ui.add(
                egui::DragValue::new(&mut draft.scm_repositories_visible)
                    .speed(1.0)
                    .range(MIN_SCM_REPOSITORIES_VISIBLE..=MAX_SCM_REPOSITORIES_VISIBLE),
            )
            .on_hover_text("Number of repositories shown in Source Control");
            ui.end_row();

            ui.label("Compact folders");
            ui.checkbox(
                &mut draft.scm_compact_folders,
                "Compact single-folder chains in the Source Control tree",
            );
            ui.end_row();

            ui.label("Page on scroll");
            ui.checkbox(
                &mut draft.scm_graph_page_on_scroll,
                "Load the next graph page when scrolling to the end",
            );
            ui.end_row();

            ui.label("Graph page size");
            ui.add(
                egui::DragValue::new(&mut draft.scm_graph_page_size)
                    .speed(10.0)
                    .range(MIN_SCM_GRAPH_PAGE_SIZE..=MAX_SCM_GRAPH_PAGE_SIZE),
            )
            .on_hover_text("Commit count loaded by default in the Source Control graph/history");
            ui.end_row();

            ui.label("Graph badges");
            scm_graph_badges_combo(ui, "scm_graph_badges", &mut draft.scm_graph_badges);
            ui.end_row();

            ui.label("Incoming changes");
            ui.checkbox(
                &mut draft.scm_graph_show_incoming_changes,
                "Show incoming changes in the Source Control graph",
            );
            ui.end_row();

            ui.label("Outgoing changes");
            ui.checkbox(
                &mut draft.scm_graph_show_outgoing_changes,
                "Show outgoing changes in the Source Control graph",
            );
            ui.end_row();

            ui.label("Commit input");
            ui.checkbox(
                &mut draft.git_show_commit_input,
                "Show commit message input and commit button",
            );
            ui.end_row();

            ui.label("Editor commit input");
            ui.checkbox(
                &mut draft.git_use_editor_as_commit_input,
                "Use the editor-style multiline commit input",
            );
            ui.end_row();

            ui.label("Verbose commit");
            ui.checkbox(
                &mut draft.git_verbose_commit,
                "Show verbose staged-change output with the editor commit input",
            );
            ui.end_row();

            ui.label("Post commit");
            git_post_commit_command_combo(
                ui,
                "git_post_commit_command",
                &mut draft.git_post_commit_command,
            );
            ui.end_row();

            ui.label("Remember post commit");
            ui.checkbox(
                &mut draft.git_remember_post_commit_command,
                "Remember the selected post-commit command",
            );
            ui.end_row();

            ui.label("Commit action button");
            ui.checkbox(
                &mut draft.git_show_action_button_commit,
                "Show the Git Commit button in the Source Control input",
            );
            ui.end_row();

            ui.label("Always sign off");
            ui.checkbox(
                &mut draft.git_always_sign_off,
                "Add a Signed-off-by trailer to Git commits",
            );
            ui.end_row();

            ui.label("Commit signing");
            ui.checkbox(&mut draft.git_enable_commit_signing, "Sign Git commits");
            ui.end_row();

            ui.label("Diagnostics hook");
            ui.checkbox(
                &mut draft.git_diagnostics_commit_hook_enabled,
                "Run diagnostics before Git commit",
            );
            ui.end_row();

            ui.label("Diagnostics sources");
            render_string_map_input(
                ui,
                &mut draft.git_diagnostics_commit_hook_sources,
                "*=error\nrust=warning",
                2,
            );
            ui.end_row();

            ui.label("Allow no verify");
            ui.checkbox(
                &mut draft.git_allow_no_verify_commit,
                "Allow commits that skip Git hooks",
            );
            ui.end_row();

            ui.label("Confirm no verify");
            ui.checkbox(
                &mut draft.git_confirm_no_verify_commit,
                "Ask before committing with no-verify",
            );
            ui.end_row();

            ui.label("Confirm committed delete");
            ui.checkbox(
                &mut draft.git_confirm_committed_delete,
                "Ask before deleting files that are committed in Git",
            );
            ui.end_row();

            ui.label("Confirm empty commits");
            ui.checkbox(
                &mut draft.git_confirm_empty_commits,
                "Ask before creating a commit without staged changes",
            );
            ui.end_row();

            ui.label("Confirm force push");
            ui.checkbox(
                &mut draft.git_confirm_force_push,
                "Ask before force pushing",
            );
            ui.end_row();

            ui.label("Allow force push");
            ui.checkbox(
                &mut draft.git_allow_force_push,
                "Allow Git force push actions",
            );
            ui.end_row();

            ui.label("Force push lease");
            ui.checkbox(
                &mut draft.git_use_force_push_with_lease,
                "Use --force-with-lease for force push",
            );
            ui.end_row();

            ui.label("Force push includes");
            ui.checkbox(
                &mut draft.git_use_force_push_if_includes,
                "Use --force-if-includes for force push",
            );
            ui.end_row();

            ui.label("Require user config");
            ui.checkbox(
                &mut draft.git_require_user_config,
                "Require explicit Git user name and email before committing",
            );
            ui.end_row();

            ui.label("Git progress");
            ui.checkbox(
                &mut draft.git_show_progress,
                "Show progress messages for Git operations",
            );
            ui.end_row();

            ui.label("Push success");
            ui.checkbox(
                &mut draft.git_show_push_success_notification,
                "Show notification after a successful Git push",
            );
            ui.end_row();

            ui.label("Status bar sync");
            ui.checkbox(
                &mut draft.git_enable_status_bar_sync,
                "Show sync actions in the status bar",
            );
            ui.end_row();

            ui.label("Confirm sync");
            ui.checkbox(
                &mut draft.git_confirm_sync,
                "Ask before synchronizing changes",
            );
            ui.end_row();

            ui.label("Fetch on pull");
            ui.checkbox(&mut draft.git_fetch_on_pull, "Fetch before pulling");
            ui.end_row();

            ui.label("Prune on fetch");
            ui.checkbox(
                &mut draft.git_prune_on_fetch,
                "Prune deleted remotes while fetching",
            );
            ui.end_row();

            ui.label("Pull tags");
            ui.checkbox(&mut draft.git_pull_tags, "Fetch tags when pulling");
            ui.end_row();

            ui.label("Follow tags");
            ui.checkbox(
                &mut draft.git_follow_tags_when_sync,
                "Follow tags when syncing",
            );
            ui.end_row();

            ui.label("Rebase sync");
            ui.checkbox(&mut draft.git_rebase_when_sync, "Use rebase while syncing");
            ui.end_row();

            ui.label("Replace pull tags");
            ui.checkbox(
                &mut draft.git_replace_tags_when_pull,
                "Replace local tags when pulling",
            );
            ui.end_row();

            ui.label("Pull before checkout");
            ui.checkbox(
                &mut draft.git_pull_before_checkout,
                "Pull current branch before checkout",
            );
            ui.end_row();

            ui.label("Auto stash");
            ui.checkbox(
                &mut draft.git_auto_stash,
                "Stash changes before pull when needed",
            );
            ui.end_row();

            ui.label("Trash untracked");
            ui.checkbox(
                &mut draft.git_discard_untracked_changes_to_trash,
                "Move discarded untracked files to the OS trash",
            );
            ui.end_row();

            ui.label("Merge editor");
            ui.checkbox(
                &mut draft.git_merge_editor,
                "Use the merge editor for Git conflicts",
            );
            ui.end_row();

            ui.label("Optimistic update");
            ui.checkbox(
                &mut draft.git_optimistic_update,
                "Update Source Control optimistically after Git operations",
            );
            ui.end_row();

            ui.label("Cancellation");
            ui.checkbox(
                &mut draft.git_support_cancellation,
                "Allow supported Git operations to be cancelled",
            );
            ui.end_row();

            ui.label("Terminal auth");
            ui.checkbox(
                &mut draft.git_terminal_authentication,
                "Use terminal authentication for Git operations",
            );
            ui.end_row();

            ui.label("Terminal Git editor");
            ui.checkbox(
                &mut draft.git_terminal_git_editor,
                "Use this app as the Git editor in integrated terminals",
            );
            ui.end_row();

            ui.label("Integrated askpass");
            ui.checkbox(
                &mut draft.git_use_integrated_ask_pass,
                "Use integrated askpass for Git authentication prompts",
            );
            ui.end_row();

            ui.label("Ignore legacy warning");
            ui.checkbox(
                &mut draft.git_ignore_legacy_warning,
                "Hide Git legacy warnings",
            );
            ui.end_row();

            ui.label("Ignore missing Git");
            ui.checkbox(
                &mut draft.git_ignore_missing_git_warning,
                "Hide warnings when Git is missing",
            );
            ui.end_row();

            ui.label("Ignore rebase warning");
            ui.checkbox(
                &mut draft.git_ignore_rebase_warning,
                "Hide Git rebase warnings",
            );
            ui.end_row();

            ui.label("Ignore Windows Git 2.7");
            ui.checkbox(
                &mut draft.git_ignore_windows_git27_warning,
                "Hide Windows Git 2.7 warnings",
            );
            ui.end_row();

            ui.label("Commands to log");
            render_string_list_input(ui, &mut draft.git_commands_to_log, "fetch\npull", 2);
            ui.end_row();

            ui.label("Reference details");
            ui.checkbox(
                &mut draft.git_show_reference_details,
                "Show branch/reference details in Source Control",
            );
            ui.end_row();

            ui.label("History author");
            ui.checkbox(
                &mut draft.git_timeline_show_author,
                "Show commit authors in Source Control history",
            );
            ui.end_row();

            ui.label("History uncommitted");
            ui.checkbox(
                &mut draft.git_timeline_show_uncommitted,
                "Show uncommitted changes in Source Control history",
            );
            ui.end_row();

            ui.label("History date");
            git_timeline_date_combo(ui, "git_timeline_date", &mut draft.git_timeline_date);
            ui.end_row();

            ui.label("Inline open file action");
            ui.checkbox(
                &mut draft.git_show_inline_open_file_action,
                "Show Open File on Source Control rows",
            );
            ui.end_row();

            ui.label("Git count badge");
            git_count_badge_combo(ui, "git_count_badge", &mut draft.git_count_badge);
            ui.end_row();

            ui.label("Status limit");
            ui.add(
                egui::DragValue::new(&mut draft.git_status_limit)
                    .speed(100.0)
                    .range(MIN_GIT_STATUS_LIMIT..=MAX_GIT_STATUS_LIMIT),
            )
            .on_hover_text("Maximum number of Git changes scanned for Source Control");
            ui.end_row();

            ui.label("Untracked changes");
            git_untracked_changes_combo(
                ui,
                "git_untracked_changes",
                &mut draft.git_untracked_changes,
            );
            ui.end_row();

            ui.label("Open diff on click");
            ui.checkbox(
                &mut draft.git_open_diff_on_click,
                "Open a diff when clicking Source Control rows",
            );
            ui.end_row();

            ui.label("Close diff on operation");
            ui.checkbox(
                &mut draft.git_close_diff_on_operation,
                "Close affected Source Control diffs after Git operations",
            );
            ui.end_row();

            ui.label("Always show staged group");
            ui.checkbox(
                &mut draft.git_always_show_staged_changes_resource_group,
                "Show Staged Changes even when it is empty",
            );
            ui.end_row();

            ui.label("Checkout refs");
            render_git_checkout_type(ui, draft);
            ui.end_row();

            ui.label("Branch sort order");
            git_branch_sort_order_combo(
                ui,
                "git_branch_sort_order",
                &mut draft.git_branch_sort_order,
            );
            ui.end_row();

            ui.label("Default branch");
            bounded_singleline_text_edit_with_hint(
                ui,
                &mut draft.git_default_branch_name,
                bounded_settings_text_edit_width(ui.available_width(), 420.0),
                Some("main"),
            )
            .on_hover_text("Default branch name used for new Git repositories");
            ui.end_row();

            ui.label("Branch prefix");
            bounded_singleline_text_edit_with_hint(
                ui,
                &mut draft.git_branch_prefix,
                bounded_settings_text_edit_width(ui.available_width(), 420.0),
                Some("feature/"),
            )
            .on_hover_text("Prefix applied to new Git branch names");
            ui.end_row();

            ui.label("Branch validation regex");
            bounded_singleline_text_edit_with_hint(
                ui,
                &mut draft.git_branch_validation_regex,
                bounded_settings_text_edit_width(ui.available_width(), 420.0),
                Some("^feature/"),
            )
            .on_hover_text("Regex new Git branch names must match before create or rename");
            ui.end_row();

            ui.label("Branch whitespace");
            bounded_singleline_text_edit_with_hint(
                ui,
                &mut draft.git_branch_whitespace_char,
                80.0,
                Some("-"),
            )
            .on_hover_text("Replacement text for whitespace in new Git branch names");
            ui.end_row();

            ui.label("Random branch names");
            ui.checkbox(
                &mut draft.git_branch_random_name_enable,
                "Generate random branch names",
            );
            ui.end_row();

            ui.label("Random dictionaries");
            render_string_list_input(
                ui,
                &mut draft.git_branch_random_name_dictionary,
                "adjectives\nanimals",
                2,
            );
            ui.end_row();

            ui.label("Protected branches");
            render_git_branch_protection_input(ui, draft);
            ui.end_row();

            ui.label("Protected branch prompt");
            git_branch_protection_prompt_combo(
                ui,
                "git_branch_protection_prompt",
                &mut draft.git_branch_protection_prompt,
            );
            ui.end_row();

            ui.label("Git decorations");
            ui.checkbox(
                &mut draft.git_decorations_enabled,
                "Show Git colors and badges in the Explorer",
            );
            ui.end_row();

            ui.label("Smart commit");
            ui.checkbox(
                &mut draft.git_enable_smart_commit,
                "Commit eligible changes when nothing is staged",
            );
            ui.end_row();

            ui.label("Suggest smart commit");
            ui.checkbox(
                &mut draft.git_suggest_smart_commit,
                "Offer to stage eligible changes when committing with nothing staged",
            );
            ui.end_row();

            ui.label("Smart commit changes");
            git_smart_commit_changes_combo(
                ui,
                "git_smart_commit_changes",
                &mut draft.git_smart_commit_changes,
            );
            ui.end_row();

            ui.label("Save before commit");
            git_prompt_to_save_files_combo(
                ui,
                "git_prompt_to_save_files_before_commit",
                &mut draft.git_prompt_to_save_files_before_commit,
            );
            ui.end_row();

            ui.label("Save before stash");
            git_prompt_to_save_files_combo(
                ui,
                "git_prompt_to_save_files_before_stash",
                &mut draft.git_prompt_to_save_files_before_stash,
            );
            ui.end_row();

            ui.label("Stash message");
            ui.checkbox(
                &mut draft.git_use_commit_input_as_stash_message,
                "Use the commit input when saving a stash without a message",
            );
            ui.end_row();

            ui.label("Short hash length");
            ui.add(
                egui::DragValue::new(&mut draft.git_commit_short_hash_length)
                    .range(MIN_GIT_COMMIT_SHORT_HASH_LENGTH..=MAX_GIT_COMMIT_SHORT_HASH_LENGTH),
            );
            ui.end_row();

            ui.label("Commit validation");
            ui.checkbox(
                &mut draft.git_input_validation,
                "Show warnings for long commit message lines",
            );
            ui.end_row();

            ui.label("Validation line length");
            ui.add(
                egui::DragValue::new(&mut draft.git_input_validation_length)
                    .range(MIN_GIT_INPUT_VALIDATION_LENGTH..=MAX_GIT_INPUT_VALIDATION_LENGTH),
            );
            ui.end_row();

            ui.label("Validation subject length");
            render_git_input_validation_subject_length(ui, draft);
            ui.end_row();

            ui.label("Blame status bar");
            ui.checkbox(
                &mut draft.git_blame_status_bar_item_enabled,
                "Show active-line git blame in the status bar",
            );
            ui.end_row();

            ui.label("Blame editor decoration");
            ui.checkbox(
                &mut draft.git_blame_editor_decoration_enabled,
                "Show active-line git blame in the editor",
            );
            ui.end_row();

            ui.label("Blame hover");
            ui.checkbox(
                &mut draft.git_blame_editor_decoration_disable_hover,
                "Disable hover for blame editor decorations",
            );
            ui.end_row();

            ui.label("Blame ignore whitespace");
            ui.checkbox(
                &mut draft.git_blame_ignore_whitespace,
                "Ignore whitespace-only changes when resolving git blame",
            );
            ui.end_row();

            ui.label("Blame status template");
            bounded_singleline_text_edit(ui, &mut draft.git_blame_status_bar_item_template, 260.0)
                .on_hover_text(
                    "Placeholders include ${authorName}, ${authorDateAgo}, ${subject}, and ${hash}",
                );
            ui.end_row();

            ui.label("Blame editor template");
            bounded_singleline_text_edit(
                ui,
                &mut draft.git_blame_editor_decoration_template,
                260.0,
            )
            .on_hover_text(
                "Placeholders include ${authorName}, ${authorDateAgo}, ${subject}, and ${hash}",
            );
            ui.end_row();

            ui.label("Input action button");
            ui.checkbox(
                &mut draft.scm_show_input_action_button,
                "Show the commit button in the Source Control input",
            );
            ui.end_row();

            ui.label("Input min lines");
            ui.add(
                egui::DragValue::new(&mut draft.scm_input_min_line_count)
                    .speed(1.0)
                    .range(MIN_SCM_INPUT_LINE_COUNT..=MAX_SCM_INPUT_LINE_COUNT),
            )
            .on_hover_text("Minimum visible lines for the Source Control input");
            ui.end_row();

            ui.label("Input max lines");
            ui.add(
                egui::DragValue::new(&mut draft.scm_input_max_line_count)
                    .speed(1.0)
                    .range(MIN_SCM_INPUT_LINE_COUNT..=MAX_SCM_INPUT_LINE_COUNT),
            )
            .on_hover_text("Maximum visible lines for the Source Control input");
            ui.end_row();

            ui.label("Input font family");
            bounded_singleline_text_edit(ui, &mut draft.scm_input_font_family, 180.0)
                .on_hover_text("Use default, editor, or a loaded font family name");
            ui.end_row();

            ui.label("Input font size");
            guarded_f32_drag_value(
                ui,
                &mut draft.scm_input_font_size,
                0.5,
                MIN_SCM_INPUT_FONT_SIZE..=MAX_SCM_INPUT_FONT_SIZE,
                DEFAULT_SCM_INPUT_FONT_SIZE,
            )
            .on_hover_text("Source Control input font size in pixels");
            ui.end_row();

            ui.label("Always show actions");
            ui.checkbox(
                &mut draft.scm_always_show_actions,
                "Keep inline file actions visible in the Source Control list",
            );
            ui.end_row();

            ui.label("Action button");
            ui.checkbox(
                &mut draft.scm_show_action_button,
                "Show Source Control view action buttons",
            );
            ui.end_row();

            ui.label("Diff decorations");
            scm_diff_decorations_combo(ui, "scm_diff_decorations", &mut draft.scm_diff_decorations);
            ui.end_row();

            ui.label("Gutter click");
            scm_diff_decorations_gutter_action_combo(
                ui,
                "scm_diff_decorations_gutter_action",
                &mut draft.scm_diff_decorations_gutter_action,
            );
            ui.end_row();

            ui.label("Gutter visibility");
            scm_diff_decorations_gutter_visibility_combo(
                ui,
                "scm_diff_decorations_gutter_visibility",
                &mut draft.scm_diff_decorations_gutter_visibility,
            );
            ui.end_row();

            ui.label("Gutter width");
            ui.add(
                egui::DragValue::new(&mut draft.scm_diff_decorations_gutter_width)
                    .speed(1.0)
                    .range(
                        MIN_SCM_DIFF_DECORATIONS_GUTTER_WIDTH
                            ..=MAX_SCM_DIFF_DECORATIONS_GUTTER_WIDTH,
                    ),
            );
            ui.end_row();

            ui.label("Gutter pattern");
            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut draft.scm_diff_decorations_gutter_pattern.added,
                    "Added",
                );
                ui.checkbox(
                    &mut draft.scm_diff_decorations_gutter_pattern.modified,
                    "Modified",
                );
            });
            ui.end_row();

            ui.label("Ignore trim whitespace");
            scm_diff_decorations_ignore_trim_whitespace_combo(
                ui,
                "scm_diff_decorations_ignore_trim_whitespace",
                &mut draft.scm_diff_decorations_ignore_trim_whitespace,
            );
            ui.end_row();
        });
}

#[cfg(test)]
mod tests;
