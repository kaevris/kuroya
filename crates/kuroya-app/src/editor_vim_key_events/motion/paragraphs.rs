use kuroya_core::TextBuffer;

use super::super::VIM_MAX_COUNT;
use super::super::state::vim_set_previous_context_mark;

pub(in crate::editor_vim_key_events) fn vim_move_next_paragraph(
    buffer: &mut TextBuffer,
    count: usize,
) {
    vim_set_previous_context_mark(buffer);
    for _ in 0..count.clamp(1, VIM_MAX_COUNT) {
        let target = vim_next_paragraph_line(buffer);
        buffer.set_single_cursor(buffer.line_column_to_char(target, 0));
    }
}

pub(in crate::editor_vim_key_events) fn vim_move_previous_paragraph(
    buffer: &mut TextBuffer,
    count: usize,
) {
    vim_set_previous_context_mark(buffer);
    for _ in 0..count.clamp(1, VIM_MAX_COUNT) {
        let target = vim_previous_paragraph_line(buffer);
        buffer.set_single_cursor(buffer.line_column_to_char(target, 0));
    }
}

pub(in crate::editor_vim_key_events) fn vim_next_paragraph_line(buffer: &TextBuffer) -> usize {
    let line_count = buffer.len_lines();
    if line_count == 0 {
        return 0;
    }

    let current = buffer
        .cursor_position()
        .line
        .min(line_count.saturating_sub(1));
    let mut line = current.saturating_add(1);
    if buffer.line_is_blank(current) {
        while line < line_count && buffer.line_is_blank(line) {
            line += 1;
        }
        return line.min(line_count.saturating_sub(1));
    }

    while line < line_count && !buffer.line_is_blank(line) {
        line += 1;
    }
    while line < line_count && buffer.line_is_blank(line) {
        line += 1;
    }
    line.min(line_count.saturating_sub(1))
}

pub(in crate::editor_vim_key_events) fn vim_previous_paragraph_line(buffer: &TextBuffer) -> usize {
    let line_count = buffer.len_lines();
    if line_count == 0 {
        return 0;
    }

    let current = buffer
        .cursor_position()
        .line
        .min(line_count.saturating_sub(1));
    if current == 0 {
        return 0;
    }

    let mut line = current.saturating_sub(1);
    while line > 0 && buffer.line_is_blank(line) {
        line -= 1;
    }
    while line > 0 && !buffer.line_is_blank(line - 1) {
        line -= 1;
    }
    line
}
