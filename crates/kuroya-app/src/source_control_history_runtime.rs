use crate::{
    KuroyaApp,
    devtools_async_tasks::path_detail,
    path_display::display_error_label_cow,
    source_control_runtime::{
        finish_source_control_load_request_state, source_control_panel_load_event_matches,
    },
    ui_events::UiEvent,
};
use kuroya_core::{
    GitCommitSummary, MAX_SCM_GRAPH_PAGE_SIZE, clamp_scm_graph_page_size,
    list_commit_history_with_timeline_date, text_match::ascii_case_insensitive_contains,
};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

impl KuroyaApp {
    pub(crate) fn begin_git_history_panel(&mut self) {
        self.source_control_history_open = true;
        self.git_panel_open_generation += 1;
        self.source_control_history_query.clear();
        self.source_control_history_selected = 0;
        self.source_control_history_has_more = false;
        self.spawn_git_history_load(self.initial_git_history_limit());
    }

    pub(crate) fn apply_git_history_loaded(
        &mut self,
        request_id: u64,
        limit: usize,
        root: PathBuf,
        operation_root: PathBuf,
        commits: Vec<GitCommitSummary>,
    ) {
        let limit = source_control_history_request_limit(limit);
        if !source_control_panel_load_event_matches(
            self.source_control_history_open,
            &self.workspace.root,
            &root,
            request_id,
            self.source_control_history_active_request_id,
        ) {
            return;
        }
        if !self.source_control_git_operation_root_matches(&operation_root) {
            return;
        }

        let count = commits.len();
        let now_seconds = current_unix_seconds();
        let previous_selected = self.source_control_history_selected;
        let selected_oid = source_control_history_selected_oid_for_query(
            &self.source_control_history,
            &self.source_control_history_query,
            previous_selected,
            now_seconds,
        )
        .map(str::to_owned);
        let history = source_control_history_with_uncommitted(
            commits,
            self.settings.git_timeline_show_uncommitted,
            self.git.counts().total() > 0,
            now_seconds,
        );
        self.source_control_history_selected =
            source_control_history_selection_after_reload_for_query(
                selected_oid.as_deref(),
                previous_selected,
                &history,
                &self.source_control_history_query,
                now_seconds,
            );
        self.source_control_history = history;
        self.source_control_history_loading = false;
        self.source_control_history_requested_limit = limit;
        self.source_control_history_has_more = source_control_history_has_more(count, limit);
        self.status = git_history_success_status(count);
    }

    pub(crate) fn apply_git_history_failed(
        &mut self,
        request_id: u64,
        limit: usize,
        root: PathBuf,
        operation_root: PathBuf,
        error: String,
    ) {
        let limit = source_control_history_request_limit(limit);
        if !source_control_panel_load_event_matches(
            self.source_control_history_open,
            &self.workspace.root,
            &root,
            request_id,
            self.source_control_history_active_request_id,
        ) {
            return;
        }
        if !self.source_control_git_operation_root_matches(&operation_root) {
            return;
        }

        self.source_control_history.clear();
        self.source_control_history_loading = false;
        self.source_control_history_requested_limit = limit;
        self.source_control_history_has_more = false;
        self.status = git_history_failure_status(&error);
    }

    pub(crate) fn request_more_git_history(&mut self) {
        if !source_control_history_can_load_more(
            self.source_control_history_loading,
            self.source_control_history_has_more,
        ) {
            return;
        }

        let committed_len = source_control_committed_history_len(&self.source_control_history);
        let limit = next_git_history_limit(
            committed_len,
            self.source_control_history_requested_limit,
            self.settings.scm_graph_page_size,
        );
        self.spawn_git_history_load(limit);
    }

    pub(crate) fn spawn_git_history_load(&mut self, limit: usize) -> bool {
        let limit = source_control_history_request_limit(limit);
        self.source_control_history_loading = true;
        self.source_control_history_requested_limit = limit;
        let Some(request_id) = self.begin_source_control_history_request() else {
            self.set_git_progress_status(git_history_pending_status());
            return false;
        };
        let event_root = self.workspace.root.clone();
        let git_root = self.source_control_git_operation_root();
        let tx = self.tx.clone();
        let short_hash_length = self.settings.git_commit_short_hash_length;
        let timeline_date = self.settings.git_timeline_date;
        self.set_git_progress_status(git_history_pending_status());
        self.record_async_task_started("Git History", path_detail(&event_root));
        self.runtime.spawn_blocking(move || {
            let result = list_commit_history_with_timeline_date(
                &git_root,
                limit,
                short_hash_length,
                timeline_date,
            );
            let event = match result {
                Ok(commits) => UiEvent::GitHistoryLoaded {
                    request_id,
                    limit,
                    root: event_root,
                    operation_root: git_root,
                    commits,
                },
                Err(error) => UiEvent::GitHistoryFailed {
                    request_id,
                    limit,
                    root: event_root,
                    operation_root: git_root,
                    error: error.to_string(),
                },
            };
            let _ = crate::ui_event_channel::send_ui_event(&tx, event);
        });
        true
    }

    pub(crate) fn spawn_restored_git_history_load(&mut self) -> bool {
        if !self.settings.git_enabled || !self.source_control_history_open {
            return false;
        }
        self.spawn_git_history_load(self.initial_git_history_limit())
    }

    fn begin_source_control_history_request(&mut self) -> Option<u64> {
        begin_source_control_history_request_state(
            &mut self.source_control_history_next_request_id,
            &mut self.source_control_history_active_request_id,
            &mut self.source_control_history_in_flight_request_id,
            &mut self.source_control_history_reload_queued,
        )
    }

    pub(crate) fn finish_source_control_history_request(&mut self, request_id: u64) -> bool {
        finish_source_control_load_request_state(
            &mut self.source_control_history_in_flight_request_id,
            &mut self.source_control_history_reload_queued,
            request_id,
        )
    }

    fn initial_git_history_limit(&self) -> usize {
        clamp_scm_graph_page_size(self.settings.scm_graph_page_size)
    }
}

fn next_source_control_history_request_id(current: u64) -> u64 {
    current.checked_add(1).filter(|id| *id != 0).unwrap_or(1)
}

fn begin_source_control_history_request_state(
    next_request_id: &mut u64,
    active_request_id: &mut u64,
    in_flight_request_id: &mut Option<u64>,
    reload_queued: &mut bool,
) -> Option<u64> {
    let request_id = reserve_source_control_history_request_id_state(
        next_request_id,
        active_request_id,
        *in_flight_request_id,
    );
    if in_flight_request_id.is_some() {
        *reload_queued = true;
        None
    } else {
        *in_flight_request_id = Some(request_id);
        Some(request_id)
    }
}

fn reserve_source_control_history_request_id_state(
    next_request_id: &mut u64,
    active_request_id: &mut u64,
    reserved_request_id: Option<u64>,
) -> u64 {
    let mut request_id = next_source_control_history_request_id(*next_request_id);
    if Some(request_id) == reserved_request_id {
        request_id = next_source_control_history_request_id(request_id);
    }
    *next_request_id = request_id;
    *active_request_id = request_id;
    request_id
}

pub(crate) const GIT_UNCOMMITTED_HISTORY_OID: &str = "kuroya:uncommitted";
const SOURCE_CONTROL_HISTORY_MAX_REQUESTED_COMMITS: usize = MAX_SCM_GRAPH_PAGE_SIZE * 100;

pub(crate) fn source_control_history_commit_is_uncommitted(commit: &GitCommitSummary) -> bool {
    commit.oid == GIT_UNCOMMITTED_HISTORY_OID
}

pub(crate) fn source_control_uncommitted_history_entry(now_seconds: i64) -> GitCommitSummary {
    GitCommitSummary {
        oid: GIT_UNCOMMITTED_HISTORY_OID.to_owned(),
        short_oid: "uncommitted".to_owned(),
        summary: "Uncommitted Changes".to_owned(),
        author: "Working Tree".to_owned(),
        time_seconds: now_seconds,
    }
}

pub(crate) fn source_control_history_with_uncommitted(
    mut commits: Vec<GitCommitSummary>,
    show_uncommitted: bool,
    has_uncommitted_changes: bool,
    now_seconds: i64,
) -> Vec<GitCommitSummary> {
    commits.retain(|commit| !source_control_history_commit_is_uncommitted(commit));
    if !show_uncommitted || !has_uncommitted_changes {
        return commits;
    }

    commits.insert(0, source_control_uncommitted_history_entry(now_seconds));
    commits
}

pub(crate) fn source_control_committed_history_len(commits: &[GitCommitSummary]) -> usize {
    commits
        .iter()
        .filter(|commit| !source_control_history_commit_is_uncommitted(commit))
        .count()
}

pub(crate) fn source_control_history_selection_after_reload(
    selected_oid: Option<&str>,
    previous_selected: usize,
    history: &[GitCommitSummary],
    filtered_indices: &[usize],
) -> usize {
    if let Some(selected_oid) = selected_oid
        && let Some(index) = filtered_indices.iter().position(|history_index| {
            history
                .get(*history_index)
                .is_some_and(|commit| commit.oid == selected_oid)
        })
    {
        return index;
    }

    previous_selected.min(filtered_indices.len().saturating_sub(1))
}

pub(crate) fn source_control_history_selection_after_reload_for_query(
    selected_oid: Option<&str>,
    previous_selected: usize,
    history: &[GitCommitSummary],
    query: &str,
    now_seconds: i64,
) -> usize {
    if source_control_history_query_is_empty(query) {
        return source_control_history_selection_after_unfiltered_reload(
            selected_oid,
            previous_selected,
            history,
        );
    }

    let filtered_indices = source_control_filtered_history_indices(history, query, now_seconds);
    source_control_history_selection_after_reload(
        selected_oid,
        previous_selected,
        history,
        &filtered_indices,
    )
}

fn source_control_history_selection_after_unfiltered_reload(
    selected_oid: Option<&str>,
    previous_selected: usize,
    history: &[GitCommitSummary],
) -> usize {
    if let Some(selected_oid) = selected_oid
        && let Some(index) = history.iter().position(|commit| commit.oid == selected_oid)
    {
        return index;
    }

    previous_selected.min(history.len().saturating_sub(1))
}

fn source_control_history_selected_oid_for_query<'a>(
    commits: &'a [GitCommitSummary],
    query: &str,
    selected: usize,
    now_seconds: i64,
) -> Option<&'a str> {
    if source_control_history_query_is_empty(query) {
        return commits.get(selected).map(|commit| commit.oid.as_str());
    }

    let filtered_indices = source_control_filtered_history_indices(commits, query, now_seconds);
    source_control_history_selected_oid(commits, &filtered_indices, selected)
}

fn source_control_history_selected_oid<'a>(
    commits: &'a [GitCommitSummary],
    filtered_indices: &[usize],
    selected: usize,
) -> Option<&'a str> {
    filtered_indices
        .get(selected)
        .and_then(|index| commits.get(*index))
        .map(|commit| commit.oid.as_str())
}

pub(crate) fn source_control_filtered_history_indices(
    commits: &[GitCommitSummary],
    query: &str,
    now_seconds: i64,
) -> Vec<usize> {
    let Some(terms) = source_control_history_filter_terms(query) else {
        let mut indices = Vec::with_capacity(commits.len());
        indices.extend(0..commits.len());
        return indices;
    };

    let mut indices = Vec::with_capacity(commits.len());
    for (index, commit) in commits.iter().enumerate() {
        if terms
            .iter()
            .all(|term| source_control_history_matches_term(commit, *term, now_seconds))
        {
            indices.push(index);
        }
    }
    indices
}

fn source_control_history_query_is_empty(query: &str) -> bool {
    query.split_whitespace().next().is_none()
}

fn source_control_history_filter_terms(
    query: &str,
) -> Option<Vec<SourceControlHistoryFilterTerm<'_>>> {
    let mut terms = query.split_whitespace();
    let first = terms.next()?;
    let mut parsed = Vec::with_capacity(1 + terms.size_hint().0);
    parsed.push(SourceControlHistoryFilterTerm::parse(first));
    parsed.extend(terms.map(SourceControlHistoryFilterTerm::parse));
    Some(parsed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceControlHistoryFilterScope {
    Oid,
    Message,
    Author,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceControlHistoryFilterTerm<'a> {
    Plain(&'a str),
    Scoped {
        scope: SourceControlHistoryFilterScope,
        value: &'a str,
    },
}

impl<'a> SourceControlHistoryFilterTerm<'a> {
    fn parse(term: &'a str) -> Self {
        source_control_history_scoped_filter_term(term)
            .map(|(scope, value)| Self::Scoped { scope, value })
            .unwrap_or(Self::Plain(term))
    }
}

fn source_control_history_scoped_filter_term(
    term: &str,
) -> Option<(SourceControlHistoryFilterScope, &str)> {
    let (scope, value) = term.split_once(':')?;
    let scope = if scope.eq_ignore_ascii_case("oid") {
        SourceControlHistoryFilterScope::Oid
    } else if scope.eq_ignore_ascii_case("message") {
        SourceControlHistoryFilterScope::Message
    } else if scope.eq_ignore_ascii_case("author") {
        SourceControlHistoryFilterScope::Author
    } else {
        return None;
    };
    Some((scope, value))
}

fn source_control_history_matches_term(
    commit: &GitCommitSummary,
    term: SourceControlHistoryFilterTerm<'_>,
    now_seconds: i64,
) -> bool {
    match term {
        SourceControlHistoryFilterTerm::Scoped { scope, value } => {
            !value.is_empty()
                && match scope {
                    SourceControlHistoryFilterScope::Oid => {
                        ascii_case_insensitive_contains(&commit.oid, value)
                            || ascii_case_insensitive_contains(&commit.short_oid, value)
                    }
                    SourceControlHistoryFilterScope::Message => {
                        ascii_case_insensitive_contains(&commit.summary, value)
                    }
                    SourceControlHistoryFilterScope::Author => {
                        ascii_case_insensitive_contains(&commit.author, value)
                    }
                }
        }
        SourceControlHistoryFilterTerm::Plain(term) => {
            ascii_case_insensitive_contains(&commit.oid, term)
                || ascii_case_insensitive_contains(&commit.short_oid, term)
                || ascii_case_insensitive_contains(&commit.summary, term)
                || ascii_case_insensitive_contains(&commit.author, term)
                || source_control_history_age_is_within_term(commit, term, now_seconds)
        }
    }
}

fn source_control_history_age_is_within_term(
    commit: &GitCommitSummary,
    term: &str,
    now_seconds: i64,
) -> bool {
    let Some(max_age_seconds) = source_control_history_age_term_seconds(term) else {
        return false;
    };
    now_seconds.saturating_sub(commit.time_seconds) <= max_age_seconds
}

fn source_control_history_age_term_seconds(term: &str) -> Option<i64> {
    let (unit_start, _) = term.char_indices().next_back()?;
    let (amount, unit) = term.split_at(unit_start);
    if amount.is_empty() || !amount.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let unit_seconds = match unit {
        "s" | "S" => 1,
        "m" | "M" => 60,
        "h" | "H" => 60 * 60,
        "d" | "D" => 24 * 60 * 60,
        "w" | "W" => 7 * 24 * 60 * 60,
        _ => return None,
    };
    amount.parse::<i64>().ok()?.checked_mul(unit_seconds)
}

pub(crate) fn source_control_commit_age_label_at(
    commit: &GitCommitSummary,
    now_seconds: i64,
) -> String {
    let seconds = now_seconds.saturating_sub(commit.time_seconds);
    if seconds < 60 {
        return "just now".to_owned();
    }

    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m ago");
    }

    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h ago");
    }

    let days = hours / 24;
    if days < 30 {
        return format!("{days}d ago");
    }

    if days < 365 {
        let months = (days / 30).max(1);
        return format!("{months}mo ago");
    }

    let years = (days / 365).max(1);
    format!("{years}y ago")
}

fn current_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or_default()
}

pub(crate) fn next_git_history_limit(
    current_len: usize,
    requested_limit: usize,
    page_size: usize,
) -> usize {
    let page_size = clamp_scm_graph_page_size(page_size);
    current_len
        .max(source_control_history_request_limit(requested_limit))
        .saturating_add(page_size)
        .min(SOURCE_CONTROL_HISTORY_MAX_REQUESTED_COMMITS)
}

pub(crate) fn source_control_history_can_load_more(loading: bool, has_more: bool) -> bool {
    !loading && has_more
}

pub(crate) fn source_control_history_has_more(count: usize, limit: usize) -> bool {
    let limit = source_control_history_request_limit(limit);
    limit > 0 && limit < SOURCE_CONTROL_HISTORY_MAX_REQUESTED_COMMITS && count >= limit
}

fn source_control_history_request_limit(limit: usize) -> usize {
    if limit == 0 {
        0
    } else {
        limit.min(SOURCE_CONTROL_HISTORY_MAX_REQUESTED_COMMITS)
    }
}

pub(crate) fn source_control_history_should_page_on_scroll(
    page_on_scroll: bool,
    loading: bool,
    has_more: bool,
    offset_y: f32,
    viewport_height: f32,
    content_height: f32,
) -> bool {
    if !page_on_scroll || !source_control_history_can_load_more(loading, has_more) {
        return false;
    }
    offset_y + viewport_height + 24.0 >= content_height
}

pub(crate) fn git_history_pending_status() -> String {
    "Loading git history".to_owned()
}

pub(crate) fn git_history_success_status(count: usize) -> String {
    match count {
        0 => "No git history found".to_owned(),
        1 => "Loaded 1 commit".to_owned(),
        _ => format!("Loaded {count} commits"),
    }
}

pub(crate) fn git_history_failure_status(error: &str) -> String {
    let error = display_error_label_cow(error);
    format!("Could not load git history: {}", error.as_ref())
}

#[cfg(test)]
mod tests {
    use super::{
        SOURCE_CONTROL_HISTORY_MAX_REQUESTED_COMMITS, begin_source_control_history_request_state,
        git_history_failure_status, next_git_history_limit,
        source_control_filtered_history_indices, source_control_history_has_more,
        source_control_history_selection_after_reload,
        source_control_history_selection_after_reload_for_query,
    };
    use crate::{
        path_display::DISPLAY_ERROR_LABEL_MAX_CHARS,
        source_control_runtime::source_control_app_for_test,
    };
    use kuroya_core::GitCommitSummary;
    use std::path::PathBuf;

    fn commit(short_oid: &str, summary: &str, time_seconds: i64) -> GitCommitSummary {
        GitCommitSummary {
            oid: format!("{short_oid}00000000000000000000000000000000"),
            short_oid: short_oid.to_owned(),
            summary: summary.to_owned(),
            author: "Kuroya Test".to_owned(),
            time_seconds,
        }
    }

    #[test]
    fn history_request_ids_wrap_instead_of_saturating() {
        let mut next_request_id = u64::MAX - 1;
        let mut active_request_id = 0;
        let mut in_flight_request_id = None;
        let mut reload_queued = false;

        assert_eq!(
            begin_source_control_history_request_state(
                &mut next_request_id,
                &mut active_request_id,
                &mut in_flight_request_id,
                &mut reload_queued,
            ),
            Some(u64::MAX)
        );
        assert_eq!(next_request_id, u64::MAX);
        assert_eq!(active_request_id, u64::MAX);
        assert_eq!(in_flight_request_id, Some(u64::MAX));
        assert!(!reload_queued);

        assert_eq!(
            begin_source_control_history_request_state(
                &mut next_request_id,
                &mut active_request_id,
                &mut in_flight_request_id,
                &mut reload_queued,
            ),
            None
        );
        assert_eq!(next_request_id, 1);
        assert_eq!(active_request_id, 1);
        assert_eq!(in_flight_request_id, Some(u64::MAX));
        assert!(reload_queued);
    }

    #[test]
    fn queued_history_request_gets_fresh_id_after_wrapped_in_flight_finishes() {
        let root = PathBuf::from("workspace");
        let mut app = source_control_app_for_test(root, true);
        app.source_control_history_next_request_id = u64::MAX - 1;

        assert_eq!(app.begin_source_control_history_request(), Some(u64::MAX));
        assert_eq!(app.begin_source_control_history_request(), None);
        assert_eq!(app.source_control_history_active_request_id, 1);
        assert_eq!(
            app.source_control_history_in_flight_request_id,
            Some(u64::MAX)
        );
        assert!(app.source_control_history_reload_queued);

        assert!(app.finish_source_control_history_request(u64::MAX));
        assert_eq!(app.source_control_history_in_flight_request_id, None);
        assert!(!app.source_control_history_reload_queued);
        assert_eq!(app.begin_source_control_history_request(), Some(2));
        assert_eq!(app.source_control_history_active_request_id, 2);
        assert_eq!(app.source_control_history_in_flight_request_id, Some(2));
    }

    #[test]
    fn queued_history_request_ids_skip_current_in_flight_after_wrap() {
        let mut next_request_id = u64::MAX;
        let mut active_request_id = 1;
        let mut in_flight_request_id = Some(1);
        let mut reload_queued = false;

        assert_eq!(
            begin_source_control_history_request_state(
                &mut next_request_id,
                &mut active_request_id,
                &mut in_flight_request_id,
                &mut reload_queued,
            ),
            None
        );
        assert_eq!(next_request_id, 2);
        assert_eq!(active_request_id, 2);
        assert_eq!(in_flight_request_id, Some(1));
        assert!(reload_queued);
    }

    #[test]
    fn stale_history_load_after_wrapped_queued_reload_does_not_apply() {
        let root = PathBuf::from("workspace");
        let existing = commit("aaaaaaaa", "Current history", 20);
        let stale = commit("bbbbbbbb", "Stale history", 30);
        let mut app = source_control_app_for_test(root.clone(), true);
        app.source_control_history_open = true;
        app.source_control_history_next_request_id = u64::MAX;
        app.source_control_history_active_request_id = 1;
        app.source_control_history_in_flight_request_id = Some(1);
        app.source_control_history_loading = true;
        app.source_control_history_requested_limit = 8;
        app.source_control_history_has_more = true;
        app.source_control_history = vec![existing.clone()];
        app.status = "current history".to_owned();

        assert!(!app.spawn_git_history_load(8));
        app.status = "current history".to_owned();
        app.apply_git_history_loaded(1, 1, root.clone(), root, vec![stale]);

        assert_eq!(app.source_control_history_next_request_id, 2);
        assert_eq!(app.source_control_history_active_request_id, 2);
        assert_eq!(app.source_control_history_in_flight_request_id, Some(1));
        assert!(app.source_control_history_reload_queued);
        assert_eq!(app.source_control_history, vec![existing]);
        assert!(app.source_control_history_loading);
        assert_eq!(app.source_control_history_requested_limit, 8);
        assert!(app.source_control_history_has_more);
        assert_eq!(app.status, "current history");
    }

    #[test]
    fn history_reload_preserves_selected_commit_by_oid() {
        let root = PathBuf::from("workspace");
        let old_head = commit("aaaaaaaa", "Old head", 20);
        let selected = commit("bbbbbbbb", "Selected commit", 10);
        let new_head = commit("cccccccc", "New head", 30);
        let mut app = source_control_app_for_test(root.clone(), true);
        app.source_control_history_open = true;
        app.source_control_history_active_request_id = 1;
        app.source_control_history = vec![old_head.clone(), selected.clone()];
        app.source_control_history_selected = 1;

        app.apply_git_history_loaded(
            1,
            3,
            root.clone(),
            root,
            vec![new_head, old_head, selected.clone()],
        );

        assert_eq!(app.source_control_history_selected, 2);
        assert_eq!(app.source_control_history[2].oid, selected.oid);
    }

    #[test]
    fn history_reload_clamps_selection_when_selected_commit_disappears() {
        let old_head = commit("aaaaaaaa", "Old head", 20);
        let removed = commit("bbbbbbbb", "Removed commit", 10);
        let new_head = commit("cccccccc", "New head", 30);
        let history = vec![new_head, old_head];
        let filtered_indices = source_control_filtered_history_indices(&history, "", 60);

        assert_eq!(
            source_control_history_selection_after_reload(
                Some(&removed.oid),
                1,
                &history,
                &filtered_indices
            ),
            1
        );
        assert_eq!(
            source_control_history_selection_after_reload(
                Some(&removed.oid),
                4,
                &history,
                &filtered_indices
            ),
            1
        );
        assert_eq!(
            source_control_history_selection_after_reload(Some(&removed.oid), 4, &[], &[]),
            0
        );
    }

    #[test]
    fn unfiltered_history_reload_preserves_selection_without_filtered_indices() {
        let old_head = commit("aaaaaaaa", "Old head", 20);
        let selected = commit("bbbbbbbb", "Selected commit", 10);
        let new_head = commit("cccccccc", "New head", 30);
        let history = vec![new_head, old_head, selected.clone()];

        assert_eq!(
            source_control_history_selection_after_reload_for_query(
                Some(&selected.oid),
                1,
                &history,
                " \t\n ",
                60
            ),
            2
        );
        assert_eq!(
            source_control_history_selection_after_reload_for_query(
                Some("missing"),
                7,
                &history,
                "",
                60
            ),
            2
        );
    }

    #[test]
    fn filtered_history_reload_helper_matches_filtered_index_selection() {
        let unfiltered = commit("aaaaaaaa", "docs update", 40);
        let first_match = commit("bbbbbbbb", "feature first", 30);
        let selected = commit("cccccccc", "feature selected", 20);
        let inserted_match = commit("dddddddd", "feature new head", 50);
        let history = vec![inserted_match, unfiltered, first_match, selected.clone()];
        let filtered_indices = source_control_filtered_history_indices(&history, "feature", 60);

        assert_eq!(
            source_control_history_selection_after_reload_for_query(
                Some(&selected.oid),
                1,
                &history,
                "feature",
                60
            ),
            source_control_history_selection_after_reload(
                Some(&selected.oid),
                1,
                &history,
                &filtered_indices,
            )
        );
    }

    #[test]
    fn filtered_history_reload_preserves_selected_filtered_commit_by_oid() {
        let root = PathBuf::from("workspace");
        let unfiltered = commit("aaaaaaaa", "docs update", 30);
        let selected = commit("bbbbbbbb", "feature selected", 20);
        let second_match = commit("cccccccc", "feature second", 10);
        let new_head = commit("dddddddd", "new docs", 40);
        let mut app = source_control_app_for_test(root.clone(), true);
        app.source_control_history_open = true;
        app.source_control_history_active_request_id = 1;
        app.source_control_history_query = "feature".to_owned();
        app.source_control_history =
            vec![unfiltered.clone(), selected.clone(), second_match.clone()];
        app.source_control_history_selected = 0;

        app.apply_git_history_loaded(
            1,
            4,
            root.clone(),
            root,
            vec![new_head, unfiltered, selected.clone(), second_match],
        );

        let filtered =
            source_control_filtered_history_indices(&app.source_control_history, "feature", 60);
        let selected_raw = filtered[app.source_control_history_selected];
        assert_eq!(app.source_control_history_selected, 0);
        assert_eq!(app.source_control_history[selected_raw].oid, selected.oid);
    }

    #[test]
    fn filtered_history_reload_returns_filtered_row_when_matching_head_is_inserted() {
        let root = PathBuf::from("workspace");
        let unfiltered = commit("aaaaaaaa", "docs update", 40);
        let first_match = commit("bbbbbbbb", "feature first", 30);
        let selected = commit("cccccccc", "feature selected", 20);
        let inserted_match = commit("dddddddd", "feature new head", 50);
        let mut app = source_control_app_for_test(root.clone(), true);
        app.source_control_history_open = true;
        app.source_control_history_active_request_id = 1;
        app.source_control_history_query = "feature".to_owned();
        app.source_control_history =
            vec![unfiltered.clone(), first_match.clone(), selected.clone()];
        app.source_control_history_selected = 1;

        app.apply_git_history_loaded(
            1,
            4,
            root.clone(),
            root,
            vec![inserted_match, unfiltered, first_match, selected.clone()],
        );

        let filtered =
            source_control_filtered_history_indices(&app.source_control_history, "feature", 60);
        let selected_raw = filtered[app.source_control_history_selected];
        assert_eq!(app.source_control_history_selected, 2);
        assert_eq!(app.source_control_history[selected_raw].oid, selected.oid);
    }

    #[test]
    fn filtered_history_reload_clamps_selection_against_filtered_results() {
        let root = PathBuf::from("workspace");
        let unfiltered = commit("aaaaaaaa", "docs update", 40);
        let first_match = commit("bbbbbbbb", "feature first", 30);
        let selected = commit("cccccccc", "feature removed", 20);
        let mut app = source_control_app_for_test(root.clone(), true);
        app.source_control_history_open = true;
        app.source_control_history_active_request_id = 1;
        app.source_control_history_query = "feature".to_owned();
        app.source_control_history = vec![unfiltered.clone(), first_match.clone(), selected];
        app.source_control_history_selected = 1;

        app.apply_git_history_loaded(
            1,
            2,
            root.clone(),
            root,
            vec![unfiltered, first_match.clone()],
        );

        let filtered =
            source_control_filtered_history_indices(&app.source_control_history, "feature", 60);
        assert_eq!(app.source_control_history_selected, 0);
        assert_eq!(filtered, vec![1]);
        assert_eq!(app.source_control_history[filtered[0]].oid, first_match.oid);
    }

    #[test]
    fn filtered_history_reload_with_no_matches_keeps_zero_selection() {
        let root = PathBuf::from("workspace");
        let unfiltered = commit("aaaaaaaa", "docs update", 40);
        let selected = commit("bbbbbbbb", "feature selected", 20);
        let mut app = source_control_app_for_test(root.clone(), true);
        app.source_control_history_open = true;
        app.source_control_history_active_request_id = 1;
        app.source_control_history_query = "feature".to_owned();
        app.source_control_history = vec![selected];
        app.source_control_history_selected = 0;

        app.apply_git_history_loaded(1, 1, root.clone(), root, vec![unfiltered]);

        assert_eq!(
            source_control_filtered_history_indices(&app.source_control_history, "feature", 60),
            Vec::<usize>::new()
        );
        assert_eq!(app.source_control_history_selected, 0);
    }

    #[test]
    fn history_filter_scoped_terms_restrict_matching_to_one_field() {
        let commits = vec![commit("aaaaaaaa", "Add search panel", 10)];

        // The author is "Kuroya Test" and the summary is "Add search panel".
        assert_eq!(
            source_control_filtered_history_indices(&commits, "author:kuroya", 60),
            vec![0]
        );
        assert_eq!(
            source_control_filtered_history_indices(&commits, "AUTHOR:Test", 60),
            vec![0]
        );
        assert!(source_control_filtered_history_indices(&commits, "author:search", 60).is_empty());

        assert_eq!(
            source_control_filtered_history_indices(&commits, "message:SEARCH", 60),
            vec![0]
        );
        assert!(source_control_filtered_history_indices(&commits, "message:kuroya", 60).is_empty());

        // The oid scope matches both the full oid and the short oid.
        assert_eq!(
            source_control_filtered_history_indices(&commits, "oid:aaaaaaaa", 60),
            vec![0]
        );
        assert_eq!(
            source_control_filtered_history_indices(&commits, "oid:00000000", 60),
            vec![0]
        );
        assert!(source_control_filtered_history_indices(&commits, "oid:kuroya", 60).is_empty());

        // Unknown scopes stay plain terms; empty scoped values match nothing.
        assert!(source_control_filtered_history_indices(&commits, "scope:search", 60).is_empty());
        assert!(source_control_filtered_history_indices(&commits, "author:", 60).is_empty());
    }

    #[test]
    fn history_filter_plain_age_terms_match_commits_within_the_duration() {
        let now = 7 * 24 * 60 * 60;
        let commits = vec![
            commit("aaaaaaaa", "just committed", now - 30),
            commit("bbbbbbbb", "hour old", now - 60 * 60),
            commit("cccccccc", "day old", now - 24 * 60 * 60),
            commit("dddddddd", "week old", 0),
        ];

        assert_eq!(
            source_control_filtered_history_indices(&commits, "30s", now),
            vec![0]
        );
        assert_eq!(
            source_control_filtered_history_indices(&commits, "1h", now),
            vec![0, 1]
        );
        assert_eq!(
            source_control_filtered_history_indices(&commits, "1H", now),
            vec![0, 1]
        );
        // A commit exactly at the boundary is still within the duration.
        assert_eq!(
            source_control_filtered_history_indices(&commits, "1d", now),
            vec![0, 1, 2]
        );
        assert_eq!(
            source_control_filtered_history_indices(&commits, "1w", now),
            vec![0, 1, 2, 3]
        );
        // Scoped and age terms combine with AND semantics.
        assert_eq!(
            source_control_filtered_history_indices(&commits, "author:Kuroya 1h", now),
            vec![0, 1]
        );
        // Terms that do not parse as an age never match by time.
        assert!(source_control_filtered_history_indices(&commits, "2x", now).is_empty());
    }

    #[test]
    fn history_filter_no_longer_matches_rendered_age_labels() {
        let now = 90;
        // The commit is 90 seconds old and renders as "1m ago".
        let commits = vec![commit("aaaaaaaa", "Fix scrollback", 0)];

        // The rendered age label text no longer matches.
        assert!(source_control_filtered_history_indices(&commits, "1m", now).is_empty());
        assert!(source_control_filtered_history_indices(&commits, "1m ago", now).is_empty());
        assert!(source_control_filtered_history_indices(&commits, "just now", now).is_empty());
        // ...but an age range covering the commit still matches, and plain
        // text matching is unchanged.
        assert_eq!(
            source_control_filtered_history_indices(&commits, "5m", now),
            vec![0]
        );
        assert_eq!(
            source_control_filtered_history_indices(&commits, "scrollback", now),
            vec![0]
        );
    }

    #[test]
    fn stale_git_history_loaded_request_id_is_ignored() {
        let root = PathBuf::from("workspace");
        let existing = commit("aaaaaaaa", "Current history", 20);
        let stale = commit("bbbbbbbb", "Stale history", 30);
        let mut app = source_control_app_for_test(root.clone(), true);
        app.source_control_history_open = true;
        app.source_control_history_active_request_id = 2;
        app.source_control_history_loading = true;
        app.source_control_history_requested_limit = 8;
        app.source_control_history_has_more = true;
        app.source_control_history = vec![existing.clone()];
        app.status = "current history".to_owned();

        app.apply_git_history_loaded(1, 1, root.clone(), root, vec![stale]);

        assert_eq!(app.source_control_history, vec![existing]);
        assert!(app.source_control_history_loading);
        assert_eq!(app.source_control_history_requested_limit, 8);
        assert!(app.source_control_history_has_more);
        assert_eq!(app.status, "current history");
    }

    #[test]
    fn stale_git_history_failed_request_id_is_ignored() {
        let root = PathBuf::from("workspace");
        let existing = commit("aaaaaaaa", "Current history", 20);
        let mut app = source_control_app_for_test(root.clone(), true);
        app.source_control_history_open = true;
        app.source_control_history_active_request_id = 2;
        app.source_control_history_loading = true;
        app.source_control_history_requested_limit = 8;
        app.source_control_history_has_more = true;
        app.source_control_history = vec![existing.clone()];
        app.status = "current history".to_owned();

        app.apply_git_history_failed(1, 1, root.clone(), root, "stale failure".to_owned());

        assert_eq!(app.source_control_history, vec![existing]);
        assert!(app.source_control_history_loading);
        assert_eq!(app.source_control_history_requested_limit, 8);
        assert!(app.source_control_history_has_more);
        assert_eq!(app.status, "current history");
    }

    #[test]
    fn stale_git_history_loaded_after_operation_root_change_is_ignored() {
        let root = PathBuf::from("workspace");
        let stale_operation_root = root.join("old-repo");
        let existing = commit("aaaaaaaa", "Current history", 20);
        let stale = commit("bbbbbbbb", "Stale history", 30);
        let mut app = source_control_app_for_test(root.clone(), true);
        app.source_control_history_open = true;
        app.source_control_history_active_request_id = 1;
        app.source_control_history_loading = true;
        app.source_control_history_requested_limit = 8;
        app.source_control_history_has_more = true;
        app.source_control_history = vec![existing.clone()];
        app.source_control_history_selected = 0;
        app.status = "current history".to_owned();

        app.apply_git_history_loaded(1, 1, root, stale_operation_root, vec![stale]);

        assert_eq!(app.source_control_history, vec![existing]);
        assert_eq!(app.source_control_history_selected, 0);
        assert!(app.source_control_history_loading);
        assert_eq!(app.source_control_history_requested_limit, 8);
        assert!(app.source_control_history_has_more);
        assert_eq!(app.status, "current history");
    }

    #[test]
    fn stale_git_history_failed_after_operation_root_change_is_ignored() {
        let root = PathBuf::from("workspace");
        let stale_operation_root = root.join("old-repo");
        let existing = commit("aaaaaaaa", "Current history", 20);
        let mut app = source_control_app_for_test(root.clone(), true);
        app.source_control_history_open = true;
        app.source_control_history_active_request_id = 1;
        app.source_control_history_loading = true;
        app.source_control_history_requested_limit = 8;
        app.source_control_history_has_more = true;
        app.source_control_history = vec![existing.clone()];
        app.status = "current history".to_owned();

        app.apply_git_history_failed(1, 1, root, stale_operation_root, "stale failure".to_owned());

        assert_eq!(app.source_control_history, vec![existing]);
        assert!(app.source_control_history_loading);
        assert_eq!(app.source_control_history_requested_limit, 8);
        assert!(app.source_control_history_has_more);
        assert_eq!(app.status, "current history");
    }

    #[test]
    fn git_history_failure_status_sanitizes_and_bounds_error_detail() {
        let status = git_history_failure_status(&format!(
            "first line\nsecond line \u{202e}{}",
            "git-error-".repeat(DISPLAY_ERROR_LABEL_MAX_CHARS * 2)
        ));

        assert!(status.starts_with("Could not load git history: first line "));
        assert!(!status.contains('\n'));
        assert!(!status.contains('\u{202e}'));
        assert!(status.contains("..."));
        assert!(
            status.chars().count()
                <= "Could not load git history: ".chars().count() + DISPLAY_ERROR_LABEL_MAX_CHARS
        );
    }

    #[test]
    fn next_git_history_limit_caps_large_history_requests() {
        assert_eq!(next_git_history_limit(50, 50, 25), 75);
        assert_eq!(
            next_git_history_limit(
                SOURCE_CONTROL_HISTORY_MAX_REQUESTED_COMMITS - 5,
                50,
                usize::MAX
            ),
            SOURCE_CONTROL_HISTORY_MAX_REQUESTED_COMMITS
        );
        assert_eq!(
            next_git_history_limit(usize::MAX - 10, usize::MAX - 20, usize::MAX),
            SOURCE_CONTROL_HISTORY_MAX_REQUESTED_COMMITS
        );
    }

    #[test]
    fn history_has_more_stops_at_request_cap() {
        assert!(source_control_history_has_more(50, 50));
        assert!(!source_control_history_has_more(0, 0));
        assert!(!source_control_history_has_more(
            SOURCE_CONTROL_HISTORY_MAX_REQUESTED_COMMITS,
            SOURCE_CONTROL_HISTORY_MAX_REQUESTED_COMMITS
        ));
        assert!(!source_control_history_has_more(usize::MAX, usize::MAX));
    }
}
