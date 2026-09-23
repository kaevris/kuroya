use crate::{
    KuroyaApp,
    navigation_targets::navigation_target_label,
    quick_open::{MAX_QUICK_OPEN_RECENT_FILES, record_quick_open_navigation},
    session_state::EditorPane,
    ui_state::wrapped_index,
    workspace_state::{PaneId, take_pending_panes_for_path},
};
use kuroya_core::BufferId;
use std::path::Path;

impl KuroyaApp {
    pub(crate) fn assign_buffer_to_pane(&mut self, pane_id: PaneId, id: BufferId) {
        if let Some(pane) = self.panes.iter_mut().find(|pane| pane.id == pane_id) {
            pane.active = Some(id);
        } else {
            self.panes.push(EditorPane {
                id: pane_id,
                active: Some(id),
                weight: 1.0,
            });
            self.normalize_pane_weights();
        }
    }

    pub(crate) fn pane_id_for_buffer(&self, id: BufferId) -> Option<PaneId> {
        self.panes
            .iter()
            .find(|pane| pane.active == Some(id))
            .map(|pane| pane.id)
    }

    pub(crate) fn active_pane_holding_buffer(&self, id: BufferId) -> Option<PaneId> {
        self.panes
            .iter()
            .find(|pane| pane.id == self.active_pane && pane.active == Some(id))
            .map(|pane| pane.id)
    }

    pub(crate) fn take_pending_panes_for_path(&mut self, path: &Path) -> Vec<PaneId> {
        take_pending_panes_for_path(&mut self.pending_pane_paths, path)
    }

    pub(crate) fn set_active_buffer(&mut self, id: BufferId) {
        self.vim_discard_half_typed_sequence();
        self.close_stale_vim_insert_undo_group(id);
        self.clear_completion_popup_for_inactive_buffer(id);
        let pane_id = self.pane_id_for_activation(id);
        self.active_pane = pane_id;
        self.active = Some(id);
        self.focused_pane = Some(pane_id);
        self.assign_buffer_to_pane(pane_id, id);
        self.record_quick_open_active_file(id);
        self.maybe_auto_reveal_active_file_in_source_control();

        if self.symbols_panel {
            self.request_lsp_document_symbols();
        }
    }

    pub(crate) fn set_active_buffer_in_pane(&mut self, pane_id: PaneId, id: BufferId) {
        self.vim_discard_half_typed_sequence();
        self.close_stale_vim_insert_undo_group(id);
        self.clear_completion_popup_for_inactive_buffer(id);
        self.active_pane = pane_id;
        self.focused_pane = Some(pane_id);
        self.active = Some(id);
        self.assign_buffer_to_pane(pane_id, id);
        self.record_quick_open_active_file(id);
        self.maybe_auto_reveal_active_file_in_source_control();

        if self.symbols_panel {
            self.request_lsp_document_symbols();
        }
    }

    fn pane_id_for_activation(&self, id: BufferId) -> PaneId {
        if self.pane_exists(self.active_pane) {
            return self.active_pane;
        }
        if let Some(focused_pane) = self
            .focused_pane
            .filter(|pane_id| self.pane_exists(*pane_id))
        {
            return focused_pane;
        }
        if let Some(pane_id) = self.pane_id_for_buffer(id) {
            return pane_id;
        }
        self.panes
            .first()
            .map(|pane| pane.id)
            .unwrap_or(self.active_pane)
    }

    fn pane_exists(&self, pane_id: PaneId) -> bool {
        self.panes.iter().any(|pane| pane.id == pane_id)
    }

    pub(crate) fn clear_completion_popup_for_inactive_buffer(&mut self, id: BufferId) {
        if self.completion_buffer_id.is_some_and(|origin| origin != id) {
            self.clear_completion_popup_state();
        }
    }

    fn record_quick_open_active_file(&mut self, id: BufferId) {
        let Some(path) = self.buffer(id).and_then(|buffer| buffer.path()).cloned() else {
            return;
        };

        record_quick_open_navigation(
            &mut self.quick_open_recent_files,
            &self.workspace.root,
            &path,
            MAX_QUICK_OPEN_RECENT_FILES,
        );
    }

    pub(crate) fn activate_relative_tab(&mut self, direction: isize) {
        if self.buffers.is_empty() {
            self.status = "No open tabs".to_owned();
            return;
        }

        let next = self
            .active
            .and_then(|id| self.buffers.iter().position(|buffer| buffer.id() == id))
            .map(|current| wrapped_index(current, self.buffers.len(), direction))
            .unwrap_or(0);
        let id = self.buffers[next].id();
        let label = navigation_target_label(&self.buffer_label(id));
        self.set_active_buffer(id);
        self.status = format!("Active tab: {label}");
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        KuroyaApp, app_startup_context::AppStartupContext,
        navigation_targets::NAVIGATION_TARGET_LABEL_MAX_CHARS, terminal::TerminalPane,
    };
    use kuroya_core::{EditorSettings, TextBuffer, Workspace};
    use std::{path::PathBuf, time::Instant};
    use tokio::runtime::Runtime;

    #[test]
    fn activate_relative_tab_sanitizes_status_label() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        app.buffers
            .push(TextBuffer::from_text(1, None, "one".to_owned()));
        app.buffers
            .push(TextBuffer::from_text(2, None, "two".to_owned()));
        app.virtual_buffer_labels.insert(
            2,
            format!("next\n{}\u{202e}tail.rs", "very-long-component-".repeat(16)),
        );
        app.active = Some(1);
        app.panes[0].active = Some(1);

        app.activate_relative_tab(1);

        assert!(app.status.starts_with("Active tab: next "));
        assert!(!app.status.contains('\n'));
        assert!(!app.status.contains('\u{202e}'));
        assert!(app.status.contains("..."));
        assert!(
            app.status
                .trim_start_matches("Active tab: ")
                .chars()
                .count()
                <= NAVIGATION_TARGET_LABEL_MAX_CHARS
        );
        assert_eq!(app.active, Some(2));
    }

    #[test]
    fn set_active_buffer_clears_completion_popup_for_previous_buffer() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root.clone());
        app.buffers.push(TextBuffer::from_text(
            1,
            Some(root.join("src/main.rs")),
            "one".to_owned(),
        ));
        app.buffers.push(TextBuffer::from_text(
            2,
            Some(root.join("src/lib.rs")),
            "two".to_owned(),
        ));
        app.active = Some(1);
        app.completion_open = true;
        app.completion_buffer_id = Some(1);
        app.completion_path = Some(root.join("src/main.rs"));
        app.completion_version = Some(3);
        app.completion_line = 4;
        app.completion_column = 5;
        app.completion_prefix = "on".to_owned();
        app.completion_selected = 1;

        app.set_active_buffer(2);

        assert_eq!(app.active, Some(2));
        assert!(!app.completion_open);
        assert_eq!(app.completion_buffer_id, None);
        assert_eq!(app.completion_path, None);
        assert_eq!(app.completion_version, None);
        assert_eq!(app.completion_line, 0);
        assert_eq!(app.completion_column, 0);
        assert!(app.completion_prefix.is_empty());
        assert_eq!(app.completion_selected, 0);
    }

    #[test]
    fn set_active_buffer_repairs_stale_active_pane() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        app.buffers
            .push(TextBuffer::from_text(1, None, "one".to_owned()));
        app.active_pane = 99;
        app.focused_pane = Some(1);

        app.set_active_buffer(1);

        assert_eq!(app.active, Some(1));
        assert_eq!(app.active_pane, 1);
        assert_eq!(app.focused_pane, Some(1));
        assert_eq!(app.panes.len(), 1);
        assert_eq!(app.panes[0].active, Some(1));
    }

    #[test]
    fn activate_relative_tab_uses_first_tab_when_active_buffer_is_stale() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        app.buffers
            .push(TextBuffer::from_text(1, None, "one".to_owned()));
        app.buffers
            .push(TextBuffer::from_text(2, None, "two".to_owned()));
        app.active = Some(99);

        app.activate_relative_tab(1);

        assert_eq!(app.active, Some(1));
        assert_eq!(app.panes[0].active, Some(1));
        assert!(app.status.starts_with("Active tab:"));
    }

    #[test]
    fn set_active_buffer_discards_half_typed_vim_sequence() {
        use crate::editor_vim_key_events::{
            EditorVimMode, EditorVimPendingKey, EditorVimRegister, EditorVimRegisterKind,
            vim_command_input_text_for_test, vim_search_input_text_for_test,
        };

        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root.clone());
        app.buffers
            .push(TextBuffer::from_text(1, None, "one".to_owned()));
        app.buffers
            .push(TextBuffer::from_text(2, None, "two".to_owned()));
        app.active = Some(1);
        app.panes[0].active = Some(1);
        app.editor_vim_mode = EditorVimMode::Insert;
        app.editor_vim_pending_key = Some(EditorVimPendingKey::DeleteMotionCount {
            operator_count: 1,
            motion_count: 2,
        });
        app.editor_vim_unnamed_register = Some(EditorVimRegister {
            text: "yanked".to_owned(),
            kind: EditorVimRegisterKind::Characterwise,
        });
        crate::editor_vim_key_events::vim_set_search_input_text_for_test("query");
        crate::editor_vim_key_events::vim_set_command_input_text_for_test("s/a/b");

        app.set_active_buffer(2);

        assert_eq!(app.editor_vim_pending_key, None);
        assert_eq!(vim_search_input_text_for_test(), "");
        assert_eq!(vim_command_input_text_for_test(), "");

        assert_eq!(app.editor_vim_mode, EditorVimMode::Insert);
        assert_eq!(
            app.editor_vim_unnamed_register,
            Some(EditorVimRegister {
                text: "yanked".to_owned(),
                kind: EditorVimRegisterKind::Characterwise,
            })
        );
        assert_eq!(app.active, Some(2));
    }

    #[test]
    fn set_active_buffer_in_pane_discards_half_typed_vim_sequence() {
        use crate::editor_vim_key_events::{EditorVimPendingKey, vim_search_input_text_for_test};

        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root.clone());
        app.buffers
            .push(TextBuffer::from_text(1, None, "one".to_owned()));
        app.buffers
            .push(TextBuffer::from_text(2, None, "two".to_owned()));
        app.active = Some(1);
        app.panes[0].active = Some(1);
        app.editor_vim_pending_key = Some(EditorVimPendingKey::Count(3));
        crate::editor_vim_key_events::vim_set_search_input_text_for_test("query");

        app.set_active_buffer_in_pane(1, 2);

        assert_eq!(app.editor_vim_pending_key, None);
        assert_eq!(vim_search_input_text_for_test(), "");
        assert_eq!(app.active, Some(2));
    }

    #[test]
    fn set_active_buffer_ends_insert_undo_group_opened_on_previous_buffer() {
        use crate::editor_vim_key_events::EditorVimMode;
        use eframe::egui::{Context, Event, Key, Modifiers};

        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root.clone());
        app.buffers
            .push(TextBuffer::from_text(1, None, "one".to_owned()));
        app.buffers
            .push(TextBuffer::from_text(2, None, "two".to_owned()));
        app.active = Some(1);
        app.panes[0].active = Some(1);
        app.focused_pane = Some(1);
        app.settings.vim_keybindings = true;

        let ctx = Context::default();
        ctx.input_mut(|input| {
            input.events.push(Event::Key {
                key: Key::O,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            });
        });
        app.handle_editor_input(&ctx, 1, 1);

        assert_eq!(app.editor_vim_mode, EditorVimMode::Insert);
        assert_eq!(app.editor_vim_insert_undo_group_buffer, Some(1));

        app.set_active_buffer(2);

        assert_eq!(app.editor_vim_insert_undo_group_buffer, None);
        let buffer_one = app.buffer_mut(1).expect("buffer 1 remains loaded");
        assert_eq!(buffer_one.text(), "one\n");
        assert_eq!(buffer_one.undo_entry_count(), 1);
        assert!(buffer_one.undo());
        assert_eq!(buffer_one.text(), "one");
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
}
