use eframe::egui::{Key, Modifiers};
use kuroya_core::{TextBuffer, TextEdit};

use super::super::state::{
    EditorVimRegisterWriteScope, vim_adjust_marks_for_edit, vim_write_registers_with_scope,
};
use super::super::{
    EditorVimLastChange, EditorVimMode, EditorVimPendingKey, EditorVimRegister,
    EditorVimRegisterKind, EditorVimRepeatAction, VIM_MAX_COUNT, VimKeyResult,
    vim_line_first_non_whitespace_char, vim_repeatable_change_result,
};
use super::selection::{vim_visual_character_line_span, vim_visual_character_range};
use super::{
    vim_convert_case_visual_character, vim_delete_visual_character,
    vim_indent_visual_character_lines, vim_join_visual_character_lines,
    vim_outdent_visual_character_lines, vim_visual_character_case_conversion,
    vim_visual_character_change_key, vim_visual_character_clamped_cursor,
    vim_visual_character_delete_key, vim_visual_character_indent_key,
    vim_visual_character_join_key, vim_visual_character_join_repeat_count,
    vim_visual_character_line_repeat_count, vim_visual_character_outdent_key,
    vim_visual_character_repeat_count, vim_visual_character_replace_key,
    vim_visual_character_yank_key, vim_yank_visual_character,
};

pub(in crate::editor_vim_key_events) fn vim_visual_character_put_key(
    key: Key,
    modifiers: Modifiers,
) -> bool {
    key == Key::P && !modifiers.command && !modifiers.alt && !modifiers.ctrl
}

pub(in crate::editor_vim_key_events) fn vim_put_over_visual_character(
    buffer: &mut TextBuffer,
    anchor: usize,
    cursor: usize,
    source_register: Option<&EditorVimRegister>,
    unnamed_register: &mut Option<EditorVimRegister>,
) -> bool {
    let Some(range) = vim_visual_character_range(buffer, anchor, cursor) else {
        buffer.set_single_cursor(cursor.min(buffer.len_chars()));
        return false;
    };
    let Some(pasted) = source_register
        .filter(|register| !register.text.is_empty())
        .cloned()
    else {
        buffer.set_single_cursor(range.start);
        return false;
    };
    let Some(replaced_text) = buffer.text_range(range.clone()) else {
        buffer.set_single_cursor(range.start);
        return false;
    };
    let deleted_start = buffer.char_position(range.start);
    let deleted_end = buffer.char_position(range.end);
    let edit = TextEdit {
        range: range.start..range.end,
        inserted: pasted.text,
    };
    let changed = buffer.apply_edits_with_inserted_selection(vec![edit.clone()], &edit, 0..0);
    if changed {
        vim_write_registers_with_scope(
            unnamed_register,
            None,
            EditorVimRegister {
                text: replaced_text,
                kind: EditorVimRegisterKind::Characterwise,
            },
            EditorVimRegisterWriteScope::Delete,
        );
        vim_adjust_marks_for_edit(
            buffer.id(),
            (deleted_start.line, deleted_start.column),
            (deleted_end.line, deleted_end.column),
            &edit.inserted,
        );
        buffer.set_single_cursor(range.start);
    }
    changed
}

pub(in crate::editor_vim_key_events) fn vim_put_over_visual_lines(
    buffer: &mut TextBuffer,
    anchor: usize,
    cursor: usize,
    source_register: Option<&EditorVimRegister>,
    unnamed_register: &mut Option<EditorVimRegister>,
    count: Option<usize>,
) -> bool {
    let Some((span, _, _)) = vim_visual_character_line_span(buffer, anchor, cursor) else {
        buffer.set_single_cursor(cursor.min(buffer.len_chars()));
        return false;
    };
    let Some(pasted) = source_register
        .filter(|register| !register.text.is_empty())
        .cloned()
    else {
        buffer.set_single_cursor(span.start);
        return false;
    };
    let Some(replaced_text) = buffer.text_range(span.clone()) else {
        buffer.set_single_cursor(span.start);
        return false;
    };
    let repeat = count.unwrap_or(1).clamp(1, VIM_MAX_COUNT);
    let mut inserted = if repeat == 1 {
        pasted.text
    } else {
        pasted.text.repeat(repeat)
    };

    if replaced_text.ends_with('\n') && !inserted.ends_with('\n') {
        inserted.push('\n');
    }
    let deleted_start = buffer.char_position(span.start);
    let deleted_end = buffer.char_position(span.end);
    let edit = TextEdit {
        range: span.start..span.end,
        inserted,
    };
    let changed = buffer.apply_edits_with_inserted_selection(vec![edit.clone()], &edit, 0..0);
    if changed {
        vim_write_registers_with_scope(
            unnamed_register,
            None,
            EditorVimRegister {
                text: replaced_text,
                kind: EditorVimRegisterKind::Linewise,
            },
            EditorVimRegisterWriteScope::Delete,
        );
        vim_adjust_marks_for_edit(
            buffer.id(),
            (deleted_start.line, deleted_start.column),
            (deleted_end.line, deleted_end.column),
            &edit.inserted,
        );
        buffer.set_single_cursor(vim_line_first_non_whitespace_char(
            buffer,
            deleted_start.line,
        ));
    }
    changed
}

pub(in crate::editor_vim_key_events) fn handle_vim_visual_character_action_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    anchor: usize,
    cursor: usize,
    count: Option<usize>,
    indent_unit: &str,
    suppress_text: Option<char>,
) -> Option<VimKeyResult> {
    if vim_visual_character_yank_key(key, modifiers) {
        vim_yank_visual_character(buffer, anchor, cursor, unnamed_register);

        super::vim_record_visual_bounds_marks(buffer, anchor, cursor);
        *pending = None;
        return Some(VimKeyResult::handled(suppress_text));
    }
    if vim_visual_character_put_key(key, modifiers) {
        let repeat = count.unwrap_or(1).clamp(1, VIM_MAX_COUNT);
        let source = unnamed_register.clone().map(|register| EditorVimRegister {
            text: if repeat == 1 {
                register.text
            } else {
                register.text.repeat(repeat)
            },
            kind: register.kind,
        });
        let changed = vim_put_over_visual_character(
            buffer,
            anchor,
            cursor,
            source.as_ref(),
            unnamed_register,
        );
        *pending = None;
        return Some(if changed {
            VimKeyResult::changed(suppress_text)
        } else {
            VimKeyResult::handled(suppress_text)
        });
    }
    if vim_visual_character_join_key(key, modifiers) {
        let amount = count
            .unwrap_or(0)
            .max(vim_visual_character_join_repeat_count(buffer, anchor, cursor).saturating_add(1));
        let changed = vim_join_visual_character_lines(buffer, anchor, cursor, amount);
        *pending = None;
        return Some(vim_repeatable_change_result(
            changed,
            last_change,
            EditorVimRepeatAction::JoinLines,
            amount.saturating_sub(1).max(1),
            suppress_text,
        ));
    }
    if vim_visual_character_indent_key(key, modifiers) {
        let repeat_count = vim_visual_character_line_repeat_count(buffer, anchor, cursor);
        let changed = vim_indent_visual_character_lines(
            buffer,
            anchor,
            cursor,
            count.unwrap_or(1),
            indent_unit,
        );
        *pending = None;
        return Some(vim_repeatable_change_result(
            changed,
            last_change,
            EditorVimRepeatAction::IndentLines,
            repeat_count,
            suppress_text,
        ));
    }
    if vim_visual_character_outdent_key(key, modifiers) {
        let repeat_count = vim_visual_character_line_repeat_count(buffer, anchor, cursor);
        let changed = vim_outdent_visual_character_lines(
            buffer,
            anchor,
            cursor,
            count.unwrap_or(1),
            indent_unit,
        );
        *pending = None;
        return Some(vim_repeatable_change_result(
            changed,
            last_change,
            EditorVimRepeatAction::OutdentLines,
            repeat_count,
            suppress_text,
        ));
    }
    if let Some(conversion) = vim_visual_character_case_conversion(key, modifiers) {
        let repeat_count = vim_visual_character_repeat_count(buffer, anchor, cursor);
        let changed = vim_convert_case_visual_character(buffer, anchor, cursor, conversion);
        *pending = None;
        return Some(vim_repeatable_change_result(
            changed,
            last_change,
            EditorVimRepeatAction::ConvertCaseForwardChars(conversion),
            repeat_count,
            suppress_text,
        ));
    }
    if vim_visual_character_delete_key(key, modifiers) {
        let repeat_count = vim_visual_character_repeat_count(buffer, anchor, cursor);

        let area_bounds = vim_visual_character_range(buffer, anchor, cursor).map(|range| {
            let start = vim_visual_character_clamped_cursor(buffer, range.start);
            let end = vim_visual_character_clamped_cursor(
                buffer,
                range.end.saturating_sub(1).max(range.start),
            );
            let start = buffer.char_position(start);
            let end = buffer.char_position(end);
            (start, end)
        });
        let changed = vim_delete_visual_character(buffer, anchor, cursor, unnamed_register);
        if changed && let Some((start, end)) = area_bounds {
            super::super::state::vim_store_visual_bounds_mark_positions(
                buffer,
                (start.line, start.column),
                (end.line, end.column),
            );
        }
        *pending = None;
        return Some(vim_repeatable_change_result(
            changed,
            last_change,
            EditorVimRepeatAction::DeleteForwardChars,
            repeat_count,
            suppress_text,
        ));
    }
    if vim_visual_character_change_key(key, modifiers) {
        let repeat_count = vim_visual_character_repeat_count(buffer, anchor, cursor);
        let changed = vim_delete_visual_character(buffer, anchor, cursor, unnamed_register);
        *pending = None;
        return Some(if changed {
            *mode = EditorVimMode::Insert;
            vim_repeatable_change_result(
                changed,
                last_change,
                EditorVimRepeatAction::SubstituteForwardChars,
                repeat_count,
                suppress_text,
            )
        } else {
            VimKeyResult::handled(suppress_text)
        });
    }
    if vim_visual_character_replace_key(key, modifiers) {
        *pending = Some(EditorVimPendingKey::VisualCharacterReplace { anchor, cursor });
        return Some(VimKeyResult::handled(suppress_text));
    }

    None
}
