use super::{
    TERMINAL_DRAIN_BYTE_BUDGET, TERMINAL_DRAIN_EVENT_BUDGET, TERMINAL_OUTPUT_CHANNEL_BOUND,
    TerminalPane, TerminalSession, terminal_close_channel, terminal_command_channel,
    terminal_output_channel,
};
use crate::terminal_process::{
    TerminalEvent, TerminalFinishReason, TerminalLaunch, run_pty,
    send_terminal_event_blocking_with_repaint, terminal_failure_message,
};
use egui::Context;
use portable_pty::PtySize;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::{thread, time::Instant};

impl TerminalPane {
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        if self.visible {
            self.focus_input_on_show = true;
            if self.sessions.is_empty() {
                self.open_new_session();
            } else if let Some(active) = self.active_session_index() {
                self.start_session_if_needed(active);
            }
        } else {
            self.fullscreen = false;
            if self.sessions.len() < 2 {
                self.split_view = false;
            }
        }
    }

    pub fn drain_output(&mut self) -> usize {
        self.prune_stale_session_state();

        let session_count = self.sessions.len();
        if session_count == 0 {
            return 0;
        }

        let start = self.active_session.min(session_count - 1);
        let mut budget =
            TerminalDrainBudget::new(TERMINAL_DRAIN_EVENT_BUDGET, TERMINAL_DRAIN_BYTE_BUDGET);
        let mut pending_sessions = self.pending_output_session_indices(start);
        let mut total = TerminalDrainStats::default();

        while !pending_sessions.is_empty() && budget.can_drain() {
            let round_budget = budget.fair_share(pending_sessions.len());
            let mut next_pending_sessions = Vec::new();
            let mut round_progress = false;

            for index in pending_sessions {
                if !budget.can_drain() {
                    if self.session_has_pending_output(index) {
                        next_pending_sessions.push(index);
                    }
                    continue;
                }

                let drain_budget = round_budget.clamp_to(budget);
                if !drain_budget.can_drain() {
                    break;
                }

                let Some(session) = self.sessions.get_mut(index) else {
                    continue;
                };
                let stats = session.drain_output(drain_budget.events, drain_budget.bytes);
                round_progress |= stats.events > 0;
                budget.consume(stats);
                total.add(stats);

                if self.session_has_pending_output(index) {
                    next_pending_sessions.push(index);
                }
            }

            if !round_progress {
                break;
            }
            pending_sessions = next_pending_sessions;
        }

        if self.enable_bell && total.bells > 0 {
            self.last_bell_at = Some(Instant::now());
        }
        self.prune_stale_session_state();
        if total.events > 0 && self.has_pending_output() {
            self.request_output_repaint();
        }
        total.events
    }

    pub fn has_pending_output(&self) -> bool {
        self.sessions
            .iter()
            .any(|session| !session.rx_output.is_empty())
    }

    fn pending_output_session_indices(&self, start: usize) -> Vec<usize> {
        let session_count = self.sessions.len();
        let mut indices = Vec::with_capacity(session_count);
        for offset in 0..session_count {
            let index = (start + offset) % session_count;
            if self.session_has_pending_output(index) {
                indices.push(index);
            }
        }
        indices
    }

    fn session_has_pending_output(&self, index: usize) -> bool {
        self.sessions
            .get(index)
            .is_some_and(|session| !session.rx_output.is_empty())
    }

    fn request_output_repaint(&self) {
        if let Some(ctx) = &self.repaint_context {
            ctx.request_repaint();
        }
    }

    pub fn drain_output_for_shutdown(&mut self) -> usize {
        let max_events = self
            .sessions
            .len()
            .max(1)
            .saturating_mul(TERMINAL_OUTPUT_CHANNEL_BOUND);
        let mut drained = 0usize;
        while self.has_pending_output() && drained < max_events {
            let events = self.drain_output();
            if events == 0 {
                break;
            }
            drained = drained.saturating_add(events);
        }
        drained
    }

    pub(super) fn start_session_if_needed(&mut self, index: usize) {
        let Some(cwd) = self.sessions.get(index).and_then(|session| {
            session.can_auto_start_shell().then(|| {
                session
                    .initial_cwd
                    .clone()
                    .unwrap_or_else(|| self.launch_cwd())
            })
        }) else {
            return;
        };
        let repaint_context = self.repaint_context.clone();
        let Some(session) = self.sessions.get_mut(index) else {
            return;
        };
        session.start(
            &cwd,
            self.last_size,
            self.shell_path.clone(),
            self.shell_args.clone(),
            self.show_exit_alert,
            repaint_context,
        );
    }
}

impl TerminalSession {
    pub(super) fn start(
        &mut self,
        cwd: &Path,
        initial_size: PtySize,
        shell_path: Option<String>,
        shell_args: Vec<String>,
        show_exit_alert: bool,
        repaint_context: Option<Context>,
    ) {
        self.remember_shell_launch(shell_path.clone(), shell_args.clone());
        self.start_launch(
            cwd,
            initial_size,
            TerminalLaunch::Shell {
                shell_path,
                shell_args,
            },
            show_exit_alert,
            None,
            true,
            repaint_context,
        );
    }

    fn remember_shell_launch(&mut self, shell_path: Option<String>, shell_args: Vec<String>) {
        self.launch_shell_path = shell_path;
        self.launch_shell_args = shell_args;
    }

    pub(super) fn start_process(
        &mut self,
        cwd: &Path,
        initial_size: PtySize,
        program: String,
        args: Vec<String>,
        env: std::collections::BTreeMap<String, String>,
        show_exit_alert: bool,
        label: String,
        repaint_context: Option<Context>,
    ) {
        self.start_launch(
            cwd,
            initial_size,
            TerminalLaunch::Process { program, args, env },
            show_exit_alert,
            Some(label),
            false,
            repaint_context,
        );
    }

    fn start_launch(
        &mut self,
        cwd: &Path,
        initial_size: PtySize,
        launch: TerminalLaunch,
        show_exit_alert: bool,
        label: Option<String>,
        auto_start_shell: bool,
        repaint_context: Option<Context>,
    ) {
        if self.started {
            return;
        }
        self.started = true;
        self.replace_output_channel_for_launch();
        self.reset_search_output_decoder();
        self.reset_shell_integration_state();
        self.last_process_exit_code = None;
        self.last_process_terminal_error = false;
        self.auto_start_shell = auto_start_shell;
        self.initial_cwd = Some(cwd.to_path_buf());
        self.process_label = label;
        self.close_requested.store(false, Ordering::SeqCst);

        let (tx_command, rx_command) = terminal_command_channel();
        let (tx_close, rx_close) = terminal_close_channel();
        self.tx_command = Some(tx_command);
        self.tx_close = Some(tx_close);
        let tx_output = self.tx_output.clone();
        let cwd = cwd.to_path_buf();
        let close_requested = self.close_requested.clone();

        thread::spawn(move || {
            let error_repaint_context = repaint_context.clone();
            let result = run_pty(
                cwd,
                initial_size,
                launch,
                show_exit_alert,
                rx_command,
                rx_close,
                close_requested,
                tx_output.clone(),
                repaint_context,
            );
            if let Err(error) = result {
                let _ = send_terminal_event_blocking_with_repaint(
                    &tx_output,
                    TerminalEvent::Finished {
                        message: Some(terminal_failure_message("terminal error", error)),
                        process_exit_code: None,
                        reason: TerminalFinishReason::TerminalError,
                    },
                    error_repaint_context.as_ref(),
                );
            }
        });
    }

    fn drain_output(&mut self, event_budget: usize, byte_budget: usize) -> TerminalDrainStats {
        let mut stats = TerminalDrainStats::default();
        while stats.events < event_budget && stats.bytes < byte_budget {
            let Ok(event) = self.rx_output.try_recv() else {
                break;
            };
            stats.events = stats.events.saturating_add(1);
            match event {
                TerminalEvent::Output(chunk) => {
                    if !self.started {
                        continue;
                    }
                    self.process_terminal_output(&chunk, &mut stats);
                }
                TerminalEvent::Finished {
                    message,
                    process_exit_code,
                    reason,
                } => {
                    if !self.started {
                        continue;
                    }
                    if let Some(message) = message {
                        self.process_terminal_output(message.as_bytes(), &mut stats);
                    }
                    if reason == TerminalFinishReason::TerminalError {
                        self.last_process_terminal_error = true;
                        self.last_process_exit_code = None;
                    } else if !self.last_process_terminal_error {
                        self.last_process_exit_code = process_exit_code;
                    }
                    self.mark_stopped();
                }
            }
        }
        stats
    }

    fn process_terminal_output(&mut self, output: &[u8], stats: &mut TerminalDrainStats) {
        stats.bytes = stats.bytes.saturating_add(output.len());
        let mut cursor = 0;
        for clear_end in terminal_scrollback_clear_sequence_ends(output) {
            self.process_terminal_output_segment(&output[cursor..clear_end]);
            if self.take_pending_scrollback_clear() {
                self.clear_scrollback_from_terminal();
            }
            cursor = clear_end;
        }
        self.process_terminal_output_segment(&output[cursor..]);
        if self.take_pending_scrollback_clear() {
            self.clear_scrollback_from_terminal();
        }
        self.flush_terminal_responses();
        stats.bells = stats.bells.saturating_add(self.take_pending_bells());
    }

    fn process_terminal_output_segment(&mut self, output: &[u8]) {
        if output.is_empty() {
            return;
        }
        self.append_search_output(output);
        self.parser.process(output);
    }

    pub(super) fn mark_stopped(&mut self) {
        self.started = false;
        self.tx_command = None;
        self.tx_close = None;
        self.reset_search_output_decoder();
    }

    fn replace_output_channel_for_launch(&mut self) {
        let (tx_output, rx_output) = terminal_output_channel();
        self.tx_output = tx_output;
        self.rx_output = rx_output;
    }

    #[cfg(test)]
    pub(super) fn replace_output_channel_for_launch_for_test(&mut self) {
        self.replace_output_channel_for_launch();
    }

    fn can_auto_start_shell(&self) -> bool {
        !self.started && self.auto_start_shell
    }

    fn take_pending_bells(&mut self) -> usize {
        std::mem::take(&mut self.parser.callbacks_mut().pending_bells)
    }

    fn take_pending_scrollback_clear(&mut self) -> bool {
        std::mem::take(&mut self.parser.callbacks_mut().pending_clear_scrollback)
    }
}

fn terminal_scrollback_clear_sequence_ends(output: &[u8]) -> Vec<usize> {
    let mut clear_ends = Vec::new();
    let mut index = 0;
    while index < output.len() {
        let Some((end, clears_scrollback)) = terminal_csi_sequence_end(output, index) else {
            index += 1;
            continue;
        };
        if clears_scrollback {
            clear_ends.push(end);
        }
        index = end;
    }
    clear_ends
}

fn terminal_csi_sequence_end(output: &[u8], index: usize) -> Option<(usize, bool)> {
    let mut cursor = match output.get(index..index.saturating_add(2)) {
        Some(b"\x1b[") => index + 2,
        _ if output.get(index) == Some(&0x9b) => index + 1,
        _ => return None,
    };

    let private = output.get(cursor) == Some(&b'?');
    if private {
        cursor += 1;
    }
    while output.get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    let clears_scrollback = output.get(cursor) == Some(&b'J')
        && terminal_csi_first_numeric_param(&output[index..cursor]) == Some(3);
    Some((cursor.saturating_add(1), clears_scrollback))
}

fn terminal_csi_first_numeric_param(sequence_prefix: &[u8]) -> Option<u16> {
    let start = sequence_prefix
        .iter()
        .rposition(|byte| *byte == b'[' || *byte == 0x9b)?
        .saturating_add(1);
    let start = if sequence_prefix.get(start) == Some(&b'?') {
        start + 1
    } else {
        start
    };
    let mut value = 0_u16;
    let mut saw_digit = false;
    for byte in &sequence_prefix[start..] {
        if !byte.is_ascii_digit() {
            break;
        }
        saw_digit = true;
        value = value
            .saturating_mul(10)
            .saturating_add(u16::from(byte - b'0'));
    }
    saw_digit.then_some(value)
}

#[derive(Clone, Copy, Debug, Default)]
struct TerminalDrainBudget {
    events: usize,
    bytes: usize,
}

impl TerminalDrainBudget {
    fn new(events: usize, bytes: usize) -> Self {
        Self { events, bytes }
    }

    fn can_drain(&self) -> bool {
        self.events > 0 && self.bytes > 0
    }

    fn fair_share(&self, session_count: usize) -> Self {
        if session_count == 0 || !self.can_drain() {
            return Self::default();
        }
        Self {
            events: self.events.div_ceil(session_count),
            bytes: self.bytes.div_ceil(session_count),
        }
    }

    fn clamp_to(&self, remaining: Self) -> Self {
        Self {
            events: self.events.min(remaining.events),
            bytes: self.bytes.min(remaining.bytes),
        }
    }

    fn consume(&mut self, stats: TerminalDrainStats) {
        self.events = self.events.saturating_sub(stats.events);
        self.bytes = self.bytes.saturating_sub(stats.bytes);
    }
}

#[derive(Clone, Copy, Default)]
struct TerminalDrainStats {
    events: usize,
    bytes: usize,
    bells: usize,
}

impl TerminalDrainStats {
    fn add(&mut self, other: Self) {
        self.events = self.events.saturating_add(other.events);
        self.bytes = self.bytes.saturating_add(other.bytes);
        self.bells = self.bells.saturating_add(other.bells);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn pane_for_lifecycle_tests() -> TerminalPane {
        TerminalPane::new(PathBuf::from("workspace"), 100, 12.0, 1.2)
    }

    #[test]
    fn terminal_drain_budget_fair_share_rounds_up() {
        let budget = TerminalDrainBudget::new(10, 1025);

        let share = budget.fair_share(4);

        assert_eq!(share.events, 3);
        assert_eq!(share.bytes, 257);
    }

    #[test]
    fn terminal_drain_budget_consumes_oversized_events_saturating() {
        let mut budget = TerminalDrainBudget::new(4, 32);

        budget.consume(TerminalDrainStats {
            events: 1,
            bytes: 128,
            bells: 0,
        });

        assert_eq!(budget.events, 3);
        assert_eq!(budget.bytes, 0);
        assert!(!budget.can_drain());
    }

    #[test]
    fn terminal_scrollback_clear_sequence_detection_finds_csi_3j_variants() {
        let output = b"old\x1b[H\x1b[2J\x1b[3Jnew\x9b?3Jtail";
        let ends = terminal_scrollback_clear_sequence_ends(output);

        assert_eq!(ends.len(), 2);
        assert_eq!(&output[ends[0]..], b"new\x9b?3Jtail");
        assert_eq!(&output[ends[1]..], b"tail");
    }

    #[test]
    fn terminal_drain_ignores_finished_status_for_already_stopped_session() {
        let mut pane = pane_for_lifecycle_tests();
        let _rx = pane.add_process_session_for_test(1);
        pane.sessions[0].last_process_exit_code = Some(0);
        pane.sessions[0].mark_stopped();

        pane.sessions[0]
            .tx_output
            .send(TerminalEvent::Finished {
                message: Some("late terminal error".to_owned()),
                process_exit_code: Some(17),
                reason: TerminalFinishReason::TerminalError,
            })
            .unwrap();

        assert_eq!(pane.drain_output(), 1);
        assert_eq!(pane.sessions[0].last_process_exit_code, Some(0));
        assert!(!pane.sessions[0].last_process_terminal_error);
        assert!(
            !pane.sessions[0]
                .parser
                .screen()
                .contents()
                .contains("late terminal error")
        );
    }

    #[test]
    fn terminal_drain_ignores_output_after_finished_status() {
        let mut pane = pane_for_lifecycle_tests();
        let _rx = pane.add_process_session_for_test(1);

        pane.sessions[0]
            .tx_output
            .send(TerminalEvent::Output(b"before\r\n".to_vec()))
            .unwrap();
        pane.sessions[0]
            .tx_output
            .send(TerminalEvent::Finished {
                message: None,
                process_exit_code: Some(0),
                reason: TerminalFinishReason::ProcessExit,
            })
            .unwrap();
        pane.sessions[0]
            .tx_output
            .send(TerminalEvent::Output(b"after\r\n".to_vec()))
            .unwrap();

        assert_eq!(pane.drain_output(), 3);
        assert!(!pane.sessions[0].started);
        assert_eq!(pane.sessions[0].last_process_exit_code, Some(0));
        assert!(pane.sessions[0].search_buffer.contains("before"));
        assert!(!pane.sessions[0].search_buffer.contains("after"));
        assert!(
            pane.sessions[0]
                .parser
                .screen()
                .contents()
                .contains("before")
        );
        assert!(
            !pane.sessions[0]
                .parser
                .screen()
                .contents()
                .contains("after")
        );
    }
}
