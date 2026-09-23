use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;
use std::ops::Range;

use super::super::VIM_MAX_COUNT;
use super::super::motion::vim_line_first_non_whitespace_char;
use super::super::state::vim_set_previous_context_mark;

pub(in crate::editor_vim_key_events) fn vim_go_to_line(
    buffer: &mut TextBuffer,
    line_one_based: usize,
) {
    vim_set_previous_context_mark(buffer);

    let line = line_one_based.saturating_sub(1);
    let cursor = vim_line_first_non_whitespace_char(buffer, line);
    buffer.set_single_cursor(cursor);
}

pub(in crate::editor_vim_key_events) fn vim_line_range_for_count(
    buffer: &TextBuffer,
    count: usize,
) -> Option<Range<usize>> {
    if buffer.len_lines() == 0 || buffer.len_chars() == 0 {
        return None;
    }

    let count = count.clamp(1, VIM_MAX_COUNT);
    let start_line = buffer.cursor_position().line;
    let end_line = start_line
        .saturating_add(count.saturating_sub(1))
        .min(buffer.len_lines().saturating_sub(1));
    let start = buffer.line_column_to_char(start_line, 0);
    let end = if end_line + 1 < buffer.len_lines() {
        buffer.line_column_to_char(end_line + 1, 0)
    } else {
        buffer.len_chars()
    };
    (start < end).then_some(start..end)
}

pub(in crate::editor_vim_key_events) fn vim_line_column_motion_key(
    key: Key,
    modifiers: Modifiers,
) -> bool {
    if modifiers.command || modifiers.alt || modifiers.ctrl {
        return false;
    }
    matches!(
        (key, modifiers.shift),
        (Key::Backslash, true) | (Key::Pipe, _)
    )
}
