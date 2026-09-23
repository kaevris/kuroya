mod result;

use crate::ui_event_channel::Sender;
use crate::ui_events::UiEvent;
use kuroya_core::BufferId;
use result::{send_prepare_rename_result, send_rename_result};
use serde_json::Value;
use std::path::PathBuf;

pub(super) fn handle_rename_response(
    id: BufferId,
    path: PathBuf,
    version: u64,
    line: usize,
    character: usize,
    new_name: String,
    value: &Value,
    ui_tx: &Sender<UiEvent>,
) {
    send_rename_result(id, path, version, line, character, new_name, value, ui_tx);
}

pub(super) fn handle_prepare_rename_response(
    id: BufferId,
    path: PathBuf,
    version: u64,
    line: usize,
    character: usize,
    value: &Value,
    ui_tx: &Sender<UiEvent>,
) {
    send_prepare_rename_result(id, path, version, line, character, value, ui_tx);
}
