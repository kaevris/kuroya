use super::super::state::vim_set_previous_context_mark;
use super::matching::{vim_search_match_range_for_query, vim_search_word_target};
use super::state::VIM_SEARCHES;
use kuroya_core::TextBuffer;
use std::ops::Range;

pub(in crate::editor_vim_key_events) fn vim_operator_search_repeat_range(
    buffer: &mut TextBuffer,
    count: usize,
    reverse: bool,
) -> Option<Range<usize>> {
    let start = buffer.cursor();

    let target = vim_last_search_target(buffer, count, reverse)?;
    buffer.set_single_cursor(target);
    let end = buffer.cursor();
    buffer.set_single_cursor(start);
    (start != end).then_some(start.min(end)..start.max(end))
}

pub(in crate::editor_vim_key_events) fn vim_operator_search_match_range(
    buffer: &TextBuffer,
    count: usize,
    reverse: bool,
) -> Option<Range<usize>> {
    vim_search_match_range(buffer, count, reverse)
}

pub(in crate::editor_vim_key_events) fn vim_search_match_range(
    buffer: &TextBuffer,
    count: usize,
    reverse: bool,
) -> Option<Range<usize>> {
    let buffer_id = buffer.id();
    let origin = buffer.cursor();
    VIM_SEARCHES.with(|searches| {
        let mut searches = searches.borrow_mut();
        let search = searches
            .iter_mut()
            .find(|entry| entry.buffer_id == buffer_id)
            .map(|entry| &mut entry.search)?;
        let forward = if reverse {
            !search.forward
        } else {
            search.forward
        };
        let range = vim_search_match_range_for_query(
            buffer,
            &search.word,
            origin,
            count,
            forward,
            search.whole_word,
        )?;
        search.forward = forward;
        Some(range)
    })
}

pub(in crate::editor_vim_key_events) fn vim_repeat_last_search(
    buffer: &mut TextBuffer,
    count: usize,
    reverse: bool,
) -> bool {
    let Some(target) = vim_last_search_target(buffer, count, reverse) else {
        return false;
    };

    vim_set_previous_context_mark(buffer);
    buffer.set_single_cursor(target);
    true
}

fn vim_last_search_target(buffer: &TextBuffer, count: usize, reverse: bool) -> Option<usize> {
    let buffer_id = buffer.id();
    let origin = buffer.cursor();
    VIM_SEARCHES.with(|searches| {
        let searches = searches.borrow();
        let search = searches
            .iter()
            .find(|entry| entry.buffer_id == buffer_id)
            .map(|entry| &entry.search)?;
        let forward = if reverse {
            !search.forward
        } else {
            search.forward
        };
        vim_search_word_target(
            buffer,
            &search.word,
            origin,
            count,
            forward,
            search.whole_word,
        )
    })
}

pub(in crate::editor_vim_key_events) fn vim_repeat_last_search_in_direction(
    buffer: &mut TextBuffer,
    count: usize,
    forward: bool,
) -> bool {
    let buffer_id = buffer.id();
    let origin = buffer.cursor();
    let target = VIM_SEARCHES.with(|searches| {
        let mut searches = searches.borrow_mut();
        let search = searches
            .iter_mut()
            .find(|entry| entry.buffer_id == buffer_id)
            .map(|entry| &mut entry.search)?;
        let target = vim_search_word_target(
            buffer,
            &search.word,
            origin,
            count,
            forward,
            search.whole_word,
        )?;
        search.forward = forward;
        Some(target)
    });

    let Some(target) = target else {
        return false;
    };

    vim_set_previous_context_mark(buffer);
    buffer.set_single_cursor(target);
    true
}
