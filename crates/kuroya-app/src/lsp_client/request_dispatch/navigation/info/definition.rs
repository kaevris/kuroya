use crate::lsp_client::pending::PendingLspRequests;
mod dispatch;
mod pending;

use crate::lsp_client::commands::LspClientCommand;
use dispatch::dispatch_definition;
use tokio::process::ChildStdin;

pub(super) async fn handle_definition_request_command(
    command: LspClientCommand,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    let LspClientCommand::Definition {
        id,
        path,
        version,
        line,
        character,
    } = command
    else {
        return true;
    };

    dispatch_definition(
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
