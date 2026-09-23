use crate::lsp_runtime::{lsp_read_error_status_message, lsp_stopped_status_message_with_detail};
use crate::ui_event_channel::Sender;
use crate::{lsp_ui_events::LspUiEvent, ui_events::UiEvent};
use std::path::Path;

pub(super) fn send_lsp_read_error_status(
    language: &str,
    root: &Path,
    generation: u64,
    error: &anyhow::Error,
    ui_tx: &Sender<UiEvent>,
) {
    let _ = crate::ui_event_channel::send_ui_event(
        ui_tx,
        UiEvent::Lsp(LspUiEvent::Status {
            language: language.to_owned(),
            root: root.to_path_buf(),
            generation,
            message: lsp_read_error_status_message(language, error),
        }),
    );
}

pub(super) fn send_lsp_stopped_status(
    language: &str,
    root: &Path,
    generation: u64,
    detail: Option<&str>,
    ui_tx: &Sender<UiEvent>,
) {
    let language_key = language.to_owned();
    let root_path = root.to_path_buf();
    let _ = crate::ui_event_channel::send_ui_event(
        ui_tx,
        UiEvent::Lsp(LspUiEvent::Status {
            language: language_key.clone(),
            root: root_path.clone(),
            generation,
            message: lsp_stopped_status_message_with_detail(language, detail),
        }),
    );
    let _ = crate::ui_event_channel::send_critical_ui_event(
        ui_tx,
        UiEvent::Lsp(LspUiEvent::ServerStopped {
            language: language_key,
            root: root_path,
            generation,
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        lsp_runtime::{LSP_LANGUAGE_LABEL_MAX_CHARS, LSP_STATUS_MESSAGE_MAX_CHARS},
        lsp_ui_events::LspUiEvent,
        ui_event_channel::{UI_EVENT_CHANNEL_BOUND, ui_event_channel},
        ui_events::UiEvent,
    };
    use std::{path::PathBuf, thread, time::Duration};

    #[test]
    fn stopped_status_delivers_server_stopped_when_channel_is_full() {
        let (tx, rx) = ui_event_channel();

        for index in 0..UI_EVENT_CHANNEL_BOUND {
            tx.try_send(UiEvent::Lsp(LspUiEvent::WorkDoneProgressCreated {
                token: format!("progress-token-{index}"),
            }))
            .expect("event should fit within channel bound");
        }

        let status_tx = tx.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let sender = thread::spawn(move || {
            send_lsp_stopped_status("rust", &PathBuf::from("workspace"), 7, None, &status_tx);
            done_tx.send(()).unwrap();
        });

        let mut drained = Vec::new();
        while done_rx.recv_timeout(Duration::from_millis(10)).is_err() {
            drained.push(
                rx.recv_timeout(Duration::from_secs(1))
                    .expect("draining capacity should unblock stopped event sender"),
            );
        }
        sender.join().unwrap();

        let mut delivered = false;
        drained.extend(rx.try_iter());
        for _ in 0..UI_EVENT_CHANNEL_BOUND {
            let Some(event) = drained.pop().or_else(|| rx.try_recv().ok()) else {
                break;
            };
            if matches!(
                &event,
                UiEvent::Lsp(LspUiEvent::ServerStopped { language, root, generation })
                    if language == "rust" && root == &PathBuf::from("workspace") && *generation == 7
            ) {
                delivered = true;
                break;
            }
        }
        assert!(delivered);
    }

    #[test]
    fn read_error_status_sanitizes_message_but_preserves_raw_language_key() {
        let (tx, rx) = ui_event_channel();
        let language = format!(
            "rust\n{}\u{202e}",
            "language-fragment-".repeat(LSP_LANGUAGE_LABEL_MAX_CHARS)
        );
        let error = anyhow::anyhow!(
            "read failed\n{}\u{2066}",
            "error-fragment-".repeat(LSP_STATUS_MESSAGE_MAX_CHARS)
        );

        send_lsp_read_error_status(&language, &PathBuf::from("workspace"), 7, &error, &tx);

        let event = rx.try_recv().expect("status event should be queued");
        let UiEvent::Lsp(LspUiEvent::Status {
            language: event_language,
            root,
            generation,
            message,
        }) = event
        else {
            panic!("expected LSP status event");
        };
        assert_eq!(event_language, language);
        assert_eq!(root, PathBuf::from("workspace"));
        assert_eq!(generation, 7);
        assert_display_safe(&message);
        assert!(message.contains("..."));
        assert!(message.chars().count() <= LSP_STATUS_MESSAGE_MAX_CHARS);
    }

    #[test]
    fn stopped_status_sanitizes_message_but_preserves_raw_server_stopped_key() {
        let (tx, rx) = ui_event_channel();
        let language = format!(
            "rust\n{}\u{202e}",
            "language-fragment-".repeat(LSP_LANGUAGE_LABEL_MAX_CHARS)
        );

        send_lsp_stopped_status(&language, &PathBuf::from("workspace"), 7, None, &tx);

        let status_event = rx.try_recv().expect("status event should be queued");
        let UiEvent::Lsp(LspUiEvent::Status {
            language: status_language,
            message,
            ..
        }) = status_event
        else {
            panic!("expected LSP status event");
        };
        assert_eq!(status_language, language);
        assert_display_safe(&message);
        assert!(message.contains("..."));
        assert!(message.chars().count() <= LSP_STATUS_MESSAGE_MAX_CHARS);

        let stopped_event = rx
            .try_recv()
            .expect("server stopped event should be queued");
        assert!(matches!(
            stopped_event,
            UiEvent::Lsp(LspUiEvent::ServerStopped { language: event_language, root, generation })
                if event_language == language
                    && root == Path::new("workspace")
                    && generation == 7
        ));
    }

    #[test]
    fn stopped_status_sanitizes_exit_detail_into_bounded_message() {
        let (tx, rx) = ui_event_channel();
        let detail = format!(
            "exit code 1\nsecond line \u{2066}{}",
            "detail-fragment-".repeat(40)
        );

        send_lsp_stopped_status("rust", &PathBuf::from("workspace"), 7, Some(&detail), &tx);

        let status_event = rx.try_recv().expect("stopped status should be queued");
        let UiEvent::Lsp(LspUiEvent::Status { message, .. }) = status_event else {
            panic!("expected LSP status event");
        };
        assert!(message.starts_with("rust LSP stopped ("), "{message}");
        assert_display_safe(&message);
        assert!(message.contains("..."));
        assert!(message.chars().count() <= LSP_STATUS_MESSAGE_MAX_CHARS);
    }

    fn assert_display_safe(value: &str) {
        assert!(!value.chars().any(char::is_control), "{value:?}");
        assert!(!value.chars().any(is_bidi_format_control), "{value:?}");
    }

    fn is_bidi_format_control(ch: char) -> bool {
        matches!(
            ch,
            '\u{061c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
        )
    }
}
