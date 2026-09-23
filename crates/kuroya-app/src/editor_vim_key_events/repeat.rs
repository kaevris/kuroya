use kuroya_core::TextBuffer;

use super::{
    EditorVimLastChange, EditorVimMode, EditorVimOperatorGoKind, EditorVimOperatorMotion,
    EditorVimRegister, EditorVimRepeatAction, VIM_MAX_COUNT, VimKeyResult,
    vim_apply_change_operator_motion, vim_apply_change_operator_motion_into_named_register,
    vim_apply_operator_motion, vim_apply_operator_motion_into_named_register, vim_combined_count,
    vim_convert_case_operator_motion, vim_toggle_case_operator_motion, vim_yank_operator_motion,
    vim_yank_operator_motion_into_named_register,
};

pub(super) fn vim_repeatable_change_result(
    changed: bool,
    last_change: &mut Option<EditorVimLastChange>,
    action: EditorVimRepeatAction,
    count: usize,
    suppress_text: Option<char>,
) -> VimKeyResult {
    if changed {
        *last_change = Some(EditorVimLastChange {
            action,
            count: count.clamp(1, VIM_MAX_COUNT),
            insert_replay: Vec::new(),
        });
        VimKeyResult::changed(suppress_text)
    } else {
        VimKeyResult::handled(suppress_text)
    }
}

pub(super) fn handle_vim_operator_go_motion_key_event(
    buffer: &mut TextBuffer,
    mode: &mut EditorVimMode,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    operator_count: usize,
    motion_count: usize,
    operator: EditorVimOperatorGoKind,
    motion: EditorVimOperatorMotion,
    suppress_text: Option<char>,
) -> VimKeyResult {
    let count = vim_combined_count(operator_count, motion_count);
    match operator {
        EditorVimOperatorGoKind::Change => {
            let changed = vim_apply_change_operator_motion(
                buffer,
                operator_count,
                motion_count,
                motion,
                unnamed_register,
            );
            *mode = EditorVimMode::Insert;
            vim_repeatable_change_result(
                changed,
                last_change,
                EditorVimRepeatAction::ChangeOperatorMotion(motion),
                count,
                suppress_text,
            )
        }
        EditorVimOperatorGoKind::ChangeIntoRegister(register) => {
            let changed = vim_apply_change_operator_motion_into_named_register(
                buffer,
                operator_count,
                motion_count,
                motion,
                unnamed_register,
                register,
            );
            *mode = EditorVimMode::Insert;
            vim_repeatable_change_result(
                changed,
                last_change,
                EditorVimRepeatAction::ChangeOperatorMotionIntoRegister { motion, register },
                count,
                suppress_text,
            )
        }
        EditorVimOperatorGoKind::ConvertCase(conversion) => vim_repeatable_change_result(
            vim_convert_case_operator_motion(
                buffer,
                operator_count,
                motion_count,
                motion,
                conversion,
            ),
            last_change,
            EditorVimRepeatAction::ConvertCaseOperatorMotion { motion, conversion },
            count,
            suppress_text,
        ),
        EditorVimOperatorGoKind::Delete => vim_repeatable_change_result(
            vim_apply_operator_motion(
                buffer,
                operator_count,
                motion_count,
                motion,
                unnamed_register,
            ),
            last_change,
            EditorVimRepeatAction::DeleteOperatorMotion(motion),
            count,
            suppress_text,
        ),
        EditorVimOperatorGoKind::DeleteIntoRegister(register) => vim_repeatable_change_result(
            vim_apply_operator_motion_into_named_register(
                buffer,
                operator_count,
                motion_count,
                motion,
                unnamed_register,
                register,
            ),
            last_change,
            EditorVimRepeatAction::DeleteOperatorMotionIntoRegister { motion, register },
            count,
            suppress_text,
        ),
        EditorVimOperatorGoKind::ToggleCase => vim_repeatable_change_result(
            vim_toggle_case_operator_motion(buffer, operator_count, motion_count, motion),
            last_change,
            EditorVimRepeatAction::ToggleCaseOperatorMotion(motion),
            count,
            suppress_text,
        ),
        EditorVimOperatorGoKind::Yank => {
            vim_yank_operator_motion(
                buffer,
                operator_count,
                motion_count,
                motion,
                unnamed_register,
            );
            VimKeyResult::handled(suppress_text)
        }
        EditorVimOperatorGoKind::YankIntoRegister(register) => {
            vim_yank_operator_motion_into_named_register(
                buffer,
                operator_count,
                motion_count,
                motion,
                unnamed_register,
                register,
            );
            VimKeyResult::handled(suppress_text)
        }
    }
}

pub(super) fn vim_record_insert_change(
    last_change: &mut Option<EditorVimLastChange>,
    action: EditorVimRepeatAction,
    count: usize,
) {
    *last_change = Some(EditorVimLastChange {
        action,
        count: count.clamp(1, VIM_MAX_COUNT),
        insert_replay: Vec::new(),
    });
}

impl EditorVimRepeatAction {
    pub(super) fn accepts_inserted_text(&self) -> bool {
        matches!(
            self,
            Self::ChangeLines
                | Self::ChangeLinesIntoRegister(_)
                | Self::ChangeOperatorMotion(_)
                | Self::ChangeOperatorMotionIntoRegister { .. }
                | Self::ChangeTextObject { .. }
                | Self::ChangeTextObjectIntoRegister { .. }
                | Self::ChangeToLineEnd
                | Self::ChangeToLineEndIntoRegister(_)
                | Self::AppendAfterCursor
                | Self::InsertAtCursor
                | Self::InsertLineEnd
                | Self::InsertLineFirstNonWhitespace
                | Self::OpenLineAbove
                | Self::OpenLineBelow
                | Self::SubstituteForwardChars
                | Self::SubstituteForwardCharsIntoRegister(_)
        )
    }

    pub(super) fn is_plain_insert(&self) -> bool {
        matches!(
            self,
            Self::AppendAfterCursor
                | Self::InsertAtCursor
                | Self::InsertLineEnd
                | Self::InsertLineFirstNonWhitespace
        )
    }
}
