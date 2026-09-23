use crate::lsp_client::pending::PendingLspRequests;
use crate::lsp_client::pending::{PendingLspRequest, register_pending_request};
use kuroya_core::BufferId;
use std::path::PathBuf;

pub(super) fn register_formatting_request(
    lsp_request_id: u64,
    formatting_request_id: u64,
    id: BufferId,
    path: PathBuf,
    version: u64,
    pending_requests: &mut PendingLspRequests,
) {
    register_pending_request(
        pending_requests,
        lsp_request_id,
        PendingLspRequest::Formatting {
            request_id: formatting_request_id,
            id,
            path,
            version,
        },
    );
}
