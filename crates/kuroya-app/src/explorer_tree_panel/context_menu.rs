use crate::{
    KuroyaApp,
    explorer_rows::ExplorerRowOpenabilityCache,
    path_clipboard::{PathCopyKind, copy_path_to_clipboard},
    path_display::{DISPLAY_PATH_LABEL_MAX_CHARS, sanitized_display_label_cow},
    workspace_state::paths_match_lexically,
    workspace_trust::workspace_path_stays_within_root_lexically,
};
use eframe::egui;
use kuroya_core::{Command, GitChangeStage, GitFileStatus, ProjectEntry};
use std::{
    borrow::Cow,
    path::{Component, Path},
};

impl KuroyaApp {
    pub(super) fn render_explorer_entry_context_menu(
        &mut self,
        ui: &mut egui::Ui,
        current_entries: &[ProjectEntry],
        path: &Path,
        relative_path: &Path,
        is_dir: bool,
        expanded: bool,
    ) {
        if !explorer_context_menu_path_is_current(
            current_entries,
            &self.workspace.root,
            path,
            relative_path,
            is_dir,
        ) {
            ui.label("Path no longer available");
            return;
        }

        if is_dir {
            let label = if expanded { "Collapse" } else { "Expand" };
            if ui.button(label).clicked() {
                if expanded {
                    self.explorer_expanded.remove(path);
                } else {
                    self.explorer_expanded.insert(path.to_path_buf());
                }
                ui.close();
            }
            if ui.button("New File").clicked() {
                self.command_bus
                    .push(Command::CreateFileIn(path.to_path_buf()));
                ui.close();
            }
            if ui.button("New Folder").clicked() {
                self.command_bus
                    .push(Command::CreateFolderIn(path.to_path_buf()));
                ui.close();
            }
        }
        if !is_dir && ui.button("Open").clicked() {
            self.command_bus.push(Command::OpenFile(path.to_path_buf()));
            ui.close();
        }
        if !is_dir && ui.button("Open in New Pane").clicked() {
            self.open_path_in_new_pane(path.to_path_buf());
            ui.close();
        }
        let mut openability_cache = ExplorerRowOpenabilityCache::default();
        let openability = openability_cache.row_openability(
            &self.buffers,
            self.index.files(),
            path,
            self.explorer_compare_path.as_deref(),
            is_dir,
            Path::exists,
        );
        let can_compare_with_selected = explorer_context_can_compare_with_selected(
            path,
            self.explorer_compare_path.as_deref(),
            openability.can_compare_with_selected,
        );
        let can_open_blame = openability.can_open_blame;
        if !is_dir && ui.button("Select for Compare").clicked() {
            self.command_bus
                .push(Command::SelectFileForCompare(path.to_path_buf()));
            ui.close();
        }
        if can_compare_with_selected && ui.button("Compare with Selected").clicked() {
            self.command_bus
                .push(Command::CompareFileWithSelected(path.to_path_buf()));
            ui.close();
        }
        let source_control_status = (!is_dir).then(|| self.git.status_for(path)).flatten();
        let has_unstaged_changes =
            !is_dir && self.git.has_stage_for(path, GitChangeStage::Unstaged);
        let has_staged_changes = !is_dir && self.git.has_stage_for(path, GitChangeStage::Staged);
        let has_source_control_changes =
            source_control_status.is_some() || has_unstaged_changes || has_staged_changes;
        let has_head_revision =
            source_control_status.is_some_and(explorer_source_control_has_head_revision);
        let has_index_revision = source_control_status.is_some_and(|status| {
            explorer_source_control_has_index_revision(
                status,
                has_unstaged_changes,
                has_staged_changes,
            )
        });
        let can_open_hunks = source_control_status != Some(GitFileStatus::Conflicted);

        if has_unstaged_changes && ui.button("Open Changes").clicked() {
            self.command_bus
                .push(Command::OpenFileChanges(path.to_path_buf()));
            ui.close();
        }
        if has_unstaged_changes && ui.button("Copy Patch").clicked() {
            self.command_bus
                .push(Command::CopyFilePatch(path.to_path_buf()));
            ui.close();
        }
        if has_staged_changes && ui.button("Open Staged Changes").clicked() {
            self.command_bus
                .push(Command::OpenStagedFileChanges(path.to_path_buf()));
            ui.close();
        }
        if has_staged_changes && ui.button("Copy Staged Patch").clicked() {
            self.command_bus
                .push(Command::CopyStagedFilePatch(path.to_path_buf()));
            ui.close();
        }
        if has_source_control_changes && ui.button("Reveal in Source Control").clicked() {
            self.command_bus
                .push(Command::RevealFileInSourceControl(path.to_path_buf()));
            ui.close();
        }
        if has_source_control_changes && ui.button("Compare with HEAD").clicked() {
            self.command_bus
                .push(Command::OpenFileHeadChanges(path.to_path_buf()));
            ui.close();
        }
        if has_head_revision && ui.button("Open File at HEAD").clicked() {
            self.command_bus
                .push(Command::OpenFileHeadRevision(path.to_path_buf()));
            ui.close();
        }
        if has_index_revision && ui.button("Open File at Index").clicked() {
            self.command_bus
                .push(Command::OpenFileIndexRevision(path.to_path_buf()));
            ui.close();
        }
        if has_unstaged_changes && can_open_hunks && ui.button("Open Hunks").clicked() {
            self.command_bus
                .push(Command::OpenFileHunks(path.to_path_buf()));
            ui.close();
        }
        if has_staged_changes && can_open_hunks && ui.button("Open Staged Hunks").clicked() {
            self.command_bus
                .push(Command::OpenStagedFileHunks(path.to_path_buf()));
            ui.close();
        }
        if has_unstaged_changes && ui.button("Stage Changes").clicked() {
            self.command_bus
                .push(Command::StageFileChange(path.to_path_buf()));
            ui.close();
        }
        if has_staged_changes && ui.button("Unstage Changes").clicked() {
            self.command_bus
                .push(Command::UnstageFileChange(path.to_path_buf()));
            ui.close();
        }
        if has_source_control_changes && ui.button("Discard Changes").clicked() {
            self.command_bus
                .push(Command::DiscardFileChanges(path.to_path_buf()));
            ui.close();
        }
        if can_open_blame && ui.button("Open Blame").clicked() {
            self.command_bus
                .push(Command::OpenFileBlame(path.to_path_buf()));
            ui.close();
        }
        if ui.button("Copy Path").clicked() {
            self.status = copy_path_to_clipboard(
                ui.ctx(),
                &self.workspace.root,
                path,
                PathCopyKind::Absolute,
            );
            ui.close();
        }
        if ui.button("Copy Relative Path").clicked() {
            self.status = copy_path_to_clipboard(
                ui.ctx(),
                &self.workspace.root,
                path,
                PathCopyKind::Relative,
            );
            ui.close();
        }
        if ui.button("Rename").clicked() {
            self.command_bus
                .push(Command::RenamePath(path.to_path_buf()));
            ui.close();
        }
        if ui.button("Delete").clicked() {
            self.command_bus
                .push(Command::DeletePath(path.to_path_buf()));
            ui.close();
        }
        if ui.button("Show Path").clicked() {
            self.status = explorer_context_path_display_label(relative_path);
            ui.close();
        }
    }
}

pub(crate) fn explorer_context_path_display_label(path: &Path) -> String {
    explorer_context_path_display_label_cow(path).into_owned()
}

fn explorer_context_path_display_label_cow(path: &Path) -> Cow<'_, str> {
    if let Some(path) = path.to_str() {
        return sanitized_display_label_cow(path, DISPLAY_PATH_LABEL_MAX_CHARS, ".");
    }

    let path = path.display().to_string();
    Cow::Owned(sanitized_display_label_cow(&path, DISPLAY_PATH_LABEL_MAX_CHARS, ".").into_owned())
}

pub(crate) fn explorer_context_menu_path_is_current(
    current_entries: &[ProjectEntry],
    workspace_root: &Path,
    path: &Path,
    relative_path: &Path,
    is_dir: bool,
) -> bool {
    if !explorer_context_relative_path_is_unambiguous(relative_path) {
        return false;
    }

    if !workspace_path_stays_within_root_lexically(workspace_root, path) {
        return false;
    }

    let expected_path = workspace_root.join(relative_path);
    if expected_path != path && !paths_match_lexically(&expected_path, path) {
        return false;
    }

    current_entries
        .iter()
        .any(|entry| entry.is_dir == is_dir && explorer_context_entry_matches_path(entry, path))
}

fn explorer_context_entry_matches_path(entry: &ProjectEntry, path: &Path) -> bool {
    entry.path == path || paths_match_lexically(&entry.path, path)
}

fn explorer_context_relative_path_is_unambiguous(path: &Path) -> bool {
    let mut has_normal_component = false;
    for component in path.components() {
        match component {
            Component::Normal(_) => has_normal_component = true,
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    has_normal_component
}

fn explorer_context_can_compare_with_selected(
    path: &Path,
    selected_compare_path: Option<&Path>,
    selected_known_openable: bool,
) -> bool {
    selected_known_openable
        && selected_compare_path
            .is_some_and(|selected| selected != path && !paths_match_lexically(selected, path))
}

#[cfg(test)]
pub(crate) fn explorer_context_path_known_openable(
    buffers: &[kuroya_core::TextBuffer],
    indexed_files: &[std::path::PathBuf],
    path: &Path,
    path_exists: impl FnOnce(&Path) -> bool,
) -> bool {
    crate::file_runtime::file_path_open_buffer_or_known_openable(
        buffers,
        indexed_files,
        path,
        path_exists,
    )
}

#[cfg(test)]
pub(crate) fn explorer_file_compare_context_action_labels(
    is_dir: bool,
    has_selected_compare: bool,
    selected_is_same: bool,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if !is_dir {
        labels.push("Select for Compare");
    }
    if !is_dir && has_selected_compare && !selected_is_same {
        labels.push("Compare with Selected");
    }
    labels
}

#[cfg(test)]
pub(crate) fn explorer_file_source_control_context_action_labels(
    status: Option<GitFileStatus>,
    has_unstaged_changes: bool,
    has_staged_changes: bool,
) -> Vec<&'static str> {
    let has_source_control_changes = status.is_some() || has_unstaged_changes || has_staged_changes;
    let has_head_revision = status.is_some_and(explorer_source_control_has_head_revision);
    let has_index_revision = status.is_some_and(|status| {
        explorer_source_control_has_index_revision(status, has_unstaged_changes, has_staged_changes)
    });
    let can_open_hunks = status != Some(GitFileStatus::Conflicted);
    let mut labels = Vec::new();

    if has_unstaged_changes {
        labels.push("Open Changes");
        labels.push("Copy Patch");
    }
    if has_staged_changes {
        labels.push("Open Staged Changes");
        labels.push("Copy Staged Patch");
    }
    if has_source_control_changes {
        labels.push("Reveal in Source Control");
        labels.push("Compare with HEAD");
    }
    if has_head_revision {
        labels.push("Open File at HEAD");
    }
    if has_index_revision {
        labels.push("Open File at Index");
    }
    if has_unstaged_changes && can_open_hunks {
        labels.push("Open Hunks");
    }
    if has_staged_changes && can_open_hunks {
        labels.push("Open Staged Hunks");
    }
    if has_unstaged_changes {
        labels.push("Stage Changes");
    }
    if has_staged_changes {
        labels.push("Unstage Changes");
    }
    if has_source_control_changes {
        labels.push("Discard Changes");
    }

    labels
}

fn explorer_source_control_has_head_revision(status: GitFileStatus) -> bool {
    !matches!(status, GitFileStatus::Added | GitFileStatus::Untracked)
}

fn explorer_source_control_has_index_revision(
    status: GitFileStatus,
    has_unstaged_changes: bool,
    has_staged_changes: bool,
) -> bool {
    let staged_index = has_staged_changes
        && !matches!(
            status,
            GitFileStatus::Deleted | GitFileStatus::Untracked | GitFileStatus::Conflicted
        );
    let unstaged_index = has_unstaged_changes
        && !matches!(
            status,
            GitFileStatus::Added | GitFileStatus::Untracked | GitFileStatus::Conflicted
        );
    staged_index || unstaged_index
}

#[cfg(test)]
mod tests {
    use super::{
        explorer_context_can_compare_with_selected, explorer_context_menu_path_is_current,
        explorer_context_path_display_label, explorer_context_path_display_label_cow,
    };
    use crate::path_display::DISPLAY_PATH_LABEL_MAX_CHARS;
    use kuroya_core::ProjectEntry;
    use std::{borrow::Cow, path::PathBuf};

    #[test]
    fn explorer_context_path_display_label_cow_borrows_clean_ascii_and_unicode_paths() {
        let ascii = PathBuf::from("src/main.rs");
        assert!(matches!(
            explorer_context_path_display_label_cow(&ascii),
            Cow::Borrowed("src/main.rs")
        ));

        let unicode = PathBuf::from("src/clean-\u{03bb}.rs");
        match explorer_context_path_display_label_cow(&unicode) {
            Cow::Borrowed(label) => assert_eq!(label, "src/clean-\u{03bb}.rs"),
            Cow::Owned(label) => panic!("expected borrowed label, got {label:?}"),
        }
    }

    #[test]
    fn explorer_context_path_display_label_cow_owns_dirty_truncated_and_fallback_output() {
        let raw = format!(
            "src/bad\nname\u{202e}/{}tail.rs",
            "very-long-segment-".repeat(DISPLAY_PATH_LABEL_MAX_CHARS)
        );
        let path = PathBuf::from(&raw);

        let label = explorer_context_path_display_label_cow(&path);

        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));
        assert!(label.contains("..."));
        assert!(label.chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS);
        assert!(matches!(label, Cow::Owned(_)));
        assert_eq!(path, PathBuf::from(raw));

        let fallback_path = PathBuf::from("\n\u{202e}");
        let fallback = explorer_context_path_display_label_cow(&fallback_path);
        assert_eq!(fallback, ".");
        assert!(matches!(fallback, Cow::Owned(_)));
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn explorer_context_path_display_label_cow_owns_display_string_fallback_paths() {
        let path = non_unicode_path();
        assert!(path.to_str().is_none());

        let label = explorer_context_path_display_label_cow(&path);

        assert_eq!(label.as_ref(), explorer_context_path_display_label(&path));
        assert!(matches!(label, Cow::Owned(_)));
        assert!(!label.is_empty());
        assert!(label.chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS);
    }

    #[test]
    fn explorer_context_path_display_label_wrapper_matches_cow_helper() {
        let cases = [
            PathBuf::from("src/main.rs"),
            PathBuf::from("src/clean-\u{03bb}.rs"),
            PathBuf::from("src/bad\nname\u{202e}/tail.rs"),
            PathBuf::from("a".repeat(DISPLAY_PATH_LABEL_MAX_CHARS + 1)),
            PathBuf::from("\n\u{202e}"),
        ];

        for path in cases {
            assert_eq!(
                explorer_context_path_display_label(&path),
                explorer_context_path_display_label_cow(&path).into_owned()
            );
        }
    }

    #[test]
    fn explorer_context_path_display_label_falls_back_for_blank_control_text() {
        assert_eq!(
            explorer_context_path_display_label(&PathBuf::from("\n\u{202e}")),
            "."
        );
    }

    #[test]
    fn explorer_context_compare_rejects_equivalent_selected_path() {
        let path = PathBuf::from("workspace").join("src/main.rs");
        let selected = PathBuf::from("workspace")
            .join("src")
            .join("..")
            .join("src/main.rs");

        assert!(!explorer_context_can_compare_with_selected(
            &path,
            Some(&selected),
            true
        ));
        assert!(explorer_context_can_compare_with_selected(
            &path,
            Some(&PathBuf::from("workspace").join("src/lib.rs")),
            true
        ));
        assert!(!explorer_context_can_compare_with_selected(
            &path,
            Some(&PathBuf::from("workspace").join("src/lib.rs")),
            false
        ));
        assert_eq!(
            selected,
            PathBuf::from("workspace")
                .join("src")
                .join("..")
                .join("src/main.rs")
        );
    }

    #[test]
    fn explorer_context_menu_path_current_requires_visible_workspace_row() {
        let root = PathBuf::from("workspace");
        let file = explorer_entry(&root, "src/main.rs", false);
        let folder = explorer_entry(&root, "src", true);
        let entries = vec![folder.clone(), file.clone()];

        assert!(explorer_context_menu_path_is_current(
            &entries,
            &root,
            &file.path,
            &file.relative_path,
            false
        ));
        assert!(!explorer_context_menu_path_is_current(
            &entries,
            &root,
            &root.join("src/missing.rs"),
            &PathBuf::from("src/missing.rs"),
            false
        ));
        assert!(!explorer_context_menu_path_is_current(
            &entries,
            &root,
            &file.path,
            &file.relative_path,
            true
        ));
    }

    #[test]
    fn explorer_context_menu_path_current_rejects_ambiguous_relative_paths() {
        let root = PathBuf::from("workspace");
        let file = explorer_entry(&root, "safe.rs", false);
        let entries = vec![file.clone()];

        assert!(!explorer_context_menu_path_is_current(
            &entries,
            &root,
            &file.path,
            &PathBuf::from("src").join("..").join("safe.rs"),
            false
        ));
        assert!(!explorer_context_menu_path_is_current(
            &entries,
            &root,
            &PathBuf::from("outside/safe.rs"),
            &file.relative_path,
            false
        ));
    }

    fn explorer_entry(root: &std::path::Path, relative: &str, is_dir: bool) -> ProjectEntry {
        let relative_path = PathBuf::from(relative);
        ProjectEntry {
            path: root.join(&relative_path),
            depth: relative_path.components().count().saturating_sub(1),
            relative_path,
            is_dir,
            ..Default::default()
        }
    }

    #[cfg(unix)]
    fn non_unicode_path() -> PathBuf {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};

        PathBuf::from(OsString::from_vec(vec![b'f', b'o', b'o', 0xff]))
    }

    #[cfg(windows)]
    fn non_unicode_path() -> PathBuf {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt};

        PathBuf::from(OsString::from_wide(&[
            b'f' as u16,
            b'o' as u16,
            b'o' as u16,
            0xd800,
        ]))
    }
}
