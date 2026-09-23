use eframe::egui::{Key, Modifiers};
use kuroya_core::{BufferId, TextBuffer};
use std::collections::HashMap;

use super::{
    EditorVimCharFind, EditorVimInsertReplayStep, EditorVimLastChange, EditorVimMode,
    EditorVimPendingKey, VimKeyResult, vim_delete_line_backward, vim_escape_key,
    vim_insert_delete_char_backward_key, vim_insert_delete_line_backward_key,
    vim_insert_delete_word_backward_key, vim_replay_counted_insert_session,
};

pub(super) fn handle_vim_insert_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    _last_char_find: &mut Option<EditorVimCharFind>,
    last_change: &mut Option<EditorVimLastChange>,
    indent_unit: &str,
) -> VimKeyResult {
    if vim_escape_key(key, modifiers) {
        *mode = EditorVimMode::Normal;
        *pending = None;

        vim_replay_counted_insert_session(buffer, last_change, indent_unit);
        vim_rest_cursor_on_last_typed_char(buffer);
        VimKeyResult::handled(None)
    } else if vim_insert_delete_char_backward_key(key, modifiers) {
        let changed = buffer.delete_backward_with_auto_pair_delete(false);
        if changed {
            vim_record_insert_replay_step(last_change, EditorVimInsertReplayStep::Backspace);
            VimKeyResult::changed(None)
        } else {
            VimKeyResult::handled(None)
        }
    } else if vim_insert_delete_line_backward_key(key, modifiers) {
        let changed = vim_delete_line_backward(buffer);
        if changed {
            vim_record_insert_replay_step(
                last_change,
                EditorVimInsertReplayStep::DeleteLineBackward,
            );
            VimKeyResult::changed(None)
        } else {
            VimKeyResult::handled(None)
        }
    } else if vim_insert_delete_word_backward_key(key, modifiers) {
        let changed = buffer.delete_word_backward();
        if changed {
            vim_record_insert_replay_step(
                last_change,
                EditorVimInsertReplayStep::DeleteWordBackward,
            );
            VimKeyResult::changed(None)
        } else {
            VimKeyResult::handled(None)
        }
    } else {
        VimKeyResult::ignored()
    }
}

fn vim_rest_cursor_on_last_typed_char(buffer: &mut TextBuffer) {
    let position = buffer.cursor_position();
    let line_start = buffer.line_column_to_char(position.line, 0);
    if buffer.cursor() > line_start {
        buffer.move_left();
    }
}

pub(crate) fn vim_record_inserted_text(last_change: &mut Option<EditorVimLastChange>, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(change) = last_change.as_mut()
        && change.action.accepts_inserted_text()
    {
        match change.insert_replay.last_mut() {
            Some(EditorVimInsertReplayStep::InsertText(existing)) => existing.push_str(text),
            _ => change
                .insert_replay
                .push(EditorVimInsertReplayStep::InsertText(text.to_owned())),
        }
    }
}

pub(crate) fn vim_record_insert_replay_key_with_auto_indent(
    last_change: &mut Option<EditorVimLastChange>,
    key: Key,
    modifiers: Modifiers,
    auto_indent: bool,
) {
    let Some(change) = last_change.as_mut() else {
        return;
    };
    if !change.action.accepts_inserted_text() {
        return;
    }
    if modifiers.command || modifiers.alt {
        return;
    }
    if modifiers.ctrl {
        if vim_insert_delete_char_backward_key(key, modifiers) {
            change
                .insert_replay
                .push(EditorVimInsertReplayStep::Backspace);
        } else if vim_insert_delete_line_backward_key(key, modifiers) {
            change
                .insert_replay
                .push(EditorVimInsertReplayStep::DeleteLineBackward);
        } else if vim_insert_delete_word_backward_key(key, modifiers) {
            change
                .insert_replay
                .push(EditorVimInsertReplayStep::DeleteWordBackward);
        }
        return;
    }
    let Some(step) = (match key {
        Key::Backspace if !modifiers.shift => Some(EditorVimInsertReplayStep::Backspace),
        Key::Enter if !modifiers.shift && auto_indent => {
            Some(EditorVimInsertReplayStep::EnterAutoIndent)
        }
        Key::Enter if !modifiers.shift => Some(EditorVimInsertReplayStep::Enter),
        Key::Tab if modifiers.shift => Some(EditorVimInsertReplayStep::ShiftTab),
        Key::Tab => Some(EditorVimInsertReplayStep::Tab),
        _ => None,
    }) else {
        return;
    };
    change.insert_replay.push(step);
}

fn vim_record_insert_replay_step(
    last_change: &mut Option<EditorVimLastChange>,
    step: EditorVimInsertReplayStep,
) {
    let Some(change) = last_change.as_mut() else {
        return;
    };
    if change.action.accepts_inserted_text() {
        change.insert_replay.push(step);
    }
}

pub(crate) fn vim_record_inserted_text_for_buffer(
    last_changes: &mut HashMap<BufferId, EditorVimLastChange>,
    buffer_id: BufferId,
    text: &str,
) {
    let mut last_change = last_changes.remove(&buffer_id);
    vim_record_inserted_text(&mut last_change, text);
    if let Some(last_change) = last_change {
        last_changes.insert(buffer_id, last_change);
    }
}

pub(crate) fn vim_record_insert_replay_key_with_auto_indent_for_buffer(
    last_changes: &mut HashMap<BufferId, EditorVimLastChange>,
    buffer_id: BufferId,
    key: Key,
    modifiers: Modifiers,
    auto_indent: bool,
) {
    let mut last_change = last_changes.remove(&buffer_id);
    vim_record_insert_replay_key_with_auto_indent(&mut last_change, key, modifiers, auto_indent);
    if let Some(last_change) = last_change {
        last_changes.insert(buffer_id, last_change);
    }
}
