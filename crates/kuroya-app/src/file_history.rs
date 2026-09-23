use crate::persistence_storage::{atomic_write_async, read_file_bytes_with_limit_async};
use std::{
    cmp::Ordering as CmpOrdering,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::SystemTime,
};

mod paths;

use paths::{
    LocalHistorySnapshotLookup, history_unique_id, local_history_snapshot_lookup,
    local_history_snapshot_path_in_dir,
};
#[cfg(test)]
use paths::{history_unique_id_from_parts, local_history_snapshot_path};

pub(crate) const LOCAL_HISTORY_MAX_BYTES: u64 = 1024 * 1024;
pub(crate) const LOCAL_HISTORY_MAX_SNAPSHOTS_PER_FILE: usize = 32;
const LOCAL_HISTORY_MAX_READ_CANDIDATES: usize = LOCAL_HISTORY_MAX_SNAPSHOTS_PER_FILE;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalHistorySnapshot {
    pub(crate) sequence: u128,
    pub(crate) path: PathBuf,
    pub(crate) bytes: u64,

    pub(crate) modified: Option<SystemTime>,
}

pub(crate) async fn snapshot_file_before_save_async(
    workspace_root: &Path,
    path: &Path,
    next_bytes: &[u8],
    max_bytes: u64,
) -> anyhow::Result<Option<PathBuf>> {
    let current_bytes = match read_file_bytes_with_limit_async(path, max_bytes).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) if error.kind() == ErrorKind::InvalidData => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if current_bytes == next_bytes {
        return Ok(None);
    }
    if std::str::from_utf8(&current_bytes).is_err() {
        return Ok(None);
    }

    let lookup = local_history_snapshot_lookup(workspace_root, path);
    if latest_readable_local_history_snapshot_matches_bytes_async(
        &lookup,
        &current_bytes,
        max_bytes,
    )
    .await?
    {
        return Ok(None);
    }

    let snapshot =
        local_history_snapshot_path_in_dir(&lookup.dir, history_unique_id(), &lookup.primary_name);
    tokio::fs::create_dir_all(&lookup.dir).await?;
    atomic_write_async(&snapshot, &current_bytes).await?;
    prune_local_history_snapshots_async(
        &lookup.dir,
        &lookup.primary_name,
        LOCAL_HISTORY_MAX_SNAPSHOTS_PER_FILE,
        &snapshot,
    )
    .await?;
    Ok(Some(snapshot))
}

pub(crate) async fn local_history_snapshots_for_file_async(
    workspace_root: &Path,
    path: &Path,
) -> anyhow::Result<Vec<LocalHistorySnapshot>> {
    let lookup = local_history_snapshot_lookup(workspace_root, path);
    let snapshots = local_history_snapshot_lookup_files_async(&lookup)
        .await?
        .into_iter()
        .rev()
        .map(LocalHistorySnapshot::from)
        .collect::<Vec<_>>();
    Ok(snapshots)
}

pub(crate) async fn latest_local_history_snapshot_text_async(
    workspace_root: &Path,
    path: &Path,
    max_bytes: u64,
) -> anyhow::Result<Option<(LocalHistorySnapshot, String)>> {
    if max_bytes == 0 {
        return Ok(None);
    }

    let lookup = local_history_snapshot_lookup(workspace_root, path);
    let candidates = local_history_snapshot_lookup_candidate_sets_async(&lookup).await?;
    if let Some(snapshot) = latest_readable_local_history_snapshot_text_from_candidates_async(
        candidates.primary,
        max_bytes,
    )
    .await?
    {
        return Ok(Some(snapshot));
    }

    latest_readable_local_history_snapshot_text_from_candidates_async(candidates.legacy, max_bytes)
        .await
}

async fn latest_readable_local_history_snapshot_text_from_candidates_async(
    mut candidates: Vec<LocalHistorySnapshotCandidate>,
    max_bytes: u64,
) -> anyhow::Result<Option<(LocalHistorySnapshot, String)>> {
    if max_bytes == 0 {
        return Ok(None);
    }

    let mut remaining = LOCAL_HISTORY_MAX_READ_CANDIDATES;
    while let Some(candidate) =
        next_newest_local_history_read_candidate(&mut candidates, &mut remaining)
    {
        if let Some((snapshot, text)) =
            readable_local_history_snapshot_text_async(candidate, max_bytes).await?
        {
            return Ok(Some((snapshot, text)));
        }
    }

    Ok(None)
}

async fn readable_local_history_snapshot_text_async(
    candidate: LocalHistorySnapshotCandidate,
    max_bytes: u64,
) -> anyhow::Result<Option<(LocalHistorySnapshot, String)>> {
    let bytes = match read_file_bytes_with_limit_async(&candidate.path, max_bytes).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) if error.kind() == ErrorKind::InvalidData => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let bytes_len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => return Ok(None),
    };
    Ok(Some((
        LocalHistorySnapshot {
            sequence: candidate.sequence,
            path: candidate.path,
            bytes: bytes_len,
            modified: None,
        },
        text,
    )))
}

async fn latest_readable_local_history_snapshot_matches_bytes_async(
    lookup: &LocalHistorySnapshotLookup,
    expected: &[u8],
    max_bytes: u64,
) -> anyhow::Result<bool> {
    if max_bytes == 0 {
        return Ok(false);
    }

    let candidates = local_history_snapshot_lookup_candidate_sets_async(lookup).await?;
    if let Some(matches) =
        latest_readable_local_history_snapshot_matches_bytes_from_candidates_async(
            candidates.primary,
            expected,
            max_bytes,
        )
        .await?
    {
        return Ok(matches);
    }

    Ok(
        latest_readable_local_history_snapshot_matches_bytes_from_candidates_async(
            candidates.legacy,
            expected,
            max_bytes,
        )
        .await?
        .unwrap_or(false),
    )
}

async fn latest_readable_local_history_snapshot_matches_bytes_from_candidates_async(
    mut candidates: Vec<LocalHistorySnapshotCandidate>,
    expected: &[u8],
    max_bytes: u64,
) -> anyhow::Result<Option<bool>> {
    if max_bytes == 0 {
        return Ok(None);
    }

    let mut remaining = LOCAL_HISTORY_MAX_READ_CANDIDATES;
    while let Some(candidate) =
        next_newest_local_history_read_candidate(&mut candidates, &mut remaining)
    {
        let bytes = match read_file_bytes_with_limit_async(&candidate.path, max_bytes).await {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) if error.kind() == ErrorKind::InvalidData => continue,
            Err(error) => return Err(error.into()),
        };
        return Ok(Some(bytes == expected));
    }
    Ok(None)
}

fn next_newest_local_history_read_candidate(
    candidates: &mut Vec<LocalHistorySnapshotCandidate>,
    remaining: &mut usize,
) -> Option<LocalHistorySnapshotCandidate> {
    if *remaining == 0 {
        return None;
    }
    let candidate = candidates.pop()?;
    *remaining -= 1;
    Some(candidate)
}

struct LocalHistorySnapshotEntry {
    sequence: u128,
    path: PathBuf,
    bytes: u64,
    modified: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LocalHistorySnapshotCandidate {
    sequence: u128,
    path: PathBuf,
}

#[derive(Debug, Default)]
struct LocalHistorySnapshotLookupCandidates {
    primary: Vec<LocalHistorySnapshotCandidate>,
    legacy: Vec<LocalHistorySnapshotCandidate>,
}

impl LocalHistorySnapshotLookupCandidates {
    fn into_preferred(self) -> Vec<LocalHistorySnapshotCandidate> {
        if self.primary.is_empty() {
            self.legacy
        } else {
            self.primary
        }
    }
}

impl From<LocalHistorySnapshotEntry> for LocalHistorySnapshot {
    fn from(entry: LocalHistorySnapshotEntry) -> Self {
        Self {
            sequence: entry.sequence,
            path: entry.path,
            bytes: entry.bytes,
            modified: entry.modified,
        }
    }
}

async fn prune_local_history_snapshots_async(
    dir: &Path,
    original_file_name: &str,
    max_snapshots: usize,
    protected_path: &Path,
) -> anyhow::Result<()> {
    let suffix = local_history_snapshot_suffix(original_file_name);
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let mut retained = Vec::with_capacity(max_snapshots);

    while let Some(entry) = entries.next_entry().await? {
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        let Some(sequence) = local_history_snapshot_sequence(file_name, &suffix) else {
            continue;
        };
        let Some(candidate) =
            local_history_snapshot_file_candidate_from_entry_async(&entry, sequence, dir).await?
        else {
            continue;
        };
        if let Some(overflow) = push_bounded_local_history_snapshot_candidate(
            &mut retained,
            candidate,
            max_snapshots,
            Some(protected_path),
        ) {
            remove_local_history_snapshot_file_if_present_async(&overflow.path).await?;
        }
    }

    Ok(())
}

#[cfg(test)]
async fn local_history_snapshot_files_async(
    dir: &Path,
    original_file_name: &str,
) -> anyhow::Result<Vec<LocalHistorySnapshotEntry>> {
    let candidates = local_history_snapshot_candidates_async(dir, original_file_name).await?;
    let mut snapshots = Vec::with_capacity(candidates.len());

    for candidate in candidates {
        if let Some(snapshot) = local_history_snapshot_entry_from_candidate_async(candidate).await?
        {
            snapshots.push(snapshot);
        }
    }

    Ok(snapshots)
}

async fn local_history_snapshot_lookup_files_async(
    lookup: &LocalHistorySnapshotLookup,
) -> anyhow::Result<Vec<LocalHistorySnapshotEntry>> {
    let candidates = local_history_snapshot_lookup_candidates_async(lookup).await?;
    let mut snapshots = Vec::with_capacity(candidates.len());

    for candidate in candidates {
        if let Some(snapshot) = local_history_snapshot_entry_from_candidate_async(candidate).await?
        {
            snapshots.push(snapshot);
        }
    }

    Ok(snapshots)
}

async fn local_history_snapshot_entry_from_candidate_async(
    candidate: LocalHistorySnapshotCandidate,
) -> anyhow::Result<Option<LocalHistorySnapshotEntry>> {
    let metadata = match tokio::fs::symlink_metadata(&candidate.path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() {
        return Ok(None);
    }

    Ok(Some(LocalHistorySnapshotEntry {
        sequence: candidate.sequence,
        path: candidate.path,
        bytes: metadata.len(),
        modified: metadata.modified().ok(),
    }))
}

async fn local_history_snapshot_lookup_candidates_async(
    lookup: &LocalHistorySnapshotLookup,
) -> anyhow::Result<Vec<LocalHistorySnapshotCandidate>> {
    Ok(local_history_snapshot_lookup_candidate_sets_async(lookup)
        .await?
        .into_preferred())
}

async fn local_history_snapshot_lookup_candidate_sets_async(
    lookup: &LocalHistorySnapshotLookup,
) -> anyhow::Result<LocalHistorySnapshotLookupCandidates> {
    let primary_suffix = local_history_snapshot_suffix(&lookup.primary_name);
    let legacy_suffix = lookup
        .legacy_name
        .as_deref()
        .map(local_history_snapshot_suffix);
    let mut candidates = LocalHistorySnapshotLookupCandidates {
        primary: Vec::with_capacity(LOCAL_HISTORY_MAX_READ_CANDIDATES),
        legacy: Vec::with_capacity(LOCAL_HISTORY_MAX_READ_CANDIDATES),
    };

    collect_local_history_snapshot_lookup_candidates_async(
        &lookup.dir,
        &primary_suffix,
        legacy_suffix.as_deref(),
        true,
        &mut candidates,
    )
    .await?;
    if let Some(legacy_dir) = &lookup.legacy_dir {
        collect_local_history_snapshot_lookup_candidates_async(
            legacy_dir,
            &primary_suffix,
            legacy_suffix.as_deref(),
            false,
            &mut candidates,
        )
        .await?;
    }

    Ok(candidates)
}

async fn collect_local_history_snapshot_lookup_candidates_async(
    dir: &Path,
    primary_suffix: &str,
    legacy_suffix: Option<&str>,
    prefer_primary: bool,
    candidates: &mut LocalHistorySnapshotLookupCandidates,
) -> anyhow::Result<()> {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };

    while let Some(entry) = entries.next_entry().await? {
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if let Some(sequence) = local_history_snapshot_sequence(file_name, primary_suffix) {
            push_local_history_snapshot_lookup_candidate_async(
                if prefer_primary {
                    &mut candidates.primary
                } else {
                    &mut candidates.legacy
                },
                &entry,
                sequence,
                dir,
            )
            .await?;
            continue;
        }
        if let Some(legacy_suffix) = legacy_suffix {
            if let Some(sequence) = local_history_snapshot_sequence(file_name, legacy_suffix) {
                push_local_history_snapshot_lookup_candidate_async(
                    &mut candidates.legacy,
                    &entry,
                    sequence,
                    dir,
                )
                .await?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
async fn local_history_snapshot_candidates_async(
    dir: &Path,
    original_file_name: &str,
) -> anyhow::Result<Vec<LocalHistorySnapshotCandidate>> {
    let suffix = local_history_snapshot_suffix(original_file_name);
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut candidates = Vec::new();

    while let Some(entry) = entries.next_entry().await? {
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        let Some(sequence) = local_history_snapshot_sequence(file_name, &suffix) else {
            continue;
        };
        if let Some(candidate) =
            local_history_snapshot_file_candidate_from_entry_async(&entry, sequence, dir).await?
        {
            candidates.push(candidate);
        }
    }

    sort_local_history_snapshot_candidates(&mut candidates);
    Ok(candidates)
}

async fn push_local_history_snapshot_lookup_candidate_async(
    candidates: &mut Vec<LocalHistorySnapshotCandidate>,
    entry: &tokio::fs::DirEntry,
    sequence: u128,
    bucket_dir: &Path,
) -> anyhow::Result<()> {
    if !local_history_snapshot_sequence_may_enter_bounded_candidates(
        candidates,
        sequence,
        LOCAL_HISTORY_MAX_READ_CANDIDATES,
    ) {
        return Ok(());
    }
    let Some(candidate) =
        local_history_snapshot_file_candidate_from_entry_async(entry, sequence, bucket_dir).await?
    else {
        return Ok(());
    };
    push_bounded_local_history_snapshot_candidate(
        candidates,
        candidate,
        LOCAL_HISTORY_MAX_READ_CANDIDATES,
        None,
    );
    Ok(())
}

async fn local_history_snapshot_file_candidate_from_entry_async(
    entry: &tokio::fs::DirEntry,
    sequence: u128,
    bucket_dir: &Path,
) -> anyhow::Result<Option<LocalHistorySnapshotCandidate>> {
    if !local_history_snapshot_entry_may_resolve_to_file_async(entry).await {
        return Ok(None);
    }
    let path = entry.path();
    if local_history_snapshot_file_len_async(&path)
        .await?
        .is_none()
    {
        return Ok(None);
    }
    if !local_history_snapshot_resolves_within_bucket(&path, bucket_dir) {
        return Ok(None);
    }
    Ok(Some(LocalHistorySnapshotCandidate { sequence, path }))
}

async fn local_history_snapshot_entry_may_resolve_to_file_async(
    entry: &tokio::fs::DirEntry,
) -> bool {
    matches!(entry.file_type().await, Ok(file_type) if file_type.is_file())
}

fn local_history_snapshot_resolves_within_bucket(path: &Path, bucket_dir: &Path) -> bool {
    match (fs::canonicalize(path), fs::canonicalize(bucket_dir)) {
        (Ok(resolved), Ok(bucket)) => resolved.starts_with(bucket),
        _ => false,
    }
}

async fn local_history_snapshot_file_len_async(path: &Path) -> anyhow::Result<Option<u64>> {
    let metadata = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() {
        return Ok(None);
    }
    Ok(Some(metadata.len()))
}

async fn remove_local_history_snapshot_file_if_present_async(path: &Path) -> anyhow::Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn local_history_snapshot_suffix(original_file_name: &str) -> String {
    format!(".{original_file_name}.bak")
}

#[cfg(test)]
fn sort_local_history_snapshot_candidates(candidates: &mut [LocalHistorySnapshotCandidate]) {
    candidates.sort_by(compare_local_history_snapshot_candidates);
}

fn push_bounded_local_history_snapshot_candidate(
    candidates: &mut Vec<LocalHistorySnapshotCandidate>,
    candidate: LocalHistorySnapshotCandidate,
    max_candidates: usize,
    protected_path: Option<&Path>,
) -> Option<LocalHistorySnapshotCandidate> {
    if max_candidates == 0 {
        return Some(candidate);
    }

    let insertion = candidates.binary_search_by(|existing| {
        compare_local_history_snapshot_candidates(existing, &candidate)
    });

    match insertion {
        Ok(index) => {
            candidates[index] = candidate;
            return None;
        }
        Err(0) if candidates.len() == max_candidates => {
            if Some(candidate.path.as_path()) == protected_path {
                return evict_instead_of_protected_local_history_snapshot_candidate(
                    candidates, candidate,
                );
            }
            return Some(candidate);
        }
        Err(_) => {}
    }

    let index = insertion.unwrap_or_else(|index| index);
    candidates.insert(index, candidate);
    if candidates.len() > max_candidates {
        let overflow = candidates.remove(0);
        if Some(overflow.path.as_path()) == protected_path {
            return evict_instead_of_protected_local_history_snapshot_candidate(
                candidates, overflow,
            );
        }
        return Some(overflow);
    }
    None
}

fn evict_instead_of_protected_local_history_snapshot_candidate(
    candidates: &mut Vec<LocalHistorySnapshotCandidate>,
    protected: LocalHistorySnapshotCandidate,
) -> Option<LocalHistorySnapshotCandidate> {
    if candidates.is_empty() {
        candidates.push(protected);
        return None;
    }
    let evicted = candidates.remove(0);
    candidates.insert(0, protected);
    Some(evicted)
}

fn local_history_snapshot_sequence_may_enter_bounded_candidates(
    candidates: &[LocalHistorySnapshotCandidate],
    sequence: u128,
    max_candidates: usize,
) -> bool {
    if max_candidates == 0 {
        return false;
    }
    candidates.len() < max_candidates
        || candidates
            .first()
            .is_some_and(|oldest| sequence >= oldest.sequence)
}

fn compare_local_history_snapshot_candidates(
    left: &LocalHistorySnapshotCandidate,
    right: &LocalHistorySnapshotCandidate,
) -> CmpOrdering {
    left.sequence
        .cmp(&right.sequence)
        .then_with(|| left.path.cmp(&right.path))
}

fn local_history_snapshot_sequence(file_name: &str, suffix: &str) -> Option<u128> {
    let prefix = file_name.strip_suffix(suffix)?;
    prefix.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{
        LOCAL_HISTORY_MAX_READ_CANDIDATES, LOCAL_HISTORY_MAX_SNAPSHOTS_PER_FILE,
        LocalHistorySnapshotCandidate, history_unique_id_from_parts,
        latest_local_history_snapshot_text_async,
        local_history_snapshot_entry_from_candidate_async,
        local_history_snapshot_file_candidate_from_entry_async, local_history_snapshot_files_async,
        local_history_snapshot_lookup, local_history_snapshot_lookup_candidates_async,
        local_history_snapshot_path, local_history_snapshots_for_file_async,
        prune_local_history_snapshots_async, push_bounded_local_history_snapshot_candidate,
        snapshot_file_before_save_async,
    };
    use crate::persistence_storage::{legacy_state_dir, state_dir};
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn platform_absolute_path(windows: &str, unix: &str) -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(windows)
        } else {
            PathBuf::from(unix)
        }
    }

    #[tokio::test]
    async fn local_history_snapshots_previous_file_content_before_save() {
        let workspace = temp_workspace("snapshot");
        fs::create_dir_all(workspace.join("src")).unwrap();
        let path = workspace.join("src/main.rs");
        fs::write(&path, "old text").unwrap();

        let snapshot = snapshot_file_before_save_async(&workspace, &path, b"new text", 1024)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(fs::read_to_string(&snapshot).unwrap(), "old text");
        assert!(snapshot.starts_with(state_dir(&workspace).join("history")));

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn local_history_skips_missing_unchanged_and_oversized_files() {
        let workspace = temp_workspace("skip");
        fs::create_dir_all(&workspace).unwrap();
        let missing = workspace.join("missing.rs");
        assert!(
            snapshot_file_before_save_async(&workspace, &missing, b"new", 1024)
                .await
                .unwrap()
                .is_none()
        );

        let unchanged = workspace.join("unchanged.rs");
        fs::write(&unchanged, "same").unwrap();
        assert!(
            snapshot_file_before_save_async(&workspace, &unchanged, b"same", 1024)
                .await
                .unwrap()
                .is_none()
        );

        let large = workspace.join("large.rs");
        fs::write(&large, "too large").unwrap();
        assert!(
            snapshot_file_before_save_async(&workspace, &large, b"new", 4)
                .await
                .unwrap()
                .is_none()
        );

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn local_history_skips_non_file_current_path() {
        let workspace = temp_workspace("skip-non-file");
        fs::create_dir_all(&workspace).unwrap();
        let path = workspace.join("directory.rs");
        fs::create_dir(&path).unwrap();

        assert!(
            snapshot_file_before_save_async(&workspace, &path, b"replacement", 1024)
                .await
                .unwrap()
                .is_none()
        );

        let snapshots = local_history_snapshots_for_file_async(&workspace, &path)
            .await
            .unwrap();
        assert!(snapshots.is_empty());

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn local_history_skips_corrupt_utf8_current_file_snapshots() {
        let workspace = temp_workspace("skip-corrupt");
        fs::create_dir_all(&workspace).unwrap();
        let path = workspace.join("main.rs");
        fs::write(&path, b"\xff\xfe\xfd").unwrap();

        assert!(
            snapshot_file_before_save_async(&workspace, &path, b"replacement", 1024)
                .await
                .unwrap()
                .is_none()
        );

        let snapshots = local_history_snapshots_for_file_async(&workspace, &path)
            .await
            .unwrap();
        assert!(snapshots.is_empty());

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn local_history_skips_duplicate_latest_snapshot_content() {
        let workspace = temp_workspace("skip-duplicate-latest");
        fs::create_dir_all(workspace.join("src")).unwrap();
        let path = workspace.join("src/main.rs");
        fs::write(&path, "old text").unwrap();
        let existing = local_history_snapshot_path(&workspace, &path, 1);
        fs::create_dir_all(existing.parent().unwrap()).unwrap();
        fs::write(&existing, "old text").unwrap();

        let snapshot = snapshot_file_before_save_async(&workspace, &path, b"new text", 1024)
            .await
            .unwrap();

        assert!(snapshot.is_none());
        let snapshots = local_history_snapshots_for_file_async(&workspace, &path)
            .await
            .unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].path, existing);
        assert_eq!(snapshots[0].sequence, 1);

        fs::remove_dir_all(workspace).unwrap();
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn local_history_skips_duplicate_legacy_snapshot_after_stale_primary_candidate() {
        let workspace = temp_workspace("skip-duplicate-stale-primary");
        fs::create_dir_all(workspace.join("src")).unwrap();
        let path = workspace.join("src").join("bad:name.rs");
        fs::write(&path, "old text").unwrap();
        let primary_snapshot = local_history_snapshot_path(&workspace, &path, 10);
        let legacy_snapshot = state_dir(&workspace)
            .join("history")
            .join("src")
            .join("9.bad_name.rs.bak");
        fs::create_dir_all(primary_snapshot.parent().unwrap()).unwrap();
        fs::create_dir(&primary_snapshot).unwrap();
        fs::write(&legacy_snapshot, "old text").unwrap();

        let snapshot = snapshot_file_before_save_async(&workspace, &path, b"new text", 1024)
            .await
            .unwrap();

        assert!(snapshot.is_none());
        assert_eq!(fs::read_to_string(&legacy_snapshot).unwrap(), "old text");

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn local_history_writes_when_latest_snapshot_content_differs() {
        let workspace = temp_workspace("dedupe-latest-differs");
        fs::create_dir_all(workspace.join("src")).unwrap();
        let path = workspace.join("src/main.rs");
        fs::write(&path, "old text").unwrap();
        let older = local_history_snapshot_path(&workspace, &path, 1);
        let latest = local_history_snapshot_path(&workspace, &path, 2);
        fs::create_dir_all(latest.parent().unwrap()).unwrap();
        fs::write(&older, "old text").unwrap();
        fs::write(&latest, "different text").unwrap();

        let snapshot = snapshot_file_before_save_async(&workspace, &path, b"new text", 1024)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(fs::read_to_string(&snapshot).unwrap(), "old text");
        let snapshots = local_history_snapshots_for_file_async(&workspace, &path)
            .await
            .unwrap();
        assert_eq!(snapshots.len(), 3);
        assert!(snapshots.iter().any(|entry| entry.path == snapshot));

        fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn local_history_paths_are_sanitized_inside_state_dir() {
        let workspace = platform_absolute_path("C:/repo", "/repo");
        let path = workspace.join("src").join("bad:name.rs");
        let snapshot = local_history_snapshot_path(&workspace, &path, 42);
        let snapshot_name = snapshot.file_name().unwrap().to_str().unwrap();

        assert_eq!(
            snapshot.parent().unwrap(),
            state_dir(&workspace).join("history").join("src")
        );
        assert!(snapshot_name.starts_with("42.bad_name.rs."));
        assert!(snapshot_name.ends_with(".bak"));
        let hash = snapshot_name
            .strip_prefix("42.bad_name.rs.")
            .unwrap()
            .strip_suffix(".bak")
            .unwrap();
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|ch| ch.is_ascii_hexdigit()));

        let plain = local_history_snapshot_path(&workspace, &workspace.join("src/main.rs"), 42);
        assert_eq!(
            plain,
            state_dir(&workspace)
                .join("history")
                .join("src")
                .join("42.main.rs.bak")
        );

        let external = local_history_snapshot_path(
            &workspace,
            &platform_absolute_path("D:/other/file.rs", "/other/file.rs"),
            7,
        );
        assert!(external.starts_with(state_dir(&workspace).join("history").join("external")));
        assert_eq!(external.file_name().unwrap(), "7.file.rs.bak");
    }

    #[test]
    fn local_history_sanitized_name_collisions_use_distinct_snapshot_names() {
        let workspace = PathBuf::from("C:/repo");
        let first =
            local_history_snapshot_path(&workspace, &workspace.join("src").join("bad:name.rs"), 42);
        let second =
            local_history_snapshot_path(&workspace, &workspace.join("src").join("bad?name.rs"), 42);

        assert_eq!(first.parent(), second.parent());
        assert_ne!(first.file_name(), second.file_name());
        assert!(
            first
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("42.bad_name.rs.")
        );
        assert!(
            second
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("42.bad_name.rs.")
        );
    }

    #[test]
    fn local_history_sanitized_parent_collisions_use_distinct_snapshot_names() {
        let workspace = PathBuf::from("C:/repo");
        let first = local_history_snapshot_path(
            &workspace,
            &workspace.join("src").join("bad:name").join("main.rs"),
            42,
        );
        let second = local_history_snapshot_path(
            &workspace,
            &workspace.join("src").join("bad?name").join("main.rs"),
            42,
        );

        assert_eq!(first.parent(), second.parent());
        assert_ne!(first.file_name(), second.file_name());
        assert_eq!(
            first.parent().unwrap(),
            state_dir(&workspace)
                .join("history")
                .join("src")
                .join("bad_name")
        );
        assert!(
            first
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("42.main.rs.")
        );
        assert!(
            second
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("42.main.rs.")
        );
    }

    #[test]
    fn local_history_parent_labels_are_safe_path_components() {
        let workspace = PathBuf::from("C:/repo");
        let snapshot = local_history_snapshot_path(
            &workspace,
            &workspace.join("CON").join("folder. ").join("main.rs"),
            42,
        );

        assert!(
            snapshot.starts_with(
                state_dir(&workspace)
                    .join("history")
                    .join("_CON")
                    .join("folder__")
            )
        );
        assert!(
            snapshot
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("42.main.rs.")
        );
    }

    #[tokio::test]
    async fn local_history_prefers_collision_safe_snapshots_over_legacy_sanitized_names() {
        let workspace = temp_workspace("sanitized-collision");
        let first = workspace.join("src").join("bad:name.rs");
        let second = workspace.join("src").join("bad?name.rs");
        let first_snapshot = local_history_snapshot_path(&workspace, &first, 2);
        let second_snapshot = local_history_snapshot_path(&workspace, &second, 3);
        fs::create_dir_all(first_snapshot.parent().unwrap()).unwrap();
        fs::write(
            state_dir(&workspace)
                .join("history")
                .join("src")
                .join("9.bad_name.rs.bak"),
            "legacy mixed",
        )
        .unwrap();
        fs::write(&first_snapshot, "first only").unwrap();
        fs::write(&second_snapshot, "second only").unwrap();

        let (snapshot, text) = latest_local_history_snapshot_text_async(&workspace, &first, 1024)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(snapshot.path, first_snapshot);
        assert_eq!(text, "first only");

        let (snapshot, text) = latest_local_history_snapshot_text_async(&workspace, &second, 1024)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(snapshot.path, second_snapshot);
        assert_eq!(text, "second only");

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn local_history_falls_back_to_legacy_sanitized_snapshot_names() {
        let workspace = temp_workspace("legacy-sanitized");
        let path = workspace.join("src").join("bad:name.rs");
        let legacy_snapshot = state_dir(&workspace)
            .join("history")
            .join("src")
            .join("9.bad_name.rs.bak");
        fs::create_dir_all(legacy_snapshot.parent().unwrap()).unwrap();
        fs::write(&legacy_snapshot, "legacy text").unwrap();

        let (snapshot, text) = latest_local_history_snapshot_text_async(&workspace, &path, 1024)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(snapshot.path, legacy_snapshot);
        assert_eq!(text, "legacy text");

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn local_history_reads_workspace_local_legacy_state_dir() {
        let workspace = temp_workspace("workspace-local-legacy");
        fs::create_dir_all(workspace.join("src")).unwrap();
        let path = workspace.join("src/main.rs");
        fs::write(&path, "legacy text").unwrap();
        let legacy_snapshot = legacy_state_dir(&workspace)
            .join("history")
            .join("src")
            .join("1.main.rs.bak");
        fs::create_dir_all(legacy_snapshot.parent().unwrap()).unwrap();
        fs::write(&legacy_snapshot, "legacy text").unwrap();

        let (snapshot, text) = latest_local_history_snapshot_text_async(&workspace, &path, 1024)
            .await
            .unwrap()
            .expect("legacy workspace-local snapshot should load");

        assert_eq!(snapshot.path, legacy_snapshot);
        assert_eq!(text, "legacy text");
        assert!(
            snapshot_file_before_save_async(&workspace, &path, b"new text", 1024)
                .await
                .unwrap()
                .is_none()
        );

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn local_history_falls_back_to_legacy_when_primary_candidates_are_stale() {
        let workspace = temp_workspace("legacy-after-stale-primary");
        let path = workspace.join("src").join("bad:name.rs");
        let primary_snapshot = local_history_snapshot_path(&workspace, &path, 10);
        let legacy_snapshot = state_dir(&workspace)
            .join("history")
            .join("src")
            .join("9.bad_name.rs.bak");
        fs::create_dir_all(primary_snapshot.parent().unwrap()).unwrap();
        fs::create_dir(&primary_snapshot).unwrap();
        fs::write(&legacy_snapshot, "legacy text").unwrap();

        let (snapshot, text) = latest_local_history_snapshot_text_async(&workspace, &path, 1024)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(snapshot.path, legacy_snapshot);
        assert_eq!(text, "legacy text");

        fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn local_history_paths_are_lexically_normalized_before_bucket_selection() {
        let workspace = platform_absolute_path("C:/repo/root/./project", "/repo/root/./project");
        let normalized_workspace =
            platform_absolute_path("C:/repo/root/project", "/repo/root/project");
        let direct = local_history_snapshot_path(
            &workspace,
            &platform_absolute_path("C:/repo/root/project/main.rs", "/repo/root/project/main.rs"),
            11,
        );
        let equivalent = local_history_snapshot_path(
            &workspace,
            &platform_absolute_path(
                "C:/repo/root/project/src/../main.rs",
                "/repo/root/project/src/../main.rs",
            ),
            11,
        );

        assert_eq!(equivalent, direct);
        assert_eq!(
            equivalent,
            state_dir(&normalized_workspace)
                .join("history")
                .join("11.main.rs.bak")
        );

        let escaped = local_history_snapshot_path(
            &workspace,
            &platform_absolute_path(
                "C:/repo/root/project/../outside/main.rs",
                "/repo/root/project/../outside/main.rs",
            ),
            12,
        );
        assert!(
            escaped.starts_with(
                state_dir(&normalized_workspace)
                    .join("history")
                    .join("external")
            )
        );
        assert_eq!(escaped.file_name().unwrap(), "12.main.rs.bak");
    }

    #[test]
    fn local_history_unique_ids_include_process_and_counter_parts() {
        let first = history_unique_id_from_parts(123, 42, 0);
        let second = history_unique_id_from_parts(123, 42, 1);
        let other_process = history_unique_id_from_parts(123, 43, 0);
        let later = history_unique_id_from_parts(124, 1, 0);

        assert!(first < second);
        assert!(second < other_process);
        assert!(other_process < later);
    }

    #[test]
    fn local_history_bounded_candidates_replace_duplicates_and_evict_oldest() {
        let mut candidates = Vec::new();
        let first = LocalHistorySnapshotCandidate {
            sequence: 1,
            path: PathBuf::from("1.main.rs.bak"),
        };
        let duplicate = first.clone();

        assert!(
            push_bounded_local_history_snapshot_candidate(&mut candidates, first, 2, None)
                .is_none()
        );
        assert!(
            push_bounded_local_history_snapshot_candidate(&mut candidates, duplicate, 2, None)
                .is_none()
        );
        assert_eq!(candidates.len(), 1);

        push_bounded_local_history_snapshot_candidate(
            &mut candidates,
            LocalHistorySnapshotCandidate {
                sequence: 2,
                path: PathBuf::from("2.main.rs.bak"),
            },
            2,
            None,
        );
        let evicted = push_bounded_local_history_snapshot_candidate(
            &mut candidates,
            LocalHistorySnapshotCandidate {
                sequence: 3,
                path: PathBuf::from("3.main.rs.bak"),
            },
            2,
            None,
        )
        .unwrap();

        assert_eq!(evicted.sequence, 1);
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.sequence)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );
    }

    #[tokio::test]
    async fn local_history_prune_keeps_just_written_snapshot_when_clock_rolls_back() {
        let workspace = temp_workspace("prune-protect-just-written");
        let dir = state_dir(&workspace).join("history").join("src");
        fs::create_dir_all(&dir).unwrap();
        for sequence in [1_u128, 2] {
            fs::write(
                dir.join(format!("{sequence}.main.rs.bak")),
                format!("snapshot {sequence}"),
            )
            .unwrap();
        }

        let just_written = dir.join("1.main.rs.bak");

        prune_local_history_snapshots_async(&dir, "main.rs", 1, &just_written)
            .await
            .unwrap();

        assert!(just_written.exists());
        assert!(!dir.join("2.main.rs.bak").exists());

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn local_history_prune_evicts_next_oldest_around_the_just_written_snapshot() {
        let workspace = temp_workspace("prune-protect-next-oldest");
        let dir = state_dir(&workspace).join("history").join("src");
        fs::create_dir_all(&dir).unwrap();
        for sequence in [1_u128, 2, 3] {
            fs::write(
                dir.join(format!("{sequence}.main.rs.bak")),
                format!("snapshot {sequence}"),
            )
            .unwrap();
        }

        let just_written = dir.join("1.main.rs.bak");

        prune_local_history_snapshots_async(&dir, "main.rs", 2, &just_written)
            .await
            .unwrap();

        assert!(just_written.exists());
        assert!(dir.join("3.main.rs.bak").exists());
        assert!(!dir.join("2.main.rs.bak").exists());

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn local_history_prunes_oldest_snapshots_for_same_file_only() {
        let workspace = temp_workspace("prune");
        let dir = state_dir(&workspace).join("history").join("src");
        fs::create_dir_all(&dir).unwrap();
        for sequence in [9_u128, 10, 11, 12] {
            fs::write(
                dir.join(format!("{sequence}.main.rs.bak")),
                format!("snapshot {sequence}"),
            )
            .unwrap();
        }
        fs::write(dir.join("1.other.rs.bak"), "other").unwrap();
        fs::write(dir.join("not-a-sequence.main.rs.bak"), "ignored").unwrap();

        prune_local_history_snapshots_async(&dir, "main.rs", 2, Path::new(""))
            .await
            .unwrap();

        assert!(!dir.join("9.main.rs.bak").exists());
        assert!(!dir.join("10.main.rs.bak").exists());
        assert!(dir.join("11.main.rs.bak").exists());
        assert!(dir.join("12.main.rs.bak").exists());
        assert!(dir.join("1.other.rs.bak").exists());
        assert!(dir.join("not-a-sequence.main.rs.bak").exists());

        let remaining = local_history_snapshot_files_async(&dir, "main.rs")
            .await
            .unwrap()
            .into_iter()
            .map(|entry| entry.sequence)
            .collect::<Vec<_>>();
        assert_eq!(remaining, vec![11, 12]);

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn local_history_prunes_pathological_overflow_without_removing_stale_directories() {
        let workspace = temp_workspace("prune-pathological-overflow");
        let dir = state_dir(&workspace).join("history").join("src");
        fs::create_dir_all(&dir).unwrap();
        let keep = 3_usize;
        let total = LOCAL_HISTORY_MAX_SNAPSHOTS_PER_FILE + 8;
        for sequence in 1..=total {
            fs::write(
                dir.join(format!("{sequence}.main.rs.bak")),
                format!("snapshot {sequence}"),
            )
            .unwrap();
        }
        let stale_dir_sequence = total + 100;
        let stale_dir = dir.join(format!("{stale_dir_sequence}.main.rs.bak"));
        fs::create_dir(&stale_dir).unwrap();

        prune_local_history_snapshots_async(&dir, "main.rs", keep, Path::new(""))
            .await
            .unwrap();

        for sequence in 1..=total - keep {
            assert!(!dir.join(format!("{sequence}.main.rs.bak")).exists());
        }
        for sequence in total - keep + 1..=total {
            assert!(dir.join(format!("{sequence}.main.rs.bak")).exists());
        }
        assert!(stale_dir.is_dir());

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn local_history_uses_same_bucket_for_lexically_equivalent_paths() {
        let workspace = temp_workspace("lexical-equivalent");
        fs::create_dir_all(workspace.join("src")).unwrap();
        let direct = workspace.join("main.rs");
        let equivalent = workspace.join("src").join("..").join("main.rs");
        fs::write(&direct, "old text").unwrap();

        let snapshot = snapshot_file_before_save_async(&workspace, &equivalent, b"new text", 1024)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            snapshot.parent().unwrap(),
            state_dir(&workspace).join("history")
        );

        let snapshots = local_history_snapshots_for_file_async(&workspace, &direct)
            .await
            .unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].path, snapshot);

        let (loaded, text) = latest_local_history_snapshot_text_async(&workspace, &direct, 1024)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.path, snapshot);
        assert_eq!(text, "old text");

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn local_history_lookup_candidates_are_bounded_to_newest_files() {
        let workspace = temp_workspace("bounded-lookup");
        let dir = state_dir(&workspace).join("history").join("src");
        fs::create_dir_all(&dir).unwrap();
        let overflow = 8_usize;
        let total = LOCAL_HISTORY_MAX_READ_CANDIDATES + overflow;
        for sequence in 1..=total {
            fs::write(
                dir.join(format!("{sequence}.main.rs.bak")),
                format!("snapshot {sequence}"),
            )
            .unwrap();
        }
        let stale_dir_sequence = total + 100;
        fs::create_dir(dir.join(format!("{stale_dir_sequence}.main.rs.bak"))).unwrap();
        let file = workspace.join("src/main.rs");
        let lookup = local_history_snapshot_lookup(&workspace, &file);

        let candidates = local_history_snapshot_lookup_candidates_async(&lookup)
            .await
            .unwrap();

        assert_eq!(candidates.len(), LOCAL_HISTORY_MAX_READ_CANDIDATES);
        let sequences = candidates
            .iter()
            .map(|candidate| candidate.sequence)
            .collect::<Vec<_>>();
        assert_eq!(
            sequences.first(),
            Some(&u128::try_from(overflow + 1).unwrap())
        );
        assert_eq!(sequences.last(), Some(&u128::try_from(total).unwrap()));
        assert!(!sequences.contains(&u128::try_from(stale_dir_sequence).unwrap()));

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn local_history_lists_newest_snapshots_for_file() {
        let workspace = temp_workspace("list");
        let dir = state_dir(&workspace).join("history").join("src");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("7.main.rs.bak"), "old").unwrap();
        fs::write(dir.join("9.main.rs.bak"), "newest").unwrap();
        fs::write(dir.join("8.other.rs.bak"), "other").unwrap();

        let snapshots =
            local_history_snapshots_for_file_async(&workspace, &workspace.join("src/main.rs"))
                .await
                .unwrap();

        assert_eq!(
            snapshots
                .iter()
                .map(|snapshot| snapshot.sequence)
                .collect::<Vec<_>>(),
            vec![9, 7]
        );
        assert_eq!(snapshots[0].bytes, 6);
        assert!(snapshots[0].path.ends_with(Path::new("9.main.rs.bak")));

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn local_history_enumerates_three_saved_snapshots_newest_first_with_timestamps() {
        let workspace = temp_workspace("browser-enumerate");
        fs::create_dir_all(workspace.join("src")).unwrap();
        let path = workspace.join("src/main.rs");
        for (index, saved_text) in ["one", "two", "three"].into_iter().enumerate() {
            fs::write(&path, saved_text).unwrap();
            snapshot_file_before_save_async(
                &workspace,
                &path,
                format!("next {index}").as_bytes(),
                1024,
            )
            .await
            .unwrap()
            .unwrap();
        }

        let snapshots = local_history_snapshots_for_file_async(&workspace, &path)
            .await
            .unwrap();

        assert_eq!(snapshots.len(), 3);
        let mut sequences = snapshots
            .iter()
            .map(|snapshot| snapshot.sequence)
            .collect::<Vec<_>>();
        sequences.sort();
        sequences.dedup();
        assert_eq!(sequences.len(), 3, "each save records its own snapshot");
        let newest_first = snapshots
            .iter()
            .map(|snapshot| snapshot.sequence)
            .collect::<Vec<_>>();
        let mut ascending = newest_first.clone();
        ascending.sort();
        ascending.reverse();
        assert_eq!(newest_first, ascending, "snapshots list newest first");
        let contents = {
            let mut contents = Vec::with_capacity(snapshots.len());
            for snapshot in &snapshots {
                contents.push(fs::read_to_string(&snapshot.path).unwrap());
            }
            contents
        };
        assert_eq!(contents, vec!["three", "two", "one"]);
        assert!(
            snapshots.iter().all(|snapshot| snapshot.modified.is_some()),
            "enumeration derives the modified timestamp from each snapshot"
        );

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn local_history_candidates_reject_symlink_to_file_snapshots() {
        let workspace = temp_workspace("symlink-candidate");
        let dir = state_dir(&workspace).join("history").join("src");
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("target-snapshot");
        let link = dir.join("7.main.rs.bak");
        fs::write(&target, "linked").unwrap();
        fs::write(dir.join("8.main.rs.bak"), "regular").unwrap();
        if !create_file_symlink_for_test(&target, &link) {
            fs::remove_dir_all(workspace).unwrap();
            return;
        }

        let snapshots =
            local_history_snapshots_for_file_async(&workspace, &workspace.join("src/main.rs"))
                .await
                .unwrap();

        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].sequence, 8);
        assert_eq!(snapshots[0].path, dir.join("8.main.rs.bak"));
        assert_eq!(snapshots[0].bytes, 7);

        let (snapshot, text) = latest_local_history_snapshot_text_async(
            &workspace,
            &workspace.join("src/main.rs"),
            64,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(snapshot.sequence, 8);
        assert_eq!(text, "regular");

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn local_history_candidate_rejected_when_resolving_outside_bucket_root() {
        let workspace = temp_workspace("candidate-outside-bucket");
        let outside = workspace.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("7.main.rs.bak"), "escaped").unwrap();
        let bucket = workspace.join("bucket");
        fs::create_dir_all(&bucket).unwrap();

        let mut entries = tokio::fs::read_dir(&outside).await.unwrap();
        let entry = entries.next_entry().await.unwrap().unwrap();

        let candidate = local_history_snapshot_file_candidate_from_entry_async(&entry, 7, &bucket)
            .await
            .unwrap();
        assert!(candidate.is_none());

        let candidate = local_history_snapshot_file_candidate_from_entry_async(
            &entry,
            7,
            &workspace.join("outside"),
        )
        .await
        .unwrap()
        .expect("candidates resolving inside their bucket stay eligible");
        assert_eq!(candidate.sequence, 7);
        assert_eq!(candidate.path, outside.join("7.main.rs.bak"));

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn latest_local_history_snapshot_reads_newest_text_with_size_guard() {
        let workspace = temp_workspace("latest");
        let dir = state_dir(&workspace).join("history").join("src");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("1.main.rs.bak"), "old").unwrap();
        fs::write(dir.join("2.main.rs.bak"), "latest").unwrap();
        let file = workspace.join("src/main.rs");

        let (snapshot, text) = latest_local_history_snapshot_text_async(&workspace, &file, 16)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(snapshot.sequence, 2);
        assert_eq!(snapshot.bytes, 6);
        assert_eq!(text, "latest");

        let (snapshot, text) = latest_local_history_snapshot_text_async(&workspace, &file, 4)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(snapshot.sequence, 1);
        assert_eq!(snapshot.bytes, 3);
        assert_eq!(text, "old");

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn latest_local_history_snapshot_ignores_stale_directories_before_read_window() {
        let workspace = temp_workspace("latest-stale-directory-window");
        let dir = state_dir(&workspace).join("history").join("src");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("1.main.rs.bak"), "valid").unwrap();
        for sequence in 2..=u128::try_from(LOCAL_HISTORY_MAX_READ_CANDIDATES).unwrap() + 1 {
            fs::create_dir(dir.join(format!("{sequence}.main.rs.bak"))).unwrap();
        }
        let file = workspace.join("src/main.rs");

        let (snapshot, text) = latest_local_history_snapshot_text_async(&workspace, &file, 1024)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(snapshot.sequence, 1);
        assert_eq!(text, "valid");

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn latest_local_history_snapshot_skips_reads_when_limit_is_zero() {
        let workspace = temp_workspace("latest-zero-limit");
        let dir = state_dir(&workspace).join("history").join("src");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("1.main.rs.bak"), "old").unwrap();
        let file = workspace.join("src/main.rs");

        assert!(
            latest_local_history_snapshot_text_async(&workspace, &file, 0)
                .await
                .unwrap()
                .is_none()
        );

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn latest_local_history_snapshot_skips_unreadable_newer_entries() {
        let workspace = temp_workspace("latest-valid");
        let dir = state_dir(&workspace).join("history").join("src");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("1.main.rs.bak"), "valid").unwrap();
        fs::write(dir.join("2.main.rs.bak"), b"\xff\xfe\xfd").unwrap();
        fs::write(dir.join("3.main.rs.bak"), "too large").unwrap();
        let file = workspace.join("src/main.rs");

        let (snapshot, text) = latest_local_history_snapshot_text_async(&workspace, &file, 8)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(snapshot.sequence, 1);
        assert_eq!(text, "valid");

        assert!(
            latest_local_history_snapshot_text_async(&workspace, &file, 4)
                .await
                .unwrap()
                .is_none()
        );

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn latest_local_history_snapshot_bounds_read_attempts_to_retained_window() {
        let workspace = temp_workspace("latest-read-window");
        let dir = state_dir(&workspace).join("history").join("src");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("1.main.rs.bak"), "old").unwrap();
        for sequence in 2..=u128::try_from(LOCAL_HISTORY_MAX_SNAPSHOTS_PER_FILE).unwrap() + 1 {
            fs::write(dir.join(format!("{sequence}.main.rs.bak")), "too large").unwrap();
        }
        let file = workspace.join("src/main.rs");

        assert!(
            latest_local_history_snapshot_text_async(&workspace, &file, 4)
                .await
                .unwrap()
                .is_none()
        );

        fs::remove_dir_all(workspace).unwrap();
    }

    #[tokio::test]
    async fn local_history_snapshot_listing_skips_stale_candidate_entries() {
        let workspace = temp_workspace("stale-candidate");
        let candidate = LocalHistorySnapshotCandidate {
            sequence: 1,
            path: workspace.join("missing.main.rs.bak"),
        };

        let entry = local_history_snapshot_entry_from_candidate_async(candidate)
            .await
            .unwrap();

        assert!(entry.is_none());
    }

    #[cfg(unix)]
    fn create_file_symlink_for_test(target: &Path, link: &Path) -> bool {
        std::os::unix::fs::symlink(target, link).is_ok()
    }

    #[cfg(windows)]
    fn create_file_symlink_for_test(target: &Path, link: &Path) -> bool {
        std::os::windows::fs::symlink_file(target, link).is_ok()
    }

    #[cfg(not(any(unix, windows)))]
    fn create_file_symlink_for_test(_target: &Path, _link: &Path) -> bool {
        false
    }

    fn temp_workspace(name: &str) -> PathBuf {
        let workspace = std::env::temp_dir().join(format!(
            "kuroya-local-history-{name}-{}-{}",
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
