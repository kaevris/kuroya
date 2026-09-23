use crate::{KuroyaApp, large_file_mode::buffer_uses_large_file_mode, workspace_state::PaneId};
use kuroya_core::{BufferId, EditorFindAutoFindInSelection, TextBuffer};
use std::{
    ops::Range,
    sync::{LazyLock, Mutex},
};

mod replace;

pub(crate) const LARGE_FILE_FIND_STATUS: &str = "Find is disabled in large file mode";
pub(crate) const BUFFER_FIND_QUERY_TOO_LONG_STATUS: &str = "Find query is too long";
pub(crate) const BUFFER_FIND_MAX_QUERY_BYTES: usize = 16 * 1024;
const BUFFER_FIND_MAX_MATCHES: usize = 5_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BufferFindScope {
    pub(crate) buffer_id: BufferId,
    pub(crate) range: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BufferFindCacheKey {
    pub(crate) buffer_id: BufferId,
    pub(crate) version: u64,
    pub(crate) len_chars: usize,
    pub(crate) query: String,
    pub(crate) case_sensitive: bool,
    pub(crate) whole_word: bool,
    pub(crate) regex: bool,
    pub(crate) scope: Option<Range<usize>>,
}

impl BufferFindCacheKey {
    #[cfg(test)]
    fn for_buffer(
        buffer: &TextBuffer,
        query: &str,
        case_sensitive: bool,
        whole_word: bool,
        regex: bool,
        scope: Option<Range<usize>>,
    ) -> Self {
        BufferFindCacheLookupKey::for_buffer(
            buffer,
            query,
            case_sensitive,
            whole_word,
            regex,
            scope,
        )
        .to_cache_key()
    }
}

impl PartialEq<BufferFindCacheLookupKey<'_>> for BufferFindCacheKey {
    fn eq(&self, key: &BufferFindCacheLookupKey<'_>) -> bool {
        self.buffer_id == key.buffer_id
            && self.version == key.version
            && self.len_chars == key.len_chars
            && self.query.as_str() == key.query
            && self.case_sensitive == key.case_sensitive
            && self.whole_word == key.whole_word
            && self.regex == key.regex
            && self.scope.as_ref() == key.scope.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BufferFindCacheLookupKey<'a> {
    buffer_id: BufferId,
    version: u64,
    len_chars: usize,
    query: &'a str,
    case_sensitive: bool,
    whole_word: bool,
    regex: bool,
    scope: Option<Range<usize>>,
}

impl<'a> BufferFindCacheLookupKey<'a> {
    fn for_buffer(
        buffer: &TextBuffer,
        query: &'a str,
        case_sensitive: bool,
        whole_word: bool,
        regex: bool,
        scope: Option<Range<usize>>,
    ) -> Self {
        let len_chars = buffer.len_chars();
        Self {
            buffer_id: buffer.id(),
            version: buffer_find_cache_version(
                buffer.version(),
                whole_word,
                buffer.word_separators(),
            ),
            len_chars,
            query,
            case_sensitive,
            whole_word,
            regex,
            scope: normalize_buffer_find_scope(scope, len_chars),
        }
    }

    fn to_cache_key(&self) -> BufferFindCacheKey {
        BufferFindCacheKey {
            buffer_id: self.buffer_id,
            version: self.version,
            len_chars: self.len_chars,
            query: self.query.to_owned(),
            case_sensitive: self.case_sensitive,
            whole_word: self.whole_word,
            regex: self.regex,
            scope: self.scope.clone(),
        }
    }
}

pub(crate) const BUFFER_FIND_CACHE_CAPACITY: usize = 8;

#[derive(Clone, Debug, Default)]
pub(crate) struct BufferFindCache {
    entries: Vec<(BufferFindCacheKey, Vec<Range<usize>>)>,
}

impl BufferFindCache {
    #[cfg(test)]
    fn get(&self, key: &BufferFindCacheKey) -> Option<Vec<Range<usize>>> {
        self.matches_for_key(key).map(<[_]>::to_vec)
    }

    #[cfg(test)]
    fn matches_for_key(&self, key: &BufferFindCacheKey) -> Option<&[Range<usize>]> {
        self.entries
            .iter()
            .find(|(stored, _)| stored == key)
            .map(|(_, matches)| matches.as_slice())
    }

    #[cfg(test)]
    fn matches_key(&self, key: &BufferFindCacheKey) -> bool {
        self.entries.iter().any(|(stored, _)| stored == key)
    }

    fn matches_for_lookup_key(
        &self,
        key: &BufferFindCacheLookupKey<'_>,
    ) -> Option<&[Range<usize>]> {
        self.entries
            .iter()
            .find(|(stored, _)| stored.eq(key))
            .map(|(_, matches)| matches.as_slice())
    }

    fn touch_lookup_key(&mut self, key: &BufferFindCacheLookupKey<'_>) -> bool {
        let Some(index) = self.entries.iter().position(|(stored, _)| stored.eq(key)) else {
            return false;
        };
        let entry = self.entries.remove(index);
        self.entries.push(entry);
        true
    }

    fn store(&mut self, key: BufferFindCacheKey, matches: Vec<Range<usize>>) -> &[Range<usize>] {
        if let Some(index) = self.entries.iter().position(|(stored, _)| stored == &key) {
            self.entries.remove(index);
        }
        if self.entries.len() >= BUFFER_FIND_CACHE_CAPACITY {
            self.entries.remove(0);
        }
        self.entries.push((key, matches));
        let index = self.entries.len() - 1;
        self.entries[index].1.as_slice()
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(crate) fn clear_for_buffer(&mut self, buffer_id: BufferId) {
        self.entries.retain(|(key, _)| key.buffer_id != buffer_id);
    }

    #[cfg(test)]
    pub(crate) fn cached_buffer_ids_for_test(&self) -> Vec<BufferId> {
        let mut buffer_ids: Vec<BufferId> =
            self.entries.iter().map(|(key, _)| key.buffer_id).collect();
        buffer_ids.sort_unstable();
        buffer_ids.dedup();
        buffer_ids
    }

    #[cfg(test)]
    fn len_for_test(&self) -> usize {
        self.entries.len()
    }
}

const MAX_PANE_ACTIVE_FIND_MATCH_ENTRIES: usize = 128;

type PaneActiveFindMatchEntries = Vec<((PaneId, BufferId), usize)>;

fn pane_active_find_match_entries() -> &'static Mutex<PaneActiveFindMatchEntries> {
    static PANE_ACTIVE_FIND_MATCHES: LazyLock<Mutex<PaneActiveFindMatchEntries>> =
        LazyLock::new(|| Mutex::new(Vec::new()));
    &PANE_ACTIVE_FIND_MATCHES
}

fn store_pane_active_find_match(
    entries: &mut PaneActiveFindMatchEntries,
    pane_id: PaneId,
    buffer_id: BufferId,
    ordinal: usize,
) {
    if let Some(entry) = entries.iter_mut().find(|((entry_pane, entry_buffer), _)| {
        *entry_pane == pane_id && *entry_buffer == buffer_id
    }) {
        entry.1 = ordinal;
        return;
    }
    if entries.len() >= MAX_PANE_ACTIVE_FIND_MATCH_ENTRIES {
        entries.remove(0);
    }
    entries.push(((pane_id, buffer_id), ordinal));
}

fn pane_active_find_match_entry(
    entries: &[((PaneId, BufferId), usize)],
    pane_id: PaneId,
    buffer_id: BufferId,
) -> Option<usize> {
    entries
        .iter()
        .find(|((entry_pane, entry_buffer), _)| {
            *entry_pane == pane_id && *entry_buffer == buffer_id
        })
        .map(|(_, ordinal)| *ordinal)
}

impl KuroyaApp {
    fn active_find_buffer_index(&self) -> Option<usize> {
        let id = self.active?;
        self.buffers.iter().position(|buffer| buffer.id() == id)
    }

    pub(crate) fn visible_pane_active_find_match(
        &mut self,
        pane_id: PaneId,
        buffer_id: BufferId,
    ) -> usize {
        if self.panes.len() <= 1 || self.active == Some(buffer_id) {
            let ordinal = self.buffer_find_match;
            store_pane_active_find_match(
                &mut pane_active_find_match_entries()
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()),
                pane_id,
                buffer_id,
                ordinal,
            );
            return ordinal;
        }
        pane_active_find_match_entry(
            &pane_active_find_match_entries()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            pane_id,
            buffer_id,
        )
        .unwrap_or(self.buffer_find_match)
    }

    #[cfg(test)]
    pub(crate) fn active_find_matches(&mut self) -> Vec<Range<usize>> {
        let Some(buffer_index) = self.active_find_buffer_index() else {
            return Vec::new();
        };
        self.find_matches_for_buffer_index(buffer_index)
    }

    pub(crate) fn active_find_match_count(&mut self) -> usize {
        let Some(buffer_index) = self.active_find_buffer_index() else {
            return 0;
        };
        self.find_match_count_for_buffer_index(buffer_index)
    }

    pub(crate) fn find_matches_for_buffer_index(
        &mut self,
        buffer_index: usize,
    ) -> Vec<Range<usize>> {
        self.find_matches_ref_for_buffer_index(buffer_index)
            .map(<[_]>::to_vec)
            .unwrap_or_default()
    }

    fn find_match_count_for_buffer_index(&mut self, buffer_index: usize) -> usize {
        self.find_matches_ref_for_buffer_index(buffer_index)
            .map(<[_]>::len)
            .unwrap_or_default()
    }

    fn find_matches_ref_for_buffer_index(
        &mut self,
        buffer_index: usize,
    ) -> Option<&[Range<usize>]> {
        let buffer = self.buffers.get(buffer_index)?;
        let buffer_id = buffer.id();
        if !buffer_find_enabled_for_buffer(buffer) {
            self.buffer_find_cache.clear_for_buffer(buffer_id);
            return None;
        }
        if buffer_find_query_too_large(&self.buffer_find_query) {
            self.buffer_find_cache.clear_for_buffer(buffer_id);
            return Some(&[]);
        }
        let case_sensitive = self.buffer_find_case_sensitive;
        let whole_word = self.buffer_find_whole_word;
        let regex = self.buffer_find_regex;
        let scope = self.active_find_scope(buffer);
        let lookup_key = BufferFindCacheLookupKey::for_buffer(
            buffer,
            &self.buffer_find_query,
            case_sensitive,
            whole_word,
            regex,
            scope,
        );

        if self.buffer_find_cache.touch_lookup_key(&lookup_key) {
            return Some(
                self.buffer_find_cache
                    .matches_for_lookup_key(&lookup_key)
                    .expect("touched cache entry is present"),
            );
        }

        let key = lookup_key.to_cache_key();
        let matches = find_matches_for_normalized_buffer_key(buffer, &key);
        Some(self.buffer_find_cache.store(key, matches))
    }

    pub(crate) fn active_find_blocked_by_large_file_mode(&self) -> bool {
        self.active_buffer()
            .is_some_and(|buffer| !buffer_find_enabled_for_buffer(buffer))
    }

    pub(crate) fn active_find_scope(&self, buffer: &TextBuffer) -> Option<Range<usize>> {
        let scope = self.buffer_find_scope.as_ref()?;
        (scope.buffer_id == buffer.id())
            .then(|| normalize_buffer_find_scope_range(scope.range.clone(), buffer.len_chars()))
    }

    pub(crate) fn capture_active_find_scope(&self) -> Option<BufferFindScope> {
        let buffer = self.active_buffer()?;
        buffer_find_scope_from_selection(buffer, self.settings.find_auto_find_in_selection).map(
            |range| BufferFindScope {
                buffer_id: buffer.id(),
                range,
            },
        )
    }

    pub(crate) fn goto_find_match(&mut self, direction: isize) {
        self.goto_find_match_with_result(direction);
    }

    pub(crate) fn goto_find_match_with_result(&mut self, direction: isize) -> bool {
        if self.active_find_blocked_by_large_file_mode() {
            self.status = LARGE_FILE_FIND_STATUS.to_owned();
            return false;
        }
        if buffer_find_query_too_large(&self.buffer_find_query) {
            self.status = BUFFER_FIND_QUERY_TOO_LONG_STATUS.to_owned();
            return false;
        }

        let (next_match, range, len) = {
            let Some(buffer_index) = self.active_find_buffer_index() else {
                self.status = "No matches".to_owned();
                return false;
            };
            let current_find_match = self.buffer_find_match;
            let find_loop = self.settings.find_loop;
            let Some(matches) = self.find_matches_ref_for_buffer_index(buffer_index) else {
                self.status = "No matches".to_owned();
                return false;
            };
            let len = matches.len();
            if len == 0 {
                self.status = "No matches".to_owned();
                return false;
            }

            let current = current_find_match.min(len - 1);
            let Some(next_match) = next_find_match_index(current, len, direction, find_loop) else {
                self.buffer_find_match = current;
                self.status = if direction < 0 {
                    format!("First match of {len}")
                } else {
                    format!("Last match of {len}")
                };
                return false;
            };
            let Some(range) = matches.get(next_match).cloned() else {
                return false;
            };
            (next_match, range, len)
        };
        self.buffer_find_match = next_match;

        if let Some(target) = self.navigation_location_for_active_char(range.start) {
            self.record_navigation_origin(&target);
        }
        self.select_find_match_range(range, len)
    }

    pub(crate) fn select_find_match(&mut self) {
        self.select_find_match_with_result();
    }

    pub(crate) fn select_find_match_with_result(&mut self) -> bool {
        if self.active_find_blocked_by_large_file_mode() {
            self.status = LARGE_FILE_FIND_STATUS.to_owned();
            return false;
        }
        if buffer_find_query_too_large(&self.buffer_find_query) {
            self.status = BUFFER_FIND_QUERY_TOO_LONG_STATUS.to_owned();
            return false;
        }
        let (match_index, range, match_count) = {
            let Some(buffer_index) = self.active_find_buffer_index() else {
                self.status = "No matches".to_owned();
                return false;
            };
            let current_find_match = self.buffer_find_match;
            let Some(matches) = self.find_matches_ref_for_buffer_index(buffer_index) else {
                self.status = "No matches".to_owned();
                return false;
            };
            let match_count = matches.len();
            if match_count == 0 {
                self.status = "No matches".to_owned();
                return false;
            }
            let match_index = current_find_match.min(match_count - 1);
            let Some(range) = matches.get(match_index).cloned() else {
                return false;
            };
            (match_index, range, match_count)
        };
        self.buffer_find_match = match_index;
        self.select_find_match_range(range, match_count)
    }

    fn select_find_match_with_count(&mut self, match_count: usize) -> bool {
        if match_count == 0 {
            return false;
        }
        let (match_index, range) = {
            let Some(buffer_index) = self.active_find_buffer_index() else {
                return false;
            };
            let current_find_match = self.buffer_find_match;
            let Some(matches) = self.find_matches_ref_for_buffer_index(buffer_index) else {
                return false;
            };
            let match_index = current_find_match.min(match_count - 1);
            let Some(range) = matches.get(match_index).cloned() else {
                return false;
            };
            (match_index, range)
        };
        self.buffer_find_match = match_index;
        self.select_find_match_range(range, match_count)
    }

    fn select_find_match_range(&mut self, range: Range<usize>, match_count: usize) -> bool {
        if let Some(id) = self.active {
            let line = if let Some(buffer) = self.buffer_mut(id) {
                let line = buffer.char_position(range.start).line;
                buffer.set_selection(range.start, range.end);
                Some(line)
            } else {
                None
            };
            if let Some(line) = line {
                self.reveal_buffer_line(id, line + 1);
                self.pending_scroll_lines.insert(id, line);
            }
        }
        self.status = format!("Match {} of {}", self.buffer_find_match + 1, match_count);
        true
    }

    pub(crate) fn update_find_match_count_status(&mut self) {
        if self.active_find_blocked_by_large_file_mode() {
            self.status = LARGE_FILE_FIND_STATUS.to_owned();
            return;
        }
        if buffer_find_query_too_large(&self.buffer_find_query) {
            self.status = BUFFER_FIND_QUERY_TOO_LONG_STATUS.to_owned();
            return;
        }
        let count = self.active_find_match_count();
        self.status = match count {
            0 => "No matches".to_owned(),
            1 => "1 match".to_owned(),
            count => format!("{count} matches"),
        };
    }
}

pub(crate) fn buffer_find_enabled_for_buffer(buffer: &TextBuffer) -> bool {
    !buffer_uses_large_file_mode(buffer)
}

#[cfg(test)]
fn find_matches_for_buffer(buffer: &TextBuffer, key: &BufferFindCacheKey) -> Vec<Range<usize>> {
    if !buffer_find_enabled_for_buffer(buffer) {
        return Vec::new();
    }

    let query = key.query.as_str();
    if query.is_empty() {
        return Vec::new();
    }
    if query.len() > BUFFER_FIND_MAX_QUERY_BYTES {
        return Vec::new();
    }

    let buffer_len = buffer.len_chars();
    let scope = normalize_buffer_find_scope(key.scope.clone(), buffer_len);
    find_matches_for_buffer_with_normalized_inputs(buffer, key, query, scope.as_ref(), buffer_len)
}

fn find_matches_for_normalized_buffer_key(
    buffer: &TextBuffer,
    key: &BufferFindCacheKey,
) -> Vec<Range<usize>> {
    if !buffer_find_enabled_for_buffer(buffer) {
        return Vec::new();
    }

    find_matches_for_buffer_with_normalized_inputs(
        buffer,
        key,
        key.query.as_str(),
        key.scope.as_ref(),
        key.len_chars,
    )
}

fn find_matches_for_buffer_with_normalized_inputs(
    buffer: &TextBuffer,
    key: &BufferFindCacheKey,
    query: &str,
    scope: Option<&Range<usize>>,
    buffer_len: usize,
) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    if query.len() > BUFFER_FIND_MAX_QUERY_BYTES {
        return Vec::new();
    }

    if scope.as_ref().is_some_and(|scope| scope.start >= scope.end) {
        return Vec::new();
    }

    let search_len = scope.map_or(buffer_len, |scope| scope.end - scope.start);
    if !key.regex && literal_query_longer_than_char_len(query, search_len) {
        return Vec::new();
    }

    if let Some(scope) = scope {
        if let Some(matches) =
            find_line_local_regex_matches_for_buffer_scope(buffer, key, query, scope, buffer_len)
        {
            return matches;
        }
        if let Some(matches) =
            find_literal_matches_for_buffer_scope(buffer, key, query, scope, buffer_len)
        {
            return matches;
        }
    }

    let mut matches = if key.regex {
        buffer
            .find_regex_matches_with_options(
                query,
                BUFFER_FIND_MAX_MATCHES,
                key.case_sensitive,
                key.whole_word,
            )
            .unwrap_or_default()
    } else {
        buffer.find_matches_with_options(
            query,
            BUFFER_FIND_MAX_MATCHES,
            key.case_sensitive,
            key.whole_word,
        )
    };
    if let Some(scope) = scope {
        if scope.start > 0 || scope.end < buffer_len {
            matches.retain(|range| range.start >= scope.start && range.end <= scope.end);
        }
    }
    matches
}

pub(crate) fn buffer_find_query_too_large(query: &str) -> bool {
    query.len() > BUFFER_FIND_MAX_QUERY_BYTES
}

fn buffer_find_cache_version(buffer_version: u64, whole_word: bool, word_separators: &str) -> u64 {
    if !whole_word {
        return buffer_version;
    }

    let mut fingerprint = 0xcbf2_9ce4_8422_2325u64;
    for byte in word_separators.bytes() {
        fingerprint ^= u64::from(byte);
        fingerprint = fingerprint.wrapping_mul(0x0000_0001_0000_01b3);
    }
    buffer_version ^ fingerprint.rotate_left(1)
}

fn normalize_buffer_find_scope(
    scope: Option<Range<usize>>,
    buffer_len: usize,
) -> Option<Range<usize>> {
    scope.map(|scope| normalize_buffer_find_scope_range(scope, buffer_len))
}

fn normalize_buffer_find_scope_range(scope: Range<usize>, buffer_len: usize) -> Range<usize> {
    let start = scope.start.min(buffer_len);
    let end = scope.end.min(buffer_len).max(start);
    start..end
}

fn literal_query_longer_than_char_len(query: &str, char_len: usize) -> bool {
    query.chars().nth(char_len).is_some()
}

fn find_line_local_regex_matches_for_buffer_scope(
    buffer: &TextBuffer,
    key: &BufferFindCacheKey,
    query: &str,
    scope: &Range<usize>,
    buffer_len: usize,
) -> Option<Vec<Range<usize>>> {
    if !key.regex
        || !buffer_find_regex_query_is_line_local(query)
        || (scope.start == 0 && scope.end >= buffer_len)
    {
        return None;
    }

    let search_scope = line_bounded_scoped_search_range(buffer, scope, buffer_len);
    let search_len = search_scope.end.saturating_sub(search_scope.start);
    if search_len.saturating_mul(2) > buffer_len {
        return None;
    }

    let scoped_text = buffer.text_range(search_scope.clone())?;
    let mut scoped_buffer = TextBuffer::from_text(key.buffer_id, None, scoped_text);
    if key.whole_word {
        scoped_buffer.set_word_separators(buffer.word_separators().to_owned());
    }
    let mut matches = scoped_buffer
        .find_regex_matches_with_options(
            query,
            BUFFER_FIND_MAX_MATCHES,
            key.case_sensitive,
            key.whole_word,
        )
        .unwrap_or_default();
    for range in &mut matches {
        range.start += search_scope.start;
        range.end += search_scope.start;
    }
    if search_scope.start < scope.start || search_scope.end > scope.end {
        matches.retain(|range| range.start >= scope.start && range.end <= scope.end);
    }
    Some(matches)
}

fn line_bounded_scoped_search_range(
    buffer: &TextBuffer,
    scope: &Range<usize>,
    buffer_len: usize,
) -> Range<usize> {
    let start_line = buffer.char_position(scope.start).line;
    let end_line = buffer.char_position(scope.end.saturating_sub(1)).line;
    let start = buffer.line_column_to_char(start_line, 0);
    let next_line = end_line.saturating_add(1);
    let end = if next_line < buffer.len_lines() {
        buffer.line_column_to_char(next_line, 0)
    } else {
        buffer_len
    };
    start..end
}

fn find_literal_matches_for_buffer_scope(
    buffer: &TextBuffer,
    key: &BufferFindCacheKey,
    query: &str,
    scope: &Range<usize>,
    buffer_len: usize,
) -> Option<Vec<Range<usize>>> {
    if key.regex || (scope.start == 0 && scope.end >= buffer_len) {
        return None;
    }

    let search_scope = literal_scoped_search_range(scope, key.whole_word, buffer_len);
    let search_len = search_scope.end.saturating_sub(search_scope.start);
    if search_len.saturating_mul(2) > buffer_len {
        return None;
    }

    let scoped_text = buffer.text_range(search_scope.clone())?;
    let mut scoped_buffer = TextBuffer::from_text(key.buffer_id, None, scoped_text);
    if key.whole_word {
        scoped_buffer.set_word_separators(buffer.word_separators().to_owned());
    }
    let mut matches = scoped_buffer.find_matches_with_options(
        query,
        BUFFER_FIND_MAX_MATCHES,
        key.case_sensitive,
        key.whole_word,
    );
    for range in &mut matches {
        range.start += search_scope.start;
        range.end += search_scope.start;
    }
    if search_scope.start < scope.start || search_scope.end > scope.end {
        matches.retain(|range| range.start >= scope.start && range.end <= scope.end);
    }
    Some(matches)
}

fn literal_scoped_search_range(
    scope: &Range<usize>,
    include_word_boundaries: bool,
    buffer_len: usize,
) -> Range<usize> {
    if include_word_boundaries {
        scope.start.saturating_sub(1)..scope.end.saturating_add(1).min(buffer_len)
    } else {
        scope.clone()
    }
}

fn buffer_find_regex_query_is_line_local(query: &str) -> bool {
    if query.contains(['\n', '\r']) {
        return false;
    }

    let mut chars = query.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                let Some(escaped) = chars.next() else {
                    continue;
                };
                if buffer_find_regex_escape_can_match_line_break(escaped) {
                    return false;
                }
            }
            '[' => {
                if !buffer_find_regex_class_is_line_local(&mut chars) {
                    return false;
                }
            }
            '(' if chars.peek() == Some(&'?') => {
                if buffer_find_regex_inline_flags_can_enable_dotall(&mut chars) {
                    return false;
                }
            }
            _ => {}
        }
    }

    true
}

fn buffer_find_regex_escape_can_match_line_break(escaped: char) -> bool {
    matches!(escaped, 'n' | 'r' | 's' | 'D' | 'W' | 'P' | 'p' | 'x' | 'u')
}

fn buffer_find_regex_class_is_line_local(
    chars: &mut std::iter::Peekable<impl Iterator<Item = char>>,
) -> bool {
    if chars.peek() == Some(&'^') {
        return false;
    }

    let mut escaped = false;
    for ch in chars.by_ref() {
        if escaped {
            escaped = false;
            if buffer_find_regex_escape_can_match_line_break(ch) {
                return false;
            }
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '\n' | '\r' => return false,
            ']' => return true,
            _ => {}
        }
    }

    true
}

fn buffer_find_regex_inline_flags_can_enable_dotall(
    chars: &mut std::iter::Peekable<impl Iterator<Item = char>>,
) -> bool {
    let Some('?') = chars.next() else {
        return false;
    };

    let mut disabling = false;
    while let Some(ch) = chars.peek().copied() {
        match ch {
            ':' | ')' => return false,
            '-' => {
                disabling = true;
                chars.next();
            }
            's' if !disabling => return true,
            _ => {
                chars.next();
            }
        }
    }

    false
}

pub(crate) fn next_find_match_index(
    current: usize,
    len: usize,
    direction: isize,
    loop_enabled: bool,
) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let current = current.min(len - 1);
    if direction < 0 {
        if current == 0 {
            loop_enabled.then_some(len - 1)
        } else {
            Some(current - 1)
        }
    } else if current + 1 >= len {
        loop_enabled.then_some(0)
    } else {
        Some(current + 1)
    }
}

pub(crate) fn live_find_query_should_move_cursor(
    find_on_type: bool,
    cursor_move_on_type: bool,
) -> bool {
    find_on_type && cursor_move_on_type
}

pub(crate) fn buffer_find_scope_from_selection(
    buffer: &TextBuffer,
    mode: EditorFindAutoFindInSelection,
) -> Option<Range<usize>> {
    if matches!(mode, EditorFindAutoFindInSelection::Never) {
        return None;
    }
    let range = buffer
        .selections()
        .iter()
        .rev()
        .find_map(|selection| (!selection.is_caret()).then(|| selection.range()))?;
    let range = normalize_buffer_find_scope_range(range, buffer.len_chars());
    if matches!(mode, EditorFindAutoFindInSelection::Multiline)
        && !buffer_range_contains_line_break(buffer, &range)
    {
        return None;
    }
    Some(range)
}

fn buffer_range_contains_line_break(buffer: &TextBuffer, range: &Range<usize>) -> bool {
    if range.start >= range.end {
        return false;
    }
    buffer.char_position(range.start).line < buffer.char_position(range.end).line
}

#[cfg(test)]
mod tests {
    use super::{
        BUFFER_FIND_CACHE_CAPACITY, BUFFER_FIND_MAX_MATCHES, BUFFER_FIND_MAX_QUERY_BYTES,
        BufferFindCache, BufferFindCacheKey, BufferFindCacheLookupKey,
        MAX_PANE_ACTIVE_FIND_MATCH_ENTRIES, buffer_find_enabled_for_buffer,
        buffer_find_query_too_large, buffer_find_scope_from_selection, find_matches_for_buffer,
        pane_active_find_match_entries, pane_active_find_match_entry, store_pane_active_find_match,
    };
    use crate::{
        KuroyaApp, app_startup_context::AppStartupContext,
        large_file_mode::LARGE_FILE_MODE_MAX_BYTES, session_state::EditorPane,
        terminal::TerminalPane,
    };
    use kuroya_core::{
        EditorFindAutoFindInSelection, EditorSettings, Selection, TextBuffer, Workspace,
    };
    use std::{ops::Range, path::PathBuf, time::Instant};
    use tokio::runtime::Runtime;

    #[test]
    fn buffer_find_is_disabled_for_large_file_mode_buffers() {
        let small = TextBuffer::from_text(1, None, "needle".to_owned());
        let large = TextBuffer::from_text(2, None, "x".repeat(LARGE_FILE_MODE_MAX_BYTES + 1));

        assert!(buffer_find_enabled_for_buffer(&small));
        assert!(!buffer_find_enabled_for_buffer(&large));
    }

    #[test]
    fn buffer_find_cache_reuses_only_matching_query_state() {
        let mut cache = BufferFindCache::default();
        let key = find_key(7, 1, "needle", false, None);

        assert_eq!(cache.get(&key), None);
        assert!(!cache.matches_key(&key));
        cache.store(key.clone(), vec![0..6, 12..18]);
        assert_eq!(cache.get(&key), Some(vec![0..6, 12..18]));
        assert!(cache.matches_key(&key));

        let changed_query = find_key(7, 1, "other", false, None);
        assert_eq!(cache.get(&changed_query), None);
        assert!(!cache.matches_key(&changed_query));

        let changed_version = find_key(7, 2, "needle", false, None);
        assert_eq!(cache.get(&changed_version), None);
        assert!(!cache.matches_key(&changed_version));

        cache.clear_for_buffer(8);
        assert_eq!(cache.get(&key), Some(vec![0..6, 12..18]));
        cache.clear_for_buffer(7);
        assert_eq!(cache.get(&key), None);
    }

    #[test]
    fn buffer_find_cache_keeps_alternating_keys_cached() {
        let mut cache = BufferFindCache::default();
        let alpha = find_key(7, 1, "alpha", false, None);
        let beta = find_key(8, 1, "beta", false, None);

        cache.store(alpha.clone(), std::iter::once(0..5).collect());
        cache.store(beta.clone(), std::iter::once(0..4).collect());

        for _ in 0..4 {
            assert!(cache.matches_key(&alpha));
            assert!(cache.matches_key(&beta));
        }
        assert_eq!(
            cache.get(&alpha),
            Some(std::iter::once(0..5).collect::<Vec<_>>())
        );
        assert_eq!(
            cache.get(&beta),
            Some(std::iter::once(0..4).collect::<Vec<_>>())
        );
        assert_eq!(cache.len_for_test(), 2);
    }

    #[test]
    fn buffer_find_cache_evicts_oldest_entry_beyond_capacity() {
        let mut cache = BufferFindCache::default();
        let mut keys = Vec::new();
        for index in 0..BUFFER_FIND_CACHE_CAPACITY {
            let key = find_key(7, index as u64 + 1, "needle", false, None);
            cache.store(key.clone(), std::iter::once(0..6).collect());
            keys.push(key);
        }
        assert_eq!(cache.len_for_test(), BUFFER_FIND_CACHE_CAPACITY);

        let newest = find_key(
            7,
            BUFFER_FIND_CACHE_CAPACITY as u64 + 1,
            "needle",
            false,
            None,
        );
        cache.store(newest.clone(), std::iter::once(1..7).collect());

        assert_eq!(cache.len_for_test(), BUFFER_FIND_CACHE_CAPACITY);
        assert_eq!(cache.get(&keys[0]), None);
        for key in keys.iter().skip(1) {
            assert_eq!(
                cache.get(key),
                Some(std::iter::once(0..6).collect::<Vec<_>>())
            );
        }
        assert_eq!(
            cache.get(&newest),
            Some(std::iter::once(1..7).collect::<Vec<_>>())
        );
    }

    #[test]
    fn buffer_find_cache_lookup_refreshes_recency_before_eviction() {
        let hot_buffer = TextBuffer::from_text(1, None, "alpha beta".to_owned());
        let mut cache = BufferFindCache::default();
        let hot_lookup =
            BufferFindCacheLookupKey::for_buffer(&hot_buffer, "alpha", true, false, false, None);
        cache.store(hot_lookup.to_cache_key(), std::iter::once(0..5).collect());

        for round in 0..BUFFER_FIND_CACHE_CAPACITY - 1 {
            assert!(cache.touch_lookup_key(&hot_lookup));
            let other = TextBuffer::from_text(round as u64 + 2, None, "alpha beta".to_owned());
            let other_lookup =
                BufferFindCacheLookupKey::for_buffer(&other, "alpha", true, false, false, None);
            cache.store(other_lookup.to_cache_key(), std::iter::once(0..5).collect());
        }
        assert_eq!(cache.len_for_test(), BUFFER_FIND_CACHE_CAPACITY);

        let cold = TextBuffer::from_text(999, None, "alpha beta".to_owned());
        let cold_lookup =
            BufferFindCacheLookupKey::for_buffer(&cold, "alpha", true, false, false, None);
        cache.store(cold_lookup.to_cache_key(), std::iter::once(0..5).collect());

        assert_eq!(cache.len_for_test(), BUFFER_FIND_CACHE_CAPACITY);
        assert!(cache.matches_for_lookup_key(&hot_lookup).is_some());
        assert!(cache.matches_for_lookup_key(&cold_lookup).is_some());
    }

    #[test]
    fn buffer_find_cache_matches_borrowed_lookup_key() {
        let buffer = TextBuffer::from_text(7, None, "alpha beta".to_owned());
        let key = BufferFindCacheKey::for_buffer(&buffer, "alpha", true, false, false, Some(6..99));
        let mut cache = BufferFindCache::default();
        cache.store(key, std::iter::once(6..10).collect());

        let lookup = BufferFindCacheLookupKey::for_buffer(
            &buffer,
            "alpha",
            true,
            false,
            false,
            Some(6..usize::MAX),
        );
        assert!(cache.touch_lookup_key(&lookup));

        let padded_query = BufferFindCacheLookupKey::for_buffer(
            &buffer,
            "  alpha  ",
            true,
            false,
            false,
            Some(6..99),
        );
        assert!(!cache.touch_lookup_key(&padded_query));

        let changed_query =
            BufferFindCacheLookupKey::for_buffer(&buffer, "beta", true, false, false, Some(6..99));
        assert!(!cache.touch_lookup_key(&changed_query));

        let changed_case = BufferFindCacheLookupKey::for_buffer(
            &buffer,
            "alpha",
            false,
            false,
            false,
            Some(6..99),
        );
        assert!(!cache.touch_lookup_key(&changed_case));

        let changed_scope =
            BufferFindCacheLookupKey::for_buffer(&buffer, "alpha", true, false, false, Some(0..5));
        assert!(!cache.touch_lookup_key(&changed_scope));
    }

    #[test]
    fn buffer_find_cache_key_preserves_query_whitespace_and_normalizes_scope() {
        let mut buffer = TextBuffer::from_text(7, None, "alpha beta".to_owned());
        buffer.set_word_separators(".");

        let key =
            BufferFindCacheKey::for_buffer(&buffer, "  alpha  ", true, true, false, Some(6..99));
        assert_eq!(key.query, "  alpha  ");
        assert_eq!(key.scope, Some(6..10));
        assert_eq!(key.len_chars, "alpha beta".chars().count());

        buffer.set_word_separators("");
        let changed_whole_word_key =
            BufferFindCacheKey::for_buffer(&buffer, "alpha", true, true, false, Some(6..99));
        assert_ne!(key, changed_whole_word_key);

        let non_word_key = BufferFindCacheKey::for_buffer(
            &buffer,
            "alpha",
            true,
            false,
            false,
            Some(Range { start: 7, end: 5 }),
        );
        buffer.set_word_separators(".");
        let non_word_changed = BufferFindCacheKey::for_buffer(
            &buffer,
            "alpha",
            true,
            false,
            false,
            Some(Range { start: 7, end: 5 }),
        );
        assert_eq!(non_word_key, non_word_changed);
        assert_eq!(non_word_key.scope, Some(7..7));
    }

    #[test]
    fn buffer_find_cache_key_includes_buffer_length_identity() {
        let short = TextBuffer::from_text(7, None, "needle".to_owned());
        let long = TextBuffer::from_text(7, None, "needle tail".to_owned());

        let short_key = BufferFindCacheKey::for_buffer(&short, "needle", true, false, false, None);
        let long_key = BufferFindCacheKey::for_buffer(&long, "needle", true, false, false, None);

        assert_ne!(short_key, long_key);
    }

    #[test]
    fn find_matches_for_buffer_blocks_large_buffers_directly() {
        let buffer = TextBuffer::from_text(
            7,
            None,
            format!("needle{}", "x".repeat(LARGE_FILE_MODE_MAX_BYTES + 1)),
        );
        let key = find_key(7, buffer.version(), "needle", false, None);

        assert_eq!(
            find_matches_for_buffer(&buffer, &key),
            Vec::<Range<usize>>::new()
        );
    }

    #[test]
    fn find_matches_for_buffer_applies_scope_before_caching() {
        let buffer = TextBuffer::from_text(7, None, "needle needle needle".to_owned());
        let key = find_key(7, buffer.version(), "needle", false, Some(7..13));

        assert_eq!(find_matches_for_buffer(&buffer, &key), vec![7..13]);
    }

    #[test]
    fn find_matches_for_buffer_searches_small_literal_scope_before_match_limit() {
        let prefix = "needle ".repeat(BUFFER_FIND_MAX_MATCHES + 1);
        let scoped_text = "scope needle tail";
        let scope_start = prefix.chars().count();
        let scope_end = scope_start + scoped_text.chars().count();
        let buffer = TextBuffer::from_text(7, None, format!("{prefix}{scoped_text}"));
        let key = find_key(
            7,
            buffer.version(),
            "needle",
            false,
            Some(scope_start..scope_end),
        );
        let match_start = scope_start + "scope ".chars().count();

        assert_eq!(
            find_matches_for_buffer(&buffer, &key),
            vec![match_start..match_start + "needle".chars().count()]
        );
    }

    #[test]
    fn find_matches_for_buffer_searches_small_whole_word_literal_scope_before_match_limit() {
        let prefix = "needle ".repeat(BUFFER_FIND_MAX_MATCHES + 1);
        let scoped_text = "xneedle needle tail";
        let scope_start = prefix.chars().count() + "x".chars().count();
        let scope_end = scope_start + "needle needle tail".chars().count();
        let buffer = TextBuffer::from_text(7, None, format!("{prefix}{scoped_text}"));
        let mut key = find_key(
            7,
            buffer.version(),
            "needle",
            false,
            Some(scope_start..scope_end),
        );
        key.whole_word = true;
        let match_start = scope_start + "needle ".chars().count();

        assert_eq!(
            find_matches_for_buffer(&buffer, &key),
            vec![match_start..match_start + "needle".chars().count()]
        );
    }

    #[test]
    fn find_matches_for_buffer_searches_small_line_local_regex_scope_before_match_limit() {
        let prefix = "needle-1\n".repeat(BUFFER_FIND_MAX_MATCHES + 1);
        let scoped_text = "scope needle-777 tail\n";
        let scope_start = prefix.chars().count();
        let scope_end = scope_start + scoped_text.chars().count();
        let buffer = TextBuffer::from_text(7, None, format!("{prefix}{scoped_text}"));
        let key = find_key(
            7,
            buffer.version(),
            r"needle-\d+",
            true,
            Some(scope_start..scope_end),
        );
        let match_start = scope_start + "scope ".chars().count();

        assert_eq!(
            find_matches_for_buffer(&buffer, &key),
            vec![match_start..match_start + "needle-777".chars().count()]
        );
    }

    #[test]
    fn find_matches_for_buffer_keeps_line_local_regex_anchors_bound_to_original_lines() {
        let text = "prefix needle-1\nneedle-2\n";
        let scope_start = "prefix ".chars().count();
        let scope_end = "prefix needle-1".chars().count();
        let buffer = TextBuffer::from_text(7, None, text.to_owned());
        let key = find_key(
            7,
            buffer.version(),
            r"^needle-\d+",
            true,
            Some(scope_start..scope_end),
        );

        assert_eq!(
            find_matches_for_buffer(&buffer, &key),
            Vec::<Range<usize>>::new()
        );
    }

    #[test]
    fn find_matches_for_buffer_normalizes_out_of_bounds_scope() {
        let buffer = TextBuffer::from_text(7, None, "alpha needle".to_owned());
        let key = find_key(7, buffer.version(), "needle", false, Some(6..usize::MAX));

        assert_eq!(find_matches_for_buffer(&buffer, &key), vec![6..12]);

        let key = find_key(
            7,
            buffer.version(),
            "needle",
            false,
            Some(Range { start: 10, end: 5 }),
        );

        assert_eq!(
            find_matches_for_buffer(&buffer, &key),
            Vec::<Range<usize>>::new()
        );
    }

    #[test]
    fn find_matches_for_buffer_rejects_literal_queries_longer_than_scope_by_chars() {
        let buffer = TextBuffer::from_text(7, None, "\u{e9} needle".to_owned());
        let key = find_key(7, buffer.version(), "\u{e9}x", false, Some(0..1));

        assert_eq!(
            find_matches_for_buffer(&buffer, &key),
            Vec::<Range<usize>>::new()
        );

        let key = find_key(7, buffer.version(), "\u{e9}", false, Some(0..1));

        assert_eq!(find_matches_for_buffer(&buffer, &key), vec![0..1]);
    }

    #[test]
    fn find_matches_for_buffer_short_circuits_oversized_queries() {
        let buffer = TextBuffer::from_text(7, None, "needle".to_owned());
        let query = "x".repeat(BUFFER_FIND_MAX_QUERY_BYTES + 1);
        let literal = find_key(7, buffer.version(), &query, false, None);
        let regex = find_key(7, buffer.version(), &query, true, None);

        assert!(buffer_find_query_too_large(&query));
        assert_eq!(
            find_matches_for_buffer(&buffer, &literal),
            Vec::<Range<usize>>::new()
        );
        assert_eq!(
            find_matches_for_buffer(&buffer, &regex),
            Vec::<Range<usize>>::new()
        );
    }

    #[test]
    fn find_scope_multiline_mode_checks_selection_range_lines() {
        let mut buffer = TextBuffer::from_text(7, None, "one\ntwo\nthree".to_owned());
        buffer.set_selections([Selection {
            anchor: 0,
            cursor: 3,
        }]);
        assert_eq!(
            buffer_find_scope_from_selection(&buffer, EditorFindAutoFindInSelection::Multiline),
            None
        );

        buffer.set_selections([Selection {
            anchor: 0,
            cursor: 4,
        }]);
        assert_eq!(
            buffer_find_scope_from_selection(&buffer, EditorFindAutoFindInSelection::Multiline),
            Some(0..4)
        );
    }

    fn find_key(
        buffer_id: u64,
        version: u64,
        query: &str,
        regex: bool,
        scope: Option<Range<usize>>,
    ) -> BufferFindCacheKey {
        BufferFindCacheKey {
            buffer_id,
            version,
            len_chars: 0,
            query: query.to_owned(),
            case_sensitive: true,
            whole_word: false,
            regex,
            scope,
        }
    }

    #[test]
    fn pane_active_find_match_entries_store_update_and_prune_oldest() {
        let mut entries = Vec::new();
        store_pane_active_find_match(&mut entries, 1, 11, 3);
        store_pane_active_find_match(&mut entries, 2, 12, 7);

        assert_eq!(pane_active_find_match_entry(&entries, 1, 11), Some(3));
        assert_eq!(pane_active_find_match_entry(&entries, 2, 12), Some(7));
        assert_eq!(pane_active_find_match_entry(&entries, 9, 11), None);
        assert_eq!(pane_active_find_match_entry(&entries, 1, 12), None);

        store_pane_active_find_match(&mut entries, 1, 11, 5);
        assert_eq!(entries.len(), 2);
        assert_eq!(pane_active_find_match_entry(&entries, 1, 11), Some(5));

        for index in 0..MAX_PANE_ACTIVE_FIND_MATCH_ENTRIES {
            store_pane_active_find_match(
                &mut entries,
                100 + index as u64,
                1000 + index as u64,
                index,
            );
        }

        assert_eq!(entries.len(), MAX_PANE_ACTIVE_FIND_MATCH_ENTRIES);
        assert_eq!(pane_active_find_match_entry(&entries, 1, 11), None);
        assert_eq!(
            pane_active_find_match_entry(
                &entries,
                100 + MAX_PANE_ACTIVE_FIND_MATCH_ENTRIES as u64 - 1,
                1000 + MAX_PANE_ACTIVE_FIND_MATCH_ENTRIES as u64 - 1
            ),
            Some(MAX_PANE_ACTIVE_FIND_MATCH_ENTRIES - 1)
        );
    }

    #[test]
    fn visible_pane_find_ordinals_stay_independent_across_split_panes() {
        let mut app = app_for_buffer_find_test();
        app.buffers
            .push(TextBuffer::from_text(101, None, "needle needle".to_owned()));
        app.buffers
            .push(TextBuffer::from_text(102, None, "needle".to_owned()));
        app.panes = vec![
            EditorPane {
                id: 21,
                active: Some(101),
                weight: 1.0,
            },
            EditorPane {
                id: 22,
                active: Some(102),
                weight: 1.0,
            },
        ];
        app.active_pane = 21;
        app.active = Some(101);
        app.buffer_find_open = true;
        app.buffer_find_query = "needle".to_owned();

        app.buffer_find_match = 1;
        assert_eq!(app.visible_pane_active_find_match(21, 101), 1);

        assert_eq!(app.visible_pane_active_find_match(22, 102), 1);

        app.active = Some(102);
        app.buffer_find_match = 0;
        assert_eq!(app.visible_pane_active_find_match(22, 102), 0);

        assert_eq!(app.visible_pane_active_find_match(21, 101), 1);
    }

    #[test]
    fn split_pane_alternating_find_lookups_keep_both_buffers_cached() {
        let mut app = app_for_buffer_find_test();
        app.buffers
            .push(TextBuffer::from_text(101, None, "needle needle".to_owned()));
        app.buffers
            .push(TextBuffer::from_text(102, None, "needle".to_owned()));
        app.buffer_find_open = true;
        app.buffer_find_query = "needle".to_owned();

        let first = app.find_matches_for_buffer_index(0);
        let second = app.find_matches_for_buffer_index(1);
        assert_eq!(first, vec![0..6, 7..13]);
        assert_eq!(second, vec![0..6]);

        for _ in 0..4 {
            assert_eq!(app.find_matches_for_buffer_index(0), first);
            assert_eq!(app.find_matches_for_buffer_index(1), second);
        }

        assert_eq!(
            app.buffer_find_cache.cached_buffer_ids_for_test(),
            vec![101, 102]
        );
    }

    #[test]
    fn find_query_preserves_leading_and_trailing_whitespace() {
        let mut app = app_for_buffer_find_test();
        app.buffers.push(TextBuffer::from_text(
            101,
            None,
            "alpha beta  gamma ".to_owned(),
        ));
        app.active = Some(101);
        app.buffer_find_open = true;

        app.buffer_find_query = " ".to_owned();
        assert_eq!(
            app.find_matches_for_buffer_index(0),
            vec![5..6, 10..11, 11..12, 17..18]
        );

        app.buffer_find_query = " beta ".to_owned();
        assert_eq!(app.find_matches_for_buffer_index(0), vec![5..11]);

        app.buffer_find_query = "gamma ".to_owned();
        assert_eq!(app.find_matches_for_buffer_index(0), vec![12..18]);
    }

    #[test]
    fn find_regex_cache_miss_reuses_memoized_regex_with_identical_ranges() {
        let mut app = app_for_buffer_find_test();
        app.buffers
            .push(TextBuffer::from_text(101, None, "a1 b2 a1".to_owned()));
        app.active = Some(101);
        app.buffer_find_open = true;
        app.buffer_find_regex = true;
        app.buffer_find_query = r"a\d".to_owned();

        assert_eq!(app.find_matches_for_buffer_index(0), vec![0..2, 6..8]);

        let len_chars = app.buffers[0].len_chars();
        assert!(app.buffers[0].replace_range(len_chars..len_chars, " a4"));
        assert_eq!(
            app.find_matches_for_buffer_index(0),
            vec![0..2, 6..8, 9..11]
        );
    }

    #[test]
    fn single_visible_pane_keeps_the_live_find_ordinal() {
        let mut app = app_for_buffer_find_test();
        app.buffer_find_open = true;
        app.buffer_find_query = "needle".to_owned();
        app.buffer_find_match = 4;

        assert_eq!(app.panes.len(), 1);
        store_pane_active_find_match(
            &mut pane_active_find_match_entries()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            31,
            41,
            0,
        );

        assert_eq!(app.visible_pane_active_find_match(31, 41), 4);
    }

    fn app_for_buffer_find_test() -> KuroyaApp {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let settings = EditorSettings::default();
        let root = PathBuf::from("kuroya-buffer-find-tests");
        KuroyaApp::from_startup_context(AppStartupContext {
            runtime: Runtime::new().expect("test runtime"),
            tx,
            rx,
            workspace: Workspace::new(root.clone()),
            settings: settings.clone(),
            settings_panel_draft: settings,
            settings_editor_font_path: String::new(),
            settings_ui_font_path: String::new(),
            theme_picker_selected: 0,
            saved_session: None,
            terminal: TerminalPane::new(root.clone(), 100, 12.0, 1.2),
            watcher: None,
            recent_projects: Vec::new(),
            trusted_workspaces: vec![root],
            now: Instant::now(),
            startup_timings: Vec::new(),
        })
    }
}
