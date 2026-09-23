use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

use super::super::{
    EditorVimCharFind, EditorVimLastChange, EditorVimMode, EditorVimPendingKey, EditorVimRegister,
    VimKeyResult, vim_count_digit, vim_escape_key, vim_operator_char_find_motion_for_key,
    vim_push_count_digit, vim_text_object_scope_for_key,
};
use super::{
    character_action::handle_vim_visual_character_action_key_event,
    character_navigation::handle_vim_visual_character_navigation_key_event,
    vim_exit_visual_selection, vim_restore_visual_character_pending,
    vim_set_visual_character_selection, vim_visual_character_clamped_cursor,
    vim_visual_character_swap_key, vim_visual_character_toggle_key,
};

pub(in crate::editor_vim_key_events) fn handle_vim_visual_character_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    last_char_find: &mut Option<EditorVimCharFind>,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    anchor: usize,
    cursor: usize,
    count: Option<usize>,
    indent_unit: &str,
    suppress_text: Option<char>,
) -> VimKeyResult {
    let anchor = vim_visual_character_clamped_cursor(buffer, anchor);
    let cursor = vim_visual_character_clamped_cursor(buffer, cursor);
    if vim_escape_key(key, modifiers) || vim_visual_character_toggle_key(key, modifiers) {
        *pending = None;
        vim_exit_visual_selection(buffer, anchor, cursor);
        return VimKeyResult::handled(suppress_text);
    }
    if modifiers.command || modifiers.alt || modifiers.ctrl {
        vim_restore_visual_character_pending(pending, anchor, cursor, count);
        return VimKeyResult::ignored();
    }

    if key == Key::V && modifiers.shift && !modifiers.command && !modifiers.alt {
        super::vim_set_visual_line_selection(buffer, anchor, cursor);
        *pending = Some(EditorVimPendingKey::VisualLine {
            anchor,
            cursor,
            count: None,
        });
        return VimKeyResult::handled(suppress_text);
    }
    if count.is_none()
        && let Some(digit) = vim_count_digit(key, modifiers, false)
    {
        *pending = Some(EditorVimPendingKey::VisualCharacterCount {
            anchor,
            cursor,
            count: digit,
        });
        return VimKeyResult::handled(suppress_text);
    }
    if let Some(count) = count
        && let Some(digit) = vim_count_digit(key, modifiers, true)
    {
        *pending = Some(EditorVimPendingKey::VisualCharacterCount {
            anchor,
            cursor,
            count: vim_push_count_digit(count, digit),
        });
        return VimKeyResult::handled(suppress_text);
    }
    if key == Key::Quote && modifiers.shift {
        *pending = Some(EditorVimPendingKey::VisualCharacterRegisterPrefix {
            anchor,
            cursor,
            count,
        });
        return VimKeyResult::handled(suppress_text);
    }
    if let Some(scope) = vim_text_object_scope_for_key(key, modifiers) {
        *pending = Some(EditorVimPendingKey::VisualCharacterTextObject {
            anchor,
            cursor,
            count,
            scope,
        });
        return VimKeyResult::handled(suppress_text);
    }
    if let Some(motion) = vim_operator_char_find_motion_for_key(key, modifiers) {
        *pending = Some(EditorVimPendingKey::VisualCharacterCharFind {
            anchor,
            cursor,
            count,
            motion,
        });
        return VimKeyResult::handled(suppress_text);
    }
    if vim_visual_character_swap_key(key, modifiers) {
        vim_set_visual_character_selection(buffer, cursor, anchor);
        *pending = Some(EditorVimPendingKey::VisualCharacter {
            anchor: cursor,
            cursor: anchor,
        });
        return VimKeyResult::handled(suppress_text);
    }
    if key == Key::G && !modifiers.shift {
        *pending = Some(EditorVimPendingKey::VisualCharacterGo {
            anchor,
            cursor,
            count,
        });
        return VimKeyResult::handled(suppress_text);
    }
    if let Some(result) = handle_vim_visual_character_action_key_event(
        buffer,
        key,
        modifiers,
        mode,
        pending,
        unnamed_register,
        last_change,
        anchor,
        cursor,
        count,
        indent_unit,
        suppress_text,
    ) {
        return result;
    }

    if let Some(result) = handle_vim_visual_character_navigation_key_event(
        buffer,
        key,
        modifiers,
        pending,
        last_char_find,
        anchor,
        cursor,
        count,
        suppress_text,
    ) {
        return result;
    }

    vim_restore_visual_character_pending(pending, anchor, cursor, count);
    if suppress_text.is_some() {
        VimKeyResult::handled(suppress_text)
    } else {
        VimKeyResult::ignored()
    }
}
