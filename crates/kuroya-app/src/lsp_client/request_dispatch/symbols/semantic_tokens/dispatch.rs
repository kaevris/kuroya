use super::super::super::reserve_request_id;
use super::pending::register_semantic_tokens_request;
use crate::lsp_client::pending::PendingLspRequests;
use crate::lsp_client::{
    pending::lsp_request_target_is_valid, request_dispatch::write_request_message,
};
use kuroya_core::{BufferId, LspWireMessage};
use std::path::PathBuf;
use tokio::process::ChildStdin;

pub(super) async fn dispatch_semantic_tokens(
    id: BufferId,
    path: PathBuf,
    version: u64,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    if !lsp_request_target_is_valid(id, &path) {
        return true;
    }

    let request_id = reserve_request_id(next_request_id, pending_requests);
    let message = LspWireMessage::semantic_tokens(request_id, &path).to_json();
    register_semantic_tokens_request(request_id, id, path, version, pending_requests);
    write_request_message(writer, pending_requests, request_id, message).await
}
