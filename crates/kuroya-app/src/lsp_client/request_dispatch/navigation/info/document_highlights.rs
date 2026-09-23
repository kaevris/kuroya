use crate::lsp_client::pending::PendingLspRequests;
mod dispatch;
mod pending;

use crate::lsp_client::commands::LspClientCommand;
use dispatch::dispatch_document_highlights;
use tokio::process::ChildStdin;

pub(super) async fn handle_document_highlights_request_command(
    command: LspClientCommand,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    let LspClientCommand::DocumentHighlights {
        id,
        path,
        version,
        line,
        character,
    } = command
    else {
        return true;
    };

    dispatch_document_highlights(
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
