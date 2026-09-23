use crate::lsp_client::{commands::LspClientCommand, wire::write_message};
use kuroya_core::{LspRequestId, LspWireMessage};
use serde_json::Value;
use tokio::process::ChildStdin;

pub(super) async fn handle_direct_response_command(
    command: LspClientCommand,
    writer: &mut ChildStdin,
) -> bool {
    match command {
        LspClientCommand::ApplyWorkspaceEditResponse {
            request_id,
            applied,
            failure_reason,
        } => {
            let message = apply_workspace_edit_response_message(
                request_id,
                applied,
                failure_reason.as_deref(),
            );
            write_message(writer, &message).await.is_ok()
        }
        LspClientCommand::DidChangeWatchedFiles { changes } => write_message(
            writer,
            &LspWireMessage::did_change_watched_files(&changes).to_json(),
        )
        .await
        .is_ok(),
        _ => true,
    }
}

fn apply_workspace_edit_response_message(
    request_id: LspRequestId,
    applied: bool,
    failure_reason: Option<&str>,
) -> Value {
    LspWireMessage::apply_workspace_edit_response(request_id, applied, failure_reason, None)
        .to_json()
}

#[cfg(test)]
mod tests {
    use super::apply_workspace_edit_response_message;
    use kuroya_core::LspRequestId;

    #[test]
    fn apply_workspace_edit_success_response_has_apply_result() {
        let message =
            apply_workspace_edit_response_message(LspRequestId::Number(17), true, Some("ignored"));

        assert_eq!(message["jsonrpc"], "2.0");
        assert_eq!(message["id"], 17);
        assert_eq!(message["result"]["applied"], true);
        assert!(message["result"].get("failureReason").is_none());
        assert!(message["result"].get("failedChange").is_none());
        assert!(message.get("method").is_none());
    }

    #[test]
    fn apply_workspace_edit_failure_response_includes_failure_reason() {
        let message = apply_workspace_edit_response_message(
            LspRequestId::Number(18),
            false,
            Some("buffer changed"),
        );

        assert_eq!(message["jsonrpc"], "2.0");
        assert_eq!(message["id"], 18);
        assert_eq!(message["result"]["applied"], false);
        assert_eq!(message["result"]["failureReason"], "buffer changed");
        assert!(message["result"].get("failedChange").is_none());
        assert!(message.get("method").is_none());
    }

    #[test]
    fn apply_workspace_edit_response_preserves_string_request_id() {
        let message = apply_workspace_edit_response_message(
            LspRequestId::String("abc".to_owned()),
            true,
            None,
        );

        assert_eq!(message["id"], "abc");
        assert_eq!(message["result"]["applied"], true);
    }
}
