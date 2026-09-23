use crate::lsp_client::pending::PendingLspRequests;
use crate::lsp_client::{
    commands::LspClientCommand,
    pending::{PendingLspRequest, lsp_request_target_is_valid, register_pending_request},
    request_dispatch::write_request_message,
};
use kuroya_core::LspWireMessage;
use tokio::process::ChildStdin;

use super::super::reserve_request_id;

pub(super) async fn handle_call_hierarchy_request_command(
    command: LspClientCommand,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    match command {
        LspClientCommand::PrepareCallHierarchy {
            id,
            path,
            version,
            line,
            character,
        } => {
            if !lsp_request_target_is_valid(id, &path) {
                return true;
            }

            let request_id = reserve_request_id(next_request_id, pending_requests);
            let message =
                LspWireMessage::prepare_call_hierarchy(request_id, &path, line, character)
                    .to_json();
            register_pending_request(
                pending_requests,
                request_id,
                PendingLspRequest::PrepareCallHierarchy {
                    id,
                    path,
                    version,
                    line,
                    character,
                },
            );
            write_request_message(writer, pending_requests, request_id, message).await
        }
        LspClientCommand::CallHierarchyIncoming {
            id,
            path,
            version,
            item,
        } => {
            if !lsp_request_target_is_valid(id, &path) {
                return true;
            }

            let request_id = reserve_request_id(next_request_id, pending_requests);
            let message = LspWireMessage::call_hierarchy_incoming(request_id, &item).to_json();
            register_pending_request(
                pending_requests,
                request_id,
                PendingLspRequest::CallHierarchyIncoming {
                    id,
                    path,
                    version,
                    item,
                },
            );
            write_request_message(writer, pending_requests, request_id, message).await
        }
        LspClientCommand::CallHierarchyOutgoing {
            id,
            path,
            version,
            item,
        } => {
            if !lsp_request_target_is_valid(id, &path) {
                return true;
            }

            let request_id = reserve_request_id(next_request_id, pending_requests);
            let message = LspWireMessage::call_hierarchy_outgoing(request_id, &item).to_json();
            register_pending_request(
                pending_requests,
                request_id,
                PendingLspRequest::CallHierarchyOutgoing {
                    id,
                    path,
                    version,
                    item,
                },
            );
            write_request_message(writer, pending_requests, request_id, message).await
        }
        _ => true,
    }
}
