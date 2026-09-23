mod dispatch;
mod pending;

use crate::lsp_client::pending::PendingLspRequests;
use dispatch::dispatch_document_symbols;
use kuroya_core::BufferId;
use std::path::PathBuf;
use tokio::process::ChildStdin;

pub(super) async fn dispatch_document_symbols_request(
    id: BufferId,
    path: PathBuf,
    version: u64,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    dispatch_document_symbols(id, path, version, writer, next_request_id, pending_requests).await
}
