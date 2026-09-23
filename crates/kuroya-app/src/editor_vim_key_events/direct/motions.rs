use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

use super::super::motion::{
    vim_apply_char_find_repeat, vim_line_first_non_whitespace_char,
    vim_move_counted_line_first_non_whitespace, vim_move_next_line_first_non_whitespace,
    vim_move_next_paragraph, vim_move_previous_line_first_non_whitespace,
    vim_move_previous_paragraph, vim_move_space_backward, vim_move_space_forward,
    vim_move_to_line_column, vim_move_to_line_end, vim_move_to_matching_bracket,
    vim_next_word_start,
};
use super::super::search::{vim_repeat_last_search, vim_search_word_under_cursor};
use super::super::state::vim_set_previous_context_mark;
use super::super::{
    EditorVimCharFind, VimKeyResult, no_text_modifiers, vim_go_to_line, vim_line_column_motion_key,
};

pub(super) fn handle_vim_direct_motion_key(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    last_char_find: Option<EditorVimCharFind>,
    count: Option<usize>,
    count_value: usize,
    suppress_text: Option<char>,
) -> Option<VimKeyResult> {
    match key {
        Key::Semicolon if !modifiers.shift => {
            if let Some(last) = last_char_find {
                vim_apply_char_find_repeat(buffer, count_value, last.motion, last.target);
            }
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Comma if !modifiers.shift => {
            if let Some(last) = last_char_find {
                vim_apply_char_find_repeat(
                    buffer,
                    count_value,
                    last.motion.reversed(),
                    last.target,
                );
            }
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::H if !modifiers.shift => {
            for _ in 0..count_value {
                if buffer.cursor_position().column == 0 {
                    break;
                }
                buffer.move_left();
            }
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Backspace if no_text_modifiers(modifiers) => {
            vim_move_space_backward(buffer, count_value);
            Some(VimKeyResult::handled(None))
        }
        Key::Enter if no_text_modifiers(modifiers) => {
            vim_move_next_line_first_non_whitespace(buffer, count_value);
            Some(VimKeyResult::handled(None))
        }
        Key::Equals if modifiers.shift => {
            vim_move_next_line_first_non_whitespace(buffer, count_value);
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Minus if !modifiers.shift => {
            vim_move_previous_line_first_non_whitespace(buffer, count_value);
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Minus if modifiers.shift => {
            vim_move_counted_line_first_non_whitespace(buffer, count_value);
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::J if !modifiers.shift => {
            for _ in 0..count_value {
                buffer.move_down();
            }
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::K if !modifiers.shift => {
            for _ in 0..count_value {
                buffer.move_up();
            }
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::L if !modifiers.shift => {
            for _ in 0..count_value {
                let position = buffer.cursor_position();
                let line_content_end = buffer.line_content_end_char(position.line);
                if buffer.cursor() + 1 >= line_content_end {
                    break;
                }
                buffer.move_right();
            }
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Space if no_text_modifiers(modifiers) => {
            vim_move_space_forward(buffer, count_value);
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::W if !modifiers.shift => {
            let mut cursor = buffer.cursor();
            for _ in 0..count_value {
                cursor = vim_next_word_start(buffer, cursor);
            }
            buffer.set_single_cursor(cursor);
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::W if modifiers.shift => {
            for _ in 0..count_value {
                buffer.move_big_word_right();
            }
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::E if !modifiers.shift => {
            for _ in 0..count_value {
                buffer.move_word_end();
            }
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::E if modifiers.shift => {
            for _ in 0..count_value {
                buffer.move_big_word_end();
            }
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::B if !modifiers.shift => {
            for _ in 0..count_value {
                buffer.move_word_left();
            }
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::B if modifiers.shift => {
            for _ in 0..count_value {
                buffer.move_big_word_left();
            }
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Num0 if !modifiers.shift => {
            buffer.move_line_column_start();
            Some(VimKeyResult::handled(suppress_text))
        }
        key if vim_line_column_motion_key(key, modifiers) => {
            vim_move_to_line_column(buffer, count_value);
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Home if no_text_modifiers(modifiers) => {
            buffer.move_line_column_start();
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Num6 if modifiers.shift => {
            buffer.move_line_first_non_whitespace();
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Num4 if modifiers.shift => {
            vim_move_to_line_end(buffer, count_value);
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::End if no_text_modifiers(modifiers) => {
            vim_move_to_line_end(buffer, count_value);
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Num5 if modifiers.shift => {
            vim_move_to_matching_bracket(buffer);
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::CloseBracket if modifiers.shift => {
            vim_move_next_paragraph(buffer, count_value);
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::OpenBracket if modifiers.shift => {
            vim_move_previous_paragraph(buffer, count_value);
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Num8 if modifiers.shift => {
            vim_search_word_under_cursor(buffer, count_value, true, true);
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::Num3 if modifiers.shift => {
            vim_search_word_under_cursor(buffer, count_value, false, true);
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::N if !modifiers.shift => {
            vim_repeat_last_search(buffer, count_value, false);
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::N if modifiers.shift => {
            vim_repeat_last_search(buffer, count_value, true);
            Some(VimKeyResult::handled(suppress_text))
        }
        Key::G if modifiers.shift => {
            match count {
                Some(line) => vim_go_to_line(buffer, line),
                None => {
                    vim_set_previous_context_mark(buffer);

                    let mut last_line = buffer.len_lines().saturating_sub(1);
                    if last_line > 0 && buffer.line(last_line).is_some_and(|line| line.is_empty()) {
                        last_line -= 1;
                    }

                    let cursor = vim_line_first_non_whitespace_char(buffer, last_line);
                    buffer.set_single_cursor(cursor);
                }
            }
            Some(VimKeyResult::handled(suppress_text))
        }
        _ => None,
    }
}
