use kuroya_core::TextBuffer;
use std::ops::Range;

use super::super::state::vim_set_previous_context_mark;
use super::vim_char_at;

pub(in crate::editor_vim_key_events) fn vim_move_to_matching_bracket(
    buffer: &mut TextBuffer,
) -> bool {
    let Some((_, target)) = vim_matching_bracket_pair(buffer) else {
        return false;
    };

    vim_set_previous_context_mark(buffer);
    buffer.set_single_cursor(target);
    true
}

pub(in crate::editor_vim_key_events) fn vim_matching_bracket_range(
    buffer: &mut TextBuffer,
) -> Option<Range<usize>> {
    let (anchor, target) = vim_matching_bracket_pair(buffer)?;
    let start = anchor.min(target);
    let end = anchor.max(target).saturating_add(1).min(buffer.len_chars());
    (start < end).then_some(start..end)
}

fn vim_matching_bracket_pair(buffer: &mut TextBuffer) -> Option<(usize, usize)> {
    let cursor = buffer.cursor();
    if vim_char_at(buffer, cursor).is_some_and(vim_is_bracket_char) {
        let probe = cursor.saturating_add(1).min(buffer.len_chars());
        buffer.set_single_cursor(probe);
        let pair = buffer.matching_bracket();
        buffer.set_single_cursor(cursor);
        pair
    } else {
        buffer.matching_bracket()
    }
}

fn vim_is_bracket_char(ch: char) -> bool {
    matches!(ch, '(' | ')' | '[' | ']' | '{' | '}')
}
