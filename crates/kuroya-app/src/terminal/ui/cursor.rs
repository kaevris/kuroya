use egui::{Color32, Rect, Stroke, StrokeKind};
use kuroya_core::{TerminalCursorStyle, TerminalInactiveCursorStyle};
use std::time::Duration;

pub(super) fn terminal_cursor_visible(
    ui: &egui::Ui,
    has_focus: bool,
    cursor_blinking: bool,
) -> bool {
    if !has_focus || !cursor_blinking {
        return true;
    }

    ui.ctx().request_repaint_after(Duration::from_millis(120));
    ui.input(|input| {
        if input.time.is_finite() {
            input.time.rem_euclid(1.0) < 0.5
        } else {
            true
        }
    })
}

pub(super) fn draw_terminal_cursor(
    painter: &egui::Painter,
    rect: Rect,
    color: Color32,
    has_focus: bool,
    cursor_style: TerminalCursorStyle,
    cursor_width: f32,
    inactive_style: TerminalInactiveCursorStyle,
) {
    let Some(rect) = safe_cursor_rect(rect) else {
        return;
    };
    if color.a() == 0 {
        return;
    }

    if has_focus {
        match cursor_style {
            TerminalCursorStyle::Block => draw_block_cursor(painter, rect, color),
            TerminalCursorStyle::Line => draw_line_cursor(painter, rect, color, cursor_width),
            TerminalCursorStyle::Underline => draw_underline_cursor(painter, rect, color),
        }
        return;
    }

    match inactive_style {
        TerminalInactiveCursorStyle::Outline => {
            let stroke_rect = inset_rect_inside(rect, 1.0);
            painter.rect_stroke(
                stroke_rect,
                0.0,
                Stroke::new(1.2_f32, color),
                StrokeKind::Inside,
            );
        }
        TerminalInactiveCursorStyle::Block => draw_block_cursor(painter, rect, color),
        TerminalInactiveCursorStyle::Line => draw_line_cursor(painter, rect, color, cursor_width),
        TerminalInactiveCursorStyle::Underline => draw_underline_cursor(painter, rect, color),
        TerminalInactiveCursorStyle::None => {}
    }
}

fn draw_block_cursor(painter: &egui::Painter, rect: Rect, color: Color32) {
    let fill = Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), 94);
    let stroke_rect = inset_rect_inside(rect, 1.0);
    painter.rect_filled(stroke_rect, 0.0, fill);
    painter.rect_stroke(
        stroke_rect,
        0.0,
        Stroke::new(1.0_f32, color),
        StrokeKind::Inside,
    );
}

fn draw_line_cursor(painter: &egui::Painter, rect: Rect, color: Color32, cursor_width: f32) {
    let stroke_width = line_cursor_stroke_width(rect, cursor_width);
    let x = line_cursor_x(rect, stroke_width);
    let (top, bottom) = inset_axis_range(rect.top(), rect.bottom(), 1.0);
    painter.line_segment(
        [egui::pos2(x, top), egui::pos2(x, bottom)],
        Stroke::new(stroke_width, color),
    );
}

fn draw_underline_cursor(painter: &egui::Painter, rect: Rect, color: Color32) {
    let y = underline_cursor_y(rect);
    let (left, right) = inset_axis_range(rect.left(), rect.right(), 1.0);
    painter.line_segment(
        [egui::pos2(left, y), egui::pos2(right, y)],
        Stroke::new(2.0_f32, color),
    );
}

fn line_cursor_stroke_width(rect: Rect, cursor_width: f32) -> f32 {
    let requested = if cursor_width.is_finite() {
        cursor_width
    } else {
        1.0
    };
    let cell_width = if rect.width().is_finite() && rect.width() > 0.0 {
        rect.width()
    } else {
        1.0
    };
    requested.clamp(1.0, cell_width.max(1.0))
}

fn line_cursor_x(rect: Rect, stroke_width: f32) -> f32 {
    let cell_width = if rect.width().is_finite() && rect.width() > 0.0 {
        rect.width()
    } else {
        0.0
    };
    rect.left() + (stroke_width * 0.5).min(cell_width * 0.5).max(0.0)
}

fn underline_cursor_y(rect: Rect) -> f32 {
    let y = rect.bottom() - 2.0;
    if y.is_finite() && y >= rect.top() {
        y
    } else {
        rect.center().y
    }
}

fn inset_axis_range(start: f32, end: f32, inset: f32) -> (f32, f32) {
    if !start.is_finite() || !end.is_finite() || !inset.is_finite() {
        return (0.0, 0.0);
    }

    let inner_start = start + inset;
    let inner_end = end - inset;
    if inner_start <= inner_end {
        (inner_start, inner_end)
    } else {
        let center = start + (end - start) * 0.5;
        (center, center)
    }
}

fn safe_cursor_rect(rect: Rect) -> Option<Rect> {
    if !rect.min.x.is_finite()
        || !rect.min.y.is_finite()
        || !rect.max.x.is_finite()
        || !rect.max.y.is_finite()
        || rect.width() <= 0.0
        || rect.height() <= 0.0
    {
        return None;
    }

    Some(rect)
}

fn inset_rect_inside(rect: Rect, inset: f32) -> Rect {
    let inset = if inset.is_finite() && inset > 0.0 {
        inset
    } else {
        0.0
    };
    let x_inset = inset.min(rect.width() * 0.5);
    let y_inset = inset.min(rect.height() * 0.5);
    Rect::from_min_max(
        egui::pos2(rect.left() + x_inset, rect.top() + y_inset),
        egui::pos2(rect.right() - x_inset, rect.bottom() - y_inset),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        inset_axis_range, inset_rect_inside, line_cursor_stroke_width, line_cursor_x,
        safe_cursor_rect, underline_cursor_y,
    };
    use egui::{Rect, pos2, vec2};

    #[test]
    fn line_cursor_width_stays_inside_narrow_cells() {
        let rect = Rect::from_min_size(pos2(10.0, 0.0), vec2(4.0, 16.0));

        assert_eq!(line_cursor_stroke_width(rect, 8.0), 4.0);
        assert_eq!(line_cursor_x(rect, 4.0), 12.0);
    }

    #[test]
    fn line_cursor_width_uses_safe_default_for_invalid_width() {
        let rect = Rect::from_min_size(pos2(10.0, 0.0), vec2(6.0, 16.0));

        assert_eq!(line_cursor_stroke_width(rect, f32::NAN), 1.0);
        assert_eq!(line_cursor_x(rect, 1.0), 10.5);
    }

    #[test]
    fn cursor_segments_collapse_inside_tiny_cells() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(0.5, 1.0));

        assert_eq!(
            inset_axis_range(rect.left(), rect.right(), 1.0),
            (10.25, 10.25)
        );
        assert_eq!(
            inset_axis_range(rect.top(), rect.bottom(), 1.0),
            (20.5, 20.5)
        );
        assert_eq!(underline_cursor_y(rect), 20.5);
    }

    #[test]
    fn cursor_rects_reject_non_finite_or_empty_geometry() {
        assert!(safe_cursor_rect(Rect::from_min_size(pos2(0.0, 0.0), vec2(1.0, 1.0))).is_some());
        assert!(
            safe_cursor_rect(Rect::from_min_size(pos2(f32::NAN, 0.0), vec2(1.0, 1.0))).is_none()
        );
        assert!(safe_cursor_rect(Rect::from_min_size(pos2(0.0, 0.0), vec2(0.0, 1.0))).is_none());
    }

    #[test]
    fn cursor_stroke_rect_stays_inside_tiny_cells() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(0.5, 0.5));
        let stroke_rect = inset_rect_inside(rect, 1.0);

        assert_eq!(stroke_rect.min, pos2(10.25, 20.25));
        assert_eq!(stroke_rect.max, pos2(10.25, 20.25));
    }

    #[test]
    fn cursor_axis_insets_collapse_non_finite_ranges() {
        assert_eq!(inset_axis_range(f32::NAN, 10.0, 1.0), (0.0, 0.0));
        assert_eq!(inset_axis_range(0.0, f32::INFINITY, 1.0), (0.0, 0.0));
    }
}
