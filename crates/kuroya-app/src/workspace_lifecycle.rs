use crate::{
    KuroyaApp,
    fs_watcher::FileWatcher,
    path_display::{
        DISPLAY_PATH_LABEL_MAX_CHARS, display_error_label_cow, display_path_label_cow,
        sanitized_display_label_cow,
    },
    persistence::PersistedSession,
    preferences::load_app_settings,
    transient_state::PendingWorkspaceSwitch,
    workspace_state::paths_match_lexically,
    workspace_trust::workspace_is_trusted,
};
use kuroya_core::{BufferId, TextBuffer, Workspace};
use std::{
    collections::HashSet,
    fmt::Display,
    path::{Path, PathBuf},
};

impl KuroyaApp {
    pub(crate) fn request_open_workspace(&mut self, path: PathBuf) {
        if self.exit_confirmed || self.pending_exit.is_some() {
            self.pending_workspace_switch = None;
            self.status = workspace_switch_blocked_by_exit_status();
            return;
        }
        if paths_match_lexically(&path, &self.workspace.root) {
            self.pending_workspace_switch = None;
            self.status = already_in_workspace_status(&path);
            return;
        }
        if !path.is_dir() {
            self.pending_workspace_switch = None;
            self.status = workspace_path_not_folder_status(&path);
            return;
        }
        if is_home_directory(
            &path,
            crate::app_startup_context::home_dir_from_env().as_deref(),
        ) {
            self.pending_workspace_switch =
                Some(PendingWorkspaceSwitch::ConfirmHomeDirectory { target: path });
            self.command_palette = false;
            self.open_workspace_open = false;
            self.status = home_directory_open_needs_confirmation_status();
            return;
        }

        let dirty_count = workspace_switch_dirty_buffer_count(&self.buffers);
        if dirty_count != 0 {
            self.pending_workspace_switch = Some(PendingWorkspaceSwitch::Confirm { target: path });
            self.command_palette = false;
            self.open_workspace_open = false;
            self.status = workspace_switch_unsaved_status(dirty_count);
            return;
        }

        self.open_workspace_now(path);
    }

    pub(crate) fn open_workspace_now(&mut self, path: PathBuf) {
        self.pending_workspace_switch = None;
        if self.exit_confirmed || self.pending_exit.is_some() {
            self.status = workspace_switch_blocked_by_exit_status();
            return;
        }
        if paths_match_lexically(&path, &self.workspace.root) {
            self.status = already_in_workspace_status(&path);
            return;
        }
        if !path.is_dir() {
            self.status = workspace_path_not_folder_status(&path);
            return;
        }

        let mut warnings: Vec<String> = Vec::new();
        let watcher = match FileWatcher::new(&path) {
            Ok(watcher) => Some(watcher),
            Err(error) => {
                warnings.push(workspace_watcher_failure_warning(error));
                self.watcher_rebuild_attempts = 0;
                self.note_watcher_build_failure();
                None
            }
        };
        let session = match PersistedSession::load(&path) {
            Ok(Some(session)) => Some(session),
            Ok(None) => None,
            Err(error) => {
                warnings.push(workspace_session_load_warning(error));
                None
            }
        };
        if let Err(error) = load_app_settings(&path) {
            warnings.push(workspace_settings_load_warning(error));
        }
        let workspace = Workspace::new(path);

        let previous_workspace_placeholder = self.workspace_placeholder;
        if !previous_workspace_placeholder {
            self.request_session_save(
                self.workspace.root.clone(),
                self.build_session_save_snapshot(),
            );
        }
        self.notify_lsp_close_all();
        for client in self.lsp_clients.values() {
            client.shutdown();
        }
        self.reset_workspace_lsp_clients();

        self.workspace = workspace;
        self.workspace_placeholder = false;
        self.workspace_trusted =
            workspace_is_trusted(&self.trusted_workspaces, &self.workspace.root);
        self.record_recent_project(self.workspace.root.clone());
        self.watcher = watcher;
        warnings.extend(self.reset_open_workspace_state());
        self.reload_settings();
        if let Some(session) = session {
            self.restore_session(session);
        }
        if let Err(error) = self.save_app_state() {
            warnings.push(recent_projects_save_failure_status(error));
        }
        self.spawn_index();
        self.spawn_git_scan();
        self.spawn_workspace_task_load();
        self.spawn_plugin_discovery();
        self.arm_workspace_trust_prompt();

        if !warnings.is_empty() {
            self.status = warnings.join("; ");
        }
    }
}

pub(crate) fn already_in_workspace_status(path: &Path) -> String {
    format!("Already in {}", display_path_label_cow(path))
}

pub(crate) fn is_home_directory(path: &Path, home: Option<&Path>) -> bool {
    home.is_some_and(|home| paths_match_lexically(path, home))
}

pub(crate) fn home_directory_open_needs_confirmation_status() -> String {
    "Confirm opening your home directory".to_owned()
}

pub(crate) fn workspace_path_not_folder_status(path: &Path) -> String {
    let display_path = path.display().to_string();
    let label = sanitized_display_label_cow(&display_path, DISPLAY_PATH_LABEL_MAX_CHARS, ".");
    format!("Workspace path is not a folder: {label}")
}

pub(crate) fn workspace_switch_blocked_by_exit_status() -> String {
    "Workspace switch canceled; exit is in progress".to_owned()
}

fn recent_projects_save_failure_status(error: impl Display) -> String {
    let error = error.to_string();
    format!(
        "Could not save recent projects: {}",
        display_error_label_cow(&error)
    )
}

fn workspace_watcher_failure_warning(error: impl Display) -> String {
    let error = error.to_string();
    format!(
        "Could not watch workspace files: {}",
        display_error_label_cow(&error)
    )
}

fn workspace_session_load_warning(error: impl Display) -> String {
    let error = error.to_string();
    format!(
        "Could not load saved session: {}",
        display_error_label_cow(&error)
    )
}

fn workspace_settings_load_warning(error: impl Display) -> String {
    let error = error.to_string();
    format!(
        "Could not load workspace settings: {}",
        display_error_label_cow(&error)
    )
}

fn workspace_switch_unsaved_status(dirty_count: usize) -> String {
    let noun = if dirty_count == 1 {
        "unsaved file"
    } else {
        "unsaved files"
    };
    format!("{dirty_count} {noun} before switching workspace")
}

fn workspace_switch_dirty_buffer_count(buffers: &[TextBuffer]) -> usize {
    let mut dirty_ids: HashSet<BufferId> = HashSet::new();
    buffers
        .iter()
        .filter(|buffer| buffer.is_dirty() && dirty_ids.insert(buffer.id()))
        .count()
}

#[cfg(test)]
mod tests {
    use super::{
        already_in_workspace_status, home_directory_open_needs_confirmation_status,
        is_home_directory, recent_projects_save_failure_status, workspace_path_not_folder_status,
        workspace_switch_dirty_buffer_count, workspace_switch_unsaved_status,
    };
    use crate::{
        KuroyaApp,
        app_startup_context::AppStartupContext,
        path_display::{
            DISPLAY_ERROR_LABEL_MAX_CHARS, DISPLAY_PATH_LABEL_MAX_CHARS, sanitized_display_label,
            sanitized_display_label_cow,
        },
        terminal::TerminalPane,
        transient_state::{PendingExit, PendingWorkspaceSwitch},
        workspace_state::settings_path,
    };
    use kuroya_core::{EditorSettings, TextBuffer, Workspace};
    use std::{
        borrow::Cow,
        fs,
        path::{Path, PathBuf},
        time::{Instant, SystemTime, UNIX_EPOCH},
    };
    use tokio::runtime::Runtime;

    #[test]
    fn is_home_directory_matches_only_exact_home_root() {
        let home = if cfg!(windows) {
            Path::new(r"C:\Users\kuroya")
        } else {
            Path::new("/home/kuroya")
        };

        assert!(is_home_directory(home, Some(home)));
        assert!(is_home_directory(
            &home.join("child").join(".."),
            Some(home)
        ));
        assert!(is_home_directory(&home.join("."), Some(home)));
        assert!(!is_home_directory(&home.join("child"), Some(home)));
        assert!(!is_home_directory(
            home.parent().unwrap_or(home),
            Some(home)
        ));
        assert!(!is_home_directory(home, None));
        assert!(!is_home_directory(Path::new(""), Some(home)));
    }

    #[cfg(windows)]
    #[test]
    fn is_home_directory_matches_case_insensitively_on_windows() {
        assert!(is_home_directory(
            Path::new(r"c:\USERS\KUROYA"),
            Some(Path::new(r"C:\Users\kuroya"))
        ));
    }

    #[test]
    fn home_directory_open_status_asks_for_confirmation() {
        assert_eq!(
            home_directory_open_needs_confirmation_status(),
            "Confirm opening your home directory"
        );
    }

    #[test]
    fn already_in_workspace_status_sanitizes_and_bounds_compact_path() {
        let path = Path::new("workspace").join(format!(
            "project\n{}\u{202e}tail",
            "very-long-component-".repeat(16)
        ));

        let status = already_in_workspace_status(&path);

        assert!(status.starts_with("Already in project "));
        assert!(!status.contains('\n'));
        assert!(!status.contains('\u{202e}'));
        assert!(status.contains("..."));
        assert!(
            status.chars().count() <= "Already in ".chars().count() + DISPLAY_PATH_LABEL_MAX_CHARS
        );
    }

    #[test]
    fn request_open_workspace_treats_equivalent_root_as_current() {
        let root = temp_workspace("equivalent-current");
        fs::create_dir_all(root.join("src")).unwrap();
        let equivalent_root = root.join("src").join("..");
        let mut app = app_for_test(root.clone());

        app.request_open_workspace(equivalent_root);

        assert_eq!(app.workspace.root, root);
        assert!(app.pending_workspace_switch.is_none());
        assert!(app.status.starts_with("Already in "), "{}", app.status);
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn request_open_workspace_treats_verbatim_root_as_current() {
        let root = temp_workspace("verbatim-current");
        fs::create_dir_all(&root).unwrap();
        let verbatim_root = PathBuf::from(format!(r"\\?\{}", root.display()));
        let mut app = app_for_test(root.clone());

        app.request_open_workspace(verbatim_root);

        assert_eq!(app.workspace.root, root);
        assert!(app.pending_workspace_switch.is_none());
        assert!(app.status.starts_with("Already in "), "{}", app.status);
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn request_open_workspace_does_not_prompt_dirty_buffers_for_equivalent_root() {
        let root = temp_workspace("equivalent-current-dirty");
        fs::create_dir_all(root.join("src")).unwrap();
        let equivalent_root = root.join("src").join("..");
        let mut app = app_for_test(root.clone());
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(root.join("src/main.rs")),
            "fn main() {}\n".to_owned(),
        ));
        app.buffer_mut(7).unwrap().insert_at_cursor("// dirty\n");

        app.request_open_workspace(equivalent_root);

        assert_eq!(app.workspace.root, root);
        assert!(!matches!(
            app.pending_workspace_switch,
            Some(PendingWorkspaceSwitch::Confirm { .. })
        ));
        assert!(app.status.starts_with("Already in "), "{}", app.status);
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn request_open_workspace_preserves_raw_clean_target_path() {
        let root = temp_workspace("raw-clean-root");
        let target = temp_workspace("raw-clean-target");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(target.join("child")).unwrap();
        let raw_target = target.join("child").join("..");
        let mut app = app_for_test(root.clone());

        app.request_open_workspace(raw_target.clone());

        assert_eq!(app.workspace.root, raw_target);
        assert!(app.pending_workspace_switch.is_none());
        drop(app);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn open_workspace_now_clears_placeholder_without_saving_placeholder_session() {
        let root = temp_workspace("placeholder-root");
        let target = temp_workspace("placeholder-target");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&target).unwrap();
        let mut app = app_for_test(root.clone());
        app.workspace_placeholder = true;

        app.open_workspace_now(target.clone());

        assert!(!app.workspace_placeholder);
        assert_eq!(app.workspace.root, target);
        assert!(app.session_save_in_flight.is_none());
        assert!(!app.queued_session_saves.contains_key(&root));
        drop(app);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn open_workspace_now_does_not_create_missing_app_settings() {
        let root = temp_workspace("no-settings-open-root");
        let target = temp_workspace("no-settings-open-target");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&target).unwrap();
        let mut app = app_for_test(root.clone());
        app.app_state_path_override = Some(root.join("app-state.json"));
        app.trusted_workspaces.push(target.clone());

        app.open_workspace_now(target.clone());

        assert_eq!(app.workspace.root, target);
        assert!(app.workspace_trusted);
        assert!(!settings_path(&app.workspace.root).exists());
        drop(app);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn open_workspace_now_arms_trust_prompt_only_for_untrusted_workspace() {
        let root = temp_workspace("trust-arm-root");
        let target = temp_workspace("trust-arm-target");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&target).unwrap();
        let mut app = app_for_test(root.clone());

        app.open_workspace_now(target.clone());

        assert_eq!(app.workspace.root, target);
        assert!(!app.workspace_trusted);
        assert_eq!(app.pending_workspace_trust_prompt, Some(target.clone()));

        app.trust_current_workspace();
        assert!(app.pending_workspace_trust_prompt.is_none());

        app.request_open_workspace(root.clone());

        assert_eq!(app.workspace.root, root);
        assert!(app.workspace_trusted);
        assert!(app.pending_workspace_trust_prompt.is_none());
        drop(app);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn open_workspace_now_does_not_arm_trust_prompt_for_trusted_workspace() {
        let root = temp_workspace("trust-arm-trusted-root");
        let target = temp_workspace("trust-arm-trusted-target");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&target).unwrap();
        let mut app = app_for_test(root.clone());
        app.trusted_workspaces.push(target.clone());

        app.open_workspace_now(target.clone());

        assert_eq!(app.workspace.root, target);
        assert!(app.workspace_trusted);
        assert!(app.pending_workspace_trust_prompt.is_none());
        drop(app);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn open_workspace_now_preflight_failure_leaves_current_state_untouched() {
        let root = temp_workspace("atomic-preflight-root");
        fs::create_dir_all(&root).unwrap();
        let mut app = app_for_test(root.clone());
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(root.join("src/main.rs")),
            "fn main() {}\n".to_owned(),
        ));
        let generation_before = app.workspace_event_generation;

        app.open_workspace_now(root.join("missing-target"));

        assert_eq!(app.workspace.root, root);
        assert_eq!(app.workspace_event_generation, generation_before);
        assert_eq!(app.buffers.len(), 1);
        assert!(app.status.starts_with("Workspace path is not a folder: "));
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn request_open_workspace_preserves_raw_dirty_pending_target_path() {
        let root = temp_workspace("raw-dirty-root");
        let target = temp_workspace("raw-dirty-target");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(target.join("child")).unwrap();
        let raw_target = target.join("child").join("..");
        let mut app = app_for_test(root.clone());
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(root.join("src/main.rs")),
            "fn main() {}\n".to_owned(),
        ));
        app.buffer_mut(7).unwrap().insert_at_cursor("// dirty\n");

        app.request_open_workspace(raw_target.clone());

        assert!(matches!(
            app.pending_workspace_switch,
            Some(PendingWorkspaceSwitch::Confirm { target: ref actual }) if actual == &raw_target
        ));
        assert_eq!(app.workspace.root, root);
        drop(app);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn open_workspace_now_rejects_stale_target_and_cleans_pending_switch() {
        let root = temp_workspace("stale-open-root");
        fs::create_dir_all(&root).unwrap();
        let stale_target = root.join(format!(
            "missing\n{}\u{202e}tail",
            "very-long-component-".repeat(16)
        ));
        let mut app = app_for_test(root.clone());
        app.pending_workspace_switch = Some(PendingWorkspaceSwitch::Confirm {
            target: stale_target.clone(),
        });

        app.open_workspace_now(stale_target);

        assert_eq!(app.workspace.root, root);
        assert!(app.pending_workspace_switch.is_none());
        assert!(app.status.starts_with("Workspace path is not a folder: "));
        assert!(!app.status.contains('\n'));
        assert!(!app.status.contains('\u{202e}'));
        assert!(
            app.status.chars().count()
                <= "Workspace path is not a folder: ".chars().count()
                    + DISPLAY_PATH_LABEL_MAX_CHARS
        );
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn request_open_workspace_does_not_route_while_exit_is_pending() {
        let root = temp_workspace("exit-block-root");
        let target = temp_workspace("exit-block-target");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&target).unwrap();
        let mut app = app_for_test(root.clone());
        app.pending_exit = Some(PendingExit::Confirm);

        app.request_open_workspace(target.clone());

        assert_eq!(app.workspace.root, root);
        assert!(app.pending_workspace_switch.is_none());
        assert_eq!(app.status, "Workspace switch canceled; exit is in progress");
        drop(app);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn request_open_workspace_clears_pending_switch_when_exit_blocks() {
        let root = temp_workspace("exit-block-clears-root");
        let target = temp_workspace("exit-block-clears-target");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&target).unwrap();
        let mut app = app_for_test(root.clone());
        app.pending_workspace_switch = Some(PendingWorkspaceSwitch::Confirm {
            target: target.clone(),
        });
        app.pending_exit = Some(PendingExit::Confirm);

        app.request_open_workspace(target.clone());

        assert_eq!(app.workspace.root, root);
        assert!(app.pending_workspace_switch.is_none());
        assert_eq!(app.status, "Workspace switch canceled; exit is in progress");
        drop(app);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn request_open_workspace_clears_pending_switch_when_target_is_invalid() {
        let root = temp_workspace("invalid-block-clears-root");
        let pending_target = temp_workspace("invalid-block-pending-target");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&pending_target).unwrap();
        let missing_target = root.join(format!(
            "missing\n{}\u{202e}tail",
            "very-long-component-".repeat(16)
        ));
        let mut app = app_for_test(root.clone());
        app.pending_workspace_switch = Some(PendingWorkspaceSwitch::Confirm {
            target: pending_target.clone(),
        });

        app.request_open_workspace(missing_target);

        assert_eq!(app.workspace.root, root);
        assert!(app.pending_workspace_switch.is_none());
        assert!(app.status.starts_with("Workspace path is not a folder: "));
        assert!(!app.status.contains('\n'));
        assert!(!app.status.contains('\u{202e}'));
        assert!(
            app.status.chars().count()
                <= "Workspace path is not a folder: ".chars().count()
                    + DISPLAY_PATH_LABEL_MAX_CHARS
        );
        drop(app);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(pending_target).unwrap();
    }

    #[test]
    fn workspace_path_not_folder_status_sanitizes_and_bounds_full_path() {
        let path = Path::new("workspace").join(format!(
            "missing\n{}\u{2066}tail",
            "very-long-component-".repeat(16)
        ));

        let status = workspace_path_not_folder_status(&path);

        assert!(status.starts_with("Workspace path is not a folder: "));
        assert!(status.contains("workspace"));
        assert!(!status.contains('\n'));
        assert!(!status.contains('\u{2066}'));
        assert!(status.contains("..."));
        assert!(
            status.chars().count()
                <= "Workspace path is not a folder: ".chars().count()
                    + DISPLAY_PATH_LABEL_MAX_CHARS
        );
    }

    #[test]
    fn workspace_path_not_folder_status_reuses_clean_ascii_display_label() {
        let path = Path::new("workspace").join("missing-folder");
        let display_path = path.display().to_string();

        let label = sanitized_display_label_cow(&display_path, DISPLAY_PATH_LABEL_MAX_CHARS, ".");
        match label {
            Cow::Borrowed(label) => assert_eq!(label, display_path.as_str()),
            Cow::Owned(label) => panic!("expected borrowed label, got {label:?}"),
        }
        assert_eq!(
            workspace_path_not_folder_status(&path),
            format!("Workspace path is not a folder: {display_path}")
        );
    }

    #[test]
    fn workspace_path_not_folder_status_reuses_clean_unicode_display_label() {
        let path = Path::new("workspace").join("missing-\u{03bb}-\u{4e2d}");
        let display_path = path.display().to_string();

        let label = sanitized_display_label_cow(&display_path, DISPLAY_PATH_LABEL_MAX_CHARS, ".");
        match label {
            Cow::Borrowed(label) => assert_eq!(label, display_path.as_str()),
            Cow::Owned(label) => panic!("expected borrowed label, got {label:?}"),
        }
        assert_eq!(
            workspace_path_not_folder_status(&path),
            format!("Workspace path is not a folder: {display_path}")
        );
    }

    #[test]
    fn workspace_path_not_folder_status_dirty_truncated_and_fallback_labels_are_owned() {
        let dirty_path = Path::new("workspace").join("missing\nfolder\u{202e}");
        let truncated_path = Path::new("workspace").join(format!("missing-{}", "x".repeat(256)));
        let fallback_path = PathBuf::new();

        for path in [&dirty_path, &truncated_path, &fallback_path] {
            let display_path = path.display().to_string();
            let expected_label =
                sanitized_display_label_cow(&display_path, DISPLAY_PATH_LABEL_MAX_CHARS, ".");

            assert!(
                matches!(&expected_label, Cow::Owned(_)),
                "expected owned label for {display_path:?}"
            );
            assert_eq!(
                workspace_path_not_folder_status(path),
                format!("Workspace path is not a folder: {expected_label}")
            );
        }

        let dirty_status = workspace_path_not_folder_status(&dirty_path);
        assert!(!dirty_status.contains('\n'));
        assert!(!dirty_status.contains('\u{202e}'));

        let truncated_status = workspace_path_not_folder_status(&truncated_path);
        assert!(truncated_status.contains("..."));
        assert!(
            truncated_status.chars().count()
                <= "Workspace path is not a folder: ".chars().count()
                    + DISPLAY_PATH_LABEL_MAX_CHARS
        );

        assert_eq!(
            workspace_path_not_folder_status(&fallback_path),
            "Workspace path is not a folder: ."
        );
    }

    #[test]
    fn workspace_path_not_folder_status_matches_sanitized_display_label_parity() {
        for path in [
            Path::new("workspace").join("missing-folder"),
            Path::new("workspace").join("missing-\u{03bb}"),
            Path::new("workspace").join("missing\nfolder\u{2066}"),
            Path::new("workspace").join(format!("missing-{}", "x".repeat(256))),
            PathBuf::new(),
        ] {
            let display_path = path.display().to_string();
            let expected_label =
                sanitized_display_label(&display_path, DISPLAY_PATH_LABEL_MAX_CHARS, ".");

            assert_eq!(
                workspace_path_not_folder_status(&path),
                format!("Workspace path is not a folder: {expected_label}")
            );
        }
    }

    #[test]
    fn workspace_path_not_folder_status_preserves_raw_display_path_for_clean_labels() {
        let path = Path::new("workspace")
            .join("child")
            .join("..")
            .join("missing-folder");
        let display_path = path.display().to_string();

        assert!(display_path.contains(".."));
        assert_eq!(
            workspace_path_not_folder_status(&path),
            format!("Workspace path is not a folder: {display_path}")
        );
    }

    #[test]
    fn recent_projects_save_failure_status_sanitizes_and_bounds_error_detail() {
        let status = recent_projects_save_failure_status(format!(
            "first line\n{}\u{202e}tail",
            "error-detail-".repeat(24)
        ));

        assert!(status.starts_with("Could not save recent projects: first line "));
        assert!(!status.contains('\n'));
        assert!(!status.contains('\u{202e}'));
        assert!(status.contains("..."));
        assert!(
            status.chars().count()
                <= "Could not save recent projects: ".chars().count()
                    + DISPLAY_ERROR_LABEL_MAX_CHARS
        );
    }

    #[test]
    fn recent_projects_save_failure_status_falls_back_for_blank_error_detail() {
        assert_eq!(
            recent_projects_save_failure_status("\n\u{202e}\u{0007}"),
            "Could not save recent projects: unknown error"
        );
    }

    #[test]
    fn workspace_switch_unsaved_status_uses_file_count_labels() {
        assert_eq!(
            workspace_switch_unsaved_status(1),
            "1 unsaved file before switching workspace"
        );
        assert_eq!(
            workspace_switch_unsaved_status(2),
            "2 unsaved files before switching workspace"
        );
    }

    #[test]
    fn workspace_switch_dirty_buffer_count_dedupes_duplicate_buffer_ids() {
        let mut first = TextBuffer::from_text(7, Some(PathBuf::from("workspace/a.rs")), "a".into());
        first.mark_dirty();
        let mut duplicate =
            TextBuffer::from_text(7, Some(PathBuf::from("workspace/b.rs")), "b".into());
        duplicate.mark_dirty();
        let mut second = TextBuffer::from_text(8, None, "c".into());
        second.mark_dirty();
        let clean_duplicate =
            TextBuffer::from_text(8, Some(PathBuf::from("workspace/clean.rs")), "d".into());

        assert_eq!(
            workspace_switch_dirty_buffer_count(&[first, duplicate, second, clean_duplicate]),
            2
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
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "kuroya-workspace-lifecycle-{name}-{}-{nanos}",
            std::process::id()
        ))
    }
}
