use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

use super::super::{
    EditorVimCharFind, EditorVimCharFindMotion, EditorVimPendingKey, VimKeyResult, vim_escape_key,
};
use super::{
    vim_exit_visual_selection, vim_set_visual_character_selection,
    vim_visual_character_char_find_target, vim_visual_character_clamped_cursor,
};

pub(in crate::editor_vim_key_events) fn handle_vim_visual_character_char_find_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    pending: &mut Option<EditorVimPendingKey>,
    last_char_find: &mut Option<EditorVimCharFind>,
    anchor: usize,
    cursor: usize,
    count: Option<usize>,
    motion: EditorVimCharFindMotion,
    suppress_text: Option<char>,
) -> VimKeyResult {
    let anchor = vim_visual_character_clamped_cursor(buffer, anchor);
    let cursor = vim_visual_character_clamped_cursor(buffer, cursor);
    if vim_escape_key(key, modifiers) {
        *pending = None;
        vim_exit_visual_selection(buffer, anchor, cursor);
        return VimKeyResult::handled(suppress_text);
    }
    if modifiers.command || modifiers.alt || modifiers.ctrl {
        *pending = Some(EditorVimPendingKey::VisualCharacterCharFind {
            anchor,
            cursor,
            count,
            motion,
        });
        return VimKeyResult::ignored();
    }
    if let Some(target_char) = suppress_text {
        if let Some(target) = vim_visual_character_char_find_target(
            buffer,
            cursor,
            count.unwrap_or(1),
            motion,
            target_char,
        ) {
            *last_char_find = Some(EditorVimCharFind {
                motion,
                target: target_char,
            });
            vim_set_visual_character_selection(buffer, anchor, target);
            *pending = Some(EditorVimPendingKey::VisualCharacter {
                anchor,
                cursor: target,
            });
        } else {
            vim_set_visual_character_selection(buffer, anchor, cursor);
            *pending = Some(EditorVimPendingKey::VisualCharacter { anchor, cursor });
        }
        return VimKeyResult::handled(suppress_text);
    }

    *pending = Some(EditorVimPendingKey::VisualCharacterCharFind {
        anchor,
        cursor,
        count,
        motion,
    });
    VimKeyResult::ignored()
}
