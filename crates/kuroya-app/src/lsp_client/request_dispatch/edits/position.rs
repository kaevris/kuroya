use crate::lsp_client::pending::PendingLspRequests;
mod completion;
mod signature;

use crate::lsp_client::commands::LspClientCommand;
use tokio::process::ChildStdin;

enum PositionEditRequestKind {
    Completion,
    ResolveCompletionItem,
    SignatureHelp,
}

fn position_edit_request_kind(command: &LspClientCommand) -> Option<PositionEditRequestKind> {
    match command {
        LspClientCommand::Completion { .. } => Some(PositionEditRequestKind::Completion),
        LspClientCommand::ResolveCompletionItem { .. } => {
            Some(PositionEditRequestKind::ResolveCompletionItem)
        }
        LspClientCommand::SignatureHelp { .. } => Some(PositionEditRequestKind::SignatureHelp),
        _ => None,
    }
}

pub(super) async fn handle_position_edit_request_command(
    command: LspClientCommand,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    let Some(kind) = position_edit_request_kind(&command) else {
        return true;
    };

    match kind {
        PositionEditRequestKind::Completion => {
            completion::handle_completion_request_command(
                command,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        PositionEditRequestKind::ResolveCompletionItem => {
            completion::handle_completion_request_command(
                command,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        PositionEditRequestKind::SignatureHelp => {
            signature::handle_signature_help_request_command(
                command,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::position_edit_request_kind;
    use crate::lsp_client::commands::LspClientCommand;

    #[test]
    fn edit_request_routing_ignores_non_position_commands() {
        assert!(position_edit_request_kind(&LspClientCommand::Shutdown).is_none());
    }
}
