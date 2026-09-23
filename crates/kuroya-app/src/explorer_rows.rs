#[cfg(test)]
use crate::path_display::sanitized_display_label;
#[cfg(test)]
use crate::workspace_trust::{trusted_workspace_paths_match, workspace_path_contains_lexically};
use crate::{
    file_runtime::file_path_open_buffer_or_known_openable,
    path_display::sanitized_display_label_cow,
    ui_icons::{IconKind, draw_icon},
    workspace_state::paths_match_lexically,
};
use egui::{self, Color32, FontId, Sense, TextStyle, WidgetInfo, WidgetType, pos2, vec2};
use kuroya_core::{GitFileStatus, GitStatusEntry, ProjectEntry, TextBuffer};
use std::{
    borrow::Cow,
    collections::{HashMap, hash_map::Entry},
    ffi::{OsStr, OsString},
    path::{Component, Path, PathBuf},
};

pub(crate) const EXPLORER_ROW_HEIGHT: f32 = 30.0;
const EXPLORER_ROW_INDENT_WIDTH: f32 = 12.0;
const EXPLORER_ROW_MAX_INDENT: f32 = 240.0;
const EXPLORER_ROW_MAX_LABEL_CHARS: usize = 160;
const EXPLORER_ROW_SANITIZE_INPUT_MAX_BYTES: usize = 4096;
const EXPLORER_ROW_SANITIZE_SAMPLE_EXTRA_CHARS: usize = 32;
const EXPLORER_ROW_OPENABILITY_CACHE_LIMIT: usize = 128;

pub(crate) fn file_icon(path: &Path) -> IconKind {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
    {
        "rs" | "toml" | "json" | "py" => IconKind::Code,
        "md" => IconKind::File,
        _ => IconKind::File,
    }
}

pub(crate) fn explorer_prepared_entry_row(
    ui: &mut egui::Ui,
    row: &ExplorerPreparedRow<'_>,
) -> egui::Response {
    render_explorer_entry_row(
        ui,
        ExplorerEntryRowDisplay {
            depth: row.depth,
            is_dir: row.is_dir,
            expanded: row.expanded,
            selected: row.selected,
            git_decoration: row.git_decoration,
            icon: row.icon,
            display_name: row.display_name.as_ref(),
            accessibility_label: row.accessibility_label.as_str(),
        },
    )
}

struct ExplorerEntryRowDisplay<'a> {
    depth: usize,
    is_dir: bool,
    expanded: bool,
    selected: bool,
    git_decoration: Option<ExplorerGitDecoration>,
    icon: IconKind,
    display_name: &'a str,
    accessibility_label: &'a str,
}

fn render_explorer_entry_row(
    ui: &mut egui::Ui,
    row: ExplorerEntryRowDisplay<'_>,
) -> egui::Response {
    let width = ui.available_width().max(120.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, EXPLORER_ROW_HEIGHT), Sense::click());
    let visuals = ui.visuals();

    let fill = if row.selected {
        visuals.widgets.active.bg_fill
    } else if response.hovered() {
        visuals.widgets.hovered.bg_fill
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter()
            .rect_filled(rect.shrink2(vec2(2.0, 1.0)), 4.0, fill);
    }

    let icon_tint = visuals.widgets.inactive.fg_stroke.color;
    let text_color = if row.selected {
        visuals.text_color()
    } else {
        icon_tint
    };
    let mut x = rect.left() + 6.0 + explorer_entry_indent(row.depth);
    let center_y = rect.center().y;

    if row.is_dir {
        let chevron = if row.expanded {
            IconKind::ChevronDown
        } else {
            IconKind::ChevronRight
        };
        draw_icon(
            ui,
            egui::Rect::from_center_size(pos2(x + 7.0, center_y), vec2(14.0, 14.0)),
            chevron,
            icon_tint,
        );
    }
    x += 16.0;

    if let Some(decoration) = row.git_decoration {
        ui.painter().text(
            pos2(x + 4.0, center_y),
            egui::Align2::CENTER_CENTER,
            decoration.marker,
            FontId::monospace(10.5),
            decoration.color,
        );
    }
    x += 14.0;

    draw_icon(
        ui,
        egui::Rect::from_center_size(pos2(x + 10.0, center_y), vec2(20.0, 20.0)),
        row.icon,
        icon_tint,
    );
    x += 28.0;

    ui.painter().text(
        pos2(x, center_y),
        egui::Align2::LEFT_CENTER,
        row.display_name,
        TextStyle::Body.resolve(ui.style()),
        text_color,
    );

    response.widget_info(|| {
        WidgetInfo::selected(
            WidgetType::SelectableLabel,
            ui.is_enabled(),
            row.selected,
            row.accessibility_label,
        )
    });

    response
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ExplorerRowPreparationInput<'a> {
    pub(crate) entry: &'a ProjectEntry,
    pub(crate) expanded: bool,
    pub(crate) selected: bool,
    pub(crate) git_decoration: Option<ExplorerGitDecoration>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExplorerPreparedRow<'a> {
    pub(crate) path: &'a Path,
    pub(crate) relative_path: &'a Path,
    pub(crate) depth: usize,
    pub(crate) is_dir: bool,
    pub(crate) expanded: bool,
    pub(crate) selected: bool,
    pub(crate) git_decoration: Option<ExplorerGitDecoration>,
    pub(crate) icon: IconKind,
    pub(crate) display_name: Cow<'a, str>,
    pub(crate) hover_path: Cow<'a, str>,
    pub(crate) accessibility_label: String,
}

impl<'a> ExplorerPreparedRow<'a> {
    #[cfg(test)]
    pub(crate) fn accessibility_label(&self) -> &str {
        self.accessibility_label.as_str()
    }
}

pub(crate) fn prepare_explorer_row<'a>(
    input: ExplorerRowPreparationInput<'a>,
) -> ExplorerPreparedRow<'a> {
    let path = input.entry.path.as_path();
    let relative_path = input.entry.relative_path.as_path();
    let is_dir = input.entry.is_dir;
    let expanded = is_dir && input.expanded;
    let name = explorer_entry_relative_name(relative_path);
    let display_name = explorer_entry_display_name_text(name);
    let accessibility_label = explorer_entry_accessibility_label(
        display_name.as_ref(),
        is_dir,
        expanded,
        input.git_decoration,
    );
    let hover_path = explorer_entry_hover_path_display_name(relative_path, &display_name);

    ExplorerPreparedRow {
        path,
        relative_path,
        depth: input.entry.depth,
        is_dir,
        expanded,
        selected: input.selected,
        git_decoration: input.git_decoration,
        icon: explorer_entry_icon(path, is_dir, expanded),
        display_name,
        hover_path,
        accessibility_label,
    }
}

pub(crate) fn explorer_entry_icon(path: &Path, is_dir: bool, expanded: bool) -> IconKind {
    if is_dir {
        if expanded {
            IconKind::FolderOpen
        } else {
            IconKind::Folder
        }
    } else {
        file_icon(path)
    }
}

fn explorer_entry_relative_name(relative_path: &Path) -> &str {
    relative_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_else(|| relative_path.to_str().unwrap_or("?"))
}

pub(crate) fn explorer_entry_display_name(name: &str) -> String {
    explorer_entry_display_name_text(name).into_owned()
}

fn explorer_entry_display_name_text(name: &str) -> Cow<'_, str> {
    match explorer_entry_name_sanitizer_input(name) {
        Cow::Borrowed(name) => sanitized_display_label_cow(name, EXPLORER_ROW_MAX_LABEL_CHARS, "."),
        Cow::Owned(name) => Cow::Owned(explorer_entry_owned_display_name(name)),
    }
}

fn explorer_entry_owned_display_name(name: String) -> String {
    match sanitized_display_label_cow(&name, EXPLORER_ROW_MAX_LABEL_CHARS, ".") {
        Cow::Borrowed(label) if label.as_ptr() == name.as_ptr() && label.len() == name.len() => {
            name
        }
        Cow::Borrowed(label) => label.to_owned(),
        Cow::Owned(label) => label,
    }
}

fn explorer_entry_name_sanitizer_input(name: &str) -> Cow<'_, str> {
    if name.len() <= EXPLORER_ROW_SANITIZE_INPUT_MAX_BYTES {
        return Cow::Borrowed(name);
    }

    let keep = EXPLORER_ROW_MAX_LABEL_CHARS.saturating_sub(3);
    let head_chars = keep / 2 + EXPLORER_ROW_SANITIZE_SAMPLE_EXTRA_CHARS;
    let tail_chars = keep.saturating_sub(keep / 2) + EXPLORER_ROW_SANITIZE_SAMPLE_EXTRA_CHARS;
    let head_end = name
        .char_indices()
        .nth(head_chars)
        .map_or(name.len(), |(index, _)| index);
    if head_end == name.len() {
        return Cow::Borrowed(name);
    }

    let tail_start = name
        .char_indices()
        .rev()
        .nth(tail_chars.saturating_sub(1))
        .map_or(name.len(), |(index, _)| index);
    if tail_start <= head_end {
        return Cow::Borrowed(name);
    }

    let mut sample = String::with_capacity(head_end + 3 + name.len().saturating_sub(tail_start));
    sample.push_str(&name[..head_end]);
    sample.push_str("...");
    sample.push_str(&name[tail_start..]);
    Cow::Owned(sample)
}

pub(crate) fn explorer_entry_path_display_name(path: &Path) -> String {
    if let Some(path) = path.to_str() {
        explorer_entry_display_name(path)
    } else {
        explorer_entry_display_name(&path.display().to_string())
    }
}

fn explorer_entry_hover_path_display_name<'a>(
    relative_path: &'a Path,
    display_name: &Cow<'a, str>,
) -> Cow<'a, str> {
    if explorer_entry_path_has_single_component(relative_path) {
        display_name.clone()
    } else {
        Cow::Owned(explorer_entry_path_display_name(relative_path))
    }
}

fn explorer_entry_path_has_single_component(path: &Path) -> bool {
    let mut components = path.components();
    components.next().is_some() && components.next().is_none()
}

pub(crate) fn explorer_entry_accessibility_label(
    name: &str,
    is_dir: bool,
    expanded: bool,
    git_decoration: Option<ExplorerGitDecoration>,
) -> String {
    let prefix = if is_dir { "Folder " } else { "File " };
    let mut label = String::with_capacity(
        prefix.len()
            + name.len()
            + usize::from(is_dir) * ", collapsed".len()
            + git_decoration.map_or(0, |decoration| 2 + decoration.label.len()),
    );
    label.push_str(prefix);
    label.push_str(name);
    if is_dir {
        label.push_str(if expanded {
            ", expanded"
        } else {
            ", collapsed"
        });
    }
    if let Some(decoration) = git_decoration {
        label.push_str(", ");
        label.push_str(decoration.label);
    }
    label
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExplorerGitDecoration {
    pub(crate) marker: &'static str,
    pub(crate) label: &'static str,
    pub(crate) color: Color32,
}

#[derive(Debug, Default)]
pub(crate) struct ExplorerGitDecorations {
    files: HashMap<PathBuf, GitFileStatus>,
    folders: HashMap<PathBuf, GitFileStatus>,
}

impl ExplorerGitDecorations {
    pub(crate) fn from_entries(root: &Path, entries: &[GitStatusEntry], enabled: bool) -> Self {
        if !enabled || entries.is_empty() {
            return Self::default();
        }

        let root_key = explorer_git_path_key(root);
        let mut decorations = Self {
            files: HashMap::with_capacity(entries.len()),
            folders: HashMap::new(),
        };
        for entry in entries {
            let path_key = explorer_git_path_key(&entry.path);
            record_git_status(&mut decorations.files, path_key.clone(), entry.status);
            decorations.record_parent_folders(&root_key, &path_key, entry.status);
        }
        decorations
    }

    pub(crate) fn decoration_for_path(
        &self,
        path: &Path,
        is_dir: bool,
    ) -> Option<ExplorerGitDecoration> {
        if self.files.is_empty() && self.folders.is_empty() {
            return None;
        }

        let path = explorer_git_path_key(path);
        let status = if is_dir {
            self.folders.get(&path)
        } else {
            self.files.get(&path)
        };
        status.copied().map(explorer_git_decoration_for_status)
    }

    fn record_parent_folders(&mut self, root_key: &Path, path_key: &Path, status: GitFileStatus) {
        let Ok(relative) = path_key.strip_prefix(root_key) else {
            return;
        };
        let ancestor_count = relative.components().count().saturating_sub(1);
        if ancestor_count == 0 {
            return;
        }

        let mut current = root_key.to_path_buf();
        for component in relative.components().take(ancestor_count) {
            current.push(component.as_os_str());
            record_git_status(&mut self.folders, current.clone(), status);
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct ExplorerRowOpenabilityCache {
    paths: HashMap<PathBuf, bool>,
}

impl ExplorerRowOpenabilityCache {
    pub(crate) fn path_known_openable(
        &mut self,
        buffers: &[TextBuffer],
        indexed_files: &[PathBuf],
        path: &Path,
        path_exists: impl FnOnce(&Path) -> bool,
    ) -> bool {
        let key = explorer_row_openability_path_key(path);
        if let Some(openable) = self.paths.get(&key) {
            return *openable;
        }

        let openable =
            file_path_open_buffer_or_known_openable(buffers, indexed_files, path, path_exists);
        if self.paths.len() >= EXPLORER_ROW_OPENABILITY_CACHE_LIMIT {
            self.paths.clear();
        }
        self.paths.insert(key, openable);
        openable
    }

    pub(crate) fn row_openability(
        &mut self,
        buffers: &[TextBuffer],
        indexed_files: &[PathBuf],
        path: &Path,
        selected_compare_path: Option<&Path>,
        is_dir: bool,
        mut path_exists: impl FnMut(&Path) -> bool,
    ) -> ExplorerRowOpenability {
        if is_dir {
            return ExplorerRowOpenability::default();
        }

        let can_compare_with_selected = if let Some(selected) = selected_compare_path {
            selected != path
                && !paths_match_lexically(selected, path)
                && self
                    .path_known_openable(buffers, indexed_files, selected, |path| path_exists(path))
        } else {
            false
        };
        let can_open_blame =
            self.path_known_openable(buffers, indexed_files, path, |path| path_exists(path));

        ExplorerRowOpenability {
            can_compare_with_selected,
            can_open_blame,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ExplorerRowOpenability {
    pub(crate) can_compare_with_selected: bool,
    pub(crate) can_open_blame: bool,
}

fn explorer_row_openability_path_key(path: &Path) -> PathBuf {
    let mut key = PathBuf::new();
    let mut has_root = false;
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                key.push(explorer_row_openability_component_key(prefix.as_os_str()));
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
                key.push(explorer_row_openability_component_key(component));
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
fn explorer_row_openability_component_key(component: &OsStr) -> OsString {
    component.to_string_lossy().to_lowercase().into()
}

#[cfg(not(windows))]
fn explorer_row_openability_component_key(component: &OsStr) -> OsString {
    component.to_os_string()
}

#[cfg(test)]
pub(crate) fn explorer_git_decoration_for_path(
    path: &Path,
    is_dir: bool,
    entries: &[GitStatusEntry],
    enabled: bool,
) -> Option<ExplorerGitDecoration> {
    if !enabled {
        return None;
    }
    explorer_git_status_for_path(path, is_dir, entries).map(explorer_git_decoration_for_status)
}

#[cfg(test)]
pub(crate) fn explorer_git_status_for_path(
    path: &Path,
    is_dir: bool,
    entries: &[GitStatusEntry],
) -> Option<GitFileStatus> {
    let mut best = None;
    let mut best_priority = 0;
    for entry in entries {
        let matches = entry.path == path
            || if is_dir {
                workspace_path_contains_lexically(path, &entry.path)
            } else {
                trusted_workspace_paths_match(&entry.path, path)
            };
        if !matches {
            continue;
        }

        let priority = git_status_priority(entry.status);
        if priority > best_priority {
            best = Some(entry.status);
            best_priority = priority;
            if entry.status == GitFileStatus::Conflicted {
                break;
            }
        }
    }
    best
}

fn explorer_git_decoration_for_status(status: GitFileStatus) -> ExplorerGitDecoration {
    ExplorerGitDecoration {
        marker: git_status_marker(status),
        label: git_status_label(status),
        color: git_status_color(status),
    }
}

fn git_status_marker(status: GitFileStatus) -> &'static str {
    match status {
        GitFileStatus::Modified => "M",
        GitFileStatus::Added => "A",
        GitFileStatus::Deleted => "D",
        GitFileStatus::Renamed => "R",
        GitFileStatus::Untracked => "?",
        GitFileStatus::Conflicted => "!",
    }
}

fn git_status_label(status: GitFileStatus) -> &'static str {
    match status {
        GitFileStatus::Modified => "Modified",
        GitFileStatus::Added => "Added",
        GitFileStatus::Deleted => "Deleted",
        GitFileStatus::Renamed => "Renamed",
        GitFileStatus::Untracked => "Untracked",
        GitFileStatus::Conflicted => "Conflicted",
    }
}

fn git_status_color(status: GitFileStatus) -> Color32 {
    match status {
        GitFileStatus::Added | GitFileStatus::Untracked => Color32::from_rgb(89, 168, 105),
        GitFileStatus::Deleted => Color32::from_rgb(224, 108, 117),
        GitFileStatus::Renamed => Color32::from_rgb(86, 156, 214),
        GitFileStatus::Conflicted => Color32::from_rgb(244, 191, 117),
        GitFileStatus::Modified => Color32::from_rgb(220, 220, 170),
    }
}

fn git_status_priority(status: GitFileStatus) -> usize {
    match status {
        GitFileStatus::Conflicted => 6,
        GitFileStatus::Deleted => 5,
        GitFileStatus::Renamed => 4,
        GitFileStatus::Added => 3,
        GitFileStatus::Untracked => 2,
        GitFileStatus::Modified => 1,
    }
}

fn record_git_status(
    statuses: &mut HashMap<PathBuf, GitFileStatus>,
    path: PathBuf,
    status: GitFileStatus,
) {
    match statuses.entry(path) {
        Entry::Vacant(entry) => {
            entry.insert(status);
        }
        Entry::Occupied(mut entry) => {
            if git_status_priority(status) > git_status_priority(*entry.get()) {
                entry.insert(status);
            }
        }
    }
}

fn explorer_git_path_key(path: &Path) -> PathBuf {
    let mut key = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => key.push(explorer_git_component_key(prefix.as_os_str())),
            Component::RootDir => key.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !key.pop() {
                    key.push("..");
                }
            }
            Component::Normal(component) => key.push(explorer_git_component_key(component)),
        }
    }

    if key.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        key
    }
}

#[cfg(windows)]
fn explorer_git_component_key(component: &OsStr) -> OsString {
    component.to_string_lossy().to_lowercase().into()
}

#[cfg(not(windows))]
fn explorer_git_component_key(component: &OsStr) -> OsString {
    component.to_os_string()
}

fn explorer_entry_indent(depth: usize) -> f32 {
    (depth as f32 * EXPLORER_ROW_INDENT_WIDTH).min(EXPLORER_ROW_MAX_INDENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuroya_core::{GitChangeStage, GitFileStatus, GitStatusEntry};
    use std::{cell::Cell, path::PathBuf};

    #[test]
    fn explorer_entry_display_name_is_single_line_and_bounded() {
        let name = format!("main  name\n\t\u{202e}\u{2028}.rs{}", "x".repeat(220));

        let display = explorer_entry_display_name(&name);

        assert!(display.starts_with("main  name .rs"));
        assert!(display.contains("..."));
        assert!(!display.contains('\n'));
        assert!(!display.contains('\t'));
        assert!(!display.contains('\u{202e}'));
        assert!(!display.contains('\u{2028}'));
        assert!(display.chars().count() <= EXPLORER_ROW_MAX_LABEL_CHARS);
    }

    #[test]
    fn explorer_entry_display_name_falls_back_for_blank_control_names() {
        assert_eq!(explorer_entry_display_name("\n\t\u{202e}\u{2066}"), ".");
        assert_eq!(explorer_entry_display_name("   \u{2029}"), ".");
    }

    #[test]
    fn explorer_entry_display_name_sanitizes_hostile_names() {
        let display = explorer_entry_display_name("bad\n\t\u{202e}\u{2066}name\u{2028}.rs");

        assert_eq!(display, "bad name .rs");
    }

    #[test]
    fn explorer_entry_display_name_text_borrows_clean_ascii_and_unicode() {
        let ascii = "clean-name.rs";
        assert!(matches!(
            explorer_entry_display_name_text(ascii),
            Cow::Borrowed("clean-name.rs")
        ));

        let unicode = "clean-\u{03bb}.rs";
        match explorer_entry_display_name_text(unicode) {
            Cow::Borrowed(label) => assert_eq!(label, unicode),
            Cow::Owned(label) => panic!("expected borrowed label, got {label:?}"),
        }
    }

    #[test]
    fn explorer_entry_display_name_text_owns_dirty_truncated_and_fallback_names() {
        let long = "abcdefghijklmnopqrstuvwxyz".repeat(8);
        let cases = [
            "  clean.rs  ",
            "bad\n\tname.rs",
            "\u{200b}alpha.rs",
            long.as_str(),
            "\n\t\u{202e}\u{2066}",
        ];

        for name in cases {
            let display = explorer_entry_display_name_text(name);

            assert_eq!(
                display.as_ref(),
                sanitized_display_label(name, EXPLORER_ROW_MAX_LABEL_CHARS, ".")
            );
            assert!(
                matches!(display, Cow::Owned(_)),
                "expected owned display name for {name:?}"
            );
        }
    }

    #[test]
    fn explorer_entry_display_name_bounds_huge_sanitizer_inputs() {
        let name = format!(
            "prefix-{}mid\n\t\u{202e}{}-final.rs",
            "x".repeat(EXPLORER_ROW_SANITIZE_INPUT_MAX_BYTES * 2),
            "y".repeat(EXPLORER_ROW_SANITIZE_INPUT_MAX_BYTES * 2)
        );

        let sanitizer_input = explorer_entry_name_sanitizer_input(&name);
        let display = explorer_entry_display_name_text(&name);

        assert!(matches!(sanitizer_input, Cow::Owned(_)));
        assert!(sanitizer_input.len() < EXPLORER_ROW_SANITIZE_INPUT_MAX_BYTES);
        assert!(display.starts_with("prefix-"));
        assert!(display.ends_with("-final.rs"));
        assert!(display.contains("..."));
        assert!(!display.contains('\n'));
        assert!(!display.contains('\t'));
        assert!(!display.contains('\u{202e}'));
        assert!(display.chars().count() <= EXPLORER_ROW_MAX_LABEL_CHARS);
        assert_eq!(explorer_entry_display_name(&name), display.as_ref());
        assert!(matches!(display, Cow::Owned(_)));
    }

    #[test]
    fn explorer_entry_accessibility_label_sanitizes_display_name() {
        let label = explorer_entry_accessibility_label(
            &explorer_entry_display_name("bad\n\u{202e}name\u{2029}.rs"),
            false,
            false,
            None,
        );

        assert_eq!(label, "File bad name .rs");
        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));
        assert!(!label.contains('\u{2029}'));
    }

    #[test]
    fn explorer_entry_indent_is_capped_for_deep_rows() {
        assert_eq!(explorer_entry_indent(2), EXPLORER_ROW_INDENT_WIDTH * 2.0);
        assert_eq!(explorer_entry_indent(usize::MAX), EXPLORER_ROW_MAX_INDENT);
    }

    #[test]
    fn explorer_entry_path_has_single_component_matches_row_names() {
        assert!(explorer_entry_path_has_single_component(Path::new(
            "README.md"
        )));
        assert!(!explorer_entry_path_has_single_component(Path::new("")));
        assert!(!explorer_entry_path_has_single_component(Path::new(
            "src/main.rs"
        )));
    }

    #[test]
    fn prepare_explorer_row_preserves_raw_paths_and_sanitizes_display_snapshot() {
        let relative_path = PathBuf::from("src").join("bad\n\u{202e}name\u{2028}.rs");
        let raw_path = PathBuf::from("workspace").join(&relative_path);
        let entry = ProjectEntry {
            path: raw_path.clone(),
            relative_path: relative_path.clone(),
            is_dir: false,
            depth: 1,
            ..Default::default()
        };
        let git_decoration = Some(explorer_git_decoration_for_status(GitFileStatus::Modified));

        let row = prepare_explorer_row(ExplorerRowPreparationInput {
            entry: &entry,
            expanded: true,
            selected: true,
            git_decoration,
        });

        assert_eq!(row.path, entry.path.as_path());
        assert_eq!(row.relative_path, entry.relative_path.as_path());
        assert_eq!(entry.path, raw_path);
        assert_eq!(entry.relative_path, relative_path);
        assert_eq!(row.depth, 1);
        assert!(!row.expanded);
        assert!(row.selected);
        assert_eq!(row.icon, IconKind::Code);
        assert_eq!(row.display_name.as_ref(), "bad name .rs");
        assert!(!row.hover_path.contains('\n'));
        assert!(!row.hover_path.contains('\u{202e}'));
        assert_eq!(row.accessibility_label(), "File bad name .rs, Modified");
    }

    #[test]
    fn prepare_explorer_row_snapshots_folder_icon_from_expansion_state() {
        let root = PathBuf::from("workspace");
        let entry = ProjectEntry {
            path: root.join("src"),
            relative_path: PathBuf::from("src"),
            is_dir: true,
            depth: 0,
            ..Default::default()
        };

        let collapsed = prepare_explorer_row(ExplorerRowPreparationInput {
            entry: &entry,
            expanded: false,
            selected: false,
            git_decoration: None,
        });
        let expanded = prepare_explorer_row(ExplorerRowPreparationInput {
            entry: &entry,
            expanded: true,
            selected: false,
            git_decoration: None,
        });

        assert_eq!(collapsed.icon, IconKind::Folder);
        assert_eq!(collapsed.accessibility_label(), "Folder src, collapsed");
        assert_eq!(expanded.icon, IconKind::FolderOpen);
        assert_eq!(expanded.accessibility_label(), "Folder src, expanded");
    }

    #[test]
    fn prepare_explorer_row_reuses_single_component_display_for_hover() {
        let entry = ProjectEntry {
            path: PathBuf::from("workspace").join("README.md"),
            relative_path: PathBuf::from("README.md"),
            is_dir: false,
            depth: 0,
            ..Default::default()
        };

        let row = prepare_explorer_row(ExplorerRowPreparationInput {
            entry: &entry,
            expanded: false,
            selected: false,
            git_decoration: None,
        });

        assert_eq!(row.accessibility_label, "File README.md");
        assert!(matches!(row.display_name, Cow::Borrowed("README.md")));
        assert!(matches!(row.hover_path, Cow::Borrowed("README.md")));
    }

    #[test]
    fn prepare_explorer_row_bounds_nested_display_fields() {
        let relative_path = PathBuf::from("src").join(format!(
            "bad\n{}\u{202e}tail.rs",
            "very-long-segment-".repeat(EXPLORER_ROW_MAX_LABEL_CHARS)
        ));
        let entry = ProjectEntry {
            path: PathBuf::from("workspace").join(&relative_path),
            relative_path,
            is_dir: false,
            depth: 1,
            ..Default::default()
        };

        let row = prepare_explorer_row(ExplorerRowPreparationInput {
            entry: &entry,
            expanded: true,
            selected: false,
            git_decoration: Some(explorer_git_decoration_for_status(GitFileStatus::Added)),
        });

        for value in [
            row.display_name.as_ref(),
            row.hover_path.as_ref(),
            row.accessibility_label.as_str(),
        ] {
            assert!(!value.contains('\n'));
            assert!(!value.contains('\u{202e}'));
            assert!(value.contains("..."));
        }
        assert!(row.display_name.chars().count() <= EXPLORER_ROW_MAX_LABEL_CHARS);
        assert!(row.hover_path.chars().count() <= EXPLORER_ROW_MAX_LABEL_CHARS);
        assert!(
            row.accessibility_label.chars().count()
                <= "File ".len() + EXPLORER_ROW_MAX_LABEL_CHARS + ", Added".len()
        );
        assert!(matches!(row.hover_path, Cow::Owned(_)));
    }

    #[test]
    fn explorer_row_openability_cache_reuses_path_probe_results() {
        let mut cache = ExplorerRowOpenabilityCache::default();
        let path = PathBuf::from("workspace/src/main.rs");
        let probes = Cell::new(0);

        assert!(cache.path_known_openable(&[], &[], &path, |_| {
            probes.set(probes.get() + 1);
            true
        }));
        assert!(cache.path_known_openable(&[], &[], &path, |_| {
            panic!("cached explorer path should not probe again")
        }));

        assert_eq!(probes.get(), 1);
    }

    #[test]
    fn explorer_row_openability_cache_reuses_lexically_equivalent_probe_results() {
        let mut cache = ExplorerRowOpenabilityCache::default();
        let raw_path = PathBuf::from("workspace/src")
            .join("..")
            .join("src/missing.rs");
        let equivalent_path = PathBuf::from("workspace/src/missing.rs");
        let probes = Cell::new(0);

        assert!(!cache.path_known_openable(&[], &[], &raw_path, |_| {
            probes.set(probes.get() + 1);
            false
        }));
        assert!(!cache.path_known_openable(&[], &[], &equivalent_path, |_| {
            panic!("equivalent cached explorer path should not probe again")
        }));

        assert_eq!(probes.get(), 1);
        assert_eq!(
            raw_path,
            PathBuf::from("workspace/src")
                .join("..")
                .join("src/missing.rs")
        );
    }

    #[test]
    fn explorer_row_openability_uses_known_paths_before_probe() {
        let mut cache = ExplorerRowOpenabilityCache::default();
        let path = PathBuf::from("workspace/src/main.rs");
        let indexed_files = vec![path.clone()];

        assert!(cache.path_known_openable(&[], &indexed_files, &path, |_| {
            panic!("indexed explorer path should not probe the filesystem")
        }));
    }

    #[test]
    fn explorer_row_openability_preserves_compare_and_blame_path_checks() {
        let mut cache = ExplorerRowOpenabilityCache::default();
        let current = PathBuf::from("workspace/src/main.rs");
        let selected = PathBuf::from("workspace/src/lib.rs");
        let mut probed = Vec::new();

        let openability =
            cache.row_openability(&[], &[], &current, Some(&selected), false, |path| {
                probed.push(path.to_path_buf());
                path == selected.as_path()
            });

        assert_eq!(
            openability,
            ExplorerRowOpenability {
                can_compare_with_selected: true,
                can_open_blame: false,
            }
        );
        assert_eq!(probed, vec![selected, current]);
    }

    #[test]
    fn explorer_row_openability_skips_equivalent_selected_compare_probe() {
        let mut cache = ExplorerRowOpenabilityCache::default();
        let current = PathBuf::from("workspace/src/main.rs");
        let selected = PathBuf::from("workspace/src/../src/main.rs");
        let mut probed = Vec::new();

        let openability =
            cache.row_openability(&[], &[], &current, Some(&selected), false, |path| {
                probed.push(path.to_path_buf());
                path == current.as_path()
            });

        assert_eq!(
            openability,
            ExplorerRowOpenability {
                can_compare_with_selected: false,
                can_open_blame: true,
            }
        );
        assert_eq!(probed, vec![current]);
    }

    #[test]
    fn explorer_row_openability_keeps_equivalent_open_buffer_blame_match() {
        let mut cache = ExplorerRowOpenabilityCache::default();
        let current = PathBuf::from("workspace/src/main.rs");
        let selected = PathBuf::from("workspace/src/../src/main.rs");
        let buffers = vec![TextBuffer::from_text(
            7,
            Some(selected.clone()),
            "open\n".to_owned(),
        )];

        let openability =
            cache.row_openability(&buffers, &[], &current, Some(&selected), false, |_| {
                panic!("equivalent open buffer should make row blame openable without probing")
            });

        assert_eq!(
            openability,
            ExplorerRowOpenability {
                can_compare_with_selected: false,
                can_open_blame: true,
            }
        );
    }

    #[test]
    fn explorer_git_decorations_cache_covers_files_and_parent_folders() {
        let root = PathBuf::from("workspace");
        let entries = vec![
            GitStatusEntry {
                path: root.join("src").join("main.rs"),
                status: GitFileStatus::Modified,
                stage: GitChangeStage::Unstaged,
            },
            GitStatusEntry {
                path: root
                    .join("src")
                    .join("..")
                    .join("src")
                    .join("nested")
                    .join("lib.rs"),
                status: GitFileStatus::Conflicted,
                stage: GitChangeStage::Unstaged,
            },
        ];

        let decorations = ExplorerGitDecorations::from_entries(&root, &entries, true);

        assert_eq!(
            decorations.decoration_for_path(&root.join("src/main.rs"), false),
            Some(explorer_git_decoration_for_status(GitFileStatus::Modified))
        );
        assert_eq!(
            decorations.decoration_for_path(&root.join("src"), true),
            Some(explorer_git_decoration_for_status(
                GitFileStatus::Conflicted
            ))
        );
        assert_eq!(
            decorations.decoration_for_path(&root.join("target"), true),
            None
        );
    }

    #[test]
    fn explorer_git_decorations_cache_respects_disabled_setting() {
        let root = PathBuf::from("workspace");
        let entries = vec![GitStatusEntry {
            path: root.join("src/main.rs"),
            status: GitFileStatus::Modified,
            stage: GitChangeStage::Unstaged,
        }];

        let decorations = ExplorerGitDecorations::from_entries(&root, &entries, false);

        assert_eq!(
            decorations.decoration_for_path(&root.join("src/main.rs"), false),
            None
        );
    }

    #[test]
    fn explorer_git_decorations_empty_cache_has_no_path_decorations() {
        let decorations = ExplorerGitDecorations::default();

        assert_eq!(
            decorations.decoration_for_path(&PathBuf::from("\n\t\u{202e}src/main.rs"), false),
            None
        );
        assert_eq!(
            decorations.decoration_for_path(&PathBuf::from("\n\t\u{202e}src"), true),
            None
        );
    }
}
