use super::SourceControlViewMode;
use crate::{
    path_display::{
        DISPLAY_PATH_LABEL_MAX_CHARS, compact_path, display_path_label_cow,
        sanitized_display_label_cow,
    },
    ui_text::{count_label, truncate_middle},
};
use kuroya_core::GitUntrackedChanges;
use std::{borrow::Cow, path::Path};

pub(crate) const SOURCE_CONTROL_REF_LABEL_MAX_CHARS: usize = DISPLAY_PATH_LABEL_MAX_CHARS;

pub(crate) fn source_control_branch_display_label(branch: Option<&str>) -> String {
    source_control_branch_display_label_cow(branch).into_owned()
}

pub(super) fn source_control_branch_display_label_cow<'a>(branch: Option<&'a str>) -> Cow<'a, str> {
    source_control_ref_display_label_cow(branch.unwrap_or("detached"), "detached")
}

#[cfg(test)]
pub(super) fn source_control_ref_display_label(value: &str, fallback: &str) -> String {
    source_control_ref_display_label_cow(value, fallback).into_owned()
}

pub(super) fn source_control_ref_display_label_cow<'a>(
    value: &'a str,
    fallback: &str,
) -> Cow<'a, str> {
    sanitized_display_label_cow(value, SOURCE_CONTROL_REF_LABEL_MAX_CHARS, fallback)
}

pub(crate) fn source_control_repository_label(
    root: &Path,
    branch: Option<&str>,
    show_reference_details: bool,
) -> String {
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("repository");
    let name = source_control_ref_display_label_cow(name, "repository");
    if show_reference_details {
        let branch = source_control_branch_display_label_cow(branch);
        let mut label = String::with_capacity(name.len() + branch.len() + 3);
        label.push_str(&name);
        label.push_str(" (");
        label.push_str(&branch);
        label.push(')');
        match source_control_ref_display_label_cow(&label, "repository") {
            Cow::Borrowed(_) => label,
            Cow::Owned(label) => label,
        }
    } else {
        name.into_owned()
    }
}

pub(crate) fn source_control_empty_changes_label(
    raw_entry_count: usize,
    untracked_changes: GitUntrackedChanges,
) -> &'static str {
    if raw_entry_count > 0 && untracked_changes == GitUntrackedChanges::Hidden {
        "Untracked changes are hidden"
    } else {
        "No source control changes"
    }
}

pub(crate) fn source_control_filter_empty_label(query: &str) -> String {
    let query = query.trim();
    if query.is_empty() {
        "No matching changes".to_owned()
    } else {
        format!("No matching changes for \"{}\"", truncate_middle(query, 48))
    }
}

/// Renders a captured git scan failure for the source control panel. The
/// error text is already control-character sanitized and bounded at capture
/// time; this additionally trims it and applies the shared ref-label
/// sanitization so the panel never renders raw error output.
pub(crate) fn source_control_git_error_label(error: &str) -> String {
    let error = error.trim();
    if error.is_empty() {
        return "Git error".to_owned();
    }
    format!(
        "Git error: {}",
        source_control_ref_display_label_cow(error, "unknown error")
    )
}

pub(crate) fn source_control_result_count_label(
    total_count: usize,
    result_count: usize,
    query: &str,
) -> Option<String> {
    if query.trim().is_empty() {
        None
    } else {
        Some(format!(
            "{result_count} of {}",
            count_label(total_count, "change", "changes")
        ))
    }
}

#[cfg(test)]
pub(crate) fn source_control_display_path_label(
    root: &Path,
    path: &Path,
    view_mode: SourceControlViewMode,
    compact_folders: bool,
) -> String {
    source_control_display_path_label_cow(root, path, view_mode, compact_folders).into_owned()
}

pub(super) fn source_control_display_path_label_cow<'a>(
    root: &Path,
    path: &'a Path,
    view_mode: SourceControlViewMode,
    compact_folders: bool,
) -> Cow<'a, str> {
    match view_mode {
        SourceControlViewMode::List => source_control_display_compact_path_label_cow(root, path),
        SourceControlViewMode::Tree => {
            source_control_display_tree_path_label_cow(root, path, compact_folders)
        }
    }
}

fn source_control_display_compact_path_label_cow<'a>(root: &Path, path: &'a Path) -> Cow<'a, str> {
    display_path_label_cow(source_control_relative_path(root, path))
}

pub(super) fn source_control_status_path_label(path: &Path) -> String {
    source_control_status_path_label_cow(path).into_owned()
}

pub(super) fn source_control_status_path_label_cow(path: &Path) -> Cow<'_, str> {
    display_path_label_cow(path)
}

pub(super) fn source_control_sanitized_path_label_cow<'a>(label: &'a str) -> Cow<'a, str> {
    sanitized_display_label_cow(label, DISPLAY_PATH_LABEL_MAX_CHARS, ".")
}

pub(super) fn source_control_sanitized_path_label(label: &str) -> String {
    source_control_sanitized_path_label_cow(label).into_owned()
}

pub(super) fn source_control_sanitized_path_label_owned(label: String) -> String {
    let borrows_full_original = matches!(
        source_control_sanitized_path_label_cow(&label),
        Cow::Borrowed(borrowed) if borrowed.as_ptr() == label.as_ptr() && borrowed.len() == label.len()
    );
    if borrows_full_original {
        label
    } else {
        source_control_sanitized_path_label(&label)
    }
}

pub(super) fn source_control_display_tree_path_label(
    root: &Path,
    path: &Path,
    compact_folders: bool,
) -> String {
    source_control_display_tree_path_label_cow(root, path, compact_folders).into_owned()
}

fn source_control_display_tree_path_label_cow<'a>(
    root: &Path,
    path: &'a Path,
    compact_folders: bool,
) -> Cow<'a, str> {
    let relative = source_control_relative_path(root, path);
    if compact_folders {
        match source_control_tree_path_text_cow(relative) {
            Cow::Borrowed(label) => source_control_sanitized_path_label_cow(label),
            Cow::Owned(label) => Cow::Owned(source_control_sanitized_path_label_owned(label)),
        }
    } else {
        display_path_label_cow(relative)
    }
}

pub(super) fn source_control_tree_path_label(
    root: &Path,
    path: &Path,
    compact_folders: bool,
) -> String {
    let relative = source_control_relative_path(root, path);
    if compact_folders {
        source_control_tree_path_text(relative)
    } else {
        compact_path(relative)
    }
}

fn source_control_relative_path<'a>(root: &Path, path: &'a Path) -> &'a Path {
    path.strip_prefix(root)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())
        .unwrap_or(path)
}

fn source_control_tree_path_text(path: &Path) -> String {
    source_control_tree_path_text_cow(path).into_owned()
}

fn source_control_tree_path_text_cow(path: &Path) -> Cow<'_, str> {
    let label = path.to_string_lossy();
    if label.contains('\\') {
        Cow::Owned(label.replace('\\', "/"))
    } else {
        label
    }
}

pub(super) fn source_control_path_label(root: &Path, path: &Path) -> String {
    compact_path(source_control_relative_path(root, path))
}
