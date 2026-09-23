use kuroya_core::TextBuffer;

use super::super::motion::{vim_delete_line_backward, vim_open_line_above, vim_open_line_below};
use super::super::{
    EditorVimInsertReplayStep, EditorVimLastChange, EditorVimRepeatAction, VIM_MAX_COUNT,
};

pub(in crate::editor_vim_key_events) fn vim_replay_counted_insert_session(
    buffer: &mut TextBuffer,
    last_change: &Option<EditorVimLastChange>,
    indent_unit: &str,
) {
    let Some(change) = last_change else {
        return;
    };
    if change.count <= 1 || change.insert_replay.is_empty() {
        return;
    }
    let extra = change.count - 1;
    if change.action.is_plain_insert() {
        vim_replay_insert_steps(buffer, &change.insert_replay, extra, indent_unit);
        return;
    }
    match change.action {
        EditorVimRepeatAction::OpenLineAbove => {
            for _ in 0..extra {
                vim_open_line_above(buffer);
                vim_replay_insert_steps(buffer, &change.insert_replay, 1, indent_unit);
            }
        }
        EditorVimRepeatAction::OpenLineBelow => {
            for _ in 0..extra {
                vim_open_line_below(buffer);
                vim_replay_insert_steps(buffer, &change.insert_replay, 1, indent_unit);
            }
        }
        _ => {}
    }
}

pub(super) fn vim_replay_insert_steps(
    buffer: &mut TextBuffer,
    steps: &[EditorVimInsertReplayStep],
    count: usize,
    indent_unit: &str,
) -> bool {
    if steps.is_empty() {
        return false;
    }
    let mut changed = false;
    for _ in 0..count.clamp(1, VIM_MAX_COUNT) {
        for step in steps {
            changed |= vim_replay_insert_step(buffer, step, indent_unit);
        }
    }
    changed
}

fn vim_replay_insert_step(
    buffer: &mut TextBuffer,
    step: &EditorVimInsertReplayStep,
    indent_unit: &str,
) -> bool {
    match step {
        EditorVimInsertReplayStep::Backspace => buffer.delete_backward_with_auto_pair_delete(false),
        EditorVimInsertReplayStep::DeleteLineBackward => vim_delete_line_backward(buffer),
        EditorVimInsertReplayStep::DeleteWordBackward => buffer.delete_word_backward(),
        EditorVimInsertReplayStep::Enter => {
            buffer.insert_at_cursors("\n");
            true
        }
        EditorVimInsertReplayStep::EnterAutoIndent => {
            buffer.insert_newline_with_indent_unit(indent_unit);
            true
        }
        EditorVimInsertReplayStep::InsertText(text) => {
            buffer.insert_at_cursors(text);
            !text.is_empty()
        }
        EditorVimInsertReplayStep::Tab => {
            if indent_unit.is_empty() {
                return false;
            }
            buffer.insert_at_cursors(indent_unit);
            true
        }
        EditorVimInsertReplayStep::ShiftTab => buffer.outdent_lines(indent_unit),
    }
}
