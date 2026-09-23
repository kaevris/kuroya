use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

mod edits;
mod insert_transition;
mod motions;
mod pending_starter;

use self::edits::handle_vim_direct_edit_key;
use self::insert_transition::handle_vim_direct_insert_transition_key;
use self::motions::handle_vim_direct_motion_key;
use self::pending_starter::handle_vim_direct_pending_starter_key;
use super::{
    EditorVimCharFind, EditorVimLastChange, EditorVimMode, EditorVimPendingKey, EditorVimRegister,
    VimKeyResult,
};

pub(super) fn handle_vim_direct_normal_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    last_char_find: &mut Option<EditorVimCharFind>,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    indent_unit: &str,
    count: Option<usize>,
    count_value: usize,
    suppress_text: Option<char>,
) -> VimKeyResult {
    if let Some(result) = handle_vim_direct_pending_starter_key(
        buffer,
        key,
        modifiers,
        pending,
        count,
        count_value,
        suppress_text,
    ) {
        return result;
    }
    if let Some(result) = handle_vim_direct_edit_key(
        buffer,
        key,
        modifiers,
        mode,
        pending,
        unnamed_register,
        last_change,
        indent_unit,
        count,
        count_value,
        suppress_text,
    ) {
        return result;
    }
    if let Some(result) = handle_vim_direct_motion_key(
        buffer,
        key,
        modifiers,
        *last_char_find,
        count,
        count_value,
        suppress_text,
    ) {
        return result;
    }
    if let Some(result) = handle_vim_direct_insert_transition_key(
        buffer,
        key,
        modifiers,
        mode,
        pending,
        last_change,
        count_value,
        suppress_text,
    ) {
        return result;
    }
    VimKeyResult::ignored()
}
