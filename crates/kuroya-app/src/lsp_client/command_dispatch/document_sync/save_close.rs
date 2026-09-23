use super::DocumentSyncState;
use crate::lsp_client::{commands::LspClientCommand, wire::write_message};
use kuroya_core::LspWireMessage;
use tokio::process::ChildStdin;

pub(super) async fn handle_save_close_command(
    command: LspClientCommand,
    writer: &mut ChildStdin,
    sync_state: &mut DocumentSyncState,
) -> bool {
    let (message, closed_path) = match command {
        LspClientCommand::DidSave { path } => (LspWireMessage::did_save(&path).to_json(), None),
        LspClientCommand::DidClose { path } => {
            (LspWireMessage::did_close(&path).to_json(), Some(path))
        }
        _ => return true,
    };

    let write_ok = write_message(writer, &message).await.is_ok();
    if write_ok {
        // didClose ends tracking of what the server holds for the document.
        if let Some(path) = closed_path {
            sync_state.forget_synced_text(&path);
        }
    }
    write_ok
}
