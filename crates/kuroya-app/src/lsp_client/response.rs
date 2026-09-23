mod edits;
mod error;
mod navigation;
mod symbols;

use super::pending::{PendingLspRequest, PendingLspRequests};
use crate::lsp_ui_events::{LspServerResultTarget, LspUiEvent};
use crate::ui_event_channel::{
    Sender, send_critical_ui_event, send_critical_ui_event_with_timeout, send_ui_event,
};
use crate::ui_events::UiEvent;
use serde_json::Value;
#[cfg(test)]
use serde_json::json;
use std::{
    cell::RefCell,
    time::{Duration, Instant},
};

pub(super) use error::response_error;

pub(super) const LSP_SERVER_STOPPED_PENDING_REQUEST_ERROR: &str =
    "LSP server stopped before responding";
pub(super) const LSP_REQUEST_TIMED_OUT_ERROR: &str = "request timed out after 20s";
pub(super) const LSP_REQUEST_CANCELLED_ON_STOP_ERROR: &str = "cancelled: language server stopped";
const PENDING_LSP_FAILURE_BATCH_TIMEOUT_MS: u64 = 100;

thread_local! {
    static LSP_RESPONSE_TARGET: RefCell<Option<LspServerResultTarget>> = const { RefCell::new(None) };
}

struct LspResponseTargetGuard {
    previous: Option<LspServerResultTarget>,
}

impl Drop for LspResponseTargetGuard {
    fn drop(&mut self) {
        LSP_RESPONSE_TARGET.with(|target| {
            *target.borrow_mut() = self.previous.take();
        });
    }
}

fn lsp_response_target_guard(target: LspServerResultTarget) -> LspResponseTargetGuard {
    let previous = LSP_RESPONSE_TARGET.with(|current| current.replace(Some(target)));
    LspResponseTargetGuard { previous }
}

fn current_lsp_response_target() -> Option<LspServerResultTarget> {
    LSP_RESPONSE_TARGET.with(|target| target.borrow().clone())
}

fn wrap_lsp_server_result_event(target: LspServerResultTarget, event: LspUiEvent) -> LspUiEvent {
    match event {
        LspUiEvent::ServerResult { .. } => event,
        event => LspUiEvent::ServerResult {
            target,
            event: Box::new(event),
        },
    }
}

fn wrap_lsp_server_result_ui_event(target: LspServerResultTarget, event: UiEvent) -> UiEvent {
    match event {
        UiEvent::Lsp(event) => UiEvent::Lsp(wrap_lsp_server_result_event(target, event)),
        event => event,
    }
}

fn lsp_response_ui_event(event: LspUiEvent) -> UiEvent {
    match current_lsp_response_target() {
        Some(target) => wrap_lsp_server_result_ui_event(target, UiEvent::Lsp(event)),
        None => UiEvent::Lsp(event),
    }
}

pub(in crate::lsp_client::response) fn emit_lsp_response_event(
    ui_tx: &Sender<UiEvent>,
    event: LspUiEvent,
) -> bool {
    send_ui_event(ui_tx, lsp_response_ui_event(event))
}

pub(in crate::lsp_client::response) fn emit_critical_lsp_response_event(
    ui_tx: &Sender<UiEvent>,
    event: LspUiEvent,
) -> bool {
    send_critical_ui_event(ui_tx, lsp_response_ui_event(event))
}

pub(super) fn handle_lsp_response(
    pending: PendingLspRequest,
    value: Value,
    ui_tx: &Sender<UiEvent>,
) {
    match pending {
        pending @ (PendingLspRequest::Hover { .. }
        | PendingLspRequest::DocumentHighlights { .. }
        | PendingLspRequest::Definition { .. }
        | PendingLspRequest::PrepareCallHierarchy { .. }
        | PendingLspRequest::CallHierarchyIncoming { .. }
        | PendingLspRequest::CallHierarchyOutgoing { .. }
        | PendingLspRequest::PrepareTypeHierarchy { .. }
        | PendingLspRequest::TypeHierarchySupertypes { .. }
        | PendingLspRequest::TypeHierarchySubtypes { .. }
        | PendingLspRequest::References { .. }
        | PendingLspRequest::PrepareRename { .. }
        | PendingLspRequest::Rename { .. }) => {
            navigation::handle_navigation_response(pending, value, ui_tx);
        }
        pending @ (PendingLspRequest::DocumentSymbols { .. }
        | PendingLspRequest::FoldingRanges { .. }
        | PendingLspRequest::InlayHints { .. }
        | PendingLspRequest::CodeLenses { .. }
        | PendingLspRequest::ResolveCodeLens { .. }
        | PendingLspRequest::ExecuteCommand { .. }
        | PendingLspRequest::SemanticTokens { .. }
        | PendingLspRequest::WorkspaceSymbols { .. }) => {
            symbols::handle_symbol_response(pending, value, ui_tx);
        }
        pending @ (PendingLspRequest::Completion { .. }
        | PendingLspRequest::ResolveCompletionItem { .. }
        | PendingLspRequest::SignatureHelp { .. }
        | PendingLspRequest::Formatting { .. }
        | PendingLspRequest::CodeActions { .. }
        | PendingLspRequest::ResolveCodeAction { .. }) => {
            edits::handle_edit_response(pending, value, ui_tx);
        }
    }
}

pub(super) fn handle_lsp_response_for_server(
    target: LspServerResultTarget,
    pending: PendingLspRequest,
    value: Value,
    ui_tx: &Sender<UiEvent>,
) {
    let _guard = lsp_response_target_guard(target);
    handle_lsp_response(pending, value, ui_tx);
}

#[cfg(test)]
pub(super) fn emit_pending_lsp_request_failures(
    pending_requests: &mut PendingLspRequests,
    ui_tx: &Sender<UiEvent>,
) -> usize {
    let deadline = Instant::now() + Duration::from_millis(PENDING_LSP_FAILURE_BATCH_TIMEOUT_MS);
    emit_pending_lsp_request_failures_with_deadline(pending_requests, ui_tx, deadline, None)
}

pub(super) fn emit_pending_lsp_request_failures_for_server(
    target: LspServerResultTarget,
    pending_requests: &mut PendingLspRequests,
    ui_tx: &Sender<UiEvent>,
) -> usize {
    let deadline = Instant::now() + Duration::from_millis(PENDING_LSP_FAILURE_BATCH_TIMEOUT_MS);
    emit_pending_lsp_request_failures_with_deadline(
        pending_requests,
        ui_tx,
        deadline,
        Some(&target),
    )
}

fn emit_pending_lsp_request_failures_with_deadline(
    pending_requests: &mut PendingLspRequests,
    ui_tx: &Sender<UiEvent>,
    deadline: Instant,
    target: Option<&LspServerResultTarget>,
) -> usize {
    let mut pending = pending_requests.drain().collect::<Vec<_>>();
    pending.sort_by_key(|(request_id, _)| *request_id);
    emit_drained_lsp_request_failures(
        pending,
        ui_tx,
        deadline,
        target,
        LSP_SERVER_STOPPED_PENDING_REQUEST_ERROR,
    )
}

pub(super) fn emit_pending_lsp_request_cancellations_for_server(
    target: LspServerResultTarget,
    pending_requests: &mut PendingLspRequests,
    ui_tx: &Sender<UiEvent>,
) -> usize {
    let deadline = Instant::now() + Duration::from_millis(PENDING_LSP_FAILURE_BATCH_TIMEOUT_MS);
    let mut pending = pending_requests.drain().collect::<Vec<_>>();
    pending.sort_by_key(|(request_id, _)| *request_id);
    emit_drained_lsp_request_failures(
        pending,
        ui_tx,
        deadline,
        Some(&target),
        LSP_REQUEST_CANCELLED_ON_STOP_ERROR,
    )
}

fn emit_drained_lsp_request_failures(
    pending: Vec<(u64, PendingLspRequest)>,
    ui_tx: &Sender<UiEvent>,
    deadline: Instant,
    target: Option<&LspServerResultTarget>,
    error: &str,
) -> usize {
    let count = pending.len();
    for (_, pending) in pending {
        handle_lsp_failure_response(pending, ui_tx, deadline, target, error);
    }
    count
}

pub(super) fn emit_expired_lsp_request_timeouts(
    target: LspServerResultTarget,
    expired: Vec<(u64, PendingLspRequest)>,
    ui_tx: &Sender<UiEvent>,
) -> usize {
    let deadline = Instant::now() + Duration::from_millis(PENDING_LSP_FAILURE_BATCH_TIMEOUT_MS);
    emit_drained_lsp_request_failures(
        expired,
        ui_tx,
        deadline,
        Some(&target),
        LSP_REQUEST_TIMED_OUT_ERROR,
    )
}

fn handle_lsp_failure_response(
    pending: PendingLspRequest,
    ui_tx: &Sender<UiEvent>,
    deadline: Instant,
    target: Option<&LspServerResultTarget>,
    error: &str,
) {
    let timeout = deadline.saturating_duration_since(Instant::now());
    let event = pending_lsp_failure_event(pending, error);
    let event = match target {
        Some(target) => wrap_lsp_server_result_ui_event(target.clone(), event),
        None => event,
    };
    let _ = send_critical_ui_event_with_timeout(ui_tx, event, timeout);
}

fn pending_lsp_failure_event(pending: PendingLspRequest, error: &str) -> UiEvent {
    let error = Some(error.to_owned());
    UiEvent::Lsp(match pending {
        PendingLspRequest::Hover {
            id,
            path,
            version,
            line,
            character,
        } => LspUiEvent::HoverResult {
            id,
            path,
            version,
            line,
            column: lsp_failure_one_based_column(character),
            contents: error,
        },
        PendingLspRequest::DocumentHighlights {
            id,
            path,
            version,
            line,
            character,
        } => LspUiEvent::DocumentHighlightsResult {
            id,
            path,
            version,
            line,
            column: lsp_failure_one_based_column(character),
            highlights: None,
            error,
        },
        PendingLspRequest::Definition {
            id,
            path,
            version,
            line,
            character,
        } => LspUiEvent::DefinitionResult {
            id,
            origin_path: path,
            version,
            origin_line: line,
            origin_column: lsp_failure_one_based_column(character),
            definition: None,
            error,
        },
        PendingLspRequest::PrepareCallHierarchy {
            id,
            path,
            version,
            line,
            character,
        } => LspUiEvent::CallHierarchyPrepared {
            id,
            path,
            version,
            line,
            column: character,
            items: None,
            error,
        },
        PendingLspRequest::CallHierarchyIncoming {
            id,
            path,
            version,
            item,
        } => LspUiEvent::CallHierarchyIncomingResult {
            id,
            path,
            version,
            item,
            calls: None,
            error,
        },
        PendingLspRequest::CallHierarchyOutgoing {
            id,
            path,
            version,
            item,
        } => LspUiEvent::CallHierarchyOutgoingResult {
            id,
            path,
            version,
            item,
            calls: None,
            error,
        },
        PendingLspRequest::PrepareTypeHierarchy {
            id,
            path,
            version,
            line,
            character,
        } => LspUiEvent::TypeHierarchyPrepared {
            id,
            path,
            version,
            line,
            column: character,
            items: None,
            error,
        },
        PendingLspRequest::TypeHierarchySupertypes {
            id,
            path,
            version,
            item,
        } => LspUiEvent::TypeHierarchySupertypesResult {
            id,
            path,
            version,
            item,
            supertypes: None,
            error,
        },
        PendingLspRequest::TypeHierarchySubtypes {
            id,
            path,
            version,
            item,
        } => LspUiEvent::TypeHierarchySubtypesResult {
            id,
            path,
            version,
            item,
            subtypes: None,
            error,
        },
        PendingLspRequest::References {
            id,
            path,
            version,
            line,
            character,
        } => LspUiEvent::ReferencesResult {
            id,
            path,
            version,
            line,
            column: lsp_failure_one_based_column(character),
            references: None,
            error,
        },
        PendingLspRequest::PrepareRename {
            id,
            path,
            version,
            line,
            character,
        } => LspUiEvent::PrepareRenameResult {
            id,
            path,
            version,
            line,

            column: character,
            range: None,
            error,
        },
        PendingLspRequest::Rename {
            id,
            path,
            version,
            line,
            character,
            new_name,
        } => LspUiEvent::RenameResult {
            id,
            origin_path: path,
            version,
            origin_line: line,
            origin_column: lsp_failure_one_based_column(character),
            new_name,
            edits: None,
            error,
        },
        PendingLspRequest::DocumentSymbols { id, path, version } => {
            LspUiEvent::DocumentSymbolsResult {
                id,
                path,
                version,
                symbols: None,
                error,
            }
        }
        PendingLspRequest::FoldingRanges { id, path, version } => LspUiEvent::FoldingRangesResult {
            id,
            path,
            version,
            ranges: None,
            error,
        },
        PendingLspRequest::InlayHints {
            id, path, version, ..
        } => LspUiEvent::InlayHintsResult {
            id,
            path,
            version,
            hints: None,
            error,
        },
        PendingLspRequest::CodeLenses { id, path, version } => LspUiEvent::CodeLensesResult {
            id,
            path,
            version,
            lenses: None,
            error,
        },
        PendingLspRequest::ResolveCodeLens { id, path, version } => {
            LspUiEvent::CodeLensResolveResult {
                id,
                path,
                version,
                lens: None,
                error,
            }
        }
        PendingLspRequest::ExecuteCommand {
            id,
            path,
            version,
            title,
            command,
        } => LspUiEvent::CodeLensCommandResult {
            id,
            path,
            version,
            title,
            command,
            error,
        },
        PendingLspRequest::SemanticTokens { id, path, version } => {
            LspUiEvent::SemanticTokensResult {
                id,
                path,
                version,
                tokens: None,
                error,
            }
        }
        PendingLspRequest::WorkspaceSymbols { id, path, query } => {
            LspUiEvent::WorkspaceSymbolsResult {
                id,
                path,
                query,
                symbols: None,
                error,
            }
        }
        PendingLspRequest::Completion {
            id,
            path,
            version,
            line,
            character,
        } => LspUiEvent::CompletionResult {
            id,
            path,
            version,
            line,
            column: lsp_failure_one_based_column(character),
            items: None,
            error,
        },
        PendingLspRequest::ResolveCompletionItem {
            id,
            path,
            version,
            line,
            character,
            item,
            intent,
        } => LspUiEvent::CompletionItemResolveResult {
            id,
            path,
            version,
            line,
            column: lsp_failure_one_based_column(character),
            item: None,
            fallback_item: item,
            intent,
            error,
        },
        PendingLspRequest::SignatureHelp {
            id,
            path,
            version,
            line,
            character,
        } => LspUiEvent::SignatureHelpResult {
            id,
            path,
            version,
            line,
            column: lsp_failure_one_based_column(character),
            help: None,
            error,
        },
        PendingLspRequest::Formatting {
            request_id,
            id,
            path,
            version,
        } => LspUiEvent::FormattingResult {
            request_id,
            id,
            path,
            version,
            edits: None,
            error,
        },
        PendingLspRequest::CodeActions {
            id,
            path,
            version,
            line,
            character,
        } => LspUiEvent::CodeActionsResult {
            id,
            path,
            version,
            line,
            column: lsp_failure_one_based_column(character),
            actions: None,
            error,
        },
        PendingLspRequest::ResolveCodeAction {
            id,
            path,
            version,
            line,
            character,
        } => LspUiEvent::CodeActionResolveResult {
            id,
            path,
            version,
            line,
            column: lsp_failure_one_based_column(character),
            action: None,
            error,
        },
    })
}

fn lsp_failure_one_based_column(character: usize) -> usize {
    character.saturating_add(1)
}

#[cfg(test)]
fn pending_request_failure_response(request_id: u64) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "error": {
            "code": -32000,
            "message": LSP_SERVER_STOPPED_PENDING_REQUEST_ERROR,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{
        LSP_REQUEST_CANCELLED_ON_STOP_ERROR, LSP_SERVER_STOPPED_PENDING_REQUEST_ERROR,
        emit_pending_lsp_request_cancellations_for_server, emit_pending_lsp_request_failures,
        emit_pending_lsp_request_failures_for_server,
        emit_pending_lsp_request_failures_with_deadline, handle_lsp_response,
        pending_request_failure_response, response_error,
    };
    use crate::{
        lsp_client::pending::{PendingLspRequest, PendingLspRequests},
        lsp_completion_resolve::CompletionResolveIntent,
        lsp_ui_events::{LspServerResultTarget, LspUiEvent},
        ui_event_channel::UI_EVENT_CHANNEL_BOUND,
        ui_events::UiEvent,
    };
    use kuroya_core::LspCompletionItem;
    use serde_json::{Value, json};
    use std::{
        path::PathBuf,
        thread,
        time::{Duration, Instant},
    };

    #[test]
    fn pending_request_failure_response_uses_typed_response_error_shape() {
        let response = pending_request_failure_response(42);

        assert_eq!(response["id"], 42);
        assert_eq!(
            response_error(&response).as_deref(),
            Some(LSP_SERVER_STOPPED_PENDING_REQUEST_ERROR)
        );
    }

    #[test]
    fn pending_lsp_request_failures_emit_completion_errors_and_drain() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let mut pending_requests = PendingLspRequests::from([(
            11,
            PendingLspRequest::Completion {
                id: 7,
                path: PathBuf::from("src/main.rs"),
                version: 3,
                line: 2,
                character: 4,
            },
        )]);

        assert_eq!(
            emit_pending_lsp_request_failures(&mut pending_requests, &tx),
            1
        );

        assert!(pending_requests.is_empty());
        match rx.recv().expect("completion result event") {
            UiEvent::Lsp(LspUiEvent::CompletionResult {
                id,
                path,
                version,
                line,
                column,
                items,
                error,
            }) => {
                assert_eq!(id, 7);
                assert_eq!(path, PathBuf::from("src/main.rs"));
                assert_eq!(version, 3);
                assert_eq!(line, 2);
                assert_eq!(column, 5);
                assert_eq!(items, None);
                assert_eq!(
                    error.as_deref(),
                    Some(LSP_SERVER_STOPPED_PENDING_REQUEST_ERROR)
                );
            }
            other => panic!("expected completion result event, got {other:?}"),
        }
    }

    #[test]
    fn pending_lsp_request_cancellations_use_distinct_stopped_wording_and_drain() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let target = LspServerResultTarget {
            language: "rust".to_owned(),
            root: PathBuf::from("workspace"),
            generation: 9,
        };
        let mut pending_requests = PendingLspRequests::from([(
            11,
            PendingLspRequest::Completion {
                id: 7,
                path: PathBuf::from("src/main.rs"),
                version: 3,
                line: 2,
                character: 4,
            },
        )]);

        assert_eq!(
            emit_pending_lsp_request_cancellations_for_server(
                target.clone(),
                &mut pending_requests,
                &tx
            ),
            1
        );

        assert!(pending_requests.is_empty());
        match rx.recv().expect("completion cancellation event") {
            UiEvent::Lsp(LspUiEvent::ServerResult {
                target: event_target,
                event,
            }) => {
                assert_eq!(event_target, target);
                match *event {
                    LspUiEvent::CompletionResult { id, error, .. } => {
                        assert_eq!(id, 7);
                        assert_eq!(error.as_deref(), Some(LSP_REQUEST_CANCELLED_ON_STOP_ERROR));
                    }
                    other => panic!("expected completion result, got {other:?}"),
                }
            }
            other => panic!("expected wrapped cancellation, got {other:?}"),
        }
    }

    #[test]
    fn pending_lsp_request_failures_emit_symbol_and_formatting_errors_in_order() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let mut pending_requests = PendingLspRequests::from([
            (
                12,
                PendingLspRequest::Formatting {
                    request_id: 12,
                    id: 3,
                    path: PathBuf::from("src/lib.rs"),
                    version: 8,
                },
            ),
            (
                10,
                PendingLspRequest::WorkspaceSymbols {
                    id: 2,
                    path: PathBuf::from("src/main.rs"),
                    query: "main".to_owned(),
                },
            ),
        ]);

        assert_eq!(
            emit_pending_lsp_request_failures(&mut pending_requests, &tx),
            2
        );

        assert!(pending_requests.is_empty());
        match rx.recv().expect("workspace symbols result event") {
            UiEvent::Lsp(LspUiEvent::WorkspaceSymbolsResult {
                id,
                path,
                query,
                symbols,
                error,
            }) => {
                assert_eq!(id, 2);
                assert_eq!(path, PathBuf::from("src/main.rs"));
                assert_eq!(query, "main");
                assert_eq!(symbols, None);
                assert_eq!(
                    error.as_deref(),
                    Some(LSP_SERVER_STOPPED_PENDING_REQUEST_ERROR)
                );
            }
            other => panic!("expected workspace symbols result event, got {other:?}"),
        }
        match rx.recv().expect("formatting result event") {
            UiEvent::Lsp(LspUiEvent::FormattingResult {
                request_id,
                id,
                path,
                version,
                edits,
                error,
            }) => {
                assert_eq!(request_id, 12);
                assert_eq!(id, 3);
                assert_eq!(path, PathBuf::from("src/lib.rs"));
                assert_eq!(version, 8);
                assert_eq!(edits, None);
                assert_eq!(
                    error.as_deref(),
                    Some(LSP_SERVER_STOPPED_PENDING_REQUEST_ERROR)
                );
            }
            other => panic!("expected formatting result event, got {other:?}"),
        }
    }

    #[test]
    fn pending_lsp_request_failures_for_server_wrap_result_identity() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let target = LspServerResultTarget {
            language: "rust".to_owned(),
            root: PathBuf::from("workspace"),
            generation: 7,
        };
        let mut pending_requests = PendingLspRequests::from([(
            12,
            PendingLspRequest::Formatting {
                request_id: 12,
                id: 3,
                path: PathBuf::from("src/lib.rs"),
                version: 8,
            },
        )]);

        assert_eq!(
            emit_pending_lsp_request_failures_for_server(
                target.clone(),
                &mut pending_requests,
                &tx,
            ),
            1
        );

        assert!(pending_requests.is_empty());
        match rx.recv().expect("wrapped formatting failure event") {
            UiEvent::Lsp(LspUiEvent::ServerResult {
                target: actual_target,
                event,
            }) => {
                assert_eq!(actual_target, target);
                match *event {
                    LspUiEvent::FormattingResult {
                        request_id,
                        id,
                        path,
                        version,
                        edits,
                        error,
                    } => {
                        assert_eq!(request_id, 12);
                        assert_eq!(id, 3);
                        assert_eq!(path, PathBuf::from("src/lib.rs"));
                        assert_eq!(version, 8);
                        assert_eq!(edits, None);
                        assert_eq!(
                            error.as_deref(),
                            Some(LSP_SERVER_STOPPED_PENDING_REQUEST_ERROR)
                        );
                    }
                    other => panic!("expected formatting result, got {other:?}"),
                }
            }
            other => panic!("expected wrapped server result, got {other:?}"),
        }
    }

    #[test]
    fn pending_lsp_request_failures_deliver_formatting_error_under_ui_backpressure() {
        assert_pending_lsp_failure_delivered_under_backpressure(
            PendingLspRequest::Formatting {
                request_id: 12,
                id: 3,
                path: PathBuf::from("src/lib.rs"),
                version: 8,
            },
            |event| {
                let UiEvent::Lsp(LspUiEvent::FormattingResult {
                    request_id,
                    id,
                    path,
                    version,
                    edits,
                    error,
                }) = event
                else {
                    return false;
                };
                assert_eq!(*id, 3);
                assert_eq!(*request_id, 12);
                assert_eq!(path, &PathBuf::from("src/lib.rs"));
                assert_eq!(*version, 8);
                assert_eq!(edits, &None);
                assert_eq!(
                    error.as_deref(),
                    Some(LSP_SERVER_STOPPED_PENDING_REQUEST_ERROR)
                );
                true
            },
        );
    }

    #[test]
    fn pending_lsp_request_failures_deliver_completion_error_under_ui_backpressure() {
        assert_pending_lsp_failure_delivered_under_backpressure(
            PendingLspRequest::Completion {
                id: 7,
                path: PathBuf::from("src/main.rs"),
                version: 3,
                line: 2,
                character: 4,
            },
            |event| {
                let UiEvent::Lsp(LspUiEvent::CompletionResult {
                    id,
                    path,
                    version,
                    line,
                    column,
                    items,
                    error,
                }) = event
                else {
                    return false;
                };
                assert_eq!(*id, 7);
                assert_eq!(path, &PathBuf::from("src/main.rs"));
                assert_eq!(*version, 3);
                assert_eq!(*line, 2);
                assert_eq!(*column, 5);
                assert_eq!(items, &None);
                assert_eq!(
                    error.as_deref(),
                    Some(LSP_SERVER_STOPPED_PENDING_REQUEST_ERROR)
                );
                true
            },
        );
    }

    #[test]
    fn pending_lsp_request_failures_deliver_workspace_symbols_error_under_ui_backpressure() {
        assert_pending_lsp_failure_delivered_under_backpressure(
            PendingLspRequest::WorkspaceSymbols {
                id: 2,
                path: PathBuf::from("src/main.rs"),
                query: "main".to_owned(),
            },
            |event| {
                let UiEvent::Lsp(LspUiEvent::WorkspaceSymbolsResult {
                    id,
                    path,
                    query,
                    symbols,
                    error,
                }) = event
                else {
                    return false;
                };
                assert_eq!(*id, 2);
                assert_eq!(path, &PathBuf::from("src/main.rs"));
                assert_eq!(query, "main");
                assert_eq!(symbols, &None);
                assert_eq!(
                    error.as_deref(),
                    Some(LSP_SERVER_STOPPED_PENDING_REQUEST_ERROR)
                );
                true
            },
        );
    }

    #[test]
    fn pending_lsp_request_failures_deliver_references_error_under_ui_backpressure() {
        assert_pending_lsp_failure_delivered_under_backpressure(
            PendingLspRequest::References {
                id: 13,
                path: PathBuf::from("src/main.rs"),
                version: 9,
                line: 6,
                character: 2,
            },
            |event| {
                let UiEvent::Lsp(LspUiEvent::ReferencesResult {
                    id,
                    path,
                    version,
                    line,
                    column,
                    references,
                    error,
                }) = event
                else {
                    return false;
                };
                assert_eq!(*id, 13);
                assert_eq!(path, &PathBuf::from("src/main.rs"));
                assert_eq!(*version, 9);
                assert_eq!(*line, 6);
                assert_eq!(*column, 3);
                assert_eq!(references, &None);
                assert_eq!(
                    error.as_deref(),
                    Some(LSP_SERVER_STOPPED_PENDING_REQUEST_ERROR)
                );
                true
            },
        );
    }

    #[test]
    fn pending_lsp_request_failures_saturate_one_based_columns() {
        assert_pending_lsp_failure_event(
            PendingLspRequest::Completion {
                id: 7,
                path: PathBuf::from("src/main.rs"),
                version: 3,
                line: 2,
                character: usize::MAX,
            },
            |event| {
                matches!(
                    event,
                    UiEvent::Lsp(LspUiEvent::CompletionResult {
                        id: 7,
                        version: 3,
                        line: 2,
                        column: usize::MAX,
                        items: None,
                        ..
                    })
                )
            },
        );
    }

    #[test]
    fn interactive_lsp_response_results_use_critical_delivery_under_ui_backpressure() {
        assert_lsp_response_delivered_under_backpressure(
            PendingLspRequest::Formatting {
                request_id: 12,
                id: 3,
                path: PathBuf::from("src/lib.rs"),
                version: 8,
            },
            json!({ "result": null }),
            |event| {
                matches!(
                    event,
                    UiEvent::Lsp(LspUiEvent::FormattingResult {
                        request_id: 12,
                        id: 3,
                        path,
                        version: 8,
                        edits: Some(edits),
                        error: None,
                    }) if path == &PathBuf::from("src/lib.rs") && edits.is_empty()
                )
            },
        );
        assert_lsp_response_delivered_under_backpressure(
            PendingLspRequest::Definition {
                id: 7,
                path: PathBuf::from("src/main.rs"),
                version: 3,
                line: 2,
                character: 4,
            },
            json!({ "result": null }),
            |event| {
                matches!(
                    event,
                    UiEvent::Lsp(LspUiEvent::DefinitionResult {
                        id: 7,
                        origin_path,
                        version: 3,
                        origin_line: 2,
                        origin_column: 5,
                        definition: None,
                        error: None,
                    }) if origin_path == &PathBuf::from("src/main.rs")
                )
            },
        );
        assert_lsp_response_delivered_under_backpressure(
            PendingLspRequest::WorkspaceSymbols {
                id: 11,
                path: PathBuf::from("src/main.rs"),
                query: "main".to_owned(),
            },
            json!({ "result": null }),
            |event| {
                matches!(
                    event,
                    UiEvent::Lsp(LspUiEvent::WorkspaceSymbolsResult {
                        id: 11,
                        path,
                        query,
                        symbols: Some(symbols),
                        error: None,
                    }) if path == &PathBuf::from("src/main.rs")
                        && query == "main"
                        && symbols.is_empty()
                )
            },
        );
        assert_lsp_response_delivered_under_backpressure(
            PendingLspRequest::ResolveCompletionItem {
                id: 13,
                path: PathBuf::from("src/main.rs"),
                version: 5,
                line: 1,
                character: 8,
                item: Box::new(completion_item()),
                intent: CompletionResolveIntent::Apply { commit_text: None },
            },
            json!({ "result": null }),
            |event| {
                matches!(
                    event,
                    UiEvent::Lsp(LspUiEvent::CompletionItemResolveResult {
                        id: 13,
                        path,
                        version: 5,
                        line: 1,
                        column: 9,
                        item: None,
                        fallback_item,
                        intent: CompletionResolveIntent::Apply { commit_text: None },
                        error: Some(_),
                    }) if path == &PathBuf::from("src/main.rs")
                        && fallback_item.label == "HashMap"
                )
            },
        );
    }

    #[test]
    fn pending_lsp_request_failures_preserve_hierarchy_prepare_coordinate_conventions() {
        assert_pending_lsp_failure_event(
            PendingLspRequest::PrepareCallHierarchy {
                id: 17,
                path: PathBuf::from("src/main.rs"),
                version: 4,
                line: 8,
                character: 3,
            },
            |event| {
                matches!(
                    event,
                    UiEvent::Lsp(LspUiEvent::CallHierarchyPrepared {
                        id: 17,
                        version: 4,
                        line: 8,
                        column: 3,
                        items: None,
                        ..
                    })
                )
            },
        );
        assert_pending_lsp_failure_event(
            PendingLspRequest::PrepareTypeHierarchy {
                id: 18,
                path: PathBuf::from("src/lib.rs"),
                version: 5,
                line: 9,
                character: 4,
            },
            |event| {
                matches!(
                    event,
                    UiEvent::Lsp(LspUiEvent::TypeHierarchyPrepared {
                        id: 18,
                        version: 5,
                        line: 9,
                        column: 4,
                        items: None,
                        ..
                    })
                )
            },
        );
    }

    #[test]
    fn pending_lsp_request_failures_share_one_batch_backpressure_deadline() {
        let (tx, _rx) = crate::ui_event_channel::ui_event_channel();
        for index in 0..UI_EVENT_CHANNEL_BOUND {
            tx.try_send(UiEvent::Lsp(LspUiEvent::WorkDoneProgressCreated {
                token: format!("progress-token-{index}"),
            }))
            .expect("event should fit within channel bound");
        }

        let mut pending_requests = PendingLspRequests::default();
        for request_id in 0..32 {
            pending_requests.insert(
                request_id,
                PendingLspRequest::Formatting {
                    request_id,
                    id: request_id,
                    path: PathBuf::from("src/lib.rs"),
                    version: request_id,
                },
            );
        }

        let started = Instant::now();
        assert_eq!(
            emit_pending_lsp_request_failures_with_deadline(
                &mut pending_requests,
                &tx,
                Instant::now(),
                None,
            ),
            32
        );
        assert!(pending_requests.is_empty());
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "expired batch deadline should not wait once per pending request"
        );
    }

    fn assert_pending_lsp_failure_event(
        pending: PendingLspRequest,
        is_expected: impl Fn(UiEvent) -> bool,
    ) {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let mut pending_requests = PendingLspRequests::from([(10, pending)]);

        assert_eq!(
            emit_pending_lsp_request_failures(&mut pending_requests, &tx),
            1
        );
        let event = rx
            .recv_timeout(Duration::from_secs(1))
            .expect("pending failure should be emitted");
        assert!(is_expected(event));
    }

    fn assert_pending_lsp_failure_delivered_under_backpressure(
        pending: PendingLspRequest,
        is_expected: impl Fn(&UiEvent) -> bool,
    ) {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        for index in 0..UI_EVENT_CHANNEL_BOUND {
            tx.try_send(UiEvent::Lsp(LspUiEvent::WorkDoneProgressCreated {
                token: format!("progress-token-{index}"),
            }))
            .expect("event should fit within channel bound");
        }

        let failure_tx = tx.clone();
        let sender = thread::spawn(move || {
            let mut pending_requests = PendingLspRequests::from([(10, pending)]);
            emit_pending_lsp_request_failures(&mut pending_requests, &failure_tx)
        });

        let _ = rx
            .recv_timeout(Duration::from_secs(1))
            .expect("freeing capacity should unblock pending failure sender");
        assert_eq!(sender.join().unwrap(), 1);

        let mut delivered = false;
        for _ in 0..UI_EVENT_CHANNEL_BOUND {
            let event = rx
                .recv_timeout(Duration::from_secs(1))
                .expect("pending failure should be queued after capacity is freed");
            if is_expected(&event) {
                delivered = true;
                break;
            }
        }
        assert!(delivered);
    }

    fn assert_lsp_response_delivered_under_backpressure(
        pending: PendingLspRequest,
        value: Value,
        is_expected: impl Fn(&UiEvent) -> bool,
    ) {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        for index in 0..UI_EVENT_CHANNEL_BOUND {
            tx.try_send(UiEvent::Lsp(LspUiEvent::WorkDoneProgressCreated {
                token: format!("progress-token-{index}"),
            }))
            .expect("event should fit within channel bound");
        }

        let response_tx = tx.clone();
        let sender = thread::spawn(move || handle_lsp_response(pending, value, &response_tx));

        let _ = rx
            .recv_timeout(Duration::from_secs(1))
            .expect("freeing capacity should unblock LSP response sender");
        sender.join().unwrap();

        let mut delivered = false;
        for _ in 0..UI_EVENT_CHANNEL_BOUND {
            let event = rx
                .recv_timeout(Duration::from_secs(1))
                .expect("LSP response should be queued after capacity is freed");
            if is_expected(&event) {
                delivered = true;
                break;
            }
        }
        assert!(delivered);
    }

    fn completion_item() -> LspCompletionItem {
        LspCompletionItem {
            label: "HashMap".to_owned(),
            detail: None,
            documentation: None,
            kind: None,
            deprecated: false,
            is_snippet: false,
            sort_text: None,
            filter_text: None,
            preselect: false,
            commit_characters: Vec::new(),
            insert_text: "HashMap".to_owned(),
            snippet_selection: None,
            snippet_tabstops: Vec::new(),
            snippet_tabstop_groups: Vec::new(),
            text_edit: None,
            insert_text_edit: None,
            additional_text_edits: Vec::new(),
            resolve_payload: None,
        }
    }
}
