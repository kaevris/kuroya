use crate::lsp_client::pending::PendingLspRequests;
use crate::lsp_client::pending::{PendingLspRequest, register_pending_request};
use kuroya_core::BufferId;
use std::path::PathBuf;

pub(super) fn register_signature_help_request(
    request_id: u64,
    id: BufferId,
    path: PathBuf,
    version: u64,
    line: usize,
    character: usize,
    pending_requests: &mut PendingLspRequests,
) {
    register_pending_request(
        pending_requests,
        request_id,
        PendingLspRequest::SignatureHelp {
            id,
            path,
            version,
            line,
            character,
        },
    );
}
