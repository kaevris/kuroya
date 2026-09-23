use crate::{
    KuroyaApp,
    devtools_async_tasks::MAX_ASYNC_TASK_DETAIL_CHARS,
    path_display::display_path_label_cow,
    project_search_state::{
        MAX_PROJECT_SEARCH_RECENT_QUERIES, ProjectSearchKey, next_project_search_request_id,
        normalize_project_search_request_query, parse_project_globs,
        project_search_status_query_label, quoted_project_search_query_label,
        record_recent_project_search_from_parsed_globs,
    },
    ui_events::UiEvent,
    ui_state::{clamp_selection, move_selection},
};
use kuroya_core::{
    EditorSettings, SearchOptions, SearchResult, clamp_project_search_max_file_size_mb,
    clamp_project_search_max_results, merged_exclude_globs,
    search::search_project_with_metadata_cache_and_progress,
};
use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
    sync::{
        Condvar, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
};

impl KuroyaApp {
    pub(crate) fn spawn_project_search(&mut self) {
        let Some(query) = normalize_project_search_request_query(&self.project_search_query) else {
            self.invalidate_project_search_requests();
            self.project_search_submitted_key = None;
            self.project_search_result = SearchResult::default();
            self.project_search_result_query.clear();
            self.project_search_result_index_generation = self.project_search_index_generation;
            self.project_search_selected = 0;
            return;
        };

        if self.project_index_generation == 0 && self.index.files().is_empty() {
            self.ensure_workspace_index_started();
            if self.workspace_index_in_flight_request_id.is_some() {
                self.invalidate_project_search_requests();
                self.project_search_submitted_key = None;
                self.project_search_result = SearchResult::default();
                self.project_search_result_query.clear();
                self.project_search_result_index_generation = self.project_search_index_generation;
                self.project_search_selected = 0;
                self.status = "Indexing workspace before search".to_owned();
                return;
            }
        }

        let request_id = self.reserve_project_search_request_id();
        let index = self.index.clone();
        let index_generation = self.project_search_index_generation;
        let workspace_root = self.workspace.root.clone();
        let metadata_cache = self.project_search_metadata_cache.clone();
        let tx = self.tx.clone();
        let cancel_generation = self.project_search_cancel_generation.clone();
        let case_sensitive = self.project_search_case_sensitive;
        let whole_word = self.project_search_whole_word;
        let regex = self.project_search_regex;
        let include_globs = parse_project_globs(&self.project_search_include);
        let panel_exclude_globs = parse_project_globs(&self.project_search_exclude);
        let exclude_globs =
            effective_project_search_exclude_globs(&self.settings, &panel_exclude_globs);
        let max_file_bytes = effective_project_search_max_file_bytes(&self.settings);
        let max_results = effective_project_search_max_results(&self.settings);
        record_recent_project_search_from_parsed_globs(
            &mut self.project_search_recent,
            &query,
            case_sensitive,
            whole_word,
            regex,
            &include_globs,
            &panel_exclude_globs,
            MAX_PROJECT_SEARCH_RECENT_QUERIES,
        );
        self.status = project_search_start_status(&query);
        self.project_search_submitted_key = Some(ProjectSearchKey {
            root: workspace_root.clone(),
            query: query.clone(),
            case_sensitive,
            whole_word,
            regex,
            include_globs: include_globs.clone(),
            exclude_globs: exclude_globs.clone(),
            max_file_bytes,
            max_results,
        });
        self.project_search_result = SearchResult::default();
        self.project_search_result_query = query.clone();
        self.project_search_result_index_generation = index_generation;
        self.project_search_result_case_sensitive = case_sensitive;
        self.project_search_result_whole_word = whole_word;
        self.project_search_result_regex = regex;
        self.project_search_result_include_globs = include_globs.clone();
        self.project_search_result_exclude_globs = exclude_globs.clone();
        self.project_search_result_max_file_bytes = max_file_bytes;
        self.project_search_result_max_results = max_results;
        self.project_search_selected = 0;
        self.record_async_task_started("Project Search", project_search_start_detail(&query));
        self.runtime.spawn_blocking(move || {
            let options = SearchOptions {
                query,
                max_file_bytes,
                max_results,
                case_sensitive,
                whole_word,
                regex,
                include_globs,
                exclude_globs,
            };
            let result = run_project_search_one_at_a_time(
                &cancel_generation,
                request_id,
                || {
                    search_project_with_metadata_cache_and_progress(
                        &index,
                        &options,
                        &metadata_cache,
                        || project_search_request_is_cancelled(&cancel_generation, request_id),
                        |progress| {
                            let _ = crate::ui_event_channel::send_ui_event(
                                &tx,
                                UiEvent::SearchProgress {
                                    request_id,
                                    index_generation,
                                    workspace_root: workspace_root.clone(),
                                    query: options.query.clone(),
                                    case_sensitive,
                                    whole_word,
                                    include_globs: options.include_globs.clone(),
                                    exclude_globs: options.exclude_globs.clone(),
                                    max_file_bytes: options.max_file_bytes,
                                    max_results: options.max_results,
                                    progress,
                                },
                            );
                        },
                    )
                    .unwrap_or_default()
                },
                SearchResult::default,
            );
            let SearchOptions {
                query,
                include_globs,
                exclude_globs,
                max_file_bytes,
                max_results,
                ..
            } = options;
            let _ = crate::ui_event_channel::send_critical_ui_event(
                &tx,
                UiEvent::SearchFinished {
                    request_id,
                    index_generation,
                    workspace_root,
                    query,
                    case_sensitive,
                    whole_word,
                    include_globs,
                    exclude_globs,
                    max_file_bytes,
                    max_results,
                    result,
                },
            );
        });
    }

    pub(crate) fn project_search_should_refresh_after_index(&self) -> bool {
        self.project_search
            && self.project_index_generation > 0
            && self.workspace_index_in_flight_request_id.is_none()
            && normalize_project_search_request_query(&self.project_search_query).is_some()
    }

    fn reserve_project_search_request_id(&mut self) -> u64 {
        self.project_search_next_request_id =
            next_project_search_request_id(self.project_search_next_request_id);
        self.project_search_active_request_id = self.project_search_next_request_id;
        self.project_search_cancel_generation
            .store(self.project_search_active_request_id, Ordering::Relaxed);
        self.project_search_active_request_id
    }

    pub(crate) fn invalidate_project_search_requests(&mut self) {
        self.reserve_project_search_request_id();
    }

    fn current_project_search_key(&self) -> ProjectSearchKey {
        let panel_exclude_globs = parse_project_globs(&self.project_search_exclude);
        ProjectSearchKey {
            root: self.workspace.root.clone(),
            query: normalize_project_search_request_query(&self.project_search_query)
                .unwrap_or_default(),
            case_sensitive: self.project_search_case_sensitive,
            whole_word: self.project_search_whole_word,
            regex: self.project_search_regex,
            include_globs: parse_project_globs(&self.project_search_include),
            exclude_globs: effective_project_search_exclude_globs(
                &self.settings,
                &panel_exclude_globs,
            ),
            max_file_bytes: effective_project_search_max_file_bytes(&self.settings),
            max_results: effective_project_search_max_results(&self.settings),
        }
    }

    pub(crate) fn project_search_key_matches_submitted(&self, key: &ProjectSearchKey) -> bool {
        self.project_search_submitted_key.as_ref() == Some(key)
    }

    pub(crate) fn sync_project_search_after_settings_change(&mut self) {
        if !self.project_search {
            return;
        }
        let current = self.current_project_search_key();
        if self.project_search_key_matches_submitted(&current) {
            return;
        }
        let Some(submitted) = self.project_search_submitted_key.clone() else {
            return;
        };
        let panel_include_globs = parse_project_globs(&self.project_search_include);
        let panel_exclude_globs = parse_project_globs(&self.project_search_exclude);
        let exclude_panel_start = submitted
            .exclude_globs
            .len()
            .saturating_sub(panel_exclude_globs.len());
        let panel_inputs_match = current.root == submitted.root
            && current.query == submitted.query
            && current.case_sensitive == submitted.case_sensitive
            && current.whole_word == submitted.whole_word
            && current.regex == submitted.regex
            && panel_include_globs == submitted.include_globs
            && submitted.exclude_globs.get(exclude_panel_start..) == Some(&panel_exclude_globs[..]);
        if !panel_inputs_match {
            return;
        }
        if current.max_file_bytes == submitted.max_file_bytes
            && current.max_results == submitted.max_results
            && current.exclude_globs == submitted.exclude_globs
        {
            return;
        }
        self.spawn_project_search();
    }

    pub(crate) fn project_search_results_match_current_query(&self) -> bool {
        self.project_search_result_index_generation == self.project_search_index_generation
            && self.project_search_results_match_current_inputs()
    }

    pub(crate) fn project_search_results_match_current_inputs(&self) -> bool {
        self.project_search_result_key() == self.current_project_search_key()
    }

    fn project_search_result_key(&self) -> ProjectSearchKey {
        ProjectSearchKey {
            root: self.workspace.root.clone(),
            query: self.project_search_result_query.clone(),
            case_sensitive: self.project_search_result_case_sensitive,
            whole_word: self.project_search_result_whole_word,
            regex: self.project_search_result_regex,
            include_globs: self.project_search_result_include_globs.clone(),
            exclude_globs: self.project_search_result_exclude_globs.clone(),
            max_file_bytes: self.project_search_result_max_file_bytes,
            max_results: self.project_search_result_max_results,
        }
    }

    pub(crate) fn goto_project_search_result(&mut self, direction: isize) {
        if !self.project_search_results_match_current_inputs() {
            self.status =
                if normalize_project_search_request_query(&self.project_search_query).is_none() {
                    "No project search query".to_owned()
                } else {
                    "Run project search to refresh results".to_owned()
                };
            return;
        }

        let len = self.project_search_result.matches.len();
        if len == 0 {
            self.status = "No project search matches".to_owned();
            return;
        }

        move_project_search_selection(&mut self.project_search_selected, len, direction);
        let Some(jump) =
            project_search_result_jump(&self.project_search_result, self.project_search_selected)
        else {
            return;
        };

        let status = project_search_match_status(
            self.project_search_selected + 1,
            len,
            &jump.path,
            jump.line,
            jump.column,
        );
        let selection_length = self.project_search_result_query.chars().count();
        self.open_file_selection_at_known_openable(
            jump.path,
            jump.line,
            jump.column,
            selection_length,
        );
        self.status = status;
    }
}

pub(crate) fn effective_project_search_exclude_globs(
    settings: &EditorSettings,
    panel_exclude_globs: &[String],
) -> Vec<String> {
    let mut user_globs = settings.project_search_exclude_globs.clone();
    user_globs.extend(panel_exclude_globs.iter().cloned());
    merged_exclude_globs(&user_globs)
}

pub(crate) fn effective_project_search_max_file_bytes(settings: &EditorSettings) -> u64 {
    clamp_project_search_max_file_size_mb(settings.project_search_max_file_size_mb)
        .saturating_mul(1024 * 1024)
}

pub(crate) fn effective_project_search_max_results(settings: &EditorSettings) -> usize {
    clamp_project_search_max_results(settings.project_search_max_results)
}

fn move_project_search_selection(selection: &mut usize, len: usize, direction: isize) {
    clamp_selection(selection, len);
    move_selection(selection, len, direction);
}

#[derive(Debug, PartialEq, Eq)]
struct ProjectSearchResultJump {
    path: PathBuf,
    line: usize,
    column: usize,
}

fn project_search_result_jump(
    result: &SearchResult,
    selected_index: usize,
) -> Option<ProjectSearchResultJump> {
    let result_match = result.matches.get(selected_index)?;
    Some(ProjectSearchResultJump {
        path: result_match.path.clone(),
        line: result_match.line,
        column: result_match.column,
    })
}

fn project_search_start_status(query: &str) -> String {
    let label = project_search_status_query_label(query);
    let mut status = String::with_capacity("Searching for `".len() + label.len() + 1);
    status.push_str("Searching for `");
    status.push_str(&label);
    status.push('`');
    status
}

fn project_search_start_detail(query: &str) -> String {
    quoted_project_search_query_label(query, MAX_ASYNC_TASK_DETAIL_CHARS)
}

fn project_search_request_is_cancelled(cancel_generation: &AtomicU64, request_id: u64) -> bool {
    cancel_generation.load(Ordering::Relaxed) != request_id
}

fn run_project_search_one_at_a_time<T>(
    cancel_generation: &AtomicU64,
    request_id: u64,
    run_search: impl FnOnce() -> T,
    cancelled: impl FnOnce() -> T,
) -> T {
    if project_search_job_admission(cancel_generation, request_id) {
        let result = run_search();
        release_project_search_job_slot(request_id);
        result
    } else {
        cancelled()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectSearchJobStep {
    Active,
    Pending,
}

struct ProjectSearchJobSlots {
    active: Option<u64>,
    pending: Option<u64>,
}

impl ProjectSearchJobSlots {
    const fn empty() -> Self {
        Self {
            active: None,
            pending: None,
        }
    }

    fn step(&mut self, request_id: u64) -> ProjectSearchJobStep {
        match self.active {
            Some(active) if active == request_id => ProjectSearchJobStep::Active,
            Some(_) => {
                self.pending = Some(request_id);
                ProjectSearchJobStep::Pending
            }
            None => {
                if self.pending == Some(request_id) {
                    self.pending = None;
                }
                self.active = Some(request_id);
                ProjectSearchJobStep::Active
            }
        }
    }

    fn release(&mut self, request_id: u64) {
        if self.active == Some(request_id) {
            self.active = None;
        }
        if self.pending == Some(request_id) {
            self.pending = None;
        }
    }
}

fn project_search_job_slots() -> MutexGuard<'static, ProjectSearchJobSlots> {
    static PROJECT_SEARCH_JOB_SLOTS: Mutex<ProjectSearchJobSlots> =
        Mutex::new(ProjectSearchJobSlots::empty());
    PROJECT_SEARCH_JOB_SLOTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn project_search_job_slots_changed() -> &'static Condvar {
    static PROJECT_SEARCH_JOB_SLOT_CHANGED: Condvar = Condvar::new();
    &PROJECT_SEARCH_JOB_SLOT_CHANGED
}

fn project_search_job_admission(cancel_generation: &AtomicU64, request_id: u64) -> bool {
    let mut jobs = project_search_job_slots();
    loop {
        if project_search_request_is_cancelled(cancel_generation, request_id) {
            jobs.release(request_id);
            return false;
        }
        match jobs.step(request_id) {
            ProjectSearchJobStep::Active => return true,
            ProjectSearchJobStep::Pending => {
                project_search_job_slots_changed().notify_all();
                jobs = project_search_job_slots_changed()
                    .wait(jobs)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
        }
    }
}

fn release_project_search_job_slot(request_id: u64) {
    {
        let mut jobs = project_search_job_slots();
        jobs.release(request_id);
    }
    project_search_job_slots_changed().notify_all();
}

fn project_search_match_status(
    selected_index: usize,
    result_count: usize,
    path: &Path,
    line: usize,
    column: usize,
) -> String {
    let path = display_path_label_cow(path);
    let mut status = String::with_capacity(path.len().saturating_add(48));
    let _ = write!(
        status,
        "Project match {selected_index}/{result_count} at {}:{line}:{column}",
        path.as_ref()
    );
    status
}

#[cfg(test)]
mod tests {
    use super::{
        ProjectSearchJobSlots, ProjectSearchJobStep, ProjectSearchResultJump,
        effective_project_search_exclude_globs, move_project_search_selection,
        project_search_job_admission, project_search_match_status,
        project_search_request_is_cancelled, project_search_result_jump,
        project_search_start_status, release_project_search_job_slot,
        run_project_search_one_at_a_time,
    };
    use crate::{
        KuroyaApp,
        app_startup_context::AppStartupContext,
        devtools_async_tasks::{MAX_ASYNC_TASK_DETAIL_CHARS, async_task_event_label},
        path_display::DISPLAY_PATH_LABEL_MAX_CHARS,
        project_search_state::{
            MAX_PROJECT_SEARCH_QUERY_CHARS, MAX_PROJECT_SEARCH_STATUS_QUERY_CHARS,
            normalize_project_search_request_query,
        },
        terminal::TerminalPane,
        ui_event_channel::ui_event_channel,
        ui_events::UiEvent,
    };
    use kuroya_core::{
        EditorSettings, ProjectIndex, SearchMatch, SearchResult, TextBuffer, Workspace,
        merged_exclude_globs,
    };
    use std::{
        cell::Cell,
        fs,
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };
    use tokio::runtime::Runtime;

    fn recv_project_search_finished(app: &KuroyaApp) -> UiEvent {
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let now = Instant::now();
            assert!(
                now < deadline,
                "timed out waiting for project search completion"
            );
            let event = app
                .rx
                .recv_timeout(deadline.saturating_duration_since(now))
                .expect("project search event");
            if matches!(event, UiEvent::SearchFinished { .. }) {
                return event;
            }
        }
    }

    #[test]
    fn project_search_start_status_sanitizes_and_bounds_query_label() {
        let query = format!(
            "alpha\nbeta\u{202e}{}",
            "query-fragment-".repeat(MAX_PROJECT_SEARCH_STATUS_QUERY_CHARS)
        );

        let status = project_search_start_status(&query);

        assert!(status.starts_with("Searching for `alpha beta"));
        assert!(!status.contains('\n'));
        assert!(!status.contains('\u{202e}'));
        assert!(status.contains("..."));
        assert!(
            status.chars().count()
                <= "Searching for ``".chars().count() + MAX_PROJECT_SEARCH_STATUS_QUERY_CHARS
        );
    }

    #[test]
    fn project_search_match_status_sanitizes_and_bounds_path_label() {
        let path = PathBuf::from(format!(
            "workspace/src/bad\n{}\u{202e}tail.rs",
            "path-fragment-".repeat(DISPLAY_PATH_LABEL_MAX_CHARS)
        ));

        let status = project_search_match_status(1, 2, &path, 3, 5);

        assert!(status.starts_with("Project match 1/2 at "));
        assert!(!status.contains('\n'));
        assert!(!status.contains('\u{202e}'));
        assert!(status.contains("..."));
        assert!(
            status.chars().count()
                <= "Project match 1/2 at :3:5".chars().count() + DISPLAY_PATH_LABEL_MAX_CHARS
        );
    }

    #[test]
    fn project_search_result_jump_captures_only_open_target_fields() {
        let path = PathBuf::from("workspace/src/main.rs");
        let result = SearchResult {
            matches: vec![SearchMatch {
                path: path.clone(),
                line: 3,
                column: 5,
                preview: "needle preview that is not needed for opening".repeat(32),
            }],
            ..SearchResult::default()
        };

        assert_eq!(
            project_search_result_jump(&result, 0),
            Some(ProjectSearchResultJump {
                path,
                line: 3,
                column: 5,
            })
        );
        assert_eq!(project_search_result_jump(&result, 1), None);
    }

    #[test]
    fn project_search_selection_clamps_before_keyboard_wrap() {
        let mut selected = usize::MAX;

        move_project_search_selection(&mut selected, 3, 1);

        assert_eq!(selected, 0);

        selected = usize::MAX;
        move_project_search_selection(&mut selected, 3, -1);

        assert_eq!(selected, 1);
    }

    #[test]
    fn spawn_project_search_sanitizes_visible_labels_and_preserves_raw_request_query() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_project_search_test(root);
        app.project_index_generation = 1;
        app.project_search_index_generation = 1;
        let raw_query = format!(
            "  alpha\n  beta\u{202e} {}  ",
            "query-fragment-".repeat(MAX_PROJECT_SEARCH_QUERY_CHARS)
        );
        let request_query =
            normalize_project_search_request_query(&raw_query).expect("request query");
        app.project_search_query = raw_query.clone();

        app.spawn_project_search();

        assert_eq!(app.project_search_query, raw_query);
        assert!(!app.status.contains('\n'));
        assert!(!app.status.contains('\u{202e}'));
        assert!(app.status.contains("..."));

        let active_detail = app
            .active_async_tasks
            .front()
            .expect("project search active task")
            .detail
            .clone();
        assert!(active_detail.starts_with('`'));
        assert!(active_detail.ends_with('`'));
        assert!(!active_detail.contains('\n'));
        assert!(!active_detail.contains('\u{202e}'));
        assert!(active_detail.contains("..."));
        assert!(active_detail.chars().count() <= MAX_ASYNC_TASK_DETAIL_CHARS);

        let event = recv_project_search_finished(&app);
        let label = async_task_event_label(&event).expect("search event label");
        assert_eq!(label.detail, active_detail);
        match event {
            UiEvent::SearchFinished { query, .. } => {
                assert_eq!(query, request_query);
                assert!(!query.contains('\n'));
                assert!(!query.contains('\u{202e}'));
                assert!(query.chars().count() <= MAX_PROJECT_SEARCH_QUERY_CHARS);
            }
            _ => panic!("expected project search completion event"),
        }
    }

    #[test]
    fn spawn_project_search_uses_metadata_project_index() {
        let root = temp_project_search_root("metadata-project-index-search");
        fs::create_dir_all(root.join("src")).unwrap();
        let path = root.join("src/main.rs");
        fs::write(&path, "fn main() {\n    let needle = 1;\n}\n").unwrap();
        let mut app = app_for_project_search_test(root.clone());
        app.index = ProjectIndex::rebuild(&root, 40_000);
        app.project_search_index_generation = 3;
        app.project_search_query = "needle".to_owned();

        app.spawn_project_search();

        let deadline = Instant::now() + Duration::from_secs(1);
        let mut saw_progress = false;
        loop {
            let now = Instant::now();
            assert!(
                now < deadline,
                "timed out waiting for project search completion"
            );
            let event = app
                .rx
                .recv_timeout(deadline.saturating_duration_since(now))
                .expect("project search event");
            match event {
                UiEvent::SearchProgress {
                    index_generation,
                    query,
                    progress,
                    ..
                } => {
                    assert_eq!(index_generation, 3);
                    assert_eq!(query, "needle");
                    assert_eq!(progress.matches.len(), 1);
                    assert_eq!(progress.matches[0].path, path);
                    saw_progress = true;
                }
                UiEvent::SearchFinished {
                    index_generation,
                    query,
                    result,
                    ..
                } => {
                    assert!(saw_progress);
                    assert_eq!(index_generation, 3);
                    assert_eq!(query, "needle");
                    assert_eq!(result.matches.len(), 1);
                    assert_eq!(result.matches[0].path, path);
                    assert_eq!(result.matches[0].line, 2);
                    break;
                }
                _ => panic!("expected project search event"),
            }
        }
        assert!(app.rx.try_recv().is_err());
        assert_eq!(app.project_search_metadata_cache.len(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn spawn_project_search_uses_search_limit_and_exclude_settings() {
        let root = temp_project_search_root("settings-project-index-search");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("ignored")).unwrap();
        let large_path = root.join("src/large.rs");
        fs::write(&large_path, format!("needle{}\n", "x".repeat(1024 * 1024))).unwrap();
        fs::write(root.join("src/one.rs"), "needle one\n").unwrap();
        fs::write(root.join("src/two.rs"), "needle two\n").unwrap();
        fs::write(root.join("ignored/skip.rs"), "needle ignored\n").unwrap();

        let mut app = app_for_project_search_test(root.clone());
        app.settings.project_search_exclude_globs = vec!["ignored".to_owned()];
        app.settings.project_search_max_file_size_mb = 1;
        app.settings.project_search_max_results = 1;
        app.index = ProjectIndex::rebuild(&root, 40_000);
        app.project_search_index_generation = 7;
        app.project_search_query = "needle".to_owned();

        app.spawn_project_search();

        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let now = Instant::now();
            assert!(
                now < deadline,
                "timed out waiting for project search completion"
            );
            let event = app
                .rx
                .recv_timeout(deadline.saturating_duration_since(now))
                .expect("project search event");
            if let UiEvent::SearchFinished {
                query,
                exclude_globs,
                max_file_bytes,
                max_results,
                result,
                ..
            } = event
            {
                assert_eq!(query, "needle");
                assert_eq!(exclude_globs, merged_exclude_globs(&["ignored".to_owned()]));
                assert_eq!(max_file_bytes, 1024 * 1024);
                assert_eq!(max_results, 1);
                assert_eq!(result.matches.len(), 1);
                assert!(result.truncated);
                assert_eq!(result.stats.skipped_large_files, 1);
                assert!(
                    result
                        .matches
                        .iter()
                        .all(|matched| !matched.path.starts_with(root.join("ignored")))
                );
                break;
            }
        }

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn spawn_project_search_defers_until_workspace_index_is_available() {
        let root = temp_project_search_root("deferred-project-index-search");
        fs::create_dir_all(&root).unwrap();
        let mut app = app_for_project_search_test(root.clone());
        app.project_search = true;
        app.project_search_query = "needle".to_owned();

        app.spawn_project_search();

        assert_eq!(app.workspace_index_in_flight_request_id, Some(1));
        assert_eq!(app.project_search_active_request_id, 1);
        assert!(app.project_search_result.matches.is_empty());
        assert!(app.project_search_result_query.is_empty());
        assert_eq!(app.status, "Indexing workspace before search");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_search_request_reservation_updates_cancellation_generation() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_project_search_test(root);

        let first_request = app.reserve_project_search_request_id();
        assert_eq!(first_request, 1);
        assert_eq!(app.project_search_active_request_id, 1);
        assert_eq!(
            app.project_search_cancel_generation.load(Ordering::Relaxed),
            1
        );

        app.invalidate_project_search_requests();
        assert_eq!(app.project_search_active_request_id, 2);
        assert_eq!(
            app.project_search_cancel_generation.load(Ordering::Relaxed),
            2
        );
    }

    #[test]
    fn project_search_request_cancellation_tracks_generation_mismatches() {
        let cancel_generation = std::sync::atomic::AtomicU64::new(7);

        assert!(!project_search_request_is_cancelled(&cancel_generation, 7));

        cancel_generation.store(8, Ordering::Relaxed);

        assert!(project_search_request_is_cancelled(&cancel_generation, 7));
    }

    #[test]
    fn project_search_job_slots_keep_only_latest_pending_and_promote_once() {
        let mut slots = ProjectSearchJobSlots::empty();

        assert_eq!(slots.step(1), ProjectSearchJobStep::Active);
        assert_eq!(slots.active, Some(1));
        assert_eq!(slots.pending, None);
        assert_eq!(slots.step(2), ProjectSearchJobStep::Pending);
        assert_eq!(slots.pending, Some(2));
        assert_eq!(slots.step(3), ProjectSearchJobStep::Pending);
        assert_eq!(slots.pending, Some(3));

        slots.release(1);
        assert_eq!(slots.active, None);
        assert_eq!(slots.pending, Some(3));
        assert_eq!(slots.step(3), ProjectSearchJobStep::Active);
        assert_eq!(slots.active, Some(3));
        assert_eq!(slots.pending, None);

        slots.release(3);
        slots.release(2);
        assert_eq!(slots.active, None);
        assert_eq!(slots.pending, None);
    }

    #[test]
    fn project_search_coordinator_runs_only_the_latest_queued_request() {
        let cancel_generation = Arc::new(std::sync::atomic::AtomicU64::new(1));
        let first_started = Arc::new(AtomicBool::new(false));
        let (first_release_tx, first_release_rx) = mpsc::channel::<()>();
        let (first_ran_tx, first_ran_rx) = mpsc::channel();

        let started_in_thread = first_started.clone();
        let cancel_generation_in_thread = cancel_generation.clone();
        let first_worker = std::thread::spawn(move || {
            run_project_search_one_at_a_time(
                &cancel_generation_in_thread,
                1,
                || {
                    started_in_thread.store(true, Ordering::SeqCst);
                    first_release_rx.recv().unwrap();
                    first_ran_tx.send(()).unwrap();
                },
                || (),
            );
        });

        let deadline = Instant::now() + Duration::from_secs(1);
        while !first_started.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(first_started.load(Ordering::SeqCst));

        cancel_generation.store(3, Ordering::Relaxed);
        let (intermediate_tx, intermediate_rx) = mpsc::channel();
        let cancel_generation_in_thread = cancel_generation.clone();
        let intermediate_worker = std::thread::spawn(move || {
            run_project_search_one_at_a_time(
                &cancel_generation_in_thread,
                2,
                || intermediate_tx.send("ran").unwrap(),
                || intermediate_tx.send("cancelled").unwrap(),
            );
        });
        let (latest_tx, latest_rx) = mpsc::channel();
        let cancel_generation_in_thread = cancel_generation.clone();
        let latest_worker = std::thread::spawn(move || {
            run_project_search_one_at_a_time(
                &cancel_generation_in_thread,
                3,
                || latest_tx.send(()).unwrap(),
                || (),
            );
        });

        first_release_tx.send(()).unwrap();
        first_worker.join().unwrap();
        first_ran_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("active search should finish exactly once");

        latest_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("latest queued request should run after active completes");
        latest_worker.join().unwrap();

        assert_eq!(
            intermediate_rx.recv_timeout(Duration::from_secs(1)),
            Ok("cancelled")
        );
        intermediate_worker.join().unwrap();
    }

    #[test]
    fn project_search_admission_claims_active_slot_and_fast_fails_stale_requests() {
        let cancel_generation = std::sync::atomic::AtomicU64::new(6);

        assert!(!project_search_job_admission(&cancel_generation, 5));
        assert!(project_search_job_admission(&cancel_generation, 6));

        cancel_generation.store(7, Ordering::Relaxed);
        assert!(!project_search_job_admission(&cancel_generation, 6));
        release_project_search_job_slot(6);
    }

    #[test]
    fn project_search_guard_skips_cancelled_request_before_running_search() {
        let cancel_generation = std::sync::atomic::AtomicU64::new(2);
        let ran_search = Cell::new(false);

        let result = run_project_search_one_at_a_time(
            &cancel_generation,
            1,
            || {
                ran_search.set(true);
                "searched"
            },
            || "cancelled",
        );

        assert_eq!(result, "cancelled");
        assert!(!ran_search.get());
    }

    #[test]
    fn project_search_results_match_current_query_checks_options_before_globs() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_project_search_test(root);
        app.project_search_query = " needle ".to_owned();
        app.project_search_result_query = "needle".to_owned();
        app.project_search_result_index_generation = app.project_search_index_generation;
        app.project_search_result_include_globs = vec!["src/**/*.rs".to_owned()];
        app.project_search_result_exclude_globs =
            effective_project_search_exclude_globs(&app.settings, &["target/**".to_owned()]);
        app.project_search_include = " src/**/*.rs, src/**/*.rs ".to_owned();
        app.project_search_exclude = "target/**".to_owned();

        assert!(app.project_search_results_match_current_query());

        app.project_search_case_sensitive = true;

        assert!(!app.project_search_results_match_current_query());
    }

    #[test]
    fn project_search_results_match_current_query_normalizes_current_text() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_project_search_test(root);
        app.project_search_query = " needle\n  value ".to_owned();
        app.project_search_result_query = "needle value".to_owned();
        app.project_search_result_index_generation = app.project_search_index_generation;

        assert!(app.project_search_results_match_current_query());

        app.project_search_result_query = "needle  value".to_owned();

        assert!(!app.project_search_results_match_current_query());
    }

    #[test]
    fn generation_only_staleness_keeps_matching_results_openable() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_project_search_test(root);
        app.project_search_query = "needle".to_owned();
        app.project_search_result_query = "needle".to_owned();
        app.project_search_index_generation = 2;
        app.project_search_result_index_generation = 1;

        assert!(!app.project_search_results_match_current_query());
        assert!(app.project_search_results_match_current_inputs());
    }

    #[test]
    fn project_search_result_jump_records_history_without_filesystem_precheck() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_project_search_test(root.clone());
        let origin_path = root.join("src").join("origin.rs");
        app.buffers.push(TextBuffer::from_text(
            1,
            Some(origin_path.clone()),
            "origin\n".to_owned(),
        ));
        app.set_active_buffer(1);
        app.project_search_query = "needle".to_owned();
        app.project_search_result_query = "needle".to_owned();
        app.project_search_result = SearchResult {
            matches: vec![SearchMatch {
                path: root.join("src").join("indexed-result.rs"),
                line: 3,
                column: 5,
                preview: "needle".to_owned(),
            }],
            ..SearchResult::default()
        };

        app.goto_project_search_result(1);

        assert_eq!(
            app.navigation_back.back().map(|location| &location.path),
            Some(&origin_path)
        );
        assert!(
            app.pending_open_paths
                .contains(&root.join("src").join("indexed-result.rs"))
        );
        assert_eq!(
            app.pending_file_jump
                .as_ref()
                .and_then(|jump| jump.selection_length),
            Some("needle".chars().count())
        );
    }

    #[test]
    fn project_search_result_jump_reuses_lexically_equivalent_open_buffer() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_project_search_test(root.clone());
        let origin_path = root.join("src").join("origin.rs");
        let open_path = root.join("src").join("main.rs");
        let search_path = root.join("src").join("..").join("src").join("main.rs");

        app.buffers.push(TextBuffer::from_text(
            1,
            Some(origin_path.clone()),
            "origin\n".to_owned(),
        ));
        app.buffers.push(TextBuffer::from_text(
            2,
            Some(open_path.clone()),
            "first\nsecond needle\nthird\n".to_owned(),
        ));
        app.set_active_buffer(1);
        app.project_search_query = "needle".to_owned();
        app.project_search_result_query = "needle".to_owned();
        app.project_search_result = SearchResult {
            matches: vec![SearchMatch {
                path: search_path,
                line: 2,
                column: 8,
                preview: "second needle".to_owned(),
            }],
            ..SearchResult::default()
        };

        app.goto_project_search_result(1);

        assert_eq!(app.active, Some(2));
        assert!(app.pending_open_paths.is_empty());
        assert!(app.pending_file_jump.is_none());
        assert_eq!(app.buffer(2).and_then(TextBuffer::path), Some(&open_path));
        let buffer = app.buffer(2).unwrap();
        let selection_start = buffer.line_column_to_char(1, 7);
        assert_eq!(
            buffer.selections()[0].range(),
            selection_start..selection_start + "needle".chars().count()
        );
        assert_eq!(
            app.navigation_back.back().map(|location| &location.path),
            Some(&origin_path)
        );
        assert_eq!(app.status, "Project match 1/1 at main.rs:2:8");
    }

    #[test]
    fn project_search_results_match_current_query_checks_regex_toggle() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_project_search_test(root);
        app.project_search_query = "needle".to_owned();
        app.project_search_result_query = "needle".to_owned();
        app.project_search_result_index_generation = app.project_search_index_generation;

        assert!(app.project_search_results_match_current_query());

        app.project_search_regex = true;

        assert!(!app.project_search_results_match_current_query());

        app.project_search_result_regex = true;

        assert!(app.project_search_results_match_current_query());
    }

    #[test]
    fn spawn_project_search_runs_regex_query() {
        let root = temp_project_search_root("regex-project-search");
        fs::create_dir_all(root.join("src")).unwrap();
        let path = root.join("src/main.rs");
        fs::write(&path, "fn needle() {}\nfn nadle() {}\n").unwrap();
        let mut app = app_for_project_search_test(root.clone());
        app.index = ProjectIndex::rebuild(&root, 40_000);
        app.project_search_index_generation = 5;
        app.project_search_query = "n(ee|a)dle".to_owned();
        app.project_search_regex = true;

        app.spawn_project_search();

        let event = recv_project_search_finished(&app);
        match event {
            UiEvent::SearchFinished { result, .. } => {
                assert!(result.error.is_none());
                assert_eq!(result.matches.len(), 2);
                assert_eq!(result.matches[0].path, path);
                assert_eq!(result.matches[0].line, 1);
                assert_eq!(result.matches[1].line, 2);
            }
            _ => panic!("expected project search completion event"),
        }

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn spawn_project_search_reports_invalid_regex_as_error() {
        let root = temp_project_search_root("invalid-regex-project-search");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "needle\n").unwrap();
        let mut app = app_for_project_search_test(root.clone());
        app.index = ProjectIndex::rebuild(&root, 40_000);
        app.project_search_index_generation = 5;
        app.project_search_query = "n(eedle".to_owned();
        app.project_search_regex = true;

        app.spawn_project_search();

        let event = recv_project_search_finished(&app);
        match event {
            UiEvent::SearchFinished { result, .. } => {
                assert!(result.matches.is_empty());
                let error = result.error.expect("invalid regex should fail");
                assert!(error.contains("Invalid regular expression"));
            }
            _ => panic!("expected project search completion event"),
        }

        fs::remove_dir_all(root).unwrap();
    }

    fn app_for_project_search_test(root: PathBuf) -> KuroyaApp {
        let (tx, rx) = ui_event_channel();
        let mut settings = EditorSettings::default();
        settings.project_search_exclude_globs.clear();
        let mut app = KuroyaApp::from_startup_context(AppStartupContext {
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
        });
        app.project_search_result_exclude_globs =
            effective_project_search_exclude_globs(&app.settings, &[]);
        app
    }

    fn temp_project_search_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("kuroya-{name}-{}-{nanos}", std::process::id()))
    }
}
