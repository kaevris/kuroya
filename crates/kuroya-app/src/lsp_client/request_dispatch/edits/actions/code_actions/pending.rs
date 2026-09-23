use crate::lsp_client::pending::{PendingLspRequest, PendingLspRequests, register_pending_request};
use kuroya_core::BufferId;
use std::path::PathBuf;

pub(super) fn register_code_actions_request(
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
        PendingLspRequest::CodeActions {
            id,
            path,
            version,
            line,
            character,
        },
    );
}

pub(super) fn register_code_action_resolve_request(
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
        PendingLspRequest::ResolveCodeAction {
            id,
            path,
            version,
            line,
            character,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::{register_code_action_resolve_request, register_code_actions_request};
    use crate::lsp_client::pending::{PendingLspRequest, PendingLspRequests};
    use std::path::PathBuf;

    #[test]
    fn code_actions_pending_request_keeps_origin_cursor_position() {
        let request_id = 1;
        let mut pending_requests = PendingLspRequests::default();
        let path = PathBuf::from("src/main.rs");

        register_code_actions_request(request_id, 9, path, 12, 4, 17, &mut pending_requests);

        match pending_requests.get(&request_id) {
            Some(PendingLspRequest::CodeActions {
                id,
                path,
                version,
                line,
                character,
            }) => {
                assert_eq!(*id, 9);
                assert_eq!(path, &PathBuf::from("src/main.rs"));
                assert_eq!(*version, 12);
                assert_eq!(*line, 4);
                assert_eq!(*character, 17);
            }
            other => panic!("expected code action pending request, got {other:?}"),
        }
    }

    #[test]
    fn code_action_resolve_pending_request_keeps_origin_cursor_position() {
        let request_id = 1;
        let mut pending_requests = PendingLspRequests::default();
        let path = PathBuf::from("src/main.rs");

        register_code_action_resolve_request(request_id, 9, path, 12, 4, 17, &mut pending_requests);

        match pending_requests.get(&request_id) {
            Some(PendingLspRequest::ResolveCodeAction {
                id,
                path,
                version,
                line,
                character,
            }) => {
                assert_eq!(*id, 9);
                assert_eq!(path, &PathBuf::from("src/main.rs"));
                assert_eq!(*version, 12);
                assert_eq!(*line, 4);
                assert_eq!(*character, 17);
            }
            other => panic!("expected code action resolve pending request, got {other:?}"),
        }
    }
}
