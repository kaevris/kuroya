use crate::{
    KuroyaApp,
    lsp_lifecycle::{
        background_language_block_reason, lsp_lifecycle_target_for_buffer,
        lsp_lifecycle_targets_for_buffers, lsp_server_config_for_buffer,
    },
    lsp_runtime::{
        LSP_SYMBOL_REFRESH_DEBOUNCE, lsp_command_queue_failed_status,
        lsp_server_configs_for_settings, record_pending_lsp_resync_path,
        take_due_lsp_symbol_refresh_ids,
    },
    lsp_text_positions::lsp_line_content_utf16_len,
    path_display::display_path_label_cow,
};
use kuroya_core::{BufferId, TextBuffer, TextSnapshot};
use std::{
    borrow::Cow,
    collections::HashSet,
    path::{Path, PathBuf},
    time::Instant,
};

impl KuroyaApp {
    /// didOpen fans out to EVERY live client matching the buffer (each client
    /// tracks its own sync state); follow-up feature requests (symbols, inlay
    /// hints, code lens, semantic tokens) go to the PRIMARY client only.
    pub(crate) fn notify_lsp_open(&mut self, id: BufferId) {
        let Some((path, language, version)) = self.lsp_document_sync_target(id) else {
            return;
        };
        let clients = self.ensure_lsp_clients_for_buffer(id);
        let Some(text) = self.lsp_text_snapshot_for_version(id, version) else {
            return;
        };

        let mut primary_synced = true;
        for (index, client) in clients.iter().enumerate() {
            if client.did_open(id, path.clone(), language.clone(), version, text.clone()) {
                // Track the open document so didClose only fans out to
                // clients that actually hold it open.
                client.open_documents().note_open(&path);
                self.record_lsp_client_trace(
                    "textDocument/didOpen",
                    lsp_document_version_trace_label(&path, version),
                );
            } else {
                self.status = lsp_command_queue_failed_status("textDocument/didOpen");
                record_pending_lsp_resync_path(&mut self.pending_lsp_resync, path.clone());
                if index == 0 {
                    primary_synced = false;
                }
            }
        }
        self.pending_lsp_symbol_refreshes.remove(&id);
        if primary_synced && let Some(primary) = clients.first() {
            self.request_lsp_symbol_refreshes(primary, id, &path, version);
        }
    }

    /// didChange fans out to EVERY live client matching the buffer.
    pub(crate) fn notify_lsp_change(&mut self, id: BufferId) {
        let Some((path, _language, version)) = self.lsp_document_sync_target(id) else {
            return;
        };
        let clients = self.ensure_lsp_clients_for_buffer(id);
        let Some(text) = self.lsp_text_snapshot_for_version(id, version) else {
            return;
        };

        let mut primary_synced = true;
        for (index, client) in clients.iter().enumerate() {
            if client.did_change(id, path.clone(), version, text.clone()) {
                self.record_lsp_client_trace(
                    "textDocument/didChange",
                    lsp_document_version_trace_label(&path, version),
                );
            } else {
                self.status = lsp_command_queue_failed_status("textDocument/didChange");
                record_pending_lsp_resync_path(&mut self.pending_lsp_resync, path.clone());
                if index == 0 {
                    primary_synced = false;
                }
            }
        }
        if primary_synced {
            self.schedule_lsp_symbol_refresh(id);
        }
    }

    /// didSave fans out to EVERY live client matching the buffer.
    pub(crate) fn notify_lsp_save(&mut self, id: BufferId) {
        let Some(buffer) = self.buffer(id) else {
            return;
        };
        if background_language_block_reason(
            id,
            buffer,
            &self.lossy_decoded_buffers,
            &self.binary_preview_buffers,
        )
        .is_some()
        {
            return;
        }
        let Some(path) = buffer.path().cloned() else {
            return;
        };

        for client in self.ensure_lsp_clients_for_buffer(id) {
            if client.did_save(path.clone()) {
                self.record_lsp_client_trace(
                    "textDocument/didSave",
                    lsp_document_trace_path_label(&path),
                );
            } else {
                self.status = lsp_command_queue_failed_status("textDocument/didSave");
            }
        }
    }

    /// didClose fans out to the live clients for the buffer's language that
    /// actually hold the document open (tracked since didOpen); missing
    /// clients are skipped (their restart ladder still applies per client
    /// key). A client with no tracked documents at all (never saw a didOpen
    /// for this session) keeps the legacy language-wide fan-out for backward
    /// safety.
    pub(crate) fn notify_lsp_close(&mut self, id: BufferId) {
        let Some(buffer) = self.buffer(id) else {
            return;
        };
        let lsp_configs = lsp_server_configs_for_settings(&self.settings);
        let Some((language, path)) = lsp_lifecycle_target_for_buffer(
            buffer,
            &lsp_configs,
            &self.plugin_languages,
            &self.lossy_decoded_buffers,
            &self.binary_preview_buffers,
        ) else {
            return;
        };

        for client in self.live_lsp_clients_for_language(&language) {
            if !client_should_receive_did_close(&client, &path) {
                continue;
            }
            if client.did_close(path.clone()) {
                client.open_documents().note_closed(&path);
                self.record_lsp_client_trace(
                    "textDocument/didClose",
                    lsp_document_trace_path_label(&path),
                );
            } else {
                self.status = lsp_command_queue_failed_status("textDocument/didClose");
            }
        }
        self.pending_lsp_symbol_refreshes.remove(&id);
    }

    pub(crate) fn notify_lsp_close_all(&mut self) {
        for (language, path) in lsp_lifecycle_targets_for_buffers(
            &self.buffers,
            &lsp_server_configs_for_settings(&self.settings),
            &self.plugin_languages,
            &self.lossy_decoded_buffers,
            &self.binary_preview_buffers,
        ) {
            for client in self.live_lsp_clients_for_language(&language) {
                if !client_should_receive_did_close(&client, &path) {
                    continue;
                }
                if client.did_close(path.clone()) {
                    client.open_documents().note_closed(&path);
                    self.record_lsp_client_trace(
                        "textDocument/didClose",
                        lsp_document_trace_path_label(&path),
                    );
                }
            }
        }
    }

    pub(crate) fn flush_pending_lsp_symbol_refreshes(&mut self) -> usize {
        let ids = take_due_lsp_symbol_refresh_ids(
            &mut self.pending_lsp_symbol_refreshes,
            Instant::now(),
            LSP_SYMBOL_REFRESH_DEBOUNCE,
        );
        let mut count = 0usize;
        for id in ids {
            if self.request_lsp_symbol_refresh_for_buffer(id) {
                count = count.saturating_add(1);
            }
        }
        count
    }

    pub(crate) fn schedule_lsp_symbol_refreshes_for_open_buffers(&mut self) -> usize {
        let now = Instant::now();
        let scheduled_at = now.checked_sub(LSP_SYMBOL_REFRESH_DEBOUNCE).unwrap_or(now);
        let mut count = 0usize;
        let mut eligible_ids = HashSet::new();

        for buffer in &self.buffers {
            if lsp_symbol_refresh_buffer_is_eligible(
                buffer,
                &self.lossy_decoded_buffers,
                &self.binary_preview_buffers,
            ) {
                let id = buffer.id();
                eligible_ids.insert(id);
                self.pending_lsp_symbol_refreshes.insert(id, scheduled_at);
                count = count.saturating_add(1);
            }
        }

        self.pending_lsp_symbol_refreshes
            .retain(|id, _| eligible_ids.contains(id));
        count
    }

    fn schedule_lsp_symbol_refresh(&mut self, id: BufferId) {
        self.pending_lsp_symbol_refreshes.insert(id, Instant::now());
    }

    fn request_lsp_symbol_refresh_for_buffer(&mut self, id: BufferId) -> bool {
        let Some((path, version)) = self.lsp_symbol_refresh_target(id) else {
            return false;
        };
        let Some(client) = self.ensure_lsp_for_buffer(id) else {
            return false;
        };

        self.request_lsp_symbol_refreshes(&client, id, &path, version)
    }

    fn lsp_symbol_refresh_target(&self, id: BufferId) -> Option<(PathBuf, u64)> {
        let buffer = self.buffer(id)?;
        lsp_symbol_refresh_target_for_buffer(
            buffer,
            &self.lossy_decoded_buffers,
            &self.binary_preview_buffers,
        )
    }

    fn request_lsp_symbol_refreshes(
        &mut self,
        client: &crate::lsp_client::LspClientHandle,
        id: BufferId,
        path: &Path,
        version: u64,
    ) -> bool {
        let mut queued = false;
        let trace_label = lsp_document_trace_path_label(path);
        let path_buf = path.to_path_buf();
        let inlay_hint_range = if self.settings.inlay_hints {
            self.lsp_inlay_hint_range_end(id)
        } else {
            None
        };
        // Provider capability gates: servers that never advertised a
        // provider are not asked for it. Until the initialize response has
        // been processed (capabilities unknown) the legacy behavior — send
        // everything — applies so under-declaring servers keep working.
        let capabilities = client.capabilities();
        let inlay_hints_advertised = capabilities
            .map(|capabilities| capabilities.inlay_hint_provider)
            .unwrap_or(true);
        let code_lens_advertised = capabilities
            .map(|capabilities| capabilities.code_lens_provider)
            .unwrap_or(true);
        let semantic_tokens_advertised = capabilities
            .map(|capabilities| capabilities.semantic_tokens_provider)
            .unwrap_or(true);

        if self.settings.inlay_hints && inlay_hints_advertised {
            if let Some((end_line, end_character)) = inlay_hint_range {
                if client.inlay_hints(id, path_buf.clone(), version, end_line, end_character) {
                    self.record_lsp_client_trace("textDocument/inlayHint", trace_label.clone());
                    queued = true;
                } else {
                    self.status = lsp_command_queue_failed_status("textDocument/inlayHint");
                }
            }
        } else {
            self.inlay_hints.remove(path);
        }

        if self.settings.code_lens && code_lens_advertised {
            if client.code_lenses(id, path_buf.clone(), version) {
                self.record_lsp_client_trace("textDocument/codeLens", trace_label.clone());
                queued = true;
            } else {
                self.status = lsp_command_queue_failed_status("textDocument/codeLens");
            }
        } else {
            self.code_lenses.remove(path);
        }

        if semantic_tokens_advertised {
            if client.semantic_tokens(id, path_buf, version) {
                self.record_lsp_client_trace("textDocument/semanticTokens/full", trace_label);
                queued = true;
            } else {
                self.status = lsp_command_queue_failed_status("textDocument/semanticTokens/full");
            }
        } else {
            self.semantic_tokens.remove(path);
        }
        queued
    }

    fn lsp_document_sync_target(&self, id: BufferId) -> Option<(PathBuf, String, u64)> {
        let buffer = self.buffer(id)?;
        if background_language_block_reason(
            id,
            buffer,
            &self.lossy_decoded_buffers,
            &self.binary_preview_buffers,
        )
        .is_some()
        {
            return None;
        }
        let lsp_configs = lsp_server_configs_for_settings(&self.settings);
        let (_, language) =
            lsp_server_config_for_buffer(&lsp_configs, &self.plugin_languages, buffer)?;
        Some((
            buffer.path()?.clone(),
            language.into_owned(),
            buffer.version(),
        ))
    }

    fn lsp_text_snapshot_for_version(&self, id: BufferId, version: u64) -> Option<TextSnapshot> {
        let buffer = self.buffer(id)?;
        (buffer.version() == version).then(|| buffer.text_snapshot())
    }

    fn lsp_inlay_hint_range_end(&self, id: BufferId) -> Option<(usize, usize)> {
        self.buffer(id).map(inlay_hint_range_end)
    }
}

fn lsp_symbol_refresh_target_for_buffer(
    buffer: &TextBuffer,
    lossy_buffers: &HashSet<BufferId>,
    binary_buffers: &HashSet<BufferId>,
) -> Option<(PathBuf, u64)> {
    if !lsp_symbol_refresh_buffer_is_eligible(buffer, lossy_buffers, binary_buffers) {
        return None;
    }

    Some((buffer.path()?.clone(), buffer.version()))
}

/// Whether this client should receive the didClose for `path`. Clients that
/// track at least one open document only get the notification if they hold
/// THIS document; clients with an empty tracking table never saw a didOpen
/// (unknown state) and keep the legacy language-wide fan-out.
fn client_should_receive_did_close(
    client: &crate::lsp_client::LspClientHandle,
    path: &Path,
) -> bool {
    let open_documents = client.open_documents();
    open_documents.is_empty() || open_documents.contains(path)
}

fn lsp_symbol_refresh_buffer_is_eligible(
    buffer: &TextBuffer,
    lossy_buffers: &HashSet<BufferId>,
    binary_buffers: &HashSet<BufferId>,
) -> bool {
    let id = buffer.id();
    if background_language_block_reason(id, buffer, lossy_buffers, binary_buffers).is_some() {
        return false;
    }

    buffer.path().is_some()
}

fn lsp_document_version_trace_label(path: &Path, version: u64) -> String {
    format!("{} v{version}", lsp_document_trace_path_label(path))
}

fn lsp_document_trace_path_label(path: &Path) -> Cow<'_, str> {
    display_path_label_cow(path)
}

fn inlay_hint_range_end(buffer: &kuroya_core::TextBuffer) -> (usize, usize) {
    let end_line = buffer.len_lines().saturating_sub(1);
    let end_character = lsp_line_content_utf16_len(buffer, end_line).unwrap_or_default();
    (end_line, end_character)
}

#[cfg(test)]
mod tests {
    use super::{
        inlay_hint_range_end, lsp_document_trace_path_label, lsp_document_version_trace_label,
        lsp_symbol_refresh_buffer_is_eligible, lsp_symbol_refresh_target_for_buffer,
    };
    use crate::{
        KuroyaApp, app_startup_context::AppStartupContext, lsp_client::LspClientCommand,
        lsp_client::LspClientHandle, lsp_runtime::LSP_SYMBOL_REFRESH_DEBOUNCE,
        lsp_runtime::due_lsp_symbol_refresh_ids, lsp_runtime::lsp_client_key,
        path_display::DISPLAY_PATH_LABEL_MAX_CHARS, terminal::TerminalPane,
    };
    use kuroya_core::{EditorSettings, LspServerConfig, TextBuffer, Workspace};
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };
    use tokio::{runtime::Runtime, sync::mpsc};

    #[test]
    fn notifications_fan_out_to_every_client_and_requests_stay_on_the_primary() {
        let root = temp_root("multi-server-fan-out");
        let source = root.join("src").join("main.rs");
        let settings = EditorSettings {
            lsp_servers: vec![
                LspServerConfig {
                    language: "rust".to_owned(),
                    command: "rust-analyzer".to_owned(),
                    args: Vec::new(),
                    extensions: Vec::new(),
                    root_markers: vec!["Cargo.toml".to_owned()],
                    enabled: true,
                },
                LspServerConfig {
                    language: "rust".to_owned(),
                    command: "rust-analyzer-obsidian".to_owned(),
                    args: vec!["--stdio".to_owned()],
                    extensions: Vec::new(),
                    root_markers: vec!["Cargo.toml".to_owned()],
                    enabled: true,
                },
            ],
            ..EditorSettings::default()
        };
        let mut app = app_for_test_with_settings(root.clone(), settings);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(source.clone()),
            "fn main() {}\n".to_owned(),
        ));

        let configs = app.settings.lsp_server_configs();
        let primary_config = configs
            .iter()
            .find(|config| config.command == "rust-analyzer")
            .expect("primary rust config")
            .clone();
        let secondary_config = configs
            .iter()
            .find(|config| config.command == "rust-analyzer-obsidian")
            .expect("secondary rust config")
            .clone();
        let primary_key = lsp_client_key(&primary_config, &configs);
        let secondary_key = lsp_client_key(&secondary_config, &configs);
        assert_ne!(
            primary_key, secondary_key,
            "both servers need their own key"
        );

        let (primary_tx, mut primary_rx) = mpsc::channel(64);
        let (secondary_tx, mut secondary_rx) = mpsc::channel(64);
        app.lsp_clients.insert(
            primary_key,
            LspClientHandle::from_sender_for_test(primary_tx, 10),
        );
        app.lsp_clients.insert(
            secondary_key,
            LspClientHandle::from_sender_for_test(secondary_tx, 11),
        );

        // Interactive resolution returns the primary only.
        assert_eq!(
            app.ensure_lsp_for_buffer(7)
                .map(|client| client.generation()),
            Some(10)
        );

        app.notify_lsp_open(7);
        app.notify_lsp_change(7);
        app.notify_lsp_save(7);
        app.notify_lsp_close(7);

        for (name, rx) in [
            ("primary", &mut primary_rx),
            ("secondary", &mut secondary_rx),
        ] {
            let mut did_open = false;
            let mut did_change = false;
            let mut did_save = false;
            let mut did_close = false;
            while let Ok(command) = rx.try_recv() {
                match command {
                    LspClientCommand::DidOpen { id, path, .. } => {
                        assert_eq!((id, path), (7, source.clone()));
                        did_open = true;
                    }
                    LspClientCommand::DidChange { id, path, .. } => {
                        assert_eq!((id, path), (7, source.clone()));
                        did_change = true;
                    }
                    LspClientCommand::DidSave { path } => {
                        assert_eq!(path, source);
                        did_save = true;
                    }
                    LspClientCommand::DidClose { path } => {
                        assert_eq!(path, source);
                        did_close = true;
                    }
                    // Primary-only feature requests (symbol refreshes).
                    _ => {}
                }
            }
            assert!(did_open, "{name} client should receive didOpen");
            assert!(did_change, "{name} client should receive didChange");
            assert!(did_save, "{name} client should receive didSave");
            assert!(did_close, "{name} client should receive didClose");
        }

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refresh_requests_are_gated_by_advertised_provider_capabilities() {
        let root = temp_root("capability-gated-refreshes");
        let source = root.join("src").join("main.rs");
        let mut app = app_for_test(root.clone());
        let version = {
            app.buffers.push(TextBuffer::from_text(
                7,
                Some(source.clone()),
                "fn main() {}\n".to_owned(),
            ));
            app.buffer(7).expect("buffer").version()
        };
        let configs = app.settings.lsp_server_configs();
        let key = lsp_client_key(
            configs
                .iter()
                .find(|config| config.language == "rust")
                .expect("default rust config"),
            &configs,
        );
        let (gated_tx, mut gated_rx) = mpsc::channel(64);
        let gated_handle = LspClientHandle::from_sender_for_test(gated_tx, 21);
        gated_handle.set_capabilities_for_test(crate::lsp_client::LspServerCapabilities {
            rename_provider: true,
            prepare_rename_supported: true,
            // None of the three refresh providers advertised.
            semantic_tokens_provider: false,
            inlay_hint_provider: false,
            code_lens_provider: false,
        });
        app.lsp_clients.insert(key.clone(), gated_handle);

        // didOpen still syncs the document; only the feature requests are gated.
        app.notify_lsp_open(7);
        let mut saw_request = false;
        while let Ok(command) = gated_rx.try_recv() {
            if !matches!(
                command,
                LspClientCommand::DidOpen { .. } | LspClientCommand::DidChange { .. }
            ) {
                saw_request = true;
            }
        }
        assert!(
            !saw_request,
            "a server without providers must not receive inlay/codeLens/semanticTokens requests"
        );

        // A server advertising every provider receives all three requests.
        let (full_tx, mut full_rx) = mpsc::channel(64);
        let full_handle = LspClientHandle::from_sender_for_test(full_tx, 22);
        full_handle.set_capabilities_for_test(crate::lsp_client::LspServerCapabilities {
            rename_provider: true,
            prepare_rename_supported: false,
            semantic_tokens_provider: true,
            inlay_hint_provider: true,
            code_lens_provider: true,
        });
        app.lsp_clients.insert(key, full_handle);
        app.lsp_trace.clear();
        // Schedules due immediately (now minus the debounce), unlike the
        // plain refresh scheduler.
        assert_eq!(app.schedule_lsp_symbol_refreshes_for_open_buffers(), 1);
        assert!(app.flush_pending_lsp_symbol_refreshes() >= 1);

        let mut methods = std::collections::HashSet::new();
        while let Ok(command) = full_rx.try_recv() {
            match command {
                LspClientCommand::InlayHints {
                    path, version: v, ..
                } => {
                    assert_eq!((path, v), (source.clone(), version));
                    methods.insert("textDocument/inlayHint");
                }
                LspClientCommand::CodeLenses { .. } => {
                    methods.insert("textDocument/codeLens");
                }
                LspClientCommand::SemanticTokens { .. } => {
                    methods.insert("textDocument/semanticTokens/full");
                }
                _ => {}
            }
        }
        assert_eq!(
            methods,
            std::collections::HashSet::from([
                "textDocument/inlayHint",
                "textDocument/codeLens",
                "textDocument/semanticTokens/full",
            ]),
            "an advertising server receives every refresh request"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn did_close_is_filtered_to_clients_holding_the_document() {
        let root = temp_root("did-close-filtered");
        let source = root.join("src").join("main.rs");
        let settings = EditorSettings {
            lsp_servers: vec![
                LspServerConfig {
                    language: "rust".to_owned(),
                    command: "rust-analyzer".to_owned(),
                    args: Vec::new(),
                    extensions: Vec::new(),
                    root_markers: vec!["Cargo.toml".to_owned()],
                    enabled: true,
                },
                LspServerConfig {
                    language: "rust".to_owned(),
                    command: "rust-analyzer-obsidian".to_owned(),
                    args: vec!["--stdio".to_owned()],
                    extensions: Vec::new(),
                    root_markers: vec!["Cargo.toml".to_owned()],
                    enabled: true,
                },
            ],
            ..EditorSettings::default()
        };
        let mut app = app_for_test_with_settings(root.clone(), settings);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(source.clone()),
            "fn main() {}\n".to_owned(),
        ));

        let configs = app.settings.lsp_server_configs();
        let primary_key = lsp_client_key(
            configs
                .iter()
                .find(|config| config.command == "rust-analyzer")
                .expect("primary rust config"),
            &configs,
        );
        let secondary_key = lsp_client_key(
            configs
                .iter()
                .find(|config| config.command == "rust-analyzer-obsidian")
                .expect("secondary rust config"),
            &configs,
        );
        let (primary_tx, mut primary_rx) = mpsc::channel(64);
        let (secondary_tx, mut secondary_rx) = mpsc::channel(64);
        app.lsp_clients.insert(
            primary_key.clone(),
            LspClientHandle::from_sender_for_test(primary_tx, 30),
        );
        app.lsp_clients.insert(
            secondary_key.clone(),
            LspClientHandle::from_sender_for_test(secondary_tx, 31),
        );

        // Both clients learn about the document.
        app.notify_lsp_open(7);
        // The secondary then closes it on its own and opens a sibling file:
        // it no longer holds main.rs but DOES track documents, so it must
        // not receive the didClose for main.rs anymore.
        secondary_open_documents_note(&app, &secondary_key, &source, false);
        secondary_open_documents_note(&app, &secondary_key, &PathBuf::from("other.rs"), true);

        app.notify_lsp_close(7);

        let primary_closed = did_close_paths(&mut primary_rx);
        let secondary_closed = did_close_paths(&mut secondary_rx);
        assert_eq!(primary_closed, vec![source.clone()]);
        assert!(
            secondary_closed.is_empty(),
            "a client that does not hold the document must not receive didClose: {secondary_closed:?}"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn did_close_falls_back_to_language_wide_fan_out_without_tracking() {
        let root = temp_root("did-close-fallback");
        let source = root.join("src").join("main.rs");
        let mut app = app_for_test(root.clone());
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(source.clone()),
            "fn main() {}\n".to_owned(),
        ));
        let configs = app.settings.lsp_server_configs();
        let key = lsp_client_key(
            configs
                .iter()
                .find(|config| config.language == "rust")
                .expect("default rust config"),
            &configs,
        );
        let (tx, mut rx) = mpsc::channel(64);
        // A client with NO tracked documents (unknown didOpen state) keeps
        // the legacy behavior: the didClose still reaches it.
        app.lsp_clients
            .insert(key, LspClientHandle::from_sender_for_test(tx, 41));

        app.notify_lsp_close(7);

        assert_eq!(did_close_paths(&mut rx), vec![source]);

        let _ = fs::remove_dir_all(root);
    }

    fn secondary_open_documents_note(app: &KuroyaApp, client_key: &str, path: &Path, open: bool) {
        let client = app.lsp_clients.get(client_key).expect("client");
        if open {
            client.open_documents().note_open(path);
        } else {
            client.open_documents().note_closed(path);
        }
    }

    fn did_close_paths(rx: &mut mpsc::Receiver<LspClientCommand>) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        while let Ok(command) = rx.try_recv() {
            if let LspClientCommand::DidClose { path } = command {
                paths.push(path);
            }
        }
        paths
    }

    #[test]
    fn spawning_a_client_under_a_pending_restart_replays_did_open_for_every_buffer() {
        let root = temp_root("pending-restart-spawn-replays-did-open");
        let main_source = root.join("src").join("main.rs");
        let lib_source = root.join("src").join("lib.rs");
        // The stall server stays alive silently, so the spawned client task
        // parks in the initialize handshake and its command queue keeps
        // accepting the didOpen commands this test asserts on.
        let settings = EditorSettings {
            lsp_servers: vec![silent_stall_server_config()],
            ..EditorSettings::default()
        };
        let mut app = app_for_test_with_settings(root.clone(), settings);
        // The spawned server process runs with its working directory set to
        // the workspace root, so the directory must exist for the spawn to
        // succeed (otherwise the client task exits immediately and drops its
        // command queue while this test is still queueing didOpen).
        std::fs::create_dir_all(root.join("src")).expect("create test workspace");
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(main_source.clone()),
            "fn main() {}\n".to_owned(),
        ));
        app.buffers.push(TextBuffer::from_text(
            8,
            Some(lib_source.clone()),
            "fn helper() {}\n".to_owned(),
        ));
        // The rust server died while both buffers were open: its restart is
        // pending. A keystroke now spawns a fresh client before the restart
        // ladder flushes, and that spawn must replay the ladder's didOpen
        // pass for both buffers instead of leaving them feature-dead.
        app.pending_lsp_restarts
            .insert("rust".to_owned(), Instant::now() - Duration::from_millis(1));

        let handles = app.ensure_lsp_clients_for_buffer(7);

        assert_eq!(handles.len(), 1, "the settings attach one rust server");
        assert!(app.lsp_clients.contains_key("rust"));
        assert!(
            !app.pending_lsp_restarts.contains_key("rust"),
            "the spawned client supersedes the pending restart"
        );
        let mut did_open_targets = did_open_trace_targets(&app);
        did_open_targets.sort();
        assert_eq!(
            did_open_targets.len(),
            2,
            "didOpen must be queued once per open rust buffer: {did_open_targets:?}"
        );
        assert!(
            did_open_targets
                .iter()
                .any(|detail| detail.contains("main.rs")),
            "{did_open_targets:?}"
        );
        assert!(
            did_open_targets
                .iter()
                .any(|detail| detail.contains("lib.rs")),
            "didOpen must cover the sibling buffer too: {did_open_targets:?}"
        );

        let _ = fs::remove_dir_all(root);
    }

    /// A fake rust server that stays alive without ever speaking LSP, so a
    /// client spawned for it parks in the initialize handshake (the command
    /// queue keeps accepting commands) instead of failing startup.
    fn silent_stall_server_config() -> LspServerConfig {
        #[cfg(windows)]
        let (command, args) = (
            "cmd".to_owned(),
            vec!["/C".to_owned(), "ping -n 30 127.0.0.1 > NUL".to_owned()],
        );

        #[cfg(not(windows))]
        let (command, args) = (
            "sh".to_owned(),
            vec!["-c".to_owned(), "sleep 30".to_owned()],
        );

        LspServerConfig {
            language: "rust".to_owned(),
            command,
            args,
            extensions: Vec::new(),
            root_markers: vec!["Cargo.toml".to_owned()],
            enabled: true,
        }
    }

    #[test]
    fn spawning_a_client_without_a_pending_restart_does_not_replay_did_open() {
        let root = temp_root("plain-spawn-skips-did-open-replay");
        let main_source = root.join("src").join("main.rs");
        let mut app = app_for_test(root.clone());
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(main_source),
            "fn main() {}\n".to_owned(),
        ));

        let handles = app.ensure_lsp_clients_for_buffer(7);

        assert_eq!(handles.len(), 1);
        assert!(app.lsp_clients.contains_key("rust"));
        assert!(
            did_open_trace_targets(&app).is_empty(),
            "a plain first spawn leaves the didOpen flow to notify_lsp_open"
        );

        let _ = fs::remove_dir_all(root);
    }

    fn did_open_trace_targets(app: &KuroyaApp) -> Vec<String> {
        app.lsp_trace
            .iter()
            .filter(|entry| {
                entry.direction == crate::devtools_lsp_trace::LspTraceDirection::Client
                    && entry.method == "textDocument/didOpen"
            })
            .map(|entry| entry.detail.clone())
            .collect()
    }

    #[test]
    fn inlay_hint_range_end_uses_utf16_line_length() {
        let buffer = TextBuffer::from_text(1, None, "alpha\n\u{1f600}x".to_owned());

        assert_eq!(inlay_hint_range_end(&buffer), (1, 3));
    }

    #[test]
    fn lsp_document_trace_labels_are_display_safe_and_bounded() {
        let path = PathBuf::from("workspace").join(format!(
            "sync\n{}\u{202e}.rs",
            "path-".repeat(DISPLAY_PATH_LABEL_MAX_CHARS)
        ));

        let path_label = lsp_document_trace_path_label(&path);
        let version_label = lsp_document_version_trace_label(&path, 42);

        for label in [path_label.as_ref(), version_label.trim_end_matches(" v42")] {
            assert!(!label.contains('\n'));
            assert!(!label.contains('\u{202e}'));
            assert!(label.contains("..."));
            assert!(label.chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS);
        }
        assert!(version_label.ends_with(" v42"));
    }

    #[test]
    fn open_buffer_symbol_refreshes_are_scheduled_due_immediately() {
        let root = temp_root("open-buffer-symbol-refreshes");
        let source = root.join("src").join("main.rs");
        let mut app = app_for_test(root.clone());
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(source),
            "fn main() {}\n".to_owned(),
        ));
        app.buffers
            .push(TextBuffer::from_text(8, None, "fn helper() {}".to_owned()));

        assert_eq!(app.schedule_lsp_symbol_refreshes_for_open_buffers(), 1);
        assert_eq!(
            due_lsp_symbol_refresh_ids(
                &app.pending_lsp_symbol_refreshes,
                Instant::now(),
                LSP_SYMBOL_REFRESH_DEBOUNCE,
            ),
            vec![7]
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn open_buffer_symbol_refresh_scheduling_prunes_stale_pending_ids() {
        let root = temp_root("symbol-refresh-prune-stale");
        let source = root.join("src").join("main.rs");
        let mut app = app_for_test(root.clone());
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(source),
            "fn main() {}\n".to_owned(),
        ));
        app.buffers
            .push(TextBuffer::from_text(8, None, "fn helper() {}".to_owned()));
        app.pending_lsp_symbol_refreshes.insert(99, Instant::now());

        assert_eq!(app.schedule_lsp_symbol_refreshes_for_open_buffers(), 1);

        assert!(app.pending_lsp_symbol_refreshes.contains_key(&7));
        assert!(!app.pending_lsp_symbol_refreshes.contains_key(&8));
        assert!(!app.pending_lsp_symbol_refreshes.contains_key(&99));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn lsp_symbol_refresh_target_keeps_raw_path_while_filtering_blocked_buffers() {
        let raw_path = PathBuf::from("workspace/src/raw\n\u{202e}.rs");
        let buffer = TextBuffer::from_text(7, Some(raw_path.clone()), "fn main() {}".to_owned());
        let lossy = std::collections::HashSet::new();
        let binary = std::collections::HashSet::new();

        assert!(lsp_symbol_refresh_buffer_is_eligible(
            &buffer, &lossy, &binary
        ));

        let (path, version) = lsp_symbol_refresh_target_for_buffer(&buffer, &lossy, &binary)
            .expect("path-backed text buffer is eligible");

        assert_eq!(path, raw_path);
        assert_eq!(version, buffer.version());

        let lossy = std::collections::HashSet::from([7]);
        assert!(!lsp_symbol_refresh_buffer_is_eligible(
            &buffer, &lossy, &binary
        ));
        assert!(lsp_symbol_refresh_target_for_buffer(&buffer, &lossy, &binary).is_none());

        let unbacked_buffer = TextBuffer::from_text(8, None, "fn helper() {}".to_owned());
        let lossy = std::collections::HashSet::new();
        assert!(!lsp_symbol_refresh_buffer_is_eligible(
            &unbacked_buffer,
            &lossy,
            &binary
        ));
        assert!(lsp_symbol_refresh_target_for_buffer(&unbacked_buffer, &lossy, &binary).is_none());
    }

    #[test]
    fn text_snapshot_for_version_rejects_stale_buffers() {
        let root = temp_root("stale-text-snapshot");
        let source = root.join("src").join("main.rs");
        let mut app = app_for_test(root.clone());
        app.buffers
            .push(TextBuffer::from_text(7, Some(source), "alpha".to_owned()));
        let version = app.buffer(7).expect("buffer").version();

        let snapshot = app
            .lsp_text_snapshot_for_version(7, version)
            .expect("current version snapshot");
        assert_eq!(snapshot.text(), "alpha");

        app.buffer_mut(7).expect("buffer").insert_at_cursor(" beta");

        assert!(app.lsp_text_snapshot_for_version(7, version).is_none());

        let _ = fs::remove_dir_all(root);
    }

    fn app_for_test(root: PathBuf) -> KuroyaApp {
        let settings = EditorSettings::default();
        app_for_test_with_settings(root, settings)
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

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!(
            "kuroya-document-sync-{name}-{}-{nanos}",
            std::process::id()
        ))
    }
}
