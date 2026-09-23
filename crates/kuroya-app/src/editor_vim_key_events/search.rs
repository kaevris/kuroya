mod input;
mod matching;
mod repeat;
mod state;
mod status;
mod word;

pub(crate) use input::vim_clear_search_input;
pub(super) use input::{
    handle_vim_search_input_key_event, vim_search_input_accept_key, vim_search_input_cancel_key,
    vim_search_input_control_edit,
};
#[cfg(test)]
pub(in crate::editor_vim_key_events) use matching::vim_search_word_target;
pub(super) use repeat::{
    vim_operator_search_match_range, vim_operator_search_repeat_range, vim_repeat_last_search,
    vim_search_match_range,
};
pub(in crate::editor_vim_key_events) use state::VIM_SEARCH_INPUT;
#[cfg(test)]
pub(in crate::editor_vim_key_events) use state::{VIM_SEARCHES, vim_set_last_search};
pub(crate) use state::{
    vim_clear_searches, vim_forget_searches_for_buffer, vim_search_highlight_ranges_for_buffer,
};
#[cfg(test)]
pub(crate) use state::{vim_last_search_word_for_test, vim_set_last_search_for_test};
pub(crate) use status::vim_pending_search_status_label;
pub(super) use word::{
    vim_literal_search, vim_operator_search_word_under_cursor_range, vim_search_word_under_cursor,
};
