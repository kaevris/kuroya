use super::lsp_unavailable_status_message;
use super::send_lsp_server_unavailable;
use crate::ui_event_channel::Sender;
use crate::{lsp_ui_events::LspUiEvent, ui_events::UiEvent};
use kuroya_core::LspServerConfig;
use std::path::Path;
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout};

pub(super) fn take_lsp_stdio(
    child: &mut Child,
    config: &LspServerConfig,
    root: &Path,
    generation: u64,
    ui_tx: &Sender<UiEvent>,
) -> Option<(ChildStdin, ChildStdout, Option<ChildStderr>)> {
    let Some(writer) = child.stdin.take() else {
        send_lsp_stdio_unavailable_status(config, root, generation, "missing stdin", ui_tx);
        return None;
    };
    let Some(stdout) = child.stdout.take() else {
        send_lsp_stdio_unavailable_status(config, root, generation, "missing stdout", ui_tx);
        return None;
    };
    // stderr is optional for capture; servers without one simply log nothing.
    let stderr = child.stderr.take();

    Some((writer, stdout, stderr))
}

fn send_lsp_stdio_unavailable_status(
    config: &LspServerConfig,
    root: &Path,
    generation: u64,
    detail: &str,
    ui_tx: &Sender<UiEvent>,
) {
    let _ = crate::ui_event_channel::send_critical_ui_event(
        ui_tx,
        UiEvent::Lsp(LspUiEvent::Status {
            language: config.language.clone(),
            root: root.to_path_buf(),
            generation,
            message: lsp_unavailable_status_message(&config.language, &config.command, detail),
        }),
    );
    send_lsp_server_unavailable(config, root, generation, ui_tx);
}

#[cfg(test)]
mod tests {
    use super::send_lsp_stdio_unavailable_status;
    use crate::{
        lsp_runtime::LSP_STATUS_MESSAGE_MAX_CHARS,
        lsp_ui_events::LspUiEvent,
        ui_event_channel::{UI_EVENT_CHANNEL_BOUND, ui_event_channel},
        ui_events::UiEvent,
    };
    use kuroya_core::LspServerConfig;
    use std::{path::PathBuf, thread, time::Duration};

    #[test]
    fn lsp_process_stdio_status_sanitizes_message_but_preserves_raw_language() {
        let (tx, rx) = ui_event_channel();
        let language = format!("rust\n{}\u{202e}", "language-fragment-".repeat(16));
        let config = LspServerConfig {
            language: language.clone(),
            command: "test-language-server".to_owned(),
            args: Vec::new(),
            extensions: Vec::new(),
            root_markers: Vec::new(),
            enabled: true,
        };

        send_lsp_stdio_unavailable_status(
            &config,
            &PathBuf::from("workspace"),
            9,
            "missing stdin\nignored \u{2066}",
            &tx,
        );

        let event = rx.try_recv().expect("stdio status should be queued");
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
        assert_eq!(generation, 9);
        assert_display_safe(&message);
        assert!(message.contains("..."));
        assert!(message.chars().count() <= LSP_STATUS_MESSAGE_MAX_CHARS);
    }

    #[test]
    fn lsp_process_stdio_status_survives_ui_backpressure() {
        let (tx, rx) = ui_event_channel();
        for index in 0..UI_EVENT_CHANNEL_BOUND {
            tx.try_send(UiEvent::Lsp(LspUiEvent::WorkDoneProgressCreated {
                token: format!("progress-token-{index}"),
            }))
            .expect("event should fit within channel bound");
        }
        let config = LspServerConfig {
            language: "rust".to_owned(),
            command: "test-language-server".to_owned(),
            args: Vec::new(),
            extensions: Vec::new(),
            root_markers: Vec::new(),
            enabled: true,
        };
        let status_tx = tx.clone();

        let sender = thread::spawn(move || {
            send_lsp_stdio_unavailable_status(
                &config,
                &PathBuf::from("workspace"),
                9,
                "missing stdin",
                &status_tx,
            );
        });

        let _ = rx
            .recv_timeout(Duration::from_secs(1))
            .expect("freeing capacity should unblock stdio status sender");
        sender.join().unwrap();

        let mut delivered = false;
        for _ in 0..UI_EVENT_CHANNEL_BOUND {
            let event = rx
                .recv_timeout(Duration::from_secs(1))
                .expect("stdio status should be queued after capacity is freed");
            if matches!(
                &event,
                UiEvent::Lsp(LspUiEvent::Status { language, root, generation, message })
                    if language == "rust"
                        && root == &PathBuf::from("workspace")
                        && *generation == 9
                        && message.starts_with(
                            "rust LSP unavailable: failed to start test-language-server:"
                        )
            ) {
                delivered = true;
                break;
            }
        }
        assert!(delivered);
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
