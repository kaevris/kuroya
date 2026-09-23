use super::{VIM_SEARCH_INPUT, vim_literal_search};
use crate::editor_vim_key_events::input_edit::{
    EditorVimInputEdit, vim_clear_input, vim_delete_input_word_backward, vim_input_control_edit,
    vim_pop_input, vim_push_input,
};
use crate::editor_vim_key_events::{
    EditorVimPendingKey, VimKeyResult, no_text_modifiers, vim_printable_key_char,
};
use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

pub(crate) fn vim_clear_search_input() {
    vim_clear_input(&VIM_SEARCH_INPUT);
}

fn vim_finish_pending_literal_search(buffer: &mut TextBuffer, count: usize, forward: bool) -> bool {
    VIM_SEARCH_INPUT.with(|input| {
        let mut query = input.borrow_mut();
        let moved = vim_literal_search(buffer, &query, count, forward);
        query.clear();
        moved
    })
}

pub(in crate::editor_vim_key_events) fn handle_vim_search_input_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    pending: &mut Option<EditorVimPendingKey>,
    count: usize,
    forward: bool,
    suppress_text: Option<char>,
) -> VimKeyResult {
    if vim_search_input_accept_key(key, modifiers) {
        vim_finish_pending_literal_search(buffer, count, forward);
        return VimKeyResult::handled(None);
    }

    if let Some(edit) = vim_search_input_control_edit(key, modifiers) {
        match edit {
            EditorVimInputEdit::DeleteCharBackward => vim_pop_input(&VIM_SEARCH_INPUT),
            EditorVimInputEdit::Clear => vim_clear_search_input(),
            EditorVimInputEdit::DeleteWordBackward => {
                vim_delete_input_word_backward(&VIM_SEARCH_INPUT)
            }
        }
        *pending = Some(EditorVimPendingKey::SearchInput { count, forward });
        return VimKeyResult::handled(None);
    }

    if let Some(ch) = vim_printable_key_char(key, modifiers) {
        vim_push_input(&VIM_SEARCH_INPUT, ch);
        *pending = Some(EditorVimPendingKey::SearchInput { count, forward });
        return VimKeyResult::handled(suppress_text);
    }

    *pending = Some(EditorVimPendingKey::SearchInput { count, forward });
    VimKeyResult::handled(None)
}

pub(in crate::editor_vim_key_events) fn vim_search_input_control_edit(
    key: Key,
    modifiers: Modifiers,
) -> Option<EditorVimInputEdit> {
    vim_input_control_edit(key, modifiers)
}

pub(in crate::editor_vim_key_events) fn vim_search_input_accept_key(
    key: Key,
    modifiers: Modifiers,
) -> bool {
    (key == Key::Enter && no_text_modifiers(modifiers))
        || (matches!(key, Key::J | Key::M)
            && modifiers.ctrl
            && !modifiers.shift
            && !modifiers.alt
            && !modifiers.command)
}

pub(in crate::editor_vim_key_events) fn vim_search_input_cancel_key(
    key: Key,
    modifiers: Modifiers,
) -> bool {
    key == Key::C && modifiers.ctrl && !modifiers.shift && !modifiers.alt && !modifiers.command
}
