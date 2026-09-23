use kuroya_core::TextBuffer;

use super::super::super::{EditorVimRegister, EditorVimRepeatAction};
use super::{
    case::vim_apply_case_repeat_action, change_delete::vim_apply_change_delete_repeat_action,
    edits::vim_apply_edit_repeat_action, register_put::vim_apply_register_put_repeat_action,
};

pub(super) struct ApplyLastActionOutcome {
    pub(super) changed: bool,
    pub(super) enters_insert: bool,
}

impl ApplyLastActionOutcome {
    pub(super) fn normal(changed: bool) -> Self {
        Self {
            changed,
            enters_insert: false,
        }
    }

    pub(super) fn insert(changed: bool) -> Self {
        Self {
            changed,
            enters_insert: true,
        }
    }
}

pub(super) fn vim_apply_repeat_action(
    buffer: &mut TextBuffer,
    action: EditorVimRepeatAction,
    count: usize,
    unnamed_register: &mut Option<EditorVimRegister>,
    indent_unit: &str,
) -> ApplyLastActionOutcome {
    match action {
        EditorVimRepeatAction::ChangeLines
        | EditorVimRepeatAction::ChangeLinesIntoRegister(_)
        | EditorVimRepeatAction::ChangeOperatorMotion(_)
        | EditorVimRepeatAction::ChangeOperatorMotionIntoRegister { .. }
        | EditorVimRepeatAction::ChangeTextObject { .. }
        | EditorVimRepeatAction::ChangeTextObjectIntoRegister { .. }
        | EditorVimRepeatAction::ChangeToLineEnd
        | EditorVimRepeatAction::ChangeToLineEndIntoRegister(_)
        | EditorVimRepeatAction::DeleteBackwardChars
        | EditorVimRepeatAction::DeleteBackwardCharsIntoRegister(_)
        | EditorVimRepeatAction::DeleteForwardChars
        | EditorVimRepeatAction::DeleteForwardCharsIntoRegister(_)
        | EditorVimRepeatAction::DeleteLines
        | EditorVimRepeatAction::DeleteLinesIntoRegister(_)
        | EditorVimRepeatAction::DeleteOperatorMotion(_)
        | EditorVimRepeatAction::DeleteOperatorMotionIntoRegister { .. }
        | EditorVimRepeatAction::DeleteTextObject { .. }
        | EditorVimRepeatAction::DeleteTextObjectIntoRegister { .. }
        | EditorVimRepeatAction::DeleteToLineEnd
        | EditorVimRepeatAction::DeleteToLineEndIntoRegister(_)
        | EditorVimRepeatAction::SubstituteForwardChars
        | EditorVimRepeatAction::SubstituteForwardCharsIntoRegister(_) => {
            vim_apply_change_delete_repeat_action(buffer, action, count, unnamed_register)
        }
        EditorVimRepeatAction::AppendAfterCursor
        | EditorVimRepeatAction::IndentLines
        | EditorVimRepeatAction::InsertAtCursor
        | EditorVimRepeatAction::InsertLineEnd
        | EditorVimRepeatAction::InsertLineFirstNonWhitespace
        | EditorVimRepeatAction::JoinLines
        | EditorVimRepeatAction::JoinLinesWithoutWhitespace
        | EditorVimRepeatAction::OpenLineAbove
        | EditorVimRepeatAction::OpenLineBelow
        | EditorVimRepeatAction::OutdentLines
        | EditorVimRepeatAction::ReplaceForwardChars(_)
        | EditorVimRepeatAction::Substitute { .. } => {
            vim_apply_edit_repeat_action(buffer, action, count, indent_unit)
        }
        EditorVimRepeatAction::PutAfter
        | EditorVimRepeatAction::PutAfterNamed(_)
        | EditorVimRepeatAction::PutBefore
        | EditorVimRepeatAction::PutBeforeNamed(_) => {
            vim_apply_register_put_repeat_action(buffer, action, count, unnamed_register)
        }
        EditorVimRepeatAction::ConvertCaseForwardChars(_)
        | EditorVimRepeatAction::ConvertCaseLines(_)
        | EditorVimRepeatAction::ConvertCaseOperatorMotion { .. }
        | EditorVimRepeatAction::ConvertCaseTextObject { .. }
        | EditorVimRepeatAction::ToggleCaseForwardChars
        | EditorVimRepeatAction::ToggleCaseOperatorMotion(_)
        | EditorVimRepeatAction::ToggleCaseTextObject { .. } => {
            vim_apply_case_repeat_action(buffer, action, count)
        }
    }
}
