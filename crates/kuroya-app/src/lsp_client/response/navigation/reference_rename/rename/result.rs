use super::super::super::super::response_error;
use crate::ui_event_channel::Sender;
use crate::{lsp_ui_events::LspUiEvent, ui_events::UiEvent};
use kuroya_core::{
    BufferId, LspTextEdit, lsp::file_uri_to_path, parse_prepare_rename_response,
    parse_workspace_edit_response,
};
use serde_json::{Map, Value};
use std::path::PathBuf;

const INVALID_RENAME_RESPONSE: &str = "invalid textDocument/rename response";

pub(super) fn send_prepare_rename_result(
    id: BufferId,
    path: PathBuf,
    version: u64,
    line: usize,
    character: usize,
    value: &Value,
    ui_tx: &Sender<UiEvent>,
) {
    let error = response_error(value);
    let range = if error.is_none() {
        parse_prepare_rename_response(value)
    } else {
        None
    };
    let _ = crate::lsp_client::response::emit_critical_lsp_response_event(
        ui_tx,
        LspUiEvent::PrepareRenameResult {
            id,
            path,
            version,
            line,
            column: character,
            range,
            error,
        },
    );
}

pub(super) fn send_rename_result(
    id: BufferId,
    path: PathBuf,
    version: u64,
    line: usize,
    character: usize,
    new_name: String,
    value: &Value,
    ui_tx: &Sender<UiEvent>,
) {
    let mut error = response_error(value);
    let edits = if error.is_none() {
        match parse_rename_success_response(value) {
            RenameSuccessResponse::Edits(edits) => Some(edits),
            RenameSuccessResponse::UnsupportedResourceOperation => None,
            RenameSuccessResponse::Invalid => {
                error = Some(INVALID_RENAME_RESPONSE.to_owned());
                None
            }
        }
    } else {
        None
    };
    let _ = crate::lsp_client::response::emit_critical_lsp_response_event(
        ui_tx,
        LspUiEvent::RenameResult {
            id,
            origin_path: path,
            version,
            origin_line: line,
            origin_column: character.saturating_add(1),
            new_name,
            edits,
            error,
        },
    );
}

enum RenameSuccessResponse {
    Edits(Vec<LspTextEdit>),
    UnsupportedResourceOperation,
    Invalid,
}

fn parse_rename_success_response(value: &Value) -> RenameSuccessResponse {
    let Some(result) = value.get("result") else {
        return RenameSuccessResponse::Invalid;
    };
    if result.is_null() {
        return RenameSuccessResponse::Edits(Vec::new());
    }
    if !result.is_object() {
        return RenameSuccessResponse::Invalid;
    }

    if let Some(document_changes) = result.get("documentChanges") {
        let Some(document_changes) = document_changes.as_array() else {
            return RenameSuccessResponse::Invalid;
        };
        let mut has_resource_operation = false;
        for document_change in document_changes {
            match document_change_kind(document_change) {
                DocumentChangeKind::TextEdit => {
                    if !single_document_change_text_edit_is_parseable(document_change) {
                        return RenameSuccessResponse::Invalid;
                    }
                }
                DocumentChangeKind::ResourceOperation => {
                    has_resource_operation = true;
                }
                DocumentChangeKind::Invalid => {
                    return RenameSuccessResponse::Invalid;
                }
            }
        }

        if has_resource_operation {
            return RenameSuccessResponse::UnsupportedResourceOperation;
        }
    }

    match parse_workspace_edit_response(value) {
        Some(edits) => RenameSuccessResponse::Edits(edits),
        None => RenameSuccessResponse::Invalid,
    }
}

enum DocumentChangeKind {
    TextEdit,
    ResourceOperation,
    Invalid,
}

fn document_change_kind(value: &Value) -> DocumentChangeKind {
    let Some(object) = value.as_object() else {
        return DocumentChangeKind::Invalid;
    };
    let Some(kind) = object.get("kind") else {
        return DocumentChangeKind::TextEdit;
    };
    if object.contains_key("textDocument") {
        return DocumentChangeKind::Invalid;
    }

    let Some(kind) = kind.as_str() else {
        return DocumentChangeKind::Invalid;
    };
    match kind {
        "create" => {
            if resource_operation_uri_is_valid(object, "uri") {
                DocumentChangeKind::ResourceOperation
            } else {
                DocumentChangeKind::Invalid
            }
        }
        "rename" => {
            if resource_operation_uri_is_valid(object, "oldUri")
                && resource_operation_uri_is_valid(object, "newUri")
            {
                DocumentChangeKind::ResourceOperation
            } else {
                DocumentChangeKind::Invalid
            }
        }
        "delete" => {
            if resource_operation_uri_is_valid(object, "uri") {
                DocumentChangeKind::ResourceOperation
            } else {
                DocumentChangeKind::Invalid
            }
        }
        _ => DocumentChangeKind::Invalid,
    }
}

fn resource_operation_uri_is_valid(object: &Map<String, Value>, field: &str) -> bool {
    object
        .get(field)
        .and_then(Value::as_str)
        .and_then(file_uri_to_path)
        .is_some()
}

fn single_document_change_text_edit_is_parseable(document_change: &Value) -> bool {
    let value = Value::Object(Map::from_iter([(
        "result".to_owned(),
        Value::Object(Map::from_iter([(
            "documentChanges".to_owned(),
            Value::Array(vec![document_change.clone()]),
        )])),
    )]));
    parse_workspace_edit_response(&value).is_some()
}

#[cfg(test)]
mod tests {
    use super::{INVALID_RENAME_RESPONSE, send_rename_result};
    use crate::{lsp_ui_events::LspUiEvent, ui_events::UiEvent};
    use kuroya_core::lsp::path_to_file_uri;
    use serde_json::{Value, json};
    use std::path::{Path, PathBuf};

    #[test]
    fn rename_result_reports_invalid_success_payload() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        send_rename_result(
            7,
            PathBuf::from("workspace/src/main.rs"),
            11,
            3,
            12,
            "new_name".to_owned(),
            &json!({ "result": [] }),
            &tx,
        );

        match rx.recv().expect("rename result event") {
            UiEvent::Lsp(LspUiEvent::RenameResult { edits, error, .. }) => {
                assert_eq!(edits, None);
                assert_eq!(error.as_deref(), Some(INVALID_RENAME_RESPONSE));
            }
            other => panic!("expected rename result event, got {other:?}"),
        }
    }

    #[test]
    fn rename_result_reports_malformed_workspace_edit() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        send_rename_result(
            7,
            PathBuf::from("workspace/src/main.rs"),
            11,
            3,
            12,
            "new_name".to_owned(),
            &json!({
                "result": {
                    "changes": {
                        "file:///workspace/src/main.rs": [{
                            "range": {
                                "start": { "line": 3, "character": 12 },
                                "end": { "line": "bad", "character": 20 }
                            },
                            "newText": "new_name"
                        }]
                    }
                }
            }),
            &tx,
        );

        match rx.recv().expect("rename result event") {
            UiEvent::Lsp(LspUiEvent::RenameResult { edits, error, .. }) => {
                assert_eq!(edits, None);
                assert_eq!(error.as_deref(), Some(INVALID_RENAME_RESPONSE));
            }
            other => panic!("expected rename result event, got {other:?}"),
        }
    }

    #[test]
    fn rename_result_reports_malformed_document_change_beside_resource_operation() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        send_rename_result(
            7,
            PathBuf::from("workspace/src/main.rs"),
            11,
            3,
            12,
            "new_name".to_owned(),
            &json!({
                "result": {
                    "documentChanges": [
                        {
                            "textDocument": {
                                "uri": "file:///workspace/src/main.rs",
                                "version": 11
                            },
                            "edits": [{
                                "range": {
                                    "start": { "line": 3, "character": 12 },
                                    "end": { "line": 3 }
                                },
                                "newText": "new_name"
                            }]
                        },
                        {
                            "kind": "create",
                            "uri": "file:///workspace/src/new_name.rs"
                        }
                    ]
                }
            }),
            &tx,
        );

        match rx.recv().expect("rename result event") {
            UiEvent::Lsp(LspUiEvent::RenameResult { edits, error, .. }) => {
                assert_eq!(edits, None);
                assert_eq!(error.as_deref(), Some(INVALID_RENAME_RESPONSE));
            }
            other => panic!("expected rename result event, got {other:?}"),
        }
    }

    #[test]
    fn rename_result_reports_malformed_resource_operation_uri() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        send_rename_result(
            7,
            PathBuf::from("workspace/src/main.rs"),
            11,
            3,
            12,
            "new_name".to_owned(),
            &json!({
                "result": {
                    "documentChanges": [
                        {
                            "textDocument": {
                                "uri": "file:///workspace/src/main.rs",
                                "version": 11
                            },
                            "edits": [{
                                "range": {
                                    "start": { "line": 3, "character": 12 },
                                    "end": { "line": 3, "character": 20 }
                                },
                                "newText": "new_name"
                            }]
                        },
                        {
                            "kind": "create",
                            "uri": "file://server%GG/share/new_name.rs"
                        }
                    ]
                }
            }),
            &tx,
        );

        match rx.recv().expect("rename result event") {
            UiEvent::Lsp(LspUiEvent::RenameResult { edits, error, .. }) => {
                assert_eq!(edits, None);
                assert_eq!(error.as_deref(), Some(INVALID_RENAME_RESPONSE));
            }
            other => panic!("expected rename result event, got {other:?}"),
        }
    }

    #[test]
    fn rename_result_treats_null_success_as_no_edits() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        send_rename_result(
            7,
            PathBuf::from("workspace/src/main.rs"),
            11,
            3,
            12,
            "new_name".to_owned(),
            &json!({ "result": null }),
            &tx,
        );

        match rx.recv().expect("rename result event") {
            UiEvent::Lsp(LspUiEvent::RenameResult { edits, error, .. }) => {
                assert_eq!(edits, Some(Vec::new()));
                assert_eq!(error, None);
            }
            other => panic!("expected rename result event, got {other:?}"),
        }
    }

    #[test]
    fn rename_result_bounds_unrepresentable_origin_column() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        send_rename_result(
            7,
            PathBuf::from("workspace/src/main.rs"),
            11,
            3,
            usize::MAX,
            "new_name".to_owned(),
            &json!({ "result": null }),
            &tx,
        );

        match rx.recv().expect("rename result event") {
            UiEvent::Lsp(LspUiEvent::RenameResult {
                origin_column,
                edits,
                error,
                ..
            }) => {
                assert_eq!(origin_column, usize::MAX);
                assert_eq!(edits, Some(Vec::new()));
                assert_eq!(error, None);
            }
            other => panic!("expected rename result event, got {other:?}"),
        }
    }

    #[test]
    fn rename_result_preserves_valid_workspace_edits() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let path = PathBuf::from("workspace/src/main.rs");
        let uri = path_to_file_uri(Path::new("workspace/src/main.rs"));

        send_rename_result(
            7,
            path.clone(),
            11,
            3,
            12,
            "new_name".to_owned(),
            &json!({
                "result": {
                    "changes": {
                        uri: [{
                            "range": {
                                "start": { "line": 3, "character": 12 },
                                "end": { "line": 3, "character": 20 }
                            },
                            "newText": "new_name"
                        }]
                    }
                }
            }),
            &tx,
        );

        match rx.recv().expect("rename result event") {
            UiEvent::Lsp(LspUiEvent::RenameResult { edits, error, .. }) => {
                let edits = edits.expect("parsed rename edits");
                assert_eq!(edits.len(), 1);
                assert!(edits[0].path.ends_with(Path::new("workspace/src/main.rs")));
                assert_eq!(edits[0].start_line, 4);
                assert_eq!(edits[0].start_column, 13);
                assert_eq!(edits[0].end_column, 21);
                assert_eq!(edits[0].new_text, "new_name");
                assert_eq!(error, None);
            }
            other => panic!("expected rename result event, got {other:?}"),
        }
    }

    #[test]
    fn rename_result_preserves_raw_requested_name_and_edit_text() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let path = PathBuf::from("workspace/src/main.rs");
        let uri = path_to_file_uri(Path::new("workspace/src/main.rs"));
        let raw_new_name = "new\n\u{202e}name";
        let raw_new_text = "replacement\r\n\u{202e}text";

        send_rename_result(
            7,
            path.clone(),
            11,
            3,
            12,
            raw_new_name.to_owned(),
            &json!({
                "result": {
                    "changes": {
                        uri: [{
                            "range": {
                                "start": { "line": 3, "character": 12 },
                                "end": { "line": 3, "character": 20 }
                            },
                            "newText": raw_new_text
                        }]
                    }
                }
            }),
            &tx,
        );

        match rx.recv().expect("rename result event") {
            UiEvent::Lsp(LspUiEvent::RenameResult {
                origin_path,
                new_name,
                edits,
                error,
                ..
            }) => {
                let edits = edits.expect("parsed rename edits");
                assert_eq!(origin_path, path);
                assert_eq!(new_name, raw_new_name);
                assert_eq!(edits.len(), 1);
                assert_eq!(edits[0].new_text, raw_new_text);
                assert!(edits[0].new_text.contains('\r'));
                assert!(edits[0].new_text.contains('\n'));
                assert!(edits[0].new_text.contains('\u{202e}'));
                assert_eq!(error, None);
            }
            other => panic!("expected rename result event, got {other:?}"),
        }
    }

    #[test]
    fn rename_result_suppresses_partial_edits_when_document_changes_create_files() {
        assert_rename_result_degrades_safely(json!({
            "kind": "create",
            "uri": "file:///workspace/src/new_name.rs"
        }));
    }

    #[test]
    fn rename_result_suppresses_partial_edits_when_document_changes_rename_files() {
        assert_rename_result_degrades_safely(json!({
            "kind": "rename",
            "oldUri": "file:///workspace/src/old_name.rs",
            "newUri": "file:///workspace/src/new_name.rs"
        }));
    }

    #[test]
    fn rename_result_suppresses_partial_edits_when_document_changes_delete_files() {
        assert_rename_result_degrades_safely(json!({
            "kind": "delete",
            "uri": "file:///workspace/src/old_name.rs"
        }));
    }

    fn assert_rename_result_degrades_safely(resource_operation: Value) {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let path = PathBuf::from("workspace/src/main.rs");

        send_rename_result(
            7,
            path.clone(),
            11,
            3,
            12,
            "new_name".to_owned(),
            &json!({
                "jsonrpc": "2.0",
                "id": 31,
                "result": {
                    "documentChanges": [
                        {
                            "textDocument": {
                                "uri": "file:///workspace/src/main.rs",
                                "version": 11
                            },
                            "edits": [{
                                "range": {
                                    "start": { "line": 3, "character": 12 },
                                    "end": { "line": 3, "character": 20 }
                                },
                                "newText": "new_name"
                            }]
                        },
                        resource_operation
                    ]
                }
            }),
            &tx,
        );

        match rx.recv().expect("rename result event") {
            UiEvent::Lsp(LspUiEvent::RenameResult {
                id,
                origin_path,
                version,
                origin_line,
                origin_column,
                new_name,
                edits,
                error,
            }) => {
                assert_eq!(id, 7);
                assert_eq!(origin_path, path);
                assert_eq!(version, 11);
                assert_eq!(origin_line, 3);
                assert_eq!(origin_column, 13);
                assert_eq!(new_name, "new_name");
                assert_eq!(edits, None);
                assert_eq!(error, None);
            }
            other => panic!("expected rename result event, got {other:?}"),
        }
    }
}
