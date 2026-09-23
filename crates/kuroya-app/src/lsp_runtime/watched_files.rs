use crate::{
    KuroyaApp, lsp_runtime::lsp_command_queue_failed_status,
    workspace_state::app_owned_state_dirs_in_workspace,
    workspace_trust::workspace_path_contains_lexically,
};
use kuroya_core::{WatchedFileChange, lsp::path_to_file_uri};
use std::path::{Path, PathBuf};

/// Upper bound on filesystem events batched into a single
/// `workspace/didChangeWatchedFiles` notification per client per frame.
/// Excess events are dropped for that frame; servers re-stat on the next
/// event, and watcher overflows already trigger a workspace refresh.
pub(crate) const LSP_WATCHED_FILES_MAX_EVENTS_PER_FRAME: usize = 256;

impl KuroyaApp {
    /// Forwards this frame's external filesystem changes to every live LSP
    /// client that registered a matching `workspace/didChangeWatchedFiles`
    /// watcher, one batched notification per interested client.
    ///
    /// The watcher drain carries bare paths (change kinds are lost at the
    /// watcher channel), so every event is reported as `Changed` (type 2):
    /// servers re-stat the path, which resolves created and deleted files as
    /// well. Events under `.git` or app-owned state directories never reach
    /// the servers.
    pub(crate) fn forward_external_changes_to_lsp_watchers(&mut self, changed: &[PathBuf]) {
        if self.lsp_clients.is_empty() || changed.is_empty() {
            return;
        }
        let (events, dropped) = lsp_watched_file_events(changed, &self.workspace.root);
        if events.is_empty() {
            return;
        }

        let clients: Vec<_> = self.lsp_clients.values().cloned().collect();
        for client in clients {
            let watched_files = client.watched_files();
            if watched_files.is_empty() {
                continue;
            }
            let batch: Vec<WatchedFileChange> = events
                .iter()
                .filter(|(path, _)| watched_files.matches_any(path))
                .map(|(_, change)| change.clone())
                .collect();
            if batch.is_empty() {
                continue;
            }
            let trace_detail = watched_files_trace_detail(batch.len(), dropped);
            if client.did_change_watched_files(batch) {
                self.record_lsp_client_trace("workspace/didChangeWatchedFiles", trace_detail);
            } else {
                self.status = lsp_command_queue_failed_status("workspace/didChangeWatchedFiles");
            }
        }
    }
}

/// Turns drained watcher paths into batched didChangeWatchedFiles events:
/// `.git` internals, app-owned state directories, and directories are
/// skipped, and the batch is capped per frame. Returns the kept
/// `(path, change)` pairs plus the number of dropped events.
fn lsp_watched_file_events(
    changed: &[PathBuf],
    workspace_root: &Path,
) -> (Vec<(PathBuf, WatchedFileChange)>, usize) {
    let app_state_dirs = app_owned_state_dirs_in_workspace(workspace_root);
    filter_watched_file_events(changed, &app_state_dirs)
}

fn filter_watched_file_events(
    changed: &[PathBuf],
    app_state_dirs: &[PathBuf],
) -> (Vec<(PathBuf, WatchedFileChange)>, usize) {
    let mut events = Vec::with_capacity(changed.len());
    let mut dropped = 0usize;
    for path in changed {
        if path_is_git_internal(path)
            || app_state_dirs
                .iter()
                .any(|dir| workspace_path_contains_lexically(dir, path))
            || path.is_dir()
        {
            continue;
        }
        if events.len() >= LSP_WATCHED_FILES_MAX_EVENTS_PER_FRAME {
            dropped = dropped.saturating_add(1);
            continue;
        }
        events.push((
            path.clone(),
            WatchedFileChange {
                uri: path_to_file_uri(path),
                kind: WatchedFileChange::CHANGED,
            },
        ));
    }
    (events, dropped)
}

fn path_is_git_internal(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == ".git")
}

fn watched_files_trace_detail(sent: usize, dropped: usize) -> String {
    if dropped == 0 {
        format!("{sent} path(s)")
    } else {
        format!("{sent} path(s); {dropped} dropped")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LSP_WATCHED_FILES_MAX_EVENTS_PER_FRAME, filter_watched_file_events,
        lsp_watched_file_events, path_is_git_internal,
    };
    use crate::{
        KuroyaApp, app_startup_context::AppStartupContext, lsp_client::LspClientHandle,
        terminal::TerminalPane,
    };
    use kuroya_core::{EditorSettings, WatchedFileChange, Workspace};
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{Instant, SystemTime, UNIX_EPOCH},
    };
    use tokio::{runtime::Runtime, sync::mpsc};

    #[test]
    fn watched_file_events_skip_git_directories_and_app_state_dirs() {
        let workspace = temp_root("event-filters");
        let source = workspace.join("src/main.rs");
        let assets = workspace.join("src/assets");
        let app_state = workspace.join(".kuroya");
        fs::create_dir_all(&assets).unwrap();

        let paths = vec![
            workspace.join(".git/index"),
            source.clone(),
            assets,
            app_state.join("session.json"),
        ];

        let (events, dropped) =
            filter_watched_file_events(&paths, std::slice::from_ref(&app_state));

        assert_eq!(dropped, 0);
        assert_eq!(events.len(), 1, "only the regular file survives");
        assert_eq!(events[0].0, source);
        assert_eq!(events[0].1.kind, WatchedFileChange::CHANGED);
        assert!(
            events[0].1.uri.starts_with("file://"),
            "expected file URI, got {}",
            events[0].1.uri
        );

        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn watched_file_events_cap_the_batch_per_frame() {
        let workspace = temp_root("event-cap");
        let flood: Vec<PathBuf> = (0..(LSP_WATCHED_FILES_MAX_EVENTS_PER_FRAME + 3))
            .map(|index| workspace.join(format!("src/file-{index}.rs")))
            .collect();

        let (events, dropped) = lsp_watched_file_events(&flood, &workspace);

        assert_eq!(events.len(), LSP_WATCHED_FILES_MAX_EVENTS_PER_FRAME);
        assert_eq!(dropped, 3);

        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn git_internal_paths_are_detected_by_component() {
        assert!(path_is_git_internal(Path::new("workspace/.git/index")));
        assert!(path_is_git_internal(Path::new(".git")));
        assert!(!path_is_git_internal(Path::new(
            "workspace/.github/workflows"
        )));
        assert!(!path_is_git_internal(Path::new("workspace/src/gitignore")));
    }

    #[test]
    fn registered_watchers_receive_batched_did_change_watched_files_commands() {
        let root = temp_root("watched-files-forwarding");
        fs::create_dir_all(&root).unwrap();
        let mut app = app_for_test(root.clone());

        let (tx, mut rx) = mpsc::channel(4);
        let handle = LspClientHandle::from_sender_for_test(tx, 1);
        handle
            .watched_files()
            .register("watcher-rs", vec!["**/*.rs".to_owned()]);
        app.lsp_clients.insert("rust".to_owned(), handle);

        let source = root.join("src/main.rs");
        app.forward_external_changes_to_lsp_watchers(std::slice::from_ref(&source));

        let command = rx.try_recv().expect("forwarded watched-files command");
        match command {
            crate::lsp_client::LspClientCommand::DidChangeWatchedFiles { changes } => {
                assert_eq!(changes.len(), 1);
                assert_eq!(changes[0].kind, WatchedFileChange::CHANGED);
                assert!(
                    changes[0].uri.ends_with("/src/main.rs"),
                    "expected forwarded URI, got {}",
                    changes[0].uri
                );
            }
            other => panic!("expected didChangeWatchedFiles command, got {other:?}"),
        }
        assert!(rx.try_recv().is_err());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unregistered_watchers_receive_no_did_change_watched_files_commands() {
        let root = temp_root("watched-files-unregistered");
        fs::create_dir_all(&root).unwrap();
        let mut app = app_for_test(root.clone());

        let (tx, mut rx) = mpsc::channel(4);
        let handle = LspClientHandle::from_sender_for_test(tx, 1);
        handle
            .watched_files()
            .register("watcher-rs", vec!["**/*.rs".to_owned()]);
        handle
            .watched_files()
            .unregister(&["watcher-rs".to_owned()]);
        app.lsp_clients.insert("rust".to_owned(), handle);

        let source = root.join("src/main.rs");
        app.forward_external_changes_to_lsp_watchers(std::slice::from_ref(&source));

        assert!(rx.try_recv().is_err(), "no command after unregistration");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn clients_without_registrations_receive_no_watched_files_commands() {
        let root = temp_root("watched-files-no-registrations");
        fs::create_dir_all(&root).unwrap();
        let mut app = app_for_test(root.clone());

        let (tx, mut rx) = mpsc::channel(4);
        app.lsp_clients.insert(
            "rust".to_owned(),
            LspClientHandle::from_sender_for_test(tx, 1),
        );

        let source = root.join("src/main.rs");
        app.forward_external_changes_to_lsp_watchers(std::slice::from_ref(&source));

        assert!(rx.try_recv().is_err());

        let _ = fs::remove_dir_all(root);
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

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!(
            "kuroya-lsp-watched-files-{name}-{}-{nanos}",
            std::process::id()
        ))
    }
}
