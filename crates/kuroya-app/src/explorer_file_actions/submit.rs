use crate::{
    KuroyaApp,
    explorer::{ExplorerEntryKind, ExplorerFileAction, workspace_child_path},
    explorer_runtime::{explorer_operation_error_detail, explorer_operation_path_label},
};
use kuroya_core::{ProjectEntry, TextBuffer};
use std::{
    collections::HashSet,
    ffi::OsStr,
    fs,
    path::{Component, Path, PathBuf},
};

impl KuroyaApp {
    pub(crate) fn submit_explorer_file_action(&mut self) {
        let Some(action) = self.explorer_file_action.take() else {
            return;
        };

        match action {
            ExplorerFileAction::Rename { path, kind } => {
                let indexed_entries = self.index.all_entries();
                if let Some(status) = super::explorer_rename_rejected_status(
                    &self.workspace.root,
                    indexed_entries,
                    &path,
                    Path::exists,
                ) {
                    self.explorer_file_action = Some(ExplorerFileAction::Rename { path, kind });
                    self.status = status;
                    return;
                }
                if let Some(status) = explorer_rename_target_kind_rejected_status(
                    indexed_entries,
                    &path,
                    kind,
                    explorer_file_action_probe_path_kind,
                ) {
                    self.explorer_file_action = Some(ExplorerFileAction::Rename { path, kind });
                    self.status = status;
                    return;
                }
                let parent = path.parent().unwrap_or(&self.workspace.root);
                if let Some(error) =
                    explorer_rename_target_name_rejected_status(&self.explorer_file_input)
                {
                    self.explorer_file_action = Some(ExplorerFileAction::Rename { path, kind });
                    self.status = explorer_file_action_error_status(error);
                    return;
                }
                let new_path = match workspace_child_path(
                    &self.workspace.root,
                    parent,
                    &self.explorer_file_input,
                ) {
                    Ok(path) => path,
                    Err(error) => {
                        self.explorer_file_action = Some(ExplorerFileAction::Rename { path, kind });
                        self.status = explorer_file_action_error_status(&error);
                        return;
                    }
                };
                if explorer_file_action_paths_match_after_cleaning(&new_path, &path) {
                    self.explorer_file_input.clear();
                    self.status = "Rename unchanged".to_owned();
                    return;
                }
                let availability = ExplorerFileActionPathAvailability::from_sources(
                    indexed_entries,
                    &self.buffers,
                    self.index.files(),
                );
                if availability.known_or_exists(&new_path, Path::exists) {
                    self.explorer_file_action = Some(ExplorerFileAction::Rename { path, kind });
                    self.status = explorer_file_action_already_exists_status(&new_path);
                    return;
                }

                self.explorer_file_input.clear();
                self.spawn_rename_path(path, new_path, kind);
            }
        }
    }
}

fn explorer_rename_target_kind_rejected_status(
    indexed_entries: &[ProjectEntry],
    path: &Path,
    expected_kind: ExplorerEntryKind,
    probe_path_kind: impl FnOnce(&Path) -> ExplorerFileActionPathKindProbe,
) -> Option<String> {
    let indexed_kind = super::explorer_file_action_indexed_kind(indexed_entries, path);
    match probe_path_kind(path) {
        ExplorerFileActionPathKindProbe::Known(actual_kind) => {
            if actual_kind != expected_kind {
                return Some(explorer_file_action_kind_changed_status(
                    path,
                    expected_kind,
                ));
            }
            return None;
        }
        ExplorerFileActionPathKindProbe::Missing => {
            return Some(explorer_file_action_missing_status(path));
        }
        ExplorerFileActionPathKindProbe::Unknown => {}
    }

    indexed_kind
        .filter(|actual_kind| *actual_kind != expected_kind)
        .map(|_| explorer_file_action_kind_changed_status(path, expected_kind))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExplorerFileActionPathKindProbe {
    Known(ExplorerEntryKind),
    Missing,
    Unknown,
}

fn explorer_file_action_probe_path_kind(path: &Path) -> ExplorerFileActionPathKindProbe {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => {
            ExplorerFileActionPathKindProbe::Known(ExplorerEntryKind::File)
        }
        Ok(metadata) if metadata.is_dir() => {
            ExplorerFileActionPathKindProbe::Known(ExplorerEntryKind::Folder)
        }
        Ok(_) => ExplorerFileActionPathKindProbe::Unknown,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            ExplorerFileActionPathKindProbe::Missing
        }
        Err(_) => ExplorerFileActionPathKindProbe::Unknown,
    }
}

fn explorer_rename_target_name_rejected_status(input: &str) -> Option<&'static str> {
    let trimmed = input.trim();
    if trimmed.is_empty() || Path::new(trimmed).is_absolute() {
        return None;
    }

    let mut normal_components = 0usize;
    let mut has_current_dir_component = false;
    for component in Path::new(trimmed).components() {
        match component {
            Component::Normal(_) => normal_components += 1,
            Component::CurDir => has_current_dir_component = true,
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }

    if normal_components > 1
        || has_current_dir_component
        || explorer_rename_target_contains_separator(trimmed)
    {
        Some("Rename target must be a single name")
    } else {
        None
    }
}

#[cfg(windows)]
fn explorer_rename_target_contains_separator(input: &str) -> bool {
    input.contains('/') || input.contains('\\')
}

#[cfg(not(windows))]
fn explorer_rename_target_contains_separator(input: &str) -> bool {
    input.contains('/')
}

#[derive(Debug, Default)]
struct ExplorerFileActionPathAvailability {
    known_paths: HashSet<PathBuf>,
}

impl ExplorerFileActionPathAvailability {
    fn from_sources(
        indexed_entries: &[ProjectEntry],
        buffers: &[TextBuffer],
        indexed_files: &[PathBuf],
    ) -> Self {
        let mut known_paths =
            HashSet::with_capacity(indexed_entries.len() + buffers.len() + indexed_files.len());
        for entry in indexed_entries {
            known_paths.insert(explorer_file_action_path_key(&entry.path));
        }
        for buffer in buffers {
            if let Some(path) = buffer.path() {
                known_paths.insert(explorer_file_action_path_key(path));
            }
        }
        for path in indexed_files {
            known_paths.insert(explorer_file_action_path_key(path));
        }

        Self { known_paths }
    }

    fn known_or_exists(&self, path: &Path, path_exists: impl FnOnce(&Path) -> bool) -> bool {
        if self.known_paths.is_empty() {
            return path_exists(path);
        }

        self.known_paths
            .contains(&explorer_file_action_path_key(path))
            || path_exists(path)
    }
}

#[cfg(test)]
fn explorer_file_action_path_known_or_exists(
    indexed_entries: &[ProjectEntry],
    buffers: &[TextBuffer],
    indexed_files: &[PathBuf],
    path: &Path,
    path_exists: impl FnOnce(&Path) -> bool,
) -> bool {
    ExplorerFileActionPathAvailability::from_sources(indexed_entries, buffers, indexed_files)
        .known_or_exists(path, path_exists)
}

fn explorer_file_action_error_status(error: &str) -> String {
    explorer_operation_error_detail(error)
}

fn explorer_file_action_already_exists_status(path: &Path) -> String {
    format!("{} already exists", explorer_operation_path_label(path))
}

fn explorer_file_action_missing_status(path: &Path) -> String {
    format!("{} no longer exists", explorer_operation_path_label(path))
}

fn explorer_file_action_kind_changed_status(
    path: &Path,
    expected_kind: ExplorerEntryKind,
) -> String {
    let kind = match expected_kind {
        ExplorerEntryKind::File => "file",
        ExplorerEntryKind::Folder => "folder",
    };
    format!(
        "{} is no longer a {kind}",
        explorer_operation_path_label(path)
    )
}

fn explorer_file_action_paths_match_after_cleaning(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    if explorer_file_action_path_key(left) == explorer_file_action_path_key(right) {
        return true;
    }
    if !explorer_file_action_path_needs_cleaning(left)
        && !explorer_file_action_path_needs_cleaning(right)
    {
        return false;
    }

    explorer_file_action_clean_path(left)
        .zip(explorer_file_action_clean_path(right))
        .is_some_and(|(left, right)| left == right)
}

fn explorer_file_action_path_needs_cleaning(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
}

fn explorer_file_action_clean_path(path: &Path) -> Option<PathBuf> {
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => clean.push(prefix.as_os_str()),
            Component::RootDir => clean.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !clean.pop() {
                    return None;
                }
            }
            Component::Normal(component) => clean.push(component),
        }
    }
    Some(clean)
}

fn explorer_file_action_path_key(path: &Path) -> PathBuf {
    let mut key = PathBuf::new();
    let mut has_root = false;
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                explorer_file_action_push_component_key(&mut key, prefix.as_os_str());
                has_root = false;
            }
            Component::RootDir => {
                key.push(component.as_os_str());
                has_root = true;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                let can_pop = matches!(key.components().next_back(), Some(Component::Normal(_)));
                if can_pop {
                    key.pop();
                } else if !has_root {
                    key.push("..");
                }
            }
            Component::Normal(component) => {
                explorer_file_action_push_component_key(&mut key, component)
            }
        }
    }

    if key.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        key
    }
}

#[cfg(windows)]
fn explorer_file_action_push_component_key(key: &mut PathBuf, component: &OsStr) {
    use std::borrow::Cow;

    match component.to_string_lossy() {
        Cow::Borrowed(text) if !explorer_file_action_component_needs_lowercase(text) => {
            key.push(Path::new(component));
        }
        Cow::Borrowed(text) if text.is_ascii() => {
            let mut lowered = text.to_owned();
            lowered.make_ascii_lowercase();
            key.push(Path::new(&lowered));
        }
        component => {
            let lowered = component.to_lowercase();
            key.push(Path::new(&lowered));
        }
    }
}

#[cfg(windows)]
fn explorer_file_action_component_needs_lowercase(component: &str) -> bool {
    if component.is_ascii() {
        return component.bytes().any(|byte| byte.is_ascii_uppercase());
    }

    component.chars().any(|ch| {
        let mut lowercase = ch.to_lowercase();
        lowercase.next() != Some(ch) || lowercase.next().is_some()
    })
}

#[cfg(not(windows))]
fn explorer_file_action_push_component_key(key: &mut PathBuf, component: &OsStr) {
    key.push(Path::new(component));
}

#[cfg(test)]
mod tests {
    use super::super::explorer_file_action_kind_for_path;
    use super::{
        ExplorerFileActionPathAvailability, ExplorerFileActionPathKindProbe,
        explorer_file_action_already_exists_status, explorer_file_action_error_status,
        explorer_file_action_missing_status, explorer_file_action_path_known_or_exists,
        explorer_file_action_paths_match_after_cleaning,
        explorer_rename_target_kind_rejected_status, explorer_rename_target_name_rejected_status,
    };
    use crate::{
        KuroyaApp,
        app_startup_context::AppStartupContext,
        explorer::{ExplorerEntryKind, ExplorerFileAction},
        explorer_runtime::{
            EXPLORER_OPERATION_ERROR_DETAIL_MAX_CHARS, EXPLORER_OPERATION_PATH_LABEL_MAX_CHARS,
        },
        terminal::TerminalPane,
    };
    use kuroya_core::{EditorSettings, ProjectEntry, TextBuffer, Workspace};
    use std::{
        cell::Cell,
        fs,
        path::PathBuf,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };
    use tokio::runtime::Runtime;

    #[test]
    fn explorer_file_action_errors_are_display_safe_and_bounded() {
        let status = explorer_file_action_error_status(&format!(
            "bad\nerror\u{202e}{}",
            "-detail".repeat(EXPLORER_OPERATION_ERROR_DETAIL_MAX_CHARS)
        ));

        assert!(!status.contains('\n'));
        assert!(!status.contains('\u{202e}'));
        assert!(status.contains("..."));
        assert!(status.chars().count() <= EXPLORER_OPERATION_ERROR_DETAIL_MAX_CHARS);
    }

    #[test]
    fn explorer_file_action_path_statuses_are_display_safe_and_bounded() {
        let path = PathBuf::from("workspace").join(format!(
            "bad\n\u{202e}path{}",
            "-segment".repeat(EXPLORER_OPERATION_PATH_LABEL_MAX_CHARS)
        ));

        let exists = explorer_file_action_already_exists_status(&path);
        let missing = explorer_file_action_missing_status(&path);

        for status in [exists, missing] {
            assert!(!status.contains('\n'));
            assert!(!status.contains('\u{202e}'));
            assert!(status.contains("..."));
            assert!(
                status.chars().count()
                    <= EXPLORER_OPERATION_PATH_LABEL_MAX_CHARS
                        + " no longer exists".chars().count()
            );
        }
    }

    #[test]
    fn explorer_file_action_statuses_do_not_modify_raw_paths() {
        let path = PathBuf::from("workspace").join("raw\n\u{202e}name.rs");
        let original = path.clone();

        let status = explorer_file_action_already_exists_status(&path);

        assert_eq!(path, original);
        assert!(path.as_os_str().to_string_lossy().contains('\n'));
        assert!(path.as_os_str().to_string_lossy().contains('\u{202e}'));
        assert!(!status.contains('\n'));
        assert!(!status.contains('\u{202e}'));
    }

    #[test]
    fn explorer_file_action_path_probe_uses_index_entries_before_filesystem_probe() {
        let path = PathBuf::from("workspace/src");
        let entries = vec![project_entry(path.clone(), true)];
        let probes = Cell::new(0usize);

        assert!(explorer_file_action_path_known_or_exists(
            &entries,
            &[],
            &[],
            &path,
            |_| {
                probes.set(probes.get() + 1);
                false
            }
        ));
        assert_eq!(probes.get(), 0);
    }

    #[test]
    fn explorer_file_action_path_probe_uses_equivalent_index_entry_before_filesystem_probe() {
        let path = PathBuf::from("workspace/src/main.rs");
        let equivalent_path = PathBuf::from("workspace/src/../src/main.rs");
        let entries = vec![project_entry(path, false)];
        let probes = Cell::new(0usize);

        assert!(explorer_file_action_path_known_or_exists(
            &entries,
            &[],
            &[],
            &equivalent_path,
            |_| {
                probes.set(probes.get() + 1);
                false
            }
        ));
        assert_eq!(probes.get(), 0);
    }

    #[test]
    fn explorer_file_action_path_probe_uses_open_buffer_before_filesystem_probe() {
        let path = PathBuf::from("workspace/src/main.rs");
        let equivalent_path = PathBuf::from("workspace/src/../src/main.rs");
        let buffers = vec![TextBuffer::from_text(7, Some(path), "open\n".to_owned())];
        let probes = Cell::new(0usize);

        assert!(explorer_file_action_path_known_or_exists(
            &[],
            &buffers,
            &[],
            &equivalent_path,
            |_| {
                probes.set(probes.get() + 1);
                false
            }
        ));
        assert_eq!(probes.get(), 0);
    }

    #[test]
    fn explorer_file_action_path_probe_uses_indexed_files_before_filesystem_probe() {
        let path = PathBuf::from("workspace/src/main.rs");
        let indexed_files = vec![path.clone()];
        let probes = Cell::new(0usize);

        assert!(explorer_file_action_path_known_or_exists(
            &[],
            &[],
            &indexed_files,
            &path,
            |_| {
                probes.set(probes.get() + 1);
                false
            }
        ));
        assert_eq!(probes.get(), 0);
    }

    #[test]
    fn explorer_file_action_path_availability_reuses_cached_equivalent_paths() {
        let root = PathBuf::from("workspace");
        let indexed_entries = vec![project_entry(root.join("src"), true)];
        let buffers = vec![TextBuffer::from_text(
            7,
            Some(root.join("src").join("main.rs")),
            "open\n".to_owned(),
        )];
        let indexed_files = vec![root.join("src").join("lib.rs")];
        let availability = ExplorerFileActionPathAvailability::from_sources(
            &indexed_entries,
            &buffers,
            &indexed_files,
        );
        let probes = Cell::new(0usize);

        assert!(
            availability.known_or_exists(&root.join("src").join("..").join("src"), |_| {
                probes.set(probes.get() + 1);
                false
            })
        );
        assert!(availability.known_or_exists(
            &root.join("src").join("..").join("src").join("main.rs"),
            |_| {
                probes.set(probes.get() + 1);
                false
            },
        ));
        assert!(availability.known_or_exists(
            &root.join("src").join("..").join("src").join("lib.rs"),
            |_| {
                probes.set(probes.get() + 1);
                false
            },
        ));

        assert_eq!(probes.get(), 0);
    }

    #[test]
    fn explorer_file_action_path_probe_falls_back_for_unknown_paths() {
        let path = PathBuf::from("workspace/src/generated.rs");
        let probes = Cell::new(0usize);

        assert!(explorer_file_action_path_known_or_exists(
            &[],
            &[],
            &[],
            &path,
            |_| {
                probes.set(probes.get() + 1);
                true
            }
        ));
        assert_eq!(probes.get(), 1);
    }

    #[test]
    fn explorer_file_action_cleaned_paths_match_without_filesystem_probe() {
        assert!(explorer_file_action_paths_match_after_cleaning(
            &PathBuf::from("workspace/src/main.rs"),
            &PathBuf::from("workspace/src/../src/main.rs"),
        ));
        #[cfg(not(windows))]
        assert!(!explorer_file_action_paths_match_after_cleaning(
            &PathBuf::from("workspace/src/Main.rs"),
            &PathBuf::from("workspace/src/main.rs"),
        ));
        #[cfg(windows)]
        assert!(explorer_file_action_paths_match_after_cleaning(
            &PathBuf::from("workspace/src/Main.rs"),
            &PathBuf::from("workspace/src/main.rs"),
        ));
    }

    #[test]
    fn explorer_rename_target_name_rejects_path_syntax_only() {
        assert_eq!(
            explorer_rename_target_name_rejected_status("nested/main.rs"),
            Some("Rename target must be a single name")
        );
        assert_eq!(
            explorer_rename_target_name_rejected_status("./main.rs"),
            Some("Rename target must be a single name")
        );
        assert_eq!(
            explorer_rename_target_name_rejected_status("main.rs/"),
            Some("Rename target must be a single name")
        );
        assert_eq!(explorer_rename_target_name_rejected_status("main.rs"), None);
        assert_eq!(explorer_rename_target_name_rejected_status("  "), None);
        assert_eq!(
            explorer_rename_target_name_rejected_status("../main.rs"),
            None
        );

        #[cfg(windows)]
        assert_eq!(
            explorer_rename_target_name_rejected_status(r"nested\main.rs"),
            Some("Rename target must be a single name")
        );
    }

    #[test]
    fn explorer_rename_target_kind_rejects_stale_or_changed_targets() {
        let path = PathBuf::from("workspace").join("src").join("main.rs");
        let entries = vec![project_entry(path.clone(), false)];

        assert_eq!(
            explorer_rename_target_kind_rejected_status(
                &entries,
                &path,
                ExplorerEntryKind::File,
                |_| ExplorerFileActionPathKindProbe::Missing,
            ),
            Some("main.rs no longer exists".to_owned())
        );
        assert_eq!(
            explorer_rename_target_kind_rejected_status(
                &[],
                &path,
                ExplorerEntryKind::File,
                |_| ExplorerFileActionPathKindProbe::Missing,
            ),
            Some("main.rs no longer exists".to_owned())
        );
        assert_eq!(
            explorer_rename_target_kind_rejected_status(
                &[],
                &path,
                ExplorerEntryKind::File,
                |_| ExplorerFileActionPathKindProbe::Known(ExplorerEntryKind::Folder),
            ),
            Some("main.rs is no longer a file".to_owned())
        );
        assert_eq!(
            explorer_rename_target_kind_rejected_status(
                &entries,
                &path,
                ExplorerEntryKind::File,
                |_| ExplorerFileActionPathKindProbe::Known(ExplorerEntryKind::File),
            ),
            None
        );
    }

    #[cfg(windows)]
    #[test]
    fn explorer_file_action_cleaned_paths_match_windows_equivalents() {
        assert!(explorer_file_action_paths_match_after_cleaning(
            &PathBuf::from(r"C:\Repo\Project\src\Main.rs"),
            &PathBuf::from(r"c:\repo\project\src\main.rs"),
        ));
    }

    #[test]
    fn explorer_file_action_kind_uses_index_entry_before_filesystem_probe() {
        let path = PathBuf::from("workspace/src");
        let equivalent_path = PathBuf::from("workspace/src/../src");
        let entries = vec![project_entry(path, true)];

        let kind = explorer_file_action_kind_for_path(&entries, &equivalent_path, |_| {
            panic!("indexed Explorer entry should not probe the filesystem")
        });

        assert_eq!(kind, ExplorerEntryKind::Folder);
    }

    #[test]
    fn explorer_rename_rejects_equivalent_workspace_root() {
        let root = temp_workspace("rename-equivalent-root");
        fs::create_dir_all(root.join("src")).unwrap();
        let equivalent_root = root.join("src").join("..");
        let mut app = app_for_test(root.clone());
        app.explorer_file_action = Some(ExplorerFileAction::Rename {
            path: equivalent_root,
            kind: ExplorerEntryKind::Folder,
        });
        app.explorer_file_input = "renamed".to_owned();

        app.submit_explorer_file_action();

        assert_eq!(app.status, "Use Open Folder to change workspace roots");
        assert!(matches!(
            app.explorer_file_action,
            Some(ExplorerFileAction::Rename { .. })
        ));
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explorer_rename_submit_rechecks_stale_target() {
        let root = temp_workspace("rename-stale-target");
        fs::create_dir_all(&root).unwrap();
        let mut app = app_for_test(root.clone());
        app.explorer_file_action = Some(ExplorerFileAction::Rename {
            path: root.join("missing.rs"),
            kind: ExplorerEntryKind::File,
        });
        app.explorer_file_input = "renamed.rs".to_owned();

        app.submit_explorer_file_action();

        assert_eq!(app.status, "missing.rs no longer exists");
        assert!(matches!(
            app.explorer_file_action,
            Some(ExplorerFileAction::Rename { .. })
        ));
        assert_eq!(app.explorer_file_input, "renamed.rs");
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explorer_rename_submit_rejects_target_that_changed_kind() {
        let root = temp_workspace("rename-target-kind-changed");
        let original_path = root.join("src").join("main.rs");
        fs::create_dir_all(&original_path).unwrap();
        let mut app = app_for_test(root.clone());
        app.explorer_file_action = Some(ExplorerFileAction::Rename {
            path: original_path.clone(),
            kind: ExplorerEntryKind::File,
        });
        app.explorer_file_input = "lib.rs".to_owned();

        app.submit_explorer_file_action();

        assert_eq!(app.status, "main.rs is no longer a file");
        assert!(matches!(
            app.explorer_file_action,
            Some(ExplorerFileAction::Rename { .. })
        ));
        assert_eq!(app.explorer_file_input, "lib.rs");
        assert!(original_path.is_dir());
        assert!(!root.join("src").join("lib.rs").exists());
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explorer_rename_submit_rejects_nested_target_name() {
        let root = temp_workspace("rename-nested-target-name");
        fs::create_dir_all(root.join("src").join("nested")).unwrap();
        let original_path = root.join("src/main.rs");
        fs::write(&original_path, "fn main() {}\n").unwrap();
        let mut app = app_for_test(root.clone());
        app.explorer_file_action = Some(ExplorerFileAction::Rename {
            path: original_path.clone(),
            kind: ExplorerEntryKind::File,
        });
        app.explorer_file_input = "nested/main.rs".to_owned();

        app.submit_explorer_file_action();

        assert_eq!(app.status, "Rename target must be a single name");
        assert!(matches!(
            app.explorer_file_action,
            Some(ExplorerFileAction::Rename { .. })
        ));
        assert_eq!(app.explorer_file_input, "nested/main.rs");
        assert!(original_path.exists());
        assert!(!root.join("src/nested/main.rs").exists());
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explorer_rename_treats_cleaned_same_path_as_unchanged() {
        let root = temp_workspace("rename-cleaned-same-path");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        let equivalent_path = root.join("src").join("..").join("src").join("main.rs");
        let mut app = app_for_test(root.clone());
        app.explorer_file_action = Some(ExplorerFileAction::Rename {
            path: equivalent_path,
            kind: ExplorerEntryKind::File,
        });
        app.explorer_file_input = "main.rs".to_owned();

        app.submit_explorer_file_action();

        assert_eq!(app.status, "Rename unchanged");
        assert!(app.explorer_file_action.is_none());
        assert!(app.explorer_file_input.is_empty());
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
            "kuroya-explorer-file-action-{name}-{}-{nanos}",
            std::process::id()
        ))
    }
}
