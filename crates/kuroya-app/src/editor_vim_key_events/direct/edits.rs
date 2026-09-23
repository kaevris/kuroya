use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

use super::super::commands::{
    vim_apply_last_change, vim_change_lines_into_register, vim_put_register_after,
    vim_put_register_before, vim_yank_lines,
};
use super::super::motion::{
    vim_delete_backward_chars, vim_delete_forward_chars, vim_join_lines,
    vim_toggle_case_forward_chars,
};
use super::super::operator::vim_delete_to_line_end;
use super::super::{
    EditorVimLastChange, EditorVimMode, EditorVimPendingKey, EditorVimRegister,
    EditorVimRepeatAction, VimKeyResult, vim_repeatable_change_result,
};

pub(super) fn handle_vim_direct_edit_key(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    indent_unit: &str,
    count: Option<usize>,
    count_value: usize,
    suppress_text: Option<char>,
) -> Option<VimKeyResult> {
    match key {
        Key::Y if modifiers.shift => {
            vim_yank_lines(buffer, count_value, unnamed_register);
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Period if !modifiers.shift => Some(repeat_last_change(
            buffer,
            mode,
            unnamed_register,
            last_change,
            indent_unit,
            count,
            suppress_text,
        )),
        Key::P if !modifiers.shift => Some(vim_repeatable_change_result(
            vim_put_register_after(buffer, unnamed_register.as_ref(), count_value),
            last_change,
            EditorVimRepeatAction::PutAfter,
            count_value,
            suppress_text,
        )),
        Key::P if modifiers.shift => Some(vim_repeatable_change_result(
            vim_put_register_before(buffer, unnamed_register.as_ref(), count_value),
            last_change,
            EditorVimRepeatAction::PutBefore,
            count_value,
            suppress_text,
        )),
        Key::Backtick if modifiers.shift => Some(vim_repeatable_change_result(
            vim_toggle_case_forward_chars(buffer, count_value),
            last_change,
            EditorVimRepeatAction::ToggleCaseForwardChars,
            count_value,
            suppress_text,
        )),
        Key::D if modifiers.shift => Some(vim_repeatable_change_result(
            vim_delete_to_line_end(buffer, count_value, unnamed_register),
            last_change,
            EditorVimRepeatAction::DeleteToLineEnd,
            count_value,
            suppress_text,
        )),
        Key::C if modifiers.shift => {
            let changed = vim_delete_to_line_end(buffer, count_value, unnamed_register);
            *pending = None;
            *mode = EditorVimMode::Insert;
            Some(vim_repeatable_change_result(
                changed,
                last_change,
                EditorVimRepeatAction::ChangeToLineEnd,
                count_value,
                suppress_text,
            ))
        }
        Key::S if !modifiers.shift => {
            let changed = vim_delete_forward_chars(buffer, count_value, unnamed_register);
            *pending = None;
            *mode = EditorVimMode::Insert;
            Some(vim_repeatable_change_result(
                changed,
                last_change,
                EditorVimRepeatAction::SubstituteForwardChars,
                count_value,
                suppress_text,
            ))
        }
        Key::S if modifiers.shift => {
            let changed = vim_change_lines_into_register(buffer, count_value, unnamed_register);
            *pending = None;
            *mode = EditorVimMode::Insert;
            Some(vim_repeatable_change_result(
                changed,
                last_change,
                EditorVimRepeatAction::ChangeLines,
                count_value,
                suppress_text,
            ))
        }
        Key::J if modifiers.shift => Some(vim_repeatable_change_result(
            vim_join_lines(buffer, count_value),
            last_change,
            EditorVimRepeatAction::JoinLines,
            count_value,
            suppress_text,
        )),
        Key::X if !modifiers.shift => Some(vim_repeatable_change_result(
            vim_delete_forward_chars(buffer, count_value, unnamed_register),
            last_change,
            EditorVimRepeatAction::DeleteForwardChars,
            count_value,
            suppress_text,
        )),
        Key::X if modifiers.shift => Some(vim_repeatable_change_result(
            vim_delete_backward_chars(buffer, count_value, unnamed_register),
            last_change,
            EditorVimRepeatAction::DeleteBackwardChars,
            count_value,
            suppress_text,
        )),
        Key::U if !modifiers.shift => {
            let mut changed = false;
            for _ in 0..count_value {
                changed |= buffer.undo();
            }
            if changed {
                Some(VimKeyResult::changed(suppress_text))
            } else {
                Some(VimKeyResult::handled(suppress_text))
            }
        }
        _ => None,
    }
}

fn repeat_last_change(
    buffer: &mut TextBuffer,
    mode: &mut EditorVimMode,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    indent_unit: &str,
    count: Option<usize>,
    suppress_text: Option<char>,
) -> VimKeyResult {
    if let Some(change) = last_change.clone() {
        if vim_apply_last_change(buffer, change, count, mode, unnamed_register, indent_unit) {
            VimKeyResult::changed(suppress_text)
        } else {
            VimKeyResult::handled(suppress_text)
        }
    } else {
        VimKeyResult::handled(suppress_text)
    }
}
