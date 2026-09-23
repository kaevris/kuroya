use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

use super::super::search::vim_clear_search_input;
use super::super::visual::{
    vim_set_visual_character_selection, vim_set_visual_line_selection,
    vim_visual_character_clamped_cursor,
};
use super::super::{
    EditorVimPendingKey, VimKeyResult, vim_clear_command_input, vim_count_digit,
    vim_search_direction_for_key,
};

pub(super) fn handle_vim_direct_pending_starter_key(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    pending: &mut Option<EditorVimPendingKey>,
    count: Option<usize>,
    count_value: usize,
    suppress_text: Option<char>,
) -> Option<VimKeyResult> {
    match key {
        Key::Escape => Some(VimKeyResult::handled(None)),
        key if vim_count_digit(key, modifiers, false).is_some() => {
            let count = vim_count_digit(key, modifiers, false).unwrap_or(1);
            *pending = Some(EditorVimPendingKey::Count(count));
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Quote if modifiers.shift => {
            *pending = Some(EditorVimPendingKey::RegisterPrefix(count_value));
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::D if !modifiers.shift => {
            *pending = Some(EditorVimPendingKey::DeleteLine(count_value));
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::C if !modifiers.shift => {
            *pending = Some(EditorVimPendingKey::ChangeLine(count_value));
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Y if !modifiers.shift => {
            *pending = Some(EditorVimPendingKey::YankLine(count_value));
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::V if !modifiers.shift => {
            let cursor = vim_visual_character_clamped_cursor(buffer, buffer.cursor());
            vim_set_visual_character_selection(buffer, cursor, cursor);
            *pending = Some(EditorVimPendingKey::VisualCharacter {
                anchor: cursor,
                cursor,
            });
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::V if modifiers.shift => {
            let cursor = vim_visual_character_clamped_cursor(buffer, buffer.cursor());
            vim_set_visual_line_selection(buffer, cursor, cursor);
            *pending = Some(EditorVimPendingKey::VisualLine {
                anchor: cursor,
                cursor,
                count: None,
            });
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::F if !modifiers.shift => {
            *pending = Some(EditorVimPendingKey::FindCharForward(count_value));
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::F if modifiers.shift => {
            *pending = Some(EditorVimPendingKey::FindCharBackward(count_value));
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::T if !modifiers.shift => {
            *pending = Some(EditorVimPendingKey::TillCharForward(count_value));
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::T if modifiers.shift => {
            *pending = Some(EditorVimPendingKey::TillCharBackward(count_value));
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::R if !modifiers.shift => {
            *pending = Some(EditorVimPendingKey::ReplaceChar(count_value));
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Period if modifiers.shift => {
            *pending = Some(EditorVimPendingKey::IndentLine(count_value));
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Comma if modifiers.shift => {
            *pending = Some(EditorVimPendingKey::OutdentLine(count_value));
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::G if !modifiers.shift => {
            *pending = Some(EditorVimPendingKey::Go(count));
            Some(VimKeyResult::handled(suppress_text))
        }
        key if vim_search_direction_for_key(key, modifiers).is_some() => {
            let forward = vim_search_direction_for_key(key, modifiers).unwrap_or(true);
            vim_clear_search_input();
            *pending = Some(EditorVimPendingKey::SearchInput {
                count: count_value,
                forward,
            });
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Colon => {
            vim_clear_command_input();
            *pending = Some(EditorVimPendingKey::CommandInput);
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Semicolon if modifiers.shift => {
            vim_clear_command_input();
            *pending = Some(EditorVimPendingKey::CommandInput);
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::M if !modifiers.shift => {
            *pending = Some(EditorVimPendingKey::SetMark);
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Quote if !modifiers.shift => {
            *pending = Some(EditorVimPendingKey::JumpMark { linewise: true });
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Backtick if !modifiers.shift => {
            *pending = Some(EditorVimPendingKey::JumpMark { linewise: false });
            Some(VimKeyResult::handled(suppress_text))
        }
        _ => None,
    }
}
