use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

mod substates;

use self::substates::{VisualPendingStateAfterKey, vim_visual_pending_substate_after_key};
use super::super::{
    EditorVimPendingKey, vim_count_digit, vim_escape_key, vim_operator_char_find_motion_for_key,
    vim_push_count_digit, vim_text_object_scope_for_key,
};
use super::keys::{
    vim_visual_character_case_conversion, vim_visual_character_change_key,
    vim_visual_character_char_find_repeat_key, vim_visual_character_delete_key,
    vim_visual_character_indent_key, vim_visual_character_join_key,
    vim_visual_character_outdent_key, vim_visual_character_replace_key,
    vim_visual_character_swap_key, vim_visual_character_toggle_key, vim_visual_character_yank_key,
};
use super::line::vim_visual_line_pending_after_key;
use super::motion::vim_visual_character_motion_key;
use super::selection::vim_exit_visual_selection;
pub(in crate::editor_vim_key_events) fn vim_visual_pending_after_key(
    pending: Option<EditorVimPendingKey>,
    key: Key,
    modifiers: Modifiers,
    printable_key_char: Option<char>,
) -> Option<Option<EditorVimPendingKey>> {
    if matches!(
        pending,
        Some(EditorVimPendingKey::VisualLine { .. } | EditorVimPendingKey::VisualLineGo { .. })
    ) {
        return vim_visual_line_pending_after_key(pending, key, modifiers, printable_key_char);
    }
    let base =
        match vim_visual_pending_substate_after_key(pending, key, modifiers, printable_key_char) {
            VisualPendingStateAfterKey::Base(base) => base,
            VisualPendingStateAfterKey::Resolved(result) => return result,
        };
    let (anchor, cursor, count) = (base.anchor, base.cursor, base.count);
    if vim_escape_key(key, modifiers)
        || vim_visual_character_toggle_key(key, modifiers)
        || vim_visual_character_yank_key(key, modifiers)
        || vim_visual_character_join_key(key, modifiers)
        || vim_visual_character_indent_key(key, modifiers)
        || vim_visual_character_outdent_key(key, modifiers)
        || vim_visual_character_case_conversion(key, modifiers).is_some()
        || vim_visual_character_delete_key(key, modifiers)
        || vim_visual_character_change_key(key, modifiers)
    {
        return Some(None);
    }
    if modifiers.command || modifiers.alt || modifiers.ctrl {
        return None;
    }
    if key == Key::Quote && modifiers.shift {
        return Some(Some(EditorVimPendingKey::VisualCharacterRegisterPrefix {
            anchor,
            cursor,
            count,
        }));
    }
    if let Some(scope) = vim_text_object_scope_for_key(key, modifiers) {
        return Some(Some(EditorVimPendingKey::VisualCharacterTextObject {
            anchor,
            cursor,
            count,
            scope,
        }));
    }
    if let Some(motion) = vim_operator_char_find_motion_for_key(key, modifiers) {
        return Some(Some(EditorVimPendingKey::VisualCharacterCharFind {
            anchor,
            cursor,
            count,
            motion,
        }));
    }
    if vim_visual_character_replace_key(key, modifiers) {
        return Some(Some(EditorVimPendingKey::VisualCharacterReplace {
            anchor,
            cursor,
        }));
    }
    if vim_visual_character_char_find_repeat_key(key, modifiers).is_some() {
        return Some(Some(EditorVimPendingKey::VisualCharacter {
            anchor,
            cursor,
        }));
    }
    if vim_visual_character_swap_key(key, modifiers) {
        return Some(Some(EditorVimPendingKey::VisualCharacter {
            anchor: cursor,
            cursor: anchor,
        }));
    }
    if key == Key::G && !modifiers.shift {
        return Some(Some(EditorVimPendingKey::VisualCharacterGo {
            anchor,
            cursor,
            count,
        }));
    }
    if count.is_none()
        && let Some(digit) = vim_count_digit(key, modifiers, false)
    {
        return Some(Some(EditorVimPendingKey::VisualCharacterCount {
            anchor,
            cursor,
            count: digit,
        }));
    }
    if let Some(count) = count
        && let Some(digit) = vim_count_digit(key, modifiers, true)
    {
        return Some(Some(EditorVimPendingKey::VisualCharacterCount {
            anchor,
            cursor,
            count: vim_push_count_digit(count, digit),
        }));
    }
    if vim_visual_character_motion_key(key, modifiers) {
        return Some(Some(EditorVimPendingKey::VisualCharacter {
            anchor,
            cursor,
        }));
    }
    printable_key_char.is_some().then_some(pending)
}

pub(in crate::editor_vim_key_events) fn vim_restore_visual_character_pending(
    pending: &mut Option<EditorVimPendingKey>,
    anchor: usize,
    cursor: usize,
    count: Option<usize>,
) {
    *pending = Some(if let Some(count) = count {
        EditorVimPendingKey::VisualCharacterCount {
            anchor,
            cursor,
            count,
        }
    } else {
        EditorVimPendingKey::VisualCharacter { anchor, cursor }
    });
}

pub(in crate::editor_vim_key_events) fn vim_cancel_pending_visual_character(
    buffer: &mut TextBuffer,
    pending: Option<EditorVimPendingKey>,
) {
    if let Some(
        EditorVimPendingKey::VisualCharacter { anchor, cursor, .. }
        | EditorVimPendingKey::VisualCharacterCount { anchor, cursor, .. }
        | EditorVimPendingKey::VisualCharacterGo { anchor, cursor, .. }
        | EditorVimPendingKey::VisualCharacterReplace { anchor, cursor }
        | EditorVimPendingKey::VisualLineReplace { anchor, cursor }
        | EditorVimPendingKey::VisualCharacterCharFind { anchor, cursor, .. }
        | EditorVimPendingKey::VisualCharacterTextObject { anchor, cursor, .. }
        | EditorVimPendingKey::VisualCharacterRegisterPrefix { anchor, cursor, .. }
        | EditorVimPendingKey::VisualCharacterRegisterCommand { anchor, cursor, .. }
        | EditorVimPendingKey::VisualLine { anchor, cursor, .. }
        | EditorVimPendingKey::VisualLineGo { anchor, cursor, .. },
    ) = pending
    {
        vim_exit_visual_selection(buffer, anchor, cursor);
    }
}
