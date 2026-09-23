mod action;
mod case;
mod change_delete;
mod edits;
mod register_put;

use kuroya_core::TextBuffer;

use self::action::vim_apply_repeat_action;
use super::super::{EditorVimLastChange, EditorVimMode, EditorVimRegister, VIM_MAX_COUNT};
use super::insert_replay::vim_replay_insert_steps;

pub(in crate::editor_vim_key_events) fn vim_apply_last_change(
    buffer: &mut TextBuffer,
    change: EditorVimLastChange,
    repeat_count: Option<usize>,
    mode: &mut EditorVimMode,
    unnamed_register: &mut Option<EditorVimRegister>,
    indent_unit: &str,
) -> bool {
    let count = repeat_count.unwrap_or(change.count).clamp(1, VIM_MAX_COUNT);
    let action = change.action;
    let outcome =
        vim_apply_repeat_action(buffer, action.clone(), count, unnamed_register, indent_unit);

    if action.accepts_inserted_text() && !change.insert_replay.is_empty() {
        let insert_count = if action.is_plain_insert() { count } else { 1 };
        let inserted =
            vim_replay_insert_steps(buffer, &change.insert_replay, insert_count, indent_unit);
        *mode = EditorVimMode::Normal;
        return outcome.changed || inserted;
    }

    if outcome.enters_insert || action.is_plain_insert() {
        *mode = EditorVimMode::Insert;
    }
    outcome.changed
}
