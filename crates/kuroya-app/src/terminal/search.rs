use super::{
    TERMINAL_SEARCH_BUFFER_MAX_BYTES, TERMINAL_SEARCH_BUFFER_TRIM_TARGET_BYTES, TerminalPane,
    TerminalSearchCache, TerminalSearchCacheProgress, TerminalSearchCacheScope,
    TerminalSearchCacheSessionProgress, TerminalSearchMatch, TerminalSession,
};
#[path = "search/matches.rs"]
mod matches;
#[path = "search/plain_text.rs"]
mod plain_text;
#[path = "search/query.rs"]
mod query;

#[cfg(test)]
use matches::terminal_search_preview;
pub(super) use matches::terminal_visible_search_spans_with_normalized_query;
use matches::{
    PreparedTerminalSearchQuery, terminal_search_cached_match_matches_query,
    terminal_search_matches_with_normalized_query_into,
    terminal_search_matches_with_prepared_query_into,
};
#[cfg(test)]
pub(super) use matches::{terminal_search_matches, terminal_visible_search_spans};
#[cfg(test)]
use plain_text::TERMINAL_SEARCH_CONTROL_SEQUENCE_MAX_CHARS;
use plain_text::append_terminal_plain_text;
pub(super) use plain_text::{TerminalSearchAnsiState, terminal_plain_text};
use query::normalize_terminal_search_query;
#[cfg(test)]
use query::{TERMINAL_SEARCH_QUERY_MAX_CHARS, TERMINAL_SEARCH_QUERY_SCAN_MAX_CHARS};
#[cfg(test)]
use std::borrow::Cow;

const TERMINAL_SEARCH_MATCH_LIMIT: usize = 20_000;
const TERMINAL_SEARCH_FULL_SCAN_MAX_LINES: usize = 100_000;
const TERMINAL_SEARCH_FULL_SCAN_MAX_BYTES: usize = TERMINAL_SEARCH_BUFFER_TRIM_TARGET_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TerminalVisibleSearchSpan {
    pub(super) row: u16,
    pub(super) start_col: u16,
    pub(super) end_col: u16,

    pub(super) active: bool,
}

impl TerminalVisibleSearchSpan {
    pub(super) fn contains_cell(self, row: u16, col: u16) -> bool {
        self.row == row && col >= self.start_col && col < self.end_col
    }
}

impl TerminalPane {
    pub(crate) fn open_terminal_search(&mut self) {
        self.search_open = true;
        self.search_focus_on_show = true;
        self.search_match = 0;
    }

    pub(crate) fn toggle_terminal_search(&mut self) {
        if self.search_open {
            self.close_terminal_search();
        } else {
            self.open_terminal_search();
        }
    }

    pub(super) fn close_terminal_search(&mut self) {
        self.search_open = false;
        self.search_focus_on_show = false;
        self.search_match = 0;
        self.search_cache.clear();
    }

    pub(super) fn take_terminal_search_focus_request(&mut self) -> bool {
        std::mem::take(&mut self.search_focus_on_show)
    }

    pub(super) fn reset_terminal_search_cursor(&mut self) {
        self.search_match = self
            .first_terminal_search_match_index_for_active_session()
            .unwrap_or_default();
        self.reveal_active_terminal_search_match();
    }

    pub(crate) fn next_terminal_search_result(&mut self) {
        self.goto_terminal_search_result(1);
    }

    pub(crate) fn previous_terminal_search_result(&mut self) {
        self.goto_terminal_search_result(-1);
    }

    pub(crate) fn advance_terminal_search_result_if_open(&mut self, delta: i32) -> bool {
        if !self.visible || !self.search_open {
            return false;
        }
        self.advance_terminal_search(delta);
        true
    }

    fn goto_terminal_search_result(&mut self, delta: i32) {
        let was_open = self.search_open;
        if !was_open {
            self.open_terminal_search();
        }

        if was_open {
            self.advance_terminal_search(delta);
        } else {
            self.reset_terminal_search_cursor();
        }
    }

    pub(super) fn advance_terminal_search(&mut self, delta: i32) {
        let count = self.active_terminal_search_matches().len();
        if count == 0 {
            self.search_match = 0;
            return;
        }

        let current = self.search_match.min(count.saturating_sub(1)) as i64;
        self.search_match = (current + i64::from(delta)).rem_euclid(count as i64) as usize;
        self.reveal_active_terminal_search_match();
    }

    pub(super) fn active_terminal_search_matches(&mut self) -> &[TerminalSearchMatch] {
        if self.search_cache_is_current_for_raw_query() {
            self.clamp_terminal_search_match();
            return &self.search_cache.matches;
        }

        let Some(query) = normalize_terminal_search_query(&self.search_query) else {
            self.search_cache.clear();
            self.search_match = 0;
            return &self.search_cache.matches;
        };
        if self.search_cache_is_current_for_normalized_query(query.as_ref()) {
            self.clamp_terminal_search_match();
            return &self.search_cache.matches;
        }

        let query = query.into_owned();
        if self.terminal_search_uses_split_scope() {
            self.refresh_split_terminal_search_matches(query);
        } else {
            self.refresh_active_terminal_search_matches(query);
        }

        self.clamp_terminal_search_match();
        &self.search_cache.matches
    }

    pub(super) fn cached_terminal_search_query(&self) -> Option<&str> {
        (!self.search_cache.query.is_empty()).then_some(self.search_cache.query.as_str())
    }

    pub(super) fn prune_stale_terminal_search_cache(&mut self) {
        if self.search_cache.scope == TerminalSearchCacheScope::Empty {
            self.search_match = 0;
            return;
        }

        if self.terminal_search_cache_scope_is_live() && self.search_cache_matches_live_scope() {
            self.clamp_terminal_search_match();
        } else {
            self.search_cache.clear();
            self.search_match = 0;
        }
    }

    fn terminal_search_cache_scope_is_live(&self) -> bool {
        match &self.search_cache.scope {
            TerminalSearchCacheScope::Empty => true,
            TerminalSearchCacheScope::Single {
                session_id,
                generation: _,
            } => {
                if self.terminal_search_uses_split_scope() {
                    return false;
                }
                self.active_session_index()
                    .and_then(|index| self.sessions.get(index))
                    .is_some_and(|session| session.id == *session_id)
            }
            TerminalSearchCacheScope::Split { sessions } => {
                self.terminal_search_uses_split_scope()
                    && sessions.len() == self.sessions.len()
                    && sessions
                        .iter()
                        .zip(&self.sessions)
                        .all(|((session_id, _), session)| *session_id == session.id)
            }
        }
    }

    fn search_cache_is_current_for_raw_query(&self) -> bool {
        if self.search_cache.query.is_empty() || self.search_cache.query != self.search_query {
            return false;
        }

        self.search_cache_is_current_for_normalized_query(&self.search_query)
    }

    fn search_cache_is_current_for_normalized_query(&self, query: &str) -> bool {
        if self.search_cache.query.is_empty() || self.search_cache.query != query {
            return false;
        }

        if self.terminal_search_uses_split_scope() {
            return self
                .search_cache
                .matches_split_sessions(&self.sessions, query)
                && self.search_cache_matches_live_scope();
        }

        let Some(index) = self.active_session_index() else {
            return false;
        };
        let Some(session) = self.sessions.get(index) else {
            return false;
        };
        let scope = TerminalSearchCacheScope::Single {
            session_id: session.id,
            generation: session.search_generation,
        };
        self.search_cache.matches(&scope, query) && self.search_cache_matches_live_scope()
    }

    fn search_cache_matches_live_scope(&self) -> bool {
        if self.search_cache.matches.len() > TERMINAL_SEARCH_MATCH_LIMIT {
            return false;
        }

        match &self.search_cache.scope {
            TerminalSearchCacheScope::Empty => self.search_cache.matches.is_empty(),
            TerminalSearchCacheScope::Single { session_id, .. } => {
                let Some(query) = PreparedTerminalSearchQuery::new(&self.search_cache.query) else {
                    return false;
                };
                let Some(session) = self
                    .sessions
                    .iter()
                    .find(|session| session.id == *session_id)
                else {
                    return false;
                };
                terminal_search_live_match_run_end_for_session(
                    &self.search_cache.matches,
                    0,
                    session,
                    query,
                )
                .is_some_and(|index| index == self.search_cache.matches.len())
            }
            TerminalSearchCacheScope::Split { sessions } => {
                if sessions.len() != self.sessions.len()
                    || !sessions
                        .iter()
                        .zip(&self.sessions)
                        .all(|((session_id, _), session)| *session_id == session.id)
                {
                    return false;
                }
                if self.sessions.is_empty() {
                    return self.search_cache.matches.is_empty();
                }

                let Some(query) = PreparedTerminalSearchQuery::new(&self.search_cache.query) else {
                    return false;
                };
                let mut match_index = 0usize;
                for session in &self.sessions {
                    let Some(next_index) = terminal_search_live_match_run_end_for_session(
                        &self.search_cache.matches,
                        match_index,
                        session,
                        query,
                    ) else {
                        return false;
                    };
                    match_index = next_index;
                }
                match_index == self.search_cache.matches.len()
            }
        }
    }

    fn refresh_active_terminal_search_matches(&mut self, query: String) {
        let Some(index) = self.active_session_index() else {
            self.search_cache.clear();
            self.search_match = 0;
            return;
        };
        let Some(session) = self.sessions.get(index) else {
            self.search_cache.clear();
            self.search_match = 0;
            return;
        };

        let scope = TerminalSearchCacheScope::Single {
            session_id: session.id,
            generation: session.search_generation,
        };
        if self.search_cache.matches(&scope, &query) && self.search_cache_matches_live_scope() {
            return;
        }

        if self
            .search_cache
            .refresh_single_from_append(&scope, &query, session)
        {
            return;
        }

        let mut matches = self.search_cache.take_matches_for_refresh();
        let (search_buffer, line_offset) = terminal_search_limited_full_scan_text(
            &session.search_buffer,
            session.search_line_count,
        );
        terminal_search_matches_with_normalized_query_into(
            &mut matches,
            session.id,
            search_buffer,
            &query,
            TERMINAL_SEARCH_MATCH_LIMIT,
            line_offset,
        );
        self.search_cache.replace_with_progress(
            scope,
            query,
            matches,
            TerminalSearchCacheProgress::Single(TerminalSearchCacheSessionProgress::for_session(
                session,
            )),
        );
    }

    fn refresh_split_terminal_search_matches(&mut self, query: String) {
        if self.sessions.is_empty() {
            self.search_cache.clear();
            self.search_match = 0;
            return;
        }

        if self
            .search_cache
            .matches_split_sessions(&self.sessions, &query)
            && self.search_cache_matches_live_scope()
        {
            return;
        }

        let mut signature = Vec::with_capacity(self.sessions.len());
        signature.extend(
            self.sessions
                .iter()
                .map(|session| (session.id, session.search_generation)),
        );
        let scope = TerminalSearchCacheScope::Split {
            sessions: signature,
        };

        let mut matches = self.search_cache.take_matches_for_refresh();
        for session in &self.sessions {
            let (search_buffer, line_offset) = terminal_search_limited_full_scan_text(
                &session.search_buffer,
                session.search_line_count,
            );
            terminal_search_matches_with_normalized_query_into(
                &mut matches,
                session.id,
                search_buffer,
                &query,
                TERMINAL_SEARCH_MATCH_LIMIT,
                line_offset,
            );
            if matches.len() >= TERMINAL_SEARCH_MATCH_LIMIT {
                break;
            }
        }
        self.search_cache.replace(scope, query, matches);
    }

    pub(super) fn active_terminal_search_result_label(&self, count: usize) -> String {
        terminal_search_result_label(self.search_match, count)
    }

    fn reveal_active_terminal_search_match(&mut self) {
        let Some((session_id, line)) = self.active_terminal_search_match_location() else {
            return;
        };
        let Some(index) = self
            .sessions
            .iter()
            .position(|session| session.id == session_id)
        else {
            return;
        };
        if self.terminal_search_uses_split_scope() {
            self.set_active_session_without_focus(index);
        }
        if let Some(session) = self.sessions.get_mut(index) {
            session.reveal_search_line(line);
        }
    }

    fn active_terminal_search_match_location(&mut self) -> Option<(usize, usize)> {
        let search_match = self.search_match;
        let matches = self.active_terminal_search_matches();
        let selected = search_match.min(matches.len().saturating_sub(1));
        matches
            .get(selected)
            .map(|matched| (matched.session_id, matched.line))
    }

    fn first_terminal_search_match_index_for_active_session(&mut self) -> Option<usize> {
        if !self.terminal_search_uses_split_scope() {
            return Some(0);
        }

        let active_session_id = self
            .active_session_index()
            .and_then(|index| self.sessions.get(index))
            .map(|session| session.id)?;
        let matches = self.active_terminal_search_matches();
        matches
            .iter()
            .position(|matched| matched.session_id == active_session_id)
            .or_else(|| (!matches.is_empty()).then_some(0))
    }

    fn terminal_search_uses_split_scope(&self) -> bool {
        self.split_view && self.sessions.len() > 1
    }

    fn clamp_terminal_search_match(&mut self) {
        let count = self.search_cache.matches.len();
        self.search_match = if count == 0 {
            0
        } else {
            self.search_match.min(count - 1)
        };
    }
}

impl TerminalSession {
    pub(super) fn append_search_output(&mut self, chunk: &[u8]) {
        if !append_terminal_plain_text(
            &mut self.search_buffer,
            &mut self.search_line_count,
            &mut self.search_pending_carriage_return,
            &mut self.search_ansi_state,
            &mut self.search_utf8_tail,
            chunk,
        ) {
            return;
        }
        if trim_search_buffer(&mut self.search_buffer, &mut self.search_line_count) {
            self.mark_search_buffer_rebased();
        } else {
            self.mark_search_buffer_changed();
        }
    }

    pub(super) fn replace_search_buffer(&mut self, buffer: String) {
        self.reset_search_output_decoder();
        self.search_line_count = terminal_search_line_count(&buffer);
        self.search_buffer = buffer;
        self.mark_search_buffer_rebased();
    }

    pub(super) fn clear_search_buffer(&mut self) {
        self.reset_search_output_decoder();
        self.search_line_count = 0;
        if !self.search_buffer.is_empty() {
            self.search_buffer.clear();
            self.mark_search_buffer_rebased();
        }
    }

    pub(super) fn reset_search_output_decoder(&mut self) {
        self.search_pending_carriage_return = false;
        self.search_ansi_state = TerminalSearchAnsiState::default();
        self.search_utf8_tail.clear();
    }

    pub(super) fn mark_search_buffer_changed(&mut self) {
        self.search_generation = self.search_generation.wrapping_add(1);
    }

    fn mark_search_buffer_rebased(&mut self) {
        self.search_edit_generation = self.search_edit_generation.wrapping_add(1);
        self.mark_search_buffer_changed();
    }

    fn reveal_search_line(&mut self, line: usize) {
        let (rows, _) = self.parser.screen().size();
        let total_lines = self.search_line_count;
        let scrollback = terminal_search_scrollback_for_line(total_lines, usize::from(rows), line);
        self.parser.screen_mut().set_scrollback(scrollback);
    }
}

impl TerminalSearchCache {
    fn matches(&self, scope: &TerminalSearchCacheScope, query: &str) -> bool {
        &self.scope == scope && self.query == query
    }

    fn matches_split_sessions(&self, sessions: &[TerminalSession], query: &str) -> bool {
        if self.query != query {
            return false;
        }

        let TerminalSearchCacheScope::Split { sessions: cached } = &self.scope else {
            return false;
        };
        cached.len() == sessions.len()
            && cached
                .iter()
                .zip(sessions)
                .all(|((id, generation), session)| {
                    *id == session.id && *generation == session.search_generation
                })
    }

    fn refresh_single_from_append(
        &mut self,
        scope: &TerminalSearchCacheScope,
        query: &str,
        session: &TerminalSession,
    ) -> bool {
        if self.query != query {
            return false;
        }

        let TerminalSearchCacheScope::Single { session_id, .. } = &self.scope else {
            return false;
        };
        if *session_id != session.id {
            return false;
        }
        if self.matches.len() > TERMINAL_SEARCH_MATCH_LIMIT {
            return false;
        }

        let Some(query) = PreparedTerminalSearchQuery::new(query) else {
            return false;
        };
        let TerminalSearchCacheProgress::Single(progress) = self.progress else {
            return false;
        };
        if progress.session_id != session.id
            || progress.edit_generation != session.search_edit_generation
            || progress.resume_byte > session.search_buffer.len()
            || progress.resume_line > session.search_line_count
            || !session.search_buffer.is_char_boundary(progress.resume_byte)
        {
            return false;
        }
        if terminal_search_live_match_run_end_for_session(&self.matches, 0, session, query)
            .is_none_or(|index| index != self.matches.len())
        {
            return false;
        }

        retain_terminal_search_matches_before_resume_line(
            &mut self.matches,
            session.id,
            progress.resume_line,
        );
        if self.matches.len() < TERMINAL_SEARCH_MATCH_LIMIT {
            terminal_search_matches_with_prepared_query_into(
                &mut self.matches,
                session.id,
                &session.search_buffer[progress.resume_byte..],
                query,
                TERMINAL_SEARCH_MATCH_LIMIT,
                progress.resume_line,
            );
        }

        self.scope = scope.clone();
        self.progress = TerminalSearchCacheProgress::Single(
            TerminalSearchCacheSessionProgress::for_session(session),
        );
        true
    }

    fn replace(
        &mut self,
        scope: TerminalSearchCacheScope,
        query: String,
        matches: Vec<TerminalSearchMatch>,
    ) {
        self.replace_with_progress(scope, query, matches, TerminalSearchCacheProgress::Empty);
    }

    fn replace_with_progress(
        &mut self,
        scope: TerminalSearchCacheScope,
        query: String,
        matches: Vec<TerminalSearchMatch>,
        progress: TerminalSearchCacheProgress,
    ) {
        self.scope = scope;
        self.query = query;
        self.matches = matches;
        self.progress = progress;
    }

    fn take_matches_for_refresh(&mut self) -> Vec<TerminalSearchMatch> {
        let mut matches = std::mem::take(&mut self.matches);
        matches.clear();
        matches
    }

    fn clear(&mut self) {
        self.scope = TerminalSearchCacheScope::Empty;
        self.query.clear();
        self.matches.clear();
        self.progress = TerminalSearchCacheProgress::Empty;
    }
}

fn retain_terminal_search_matches_before_resume_line(
    matches: &mut Vec<TerminalSearchMatch>,
    session_id: usize,
    resume_line: usize,
) {
    let mut retaining_stable_prefix = true;
    let mut previous = None::<(usize, usize)>;
    matches.retain(|matched| {
        if !retaining_stable_prefix {
            return false;
        }

        let is_stable = matched.session_id == session_id
            && matched.line < resume_line
            && matched.start < matched.end
            && previous.is_none_or(|(line, end)| {
                matched.line > line || (matched.line == line && matched.start >= end)
            });
        if is_stable {
            previous = Some((matched.line, matched.end));
        } else {
            retaining_stable_prefix = false;
        }
        is_stable
    });
}

fn terminal_search_live_match_run_end_for_session(
    matches: &[TerminalSearchMatch],
    mut index: usize,
    session: &TerminalSession,
    query: PreparedTerminalSearchQuery<'_>,
) -> Option<usize> {
    let mut previous = None::<(usize, usize)>;
    let mut previous_next_start = None::<usize>;
    let mut cursor_line = 0usize;
    let mut cursor_byte = 0usize;
    let hit_match_limit = matches.len() >= TERMINAL_SEARCH_MATCH_LIMIT;
    while let Some(matched) = matches.get(index) {
        if matched.session_id != session.id {
            if previous_next_start.is_some() {
                return None;
            }
            break;
        }
        if matched.start >= matched.end || matched.line >= session.search_line_count {
            return None;
        }
        if previous.is_some_and(|(line, end)| {
            matched.line < line || (matched.line == line && matched.start < end)
        }) {
            return None;
        }
        let line = terminal_search_cached_match_line(
            &session.search_buffer,
            matched.line,
            &mut cursor_line,
            &mut cursor_byte,
        )?;
        if !terminal_search_cached_match_matches_query(matched, line, query) {
            return None;
        }
        let expected_start = if previous.is_some_and(|(line, _)| line == matched.line) {
            previous_next_start
        } else {
            query.find_from(line, 0)
        };
        if expected_start != Some(matched.start) {
            return None;
        }
        previous_next_start =
            query.find_from(line, matched.end.max(matched.start.saturating_add(1)));
        previous = Some((matched.line, matched.end));
        index += 1;
    }

    if previous_next_start.is_some() && !hit_match_limit {
        return None;
    }

    Some(index)
}

fn terminal_search_cached_match_line<'a>(
    buffer: &'a str,
    line: usize,
    cursor_line: &mut usize,
    cursor_byte: &mut usize,
) -> Option<&'a str> {
    if line < *cursor_line {
        return None;
    }

    while *cursor_line < line {
        let newline_offset = buffer.get(*cursor_byte..)?.find('\n')?;
        *cursor_byte = (*cursor_byte).saturating_add(newline_offset + 1);
        *cursor_line = (*cursor_line).saturating_add(1);
    }

    let remaining = buffer.get(*cursor_byte..)?;
    let line_end = remaining
        .find('\n')
        .map_or(buffer.len(), |offset| (*cursor_byte).saturating_add(offset));
    buffer.get(*cursor_byte..line_end)
}

impl TerminalSearchCacheSessionProgress {
    fn for_session(session: &TerminalSession) -> Self {
        let (resume_byte, resume_line) = terminal_search_resume_point_from_line_count(
            &session.search_buffer,
            session.search_line_count,
        );
        Self {
            session_id: session.id,
            edit_generation: session.search_edit_generation,
            resume_byte,
            resume_line,
        }
    }
}

fn terminal_search_result_label(active: usize, count: usize) -> String {
    if count == 0 {
        "No results".to_owned()
    } else {
        let active = active.min(count - 1) + 1;
        let mut label = String::with_capacity(16);
        use std::fmt::Write as _;
        let _ = write!(&mut label, "{active}/{count} results");
        label
    }
}

fn terminal_search_scrollback_for_line(
    total_lines: usize,
    visible_rows: usize,
    line: usize,
) -> usize {
    if total_lines == 0 || visible_rows == 0 {
        return 0;
    }

    let latest_top_line = total_lines.saturating_sub(visible_rows);
    let centered_top_line = line.saturating_sub(visible_rows / 2).min(latest_top_line);
    latest_top_line.saturating_sub(centered_top_line)
}

fn trim_search_buffer(buffer: &mut String, line_count: &mut usize) -> bool {
    if buffer.len() <= TERMINAL_SEARCH_BUFFER_MAX_BYTES {
        return false;
    }

    let mut remove_until = buffer
        .len()
        .saturating_sub(TERMINAL_SEARCH_BUFFER_TRIM_TARGET_BYTES);
    while !buffer.is_char_boundary(remove_until) {
        remove_until += 1;
    }
    if let Some(newline_offset) = buffer[remove_until..].find('\n') {
        remove_until += newline_offset + 1;
    }
    let removed_lines = terminal_search_newline_count(&buffer[..remove_until]);
    buffer.drain(..remove_until);
    *line_count = line_count.saturating_sub(removed_lines);
    if buffer.is_empty() {
        *line_count = 0;
    }
    true
}

fn terminal_search_line_count(buffer: &str) -> usize {
    if buffer.is_empty() {
        0
    } else {
        terminal_search_newline_count(buffer) + usize::from(!buffer.ends_with('\n'))
    }
}

fn terminal_search_newline_count(buffer: &str) -> usize {
    buffer
        .as_bytes()
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
}

fn terminal_search_resume_point_from_line_count(buffer: &str, line_count: usize) -> (usize, usize) {
    let resume_byte = buffer.rfind('\n').map_or(0, |index| index + 1);
    let resume_line = if buffer.is_empty() {
        0
    } else if buffer.ends_with('\n') {
        line_count
    } else {
        line_count.saturating_sub(1)
    };
    (resume_byte, resume_line)
}

fn terminal_search_limited_full_scan_text(buffer: &str, line_count: usize) -> (&str, usize) {
    let (buffer, line_offset) =
        terminal_search_limited_scan_text(buffer, line_count, TERMINAL_SEARCH_FULL_SCAN_MAX_LINES);
    terminal_search_limited_scan_bytes(buffer, line_offset, TERMINAL_SEARCH_FULL_SCAN_MAX_BYTES)
}

fn terminal_search_limited_scan_text(
    buffer: &str,
    line_count: usize,
    line_limit: usize,
) -> (&str, usize) {
    if line_limit == 0 || buffer.is_empty() || line_count <= line_limit {
        return (buffer, 0);
    }

    let target_offset = line_count - line_limit;
    let mut skipped_lines = 0usize;
    for (byte, ch) in buffer.as_bytes().iter().enumerate() {
        if *ch != b'\n' {
            continue;
        }
        skipped_lines += 1;
        if skipped_lines == target_offset {
            return (&buffer[byte + 1..], skipped_lines);
        }
    }

    (buffer, 0)
}

fn terminal_search_limited_scan_bytes(
    buffer: &str,
    line_offset: usize,
    byte_limit: usize,
) -> (&str, usize) {
    if byte_limit == 0 || buffer.is_empty() || buffer.len() <= byte_limit {
        return (buffer, line_offset);
    }

    let mut start = buffer.len().saturating_sub(byte_limit);
    while !buffer.is_char_boundary(start) {
        start += 1;
    }

    let remove_until = if start == 0 || buffer.as_bytes()[start.saturating_sub(1)] == b'\n' {
        start
    } else {
        let Some(newline_offset) = buffer[start..].find('\n') else {
            return (
                "",
                line_offset.saturating_add(terminal_search_newline_count(buffer)),
            );
        };
        start + newline_offset + 1
    };

    let removed_lines = terminal_search_newline_count(&buffer[..remove_until]);
    (
        &buffer[remove_until..],
        line_offset.saturating_add(removed_lines),
    )
}

#[cfg(test)]
pub(super) fn trim_terminal_search_buffer_for_test(buffer: &mut String) {
    let mut line_count = terminal_search_line_count(buffer);
    trim_search_buffer(buffer, &mut line_count);
}

#[cfg(test)]
pub(super) fn terminal_search_result_label_for_test(active: usize, count: usize) -> String {
    terminal_search_result_label(active, count)
}

#[cfg(test)]
pub(super) fn terminal_search_scrollback_for_line_for_test(
    total_lines: usize,
    visible_rows: usize,
    line: usize,
) -> usize {
    terminal_search_scrollback_for_line(total_lines, visible_rows, line)
}

#[cfg(test)]
pub(super) fn terminal_search_resume_point_from_line_count_for_test(
    buffer: &str,
    line_count: usize,
) -> (usize, usize) {
    terminal_search_resume_point_from_line_count(buffer, line_count)
}

#[cfg(test)]
pub(super) fn terminal_search_match_limit_for_test() -> usize {
    TERMINAL_SEARCH_MATCH_LIMIT
}

#[cfg(test)]
pub(super) fn terminal_search_query_max_chars_for_test() -> usize {
    TERMINAL_SEARCH_QUERY_MAX_CHARS
}

#[cfg(test)]
pub(super) fn terminal_search_full_scan_max_bytes_for_test() -> usize {
    TERMINAL_SEARCH_FULL_SCAN_MAX_BYTES
}

#[cfg(test)]
pub(super) fn terminal_search_control_sequence_max_chars_for_test() -> usize {
    TERMINAL_SEARCH_CONTROL_SEQUENCE_MAX_CHARS
}

#[cfg(test)]
pub(super) fn normalized_terminal_search_query_for_test(query: &str) -> Option<String> {
    normalize_terminal_search_query(query).map(Cow::into_owned)
}

#[cfg(test)]
mod terminal_search_scan_tests {
    use super::*;

    #[test]
    fn terminal_search_skips_short_lines_without_shifting_match_offsets() {
        let matches = terminal_search_matches(4, "al\nalpha\nbe\nALPHA\n", "alpha");

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].session_id, 4);
        assert_eq!(matches[0].line, 1);
        assert_eq!(matches[0].start, 0);
        assert_eq!(matches[0].end, 5);
        assert_eq!(matches[1].line, 3);
        assert_eq!(matches[1].start, 0);
        assert_eq!(matches[1].end, 5);
    }

    #[test]
    fn terminal_search_line_length_guard_keeps_equal_byte_length_candidates() {
        let matches = terminal_search_matches(8, "x\n\u{00e9}\n", "\u{00e9}");

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].session_id, 8);
        assert_eq!(matches[0].line, 1);
        assert_eq!(matches[0].start, 0);
        assert_eq!(matches[0].end, "\u{00e9}".len());
    }

    #[test]
    fn terminal_search_prepared_query_matches_unicode_with_ascii_case_folding() {
        let query = "r\u{00e9}sum\u{00e9}";
        let matches = terminal_search_matches(
            9,
            "R\u{00e9}sum\u{00e9}\nRESUME\nr\u{00e9}sum\u{00e9}\n",
            query,
        );

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].line, 0);
        assert_eq!(matches[0].start, 0);
        assert_eq!(matches[0].end, query.len());
        assert_eq!(matches[1].line, 2);
        assert_eq!(matches[1].start, 0);
        assert_eq!(matches[1].end, query.len());
    }

    #[test]
    fn terminal_search_normalized_scan_ignores_empty_prepared_query() {
        let mut matches = Vec::new();
        terminal_search_matches_with_normalized_query_into(
            &mut matches,
            1,
            "alpha\nalpha\n",
            "",
            10,
            0,
        );

        assert!(matches.is_empty());
    }

    #[test]
    fn terminal_search_normalized_scan_honors_zero_match_limit() {
        let mut matches = Vec::new();
        terminal_search_matches_with_normalized_query_into(
            &mut matches,
            1,
            "alpha\nalpha\n",
            "alpha",
            0,
            0,
        );

        assert!(matches.is_empty());
    }

    #[test]
    fn terminal_search_limited_scan_uses_recent_lines_with_line_offsets() {
        let (text, line_offset) =
            terminal_search_limited_scan_text("old\nrecent one\nrecent two\n", 3, 2);

        assert_eq!(text, "recent one\nrecent two\n");
        assert_eq!(line_offset, 1);

        let mut matches = Vec::new();
        terminal_search_matches_with_normalized_query_into(
            &mut matches,
            7,
            text,
            "recent",
            TERMINAL_SEARCH_MATCH_LIMIT,
            line_offset,
        );

        let lines = matches
            .iter()
            .map(|matched| matched.line)
            .collect::<Vec<_>>();
        assert_eq!(lines, vec![1, 2]);
    }

    #[test]
    fn terminal_search_limited_scan_falls_back_when_line_count_is_stale() {
        let (text, line_offset) = terminal_search_limited_scan_text("only\none\n", 99, 2);

        assert_eq!(text, "only\none\n");
        assert_eq!(line_offset, 0);
    }

    #[test]
    fn terminal_search_limited_full_scan_caps_recent_bytes_at_line_boundary() {
        let old_line = "old ".to_owned() + &"x".repeat(TERMINAL_SEARCH_FULL_SCAN_MAX_BYTES);
        let buffer = format!("{old_line}\nrecent one\nrecent two\n");

        let (text, line_offset) =
            terminal_search_limited_full_scan_text(&buffer, terminal_search_line_count(&buffer));

        assert_eq!(text, "recent one\nrecent two\n");
        assert_eq!(line_offset, 1);
    }

    #[test]
    fn terminal_search_limited_full_scan_keeps_line_boundary_byte_window() {
        let old_line = "old ".to_owned() + &"x".repeat(32);
        let recent_line =
            "recent ".to_owned() + &"y".repeat(TERMINAL_SEARCH_FULL_SCAN_MAX_BYTES - 8);
        let buffer = format!("{old_line}\n{recent_line}\n");

        let (text, line_offset) =
            terminal_search_limited_full_scan_text(&buffer, terminal_search_line_count(&buffer));

        let expected = format!("{recent_line}\n");
        assert_eq!(text, expected.as_str());
        assert_eq!(line_offset, 1);
    }

    #[test]
    fn terminal_search_limited_full_scan_drops_single_line_over_byte_cap() {
        let buffer = "x".repeat(TERMINAL_SEARCH_FULL_SCAN_MAX_BYTES + 1);

        let (text, line_offset) =
            terminal_search_limited_full_scan_text(&buffer, terminal_search_line_count(&buffer));

        assert_eq!(text, "");
        assert_eq!(line_offset, 0);
    }
}

#[cfg(test)]
mod terminal_search_plain_text_tests {
    use super::*;

    #[test]
    fn terminal_search_plain_text_ignores_non_sequence_raw_c1_controls() {
        let text = terminal_plain_text(&[b'a', 0x80, b'b', 0x81, b'c', 0x99, b'd']);

        assert_eq!(text, "abcd");
    }

    #[test]
    fn terminal_search_plain_text_replaces_invalid_non_c1_bytes_and_flushes_tail() {
        let text = terminal_plain_text(&[b'a', 0xff, b'b', 0xe2, 0x82]);

        assert_eq!(text, "a\u{fffd}b\u{fffd}");
    }
}

#[cfg(test)]
mod terminal_search_cache_tests {
    use super::*;

    fn match_on_line(session_id: usize, line: usize) -> TerminalSearchMatch {
        match_at(session_id, line, 0)
    }

    fn match_at(session_id: usize, line: usize, start: usize) -> TerminalSearchMatch {
        TerminalSearchMatch {
            session_id,
            line,
            start,
            end: start + 1,
            preview: std::sync::Arc::new(String::new()),
        }
    }

    #[test]
    fn terminal_search_cache_retains_only_stable_prefix_before_resume_line() {
        let mut matches = vec![
            match_on_line(7, 0),
            match_on_line(7, 1),
            match_on_line(7, 2),
            match_on_line(8, 2),
            match_on_line(7, 3),
        ];

        retain_terminal_search_matches_before_resume_line(&mut matches, 7, 2);

        let retained = matches
            .iter()
            .map(|matched| (matched.session_id, matched.line))
            .collect::<Vec<_>>();
        assert_eq!(retained, vec![(7, 0), (7, 1)]);
    }

    #[test]
    fn terminal_search_cache_retains_only_ordered_stable_prefix_before_resume_line() {
        let mut matches = vec![
            match_at(7, 0, 0),
            match_at(7, 1, 4),
            match_at(7, 1, 4),
            match_at(7, 1, 9),
            match_at(7, 2, 0),
        ];

        retain_terminal_search_matches_before_resume_line(&mut matches, 7, 2);

        let retained = matches
            .iter()
            .map(|matched| (matched.session_id, matched.line, matched.start))
            .collect::<Vec<_>>();
        assert_eq!(retained, vec![(7, 0, 0), (7, 1, 4)]);
    }

    #[test]
    fn terminal_search_cache_retains_only_non_overlapping_stable_prefix() {
        let mut matches = vec![
            TerminalSearchMatch {
                session_id: 7,
                line: 0,
                start: 0,
                end: 3,
                preview: std::sync::Arc::new(String::new()),
            },
            TerminalSearchMatch {
                session_id: 7,
                line: 0,
                start: 2,
                end: 5,
                preview: std::sync::Arc::new(String::new()),
            },
            TerminalSearchMatch {
                session_id: 7,
                line: 0,
                start: 5,
                end: 8,
                preview: std::sync::Arc::new(String::new()),
            },
        ];

        retain_terminal_search_matches_before_resume_line(&mut matches, 7, 1);

        let retained = matches
            .iter()
            .map(|matched| matched.start..matched.end)
            .collect::<Vec<_>>();
        assert_eq!(retained, vec![0..3]);
    }

    #[test]
    fn terminal_search_append_cache_rejects_over_limit_cached_matches() {
        let size = portable_pty::PtySize {
            rows: 2,
            cols: 4,
            pixel_width: 0,
            pixel_height: 0,
        };
        let mut session = TerminalSession::new(7, size, 128);
        let line_count = TERMINAL_SEARCH_MATCH_LIMIT + 1;
        session.replace_search_buffer("a\n".repeat(line_count));
        let progress = TerminalSearchCacheSessionProgress::for_session(&session);
        let preview = terminal_search_preview("a");
        let matches = (0..line_count)
            .map(|line| TerminalSearchMatch {
                session_id: 7,
                line,
                start: 0,
                end: 1,
                preview: std::sync::Arc::clone(&preview),
            })
            .collect::<Vec<_>>();
        let mut cache = TerminalSearchCache {
            scope: TerminalSearchCacheScope::Single {
                session_id: 7,
                generation: session.search_generation,
            },
            query: "a".to_owned(),
            matches,
            progress: TerminalSearchCacheProgress::Single(progress),
        };

        session.append_search_output(b"a\n");
        let current_scope = TerminalSearchCacheScope::Single {
            session_id: 7,
            generation: session.search_generation,
        };

        assert!(!cache.refresh_single_from_append(&current_scope, "a", &session));
    }

    #[test]
    fn terminal_search_cached_match_rejects_stale_preview() {
        let query = PreparedTerminalSearchQuery::new("alpha").expect("query");
        let mut matched = TerminalSearchMatch {
            session_id: 7,
            line: 0,
            start: 0,
            end: 5,
            preview: std::sync::Arc::new("stale".to_owned()),
        };

        assert!(!terminal_search_cached_match_matches_query(
            &matched,
            "alpha current",
            query
        ));

        matched.preview = terminal_search_preview("alpha current");
        assert!(terminal_search_cached_match_matches_query(
            &matched,
            "alpha current",
            query
        ));
    }
}

#[cfg(test)]
mod search_query_normalization_tests {
    use super::*;

    #[test]
    fn terminal_search_query_normalization_borrows_already_normalized_query() {
        let query = "alpha  beta";
        let normalized = normalize_terminal_search_query(query).expect("normalized query");
        let Cow::Borrowed(borrowed) = normalized else {
            panic!("expected borrowed query");
        };

        assert_eq!(borrowed, query);
        assert_eq!(borrowed.as_ptr(), query.as_ptr());
    }

    #[test]
    fn terminal_search_query_normalization_borrows_clean_trimmed_query_for_cache_comparison() {
        let query = "  alpha  beta  ";
        let normalized = normalize_terminal_search_query(query).expect("normalized query");
        let Cow::Borrowed(borrowed) = normalized else {
            panic!("expected borrowed query");
        };
        assert_eq!(borrowed, "alpha  beta");
        assert_eq!(borrowed.as_ptr(), query["  ".len()..].as_ptr());

        let scope = TerminalSearchCacheScope::Single {
            session_id: 7,
            generation: 11,
        };
        let mut cache = TerminalSearchCache::default();
        cache.replace(scope.clone(), "alpha  beta".to_owned(), Vec::new());

        assert!(cache.matches(&scope, borrowed));
    }

    #[test]
    fn terminal_search_query_normalization_owns_unsafe_whitespace_regression() {
        let query = "alpha\t \n\u{00a0} beta\r gamma";
        let normalized = normalize_terminal_search_query(query).expect("normalized query");

        assert_eq!(normalized.as_ref(), "alpha beta gamma");
        assert!(matches!(normalized, Cow::Owned(_)));
        assert!(normalize_terminal_search_query("\t\u{00a0}\r\n").is_none());
    }

    #[test]
    fn terminal_search_query_normalization_owns_leading_sanitized_query() {
        let normalized =
            normalize_terminal_search_query("\t \u{202e} alpha").expect("normalized query");

        assert_eq!(normalized.as_ref(), "alpha");
        assert!(matches!(normalized, Cow::Owned(_)));
    }

    #[test]
    fn terminal_search_query_normalization_owns_bidi_regression() {
        let query = "\u{202e}alpha\u{2066} beta\u{2069}\u{200f}";
        let normalized = normalize_terminal_search_query(query).expect("normalized query");

        assert_eq!(normalized.as_ref(), "alpha beta");
        assert!(matches!(normalized, Cow::Owned(_)));
    }

    #[test]
    fn terminal_search_query_normalization_matches_expected_values() {
        for (query, expected) in [
            ("  alpha  beta  ", Some("alpha  beta")),
            ("alpha\t \n\u{00a0} beta\r gamma", Some("alpha beta gamma")),
            (
                "\u{202e}alpha\u{2066} beta\u{2069}\u{200f}",
                Some("alpha beta"),
            ),
            ("alpha  \t  beta", Some("alpha  beta")),
            ("alpha\t  beta", Some("alpha beta")),
            ("\t\u{00a0}\r\n", None),
        ] {
            let normalized = normalize_terminal_search_query(query);
            assert_eq!(normalized.as_deref(), expected, "query={query:?}");
        }
    }

    #[test]
    fn terminal_search_cache_reuses_once_normalized_query() {
        let scope = TerminalSearchCacheScope::Single {
            session_id: 7,
            generation: 11,
        };
        let mut cache = TerminalSearchCache::default();
        cache.replace(scope.clone(), "alpha beta".to_owned(), Vec::new());
        let query =
            normalize_terminal_search_query(" \talpha\u{202e} beta\u{2069}\n").expect("query");

        assert_eq!(query.as_ref(), "alpha beta");
        assert!(cache.matches(&scope, query.as_ref()));
    }

    #[test]
    fn terminal_search_cache_rejects_stale_scope_for_once_normalized_query() {
        let scope = TerminalSearchCacheScope::Single {
            session_id: 7,
            generation: 11,
        };
        let stale_scope = TerminalSearchCacheScope::Single {
            session_id: 7,
            generation: 12,
        };
        let mut cache = TerminalSearchCache::default();
        cache.replace(scope, "alpha beta".to_owned(), Vec::new());
        let query =
            normalize_terminal_search_query(" \talpha\u{202e} beta\u{2069}\n").expect("query");

        assert_eq!(query.as_ref(), "alpha beta");
        assert!(!cache.matches(&stale_scope, query.as_ref()));
    }

    #[test]
    fn terminal_search_query_normalization_stops_after_scan_bound() {
        let query = format!(
            "{}alpha",
            " ".repeat(TERMINAL_SEARCH_QUERY_SCAN_MAX_CHARS + 1)
        );

        assert!(normalize_terminal_search_query(&query).is_none());
    }

    #[test]
    fn terminal_search_query_normalization_accepts_exact_char_bound() {
        let query = "a".repeat(TERMINAL_SEARCH_QUERY_MAX_CHARS);

        assert_eq!(
            normalize_terminal_search_query(&query).as_deref(),
            Some(query.as_str())
        );
    }

    #[test]
    fn terminal_search_query_normalization_rejects_char_bound_overflow() {
        let query = "a".repeat(TERMINAL_SEARCH_QUERY_MAX_CHARS + 1);

        assert!(normalize_terminal_search_query(&query).is_none());
    }
}
