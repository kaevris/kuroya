use eframe::egui::{Key, Modifiers};

use super::super::EditorVimCaseConversion;
pub(in crate::editor_vim_key_events) fn vim_visual_character_toggle_key(
    key: Key,
    modifiers: Modifiers,
) -> bool {
    key == Key::V && !modifiers.shift && !modifiers.command && !modifiers.alt && !modifiers.ctrl
}

pub(in crate::editor_vim_key_events) fn vim_visual_character_yank_key(
    key: Key,
    modifiers: Modifiers,
) -> bool {
    key == Key::Y && !modifiers.command && !modifiers.alt && !modifiers.ctrl
}

pub(in crate::editor_vim_key_events) fn vim_visual_character_join_key(
    key: Key,
    modifiers: Modifiers,
) -> bool {
    key == Key::J && modifiers.shift && !modifiers.command && !modifiers.alt && !modifiers.ctrl
}

pub(in crate::editor_vim_key_events) fn vim_visual_character_indent_key(
    key: Key,
    modifiers: Modifiers,
) -> bool {
    key == Key::Period && modifiers.shift && !modifiers.command && !modifiers.alt && !modifiers.ctrl
}

pub(in crate::editor_vim_key_events) fn vim_visual_character_outdent_key(
    key: Key,
    modifiers: Modifiers,
) -> bool {
    key == Key::Comma && modifiers.shift && !modifiers.command && !modifiers.alt && !modifiers.ctrl
}

pub(in crate::editor_vim_key_events) fn vim_visual_character_swap_key(
    key: Key,
    modifiers: Modifiers,
) -> bool {
    key == Key::O && !modifiers.command && !modifiers.alt && !modifiers.ctrl
}

pub(in crate::editor_vim_key_events) fn vim_visual_character_case_conversion(
    key: Key,
    modifiers: Modifiers,
) -> Option<EditorVimCaseConversion> {
    if modifiers.command || modifiers.alt || modifiers.ctrl {
        return None;
    }
    match (key, modifiers.shift) {
        (Key::Backtick, true) => Some(EditorVimCaseConversion::Toggle),
        (Key::U, false) => Some(EditorVimCaseConversion::Lower),
        (Key::U, true) => Some(EditorVimCaseConversion::Upper),
        _ => None,
    }
}

pub(in crate::editor_vim_key_events) fn vim_visual_character_delete_key(
    key: Key,
    modifiers: Modifiers,
) -> bool {
    if modifiers.command || modifiers.alt || modifiers.ctrl {
        return false;
    }
    matches!(
        (key, modifiers.shift),
        (Key::D, false) | (Key::D, true) | (Key::X, false) | (Key::X, true)
    )
}

pub(in crate::editor_vim_key_events) fn vim_visual_character_change_key(
    key: Key,
    modifiers: Modifiers,
) -> bool {
    if modifiers.command || modifiers.alt || modifiers.ctrl {
        return false;
    }
    matches!(
        (key, modifiers.shift),
        (Key::C, false) | (Key::C, true) | (Key::S, false) | (Key::S, true)
    )
}

pub(in crate::editor_vim_key_events) fn vim_visual_character_replace_key(
    key: Key,
    modifiers: Modifiers,
) -> bool {
    key == Key::R && !modifiers.shift && !modifiers.command && !modifiers.alt && !modifiers.ctrl
}

pub(in crate::editor_vim_key_events) fn vim_visual_character_char_find_repeat_key(
    key: Key,
    modifiers: Modifiers,
) -> Option<bool> {
    if modifiers.command || modifiers.alt || modifiers.ctrl || modifiers.shift {
        return None;
    }
    match key {
        Key::Semicolon => Some(false),
        Key::Comma => Some(true),
        _ => None,
    }
}
