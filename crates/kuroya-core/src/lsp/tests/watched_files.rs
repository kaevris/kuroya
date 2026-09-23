use super::*;

#[test]
fn did_change_watched_files_notification_carries_changes() {
    let message = LspWireMessage::did_change_watched_files(&[
        WatchedFileChange {
            uri: "file:///workspace/src/main.rs".to_owned(),
            kind: WatchedFileChange::CHANGED,
        },
        WatchedFileChange {
            uri: "file:///workspace/src/new.rs".to_owned(),
            kind: WatchedFileChange::CREATED,
        },
        WatchedFileChange {
            uri: "file:///workspace/src/old.rs".to_owned(),
            kind: WatchedFileChange::DELETED,
        },
    ]);

    let LspWireMessage::Notification { method, params } = message else {
        panic!("did_change_watched_files must build a notification");
    };
    assert_eq!(method, "workspace/didChangeWatchedFiles");
    let changes = params["changes"].as_array().expect("changes array");
    assert_eq!(changes.len(), 3);
    assert_eq!(changes[0]["uri"], "file:///workspace/src/main.rs");
    assert_eq!(changes[0]["type"], 2);
    assert_eq!(changes[1]["uri"], "file:///workspace/src/new.rs");
    assert_eq!(changes[1]["type"], 1);
    assert_eq!(changes[2]["uri"], "file:///workspace/src/old.rs");
    assert_eq!(changes[2]["type"], 3);
}

#[test]
fn did_change_watched_files_from_paths_uses_file_uris() {
    let path = std::path::Path::new("workspace/src/main.rs");
    let message = LspWireMessage::did_change_watched_files(&[WatchedFileChange {
        uri: path_to_file_uri(path),
        kind: WatchedFileChange::CHANGED,
    }]);

    let LspWireMessage::Notification { params, .. } = message else {
        panic!("did_change_watched_files must build a notification");
    };
    let uri = params["changes"][0]["uri"].as_str().expect("uri string");
    assert!(uri.starts_with("file://"), "expected file URI, got {uri}");
    assert!(uri.ends_with("/workspace/src/main.rs"));
}

#[test]
fn register_capability_response_has_empty_capabilities_result() {
    let message = LspWireMessage::register_capability_response(LspRequestId::Number(41));

    let LspWireMessage::Response { id, result } = message else {
        panic!("register_capability_response must build a response");
    };
    assert_eq!(id, LspRequestId::Number(41));
    assert_eq!(result["capabilities"], serde_json::json!({}));
}

#[test]
fn unregister_capability_response_has_empty_capabilities_result() {
    let message = LspWireMessage::unregister_capability_response(LspRequestId::Number(42));

    let LspWireMessage::Response { id, result } = message else {
        panic!("unregister_capability_response must build a response");
    };
    assert_eq!(id, LspRequestId::Number(42));
    assert_eq!(result["capabilities"], serde_json::json!({}));
}

#[test]
fn capability_responses_preserve_string_request_ids() {
    for message in [
        LspWireMessage::register_capability_response(LspRequestId::String("watch-1".to_owned())),
        LspWireMessage::unregister_capability_response(LspRequestId::String("watch-2".to_owned())),
    ] {
        let LspWireMessage::Response { id, .. } = message else {
            panic!("capability responses must build responses");
        };
        assert!(matches!(id, LspRequestId::String(_)));
    }
}
