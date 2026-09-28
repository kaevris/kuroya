use crate::terminal_support::terminal_cell_size;
use egui::{Pos2, Rect, Vec2, pos2, vec2};

const TERMINAL_SCREEN_PADDING_X: f32 = 10.0;
const TERMINAL_SCREEN_PADDING_Y: f32 = 8.0;
pub(super) const TERMINAL_SPLIT_SEPARATOR_WIDTH: f32 = 7.0;
const TERMINAL_SPLIT_SEPARATOR_LINE_WIDTH: f32 = 1.0;
pub(super) const TERMINAL_MAX_LAYOUT_POINTS: f32 = 1_000_000.0;
const TERMINAL_FALLBACK_FONT_SIZE: f32 = 14.0;
const TERMINAL_MIN_FONT_SIZE: f32 = 1.0;
const TERMINAL_MAX_FONT_SIZE: f32 = 256.0;
const TERMINAL_MAX_CELL_POINTS: f32 = 512.0;
pub(super) const TERMINAL_PATH_LINK_SCAN_MAX_COLUMNS: u16 = 4096;

pub(super) fn terminal_mouse_wheel_zoom_modifier(modifiers: egui::Modifiers) -> bool {
    modifiers.command || modifiers.ctrl
}

pub(super) const TERMINAL_TAB_HEIGHT: f32 = 32.0;
pub(super) const TERMINAL_TAB_TRASH_BUTTON_SIZE: f32 = 20.0;
const TERMINAL_TAB_TRASH_BUTTON_MARGIN: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TerminalTabOrientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TerminalTabAnchor {
    Left,
    Right,
}

pub(super) fn terminal_tab_rects(
    origin: Pos2,
    tab_size: Vec2,
    spacing: f32,
    orientation: TerminalTabOrientation,
    anchor: TerminalTabAnchor,
    rail_width: f32,
    session_ids: &[usize],
) -> Vec<(usize, Rect)> {
    session_ids
        .iter()
        .enumerate()
        .map(|(index, session_id)| {
            let offset = index as f32
                * match orientation {
                    TerminalTabOrientation::Horizontal => tab_size.x + spacing,
                    TerminalTabOrientation::Vertical => tab_size.y + spacing,
                };
            let origin_x = bounded_terminal_layout_value(origin.x);
            let rail_width = bounded_terminal_layout_value(rail_width);
            let min_x = match anchor {
                TerminalTabAnchor::Left => origin_x,
                TerminalTabAnchor::Right => origin_x + (rail_width - tab_size.x).max(0.0),
            };
            let min = match orientation {
                TerminalTabOrientation::Horizontal => pos2(min_x + offset, origin.y),
                TerminalTabOrientation::Vertical => pos2(min_x, origin.y + offset),
            };
            (*session_id, Rect::from_min_size(min, tab_size))
        })
        .collect()
}

pub(super) fn terminal_tab_trash_rect(tab_rect: Rect, hovered: bool) -> Option<Rect> {
    let tab_rect = terminal_normalized_rect(tab_rect)?;
    if !hovered || tab_rect.width() <= 0.0 || tab_rect.height() <= 0.0 {
        return None;
    }

    let size = TERMINAL_TAB_TRASH_BUTTON_SIZE
        .min(tab_rect.width())
        .min(tab_rect.height());
    Some(Rect::from_center_size(
        pos2(
            tab_rect.right() - size * 0.5 - TERMINAL_TAB_TRASH_BUTTON_MARGIN,
            tab_rect.center().y,
        ),
        vec2(size, size),
    ))
}

pub(super) fn terminal_tab_click_targets_trash(
    trash_rect: Option<Rect>,
    pointer: Option<Pos2>,
) -> bool {
    let Some(trash_rect) = trash_rect else {
        return false;
    };
    let Some(pointer) = pointer else {
        return false;
    };
    terminal_rect_contains_pointer(trash_rect, pointer)
}

#[cfg(test)]
pub(super) fn terminal_tab_session_at(tab_rects: &[(usize, Rect)], pointer: Pos2) -> Option<usize> {
    if !pointer.x.is_finite() || !pointer.y.is_finite() {
        return None;
    }
    tab_rects
        .iter()
        .rev()
        .find(|(_, rect)| rect.contains(pointer))
        .map(|(session_id, _)| *session_id)
}

pub(super) fn terminal_link_click_modifier(modifiers: egui::Modifiers) -> bool {
    modifiers.command || modifiers.ctrl
}

pub(super) fn terminal_path_link_scan_allowed((rows, cols): (u16, u16)) -> bool {
    rows > 0 && cols > 0 && cols <= TERMINAL_PATH_LINK_SCAN_MAX_COLUMNS
}

pub(super) fn bounded_terminal_layout_value(value: f32) -> f32 {
    if value.is_nan() {
        0.0
    } else if value.is_finite() {
        value.clamp(0.0, TERMINAL_MAX_LAYOUT_POINTS)
    } else if value.is_sign_positive() {
        TERMINAL_MAX_LAYOUT_POINTS
    } else {
        0.0
    }
}

pub(super) fn bounded_terminal_layout_size(size: Vec2) -> Vec2 {
    vec2(
        bounded_terminal_layout_value(size.x),
        bounded_terminal_layout_value(size.y),
    )
}

pub(super) fn terminal_safe_font_size(font_size: f32) -> f32 {
    if font_size.is_finite() {
        font_size.clamp(TERMINAL_MIN_FONT_SIZE, TERMINAL_MAX_FONT_SIZE)
    } else {
        TERMINAL_FALLBACK_FONT_SIZE
    }
}

pub(super) fn terminal_safe_cell_size(
    font_size: f32,
    line_height: f32,
    letter_spacing: f32,
) -> (f32, f32) {
    let (cell_width, cell_height) = terminal_cell_size(font_size, line_height, letter_spacing);
    (
        terminal_positive_layout_value(cell_width, 1.0, TERMINAL_MAX_CELL_POINTS),
        terminal_positive_layout_value(cell_height, font_size, TERMINAL_MAX_CELL_POINTS),
    )
}

pub(super) fn terminal_positive_layout_value(value: f32, fallback: f32, max: f32) -> f32 {
    let value = if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    };
    value.clamp(1.0, max)
}

pub(super) fn terminal_content_rect(rect: Rect) -> Rect {
    let Some(rect) = terminal_normalized_rect(rect) else {
        return Rect::from_min_size(pos2(0.0, 0.0), Vec2::ZERO);
    };
    let x_inset = TERMINAL_SCREEN_PADDING_X.min(rect.width() * 0.5);
    let y_inset = TERMINAL_SCREEN_PADDING_Y.min(rect.height() * 0.5);
    Rect::from_min_max(
        pos2(rect.left() + x_inset, rect.top() + y_inset),
        pos2(rect.right() - x_inset, rect.bottom() - y_inset),
    )
}

pub(super) fn terminal_normalized_rect(rect: Rect) -> Option<Rect> {
    if !rect.min.x.is_finite()
        || !rect.min.y.is_finite()
        || !rect.max.x.is_finite()
        || !rect.max.y.is_finite()
    {
        return None;
    }

    Some(Rect::from_min_max(
        pos2(rect.left().min(rect.right()), rect.top().min(rect.bottom())),
        pos2(rect.left().max(rect.right()), rect.top().max(rect.bottom())),
    ))
}

pub(super) fn terminal_rect_contains_pointer(rect: Rect, pointer: Pos2) -> bool {
    let Some(rect) = terminal_normalized_rect(rect) else {
        return false;
    };
    pointer.x.is_finite()
        && pointer.y.is_finite()
        && pointer.x >= rect.left()
        && pointer.x < rect.right()
        && pointer.y >= rect.top()
        && pointer.y < rect.bottom()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TerminalRenderGrid {
    pub(super) inner: Rect,
    pub(super) cell_width: f32,
    pub(super) cell_height: f32,
    pub(super) visible_rows: u16,
    pub(super) visible_cols: u16,
}

impl TerminalRenderGrid {
    pub(super) fn cell_rect(self, row: u16, col: u16, width_cols: u16) -> Option<Rect> {
        if row >= self.visible_rows || col >= self.visible_cols || width_cols == 0 {
            return None;
        }

        let x = self.inner.left() + f32::from(col) * self.cell_width;
        let y = self.inner.top() + f32::from(row) * self.cell_height;
        let width = (f32::from(width_cols) * self.cell_width).min(self.inner.right() - x);
        let height = self.cell_height.min(self.inner.bottom() - y);
        if width <= 0.0 || height <= 0.0 {
            return None;
        }

        Some(Rect::from_min_size(pos2(x, y), vec2(width, height)))
    }
}

pub(super) fn terminal_render_grid(
    inner: Rect,
    rows: u16,
    cols: u16,
    cell_width: f32,
    cell_height: f32,
) -> Option<TerminalRenderGrid> {
    let inner = terminal_normalized_rect(inner)?;
    if rows == 0
        || cols == 0
        || inner.width() <= 0.0
        || inner.height() <= 0.0
        || !cell_width.is_finite()
        || !cell_height.is_finite()
        || cell_width <= 0.0
        || cell_height <= 0.0
    {
        return None;
    }

    let visible_rows = terminal_visible_cell_count(inner.height(), cell_height, rows);
    let visible_cols = terminal_visible_cell_count(inner.width(), cell_width, cols);
    if visible_rows == 0 || visible_cols == 0 {
        return None;
    }

    Some(TerminalRenderGrid {
        inner,
        cell_width,
        cell_height,
        visible_rows,
        visible_cols,
    })
}

pub(super) fn terminal_visible_cell_count(extent: f32, cell_size: f32, limit: u16) -> u16 {
    let count = (extent / cell_size).ceil();
    if !count.is_finite() || count <= 0.0 {
        0
    } else if count >= f32::from(limit) {
        limit
    } else {
        count as u16
    }
}

#[cfg(test)]
pub(super) fn terminal_cell_rect(
    inner: Rect,
    row: u16,
    col: u16,
    width_cols: u16,
    cell_width: f32,
    cell_height: f32,
) -> Option<Rect> {
    let inner = terminal_normalized_rect(inner)?;
    if inner.width() <= 0.0
        || inner.height() <= 0.0
        || width_cols == 0
        || !cell_width.is_finite()
        || !cell_height.is_finite()
        || cell_width <= 0.0
        || cell_height <= 0.0
    {
        return None;
    }

    let x = inner.left() + f32::from(col) * cell_width;
    let y = inner.top() + f32::from(row) * cell_height;
    if !x.is_finite() || !y.is_finite() || x >= inner.right() || y >= inner.bottom() {
        return None;
    }

    let width = (f32::from(width_cols) * cell_width).min(inner.right() - x);
    let height = cell_height.min(inner.bottom() - y);
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return None;
    }

    Some(Rect::from_min_size(pos2(x, y), vec2(width, height)))
}

pub(super) fn terminal_cell_position_at_pointer(
    pointer: Option<Pos2>,
    inner: Rect,
    cell_width: f32,
    cell_height: f32,
    rows: u16,
    cols: u16,
) -> Option<super::super::TerminalCellPosition> {
    let pointer = pointer?;
    let inner = terminal_normalized_rect(inner)?;
    if rows == 0
        || cols == 0
        || inner.width() <= 0.0
        || inner.height() <= 0.0
        || !pointer.x.is_finite()
        || !pointer.y.is_finite()
        || !cell_width.is_finite()
        || !cell_height.is_finite()
        || cell_width <= 0.0
        || cell_height <= 0.0
    {
        return None;
    }

    let x = terminal_clamped_axis_position(pointer.x, inner.left(), inner.right())?;
    let y = terminal_clamped_axis_position(pointer.y, inner.top(), inner.bottom())?;
    Some(super::super::TerminalCellPosition {
        row: terminal_cell_axis_index(y, inner.top(), cell_height, rows),
        col: terminal_cell_axis_index(x, inner.left(), cell_width, cols),
    })
}

pub(super) fn terminal_clamped_axis_position(value: f32, start: f32, end: f32) -> Option<f32> {
    if !value.is_finite() || !start.is_finite() || !end.is_finite() || end <= start {
        return None;
    }

    Some(value.clamp(start, (end - f32::EPSILON).max(start)))
}

pub(super) fn terminal_cell_axis_index(
    position: f32,
    start: f32,
    cell_size: f32,
    limit: u16,
) -> u16 {
    let index = ((position - start) / cell_size).floor();
    if !index.is_finite() || index <= 0.0 {
        0
    } else {
        (index as u16).min(limit.saturating_sub(1))
    }
}

pub(super) fn terminal_split_separator_width(available_width: f32, split_count: usize) -> f32 {
    let separator_count = split_count.saturating_sub(1);
    if separator_count == 0 {
        return 0.0;
    }

    let available_width = bounded_terminal_layout_value(available_width);
    TERMINAL_SPLIT_SEPARATOR_WIDTH.min(available_width / separator_count as f32)
}

pub(super) fn terminal_split_separator_line_rect(rect: Rect) -> Option<Rect> {
    let rect = terminal_normalized_rect(rect)?;
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return None;
    }

    let line_width = TERMINAL_SPLIT_SEPARATOR_LINE_WIDTH.min(rect.width());
    Some(Rect::from_center_size(
        rect.center(),
        vec2(line_width, rect.height()),
    ))
}
