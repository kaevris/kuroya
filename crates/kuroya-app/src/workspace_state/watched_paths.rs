#[cfg(not(test))]
use crate::persistence_storage::app_settings_path;
#[cfg(test)]
use crate::persistence_storage::state_dir;
use crate::workspace_trust::{
    trusted_workspace_paths_match, workspace_path_contains_lexically,
    workspace_path_stays_within_root_lexically,
};
use kuroya_core::{BufferId, TextBuffer, workspace_plugins_dir, workspace_tasks_path};
use std::{
    collections::HashSet,
    ffi::OsStr,
    path::{Component, Path, PathBuf},
};

const INFERRED_TASK_SOURCE_FILES: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "bun.lockb",
    "bun.lock",
    "Makefile",
    "makefile",
    "justfile",
    ".justfile",
];
const INFERRED_TASK_PRUNED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    "coverage",
    ".next",
    "out",
];

#[derive(Debug, Default)]
pub(crate) struct WatchedPathChanges {
    pub(crate) settings_changed: bool,
    pub(crate) tasks_changed: bool,
    pub(crate) plugins_changed: bool,
    pub(crate) workspace_refresh_needed: bool,
    pub(crate) project_paths: Vec<PathBuf>,
}

impl PartialEq for WatchedPathChanges {
    fn eq(&self, other: &Self) -> bool {
        self.settings_changed == other.settings_changed
            && self.tasks_changed == other.tasks_changed
            && self.plugins_changed == other.plugins_changed
            && self.workspace_refresh_needed == other.workspace_refresh_needed
            && watched_project_paths_match(&self.project_paths, &other.project_paths)
    }
}

impl Eq for WatchedPathChanges {}

fn watched_project_paths_match(left: &[PathBuf], right: &[PathBuf]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left == right || trusted_workspace_paths_match(left, right))
}

pub(crate) fn settings_path(_root: &Path) -> PathBuf {
    #[cfg(test)]
    {
        return state_dir(_root).join("settings.toml");
    }

    #[cfg(not(test))]
    app_settings_path()
}

pub(crate) fn classify_watched_paths(
    workspace_root: &Path,
    changed: &[PathBuf],
) -> WatchedPathChanges {
    let workspace_root = lexical_normalize_path(workspace_root);
    let settings = settings_path(&workspace_root);
    let tasks = workspace_tasks_path(&workspace_root);
    let plugins = workspace_plugins_dir(&workspace_root);
    let state_dir = workspace_root.join(".kuroya");
    let mut classified = WatchedPathChanges::default();
    let mut seen_project_paths = HashSet::with_capacity(changed.len());

    for raw_path in changed {
        let path = lexical_normalize_path(raw_path);
        if trusted_workspace_paths_match(&path, &settings) {
            classified.settings_changed = true;
            continue;
        }
        if !path_is_within_workspace(&workspace_root, raw_path, &path) {
            continue;
        }
        if trusted_workspace_paths_match(&path, &tasks) {
            classified.tasks_changed = true;
            continue;
        }
        if workspace_path_contains_lexically(&plugins, &path) {
            classified.plugins_changed = true;
            continue;
        }
        if workspace_path_contains_lexically(&state_dir, &path) {
            continue;
        }
        if path_has_pruned_project_dir(&workspace_root, &path) {
            continue;
        }
        if inferred_task_source_changed(&workspace_root, &path) {
            classified.tasks_changed = true;
        }
        if !path_has_pruned_workspace_refresh_dir(&workspace_root, &path) {
            classified.workspace_refresh_needed = true;
        }
        if seen_project_paths.insert(normalized_watched_path_key(&path)) {
            classified.project_paths.push(raw_path.clone());
        }
    }

    classified
}

fn lexical_normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    let mut has_root = false;

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => {
                normalized.push(component.as_os_str());
                has_root = true;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                let can_pop_normal = normalized
                    .components()
                    .next_back()
                    .is_some_and(|component| matches!(component, Component::Normal(_)));
                if can_pop_normal {
                    normalized.pop();
                } else if !has_root {
                    normalized.push("..");
                }
            }
            Component::Normal(part) => normalized.push(part),
        }
    }

    if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct WatchedPathKey {
    prefix: Option<String>,
    rooted: bool,
    components: Vec<String>,
}

fn normalized_watched_path_key(path: &Path) -> WatchedPathKey {
    let mut key = WatchedPathKey {
        prefix: None,
        rooted: false,
        components: Vec::new(),
    };

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                key.prefix = Some(normalize_watched_path_component(prefix.as_os_str()));
            }
            Component::RootDir => key.rooted = true,
            Component::CurDir => {}
            Component::ParentDir => key.components.push("..".to_owned()),
            Component::Normal(component) => {
                key.components
                    .push(normalize_watched_path_component(component));
            }
        }
    }

    key
}

fn normalize_watched_path_component(component: &OsStr) -> String {
    let component = component.to_string_lossy();
    #[cfg(windows)]
    {
        if component.is_ascii() {
            let mut component = component.into_owned();
            component.make_ascii_lowercase();
            component
        } else {
            component.to_lowercase()
        }
    }
    #[cfg(not(windows))]
    {
        component.into_owned()
    }
}

fn path_is_within_workspace(
    workspace_root: &Path,
    raw_path: &Path,
    normalized_path: &Path,
) -> bool {
    workspace_path_contains_lexically(workspace_root, normalized_path)
        && workspace_path_stays_within_root_lexically(workspace_root, raw_path)
}

fn inferred_task_source_changed(workspace_root: &Path, path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            INFERRED_TASK_SOURCE_FILES
                .iter()
                .any(|source| name.eq_ignore_ascii_case(source))
        })
        && !path_has_pruned_task_dir(workspace_root, path)
}

fn path_has_pruned_task_dir(workspace_root: &Path, path: &Path) -> bool {
    path_has_pruned_dir(workspace_root, path, INFERRED_TASK_PRUNED_DIRS)
}

fn path_has_pruned_workspace_refresh_dir(workspace_root: &Path, path: &Path) -> bool {
    path_has_pruned_dir(workspace_root, path, INFERRED_TASK_PRUNED_DIRS)
}

fn path_has_pruned_project_dir(workspace_root: &Path, path: &Path) -> bool {
    path_has_pruned_dir(workspace_root, path, INFERRED_TASK_PRUNED_DIRS)
}

fn path_has_pruned_dir(workspace_root: &Path, path: &Path, pruned_dirs: &[&str]) -> bool {
    let parent = path.parent().unwrap_or(path);
    let relative_parent = parent.strip_prefix(workspace_root).unwrap_or(parent);
    relative_parent.components().any(|component| {
        let Component::Normal(name) = component else {
            return false;
        };
        name.to_str().is_some_and(|name| {
            pruned_dirs
                .iter()
                .any(|pruned| name.eq_ignore_ascii_case(pruned))
        })
    })
}

pub(crate) fn reloadable_open_buffers_for_changes(
    changed: &[PathBuf],
    buffers: &[TextBuffer],
) -> Vec<(BufferId, PathBuf)> {
    open_buffers_for_changes(changed, buffers, false)
}

pub(crate) fn dirty_open_buffers_for_changes(
    changed: &[PathBuf],
    buffers: &[TextBuffer],
) -> Vec<(BufferId, PathBuf)> {
    open_buffers_for_changes(changed, buffers, true)
}

fn open_buffers_for_changes(
    changed: &[PathBuf],
    buffers: &[TextBuffer],
    dirty: bool,
) -> Vec<(BufferId, PathBuf)> {
    let changed = normalized_unique_watched_paths(changed);
    if changed.is_empty() {
        return Vec::new();
    }

    buffers
        .iter()
        .filter(|buffer| buffer.is_dirty() == dirty)
        .filter_map(|buffer| {
            let path = buffer.path()?;
            changed_paths_affect_buffer_path(&changed, path)
                .then(|| (buffer.id(), path.to_path_buf()))
        })
        .collect()
}

fn normalized_unique_watched_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut unique_paths = Vec::with_capacity(paths.len());
    let mut seen_paths = HashSet::with_capacity(paths.len());
    for path in paths {
        if path.as_os_str().is_empty() {
            continue;
        }
        let path = lexical_normalize_path(path);
        if seen_paths.insert(normalized_watched_path_key(&path)) {
            unique_paths.push(path);
        }
    }
    unique_paths
}

fn changed_paths_affect_buffer_path(changed: &[PathBuf], buffer_path: &Path) -> bool {
    changed
        .iter()
        .any(|changed_path| workspace_path_contains_lexically(changed_path, buffer_path))
}

#[cfg(test)]
mod tests {
    use super::{WatchedPathChanges, classify_watched_paths, settings_path};
    use std::path::PathBuf;

    #[test]
    fn settings_path_uses_app_state_dir_outside_workspace() {
        let root =
            std::env::temp_dir().join(format!("kuroya-settings-workspace-{}", std::process::id()));
        let settings = settings_path(&root);

        assert_eq!(
            settings.file_name().and_then(|name| name.to_str()),
            Some("settings.toml")
        );
        assert!(
            !settings.starts_with(&root),
            "{} unexpectedly stayed under {}",
            settings.display(),
            root.display()
        );
        assert!(
            !settings.starts_with(root.join(".kuroya")),
            "{} unexpectedly used workspace-local settings",
            settings.display()
        );
    }

    #[test]
    fn classify_watched_paths_uses_global_settings_but_keeps_tasks_and_plugins_local() {
        let root = PathBuf::from("workspace");
        let settings = settings_path(&root);
        let workspace_settings = root.join(".kuroya/settings.toml");
        let tasks = root.join(".kuroya/tasks.toml");
        let plugin_manifest = root.join(".kuroya/plugins/example/plugin.toml");

        assert_eq!(
            classify_watched_paths(&root, &[workspace_settings]),
            WatchedPathChanges::default()
        );
        assert_eq!(
            classify_watched_paths(&root, &[settings, tasks, plugin_manifest]),
            WatchedPathChanges {
                settings_changed: true,
                tasks_changed: true,
                plugins_changed: true,
                workspace_refresh_needed: false,
                project_paths: Vec::new(),
            }
        );
    }

    #[test]
    fn classify_watched_paths_keeps_first_raw_project_path_for_normalized_duplicates() {
        let root = PathBuf::from("workspace");
        let raw = root.join("src/./main.rs");
        let duplicate = root.join("src/generated/../main.rs");

        assert_eq!(
            classify_watched_paths(&root, &[raw.clone(), duplicate]),
            WatchedPathChanges {
                settings_changed: false,
                tasks_changed: false,
                plugins_changed: false,
                workspace_refresh_needed: true,
                project_paths: vec![raw],
            }
        );
    }

    #[test]
    fn classify_watched_paths_rejects_parent_reentry_into_workspace_root() {
        let root = PathBuf::from("workspace/current");
        let reentry = PathBuf::from("workspace/current/../current/src/main.rs");

        assert_eq!(
            classify_watched_paths(&root, &[reentry]),
            WatchedPathChanges::default()
        );
    }

    #[test]
    fn classify_watched_paths_ignores_generated_project_dirs() {
        let root = PathBuf::from("workspace");
        let changes = [
            root.join("target/debug/kuroya.exe"),
            root.join(".git/index.lock"),
            root.join("node_modules/pkg/index.js"),
            root.join("coverage/report.json"),
        ];

        assert_eq!(
            classify_watched_paths(&root, &changes),
            WatchedPathChanges::default()
        );
    }
}
