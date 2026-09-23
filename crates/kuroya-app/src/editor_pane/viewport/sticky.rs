use crate::{
    editor_bracket_overlay_cache::EditorBracketOverlayCache,
    editor_pane_data::EditorPaneData,
    editor_pane_rows::EditorRowContext,
    editor_row_paint::{active_indent_guide_column_for_buffer, paint_editor_row},
    syntax::SyntaxHighlighter,
};
use eframe::egui::{self, Rect, Stroke, pos2, vec2};
use kuroya_core::{LspFoldingRange, TextBuffer};

pub(super) fn first_visible_row_from_scroll(scroll_offset: f32, row_height: f32) -> usize {
    if !scroll_offset.is_finite() || !row_height.is_finite() || row_height <= 0.0 {
        return 0;
    }

    sticky_scroll_bounded_usize((scroll_offset.max(0.0) as f64 / row_height as f64).floor())
}

pub(super) fn sticky_scroll_max_visible_line_count(
    viewport_height: f32,
    row_height: f32,
    configured_max_line_count: usize,
) -> usize {
    if configured_max_line_count == 0
        || !viewport_height.is_finite()
        || !row_height.is_finite()
        || viewport_height <= 0.0
        || row_height <= 0.0
    {
        return 0;
    }

    let visible_rows =
        sticky_scroll_bounded_usize((viewport_height as f64 / row_height as f64).floor());
    configured_max_line_count.min(visible_rows.saturating_sub(1))
}

#[cfg(test)]
fn sticky_scroll_line(
    visible_line_indices: &[usize],
    visible_line_count: usize,
    folding_ranges: &[LspFoldingRange],
    first_visible_row: usize,
) -> Option<usize> {
    sticky_scroll_lines(
        visible_line_indices,
        visible_line_count,
        folding_ranges,
        first_visible_row,
        1,
    )
    .into_iter()
    .next()
}

pub(super) fn sticky_scroll_lines(
    visible_line_indices: &[usize],
    visible_line_count: usize,
    folding_ranges: &[LspFoldingRange],
    first_visible_row: usize,
    max_line_count: usize,
) -> Vec<usize> {
    if max_line_count == 0 {
        return Vec::new();
    }

    let Some(top_line_idx) =
        visible_line_at_row(visible_line_indices, visible_line_count, first_visible_row)
    else {
        return Vec::new();
    };
    let Some(top_line) = top_line_idx.checked_add(1) else {
        return Vec::new();
    };
    if top_line <= 1 {
        return Vec::new();
    }

    let mut lines: Vec<&LspFoldingRange> =
        Vec::with_capacity(max_line_count.min(folding_ranges.len()));
    let active_range_end = folding_ranges.partition_point(|range| range.start_line < top_line);
    for range in &folding_ranges[..active_range_end] {
        if !sticky_scroll_range_active(range, top_line, visible_line_indices, visible_line_count) {
            continue;
        }
        let insert_at = lines
            .iter()
            .position(|candidate| sticky_scroll_range_precedes(range, candidate))
            .unwrap_or(lines.len());
        if insert_at < max_line_count {
            lines.insert(insert_at, range);
            lines.truncate(max_line_count);
        }
    }

    let mut sticky_lines = Vec::with_capacity(lines.len());
    sticky_lines.extend(lines.into_iter().rev().map(|range| range.start_line - 1));
    sticky_lines
}

fn sticky_scroll_range_active(
    range: &LspFoldingRange,
    top_line: usize,
    visible_line_indices: &[usize],
    visible_line_count: usize,
) -> bool {
    range.start_line > 0
        && range.start_line < top_line
        && top_line <= range.end_line
        && visible_line_contains(
            visible_line_indices,
            visible_line_count,
            range.start_line - 1,
        )
}

fn visible_line_at_row(
    visible_line_indices: &[usize],
    visible_line_count: usize,
    row: usize,
) -> Option<usize> {
    if visible_line_indices.is_empty() {
        (row < visible_line_count).then_some(row)
    } else {
        visible_line_indices.get(row).copied()
    }
}

fn visible_line_contains(
    visible_line_indices: &[usize],
    visible_line_count: usize,
    line_idx: usize,
) -> bool {
    if visible_line_indices.is_empty() {
        line_idx < visible_line_count
    } else {
        visible_line_indices.binary_search(&line_idx).is_ok()
    }
}

fn sticky_scroll_range_precedes(range: &LspFoldingRange, candidate: &LspFoldingRange) -> bool {
    range
        .start_line
        .cmp(&candidate.start_line)
        .reverse()
        .then_with(|| {
            range
                .end_line
                .saturating_sub(range.start_line)
                .cmp(&candidate.end_line.saturating_sub(candidate.start_line))
        })
        .is_lt()
}

pub(super) fn paint_sticky_scroll_row(
    ui: &mut egui::Ui,
    scroll_rect: Rect,
    row_width: f32,
    buffer: &TextBuffer,
    highlighter: &mut SyntaxHighlighter,
    bracket_overlay_cache: &mut EditorBracketOverlayCache,
    data: &EditorPaneData,
    background_image_active: bool,
    active_find_match: usize,
    line_idx: usize,
    sticky_row_index: usize,
    horizontal_scroll_offset: f32,
) {
    let line_count = buffer.len_lines();
    if line_idx >= line_count
        || !sticky_scroll_paint_geometry_valid(scroll_rect, row_width, data.row_height)
    {
        return;
    }

    let row_top = sticky_scroll_row_top(scroll_rect.top(), data.row_height, sticky_row_index);
    let background_rect = Rect::from_min_size(
        pos2(scroll_rect.left(), row_top),
        vec2(scroll_rect.width(), data.row_height),
    );
    let row_left = sticky_scroll_row_content_left(
        scroll_rect.left(),
        horizontal_scroll_offset,
        data.sticky_scroll_scroll_with_editor,
    );
    let row_rect = Rect::from_min_size(pos2(row_left, row_top), vec2(row_width, data.row_height));
    let visuals = ui.visuals();
    ui.painter().rect_filled(
        background_rect,
        0.0,
        crate::editor_row_paint::editor_sticky_scroll_fill(
            visuals.faint_bg_color,
            background_image_active,
        ),
    );
    ui.painter().line_segment(
        [
            pos2(background_rect.left(), background_rect.bottom() - 1.0),
            pos2(background_rect.right(), background_rect.bottom() - 1.0),
        ],
        Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color),
    );

    let actual_end = line_idx.saturating_add(1).min(line_count);
    let mut highlighted_jobs = highlighter.layout_visible(
        buffer,
        data.font_size,
        data.tab_width,
        line_idx..actual_end,
        data.syntax_highlighting,
        visuals.text_color(),
        data.stop_rendering_line_after,
    );
    let bracket_colors = if data.bracket_pair_colorization {
        bracket_overlay_cache.bracket_colors_for_lines(
            buffer,
            line_idx,
            actual_end.saturating_sub(line_idx),
            data.bracket_pair_colorization_independent_color_pool_per_bracket_type,
        )
    } else {
        Vec::new()
    };
    let active_indent_guide_column = (data.indent_guides
        && data.highlight_active_indentation.visible(data.focused))
    .then(|| active_indent_guide_column_for_buffer(buffer, data.tab_width))
    .flatten();
    let row_context = EditorRowContext {
        buffer,
        row_height: data.row_height,
        row_left_inset: 0.0,
        row_width,
        gutter_width: data.gutter_width,
        char_width: data.char_width,
        font_size: data.font_size,
        text_color: visuals.text_color(),
        weak_text_color: visuals.weak_text_color(),
        background_image_active,
        selection_bg_fill: crate::editor_row_paint::editor_selection_fill(
            data.selection_bg_fill,
            background_image_active,
        ),
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
        ime_output_enabled: false,
        accessibility_enabled: false,
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
        ime_preedit: None,
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

    paint_editor_row(
        ui,
        row_rect,
        line_idx,
        highlighted_jobs.get_mut(0).map(std::mem::take),
        None,
        &bracket_colors,
        &row_context,
        false,
    );
}

fn sticky_scroll_row_content_left(
    scroll_left: f32,
    horizontal_scroll_offset: f32,
    scroll_with_editor: bool,
) -> f32 {
    if !scroll_left.is_finite() {
        return 0.0;
    }
    if scroll_with_editor {
        sticky_scroll_finite_or_zero(
            scroll_left - sticky_scroll_non_negative_finite(horizontal_scroll_offset),
        )
    } else {
        scroll_left
    }
}

fn sticky_scroll_row_top(scroll_top: f32, row_height: f32, sticky_row_index: usize) -> f32 {
    let scroll_top = sticky_scroll_finite_or_zero(scroll_top);
    let offset = sticky_scroll_non_negative_finite(row_height) as f64 * sticky_row_index as f64;
    sticky_scroll_f64_to_f32(scroll_top as f64 + offset)
}

fn sticky_scroll_paint_geometry_valid(scroll_rect: Rect, row_width: f32, row_height: f32) -> bool {
    scroll_rect.left().is_finite()
        && scroll_rect.top().is_finite()
        && scroll_rect.width().is_finite()
        && scroll_rect.width() > 0.0
        && row_width.is_finite()
        && row_width > 0.0
        && row_height.is_finite()
        && row_height > 0.0
}

fn sticky_scroll_bounded_usize(value: f64) -> usize {
    if !value.is_finite() || value <= 0.0 {
        0
    } else if value >= usize::MAX as f64 {
        usize::MAX
    } else {
        value as usize
    }
}

fn sticky_scroll_finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn sticky_scroll_non_negative_finite(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

fn sticky_scroll_f64_to_f32(value: f64) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    if value > f32::MAX as f64 {
        f32::MAX
    } else if value < f32::MIN as f64 {
        f32::MIN
    } else {
        value as f32
    }
}

#[cfg(test)]
mod tests {
    use super::{
        first_visible_row_from_scroll, sticky_scroll_line, sticky_scroll_lines,
        sticky_scroll_max_visible_line_count, sticky_scroll_paint_geometry_valid,
        sticky_scroll_row_content_left, sticky_scroll_row_top,
    };
    use eframe::egui::{Rect, pos2, vec2};
    use kuroya_core::LspFoldingRange;

    fn range(start_line: usize, end_line: usize) -> LspFoldingRange {
        LspFoldingRange {
            start_line,
            start_column: None,
            end_line,
            end_column: None,
            kind: None,
        }
    }

    #[test]
    fn first_visible_row_tracks_scroll_offset_safely() {
        assert_eq!(first_visible_row_from_scroll(0.0, 20.0), 0);
        assert_eq!(first_visible_row_from_scroll(39.0, 20.0), 1);
        assert_eq!(first_visible_row_from_scroll(-10.0, 20.0), 0);
        assert_eq!(first_visible_row_from_scroll(f32::NAN, 20.0), 0);
        assert_eq!(first_visible_row_from_scroll(40.0, f32::NAN), 0);
        assert_eq!(first_visible_row_from_scroll(40.0, f32::INFINITY), 0);
        assert_eq!(first_visible_row_from_scroll(40.0, 0.0), 0);
        assert_eq!(
            first_visible_row_from_scroll(f32::MAX, f32::MIN_POSITIVE),
            usize::MAX
        );
    }

    #[test]
    fn sticky_scroll_max_visible_line_count_leaves_room_for_editor_rows() {
        assert_eq!(sticky_scroll_max_visible_line_count(100.0, 20.0, 10), 4);
        assert_eq!(sticky_scroll_max_visible_line_count(100.0, 20.0, 2), 2);
        assert_eq!(sticky_scroll_max_visible_line_count(39.0, 20.0, 10), 0);
        assert_eq!(sticky_scroll_max_visible_line_count(40.0, 20.0, 10), 1);
        assert_eq!(sticky_scroll_max_visible_line_count(f32::NAN, 20.0, 10), 0);
        assert_eq!(sticky_scroll_max_visible_line_count(100.0, 0.0, 10), 0);
        assert_eq!(sticky_scroll_max_visible_line_count(100.0, 20.0, 0), 0);
        assert_eq!(
            sticky_scroll_max_visible_line_count(f32::MAX, f32::MIN_POSITIVE, 10),
            10
        );
    }

    #[test]
    fn sticky_scroll_line_uses_innermost_visible_folding_range() {
        let visible = (0..12).collect::<Vec<_>>();
        let ranges = [range(1, 12), range(3, 10), range(3, 6), range(7, 9)];

        assert_eq!(
            sticky_scroll_line(&visible, visible.len(), &ranges, 0),
            None
        );
        assert_eq!(
            sticky_scroll_line(&visible, visible.len(), &ranges, 2),
            Some(0)
        );
        assert_eq!(
            sticky_scroll_line(&visible, visible.len(), &ranges, 4),
            Some(2)
        );
        assert_eq!(
            sticky_scroll_line(&visible, visible.len(), &ranges, 7),
            Some(6)
        );
        assert_eq!(
            sticky_scroll_line(&visible, visible.len(), &ranges, 11),
            Some(0)
        );
    }

    #[test]
    fn sticky_scroll_lines_keep_innermost_scopes_within_line_limit() {
        let visible = (0..14).collect::<Vec<_>>();
        let ranges = [range(1, 14), range(3, 12), range(5, 10), range(7, 9)];

        assert!(sticky_scroll_lines(&visible, visible.len(), &ranges, 8, 0).is_empty());
        assert_eq!(
            sticky_scroll_lines(&visible, visible.len(), &ranges, 8, 1),
            vec![6]
        );
        assert_eq!(
            sticky_scroll_lines(&visible, visible.len(), &ranges, 8, 2),
            vec![4, 6]
        );
        assert_eq!(
            sticky_scroll_lines(&visible, visible.len(), &ranges, 8, 3),
            vec![2, 4, 6]
        );
        assert_eq!(
            sticky_scroll_lines(&visible, visible.len(), &ranges, 8, 8),
            vec![0, 2, 4, 6]
        );
    }

    #[test]
    fn sticky_scroll_lines_use_unfolded_rows_without_visible_index_map() {
        let ranges = [range(1, 12), range(3, 10)];

        assert_eq!(sticky_scroll_lines(&[], 12, &ranges, 4, 2), vec![0, 2]);
    }

    #[test]
    fn sticky_scroll_lines_ignore_stale_visible_rows_that_overflow_line_numbers() {
        let visible = [usize::MAX];
        let ranges = [range(1, usize::MAX)];

        assert!(sticky_scroll_lines(&visible, visible.len(), &ranges, 0, 2).is_empty());
    }

    #[test]
    fn sticky_scroll_line_skips_hidden_or_inactive_ranges() {
        let visible = [0, 4, 5, 9, 10];
        let ranges = [range(2, 6), range(5, 8), range(10, 12)];

        assert_eq!(
            sticky_scroll_line(&visible, visible.len(), &ranges, 2),
            Some(4)
        );
        assert_eq!(
            sticky_scroll_line(&visible, visible.len(), &ranges, 3),
            None
        );
        assert_eq!(
            sticky_scroll_line(&visible, visible.len(), &ranges, 4),
            Some(9)
        );
        assert_eq!(
            sticky_scroll_line(&visible, visible.len(), &ranges, 9),
            None
        );
    }

    #[test]
    fn sticky_scroll_content_can_follow_horizontal_editor_scroll() {
        assert_eq!(sticky_scroll_row_content_left(20.0, 48.0, true), -28.0);
        assert_eq!(sticky_scroll_row_content_left(20.0, 48.0, false), 20.0);
        assert_eq!(sticky_scroll_row_content_left(20.0, -10.0, true), 20.0);
        assert_eq!(sticky_scroll_row_content_left(f32::NAN, 48.0, true), 0.0);
        assert_eq!(
            sticky_scroll_row_content_left(20.0, f32::INFINITY, true),
            20.0
        );
    }

    #[test]
    fn sticky_scroll_rows_stack_by_row_height() {
        assert_eq!(sticky_scroll_row_top(10.0, 18.0, 0), 10.0);
        assert_eq!(sticky_scroll_row_top(10.0, 18.0, 1), 28.0);
        assert_eq!(sticky_scroll_row_top(10.0, 18.0, 3), 64.0);
        assert_eq!(sticky_scroll_row_top(10.0, -18.0, 3), 10.0);
        assert_eq!(sticky_scroll_row_top(f32::NAN, f32::NAN, 3), 0.0);
        assert_eq!(sticky_scroll_row_top(10.0, f32::MAX, usize::MAX), f32::MAX);
    }

    #[test]
    fn sticky_scroll_paint_geometry_rejects_non_finite_dimensions() {
        let rect = Rect::from_min_size(pos2(0.0, 10.0), vec2(80.0, 200.0));

        assert!(sticky_scroll_paint_geometry_valid(rect, 120.0, 18.0));
        assert!(!sticky_scroll_paint_geometry_valid(rect, f32::NAN, 18.0));
        assert!(!sticky_scroll_paint_geometry_valid(
            rect,
            120.0,
            f32::INFINITY
        ));
        assert!(!sticky_scroll_paint_geometry_valid(
            Rect::from_min_size(pos2(f32::NAN, 10.0), vec2(80.0, 200.0)),
            120.0,
            18.0
        ));
    }
}
