use crate::{
    KuroyaApp,
    lsp_rename_requests::{
        lsp_rename_display_label, lsp_rename_request_target, lsp_rename_target_error_status,
    },
    lsp_runtime::lsp_command_queue_failed_status,
    path_display::compact_path,
    ui_text::truncate_middle,
};

const LSP_RENAME_SUBMIT_LABEL_MAX_CHARS: usize = 64;

impl KuroyaApp {
    pub(crate) fn submit_lsp_rename(&mut self) {
        let new_name = match lsp_rename_request_target(&self.lsp_rename_input) {
            Ok(new_name) => new_name,
            Err(error) => {
                self.status = lsp_rename_target_error_status(error);
                return;
            }
        };
        let Some((id, path, version, line, character)) = self.active_lsp_position() else {
            self.lsp_rename_open = false;
            self.status = "No LSP rename target".to_owned();
            return;
        };
        let Some(client) = self.ensure_lsp_for_buffer(id) else {
            self.status = "No LSP server configured for this buffer".to_owned();
            return;
        };

        if let Some(prepare) = &self.lsp_rename_prepare {
            if !prepare.matches_position(id, &path, version, line, character) {
                self.status = "Rename position changed; cancel and rename again".to_owned();
                return;
            }
            if !prepare.contains_position(line, character) {
                self.status = "Rename is not available at this position".to_owned();
                return;
            }
        }

        if !client.rename(id, path.clone(), version, line, character, new_name.clone()) {
            self.status = lsp_command_queue_failed_status("textDocument/rename");
            return;
        }
        let new_name_label = lsp_rename_submit_label(&new_name);
        let path_label = lsp_rename_submit_label(&compact_path(&path));
        let display_line = line.saturating_add(1);
        let display_character = character.saturating_add(1);
        self.record_lsp_client_trace(
            "textDocument/rename",
            format!(
                "{path_label}:{}:{} `{new_name_label}`",
                display_line, display_character
            ),
        );
        self.lsp_rename_open = false;
        self.lsp_rename_prepare = None;
        self.lsp_rename_prepare_pending = None;
        self.status = format!(
            "Requesting rename at {path_label}:{}:{} to `{new_name_label}`",
            display_line, display_character
        );
    }
}

fn lsp_rename_submit_label(text: &str) -> String {
    truncate_middle(
        &lsp_rename_display_label(text),
        LSP_RENAME_SUBMIT_LABEL_MAX_CHARS,
    )
}

#[cfg(test)]
mod tests {
    use super::{LSP_RENAME_SUBMIT_LABEL_MAX_CHARS, lsp_rename_submit_label};
    use crate::{
        KuroyaApp, app_startup_context::AppStartupContext, lsp_client::LspClientCommand,
        lsp_client::LspClientHandle, lsp_client::LspServerCapabilities,
        lsp_rename_requests::LspRenamePrepareTarget, lsp_runtime::lsp_client_key,
        terminal::TerminalPane,
    };
    use kuroya_core::{EditorSettings, LspServerConfig, TextBuffer, Workspace};
    use std::{path::PathBuf, time::Instant};
    use tokio::{runtime::Runtime, sync::mpsc};

    #[test]
    fn submit_lsp_rename_closes_stale_popup_when_no_active_target() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        app.lsp_rename_open = true;
        app.lsp_rename_input = "renamed_symbol".to_owned();
        app.status = "unchanged".to_owned();

        app.submit_lsp_rename();

        assert!(!app.lsp_rename_open);
        assert_eq!(app.lsp_rename_input, "renamed_symbol");
        assert_eq!(app.status, "No LSP rename target");
    }

    #[test]
    fn rename_submit_label_escapes_and_bounds_display_text() {
        let raw = format!(
            "path\n{}\t\u{202e}tail",
            "segment-".repeat(LSP_RENAME_SUBMIT_LABEL_MAX_CHARS)
        );
        let label = lsp_rename_submit_label(&raw);

        assert!(raw.contains('\n'));
        assert!(raw.contains('\u{202e}'));
        assert!(!label.contains('\n'));
        assert!(!label.contains('\t'));
        assert!(!label.contains('\u{202e}'));
        assert!(label.contains("..."));
        assert!(label.chars().count() <= LSP_RENAME_SUBMIT_LABEL_MAX_CHARS);
    }

    #[test]
    fn submit_lsp_rename_sends_rename_directly_without_a_prepare_result() {
        let root = PathBuf::from("workspace");
        let source = root.join("src/main.rs");
        let (mut app, mut rx) = app_for_test_with_server(root, false);
        open_active_buffer(&mut app, 7, source.clone(), 3);
        app.lsp_rename_open = true;
        app.lsp_rename_input = "renamed_symbol".to_owned();

        app.submit_lsp_rename();

        assert!(!app.lsp_rename_open);
        match rx.try_recv() {
            Ok(LspClientCommand::Rename { new_name, .. }) => {
                assert_eq!(new_name, "renamed_symbol");
            }
            other => panic!("expected a rename command, got {other:?}"),
        }
        assert!(rx.try_recv().is_err(), "no other command may be queued");
    }

    #[test]
    fn submit_lsp_rename_requires_a_still_valid_prepare_result() {
        let root = PathBuf::from("workspace");
        let source = root.join("src/main.rs");
        let (mut app, mut rx) = app_for_test_with_server(root, true);
        let version = open_active_buffer(&mut app, 7, source.clone(), 3);
        app.lsp_rename_open = true;
        app.lsp_rename_input = "renamed_symbol".to_owned();
        app.lsp_rename_prepare = Some(LspRenamePrepareTarget {
            id: 7,
            path: source,
            version,
            line: 0,
            character: 3,
            start_line: 1,
            start_column: 4,
            end_line: 1,
            end_column: 8,
        });
        app.status = "unchanged".to_owned();

        app.submit_lsp_rename();

        match rx.try_recv() {
            Ok(LspClientCommand::Rename { .. }) => {}
            other => panic!("expected the rename command, got {other:?}"),
        }
        assert!(!app.lsp_rename_open);
        assert!(app.lsp_rename_prepare.is_none());
        assert!(app.lsp_rename_prepare_pending.is_none());
        assert!(
            app.status.starts_with("Requesting rename at "),
            "{}",
            app.status
        );
        assert!(app.status.contains("`renamed_symbol`"), "{}", app.status);
    }

    #[test]
    fn submit_lsp_rename_rejects_a_drifted_prepare_position() {
        let root = PathBuf::from("workspace");
        let source = root.join("src/main.rs");
        let (mut app, mut rx) = app_for_test_with_server(root, true);
        let version = open_active_buffer(&mut app, 7, source.clone(), 3);
        app.lsp_rename_open = true;
        app.lsp_rename_input = "renamed_symbol".to_owned();

        app.lsp_rename_prepare = Some(LspRenamePrepareTarget {
            id: 7,
            path: source,
            version: version.wrapping_add(1),
            line: 0,
            character: 3,
            start_line: 1,
            start_column: 4,
            end_line: 1,
            end_column: 8,
        });
        app.status = "unchanged".to_owned();

        app.submit_lsp_rename();

        assert!(app.lsp_rename_open, "the popup stays open for a retry");
        assert!(app.lsp_rename_prepare.is_some());
        assert!(rx.try_recv().is_err(), "no rename may be queued");
        assert_eq!(
            app.status,
            "Rename position changed; cancel and rename again"
        );
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
