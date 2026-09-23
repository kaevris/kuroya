use crate::lsp_client::pending::{PendingLspRequest, PendingLspRequests, register_pending_request};
use kuroya_core::BufferId;
use std::path::PathBuf;

pub(super) fn register_code_lenses_request(
    request_id: u64,
    id: BufferId,
    path: PathBuf,
    version: u64,
    pending_requests: &mut PendingLspRequests,
) {
    register_pending_request(
        pending_requests,
        request_id,
        PendingLspRequest::CodeLenses { id, path, version },
    );
}

pub(super) fn register_code_lens_resolve_request(
    request_id: u64,
    id: BufferId,
    path: PathBuf,
    version: u64,
    pending_requests: &mut PendingLspRequests,
) {
    register_pending_request(
        pending_requests,
        request_id,
        PendingLspRequest::ResolveCodeLens { id, path, version },
    );
}

pub(super) fn register_execute_command_request(
    request_id: u64,
    id: BufferId,
    path: PathBuf,
    version: u64,
    title: String,
    command: String,
    pending_requests: &mut PendingLspRequests,
) {
    register_pending_request(
        pending_requests,
        request_id,
        PendingLspRequest::ExecuteCommand {
            id,
            path,
            version,
            title,
            command,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::{
        register_code_lens_resolve_request, register_code_lenses_request,
        register_execute_command_request,
    };
    use crate::lsp_client::pending::{PendingLspRequest, PendingLspRequests};
    use std::path::Path;

    #[test]
    fn code_lens_resolve_pending_request_keeps_buffer_identity() {
        let mut pending_requests = PendingLspRequests::default();
        let path = Path::new("src/main.rs");
        let request_id = 1;

        register_code_lens_resolve_request(
            request_id,
            9,
            path.to_path_buf(),
            12,
            &mut pending_requests,
        );

        match pending_requests.get(&request_id) {
            Some(PendingLspRequest::ResolveCodeLens { id, path, version }) => {
                assert_eq!(*id, 9);
                assert_eq!(path, Path::new("src/main.rs"));
                assert_eq!(*version, 12);
            }
            other => panic!("expected code lens resolve pending request, got {other:?}"),
        }
    }

    #[test]
    fn code_lenses_pending_request_keeps_buffer_identity() {
        let mut pending_requests = PendingLspRequests::default();
        let path = Path::new("src/main.rs");
        let request_id = 1;

        register_code_lenses_request(request_id, 9, path.to_path_buf(), 12, &mut pending_requests);

        match pending_requests.get(&request_id) {
            Some(PendingLspRequest::CodeLenses { id, path, version }) => {
                assert_eq!(*id, 9);
                assert_eq!(path, Path::new("src/main.rs"));
                assert_eq!(*version, 12);
            }
            other => panic!("expected code lenses pending request, got {other:?}"),
        }
    }

    #[test]
    fn execute_command_pending_request_keeps_command_context() {
        let mut pending_requests = PendingLspRequests::default();
        let path = Path::new("src/main.rs");
        let request_id = 1;

        register_execute_command_request(
            request_id,
            9,
            path.to_path_buf(),
            12,
            "Run Test".to_owned(),
            "rust-analyzer.runSingle".to_owned(),
            &mut pending_requests,
        );

        match pending_requests.get(&request_id) {
            Some(PendingLspRequest::ExecuteCommand {
                id,
                path,
                version,
                title,
                command,
            }) => {
                assert_eq!(*id, 9);
                assert_eq!(path, Path::new("src/main.rs"));
                assert_eq!(*version, 12);
                assert_eq!(title, "Run Test");
                assert_eq!(command, "rust-analyzer.runSingle");
            }
            other => panic!("expected execute command pending request, got {other:?}"),
        }
    }
}
