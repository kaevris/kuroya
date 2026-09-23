mod handshake;
mod process;

use crate::lsp_client::handle::LspServerCapabilitiesState;
use crate::lsp_client::stderr_log::LspStderrLog;
use crate::lsp_client::watched_files::LspWatchedFilesState;
use crate::ui_event_channel::Sender;
use crate::{lsp_ui_events::LspUiEvent, ui_events::UiEvent};
use kuroya_core::{LspServerConfig, TextDocumentSyncKindSetting};
use std::path::Path;
use tokio::{
    io::BufReader,
    process::{Child, ChildStdin, ChildStdout},
    sync::watch,
};

use super::{shutdown_signal_requested, status::send_lsp_stopped_status};
use handshake::{LspStartupHandshakeResult, complete_lsp_startup_handshake};
use process::prepare_lsp_process_io;

pub(super) struct StartedLspClient {
    pub(super) child: Child,
    pub(super) writer: ChildStdin,
    pub(super) reader: BufReader<ChildStdout>,
    /// Negotiated `textDocumentSync` change kind from the initialize result.
    pub(super) sync_kind: TextDocumentSyncKindSetting,
}

pub(super) async fn start_lsp_process(
    config: &LspServerConfig,
    root: &Path,
    generation: u64,
    shutdown_rx: &mut watch::Receiver<bool>,
    ui_tx: &Sender<UiEvent>,
    stderr_log: &LspStderrLog,
    watched_files: &LspWatchedFilesState,
    capabilities: &LspServerCapabilitiesState,
) -> Option<StartedLspClient> {
    if shutdown_signal_requested(shutdown_rx) {
        return None;
    }

    let _ = crate::ui_event_channel::send_ui_event(
        ui_tx,
        UiEvent::Lsp(LspUiEvent::Status {
            language: config.language.clone(),
            root: root.to_path_buf(),
            generation,
            message: format!("Starting {} LSP", config.language),
        }),
    );

    let mut started = prepare_lsp_process_io(config, root, generation, ui_tx, stderr_log)?;

    match complete_lsp_startup_handshake(
        &mut started.writer,
        &mut started.reader,
        root,
        config,
        generation,
        shutdown_rx,
        ui_tx,
        stderr_log,
        watched_files,
        capabilities,
    )
    .await
    {
        LspStartupHandshakeResult::Ready(sync_kind) => {
            started.sync_kind = sync_kind;
            Some(started)
        }
        LspStartupHandshakeResult::Failed => {
            let _ = started.child.kill().await;
            send_lsp_stopped_status(&config.language, root, generation, None, ui_tx);
            None
        }
        LspStartupHandshakeResult::ShutdownRequested => {
            let _ = started.child.kill().await;
            None
        }
    }
}
