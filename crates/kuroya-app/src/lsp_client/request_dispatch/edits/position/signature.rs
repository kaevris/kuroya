use crate::lsp_client::pending::PendingLspRequests;
mod dispatch;
mod pending;

use crate::lsp_client::commands::LspClientCommand;
use dispatch::dispatch_signature_help;
use tokio::process::ChildStdin;

pub(super) async fn handle_signature_help_request_command(
    command: LspClientCommand,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    let LspClientCommand::SignatureHelp {
        id,
        path,
        version,
        line,
        character,
    } = command
    else {
        return true;
    };

    dispatch_signature_help(
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
