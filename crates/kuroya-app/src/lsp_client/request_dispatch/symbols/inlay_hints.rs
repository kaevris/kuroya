mod dispatch;
mod pending;

use crate::lsp_client::pending::PendingLspRequests;
use dispatch::dispatch_inlay_hints;
use kuroya_core::BufferId;
use std::path::PathBuf;
use tokio::process::ChildStdin;

pub(super) async fn dispatch_inlay_hints_request(
    id: BufferId,
    path: PathBuf,
    version: u64,
    end_line: usize,
    end_character: usize,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    dispatch_inlay_hints(
        id,
        path,
        version,
        end_line,
        end_character,
        writer,
        next_request_id,
        pending_requests,
    )
    .await
}
