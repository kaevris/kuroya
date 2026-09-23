use super::super::super::reserve_request_id;
use super::pending::register_workspace_symbols_request;
use crate::lsp_client::pending::PendingLspRequests;
use crate::lsp_client::{
    pending::{
        MAX_LSP_OUTBOUND_TEXT_PAYLOAD_CHARS, bounded_lsp_outbound_text, lsp_request_target_is_valid,
    },
    request_dispatch::write_request_message,
};
use kuroya_core::{BufferId, LspWireMessage};
use std::path::PathBuf;
use tokio::process::ChildStdin;

pub(super) async fn dispatch_workspace_symbols(
    id: BufferId,
    path: PathBuf,
    query: String,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    if !lsp_request_target_is_valid(id, &path) {
        return true;
    }
    let Some(query) = dispatch_workspace_symbols_query(query) else {
        return true;
    };

    let request_id = reserve_request_id(next_request_id, pending_requests);
    let message = LspWireMessage::workspace_symbols(request_id, &query).to_json();
    register_workspace_symbols_request(request_id, id, path, query, pending_requests);
    write_request_message(writer, pending_requests, request_id, message).await
}

fn dispatch_workspace_symbols_query(query: String) -> Option<String> {
    bounded_lsp_outbound_text(query, MAX_LSP_OUTBOUND_TEXT_PAYLOAD_CHARS)
}

#[cfg(test)]
mod tests {
    use super::dispatch_workspace_symbols_query;
    use crate::lsp_client::pending::MAX_LSP_OUTBOUND_TEXT_PAYLOAD_CHARS;

    #[test]
    fn workspace_symbols_query_keeps_bounded_text() {
        let query = "symbol".to_owned();

        assert_eq!(dispatch_workspace_symbols_query(query.clone()), Some(query));
    }

    #[test]
    fn workspace_symbols_query_rejects_oversized_text() {
        let query = "x".repeat(MAX_LSP_OUTBOUND_TEXT_PAYLOAD_CHARS + 1);

        assert!(dispatch_workspace_symbols_query(query).is_none());
    }
}
