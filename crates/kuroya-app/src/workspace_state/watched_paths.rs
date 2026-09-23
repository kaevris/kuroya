use crate::fs_watcher::is_recent_app_write;
#[cfg(not(test))]
use crate::persistence_storage::app_settings_path;
use crate::persistence_storage::{app_state_dir, state_dir};
use crate::workspace_trust::{
    trusted_workspace_paths_match, workspace_path_contains_lexically,
    workspace_path_stays_within_root_lexically,
};
#[cfg(test)]
use kuroya_core::ProjectIndexOptions;
use kuroya_core::{
    BufferId, ProjectIndexPathFilter, TextBuffer, normalize_child_path, workspace_plugins_dir,
    workspace_tasks_path,
};
use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    fs,
    path::{Component, Path, PathBuf},
    sync::{LazyLock, Mutex},
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
    pub(crate) git_metadata_changed: bool,
    pub(crate) workspace_refresh_needed: bool,
    pub(crate) project_paths: Vec<PathBuf>,
}

impl PartialEq for WatchedPathChanges {
    fn eq(&self, other: &Self) -> bool {
        self.settings_changed == other.settings_changed
            && self.tasks_changed == other.tasks_changed
            && self.plugins_changed == other.plugins_changed
            && self.git_metadata_changed == other.git_metadata_changed
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
        state_dir(_root).join("settings.toml")
    }

    #[cfg(not(test))]
    app_settings_path()
}

#[cfg(test)]
pub(crate) fn classify_watched_paths(
    workspace_root: &Path,
    changed: &[PathBuf],
) -> WatchedPathChanges {
    let path_filter = ProjectIndexOptions::default().path_filter(workspace_root);
    classify_watched_paths_with_filter(workspace_root, changed, &path_filter)
}

pub(crate) fn classify_watched_paths_with_filter(
    workspace_root: &Path,
    changed: &[PathBuf],
    index_path_filter: &ProjectIndexPathFilter,
) -> WatchedPathChanges {
    let workspace_root = lexical_normalize_path(workspace_root);
    let settings = settings_path(&workspace_root);
    let app_owned_state_dirs = app_owned_state_dirs_in_workspace(&workspace_root);
    let tasks = workspace_tasks_path(&workspace_root);
    let plugins = workspace_plugins_dir(&workspace_root);
    let state_dir = workspace_root.join(".kuroya");
    let git_dir = workspace_root.join(".git");
    let mut classified = WatchedPathChanges::default();
    let mut seen_project_paths = HashSet::with_capacity(changed.len());

    for raw_path in changed {
        let path = lexical_normalize_path(raw_path);
        // The app's own writes (e.g. saving settings from the preferences
        // panel) must not be classified as external settings changes, or the
        // watcher would immediately reload them back over the saved state.
        if !is_recent_app_write(raw_path) && trusted_workspace_paths_match(&path, &settings) {
            classified.settings_changed = true;
            continue;
        }
        if !path_is_within_workspace(&workspace_root, raw_path, &path) {
            continue;
        }
        if workspace_path_contains_lexically(&git_dir, &path) {
            classified.git_metadata_changed = true;
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
        if app_owned_state_dirs
            .iter()
            .any(|app_state| workspace_path_contains_lexically(app_state, &path))
        {
            continue;
        }
        if workspace_path_contains_lexically(&state_dir, &path) {
            continue;
        }
        if inferred_task_source_changed(&workspace_root, &path) {
            classified.tasks_changed = true;
        }
        if index_path_filter.is_excluded(&path) {
            continue;
        }
        classified.workspace_refresh_needed = true;
        if seen_project_paths.insert(normalized_watched_path_key(&path)) {
            classified.project_paths.push(raw_path.clone());
        }
    }

    classified
}

pub(crate) fn app_owned_state_dirs_in_workspace(workspace_root: &Path) -> Vec<PathBuf> {
    let mut owned = Vec::with_capacity(2);
    if let Some(app_state) = proper_workspace_descendant(workspace_root, &app_state_dir()) {
        owned.push(app_state);
    }
    if let Some(workspace_state) =
        proper_workspace_descendant(workspace_root, &state_dir(workspace_root))
        && !owned
            .iter()
            .any(|parent| workspace_path_contains_lexically(parent, &workspace_state))
    {
        owned.push(workspace_state);
    }
    owned
}

fn proper_workspace_descendant(workspace_root: &Path, path: &Path) -> Option<PathBuf> {
    let path = normalize_child_path(workspace_root, path)?;
    (!trusted_workspace_paths_match(workspace_root, &path)).then_some(path)
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
        .any(|changed_path| changed_path_affects_buffer_path(changed_path, buffer_path))
}

fn changed_path_affects_buffer_path(changed_path: &Path, buffer_path: &Path) -> bool {
    if workspace_path_contains_lexically(changed_path, buffer_path) {
        return true;
    }
    // A buffer opened through a symlink (`workspace\link.rs` backed by a
    // target elsewhere) never lexically matches the target path the watcher
    // reports for writes; compare canonical spellings (symlinks resolved on
    // both sides) as well. Limitation: writes to a target that sits outside
    // every watched root still produce no events to attribute — watching the
    // target itself would be needed and is out of scope here.
    let Some(changed_canonical) = canonicalize_path_cached(changed_path) else {
        return false;
    };
    let Some(buffer_canonical) = canonicalize_path_cached(buffer_path) else {
        return false;
    };
    workspace_path_contains_lexically(&changed_canonical, &buffer_canonical)
}

/// Upper bound on cached canonicalizations; hitting it clears the cache
/// rather than growing unbounded across a long session (entries can also go
/// stale when a symlink is re-pointed, so caching forever would be wrong).
const CANONICAL_PATH_CACHE_CAPACITY: usize = 256;

fn canonicalize_path_cached(path: &Path) -> Option<PathBuf> {
    static CACHE: LazyLock<Mutex<HashMap<PathBuf, Option<PathBuf>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    let mut cache = CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(cached) = cache.get(path) {
        return cached.clone();
    }
    if cache.len() >= CANONICAL_PATH_CACHE_CAPACITY {
        cache.clear();
    }
    let canonical = fs::canonicalize(path).ok();
    cache.insert(path.to_path_buf(), canonical.clone());
    canonical
}

#[cfg(test)]
mod tests {
    use super::{
        WatchedPathChanges, classify_watched_paths, classify_watched_paths_with_filter,
        reloadable_open_buffers_for_changes, settings_path,
    };
    use crate::persistence_storage::{app_state_dir, state_dir};
    use kuroya_core::{ProjectIndexOptions, TextBuffer};
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
                git_metadata_changed: false,
                workspace_refresh_needed: false,
                project_paths: Vec::new(),
            }
        );
    }

    #[test]
    fn classify_watched_paths_ignores_app_owned_state_inside_workspace() {
        let app_state = app_state_dir();
        let root = app_state
            .parent()
            .expect("test app state should have a parent")
            .to_path_buf();
        let changes = [
            app_state.join("state.json"),
            app_state.join("workspaces/current/session.json"),
            app_state.join("workspaces/current/project-index.json"),
        ];

        assert_eq!(
            classify_watched_paths(&root, &changes),
            WatchedPathChanges::default()
        );
    }

    #[test]
    fn classify_watched_paths_keeps_workspace_equal_to_or_below_app_state() {
        let app_state = app_state_dir();
        let nested_root = app_state.join("opened-workspace");
        let equal_path = app_state.join("src/equal.rs");
        let equal_owned_state = state_dir(&app_state).join("session.json");
        let nested_path = nested_root.join("src/nested.rs");

        assert_eq!(
            classify_watched_paths(&app_state, &[equal_owned_state, equal_path.clone()]),
            WatchedPathChanges {
                workspace_refresh_needed: true,
                project_paths: vec![equal_path],
                ..WatchedPathChanges::default()
            }
        );
        assert_eq!(
            classify_watched_paths(&nested_root, std::slice::from_ref(&nested_path)),
            WatchedPathChanges {
                workspace_refresh_needed: true,
                project_paths: vec![nested_path],
                ..WatchedPathChanges::default()
            }
        );
    }

    #[test]
    fn classify_watched_paths_uses_effective_index_filter() {
        let root = PathBuf::from("workspace");
        let excluded = root.join("generated/output.rs");
        let included = root.join("target/output.rs");
        let options = ProjectIndexOptions::with_exclude_globs(40_000, vec!["generated".to_owned()]);
        let filter = options.path_filter(&root);

        assert_eq!(
            classify_watched_paths_with_filter(&root, &[excluded, included.clone()], &filter),
            WatchedPathChanges {
                workspace_refresh_needed: true,
                project_paths: vec![included],
                ..WatchedPathChanges::default()
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
                git_metadata_changed: false,
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
            root.join("node_modules/pkg/index.js"),
            root.join("coverage/report.json"),
        ];

        assert_eq!(
            classify_watched_paths(&root, &changes),
            WatchedPathChanges::default()
        );
    }

    #[test]
    fn classify_watched_paths_maps_git_metadata_to_dedicated_category() {
        let root = PathBuf::from("workspace");
        let changes = [
            root.join(".git/index"),
            root.join(".git/index.lock"),
            root.join(".git/HEAD"),
            root.join(".git/refs/heads/main"),
        ];

        assert_eq!(
            classify_watched_paths(&root, &changes),
            WatchedPathChanges {
                settings_changed: false,
                tasks_changed: false,
                plugins_changed: false,
                git_metadata_changed: true,
                workspace_refresh_needed: false,
                project_paths: Vec::new(),
            }
        );
    }

    #[test]
    fn open_buffers_reload_when_the_watcher_reports_symlink_targets() {
        let unique = format!(
            "kuroya-symlink-reload-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        );
        let target_root = std::env::temp_dir().join(format!("{unique}-targets"));
        let workspace_root = std::env::temp_dir().join(format!("{unique}-workspace"));
        let _ = std::fs::remove_dir_all(&target_root);
        let _ = std::fs::remove_dir_all(&workspace_root);
        std::fs::create_dir_all(&target_root).unwrap();
        std::fs::create_dir_all(&workspace_root).unwrap();
        let target = target_root.join("real.rs");
        std::fs::write(&target, "fn real() {}\n").unwrap();
        let link = workspace_root.join("link.rs");
        #[cfg(windows)]
        let created = std::os::windows::fs::symlink_file(&target, &link).is_ok();
        #[cfg(not(windows))]
        let created = std::os::unix::fs::symlink(&target, &link).is_ok();
        if !created {
            let _ = std::fs::remove_dir_all(&target_root);
            let _ = std::fs::remove_dir_all(&workspace_root);
            eprintln!(
                "skipping symlink attribution: creating file symlinks requires privileges here"
            );
            return;
        }

        // The buffer was opened through the in-workspace symlink while the
        // watcher reports the resolved target path; only canonical matching
        // attributes the event back to the buffer.
        let buffer = TextBuffer::from_text(7, Some(link.clone()), "opened".to_owned());
        let buffers = vec![buffer];

        assert_eq!(
            reloadable_open_buffers_for_changes(std::slice::from_ref(&target), &buffers),
            vec![(7, link)]
        );

        std::fs::remove_dir_all(&target_root).unwrap();
        std::fs::remove_dir_all(&workspace_root).unwrap();
    }
}
