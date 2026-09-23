mod brackets;
mod case;
mod char_find;
mod character_edits;
mod chars;
mod joins;
mod line_edits;
mod line_motions;
mod paragraphs;
mod scroll;
mod words;

pub(super) use brackets::{vim_matching_bracket_range, vim_move_to_matching_bracket};
pub(super) use case::{
    vim_case_conversion_repeated_operator_key, vim_convert_case_forward_chars,
    vim_convert_case_lines, vim_convert_case_range, vim_toggle_case_forward_chars,
    vim_toggle_case_range,
};
pub(super) use char_find::{vim_apply_char_find, vim_apply_char_find_repeat};
pub(super) use character_edits::{
    vim_delete_backward_chars, vim_delete_backward_chars_into_named_register,
    vim_delete_forward_chars, vim_delete_forward_chars_into_named_register,
    vim_delete_line_backward, vim_replace_forward_chars,
};
pub(super) use chars::vim_char_at;
pub(super) use joins::{vim_join_lines, vim_join_lines_without_whitespace};
pub(super) use line_edits::{
    vim_indent_lines, vim_line_outdent_len, vim_open_line_above, vim_open_line_below,
    vim_outdent_lines,
};
#[cfg(test)]
pub(super) use line_edits::{vim_open_line_above_text, vim_open_line_below_text};
pub(super) use line_motions::{
    vim_line_column_motion_char, vim_line_first_non_whitespace_char,
    vim_move_counted_line_first_non_whitespace, vim_move_next_line_first_non_whitespace,
    vim_move_previous_line_first_non_whitespace, vim_move_space_backward, vim_move_space_forward,
    vim_move_to_line_column, vim_move_to_line_end,
};
pub(super) use paragraphs::{
    vim_move_next_paragraph, vim_move_previous_paragraph, vim_next_paragraph_line,
    vim_previous_paragraph_line,
};
pub(super) use scroll::{
    vim_ctrl_scroll_lines, vim_line_scroll_lines, vim_move_down_lines, vim_move_up_lines,
    vim_page_scroll_lines,
};
pub(super) use words::{
    vim_change_word_motion_range, vim_move_previous_big_word_end, vim_next_word_start,
    vim_word_motion_range,
};
