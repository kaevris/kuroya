use super::LspServerMessageHandlerOutcome;
use crate::{
    lsp_client::watched_files::LspWatchedFilesState, lsp_client::wire::write_message,
    lsp_ui_events::LspUiEvent, ui_event_channel::Sender, ui_events::UiEvent,
};
use kuroya_core::{
    LspRequestId, LspWireMessage, parse_apply_workspace_edit_request, parse_lsp_request_id,
};
use serde_json::{Value, json};
use std::path::Path;
use tokio::process::ChildStdin;

const JSON_RPC_METHOD_NOT_FOUND: i64 = -32601;

const REGISTER_CAPABILITY_METHOD: &str = "client/registerCapability";
const UNREGISTER_CAPABILITY_METHOD: &str = "client/unregisterCapability";
const DID_CHANGE_WATCHED_FILES_METHOD: &str = "workspace/didChangeWatchedFiles";

pub(super) async fn handle_server_request(
    value: &Value,
    language: &str,
    root: &Path,
    generation: u64,
    ui_tx: &Sender<UiEvent>,
    writer: &mut ChildStdin,
    watched_files: &LspWatchedFilesState,
) -> LspServerMessageHandlerOutcome {
    let Some(method) = value.get("method").and_then(Value::as_str) else {
        return LspServerMessageHandlerOutcome::Unhandled;
    };

    match method {
        "workspace/applyEdit" => {
            handle_apply_workspace_edit_request(value, language, root, generation, ui_tx, writer)
                .await
        }
        REGISTER_CAPABILITY_METHOD => {
            handle_register_capability_request(value, writer, watched_files).await
        }
        UNREGISTER_CAPABILITY_METHOD => {
            handle_unregister_capability_request(value, writer, watched_files).await
        }
        _ => {
            if let Some(request_id) = value.get("id").and_then(parse_lsp_request_id) {
                return if write_unsupported_server_request_response(writer, request_id).await {
                    LspServerMessageHandlerOutcome::Handled
                } else {
                    LspServerMessageHandlerOutcome::FatalWriteFailure
                };
            }
            LspServerMessageHandlerOutcome::Unhandled
        }
    }
}

async fn handle_apply_workspace_edit_request(
    value: &Value,
    language: &str,
    root: &Path,
    generation: u64,
    ui_tx: &Sender<UiEvent>,
    writer: &mut ChildStdin,
) -> LspServerMessageHandlerOutcome {
    let Some(request_id) = value.get("id").and_then(parse_lsp_request_id) else {
        return LspServerMessageHandlerOutcome::Handled;
    };

    let Some(request) = parse_apply_workspace_edit_request(value) else {
        return if write_apply_edit_response(
            writer,
            request_id,
            false,
            Some("invalid workspace/applyEdit request"),
        )
        .await
        {
            LspServerMessageHandlerOutcome::Handled
        } else {
            LspServerMessageHandlerOutcome::FatalWriteFailure
        };
    };

    let sent = crate::ui_event_channel::send_ui_event(
        ui_tx,
        UiEvent::Lsp(LspUiEvent::WorkspaceApplyEditRequest {
            language: language.to_owned(),
            root: root.to_path_buf(),
            generation,
            request_id: request.id,
            label: request.label,
            edits: Some(request.edits),
            document_changes: request.document_changes,
            document_versions: request.document_versions,
            error: None,
        }),
    );
    if !sent {
        return if write_apply_edit_response(
            writer,
            request_id,
            false,
            Some("workspace/applyEdit request queue is full"),
        )
        .await
        {
            LspServerMessageHandlerOutcome::Handled
        } else {
            LspServerMessageHandlerOutcome::FatalWriteFailure
        };
    }

    LspServerMessageHandlerOutcome::Handled
}

/// Answers `client/registerCapability` with a spec-valid success result.
/// Registrations for `workspace/didChangeWatchedFiles` additionally feed the
/// per-server watcher table so the frame loop knows which filesystem events
/// to forward. Servers (notably vscode-languageserver-node based ones) treat
/// a failure here as fatal, so every registration method gets a response.
async fn handle_register_capability_request(
    value: &Value,
    writer: &mut ChildStdin,
    watched_files: &LspWatchedFilesState,
) -> LspServerMessageHandlerOutcome {
    let Some(request_id) = value.get("id").and_then(parse_lsp_request_id) else {
        return LspServerMessageHandlerOutcome::Handled;
    };

    if let Some(registrations) = value
        .get("params")
        .and_then(|params| params.get("registrations"))
        .and_then(Value::as_array)
    {
        for registration in registrations {
            if registration.get("method").and_then(Value::as_str)
                != Some(DID_CHANGE_WATCHED_FILES_METHOD)
            {
                continue;
            }
            let Some(registration_id) = registration.get("id").and_then(Value::as_str) else {
                continue;
            };
            watched_files.register(registration_id, watcher_glob_patterns(registration));
        }
    }

    if write_message(
        writer,
        &LspWireMessage::register_capability_response(request_id).to_json(),
    )
    .await
    .is_ok()
    {
        LspServerMessageHandlerOutcome::Handled
    } else {
        LspServerMessageHandlerOutcome::FatalWriteFailure
    }
}

async fn handle_unregister_capability_request(
    value: &Value,
    writer: &mut ChildStdin,
    watched_files: &LspWatchedFilesState,
) -> LspServerMessageHandlerOutcome {
    let Some(request_id) = value.get("id").and_then(parse_lsp_request_id) else {
        return LspServerMessageHandlerOutcome::Handled;
    };

    if let Some(unregistrations) = value
        .get("params")
        .and_then(|params| params.get("unregistrations"))
        .and_then(Value::as_array)
    {
        let ids: Vec<String> = unregistrations
            .iter()
            .filter(|unregistration| {
                unregistration.get("method").and_then(Value::as_str)
                    == Some(DID_CHANGE_WATCHED_FILES_METHOD)
            })
            .filter_map(|unregistration| {
                unregistration
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .collect();
        watched_files.unregister(&ids);
    }

    if write_message(
        writer,
        &LspWireMessage::unregister_capability_response(request_id).to_json(),
    )
    .await
    .is_ok()
    {
        LspServerMessageHandlerOutcome::Handled
    } else {
        LspServerMessageHandlerOutcome::FatalWriteFailure
    }
}

/// Collects the plain string `globPattern` entries of a didChangeWatchedFiles
/// registration's watchers. Object-shaped patterns (`{ baseUri, pattern }`)
/// are skipped.
fn watcher_glob_patterns(registration: &Value) -> Vec<String> {
    registration
        .get("registerOptions")
        .and_then(|options| options.get("watchers"))
        .and_then(Value::as_array)
        .map(|watchers| {
            watchers
                .iter()
                .filter_map(|watcher| {
                    watcher
                        .get("globPattern")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .collect()
        })
        .unwrap_or_default()
}

async fn write_unsupported_server_request_response(
    writer: &mut ChildStdin,
    request_id: LspRequestId,
) -> bool {
    write_message(
        writer,
        &json!({
            "jsonrpc": "2.0",
            "id": lsp_request_id_json(&request_id),
            "error": {
                "code": JSON_RPC_METHOD_NOT_FOUND,
                "message": "Method not found",
            }
        }),
    )
    .await
    .is_ok()
}

fn lsp_request_id_json(request_id: &LspRequestId) -> Value {
    match request_id {
        LspRequestId::Number(id) => json!(id),
        LspRequestId::String(id) => json!(id),
    }
}

async fn write_apply_edit_response(
    writer: &mut ChildStdin,
    request_id: LspRequestId,
    applied: bool,
    failure_reason: Option<&str>,
) -> bool {
    write_message(
        writer,
        &LspWireMessage::apply_workspace_edit_response(request_id, applied, failure_reason, None)
            .to_json(),
    )
    .await
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::super::LspServerMessageHandlerOutcome;
    use super::handle_server_request;
    use crate::{lsp_ui_events::LspUiEvent, ui_events::UiEvent};
    use kuroya_core::{
        LspRequestId, LspWorkspaceDocumentChange, LspWorkspaceResourceOperation,
        lsp::path_to_file_uri,
    };
    use serde_json::{Value, json};
    use std::{path::PathBuf, process::Stdio};
    use tokio::{io::AsyncReadExt, process::Command};

    #[tokio::test]
    async fn workspace_apply_edit_request_sends_ui_event() -> Result<(), String> {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let value = json!({
            "jsonrpc": "2.0",
            "id": 17,
            "method": "workspace/applyEdit",
            "params": {
                "label": "Fix import",
                "edit": {
                    "changes": {
                        path_to_file_uri(&path): [{
                            "range": {
                                "start": { "line": 0, "character": 0 },
                                "end": { "line": 0, "character": 0 }
                            },
                            "newText": "use std::fs;\n"
                        }]
                    }
                }
            }
        });
        let (ui_tx, ui_rx) = crate::ui_event_channel::ui_event_channel();
        let (mut child, mut writer, mut stdout) = stdio_sink_child().await;

        assert_eq!(
            handle_server_request(
                &value,
                "rust",
                &root,
                3,
                &ui_tx,
                &mut writer,
                &watched_files()
            )
            .await,
            LspServerMessageHandlerOutcome::Handled
        );

        let UiEvent::Lsp(LspUiEvent::WorkspaceApplyEditRequest {
            language,
            root: event_root,
            generation,
            request_id,
            label,
            edits,
            document_changes,
            document_versions,
            error,
        }) = ui_rx.try_recv().map_err(|err| err.to_string())?
        else {
            return Err("expected workspace apply-edit request event".to_owned());
        };
        assert_eq!(language, "rust");
        assert_eq!(event_root, root);
        assert_eq!(generation, 3);
        assert_eq!(request_id, LspRequestId::Number(17));
        assert_eq!(label.as_deref(), Some("Fix import"));
        assert_eq!(edits.as_ref().map(Vec::len), Some(1));
        assert!(document_changes.is_empty());
        assert!(document_versions.is_empty());
        assert!(error.is_none());

        drop(writer);
        let mut output = String::new();
        stdout
            .read_to_string(&mut output)
            .await
            .map_err(|err| err.to_string())?;
        assert!(output.is_empty());
        let _ = child.kill().await;
        Ok(())
    }

    #[tokio::test]
    async fn workspace_apply_edit_request_carries_resource_operations() -> Result<(), String> {
        let root = PathBuf::from("workspace");
        let path = root.join("src/new.rs");
        let value = json!({
            "jsonrpc": "2.0",
            "id": 18,
            "method": "workspace/applyEdit",
            "params": {
                "label": "Create file",
                "edit": {
                    "documentChanges": [{
                        "kind": "create",
                        "uri": path_to_file_uri(&path),
                        "options": { "ignoreIfExists": true }
                    }]
                }
            }
        });
        let (ui_tx, ui_rx) = crate::ui_event_channel::ui_event_channel();
        let (mut child, mut writer, mut stdout) = stdio_sink_child().await;

        assert_eq!(
            handle_server_request(
                &value,
                "rust",
                &root,
                3,
                &ui_tx,
                &mut writer,
                &watched_files()
            )
            .await,
            LspServerMessageHandlerOutcome::Handled
        );

        let UiEvent::Lsp(LspUiEvent::WorkspaceApplyEditRequest {
            document_changes,
            edits,
            ..
        }) = ui_rx.try_recv().map_err(|err| err.to_string())?
        else {
            return Err("expected workspace apply-edit request event".to_owned());
        };
        assert_eq!(edits.as_ref().map(Vec::len), Some(0));
        assert!(matches!(
            &document_changes[0],
            LspWorkspaceDocumentChange::Resource(LspWorkspaceResourceOperation::CreateFile {
                ignore_if_exists: true,
                ..
            })
        ));

        drop(writer);
        let mut output = String::new();
        stdout
            .read_to_string(&mut output)
            .await
            .map_err(|err| err.to_string())?;
        assert!(output.is_empty());
        let _ = child.kill().await;
        Ok(())
    }

    #[tokio::test]
    async fn workspace_apply_edit_request_prefers_document_changes_over_changes()
    -> Result<(), String> {
        let root = PathBuf::from("workspace");
        let ignored_path = root.join("src/ignored.rs");
        let created_path = root.join("src/new.rs");
        let value = json!({
            "jsonrpc": "2.0",
            "id": 19,
            "method": "workspace/applyEdit",
            "params": {
                "label": "Create file",
                "edit": {
                    "changes": {
                        path_to_file_uri(&ignored_path): [{
                            "range": {
                                "start": { "line": 0, "character": 0 },
                                "end": { "line": 0, "character": 0 }
                            },
                            "newText": "ignored\n"
                        }]
                    },
                    "documentChanges": [
                        {
                            "kind": "create",
                            "uri": path_to_file_uri(&created_path)
                        }
                    ]
                }
            }
        });
        let (ui_tx, ui_rx) = crate::ui_event_channel::ui_event_channel();
        let (mut child, mut writer, mut stdout) = stdio_sink_child().await;

        assert_eq!(
            handle_server_request(
                &value,
                "rust",
                &root,
                3,
                &ui_tx,
                &mut writer,
                &watched_files()
            )
            .await,
            LspServerMessageHandlerOutcome::Handled
        );

        let UiEvent::Lsp(LspUiEvent::WorkspaceApplyEditRequest {
            document_changes,
            edits,
            ..
        }) = ui_rx.try_recv().map_err(|err| err.to_string())?
        else {
            return Err("expected workspace apply-edit request event".to_owned());
        };
        assert_eq!(edits.as_ref().map(Vec::len), Some(0));
        assert_eq!(document_changes.len(), 1);
        assert!(matches!(
            &document_changes[0],
            LspWorkspaceDocumentChange::Resource(LspWorkspaceResourceOperation::CreateFile { .. })
        ));

        drop(writer);
        let mut output = String::new();
        stdout
            .read_to_string(&mut output)
            .await
            .map_err(|err| err.to_string())?;
        assert!(output.is_empty());
        let _ = child.kill().await;
        Ok(())
    }

    #[tokio::test]
    async fn unsupported_server_request_with_numeric_id_writes_method_not_found()
    -> Result<(), String> {
        let response = unsupported_request_response_for(json!(91)).await?;

        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 91);
        assert_eq!(response["error"]["code"], -32601);
        assert_eq!(response["error"]["message"], "Method not found");
        Ok(())
    }

    #[tokio::test]
    async fn unsupported_server_request_with_string_id_writes_method_not_found()
    -> Result<(), String> {
        let response = unsupported_request_response_for(json!("config-92")).await?;

        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], "config-92");
        assert_eq!(response["error"]["code"], -32601);
        assert_eq!(response["error"]["message"], "Method not found");
        Ok(())
    }

    #[tokio::test]
    async fn unsupported_server_notification_is_ignored() -> Result<(), String> {
        let root = PathBuf::from("workspace");
        let value = json!({
            "jsonrpc": "2.0",
            "method": "workspace/configuration",
            "params": {}
        });
        let (ui_tx, ui_rx) = crate::ui_event_channel::ui_event_channel();
        let (mut child, mut writer, mut stdout) = stdio_sink_child().await;

        assert_eq!(
            handle_server_request(
                &value,
                "rust",
                &root,
                3,
                &ui_tx,
                &mut writer,
                &watched_files()
            )
            .await,
            LspServerMessageHandlerOutcome::Unhandled
        );
        assert!(ui_rx.try_recv().is_err());

        drop(writer);
        let mut output = String::new();
        stdout
            .read_to_string(&mut output)
            .await
            .map_err(|err| err.to_string())?;
        assert!(output.is_empty());
        let _ = child.kill().await;
        Ok(())
    }

    #[tokio::test]
    async fn register_capability_request_records_watchers_and_replies_success() -> Result<(), String>
    {
        let root = PathBuf::from("workspace");
        let value = json!({
            "jsonrpc": "2.0",
            "id": 23,
            "method": "client/registerCapability",
            "params": {
                "registrations": [{
                    "id": "watcher-rs",
                    "method": "workspace/didChangeWatchedFiles",
                    "registerOptions": {
                        "watchers": [
                            { "globPattern": "**/*.rs" },
                            { "globPattern": { "baseUri": "file:///workspace", "pattern": "**/*.ignored" } }
                        ]
                    }
                }]
            }
        });
        let state = watched_files();
        let (ui_tx, _ui_rx) = crate::ui_event_channel::ui_event_channel();
        let (mut child, mut writer, mut stdout) = stdio_echo_child().await;

        assert_eq!(
            handle_server_request(&value, "rust", &root, 3, &ui_tx, &mut writer, &state).await,
            LspServerMessageHandlerOutcome::Handled
        );

        drop(writer);
        let mut output = String::new();
        stdout
            .read_to_string(&mut output)
            .await
            .map_err(|err| err.to_string())?;
        let _ = child.kill().await;
        let response = lsp_response_body(&output)?;
        assert_eq!(response["id"], 23);
        assert_eq!(response["result"]["capabilities"], json!({}));

        // String glob patterns are tracked; object-shaped ones are ignored.
        assert!(state.matches_any(&root.join("src/main.rs")));
        assert!(!state.matches_any(&root.join("src/main.ts")));
        Ok(())
    }

    #[tokio::test]
    async fn unregister_capability_request_stops_watcher_forwarding() -> Result<(), String> {
        let root = PathBuf::from("workspace");
        let state = watched_files();
        state.register("watcher-rs", vec!["**/*.rs".to_owned()]);
        let value = json!({
            "jsonrpc": "2.0",
            "id": "unregister-24",
            "method": "client/unregisterCapability",
            "params": {
                "unregistrations": [{
                    "id": "watcher-rs",
                    "method": "workspace/didChangeWatchedFiles"
                }]
            }
        });
        let (ui_tx, _ui_rx) = crate::ui_event_channel::ui_event_channel();
        let (mut child, mut writer, mut stdout) = stdio_echo_child().await;

        assert_eq!(
            handle_server_request(&value, "rust", &root, 3, &ui_tx, &mut writer, &state).await,
            LspServerMessageHandlerOutcome::Handled
        );

        drop(writer);
        let mut output = String::new();
        stdout
            .read_to_string(&mut output)
            .await
            .map_err(|err| err.to_string())?;
        let _ = child.kill().await;
        let response = lsp_response_body(&output)?;
        assert_eq!(response["id"], "unregister-24");
        assert_eq!(response["result"]["capabilities"], json!({}));
        assert!(!state.matches_any(&root.join("src/main.rs")));
        Ok(())
    }

    #[tokio::test]
    async fn register_capability_request_for_other_methods_replies_success_without_watchers()
    -> Result<(), String> {
        let root = PathBuf::from("workspace");
        let value = json!({
            "jsonrpc": "2.0",
            "id": 25,
            "method": "client/registerCapability",
            "params": {
                "registrations": [{
                    "id": "config-1",
                    "method": "workspace/didChangeConfiguration",
                    "registerOptions": {}
                }]
            }
        });
        let state = watched_files();
        let (ui_tx, _ui_rx) = crate::ui_event_channel::ui_event_channel();
        let (mut child, mut writer, mut stdout) = stdio_echo_child().await;

        assert_eq!(
            handle_server_request(&value, "rust", &root, 3, &ui_tx, &mut writer, &state).await,
            LspServerMessageHandlerOutcome::Handled
        );

        drop(writer);
        let mut output = String::new();
        stdout
            .read_to_string(&mut output)
            .await
            .map_err(|err| err.to_string())?;
        let _ = child.kill().await;
        let response = lsp_response_body(&output)?;
        assert_eq!(response["id"], 25);
        assert_eq!(response["result"]["capabilities"], json!({}));
        assert!(state.is_empty());
        Ok(())
    }

    fn watched_files() -> crate::lsp_client::watched_files::LspWatchedFilesState {
        crate::lsp_client::watched_files::LspWatchedFilesState::default()
    }

    async fn unsupported_request_response_for(id: Value) -> Result<Value, String> {
        let root = PathBuf::from("workspace");
        let value = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "workspace/configuration",
            "params": []
        });
        let (ui_tx, ui_rx) = crate::ui_event_channel::ui_event_channel();
        let (mut child, mut writer, mut stdout) = stdio_echo_child().await;

        assert_eq!(
            handle_server_request(
                &value,
                "rust",
                &root,
                3,
                &ui_tx,
                &mut writer,
                &watched_files()
            )
            .await,
            LspServerMessageHandlerOutcome::Handled
        );
        assert!(ui_rx.try_recv().is_err());

        drop(writer);
        let mut output = String::new();
        stdout
            .read_to_string(&mut output)
            .await
            .map_err(|err| err.to_string())?;
        let _ = child.kill().await;
        lsp_response_body(&output)
    }

    fn lsp_response_body(output: &str) -> Result<Value, String> {
        let body = output
            .split_once("\r\n\r\n")
            .or_else(|| output.split_once("\n\n"))
            .map(|(_, body)| body.trim())
            .ok_or_else(|| format!("missing LSP message separator in {output:?}"))?;
        serde_json::from_str(body).map_err(|err| err.to_string())
    }

    async fn stdio_echo_child() -> (
        tokio::process::Child,
        tokio::process::ChildStdin,
        tokio::process::ChildStdout,
    ) {
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("cmd");
            command.args(["/C", "more"]);
            command
        };

        #[cfg(not(windows))]
        let mut command = Command::new("cat");

        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn stdin echo child process");
        let stdin = child.stdin.take().expect("child stdin is piped");
        let stdout = child.stdout.take().expect("child stdout is piped");
        (child, stdin, stdout)
    }

    async fn stdio_sink_child() -> (
        tokio::process::Child,
        tokio::process::ChildStdin,
        tokio::process::ChildStdout,
    ) {
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("cmd");
            command.args(["/C", "more > NUL"]);
            command
        };

        #[cfg(not(windows))]
        let mut command = {
            let mut command = Command::new("sh");
            command.args(["-c", "cat >/dev/null"]);
            command
        };

        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn stdin sink child process");
        let stdin = child.stdin.take().expect("child stdin is piped");
        let stdout = child.stdout.take().expect("child stdout is piped");
        (child, stdin, stdout)
    }
}
