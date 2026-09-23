use crate::ui_events::UiEvent;
pub(crate) use crossbeam_channel::{Receiver, Sender};
use crossbeam_channel::{SendTimeoutError, TrySendError, bounded};
use std::{
    sync::{
        Arc, OnceLock, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

pub(crate) const UI_EVENT_CHANNEL_BOUND: usize = 4096;
const CRITICAL_UI_EVENT_SEND_TIMEOUT_MS: u64 = 100;
static DROPPED_UI_EVENTS: AtomicUsize = AtomicUsize::new(0);
type UiWakeHook = Arc<dyn Fn() + Send + Sync>;
static UI_WAKE_HOOK: OnceLock<RwLock<Option<UiWakeHook>>> = OnceLock::new();

pub(crate) fn ui_event_channel() -> (Sender<UiEvent>, Receiver<UiEvent>) {
    bounded(UI_EVENT_CHANNEL_BOUND)
}

pub(crate) fn send_ui_event(tx: &Sender<UiEvent>, event: UiEvent) -> bool {
    match tx.try_send(event) {
        Ok(()) => {
            invoke_ui_wake_hook();
            true
        }
        Err(_) => {
            track_dropped_ui_event();
            false
        }
    }
}

pub(crate) fn set_ui_wake_hook(hook: UiWakeHook) {
    if let Ok(mut slot) = ui_wake_hook_slot().write() {
        *slot = Some(hook);
    }
}

pub(crate) fn notify_background_activity() {
    invoke_ui_wake_hook();
}

fn ui_wake_hook_slot() -> &'static RwLock<Option<UiWakeHook>> {
    UI_WAKE_HOOK.get_or_init(|| RwLock::new(None))
}

fn invoke_ui_wake_hook() {
    if let Ok(slot) = ui_wake_hook_slot().read()
        && let Some(hook) = slot.as_ref()
    {
        hook();
    }
}

pub(crate) fn send_critical_ui_event(tx: &Sender<UiEvent>, event: UiEvent) -> bool {
    send_critical_ui_event_with_timeout(
        tx,
        event,
        Duration::from_millis(CRITICAL_UI_EVENT_SEND_TIMEOUT_MS),
    )
}

pub(crate) fn send_critical_ui_event_with_timeout(
    tx: &Sender<UiEvent>,
    event: UiEvent,
    timeout: Duration,
) -> bool {
    if timeout.is_zero() {
        return match tx.try_send(event) {
            Ok(()) => {
                invoke_ui_wake_hook();
                true
            }
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                track_dropped_ui_event();
                false
            }
        };
    }

    match tx.send_timeout(event, timeout) {
        Ok(()) => {
            invoke_ui_wake_hook();
            true
        }
        Err(SendTimeoutError::Timeout(_)) | Err(SendTimeoutError::Disconnected(_)) => {
            track_dropped_ui_event();
            false
        }
    }
}

fn track_dropped_ui_event() {
    DROPPED_UI_EVENTS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(test)]
fn dropped_ui_event_count() -> usize {
    DROPPED_UI_EVENTS.load(Ordering::Relaxed)
}

pub(crate) fn take_dropped_ui_event_count() -> usize {
    DROPPED_UI_EVENTS.swap(0, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp_ui_events::LspUiEvent;
    use crossbeam_channel::TrySendError;
    use kuroya_core::{GitSnapshot, SearchOptions, SearchResult, TextBuffer};
    use std::{
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
        time::Duration,
    };

    static WAKE_HOOK_CALLS: AtomicUsize = AtomicUsize::new(0);

    fn install_wake_counting_hook() -> usize {
        set_ui_wake_hook(Arc::new(|| {
            WAKE_HOOK_CALLS.fetch_add(1, Ordering::Relaxed);
        }));
        WAKE_HOOK_CALLS.load(Ordering::Relaxed)
    }

    #[test]
    fn ui_event_channel_is_bounded() {
        let (tx, _rx) = ui_event_channel();

        for index in 0..UI_EVENT_CHANNEL_BOUND {
            tx.try_send(UiEvent::Lsp(LspUiEvent::WorkDoneProgressCreated {
                token: format!("progress-token-{index}"),
            }))
            .expect("event should fit within channel bound");
        }

        assert!(matches!(
            tx.try_send(UiEvent::Lsp(LspUiEvent::WorkDoneProgressCreated {
                token: "overflow".to_owned(),
            })),
            Err(TrySendError::Full(_))
        ));
    }

    #[test]
    fn send_ui_event_drops_when_channel_is_full() {
        let (tx, _rx) = ui_event_channel();

        for index in 0..UI_EVENT_CHANNEL_BOUND {
            assert!(send_ui_event(
                &tx,
                UiEvent::Lsp(LspUiEvent::WorkDoneProgressCreated {
                    token: format!("progress-token-{index}"),
                })
            ));
        }

        assert!(!send_ui_event(
            &tx,
            UiEvent::Lsp(LspUiEvent::WorkDoneProgressCreated {
                token: "overflow".to_owned(),
            })
        ));
    }

    #[test]
    fn dropped_ui_event_count_tracks_channel_backpressure() {
        let before = dropped_ui_event_count();
        let (tx, _rx) = ui_event_channel();

        for index in 0..UI_EVENT_CHANNEL_BOUND {
            assert!(send_ui_event(
                &tx,
                UiEvent::Lsp(LspUiEvent::WorkDoneProgressCreated {
                    token: format!("progress-token-{index}"),
                })
            ));
        }

        assert!(!send_ui_event(
            &tx,
            UiEvent::Lsp(LspUiEvent::WorkDoneProgressCreated {
                token: "overflow".to_owned(),
            })
        ));
        assert!(dropped_ui_event_count() >= before.saturating_add(1));
    }

    #[test]
    fn send_critical_ui_event_waits_for_capacity_instead_of_dropping() {
        assert_critical_event_delivered_when_full(
            UiEvent::Lsp(LspUiEvent::ServerStopped {
                language: "rust".to_owned(),
                root: PathBuf::from("workspace"),
                generation: 1,
            }),
            |event| matches!(event, UiEvent::Lsp(LspUiEvent::ServerStopped { .. })),
        );
    }

    #[test]
    fn send_critical_ui_event_returns_false_and_tracks_drop_when_channel_stays_full() {
        let before = dropped_ui_event_count();
        let (tx, _rx) = ui_event_channel();

        for index in 0..UI_EVENT_CHANNEL_BOUND {
            tx.try_send(UiEvent::Lsp(LspUiEvent::WorkDoneProgressCreated {
                token: format!("progress-token-{index}"),
            }))
            .expect("event should fit within channel bound");
        }

        assert!(!send_critical_ui_event(
            &tx,
            UiEvent::Lsp(LspUiEvent::ServerStopped {
                language: "rust".to_owned(),
                root: PathBuf::from("workspace"),
                generation: 1,
            })
        ));
        assert!(dropped_ui_event_count() >= before.saturating_add(1));
    }

    #[test]
    fn send_critical_ui_event_returns_false_and_tracks_drop_when_disconnected() {
        let before = dropped_ui_event_count();
        let (tx, rx) = ui_event_channel();
        drop(rx);

        assert!(!send_critical_ui_event(
            &tx,
            UiEvent::Lsp(LspUiEvent::ServerStopped {
                language: "rust".to_owned(),
                root: PathBuf::from("workspace"),
                generation: 1,
            })
        ));
        assert!(dropped_ui_event_count() >= before.saturating_add(1));
    }

    #[test]
    fn file_save_completion_uses_critical_delivery_under_backpressure() {
        assert_critical_event_delivered_when_full(
            UiEvent::FileSaved {
                root: PathBuf::from("workspace"),
                generation: 7,
                id: 11,
                path: PathBuf::from("workspace/src/main.rs"),
                version: 3,
            },
            |event| {
                matches!(
                    event,
                    UiEvent::FileSaved {
                        id: 11,
                        version: 3,
                        ..
                    }
                )
            },
        );
    }

    #[test]
    fn file_save_failure_completion_uses_critical_delivery_under_backpressure() {
        assert_critical_event_delivered_when_full(
            UiEvent::FileSaveFailed {
                root: PathBuf::from("workspace"),
                generation: 7,
                id: 11,
                path: PathBuf::from("workspace/src/main.rs"),
                error: "disk full".to_owned(),
            },
            |event| matches!(event, UiEvent::FileSaveFailed { id: 11, .. }),
        );
    }

    #[test]
    fn file_load_completion_uses_critical_delivery_under_backpressure() {
        assert_critical_event_delivered_when_full(
            UiEvent::FileLoaded {
                root: PathBuf::from("workspace"),
                generation: 7,
                path: PathBuf::from("workspace/src/main.rs"),
                buffer: TextBuffer::from_text(
                    11,
                    Some(PathBuf::from("workspace/src/main.rs")),
                    "fn main() {}\n".to_owned(),
                ),
                elapsed: Duration::from_millis(4),
                activate: true,
                lossy: false,
                binary: false,
            },
            |event| matches!(event, UiEvent::FileLoaded { path, .. } if path.ends_with("src/main.rs")),
        );
    }

    #[test]
    fn file_load_failure_completion_uses_critical_delivery_under_backpressure() {
        assert_critical_event_delivered_when_full(
            UiEvent::FileLoadFailed {
                root: PathBuf::from("workspace"),
                generation: 7,
                path: PathBuf::from("workspace/src/main.rs"),
                error: "not found".to_owned(),
            },
            |event| matches!(event, UiEvent::FileLoadFailed { path, .. } if path.ends_with("src/main.rs")),
        );
    }

    #[test]
    fn file_reload_completion_uses_critical_delivery_under_backpressure() {
        assert_critical_event_delivered_when_full(
            UiEvent::FileReloaded {
                root: PathBuf::from("workspace"),
                generation: 7,
                request_id: 13,
                id: 11,
                path: PathBuf::from("workspace/src/main.rs"),
                buffer: TextBuffer::from_text(
                    11,
                    Some(PathBuf::from("workspace/src/main.rs")),
                    "fn main() {}\n".to_owned(),
                ),
                elapsed: Duration::from_millis(4),
                version: 3,
                force_dirty: true,
                lossy: false,
                binary: false,
            },
            |event| {
                matches!(
                    event,
                    UiEvent::FileReloaded {
                        request_id: 13,
                        id: 11,
                        version: 3,
                        force_dirty: true,
                        ..
                    }
                )
            },
        );
    }

    #[test]
    fn file_reload_failure_completion_uses_critical_delivery_under_backpressure() {
        assert_critical_event_delivered_when_full(
            UiEvent::FileReloadFailed {
                root: PathBuf::from("workspace"),
                generation: 7,
                request_id: 13,
                id: 11,
                path: PathBuf::from("workspace/src/main.rs"),
                error: "read failed".to_owned(),
                version: 3,
                force_dirty: true,
            },
            |event| {
                matches!(
                    event,
                    UiEvent::FileReloadFailed {
                        request_id: 13,
                        id: 11,
                        version: 3,
                        force_dirty: true,
                        ..
                    }
                )
            },
        );
    }

    #[test]
    fn workspace_task_load_completion_uses_critical_delivery_under_backpressure() {
        assert_critical_event_delivered_when_full(
            UiEvent::WorkspaceTasksLoaded {
                request_id: 17,
                root: PathBuf::from("workspace"),
                tasks: Vec::new(),
            },
            |event| matches!(event, UiEvent::WorkspaceTasksLoaded { request_id: 17, .. }),
        );
    }

    #[test]
    fn diagnostics_completion_uses_critical_delivery_under_backpressure() {
        assert_critical_event_delivered_when_full(
            UiEvent::DiagnosticsComputed {
                request_id: 23,
                id: 11,
                path: PathBuf::from("workspace/src/main.rs"),
                version: 5,
                diagnostics: Vec::new(),
            },
            |event| {
                matches!(
                    event,
                    UiEvent::DiagnosticsComputed {
                        request_id: 23,
                        id: 11,
                        version: 5,
                        ..
                    }
                )
            },
        );
    }

    #[test]
    fn project_search_completion_uses_critical_delivery_under_backpressure() {
        assert_critical_event_delivered_when_full(
            UiEvent::SearchFinished {
                request_id: 31,
                index_generation: 2,
                workspace_root: PathBuf::from("workspace"),
                query: "main".to_owned(),
                case_sensitive: false,
                whole_word: false,
                include_globs: Vec::new(),
                exclude_globs: Vec::new(),
                max_file_bytes: SearchOptions::default().max_file_bytes,
                max_results: SearchOptions::default().max_results,
                result: SearchResult::default(),
            },
            |event| matches!(event, UiEvent::SearchFinished { request_id: 31, .. }),
        );
    }

    #[test]
    fn startup_completion_events_use_critical_delivery_under_backpressure() {
        assert_critical_event_delivered_when_full(
            UiEvent::GitScanned {
                request_id: 41,
                root: PathBuf::from("workspace"),
                scan_root: Some(PathBuf::from("workspace")),
                root_cache_entry: None,
                git: GitSnapshot::default(),
            },
            |event| matches!(event, UiEvent::GitScanned { request_id: 41, .. }),
        );

        assert_critical_event_delivered_when_full(
            UiEvent::WorkspacePluginsFailed {
                request_id: 43,
                root: PathBuf::from("workspace"),
                error: "plugin discovery failed".to_owned(),
            },
            |event| {
                matches!(
                    event,
                    UiEvent::WorkspacePluginsFailed { request_id: 43, .. }
                )
            },
        );
    }

    #[test]
    fn wake_hook_fires_on_successful_normal_send() {
        let before = install_wake_counting_hook();
        let (tx, _rx) = ui_event_channel();

        assert!(send_ui_event(
            &tx,
            UiEvent::Lsp(LspUiEvent::WorkDoneProgressCreated {
                token: "wake-hook-normal".to_owned(),
            })
        ));

        assert!(WAKE_HOOK_CALLS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn wake_hook_fires_on_successful_critical_send() {
        let before = install_wake_counting_hook();
        let (tx, rx) = ui_event_channel();

        assert!(send_critical_ui_event(
            &tx,
            UiEvent::SessionSaved {
                root: PathBuf::from("workspace")
            }
        ));

        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(1)),
            Ok(UiEvent::SessionSaved { .. })
        ));
        assert!(WAKE_HOOK_CALLS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn notify_background_activity_invokes_wake_hook() {
        let before = install_wake_counting_hook();

        notify_background_activity();

        assert!(WAKE_HOOK_CALLS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn session_save_completion_uses_critical_delivery_under_backpressure() {
        assert_critical_event_delivered_when_full(
            UiEvent::SessionSaved {
                root: PathBuf::from("workspace"),
            },
            |event| matches!(event, UiEvent::SessionSaved { .. }),
        );
    }

    #[test]
    fn session_save_failure_completion_uses_critical_delivery_under_backpressure() {
        assert_critical_event_delivered_when_full(
            UiEvent::SessionSaveFailed {
                root: PathBuf::from("workspace"),
                error: "disk full".to_owned(),
            },
            |event| matches!(event, UiEvent::SessionSaveFailed { error, .. } if error == "disk full"),
        );
    }

    fn assert_critical_event_delivered_when_full(
        critical_event: UiEvent,
        is_expected: impl Fn(&UiEvent) -> bool,
    ) {
        let (tx, rx) = ui_event_channel();

        for index in 0..UI_EVENT_CHANNEL_BOUND {
            tx.try_send(UiEvent::Lsp(LspUiEvent::WorkDoneProgressCreated {
                token: format!("progress-token-{index}"),
            }))
            .expect("event should fit within channel bound");
        }

        let critical_tx = tx.clone();
        let sender = thread::spawn(move || send_critical_ui_event(&critical_tx, critical_event));

        let _ = rx
            .recv_timeout(Duration::from_secs(1))
            .expect("freeing capacity should unblock critical event sender");
        assert!(sender.join().unwrap());

        let mut delivered = false;
        for _ in 0..UI_EVENT_CHANNEL_BOUND {
            let event = rx
                .recv_timeout(Duration::from_secs(1))
                .expect("critical event should be queued after capacity is freed");
            if is_expected(&event) {
                delivered = true;
                break;
            }
        }
        assert!(delivered);
    }
}
