use crate::persistence_storage::{legacy_state_dir, state_dir};
use std::{
    ffi::{OsStr, OsString},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static LOCAL_HISTORY_SEQUENCE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
pub(super) fn local_history_snapshot_path(
    workspace_root: &Path,
    path: &Path,
    unique: u128,
) -> PathBuf {
    let (dir, name) = local_history_snapshot_location(workspace_root, path);
    local_history_snapshot_path_in_dir(&dir, unique, &name)
}

pub(super) fn local_history_snapshot_path_in_dir(dir: &Path, unique: u128, name: &str) -> PathBuf {
    dir.join(format!("{unique}.{name}.bak"))
}

#[cfg(test)]
pub(super) fn local_history_snapshot_location(
    workspace_root: &Path,
    path: &Path,
) -> (PathBuf, String) {
    let lookup = local_history_snapshot_lookup(workspace_root, path);
    (lookup.dir, lookup.primary_name)
}

#[derive(Debug, Clone)]
pub(super) struct LocalHistorySnapshotLookup {
    pub(super) dir: PathBuf,
    pub(super) legacy_dir: Option<PathBuf>,
    pub(super) primary_name: String,
    pub(super) legacy_name: Option<String>,
}

pub(super) fn local_history_snapshot_lookup(
    workspace_root: &Path,
    path: &Path,
) -> LocalHistorySnapshotLookup {
    let workspace_root = normalize_path_lexically(workspace_root);
    let path = normalize_path_lexically(path);
    let mut dir = state_dir(&workspace_root).join("history");
    let mut legacy_dir = legacy_state_dir(&workspace_root).join("history");
    let collision_path = if let Ok(relative) = path.strip_prefix(&workspace_root) {
        append_sanitized_parent_components(&mut dir, relative);
        append_sanitized_parent_components(&mut legacy_dir, relative);
        relative
    } else {
        dir = dir.join("external").join(path_hash(&path));
        legacy_dir = legacy_dir.join("external").join(path_hash(&path));
        path.as_path()
    };
    let (primary_name, legacy_name) =
        local_history_file_names_from_normalized_path(&path, collision_path);
    let legacy_dir = (legacy_dir != dir).then_some(legacy_dir);
    LocalHistorySnapshotLookup {
        dir,
        legacy_dir,
        primary_name,
        legacy_name,
    }
}

fn local_history_file_names_from_normalized_path(
    path: &Path,
    collision_path: &Path,
) -> (String, Option<String>) {
    let legacy_name = legacy_local_history_file_name_from_normalized_path(path);
    if local_history_path_needs_name_hash(collision_path) {
        (
            format!("{legacy_name}.{}", path_hash(path)),
            Some(legacy_name),
        )
    } else {
        (legacy_name, None)
    }
}

fn legacy_local_history_file_name_from_normalized_path(path: &Path) -> String {
    path.file_name()
        .map(sanitize_snapshot_file_name_component)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "untitled".to_owned())
}

fn local_history_path_needs_name_hash(path: &Path) -> bool {
    path.components().any(|component| match component {
        Component::Normal(value) => component_needs_collision_guard(value),
        Component::Prefix(_) | Component::RootDir | Component::CurDir | Component::ParentDir => {
            false
        }
    })
}

fn normalize_path_lexically(path: &Path) -> PathBuf {
    let mut prefix = None;
    let mut has_root = false;
    let mut parts = Vec::<OsString>::new();

    for component in path.components() {
        match component {
            Component::Prefix(value) => {
                prefix = Some(value.as_os_str().to_os_string());
                parts.clear();
                has_root = false;
            }
            Component::RootDir => {
                has_root = true;
                parts.clear();
            }
            Component::CurDir => {}
            Component::ParentDir => match parts.last().map(|part| part.as_os_str()) {
                Some(last) if last != OsStr::new("..") => {
                    parts.pop();
                }
                _ if !has_root => parts.push(OsString::from("..")),
                _ => {}
            },
            Component::Normal(value) => parts.push(value.to_os_string()),
        }
    }

    let mut normalized = PathBuf::new();
    if let Some(prefix) = prefix {
        normalized.push(prefix);
    }
    if has_root {
        normalized.push(std::path::MAIN_SEPARATOR.to_string());
    }
    for part in parts {
        normalized.push(part);
    }

    if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}

fn append_sanitized_parent_components(dir: &mut PathBuf, relative: &Path) {
    if let Some(parent) = relative.parent() {
        for component in parent.components() {
            match component {
                Component::Normal(value) => dir.push(sanitize_component(value)),
                Component::ParentDir => dir.push("__parent__"),
                Component::CurDir | Component::Prefix(_) | Component::RootDir => {}
            }
        }
    }
}

fn sanitize_component(value: &OsStr) -> String {
    if let Some(text) = value.to_str() {
        if text.is_empty() {
            return "_".to_owned();
        }
        if !component_needs_sanitizing(text) && !component_needs_safe_label_guard(text) {
            return text.to_owned();
        }
    }

    let mut sanitized = value
        .to_string_lossy()
        .chars()
        .map(sanitized_component_char)
        .collect::<String>();
    guard_sanitized_component_label(&mut sanitized);
    if sanitized.is_empty() {
        "_".to_owned()
    } else {
        sanitized
    }
}

fn sanitize_snapshot_file_name_component(value: &OsStr) -> String {
    if let Some(text) = value.to_str() {
        if text.is_empty() {
            return "_".to_owned();
        }
        if !component_needs_sanitizing(text) {
            return text.to_owned();
        }
    }

    let sanitized = value
        .to_string_lossy()
        .chars()
        .map(sanitized_component_char)
        .collect::<String>();
    if sanitized.is_empty() {
        "_".to_owned()
    } else {
        sanitized
    }
}

fn sanitized_component_char(ch: char) -> char {
    if ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
        '_'
    } else {
        ch
    }
}

fn component_needs_sanitizing(text: &str) -> bool {
    text.chars().any(|ch| {
        ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
    })
}

fn component_needs_collision_guard(value: &OsStr) -> bool {
    let Some(text) = value.to_str() else {
        return true;
    };
    component_needs_sanitizing(text) || component_needs_safe_label_guard(text)
}

fn component_needs_safe_label_guard(text: &str) -> bool {
    text.is_empty()
        || component_has_trailing_windows_trim_char(text)
        || component_has_reserved_windows_label(text)
}

fn guard_sanitized_component_label(label: &mut String) {
    replace_trailing_windows_trim_chars(label);
    if label.is_empty() {
        label.push('_');
    }
    if component_has_reserved_windows_label(label) {
        label.insert(0, '_');
    }
}

fn component_has_trailing_windows_trim_char(text: &str) -> bool {
    matches!(text.as_bytes().last(), Some(b' ' | b'.'))
}

fn replace_trailing_windows_trim_chars(label: &mut String) {
    let trailing = label
        .bytes()
        .rev()
        .take_while(|byte| matches!(byte, b' ' | b'.'))
        .count();
    if trailing == 0 {
        return;
    }

    let keep = label.len().saturating_sub(trailing);
    label.truncate(keep);
    for _ in 0..trailing {
        label.push('_');
    }
}

fn component_has_reserved_windows_label(text: &str) -> bool {
    let stem = text.split('.').next().unwrap_or(text);
    stem.eq_ignore_ascii_case("con")
        || stem.eq_ignore_ascii_case("prn")
        || stem.eq_ignore_ascii_case("aux")
        || stem.eq_ignore_ascii_case("nul")
        || component_has_reserved_windows_numbered_label(stem, "com")
        || component_has_reserved_windows_numbered_label(stem, "lpt")
}

fn component_has_reserved_windows_numbered_label(stem: &str, prefix: &str) -> bool {
    stem.len() == 4
        && stem
            .get(..3)
            .is_some_and(|stem_prefix| stem_prefix.eq_ignore_ascii_case(prefix))
        && matches!(stem.as_bytes()[3], b'1'..=b'9')
}

pub(super) fn history_unique_id() -> u128 {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let counter = LOCAL_HISTORY_SEQUENCE_COUNTER.fetch_add(1, Ordering::Relaxed);
    history_unique_id_from_parts(time, std::process::id(), counter)
}

pub(super) fn history_unique_id_from_parts(
    time_nanos: u128,
    process_id: u32,
    counter: u64,
) -> u128 {
    time_nanos
        .saturating_mul(1_000_000_000_000)
        .saturating_add(u128::from(process_id).saturating_mul(1_000_000))
        .saturating_add(u128::from(counter % 1_000_000))
}

fn path_hash(path: &Path) -> String {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;
    for byte in path.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}
