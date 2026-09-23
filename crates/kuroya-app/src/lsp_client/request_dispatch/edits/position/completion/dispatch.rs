use super::super::super::super::reserve_request_id;
use super::pending::{register_completion_item_resolve_request, register_completion_request};
use crate::lsp_client::pending::PendingLspRequests;
use crate::{
    lsp_client::{
        pending::{
            MAX_LSP_OUTBOUND_JSON_PAYLOAD_BYTES, lsp_json_payload_is_bounded,
            lsp_request_target_is_valid,
        },
        request_dispatch::write_request_message,
    },
    lsp_completion_resolve::CompletionResolveIntent,
};
use kuroya_core::{BufferId, LspCompletionItem, LspWireMessage};
use std::path::PathBuf;
use tokio::process::ChildStdin;

pub(super) async fn dispatch_completion(
    id: BufferId,
    path: PathBuf,
    version: u64,
    line: usize,
    character: usize,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    if !lsp_request_target_is_valid(id, &path) {
        return true;
    }

    let request_id = reserve_request_id(next_request_id, pending_requests);
    let message = LspWireMessage::completion(request_id, &path, line, character).to_json();
    register_completion_request(
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

pub(super) async fn dispatch_completion_item_resolve(
    id: BufferId,
    path: PathBuf,
    version: u64,
    line: usize,
    character: usize,
    item: Box<LspCompletionItem>,
    intent: CompletionResolveIntent,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    if !lsp_request_target_is_valid(id, &path) {
        return true;
    }

    let Some((request_id, message)) =
        reserve_completion_item_resolve_message(next_request_id, pending_requests, &item)
    else {
        return false;
    };
    register_completion_item_resolve_request(
        request_id,
        id,
        path,
        version,
        line,
        character,
        item,
        intent,
        pending_requests,
    );
    write_request_message(writer, pending_requests, request_id, message).await
}

fn reserve_completion_item_resolve_message(
    next_request_id: &mut u64,
    pending_requests: &PendingLspRequests,
    item: &LspCompletionItem,
) -> Option<(u64, serde_json::Value)> {
    let resolve_payload = item.resolve_payload.as_ref()?;
    if !lsp_json_payload_is_bounded(
        resolve_payload.as_ref(),
        MAX_LSP_OUTBOUND_JSON_PAYLOAD_BYTES,
    ) {
        return None;
    }

    let request_id = reserve_request_id(next_request_id, pending_requests);
    let message = serde_json::json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "completionItem/resolve",
        "params": resolve_payload.as_ref(),
    });
    Some((request_id, message))
}

#[cfg(test)]
mod tests {
    use super::reserve_completion_item_resolve_message;
    use crate::lsp_client::pending::PendingLspRequests;
    use crate::lsp_client::pending::{MAX_LSP_OUTBOUND_JSON_PAYLOAD_BYTES, PendingLspRequest};
    use kuroya_core::LspCompletionItem;
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
    fn completion_item_resolve_message_does_not_reserve_without_payload() {
        let mut next_request_id = 9;
        let pending_requests = PendingLspRequests::default();
        let item = completion_item(None);

        let message =
            reserve_completion_item_resolve_message(&mut next_request_id, &pending_requests, &item);

        assert!(message.is_none());
        assert_eq!(next_request_id, 9);
    }

    #[test]
    fn completion_item_resolve_message_reserves_after_payload_exists() {
        let mut next_request_id = 9;
        let pending_requests = PendingLspRequests::default();
        let item = completion_item(Some(Arc::new(json!({
            "label": "HashMap",
            "data": { "id": 7 }
        }))));

        let (request_id, message) =
            reserve_completion_item_resolve_message(&mut next_request_id, &pending_requests, &item)
                .expect("resolve payload builds request");

        assert_eq!(request_id, 9);
        assert_eq!(next_request_id, 10);
        assert_eq!(message["id"], 9);
        assert_eq!(message["method"], "completionItem/resolve");
        assert_eq!(message["params"]["data"]["id"], 7);
    }

    #[test]
    fn completion_item_resolve_message_skips_active_pending_id() {
        let mut next_request_id = 9;
        let pending_requests = PendingLspRequests::from([(9, hover(1))]);
        let item = completion_item(Some(Arc::new(json!({
            "label": "HashMap",
            "data": { "id": 7 }
        }))));

        let (request_id, message) =
            reserve_completion_item_resolve_message(&mut next_request_id, &pending_requests, &item)
                .expect("resolve payload builds request");

        assert_eq!(request_id, 10);
        assert_eq!(next_request_id, 11);
        assert_eq!(message["id"], 10);
    }

    #[test]
    fn completion_item_resolve_message_does_not_reserve_oversized_payload() {
        let mut next_request_id = 9;
        let pending_requests = PendingLspRequests::default();
        let item = completion_item(Some(Arc::new(json!({
            "data": "x".repeat(MAX_LSP_OUTBOUND_JSON_PAYLOAD_BYTES)
        }))));

        let message =
            reserve_completion_item_resolve_message(&mut next_request_id, &pending_requests, &item);

        assert!(message.is_none());
        assert_eq!(next_request_id, 9);
    }

    fn completion_item(resolve_payload: Option<Arc<serde_json::Value>>) -> LspCompletionItem {
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
            resolve_payload,
        }
    }
}
