use kuroya_core::TextBuffer;
use std::ops::Range;

use super::super::super::{VIM_MAX_COUNT, vim_char_at};

pub(super) fn vim_inner_word_range(buffer: &mut TextBuffer, count: usize) -> Option<Range<usize>> {
    let original_cursor = buffer.cursor();
    let first = vim_word_object_range_at(buffer, original_cursor)?;
    let mut end = first.end;
    for _ in 1..count.clamp(1, VIM_MAX_COUNT) {
        buffer.set_single_cursor(end);
        buffer.move_word_right();
        end = buffer.cursor().max(end);
    }
    buffer.set_single_cursor(original_cursor);
    (first.start < end).then_some(first.start..end)
}

fn vim_word_object_range_at(buffer: &TextBuffer, cursor: usize) -> Option<Range<usize>> {
    let len = buffer.len_chars();
    let cursor = cursor.min(len);
    if vim_char_at(buffer, cursor).is_some_and(vim_is_text_object_blank) {
        return vim_blank_run_range_at(buffer, cursor);
    }
    if let Some(class) = vim_char_at(buffer, cursor).map(vim_word_class) {
        return Some(vim_class_run_at(buffer, cursor, class));
    }

    if cursor > 0
        && let Some(class) = vim_char_at(buffer, cursor - 1)
            .filter(|ch| !vim_is_newline(*ch))
            .map(vim_word_class)
    {
        let mut start = cursor - 1;
        while start > 0
            && vim_char_at(buffer, start - 1)
                .filter(|ch| !vim_is_newline(*ch))
                .is_some_and(|ch| vim_word_class(ch) == class)
        {
            start -= 1;
        }
        return Some(start..cursor);
    }
    None
}

fn vim_blank_run_range_at(buffer: &TextBuffer, cursor: usize) -> Option<Range<usize>> {
    let len = buffer.len_chars();
    let cursor = cursor.min(len);
    if !vim_char_at(buffer, cursor).is_some_and(vim_is_text_object_blank) {
        return None;
    }

    let mut start = cursor;
    while start > 0 && vim_char_at(buffer, start - 1).is_some_and(vim_is_text_object_blank) {
        start -= 1;
    }
    let mut end = cursor + 1;
    while end < len && vim_char_at(buffer, end).is_some_and(vim_is_text_object_blank) {
        end += 1;
    }
    Some(start..end)
}

pub(super) fn vim_inner_big_word_range(buffer: &TextBuffer, count: usize) -> Option<Range<usize>> {
    let cursor = buffer.cursor();

    let first = if vim_char_at(buffer, cursor.min(buffer.len_chars()))
        .is_some_and(vim_is_text_object_blank)
    {
        vim_blank_run_range_at(buffer, cursor)?
    } else {
        vim_big_word_range_at(buffer, cursor)?
    };
    let mut end = first.end;
    for _ in 1..count.clamp(1, VIM_MAX_COUNT) {
        let next = vim_big_word_range_after(buffer, end)?;
        end = next.end.max(end);
    }
    (first.start < end).then_some(first.start..end)
}

pub(super) fn vim_outer_word_range(
    buffer: &TextBuffer,
    inner: Range<usize>,
    cursor: usize,
) -> Range<usize> {
    let len = buffer.len_chars();
    let cursor = cursor.min(len);
    if vim_char_at(buffer, cursor).is_some_and(vim_is_text_object_blank) {
        let word_follows = vim_char_at(buffer, inner.end.min(len))
            .filter(|ch| !vim_is_newline(*ch))
            .is_some_and(|ch| !ch.is_whitespace());
        if word_follows {
            let mut end = inner.end.min(len) + 1;
            while end < len
                && vim_char_at(buffer, end)
                    .filter(|ch| !vim_is_newline(*ch))
                    .is_some_and(|ch| !ch.is_whitespace())
            {
                end += 1;
            }
            return inner.start..end;
        }

        let mut start = inner.start;
        while start > 0
            && vim_char_at(buffer, start - 1)
                .filter(|ch| !vim_is_newline(*ch))
                .is_some_and(|ch| !ch.is_whitespace())
        {
            start -= 1;
        }
        return start..inner.end;
    }

    let mut end = inner.end.min(len);
    while end < len && vim_char_at(buffer, end).is_some_and(vim_is_text_object_blank) {
        end += 1;
    }
    if end > inner.end {
        return inner.start..end;
    }

    let mut start = inner.start.min(len);
    while start > 0 && vim_char_at(buffer, start - 1).is_some_and(vim_is_text_object_blank) {
        start -= 1;
    }
    start..inner.end
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VimWordClass {
    Word,
    Punct,
}

fn vim_word_class(ch: char) -> VimWordClass {
    if ch.is_alphanumeric() || ch == '_' {
        VimWordClass::Word
    } else {
        VimWordClass::Punct
    }
}

fn vim_class_run_at(buffer: &TextBuffer, cursor: usize, class: VimWordClass) -> Range<usize> {
    let len = buffer.len_chars();
    let mut start = cursor;
    while start > 0
        && vim_char_at(buffer, start - 1)
            .filter(|ch| !vim_is_newline(*ch))
            .is_some_and(|ch| vim_word_class(ch) == class)
    {
        start -= 1;
    }
    let mut end = cursor + 1;
    while end < len
        && vim_char_at(buffer, end)
            .filter(|ch| !vim_is_newline(*ch))
            .is_some_and(|ch| vim_word_class(ch) == class)
    {
        end += 1;
    }
    start..end
}

fn vim_big_word_range_at(buffer: &TextBuffer, cursor: usize) -> Option<Range<usize>> {
    let len = buffer.len_chars();
    if len == 0 {
        return None;
    }

    let cursor = cursor.min(len);
    let word_idx = if cursor < len && !vim_char_at(buffer, cursor)?.is_whitespace() {
        cursor
    } else if cursor > 0 && !vim_char_at(buffer, cursor - 1)?.is_whitespace() {
        cursor - 1
    } else {
        return None;
    };

    let mut start = word_idx;
    while start > 0 && !vim_char_at(buffer, start - 1)?.is_whitespace() {
        start -= 1;
    }

    let mut end = word_idx + 1;
    while end < len && !vim_char_at(buffer, end)?.is_whitespace() {
        end += 1;
    }

    Some(start..end)
}

fn vim_big_word_range_after(buffer: &TextBuffer, after: usize) -> Option<Range<usize>> {
    let len = buffer.len_chars();
    let mut start = after.min(len);
    while start < len && vim_char_at(buffer, start)?.is_whitespace() {
        start += 1;
    }
    if start >= len {
        return None;
    }

    let mut end = start + 1;
    while end < len && !vim_char_at(buffer, end)?.is_whitespace() {
        end += 1;
    }
    Some(start..end)
}

fn vim_is_text_object_blank(ch: char) -> bool {
    ch.is_whitespace() && !vim_is_newline(ch)
}

fn vim_is_newline(ch: char) -> bool {
    ch == '\n' || ch == '\r'
}
