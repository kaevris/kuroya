use super::super::super::super::reserve_request_id;
use super::pending::{register_code_action_resolve_request, register_code_actions_request};
use crate::lsp_client::pending::PendingLspRequests;
use crate::lsp_client::{
    pending::{
        MAX_LSP_OUTBOUND_JSON_PAYLOAD_BYTES, lsp_json_payload_is_bounded,
        lsp_request_target_is_valid,
    },
    request_dispatch::write_request_message,
};
use kuroya_core::{BufferId, Diagnostic, LspCodeAction, LspWireMessage};
use std::path::PathBuf;
use tokio::process::ChildStdin;

pub(super) async fn dispatch_code_actions(
    id: BufferId,
    path: PathBuf,
    version: u64,
    origin_line: usize,
    origin_character: usize,
    start_line: usize,
    start_character: usize,
    end_line: usize,
    end_character: usize,
    diagnostics: Vec<Diagnostic>,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    if !lsp_request_target_is_valid(id, &path) {
        return true;
    }

    let request_id = reserve_request_id(next_request_id, pending_requests);
    let message = LspWireMessage::code_action_with_diagnostics(
        request_id,
        &path,
        start_line,
        start_character,
        end_line,
        end_character,
        &diagnostics,
    )
    .to_json();
    register_code_actions_request(
        request_id,
        id,
        path,
        version,
        origin_line,
        origin_character,
        pending_requests,
    );
    write_request_message(writer, pending_requests, request_id, message).await
}

pub(super) async fn dispatch_code_action_resolve(
    id: BufferId,
    path: PathBuf,
    version: u64,
    line: usize,
    character: usize,
    action: LspCodeAction,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    if !lsp_request_target_is_valid(id, &path) {
        return true;
    }

    let Some((request_id, message)) =
        reserve_code_action_resolve_message(next_request_id, pending_requests, &action)
    else {
        return true;
    };
    register_code_action_resolve_request(
        request_id,
        id,
        path,
        version,
        line,
        character,
        pending_requests,
    );
    write_request_message(writer, pending_requests, request_id, message).await
}

fn reserve_code_action_resolve_message(
    next_request_id: &mut u64,
    pending_requests: &PendingLspRequests,
    action: &LspCodeAction,
) -> Option<(u64, serde_json::Value)> {
    let resolve_payload = action.resolve_payload.as_ref()?;
    if !lsp_json_payload_is_bounded(
        resolve_payload.as_ref(),
        MAX_LSP_OUTBOUND_JSON_PAYLOAD_BYTES,
    ) {
        return None;
    }

    let mut candidate_next_request_id = *next_request_id;
    let request_id = reserve_request_id(&mut candidate_next_request_id, pending_requests);
    let message = LspWireMessage::code_action_resolve(request_id, action)?.to_json();
    *next_request_id = candidate_next_request_id;
    Some((request_id, message))
}

#[cfg(test)]
mod tests {
    use super::reserve_code_action_resolve_message;
    use crate::lsp_client::pending::PendingLspRequests;
    use crate::lsp_client::pending::{MAX_LSP_OUTBOUND_JSON_PAYLOAD_BYTES, PendingLspRequest};
    use kuroya_core::LspCodeAction;
    use serde_json::json;
    use std::{path::PathBuf, sync::Arc};

    fn hover(version: u64) -> PendingLspRequest {
        PendingLspRequest::Hover {
            id: 1,
            path: PathBuf::from("src/main.rs"),
            version,
            line: 0,
            character: 0,
        }
    }

    #[test]
    fn code_action_resolve_message_does_not_reserve_without_payload() {
        let mut next_request_id = 9;
        let pending_requests = PendingLspRequests::default();
        let action = code_action(None);

        let message =
            reserve_code_action_resolve_message(&mut next_request_id, &pending_requests, &action);

        assert!(message.is_none());
        assert_eq!(next_request_id, 9);
    }

    #[test]
    fn code_action_resolve_message_reserves_after_payload_is_sendable() {
        let mut next_request_id = 9;
        let pending_requests = PendingLspRequests::default();
        let action = code_action(Some(Arc::new(json!({
            "title": "Import HashMap",
            "data": { "id": 7 }
        }))));

        let (request_id, message) =
            reserve_code_action_resolve_message(&mut next_request_id, &pending_requests, &action)
                .expect("resolve payload builds request");

        assert_eq!(request_id, 9);
        assert_eq!(next_request_id, 10);
        assert_eq!(message["id"], 9);
        assert_eq!(message["method"], "codeAction/resolve");
        assert_eq!(message["params"]["data"]["id"], 7);
    }

    #[test]
    fn code_action_resolve_message_skips_active_pending_id() {
        let mut next_request_id = 9;
        let pending_requests = PendingLspRequests::from([(9, hover(1))]);
        let action = code_action(Some(Arc::new(json!({
            "title": "Import HashMap",
            "data": { "id": 7 }
        }))));

        let (request_id, message) =
            reserve_code_action_resolve_message(&mut next_request_id, &pending_requests, &action)
                .expect("resolve payload builds request");

        assert_eq!(request_id, 10);
        assert_eq!(next_request_id, 11);
        assert_eq!(message["id"], 10);
    }

    #[test]
    fn code_action_resolve_message_does_not_reserve_oversized_payload() {
        let mut next_request_id = 9;
        let pending_requests = PendingLspRequests::default();
        let action = code_action(Some(Arc::new(json!({
            "data": "x".repeat(MAX_LSP_OUTBOUND_JSON_PAYLOAD_BYTES)
        }))));

        let message =
            reserve_code_action_resolve_message(&mut next_request_id, &pending_requests, &action);

        assert!(message.is_none());
        assert_eq!(next_request_id, 9);
    }

    fn code_action(resolve_payload: Option<Arc<serde_json::Value>>) -> LspCodeAction {
        LspCodeAction {
            title: "Import HashMap".to_owned(),
            kind: Some("quickfix".to_owned()),
            edits: Vec::new(),
            document_changes: Vec::new(),
            resolve_payload,
        }
    }
}
