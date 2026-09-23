use std::{cmp::Ordering, sync::Arc};

#[cfg(test)]
use super::query::normalize_terminal_search_query;
use super::{TerminalSearchMatch, TerminalVisibleSearchSpan};

const TERMINAL_SEARCH_PREVIEW_MAX_CHARS: usize = 160;
const TERMINAL_SEARCH_PREVIEW_MAX_BYTES: usize = TERMINAL_SEARCH_PREVIEW_MAX_CHARS * 4 + 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TerminalVisibleCellSpan {
    pub(super) byte_start: usize,
    pub(super) byte_end: usize,
    pub(super) start_col: u16,
    pub(super) end_col: u16,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PreparedTerminalSearchQuery<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) byte_len: usize,
    pub(super) first_byte: u8,
    pub(super) first_byte_needs_ascii_case_fold: bool,
    pub(super) is_ascii: bool,
}

impl<'a> PreparedTerminalSearchQuery<'a> {
    pub(super) fn new(query: &'a str) -> Option<Self> {
        let bytes = query.as_bytes();
        let first_byte = bytes.first().copied()?;
        Some(Self {
            bytes,
            byte_len: bytes.len(),
            first_byte,
            first_byte_needs_ascii_case_fold: first_byte.is_ascii_alphabetic(),
            is_ascii: query.is_ascii(),
        })
    }

    pub(super) fn byte_len(self) -> usize {
        self.byte_len
    }

    fn may_match_line(self, line: &str) -> bool {
        line.len() >= self.byte_len
    }

    pub(super) fn find_from(self, haystack: &str, search_from: usize) -> Option<usize> {
        let haystack_bytes = haystack.as_bytes();
        let max_start = haystack_bytes.len().checked_sub(self.byte_len)?;
        if search_from > max_start {
            return None;
        }

        let mut start = if self.is_ascii {
            search_from
        } else {
            terminal_search_next_char_boundary_at_or_after(haystack, search_from, max_start)?
        };

        while start <= max_start {
            let offset = haystack_bytes[start..=max_start].iter().position(|byte| {
                terminal_search_first_byte_matches(
                    *byte,
                    self.first_byte,
                    self.first_byte_needs_ascii_case_fold,
                )
            })?;
            start += offset;

            let end = start + self.byte_len;
            let matched = if self.is_ascii {
                self.byte_len == 1
                    || terminal_search_ascii_case_insensitive_bytes_eq(
                        &haystack_bytes[start + 1..end],
                        &self.bytes[1..],
                    )
            } else {
                terminal_search_ascii_case_insensitive_bytes_eq(
                    &haystack_bytes[start..end],
                    self.bytes,
                ) && haystack.is_char_boundary(start)
                    && haystack.is_char_boundary(end)
            };
            if matched {
                return Some(start);
            }

            start = start.saturating_add(1);
            if !self.is_ascii {
                start = terminal_search_next_char_boundary_at_or_after(haystack, start, max_start)?;
            }
        }

        None
    }
}

pub(super) fn terminal_search_first_byte_matches(
    haystack_byte: u8,
    query_first: u8,
    needs_ascii_case_fold: bool,
) -> bool {
    if needs_ascii_case_fold {
        haystack_byte.eq_ignore_ascii_case(&query_first)
    } else {
        haystack_byte == query_first
    }
}

pub(super) fn terminal_search_ascii_case_insensitive_bytes_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
}

pub(super) fn terminal_search_next_char_boundary_at_or_after(
    haystack: &str,
    mut index: usize,
    max: usize,
) -> Option<usize> {
    while index <= max && !haystack.is_char_boundary(index) {
        index += 1;
    }
    (index <= max).then_some(index)
}

pub(super) fn terminal_search_cached_match_matches_query(
    matched: &TerminalSearchMatch,
    line: &str,
    query: PreparedTerminalSearchQuery<'_>,
) -> bool {
    let Some(match_len) = matched.end.checked_sub(matched.start) else {
        return false;
    };
    if match_len != query.byte_len() || matched.end > line.len() {
        return false;
    }
    if !line.is_char_boundary(matched.start) || !line.is_char_boundary(matched.end) {
        return false;
    }

    terminal_search_ascii_case_insensitive_bytes_eq(
        &line.as_bytes()[matched.start..matched.end],
        query.bytes,
    ) && terminal_search_preview_matches_line(matched.preview.as_str(), line)
}

pub(super) fn terminal_search_preview_matches_line(preview: &str, line: &str) -> bool {
    let mut line_chars = line.trim().chars();
    let mut preview_chars = preview.chars();

    for line_ch in line_chars.by_ref().take(TERMINAL_SEARCH_PREVIEW_MAX_CHARS) {
        if preview_chars.next() != Some(line_ch) {
            return false;
        }
    }

    if line_chars.next().is_some() {
        preview_chars.next() == Some('.')
            && preview_chars.next() == Some('.')
            && preview_chars.next() == Some('.')
            && preview_chars.next().is_none()
    } else {
        preview_chars.next().is_none()
    }
}

#[cfg(test)]
pub(in crate::terminal) fn terminal_search_matches(
    session_id: usize,
    text: &str,
    query: &str,
) -> Vec<TerminalSearchMatch> {
    let mut matches = Vec::new();
    terminal_search_matches_into(
        &mut matches,
        session_id,
        text,
        query,
        super::TERMINAL_SEARCH_MATCH_LIMIT,
    );
    matches
}

#[cfg(test)]
pub(super) fn terminal_search_matches_into(
    matches: &mut Vec<TerminalSearchMatch>,
    session_id: usize,
    text: &str,
    query: &str,
    match_limit: usize,
) {
    let Some(query) = normalize_terminal_search_query(query) else {
        return;
    };
    terminal_search_matches_with_normalized_query_into(
        matches,
        session_id,
        text,
        query.as_ref(),
        match_limit,
        0,
    );
}

pub(super) fn terminal_search_matches_with_normalized_query_into(
    matches: &mut Vec<TerminalSearchMatch>,
    session_id: usize,
    text: &str,
    query: &str,
    match_limit: usize,
    line_offset: usize,
) {
    if matches.len() >= match_limit {
        return;
    }

    let Some(query) = PreparedTerminalSearchQuery::new(query) else {
        return;
    };
    terminal_search_matches_with_prepared_query_into(
        matches,
        session_id,
        text,
        query,
        match_limit,
        line_offset,
    );
}

pub(super) fn terminal_search_matches_with_prepared_query_into(
    matches: &mut Vec<TerminalSearchMatch>,
    session_id: usize,
    text: &str,
    query: PreparedTerminalSearchQuery<'_>,
    match_limit: usize,
    line_offset: usize,
) {
    for (line_index, line) in text.lines().enumerate() {
        let line_index = line_offset.saturating_add(line_index);
        if !query.may_match_line(line) {
            continue;
        }
        let mut search_from = 0usize;
        let mut preview = None::<Arc<String>>;
        while let Some(match_start) = query.find_from(line, search_from) {
            let match_end = match_start + query.byte_len();
            matches.push(TerminalSearchMatch {
                session_id,
                line: line_index,
                start: match_start,
                end: match_end,
                preview: Arc::clone(preview.get_or_insert_with(|| terminal_search_preview(line))),
            });
            if matches.len() >= match_limit {
                return;
            }
            search_from = match_end.max(match_start.saturating_add(1));
        }
    }
}

#[cfg(test)]
pub(in crate::terminal) fn terminal_visible_search_spans(
    screen: &vt100::Screen,
    query: &str,
) -> Vec<TerminalVisibleSearchSpan> {
    let Some(query) = normalize_terminal_search_query(query) else {
        return Vec::new();
    };
    terminal_visible_search_spans_with_normalized_query(screen, query.as_ref(), None)
}

pub(in crate::terminal) fn terminal_visible_search_spans_with_normalized_query(
    screen: &vt100::Screen,
    query: &str,
    active: Option<(u16, (usize, usize))>,
) -> Vec<TerminalVisibleSearchSpan> {
    let Some(query) = PreparedTerminalSearchQuery::new(query) else {
        return Vec::new();
    };
    let (rows, cols) = screen.size();
    let mut spans = Vec::with_capacity(usize::from(rows));
    let mut line = String::with_capacity(usize::from(cols));
    let mut cell_spans = Vec::with_capacity(usize::from(cols));
    for row in 0..rows {
        terminal_visible_line_text_into(screen, row, cols, &mut line);
        if !query.may_match_line(&line) {
            continue;
        }
        let Some(mut match_start) = query.find_from(&line, 0) else {
            continue;
        };

        terminal_visible_line_cell_spans_into(screen, row, cols, &mut cell_spans);
        loop {
            let match_end = match_start + query.byte_len();
            if let Some((start_col, end_col)) =
                terminal_visible_match_cols(match_start, match_end, &cell_spans, cols)
            {
                let is_active = active.is_some_and(|(active_row, (active_start, active_end))| {
                    row == active_row && match_start == active_start && match_end == active_end
                });
                spans.push(TerminalVisibleSearchSpan {
                    row,
                    start_col,
                    end_col,
                    active: is_active,
                });
            }

            let search_from = match_end.max(match_start.saturating_add(1));
            let Some(next_match_start) = query.find_from(&line, search_from) else {
                break;
            };
            match_start = next_match_start;
        }
    }

    spans
}

pub(super) fn terminal_visible_match_cols(
    match_start: usize,
    match_end: usize,
    cell_spans: &[TerminalVisibleCellSpan],
    cols: u16,
) -> Option<(u16, u16)> {
    if match_start >= match_end {
        return None;
    }

    let start = terminal_visible_cell_span_for_byte(cell_spans, match_start)?;
    let end = terminal_visible_cell_span_for_byte(cell_spans, match_end.checked_sub(1)?)?;
    let end_col = end.end_col.max(start.start_col.saturating_add(1)).min(cols);
    Some((start.start_col, end_col))
}

pub(super) fn terminal_visible_cell_span_for_byte(
    cell_spans: &[TerminalVisibleCellSpan],
    byte_offset: usize,
) -> Option<TerminalVisibleCellSpan> {
    cell_spans
        .binary_search_by(|span| {
            if span.byte_end <= byte_offset {
                Ordering::Less
            } else if span.byte_start > byte_offset {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        })
        .ok()
        .and_then(|index| cell_spans.get(index).copied())
}

pub(super) fn terminal_visible_line_text_into(
    screen: &vt100::Screen,
    row: u16,
    cols: u16,
    line: &mut String,
) {
    line.clear();
    for col in 0..cols {
        let Some(cell) = screen.cell(row, col) else {
            continue;
        };
        if cell.is_wide_continuation() {
            continue;
        }

        let contents = cell.contents();
        if contents.is_empty() {
            line.push(' ');
        } else {
            line.push_str(contents);
        }
    }
}

pub(super) fn terminal_visible_line_cell_spans_into(
    screen: &vt100::Screen,
    row: u16,
    cols: u16,
    cell_spans: &mut Vec<TerminalVisibleCellSpan>,
) {
    cell_spans.clear();
    let mut byte_offset = 0usize;
    for col in 0..cols {
        let Some(cell) = screen.cell(row, col) else {
            continue;
        };
        if cell.is_wide_continuation() {
            continue;
        }

        let contents = cell.contents();
        let byte_start = byte_offset;
        byte_offset = byte_offset.saturating_add(if contents.is_empty() {
            1
        } else {
            contents.len()
        });
        let span = TerminalVisibleCellSpan {
            byte_start,
            byte_end: byte_offset,
            start_col: col,
            end_col: terminal_visible_cell_end_col(screen, row, col, cols),
        };
        cell_spans.push(span);
    }
}

pub(super) fn terminal_visible_cell_end_col(
    screen: &vt100::Screen,
    row: u16,
    start_col: u16,
    cols: u16,
) -> u16 {
    let mut end_col = start_col.saturating_add(1).min(cols);
    while end_col < cols {
        let Some(cell) = screen.cell(row, end_col) else {
            break;
        };
        if !cell.is_wide_continuation() {
            break;
        }
        end_col = end_col.saturating_add(1).min(cols);
    }
    end_col
}

pub(super) fn terminal_search_preview(line: &str) -> Arc<String> {
    let trimmed = line.trim();
    let mut chars = trimmed.chars();
    let mut preview = String::with_capacity(trimmed.len().min(TERMINAL_SEARCH_PREVIEW_MAX_BYTES));
    for ch in chars.by_ref().take(TERMINAL_SEARCH_PREVIEW_MAX_CHARS) {
        preview.push(ch);
    }
    if chars.next().is_some() {
        preview.push_str("...");
    }
    Arc::new(preview)
}
