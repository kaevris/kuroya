use crate::{
    editor_pane_rows::EditorRowContext,
    editor_row_gutter::paint_row_gutter,
    editor_row_overlays::{
        paint_bracket_depth_markers, paint_bracket_match_boxes, paint_bracket_pair_guides,
        paint_code_lenses, paint_completion_preview, paint_cursors, paint_diagnostic_message,
        paint_folded_label, paint_ime_preedit, paint_inlay_hints, primary_insertion_cursor_rect,
    },
    editor_text_geometry::visual_width_for_char,
    folding::{best_folding_range_starting_at, folded_range_starting_at},
    source_control_blame_runtime::git_blame_editor_decoration_label,
    syntax_tree_cache::TreeSitterInjection,
};
use colors::paint_color_decorators;
#[cfg(test)]
pub(crate) use colors::{color_decorators_visible, hex_color_decorations, parse_hex_color};
use eframe::egui::{self, Color32, FontFamily, FontId, Stroke, pos2};
use highlights::{paint_row_deprecated_diagnostic_tags, paint_row_highlights};
use kuroya_core::buffer::BracketColor;
use kuroya_core::{
    EditorExperimentalWhitespaceRendering, EditorRenderLineHighlight, EditorRenderWhitespace,
    EditorWordWrap, LanguageId, MergeConflictLineKind, Selection, TextBuffer,
    clamp_editor_word_wrap_column, editor_stop_rendering_line_after_limit,
    merge_conflict_line_kind,
};
use std::{ops::Range, sync::Arc, time::Duration};

mod colors;
mod highlights;

const MAX_WHITESPACE_SELECTION_RANGES_PER_ROW: usize = 1024;
const BACKGROUND_IMAGE_LINE_HIGHLIGHT_MAX_ALPHA: u8 = 56;
const BACKGROUND_IMAGE_SELECTION_MAX_ALPHA: u8 = 112;
const BACKGROUND_IMAGE_STICKY_SCROLL_MAX_ALPHA: u8 = 184;

pub(crate) fn paint_editor_row(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    line_idx: usize,
    highlighted_job: Option<egui::text::LayoutJob>,
    cached_galley: Option<Arc<egui::epaint::Galley>>,
    bracket_colors: &[BracketColor],
    row: &EditorRowContext<'_>,
    row_hovered: bool,
) {
    let Some(rendered_line) =
        editor_row_line_snapshot(row.buffer, line_idx, row.stop_rendering_line_after)
    else {
        return;
    };
    let RenderedLineSnapshot {
        snapshot,
        line_end_visible,
    } = rendered_line;
    let text = snapshot.text.trim_end_matches(['\r', '\n']);
    let painter = ui.painter();
    let line_number = line_idx + 1;
    let folded_here = row
        .folding
        .then(|| folded_range_starting_at(row.folded_ranges, line_number))
        .flatten();
    let foldable_here =
        row.folding && best_folding_range_starting_at(row.folding_ranges, line_number).is_some();
    let cursor_on_line = row
        .cursor_positions
        .iter()
        .find(|cursor| cursor.line == line_idx);
    let cursor_char_idx = cursor_on_line.map(|cursor| cursor.char_idx);
    let has_cursor_on_line = cursor_on_line.is_some();
    if has_cursor_on_line
        && line_highlight_visible(row.render_line_highlight_only_when_focus, row.focused)
    {
        paint_line_highlight(painter, rect, row, ui.visuals());
    }
    if folded_region_highlight_visible(row.folding_highlight, folded_here.is_some()) {
        paint_folded_region_highlight(painter, rect, row.weak_text_color);
    }
    paint_merge_conflict_background(painter, rect, line_idx, row);
    paint_diff_move_decoration(painter, rect, line_number, row);
    let text_metrics = row_text_metrics(text, row.tab_width);
    paint_injected_language_ranges(
        painter,
        rect,
        &snapshot.char_range,
        text,
        text_metrics.char_count,
        row,
    );

    let visual_text_width = text_metrics.visual_width;
    paint_row_highlights(
        painter,
        rect,
        &snapshot.char_range,
        line_end_visible,
        text,
        row,
    );
    paint_diff_empty_decoration(painter, rect, row, text);
    paint_column_ruler(painter, rect, row, ui.visuals());
    paint_indent_guides(painter, rect, text, row, ui.visuals());

    paint_row_gutter(
        ui,
        rect,
        line_idx,
        text,
        folded_here.is_some(),
        foldable_here,
        row,
        row_hovered,
    );

    let galley = match cached_galley {
        Some(galley) => galley,
        None => {
            let Some(mut job) = highlighted_job else {
                return;
            };
            limit_layout_job_line_rendering(&mut job, row.stop_rendering_line_after);
            job.wrap.max_width = editor_row_wrap_width(
                rect.width(),
                row.gutter_width,
                row.word_wrap,
                row.word_wrap_column,
                row.char_width,
            );
            ui.fonts_mut(|fonts| fonts.layout_job(job))
        }
    };
    let text_pos = pos2(rect.left() + row.gutter_width, rect.top() + 3.0);
    paint_bracket_pair_guides(painter, line_idx, text, text_pos, rect, row);
    painter.galley(text_pos, galley, ui.visuals().text_color());
    paint_color_decorators(painter, rect, text_pos, text, row, row_hovered);
    paint_row_deprecated_diagnostic_tags(painter, rect, &snapshot.char_range, text, row);
    paint_control_character_markers(ui, painter, rect, text_pos, text, &text_metrics, row);
    paint_whitespace_markers(
        ui,
        painter,
        rect,
        text_pos,
        &snapshot.char_range,
        text,
        &text_metrics,
        line_end_visible,
        row,
    );
    paint_inlay_hints(painter, rect, text_pos, text, line_idx, row);
    paint_completion_preview(
        painter,
        rect,
        text_pos,
        &snapshot.char_range,
        line_idx,
        cursor_char_idx,
        text_metrics.char_count,
        visual_text_width,
        row,
    );
    paint_code_lenses(painter, rect, text_pos, visual_text_width, line_idx, row);
    paint_git_blame_decoration(
        painter,
        rect,
        text_pos,
        visual_text_width,
        line_idx,
        has_cursor_on_line,
        row,
    );
    if let Some(range) = folded_here {
        paint_folded_label(painter, rect, text_pos, visual_text_width, row, range);
    }

    if row.bracket_pair_colorization {
        paint_bracket_depth_markers(
            painter,
            &snapshot.char_range,
            text,
            text_pos,
            rect,
            bracket_colors,
            row,
        );
    }
    paint_diagnostic_message(painter, rect, text_pos, visual_text_width, line_idx, row);
    if row.match_brackets.enabled() {
        paint_bracket_match_boxes(painter, &snapshot.char_range, text, text_pos, rect, row);
    }
    emit_editor_ime_output(
        ui,
        rect,
        text_pos,
        &snapshot.char_range,
        text,
        line_idx,
        row,
    );
    paint_ime_preedit(
        painter,
        rect,
        text_pos,
        &snapshot.char_range,
        text,
        cursor_char_idx,
        row,
    );
    if has_cursor_on_line && editor_cursor_visible(ui, row.cursor_blinking) {
        paint_cursors(
            ui,
            painter,
            text_pos,
            rect,
            &snapshot.char_range,
            text,
            line_idx,
            row,
        );
    }
}

fn emit_editor_ime_output(
    ui: &egui::Ui,
    rect: egui::Rect,
    text_pos: egui::Pos2,
    snapshot_range: &std::ops::Range<usize>,
    text: &str,
    line_idx: usize,
    row: &EditorRowContext<'_>,
) {
    if !editor_ime_output_enabled(row.ime_output_enabled, row.buffer.is_read_only()) {
        return;
    }
    let Some(cursor_rect) =
        primary_insertion_cursor_rect(text_pos, rect, snapshot_range, text, line_idx, row)
    else {
        return;
    };

    let to_global = ui
        .ctx()
        .layer_transform_to_global(ui.layer_id())
        .unwrap_or_default();
    ui.ctx().output_mut(|output| {
        output.ime = Some(egui::output::IMEOutput {
            rect: to_global * rect,
            cursor_rect: to_global * cursor_rect,
        });
    });
}

pub(crate) fn editor_ime_output_enabled(accepts_text_input: bool, read_only: bool) -> bool {
    accepts_text_input && !read_only
}

fn paint_injected_language_ranges(
    painter: &egui::Painter,
    rect: egui::Rect,
    snapshot_range: &std::ops::Range<usize>,
    text: &str,
    text_len: usize,
    row: &EditorRowContext<'_>,
) {
    if row.syntax_injections.is_empty() || !row.char_width.is_finite() || row.char_width <= 0.0 {
        return;
    }

    let mut visual_columns = VisualColumnScanner::new(text, row.tab_width);
    for injection in row.syntax_injections {
        let Some(render_rect) = injected_language_render_rect_with_scanner(
            rect,
            snapshot_range,
            text_len,
            row.gutter_width,
            row.char_width,
            injection,
            &mut visual_columns,
        ) else {
            continue;
        };

        painter.rect_filled(render_rect, 2.0, injected_language_fill(injection.language));
    }
}

#[cfg(test)]
fn injected_language_render_rect(
    rect: egui::Rect,
    snapshot_range: &std::ops::Range<usize>,
    text: &str,
    gutter_width: f32,
    tab_width: usize,
    char_width: f32,
    injection: &TreeSitterInjection,
) -> Option<egui::Rect> {
    let text_len = text.chars().count();
    let mut visual_columns = VisualColumnScanner::new(text, tab_width);
    injected_language_render_rect_with_scanner(
        rect,
        snapshot_range,
        text_len,
        gutter_width,
        char_width,
        injection,
        &mut visual_columns,
    )
}

fn injected_language_render_rect_with_scanner(
    rect: egui::Rect,
    snapshot_range: &std::ops::Range<usize>,
    text_len: usize,
    gutter_width: f32,
    char_width: f32,
    injection: &TreeSitterInjection,
    visual_columns: &mut VisualColumnScanner<'_>,
) -> Option<egui::Rect> {
    if !char_width.is_finite() || char_width <= 0.0 {
        return None;
    }

    let line_start = snapshot_range.start;
    let line_end = line_start + text_len;
    let start = injection.range.start.max(line_start);
    let end = injection.range.end.min(line_end);
    if end <= start {
        return None;
    }

    let start_column = start.saturating_sub(line_start).min(text_len);
    let end_column = end.saturating_sub(line_start).min(text_len);
    let (start_visual, end_visual) =
        visual_columns.visual_columns_for_offsets(start_column, end_column);
    let end_visual = end_visual.max(start_visual.saturating_add(1));
    let left = rect.left() + gutter_width + start_visual as f32 * char_width;
    let right = (rect.left() + gutter_width + end_visual as f32 * char_width).min(rect.right());
    if right <= left {
        return None;
    }

    Some(egui::Rect::from_min_max(
        pos2(left, rect.top() + 3.0),
        pos2(right, rect.bottom() - 3.0),
    ))
}

struct VisualColumnScanner<'a> {
    text: &'a str,
    chars: std::str::Chars<'a>,
    tab_width: usize,
    char_offset: usize,
    visual_column: usize,
}

impl<'a> VisualColumnScanner<'a> {
    fn new(text: &'a str, tab_width: usize) -> Self {
        Self {
            text,
            chars: text.chars(),
            tab_width: tab_width.max(1),
            char_offset: 0,
            visual_column: 0,
        }
    }

    fn visual_columns_for_offsets(
        &mut self,
        start_offset: usize,
        end_offset: usize,
    ) -> (usize, usize) {
        let start_offset = start_offset.min(end_offset);
        let start_visual = self.visual_column_at(start_offset);
        let end_visual = self.visual_column_at(end_offset);
        (start_visual, end_visual)
    }

    fn visual_column_at(&mut self, char_offset: usize) -> usize {
        if char_offset < self.char_offset {
            self.rewind();
        }

        while self.char_offset < char_offset {
            let Some(ch) = self.chars.next() else {
                break;
            };
            self.visual_column = self.visual_column.saturating_add(visual_width_for_char(
                ch,
                self.visual_column,
                self.tab_width,
            ));
            self.char_offset += 1;
        }

        self.visual_column
    }

    fn rewind(&mut self) {
        self.chars = self.text.chars();
        self.char_offset = 0;
        self.visual_column = 0;
    }
}

fn injected_language_fill(language: LanguageId) -> Color32 {
    match language {
        LanguageId::Sql => Color32::from_rgba_premultiplied(196, 155, 74, 34),
        LanguageId::Json | LanguageId::Toml | LanguageId::Yaml => {
            Color32::from_rgba_premultiplied(91, 141, 239, 30)
        }
        LanguageId::Markdown
        | LanguageId::Html
        | LanguageId::Xml
        | LanguageId::Vue
        | LanguageId::Svelte => Color32::from_rgba_premultiplied(126, 168, 97, 30),
        LanguageId::TypeScript | LanguageId::JavaScript | LanguageId::Css => {
            Color32::from_rgba_premultiplied(208, 190, 86, 28)
        }
        LanguageId::Python
        | LanguageId::PowerShell
        | LanguageId::Ruby
        | LanguageId::Lua
        | LanguageId::Shell
        | LanguageId::Dockerfile
        | LanguageId::Terraform => Color32::from_rgba_premultiplied(112, 155, 229, 28),
        LanguageId::Rust
        | LanguageId::Go
        | LanguageId::Java
        | LanguageId::C
        | LanguageId::Cpp
        | LanguageId::CSharp
        | LanguageId::Php
        | LanguageId::Dart
        | LanguageId::Kotlin
        | LanguageId::Swift
        | LanguageId::Diff
        | LanguageId::PlainText => Color32::from_rgba_premultiplied(126, 136, 150, 24),
    }
}

#[derive(Debug, Clone)]
struct RenderedLineSnapshot {
    snapshot: kuroya_core::buffer::LineSnapshot,
    line_end_visible: bool,
}

fn editor_row_line_snapshot(
    buffer: &TextBuffer,
    line_idx: usize,
    stop_rendering_line_after: i64,
) -> Option<RenderedLineSnapshot> {
    match editor_stop_rendering_line_after_limit(stop_rendering_line_after) {
        Some(limit) => {
            let mut snapshot = buffer.line_snapshot_prefix(line_idx, limit.saturating_add(1))?;
            let line_end_visible =
                if let Some(cut_byte) = byte_index_after_chars(&snapshot.text, limit) {
                    snapshot.text.truncate(cut_byte);
                    snapshot.char_range.end = snapshot.char_range.start.saturating_add(limit);
                    false
                } else {
                    true
                };
            Some(RenderedLineSnapshot {
                snapshot,
                line_end_visible,
            })
        }
        None => buffer
            .line_snapshot(line_idx)
            .map(|snapshot| RenderedLineSnapshot {
                snapshot,
                line_end_visible: true,
            }),
    }
}

fn paint_git_blame_decoration(
    painter: &egui::Painter,
    rect: egui::Rect,
    text_pos: egui::Pos2,
    visual_text_width: usize,
    line_idx: usize,
    has_cursor_on_line: bool,
    row: &EditorRowContext<'_>,
) {
    if !row.git_blame_editor_decoration_enabled
        || row.git_blame_lines.is_empty()
        || !has_cursor_on_line
    {
        return;
    }

    let Some(label) = git_blame_editor_decoration_label(
        row.git_blame_lines,
        line_idx + 1,
        row.git_blame_editor_decoration_template,
    ) else {
        return;
    };
    let x = text_pos.x + visual_text_width as f32 * row.char_width + 24.0;
    if x >= rect.right() {
        return;
    }
    painter.text(
        pos2(x, rect.top() + 3.0),
        egui::Align2::LEFT_TOP,
        label,
        FontId::new((row.font_size * 0.86).max(8.0), FontFamily::Monospace),
        row.weak_text_color,
    );
}

fn paint_merge_conflict_background(
    painter: &egui::Painter,
    rect: egui::Rect,
    line_idx: usize,
    row: &EditorRowContext<'_>,
) {
    let Some(kind) = merge_conflict_line_kind(row.merge_conflicts, line_idx) else {
        return;
    };
    painter.rect_filled(rect, 0.0, merge_conflict_line_fill(kind));
}

fn paint_diff_empty_decoration(
    painter: &egui::Painter,
    rect: egui::Rect,
    row: &EditorRowContext<'_>,
    line_text: &str,
) {
    if !diff_empty_decoration_visible(
        row.diff_experimental_show_empty_decorations,
        row.diff_patch_actions,
        line_text,
    ) {
        return;
    }

    let left = rect.left() + row.gutter_width + row.char_width;
    let right = (left + row.char_width * 3.0).min(rect.right());
    if right <= left {
        return;
    }

    painter.rect_filled(
        egui::Rect::from_min_max(
            pos2(left, rect.top() + 3.0),
            pos2(right, rect.bottom() - 3.0),
        ),
        2.0,
        diff_empty_decoration_fill(line_text),
    );
}

fn paint_diff_move_decoration(
    painter: &egui::Painter,
    rect: egui::Rect,
    line_number: usize,
    row: &EditorRowContext<'_>,
) {
    if !diff_move_decoration_visible(
        row.diff_patch_actions,
        row.diff_move_lines.contains(&line_number),
    ) {
        return;
    }

    painter.rect_filled(rect, 0.0, diff_move_decoration_fill());
    painter.rect_filled(
        egui::Rect::from_min_max(
            pos2(rect.left() + row.gutter_width, rect.top()),
            pos2(rect.left() + row.gutter_width + 3.0, rect.bottom()),
        ),
        0.0,
        diff_move_decoration_stripe_fill(),
    );
}

pub(crate) fn diff_move_decoration_visible(diff_patch_actions: bool, moved_line: bool) -> bool {
    diff_patch_actions && moved_line
}

pub(crate) fn diff_move_decoration_fill() -> Color32 {
    Color32::from_rgba_premultiplied(90, 121, 184, 42)
}

pub(crate) fn diff_move_decoration_stripe_fill() -> Color32 {
    Color32::from_rgb(112, 155, 229)
}

pub(crate) fn diff_empty_decoration_visible(
    show_empty_decorations: bool,
    diff_patch_actions: bool,
    line_text: &str,
) -> bool {
    show_empty_decorations && diff_patch_actions && matches!(line_text, "+" | "-")
}

pub(crate) fn diff_empty_decoration_fill(line_text: &str) -> Color32 {
    match line_text {
        "+" => Color32::from_rgba_premultiplied(72, 142, 99, 60),
        "-" => Color32::from_rgba_premultiplied(170, 70, 70, 58),
        _ => Color32::TRANSPARENT,
    }
}

pub(crate) fn merge_conflict_line_fill(kind: MergeConflictLineKind) -> Color32 {
    match kind {
        MergeConflictLineKind::Start
        | MergeConflictLineKind::Separator
        | MergeConflictLineKind::End => Color32::from_rgba_premultiplied(108, 83, 38, 82),
        MergeConflictLineKind::Current => Color32::from_rgba_premultiplied(40, 105, 72, 54),
        MergeConflictLineKind::Incoming => Color32::from_rgba_premultiplied(42, 91, 134, 54),
    }
}

pub(crate) fn line_highlight_visible(only_when_focus: bool, focused: bool) -> bool {
    !only_when_focus || focused
}

pub(crate) fn folded_region_highlight_visible(folding_highlight: bool, folded_here: bool) -> bool {
    folding_highlight && folded_here
}

fn paint_folded_region_highlight(painter: &egui::Painter, rect: egui::Rect, base: Color32) {
    painter.rect_filled(rect, 0.0, folded_region_highlight_fill(base));
}

pub(crate) fn folded_region_highlight_fill(base: Color32) -> Color32 {
    // egui stores premultiplied channels; building the fill with the
    // premultiplied constructor kept the full base channels under a tiny alpha
    // and rendered the highlight roughly 255/44 times too strong.
    Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 44)
}

fn paint_line_highlight(
    painter: &egui::Painter,
    rect: egui::Rect,
    row: &EditorRowContext<'_>,
    visuals: &egui::Visuals,
) {
    let color = line_highlight_fill(visuals, row.background_image_active);
    let corner_radius = if row.background_image_active {
        2.0
    } else {
        0.0
    };
    match row.render_line_highlight {
        EditorRenderLineHighlight::None => {}
        EditorRenderLineHighlight::Gutter => {
            painter.rect_filled(
                egui::Rect::from_min_max(
                    rect.min,
                    pos2(rect.left() + row.gutter_width, rect.bottom()),
                ),
                corner_radius,
                color,
            );
        }
        EditorRenderLineHighlight::Line => {
            painter.rect_filled(
                egui::Rect::from_min_max(
                    pos2(rect.left() + row.gutter_width, rect.top()),
                    rect.max,
                ),
                corner_radius,
                color,
            );
        }
        EditorRenderLineHighlight::All => {
            painter.rect_filled(rect, corner_radius, color);
        }
    }
}

fn line_highlight_fill(visuals: &egui::Visuals, background_image_active: bool) -> Color32 {
    overlay_fill_for_background_image(
        visuals.widgets.active.weak_bg_fill,
        background_image_active,
        BACKGROUND_IMAGE_LINE_HIGHLIGHT_MAX_ALPHA,
    )
}

pub(crate) fn editor_selection_fill(color: Color32, background_image_active: bool) -> Color32 {
    overlay_fill_for_background_image(
        color,
        background_image_active,
        BACKGROUND_IMAGE_SELECTION_MAX_ALPHA,
    )
}

pub(crate) fn editor_sticky_scroll_fill(color: Color32, background_image_active: bool) -> Color32 {
    overlay_fill_for_background_image(
        color,
        background_image_active,
        BACKGROUND_IMAGE_STICKY_SCROLL_MAX_ALPHA,
    )
}

fn overlay_fill_for_background_image(
    color: Color32,
    background_image_active: bool,
    max_alpha: u8,
) -> Color32 {
    if !background_image_active || color.a() <= max_alpha {
        return color;
    }

    let scale = f32::from(max_alpha) / f32::from(color.a());
    let scaled = |channel: u8| (f32::from(channel) * scale).round() as u8;
    Color32::from_rgba_premultiplied(
        scaled(color.r()),
        scaled(color.g()),
        scaled(color.b()),
        max_alpha,
    )
}

fn paint_column_ruler(
    painter: &egui::Painter,
    rect: egui::Rect,
    row: &EditorRowContext<'_>,
    visuals: &egui::Visuals,
) {
    if row.ruler_column == 0 {
        return;
    }

    let x = rect.left() + row.gutter_width + row.ruler_column as f32 * row.char_width;
    if x <= rect.left() || x >= rect.right() {
        return;
    }

    painter.line_segment(
        [pos2(x, rect.top()), pos2(x, rect.bottom())],
        egui::Stroke::new(1.0, column_ruler_color(visuals)),
    );
}

fn column_ruler_color(visuals: &egui::Visuals) -> Color32 {
    visuals.widgets.noninteractive.bg_stroke.color
}

fn paint_indent_guides(
    painter: &egui::Painter,
    rect: egui::Rect,
    text: &str,
    row: &EditorRowContext<'_>,
    visuals: &egui::Visuals,
) {
    if !row.indent_guides {
        return;
    }

    let active_column = row.active_indent_guide_column;
    visit_leading_indent_guide_columns(text, row.tab_width, |column| {
        let x = rect.left() + row.gutter_width + column as f32 * row.char_width;
        if x >= rect.right() {
            return false;
        }
        let is_active = active_column == Some(column);
        let color = indent_guide_color(visuals, is_active);
        painter.line_segment(
            [pos2(x, rect.top() + 2.0), pos2(x, rect.bottom() - 2.0)],
            egui::Stroke::new(if is_active { 1.5 } else { 1.0 }, color),
        );
        true
    });
}

fn indent_guide_color(visuals: &egui::Visuals, active: bool) -> Color32 {
    if active {
        visuals.selection.stroke.color
    } else {
        visuals.widgets.inactive.bg_stroke.color
    }
}

pub(crate) fn active_indent_guide_column_for_buffer(
    buffer: &TextBuffer,
    tab_width: usize,
) -> Option<usize> {
    let cursor = buffer.cursor_position();
    active_indent_guide_column_from_visual_width(
        buffer.line_leading_indent_visual_width_capped(cursor.line, cursor.column, tab_width)?,
        tab_width,
    )
}

#[cfg(test)]
pub(crate) fn active_indent_guide_column(
    line_text: &str,
    cursor_column: usize,
    tab_width: usize,
) -> Option<usize> {
    active_indent_guide_column_from_chars(line_text.chars(), cursor_column, tab_width)
}

#[cfg(test)]
fn active_indent_guide_column_from_chars(
    chars: impl Iterator<Item = char>,
    cursor_column: usize,
    tab_width: usize,
) -> Option<usize> {
    let tab_width = tab_width.max(1);
    let mut column = 0usize;
    let mut next_guide = tab_width;
    let mut active = None;
    for (offset, ch) in chars.enumerate() {
        if offset >= cursor_column || matches!(ch, '\r' | '\n') {
            break;
        }
        match ch {
            ' ' => column += 1,
            '\t' => {
                let remainder = column % tab_width;
                column += if remainder == 0 {
                    tab_width
                } else {
                    tab_width - remainder
                };
            }
            _ => break,
        }

        while next_guide <= column {
            active = Some(next_guide);
            next_guide += tab_width;
        }
    }
    active
}

fn active_indent_guide_column_from_visual_width(width: usize, tab_width: usize) -> Option<usize> {
    let tab_width = tab_width.max(1);
    let column = width / tab_width * tab_width;
    (column > 0).then_some(column)
}

#[cfg(test)]
pub(crate) fn leading_indent_guide_columns(text: &str, tab_width: usize) -> Vec<usize> {
    let mut columns = Vec::new();
    visit_leading_indent_guide_columns(text, tab_width, |column| {
        columns.push(column);
        true
    });
    columns
}

fn visit_leading_indent_guide_columns(
    text: &str,
    tab_width: usize,
    mut visit: impl FnMut(usize) -> bool,
) {
    let tab_width = tab_width.max(1);
    let mut column = 0usize;
    let mut next_guide = tab_width;
    for ch in text.chars() {
        match ch {
            ' ' => column += 1,
            '\t' => {
                let remainder = column % tab_width;
                column += if remainder == 0 {
                    tab_width
                } else {
                    tab_width - remainder
                };
            }
            _ => break,
        }

        while next_guide <= column {
            if !visit(next_guide) {
                return;
            }
            next_guide += tab_width;
        }
    }
}

fn paint_control_character_markers(
    ui: &egui::Ui,
    painter: &egui::Painter,
    rect: egui::Rect,
    text_pos: egui::Pos2,
    text: &str,
    text_metrics: &RowTextMetrics,
    row: &EditorRowContext<'_>,
) {
    if !row.render_control_characters || !text_metrics.has_control_characters {
        return;
    }

    let color = ui.visuals().weak_text_color();
    let font = FontId::new((row.font_size * 0.68).max(8.0), FontFamily::Monospace);
    let tab_width = row.tab_width.max(1);
    let mut visual_col = 0usize;
    for ch in text.chars() {
        if let Some(label) = control_character_label(ch) {
            painter.text(
                pos2(
                    text_pos.x + visual_col as f32 * row.char_width,
                    rect.top() + row.row_height * 0.18,
                ),
                egui::Align2::LEFT_TOP,
                label,
                font.clone(),
                color,
            );
        }
        visual_col = visual_col.saturating_add(visual_width_for_char(ch, visual_col, tab_width));
    }
}

pub(crate) fn control_character_label(ch: char) -> Option<&'static str> {
    match ch {
        '\u{0000}' => Some("NUL"),
        '\u{0001}' => Some("SOH"),
        '\u{0002}' => Some("STX"),
        '\u{0003}' => Some("ETX"),
        '\u{0004}' => Some("EOT"),
        '\u{0005}' => Some("ENQ"),
        '\u{0006}' => Some("ACK"),
        '\u{0007}' => Some("BEL"),
        '\u{0008}' => Some("BS"),
        '\u{000B}' => Some("VT"),
        '\u{000C}' => Some("FF"),
        '\u{000E}' => Some("SO"),
        '\u{000F}' => Some("SI"),
        '\u{001B}' => Some("ESC"),
        '\u{007F}' => Some("DEL"),
        ch if ch.is_control() && ch != '\t' => Some("CTL"),
        _ => None,
    }
}

fn paint_whitespace_markers(
    ui: &egui::Ui,
    painter: &egui::Painter,
    rect: egui::Rect,
    text_pos: egui::Pos2,
    snapshot_range: &std::ops::Range<usize>,
    text: &str,
    text_metrics: &RowTextMetrics,
    line_end_visible: bool,
    row: &EditorRowContext<'_>,
) {
    let Some(strategy) = whitespace_marker_paint_strategy(
        row.experimental_whitespace_rendering,
        row.render_whitespace,
    ) else {
        return;
    };

    let whitespace_selection_ranges = whitespace_selection_ranges_for_marker_scan(
        row.render_whitespace,
        row.selections,
        snapshot_range,
        text_metrics,
    );
    if !whitespace_marker_scan_needed_for_ranges(
        row.render_whitespace,
        whitespace_selection_ranges.as_deref(),
        text_metrics,
    ) {
        return;
    }

    let trailing_start =
        whitespace_marker_trailing_start(row.render_whitespace, text_metrics, line_end_visible);
    let color = ui.visuals().weak_text_color();
    let font = match strategy {
        WhitespaceMarkerPaintStrategy::Svg => None,
        WhitespaceMarkerPaintStrategy::Font => Some(FontId::new(
            (row.font_size * 0.78).max(8.0),
            FontFamily::Monospace,
        )),
    };
    let mut previous_whitespace = false;
    let mut chars = text.chars().enumerate().peekable();
    let tab_width = row.tab_width.max(1);
    let mut visual_col = 0usize;
    let mut selection_range_cursor =
        SelectionRangeCursor::new(whitespace_selection_ranges.as_deref().unwrap_or(&[]));

    while let Some((col, ch)) = chars.next() {
        let next_whitespace = chars
            .peek()
            .map(|(_, next)| renderable_whitespace(*next))
            .unwrap_or(false);
        if let Some(kind) = whitespace_marker_kind_for_selection_cursor(
            row.render_whitespace,
            ch,
            col,
            snapshot_range.start + col,
            trailing_start,
            previous_whitespace,
            next_whitespace,
            &mut selection_range_cursor,
        ) {
            let marker = WhitespaceMarker { strategy, kind };
            paint_whitespace_marker(
                painter,
                rect,
                text_pos,
                visual_col,
                row,
                marker,
                font.as_ref(),
                color,
            );
        }
        previous_whitespace = renderable_whitespace(ch);
        visual_col = visual_col.saturating_add(visual_width_for_char(ch, visual_col, tab_width));
    }
}

#[cfg(test)]
fn whitespace_marker_scan_needed(
    mode: EditorRenderWhitespace,
    selections: &[Selection],
    snapshot_range: &std::ops::Range<usize>,
    text_metrics: &RowTextMetrics,
) -> bool {
    let selection_ranges =
        whitespace_selection_ranges_for_marker_scan(mode, selections, snapshot_range, text_metrics);
    whitespace_marker_scan_needed_for_ranges(mode, selection_ranges.as_deref(), text_metrics)
}

fn whitespace_marker_scan_needed_for_ranges(
    mode: EditorRenderWhitespace,
    selection_ranges: Option<&[Range<usize>]>,
    text_metrics: &RowTextMetrics,
) -> bool {
    text_metrics.has_renderable_whitespace
        && whitespace_selection_marker_scan_needed_for_ranges(mode, selection_ranges)
}

fn whitespace_mode_uses_trailing_start(mode: EditorRenderWhitespace) -> bool {
    matches!(
        mode,
        EditorRenderWhitespace::Trailing | EditorRenderWhitespace::Boundary
    )
}

fn whitespace_marker_trailing_start(
    mode: EditorRenderWhitespace,
    text_metrics: &RowTextMetrics,
    line_end_visible: bool,
) -> usize {
    if whitespace_mode_uses_trailing_start(mode) && line_end_visible {
        text_metrics.trailing_whitespace_start
    } else {
        usize::MAX
    }
}

#[cfg(test)]
fn whitespace_selection_marker_scan_needed(
    mode: EditorRenderWhitespace,
    selections: &[Selection],
    snapshot_range: &std::ops::Range<usize>,
) -> bool {
    let selection_ranges =
        whitespace_selection_ranges_for_snapshot(mode, selections, snapshot_range);
    whitespace_selection_marker_scan_needed_for_ranges(mode, selection_ranges.as_deref())
}

fn whitespace_selection_marker_scan_needed_for_ranges(
    mode: EditorRenderWhitespace,
    selection_ranges: Option<&[Range<usize>]>,
) -> bool {
    if mode != EditorRenderWhitespace::Selection {
        return true;
    }

    selection_ranges.is_some_and(|ranges| !ranges.is_empty())
}

fn whitespace_selection_ranges_for_snapshot(
    mode: EditorRenderWhitespace,
    selections: &[Selection],
    snapshot_range: &Range<usize>,
) -> Option<Vec<Range<usize>>> {
    (mode == EditorRenderWhitespace::Selection).then(|| {
        selection_ranges_for_snapshot(
            selections,
            snapshot_range,
            MAX_WHITESPACE_SELECTION_RANGES_PER_ROW,
        )
    })
}

fn whitespace_selection_ranges_for_marker_scan(
    mode: EditorRenderWhitespace,
    selections: &[Selection],
    snapshot_range: &Range<usize>,
    text_metrics: &RowTextMetrics,
) -> Option<Vec<Range<usize>>> {
    if !text_metrics.has_renderable_whitespace {
        return None;
    }

    whitespace_selection_ranges_for_snapshot(mode, selections, snapshot_range)
}

fn selection_ranges_for_snapshot(
    selections: &[Selection],
    snapshot_range: &Range<usize>,
    max_ranges: usize,
) -> Vec<Range<usize>> {
    if max_ranges == 0 {
        return Vec::new();
    }

    let mut ranges = Vec::with_capacity(max_ranges.min(selections.len()));
    let mut ordered = true;
    for selection in selections {
        let Some(range) = selection_range_for_snapshot(*selection, snapshot_range) else {
            continue;
        };
        ordered &= ranges
            .last()
            .is_none_or(|previous: &Range<usize>| previous.start <= range.start);
        ranges.push(range);
        if ranges.len() >= max_ranges {
            break;
        }
    }
    if !ordered {
        ranges.sort_unstable_by_key(|range| (range.start, range.end));
    }
    ranges
}

fn selection_range_for_snapshot(
    selection: Selection,
    snapshot_range: &Range<usize>,
) -> Option<Range<usize>> {
    if selection.is_caret() {
        return None;
    }

    let range = selection.range();
    let start = range.start.max(snapshot_range.start);
    let end = range.end.min(snapshot_range.end);
    (start < end).then_some(start..end)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WhitespaceMarker {
    pub(crate) strategy: WhitespaceMarkerPaintStrategy,
    pub(crate) kind: WhitespaceMarkerKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WhitespaceMarkerPaintStrategy {
    Svg,
    Font,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WhitespaceMarkerKind {
    Space,
    Tab,
}

impl WhitespaceMarkerKind {
    fn label(self) -> &'static str {
        match self {
            Self::Space => ".",
            Self::Tab => ">",
        }
    }
}

pub(crate) fn whitespace_marker_paint_strategy(
    rendering: EditorExperimentalWhitespaceRendering,
    mode: EditorRenderWhitespace,
) -> Option<WhitespaceMarkerPaintStrategy> {
    if mode == EditorRenderWhitespace::None {
        return None;
    }

    match rendering {
        EditorExperimentalWhitespaceRendering::Svg => Some(WhitespaceMarkerPaintStrategy::Svg),
        EditorExperimentalWhitespaceRendering::Font => Some(WhitespaceMarkerPaintStrategy::Font),
        EditorExperimentalWhitespaceRendering::Off => None,
    }
}

#[cfg(test)]
pub(crate) fn visible_whitespace_marker(
    rendering: EditorExperimentalWhitespaceRendering,
    mode: EditorRenderWhitespace,
    ch: char,
    col: usize,
    char_idx: usize,
    trailing_start: usize,
    previous_whitespace: bool,
    next_whitespace: bool,
    selections: &[Selection],
) -> Option<WhitespaceMarker> {
    Some(WhitespaceMarker {
        strategy: whitespace_marker_paint_strategy(rendering, mode)?,
        kind: whitespace_marker_kind(
            mode,
            ch,
            col,
            char_idx,
            trailing_start,
            previous_whitespace,
            next_whitespace,
            selections,
        )?,
    })
}

fn paint_whitespace_marker(
    painter: &egui::Painter,
    rect: egui::Rect,
    text_pos: egui::Pos2,
    visual_col: usize,
    row: &EditorRowContext<'_>,
    marker: WhitespaceMarker,
    font: Option<&FontId>,
    color: Color32,
) {
    match marker.strategy {
        WhitespaceMarkerPaintStrategy::Svg => {
            paint_svg_whitespace_marker(
                painter,
                rect,
                text_pos,
                visual_col,
                row,
                marker.kind,
                color,
            );
        }
        WhitespaceMarkerPaintStrategy::Font => {
            let Some(font) = font else {
                return;
            };
            painter.text(
                pos2(
                    text_pos.x + visual_col as f32 * row.char_width + row.char_width * 0.38,
                    rect.top() + 4.0,
                ),
                egui::Align2::LEFT_TOP,
                marker.kind.label(),
                font.clone(),
                color,
            );
        }
    }
}

fn paint_svg_whitespace_marker(
    painter: &egui::Painter,
    rect: egui::Rect,
    text_pos: egui::Pos2,
    visual_col: usize,
    row: &EditorRowContext<'_>,
    kind: WhitespaceMarkerKind,
    color: Color32,
) {
    let center = pos2(
        text_pos.x + visual_col as f32 * row.char_width + row.char_width * 0.5,
        rect.top() + row.row_height * 0.56,
    );

    match kind {
        WhitespaceMarkerKind::Space => {
            painter.circle_filled(center, (row.char_width * 0.09).clamp(1.0, 1.8), color);
        }
        WhitespaceMarkerKind::Tab => {
            let stroke = Stroke::new(1.0, color);
            let tail = pos2(center.x - row.char_width * 0.22, center.y);
            let head = pos2(center.x + row.char_width * 0.28, center.y);
            let arrow = (row.char_width * 0.13).clamp(1.0, 2.0);
            painter.line_segment([tail, head], stroke);
            painter.line_segment([head, pos2(head.x - arrow, head.y - arrow)], stroke);
            painter.line_segment([head, pos2(head.x - arrow, head.y + arrow)], stroke);
        }
    }
}

#[cfg(test)]
pub(crate) fn whitespace_marker_kind(
    mode: EditorRenderWhitespace,
    ch: char,
    col: usize,
    char_idx: usize,
    trailing_start: usize,
    previous_whitespace: bool,
    next_whitespace: bool,
    selections: &[Selection],
) -> Option<WhitespaceMarkerKind> {
    whitespace_marker_kind_with_selection_lookup(
        mode,
        ch,
        col,
        char_idx,
        trailing_start,
        previous_whitespace,
        next_whitespace,
        |char_idx| {
            selections.iter().any(|selection| {
                let range = selection.range();
                !selection.is_caret() && range.contains(&char_idx)
            })
        },
    )
}

#[cfg(test)]
fn whitespace_marker_kind_for_selection_ranges(
    mode: EditorRenderWhitespace,
    ch: char,
    col: usize,
    char_idx: usize,
    trailing_start: usize,
    previous_whitespace: bool,
    next_whitespace: bool,
    selection_ranges: &[Range<usize>],
) -> Option<WhitespaceMarkerKind> {
    whitespace_marker_kind_with_selection_lookup(
        mode,
        ch,
        col,
        char_idx,
        trailing_start,
        previous_whitespace,
        next_whitespace,
        |char_idx| {
            selection_ranges
                .iter()
                .any(|range| range.contains(&char_idx))
        },
    )
}

fn whitespace_marker_kind_for_selection_cursor(
    mode: EditorRenderWhitespace,
    ch: char,
    col: usize,
    char_idx: usize,
    trailing_start: usize,
    previous_whitespace: bool,
    next_whitespace: bool,
    selection_range_cursor: &mut SelectionRangeCursor<'_>,
) -> Option<WhitespaceMarkerKind> {
    whitespace_marker_kind_with_selection_lookup(
        mode,
        ch,
        col,
        char_idx,
        trailing_start,
        previous_whitespace,
        next_whitespace,
        |char_idx| selection_range_cursor.contains(char_idx),
    )
}

fn whitespace_marker_kind_with_selection_lookup(
    mode: EditorRenderWhitespace,
    ch: char,
    col: usize,
    char_idx: usize,
    trailing_start: usize,
    previous_whitespace: bool,
    next_whitespace: bool,
    selection_contains_char: impl FnOnce(usize) -> bool,
) -> Option<WhitespaceMarkerKind> {
    if !renderable_whitespace(ch) {
        return None;
    }

    let visible = whitespace_marker_visible(
        mode,
        col,
        char_idx,
        trailing_start,
        ch == '\t',
        previous_whitespace,
        next_whitespace,
        selection_contains_char,
    );

    visible.then_some(if ch == '\t' {
        WhitespaceMarkerKind::Tab
    } else {
        WhitespaceMarkerKind::Space
    })
}

#[cfg(test)]
pub(crate) fn whitespace_marker_label(
    mode: EditorRenderWhitespace,
    ch: char,
    col: usize,
    char_idx: usize,
    trailing_start: usize,
    previous_whitespace: bool,
    next_whitespace: bool,
    selections: &[Selection],
) -> Option<&'static str> {
    whitespace_marker_kind(
        mode,
        ch,
        col,
        char_idx,
        trailing_start,
        previous_whitespace,
        next_whitespace,
        selections,
    )
    .map(WhitespaceMarkerKind::label)
}

fn whitespace_marker_visible(
    mode: EditorRenderWhitespace,
    col: usize,
    char_idx: usize,
    trailing_start: usize,
    tab: bool,
    previous_whitespace: bool,
    next_whitespace: bool,
    selection_contains_char: impl FnOnce(usize) -> bool,
) -> bool {
    match mode {
        EditorRenderWhitespace::None => false,
        EditorRenderWhitespace::All => true,
        EditorRenderWhitespace::Trailing => col >= trailing_start,
        EditorRenderWhitespace::Selection => selection_contains_char(char_idx),
        EditorRenderWhitespace::Boundary => {
            tab || col >= trailing_start || col == 0 || previous_whitespace || next_whitespace
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct SelectionRangeCursor<'a> {
    ranges: &'a [Range<usize>],
    index: usize,
}

impl<'a> SelectionRangeCursor<'a> {
    fn new(ranges: &'a [Range<usize>]) -> Self {
        Self { ranges, index: 0 }
    }

    fn contains(&mut self, char_idx: usize) -> bool {
        while let Some(range) = self.ranges.get(self.index) {
            if range.end > char_idx {
                return range.start <= char_idx;
            }
            self.index += 1;
        }
        false
    }
}

pub(crate) fn editor_row_wrap_width(
    row_width: f32,
    gutter_width: f32,
    word_wrap: EditorWordWrap,
    word_wrap_column: usize,
    char_width: f32,
) -> f32 {
    let viewport_width =
        (finite_nonnegative_extent(row_width) - finite_nonnegative_extent(gutter_width)).max(120.0);
    let char_width = if char_width.is_finite() && char_width > 0.0 {
        char_width
    } else {
        8.0
    };
    let column_width = saturated_f32_from_f64(
        clamp_editor_word_wrap_column(word_wrap_column) as f64 * char_width as f64,
    )
    .max(120.0);
    match word_wrap {
        EditorWordWrap::Off => f32::INFINITY,
        EditorWordWrap::On => viewport_width,
        EditorWordWrap::WordWrapColumn => column_width,
        EditorWordWrap::Bounded => viewport_width.min(column_width).max(120.0),
    }
}

fn finite_nonnegative_extent(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

fn saturated_f32_from_f64(value: f64) -> f32 {
    if !value.is_finite() || value > f32::MAX as f64 {
        f32::MAX
    } else if value < -(f32::MAX as f64) {
        -f32::MAX
    } else {
        value as f32
    }
}

pub(crate) fn limit_layout_job_line_rendering(
    job: &mut egui::text::LayoutJob,
    stop_rendering_line_after: i64,
) {
    let Some(limit) = editor_stop_rendering_line_after_limit(stop_rendering_line_after) else {
        return;
    };
    let Some(cut_byte) = byte_index_after_chars(&job.text, limit) else {
        return;
    };

    job.text.truncate(cut_byte);
    for section in &mut job.sections {
        let start = section.byte_range.start.min(cut_byte);
        let end = section.byte_range.end.min(cut_byte);
        section.byte_range = start..end;
    }
    job.sections
        .retain(|section| section.byte_range.start < section.byte_range.end);
}

fn byte_index_after_chars(text: &str, max_chars: usize) -> Option<usize> {
    if max_chars == 0 {
        return (!text.is_empty()).then_some(0);
    }
    text.char_indices().nth(max_chars).map(|(index, _)| index)
}

#[cfg(test)]
pub(crate) fn trailing_whitespace_start(text: &str) -> usize {
    let mut count = 0usize;
    let mut after_last_non_whitespace = 0usize;
    for ch in text.chars() {
        count += 1;
        if !renderable_whitespace(ch) {
            after_last_non_whitespace = count;
        }
    }
    after_last_non_whitespace
}

#[cfg(test)]
pub(crate) fn rendered_trailing_whitespace_start(text: &str, line_end_visible: bool) -> usize {
    if line_end_visible {
        trailing_whitespace_start(text)
    } else {
        text.chars().count()
    }
}

fn renderable_whitespace(ch: char) -> bool {
    matches!(ch, ' ' | '\t')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RowTextMetrics {
    char_count: usize,
    visual_width: usize,
    has_renderable_whitespace: bool,
    has_control_characters: bool,
    trailing_whitespace_start: usize,
}

fn row_text_metrics(text: &str, tab_width: usize) -> RowTextMetrics {
    let tab_width = tab_width.max(1);
    if text.is_ascii() {
        return ascii_text_metrics(text, tab_width);
    }

    let mut char_count = 0usize;
    let mut visual_width = 0usize;
    let mut has_renderable_whitespace = false;
    let mut has_control_characters = false;
    let mut trailing_whitespace_start = 0usize;
    for ch in text.chars() {
        char_count += 1;
        if renderable_whitespace(ch) {
            has_renderable_whitespace = true;
        } else {
            trailing_whitespace_start = char_count;
        }
        if control_character_label(ch).is_some() {
            has_control_characters = true;
        }
        visual_width =
            visual_width.saturating_add(visual_width_for_char(ch, visual_width, tab_width));
    }
    RowTextMetrics {
        char_count,
        visual_width,
        has_renderable_whitespace,
        has_control_characters,
        trailing_whitespace_start,
    }
}

fn ascii_text_metrics(text: &str, tab_width: usize) -> RowTextMetrics {
    let mut visual_width = 0usize;
    let mut has_renderable_whitespace = false;
    let mut has_control_characters = false;
    let mut trailing_whitespace_start = 0usize;
    for (char_idx, byte) in text.as_bytes().iter().copied().enumerate() {
        if renderable_whitespace_byte(byte) {
            has_renderable_whitespace = true;
        } else {
            trailing_whitespace_start = char_idx + 1;
        }
        if byte.is_ascii_control() && byte != b'\t' {
            has_control_characters = true;
        }
        visual_width = if byte == b'\t' {
            visual_width.saturating_add(visual_width_for_char('\t', visual_width, tab_width))
        } else {
            visual_width.saturating_add(1)
        }
    }
    RowTextMetrics {
        char_count: text.len(),
        visual_width,
        has_renderable_whitespace,
        has_control_characters,
        trailing_whitespace_start,
    }
}

fn renderable_whitespace_byte(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t')
}

fn editor_cursor_visible(ui: &egui::Ui, blinking: bool) -> bool {
    if !blinking {
        return true;
    }

    ui.ctx().request_repaint_after(Duration::from_millis(120));
    ui.input(|input| input.time % 1.0 < 0.55)
}

#[cfg(test)]
mod tests;
