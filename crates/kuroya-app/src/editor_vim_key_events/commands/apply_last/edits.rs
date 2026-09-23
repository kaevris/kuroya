use kuroya_core::TextBuffer;

use super::super::super::EditorVimRepeatAction;
use super::super::super::motion::{
    vim_indent_lines, vim_join_lines, vim_join_lines_without_whitespace, vim_open_line_above,
    vim_open_line_below, vim_outdent_lines, vim_replace_forward_chars,
};
use super::action::ApplyLastActionOutcome;

pub(super) fn vim_apply_edit_repeat_action(
    buffer: &mut TextBuffer,
    action: EditorVimRepeatAction,
    count: usize,
    indent_unit: &str,
) -> ApplyLastActionOutcome {
    match action {
        EditorVimRepeatAction::AppendAfterCursor => {
            buffer.move_right();
            ApplyLastActionOutcome::normal(false)
        }
        EditorVimRepeatAction::Substitute {
            query,
            replacement,
            global,
        } => ApplyLastActionOutcome::normal(
            super::super::super::command_input::vim_apply_substitute_repeat(
                buffer,
                &query,
                &replacement,
                global,
            ),
        ),
        EditorVimRepeatAction::IndentLines => {
            ApplyLastActionOutcome::normal(vim_indent_lines(buffer, count, indent_unit))
        }
        EditorVimRepeatAction::InsertAtCursor => ApplyLastActionOutcome::normal(false),
        EditorVimRepeatAction::InsertLineEnd => {
            buffer.move_line_end();
            ApplyLastActionOutcome::normal(false)
        }
        EditorVimRepeatAction::InsertLineFirstNonWhitespace => {
            buffer.move_line_first_non_whitespace();
            ApplyLastActionOutcome::normal(false)
        }
        EditorVimRepeatAction::JoinLines => {
            ApplyLastActionOutcome::normal(vim_join_lines(buffer, count))
        }
        EditorVimRepeatAction::JoinLinesWithoutWhitespace => {
            ApplyLastActionOutcome::normal(vim_join_lines_without_whitespace(buffer, count))
        }
        EditorVimRepeatAction::OpenLineAbove => {
            for _ in 0..count {
                vim_open_line_above(buffer);
            }
            ApplyLastActionOutcome::insert(true)
        }
        EditorVimRepeatAction::OpenLineBelow => {
            for _ in 0..count {
                vim_open_line_below(buffer);
            }
            ApplyLastActionOutcome::insert(true)
        }
        EditorVimRepeatAction::OutdentLines => {
            ApplyLastActionOutcome::normal(vim_outdent_lines(buffer, count, indent_unit))
        }
        EditorVimRepeatAction::ReplaceForwardChars(replacement) => {
            ApplyLastActionOutcome::normal(vim_replace_forward_chars(buffer, count, replacement))
        }
        _ => unreachable!("non-edit repeat action routed to edit handler"),
    }
}
