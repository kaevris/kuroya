use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

use super::super::{
    EditorVimCharFind, EditorVimLastChange, EditorVimMode, EditorVimPendingKey, EditorVimRegister,
    EditorVimRepeatAction, VimKeyResult, vim_repeatable_change_result,
    vim_search_word_under_cursor,
};
use super::{
    handle_vim_visual_character_key_event, vim_join_visual_character_lines_without_whitespace,
    vim_set_visual_character_selection, vim_visual_character_clamped_cursor,
    vim_visual_character_join_repeat_count,
};

pub(in crate::editor_vim_key_events) fn handle_vim_visual_character_go_key_event(
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
    match key {
        Key::Num8 if modifiers.shift => {
            buffer.set_single_cursor(cursor.min(buffer.len_chars()));
            vim_search_word_under_cursor(buffer, count.unwrap_or(1), true, false);
            let target = vim_visual_character_clamped_cursor(buffer, buffer.cursor());
            vim_set_visual_character_selection(buffer, anchor, target);
            *pending = Some(EditorVimPendingKey::VisualCharacter {
                anchor,
                cursor: target,
            });
            VimKeyResult::handled(suppress_text)
        }
        Key::Num3 if modifiers.shift => {
            buffer.set_single_cursor(cursor.min(buffer.len_chars()));
            vim_search_word_under_cursor(buffer, count.unwrap_or(1), false, false);
            let target = vim_visual_character_clamped_cursor(buffer, buffer.cursor());
            vim_set_visual_character_selection(buffer, anchor, target);
            *pending = Some(EditorVimPendingKey::VisualCharacter {
                anchor,
                cursor: target,
            });
            VimKeyResult::handled(suppress_text)
        }
        Key::J if modifiers.shift => {
            let amount = count.unwrap_or(0).max(
                vim_visual_character_join_repeat_count(buffer, anchor, cursor).saturating_add(1),
            );
            let changed =
                vim_join_visual_character_lines_without_whitespace(buffer, anchor, cursor, amount);
            *pending = None;
            vim_repeatable_change_result(
                changed,
                last_change,
                EditorVimRepeatAction::JoinLinesWithoutWhitespace,
                amount.saturating_sub(1).max(1),
                suppress_text,
            )
        }
        _ => handle_vim_visual_character_key_event(
            buffer,
            key,
            modifiers,
            mode,
            pending,
            last_char_find,
            unnamed_register,
            last_change,
            anchor,
            cursor,
            count,
            indent_unit,
            suppress_text,
        ),
    }
}
