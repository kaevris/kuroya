use kuroya_core::{TextBuffer, TextEdit};

use super::super::VIM_MAX_COUNT;
use super::super::state::vim_adjust_marks_for_edit;

pub(in crate::editor_vim_key_events) fn vim_join_lines(
    buffer: &mut TextBuffer,
    count: usize,
) -> bool {
    let count = count.clamp(1, VIM_MAX_COUNT);
    let mut changed = false;
    for _ in 0..count {
        let seam = vim_join_seam(buffer);
        if buffer.join_lines() {
            if let Some(seam) = seam {
                vim_adjust_marks_for_edit(
                    buffer.id(),
                    seam.deleted_start,
                    seam.deleted_end,
                    &seam.separator,
                );
            }
            changed = true;
        }
    }
    changed
}

struct VimJoinSeam {
    deleted_start: (usize, usize),
    deleted_end: (usize, usize),
    separator: String,
}

fn vim_join_seam(buffer: &TextBuffer) -> Option<VimJoinSeam> {
    let line = buffer.cursor_position().line;
    let next = line.checked_add(1)?;
    if next >= buffer.len_lines() {
        return None;
    }

    let line_start = buffer.line_column_to_char(line, 0);

    let mut start = buffer.line_content_end_char(line);
    while start > line_start
        && buffer
            .char_at(start.saturating_sub(1))
            .is_some_and(|ch| matches!(ch, ' ' | '\t'))
    {
        start -= 1;
    }

    let next_line_start = buffer.line_column_to_char(next, 0);
    let next_content_end = buffer.line_content_end_char(next);
    let mut end = next_line_start;
    while end < next_content_end
        && buffer
            .char_at(end)
            .is_some_and(|ch| matches!(ch, ' ' | '\t'))
    {
        end += 1;
    }
    if start >= end {
        return None;
    }

    let separator = vim_join_line_separator(buffer, line_start, start, end, next_content_end);
    Some(VimJoinSeam {
        deleted_start: (line, start - line_start),
        deleted_end: (next, end - next_line_start),
        separator,
    })
}

fn vim_join_line_separator(
    buffer: &TextBuffer,
    line_start: usize,
    seam_start: usize,
    seam_end: usize,
    next_content_end: usize,
) -> String {
    let left = seam_start
        .checked_sub(1)
        .and_then(|idx| (idx >= line_start).then(|| buffer.char_at(idx)))
        .flatten();
    let right = (seam_end < next_content_end)
        .then(|| buffer.char_at(seam_end))
        .flatten();
    let (Some(left), Some(right)) = (left, right) else {
        return String::new();
    };

    if matches!(left, '(' | '[' | '.' | ':' | '/' | '\\')
        || matches!(right, ')' | ']' | '}' | ',' | ';' | '.' | ':')
    {
        String::new()
    } else {
        " ".to_owned()
    }
}

pub(in crate::editor_vim_key_events) fn vim_join_lines_without_whitespace(
    buffer: &mut TextBuffer,
    count: usize,
) -> bool {
    let count = count.clamp(1, VIM_MAX_COUNT);
    let mut changed = false;
    for _ in 0..count {
        changed |= vim_join_next_line_without_whitespace(buffer);
    }
    changed
}

fn vim_join_next_line_without_whitespace(buffer: &mut TextBuffer) -> bool {
    let line = buffer.cursor_position().line;
    if line + 1 >= buffer.len_lines() {
        return false;
    }
    if buffer.is_final_newline_line(line + 1) {
        return false;
    }

    let start = buffer.line_content_end_char(line);
    let next_line_start = buffer.line_column_to_char(line + 1, 0);
    let next_indent = buffer
        .line(line + 1)
        .unwrap_or_default()
        .chars()
        .take_while(|ch| matches!(ch, ' ' | '\t'))
        .count();
    let end = next_line_start
        .saturating_add(next_indent)
        .min(buffer.len_chars());
    if start >= end {
        return false;
    }

    let original_cursor = buffer.cursor();
    let changed = buffer.apply_edits(vec![TextEdit {
        range: start..end,
        inserted: String::new(),
    }]);
    if changed {
        buffer.set_single_cursor(original_cursor.min(buffer.len_chars()));
    }
    changed
}
