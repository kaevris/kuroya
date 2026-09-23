use crate::lsp_client::pending::PendingLspRequests;
mod code_actions;
mod formatting;

use crate::lsp_client::commands::LspClientCommand;
use tokio::process::ChildStdin;

enum ActionEditRequestKind {
    Formatting,
    CodeActions,
}

fn action_edit_request_kind(command: &LspClientCommand) -> Option<ActionEditRequestKind> {
    match command {
        LspClientCommand::Formatting { .. } => Some(ActionEditRequestKind::Formatting),
        LspClientCommand::CodeActions { .. } | LspClientCommand::ResolveCodeAction { .. } => {
            Some(ActionEditRequestKind::CodeActions)
        }
        _ => None,
    }
}

pub(super) async fn handle_action_edit_request_command(
    command: LspClientCommand,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    let Some(kind) = action_edit_request_kind(&command) else {
        return true;
    };

    match kind {
        ActionEditRequestKind::Formatting => {
            formatting::handle_formatting_request_command(
                command,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        ActionEditRequestKind::CodeActions => {
            code_actions::handle_code_actions_request_command(
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
    use super::action_edit_request_kind;
    use crate::lsp_client::commands::LspClientCommand;

    #[test]
    fn edit_request_routing_ignores_non_action_commands() {
        assert!(action_edit_request_kind(&LspClientCommand::Shutdown).is_none());
    }
}
