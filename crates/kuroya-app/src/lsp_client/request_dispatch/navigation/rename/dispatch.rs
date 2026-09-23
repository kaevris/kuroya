use super::super::super::reserve_request_id;
use super::pending::{register_prepare_rename_request, register_rename_request};
use crate::lsp_client::pending::PendingLspRequests;
use crate::lsp_client::{
    pending::{
        MAX_LSP_OUTBOUND_TEXT_PAYLOAD_CHARS, bounded_lsp_outbound_text, lsp_request_target_is_valid,
    },
    request_dispatch::write_request_message,
};
use kuroya_core::{BufferId, LspWireMessage};
use std::path::PathBuf;
use tokio::process::ChildStdin;

pub(super) async fn dispatch_prepare_rename(
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

    let request_id = reserve_request_id(next_request_id, pending_requests);
    let message = LspWireMessage::prepare_rename(request_id, &path, line, character).to_json();
    register_prepare_rename_request(
        request_id,
        id,
        path,
        version,
        line,
        character,
        pending_requests,
    );
    write_request_message(writer, pending_requests, request_id, message).await
}

pub(super) async fn dispatch_rename(
    id: BufferId,
    path: PathBuf,
    version: u64,
    line: usize,
    character: usize,
    new_name: String,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    if !lsp_request_target_is_valid(id, &path) {
        return true;
    }
    let Some(new_name) = dispatch_rename_new_name(new_name) else {
        return true;
    };

    let request_id = reserve_request_id(next_request_id, pending_requests);
    let message = LspWireMessage::rename(request_id, &path, line, character, &new_name).to_json();
    register_rename_request(
        request_id,
        id,
        path,
        version,
        line,
        character,
        new_name,
        pending_requests,
    );
    write_request_message(writer, pending_requests, request_id, message).await
}

fn dispatch_rename_new_name(new_name: String) -> Option<String> {
    if new_name.is_empty() {
        return None;
    }

    bounded_lsp_outbound_text(new_name, MAX_LSP_OUTBOUND_TEXT_PAYLOAD_CHARS)
}

#[cfg(test)]
mod tests {
    use super::dispatch_rename_new_name;
    use crate::lsp_client::pending::MAX_LSP_OUTBOUND_TEXT_PAYLOAD_CHARS;

    #[test]
    fn rename_new_name_keeps_bounded_text() {
        let new_name = "renamed_symbol".to_owned();

        assert_eq!(dispatch_rename_new_name(new_name.clone()), Some(new_name));
    }

    #[test]
    fn rename_new_name_rejects_empty_or_oversized_text() {
        assert!(dispatch_rename_new_name(String::new()).is_none());
        assert!(
            dispatch_rename_new_name("x".repeat(MAX_LSP_OUTBOUND_TEXT_PAYLOAD_CHARS + 1)).is_none()
        );
    }
}
