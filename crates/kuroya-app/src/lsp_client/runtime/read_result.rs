use super::super::pending::PendingLspRequests;
use super::messages::{LspServerMessageOutcome, handle_lsp_server_message};
use super::status::send_lsp_read_error_status;
use crate::lsp_client::stderr_log::LspStderrLog;
use crate::lsp_client::watched_files::LspWatchedFilesState;
use crate::ui_event_channel::Sender;
use crate::ui_events::UiEvent;
use serde_json::Value;
use std::path::Path;
use tokio::process::ChildStdin;

pub(super) async fn handle_lsp_read_result(
    message: anyhow::Result<Option<Value>>,
    language: &str,
    root: &Path,
    generation: u64,
    pending_requests: &mut PendingLspRequests,
    ui_tx: &Sender<UiEvent>,
    writer: &mut ChildStdin,
    stderr_log: &LspStderrLog,
    watched_files: &LspWatchedFilesState,
) -> bool {
    match message {
        Ok(Some(value)) => {
            matches!(
                handle_lsp_server_message(
                    value,
                    language,
                    root,
                    generation,
                    pending_requests,
                    ui_tx,
                    writer,
                    stderr_log,
                    watched_files,
                )
                .await,
                LspServerMessageOutcome::Continue
            )
        }
        Ok(None) => false,
        Err(error) => {
            send_lsp_read_error_status(language, root, generation, &error, ui_tx);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::handle_lsp_read_result;
    use crate::{
        lsp_client::pending::PendingLspRequests, lsp_client::stderr_log::LspStderrLog,
        ui_event_channel::ui_event_channel,
    };
    use serde_json::json;
    use std::{path::PathBuf, process::Stdio};
    use tokio::process::{ChildStdin, Command};

    #[tokio::test]
    async fn work_done_progress_create_write_failure_stops_runtime_path() {
        let root = PathBuf::from("workspace-progress-write-failure");
        let message = json!({
            "jsonrpc": "2.0",
            "id": 17,
            "method": "window/workDoneProgress/create",
            "params": {
                "token": "cargo-check"
            }
        });
        let mut pending_requests = PendingLspRequests::default();
        let (ui_tx, ui_rx) = ui_event_channel();
        let mut writer = exited_child_stdin().await;

        let keep_running = handle_lsp_read_result(
            Ok(Some(message)),
            "rust",
            &root,
            7,
            &mut pending_requests,
            &ui_tx,
            &mut writer,
            &LspStderrLog::default(),
            &crate::lsp_client::watched_files::LspWatchedFilesState::default(),
        )
        .await;

        assert!(!keep_running);
        assert!(ui_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn unsupported_server_request_write_failure_stops_runtime_path() {
        let root = PathBuf::from("workspace-request-write-failure");
        let message = json!({
            "jsonrpc": "2.0",
            "id": 19,
            "method": "workspace/configuration",
            "params": []
        });
        let mut pending_requests = PendingLspRequests::default();
        let (ui_tx, ui_rx) = ui_event_channel();
        let mut writer = exited_child_stdin().await;

        let keep_running = handle_lsp_read_result(
            Ok(Some(message)),
            "rust",
            &root,
            7,
            &mut pending_requests,
            &ui_tx,
            &mut writer,
            &LspStderrLog::default(),
            &crate::lsp_client::watched_files::LspWatchedFilesState::default(),
        )
        .await;

        assert!(!keep_running);
        assert!(ui_rx.try_recv().is_err());
    }

    async fn exited_child_stdin() -> ChildStdin {
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("cmd");
            command.args(["/C", "exit 0"]);
            command
        };

        #[cfg(not(windows))]
        let mut command = {
            let mut command = Command::new("sh");
            command.args(["-c", "true"]);
            command
        };

        let mut child = command
            .stdin(Stdio::piped())
            .spawn()
            .expect("spawn child process with stdin");
        let stdin = child.stdin.take().expect("child stdin is piped");
        child.wait().await.expect("child exits cleanly");
        stdin
    }
}
