use crate::lsp_client::pending::PendingLspRequests;
mod dispatch;
mod pending;

use crate::lsp_client::commands::LspClientCommand;
use dispatch::{dispatch_code_action_resolve, dispatch_code_actions};
use tokio::process::ChildStdin;

pub(super) async fn handle_code_actions_request_command(
    command: LspClientCommand,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    match command {
        LspClientCommand::CodeActions {
            id,
            path,
            version,
            origin_line,
            origin_character,
            start_line,
            start_character,
            end_line,
            end_character,
            diagnostics,
        } => {
            dispatch_code_actions(
                id,
                path,
                version,
                origin_line,
                origin_character,
                start_line,
                start_character,
                end_line,
                end_character,
                diagnostics,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        LspClientCommand::ResolveCodeAction {
            id,
            path,
            version,
            line,
            character,
            action,
        } => {
            dispatch_code_action_resolve(
                id,
                path,
                version,
                line,
                character,
                action,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        _ => true,
    }
}
