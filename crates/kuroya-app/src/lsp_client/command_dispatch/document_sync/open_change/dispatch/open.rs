use super::super::super::DocumentSyncState;
use super::send_buffer_synced;
use crate::ui_event_channel::Sender;
use crate::{
    lsp_client::wire::{lsp_version, write_did_open_full_document},
    ui_events::UiEvent,
};
use kuroya_core::{BufferId, TextSnapshot};
use std::path::PathBuf;
use tokio::process::ChildStdin;

pub(in crate::lsp_client::command_dispatch::document_sync::open_change) async fn dispatch_did_open(
    id: BufferId,
    path: PathBuf,
    language: String,
    version: u64,
    text: TextSnapshot,
    sync_state: &mut DocumentSyncState,
    writer: &mut ChildStdin,
    ui_tx: &Sender<UiEvent>,
) -> bool {
    let wire_version = lsp_version(version);
    let write_result =
        write_did_open_full_document(writer, &path, &language, wire_version, &text).await;
    if write_result.is_ok() {
        // didOpen (re)establishes what the server holds for this document.
        // The snapshot materialization only happens for incremental servers,
        // the only kind that consults the synced-text tracker.
        if sync_state.sync_kind == kuroya_core::TextDocumentSyncKindSetting::Incremental {
            sync_state.record_synced_text(&path, &text.text());
        }
        send_buffer_synced(id, path, version, ui_tx);
        true
    } else {
        false
    }
}
