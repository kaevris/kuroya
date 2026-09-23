use crate::lsp_client::pending::PendingLspRequests;
mod dispatch;
mod pending;

use crate::lsp_client::commands::LspClientCommand;
use dispatch::{dispatch_completion, dispatch_completion_item_resolve};
use tokio::process::ChildStdin;

pub(super) async fn handle_completion_request_command(
    command: LspClientCommand,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    match command {
        LspClientCommand::Completion {
            id,
            path,
            version,
            line,
            character,
        } => {
            dispatch_completion(
                id,
                path,
                version,
                line,
                character,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        LspClientCommand::ResolveCompletionItem {
            id,
            path,
            version,
            line,
            character,
            item,
            intent,
        } => {
            dispatch_completion_item_resolve(
                id,
                path,
                version,
                line,
                character,
                item,
                intent,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        _ => true,
    }
}
