use kuroya_core::TextBuffer;

use super::super::state::vim_adjust_marks_for_edit;
use super::super::{VIM_MAX_COUNT, vim_line_range_for_count};

pub(in crate::editor_vim_key_events) fn vim_open_line_below(buffer: &mut TextBuffer) {
    let line = buffer.cursor_position().line;
    let indent = vim_line_indent(buffer, line);
    let insert_at = buffer.line_content_end_char(line);
    let insert_column = buffer.char_position(insert_at).column;
    buffer.move_line_end();
    let inserted = vim_open_line_below_text(&indent);
    buffer.insert_at_cursors(&inserted);
    vim_adjust_marks_for_edit(
        buffer.id(),
        (line, insert_column),
        (line, insert_column),
        &inserted,
    );
}

pub(in crate::editor_vim_key_events) fn vim_open_line_above(buffer: &mut TextBuffer) {
    let line = buffer.cursor_position().line;
    let line_start = buffer.line_column_to_char(line, 0);
    let indent = vim_line_indent(buffer, line);
    let cursor = line_start.saturating_add(indent.chars().count());
    buffer.set_single_cursor(line_start);
    let inserted = vim_open_line_above_text(&indent);
    buffer.insert_at_cursors(&inserted);
    buffer.set_single_cursor(cursor);
    vim_adjust_marks_for_edit(buffer.id(), (line, 0), (line, 0), &inserted);
}

pub(in crate::editor_vim_key_events) fn vim_open_line_below_text(indent: &str) -> String {
    let mut text = String::with_capacity(1 + indent.len());
    text.push('\n');
    text.push_str(indent);
    text
}

pub(in crate::editor_vim_key_events) fn vim_open_line_above_text(indent: &str) -> String {
    let mut text = String::with_capacity(indent.len() + 1);
    text.push_str(indent);
    text.push('\n');
    text
}

fn vim_line_indent(buffer: &TextBuffer, line: usize) -> String {
    buffer
        .line(line)
        .unwrap_or_default()
        .chars()
        .take_while(|ch| matches!(ch, ' ' | '\t'))
        .collect()
}

pub(in crate::editor_vim_key_events) fn vim_indent_lines(
    buffer: &mut TextBuffer,
    count: usize,
    indent_unit: &str,
) -> bool {
    if indent_unit.is_empty() {
        return false;
    }
    let position = buffer.cursor_position();
    let Some(range) = vim_line_range_for_count(buffer, count) else {
        return false;
    };
    let first_line = buffer.char_position(range.start).line;
    let last_exclusive = (first_line + count.clamp(1, VIM_MAX_COUNT)).min(buffer.len_lines());

    buffer.set_selection(range.start, range.end);
    let changed = buffer.indent_lines(indent_unit);
    if changed {
        let column = position.column.saturating_add(indent_unit.chars().count());
        buffer.set_single_cursor(buffer.line_column_to_char(position.line, column));
        for line in first_line..last_exclusive {
            vim_adjust_marks_for_edit(buffer.id(), (line, 0), (line, 0), indent_unit);
        }
    }
    changed
}

pub(in crate::editor_vim_key_events) fn vim_outdent_lines(
    buffer: &mut TextBuffer,
    count: usize,
    indent_unit: &str,
) -> bool {
    let position = buffer.cursor_position();
    let Some(range) = vim_line_range_for_count(buffer, count) else {
        return false;
    };
    let first_line = buffer.char_position(range.start).line;
    let last_exclusive = (first_line + count.clamp(1, VIM_MAX_COUNT)).min(buffer.len_lines());
    let removed_widths = (first_line..last_exclusive)
        .map(|line| vim_line_outdent_len(buffer, line, indent_unit))
        .collect::<Vec<_>>();

    buffer.set_selection(range.start, range.end);
    let changed = buffer.outdent_lines(indent_unit);
    if changed {
        let cursor_remove_len = removed_widths
            .get(position.line.saturating_sub(first_line))
            .copied()
            .unwrap_or(0);
        let column = position.column.saturating_sub(cursor_remove_len);
        buffer.set_single_cursor(buffer.line_column_to_char(position.line, column));
        for (offset, removed) in removed_widths.iter().enumerate() {
            if *removed > 0 {
                vim_adjust_marks_for_edit(
                    buffer.id(),
                    (first_line + offset, 0),
                    (first_line + offset, *removed),
                    "",
                );
            }
        }
    }
    changed
}

pub(in crate::editor_vim_key_events) fn vim_line_outdent_len(
    buffer: &TextBuffer,
    line: usize,
    indent_unit: &str,
) -> usize {
    let indent_width = indent_unit.chars().count().max(1);
    let Some(text) = buffer.line(line) else {
        return 0;
    };
    let mut chars = text.chars();
    if matches!(chars.next(), Some('\t')) {
        return 1;
    }

    text.chars()
        .take_while(|ch| *ch == ' ')
        .take(indent_width)
        .count()
}
