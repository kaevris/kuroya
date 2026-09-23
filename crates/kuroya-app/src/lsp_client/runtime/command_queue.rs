use crate::lsp_client::{commands::LspClientCommand, handle::LSP_COMMAND_QUEUE_CAPACITY};
use std::{collections::VecDeque, path::Path};
use tokio::sync::mpsc;

const MAX_DID_CHANGE_COALESCE_DRAIN_PER_RECV: usize = 64;
const MAX_INTERNAL_PENDING_COMMANDS: usize = LSP_COMMAND_QUEUE_CAPACITY;

#[derive(Default)]
pub(super) struct LspClientCommandQueue {
    pending: VecDeque<LspClientCommand>,
}

impl LspClientCommandQueue {
    pub(super) async fn recv(
        &mut self,
        rx: &mut mpsc::Receiver<LspClientCommand>,
    ) -> Option<LspClientCommand> {
        let command = match self.pending.pop_front() {
            Some(command) => command,
            None => rx.recv().await?,
        };

        if command_is_shutdown(&command) {
            self.discard_buffered_commands(rx);
            return Some(command);
        }
        if self.promote_buffered_shutdown(rx) {
            return Some(LspClientCommand::Shutdown);
        }

        Some(self.coalesce_queued_did_changes(command, rx))
    }

    fn coalesce_queued_did_changes(
        &mut self,
        mut command: LspClientCommand,
        rx: &mut mpsc::Receiver<LspClientCommand>,
    ) -> LspClientCommand {
        if !matches!(command, LspClientCommand::DidChange { .. }) {
            return command;
        }

        for _ in 0..MAX_DID_CHANGE_COALESCE_DRAIN_PER_RECV {
            let Some(next) = self.try_recv_buffered_command(rx) else {
                break;
            };
            if did_change_commands_target_same_document(&command, &next) {
                command = next;
            } else {
                self.pending.push_front(next);
                break;
            }
        }

        command
    }

    fn promote_buffered_shutdown(&mut self, rx: &mut mpsc::Receiver<LspClientCommand>) -> bool {
        if self.pending.iter().any(command_is_shutdown) {
            self.pending.clear();
            self.discard_buffered_commands(rx);
            return true;
        }

        while let Ok(next) = rx.try_recv() {
            if command_is_shutdown(&next) {
                self.pending.clear();
                self.discard_buffered_commands(rx);
                return true;
            }
            if self.pending.len() >= MAX_INTERNAL_PENDING_COMMANDS {
                break;
            }
            self.pending.push_back(next);
        }

        false
    }

    fn discard_buffered_commands(&mut self, rx: &mut mpsc::Receiver<LspClientCommand>) {
        self.pending.clear();
        while rx.try_recv().is_ok() {}
    }

    fn try_recv_buffered_command(
        &mut self,
        rx: &mut mpsc::Receiver<LspClientCommand>,
    ) -> Option<LspClientCommand> {
        self.pending.pop_front().or_else(|| rx.try_recv().ok())
    }
}

fn command_is_shutdown(command: &LspClientCommand) -> bool {
    matches!(command, LspClientCommand::Shutdown)
}

fn did_change_commands_target_same_document(
    current: &LspClientCommand,
    next: &LspClientCommand,
) -> bool {
    match (current, next) {
        (
            LspClientCommand::DidChange {
                id: current_id,
                path: current_path,
                ..
            },
            LspClientCommand::DidChange {
                id: next_id,
                path: next_path,
                ..
            },
        ) => current_id == next_id && paths_match(current_path, next_path),
        _ => false,
    }
}

fn paths_match(left: &Path, right: &Path) -> bool {
    left == right
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuroya_core::TextBuffer;
    use std::path::PathBuf;

    fn text_snapshot(text: &str) -> kuroya_core::TextSnapshot {
        TextBuffer::from_text(1, None, text.to_owned()).text_snapshot()
    }

    fn did_change(id: u64, path: &str, version: u64, text: &str) -> LspClientCommand {
        LspClientCommand::DidChange {
            id,
            path: PathBuf::from(path),
            version,
            text: text_snapshot(text),
        }
    }

    fn did_open(id: u64, path: &str) -> LspClientCommand {
        LspClientCommand::DidOpen {
            id,
            path: PathBuf::from(path),
            language: "rust".to_owned(),
            version: 1,
            text: text_snapshot("open"),
        }
    }

    fn did_save(path: &str) -> LspClientCommand {
        LspClientCommand::DidSave {
            path: PathBuf::from(path),
        }
    }

    fn did_close(path: &str) -> LspClientCommand {
        LspClientCommand::DidClose {
            path: PathBuf::from(path),
        }
    }

    fn hover(id: u64, path: &str) -> LspClientCommand {
        LspClientCommand::Hover {
            id,
            path: PathBuf::from(path),
            version: 1,
            line: 0,
            character: 0,
        }
    }

    fn assert_did_change(command: LspClientCommand, id: u64, path: &str, version: u64, text: &str) {
        match command {
            LspClientCommand::DidChange {
                id: actual_id,
                path: actual_path,
                version: actual_version,
                text: actual_text,
            } => {
                assert_eq!(actual_id, id);
                assert_eq!(actual_path, PathBuf::from(path));
                assert_eq!(actual_version, version);
                assert_eq!(actual_text.text(), text);
            }
            other => panic!("expected didChange command, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn recv_coalesces_contiguous_did_changes_for_same_document() {
        let (tx, mut rx) = mpsc::channel(8);
        tx.try_send(did_change(1, "src/main.rs", 1, "one")).unwrap();
        tx.try_send(did_change(1, "src/main.rs", 2, "two")).unwrap();
        tx.try_send(did_change(1, "src/main.rs", 3, "three"))
            .unwrap();
        tx.try_send(did_save("src/main.rs")).unwrap();
        drop(tx);

        let mut queue = LspClientCommandQueue::default();

        assert_did_change(
            queue.recv(&mut rx).await.expect("coalesced didChange"),
            1,
            "src/main.rs",
            3,
            "three",
        );
        assert!(matches!(
            queue.recv(&mut rx).await,
            Some(LspClientCommand::DidSave { .. })
        ));
        assert!(queue.recv(&mut rx).await.is_none());
    }

    #[tokio::test]
    async fn recv_bounds_contiguous_did_change_coalescing_per_pass() {
        let change_count = MAX_DID_CHANGE_COALESCE_DRAIN_PER_RECV + 2;
        let (tx, mut rx) = mpsc::channel(change_count + 1);
        for version in 1..=change_count {
            tx.try_send(did_change(
                1,
                "src/main.rs",
                version as u64,
                &format!("version {version}"),
            ))
            .unwrap();
        }
        tx.try_send(did_save("src/main.rs")).unwrap();
        drop(tx);

        let mut queue = LspClientCommandQueue::default();

        let first_expected_version = MAX_DID_CHANGE_COALESCE_DRAIN_PER_RECV + 1;
        assert_did_change(
            queue
                .recv(&mut rx)
                .await
                .expect("first bounded didChange batch"),
            1,
            "src/main.rs",
            first_expected_version as u64,
            &format!("version {first_expected_version}"),
        );
        assert_did_change(
            queue
                .recv(&mut rx)
                .await
                .expect("remaining didChange after bounded batch"),
            1,
            "src/main.rs",
            change_count as u64,
            &format!("version {change_count}"),
        );
        assert!(matches!(
            queue.recv(&mut rx).await,
            Some(LspClientCommand::DidSave { .. })
        ));
        assert!(queue.recv(&mut rx).await.is_none());
    }

    #[tokio::test]
    async fn recv_keeps_different_documents_as_order_barriers() {
        let (tx, mut rx) = mpsc::channel(8);
        tx.try_send(did_change(1, "src/main.rs", 1, "one")).unwrap();
        tx.try_send(did_change(2, "src/lib.rs", 1, "lib")).unwrap();
        tx.try_send(did_change(1, "src/main.rs", 2, "two")).unwrap();
        drop(tx);

        let mut queue = LspClientCommandQueue::default();

        assert_did_change(
            queue.recv(&mut rx).await.expect("first didChange"),
            1,
            "src/main.rs",
            1,
            "one",
        );
        assert_did_change(
            queue
                .recv(&mut rx)
                .await
                .expect("different document didChange"),
            2,
            "src/lib.rs",
            1,
            "lib",
        );
        assert_did_change(
            queue.recv(&mut rx).await.expect("later didChange"),
            1,
            "src/main.rs",
            2,
            "two",
        );
    }

    #[tokio::test]
    async fn recv_does_not_coalesce_across_save_or_request_commands() {
        let (tx, mut rx) = mpsc::channel(8);
        tx.try_send(did_change(1, "src/main.rs", 1, "one")).unwrap();
        tx.try_send(did_save("src/main.rs")).unwrap();
        tx.try_send(did_change(1, "src/main.rs", 2, "two")).unwrap();
        tx.try_send(hover(1, "src/main.rs")).unwrap();
        tx.try_send(did_change(1, "src/main.rs", 3, "three"))
            .unwrap();
        drop(tx);

        let mut queue = LspClientCommandQueue::default();

        assert_did_change(
            queue.recv(&mut rx).await.expect("first didChange"),
            1,
            "src/main.rs",
            1,
            "one",
        );
        assert!(matches!(
            queue.recv(&mut rx).await,
            Some(LspClientCommand::DidSave { .. })
        ));
        assert_did_change(
            queue.recv(&mut rx).await.expect("second didChange"),
            1,
            "src/main.rs",
            2,
            "two",
        );
        assert!(matches!(
            queue.recv(&mut rx).await,
            Some(LspClientCommand::Hover { .. })
        ));
        assert_did_change(
            queue.recv(&mut rx).await.expect("third didChange"),
            1,
            "src/main.rs",
            3,
            "three",
        );
    }

    #[tokio::test]
    async fn recv_does_not_coalesce_across_close_commands() {
        let (tx, mut rx) = mpsc::channel(8);
        tx.try_send(did_change(1, "src/main.rs", 1, "one")).unwrap();
        tx.try_send(did_close("src/main.rs")).unwrap();
        tx.try_send(did_change(1, "src/main.rs", 2, "two")).unwrap();
        drop(tx);

        let mut queue = LspClientCommandQueue::default();

        assert_did_change(
            queue.recv(&mut rx).await.expect("first didChange"),
            1,
            "src/main.rs",
            1,
            "one",
        );
        assert!(matches!(
            queue.recv(&mut rx).await,
            Some(LspClientCommand::DidClose { .. })
        ));
        assert_did_change(
            queue.recv(&mut rx).await.expect("second didChange"),
            1,
            "src/main.rs",
            2,
            "two",
        );
        assert!(queue.recv(&mut rx).await.is_none());
    }

    #[tokio::test]
    async fn recv_leaves_non_change_commands_unmodified() {
        let (tx, mut rx) = mpsc::channel(4);
        tx.try_send(did_open(1, "src/main.rs")).unwrap();
        tx.try_send(did_change(1, "src/main.rs", 2, "two")).unwrap();
        drop(tx);

        let mut queue = LspClientCommandQueue::default();

        assert!(matches!(
            queue.recv(&mut rx).await,
            Some(LspClientCommand::DidOpen { .. })
        ));
        assert_did_change(
            queue.recv(&mut rx).await.expect("didChange after open"),
            1,
            "src/main.rs",
            2,
            "two",
        );
    }

    #[tokio::test]
    async fn recv_does_not_coalesce_across_did_open_commands() {
        let (tx, mut rx) = mpsc::channel(4);
        tx.try_send(did_change(1, "src/main.rs", 1, "one")).unwrap();
        tx.try_send(did_open(1, "src/main.rs")).unwrap();
        tx.try_send(did_change(1, "src/main.rs", 2, "two")).unwrap();
        drop(tx);

        let mut queue = LspClientCommandQueue::default();

        assert_did_change(
            queue.recv(&mut rx).await.expect("first didChange"),
            1,
            "src/main.rs",
            1,
            "one",
        );
        assert!(matches!(
            queue.recv(&mut rx).await,
            Some(LspClientCommand::DidOpen { .. })
        ));
        assert_did_change(
            queue.recv(&mut rx).await.expect("second didChange"),
            1,
            "src/main.rs",
            2,
            "two",
        );
    }

    #[tokio::test]
    async fn recv_promotes_shutdown_over_buffered_commands() {
        let (tx, mut rx) = mpsc::channel(8);
        tx.try_send(did_change(1, "src/main.rs", 1, "one")).unwrap();
        tx.try_send(did_save("src/main.rs")).unwrap();
        tx.try_send(LspClientCommand::Shutdown).unwrap();
        tx.try_send(did_change(1, "src/main.rs", 2, "two")).unwrap();
        drop(tx);

        let mut queue = LspClientCommandQueue::default();

        assert!(matches!(
            queue.recv(&mut rx).await,
            Some(LspClientCommand::Shutdown)
        ));
        assert!(queue.pending.is_empty());
        assert!(queue.recv(&mut rx).await.is_none());
    }

    #[tokio::test]
    async fn recv_promotes_shutdown_from_full_buffered_channel() {
        let (tx, mut rx) = mpsc::channel(2);
        tx.try_send(did_change(1, "src/main.rs", 1, "one")).unwrap();
        tx.try_send(LspClientCommand::Shutdown).unwrap();
        drop(tx);

        let mut queue = LspClientCommandQueue::default();

        assert!(matches!(
            queue.recv(&mut rx).await,
            Some(LspClientCommand::Shutdown)
        ));
        assert!(queue.pending.is_empty());
        assert!(queue.recv(&mut rx).await.is_none());
    }

    #[tokio::test]
    async fn recv_does_not_overfill_internal_pending_buffer_when_scanning_for_shutdown() {
        let (tx, mut rx) = mpsc::channel(MAX_INTERNAL_PENDING_COMMANDS + 8);
        for version in 0..(MAX_INTERNAL_PENDING_COMMANDS + 8) {
            tx.try_send(did_save(&format!("src/file_{version}.rs")))
                .unwrap();
        }
        drop(tx);
        let mut queue = LspClientCommandQueue::default();
        for version in 0..MAX_INTERNAL_PENDING_COMMANDS {
            queue
                .pending
                .push_back(did_save(&format!("src/pending_{version}.rs")));
        }

        assert!(matches!(
            queue.recv(&mut rx).await,
            Some(LspClientCommand::DidSave { .. })
        ));

        assert_eq!(queue.pending.len(), MAX_INTERNAL_PENDING_COMMANDS);
        assert!(!rx.is_empty());
    }

    #[tokio::test]
    async fn recv_discards_commands_after_shutdown() {
        let (tx, mut rx) = mpsc::channel(8);
        tx.try_send(LspClientCommand::Shutdown).unwrap();
        tx.try_send(did_change(1, "src/main.rs", 1, "one")).unwrap();
        tx.try_send(did_save("src/main.rs")).unwrap();
        drop(tx);

        let mut queue = LspClientCommandQueue::default();

        assert!(matches!(
            queue.recv(&mut rx).await,
            Some(LspClientCommand::Shutdown)
        ));
        assert!(queue.pending.is_empty());
        assert!(queue.recv(&mut rx).await.is_none());
    }
}
