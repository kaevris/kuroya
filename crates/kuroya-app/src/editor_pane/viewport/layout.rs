use super::{
    EDITOR_MAX_FONT_SIZE, EDITOR_MIN_FONT_SIZE, EDITOR_MINIMAP_MIN_VIEWPORT_WIDTH,
    EDITOR_MINIMAP_WIDTH,
};
use crate::ui_scrollbars;
#[cfg(test)]
use eframe::egui::Color32;
use eframe::egui::{self, Rect, pos2};
#[cfg(test)]
use egui::scroll_area::ScrollBarVisibility;
use egui::scroll_area::ScrollSource;
use kuroya_core::{
    EditorMinimapAutohide, EditorMinimapSide, EditorMinimapSize, EditorScrollbarVisibility,
    MAX_EDITOR_LINE_HEIGHT, TextBuffer, buffer::CursorPosition,
};
use std::{collections::HashSet, ops::Range};

pub(super) fn minimap_decoration_line_sets(
    buffer: &TextBuffer,
    find_matches: &[Range<usize>],
    cursor_positions: &[CursorPosition],
) -> (HashSet<usize>, HashSet<usize>) {
    let line_count = buffer.len_lines().max(1);
    let buffer_len_chars = buffer.len_chars();
    let mut find_match_lines = HashSet::with_capacity(find_matches.len().min(line_count));
    for range in find_matches {
        if range.start >= range.end || range.start >= buffer_len_chars {
            continue;
        }
        if let Some(line_number) =
            one_based_buffer_line(buffer.char_position(range.start).line, line_count)
        {
            find_match_lines.insert(line_number);
        }
    }

    let mut cursor_lines = HashSet::with_capacity(cursor_positions.len().min(line_count));
    for cursor in cursor_positions {
        if let Some(line_number) = one_based_buffer_line(cursor.line, line_count) {
            cursor_lines.insert(line_number);
        }
    }

    (find_match_lines, cursor_lines)
}

pub(super) fn editor_minimap_visible(
    minimap_enabled: bool,
    autohide: EditorMinimapAutohide,
    editor_hovered_or_dragged: bool,
) -> bool {
    minimap_enabled
        && match autohide {
            EditorMinimapAutohide::None => true,
            EditorMinimapAutohide::Mouseover | EditorMinimapAutohide::Scroll => {
                editor_hovered_or_dragged
            }
        }
}

pub(super) fn editor_minimap_width(
    viewport_width: f32,
    minimap_enabled: bool,
    minimap_size: EditorMinimapSize,
    minimap_max_column: usize,
    minimap_scale: usize,
) -> f32 {
    if !minimap_enabled
        || !viewport_width.is_finite()
        || viewport_width < EDITOR_MINIMAP_MIN_VIEWPORT_WIDTH
    {
        0.0
    } else {
        let max_width = saturated_f32_from_f64((viewport_width as f64 * 0.28).floor());
        match minimap_size {
            EditorMinimapSize::Proportional => EDITOR_MINIMAP_WIDTH.min(max_width),
            EditorMinimapSize::Fill => max_width,
            EditorMinimapSize::Fit => {
                let scale = minimap_scale.clamp(1, 3) as f64;
                let fit_width =
                    saturated_f32_from_f64(12.0 + minimap_max_column.max(1) as f64 * scale * 0.35);
                fit_width.clamp(24.0, max_width)
            }
        }
    }
}

pub(super) fn editor_viewport_rects(
    viewport_rect: Rect,
    minimap_width: f32,
    minimap_side: EditorMinimapSide,
) -> (Option<Rect>, Rect, Option<Rect>) {
    if !editor_rect_finite(viewport_rect) {
        return (None, viewport_rect, None);
    }

    let viewport_width = viewport_rect.width();
    if !minimap_width.is_finite()
        || minimap_width <= 0.0
        || !viewport_width.is_finite()
        || viewport_width <= 0.0
    {
        return (None, viewport_rect, None);
    }
    let minimap_width = minimap_width.min(viewport_width);

    match minimap_side {
        EditorMinimapSide::Left => {
            let minimap_rect = Rect::from_min_max(
                viewport_rect.min,
                pos2(viewport_rect.left() + minimap_width, viewport_rect.bottom()),
            );
            let scroll_rect = Rect::from_min_max(
                pos2(minimap_rect.right(), viewport_rect.top()),
                viewport_rect.max,
            );
            (Some(minimap_rect), scroll_rect, None)
        }
        EditorMinimapSide::Right => {
            let minimap_rect = Rect::from_min_max(
                pos2(viewport_rect.right() - minimap_width, viewport_rect.top()),
                viewport_rect.max,
            );
            let scroll_rect = Rect::from_min_max(
                viewport_rect.min,
                pos2(minimap_rect.left(), viewport_rect.bottom()),
            );
            (None, scroll_rect, Some(minimap_rect))
        }
    }
}

pub(super) fn editor_content_rect_with_padding(
    scroll_rect: Rect,
    padding_top: usize,
    padding_bottom: usize,
    row_height: f32,
) -> Rect {
    if !editor_rect_finite(scroll_rect) {
        return scroll_rect;
    }

    let height = scroll_rect.height().max(0.0);
    let row_height = if row_height.is_finite() && row_height > 0.0 {
        row_height
    } else {
        1.0
    };
    let min_content_height = row_height.max(1.0).min(height);
    let max_total_padding = (height - min_content_height).max(0.0);
    let mut top = padding_top as f64;
    let mut bottom = padding_bottom as f64;
    let total = top + bottom;
    if total > max_total_padding as f64 && total > 0.0 {
        let scale = max_total_padding as f64 / total;
        top *= scale;
        bottom *= scale;
    }
    let top = saturated_f32_from_f64(top);
    let bottom = saturated_f32_from_f64(bottom);

    Rect::from_min_max(
        pos2(scroll_rect.left(), scroll_rect.top() + top),
        pos2(scroll_rect.right(), scroll_rect.bottom() - bottom),
    )
}

pub(super) fn editor_row_width(
    viewport_width: f32,
    gutter_width: f32,
    char_width: f32,
    scroll_beyond_last_column: usize,
    reveal_horizontal_right_padding: usize,
) -> f32 {
    let viewport_width = finite_non_negative_or(viewport_width, 0.0);
    let gutter_width = finite_non_negative_or(gutter_width, 0.0);
    let char_width = if char_width.is_finite() && char_width > 0.0 {
        char_width
    } else {
        0.0
    };
    let base_width = (viewport_width as f64).max(gutter_width as f64 + char_width as f64);
    let scroll_width = scroll_beyond_last_column as f64 * char_width as f64;
    let reveal_padding = reveal_horizontal_right_padding as f64;
    saturated_f32_from_f64(base_width + scroll_width + reveal_padding)
}

pub(super) fn editor_vertical_scrollbar_needed(
    line_total: usize,
    row_height: f32,
    viewport_height: f32,
) -> bool {
    if !row_height.is_finite()
        || row_height <= 0.0
        || !viewport_height.is_finite()
        || viewport_height <= 0.0
    {
        return false;
    }

    (line_total as f64 * row_height as f64) > viewport_height as f64 + 0.5
}

pub(super) fn editor_horizontal_scrollbar_needed(content_width: f32, viewport_width: f32) -> bool {
    content_width.is_finite()
        && viewport_width.is_finite()
        && content_width > viewport_width.max(0.0) + 0.5
}

pub(super) fn editor_visible_rows_for_render(
    rows: Range<usize>,
    line_total: usize,
) -> Option<Range<usize>> {
    let line_total = line_total.max(1);
    let start = rows.start.min(line_total);
    let end = rows.end.min(line_total);
    (start < end).then_some(start..end)
}

#[cfg(test)]
pub(super) fn editor_scrollbar_visibility(
    setting: EditorScrollbarVisibility,
) -> ScrollBarVisibility {
    ui_scrollbars::scrollbar_visibility(setting)
}

#[cfg(test)]
pub(super) fn editor_scrollbar_axis_enabled(setting: EditorScrollbarVisibility) -> bool {
    ui_scrollbars::scrollbar_axis_enabled(setting)
}

pub(super) fn editor_scrollbar_axes(
    vertical: EditorScrollbarVisibility,
    horizontal: EditorScrollbarVisibility,
    vertical_needed: bool,
    horizontal_needed: bool,
) -> ui_scrollbars::ScrollbarAxes {
    ui_scrollbars::scrollbar_axes(vertical, horizontal, vertical_needed, horizontal_needed)
}

#[cfg(test)]
pub(super) fn editor_scrollbar_visibility_for_axes(
    vertical: EditorScrollbarVisibility,
    horizontal: EditorScrollbarVisibility,
    vertical_needed: bool,
    horizontal_needed: bool,
) -> ScrollBarVisibility {
    ui_scrollbars::scrollbar_visibility_for_axes(
        vertical,
        horizontal,
        vertical_needed,
        horizontal_needed,
    )
}

pub(super) fn editor_scrollbar_rect_for_axes(
    rect: Rect,
    axes: ui_scrollbars::ScrollbarAxes,
) -> Rect {
    ui_scrollbars::scrollbar_rect_for_axes(rect, axes)
}

#[cfg(test)]
pub(super) fn editor_scrollbar_width(vertical_size: usize, horizontal_size: usize) -> f32 {
    ui_scrollbars::themed_scrollbar_width(vertical_size, horizontal_size)
}

pub(super) fn editor_scrollbar_style(
    vertical_size: usize,
    horizontal_size: usize,
    floating: bool,
) -> egui::style::ScrollStyle {
    ui_scrollbars::themed_scrollbar_style(vertical_size, horizontal_size, floating)
}

pub(super) fn apply_editor_scrollbar_visuals(ui: &mut egui::Ui) {
    ui_scrollbars::apply_themed_scrollbar_visuals(ui);
}

#[cfg(test)]
pub(super) fn editor_scrollbar_handle_colors(
    visuals: &egui::Visuals,
) -> (Color32, Color32, Color32) {
    ui_scrollbars::themed_scrollbar_handle_colors(visuals)
}

pub(super) fn editor_wheel_scroll_multiplier(
    mouse_wheel_scroll_sensitivity: f32,
    fast_scroll_sensitivity: f32,
    fast_scroll_modifier_active: bool,
    zoom_modifier_active: bool,
    scroll_predominant_axis: bool,
    scroll_delta: egui::Vec2,
) -> egui::Vec2 {
    if zoom_modifier_active {
        return egui::Vec2::ZERO;
    }

    let base = finite_non_negative_or(mouse_wheel_scroll_sensitivity, 1.0);
    let fast = if fast_scroll_modifier_active {
        finite_non_negative_or(fast_scroll_sensitivity, 1.0)
    } else {
        1.0
    };
    let multiplier = egui::Vec2::splat(finite_non_negative_product(base, fast));
    predominant_axis_scroll_multiplier(multiplier, scroll_predominant_axis, scroll_delta)
}

pub(super) fn editor_scroll_source(inertial_scroll: bool) -> ScrollSource {
    if inertial_scroll {
        ScrollSource {
            mouse_wheel: false,
            ..ScrollSource::ALL
        }
    } else {
        ScrollSource::ALL
    }
}

fn predominant_axis_scroll_multiplier(
    multiplier: egui::Vec2,
    enabled: bool,
    scroll_delta: egui::Vec2,
) -> egui::Vec2 {
    if !enabled {
        return multiplier;
    }

    let x = scroll_delta.x.abs();
    let y = scroll_delta.y.abs();
    if x > y && y > 0.0 {
        egui::Vec2::new(multiplier.x, 0.0)
    } else if y > x && x > 0.0 {
        egui::Vec2::new(0.0, multiplier.y)
    } else {
        multiplier
    }
}

pub(super) fn finite_non_negative_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        fallback
    }
}

fn finite_non_negative_product(left: f32, right: f32) -> f32 {
    saturated_f32_from_f64(left.max(0.0) as f64 * right.max(0.0) as f64).max(0.0)
}

pub(super) fn editor_viewport_row_height(row_height: f32) -> f32 {
    finite_non_negative_or(row_height, 1.0).clamp(1.0, MAX_EDITOR_LINE_HEIGHT.max(1.0))
}

pub(super) fn saturated_f32_from_f64(value: f64) -> f32 {
    if !value.is_finite() {
        if value.is_sign_negative() {
            -f32::MAX
        } else {
            f32::MAX
        }
    } else if value > f32::MAX as f64 {
        f32::MAX
    } else if value < -(f32::MAX as f64) {
        -f32::MAX
    } else {
        value as f32
    }
}

pub(super) fn editor_rect_finite(rect: Rect) -> bool {
    rect.left().is_finite()
        && rect.right().is_finite()
        && rect.top().is_finite()
        && rect.bottom().is_finite()
}

pub(super) fn editor_mouse_wheel_zoom_delta_y(ui: &egui::Ui) -> f32 {
    ui.input(|input| {
        input
            .events
            .iter()
            .filter_map(|event| match event {
                egui::Event::MouseWheel {
                    delta, modifiers, ..
                } if editor_mouse_wheel_zoom_modifier(*modifiers) => Some(delta.y),
                _ => None,
            })
            .sum()
    })
}

pub(super) fn editor_mouse_wheel_zoom_modifier(modifiers: egui::Modifiers) -> bool {
    modifiers.command || modifiers.ctrl
}

pub(super) fn editor_zoomed_font_size(font_size: f32, wheel_delta_y: f32) -> Option<f32> {
    if !wheel_delta_y.is_finite() || wheel_delta_y == 0.0 {
        return None;
    }

    let current = if font_size.is_finite() {
        font_size
    } else {
        13.0
    };
    let step = if wheel_delta_y > 0.0 { 1.0 } else { -1.0 };
    let next = (current + step).clamp(EDITOR_MIN_FONT_SIZE, EDITOR_MAX_FONT_SIZE);
    (next.to_bits() != current.to_bits()).then_some(next)
}

pub(crate) fn editor_scroll_row_count(
    visible_line_count: usize,
    viewport_height: f32,
    row_height: f32,
    scroll_beyond_last_line: bool,
) -> usize {
    let visible_line_count = visible_line_count.max(1);
    if !scroll_beyond_last_line
        || !viewport_height.is_finite()
        || viewport_height <= 0.0
        || !row_height.is_finite()
        || row_height <= 0.0
    {
        return visible_line_count;
    }

    let extra_rows = (viewport_height as f64 / row_height as f64)
        .floor()
        .max(1.0);
    let extra_rows = if extra_rows >= usize::MAX as f64 {
        usize::MAX
    } else {
        extra_rows as usize
    };
    visible_line_count.saturating_add(extra_rows.saturating_sub(1))
}

pub(super) fn one_based_buffer_line(line_idx: usize, line_count: usize) -> Option<usize> {
    (line_idx < line_count)
        .then(|| line_idx.checked_add(1))
        .flatten()
}
