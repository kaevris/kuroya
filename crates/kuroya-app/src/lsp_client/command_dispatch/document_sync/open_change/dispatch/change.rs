use super::super::super::DocumentSyncState;
use super::send_buffer_synced;
use crate::ui_event_channel::Sender;
use crate::{
    lsp_client::wire::{lsp_version, write_did_change_full_document, write_did_change_incremental},
    ui_events::UiEvent,
};
use kuroya_core::{
    BufferId, ContentChange, TextDocumentSyncKindSetting, TextSnapshot,
    text_document_content_change_event,
};
use std::path::{Path, PathBuf};
use tokio::process::ChildStdin;

pub(in crate::lsp_client::command_dispatch::document_sync::open_change) async fn dispatch_did_change(
    id: BufferId,
    path: PathBuf,
    version: u64,
    text: TextSnapshot,
    sync_state: &mut DocumentSyncState,
    writer: &mut ChildStdin,
    ui_tx: &Sender<UiEvent>,
) -> bool {
    let wire_version = lsp_version(version);
    let write_result = match sync_state.sync_kind {
        TextDocumentSyncKindSetting::None => Ok(()),
        TextDocumentSyncKindSetting::Full => {
            write_did_change_full_document(writer, &path, wire_version, &text).await
        }
        TextDocumentSyncKindSetting::Incremental => {
            dispatch_incremental_did_change(&path, wire_version, &text, sync_state, writer).await
        }
    };
    if write_result.is_ok() {
        send_buffer_synced(id, path, version, ui_tx);
        true
    } else {
        false
    }
}

async fn dispatch_incremental_did_change(
    path: &Path,
    wire_version: i32,
    text: &TextSnapshot,
    sync_state: &mut DocumentSyncState,
    writer: &mut ChildStdin,
) -> anyhow::Result<()> {
    let new_text = text.text();
    let previous_text = sync_state.synced_texts.get(path).map(String::as_str);
    let write_result = match plan_incremental_write(previous_text, &new_text) {
        IncrementalWrite::Skip => Ok(()),
        IncrementalWrite::Full => {
            write_did_change_full_document(writer, path, wire_version, text).await
        }
        IncrementalWrite::Ranged(change) => {
            write_did_change_incremental(writer, path, wire_version, &change).await
        }
    };
    if write_result.is_ok() {
        sync_state.record_synced_text(path, &new_text);
    }
    write_result
}

#[derive(Debug, PartialEq, Eq)]
enum IncrementalWrite {
    Skip,

    Full,

    Ranged(ContentChange),
}

fn plan_incremental_write(previous_text: Option<&str>, new_text: &str) -> IncrementalWrite {
    match previous_text {
        None => IncrementalWrite::Full,
        Some(previous) => text_document_content_change_event(previous, new_text)
            .map_or(IncrementalWrite::Skip, IncrementalWrite::Ranged),
    }
}

#[cfg(test)]
mod tests {
    use super::{DocumentSyncState, IncrementalWrite, plan_incremental_write};
    use kuroya_core::{ContentChange, TextDocumentSyncKindSetting};

    fn change(
        start_line: usize,
        start_character: usize,
        end_line: usize,
        end_character: usize,
        text: &str,
    ) -> IncrementalWrite {
        IncrementalWrite::Ranged(ContentChange {
            start_line,
            start_character,
            end_line,
            end_character,
            text: text.to_owned(),
        })
    }

    #[test]
    fn plan_falls_back_to_full_write_without_synced_text() {
        assert_eq!(
            plan_incremental_write(None, "brand new"),
            IncrementalWrite::Full
        );
    }

    #[test]
    fn plan_diffs_newest_snapshot_against_last_synced_text() {
        let mut sync_state = DocumentSyncState::new(TextDocumentSyncKindSetting::Incremental);
        sync_state.record_synced_text(std::path::Path::new("src/main.rs"), "one");

        let previous = sync_state
            .synced_texts
            .get(std::path::Path::new("src/main.rs"))
            .map(String::as_str);

        assert_eq!(
            plan_incremental_write(previous, "three"),
            change(0, 0, 0, 2, "thre")
        );
    }

    #[test]
    fn plan_sends_minimal_range_for_first_change_after_did_open() {
        let opened = "fn main() {\n}\n";
        let mut sync_state = DocumentSyncState::new(TextDocumentSyncKindSetting::Incremental);
        sync_state.record_synced_text(std::path::Path::new("src/main.rs"), opened);

        let previous = sync_state
            .synced_texts
            .get(std::path::Path::new("src/main.rs"))
            .map(String::as_str);

        assert_eq!(
            plan_incremental_write(previous, "fn main() {\n    body();\n}\n"),
            change(1, 0, 1, 0, "    body();\n")
        );
    }

    #[test]
    fn plan_skips_identical_text_and_uses_full_for_other_kinds() {
        assert_eq!(
            plan_incremental_write(Some("same"), "same"),
            IncrementalWrite::Skip,
            "identical text must not produce a write"
        );

        for kind in [
            TextDocumentSyncKindSetting::Full,
            TextDocumentSyncKindSetting::None,
        ] {
            let mut sync_state = DocumentSyncState::new(kind);
            sync_state.record_synced_text(std::path::Path::new("src/main.rs"), "one");
            assert!(sync_state.synced_texts.is_empty());
        }
    }
}
