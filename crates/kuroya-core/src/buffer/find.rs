use regex::{Captures, Regex, RegexBuilder};
use std::{
    ops::Range,
    sync::{LazyLock, Mutex, MutexGuard},
};

use super::edits::edit_range_is_valid;
use super::text::{
    byte_range_to_char_range, char_range_to_byte_range, chars_match, normalize_char_range,
    rope_slice_text,
};
use super::{REGEX_FULL_TEXT_MAX_BYTES, RegexReplaceAllOptions, TextBuffer, TextEdit};

const FIND_RESULT_PREALLOC_LIMIT: usize = 8;

impl TextBuffer {
    pub fn find_matches(&self, query: &str, max_matches: usize) -> Vec<Range<usize>> {
        self.find_matches_with_options(query, max_matches, true, false)
    }

    pub fn find_matches_with_options(
        &self,
        query: &str,
        max_matches: usize,
        case_sensitive: bool,
        whole_word: bool,
    ) -> Vec<Range<usize>> {
        if query.is_empty() || max_matches == 0 {
            return Vec::new();
        }
        self.find_literal_matches_with_options(query, max_matches, case_sensitive, whole_word)
    }

    fn find_literal_matches_with_options(
        &self,
        query: &str,
        max_matches: usize,
        case_sensitive: bool,
        whole_word: bool,
    ) -> Vec<Range<usize>> {
        let mut query_char_iter = query.chars();
        let Some(first_query_char) = query_char_iter.next() else {
            return Vec::new();
        };
        let len_chars = self.len_chars();
        let Some(second_query_char) = query_char_iter.next() else {
            return self.find_single_char_matches_with_options(
                first_query_char,
                max_matches,
                case_sensitive,
                whole_word,
                len_chars,
            );
        };

        let mut query_chars = Vec::with_capacity(query.len());
        query_chars.push(first_query_char);
        query_chars.push(second_query_char);
        query_chars.extend(query_char_iter);
        let query_len = query_chars.len();
        if query_len > len_chars {
            return Vec::new();
        }

        let mut matches = Vec::with_capacity(find_result_capacity(max_matches));
        let max_start = len_chars - query_len;
        let mut next_allowed_start = 0;
        for (start, ch) in self.rope.chars().enumerate() {
            if start > max_start {
                break;
            }
            if matches.len() >= max_matches {
                break;
            }
            if start < next_allowed_start || !chars_match(ch, first_query_char, case_sensitive) {
                continue;
            }
            let end = start + query_len;
            if !self.query_tail_matches_at(start + 1, &query_chars[1..], case_sensitive, len_chars)
            {
                continue;
            }
            if whole_word && !self.is_whole_word_char_range(start, end, len_chars) {
                continue;
            }
            matches.push(start..end);
            next_allowed_start = end.max(start + 1);
        }
        matches
    }

    fn find_single_char_matches_with_options(
        &self,
        query: char,
        max_matches: usize,
        case_sensitive: bool,
        whole_word: bool,
        len_chars: usize,
    ) -> Vec<Range<usize>> {
        let mut matches = Vec::with_capacity(find_result_capacity(max_matches));
        for (idx, ch) in self.rope.chars().enumerate() {
            if matches.len() >= max_matches {
                break;
            }
            if !chars_match(ch, query, case_sensitive) {
                continue;
            }

            let end = idx + 1;
            if whole_word && !self.is_whole_word_char_range(idx, end, len_chars) {
                continue;
            }
            matches.push(idx..end);
        }
        matches
    }

    pub(super) fn query_tail_matches_at(
        &self,
        start: usize,
        query_chars: &[char],
        case_sensitive: bool,
        len_chars: usize,
    ) -> bool {
        if start + query_chars.len() > len_chars {
            return false;
        }

        self.rope
            .chars_at(start)
            .zip(query_chars.iter().copied())
            .all(|(left, right)| chars_match(left, right, case_sensitive))
    }

    fn is_whole_word_char_range(&self, start: usize, end: usize, len_chars: usize) -> bool {
        let before = (start > 0).then(|| self.rope.char(start - 1));
        let after = (end < len_chars).then(|| self.rope.char(end));
        !before.is_some_and(|ch| self.is_word_char(ch))
            && !after.is_some_and(|ch| self.is_word_char(ch))
    }

    pub fn find_regex_matches_with_options(
        &self,
        query: &str,
        max_matches: usize,
        case_sensitive: bool,
        whole_word: bool,
    ) -> Result<Vec<Range<usize>>, regex::Error> {
        if query.is_empty() || max_matches == 0 {
            return Ok(Vec::new());
        }

        let regex = find_regex(query, case_sensitive)?;
        if regex_query_is_line_local(query) {
            return Ok(self.find_line_local_regex_matches_with_options(
                &regex,
                max_matches,
                whole_word,
            ));
        }
        if !self.regex_full_text_fallback_allowed() {
            return Ok(Vec::new());
        }

        let text = self.text();
        Ok(regex_match_ranges(
            &regex,
            &text,
            whole_word,
            max_matches,
            |start, end| self.is_whole_word_match(&text, start, end),
        ))
    }

    fn find_line_local_regex_matches_with_options(
        &self,
        regex: &Regex,
        max_matches: usize,
        whole_word: bool,
    ) -> Vec<Range<usize>> {
        let mut matches = Vec::with_capacity(find_result_capacity(max_matches));
        let len_lines = self.len_lines();
        let mut line_start_char = 0usize;
        for line_idx in 0..len_lines {
            if matches.len() >= max_matches {
                break;
            }

            let line_slice = self.rope.line(line_idx);
            let next_line_start_char = line_start_char + line_slice.len_chars();
            let line = rope_slice_text(&line_slice);
            let line = line.as_ref();
            for matched in regex.find_iter(line) {
                if matches.len() >= max_matches {
                    break;
                }
                if matched.is_empty() {
                    continue;
                }
                if whole_word && !self.is_whole_word_match(line, matched.start(), matched.end()) {
                    continue;
                }
                let Some(range) = byte_range_to_char_range(line, matched.start()..matched.end())
                else {
                    continue;
                };
                matches.push(line_start_char + range.start..line_start_char + range.end);
            }
            line_start_char = next_line_start_char;
        }
        matches
    }

    pub fn replace_range(&mut self, range: Range<usize>, replacement: &str) -> bool {
        self.replace_range_with_options(range, replacement, false)
            .is_some()
    }

    pub fn replace_range_with_options(
        &mut self,
        range: Range<usize>,
        replacement: &str,
        preserve_case: bool,
    ) -> Option<usize> {
        if !edit_range_is_valid(&range, self.len_chars()) {
            return None;
        }

        let inserted = if preserve_case && range.start != range.end {
            let matched = self.rope.slice(range.clone()).to_string();
            replacement_with_preserved_case(&matched, replacement, true)
        } else {
            replacement.to_owned()
        };
        let inserted_len = inserted.chars().count();
        self.apply_transaction(vec![TextEdit { range, inserted }])
            .then_some(inserted_len)
    }

    pub fn replace_all_matches(
        &mut self,
        query: &str,
        replacement: &str,
        case_sensitive: bool,
        whole_word: bool,
        max_matches: usize,
    ) -> usize {
        self.replace_all_matches_with_options(
            query,
            replacement,
            case_sensitive,
            whole_word,
            max_matches,
            false,
        )
    }

    pub fn replace_all_matches_with_options(
        &mut self,
        query: &str,
        replacement: &str,
        case_sensitive: bool,
        whole_word: bool,
        max_matches: usize,
        preserve_case: bool,
    ) -> usize {
        let matches =
            self.find_matches_with_options(query, max_matches, case_sensitive, whole_word);
        if matches.is_empty() {
            return 0;
        }

        let count = matches.len();
        let edits = matches
            .into_iter()
            .map(|range| TextEdit {
                inserted: if preserve_case {
                    let matched = self.rope.slice(range.start..range.end).to_string();
                    replacement_with_preserved_case(&matched, replacement, true)
                } else {
                    replacement.to_owned()
                },
                range,
            })
            .collect::<Vec<_>>();
        if self.apply_transaction(edits) {
            count
        } else {
            0
        }
    }

    pub fn replace_match_ranges<I>(&mut self, ranges: I, replacement: &str) -> usize
    where
        I: IntoIterator<Item = Range<usize>>,
    {
        self.replace_match_ranges_with_options(ranges, replacement, false)
    }

    pub fn replace_match_ranges_with_options<I>(
        &mut self,
        ranges: I,
        replacement: &str,
        preserve_case: bool,
    ) -> usize
    where
        I: IntoIterator<Item = Range<usize>>,
    {
        let len_chars = self.len_chars();
        let mut edits = Vec::new();
        for range in ranges {
            if !edit_range_is_valid(&range, len_chars) {
                return 0;
            }

            let inserted = if preserve_case {
                if range.start != range.end {
                    let matched = self.rope.slice(range.clone()).to_string();
                    replacement_with_preserved_case(&matched, replacement, true)
                } else {
                    replacement.to_owned()
                }
            } else {
                replacement.to_owned()
            };
            edits.push(TextEdit { range, inserted });
        }
        let count = edits.len();
        if count > 0 && self.apply_transaction(edits) {
            count
        } else {
            0
        }
    }

    pub fn replace_regex_match(
        &mut self,
        range: Range<usize>,
        query: &str,
        replacement: &str,
        case_sensitive: bool,
        whole_word: bool,
        preserve_case: bool,
    ) -> Result<Option<usize>, regex::Error> {
        if query.is_empty() {
            return Ok(None);
        }

        let regex = find_regex(query, case_sensitive)?;
        if regex_query_is_line_local(query) {
            let Some(replaced) = self.line_local_regex_replacement_for_range(
                &regex,
                range.start..range.end,
                replacement,
                whole_word,
                preserve_case,
            ) else {
                return Ok(None);
            };
            let replacement_len = replaced.chars().count();
            return Ok(if self.replace_range(range, &replaced) {
                Some(replacement_len)
            } else {
                None
            });
        }

        if !self.regex_full_text_fallback_allowed() {
            return Ok(None);
        }

        let text = self.text();
        let Some(byte_range) = char_range_to_byte_range(&text, &range) else {
            return Ok(None);
        };
        let Some(replaced) = regex_replacement_for_byte_range(
            &regex,
            &text,
            byte_range,
            replacement,
            whole_word,
            preserve_case,
            |start, end| self.is_whole_word_match(&text, start, end),
        ) else {
            return Ok(None);
        };
        let replacement_len = replaced.chars().count();
        if self.replace_range(range, &replaced) {
            Ok(Some(replacement_len))
        } else {
            Ok(None)
        }
    }

    pub fn replace_all_regex_matches(
        &mut self,
        query: &str,
        replacement: &str,
        case_sensitive: bool,
        whole_word: bool,
        scope: Option<Range<usize>>,
        max_matches: usize,
    ) -> Result<usize, regex::Error> {
        self.replace_all_regex_matches_with_options(
            query,
            replacement,
            RegexReplaceAllOptions {
                case_sensitive,
                whole_word,
                scope,
                max_matches,
                preserve_case: false,
            },
        )
    }

    pub fn replace_all_regex_matches_with_options(
        &mut self,
        query: &str,
        replacement: &str,
        options: RegexReplaceAllOptions,
    ) -> Result<usize, regex::Error> {
        if query.is_empty() || options.max_matches == 0 {
            return Ok(0);
        }

        let regex = find_regex(query, options.case_sensitive)?;
        let edits = if regex_query_is_line_local(query) {
            self.line_local_regex_replace_edits(&regex, replacement, &options)
        } else {
            if !self.regex_full_text_fallback_allowed() {
                return Ok(0);
            }
            let text = self.text();
            let scope_byte_range = options
                .scope
                .as_ref()
                .and_then(|range| char_range_to_byte_range(&text, range));
            regex_replace_edits(
                &regex,
                &text,
                replacement,
                RegexReplaceEditOptions {
                    whole_word: options.whole_word,
                    scope: scope_byte_range,
                    max_matches: options.max_matches,
                    preserve_case: options.preserve_case,
                },
                |start, end| self.is_whole_word_match(&text, start, end),
            )
        };
        let count = edits.len();
        if count > 0 && self.apply_transaction(edits) {
            Ok(count)
        } else {
            Ok(0)
        }
    }

    fn regex_full_text_fallback_allowed(&self) -> bool {
        self.len_bytes() <= REGEX_FULL_TEXT_MAX_BYTES
    }

    fn line_local_regex_replacement_for_range(
        &self,
        regex: &Regex,
        range: Range<usize>,
        replacement: &str,
        whole_word: bool,
        preserve_case: bool,
    ) -> Option<String> {
        let range = normalize_char_range(&range, self.len_chars());
        if range.start >= range.end {
            return None;
        }

        let start_line = self.char_position(range.start).line;
        let end_line = self.char_position(range.end.saturating_sub(1)).line;
        if start_line != end_line {
            return None;
        }

        let line = self.rope.line(start_line);
        let line = rope_slice_text(&line);
        let line = line.as_ref();
        let line_start_char = self.rope.line_to_char(start_line);
        let line_range = range.start - line_start_char..range.end - line_start_char;
        let byte_range = char_range_to_byte_range(line, &line_range)?;
        regex_replacement_for_byte_range(
            regex,
            line,
            byte_range,
            replacement,
            whole_word,
            preserve_case,
            |start, end| self.is_whole_word_match(line, start, end),
        )
    }

    fn line_local_regex_replace_edits(
        &self,
        regex: &Regex,
        replacement: &str,
        options: &RegexReplaceAllOptions,
    ) -> Vec<TextEdit> {
        let mut edits = Vec::with_capacity(find_result_capacity(options.max_matches));
        let len_lines = self.len_lines();
        let mut line_start_char = 0usize;
        for line_idx in 0..len_lines {
            if edits.len() >= options.max_matches {
                break;
            }

            let line_slice = self.rope.line(line_idx);
            let line_end_char = line_start_char + line_slice.len_chars();
            let line = rope_slice_text(&line_slice);
            let line = line.as_ref();
            if let Some(scope) = options.scope.as_ref()
                && (line_start_char >= scope.end || line_end_char <= scope.start)
            {
                line_start_char = line_end_char;
                continue;
            }

            for captures in regex.captures_iter(line) {
                if edits.len() >= options.max_matches {
                    break;
                }
                let Some(matched) = captures.get(0) else {
                    continue;
                };
                if matched.is_empty() {
                    continue;
                }
                if options.whole_word
                    && !self.is_whole_word_match(line, matched.start(), matched.end())
                {
                    continue;
                }

                let Some(range) = byte_range_to_char_range(line, matched.start()..matched.end())
                else {
                    continue;
                };
                let range = line_start_char + range.start..line_start_char + range.end;
                if let Some(scope) = options.scope.as_ref()
                    && (range.start < scope.start || range.end > scope.end)
                {
                    continue;
                }

                edits.push(TextEdit {
                    range,
                    inserted: regex_replacement_text(
                        &captures,
                        replacement,
                        matched.as_str(),
                        options.preserve_case,
                    ),
                });
            }
            line_start_char = line_end_char;
        }
        edits
    }
}

const FIND_REGEX_MEMO_CAPACITY: usize = 32;

type FindRegexMemo = Vec<((String, bool), Regex)>;

static FIND_REGEX_MEMO: LazyLock<Mutex<FindRegexMemo>> = LazyLock::new(|| Mutex::new(Vec::new()));

fn find_regex_memo() -> MutexGuard<'static, FindRegexMemo> {
    FIND_REGEX_MEMO
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn find_regex_memo_entry_index(
    memo: &FindRegexMemo,
    query: &str,
    case_sensitive: bool,
) -> Option<usize> {
    memo.iter().position(|((stored_query, stored_case), _)| {
        *stored_case == case_sensitive && stored_query.as_str() == query
    })
}

fn find_regex_memo_lookup(query: &str, case_sensitive: bool) -> Option<Regex> {
    let mut memo = find_regex_memo();
    let index = find_regex_memo_entry_index(&memo, query, case_sensitive)?;
    let entry = memo.remove(index);
    memo.push(entry);
    memo.last().map(|(_, regex)| regex.clone())
}

fn find_regex_memo_store(query: &str, case_sensitive: bool, regex: Regex) {
    let mut memo = find_regex_memo();
    if let Some(index) = find_regex_memo_entry_index(&memo, query, case_sensitive) {
        memo.remove(index);
    }
    if memo.len() >= FIND_REGEX_MEMO_CAPACITY {
        memo.remove(0);
    }
    memo.push(((query.to_owned(), case_sensitive), regex));
}

pub(super) fn find_regex(query: &str, case_sensitive: bool) -> Result<Regex, regex::Error> {
    if let Some(regex) = find_regex_memo_lookup(query, case_sensitive) {
        return Ok(regex);
    }

    let regex = RegexBuilder::new(query)
        .case_insensitive(!case_sensitive)
        .build()?;
    find_regex_memo_store(query, case_sensitive, regex.clone());
    Ok(regex)
}

#[cfg(test)]
pub(crate) fn find_regex_memo_contains(query: &str, case_sensitive: bool) -> bool {
    find_regex_memo_entry_index(&find_regex_memo(), query, case_sensitive).is_some()
}

pub fn validate_find_regex(query: &str, case_sensitive: bool) -> Result<(), regex::Error> {
    find_regex(query, case_sensitive).map(|_| ())
}

pub(crate) fn regex_query_is_line_local(query: &str) -> bool {
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
                if regex_escape_can_match_line_break(escaped) {
                    return false;
                }
            }
            '[' => {
                if !regex_class_is_line_local(&mut chars) {
                    return false;
                }
            }
            '(' if chars.peek() == Some(&'?') => {
                if regex_inline_flags_can_enable_dotall(&mut chars) {
                    return false;
                }
            }
            _ => {}
        }
    }

    true
}

fn regex_escape_can_match_line_break(escaped: char) -> bool {
    matches!(escaped, 'n' | 'r' | 's' | 'D' | 'W' | 'P' | 'p' | 'x' | 'u')
}

fn regex_class_is_line_local(chars: &mut std::iter::Peekable<impl Iterator<Item = char>>) -> bool {
    if chars.peek() == Some(&'^') {
        return false;
    }

    let mut escaped = false;
    for ch in chars.by_ref() {
        if escaped {
            escaped = false;
            if regex_escape_can_match_line_break(ch) {
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

fn regex_inline_flags_can_enable_dotall(
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

pub(super) fn regex_match_ranges(
    regex: &Regex,
    text: &str,
    whole_word: bool,
    max_matches: usize,
    whole_word_match: impl Fn(usize, usize) -> bool,
) -> Vec<Range<usize>> {
    let mut matches = Vec::with_capacity(find_result_capacity(max_matches));
    for matched in regex
        .find_iter(text)
        .filter(|matched| !matched.is_empty())
        .filter(|matched| !whole_word || whole_word_match(matched.start(), matched.end()))
        .take(max_matches)
    {
        if let Some(range) = byte_range_to_char_range(text, matched.start()..matched.end()) {
            matches.push(range);
        }
    }
    matches
}

pub(super) fn find_result_capacity(max_matches: usize) -> usize {
    max_matches.min(FIND_RESULT_PREALLOC_LIMIT)
}

pub(super) fn regex_replacement_for_byte_range(
    regex: &Regex,
    text: &str,
    range: Range<usize>,
    replacement: &str,
    whole_word: bool,
    preserve_case: bool,
    whole_word_match: impl Fn(usize, usize) -> bool,
) -> Option<String> {
    regex.captures_iter(text).find_map(|captures| {
        let matched = captures.get(0)?;
        if matched.start() != range.start || matched.end() != range.end || matched.is_empty() {
            return None;
        }
        if whole_word && !whole_word_match(matched.start(), matched.end()) {
            return None;
        }
        Some(regex_replacement_text(
            &captures,
            replacement,
            matched.as_str(),
            preserve_case,
        ))
    })
}

pub(super) struct RegexReplaceEditOptions {
    pub(super) whole_word: bool,
    pub(super) scope: Option<Range<usize>>,
    pub(super) max_matches: usize,
    pub(super) preserve_case: bool,
}

pub(super) fn regex_replace_edits(
    regex: &Regex,
    text: &str,
    replacement: &str,
    options: RegexReplaceEditOptions,
    whole_word_match: impl Fn(usize, usize) -> bool,
) -> Vec<TextEdit> {
    let RegexReplaceEditOptions {
        whole_word,
        scope,
        max_matches,
        preserve_case,
    } = options;

    regex
        .captures_iter(text)
        .filter_map(|captures| {
            let matched = captures.get(0)?;
            if matched.is_empty() {
                return None;
            }
            if whole_word && !whole_word_match(matched.start(), matched.end()) {
                return None;
            }
            if let Some(scope) = scope.as_ref()
                && (matched.start() < scope.start || matched.end() > scope.end)
            {
                return None;
            }
            let range = byte_range_to_char_range(text, matched.start()..matched.end())?;
            Some(TextEdit {
                range,
                inserted: regex_replacement_text(
                    &captures,
                    replacement,
                    matched.as_str(),
                    preserve_case,
                ),
            })
        })
        .take(max_matches)
        .collect()
}

pub(super) fn regex_replacement_text(
    captures: &Captures<'_>,
    replacement: &str,
    matched_text: &str,
    preserve_case: bool,
) -> String {
    let mut replaced = String::new();
    captures.expand(replacement, &mut replaced);
    replacement_with_preserved_case(matched_text, &replaced, preserve_case)
}

pub(super) fn replacement_with_preserved_case(
    matched_text: &str,
    replacement: &str,
    preserve_case: bool,
) -> String {
    if !preserve_case || replacement.is_empty() {
        return replacement.to_owned();
    }

    let mut first_letter = None;
    let mut has_uppercase = false;
    let mut has_lowercase = false;
    let mut uppercase_after_first = false;
    for ch in matched_text.chars().filter(|ch| ch.is_alphabetic()) {
        if first_letter.is_none() {
            first_letter = Some(ch);
        } else if ch.is_uppercase() {
            uppercase_after_first = true;
        }
        has_uppercase |= ch.is_uppercase();
        has_lowercase |= ch.is_lowercase();
    }
    let Some(first_letter) = first_letter else {
        return replacement.to_owned();
    };

    if has_uppercase && !has_lowercase {
        return replacement.chars().flat_map(char::to_uppercase).collect();
    }

    if has_lowercase && !has_uppercase {
        return replacement.chars().flat_map(char::to_lowercase).collect();
    }

    let title_case = first_letter.is_uppercase() && !uppercase_after_first;
    if title_case {
        return capitalize_first_alpha(
            &replacement
                .chars()
                .flat_map(char::to_lowercase)
                .collect::<String>(),
        );
    }

    replacement.to_owned()
}

fn capitalize_first_alpha(text: &str) -> String {
    let mut capitalized = String::new();
    let mut changed = false;
    for ch in text.chars() {
        if !changed && ch.is_alphabetic() {
            capitalized.extend(ch.to_uppercase());
            changed = true;
        } else {
            capitalized.push(ch);
        }
    }
    capitalized
}
