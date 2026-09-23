mod open_change;
mod save_close;

use super::super::commands::LspClientCommand;
use crate::ui_event_channel::Sender;
use crate::ui_events::UiEvent;
use kuroya_core::TextDocumentSyncKindSetting;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::process::ChildStdin;

/// Per-server document sync state owned by the runtime task: the negotiated
/// `textDocumentSync` kind plus, per open document, the last text the server
/// acknowledged (the basis for incremental diffs).
///
/// `synced_texts` only tracks buffers while a server negotiated incremental
/// sync, so the cost is one text copy per open synced buffer there. Full and
/// None servers keep no text; large, lossy, and binary buffers never sync
/// (existing eligibility checks), which bounds each copy.
pub(in crate::lsp_client) struct DocumentSyncState {
    pub(in crate::lsp_client) sync_kind: TextDocumentSyncKindSetting,
    pub(super) synced_texts: HashMap<PathBuf, String>,
}

impl DocumentSyncState {
    pub(in crate::lsp_client) fn new(sync_kind: TextDocumentSyncKindSetting) -> Self {
        Self {
            sync_kind,
            synced_texts: HashMap::new(),
        }
    }

    /// Records the text the server now holds for an open document. Only
    /// incremental servers need the tracker, so other kinds store nothing.
    pub(super) fn record_synced_text(&mut self, path: &Path, text: &str) {
        if self.sync_kind == TextDocumentSyncKindSetting::Incremental {
            self.synced_texts
                .insert(path.to_path_buf(), text.to_owned());
        }
    }

    pub(super) fn forget_synced_text(&mut self, path: &Path) {
        self.synced_texts.remove(path);
    }
}

pub(super) async fn handle_document_sync_command(
    command: LspClientCommand,
    writer: &mut ChildStdin,
    sync_state: &mut DocumentSyncState,
    ui_tx: &Sender<UiEvent>,
) -> bool {
    match command {
        command @ (LspClientCommand::DidOpen { .. } | LspClientCommand::DidChange { .. }) => {
            open_change::handle_open_change_command(command, writer, sync_state, ui_tx).await
        }
        command @ (LspClientCommand::DidSave { .. } | LspClientCommand::DidClose { .. }) => {
            save_close::handle_save_close_command(command, writer, sync_state).await
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::DocumentSyncState;
    use kuroya_core::TextDocumentSyncKindSetting;
    use std::path::{Path, PathBuf};

    fn path(value: &str) -> PathBuf {
        PathBuf::from(value)
    }

    fn synced_paths(state: &DocumentSyncState) -> Vec<&Path> {
        let mut paths: Vec<&Path> = state.synced_texts.keys().map(PathBuf::as_path).collect();
        paths.sort();
        paths
    }

    #[test]
    fn record_synced_text_tracks_only_incremental_servers() {
        let mut incremental = DocumentSyncState::new(TextDocumentSyncKindSetting::Incremental);
        incremental.record_synced_text(&path("src/main.rs"), "text one");
        assert_eq!(
            incremental
                .synced_texts
                .get(&path("src/main.rs"))
                .map(String::as_str),
            Some("text one")
        );
        incremental.record_synced_text(&path("src/main.rs"), "text two");
        assert_eq!(
            incremental
                .synced_texts
                .get(&path("src/main.rs"))
                .map(String::as_str),
            Some("text two")
        );

        for kind in [
            TextDocumentSyncKindSetting::Full,
            TextDocumentSyncKindSetting::None,
        ] {
            let mut state = DocumentSyncState::new(kind);
            state.record_synced_text(&path("src/main.rs"), "text");
            assert!(state.synced_texts.is_empty());
        }
    }

    #[test]
    fn forget_synced_text_removes_only_that_document() {
        let mut state = DocumentSyncState::new(TextDocumentSyncKindSetting::Incremental);
        state.record_synced_text(&path("src/main.rs"), "main");
        state.record_synced_text(&path("src/lib.rs"), "lib");

        state.forget_synced_text(&path("src/main.rs"));

        assert_eq!(synced_paths(&state), vec![Path::new("src/lib.rs")]);
        assert_eq!(
            state
                .synced_texts
                .get(&path("src/lib.rs"))
                .map(String::as_str),
            Some("lib")
        );
    }
}
