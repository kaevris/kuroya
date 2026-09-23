use crate::persistence_storage::{
    atomic_write, legacy_project_index_cache_path, project_index_cache_path,
    read_file_bytes_with_limit, state_dir,
};
use kuroya_core::{ProjectIndex, ProjectIndexOptions, ProjectIndexSignature};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    io::{self, ErrorKind, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const PROJECT_INDEX_CACHE_SCHEMA: u32 = 4;
const PROJECT_INDEX_CACHE_MAX_BYTES: u64 = 32 * 1024 * 1024;
const PROJECT_INDEX_CACHE_MAX_BYTES_USIZE: usize = PROJECT_INDEX_CACHE_MAX_BYTES as usize;
const PROJECT_INDEX_CACHE_MAX_SYMBOLS: usize = 20_000;
const PROJECT_INDEX_CACHE_MAX_TRACKED_PATHS: usize = 250_000;
const PROJECT_INDEX_CACHE_HASH_OFFSET: u64 = 0xcbf29ce484222325;
const PROJECT_INDEX_CACHE_HASH_PRIME: u64 = 0x100000001b3;

#[derive(Serialize)]
struct ProjectIndexCacheRef<'a> {
    schema: u32,
    root: &'a Path,
    signature: ProjectIndexSignature,
    payload_hash: u64,
    index: &'a ProjectIndex,
}

#[derive(Deserialize)]
struct ProjectIndexCache {
    schema: u32,
    root: PathBuf,
    signature: ProjectIndexSignature,
    payload_hash: u64,
    index: ProjectIndex,
}

pub(crate) struct LoadedProjectIndexCache {
    pub(crate) index: ProjectIndex,
    pub(crate) signature: ProjectIndexSignature,
}

#[cfg(test)]
pub(crate) fn load_project_index_cache(
    workspace_root: &Path,
    max_files: usize,
) -> Option<ProjectIndex> {
    load_project_index_cache_validated(workspace_root, &ProjectIndexOptions::new(max_files))
        .map(|cache| cache.index)
}

#[cfg(test)]
pub(crate) fn load_project_index_cache_unverified(
    workspace_root: &Path,
    max_files: usize,
) -> Option<LoadedProjectIndexCache> {
    load_project_index_cache_unverified_with_options(
        workspace_root,
        &ProjectIndexOptions::new(max_files),
    )
}

pub(crate) fn load_project_index_cache_unverified_with_options(
    workspace_root: &Path,
    options: &ProjectIndexOptions,
) -> Option<LoadedProjectIndexCache> {
    load_project_index_cache_validated(workspace_root, options)
}

fn load_project_index_cache_validated(
    workspace_root: &Path,
    options: &ProjectIndexOptions,
) -> Option<LoadedProjectIndexCache> {
    let path = project_index_cache_path(workspace_root);
    load_project_index_cache_file(&path, workspace_root, options).or_else(|| {
        let legacy_path = legacy_project_index_cache_path(workspace_root);
        load_project_index_cache_file(&legacy_path, workspace_root, options)
    })
}

fn load_project_index_cache_file(
    path: &Path,
    workspace_root: &Path,
    options: &ProjectIndexOptions,
) -> Option<LoadedProjectIndexCache> {
    let bytes = match read_file_bytes_with_limit(path, PROJECT_INDEX_CACHE_MAX_BYTES) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return None,
        Err(error) if error.kind() == ErrorKind::InvalidData => {
            return quarantine_invalid_project_index_cache(path);
        }
        Err(_) => return None,
    };
    let cache = match serde_json::from_slice::<ProjectIndexCache>(&bytes) {
        Ok(cache) => cache,
        Err(_) => {
            return quarantine_invalid_project_index_cache(path);
        }
    };
    if cache.schema != PROJECT_INDEX_CACHE_SCHEMA {
        return quarantine_invalid_project_index_cache(path);
    }
    if !project_index_cache_roots_are_exact(&cache.root, workspace_root)
        || !project_index_cache_roots_are_exact(cache.index.root(), workspace_root)
    {
        if project_index_cache_root_is_rebuild_only_mismatch(&cache.root, workspace_root)
            && project_index_cache_root_is_rebuild_only_mismatch(cache.index.root(), workspace_root)
        {
            return None;
        }
        return quarantine_invalid_project_index_cache(path);
    }
    if !cache.signature.matches_options(options) {
        return None;
    }
    if !project_index_cache_index_matches_signature(&cache.index, cache.signature) {
        return quarantine_invalid_project_index_cache(path);
    }
    if !project_index_cache_paths_are_inside_root(&cache.index, workspace_root) {
        return quarantine_invalid_project_index_cache(path);
    }
    let payload_hash = match project_index_cache_payload_hash(&cache.index) {
        Ok(payload_hash) => payload_hash,
        Err(_) => {
            return quarantine_invalid_project_index_cache(path);
        }
    };
    if cache.payload_hash != payload_hash {
        return quarantine_invalid_project_index_cache(path);
    }
    Some(LoadedProjectIndexCache {
        index: cache.index,
        signature: cache.signature,
    })
}

fn quarantine_invalid_project_index_cache<T>(path: &Path) -> Option<T> {
    let _ = quarantine_project_index_cache(path);
    None
}

pub(crate) fn save_project_index_cache(
    workspace_root: &Path,
    index: &ProjectIndex,
    signature: ProjectIndexSignature,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        project_index_cache_roots_are_exact(index.root(), workspace_root),
        "project index cache root does not match workspace root"
    );
    anyhow::ensure!(
        project_index_cache_index_is_bounded(index),
        "project index cache is too large to persist"
    );
    anyhow::ensure!(
        project_index_cache_paths_are_inside_root(index, workspace_root),
        "project index cache contains paths outside workspace root"
    );
    anyhow::ensure!(
        project_index_cache_index_matches_signature(index, signature),
        "project index cache signature does not match index shape"
    );
    std::fs::create_dir_all(state_dir(workspace_root))?;
    let payload_hash = project_index_cache_payload_hash(index)?;
    let cache = ProjectIndexCacheRef {
        schema: PROJECT_INDEX_CACHE_SCHEMA,
        root: workspace_root,
        signature,
        payload_hash,
        index,
    };
    let bytes = project_index_cache_bytes_for_write(&cache, PROJECT_INDEX_CACHE_MAX_BYTES_USIZE)?;
    atomic_write(&project_index_cache_path(workspace_root), &bytes)?;
    Ok(())
}

fn project_index_cache_bytes_for_write(
    cache: &ProjectIndexCacheRef<'_>,
    max_bytes: usize,
) -> anyhow::Result<Vec<u8>> {
    let bytes = serde_json::to_vec(cache)?;
    anyhow::ensure!(
        bytes.len() <= max_bytes,
        "project index cache is too large to persist: {} bytes exceeds {} bytes",
        bytes.len(),
        max_bytes
    );
    Ok(bytes)
}

fn project_index_cache_payload_hash(index: &ProjectIndex) -> anyhow::Result<u64> {
    let mut writer = ProjectIndexCacheHashWriter::new();
    serde_json::to_writer(&mut writer, index)?;
    Ok(writer.finish())
}

struct ProjectIndexCacheHashWriter {
    hash: u64,
}

impl ProjectIndexCacheHashWriter {
    fn new() -> Self {
        Self {
            hash: PROJECT_INDEX_CACHE_HASH_OFFSET,
        }
    }

    fn finish(self) -> u64 {
        self.hash
    }
}

impl Write for ProjectIndexCacheHashWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        project_index_cache_hash_update(&mut self.hash, bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
fn project_index_cache_hash_bytes(bytes: &[u8]) -> u64 {
    let mut hash = PROJECT_INDEX_CACHE_HASH_OFFSET;
    project_index_cache_hash_update(&mut hash, bytes);
    hash
}

fn project_index_cache_hash_update(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(PROJECT_INDEX_CACHE_HASH_PRIME);
    }
}

fn quarantine_project_index_cache(path: &Path) -> std::io::Result<PathBuf> {
    let quarantine = corrupt_project_index_cache_path(path);
    quarantine_project_index_cache_to(path, &quarantine)?;
    Ok(quarantine)
}

fn quarantine_project_index_cache_to(path: &Path, quarantine: &Path) -> std::io::Result<()> {
    match std::fs::rename(path, quarantine) {
        Ok(()) => Ok(()),
        Err(rename_error) => {
            let _ = std::fs::remove_file(path);
            Err(rename_error)
        }
    }
}

fn corrupt_project_index_cache_path(path: &Path) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project-index.json");
    path.with_file_name(format!(
        "{file_name}.corrupt.{}.{}",
        std::process::id(),
        unique
    ))
}

fn project_index_cache_index_matches_signature(
    index: &ProjectIndex,
    signature: ProjectIndexSignature,
) -> bool {
    signature.file_count == index.files().len()
        && signature.entry_count == index.all_entries().len()
        && signature.truncated == index.truncated()
        && signature.max_files == index.max_files()
        && index.files().len() <= signature.max_files
        && project_index_cache_index_is_bounded(index)
}

fn project_index_cache_index_is_bounded(index: &ProjectIndex) -> bool {
    index.symbols().len() <= PROJECT_INDEX_CACHE_MAX_SYMBOLS
        && project_index_cache_tracked_path_count(index) <= PROJECT_INDEX_CACHE_MAX_TRACKED_PATHS
}

fn project_index_cache_tracked_path_count(index: &ProjectIndex) -> usize {
    index
        .files()
        .len()
        .saturating_add(index.all_entries().len())
        .saturating_add(index.symbols().len())
}

fn project_index_cache_paths_are_inside_root(index: &ProjectIndex, root: &Path) -> bool {
    let mut path_validator = ProjectIndexCachePathValidator::new(
        root,
        project_index_cache_tracked_path_count(index),
        index.files().len(),
        index.all_entries().len(),
    );
    for path in index.files() {
        if !path_validator.file_path_is_inside_root(path) {
            return false;
        }
    }
    for symbol in index.symbols() {
        if !path_validator.symbol_matches_relative_path(&symbol.path, &symbol.relative_path) {
            return false;
        }
    }
    for entry in index.all_entries() {
        if !path_validator.entry_matches_relative_path(
            &entry.path,
            &entry.relative_path,
            entry.is_dir,
            entry.depth,
        ) {
            return false;
        }
    }
    path_validator.file_paths_match_entries()
}

struct ProjectIndexCachePathValidator<'a> {
    root: &'a Path,
    file_paths: HashSet<PathBuf>,
    file_entry_paths: HashSet<PathBuf>,
    entry_paths: HashSet<PathBuf>,
    accepted_paths: HashSet<PathBuf>,
}

impl<'a> ProjectIndexCachePathValidator<'a> {
    fn new(
        root: &'a Path,
        expected_paths: usize,
        expected_files: usize,
        expected_entries: usize,
    ) -> Self {
        Self {
            root,
            file_paths: HashSet::with_capacity(expected_files),
            file_entry_paths: HashSet::with_capacity(expected_files),
            entry_paths: HashSet::with_capacity(expected_entries),
            accepted_paths: HashSet::with_capacity(
                expected_paths.min(PROJECT_INDEX_CACHE_MAX_TRACKED_PATHS),
            ),
        }
    }

    fn file_path_is_inside_root(&mut self, path: &Path) -> bool {
        let Some(relative_path) = self.path_relative_to_root(path) else {
            return false;
        };
        self.file_paths.insert(relative_path)
    }

    fn symbol_matches_relative_path(&mut self, path: &Path, relative_path: &Path) -> bool {
        let Some(relative_path) = self.matching_relative_path(path, relative_path) else {
            return false;
        };
        self.file_paths.contains(&relative_path)
    }

    fn entry_matches_relative_path(
        &mut self,
        path: &Path,
        relative_path: &Path,
        is_dir: bool,
        depth: usize,
    ) -> bool {
        let Some(relative_path) = self.matching_relative_path(path, relative_path) else {
            return false;
        };
        if project_index_cache_relative_path_depth(&relative_path) != Some(depth) {
            return false;
        }
        if !self.entry_paths.insert(relative_path.clone()) {
            return false;
        }
        if !is_dir {
            self.file_entry_paths.insert(relative_path);
        }
        true
    }

    fn file_paths_match_entries(&self) -> bool {
        self.file_paths == self.file_entry_paths
    }

    fn matching_relative_path(&mut self, path: &Path, relative_path: &Path) -> Option<PathBuf> {
        let path_relative = self.path_relative_to_root(path)?;
        let expected_relative = project_index_cache_normalized_relative_path(relative_path)?;
        (path_relative == expected_relative).then_some(path_relative)
    }

    fn path_relative_to_root(&mut self, path: &Path) -> Option<PathBuf> {
        let relative_path = project_index_cache_relative_path_for_root(path, self.root)?;
        if self.accepted_paths.len() >= PROJECT_INDEX_CACHE_MAX_TRACKED_PATHS
            && !self.accepted_paths.contains(&relative_path)
        {
            return None;
        }
        self.accepted_paths.insert(relative_path.clone());
        Some(relative_path)
    }
}

#[cfg(test)]
fn project_index_cache_path_is_inside_root(path: &Path, root: &Path) -> bool {
    project_index_cache_relative_path_for_root(path, root).is_some()
}

fn project_index_cache_relative_path_for_root(path: &Path, root: &Path) -> Option<PathBuf> {
    if let Ok(relative) = path.strip_prefix(root) {
        return project_index_cache_normalized_relative_path(relative);
    }
    #[cfg(windows)]
    if let Some(relative) = project_index_cache_case_insensitive_relative_path(path, root) {
        return project_index_cache_normalized_relative_path(&relative);
    }
    if project_index_cache_root_is_current_dir(root) && path.is_relative() {
        return project_index_cache_normalized_relative_path(path);
    }
    None
}

fn project_index_cache_root_is_current_dir(root: &Path) -> bool {
    root.as_os_str().is_empty()
        || root
            .components()
            .all(|component| matches!(component, Component::CurDir))
}

fn project_index_cache_roots_are_exact(left: &Path, right: &Path) -> bool {
    left.as_os_str() == right.as_os_str()
}

#[cfg(windows)]
fn project_index_cache_case_insensitive_relative_path(path: &Path, root: &Path) -> Option<PathBuf> {
    if !crate::workspace_trust::workspace_path_contains_lexically(root, path) {
        return None;
    }
    let mut relative = PathBuf::new();
    for component in path.components().skip(root.components().count()) {
        relative.push(component.as_os_str());
    }
    Some(relative)
}

fn project_index_cache_root_is_rebuild_only_mismatch(root: &Path, workspace_root: &Path) -> bool {
    project_index_cache_roots_are_exact(root, workspace_root)
        || crate::workspace_trust::trusted_workspace_paths_match(root, workspace_root)
        || (project_index_cache_root_is_current_dir(root)
            && project_index_cache_root_is_current_dir(workspace_root))
}

fn project_index_cache_normalized_relative_path(path: &Path) -> Option<PathBuf> {
    let mut normalized = PathBuf::new();
    let mut has_normal_component = false;
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(component) => {
                normalized.push(component);
                has_normal_component = true;
            }
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => return None,
        }
    }
    has_normal_component.then_some(normalized)
}

fn project_index_cache_relative_path_depth(path: &Path) -> Option<usize> {
    project_index_cache_normalized_relative_path(path)
        .map(|path| path.components().count().saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::{
        PROJECT_INDEX_CACHE_MAX_BYTES, PROJECT_INDEX_CACHE_SCHEMA, ProjectIndexCacheRef,
        load_project_index_cache, load_project_index_cache_unverified,
        load_project_index_cache_unverified_with_options, project_index_cache_bytes_for_write,
        project_index_cache_hash_bytes, project_index_cache_path_is_inside_root,
        project_index_cache_payload_hash, quarantine_project_index_cache_to,
        save_project_index_cache,
    };
    use crate::persistence_storage::{project_index_cache_path, state_dir};
    use kuroya_core::{ProjectIndex, ProjectIndexOptions, ProjectIndexSignature};
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn project_index_cache_round_trips_index() {
        let root = temp_workspace("kuroya-project-index-cache-roundtrip");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);

        save_project_index_cache(&root, &index, signature).unwrap();
        let loaded = load_project_index_cache(&root, 40_000).unwrap();

        assert_eq!(loaded.root(), root.as_path());
        assert_eq!(loaded.files().len(), 1);
        assert_eq!(loaded.symbols()[0].name, "indexed");
        assert!(!loaded.truncated());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_unverified_load_returns_stale_cache() {
        let root = temp_workspace("kuroya-project-index-cache-stale-preview");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);
        save_project_index_cache(&root, &index, signature).unwrap();

        fs::write(root.join("src/new.rs"), "fn newer() {}\n").unwrap();

        let loaded = load_project_index_cache_unverified(&root, 40_000).unwrap();
        assert_eq!(loaded.index.files().len(), 1);

        assert!(load_project_index_cache(&root, 40_000).is_some());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_unverified_load_rejects_different_index_options() {
        let root = temp_workspace("kuroya-project-index-cache-options");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        fs::write(root.join("target/generated.rs"), "fn generated() {}\n").unwrap();
        let saved_options = ProjectIndexOptions::new(40_000);
        let load_options = ProjectIndexOptions::with_exclude_globs(40_000, Vec::new());
        let (index, signature) =
            ProjectIndex::rebuild_with_signature_options(&root, &saved_options);

        save_project_index_cache(&root, &index, signature).unwrap();

        assert!(load_project_index_cache_unverified_with_options(&root, &load_options).is_none());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_payload_hash_matches_serialized_index_bytes() {
        let root = temp_workspace("kuroya-project-index-cache-hash-stream");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let index = ProjectIndex::rebuild(&root, 40_000);
        let bytes = serde_json::to_vec(&index).unwrap();

        assert_eq!(
            project_index_cache_payload_hash(&index).unwrap(),
            project_index_cache_hash_bytes(&bytes)
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_treats_different_file_limit_as_miss_without_quarantine() {
        let root = temp_workspace("kuroya-project-index-cache-max-files");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);

        save_project_index_cache(&root, &index, signature).unwrap();
        let path = project_index_cache_path(&root);

        assert!(load_project_index_cache(&root, 1).is_none());
        assert!(path.exists());
        assert!(quarantined_project_index_cache_files(&root).is_empty());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_treats_normalized_workspace_root_as_miss_without_quarantine() {
        let root = temp_workspace("kuroya-project-index-cache-normalized-root");
        let stored_root = root.join(".");
        let stored_src = stored_root.join("src");
        let stored_file = stored_src.join("main.rs");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let index: ProjectIndex = serde_json::from_value(serde_json::json!({
            "root": stored_root,
            "files": [stored_file],
            "entries": [
                {
                    "path": stored_src,
                    "relative_path": "src",
                    "is_dir": true,
                    "depth": 0,
                },
                {
                    "path": stored_file,
                    "relative_path": "src/main.rs",
                    "is_dir": false,
                    "depth": 1,
                }
            ],
            "symbols": [],
            "truncated": false,
            "max_files": 40_000,
        }))
        .unwrap();
        let signature = ProjectIndexSignature {
            max_files: 40_000,
            options_fingerprint: ProjectIndexOptions::new(40_000).options_fingerprint(),
            file_count: 1,
            entry_count: 2,
            truncated: false,
            symbol_policy_version: kuroya_core::PROJECT_INDEX_SYMBOL_POLICY_VERSION,
            fingerprint: 0,
        };

        save_project_index_cache(&stored_root, &index, signature).unwrap();
        let path = project_index_cache_path(&root);

        assert!(load_project_index_cache(&root, 40_000).is_none());
        assert!(path.exists());
        assert!(quarantined_project_index_cache_files(&root).is_empty());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_quarantines_signature_file_count_mismatch() {
        let root = temp_workspace("kuroya-project-index-cache-signature-file-count");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);

        save_project_index_cache(&root, &index, signature).unwrap();
        let path = project_index_cache_path(&root);
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["signature"]["file_count"] = serde_json::Value::from(999_999u64);
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();

        assert!(load_project_index_cache(&root, 40_000).is_none());
        assert!(!path.exists());
        assert_eq!(quarantined_project_index_cache_files(&root).len(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_rejects_wrong_workspace_root() {
        let root = temp_workspace("kuroya-project-index-cache-wrong-root");
        let other = temp_workspace("kuroya-project-index-cache-other-root");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(&other).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);

        save_project_index_cache(&root, &index, signature).unwrap();

        assert!(load_project_index_cache(&other, 40_000).is_none());

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(other).unwrap();
    }

    #[test]
    fn project_index_cache_quarantines_wrong_embedded_workspace_root() {
        let root = temp_workspace("kuroya-project-index-cache-wrong-embedded-root");
        let other = temp_workspace("kuroya-project-index-cache-wrong-embedded-other");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(&other).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);

        save_project_index_cache(&root, &index, signature).unwrap();
        let path = project_index_cache_path(&root);
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["index"]["root"] = serde_json::Value::String(other.display().to_string());
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();

        assert!(load_project_index_cache(&root, 40_000).is_none());
        assert!(!path.exists());
        assert_eq!(quarantined_project_index_cache_files(&root).len(), 1);

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(other).unwrap();
    }

    #[test]
    fn project_index_cache_rejects_index_files_outside_workspace_root() {
        let root = temp_workspace("kuroya-project-index-cache-outside-file");
        let outside = temp_workspace("kuroya-project-index-cache-outside-target");
        let outside_file = outside.join("leak.rs");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        fs::write(&outside_file, "fn leaked() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);

        save_project_index_cache(&root, &index, signature).unwrap();
        let path = project_index_cache_path(&root);
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["index"]["files"] = serde_json::Value::Array(vec![serde_json::Value::String(
            outside_file.display().to_string(),
        )]);
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();

        assert!(load_project_index_cache(&root, 40_000).is_none());
        assert!(!path.exists());
        assert_eq!(quarantined_project_index_cache_files(&root).len(), 1);

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn project_index_cache_rejects_workspace_root_as_index_file() {
        let root = temp_workspace("kuroya-project-index-cache-root-file");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);

        save_project_index_cache(&root, &index, signature).unwrap();
        let path = project_index_cache_path(&root);
        mutate_project_index_cache_json(&path, |value| {
            value["index"]["files"] = serde_json::Value::Array(vec![serde_json::Value::String(
                root.display().to_string(),
            )]);
        });

        assert!(load_project_index_cache(&root, 40_000).is_none());
        assert!(!path.exists());
        assert_eq!(quarantined_project_index_cache_files(&root).len(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_refuses_to_save_index_files_outside_workspace_root() {
        let root = temp_workspace("kuroya-project-index-cache-save-outside-file");
        let outside = temp_workspace("kuroya-project-index-cache-save-outside-target");
        let outside_file = outside.join("leak.rs");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(&outside_file, "fn leaked() {}\n").unwrap();
        let index: ProjectIndex = serde_json::from_value(serde_json::json!({
            "root": root,
            "files": [outside_file],
            "entries": [],
            "symbols": [],
            "truncated": false,
        }))
        .unwrap();
        let signature = mismatched_shape_signature();

        let error = save_project_index_cache(&root, &index, signature).unwrap_err();

        assert!(error.to_string().contains("paths outside workspace root"));
        assert!(!project_index_cache_path(&root).exists());

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn project_index_cache_refuses_to_save_workspace_root_as_index_file() {
        let root = temp_workspace("kuroya-project-index-cache-save-root-file");
        fs::create_dir_all(&root).unwrap();
        let index: ProjectIndex = serde_json::from_value(serde_json::json!({
            "root": root,
            "files": [root],
            "entries": [],
            "symbols": [],
            "truncated": false,
        }))
        .unwrap();
        let signature = mismatched_shape_signature();

        let error = save_project_index_cache(&root, &index, signature).unwrap_err();

        assert!(error.to_string().contains("paths outside workspace root"));
        assert!(!project_index_cache_path(&root).exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_refuses_to_serialize_oversized_payloads() {
        let root = temp_workspace("kuroya-project-index-cache-oversized-write");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);
        let cache = ProjectIndexCacheRef {
            schema: PROJECT_INDEX_CACHE_SCHEMA,
            root: &root,
            signature,
            payload_hash: project_index_cache_payload_hash(&index).unwrap(),
            index: &index,
        };
        let exact = project_index_cache_bytes_for_write(&cache, usize::MAX).unwrap();

        let error =
            project_index_cache_bytes_for_write(&cache, exact.len().saturating_sub(1)).unwrap_err();

        assert!(error.to_string().contains("too large to persist"));
        assert!(!project_index_cache_path(&root).exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_accepts_current_dir_relative_child_paths() {
        assert!(project_index_cache_path_is_inside_root(
            Path::new("src/main.rs"),
            Path::new(".")
        ));
        assert!(project_index_cache_path_is_inside_root(
            Path::new("./src/main.rs"),
            Path::new(".")
        ));
    }

    #[test]
    fn project_index_cache_rejects_current_dir_root_path() {
        assert!(!project_index_cache_path_is_inside_root(
            Path::new("."),
            Path::new(".")
        ));
        assert!(!project_index_cache_path_is_inside_root(
            Path::new(""),
            Path::new(".")
        ));
    }

    #[test]
    fn project_index_cache_rejects_current_dir_parent_escape() {
        assert!(!project_index_cache_path_is_inside_root(
            Path::new("../outside.rs"),
            Path::new(".")
        ));
        assert!(!project_index_cache_path_is_inside_root(
            Path::new("src/../../outside.rs"),
            Path::new(".")
        ));
    }

    #[cfg(windows)]
    #[test]
    fn project_index_cache_treats_case_variant_workspace_root_as_miss_without_quarantine() {
        let root = temp_workspace("kuroya-project-index-cache-case-root");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);

        save_project_index_cache(&root, &index, signature).unwrap();
        let path = project_index_cache_path(&root);
        let case_variant_root = ascii_case_variant_path(&root);
        mutate_project_index_cache_json(&path, |value| {
            value["root"] = serde_json::Value::String(case_variant_root.display().to_string());
            value["index"]["root"] =
                serde_json::Value::String(case_variant_root.display().to_string());
        });

        assert!(load_project_index_cache(&root, 40_000).is_none());
        assert!(path.exists());
        assert!(quarantined_project_index_cache_files(&root).is_empty());

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn project_index_cache_saves_case_variant_descendant_paths() {
        let root = temp_workspace("kuroya-project-index-cache-case-descendant");
        fs::create_dir_all(&root).unwrap();
        let case_variant_src = ascii_case_variant_path(&root).join("src");
        let case_variant_file = case_variant_src.join("main.rs");
        let index: ProjectIndex = serde_json::from_value(serde_json::json!({
            "root": root,
            "files": [case_variant_file],
            "entries": [
                {
                    "path": case_variant_src,
                    "relative_path": "src",
                    "is_dir": true,
                    "depth": 0,
                },
                {
                    "path": case_variant_file,
                    "relative_path": "src/main.rs",
                    "is_dir": false,
                    "depth": 1,
                }
            ],
            "symbols": [],
            "truncated": false,
            "max_files": 40_000,
        }))
        .unwrap();
        let signature = ProjectIndexSignature {
            max_files: 40_000,
            options_fingerprint: ProjectIndexOptions::new(40_000).options_fingerprint(),
            file_count: 1,
            entry_count: 2,
            truncated: false,
            symbol_policy_version: kuroya_core::PROJECT_INDEX_SYMBOL_POLICY_VERSION,
            fingerprint: 0,
        };

        save_project_index_cache(&root, &index, signature).unwrap();

        assert!(project_index_cache_path(&root).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn project_index_cache_rejects_case_variant_sibling_prefix() {
        let root = temp_workspace("kuroya-project-index-cache-case-sibling");
        fs::create_dir_all(&root).unwrap();
        let sibling_file = PathBuf::from(format!("{}-sibling", root.display())).join("leak.rs");
        let index: ProjectIndex = serde_json::from_value(serde_json::json!({
            "root": root,
            "files": [sibling_file],
            "entries": [],
            "symbols": [],
            "truncated": false,
        }))
        .unwrap();
        let signature = mismatched_shape_signature();

        let error = save_project_index_cache(&root, &index, signature).unwrap_err();

        assert!(error.to_string().contains("paths outside workspace root"));
        assert!(!project_index_cache_path(&root).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_rejects_index_files_with_parent_dir_components() {
        let root = temp_workspace("kuroya-project-index-cache-parent-dir-file");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);

        save_project_index_cache(&root, &index, signature).unwrap();
        let path = project_index_cache_path(&root);
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["index"]["files"] = serde_json::Value::Array(vec![serde_json::Value::String(
            root.join("src")
                .join("..")
                .join("outside.rs")
                .display()
                .to_string(),
        )]);
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();

        assert!(load_project_index_cache(&root, 40_000).is_none());
        assert!(!path.exists());
        assert_eq!(quarantined_project_index_cache_files(&root).len(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_rejects_workspace_root_as_entry_path() {
        let root = temp_workspace("kuroya-project-index-cache-root-entry");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);

        save_project_index_cache(&root, &index, signature).unwrap();
        let path = project_index_cache_path(&root);
        mutate_project_index_cache_json(&path, |value| {
            value["index"]["entries"] = serde_json::json!([
                {
                    "path": root,
                    "relative_path": "",
                    "is_dir": true,
                    "depth": 0,
                }
            ]);
        });

        assert!(load_project_index_cache(&root, 40_000).is_none());
        assert!(!path.exists());
        assert_eq!(quarantined_project_index_cache_files(&root).len(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_rejects_entry_relative_path_with_parent_dir_components() {
        let root = temp_workspace("kuroya-project-index-cache-parent-dir-entry");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);

        save_project_index_cache(&root, &index, signature).unwrap();
        let path = project_index_cache_path(&root);
        mutate_project_index_cache_json(&path, |value| {
            value["index"]["entries"][0]["relative_path"] =
                serde_json::Value::String("../outside.rs".to_owned());
        });

        assert!(load_project_index_cache(&root, 40_000).is_none());
        assert!(!path.exists());
        assert_eq!(quarantined_project_index_cache_files(&root).len(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_load_accepts_stale_signature_for_startup_reconciliation() {
        let root = temp_workspace("kuroya-project-index-cache-stale");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);

        save_project_index_cache(&root, &index, signature).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\nfn newer() {}\n").unwrap();
        let path = project_index_cache_path(&root);

        let loaded = load_project_index_cache(&root, 40_000).unwrap();
        assert_eq!(loaded.files().len(), 1);
        assert!(path.exists());
        assert!(quarantined_project_index_cache_files(&root).is_empty());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_validates_payload_hash_on_every_load() {
        let root = temp_workspace("kuroya-project-index-cache-tampered-payload");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);

        save_project_index_cache(&root, &index, signature).unwrap();
        let path = project_index_cache_path(&root);
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let payload_hash = value["payload_hash"]
            .as_u64()
            .expect("project index cache payload hash");
        value["payload_hash"] = serde_json::Value::from(payload_hash ^ 1);
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();

        assert!(load_project_index_cache(&root, 40_000).is_none());
        assert!(!path.exists());
        assert_eq!(quarantined_project_index_cache_files(&root).len(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_rejects_max_files_mismatch_between_index_and_signature() {
        let root = temp_workspace("kuroya-project-index-cache-max-files-shape");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let (index, mut signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);
        signature.max_files += 1;

        let error = save_project_index_cache(&root, &index, signature).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("signature does not match index shape")
        );
        assert!(!project_index_cache_path(&root).exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_round_trips_entry_metadata_and_incremental_updates() {
        let root = temp_workspace("kuroya-project-index-cache-incremental");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();
        let options = ProjectIndexOptions::new(40_000);
        let index = ProjectIndex::rebuild_with_options(&root, &options);
        let (_, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);
        save_project_index_cache(&root, &index, signature).unwrap();

        fs::write(root.join("src/b.rs"), "pub struct Added {}\n").unwrap();
        let mut updated = load_project_index_cache_unverified_with_options(&root, &options)
            .unwrap()
            .index
            .clone_for_update();
        assert!(updated.apply_path_changes(&root, &[root.join("src/b.rs")]));
        let updated_signature = updated.signature_from_entries(&options);
        let (fresh, fresh_signature) =
            ProjectIndex::rebuild_with_signature_options(&root, &options);
        assert_eq!(updated_signature, fresh_signature);
        assert_eq!(updated.files(), fresh.files());
        save_project_index_cache(&root, &updated, updated_signature).unwrap();
        let reloaded = load_project_index_cache(&root, 40_000).unwrap();
        assert_eq!(reloaded.files().len(), 2);

        let names: Vec<&str> = reloaded.symbols().iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"a"), "symbols: {names:?}");
        assert!(names.contains(&"Added"), "symbols: {names:?}");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_quarantines_index_payload_mismatching_signature() {
        let root = temp_workspace("kuroya-project-index-cache-mismatched-index");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);

        save_project_index_cache(&root, &index, signature).unwrap();
        let path = project_index_cache_path(&root);
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["index"]["files"] = serde_json::Value::Array(Vec::new());
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();

        assert!(load_project_index_cache(&root, 40_000).is_none());
        assert!(!path.exists());
        assert_eq!(quarantined_project_index_cache_files(&root).len(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_quarantines_same_count_index_file_mismatching_signature() {
        let root = temp_workspace("kuroya-project-index-cache-same-count-mismatch");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);

        save_project_index_cache(&root, &index, signature).unwrap();
        let path = project_index_cache_path(&root);
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["index"]["files"][0] =
            serde_json::Value::String(root.join("src/ghost.rs").display().to_string());
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();

        assert!(load_project_index_cache(&root, 40_000).is_none());
        assert!(!path.exists());
        assert_eq!(quarantined_project_index_cache_files(&root).len(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_quarantines_rehashed_file_list_that_does_not_match_entries() {
        let root = temp_workspace("kuroya-project-index-cache-file-entry-mismatch");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);

        save_project_index_cache(&root, &index, signature).unwrap();
        let path = project_index_cache_path(&root);
        mutate_project_index_cache_json(&path, |value| {
            value["index"]["files"][0] =
                serde_json::Value::String(root.join("src/ghost.rs").display().to_string());
        });

        assert!(load_project_index_cache(&root, 40_000).is_none());
        assert!(!path.exists());
        assert_eq!(quarantined_project_index_cache_files(&root).len(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_quarantines_duplicate_existing_index_file() {
        let root = temp_workspace("kuroya-project-index-cache-duplicate-existing");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/a.rs"), "fn first() {}\n").unwrap();
        fs::write(root.join("src/b.rs"), "fn second() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);

        save_project_index_cache(&root, &index, signature).unwrap();
        let path = project_index_cache_path(&root);
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let duplicate = value["index"]["files"][1].clone();
        value["index"]["files"][0] = duplicate;
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();

        assert!(load_project_index_cache(&root, 40_000).is_none());
        assert!(!path.exists());
        assert_eq!(quarantined_project_index_cache_files(&root).len(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_quarantines_forged_symbol_payload() {
        let root = temp_workspace("kuroya-project-index-cache-forged-symbol");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);

        save_project_index_cache(&root, &index, signature).unwrap();
        let path = project_index_cache_path(&root);
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["index"]["symbols"][0]["name"] = serde_json::Value::String("forged".to_owned());
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();

        assert!(load_project_index_cache(&root, 40_000).is_none());
        assert!(!path.exists());
        assert_eq!(quarantined_project_index_cache_files(&root).len(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_rejects_workspace_root_as_symbol_path() {
        let root = temp_workspace("kuroya-project-index-cache-root-symbol-path");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);

        save_project_index_cache(&root, &index, signature).unwrap();
        let path = project_index_cache_path(&root);
        mutate_project_index_cache_json(&path, |value| {
            value["index"]["symbols"][0]["path"] =
                serde_json::Value::String(root.display().to_string());
        });

        assert!(load_project_index_cache(&root, 40_000).is_none());
        assert!(!path.exists());
        assert_eq!(quarantined_project_index_cache_files(&root).len(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_rejects_empty_symbol_relative_path() {
        let root = temp_workspace("kuroya-project-index-cache-empty-symbol-relative");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);

        save_project_index_cache(&root, &index, signature).unwrap();
        let path = project_index_cache_path(&root);
        mutate_project_index_cache_json(&path, |value| {
            value["index"]["symbols"][0]["relative_path"] =
                serde_json::Value::String(String::new());
        });

        assert!(load_project_index_cache(&root, 40_000).is_none());
        assert!(!path.exists());
        assert_eq!(quarantined_project_index_cache_files(&root).len(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_quarantines_malformed_cache_bytes() {
        let root = temp_workspace("kuroya-project-index-cache-corrupt");
        fs::create_dir_all(state_dir(&root)).unwrap();
        let path = project_index_cache_path(&root);
        fs::write(&path, "{not json").unwrap();

        assert!(load_project_index_cache(&root, 40_000).is_none());
        assert!(!path.exists());
        let quarantined = quarantined_project_index_cache_files(&root);
        assert_eq!(quarantined.len(), 1);
        assert_eq!(fs::read_to_string(&quarantined[0]).unwrap(), "{not json");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_removes_invalid_cache_when_quarantine_move_fails() {
        let root = temp_workspace("kuroya-project-index-cache-quarantine-fallback");
        fs::create_dir_all(state_dir(&root)).unwrap();
        let path = project_index_cache_path(&root);
        let blocked_quarantine = state_dir(&root).join("blocked-quarantine");
        fs::write(&path, "{not json").unwrap();
        fs::create_dir_all(&blocked_quarantine).unwrap();

        assert!(quarantine_project_index_cache_to(&path, &blocked_quarantine).is_err());
        assert!(!path.exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_quarantines_oversized_cache_file() {
        let root = temp_workspace("kuroya-project-index-cache-oversized-load");
        fs::create_dir_all(state_dir(&root)).unwrap();
        let path = project_index_cache_path(&root);
        fs::File::create(&path)
            .unwrap()
            .set_len(PROJECT_INDEX_CACHE_MAX_BYTES + 1)
            .unwrap();

        assert!(load_project_index_cache(&root, 40_000).is_none());
        assert!(!path.exists());
        let quarantined = quarantined_project_index_cache_files(&root);
        assert_eq!(quarantined.len(), 1);
        assert_eq!(
            fs::metadata(&quarantined[0]).unwrap().len(),
            PROJECT_INDEX_CACHE_MAX_BYTES + 1
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_cache_quarantines_wrong_schema_cache() {
        let root = temp_workspace("kuroya-project-index-cache-wrong-schema");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn indexed() {}\n").unwrap();
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);
        save_project_index_cache(&root, &index, signature).unwrap();
        let path = project_index_cache_path(&root);
        let mut value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        value["schema"] = serde_json::Value::from(999);
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();

        assert!(load_project_index_cache(&root, 40_000).is_none());
        assert!(!path.exists());
        assert_eq!(quarantined_project_index_cache_files(&root).len(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    fn mismatched_shape_signature() -> ProjectIndexSignature {
        ProjectIndexSignature {
            max_files: 40_000,
            options_fingerprint: ProjectIndexOptions::new(40_000).options_fingerprint(),
            file_count: 1,
            entry_count: 1,
            truncated: false,
            symbol_policy_version: kuroya_core::PROJECT_INDEX_SYMBOL_POLICY_VERSION,
            fingerprint: 0,
        }
    }

    fn quarantined_project_index_cache_files(root: &Path) -> Vec<PathBuf> {
        fs::read_dir(state_dir(root))
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("project-index.json.corrupt."))
            })
            .collect()
    }

    fn mutate_project_index_cache_json(path: &Path, mutate: impl FnOnce(&mut serde_json::Value)) {
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        mutate(&mut value);
        let index: ProjectIndex = serde_json::from_value(value["index"].clone()).unwrap();
        value["payload_hash"] =
            serde_json::Value::from(project_index_cache_payload_hash(&index).unwrap());
        fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    }

    fn temp_workspace(prefix: &str) -> PathBuf {
        let workspace = std::env::temp_dir().join(format!("{prefix}-{}", unique_suffix()));
        fs::create_dir_all(&workspace).unwrap();
        workspace
    }

    #[cfg(windows)]
    fn ascii_case_variant_path(path: &Path) -> PathBuf {
        let mut text = path.display().to_string();
        for (index, ch) in text.char_indices() {
            if ch.is_ascii_lowercase() {
                text.replace_range(
                    index..index + ch.len_utf8(),
                    &ch.to_ascii_uppercase().to_string(),
                );
                return PathBuf::from(text);
            }
            if ch.is_ascii_uppercase() {
                text.replace_range(
                    index..index + ch.len_utf8(),
                    &ch.to_ascii_lowercase().to_string(),
                );
                return PathBuf::from(text);
            }
        }
        path.to_path_buf()
    }

    fn unique_suffix() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }
}
