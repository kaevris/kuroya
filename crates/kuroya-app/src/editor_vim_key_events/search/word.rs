use super::super::state::vim_set_previous_context_mark;
use super::matching::{vim_is_buffer_word_char, vim_search_word_target};
use super::repeat::vim_repeat_last_search_in_direction;
use super::state::vim_set_last_search;
use kuroya_core::TextBuffer;
use std::ops::Range;

pub(in crate::editor_vim_key_events) fn vim_operator_search_word_under_cursor_range(
    buffer: &mut TextBuffer,
    count: usize,
    forward: bool,
    whole_word: bool,
) -> Option<Range<usize>> {
    let start = buffer.cursor();
    let word_range = vim_search_word_range_at_or_after_cursor(buffer)?;
    let word = buffer.text_range(word_range.clone())?;
    if word.is_empty() {
        return None;
    }

    vim_set_last_search(buffer, &word, forward, whole_word);
    let target =
        vim_search_word_target(buffer, &word, word_range.start, count, forward, whole_word)?;
    (start != target).then_some(start.min(target)..start.max(target))
}

pub(in crate::editor_vim_key_events) fn vim_search_word_under_cursor(
    buffer: &mut TextBuffer,
    count: usize,
    forward: bool,
    whole_word: bool,
) -> bool {
    let Some(word_range) = vim_search_word_range_at_or_after_cursor(buffer) else {
        return false;
    };
    let Some(word) = buffer.text_range(word_range.clone()) else {
        return false;
    };
    if word.is_empty() {
        return false;
    }

    vim_set_last_search(buffer, &word, forward, whole_word);
    vim_search_word(buffer, &word, word_range.start, count, forward, whole_word)
}

pub(in crate::editor_vim_key_events) fn vim_literal_search(
    buffer: &mut TextBuffer,
    query: &str,
    count: usize,
    forward: bool,
) -> bool {
    if query.is_empty() {
        return vim_repeat_last_search_in_direction(buffer, count, forward);
    }
    vim_set_last_search(buffer, query, forward, false);
    vim_search_word(buffer, query, buffer.cursor(), count, forward, false)
}

fn vim_search_word_range_at_or_after_cursor(buffer: &TextBuffer) -> Option<Range<usize>> {
    if buffer
        .char_at(buffer.cursor())
        .is_some_and(|ch| vim_is_buffer_word_char(buffer, ch))
    {
        if let Some(range) = buffer.word_range_at_cursor() {
            return Some(range);
        }
    }

    if let Some(range) = vim_search_word_range_before_cursor_at_line_end(buffer) {
        return Some(range);
    }

    let len = buffer.len_chars();
    let mut idx = buffer.cursor().saturating_add(1);
    while idx < len {
        let ch = buffer.char_at(idx)?;
        if vim_is_buffer_word_char(buffer, ch) {
            return vim_word_range_at(buffer, idx);
        }
        idx += 1;
    }
    None
}

fn vim_search_word_range_before_cursor_at_line_end(buffer: &TextBuffer) -> Option<Range<usize>> {
    let cursor = buffer.cursor();
    if cursor == 0 {
        return None;
    }

    let line = buffer.cursor_position().line;
    if cursor != buffer.line_content_end_char(line) {
        return None;
    }

    let previous = cursor - 1;
    if !buffer
        .char_at(previous)
        .is_some_and(|ch| vim_is_buffer_word_char(buffer, ch))
    {
        return None;
    }

    vim_word_range_at(buffer, previous)
}

fn vim_word_range_at(buffer: &TextBuffer, cursor: usize) -> Option<Range<usize>> {
    let ch = buffer.char_at(cursor)?;
    if !vim_is_buffer_word_char(buffer, ch) {
        return None;
    }

    let mut start = cursor;
    while start > 0
        && buffer
            .char_at(start - 1)
            .is_some_and(|ch| vim_is_buffer_word_char(buffer, ch))
    {
        start -= 1;
    }

    let len = buffer.len_chars();
    let mut end = cursor + 1;
    while end < len
        && buffer
            .char_at(end)
            .is_some_and(|ch| vim_is_buffer_word_char(buffer, ch))
    {
        end += 1;
    }

    Some(start..end)
}

fn vim_search_word(
    buffer: &mut TextBuffer,
    word: &str,
    origin: usize,
    count: usize,
    forward: bool,
    whole_word: bool,
) -> bool {
    let Some(target) = vim_search_word_target(buffer, word, origin, count, forward, whole_word)
    else {
        return false;
    };

    vim_set_previous_context_mark(buffer);
    buffer.set_single_cursor(target);
    true
}
