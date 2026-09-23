use crate::native_paths::normalize_native_path;
use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Component, Path, PathBuf},
};

#[cfg(not(test))]
const APP_STATE_FILE_NAME: &str = "state.json";
const APP_SETTINGS_FILE_NAME: &str = "settings.toml";
const LEGACY_WORKSPACE_STATE_DIR_NAME: &str = ".kuroya";
const SESSION_FILE_NAME: &str = "session.json";
const PROJECT_INDEX_CACHE_FILE_NAME: &str = "project-index.json";
const SESSION_SNAPSHOTS_DIR_NAME: &str = "snapshots";
const WORKSPACE_SNAPSHOTS_DIR_NAME: &str = "workspace-snapshots";
const WORKSPACE_STATE_BUCKET_DIR_NAME: &str = "workspaces";
const WORKSPACE_STATE_BUCKET_FALLBACK_LABEL: &str = "workspace";
const WORKSPACE_STATE_BUCKET_LABEL_MAX_BYTES: usize = 48;
const WORKSPACE_STATE_HASH_OFFSET: u64 = 0xcbf29ce484222325;
const WORKSPACE_STATE_HASH_PRIME: u64 = 0x100000001b3;

#[cfg(not(test))]
pub(crate) fn app_state_path() -> PathBuf {
    app_state_dir().join(APP_STATE_FILE_NAME)
}

pub(crate) fn app_settings_path() -> PathBuf {
    app_state_dir().join(APP_SETTINGS_FILE_NAME)
}

pub(crate) fn state_dir(workspace_root: &Path) -> PathBuf {
    let normalized = canonical_workspace_root_for_storage(workspace_root);
    external_workspace_state_dir(&normalized)
}

/// Buckets workspace state by the canonical workspace root so the same
/// folder opened through different spellings — case differences, junctions,
/// subst drives, `.`/`..` segments — hashes to a single bucket instead of
/// silently stranding the session and re-arming the trust prompt.
/// Canonicalization needs the path to exist; when it fails (fresh or
/// inaccessible root) the lexical normalization below is used, which keeps
/// such buckets stable. Sessions saved before canonicalization stay under
/// the old raw-text bucket; the canonical bucket starts fresh (accepted
/// one-time migration cost for already-saved workspaces).
fn canonical_workspace_root_for_storage(workspace_root: &Path) -> PathBuf {
    let normalized = normalize_workspace_root_for_storage(workspace_root);
    match fs::canonicalize(&normalized) {
        // canonicalize yields `\\?\`-prefixed verbatim paths on Windows;
        // strip the prefix so the hashed text matches plain spellings.
        Ok(canonical) => normalize_native_path(canonical),
        Err(_) => normalized,
    }
}

pub(crate) fn legacy_state_dir(workspace_root: &Path) -> PathBuf {
    normalize_workspace_root_for_storage(workspace_root).join(LEGACY_WORKSPACE_STATE_DIR_NAME)
}

pub(crate) fn session_path(workspace_root: &Path) -> PathBuf {
    workspace_storage_path(workspace_root, SESSION_FILE_NAME)
}

pub(crate) fn legacy_session_path(workspace_root: &Path) -> PathBuf {
    legacy_workspace_storage_path(workspace_root, SESSION_FILE_NAME)
}

pub(crate) fn project_index_cache_path(workspace_root: &Path) -> PathBuf {
    workspace_storage_path(workspace_root, PROJECT_INDEX_CACHE_FILE_NAME)
}

pub(crate) fn legacy_project_index_cache_path(workspace_root: &Path) -> PathBuf {
    legacy_workspace_storage_path(workspace_root, PROJECT_INDEX_CACHE_FILE_NAME)
}

pub(crate) fn session_snapshots_dir(workspace_root: &Path) -> PathBuf {
    workspace_storage_path(workspace_root, SESSION_SNAPSHOTS_DIR_NAME)
}

pub(crate) fn legacy_session_snapshots_dir(workspace_root: &Path) -> PathBuf {
    legacy_workspace_storage_path(workspace_root, SESSION_SNAPSHOTS_DIR_NAME)
}

pub(crate) fn workspace_snapshots_dir(workspace_root: &Path) -> PathBuf {
    workspace_storage_path(workspace_root, WORKSPACE_SNAPSHOTS_DIR_NAME)
}

pub(crate) fn legacy_workspace_snapshots_dir(workspace_root: &Path) -> PathBuf {
    legacy_workspace_storage_path(workspace_root, WORKSPACE_SNAPSHOTS_DIR_NAME)
}

fn workspace_storage_path(workspace_root: &Path, storage_name: &str) -> PathBuf {
    state_dir(workspace_root).join(storage_name)
}

fn legacy_workspace_storage_path(workspace_root: &Path, storage_name: &str) -> PathBuf {
    legacy_state_dir(workspace_root).join(storage_name)
}

fn normalize_workspace_root_for_storage(workspace_root: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    let mut normal_components = 0usize;
    let mut anchored = false;

    for component in workspace_root.components() {
        match component {
            Component::Prefix(prefix) => {
                normalized.push(prefix.as_os_str());
                anchored = true;
                normal_components = 0;
            }
            Component::RootDir => {
                normalized.push(component.as_os_str());
                anchored = true;
                normal_components = 0;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if normal_components > 0 {
                    normalized.pop();
                    normal_components -= 1;
                } else if !anchored {
                    normalized.push(component.as_os_str());
                }
            }
            Component::Normal(component) => {
                normalized.push(component);
                normal_components += 1;
            }
        }
    }

    normalized
}

fn is_unsafe_storage_component_char(ch: char) -> bool {
    ch.is_control()
        || matches!(
            ch,
            '\u{061c}'
                | '\u{200b}'..='\u{200f}'
                | '\u{2028}'..='\u{202e}'
                | '\u{2060}'..='\u{206f}'
                | '\u{feff}'
        )
}

fn storage_component_has_reserved_windows_label(text: &str) -> bool {
    let stem = text.split('.').next().unwrap_or(text);
    stem.eq_ignore_ascii_case("con")
        || stem.eq_ignore_ascii_case("prn")
        || stem.eq_ignore_ascii_case("aux")
        || stem.eq_ignore_ascii_case("nul")
        || storage_component_has_reserved_windows_numbered_label(stem, "com")
        || storage_component_has_reserved_windows_numbered_label(stem, "lpt")
}

fn storage_component_has_reserved_windows_numbered_label(stem: &str, prefix: &str) -> bool {
    stem.len() == 4
        && stem
            .get(..3)
            .is_some_and(|stem_prefix| stem_prefix.eq_ignore_ascii_case(prefix))
        && matches!(stem.as_bytes()[3], b'1'..=b'9')
}

fn external_workspace_state_dir(workspace_root: &Path) -> PathBuf {
    app_state_dir()
        .join(WORKSPACE_STATE_BUCKET_DIR_NAME)
        .join(workspace_state_bucket_name(workspace_root))
}

fn workspace_state_bucket_name(workspace_root: &Path) -> String {
    let label = workspace_state_bucket_label(workspace_root);
    let hash = workspace_state_hash(workspace_root);
    format!("{label}-{hash:016x}")
}

fn workspace_state_bucket_label(workspace_root: &Path) -> String {
    let mut label = String::with_capacity(WORKSPACE_STATE_BUCKET_LABEL_MAX_BYTES);
    let raw_label = workspace_root
        .file_name()
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| OsStr::new(WORKSPACE_STATE_BUCKET_FALLBACK_LABEL));

    for ch in raw_label.to_string_lossy().chars() {
        let Some(ch) = workspace_state_bucket_label_char(ch) else {
            continue;
        };
        if label.len() >= WORKSPACE_STATE_BUCKET_LABEL_MAX_BYTES {
            break;
        }
        label.push(ch);
    }

    guard_workspace_state_bucket_label(&mut label);
    if label.is_empty() {
        WORKSPACE_STATE_BUCKET_FALLBACK_LABEL.to_owned()
    } else {
        label
    }
}

fn workspace_state_bucket_label_char(ch: char) -> Option<char> {
    if is_unsafe_storage_component_char(ch) {
        return None;
    }
    if ch.is_ascii_alphanumeric() {
        return Some(ch.to_ascii_lowercase());
    }
    if matches!(ch, '-' | '_' | '.') {
        return Some(ch);
    }
    Some('_')
}

fn guard_workspace_state_bucket_label(label: &mut String) {
    while label.ends_with('.') {
        label.pop();
        label.push('_');
    }
    while label.starts_with('.') {
        label.remove(0);
    }
    if storage_component_has_reserved_windows_label(label) {
        label.insert(0, '_');
    }
}

fn workspace_state_hash(workspace_root: &Path) -> u64 {
    let mut hash = WORKSPACE_STATE_HASH_OFFSET;
    for byte in workspace_root.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(WORKSPACE_STATE_HASH_PRIME);
    }
    hash
}

#[cfg(test)]
pub(crate) fn app_state_dir() -> PathBuf {
    if let Some(path) = configured_app_state_dir() {
        return path;
    }

    env::temp_dir()
        .join("kuroya-test-app-state")
        .join(std::process::id().to_string())
        .join(test_thread_storage_label())
}

#[cfg(not(test))]
pub(crate) fn app_state_dir() -> PathBuf {
    if let Some(path) = configured_app_state_dir() {
        return path;
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(path) = env::var_os("APPDATA").or_else(|| env::var_os("LOCALAPPDATA")) {
            return PathBuf::from(path).join("Kuroya");
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = home_dir() {
            return home
                .join("Library")
                .join("Application Support")
                .join("Kuroya");
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Some(path) = env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(path).join("Kuroya");
        }
        if let Some(home) = home_dir() {
            return home.join(".local").join("share").join("Kuroya");
        }
    }

    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".kuroya")
        .join("app-state")
}

fn configured_app_state_dir() -> Option<PathBuf> {
    let path = PathBuf::from(env::var_os("KUROYA_STATE_DIR")?);
    if path.as_os_str().is_empty() {
        return None;
    }
    if path.is_absolute() {
        return Some(path);
    }
    Some(env::current_dir().map_or(path.clone(), |current| current.join(path)))
}

#[cfg(all(not(test), not(target_os = "windows")))]
fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
fn test_thread_storage_label() -> String {
    format!("{:?}", std::thread::current().id())
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        ffi::OsStr,
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_path(name: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "kuroya-persistence-paths-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn app_workspace_state_bucket(workspace_root: &Path) -> PathBuf {
        app_state_dir()
            .join(WORKSPACE_STATE_BUCKET_DIR_NAME)
            .join(workspace_state_bucket_name(workspace_root))
    }

    fn assert_not_workspace_kuroya_path(workspace_root: &Path, path: &Path) {
        let old_workspace_state = workspace_root.join(".kuroya");
        assert!(
            !path.starts_with(&old_workspace_state),
            "{} unexpectedly used {}",
            path.display(),
            old_workspace_state.display()
        );
    }

    #[test]
    fn app_settings_path_uses_app_state_dir() {
        assert_eq!(
            app_settings_path(),
            app_state_dir().join(APP_SETTINGS_FILE_NAME)
        );
    }

    #[test]
    fn workspace_storage_paths_share_app_state_bucket_for_normal_workspace() {
        let workspace = temp_path("normal-workspace").join("root");
        fs::create_dir_all(workspace.join("src")).unwrap();

        let workspace_with_normalization = workspace.join(".").join("src").join("..");
        let normalized = normalize_workspace_root_for_storage(&workspace_with_normalization);
        let state = app_workspace_state_bucket(&normalized);

        assert_eq!(normalized, workspace);
        assert_eq!(state_dir(&workspace_with_normalization), state);
        assert_not_workspace_kuroya_path(&workspace, &state);
        assert_eq!(
            session_path(&workspace_with_normalization),
            state.join(SESSION_FILE_NAME)
        );
        assert_eq!(
            project_index_cache_path(&workspace_with_normalization),
            state.join(PROJECT_INDEX_CACHE_FILE_NAME)
        );
        assert_eq!(
            session_snapshots_dir(&workspace_with_normalization),
            state.join(SESSION_SNAPSHOTS_DIR_NAME)
        );
        assert_eq!(
            workspace_snapshots_dir(&workspace_with_normalization),
            state.join(WORKSPACE_SNAPSHOTS_DIR_NAME)
        );
        assert_not_workspace_kuroya_path(&workspace, &session_path(&workspace_with_normalization));
        assert_not_workspace_kuroya_path(
            &workspace,
            &project_index_cache_path(&workspace_with_normalization),
        );
        assert_not_workspace_kuroya_path(
            &workspace,
            &session_snapshots_dir(&workspace_with_normalization),
        );
        assert_not_workspace_kuroya_path(
            &workspace,
            &workspace_snapshots_dir(&workspace_with_normalization),
        );

        fs::remove_dir_all(workspace.parent().unwrap()).unwrap();
    }

    #[test]
    fn workspace_root_normalization_preserves_leading_parent_components() {
        let workspace = PathBuf::from("..")
            .join("outside")
            .join("workspace")
            .join("..");

        assert_eq!(
            state_dir(&workspace),
            app_workspace_state_bucket(&PathBuf::from("..").join("outside"))
        );
    }

    #[test]
    fn legacy_workspace_storage_paths_stay_workspace_local() {
        let workspace = PathBuf::from("workspace").join(".").join("src").join("..");
        let legacy = PathBuf::from("workspace").join(LEGACY_WORKSPACE_STATE_DIR_NAME);

        assert_eq!(legacy_state_dir(&workspace), legacy);
        assert_eq!(
            legacy_session_path(&workspace),
            legacy.join(SESSION_FILE_NAME)
        );
        assert_eq!(
            legacy_project_index_cache_path(&workspace),
            legacy.join(PROJECT_INDEX_CACHE_FILE_NAME)
        );
        assert_eq!(
            legacy_session_snapshots_dir(&workspace),
            legacy.join(SESSION_SNAPSHOTS_DIR_NAME)
        );
        assert_eq!(
            legacy_workspace_snapshots_dir(&workspace),
            legacy.join(WORKSPACE_SNAPSHOTS_DIR_NAME)
        );
    }

    #[test]
    fn unsafe_workspace_root_uses_bounded_external_state_bucket() {
        let raw_name = format!("raw\n\u{202e}workspace{}", "x".repeat(160));
        let workspace = PathBuf::from("workspace")
            .join("..")
            .join(&raw_name)
            .join(".");
        let state = state_dir(&workspace);
        let state_text = state.as_os_str().to_string_lossy();
        let bucket = state.file_name().unwrap().to_string_lossy();

        assert_eq!(
            state
                .parent()
                .and_then(Path::file_name)
                .and_then(OsStr::to_str),
            Some(WORKSPACE_STATE_BUCKET_DIR_NAME)
        );
        assert!(!state_text.contains('\n'));
        assert!(!state_text.contains('\u{202e}'));
        assert!(!state_text.contains(&"x".repeat(160)));
        assert!(bucket.len() <= WORKSPACE_STATE_BUCKET_LABEL_MAX_BYTES + 1 + 16);
        assert!(
            bucket
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        );
    }

    #[test]
    fn file_shaped_workspace_root_uses_external_state_bucket() {
        let workspace = temp_path("file-root");
        fs::write(&workspace, b"not a directory").unwrap();

        let state = state_dir(&workspace);

        assert!(!state.starts_with(&workspace));
        assert_eq!(
            state
                .parent()
                .and_then(Path::file_name)
                .and_then(OsStr::to_str),
            Some(WORKSPACE_STATE_BUCKET_DIR_NAME)
        );

        fs::remove_file(workspace).unwrap();
    }

    #[test]
    fn workspace_state_bucket_matches_lexical_equivalents_of_existing_roots() {
        let workspace = temp_path("canonical-equivalent");
        fs::create_dir_all(workspace.join("src")).unwrap();
        let dotted = workspace.join("src").join("..");

        // Canonicalization resolves `.`/`..` spellings of an existing root
        // to one bucket.
        assert_eq!(state_dir(&dotted), state_dir(&workspace));

        fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn workspace_state_bucket_falls_back_to_lexical_normalization_for_missing_roots() {
        let workspace = temp_path("canonical-missing-root");
        let dotted = workspace.join(".").join("sub").join("..");

        // Neither spelling exists, so canonicalization fails for both and
        // the lexical normalization keeps one bucket.
        assert_eq!(state_dir(&dotted), state_dir(&workspace));
    }

    #[cfg(windows)]
    #[test]
    fn workspace_state_bucket_matches_roots_differing_only_by_case() {
        let workspace = temp_path("case-bucket-root");
        fs::create_dir_all(&workspace).unwrap();
        let lower_spelling = PathBuf::from(workspace.to_string_lossy().to_ascii_lowercase());
        assert_ne!(lower_spelling, workspace);

        // Canonicalization resolves both spellings to the true on-disk
        // casing, so one folder hashes to one bucket either way.
        assert_eq!(state_dir(&lower_spelling), state_dir(&workspace));

        fs::remove_dir_all(&workspace).unwrap();
    }
}
