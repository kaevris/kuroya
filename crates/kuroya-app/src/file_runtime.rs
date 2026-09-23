use crate::{
    KuroyaApp,
    file_io::read_text_file,
    history::NavigationLocation,
    image_preview::{image_preview_buffer_text, load_image_preview, path_is_image_preview},
    navigation_history_runtime::file_jump_target_column,
    path_display::display_path_label_cow,
    transient_state::{FileJump, FileJumpColumnEncoding},
    ui_events::UiEvent,
    workspace_state::{
        OpenFileRequest, lsp_event_path_is_current, path_set_contains_exact_or_lexically,
        paths_match_exact_or_lexically, paths_match_lexically,
    },
};
use kuroya_core::{BufferId, TextBuffer};
use std::{
    borrow::Cow,
    collections::HashSet,
    path::{Component, Path, PathBuf},
    time::Instant,
};

const STATUS_PATH_LABEL_MAX_CHARS: usize = 120;
const STATUS_PATH_LABEL_OMISSION: &str = "...";

impl KuroyaApp {
    pub(crate) fn spawn_open_file(&mut self, path: PathBuf) {
        self.spawn_open_file_with_activation(path, true);
    }

    pub(crate) fn open_file_at(&mut self, path: PathBuf, line: usize, column: usize) {
        self.open_file_at_with_history(path, line, column, true);
    }

    pub(crate) fn open_lsp_file_at(&mut self, path: PathBuf, line: usize, column: usize) -> bool {
        if !lsp_event_path_is_current(&self.workspace.root, &path) {
            self.status = rejected_lsp_navigation_status(&path);
            return false;
        }
        self.open_file_at_with_encoding(path, line, column, FileJumpColumnEncoding::LspUtf16, true);
        true
    }

    pub(crate) fn open_file_at_with_history(
        &mut self,
        path: PathBuf,
        line: usize,
        column: usize,
        record_history: bool,
    ) {
        self.open_file_at_with_encoding(
            path,
            line,
            column,
            FileJumpColumnEncoding::Char,
            record_history,
        );
    }

    pub(crate) fn open_file_at_known_openable(
        &mut self,
        path: PathBuf,
        line: usize,
        column: usize,
    ) {
        self.open_file_at_with_encoding_and_openability(
            path,
            line,
            column,
            FileJumpColumnEncoding::Char,
            true,
            true,
            None,
        );
    }

    pub(crate) fn open_file_selection_at_known_openable(
        &mut self,
        path: PathBuf,
        line: usize,
        column: usize,
        selection_length: usize,
    ) {
        self.open_file_at_with_encoding_and_openability(
            path,
            line,
            column,
            FileJumpColumnEncoding::Char,
            true,
            true,
            Some(selection_length),
        );
    }

    fn open_file_at_with_encoding(
        &mut self,
        path: PathBuf,
        line: usize,
        column: usize,
        column_encoding: FileJumpColumnEncoding,
        record_history: bool,
    ) {
        self.open_file_at_with_encoding_and_openability(
            path,
            line,
            column,
            column_encoding,
            record_history,
            false,
            None,
        );
    }

    fn open_file_at_with_encoding_and_openability(
        &mut self,
        path: PathBuf,
        line: usize,
        column: usize,
        column_encoding: FileJumpColumnEncoding,
        record_history: bool,
        target_known_openable: bool,
        selection_length: Option<usize>,
    ) {
        let mut target = classify_open_file_at_target(
            &path,
            &self.buffers,
            &self.pending_open_paths,
            line,
            column,
            column_encoding,
            record_history,
        );
        let runtime_known_openable = target.runtime_known_openable();
        let target_column = target.target_column(column);
        if record_history
            && (target_known_openable
                || open_file_jump_should_record_history(
                    runtime_known_openable,
                    self.index.files(),
                    &path,
                    path_is_openable_file,
                ))
        {
            let target_path = target.take_history_path().unwrap_or_else(|| path.clone());
            let target = NavigationLocation::new(target_path, line, target_column);
            self.record_navigation_origin(&target);
        }

        match target {
            OpenFileAtTarget::Open {
                id,
                status_path_label,
                target_column,
                ..
            } => {
                self.set_active_buffer(id);
                self.apply_file_jump_with_encoding_and_selection(
                    id,
                    line,
                    column,
                    column_encoding,
                    selection_length,
                );
                self.status = format!("Jumped to {status_path_label}:{line}:{target_column}");
            }
            OpenFileAtTarget::Pending => {
                let status = format!("Already opening {}", runtime_status_path_label_cow(&path));
                self.pending_active_path = Some(path.clone());
                self.pending_file_jump = Some(file_jump_for_path(
                    path,
                    line,
                    column,
                    column_encoding,
                    selection_length,
                ));
                self.status = status;
            }
            OpenFileAtTarget::Spawn => {
                self.pending_file_jump = Some(file_jump_for_path(
                    path.clone(),
                    line,
                    column,
                    column_encoding,
                    selection_length,
                ));
                self.spawn_file_load_task(path, true);
            }
        }
    }

    pub(crate) fn spawn_open_file_with_activation(&mut self, path: PathBuf, activate: bool) {
        match classify_open_file_request_for_runtime(&path, &self.buffers, &self.pending_open_paths)
        {
            OpenFileRequest::AlreadyOpen(id) => {
                if activate {
                    self.set_active_buffer(id);
                }
                return;
            }
            OpenFileRequest::AlreadyPending => {
                if activate {
                    self.pending_active_path = Some(path.clone());
                }
                self.status = format!("Already opening {}", runtime_status_path_label_cow(&path));
                return;
            }
            OpenFileRequest::Spawn => {}
        }

        self.spawn_file_load_task(path, activate);
    }

    fn spawn_file_load_task(&mut self, path: PathBuf, activate: bool) {
        self.pending_open_paths.insert(path.clone());
        let id = self.next_id();
        let root = self.workspace.root.clone();
        let generation = self.workspace_event_generation;
        let word_separators = self.settings.word_separators.clone();
        let path_label = bounded_status_path_label_cow(display_path_label_cow(&path));
        self.status = format!("Opening {path_label}");
        let tx = self.tx.clone();
        self.record_async_task_started("File Load", path_label.into_owned());
        self.runtime.spawn(async move {
            let started = Instant::now();
            if path_is_image_preview(&path) {
                match load_image_preview(&path).await {
                    Ok(preview) => {
                        let buffer = loaded_text_buffer(
                            id,
                            path.clone(),
                            image_preview_buffer_text(&preview),
                            word_separators,
                        );
                        let _ = crate::ui_event_channel::send_critical_ui_event(
                            &tx,
                            UiEvent::ImageFileLoaded {
                                root,
                                generation,
                                path,
                                buffer,
                                preview,
                                elapsed: started.elapsed(),
                                activate,
                            },
                        );
                    }
                    Err(error) => {
                        let _ = crate::ui_event_channel::send_critical_ui_event(
                            &tx,
                            UiEvent::FileLoadFailed {
                                root,
                                generation,
                                path,
                                error,
                            },
                        );
                    }
                }
                return;
            }
            match read_text_file(&path).await {
                Ok(decoded) => {
                    let buffer =
                        loaded_text_buffer(id, path.clone(), decoded.text, word_separators);
                    let _ = crate::ui_event_channel::send_critical_ui_event(
                        &tx,
                        UiEvent::FileLoaded {
                            root,
                            generation,
                            path,
                            buffer,
                            elapsed: started.elapsed(),
                            activate,
                            lossy: decoded.lossy,
                            binary: decoded.binary,
                        },
                    );
                }
                Err(error) => {
                    let _ = crate::ui_event_channel::send_critical_ui_event(
                        &tx,
                        UiEvent::FileLoadFailed {
                            root,
                            generation,
                            path,
                            error,
                        },
                    );
                }
            }
        });
    }
}

fn classify_open_file_request_for_runtime(
    path: &Path,
    buffers: &[TextBuffer],
    pending_open_paths: &HashSet<PathBuf>,
) -> OpenFileRequest {
    if let Some(open_buffer) = open_buffer_for_path(buffers, path) {
        OpenFileRequest::AlreadyOpen(open_buffer.buffer.id())
    } else if pending_open_path_matches(pending_open_paths, path) {
        OpenFileRequest::AlreadyPending
    } else {
        OpenFileRequest::Spawn
    }
}

enum OpenFileAtTarget {
    Open {
        id: BufferId,
        history_path: Option<PathBuf>,
        status_path_label: String,
        target_column: usize,
    },
    Pending,
    Spawn,
}

impl OpenFileAtTarget {
    fn runtime_known_openable(&self) -> bool {
        matches!(
            self,
            OpenFileAtTarget::Open { .. } | OpenFileAtTarget::Pending
        )
    }

    fn target_column(&self, fallback: usize) -> usize {
        match self {
            OpenFileAtTarget::Open { target_column, .. } => *target_column,
            OpenFileAtTarget::Pending | OpenFileAtTarget::Spawn => fallback,
        }
    }

    fn take_history_path(&mut self) -> Option<PathBuf> {
        match self {
            OpenFileAtTarget::Open { history_path, .. } => history_path.take(),
            OpenFileAtTarget::Pending | OpenFileAtTarget::Spawn => None,
        }
    }
}

fn classify_open_file_at_target(
    path: &Path,
    buffers: &[TextBuffer],
    pending_open_paths: &HashSet<PathBuf>,
    line: usize,
    column: usize,
    column_encoding: FileJumpColumnEncoding,
    record_history: bool,
) -> OpenFileAtTarget {
    if let Some(open_buffer) = open_buffer_for_path(buffers, path) {
        OpenFileAtTarget::Open {
            id: open_buffer.buffer.id(),
            history_path: record_history.then(|| open_buffer.path.clone()),
            status_path_label: runtime_status_path_label(open_buffer.path),
            target_column: file_jump_target_column(
                open_buffer.buffer,
                line,
                column,
                column_encoding,
            ),
        }
    } else if pending_open_path_matches(pending_open_paths, path) {
        OpenFileAtTarget::Pending
    } else {
        OpenFileAtTarget::Spawn
    }
}

fn pending_open_path_matches(pending_open_paths: &HashSet<PathBuf>, path: &Path) -> bool {
    path_set_contains_exact_or_lexically(pending_open_paths, path)
}

fn file_jump_for_path(
    path: PathBuf,
    line: usize,
    column: usize,
    column_encoding: FileJumpColumnEncoding,
    selection_length: Option<usize>,
) -> FileJump {
    match (column_encoding, selection_length) {
        (FileJumpColumnEncoding::Char, Some(selection_length)) => {
            FileJump::char_selection(path, line, column, selection_length)
        }
        (FileJumpColumnEncoding::Char, None) => FileJump::char(path, line, column),
        (FileJumpColumnEncoding::LspUtf16, _) => FileJump::lsp_utf16(path, line, column),
    }
}

pub(crate) fn loaded_text_buffer(
    id: kuroya_core::BufferId,
    path: PathBuf,
    text: String,
    word_separators: String,
) -> TextBuffer {
    let mut buffer = TextBuffer::from_text(id, Some(path), text);
    buffer.set_word_separators(word_separators);
    buffer
}

pub(crate) fn loaded_buffer_path_matches_request(buffer: &TextBuffer, path: &Path) -> bool {
    buffer
        .path()
        .is_some_and(|buffer_path| paths_match_exact_or_lexically(buffer_path, path))
}

fn open_file_jump_should_record_history(
    runtime_known_openable: bool,
    indexed_files: &[PathBuf],
    path: &Path,
    path_is_openable: impl FnOnce(&Path) -> bool,
) -> bool {
    runtime_known_openable || file_path_known_openable(indexed_files, path, path_is_openable)
}

pub(crate) fn file_path_known_openable(
    indexed_files: &[PathBuf],
    path: &Path,
    path_is_openable: impl FnOnce(&Path) -> bool,
) -> bool {
    indexed_file_path_matches(indexed_files, path) || path_is_openable(path)
}

pub(crate) fn file_path_open_buffer_or_known_openable(
    buffers: &[TextBuffer],
    indexed_files: &[PathBuf],
    path: &Path,
    path_is_openable: impl FnOnce(&Path) -> bool,
) -> bool {
    open_buffer_path_matches(buffers, path)
        || file_path_known_openable(indexed_files, path, path_is_openable)
}

fn path_is_openable_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
}

fn open_buffer_path_matches(buffers: &[TextBuffer], path: &Path) -> bool {
    open_buffer_for_path(buffers, path).is_some()
}

struct OpenBufferMatch<'a> {
    buffer: &'a TextBuffer,
    path: &'a PathBuf,
}

fn open_buffer_for_path<'a>(buffers: &'a [TextBuffer], path: &Path) -> Option<OpenBufferMatch<'a>> {
    let mut lexical_match = None;

    for buffer in buffers {
        let Some(candidate) = buffer.path() else {
            continue;
        };
        if candidate == path {
            return Some(OpenBufferMatch {
                buffer,
                path: candidate,
            });
        }
        if lexical_match.is_none() && paths_match_lexically(candidate, path) {
            lexical_match = Some(OpenBufferMatch {
                buffer,
                path: candidate,
            });
        }
    }

    lexical_match
}

fn indexed_file_path_matches(indexed_files: &[PathBuf], path: &Path) -> bool {
    if indexed_file_path_binary_search(indexed_files, path) {
        return true;
    }

    let path_needs_normalization = path_needs_lexical_normalization(path);
    if path_needs_normalization {
        let normalized = lexically_normalize_runtime_path(path);
        if normalized.as_path() != path
            && indexed_file_path_binary_search(indexed_files, normalized.as_path())
        {
            return true;
        }
    }

    #[cfg(not(windows))]
    if !path_needs_normalization {
        return false;
    }

    indexed_files
        .iter()
        .any(|candidate| paths_match_lexically(candidate, path))
}

fn indexed_file_path_binary_search(indexed_files: &[PathBuf], path: &Path) -> bool {
    indexed_files
        .binary_search_by(|candidate| candidate.as_path().cmp(path))
        .is_ok()
}

fn path_needs_lexical_normalization(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
}

fn lexically_normalize_runtime_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    let mut has_root = false;
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                normalized.push(prefix.as_os_str());
                has_root = false;
            }
            Component::RootDir => {
                normalized.push(component.as_os_str());
                has_root = true;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                let can_pop = matches!(
                    normalized.components().next_back(),
                    Some(Component::Normal(_))
                );
                if can_pop {
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

fn runtime_status_path_label(path: &Path) -> String {
    runtime_status_path_label_cow(path).into_owned()
}

fn runtime_status_path_label_cow(path: &Path) -> Cow<'_, str> {
    bounded_status_path_label_cow(display_path_label_cow(path))
}

fn bounded_status_path_label_cow(label: Cow<'_, str>) -> Cow<'_, str> {
    let label_chars = label.chars().count();
    if label_chars <= STATUS_PATH_LABEL_MAX_CHARS {
        return label;
    }

    let omission_chars = STATUS_PATH_LABEL_OMISSION.chars().count();
    let kept_chars = STATUS_PATH_LABEL_MAX_CHARS.saturating_sub(omission_chars);
    let prefix_chars = kept_chars / 2;
    let suffix_chars = kept_chars - prefix_chars;
    let label = label.as_ref();
    let prefix: String = label.chars().take(prefix_chars).collect();
    let suffix: String = label.chars().skip(label_chars - suffix_chars).collect();

    Cow::Owned(format!("{prefix}{STATUS_PATH_LABEL_OMISSION}{suffix}"))
}

fn rejected_lsp_navigation_status(path: &Path) -> String {
    format!(
        "Cannot open LSP location outside the workspace: {}",
        runtime_status_path_label_cow(path)
    )
}

#[cfg(test)]
mod tests {
    use super::{
        OpenFileAtTarget, STATUS_PATH_LABEL_MAX_CHARS, classify_open_file_at_target,
        classify_open_file_request_for_runtime, file_path_known_openable,
        file_path_open_buffer_or_known_openable, indexed_file_path_matches,
        lexically_normalize_runtime_path, loaded_text_buffer, open_buffer_for_path,
        open_file_jump_should_record_history, path_is_openable_file,
        path_needs_lexical_normalization, runtime_status_path_label,
    };
    use crate::navigation_history_runtime::file_jump_target_column;
    use crate::transient_state::FileJumpColumnEncoding;
    use crate::workspace_state::OpenFileRequest;
    use crate::{KuroyaApp, app_startup_context::AppStartupContext, terminal::TerminalPane};
    use kuroya_core::{EditorSettings, TextBuffer, Workspace};
    use std::{
        cell::Cell,
        collections::HashSet,
        fs,
        path::PathBuf,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };
    use tokio::runtime::Runtime;

    #[test]
    fn loaded_text_buffer_is_prebuilt_with_editor_word_separators() {
        let path = PathBuf::from("workspace/src/main.rs");
        let buffer = loaded_text_buffer(17, path.clone(), "alpha.beta".to_owned(), ".".to_owned());

        assert_eq!(buffer.id(), 17);
        assert_eq!(buffer.path(), Some(&path));
        assert_eq!(buffer.word_separators(), ".");
        assert_eq!(buffer.len_bytes(), "alpha.beta".len());
    }

    #[test]
    fn file_jump_target_column_converts_lsp_utf16_columns() {
        let buffer = loaded_text_buffer(
            17,
            PathBuf::from("workspace/src/main.rs"),
            "😀alpha".to_owned(),
            ".".to_owned(),
        );

        assert_eq!(
            file_jump_target_column(&buffer, 1, 3, FileJumpColumnEncoding::LspUtf16),
            2
        );
        assert_eq!(
            file_jump_target_column(&buffer, 1, 2, FileJumpColumnEncoding::LspUtf16),
            2
        );
        assert_eq!(
            file_jump_target_column(&buffer, 1, 3, FileJumpColumnEncoding::Char),
            3
        );
    }

    #[test]
    fn open_file_jump_history_uses_index_before_filesystem_probe() {
        let indexed = vec![
            PathBuf::from("workspace/src/lib.rs"),
            PathBuf::from("workspace/src/main.rs"),
        ];
        let probes = Cell::new(0usize);

        assert!(open_file_jump_should_record_history(
            false,
            &indexed,
            &PathBuf::from("workspace/src/main.rs"),
            |_| {
                probes.set(probes.get() + 1);
                false
            },
        ));

        assert_eq!(probes.get(), 0);
    }

    #[test]
    fn open_file_jump_history_uses_lexical_index_match_before_filesystem_probe() {
        let indexed = vec![PathBuf::from("workspace/src/main.rs")];
        let probes = Cell::new(0usize);

        assert!(open_file_jump_should_record_history(
            false,
            &indexed,
            &PathBuf::from("workspace/src/../src/main.rs"),
            |_| {
                probes.set(probes.get() + 1);
                false
            },
        ));

        assert_eq!(probes.get(), 0);
    }

    #[test]
    fn indexed_file_path_match_normalizes_equivalent_paths_before_scan_fallback() {
        let indexed = vec![
            PathBuf::from("workspace/src/lib.rs"),
            PathBuf::from("workspace/src/main.rs"),
        ];
        let equivalent_path = PathBuf::from("workspace/src/./nested/../main.rs");

        assert!(path_needs_lexical_normalization(&equivalent_path));
        assert_eq!(
            lexically_normalize_runtime_path(&equivalent_path),
            PathBuf::from("workspace/src/main.rs")
        );
        assert!(indexed_file_path_matches(&indexed, &equivalent_path));
    }

    #[test]
    fn known_openable_path_uses_index_before_filesystem_probe() {
        let indexed = vec![
            PathBuf::from("workspace/src/lib.rs"),
            PathBuf::from("workspace/src/main.rs"),
        ];
        let probes = Cell::new(0usize);

        assert!(file_path_known_openable(
            &indexed,
            &PathBuf::from("workspace/src/main.rs"),
            |_| {
                probes.set(probes.get() + 1);
                false
            },
        ));

        assert_eq!(probes.get(), 0);
    }

    #[test]
    fn open_buffer_or_known_openable_path_uses_exact_open_buffer_before_filesystem_probe() {
        let path = PathBuf::from("workspace/src/main.rs");
        let buffers = vec![TextBuffer::from_text(
            7,
            Some(path.clone()),
            "open\n".to_owned(),
        )];
        let probes = Cell::new(0usize);

        assert!(file_path_open_buffer_or_known_openable(
            &buffers,
            &[],
            &path,
            |_| {
                probes.set(probes.get() + 1);
                false
            },
        ));

        assert_eq!(probes.get(), 0);
    }

    #[test]
    fn open_buffer_or_known_openable_path_uses_lexical_open_buffer_before_filesystem_probe() {
        let path = PathBuf::from("workspace/src/main.rs");
        let equivalent_path = PathBuf::from("workspace/src/../src/main.rs");
        let buffers = vec![TextBuffer::from_text(7, Some(path), "open\n".to_owned())];
        let probes = Cell::new(0usize);

        assert!(file_path_open_buffer_or_known_openable(
            &buffers,
            &[],
            &equivalent_path,
            |_| {
                probes.set(probes.get() + 1);
                false
            },
        ));

        assert_eq!(probes.get(), 0);
    }

    #[test]
    fn open_buffer_for_path_prefers_exact_path_over_earlier_lexical_match() {
        let path = PathBuf::from("workspace/src/main.rs");
        let equivalent_path = PathBuf::from("workspace/src/../src/main.rs");
        let buffers = vec![
            TextBuffer::from_text(7, Some(equivalent_path), "lexical\n".to_owned()),
            TextBuffer::from_text(9, Some(path.clone()), "exact\n".to_owned()),
        ];

        assert_eq!(
            open_buffer_for_path(&buffers, &path).map(|target| target.buffer.id()),
            Some(9)
        );
    }

    #[test]
    fn runtime_open_classification_prefers_exact_buffer_over_earlier_lexical_match() {
        let path = PathBuf::from("workspace/src/main.rs");
        let equivalent_path = PathBuf::from("workspace/src/../src/main.rs");
        let buffers = vec![
            TextBuffer::from_text(7, Some(equivalent_path), "lexical\n".to_owned()),
            TextBuffer::from_text(9, Some(path.clone()), "exact\n".to_owned()),
        ];

        assert_eq!(
            classify_open_file_request_for_runtime(&path, &buffers, &HashSet::new()),
            OpenFileRequest::AlreadyOpen(9)
        );
    }

    #[test]
    fn open_file_at_target_reuses_pending_equivalent_path_without_spawn_classification() {
        let path = PathBuf::from("workspace/src/main.rs");
        let equivalent_path = PathBuf::from("workspace/src/../src/main.rs");
        let pending_open_paths = HashSet::from([path]);

        let target = classify_open_file_at_target(
            &equivalent_path,
            &[],
            &pending_open_paths,
            9,
            3,
            FileJumpColumnEncoding::Char,
            true,
        );

        assert!(matches!(target, OpenFileAtTarget::Pending));
        assert!(target.runtime_known_openable());
        assert_eq!(target.target_column(3), 3);
    }

    #[test]
    fn known_openable_path_falls_back_to_filesystem_for_unknown_paths() {
        let root = temp_root("known-openable-file");
        let path = root.join("generated.rs");
        fs::create_dir_all(&root).unwrap();
        fs::write(&path, "generated\n").unwrap();
        let indexed = vec![PathBuf::from("workspace/src/main.rs")];
        let probes = Cell::new(0usize);

        assert!(file_path_known_openable(&indexed, &path, |path| {
            probes.set(probes.get() + 1);
            path_is_openable_file(path)
        }));
        assert_eq!(probes.get(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn known_openable_path_rejects_existing_directories() {
        let root = temp_root("known-openable-directory");
        let path = root.join("generated.rs");
        fs::create_dir_all(&path).unwrap();

        assert!(!file_path_known_openable(&[], &path, path_is_openable_file));
        assert!(!open_file_jump_should_record_history(
            false,
            &[],
            &path,
            path_is_openable_file,
        ));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn open_file_jump_history_falls_back_to_filesystem_for_unknown_paths() {
        let root = temp_root("open-history-file");
        let path = root.join("generated.rs");
        fs::create_dir_all(&root).unwrap();
        fs::write(&path, "generated\n").unwrap();
        let indexed = vec![PathBuf::from("workspace/src/main.rs")];
        let probes = Cell::new(0usize);

        assert!(open_file_jump_should_record_history(
            false,
            &indexed,
            &path,
            |path| {
                probes.set(probes.get() + 1);
                path_is_openable_file(path)
            },
        ));
        assert_eq!(probes.get(), 1);

        assert!(open_file_jump_should_record_history(
            true,
            &indexed,
            &path,
            |_| {
                probes.set(probes.get() + 1);
                false
            },
        ));
        assert_eq!(probes.get(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_status_path_label_leaves_short_labels_unchanged() {
        let path = PathBuf::from("workspace/src/main.rs");

        assert_eq!(runtime_status_path_label(&path), "main.rs");
    }

    #[test]
    fn runtime_status_path_label_bounds_long_labels() {
        let path = PathBuf::from("workspace/src").join(format!("{}.rs", "a".repeat(180)));
        let label = runtime_status_path_label(&path);

        assert_eq!(label.chars().count(), STATUS_PATH_LABEL_MAX_CHARS);
        assert!(label.starts_with("aaaa"));
        assert!(label.contains("..."));
        assert!(label.ends_with(".rs"));
    }

    #[test]
    fn open_file_at_pending_equivalent_path_reuses_pending_open_without_spawning_load() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let equivalent_path = root.join("src").join("..").join("src/main.rs");
        let mut app = app_for_test(root);
        app.pending_open_paths.insert(path);

        app.open_file_at_with_history(equivalent_path.clone(), 9, 3, true);

        assert!(app.active.is_none());
        assert!(app.active_async_tasks.is_empty());
        assert_eq!(app.pending_active_path, Some(equivalent_path.clone()));
        let jump = app
            .pending_file_jump
            .as_ref()
            .expect("pending jump should be preserved for pending open");
        assert_eq!(jump.path, equivalent_path);
        assert_eq!(jump.line, 9);
        assert_eq!(jump.column, 3);
        assert_eq!(jump.column_encoding, FileJumpColumnEncoding::Char);
        assert_eq!(app.status, "Already opening main.rs");
    }

    #[test]
    fn open_lsp_file_at_reuses_lexically_equivalent_open_buffer() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let equivalent_path = root.join("src").join("..").join("src/main.rs");
        let mut app = app_for_test(root);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(path.clone()),
            "fn main() {}\n".to_owned(),
        ));

        assert!(app.open_lsp_file_at(equivalent_path, 1, 1));

        assert_eq!(app.active, Some(7));
        assert!(app.pending_open_paths.is_empty());
        assert!(app.pending_file_jump.is_none());
        assert_eq!(app.buffer(7).and_then(TextBuffer::path), Some(&path));
        assert_eq!(app.status, "Jumped to main.rs:1:1");
    }

    #[test]
    fn open_lsp_file_at_existing_buffer_uses_buffer_for_utf16_column() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let equivalent_path = root.join("src").join("..").join("src/main.rs");
        let mut app = app_for_test(root);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(path.clone()),
            "\u{1f600}alpha\n".to_owned(),
        ));

        assert!(app.open_lsp_file_at(equivalent_path, 1, 3));

        assert_eq!(app.active, Some(7));
        assert!(app.pending_open_paths.is_empty());
        assert!(app.pending_file_jump.is_none());
        assert_eq!(app.status, "Jumped to main.rs:1:2");
    }

    #[test]
    fn open_lsp_file_at_prefers_exact_buffer_over_earlier_lexical_buffer() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let equivalent_path = root.join("src").join("..").join("src/main.rs");
        let mut app = app_for_test(root);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(equivalent_path),
            "\u{1f600}alpha\n".to_owned(),
        ));
        app.buffers.push(TextBuffer::from_text(
            9,
            Some(path.clone()),
            "alpha\n".to_owned(),
        ));

        assert!(app.open_lsp_file_at(path, 1, 3));

        assert_eq!(app.active, Some(9));
        assert!(app.pending_open_paths.is_empty());
        assert!(app.pending_file_jump.is_none());
        assert_eq!(app.status, "Jumped to main.rs:1:3");
    }

    #[test]
    fn open_lsp_file_at_rejects_paths_outside_workspace() {
        let root = PathBuf::from("workspace");
        let outside = PathBuf::from("outside/main.rs");
        let mut app = app_for_test(root);

        assert!(!app.open_lsp_file_at(outside, 1, 1));

        assert!(app.active.is_none());
        assert!(app.pending_open_paths.is_empty());
        assert!(app.pending_file_jump.is_none());
        assert!(app.active_async_tasks.is_empty());
        assert!(app.async_task_trace.is_empty());
        assert_eq!(
            app.status,
            "Cannot open LSP location outside the workspace: main.rs"
        );
    }

    #[test]
    fn open_lsp_file_at_rejects_lexical_parent_escape() {
        let root = PathBuf::from("workspace");
        let escaped = root
            .join("src")
            .join("..")
            .join("..")
            .join("outside/main.rs");
        let mut app = app_for_test(root);

        assert!(!app.open_lsp_file_at(escaped, 1, 1));

        assert!(app.active.is_none());
        assert!(app.pending_open_paths.is_empty());
        assert!(app.pending_file_jump.is_none());
        assert!(app.active_async_tasks.is_empty());
        assert!(app.async_task_trace.is_empty());
        assert_eq!(
            app.status,
            "Cannot open LSP location outside the workspace: main.rs"
        );
    }

    fn app_for_test(root: PathBuf) -> KuroyaApp {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let settings = EditorSettings::default();
        KuroyaApp::from_startup_context(AppStartupContext {
            runtime: Runtime::new().expect("test runtime"),
            tx,
            rx,
            workspace: Workspace::new(root.clone()),
            settings: settings.clone(),
            settings_panel_draft: settings,
            settings_editor_font_path: String::new(),
            settings_ui_font_path: String::new(),
            theme_picker_selected: 0,
            saved_session: None,
            terminal: TerminalPane::new(root.clone(), 100, 12.0, 1.2),
            watcher: None,
            recent_projects: Vec::new(),
            trusted_workspaces: vec![root],
            now: Instant::now(),
            startup_timings: Vec::new(),
        })
    }

    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "kuroya-file-runtime-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
