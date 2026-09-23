use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

use super::super::state::vim_visual_bounds_marks;
use super::super::visual::vim_visual_character_clamped_cursor;
use super::super::{
    EditorVimCaseConversion, EditorVimCharFind, EditorVimCharFindMotion, EditorVimLastChange,
    EditorVimPendingKey, EditorVimRepeatAction, VIM_MAX_COUNT, VimKeyResult, vim_apply_char_find,
    vim_go_to_line, vim_indent_lines, vim_join_lines_without_whitespace,
    vim_jump_mark_name_for_key, vim_jump_to_mark, vim_mark_name_for_key,
    vim_move_previous_big_word_end, vim_outdent_lines, vim_printable_key_char,
    vim_repeatable_change_result, vim_replace_forward_chars, vim_replacement_key_char,
    vim_search_match_range, vim_search_word_under_cursor, vim_set_mark,
    vim_set_visual_character_selection,
};

pub(super) fn handle_vim_simple_pending_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    pending: &mut Option<EditorVimPendingKey>,
    last_char_find: &mut Option<EditorVimCharFind>,
    last_change: &mut Option<EditorVimLastChange>,
    indent_unit: &str,
    pending_key: EditorVimPendingKey,
    suppress_text: Option<char>,
) -> Option<VimKeyResult> {
    match pending_key {
        EditorVimPendingKey::FindCharForward(count) => handle_vim_char_find_input(
            buffer,
            key,
            modifiers,
            last_char_find,
            count,
            EditorVimCharFindMotion::FindForward,
            suppress_text,
        ),
        EditorVimPendingKey::FindCharBackward(count) => handle_vim_char_find_input(
            buffer,
            key,
            modifiers,
            last_char_find,
            count,
            EditorVimCharFindMotion::FindBackward,
            suppress_text,
        ),
        EditorVimPendingKey::TillCharForward(count) => handle_vim_char_find_input(
            buffer,
            key,
            modifiers,
            last_char_find,
            count,
            EditorVimCharFindMotion::TillForward,
            suppress_text,
        ),
        EditorVimPendingKey::TillCharBackward(count) => handle_vim_char_find_input(
            buffer,
            key,
            modifiers,
            last_char_find,
            count,
            EditorVimCharFindMotion::TillBackward,
            suppress_text,
        ),
        EditorVimPendingKey::Go(count) => handle_vim_go_pending_key_event(
            buffer,
            key,
            modifiers,
            pending,
            last_change,
            count,
            suppress_text,
        ),
        EditorVimPendingKey::JumpMark { linewise } => {
            let mark = vim_jump_mark_name_for_key(key, modifiers)?;
            vim_jump_to_mark(buffer, mark, linewise);
            Some(VimKeyResult::handled(suppress_text))
        }
        EditorVimPendingKey::IndentLine(count) if key == Key::Period && modifiers.shift => {
            Some(vim_repeatable_change_result(
                vim_indent_lines(buffer, count, indent_unit),
                last_change,
                EditorVimRepeatAction::IndentLines,
                count,
                suppress_text,
            ))
        }
        EditorVimPendingKey::OutdentLine(count) if key == Key::Comma && modifiers.shift => {
            Some(vim_repeatable_change_result(
                vim_outdent_lines(buffer, count, indent_unit),
                last_change,
                EditorVimRepeatAction::OutdentLines,
                count,
                suppress_text,
            ))
        }
        EditorVimPendingKey::ReplaceChar(count) => {
            let replacement = vim_replacement_key_char(key, modifiers)?;
            Some(vim_repeatable_change_result(
                vim_replace_forward_chars(buffer, count, replacement),
                last_change,
                EditorVimRepeatAction::ReplaceForwardChars(replacement),
                count,
                suppress_text,
            ))
        }
        EditorVimPendingKey::SetMark => {
            let mark = vim_mark_name_for_key(key, modifiers)?;
            vim_set_mark(buffer, mark);
            Some(VimKeyResult::handled(suppress_text))
        }
        _ => None,
    }
}

fn handle_vim_char_find_input(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    last_char_find: &mut Option<EditorVimCharFind>,
    count: usize,
    motion: EditorVimCharFindMotion,
    suppress_text: Option<char>,
) -> Option<VimKeyResult> {
    let target = vim_printable_key_char(key, modifiers)?;
    *last_char_find = Some(EditorVimCharFind { motion, target });
    vim_apply_char_find(buffer, count, motion, target);
    Some(VimKeyResult::handled(suppress_text))
}

fn handle_vim_go_pending_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    pending: &mut Option<EditorVimPendingKey>,
    last_change: &mut Option<EditorVimLastChange>,
    count: Option<usize>,
    suppress_text: Option<char>,
) -> Option<VimKeyResult> {
    match key {
        Key::V if !modifiers.shift => {
            let Some(((start_line, start_column), (end_line, end_column))) =
                vim_visual_bounds_marks(buffer)
            else {
                return Some(VimKeyResult::handled(suppress_text));
            };
            let last_line = buffer.len_lines().saturating_sub(1);
            let anchor = buffer.line_column_to_char(start_line.min(last_line), start_column);
            let cursor = buffer.line_column_to_char(end_line.min(last_line), end_column);
            let anchor = vim_visual_character_clamped_cursor(buffer, anchor);
            let cursor = vim_visual_character_clamped_cursor(buffer, cursor);
            vim_set_visual_character_selection(buffer, anchor, cursor);
            *pending = Some(EditorVimPendingKey::VisualCharacter { anchor, cursor });
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::G if !modifiers.shift => {
            vim_go_to_line(buffer, count.unwrap_or(1));
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Num8 if modifiers.shift => {
            vim_search_word_under_cursor(buffer, count.unwrap_or(1), true, false);
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Num3 if modifiers.shift => {
            vim_search_word_under_cursor(buffer, count.unwrap_or(1), false, false);
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::N if !modifiers.command && !modifiers.alt && !modifiers.ctrl => {
            if let Some(range) = vim_search_match_range(buffer, count.unwrap_or(1), modifiers.shift)
                && range.start < range.end
            {
                let cursor = range.end.saturating_sub(1);
                vim_set_visual_character_selection(buffer, range.start, cursor);
                *pending = Some(EditorVimPendingKey::VisualCharacter {
                    anchor: range.start,
                    cursor,
                });
            }
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::E if !modifiers.shift => {
            for _ in 0..count.unwrap_or(1).clamp(1, VIM_MAX_COUNT) {
                buffer.move_previous_word_end();
            }
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::E if modifiers.shift => {
            for _ in 0..count.unwrap_or(1).clamp(1, VIM_MAX_COUNT) {
                vim_move_previous_big_word_end(buffer);
            }
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::J if modifiers.shift => {
            let count = count.unwrap_or(1).clamp(1, VIM_MAX_COUNT);
            Some(vim_repeatable_change_result(
                vim_join_lines_without_whitespace(buffer, count),
                last_change,
                EditorVimRepeatAction::JoinLinesWithoutWhitespace,
                count,
                suppress_text,
            ))
        }
        Key::U if !modifiers.command && !modifiers.alt && !modifiers.ctrl => {
            *pending = Some(EditorVimPendingKey::ConvertCaseOperator {
                operator_count: count.unwrap_or(1),
                conversion: if modifiers.shift {
                    EditorVimCaseConversion::Upper
                } else {
                    EditorVimCaseConversion::Lower
                },
            });
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Backtick if modifiers.shift => {
            *pending = Some(EditorVimPendingKey::ToggleCaseOperator(count.unwrap_or(1)));
            Some(VimKeyResult::handled(suppress_text))
        }
        _ => None,
    }
}
