use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

use super::super::*;

pub(super) fn handle_vim_register_pending_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    pending_key: EditorVimPendingKey,
    suppress_text: Option<char>,
) -> Option<VimKeyResult> {
    match pending_key {
        EditorVimPendingKey::RegisterPrefix(count) => {
            let Some(register) = vim_named_register_for_key(key, modifiers) else {
                *pending = None;
                return Some(VimKeyResult::handled(suppress_text));
            };
            *pending = Some(EditorVimPendingKey::RegisterCommand {
                prefix_count: count,
                command_count: None,
                register,
            });
            Some(VimKeyResult::handled(suppress_text))
        }
        EditorVimPendingKey::RegisterCommand {
            prefix_count,
            command_count,
            register,
        } => handle_vim_register_command_key_event(
            buffer,
            key,
            modifiers,
            mode,
            pending,
            unnamed_register,
            last_change,
            prefix_count,
            command_count,
            register,
            suppress_text,
        ),
        _ => None,
    }
}

fn handle_vim_register_command_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    prefix_count: usize,
    command_count: Option<usize>,
    register: EditorVimNamedRegister,
    suppress_text: Option<char>,
) -> Option<VimKeyResult> {
    if let Some(command_count) = vim_register_command_next_count(command_count, key, modifiers) {
        *pending = Some(EditorVimPendingKey::RegisterCommand {
            prefix_count,
            command_count: Some(command_count),
            register,
        });
        return Some(VimKeyResult::handled(suppress_text));
    }

    let count = vim_register_command_count(prefix_count, command_count);
    match (key, modifiers.shift) {
        (Key::C, false) => {
            *pending = Some(EditorVimPendingKey::ChangeLineIntoRegister {
                operator_count: count,
                register,
            });
            Some(VimKeyResult::handled(suppress_text))
        }
        (Key::C, true) => {
            let changed = vim_delete_to_line_end_into_named_register(
                buffer,
                count,
                unnamed_register,
                register,
            );
            *pending = None;
            *mode = EditorVimMode::Insert;
            Some(vim_repeatable_change_result(
                changed,
                last_change,
                EditorVimRepeatAction::ChangeToLineEndIntoRegister(register),
                count,
                suppress_text,
            ))
        }
        (Key::D, false) => {
            *pending = Some(EditorVimPendingKey::DeleteLineIntoRegister {
                operator_count: count,
                register,
            });
            Some(VimKeyResult::handled(suppress_text))
        }
        (Key::D, true) => Some(vim_repeatable_change_result(
            vim_delete_to_line_end_into_named_register(buffer, count, unnamed_register, register),
            last_change,
            EditorVimRepeatAction::DeleteToLineEndIntoRegister(register),
            count,
            suppress_text,
        )),
        (Key::Y, false) => {
            *pending = Some(EditorVimPendingKey::YankLineIntoRegister {
                operator_count: count,
                register,
            });
            Some(VimKeyResult::handled(suppress_text))
        }
        (Key::Y, true) => {
            vim_yank_lines_into_named_register(buffer, count, unnamed_register, register);
            Some(VimKeyResult::handled(suppress_text))
        }
        (Key::P, false) => {
            let named_register = vim_named_register(register);
            Some(vim_repeatable_change_result(
                vim_put_register_after(buffer, named_register.as_ref(), count),
                last_change,
                EditorVimRepeatAction::PutAfterNamed(register),
                count,
                suppress_text,
            ))
        }
        (Key::P, true) => {
            let named_register = vim_named_register(register);
            Some(vim_repeatable_change_result(
                vim_put_register_before(buffer, named_register.as_ref(), count),
                last_change,
                EditorVimRepeatAction::PutBeforeNamed(register),
                count,
                suppress_text,
            ))
        }
        (Key::S, false) => {
            let changed = vim_delete_forward_chars_into_named_register(
                buffer,
                count,
                unnamed_register,
                register,
            );
            *pending = None;
            *mode = EditorVimMode::Insert;
            Some(vim_repeatable_change_result(
                changed,
                last_change,
                EditorVimRepeatAction::SubstituteForwardCharsIntoRegister(register),
                count,
                suppress_text,
            ))
        }
        (Key::S, true) => {
            let changed =
                vim_change_lines_into_named_register(buffer, count, unnamed_register, register);
            *pending = None;
            *mode = EditorVimMode::Insert;
            Some(vim_repeatable_change_result(
                changed,
                last_change,
                EditorVimRepeatAction::ChangeLinesIntoRegister(register),
                count,
                suppress_text,
            ))
        }
        (Key::X, false) => Some(vim_repeatable_change_result(
            vim_delete_forward_chars_into_named_register(buffer, count, unnamed_register, register),
            last_change,
            EditorVimRepeatAction::DeleteForwardCharsIntoRegister(register),
            count,
            suppress_text,
        )),
        (Key::X, true) => Some(vim_repeatable_change_result(
            vim_delete_backward_chars_into_named_register(
                buffer,
                count,
                unnamed_register,
                register,
            ),
            last_change,
            EditorVimRepeatAction::DeleteBackwardCharsIntoRegister(register),
            count,
            suppress_text,
        )),
        _ => None,
    }
}
