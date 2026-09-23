use eframe::egui::{Key, Modifiers};

use super::super::{
    EditorVimCharFind, EditorVimCharFindMotion, EditorVimOperatorGoKind, EditorVimOperatorMotion,
    EditorVimPendingKey, no_text_modifiers, vim_line_column_motion_key,
};

pub(in crate::editor_vim_key_events) fn vim_operator_motion_for_key(
    key: Key,
    modifiers: Modifiers,
) -> Option<EditorVimOperatorMotion> {
    if modifiers.command || modifiers.alt || modifiers.ctrl {
        return None;
    }
    if vim_line_column_motion_key(key, modifiers) {
        return Some(EditorVimOperatorMotion::LineColumn);
    }
    if key == Key::Home && no_text_modifiers(modifiers) {
        return Some(EditorVimOperatorMotion::LineColumnStart);
    }
    if key == Key::End && no_text_modifiers(modifiers) {
        return Some(EditorVimOperatorMotion::LineEnd);
    }
    match (key, modifiers.shift) {
        (Key::B, false) => Some(EditorVimOperatorMotion::WordBackward),
        (Key::B, true) => Some(EditorVimOperatorMotion::BigWordBackward),
        (Key::E, false) => Some(EditorVimOperatorMotion::WordEnd),
        (Key::E, true) => Some(EditorVimOperatorMotion::BigWordEnd),
        (Key::Backspace, false) => Some(EditorVimOperatorMotion::CharacterBackward),
        (Key::G, true) => Some(EditorVimOperatorMotion::LastLine),
        (Key::H, false) => Some(EditorVimOperatorMotion::CharacterBackward),
        (Key::J, false) => Some(EditorVimOperatorMotion::LineDown),
        (Key::K, false) => Some(EditorVimOperatorMotion::LineUp),
        (Key::L, false) => Some(EditorVimOperatorMotion::CharacterForward),
        (Key::Space, false) => Some(EditorVimOperatorMotion::CharacterForward),
        (Key::W, false) => Some(EditorVimOperatorMotion::WordForward),
        (Key::W, true) => Some(EditorVimOperatorMotion::BigWordForward),
        (Key::Num0, false) => Some(EditorVimOperatorMotion::LineColumnStart),
        (Key::Num4, true) => Some(EditorVimOperatorMotion::LineEnd),
        (Key::Num5, true) => Some(EditorVimOperatorMotion::MatchingBracket),
        (Key::Num6, true) => Some(EditorVimOperatorMotion::LineFirstNonWhitespace),
        (Key::CloseBracket, true) => Some(EditorVimOperatorMotion::ParagraphForward),
        (Key::OpenBracket, true) => Some(EditorVimOperatorMotion::ParagraphBackward),
        (Key::Num3, true) => Some(EditorVimOperatorMotion::SearchWordUnderCursor {
            forward: false,
            whole_word: true,
        }),
        (Key::Num8, true) => Some(EditorVimOperatorMotion::SearchWordUnderCursor {
            forward: true,
            whole_word: true,
        }),
        (Key::N, false) => Some(EditorVimOperatorMotion::SearchRepeat { reverse: false }),
        (Key::N, true) => Some(EditorVimOperatorMotion::SearchRepeat { reverse: true }),
        _ => None,
    }
}

pub(in crate::editor_vim_key_events) fn vim_operator_last_find_motion_for_key(
    key: Key,
    modifiers: Modifiers,
    last_char_find: Option<EditorVimCharFind>,
) -> Option<EditorVimOperatorMotion> {
    if modifiers.command || modifiers.alt || modifiers.ctrl || modifiers.shift {
        return None;
    }
    let last = last_char_find?;
    match key {
        Key::Semicolon => Some(EditorVimOperatorMotion::CharFind {
            motion: last.motion,
            target: last.target,
        }),
        Key::Comma => Some(EditorVimOperatorMotion::CharFind {
            motion: last.motion.reversed(),
            target: last.target,
        }),
        _ => None,
    }
}

pub(in crate::editor_vim_key_events) fn vim_pending_key_last_find_operator_go(
    pending_key: &EditorVimPendingKey,
) -> Option<(usize, usize, EditorVimOperatorGoKind)> {
    Some(match pending_key {
        EditorVimPendingKey::ChangeLine(operator_count) => {
            (*operator_count, 1, EditorVimOperatorGoKind::Change)
        }
        EditorVimPendingKey::ChangeLineIntoRegister {
            operator_count,
            register,
        } => (
            *operator_count,
            1,
            EditorVimOperatorGoKind::ChangeIntoRegister(*register),
        ),
        EditorVimPendingKey::ChangeMotionCount {
            operator_count,
            motion_count,
        } => (
            *operator_count,
            *motion_count,
            EditorVimOperatorGoKind::Change,
        ),
        EditorVimPendingKey::ChangeMotionCountIntoRegister {
            operator_count,
            motion_count,
            register,
        } => (
            *operator_count,
            *motion_count,
            EditorVimOperatorGoKind::ChangeIntoRegister(*register),
        ),
        EditorVimPendingKey::DeleteLine(operator_count) => {
            (*operator_count, 1, EditorVimOperatorGoKind::Delete)
        }
        EditorVimPendingKey::DeleteLineIntoRegister {
            operator_count,
            register,
        } => (
            *operator_count,
            1,
            EditorVimOperatorGoKind::DeleteIntoRegister(*register),
        ),
        EditorVimPendingKey::DeleteMotionCount {
            operator_count,
            motion_count,
        } => (
            *operator_count,
            *motion_count,
            EditorVimOperatorGoKind::Delete,
        ),
        EditorVimPendingKey::DeleteMotionCountIntoRegister {
            operator_count,
            motion_count,
            register,
        } => (
            *operator_count,
            *motion_count,
            EditorVimOperatorGoKind::DeleteIntoRegister(*register),
        ),
        EditorVimPendingKey::YankLine(operator_count) => {
            (*operator_count, 1, EditorVimOperatorGoKind::Yank)
        }
        EditorVimPendingKey::YankLineIntoRegister {
            operator_count,
            register,
        } => (
            *operator_count,
            1,
            EditorVimOperatorGoKind::YankIntoRegister(*register),
        ),
        EditorVimPendingKey::YankMotionCount {
            operator_count,
            motion_count,
        } => (
            *operator_count,
            *motion_count,
            EditorVimOperatorGoKind::Yank,
        ),
        EditorVimPendingKey::YankMotionCountIntoRegister {
            operator_count,
            motion_count,
            register,
        } => (
            *operator_count,
            *motion_count,
            EditorVimOperatorGoKind::YankIntoRegister(*register),
        ),
        EditorVimPendingKey::ToggleCaseOperator(operator_count) => {
            (*operator_count, 1, EditorVimOperatorGoKind::ToggleCase)
        }
        EditorVimPendingKey::ToggleCaseMotionCount {
            operator_count,
            motion_count,
        } => (
            *operator_count,
            *motion_count,
            EditorVimOperatorGoKind::ToggleCase,
        ),
        EditorVimPendingKey::ConvertCaseOperator {
            operator_count,
            conversion,
        } => (
            *operator_count,
            1,
            EditorVimOperatorGoKind::ConvertCase(*conversion),
        ),
        EditorVimPendingKey::ConvertCaseMotionCount {
            operator_count,
            motion_count,
            conversion,
        } => (
            *operator_count,
            *motion_count,
            EditorVimOperatorGoKind::ConvertCase(*conversion),
        ),
        _ => return None,
    })
}

pub(in crate::editor_vim_key_events) fn vim_operator_go_motion_for_key(
    key: Key,
    modifiers: Modifiers,
) -> Option<EditorVimOperatorMotion> {
    if modifiers.command || modifiers.alt || modifiers.ctrl {
        return None;
    }
    match (key, modifiers.shift) {
        (Key::E, false) => Some(EditorVimOperatorMotion::WordEndBackward),
        (Key::E, true) => Some(EditorVimOperatorMotion::BigWordEndBackward),
        (Key::G, false) => Some(EditorVimOperatorMotion::FirstLine),
        (Key::N, false) => Some(EditorVimOperatorMotion::SearchMatch { reverse: false }),
        (Key::N, true) => Some(EditorVimOperatorMotion::SearchMatch { reverse: true }),
        (Key::Num3, true) => Some(EditorVimOperatorMotion::SearchWordUnderCursor {
            forward: false,
            whole_word: false,
        }),
        (Key::Num8, true) => Some(EditorVimOperatorMotion::SearchWordUnderCursor {
            forward: true,
            whole_word: false,
        }),
        _ => None,
    }
}

pub(in crate::editor_vim_key_events) fn vim_operator_char_find_motion_for_key(
    key: Key,
    modifiers: Modifiers,
) -> Option<EditorVimCharFindMotion> {
    if modifiers.command || modifiers.alt || modifiers.ctrl {
        return None;
    }
    match (key, modifiers.shift) {
        (Key::F, false) => Some(EditorVimCharFindMotion::FindForward),
        (Key::F, true) => Some(EditorVimCharFindMotion::FindBackward),
        (Key::T, false) => Some(EditorVimCharFindMotion::TillForward),
        (Key::T, true) => Some(EditorVimCharFindMotion::TillBackward),
        _ => None,
    }
}
