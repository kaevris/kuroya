use kuroya_core::TextBuffer;
use std::ops::Range;

use super::super::VIM_MAX_COUNT;
use super::vim_char_at;

pub(in crate::editor_vim_key_events) fn vim_move_previous_big_word_end(buffer: &mut TextBuffer) {
    let target = vim_previous_big_word_end_char(buffer, buffer.cursor());
    buffer.set_single_cursor(target);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VimWordClass {
    Word,
    Punct,
}

fn vim_word_class(ch: char) -> Option<VimWordClass> {
    if ch.is_whitespace() {
        None
    } else if ch.is_alphanumeric() || ch == '_' {
        Some(VimWordClass::Word)
    } else {
        Some(VimWordClass::Punct)
    }
}

fn vim_word_class_at(buffer: &TextBuffer, idx: usize) -> Option<VimWordClass> {
    vim_char_at(buffer, idx).and_then(vim_word_class)
}

pub(in crate::editor_vim_key_events) fn vim_next_word_start(
    buffer: &TextBuffer,
    cursor: usize,
) -> usize {
    let len = buffer.len_chars();
    let mut idx = cursor.min(len);
    if idx >= len {
        return len;
    }

    if let Some(class) = vim_word_class_at(buffer, idx) {
        while idx < len && vim_word_class_at(buffer, idx) == Some(class) {
            idx += 1;
        }
    }
    while idx < len && vim_word_class_at(buffer, idx).is_none() {
        if vim_char_at(buffer, idx) == Some('\n') && vim_line_is_empty_after(buffer, idx) {
            return idx + 1;
        }
        idx += 1;
    }
    idx
}

fn vim_line_is_empty_after(buffer: &TextBuffer, newline_idx: usize) -> bool {
    let next_line = buffer.char_position(newline_idx).line + 1;
    if next_line >= buffer.len_lines() {
        return false;
    }
    let line_start = buffer.line_column_to_char(next_line, 0);
    buffer.line_content_end_char(next_line) == line_start
}

fn vim_word_run_end(buffer: &TextBuffer, cursor: usize) -> usize {
    let len = buffer.len_chars();
    let Some(class) = vim_word_class_at(buffer, cursor.min(len)) else {
        return cursor.min(len);
    };
    let mut idx = cursor.min(len);
    while idx < len && vim_word_class_at(buffer, idx) == Some(class) {
        idx += 1;
    }
    idx
}

fn vim_exclusive_word_end(buffer: &TextBuffer, end: usize) -> usize {
    let len = buffer.len_chars();
    let end = end.min(len);
    if end == 0 {
        return end;
    }

    let position = buffer.char_position(end);
    if position.column != 0 {
        return end;
    }

    buffer.line_content_end_char(position.line.saturating_sub(1))
}

pub(in crate::editor_vim_key_events) fn vim_word_motion_range(
    buffer: &TextBuffer,
    count: usize,
) -> Option<Range<usize>> {
    let start = buffer.cursor();
    let mut end = start;
    for _ in 0..count.clamp(1, VIM_MAX_COUNT) {
        let next = vim_next_word_start(buffer, end);
        if next <= end {
            end = buffer.len_chars();
            break;
        }
        end = next;
    }
    let end = vim_exclusive_word_end(buffer, end);
    (end > start).then_some(start..end)
}

pub(in crate::editor_vim_key_events) fn vim_change_word_motion_range(
    buffer: &TextBuffer,
    count: usize,
) -> Option<Range<usize>> {
    let start = buffer.cursor();
    if vim_word_class_at(buffer, start.min(buffer.len_chars())).is_none() {
        return vim_word_motion_range(buffer, count);
    }

    let mut idx = start;
    for _ in 1..count.clamp(1, VIM_MAX_COUNT) {
        let next = vim_next_word_start(buffer, idx);
        if next <= idx {
            break;
        }
        idx = next;
    }
    let end = vim_word_run_end(buffer, idx);
    (end > start).then_some(start..end)
}

fn vim_previous_big_word_end_char(buffer: &TextBuffer, cursor: usize) -> usize {
    let len = buffer.len_chars();
    let idx = cursor.min(len);
    if idx == 0 {
        return 0;
    }

    let mut probe = idx - 1;
    if idx < len && vim_char_at(buffer, idx).is_some_and(|ch| !ch.is_whitespace()) {
        while probe > 0 && vim_char_at(buffer, probe).is_some_and(|ch| !ch.is_whitespace()) {
            probe -= 1;
        }
        if vim_char_at(buffer, probe).is_some_and(|ch| !ch.is_whitespace()) {
            return 0;
        }
    }

    while probe > 0 && vim_char_at(buffer, probe).is_some_and(char::is_whitespace) {
        probe -= 1;
    }
    if vim_char_at(buffer, probe).is_some_and(char::is_whitespace) {
        0
    } else {
        probe
    }
}
