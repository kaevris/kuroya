mod dispatch;

use super::DocumentSyncState;
use crate::ui_event_channel::Sender;
use crate::{lsp_client::commands::LspClientCommand, ui_events::UiEvent};
use dispatch::{dispatch_did_change, dispatch_did_open};
use tokio::process::ChildStdin;

pub(super) async fn handle_open_change_command(
    command: LspClientCommand,
    writer: &mut ChildStdin,
    sync_state: &mut DocumentSyncState,
    ui_tx: &Sender<UiEvent>,
) -> bool {
    match command {
        LspClientCommand::DidOpen {
            id,
            path,
            language,
            version,
            text,
        } => dispatch_did_open(id, path, language, version, text, sync_state, writer, ui_tx).await,
        LspClientCommand::DidChange {
            id,
            path,
            version,
            text,
        } => dispatch_did_change(id, path, version, text, sync_state, writer, ui_tx).await,
        _ => true,
    }
}
