use crate::lsp_client::handle::{LspServerCapabilities, LspServerCapabilitiesState};
use crate::lsp_client::stderr_log::LspStderrLog;
use crate::lsp_client::watched_files::LspWatchedFilesState;
use crate::ui_event_channel::Sender;
use crate::{
    lsp_client::{
        pending::PendingLspRequests,
        runtime::messages::handle_lsp_server_message,
        wire::{LspMessageReadBuffer, read_message, write_message},
    },
    lsp_runtime::{
        lsp_language_display_label, lsp_server_ready_status, lsp_status_display_message,
    },
    lsp_ui_events::LspUiEvent,
    path_display::display_error_label_cow,
    ui_events::UiEvent,
};
use kuroya_core::{
    LspServerConfig, LspWireMessage, TextDocumentSyncKindSetting, parse_text_document_sync_kind,
};
use serde_json::Value;
use std::{path::Path, time::Duration};
use tokio::{
    io::BufReader,
    process::{ChildStdin, ChildStdout},
    sync::watch,
    time::{Instant, timeout_at},
};

use super::super::shutdown_signal_requested;

const LSP_INITIALIZE_REQUEST_ID: u64 = 1;
pub(super) const LSP_INITIALIZE_TIMEOUT: Duration = Duration::from_secs(10);

/// Everything the initialize response teaches this client about the server:
/// the negotiated sync kind plus the advertised provider capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LspServerHandshake {
    sync_kind: TextDocumentSyncKindSetting,
    capabilities: LspServerCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InitializeResponseState {
    Waiting,
    Ready(LspServerHandshake),
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LspStartupHandshakeResult {
    Ready(TextDocumentSyncKindSetting),
    Failed,
    ShutdownRequested,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StartupHandshakeError {
    Failed(String),
    ShutdownRequested,
}

pub(super) async fn complete_lsp_startup_handshake(
    writer: &mut ChildStdin,
    reader: &mut BufReader<ChildStdout>,
    root: &Path,
    config: &LspServerConfig,
    generation: u64,
    shutdown_rx: &mut watch::Receiver<bool>,
    ui_tx: &Sender<UiEvent>,
    stderr_log: &LspStderrLog,
    watched_files: &LspWatchedFilesState,
    capabilities: &LspServerCapabilitiesState,
) -> LspStartupHandshakeResult {
    if shutdown_signal_requested(shutdown_rx) {
        return LspStartupHandshakeResult::ShutdownRequested;
    }

    if let Err(error) = write_message(
        writer,
        &LspWireMessage::initialize(LSP_INITIALIZE_REQUEST_ID, root).to_json(),
    )
    .await
    {
        send_lsp_startup_status(
            &config.language,
            root,
            generation,
            lsp_initialize_failed_status_message(&config.language, &error.to_string()),
            ui_tx,
        );
        return LspStartupHandshakeResult::Failed;
    }

    let handshake = match wait_for_initialize_response(
        writer,
        reader,
        &config.language,
        root,
        generation,
        shutdown_rx,
        ui_tx,
        stderr_log,
        watched_files,
    )
    .await
    {
        Ok(handshake) => handshake,
        Err(StartupHandshakeError::ShutdownRequested) => {
            return LspStartupHandshakeResult::ShutdownRequested;
        }
        Err(StartupHandshakeError::Failed(error)) => {
            send_lsp_startup_status(
                &config.language,
                root,
                generation,
                lsp_initialize_failed_status_message(&config.language, &error),
                ui_tx,
            );
            return LspStartupHandshakeResult::Failed;
        }
    };
    // Retain the advertised providers on the shared state so the app frame
    // loop can gate requests the server never advertised (a server without
    // inlayHint/codeLens/semanticTokens providers must not be asked for them).
    capabilities.set(handshake.capabilities);

    if shutdown_signal_requested(shutdown_rx) {
        return LspStartupHandshakeResult::ShutdownRequested;
    }

    if let Err(error) = write_message(writer, &LspWireMessage::initialized().to_json()).await {
        send_lsp_startup_status(
            &config.language,
            root,
            generation,
            lsp_initialized_notification_failed_status_message(
                &config.language,
                &error.to_string(),
            ),
            ui_tx,
        );
        return LspStartupHandshakeResult::Failed;
    }

    if shutdown_signal_requested(shutdown_rx) {
        return LspStartupHandshakeResult::ShutdownRequested;
    }

    send_lsp_startup_status(
        &config.language,
        root,
        generation,
        lsp_startup_ready_status_message(&config.language),
        ui_tx,
    );
    send_lsp_server_ready(&config.language, root, generation, ui_tx);
    LspStartupHandshakeResult::Ready(handshake.sync_kind)
}

async fn wait_for_initialize_response(
    writer: &mut ChildStdin,
    reader: &mut BufReader<ChildStdout>,
    language: &str,
    root: &Path,
    generation: u64,
    shutdown_rx: &mut watch::Receiver<bool>,
    ui_tx: &Sender<UiEvent>,
    stderr_log: &LspStderrLog,
    watched_files: &LspWatchedFilesState,
) -> Result<LspServerHandshake, StartupHandshakeError> {
    if shutdown_signal_requested(shutdown_rx) {
        return Err(StartupHandshakeError::ShutdownRequested);
    }

    let mut startup_pending_requests = PendingLspRequests::default();
    let mut read_buffer = LspMessageReadBuffer::default();
    let deadline = Instant::now() + LSP_INITIALIZE_TIMEOUT;
    loop {
        let message = tokio::select! {
            biased;
            changed = shutdown_rx.changed() => {
                match changed {
                    Ok(()) if shutdown_signal_requested(shutdown_rx) => {
                        return Err(StartupHandshakeError::ShutdownRequested);
                    }
                    Ok(()) => {
                        continue;
                    }
                    Err(_) => {
                        return Err(StartupHandshakeError::ShutdownRequested);
                    }
                }
            }
            message = timeout_at(deadline, read_message(reader, &mut read_buffer)) => {
                message
                    .map_err(|_| {
                        StartupHandshakeError::Failed(
                            "timed out waiting for initialize response".to_owned(),
                        )
                    })?
                    .map_err(|error| StartupHandshakeError::Failed(error.to_string()))?
            }
        };

        let Some(value) = message else {
            return Err(StartupHandshakeError::Failed(
                "server closed stdout before initialize response".to_owned(),
            ));
        };

        match initialize_response_state(&value, LSP_INITIALIZE_REQUEST_ID) {
            InitializeResponseState::Ready(handshake) => return Ok(handshake),
            InitializeResponseState::Failed(error) => {
                return Err(StartupHandshakeError::Failed(error));
            }
            InitializeResponseState::Waiting => {
                handle_lsp_server_message(
                    value,
                    language,
                    root,
                    generation,
                    &mut startup_pending_requests,
                    ui_tx,
                    writer,
                    stderr_log,
                    watched_files,
                )
                .await;
            }
        }
    }
}

fn initialize_response_state(value: &Value, request_id: u64) -> InitializeResponseState {
    if value.get("id").and_then(Value::as_u64) != Some(request_id) {
        return InitializeResponseState::Waiting;
    }

    if let Some(error) = value.get("error") {
        return InitializeResponseState::Failed(initialize_error_summary(error));
    }

    if let Some(result) = value.get("result") {
        InitializeResponseState::Ready(LspServerHandshake {
            sync_kind: initialize_result_sync_kind(result),
            capabilities: initialize_result_capabilities(result),
        })
    } else {
        InitializeResponseState::Failed("initialize response missing result".to_owned())
    }
}

/// Extracts the negotiated `textDocumentSync` change kind from the initialize
/// result. Servers that omit or malformedly declare the capability keep the
/// full-document sync default.
fn initialize_result_sync_kind(result: &Value) -> TextDocumentSyncKindSetting {
    result
        .get("capabilities")
        .and_then(|capabilities| capabilities.get("textDocumentSync"))
        .map(parse_text_document_sync_kind)
        .unwrap_or_default()
}

/// Extracts the advertised provider capabilities from the initialize result.
/// Providers arrive in several shapes — `true`, an options object such as
/// `{ workDoneProgress: true }`, or absent — so only `true` and object shapes
/// count as advertised; `false`, `null`, and missing keys do not.
fn initialize_result_capabilities(result: &Value) -> LspServerCapabilities {
    let capabilities = result.get("capabilities");
    let (rename_provider, prepare_rename_supported) =
        match capabilities.and_then(|capabilities| capabilities.get("renameProvider")) {
            Some(Value::Bool(true)) => (true, false),
            Some(rename_provider @ Value::Object(_)) => (
                true,
                rename_provider
                    .get("prepareProvider")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            ),
            _ => (false, false),
        };
    LspServerCapabilities {
        rename_provider,
        prepare_rename_supported,
        semantic_tokens_provider: provider_advertised(capabilities, "semanticTokensProvider"),
        inlay_hint_provider: provider_advertised(capabilities, "inlayHintProvider"),
        code_lens_provider: provider_advertised(capabilities, "codeLensProvider"),
    }
}

fn provider_advertised(capabilities: Option<&Value>, key: &str) -> bool {
    matches!(
        capabilities.and_then(|capabilities| capabilities.get(key)),
        Some(Value::Bool(true)) | Some(Value::Object(_))
    )
}

fn initialize_error_summary(error: &Value) -> String {
    error
        .get("message")
        .and_then(Value::as_str)
        .filter(|message| !message.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| error.to_string())
}

fn lsp_initialize_failed_status_message(language: &str, error: &str) -> String {
    lsp_status_display_message(&format!(
        "{} LSP initialize failed: {}",
        lsp_language_display_label(language),
        display_error_label_cow(error)
    ))
}

fn lsp_initialized_notification_failed_status_message(language: &str, error: &str) -> String {
    lsp_status_display_message(&format!(
        "{} LSP initialized notification failed: {}",
        lsp_language_display_label(language),
        display_error_label_cow(error)
    ))
}

fn lsp_startup_ready_status_message(language: &str) -> String {
    lsp_status_display_message(&lsp_server_ready_status(language))
}

fn send_lsp_startup_status(
    language: &str,
    root: &Path,
    generation: u64,
    message: String,
    ui_tx: &Sender<UiEvent>,
) {
    let _ = crate::ui_event_channel::send_ui_event(
        ui_tx,
        UiEvent::Lsp(LspUiEvent::Status {
            language: language.to_owned(),
            root: root.to_path_buf(),
            generation,
            message: lsp_status_display_message(&message),
        }),
    );
}

fn send_lsp_server_ready(language: &str, root: &Path, generation: u64, ui_tx: &Sender<UiEvent>) {
    let _ = crate::ui_event_channel::send_critical_ui_event(
        ui_tx,
        UiEvent::Lsp(LspUiEvent::ServerReady {
            language: language.to_owned(),
            root: root.to_path_buf(),
            generation,
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::{
        InitializeResponseState, StartupHandshakeError, initialize_response_state,
        initialize_result_capabilities, initialize_result_sync_kind,
        lsp_initialize_failed_status_message, lsp_initialized_notification_failed_status_message,
        lsp_startup_ready_status_message, send_lsp_server_ready, send_lsp_startup_status,
        wait_for_initialize_response,
    };
    use crate::{
        lsp_client::handle::LspServerCapabilities, lsp_client::stderr_log::LspStderrLog,
        lsp_runtime::LSP_STATUS_MESSAGE_MAX_CHARS, lsp_ui_events::LspUiEvent,
        ui_event_channel::ui_event_channel, ui_events::UiEvent,
    };
    use kuroya_core::TextDocumentSyncKindSetting;
    use serde_json::{Value, json};
    use std::{path::PathBuf, process::Stdio};
    use tokio::{
        io::BufReader,
        process::{Child, ChildStdin, ChildStdout, Command},
        sync::watch,
    };

    fn ready_handshake(sync_kind: TextDocumentSyncKindSetting) -> InitializeResponseState {
        InitializeResponseState::Ready(super::LspServerHandshake {
            sync_kind,
            capabilities: LspServerCapabilities::default(),
        })
    }

    #[test]
    fn initialize_response_state_waits_for_matching_id() {
        assert_eq!(
            initialize_response_state(&json!({"jsonrpc": "2.0", "method": "window/logMessage"}), 1),
            InitializeResponseState::Waiting
        );
        assert_eq!(
            initialize_response_state(&json!({"jsonrpc": "2.0", "id": 2, "result": {}}), 1),
            InitializeResponseState::Waiting
        );
    }

    #[test]
    fn initialize_response_state_accepts_result_for_matching_id() {
        assert_eq!(
            initialize_response_state(&json!({"jsonrpc": "2.0", "id": 1, "result": {}}), 1),
            ready_handshake(TextDocumentSyncKindSetting::Full)
        );
    }

    #[test]
    fn initialize_result_sync_kind_reads_numbers_and_options_objects() {
        let sync = |text_document_sync: Value| {
            initialize_result_sync_kind(&json!({
                "capabilities": { "textDocumentSync": text_document_sync }
            }))
        };

        assert_eq!(sync(json!(2)), TextDocumentSyncKindSetting::Incremental);
        assert_eq!(sync(json!(0)), TextDocumentSyncKindSetting::None);
        assert_eq!(
            sync(json!({"openClose": true, "change": 2})),
            TextDocumentSyncKindSetting::Incremental
        );
        // Anything underdetermined falls back to full-document sync.
        assert_eq!(sync(json!(1)), TextDocumentSyncKindSetting::Full);
        assert_eq!(sync(json!(9)), TextDocumentSyncKindSetting::Full);
        assert_eq!(
            sync(json!({"openClose": true})),
            TextDocumentSyncKindSetting::Full
        );
        assert_eq!(
            initialize_result_sync_kind(&json!({"capabilities": {}})),
            TextDocumentSyncKindSetting::Full
        );
        assert_eq!(
            initialize_result_sync_kind(&json!({})),
            TextDocumentSyncKindSetting::Full
        );
    }

    #[test]
    fn initialize_response_state_reports_error_for_matching_id() {
        assert_eq!(
            initialize_response_state(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "error": {"code": -32603, "message": "workspace rejected"}
                }),
                1
            ),
            InitializeResponseState::Failed("workspace rejected".to_owned())
        );
    }

    #[test]
    fn initialize_response_error_status_sanitizes_newline_bidi_and_long_details() {
        let language = format!("rust\n{}\u{202e}", "language-fragment-".repeat(16));
        let error = format!(
            "first line\nsecond line \u{2066}{}",
            "error-fragment-".repeat(24)
        );

        let initialize_failed = lsp_initialize_failed_status_message(&language, &error);
        let initialized_failed =
            lsp_initialized_notification_failed_status_message(&language, &error);
        let ready = lsp_startup_ready_status_message(&language);

        for message in [initialize_failed, initialized_failed, ready] {
            assert_display_safe(&message);
            assert!(message.contains("..."));
            assert!(message.chars().count() <= LSP_STATUS_MESSAGE_MAX_CHARS);
        }
    }

    #[test]
    fn initialize_response_state_rejects_malformed_matching_response() {
        assert_eq!(
            initialize_response_state(&json!({"jsonrpc": "2.0", "id": 1}), 1),
            InitializeResponseState::Failed("initialize response missing result".to_owned())
        );
    }

    #[test]
    fn initialize_result_capabilities_reads_every_provider_shape() {
        let capabilities = |capabilities_json: Value| {
            initialize_result_capabilities(&json!({ "capabilities": capabilities_json }))
        };

        // Bare boolean providers.
        assert_eq!(
            capabilities(json!({
                "semanticTokensProvider": true,
                "inlayHintProvider": true,
                "codeLensProvider": true,
                "renameProvider": true
            })),
            LspServerCapabilities {
                rename_provider: true,
                prepare_rename_supported: false,
                semantic_tokens_provider: true,
                inlay_hint_provider: true,
                code_lens_provider: true,
            }
        );

        // Object shapes ({workDoneProgress} and prepareProvider) count.
        assert_eq!(
            capabilities(json!({
                "semanticTokensProvider": { "workDoneProgress": true },
                "inlayHintProvider": {},
                "codeLensProvider": { "resolveProvider": true },
                "renameProvider": { "prepareProvider": true }
            })),
            LspServerCapabilities {
                rename_provider: true,
                prepare_rename_supported: true,
                semantic_tokens_provider: true,
                inlay_hint_provider: true,
                code_lens_provider: true,
            }
        );

        // Explicit false, null, and absent keys are all "not advertised".
        assert_eq!(
            capabilities(json!({
                "semanticTokensProvider": false,
                "inlayHintProvider": null,
                "renameProvider": false
            })),
            LspServerCapabilities::default()
        );
        assert_eq!(
            capabilities(json!({})),
            LspServerCapabilities::default(),
            "a server advertising nothing gets no provider capabilities"
        );

        // renameProvider object without prepareProvider keeps rename only.
        assert_eq!(
            capabilities(json!({ "renameProvider": { "prepareProvider": false } })),
            LspServerCapabilities {
                rename_provider: true,
                prepare_rename_supported: false,
                semantic_tokens_provider: false,
                inlay_hint_provider: false,
                code_lens_provider: false,
            }
        );
    }

    #[test]
    fn lsp_server_ready_event_names_language_and_root() {
        let (tx, rx) = ui_event_channel();
        let root = PathBuf::from("workspace");

        send_lsp_server_ready("rust", &root, 11, &tx);

        let event = rx.try_recv().expect("ready event should be queued");
        assert!(matches!(
            event,
            UiEvent::Lsp(LspUiEvent::ServerReady { language, root: event_root, generation })
                if language == "rust" && event_root == root && generation == 11
        ));
    }

    #[test]
    fn lsp_startup_status_event_sanitizes_message_but_preserves_raw_language() {
        let (tx, rx) = ui_event_channel();
        let root = PathBuf::from("workspace");
        let language = format!("rust\n{}\u{202e}", "language-fragment-".repeat(16));
        let error = format!(
            "first line\nsecond line \u{2066}{}",
            "error-fragment-".repeat(24)
        );

        send_lsp_startup_status(
            &language,
            &root,
            12,
            lsp_initialize_failed_status_message(&language, &error),
            &tx,
        );

        let event = rx.try_recv().expect("startup status should be queued");
        let UiEvent::Lsp(LspUiEvent::Status {
            language: event_language,
            root: event_root,
            generation,
            message,
        }) = event
        else {
            panic!("expected LSP status event");
        };
        assert_eq!(event_language, language);
        assert_eq!(event_root, root);
        assert_eq!(generation, 12);
        assert_display_safe(&message);
        assert!(message.contains("..."));
        assert!(message.chars().count() <= LSP_STATUS_MESSAGE_MAX_CHARS);
    }

    #[test]
    fn lsp_startup_ready_status_sanitizes_message_but_ready_event_preserves_raw_language() {
        let (tx, rx) = ui_event_channel();
        let root = PathBuf::from("workspace");
        let language = format!("rust\n{}\u{202e}", "language-fragment-".repeat(16));

        send_lsp_startup_status(
            &language,
            &root,
            13,
            lsp_startup_ready_status_message(&language),
            &tx,
        );
        send_lsp_server_ready(&language, &root, 13, &tx);

        let status_event = rx.try_recv().expect("ready status should be queued");
        let UiEvent::Lsp(LspUiEvent::Status {
            language: status_language,
            message,
            ..
        }) = status_event
        else {
            panic!("expected LSP status event");
        };
        assert_eq!(status_language, language);
        assert_display_safe(&message);
        assert!(message.contains("..."));
        assert!(message.chars().count() <= LSP_STATUS_MESSAGE_MAX_CHARS);

        let ready_event = rx.try_recv().expect("server ready should be queued");
        assert!(matches!(
            ready_event,
            UiEvent::Lsp(LspUiEvent::ServerReady { language: event_language, root: event_root, generation })
                if event_language == language && event_root == root && generation == 13
        ));
    }

    fn assert_display_safe(value: &str) {
        assert!(!value.chars().any(char::is_control), "{value:?}");
        assert!(!value.chars().any(is_bidi_format_control), "{value:?}");
    }

    fn is_bidi_format_control(ch: char) -> bool {
        matches!(
            ch,
            '\u{061c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
        )
    }

    #[tokio::test]
    async fn wait_for_initialize_response_aborts_when_shutdown_is_requested() {
        let (mut child, mut writer, mut reader) = silent_child_with_stdio().await;
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        shutdown_tx
            .send(true)
            .expect("shutdown receiver should be live");
        let (ui_tx, _ui_rx) = ui_event_channel();
        let root = PathBuf::from("workspace");

        let result = wait_for_initialize_response(
            &mut writer,
            &mut reader,
            "rust",
            &root,
            7,
            &mut shutdown_rx,
            &ui_tx,
            &LspStderrLog::default(),
            &crate::lsp_client::watched_files::LspWatchedFilesState::default(),
        )
        .await;

        assert_eq!(result, Err(StartupHandshakeError::ShutdownRequested));
        let _ = child.kill().await;
    }

    #[tokio::test]
    async fn wait_for_initialize_response_aborts_when_shutdown_sender_is_dropped() {
        let (mut child, mut writer, mut reader) = silent_child_with_stdio().await;
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        drop(shutdown_tx);
        let (ui_tx, _ui_rx) = ui_event_channel();
        let root = PathBuf::from("workspace");

        let result = wait_for_initialize_response(
            &mut writer,
            &mut reader,
            "rust",
            &root,
            8,
            &mut shutdown_rx,
            &ui_tx,
            &LspStderrLog::default(),
            &crate::lsp_client::watched_files::LspWatchedFilesState::default(),
        )
        .await;

        assert_eq!(result, Err(StartupHandshakeError::ShutdownRequested));
        let _ = child.kill().await;
    }

    async fn silent_child_with_stdio() -> (Child, ChildStdin, BufReader<ChildStdout>) {
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("cmd");
            command.args(["/C", "more > NUL"]);
            command
        };

        #[cfg(not(windows))]
        let mut command = {
            let mut command = Command::new("sh");
            command.args(["-c", "cat >/dev/null"]);
            command
        };

        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn silent child");
        let writer = child.stdin.take().expect("child stdin should be piped");
        let stdout = child.stdout.take().expect("child stdout should be piped");
        (child, writer, BufReader::new(stdout))
    }
}
