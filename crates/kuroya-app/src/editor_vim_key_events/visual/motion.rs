use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

use super::super::motion::vim_next_word_start;
use super::super::{
    EditorVimCharFindMotion, VIM_MAX_COUNT, no_text_modifiers, vim_apply_char_find,
    vim_line_column_motion_key, vim_move_counted_line_first_non_whitespace,
    vim_move_next_line_first_non_whitespace, vim_move_next_paragraph,
    vim_move_previous_line_first_non_whitespace, vim_move_previous_paragraph,
    vim_move_space_backward, vim_move_space_forward, vim_move_to_line_column,
    vim_move_to_matching_bracket, vim_operator_motion_for_key, vim_repeat_last_search,
    vim_search_word_under_cursor,
};
use super::selection::vim_visual_character_clamped_cursor;
pub(in crate::editor_vim_key_events) fn vim_visual_character_char_find_target(
    buffer: &mut TextBuffer,
    cursor: usize,
    count: usize,
    motion: EditorVimCharFindMotion,
    target: char,
) -> Option<usize> {
    let original_cursor = buffer.cursor();
    buffer.set_single_cursor(cursor.min(buffer.len_chars()));
    let found = vim_apply_char_find(buffer, count, motion, target);
    let target_cursor = vim_visual_character_clamped_cursor(buffer, buffer.cursor());
    buffer.set_single_cursor(original_cursor.min(buffer.len_chars()));
    found.then_some(target_cursor)
}

pub(in crate::editor_vim_key_events) fn vim_visual_character_motion_key(
    key: Key,
    modifiers: Modifiers,
) -> bool {
    if vim_operator_motion_for_key(key, modifiers).is_some() {
        return true;
    }
    if modifiers.command || modifiers.alt || modifiers.ctrl {
        return false;
    }
    if matches!(key, Key::Enter) {
        return no_text_modifiers(modifiers);
    }
    matches!(
        (key, modifiers.shift),
        (Key::Equals, true)
            | (Key::J, false)
            | (Key::K, false)
            | (Key::Minus, false)
            | (Key::Minus, true)
            | (Key::N, false)
            | (Key::N, true)
            | (Key::Num3, true)
            | (Key::Num8, true)
    )
}

pub(in crate::editor_vim_key_events) fn vim_visual_character_motion_target(
    buffer: &mut TextBuffer,
    cursor: usize,
    count: usize,
    key: Key,
    modifiers: Modifiers,
) -> Option<usize> {
    if !vim_visual_character_motion_key(key, modifiers) {
        return None;
    }
    let count = count.clamp(1, VIM_MAX_COUNT);
    buffer.set_single_cursor(cursor.min(buffer.len_chars()));
    match key {
        Key::H if !modifiers.shift => {
            for _ in 0..count {
                buffer.move_left();
            }
        }
        Key::Backspace if no_text_modifiers(modifiers) => {
            vim_move_space_backward(buffer, count);
        }
        Key::Enter if no_text_modifiers(modifiers) => {
            vim_move_next_line_first_non_whitespace(buffer, count);
        }
        Key::Equals if modifiers.shift => {
            vim_move_next_line_first_non_whitespace(buffer, count);
        }
        Key::Minus if !modifiers.shift => {
            vim_move_previous_line_first_non_whitespace(buffer, count);
        }
        Key::Minus if modifiers.shift => {
            vim_move_counted_line_first_non_whitespace(buffer, count);
        }
        Key::J if !modifiers.shift => {
            for _ in 0..count {
                buffer.move_down();
            }
        }
        Key::K if !modifiers.shift => {
            for _ in 0..count {
                buffer.move_up();
            }
        }
        Key::L if !modifiers.shift => {
            for _ in 0..count {
                buffer.move_right();
            }
        }
        Key::Space if no_text_modifiers(modifiers) => {
            vim_move_space_forward(buffer, count);
        }
        Key::W if !modifiers.shift => {
            for _ in 0..count {
                let cursor = buffer.cursor();
                let next = vim_next_word_start(buffer, cursor);
                if next == cursor {
                    break;
                }
                buffer.set_single_cursor(next);
            }
        }
        Key::W if modifiers.shift => {
            for _ in 0..count {
                buffer.move_big_word_right();
            }
        }
        Key::E if !modifiers.shift => {
            for _ in 0..count {
                buffer.move_word_end();
            }
        }
        Key::E if modifiers.shift => {
            for _ in 0..count {
                buffer.move_big_word_end();
            }
        }
        Key::B if !modifiers.shift => {
            for _ in 0..count {
                buffer.move_word_left();
            }
        }
        Key::B if modifiers.shift => {
            for _ in 0..count {
                buffer.move_big_word_left();
            }
        }
        Key::Num0 if !modifiers.shift => {
            buffer.move_line_column_start();
        }
        key if vim_line_column_motion_key(key, modifiers) => {
            vim_move_to_line_column(buffer, count);
        }
        Key::Home if no_text_modifiers(modifiers) => {
            buffer.move_line_column_start();
        }
        Key::Num6 if modifiers.shift => {
            buffer.move_line_first_non_whitespace();
        }
        Key::Num4 if modifiers.shift => {
            vim_move_to_visual_line_end(buffer, count);
        }
        Key::End if no_text_modifiers(modifiers) => {
            vim_move_to_visual_line_end(buffer, count);
        }
        Key::Num5 if modifiers.shift => {
            vim_move_to_matching_bracket(buffer);
        }
        Key::CloseBracket if modifiers.shift => {
            vim_move_next_paragraph(buffer, count);
        }
        Key::OpenBracket if modifiers.shift => {
            vim_move_previous_paragraph(buffer, count);
        }
        Key::Num8 if modifiers.shift => {
            vim_search_word_under_cursor(buffer, count, true, true);
        }
        Key::Num3 if modifiers.shift => {
            vim_search_word_under_cursor(buffer, count, false, true);
        }
        Key::N if !modifiers.shift => {
            vim_repeat_last_search(buffer, count, false);
        }
        Key::N if modifiers.shift => {
            vim_repeat_last_search(buffer, count, true);
        }
        _ => return None,
    }
    Some(vim_visual_character_clamped_cursor(buffer, buffer.cursor()))
}

pub(in crate::editor_vim_key_events) fn vim_move_to_visual_line_end(
    buffer: &mut TextBuffer,
    count: usize,
) {
    let count = count.clamp(1, VIM_MAX_COUNT);
    let line = buffer
        .cursor_position()
        .line
        .saturating_add(count.saturating_sub(1))
        .min(buffer.len_lines().saturating_sub(1));
    let line_start = buffer.line_column_to_char(line, 0);
    let content_end = buffer.line_content_end_char(line);
    let last_cluster_start = buffer
        .snap_back_to_grapheme_boundary(content_end.saturating_sub(1))
        .max(line_start);
    buffer.set_single_cursor(last_cluster_start);
}
