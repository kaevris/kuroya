use super::LspServerMessageHandlerOutcome;
use crate::{
    lsp_client::wire::write_message, lsp_ui_events::LspUiEvent, ui_event_channel::Sender,
    ui_events::UiEvent, workspace_state::workspace_event_matches,
};
use kuroya_core::{
    LspRequestId, LspWireMessage, LspWorkDoneProgressKind, parse_work_done_progress,
    parse_work_done_progress_create,
};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
use tokio::process::ChildStdin;

const MAX_CREATED_WORK_DONE_PROGRESS_TOKENS_PER_SERVER: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkDoneProgressTokenServer {
    language: String,
    root: PathBuf,
    generation: u64,
    tokens: HashSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkDoneProgressTokenCreate {
    Created,
    Duplicate,
    TooMany,
}

impl WorkDoneProgressTokenServer {
    fn new(language: &str, root: &Path, generation: u64, token: String) -> Self {
        let mut tokens = HashSet::with_capacity(1);
        tokens.insert(token);
        Self {
            language: language.to_owned(),
            root: root.to_path_buf(),
            generation,
            tokens,
        }
    }

    fn same_server(&self, language: &str, root: &Path, generation: u64) -> bool {
        self.language == language
            && workspace_event_matches(&self.root, root)
            && self.generation == generation
    }
}

#[derive(Debug, Default)]
struct WorkDoneProgressTokenStore {
    servers: Vec<WorkDoneProgressTokenServer>,
}

impl WorkDoneProgressTokenStore {
    fn remember(
        &mut self,
        language: &str,
        root: &Path,
        generation: u64,
        token: &str,
    ) -> WorkDoneProgressTokenCreate {
        self.prune_stale_generations(language, root, generation);

        if let Some(server) = self
            .servers
            .iter_mut()
            .find(|server| server.same_server(language, root, generation))
        {
            if server.tokens.contains(token) {
                return WorkDoneProgressTokenCreate::Duplicate;
            }
            if server.tokens.len() >= MAX_CREATED_WORK_DONE_PROGRESS_TOKENS_PER_SERVER {
                return WorkDoneProgressTokenCreate::TooMany;
            }
            server.tokens.insert(token.to_owned());
            return WorkDoneProgressTokenCreate::Created;
        }

        self.servers.push(WorkDoneProgressTokenServer::new(
            language,
            root,
            generation,
            token.to_owned(),
        ));
        WorkDoneProgressTokenCreate::Created
    }

    fn forget(&mut self, language: &str, root: &Path, generation: u64, token: &str) {
        let Some(index) = self
            .servers
            .iter()
            .position(|server| server.same_server(language, root, generation))
        else {
            return;
        };
        self.servers[index].tokens.remove(token);
        if self.servers[index].tokens.is_empty() {
            self.servers.swap_remove(index);
        }
    }

    fn forget_server(&mut self, language: &str, root: &Path, generation: u64) {
        self.servers
            .retain(|server| !server.same_server(language, root, generation));
    }

    fn prune_stale_generations(&mut self, language: &str, root: &Path, generation: u64) {
        self.servers.retain(|server| {
            server.language != language
                || !workspace_event_matches(&server.root, root)
                || server.generation == generation
        });
    }
}

fn created_work_done_progress_tokens() -> &'static Mutex<WorkDoneProgressTokenStore> {
    static TOKENS: OnceLock<Mutex<WorkDoneProgressTokenStore>> = OnceLock::new();
    TOKENS.get_or_init(|| Mutex::new(WorkDoneProgressTokenStore::default()))
}

fn remember_created_work_done_progress_token(
    language: &str,
    root: &Path,
    generation: u64,
    token: &str,
) -> WorkDoneProgressTokenCreate {
    let Ok(mut tokens) = created_work_done_progress_tokens().lock() else {
        return WorkDoneProgressTokenCreate::TooMany;
    };
    tokens.remember(language, root, generation, token)
}

fn forget_created_work_done_progress_token(
    language: &str,
    root: &Path,
    generation: u64,
    token: &str,
) {
    if let Ok(mut tokens) = created_work_done_progress_tokens().lock() {
        tokens.forget(language, root, generation, token);
    }
}

pub(in crate::lsp_client::runtime) fn forget_created_work_done_progress_tokens_for_server(
    language: &str,
    root: &Path,
    generation: u64,
) {
    if let Ok(mut tokens) = created_work_done_progress_tokens().lock() {
        tokens.forget_server(language, root, generation);
    }
}

fn request_id_json(id: &LspRequestId) -> Value {
    match id {
        LspRequestId::Number(id) => json!(id),
        LspRequestId::String(id) => json!(id),
    }
}

fn work_done_progress_create_error_response(id: &LspRequestId, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": request_id_json(id),
        "error": {
            "code": -32602,
            "message": message
        }
    })
}

pub(super) fn send_work_done_progress(
    value: &Value,
    language: &str,
    root: &Path,
    generation: u64,
    ui_tx: &Sender<UiEvent>,
) -> bool {
    let Some(progress) = parse_work_done_progress(value) else {
        return false;
    };
    let is_end = progress.kind == LspWorkDoneProgressKind::End;
    if is_end {
        forget_created_work_done_progress_token(language, root, generation, &progress.token);
    }

    let _ = crate::ui_event_channel::send_ui_event(
        ui_tx,
        UiEvent::Lsp(LspUiEvent::WorkDoneProgress {
            language: language.to_owned(),
            root: root.to_path_buf(),
            generation,
            progress,
        }),
    );
    true
}

pub(super) async fn acknowledge_work_done_progress_create(
    value: &Value,
    language: &str,
    root: &Path,
    generation: u64,
    ui_tx: &Sender<UiEvent>,
    writer: &mut ChildStdin,
) -> LspServerMessageHandlerOutcome {
    let Some(request) = parse_work_done_progress_create(value) else {
        return LspServerMessageHandlerOutcome::Unhandled;
    };

    let create =
        remember_created_work_done_progress_token(language, root, generation, &request.token);
    let response = match create {
        WorkDoneProgressTokenCreate::Created => {
            LspWireMessage::response(request.id.clone(), json!(null)).to_json()
        }
        WorkDoneProgressTokenCreate::Duplicate => work_done_progress_create_error_response(
            &request.id,
            "workDoneProgress token already exists",
        ),
        WorkDoneProgressTokenCreate::TooMany => work_done_progress_create_error_response(
            &request.id,
            "too many active workDoneProgress tokens",
        ),
    };

    let wrote_response = write_message(writer, &response).await.is_ok();
    if !wrote_response {
        if create == WorkDoneProgressTokenCreate::Created {
            forget_created_work_done_progress_token(language, root, generation, &request.token);
        }
        return LspServerMessageHandlerOutcome::FatalWriteFailure;
    }
    if create == WorkDoneProgressTokenCreate::Created {
        let _ = crate::ui_event_channel::send_ui_event(
            ui_tx,
            UiEvent::Lsp(LspUiEvent::WorkDoneProgressCreated {
                token: request.token,
            }),
        );
    }
    LspServerMessageHandlerOutcome::Handled
}

#[cfg(test)]
mod tests {
    use super::super::LspServerMessageHandlerOutcome;
    use super::{
        MAX_CREATED_WORK_DONE_PROGRESS_TOKENS_PER_SERVER, WorkDoneProgressTokenCreate,
        acknowledge_work_done_progress_create, forget_created_work_done_progress_token,
        forget_created_work_done_progress_tokens_for_server,
        remember_created_work_done_progress_token, send_work_done_progress,
    };
    use crate::{lsp_ui_events::LspUiEvent, ui_events::UiEvent};
    use kuroya_core::LspWorkDoneProgressKind;
    use serde_json::json;
    use std::{path::PathBuf, process::Stdio};
    use tokio::{io::AsyncReadExt, process::Command};

    #[test]
    fn sends_work_done_progress_ui_events() -> Result<(), String> {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let value = json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": {
                "token": "token",
                "value": {
                    "kind": "begin",
                    "title": "Indexing"
                }
            }
        });

        assert!(send_work_done_progress(
            &value,
            "rust",
            &PathBuf::from("workspace"),
            7,
            &tx
        ));
        let event = rx.try_recv().map_err(|err| err.to_string())?;

        let UiEvent::Lsp(LspUiEvent::WorkDoneProgress {
            language,
            root,
            generation,
            progress,
        }) = event
        else {
            return Err("expected LSP work-done progress event".to_owned());
        };
        assert_eq!(language, "rust");
        assert_eq!(root, PathBuf::from("workspace"));
        assert_eq!(generation, 7);
        assert_eq!(progress.token, "token");
        assert_eq!(progress.kind, LspWorkDoneProgressKind::Begin);
        assert_eq!(progress.title.as_deref(), Some("Indexing"));
        Ok(())
    }

    #[test]
    fn work_done_progress_end_retires_created_tokens() {
        let (tx, _rx) = crate::ui_event_channel::ui_event_channel();
        let root = PathBuf::from("workspace-retire-progress-token");
        assert_eq!(
            remember_created_work_done_progress_token("rust", &root, 7, "token"),
            WorkDoneProgressTokenCreate::Created
        );
        assert_eq!(
            remember_created_work_done_progress_token("rust", &root, 7, "token"),
            WorkDoneProgressTokenCreate::Duplicate
        );

        let end = json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": {
                "token": "token",
                "value": {
                    "kind": "end"
                }
            }
        });
        assert!(send_work_done_progress(&end, "rust", &root, 7, &tx));

        assert_eq!(
            remember_created_work_done_progress_token("rust", &root, 7, "token"),
            WorkDoneProgressTokenCreate::Created
        );
        forget_created_work_done_progress_token("rust", &root, 7, "token");
    }

    #[test]
    fn work_done_progress_tokens_match_equivalent_roots() {
        let root = PathBuf::from("workspace-progress-equivalent-root");
        let equivalent_root = root.join("src").join("..");
        assert_eq!(
            remember_created_work_done_progress_token("rust", &equivalent_root, 7, "token"),
            WorkDoneProgressTokenCreate::Created
        );
        assert_eq!(
            remember_created_work_done_progress_token("rust", &root, 7, "token"),
            WorkDoneProgressTokenCreate::Duplicate
        );

        forget_created_work_done_progress_token("rust", &root, 7, "token");
        assert_eq!(
            remember_created_work_done_progress_token("rust", &equivalent_root, 7, "token"),
            WorkDoneProgressTokenCreate::Created
        );
        forget_created_work_done_progress_token("rust", &equivalent_root, 7, "token");
    }

    #[test]
    fn work_done_progress_tokens_prune_equivalent_stale_generation_roots() {
        let root = PathBuf::from("workspace-progress-prune-equivalent-root");
        let equivalent_root = root.join("src").join("..");
        assert_eq!(
            remember_created_work_done_progress_token("rust", &equivalent_root, 6, "token"),
            WorkDoneProgressTokenCreate::Created
        );
        assert_eq!(
            remember_created_work_done_progress_token("rust", &root, 7, "token"),
            WorkDoneProgressTokenCreate::Created
        );
        assert_eq!(
            remember_created_work_done_progress_token("rust", &equivalent_root, 6, "token"),
            WorkDoneProgressTokenCreate::Created
        );

        forget_created_work_done_progress_token("rust", &equivalent_root, 6, "token");
        forget_created_work_done_progress_token("rust", &root, 7, "token");
    }

    #[test]
    fn work_done_progress_server_cleanup_retires_only_matching_tokens() {
        let root = PathBuf::from("workspace-progress-server-cleanup");
        let equivalent_root = root.join("src").join("..");
        let other_root = PathBuf::from("other-workspace-progress-server-cleanup");
        assert_eq!(
            remember_created_work_done_progress_token("rust", &equivalent_root, 7, "token-a"),
            WorkDoneProgressTokenCreate::Created
        );
        assert_eq!(
            remember_created_work_done_progress_token("rust", &root, 7, "token-b"),
            WorkDoneProgressTokenCreate::Created
        );
        assert_eq!(
            remember_created_work_done_progress_token("rust", &root, 8, "token-c"),
            WorkDoneProgressTokenCreate::Created
        );
        assert_eq!(
            remember_created_work_done_progress_token("typescript", &root, 7, "token-d"),
            WorkDoneProgressTokenCreate::Created
        );
        assert_eq!(
            remember_created_work_done_progress_token("rust", &other_root, 7, "token-e"),
            WorkDoneProgressTokenCreate::Created
        );

        forget_created_work_done_progress_tokens_for_server("rust", &root, 7);

        assert_eq!(
            remember_created_work_done_progress_token("rust", &root, 8, "token-c"),
            WorkDoneProgressTokenCreate::Duplicate
        );
        assert_eq!(
            remember_created_work_done_progress_token("typescript", &root, 7, "token-d"),
            WorkDoneProgressTokenCreate::Duplicate
        );
        assert_eq!(
            remember_created_work_done_progress_token("rust", &other_root, 7, "token-e"),
            WorkDoneProgressTokenCreate::Duplicate
        );
        assert_eq!(
            remember_created_work_done_progress_token("rust", &root, 7, "token-a"),
            WorkDoneProgressTokenCreate::Created
        );
        assert_eq!(
            remember_created_work_done_progress_token("rust", &equivalent_root, 7, "token-b"),
            WorkDoneProgressTokenCreate::Created
        );

        forget_created_work_done_progress_token("rust", &root, 7, "token-a");
        forget_created_work_done_progress_token("rust", &root, 7, "token-b");
        forget_created_work_done_progress_token("rust", &root, 8, "token-c");
        forget_created_work_done_progress_token("typescript", &root, 7, "token-d");
        forget_created_work_done_progress_token("rust", &other_root, 7, "token-e");
    }

    #[test]
    fn work_done_progress_token_limit_is_enforced_per_server() {
        let root = PathBuf::from("workspace-progress-token-limit");
        let other_root = PathBuf::from("workspace-progress-token-limit-other");
        forget_created_work_done_progress_tokens_for_server("rust", &root, 7);
        forget_created_work_done_progress_tokens_for_server("rust", &other_root, 7);

        for index in 0..MAX_CREATED_WORK_DONE_PROGRESS_TOKENS_PER_SERVER {
            assert_eq!(
                remember_created_work_done_progress_token(
                    "rust",
                    &root,
                    7,
                    &format!("token-{index}")
                ),
                WorkDoneProgressTokenCreate::Created
            );
        }
        assert_eq!(
            remember_created_work_done_progress_token("rust", &root, 7, "token-0"),
            WorkDoneProgressTokenCreate::Duplicate
        );
        assert_eq!(
            remember_created_work_done_progress_token("rust", &root, 7, "token-overflow"),
            WorkDoneProgressTokenCreate::TooMany
        );
        assert_eq!(
            remember_created_work_done_progress_token("rust", &other_root, 7, "token-overflow"),
            WorkDoneProgressTokenCreate::Created
        );

        forget_created_work_done_progress_tokens_for_server("rust", &root, 7);
        forget_created_work_done_progress_tokens_for_server("rust", &other_root, 7);
    }

    #[tokio::test]
    async fn acknowledges_work_done_progress_create_with_string_request_ids() -> Result<(), String>
    {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let value = json!({
            "jsonrpc": "2.0",
            "id": "progress-create-9",
            "method": "window/workDoneProgress/create",
            "params": {
                "token": "cargo-check"
            }
        });
        let (mut child, mut writer, mut stdout) = stdio_echo_child().await;

        let root = PathBuf::from("workspace");
        assert_eq!(
            acknowledge_work_done_progress_create(&value, "rust", &root, 7, &tx, &mut writer).await,
            LspServerMessageHandlerOutcome::Handled
        );

        let UiEvent::Lsp(LspUiEvent::WorkDoneProgressCreated { token }) =
            rx.try_recv().map_err(|err| err.to_string())?
        else {
            return Err("expected work-done progress created event".to_owned());
        };
        assert_eq!(token, "cargo-check");

        drop(writer);
        let mut output = String::new();
        stdout
            .read_to_string(&mut output)
            .await
            .map_err(|err| err.to_string())?;
        assert!(output.contains("Content-Length:"));
        assert!(output.contains(r#""id":"progress-create-9""#));
        assert!(output.contains(r#""result":null"#));
        forget_created_work_done_progress_token("rust", &root, 7, "cargo-check");
        let _ = child.kill().await;
        Ok(())
    }

    #[tokio::test]
    async fn rejects_duplicate_work_done_progress_create_tokens() -> Result<(), String> {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let root = PathBuf::from("workspace-duplicate-progress-token");
        let first = json!({
            "jsonrpc": "2.0",
            "id": 11,
            "method": "window/workDoneProgress/create",
            "params": {
                "token": "cargo-check"
            }
        });
        let duplicate = json!({
            "jsonrpc": "2.0",
            "id": 12,
            "method": "window/workDoneProgress/create",
            "params": {
                "token": "cargo-check"
            }
        });
        let (mut child, mut writer, mut stdout) = stdio_echo_child().await;

        assert_eq!(
            acknowledge_work_done_progress_create(&first, "rust", &root, 7, &tx, &mut writer).await,
            LspServerMessageHandlerOutcome::Handled
        );
        assert_eq!(
            acknowledge_work_done_progress_create(&duplicate, "rust", &root, 7, &tx, &mut writer)
                .await,
            LspServerMessageHandlerOutcome::Handled
        );

        let UiEvent::Lsp(LspUiEvent::WorkDoneProgressCreated { token }) =
            rx.try_recv().map_err(|err| err.to_string())?
        else {
            return Err("expected first work-done progress created event".to_owned());
        };
        assert_eq!(token, "cargo-check");
        assert!(rx.try_recv().is_err());

        drop(writer);
        let mut output = String::new();
        stdout
            .read_to_string(&mut output)
            .await
            .map_err(|err| err.to_string())?;
        assert!(output.contains(r#""id":11"#));
        assert!(output.contains(r#""result":null"#));
        assert!(output.contains(r#""id":12"#));
        assert!(output.contains(r#""error""#));
        assert!(output.contains("workDoneProgress token already exists"));

        forget_created_work_done_progress_token("rust", &root, 7, "cargo-check");
        let _ = child.kill().await;
        Ok(())
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
}
