use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

use super::super::{
    EditorVimLastChange, EditorVimPendingKey, EditorVimRepeatAction, VIM_MAX_COUNT, VimKeyResult,
    vim_escape_key, vim_repeatable_change_result,
};
use super::{
    vim_exit_visual_selection, vim_replace_visual_character, vim_replace_visual_line_span,
    vim_visual_character_clamped_cursor, vim_visual_character_line_span,
    vim_visual_character_repeat_count,
};

pub(in crate::editor_vim_key_events) fn handle_vim_visual_character_replace_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    pending: &mut Option<EditorVimPendingKey>,
    last_change: &mut Option<EditorVimLastChange>,
    anchor: usize,
    cursor: usize,
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
        *pending = Some(EditorVimPendingKey::VisualCharacterReplace { anchor, cursor });
        return VimKeyResult::ignored();
    }
    if let Some(replacement) = suppress_text {
        let repeat_count = vim_visual_character_repeat_count(buffer, anchor, cursor);
        let changed = vim_replace_visual_character(buffer, anchor, cursor, replacement);
        *pending = None;
        return vim_repeatable_change_result(
            changed,
            last_change,
            EditorVimRepeatAction::ReplaceForwardChars(replacement),
            repeat_count,
            suppress_text,
        );
    }

    *pending = Some(EditorVimPendingKey::VisualCharacterReplace { anchor, cursor });
    VimKeyResult::ignored()
}

pub(in crate::editor_vim_key_events) fn handle_vim_visual_line_replace_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    pending: &mut Option<EditorVimPendingKey>,
    last_change: &mut Option<EditorVimLastChange>,
    anchor: usize,
    cursor: usize,
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
        *pending = Some(EditorVimPendingKey::VisualLineReplace { anchor, cursor });
        return VimKeyResult::ignored();
    }
    if let Some(replacement) = suppress_text {
        let repeat_count = vim_visual_character_line_span(buffer, anchor, cursor)
            .map(|(span, _, _)| span.end.saturating_sub(span.start).clamp(1, VIM_MAX_COUNT))
            .unwrap_or(1);
        let changed = vim_replace_visual_line_span(buffer, anchor, cursor, replacement);
        *pending = None;
        return vim_repeatable_change_result(
            changed,
            last_change,
            EditorVimRepeatAction::ReplaceForwardChars(replacement),
            repeat_count,
            suppress_text,
        );
    }

    *pending = Some(EditorVimPendingKey::VisualLineReplace { anchor, cursor });
    VimKeyResult::ignored()
}
