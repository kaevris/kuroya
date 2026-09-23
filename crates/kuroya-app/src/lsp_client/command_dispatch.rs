use crate::lsp_client::pending::PendingLspRequests;
mod direct_response;
pub(super) mod document_sync;
mod family;
mod lifecycle;

use super::{
    commands::LspClientCommand, pending::lsp_request_target_is_valid,
    request_dispatch::handle_lsp_request_command,
};
use crate::ui_event_channel::Sender;
use crate::ui_events::UiEvent;
use document_sync::DocumentSyncState;
use family::{ClientCommandFamily, client_command_family};
use kuroya_core::BufferId;
use std::path::Path;
use tokio::process::ChildStdin;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LspClientCommandOutcome {
    Continue,
    Stop(LspClientStopReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LspClientStopReason {
    Unexpected,
    Intentional,
}

pub(super) async fn handle_lsp_client_command(
    command: Option<LspClientCommand>,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
    sync_state: &mut DocumentSyncState,
    ui_tx: &Sender<UiEvent>,
) -> LspClientCommandOutcome {
    if command
        .as_ref()
        .is_some_and(|command| !lsp_client_command_target_is_valid(command))
    {
        return LspClientCommandOutcome::Continue;
    }

    let command_family = client_command_family(command);
    let stop_reason = command_family.stop_reason();

    match command_family {
        ClientCommandFamily::DocumentSync(command) => {
            if !document_sync::handle_document_sync_command(command, writer, sync_state, ui_tx)
                .await
            {
                return LspClientCommandOutcome::Stop(LspClientStopReason::Unexpected);
            }
        }
        ClientCommandFamily::DirectResponse(command) => {
            if !direct_response::handle_direct_response_command(command, writer).await {
                return LspClientCommandOutcome::Stop(LspClientStopReason::Unexpected);
            }
        }
        ClientCommandFamily::Shutdown => {
            lifecycle::handle_shutdown_messages(writer).await;
            return LspClientCommandOutcome::Stop(
                stop_reason.unwrap_or(LspClientStopReason::Intentional),
            );
        }
        ClientCommandFamily::Request(command) => {
            if !handle_lsp_request_command(command, writer, next_request_id, pending_requests).await
            {
                return LspClientCommandOutcome::Stop(LspClientStopReason::Unexpected);
            }
        }
        ClientCommandFamily::Closed => {
            return LspClientCommandOutcome::Stop(
                stop_reason.unwrap_or(LspClientStopReason::Intentional),
            );
        }
    }

    LspClientCommandOutcome::Continue
}

fn lsp_client_command_target_is_valid(command: &LspClientCommand) -> bool {
    match command {
        LspClientCommand::DidOpen { id, path, .. }
        | LspClientCommand::DidChange { id, path, .. }
        | LspClientCommand::Hover { id, path, .. }
        | LspClientCommand::DocumentHighlights { id, path, .. }
        | LspClientCommand::Definition { id, path, .. }
        | LspClientCommand::PrepareRename { id, path, .. }
        | LspClientCommand::PrepareCallHierarchy { id, path, .. }
        | LspClientCommand::CallHierarchyIncoming { id, path, .. }
        | LspClientCommand::CallHierarchyOutgoing { id, path, .. }
        | LspClientCommand::PrepareTypeHierarchy { id, path, .. }
        | LspClientCommand::TypeHierarchySupertypes { id, path, .. }
        | LspClientCommand::TypeHierarchySubtypes { id, path, .. }
        | LspClientCommand::References { id, path, .. }
        | LspClientCommand::Rename { id, path, .. }
        | LspClientCommand::DocumentSymbols { id, path, .. }
        | LspClientCommand::FoldingRanges { id, path, .. }
        | LspClientCommand::InlayHints { id, path, .. }
        | LspClientCommand::CodeLenses { id, path, .. }
        | LspClientCommand::ResolveCodeLens { id, path, .. }
        | LspClientCommand::ExecuteCommand { id, path, .. }
        | LspClientCommand::SemanticTokens { id, path, .. }
        | LspClientCommand::WorkspaceSymbols { id, path, .. }
        | LspClientCommand::Completion { id, path, .. }
        | LspClientCommand::ResolveCompletionItem { id, path, .. }
        | LspClientCommand::SignatureHelp { id, path, .. }
        | LspClientCommand::Formatting { id, path, .. }
        | LspClientCommand::CodeActions { id, path, .. }
        | LspClientCommand::ResolveCodeAction { id, path, .. } => {
            command_buffer_target_is_valid(*id, path)
        }
        LspClientCommand::DidSave { path } | LspClientCommand::DidClose { path } => {
            command_path_is_valid(path)
        }
        LspClientCommand::ApplyWorkspaceEditResponse { .. }
        | LspClientCommand::DidChangeWatchedFiles { .. }
        | LspClientCommand::Shutdown => true,
    }
}

fn command_buffer_target_is_valid(id: BufferId, path: &Path) -> bool {
    lsp_request_target_is_valid(id, path)
}

fn command_path_is_valid(path: &Path) -> bool {
    !path.as_os_str().is_empty()
}

#[cfg(test)]
mod tests {
    use super::{
        DocumentSyncState, LspClientCommandOutcome, LspClientStopReason, handle_lsp_client_command,
        lsp_client_command_target_is_valid,
    };
    use crate::lsp_client::pending::PendingLspRequests;
    use crate::{
        lsp_client::{commands::LspClientCommand, pending::PendingLspRequest},
        ui_event_channel::ui_event_channel,
    };
    use kuroya_core::{LspRequestId, TextBuffer, TextDocumentSyncKindSetting};
    use std::{path::PathBuf, process::Stdio};
    use tokio::process::{Child, ChildStdin, Command};

    fn full_document_sync_state() -> DocumentSyncState {
        DocumentSyncState::new(TextDocumentSyncKindSetting::Full)
    }

    async fn exited_child_with_stdin() -> (Child, ChildStdin) {
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
        (child, stdin)
    }

    async fn stdin_sink_child() -> (Child, ChildStdin) {
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
            .spawn()
            .expect("spawn stdin sink child process");
        let stdin = child.stdin.take().expect("child stdin is piped");
        (child, stdin)
    }

    fn text_snapshot(text: &str) -> kuroya_core::TextSnapshot {
        TextBuffer::from_text(1, None, text.to_owned()).text_snapshot()
    }

    #[tokio::test]
    async fn request_write_failure_stops_as_unexpected() {
        let (mut child, mut writer) = exited_child_with_stdin().await;
        let mut next_request_id = 2;
        let mut pending_requests = PendingLspRequests::default();
        let (ui_tx, _ui_rx) = ui_event_channel();

        let outcome = handle_lsp_client_command(
            Some(LspClientCommand::Hover {
                id: 1,
                path: PathBuf::from("src/main.rs"),
                version: 3,
                line: 0,
                character: 0,
            }),
            &mut writer,
            &mut next_request_id,
            &mut pending_requests,
            &mut full_document_sync_state(),
            &ui_tx,
        )
        .await;
        let _ = child.kill().await;

        assert_eq!(
            outcome,
            LspClientCommandOutcome::Stop(LspClientStopReason::Unexpected)
        );
        assert_eq!(next_request_id, 4);
        assert!(pending_requests.is_empty());
    }

    #[tokio::test]
    async fn direct_response_does_not_allocate_or_cancel_pending_requests() {
        let (mut child, mut writer) = stdin_sink_child().await;
        let mut next_request_id = 31;
        let path = PathBuf::from("src/main.rs");
        let mut pending_requests = PendingLspRequests::from([(
            7,
            PendingLspRequest::Hover {
                id: 1,
                path: path.clone(),
                version: 2,
                line: 1,
                character: 1,
            },
        )]);
        let (ui_tx, _ui_rx) = ui_event_channel();

        let outcome = handle_lsp_client_command(
            Some(LspClientCommand::ApplyWorkspaceEditResponse {
                request_id: LspRequestId::Number(17),
                applied: false,
                failure_reason: Some("buffer changed".to_owned()),
            }),
            &mut writer,
            &mut next_request_id,
            &mut pending_requests,
            &mut full_document_sync_state(),
            &ui_tx,
        )
        .await;

        assert_eq!(outcome, LspClientCommandOutcome::Continue);
        assert_eq!(next_request_id, 31);
        assert_eq!(pending_requests.len(), 1);
        assert!(matches!(
            pending_requests.get(&7),
            Some(PendingLspRequest::Hover {
                path: pending_path,
                version: 2,
                line: 1,
                character: 1,
                ..
            }) if pending_path == &path
        ));
        drop(writer);
        let _ = child.kill().await;
    }

    #[test]
    fn command_target_guard_rejects_zero_buffer_ids_and_empty_paths() {
        assert!(!lsp_client_command_target_is_valid(
            &LspClientCommand::DidChange {
                id: 0,
                path: PathBuf::from("src/main.rs"),
                version: 1,
                text: text_snapshot("change"),
            }
        ));
        assert!(!lsp_client_command_target_is_valid(
            &LspClientCommand::DidSave {
                path: PathBuf::new(),
            }
        ));
        assert!(!lsp_client_command_target_is_valid(
            &LspClientCommand::Hover {
                id: 1,
                path: PathBuf::new(),
                version: 1,
                line: 0,
                character: 0,
            }
        ));
        assert!(lsp_client_command_target_is_valid(
            &LspClientCommand::DidOpen {
                id: 1,
                path: PathBuf::from("src/main.rs"),
                language: "rust".to_owned(),
                version: 0,
                text: text_snapshot("open"),
            }
        ));
        assert!(lsp_client_command_target_is_valid(
            &LspClientCommand::ApplyWorkspaceEditResponse {
                request_id: LspRequestId::Number(17),
                applied: true,
                failure_reason: None,
            }
        ));
    }

    #[tokio::test]
    async fn invalid_document_sync_command_is_ignored_without_write_or_synced_event() {
        let (mut child, mut writer) = exited_child_with_stdin().await;
        let mut next_request_id = 31;
        let mut pending_requests = PendingLspRequests::default();
        let (ui_tx, ui_rx) = ui_event_channel();

        let outcome = handle_lsp_client_command(
            Some(LspClientCommand::DidOpen {
                id: 0,
                path: PathBuf::from("src/main.rs"),
                language: "rust".to_owned(),
                version: 1,
                text: text_snapshot("open"),
            }),
            &mut writer,
            &mut next_request_id,
            &mut pending_requests,
            &mut full_document_sync_state(),
            &ui_tx,
        )
        .await;
        let _ = child.kill().await;

        assert_eq!(outcome, LspClientCommandOutcome::Continue);
        assert_eq!(next_request_id, 31);
        assert!(pending_requests.is_empty());
        assert!(ui_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn invalid_request_command_does_not_cancel_pending_or_allocate_request_id() {
        let (mut child, mut writer) = exited_child_with_stdin().await;
        let mut next_request_id = 31;
        let path = PathBuf::from("src/main.rs");
        let mut pending_requests = PendingLspRequests::from([(
            7,
            PendingLspRequest::Hover {
                id: 1,
                path: path.clone(),
                version: 2,
                line: 1,
                character: 1,
            },
        )]);
        let (ui_tx, _ui_rx) = ui_event_channel();

        let outcome = handle_lsp_client_command(
            Some(LspClientCommand::Hover {
                id: 0,
                path,
                version: 3,
                line: 4,
                character: 5,
            }),
            &mut writer,
            &mut next_request_id,
            &mut pending_requests,
            &mut full_document_sync_state(),
            &ui_tx,
        )
        .await;
        let _ = child.kill().await;

        assert_eq!(outcome, LspClientCommandOutcome::Continue);
        assert_eq!(next_request_id, 31);
        assert!(matches!(
            pending_requests.get(&7),
            Some(PendingLspRequest::Hover {
                version: 2,
                line: 1,
                character: 1,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn shutdown_command_stops_intentionally_without_touching_pending_requests() {
        let (mut child, mut writer) = stdin_sink_child().await;
        let mut next_request_id = 31;
        let path = PathBuf::from("src/main.rs");
        let mut pending_requests = PendingLspRequests::from([(
            7,
            PendingLspRequest::Hover {
                id: 1,
                path: path.clone(),
                version: 2,
                line: 1,
                character: 1,
            },
        )]);
        let (ui_tx, _ui_rx) = ui_event_channel();

        let outcome = handle_lsp_client_command(
            Some(LspClientCommand::Shutdown),
            &mut writer,
            &mut next_request_id,
            &mut pending_requests,
            &mut full_document_sync_state(),
            &ui_tx,
        )
        .await;
        drop(writer);
        let _ = child.kill().await;

        assert_eq!(
            outcome,
            LspClientCommandOutcome::Stop(LspClientStopReason::Intentional)
        );
        assert_eq!(next_request_id, 31);
        assert!(matches!(
            pending_requests.get(&7),
            Some(PendingLspRequest::Hover {
                path: pending_path,
                version: 2,
                line: 1,
                character: 1,
                ..
            }) if pending_path == &path
        ));
    }
}
