use crate::lsp_completion_imports::completion_has_auto_import_edit;
use kuroya_core::{
    EditorSettings, EditorSnippetSuggestions, EditorSuggestSelection, EditorSuggestSelectionMode,
    LspCompletionItem, TextBuffer,
};
use std::{borrow::Cow, cell::OnceCell, cmp::Ordering};

const COMPLETION_LOCALITY_MAX_DISTANCE: usize = 50;
const COMPLETION_RANKING_TEXT_MAX_CHARS: usize = 512;
const COMPLETION_RANKING_TEXT_SAMPLE_BYTES: usize = 4 * 1024;
const COMPLETION_RANKING_PREFIX_MAX_CHARS: usize = 256;
const COMPLETION_LOCALITY_LINE_SAMPLE_BYTES: usize = 8 * 1024;

pub(crate) fn completion_prefix_at(buffer: &TextBuffer, line: usize, column: usize) -> String {
    let before_cursor = buffer
        .line_content_prefix(line, column.saturating_sub(1))
        .unwrap_or_default();
    let start = before_cursor
        .char_indices()
        .rfind(|(_, ch)| !completion_word_char(*ch, buffer.word_separators()))
        .map(|(idx, ch)| idx + ch.len_utf8())
        .unwrap_or(0);

    if start == 0 {
        before_cursor
    } else {
        before_cursor[start..].to_owned()
    }
}

pub(crate) fn rank_completion_items(
    items: &mut [LspCompletionItem],
    prefix: &str,
    settings: &EditorSettings,
) {
    rank_completion_items_with_locality(items, prefix, settings, None);
}

pub(crate) fn rank_completion_items_for_buffer(
    items: &mut [LspCompletionItem],
    prefix: &str,
    settings: &EditorSettings,
    buffer: &TextBuffer,
    line: usize,
) {
    if items.len() <= 1 {
        return;
    }

    let locality = settings
        .suggest_locality_bonus
        .then(|| CompletionLocality::new(buffer, line));
    rank_completion_items_with_locality(items, prefix, settings, locality.as_ref());
}

fn rank_completion_items_with_locality(
    items: &mut [LspCompletionItem],
    prefix: &str,
    settings: &EditorSettings,
    locality: Option<&CompletionLocality>,
) {
    if items.len() <= 1 {
        return;
    }

    let prefix = CompletionPrefix::new(prefix);

    if items.iter().any(|item| item.sort_text.is_some()) {
        items.sort_by(compare_completion_sort_text);
    }
    if prefix.is_empty() && completion_empty_prefix_keeps_display_order(items, settings, locality) {
        return;
    }
    items.sort_by_cached_key(|item| {
        (
            completion_snippet_rank(item, settings.snippet_suggestions),
            completion_rank(item, &prefix, locality),
        )
    });
}

fn completion_empty_prefix_keeps_display_order(
    items: &[LspCompletionItem],
    settings: &EditorSettings,
    locality: Option<&CompletionLocality>,
) -> bool {
    locality.is_none()
        && matches!(
            settings.snippet_suggestions,
            EditorSnippetSuggestions::Inline | EditorSnippetSuggestions::None
        )
        && !items
            .iter()
            .any(|item| item.preselect || completion_has_auto_import_edit(item))
}

pub(crate) fn selected_completion_index(
    items: &[LspCompletionItem],
    prefix: &str,
    settings: &EditorSettings,
    recent_labels: &std::collections::VecDeque<String>,
    recent_prefix_labels: &std::collections::VecDeque<(String, String)>,
) -> usize {
    if items.is_empty()
        || matches!(
            settings.suggest_selection_mode,
            EditorSuggestSelectionMode::Never
        )
    {
        return 0;
    }

    match settings.suggest_selection {
        EditorSuggestSelection::First => 0,
        EditorSuggestSelection::RecentlyUsed => {
            recent_completion_index(items, recent_labels).unwrap_or(0)
        }
        EditorSuggestSelection::RecentlyUsedByPrefix => {
            let prefix = completion_normalized_lowercase(prefix);
            recent_prefix_labels
                .iter()
                .find(|(candidate_prefix, _)| candidate_prefix.as_str() == prefix.as_ref())
                .and_then(|(_, label)| completion_label_index(items, label))
                .or_else(|| recent_completion_index(items, recent_labels))
                .unwrap_or(0)
        }
    }
}

pub(crate) fn filter_completion_items_by_settings(
    items: &mut Vec<LspCompletionItem>,
    settings: &EditorSettings,
    prefix: &str,
) {
    if items.is_empty() {
        return;
    }

    let prefix = prefix.trim();
    if prefix.is_empty() {
        items.retain(|item| completion_item_visible_by_settings(item, settings));
        return;
    }

    let prefix = CompletionPrefix::new(prefix);
    items.retain(|item| {
        completion_item_visible_by_settings(item, settings)
            && completion_visible_for_prefix(item, &prefix, settings)
    });
}

fn completion_item_visible_by_settings(
    item: &LspCompletionItem,
    settings: &EditorSettings,
) -> bool {
    completion_kind_visible(item.kind, settings)
        && completion_snippet_visible(item, settings)
        && completion_deprecated_visible(item, settings)
}

fn completion_kind_visible(kind: Option<u8>, settings: &EditorSettings) -> bool {
    match kind {
        None => true,
        Some(1) => settings.suggest_show_words,
        Some(2) => settings.suggest_show_methods,
        Some(3) => settings.suggest_show_functions,
        Some(4) => settings.suggest_show_constructors,
        Some(5) => settings.suggest_show_fields,
        Some(6) => settings.suggest_show_variables,
        Some(7) => settings.suggest_show_classes,
        Some(8) => settings.suggest_show_interfaces,
        Some(9) => settings.suggest_show_modules,
        Some(10) => settings.suggest_show_properties,
        Some(11) => settings.suggest_show_units,
        Some(12) => settings.suggest_show_values,
        Some(13) => settings.suggest_show_enums,
        Some(14) => settings.suggest_show_keywords,
        Some(15) => settings.suggest_show_snippets,
        Some(16) => settings.suggest_show_colors,
        Some(17) => settings.suggest_show_files,
        Some(18) => settings.suggest_show_references,
        Some(19) => settings.suggest_show_folders,
        Some(20) => settings.suggest_show_enum_members,
        Some(21) => settings.suggest_show_constants,
        Some(22) => settings.suggest_show_structs,
        Some(23) => settings.suggest_show_events,
        Some(24) => settings.suggest_show_operators,
        Some(25) => settings.suggest_show_type_parameters,
        Some(_) => true,
    }
}

fn completion_snippet_visible(item: &LspCompletionItem, settings: &EditorSettings) -> bool {
    if !completion_item_is_snippet(item) {
        return true;
    }

    settings.suggest_show_snippets
        && !matches!(settings.snippet_suggestions, EditorSnippetSuggestions::None)
}

fn completion_deprecated_visible(item: &LspCompletionItem, settings: &EditorSettings) -> bool {
    !item.deprecated || settings.suggest_show_deprecated
}

fn completion_visible_for_prefix(
    item: &LspCompletionItem,
    prefix: &CompletionPrefix<'_>,
    settings: &EditorSettings,
) -> bool {
    if prefix.is_empty() {
        return true;
    }

    let candidate = PreparedCompletionCandidate::for_item(item);
    if settings.suggest_match_on_word_start_only {
        return completion_matches_word_start_or_subsequence(candidate.text(), prefix);
    }

    let (tier, _) = completion_match_tier_and_distance(&candidate, prefix);
    tier < 5 || (settings.suggest_filter_graceful && tier == 5)
}

fn completion_matches_word_start_or_subsequence(
    candidate: &str,
    prefix: &CompletionPrefix<'_>,
) -> bool {
    let Some(first_prefix_char) = prefix.first_char else {
        return true;
    };

    let mut expected = prefix.chars();
    let Some(mut current) = expected.next() else {
        return true;
    };

    for (idx, ch) in candidate.char_indices() {
        if !completion_word_start(candidate, idx, ch) {
            continue;
        }
        if completion_lowercase_char_matches(ch, first_prefix_char)
            && completion_lowercase_starts_with(&candidate[idx..], prefix.as_str())
        {
            return true;
        }
        if completion_lowercase_char_matches(ch, current) {
            if let Some(next) = expected.next() {
                current = next;
            } else {
                return true;
            }
        }
    }

    false
}

fn completion_word_start(candidate: &str, idx: usize, ch: char) -> bool {
    if idx == 0 {
        return true;
    }

    let previous = candidate[..idx].chars().next_back();
    previous.is_some_and(|previous| {
        !completion_identifier_char(previous) || (previous.is_lowercase() && ch.is_uppercase())
    })
}

fn completion_identifier_char(ch: char) -> bool {
    ch.is_alphanumeric()
}

fn completion_rank(
    item: &LspCompletionItem,
    prefix: &CompletionPrefix<'_>,
    locality: Option<&CompletionLocality>,
) -> CompletionRank {
    let preselect_rank = usize::from(!item.preselect);
    let auto_import_rank = usize::from(!completion_has_auto_import_edit(item));

    if prefix.is_empty() {
        let locality_rank = locality
            .map(|locality| {
                let candidate = PreparedCompletionCandidate::for_item(item);
                completion_locality_rank(&candidate, locality)
            })
            .unwrap_or(0);
        return CompletionRank {
            match_tier: 0,
            locality_rank,
            preselect_rank,
            auto_import_rank,
            distance: 0,
        };
    }

    let candidate = PreparedCompletionCandidate::for_item(item);
    let locality_rank = locality
        .map(|locality| completion_locality_rank(&candidate, locality))
        .unwrap_or(0);
    let (match_tier, distance) = completion_match_tier_and_distance(&candidate, prefix);
    CompletionRank {
        match_tier,
        locality_rank,
        preselect_rank,
        auto_import_rank,
        distance,
    }
}

fn completion_locality_rank(
    candidate: &PreparedCompletionCandidate<'_>,
    locality: &CompletionLocality,
) -> usize {
    let needle = candidate.normalized_lowercase();
    if needle.is_empty() {
        return usize::MAX;
    }

    locality.rank_for(needle.as_ref()).unwrap_or(usize::MAX)
}

fn completion_match_tier_and_distance(
    candidate: &PreparedCompletionCandidate<'_>,
    prefix: &CompletionPrefix<'_>,
) -> (usize, usize) {
    let candidate_lower = candidate.lowercase();
    let prefix_text = prefix.as_str();
    if candidate_lower == prefix_text {
        return (0, 0);
    }
    if candidate_lower.starts_with(prefix_text) {
        return (
            1,
            candidate
                .lowercase_char_count()
                .saturating_sub(prefix.char_count),
        );
    }
    if let Some(distance) = completion_word_start_prefix_distance(candidate.text(), prefix) {
        return (2, distance);
    }
    if let Some(idx) = candidate_lower.find(prefix_text) {
        return (3, idx);
    }
    if let Some(distance) = completion_word_start_subsequence_distance(candidate.text(), prefix) {
        return (4, distance);
    }
    if let Some(distance) = fuzzy_subsequence_distance(candidate_lower, prefix) {
        return (5, distance);
    }
    (6, 0)
}

fn fuzzy_subsequence_distance(candidate: &str, prefix: &CompletionPrefix<'_>) -> Option<usize> {
    let mut search = candidate.char_indices();
    let mut first_idx = None;
    let mut last_idx = 0usize;
    for expected in prefix.chars() {
        let (idx, _) = search.find(|(_, ch)| *ch == expected)?;
        first_idx.get_or_insert(idx);
        last_idx = idx;
    }
    Some(last_idx.saturating_sub(first_idx.unwrap_or(0)))
}

fn completion_word_start_prefix_distance(
    candidate: &str,
    prefix: &CompletionPrefix<'_>,
) -> Option<usize> {
    let first_prefix_char = prefix.first_char?;
    candidate
        .char_indices()
        .filter(|(idx, ch)| completion_word_start(candidate, *idx, *ch))
        .find_map(|(idx, ch)| {
            (completion_lowercase_char_matches(ch, first_prefix_char)
                && completion_lowercase_starts_with(&candidate[idx..], prefix.as_str()))
            .then_some(idx)
        })
}

fn completion_word_start_subsequence_distance(
    candidate: &str,
    prefix: &CompletionPrefix<'_>,
) -> Option<usize> {
    let mut expected = prefix.chars();
    let mut current = expected.next()?;
    let mut first_idx = None;

    for (idx, ch) in candidate.char_indices() {
        if !completion_word_start(candidate, idx, ch) {
            continue;
        }
        if completion_lowercase_char_matches(ch, current) {
            let first = *first_idx.get_or_insert(idx);
            if let Some(next) = expected.next() {
                current = next;
            } else {
                return Some(idx.saturating_sub(first));
            }
        }
    }

    None
}

#[cfg(test)]
fn completion_filter_text(item: &LspCompletionItem) -> &str {
    item.filter_text
        .as_deref()
        .filter(|text| {
            !completion_ranking_text_sample(text, COMPLETION_RANKING_TEXT_MAX_CHARS).is_empty()
        })
        .unwrap_or(&item.label)
}

fn completion_filter_text_sample(item: &LspCompletionItem) -> &str {
    if let Some(filter_text) = item.filter_text.as_deref() {
        let sample = completion_ranking_text_sample(filter_text, COMPLETION_RANKING_TEXT_MAX_CHARS);
        if !sample.is_empty() {
            return sample;
        }
    }

    completion_ranking_text_sample(&item.label, COMPLETION_RANKING_TEXT_MAX_CHARS)
}

fn completion_snippet_rank(item: &LspCompletionItem, mode: EditorSnippetSuggestions) -> usize {
    match mode {
        EditorSnippetSuggestions::Top => usize::from(!completion_item_is_snippet(item)),
        EditorSnippetSuggestions::Bottom => usize::from(completion_item_is_snippet(item)),
        EditorSnippetSuggestions::Inline | EditorSnippetSuggestions::None => 0,
    }
}

fn completion_item_is_snippet(item: &LspCompletionItem) -> bool {
    item.is_snippet || item.kind == Some(15)
}

fn recent_completion_index(
    items: &[LspCompletionItem],
    recent_labels: &std::collections::VecDeque<String>,
) -> Option<usize> {
    recent_labels
        .iter()
        .find_map(|label| completion_label_index(items, label))
}

fn completion_label_index(items: &[LspCompletionItem], label: &str) -> Option<usize> {
    items.iter().position(|item| item.label == label)
}

fn compare_completion_sort_text(left: &LspCompletionItem, right: &LspCompletionItem) -> Ordering {
    completion_sort_order_text(left).cmp(completion_sort_order_text(right))
}

fn completion_sort_order_text(item: &LspCompletionItem) -> &str {
    completion_order_text_sample(
        item.sort_text.as_deref().unwrap_or(&item.label),
        COMPLETION_RANKING_TEXT_MAX_CHARS,
    )
}

fn completion_word_char(ch: char, separators: &str) -> bool {
    !ch.is_whitespace() && !separators.contains(ch)
}

fn completion_normalized_lowercase(text: &str) -> Cow<'_, str> {
    completion_lowercase_cow(completion_ranking_text_sample(
        text,
        COMPLETION_RANKING_TEXT_MAX_CHARS,
    ))
}

fn completion_lowercase_cow(text: &str) -> Cow<'_, str> {
    if completion_needs_lowercase(text) {
        Cow::Owned(text.to_lowercase())
    } else {
        Cow::Borrowed(text)
    }
}

fn completion_lowercase_starts_with(text: &str, lowercase_prefix: &str) -> bool {
    if lowercase_prefix.is_empty() {
        return true;
    }

    let mut text_lower = text.chars().flat_map(char::to_lowercase);
    lowercase_prefix
        .chars()
        .all(|expected| text_lower.next().is_some_and(|ch| ch == expected))
}

fn completion_lowercase_char_matches(ch: char, lowercase_expected: char) -> bool {
    ch.to_lowercase().any(|lower| lower == lowercase_expected)
}

fn completion_needs_lowercase(text: &str) -> bool {
    text.chars().any(|ch| {
        let mut lower = ch.to_lowercase();
        lower.next() != Some(ch) || lower.next().is_some()
    })
}

#[derive(Debug)]
struct PreparedCompletionCandidate<'a> {
    text: &'a str,
    lowercase: OnceCell<Cow<'a, str>>,
    lowercase_char_count: OnceCell<usize>,
}

impl<'a> PreparedCompletionCandidate<'a> {
    fn for_item(item: &'a LspCompletionItem) -> Self {
        Self::from_sampled_text(completion_filter_text_sample(item))
    }

    #[cfg(test)]
    fn new(text: &'a str) -> Self {
        let text = completion_ranking_text_sample(text, COMPLETION_RANKING_TEXT_MAX_CHARS);
        Self::from_sampled_text(text)
    }

    fn from_sampled_text(text: &'a str) -> Self {
        Self {
            text,
            lowercase: OnceCell::new(),
            lowercase_char_count: OnceCell::new(),
        }
    }

    fn text(&self) -> &'a str {
        self.text
    }

    fn lowercase(&self) -> &str {
        self.lowercase
            .get_or_init(|| completion_lowercase_cow(self.text))
            .as_ref()
    }

    fn lowercase_char_count(&self) -> usize {
        *self
            .lowercase_char_count
            .get_or_init(|| self.lowercase().chars().count())
    }

    fn normalized_lowercase(&self) -> Cow<'_, str> {
        let trimmed = self.text.trim();
        if trimmed.is_empty() {
            return Cow::Borrowed("");
        }
        if trimmed.len() == self.text.len() {
            return Cow::Borrowed(self.lowercase());
        }
        completion_lowercase_cow(trimmed)
    }
}

#[derive(Debug, Clone)]
struct CompletionPrefix<'a> {
    text: Cow<'a, str>,
    char_count: usize,
    chars: Vec<char>,
    first_char: Option<char>,
}

impl<'a> CompletionPrefix<'a> {
    fn new(text: &'a str) -> Self {
        let text = completion_lowercase_cow(completion_ranking_text_sample(
            text,
            COMPLETION_RANKING_PREFIX_MAX_CHARS,
        ));
        let chars = text.chars().collect::<Vec<_>>();
        let first_char = chars.first().copied();
        let char_count = chars.len();
        Self {
            text,
            char_count,
            chars,
            first_char,
        }
    }

    fn as_str(&self) -> &str {
        self.text.as_ref()
    }

    fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    fn chars(&self) -> impl Iterator<Item = char> + '_ {
        self.chars.iter().copied()
    }
}

fn completion_ranking_text_sample(text: &str, max_chars: usize) -> &str {
    if max_chars == 0 {
        return "";
    }
    completion_text_sample(
        text,
        max_chars,
        COMPLETION_RANKING_TEXT_SAMPLE_BYTES.max(max_chars.saturating_mul(4)),
    )
}

fn completion_order_text_sample(text: &str, max_chars: usize) -> &str {
    if max_chars == 0 {
        return "";
    }
    completion_text_sample_inner(
        text,
        max_chars,
        COMPLETION_RANKING_TEXT_SAMPLE_BYTES.max(max_chars.saturating_mul(4)),
        false,
    )
}

fn completion_locality_line_sample(text: &str) -> &str {
    completion_text_sample(
        text,
        COMPLETION_RANKING_TEXT_MAX_CHARS,
        COMPLETION_LOCALITY_LINE_SAMPLE_BYTES,
    )
}

fn completion_text_sample(text: &str, max_chars: usize, max_bytes: usize) -> &str {
    if max_chars == 0 || max_bytes == 0 {
        return "";
    }

    completion_text_sample_inner(text, max_chars, max_bytes, true)
}

fn completion_text_sample_inner(
    text: &str,
    max_chars: usize,
    max_bytes: usize,
    trim: bool,
) -> &str {
    let mut end = text.len().min(max_bytes);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let text = if trim {
        text[..end].trim()
    } else {
        &text[..end]
    };
    if text.is_empty() {
        return "";
    }
    if let Some((idx, _)) = text.char_indices().nth(max_chars) {
        if trim {
            text[..idx].trim()
        } else {
            &text[..idx]
        }
    } else {
        text
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CompletionRank {
    match_tier: usize,
    locality_rank: usize,
    preselect_rank: usize,
    auto_import_rank: usize,
    distance: usize,
}

#[derive(Debug, Clone)]
struct CompletionLocality {
    lines: Vec<CompletionLocalityLine>,
}

impl CompletionLocality {
    fn new(buffer: &TextBuffer, line: usize) -> Self {
        let line_count = buffer.len_lines();
        let mut lines =
            Vec::with_capacity((COMPLETION_LOCALITY_MAX_DISTANCE * 2 + 1).min(line_count));
        for distance in 0..=COMPLETION_LOCALITY_MAX_DISTANCE {
            if let Some(line_idx) = line.checked_sub(distance)
                && line_idx < line_count
            {
                Self::push_line(buffer, line_idx, distance, &mut lines);
            }
            if distance > 0
                && let Some(line_idx) = line.checked_add(distance)
                && line_idx < line_count
            {
                Self::push_line(buffer, line_idx, distance, &mut lines);
            }
        }
        Self { lines }
    }

    fn push_line(
        buffer: &TextBuffer,
        line_idx: usize,
        distance: usize,
        lines: &mut Vec<CompletionLocalityLine>,
    ) {
        if let Some(text) = buffer.line(line_idx) {
            lines.push(CompletionLocalityLine::new(distance, text));
        }
    }

    fn rank_for(&self, needle: &str) -> Option<usize> {
        self.lines
            .iter()
            .find(|line| line.contains(needle))
            .map(|line| line.distance)
    }
}

#[derive(Debug, Clone)]
struct CompletionLocalityLine {
    distance: usize,
    text: String,
    lowercase: Option<String>,
}

impl CompletionLocalityLine {
    fn new(distance: usize, text: String) -> Self {
        let text = completion_locality_line_sample(&text).to_owned();
        let lowercase = completion_needs_lowercase(&text).then(|| text.to_lowercase());
        Self {
            distance,
            text,
            lowercase,
        }
    }

    fn contains(&self, lowercase_needle: &str) -> bool {
        self.text.contains(lowercase_needle)
            || self
                .lowercase
                .as_deref()
                .is_some_and(|text| text.contains(lowercase_needle))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        COMPLETION_LOCALITY_LINE_SAMPLE_BYTES, COMPLETION_RANKING_PREFIX_MAX_CHARS,
        COMPLETION_RANKING_TEXT_MAX_CHARS, COMPLETION_RANKING_TEXT_SAMPLE_BYTES,
        CompletionLocality, CompletionPrefix, PreparedCompletionCandidate, completion_filter_text,
        completion_match_tier_and_distance, completion_prefix_at, completion_ranking_text_sample,
        filter_completion_items_by_settings, rank_completion_items,
        rank_completion_items_for_buffer, selected_completion_index,
    };
    use kuroya_core::{
        EditorSettings, EditorSnippetSuggestions, EditorSuggestSelection, LspCompletionItem,
        LspTextEdit, TextBuffer,
    };
    use std::{collections::VecDeque, path::PathBuf};

    fn item(label: &str) -> LspCompletionItem {
        LspCompletionItem {
            label: label.to_owned(),
            detail: None,
            documentation: None,
            kind: None,
            deprecated: false,
            is_snippet: false,
            sort_text: None,
            filter_text: None,
            preselect: false,
            commit_characters: Vec::new(),
            insert_text: label.to_owned(),
            snippet_selection: None,
            snippet_tabstops: Vec::new(),
            snippet_tabstop_groups: Vec::new(),
            text_edit: None,
            insert_text_edit: None,
            additional_text_edits: Vec::new(),
            resolve_payload: None,
        }
    }

    fn item_with_kind(label: &str, kind: Option<u8>) -> LspCompletionItem {
        LspCompletionItem {
            kind,
            ..item(label)
        }
    }

    fn auto_import_item(label: &str) -> LspCompletionItem {
        LspCompletionItem {
            additional_text_edits: vec![LspTextEdit {
                path: PathBuf::from("src/main.rs"),
                start_line: 1,
                start_column: 1,
                end_line: 1,
                end_column: 1,
                new_text: format!("use std::{label};\n"),
            }],
            ..item(label)
        }
    }

    fn replacement_import_item(label: &str) -> LspCompletionItem {
        LspCompletionItem {
            additional_text_edits: vec![LspTextEdit {
                path: PathBuf::from("src/main.rs"),
                start_line: 1,
                start_column: 1,
                end_line: 1,
                end_column: 8,
                new_text: format!("use std::{label};\n"),
            }],
            ..item(label)
        }
    }

    #[test]
    fn completion_prefix_uses_buffer_word_separators() {
        let mut buffer = TextBuffer::from_text(
            1,
            Some(PathBuf::from("src/main.rs")),
            "alpha.beta".to_owned(),
        );
        buffer.set_word_separators(".");

        assert_eq!(completion_prefix_at(&buffer, 0, 11), "beta");

        buffer.set_word_separators("");
        assert_eq!(completion_prefix_at(&buffer, 0, 11), "alpha.beta");
    }

    #[test]
    fn completion_prefix_handles_multibyte_columns_without_extra_scan_storage() {
        let buffer = TextBuffer::from_text(
            1,
            Some(PathBuf::from("src/main.rs")),
            "let cafe = r\u{00e9}sum\u{00e9}".to_owned(),
        );

        assert_eq!(completion_prefix_at(&buffer, 0, 18), "r\u{00e9}sum\u{00e9}");
    }

    #[test]
    fn completion_ranking_prefers_prefix_matches_then_filter_text() {
        let mut items = vec![item("panic!"), item("eprintln!"), item("println!")];
        items[1].filter_text = Some("println".to_owned());

        rank_completion_items(&mut items, "pri", &EditorSettings::default());

        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["eprintln!", "println!", "panic!"]
        );
    }

    #[test]
    fn completion_ranking_prefers_word_start_acronyms_over_plain_fuzzy_matches() {
        let mut items = vec![item("crab"), item("copyBuffer")];

        rank_completion_items(&mut items, "cb", &EditorSettings::default());

        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["copyBuffer", "crab"]
        );
    }

    #[test]
    fn completion_ranking_prefers_word_start_prefixes_over_plain_substrings() {
        let mut items = vec![
            item("decomposition"),
            item("WidgetComponent"),
            item("my_component"),
        ];

        rank_completion_items(&mut items, "comp", &EditorSettings::default());

        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["my_component", "WidgetComponent", "decomposition"]
        );
    }

    #[test]
    fn completion_ranking_preserves_unicode_case_folding() {
        let mut items = vec![item("resume"), item("r\u{00e9}sum\u{00e9}")];

        rank_completion_items(&mut items, "R\u{00c9}", &EditorSettings::default());

        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["r\u{00e9}sum\u{00e9}", "resume"]
        );
    }

    #[test]
    fn completion_ranking_reuses_unicode_prefix_character_count() {
        let prefix = CompletionPrefix::new("R\u{00c9}");
        let candidate = PreparedCompletionCandidate::new("r\u{00e9}sum\u{00e9}");

        assert_eq!(
            completion_match_tier_and_distance(&candidate, &prefix),
            (1, 4)
        );
    }

    #[test]
    fn completion_ranking_bounds_candidate_cache_text_without_mutating_item() {
        let mut completion = item("fallback");
        completion.filter_text = Some(format!(
            "{}tail",
            "\u{03bb}".repeat(COMPLETION_RANKING_TEXT_MAX_CHARS + 8)
        ));
        let raw_completion = completion.clone();

        let candidate = PreparedCompletionCandidate::for_item(&completion);

        assert_eq!(
            candidate.text().chars().count(),
            COMPLETION_RANKING_TEXT_MAX_CHARS
        );
        assert!(!candidate.text().contains("tail"));
        assert!(completion_filter_text(&completion).contains("tail"));
        assert_eq!(completion, raw_completion);
    }

    #[test]
    fn completion_ranking_bounds_prefix_cache_on_utf8_boundary() {
        let raw_prefix = "\u{03bb}".repeat(COMPLETION_RANKING_PREFIX_MAX_CHARS + 8);
        let prefix = CompletionPrefix::new(&raw_prefix);

        assert_eq!(prefix.char_count, COMPLETION_RANKING_PREFIX_MAX_CHARS);
        assert_eq!(prefix.as_str().chars().last(), Some('\u{03bb}'));
    }

    #[test]
    fn completion_ranking_samples_before_trimming_huge_text() {
        let raw_text = format!(
            "{}tail",
            " ".repeat(COMPLETION_RANKING_TEXT_SAMPLE_BYTES + 32)
        );

        assert_eq!(
            completion_ranking_text_sample(&raw_text, COMPLETION_RANKING_TEXT_MAX_CHARS),
            ""
        );
    }

    #[test]
    fn completion_ranking_ignores_filter_text_tail_past_sample_bound() {
        let mut completion = item("fallback");
        completion.filter_text = Some(format!(
            "{}println",
            " ".repeat(COMPLETION_RANKING_TEXT_SAMPLE_BYTES + 32)
        ));
        let raw_completion = completion.clone();
        let mut items = vec![completion, item("println")];

        rank_completion_items(&mut items, "pri", &EditorSettings::default());

        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["println", "fallback"]
        );
        assert_eq!(items[1], raw_completion);
        assert_eq!(completion_filter_text(&items[1]), "fallback");
    }

    #[test]
    fn completion_ranking_bounds_sort_text_comparison_without_mutating_items() {
        let mut first = item("first");
        first.sort_text = Some(format!(
            "{}z",
            "a".repeat(COMPLETION_RANKING_TEXT_MAX_CHARS + 8)
        ));
        let mut second = item("second");
        second.sort_text = Some(format!(
            "{}a",
            "a".repeat(COMPLETION_RANKING_TEXT_MAX_CHARS + 8)
        ));
        let raw_items = vec![first.clone(), second.clone()];
        let mut items = vec![first, second];

        rank_completion_items(&mut items, "", &EditorSettings::default());

        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "second"]
        );
        assert_eq!(items, raw_items);
    }

    #[test]
    fn completion_ranking_uses_preselect_and_sort_text_for_ties() {
        let mut items = vec![item("beta"), item("alpha"), item("gamma")];
        items[0].sort_text = Some("0003".to_owned());
        items[1].sort_text = Some("0002".to_owned());
        items[2].sort_text = Some("0001".to_owned());
        items[0].preselect = true;

        rank_completion_items(&mut items, "", &EditorSettings::default());

        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["beta", "gamma", "alpha"]
        );
    }

    #[test]
    fn completion_ranking_preserves_empty_prefix_server_order() {
        let mut items = vec![item("x"), item("longerCompletion"), item("medium")];

        rank_completion_items(&mut items, "", &EditorSettings::default());

        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["x", "longerCompletion", "medium"]
        );
    }

    #[test]
    fn completion_ranking_honors_sort_text_with_empty_prefix() {
        let mut items = vec![item("x"), item("longerCompletion"), item("medium")];
        items[0].sort_text = Some("0003".to_owned());
        items[1].sort_text = Some("0001".to_owned());
        items[2].sort_text = Some("0002".to_owned());

        rank_completion_items(&mut items, "", &EditorSettings::default());

        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["longerCompletion", "medium", "x"]
        );
    }

    #[test]
    fn completion_ranking_surfaces_auto_imports_with_equal_match_quality() {
        let mut items = vec![item("HashMap"), auto_import_item("HashSet")];

        rank_completion_items(&mut items, "Hash", &EditorSettings::default());

        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["HashSet", "HashMap"]
        );
    }

    #[test]
    fn completion_ranking_still_surfaces_empty_prefix_auto_imports() {
        let mut items = vec![item("HashMap"), auto_import_item("HashSet")];

        rank_completion_items(&mut items, "", &EditorSettings::default());

        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["HashSet", "HashMap"]
        );
    }

    #[test]
    fn completion_ranking_does_not_boost_replacement_import_edits() {
        let mut items = vec![item("HashMap"), replacement_import_item("HashSet")];

        rank_completion_items(&mut items, "Hash", &EditorSettings::default());

        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["HashMap", "HashSet"]
        );
    }

    #[test]
    fn completion_ranking_preserves_server_order_for_equal_ties() {
        let mut items = vec![item("zeta"), item("alpha"), item("beta")];

        rank_completion_items(&mut items, "no-match", &EditorSettings::default());

        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["zeta", "alpha", "beta"]
        );
    }

    #[test]
    fn completion_ranking_preserves_lsp_order_for_equal_sort_text_ties() {
        let mut first = item("same");
        first.sort_text = Some("0001".to_owned());
        first.insert_text = "first".to_owned();
        let mut second = item("same");
        second.sort_text = Some("0001".to_owned());
        second.insert_text = "second".to_owned();
        let mut third = item("same");
        third.sort_text = Some("0001".to_owned());
        third.insert_text = "third".to_owned();
        let mut items = vec![first, second, third];

        rank_completion_items(&mut items, "same", &EditorSettings::default());

        assert_eq!(
            items
                .iter()
                .map(|item| item.insert_text.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "second", "third"]
        );
    }

    #[test]
    fn completion_filter_honors_suggest_show_kind_settings() {
        let settings = EditorSettings {
            suggest_show_functions: false,
            suggest_show_classes: false,
            suggest_show_snippets: false,
            ..Default::default()
        };
        let mut items = vec![
            item_with_kind("println", Some(3)),
            item_with_kind("String", Some(7)),
            item_with_kind("for", Some(15)),
            item_with_kind("field", Some(5)),
            item_with_kind("unknown", None),
        ];

        filter_completion_items_by_settings(&mut items, &settings, "");

        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["field", "unknown"]
        );
    }

    #[test]
    fn completion_filter_honors_snippet_and_deprecated_settings() {
        let mut settings = EditorSettings {
            suggest_show_deprecated: false,
            snippet_suggestions: EditorSnippetSuggestions::None,
            ..Default::default()
        };
        let mut snippet = item("for-loop");
        snippet.is_snippet = true;
        let mut deprecated = item("old_api");
        deprecated.deprecated = true;
        let mut items = vec![snippet, deprecated, item("current_api")];

        filter_completion_items_by_settings(&mut items, &settings, "");
        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["current_api"]
        );

        settings.snippet_suggestions = EditorSnippetSuggestions::Inline;
        settings.suggest_show_deprecated = true;
        let mut snippet = item("for-loop");
        snippet.is_snippet = true;
        let mut deprecated = item("old_api");
        deprecated.deprecated = true;
        let mut items = vec![snippet, deprecated, item("current_api")];

        filter_completion_items_by_settings(&mut items, &settings, "");
        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["for-loop", "old_api", "current_api"]
        );
    }

    #[test]
    fn completion_ranking_places_snippets_by_setting() {
        let mut snippet = item("for-loop");
        snippet.is_snippet = true;
        let mut settings = EditorSettings {
            snippet_suggestions: EditorSnippetSuggestions::Top,
            ..Default::default()
        };
        let mut items = vec![item("format"), snippet.clone(), item("foreach")];

        rank_completion_items(&mut items, "fo", &settings);
        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["for-loop", "format", "foreach"]
        );

        settings.snippet_suggestions = EditorSnippetSuggestions::Bottom;
        let mut items = vec![item("format"), snippet, item("foreach")];

        rank_completion_items(&mut items, "fo", &settings);
        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["format", "foreach", "for-loop"]
        );
    }

    #[test]
    fn completion_filter_can_require_word_start_matches() {
        let settings = EditorSettings::default();
        let mut items = vec![
            item("Console"),
            item("WebContext"),
            item("my_component"),
            item("description"),
        ];

        filter_completion_items_by_settings(&mut items, &settings, "c");

        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Console", "WebContext", "my_component"]
        );
    }

    #[test]
    fn completion_filter_word_start_mode_accepts_acronym_matches() {
        let settings = EditorSettings::default();
        let mut items = vec![item("copyBuffer"), item("copy_buffer"), item("crab")];

        filter_completion_items_by_settings(&mut items, &settings, "cb");

        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["copyBuffer", "copy_buffer"]
        );
    }

    #[test]
    fn completion_filter_word_start_mode_keeps_unicode_prefix_matches() {
        let settings = EditorSettings::default();
        let mut items = vec![
            item("responseValue"),
            item("R\u{00e9}sum\u{00e9}Value"),
            item("RenderElement"),
        ];

        filter_completion_items_by_settings(&mut items, &settings, "R\u{00c9}");

        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["R\u{00e9}sum\u{00e9}Value"]
        );
    }

    #[test]
    fn completion_filter_can_allow_mid_word_and_graceful_matches() {
        let mut settings = EditorSettings {
            suggest_match_on_word_start_only: false,
            suggest_filter_graceful: false,
            ..Default::default()
        };
        let mut strict_items = vec![item("description"), item("println")];

        filter_completion_items_by_settings(&mut strict_items, &settings, "ptln");
        assert!(strict_items.is_empty());

        settings.suggest_filter_graceful = true;
        let mut graceful_items = vec![item("description"), item("println")];

        filter_completion_items_by_settings(&mut graceful_items, &settings, "ptln");
        assert_eq!(
            graceful_items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["println"]
        );

        let mut mid_word_items = vec![item("description")];
        filter_completion_items_by_settings(&mut mid_word_items, &settings, "script");
        assert_eq!(mid_word_items[0].label, "description");
    }

    #[test]
    fn completion_filter_falls_back_to_label_for_blank_filter_text() {
        let settings = EditorSettings::default();
        let mut completion = item("println");
        completion.filter_text = Some(" \t\n ".to_owned());
        let mut items = vec![item("panic"), completion];

        filter_completion_items_by_settings(&mut items, &settings, "pri");

        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["println"]
        );
    }

    #[test]
    fn completion_ranking_falls_back_to_label_for_blank_filter_text() {
        let mut completion = item("println");
        completion.filter_text = Some(" \t\n ".to_owned());
        let mut items = vec![item("panic"), completion];

        rank_completion_items(&mut items, "pri", &EditorSettings::default());

        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["println", "panic"]
        );
    }

    #[test]
    fn completion_selection_can_use_recent_label_or_prefix_history() {
        let items = vec![item("alpha"), item("beta"), item("gamma")];
        let recent_labels = VecDeque::from(["gamma".to_owned()]);
        let recent_prefix_labels = VecDeque::from([("be".to_owned(), "beta".to_owned())]);
        let mut settings = EditorSettings {
            suggest_selection: EditorSuggestSelection::RecentlyUsed,
            ..Default::default()
        };

        assert_eq!(
            selected_completion_index(
                &items,
                "be",
                &settings,
                &recent_labels,
                &recent_prefix_labels,
            ),
            2
        );

        settings.suggest_selection = EditorSuggestSelection::RecentlyUsedByPrefix;
        assert_eq!(
            selected_completion_index(
                &items,
                "be",
                &settings,
                &recent_labels,
                &recent_prefix_labels,
            ),
            1
        );
    }

    #[test]
    fn completion_ranking_can_prefer_nearby_symbols() {
        let buffer = TextBuffer::from_text(
            1,
            Some(PathBuf::from("src/main.rs")),
            "let aa_helper = 1;\n\nlet bb_helper = 2;\n".to_owned(),
        );
        let mut settings = EditorSettings {
            suggest_locality_bonus: true,
            ..Default::default()
        };
        let mut items = vec![item("bb_helper"), item("aa_helper")];

        rank_completion_items_for_buffer(&mut items, "helper", &settings, &buffer, 0);

        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["aa_helper", "bb_helper"]
        );

        settings.suggest_locality_bonus = false;
        let mut items = vec![item("bb_helper"), item("aa_helper")];
        rank_completion_items_for_buffer(&mut items, "helper", &settings, &buffer, 0);
        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["bb_helper", "aa_helper"]
        );
    }

    #[test]
    fn completion_ranking_locality_matches_cached_lines_case_insensitively() {
        let buffer = TextBuffer::from_text(
            1,
            Some(PathBuf::from("src/main.rs")),
            "let anchor = 1;\nlet NearbySymbol = 2;\n".to_owned(),
        );
        let settings = EditorSettings {
            suggest_locality_bonus: true,
            ..Default::default()
        };
        let mut items = vec![item("far_symbol"), item("nearbysymbol")];

        rank_completion_items_for_buffer(&mut items, "", &settings, &buffer, 0);

        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["nearbysymbol", "far_symbol"]
        );
    }

    #[test]
    fn completion_ranking_locality_cache_stores_bounded_line_samples() {
        let long_line = format!(
            "{}NearbySymbol\n",
            "\u{03bb}".repeat(COMPLETION_LOCALITY_LINE_SAMPLE_BYTES)
        );
        let buffer = TextBuffer::from_text(1, Some(PathBuf::from("src/main.rs")), long_line);

        let locality = CompletionLocality::new(&buffer, 0);
        let line = locality.lines.first().expect("sampled line");

        assert!(line.text.len() <= COMPLETION_LOCALITY_LINE_SAMPLE_BYTES);
        assert!(!line.text.contains("NearbySymbol"));
        assert!(
            line.lowercase
                .as_ref()
                .is_none_or(|text| text.len() <= COMPLETION_LOCALITY_LINE_SAMPLE_BYTES)
        );
    }
}
