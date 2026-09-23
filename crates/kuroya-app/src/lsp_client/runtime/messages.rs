mod diagnostics;
mod progress;
mod requests;
mod responses;

use super::super::pending::PendingLspRequests;
use crate::lsp_client::stderr_log::LspStderrLog;
use crate::lsp_client::watched_files::LspWatchedFilesState;
use crate::ui_event_channel::Sender;
use crate::ui_events::UiEvent;
use diagnostics::send_publish_diagnostics;
pub(super) use progress::forget_created_work_done_progress_tokens_for_server;
use progress::{acknowledge_work_done_progress_create, send_work_done_progress};
use requests::handle_server_request;
use responses::handle_response_message;
use serde_json::Value;
use std::path::Path;
use tokio::process::ChildStdin;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LspServerMessageOutcome {
    Continue,
    FatalWriteFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LspServerMessageHandlerOutcome {
    Unhandled,
    Handled,
    FatalWriteFailure,
}

pub(super) async fn handle_lsp_server_message(
    value: Value,
    language: &str,
    root: &Path,
    generation: u64,
    pending_requests: &mut PendingLspRequests,
    ui_tx: &Sender<UiEvent>,
    writer: &mut ChildStdin,
    stderr_log: &LspStderrLog,
    watched_files: &LspWatchedFilesState,
) -> LspServerMessageOutcome {
    if value.get("method").is_none() {
        handle_response_message(value, language, root, generation, pending_requests, ui_tx);
        return LspServerMessageOutcome::Continue;
    }

    if send_publish_diagnostics(&value, language, root, generation, ui_tx) {
        return LspServerMessageOutcome::Continue;
    }
    if send_work_done_progress(&value, language, root, generation, ui_tx) {
        return LspServerMessageOutcome::Continue;
    }
    if let Some(text) = lsp_window_log_message_text(&value) {
        stderr_log.push_line(text);
        return LspServerMessageOutcome::Continue;
    }
    match acknowledge_work_done_progress_create(&value, language, root, generation, ui_tx, writer)
        .await
    {
        LspServerMessageHandlerOutcome::Handled => return LspServerMessageOutcome::Continue,
        LspServerMessageHandlerOutcome::FatalWriteFailure => {
            return LspServerMessageOutcome::FatalWriteFailure;
        }
        LspServerMessageHandlerOutcome::Unhandled => {}
    }
    match handle_server_request(
        &value,
        language,
        root,
        generation,
        ui_tx,
        writer,
        watched_files,
    )
    .await
    {
        LspServerMessageHandlerOutcome::Handled => return LspServerMessageOutcome::Continue,
        LspServerMessageHandlerOutcome::FatalWriteFailure => {
            return LspServerMessageOutcome::FatalWriteFailure;
        }
        LspServerMessageHandlerOutcome::Unhandled => {}
    }

    handle_response_message(value, language, root, generation, pending_requests, ui_tx);
    LspServerMessageOutcome::Continue
}

/// Extracts the human-readable text of a `window/logMessage` notification so
/// it can be retained in the shared stderr ring instead of being dropped.
fn lsp_window_log_message_text(value: &Value) -> Option<&str> {
    if value.get("method").and_then(Value::as_str) != Some("window/logMessage") {
        return None;
    }
    value
        .get("params")
        .and_then(|params| params.get("message"))
        .and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::lsp_window_log_message_text;
    use serde_json::json;

    #[test]
    fn window_log_message_text_extracts_params_message() {
        assert_eq!(
            lsp_window_log_message_text(&json!({
                "jsonrpc": "2.0",
                "method": "window/logMessage",
                "params": {"type": 2, "message": "indexing stalled"}
            })),
            Some("indexing stalled")
        );
    }

    #[test]
    fn window_log_message_text_ignores_other_methods_and_shapes() {
        assert_eq!(
            lsp_window_log_message_text(&json!({
                "jsonrpc": "2.0",
                "method": "window/showMessage",
                "params": {"message": "hello"}
            })),
            None
        );
        assert_eq!(
            lsp_window_log_message_text(&json!({
                "jsonrpc": "2.0",
                "method": "window/logMessage",
                "params": {"type": 1}
            })),
            None
        );
        assert_eq!(
            lsp_window_log_message_text(&json!({
                "jsonrpc": "2.0",
                "id": 4,
                "result": {}
            })),
            None
        );
    }
}
