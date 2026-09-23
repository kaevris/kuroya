use super::super::super::super::reserve_request_id;
use super::pending::register_formatting_request;
use crate::lsp_client::pending::PendingLspRequests;
use crate::lsp_client::{
    pending::lsp_request_target_is_valid, request_dispatch::write_request_message,
};
use kuroya_core::{BufferId, LspWireMessage};
use std::path::PathBuf;
use tokio::process::ChildStdin;

pub(super) async fn dispatch_formatting(
    formatting_request_id: u64,
    id: BufferId,
    path: PathBuf,
    version: u64,
    tab_size: usize,
    insert_spaces: bool,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    if !lsp_request_target_is_valid(id, &path) {
        return true;
    }

    let lsp_request_id = reserve_request_id(next_request_id, pending_requests);
    let message =
        LspWireMessage::formatting(lsp_request_id, &path, tab_size, insert_spaces).to_json();
    register_formatting_request(
        lsp_request_id,
        formatting_request_id,
        id,
        path,
        version,
        pending_requests,
    );
    write_request_message(writer, pending_requests, lsp_request_id, message).await
}
