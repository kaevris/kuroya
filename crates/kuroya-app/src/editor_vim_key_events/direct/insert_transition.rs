use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

use super::super::motion::{vim_open_line_above, vim_open_line_below};
use super::super::{
    EditorVimLastChange, EditorVimMode, EditorVimPendingKey, EditorVimRepeatAction, VimKeyResult,
    vim_collapse_selection_for_insert, vim_record_insert_change, vim_repeatable_change_result,
};

pub(super) fn handle_vim_direct_insert_transition_key(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    last_change: &mut Option<EditorVimLastChange>,
    count_value: usize,
    suppress_text: Option<char>,
) -> Option<VimKeyResult> {
    match key {
        Key::I => {
            vim_collapse_selection_for_insert(buffer);
            if modifiers.shift {
                buffer.move_line_first_non_whitespace();
                vim_record_insert_change(
                    last_change,
                    EditorVimRepeatAction::InsertLineFirstNonWhitespace,
                    count_value,
                );
            } else {
                vim_record_insert_change(
                    last_change,
                    EditorVimRepeatAction::InsertAtCursor,
                    count_value,
                );
            }
            *pending = None;
            *mode = EditorVimMode::Insert;
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::A => {
            vim_collapse_selection_for_insert(buffer);
            if modifiers.shift {
                buffer.move_line_end();
                vim_record_insert_change(
                    last_change,
                    EditorVimRepeatAction::InsertLineEnd,
                    count_value,
                );
            } else {
                buffer.move_right();
                vim_record_insert_change(
                    last_change,
                    EditorVimRepeatAction::AppendAfterCursor,
                    count_value,
                );
            }
            *pending = None;
            *mode = EditorVimMode::Insert;
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::O => {
            vim_collapse_selection_for_insert(buffer);
            if modifiers.shift {
                vim_open_line_above(buffer);
            } else {
                vim_open_line_below(buffer);
            }
            *pending = None;
            *mode = EditorVimMode::Insert;
            Some(vim_repeatable_change_result(
                true,
                last_change,
                if modifiers.shift {
                    EditorVimRepeatAction::OpenLineAbove
                } else {
                    EditorVimRepeatAction::OpenLineBelow
                },
                count_value,
                suppress_text,
            ))
        }
        _ => None,
    }
}
