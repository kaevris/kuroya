use kuroya_core::{TextBuffer, TextEdit};

use super::super::{
    EditorVimCaseConversion, EditorVimNamedRegister, EditorVimRegister, EditorVimRegisterKind,
    vim_convert_case_range, vim_delete_range_into_register, vim_line_outdent_len,
    vim_yank_range_into_register,
};
use super::selection::{vim_visual_character_line_span, vim_visual_character_range};
pub(in crate::editor_vim_key_events) fn vim_join_visual_character_lines(
    buffer: &mut TextBuffer,
    anchor: usize,
    cursor: usize,
    amount: usize,
) -> bool {
    let Some(range) = vim_visual_character_range(buffer, anchor, cursor) else {
        buffer.set_single_cursor(cursor.min(buffer.len_chars()));
        return false;
    };
    let start_line = buffer.char_position(range.start).line;
    let selection_end_line = buffer.char_position(range.end.saturating_sub(1)).line;

    let join_end_line = selection_end_line
        .max(start_line.saturating_add(amount.saturating_sub(1)))
        .min(buffer.len_lines().saturating_sub(1));
    let join_end = if join_end_line.saturating_add(1) < buffer.len_lines() {
        buffer.line_column_to_char(join_end_line + 1, 0)
    } else {
        buffer.len_chars()
    };
    let cursor = range.start;
    buffer.set_selection(range.start, join_end);
    let changed = buffer.join_lines();
    buffer.set_single_cursor(cursor.min(buffer.len_chars()));
    changed
}

pub(in crate::editor_vim_key_events) fn vim_indent_visual_character_lines(
    buffer: &mut TextBuffer,
    anchor: usize,
    cursor: usize,
    amount: usize,
    indent_unit: &str,
) -> bool {
    let Some((range, selection_start, _)) = vim_visual_character_line_span(buffer, anchor, cursor)
    else {
        buffer.set_single_cursor(cursor.min(buffer.len_chars()));
        return false;
    };
    let position = buffer.char_position(selection_start);
    if indent_unit.is_empty() || amount == 0 {
        buffer.set_single_cursor(selection_start.min(buffer.len_chars()));
        return false;
    }
    let start_line = buffer.char_position(range.start).line;
    let end_line = buffer.char_position(range.end.saturating_sub(1)).line;
    let last_exclusive = end_line.saturating_add(1);

    let mut changed = false;
    for _ in 0..amount {
        let line_start = buffer.line_column_to_char(start_line, 0);
        let line_end = if last_exclusive < buffer.len_lines() {
            buffer.line_column_to_char(last_exclusive, 0)
        } else {
            buffer.len_chars()
        };

        buffer.set_selection(line_end, line_start);
        changed |= buffer.indent_lines(indent_unit);
    }
    if changed {
        let column = position
            .column
            .saturating_add(indent_unit.chars().count().saturating_mul(amount));
        buffer.set_single_cursor(buffer.line_column_to_char(position.line, column));
    } else {
        buffer.set_single_cursor(selection_start.min(buffer.len_chars()));
    }
    changed
}

pub(in crate::editor_vim_key_events) fn vim_outdent_visual_character_lines(
    buffer: &mut TextBuffer,
    anchor: usize,
    cursor: usize,
    amount: usize,
    indent_unit: &str,
) -> bool {
    let Some((range, selection_start, _)) = vim_visual_character_line_span(buffer, anchor, cursor)
    else {
        buffer.set_single_cursor(cursor.min(buffer.len_chars()));
        return false;
    };
    let position = buffer.char_position(selection_start);
    let start_line = buffer.char_position(range.start).line;
    let end_line = buffer.char_position(range.end.saturating_sub(1)).line;
    let last_exclusive = end_line.saturating_add(1);

    let mut changed = false;
    let mut removed_len = 0;
    for _ in 0..amount {
        let line_start = buffer.line_column_to_char(start_line, 0);
        let line_end = if last_exclusive < buffer.len_lines() {
            buffer.line_column_to_char(last_exclusive, 0)
        } else {
            buffer.len_chars()
        };

        buffer.set_selection(line_end, line_start);
        let remove_len = vim_line_outdent_len(buffer, position.line, indent_unit);
        if buffer.outdent_lines(indent_unit) {
            changed = true;
            removed_len += remove_len;
        }
    }
    if changed {
        let column = position.column.saturating_sub(removed_len);
        buffer.set_single_cursor(buffer.line_column_to_char(position.line, column));
    } else {
        buffer.set_single_cursor(selection_start.min(buffer.len_chars()));
    }
    changed
}

pub(in crate::editor_vim_key_events) fn vim_yank_visual_character(
    buffer: &mut TextBuffer,
    anchor: usize,
    cursor: usize,
    unnamed_register: &mut Option<EditorVimRegister>,
) -> bool {
    vim_yank_visual_character_into_registers(buffer, anchor, cursor, unnamed_register, None)
}

pub(in crate::editor_vim_key_events) fn vim_yank_visual_character_into_named_register(
    buffer: &mut TextBuffer,
    anchor: usize,
    cursor: usize,
    unnamed_register: &mut Option<EditorVimRegister>,
    named_register: EditorVimNamedRegister,
) -> bool {
    vim_yank_visual_character_into_registers(
        buffer,
        anchor,
        cursor,
        unnamed_register,
        Some(named_register),
    )
}

pub(in crate::editor_vim_key_events) fn vim_yank_visual_character_into_registers(
    buffer: &mut TextBuffer,
    anchor: usize,
    cursor: usize,
    unnamed_register: &mut Option<EditorVimRegister>,
    named_register: Option<EditorVimNamedRegister>,
) -> bool {
    let Some(range) = vim_visual_character_range(buffer, anchor, cursor) else {
        buffer.set_single_cursor(cursor.min(buffer.len_chars()));
        return false;
    };
    let yanked = vim_yank_range_into_register(
        buffer,
        range.clone(),
        EditorVimRegisterKind::Characterwise,
        unnamed_register,
        named_register,
    );
    buffer.set_single_cursor(range.start);
    yanked
}

pub(in crate::editor_vim_key_events) fn vim_delete_visual_character(
    buffer: &mut TextBuffer,
    anchor: usize,
    cursor: usize,
    unnamed_register: &mut Option<EditorVimRegister>,
) -> bool {
    vim_delete_visual_character_into_registers(buffer, anchor, cursor, unnamed_register, None)
}

pub(in crate::editor_vim_key_events) fn vim_delete_visual_character_into_named_register(
    buffer: &mut TextBuffer,
    anchor: usize,
    cursor: usize,
    unnamed_register: &mut Option<EditorVimRegister>,
    named_register: EditorVimNamedRegister,
) -> bool {
    vim_delete_visual_character_into_registers(
        buffer,
        anchor,
        cursor,
        unnamed_register,
        Some(named_register),
    )
}

pub(in crate::editor_vim_key_events) fn vim_delete_visual_character_into_registers(
    buffer: &mut TextBuffer,
    anchor: usize,
    cursor: usize,
    unnamed_register: &mut Option<EditorVimRegister>,
    named_register: Option<EditorVimNamedRegister>,
) -> bool {
    let Some(range) = vim_visual_character_range(buffer, anchor, cursor) else {
        buffer.set_single_cursor(cursor.min(buffer.len_chars()));
        return false;
    };
    vim_delete_range_into_register(
        buffer,
        range,
        EditorVimRegisterKind::Characterwise,
        unnamed_register,
        named_register,
    )
}

pub(in crate::editor_vim_key_events) fn vim_convert_case_visual_character(
    buffer: &mut TextBuffer,
    anchor: usize,
    cursor: usize,
    conversion: EditorVimCaseConversion,
) -> bool {
    let Some(range) = vim_visual_character_range(buffer, anchor, cursor) else {
        buffer.set_single_cursor(cursor.min(buffer.len_chars()));
        return false;
    };
    vim_convert_case_range(buffer, range.clone(), range.start, conversion)
}

pub(in crate::editor_vim_key_events) fn vim_replace_visual_character(
    buffer: &mut TextBuffer,
    anchor: usize,
    cursor: usize,
    replacement: char,
) -> bool {
    let Some(range) = vim_visual_character_range(buffer, anchor, cursor) else {
        buffer.set_single_cursor(cursor.min(buffer.len_chars()));
        return false;
    };
    vim_replace_visual_range(buffer, range, replacement)
}

pub(in crate::editor_vim_key_events) fn vim_replace_visual_line_span(
    buffer: &mut TextBuffer,
    anchor: usize,
    cursor: usize,
    replacement: char,
) -> bool {
    let Some((span, _, _)) = vim_visual_character_line_span(buffer, anchor, cursor) else {
        buffer.set_single_cursor(cursor.min(buffer.len_chars()));
        return false;
    };
    let start = span.start;
    let changed = vim_replace_visual_range(buffer, span, replacement);
    if changed {
        buffer.set_single_cursor(start.min(buffer.len_chars()));
    }
    changed
}

pub(in crate::editor_vim_key_events) fn vim_join_visual_character_lines_without_whitespace(
    buffer: &mut TextBuffer,
    anchor: usize,
    cursor: usize,
    amount: usize,
) -> bool {
    let Some(range) = vim_visual_character_range(buffer, anchor, cursor) else {
        buffer.set_single_cursor(cursor.min(buffer.len_chars()));
        return false;
    };
    let start_line = buffer.char_position(range.start).line;
    let selection_end_line = buffer.char_position(range.end.saturating_sub(1)).line;

    let join_end_line = selection_end_line
        .max(start_line.saturating_add(amount.saturating_sub(1)))
        .min(buffer.len_lines().saturating_sub(1));
    let mut edits = Vec::new();
    let mut join_cursor = None;
    for line in start_line..join_end_line {
        let newline = buffer.line_content_end_char(line);
        if buffer.char_at(newline) == Some('\n') {
            join_cursor.get_or_insert(newline);
            edits.push(TextEdit {
                range: newline..newline + 1,
                inserted: String::new(),
            });
        }
    }
    if edits.is_empty() {
        buffer.set_single_cursor(range.start.min(buffer.len_chars()));
        return false;
    }
    let changed = buffer.apply_edits(edits);
    buffer.set_single_cursor(join_cursor.unwrap_or(range.start).min(buffer.len_chars()));
    changed
}

fn vim_replace_visual_range(
    buffer: &mut TextBuffer,
    range: std::ops::Range<usize>,
    replacement: char,
) -> bool {
    let mut inserted = String::new();
    let mut cluster_start = range.start;
    while cluster_start < range.end {
        let cluster_end = buffer
            .snap_forward_to_grapheme_boundary(cluster_start + 1)
            .min(range.end);

        if buffer.char_at(cluster_start) == Some('\n') {
            inserted.push('\n');
        } else {
            inserted.push(replacement);
        }
        cluster_start = cluster_end;
    }
    if inserted.is_empty() {
        buffer.set_single_cursor(range.start);
        return false;
    }
    let edit = TextEdit { range, inserted };
    buffer.apply_edits_with_inserted_selection(vec![edit.clone()], &edit, 0..0)
}
