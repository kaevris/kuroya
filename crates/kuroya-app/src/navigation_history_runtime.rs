use crate::{
    KuroyaApp,
    file_runtime::file_path_open_buffer_or_known_openable,
    folding::remove_folds_hiding_line,
    history::{
        NavigationLocation, navigation_locations_coalesce, push_navigation_location,
        take_navigation_history_target,
    },
    lsp_text_positions::lsp_one_based_utf16_column_to_char_column,
    navigation_targets::{navigation_location_label, navigation_status_text},
    transient_state::FileJumpColumnEncoding,
    workspace_state::paths_match_lexically,
};
use kuroya_core::{BufferId, TextBuffer};
use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    path::{Component, Path, PathBuf},
};

impl KuroyaApp {
    pub(crate) fn current_navigation_location(&self) -> Option<NavigationLocation> {
        let buffer = self.active_buffer()?;
        let path = buffer.path()?.clone();
        let position = buffer.cursor_position();
        Some(NavigationLocation::new(
            path,
            position.line + 1,
            position.column + 1,
        ))
    }

    fn navigation_location_for_buffer(
        &self,
        id: BufferId,
        line: usize,
        column: usize,
    ) -> Option<NavigationLocation> {
        let buffer = self.buffer(id)?;
        let path = buffer.path()?.clone();
        let target = file_jump_target(buffer, line, column, FileJumpColumnEncoding::Char);
        Some(NavigationLocation::new(path, target.line, target.column))
    }

    pub(crate) fn navigation_location_for_active_char(
        &self,
        char_idx: usize,
    ) -> Option<NavigationLocation> {
        let buffer = self.active_buffer()?;
        let path = buffer.path()?.clone();
        let position = buffer.char_position(char_idx);
        Some(NavigationLocation::new(
            path,
            position.line + 1,
            position.column + 1,
        ))
    }

    pub(crate) fn record_navigation_origin(&mut self, target: &NavigationLocation) {
        let origin = self.current_navigation_location();
        if origin.as_ref() == Some(target) {
            return;
        }

        if let Some(origin) = origin {
            push_navigation_location(&mut self.navigation_back, origin);
        }
        self.navigation_forward.clear();
    }

    pub(crate) fn apply_file_jump(&mut self, id: BufferId, line: usize, column: usize) {
        self.apply_file_jump_with_encoding(id, line, column, FileJumpColumnEncoding::Char);
    }

    pub(crate) fn apply_file_jump_with_encoding(
        &mut self,
        id: BufferId,
        line: usize,
        column: usize,
        column_encoding: FileJumpColumnEncoding,
    ) {
        self.apply_file_jump_with_encoding_and_selection(id, line, column, column_encoding, None);
    }

    pub(crate) fn apply_file_jump_with_encoding_and_selection(
        &mut self,
        id: BufferId,
        line: usize,
        column: usize,
        column_encoding: FileJumpColumnEncoding,
        selection_length: Option<usize>,
    ) {
        let Some(target) = self
            .buffer(id)
            .map(|buffer| file_jump_target(buffer, line, column, column_encoding))
        else {
            return;
        };

        self.reveal_buffer_line(id, target.line);
        if let Some(buffer) = self.buffer_mut(id) {
            let selection_end = selection_length
                .filter(|length| *length > 0)
                .map(|length| {
                    let line_end =
                        buffer.line_column_to_char(target.line.saturating_sub(1), usize::MAX);
                    target.cursor.saturating_add(length).min(line_end)
                })
                .filter(|end| *end > target.cursor);
            if let Some(selection_end) = selection_end {
                buffer.set_selection(target.cursor, selection_end);
            } else {
                buffer.set_single_cursor(target.cursor);
            }
            self.pending_scroll_lines
                .insert(id, target.line.saturating_sub(1));
        }
    }

    pub(crate) fn apply_file_jump_with_history(
        &mut self,
        id: BufferId,
        line: usize,
        column: usize,
    ) {
        if let Some(target) = self.navigation_location_for_buffer(id, line, column) {
            self.record_navigation_origin(&target);
        }
        self.apply_file_jump(id, line, column);
    }

    pub(crate) fn navigate_history(&mut self, direction: isize) {
        let current = self.current_navigation_location();
        let destination_snapshot = navigation_history_destination_snapshot(
            &self.navigation_back,
            &self.navigation_forward,
            direction,
        );
        let mut openability_cache = NavigationHistoryOpenabilityCache::default();
        let mut skipped_unavailable_target = false;
        loop {
            let Some(target) = take_navigation_history_target(
                &mut self.navigation_back,
                &mut self.navigation_forward,
                current.clone(),
                direction,
            ) else {
                restore_navigation_history_destination(
                    &mut self.navigation_back,
                    &mut self.navigation_forward,
                    direction,
                    destination_snapshot,
                );
                self.status =
                    unavailable_navigation_history_status(direction, skipped_unavailable_target)
                        .to_owned();
                return;
            };

            if !self.navigation_history_target_is_openable(&target, &mut openability_cache) {
                skipped_unavailable_target = true;
                continue;
            }

            let target = resolve_navigation_history_target_for_open_buffers(&self.buffers, target);
            if navigation_history_target_matches_current(current.as_ref(), &target) {
                skipped_unavailable_target = true;
                continue;
            }
            let label = navigation_location_label(&target);
            let NavigationLocation { path, line, column } = target;
            self.open_file_at_with_history(path, line, column, false);
            let prefix = if direction < 0 {
                "Navigated back to "
            } else {
                "Navigated forward to "
            };
            self.status = navigation_history_status(prefix, &label);
            return;
        }
    }

    fn navigation_history_target_is_openable(
        &self,
        target: &NavigationLocation,
        openability_cache: &mut NavigationHistoryOpenabilityCache,
    ) -> bool {
        openability_cache.target_is_openable(
            &self.buffers,
            self.index.files(),
            &target.path,
            Path::exists,
        )
    }

    pub(crate) fn reveal_buffer_line(&mut self, id: BufferId, line: usize) -> bool {
        let Some(path) = self.buffer(id).and_then(|buffer| buffer.path()).cloned() else {
            return false;
        };
        let Some(folded) = self.folded_ranges.get_mut(&path) else {
            return false;
        };

        remove_folds_hiding_line(folded, line)
    }
}

#[derive(Debug, Default)]
struct NavigationHistoryOpenabilityCache {
    missing_paths: HashSet<PathBuf>,
}

impl NavigationHistoryOpenabilityCache {
    fn target_is_openable(
        &mut self,
        buffers: &[TextBuffer],
        indexed_files: &[PathBuf],
        path: &Path,
        path_exists: impl FnOnce(&Path) -> bool,
    ) -> bool {
        file_path_open_buffer_or_known_openable(buffers, indexed_files, path, |path| {
            let key = navigation_history_openability_cache_key(path);
            if self.missing_paths.contains(&key) {
                return false;
            }

            let openable = path_exists(path);
            if !openable {
                self.missing_paths.insert(key);
            }
            openable
        })
    }
}

fn navigation_history_openability_cache_key(path: &Path) -> PathBuf {
    let mut key = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                key.push(navigation_history_openability_component_key(
                    prefix.as_os_str(),
                ));
            }
            Component::RootDir => key.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => key.push(".."),
            Component::Normal(component) => {
                key.push(navigation_history_openability_component_key(component));
            }
        }
    }

    if key.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        key
    }
}

#[cfg(windows)]
fn navigation_history_openability_component_key(component: &OsStr) -> OsString {
    component.to_string_lossy().to_lowercase().into()
}

#[cfg(not(windows))]
fn navigation_history_openability_component_key(component: &OsStr) -> OsString {
    component.to_os_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileJumpTarget {
    line: usize,
    column: usize,
    cursor: usize,
}

fn file_jump_target(
    buffer: &TextBuffer,
    line: usize,
    column: usize,
    column_encoding: FileJumpColumnEncoding,
) -> FileJumpTarget {
    let target_column = file_jump_target_column(buffer, line, column, column_encoding);
    let cursor =
        buffer.line_column_to_char(line.saturating_sub(1), target_column.saturating_sub(1));
    let position = buffer.char_position(cursor);
    FileJumpTarget {
        line: position.line + 1,
        column: position.column + 1,
        cursor,
    }
}

fn resolve_navigation_history_target_for_open_buffers(
    buffers: &[TextBuffer],
    target: NavigationLocation,
) -> NavigationLocation {
    let Some(buffer) = open_buffer_for_navigation_history_target(buffers, &target.path) else {
        return target;
    };

    let resolved = file_jump_target(
        buffer,
        target.line,
        target.column,
        FileJumpColumnEncoding::Char,
    );
    NavigationLocation::new(target.path, resolved.line, resolved.column)
}

fn navigation_history_target_matches_current(
    current: Option<&NavigationLocation>,
    target: &NavigationLocation,
) -> bool {
    current.is_some_and(|current| navigation_locations_coalesce(current, target))
}

fn open_buffer_for_navigation_history_target<'a>(
    buffers: &'a [TextBuffer],
    path: &Path,
) -> Option<&'a TextBuffer> {
    let mut exact_match = None;
    let mut exact_matches = 0usize;
    let mut lexical_match = None;
    let mut lexical_matches = 0usize;

    for buffer in buffers {
        let Some(candidate) = buffer.path() else {
            continue;
        };
        if candidate == path {
            exact_matches = exact_matches.saturating_add(1);
            exact_match = Some(buffer);
            continue;
        }
        if paths_match_lexically(candidate, path) {
            lexical_matches = lexical_matches.saturating_add(1);
            lexical_match = Some(buffer);
        }
    }

    match exact_matches {
        0 => match lexical_matches {
            1 => lexical_match,
            _ => None,
        },
        1 => exact_match,
        _ => None,
    }
}

fn navigation_history_destination_snapshot(
    back: &std::collections::VecDeque<NavigationLocation>,
    forward: &std::collections::VecDeque<NavigationLocation>,
    direction: isize,
) -> std::collections::VecDeque<NavigationLocation> {
    if direction < 0 {
        forward.clone()
    } else {
        back.clone()
    }
}

fn restore_navigation_history_destination(
    back: &mut std::collections::VecDeque<NavigationLocation>,
    forward: &mut std::collections::VecDeque<NavigationLocation>,
    direction: isize,
    snapshot: std::collections::VecDeque<NavigationLocation>,
) {
    if direction < 0 {
        *forward = snapshot;
    } else {
        *back = snapshot;
    }
}

fn navigation_history_status(prefix: &str, label: &str) -> String {
    let mut status = String::with_capacity(prefix.len().saturating_add(label.len()));
    status.push_str(prefix);
    status.push_str(label);
    navigation_status_text(status)
}

fn unavailable_navigation_history_status(
    direction: isize,
    skipped_unavailable_target: bool,
) -> &'static str {
    match (direction < 0, skipped_unavailable_target) {
        (true, true) => "No previous available navigation location",
        (true, false) => "No previous navigation location",
        (false, true) => "No next available navigation location",
        (false, false) => "No next navigation location",
    }
}

pub(crate) fn file_jump_target_column(
    buffer: &TextBuffer,
    line: usize,
    column: usize,
    column_encoding: FileJumpColumnEncoding,
) -> usize {
    match column_encoding {
        FileJumpColumnEncoding::Char => column,
        FileJumpColumnEncoding::LspUtf16 => {
            lsp_one_based_utf16_column_to_char_column(buffer, line, column)
                .map(|char_column| char_column + 1)
                .unwrap_or(column)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FileJumpTarget, NavigationHistoryOpenabilityCache, file_jump_target,
        navigation_history_destination_snapshot, navigation_history_target_matches_current,
        resolve_navigation_history_target_for_open_buffers, restore_navigation_history_destination,
    };
    use crate::{
        history::{NavigationLocation, take_navigation_history_target},
        transient_state::FileJumpColumnEncoding,
    };
    use kuroya_core::TextBuffer;
    use std::{cell::Cell, collections::VecDeque, path::PathBuf};

    #[test]
    fn file_jump_target_reports_clamped_buffer_location() {
        let buffer = TextBuffer::from_text(7, None, "alpha\nbeta".to_owned());

        assert_eq!(
            file_jump_target(&buffer, 99, 99, FileJumpColumnEncoding::Char),
            FileJumpTarget {
                line: 2,
                column: 5,
                cursor: buffer.len_chars(),
            }
        );
        assert_eq!(
            file_jump_target(&buffer, 0, 0, FileJumpColumnEncoding::Char),
            FileJumpTarget {
                line: 1,
                column: 1,
                cursor: 0,
            }
        );
    }

    #[test]
    fn file_jump_target_reports_actual_utf16_jump_location() {
        let buffer = TextBuffer::from_text(7, None, "\u{1f600}alpha\nbeta".to_owned());

        assert_eq!(
            file_jump_target(&buffer, 1, 3, FileJumpColumnEncoding::LspUtf16),
            FileJumpTarget {
                line: 1,
                column: 2,
                cursor: 1,
            }
        );
    }

    #[test]
    fn open_buffer_history_target_is_clamped_and_uses_exact_path() {
        let path = PathBuf::from("workspace/src/main.rs");
        let equivalent_path = PathBuf::from("workspace/src/../src/main.rs");
        let buffers = vec![
            TextBuffer::from_text(7, Some(equivalent_path), "lexical\n".to_owned()),
            TextBuffer::from_text(9, Some(path.clone()), "alpha\nbeta".to_owned()),
        ];

        let target = resolve_navigation_history_target_for_open_buffers(
            &buffers,
            NavigationLocation::new(path.clone(), 99, 99),
        );

        assert_eq!(target, NavigationLocation::new(path, 2, 5));
    }

    #[test]
    fn open_buffer_history_target_accepts_lexical_path_match() {
        let stored_path = PathBuf::from("workspace/src/main.rs");
        let open_path = PathBuf::from("workspace/src/./main.rs");
        let buffers = vec![TextBuffer::from_text(
            7,
            Some(open_path),
            "alpha\nbeta".to_owned(),
        )];

        let target = resolve_navigation_history_target_for_open_buffers(
            &buffers,
            NavigationLocation::new(stored_path.clone(), 99, 99),
        );

        assert_eq!(target, NavigationLocation::new(stored_path, 2, 5));
    }

    #[test]
    fn open_buffer_history_target_preserves_raw_target_path_on_lexical_match() {
        let stored_path = PathBuf::from("workspace/src/../src/main.rs");
        let open_path = PathBuf::from("workspace/src/main.rs");
        let buffers = vec![TextBuffer::from_text(
            7,
            Some(open_path),
            "alpha\nbeta".to_owned(),
        )];

        let target = resolve_navigation_history_target_for_open_buffers(
            &buffers,
            NavigationLocation::new(stored_path.clone(), 99, 99),
        );

        assert_eq!(target, NavigationLocation::new(stored_path, 2, 5));
    }

    #[test]
    fn open_buffer_history_target_leaves_ambiguous_lexical_matches_unresolved() {
        let stored_path = PathBuf::from("workspace/src/main.rs");
        let first_open_path = PathBuf::from("workspace/src/./main.rs");
        let second_open_path = PathBuf::from("workspace/./src/main.rs");
        let buffers = vec![
            TextBuffer::from_text(7, Some(first_open_path), "short\n".to_owned()),
            TextBuffer::from_text(9, Some(second_open_path), "alpha\nbeta\ngamma\n".to_owned()),
        ];

        let target = resolve_navigation_history_target_for_open_buffers(
            &buffers,
            NavigationLocation::new(stored_path.clone(), 99, 99),
        );

        assert_eq!(target, NavigationLocation::new(stored_path, 99, 99));
    }

    #[test]
    fn stale_history_target_matches_current_after_lexical_resolution() {
        let current = NavigationLocation::new(PathBuf::from("workspace/src/main.rs"), 2, 4);
        let target = NavigationLocation::new(PathBuf::from("workspace/src/../src/main.rs"), 2, 1);

        assert!(navigation_history_target_matches_current(
            Some(&current),
            &target
        ));
        assert!(!navigation_history_target_matches_current(None, &target));
    }

    #[test]
    fn navigation_history_openability_cache_reuses_equivalent_missing_targets() {
        let mut cache = NavigationHistoryOpenabilityCache::default();
        let buffers: Vec<TextBuffer> = Vec::new();
        let indexed_files: Vec<PathBuf> = Vec::new();
        let probes = Cell::new(0);
        let missing = PathBuf::from("workspace/src/missing.rs");
        let equivalent_missing = PathBuf::from("workspace/src/./missing.rs");

        assert!(
            !cache.target_is_openable(&buffers, &indexed_files, &missing, |_| {
                probes.set(probes.get() + 1);
                false
            })
        );
        assert!(
            !cache.target_is_openable(&buffers, &indexed_files, &equivalent_missing, |_| {
                probes.set(probes.get() + 1);
                true
            })
        );

        assert_eq!(probes.get(), 1);
    }

    #[test]
    fn navigation_history_openability_cache_preserves_parent_traversal_misses() {
        let mut cache = NavigationHistoryOpenabilityCache::default();
        let buffers: Vec<TextBuffer> = Vec::new();
        let indexed_files: Vec<PathBuf> = Vec::new();
        let probes = Cell::new(0);
        let missing = PathBuf::from("workspace/src/missing.rs");
        let parent_traversal_missing = PathBuf::from("workspace/src/../src/missing.rs");

        assert!(
            !cache.target_is_openable(&buffers, &indexed_files, &missing, |_| {
                probes.set(probes.get() + 1);
                false
            })
        );
        assert!(!cache.target_is_openable(
            &buffers,
            &indexed_files,
            &parent_traversal_missing,
            |_| {
                probes.set(probes.get() + 1);
                false
            }
        ));

        assert_eq!(probes.get(), 2);
    }

    #[test]
    fn navigation_history_openability_cache_keeps_open_buffers_before_cached_misses() {
        let mut cache = NavigationHistoryOpenabilityCache::default();
        let empty_buffers: Vec<TextBuffer> = Vec::new();
        let indexed_files: Vec<PathBuf> = Vec::new();
        let missing = PathBuf::from("workspace/src/missing.rs");
        let equivalent_open = PathBuf::from("workspace/src/./missing.rs");

        assert!(!cache.target_is_openable(&empty_buffers, &indexed_files, &missing, |_| false));

        let buffers = vec![TextBuffer::from_text(
            7,
            Some(equivalent_open.clone()),
            "alpha\n".to_owned(),
        )];
        assert!(
            cache.target_is_openable(&buffers, &indexed_files, &equivalent_open, |_| {
                panic!("open buffer should short-circuit before cached miss")
            })
        );
    }

    #[test]
    fn navigation_history_openability_cache_keeps_indexed_files_before_cached_misses() {
        let mut cache = NavigationHistoryOpenabilityCache::default();
        let buffers: Vec<TextBuffer> = Vec::new();
        let empty_indexed_files: Vec<PathBuf> = Vec::new();
        let missing = PathBuf::from("workspace/src/missing.rs");
        let equivalent_indexed = PathBuf::from("workspace/src/./missing.rs");

        assert!(!cache.target_is_openable(&buffers, &empty_indexed_files, &missing, |_| false));

        let indexed_files = vec![equivalent_indexed.clone()];
        assert!(
            cache.target_is_openable(&buffers, &indexed_files, &equivalent_indexed, |_| {
                panic!("indexed file should short-circuit before cached miss")
            })
        );
    }

    #[test]
    fn failed_back_navigation_prunes_source_without_polluting_forward_history() {
        let current = NavigationLocation::new(PathBuf::from("workspace/src/current.rs"), 3, 1);
        let unavailable = NavigationLocation::new(PathBuf::from("workspace/src/missing.rs"), 4, 1);
        let existing_forward =
            NavigationLocation::new(PathBuf::from("workspace/src/../src/forward.rs"), 8, 2);
        let mut back = VecDeque::from([unavailable.clone()]);
        let mut forward = VecDeque::from([existing_forward.clone()]);
        let snapshot = navigation_history_destination_snapshot(&back, &forward, -1);

        assert_eq!(
            take_navigation_history_target(&mut back, &mut forward, Some(current), -1),
            Some(unavailable)
        );
        restore_navigation_history_destination(&mut back, &mut forward, -1, snapshot);

        assert!(back.is_empty());
        assert_eq!(forward, VecDeque::from([existing_forward]));
    }

    #[test]
    fn failed_forward_navigation_prunes_source_without_polluting_back_history() {
        let current = NavigationLocation::new(PathBuf::from("workspace/src/current.rs"), 3, 1);
        let unavailable = NavigationLocation::new(PathBuf::from("workspace/src/missing.rs"), 4, 1);
        let existing_back =
            NavigationLocation::new(PathBuf::from("workspace/src/../src/back.rs"), 8, 2);
        let mut back = VecDeque::from([existing_back.clone()]);
        let mut forward = VecDeque::from([unavailable.clone()]);
        let snapshot = navigation_history_destination_snapshot(&back, &forward, 1);

        assert_eq!(
            take_navigation_history_target(&mut back, &mut forward, Some(current), 1),
            Some(unavailable)
        );
        restore_navigation_history_destination(&mut back, &mut forward, 1, snapshot);

        assert_eq!(back, VecDeque::from([existing_back]));
        assert!(forward.is_empty());
    }
}
