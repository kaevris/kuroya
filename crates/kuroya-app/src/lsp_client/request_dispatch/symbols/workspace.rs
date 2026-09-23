mod dispatch;
mod pending;

use crate::lsp_client::pending::PendingLspRequests;
use dispatch::dispatch_workspace_symbols;
use kuroya_core::BufferId;
use std::path::PathBuf;
use tokio::process::ChildStdin;

pub(super) async fn dispatch_workspace_symbols_request(
    id: BufferId,
    path: PathBuf,
    query: String,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    dispatch_workspace_symbols(id, path, query, writer, next_request_id, pending_requests).await
}
