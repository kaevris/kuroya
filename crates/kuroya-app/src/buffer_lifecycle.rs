use crate::{
    KuroyaApp,
    lsp_hover_cache::remove_hover_cache_entries_for_path,
    lsp_lifecycle::background_language_block_reason,
    path_display::{
        DISPLAY_PATH_LABEL_MAX_CHARS, display_path_label_cow, sanitized_display_label_cow,
    },
    workspace_state::paths_match_lexically,
};
use kuroya_core::{BufferId, TextBuffer};
use std::{borrow::Cow, collections::HashSet, path::Path, time::Instant};

impl KuroyaApp {
    pub(crate) fn next_id(&mut self) -> BufferId {
        let id = self.next_buffer_id;
        self.next_buffer_id += 1;
        id
    }

    pub(crate) fn active_buffer(&self) -> Option<&TextBuffer> {
        let id = self.active?;
        self.buffer(id)
    }

    pub(crate) fn buffer_label(&self, id: BufferId) -> String {
        if let Some(buffer) = self.buffer(id) {
            return self.buffer_label_for(buffer);
        }
        "Untitled".to_owned()
    }

    pub(crate) fn buffer_label_for(&self, buffer: &TextBuffer) -> String {
        let id = buffer.id();
        if let Some(label) = self.virtual_buffer_labels.get(&id) {
            return buffer_label_display_text(label);
        }
        buffer
            .path()
            .map(|path| display_path_label_cow(path.as_path()).into_owned())
            .unwrap_or_else(|| "Untitled".to_owned())
    }

    pub(crate) fn buffer_tab_name(&self, id: BufferId) -> String {
        if let Some(label) = self.virtual_buffer_labels.get(&id) {
            return label.clone();
        }
        self.buffer(id)
            .and_then(TextBuffer::path)
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| "Untitled".to_owned())
    }

    pub(crate) fn buffer(&self, id: BufferId) -> Option<&TextBuffer> {
        self.buffers.iter().find(|buffer| buffer.id() == id)
    }

    pub(crate) fn buffer_mut(&mut self, id: BufferId) -> Option<&mut TextBuffer> {
        self.buffers.iter_mut().find(|buffer| buffer.id() == id)
    }

    pub(crate) fn buffer_by_path(&self, path: &Path) -> Option<&TextBuffer> {
        self.buffers
            .iter()
            .find(|buffer| buffer.path().is_some_and(|candidate| candidate == path))
    }

    pub(crate) fn buffer_by_lexical_path(&self, path: &Path) -> Option<&TextBuffer> {
        if let Some(buffer) = self.buffer_by_path(path) {
            return Some(buffer);
        }

        self.buffers.iter().find(|buffer| {
            buffer
                .path()
                .is_some_and(|candidate| paths_match_lexically(candidate, path))
        })
    }

    pub(crate) fn mark_buffer_changed(&mut self, id: BufferId) {
        self.buffer_find_cache.clear_for_buffer(id);
        self.editor_bracket_overlay_cache.clear_for_buffer(id);
        self.editor_row_render_cache.clear_for_buffer(id);
        self.minimap_line_length_cache.clear_for_buffer(id);
        self.minimap_section_header_cache.clear_for_buffer(id);
        self.diff_cache.remove(&id);
        self.diff_cache_pending.retain(|key, _| key.buffer_id != id);
        self.clear_buffer_merge_conflict_cache(id);
        self.clear_editor_selection_drag_for_buffer(id);
        self.clear_pending_lsp_hover_for_buffer(id);
        if let Some(path) = self.buffer(id).and_then(TextBuffer::path).cloned() {
            self.clear_buffer_path_state_for_path(&path);
        }
        self.pending_lsp_symbol_refreshes.remove(&id);
        self.schedule_language_sync(id);
    }

    pub(crate) fn clear_buffer_path_state_for_path(&mut self, path: &Path) {
        self.clear_lsp_transient_ui_for_path(path);
        self.folding_ranges.remove(path);
        self.clear_lsp_overlay_caches_for_path(path);
        self.folded_ranges.remove(path);
        if self
            .pending_fold_line
            .as_ref()
            .is_some_and(|(pending_path, _)| pending_path == path)
        {
            self.pending_fold_line = None;
        }
        self.clear_source_control_blame_for_path(path);
        self.clear_source_control_hunks_for_path(path);
    }

    pub(crate) fn clear_lsp_overlay_caches_for_path(&mut self, path: &Path) {
        self.inlay_hints.remove(path);
        self.code_lenses.remove(path);
        self.semantic_tokens.remove(path);
    }

    pub(crate) fn clear_lsp_transient_ui_for_path(&mut self, path: &Path) {
        if self.lsp_rename_open
            && self
                .active_buffer()
                .and_then(TextBuffer::path)
                .is_some_and(|active_path| active_path == path)
        {
            self.lsp_rename_open = false;
            self.lsp_rename_input.clear();
        }
        if self.document_symbols_path.as_deref() == Some(path) {
            self.document_symbols.clear();
            self.document_symbols_path = None;
            self.document_symbols_selected = 0;
        }
        if self.document_highlights_path.as_deref() == Some(path) {
            self.document_highlights_path = None;
            self.document_highlights.clear();
        }
        if self.completion_path.as_deref() == Some(path) {
            self.clear_completion_popup_state();
        }
        if self
            .signature_help
            .as_ref()
            .is_some_and(|popup| popup.path == path)
        {
            self.signature_help = None;
        }
        if self.code_actions_path.as_deref() == Some(path) {
            self.code_actions_open = false;
            self.code_actions.clear();
            self.code_actions_buffer_id = None;
            self.code_actions_path = None;
            self.code_actions_version = None;
            self.code_actions_line = 0;
            self.code_actions_column = 0;
            self.code_actions_selected = 0;
        }
        if self.references_path.as_deref() == Some(path) {
            self.references_open = false;
            self.references.clear();
            self.references_path = None;
            self.references_line = 0;
            self.references_column = 0;
            self.references_selected = 0;
        }
        if self.call_hierarchy_path.as_deref() == Some(path) {
            self.clear_call_hierarchy();
        }
        if self.type_hierarchy_path.as_deref() == Some(path) {
            self.clear_type_hierarchy();
        }
        if self
            .lsp_hover
            .as_ref()
            .is_some_and(|hover| hover.path == path)
        {
            self.lsp_hover = None;
        }
        if let Some(buffer_id) = self.buffer_by_path(path).map(TextBuffer::id) {
            self.pending_completion_requests.remove(&buffer_id);
            self.pending_signature_help_requests.remove(&buffer_id);
            self.pending_format_on_type_requests.remove(&buffer_id);
            self.clear_pending_lsp_hover_for_buffer(buffer_id);
        }
        if self
            .lsp_hover_request
            .as_ref()
            .is_some_and(|target| target.path.as_path() == path)
        {
            self.lsp_hover_request = None;
        }
        remove_hover_cache_entries_for_path(&mut self.lsp_hover_cache, path);
        self.clear_lsp_rename_preview_for_path(path);
    }

    pub(crate) fn schedule_language_sync(&mut self, id: BufferId) {
        let Some(buffer) = self.buffer(id) else {
            self.pending_language_sync.remove(&id);
            self.clear_static_diagnostics_request(id);
            return;
        };
        if should_schedule_language_sync(
            id,
            buffer,
            &self.lossy_decoded_buffers,
            &self.binary_preview_buffers,
        ) {
            self.pending_language_sync.insert(id, Instant::now());
        } else {
            let diagnostic_path = self.diagnostic_path_for(buffer);
            self.pending_language_sync.remove(&id);
            self.invalidate_static_diagnostics_request(id);
            self.diagnostics.replace_static(diagnostic_path, Vec::new());
        }
    }
}

fn buffer_label_display_text(label: &str) -> String {
    buffer_label_display_text_cow(label).into_owned()
}

fn buffer_label_display_text_cow(label: &str) -> Cow<'_, str> {
    sanitized_display_label_cow(label, DISPLAY_PATH_LABEL_MAX_CHARS, "Untitled")
}

pub(crate) fn should_schedule_language_sync(
    id: BufferId,
    buffer: &TextBuffer,
    lossy_buffers: &HashSet<BufferId>,
    binary_buffers: &HashSet<BufferId>,
) -> bool {
    background_language_block_reason(id, buffer, lossy_buffers, binary_buffers).is_none()
}

#[cfg(test)]
mod tests {
    use super::{
        buffer_label_display_text, buffer_label_display_text_cow, should_schedule_language_sync,
    };
    use crate::large_file_mode::LARGE_FILE_MODE_MAX_LINES;
    use crate::path_display::DISPLAY_PATH_LABEL_MAX_CHARS;
    use crate::{
        KuroyaApp,
        app_startup_context::AppStartupContext,
        terminal::TerminalPane,
        transient_state::{LspHoverPopup, LspSignatureHelpPopup},
    };
    use kuroya_core::{
        EditorMatchBrackets, EditorSettings, GitBlameLine, GitChangeStage, GitDiffHunk,
        LspCodeLens, LspDocumentHighlight, LspInlayHint, LspSemanticToken, LspSignatureHelp,
        LspTextEdit, TextBuffer, Workspace,
    };
    use std::{borrow::Cow, collections::HashSet, path::PathBuf, time::Instant};
    use tokio::runtime::Runtime;

    #[test]
    fn language_sync_scheduling_skips_protected_buffers() {
        let safe = TextBuffer::from_text(
            1,
            Some(PathBuf::from("workspace/src/main.rs")),
            "fn main() {}".to_owned(),
        );
        let large_text = std::iter::repeat_n("x", LARGE_FILE_MODE_MAX_LINES + 1)
            .collect::<Vec<_>>()
            .join("\n");
        let large =
            TextBuffer::from_text(2, Some(PathBuf::from("workspace/src/large.rs")), large_text);

        assert!(should_schedule_language_sync(
            safe.id(),
            &safe,
            &HashSet::new(),
            &HashSet::new()
        ));
        assert!(!should_schedule_language_sync(
            safe.id(),
            &safe,
            &HashSet::from([safe.id()]),
            &HashSet::new()
        ));
        assert!(!should_schedule_language_sync(
            safe.id(),
            &safe,
            &HashSet::new(),
            &HashSet::from([safe.id()])
        ));
        assert!(!should_schedule_language_sync(
            large.id(),
            &large,
            &HashSet::new(),
            &HashSet::new()
        ));
    }

    #[test]
    fn buffer_label_sanitizes_virtual_labels_for_shared_display_text() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        app.buffers
            .push(TextBuffer::from_text(7, None, "virtual".to_owned()));
        let raw_label = format!("untitled\n{}\u{202e}.rs", "very-long-".repeat(32));
        app.virtual_buffer_labels.insert(7, raw_label.clone());

        let label = app.buffer_label(7);

        assert_safe_display_text(&label);
        assert!(label.contains("..."));
        assert!(label.chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS);
        assert_eq!(app.virtual_buffer_labels.get(&7), Some(&raw_label));
    }

    #[test]
    fn buffer_label_sanitizes_path_labels_without_rewriting_buffer_path() {
        let root = PathBuf::from("workspace");
        let path = root
            .join("src")
            .join(format!("main\n{}\u{202e}.rs", "very-long-".repeat(32)));
        let mut app = app_for_test(root);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(path.clone()),
            "fn main() {}\n".to_owned(),
        ));

        let label = app.buffer_label(7);

        assert_safe_display_text(&label);
        assert!(label.contains("..."));
        assert!(label.chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS);
        assert_eq!(app.buffer(7).and_then(TextBuffer::path), Some(&path));
    }

    #[test]
    fn buffer_label_display_text_falls_back_for_blank_control_labels() {
        assert_eq!(buffer_label_display_text("\n\u{202e}\u{0007}"), "Untitled");
    }

    #[test]
    fn buffer_label_display_text_cow_borrows_clean_ascii_and_unicode_labels() {
        assert!(matches!(
            buffer_label_display_text_cow("clean.rs"),
            Cow::Borrowed("clean.rs")
        ));

        let unicode = "clean-\u{03bb}.rs";
        match buffer_label_display_text_cow(unicode) {
            Cow::Borrowed(label) => assert_eq!(label, unicode),
            Cow::Owned(label) => panic!("expected borrowed label, got {label:?}"),
        }
    }

    #[test]
    fn buffer_label_display_text_cow_owns_dirty_truncated_and_fallback_labels() {
        let long = format!("main-{}.rs", "x".repeat(DISPLAY_PATH_LABEL_MAX_CHARS * 2));
        let labels = [
            " clean.rs ",
            "bad\nname\u{202e}",
            long.as_str(),
            "\n\u{202e}\u{0007}",
        ];

        for label in labels {
            let display_label = buffer_label_display_text_cow(label);

            assert_eq!(display_label.as_ref(), buffer_label_display_text(label));
            assert!(
                matches!(&display_label, Cow::Owned(_)),
                "expected owned label for {label:?}"
            );
        }
    }

    #[test]
    fn buffer_label_display_text_string_wrapper_matches_cow_helper() {
        let long = format!("main-{}.rs", "x".repeat(DISPLAY_PATH_LABEL_MAX_CHARS * 2));
        let labels = [
            "clean.rs",
            "clean-\u{03bb}.rs",
            "bad\nname\u{202e}",
            long.as_str(),
            "\n\u{202e}\u{0007}",
        ];

        for label in labels {
            assert_eq!(
                buffer_label_display_text(label),
                buffer_label_display_text_cow(label).into_owned()
            );
        }
    }

    #[test]
    fn buffer_by_lexical_path_matches_equivalent_open_path() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let equivalent_path = root.join("src").join("..").join("src").join("main.rs");
        let mut app = app_for_test(root);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(path.clone()),
            "fn main() {}\n".to_owned(),
        ));

        assert_eq!(
            app.buffer_by_lexical_path(&equivalent_path)
                .map(TextBuffer::id),
            Some(7)
        );
        assert!(
            app.buffer_by_lexical_path(PathBuf::from("other/main.rs").as_path())
                .is_none()
        );
    }

    #[test]
    fn buffer_changes_clear_path_scoped_lsp_popups() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.buffers.push(TextBuffer::from_text(
            1,
            Some(path.clone()),
            "fn main() {}".to_owned(),
        ));
        app.panes[0].active = Some(1);
        app.active = Some(1);

        app.document_symbols_path = Some(path.clone());
        app.document_symbols_selected = 2;
        app.document_highlights_path = Some(path.clone());
        app.document_highlights = vec![LspDocumentHighlight {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            kind: Some(2),
        }];
        app.inlay_hints.insert(path.clone(), vec![inlay_hint(1, 1)]);
        app.code_lenses
            .insert(path.clone(), vec![code_lens(1, 1, "Run")]);
        app.semantic_tokens
            .insert(path.clone(), vec![semantic_token(1, 1, 2, "function")]);
        app.lsp_rename_open = true;
        app.lsp_rename_input = "main".to_owned();
        app.open_lsp_rename_preview(
            "renamed".to_owned(),
            vec![rename_edit(path.clone(), "renamed")],
        );
        app.source_control_blame_cache
            .insert(path.clone(), vec![blame_line(1)]);
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
        app.completion_selected = 1;
        app.pending_completion_requests.insert(1, Instant::now());
        app.pending_signature_help_requests
            .insert(1, Instant::now());
        app.pending_format_on_type_requests
            .insert(1, Instant::now());
        app.signature_help = Some(signature_popup(1, path.clone()));
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
        app.type_hierarchy_open = true;
        app.type_hierarchy_path = Some(path.clone());
        app.type_hierarchy_line = 4;
        app.type_hierarchy_column = 7;
        app.lsp_hover = Some(LspHoverPopup {
            id: 1,
            path: path.clone(),
            line: 4,
            column: 7,
            contents: "hover".to_owned(),
            opened_at: Instant::now(),
        });

        app.mark_buffer_changed(1);

        assert_eq!(app.document_symbols_path, None);
        assert_eq!(app.document_symbols_selected, 0);
        assert_eq!(app.document_highlights_path, None);
        assert!(app.document_highlights.is_empty());
        assert!(!app.inlay_hints.contains_key(&path));
        assert!(!app.code_lenses.contains_key(&path));
        assert!(!app.semantic_tokens.contains_key(&path));
        assert!(!app.lsp_rename_open);
        assert!(app.lsp_rename_input.is_empty());
        assert!(!app.lsp_rename_preview_open);
        assert!(app.lsp_rename_preview_new_name.is_empty());
        assert!(app.lsp_rename_preview_edits.is_empty());
        assert!(app.lsp_rename_preview_rows.is_empty());
        assert!(app.lsp_rename_preview_versions.is_empty());
        assert!(!app.source_control_blame_cache.contains_key(&path));
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
        assert_eq!(app.completion_selected, 0);
        assert!(!app.pending_completion_requests.contains_key(&1));
        assert!(!app.pending_signature_help_requests.contains_key(&1));
        assert!(!app.pending_format_on_type_requests.contains_key(&1));
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
    }

    #[test]
    fn mark_buffer_changed_clears_buffer_scoped_editor_caches() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.buffers.push(TextBuffer::from_text(
            1,
            Some(path),
            "{\n  needle\n}\n".to_owned(),
        ));
        let buffer = app.buffer(1).unwrap().clone();

        app.editor_bracket_overlay_cache
            .bracket_colors_for_lines(&buffer, 0, 3, false);
        app.editor_bracket_overlay_cache
            .bracket_pair_guides(&buffer);
        app.editor_bracket_overlay_cache
            .bracket_matches(&buffer, EditorMatchBrackets::Near);
        app.minimap_line_length_cache
            .sampled_lengths_for(&buffer, 1, 80, true);
        app.minimap_section_header_cache
            .headers_for(&buffer, true, false, "");
        assert!(app.editor_bracket_overlay_cache.contains_buffer_for_test(1));
        assert!(app.minimap_line_length_cache.contains_buffer_for_test(1));
        assert!(app.minimap_section_header_cache.contains_buffer_for_test(1));

        app.mark_buffer_changed(1);

        assert!(!app.editor_bracket_overlay_cache.contains_buffer_for_test(1));
        assert!(!app.minimap_line_length_cache.contains_buffer_for_test(1));
        assert!(!app.minimap_section_header_cache.contains_buffer_for_test(1));
    }

    #[test]
    fn buffer_changes_keep_other_file_lsp_popups() {
        let root = PathBuf::from("workspace");
        let changed_path = root.join("src/main.rs");
        let popup_path = root.join("src/other.rs");
        let mut app = app_for_test(root);
        app.buffers.push(TextBuffer::from_text(
            1,
            Some(changed_path),
            "fn main() {}".to_owned(),
        ));
        app.completion_open = true;
        app.completion_path = Some(popup_path.clone());
        app.signature_help = Some(signature_popup(1, popup_path.clone()));
        app.code_actions_open = true;
        app.code_actions_path = Some(popup_path.clone());
        app.references_open = true;
        app.references_path = Some(popup_path.clone());
        app.lsp_hover = Some(LspHoverPopup {
            id: 1,
            path: popup_path.clone(),
            line: 4,
            column: 7,
            contents: "hover".to_owned(),
            opened_at: Instant::now(),
        });
        app.source_control_hunks_open = true;
        app.source_control_hunk_path = Some(popup_path.clone());
        app.source_control_hunk_stage = GitChangeStage::Staged;
        app.source_control_hunks = vec![hunk(3)];
        app.source_control_hunk_selected = 0;
        app.source_control_hunks_active_request_id = 55;

        app.mark_buffer_changed(1);

        assert!(app.completion_open);
        assert_eq!(app.completion_path, Some(popup_path.clone()));
        assert!(app.signature_help.is_some());
        assert!(app.code_actions_open);
        assert_eq!(app.code_actions_path, Some(popup_path.clone()));
        assert!(app.references_open);
        assert_eq!(app.references_path, Some(popup_path.clone()));
        assert!(app.lsp_hover.is_some());
        assert!(app.source_control_hunks_open);
        assert_eq!(app.source_control_hunk_path, Some(popup_path));
        assert_eq!(app.source_control_hunk_stage, GitChangeStage::Staged);
        assert_eq!(app.source_control_hunks, vec![hunk(3)]);
        assert_eq!(app.source_control_hunks_active_request_id, 55);
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

    fn inlay_hint(line: usize, column: usize) -> LspInlayHint {
        LspInlayHint {
            line,
            column,
            label: "hint".to_owned(),
            kind: None,
        }
    }

    fn code_lens(line: usize, column: usize, title: &str) -> LspCodeLens {
        LspCodeLens {
            line,
            column,
            title: title.to_owned(),
            command: None,
            command_arguments: None,
            resolve_payload: None,
        }
    }

    fn semantic_token(
        line: usize,
        column: usize,
        length: usize,
        token_type: &str,
    ) -> LspSemanticToken {
        LspSemanticToken {
            line,
            column,
            length,
            token_type: token_type.to_owned(),
            modifiers: Vec::new(),
        }
    }

    fn assert_safe_display_text(label: &str) {
        assert!(!label.contains('\n'), "{label:?}");
        assert!(!label.contains('\r'), "{label:?}");
        assert!(!label.contains('\u{202e}'), "{label:?}");
        assert!(!label.contains('\u{2066}'), "{label:?}");
    }
}
