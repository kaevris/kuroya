use super::commands::LspClientCommand;
use super::stderr_log::LspStderrLog;
use super::watched_files::LspWatchedFilesState;
use crate::ui_event_channel::Sender as UiSender;
use crate::ui_events::UiEvent;
use kuroya_core::{LspRequestId, LspServerConfig};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::{
    runtime::Runtime,
    sync::{
        mpsc::{self, Sender as CommandSender},
        watch::{self, Sender as ShutdownSender},
    },
};

pub(crate) const LSP_COMMAND_QUEUE_CAPACITY: usize = 1024;
static NEXT_LSP_CLIENT_GENERATION: AtomicU64 = AtomicU64::new(1);

const MAX_TRACKED_OPEN_DOCUMENTS_PER_CLIENT: usize = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct LspServerCapabilities {
    pub(crate) rename_provider: bool,

    pub(crate) prepare_rename_supported: bool,
    pub(crate) semantic_tokens_provider: bool,
    pub(crate) inlay_hint_provider: bool,
    pub(crate) code_lens_provider: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct LspServerCapabilitiesState {
    inner: Arc<Mutex<Option<LspServerCapabilities>>>,
}

impl LspServerCapabilitiesState {
    pub(crate) fn set(&self, capabilities: LspServerCapabilities) {
        if let Ok(mut slot) = self.inner.lock() {
            *slot = Some(capabilities);
        }
    }

    pub(crate) fn get(&self) -> Option<LspServerCapabilities> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .copied()
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct LspOpenDocumentsState {
    inner: Arc<Mutex<HashSet<PathBuf>>>,
}

impl LspOpenDocumentsState {
    pub(crate) fn note_open(&self, path: &Path) {
        if let Ok(mut documents) = self.inner.lock()
            && documents.len() < MAX_TRACKED_OPEN_DOCUMENTS_PER_CLIENT
        {
            documents.insert(path.to_path_buf());
        }
    }

    pub(crate) fn note_closed(&self, path: &Path) {
        if let Ok(mut documents) = self.inner.lock() {
            documents.remove(path);
        }
    }

    pub(crate) fn contains(&self, path: &Path) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains(path)
    }

    pub(crate) fn len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone)]
pub struct LspClientHandle {
    pub(super) tx: CommandSender<LspClientCommand>,
    shutdown_tx: ShutdownSender<bool>,

    #[allow(dead_code)]
    pub(super) stderr_log: LspStderrLog,

    pub(super) watched_files: LspWatchedFilesState,

    pub(super) capabilities: LspServerCapabilitiesState,

    pub(super) open_documents: LspOpenDocumentsState,
    pub(super) generation: u64,
}

impl LspClientHandle {
    pub fn spawn_on(
        runtime: &Runtime,
        config: LspServerConfig,
        root: PathBuf,
        ui_tx: UiSender<UiEvent>,
    ) -> Self {
        let (tx, rx) = mpsc::channel(LSP_COMMAND_QUEUE_CAPACITY);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let generation = NEXT_LSP_CLIENT_GENERATION.fetch_add(1, Ordering::Relaxed);
        let stderr_log = LspStderrLog::default();
        let watched_files = LspWatchedFilesState::default();
        let capabilities = LspServerCapabilitiesState::default();
        let handle = Self {
            tx,
            shutdown_tx,
            stderr_log: stderr_log.clone(),
            watched_files: watched_files.clone(),
            capabilities: capabilities.clone(),
            open_documents: LspOpenDocumentsState::default(),
            generation,
        };

        runtime.spawn(async move {
            super::runtime::run_lsp_client(
                generation,
                config,
                root,
                rx,
                shutdown_rx,
                ui_tx,
                stderr_log,
                watched_files,
                capabilities,
            )
            .await;
        });

        handle
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn capabilities(&self) -> Option<LspServerCapabilities> {
        self.capabilities.get()
    }

    pub(crate) fn open_documents(&self) -> &LspOpenDocumentsState {
        &self.open_documents
    }

    #[cfg(test)]
    pub(crate) fn set_capabilities_for_test(&self, capabilities: LspServerCapabilities) {
        self.capabilities.set(capabilities);
    }

    pub fn shutdown(&self) -> bool {
        let signaled = self.shutdown_tx.send(true).is_ok();
        let queued = self.queue_command(LspClientCommand::Shutdown);
        signaled || queued
    }

    pub fn apply_workspace_edit_response(
        &self,
        request_id: LspRequestId,
        applied: bool,
        failure_reason: Option<String>,
    ) -> bool {
        self.queue_command(LspClientCommand::ApplyWorkspaceEditResponse {
            request_id,
            applied,
            failure_reason,
        })
    }

    pub(super) fn queue_command(&self, command: LspClientCommand) -> bool {
        self.tx.try_send(command).is_ok()
    }

    #[cfg(test)]
    pub(crate) fn disconnected_for_test() -> Self {
        Self::disconnected_with_generation_for_test(1)
    }

    #[cfg(test)]
    pub(crate) fn disconnected_with_generation_for_test(generation: u64) -> Self {
        let (tx, _rx) = mpsc::channel(LSP_COMMAND_QUEUE_CAPACITY);
        Self::from_sender_for_test(tx, generation)
    }

    #[cfg(test)]
    pub(crate) fn accepting_for_test() -> Self {
        let (tx, rx) = mpsc::channel(LSP_COMMAND_QUEUE_CAPACITY);
        let _rx = Box::leak(Box::new(rx));
        Self::from_sender_for_test(tx, 1)
    }

    #[cfg(test)]
    pub(crate) fn full_queue_for_test() -> Self {
        let (tx, rx) = mpsc::channel(1);
        tx.try_send(LspClientCommand::DidSave {
            path: PathBuf::from("queued.rs"),
        })
        .expect("test queue should accept initial command");
        let _rx = Box::leak(Box::new(rx));
        Self::from_sender_for_test(tx, 1)
    }

    #[cfg(test)]
    pub(crate) fn from_sender_for_test(
        tx: mpsc::Sender<LspClientCommand>,
        generation: u64,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let _shutdown_rx = Box::leak(Box::new(shutdown_rx));
        Self {
            tx,
            shutdown_tx,
            stderr_log: LspStderrLog::default(),
            watched_files: LspWatchedFilesState::default(),
            capabilities: LspServerCapabilitiesState::default(),
            open_documents: LspOpenDocumentsState::default(),
            generation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LSP_COMMAND_QUEUE_CAPACITY, LspClientHandle, LspOpenDocumentsState, LspServerCapabilities,
        LspServerCapabilitiesState, MAX_TRACKED_OPEN_DOCUMENTS_PER_CLIENT,
    };
    use crate::lsp_client::commands::LspClientCommand;
    use crate::lsp_client::stderr_log::LspStderrLog;
    use kuroya_core::LspRequestId;
    use std::path::{Path, PathBuf};
    use tokio::sync::{mpsc, watch};

    #[test]
    fn lsp_command_queue_is_bounded_and_nonblocking() {
        let (tx, _rx) = mpsc::channel(1);
        let handle = LspClientHandle::from_sender_for_test(tx, 1);

        assert!(handle.queue_command(LspClientCommand::DidSave {
            path: PathBuf::from("src/main.rs"),
        }));
        assert!(!handle.queue_command(LspClientCommand::DidSave {
            path: PathBuf::from("src/lib.rs"),
        }));
    }

    #[test]
    fn apply_workspace_edit_response_queues_direct_response_command() {
        let (tx, mut rx) = mpsc::channel(1);
        let handle = LspClientHandle::from_sender_for_test(tx, 1);

        assert!(handle.apply_workspace_edit_response(
            LspRequestId::Number(17),
            false,
            Some("buffer changed".to_owned())
        ));

        match rx.try_recv() {
            Ok(LspClientCommand::ApplyWorkspaceEditResponse {
                request_id,
                applied,
                failure_reason,
            }) => {
                assert_eq!(request_id, LspRequestId::Number(17));
                assert!(!applied);
                assert_eq!(failure_reason.as_deref(), Some("buffer changed"));
            }
            other => panic!("expected apply-edit response command, got {other:?}"),
        }
    }

    #[test]
    fn shutdown_signals_even_when_command_queue_is_full() {
        let (tx, _rx) = mpsc::channel(1);
        tx.try_send(LspClientCommand::DidSave {
            path: PathBuf::from("queued.rs"),
        })
        .expect("test queue should accept initial command");
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        let handle = LspClientHandle {
            tx,
            shutdown_tx,
            stderr_log: LspStderrLog::default(),
            watched_files: crate::lsp_client::watched_files::LspWatchedFilesState::default(),
            capabilities: LspServerCapabilitiesState::default(),
            open_documents: LspOpenDocumentsState::default(),
            generation: 1,
        };

        assert!(!handle.queue_command(LspClientCommand::DidSave {
            path: PathBuf::from("blocked.rs"),
        }));
        assert!(handle.shutdown());

        assert!(
            shutdown_rx.has_changed().expect("shutdown sender is live"),
            "shutdown signal should not depend on command queue capacity"
        );
        assert!(*shutdown_rx.borrow_and_update());
    }

    #[test]
    fn capabilities_state_reads_back_the_last_written_value() {
        let state = LspServerCapabilitiesState::default();
        assert_eq!(state.get(), None, "unknown until the handshake writes it");

        state.set(LspServerCapabilities {
            rename_provider: true,
            prepare_rename_supported: true,
            semantic_tokens_provider: false,
            inlay_hint_provider: true,
            code_lens_provider: false,
        });

        assert_eq!(
            state.get(),
            Some(LspServerCapabilities {
                rename_provider: true,
                prepare_rename_supported: true,
                semantic_tokens_provider: false,
                inlay_hint_provider: true,
                code_lens_provider: false,
            })
        );
    }

    #[test]
    fn test_handles_start_with_unknown_capabilities_and_empty_tracking() {
        let handle =
            LspClientHandle::from_sender_for_test(mpsc::channel(LSP_COMMAND_QUEUE_CAPACITY).0, 1);

        assert_eq!(handle.capabilities(), None);
        assert!(handle.open_documents().is_empty());
    }

    #[test]
    fn open_documents_tracking_records_and_drops_paths() {
        let tracking = LspOpenDocumentsState::default();
        let path = Path::new("src/main.rs");
        assert!(!tracking.contains(path));

        tracking.note_open(path);
        assert!(tracking.contains(path));
        assert_eq!(tracking.len(), 1);

        tracking.note_open(path);
        assert_eq!(tracking.len(), 1, "re-open stays a single entry");

        tracking.note_closed(path);
        assert!(!tracking.contains(path));
        assert!(tracking.is_empty());

        tracking.note_closed(path);
        assert_eq!(tracking.len(), 0, "closing unknown paths is a no-op");
    }

    #[test]
    fn open_documents_tracking_is_bounded_per_client() {
        let tracking = LspOpenDocumentsState::default();
        for index in 0..MAX_TRACKED_OPEN_DOCUMENTS_PER_CLIENT + 16 {
            tracking.note_open(Path::new(&format!("src/{index}.rs")));
        }

        assert_eq!(tracking.len(), MAX_TRACKED_OPEN_DOCUMENTS_PER_CLIENT);
        assert!(tracking.contains(Path::new("src/0.rs")));
        assert!(
            !tracking.contains(Path::new(&format!(
                "src/{}.rs",
                MAX_TRACKED_OPEN_DOCUMENTS_PER_CLIENT + 15
            ))),
            "paths past the cap are not tracked"
        );
    }
}
