use crate::{
    KuroyaApp,
    file_history::{LOCAL_HISTORY_MAX_BYTES, latest_local_history_snapshot_text_async},
    path_display::sanitized_display_label_cow,
    ui_events::UiEvent,
    workspace_state::paths_match_lexically,
};
use std::{
    borrow::Cow,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

const LOCAL_HISTORY_PATH_DISPLAY_MAX_CHARS: usize = 96;
const LOCAL_HISTORY_ERROR_DISPLAY_MAX_CHARS: usize = 160;

impl KuroyaApp {
    pub(crate) fn open_active_file_latest_local_history(&mut self) {
        let Some(path) = self.active_file_or_diff_source_path("open local history") else {
            return;
        };
        let root = self.workspace.root.clone();
        let generation = self.workspace_event_generation;
        let tx = self.tx.clone();
        let path_label = local_history_display_path(&path);
        self.status = local_history_loading_status_for_label(path_label.as_ref());
        self.record_async_task_started("Local History", path_label);
        self.runtime.spawn(async move {
            let event = match latest_local_history_snapshot_text_async(
                &root,
                &path,
                LOCAL_HISTORY_MAX_BYTES,
            )
            .await
            {
                Ok(Some((snapshot, text))) => UiEvent::LocalHistoryLoaded {
                    root,
                    generation,
                    path,
                    snapshot_path: snapshot.path,
                    sequence: snapshot.sequence,
                    text,
                },
                Ok(None) => UiEvent::LocalHistoryFailed {
                    root,
                    generation,
                    path,
                    error: "no snapshots".to_owned(),
                },
                Err(error) => UiEvent::LocalHistoryFailed {
                    root,
                    generation,
                    path,
                    error: error.to_string(),
                },
            };
            let _ = crate::ui_event_channel::send_ui_event(&tx, event);
        });
    }

    pub(crate) fn apply_local_history_loaded(
        &mut self,
        root: PathBuf,
        generation: u64,
        path: PathBuf,
        snapshot_path: PathBuf,
        sequence: u128,
        text: String,
    ) {
        if !self.workspace_event_is_current(&root, generation) {
            return;
        }
        let path_label = local_history_display_path(&path);
        if let Some(reason) = local_history_loaded_text_block_reason(&text) {
            self.status = local_history_failed_status_for_labels(path_label.as_ref(), reason);
            return;
        }

        let revision_path_label = local_history_revision_path_label_for_sources(
            &path,
            path_label.as_ref(),
            self.local_history_open_source_paths(),
        );
        let snapshot_label = local_history_display_path(&snapshot_path);
        let label = local_history_revision_label_for_path_label(revision_path_label.as_ref());
        let target =
            local_history_revision_target_for_path_label(revision_path_label.as_ref(), sequence);
        let status =
            local_history_loaded_status_for_labels(path_label.as_ref(), snapshot_label.as_ref());
        self.open_virtual_revision_buffer(label, path, text, target, "local history");
        self.status = status;
    }

    pub(crate) fn apply_local_history_failed(
        &mut self,
        root: PathBuf,
        generation: u64,
        path: PathBuf,
        error: String,
    ) {
        if !self.workspace_event_is_current(&root, generation) {
            return;
        }
        self.status = local_history_failed_status(&path, &error);
    }

    fn local_history_open_source_paths(&self) -> impl Iterator<Item = &Path> {
        self.buffers
            .iter()
            .filter_map(|buffer| buffer.path().map(PathBuf::as_path))
            .chain(
                self.diff_buffer_sources
                    .values()
                    .map(|source| source.path.as_path()),
            )
    }
}

#[cfg(test)]
pub(crate) fn local_history_revision_label(path: &Path) -> String {
    let path_label = local_history_display_path(path);
    local_history_revision_label_for_path_label(path_label.as_ref())
}

fn local_history_revision_label_for_path_label(path_label: &str) -> String {
    format!("{path_label} (Local History)")
}

fn local_history_revision_path_label_for_sources<'a, 'b>(
    path: &Path,
    compact_path_label: &'b str,
    source_paths: impl IntoIterator<Item = &'a Path>,
) -> Cow<'b, str> {
    if local_history_source_path_file_name_is_ambiguous(path, source_paths) {
        Cow::Owned(local_history_revision_identity_path_label(path))
    } else {
        Cow::Borrowed(compact_path_label)
    }
}

#[cfg(test)]
pub(crate) fn local_history_loading_status(path: &Path) -> String {
    let path_label = local_history_display_path(path);
    local_history_loading_status_for_label(path_label.as_ref())
}

fn local_history_loading_status_for_label(path_label: &str) -> String {
    format!("Loading local history for {path_label}")
}

#[cfg(test)]
pub(crate) fn local_history_loaded_status(path: &Path, snapshot_path: &Path) -> String {
    let path_label = local_history_display_path(path);
    let snapshot_label = local_history_display_path(snapshot_path);
    local_history_loaded_status_for_labels(path_label.as_ref(), snapshot_label.as_ref())
}

fn local_history_loaded_status_for_labels(path_label: &str, snapshot_label: &str) -> String {
    format!("Opened local history for {path_label} from {snapshot_label}")
}

pub(crate) fn local_history_failed_status(path: &Path, error: &str) -> String {
    let path_label = local_history_display_path(path);
    if error == "no snapshots" {
        local_history_no_snapshots_status_for_label(path_label.as_ref())
    } else {
        let error_label = local_history_display_error(error);
        local_history_failed_status_for_labels(path_label.as_ref(), error_label.as_ref())
    }
}

fn local_history_no_snapshots_status_for_label(path_label: &str) -> String {
    format!("No local history snapshots for {path_label}")
}

fn local_history_failed_status_for_labels(path_label: &str, error_label: &str) -> String {
    format!("Could not open local history for {path_label}: {error_label}")
}

fn local_history_revision_target_for_path_label(path_label: &str, sequence: u128) -> String {
    format!("{path_label} local history snapshot {sequence}")
}

fn local_history_revision_identity_path_label(path: &Path) -> String {
    let hash_suffix = format!(" #{:016x}", local_history_path_hash(path));
    let max_path_chars = LOCAL_HISTORY_PATH_DISPLAY_MAX_CHARS.saturating_sub(hash_suffix.len());
    let path_label =
        local_history_display_label(local_history_full_path_text(path), max_path_chars, ".");
    local_history_display_label(
        Cow::Owned(format!("{}{}", path_label.as_ref(), hash_suffix)),
        LOCAL_HISTORY_PATH_DISPLAY_MAX_CHARS,
        ".",
    )
    .into_owned()
}

fn local_history_source_path_file_name_is_ambiguous<'a>(
    path: &Path,
    source_paths: impl IntoIterator<Item = &'a Path>,
) -> bool {
    let Some(file_name) = path.file_name() else {
        return false;
    };

    source_paths.into_iter().any(|candidate| {
        candidate.file_name() == Some(file_name) && !paths_match_lexically(candidate, path)
    })
}

fn local_history_path_hash(path: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn local_history_display_path(path: &Path) -> Cow<'_, str> {
    local_history_display_label(
        local_history_compact_path_text(path),
        LOCAL_HISTORY_PATH_DISPLAY_MAX_CHARS,
        ".",
    )
}

fn local_history_compact_path_text(path: &Path) -> Cow<'_, str> {
    if path.as_os_str().is_empty() {
        return Cow::Borrowed(".");
    }

    path.file_name()
        .and_then(|name| name.to_str())
        .or_else(|| path.to_str())
        .map(Cow::Borrowed)
        .unwrap_or_else(|| Cow::Owned(path.display().to_string()))
}

fn local_history_full_path_text(path: &Path) -> Cow<'_, str> {
    if path.as_os_str().is_empty() {
        return Cow::Borrowed(".");
    }

    path.to_str()
        .map(Cow::Borrowed)
        .unwrap_or_else(|| Cow::Owned(path.display().to_string()))
}

fn local_history_display_error(error: &str) -> Cow<'_, str> {
    local_history_display_label(
        Cow::Borrowed(error),
        LOCAL_HISTORY_ERROR_DISPLAY_MAX_CHARS,
        "unknown error",
    )
}

fn local_history_display_label<'a>(
    value: Cow<'a, str>,
    max_chars: usize,
    fallback: &str,
) -> Cow<'a, str> {
    let owned_label = {
        let raw = value.as_ref();
        match sanitized_display_label_cow(raw, max_chars, fallback) {
            Cow::Borrowed(label) => {
                let borrowed_original =
                    !raw.is_empty() && label.as_ptr() == raw.as_ptr() && label.len() == raw.len();
                if borrowed_original {
                    None
                } else {
                    Some(label.to_owned())
                }
            }
            Cow::Owned(label) => Some(label),
        }
    };

    match owned_label {
        Some(label) => Cow::Owned(label),
        None => value,
    }
}

pub(crate) fn local_history_loaded_text_block_reason(text: &str) -> Option<&'static str> {
    if text.contains('\0') {
        return Some("snapshot contains binary data");
    }
    if text.len() > usize::try_from(LOCAL_HISTORY_MAX_BYTES).unwrap_or(usize::MAX) {
        return Some("snapshot exceeds local history size limit");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        LOCAL_HISTORY_ERROR_DISPLAY_MAX_CHARS, LOCAL_HISTORY_MAX_BYTES,
        LOCAL_HISTORY_PATH_DISPLAY_MAX_CHARS, local_history_display_error,
        local_history_display_label, local_history_display_path, local_history_failed_status,
        local_history_loaded_status, local_history_loaded_text_block_reason,
        local_history_loading_status, local_history_revision_label,
        local_history_revision_path_label_for_sources,
    };
    use std::{
        borrow::Cow,
        path::{Path, PathBuf},
    };

    #[test]
    fn local_history_statuses_name_target_and_failures() {
        let path = Path::new("C:/repo/src/main.rs");
        let snapshot = Path::new("C:/repo/.kuroya/history/src/42.main.rs.bak");

        assert_eq!(
            local_history_revision_label(path),
            "main.rs (Local History)"
        );
        assert_eq!(
            local_history_loading_status(path),
            "Loading local history for main.rs"
        );
        assert_eq!(
            local_history_loaded_status(path, snapshot),
            "Opened local history for main.rs from 42.main.rs.bak"
        );
        assert_eq!(
            local_history_failed_status(path, "no snapshots"),
            "No local history snapshots for main.rs"
        );
        assert_eq!(
            local_history_failed_status(path, "denied"),
            "Could not open local history for main.rs: denied"
        );
    }

    #[test]
    fn local_history_loaded_text_rejects_binary_and_oversized_payloads() {
        assert_eq!(
            local_history_loaded_text_block_reason("old\0snapshot"),
            Some("snapshot contains binary data")
        );
        assert_eq!(
            local_history_loaded_text_block_reason(
                &"x".repeat(usize::try_from(LOCAL_HISTORY_MAX_BYTES).unwrap() + 1)
            ),
            Some("snapshot exceeds local history size limit")
        );
        assert_eq!(local_history_loaded_text_block_reason("old snapshot"), None);
    }

    #[test]
    fn local_history_revision_path_label_keeps_compact_label_when_unambiguous() {
        let path = Path::new("C:/repo/src/main.rs");
        let other = Path::new("C:/repo/src/lib.rs");

        assert!(matches!(
            local_history_revision_path_label_for_sources(path, "main.rs", [path, other]),
            Cow::Borrowed("main.rs")
        ));
    }

    #[test]
    fn local_history_revision_path_label_disambiguates_duplicate_file_names() {
        let path = PathBuf::from("C:/repo/src/main.rs");
        let other = PathBuf::from("C:/repo/tests/main.rs");
        let label = local_history_revision_path_label_for_sources(
            &path,
            "main.rs",
            [path.as_path(), other.as_path()],
        )
        .into_owned();

        assert_ne!(label, "main.rs");
        assert!(label.contains("src"));
        assert!(label.contains("main.rs"));
        assert!(label.contains(" #"));
        assert!(!has_control_chars(&label));
        assert!(!has_bidi_format_controls(&label));
        assert!(label.chars().count() <= LOCAL_HISTORY_PATH_DISPLAY_MAX_CHARS);
    }

    #[test]
    fn local_history_labels_and_statuses_sanitize_and_bound_fragments() {
        let long_name = format!("bad\n\u{202e}{}.rs", "x".repeat(200));
        let path = PathBuf::from("C:/repo/src").join(long_name);
        let snapshot_name = format!("1.snapshot\r\u{2066}{}.bak", "s".repeat(200));
        let snapshot = PathBuf::from("C:/repo/.kuroya/history/src").join(snapshot_name);

        let label = local_history_revision_label(&path);
        assert!(!has_control_chars(&label));
        assert!(!has_bidi_format_controls(&label));
        assert!(label.contains("..."));
        assert!(label.ends_with(" (Local History)"));
        assert!(
            label.chars().count()
                <= LOCAL_HISTORY_PATH_DISPLAY_MAX_CHARS + " (Local History)".len()
        );

        let status = local_history_loaded_status(&path, &snapshot);
        assert!(!has_control_chars(&status));
        assert!(!has_bidi_format_controls(&status));
        assert!(status.contains("bad "));
        assert!(status.contains("snapshot "));
        assert!(
            status.chars().count()
                <= "Opened local history for  from ".len()
                    + LOCAL_HISTORY_PATH_DISPLAY_MAX_CHARS * 2
        );
    }

    #[test]
    fn local_history_failure_status_sanitizes_bounded_error_text() {
        let error = format!("permission denied\n\u{202e}{}", "e".repeat(300));
        let status = local_history_failed_status(Path::new("main.rs"), &error);
        let prefix = "Could not open local history for main.rs: ";

        assert!(!has_control_chars(&status));
        assert!(!has_bidi_format_controls(&status));
        assert!(status.starts_with(prefix));
        assert!(status.contains("..."));
        assert!(status.chars().count() <= prefix.len() + LOCAL_HISTORY_ERROR_DISPLAY_MAX_CHARS);
        assert_eq!(
            local_history_failed_status(Path::new("main.rs"), ""),
            "Could not open local history for main.rs: unknown error"
        );
    }

    #[test]
    fn local_history_display_label_borrows_clean_ascii_and_unicode() {
        assert!(matches!(
            local_history_display_label(Cow::Borrowed("main.rs"), 32, "."),
            Cow::Borrowed("main.rs")
        ));

        let unicode = "clean-\u{03bb}.rs";
        match local_history_display_label(Cow::Borrowed(unicode), 32, ".") {
            Cow::Borrowed(label) => assert_eq!(label, unicode),
            Cow::Owned(label) => panic!("expected borrowed label, got {label:?}"),
        }
    }

    #[test]
    fn local_history_display_label_preserves_owned_clean_input() {
        let mut raw = String::with_capacity(64);
        raw.push_str("owned-clean.rs");
        let raw_ptr = raw.as_ptr();
        let raw_capacity = raw.capacity();

        match local_history_display_label(Cow::Owned(raw), 32, ".") {
            Cow::Owned(label) => {
                assert_eq!(label, "owned-clean.rs");
                assert_eq!(label.as_ptr(), raw_ptr);
                assert_eq!(label.capacity(), raw_capacity);
            }
            Cow::Borrowed(label) => panic!("expected owned label, got {label:?}"),
        }
    }

    #[test]
    fn local_history_display_label_owns_dirty_truncated_and_fallback_output() {
        let dirty =
            local_history_display_label(Cow::Owned("  bad\n\u{200b}name.rs  ".to_owned()), 64, ".");
        assert_eq!(dirty.as_ref(), "bad name.rs");
        assert!(matches!(dirty, Cow::Owned(_)));
        assert!(!has_control_chars(dirty.as_ref()));
        assert!(!dirty.contains('\u{200b}'));

        let truncated = local_history_display_label(
            Cow::Owned("abcdefghijklmnopqrstuvwxyz".to_owned()),
            12,
            ".",
        );
        assert_eq!(truncated.as_ref(), "abcd...vwxyz");
        assert!(matches!(truncated, Cow::Owned(_)));

        let fallback = local_history_display_label(
            Cow::Owned("\n\u{202e}".to_owned()),
            LOCAL_HISTORY_ERROR_DISPLAY_MAX_CHARS,
            "unknown error",
        );
        assert_eq!(fallback.as_ref(), "unknown error");
        assert!(matches!(fallback, Cow::Owned(_)));
    }

    #[test]
    fn local_history_display_path_and_error_statuses_preserve_fallbacks() {
        assert!(matches!(
            local_history_display_path(Path::new("C:/repo/src/main.rs")),
            Cow::Borrowed("main.rs")
        ));
        assert_eq!(
            local_history_loading_status(Path::new("")),
            "Loading local history for ."
        );
        assert_eq!(
            local_history_failed_status(Path::new("main.rs"), "no snapshots"),
            "No local history snapshots for main.rs"
        );
        assert_eq!(
            local_history_failed_status(Path::new("main.rs"), ""),
            "Could not open local history for main.rs: unknown error"
        );
    }

    #[test]
    fn local_history_display_fragments_reuse_simple_labels() {
        assert!(matches!(
            local_history_display_path(Path::new("C:/repo/src/main.rs")),
            Cow::Borrowed("main.rs")
        ));
        assert!(matches!(
            local_history_display_path(Path::new("")),
            Cow::Borrowed(".")
        ));
        assert!(matches!(
            local_history_display_path(Path::new("/")),
            Cow::Borrowed("/")
        ));
        assert!(matches!(
            local_history_display_error("permission denied"),
            Cow::Borrowed("permission denied")
        ));

        let hostile = PathBuf::from("C:/repo/src").join(format!("bad\n{}.rs", "x".repeat(200)));
        assert!(matches!(
            local_history_display_path(&hostile),
            Cow::Owned(_)
        ));
        assert!(matches!(
            local_history_display_error("\n\u{202e}"),
            Cow::Owned(_)
        ));
    }

    #[test]
    fn local_history_display_falls_back_for_blank_path_and_error() {
        assert_eq!(
            local_history_loading_status(Path::new("")),
            "Loading local history for ."
        );
        assert_eq!(
            local_history_failed_status(Path::new("main.rs"), "\n\u{202e}"),
            "Could not open local history for main.rs: unknown error"
        );
    }

    fn has_control_chars(value: &str) -> bool {
        value.chars().any(char::is_control)
    }

    fn has_bidi_format_controls(value: &str) -> bool {
        value.chars().any(|ch| {
            matches!(
                ch,
                '\u{061c}'
                    | '\u{200e}'
                    | '\u{200f}'
                    | '\u{202a}'..='\u{202e}'
                    | '\u{2066}'..='\u{2069}'
            )
        })
    }
}
