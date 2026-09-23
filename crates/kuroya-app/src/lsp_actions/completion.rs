use crate::{
    KuroyaApp,
    lsp_completion_resolve::CompletionResolveIntent,
    lsp_edits::completion_buffer_edit_plan,
    lsp_runtime::lsp_command_queue_failed_status,
    path_display::{display_path_label_cow, sanitized_display_label_cow},
    snippet_session::SnippetSession,
    workspace_state::active_buffer_lsp_position_matches,
};
use kuroya_core::{LspCompletionItem, Selection};
use std::{borrow::Cow, fmt::Write as _, ops::Range, path::Path};

const MAX_COMPLETION_RECENT_SELECTIONS: usize = 64;
const MAX_COMPLETION_RECENT_PREFIX_SELECTIONS: usize = 128;
const COMPLETION_STATUS_LABEL_MAX_CHARS: usize = 80;
const COMPLETION_STATUS_COMMIT_TEXT_MAX_CHARS: usize = 80;

impl KuroyaApp {
    pub(crate) fn apply_completion_item_with_commit(
        &mut self,
        item: LspCompletionItem,
        commit_text: Option<String>,
    ) {
        if item.needs_resolve()
            && self.resolve_completion_item_before_apply(item.clone(), commit_text.clone())
        {
            return;
        }
        self.apply_resolved_completion_item_with_commit(item, commit_text);
    }

    pub(crate) fn apply_resolved_completion_item_with_commit(
        &mut self,
        item: LspCompletionItem,
        commit_text: Option<String>,
    ) {
        if self.active.is_none() {
            self.clear_completion_popup_state();
            self.status = "No active completion target".to_owned();
            return;
        }
        if !self.completion_target_matches_active_position() {
            self.clear_completion_popup_state();
            self.status =
                completion_could_not_apply_to_target_status(&item.label, "target changed");
            return;
        }

        self.apply_resolved_completion_item_to_active_buffer(item, commit_text);
    }

    pub(crate) fn apply_resolved_completion_item_to_active_buffer(
        &mut self,
        item: LspCompletionItem,
        commit_text: Option<String>,
    ) {
        let Some(id) = self.active else {
            self.clear_completion_popup_state();
            self.status = "No active completion target".to_owned();
            return;
        };
        if self.block_protected_preview_edit(id) {
            self.clear_completion_popup_state();
            return;
        }

        let Some(buffer) = self.buffer(id) else {
            self.clear_completion_popup_state();
            self.status = "Completion target closed".to_owned();
            return;
        };
        let Some(buffer_edit_plan) =
            completion_buffer_edit_plan(buffer, &item, self.settings.suggest_insert_mode)
        else {
            self.clear_completion_popup_state();
            self.status = completion_could_not_apply_status(&item.label);
            return;
        };
        let auto_pair_settings = kuroya_core::buffer::AutoPairSettings {
            brackets: self.settings.auto_closing_brackets,
            quotes: self.settings.auto_closing_quotes,
            surround: self.settings.auto_surround,
            overtype: !matches!(
                self.settings.auto_closing_overtype,
                kuroya_core::EditorAutoClosingEditStrategy::Never
            ),
        };
        let crate::lsp_edits::CompletionBufferEditPlan {
            edits,
            primary_edit,
            snippet_selection,
            snippet_tabstops,
            snippet_tabstop_groups,
        } = buffer_edit_plan;
        let mut snippet_tabstop_groups_after = None;
        let changed = self.buffer_mut(id).is_some_and(|buffer| {
            let mut changed = if let (Some(primary_edit), true) =
                (primary_edit.as_ref(), !snippet_tabstop_groups.is_empty())
            {
                let flattened_tabstops;
                let inserted_tabstops = if snippet_tabstops.is_empty() {
                    flattened_tabstops = flatten_snippet_tabstop_groups(&snippet_tabstop_groups);
                    &flattened_tabstops
                } else {
                    &snippet_tabstops
                };
                if let Some(tabstops) = buffer.apply_edits_with_inserted_selections(
                    edits,
                    primary_edit,
                    inserted_tabstops,
                ) {
                    let groups = group_inserted_tabstops(&snippet_tabstop_groups, tabstops);
                    if let Some(group) = groups.first() {
                        buffer.set_selections(group.iter().map(|range| Selection {
                            anchor: range.start,
                            cursor: range.end,
                        }));
                    }
                    snippet_tabstop_groups_after = Some(groups);
                    true
                } else {
                    false
                }
            } else if let (Some(primary_edit), true) =
                (primary_edit.as_ref(), !snippet_tabstops.is_empty())
            {
                if let Some(tabstops) = buffer.apply_edits_with_inserted_selections(
                    edits,
                    primary_edit,
                    &snippet_tabstops,
                ) {
                    snippet_tabstop_groups_after =
                        Some(tabstops.into_iter().map(|tabstop| vec![tabstop]).collect());
                    true
                } else {
                    false
                }
            } else if let (Some(primary_edit), Some(snippet_selection)) =
                (primary_edit.as_ref(), snippet_selection)
            {
                buffer.apply_edits_with_inserted_selection(edits, primary_edit, snippet_selection)
            } else if let (Some(primary_edit), Some(_)) =
                (primary_edit.as_ref(), commit_text.as_ref())
            {
                let inserted_len = primary_edit.inserted.chars().count();
                buffer.apply_edits_with_inserted_selection(
                    edits,
                    primary_edit,
                    inserted_len..inserted_len,
                )
            } else {
                buffer.apply_edits(edits)
            };
            if changed && let Some(text) = commit_text.as_deref() {
                changed |= buffer.insert_text_with_auto_pair_settings(text, auto_pair_settings);
            }
            changed
        });

        let label = item.label;
        if changed {
            let prefix = completion_recent_prefix(&self.completion_prefix);
            self.clear_completion_popup_state();
            self.record_completion_selection(label.clone(), prefix);
            self.mark_buffer_changed(id);
            self.snippet_session = if commit_text.is_none() {
                snippet_tabstop_groups_after
                    .and_then(|groups| SnippetSession::new_grouped(id, groups))
            } else {
                None
            };
            self.status = completion_inserted_status(&label, commit_text.as_deref());
        } else {
            self.clear_completion_popup_state();
            self.status = completion_made_no_change_status(&label);
        }
    }

    pub(crate) fn clear_completion_popup_state(&mut self) {
        self.completion_open = false;
        self.completion_items.clear();
        self.completion_buffer_id = None;
        self.completion_path = None;
        self.completion_version = None;
        self.completion_line = 0;
        self.completion_column = 0;
        self.completion_prefix.clear();
        self.completion_selected = 0;
        self.completion_preview_resolve_in_flight.clear();
        self.completion_preview_resolve_recent_attempts.clear();
    }

    fn completion_target_matches_active_position(&self) -> bool {
        let Some(active_id) = self.active else {
            return false;
        };
        let Some(origin_id) = self.completion_buffer_id else {
            return false;
        };
        if active_id != origin_id {
            return false;
        }
        let Some(origin_path) = self.completion_path.as_ref() else {
            return false;
        };
        let Some(origin_version) = self.completion_version else {
            return false;
        };
        let Some(line) = self.completion_line.checked_sub(1) else {
            return false;
        };
        active_buffer_lsp_position_matches(
            self.active_buffer(),
            origin_path,
            origin_version,
            line,
            self.completion_column,
        )
    }

    fn resolve_completion_item_before_apply(
        &mut self,
        item: LspCompletionItem,
        commit_text: Option<String>,
    ) -> bool {
        let Some(origin_id) = self.completion_buffer_id else {
            self.status =
                completion_could_not_resolve_status(&item.label, "missing completion target");
            return false;
        };
        let Some(origin_path) = self.completion_path.as_ref() else {
            self.status =
                completion_could_not_resolve_status(&item.label, "missing completion target");
            return false;
        };
        let Some(origin_version) = self.completion_version else {
            self.status =
                completion_could_not_resolve_status(&item.label, "missing completion target");
            return false;
        };
        let Some((id, path, version, line, character)) = self.active_lsp_position() else {
            self.status = completion_could_not_resolve_status(&item.label, "no active LSP target");
            return false;
        };
        if id != origin_id
            || path.as_path() != origin_path
            || version != origin_version
            || self.completion_line != line + 1
            || self.completion_column != character + 1
        {
            self.clear_completion_popup_state();
            self.status = completion_could_not_resolve_status(&item.label, "target changed");
            return true;
        }
        let trace_label = completion_lsp_trace_label(origin_path, line + 1, character + 1);
        let origin_path = origin_path.clone();
        if self.block_protected_preview_edit(origin_id) {
            self.clear_completion_popup_state();
            return true;
        }
        let Some(client) = self.ensure_lsp_for_buffer(origin_id) else {
            self.apply_resolved_completion_item_with_commit(item, commit_text);
            return true;
        };
        let resolving_status = completion_resolving_status(&item.label);
        if !client.resolve_completion_item(
            origin_id,
            origin_path,
            origin_version,
            line,
            character,
            item,
            CompletionResolveIntent::Apply { commit_text },
        ) {
            self.status = lsp_command_queue_failed_status("completionItem/resolve");
            return false;
        }

        self.record_lsp_client_trace("completionItem/resolve", trace_label);
        self.clear_completion_popup_state();
        self.status = resolving_status;
        true
    }

    fn record_completion_selection(&mut self, label: String, prefix: Option<String>) {
        self.completion_recent_labels
            .retain(|existing| existing != &label);
        let Some(prefix) = prefix else {
            self.completion_recent_labels.push_front(label);
            self.completion_recent_labels
                .truncate(MAX_COMPLETION_RECENT_SELECTIONS);
            return;
        };

        self.completion_recent_labels.push_front(label.clone());
        self.completion_recent_labels
            .truncate(MAX_COMPLETION_RECENT_SELECTIONS);

        self.completion_recent_prefix_labels
            .retain(|(existing, _)| existing != &prefix);
        self.completion_recent_prefix_labels
            .push_front((prefix, label));
        self.completion_recent_prefix_labels
            .truncate(MAX_COMPLETION_RECENT_PREFIX_SELECTIONS);
    }
}

fn completion_could_not_apply_status(label: &str) -> String {
    let label = completion_status_label(label);
    format!("Could not apply completion `{}`", label.as_ref())
}

fn completion_could_not_apply_to_target_status(label: &str, reason: &str) -> String {
    let label = completion_status_label(label);
    format!("Could not apply completion `{}`: {reason}", label.as_ref())
}

fn completion_inserted_status(label: &str, commit_text: Option<&str>) -> String {
    let label = completion_status_label(label);
    if let Some(text) = commit_text {
        let commit_text = completion_status_commit_text(text);
        format!(
            "Inserted completion `{}` and `{}`",
            label.as_ref(),
            commit_text.as_ref()
        )
    } else {
        format!("Inserted completion `{}`", label.as_ref())
    }
}

fn completion_made_no_change_status(label: &str) -> String {
    let label = completion_status_label(label);
    format!("Completion `{}` made no change", label.as_ref())
}

fn completion_could_not_resolve_status(label: &str, reason: &str) -> String {
    let label = completion_status_label(label);
    format!(
        "Could not resolve completion `{}`: {reason}",
        label.as_ref()
    )
}

fn completion_resolving_status(label: &str) -> String {
    let label = completion_status_label(label);
    format!("Resolving completion `{}`", label.as_ref())
}

fn completion_status_label(label: &str) -> Cow<'_, str> {
    sanitized_display_label_cow(label, COMPLETION_STATUS_LABEL_MAX_CHARS, "completion")
}

fn completion_status_commit_text(text: &str) -> Cow<'_, str> {
    sanitized_display_label_cow(text, COMPLETION_STATUS_COMMIT_TEXT_MAX_CHARS, "commit text")
}

fn completion_lsp_trace_label(path: &Path, line: usize, column: usize) -> String {
    let path = display_path_label_cow(path);
    let mut label = String::with_capacity(path.len() + 24);
    label.push_str(path.as_ref());
    let _ = write!(label, ":{line}:{column}");
    label
}

fn flatten_snippet_tabstop_groups(groups: &[Vec<Range<usize>>]) -> Vec<Range<usize>> {
    let len = groups.iter().map(Vec::len).sum();
    let mut tabstops = Vec::with_capacity(len);
    for group in groups {
        tabstops.extend(group.iter().cloned());
    }
    tabstops
}

fn group_inserted_tabstops(
    template: &[Vec<Range<usize>>],
    inserted_tabstops: Vec<Range<usize>>,
) -> Vec<Vec<Range<usize>>> {
    let mut inserted = inserted_tabstops.into_iter();
    let mut groups = Vec::with_capacity(template.len());
    for group in template {
        let mut mapped = Vec::with_capacity(group.len());
        for _ in 0..group.len() {
            let Some(tabstop) = inserted.next() else {
                break;
            };
            mapped.push(tabstop);
        }
        if !mapped.is_empty() {
            groups.push(mapped);
        }
    }
    groups
}

fn completion_recent_prefix(prefix: &str) -> Option<String> {
    let prefix = prefix.trim();
    (!prefix.is_empty()).then(|| prefix.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::{
        COMPLETION_STATUS_COMMIT_TEXT_MAX_CHARS, COMPLETION_STATUS_LABEL_MAX_CHARS,
        completion_could_not_apply_status, completion_could_not_apply_to_target_status,
        completion_inserted_status, completion_lsp_trace_label, completion_made_no_change_status,
        completion_resolving_status, completion_status_commit_text, completion_status_label,
        flatten_snippet_tabstop_groups, group_inserted_tabstops,
    };
    use crate::{
        KuroyaApp, app_startup_context::AppStartupContext,
        path_display::DISPLAY_PATH_LABEL_MAX_CHARS, terminal::TerminalPane,
    };
    use kuroya_core::{EditorSettings, LspCompletionItem, TextBuffer, TextEdit, Workspace};
    use std::{borrow::Cow, path::PathBuf, time::Instant};
    use tokio::runtime::Runtime;

    #[test]
    fn grouped_snippet_tabstops_round_trip_after_insert_mapping() {
        let template = vec![vec![0..5, 8..13], vec![15..19], vec![25..25]];
        assert_eq!(
            flatten_snippet_tabstop_groups(&template),
            vec![0..5, 8..13, 15..19, 25..25]
        );

        let grouped = group_inserted_tabstops(&template, vec![3..8, 11..16, 18..22, 28..28]);

        assert_eq!(
            grouped,
            vec![vec![3..8, 11..16], vec![18..22], vec![28..28]]
        );
    }

    #[test]
    fn completion_statuses_sanitize_and_bound_lsp_labels() {
        let raw_label = format!(
            "  first\nsecond\u{202e}{}tail  ",
            "very-long-label-".repeat(COMPLETION_STATUS_LABEL_MAX_CHARS)
        );
        let statuses = [
            completion_could_not_apply_status(&raw_label),
            completion_inserted_status(&raw_label, None),
            completion_made_no_change_status(&raw_label),
            completion_resolving_status(&raw_label),
        ];

        for status in statuses {
            assert_safe_status_text(&status);
            assert!(!status.contains("first\nsecond"));
            assert!(!status.contains('\u{202e}'));
            assert!(status.contains("first second"));
            assert!(status.contains("..."));
            assert!(
                status.chars().count()
                    <= "Could not apply completion ``".chars().count()
                        + COMPLETION_STATUS_LABEL_MAX_CHARS
            );
        }

        let target_status =
            completion_could_not_apply_to_target_status(&raw_label, "target changed");
        assert_safe_status_text(&target_status);
        assert!(target_status.contains("first second"));
        assert!(target_status.contains("target changed"));
        assert!(target_status.contains("..."));
        assert!(
            target_status.chars().count()
                <= "Could not apply completion ``: target changed"
                    .chars()
                    .count()
                    + COMPLETION_STATUS_LABEL_MAX_CHARS
        );
    }

    #[test]
    fn completion_status_label_cow_borrows_clean_ascii_and_unicode() {
        match completion_status_label("HashMap") {
            Cow::Borrowed(label) => assert_eq!(label, "HashMap"),
            Cow::Owned(label) => panic!("clean ASCII label allocated: {label}"),
        }

        let unicode = "r\u{00e9}sum\u{00e9}\u{88dc}\u{5b8c}";
        match completion_status_label(unicode) {
            Cow::Borrowed(label) => assert_eq!(label, unicode),
            Cow::Owned(label) => panic!("clean Unicode label allocated: {label}"),
        }

        match completion_status_commit_text(";") {
            Cow::Borrowed(text) => assert_eq!(text, ";"),
            Cow::Owned(text) => panic!("clean ASCII commit text allocated: {text}"),
        }

        let unicode_commit = "\u{2192}\u{5b8c}\u{4e86}";
        match completion_status_commit_text(unicode_commit) {
            Cow::Borrowed(text) => assert_eq!(text, unicode_commit),
            Cow::Owned(text) => panic!("clean Unicode commit text allocated: {text}"),
        }
    }

    #[test]
    fn completion_status_label_cow_owns_dirty_truncated_and_fallback_text() {
        let dirty_label = " first\nsecond\u{202e} ";
        match completion_status_label(dirty_label) {
            Cow::Owned(label) => {
                assert_eq!(label, "first second");
                assert_safe_status_text(&label);
            }
            Cow::Borrowed(label) => panic!("dirty label was borrowed: {label}"),
        }

        let long_label = "very-long-label-".repeat(COMPLETION_STATUS_LABEL_MAX_CHARS);
        match completion_status_label(&long_label) {
            Cow::Owned(label) => {
                assert!(label.contains("..."), "{label}");
                assert!(label.chars().count() <= COMPLETION_STATUS_LABEL_MAX_CHARS);
            }
            Cow::Borrowed(label) => panic!("truncated label was borrowed: {label}"),
        }

        match completion_status_label(" \n\t") {
            Cow::Owned(label) => assert_eq!(label, "completion"),
            Cow::Borrowed(label) => panic!("fallback label was borrowed: {label}"),
        }

        match completion_status_commit_text(" \u{202e}\n") {
            Cow::Owned(text) => assert_eq!(text, "commit text"),
            Cow::Borrowed(text) => panic!("fallback commit text was borrowed: {text}"),
        }
    }

    #[test]
    fn completion_inserted_status_sanitizes_and_bounds_commit_text() {
        let raw_commit = format!(
            "\ncommit\u{202e}{}tail\u{2029}",
            "very-long-commit-".repeat(COMPLETION_STATUS_COMMIT_TEXT_MAX_CHARS)
        );
        let status = completion_inserted_status("label", Some(&raw_commit));

        assert_safe_status_text(&status);
        assert!(!status.contains('\n'));
        assert!(!status.contains('\u{202e}'));
        assert!(!status.contains('\u{2029}'));
        assert!(status.contains("commit"));
        assert!(status.contains("..."));
        assert!(
            status.chars().count()
                <= "Inserted completion `` and ``".chars().count()
                    + COMPLETION_STATUS_LABEL_MAX_CHARS
                    + COMPLETION_STATUS_COMMIT_TEXT_MAX_CHARS
        );
    }

    #[test]
    fn completion_lsp_trace_label_sanitizes_and_bounds_path() {
        let path = PathBuf::from("workspace/src").join(format!(
            "bad\n{}\u{202e}completion.rs",
            "very-long-path-".repeat(DISPLAY_PATH_LABEL_MAX_CHARS)
        ));

        let trace = completion_lsp_trace_label(&path, 4, 2);

        assert_safe_status_text(&trace);
        assert!(trace.contains("..."), "{trace}");
        assert!(trace.chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS + ":4:2".chars().count());
    }

    #[test]
    fn completion_apply_sanitizes_status_but_keeps_raw_commit_and_recent_label() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "pri".to_owned());
        buffer.set_single_cursor(buffer.line_column_to_char(0, 3));
        let version = buffer.version();
        app.active = Some(7);
        app.buffers.push(buffer);
        set_completion_origin(&mut app, 7, path.clone(), version, 1, 4, "pri");

        let raw_label = format!(
            "print\n{}\u{202e}line",
            "unsafe-label-".repeat(COMPLETION_STATUS_LABEL_MAX_CHARS)
        );
        let raw_commit = format!(
            "\n{}\u{202e};",
            "unsafe-commit-".repeat(COMPLETION_STATUS_COMMIT_TEXT_MAX_CHARS)
        );
        app.apply_resolved_completion_item_with_commit(
            LspCompletionItem {
                label: raw_label.clone(),
                insert_text: "println".to_owned(),
                ..completion_item("println")
            },
            Some(raw_commit.clone()),
        );

        assert_safe_status_text(&app.status);
        assert!(!app.status.contains('\n'));
        assert!(!app.status.contains('\u{202e}'));
        assert!(app.status.contains("Inserted completion `print"));
        assert_eq!(
            app.buffer(7).expect("buffer should remain open").text(),
            format!("println{raw_commit}")
        );
        assert_eq!(app.completion_recent_labels.front(), Some(&raw_label));
        assert_eq!(
            app.completion_recent_prefix_labels.front(),
            Some(&("pri".to_owned(), raw_label))
        );
    }

    #[test]
    fn completion_apply_ignores_popup_after_active_buffer_switch() {
        let root = PathBuf::from("workspace");
        let completion_path = root.join("src/main.rs");
        let other_path = root.join("src/lib.rs");
        let mut app = app_for_test(root);
        let mut completion_buffer =
            TextBuffer::from_text(7, Some(completion_path.clone()), "Hash".to_owned());
        completion_buffer.set_single_cursor(completion_buffer.line_column_to_char(0, 4));
        let completion_version = completion_buffer.version();
        let mut other_buffer = TextBuffer::from_text(8, Some(other_path.clone()), "Vec".to_owned());
        other_buffer.set_single_cursor(other_buffer.line_column_to_char(0, 3));
        app.buffers.push(completion_buffer);
        app.buffers.push(other_buffer);
        app.active = Some(8);
        set_completion_origin(
            &mut app,
            7,
            completion_path.clone(),
            completion_version,
            1,
            5,
            "Hash",
        );

        app.apply_completion_item_with_commit(completion_item("HashMap"), None);

        assert_eq!(app.buffer(7).expect("completion buffer").text(), "Hash");
        assert_eq!(app.buffer(8).expect("active buffer").text(), "Vec");
        assert!(!app.completion_open);
        assert!(app.completion_items.is_empty());
        assert_eq!(app.completion_buffer_id, None);
        assert_eq!(app.completion_path, None);
        assert_eq!(app.completion_version, None);
        assert_eq!(
            app.status,
            "Could not apply completion `HashMap`: target changed"
        );
    }

    #[test]
    fn completion_apply_ignores_popup_after_buffer_version_changes() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "Hash".to_owned());
        buffer.set_single_cursor(buffer.line_column_to_char(0, 4));
        let stale_version = buffer.version();
        app.active = Some(7);
        app.buffers.push(buffer);
        set_completion_origin(&mut app, 7, path, stale_version, 1, 5, "Hash");
        app.buffer_mut(7)
            .expect("completion buffer")
            .apply_edit(TextEdit {
                range: 0..0,
                inserted: "type ".to_owned(),
            });

        app.apply_completion_item_with_commit(completion_item("HashMap"), None);

        assert_eq!(
            app.buffer(7).expect("completion buffer").text(),
            "type Hash"
        );
        assert_eq!(
            app.status,
            "Could not apply completion `HashMap`: target changed"
        );
    }

    #[test]
    fn completion_apply_ignores_popup_after_cursor_moves() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "Hash".to_owned());
        buffer.set_single_cursor(buffer.line_column_to_char(0, 4));
        let version = buffer.version();
        app.active = Some(7);
        app.buffers.push(buffer);
        set_completion_origin(&mut app, 7, path, version, 1, 5, "Hash");
        app.buffer_mut(7)
            .expect("completion buffer")
            .set_single_cursor(0);

        app.apply_completion_item_with_commit(completion_item("HashMap"), None);

        assert_eq!(app.buffer(7).expect("completion buffer").text(), "Hash");
        assert_eq!(
            app.status,
            "Could not apply completion `HashMap`: target changed"
        );
    }

    fn completion_item(label: &str) -> LspCompletionItem {
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

    fn set_completion_origin(
        app: &mut KuroyaApp,
        id: u64,
        path: PathBuf,
        version: u64,
        line: usize,
        column: usize,
        prefix: &str,
    ) {
        app.completion_open = true;
        app.completion_items = vec![completion_item("HashMap")];
        app.completion_buffer_id = Some(id);
        app.completion_path = Some(path);
        app.completion_version = Some(version);
        app.completion_line = line;
        app.completion_column = column;
        app.completion_prefix = prefix.to_owned();
    }

    fn app_for_test(root: PathBuf) -> KuroyaApp {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let settings = EditorSettings::default();
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

    fn assert_safe_status_text(status: &str) {
        assert!(
            !status.chars().any(is_unsafe_status_char),
            "status contains unsafe display characters: {status:?}"
        );
    }

    fn is_unsafe_status_char(ch: char) -> bool {
        ch.is_control()
            || matches!(
                ch,
                '\u{061c}'
                    | '\u{200e}'
                    | '\u{200f}'
                    | '\u{2028}'..='\u{202e}'
                    | '\u{2066}'..='\u{2069}'
            )
    }
}
