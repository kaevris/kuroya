use kuroya_core::TextBuffer;

use super::super::super::motion::{
    vim_delete_backward_chars, vim_delete_backward_chars_into_named_register,
    vim_delete_forward_chars, vim_delete_forward_chars_into_named_register,
};
use super::super::super::operator::{
    vim_apply_operator_motion, vim_apply_operator_motion_into_named_register,
    vim_apply_text_object, vim_apply_text_object_into_named_register, vim_delete_to_line_end,
    vim_delete_to_line_end_into_named_register,
};
use super::super::super::{EditorVimRegister, EditorVimRepeatAction};
use super::super::linewise_registers::{
    vim_change_lines_into_named_register, vim_change_lines_into_register,
    vim_delete_lines_into_named_register, vim_delete_lines_into_register,
};
use super::action::ApplyLastActionOutcome;

pub(super) fn vim_apply_change_delete_repeat_action(
    buffer: &mut TextBuffer,
    action: EditorVimRepeatAction,
    count: usize,
    unnamed_register: &mut Option<EditorVimRegister>,
) -> ApplyLastActionOutcome {
    match action {
        EditorVimRepeatAction::ChangeLines => ApplyLastActionOutcome::insert(
            vim_change_lines_into_register(buffer, count, unnamed_register),
        ),
        EditorVimRepeatAction::ChangeLinesIntoRegister(register) => ApplyLastActionOutcome::insert(
            vim_change_lines_into_named_register(buffer, count, unnamed_register, register),
        ),
        EditorVimRepeatAction::ChangeOperatorMotion(motion) => ApplyLastActionOutcome::insert(
            vim_apply_operator_motion(buffer, 1, count, motion, unnamed_register),
        ),
        EditorVimRepeatAction::ChangeOperatorMotionIntoRegister { motion, register } => {
            ApplyLastActionOutcome::insert(vim_apply_operator_motion_into_named_register(
                buffer,
                1,
                count,
                motion,
                unnamed_register,
                register,
            ))
        }
        EditorVimRepeatAction::ChangeTextObject { scope, kind } => ApplyLastActionOutcome::insert(
            vim_apply_text_object(buffer, 1, count, scope, kind, unnamed_register),
        ),
        EditorVimRepeatAction::ChangeTextObjectIntoRegister {
            scope,
            kind,
            register,
        } => ApplyLastActionOutcome::insert(vim_apply_text_object_into_named_register(
            buffer,
            1,
            count,
            scope,
            kind,
            unnamed_register,
            register,
        )),
        EditorVimRepeatAction::ChangeToLineEnd => {
            ApplyLastActionOutcome::insert(vim_delete_to_line_end(buffer, count, unnamed_register))
        }
        EditorVimRepeatAction::ChangeToLineEndIntoRegister(register) => {
            ApplyLastActionOutcome::insert(vim_delete_to_line_end_into_named_register(
                buffer,
                count,
                unnamed_register,
                register,
            ))
        }
        EditorVimRepeatAction::DeleteBackwardChars => ApplyLastActionOutcome::normal(
            vim_delete_backward_chars(buffer, count, unnamed_register),
        ),
        EditorVimRepeatAction::DeleteBackwardCharsIntoRegister(register) => {
            ApplyLastActionOutcome::normal(vim_delete_backward_chars_into_named_register(
                buffer,
                count,
                unnamed_register,
                register,
            ))
        }
        EditorVimRepeatAction::DeleteForwardChars => ApplyLastActionOutcome::normal(
            vim_delete_forward_chars(buffer, count, unnamed_register),
        ),
        EditorVimRepeatAction::DeleteForwardCharsIntoRegister(register) => {
            ApplyLastActionOutcome::normal(vim_delete_forward_chars_into_named_register(
                buffer,
                count,
                unnamed_register,
                register,
            ))
        }
        EditorVimRepeatAction::DeleteLines => ApplyLastActionOutcome::normal(
            vim_delete_lines_into_register(buffer, count, unnamed_register),
        ),
        EditorVimRepeatAction::DeleteLinesIntoRegister(register) => ApplyLastActionOutcome::normal(
            vim_delete_lines_into_named_register(buffer, count, unnamed_register, register),
        ),
        EditorVimRepeatAction::DeleteOperatorMotion(motion) => ApplyLastActionOutcome::normal(
            vim_apply_operator_motion(buffer, 1, count, motion, unnamed_register),
        ),
        EditorVimRepeatAction::DeleteOperatorMotionIntoRegister { motion, register } => {
            ApplyLastActionOutcome::normal(vim_apply_operator_motion_into_named_register(
                buffer,
                1,
                count,
                motion,
                unnamed_register,
                register,
            ))
        }
        EditorVimRepeatAction::DeleteTextObject { scope, kind } => ApplyLastActionOutcome::normal(
            vim_apply_text_object(buffer, 1, count, scope, kind, unnamed_register),
        ),
        EditorVimRepeatAction::DeleteTextObjectIntoRegister {
            scope,
            kind,
            register,
        } => ApplyLastActionOutcome::normal(vim_apply_text_object_into_named_register(
            buffer,
            1,
            count,
            scope,
            kind,
            unnamed_register,
            register,
        )),
        EditorVimRepeatAction::DeleteToLineEnd => {
            ApplyLastActionOutcome::normal(vim_delete_to_line_end(buffer, count, unnamed_register))
        }
        EditorVimRepeatAction::DeleteToLineEndIntoRegister(register) => {
            ApplyLastActionOutcome::normal(vim_delete_to_line_end_into_named_register(
                buffer,
                count,
                unnamed_register,
                register,
            ))
        }
        EditorVimRepeatAction::SubstituteForwardChars => ApplyLastActionOutcome::insert(
            vim_delete_forward_chars(buffer, count, unnamed_register),
        ),
        EditorVimRepeatAction::SubstituteForwardCharsIntoRegister(register) => {
            ApplyLastActionOutcome::insert(vim_delete_forward_chars_into_named_register(
                buffer,
                count,
                unnamed_register,
                register,
            ))
        }
        _ => unreachable!("non-change/delete repeat action routed to change/delete handler"),
    }
}
