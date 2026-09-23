use super::super::super::{
    pending::{PendingLspRequest, PendingLspRequests},
    response::handle_lsp_response_for_server,
};
use crate::lsp_ui_events::LspServerResultTarget;
use crate::ui_event_channel::Sender;
use crate::ui_events::UiEvent;
use serde_json::Value;
use std::path::Path;

pub(super) fn handle_response_message(
    value: Value,
    language: &str,
    root: &Path,
    generation: u64,
    pending_requests: &mut PendingLspRequests,
    ui_tx: &Sender<UiEvent>,
) {
    let Some(pending) = take_pending_response(&value, pending_requests) else {
        return;
    };

    handle_lsp_response_for_server(
        LspServerResultTarget {
            language: language.to_owned(),
            root: root.to_path_buf(),
            generation,
        },
        pending,
        value,
        ui_tx,
    );
}

fn take_pending_response(
    value: &Value,
    pending_requests: &mut PendingLspRequests,
) -> Option<PendingLspRequest> {
    let object = value.as_object()?;
    if object.contains_key("method") || !has_response_payload(object) {
        return None;
    }

    let response_id = object.get("id").and_then(Value::as_u64)?;
    pending_requests.remove(&response_id)
}

fn has_response_payload(object: &serde_json::Map<String, Value>) -> bool {
    object.contains_key("result") ^ object.contains_key("error")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp_client::pending::PendingLspRequests;
    use crate::lsp_ui_events::LspUiEvent;
    use crossbeam_channel::TryRecvError;
    use serde_json::json;
    use std::path::PathBuf;

    fn hover_pending() -> PendingLspRequest {
        PendingLspRequest::Hover {
            id: 3,
            path: PathBuf::from("src/main.rs"),
            version: 5,
            line: 1,
            character: 4,
        }
    }

    #[test]
    fn response_message_wraps_result_with_current_server_identity() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let mut pending_requests = PendingLspRequests::from([(7, hover_pending())]);
        let root = PathBuf::from("workspace");

        handle_response_message(
            json!({
                "jsonrpc": "2.0",
                "id": 7,
                "result": {
                    "contents": "hover docs"
                }
            }),
            "rust",
            &root,
            9,
            &mut pending_requests,
            &tx,
        );

        assert!(pending_requests.is_empty());
        match rx.recv().expect("wrapped hover result event") {
            UiEvent::Lsp(LspUiEvent::ServerResult { target, event }) => {
                assert_eq!(target.language, "rust");
                assert_eq!(target.root, root);
                assert_eq!(target.generation, 9);
                match *event {
                    LspUiEvent::HoverResult {
                        id,
                        path,
                        version,
                        line,
                        column,
                        contents,
                    } => {
                        assert_eq!(id, 3);
                        assert_eq!(path, PathBuf::from("src/main.rs"));
                        assert_eq!(version, 5);
                        assert_eq!(line, 1);
                        assert_eq!(column, 5);
                        assert_eq!(contents.as_deref(), Some("hover docs"));
                    }
                    other => panic!("expected hover result, got {other:?}"),
                }
            }
            other => panic!("expected wrapped server result, got {other:?}"),
        }
    }

    #[test]
    fn canceled_response_without_pending_request_emits_no_ui_event() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let mut pending_requests = PendingLspRequests::default();

        handle_response_message(
            json!({
                "jsonrpc": "2.0",
                "id": 7,
                "error": {
                    "code": -32800,
                    "message": "Request cancelled"
                }
            }),
            "rust",
            std::path::Path::new("workspace"),
            3,
            &mut pending_requests,
            &tx,
        );

        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn id_only_message_does_not_consume_pending_request() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let mut pending_requests = PendingLspRequests::from([(7, hover_pending())]);

        handle_response_message(
            json!({
                "jsonrpc": "2.0",
                "id": 7
            }),
            "rust",
            std::path::Path::new("workspace"),
            3,
            &mut pending_requests,
            &tx,
        );

        assert!(pending_requests.contains_key(&7));
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn ambiguous_response_payload_does_not_consume_pending_request() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let mut pending_requests = PendingLspRequests::from([(7, hover_pending())]);

        handle_response_message(
            json!({
                "jsonrpc": "2.0",
                "id": 7,
                "result": null,
                "error": {
                    "code": -32603,
                    "message": "Internal error"
                }
            }),
            "rust",
            std::path::Path::new("workspace"),
            3,
            &mut pending_requests,
            &tx,
        );

        assert!(pending_requests.contains_key(&7));
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
    }
}
