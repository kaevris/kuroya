use crate::lsp_client::pending::PendingLspRequests;
mod actions;
mod family;
mod position;

use crate::lsp_client::commands::LspClientCommand;
use family::{EditRequestFamily, edit_request_family};
use tokio::process::ChildStdin;

pub(super) async fn handle_edit_request_command(
    command: LspClientCommand,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    let Some(family) = edit_request_family(&command) else {
        return true;
    };

    match family {
        EditRequestFamily::Position => {
            position::handle_position_edit_request_command(
                command,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        EditRequestFamily::Actions => {
            actions::handle_action_edit_request_command(
                command,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
    }
}
