use crate::{
    KuroyaApp,
    explorer::{ExplorerEntryKind, ExplorerFileAction, explorer_kind_for_path},
    explorer_runtime::{explorer_operation_error_detail, explorer_operation_path_label},
    native_paths::normalize_native_path,
    ui_events::UiEvent,
    workspace_state::paths_match_lexically,
    workspace_trust::{
        trusted_workspace_paths_match, workspace_path_contains_lexically,
        workspace_path_stays_within_root_lexically,
    },
};
use kuroya_core::ProjectEntry;
use std::{
    fs,
    path::{Path, PathBuf},
};

mod submit;

impl KuroyaApp {
    pub(crate) fn begin_create_file(&mut self, parent: PathBuf) {
        if self.workspace_placeholder {
            self.status = "Open a folder before creating workspace files".to_owned();
            return;
        }
        if let Some(status) = explorer_create_parent_rejected_status(
            &self.workspace.root,
            self.index.all_entries(),
            &parent,
            Path::is_dir,
        ) {
            self.status = status;
            return;
        }

        self.explorer_file_action = None;
        self.explorer_file_input.clear();
        self.spawn_create_file_picker(parent);
        self.status = "Choose a file name".to_owned();
    }

    pub(crate) fn begin_create_folder(&mut self, parent: PathBuf) {
        if self.workspace_placeholder {
            self.status = "Open a folder before creating workspace folders".to_owned();
            return;
        }
        if let Some(status) = explorer_create_parent_rejected_status(
            &self.workspace.root,
            self.index.all_entries(),
            &parent,
            Path::is_dir,
        ) {
            self.status = status;
            return;
        }

        self.explorer_file_action = None;
        self.explorer_file_input.clear();
        self.spawn_create_folder_picker(parent);
        self.status = "Choose or create a workspace folder".to_owned();
    }

    pub(crate) fn begin_rename_path(&mut self, path: PathBuf) {
        let indexed_entries = self.index.all_entries();
        if let Some(status) = explorer_rename_rejected_status(
            &self.workspace.root,
            indexed_entries,
            &path,
            Path::exists,
        ) {
            self.status = status;
            return;
        }

        let kind =
            explorer_file_action_kind_for_path(indexed_entries, &path, explorer_kind_for_path);
        self.explorer_file_input = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned();
        self.explorer_file_action = Some(ExplorerFileAction::Rename { path, kind });
        self.status = "Rename workspace item".to_owned();
    }
}

impl KuroyaApp {
    fn spawn_create_file_picker(&mut self, initial_dir: PathBuf) {
        self.spawn_explorer_create_path_picker(ExplorerEntryKind::File, initial_dir);
    }

    fn spawn_create_folder_picker(&mut self, initial_dir: PathBuf) {
        self.spawn_explorer_create_path_picker(ExplorerEntryKind::Folder, initial_dir);
    }

    fn spawn_explorer_create_path_picker(&mut self, kind: ExplorerEntryKind, initial_dir: PathBuf) {
        let tx = self.tx.clone();
        let root = self.workspace.root.clone();
        let generation = self.workspace_event_generation;
        self.runtime.spawn_blocking(move || {
            let picked = match kind {
                ExplorerEntryKind::File => pick_create_file_path(&initial_dir),
                ExplorerEntryKind::Folder => pick_create_folder_path(&initial_dir),
            };
            let event = match picked {
                Ok(Some(path)) => UiEvent::ExplorerCreatePathPicked {
                    root,
                    generation,
                    kind,
                    path,
                },
                Ok(None) => UiEvent::ExplorerCreatePathPickerCanceled {
                    root,
                    generation,
                    kind,
                },
                Err(error) => UiEvent::ExplorerCreatePathPickerFailed {
                    root,
                    generation,
                    kind,
                    error,
                },
            };
            let _ = crate::ui_event_channel::send_critical_ui_event(&tx, event);
        });
    }

    pub(crate) fn apply_explorer_create_path_picked(
        &mut self,
        kind: ExplorerEntryKind,
        path: PathBuf,
    ) {
        let path = normalize_native_path(path);
        match kind {
            ExplorerEntryKind::File => self.apply_create_file_picker_path(path),
            ExplorerEntryKind::Folder => self.apply_create_folder_picker_path(path),
        }
    }

    pub(crate) fn apply_explorer_create_path_picker_canceled(&mut self, kind: ExplorerEntryKind) {
        self.status = match kind {
            ExplorerEntryKind::File => "File creation canceled".to_owned(),
            ExplorerEntryKind::Folder => "Folder creation canceled".to_owned(),
        };
    }

    pub(crate) fn apply_explorer_create_path_picker_failed(
        &mut self,
        kind: ExplorerEntryKind,
        error: String,
    ) {
        let action = match kind {
            ExplorerEntryKind::File => "file picker",
            ExplorerEntryKind::Folder => "folder picker",
        };
        self.status = format!(
            "{action} failed: {}",
            explorer_operation_error_detail(&error)
        );
    }

    fn apply_create_file_picker_path(&mut self, path: PathBuf) {
        if let Some(status) =
            picked_create_path_rejected_status(&self.workspace.root, &path, ExplorerEntryKind::File)
        {
            self.status = status;
            return;
        }

        if path.exists() {
            self.status = format!("{} already exists", explorer_operation_path_label(&path));
            return;
        }

        self.spawn_create_file(path);
    }

    fn apply_create_folder_picker_path(&mut self, path: PathBuf) {
        if let Some(status) = picked_create_path_rejected_status(
            &self.workspace.root,
            &path,
            ExplorerEntryKind::Folder,
        ) {
            self.status = status;
            return;
        }

        if path.is_dir() {
            self.reveal_file_in_explorer(path.clone());
            self.explorer_expanded.insert(path.clone());
            self.status = format!("Selected folder {}", explorer_operation_path_label(&path));
            self.spawn_index();
            self.spawn_git_auto_refresh();
            return;
        }

        if path.exists() {
            self.status = format!("{} is not a folder", explorer_operation_path_label(&path));
            return;
        }

        self.spawn_create_folder(path);
    }
}

fn picked_create_path_rejected_status(
    workspace_root: &Path,
    path: &Path,
    kind: ExplorerEntryKind,
) -> Option<String> {
    if trusted_workspace_paths_match(workspace_root, path) {
        return Some(match kind {
            ExplorerEntryKind::File => "Choose a file inside the workspace".to_owned(),
            ExplorerEntryKind::Folder => {
                "Choose or create a folder inside the workspace".to_owned()
            }
        });
    }
    if !workspace_path_contains_lexically(workspace_root, path)
        || !workspace_path_stays_within_root_lexically(workspace_root, path)
    {
        return Some("Explorer target must stay inside the workspace".to_owned());
    }
    None
}

fn pick_create_file_path(initial_dir: &Path) -> Result<Option<PathBuf>, String> {
    Ok(rfd::FileDialog::new()
        .set_title("Create file")
        .set_directory(initial_dir)
        .set_file_name("untitled")
        .set_can_create_directories(true)
        .save_file()
        .map(normalize_native_path))
}

fn pick_create_folder_path(initial_dir: &Path) -> Result<Option<PathBuf>, String> {
    Ok(rfd::FileDialog::new()
        .set_title("Choose or create workspace folder")
        .set_directory(initial_dir)
        .set_can_create_directories(true)
        .pick_folder()
        .map(normalize_native_path))
}

fn explorer_file_action_kind_for_path(
    indexed_entries: &[ProjectEntry],
    path: &Path,
    fallback_kind: impl FnOnce(&Path) -> ExplorerEntryKind,
) -> ExplorerEntryKind {
    explorer_file_action_indexed_kind(indexed_entries, path).unwrap_or_else(|| fallback_kind(path))
}

fn explorer_file_action_indexed_kind(
    indexed_entries: &[ProjectEntry],
    path: &Path,
) -> Option<ExplorerEntryKind> {
    indexed_entries
        .iter()
        .find(|entry| entry.path == path || paths_match_lexically(&entry.path, path))
        .map(|entry| {
            if entry.is_dir {
                ExplorerEntryKind::Folder
            } else {
                ExplorerEntryKind::File
            }
        })
}

fn explorer_file_action_index_contains_path(indexed_entries: &[ProjectEntry], path: &Path) -> bool {
    explorer_file_action_indexed_kind(indexed_entries, path).is_some()
}

fn explorer_create_parent_rejected_status(
    workspace_root: &Path,
    indexed_entries: &[ProjectEntry],
    parent: &Path,
    _parent_is_dir: impl FnOnce(&Path) -> bool,
) -> Option<String> {
    explorer_create_parent_rejected_status_with_probe(
        workspace_root,
        indexed_entries,
        parent,
        explorer_create_parent_probe_path,
    )
}

fn explorer_create_parent_rejected_status_with_probe(
    workspace_root: &Path,
    indexed_entries: &[ProjectEntry],
    parent: &Path,
    parent_probe: impl FnOnce(&Path) -> ExplorerCreateParentProbe,
) -> Option<String> {
    if !workspace_path_stays_within_root_lexically(workspace_root, parent) {
        return Some("Explorer target must stay inside the workspace".to_owned());
    }

    match explorer_file_action_indexed_kind(indexed_entries, parent) {
        Some(ExplorerEntryKind::Folder) => None,
        Some(ExplorerEntryKind::File) => Some(format!(
            "{} is not a folder",
            explorer_operation_path_label(parent)
        )),
        None if paths_match_lexically(parent, workspace_root) => None,
        None => match parent_probe(parent) {
            ExplorerCreateParentProbe::Folder => None,
            ExplorerCreateParentProbe::ExistingNonFolder => Some(format!(
                "{} is not a folder",
                explorer_operation_path_label(parent)
            )),
            ExplorerCreateParentProbe::Missing => {
                Some(explorer_file_action_no_longer_exists_status(parent))
            }
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExplorerCreateParentProbe {
    Folder,
    ExistingNonFolder,
    Missing,
}

fn explorer_create_parent_probe_path(path: &Path) -> ExplorerCreateParentProbe {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => ExplorerCreateParentProbe::Folder,
        Ok(_) => ExplorerCreateParentProbe::ExistingNonFolder,
        Err(_) => ExplorerCreateParentProbe::Missing,
    }
}

fn explorer_rename_rejected_status(
    workspace_root: &Path,
    indexed_entries: &[ProjectEntry],
    path: &Path,
    path_exists: impl FnOnce(&Path) -> bool,
) -> Option<String> {
    if paths_match_lexically(path, workspace_root) {
        return Some("Use Open Folder to change workspace roots".to_owned());
    }
    if !workspace_path_stays_within_root_lexically(workspace_root, path) {
        return Some("Explorer target must stay inside the workspace".to_owned());
    }
    if explorer_file_action_index_contains_path(indexed_entries, path) || path_exists(path) {
        return None;
    }

    Some(explorer_file_action_no_longer_exists_status(path))
}

fn explorer_file_action_no_longer_exists_status(path: &Path) -> String {
    format!("{} no longer exists", explorer_operation_path_label(path))
}

#[cfg(test)]
mod tests {
    use super::{
        ExplorerCreateParentProbe, explorer_create_parent_rejected_status,
        explorer_create_parent_rejected_status_with_probe, explorer_rename_rejected_status,
    };
    use crate::{
        KuroyaApp,
        app_startup_context::AppStartupContext,
        explorer::{ExplorerEntryKind, ExplorerFileAction},
        terminal::TerminalPane,
    };
    use kuroya_core::{EditorSettings, ProjectEntry, Workspace};
    use std::{
        cell::Cell,
        fs,
        path::PathBuf,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };
    use tokio::runtime::Runtime;

    #[test]
    fn create_parent_guard_rejects_stale_and_file_targets() {
        let root = PathBuf::from("workspace");
        let stale = root.join("stale");
        let file = root.join("README.md");
        let entries = [project_entry(file.clone(), false)];

        assert_eq!(
            explorer_create_parent_rejected_status_with_probe(&root, &[], &stale, |_| {
                ExplorerCreateParentProbe::Missing
            }),
            Some("stale no longer exists".to_owned())
        );
        assert_eq!(
            explorer_create_parent_rejected_status(&root, &entries, &file, |_| {
                panic!("indexed file target should not probe the filesystem")
            }),
            Some("README.md is not a folder".to_owned())
        );
    }

    #[test]
    fn create_parent_guard_probes_unknown_parent_once() {
        let root = PathBuf::from("workspace");
        let parent = root.join("README.md");
        let probes = Cell::new(0usize);

        assert_eq!(
            explorer_create_parent_rejected_status_with_probe(&root, &[], &parent, |_| {
                probes.set(probes.get() + 1);
                ExplorerCreateParentProbe::ExistingNonFolder
            }),
            Some("README.md is not a folder".to_owned())
        );

        assert_eq!(probes.get(), 1);
    }

    #[test]
    fn create_parent_guard_uses_short_circuits_before_probe() {
        let root = PathBuf::from("workspace");
        let indexed_folder = root.join("src");
        let entries = [project_entry(indexed_folder.clone(), true)];

        assert!(
            explorer_create_parent_rejected_status_with_probe(
                &root,
                &entries,
                &indexed_folder,
                |_| panic!("indexed folder target should not probe the filesystem")
            )
            .is_none()
        );
        assert!(
            explorer_create_parent_rejected_status_with_probe(&root, &[], &root, |_| {
                panic!("workspace root target should not probe the filesystem")
            })
            .is_none()
        );
    }

    #[test]
    fn create_parent_guard_preserves_raw_parent_path() {
        let root = PathBuf::from("workspace");
        let parent = root.join("raw\n\u{202e}folder");
        let original = parent.clone();
        let entries = [project_entry(parent.clone(), true)];

        assert!(
            explorer_create_parent_rejected_status(&root, &entries, &parent, |_| {
                panic!("indexed folder target should not probe the filesystem")
            })
            .is_none()
        );

        assert_eq!(parent, original);
        assert!(parent.as_os_str().to_string_lossy().contains('\n'));
        assert!(parent.as_os_str().to_string_lossy().contains('\u{202e}'));
    }

    #[test]
    fn rename_guard_rejects_root_outside_and_stale_targets() {
        let root = PathBuf::from("workspace").join("current");

        assert_eq!(
            explorer_rename_rejected_status(&root, &[], &root.join("src").join(".."), |_| true),
            Some("Use Open Folder to change workspace roots".to_owned())
        );
        assert_eq!(
            explorer_rename_rejected_status(
                &root,
                &[],
                &root.join("..").join("current").join("src"),
                |_| true
            ),
            Some("Explorer target must stay inside the workspace".to_owned())
        );
        assert_eq!(
            explorer_rename_rejected_status(&root, &[], &root.join("missing.rs"), |_| false),
            Some("missing.rs no longer exists".to_owned())
        );
    }

    #[test]
    fn rename_guard_uses_index_entries_before_filesystem_probe() {
        let root = PathBuf::from("workspace");
        let path = root.join("src").join("..").join("src").join("main.rs");
        let entries = [project_entry(root.join("src").join("main.rs"), false)];

        assert!(
            explorer_rename_rejected_status(&root, &entries, &path, |_| {
                panic!("indexed rename target should not probe the filesystem")
            })
            .is_none()
        );
    }

    #[test]
    fn begin_create_file_rejects_stale_parent_without_opening_dialog() {
        let root = temp_workspace("begin-create-stale-parent");
        fs::create_dir_all(&root).unwrap();
        let mut app = app_for_test(root.clone());

        app.begin_create_file(root.join("stale"));

        assert_eq!(app.status, "stale no longer exists");
        assert!(app.explorer_file_action.is_none());
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn begin_create_actions_reject_placeholder_workspace() {
        let root = temp_workspace("begin-create-placeholder");
        fs::create_dir_all(&root).unwrap();
        let mut app = app_for_test(root.clone());
        app.workspace_placeholder = true;

        app.begin_create_file(root.clone());

        assert_eq!(app.status, "Open a folder before creating workspace files");
        assert!(app.explorer_file_action.is_none());

        app.begin_create_folder(root.clone());

        assert_eq!(
            app.status,
            "Open a folder before creating workspace folders"
        );
        assert!(app.explorer_file_action.is_none());
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn begin_rename_path_rejects_stale_target_without_opening_dialog() {
        let root = temp_workspace("begin-rename-stale-target");
        fs::create_dir_all(&root).unwrap();
        let mut app = app_for_test(root.clone());

        app.begin_rename_path(root.join("missing.rs"));

        assert_eq!(app.status, "missing.rs no longer exists");
        assert!(app.explorer_file_action.is_none());
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn begin_rename_path_preserves_raw_existing_target_path() {
        let root = temp_workspace("begin-rename-raw-target");
        fs::create_dir_all(root.join("src")).unwrap();
        let path = root.join("src").join("raw\u{202e}target.rs");
        fs::write(&path, "fn main() {}\n").unwrap();
        let mut app = app_for_test(root.clone());

        app.begin_rename_path(path.clone());

        match app.explorer_file_action.as_ref() {
            Some(ExplorerFileAction::Rename { path: target, kind }) => {
                assert_eq!(target, &path);
                assert_eq!(*kind, ExplorerEntryKind::File);
            }
            other => panic!("expected rename action, got {other:?}"),
        }
        assert!(path.as_os_str().to_string_lossy().contains('\u{202e}'));
        drop(app);
        fs::remove_dir_all(root).unwrap();
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

    fn project_entry(path: PathBuf, is_dir: bool) -> ProjectEntry {
        ProjectEntry {
            path,
            relative_path: PathBuf::new(),
            is_dir,
            depth: 0,
            ..Default::default()
        }
    }

    fn temp_workspace(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "kuroya-explorer-file-action-begin-{name}-{}-{nanos}",
            std::process::id()
        ))
    }
}
