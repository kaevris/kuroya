use crate::{
    completion_preview::CompletionInlinePreview,
    editor_context_menu::render_editor_context_menu,
    editor_focus_runtime::editor_click_drag_sense_for_tab_index,
    editor_input::EditorContextAction,
    editor_pane_actions::{PendingCodeLensCommand, PendingEditorPaneActions},
    editor_pane_support::{DocumentHighlightSpan, SemanticTokenSpan},
    editor_row_gutter::{
        code_action_marker_hit, final_newline_line_number_visible, line_change_marker_hit,
        line_number_label,
    },
    editor_row_overlays::code_lens_command_at_pointer,
    editor_row_paint::{editor_row_wrap_width, limit_layout_job_line_rendering, paint_editor_row},
    editor_row_render_cache::{EditorRowRenderKey, GpuRowRenderScope},
    editor_text_geometry::{char_offset_for_visual_column, visual_width},
    folding::{FoldedRange, best_folding_range_starting_at, folded_range_starting_at},
    source_control_blame_runtime::git_blame_editor_decoration_hover_text,
    syntax_tree_cache::TreeSitterInjection,
};
use eframe::egui::{
    self, Color32, FontFamily, FontId, PointerButton, Rect, WidgetInfo, pos2, vec2,
};
use kuroya_core::{
    DEFAULT_EDITOR_STOP_RENDERING_LINE_AFTER, DiagnosticSeverity, EditorBracketPairGuideMode,
    EditorColorDecoratorsActivatedOn, EditorCursorSmoothCaretAnimation, EditorCursorStyle,
    EditorDefaultColorDecorators, EditorExperimentalWhitespaceRendering, EditorLightbulbMode,
    EditorLineNumbers, EditorMatchBrackets, EditorMouseMiddleClickAction, EditorMouseStyle,
    EditorMultiCursorModifier, EditorRenderFinalNewline, EditorRenderLineHighlight,
    EditorRenderWhitespace, EditorShowFoldingControls, EditorWordWrap, GitBlameLine,
    GitChangeStage, GitLineChangeKind, LspCodeLens, LspFoldingRange, LspInlayHint, MergeConflict,
    ScmDiffDecorationsGutterAction, ScmDiffDecorationsGutterPattern,
    ScmDiffDecorationsGutterVisibility, Selection, TextBuffer,
    buffer::{BracketColor, BracketPairGuide, CursorPosition},
    clamp_editor_word_wrap_column, editor_stop_rendering_line_after_limit,
    merge_conflict_line_kind,
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Write as _,
    ops::Range,
};

#[derive(Debug, Clone, Copy)]
struct PreparedEditorRow {
    rect: Rect,
    row_height: f32,
    gutter_width: f32,
    text_left: f32,
}

impl PreparedEditorRow {
    fn text_pos(self, y_offset: f32) -> egui::Pos2 {
        pos2(self.text_left, self.rect.top() + y_offset)
    }

    fn text_hit(self, pos: egui::Pos2) -> bool {
        pos.x.is_finite() && pos.y.is_finite() && self.rect.contains(pos) && pos.x >= self.text_left
    }

    fn visual_column_at_pointer(self, pos: egui::Pos2, char_width: f32) -> usize {
        if !char_width.is_finite() || char_width <= 0.0 {
            return 0;
        }
        let relative_x = pos.x - self.text_left;
        if !relative_x.is_finite() || relative_x <= 0.0 {
            return 0;
        }
        (relative_x / char_width).round() as usize
    }
}

#[derive(Debug, Clone, Copy)]
struct EditorRowPointerTextPosition {
    char_idx: usize,
    column: usize,
}

pub(crate) struct EditorRowContext<'a> {
    pub(crate) buffer: &'a TextBuffer,
    pub(crate) row_height: f32,
    pub(crate) row_left_inset: f32,
    pub(crate) row_width: f32,
    pub(crate) gutter_width: f32,
    pub(crate) char_width: f32,
    pub(crate) font_size: f32,
    pub(crate) text_color: Color32,
    pub(crate) weak_text_color: Color32,
    pub(crate) background_image_active: bool,
    pub(crate) selection_bg_fill: Color32,
    pub(crate) warn_fg_color: Color32,
    pub(crate) line_numbers: EditorLineNumbers,
    pub(crate) select_on_line_numbers: bool,
    pub(crate) render_whitespace: EditorRenderWhitespace,
    pub(crate) experimental_whitespace_rendering: EditorExperimentalWhitespaceRendering,
    pub(crate) render_final_newline: EditorRenderFinalNewline,
    pub(crate) render_control_characters: bool,
    pub(crate) unicode_highlight_ambiguous_characters: bool,
    pub(crate) unicode_highlight_invisible_characters: bool,
    pub(crate) unicode_highlight_non_basic_ascii: bool,
    pub(crate) unicode_highlight_allowed_characters: &'a BTreeSet<char>,
    pub(crate) unicode_highlight_allowed_locales: &'a BTreeSet<String>,
    pub(crate) render_line_highlight: EditorRenderLineHighlight,
    pub(crate) render_line_highlight_only_when_focus: bool,
    pub(crate) word_wrap: EditorWordWrap,
    pub(crate) word_wrap_column: usize,
    pub(crate) stop_rendering_line_after: i64,
    pub(crate) bracket_pair_colorization: bool,
    pub(crate) bracket_pair_guides: EditorBracketPairGuideMode,
    pub(crate) bracket_pair_guides_horizontal: EditorBracketPairGuideMode,
    pub(crate) highlight_active_bracket_pair: bool,
    pub(crate) match_brackets: EditorMatchBrackets,
    pub(crate) folding: bool,
    pub(crate) folding_highlight: bool,
    pub(crate) show_folding_controls: EditorShowFoldingControls,
    pub(crate) unfold_on_click_after_end_of_line: bool,
    pub(crate) contextmenu: bool,
    pub(crate) focused: bool,
    pub(crate) multi_cursor_modifier: EditorMultiCursorModifier,
    pub(crate) double_click_selects_block: bool,
    pub(crate) drag_and_drop: bool,
    pub(crate) selection_clipboard: bool,
    pub(crate) mouse_middle_click_action: EditorMouseMiddleClickAction,
    pub(crate) mouse_style: EditorMouseStyle,
    pub(crate) glyph_margin: bool,
    pub(crate) lightbulb: EditorLightbulbMode,
    pub(crate) indent_guides: bool,
    pub(crate) active_indent_guide_column: Option<usize>,
    pub(crate) ruler_column: usize,
    pub(crate) rounded_selection: bool,
    pub(crate) color_decorators: bool,
    pub(crate) color_decorators_activated_on: EditorColorDecoratorsActivatedOn,
    pub(crate) color_decorators_limit: usize,
    pub(crate) default_color_decorators: EditorDefaultColorDecorators,
    pub(crate) tab_width: usize,
    pub(crate) cursor_smooth_caret_animation: EditorCursorSmoothCaretAnimation,
    pub(crate) cursor_style: EditorCursorStyle,
    pub(crate) cursor_blinking: bool,
    pub(crate) cursor_width: f32,
    pub(crate) cursor_height: usize,
    pub(crate) ime_output_enabled: bool,
    pub(crate) accessibility_enabled: bool,
    pub(crate) accessibility_page_size: usize,
    pub(crate) aria_label: &'a str,
    pub(crate) aria_required: bool,
    pub(crate) render_rich_screen_reader_content: bool,
    pub(crate) tab_index: i64,
    pub(crate) diff_lines: &'a BTreeMap<usize, GitLineChangeKind>,
    pub(crate) cursor_positions: &'a [CursorPosition],
    pub(crate) selections: &'a [Selection],
    pub(crate) find_matches: &'a [Range<usize>],
    pub(crate) active_find_match: usize,
    pub(crate) document_highlight_ranges: &'a [DocumentHighlightSpan],
    pub(crate) semantic_token_ranges: &'a [SemanticTokenSpan],
    pub(crate) syntax_injections: &'a [TreeSitterInjection],
    pub(crate) diagnostics_by_line: &'a HashMap<usize, DiagnosticSeverity>,
    pub(crate) diagnostic_messages: &'a HashMap<usize, String>,
    pub(crate) diagnostic_tag_spans: &'a [crate::editor_pane_support::DiagnosticTagSpan],
    pub(crate) git_blame_editor_decoration_enabled: bool,
    pub(crate) git_blame_editor_decoration_disable_hover: bool,
    pub(crate) git_blame_editor_decoration_template: &'a str,
    pub(crate) git_blame_lines: &'a [GitBlameLine],
    pub(crate) folding_ranges: &'a [LspFoldingRange],
    pub(crate) inlay_hints: &'a [LspInlayHint],
    pub(crate) inlay_hints_font_family: &'a str,
    pub(crate) inlay_hints_font_size: usize,
    pub(crate) inlay_hints_padding: bool,
    pub(crate) inlay_hints_maximum_length: usize,
    pub(crate) code_lenses: &'a [LspCodeLens],
    pub(crate) code_lens_font_family: &'a str,
    pub(crate) code_lens_font_size: usize,
    pub(crate) completion_preview: Option<&'a CompletionInlinePreview>,
    pub(crate) ime_preedit: Option<&'a str>,
    pub(crate) folded_ranges: &'a [FoldedRange],
    pub(crate) bracket_matches: &'a [(usize, usize)],
    pub(crate) active_bracket_pair_matches: &'a [(usize, usize)],
    pub(crate) bracket_pair_guide_ranges: &'a [BracketPairGuide],
    pub(crate) merge_conflicts: &'a [MergeConflict],
    pub(crate) diff_stage: Option<GitChangeStage>,
    pub(crate) diff_move_lines: &'a BTreeSet<usize>,
    pub(crate) diff_render_gutter_menu: bool,
    pub(crate) diff_render_indicators: bool,
    pub(crate) diff_render_margin_revert_icon: bool,
    pub(crate) diff_accessibility_verbose: bool,
    pub(crate) diff_experimental_show_empty_decorations: bool,
    pub(crate) show_scm_diff_gutter: bool,
    pub(crate) scm_diff_decorations_gutter_action: ScmDiffDecorationsGutterAction,
    pub(crate) scm_diff_decorations_gutter_visibility: ScmDiffDecorationsGutterVisibility,
    pub(crate) scm_diff_decorations_gutter_width: usize,
    pub(crate) scm_diff_decorations_gutter_pattern: ScmDiffDecorationsGutterPattern,
    pub(crate) staged_hunk_actions: bool,
    pub(crate) source_control_unstaged_actions: bool,
    pub(crate) source_control_staged_actions: bool,
    pub(crate) source_control_discard_actions: bool,
    pub(crate) source_control_path_actions: bool,
    pub(crate) compare_saved_actions: bool,
    pub(crate) compare_file_actions: bool,
    pub(crate) compare_with_selected_actions: bool,
    pub(crate) diff_base_file_actions: bool,
    pub(crate) diff_source_file_actions: bool,
    pub(crate) diff_patch_actions: bool,
    pub(crate) diff_refresh_actions: bool,
    pub(crate) diff_swap_actions: bool,
}

pub(crate) fn render_editor_row(
    ui: &mut egui::Ui,
    line_idx: usize,
    highlighted_job: Option<egui::text::LayoutJob>,
    bracket_colors: &[BracketColor],
    gpu_row_render: Option<&mut GpuRowRenderScope<'_>>,
    row: &EditorRowContext<'_>,
    pending_actions: &mut PendingEditorPaneActions,
) {
    if !editor_row_line_in_bounds(row.buffer, line_idx) {
        return;
    }

    let prepared_row = prepare_editor_row(ui.max_rect().left(), ui.next_widget_position().y, row);
    let rect = prepared_row.rect;
    let row_height = prepared_row.row_height;
    let gutter_width = prepared_row.gutter_width;
    let interaction_text_limit = editor_row_preparation_text_limit(row, prepared_row);
    let response = ui.allocate_rect(rect, editor_click_drag_sense_for_tab_index(row.tab_index));
    let row_hovered = response.hovered() || response.clicked();
    let mut interaction_text = None;
    if row.drag_and_drop
        && response.drag_started_by(PointerButton::Primary)
        && let Some(pos) = response.interact_pointer_pos()
        && prepared_row.text_hit(pos)
    {
        let line_text = cached_editor_row_interaction_text(&mut interaction_text, || {
            editor_row_interaction_text_with_limit(row.buffer, line_idx, interaction_text_limit)
        });
        pending_actions.drag_start_char_idx = Some(editor_row_char_index_at_pointer(
            row,
            prepared_row,
            line_idx,
            pos,
            line_text,
        ));
    }
    if row.drag_and_drop
        && response.drag_stopped_by(PointerButton::Primary)
        && let Some(pos) = response.interact_pointer_pos()
        && prepared_row.text_hit(pos)
    {
        let line_text = cached_editor_row_interaction_text(&mut interaction_text, || {
            editor_row_interaction_text_with_limit(row.buffer, line_idx, interaction_text_limit)
        });
        pending_actions.drag_drop_char_idx = Some(editor_row_char_index_at_pointer(
            row,
            prepared_row,
            line_idx,
            pos,
            line_text,
        ));
    }
    if response.middle_clicked()
        && let Some(pos) = response.interact_pointer_pos()
        && prepared_row.text_hit(pos)
    {
        let line_text = cached_editor_row_interaction_text(&mut interaction_text, || {
            editor_row_interaction_text_with_limit(row.buffer, line_idx, interaction_text_limit)
        });
        let text_position =
            editor_row_text_position_at_pointer(row, prepared_row, line_idx, pos, line_text);
        match editor_middle_click_action(
            row.mouse_middle_click_action,
            line_text,
            text_position.column,
        ) {
            EditorRowMiddleClickAction::None => {
                if row.selection_clipboard
                    && matches!(
                        row.mouse_middle_click_action,
                        EditorMouseMiddleClickAction::Default
                    )
                {
                    pending_actions.selection_clipboard_paste_char_idx =
                        Some(text_position.char_idx);
                }
            }
            EditorRowMiddleClickAction::AddCursor => {
                pending_actions.cursor = Some((text_position.char_idx, true, false));
            }
            EditorRowMiddleClickAction::OpenUrl(url) => {
                pending_actions.open_url = Some(url);
            }
        }
    } else if response.double_clicked()
        && row.double_click_selects_block
        && let Some(pos) = response.interact_pointer_pos()
    {
        if prepared_row.text_hit(pos) {
            let line_text = cached_editor_row_interaction_text(&mut interaction_text, || {
                editor_row_interaction_text_with_limit(row.buffer, line_idx, interaction_text_limit)
            });
            let char_idx =
                editor_row_char_index_at_pointer(row, prepared_row, line_idx, pos, line_text);
            if let Some(range) = row.buffer.bracket_block_selection_range_at(char_idx) {
                pending_actions.select_range = Some(range);
            } else {
                let (add_cursor, extend_selection) = ui.input(|input| {
                    (
                        multi_cursor_modifier_active(row.multi_cursor_modifier, input.modifiers),
                        input.modifiers.shift,
                    )
                });
                pending_actions.cursor = Some((char_idx, add_cursor, extend_selection));
            }
        } else {
            pending_actions.focus_editor = true;
        }
    } else if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let line_number = line_idx + 1;
        let line_text = cached_editor_row_interaction_text(&mut interaction_text, || {
            editor_row_interaction_text_with_limit(row.buffer, line_idx, interaction_text_limit)
        });
        let clicked_code_action_marker = code_action_marker_hit(
            rect,
            row_height,
            pos,
            row.lightbulb,
            row.glyph_margin,
            row.diagnostics_by_line.contains_key(&line_number),
            row_hovered,
            line_text,
        );
        let visible_change_kind = crate::editor_row_gutter::visible_line_change_kind(
            row.show_scm_diff_gutter,
            row.scm_diff_decorations_gutter_visibility,
            row_hovered,
            row.diff_lines.get(&line_number).copied(),
        );
        let source_control_gutter_action = line_change_marker_hit(
            rect,
            row_height,
            pos,
            row.glyph_margin,
            visible_change_kind,
            row.scm_diff_decorations_gutter_width,
            row.scm_diff_decorations_gutter_action,
        );
        let fold_marker_left = rect.left() + (gutter_width - 24.0).max(8.0);
        let folded_here = folded_range_starting_at(row.folded_ranges, line_number).is_some();
        let visual_text_width = visual_width(line_text, row.tab_width);
        let clicked_fold_marker = row.folding
            && pos.x >= fold_marker_left
            && pos.x <= rect.left() + gutter_width
            && (folded_here
                || best_folding_range_starting_at(row.folding_ranges, line_number).is_some());
        let clicked_folded_line_after_end = folded_line_after_end_click(
            row.unfold_on_click_after_end_of_line,
            folded_here,
            pos.x,
            rect.left() + gutter_width,
            visual_text_width,
            row.char_width,
        );
        let folding_marker_visible =
            folding_control_visible(row.show_folding_controls, row_hovered, folded_here);
        let diff_gutter_action = crate::editor_row_gutter::diff_gutter_action_hit(
            rect,
            row_height,
            gutter_width,
            pos,
            row.diff_stage,
            row.diff_render_gutter_menu,
            row.diff_render_margin_revert_icon,
            line_text,
            row_hovered,
        );
        if clicked_code_action_marker {
            pending_actions.cursor =
                Some((row.buffer.line_column_to_char(line_idx, 0), false, false));
            pending_actions.context_action = Some(EditorContextAction::CodeActions);
        } else if (clicked_fold_marker && folding_marker_visible) || clicked_folded_line_after_end {
            pending_actions.fold_toggle_line = Some(line_number);
        } else if let Some(action) = diff_gutter_action {
            pending_actions.cursor =
                Some((row.buffer.line_column_to_char(line_idx, 0), false, false));
            pending_actions.context_action = Some(diff_gutter_context_action(action));
        } else if source_control_gutter_action
            && let Some(action) = source_control_gutter_context_action(row)
        {
            pending_actions.cursor =
                Some((row.buffer.line_column_to_char(line_idx, 0), false, false));
            pending_actions.context_action = Some(action);
        } else if line_number_click_selects_line(rect, pos, line_idx, row) {
            pending_actions.select_line = Some(line_idx);
        } else if let Some(lens) = code_lens_command_at_pointer(
            ui,
            rect,
            prepared_row.text_pos(3.0),
            visual_text_width,
            line_idx,
            pos,
            row,
        ) {
            if let Some(command) = lens.command {
                pending_actions.code_lens_command = Some(PendingCodeLensCommand {
                    title: lens.title,
                    command,
                    arguments: lens.command_arguments,
                });
            }
        } else if prepared_row.text_hit(pos) {
            let char_idx =
                editor_row_char_index_at_pointer(row, prepared_row, line_idx, pos, line_text);
            let (add_cursor, extend_selection) = ui.input(|input| {
                (
                    multi_cursor_modifier_active(row.multi_cursor_modifier, input.modifiers),
                    input.modifiers.shift,
                )
            });
            pending_actions.cursor = Some((char_idx, add_cursor, extend_selection));
        } else {
            pending_actions.focus_editor = true;
        }
    }
    if response.hovered()
        && let Some(pos) = response.hover_pos()
        && prepared_row.text_hit(pos)
    {
        let line_text = cached_editor_row_interaction_text(&mut interaction_text, || {
            editor_row_interaction_text_with_limit(row.buffer, line_idx, interaction_text_limit)
        });
        pending_actions.hover_char_idx = Some(editor_row_char_index_at_pointer(
            row,
            prepared_row,
            line_idx,
            pos,
            line_text,
        ));
    }
    let merge_conflict_action_line =
        merge_conflict_line_kind(row.merge_conflicts, line_idx).map(|_| line_idx);
    let response = if let Some(cursor) = editor_mouse_cursor_icon(row.mouse_style) {
        response.on_hover_cursor(cursor)
    } else {
        response
    };
    let response = if row.git_blame_editor_decoration_enabled
        && row
            .cursor_positions
            .iter()
            .any(|cursor| cursor.line == line_idx)
        && let Some(tooltip) = git_blame_editor_decoration_hover_text(
            row.git_blame_lines,
            line_idx + 1,
            row.git_blame_editor_decoration_template,
            row.git_blame_editor_decoration_disable_hover,
        ) {
        response.on_hover_text(tooltip)
    } else {
        response
    };
    if row.contextmenu {
        response.context_menu(|ui| {
            render_editor_context_menu(
                ui,
                &mut pending_actions.context_action,
                merge_conflict_action_line,
                row.diff_stage.is_none() && !row.diff_lines.is_empty(),
                row.staged_hunk_actions,
                row.source_control_unstaged_actions,
                row.source_control_staged_actions,
                row.source_control_discard_actions,
                row.source_control_path_actions,
                row.compare_saved_actions,
                row.compare_file_actions,
                row.compare_with_selected_actions,
                row.diff_base_file_actions,
                row.diff_source_file_actions,
                row.diff_patch_actions,
                row.diff_refresh_actions,
                row.diff_swap_actions,
                row.diff_stage,
            )
        });
    }
    register_editor_row_accessibility(&response, line_idx, row);

    let prepared_galley = gpu_row_render.and_then(|scope| {
        let job = highlighted_job.as_ref()?;
        let wrap_max_width = editor_row_wrap_width(
            rect.width(),
            row.gutter_width,
            row.word_wrap,
            row.word_wrap_column,
            row.char_width,
        );
        let stop_rendering_line_after = row.stop_rendering_line_after;
        let key = EditorRowRenderKey::new(
            row.buffer.id(),
            row.buffer.version(),
            line_idx,
            wrap_max_width.to_bits(),
            &FontId::new(row.font_size, FontFamily::Monospace),
            scope.theme_revision,
            row.text_color,
            row.tab_width,
            stop_rendering_line_after,
        );
        Some(scope.cache.get_or_compute(key, || {
            let mut job = job.clone();
            limit_layout_job_line_rendering(&mut job, stop_rendering_line_after);
            job.wrap.max_width = wrap_max_width;
            ui.fonts_mut(|fonts| fonts.layout_job(job))
        }))
    });

    paint_editor_row(
        ui,
        rect,
        line_idx,
        highlighted_job,
        prepared_galley,
        bracket_colors,
        row,
        row_hovered,
    );
}

fn editor_row_char_index_at_pointer(
    row: &EditorRowContext<'_>,
    prepared_row: PreparedEditorRow,
    line_idx: usize,
    pos: egui::Pos2,
    line_text: &str,
) -> usize {
    editor_row_text_position_at_pointer(row, prepared_row, line_idx, pos, line_text).char_idx
}

fn editor_row_text_position_at_pointer(
    row: &EditorRowContext<'_>,
    prepared_row: PreparedEditorRow,
    line_idx: usize,
    pos: egui::Pos2,
    line_text: &str,
) -> EditorRowPointerTextPosition {
    let visual_col = prepared_row.visual_column_at_pointer(pos, row.char_width);
    let column = char_offset_for_visual_column(line_text, visual_col, row.tab_width);
    EditorRowPointerTextPosition {
        char_idx: row.buffer.line_column_to_char(line_idx, column),
        column,
    }
}

#[cfg(test)]
fn editor_row_visual_column_at_pointer(
    rect: Rect,
    pos: egui::Pos2,
    gutter_width: f32,
    char_width: f32,
) -> usize {
    prepared_editor_row_from_rect(rect, gutter_width).visual_column_at_pointer(pos, char_width)
}

#[cfg(test)]
fn editor_row_text_hit(rect: Rect, pos: egui::Pos2, gutter_width: f32) -> bool {
    prepared_editor_row_from_rect(rect, gutter_width).text_hit(pos)
}

fn prepare_editor_row(
    container_left: f32,
    row_top: f32,
    row: &EditorRowContext<'_>,
) -> PreparedEditorRow {
    prepare_editor_row_bounds(
        container_left,
        row_top,
        row.row_left_inset,
        row.row_width,
        row.row_height,
        row.gutter_width,
    )
}

fn prepare_editor_row_bounds(
    container_left: f32,
    row_top: f32,
    row_left_inset: f32,
    row_width: f32,
    row_height: f32,
    gutter_width: f32,
) -> PreparedEditorRow {
    let left = stable_finite_coordinate(container_left) + stable_nonnegative_extent(row_left_inset);
    let top = if row_top.is_finite() { row_top } else { 0.0 };
    let row_width = stable_nonnegative_extent(row_width);
    let row_height = stable_positive_extent(row_height, 1.0);
    let gutter_width = stable_nonnegative_extent(gutter_width).min(row_width);
    prepared_editor_row_from_parts(left, top, row_width, row_height, gutter_width)
}

#[cfg(test)]
fn prepared_editor_row_from_rect(rect: Rect, gutter_width: f32) -> PreparedEditorRow {
    let gutter_width =
        stable_nonnegative_extent(gutter_width).min(stable_nonnegative_extent(rect.width()));
    prepared_editor_row_from_parts(
        rect.left(),
        rect.top(),
        stable_nonnegative_extent(rect.width()),
        stable_positive_extent(rect.height(), 1.0),
        gutter_width,
    )
}

fn prepared_editor_row_from_parts(
    left: f32,
    top: f32,
    row_width: f32,
    row_height: f32,
    gutter_width: f32,
) -> PreparedEditorRow {
    let rect = Rect::from_min_size(pos2(left, top), vec2(row_width, row_height));
    PreparedEditorRow {
        rect,
        row_height,
        gutter_width,
        text_left: rect.left() + gutter_width,
    }
}

fn bounded_visible_editor_rows(rows: Range<usize>, visible_row_count: usize) -> Range<usize> {
    let visible_end = rows.end.min(visible_row_count);
    let visible_start = rows.start.min(visible_end);
    visible_start..visible_end
}

fn editor_row_line_in_bounds(buffer: &TextBuffer, line_idx: usize) -> bool {
    let bounded =
        bounded_visible_editor_rows(line_idx..line_idx.saturating_add(1), buffer.len_lines());
    bounded.start < bounded.end
}

fn stable_nonnegative_extent(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

fn stable_finite_coordinate(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn stable_positive_extent(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

pub(crate) fn folded_line_after_end_click(
    enabled: bool,
    folded_here: bool,
    pointer_x: f32,
    text_left: f32,
    visual_text_width: usize,
    char_width: f32,
) -> bool {
    enabled
        && folded_here
        && char_width.is_finite()
        && char_width > 0.0
        && pointer_x >= text_left + (visual_text_width as f32 * char_width)
}

fn cached_editor_row_interaction_text(
    cache: &mut Option<String>,
    build: impl FnOnce() -> String,
) -> &str {
    cache.get_or_insert_with(build).as_str()
}

fn editor_row_preparation_text_limit(
    row: &EditorRowContext<'_>,
    prepared_row: PreparedEditorRow,
) -> usize {
    let render_limit = editor_row_interaction_text_limit(row.stop_rendering_line_after);
    let Some(wrap_limit) = editor_row_wrap_interaction_text_limit(
        row.word_wrap,
        row.word_wrap_column,
        prepared_row.rect.width(),
        prepared_row.gutter_width,
        row.char_width,
    ) else {
        return render_limit;
    };
    render_limit.min(wrap_limit)
}

#[cfg(test)]
fn editor_row_interaction_text(
    buffer: &TextBuffer,
    line_idx: usize,
    stop_rendering_line_after: i64,
) -> String {
    editor_row_interaction_text_with_limit(
        buffer,
        line_idx,
        editor_row_interaction_text_limit(stop_rendering_line_after),
    )
}

fn editor_row_interaction_text_with_limit(
    buffer: &TextBuffer,
    line_idx: usize,
    max_chars: usize,
) -> String {
    buffer
        .line_content_prefix(line_idx, max_chars)
        .unwrap_or_default()
}

fn editor_row_interaction_text_limit(stop_rendering_line_after: i64) -> usize {
    let large_file_limit = default_editor_row_interaction_text_limit();
    editor_stop_rendering_line_after_limit(stop_rendering_line_after)
        .map(|limit| limit.min(large_file_limit))
        .unwrap_or(large_file_limit)
}

fn default_editor_row_interaction_text_limit() -> usize {
    usize::try_from(DEFAULT_EDITOR_STOP_RENDERING_LINE_AFTER).unwrap_or(10_000)
}

fn editor_row_wrap_interaction_text_limit(
    word_wrap: EditorWordWrap,
    word_wrap_column: usize,
    row_width: f32,
    gutter_width: f32,
    char_width: f32,
) -> Option<usize> {
    if matches!(word_wrap, EditorWordWrap::Off) {
        return None;
    }

    let viewport_columns = editor_row_viewport_text_columns(row_width, gutter_width, char_width);
    let configured_columns = clamp_editor_word_wrap_column(word_wrap_column).max(1);
    let limit = match word_wrap {
        EditorWordWrap::Off => return None,
        EditorWordWrap::On => viewport_columns,
        EditorWordWrap::WordWrapColumn => configured_columns,
        EditorWordWrap::Bounded => viewport_columns.min(configured_columns),
    };
    Some(limit.max(1))
}

fn editor_row_viewport_text_columns(row_width: f32, gutter_width: f32, char_width: f32) -> usize {
    let char_width = stable_positive_extent(char_width, 8.0);
    let text_width = stable_nonnegative_extent(row_width) - stable_nonnegative_extent(gutter_width);
    if !text_width.is_finite() || text_width <= 0.0 {
        return 1;
    }
    (text_width / char_width).ceil().max(1.0) as usize
}

fn register_editor_row_accessibility(
    response: &egui::Response,
    line_idx: usize,
    row: &EditorRowContext<'_>,
) {
    if !row.accessibility_enabled {
        return;
    }

    response.widget_info(|| editor_row_widget_info(line_idx, row));
}

fn editor_row_widget_info(line_idx: usize, row: &EditorRowContext<'_>) -> WidgetInfo {
    let line_number = line_idx + 1;
    let text = accessible_editor_row_text(
        row.buffer,
        line_idx,
        row.stop_rendering_line_after,
        row.accessibility_page_size,
    );
    let cursor = row
        .cursor_positions
        .iter()
        .any(|cursor| cursor.line == line_idx);
    let selected = row_has_selection(row.buffer, row.selections, line_idx);
    let folded = folded_range_starting_at(row.folded_ranges, line_number).is_some();
    let mut info = WidgetInfo::text_edit(
        row.ime_output_enabled && !row.buffer.is_read_only(),
        "",
        text,
        "",
    );
    info.label = Some(editor_row_accessibility_label(
        row.aria_label,
        line_number,
        row.buffer.len_lines(),
        row.aria_required,
        cursor,
        selected,
        folded,
        row.buffer.is_read_only(),
        row.diagnostics_by_line.get(&line_number).copied(),
        row.diff_lines.get(&line_number).copied(),
        row.render_rich_screen_reader_content,
        row.diff_accessibility_verbose,
    ));
    info
}

pub(crate) fn accessible_editor_row_text(
    buffer: &TextBuffer,
    line_idx: usize,
    stop_rendering_line_after: i64,
    page_size: usize,
) -> String {
    let Some(max_chars) = accessible_editor_row_text_limit(stop_rendering_line_after, page_size)
    else {
        return String::new();
    };
    sanitize_accessible_buffer_line_text(buffer, line_idx, max_chars)
}

fn accessible_editor_row_text_limit(
    stop_rendering_line_after: i64,
    page_size: usize,
) -> Option<usize> {
    let render_limit = editor_stop_rendering_line_after_limit(stop_rendering_line_after)
        .map(|limit| limit.min(default_editor_row_interaction_text_limit()))
        .unwrap_or_else(default_editor_row_interaction_text_limit);
    let max_chars = page_size.max(1).min(render_limit);
    (max_chars > 0).then_some(max_chars)
}

fn sanitize_accessible_buffer_line_text(
    buffer: &TextBuffer,
    line_idx: usize,
    max_chars: usize,
) -> String {
    let max_chars = max_chars.max(1);
    if line_idx >= buffer.len_lines() {
        return String::new();
    }

    let start = buffer.line_column_to_char(line_idx, 0);
    let end = buffer.line_content_end_char(line_idx);
    let mut sanitized = String::with_capacity(max_chars.saturating_add(3));
    let mut char_idx = start;
    for _ in 0..max_chars {
        if char_idx >= end {
            return sanitized;
        }
        let Some(ch) = buffer.char_at(char_idx) else {
            return sanitized;
        };
        sanitized.push(accessible_line_char(ch));
        char_idx = char_idx.saturating_add(1);
    }
    if char_idx < end {
        sanitized.push_str("...");
    }
    sanitized
}

#[cfg(test)]
pub(crate) fn sanitize_accessible_line_text(text: &str, max_chars: usize) -> String {
    let max_chars = max_chars.max(1);
    let mut sanitized = String::new();
    let mut chars = text.chars();
    for _ in 0..max_chars {
        let Some(ch) = chars.next() else {
            return sanitized;
        };
        sanitized.push(accessible_line_char(ch));
    }
    if chars.next().is_some() {
        sanitized.push_str("...");
    }
    sanitized
}

fn accessible_line_char(ch: char) -> char {
    if ch.is_control() && ch != '\t' {
        ' '
    } else {
        ch
    }
}

fn row_has_selection(buffer: &TextBuffer, selections: &[Selection], line_idx: usize) -> bool {
    if !editor_row_line_in_bounds(buffer, line_idx) {
        return false;
    }

    let line_start = buffer.line_column_to_char(line_idx, 0);
    let line_end = buffer.line_content_end_char(line_idx);
    selections.iter().any(|selection| {
        if selection.is_caret() {
            return false;
        }
        let range = selection.range();
        if line_start == line_end {
            range.start <= line_start && range.end >= line_start
        } else {
            range.start < line_end && range.end > line_start
        }
    })
}

pub(crate) fn editor_row_accessibility_label(
    aria_label: &str,
    line_number: usize,
    line_count: usize,
    required: bool,
    cursor: bool,
    selected: bool,
    folded: bool,
    read_only: bool,
    diagnostic: Option<DiagnosticSeverity>,
    diff_change: Option<GitLineChangeKind>,
    rich_content: bool,
    diff_accessibility_verbose: bool,
) -> String {
    let editor_label = match aria_label.trim() {
        "" => "Source editor",
        label => label,
    };
    let mut label = String::with_capacity(editor_label.len() + 96);
    label.push_str(editor_label);
    label.push_str(" line ");
    let _ = write!(label, "{line_number}");
    label.push_str(" of ");
    let _ = write!(label, "{}", line_count.max(1));
    if required {
        push_accessibility_label_part(&mut label, "required");
    }
    if cursor {
        push_accessibility_label_part(&mut label, "cursor");
    }
    if selected {
        push_accessibility_label_part(&mut label, "selected");
    }
    if rich_content {
        if folded {
            push_accessibility_label_part(&mut label, "folded");
        }
        if read_only {
            push_accessibility_label_part(&mut label, "read only");
        }
        if let Some(severity) = diagnostic {
            label.push_str(", ");
            label.push_str(diagnostic_severity_label(severity));
            label.push_str(" diagnostic");
        }
    }
    if rich_content || diff_accessibility_verbose {
        if let Some(change) = diff_change {
            label.push_str(", ");
            label.push_str(git_line_change_label(change));
            label.push_str(" line");
        }
    }
    label
}

fn push_accessibility_label_part(label: &mut String, part: &str) {
    label.push_str(", ");
    label.push_str(part);
}

fn diagnostic_severity_label(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Error => "error",
        DiagnosticSeverity::Warning => "warning",
        DiagnosticSeverity::Info => "info",
        DiagnosticSeverity::Hint => "hint",
    }
}

fn git_line_change_label(change: GitLineChangeKind) -> &'static str {
    match change {
        GitLineChangeKind::Added => "added",
        GitLineChangeKind::Modified => "modified",
        GitLineChangeKind::Deleted => "deleted",
    }
}

fn diff_gutter_context_action(
    action: crate::editor_row_gutter::DiffGutterAction,
) -> EditorContextAction {
    match action {
        crate::editor_row_gutter::DiffGutterAction::Stage => {
            EditorContextAction::StageActiveDiffHunk
        }
        crate::editor_row_gutter::DiffGutterAction::Unstage => {
            EditorContextAction::UnstageActiveDiffHunk
        }
        crate::editor_row_gutter::DiffGutterAction::Discard => {
            EditorContextAction::DiscardActiveDiffHunk
        }
    }
}

fn source_control_gutter_context_action(row: &EditorRowContext<'_>) -> Option<EditorContextAction> {
    if row.diff_stage.is_some() {
        return None;
    }
    if row.source_control_unstaged_actions {
        Some(EditorContextAction::OpenActiveFileHunkDiff)
    } else if row.source_control_staged_actions {
        Some(EditorContextAction::OpenActiveFileStagedHunkDiff)
    } else {
        None
    }
}

pub(crate) fn line_number_click_selects_line(
    rect: Rect,
    pos: egui::Pos2,
    line_idx: usize,
    row: &EditorRowContext<'_>,
) -> bool {
    row.select_on_line_numbers
        && final_newline_line_number_visible(
            row.render_final_newline,
            row.buffer.is_final_newline_line(line_idx),
        )
        && line_number_label(row.line_numbers, line_idx, row.cursor_positions).is_some()
        && line_number_hit_rect(rect, row.gutter_width, row.glyph_margin).contains(pos)
}

fn line_number_hit_rect(rect: Rect, gutter_width: f32, glyph_margin: bool) -> Rect {
    let gutter_width = stable_nonnegative_extent(gutter_width);
    let left_offset = if glyph_margin { 16.0 } else { 4.0 };
    if gutter_width <= left_offset {
        return Rect::from_min_max(rect.left_top(), rect.left_top());
    }
    let right_offset = (gutter_width - 22.0)
        .max(left_offset + 4.0)
        .min(gutter_width);
    Rect::from_min_max(
        pos2(rect.left() + left_offset, rect.top()),
        pos2(rect.left() + right_offset, rect.bottom()),
    )
}

pub(crate) fn folding_control_visible(
    controls: EditorShowFoldingControls,
    row_hovered: bool,
    folded_here: bool,
) -> bool {
    match controls {
        EditorShowFoldingControls::Always => true,
        EditorShowFoldingControls::Never => false,
        EditorShowFoldingControls::Mouseover => row_hovered || folded_here,
    }
}

pub(crate) fn multi_cursor_modifier_active(
    modifier: EditorMultiCursorModifier,
    input: egui::Modifiers,
) -> bool {
    match modifier {
        EditorMultiCursorModifier::Alt => input.alt,
        EditorMultiCursorModifier::CtrlCmd => input.ctrl || input.command,
    }
}

pub(crate) fn editor_mouse_cursor_icon(style: EditorMouseStyle) -> Option<egui::CursorIcon> {
    match style {
        EditorMouseStyle::Text => Some(egui::CursorIcon::Text),
        EditorMouseStyle::SystemDefault => None,
        EditorMouseStyle::Copy => Some(egui::CursorIcon::Copy),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EditorRowMiddleClickAction {
    None,
    AddCursor,
    OpenUrl(String),
}

pub(crate) fn editor_middle_click_action(
    action: EditorMouseMiddleClickAction,
    line_text: &str,
    char_column: usize,
) -> EditorRowMiddleClickAction {
    match action {
        EditorMouseMiddleClickAction::Default => EditorRowMiddleClickAction::None,
        EditorMouseMiddleClickAction::CtrlLeftClick => EditorRowMiddleClickAction::AddCursor,
        EditorMouseMiddleClickAction::OpenLink => url_at_char_column(line_text, char_column)
            .map(EditorRowMiddleClickAction::OpenUrl)
            .unwrap_or(EditorRowMiddleClickAction::None),
    }
}

pub(crate) fn url_at_char_column(line: &str, char_column: usize) -> Option<String> {
    for prefix in ["https://", "http://"] {
        let mut search_start = 0;
        while let Some(relative_start) = line[search_start..].find(prefix) {
            let start = search_start + relative_start;
            let end = url_span_end(line, start);
            let char_start = line[..start].chars().count();
            let char_end = line[..end].chars().count();
            if char_column >= char_start && char_column < char_end {
                return Some(line[start..end].to_owned());
            }
            search_start = end;
        }
    }

    None
}

fn url_span_end(line: &str, start: usize) -> usize {
    let mut end = line.len();
    for (offset, ch) in line[start..].char_indices() {
        if ch.is_whitespace() || matches!(ch, '"' | '\'' | '<' | '>') {
            end = start + offset;
            break;
        }
    }

    trim_trailing_url_punctuation(&line[start..end])
        .map(|trimmed| start + trimmed.len())
        .unwrap_or(end)
}

fn trim_trailing_url_punctuation(url: &str) -> Option<&str> {
    let trimmed = url.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '}']);
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EditorRowMiddleClickAction, accessible_editor_row_text, accessible_editor_row_text_limit,
        bounded_visible_editor_rows, cached_editor_row_interaction_text,
        editor_middle_click_action, editor_mouse_cursor_icon, editor_row_accessibility_label,
        editor_row_interaction_text, editor_row_interaction_text_limit, editor_row_line_in_bounds,
        editor_row_text_hit, editor_row_visual_column_at_pointer,
        editor_row_wrap_interaction_text_limit, folded_line_after_end_click,
        folding_control_visible, line_number_hit_rect, multi_cursor_modifier_active,
        prepare_editor_row_bounds, sanitize_accessible_line_text, url_at_char_column,
    };
    use eframe::egui::{CursorIcon, Modifiers, Rect, pos2, vec2};
    use kuroya_core::{
        DEFAULT_EDITOR_STOP_RENDERING_LINE_AFTER, DiagnosticSeverity, EditorMouseMiddleClickAction,
        EditorMouseStyle, EditorMultiCursorModifier, EditorShowFoldingControls, EditorWordWrap,
        GitLineChangeKind, TextBuffer,
    };
    use std::{cell::Cell, ops::Range};

    #[test]
    fn multi_cursor_modifier_controls_add_cursor_click_modifier() {
        let alt = Modifiers {
            alt: true,
            ..Modifiers::NONE
        };
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };

        assert!(multi_cursor_modifier_active(
            EditorMultiCursorModifier::Alt,
            alt
        ));
        assert!(!multi_cursor_modifier_active(
            EditorMultiCursorModifier::Alt,
            ctrl
        ));
        assert!(multi_cursor_modifier_active(
            EditorMultiCursorModifier::CtrlCmd,
            ctrl
        ));
        assert!(!multi_cursor_modifier_active(
            EditorMultiCursorModifier::CtrlCmd,
            alt
        ));
    }

    #[test]
    fn editor_mouse_cursor_icon_respects_mouse_style() {
        assert_eq!(
            editor_mouse_cursor_icon(EditorMouseStyle::Text),
            Some(CursorIcon::Text)
        );
        assert_eq!(
            editor_mouse_cursor_icon(EditorMouseStyle::SystemDefault),
            None
        );
        assert_eq!(
            editor_mouse_cursor_icon(EditorMouseStyle::Copy),
            Some(CursorIcon::Copy)
        );
    }

    #[test]
    fn editor_middle_click_action_respects_setting() {
        assert_eq!(
            editor_middle_click_action(
                EditorMouseMiddleClickAction::Default,
                "https://example.com",
                9
            ),
            EditorRowMiddleClickAction::None
        );
        assert_eq!(
            editor_middle_click_action(EditorMouseMiddleClickAction::CtrlLeftClick, "plain", 2),
            EditorRowMiddleClickAction::AddCursor
        );
        assert_eq!(
            editor_middle_click_action(
                EditorMouseMiddleClickAction::OpenLink,
                "docs https://example.com/path",
                12,
            ),
            EditorRowMiddleClickAction::OpenUrl("https://example.com/path".to_owned())
        );
    }

    #[test]
    fn url_at_char_column_detects_http_links_and_trims_punctuation() {
        let line = "see https://example.com/docs), then http://localhost:3000/path.";

        assert_eq!(
            url_at_char_column(line, 8).as_deref(),
            Some("https://example.com/docs")
        );
        assert_eq!(
            url_at_char_column(line, 45).as_deref(),
            Some("http://localhost:3000/path")
        );
        assert_eq!(url_at_char_column(line, 0), None);
    }

    #[test]
    fn folded_line_after_end_click_follows_setting_fold_state_and_text_end() {
        assert!(folded_line_after_end_click(true, true, 64.0, 40.0, 3, 8.0));
        assert!(!folded_line_after_end_click(
            false, true, 64.0, 40.0, 3, 8.0
        ));
        assert!(!folded_line_after_end_click(
            true, false, 64.0, 40.0, 3, 8.0
        ));
        assert!(!folded_line_after_end_click(true, true, 63.0, 40.0, 3, 8.0));
        assert!(!folded_line_after_end_click(true, true, 64.0, 40.0, 3, 0.0));
    }

    #[test]
    fn folding_control_visibility_respects_never_mode() {
        assert!(folding_control_visible(
            EditorShowFoldingControls::Always,
            false,
            false
        ));
        assert!(folding_control_visible(
            EditorShowFoldingControls::Mouseover,
            true,
            false
        ));
        assert!(folding_control_visible(
            EditorShowFoldingControls::Mouseover,
            false,
            true
        ));
        assert!(!folding_control_visible(
            EditorShowFoldingControls::Mouseover,
            false,
            false
        ));
        assert!(!folding_control_visible(
            EditorShowFoldingControls::Never,
            true,
            true
        ));
    }

    #[test]
    fn line_number_hit_rect_uses_line_number_zone() {
        let rect = Rect::from_min_size(pos2(100.0, 20.0), vec2(200.0, 18.0));

        let plain = line_number_hit_rect(rect, 64.0, false);
        assert!(plain.contains(pos2(108.0, 24.0)));
        assert!(!plain.contains(pos2(101.0, 24.0)));
        assert!(!plain.contains(pos2(145.0, 24.0)));

        let glyph = line_number_hit_rect(rect, 72.0, true);
        assert!(glyph.contains(pos2(120.0, 24.0)));
        assert!(!glyph.contains(pos2(108.0, 24.0)));
    }

    #[test]
    fn line_number_hit_rect_clamps_unusable_gutter_widths() {
        let rect = Rect::from_min_size(pos2(100.0, 20.0), vec2(200.0, 18.0));

        let zero = line_number_hit_rect(rect, 0.0, false);
        assert!(!zero.contains(pos2(101.0, 21.0)));

        let nan = line_number_hit_rect(rect, f32::NAN, true);
        assert!(!nan.contains(pos2(116.0, 24.0)));
    }

    #[test]
    fn prepared_editor_row_stabilizes_geometry_once() {
        let row = prepare_editor_row_bounds(-20.0, f32::NAN, 4.0, 120.0, f32::NAN, 80.0);

        assert_eq!(row.rect.left(), -16.0);
        assert_eq!(row.rect.top(), 0.0);
        assert_eq!(row.rect.width(), 120.0);
        assert_eq!(row.row_height, 1.0);
        assert_eq!(row.gutter_width, 80.0);
        assert_eq!(row.text_pos(3.0), pos2(64.0, 3.0));
        assert!(!row.text_hit(pos2(63.0, 0.5)));
        assert!(row.text_hit(pos2(64.0, 0.5)));

        let clamped = prepare_editor_row_bounds(0.0, 0.0, 0.0, 120.0, 18.0, 140.0);
        assert_eq!(clamped.gutter_width, 120.0);
    }

    #[test]
    fn bounded_visible_editor_rows_clamps_stale_ranges() {
        assert_eq!(bounded_visible_editor_rows(2..8, 5), 2..5);
        assert_eq!(bounded_visible_editor_rows(8..10, 5), 5..5);
        assert_eq!(
            bounded_visible_editor_rows(Range { start: 4, end: 2 }, 5),
            2..2
        );
    }

    #[test]
    fn editor_row_line_bounds_reject_stale_rows() {
        let buffer = TextBuffer::from_text(1, None, "first\nsecond".to_owned());

        assert!(editor_row_line_in_bounds(&buffer, 0));
        assert!(editor_row_line_in_bounds(
            &buffer,
            buffer.len_lines().saturating_sub(1)
        ));
        assert!(!editor_row_line_in_bounds(&buffer, buffer.len_lines()));
        assert!(!editor_row_line_in_bounds(&buffer, usize::MAX));
    }

    #[test]
    fn row_text_hit_ignores_gutter_and_invalid_pointer_positions() {
        let rect = Rect::from_min_size(pos2(100.0, 20.0), vec2(200.0, 18.0));

        assert!(!editor_row_text_hit(rect, pos2(120.0, 24.0), 40.0));
        assert!(editor_row_text_hit(rect, pos2(140.0, 24.0), 40.0));
        assert!(!editor_row_text_hit(rect, pos2(f32::NAN, 24.0), 40.0));
    }

    #[test]
    fn pointer_visual_column_uses_stable_geometry() {
        let rect = Rect::from_min_size(pos2(100.0, 20.0), vec2(200.0, 18.0));

        assert_eq!(
            editor_row_visual_column_at_pointer(rect, pos2(156.0, 24.0), 40.0, 8.0),
            2
        );
        assert_eq!(
            editor_row_visual_column_at_pointer(rect, pos2(120.0, 24.0), 40.0, 8.0),
            0
        );
        assert_eq!(
            editor_row_visual_column_at_pointer(rect, pos2(156.0, 24.0), 40.0, f32::NAN),
            0
        );
    }

    #[test]
    fn editor_row_interaction_text_respects_rendering_limit() {
        let buffer = TextBuffer::from_text(1, None, "abcdef\n".to_owned());

        assert_eq!(editor_row_interaction_text(&buffer, 0, 3), "abc");
        assert_eq!(editor_row_interaction_text(&buffer, 0, -1), "abcdef");
        assert_eq!(editor_row_interaction_text(&buffer, 99, 3), "");
    }

    #[test]
    fn cached_editor_row_interaction_text_builds_once_per_row() {
        let builds = Cell::new(0usize);
        let mut cache = None;

        assert_eq!(
            cached_editor_row_interaction_text(&mut cache, || {
                builds.set(builds.get() + 1);
                "abcdef".to_owned()
            }),
            "abcdef"
        );
        assert_eq!(
            cached_editor_row_interaction_text(&mut cache, || {
                builds.set(builds.get() + 1);
                "replacement".to_owned()
            }),
            "abcdef"
        );
        assert_eq!(builds.get(), 1);
    }

    #[test]
    fn editor_row_interaction_text_caps_unbounded_long_lines() {
        let long_line = "x".repeat(DEFAULT_EDITOR_STOP_RENDERING_LINE_AFTER as usize + 7);
        let buffer = TextBuffer::from_text(1, None, format!("{long_line}\n"));

        assert_eq!(
            editor_row_interaction_text(&buffer, 0, -1).len(),
            DEFAULT_EDITOR_STOP_RENDERING_LINE_AFTER as usize
        );
    }

    #[test]
    fn editor_row_interaction_text_limit_caps_explicit_large_limits() {
        let default_limit = DEFAULT_EDITOR_STOP_RENDERING_LINE_AFTER as usize;

        assert_eq!(editor_row_interaction_text_limit(-1), default_limit);
        assert_eq!(editor_row_interaction_text_limit(3), 3);
        assert_eq!(
            editor_row_interaction_text_limit(DEFAULT_EDITOR_STOP_RENDERING_LINE_AFTER + 500),
            default_limit
        );
    }

    #[test]
    fn editor_row_wrap_interaction_text_limit_tracks_visible_columns() {
        assert_eq!(
            editor_row_wrap_interaction_text_limit(EditorWordWrap::Off, 80, 404.0, 84.0, 8.0),
            None
        );
        assert_eq!(
            editor_row_wrap_interaction_text_limit(EditorWordWrap::On, 80, 404.0, 84.0, 8.0),
            Some(40)
        );
        assert_eq!(
            editor_row_wrap_interaction_text_limit(
                EditorWordWrap::WordWrapColumn,
                72,
                404.0,
                84.0,
                8.0
            ),
            Some(72)
        );
        assert_eq!(
            editor_row_wrap_interaction_text_limit(EditorWordWrap::Bounded, 72, 404.0, 84.0, 8.0),
            Some(40)
        );
        assert_eq!(
            editor_row_wrap_interaction_text_limit(EditorWordWrap::On, 80, 40.0, 80.0, f32::NAN),
            Some(1)
        );
    }

    #[test]
    fn accessible_editor_row_text_is_sanitized_and_bounded() {
        let buffer = TextBuffer::from_text(1, None, "abcd\u{0007}efghij\n".to_owned());

        assert_eq!(sanitize_accessible_line_text("ab\u{0007}c", 10), "ab c");
        assert_eq!(accessible_editor_row_text(&buffer, 0, -1, 5), "abcd ...");
        assert_eq!(accessible_editor_row_text(&buffer, 0, 3, 10), "abc...");
        assert_eq!(accessible_editor_row_text(&buffer, 0, 0, 10), "");
        assert_eq!(
            accessible_editor_row_text_limit(-1, usize::MAX),
            Some(DEFAULT_EDITOR_STOP_RENDERING_LINE_AFTER as usize)
        );
    }

    #[test]
    fn accessible_editor_row_text_scans_buffer_line_directly() {
        let buffer = TextBuffer::from_text(
            1,
            None,
            "alpha\n\u{03b1}\u{0007}\u{03b2}\u{03b3}\n".to_owned(),
        );

        assert_eq!(
            accessible_editor_row_text(&buffer, 1, -1, 3),
            "\u{03b1} \u{03b2}..."
        );
        assert_eq!(accessible_editor_row_text(&buffer, 99, -1, 3), "");
    }

    #[test]
    fn editor_row_accessibility_label_adds_rich_line_state_when_enabled() {
        assert_eq!(
            editor_row_accessibility_label(
                " Source editor ",
                4,
                12,
                true,
                true,
                true,
                true,
                true,
                Some(DiagnosticSeverity::Warning),
                Some(GitLineChangeKind::Modified),
                true,
                false,
            ),
            "Source editor line 4 of 12, required, cursor, selected, folded, read only, warning diagnostic, modified line"
        );
        assert_eq!(
            editor_row_accessibility_label(
                "",
                1,
                0,
                false,
                false,
                false,
                true,
                true,
                Some(DiagnosticSeverity::Error),
                Some(GitLineChangeKind::Added),
                false,
                false,
            ),
            "Source editor line 1 of 1"
        );
        assert_eq!(
            editor_row_accessibility_label(
                "",
                2,
                4,
                false,
                false,
                false,
                false,
                false,
                None,
                Some(GitLineChangeKind::Added),
                false,
                true,
            ),
            "Source editor line 2 of 4, added line"
        );
    }

    #[test]
    fn editor_row_accessibility_label_keeps_optional_part_order() {
        assert_eq!(
            editor_row_accessibility_label(
                " Editor ",
                42,
                0,
                true,
                true,
                false,
                false,
                false,
                Some(DiagnosticSeverity::Hint),
                Some(GitLineChangeKind::Deleted),
                true,
                true,
            ),
            "Editor line 42 of 1, required, cursor, hint diagnostic, deleted line"
        );
    }
}
