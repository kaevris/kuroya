use super::super::commands::LspClientCommand;
use super::LspClientStopReason;

#[derive(Debug)]
pub(super) enum ClientCommandFamily {
    DocumentSync(LspClientCommand),
    DirectResponse(LspClientCommand),
    Shutdown,
    Request(LspClientCommand),
    Closed,
}

impl ClientCommandFamily {
    pub(super) fn stop_reason(&self) -> Option<LspClientStopReason> {
        match self {
            Self::Shutdown | Self::Closed => Some(LspClientStopReason::Intentional),
            Self::DocumentSync(_) | Self::DirectResponse(_) | Self::Request(_) => None,
        }
    }
}

pub(super) fn client_command_family(command: Option<LspClientCommand>) -> ClientCommandFamily {
    match command {
        Some(
            command @ (LspClientCommand::DidOpen { .. }
            | LspClientCommand::DidChange { .. }
            | LspClientCommand::DidSave { .. }
            | LspClientCommand::DidClose { .. }),
        ) => ClientCommandFamily::DocumentSync(command),
        Some(command @ LspClientCommand::ApplyWorkspaceEditResponse { .. }) => {
            ClientCommandFamily::DirectResponse(command)
        }
        Some(command @ LspClientCommand::DidChangeWatchedFiles { .. }) => {
            ClientCommandFamily::DirectResponse(command)
        }
        Some(LspClientCommand::Shutdown) => ClientCommandFamily::Shutdown,
        Some(command) => ClientCommandFamily::Request(command),
        None => ClientCommandFamily::Closed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_and_closed_command_streams_are_intentional_stops() {
        assert_eq!(
            client_command_family(Some(LspClientCommand::Shutdown)).stop_reason(),
            Some(LspClientStopReason::Intentional)
        );
        assert_eq!(
            client_command_family(None).stop_reason(),
            Some(LspClientStopReason::Intentional)
        );
    }

    #[test]
    fn apply_workspace_edit_response_is_direct_response_command() {
        match client_command_family(Some(LspClientCommand::ApplyWorkspaceEditResponse {
            request_id: kuroya_core::LspRequestId::Number(17),
            applied: true,
            failure_reason: None,
        })) {
            ClientCommandFamily::DirectResponse(LspClientCommand::ApplyWorkspaceEditResponse {
                request_id,
                applied,
                failure_reason,
            }) => {
                assert_eq!(request_id, kuroya_core::LspRequestId::Number(17));
                assert!(applied);
                assert!(failure_reason.is_none());
            }
            other => panic!("expected direct response command, got {other:?}"),
        }
    }
}
