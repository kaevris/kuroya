use crate::{
    KuroyaApp,
    editor_pane_scroll::{
        clear_editor_horizontal_scroll_offsets_for_buffer,
        clear_editor_inertial_scrolls_for_buffer, clear_editor_middle_click_scroll_for_buffer,
        clear_editor_scroll_state_for_buffer,
    },
    file_runtime::file_path_open_buffer_or_known_openable,
    history::{closed_file_entry_for_buffer, push_closed_file_entry},
    path_display::{compact_path, sanitized_display_label_cow},
};
use kuroya_core::BufferId;
use std::{
    borrow::Cow,
    collections::HashSet,
    ffi::{OsStr, OsString},
    path::{Component, Path, PathBuf},
};

mod pending;

const BUFFER_CLOSE_STATUS_LABEL_MAX_CHARS: usize = 96;

impl KuroyaApp {
    pub(crate) fn reopen_closed_file(&mut self) {
        let mut openability_cache = ClosedFileOpenabilityCache::default();
        while let Some(entry) = self.closed_files.pop_back() {
            if !openability_cache.target_openable(
                &self.buffers,
                self.index.files(),
                &entry.path,
                Path::exists,
            ) {
                continue;
            }
            let label = buffer_close_path_label(&entry.path);
            self.open_file_at_known_openable(entry.path, entry.line, entry.column);
            self.status = format!("Reopened {label}:{}:{}", entry.line, entry.column);
            return;
        }

        self.status = "No closed files to reopen".to_owned();
    }

    pub(crate) fn request_close_buffer(&mut self, id: BufferId) {
        let dirty_label = match self.buffer(id) {
            Some(buffer) if buffer.is_dirty() => {
                Some(buffer_close_status_label(&self.buffer_label_for(buffer)))
            }
            Some(_) => None,
            None => return,
        };

        if let Some(label) = dirty_label {
            self.set_active_buffer(id);
            self.dirty_close_buffer = Some(id);
            self.status = format!("Unsaved changes in {label}");
            return;
        }

        self.force_close_buffer(id);
    }

    pub(crate) fn force_close_buffer(&mut self, id: BufferId) {
        self.clear_force_close_transients(id);
        let Some(position) = self.buffers.iter().position(|buffer| buffer.id() == id) else {
            return;
        };
        self.force_close_buffer_at_position_after_transient_clear(position);
    }

    fn force_close_buffer_at_position(&mut self, position: usize) {
        let Some(id) = self.buffers.get(position).map(|buffer| buffer.id()) else {
            return;
        };
        self.clear_force_close_transients(id);
        self.force_close_buffer_at_position_after_transient_clear(position);
    }

    fn force_close_buffer_at_position_after_transient_clear(&mut self, position: usize) {
        let id = self.buffers[position].id();
        let diagnostic_path = self.diagnostic_path_for(&self.buffers[position]);
        let label = buffer_close_status_label(&self.buffer_label_for(&self.buffers[position]));
        if let Some(entry) = closed_file_entry_for_buffer(&self.buffers[position]) {
            push_closed_file_entry(&mut self.closed_files, entry);
        }
        self.notify_lsp_close(id);
        self.diagnostics.replace(diagnostic_path, Vec::new());
        if let Some(path) = self.buffers[position].path().cloned() {
            self.clear_buffer_path_state_for_path(&path);
        }
        self.buffers.remove(position);
        crate::editor_vim_key_events::vim_forget_marks_for_buffer(id);
        crate::editor_vim_key_events::vim_forget_searches_for_buffer(id);
        self.editor_vim_last_change.remove(&id);
        if self.editor_vim_insert_undo_group_buffer == Some(id) {
            self.editor_vim_insert_undo_group_buffer = None;
        }
        self.virtual_buffer_labels.remove(&id);
        self.diff_buffer_sources.remove(&id);
        self.diff_cache.remove(&id);
        self.diff_cache_pending.retain(|key, _| key.buffer_id != id);
        self.buffer_find_cache.clear_for_buffer(id);
        self.editor_bracket_overlay_cache.clear_for_buffer(id);
        self.editor_row_render_cache.clear_for_buffer(id);
        self.minimap_line_length_cache.clear_for_buffer(id);
        self.minimap_section_header_cache.clear_for_buffer(id);
        self.syntax_tree_cache.clear_for_buffer(id);
        self.line_render_protection_cache.remove(&id);
        self.clear_buffer_merge_conflict_cache(id);
        self.clear_buffer_changed_on_disk(id);
        self.lossy_decoded_buffers.remove(&id);
        self.binary_preview_buffers.remove(&id);
        self.image_preview_buffers.remove(&id);
        self.manual_read_only_buffers.remove(&id);
        self.pending_scroll_lines.remove(&id);
        self.pending_horizontal_scroll_offsets.remove(&id);
        self.pending_pane_scroll_lines
            .retain(|(_, buffer_id), _| *buffer_id != id);
        self.pending_pane_horizontal_scroll_offsets
            .retain(|(_, buffer_id), _| *buffer_id != id);
        clear_editor_scroll_state_for_buffer(
            &mut self.editor_scroll_offsets,
            &mut self.editor_scroll_targets,
            id,
        );
        clear_editor_horizontal_scroll_offsets_for_buffer(
            &mut self.editor_horizontal_scroll_offsets,
            id,
        );
        clear_editor_inertial_scrolls_for_buffer(&mut self.editor_inertial_scrolls, id);
        clear_editor_middle_click_scroll_for_buffer(&mut self.editor_middle_click_scroll, id);
        self.clear_editor_selection_drag_for_buffer(id);
        self.pending_lsp_symbol_refreshes.remove(&id);
        self.pending_language_sync.remove(&id);
        self.clear_static_diagnostics_request(id);
        self.pending_completion_requests.remove(&id);
        self.pending_signature_help_requests.remove(&id);
        self.pending_format_on_type_requests.remove(&id);
        self.clear_pending_lsp_hover_for_buffer(id);

        let replacement = if self.buffers.is_empty() {
            None
        } else {
            Some(self.buffers[position.min(self.buffers.len() - 1)].id())
        };

        for pane in &mut self.panes {
            if pane.active == Some(id) {
                pane.active = replacement;
            }
        }

        if self.active == Some(id) {
            self.active = self
                .panes
                .iter()
                .find(|pane| pane.id == self.active_pane)
                .and_then(|pane| pane.active)
                .or(replacement);
        }
        if let Some(active) = self.active {
            self.clear_completion_popup_for_inactive_buffer(active);
        } else {
            self.clear_completion_popup_state();
        }

        self.status = format!("Closed {label}");
    }

    fn clear_force_close_transients(&mut self, id: BufferId) {
        if self.close_after_save == Some(id) {
            self.close_after_save = None;
            self.pending_close_buffers.clear();
        } else if !self.pending_close_buffers.is_empty() {
            self.pending_close_buffers.retain(|pending| *pending != id);
        }
        if self.dirty_close_buffer == Some(id) {
            self.dirty_close_buffer = None;
        }
        if self.dirty_reload_buffer == Some(id) {
            self.dirty_reload_buffer = None;
        }
        if self.save_conflict_buffer == Some(id) {
            self.save_conflict_buffer = None;
        }
        if self.save_as_buffer == Some(id) {
            self.save_as_buffer = None;
            self.save_as_open = false;
            self.save_as_path.clear();
        }
        self.in_flight_saves.remove(&id);
        self.clear_deferred_save_work(id);
        self.cancel_deferred_reload_work(id);
    }
}

#[derive(Debug, Default)]
struct ClosedFileOpenabilityCache {
    missing_paths: HashSet<PathBuf>,
}

impl ClosedFileOpenabilityCache {
    fn target_openable(
        &mut self,
        buffers: &[kuroya_core::TextBuffer],
        indexed_files: &[PathBuf],
        path: &Path,
        path_exists: impl FnOnce(&Path) -> bool,
    ) -> bool {
        closed_file_target_openable(buffers, indexed_files, path, |path| {
            let key = closed_file_openability_cache_key(path);
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

fn closed_file_target_openable(
    buffers: &[kuroya_core::TextBuffer],
    indexed_files: &[PathBuf],
    path: &Path,
    path_exists: impl FnOnce(&Path) -> bool,
) -> bool {
    file_path_open_buffer_or_known_openable(buffers, indexed_files, path, path_exists)
}

fn closed_file_openability_cache_key(path: &Path) -> PathBuf {
    let mut key = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                key.push(closed_file_openability_component_key(prefix.as_os_str()));
            }
            Component::RootDir => key.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => key.push(".."),
            Component::Normal(component) => {
                key.push(closed_file_openability_component_key(component));
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
fn closed_file_openability_component_key(component: &OsStr) -> OsString {
    component.to_string_lossy().to_lowercase().into()
}

#[cfg(not(windows))]
fn closed_file_openability_component_key(component: &OsStr) -> OsString {
    component.to_os_string()
}

fn buffer_close_path_label(path: &Path) -> String {
    buffer_close_status_label(&compact_path(path))
}

pub(crate) fn buffer_close_status_label(label: &str) -> String {
    buffer_close_status_label_cow(label).into_owned()
}

fn buffer_close_status_label_cow(label: &str) -> Cow<'_, str> {
    sanitized_display_label_cow(label, BUFFER_CLOSE_STATUS_LABEL_MAX_CHARS, "Untitled")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app_startup_context::AppStartupContext,
        app_state::{PendingFileReload, PendingFormatOnSave, QueuedFileReload},
        folding::FoldedRange,
        large_file_mode::buffer_needs_line_render_protection_cached,
        lsp_hover_cache::{LspHoverCacheKey, lookup_hover_cache, store_hover_cache},
        terminal::TerminalPane,
        transient_state::{EditorInertialScroll, LspHoverPopup, LspSignatureHelpPopup},
    };
    use kuroya_core::{
        EditorMatchBrackets, EditorSettings, GitBlameLine, GitChangeStage, GitDiffHunk,
        LspDocumentHighlight, LspFoldingRange, LspSignatureHelp, LspTextEdit, TextBuffer,
        Workspace,
    };
    use std::{
        cell::Cell,
        path::PathBuf,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };
    use tokio::runtime::Runtime;

    #[test]
    fn force_close_buffer_purges_vim_marks_for_the_closed_buffer() {
        use crate::editor_vim_key_events::{
            EditorVimMode, EditorVimPendingKey, vim_marks_are_empty_for_test,
        };
        use eframe::egui::{Key, Modifiers};

        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        app.buffers
            .push(TextBuffer::from_text(7, None, "alpha\nbeta\n".to_owned()));

        let mut mode = EditorVimMode::Normal;
        let mut pending: Option<EditorVimPendingKey> = None;
        let mut last_char_find = None;
        let mut unnamed_register = None;
        let mut last_change = None;
        let settings = EditorSettings::default();
        for (key, modifiers) in [(Key::M, Modifiers::NONE), (Key::A, Modifiers::NONE)] {
            let buffer = app.buffers.get_mut(0).expect("test buffer");
            crate::editor_vim_key_events::handle_vim_editor_key_event_with_settings_and_indent(
                buffer,
                key,
                modifiers,
                &mut mode,
                &mut pending,
                &mut last_char_find,
                &mut unnamed_register,
                &mut last_change,
                &settings.vim,
                "    ",
            );
        }
        assert!(!vim_marks_are_empty_for_test());

        app.force_close_buffer(7);

        assert!(vim_marks_are_empty_for_test());
    }

    #[test]
    fn force_close_buffer_purges_vim_searches_and_last_change_for_the_closed_buffer() {
        use crate::editor_vim_key_events::{
            EditorVimLastChange, EditorVimRepeatAction, vim_last_search_word_for_test,
        };

        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        let closed = TextBuffer::from_text(7, None, "alpha beta".to_owned());
        let kept = TextBuffer::from_text(8, None, "gamma".to_owned());
        app.buffers.push(closed);
        app.buffers.push(kept);
        {
            let closed = app.buffer(7).expect("closed buffer");
            crate::editor_vim_key_events::vim_set_last_search_for_test(
                closed, "alpha", true, false,
            );
        }
        {
            let kept = app.buffer(8).expect("kept buffer");
            crate::editor_vim_key_events::vim_set_last_search_for_test(kept, "gamma", true, false);
        }
        app.editor_vim_last_change.insert(
            7,
            EditorVimLastChange {
                action: EditorVimRepeatAction::DeleteForwardChars,
                count: 1,
                insert_replay: Vec::new(),
            },
        );
        app.editor_vim_last_change.insert(
            8,
            EditorVimLastChange {
                action: EditorVimRepeatAction::DeleteForwardChars,
                count: 1,
                insert_replay: Vec::new(),
            },
        );
        app.editor_vim_insert_undo_group_buffer = Some(7);

        app.force_close_buffer(7);

        assert_eq!(vim_last_search_word_for_test(7), None);
        assert_eq!(vim_last_search_word_for_test(8), Some("gamma".to_owned()));
        assert!(!app.editor_vim_last_change.contains_key(&7));
        assert!(app.editor_vim_last_change.contains_key(&8));
        assert_eq!(app.editor_vim_insert_undo_group_buffer, None);
    }

    #[test]
    fn request_close_buffer_sanitizes_dirty_status_label() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text(7, None, "dirty".to_owned());
        buffer.mark_dirty();
        app.buffers.push(buffer);
        let raw_label = format!(
            "alpha\n{}\u{202e}omega.rs",
            "very-long-component-".repeat(12)
        );
        app.virtual_buffer_labels.insert(7, raw_label.clone());

        app.request_close_buffer(7);

        assert!(app.status.starts_with("Unsaved changes in alpha "));
        assert!(!app.status.contains('\n'));
        assert!(!app.status.contains('\u{202e}'));
        assert!(
            app.status.chars().count()
                <= "Unsaved changes in ".chars().count() + BUFFER_CLOSE_STATUS_LABEL_MAX_CHARS
        );
        assert_eq!(app.dirty_close_buffer, Some(7));
        assert!(app.buffer(7).is_some());
        assert_eq!(app.virtual_buffer_labels.get(&7), Some(&raw_label));
    }

    #[test]
    fn pending_close_sanitizes_dirty_status_label() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text(7, None, "dirty".to_owned());
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.virtual_buffer_labels.insert(
            7,
            format!(
                "queued\n{}\u{202e}tail.rs",
                "very-long-component-".repeat(12)
            ),
        );
        app.pending_close_buffers.push(7);

        app.begin_next_pending_close();

        assert!(app.status.starts_with("Unsaved changes in queued "));
        assert!(!app.status.contains('\n'));
        assert!(!app.status.contains('\u{202e}'));
        assert!(
            app.status.chars().count()
                <= "Unsaved changes in ".chars().count() + BUFFER_CLOSE_STATUS_LABEL_MAX_CHARS
        );
        assert_eq!(app.dirty_close_buffer, Some(7));
        assert!(app.pending_close_buffers.is_empty());
    }

    #[test]
    fn force_close_buffer_sanitizes_closed_status_label_only() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        app.buffers
            .push(TextBuffer::from_text(7, None, "closed".to_owned()));
        let raw_label = format!("line\n{}\u{2066}.rs", "unsafe-label-".repeat(12));
        app.virtual_buffer_labels.insert(7, raw_label.clone());

        app.force_close_buffer(7);

        assert!(app.status.starts_with("Closed line "));
        assert!(!app.status.contains('\n'));
        assert!(!app.status.contains('\u{2066}'));
        assert!(
            app.status.chars().count()
                <= "Closed ".chars().count() + BUFFER_CLOSE_STATUS_LABEL_MAX_CHARS
        );
        assert!(app.virtual_buffer_labels.is_empty());
        assert!(!app.status.contains(&raw_label));
    }

    #[test]
    fn buffer_close_status_label_falls_back_for_blank_control_labels() {
        assert_eq!(buffer_close_status_label("\n\u{202e}\u{0007}"), "Untitled");
    }

    #[test]
    fn buffer_close_status_label_cow_borrows_clean_ascii_and_unicode() {
        assert!(matches!(
            buffer_close_status_label_cow("clean.rs"),
            Cow::Borrowed("clean.rs")
        ));

        let unicode = "clean-\u{03bb}.rs";
        match buffer_close_status_label_cow(unicode) {
            Cow::Borrowed(label) => assert_eq!(label, unicode),
            Cow::Owned(label) => panic!("expected borrowed label, got {label:?}"),
        }
    }

    #[test]
    fn buffer_close_status_label_cow_owns_dirty_truncated_and_fallback_labels() {
        let dirty = buffer_close_status_label_cow("alpha\n\u{202e}beta.rs");
        assert_eq!(dirty.as_ref(), "alpha beta.rs");
        assert!(matches!(dirty, Cow::Owned(_)));

        let long = format!("start-{}-finish.rs", "segment".repeat(24));
        let truncated = buffer_close_status_label_cow(&long);
        assert!(truncated.as_ref().starts_with("start-"));
        assert!(truncated.as_ref().contains("..."));
        assert!(truncated.as_ref().ends_with("-finish.rs"));
        assert_eq!(
            truncated.as_ref().chars().count(),
            BUFFER_CLOSE_STATUS_LABEL_MAX_CHARS
        );
        assert!(matches!(truncated, Cow::Owned(_)));

        let fallback = buffer_close_status_label_cow("\n\u{202e}\u{0007}");
        assert_eq!(fallback.as_ref(), "Untitled");
        assert!(matches!(fallback, Cow::Owned(_)));
    }

    #[test]
    fn buffer_close_status_label_matches_cow_helper() {
        for value in [
            "clean.rs",
            "clean-\u{03bb}.rs",
            "  clean.rs  ",
            "alpha\n\u{202e}beta.rs",
            "\n\u{202e}\u{0007}",
        ] {
            assert_eq!(
                buffer_close_status_label(value),
                buffer_close_status_label_cow(value).into_owned()
            );
        }

        let long = format!("start-{}-finish.rs", "segment".repeat(24));
        assert_eq!(
            buffer_close_status_label(&long),
            buffer_close_status_label_cow(&long).into_owned()
        );
    }

    #[test]
    fn closed_file_target_openability_uses_open_buffer_before_filesystem_probe() {
        let path = missing_path("exact-open-buffer").join("src/main.rs");
        let buffers = vec![TextBuffer::from_text(
            7,
            Some(path.clone()),
            "one\ntwo\nabcdef\n".to_owned(),
        )];
        let probes = Cell::new(0usize);

        assert!(closed_file_target_openable(&buffers, &[], &path, |_| {
            probes.set(probes.get() + 1);
            false
        }));

        assert_eq!(probes.get(), 0);
    }

    #[test]
    fn closed_file_target_openability_uses_lexical_open_buffer_before_filesystem_probe() {
        let root = missing_path("lexical-open-buffer");
        let path = root.join("src/main.rs");
        let equivalent_path = root.join("src").join("..").join("src/main.rs");
        let buffers = vec![TextBuffer::from_text(
            7,
            Some(path),
            "one\ntwo\nabcdef\n".to_owned(),
        )];
        let probes = Cell::new(0usize);

        assert!(closed_file_target_openable(
            &buffers,
            &[],
            &equivalent_path,
            |_| {
                probes.set(probes.get() + 1);
                false
            }
        ));

        assert_eq!(probes.get(), 0);
    }

    #[test]
    fn closed_file_openability_cache_reuses_equivalent_missing_probe_results() {
        let mut cache = ClosedFileOpenabilityCache::default();
        let path = PathBuf::from("workspace/src").join(".").join("missing.rs");
        let equivalent_path = PathBuf::from("workspace/src/missing.rs");
        let probes = Cell::new(0usize);

        assert!(!cache.target_openable(&[], &[], &path, |_| {
            probes.set(probes.get() + 1);
            false
        }));
        assert!(!cache.target_openable(&[], &[], &equivalent_path, |_| {
            panic!("equivalent missing closed-file path should not probe again")
        }));

        assert_eq!(probes.get(), 1);
    }

    #[test]
    fn closed_file_openability_cache_keeps_known_paths_before_cached_misses() {
        let mut cache = ClosedFileOpenabilityCache::default();
        let open_path = PathBuf::from("workspace/src/main.rs");
        let cached_open_path = PathBuf::from("workspace/src").join(".").join("main.rs");
        let indexed_path = PathBuf::from("workspace/src/lib.rs");
        let cached_indexed_path = PathBuf::from("workspace/src").join(".").join("lib.rs");

        assert!(!cache.target_openable(&[], &[], &cached_open_path, |_| false));
        assert!(!cache.target_openable(&[], &[], &cached_indexed_path, |_| false));

        let buffers = vec![TextBuffer::from_text(
            7,
            Some(open_path.clone()),
            "one\ntwo\nabcdef\n".to_owned(),
        )];
        assert!(cache.target_openable(&buffers, &[], &open_path, |_| {
            panic!("open buffer should win before a cached missing fallback")
        }));
        assert!(cache.target_openable(
            &[],
            std::slice::from_ref(&indexed_path),
            &indexed_path,
            |_| { panic!("indexed file should win before a cached missing fallback") }
        ));
    }

    #[test]
    fn closed_file_openability_cache_keeps_parent_traversal_probes_distinct() {
        let mut cache = ClosedFileOpenabilityCache::default();
        let parent_path = PathBuf::from("workspace/link")
            .join("..")
            .join("missing.rs");
        let direct_path = PathBuf::from("workspace/missing.rs");
        let probes = Cell::new(0usize);

        assert!(!cache.target_openable(&[], &[], &parent_path, |_| {
            probes.set(probes.get() + 1);
            false
        }));
        assert!(!cache.target_openable(&[], &[], &direct_path, |_| {
            probes.set(probes.get() + 1);
            false
        }));

        assert_eq!(probes.get(), 2);
    }

    #[test]
    fn reopen_closed_file_reuses_already_open_missing_file() {
        let root = missing_path("reopen-exact-open-buffer");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(path.clone()),
            "one\ntwo\nabcdef\n".to_owned(),
        ));
        app.closed_files
            .push_back(crate::history::ClosedFileEntry::new(path.clone(), 3, 5));

        assert!(!path.exists());
        app.reopen_closed_file();

        assert_eq!(app.active, Some(7));
        let cursor = app.buffer(7).unwrap().cursor_position();
        assert_eq!((cursor.line, cursor.column), (2, 4));
        assert!(app.pending_open_paths.is_empty());
        assert!(app.pending_file_jump.is_none());
        assert_eq!(app.status, "Reopened main.rs:3:5");
    }

    #[test]
    fn reopen_closed_file_reuses_lexically_equivalent_open_missing_file() {
        let root = missing_path("reopen-lexical-open-buffer");
        let active_path = root.join("src/active.rs");
        let path = root.join("src/main.rs");
        let equivalent_path = root.join("src").join("..").join("src/main.rs");
        let mut app = app_for_test(root);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(active_path),
            "active\n".to_owned(),
        ));
        app.buffers.push(TextBuffer::from_text(
            8,
            Some(path.clone()),
            "one\ntwo\nabcdef\n".to_owned(),
        ));
        app.set_active_buffer(7);
        app.closed_files.push_back(crate::history::ClosedFileEntry {
            path: equivalent_path.clone(),
            line: 3,
            column: 5,
        });

        assert!(!equivalent_path.exists());
        app.reopen_closed_file();

        assert_eq!(app.active, Some(8));
        let cursor = app.buffer(8).unwrap().cursor_position();
        assert_eq!((cursor.line, cursor.column), (2, 4));
        assert!(app.pending_open_paths.is_empty());
        assert!(app.pending_file_jump.is_none());
        assert_eq!(app.buffer(8).and_then(TextBuffer::path), Some(&path));
        assert_eq!(app.status, "Reopened main.rs:3:5");
    }

    #[test]
    fn force_close_buffer_clears_editor_scroll_state_for_buffer() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root.clone());
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(root.join("src/main.rs")),
            "fn main() {}\n".to_owned(),
        ));
        app.buffers.push(TextBuffer::from_text(
            8,
            Some(root.join("src/lib.rs")),
            "pub fn lib() {}\n".to_owned(),
        ));
        app.panes[0].active = Some(7);
        app.active = Some(7);
        app.editor_scroll_offsets.insert((1, 7), 120.0);
        app.editor_scroll_offsets.insert((1, 8), 240.0);
        app.editor_horizontal_scroll_offsets.insert((1, 7), 12.0);
        app.editor_horizontal_scroll_offsets.insert((1, 8), 24.0);
        app.editor_scroll_targets.insert((1, 7), 300.0);
        app.editor_scroll_targets.insert((1, 8), 360.0);
        app.editor_inertial_scrolls.insert(
            (1, 7),
            EditorInertialScroll {
                velocity_x: 12.0,
                velocity_y: 24.0,
            },
        );
        app.editor_inertial_scrolls.insert(
            (1, 8),
            EditorInertialScroll {
                velocity_x: 36.0,
                velocity_y: 48.0,
            },
        );

        app.force_close_buffer(7);

        assert!(!app.editor_scroll_offsets.contains_key(&(1, 7)));
        assert!(!app.editor_horizontal_scroll_offsets.contains_key(&(1, 7)));
        assert!(!app.editor_scroll_targets.contains_key(&(1, 7)));
        assert!(!app.editor_inertial_scrolls.contains_key(&(1, 7)));
        assert_eq!(app.editor_scroll_offsets.get(&(1, 8)), Some(&240.0));
        assert_eq!(
            app.editor_horizontal_scroll_offsets.get(&(1, 8)),
            Some(&24.0)
        );
        assert_eq!(app.editor_scroll_targets.get(&(1, 8)), Some(&360.0));
        assert_eq!(
            app.editor_inertial_scrolls.get(&(1, 8)),
            Some(&EditorInertialScroll {
                velocity_x: 36.0,
                velocity_y: 48.0,
            })
        );
    }

    #[test]
    fn force_close_buffer_clears_buffer_scoped_editor_caches() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root.clone());
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(root.join("src/main.rs")),
            "needle\n".to_owned(),
        ));
        app.buffers.push(TextBuffer::from_text(
            8,
            Some(root.join("src/lib.rs")),
            "needle\n".to_owned(),
        ));
        app.active = Some(7);
        app.buffer_find_query = "needle".to_owned();
        assert_eq!(app.active_find_matches(), vec![0..6]);
        assert_eq!(app.buffer_find_cache.cached_buffer_ids_for_test(), vec![7]);
        let buffer = app.buffer(7).unwrap().clone();
        assert!(!buffer_needs_line_render_protection_cached(
            &mut app.line_render_protection_cache,
            &buffer,
        ));
        assert!(app.line_render_protection_cache.contains_key(&7));
        assert_eq!(
            app.editor_bracket_overlay_cache
                .bracket_colors_for_lines(&buffer, 0, 1, false),
            buffer.bracket_colors_for_lines_with_options(0, 1, false)
        );
        assert_eq!(
            app.editor_bracket_overlay_cache
                .bracket_pair_guides(&buffer),
            buffer.bracket_pair_guides()
        );
        assert_eq!(
            app.editor_bracket_overlay_cache
                .bracket_matches(&buffer, EditorMatchBrackets::Near),
            buffer.matching_brackets()
        );
        app.minimap_line_length_cache
            .sampled_lengths_for(&buffer, 1, 80, true);
        app.minimap_section_header_cache
            .headers_for(&buffer, true, false, "");
        assert!(app.editor_bracket_overlay_cache.contains_buffer_for_test(7));
        assert!(app.minimap_line_length_cache.contains_buffer_for_test(7));
        assert!(app.minimap_section_header_cache.contains_buffer_for_test(7));
        assert!(
            app.syntax_tree_cache
                .folding_ranges_for_buffer(&buffer)
                .is_some()
        );
        let other_buffer = app.buffer(8).unwrap().clone();
        app.minimap_line_length_cache
            .sampled_lengths_for(&other_buffer, 1, 80, true);
        app.minimap_section_header_cache
            .headers_for(&other_buffer, true, false, "");
        assert!(
            app.syntax_tree_cache
                .folding_ranges_for_buffer(&other_buffer)
                .is_some()
        );
        assert!(app.syntax_tree_cache.contains_buffer_for_test(7));
        assert!(app.syntax_tree_cache.contains_buffer_for_test(8));
        assert!(app.minimap_line_length_cache.contains_buffer_for_test(8));
        assert!(app.minimap_section_header_cache.contains_buffer_for_test(8));

        app.force_close_buffer(7);

        assert!(
            app.buffer_find_cache
                .cached_buffer_ids_for_test()
                .is_empty()
        );
        assert!(!app.line_render_protection_cache.contains_key(&7));
        assert!(!app.editor_bracket_overlay_cache.contains_buffer_for_test(7));
        assert!(!app.minimap_line_length_cache.contains_buffer_for_test(7));
        assert!(!app.minimap_section_header_cache.contains_buffer_for_test(7));
        assert!(!app.syntax_tree_cache.contains_buffer_for_test(7));
        assert!(app.minimap_line_length_cache.contains_buffer_for_test(8));
        assert!(app.minimap_section_header_cache.contains_buffer_for_test(8));
        assert!(app.syntax_tree_cache.contains_buffer_for_test(8));
    }

    #[test]
    fn force_close_buffer_clears_path_scoped_lsp_and_folding_state() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let other_path = root.join("src/lib.rs");
        let mut app = app_for_test(root);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(path.clone()),
            "fn main() {}\n".to_owned(),
        ));
        app.buffers.push(TextBuffer::from_text(
            8,
            Some(other_path.clone()),
            "pub fn lib() {}\n".to_owned(),
        ));
        app.panes[0].active = Some(7);
        app.active = Some(7);

        app.document_symbols_path = Some(path.clone());
        app.document_symbols_selected = 2;
        app.document_highlights_path = Some(path.clone());
        app.document_highlights.push(document_highlight());
        app.lsp_rename_open = true;
        app.lsp_rename_input = "main".to_owned();
        app.open_lsp_rename_preview(
            "renamed".to_owned(),
            vec![rename_edit(path.clone(), "renamed")],
        );
        app.source_control_blame_cache
            .insert(path.clone(), vec![blame_line(1)]);
        app.source_control_blame_cache
            .insert(other_path.clone(), vec![blame_line(2)]);
        app.source_control_blame_pending_path = Some(path.clone());
        app.source_control_blame_load_opens_view = true;
        app.source_control_blame_next_request_id = 99;
        app.source_control_blame_active_request_id = 99;
        app.source_control_blame_active_request_ids
            .insert(path.clone(), 99);
        app.source_control_blame_in_flight_request_ids
            .insert(path.clone(), 99);
        app.source_control_blame_reload_queued_paths
            .insert(path.clone());
        app.source_control_blame_open_view_paths
            .insert(path.clone());
        app.source_control_hunks_open = true;
        app.source_control_hunk_path = Some(path.clone());
        app.source_control_hunk_stage = GitChangeStage::Staged;
        app.source_control_hunks = vec![hunk(2)];
        app.source_control_hunk_selected = 1;
        app.source_control_hunks_next_request_id = 101;
        app.source_control_hunks_active_request_id = 101;
        app.completion_open = true;
        app.completion_path = Some(path.clone());
        app.completion_line = 4;
        app.completion_column = 7;
        app.completion_prefix = "ma".to_owned();
        app.completion_selected = 1;
        app.pending_completion_requests.insert(7, Instant::now());
        app.signature_help = Some(signature_popup(7, path.clone()));
        app.code_actions_open = true;
        app.code_actions_path = Some(path.clone());
        app.code_actions_line = 4;
        app.code_actions_column = 7;
        app.code_actions_selected = 1;
        app.references_open = true;
        app.references_path = Some(path.clone());
        app.references_line = 4;
        app.references_column = 7;
        app.references_selected = 1;
        app.call_hierarchy_open = true;
        app.call_hierarchy_path = Some(path.clone());
        app.call_hierarchy_line = 4;
        app.call_hierarchy_column = 7;
        app.call_hierarchy_selected = 1;
        app.type_hierarchy_open = true;
        app.type_hierarchy_path = Some(path.clone());
        app.type_hierarchy_line = 4;
        app.type_hierarchy_column = 7;
        app.type_hierarchy_selected = 1;
        app.lsp_hover = Some(LspHoverPopup {
            id: 7,
            path: path.clone(),
            line: 4,
            column: 7,
            contents: "hover".to_owned(),
            opened_at: Instant::now(),
        });
        let closed_hover_key = LspHoverCacheKey::new(path.clone(), 1, 3, 6);
        let other_hover_key = LspHoverCacheKey::new(other_path.clone(), 1, 3, 6);
        store_hover_cache(
            &mut app.lsp_hover_cache,
            closed_hover_key.clone(),
            "closed".to_owned(),
            8,
        );
        store_hover_cache(
            &mut app.lsp_hover_cache,
            other_hover_key.clone(),
            "other".to_owned(),
            8,
        );
        app.folding_ranges
            .insert(path.clone(), vec![folding_range(1, 3)]);
        app.folding_ranges
            .insert(other_path.clone(), vec![folding_range(2, 4)]);
        app.folded_ranges.insert(
            path.clone(),
            vec![FoldedRange {
                start_line: 1,
                end_line: 3,
            }],
        );
        app.folded_ranges.insert(
            other_path.clone(),
            vec![FoldedRange {
                start_line: 2,
                end_line: 4,
            }],
        );
        app.pending_fold_line = Some((path.clone(), 1));

        app.force_close_buffer(7);

        assert_eq!(app.document_symbols_path, None);
        assert_eq!(app.document_symbols_selected, 0);
        assert_eq!(app.document_highlights_path, None);
        assert!(app.document_highlights.is_empty());
        assert!(!app.lsp_rename_open);
        assert!(app.lsp_rename_input.is_empty());
        assert!(!app.lsp_rename_preview_open);
        assert!(app.lsp_rename_preview_new_name.is_empty());
        assert!(app.lsp_rename_preview_edits.is_empty());
        assert!(app.lsp_rename_preview_rows.is_empty());
        assert!(app.lsp_rename_preview_versions.is_empty());
        assert!(!app.source_control_blame_cache.contains_key(&path));
        assert!(app.source_control_blame_cache.contains_key(&other_path));
        assert_eq!(app.source_control_blame_pending_path, None);
        assert!(!app.source_control_blame_load_opens_view);
        assert_eq!(app.source_control_blame_active_request_id, 100);
        assert!(app.source_control_blame_active_request_ids.is_empty());
        assert!(app.source_control_blame_in_flight_request_ids.is_empty());
        assert!(app.source_control_blame_reload_queued_paths.is_empty());
        assert!(app.source_control_blame_open_view_paths.is_empty());
        assert!(!app.source_control_hunks_open);
        assert_eq!(app.source_control_hunk_path, None);
        assert_eq!(app.source_control_hunk_stage, GitChangeStage::Unstaged);
        assert!(app.source_control_hunks.is_empty());
        assert_eq!(app.source_control_hunk_selected, 0);
        assert_eq!(app.source_control_hunks_active_request_id, 102);
        assert!(!app.completion_open);
        assert_eq!(app.completion_path, None);
        assert_eq!(app.completion_line, 0);
        assert_eq!(app.completion_column, 0);
        assert!(app.completion_prefix.is_empty());
        assert_eq!(app.completion_selected, 0);
        assert!(!app.pending_completion_requests.contains_key(&7));
        assert!(app.signature_help.is_none());
        assert!(!app.code_actions_open);
        assert_eq!(app.code_actions_path, None);
        assert_eq!(app.code_actions_selected, 0);
        assert!(!app.references_open);
        assert_eq!(app.references_path, None);
        assert_eq!(app.references_selected, 0);
        assert!(!app.call_hierarchy_open);
        assert_eq!(app.call_hierarchy_path, None);
        assert!(!app.type_hierarchy_open);
        assert_eq!(app.type_hierarchy_path, None);
        assert!(app.lsp_hover.is_none());
        assert!(lookup_hover_cache(&app.lsp_hover_cache, &closed_hover_key).is_none());
        assert_eq!(
            lookup_hover_cache(&app.lsp_hover_cache, &other_hover_key).as_deref(),
            Some("other")
        );
        assert!(!app.folding_ranges.contains_key(&path));
        assert!(app.folding_ranges.contains_key(&other_path));
        assert!(!app.folded_ranges.contains_key(&path));
        assert!(app.folded_ranges.contains_key(&other_path));
        assert_eq!(app.pending_fold_line, None);
    }

    #[test]
    fn force_close_buffer_clears_deferred_buffer_work() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(path.clone()),
            "fn main() {}\n".to_owned(),
        ));

        app.pending_lsp_symbol_refreshes.insert(7, Instant::now());
        app.pending_language_sync.insert(7, Instant::now());
        app.pending_completion_requests.insert(7, Instant::now());
        app.pending_signature_help_requests
            .insert(7, Instant::now());
        app.pending_format_on_type_requests
            .insert(7, Instant::now());
        app.in_flight_saves.insert(7);
        app.queued_save_paths.insert(7, path.clone());
        app.pending_format_on_save.insert(
            7,
            PendingFormatOnSave {
                save_path: path.clone(),
                format_path: path.clone(),
                version: 1,
                request_id: 1,
            },
        );
        app.format_on_save_bypass.insert(7);

        app.force_close_buffer(7);

        assert!(!app.pending_lsp_symbol_refreshes.contains_key(&7));
        assert!(!app.pending_language_sync.contains_key(&7));
        assert!(!app.pending_completion_requests.contains_key(&7));
        assert!(!app.pending_signature_help_requests.contains_key(&7));
        assert!(!app.pending_format_on_type_requests.contains_key(&7));
        assert!(!app.in_flight_saves.contains(&7));
        assert!(!app.queued_save_paths.contains_key(&7));
        assert!(!app.pending_format_on_save.contains_key(&7));
        assert!(!app.format_on_save_bypass.contains(&7));
    }

    #[test]
    fn force_close_buffer_clears_close_conflict_and_reload_work() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(path.clone()),
            "fn main() {}\n".to_owned(),
        ));
        let pending = PendingFileReload {
            request_id: 9,
            path: path.clone(),
            version: 1,
            force_dirty: false,
        };
        app.in_flight_reloads.insert(7, pending.clone());
        app.queued_file_reloads.insert(
            7,
            QueuedFileReload {
                path: path.clone(),
                force_dirty: false,
            },
        );
        app.save_conflict_buffer = Some(7);
        app.close_after_save = Some(7);
        app.pending_close_buffers.extend([7, 8]);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.save_as_path = path.display().to_string();

        app.force_close_buffer(7);

        assert!(app.buffer(7).is_none());
        assert_eq!(app.save_conflict_buffer, None);
        assert_eq!(app.close_after_save, None);
        assert!(app.pending_close_buffers.is_empty());
        assert!(!app.save_as_open);
        assert_eq!(app.save_as_buffer, None);
        assert!(app.save_as_path.is_empty());
        assert!(app.canceled_file_reloads.contains(&(7, pending)));
        assert!(!app.in_flight_reloads.contains_key(&7));
        assert!(!app.queued_file_reloads.contains_key(&7));
    }

    #[test]
    fn force_close_missing_buffer_still_clears_transient_state() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let pending = PendingFileReload {
            request_id: 9,
            path: path.clone(),
            version: 1,
            force_dirty: false,
        };
        app.in_flight_reloads.insert(7, pending.clone());
        app.queued_file_reloads.insert(
            7,
            QueuedFileReload {
                path: path.clone(),
                force_dirty: false,
            },
        );
        app.in_flight_saves.insert(7);
        app.queued_save_paths.insert(7, path.clone());
        app.pending_format_on_save.insert(
            7,
            PendingFormatOnSave {
                save_path: path.clone(),
                format_path: path,
                version: 1,
                request_id: 1,
            },
        );
        app.save_conflict_buffer = Some(7);
        app.close_after_save = Some(7);
        app.pending_close_buffers.extend([7, 8]);
        app.dirty_close_buffer = Some(7);
        app.dirty_reload_buffer = Some(7);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.save_as_path = "src/main.rs".to_owned();

        app.force_close_buffer(7);

        assert_eq!(app.save_conflict_buffer, None);
        assert_eq!(app.close_after_save, None);
        assert_eq!(app.dirty_close_buffer, None);
        assert_eq!(app.dirty_reload_buffer, None);
        assert!(app.pending_close_buffers.is_empty());
        assert!(!app.save_as_open);
        assert_eq!(app.save_as_buffer, None);
        assert!(app.save_as_path.is_empty());
        assert!(!app.in_flight_saves.contains(&7));
        assert!(!app.queued_save_paths.contains_key(&7));
        assert!(!app.pending_format_on_save.contains_key(&7));
        assert!(app.canceled_file_reloads.contains(&(7, pending)));
        assert!(!app.in_flight_reloads.contains_key(&7));
        assert!(!app.queued_file_reloads.contains_key(&7));
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

    fn missing_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "kuroya-buffer-close-lifecycle-{}-{unique}-{name}",
            std::process::id()
        ))
    }

    fn signature_popup(id: u64, path: PathBuf) -> LspSignatureHelpPopup {
        LspSignatureHelpPopup {
            id,
            path,
            line: 4,
            column: 7,
            help: LspSignatureHelp {
                signatures: Vec::new(),
                active_signature: 0,
                active_parameter: None,
            },
        }
    }

    fn document_highlight() -> LspDocumentHighlight {
        LspDocumentHighlight {
            line: 3,
            column: 6,
            end_line: 3,
            end_column: 10,
            kind: Some(1),
        }
    }

    fn folding_range(start_line: usize, end_line: usize) -> LspFoldingRange {
        LspFoldingRange {
            start_line,
            start_column: None,
            end_line,
            end_column: None,
            kind: None,
        }
    }

    fn rename_edit(path: PathBuf, new_text: &str) -> LspTextEdit {
        LspTextEdit {
            path,
            start_line: 1,
            start_column: 4,
            end_line: 1,
            end_column: 8,
            new_text: new_text.to_owned(),
        }
    }

    fn blame_line(line_number: usize) -> GitBlameLine {
        GitBlameLine {
            line_number,
            short_oid: "abcdef0".to_owned(),
            author: "Author".to_owned(),
            author_time_seconds: 1_700_000_000,
            summary: "summary".to_owned(),
        }
    }

    fn hunk(index: usize) -> GitDiffHunk {
        GitDiffHunk {
            index,
            fingerprint: index as u64,
            old_start: 1,
            old_lines: 1,
            new_start: 1,
            new_lines: 1,
            additions: 1,
            deletions: 0,
            header: format!("@@ -1 +{index} @@"),
        }
    }
}
