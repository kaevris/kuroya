use crate::{
    KuroyaApp,
    explorer::{ExplorerEntryKind, ExplorerOperationResult, path_matches_kind},
    path_display::sanitized_display_label_cow,
};
use std::{borrow::Cow, path::Path};

mod expanded;
pub(crate) use expanded::explorer_ancestor_paths;
#[cfg(test)]
pub(crate) use expanded::{
    clear_deleted_revealed_path, explorer_entry_visible_for, retarget_revealed_path,
};

pub(crate) const EXPLORER_OPERATION_PATH_LABEL_MAX_CHARS: usize = 160;
pub(crate) const EXPLORER_OPERATION_ERROR_DETAIL_MAX_CHARS: usize = 240;

pub(crate) fn explorer_operation_path_label(path: &Path) -> String {
    explorer_operation_path_label_text(path).into_owned()
}

pub(crate) fn explorer_operation_error_detail(error: &str) -> String {
    explorer_operation_error_detail_text(error).into_owned()
}

fn explorer_operation_path_label_text(path: &Path) -> Cow<'_, str> {
    match explorer_operation_path_text(path) {
        Cow::Borrowed(path) => {
            explorer_operation_status_text(path, EXPLORER_OPERATION_PATH_LABEL_MAX_CHARS, ".")
        }
        Cow::Owned(path) => {
            match explorer_operation_status_text(
                &path,
                EXPLORER_OPERATION_PATH_LABEL_MAX_CHARS,
                ".",
            ) {
                Cow::Borrowed(_) => Cow::Owned(path),
                Cow::Owned(label) => Cow::Owned(label),
            }
        }
    }
}

fn explorer_operation_error_detail_text(error: &str) -> Cow<'_, str> {
    explorer_operation_status_text(
        error,
        EXPLORER_OPERATION_ERROR_DETAIL_MAX_CHARS,
        "unknown error",
    )
}

impl KuroyaApp {
    pub(crate) fn reveal_active_file_in_explorer(&mut self) {
        let Some(id) = self.active else {
            self.status = "No active file to reveal".to_owned();
            return;
        };
        let path = self
            .buffer(id)
            .and_then(|buffer| buffer.path().cloned())
            .or_else(|| {
                self.diff_buffer_sources
                    .get(&id)
                    .map(|source| source.path.clone())
            });
        let Some(path) = path else {
            self.status = "No file-backed buffer to reveal".to_owned();
            return;
        };
        self.reveal_file_in_explorer(path);
    }

    pub(crate) fn apply_explorer_operation(&mut self, operation: ExplorerOperationResult) {
        match operation {
            ExplorerOperationResult::Created { path, kind } => {
                self.invalidate_explorer_directory_for_path(&path);
                self.expand_parent_of(&path);
                match kind {
                    ExplorerEntryKind::File => {
                        self.spawn_open_file(path.clone());
                        self.status = format!("Created {}", explorer_operation_path_label(&path));
                    }
                    ExplorerEntryKind::Folder => {
                        self.explorer_expanded.insert(path.clone());
                        self.status =
                            format!("Created folder {}", explorer_operation_path_label(&path));
                    }
                }
            }
            ExplorerOperationResult::Renamed {
                old_path,
                new_path,
                kind,
            } => {
                self.invalidate_explorer_directory_for_path(&old_path);
                self.invalidate_explorer_directory_for_path(&new_path);
                self.expand_parent_of(&new_path);
                if kind == ExplorerEntryKind::Folder {
                    self.retarget_expanded_paths(&old_path, &new_path);
                }
                self.retarget_revealed_path(&old_path, &new_path, kind);
                let retargeted = self.retarget_explorer_open_buffers(&old_path, &new_path, kind);
                let old_label = explorer_operation_path_label(&old_path);
                let new_label = explorer_operation_path_label(&new_path);
                self.status = if retargeted == 0 {
                    format!("Renamed {old_label} to {new_label}")
                } else {
                    format!(
                        "Renamed {old_label} to {new_label} and retargeted {retargeted} open buffers"
                    )
                };
            }
            ExplorerOperationResult::Deleted { path, kind } => {
                self.invalidate_explorer_directory_for_path(&path);
                self.explorer_expanded
                    .retain(|expanded| !path_matches_kind(expanded, &path, kind));
                self.clear_deleted_revealed_path(&path, kind);
                let (closed, retained_dirty) =
                    self.close_deleted_explorer_open_buffers(&path, kind);
                let path_label = explorer_operation_path_label(&path);
                self.status = match (closed, retained_dirty, kind) {
                    (0, 0, ExplorerEntryKind::File) => format!("Deleted {path_label}"),
                    (0, 0, ExplorerEntryKind::Folder) => format!("Deleted folder {path_label}"),
                    (_, 0, _) => {
                        format!("Deleted {path_label} and closed {closed} buffers")
                    }
                    (_, _, _) => format!(
                        "Deleted {path_label}, closed {closed} buffers, kept {retained_dirty} dirty buffers"
                    ),
                };
            }
        }

        self.spawn_index();
        self.spawn_git_auto_refresh();
    }
}

fn explorer_operation_status_text<'a>(
    text: &'a str,
    max_chars: usize,
    fallback: &str,
) -> Cow<'a, str> {
    sanitized_display_label_cow(text, max_chars, fallback)
}

fn explorer_operation_path_text(path: &Path) -> Cow<'_, str> {
    if path.as_os_str().is_empty() {
        return Cow::Borrowed(".");
    }

    path.file_name()
        .and_then(|name| name.to_str())
        .map(Cow::Borrowed)
        .unwrap_or_else(|| Cow::Owned(path.display().to_string()))
}

#[cfg(test)]
mod tests {
    use super::{
        EXPLORER_OPERATION_ERROR_DETAIL_MAX_CHARS, EXPLORER_OPERATION_PATH_LABEL_MAX_CHARS,
        explorer_operation_error_detail, explorer_operation_error_detail_text,
        explorer_operation_path_label, explorer_operation_path_label_text,
        explorer_operation_path_text, explorer_operation_status_text,
    };
    use std::{borrow::Cow, path::PathBuf};

    #[test]
    fn explorer_operation_cow_helpers_borrow_clean_ascii_and_unicode() {
        let ascii_path = PathBuf::from("workspace").join("clean.rs");

        assert!(matches!(
            explorer_operation_path_text(&ascii_path),
            Cow::Borrowed("clean.rs")
        ));
        assert!(matches!(
            explorer_operation_path_label_text(&ascii_path),
            Cow::Borrowed("clean.rs")
        ));
        assert!(matches!(
            explorer_operation_status_text("Created clean.rs", 64, "."),
            Cow::Borrowed("Created clean.rs")
        ));

        let unicode_name = "clean-\u{03bb}.rs";
        let unicode_path = PathBuf::from("workspace").join(unicode_name);
        match explorer_operation_path_label_text(&unicode_path) {
            Cow::Borrowed(label) => assert_eq!(label, unicode_name),
            Cow::Owned(label) => panic!("expected borrowed unicode path label, got {label:?}"),
        }

        let unicode_error = "failed cleanly: \u{03bb}";
        match explorer_operation_error_detail_text(unicode_error) {
            Cow::Borrowed(label) => assert_eq!(label, unicode_error),
            Cow::Owned(label) => panic!("expected borrowed unicode error detail, got {label:?}"),
        }
    }

    #[test]
    fn explorer_operation_cow_helpers_own_dirty_truncated_and_fallback_output() {
        let dirty_path = PathBuf::from("workspace").join("bad\n\u{202e}tail.rs");
        let dirty_label = explorer_operation_path_label_text(&dirty_path);

        assert!(matches!(&dirty_label, Cow::Owned(_)));
        assert!(!dirty_label.contains('\n'));
        assert!(!dirty_label.contains('\u{202e}'));

        let long_name = format!(
            "file{}",
            "-very-long".repeat(EXPLORER_OPERATION_PATH_LABEL_MAX_CHARS)
        );
        let long_path = PathBuf::from("workspace").join(long_name);
        let long_label = explorer_operation_path_label_text(&long_path);

        assert!(matches!(&long_label, Cow::Owned(_)));
        assert!(long_label.contains("..."));
        assert!(long_label.chars().count() <= EXPLORER_OPERATION_PATH_LABEL_MAX_CHARS);

        let fallback_path = PathBuf::from("\n\u{202e}\u{2029}");
        let fallback_label = explorer_operation_path_label_text(&fallback_path);

        assert!(matches!(&fallback_label, Cow::Owned(_)));
        assert_eq!(fallback_label, ".");

        let fallback_detail = explorer_operation_error_detail_text("\n\u{202e}\u{2029}");

        assert!(matches!(&fallback_detail, Cow::Owned(_)));
        assert_eq!(fallback_detail, "unknown error");

        let long_detail = format!(
            "first line{}",
            "-very-long".repeat(EXPLORER_OPERATION_ERROR_DETAIL_MAX_CHARS)
        );
        let truncated_detail = explorer_operation_error_detail_text(&long_detail);

        assert!(matches!(&truncated_detail, Cow::Owned(_)));
        assert!(truncated_detail.contains("..."));
        assert!(truncated_detail.chars().count() <= EXPLORER_OPERATION_ERROR_DETAIL_MAX_CHARS);
    }

    #[test]
    fn explorer_operation_string_wrappers_match_cow_helpers() {
        let paths = [
            PathBuf::from("workspace").join("clean.rs"),
            PathBuf::from("workspace").join("clean-\u{03bb}.rs"),
            PathBuf::from("workspace").join("bad\n\u{202e}tail.rs"),
            PathBuf::from("\n\u{202e}\u{2029}"),
            PathBuf::new(),
        ];

        for path in paths {
            assert_eq!(
                explorer_operation_path_label(&path),
                explorer_operation_path_label_text(&path).into_owned()
            );
        }

        for detail in [
            "clean failure",
            "clean failure \u{03bb}",
            "bad\n\u{202e}detail",
            "\n\u{202e}\u{2029}",
        ] {
            assert_eq!(
                explorer_operation_error_detail(detail),
                explorer_operation_error_detail_text(detail).into_owned()
            );
        }
    }

    #[test]
    fn explorer_operation_path_labels_sanitize_hostile_text_and_bound_length() {
        let path = PathBuf::from("workspace").join(format!(
            "bad\n\u{202e}tail\u{2028}{}",
            "-very-long".repeat(EXPLORER_OPERATION_PATH_LABEL_MAX_CHARS)
        ));

        let label = explorer_operation_path_label(&path);

        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));
        assert!(!label.contains('\u{2028}'));
        assert!(label.contains("..."));
        assert!(label.chars().count() <= EXPLORER_OPERATION_PATH_LABEL_MAX_CHARS);
    }

    #[test]
    fn explorer_operation_error_details_sanitize_hostile_text_and_bound_length() {
        let detail = explorer_operation_error_detail(&format!(
            "first line\nsecond line\u{202e}\u{2029}{}",
            "-very-long".repeat(EXPLORER_OPERATION_ERROR_DETAIL_MAX_CHARS)
        ));

        assert!(!detail.contains('\n'));
        assert!(!detail.contains('\u{202e}'));
        assert!(!detail.contains('\u{2029}'));
        assert!(detail.contains("..."));
        assert!(detail.chars().count() <= EXPLORER_OPERATION_ERROR_DETAIL_MAX_CHARS);
    }

    #[test]
    fn explorer_operation_labels_use_blank_fallbacks() {
        assert_eq!(explorer_operation_path_label(&PathBuf::new()), ".");
        assert_eq!(
            explorer_operation_error_detail("\n\u{202e}\u{2029}"),
            "unknown error"
        );
    }

    #[test]
    fn explorer_operation_path_label_preserves_raw_pathbuf() {
        let raw_name = "raw\n\u{202e}name.rs";
        let path = PathBuf::from("workspace").join(raw_name);
        let original = path.clone();

        let label = explorer_operation_path_label(&path);

        assert_eq!(path, original);
        assert!(path.as_os_str().to_string_lossy().contains('\n'));
        assert!(path.as_os_str().to_string_lossy().contains('\u{202e}'));
        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));
    }
}
