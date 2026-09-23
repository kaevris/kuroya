use eframe::egui::{Key, Modifiers};

use super::super::super::{
    EditorVimPendingKey, no_text_modifiers, vim_case_conversion_repeated_operator_key,
    vim_command_input_control_edit, vim_jump_mark_name_for_key, vim_mark_name_for_key,
    vim_named_register_for_key, vim_replacement_key_char, vim_search_input_control_edit,
};
use super::super::{
    vim_operator_go_motion_for_key, vim_operator_motion_for_key, vim_text_object_kind_for_key,
};

pub(in crate::editor_vim_key_events) fn vim_pending_key_accepts(
    pending: Option<EditorVimPendingKey>,
    key: Key,
    modifiers: Modifiers,
    printable_key_char: Option<char>,
) -> bool {
    if matches!(pending, Some(EditorVimPendingKey::ReplaceChar(_))) {
        return vim_replacement_key_char(key, modifiers).is_some();
    }
    if matches!(pending, Some(EditorVimPendingKey::SearchInput { .. })) {
        return (key == Key::Enter || key == Key::Backspace) && no_text_modifiers(modifiers)
            || vim_search_input_control_edit(key, modifiers).is_some()
            || printable_key_char.is_some();
    }
    if matches!(pending, Some(EditorVimPendingKey::CommandInput)) {
        return (key == Key::Enter || key == Key::Backspace) && no_text_modifiers(modifiers)
            || vim_command_input_control_edit(key, modifiers).is_some()
            || printable_key_char.is_some();
    }
    if matches!(pending, Some(EditorVimPendingKey::RegisterPrefix(_))) {
        return vim_named_register_for_key(key, modifiers).is_some();
    }
    if matches!(pending, Some(EditorVimPendingKey::RegisterCommand { .. })) {
        return matches!(
            (key, modifiers.shift),
            (Key::C, true)
                | (Key::D, true)
                | (Key::P, false)
                | (Key::P, true)
                | (Key::S, false)
                | (Key::S, true)
                | (Key::X, false)
                | (Key::X, true)
                | (Key::Y, true)
        );
    }
    if matches!(
        pending,
        Some(
            EditorVimPendingKey::FindCharForward(_)
                | EditorVimPendingKey::FindCharBackward(_)
                | EditorVimPendingKey::TillCharForward(_)
                | EditorVimPendingKey::TillCharBackward(_)
        )
    ) {
        return printable_key_char.is_some();
    }
    if matches!(
        pending,
        Some(
            EditorVimPendingKey::ChangeCharFind { .. }
                | EditorVimPendingKey::ChangeCharFindIntoRegister { .. }
                | EditorVimPendingKey::DeleteCharFind { .. }
                | EditorVimPendingKey::ToggleCaseCharFind { .. }
                | EditorVimPendingKey::ConvertCaseCharFind { .. }
                | EditorVimPendingKey::YankCharFind { .. }
                | EditorVimPendingKey::DeleteCharFindIntoRegister { .. }
                | EditorVimPendingKey::YankCharFindIntoRegister { .. }
        )
    ) {
        return printable_key_char.is_some();
    }
    if matches!(pending, Some(EditorVimPendingKey::OperatorGoMotion { .. })) {
        return vim_operator_go_motion_for_key(key, modifiers).is_some();
    }
    if matches!(pending, Some(EditorVimPendingKey::SetMark)) {
        return vim_mark_name_for_key(key, modifiers).is_some();
    }
    if matches!(pending, Some(EditorVimPendingKey::JumpMark { .. })) {
        return vim_jump_mark_name_for_key(key, modifiers).is_some();
    }
    if let Some(
        EditorVimPendingKey::ConvertCaseOperator { conversion, .. }
        | EditorVimPendingKey::ConvertCaseMotionCount { conversion, .. },
    ) = pending
        && vim_case_conversion_repeated_operator_key(conversion, key, modifiers)
    {
        return true;
    }
    matches!(
        (pending, key, modifiers.shift),
        (Some(EditorVimPendingKey::ChangeLine(_)), Key::C, false)
            | (
                Some(EditorVimPendingKey::ChangeLineIntoRegister { .. }),
                Key::C,
                false
            )
            | (Some(EditorVimPendingKey::DeleteLine(_)), Key::D, false)
            | (
                Some(EditorVimPendingKey::DeleteLineIntoRegister { .. }),
                Key::D,
                false
            )
            | (Some(EditorVimPendingKey::Go(_)), Key::Num3, true)
            | (Some(EditorVimPendingKey::Go(_)), Key::Num8, true)
            | (Some(EditorVimPendingKey::Go(_)), Key::G, false)
            | (Some(EditorVimPendingKey::Go(_)), Key::E, false)
            | (Some(EditorVimPendingKey::Go(_)), Key::E, true)
            | (Some(EditorVimPendingKey::Go(_)), Key::J, true)
            | (Some(EditorVimPendingKey::Go(_)), Key::U, false)
            | (Some(EditorVimPendingKey::Go(_)), Key::U, true)
            | (Some(EditorVimPendingKey::IndentLine(_)), Key::Period, true)
            | (Some(EditorVimPendingKey::OutdentLine(_)), Key::Comma, true)
            | (Some(EditorVimPendingKey::YankLine(_)), Key::Y, false)
            | (
                Some(EditorVimPendingKey::YankLineIntoRegister { .. }),
                Key::Y,
                false
            )
    ) || matches!(
        pending,
        Some(
            EditorVimPendingKey::ChangeLine(_)
                | EditorVimPendingKey::ChangeLineIntoRegister { .. }
                | EditorVimPendingKey::ChangeMotionCount { .. }
                | EditorVimPendingKey::ChangeMotionCountIntoRegister { .. }
                | EditorVimPendingKey::DeleteLine(_)
                | EditorVimPendingKey::DeleteMotionCount { .. }
                | EditorVimPendingKey::DeleteLineIntoRegister { .. }
                | EditorVimPendingKey::DeleteMotionCountIntoRegister { .. }
                | EditorVimPendingKey::YankLine(_)
                | EditorVimPendingKey::YankMotionCount { .. }
                | EditorVimPendingKey::YankLineIntoRegister { .. }
                | EditorVimPendingKey::YankMotionCountIntoRegister { .. }
                | EditorVimPendingKey::ConvertCaseOperator { .. }
                | EditorVimPendingKey::ConvertCaseMotionCount { .. }
                | EditorVimPendingKey::ToggleCaseOperator(_)
                | EditorVimPendingKey::ToggleCaseMotionCount { .. }
        )
    ) && vim_operator_motion_for_key(key, modifiers).is_some()
        || matches!(
            pending,
            Some(
                EditorVimPendingKey::ChangeTextObject { .. }
                    | EditorVimPendingKey::ChangeTextObjectIntoRegister { .. }
                    | EditorVimPendingKey::DeleteTextObject { .. }
                    | EditorVimPendingKey::YankTextObject { .. }
                    | EditorVimPendingKey::DeleteTextObjectIntoRegister { .. }
                    | EditorVimPendingKey::YankTextObjectIntoRegister { .. }
                    | EditorVimPendingKey::ConvertCaseTextObject { .. }
                    | EditorVimPendingKey::ToggleCaseTextObject { .. }
            )
        ) && vim_text_object_kind_for_key(key, modifiers).is_some()
}
