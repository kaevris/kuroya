use crate::lsp_client::pending::PendingLspRequests;
mod definition;
mod document_highlights;
mod hover;

use crate::lsp_client::commands::LspClientCommand;
use tokio::process::ChildStdin;

enum InfoNavigationRequestKind {
    Hover,
    DocumentHighlights,
    Definition,
}

fn info_navigation_request_kind(command: &LspClientCommand) -> Option<InfoNavigationRequestKind> {
    match command {
        LspClientCommand::Hover { .. } => Some(InfoNavigationRequestKind::Hover),
        LspClientCommand::DocumentHighlights { .. } => {
            Some(InfoNavigationRequestKind::DocumentHighlights)
        }
        LspClientCommand::Definition { .. } => Some(InfoNavigationRequestKind::Definition),
        _ => None,
    }
}

pub(super) async fn handle_info_navigation_request_command(
    command: LspClientCommand,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    let Some(kind) = info_navigation_request_kind(&command) else {
        return true;
    };

    match kind {
        InfoNavigationRequestKind::Hover => {
            hover::handle_hover_request_command(command, writer, next_request_id, pending_requests)
                .await
        }
        InfoNavigationRequestKind::DocumentHighlights => {
            document_highlights::handle_document_highlights_request_command(
                command,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        InfoNavigationRequestKind::Definition => {
            definition::handle_definition_request_command(
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
    use super::info_navigation_request_kind;
    use crate::lsp_client::commands::LspClientCommand;

    #[test]
    fn navigation_request_routing_ignores_non_info_commands() {
        assert!(info_navigation_request_kind(&LspClientCommand::Shutdown).is_none());
    }
}
