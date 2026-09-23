use crate::{
    KuroyaApp,
    explorer_rows::{
        EXPLORER_ROW_HEIGHT, ExplorerGitDecoration, ExplorerGitDecorations,
        ExplorerRowPreparationInput, explorer_entry_path_display_name, explorer_prepared_entry_row,
        prepare_explorer_row,
    },
    path_display::display_error_label_cow,
    ui_event_channel::send_ui_event,
    ui_events::UiEvent,
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
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

mod context_menu;
#[cfg(test)]
pub(crate) use context_menu::{
    explorer_context_path_known_openable, explorer_file_compare_context_action_labels,
    explorer_file_source_control_context_action_labels,
};

const EXPLORER_FLATTEN_FRAME_ROW_BUDGET: usize = 4096;
const EXPLORER_TREE_TRUNCATION_MESSAGE: &str =
    "Tree truncated — use Quick Open (Ctrl+P) to reach deeper files";

static NEXT_EXPLORER_DIRECTORY_REQUEST_TOKEN: AtomicU64 = AtomicU64::new(1);
static EXPLORER_DIRECTORY_PENDING_LOADS: LazyLock<Mutex<HashMap<PathBuf, u64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn next_explorer_directory_request_token() -> u64 {
    NEXT_EXPLORER_DIRECTORY_REQUEST_TOKEN.fetch_add(1, Ordering::Relaxed)
}

fn register_explorer_pending_load(directory: &Path, request_token: u64) -> bool {
    if let Ok(mut pending) = EXPLORER_DIRECTORY_PENDING_LOADS.lock() {
        if pending.get(directory) == Some(&request_token) {
            return false;
        }
        pending.insert(directory.to_path_buf(), request_token);
        return true;
    }
    false
}

fn finish_explorer_pending_load(directory: &Path, request_token: u64) {
    if let Ok(mut pending) = EXPLORER_DIRECTORY_PENDING_LOADS.lock()
        && pending.get(directory) == Some(&request_token)
    {
        pending.remove(directory);
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ExplorerDirectorySnapshot {
    Loading {
        request_token: u64,
        previous: Option<ExplorerDirectoryEntries>,
    },
    Ready(ExplorerDirectoryEntries),
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ExplorerDirectoryEntries {
    pub(crate) entries: Vec<ProjectEntry>,
    pub(crate) error: Option<String>,
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

        let mut pending_loads = Vec::new();
        let ExplorerTreeRows {
            entries,
            first_error,
            loading,
            truncated,
        } = explorer_entries_for_tree(
            &self.workspace.root,
            &self.explorer_expanded,
            &mut self.explorer_directory_cache,
            EXPLORER_FLATTEN_FRAME_ROW_BUDGET,
            &mut pending_loads,
        );
        for (directory, request_token) in pending_loads {
            self.spawn_explorer_directory_load(directory, request_token);
        }
        if entries.is_empty() {
            if loading && first_error.is_none() {
                return;
            }
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

        if truncated {
            egui::TopBottomPanel::bottom("kuroya-explorer-tree-truncation-footer").show_inside(
                ui,
                |ui| {
                    ui.label(
                        RichText::new(EXPLORER_TREE_TRUNCATION_MESSAGE)
                            .small()
                            .weak(),
                    );
                },
            );
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
        if let Ok(mut pending) = EXPLORER_DIRECTORY_PENDING_LOADS.lock() {
            pending.clear();
        }
    }

    pub(crate) fn invalidate_explorer_directory_for_path(&mut self, path: &Path) {
        invalidate_explorer_directory_cache_for_path(&mut self.explorer_directory_cache, path);
    }

    fn spawn_explorer_directory_load(&mut self, directory: PathBuf, request_token: u64) {
        if !register_explorer_pending_load(&directory, request_token) {
            return;
        }
        let root = self.workspace.root.clone();
        let generation = self.workspace_event_generation;
        let tx = self.tx.clone();
        self.runtime.spawn_blocking(move || {
            let snapshot = read_explorer_directory(&root, &directory);
            let _ = send_ui_event(
                &tx,
                UiEvent::ExplorerDirectoryLoaded {
                    root,
                    generation,
                    request_token,
                    directory,
                    snapshot,
                },
            );
        });
    }

    pub(crate) fn apply_explorer_directory_loaded_event(
        &mut self,
        root: &Path,
        generation: u64,
        request_token: u64,
        directory: &Path,
        snapshot: ExplorerDirectoryEntries,
    ) -> bool {
        if !self.workspace_event_is_current(root, generation) {
            return false;
        }
        if !apply_explorer_directory_loaded_to_cache(
            &mut self.explorer_directory_cache,
            directory,
            request_token,
            snapshot,
        ) {
            return false;
        }
        finish_explorer_pending_load(directory, request_token);
        true
    }
}

fn apply_explorer_directory_loaded_to_cache(
    directory_cache: &mut HashMap<PathBuf, ExplorerDirectorySnapshot>,
    directory: &Path,
    request_token: u64,
    snapshot: ExplorerDirectoryEntries,
) -> bool {
    match directory_cache.get_mut(directory) {
        Some(ExplorerDirectorySnapshot::Loading {
            request_token: token,
            ..
        }) if *token == request_token => {}
        _ => return false,
    }
    directory_cache.insert(
        directory.to_path_buf(),
        ExplorerDirectorySnapshot::Ready(snapshot),
    );
    true
}

fn invalidate_explorer_directory_cache_for_path(
    directory_cache: &mut HashMap<PathBuf, ExplorerDirectorySnapshot>,
    path: &Path,
) {
    let affected: Vec<PathBuf> = directory_cache
        .keys()
        .filter(|cached| cached.starts_with(path))
        .cloned()
        .collect();
    for cached in &affected {
        directory_cache.remove(cached);
    }
    refresh_explorer_directory_in_place(directory_cache, path.parent());
}

fn refresh_explorer_directory_in_place(
    directory_cache: &mut HashMap<PathBuf, ExplorerDirectorySnapshot>,
    directory: Option<&Path>,
) -> bool {
    let Some(directory) = directory else {
        return false;
    };
    let reloaded = match directory_cache.get_mut(directory) {
        Some(ExplorerDirectorySnapshot::Ready(entries)) => ExplorerDirectorySnapshot::Loading {
            request_token: next_explorer_directory_request_token(),
            previous: Some(std::mem::take(entries)),
        },
        Some(ExplorerDirectorySnapshot::Loading { request_token, .. }) => {
            *request_token = next_explorer_directory_request_token();
            return false;
        }
        None => return false,
    };
    directory_cache.insert(directory.to_path_buf(), reloaded);
    true
}

#[derive(Debug, Default)]
struct ExplorerTreeRows {
    entries: Vec<ProjectEntry>,
    first_error: Option<String>,
    loading: bool,
    truncated: bool,
}

fn explorer_entries_for_tree(
    root: &Path,
    expanded_paths: &HashSet<PathBuf>,
    directory_cache: &mut HashMap<PathBuf, ExplorerDirectorySnapshot>,
    limit: usize,
    pending_loads: &mut Vec<(PathBuf, u64)>,
) -> ExplorerTreeRows {
    let mut rows = ExplorerTreeRows::default();
    append_explorer_directory_entries(
        root,
        expanded_paths,
        directory_cache,
        &mut rows,
        limit,
        pending_loads,
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
    directory: &Path,
    expanded_paths: &HashSet<PathBuf>,
    directory_cache: &mut HashMap<PathBuf, ExplorerDirectorySnapshot>,
    rows: &mut ExplorerTreeRows,
    limit: usize,
    pending_loads: &mut Vec<(PathBuf, u64)>,
) {
    if rows.entries.len() >= limit {
        rows.truncated = true;
        return;
    }

    let snapshot = explorer_directory_snapshot(directory, directory_cache, pending_loads);
    let snapshot_entries: &[ProjectEntry] = match snapshot {
        ExplorerDirectorySnapshot::Ready(ready) => {
            if rows.first_error.is_none() {
                rows.first_error = ready.error.clone();
            }
            &ready.entries
        }
        ExplorerDirectorySnapshot::Loading { previous, .. } => match previous {
            Some(previous) => {
                if rows.first_error.is_none() {
                    rows.first_error = previous.error.clone();
                }
                &previous.entries
            }
            None => {
                rows.loading = true;
                &[]
            }
        },
    };
    let remaining = limit - rows.entries.len();
    if snapshot_entries.len() > remaining {
        rows.truncated = true;
    }

    let children: Vec<ProjectEntry> = snapshot_entries.iter().take(remaining).cloned().collect();

    for entry in children {
        if rows.entries.len() >= limit {
            rows.truncated = true;
            break;
        }
        let expand_child = entry.is_dir && expanded_paths.contains(entry.path.as_path());
        let expanded_child_directory = expand_child.then(|| entry.path.clone());
        rows.entries.push(entry);
        if let Some(child_directory) = expanded_child_directory {
            append_explorer_directory_entries(
                &child_directory,
                expanded_paths,
                directory_cache,
                rows,
                limit,
                pending_loads,
            );
        }
    }
}

fn explorer_directory_snapshot<'a>(
    directory: &Path,
    directory_cache: &'a mut HashMap<PathBuf, ExplorerDirectorySnapshot>,
    pending_loads: &mut Vec<(PathBuf, u64)>,
) -> &'a ExplorerDirectorySnapshot {
    let entry = directory_cache
        .entry(directory.to_path_buf())
        .or_insert_with(|| ExplorerDirectorySnapshot::Loading {
            request_token: next_explorer_directory_request_token(),
            previous: None,
        });
    if let ExplorerDirectorySnapshot::Loading { request_token, .. } = entry {
        pending_loads.push((directory.to_path_buf(), *request_token));
    }
    entry
}

fn read_explorer_directory(root: &Path, directory: &Path) -> ExplorerDirectoryEntries {
    let read_dir = match fs::read_dir(directory) {
        Ok(read_dir) => read_dir,
        Err(error) => {
            return ExplorerDirectoryEntries {
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
        let depth = relative_path.components().count().saturating_sub(1);
        let metadata = if is_dir { None } else { child.metadata().ok() };
        entries.push(ProjectEntry::from_metadata_parts(
            path,
            relative_path,
            is_dir,
            depth,
            metadata.as_ref(),
        ));
    }

    entries.sort_unstable_by(compare_explorer_directory_entries);
    ExplorerDirectoryEntries { entries, error }
}

fn compare_explorer_directory_entries(a: &ProjectEntry, b: &ProjectEntry) -> std::cmp::Ordering {
    b.is_dir.cmp(&a.is_dir).then_with(|| {
        let a_name = a.relative_path.as_os_str();
        let b_name = b.relative_path.as_os_str();
        a_name
            .to_string_lossy()
            .to_ascii_lowercase()
            .cmp(&b_name.to_string_lossy().to_ascii_lowercase())
            .then_with(|| a_name.cmp(b_name))
    })
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
    fn explorer_entries_for_tree_renders_only_expanded_ready_directories() {
        let root = temp_explorer_workspace("lazy-expanded");
        std::fs::create_dir_all(root.join("a/nested")).unwrap();
        std::fs::create_dir_all(root.join("z")).unwrap();
        std::fs::write(root.join("a/nested/hidden.rs"), "").unwrap();
        std::fs::write(root.join("z/main.rs"), "").unwrap();
        let mut expanded = HashSet::new();
        expanded.insert(root.join("z"));
        let mut cache = HashMap::new();
        cache.insert(
            root.clone(),
            ExplorerDirectorySnapshot::Ready(read_explorer_directory(&root, &root)),
        );
        cache.insert(
            root.join("a"),
            ExplorerDirectorySnapshot::Ready(read_explorer_directory(&root, &root.join("a"))),
        );
        cache.insert(
            root.join("z"),
            ExplorerDirectorySnapshot::Ready(read_explorer_directory(&root, &root.join("z"))),
        );
        let mut pending_loads = Vec::new();

        let rows =
            explorer_entries_for_tree(&root, &expanded, &mut cache, usize::MAX, &mut pending_loads);

        assert_eq!(
            rows.entries
                .into_iter()
                .map(|entry| entry.relative_path)
                .collect::<Vec<_>>(),
            vec![
                PathBuf::from("a"),
                PathBuf::from("z"),
                PathBuf::from("z/main.rs")
            ]
        );
        assert!(!rows.loading);
        assert!(pending_loads.is_empty());
        assert!(cache.contains_key(&root));
        assert!(cache.contains_key(&root.join("z")));
        assert!(cache.contains_key(&root.join("a")));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explorer_entries_for_tree_inserts_loading_placeholder_for_misses() {
        let root = temp_explorer_workspace("async-miss");
        std::fs::create_dir_all(root.join("z")).unwrap();
        std::fs::write(root.join("z/main.rs"), "").unwrap();
        let expanded = HashSet::new();
        let mut cache = HashMap::new();
        let mut pending_loads = Vec::new();

        let rows =
            explorer_entries_for_tree(&root, &expanded, &mut cache, usize::MAX, &mut pending_loads);

        assert!(rows.entries.is_empty());
        assert!(rows.loading);
        assert_eq!(pending_loads.len(), 1);
        assert_eq!(pending_loads[0].0, root);
        assert!(matches!(
            cache.get(&root),
            Some(ExplorerDirectorySnapshot::Loading { .. })
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explorer_directory_loaded_fills_cache_only_for_matching_token() {
        let root = PathBuf::from("workspace");
        let directory = root.join("src");
        let request_token = next_explorer_directory_request_token();
        let mut cache = HashMap::new();
        cache.insert(
            directory.clone(),
            ExplorerDirectorySnapshot::Loading {
                request_token,
                previous: None,
            },
        );
        let mut loaded = ExplorerDirectoryEntries::default();
        loaded
            .entries
            .push(explorer_entry(&directory, "main.rs", false));

        assert!(!apply_explorer_directory_loaded_to_cache(
            &mut cache,
            &directory,
            request_token + 1,
            loaded.clone(),
        ));
        assert!(matches!(
            cache.get(&directory),
            Some(ExplorerDirectorySnapshot::Loading { .. })
        ));

        assert!(apply_explorer_directory_loaded_to_cache(
            &mut cache,
            &directory,
            request_token,
            loaded,
        ));
        match cache.get(&directory) {
            Some(ExplorerDirectorySnapshot::Ready(entries)) => {
                assert_eq!(entries.entries.len(), 1);
                assert_eq!(entries.error, None);
            }
            _ => panic!("cache entry should be ready"),
        }
        assert!(!apply_explorer_directory_loaded_to_cache(
            &mut cache,
            &directory,
            request_token,
            ExplorerDirectoryEntries::default(),
        ));
    }

    #[test]
    fn explorer_targeted_invalidation_drops_affected_subtree_and_keeps_siblings() {
        let root = PathBuf::from("workspace");
        let renamed = root.join("src");
        let sibling = root.join("tests");
        let mut cache = HashMap::new();
        let mut root_entries = ExplorerDirectoryEntries::default();
        root_entries
            .entries
            .push(explorer_entry(&root, "src", true));
        root_entries
            .entries
            .push(explorer_entry(&root, "tests", true));
        assert_eq!(root_entries.entries.len(), 2);
        cache.insert(root.clone(), ExplorerDirectorySnapshot::Ready(root_entries));
        cache.insert(
            renamed.clone(),
            ExplorerDirectorySnapshot::Ready(ExplorerDirectoryEntries::default()),
        );
        cache.insert(
            renamed.join("nested"),
            ExplorerDirectorySnapshot::Ready(ExplorerDirectoryEntries::default()),
        );
        cache.insert(
            sibling.clone(),
            ExplorerDirectorySnapshot::Ready(ExplorerDirectoryEntries::default()),
        );
        cache.insert(
            sibling.join("keep"),
            ExplorerDirectorySnapshot::Ready(ExplorerDirectoryEntries::default()),
        );

        invalidate_explorer_directory_cache_for_path(&mut cache, &renamed);

        assert!(!cache.contains_key(&renamed));
        assert!(!cache.contains_key(&renamed.join("nested")));
        assert!(cache.contains_key(&sibling));
        assert!(cache.contains_key(&sibling.join("keep")));
        match cache.get(&root) {
            Some(ExplorerDirectorySnapshot::Loading {
                previous: Some(previous),
                ..
            }) => assert_eq!(previous.entries.len(), 2),
            other => panic!("parent should reload keeping previous rows: {other:?}"),
        }
    }

    #[test]
    fn explorer_stale_while_revalidate_keeps_previous_rows_until_reload_arrives() {
        let root = PathBuf::from("workspace");
        let changed_file = root.join("old.rs");
        let mut cache = HashMap::new();
        let mut previous = ExplorerDirectoryEntries::default();
        previous
            .entries
            .push(explorer_entry(&root, "old.rs", false));
        cache.insert(root.clone(), ExplorerDirectorySnapshot::Ready(previous));

        invalidate_explorer_directory_cache_for_path(&mut cache, &changed_file);

        let request_token = match cache.get(&root) {
            Some(ExplorerDirectorySnapshot::Loading {
                request_token,
                previous: Some(previous),
            }) => {
                assert_eq!(previous.entries.len(), 1);
                *request_token
            }
            other => panic!("expected stale-while-revalidate reload: {other:?}"),
        };

        let expanded = HashSet::new();
        let mut pending_loads = Vec::new();
        let rows =
            explorer_entries_for_tree(&root, &expanded, &mut cache, usize::MAX, &mut pending_loads);

        assert_eq!(rows.entries.len(), 1);
        assert!(!rows.loading);
        assert_eq!(pending_loads, vec![(root.clone(), request_token)]);

        let mut loaded = ExplorerDirectoryEntries::default();
        loaded.entries.push(explorer_entry(&root, "new.rs", false));
        assert!(apply_explorer_directory_loaded_to_cache(
            &mut cache,
            &root,
            request_token,
            loaded,
        ));

        match cache.get(&root) {
            Some(ExplorerDirectorySnapshot::Ready(entries)) => {
                assert_eq!(entries.entries[0].relative_path, PathBuf::from("new.rs"));
            }
            other => panic!("reload should replace previous rows: {other:?}"),
        }
    }

    #[test]
    fn explorer_flatten_budget_is_respected_across_two_passes() {
        const TEST_ROW_BUDGET: usize = 3;
        let root = temp_explorer_workspace("flatten-budget");
        std::fs::create_dir_all(root.join("a/b/c")).unwrap();
        std::fs::write(root.join("README.md"), "").unwrap();
        std::fs::write(root.join("a/one.rs"), "").unwrap();
        std::fs::write(root.join("a/b/two.rs"), "").unwrap();
        std::fs::write(root.join("a/b/c/three.rs"), "").unwrap();
        let expanded = HashSet::from([root.join("a"), root.join("a/b"), root.join("a/b/c")]);
        let mut cache = HashMap::new();
        for directory in [
            root.clone(),
            root.join("a"),
            root.join("a/b"),
            root.join("a/b/c"),
        ] {
            cache.insert(
                directory.clone(),
                ExplorerDirectorySnapshot::Ready(read_explorer_directory(&root, &directory)),
            );
        }

        let mut first_pass_pending = Vec::new();
        let first_rows = explorer_entries_for_tree(
            &root,
            &expanded,
            &mut cache,
            TEST_ROW_BUDGET,
            &mut first_pass_pending,
        );

        assert_eq!(first_rows.entries.len(), TEST_ROW_BUDGET);
        assert!(!first_rows.loading);
        assert!(first_rows.truncated);
        assert!(first_pass_pending.is_empty());
        assert!(!cache.contains_key(&root.join("unvisited-placeholder")));
        assert!(cache.contains_key(&root.join("a")));

        let mut second_pass_pending = Vec::new();
        let second_rows = explorer_entries_for_tree(
            &root,
            &expanded,
            &mut cache,
            usize::MAX,
            &mut second_pass_pending,
        );

        assert_eq!(
            second_rows
                .entries
                .iter()
                .map(|entry| entry.relative_path.to_string_lossy().replace('\\', "/"))
                .collect::<Vec<_>>(),
            vec![
                "a",
                "a/b",
                "a/b/c",
                "a/b/c/three.rs",
                "a/b/two.rs",
                "a/one.rs",
                "README.md",
            ]
        );
        assert!(!second_rows.loading);
        assert!(!second_rows.truncated);
        assert!(second_pass_pending.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explorer_directory_sort_groups_directories_first_case_insensitively() {
        let root = PathBuf::from("workspace");
        let mut entries = [
            explorer_entry(&root, "Zebra.txt", false),
            explorer_entry(&root, "beta", true),
            explorer_entry(&root, "a.txt", false),
            explorer_entry(&root, "Alpha", true),
        ];

        entries.sort_by(compare_explorer_directory_entries);

        let names = entries
            .iter()
            .map(|entry| entry.relative_path.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["Alpha", "beta", "a.txt", "Zebra.txt"]);
        assert!(entries[..2].iter().all(|entry| entry.is_dir));
        assert!(entries[2..].iter().all(|entry| !entry.is_dir));
    }

    #[test]
    fn explorer_directory_sort_tie_breaks_equal_caseless_names_by_byte_order() {
        let root = PathBuf::from("workspace");
        let mut entries = [
            explorer_entry(&root, "readme.md", false),
            explorer_entry(&root, "README.md", false),
        ];

        entries.sort_by(compare_explorer_directory_entries);

        assert_eq!(entries[0].relative_path, PathBuf::from("README.md"));
        assert_eq!(entries[1].relative_path, PathBuf::from("readme.md"));
    }

    #[test]
    fn read_explorer_directory_sorts_directories_before_files_case_insensitively() {
        let root = temp_explorer_workspace("sort-dirs-first");
        std::fs::create_dir_all(root.join("Beta")).unwrap();
        std::fs::create_dir_all(root.join("alpha")).unwrap();
        std::fs::write(root.join("Zebra.txt"), "").unwrap();
        std::fs::write(root.join("a.txt"), "").unwrap();

        let listing = read_explorer_directory(&root, &root);

        assert_eq!(listing.error, None);
        let names = listing
            .entries
            .iter()
            .map(|entry| entry.relative_path.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["alpha", "Beta", "a.txt", "Zebra.txt"]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explorer_flatten_budget_flags_truncation_and_full_pass_clears_it() {
        const TEST_ROW_BUDGET: usize = 2;
        let root = temp_explorer_workspace("budget-truncation");
        std::fs::create_dir_all(root.join("a")).unwrap();
        std::fs::write(root.join("a/one.rs"), "").unwrap();
        std::fs::write(root.join("a/two.rs"), "").unwrap();
        std::fs::write(root.join("a/three.rs"), "").unwrap();
        let expanded = HashSet::from([root.join("a")]);
        let mut cache = HashMap::new();
        for directory in [root.clone(), root.join("a")] {
            cache.insert(
                directory.clone(),
                ExplorerDirectorySnapshot::Ready(read_explorer_directory(&root, &directory)),
            );
        }

        let mut pending_loads = Vec::new();
        let truncated_rows = explorer_entries_for_tree(
            &root,
            &expanded,
            &mut cache,
            TEST_ROW_BUDGET,
            &mut pending_loads,
        );

        assert_eq!(truncated_rows.entries.len(), TEST_ROW_BUDGET);
        assert!(truncated_rows.truncated);

        let mut pending_loads = Vec::new();
        let exact_rows =
            explorer_entries_for_tree(&root, &expanded, &mut cache, 4, &mut pending_loads);

        assert_eq!(exact_rows.entries.len(), 4);
        assert!(!exact_rows.truncated);

        let mut pending_loads = Vec::new();
        let full_rows =
            explorer_entries_for_tree(&root, &expanded, &mut cache, usize::MAX, &mut pending_loads);

        assert_eq!(full_rows.entries.len(), 4);
        assert!(!full_rows.truncated);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn explorer_entry(root: &Path, relative: &str, is_dir: bool) -> ProjectEntry {
        let relative_path = PathBuf::from(relative);
        ProjectEntry {
            path: root.join(&relative_path),
            depth: relative_path.components().count().saturating_sub(1),
            relative_path,
            is_dir,
            ..Default::default()
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
