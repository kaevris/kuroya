use crate::{
    KuroyaApp,
    devtools_async_tasks::git_scan_task_detail,
    path_display::compact_path,
    project_index_cache::{
        load_project_index_cache_unverified_with_options, save_project_index_cache,
    },
    syntax::PluginSyntaxLoad,
    theme::selected_theme_index_with_plugins,
    ui_events::UiEvent,
    workspace_state::app_owned_state_dirs_in_workspace,
    workspace_trust::{trusted_workspace_paths_match, workspace_path_contains_lexically},
};
use kuroya_core::{
    GitAutoRepositoryDetection, GitOpenRepositoryInParentFolders, GitSnapshot, PluginDescriptor,
    ProjectIndex, ProjectIndexOptions, clamp_project_index_max_files, discover_workspace_plugins,
    merged_exclude_globs,
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(crate) const WORKSPACE_PLUGIN_RELOAD_DEBOUNCE: Duration = Duration::from_millis(250);
pub(crate) const WORKSPACE_PLUGIN_RELOAD_MAX_WAIT: Duration = Duration::from_secs(2);
pub(crate) const WORKSPACE_REFRESH_DEBOUNCE: Duration = Duration::from_millis(250);
pub(crate) const WORKSPACE_REFRESH_MAX_WAIT: Duration = Duration::from_secs(2);
/// Watcher batches with at most this many changed project paths are applied
/// incrementally to the warm in-memory index; larger batches fall back to a
/// full `spawn_index` re-walk because applying many individual updates costs
/// more than one walk.
/// Watcher batches up to this many paths are patched into the warm index
/// incrementally (in chunks of [`WORKSPACE_INCREMENTAL_CHUNK_PATHS`]) instead
/// of triggering a full re-walk, so bulk operations like branch switches keep
/// indexing cheap.
pub(crate) const WORKSPACE_INCREMENTAL_INDEX_MAX_PATHS: usize = 512;
/// Paths applied per `apply_path_changes` call. One call per chunk keeps each
/// incremental step bounded; the index Arc is already unique after
/// `clone_for_update`, so chunking costs no extra deep copies.
pub(crate) const WORKSPACE_INCREMENTAL_CHUNK_PATHS: usize = 64;
/// Watcher/operation batches with at most this many affected paths refresh
/// git through a pathspec-scoped status query instead of a full scan; larger
/// batches fall back to `spawn_git_scan`.
pub(crate) const GIT_SCOPED_REFRESH_MAX_PATHS: usize = 64;
const GIT_REPOSITORY_SCAN_MAX_CHILDREN_PER_FOLDER: usize = 2_048;
const GIT_REPOSITORY_SCAN_MAX_VISITED_FOLDERS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingWorkspaceRefresh {
    first_seen: Instant,
    last_seen: Instant,
    project_paths: Vec<PathBuf>,
}

impl PendingWorkspaceRefresh {
    fn new(now: Instant) -> Self {
        Self {
            first_seen: now,
            last_seen: now,
            project_paths: Vec::new(),
        }
    }

    fn record_change(&mut self, now: Instant) {
        self.last_seen = now;
    }

    fn record_project_paths(&mut self, paths: Vec<PathBuf>) {
        self.project_paths.extend(paths);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingWorkspacePluginReload {
    first_seen: Instant,
    last_seen: Instant,
}

impl PendingWorkspacePluginReload {
    fn new(now: Instant) -> Self {
        Self {
            first_seen: now,
            last_seen: now,
        }
    }

    fn record_change(&mut self, now: Instant) {
        self.last_seen = now;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GitScanRootCacheEntry {
    key: GitScanRootCacheKey,
    scan_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GitScanRootCacheKey {
    workspace_root: PathBuf,
    mode: GitAutoRepositoryDetection,
    repository_scan_max_depth: usize,
    ignored_folders: Vec<String>,
    open_editor_paths: Vec<PathBuf>,
}

impl KuroyaApp {
    pub(crate) fn spawn_index(&mut self) {
        if self.workspace_placeholder {
            self.invalidate_workspace_index_requests();
            self.status = "No folder open".to_owned();
            return;
        }
        let Some(request_id) = self.begin_workspace_index_request() else {
            reserve_startup_task_request_id_state(
                &mut self.workspace_index_next_request_id,
                &mut self.workspace_index_active_request_id,
            );
            return;
        };
        let root = self.workspace.root.clone();
        let index_options = self.project_index_options();
        let tx = self.tx.clone();
        self.record_async_task_started("Index Workspace", compact_path(&root));
        self.runtime.spawn_blocking(move || {
            // Single full walk per startup: publish the validated disk cache
            // immediately (quick open gets instant results), then reconcile
            // it against one fresh rebuild.
            if let Some(cache) =
                load_project_index_cache_unverified_with_options(&root, &index_options)
            {
                let _ = crate::ui_event_channel::send_critical_ui_event(
                    &tx,
                    UiEvent::CachedIndex {
                        request_id,
                        root: root.clone(),
                        index: cache.index.clone(),
                    },
                );
                let (fresh_index, fresh_signature) =
                    ProjectIndex::rebuild_with_signature_options(&root, &index_options);
                if fresh_signature == cache.signature {
                    let _ = crate::ui_event_channel::send_critical_ui_event(
                        &tx,
                        UiEvent::Indexed {
                            request_id,
                            root,
                            index: cache.index,
                        },
                    );
                } else {
                    let _ = save_project_index_cache(&root, &fresh_index, fresh_signature);
                    let _ = crate::ui_event_channel::send_critical_ui_event(
                        &tx,
                        UiEvent::Indexed {
                            request_id,
                            root,
                            index: fresh_index,
                        },
                    );
                }
                return;
            }
            let (index, signature) =
                ProjectIndex::rebuild_with_signature_options(&root, &index_options);
            let _ = save_project_index_cache(&root, &index, signature);
            let _ = crate::ui_event_channel::send_critical_ui_event(
                &tx,
                UiEvent::Indexed {
                    request_id,
                    root,
                    index,
                },
            );
        });
    }

    /// Applies a small watcher batch incrementally to the warm in-memory
    /// index instead of re-walking the workspace. Falls back to the full
    /// `spawn_index` walk when the index is cold, the batch is large, or the
    /// batch contains the workspace root itself.
    pub(crate) fn spawn_incremental_index(&mut self, changed_paths: Vec<PathBuf>) {
        if self.workspace_placeholder {
            self.invalidate_workspace_index_requests();
            self.status = "No folder open".to_owned();
            return;
        }
        let root = self.workspace.root.clone();
        if !workspace_refresh_paths_are_incremental(self.index.is_warm(), &root, &changed_paths) {
            self.spawn_index();
            return;
        }
        // The batch-cost check must run before the request slot is reserved:
        // falling back to `spawn_index` afterwards would find the slot taken
        // by an id whose task never publishes, wedging the index permanently.
        let indexed_files = self.index.files().len();
        if !workspace_refresh_batch_worth_patching(changed_paths.len(), indexed_files) {
            self.spawn_index();
            return;
        }
        let Some(request_id) = self.begin_workspace_index_request() else {
            reserve_startup_task_request_id_state(
                &mut self.workspace_index_next_request_id,
                &mut self.workspace_index_active_request_id,
            );
            return;
        };
        let index_options = self.project_index_options();
        let base_index = self.index.clone();
        let tx = self.tx.clone();
        self.record_async_task_started("Index Workspace", compact_path(&root));
        self.runtime.spawn_blocking(move || {
            let mut updated = base_index.clone_for_update();
            let mut changed_any = false;
            for chunk in changed_paths.chunks(WORKSPACE_INCREMENTAL_CHUNK_PATHS) {
                changed_any |= updated.apply_path_changes(&root, chunk);
            }
            if !changed_any {
                // Nothing changed on disk that affects the index; republish
                // the unchanged snapshot to release the request slot.
                let _ = crate::ui_event_channel::send_critical_ui_event(
                    &tx,
                    UiEvent::Indexed {
                        request_id,
                        root: root.clone(),
                        index: base_index,
                    },
                );
                return;
            }
            let signature = updated.signature_from_entries(&index_options);
            let _ = save_project_index_cache(&root, &updated, signature);
            let _ = crate::ui_event_channel::send_critical_ui_event(
                &tx,
                UiEvent::Indexed {
                    request_id,
                    root,
                    index: updated,
                },
            );
        });
    }

    pub(crate) fn project_index_options(&self) -> ProjectIndexOptions {
        let mut options = ProjectIndexOptions::with_exclude_globs(
            clamp_project_index_max_files(self.settings.project_index_max_files),
            merged_exclude_globs(&self.settings.project_index_exclude_globs),
        )
        .with_include_hidden_dirs(self.settings.project_index_include_hidden_dirs);
        for app_state in app_owned_state_dirs_in_workspace(&self.workspace.root) {
            options = options.with_excluded_path(app_state);
        }
        options
    }

    pub(crate) fn ensure_workspace_index_started(&mut self) -> bool {
        if self.workspace_placeholder
            || self.project_index_generation > 0
            || self.workspace_index_in_flight_request_id.is_some()
        {
            return false;
        }

        self.spawn_index();
        true
    }

    pub(crate) fn spawn_git_scan(&mut self) -> bool {
        if self.workspace_placeholder {
            self.invalidate_git_scan();
            self.status = "No folder open".to_owned();
            return false;
        }
        if !self.settings.git_enabled {
            self.invalidate_git_scan();
            return false;
        }
        if matches!(
            self.settings.git_auto_repository_detection,
            GitAutoRepositoryDetection::False
        ) {
            self.invalidate_git_scan();
            return false;
        }

        let Some(request_id) = self.begin_git_scan_request() else {
            reserve_startup_task_request_id_state(
                &mut self.git_scan_next_request_id,
                &mut self.git_scan_active_request_id,
            );
            return false;
        };
        let root = self.workspace.root.clone();
        let open_editor_paths = self
            .buffers
            .iter()
            .filter_map(|buffer| buffer.path().cloned())
            .collect::<Vec<_>>();
        let root_cache_entry = self.git_scan_root_cache.clone();
        let ignored_repositories = self.settings.git_ignored_repositories.clone();
        let auto_repository_detection = self.settings.git_auto_repository_detection;
        let repository_scan_max_depth = self.settings.git_repository_scan_max_depth;
        let repository_scan_ignored_folders =
            self.settings.git_repository_scan_ignored_folders.clone();
        let tx = self.tx.clone();
        let status_limit = self.settings.git_status_limit;
        let ignore_submodules = self.settings.git_ignore_submodules;
        let detect_submodules = self.settings.git_detect_submodules;
        let detect_submodules_limit = self.settings.git_detect_submodules_limit;
        let similarity_threshold = self.settings.git_similarity_threshold;
        let open_parent_repository_mode = self.settings.git_open_repository_in_parent_folders;
        self.record_async_task_started("Git Scan", git_scan_task_detail(request_id, &root));
        self.runtime.spawn_blocking(move || {
            let mut scan_root = None;
            let mut next_root_cache_entry = None;
            let mut git = GitSnapshot::default();
            if !git_repository_ignored(&root, &ignored_repositories) {
                let (resolved_scan_root, resolved_root_cache_entry) =
                    resolved_cached_git_scan_root_for_auto_repository_detection(
                        root_cache_entry,
                        &root,
                        auto_repository_detection,
                        repository_scan_max_depth,
                        &repository_scan_ignored_folders,
                        &open_editor_paths,
                    );
                next_root_cache_entry = resolved_root_cache_entry;
                if let Some(resolved_scan_root) = resolved_scan_root {
                    let open_parent_repositories =
                        git_open_parent_repositories(open_parent_repository_mode)
                            && !git_repository_scan_folder_ignored(
                                &root,
                                &repository_scan_ignored_folders,
                            );
                    git = GitSnapshot::scan_with_status_options_and_parent_policy(
                        &resolved_scan_root,
                        status_limit,
                        ignore_submodules,
                        detect_submodules,
                        detect_submodules_limit,
                        similarity_threshold,
                        open_parent_repositories,
                    );
                    scan_root = Some(resolved_scan_root);
                }
            }
            let _ = crate::ui_event_channel::send_critical_ui_event(
                &tx,
                UiEvent::GitScanned {
                    request_id,
                    root,
                    scan_root,
                    root_cache_entry: next_root_cache_entry,
                    git,
                },
            );
        });
        true
    }

    pub(crate) fn spawn_git_auto_refresh(&mut self) -> bool {
        if !git_auto_refresh_enabled(self.settings.git_enabled, self.settings.git_autorefresh) {
            return false;
        }
        self.spawn_git_scan()
    }

    /// Refreshes git after external file changes (watcher batches, saves):
    /// small known batches scope the status query to the changed paths,
    /// anything else falls back to a full scan. Gated on `git.autorefresh`
    /// like the other automatic refresh entry points.
    pub(crate) fn spawn_git_refresh_for_changed_paths(&mut self, paths: Vec<PathBuf>) -> bool {
        if !git_auto_refresh_enabled(self.settings.git_enabled, self.settings.git_autorefresh) {
            return false;
        }
        if paths.is_empty() || paths.len() > GIT_SCOPED_REFRESH_MAX_PATHS {
            return self.spawn_git_scan();
        }
        self.spawn_git_scoped_refresh(paths)
    }

    /// Refreshes git after a buffer save: the saved path scopes the status
    /// query when it stays inside the workspace, otherwise this falls back to
    /// the full auto refresh. Gated on `git.autorefresh`.
    pub(crate) fn spawn_git_refresh_for_saved_path(&mut self, path: &Path) -> bool {
        if !git_auto_refresh_enabled(self.settings.git_enabled, self.settings.git_autorefresh) {
            return false;
        }
        if !workspace_path_contains_lexically(&self.workspace.root, path) {
            return self.spawn_git_scan();
        }
        self.spawn_git_scoped_refresh(vec![path.to_path_buf()])
    }

    /// Updates the current snapshot with a pathspec-scoped status query for
    /// `paths` instead of rescanning the whole worktree. Falls back to the
    /// full `spawn_git_scan` whenever the snapshot cannot be safely scoped:
    /// git disabled, no repository, the resolved scan root no longer matching
    /// the snapshot root, an ignored repository, empty/oversized batches, or
    /// paths outside the snapshot root. Coalesces through the same request-id
    /// machinery as full scans.
    pub(crate) fn spawn_git_scoped_refresh(&mut self, paths: Vec<PathBuf>) -> bool {
        if self.workspace_placeholder
            || git_repository_ignored(
                &self.workspace.root,
                &self.settings.git_ignored_repositories,
            )
        {
            return self.spawn_git_scan();
        }
        let open_editor_paths = self
            .buffers
            .iter()
            .filter_map(|buffer| buffer.path().cloned())
            .collect::<Vec<_>>();
        let (resolved_scan_root, resolved_root_cache_entry) =
            resolved_cached_git_scan_root_for_auto_repository_detection(
                self.git_scan_root_cache.clone(),
                &self.workspace.root,
                self.settings.git_auto_repository_detection,
                self.settings.git_repository_scan_max_depth,
                &self.settings.git_repository_scan_ignored_folders,
                &open_editor_paths,
            );
        if !git_scoped_refresh_is_supported(
            self.settings.git_enabled,
            self.git.root(),
            resolved_scan_root.as_deref(),
            &paths,
            GIT_SCOPED_REFRESH_MAX_PATHS,
        ) {
            return self.spawn_git_scan();
        }
        let snapshot_root = self
            .git
            .root()
            .expect("scoped refresh checked the snapshot root")
            .to_path_buf();
        let Some(request_id) = self.begin_git_scan_request() else {
            reserve_startup_task_request_id_state(
                &mut self.git_scan_next_request_id,
                &mut self.git_scan_active_request_id,
            );
            return false;
        };
        let root = self.workspace.root.clone();
        let base_snapshot = self.git.clone();
        let tx = self.tx.clone();
        let status_limit = self.settings.git_status_limit;
        let ignore_submodules = self.settings.git_ignore_submodules;
        let detect_submodules = self.settings.git_detect_submodules;
        let detect_submodules_limit = self.settings.git_detect_submodules_limit;
        let similarity_threshold = self.settings.git_similarity_threshold;
        let open_parent_repositories =
            git_open_parent_repositories(self.settings.git_open_repository_in_parent_folders)
                && !git_repository_scan_folder_ignored(
                    &root,
                    &self.settings.git_repository_scan_ignored_folders,
                );
        self.record_async_task_started(
            "Git Scan",
            git_scan_task_detail(request_id, &snapshot_root),
        );
        self.runtime.spawn_blocking(move || {
            // A failed scoped query republishes the unchanged snapshot to
            // release the request slot; the watcher or the next explicit
            // refresh triggers the full rescan.
            let git = kuroya_core::git_scoped_status_snapshot(
                &base_snapshot,
                &paths,
                status_limit,
                ignore_submodules,
                detect_submodules,
                detect_submodules_limit,
                similarity_threshold,
                open_parent_repositories,
            )
            .unwrap_or(base_snapshot);
            let _ = crate::ui_event_channel::send_critical_ui_event(
                &tx,
                UiEvent::GitScanned {
                    request_id,
                    root,
                    scan_root: Some(snapshot_root),
                    root_cache_entry: resolved_root_cache_entry,
                    git,
                },
            );
        });
        true
    }

    pub(crate) fn schedule_workspace_refresh(&mut self) {
        self.schedule_workspace_refresh_with_paths(Vec::new());
    }

    pub(crate) fn schedule_workspace_refresh_with_paths(&mut self, project_paths: Vec<PathBuf>) {
        let now = Instant::now();
        if let Some(pending) = &mut self.pending_workspace_refresh {
            pending.record_change(now);
            pending.record_project_paths(project_paths);
        } else {
            let mut pending = PendingWorkspaceRefresh::new(now);
            pending.record_project_paths(project_paths);
            self.pending_workspace_refresh = Some(pending);
        }
    }

    pub(crate) fn flush_pending_workspace_refresh(&mut self) -> usize {
        let now = Instant::now();
        let pending = self.pending_workspace_refresh.take();
        let due = pending
            .as_ref()
            .is_some_and(|pending| pending_refresh_is_due(pending, now));
        if !due {
            self.pending_workspace_refresh = pending;
            return 0;
        }

        let project_paths = pending
            .map(|pending| pending.project_paths)
            .unwrap_or_default();
        if workspace_refresh_paths_are_incremental(
            self.index.is_warm(),
            &self.workspace.root,
            &project_paths,
        ) {
            self.spawn_incremental_index(project_paths.clone());
        } else {
            self.spawn_index();
        }
        1 + usize::from(self.spawn_git_refresh_for_changed_paths(project_paths))
    }

    fn begin_workspace_index_request(&mut self) -> Option<u64> {
        begin_workspace_index_request_state(
            &mut self.workspace_index_next_request_id,
            &mut self.workspace_index_active_request_id,
            &mut self.workspace_index_in_flight_request_id,
            &mut self.workspace_index_refresh_queued,
        )
    }

    pub(crate) fn finish_workspace_index_request(&mut self, request_id: u64) -> bool {
        finish_workspace_index_request_state(
            &mut self.workspace_index_in_flight_request_id,
            &mut self.workspace_index_refresh_queued,
            request_id,
        )
    }

    pub(crate) fn invalidate_workspace_index_requests(&mut self) {
        invalidate_workspace_index_request_state(
            &mut self.workspace_index_next_request_id,
            &mut self.workspace_index_active_request_id,
            &mut self.workspace_index_in_flight_request_id,
            &mut self.workspace_index_refresh_queued,
        );
    }

    fn begin_git_scan_request(&mut self) -> Option<u64> {
        begin_git_scan_request_state(
            &mut self.git_scan_next_request_id,
            &mut self.git_scan_active_request_id,
            &mut self.git_scan_in_flight_request_id,
            &mut self.git_scan_refresh_queued,
        )
    }

    pub(crate) fn finish_git_scan_request(&mut self, request_id: u64) -> bool {
        finish_git_scan_request_state(
            &mut self.git_scan_in_flight_request_id,
            &mut self.git_scan_refresh_queued,
            request_id,
        )
    }

    pub(crate) fn invalidate_git_scan_requests(&mut self) {
        invalidate_git_scan_request_state(
            &mut self.git_scan_next_request_id,
            &mut self.git_scan_active_request_id,
            &mut self.git_scan_in_flight_request_id,
            &mut self.git_scan_refresh_queued,
        );
    }

    pub(crate) fn invalidate_git_scan(&mut self) {
        self.invalidate_git_scan_requests();
        self.invalidate_virtual_source_control_open_requests();
        self.git_scan_root_cache = None;
        self.git = GitSnapshot::default();
        self.source_control_selected = 0;
    }

    pub(crate) fn sync_git_enabled_state(&mut self) {
        if self.settings.git_enabled {
            self.spawn_git_scan();
        } else {
            self.invalidate_git_scan();
            self.status = "Git is disabled".to_owned();
        }
    }

    pub(crate) fn sync_git_repository_filters_state(&mut self) {
        if !self.settings.git_enabled {
            self.invalidate_git_scan();
        } else if git_repository_ignored(
            &self.workspace.root,
            &self.settings.git_ignored_repositories,
        ) {
            self.invalidate_git_scan();
            self.status = "Git repository ignored by git.ignoredRepositories".to_owned();
        } else {
            self.spawn_git_scan();
        }
    }

    pub(crate) fn sync_git_autorefresh_state(&mut self, previous_git_autorefresh: bool) {
        if !previous_git_autorefresh && self.settings.git_autorefresh && self.settings.git_enabled {
            self.spawn_git_scan();
        }
    }

    pub(crate) fn sync_plugin_settings_state(&mut self) {
        if self.settings.plugins.enabled && workspace_plugins_enabled(self.workspace_trusted) {
            self.spawn_plugin_discovery();
            return;
        }
        if !self.workspace_plugins_state_present()
            && self.workspace_plugins_in_flight_request_id.is_none()
            && self.pending_workspace_plugin_reload.is_none()
        {
            return;
        }
        self.invalidate_workspace_plugin_discovery();
        self.pending_workspace_plugin_reload = None;
        self.clear_workspace_plugins();
        self.status = if self.settings.plugins.enabled {
            workspace_plugins_restricted_status().to_owned()
        } else {
            plugins_disabled_status().to_owned()
        };
    }

    fn workspace_plugins_state_present(&self) -> bool {
        !self.plugins.is_empty()
            || !self.plugin_errors.is_empty()
            || !self.plugin_runtimes.is_empty()
            || self.plugin_activations.active_count() > 0
            || !self.plugin_commands.is_empty()
            || !self.plugin_languages.is_empty()
            || !self.plugin_themes.is_empty()
            || !self.plugin_syntaxes.is_empty()
    }

    pub(crate) fn spawn_plugin_discovery(&mut self) -> bool {
        self.pending_workspace_plugin_reload = None;
        if self.workspace_placeholder {
            self.invalidate_workspace_plugin_discovery();
            self.clear_workspace_plugins();
            self.status = "No folder open".to_owned();
            return false;
        }
        if !workspace_plugins_enabled(self.workspace_trusted) {
            self.invalidate_workspace_plugin_discovery();
            self.clear_workspace_plugins();
            self.status = workspace_plugins_restricted_status().to_owned();
            return false;
        }
        if !self.settings.plugins.enabled {
            self.invalidate_workspace_plugin_discovery();
            self.clear_workspace_plugins();
            self.status = plugins_disabled_status().to_owned();
            return false;
        }

        let Some(request_id) = self.begin_workspace_plugin_discovery_request() else {
            return false;
        };
        let root = self.workspace.root.clone();
        let disabled_plugin_ids = self.settings.plugins.disabled_ids.clone();
        let tx = self.tx.clone();
        self.record_async_task_started("Workspace Plugins", compact_path(&root));
        self.runtime.spawn_blocking(move || {
            let event = match discover_workspace_plugins(&root) {
                Ok(mut discovery) => {
                    filter_disabled_workspace_plugins(&mut discovery.plugins, &disabled_plugin_ids);
                    let syntax_load = PluginSyntaxLoad::from_plugins(&discovery.plugins);
                    discovery.errors.extend(syntax_load.errors.clone());
                    UiEvent::WorkspacePluginsLoaded {
                        request_id,
                        root,
                        plugins: discovery.plugins,
                        errors: discovery.errors,
                        syntax_load,
                    }
                }
                Err(error) => UiEvent::WorkspacePluginsFailed {
                    request_id,
                    root,
                    error: error.to_string(),
                },
            };
            let _ = crate::ui_event_channel::send_critical_ui_event(&tx, event);
        });
        true
    }

    pub(crate) fn schedule_workspace_plugin_reload(&mut self) {
        let now = Instant::now();
        if let Some(pending) = &mut self.pending_workspace_plugin_reload {
            pending.record_change(now);
        } else {
            self.pending_workspace_plugin_reload = Some(PendingWorkspacePluginReload::new(now));
        }
    }

    pub(crate) fn flush_pending_workspace_plugin_reload(&mut self) -> usize {
        if !workspace_plugin_reload_due(
            self.pending_workspace_plugin_reload,
            Instant::now(),
            WORKSPACE_PLUGIN_RELOAD_DEBOUNCE,
            WORKSPACE_PLUGIN_RELOAD_MAX_WAIT,
        ) {
            return 0;
        }

        usize::from(self.spawn_plugin_discovery())
    }

    pub(crate) fn workspace_refresh_wakeup(&self) -> Option<Instant> {
        self.pending_workspace_refresh
            .as_ref()
            .map(pending_refresh_wakeup_at)
    }

    pub(crate) fn workspace_plugin_reload_wakeup(&self) -> Option<Instant> {
        workspace_plugin_reload_wakeup_at(
            self.pending_workspace_plugin_reload,
            WORKSPACE_PLUGIN_RELOAD_DEBOUNCE,
            WORKSPACE_PLUGIN_RELOAD_MAX_WAIT,
        )
    }

    fn begin_workspace_plugin_discovery_request(&mut self) -> Option<u64> {
        begin_workspace_plugin_discovery_request_state(
            &mut self.workspace_plugins_next_request_id,
            &mut self.workspace_plugins_active_request_id,
            &mut self.workspace_plugins_in_flight_request_id,
            &mut self.workspace_plugins_reload_queued,
        )
    }

    pub(crate) fn finish_workspace_plugin_discovery_request(&mut self, request_id: u64) -> bool {
        finish_workspace_plugin_discovery_request_state(
            &mut self.workspace_plugins_in_flight_request_id,
            &mut self.workspace_plugins_reload_queued,
            request_id,
        )
    }

    pub(crate) fn invalidate_workspace_plugin_discovery_requests(&mut self) {
        invalidate_workspace_plugin_discovery_request_state(
            &mut self.workspace_plugins_next_request_id,
            &mut self.workspace_plugins_active_request_id,
            &mut self.workspace_plugins_in_flight_request_id,
            &mut self.workspace_plugins_reload_queued,
        );
    }

    pub(crate) fn invalidate_workspace_plugin_discovery(&mut self) {
        self.invalidate_workspace_plugin_discovery_requests();
    }

    pub(crate) fn clear_workspace_plugins(&mut self) {
        self.plugins.clear();
        self.plugin_errors.clear();
        self.plugin_runtimes = Default::default();
        self.plugin_activations = Default::default();
        self.plugin_commands = Default::default();
        self.plugin_languages = Default::default();
        self.plugin_themes = Default::default();
        self.plugin_syntaxes = Default::default();
        self.highlighter.reset_plugin_syntaxes();
        self.theme_picker_selected =
            selected_theme_index_with_plugins(&self.settings.theme, &self.plugin_themes);
    }
}

fn reserve_startup_task_request_id_state(
    next_request_id: &mut u64,
    active_request_id: &mut u64,
) -> u64 {
    *next_request_id = next_startup_task_request_id(*next_request_id);
    *active_request_id = *next_request_id;
    *active_request_id
}

fn next_startup_task_request_id(current: u64) -> u64 {
    match current.wrapping_add(1) {
        0 => 1,
        next => next,
    }
}

fn begin_startup_task_request_state(
    next_request_id: &mut u64,
    active_request_id: &mut u64,
    in_flight_request_id: &mut Option<u64>,
    queued: &mut bool,
) -> Option<u64> {
    if in_flight_request_id.is_some() {
        *queued = true;
        return None;
    }
    let request_id = reserve_startup_task_request_id_state(next_request_id, active_request_id);
    *in_flight_request_id = Some(request_id);
    Some(request_id)
}

fn finish_startup_task_request_state(
    in_flight_request_id: &mut Option<u64>,
    queued: &mut bool,
    request_id: u64,
) -> bool {
    if *in_flight_request_id != Some(request_id) {
        return false;
    }
    *in_flight_request_id = None;
    let should_spawn_queued = *queued;
    *queued = false;
    should_spawn_queued
}

fn invalidate_startup_task_request_state(
    next_request_id: &mut u64,
    active_request_id: &mut u64,
    in_flight_request_id: &mut Option<u64>,
    queued: &mut bool,
) {
    let _ = reserve_startup_task_request_id_state(next_request_id, active_request_id);
    *in_flight_request_id = None;
    *queued = false;
}

fn begin_workspace_index_request_state(
    next_request_id: &mut u64,
    active_request_id: &mut u64,
    in_flight_request_id: &mut Option<u64>,
    refresh_queued: &mut bool,
) -> Option<u64> {
    begin_startup_task_request_state(
        next_request_id,
        active_request_id,
        in_flight_request_id,
        refresh_queued,
    )
}

fn finish_workspace_index_request_state(
    in_flight_request_id: &mut Option<u64>,
    refresh_queued: &mut bool,
    request_id: u64,
) -> bool {
    finish_startup_task_request_state(in_flight_request_id, refresh_queued, request_id)
}

fn invalidate_workspace_index_request_state(
    next_request_id: &mut u64,
    active_request_id: &mut u64,
    in_flight_request_id: &mut Option<u64>,
    refresh_queued: &mut bool,
) {
    invalidate_startup_task_request_state(
        next_request_id,
        active_request_id,
        in_flight_request_id,
        refresh_queued,
    );
}

fn begin_git_scan_request_state(
    next_request_id: &mut u64,
    active_request_id: &mut u64,
    in_flight_request_id: &mut Option<u64>,
    refresh_queued: &mut bool,
) -> Option<u64> {
    begin_startup_task_request_state(
        next_request_id,
        active_request_id,
        in_flight_request_id,
        refresh_queued,
    )
}

fn finish_git_scan_request_state(
    in_flight_request_id: &mut Option<u64>,
    refresh_queued: &mut bool,
    request_id: u64,
) -> bool {
    finish_startup_task_request_state(in_flight_request_id, refresh_queued, request_id)
}

fn invalidate_git_scan_request_state(
    next_request_id: &mut u64,
    active_request_id: &mut u64,
    in_flight_request_id: &mut Option<u64>,
    refresh_queued: &mut bool,
) {
    invalidate_startup_task_request_state(
        next_request_id,
        active_request_id,
        in_flight_request_id,
        refresh_queued,
    );
}

fn begin_workspace_plugin_discovery_request_state(
    next_request_id: &mut u64,
    active_request_id: &mut u64,
    in_flight_request_id: &mut Option<u64>,
    reload_queued: &mut bool,
) -> Option<u64> {
    begin_startup_task_request_state(
        next_request_id,
        active_request_id,
        in_flight_request_id,
        reload_queued,
    )
}

fn finish_workspace_plugin_discovery_request_state(
    in_flight_request_id: &mut Option<u64>,
    reload_queued: &mut bool,
    request_id: u64,
) -> bool {
    finish_startup_task_request_state(in_flight_request_id, reload_queued, request_id)
}

fn invalidate_workspace_plugin_discovery_request_state(
    next_request_id: &mut u64,
    active_request_id: &mut u64,
    in_flight_request_id: &mut Option<u64>,
    reload_queued: &mut bool,
) {
    invalidate_startup_task_request_state(
        next_request_id,
        active_request_id,
        in_flight_request_id,
        reload_queued,
    );
}

pub(crate) fn workspace_plugins_enabled(workspace_trusted: bool) -> bool {
    workspace_trusted
}

pub(crate) fn workspace_plugins_restricted_status() -> &'static str {
    "Trust this workspace to enable workspace plugins"
}

pub(crate) fn plugins_disabled_status() -> &'static str {
    "Plugins are disabled"
}

pub(crate) fn filter_disabled_workspace_plugins(
    plugins: &mut Vec<PluginDescriptor>,
    disabled_ids: &[String],
) -> usize {
    if disabled_ids.is_empty() {
        return 0;
    }
    let before = plugins.len();
    plugins.retain(|plugin| {
        !disabled_ids
            .iter()
            .any(|id| *id == plugin.manifest.id.trim())
    });
    before - plugins.len()
}

pub(crate) fn workspace_plugin_reload_due(
    pending: Option<PendingWorkspacePluginReload>,
    now: Instant,
    debounce: Duration,
    max_wait: Duration,
) -> bool {
    pending.is_some_and(|pending| {
        now.saturating_duration_since(pending.last_seen) >= debounce
            || now.saturating_duration_since(pending.first_seen) >= max_wait
    })
}

fn pending_refresh_wakeup_at(pending: &PendingWorkspaceRefresh) -> Instant {
    (pending.last_seen + WORKSPACE_REFRESH_DEBOUNCE)
        .min(pending.first_seen + WORKSPACE_REFRESH_MAX_WAIT)
}

pub(crate) fn workspace_plugin_reload_wakeup_at(
    pending: Option<PendingWorkspacePluginReload>,
    debounce: Duration,
    max_wait: Duration,
) -> Option<Instant> {
    pending.map(|pending| (pending.last_seen + debounce).min(pending.first_seen + max_wait))
}

pub(crate) fn pending_refresh_is_due(pending: &PendingWorkspaceRefresh, now: Instant) -> bool {
    now.saturating_duration_since(pending.last_seen) >= WORKSPACE_REFRESH_DEBOUNCE
        || now.saturating_duration_since(pending.first_seen) >= WORKSPACE_REFRESH_MAX_WAIT
}

/// True when a debounced watcher batch can be applied incrementally instead
/// of triggering a full re-walk: the in-memory index must be warm, the batch
/// must be small (`WORKSPACE_INCREMENTAL_INDEX_MAX_PATHS`), and it must not
/// contain the workspace root itself. Overflowed watchers and cold indexes
/// arrive here with an empty batch and fall back to the full walk.
pub(crate) fn workspace_refresh_paths_are_incremental(
    index_is_warm: bool,
    workspace_root: &Path,
    project_paths: &[PathBuf],
) -> bool {
    index_is_warm
        && !project_paths.is_empty()
        && project_paths.len() <= WORKSPACE_INCREMENTAL_INDEX_MAX_PATHS
        && project_paths
            .iter()
            .all(|path| !trusted_workspace_paths_match(path, workspace_root))
}

/// Cost check for choosing incremental patching over a re-walk: patching
/// visits one subtree per path, a walk visits every indexed file, so once a
/// batch approaches an eighth of the index a walk is cheaper and simpler.
pub(crate) fn workspace_refresh_batch_worth_patching(
    batch_len: usize,
    indexed_files: usize,
) -> bool {
    indexed_files > 0 && batch_len.saturating_mul(8) <= indexed_files
}

pub(crate) fn git_auto_refresh_enabled(git_enabled: bool, git_autorefresh: bool) -> bool {
    git_enabled && git_autorefresh
}

/// True when the current snapshot can be refreshed with a scoped status
/// query for `paths`: git must be enabled, the snapshot must have a
/// repository root, the resolved scan root must still match the snapshot
/// root, and every path must stay inside the snapshot root and within the
/// batch limit. Anything else falls back to the full scan.
pub(crate) fn git_scoped_refresh_is_supported(
    git_enabled: bool,
    snapshot_root: Option<&Path>,
    resolved_scan_root: Option<&Path>,
    paths: &[PathBuf],
    max_paths: usize,
) -> bool {
    if !git_enabled {
        return false;
    }
    let Some(snapshot_root) = snapshot_root else {
        return false;
    };
    let Some(resolved_scan_root) = resolved_scan_root else {
        return false;
    };
    if !trusted_workspace_paths_match(resolved_scan_root, snapshot_root) {
        return false;
    }
    if paths.is_empty() || paths.len() > max_paths {
        return false;
    }
    paths
        .iter()
        .all(|path| workspace_path_contains_lexically(snapshot_root, path))
}

pub(crate) fn git_open_parent_repositories(mode: GitOpenRepositoryInParentFolders) -> bool {
    !matches!(mode, GitOpenRepositoryInParentFolders::Never)
}

pub(crate) fn git_scan_root_for_auto_repository_detection(
    root: &Path,
    mode: GitAutoRepositoryDetection,
    repository_scan_max_depth: usize,
    ignored_folders: &[String],
    open_editor_paths: &[PathBuf],
) -> Option<PathBuf> {
    match mode {
        GitAutoRepositoryDetection::False => None,
        GitAutoRepositoryDetection::True => Some(root.to_path_buf()),
        GitAutoRepositoryDetection::SubFolders => {
            if git_repository_marker_exists(root) {
                Some(root.to_path_buf())
            } else {
                git_repository_in_subfolders(root, repository_scan_max_depth, ignored_folders)
            }
        }
        GitAutoRepositoryDetection::OpenEditors => open_editor_paths
            .iter()
            .filter_map(|path| git_repository_for_open_editor(root, path, ignored_folders))
            .next(),
    }
}

#[cfg(test)]
fn cached_git_scan_root_for_auto_repository_detection(
    cache: &mut Option<GitScanRootCacheEntry>,
    root: &Path,
    mode: GitAutoRepositoryDetection,
    repository_scan_max_depth: usize,
    ignored_folders: &[String],
    open_editor_paths: &[PathBuf],
) -> Option<PathBuf> {
    let (scan_root, next_cache) = resolved_cached_git_scan_root_for_auto_repository_detection(
        cache.clone(),
        root,
        mode,
        repository_scan_max_depth,
        ignored_folders,
        open_editor_paths,
    );
    *cache = next_cache;
    scan_root
}

fn resolved_cached_git_scan_root_for_auto_repository_detection(
    cache: Option<GitScanRootCacheEntry>,
    root: &Path,
    mode: GitAutoRepositoryDetection,
    repository_scan_max_depth: usize,
    ignored_folders: &[String],
    open_editor_paths: &[PathBuf],
) -> (Option<PathBuf>, Option<GitScanRootCacheEntry>) {
    if !git_scan_root_is_cacheable(mode) {
        return (
            git_scan_root_for_auto_repository_detection(
                root,
                mode,
                repository_scan_max_depth,
                ignored_folders,
                open_editor_paths,
            ),
            None,
        );
    }

    let key = git_scan_root_cache_key(
        root,
        mode,
        repository_scan_max_depth,
        ignored_folders,
        open_editor_paths,
    );
    if let Some(entry) = cache
        && entry.key == key
        && git_repository_marker_exists(&entry.scan_root)
    {
        return (Some(entry.scan_root.clone()), Some(entry));
    }

    let scan_root = git_scan_root_for_auto_repository_detection(
        root,
        mode,
        repository_scan_max_depth,
        ignored_folders,
        open_editor_paths,
    );
    let cache_entry = scan_root.as_ref().map(|scan_root| GitScanRootCacheEntry {
        key,
        scan_root: scan_root.clone(),
    });
    (scan_root, cache_entry)
}

fn git_scan_root_is_cacheable(mode: GitAutoRepositoryDetection) -> bool {
    matches!(
        mode,
        GitAutoRepositoryDetection::SubFolders | GitAutoRepositoryDetection::OpenEditors
    )
}

fn git_scan_root_cache_key(
    root: &Path,
    mode: GitAutoRepositoryDetection,
    repository_scan_max_depth: usize,
    ignored_folders: &[String],
    open_editor_paths: &[PathBuf],
) -> GitScanRootCacheKey {
    GitScanRootCacheKey {
        workspace_root: root.to_path_buf(),
        mode,
        repository_scan_max_depth: kuroya_core::clamp_git_repository_scan_max_depth(
            repository_scan_max_depth,
        ),
        ignored_folders: ignored_folders
            .iter()
            .map(|entry| entry.trim().to_owned())
            .filter(|entry| !entry.is_empty())
            .collect(),
        open_editor_paths: open_editor_paths.to_vec(),
    }
}

fn git_repository_for_open_editor(
    root: &Path,
    path: &Path,
    ignored_folders: &[String],
) -> Option<PathBuf> {
    let mut current = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()?.to_path_buf()
    };

    loop {
        if !path_is_or_inside(&current, root) {
            return None;
        }
        let ignored = !paths_match(&current, root)
            && git_repository_scan_folder_ignored(&current, ignored_folders);
        if !ignored && git_repository_marker_exists(&current) {
            return Some(current);
        }
        if paths_match(&current, root) || !current.pop() {
            return None;
        }
    }
}

fn git_repository_in_subfolders(
    root: &Path,
    max_depth: usize,
    ignored_folders: &[String],
) -> Option<PathBuf> {
    git_repository_in_subfolders_with_limits(
        root,
        max_depth,
        ignored_folders,
        GIT_REPOSITORY_SCAN_MAX_CHILDREN_PER_FOLDER,
        GIT_REPOSITORY_SCAN_MAX_VISITED_FOLDERS,
    )
}

fn git_repository_in_subfolders_with_limits(
    root: &Path,
    max_depth: usize,
    ignored_folders: &[String],
    max_children_per_folder: usize,
    max_visited_folders: usize,
) -> Option<PathBuf> {
    let max_depth = kuroya_core::clamp_git_repository_scan_max_depth(max_depth);
    if max_depth == 0 || max_children_per_folder == 0 || max_visited_folders == 0 {
        return None;
    }

    let mut pending = vec![(root.to_path_buf(), 0usize)];
    let mut visited = 0usize;
    while let Some((folder, depth)) = pending.pop() {
        if visited >= max_visited_folders {
            return None;
        }
        visited += 1;

        if depth >= max_depth {
            continue;
        }

        let mut children = git_repository_scan_children(&folder, max_children_per_folder);
        children.retain(|child| !git_repository_scan_folder_ignored(child, ignored_folders));
        for child in &children {
            if git_repository_marker_exists(child) {
                return Some(child.clone());
            }
        }
        for child in children.into_iter().rev() {
            pending.push((child, depth + 1));
        }
    }
    None
}

fn git_repository_scan_children(folder: &Path, max_children: usize) -> Vec<PathBuf> {
    if max_children == 0 {
        return Vec::new();
    }
    let Ok(read_dir) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut children = Vec::new();
    for entry in read_dir.filter_map(Result::ok) {
        let path = entry.path();
        if !git_repository_scan_entry_is_dir(&entry, &path) {
            continue;
        }
        children.push(path);
        if children.len() >= max_children {
            break;
        }
    }
    children.sort();
    children
}

fn git_repository_scan_entry_is_dir(entry: &std::fs::DirEntry, path: &Path) -> bool {
    match entry.file_type() {
        Ok(file_type) if file_type.is_dir() => true,
        Ok(file_type) if file_type.is_file() => false,
        Ok(_) | Err(_) => path.is_dir(),
    }
}

fn git_repository_marker_exists(path: &Path) -> bool {
    match std::fs::metadata(path.join(".git")) {
        Ok(metadata) => metadata.is_dir() || metadata.is_file(),
        Err(_) => false,
    }
}

pub(crate) fn git_repository_ignored(root: &Path, ignored_repositories: &[String]) -> bool {
    ignored_repositories.iter().any(|entry| {
        let entry = entry.trim();
        if entry.is_empty() {
            return false;
        }

        let configured = PathBuf::from(entry);
        if configured.is_absolute() {
            paths_match(root, &configured)
        } else {
            paths_match(root, &root.join(&configured)) || root.ends_with(&configured)
        }
    })
}

pub(crate) fn git_repository_scan_folder_ignored(root: &Path, ignored_folders: &[String]) -> bool {
    ignored_folders.iter().any(|entry| {
        let entry = entry.trim();
        if entry.is_empty() {
            return false;
        }

        let configured = PathBuf::from(entry);
        if configured.is_absolute() {
            path_is_or_inside(root, &configured)
        } else {
            path_contains_relative_folder(root, &configured)
        }
    })
}

fn path_is_or_inside(path: &Path, folder: &Path) -> bool {
    let path = normalized_path_key(path);
    let folder = normalized_path_key(folder);
    !folder.is_empty()
        && (path == folder
            || path
                .strip_prefix(&folder)
                .is_some_and(|rest| rest.starts_with('/')))
}

fn path_contains_relative_folder(path: &Path, folder: &Path) -> bool {
    let path = normalized_path_key(path);
    let folder = normalized_path_key(folder);
    !folder.is_empty() && path_contains_relative_folder_key(&path, &folder)
}

fn path_contains_relative_folder_key(path: &str, folder: &str) -> bool {
    let mut search_from = 0;
    while let Some(relative_start) = path[search_from..].find(folder) {
        let start = search_from + relative_start;
        let end = start + folder.len();
        let starts_at_boundary = start == 0 || path.as_bytes()[start - 1] == b'/';
        let ends_at_boundary = end == path.len() || path.as_bytes()[end] == b'/';
        if starts_at_boundary && ends_at_boundary {
            return true;
        }
        search_from = end;
    }
    false
}

fn paths_match(left: &Path, right: &Path) -> bool {
    normalized_path_key(left) == normalized_path_key(right)
}

fn normalized_path_key(path: &Path) -> String {
    let normalized =
        std::fs::canonicalize(path).unwrap_or_else(|_| lexical_normalize_startup_path(path));
    let mut key = String::new();
    let mut first = true;
    for component in normalized.components() {
        if first {
            first = false;
        } else {
            key.push('/');
        }
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::Prefix(prefix) => {
                push_normalized_path_key_text(&mut key, &prefix.as_os_str().to_string_lossy());
            }
            std::path::Component::RootDir => {}
            std::path::Component::ParentDir => key.push_str(".."),
            std::path::Component::Normal(part) => {
                push_normalized_path_key_text(&mut key, &part.to_string_lossy());
            }
        }
    }
    while key.len() > 1 && key.ends_with('/') {
        key.pop();
    }
    if cfg!(windows) {
        key.make_ascii_lowercase();
    }
    key
}

fn push_normalized_path_key_text(key: &mut String, text: &str) {
    if text.as_bytes().contains(&b'\\') {
        key.reserve(text.len());
        for ch in text.chars() {
            if ch == '\\' {
                key.push('/');
            } else {
                key.push(ch);
            }
        }
    } else {
        key.push_str(text);
    }
}

fn lexical_normalize_startup_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    let mut has_root = false;
    for component in path.components() {
        match component {
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            std::path::Component::RootDir => {
                has_root = true;
                normalized.push(component.as_os_str());
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                let can_pop_normal = normalized
                    .components()
                    .next_back()
                    .is_some_and(|component| matches!(component, std::path::Component::Normal(_)));
                if can_pop_normal {
                    normalized.pop();
                } else if !has_root {
                    normalized.push("..");
                }
            }
            std::path::Component::Normal(part) => normalized.push(part),
        }
    }
    if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests;
