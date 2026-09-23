use kuroya_core::TextBuffer;
use std::ops::Range;

use super::super::super::{EditorVimTextObjectScope, VIM_MAX_COUNT, vim_char_at};
use super::vim_line_start_char;

pub(super) fn vim_block_text_object_range(
    buffer: &TextBuffer,
    count: usize,
    scope: EditorVimTextObjectScope,
    open: char,
    close: char,
) -> Option<Range<usize>> {
    let (open_idx, close_idx) = vim_block_text_object_pair(buffer, count, open, close)?;
    match scope {
        EditorVimTextObjectScope::Inner => {
            let start = open_idx.saturating_add(1);
            (start < close_idx).then_some(start..close_idx)
        }

        EditorVimTextObjectScope::Outer => Some(vim_object_with_surrounding_blanks(
            buffer,
            open_idx,
            close_idx.saturating_add(1).min(buffer.len_chars()),
        )),
    }
}

fn vim_object_with_surrounding_blanks(
    buffer: &TextBuffer,
    open_idx: usize,
    close_end: usize,
) -> Range<usize> {
    let len = buffer.len_chars();
    let mut end = close_end;
    while end < len
        && vim_char_at(buffer, end).is_some_and(|ch| ch != '\n' && ch != '\r' && ch.is_whitespace())
    {
        end += 1;
    }
    if end > close_end {
        return open_idx..end;
    }
    let mut start = open_idx;
    while start > 0
        && vim_char_at(buffer, start - 1)
            .is_some_and(|ch| ch != '\n' && ch != '\r' && ch.is_whitespace())
    {
        start -= 1;
    }
    start..close_end
}

pub(super) fn vim_quote_text_object_range(
    buffer: &TextBuffer,
    count: usize,
    scope: EditorVimTextObjectScope,
    quote: char,
) -> Option<Range<usize>> {
    let (pair_count, scope) = if count == 2 {
        (1, EditorVimTextObjectScope::Outer)
    } else {
        (count, scope)
    };
    let (open_idx, close_idx) = vim_quote_text_object_pair(buffer, pair_count, quote)?;
    let close_end = close_idx.saturating_add(1).min(buffer.len_chars());
    match scope {
        EditorVimTextObjectScope::Inner => {
            let start = open_idx.saturating_add(1);
            (start < close_idx).then_some(start..close_idx)
        }

        EditorVimTextObjectScope::Outer if count == 2 => Some(open_idx..close_end),
        EditorVimTextObjectScope::Outer => Some(vim_object_with_surrounding_blanks(
            buffer, open_idx, close_end,
        )),
    }
}

fn vim_block_text_object_pair(
    buffer: &TextBuffer,
    count: usize,
    open: char,
    close: char,
) -> Option<(usize, usize)> {
    let len = buffer.len_chars();
    let cursor = buffer.cursor().min(len);
    let mut stack = Vec::new();
    let mut candidates = Vec::new();
    for idx in 0..len {
        let Some(ch) = vim_char_at(buffer, idx) else {
            continue;
        };
        if ch == open {
            stack.push(idx);
        } else if ch == close
            && let Some(open_idx) = stack.pop()
            && open_idx <= cursor
            && cursor <= idx
        {
            candidates.push((open_idx, idx));
        }
    }

    candidates.sort_by(|left, right| {
        left.1
            .saturating_sub(left.0)
            .cmp(&right.1.saturating_sub(right.0))
            .then(left.0.cmp(&right.0).reverse())
    });
    if let Some(found) = candidates
        .into_iter()
        .nth(count.clamp(1, VIM_MAX_COUNT) - 1)
    {
        return Some(found);
    }

    vim_next_block_pair_after(buffer, cursor, open, close)
}

fn vim_next_block_pair_after(
    buffer: &TextBuffer,
    cursor: usize,
    open: char,
    close: char,
) -> Option<(usize, usize)> {
    let len = buffer.len_chars();
    let mut idx = cursor;
    while idx < len {
        if vim_char_at(buffer, idx) == Some(open) {
            let mut depth = 0usize;
            let mut probe = idx;
            while probe < len {
                let ch = vim_char_at(buffer, probe);
                if ch == Some(open) {
                    depth += 1;
                } else if ch == Some(close) {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some((idx, probe));
                    }
                }
                probe += 1;
            }
        }
        idx += 1;
    }
    None
}

fn vim_quote_text_object_pair(
    buffer: &TextBuffer,
    count: usize,
    quote: char,
) -> Option<(usize, usize)> {
    let line = buffer.cursor_position().line;
    let pair = vim_quote_pair_on_line(buffer, line, count, quote);
    if pair.is_some() {
        return pair;
    }

    let mut probe_line = line + 1;
    while probe_line < buffer.len_lines() {
        if let Some(pair) = vim_quote_first_pair_on_line(buffer, probe_line, quote) {
            return Some(pair);
        }
        probe_line += 1;
    }
    None
}

fn vim_quote_first_pair_on_line(
    buffer: &TextBuffer,
    line: usize,
    quote: char,
) -> Option<(usize, usize)> {
    let line_start = vim_line_start_char(buffer, line);
    let line_end = buffer.line_content_end_char(line);
    let mut open_idx = None;
    for idx in line_start..line_end {
        if vim_char_at(buffer, idx) != Some(quote)
            || vim_quote_char_is_escaped(buffer, idx, line_start)
        {
            continue;
        }
        if let Some(open) = open_idx {
            return Some((open, idx));
        }
        open_idx = Some(idx);
    }
    None
}

fn vim_quote_pair_on_line(
    buffer: &TextBuffer,
    line: usize,
    count: usize,
    quote: char,
) -> Option<(usize, usize)> {
    let line_start = vim_line_start_char(buffer, line);
    let line_end = buffer.line_content_end_char(line);
    let cursor = buffer.cursor().min(line_end);
    let mut open_idx = None;
    let mut candidates = Vec::new();
    let mut quotes = Vec::new();

    for idx in line_start..line_end {
        if vim_char_at(buffer, idx) != Some(quote)
            || vim_quote_char_is_escaped(buffer, idx, line_start)
        {
            continue;
        }
        quotes.push(idx);

        if let Some(open) = open_idx.take() {
            if open <= cursor && cursor <= idx {
                candidates.push((open, idx));
            }
        } else {
            open_idx = Some(idx);
        }
    }

    if let Some(found) = candidates
        .into_iter()
        .nth(count.clamp(1, VIM_MAX_COUNT) - 1)
    {
        return Some(found);
    }

    let straddling = quotes
        .iter()
        .copied()
        .filter(|idx| *idx < cursor)
        .next_back()
        .zip(quotes.iter().copied().find(|idx| *idx > cursor));
    straddling.filter(|(open, close)| open < &cursor && &cursor < close)
}

fn vim_quote_char_is_escaped(buffer: &TextBuffer, idx: usize, line_start: usize) -> bool {
    let mut slash_count = 0;
    let mut probe = idx;
    while probe > line_start && vim_char_at(buffer, probe - 1) == Some('\\') {
        slash_count += 1;
        probe -= 1;
    }
    slash_count % 2 == 1
}
