use crate::{
    KuroyaApp,
    lsp_rename_requests::{
        LspRenamePrepareAwait, LspRenamePrepareTarget, lsp_prepare_range_contains_position,
        lsp_rename_prefill_target,
    },
    lsp_runtime::{lsp_command_queue_failed_status, lsp_status_display_message},
    path_display::{compact_path, display_error_label_cow},
};
use kuroya_core::{BufferId, LspPrepareRename};
use std::path::PathBuf;

impl KuroyaApp {
    pub(crate) fn begin_lsp_rename(&mut self) {
        let Some((id, path, version, line, character)) = self.active_lsp_position() else {
            self.lsp_rename_open = false;
            self.lsp_rename_input.clear();
            self.clear_lsp_rename_preview_state();
            self.lsp_rename_prepare = None;
            self.lsp_rename_prepare_pending = None;
            self.status = "No LSP rename target".to_owned();
            return;
        };

        self.clear_lsp_rename_preview_state();
        self.lsp_rename_prepare = None;
        self.lsp_rename_prepare_pending = None;
        self.lsp_rename_input = self
            .active_buffer()
            .and_then(|buffer| {
                buffer
                    .selected_text()
                    .filter(|text| !text.contains('\n'))
                    .or_else(|| buffer.word_at_cursor())
            })
            .and_then(|text| lsp_rename_prefill_target(&text))
            .unwrap_or_default();

        let Some(client) = self.ensure_lsp_for_buffer(id) else {
            self.lsp_rename_open = true;
            self.status = "Rename symbol".to_owned();
            return;
        };

        let prepare_supported = client
            .capabilities()
            .map(|capabilities| capabilities.prepare_rename_supported)
            .unwrap_or(false);
        if !prepare_supported {
            self.lsp_rename_open = true;
            self.status = "Rename symbol".to_owned();
            return;
        }

        if !client.prepare_rename(id, path.clone(), version, line, character) {
            self.status = lsp_command_queue_failed_status("textDocument/prepareRename");
            return;
        }
        self.record_lsp_client_trace(
            "textDocument/prepareRename",
            format!(
                "{}:{}:{}",
                compact_path(&path),
                line.saturating_add(1),
                character.saturating_add(1)
            ),
        );
        self.lsp_rename_prepare_pending = Some(LspRenamePrepareAwait {
            id,
            path,
            version,
            line,
            character,
        });
        self.status = "Checking rename target...".to_owned();
    }

    pub(crate) fn handle_lsp_prepare_rename_result(
        &mut self,
        id: BufferId,
        path: PathBuf,
        version: u64,
        line: usize,
        column: usize,
        range: Option<LspPrepareRename>,
        error: Option<String>,
    ) {
        let Some(pending) = self.lsp_rename_prepare_pending.as_ref() else {
            return;
        };
        if pending.id != id
            || pending.path != path
            || pending.version != version
            || pending.line != line
            || pending.character != column
        {
            return;
        }
        self.lsp_rename_prepare_pending = None;
        if self.lsp_rename_open {
            return;
        }

        if let Some(error) = error {
            let error_label = display_error_label_cow(&error);
            self.status = lsp_status_display_message(&format!("Rename unavailable: {error_label}"));
            return;
        }

        let Some(range) = range else {
            self.status = "Rename is not available at this position".to_owned();
            return;
        };

        if self.active_lsp_position() != Some((id, path.clone(), version, line, column)) {
            self.status = "Rename position changed; start rename again".to_owned();
            return;
        }
        if !lsp_prepare_range_contains_position(
            range.start_line,
            range.start_column,
            range.end_line,
            range.end_column,
            line,
            column,
        ) {
            self.status = "Rename is not available at this position".to_owned();
            return;
        }

        if let Some(placeholder) = range
            .placeholder
            .as_deref()
            .and_then(lsp_rename_prefill_target)
        {
            self.lsp_rename_input = placeholder;
        }
        self.lsp_rename_prepare = Some(LspRenamePrepareTarget {
            id,
            path,
            version,
            line,
            character: column,
            start_line: range.start_line,
            start_column: range.start_column,
            end_line: range.end_line,
            end_column: range.end_column,
        });
        self.lsp_rename_open = true;
        self.status = "Rename symbol".to_owned();
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        KuroyaApp, app_startup_context::AppStartupContext, lsp_client::LspClientCommand,
        lsp_client::LspClientHandle, lsp_client::LspServerCapabilities,
        lsp_runtime::lsp_client_key, terminal::TerminalPane,
    };
    use kuroya_core::{
        EditorSettings, LspPrepareRename, LspServerConfig, LspTextEdit, TextBuffer, Workspace,
    };
    use std::{path::PathBuf, time::Instant};
    use tokio::{runtime::Runtime, sync::mpsc};

    #[test]
    fn begin_lsp_rename_closes_stale_popup_when_no_active_target() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root.clone());
        app.lsp_rename_open = true;
        app.lsp_rename_input = "stale_name".to_owned();
        app.lsp_rename_preview_open = true;
        app.lsp_rename_preview_new_name = "stale".to_owned();
        app.lsp_rename_preview_edits = vec![text_edit(root.join("src/main.rs"))];

        app.begin_lsp_rename();

        assert!(!app.lsp_rename_open);
        assert!(app.lsp_rename_input.is_empty());
        assert!(!app.lsp_rename_preview_open);
        assert!(app.lsp_rename_preview_edits.is_empty());
        assert!(app.lsp_rename_prepare.is_none());
        assert!(app.lsp_rename_prepare_pending.is_none());
        assert_eq!(app.status, "No LSP rename target");
    }

    #[test]
    fn begin_lsp_rename_skips_prepare_and_opens_popup_when_capability_is_absent() {
        let root = PathBuf::from("workspace");
        let source = root.join("src/main.rs");
        let (mut app, _rx) = app_for_test_with_server(root, false);
        open_active_buffer(&mut app, 7, source, 3);

        app.begin_lsp_rename();

        assert!(
            app.lsp_rename_open,
            "without an advertised prepareProvider the popup opens immediately"
        );
        assert_eq!(app.status, "Rename symbol");
        assert!(app.lsp_rename_prepare_pending.is_none());
        assert_eq!(app.lsp_rename_input, "main");
    }

    #[test]
    fn begin_lsp_rename_sends_prepare_and_keeps_popup_closed_until_response() {
        let root = PathBuf::from("workspace");
        let source = root.join("src/main.rs");
        let (mut app, mut rx) = app_for_test_with_server(root, true);
        let version = open_active_buffer(&mut app, 7, source, 3);

        app.begin_lsp_rename();

        assert!(
            !app.lsp_rename_open,
            "the popup must wait for the prepareRename result"
        );
        assert_eq!(app.status, "Checking rename target...");
        let Some(pending) = app.lsp_rename_prepare_pending else {
            panic!("expected a pending prepareRename request");
        };
        assert_eq!((pending.id, pending.line, pending.character), (7, 0, 3));
        assert_eq!(pending.version, version);
        match rx.try_recv() {
            Ok(LspClientCommand::PrepareRename {
                id,
                line,
                character,
                ..
            }) => assert_eq!((id, line, character), (7, 0, 3)),
            other => panic!("expected a prepareRename command, got {other:?}"),
        }
    }

    #[test]
    fn prepare_result_opens_popup_for_valid_range_and_placeholder() {
        let root = PathBuf::from("workspace");
        let source = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let version = open_active_buffer(&mut app, 7, source.clone(), 3);
        app.lsp_rename_prepare_pending = Some(crate::lsp_rename_requests::LspRenamePrepareAwait {
            id: 7,
            path: source.clone(),
            version,
            line: 0,
            character: 3,
        });
        app.lsp_rename_input.clear();

        app.handle_lsp_prepare_rename_result(
            7,
            source,
            version,
            0,
            3,
            Some(LspPrepareRename {
                start_line: 1,
                start_column: 4,
                end_line: 1,
                end_column: 8,
                placeholder: Some("main".to_owned()),
            }),
            None,
        );

        assert!(
            app.lsp_rename_open,
            "a valid prepare result opens the popup"
        );
        assert!(app.lsp_rename_prepare_pending.is_none());
        let prepare = app
            .lsp_rename_prepare
            .expect("validated prepare target retained");
        assert_eq!(
            (prepare.start_line, prepare.start_column),
            (1, 4),
            "prepare range stores one-based coordinates"
        );
        assert_eq!(
            (prepare.end_line, prepare.end_column),
            (1, 8),
            "prepare range stores one-based coordinates"
        );
        assert!(prepare.contains_position(0, 3));
        assert_eq!(app.lsp_rename_input, "main", "placeholder prefills input");
        assert_eq!(app.status, "Rename symbol");
    }

    #[test]
    fn prepare_result_without_range_keeps_popup_closed() {
        let root = PathBuf::from("workspace");
        let source = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let version = open_active_buffer(&mut app, 7, source.clone(), 3);
        app.lsp_rename_prepare_pending = Some(crate::lsp_rename_requests::LspRenamePrepareAwait {
            id: 7,
            path: source.clone(),
            version,
            line: 0,
            character: 3,
        });
        app.status = "Checking rename target...".to_owned();

        app.handle_lsp_prepare_rename_result(7, source, version, 0, 3, None, None);

        assert!(!app.lsp_rename_open);
        assert!(app.lsp_rename_prepare.is_none());
        assert!(app.lsp_rename_prepare_pending.is_none());
        assert_eq!(app.status, "Rename is not available at this position");
    }

    #[test]
    fn stale_prepare_results_are_ignored() {
        let root = PathBuf::from("workspace");
        let source = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let version = open_active_buffer(&mut app, 7, source.clone(), 3);
        app.lsp_rename_prepare_pending = Some(crate::lsp_rename_requests::LspRenamePrepareAwait {
            id: 7,
            path: source.clone(),
            version,
            line: 0,
            character: 3,
        });

        app.handle_lsp_prepare_rename_result(
            7,
            source,
            version,
            5,
            5,
            Some(LspPrepareRename {
                start_line: 1,
                start_column: 1,
                end_line: 1,
                end_column: 3,
                placeholder: None,
            }),
            None,
        );

        assert!(app.lsp_rename_prepare_pending.is_some());
        assert!(!app.lsp_rename_open);
        assert!(app.lsp_rename_prepare.is_none());
    }

    fn text_edit(path: PathBuf) -> LspTextEdit {
        LspTextEdit {
            path,
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 5,
            new_text: "renamed".to_owned(),
        }
    }

    fn open_active_buffer(
        app: &mut KuroyaApp,
        id: kuroya_core::BufferId,
        path: PathBuf,
        cursor_char: usize,
    ) -> u64 {
        let mut buffer = TextBuffer::from_text(id, Some(path), "fn main() {}\n".to_owned());
        buffer.set_single_cursor(cursor_char);
        let version = buffer.version();
        app.buffers.push(buffer);
        app.active = Some(id);
        version
    }

    fn app_for_test_with_server(
        root: PathBuf,
        prepare: bool,
    ) -> (KuroyaApp, mpsc::Receiver<LspClientCommand>) {
        let settings = EditorSettings {
            lsp_servers: vec![LspServerConfig {
                language: "rust".to_owned(),
                command: "rust-analyzer".to_owned(),
                args: Vec::new(),
                extensions: Vec::new(),
                root_markers: vec!["Cargo.toml".to_owned()],
                enabled: true,
            }],
            ..EditorSettings::default()
        };
        let mut app = app_for_test_with_settings(root, settings);
        let configs = app.settings.lsp_server_configs();
        let key = lsp_client_key(
            configs.first().expect("configured rust server resolves"),
            &configs,
        );
        let (tx, rx) = mpsc::channel(16);
        let handle = LspClientHandle::from_sender_for_test(tx, 1);
        handle.set_capabilities_for_test(LspServerCapabilities {
            rename_provider: true,
            prepare_rename_supported: prepare,
            ..LspServerCapabilities::default()
        });
        app.lsp_clients.insert(key, handle);
        (app, rx)
    }

    fn app_for_test_with_settings(root: PathBuf, settings: EditorSettings) -> KuroyaApp {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
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

    fn app_for_test(root: PathBuf) -> KuroyaApp {
        app_for_test_with_settings(root, EditorSettings::default())
    }
}
