use super::matching::vim_search_highlight_ranges_for_query;
use kuroya_core::{BufferId, TextBuffer};
use std::{cell::RefCell, ops::Range};

const VIM_SEARCH_BUFFER_LIMIT: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::editor_vim_key_events) struct EditorVimSearch {
    pub(in crate::editor_vim_key_events) word: String,
    pub(in crate::editor_vim_key_events) forward: bool,
    pub(in crate::editor_vim_key_events) whole_word: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::editor_vim_key_events) struct EditorVimBufferSearch {
    pub(in crate::editor_vim_key_events) buffer_id: BufferId,
    pub(in crate::editor_vim_key_events) search: EditorVimSearch,
}

thread_local! {
    pub(in crate::editor_vim_key_events) static VIM_SEARCH_INPUT: RefCell<String> = const { RefCell::new(String::new()) };
    pub(in crate::editor_vim_key_events) static VIM_SEARCHES: RefCell<Vec<EditorVimBufferSearch>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn vim_clear_searches() {
    VIM_SEARCHES.with(|searches| searches.borrow_mut().clear());
}

pub(crate) fn vim_forget_searches_for_buffer(buffer_id: BufferId) {
    VIM_SEARCHES.with(|searches| {
        searches
            .borrow_mut()
            .retain(|entry| entry.buffer_id != buffer_id);
    });
}

#[cfg(test)]
pub(crate) fn vim_last_search_word_for_test(buffer_id: BufferId) -> Option<String> {
    VIM_SEARCHES.with(|searches| {
        searches
            .borrow()
            .iter()
            .find(|entry| entry.buffer_id == buffer_id)
            .map(|entry| entry.search.word.clone())
    })
}

#[cfg(test)]
pub(crate) fn vim_set_last_search_for_test(
    buffer: &TextBuffer,
    word: &str,
    forward: bool,
    whole_word: bool,
) {
    vim_set_last_search(buffer, word, forward, whole_word);
}

pub(crate) fn vim_search_highlight_ranges_for_buffer(buffer: &TextBuffer) -> Vec<Range<usize>> {
    let buffer_id = buffer.id();
    VIM_SEARCHES.with(|searches| {
        let searches = searches.borrow();
        let Some(search) = searches
            .iter()
            .find(|entry| entry.buffer_id == buffer_id)
            .map(|entry| &entry.search)
        else {
            return Vec::new();
        };
        vim_search_highlight_ranges_for_query(buffer, &search.word, search.whole_word)
    })
}

pub(in crate::editor_vim_key_events) fn vim_set_last_search(
    buffer: &TextBuffer,
    word: &str,
    forward: bool,
    whole_word: bool,
) {
    let buffer_id = buffer.id();
    VIM_SEARCHES.with(|searches| {
        let mut searches = searches.borrow_mut();
        if let Some(existing) = searches
            .iter_mut()
            .find(|entry| entry.buffer_id == buffer_id)
        {
            if existing.search.word == word
                && existing.search.forward == forward
                && existing.search.whole_word == whole_word
            {
                return;
            }
            existing.search.word.clear();
            existing.search.word.push_str(word);
            existing.search.forward = forward;
            existing.search.whole_word = whole_word;
            return;
        }

        if searches.len() >= VIM_SEARCH_BUFFER_LIMIT {
            searches.remove(0);
        }
        let search = EditorVimSearch {
            word: word.to_owned(),
            forward,
            whole_word,
        };
        searches.push(EditorVimBufferSearch { buffer_id, search });
    });
}
