use crate::{
    KuroyaApp,
    path_display::{display_error_label_cow, display_path_label_cow},
    ui_events::UiEvent,
};

impl KuroyaApp {
    pub(crate) fn handle_file_load_event(&mut self, event: UiEvent) {
        match event {
            UiEvent::FileLoaded {
                root,
                generation,
                path,
                buffer,
                elapsed,
                activate,
                lossy,
                binary,
            } => {
                if !self.workspace_event_is_current(&root, generation) {
                    return;
                }
                self.handle_file_loaded(path, buffer, elapsed, activate, lossy, binary);
            }
            UiEvent::ImageFileLoaded {
                root,
                generation,
                path,
                buffer,
                preview,
                elapsed,
                activate,
            } => {
                if !self.workspace_event_is_current(&root, generation) {
                    return;
                }
                self.handle_image_file_loaded(path, buffer, preview, elapsed, activate);
            }
            UiEvent::FileLoadFailed {
                root,
                generation,
                path,
                error,
            } => {
                if !self.workspace_event_is_current(&root, generation) {
                    return;
                }
                let pending_panes = self.take_pending_panes_for_path(&path);
                if self.clear_pending_file_load_state_for_path(&path) {
                    self.status = format!(
                        "Could not open {}: {}",
                        display_path_label_cow(&path),
                        display_error_label_cow(&error)
                    );
                }
                // Panes that were created for this load are still empty
                // (`active: None`); without a buffer they could never be
                // focused, so close them instead of leaving them orphaned.
                self.close_orphaned_panes(&pending_panes);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app_startup_context::AppStartupContext,
        file_runtime::loaded_text_buffer,
        image_preview::{LoadedImagePreview, image_preview_buffer_text},
        persistence::{BufferHistoryState, BufferViewState},
        terminal::TerminalPane,
        transient_state::FileJump,
    };
    use kuroya_core::{BufferHistorySnapshot, EditorSettings, TextBuffer, Workspace};
    use std::{
        collections::HashMap,
        path::PathBuf,
        time::{Duration, Instant},
    };
    use tokio::runtime::Runtime;

    #[test]
    fn failed_file_load_closes_orphaned_pending_panes() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let orphan_pane = app.insert_editor_pane_right(None);
        app.active_pane = orphan_pane;
        app.focused_pane = Some(orphan_pane);
        app.pending_pane_paths.insert(orphan_pane, path.clone());
        app.pending_open_paths.insert(path.clone());

        app.handle_file_load_event(crate::ui_events::UiEvent::FileLoadFailed {
            root: app.workspace.root.clone(),
            generation: app.workspace_event_generation,
            path: path.clone(),
            error: "denied".to_owned(),
        });

        assert_eq!(app.panes.len(), 1);
        assert!(!app.panes.iter().any(|pane| pane.id == orphan_pane));
        assert!(app.pending_pane_paths.is_empty());
        assert_eq!(app.active_pane, 1);
        assert_eq!(app.focused_pane, Some(1));
        assert_eq!(app.status, "Could not open main.rs: denied");
    }

    #[test]
    fn failed_file_load_keeps_orphan_panes_that_already_show_a_buffer() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let other_path = root.join("src/other.rs");
        let mut app = app_for_test(root);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(other_path),
            "open".to_owned(),
        ));
        let orphan_pane = app.insert_editor_pane_right(None);
        let assigned_pane = app.insert_editor_pane_right(None);
        app.assign_buffer_to_pane(assigned_pane, 7);
        app.pending_pane_paths =
            HashMap::from([(orphan_pane, path.clone()), (assigned_pane, path.clone())]);
        app.pending_open_paths.insert(path.clone());

        app.handle_file_load_event(crate::ui_events::UiEvent::FileLoadFailed {
            root: app.workspace.root.clone(),
            generation: app.workspace_event_generation,
            path: path.clone(),
            error: "denied".to_owned(),
        });

        assert!(!app.panes.iter().any(|pane| pane.id == orphan_pane));
        assert!(
            app.panes
                .iter()
                .any(|pane| pane.id == assigned_pane && pane.active == Some(7))
        );
        assert!(app.pending_pane_paths.is_empty());
    }

    #[test]
    fn failed_file_load_clears_pending_restore_state_for_path() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let other_path = root.join("src/lib.rs");
        let mut app = app_for_test(root);

        app.pending_open_paths.insert(path.clone());
        app.pending_open_paths.insert(other_path.clone());
        app.pending_pane_paths = HashMap::from([(2, path.clone()), (3, other_path.clone())]);
        app.pending_view_states
            .insert(path.clone(), view_state(path.clone()));
        app.pending_view_states
            .insert(other_path.clone(), view_state(other_path.clone()));
        app.pending_history_states
            .insert(path.clone(), history_state(path.clone()));
        app.pending_history_states
            .insert(other_path.clone(), history_state(other_path.clone()));
        app.pending_file_jump = Some(FileJump::char(path.clone(), 9, 3));
        app.pending_active_path = Some(path.clone());

        app.handle_file_load_event(crate::ui_events::UiEvent::FileLoadFailed {
            root: app.workspace.root.clone(),
            generation: app.workspace_event_generation,
            path: path.clone(),
            error: "denied".to_owned(),
        });

        assert!(!app.pending_open_paths.contains(&path));
        assert!(app.pending_open_paths.contains(&other_path));
        assert!(!app.pending_pane_paths.contains_key(&2));
        assert_eq!(app.pending_pane_paths.get(&3), Some(&other_path));
        assert!(!app.pending_view_states.contains_key(&path));
        assert!(app.pending_view_states.contains_key(&other_path));
        assert!(!app.pending_history_states.contains_key(&path));
        assert!(app.pending_history_states.contains_key(&other_path));
        assert!(app.pending_file_jump.is_none());
        assert_eq!(app.pending_active_path, None);
        assert_eq!(app.status, "Could not open main.rs: denied");
    }

    #[test]
    fn failed_file_load_clears_lexically_equivalent_pending_restore_state() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let event_path = root.join("src").join("..").join("src/main.rs");
        let other_path = root.join("src/lib.rs");
        let mut app = app_for_test(root);

        app.pending_open_paths.insert(path.clone());
        app.pending_open_paths.insert(other_path.clone());
        app.pending_pane_paths = HashMap::from([(2, path.clone()), (3, other_path.clone())]);
        app.pending_view_states
            .insert(path.clone(), view_state(path.clone()));
        app.pending_view_states
            .insert(other_path.clone(), view_state(other_path.clone()));
        app.pending_history_states
            .insert(path.clone(), history_state(path.clone()));
        app.pending_history_states
            .insert(other_path.clone(), history_state(other_path.clone()));
        app.pending_file_jump = Some(FileJump::char(path.clone(), 9, 3));
        app.pending_active_path = Some(path.clone());

        app.handle_file_load_event(crate::ui_events::UiEvent::FileLoadFailed {
            root: app.workspace.root.clone(),
            generation: app.workspace_event_generation,
            path: event_path,
            error: "denied".to_owned(),
        });

        assert!(!app.pending_open_paths.contains(&path));
        assert!(app.pending_open_paths.contains(&other_path));
        assert!(!app.pending_pane_paths.contains_key(&2));
        assert_eq!(app.pending_pane_paths.get(&3), Some(&other_path));
        assert!(!app.pending_view_states.contains_key(&path));
        assert!(app.pending_view_states.contains_key(&other_path));
        assert!(!app.pending_history_states.contains_key(&path));
        assert!(app.pending_history_states.contains_key(&other_path));
        assert!(app.pending_file_jump.is_none());
        assert_eq!(app.pending_active_path, None);
        assert_eq!(app.status, "Could not open main.rs: denied");
    }

    #[test]
    fn stale_file_load_events_are_ignored_after_workspace_reset() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root.clone());
        let stale_generation = app.workspace_event_generation;
        app.status = "before".to_owned();
        app.reset_open_workspace_state();

        app.handle_file_load_event(crate::ui_events::UiEvent::FileLoaded {
            root,
            generation: stale_generation,
            path: path.clone(),
            buffer: loaded_text_buffer(99, path, "fn main() {}\n".to_owned(), ".".to_owned()),
            elapsed: Duration::ZERO,
            activate: true,
            lossy: false,
            binary: false,
        });

        assert!(app.buffers.is_empty());
        assert_eq!(app.status, "before");
    }

    #[test]
    fn equivalent_root_file_loaded_event_is_applied() {
        let root = PathBuf::from("workspace");
        let event_root = root.join("src").join("..");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.pending_open_paths.insert(path.clone());

        app.handle_file_load_event(crate::ui_events::UiEvent::FileLoaded {
            root: event_root,
            generation: app.workspace_event_generation,
            path: path.clone(),
            buffer: loaded_text_buffer(
                99,
                path.clone(),
                "fn main() {}\n".to_owned(),
                ".".to_owned(),
            ),
            elapsed: Duration::ZERO,
            activate: true,
            lossy: false,
            binary: false,
        });

        assert_eq!(app.buffer(99).unwrap().text(), "fn main() {}\n");
        assert!(!app.pending_open_paths.contains(&path));
    }

    #[test]
    fn loaded_file_applies_pending_character_selection() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.pending_open_paths.insert(path.clone());
        app.pending_file_jump = Some(FileJump::char_selection(
            path.clone(),
            1,
            8,
            "na\u{00ef}ve".chars().count(),
        ));

        app.handle_file_load_event(crate::ui_events::UiEvent::FileLoaded {
            root: app.workspace.root.clone(),
            generation: app.workspace_event_generation,
            path: path.clone(),
            buffer: loaded_text_buffer(
                99,
                path,
                "before na\u{00ef}ve after\n".to_owned(),
                ".".to_owned(),
            ),
            elapsed: Duration::ZERO,
            activate: true,
            lossy: false,
            binary: false,
        });

        let buffer = app.buffer(99).unwrap();
        let selection_start = buffer.line_column_to_char(0, 7);
        assert_eq!(
            buffer.selections()[0].range(),
            selection_start..selection_start + "na\u{00ef}ve".chars().count()
        );
        assert!(app.pending_file_jump.is_none());
    }

    #[test]
    fn file_load_events_from_other_workspace_are_ignored() {
        let root = PathBuf::from("workspace");
        let other_root = PathBuf::from("other-workspace");
        let path = other_root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.status = "before".to_owned();

        app.handle_file_load_event(crate::ui_events::UiEvent::FileLoadFailed {
            root: other_root,
            generation: app.workspace_event_generation,
            path,
            error: "denied".to_owned(),
        });

        assert!(app.buffers.is_empty());
        assert_eq!(app.status, "before");
    }

    #[test]
    fn file_load_failure_without_pending_open_is_ignored() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.buffers.push(TextBuffer::from_text(
            1,
            Some(path.clone()),
            "open".to_owned(),
        ));
        app.pending_active_path = Some(path.clone());
        app.pending_view_states
            .insert(path.clone(), view_state(path.clone()));
        app.status = "already opened".to_owned();

        app.handle_file_load_event(crate::ui_events::UiEvent::FileLoadFailed {
            root: app.workspace.root.clone(),
            generation: app.workspace_event_generation,
            path: path.clone(),
            error: "late failure".to_owned(),
        });

        assert_eq!(app.buffer(1).unwrap().text(), "open");
        assert_eq!(app.pending_active_path, Some(path.clone()));
        assert!(app.pending_view_states.contains_key(&path));
        assert_eq!(app.status, "already opened");
    }

    #[test]
    fn file_load_failure_status_sanitizes_path_and_error_text() {
        let root = PathBuf::from("workspace");
        let path = root.join(format!("bad\n{}\u{202e}.rs", "very-long-name-".repeat(16)));
        let mut app = app_for_test(root);
        app.pending_open_paths.insert(path.clone());

        app.handle_file_load_event(crate::ui_events::UiEvent::FileLoadFailed {
            root: app.workspace.root.clone(),
            generation: app.workspace_event_generation,
            path,
            error: format!("denied\nbecause \u{202e}{}", "x".repeat(256)),
        });

        assert!(app.status.starts_with("Could not open "));
        assert!(!app.status.contains('\n'));
        assert!(!app.status.contains('\u{202e}'));
    }

    #[test]
    fn file_loaded_without_pending_open_is_ignored() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.buffers.push(TextBuffer::from_text(
            1,
            Some(path.clone()),
            "open".to_owned(),
        ));
        app.status = "after first open".to_owned();

        app.handle_file_load_event(crate::ui_events::UiEvent::FileLoaded {
            root: app.workspace.root.clone(),
            generation: app.workspace_event_generation,
            path: path.clone(),
            buffer: loaded_text_buffer(99, path, "late".to_owned(), ".".to_owned()),
            elapsed: Duration::ZERO,
            activate: true,
            lossy: false,
            binary: false,
        });

        assert_eq!(app.buffer(1).unwrap().text(), "open");
        assert!(app.active.is_none());
        assert_eq!(app.status, "after first open");
    }

    #[test]
    fn image_file_loaded_opens_read_only_preview_buffer() {
        let root = PathBuf::from("workspace");
        let path = root.join("assets/logo.png");
        let mut app = app_for_test(root);
        app.pending_open_paths.insert(path.clone());
        let preview = preview_for_test(2, 1, 12);
        let buffer = loaded_text_buffer(
            99,
            path.clone(),
            image_preview_buffer_text(&preview),
            ".".to_owned(),
        );

        app.handle_file_load_event(crate::ui_events::UiEvent::ImageFileLoaded {
            root: app.workspace.root.clone(),
            generation: app.workspace_event_generation,
            path: path.clone(),
            buffer,
            preview,
            elapsed: Duration::ZERO,
            activate: true,
        });

        let buffer = app.buffer(99).expect("image preview buffer should open");
        assert!(buffer.is_read_only());
        assert!(buffer.text().starts_with("Image preview\n2 x 1 px\n"));
        assert!(app.binary_preview_buffers.contains(&99));
        assert!(app.image_preview_buffers.contains_key(&99));
        assert_eq!(app.active, Some(99));
        assert_eq!(app.status, "Opened logo.png as image preview");
        assert!(!app.pending_open_paths.contains(&path));
    }

    #[test]
    fn file_loaded_accepts_equivalent_buffer_path_and_preserves_raw_request_path() {
        let root = PathBuf::from("workspace");
        let path = root.join("src").join("..").join("src/main.rs");
        let equivalent_path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.pending_open_paths.insert(path.clone());

        app.handle_file_load_event(crate::ui_events::UiEvent::FileLoaded {
            root: app.workspace.root.clone(),
            generation: app.workspace_event_generation,
            path: path.clone(),
            buffer: loaded_text_buffer(99, equivalent_path, "loaded".to_owned(), ".".to_owned()),
            elapsed: Duration::ZERO,
            activate: true,
            lossy: false,
            binary: false,
        });

        let buffer = app.buffer(99).expect("equivalent load should apply");
        assert_eq!(buffer.text(), "loaded");
        assert_eq!(buffer.path(), Some(&path));
        assert!(!app.pending_open_paths.contains(&path));
    }

    #[test]
    fn file_loaded_rejects_mismatched_buffer_path() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let other_path = root.join("src/lib.rs");
        let mut app = app_for_test(root);
        app.pending_open_paths.insert(path.clone());
        app.pending_active_path = Some(path.clone());
        app.pending_view_states
            .insert(path.clone(), view_state(path.clone()));

        app.handle_file_load_event(crate::ui_events::UiEvent::FileLoaded {
            root: app.workspace.root.clone(),
            generation: app.workspace_event_generation,
            path: path.clone(),
            buffer: loaded_text_buffer(1, other_path, "wrong".to_owned(), ".".to_owned()),
            elapsed: Duration::ZERO,
            activate: true,
            lossy: false,
            binary: false,
        });

        assert!(app.buffers.is_empty());
        assert!(!app.pending_open_paths.contains(&path));
        assert!(app.pending_active_path.is_none());
        assert!(!app.pending_view_states.contains_key(&path));
        assert_eq!(
            app.status,
            "Could not open main.rs: loaded buffer path did not match request"
        );
    }

    #[test]
    fn file_loaded_rejects_new_buffer_id_collision() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let other_path = root.join("src/lib.rs");
        let mut app = app_for_test(root);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(other_path),
            "existing".to_owned(),
        ));
        app.pending_open_paths.insert(path.clone());

        app.handle_file_load_event(crate::ui_events::UiEvent::FileLoaded {
            root: app.workspace.root.clone(),
            generation: app.workspace_event_generation,
            path: path.clone(),
            buffer: loaded_text_buffer(7, path.clone(), "new".to_owned(), ".".to_owned()),
            elapsed: Duration::ZERO,
            activate: true,
            lossy: false,
            binary: false,
        });

        assert_eq!(app.buffers.len(), 1);
        assert!(app.buffer(7).is_some());
        assert!(!app.pending_open_paths.contains(&path));
        assert_eq!(
            app.status,
            "Could not open main.rs: loaded buffer id is already in use"
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

    fn view_state(path: PathBuf) -> BufferViewState {
        BufferViewState {
            path,
            cursor_line: 1,
            cursor_column: 1,
            scroll_line: 1,
            horizontal_scroll_offset: 0.0,
            selections: Vec::new(),
        }
    }

    fn history_state(path: PathBuf) -> BufferHistoryState {
        BufferHistoryState {
            path,
            history: BufferHistorySnapshot {
                len_chars: 0,
                checksum: 0,
                undo: Vec::new(),
                redo: Vec::new(),
            },
        }
    }

    fn preview_for_test(width: usize, height: usize, byte_len: usize) -> LoadedImagePreview {
        LoadedImagePreview {
            width,
            height,
            rgba: Some(vec![255; width * height * 4]),
            byte_len,
        }
    }
}
