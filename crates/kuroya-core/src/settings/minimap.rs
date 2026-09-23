use crate::buffer::TextBuffer;
use regex::Regex;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, OnceLock};

pub(super) const MINIMAP_SECTION_HEADER_SCAN_CHAR_LIMIT: usize = 2_048;
const MAX_CACHED_MARK_SECTION_HEADER_REGEXES: usize = 16;

fn minimap_mark_regex_cache() -> &'static Mutex<HashMap<String, Option<Regex>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Regex>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Compiles each distinct mark-header pattern at most once per process. The
/// pattern is user-configurable, so entries are keyed by pattern, and failed
/// compiles are cached too so an invalid setting is not re-parsed on every
/// rescan.
fn minimap_cached_mark_regex(pattern: &str) -> Option<Regex> {
    let mut cache = minimap_mark_regex_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if cache.len() >= MAX_CACHED_MARK_SECTION_HEADER_REGEXES && !cache.contains_key(pattern) {
        cache.clear();
    }
    cache
        .entry(pattern.to_owned())
        .or_insert_with(|| Regex::new(pattern).ok())
        .clone()
}

#[cfg(test)]
pub(super) fn minimap_cached_mark_regexes_for_test() -> Vec<(String, bool)> {
    let cache = minimap_mark_regex_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut entries: Vec<_> = cache
        .iter()
        .map(|(pattern, regex)| (pattern.clone(), regex.is_some()))
        .collect();
    entries.sort();
    entries
}

pub fn minimap_section_header_lines(
    buffer: &TextBuffer,
    show_region_headers: bool,
    show_mark_headers: bool,
    mark_section_header_regex: &str,
) -> BTreeMap<usize, String> {
    if !show_region_headers && !show_mark_headers {
        return BTreeMap::new();
    }

    let mark_regex = show_mark_headers
        .then(|| minimap_cached_mark_regex(mark_section_header_regex))
        .flatten();
    let mut headers = BTreeMap::new();
    for line_idx in 0..buffer.len_lines() {
        let Some(line) =
            buffer.line_content_prefix(line_idx, MINIMAP_SECTION_HEADER_SCAN_CHAR_LIMIT)
        else {
            continue;
        };
        let line = line.trim_end_matches(['\r', '\n']);
        let label = if show_region_headers {
            minimap_region_section_header_label(line)
        } else {
            None
        }
        .or_else(|| {
            mark_regex
                .as_ref()
                .and_then(|regex| minimap_mark_section_header_label(line, regex))
        });

        if let Some(label) = label {
            headers.insert(line_idx + 1, label);
        }
    }
    headers
}

fn minimap_region_section_header_label(line: &str) -> Option<String> {
    let start = find_ascii_ignore_case(line, "#region")?;
    let label_start = start + "#region".len();
    let label = clean_minimap_section_label(&line[label_start..]);
    Some(if label.is_empty() {
        "region".to_owned()
    } else {
        label
    })
}

fn minimap_mark_section_header_label(line: &str, regex: &Regex) -> Option<String> {
    let captures = regex.captures(line)?;
    let label = captures
        .name("label")
        .or_else(|| captures.get(1))
        .or_else(|| captures.get(0))
        .map(|matched| clean_minimap_section_label(matched.as_str()))
        .unwrap_or_default();
    Some(if label.is_empty() {
        "MARK".to_owned()
    } else {
        label
    })
}

fn clean_minimap_section_label(label: &str) -> String {
    label
        .trim()
        .trim_start_matches(['-', ':', '#'])
        .trim()
        .trim_end_matches("*/")
        .trim_end_matches("-->")
        .trim_end_matches(['-', '*', '/', '>'])
        .trim()
        .chars()
        .filter(|ch| !ch.is_control())
        .take(80)
        .collect()
}

fn find_ascii_ignore_case(value: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    value.as_bytes().windows(needle.len()).position(|window| {
        window
            .iter()
            .zip(needle.bytes())
            .all(|(left, right)| left.eq_ignore_ascii_case(&right))
    })
}
