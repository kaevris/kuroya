mod dispatch;
mod pending;

use crate::lsp_client::pending::PendingLspRequests;
use dispatch::{dispatch_code_lens_resolve, dispatch_code_lenses, dispatch_execute_command};
use kuroya_core::{BufferId, LspCodeLens};
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};
use tokio::process::ChildStdin;

pub(super) async fn dispatch_code_lenses_request(
    id: BufferId,
    path: PathBuf,
    version: u64,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    dispatch_code_lenses(id, path, version, writer, next_request_id, pending_requests).await
}

pub(super) async fn dispatch_code_lens_resolve_request(
    id: BufferId,
    path: PathBuf,
    version: u64,
    lens: LspCodeLens,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    dispatch_code_lens_resolve(
        id,
        path,
        version,
        lens,
        writer,
        next_request_id,
        pending_requests,
    )
    .await
}

pub(super) async fn dispatch_execute_command_request(
    id: BufferId,
    path: PathBuf,
    version: u64,
    title: String,
    command: String,
    arguments: Option<Arc<Value>>,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    dispatch_execute_command(
        id,
        path,
        version,
        title,
        command,
        arguments,
        writer,
        next_request_id,
        pending_requests,
    )
    .await
}
