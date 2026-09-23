use crate::path_display::sanitized_display_label_cow;
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::VecDeque, path::PathBuf};

pub(crate) const MAX_PROJECT_SEARCH_RECENT_QUERIES: usize = 24;
pub(crate) const MAX_PROJECT_SEARCH_QUERY_CHARS: usize = 1024;
pub(crate) const MAX_PROJECT_SEARCH_RECENT_QUERY_CHARS: usize = 256;
pub(crate) const MAX_PROJECT_SEARCH_RECENT_LABEL_CHARS: usize = 80;
pub(crate) const MAX_PROJECT_SEARCH_STATUS_QUERY_CHARS: usize = 96;
pub(crate) const MAX_PROJECT_SEARCH_GLOBS: usize = 64;
pub(crate) const MAX_PROJECT_SEARCH_GLOB_CHARS: usize = 256;
const PROJECT_SEARCH_TEXT_SCAN_MULTIPLIER: usize = 4;
const PROJECT_SEARCH_RECENT_SCAN_MULTIPLIER: usize = 4;
const MAX_PROJECT_SEARCH_GLOB_INPUT_SCAN_CHARS: usize =
    MAX_PROJECT_SEARCH_GLOBS * (MAX_PROJECT_SEARCH_GLOB_CHARS + 4);
const MAX_PROJECT_SEARCH_GLOB_PART_SCAN_CHARS: usize =
    MAX_PROJECT_SEARCH_GLOB_CHARS * PROJECT_SEARCH_TEXT_SCAN_MULTIPLIER;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSearchQuery {
    pub query: String,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub whole_word: bool,
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub include: String,
    #[serde(default)]
    pub exclude: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectSearchKey {
    pub(crate) root: PathBuf,
    pub(crate) query: String,
    pub(crate) case_sensitive: bool,
    pub(crate) whole_word: bool,
    pub(crate) regex: bool,
    pub(crate) include_globs: Vec<String>,
    pub(crate) exclude_globs: Vec<String>,
    pub(crate) max_file_bytes: u64,
    pub(crate) max_results: usize,
}

#[cfg(test)]
pub(crate) fn project_search_query_record(
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
    include: &str,
    exclude: &str,
) -> Option<ProjectSearchQuery> {
    let query = normalize_project_search_query_text(query)?;
    Some(ProjectSearchQuery {
        query,
        case_sensitive,
        whole_word,
        regex: false,
        include: normalize_project_search_glob_draft(include),
        exclude: normalize_project_search_glob_draft(exclude),
    })
}

pub(crate) fn record_recent_project_search_from_parsed_globs(
    recent: &mut VecDeque<ProjectSearchQuery>,
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
    regex: bool,
    include_globs: &[String],
    exclude_globs: &[String],
    max_entries: usize,
) {
    let max_entries = project_search_recent_limit(max_entries);
    if max_entries == 0 {
        recent.clear();
        return;
    }
    let Some(query) = normalize_project_search_query_text(query) else {
        return;
    };
    let entry = ProjectSearchQuery {
        query,
        case_sensitive,
        whole_word,
        regex,
        include: project_search_glob_draft_from_parts(include_globs),
        exclude: project_search_glob_draft_from_parts(exclude_globs),
    };
    record_recent_normalized_project_search(recent, entry, max_entries);
}

#[cfg(test)]
pub(crate) fn record_recent_project_search(
    recent: &mut VecDeque<ProjectSearchQuery>,
    entry: ProjectSearchQuery,
    max_entries: usize,
) {
    let max_entries = project_search_recent_limit(max_entries);
    if max_entries == 0 {
        recent.clear();
        return;
    }
    let Some(entry) = normalized_project_search_query(entry) else {
        return;
    };
    record_recent_normalized_project_search(recent, entry, max_entries);
}

fn record_recent_normalized_project_search(
    recent: &mut VecDeque<ProjectSearchQuery>,
    entry: ProjectSearchQuery,
    max_entries: usize,
) {
    recent.truncate(max_entries);
    if let Some(index) = recent.iter().position(|existing| existing == &entry) {
        recent.remove(index);
    }

    recent.push_front(entry);
    while recent.len() > max_entries {
        recent.pop_back();
    }
}

pub(crate) fn normalize_recent_project_searches(
    entries: impl IntoIterator<Item = ProjectSearchQuery>,
    max_entries: usize,
) -> VecDeque<ProjectSearchQuery> {
    let max_entries = project_search_recent_limit(max_entries);
    if max_entries == 0 {
        return VecDeque::new();
    }

    let mut recent = VecDeque::new();
    for entry in entries
        .into_iter()
        .take(project_search_recent_scan_limit(max_entries))
    {
        let Some(entry) = normalized_project_search_query(entry) else {
            continue;
        };
        if recent.iter().any(|existing| existing == &entry) {
            continue;
        }
        recent.push_back(entry);
        if recent.len() >= max_entries {
            break;
        }
    }
    recent
}

pub(crate) fn project_search_recent_label(entry: &ProjectSearchQuery) -> String {
    let query_label = compact_recent_project_search_query_label_cow(&entry.query);
    if !entry.case_sensitive
        && !entry.whole_word
        && !entry.regex
        && entry.include.is_empty()
        && entry.exclude.is_empty()
    {
        return query_label.into_owned();
    }

    let mut label = String::with_capacity(query_label.len() + 32);
    label.push_str(query_label.as_ref());
    label.push_str(" (");
    let mut needs_separator = false;
    if entry.case_sensitive {
        push_project_search_recent_suffix(&mut label, &mut needs_separator, "case");
    }
    if entry.whole_word {
        push_project_search_recent_suffix(&mut label, &mut needs_separator, "word");
    }
    if entry.regex {
        push_project_search_recent_suffix(&mut label, &mut needs_separator, "regex");
    }
    if !entry.include.is_empty() {
        push_project_search_recent_suffix(&mut label, &mut needs_separator, "include");
    }
    if !entry.exclude.is_empty() {
        push_project_search_recent_suffix(&mut label, &mut needs_separator, "exclude");
    }
    label.push(')');
    compact_project_search_label_from_owned(label)
}

fn push_project_search_recent_suffix(label: &mut String, needs_separator: &mut bool, suffix: &str) {
    if *needs_separator {
        label.push_str(", ");
    }
    label.push_str(suffix);
    *needs_separator = true;
}

#[cfg(test)]
fn compact_recent_project_search_query_label(query: &str) -> String {
    compact_recent_project_search_query_label_cow(query).into_owned()
}

fn compact_recent_project_search_query_label_cow(query: &str) -> Cow<'_, str> {
    match normalize_project_search_text_cow(query.trim(), MAX_PROJECT_SEARCH_RECENT_QUERY_CHARS) {
        Some(Cow::Borrowed(query)) => compact_project_search_label_cow(query),
        Some(Cow::Owned(query)) => Cow::Owned(compact_project_search_label_from_owned(query)),
        None => Cow::Borrowed("search"),
    }
}

pub(crate) fn project_search_status_query_label(query: &str) -> String {
    project_search_display_label(query, MAX_PROJECT_SEARCH_STATUS_QUERY_CHARS, "search text")
}

pub(crate) fn quoted_project_search_query_label(query: &str, max_chars: usize) -> String {
    if max_chars <= 2 {
        return "`".repeat(max_chars);
    }

    let label = project_search_display_label_cow(query, max_chars.saturating_sub(2), "search text");
    let mut quoted = String::with_capacity(label.len().saturating_add(2));
    quoted.push('`');
    quoted.push_str(label.as_ref());
    quoted.push('`');
    quoted
}

fn normalized_project_search_query(entry: ProjectSearchQuery) -> Option<ProjectSearchQuery> {
    let query = normalize_project_search_query_text(&entry.query)?;
    Some(ProjectSearchQuery {
        query,
        case_sensitive: entry.case_sensitive,
        whole_word: entry.whole_word,
        regex: entry.regex,
        include: normalize_project_search_glob_draft(&entry.include),
        exclude: normalize_project_search_glob_draft(&entry.exclude),
    })
}

fn normalize_project_search_glob_draft(input: &str) -> String {
    let mut draft = String::new();
    for_each_project_glob(input, |part| {
        if !draft.is_empty() {
            draft.push_str(", ");
        }
        draft.push_str(part);
        true
    });
    draft
}

fn project_search_glob_draft_from_parts(globs: &[String]) -> String {
    let mut draft =
        String::with_capacity(globs.len().min(MAX_PROJECT_SEARCH_GLOBS).saturating_mul(16));
    let mut seen = Vec::new();
    for glob in globs.iter().take(MAX_PROJECT_SEARCH_GLOBS) {
        let Some(glob) = normalize_project_search_glob_part(glob) else {
            continue;
        };
        if seen.iter().any(|existing| existing == &glob) {
            continue;
        }
        if !draft.is_empty() {
            draft.push_str(", ");
        }
        draft.push_str(&glob);
        seen.push(glob);
    }
    draft
}

#[cfg(test)]
fn compact_project_search_label(label: &str) -> String {
    compact_project_search_label_cow(label).into_owned()
}

fn compact_project_search_label_cow(label: &str) -> Cow<'_, str> {
    project_search_display_label_cow(label, MAX_PROJECT_SEARCH_RECENT_LABEL_CHARS, "search")
}

fn compact_project_search_label_from_owned(label: String) -> String {
    match compact_project_search_label_cow(&label) {
        Cow::Borrowed(_) => label,
        Cow::Owned(label) => label,
    }
}

fn project_search_display_label(label: &str, max_chars: usize, fallback: &str) -> String {
    project_search_display_label_cow(label, max_chars, fallback).into_owned()
}

fn project_search_display_label_cow<'a>(
    label: &'a str,
    max_chars: usize,
    fallback: &str,
) -> Cow<'a, str> {
    sanitized_display_label_cow(label, max_chars, fallback)
}

pub(crate) fn project_search_result_is_current(
    result_query: &str,
    result_index_generation: u64,
    current_index_generation: u64,
    result_case_sensitive: bool,
    result_whole_word: bool,
    result_include_globs: &[String],
    result_exclude_globs: &[String],
    result_max_file_bytes: u64,
    result_max_results: usize,
    current_query: &str,
    current_case_sensitive: bool,
    current_whole_word: bool,
    current_include_globs: &[String],
    current_exclude_globs: &[String],
    current_max_file_bytes: u64,
    current_max_results: usize,
) -> bool {
    if result_index_generation != current_index_generation {
        return false;
    }
    let Some(current_query) = normalize_project_search_request_query_cow(current_query) else {
        return false;
    };
    // Search UI events do not carry the regex flag, so this comparison stays
    // regex-neutral; regex toggles invalidate results through request ids.
    ProjectSearchKey {
        root: PathBuf::new(),
        query: result_query.to_owned(),
        case_sensitive: result_case_sensitive,
        whole_word: result_whole_word,
        regex: false,
        include_globs: result_include_globs.to_vec(),
        exclude_globs: result_exclude_globs.to_vec(),
        max_file_bytes: result_max_file_bytes,
        max_results: result_max_results,
    } == ProjectSearchKey {
        root: PathBuf::new(),
        query: current_query.into_owned(),
        case_sensitive: current_case_sensitive,
        whole_word: current_whole_word,
        regex: false,
        include_globs: current_include_globs.to_vec(),
        exclude_globs: current_exclude_globs.to_vec(),
        max_file_bytes: current_max_file_bytes,
        max_results: current_max_results,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn project_search_request_is_current(
    request_id: u64,
    active_request_id: u64,
    request_index_generation: u64,
    current_index_generation: u64,
    result_query: &str,
    result_case_sensitive: bool,
    result_whole_word: bool,
    result_include_globs: &[String],
    result_exclude_globs: &[String],
    result_max_file_bytes: u64,
    result_max_results: usize,
    current_query: &str,
    current_case_sensitive: bool,
    current_whole_word: bool,
    current_include_globs: &[String],
    current_exclude_globs: &[String],
    current_max_file_bytes: u64,
    current_max_results: usize,
) -> bool {
    request_id != 0
        && request_id == active_request_id
        && project_search_result_is_current(
            result_query,
            request_index_generation,
            current_index_generation,
            result_case_sensitive,
            result_whole_word,
            result_include_globs,
            result_exclude_globs,
            result_max_file_bytes,
            result_max_results,
            current_query,
            current_case_sensitive,
            current_whole_word,
            current_include_globs,
            current_exclude_globs,
            current_max_file_bytes,
            current_max_results,
        )
}

pub(crate) fn next_project_search_request_id(current: u64) -> u64 {
    current.wrapping_add(1).max(1)
}

pub(crate) fn normalize_project_search_request_query(query: &str) -> Option<String> {
    normalize_project_search_request_query_cow(query).map(Cow::into_owned)
}

#[cfg(test)]
pub(crate) fn project_search_query_matches_current(
    result_query: &str,
    current_query: &str,
) -> bool {
    normalize_project_search_request_query_cow(current_query)
        .is_some_and(|current_query| result_query == current_query.as_ref())
}

pub(crate) fn parse_project_globs(input: &str) -> Vec<String> {
    let mut globs = Vec::new();
    for_each_project_glob(input, |part| {
        globs.push(part.to_owned());
        true
    });
    globs
}

#[cfg(test)]
pub(crate) fn project_search_globs_match_current(expected: &[String], input: &str) -> bool {
    let mut expected_index = 0usize;
    let mut matches_expected = true;
    for_each_project_glob(input, |part| {
        if expected.get(expected_index).map(String::as_str) != Some(part) {
            matches_expected = false;
            return false;
        }
        expected_index += 1;
        true
    });
    matches_expected && expected_index == expected.len()
}

fn for_each_project_glob(input: &str, mut visit: impl FnMut(&str) -> bool) {
    let mut seen = Vec::new();
    let mut part = String::new();
    let mut part_chars = 0usize;
    for (scanned_chars, ch) in input.chars().enumerate() {
        if scanned_chars >= MAX_PROJECT_SEARCH_GLOB_INPUT_SCAN_CHARS {
            break;
        }
        if project_search_glob_separator(ch) {
            if !finish_project_glob_part(&mut part, &mut part_chars, &mut seen, &mut visit) {
                return;
            }
            continue;
        }
        if part_chars < MAX_PROJECT_SEARCH_GLOB_PART_SCAN_CHARS {
            part.push(ch);
            part_chars += 1;
        }
    }
    let _ = finish_project_glob_part(&mut part, &mut part_chars, &mut seen, &mut visit);
}

fn finish_project_glob_part(
    part: &mut String,
    part_chars: &mut usize,
    seen: &mut Vec<String>,
    visit: &mut impl FnMut(&str) -> bool,
) -> bool {
    let normalized = normalize_project_search_glob_part(part);
    part.clear();
    *part_chars = 0;

    let Some(part) = normalized else {
        return true;
    };
    if seen.iter().any(|existing| existing == &part) {
        return true;
    }

    if !visit(&part) {
        return false;
    }
    seen.push(part);
    seen.len() < MAX_PROJECT_SEARCH_GLOBS
}

fn project_search_recent_limit(max_entries: usize) -> usize {
    max_entries.min(MAX_PROJECT_SEARCH_RECENT_QUERIES)
}

fn project_search_recent_scan_limit(max_entries: usize) -> usize {
    max_entries
        .saturating_mul(PROJECT_SEARCH_RECENT_SCAN_MULTIPLIER)
        .max(max_entries)
}

fn project_search_text_scan_chars(max_chars: usize) -> usize {
    max_chars
        .saturating_mul(PROJECT_SEARCH_TEXT_SCAN_MULTIPLIER)
        .max(max_chars)
}

fn project_search_glob_separator(ch: char) -> bool {
    ch.is_control() || matches!(ch, ',' | ';' | '\u{2028}' | '\u{2029}')
}

fn normalize_project_search_query_text(query: &str) -> Option<String> {
    normalize_project_search_text(query.trim(), MAX_PROJECT_SEARCH_RECENT_QUERY_CHARS)
}

fn normalize_project_search_request_query_cow(query: &str) -> Option<Cow<'_, str>> {
    normalize_project_search_text_cow(query.trim(), MAX_PROJECT_SEARCH_QUERY_CHARS)
}

fn normalize_project_search_glob_part(part: &str) -> Option<String> {
    normalize_project_search_glob_part_cow(part).map(Cow::into_owned)
}

fn normalize_project_search_glob_part_cow(part: &str) -> Option<Cow<'_, str>> {
    normalize_project_search_text_cow(part.trim(), MAX_PROJECT_SEARCH_GLOB_CHARS)
}

fn normalize_project_search_text(text: &str, max_chars: usize) -> Option<String> {
    normalize_project_search_text_cow(text, max_chars).map(Cow::into_owned)
}

fn normalize_project_search_text_cow(text: &str, max_chars: usize) -> Option<Cow<'_, str>> {
    if text.is_empty() || max_chars == 0 {
        return None;
    }

    if text.len() <= max_chars
        && text.is_ascii()
        && !text.as_bytes().iter().any(u8::is_ascii_control)
    {
        return Some(Cow::Borrowed(text));
    }

    for (char_count, ch) in text.chars().enumerate() {
        if project_search_text_char_needs_sanitizing(ch) || char_count >= max_chars {
            return normalize_project_search_text_slow(text, max_chars).map(Cow::Owned);
        }
    }
    Some(Cow::Borrowed(text))
}

fn normalize_project_search_text_slow(text: &str, max_chars: usize) -> Option<String> {
    let mut normalized = String::with_capacity(text.len().min(max_chars));
    let mut pending_space = false;
    let mut char_count = 0;
    for (scanned_chars, ch) in text.chars().enumerate() {
        if scanned_chars >= project_search_text_scan_chars(max_chars) {
            break;
        }
        if is_project_search_format_control(ch) {
            continue;
        }
        if is_project_search_line_or_control(ch) {
            pending_space = true;
            continue;
        }
        if pending_space && ch == ' ' {
            continue;
        }
        if pending_space {
            if !normalized.is_empty() && !normalized.ends_with(' ') {
                if char_count >= max_chars {
                    break;
                }
                normalized.push(' ');
                char_count += 1;
            }
            pending_space = false;
        }
        if char_count >= max_chars {
            break;
        }
        normalized.push(ch);
        char_count += 1;
    }
    (!normalized.is_empty()).then_some(normalized)
}

fn project_search_text_char_needs_sanitizing(ch: char) -> bool {
    is_project_search_format_control(ch) || is_project_search_line_or_control(ch)
}

fn is_project_search_line_or_control(ch: char) -> bool {
    ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}')
}

fn is_project_search_format_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{061c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
    )
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_PROJECT_SEARCH_GLOB_CHARS, MAX_PROJECT_SEARCH_GLOB_INPUT_SCAN_CHARS,
        MAX_PROJECT_SEARCH_GLOBS, MAX_PROJECT_SEARCH_QUERY_CHARS,
        MAX_PROJECT_SEARCH_RECENT_LABEL_CHARS, MAX_PROJECT_SEARCH_RECENT_QUERIES,
        MAX_PROJECT_SEARCH_STATUS_QUERY_CHARS, ProjectSearchQuery, compact_project_search_label,
        compact_recent_project_search_query_label, compact_recent_project_search_query_label_cow,
        for_each_project_glob, normalize_project_search_glob_part_cow,
        normalize_project_search_request_query, normalize_recent_project_searches,
        parse_project_globs, project_search_display_label, project_search_display_label_cow,
        project_search_globs_match_current, project_search_query_matches_current,
        project_search_query_record, project_search_recent_label, project_search_recent_scan_limit,
        project_search_request_is_current, project_search_status_query_label,
        project_search_text_scan_chars, quoted_project_search_query_label,
        record_recent_project_search_from_parsed_globs,
    };
    use std::{borrow::Cow, collections::VecDeque};

    #[test]
    fn project_search_display_label_cow_borrows_clean_ascii_and_unicode() {
        assert!(matches!(
            project_search_display_label_cow("needle value", 32, "search text"),
            Cow::Borrowed("needle value")
        ));

        let unicode = "needle \u{03bb} value";
        match project_search_display_label_cow(unicode, 32, "search text") {
            Cow::Borrowed(label) => assert_eq!(label, unicode),
            Cow::Owned(label) => panic!("expected borrowed Unicode label, got {label:?}"),
        }
    }

    #[test]
    fn project_search_display_label_cow_owns_dirty_truncated_and_fallback_labels() {
        let cases = [
            ("needle\nvalue".to_owned(), 32, "search text"),
            ("abcdefghijklmnopqrstuvwxyz".to_owned(), 12, "search text"),
            ("   ".to_owned(), 32, "search text"),
            ("\u{202e}\u{2066}".to_owned(), 32, "search text"),
        ];

        for (label, max_chars, fallback) in cases {
            let display = project_search_display_label_cow(&label, max_chars, fallback);

            assert_eq!(
                display.as_ref(),
                project_search_display_label(&label, max_chars, fallback)
            );
            assert!(
                matches!(display, Cow::Owned(_)),
                "expected owned display label for {label:?}"
            );
        }
    }

    #[test]
    fn compact_recent_project_search_query_label_cow_borrows_clean_queries() {
        assert!(matches!(
            compact_recent_project_search_query_label_cow("needle value"),
            Cow::Borrowed("needle value")
        ));

        let unicode = "needle \u{03bb}";
        match compact_recent_project_search_query_label_cow(unicode) {
            Cow::Borrowed(label) => assert_eq!(label, unicode),
            Cow::Owned(label) => panic!("expected borrowed recent query label, got {label:?}"),
        }
    }

    #[test]
    fn compact_recent_project_search_query_label_wrapper_matches_cow_output() {
        let cases = [
            "needle",
            " needle\nvalue ",
            "\u{202e}\u{2066}",
            "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz",
        ];

        for query in cases {
            assert_eq!(
                compact_recent_project_search_query_label(query),
                compact_recent_project_search_query_label_cow(query).as_ref()
            );
            assert_eq!(
                compact_project_search_label(query),
                project_search_display_label(
                    query,
                    MAX_PROJECT_SEARCH_RECENT_LABEL_CHARS,
                    "search"
                )
            );
        }
    }

    #[test]
    fn project_search_status_query_label_is_display_safe_and_bounded() {
        let query = format!(
            "first line\nsecond line \u{202e}{}",
            "query-fragment-".repeat(MAX_PROJECT_SEARCH_STATUS_QUERY_CHARS)
        );

        let label = project_search_status_query_label(&query);

        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));
        assert!(label.contains("first line second line"));
        assert!(label.contains("..."));
        assert!(label.chars().count() <= MAX_PROJECT_SEARCH_STATUS_QUERY_CHARS);
    }

    #[test]
    fn project_search_status_query_label_preserves_clean_unicode() {
        let query = "needle \u{03bb} value";

        assert_eq!(project_search_status_query_label(query), query);
    }

    #[test]
    fn quoted_project_search_query_label_accounts_for_wrapper_chars() {
        let query = format!(
            "needle\n{}\u{202e}",
            "query-fragment-".repeat(MAX_PROJECT_SEARCH_STATUS_QUERY_CHARS)
        );
        let max_chars = 32;

        let label = quoted_project_search_query_label(&query, max_chars);

        assert!(label.starts_with('`'));
        assert!(label.ends_with('`'));
        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));
        assert!(label.contains("..."));
        assert!(label.chars().count() <= max_chars);
    }

    #[test]
    fn quoted_project_search_query_label_preserves_clean_unicode() {
        assert_eq!(
            quoted_project_search_query_label("needle \u{03bb}", 32),
            "`needle \u{03bb}`"
        );
    }

    #[test]
    fn quoted_project_search_query_label_stays_bounded_for_tiny_limits() {
        assert_eq!(quoted_project_search_query_label("needle", 0), "");
        assert_eq!(quoted_project_search_query_label("needle", 1), "`");
        assert_eq!(quoted_project_search_query_label("needle", 2), "``");
    }

    #[test]
    fn project_search_request_query_normalizes_pasted_lines_and_bounds_text() {
        let query = format!(
            "  needle\n  value\t  {}  ",
            "x".repeat(MAX_PROJECT_SEARCH_QUERY_CHARS)
        );

        let normalized =
            normalize_project_search_request_query(&query).expect("normalized request query");

        assert!(normalized.starts_with("needle value "));
        assert!(!normalized.contains("needle  value"));
        assert!(!normalized.chars().any(char::is_control));
        assert!(normalized.chars().count() <= MAX_PROJECT_SEARCH_QUERY_CHARS);
    }

    #[test]
    fn project_search_request_query_bounds_hostile_hidden_control_prefix() {
        let mut query = "\u{202e}".repeat(project_search_text_scan_chars(
            MAX_PROJECT_SEARCH_QUERY_CHARS,
        ));
        query.push_str("needle");

        assert_eq!(normalize_project_search_request_query(&query), None);
    }

    #[test]
    fn project_search_query_match_uses_normalized_current_text() {
        assert!(project_search_query_matches_current(
            "needle value",
            " needle\n  value "
        ));
        assert!(!project_search_query_matches_current(
            "needle  value",
            " needle\n  value "
        ));
        assert!(!project_search_query_matches_current("needle", "\u{202e}"));
    }

    #[test]
    fn project_search_recent_label_keeps_glob_options_display_safe() {
        let label = project_search_recent_label(&ProjectSearchQuery {
            query: format!(
                "needle\n{}\u{202e}",
                "recent-query-".repeat(MAX_PROJECT_SEARCH_RECENT_LABEL_CHARS)
            ),
            case_sensitive: true,
            whole_word: true,
            regex: true,
            include: "src/**\n\u{202e}".to_owned(),
            exclude: "target/**\u{202e}".to_owned(),
        });

        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));
        assert!(label.contains("..."));
        assert!(label.contains("regex"));
        assert!(label.chars().count() <= MAX_PROJECT_SEARCH_RECENT_LABEL_CHARS);
    }

    #[test]
    fn project_search_recent_label_preserves_clean_unicode_query() {
        let label = project_search_recent_label(&ProjectSearchQuery {
            query: "needle \u{03bb}".to_owned(),
            case_sensitive: false,
            whole_word: false,
            regex: false,
            include: String::new(),
            exclude: String::new(),
        });

        assert_eq!(label, "needle \u{03bb}");
    }

    #[test]
    fn project_search_recent_label_marks_regex_only_entries() {
        let label = project_search_recent_label(&ProjectSearchQuery {
            query: "needle".to_owned(),
            case_sensitive: false,
            whole_word: false,
            regex: true,
            include: String::new(),
            exclude: String::new(),
        });

        assert_eq!(label, "needle (regex)");
    }

    #[test]
    fn project_search_recent_label_bounds_raw_history_query_before_display() {
        let label = project_search_recent_label(&ProjectSearchQuery {
            query: "needle".repeat(20_000),
            case_sensitive: true,
            whole_word: true,
            regex: false,
            include: "src/**".to_owned(),
            exclude: "target/**".to_owned(),
        });

        assert!(label.contains("..."));
        assert!(label.ends_with("exclude)"));
        assert!(label.chars().count() <= MAX_PROJECT_SEARCH_RECENT_LABEL_CHARS);
    }

    #[test]
    fn project_search_globs_match_current_dedupes_without_rebuilding_full_glob_list() {
        let expected = vec!["src/**/*.rs".to_owned(), "tests/**/*.rs".to_owned()];

        assert!(project_search_globs_match_current(
            &expected,
            " src/**/*.rs, src/**/*.rs; tests/**/*.rs "
        ));
        assert!(!project_search_globs_match_current(
            &expected,
            "tests/**/*.rs, src/**/*.rs"
        ));
        assert!(project_search_globs_match_current(&[], "  \n ; , "));
    }

    #[test]
    fn project_search_glob_scanner_stops_when_visitor_declines() {
        let mut visited = Vec::new();

        for_each_project_glob(
            " src/**/*.rs, src/**/*.rs; target/**; tests/**/*.rs ",
            |part| {
                visited.push(part.to_owned());
                part != "target/**"
            },
        );

        assert_eq!(visited, vec!["src/**/*.rs", "target/**"]);
    }

    #[test]
    fn project_search_request_currency_rejects_zero_request_ids() {
        assert!(!project_search_request_is_current(
            0,
            0,
            1,
            1,
            "needle",
            false,
            false,
            &[],
            &[],
            1024,
            100,
            "needle",
            false,
            false,
            &[],
            &[],
            1024,
            100,
        ));
    }

    #[test]
    fn project_search_state_normalization_strips_hidden_format_controls() {
        let entry = project_search_query_record(
            " needle\u{202e}\u{2066}\nvalue ",
            true,
            false,
            "src/**\u{2028}tests/**\u{202e}",
            "target/**\u{2029}*.snap\u{2069}",
        )
        .expect("normalized query");

        assert_eq!(entry.query, "needle value");
        assert_eq!(entry.include, "src/**, tests/**");
        assert_eq!(entry.exclude, "target/**, *.snap");
        assert!(!entry.query.contains('\u{202e}'));
        assert!(!entry.include.contains('\u{2028}'));
        assert!(!entry.exclude.contains('\u{2069}'));
    }

    #[test]
    fn project_search_glob_parsing_accepts_unicode_line_separators() {
        assert_eq!(
            parse_project_globs("src/**\u{2028}tests/**\u{2029}*.md"),
            vec!["src/**", "tests/**", "*.md"]
        );
        assert!(project_search_globs_match_current(
            &["src/**".to_owned(), "tests/**".to_owned()],
            "src/**\u{2028}tests/**"
        ));
    }

    #[test]
    fn project_search_glob_parsing_treats_control_chars_as_separators() {
        assert_eq!(
            parse_project_globs("src/**\ttests/**\0*.md"),
            vec!["src/**", "tests/**", "*.md"]
        );
        assert!(project_search_globs_match_current(
            &[
                "src/**".to_owned(),
                "tests/**".to_owned(),
                "*.md".to_owned()
            ],
            "src/**\ttests/**\0*.md"
        ));
    }

    #[test]
    fn project_search_glob_parsing_bounds_separator_only_prefixes() {
        let mut input = ",".repeat(MAX_PROJECT_SEARCH_GLOB_INPUT_SCAN_CHARS);
        input.push_str("src/**");

        assert!(parse_project_globs(&input).is_empty());
        assert!(project_search_globs_match_current(&[], &input));
    }

    #[test]
    fn project_search_glob_match_normalization_borrows_clean_globs() {
        assert!(matches!(
            normalize_project_search_glob_part_cow(" src/**/*.rs "),
            Some(Cow::Borrowed("src/**/*.rs"))
        ));

        let normalized =
            normalize_project_search_glob_part_cow("src/**\nmain.rs").expect("normalized glob");
        assert!(matches!(&normalized, Cow::Owned(_)));
        assert_eq!(normalized.as_ref(), "src/** main.rs");
    }

    #[test]
    fn project_search_recent_from_parsed_globs_bounds_pathological_parts() {
        let long_glob = format!(
            "src/{}\nmain.rs",
            "x".repeat(MAX_PROJECT_SEARCH_GLOB_CHARS * 2)
        );
        let include = std::iter::once(long_glob)
            .chain((0..MAX_PROJECT_SEARCH_GLOBS + 8).map(|index| format!("generated/{index}/**")))
            .collect::<Vec<_>>();
        let exclude = vec![
            "target/**\u{202e}".to_owned(),
            "target/**\u{202e}".to_owned(),
        ];
        let mut recent = VecDeque::new();

        record_recent_project_search_from_parsed_globs(
            &mut recent,
            "needle",
            false,
            false,
            false,
            &include,
            &exclude,
            MAX_PROJECT_SEARCH_RECENT_QUERIES,
        );

        let entry = recent.front().expect("recent project search");
        let include_parts = parse_project_globs(&entry.include);
        let exclude_parts = parse_project_globs(&entry.exclude);
        assert_eq!(include_parts.len(), MAX_PROJECT_SEARCH_GLOBS);
        assert!(
            include_parts
                .iter()
                .all(|glob| glob.chars().count() <= MAX_PROJECT_SEARCH_GLOB_CHARS)
        );
        assert_eq!(exclude_parts, vec!["target/**"]);
    }

    #[test]
    fn project_search_recent_normalization_stops_after_bounded_candidates() {
        let hostile_entry = ProjectSearchQuery {
            query: "\u{202e}".repeat(project_search_text_scan_chars(
                MAX_PROJECT_SEARCH_RECENT_LABEL_CHARS,
            )),
            case_sensitive: false,
            whole_word: false,
            regex: false,
            include: String::new(),
            exclude: String::new(),
        };
        let mut entries = vec![
            hostile_entry;
            project_search_recent_scan_limit(MAX_PROJECT_SEARCH_RECENT_QUERIES)
        ];
        entries.push(ProjectSearchQuery {
            query: "needle".to_owned(),
            case_sensitive: false,
            whole_word: false,
            regex: false,
            include: String::new(),
            exclude: String::new(),
        });

        let recent = normalize_recent_project_searches(entries, MAX_PROJECT_SEARCH_RECENT_QUERIES);

        assert!(recent.is_empty());
    }
}
