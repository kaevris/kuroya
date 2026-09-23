use kuroya_core::{TextBuffer, TextEdit};
use std::ops::Range;

use super::super::state::{
    EditorVimRegisterWriteScope, vim_adjust_marks_for_edit, vim_write_registers,
    vim_write_registers_with_scope,
};
use super::super::{
    EditorVimNamedRegister, EditorVimRegister, EditorVimRegisterKind, VIM_MAX_COUNT,
};

pub(in crate::editor_vim_key_events) fn vim_delete_range_into_register(
    buffer: &mut TextBuffer,
    range: Range<usize>,
    kind: EditorVimRegisterKind,
    unnamed_register: &mut Option<EditorVimRegister>,
    named_register: Option<EditorVimNamedRegister>,
) -> bool {
    let Some(value) = vim_register_value_for_range(buffer, range.clone(), kind) else {
        return false;
    };
    let deleted_start = buffer.char_position(range.start);
    let deleted_end = buffer.char_position(range.end);

    let caret = buffer.cursor();
    buffer.set_single_cursor(caret);
    let edit = TextEdit {
        range: range.clone(),
        inserted: String::new(),
    };
    let deleted = buffer.apply_edits_with_inserted_selection(vec![edit.clone()], &edit, 0..0);
    if deleted {
        vim_write_registers_with_scope(
            unnamed_register,
            named_register,
            value,
            EditorVimRegisterWriteScope::Delete,
        );
        vim_adjust_marks_for_edit(
            buffer.id(),
            (deleted_start.line, deleted_start.column),
            (deleted_end.line, deleted_end.column),
            "",
        );
    }
    deleted
}

pub(in crate::editor_vim_key_events) fn vim_yank_range_into_register(
    buffer: &TextBuffer,
    range: Range<usize>,
    kind: EditorVimRegisterKind,
    unnamed_register: &mut Option<EditorVimRegister>,
    named_register: Option<EditorVimNamedRegister>,
) -> bool {
    let Some(value) = vim_register_value_for_range(buffer, range, kind) else {
        return false;
    };
    vim_write_registers(unnamed_register, named_register, value);
    true
}

fn vim_register_value_for_range(
    buffer: &TextBuffer,
    range: Range<usize>,
    kind: EditorVimRegisterKind,
) -> Option<EditorVimRegister> {
    let text = buffer.text_range(range)?;
    if text.is_empty() {
        return None;
    }
    Some(EditorVimRegister { text, kind })
}

pub(in crate::editor_vim_key_events) fn vim_delete_to_line_end(
    buffer: &mut TextBuffer,
    count: usize,
    unnamed_register: &mut Option<EditorVimRegister>,
) -> bool {
    vim_delete_to_line_end_into_registers(buffer, count, unnamed_register, None)
}

pub(in crate::editor_vim_key_events) fn vim_delete_to_line_end_into_named_register(
    buffer: &mut TextBuffer,
    count: usize,
    unnamed_register: &mut Option<EditorVimRegister>,
    named_register: EditorVimNamedRegister,
) -> bool {
    vim_delete_to_line_end_into_registers(buffer, count, unnamed_register, Some(named_register))
}

fn vim_delete_to_line_end_into_registers(
    buffer: &mut TextBuffer,
    count: usize,
    unnamed_register: &mut Option<EditorVimRegister>,
    named_register: Option<EditorVimNamedRegister>,
) -> bool {
    let Some(range) = vim_delete_to_line_end_range(buffer, count) else {
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

fn vim_delete_to_line_end_range(buffer: &TextBuffer, count: usize) -> Option<Range<usize>> {
    let count = count.clamp(1, VIM_MAX_COUNT);
    let start = buffer.cursor();
    let start_line = buffer.cursor_position().line;
    let end_line = start_line
        .saturating_add(count.saturating_sub(1))
        .min(buffer.len_lines().saturating_sub(1));
    let end = buffer.line_content_end_char(end_line);
    (start < end).then_some(start..end)
}
