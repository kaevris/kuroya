use crate::{
    persistence::PersistedSession,
    persistence_session::{
        PERSISTED_SESSION_MAX_BYTES, normalize_persisted_session_paths_for_restore,
        persisted_session_workspace_matches, session_bytes_for_write,
    },
    persistence_storage::{
        atomic_write, legacy_workspace_snapshots_dir, read_file_bytes_with_limit,
        workspace_snapshots_dir,
    },
};
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) const MAX_WORKSPACE_SNAPSHOTS: usize = 24;
const WORKSPACE_SNAPSHOT_SCAN_LIMIT: usize = MAX_WORKSPACE_SNAPSHOTS * 128;
const WORKSPACE_SNAPSHOT_SCAN_TRIM_AT: usize = WORKSPACE_SNAPSHOT_SCAN_LIMIT * 2;
static WORKSPACE_SNAPSHOT_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LoadedWorkspaceSnapshot {
    pub(crate) path: PathBuf,
    pub(crate) session: PersistedSession,
}

pub(crate) fn save_workspace_snapshot(
    workspace_root: &Path,
    session: &PersistedSession,
) -> anyhow::Result<PathBuf> {
    if !persisted_session_workspace_matches(workspace_root, session) {
        anyhow::bail!(
            "workspace snapshot session root does not match workspace root: {}",
            session.workspace_root.display()
        );
    }

    let dir = workspace_snapshots_dir(workspace_root);
    fs::create_dir_all(&dir)?;
    let bytes = session_bytes_for_write(session)?;
    if let Some(path) = latest_duplicate_workspace_snapshot_path(&dir, &bytes)? {
        prune_workspace_snapshots(&dir)?;
        return Ok(path);
    }
    let path = unique_workspace_snapshot_path(&dir);
    atomic_write(&path, &bytes)?;
    prune_workspace_snapshots(&dir)?;
    Ok(path)
}

pub(crate) fn load_latest_workspace_snapshot(
    workspace_root: &Path,
) -> anyhow::Result<Option<LoadedWorkspaceSnapshot>> {
    if let Some(snapshot) = load_latest_workspace_snapshot_with_quarantine(
        workspace_root,
        quarantine_workspace_snapshot,
        quarantine_mismatched_workspace_snapshot,
    )? {
        return Ok(Some(snapshot));
    }

    let legacy_dir = legacy_workspace_snapshots_dir(workspace_root);
    load_latest_workspace_snapshot_in_dir(
        workspace_root,
        &legacy_dir,
        quarantine_workspace_snapshot,
        quarantine_mismatched_workspace_snapshot,
    )
}

fn load_latest_workspace_snapshot_with_quarantine(
    workspace_root: &Path,
    mut quarantine_corrupt: impl FnMut(&Path) -> anyhow::Result<PathBuf>,
    mut quarantine_mismatched: impl FnMut(&Path) -> anyhow::Result<PathBuf>,
) -> anyhow::Result<Option<LoadedWorkspaceSnapshot>> {
    let dir = workspace_snapshots_dir(workspace_root);
    load_latest_workspace_snapshot_in_dir(
        workspace_root,
        &dir,
        &mut quarantine_corrupt,
        &mut quarantine_mismatched,
    )
}

fn load_latest_workspace_snapshot_in_dir(
    workspace_root: &Path,
    dir: &Path,
    mut quarantine_corrupt: impl FnMut(&Path) -> anyhow::Result<PathBuf>,
    mut quarantine_mismatched: impl FnMut(&Path) -> anyhow::Result<PathBuf>,
) -> anyhow::Result<Option<LoadedWorkspaceSnapshot>> {
    let mut snapshots = workspace_snapshot_files_in_dir(dir)?;
    while let Some(path) = snapshots.pop() {
        let bytes = match read_file_bytes_with_limit(&path, PERSISTED_SESSION_MAX_BYTES) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) if error.kind() == ErrorKind::InvalidData => {
                let _ = quarantine_corrupt(&path);
                continue;
            }
            Err(error) => return Err(error.into()),
        };

        match serde_json::from_slice::<PersistedSession>(&bytes) {
            Ok(mut session) if persisted_session_workspace_matches(workspace_root, &session) => {
                normalize_persisted_session_paths_for_restore(workspace_root, &mut session);
                return Ok(Some(LoadedWorkspaceSnapshot { path, session }));
            }
            Ok(_) => {
                let _ = quarantine_mismatched(&path);
            }
            Err(_) => {
                let _ = quarantine_corrupt(&path);
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
pub(crate) fn workspace_snapshot_files(workspace_root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    workspace_snapshot_files_in_dir(&workspace_snapshots_dir(workspace_root))
}

fn unique_workspace_snapshot_path(dir: &Path) -> PathBuf {
    dir.join(format!(
        "workspace.{}.{}.{:016}.json",
        workspace_snapshot_unique_id(),
        std::process::id(),
        WORKSPACE_SNAPSHOT_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

fn workspace_snapshot_unique_id() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

fn latest_duplicate_workspace_snapshot_path(
    dir: &Path,
    bytes: &[u8],
) -> anyhow::Result<Option<PathBuf>> {
    let mut snapshots = workspace_snapshot_files_in_dir(dir)?;
    while let Some(path) = snapshots.pop() {
        let existing = match read_file_bytes_with_limit(&path, PERSISTED_SESSION_MAX_BYTES) {
            Ok(existing) => existing,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) if error.kind() == ErrorKind::InvalidData => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        return Ok((existing.as_slice() == bytes).then_some(path));
    }
    Ok(None)
}

fn prune_workspace_snapshots(dir: &Path) -> anyhow::Result<()> {
    let mut snapshots = workspace_snapshot_files_in_dir(dir)?;
    sort_workspace_snapshots_oldest_modified_first(&mut snapshots);
    let overflow = snapshots.len().saturating_sub(MAX_WORKSPACE_SNAPSHOTS);
    for path in snapshots.into_iter().take(overflow) {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Orders snapshots oldest-modified first so pruning from the front evicts
/// the oldest writes and keeps the newest. Snapshot names embed wall-clock
/// nanos, so after a system-clock rollback freshly written snapshots sort
/// OLDEST by name and name-order pruning would freeze the safety net
/// exactly when it is needed; the on-disk modified time tracks real write
/// order instead. Entries whose modified time cannot be read are treated
/// as oldest (pruned first, keeping their relative name order); entries
/// with readable times keep name order on ties (the stable-sort fallback).
fn sort_workspace_snapshots_oldest_modified_first(snapshots: &mut [PathBuf]) {
    snapshots.sort_by_key(|path| snapshot_modified_time(path).unwrap_or(UNIX_EPOCH));
}

fn snapshot_modified_time(path: &Path) -> Option<SystemTime> {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
}

fn workspace_snapshot_files_in_dir(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut snapshots = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        if !entry.file_type().is_ok_and(|file_type| file_type.is_file()) {
            continue;
        }

        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if file_name.starts_with("workspace.") && file_name.ends_with(".json") {
            push_workspace_snapshot_candidate(&mut snapshots, entry.path());
        }
    }
    trim_workspace_snapshot_candidates(&mut snapshots);
    sort_workspace_snapshot_paths(&mut snapshots);
    Ok(snapshots)
}

fn push_workspace_snapshot_candidate(snapshots: &mut Vec<PathBuf>, path: PathBuf) {
    snapshots.push(path);
    if snapshots.len() >= WORKSPACE_SNAPSHOT_SCAN_TRIM_AT {
        trim_workspace_snapshot_candidates(snapshots);
    }
}

fn trim_workspace_snapshot_candidates(snapshots: &mut Vec<PathBuf>) {
    let overflow = snapshots
        .len()
        .saturating_sub(WORKSPACE_SNAPSHOT_SCAN_LIMIT);
    if overflow == 0 {
        return;
    }
    sort_workspace_snapshot_paths(snapshots);
    snapshots.drain(0..overflow);
}

fn quarantine_workspace_snapshot(path: &Path) -> anyhow::Result<PathBuf> {
    let quarantine = corrupt_workspace_snapshot_path(path);
    fs::rename(path, &quarantine)?;
    Ok(quarantine)
}

fn corrupt_workspace_snapshot_path(path: &Path) -> PathBuf {
    quarantined_workspace_snapshot_path(path, "corrupt")
}

fn quarantine_mismatched_workspace_snapshot(path: &Path) -> anyhow::Result<PathBuf> {
    let quarantine = mismatched_workspace_snapshot_path(path);
    fs::rename(path, &quarantine)?;
    Ok(quarantine)
}

fn mismatched_workspace_snapshot_path(path: &Path) -> PathBuf {
    quarantined_workspace_snapshot_path(path, "mismatched")
}

fn quarantined_workspace_snapshot_path(path: &Path, reason: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("workspace.json");
    path.with_file_name(format!(
        "{file_name}.{reason}.{}.{}",
        std::process::id(),
        unique
    ))
}

fn sort_workspace_snapshot_paths(snapshots: &mut [PathBuf]) {
    // Preserve path tie-breaking without cloning every PathBuf into the cached key.
    snapshots.sort();
    snapshots.sort_by_cached_key(|path| workspace_snapshot_sort_key(path));
}

#[derive(Debug, PartialEq, Eq)]
enum WorkspaceSnapshotSortKey {
    Unparsed(String),
    Parsed {
        unique: u128,
        process_id: u32,
        counter: u64,
    },
}

impl Ord for WorkspaceSnapshotSortKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (Self::Unparsed(left), Self::Unparsed(right)) => left.cmp(right),
            (Self::Unparsed(_), Self::Parsed { .. }) => std::cmp::Ordering::Less,
            (Self::Parsed { .. }, Self::Unparsed(_)) => std::cmp::Ordering::Greater,
            (
                Self::Parsed {
                    unique,
                    process_id,
                    counter,
                },
                Self::Parsed {
                    unique: other_unique,
                    process_id: other_process_id,
                    counter: other_counter,
                },
            ) => {
                (unique, process_id, counter).cmp(&(other_unique, other_process_id, other_counter))
            }
        }
    }
}

impl PartialOrd for WorkspaceSnapshotSortKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn workspace_snapshot_sort_key(path: &Path) -> WorkspaceSnapshotSortKey {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let Some(stem) = file_name
        .strip_prefix("workspace.")
        .and_then(|name| name.strip_suffix(".json"))
    else {
        return WorkspaceSnapshotSortKey::Unparsed(file_name.to_owned());
    };
    let mut parts = stem.split('.');
    let (Some(unique), Some(process_id), Some(counter), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return WorkspaceSnapshotSortKey::Unparsed(file_name.to_owned());
    };
    let (Ok(unique), Ok(process_id), Ok(counter)) =
        (unique.parse(), process_id.parse(), counter.parse())
    else {
        return WorkspaceSnapshotSortKey::Unparsed(file_name.to_owned());
    };
    WorkspaceSnapshotSortKey::Parsed {
        unique,
        process_id,
        counter,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_WORKSPACE_SNAPSHOTS, WORKSPACE_SNAPSHOT_SCAN_LIMIT, load_latest_workspace_snapshot,
        load_latest_workspace_snapshot_with_quarantine, prune_workspace_snapshots,
        save_workspace_snapshot, sort_workspace_snapshot_paths, unique_workspace_snapshot_path,
        workspace_snapshot_files_in_dir,
    };
    use crate::{
        layout::{
            DIAGNOSTICS_PANEL_DEFAULT_WIDTH, EXPLORER_DEFAULT_WIDTH, SOURCE_CONTROL_DEFAULT_WIDTH,
            SYMBOLS_PANEL_DEFAULT_WIDTH, TERMINAL_DEFAULT_HEIGHT,
        },
        persistence::{PersistedSession, RecoveredBuffer},
        persistence_session::PERSISTED_SESSION_MAX_BYTES,
        persistence_storage::workspace_snapshots_dir,
    };
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn unique_workspace_snapshot_path_zero_pads_counter_for_lexical_sorting() {
        let path = unique_workspace_snapshot_path(Path::new("snapshots"));
        let file_name = path.file_name().unwrap().to_str().unwrap();
        let parts = file_name
            .strip_prefix("workspace.")
            .unwrap()
            .strip_suffix(".json")
            .unwrap()
            .split('.')
            .collect::<Vec<_>>();

        assert_eq!(parts.len(), 3);
        assert_eq!(parts[2].len(), 16);
    }

    #[test]
    fn workspace_snapshot_paths_sort_counters_numerically_and_legacy_first() {
        let mut snapshots = vec![
            PathBuf::from("snapshots/workspace.10.7.10.json"),
            PathBuf::from("snapshots/workspace.zzz.json"),
            PathBuf::from("snapshots/workspace.10.7.2.json"),
            PathBuf::from("snapshots/workspace.11.1.0.json"),
        ];

        sort_workspace_snapshot_paths(&mut snapshots);

        let names = snapshots
            .iter()
            .map(|path| path.file_name().unwrap().to_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "workspace.zzz.json",
                "workspace.10.7.2.json",
                "workspace.10.7.10.json",
                "workspace.11.1.0.json",
            ]
        );
    }

    #[test]
    fn workspace_snapshot_load_continues_when_corrupt_latest_quarantine_fails() {
        let workspace = temp_workspace("quarantine-fails");
        fs::create_dir_all(&workspace).unwrap();
        let valid = snapshot_session(&workspace, "valid workspace snapshot");
        let valid_path = save_workspace_snapshot(&workspace, &valid).unwrap();
        let snapshot_dir = workspace_snapshots_dir(&workspace);
        fs::write(
            snapshot_dir.join("workspace.999999999999999999999999999999.0.0000000000000000.json"),
            "{not valid json",
        )
        .unwrap();

        let loaded = load_latest_workspace_snapshot_with_quarantine(
            &workspace,
            |_| -> anyhow::Result<PathBuf> { anyhow::bail!("rename denied") },
            |_| -> anyhow::Result<PathBuf> { anyhow::bail!("rename denied") },
        )
        .unwrap()
        .unwrap();

        assert_eq!(loaded.path, valid_path);
        assert_eq!(loaded.session, valid);
        fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn workspace_snapshot_scan_bounds_candidates_and_keeps_generated_snapshot() {
        let workspace = temp_workspace("bounded-scan");
        let snapshot_dir = workspace_snapshots_dir(&workspace);
        fs::create_dir_all(&snapshot_dir).unwrap();
        for index in 0..(WORKSPACE_SNAPSHOT_SCAN_LIMIT + 16) {
            fs::write(
                snapshot_dir.join(format!("workspace.malformed-{index:04}.json")),
                "{not valid json",
            )
            .unwrap();
        }

        let valid = snapshot_session(&workspace, "valid bounded scan workspace snapshot");
        let valid_path =
            snapshot_dir.join("workspace.999999999999999999999999999999.0.0000000000000000.json");
        fs::write(&valid_path, serde_json::to_string_pretty(&valid).unwrap()).unwrap();

        let files = workspace_snapshot_files_in_dir(&snapshot_dir).unwrap();
        assert_eq!(files.len(), WORKSPACE_SNAPSHOT_SCAN_LIMIT);
        assert!(files.contains(&valid_path));

        let loaded = load_latest_workspace_snapshot(&workspace).unwrap().unwrap();
        assert_eq!(loaded.path, valid_path);
        assert_eq!(loaded.session, valid);

        fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn workspace_snapshot_load_quarantines_oversized_latest_and_restores_valid() {
        let workspace = temp_workspace("oversized-latest");
        fs::create_dir_all(&workspace).unwrap();
        let valid = snapshot_session(&workspace, "valid before oversized workspace snapshot");
        let valid_path = save_workspace_snapshot(&workspace, &valid).unwrap();
        let snapshot_dir = workspace_snapshots_dir(&workspace);
        let oversized_path =
            snapshot_dir.join("workspace.999999999999999999999999999999.0.0000000000000000.json");
        fs::write(
            &oversized_path,
            vec![b'x'; usize::try_from(PERSISTED_SESSION_MAX_BYTES + 1).unwrap()],
        )
        .unwrap();

        let loaded = load_latest_workspace_snapshot(&workspace).unwrap().unwrap();

        assert_eq!(loaded.path, valid_path);
        assert_eq!(loaded.session, valid);
        assert!(!oversized_path.exists());
        let quarantined = fs::read_dir(&snapshot_dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.contains(".corrupt."))
            })
            .collect::<Vec<_>>();
        assert_eq!(quarantined.len(), 1);
        assert_eq!(
            fs::metadata(quarantined[0].path()).unwrap().len(),
            PERSISTED_SESSION_MAX_BYTES + 1
        );

        fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn workspace_snapshot_prune_keeps_newest_modified_not_newest_name() {
        let workspace = temp_workspace("prune-mtime");
        let snapshot_dir = workspace_snapshots_dir(&workspace);
        fs::create_dir_all(&snapshot_dir).unwrap();
        let base = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let total = MAX_WORKSPACE_SNAPSHOTS + 2;
        for index in 0..total {
            let path = snapshot_dir.join(format!("workspace.{index}.0.0.json"));
            fs::write(&path, "{not valid json").unwrap();
            let file = fs::File::options().write(true).open(&path).unwrap();
            // Lowest names carry the newest modified times: the signature of
            // a clock rollback, where name order lies about write order.
            file.set_modified(base + Duration::from_secs((total - 1 - index) as u64))
                .unwrap();
        }

        prune_workspace_snapshots(&snapshot_dir).unwrap();

        // Snapshot 0 has the oldest name but the newest modified time;
        // name-order pruning would have evicted it first.
        assert!(snapshot_dir.join("workspace.0.0.0.json").exists());
        assert!(
            snapshot_dir
                .join(format!(
                    "workspace.{}.0.0.json",
                    MAX_WORKSPACE_SNAPSHOTS - 1
                ))
                .exists()
        );
        assert!(
            !snapshot_dir
                .join(format!("workspace.{MAX_WORKSPACE_SNAPSHOTS}.0.0.json"))
                .exists()
        );
        assert!(
            !snapshot_dir
                .join(format!("workspace.{}.0.0.json", total - 1))
                .exists()
        );
        let remaining = fs::read_dir(&snapshot_dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with("workspace.") && name.ends_with(".json"))
            })
            .count();
        assert_eq!(remaining, MAX_WORKSPACE_SNAPSHOTS);

        fs::remove_dir_all(workspace).unwrap();
    }

    fn snapshot_session(workspace: &Path, text: &str) -> PersistedSession {
        PersistedSession {
            workspace_root: workspace.to_path_buf(),
            explorer_width: EXPLORER_DEFAULT_WIDTH,
            symbols_panel_width: SYMBOLS_PANEL_DEFAULT_WIDTH,
            diagnostics_panel_width: DIAGNOSTICS_PANEL_DEFAULT_WIDTH,
            source_control_width: SOURCE_CONTROL_DEFAULT_WIDTH,
            terminal_height: TERMINAL_DEFAULT_HEIGHT,
            recovery: vec![RecoveredBuffer {
                path: None,
                display_name: "snapshot.rs".to_owned(),
                text: text.to_owned(),
            }],
            ..Default::default()
        }
    }

    fn temp_workspace(name: &str) -> PathBuf {
        let workspace = std::env::temp_dir().join(format!(
            "kuroya-workspace-snapshot-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&workspace).unwrap();
        workspace
    }
}
