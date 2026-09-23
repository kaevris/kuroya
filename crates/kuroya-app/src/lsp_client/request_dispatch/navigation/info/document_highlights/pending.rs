use super::super::super::super::reserve_request_id;
use crate::lsp_client::pending::PendingLspRequests;
use crate::lsp_client::pending::{PendingLspRequest, register_pending_request};
use kuroya_core::{BufferId, LspWireMessage};
use serde_json::Value;
use std::path::PathBuf;

pub(super) fn register_document_highlights_request(
    id: BufferId,
    path: PathBuf,
    version: u64,
    line: usize,
    character: usize,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> (u64, Value) {
    let request_id = reserve_request_id(next_request_id, pending_requests);
    let message = LspWireMessage::document_highlight(request_id, &path, line, character).to_json();
    register_pending_request(
        pending_requests,
        request_id,
        PendingLspRequest::DocumentHighlights {
            id,
            path,
            version,
            line,
            character,
        },
    );
    (request_id, message)
}
