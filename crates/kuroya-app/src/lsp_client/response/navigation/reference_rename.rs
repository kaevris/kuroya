mod references;
mod rename;

use crate::ui_event_channel::Sender;
use crate::{lsp_client::pending::PendingLspRequest, ui_events::UiEvent};
use serde_json::Value;

pub(super) fn handle_reference_rename_response(
    pending: PendingLspRequest,
    value: Value,
    ui_tx: &Sender<UiEvent>,
) {
    match pending {
        PendingLspRequest::References {
            id,
            path,
            version,
            line,
            character,
        } => {
            references::handle_references_response(
                id, path, version, line, character, &value, ui_tx,
            );
        }
        PendingLspRequest::PrepareRename {
            id,
            path,
            version,
            line,
            character,
        } => {
            rename::handle_prepare_rename_response(
                id, path, version, line, character, &value, ui_tx,
            );
        }
        PendingLspRequest::Rename {
            id,
            path,
            version,
            line,
            character,
            new_name,
        } => {
            rename::handle_rename_response(
                id, path, version, line, character, new_name, &value, ui_tx,
            );
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::handle_reference_rename_response;
    use crate::lsp_client::pending::PendingLspRequest;
    use crossbeam_channel::TryRecvError;
    use serde_json::json;
    use std::path::PathBuf;

    #[test]
    fn reference_rename_response_handler_ignores_misrouted_pending_request() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        handle_reference_rename_response(
            PendingLspRequest::Completion {
                id: 17,
                path: PathBuf::from("src/main.rs"),
                version: 4,
                line: 1,
                character: 2,
            },
            json!({ "result": [] }),
            &tx,
        );

        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
    }
}
