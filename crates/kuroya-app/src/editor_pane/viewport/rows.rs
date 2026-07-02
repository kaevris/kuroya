use crate::{
    editor_bracket_overlay_cache::EditorBracketOverlayCache,
    editor_pane_actions::PendingEditorPaneActions,
    editor_pane_data::EditorPaneData,
    editor_pane_rows::{EditorRowContext, render_editor_row},
    editor_row_paint::active_indent_guide_column_for_buffer,
    syntax::SyntaxHighlighter,
};
use eframe::egui;
use kuroya_core::TextBuffer;
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
struct VisiblePhysicalLineBatch {
    visible_rows: Range<usize>,
    physical_lines: Range<usize>,
}

const MAX_BATCHED_FOLDED_HIDDEN_LINES: usize = 32;

pub(super) fn render_visible_editor_rows(
    ui: &mut egui::Ui,
    rows: Range<usize>,
    row_left_inset: f32,
    row_width: f32,
    buffer: &TextBuffer,
    highlighter: &mut SyntaxHighlighter,
    bracket_overlay_cache: &mut EditorBracketOverlayCache,
    data: &EditorPaneData,
    active_find_match: usize,
    pending_actions: &mut PendingEditorPaneActions,
) {
    let active_indent_guide_column = (data.indent_guides
        && data.highlight_active_indentation.visible(data.focused))
    .then(|| active_indent_guide_column_for_buffer(buffer, data.tab_width))
    .flatten();
    let visuals = ui.visuals();
    let row_context = EditorRowContext {
        buffer,
        row_height: data.row_height,
        row_left_inset,
        row_width,
        gutter_width: data.gutter_width,
        char_width: data.char_width,
        font_size: data.font_size,
        text_color: visuals.text_color(),
        weak_text_color: visuals.weak_text_color(),
        selection_bg_fill: data.selection_bg_fill,
        warn_fg_color: visuals.warn_fg_color,
        line_numbers: data.line_numbers,
        select_on_line_numbers: data.select_on_line_numbers,
        render_whitespace: data.render_whitespace,
        experimental_whitespace_rendering: data.experimental_whitespace_rendering,
        render_final_newline: data.render_final_newline,
        render_control_characters: data.render_control_characters,
        unicode_highlight_ambiguous_characters: data.unicode_highlight_ambiguous_characters,
        unicode_highlight_invisible_characters: data.unicode_highlight_invisible_characters,
        unicode_highlight_non_basic_ascii: data.unicode_highlight_non_basic_ascii,
        unicode_highlight_allowed_characters: &data.unicode_highlight_allowed_characters,
        unicode_highlight_allowed_locales: &data.unicode_highlight_allowed_locales,
        render_line_highlight: data.render_line_highlight,
        render_line_highlight_only_when_focus: data.render_line_highlight_only_when_focus,
        word_wrap: data.word_wrap,
        word_wrap_column: data.word_wrap_column,
        stop_rendering_line_after: data.stop_rendering_line_after,
        bracket_pair_colorization: data.bracket_pair_colorization,
        bracket_pair_guides: data.bracket_pair_guides,
        bracket_pair_guides_horizontal: data.bracket_pair_guides_horizontal,
        highlight_active_bracket_pair: data.highlight_active_bracket_pair,
        match_brackets: data.match_brackets,
        folding: data.folding,
        folding_highlight: data.folding_highlight,
        show_folding_controls: data.show_folding_controls,
        unfold_on_click_after_end_of_line: data.unfold_on_click_after_end_of_line,
        contextmenu: data.contextmenu,
        focused: data.focused,
        multi_cursor_modifier: data.multi_cursor_modifier,
        double_click_selects_block: data.double_click_selects_block,
        drag_and_drop: data.drag_and_drop,
        selection_clipboard: data.selection_clipboard,
        mouse_middle_click_action: data.mouse_middle_click_action,
        mouse_style: data.mouse_style,
        glyph_margin: data.glyph_margin,
        lightbulb: data.lightbulb,
        indent_guides: data.indent_guides,
        active_indent_guide_column,
        ruler_column: data.ruler_column,
        rounded_selection: data.rounded_selection,
        color_decorators: data.color_decorators,
        color_decorators_activated_on: data.color_decorators_activated_on,
        color_decorators_limit: data.color_decorators_limit,
        default_color_decorators: data.default_color_decorators,
        tab_width: data.tab_width,
        cursor_smooth_caret_animation: data.cursor_smooth_caret_animation,
        cursor_style: data.cursor_style,
        cursor_blinking: data.cursor_blinking,
        cursor_width: data.cursor_width,
        cursor_height: data.cursor_height,
        ime_output_enabled: data.ime_output_enabled,
        accessibility_enabled: data.accessibility_enabled,
        accessibility_page_size: data.accessibility_page_size,
        aria_label: &data.aria_label,
        aria_required: data.aria_required,
        render_rich_screen_reader_content: data.render_rich_screen_reader_content,
        tab_index: data.tab_index,
        diff_lines: &data.diff_lines,
        cursor_positions: &data.cursor_positions,
        selections: &data.selections,
        find_matches: &data.find_matches,
        active_find_match,
        document_highlight_ranges: &data.document_highlight_ranges,
        semantic_token_ranges: &data.semantic_token_ranges,
        syntax_injections: &data.syntax_injections,
        diagnostics_by_line: &data.diagnostics_by_line,
        diagnostic_messages: &data.diagnostic_messages,
        diagnostic_tag_spans: &data.diagnostic_tag_spans,
        git_blame_editor_decoration_enabled: data.git_blame_editor_decoration_enabled,
        git_blame_editor_decoration_disable_hover: data.git_blame_editor_decoration_disable_hover,
        git_blame_editor_decoration_template: &data.git_blame_editor_decoration_template,
        git_blame_lines: &data.git_blame_lines,
        folding_ranges: &data.folding_ranges,
        inlay_hints: &data.inlay_hints,
        inlay_hints_font_family: &data.inlay_hints_font_family,
        inlay_hints_font_size: data.inlay_hints_font_size,
        inlay_hints_padding: data.inlay_hints_padding,
        inlay_hints_maximum_length: data.inlay_hints_maximum_length,
        code_lenses: &data.code_lenses,
        code_lens_font_family: &data.code_lens_font_family,
        code_lens_font_size: data.code_lens_font_size,
        completion_preview: data.completion_preview.as_ref(),
        ime_preedit: data.ime_preedit.as_deref(),
        folded_ranges: &data.folded_ranges,
        bracket_matches: &data.bracket_matches,
        active_bracket_pair_matches: &data.active_bracket_pair_matches,
        bracket_pair_guide_ranges: &data.bracket_pair_guide_ranges,
        merge_conflicts: &data.merge_conflicts,
        diff_stage: data.diff_stage,
        diff_move_lines: &data.diff_move_lines,
        diff_render_gutter_menu: data.diff_render_gutter_menu,
        diff_render_indicators: data.diff_render_indicators,
        diff_render_margin_revert_icon: data.diff_render_margin_revert_icon,
        diff_accessibility_verbose: data.diff_accessibility_verbose,
        diff_experimental_show_empty_decorations: data.diff_experimental_show_empty_decorations,
        show_scm_diff_gutter: data.show_scm_diff_gutter,
        scm_diff_decorations_gutter_action: data.scm_diff_decorations_gutter_action,
        scm_diff_decorations_gutter_visibility: data.scm_diff_decorations_gutter_visibility,
        scm_diff_decorations_gutter_width: data.scm_diff_decorations_gutter_width,
        scm_diff_decorations_gutter_pattern: data.scm_diff_decorations_gutter_pattern,
        staged_hunk_actions: data.staged_hunk_actions,
        source_control_unstaged_actions: data.source_control_unstaged_actions,
        source_control_staged_actions: data.source_control_staged_actions,
        source_control_discard_actions: data.source_control_discard_actions,
        source_control_path_actions: data.source_control_path_actions,
        compare_saved_actions: data.compare_saved_actions,
        compare_file_actions: data.compare_file_actions,
        compare_with_selected_actions: data.compare_with_selected_actions,
        diff_base_file_actions: data.diff_base_file_actions,
        diff_source_file_actions: data.diff_source_file_actions,
        diff_patch_actions: data.diff_patch_actions,
        diff_refresh_actions: data.diff_refresh_actions,
        diff_swap_actions: data.diff_swap_actions,
    };
    for_visible_physical_line_batch(
        rows,
        data.visible_line_count,
        buffer.len_lines(),
        data.folding,
        &data.visible_line_indices,
        |batch| {
            let physical_start = batch.physical_lines.start;
            let bracket_colors = if data.bracket_pair_colorization {
                bracket_overlay_cache.bracket_colors_for_lines(
                    buffer,
                    physical_start,
                    batch.physical_lines.end.saturating_sub(physical_start),
                    data.bracket_pair_colorization_independent_color_pool_per_bracket_type,
                )
            } else {
                Vec::new()
            };
            let mut highlighted_jobs = highlighter.layout_visible(
                buffer,
                data.font_size,
                data.tab_width,
                batch.physical_lines.clone(),
                data.syntax_highlighting,
                ui.visuals().text_color(),
                data.stop_rendering_line_after,
            );
            for visible_idx in batch.visible_rows {
                let line_idx = physical_line_for_visible_row(
                    visible_idx,
                    data.folding,
                    &data.visible_line_indices,
                );
                let highlighted_job = line_idx
                    .checked_sub(physical_start)
                    .and_then(|job_idx| highlighted_jobs.get_mut(job_idx))
                    .map(std::mem::take);
                render_editor_row(
                    ui,
                    line_idx,
                    highlighted_job,
                    &bracket_colors,
                    &row_context,
                    pending_actions,
                );
            }
        },
    );
}

fn for_visible_physical_line_batch(
    rows: Range<usize>,
    visible_line_count: usize,
    buffer_line_count: usize,
    folding: bool,
    visible_line_indices: &[usize],
    mut visit: impl FnMut(VisiblePhysicalLineBatch),
) {
    let Some(visible_rows) = bounded_visible_rows(
        rows,
        visible_line_count,
        buffer_line_count,
        folding,
        visible_line_indices,
    ) else {
        return;
    };

    if !folding || visible_line_indices.is_empty() {
        visit(VisiblePhysicalLineBatch {
            physical_lines: visible_rows.clone(),
            visible_rows,
        });
        return;
    }

    let hidden_line_budget = folded_batch_hidden_line_budget(&visible_rows);
    let mut hidden_lines_in_batch = 0usize;
    let mut current: Option<VisiblePhysicalLineBatch> = None;

    for visible_row in visible_rows {
        let physical_line = visible_line_indices[visible_row];
        let visible_end = visible_row.saturating_add(1);
        let physical_end = physical_line.saturating_add(1);
        let next = VisiblePhysicalLineBatch {
            visible_rows: visible_row..visible_end,
            physical_lines: physical_line..physical_end,
        };

        if current.is_none() {
            current = Some(next);
            continue;
        }

        let merge_hidden_gap = current.as_ref().and_then(|run| {
            if run.physical_lines.end == physical_line {
                Some(0)
            } else if run.physical_lines.end < physical_line {
                let hidden_gap = physical_line - run.physical_lines.end;
                hidden_lines_in_batch
                    .saturating_add(hidden_gap)
                    .le(&hidden_line_budget)
                    .then_some(hidden_gap)
            } else {
                None
            }
        });

        if let Some(hidden_gap) = merge_hidden_gap {
            if let Some(run) = current.as_mut() {
                run.visible_rows.end = visible_end;
                run.physical_lines.end = physical_end;
            }
            hidden_lines_in_batch = hidden_lines_in_batch.saturating_add(hidden_gap);
        } else if let Some(run) = current.replace(next) {
            visit(run);
            hidden_lines_in_batch = 0;
        }
    }

    if let Some(run) = current {
        visit(run);
    }
}

fn bounded_visible_rows(
    rows: Range<usize>,
    visible_line_count: usize,
    buffer_line_count: usize,
    folding: bool,
    visible_line_indices: &[usize],
) -> Option<Range<usize>> {
    if buffer_line_count == 0 {
        return None;
    }

    let visible_line_count = if folding && !visible_line_indices.is_empty() {
        let in_bounds_index_count =
            visible_line_indices.partition_point(|line_idx| *line_idx < buffer_line_count);
        visible_line_count.min(in_bounds_index_count)
    } else {
        visible_line_count.min(buffer_line_count)
    };
    let visible_start = rows.start.min(visible_line_count);
    let visible_end = rows.end.min(visible_line_count);
    (visible_start < visible_end).then_some(visible_start..visible_end)
}

fn folded_batch_hidden_line_budget(visible_rows: &Range<usize>) -> usize {
    visible_rows
        .end
        .saturating_sub(visible_rows.start)
        .min(MAX_BATCHED_FOLDED_HIDDEN_LINES)
}

fn physical_line_for_visible_row(
    visible_row: usize,
    folding: bool,
    visible_line_indices: &[usize],
) -> usize {
    if folding && !visible_line_indices.is_empty() {
        visible_line_indices[visible_row]
    } else {
        visible_row
    }
}

#[cfg(test)]
fn visible_physical_line_batches(
    rows: Range<usize>,
    visible_line_count: usize,
    folding: bool,
    visible_line_indices: &[usize],
) -> Vec<VisiblePhysicalLineBatch> {
    let mut batches = Vec::new();
    for_visible_physical_line_batch(
        rows,
        visible_line_count,
        inferred_buffer_line_count(visible_line_count, folding, visible_line_indices),
        folding,
        visible_line_indices,
        |run| {
            batches.push(run);
        },
    );
    batches
}

#[cfg(test)]
fn visible_physical_line_batches_for_buffer(
    rows: Range<usize>,
    visible_line_count: usize,
    buffer_line_count: usize,
    folding: bool,
    visible_line_indices: &[usize],
) -> Vec<VisiblePhysicalLineBatch> {
    let mut batches = Vec::new();
    for_visible_physical_line_batch(
        rows,
        visible_line_count,
        buffer_line_count,
        folding,
        visible_line_indices,
        |run| {
            batches.push(run);
        },
    );
    batches
}

#[cfg(test)]
fn physical_lines_for_visible_rows(
    rows: Range<usize>,
    visible_line_count: usize,
    folding: bool,
    visible_line_indices: &[usize],
) -> Vec<usize> {
    let mut lines = Vec::new();
    for_visible_physical_line_batch(
        rows,
        visible_line_count,
        inferred_buffer_line_count(visible_line_count, folding, visible_line_indices),
        folding,
        visible_line_indices,
        |run| {
            lines.extend(run.visible_rows.map(|visible_row| {
                physical_line_for_visible_row(visible_row, folding, visible_line_indices)
            }));
        },
    );
    lines
}

#[cfg(test)]
fn inferred_buffer_line_count(
    visible_line_count: usize,
    folding: bool,
    visible_line_indices: &[usize],
) -> usize {
    if folding && !visible_line_indices.is_empty() {
        visible_line_indices
            .iter()
            .copied()
            .max()
            .map(|line_idx| line_idx.saturating_add(1))
            .unwrap_or(visible_line_count)
    } else {
        visible_line_count
    }
}

#[cfg(test)]
mod tests {
    use super::{
        VisiblePhysicalLineBatch, physical_lines_for_visible_rows, visible_physical_line_batches,
        visible_physical_line_batches_for_buffer,
    };

    #[test]
    fn visible_physical_line_batches_bridge_small_fold_gaps() {
        let visible_lines = [0, 1, 4, 5, 7];

        let batches = visible_physical_line_batches(
            0..visible_lines.len(),
            visible_lines.len(),
            true,
            &visible_lines,
        );

        assert_eq!(
            batches,
            vec![VisiblePhysicalLineBatch {
                visible_rows: 0..5,
                physical_lines: 0..8,
            }]
        );
    }

    #[test]
    fn visible_physical_line_batches_preserve_folded_row_mapping() {
        let visible_lines = [0, 1, 4, 5, 7];

        let lines = physical_lines_for_visible_rows(
            0..visible_lines.len(),
            visible_lines.len(),
            true,
            &visible_lines,
        );

        assert_eq!(lines, visible_lines.to_vec());
    }

    #[test]
    fn visible_physical_line_batches_split_across_large_fold_gaps() {
        let visible_lines = [0, 1, 100, 101, 104];

        let batches = visible_physical_line_batches(
            0..visible_lines.len(),
            visible_lines.len(),
            true,
            &visible_lines,
        );

        assert_eq!(
            batches,
            vec![
                VisiblePhysicalLineBatch {
                    visible_rows: 0..2,
                    physical_lines: 0..2,
                },
                VisiblePhysicalLineBatch {
                    visible_rows: 2..5,
                    physical_lines: 100..105,
                },
            ]
        );
    }

    #[test]
    fn visible_physical_line_batches_keep_contiguous_rows_together() {
        let visible_lines = [10, 11, 12, 13];

        let batches =
            visible_physical_line_batches(1..3, visible_lines.len(), true, &visible_lines);

        assert_eq!(
            batches,
            vec![VisiblePhysicalLineBatch {
                visible_rows: 1..3,
                physical_lines: 11..13,
            }]
        );
    }

    #[test]
    fn visible_physical_line_batches_emit_dense_rows_without_fold_walk() {
        let batches = visible_physical_line_batches(8..15, 100, false, &[]);

        assert_eq!(
            batches,
            vec![VisiblePhysicalLineBatch {
                visible_rows: 8..15,
                physical_lines: 8..15,
            }]
        );
    }

    #[test]
    fn visible_physical_line_batches_clamp_dense_rows_to_visible_line_count() {
        let batches = visible_physical_line_batches(98..105, 100, false, &[]);

        assert_eq!(
            batches,
            vec![VisiblePhysicalLineBatch {
                visible_rows: 98..100,
                physical_lines: 98..100,
            }]
        );
    }

    #[test]
    fn visible_physical_line_batches_ignore_indices_when_folding_is_inactive() {
        let visible_lines = [10, 11, 12, 13];

        let batches =
            visible_physical_line_batches(1..3, visible_lines.len(), false, &visible_lines);

        assert_eq!(
            batches,
            vec![VisiblePhysicalLineBatch {
                visible_rows: 1..3,
                physical_lines: 1..3,
            }]
        );
    }

    #[test]
    fn visible_physical_line_batches_clamp_folded_rows_to_index_table() {
        let visible_lines = [4, 5];

        let batches = visible_physical_line_batches(0..5, 5, true, &visible_lines);

        assert_eq!(
            batches,
            vec![VisiblePhysicalLineBatch {
                visible_rows: 0..2,
                physical_lines: 4..6,
            }]
        );
    }

    #[test]
    fn visible_physical_line_batches_clamp_dense_rows_to_buffer_lines() {
        let batches = visible_physical_line_batches_for_buffer(2..8, 10, 5, false, &[]);

        assert_eq!(
            batches,
            vec![VisiblePhysicalLineBatch {
                visible_rows: 2..5,
                physical_lines: 2..5,
            }]
        );
    }

    #[test]
    fn visible_physical_line_batches_drop_stale_folded_index_tail() {
        let visible_lines = [0, 1, 4, 99];

        let batches = visible_physical_line_batches_for_buffer(
            0..visible_lines.len(),
            4,
            5,
            true,
            &visible_lines,
        );

        assert_eq!(
            batches,
            vec![VisiblePhysicalLineBatch {
                visible_rows: 0..3,
                physical_lines: 0..5,
            }]
        );
    }
}
