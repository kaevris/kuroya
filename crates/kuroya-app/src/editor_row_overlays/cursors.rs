use crate::{
    editor_pane_rows::EditorRowContext,
    editor_text_geometry::{
        visual_column_for_char_offset, visual_width_for_char, visual_x_for_char_idx,
    },
};
use eframe::egui::{self, Align2, Color32, FontFamily, FontId, Pos2, Rect, Visuals, pos2, vec2};
use kuroya_core::{EditorCursorSmoothCaretAnimation, EditorCursorStyle};
use std::ops::Range;

const SMOOTH_CARET_ANIMATION_SECONDS: f32 = 0.08;

pub(crate) fn paint_cursors(
    ui: &egui::Ui,
    painter: &egui::Painter,
    text_pos: Pos2,
    rect: egui::Rect,
    snapshot_range: &Range<usize>,
    line_text: &str,
    line_idx: usize,
    row: &EditorRowContext<'_>,
) {
    if !cursor_overlay_geometry_is_valid(
        rect,
        text_pos.x,
        row.row_height,
        row.char_width,
        row.cursor_width,
    ) {
        return;
    }

    for (cursor_slot, cursor) in row
        .cursor_positions
        .iter()
        .enumerate()
        .filter(|(_, cursor)| cursor.line == line_idx)
    {
        let cursor_rect = insertion_cursor_rect(
            text_pos.x,
            rect.top(),
            row.row_height,
            line_text,
            snapshot_range.start,
            cursor.char_idx,
            row.tab_width,
            row.char_width,
            row.cursor_width,
            row.cursor_height,
        );
        let cursor_rect = animated_cursor_rect(ui.ctx(), row, cursor_slot, cursor_rect);
        let cursor_x = cursor_rect.left();
        let color = ui.visuals().text_color();
        let full_height = (row.row_height - 4.0).max(1.0);
        match row.cursor_style {
            EditorCursorStyle::Line | EditorCursorStyle::LineThin => {
                let width = if matches!(row.cursor_style, EditorCursorStyle::LineThin) {
                    1.0
                } else {
                    row.cursor_width
                };
                painter.rect_filled(
                    egui::Rect::from_min_size(cursor_rect.min, vec2(width, cursor_rect.height())),
                    0.0,
                    color,
                );
            }
            EditorCursorStyle::Block => {
                let block_rect = egui::Rect::from_min_size(
                    pos2(cursor_x, rect.top() + 2.0),
                    vec2(
                        cursor_block_width(
                            line_text,
                            snapshot_range.start,
                            cursor.char_idx,
                            row.char_width,
                            row.cursor_width,
                            row.tab_width,
                        ),
                        full_height,
                    ),
                );
                let fill = opaque_color(color);
                painter.rect_filled(block_rect, 1.0, fill);

                if let Some(cell) =
                    cursor_block_cell_text(line_text, snapshot_range.start, cursor.char_idx)
                {
                    painter.text(
                        pos2(cursor_x, rect.top() + 3.0),
                        Align2::LEFT_TOP,
                        cell,
                        FontId::new(row.font_size, FontFamily::Monospace),
                        cursor_block_text_color(ui.visuals(), fill),
                    );
                }
            }
            EditorCursorStyle::BlockOutline => {
                painter.rect_stroke(
                    egui::Rect::from_min_size(
                        pos2(cursor_x, rect.top() + 2.0),
                        vec2(row.char_width.max(row.cursor_width), full_height),
                    ),
                    1.0,
                    egui::Stroke::new(1.0_f32, color),
                    egui::StrokeKind::Inside,
                );
            }
            EditorCursorStyle::Underline | EditorCursorStyle::UnderlineThin => {
                let height = if matches!(row.cursor_style, EditorCursorStyle::UnderlineThin) {
                    1.0
                } else {
                    row.cursor_width.max(1.0)
                };
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        pos2(cursor_x, rect.bottom() - height - 2.0),
                        vec2(row.char_width.max(4.0), height),
                    ),
                    0.0,
                    color,
                );
            }
        }
    }
}

fn opaque_color(color: Color32) -> Color32 {
    Color32::from_rgb(color.r(), color.g(), color.b())
}

fn cursor_block_text_color(visuals: &Visuals, block_fill: Color32) -> Color32 {
    let fill_luma = relative_luma(block_fill);
    let extreme = opaque_color(visuals.extreme_bg_color);
    let panel = opaque_color(visuals.panel_fill);

    if (relative_luma(extreme) - fill_luma).abs() >= (relative_luma(panel) - fill_luma).abs() {
        extreme
    } else {
        panel
    }
}

fn relative_luma(color: Color32) -> f32 {
    0.2126 * f32::from(color.r()) + 0.7152 * f32::from(color.g()) + 0.0722 * f32::from(color.b())
}

fn cursor_block_width(
    line_text: &str,
    snapshot_start: usize,
    cursor_char_idx: usize,
    char_width: f32,
    cursor_width: f32,
    tab_width: usize,
) -> f32 {
    let char_offset = cursor_char_idx.saturating_sub(snapshot_start);
    let visual_col = visual_column_for_char_offset(line_text, char_offset, tab_width);
    let visual_cols = line_text
        .chars()
        .nth(char_offset)
        .map(|ch| visual_width_for_char(ch, visual_col, tab_width))
        .filter(|width| *width > 0)
        .unwrap_or(1);

    (char_width * visual_cols as f32).max(cursor_width)
}

fn cursor_block_cell_text(
    line_text: &str,
    snapshot_start: usize,
    cursor_char_idx: usize,
) -> Option<String> {
    let char_offset = cursor_char_idx.checked_sub(snapshot_start)?;
    let ch = line_text.chars().nth(char_offset)?;
    let visual_col = visual_column_for_char_offset(line_text, char_offset, 1);

    if ch == '\t' || ch.is_control() || visual_width_for_char(ch, visual_col, 1) == 0 {
        return None;
    }

    Some(ch.to_string())
}

fn animated_cursor_rect(
    ctx: &egui::Context,
    row: &EditorRowContext<'_>,
    cursor_slot: usize,
    cursor_rect: Rect,
) -> Rect {
    let Some(animation_seconds) =
        cursor_smooth_caret_animation_seconds(row.cursor_smooth_caret_animation, row.focused)
    else {
        return cursor_rect;
    };
    let id = egui::Id::new(("editor_cursor", row.buffer.id(), cursor_slot));
    let x = ctx.animate_value_with_time(id.with("x"), cursor_rect.left(), animation_seconds);
    let y = ctx.animate_value_with_time(id.with("y"), cursor_rect.top(), animation_seconds);
    cursor_rect.translate(vec2(x - cursor_rect.left(), y - cursor_rect.top()))
}

fn cursor_smooth_caret_animation_seconds(
    mode: EditorCursorSmoothCaretAnimation,
    focused: bool,
) -> Option<f32> {
    match mode {
        EditorCursorSmoothCaretAnimation::Off => None,
        EditorCursorSmoothCaretAnimation::Explicit if focused => {
            Some(SMOOTH_CARET_ANIMATION_SECONDS)
        }
        EditorCursorSmoothCaretAnimation::Explicit => None,
        EditorCursorSmoothCaretAnimation::On => Some(SMOOTH_CARET_ANIMATION_SECONDS),
    }
}

pub(crate) fn primary_insertion_cursor_rect(
    text_pos: Pos2,
    row_rect: Rect,
    snapshot_range: &Range<usize>,
    line_text: &str,
    line_idx: usize,
    row: &EditorRowContext<'_>,
) -> Option<Rect> {
    if !cursor_overlay_geometry_is_valid(
        row_rect,
        text_pos.x,
        row.row_height,
        row.char_width,
        row.cursor_width,
    ) {
        return None;
    }

    let cursor = row
        .cursor_positions
        .last()
        .filter(|cursor| cursor.line == line_idx)?;
    Some(insertion_cursor_rect(
        text_pos.x,
        row_rect.top(),
        row.row_height,
        line_text,
        snapshot_range.start,
        cursor.char_idx,
        row.tab_width,
        row.char_width,
        row.cursor_width,
        row.cursor_height,
    ))
}

pub(crate) fn insertion_cursor_rect(
    text_pos_x: f32,
    row_top: f32,
    row_height: f32,
    line_text: &str,
    snapshot_start: usize,
    cursor_char_idx: usize,
    tab_width: usize,
    char_width: f32,
    cursor_width: f32,
    cursor_height: usize,
) -> Rect {
    let text_pos_x = stable_finite_coordinate(text_pos_x);
    let row_top = stable_finite_coordinate(row_top);
    let row_height = stable_positive_extent(row_height, 1.0);
    let char_width = stable_positive_extent(char_width, 8.0);
    let cursor_width = stable_positive_extent(cursor_width, 1.0);
    let full_height = (row_height - 4.0).max(1.0);
    let line_height = if cursor_height == 0 {
        full_height
    } else {
        (cursor_height as f32).clamp(1.0, full_height)
    };
    let line_y = row_top + 2.0 + ((full_height - line_height) / 2.0);
    let cursor_x = visual_x_for_char_idx(
        text_pos_x,
        line_text,
        cursor_char_idx,
        snapshot_start,
        tab_width,
        char_width,
    );
    Rect::from_min_size(
        pos2(cursor_x, line_y),
        vec2(cursor_width.max(1.0), line_height),
    )
}

fn cursor_overlay_geometry_is_valid(
    rect: egui::Rect,
    text_pos_x: f32,
    row_height: f32,
    char_width: f32,
    cursor_width: f32,
) -> bool {
    rect.left().is_finite()
        && rect.top().is_finite()
        && rect.right().is_finite()
        && rect.bottom().is_finite()
        && rect.right() >= rect.left()
        && rect.bottom() >= rect.top()
        && text_pos_x.is_finite()
        && row_height.is_finite()
        && row_height > 0.0
        && char_width.is_finite()
        && char_width > 0.0
        && cursor_width.is_finite()
        && cursor_width > 0.0
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

#[cfg(test)]
mod tests {
    use super::{
        cursor_block_cell_text, cursor_block_text_color, cursor_block_width,
        cursor_overlay_geometry_is_valid, cursor_smooth_caret_animation_seconds,
        insertion_cursor_rect, opaque_color,
    };
    use eframe::egui::{Color32, Rect, Visuals, pos2};
    use kuroya_core::EditorCursorSmoothCaretAnimation;

    #[test]
    fn insertion_cursor_rect_tracks_tabs_unicode_and_height() {
        let tabbed = insertion_cursor_rect(10.0, 20.0, 18.0, "\tab", 0, 2, 4, 8.0, 2.0, 0);
        assert_eq!(tabbed.left(), 50.0);
        assert_eq!(tabbed.top(), 22.0);
        assert_eq!(tabbed.height(), 14.0);

        let unicode = insertion_cursor_rect(10.0, 20.0, 18.0, "e\u{0301}x", 0, 2, 4, 8.0, 2.0, 8);
        assert_eq!(unicode.left(), 18.0);
        assert_eq!(unicode.top(), 25.0);
        assert_eq!(unicode.height(), 8.0);

        let clipped = insertion_cursor_rect(10.0, 20.0, 18.0, "abcdef", 3, 6, 4, 8.0, 0.0, 99);
        assert_eq!(clipped.left(), 34.0);
        assert_eq!(clipped.width(), 1.0);
        assert_eq!(clipped.height(), 14.0);
    }

    #[test]
    fn block_cursor_width_tracks_tabs_and_line_end() {
        assert_eq!(cursor_block_width("\tab", 0, 0, 8.0, 2.0, 4), 32.0);
        assert_eq!(cursor_block_width("abc", 0, 1, 8.0, 2.0, 4), 8.0);
        assert_eq!(cursor_block_width("abc", 0, 3, 8.0, 2.0, 4), 8.0);
        assert_eq!(cursor_block_width("abc", 0, 1, 8.0, 10.0, 4), 10.0);
    }

    #[test]
    fn block_cursor_cell_text_skips_blank_cells() {
        assert_eq!(
            cursor_block_cell_text("class Foo", 0, 0),
            Some("c".to_owned())
        );
        assert_eq!(cursor_block_cell_text("class Foo", 0, 9), None);
        assert_eq!(cursor_block_cell_text("\tFoo", 0, 0), None);
        assert_eq!(cursor_block_cell_text("e\u{0301}x", 0, 1), None);
    }

    #[test]
    fn block_cursor_uses_opaque_fill_and_contrasting_text() {
        assert_eq!(
            opaque_color(Color32::from_rgba_premultiplied(10, 20, 30, 96)),
            Color32::from_rgb(10, 20, 30)
        );

        let visuals = Visuals {
            extreme_bg_color: Color32::WHITE,
            panel_fill: Color32::BLACK,
            ..Visuals::dark()
        };
        assert_eq!(
            cursor_block_text_color(&visuals, Color32::BLACK),
            Color32::WHITE
        );
    }

    #[test]
    fn insertion_cursor_rect_uses_finite_fallbacks_for_invalid_geometry() {
        let rect = insertion_cursor_rect(
            f32::NAN,
            f32::INFINITY,
            f32::NAN,
            "abc",
            0,
            2,
            4,
            f32::NAN,
            f32::INFINITY,
            0,
        );

        assert!(rect.left().is_finite());
        assert!(rect.top().is_finite());
        assert!(rect.width().is_finite());
        assert!(rect.height().is_finite());
        assert_eq!(rect.left(), 16.0);
        assert_eq!(rect.top(), 2.0);
        assert_eq!(rect.width(), 1.0);
        assert_eq!(rect.height(), 1.0);
    }

    #[test]
    fn cursor_overlay_geometry_rejects_non_finite_inputs() {
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(120.0, 18.0));

        assert!(cursor_overlay_geometry_is_valid(rect, 40.0, 18.0, 8.0, 2.0));
        assert!(!cursor_overlay_geometry_is_valid(
            rect,
            f32::NAN,
            18.0,
            8.0,
            2.0
        ));
        assert!(!cursor_overlay_geometry_is_valid(
            rect,
            40.0,
            f32::INFINITY,
            8.0,
            2.0
        ));
        assert!(!cursor_overlay_geometry_is_valid(
            rect, 40.0, 18.0, 0.0, 2.0
        ));
        assert!(!cursor_overlay_geometry_is_valid(
            rect, 40.0, 18.0, 8.0, -1.0
        ));
        assert!(!cursor_overlay_geometry_is_valid(
            Rect::from_min_max(pos2(120.0, 0.0), pos2(0.0, 18.0)),
            40.0,
            18.0,
            8.0,
            2.0
        ));
    }

    #[test]
    fn cursor_smooth_caret_animation_respects_mode_and_focus() {
        assert_eq!(
            cursor_smooth_caret_animation_seconds(EditorCursorSmoothCaretAnimation::Off, true),
            None
        );
        assert_eq!(
            cursor_smooth_caret_animation_seconds(
                EditorCursorSmoothCaretAnimation::Explicit,
                false
            ),
            None
        );
        assert!(
            cursor_smooth_caret_animation_seconds(EditorCursorSmoothCaretAnimation::Explicit, true)
                .is_some()
        );
        assert!(
            cursor_smooth_caret_animation_seconds(EditorCursorSmoothCaretAnimation::On, false)
                .is_some()
        );
    }
}
