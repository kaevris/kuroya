use crate::{
    KuroyaApp,
    explorer_rows::{
        EXPLORER_ROW_HEIGHT, ExplorerGitDecoration, ExplorerGitDecorations,
        ExplorerRowPreparationInput, explorer_entry_path_display_name, explorer_prepared_entry_row,
        prepare_explorer_row,
    },
    path_display::display_error_label_cow,
    ui_icons::{IconKind, icon_label},
    ui_scrollbars::{apply_themed_scrollbar_visuals, scrollbar_visibility, themed_scrollbar_style},
    ui_state::{
        handle_list_navigation_keys, plain_key_pressed, selected_row_scroll_offset,
        selection_page_step,
    },
    workspace_trust::{trusted_workspace_paths_match, workspace_path_stays_within_root_lexically},
};
use eframe::egui::{self, Key, RichText, ScrollArea};
use kuroya_core::{Command, ProjectEntry};
use std::{
    collections::{HashMap, HashSet},
    fs,
    ops::Range,
    path::{Path, PathBuf},
};

mod context_menu;
#[cfg(test)]
pub(crate) use context_menu::{
    explorer_context_path_known_openable, explorer_file_compare_context_action_labels,
    explorer_file_source_control_context_action_labels,
};

#[derive(Debug, Clone, Default)]
pub(crate) struct ExplorerDirectorySnapshot {
    entries: Vec<ProjectEntry>,
    error: Option<String>,
}

impl KuroyaApp {
    pub(crate) fn render_explorer_tree(&mut self, ui: &mut egui::Ui) {
        if self.workspace_placeholder {
            ui.add_space(24.0);
            ui.centered_and_justified(|ui| {
                icon_label(
                    ui,
                    IconKind::Folder,
                    ui.visuals().widgets.inactive.fg_stroke.color,
                    "No folder open",
                );
                ui.label(RichText::new("No folder open").small());
            });
            return;
        }

        let ExplorerTreeRows {
            entries,
            first_error,
        } = explorer_entries_for_tree(
            &self.workspace.root,
            &self.explorer_expanded,
            &mut self.explorer_directory_cache,
            usize::MAX,
        );
        if entries.is_empty() {
            ui.add_space(24.0);
            ui.centered_and_justified(|ui| {
                if let Some(error) = first_error {
                    icon_label(
                        ui,
                        IconKind::Folder,
                        ui.visuals().widgets.inactive.fg_stroke.color,
                        "Could not read folder",
                    );
                    let error = display_error_label_cow(&error);
                    ui.label(
                        RichText::new(format!("Could not read folder: {}", error.as_ref())).small(),
                    );
                } else {
                    icon_label(
                        ui,
                        IconKind::Folder,
                        ui.visuals().widgets.inactive.fg_stroke.color,
                        "Folder is empty",
                    );
                    ui.label(RichText::new("Folder is empty").small());
                }
            });
            return;
        }

        let active_path = self
            .active_buffer()
            .and_then(|buffer| buffer.path().cloned());
        let focus_id = ui.make_persistent_id("explorer-tree-keyboard");
        let tree_focused = ui.memory(|memory| memory.has_focus(focus_id));
        let mut selected_entry_index = explorer_selected_entry_index(
            &entries,
            self.explorer_revealed_path.as_deref(),
            active_path.as_deref(),
        );
        let mut selected_index = selected_entry_index.unwrap_or(0);
        let viewport_height = ui.available_height();
        let mut scroll_to_selection = false;
        if tree_focused {
            scroll_to_selection = ui.input(|input| {
                handle_list_navigation_keys(
                    input,
                    &mut selected_index,
                    entries.len(),
                    selection_page_step(EXPLORER_ROW_HEIGHT, viewport_height),
                )
            });
            if scroll_to_selection && let Some(entry) = entries.get(selected_index) {
                self.explorer_revealed_path = Some(entry.path.clone());
                selected_entry_index = Some(selected_index);
            }
            if let Some(entry) = entries.get(selected_index) {
                if ui.input(|input| plain_key_pressed(input, Key::Enter)) {
                    self.activate_explorer_entry(entry);
                } else if ui.input(|input| plain_key_pressed(input, Key::ArrowRight))
                    && entry.is_dir
                {
                    self.explorer_expanded.insert(entry.path.clone());
                } else if ui.input(|input| plain_key_pressed(input, Key::ArrowLeft)) {
                    if entry.is_dir && self.explorer_expanded.remove(&entry.path) {
                        self.explorer_revealed_path = Some(entry.path.clone());
                        selected_entry_index = Some(selected_index);
                    } else if let Some(parent_index) =
                        explorer_parent_entry_index(&entries, &entry.path)
                        && let Some(parent) = entries.get(parent_index)
                    {
                        selected_index = parent_index;
                        self.explorer_revealed_path = Some(parent.path.clone());
                        selected_entry_index = Some(parent_index);
                        scroll_to_selection = true;
                    }
                }
            }
        }

        let content_style = ui.style().clone();
        ui.visuals_mut().clip_rect_margin = 0.0;
        ui.spacing_mut().scroll = themed_scrollbar_style(
            self.settings.scrollbar_vertical_scrollbar_size,
            self.settings.scrollbar_vertical_scrollbar_size,
            false,
        );
        apply_themed_scrollbar_visuals(ui);

        let mut scroll_area = ScrollArea::vertical()
            .scroll_bar_visibility(scrollbar_visibility(self.settings.explorer_scrollbar));
        if scroll_to_selection {
            scroll_area = scroll_area.vertical_scroll_offset(selected_row_scroll_offset(
                selected_index,
                entries.len(),
                EXPLORER_ROW_HEIGHT,
                viewport_height,
            ));
        }
        let git_decorations = ExplorerGitDecorations::from_entries(
            &self.workspace.root,
            self.git.entries_slice(),
            self.settings.git_decorations_enabled,
        );
        scroll_area.show_rows(ui, EXPLORER_ROW_HEIGHT, entries.len(), |ui, rows| {
            ui.set_style(content_style.clone());
            let (first_row, visible_entries) = visible_explorer_row_entries(&entries, rows);
            for (offset, entry) in visible_entries.iter().enumerate() {
                let row = first_row + offset;
                let expanded =
                    entry.is_dir && self.explorer_expanded.contains(entry.path.as_path());
                let selected = selected_entry_index == Some(row);
                let git_decoration =
                    git_decorations.decoration_for_path(entry.path.as_path(), entry.is_dir);
                let prepared = prepare_explorer_row(ExplorerRowPreparationInput {
                    entry,
                    expanded,
                    selected,
                    git_decoration,
                });
                let response = explorer_prepared_entry_row(ui, &prepared).on_hover_ui(|ui| {
                    explorer_entry_hover_ui(ui, prepared.relative_path, git_decoration)
                });
                if response.clicked() {
                    response.request_focus();
                    ui.memory_mut(|memory| memory.request_focus(focus_id));
                    self.activate_explorer_entry(entry);
                }
                response.context_menu(|ui| {
                    self.render_explorer_entry_context_menu(
                        ui,
                        &entries,
                        prepared.path,
                        prepared.relative_path,
                        prepared.is_dir,
                        prepared.expanded,
                    );
                });
            }
        });
    }

    fn activate_explorer_entry(&mut self, entry: &ProjectEntry) {
        if entry.is_dir {
            if !self.explorer_expanded.remove(&entry.path) {
                self.explorer_expanded.insert(entry.path.clone());
            }
        } else {
            self.command_bus.push(Command::OpenFile(entry.path.clone()));
        }
        self.explorer_revealed_path = Some(entry.path.clone());
    }

    pub(crate) fn clear_explorer_directory_cache(&mut self) {
        self.explorer_directory_cache.clear();
    }
}

#[derive(Debug, Default)]
struct ExplorerTreeRows {
    entries: Vec<ProjectEntry>,
    first_error: Option<String>,
}

fn explorer_entries_for_tree(
    root: &Path,
    expanded_paths: &HashSet<PathBuf>,
    directory_cache: &mut HashMap<PathBuf, ExplorerDirectorySnapshot>,
    limit: usize,
) -> ExplorerTreeRows {
    let mut rows = ExplorerTreeRows::default();
    append_explorer_directory_entries(
        root,
        root,
        expanded_paths,
        directory_cache,
        &mut rows,
        limit,
    );
    rows
}

fn visible_explorer_row_entries(
    entries: &[ProjectEntry],
    rows: Range<usize>,
) -> (usize, &[ProjectEntry]) {
    let (start, end) = visible_explorer_row_bounds(entries.len(), rows);
    (start, &entries[start..end])
}

fn visible_explorer_row_bounds(entry_count: usize, rows: Range<usize>) -> (usize, usize) {
    let start = rows.start.min(entry_count);
    let end = rows.end.min(entry_count).max(start);
    (start, end)
}

fn append_explorer_directory_entries(
    root: &Path,
    directory: &Path,
    expanded_paths: &HashSet<PathBuf>,
    directory_cache: &mut HashMap<PathBuf, ExplorerDirectorySnapshot>,
    rows: &mut ExplorerTreeRows,
    limit: usize,
) {
    if rows.entries.len() >= limit {
        return;
    }

    let snapshot = explorer_directory_snapshot(root, directory, directory_cache);
    if rows.first_error.is_none() {
        rows.first_error = snapshot.error.clone();
    }
    let children = snapshot.entries.clone();

    for entry in children {
        if rows.entries.len() >= limit {
            break;
        }
        let expand_child = entry.is_dir && expanded_paths.contains(entry.path.as_path());
        rows.entries.push(entry.clone());
        if expand_child {
            append_explorer_directory_entries(
                root,
                &entry.path,
                expanded_paths,
                directory_cache,
                rows,
                limit,
            );
        }
    }
}

fn explorer_directory_snapshot<'a>(
    root: &Path,
    directory: &Path,
    directory_cache: &'a mut HashMap<PathBuf, ExplorerDirectorySnapshot>,
) -> &'a ExplorerDirectorySnapshot {
    directory_cache
        .entry(directory.to_path_buf())
        .or_insert_with(|| read_explorer_directory(root, directory))
}

fn read_explorer_directory(root: &Path, directory: &Path) -> ExplorerDirectorySnapshot {
    let read_dir = match fs::read_dir(directory) {
        Ok(read_dir) => read_dir,
        Err(error) => {
            return ExplorerDirectorySnapshot {
                entries: Vec::new(),
                error: Some(error.to_string()),
            };
        }
    };

    let mut entries = Vec::new();
    let mut error = None;
    for child in read_dir {
        let child = match child {
            Ok(child) => child,
            Err(child_error) => {
                if error.is_none() {
                    error = Some(child_error.to_string());
                }
                continue;
            }
        };
        let path = child.path();
        if !workspace_path_stays_within_root_lexically(root, &path) {
            continue;
        }
        let file_type = match child.file_type() {
            Ok(file_type) => file_type,
            Err(child_error) => {
                if error.is_none() {
                    error = Some(child_error.to_string());
                }
                continue;
            }
        };
        let is_dir = file_type.is_dir();
        if !(is_dir || file_type.is_file()) {
            continue;
        }
        let Ok(relative_path) = path.strip_prefix(root) else {
            continue;
        };
        if relative_path.as_os_str().is_empty() {
            continue;
        }
        let relative_path = relative_path.to_path_buf();
        entries.push(ProjectEntry {
            path,
            depth: relative_path.components().count().saturating_sub(1),
            relative_path,
            is_dir,
        });
    }

    entries.sort_unstable_by(|a, b| {
        a.relative_path
            .cmp(&b.relative_path)
            .then(a.is_dir.cmp(&b.is_dir).reverse())
    });
    ExplorerDirectorySnapshot { entries, error }
}

pub(crate) fn explorer_selected_entry_index(
    entries: &[ProjectEntry],
    revealed_path: Option<&Path>,
    active_path: Option<&Path>,
) -> Option<usize> {
    let mut active_index = None;
    for (index, entry) in entries.iter().enumerate() {
        if revealed_path.is_some_and(|path| {
            entry.path == path || trusted_workspace_paths_match(&entry.path, path)
        }) {
            return Some(index);
        }
        if active_index.is_none()
            && active_path.is_some_and(|path| {
                entry.path == path || trusted_workspace_paths_match(&entry.path, path)
            })
        {
            active_index = Some(index);
        }
    }
    active_index
}

pub(crate) fn explorer_parent_entry_index(entries: &[ProjectEntry], path: &Path) -> Option<usize> {
    let parent = path.parent()?;
    explorer_entry_index(entries, parent)
}

fn explorer_entry_index(entries: &[ProjectEntry], path: &Path) -> Option<usize> {
    entries
        .iter()
        .position(|entry| entry.path == path || trusted_workspace_paths_match(&entry.path, path))
}

fn explorer_entry_hover_ui(
    ui: &mut egui::Ui,
    relative_path: &Path,
    git_decoration: Option<ExplorerGitDecoration>,
) {
    ui.set_max_width(ui.spacing().tooltip_width);
    let path = explorer_entry_path_display_name(relative_path);
    if let Some(decoration) = git_decoration {
        ui.label(path);
        ui.label(decoration.label);
    } else {
        ui.label(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_explorer_row_entries_clamps_to_available_entries() {
        let root = PathBuf::from("workspace");
        let entries = vec![
            explorer_entry(&root, "README.md", false),
            explorer_entry(&root, "src", true),
            explorer_entry(&root, "src/main.rs", false),
        ];

        let (first_row, visible) = visible_explorer_row_entries(&entries, 1..99);

        assert_eq!(first_row, 1);
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].path, root.join("src"));
        assert_eq!(visible[1].path, root.join("src/main.rs"));

        let (first_row, visible) = visible_explorer_row_entries(&entries, 99..100);

        assert_eq!(first_row, entries.len());
        assert!(visible.is_empty());
    }

    #[test]
    fn visible_explorer_row_entries_rejects_reversed_and_extreme_ranges() {
        let root = PathBuf::from("workspace");
        let entries = vec![
            explorer_entry(&root, "README.md", false),
            explorer_entry(&root, "src", true),
            explorer_entry(&root, "src/main.rs", false),
        ];

        let start = 2;
        let end = 1;
        let (first_row, visible) = visible_explorer_row_entries(&entries, start..end);

        assert_eq!(first_row, 2);
        assert!(visible.is_empty());

        let (first_row, visible) = visible_explorer_row_entries(&entries, 0..usize::MAX);

        assert_eq!(first_row, 0);
        assert_eq!(visible.len(), entries.len());
    }

    #[test]
    fn explorer_entries_for_tree_loads_only_expanded_directories() {
        let root = temp_explorer_workspace("lazy-expanded");
        std::fs::create_dir_all(root.join("a/nested")).unwrap();
        std::fs::create_dir_all(root.join("z")).unwrap();
        std::fs::write(root.join("a/nested/hidden.rs"), "").unwrap();
        std::fs::write(root.join("z/main.rs"), "").unwrap();
        let mut expanded = HashSet::new();
        expanded.insert(root.join("z"));
        let mut cache = HashMap::new();

        let entries = explorer_entries_for_tree(&root, &expanded, &mut cache, usize::MAX)
            .entries
            .into_iter()
            .map(|entry| entry.relative_path)
            .collect::<Vec<_>>();

        assert_eq!(
            entries,
            vec![
                PathBuf::from("a"),
                PathBuf::from("z"),
                PathBuf::from("z/main.rs")
            ]
        );
        assert!(cache.contains_key(&root));
        assert!(cache.contains_key(&root.join("z")));
        assert!(!cache.contains_key(&root.join("a")));
        assert!(!cache.contains_key(&root.join("a/nested")));
        std::fs::remove_dir_all(root).unwrap();
    }

    fn explorer_entry(root: &Path, relative: &str, is_dir: bool) -> ProjectEntry {
        let relative_path = PathBuf::from(relative);
        ProjectEntry {
            path: root.join(&relative_path),
            depth: relative_path.components().count().saturating_sub(1),
            relative_path,
            is_dir,
        }
    }

    fn temp_explorer_workspace(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "kuroya-explorer-tree-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
