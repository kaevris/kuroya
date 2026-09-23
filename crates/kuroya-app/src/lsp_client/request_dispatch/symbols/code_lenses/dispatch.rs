use super::super::super::reserve_request_id;
use super::pending::{
    register_code_lens_resolve_request, register_code_lenses_request,
    register_execute_command_request,
};
use crate::lsp_client::pending::PendingLspRequests;
use crate::lsp_client::{
    pending::{
        MAX_LSP_OUTBOUND_JSON_PAYLOAD_BYTES, MAX_LSP_OUTBOUND_TEXT_PAYLOAD_CHARS,
        bounded_lsp_outbound_text, lsp_json_payload_is_bounded, lsp_request_target_is_valid,
    },
    request_dispatch::write_request_message,
};
use kuroya_core::{BufferId, LspCodeLens, LspWireMessage};
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};
use tokio::process::ChildStdin;

pub(super) async fn dispatch_code_lenses(
    id: BufferId,
    path: PathBuf,
    version: u64,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    if !lsp_request_target_is_valid(id, &path) {
        return true;
    }

    let request_id = reserve_request_id(next_request_id, pending_requests);
    let message = LspWireMessage::code_lenses(request_id, &path).to_json();
    register_code_lenses_request(request_id, id, path, version, pending_requests);
    write_request_message(writer, pending_requests, request_id, message).await
}

pub(super) async fn dispatch_code_lens_resolve(
    id: BufferId,
    path: PathBuf,
    version: u64,
    lens: LspCodeLens,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    if !lsp_request_target_is_valid(id, &path) {
        return true;
    }

    if !lens.needs_resolve() {
        return true;
    }
    let Some(resolve_payload) = lens.resolve_payload.as_ref() else {
        return true;
    };
    if !lsp_json_payload_arc_is_bounded(resolve_payload) {
        return true;
    }

    let Some((request_id, message)) =
        reserve_optional_request_message(next_request_id, pending_requests, |request_id| {
            LspWireMessage::code_lens_resolve(request_id, &lens)
        })
    else {
        return true;
    };
    register_code_lens_resolve_request(request_id, id, path, version, pending_requests);
    write_request_message(writer, pending_requests, request_id, message).await
}

pub(super) async fn dispatch_execute_command(
    id: BufferId,
    path: PathBuf,
    version: u64,
    title: String,
    command: String,
    arguments: Option<Arc<Value>>,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    if !lsp_request_target_is_valid(id, &path) {
        return true;
    }
    let Some((title, command)) = dispatch_execute_command_target(title, command) else {
        return true;
    };
    if !optional_lsp_json_payload_arc_is_bounded(arguments.as_ref()) {
        return true;
    }

    let Some((request_id, message)) =
        reserve_optional_request_message(next_request_id, pending_requests, |request_id| {
            LspWireMessage::workspace_execute_command(request_id, &command, arguments.as_ref())
        })
    else {
        return true;
    };
    register_execute_command_request(
        request_id,
        id,
        path,
        version,
        title,
        command,
        pending_requests,
    );
    write_request_message(writer, pending_requests, request_id, message).await
}

fn reserve_optional_request_message(
    next_request_id: &mut u64,
    pending_requests: &PendingLspRequests,
    build_message: impl FnOnce(u64) -> Option<LspWireMessage>,
) -> Option<(u64, Value)> {
    let mut candidate_next_request_id = *next_request_id;
    let request_id = reserve_request_id(&mut candidate_next_request_id, pending_requests);
    let message = build_message(request_id)?.to_json();
    *next_request_id = candidate_next_request_id;
    Some((request_id, message))
}

fn optional_lsp_json_payload_arc_is_bounded(payload: Option<&Arc<Value>>) -> bool {
    match payload {
        Some(payload) => lsp_json_payload_arc_is_bounded(payload),
        None => true,
    }
}

fn dispatch_execute_command_target(title: String, command: String) -> Option<(String, String)> {
    let title = bounded_lsp_outbound_text(title, MAX_LSP_OUTBOUND_TEXT_PAYLOAD_CHARS)?;
    let command = bounded_lsp_outbound_text(command, MAX_LSP_OUTBOUND_TEXT_PAYLOAD_CHARS)?;

    (!title.trim().is_empty() && lsp_command_id_is_safe(&command)).then_some((title, command))
}

fn lsp_command_id_is_safe(command: &str) -> bool {
    !command.trim().is_empty()
        && command.trim() == command
        && !command.chars().any(|ch| ch.is_control())
}

fn lsp_json_payload_arc_is_bounded(payload: &Arc<Value>) -> bool {
    lsp_json_payload_is_bounded(payload.as_ref(), MAX_LSP_OUTBOUND_JSON_PAYLOAD_BYTES)
}

#[cfg(test)]
mod tests {
    use super::{
        dispatch_execute_command_target, lsp_json_payload_arc_is_bounded,
        optional_lsp_json_payload_arc_is_bounded, reserve_optional_request_message,
    };
    use crate::lsp_client::pending::{
        MAX_LSP_OUTBOUND_JSON_PAYLOAD_BYTES, MAX_LSP_OUTBOUND_TEXT_PAYLOAD_CHARS,
    };
    use crate::lsp_client::pending::{PendingLspRequest, PendingLspRequests};
    use kuroya_core::{LspCodeLens, LspWireMessage};
    use serde_json::json;
    use std::{
        path::{Path, PathBuf},
        sync::Arc,
    };

    fn hover(version: u64) -> PendingLspRequest {
        PendingLspRequest::Hover {
            id: 1,
            path: PathBuf::from("src/main.rs"),
            version,
            line: 0,
            character: 0,
        }
    }

    #[test]
    fn optional_request_message_does_not_reserve_rejected_request_id() {
        let mut next_request_id = 9;
        let pending_requests = PendingLspRequests::default();

        let message =
            reserve_optional_request_message(&mut next_request_id, &pending_requests, |_| {
                Option::<LspWireMessage>::None
            });

        assert!(message.is_none());
        assert_eq!(next_request_id, 9);
    }

    #[test]
    fn optional_request_message_reserves_only_after_payload_is_sendable() {
        let mut next_request_id = 9;
        let pending_requests = PendingLspRequests::default();

        let (request_id, message) = reserve_optional_request_message(
            &mut next_request_id,
            &pending_requests,
            |request_id| {
                Some(LspWireMessage::code_lenses(
                    request_id,
                    Path::new("src/main.rs"),
                ))
            },
        )
        .expect("sendable request message");

        assert_eq!(request_id, 9);
        assert_eq!(next_request_id, 10);
        assert_eq!(message["id"], 9);
        assert_eq!(message["method"], "textDocument/codeLens");
    }

    #[test]
    fn optional_code_lens_resolve_message_preserves_raw_payload() {
        let mut next_request_id = 9;
        let pending_requests = PendingLspRequests::default();
        let resolve_payload = Arc::new(json!({
            "range": {
                "start": { "line": 3, "character": 4 },
                "end": { "line": 3, "character": 8 }
            },
            "data": {
                "opaque": ["keep", 1, true]
            }
        }));
        let lens = LspCodeLens {
            line: 3,
            column: 4,
            title: String::new(),
            command: None,
            command_arguments: None,
            resolve_payload: Some(resolve_payload.clone()),
        };

        let (request_id, message) = reserve_optional_request_message(
            &mut next_request_id,
            &pending_requests,
            |request_id| LspWireMessage::code_lens_resolve(request_id, &lens),
        )
        .expect("sendable code lens resolve request");

        assert_eq!(request_id, 9);
        assert_eq!(message["method"], "codeLens/resolve");
        assert_eq!(message["params"], resolve_payload.as_ref().clone());
    }

    #[test]
    fn optional_request_message_skips_active_pending_id() {
        let mut next_request_id = 9;
        let pending_requests = PendingLspRequests::from([(9, hover(1))]);

        let (request_id, message) = reserve_optional_request_message(
            &mut next_request_id,
            &pending_requests,
            |request_id| {
                Some(LspWireMessage::code_lenses(
                    request_id,
                    Path::new("src/main.rs"),
                ))
            },
        )
        .expect("sendable request message");

        assert_eq!(request_id, 10);
        assert_eq!(next_request_id, 11);
        assert_eq!(message["id"], 10);
    }

    #[test]
    fn code_lens_json_payload_helpers_reject_oversized_payloads() {
        let valid = Arc::new(json!({ "data": "x".repeat(8) }));
        let oversized = Arc::new(json!({
            "data": "x".repeat(MAX_LSP_OUTBOUND_JSON_PAYLOAD_BYTES)
        }));

        assert!(lsp_json_payload_arc_is_bounded(&valid));
        assert!(optional_lsp_json_payload_arc_is_bounded(None));
        assert!(optional_lsp_json_payload_arc_is_bounded(Some(&valid)));
        assert!(!lsp_json_payload_arc_is_bounded(&oversized));
        assert!(!optional_lsp_json_payload_arc_is_bounded(Some(&oversized)));
    }

    #[test]
    fn execute_command_target_rejects_blank_unsafe_or_oversized_command_ids() {
        assert_eq!(
            dispatch_execute_command_target(
                "Run Test".to_owned(),
                "rust-analyzer.runSingle".to_owned(),
            ),
            Some(("Run Test".to_owned(), "rust-analyzer.runSingle".to_owned()))
        );

        assert!(dispatch_execute_command_target("Run Test".to_owned(), " ".to_owned()).is_none());
        assert!(
            dispatch_execute_command_target(
                "Run Test".to_owned(),
                " rust-analyzer.runSingle ".to_owned(),
            )
            .is_none()
        );
        assert!(
            dispatch_execute_command_target(
                "Run Test".to_owned(),
                "rust-analyzer.run\nSingle".to_owned(),
            )
            .is_none()
        );
        assert!(
            dispatch_execute_command_target(
                "Run Test".to_owned(),
                "x".repeat(MAX_LSP_OUTBOUND_TEXT_PAYLOAD_CHARS + 1),
            )
            .is_none()
        );
        assert!(
            dispatch_execute_command_target(
                "x".repeat(MAX_LSP_OUTBOUND_TEXT_PAYLOAD_CHARS + 1),
                "rust-analyzer.runSingle".to_owned(),
            )
            .is_none()
        );
    }
}
