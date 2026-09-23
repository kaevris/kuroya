use super::pending::register_hover_request;
use crate::lsp_client::pending::PendingLspRequests;
use crate::lsp_client::{
    pending::lsp_request_target_is_valid, request_dispatch::write_request_message,
};
use kuroya_core::BufferId;
use std::path::PathBuf;
use tokio::process::ChildStdin;

pub(super) async fn dispatch_hover(
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

    let (request_id, message) = register_hover_request(
        id,
        path,
        version,
        line,
        character,
        next_request_id,
        pending_requests,
    );
    write_request_message(writer, pending_requests, request_id, message).await
}
