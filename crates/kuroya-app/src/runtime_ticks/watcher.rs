use crate::{
    KuroyaApp,
    fs_watcher::{FileWatcher, collapse_case_only_path_collisions, is_recent_app_write},
    ui_text::count_label,
    workspace_state::{
        classify_watched_paths_with_filter, dirty_open_buffers_for_changes,
        reloadable_open_buffers_for_changes,
    },
    workspace_trust::{
        trusted_workspace_paths_match, workspace_path_contains_lexically,
        workspace_path_stays_within_root_lexically,
    },
};
use std::{
    collections::HashSet,
    ffi::OsStr,
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

const WATCHER_REBUILD_INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const WATCHER_REBUILD_MAX_BACKOFF: Duration = Duration::from_secs(30);
const WATCHER_REBUILD_MAX_DOUBLING_SHIFT: u32 = 5;

impl KuroyaApp {
    pub(crate) fn drain_file_watcher(&mut self) -> usize {
        self.rebuild_watcher_when_due();
        self.ensure_parent_repository_git_watch();
        let watcher_root_stale = self.watcher.as_ref().is_some_and(|watcher| {
            !watcher_root_matches_workspace(watcher.root(), &self.workspace.root)
        });
        if watcher_root_stale {
            self.watcher = match FileWatcher::new(&self.workspace.root) {
                Ok(watcher) => Some(watcher),
                Err(_) => {
                    self.note_watcher_build_failure();
                    None
                }
            };
            return 0;
        }

        let Some(watcher) = self.watcher.as_ref() else {
            return 0;
        };
        let drain = watcher.drain();
        let mut changed = drain.paths;
        dedupe_watcher_paths(&mut changed);
        let mut open_buffer_paths_added = 0;
        if drain.overflowed {
            open_buffer_paths_added =
                include_open_buffer_paths(&mut changed, &self.buffers, &self.workspace.root);
        }
        let changed_count = changed.len() + usize::from(drain.overflowed);
        if changed.is_empty() && !drain.overflowed {
            return 0;
        }

        let index_path_filter = self
            .project_index_options()
            .path_filter(&self.workspace.root);
        let mut watched =
            classify_watched_paths_with_filter(&self.workspace.root, &changed, &index_path_filter);
        if watched.settings_changed || drain.overflowed {
            self.reload_settings();
        }
        if watched.tasks_changed || drain.overflowed {
            self.spawn_workspace_task_load();
        }
        if watched.plugins_changed || drain.overflowed {
            self.schedule_workspace_plugin_reload();
        }
        // Watched git directories that resolve outside the workspace root
        // (parent repositories, worktree-style `.git` files) never reach the
        // workspace-root classification above.
        if !watched.git_metadata_changed
            && self.watcher.as_ref().is_some_and(|watcher| {
                watcher.git_watch_dirs().iter().any(|git_dir| {
                    changed
                        .iter()
                        .any(|path| workspace_path_contains_lexically(git_dir, path))
                })
            })
        {
            watched.git_metadata_changed = true;
        }
        if watched.git_metadata_changed {
            self.spawn_git_auto_refresh();
        }

        let external_changes = external_change_paths(&changed);
        self.forward_external_changes_to_lsp_watchers(&external_changes);
        let reloads = reloadable_open_buffers_for_changes(&external_changes, &self.buffers);
        for (id, path) in reloads {
            self.spawn_reload_clean_buffer(id, path);
        }

        let dirty_changes = dirty_open_buffers_for_changes(&external_changes, &self.buffers);
        for (id, _) in &dirty_changes {
            self.mark_buffer_changed_on_disk(*id);
        }

        for path in &external_changes {
            self.invalidate_explorer_directory_for_path(path);
        }

        if watched.project_paths.is_empty() && !drain.overflowed {
            return changed_count;
        }

        if let Some(status) = file_watcher_status(
            drain.overflowed,
            open_buffer_paths_added,
            changed.len(),
            dirty_changes.len(),
        ) {
            self.set_status_with_toast(status);
        }
        if watched.workspace_refresh_needed || drain.overflowed {
            if watched.workspace_refresh_needed && !drain.overflowed {
                // Small known batches are applied incrementally by the
                // debounced flush; overflow falls back to a full re-walk.
                self.schedule_workspace_refresh_with_paths(std::mem::take(
                    &mut watched.project_paths,
                ));
            } else {
                self.schedule_workspace_refresh();
            }
        }
        changed_count
    }

    /// Parent repositories resolved above the workspace root keep their
    /// `.git` directory outside the watched tree; watch it so HEAD and index
    /// changes there still refresh git status.
    fn ensure_parent_repository_git_watch(&mut self) {
        let Some(git_root) = self.git.root().map(Path::to_path_buf) else {
            return;
        };
        if workspace_path_contains_lexically(&self.workspace.root, &git_root) {
            return;
        }
        if let Some(watcher) = self.watcher.as_mut() {
            watcher.ensure_git_dir_watched(&git_root);
        }
    }

    pub(crate) fn note_watcher_build_failure(&mut self) {
        let delay = watcher_rebuild_backoff(self.watcher_rebuild_attempts);
        self.watcher_rebuild_attempts = self.watcher_rebuild_attempts.saturating_add(1);
        self.watcher_rebuild_due = Some(Instant::now() + delay);
    }

    fn rebuild_watcher_when_due(&mut self) {
        if self.watcher.is_some() {
            if self.watcher_rebuild_due.is_some() || self.watcher_rebuild_attempts > 0 {
                self.clear_watcher_rebuild_retry();
            }
            return;
        }
        let Some(due) = self.watcher_rebuild_due else {
            return;
        };
        if Instant::now() < due {
            return;
        }
        self.clear_watcher_rebuild_retry();
        match FileWatcher::new(&self.workspace.root) {
            Ok(watcher) => self.watcher = Some(watcher),
            Err(_) => self.note_watcher_build_failure(),
        }
    }

    fn clear_watcher_rebuild_retry(&mut self) {
        self.watcher_rebuild_due = None;
        self.watcher_rebuild_attempts = 0;
    }
}

fn watcher_rebuild_backoff(attempts: u32) -> Duration {
    WATCHER_REBUILD_INITIAL_BACKOFF
        .checked_mul(1_u32 << attempts.min(WATCHER_REBUILD_MAX_DOUBLING_SHIFT))
        .unwrap_or(WATCHER_REBUILD_MAX_BACKOFF)
        .min(WATCHER_REBUILD_MAX_BACKOFF)
}

fn file_watcher_status(
    overflowed: bool,
    open_buffer_paths_added: usize,
    project_changes: usize,
    dirty_changes: usize,
) -> Option<String> {
    if overflowed {
        return Some(format!(
            "Filesystem watcher missed changes; refreshing workspace and checking {}",
            count_label(
                open_buffer_paths_added,
                "open buffer path",
                "open buffer paths"
            )
        ));
    }

    if dirty_changes == 0 {
        return None;
    }

    let changes = count_label(project_changes, "filesystem change", "filesystem changes");
    Some(format!(
        "{changes} detected; {} changed on disk",
        count_label(dirty_changes, "dirty buffer", "dirty buffers")
    ))
}

fn include_open_buffer_paths(
    changed: &mut Vec<PathBuf>,
    buffers: &[kuroya_core::TextBuffer],
    workspace_root: &Path,
) -> usize {
    let mut seen = HashSet::with_capacity(changed.len().saturating_add(buffers.len()));
    seen.extend(changed.iter().map(|path| watcher_path_key(path)));
    let mut added = 0;
    for path in buffers
        .iter()
        .filter_map(kuroya_core::TextBuffer::path)
        .filter(|path| workspace_path_stays_within_root_lexically(workspace_root, path))
    {
        let path = path.to_path_buf();
        if seen.insert(watcher_path_key(&path)) {
            changed.push(path);
            added += 1;
        }
    }
    added
}

fn dedupe_watcher_paths(changed: &mut Vec<PathBuf>) -> usize {
    let original_len = changed.len();
    // Dedupes by case-folded key; case-only rename spellings (`Foo.rs` +
    // `foo.rs`, identical keys, different raw paths) collapse into their
    // parent directory so the index rescan rebuilds the real casing instead
    // of dropping the renamed file.
    collapse_case_only_path_collisions(changed);
    original_len.saturating_sub(changed.len())
}

fn external_change_paths(changed: &[PathBuf]) -> Vec<PathBuf> {
    changed
        .iter()
        .filter(|path| !is_recent_app_write(path))
        .cloned()
        .collect()
}

fn watcher_root_matches_workspace(watcher_root: &Path, workspace_root: &Path) -> bool {
    trusted_workspace_paths_match(watcher_root, workspace_root)
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct WatcherPathKey {
    prefix: Option<String>,
    rooted: bool,
    components: Vec<String>,
}

fn watcher_path_key(path: &Path) -> WatcherPathKey {
    let mut key = WatcherPathKey {
        prefix: None,
        rooted: false,
        components: Vec::new(),
    };

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                key.prefix = Some(watcher_path_component_key(prefix.as_os_str()));
            }
            Component::RootDir => {
                key.rooted = true;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if key
                    .components
                    .last()
                    .is_some_and(|component| component != "..")
                {
                    key.components.pop();
                } else if !key.rooted {
                    key.components.push("..".to_owned());
                }
            }
            Component::Normal(component) => {
                key.components.push(watcher_path_component_key(component))
            }
        }
    }

    key
}

fn watcher_path_component_key(component: &OsStr) -> String {
    let component = component.to_string_lossy();
    #[cfg(windows)]
    {
        if component.is_ascii() {
            let mut component = component.into_owned();
            component.make_ascii_lowercase();
            component
        } else {
            component.to_lowercase()
        }
    }
    #[cfg(not(windows))]
    {
        component.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        dedupe_watcher_paths, external_change_paths, file_watcher_status,
        include_open_buffer_paths, watcher_rebuild_backoff, watcher_root_matches_workspace,
    };
    use crate::{
        KuroyaApp,
        app_startup_context::AppStartupContext,
        fs_watcher::note_app_write,
        terminal::TerminalPane,
        workspace_state::{classify_watched_paths, dirty_open_buffers_for_changes},
    };
    use kuroya_core::{EditorSettings, TextBuffer, Workspace};
    use std::{
        path::{Path, PathBuf},
        time::{Duration, Instant},
    };
    use tokio::runtime::Runtime;

    #[test]
    fn recent_app_write_events_are_not_classified_as_external_changes() {
        let saved = PathBuf::from(format!(
            "kuroya-watcher-self-write/a/unique-{}-{:?}/src/main.rs",
            std::process::id(),
            std::thread::current().id()
        ));
        note_app_write(&saved);

        assert!(external_change_paths(std::slice::from_ref(&saved)).is_empty());
    }

    #[test]
    fn recent_app_settings_write_is_not_classified_as_settings_changed() {
        let root = temp_workspace("settings-self-write");
        std::fs::create_dir_all(&root).unwrap();
        let settings = crate::workspace_state::settings_path(&root);

        assert!(
            classify_watched_paths(&root, std::slice::from_ref(&settings)).settings_changed,
            "external settings writes should still classify as settings_changed"
        );

        note_app_write(&settings);

        assert!(
            !classify_watched_paths(&root, std::slice::from_ref(&settings)).settings_changed,
            "the app's own settings write must not trigger a settings reload"
        );
        drop(std::fs::remove_dir_all(root));
    }

    #[test]
    fn recent_app_write_events_do_not_flag_open_buffers_changed_on_disk() {
        let path = PathBuf::from("kuroya-watcher-self-write/b/src/main.rs");
        let mut saved_buffer = TextBuffer::from_text(1, Some(path.clone()), "dirty".to_owned());
        saved_buffer.mark_dirty();
        let other_path = PathBuf::from("kuroya-watcher-self-write/b/src/lib.rs");
        let mut other_buffer =
            TextBuffer::from_text(2, Some(other_path.clone()), "other".to_owned());
        other_buffer.mark_dirty();
        let buffers = vec![saved_buffer, other_buffer];

        note_app_write(&path);
        let saved_events = vec![PathBuf::from("kuroya-watcher-self-write/b/src/./main.rs")];
        assert!(
            dirty_open_buffers_for_changes(&external_change_paths(&saved_events), &buffers)
                .is_empty()
        );

        let other_events = vec![other_path.clone()];
        assert_eq!(
            dirty_open_buffers_for_changes(&external_change_paths(&other_events), &buffers),
            vec![(2, other_path)]
        );
    }

    #[test]
    fn watcher_rebuild_backoff_doubles_and_caps_at_thirty_seconds() {
        assert_eq!(watcher_rebuild_backoff(0), Duration::from_secs(1));
        assert_eq!(watcher_rebuild_backoff(1), Duration::from_secs(2));
        assert_eq!(watcher_rebuild_backoff(3), Duration::from_secs(8));
        assert_eq!(watcher_rebuild_backoff(4), Duration::from_secs(16));
        assert_eq!(watcher_rebuild_backoff(5), Duration::from_secs(30));
        assert_eq!(watcher_rebuild_backoff(u32::MAX), Duration::from_secs(30));
    }

    #[test]
    fn watcher_rebuild_retry_attempts_build_against_tempdir_and_clears_on_success() {
        let root = temp_workspace("rebuild-success");
        std::fs::create_dir_all(&root).unwrap();
        let mut app = app_for_test(root.clone());
        app.watcher = None;
        app.note_watcher_build_failure();

        assert_eq!(app.watcher_rebuild_attempts, 1);
        assert!(app.watcher_rebuild_due.unwrap() > Instant::now());

        app.watcher_rebuild_due = Some(Instant::now() - Duration::from_secs(1));
        assert_eq!(app.drain_file_watcher(), 0);

        assert!(app.watcher.is_some());
        assert_eq!(app.watcher_rebuild_due, None);
        assert_eq!(app.watcher_rebuild_attempts, 0);
        drop(app);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn watcher_rebuild_failure_reschedules_with_doubled_backoff() {
        let root = temp_workspace("rebuild-missing-root");
        let mut app = app_for_test(root.clone());
        app.watcher = None;
        app.watcher_rebuild_attempts = 2;
        app.note_watcher_build_failure();

        assert_eq!(app.watcher_rebuild_attempts, 3);
        let remaining = app.watcher_rebuild_due.unwrap() - Instant::now();
        assert!(remaining > Duration::from_secs(3), "{remaining:?}");
        assert!(remaining <= Duration::from_secs(4), "{remaining:?}");
    }

    #[test]
    fn watcher_with_active_watcher_clears_stale_retry_state() {
        let root = temp_workspace("rebuild-clears");
        std::fs::create_dir_all(&root).unwrap();
        let mut app = app_for_test(root.clone());
        app.watcher_rebuild_due = Some(Instant::now() - Duration::from_secs(10));
        app.watcher_rebuild_attempts = 4;
        app.drain_file_watcher();

        assert!(app.watcher.is_some());
        assert_eq!(app.watcher_rebuild_due, None);
        assert_eq!(app.watcher_rebuild_attempts, 0);
        drop(app);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn overflow_open_buffer_paths_are_deduplicated() {
        let workspace = Path::new("workspace");
        let main = PathBuf::from("workspace/src/main.rs");
        let lib = PathBuf::from("workspace/src/lib.rs");
        let mut changed = vec![main.clone()];
        let buffers = vec![
            TextBuffer::from_text(1, Some(main.clone()), "main".to_owned()),
            TextBuffer::from_text(2, Some(lib.clone()), "lib".to_owned()),
            TextBuffer::from_text(3, Some(lib.clone()), "duplicate".to_owned()),
            TextBuffer::new_untitled(4),
        ];

        let added = include_open_buffer_paths(&mut changed, &buffers, workspace);

        assert_eq!(added, 1);
        assert_eq!(changed, vec![main, lib]);
    }

    #[test]
    fn overflow_open_buffer_paths_deduplicate_lexically_equivalent_paths() {
        let workspace = Path::new("workspace");
        let raw_main = PathBuf::from("workspace/src/./main.rs");
        let open_main = PathBuf::from("workspace/src/main.rs");
        let mut changed = vec![raw_main.clone()];
        let buffers = vec![
            TextBuffer::from_text(1, Some(open_main), "main".to_owned()),
            TextBuffer::from_text(
                2,
                Some(PathBuf::from("workspace/src/generated/../main.rs")),
                "same".to_owned(),
            ),
        ];

        let added = include_open_buffer_paths(&mut changed, &buffers, workspace);

        assert_eq!(added, 0);
        assert_eq!(changed, vec![raw_main]);
    }

    #[test]
    fn watcher_paths_are_deduplicated_before_overflow_processing() {
        let raw_main = PathBuf::from("workspace/src/./main.rs");
        let equivalent_main = PathBuf::from("workspace/src/generated/../main.rs");
        let lib = PathBuf::from("workspace/src/lib.rs");
        let mut changed = vec![
            raw_main.clone(),
            equivalent_main,
            lib.clone(),
            PathBuf::from("workspace/src/./lib.rs"),
        ];

        assert_eq!(dedupe_watcher_paths(&mut changed), 2);
        assert_eq!(changed, vec![raw_main, lib]);
    }

    #[cfg(windows)]
    #[test]
    fn watcher_case_only_renames_dedupe_to_the_parent_directory() {
        let mut changed = vec![
            PathBuf::from("workspace/src/Foo.rs"),
            PathBuf::from("workspace/src/foo.rs"),
        ];

        assert_eq!(dedupe_watcher_paths(&mut changed), 1);
        assert_eq!(changed, vec![PathBuf::from("workspace/src")]);
    }

    #[test]
    fn overflow_open_buffer_paths_skip_buffers_outside_workspace() {
        let workspace = Path::new("workspace");
        let main = PathBuf::from("workspace/src/main.rs");
        let outside = PathBuf::from("other/src/lib.rs");
        let mut changed = Vec::new();
        let buffers = vec![
            TextBuffer::from_text(1, Some(main.clone()), "main".to_owned()),
            TextBuffer::from_text(2, Some(outside), "outside".to_owned()),
        ];

        let added = include_open_buffer_paths(&mut changed, &buffers, workspace);

        assert_eq!(added, 1);
        assert_eq!(changed, vec![main]);
    }

    #[test]
    fn overflow_open_buffer_paths_skip_parent_reentry_paths() {
        let workspace = Path::new("workspace/current");
        let reentry = PathBuf::from("workspace/current/../current/src/main.rs");
        let mut changed = Vec::new();
        let buffers = vec![TextBuffer::from_text(1, Some(reentry), "main".to_owned())];

        let added = include_open_buffer_paths(&mut changed, &buffers, workspace);

        assert_eq!(added, 0);
        assert!(changed.is_empty());
    }

    #[test]
    fn watcher_root_match_rejects_stale_parent_or_child_roots() {
        assert!(watcher_root_matches_workspace(
            Path::new("workspace/src/.."),
            Path::new("workspace")
        ));
        assert!(!watcher_root_matches_workspace(
            Path::new("workspace/old"),
            Path::new("workspace")
        ));
        assert!(!watcher_root_matches_workspace(
            Path::new("workspace"),
            Path::new("workspace/current")
        ));
    }

    #[test]
    fn file_watcher_status_uses_count_labels() {
        assert_eq!(file_watcher_status(false, 0, 1, 0), None);
        assert_eq!(file_watcher_status(false, 0, 2, 0), None);
        assert_eq!(
            file_watcher_status(false, 0, 1, 1),
            Some("1 filesystem change detected; 1 dirty buffer changed on disk".to_owned())
        );
        assert_eq!(
            file_watcher_status(false, 0, 2, 2),
            Some("2 filesystem changes detected; 2 dirty buffers changed on disk".to_owned())
        );
        assert_eq!(
            file_watcher_status(true, 1, 0, 0),
            Some(
                "Filesystem watcher missed changes; refreshing workspace and checking 1 open buffer path"
                    .to_owned()
            )
        );
        assert_eq!(
            file_watcher_status(true, 2, 0, 0),
            Some(
                "Filesystem watcher missed changes; refreshing workspace and checking 2 open buffer paths"
                    .to_owned()
            )
        );
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

    fn temp_workspace(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "kuroya-runtime-watcher-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ))
    }
}
