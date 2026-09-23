use super::{
    BUFFER_FIND_MAX_MATCHES, BUFFER_FIND_QUERY_TOO_LONG_STATUS, LARGE_FILE_FIND_STATUS,
    buffer_find_query_too_large,
};
use crate::{KuroyaApp, path_display::display_error_label_cow};
use kuroya_core::RegexReplaceAllOptions;
use std::{fmt::Display, ops::Range};

const INVALID_REGEX_STATUS_PREFIX: &str = "Invalid regular expression: ";
const REPLACE_ALL_MAX_MATCHES: usize = 50_000;

impl KuroyaApp {
    pub(crate) fn replace_current_find_match(&mut self) {
        let Some(id) = self.active else {
            self.status = "No active file".to_owned();
            return;
        };
        if self.block_protected_preview_edit(id) {
            return;
        }
        if self.active_find_blocked_by_large_file_mode() {
            self.status = LARGE_FILE_FIND_STATUS.to_owned();
            return;
        }
        if buffer_find_query_too_large(&self.buffer_find_query) {
            self.status = BUFFER_FIND_QUERY_TOO_LONG_STATUS.to_owned();
            return;
        }
        let (match_index, range) = {
            let Some(buffer_index) = self.active_find_buffer_index() else {
                self.status = "No match to replace".to_owned();
                return;
            };
            let active_scope = self
                .buffers
                .get(buffer_index)
                .and_then(|buffer| self.active_find_scope(buffer));
            let current_find_match = self.buffer_find_match;
            let Some(matches) = self.find_matches_ref_for_buffer_index(buffer_index) else {
                self.status = "No match to replace".to_owned();
                return;
            };
            if matches.is_empty() {
                self.status = "No match to replace".to_owned();
                return;
            }
            if current_find_match >= matches.len() {
                self.buffer_find_match = matches.len() - 1;
                self.status = "No match to replace".to_owned();
                return;
            }
            let match_index = current_find_match;
            let Some(range) = matches.get(match_index).cloned() else {
                self.status = "No match to replace".to_owned();
                return;
            };
            if !find_match_range_within_scope(&range, active_scope.as_ref()) {
                self.status = "No match to replace".to_owned();
                return;
            }
            (match_index, range)
        };
        self.buffer_find_match = match_index;
        let query = self.buffer_find_query.clone();
        let replacement = self.buffer_find_replacement.clone();
        let case_sensitive = self.buffer_find_case_sensitive;
        let whole_word = self.buffer_find_whole_word;
        let regex = self.buffer_find_regex;
        let preserve_case = self.buffer_find_preserve_case;
        let replacement_len = if regex {
            let replaced = self
                .buffer_mut(id)
                .map(|buffer| {
                    buffer.replace_regex_match(
                        range.clone(),
                        &query,
                        &replacement,
                        case_sensitive,
                        whole_word,
                        preserve_case,
                    )
                })
                .transpose();
            match replaced {
                Ok(Some(Some(len))) => Some(len),
                Ok(Some(None)) | Ok(None) => None,
                Err(err) => {
                    self.status = invalid_regex_status(err);
                    return;
                }
            }
        } else {
            self.buffer_mut(id).and_then(|buffer| {
                if !literal_range_matches_query(buffer, &range, &query, case_sensitive, whole_word)
                {
                    return None;
                }
                buffer.replace_range_with_options(range.clone(), &replacement, preserve_case)
            })
        };
        let Some(replacement_len) = replacement_len else {
            self.status = "No match to replace".to_owned();
            return;
        };

        self.update_find_scope_after_replacement(id, &range, replacement_len);
        self.mark_buffer_changed(id);
        let remaining = self.active_find_match_count();
        if remaining == 0 {
            self.status = "Replaced last match".to_owned();
        } else {
            self.select_find_match_with_count(remaining);
        }
    }

    pub(crate) fn replace_all_find_matches(&mut self) {
        let Some(id) = self.active else {
            self.status = "No active file".to_owned();
            return;
        };
        if self.block_protected_preview_edit(id) {
            return;
        }
        if self.active_find_blocked_by_large_file_mode() {
            self.status = LARGE_FILE_FIND_STATUS.to_owned();
            return;
        }
        let query = self.buffer_find_query.clone();
        if query.is_empty() {
            self.status = "No query to replace".to_owned();
            return;
        }
        if buffer_find_query_too_large(&query) {
            self.status = BUFFER_FIND_QUERY_TOO_LONG_STATUS.to_owned();
            return;
        }

        let replacement = self.buffer_find_replacement.clone();
        let case_sensitive = self.buffer_find_case_sensitive;
        let whole_word = self.buffer_find_whole_word;
        let regex = self.buffer_find_regex;
        let preserve_case = self.buffer_find_preserve_case;
        let scope = self.active_buffer().and_then(|buffer| {
            let scope = self.active_find_scope(buffer)?;
            if scope.start >= scope.end {
                return Some(scope);
            }
            (scope.start > 0 || scope.end < buffer.len_chars()).then_some(scope)
        });
        if scope.as_ref().is_some_and(|scope| scope.start >= scope.end) {
            self.status = "No matches to replace".to_owned();
            return;
        }
        let count = if regex {
            let replaced = self
                .buffer_mut(id)
                .map(|buffer| {
                    buffer.replace_all_regex_matches_with_options(
                        &query,
                        &replacement,
                        RegexReplaceAllOptions {
                            case_sensitive,
                            whole_word,
                            scope,
                            max_matches: REPLACE_ALL_MAX_MATCHES,
                            preserve_case,
                        },
                    )
                })
                .transpose();
            match replaced {
                Ok(Some(count)) => count,
                Ok(None) => 0,
                Err(err) => {
                    self.status = invalid_regex_status(err);
                    return;
                }
            }
        } else if let Some(scope) = scope {
            self.buffer_mut(id)
                .map(|buffer| {
                    let matches = scoped_literal_match_ranges(
                        buffer,
                        &query,
                        scope,
                        case_sensitive,
                        whole_word,
                        REPLACE_ALL_MAX_MATCHES,
                    );
                    buffer.replace_match_ranges_with_options(matches, &replacement, preserve_case)
                })
                .unwrap_or_default()
        } else {
            self.buffer_mut(id)
                .map(|buffer| {
                    buffer.replace_all_matches_with_options(
                        &query,
                        &replacement,
                        case_sensitive,
                        whole_word,
                        REPLACE_ALL_MAX_MATCHES,
                        preserve_case,
                    )
                })
                .unwrap_or_default()
        };
        if count == 0 {
            self.status = "No matches to replace".to_owned();
            return;
        }

        self.mark_buffer_changed(id);
        self.buffer_find_scope = None;
        self.buffer_find_match = 0;
        let remaining = self.active_find_match_count();
        self.status = replace_all_status(
            count,
            remaining,
            count == REPLACE_ALL_MAX_MATCHES && remaining > 0,
        );
    }

    fn update_find_scope_after_replacement(
        &mut self,
        id: kuroya_core::BufferId,
        range: &std::ops::Range<usize>,
        replacement_len: usize,
    ) {
        let Some(scope) = self
            .buffer_find_scope
            .as_mut()
            .filter(|scope| scope.buffer_id == id)
        else {
            return;
        };
        if range.start > scope.range.end {
            return;
        }
        let replaced_len = range.end.saturating_sub(range.start);
        if replacement_len >= replaced_len {
            scope.range.end = scope
                .range
                .end
                .saturating_add(replacement_len.saturating_sub(replaced_len));
        } else {
            scope.range.end = scope
                .range
                .end
                .saturating_sub(replaced_len.saturating_sub(replacement_len))
                .max(scope.range.start);
        }
    }
}

fn scoped_literal_match_ranges(
    buffer: &kuroya_core::TextBuffer,
    query: &str,
    scope: Range<usize>,
    case_sensitive: bool,
    whole_word: bool,
    max_matches: usize,
) -> Vec<Range<usize>> {
    if query.is_empty() || max_matches == 0 {
        return Vec::new();
    }

    let scope = normalize_range(scope, buffer.len_chars());
    if scope.start >= scope.end {
        return Vec::new();
    }

    let Some(scoped_text) = buffer.text_range(scope.clone()) else {
        return Vec::new();
    };
    let mut scoped_buffer = kuroya_core::TextBuffer::from_text(buffer.id(), None, scoped_text);
    scoped_buffer.set_word_separators(buffer.word_separators().to_owned());
    let find_limit = if whole_word {
        max_matches.saturating_add(2)
    } else {
        max_matches
    };
    let mut matches =
        scoped_buffer.find_matches_with_options(query, find_limit, case_sensitive, whole_word);
    for range in &mut matches {
        range.start += scope.start;
        range.end += scope.start;
    }
    if whole_word {
        matches.retain(|range| buffer_range_is_whole_word(buffer, range));
        matches.truncate(max_matches);
    }
    matches
}

fn literal_range_matches_query(
    buffer: &kuroya_core::TextBuffer,
    range: &Range<usize>,
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
) -> bool {
    if query.is_empty() || range.start >= range.end {
        return false;
    }
    let Some(text) = buffer.text_range(range.clone()) else {
        return false;
    };
    literal_text_matches_query(&text, query, case_sensitive)
        && (!whole_word || buffer_range_is_whole_word(buffer, range))
}

fn find_match_range_within_scope(range: &Range<usize>, scope: Option<&Range<usize>>) -> bool {
    if range.start >= range.end {
        return false;
    }
    match scope {
        Some(scope) => range.start >= scope.start && range.end <= scope.end,
        None => true,
    }
}

fn literal_text_matches_query(text: &str, query: &str, case_sensitive: bool) -> bool {
    if case_sensitive {
        return text == query;
    }
    text.chars().count() == query.chars().count()
        && text
            .chars()
            .zip(query.chars())
            .all(|(left, right)| chars_match(left, right, false))
}

fn chars_match(left: char, right: char, case_sensitive: bool) -> bool {
    if case_sensitive {
        left == right
    } else {
        left.eq_ignore_ascii_case(&right)
    }
}

fn buffer_range_is_whole_word(buffer: &kuroya_core::TextBuffer, range: &Range<usize>) -> bool {
    let separators = buffer.word_separators();
    let before = range
        .start
        .checked_sub(1)
        .and_then(|idx| buffer.char_at(idx));
    let after = buffer.char_at(range.end);
    !before.is_some_and(|ch| is_word_char(ch, separators))
        && !after.is_some_and(|ch| is_word_char(ch, separators))
}

fn is_word_char(ch: char, separators: &str) -> bool {
    !ch.is_whitespace() && !separators.contains(ch)
}

fn normalize_range(range: Range<usize>, len_chars: usize) -> Range<usize> {
    let start = range.start.min(len_chars);
    let end = range.end.min(len_chars).max(start);
    start..end
}

fn replace_all_status(replaced: usize, remaining: usize, replace_limit_reached: bool) -> String {
    let replaced_label = match_label(replaced);
    let limit = if replace_limit_reached {
        " (limit reached)"
    } else {
        ""
    };
    if remaining == 0 {
        return format!("Replaced {replaced} {replaced_label}{limit}");
    }

    let remaining_count = if remaining >= BUFFER_FIND_MAX_MATCHES {
        format!("{remaining}+")
    } else {
        remaining.to_string()
    };
    let remaining_label = match_label(remaining);
    format!(
        "Replaced {replaced} {replaced_label}{limit}, {remaining_count} {remaining_label} remaining"
    )
}

fn match_label(count: usize) -> &'static str {
    if count == 1 { "match" } else { "matches" }
}

fn invalid_regex_status(error: impl Display) -> String {
    let error = error.to_string();
    let error = display_error_label_cow(&error);
    let mut status = String::with_capacity(INVALID_REGEX_STATUS_PREFIX.len() + error.len());
    status.push_str(INVALID_REGEX_STATUS_PREFIX);
    status.push_str(&error);
    status
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app_startup_context::AppStartupContext,
        buffer_find::{BUFFER_FIND_MAX_QUERY_BYTES, BufferFindCacheKey, BufferFindScope},
        path_display::DISPLAY_ERROR_LABEL_MAX_CHARS,
        terminal::TerminalPane,
        ui_event_channel::ui_event_channel,
    };
    use kuroya_core::{EditorSettings, TextBuffer, Workspace};
    use std::{
        path::PathBuf,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };
    use tokio::runtime::Runtime;

    #[test]
    fn literal_replace_all_ignores_scope_from_inactive_buffer() {
        let mut app = app_for_replace_test("stale-find-scope");
        let text = "x".repeat(5_001);
        app.buffers.push(TextBuffer::from_text(1, None, text));
        app.active = Some(1);
        app.buffer_find_query = "x".to_owned();
        app.buffer_find_replacement = "y".to_owned();
        app.buffer_find_scope = Some(BufferFindScope {
            buffer_id: 99,
            range: 0..1,
        });

        app.replace_all_find_matches();

        assert_eq!(app.buffer(1).map(TextBuffer::text), Some("y".repeat(5_001)));
        assert_eq!(app.buffer_find_scope, None);
        assert_eq!(app.status, "Replaced 5001 matches");
    }

    #[test]
    fn literal_replace_all_preserves_raw_replacement_text() {
        let mut app = app_for_replace_test("literal-replace-all-raw-replacement");
        let replacement = "  raw\n\tvalue  ";
        app.buffers
            .push(TextBuffer::from_text(1, None, "needle".to_owned()));
        app.active = Some(1);
        app.buffer_find_query = "needle".to_owned();
        app.buffer_find_replacement = replacement.to_owned();

        app.replace_all_find_matches();

        assert_eq!(
            app.buffer(1).map(TextBuffer::text),
            Some(replacement.to_owned())
        );
        assert_eq!(app.status, "Replaced 1 match");
    }

    #[test]
    fn literal_replace_current_rejects_stale_whole_word_match() {
        let mut app = app_for_replace_test("literal-replace-current-stale-whole-word");
        let buffer = TextBuffer::from_text(1, None, "aneedle needle".to_owned());
        app.buffers.push(buffer);
        app.active = Some(1);
        app.buffer_find_query = "needle".to_owned();
        app.buffer_find_replacement = "value".to_owned();
        app.buffer_find_whole_word = true;
        let cache_key = BufferFindCacheKey::for_buffer(
            &app.buffers[0],
            "needle",
            app.buffer_find_case_sensitive,
            app.buffer_find_whole_word,
            app.buffer_find_regex,
            None,
        );
        app.buffer_find_cache
            .store(cache_key, std::iter::once(1..7).collect());

        app.replace_current_find_match();

        assert_eq!(
            app.buffer(1).map(TextBuffer::text),
            Some("aneedle needle".to_owned())
        );
        assert_eq!(app.status, "No match to replace");
    }

    #[test]
    fn replace_current_rejects_stale_match_outside_scope() {
        let mut app = app_for_replace_test("replace-current-stale-outside-scope");
        let buffer = TextBuffer::from_text(1, None, "needle scope needle".to_owned());
        app.buffers.push(buffer);
        app.active = Some(1);
        app.buffer_find_query = "needle".to_owned();
        app.buffer_find_replacement = "value".to_owned();
        app.buffer_find_scope = Some(BufferFindScope {
            buffer_id: 1,
            range: 13..19,
        });
        let cache_key = BufferFindCacheKey::for_buffer(
            &app.buffers[0],
            "needle",
            app.buffer_find_case_sensitive,
            app.buffer_find_whole_word,
            app.buffer_find_regex,
            Some(13..19),
        );
        app.buffer_find_cache
            .store(cache_key, std::iter::once(0..6).collect());

        app.replace_current_find_match();

        assert_eq!(
            app.buffer(1).map(TextBuffer::text),
            Some("needle scope needle".to_owned())
        );
        assert_eq!(
            app.buffer_find_scope,
            Some(BufferFindScope {
                buffer_id: 1,
                range: 13..19,
            })
        );
        assert_eq!(app.status, "No match to replace");
    }

    #[test]
    fn literal_replace_current_rejects_stale_match_index() {
        let mut app = app_for_replace_test("literal-replace-current-stale-index");
        app.buffers
            .push(TextBuffer::from_text(1, None, "needle needle".to_owned()));
        app.active = Some(1);
        app.buffer_find_query = "needle".to_owned();
        app.buffer_find_replacement = "value".to_owned();
        app.buffer_find_match = 99;

        app.replace_current_find_match();

        assert_eq!(
            app.buffer(1).map(TextBuffer::text),
            Some("needle needle".to_owned())
        );
        assert_eq!(app.buffer_find_match, 1);
        assert_eq!(app.status, "No match to replace");
    }

    #[test]
    fn replace_current_rejects_oversized_query() {
        let mut app = app_for_replace_test("replace-current-oversized-query");
        app.buffers
            .push(TextBuffer::from_text(1, None, "needle".to_owned()));
        app.active = Some(1);
        app.buffer_find_query = "x".repeat(BUFFER_FIND_MAX_QUERY_BYTES + 1);
        app.buffer_find_replacement = "value".to_owned();

        app.replace_current_find_match();

        assert_eq!(
            app.buffer(1).map(TextBuffer::text),
            Some("needle".to_owned())
        );
        assert_eq!(app.status, BUFFER_FIND_QUERY_TOO_LONG_STATUS);
    }

    #[test]
    fn replace_all_invalid_regex_status_sanitizes_and_bounds_error() {
        let mut app = app_for_replace_test("replace-all-invalid-regex-status");
        app.buffers
            .push(TextBuffer::from_text(1, None, "needle".to_owned()));
        app.active = Some(1);
        app.buffer_find_regex = true;
        app.buffer_find_query = invalid_regex_with_display_hazards();
        app.buffer_find_replacement = "replacement".to_owned();

        app.replace_all_find_matches();

        assert_eq!(
            app.buffer(1).map(TextBuffer::text),
            Some("needle".to_owned())
        );
        assert_invalid_regex_status_sanitized_and_bounded(&app.status);
    }

    #[test]
    fn scoped_literal_replace_all_uses_replace_all_match_limit() {
        let mut app = app_for_replace_test("scoped-literal-replace-all-limit");
        let text = "x".repeat(5_001);
        app.buffers.push(TextBuffer::from_text(1, None, text));
        app.active = Some(1);
        app.buffer_find_query = "x".to_owned();
        app.buffer_find_replacement = "y".to_owned();
        app.buffer_find_scope = Some(BufferFindScope {
            buffer_id: 1,
            range: 0..5_001,
        });

        app.replace_all_find_matches();

        assert_eq!(app.buffer(1).map(TextBuffer::text), Some("y".repeat(5_001)));
        assert_eq!(app.status, "Replaced 5001 matches");
    }

    #[test]
    fn scoped_literal_replace_all_applies_match_limit_after_scope() {
        let mut app = app_for_replace_test("scoped-literal-replace-all-limit-after-scope");
        let prefix = "x".repeat(REPLACE_ALL_MAX_MATCHES);
        let scope_start = prefix.chars().count();
        app.buffers
            .push(TextBuffer::from_text(1, None, format!("{prefix}x")));
        app.active = Some(1);
        app.buffer_find_query = "x".to_owned();
        app.buffer_find_replacement = "y".to_owned();
        app.buffer_find_scope = Some(BufferFindScope {
            buffer_id: 1,
            range: scope_start..scope_start + 1,
        });

        app.replace_all_find_matches();

        let text = app.buffer(1).map(TextBuffer::text).unwrap_or_default();
        assert_eq!(text.chars().filter(|ch| *ch == 'y').count(), 1);
        assert_eq!(
            text.chars().filter(|ch| *ch == 'x').count(),
            REPLACE_ALL_MAX_MATCHES
        );
        assert_eq!(app.status, "Replaced 1 match, 5000+ matches remaining");
    }

    #[test]
    fn replace_all_rejects_empty_normalized_scope() {
        let mut app = app_for_replace_test("replace-all-empty-scope");
        app.buffers
            .push(TextBuffer::from_text(1, None, "needle".to_owned()));
        app.active = Some(1);
        app.buffer_find_query = "needle".to_owned();
        app.buffer_find_replacement = "value".to_owned();
        app.buffer_find_scope = Some(BufferFindScope {
            buffer_id: 1,
            range: std::ops::Range { start: 5, end: 2 },
        });

        app.replace_all_find_matches();

        assert_eq!(
            app.buffer(1).map(TextBuffer::text),
            Some("needle".to_owned())
        );
        assert_eq!(app.status, "No matches to replace");
    }

    #[test]
    fn replace_all_rejects_oversized_query() {
        let mut app = app_for_replace_test("replace-all-oversized-query");
        app.buffers
            .push(TextBuffer::from_text(1, None, "needle".to_owned()));
        app.active = Some(1);
        app.buffer_find_query = "x".repeat(BUFFER_FIND_MAX_QUERY_BYTES + 1);
        app.buffer_find_replacement = "value".to_owned();

        app.replace_all_find_matches();

        assert_eq!(
            app.buffer(1).map(TextBuffer::text),
            Some("needle".to_owned())
        );
        assert_eq!(app.status, BUFFER_FIND_QUERY_TOO_LONG_STATUS);
    }

    #[test]
    fn replace_all_status_labels_replace_limit() {
        let mut app = app_for_replace_test("replace-all-limit-status");
        app.buffers.push(TextBuffer::from_text(
            1,
            None,
            "x".repeat(REPLACE_ALL_MAX_MATCHES + 1),
        ));
        app.active = Some(1);
        app.buffer_find_query = "x".to_owned();
        app.buffer_find_replacement = "y".to_owned();

        app.replace_all_find_matches();

        let text = app.buffer(1).map(TextBuffer::text).unwrap_or_default();
        assert_eq!(
            text.chars().filter(|ch| *ch == 'y').count(),
            REPLACE_ALL_MAX_MATCHES
        );
        assert_eq!(text.chars().filter(|ch| *ch == 'x').count(), 1);
        assert_eq!(
            app.status,
            "Replaced 50000 matches (limit reached), 1 match remaining"
        );
    }

    #[test]
    fn invalid_regex_status_sanitizes_and_bounds_error_text() {
        let err = kuroya_core::validate_find_regex(&invalid_regex_with_display_hazards(), true)
            .expect_err("test regex should be invalid");

        let status = invalid_regex_status(err);

        assert_invalid_regex_status_sanitized_and_bounded(&status);
    }

    fn invalid_regex_with_display_hazards() -> String {
        format!(
            "({}\n{}\u{202e}",
            "pattern-fragment".repeat(DISPLAY_ERROR_LABEL_MAX_CHARS),
            "tail".repeat(DISPLAY_ERROR_LABEL_MAX_CHARS)
        )
    }

    fn assert_invalid_regex_status_sanitized_and_bounded(status: &str) {
        assert!(status.starts_with(INVALID_REGEX_STATUS_PREFIX));
        assert!(!status.contains('\n'));
        assert!(!status.contains('\r'));
        assert!(!status.contains('\u{202e}'));
        assert!(status.contains("..."));
        assert!(
            status[INVALID_REGEX_STATUS_PREFIX.len()..].chars().count()
                <= DISPLAY_ERROR_LABEL_MAX_CHARS
        );
    }

    fn app_for_replace_test(name: &str) -> KuroyaApp {
        let root = temp_root(name);
        let (tx, rx) = ui_event_channel();
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

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("kuroya-{name}-{}-{nanos}", std::process::id()))
    }
}
