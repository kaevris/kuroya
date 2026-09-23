use crate::{
    lsp_client::pending::{PendingLspRequest, PendingLspRequests, register_pending_request},
    lsp_completion_resolve::CompletionResolveIntent,
};
use kuroya_core::{BufferId, LspCompletionItem};
use std::path::PathBuf;

pub(super) fn register_completion_request(
    request_id: u64,
    id: BufferId,
    path: PathBuf,
    version: u64,
    line: usize,
    character: usize,
    pending_requests: &mut PendingLspRequests,
) {
    register_pending_request(
        pending_requests,
        request_id,
        PendingLspRequest::Completion {
            id,
            path,
            version,
            line,
            character,
        },
    );
}

pub(super) fn register_completion_item_resolve_request(
    request_id: u64,
    id: BufferId,
    path: PathBuf,
    version: u64,
    line: usize,
    character: usize,
    item: Box<LspCompletionItem>,
    intent: CompletionResolveIntent,
    pending_requests: &mut PendingLspRequests,
) {
    register_pending_request(
        pending_requests,
        request_id,
        PendingLspRequest::ResolveCompletionItem {
            id,
            path,
            version,
            line,
            character,
            item,
            intent,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::register_completion_item_resolve_request;
    use crate::{
        lsp_client::pending::{PendingLspRequest, PendingLspRequests},
        lsp_completion_resolve::CompletionResolveIntent,
    };
    use kuroya_core::LspCompletionItem;
    use serde_json::json;
    use std::{path::PathBuf, sync::Arc};

    #[test]
    fn completion_item_resolve_pending_request_keeps_origin_and_commit_text() {
        let path = PathBuf::from("src/main.rs");
        let mut pending_requests = PendingLspRequests::default();

        register_completion_item_resolve_request(
            8,
            7,
            path.clone(),
            3,
            2,
            4,
            Box::new(completion_item()),
            CompletionResolveIntent::Apply {
                commit_text: Some(".".to_owned()),
            },
            &mut pending_requests,
        );

        match pending_requests.remove(&8) {
            Some(PendingLspRequest::ResolveCompletionItem {
                id,
                path: pending_path,
                version,
                line,
                character,
                item,
                intent,
            }) => {
                assert_eq!(id, 7);
                assert_eq!(pending_path, path);
                assert_eq!(version, 3);
                assert_eq!(line, 2);
                assert_eq!(character, 4);
                assert_eq!(item.label, "HashMap");
                assert_eq!(
                    intent,
                    CompletionResolveIntent::Apply {
                        commit_text: Some(".".to_owned())
                    }
                );
            }
            other => panic!("unexpected pending request: {other:?}"),
        }
    }

    #[test]
    fn completion_item_resolve_pending_request_preserves_raw_item_payload() {
        let path = PathBuf::from("src/main.rs");
        let mut pending_requests = PendingLspRequests::default();
        let mut raw_item = completion_item();
        raw_item.label = "Raw\nHashMap\u{202e}".to_owned();
        raw_item.detail = Some("raw detail".to_owned());
        let raw_label = raw_item.label.clone();
        raw_item.resolve_payload = Some(Arc::new(json!({
            "label": raw_label,
            "data": {
                "token": "raw-item"
            }
        })));
        let expected_item = raw_item.clone();

        register_completion_item_resolve_request(
            8,
            7,
            path,
            3,
            2,
            4,
            Box::new(raw_item),
            CompletionResolveIntent::Preview { selected: 5 },
            &mut pending_requests,
        );

        match pending_requests.remove(&8) {
            Some(PendingLspRequest::ResolveCompletionItem { item, intent, .. }) => {
                assert_eq!(*item, expected_item);
                assert_eq!(intent, CompletionResolveIntent::Preview { selected: 5 });
            }
            other => panic!("unexpected pending request: {other:?}"),
        }
    }

    fn completion_item() -> LspCompletionItem {
        LspCompletionItem {
            label: "HashMap".to_owned(),
            detail: None,
            documentation: None,
            kind: None,
            deprecated: false,
            is_snippet: false,
            sort_text: None,
            filter_text: None,
            preselect: false,
            commit_characters: Vec::new(),
            insert_text: "HashMap".to_owned(),
            snippet_selection: None,
            snippet_tabstops: Vec::new(),
            snippet_tabstop_groups: Vec::new(),
            text_edit: None,
            insert_text_edit: None,
            additional_text_edits: Vec::new(),
            resolve_payload: None,
        }
    }
}
