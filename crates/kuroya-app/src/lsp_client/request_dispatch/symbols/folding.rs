mod dispatch;
mod pending;

use crate::lsp_client::pending::PendingLspRequests;
use dispatch::dispatch_folding_ranges;
use kuroya_core::BufferId;
use std::path::PathBuf;
use tokio::process::ChildStdin;

pub(super) async fn dispatch_folding_ranges_request(
    id: BufferId,
    path: PathBuf,
    version: u64,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    dispatch_folding_ranges(id, path, version, writer, next_request_id, pending_requests).await
}
