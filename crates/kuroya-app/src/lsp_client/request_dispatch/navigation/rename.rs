use crate::lsp_client::pending::PendingLspRequests;
mod dispatch;
mod pending;

use crate::lsp_client::commands::LspClientCommand;
use dispatch::{dispatch_prepare_rename, dispatch_rename};
use tokio::process::ChildStdin;

pub(super) async fn handle_rename_request_command(
    command: LspClientCommand,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    match command {
        LspClientCommand::PrepareRename {
            id,
            path,
            version,
            line,
            character,
        } => {
            dispatch_prepare_rename(
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
        LspClientCommand::Rename {
            id,
            path,
            version,
            line,
            character,
            new_name,
        } => {
            dispatch_rename(
                id,
                path,
                version,
                line,
                character,
                new_name,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        _ => true,
    }
}
