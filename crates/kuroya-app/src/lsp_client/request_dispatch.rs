use crate::lsp_client::pending::PendingLspRequests;
mod edits;
mod family;
mod navigation;
mod request_id;
mod symbols;

use super::{
    commands::LspClientCommand,
    pending::{MAX_PENDING_LSP_REQUESTS, PendingLspRequest},
    wire::write_message,
};
use family::{RequestCommandFamily, request_command_family};
use kuroya_core::LspWireMessage;
use request_id::reserve_request_id;
use serde_json::Value;
use tokio::process::ChildStdin;

pub(super) async fn handle_lsp_request_command(
    command: LspClientCommand,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    let Some(family) = request_command_family(&command) else {
        return true;
    };
    let has_exact_pending_request = match pending_dispatch_scan(&command) {
        Some(PendingDispatchScan::Symbols) => {
            let Some(has_exact_pending_request) =
                write_cancel_symbol_request_messages(writer, pending_requests, &command).await
            else {
                return false;
            };
            has_exact_pending_request
        }
        Some(PendingDispatchScan::Coalesced) => {
            let Some(has_exact_pending_request) =
                write_cancel_coalesced_request_messages(writer, pending_requests, &command).await
            else {
                return false;
            };
            has_exact_pending_request
        }
        None => false,
    };
    if has_exact_pending_request {
        return true;
    }

    match family {
        RequestCommandFamily::Navigation => {
            navigation::handle_navigation_request_command(
                command,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        RequestCommandFamily::Symbols => {
            symbols::handle_symbol_request_command(
                command,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        RequestCommandFamily::Edits => {
            edits::handle_edit_request_command(command, writer, next_request_id, pending_requests)
                .await
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingDispatchAction {
    Keep,
    Cancel,
    CancelAfterPrimary,
    Exact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingDispatchScan {
    Coalesced,
    Symbols,
}

fn pending_dispatch_scan(command: &LspClientCommand) -> Option<PendingDispatchScan> {
    match command {
        LspClientCommand::Hover { .. }
        | LspClientCommand::DocumentHighlights { .. }
        | LspClientCommand::Completion { .. }
        | LspClientCommand::SignatureHelp { .. }
        | LspClientCommand::CodeActions { .. } => Some(PendingDispatchScan::Coalesced),
        LspClientCommand::DocumentSymbols { .. }
        | LspClientCommand::FoldingRanges { .. }
        | LspClientCommand::InlayHints { .. }
        | LspClientCommand::CodeLenses { .. }
        | LspClientCommand::SemanticTokens { .. }
        | LspClientCommand::WorkspaceSymbols { .. } => Some(PendingDispatchScan::Symbols),
        _ => None,
    }
}

struct PendingRequestIdBuffer {
    ids: [u64; MAX_PENDING_LSP_REQUESTS],
    len: usize,
}

impl PendingRequestIdBuffer {
    fn new() -> Self {
        Self {
            ids: [0; MAX_PENDING_LSP_REQUESTS],
            len: 0,
        }
    }

    fn push(&mut self, request_id: u64) {
        assert!(self.len < self.ids.len());
        self.ids[self.len] = request_id;
        self.len += 1;
    }

    fn sorted_slice(&mut self) -> &[u64] {
        let ids = &mut self.ids[..self.len];
        ids.sort_unstable();
        ids
    }
}

async fn write_cancel_symbol_request_messages(
    writer: &mut ChildStdin,
    pending_requests: &mut PendingLspRequests,
    command: &LspClientCommand,
) -> Option<bool> {
    let mut request_ids = PendingRequestIdBuffer::new();
    let mut follow_up_request_ids = PendingRequestIdBuffer::new();
    let mut has_exact_pending_request = false;

    for (request_id, pending) in pending_requests.iter() {
        match symbol_pending_dispatch_action(command, pending) {
            PendingDispatchAction::Keep => {}
            PendingDispatchAction::Cancel => request_ids.push(request_id),
            PendingDispatchAction::CancelAfterPrimary => follow_up_request_ids.push(request_id),
            PendingDispatchAction::Exact => has_exact_pending_request = true,
        }
    }

    if !write_cancel_request_messages(writer, pending_requests, request_ids.sorted_slice()).await {
        return None;
    }
    if !write_cancel_request_messages(
        writer,
        pending_requests,
        follow_up_request_ids.sorted_slice(),
    )
    .await
    {
        return None;
    }

    Some(has_exact_pending_request)
}

async fn write_cancel_coalesced_request_messages(
    writer: &mut ChildStdin,
    pending_requests: &mut PendingLspRequests,
    command: &LspClientCommand,
) -> Option<bool> {
    let mut request_ids = PendingRequestIdBuffer::new();
    let mut has_exact_pending_request = false;

    for (request_id, pending) in pending_requests.iter() {
        match coalesced_pending_dispatch_action(command, pending) {
            PendingDispatchAction::Keep | PendingDispatchAction::CancelAfterPrimary => {}
            PendingDispatchAction::Cancel => request_ids.push(request_id),
            PendingDispatchAction::Exact => has_exact_pending_request = true,
        }
    }

    if write_cancel_request_messages(writer, pending_requests, request_ids.sorted_slice()).await {
        Some(has_exact_pending_request)
    } else {
        None
    }
}

fn coalesced_pending_dispatch_action(
    command: &LspClientCommand,
    pending: &PendingLspRequest,
) -> PendingDispatchAction {
    match (command, pending) {
        (
            LspClientCommand::Hover {
                id,
                path,
                version,
                line,
                character,
            },
            PendingLspRequest::Hover {
                id: pending_id,
                path: pending_path,
                version: pending_version,
                line: pending_line,
                character: pending_character,
            },
        )
        | (
            LspClientCommand::DocumentHighlights {
                id,
                path,
                version,
                line,
                character,
            },
            PendingLspRequest::DocumentHighlights {
                id: pending_id,
                path: pending_path,
                version: pending_version,
                line: pending_line,
                character: pending_character,
            },
        )
        | (
            LspClientCommand::Completion {
                id,
                path,
                version,
                line,
                character,
            },
            PendingLspRequest::Completion {
                id: pending_id,
                path: pending_path,
                version: pending_version,
                line: pending_line,
                character: pending_character,
            },
        )
        | (
            LspClientCommand::SignatureHelp {
                id,
                path,
                version,
                line,
                character,
            },
            PendingLspRequest::SignatureHelp {
                id: pending_id,
                path: pending_path,
                version: pending_version,
                line: pending_line,
                character: pending_character,
            },
        ) if id == pending_id && path == pending_path => {
            if version == pending_version && line == pending_line && character == pending_character
            {
                PendingDispatchAction::Exact
            } else {
                PendingDispatchAction::Cancel
            }
        }
        (
            LspClientCommand::CodeActions { id, path, .. },
            PendingLspRequest::CodeActions {
                id: pending_id,
                path: pending_path,
                ..
            },
        ) if id == pending_id && path == pending_path => PendingDispatchAction::Cancel,
        _ => PendingDispatchAction::Keep,
    }
}

fn symbol_pending_dispatch_action(
    command: &LspClientCommand,
    pending: &PendingLspRequest,
) -> PendingDispatchAction {
    match (command, pending) {
        (
            LspClientCommand::DocumentSymbols { id, path, version },
            PendingLspRequest::DocumentSymbols {
                id: pending_id,
                path: pending_path,
                version: pending_version,
            },
        )
        | (
            LspClientCommand::FoldingRanges { id, path, version },
            PendingLspRequest::FoldingRanges {
                id: pending_id,
                path: pending_path,
                version: pending_version,
            },
        )
        | (
            LspClientCommand::SemanticTokens { id, path, version },
            PendingLspRequest::SemanticTokens {
                id: pending_id,
                path: pending_path,
                version: pending_version,
            },
        ) if id == pending_id && path == pending_path => {
            if version == pending_version {
                PendingDispatchAction::Exact
            } else {
                PendingDispatchAction::Cancel
            }
        }
        (
            LspClientCommand::InlayHints {
                id,
                path,
                version,
                end_line,
                end_character,
            },
            PendingLspRequest::InlayHints {
                id: pending_id,
                path: pending_path,
                version: pending_version,
                end_line: pending_end_line,
                end_character: pending_end_character,
            },
        ) if id == pending_id && path == pending_path => {
            if version == pending_version
                && end_line == pending_end_line
                && end_character == pending_end_character
            {
                PendingDispatchAction::Exact
            } else {
                PendingDispatchAction::Cancel
            }
        }
        (
            LspClientCommand::CodeLenses { id, path, version },
            PendingLspRequest::CodeLenses {
                id: pending_id,
                path: pending_path,
                version: pending_version,
            },
        ) if id == pending_id && path == pending_path => {
            if version == pending_version {
                PendingDispatchAction::Exact
            } else {
                PendingDispatchAction::Cancel
            }
        }
        (
            LspClientCommand::WorkspaceSymbols { id, path, query },
            PendingLspRequest::WorkspaceSymbols {
                id: pending_id,
                path: pending_path,
                query: pending_query,
            },
        ) if id == pending_id && path == pending_path => {
            if query == pending_query {
                PendingDispatchAction::Exact
            } else {
                PendingDispatchAction::Cancel
            }
        }
        (
            LspClientCommand::CodeLenses { id, path, .. },
            PendingLspRequest::ResolveCodeLens {
                id: pending_id,
                path: pending_path,
                ..
            },
        ) if id == pending_id && path == pending_path => PendingDispatchAction::CancelAfterPrimary,
        _ => PendingDispatchAction::Keep,
    }
}

async fn write_cancel_request_messages(
    writer: &mut ChildStdin,
    pending_requests: &mut PendingLspRequests,
    request_ids: &[u64],
) -> bool {
    for request_id in request_ids {
        if write_message(
            writer,
            &LspWireMessage::cancel_request(*request_id).to_json(),
        )
        .await
        .is_err()
        {
            return false;
        }
        pending_requests.remove(request_id);
    }
    true
}

pub(in crate::lsp_client) async fn write_request_message(
    writer: &mut ChildStdin,
    pending_requests: &mut PendingLspRequests,
    request_id: u64,
    message: Value,
) -> bool {
    if write_message(writer, &message).await.is_ok() {
        true
    } else {
        pending_requests.remove(&request_id);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{PendingDispatchScan, handle_lsp_request_command, pending_dispatch_scan};
    use crate::lsp_client::pending::PendingLspRequests;
    use crate::lsp_client::{commands::LspClientCommand, pending::PendingLspRequest};
    use kuroya_core::LspCodeLens;
    use std::{path::PathBuf, process::Stdio};
    use tokio::process::{Child, ChildStdin, Command};

    async fn exited_child_stdin() -> ChildStdin {
        #[cfg(windows)]
        {
            let mut command = {
                let mut command = Command::new("cmd");
                command.args(["/C", "exit 0"]);
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

        #[cfg(not(windows))]
        {
            use tokio::io::AsyncWriteExt;

            let mut command = {
                let mut command = Command::new("sh");
                command.args(["-c", "exec 0<&-; sleep 30"]);
                command
            };

            let mut child = command
                .stdin(Stdio::piped())
                .spawn()
                .expect("spawn child process with stdin");
            let mut stdin = child.stdin.take().expect("child stdin is piped");

            let mut pipe_broken = false;
            for _ in 0..500 {
                match stdin.write(b"\0").await {
                    Err(_) => {
                        pipe_broken = true;
                        break;
                    }
                    Ok(_) => {
                        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                    }
                }
            }

            let _ = child.kill().await;
            let _ = child.wait().await;
            assert!(
                pipe_broken,
                "child stdin pipe should close before tests simulate write failures"
            );
            stdin
        }
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

    async fn stdin_echo_child() -> (Child, ChildStdin) {
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("cmd");
            command.args(["/C", "more"]);
            command
        };

        #[cfg(not(windows))]
        let mut command = {
            let mut command = Command::new("sh");
            command.args(["-c", "cat"]);
            command
        };

        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn stdin echo child process");
        let stdin = child.stdin.take().expect("child stdin is piped");
        (child, stdin)
    }

    #[test]
    fn pending_dispatch_scan_is_limited_to_coalescing_request_commands() {
        let path = PathBuf::from("src/main.rs");

        assert_eq!(
            pending_dispatch_scan(&LspClientCommand::Hover {
                id: 1,
                path: path.clone(),
                version: 3,
                line: 4,
                character: 5,
            }),
            Some(PendingDispatchScan::Coalesced)
        );
        assert_eq!(
            pending_dispatch_scan(&LspClientCommand::CodeLenses {
                id: 1,
                path: path.clone(),
                version: 3,
            }),
            Some(PendingDispatchScan::Symbols)
        );
        assert_eq!(
            pending_dispatch_scan(&LspClientCommand::Definition {
                id: 1,
                path: path.clone(),
                version: 3,
                line: 4,
                character: 5,
            }),
            None
        );
        assert_eq!(
            pending_dispatch_scan(&LspClientCommand::ResolveCodeLens {
                id: 1,
                path,
                version: 3,
                lens: LspCodeLens {
                    line: 4,
                    column: 5,
                    title: String::new(),
                    command: None,
                    command_arguments: None,
                    resolve_payload: None,
                },
            }),
            None
        );
    }

    #[tokio::test]
    async fn request_write_failure_removes_pending_request() {
        let mut writer = exited_child_stdin().await;
        let mut next_request_id = 7;
        let mut pending_requests = PendingLspRequests::default();

        let wrote = handle_lsp_request_command(
            LspClientCommand::Completion {
                id: 1,
                path: PathBuf::from("src/main.rs"),
                version: 3,
                line: 0,
                character: 0,
            },
            &mut writer,
            &mut next_request_id,
            &mut pending_requests,
        )
        .await;

        assert!(!wrote);
        assert_eq!(next_request_id, 8);
        assert!(pending_requests.is_empty());
    }

    #[tokio::test]
    async fn inline_hierarchy_write_failure_removes_pending_request() {
        let mut writer = exited_child_stdin().await;
        let mut next_request_id = 11;
        let mut pending_requests = PendingLspRequests::default();

        let wrote = handle_lsp_request_command(
            LspClientCommand::PrepareCallHierarchy {
                id: 1,
                path: PathBuf::from("src/main.rs"),
                version: 3,
                line: 0,
                character: 0,
            },
            &mut writer,
            &mut next_request_id,
            &mut pending_requests,
        )
        .await;

        assert!(!wrote);
        assert_eq!(next_request_id, 12);
        assert!(pending_requests.is_empty());
    }

    #[tokio::test]
    async fn request_dispatch_cancels_superseded_pending_ui_request() {
        let (mut child, mut writer) = stdin_sink_child().await;
        let mut next_request_id = 20;
        let path = PathBuf::from("src/main.rs");
        let mut pending_requests = PendingLspRequests::from([
            (
                7,
                PendingLspRequest::Hover {
                    id: 1,
                    path: path.clone(),
                    version: 2,
                    line: 1,
                    character: 1,
                },
            ),
            (
                8,
                PendingLspRequest::Completion {
                    id: 1,
                    path: path.clone(),
                    version: 2,
                    line: 1,
                    character: 1,
                },
            ),
        ]);

        let wrote = handle_lsp_request_command(
            LspClientCommand::Hover {
                id: 1,
                path,
                version: 3,
                line: 4,
                character: 5,
            },
            &mut writer,
            &mut next_request_id,
            &mut pending_requests,
        )
        .await;

        assert!(wrote);
        assert_eq!(next_request_id, 21);
        assert!(!pending_requests.contains_key(&7));
        assert!(pending_requests.contains_key(&8));
        assert!(matches!(
            pending_requests.get(&20),
            Some(PendingLspRequest::Hover {
                version: 3,
                line: 4,
                character: 5,
                ..
            })
        ));
        drop(writer);
        let _ = child.kill().await;
    }

    #[tokio::test]
    async fn request_dispatch_reuses_exact_pending_ui_request_without_writing() {
        let mut writer = exited_child_stdin().await;
        let mut next_request_id = 20;
        let path = PathBuf::from("src/main.rs");
        let mut pending_requests = PendingLspRequests::from([(
            7,
            PendingLspRequest::Hover {
                id: 1,
                path: path.clone(),
                version: 3,
                line: 4,
                character: 5,
            },
        )]);

        let wrote = handle_lsp_request_command(
            LspClientCommand::Hover {
                id: 1,
                path,
                version: 3,
                line: 4,
                character: 5,
            },
            &mut writer,
            &mut next_request_id,
            &mut pending_requests,
        )
        .await;

        assert!(wrote);
        assert_eq!(next_request_id, 20);
        assert!(matches!(
            pending_requests.get(&7),
            Some(PendingLspRequest::Hover {
                version: 3,
                line: 4,
                character: 5,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn request_dispatch_reuses_exact_pending_after_canceling_stale_same_family_request() {
        let (child, mut writer) = stdin_echo_child().await;
        let mut next_request_id = 20;
        let path = PathBuf::from("src/main.rs");
        let mut pending_requests = PendingLspRequests::from([
            (
                7,
                PendingLspRequest::Hover {
                    id: 1,
                    path: path.clone(),
                    version: 2,
                    line: 1,
                    character: 1,
                },
            ),
            (
                8,
                PendingLspRequest::Hover {
                    id: 1,
                    path: path.clone(),
                    version: 3,
                    line: 4,
                    character: 5,
                },
            ),
        ]);

        let wrote = handle_lsp_request_command(
            LspClientCommand::Hover {
                id: 1,
                path,
                version: 3,
                line: 4,
                character: 5,
            },
            &mut writer,
            &mut next_request_id,
            &mut pending_requests,
        )
        .await;

        assert!(wrote);
        assert_eq!(next_request_id, 20);
        assert!(!pending_requests.contains_key(&7));
        assert!(matches!(
            pending_requests.get(&8),
            Some(PendingLspRequest::Hover {
                version: 3,
                line: 4,
                character: 5,
                ..
            })
        ));
        assert!(!pending_requests.contains_key(&20));
        drop(writer);
        let output = child
            .wait_with_output()
            .await
            .expect("stdin echo child exits cleanly");
        let output = String::from_utf8(output.stdout).expect("stdout is utf8");
        assert!(output.contains("\"method\":\"$/cancelRequest\""));
        assert!(output.contains("\"params\":{\"id\":7}"));
        assert!(!output.contains("textDocument/hover"));
    }

    #[tokio::test]
    async fn request_dispatch_reports_cancel_failure_before_reusing_exact_pending_request() {
        let mut writer = exited_child_stdin().await;
        let mut next_request_id = 20;
        let path = PathBuf::from("src/main.rs");
        let mut pending_requests = PendingLspRequests::from([
            (
                7,
                PendingLspRequest::Hover {
                    id: 1,
                    path: path.clone(),
                    version: 2,
                    line: 1,
                    character: 1,
                },
            ),
            (
                8,
                PendingLspRequest::Hover {
                    id: 1,
                    path: path.clone(),
                    version: 3,
                    line: 4,
                    character: 5,
                },
            ),
        ]);

        let wrote = handle_lsp_request_command(
            LspClientCommand::Hover {
                id: 1,
                path,
                version: 3,
                line: 4,
                character: 5,
            },
            &mut writer,
            &mut next_request_id,
            &mut pending_requests,
        )
        .await;

        assert!(!wrote);
        assert_eq!(next_request_id, 20);
        assert!(pending_requests.contains_key(&7));
        assert!(pending_requests.contains_key(&8));
        assert!(!pending_requests.contains_key(&20));
    }

    #[tokio::test]
    async fn request_dispatch_reuses_exact_pending_code_lenses_after_cleanup() {
        let (child, mut writer) = stdin_echo_child().await;
        let mut next_request_id = 20;
        let path = PathBuf::from("src/main.rs");
        let mut pending_requests = PendingLspRequests::from([
            (
                7,
                PendingLspRequest::CodeLenses {
                    id: 1,
                    path: path.clone(),
                    version: 3,
                },
            ),
            (
                8,
                PendingLspRequest::ResolveCodeLens {
                    id: 1,
                    path: path.clone(),
                    version: 3,
                },
            ),
        ]);

        let wrote = handle_lsp_request_command(
            LspClientCommand::CodeLenses {
                id: 1,
                path,
                version: 3,
            },
            &mut writer,
            &mut next_request_id,
            &mut pending_requests,
        )
        .await;

        assert!(wrote);
        assert_eq!(next_request_id, 20);
        assert!(matches!(
            pending_requests.get(&7),
            Some(PendingLspRequest::CodeLenses { version: 3, .. })
        ));
        assert!(!pending_requests.contains_key(&8));
        assert!(!pending_requests.contains_key(&20));
        drop(writer);
        let output = child
            .wait_with_output()
            .await
            .expect("stdin echo child exits cleanly");
        let output = String::from_utf8(output.stdout).expect("stdout is utf8");
        assert!(output.contains("\"method\":\"$/cancelRequest\""));
        assert!(output.contains("\"params\":{\"id\":8}"));
        assert!(!output.contains("textDocument/codeLens"));
    }

    #[tokio::test]
    async fn request_dispatch_reuses_exact_pending_workspace_symbols_without_writing() {
        let mut writer = exited_child_stdin().await;
        let mut next_request_id = 20;
        let path = PathBuf::from("src/main.rs");
        let mut pending_requests = PendingLspRequests::from([(
            7,
            PendingLspRequest::WorkspaceSymbols {
                id: 1,
                path: path.clone(),
                query: "needle".to_owned(),
            },
        )]);

        let wrote = handle_lsp_request_command(
            LspClientCommand::WorkspaceSymbols {
                id: 1,
                path,
                query: "needle".to_owned(),
            },
            &mut writer,
            &mut next_request_id,
            &mut pending_requests,
        )
        .await;

        assert!(wrote);
        assert_eq!(next_request_id, 20);
        assert!(matches!(
            pending_requests.get(&7),
            Some(PendingLspRequest::WorkspaceSymbols { query, .. }) if query == "needle"
        ));
        assert!(!pending_requests.contains_key(&20));
    }

    #[tokio::test]
    async fn request_dispatch_reuses_exact_pending_inlay_hints_without_writing() {
        let mut writer = exited_child_stdin().await;
        let mut next_request_id = 20;
        let path = PathBuf::from("src/main.rs");
        let mut pending_requests = PendingLspRequests::from([(
            7,
            PendingLspRequest::InlayHints {
                id: 1,
                path: path.clone(),
                version: 3,
                end_line: 40,
                end_character: 2,
            },
        )]);

        let wrote = handle_lsp_request_command(
            LspClientCommand::InlayHints {
                id: 1,
                path,
                version: 3,
                end_line: 40,
                end_character: 2,
            },
            &mut writer,
            &mut next_request_id,
            &mut pending_requests,
        )
        .await;

        assert!(wrote);
        assert_eq!(next_request_id, 20);
        assert!(matches!(
            pending_requests.get(&7),
            Some(PendingLspRequest::InlayHints {
                version: 3,
                end_line: 40,
                end_character: 2,
                ..
            })
        ));
        assert!(!pending_requests.contains_key(&20));
    }

    #[tokio::test]
    async fn request_dispatch_cancels_stale_code_lenses_and_dispatches_replacement() {
        let (mut child, mut writer) = stdin_sink_child().await;
        let mut next_request_id = 20;
        let path = PathBuf::from("src/main.rs");
        let mut pending_requests = PendingLspRequests::from([
            (
                7,
                PendingLspRequest::CodeLenses {
                    id: 1,
                    path: path.clone(),
                    version: 3,
                },
            ),
            (
                8,
                PendingLspRequest::ResolveCodeLens {
                    id: 1,
                    path: path.clone(),
                    version: 3,
                },
            ),
        ]);

        let wrote = handle_lsp_request_command(
            LspClientCommand::CodeLenses {
                id: 1,
                path,
                version: 4,
            },
            &mut writer,
            &mut next_request_id,
            &mut pending_requests,
        )
        .await;

        assert!(wrote);
        assert_eq!(next_request_id, 21);
        assert!(!pending_requests.contains_key(&7));
        assert!(!pending_requests.contains_key(&8));
        assert!(matches!(
            pending_requests.get(&20),
            Some(PendingLspRequest::CodeLenses { version: 4, .. })
        ));
        drop(writer);
        let _ = child.kill().await;
    }

    #[tokio::test]
    async fn request_dispatch_cancels_stale_inlay_range_and_dispatches_replacement() {
        let (mut child, mut writer) = stdin_sink_child().await;
        let mut next_request_id = 20;
        let path = PathBuf::from("src/main.rs");
        let mut pending_requests = PendingLspRequests::from([(
            7,
            PendingLspRequest::InlayHints {
                id: 1,
                path: path.clone(),
                version: 3,
                end_line: 40,
                end_character: 2,
            },
        )]);

        let wrote = handle_lsp_request_command(
            LspClientCommand::InlayHints {
                id: 1,
                path,
                version: 3,
                end_line: 41,
                end_character: 0,
            },
            &mut writer,
            &mut next_request_id,
            &mut pending_requests,
        )
        .await;

        assert!(wrote);
        assert_eq!(next_request_id, 21);
        assert!(!pending_requests.contains_key(&7));
        assert!(matches!(
            pending_requests.get(&20),
            Some(PendingLspRequest::InlayHints {
                version: 3,
                end_line: 41,
                end_character: 0,
                ..
            })
        ));
        drop(writer);
        let _ = child.kill().await;
    }

    #[tokio::test]
    async fn request_dispatch_cancels_stale_workspace_symbols_and_dispatches_replacement() {
        let (mut child, mut writer) = stdin_sink_child().await;
        let mut next_request_id = 20;
        let path = PathBuf::from("src/main.rs");
        let mut pending_requests = PendingLspRequests::from([(
            7,
            PendingLspRequest::WorkspaceSymbols {
                id: 1,
                path: path.clone(),
                query: "old".to_owned(),
            },
        )]);

        let wrote = handle_lsp_request_command(
            LspClientCommand::WorkspaceSymbols {
                id: 1,
                path,
                query: "new".to_owned(),
            },
            &mut writer,
            &mut next_request_id,
            &mut pending_requests,
        )
        .await;

        assert!(wrote);
        assert_eq!(next_request_id, 21);
        assert!(!pending_requests.contains_key(&7));
        assert!(matches!(
            pending_requests.get(&20),
            Some(PendingLspRequest::WorkspaceSymbols { query, .. }) if query == "new"
        ));
        drop(writer);
        let _ = child.kill().await;
    }

    #[tokio::test]
    async fn cancel_write_failure_stops_without_registering_replacement_request() {
        let mut writer = exited_child_stdin().await;
        let mut next_request_id = 20;
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

        let wrote = handle_lsp_request_command(
            LspClientCommand::Hover {
                id: 1,
                path,
                version: 3,
                line: 4,
                character: 5,
            },
            &mut writer,
            &mut next_request_id,
            &mut pending_requests,
        )
        .await;

        assert!(!wrote);
        assert_eq!(next_request_id, 20);
        assert!(pending_requests.contains_key(&7));
    }

    #[tokio::test]
    async fn code_lens_cleanup_cancel_write_failure_keeps_stale_resolve_pending() {
        let mut writer = exited_child_stdin().await;
        let mut next_request_id = 20;
        let path = PathBuf::from("src/main.rs");
        let mut pending_requests = PendingLspRequests::from([(
            7,
            PendingLspRequest::ResolveCodeLens {
                id: 1,
                path: path.clone(),
                version: 2,
            },
        )]);

        let wrote = handle_lsp_request_command(
            LspClientCommand::CodeLenses {
                id: 1,
                path,
                version: 3,
            },
            &mut writer,
            &mut next_request_id,
            &mut pending_requests,
        )
        .await;

        assert!(!wrote);
        assert_eq!(next_request_id, 20);
        assert!(pending_requests.contains_key(&7));
        assert!(!pending_requests.contains_key(&20));
    }
}
