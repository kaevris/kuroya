mod child_exit;
mod command_queue;
mod messages;
mod read_result;
mod startup;
mod status;

use super::{
    command_dispatch::document_sync::DocumentSyncState,
    command_dispatch::{LspClientCommandOutcome, LspClientStopReason, handle_lsp_client_command},
    commands::LspClientCommand,
    handle::LspServerCapabilitiesState,
    pending::PendingLspRequests,
    response::{
        emit_expired_lsp_request_timeouts, emit_pending_lsp_request_cancellations_for_server,
        emit_pending_lsp_request_failures_for_server,
    },
    stderr_log::{LSP_STDERR_STATUS_TAIL_CHARS, LspStderrLog},
    watched_files::LspWatchedFilesState,
    wire::{LspMessageReadBuffer, read_message, write_message},
};
use crate::lsp_ui_events::LspServerResultTarget;
use crate::ui_event_channel::Sender;
use crate::ui_events::UiEvent;
use kuroya_core::{LspServerConfig, LspWireMessage};
use std::{
    io,
    path::{Path, PathBuf},
    process::ExitStatus,
    time::Duration,
};
use tokio::{
    process::ChildStdin,
    sync::{mpsc, watch},
    time,
};

use child_exit::{
    LSP_CHILD_DEATH_REAP_TIMEOUT, LspChildWatchdog, lsp_exit_status_label, spawn_lsp_child_watchdog,
};
use command_queue::LspClientCommandQueue;
use read_result::handle_lsp_read_result;
use startup::{StartedLspClient, start_lsp_process};
use status::send_lsp_stopped_status;

/// How long any single stdin write may take before the server counts as
/// wedged. A hung-but-alive server (pipe full, nobody draining it) must not
/// stall this runtime loop forever: read handling, request deadlines, and
/// the child-death watchdog all live on the same loop, and the command
/// queue would silently fill to the `LSP_COMMAND_QUEUE_CAPACITY` bound
/// with document snapshots. On expiry the client stops as `Unexpected` so
/// the restart ladder engages.
pub(super) const LSP_STDIN_WRITE_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) async fn run_lsp_client(
    generation: u64,
    config: LspServerConfig,
    root: PathBuf,
    mut rx: mpsc::Receiver<LspClientCommand>,
    mut shutdown_rx: watch::Receiver<bool>,
    ui_tx: Sender<UiEvent>,
    stderr_log: LspStderrLog,
    watched_files: LspWatchedFilesState,
    capabilities: LspServerCapabilitiesState,
) {
    if shutdown_signal_requested(&mut shutdown_rx) {
        return;
    }

    let Some(StartedLspClient {
        child,
        mut writer,
        mut reader,
        sync_kind,
    }) = start_lsp_process(
        &config,
        &root,
        generation,
        &mut shutdown_rx,
        &ui_tx,
        &stderr_log,
        &watched_files,
        &capabilities,
    )
    .await
    else {
        return;
    };
    let mut watchdog = spawn_lsp_child_watchdog(child);

    let mut next_request_id = 3;
    let mut pending_requests = PendingLspRequests::default();
    let mut command_queue = LspClientCommandQueue::default();
    let mut sync_state = DocumentSyncState::new(sync_kind);
    let mut read_buffer = LspMessageReadBuffer::default();
    let mut shutdown_signal_closed = false;
    let mut unexpected_exit: Option<io::Result<ExitStatus>> = None;

    let stop_reason = loop {
        let next_deadline = pending_requests.earliest_deadline();
        tokio::select! {
            biased;
            changed = shutdown_rx.changed(), if !shutdown_signal_closed => {
                match changed {
                    Ok(()) if shutdown_signal_requested(&mut shutdown_rx) => {
                        break handle_lsp_shutdown_signal(
                            &mut writer,
                            &mut next_request_id,
                            &mut pending_requests,
                            &mut sync_state,
                            &ui_tx,
                        )
                        .await;
                    }
                    Ok(()) => {}
                    Err(_) => {
                        shutdown_signal_closed = true;
                    }
                }
            }
            command = command_queue.recv(&mut rx) => {
                match handle_lsp_command_with_write_deadline(
                    command,
                    &mut writer,
                    &mut next_request_id,
                    &mut pending_requests,
                    &mut sync_state,
                    &ui_tx,
                    LSP_STDIN_WRITE_TIMEOUT,
                )
                .await
                {
                    LspClientCommandOutcome::Continue => {}
                    LspClientCommandOutcome::Stop(reason) => {
                        break reason;
                    }
                }
            }
            death = watchdog.death_rx.recv() => {
                unexpected_exit = death;
                break LspClientStopReason::Unexpected;
            }
            _ = time::sleep_until(next_deadline.unwrap_or_else(time::Instant::now)), if next_deadline.is_some() => {
                let cancels_written = time::timeout(
                    LSP_STDIN_WRITE_TIMEOUT,
                    expire_due_lsp_requests(
                        &mut writer,
                        &mut pending_requests,
                        &config.language,
                        &root,
                        generation,
                        &ui_tx,
                    ),
                )
                .await;
                if cancels_written.is_err() {
                    break LspClientStopReason::Unexpected;
                }
            }
            message = read_message(&mut reader, &mut read_buffer) => {
                let kept_running = time::timeout(
                    LSP_STDIN_WRITE_TIMEOUT,
                    handle_lsp_read_result(
                        message,
                        &config.language,
                        &root,
                        generation,
                        &mut pending_requests,
                        &ui_tx,
                        &mut writer,
                        &stderr_log,
                        &watched_files,
                    ),
                )
                .await
                .unwrap_or_default();
                if !kept_running {
                    break LspClientStopReason::Unexpected;
                }
            }
        }
    };

    messages::forget_created_work_done_progress_tokens_for_server(
        &config.language,
        &root,
        generation,
    );
    match stop_reason {
        LspClientStopReason::Intentional => {
            // Intentional stops never surface an exit status; the graceful
            // exit just guarantees the child is reaped before we return.
            let _ = watchdog.request_graceful_exit().await;
            // Requests still in flight get a typed cancellation failure
            // instead of being dropped silently.
            emit_pending_lsp_request_cancellations_for_server(
                LspServerResultTarget {
                    language: config.language.clone(),
                    root: root.clone(),
                    generation,
                },
                &mut pending_requests,
                &ui_tx,
            );
        }
        LspClientStopReason::Unexpected => {
            let exit_detail =
                lsp_unexpected_stop_exit_detail(unexpected_exit.as_ref(), &mut watchdog).await;
            let detail = lsp_unexpected_stop_detail(exit_detail, &stderr_log);
            emit_pending_lsp_request_failures_for_server(
                LspServerResultTarget {
                    language: config.language.clone(),
                    root: root.clone(),
                    generation,
                },
                &mut pending_requests,
                &ui_tx,
            );
            send_lsp_stopped_status(
                &config.language,
                &root,
                generation,
                detail.as_deref(),
                &ui_tx,
            );
        }
    }
}

/// Handles one queued command under the stdin write deadline. Command
/// dispatch awaits `write_message`/`write_all` + `flush` on the server's
/// stdin pipe, so a wedged server would otherwise stall the runtime loop
/// forever. On expiry the client stops as `Unexpected` (engaging the
/// restart ladder); a partially written frame is acceptable because the
/// connection is torn down right after: stdin closes, the watchdog kills
/// the child, and the restart replays didOpen on a fresh process.
async fn handle_lsp_command_with_write_deadline(
    command: Option<LspClientCommand>,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
    sync_state: &mut DocumentSyncState,
    ui_tx: &Sender<UiEvent>,
    write_deadline: Duration,
) -> LspClientCommandOutcome {
    time::timeout(
        write_deadline,
        handle_lsp_client_command(
            command,
            writer,
            next_request_id,
            pending_requests,
            sync_state,
            ui_tx,
        ),
    )
    .await
    .unwrap_or(LspClientCommandOutcome::Stop(
        LspClientStopReason::Unexpected,
    ))
}

/// Expires requests whose deadline elapsed and notifies the server with
/// best-effort `$/cancelRequest` notifications.
async fn expire_due_lsp_requests(
    writer: &mut ChildStdin,
    pending_requests: &mut PendingLspRequests,
    language: &str,
    root: &Path,
    generation: u64,
    ui_tx: &Sender<UiEvent>,
) {
    let expired = pending_requests.take_expired(time::Instant::now());
    if expired.is_empty() {
        return;
    }
    for (request_id, _) in &expired {
        let _ = write_message(
            writer,
            &LspWireMessage::cancel_request(*request_id).to_json(),
        )
        .await;
    }
    emit_expired_lsp_request_timeouts(
        LspServerResultTarget {
            language: language.to_owned(),
            root: root.to_path_buf(),
            generation,
        },
        expired,
        ui_tx,
    );
}

/// Assembles the optional stop detail from the reaped exit status and the
/// captured stderr tail. The joined detail is sanitized later when the
/// status message is rendered.
fn lsp_unexpected_stop_detail(
    exit_detail: Option<String>,
    stderr_log: &LspStderrLog,
) -> Option<String> {
    let stderr_tail = stderr_log.tail_chars(LSP_STDERR_STATUS_TAIL_CHARS);
    let mut parts = Vec::new();
    if let Some(exit_detail) = exit_detail {
        parts.push(exit_detail);
    }
    if !stderr_tail.is_empty() {
        parts.push(format!("stderr: {stderr_tail}"));
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

async fn handle_lsp_shutdown_signal(
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
    sync_state: &mut DocumentSyncState,
    ui_tx: &Sender<UiEvent>,
) -> LspClientStopReason {
    // The shutdown + exit writes are deadline-bounded like every other
    // stdin write: a server wedged hard enough to refuse them must not
    // hang the runtime task on its way out.
    match time::timeout(
        LSP_STDIN_WRITE_TIMEOUT,
        handle_lsp_client_command(
            Some(LspClientCommand::Shutdown),
            writer,
            next_request_id,
            pending_requests,
            sync_state,
            ui_tx,
        ),
    )
    .await
    {
        Ok(LspClientCommandOutcome::Continue) => LspClientStopReason::Intentional,
        Ok(LspClientCommandOutcome::Stop(reason)) => reason,
        Err(_elapsed) => LspClientStopReason::Unexpected,
    }
}

/// Exit-status detail for an unexpected stop. When the death notification has
/// not been observed yet (for example a stdout EOF that raced with process
/// exit) wait briefly for the watchdog to reap it so the status message can
/// name the exit code.
async fn lsp_unexpected_stop_exit_detail(
    unexpected_exit: Option<&io::Result<ExitStatus>>,
    watchdog: &mut LspChildWatchdog,
) -> Option<String> {
    if let Some(Ok(status)) = unexpected_exit {
        return Some(lsp_exit_status_label(status));
    }
    let reaped = time::timeout(LSP_CHILD_DEATH_REAP_TIMEOUT, watchdog.death_rx.recv())
        .await
        .ok()
        .flatten();
    reaped
        .and_then(|status| status.ok())
        .map(|status| lsp_exit_status_label(&status))
}

pub(super) fn shutdown_signal_requested(shutdown_rx: &mut watch::Receiver<bool>) -> bool {
    *shutdown_rx.borrow_and_update()
}

#[cfg(test)]
mod tests {
    use super::{
        DocumentSyncState, LSP_CHILD_DEATH_REAP_TIMEOUT, LSP_STDIN_WRITE_TIMEOUT,
        handle_lsp_command_with_write_deadline, lsp_unexpected_stop_detail,
        shutdown_signal_requested,
    };
    use crate::lsp_client::command_dispatch::{LspClientCommandOutcome, LspClientStopReason};
    use crate::lsp_client::commands::LspClientCommand;
    use crate::lsp_client::pending::PendingLspRequests;
    use crate::lsp_client::stderr_log::{LSP_STDERR_STATUS_TAIL_CHARS, LspStderrLog};
    use crate::ui_event_channel::ui_event_channel;
    use kuroya_core::{TextBuffer, TextDocumentSyncKindSetting};
    use std::{path::PathBuf, process::Stdio, time::Duration};
    use tokio::{process::Command, sync::watch};

    fn full_document_sync_state() -> DocumentSyncState {
        DocumentSyncState::new(TextDocumentSyncKindSetting::Full)
    }

    fn text_snapshot(text: &str) -> kuroya_core::TextSnapshot {
        TextBuffer::from_text(1, None, text.to_owned()).text_snapshot()
    }

    fn did_open_with_payload(size: usize) -> LspClientCommand {
        LspClientCommand::DidOpen {
            id: 1,
            path: PathBuf::from("src/main.rs"),
            language: "rust".to_owned(),
            version: 1,
            text: text_snapshot(&"x".repeat(size)),
        }
    }

    /// A live server process that never reads its stdin pipe, so writes
    /// into it block once the pipe buffer fills.
    async fn stdin_ignored_child() -> (tokio::process::Child, tokio::process::ChildStdin) {
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("cmd");
            command.args(["/C", "ping -n 30 127.0.0.1 > NUL"]);
            command
        };

        #[cfg(not(windows))]
        let mut command = {
            let mut command = Command::new("sh");
            command.args(["-c", "sleep 30"]);
            command
        };

        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn stdin-ignoring child process");
        let stdin = child.stdin.take().expect("child stdin is piped");
        (child, stdin)
    }

    /// A live server process that drains its stdin, so writes complete.
    async fn stdin_sink_child() -> (tokio::process::Child, tokio::process::ChildStdin) {
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
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn stdin sink child process");
        let stdin = child.stdin.take().expect("child stdin is piped");
        (child, stdin)
    }

    #[test]
    fn unexpected_stop_detail_combines_exit_status_and_stderr_tail() {
        let stderr_log = LspStderrLog::default();
        assert_eq!(
            lsp_unexpected_stop_detail(None, &stderr_log),
            None,
            "no exit status and no stderr means no detail"
        );

        stderr_log.push_line("panic: server exploded");
        assert_eq!(
            lsp_unexpected_stop_detail(Some("exit code 1".to_owned()), &stderr_log),
            Some("exit code 1; stderr: panic: server exploded".to_owned())
        );
        assert_eq!(
            lsp_unexpected_stop_detail(None, &stderr_log),
            Some("stderr: panic: server exploded".to_owned())
        );
    }

    #[test]
    fn unexpected_stop_detail_bounds_the_stderr_tail() {
        let stderr_log = LspStderrLog::default();
        stderr_log.push_line(&"x".repeat(LSP_STDERR_STATUS_TAIL_CHARS * 4));

        let detail = lsp_unexpected_stop_detail(None, &stderr_log)
            .expect("stderr tail should produce detail");

        assert!(detail.starts_with("stderr: "));
        assert!(
            detail.chars().count() <= "stderr: ".len() + LSP_STDERR_STATUS_TAIL_CHARS,
            "tail must be bounded, got {} chars",
            detail.chars().count()
        );
    }

    #[test]
    fn shutdown_signal_requested_reads_and_marks_latest_signal() {
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

        assert!(!shutdown_signal_requested(&mut shutdown_rx));

        shutdown_tx
            .send(true)
            .expect("shutdown receiver should be live");

        assert!(shutdown_signal_requested(&mut shutdown_rx));
        assert!(
            !shutdown_rx
                .has_changed()
                .expect("shutdown sender should still be live")
        );
    }

    #[test]
    fn child_death_reap_window_is_bounded() {
        assert_eq!(LSP_CHILD_DEATH_REAP_TIMEOUT, Duration::from_millis(500));
    }

    #[test]
    fn stdin_write_deadline_is_ten_seconds() {
        assert_eq!(LSP_STDIN_WRITE_TIMEOUT, Duration::from_secs(10));
    }

    #[tokio::test]
    async fn write_deadline_stops_as_unexpected_when_the_server_never_reads_stdin() {
        let (mut child, mut writer) = stdin_ignored_child().await;
        let mut next_request_id = 2;
        let mut pending_requests = PendingLspRequests::default();
        let (ui_tx, _ui_rx) = ui_event_channel();

        // Far larger than any OS pipe buffer, so the write must block once
        // the pipe fills and the deadline is what stops the client.
        let outcome = handle_lsp_command_with_write_deadline(
            Some(did_open_with_payload(8 * 1024 * 1024)),
            &mut writer,
            &mut next_request_id,
            &mut pending_requests,
            &mut full_document_sync_state(),
            &ui_tx,
            Duration::from_millis(100),
        )
        .await;
        drop(writer);
        let _ = child.kill().await;

        assert_eq!(
            outcome,
            LspClientCommandOutcome::Stop(LspClientStopReason::Unexpected),
            "a wedged stdin write must surface as an unexpected stop"
        );
        assert_eq!(next_request_id, 2);
        assert!(pending_requests.is_empty());
    }

    #[tokio::test]
    async fn write_deadline_passes_commands_through_when_the_server_drains_stdin() {
        let (mut child, mut writer) = stdin_sink_child().await;
        let mut next_request_id = 2;
        let mut pending_requests = PendingLspRequests::default();
        let (ui_tx, _ui_rx) = ui_event_channel();

        let outcome = handle_lsp_command_with_write_deadline(
            Some(did_open_with_payload(64)),
            &mut writer,
            &mut next_request_id,
            &mut pending_requests,
            &mut full_document_sync_state(),
            &ui_tx,
            Duration::from_secs(5),
        )
        .await;
        drop(writer);
        let _ = child.kill().await;

        assert_eq!(outcome, LspClientCommandOutcome::Continue);
    }
}
