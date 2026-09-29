use crate::{
    KuroyaApp, status_toasts::TOAST_CATEGORY_LSP, workspace_state::workspace_event_matches,
};
use kuroya_core::{LspWorkDoneProgress, LspWorkDoneProgressKind};
use std::{
    borrow::Cow,
    cmp::Ordering,
    collections::HashMap,
    fmt::Write as _,
    path::{Path, PathBuf},
};

const MAX_TRACKED_LSP_PROGRESS_TITLES: usize = 128;
pub(crate) const MAX_VISIBLE_LSP_PROGRESS_ITEMS: usize = 6;
const LSP_PROGRESS_TITLE_DISPLAY_MAX_CHARS: usize = 96;
const LSP_PROGRESS_MESSAGE_DISPLAY_MAX_CHARS: usize = 160;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct LspProgressKey {
    pub(crate) language: String,
    pub(crate) root: PathBuf,
    pub(crate) generation: u64,
    pub(crate) token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LspProgressSummary {
    pub(crate) active_count: usize,
    pub(crate) items: Vec<LspProgressSummaryItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LspProgressSummaryItem {
    pub(crate) language: String,
    pub(crate) root: PathBuf,
    pub(crate) generation: u64,
    pub(crate) token: String,
    pub(crate) title: String,
}

impl LspProgressKey {
    pub(crate) fn new(
        language: impl Into<String>,
        root: impl Into<PathBuf>,
        generation: u64,
        token: impl Into<String>,
    ) -> Self {
        Self {
            language: language.into(),
            root: root.into(),
            generation,
            token: token.into(),
        }
    }
}

impl LspProgressSummary {
    pub(crate) fn hidden_count(&self) -> usize {
        self.active_count.saturating_sub(self.items.len())
    }
}

impl KuroyaApp {
    pub(crate) fn handle_lsp_work_done_progress(
        &mut self,
        language: String,
        root: PathBuf,
        generation: u64,
        progress: LspWorkDoneProgress,
    ) {
        let root = self.lsp_progress_storage_root(&root);
        let key = LspProgressKey::new(language, root, generation, progress.token.clone());
        let title = update_lsp_progress_titles(&mut self.lsp_progress_titles, &key, &progress);
        let status = lsp_progress_status(&progress, title.as_deref());
        self.set_status_with_toast_in_category(TOAST_CATEGORY_LSP, status);
    }

    pub(crate) fn clear_lsp_progress_for_server(
        &mut self,
        language: &str,
        root: &std::path::Path,
        generation: u64,
    ) {
        self.lsp_progress_titles.retain(|key, _| {
            !(key.language == language
                && workspace_event_matches(&key.root, root)
                && key.generation == generation)
        });
    }

    fn lsp_progress_storage_root(&self, root: &Path) -> PathBuf {
        if workspace_event_matches(&self.workspace.root, root) {
            self.workspace.root.clone()
        } else {
            root.to_path_buf()
        }
    }
}

pub(crate) fn update_lsp_progress_titles<'a>(
    titles: &'a mut HashMap<LspProgressKey, String>,
    key: &LspProgressKey,
    progress: &'a LspWorkDoneProgress,
) -> Option<Cow<'a, str>> {
    match progress.kind {
        LspWorkDoneProgressKind::Begin => {
            if let Some(title) = progress
                .title
                .as_deref()
                .and_then(lsp_progress_display_title_text)
            {
                let title_text = title.as_ref();
                if let Some(existing) = titles.get_mut(key) {
                    replace_lsp_progress_title(existing, title_text);
                } else if titles.len() < MAX_TRACKED_LSP_PROGRESS_TITLES {
                    titles.insert(key.clone(), title_text.to_owned());
                }
                Some(title)
            } else {
                titles.get(key).map(|title| Cow::Borrowed(title.as_str()))
            }
        }
        LspWorkDoneProgressKind::Report => progress
            .title
            .as_deref()
            .and_then(lsp_progress_display_title_text)
            .or_else(|| titles.get(key).map(|title| Cow::Borrowed(title.as_str()))),
        LspWorkDoneProgressKind::End => {
            let tracked_title = titles.remove(key);
            progress
                .title
                .as_deref()
                .and_then(lsp_progress_display_title_text)
                .or_else(|| tracked_title.map(Cow::Owned))
        }
    }
}

fn replace_lsp_progress_title(existing: &mut String, title: &str) {
    existing.clear();
    existing.push_str(title);
}

pub(crate) fn lsp_progress_status(
    progress: &LspWorkDoneProgress,
    known_title: Option<&str>,
) -> String {
    let title = progress
        .title
        .as_deref()
        .and_then(lsp_progress_display_title_text)
        .or_else(|| known_title.and_then(lsp_progress_display_title_text))
        .unwrap_or(Cow::Borrowed("LSP task"));
    let message = progress
        .message
        .as_deref()
        .and_then(lsp_progress_display_message_text)
        .filter(|message| message.as_ref() != title.as_ref())
        .unwrap_or(Cow::Borrowed(""));
    let title = title.as_ref();
    let message = message.as_ref();

    let percent_capacity = progress.percentage.map_or(0, |_| 5);
    let message_capacity = if message.is_empty() {
        0
    } else {
        " - ".len() + message.len()
    };
    let suffix_capacity = match progress.kind {
        LspWorkDoneProgressKind::Begin => " started".len(),
        LspWorkDoneProgressKind::Report => 0,
        LspWorkDoneProgressKind::End => " complete".len(),
    };
    let mut status = String::with_capacity(
        "LSP: ".len() + title.len() + percent_capacity + suffix_capacity + message_capacity,
    );
    status.push_str("LSP: ");
    status.push_str(title);
    if let Some(percentage) = progress.percentage {
        let _ = write!(status, " {percentage}%");
    }
    match progress.kind {
        LspWorkDoneProgressKind::Begin => status.push_str(" started"),
        LspWorkDoneProgressKind::Report => {}
        LspWorkDoneProgressKind::End => status.push_str(" complete"),
    }
    if !message.is_empty() {
        status.push_str(" - ");
        status.push_str(message);
    }
    status
}

pub(crate) fn active_lsp_progress_summary(
    titles: &HashMap<LspProgressKey, String>,
    max_items: usize,
) -> LspProgressSummary {
    if titles.is_empty() {
        return LspProgressSummary {
            active_count: 0,
            items: Vec::new(),
        };
    }

    let mut active_count = 0usize;
    let mut visible_titles: Vec<(&LspProgressKey, Cow<'_, str>)> =
        Vec::with_capacity(titles.len().min(max_items));
    let mut worst_visible_idx = None;
    for (key, title) in titles {
        let Some(title) = lsp_progress_display_title_text(title) else {
            continue;
        };
        active_count += 1;
        if max_items == 0 {
            continue;
        }
        if visible_titles.len() < max_items {
            visible_titles.push((key, title));
            if visible_titles.len() == max_items {
                worst_visible_idx = worst_lsp_progress_summary_entry_idx(&visible_titles);
            }
            continue;
        }

        let worst_idx = worst_visible_idx.unwrap_or_else(|| {
            worst_lsp_progress_summary_entry_idx(&visible_titles)
                .expect("visible LSP progress entries")
        });
        let (worst_key, worst_title) = &visible_titles[worst_idx];
        if compare_lsp_progress_summary_entries(
            key,
            title.as_ref(),
            worst_key,
            worst_title.as_ref(),
        )
        .is_lt()
        {
            visible_titles[worst_idx] = (key, title);
            worst_visible_idx = worst_lsp_progress_summary_entry_idx(&visible_titles);
        }
    }

    if active_count == 0 || max_items == 0 {
        return LspProgressSummary {
            active_count,
            items: Vec::new(),
        };
    }

    if active_count > 1 {
        visible_titles.sort_by(|(left_key, left_title), (right_key, right_title)| {
            compare_lsp_progress_summary_entries(
                left_key,
                left_title.as_ref(),
                right_key,
                right_title.as_ref(),
            )
        });
    }
    let mut items = Vec::with_capacity(visible_titles.len());
    for (key, title) in visible_titles {
        items.push(LspProgressSummaryItem {
            language: key.language.clone(),
            root: key.root.clone(),
            generation: key.generation,
            token: key.token.clone(),
            title: title.into_owned(),
        });
    }

    LspProgressSummary {
        active_count,
        items,
    }
}

fn worst_lsp_progress_summary_entry_idx(
    entries: &[(&LspProgressKey, Cow<'_, str>)],
) -> Option<usize> {
    let mut worst_idx = 0usize;
    entries.get(worst_idx)?;
    for idx in 1..entries.len() {
        let (worst_key, worst_title) = &entries[worst_idx];
        let (candidate_key, candidate_title) = &entries[idx];
        if compare_lsp_progress_summary_entries(
            worst_key,
            worst_title.as_ref(),
            candidate_key,
            candidate_title.as_ref(),
        )
        .is_lt()
        {
            worst_idx = idx;
        }
    }
    Some(worst_idx)
}

fn compare_lsp_progress_summary_entries(
    left_key: &LspProgressKey,
    left_title: &str,
    right_key: &LspProgressKey,
    right_title: &str,
) -> Ordering {
    left_title
        .cmp(right_title)
        .then_with(|| left_key.language.cmp(&right_key.language))
        .then_with(|| left_key.root.cmp(&right_key.root))
        .then_with(|| left_key.generation.cmp(&right_key.generation))
        .then_with(|| left_key.token.cmp(&right_key.token))
}

fn lsp_progress_display_title_text(title: &str) -> Option<Cow<'_, str>> {
    lsp_progress_display_text(title, LSP_PROGRESS_TITLE_DISPLAY_MAX_CHARS)
}

fn lsp_progress_display_message_text(message: &str) -> Option<Cow<'_, str>> {
    lsp_progress_display_text(message, LSP_PROGRESS_MESSAGE_DISPLAY_MAX_CHARS)
}

fn lsp_progress_display_text(text: &str, max_chars: usize) -> Option<Cow<'_, str>> {
    if max_chars == 0 {
        return None;
    }

    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if lsp_progress_display_text_is_clean(text, max_chars) {
        return Some(Cow::Borrowed(text));
    }

    let mut output = String::with_capacity(text.len().min(max_chars));
    let mut chars = 0usize;
    let mut pending_space = false;

    for ch in text.chars() {
        if chars >= max_chars {
            break;
        }
        if ch.is_control() || ch.is_whitespace() {
            pending_space = !output.is_empty();
            continue;
        }
        if is_lsp_progress_format_control(ch) {
            continue;
        }
        if pending_space {
            output.push(' ');
            chars += 1;
            pending_space = false;
            if chars >= max_chars {
                break;
            }
        }
        output.push(ch);
        chars += 1;
    }

    (!output.is_empty()).then_some(Cow::Owned(output))
}

fn lsp_progress_display_text_is_clean(text: &str, max_chars: usize) -> bool {
    if let Some(is_clean) = lsp_progress_display_text_is_ascii_clean_prefix(text, max_chars) {
        return is_clean;
    }

    let mut chars = 0usize;
    let mut previous_space = false;
    for ch in text.chars() {
        if ch.is_control() || is_lsp_progress_format_control(ch) {
            return false;
        }
        if ch.is_whitespace() {
            if ch != ' ' || previous_space {
                return false;
            }
            previous_space = true;
        } else {
            previous_space = false;
        }
        chars += 1;
        if chars > max_chars {
            return false;
        }
    }
    chars > 0
}

fn lsp_progress_display_text_is_ascii_clean_prefix(text: &str, max_chars: usize) -> Option<bool> {
    let mut chars = 0usize;
    let mut previous_space = false;
    for &byte in text.as_bytes().iter().take(max_chars.saturating_add(1)) {
        if !byte.is_ascii() {
            return None;
        }
        if byte.is_ascii_control() {
            return Some(false);
        }
        if byte == b' ' {
            if previous_space {
                return Some(false);
            }
            previous_space = true;
        } else {
            previous_space = false;
        }
        chars += 1;
        if chars > max_chars {
            return Some(false);
        }
    }
    Some(chars > 0)
}

fn is_lsp_progress_format_control(ch: char) -> bool {
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
        LSP_PROGRESS_MESSAGE_DISPLAY_MAX_CHARS, LSP_PROGRESS_TITLE_DISPLAY_MAX_CHARS,
        LspProgressKey, LspProgressSummary, LspProgressSummaryItem,
        MAX_TRACKED_LSP_PROGRESS_TITLES, active_lsp_progress_summary, lsp_progress_status,
        update_lsp_progress_titles,
    };
    use kuroya_core::{LspWorkDoneProgress, LspWorkDoneProgressKind};
    use std::{borrow::Cow, collections::HashMap, path::PathBuf};

    fn progress(
        kind: LspWorkDoneProgressKind,
        title: Option<&str>,
        message: Option<&str>,
        percentage: Option<u8>,
    ) -> LspWorkDoneProgress {
        LspWorkDoneProgress {
            token: "token-1".to_owned(),
            kind,
            title: title.map(str::to_owned),
            message: message.map(str::to_owned),
            percentage,
        }
    }

    fn progress_with_token(
        token: String,
        kind: LspWorkDoneProgressKind,
        title: Option<String>,
    ) -> LspWorkDoneProgress {
        LspWorkDoneProgress {
            token,
            kind,
            title,
            message: None,
            percentage: None,
        }
    }

    fn key(token: impl Into<String>) -> LspProgressKey {
        LspProgressKey::new("rust", PathBuf::from("workspace"), 7, token)
    }

    fn key_for(
        language: impl Into<String>,
        root: impl Into<PathBuf>,
        generation: u64,
        token: impl Into<String>,
    ) -> LspProgressKey {
        LspProgressKey::new(language, root, generation, token)
    }

    #[test]
    fn lsp_progress_titles_carry_begin_title_to_reports_and_clear_on_end() {
        let mut titles = HashMap::new();
        let key = key("token-1");
        let begin = progress(
            LspWorkDoneProgressKind::Begin,
            Some("Indexing"),
            Some("Scanning"),
            Some(3),
        );
        assert_eq!(
            update_lsp_progress_titles(&mut titles, &key, &begin).as_deref(),
            Some("Indexing")
        );

        let report = progress(
            LspWorkDoneProgressKind::Report,
            None,
            Some("Crates"),
            Some(42),
        );
        assert_eq!(
            update_lsp_progress_titles(&mut titles, &key, &report).as_deref(),
            Some("Indexing")
        );
        assert_eq!(
            lsp_progress_status(&report, Some("Indexing")),
            "LSP: Indexing 42% - Crates"
        );

        let end = progress(LspWorkDoneProgressKind::End, None, None, None);
        assert_eq!(
            update_lsp_progress_titles(&mut titles, &key, &end).as_deref(),
            Some("Indexing")
        );
        assert!(titles.is_empty());
    }

    #[test]
    fn lsp_progress_titles_clear_on_titled_end() {
        let mut titles = HashMap::new();
        let key = key("token-1");
        let begin = progress(LspWorkDoneProgressKind::Begin, Some("Indexing"), None, None);
        assert_eq!(
            update_lsp_progress_titles(&mut titles, &key, &begin).as_deref(),
            Some("Indexing")
        );

        let end = progress(
            LspWorkDoneProgressKind::End,
            Some("Finished indexing"),
            None,
            None,
        );
        assert_eq!(
            update_lsp_progress_titles(&mut titles, &key, &end).as_deref(),
            Some("Finished indexing")
        );
        assert!(titles.is_empty());
    }

    #[test]
    fn lsp_progress_status_handles_begin_and_end_messages() {
        let begin = progress(
            LspWorkDoneProgressKind::Begin,
            Some("Build"),
            Some("Starting"),
            Some(0),
        );
        assert_eq!(
            lsp_progress_status(&begin, None),
            "LSP: Build 0% started - Starting"
        );

        let end = progress(
            LspWorkDoneProgressKind::End,
            Some("Build"),
            Some("Done"),
            None,
        );
        assert_eq!(
            lsp_progress_status(&end, None),
            "LSP: Build complete - Done"
        );
    }

    #[test]
    fn lsp_progress_display_text_is_single_line_bounded_and_strips_format_controls() {
        let mut titles = HashMap::new();
        let key = key("token-1");
        let begin = progress(
            LspWorkDoneProgressKind::Begin,
            Some(&format!(
                "Indexing\n\u{202e}{}",
                "t".repeat(LSP_PROGRESS_TITLE_DISPLAY_MAX_CHARS + 12)
            )),
            Some(&format!(
                "Scanning\t\u{2066}{}",
                "m".repeat(LSP_PROGRESS_MESSAGE_DISPLAY_MAX_CHARS + 12)
            )),
            Some(25),
        );

        let title = update_lsp_progress_titles(&mut titles, &key, &begin).expect("title");
        let status = lsp_progress_status(&begin, None);

        assert!(title.chars().count() <= LSP_PROGRESS_TITLE_DISPLAY_MAX_CHARS);
        assert!(
            status.chars().count()
                <= "LSP:  25% started - ".len()
                    + LSP_PROGRESS_TITLE_DISPLAY_MAX_CHARS
                    + LSP_PROGRESS_MESSAGE_DISPLAY_MAX_CHARS
        );
        assert!(!title.chars().any(char::is_control));
        assert!(!status.chars().any(char::is_control));
        assert!(!title.chars().any(super::is_lsp_progress_format_control));
        assert!(!status.chars().any(super::is_lsp_progress_format_control));
        assert!(status.starts_with("LSP: Indexing "));
        assert_eq!(
            lsp_progress_status(
                &progress(
                    LspWorkDoneProgressKind::Report,
                    Some("\n\t\u{202e}"),
                    None,
                    None
                ),
                None
            ),
            "LSP: LSP task"
        );
    }

    #[test]
    fn lsp_progress_display_text_borrows_clean_ascii_and_sanitizes_dirty_ascii() {
        assert!(matches!(
            super::lsp_progress_display_text("  Indexing crates  ", 32),
            Some(Cow::Borrowed("Indexing crates"))
        ));

        let dirty = super::lsp_progress_display_text("Indexing\t\r\ncrates", 32)
            .expect("dirty display text");
        assert_eq!(dirty, "Indexing crates");
        assert!(matches!(dirty, Cow::Owned(_)));

        let long = "a".repeat(LSP_PROGRESS_TITLE_DISPLAY_MAX_CHARS + 8);
        let truncated =
            super::lsp_progress_display_text(&long, LSP_PROGRESS_TITLE_DISPLAY_MAX_CHARS)
                .expect("truncated display text");
        assert_eq!(
            truncated.chars().count(),
            LSP_PROGRESS_TITLE_DISPLAY_MAX_CHARS
        );
        assert_eq!(
            truncated.as_ref(),
            "a".repeat(LSP_PROGRESS_TITLE_DISPLAY_MAX_CHARS)
        );
    }

    #[test]
    fn lsp_progress_title_tracking_reuses_existing_allocation_when_retitled() {
        let mut titles = HashMap::new();
        let key = key("token-1");
        let begin = progress(
            LspWorkDoneProgressKind::Begin,
            Some("Indexing workspace dependencies"),
            None,
            None,
        );
        assert_eq!(
            update_lsp_progress_titles(&mut titles, &key, &begin).as_deref(),
            Some("Indexing workspace dependencies")
        );

        let (initial_ptr, initial_capacity) = {
            let tracked = titles.get(&key).expect("tracked title");
            (tracked.as_ptr(), tracked.capacity())
        };
        let retitled = progress(
            LspWorkDoneProgressKind::Begin,
            Some("Indexing\tdeps"),
            None,
            None,
        );

        assert_eq!(
            update_lsp_progress_titles(&mut titles, &key, &retitled).as_deref(),
            Some("Indexing deps")
        );
        let tracked = titles.get(&key).expect("tracked title");
        assert_eq!(tracked.as_str(), "Indexing deps");
        assert_eq!(tracked.as_ptr(), initial_ptr);
        assert_eq!(tracked.capacity(), initial_capacity);
    }

    #[test]
    fn lsp_progress_title_tracking_is_bounded() {
        let mut titles = HashMap::new();
        for idx in 0..MAX_TRACKED_LSP_PROGRESS_TITLES {
            let key = key(format!("token-{idx}"));
            let progress = progress_with_token(
                format!("token-{idx}"),
                LspWorkDoneProgressKind::Begin,
                Some(format!("Task {idx}")),
            );
            assert_eq!(
                update_lsp_progress_titles(&mut titles, &key, &progress).as_deref(),
                Some(format!("Task {idx}").as_str())
            );
        }
        assert_eq!(titles.len(), MAX_TRACKED_LSP_PROGRESS_TITLES);

        let overflow = progress_with_token(
            "overflow-token".to_owned(),
            LspWorkDoneProgressKind::Begin,
            Some("Overflow".to_owned()),
        );
        let overflow_key = key("overflow-token");
        assert_eq!(
            update_lsp_progress_titles(&mut titles, &overflow_key, &overflow).as_deref(),
            Some("Overflow")
        );
        assert_eq!(titles.len(), MAX_TRACKED_LSP_PROGRESS_TITLES);
        assert!(!titles.contains_key(&overflow_key));

        let existing = progress_with_token(
            "token-0".to_owned(),
            LspWorkDoneProgressKind::Begin,
            Some("Retitled".to_owned()),
        );
        let existing_key = key("token-0");
        assert_eq!(
            update_lsp_progress_titles(&mut titles, &existing_key, &existing).as_deref(),
            Some("Retitled")
        );
        assert_eq!(titles.len(), MAX_TRACKED_LSP_PROGRESS_TITLES);
        assert_eq!(
            titles.get(&existing_key).map(String::as_str),
            Some("Retitled")
        );
    }

    #[test]
    fn lsp_progress_titles_are_scoped_by_server_identity() {
        let mut titles = HashMap::new();
        let rust_key = key_for("rust", "workspace-a", 1, "token");
        let ts_key = key_for("typescript", "workspace-b", 1, "token");
        let rust_begin = progress_with_token(
            "token".to_owned(),
            LspWorkDoneProgressKind::Begin,
            Some("Rust indexing".to_owned()),
        );
        let ts_begin = progress_with_token(
            "token".to_owned(),
            LspWorkDoneProgressKind::Begin,
            Some("TS indexing".to_owned()),
        );

        assert_eq!(
            update_lsp_progress_titles(&mut titles, &rust_key, &rust_begin).as_deref(),
            Some("Rust indexing")
        );
        assert_eq!(
            update_lsp_progress_titles(&mut titles, &ts_key, &ts_begin).as_deref(),
            Some("TS indexing")
        );
        assert_eq!(titles.len(), 2);

        let rust_end = progress_with_token("token".to_owned(), LspWorkDoneProgressKind::End, None);
        assert_eq!(
            update_lsp_progress_titles(&mut titles, &rust_key, &rust_end).as_deref(),
            Some("Rust indexing")
        );
        assert!(!titles.contains_key(&rust_key));
        assert_eq!(titles.get(&ts_key).map(String::as_str), Some("TS indexing"));
    }

    #[test]
    fn lsp_progress_title_updates_borrow_clean_progress_titles() {
        let mut titles = HashMap::new();
        let key = key("token-1");
        let report = progress(
            LspWorkDoneProgressKind::Report,
            Some("Checking workspace"),
            None,
            None,
        );
        let title = update_lsp_progress_titles(&mut titles, &key, &report).expect("title");
        assert!(matches!(title, Cow::Borrowed("Checking workspace")));

        let dirty_report = progress(
            LspWorkDoneProgressKind::Report,
            Some("Checking\tworkspace"),
            None,
            None,
        );
        let title =
            update_lsp_progress_titles(&mut titles, &key, &dirty_report).expect("dirty title");
        assert_eq!(title, "Checking workspace");
        assert!(matches!(title, Cow::Owned(_)));
    }

    #[test]
    fn active_lsp_progress_summary_is_sorted_and_bounded() {
        let titles = HashMap::from([
            (
                key_for("rust", "workspace", 7, "token-3"),
                "Formatting".to_owned(),
            ),
            (
                key_for("rust", "workspace", 7, "token-1"),
                "Indexing".to_owned(),
            ),
            (
                key_for("typescript", "workspace", 7, "token-2"),
                "Indexing".to_owned(),
            ),
        ]);

        assert_eq!(
            active_lsp_progress_summary(&titles, 2),
            LspProgressSummary {
                active_count: 3,
                items: vec![
                    LspProgressSummaryItem {
                        language: "rust".to_owned(),
                        root: PathBuf::from("workspace"),
                        generation: 7,
                        token: "token-3".to_owned(),
                        title: "Formatting".to_owned(),
                    },
                    LspProgressSummaryItem {
                        language: "rust".to_owned(),
                        root: PathBuf::from("workspace"),
                        generation: 7,
                        token: "token-1".to_owned(),
                        title: "Indexing".to_owned(),
                    },
                ],
            }
        );
    }

    #[test]
    fn active_lsp_progress_summary_keeps_best_entries_after_visible_capacity() {
        let titles = HashMap::from([
            (
                key_for("rust", "workspace", 7, "token-z"),
                "Zulu".to_owned(),
            ),
            (
                key_for("rust", "workspace", 7, "token-b"),
                "Bravo".to_owned(),
            ),
            (
                key_for("rust", "workspace", 7, "token-e"),
                "Echo".to_owned(),
            ),
            (
                key_for("rust", "workspace", 7, "token-a"),
                "Alpha".to_owned(),
            ),
        ]);

        let summary = active_lsp_progress_summary(&titles, 2);

        assert_eq!(summary.active_count, 4);
        assert_eq!(
            summary
                .items
                .iter()
                .map(|item| item.title.as_str())
                .collect::<Vec<_>>(),
            vec!["Alpha", "Bravo"]
        );
    }

    #[test]
    fn active_lsp_progress_summary_reports_hidden_count() {
        let titles = HashMap::from([
            (key("token-1"), "Indexing".to_owned()),
            (key("token-2"), "Checking".to_owned()),
            (key("token-3"), "Formatting".to_owned()),
        ]);
        let summary = active_lsp_progress_summary(&titles, 1);

        assert_eq!(summary.active_count, 3);
        assert_eq!(summary.items.len(), 1);
        assert_eq!(summary.hidden_count(), 2);
    }
}
