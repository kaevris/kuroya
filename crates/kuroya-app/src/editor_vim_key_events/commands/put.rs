use kuroya_core::{TextBuffer, TextEdit};

use super::super::state::vim_adjust_marks_for_edit;
use super::super::{
    EditorVimRegister, EditorVimRegisterKind, VIM_MAX_COUNT, vim_line_first_non_whitespace_char,
};

pub(in crate::editor_vim_key_events) fn vim_put_register_after(
    buffer: &mut TextBuffer,
    register: Option<&EditorVimRegister>,
    count: usize,
) -> bool {
    vim_put_register(buffer, register, count, true)
}

pub(in crate::editor_vim_key_events) fn vim_put_register_before(
    buffer: &mut TextBuffer,
    register: Option<&EditorVimRegister>,
    count: usize,
) -> bool {
    vim_put_register(buffer, register, count, false)
}

fn vim_put_register(
    buffer: &mut TextBuffer,
    register: Option<&EditorVimRegister>,
    count: usize,
    after: bool,
) -> bool {
    let Some(register) = register else {
        return false;
    };
    match register.kind {
        EditorVimRegisterKind::Characterwise => {
            vim_put_characterwise_register(buffer, register, count, after)
        }
        EditorVimRegisterKind::Linewise => {
            vim_put_linewise_register(buffer, register, count, after)
        }
    }
}

fn vim_put_linewise_register(
    buffer: &mut TextBuffer,
    register: &EditorVimRegister,
    count: usize,
    after: bool,
) -> bool {
    if register.text.is_empty() {
        return false;
    }

    let count = count.clamp(1, VIM_MAX_COUNT);
    let current_line = buffer.cursor_position().line;
    let insert_at = if after && current_line + 1 < buffer.len_lines() {
        buffer.line_column_to_char(current_line + 1, 0)
    } else if after {
        buffer.len_chars()
    } else {
        buffer.line_column_to_char(current_line, 0)
    };

    let mut inserted = String::new();
    for _ in 0..count {
        inserted.push_str(&register.text);
    }

    let cursor_offset = if after
        && insert_at == buffer.len_chars()
        && buffer.len_chars() > 0
        && !vim_buffer_ends_with_line_break(buffer)
    {
        inserted.insert(0, '\n');
        1
    } else {
        0
    };

    let insert_position = buffer.char_position(insert_at);
    let edit = TextEdit {
        range: insert_at..insert_at,
        inserted,
    };
    let changed = buffer.apply_edits_with_inserted_selection(
        vec![edit.clone()],
        &edit,
        cursor_offset..cursor_offset,
    );
    if changed {
        vim_adjust_marks_for_edit(
            buffer.id(),
            (insert_position.line, insert_position.column),
            (insert_position.line, insert_position.column),
            &edit.inserted,
        );

        let cursor_line = if after {
            current_line + 1
        } else {
            current_line
        }
        .min(buffer.len_lines().saturating_sub(1));
        buffer.set_single_cursor(vim_line_first_non_whitespace_char(buffer, cursor_line));
    }
    changed
}

fn vim_put_characterwise_register(
    buffer: &mut TextBuffer,
    register: &EditorVimRegister,
    count: usize,
    after: bool,
) -> bool {
    if register.text.is_empty() {
        return false;
    }
    let insert_at = if after {
        buffer.cursor().saturating_add(1).min(buffer.len_chars())
    } else {
        buffer.cursor()
    };
    let count = count.clamp(1, VIM_MAX_COUNT);
    let mut inserted = String::new();
    for _ in 0..count {
        inserted.push_str(&register.text);
    }
    let inserted_len = inserted.chars().count();
    let cursor_offset = inserted_len.saturating_sub(1);
    let insert_position = buffer.char_position(insert_at);
    let edit = TextEdit {
        range: insert_at..insert_at,
        inserted,
    };
    let changed = buffer.apply_edits_with_inserted_selection(
        vec![edit.clone()],
        &edit,
        cursor_offset..cursor_offset,
    );
    if changed {
        vim_adjust_marks_for_edit(
            buffer.id(),
            (insert_position.line, insert_position.column),
            (insert_position.line, insert_position.column),
            &edit.inserted,
        );
    }
    changed
}

fn vim_buffer_ends_with_line_break(buffer: &TextBuffer) -> bool {
    let len = buffer.len_chars();
    len > 0 && buffer.char_at(len - 1) == Some('\n')
}
