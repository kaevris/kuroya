use eframe::egui::{Key, Modifiers};

use super::super::super::{
    EditorVimPendingKey, vim_printable_key_char, vim_search_direction_for_key,
};
use super::super::counts::{vim_count_digit, vim_push_count_digit};

pub(in crate::editor_vim_key_events) fn vim_normal_key_next_pending(
    key: Key,
    modifiers: Modifiers,
) -> Option<EditorVimPendingKey> {
    if modifiers.command || modifiers.alt || modifiers.ctrl {
        return None;
    }
    if key == Key::F {
        return Some(if modifiers.shift {
            EditorVimPendingKey::FindCharBackward(1)
        } else {
            EditorVimPendingKey::FindCharForward(1)
        });
    }
    if key == Key::T {
        return Some(if modifiers.shift {
            EditorVimPendingKey::TillCharBackward(1)
        } else {
            EditorVimPendingKey::TillCharForward(1)
        });
    }
    if key == Key::Period && modifiers.shift {
        return Some(EditorVimPendingKey::IndentLine(1));
    }
    if key == Key::Comma && modifiers.shift {
        return Some(EditorVimPendingKey::OutdentLine(1));
    }
    if vim_printable_key_char(key, modifiers) == Some(':') {
        return Some(EditorVimPendingKey::CommandInput);
    }
    if let Some(forward) = vim_search_direction_for_key(key, modifiers) {
        return Some(EditorVimPendingKey::SearchInput { count: 1, forward });
    }
    if key == Key::Quote && modifiers.shift {
        return Some(EditorVimPendingKey::RegisterPrefix(1));
    }
    if key == Key::V && modifiers.shift {
        return Some(EditorVimPendingKey::VisualLine {
            anchor: 0,
            cursor: 0,
            count: None,
        });
    }
    if modifiers.shift {
        return None;
    }
    match key {
        Key::C => Some(EditorVimPendingKey::ChangeLine(1)),
        Key::D => Some(EditorVimPendingKey::DeleteLine(1)),
        Key::G => Some(EditorVimPendingKey::Go(None)),
        Key::M => Some(EditorVimPendingKey::SetMark),
        Key::Quote => Some(EditorVimPendingKey::JumpMark { linewise: true }),
        Key::Backtick => Some(EditorVimPendingKey::JumpMark { linewise: false }),
        Key::R => Some(EditorVimPendingKey::ReplaceChar(1)),
        Key::V => Some(EditorVimPendingKey::VisualCharacter {
            anchor: 0,
            cursor: 0,
        }),
        Key::Y => Some(EditorVimPendingKey::YankLine(1)),
        key => vim_count_digit(key, modifiers, false).map(EditorVimPendingKey::Count),
    }
}

pub(in crate::editor_vim_key_events) fn vim_normal_key_next_pending_after_count(
    count: usize,
    key: Key,
    modifiers: Modifiers,
) -> Option<EditorVimPendingKey> {
    if modifiers.command || modifiers.alt || modifiers.ctrl {
        return None;
    }
    if key == Key::F {
        return Some(if modifiers.shift {
            EditorVimPendingKey::FindCharBackward(count)
        } else {
            EditorVimPendingKey::FindCharForward(count)
        });
    }
    if key == Key::T {
        return Some(if modifiers.shift {
            EditorVimPendingKey::TillCharBackward(count)
        } else {
            EditorVimPendingKey::TillCharForward(count)
        });
    }
    if key == Key::Period && modifiers.shift {
        return Some(EditorVimPendingKey::IndentLine(count));
    }
    if key == Key::Comma && modifiers.shift {
        return Some(EditorVimPendingKey::OutdentLine(count));
    }
    if vim_printable_key_char(key, modifiers) == Some(':') {
        return Some(EditorVimPendingKey::CommandInput);
    }
    if let Some(forward) = vim_search_direction_for_key(key, modifiers) {
        return Some(EditorVimPendingKey::SearchInput { count, forward });
    }
    if key == Key::Quote && modifiers.shift {
        return Some(EditorVimPendingKey::RegisterPrefix(count));
    }
    if key == Key::V && modifiers.shift {
        return Some(EditorVimPendingKey::VisualLine {
            anchor: 0,
            cursor: 0,
            count: None,
        });
    }
    if modifiers.shift {
        return None;
    }
    match key {
        Key::C => Some(EditorVimPendingKey::ChangeLine(count)),
        Key::D => Some(EditorVimPendingKey::DeleteLine(count)),
        Key::G => Some(EditorVimPendingKey::Go(Some(count))),
        Key::M => Some(EditorVimPendingKey::SetMark),
        Key::Quote => Some(EditorVimPendingKey::JumpMark { linewise: true }),
        Key::Backtick => Some(EditorVimPendingKey::JumpMark { linewise: false }),
        Key::R => Some(EditorVimPendingKey::ReplaceChar(count)),
        Key::V => Some(EditorVimPendingKey::VisualCharacter {
            anchor: 0,
            cursor: 0,
        }),
        Key::Y => Some(EditorVimPendingKey::YankLine(count)),
        key => vim_count_digit(key, modifiers, true)
            .map(|digit| EditorVimPendingKey::Count(vim_push_count_digit(count, digit))),
    }
}
