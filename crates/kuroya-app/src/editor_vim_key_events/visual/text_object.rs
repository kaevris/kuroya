use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

use super::super::{
    EditorVimPendingKey, EditorVimTextObjectScope, VimKeyResult, vim_escape_key,
    vim_text_object_kind_for_key, vim_text_object_range,
};
use super::{
    vim_exit_visual_selection, vim_set_visual_character_selection,
    vim_visual_character_clamped_cursor,
};

pub(in crate::editor_vim_key_events) fn handle_vim_visual_character_text_object_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    pending: &mut Option<EditorVimPendingKey>,
    anchor: usize,
    cursor: usize,
    count: Option<usize>,
    scope: EditorVimTextObjectScope,
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
        *pending = Some(EditorVimPendingKey::VisualCharacterTextObject {
            anchor,
            cursor,
            count,
            scope,
        });
        return VimKeyResult::ignored();
    }
    if let Some(kind) = vim_text_object_kind_for_key(key, modifiers) {
        let original_cursor = buffer.cursor();
        buffer.set_single_cursor(cursor);
        let range = vim_text_object_range(buffer, count.unwrap_or(1), scope, kind);
        buffer.set_single_cursor(original_cursor);

        if let Some(range) = range
            && range.start < range.end
        {
            let object_cursor = range.end.saturating_sub(1);
            vim_set_visual_character_selection(buffer, range.start, object_cursor);
            *pending = Some(EditorVimPendingKey::VisualCharacter {
                anchor: range.start,
                cursor: object_cursor,
            });
            return VimKeyResult::handled(suppress_text);
        }

        vim_set_visual_character_selection(buffer, anchor, cursor);
        *pending = Some(EditorVimPendingKey::VisualCharacter { anchor, cursor });
        return VimKeyResult::handled(suppress_text);
    }

    *pending = Some(EditorVimPendingKey::VisualCharacterTextObject {
        anchor,
        cursor,
        count,
        scope,
    });
    if suppress_text.is_some() {
        VimKeyResult::handled(suppress_text)
    } else {
        VimKeyResult::ignored()
    }
}
