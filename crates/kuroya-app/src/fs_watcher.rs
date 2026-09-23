use crate::{
    persistence_storage::app_state_dir,
    ui_event_channel::notify_background_activity,
    workspace_state::{app_owned_state_dirs_in_workspace, settings_path},
    workspace_trust::{trusted_workspace_paths_match, workspace_path_contains_lexically},
};
use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};
use kuroya_core::{workspace_plugins_dir, workspace_tasks_path};
use notify::{
    EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::{AccessKind, AccessMode, MetadataKind, ModifyKind},
    recommended_watcher,
};
use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    fs,
    path::{Component, Path, PathBuf},
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const FILE_WATCHER_CHANNEL_BOUND: usize = 4096;
const FILE_WATCHER_DRAIN_BUDGET: usize = 512;
pub(crate) const SELF_WRITE_SUPPRESSION_WINDOW_MS: u64 = 750;
const RECENT_APP_WRITE_CAPACITY: usize = 128;

type RecentAppWrites = HashMap<WatcherPathKey, Instant>;

fn recent_app_writes() -> &'static Mutex<RecentAppWrites> {
    static RECENT_APP_WRITES: LazyLock<Mutex<RecentAppWrites>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    &RECENT_APP_WRITES
}

pub(crate) fn note_app_write(path: &Path) {
    let mut writes = recent_app_writes()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    record_recent_app_write(&mut writes, path, Instant::now());
}

pub(crate) fn is_recent_app_write(path: &Path) -> bool {
    let mut writes = recent_app_writes()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    prune_expired_app_writes(&mut writes, Instant::now());
    recent_app_write_recorded(&writes, path, Instant::now())
}

fn record_recent_app_write(writes: &mut RecentAppWrites, path: &Path, now: Instant) {
    prune_expired_app_writes(writes, now);
    if writes.len() >= RECENT_APP_WRITE_CAPACITY {
        if let Some(oldest) = writes
            .iter()
            .min_by_key(|(_, recorded)| **recorded)
            .map(|(key, _)| key.clone())
        {
            writes.remove(&oldest);
        }
    }
    writes.insert(watcher_path_key(path), now);
}

fn recent_app_write_recorded(writes: &RecentAppWrites, path: &Path, now: Instant) -> bool {
    writes
        .get(&watcher_path_key(path))
        .is_some_and(|recorded| recent_app_write_within_window(*recorded, now))
}

fn recent_app_write_within_window(recorded: Instant, now: Instant) -> bool {
    if recorded > now {
        return true;
    }
    now.duration_since(recorded) <= Duration::from_millis(SELF_WRITE_SUPPRESSION_WINDOW_MS)
}

fn prune_expired_app_writes(writes: &mut RecentAppWrites, now: Instant) {
    writes.retain(|_, recorded| recent_app_write_within_window(*recorded, now));
}

pub(crate) struct FileWatcher {
    _watcher: RecommendedWatcher,
    _auxiliary_watcher: Option<RecommendedWatcher>,
    root: PathBuf,
    rx: Receiver<PathBuf>,
    overflowed: Arc<AtomicBool>,

    git_watch_dirs: Vec<PathBuf>,

    attempted_git_roots: Vec<PathBuf>,
}

#[derive(Default)]
pub(crate) struct FileWatcherDrain {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) overflowed: bool,
}

impl FileWatcher {
    pub(crate) fn new(root: &Path) -> anyhow::Result<Self> {
        let (tx, rx) = bounded(FILE_WATCHER_CHANNEL_BOUND);
        let overflowed = Arc::new(AtomicBool::new(false));
        let auxiliary_watcher = auxiliary_root_watcher(root, tx.clone(), Arc::clone(&overflowed));
        let callback_overflowed = Arc::clone(&overflowed);
        let path_filter = WatcherPathFilter::for_workspace(root);
        let mut watcher = recommended_watcher(move |event: notify::Result<notify::Event>| {
            enqueue_watcher_event_filtered(&tx, &callback_overflowed, &path_filter, event);
        })?;
        watcher.watch(root, RecursiveMode::Recursive)?;
        Ok(Self {
            _watcher: watcher,
            _auxiliary_watcher: auxiliary_watcher,
            root: root.to_path_buf(),
            rx,
            overflowed,
            git_watch_dirs: workspace_git_watch_dirs(root),
            attempted_git_roots: Vec::new(),
        })
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn git_watch_dirs(&self) -> &[PathBuf] {
        &self.git_watch_dirs
    }

    pub(crate) fn ensure_git_dir_watched(&mut self, repo_root: &Path) {
        if self
            .attempted_git_roots
            .iter()
            .any(|attempted| trusted_workspace_paths_match(attempted, repo_root))
        {
            return;
        }
        self.attempted_git_roots.push(repo_root.to_path_buf());
        let Some(git_dir) = resolve_git_watch_dir(repo_root) else {
            return;
        };
        let watched = self
            ._auxiliary_watcher
            .as_mut()
            .is_some_and(|watcher| watcher.watch(&git_dir, RecursiveMode::Recursive).is_ok());
        if watched {
            self.git_watch_dirs.push(git_dir);
        }
    }

    pub(crate) fn drain(&self) -> FileWatcherDrain {
        drain_watched_paths(&self.rx, &self.overflowed, FILE_WATCHER_DRAIN_BUDGET)
    }
}

#[derive(Default)]
struct WatcherPathFilter {
    ignored_app_state: Vec<PathBuf>,
    settings: Option<PathBuf>,
    tasks: Option<PathBuf>,
    plugins: Option<PathBuf>,
}

impl WatcherPathFilter {
    fn for_workspace(root: &Path) -> Self {
        let ignored_app_state = app_owned_state_dirs_in_workspace(root);
        if ignored_app_state.is_empty() {
            return Self::default();
        }
        Self {
            ignored_app_state,
            settings: Some(settings_path(root)),
            tasks: Some(workspace_tasks_path(root)),
            plugins: Some(workspace_plugins_dir(root)),
        }
    }

    fn should_enqueue(&self, path: &Path) -> bool {
        if !self
            .ignored_app_state
            .iter()
            .any(|app_state| workspace_path_contains_lexically(app_state, path))
        {
            return true;
        }
        self.settings
            .as_ref()
            .is_some_and(|settings| trusted_workspace_paths_match(settings, path))
            || self
                .tasks
                .as_ref()
                .is_some_and(|tasks| trusted_workspace_paths_match(tasks, path))
            || self
                .plugins
                .as_ref()
                .is_some_and(|plugins| workspace_path_contains_lexically(plugins, path))
    }
}

#[cfg(test)]
fn enqueue_watcher_event(
    tx: &Sender<PathBuf>,
    overflowed: &AtomicBool,
    event: notify::Result<notify::Event>,
) {
    enqueue_watcher_event_filtered(tx, overflowed, &WatcherPathFilter::default(), event);
}

fn auxiliary_root_watcher(
    root: &Path,
    tx: Sender<PathBuf>,
    overflowed: Arc<AtomicBool>,
) -> Option<RecommendedWatcher> {
    let path_filter = WatcherPathFilter::for_workspace(root);
    let mut watcher = recommended_watcher(move |event: notify::Result<notify::Event>| {
        enqueue_watcher_event_filtered(&tx, &overflowed, &path_filter, event);
    })
    .ok()?;

    let _ = watcher.watch(&app_state_dir(), RecursiveMode::NonRecursive);
    let dot_git = root.join(".git");
    if dot_git.is_dir() {
        let _ = watcher.watch(&dot_git, RecursiveMode::NonRecursive);
    } else if let Some(git_dir) = git_dir_from_gitfile(&dot_git) {
        let _ = watcher.watch(&git_dir, RecursiveMode::Recursive);
    }
    Some(watcher)
}

fn resolve_git_watch_dir(repo_root: &Path) -> Option<PathBuf> {
    let dot_git = repo_root.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    git_dir_from_gitfile(&dot_git)
}

fn git_dir_from_gitfile(dot_git: &Path) -> Option<PathBuf> {
    let contents = fs::read_to_string(dot_git).ok()?;
    let target = contents
        .lines()
        .next()?
        .trim()
        .strip_prefix("gitdir:")
        .map(str::trim)?;
    if target.is_empty() {
        return None;
    }
    let target = PathBuf::from(target);
    let resolved = if target.is_absolute() {
        target
    } else {
        dot_git.parent()?.join(target)
    };
    let resolved = normalize_lexical_path(&resolved);
    resolved.is_dir().then_some(resolved)
}

fn normalize_lexical_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    let mut has_root = false;
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => {
                has_root = true;
                normalized.push(component.as_os_str());
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
    normalized
}

fn workspace_git_watch_dirs(root: &Path) -> Vec<PathBuf> {
    resolve_git_watch_dir(root).into_iter().collect()
}

fn enqueue_watcher_event_filtered(
    tx: &Sender<PathBuf>,
    overflowed: &AtomicBool,
    path_filter: &WatcherPathFilter,
    event: notify::Result<notify::Event>,
) {
    match event {
        Ok(event) => {
            if event.need_rescan() {
                overflowed.store(true, Ordering::SeqCst);
            }
            if watcher_event_kind_affects_filesystem(event.kind)
                && enqueue_unique_watched_paths(
                    tx,
                    overflowed,
                    event
                        .paths
                        .into_iter()
                        .filter(|path| path_filter.should_enqueue(path)),
                )
            {
                notify_background_activity();
            }
        }
        Err(error) => {
            overflowed.store(true, Ordering::SeqCst);
            if enqueue_unique_watched_paths(
                tx,
                overflowed,
                error
                    .paths
                    .into_iter()
                    .filter(|path| path_filter.should_enqueue(path)),
            ) {
                notify_background_activity();
            }
        }
    }
}

fn watcher_event_kind_affects_filesystem(kind: EventKind) -> bool {
    match kind {
        EventKind::Access(AccessKind::Close(
            AccessMode::Write | AccessMode::Any | AccessMode::Other,
        )) => true,
        EventKind::Access(_) => false,
        EventKind::Modify(ModifyKind::Metadata(
            MetadataKind::AccessTime
            | MetadataKind::Permissions
            | MetadataKind::Ownership
            | MetadataKind::Extended,
        )) => false,
        EventKind::Modify(ModifyKind::Metadata(_)) => true,
        EventKind::Any
        | EventKind::Other
        | EventKind::Create(_)
        | EventKind::Modify(
            ModifyKind::Any | ModifyKind::Data(_) | ModifyKind::Name(_) | ModifyKind::Other,
        )
        | EventKind::Remove(_) => true,
    }
}

fn enqueue_unique_watched_paths(
    tx: &Sender<PathBuf>,
    overflowed: &AtomicBool,
    paths: impl IntoIterator<Item = PathBuf>,
) -> bool {
    let mut deduped: Vec<PathBuf> = paths.into_iter().collect();
    collapse_case_only_path_collisions(&mut deduped);
    let mut sent_any = false;
    for path in deduped {
        if !enqueue_watched_path(tx, overflowed, path) {
            break;
        }
        sent_any = true;
    }
    sent_any
}

fn enqueue_watched_path(tx: &Sender<PathBuf>, overflowed: &AtomicBool, path: PathBuf) -> bool {
    match tx.try_send(path) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) => {
            overflowed.store(true, Ordering::SeqCst);
            false
        }
        Err(TrySendError::Disconnected(_)) => false,
    }
}

pub(crate) fn collapse_case_only_path_collisions(paths: &mut Vec<PathBuf>) {
    let mut representatives: HashMap<WatcherPathKey, usize> = HashMap::with_capacity(paths.len());
    let mut colliding_keys: HashSet<WatcherPathKey> = HashSet::new();
    for (index, path) in paths.iter().enumerate() {
        let key = watcher_path_key(path);
        match representatives.get(&key) {
            None => {
                representatives.insert(key, index);
            }
            Some(&first)
                if normalize_lexical_path(&paths[first]) != normalize_lexical_path(path) =>
            {
                colliding_keys.insert(key);
            }
            Some(_) => {}
        }
    }
    if colliding_keys.is_empty() {
        let mut seen = HashSet::with_capacity(representatives.len());
        paths.retain(|path| seen.insert(watcher_path_key(path)));
        return;
    }
    let mut deduped: Vec<PathBuf> = Vec::with_capacity(paths.len());
    let mut seen: HashSet<WatcherPathKey> = HashSet::with_capacity(representatives.len());
    for (index, path) in paths.iter().enumerate() {
        let key = watcher_path_key(path);
        if colliding_keys.contains(&key) {
            if representatives[&key] != index {
                continue;
            }
            match path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                Some(parent) => {
                    if seen.insert(watcher_path_key(parent)) {
                        deduped.push(parent.to_path_buf());
                    }
                }

                None => {
                    if seen.insert(key) {
                        deduped.push(path.clone());
                    }
                }
            }
            continue;
        }
        if seen.insert(key) {
            deduped.push(path.clone());
        }
    }
    *paths = deduped;
}

fn drain_watched_paths(
    rx: &Receiver<PathBuf>,
    overflowed: &AtomicBool,
    max_paths: usize,
) -> FileWatcherDrain {
    let mut overflowed = overflowed.swap(false, Ordering::SeqCst);
    let mut raw_paths = Vec::with_capacity(max_paths.min(FILE_WATCHER_DRAIN_BUDGET));
    let read_budget = FILE_WATCHER_CHANNEL_BOUND.max(max_paths);
    while raw_paths.len() < read_budget {
        let Ok(path) = rx.try_recv() else {
            break;
        };
        raw_paths.push(path);
    }
    if raw_paths.len() >= read_budget && !rx.is_empty() {
        overflowed = true;
    }
    collapse_case_only_path_collisions(&mut raw_paths);
    let mut paths = raw_paths;
    if paths.len() > max_paths {
        overflowed = true;
        paths.truncate(max_paths);
    }
    FileWatcherDrain { paths, overflowed }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct WatcherPathKey {
    prefix: Option<String>,
    rooted: bool,
    components: Vec<String>,
}

fn watcher_path_key(path: &Path) -> WatcherPathKey {
    let mut key = WatcherPathKey {
        prefix: None,
        rooted: false,
        components: Vec::new(),
    };

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                key.prefix = Some(watcher_path_component_key(prefix.as_os_str()));
            }
            Component::RootDir => key.rooted = true,
            Component::CurDir => {}
            Component::ParentDir => {
                if key
                    .components
                    .last()
                    .is_some_and(|component| component != "..")
                {
                    key.components.pop();
                } else if !key.rooted {
                    key.components.push("..".to_owned());
                }
            }
            Component::Normal(component) => {
                key.components.push(watcher_path_component_key(component));
            }
        }
    }

    key
}

fn watcher_path_component_key(component: &OsStr) -> String {
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

#[cfg(test)]
mod tests {
    use super::{
        RECENT_APP_WRITE_CAPACITY, SELF_WRITE_SUPPRESSION_WINDOW_MS, WatcherPathFilter,
        drain_watched_paths, enqueue_watched_path, enqueue_watcher_event,
        enqueue_watcher_event_filtered, is_recent_app_write, note_app_write,
        prune_expired_app_writes, recent_app_write_recorded, record_recent_app_write,
    };
    use crossbeam_channel::bounded;
    use notify::{
        Event, EventKind,
        event::{
            AccessKind, AccessMode, CreateKind, DataChange, Flag, MetadataKind, ModifyKind,
            RemoveKind, RenameMode,
        },
    };
    use std::{
        collections::HashMap,
        path::{Path, PathBuf},
        sync::atomic::{AtomicBool, Ordering},
        time::{Duration, Instant},
    };

    #[test]
    fn recent_app_writes_suppress_matching_paths_within_window() {
        let mut writes = HashMap::new();
        let now = Instant::now();
        record_recent_app_write(
            &mut writes,
            Path::new("kuroya-self-write/a/src/main.rs"),
            now,
        );

        assert!(recent_app_write_recorded(
            &writes,
            Path::new("kuroya-self-write/a/src/main.rs"),
            now + Duration::from_millis(SELF_WRITE_SUPPRESSION_WINDOW_MS)
        ));
        assert!(recent_app_write_recorded(
            &writes,
            Path::new("kuroya-self-write/a/src/./main.rs"),
            now
        ));
        assert!(!recent_app_write_recorded(
            &writes,
            Path::new("kuroya-self-write/a/src/lib.rs"),
            now
        ));

        prune_expired_app_writes(
            &mut writes,
            now + Duration::from_millis(SELF_WRITE_SUPPRESSION_WINDOW_MS + 1),
        );
        assert!(writes.is_empty());
    }

    #[test]
    fn recent_app_writes_expire_after_the_suppression_window() {
        let mut writes = HashMap::new();
        let now = Instant::now();
        record_recent_app_write(
            &mut writes,
            Path::new("kuroya-self-write/b/src/main.rs"),
            now,
        );
        let expired = now + Duration::from_millis(SELF_WRITE_SUPPRESSION_WINDOW_MS + 1);

        assert!(!recent_app_write_recorded(
            &writes,
            Path::new("kuroya-self-write/b/src/main.rs"),
            expired
        ));
        assert!(!recent_app_write_recorded(
            &writes,
            Path::new("kuroya-self-write/b/src/generated/../main.rs"),
            expired
        ));
    }

    #[test]
    fn recent_app_writes_prune_oldest_entries_beyond_capacity() {
        let mut writes = HashMap::new();
        let now = Instant::now();
        for index in 0..RECENT_APP_WRITE_CAPACITY {
            record_recent_app_write(
                &mut writes,
                Path::new(&format!("kuroya-self-write/c/file-{index}.rs")),
                now - Duration::from_millis((RECENT_APP_WRITE_CAPACITY - index) as u64),
            );
        }
        let newest = format!("kuroya-self-write/c/file-{0}.rs", RECENT_APP_WRITE_CAPACITY);
        record_recent_app_write(&mut writes, Path::new(&newest), now);

        assert_eq!(writes.len(), RECENT_APP_WRITE_CAPACITY);
        assert!(!recent_app_write_recorded(
            &writes,
            Path::new("kuroya-self-write/c/file-0.rs"),
            now
        ));
        assert!(recent_app_write_recorded(&writes, Path::new(&newest), now));
    }

    #[test]
    fn note_app_write_marks_only_the_recorded_path_as_recent() {
        let path = PathBuf::from(format!(
            "kuroya-self-write/d/unique-{}-{:?}/main.rs",
            std::process::id(),
            std::thread::current().id()
        ));
        note_app_write(&path);

        assert!(is_recent_app_write(&path));
        let equivalent = path
            .parent()
            .and_then(|parent| path.file_name().map(|name| parent.join(".").join(name)))
            .expect("recorded path should have a parent and name");
        assert!(is_recent_app_write(&equivalent));
        assert!(!is_recent_app_write(&path.with_file_name("lib.rs")));
    }

    #[test]
    fn watcher_queue_overflow_is_signaled_without_blocking() {
        let (tx, rx) = bounded(1);
        let overflowed = AtomicBool::new(false);

        assert!(enqueue_watched_path(
            &tx,
            &overflowed,
            PathBuf::from("workspace/src/main.rs")
        ));
        assert!(!enqueue_watched_path(
            &tx,
            &overflowed,
            PathBuf::from("workspace/src/lib.rs")
        ));

        assert!(overflowed.load(Ordering::SeqCst));
        assert_eq!(rx.len(), 1);
    }

    #[test]
    fn watcher_drain_reports_and_resets_overflow() {
        let (tx, rx) = bounded(4);
        let overflowed = AtomicBool::new(true);
        tx.send(PathBuf::from("workspace/src/main.rs")).unwrap();
        tx.send(PathBuf::from("workspace/src/lib.rs")).unwrap();

        let drain = drain_watched_paths(&rx, &overflowed, 1);

        assert!(drain.overflowed);
        assert_eq!(drain.paths, [PathBuf::from("workspace/src/main.rs")]);
        assert!(!overflowed.load(Ordering::SeqCst));
        assert!(rx.is_empty());

        let drain = drain_watched_paths(&rx, &overflowed, 8);

        assert!(!drain.overflowed);
        assert!(drain.paths.is_empty());
    }

    #[test]
    fn watcher_callback_errors_force_refresh_and_keep_error_paths() {
        let (tx, rx) = bounded(4);
        let overflowed = AtomicBool::new(false);
        let path = PathBuf::from("workspace/src/main.rs");

        enqueue_watcher_event(
            &tx,
            &overflowed,
            Err(notify::Error::generic("watcher dropped events").add_path(path.clone())),
        );
        let drain = drain_watched_paths(&rx, &overflowed, 8);

        assert!(drain.overflowed);
        assert_eq!(drain.paths, [path]);
        assert!(!overflowed.load(Ordering::SeqCst));
    }

    #[test]
    fn watcher_filters_app_state_echoes_but_keeps_special_paths() {
        let (tx, rx) = bounded(8);
        let overflowed = AtomicBool::new(false);
        let root = PathBuf::from("workspace");
        let app_state = root.join("runtime-state");
        let settings = app_state.join("settings.toml");
        let tasks = app_state.join("tasks.toml");
        let plugins = app_state.join("plugins");
        let project = root.join("src/main.rs");
        let filter = WatcherPathFilter {
            ignored_app_state: vec![app_state.clone()],
            settings: Some(settings.clone()),
            tasks: Some(tasks.clone()),
            plugins: Some(plugins.clone()),
        };

        enqueue_watcher_event_filtered(
            &tx,
            &overflowed,
            &filter,
            Ok(Event::new(EventKind::Any)
                .add_path(app_state.join("state.json"))
                .add_path(app_state.join("workspaces/current/session.json"))
                .add_path(settings.clone())
                .add_path(tasks.clone())
                .add_path(plugins.join("example/plugin.toml"))
                .add_path(project.clone())),
        );
        let drain = drain_watched_paths(&rx, &overflowed, 8);

        assert!(!drain.overflowed);
        assert_eq!(
            drain.paths,
            [
                settings,
                tasks,
                plugins.join("example/plugin.toml"),
                project
            ]
        );
    }

    #[test]
    fn watcher_rescan_events_force_refresh_even_without_paths() {
        let (tx, rx) = bounded(4);
        let overflowed = AtomicBool::new(false);

        enqueue_watcher_event(
            &tx,
            &overflowed,
            Ok(Event::new(EventKind::Any).set_flag(Flag::Rescan)),
        );
        let drain = drain_watched_paths(&rx, &overflowed, 8);

        assert!(drain.overflowed);
        assert!(drain.paths.is_empty());
    }

    #[test]
    fn watcher_ignores_non_mutating_access_events() {
        for kind in [
            EventKind::Access(AccessKind::Read),
            EventKind::Access(AccessKind::Any),
            EventKind::Access(AccessKind::Other),
            EventKind::Access(AccessKind::Open(AccessMode::Read)),
            EventKind::Access(AccessKind::Open(AccessMode::Write)),
            EventKind::Access(AccessKind::Close(AccessMode::Read)),
            EventKind::Access(AccessKind::Close(AccessMode::Execute)),
        ] {
            let (overflowed, paths) = drain_single_event(kind);

            assert!(!overflowed);
            assert!(paths.is_empty());
        }
    }

    #[test]
    fn watcher_keeps_write_capable_close_access_events() {
        for kind in [
            EventKind::Access(AccessKind::Close(AccessMode::Write)),
            EventKind::Access(AccessKind::Close(AccessMode::Any)),
            EventKind::Access(AccessKind::Close(AccessMode::Other)),
        ] {
            let (overflowed, paths) = drain_single_event(kind);

            assert!(!overflowed);
            assert_eq!(paths, [PathBuf::from("workspace/src/main.rs")]);
        }
    }

    #[test]
    fn watcher_ignores_noisy_metadata_events() {
        for kind in [
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::AccessTime)),
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::Permissions)),
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::Ownership)),
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::Extended)),
        ] {
            let (overflowed, paths) = drain_single_event(kind);

            assert!(!overflowed);
            assert!(paths.is_empty());
        }
    }

    #[test]
    fn watcher_keeps_write_time_and_unknown_metadata_events() {
        for kind in [
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::WriteTime)),
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::Any)),
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::Other)),
        ] {
            let (overflowed, paths) = drain_single_event(kind);

            assert!(!overflowed);
            assert_eq!(paths, [PathBuf::from("workspace/src/main.rs")]);
        }
    }

    #[test]
    fn watcher_keeps_mutating_events() {
        for kind in [
            EventKind::Any,
            EventKind::Other,
            EventKind::Create(CreateKind::File),
            EventKind::Modify(ModifyKind::Data(DataChange::Content)),
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            EventKind::Remove(RemoveKind::File),
        ] {
            let (overflowed, paths) = drain_single_event(kind);

            assert!(!overflowed);
            assert_eq!(paths, [PathBuf::from("workspace/src/main.rs")]);
        }
    }

    #[test]
    fn watcher_rescan_on_filtered_events_still_forces_refresh() {
        let (tx, rx) = bounded(4);
        let overflowed = AtomicBool::new(false);
        let path = PathBuf::from("workspace/src/main.rs");

        enqueue_watcher_event(
            &tx,
            &overflowed,
            Ok(Event::new(EventKind::Access(AccessKind::Read))
                .set_flag(Flag::Rescan)
                .add_path(path)),
        );
        let drain = drain_watched_paths(&rx, &overflowed, 8);

        assert!(drain.overflowed);
        assert!(drain.paths.is_empty());
    }

    #[test]
    fn watcher_event_paths_are_deduplicated_before_queueing() {
        let (tx, rx) = bounded(4);
        let overflowed = AtomicBool::new(false);
        let path = PathBuf::from("workspace/src/main.rs");

        enqueue_watcher_event(
            &tx,
            &overflowed,
            Ok(Event::new(EventKind::Any)
                .add_path(path.clone())
                .add_path(path.clone())),
        );
        let drain = drain_watched_paths(&rx, &overflowed, 8);

        assert!(!drain.overflowed);
        assert_eq!(drain.paths, [path]);
    }

    #[test]
    fn watcher_event_paths_are_lexically_deduplicated_before_queueing() {
        let (tx, rx) = bounded(4);
        let overflowed = AtomicBool::new(false);
        let raw_path = PathBuf::from("workspace/src/./main.rs");
        let equivalent_path = PathBuf::from("workspace/src/generated/../main.rs");

        enqueue_watcher_event(
            &tx,
            &overflowed,
            Ok(Event::new(EventKind::Any)
                .add_path(raw_path.clone())
                .add_path(equivalent_path)),
        );
        let drain = drain_watched_paths(&rx, &overflowed, 8);

        assert!(!drain.overflowed);
        assert_eq!(drain.paths, [raw_path]);
    }

    #[test]
    fn watcher_drain_collapses_duplicate_bursts_before_later_paths() {
        let (tx, rx) = bounded(8);
        let overflowed = AtomicBool::new(false);
        let first = PathBuf::from("workspace/src/main.rs");
        let second = PathBuf::from("workspace/src/lib.rs");
        tx.send(first.clone()).unwrap();
        tx.send(first.clone()).unwrap();
        tx.send(first.clone()).unwrap();
        tx.send(second.clone()).unwrap();

        let drain = drain_watched_paths(&rx, &overflowed, 2);

        assert!(!drain.overflowed);
        assert_eq!(drain.paths, [first, second]);
        assert!(rx.is_empty());
    }

    #[test]
    fn watcher_drain_collapses_equivalent_bursts_before_later_paths() {
        let (tx, rx) = bounded(8);
        let overflowed = AtomicBool::new(false);
        let first = PathBuf::from("workspace/src/./main.rs");
        let first_equivalent = PathBuf::from("workspace/src/generated/../main.rs");
        let second = PathBuf::from("workspace/src/lib.rs");
        tx.send(first.clone()).unwrap();
        tx.send(first_equivalent).unwrap();
        tx.send(second.clone()).unwrap();

        let drain = drain_watched_paths(&rx, &overflowed, 2);

        assert!(!drain.overflowed);
        assert_eq!(drain.paths, [first, second]);
        assert!(rx.is_empty());
    }

    #[test]
    fn watcher_drain_signals_overflow_when_unique_paths_exceed_budget() {
        let (tx, rx) = bounded(8);
        let overflowed = AtomicBool::new(false);
        let first = PathBuf::from("workspace/src/main.rs");
        let second = PathBuf::from("workspace/src/lib.rs");
        tx.send(first.clone()).unwrap();
        tx.send(second).unwrap();

        let drain = drain_watched_paths(&rx, &overflowed, 1);

        assert!(drain.overflowed);
        assert_eq!(drain.paths, [first]);
        assert!(rx.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn watcher_drain_replaces_case_only_renames_with_the_parent_directory() {
        let (tx, rx) = bounded(4);
        let overflowed = AtomicBool::new(false);
        let old = PathBuf::from("workspace/src/Foo.rs");
        let new = PathBuf::from("workspace/src/foo.rs");

        enqueue_watcher_event(
            &tx,
            &overflowed,
            Ok(Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::From))).add_path(old)),
        );
        enqueue_watcher_event(
            &tx,
            &overflowed,
            Ok(Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::To))).add_path(new)),
        );
        let drain = drain_watched_paths(&rx, &overflowed, 8);

        assert!(!drain.overflowed);
        assert_eq!(drain.paths, [PathBuf::from("workspace/src")]);
    }

    #[cfg(windows)]
    #[test]
    fn watcher_events_collapse_case_only_renames_to_the_parent_directory() {
        let (tx, rx) = bounded(4);
        let overflowed = AtomicBool::new(false);

        enqueue_watcher_event(
            &tx,
            &overflowed,
            Ok(
                Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
                    .add_path(PathBuf::from("workspace/src/Foo.rs"))
                    .add_path(PathBuf::from("workspace/src/foo.rs")),
            ),
        );
        let drain = drain_watched_paths(&rx, &overflowed, 8);

        assert!(!drain.overflowed);
        assert_eq!(drain.paths, [PathBuf::from("workspace/src")]);
    }

    #[cfg(windows)]
    #[test]
    fn watcher_drain_keeps_one_parent_when_the_batch_already_has_it() {
        let (tx, rx) = bounded(8);
        let overflowed = AtomicBool::new(false);
        tx.send(PathBuf::from("workspace/src")).unwrap();
        tx.send(PathBuf::from("workspace/src/Foo.rs")).unwrap();
        tx.send(PathBuf::from("workspace/src/foo.rs")).unwrap();

        let drain = drain_watched_paths(&rx, &overflowed, 8);

        assert!(!drain.overflowed);
        assert_eq!(drain.paths, [PathBuf::from("workspace/src")]);
    }

    fn drain_single_event(kind: EventKind) -> (bool, Vec<PathBuf>) {
        let (tx, rx) = bounded(4);
        let overflowed = AtomicBool::new(false);
        let path = PathBuf::from("workspace/src/main.rs");

        enqueue_watcher_event(&tx, &overflowed, Ok(Event::new(kind).add_path(path)));
        let drain = drain_watched_paths(&rx, &overflowed, 8);

        (drain.overflowed, drain.paths)
    }
}

#[cfg(test)]
mod git_watch_tests {
    use super::{FileWatcher, git_dir_from_gitfile, workspace_git_watch_dirs};
    use std::{fs, path::PathBuf};

    fn unique_temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "kuroya-git-watch-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ))
    }

    #[test]
    fn git_dir_from_gitfile_resolves_relative_and_absolute_gitdirs() {
        let root = unique_temp_root("gitfile");
        let git_dir = root.join("parent-repo").join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        let dot_git = root.join("worktree").join(".git");
        fs::create_dir_all(dot_git.parent().unwrap()).unwrap();

        fs::write(&dot_git, "gitdir: ../parent-repo/.git\n").unwrap();
        assert_eq!(git_dir_from_gitfile(&dot_git), Some(git_dir.clone()));

        fs::write(&dot_git, format!("gitdir: {}", git_dir.display())).unwrap();
        assert_eq!(git_dir_from_gitfile(&dot_git), Some(git_dir));

        fs::write(&dot_git, "not a gitdir file").unwrap();
        assert_eq!(git_dir_from_gitfile(&dot_git), None);
        fs::write(&dot_git, "gitdir: ../missing/.git").unwrap();
        assert_eq!(git_dir_from_gitfile(&dot_git), None);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn workspace_git_watch_dirs_cover_plain_and_worktree_git_dirs() {
        let root = unique_temp_root("dirs");
        let plain = root.join("plain");
        fs::create_dir_all(plain.join(".git")).unwrap();
        let linked = root.join("linked");
        fs::create_dir_all(linked.join(".git").parent().unwrap()).unwrap();
        let git_dir = root.join("main").join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(
            linked.join(".git"),
            format!("gitdir: {}", git_dir.display()),
        )
        .unwrap();

        assert_eq!(workspace_git_watch_dirs(&plain), vec![plain.join(".git")]);
        assert_eq!(workspace_git_watch_dirs(&linked), vec![git_dir]);

        let bare = root.join("bare");
        fs::create_dir_all(&bare).unwrap();
        assert!(workspace_git_watch_dirs(&bare).is_empty());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ensure_git_dir_watched_registers_parent_repository_git_dir_once() {
        let root = unique_temp_root("parent");
        let workspace = root.join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let parent_git = root.join("parent").join(".git");
        fs::create_dir_all(&parent_git).unwrap();
        let mut watcher = FileWatcher::new(&workspace).expect("watcher on temp workspace");

        assert!(watcher.git_watch_dirs().is_empty());
        watcher.ensure_git_dir_watched(parent_git.parent().unwrap());
        assert_eq!(watcher.git_watch_dirs(), std::slice::from_ref(&parent_git));

        watcher.ensure_git_dir_watched(parent_git.parent().unwrap());
        assert_eq!(watcher.git_watch_dirs(), std::slice::from_ref(&parent_git));

        fs::remove_dir_all(root).unwrap();
    }
}
