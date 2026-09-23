use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

/// Maximum stderr text retained per language server (a bounded ring of the
/// most recent lines).
pub(crate) const LSP_STDERR_LOG_MAX_CHARS: usize = 64 * 1024;

/// How many stderr characters are surfaced in a stopped status message.
pub(crate) const LSP_STDERR_STATUS_TAIL_CHARS: usize = 96;

/// Shared ring buffer of the most recent server stderr output (and
/// `window/logMessage` text). Cloning shares the same ring.
#[derive(Debug, Clone, Default)]
pub(crate) struct LspStderrLog(Arc<Mutex<LspStderrLogState>>);

#[derive(Debug, Default)]
struct LspStderrLogState {
    lines: VecDeque<String>,
    total_chars: usize,
}

impl LspStderrLog {
    /// Append one stderr line, evicting the oldest lines once the ring
    /// exceeds [`LSP_STDERR_LOG_MAX_CHARS`]. Oversized lines are truncated.
    pub(crate) fn push_line(&self, line: &str) {
        let line = bounded_line(line);
        let mut state = match self.0.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.total_chars += line.chars().count();
        state.lines.push_back(line);
        while state.total_chars > LSP_STDERR_LOG_MAX_CHARS {
            let Some(oldest) = state.lines.pop_front() else {
                state.total_chars = 0;
                break;
            };
            state.total_chars = state.total_chars.saturating_sub(oldest.chars().count());
        }
    }

    /// Last lines joined with newlines, bounded to `max_chars` characters
    /// (empty when nothing was captured). A single oversized newest line is
    /// truncated to the budget rather than dropped. The result is not
    /// display-safe on its own; callers must sanitize before showing it.
    pub(crate) fn tail_chars(&self, max_chars: usize) -> String {
        let state = match self.0.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        let mut selected = VecDeque::new();
        let mut used = 0usize;
        for line in state.lines.iter().rev() {
            let line_chars = line.chars().count();
            let separator = usize::from(!selected.is_empty());
            if used + line_chars + separator > max_chars {
                if selected.is_empty() && max_chars > 0 {
                    // Always surface the newest line, truncated to budget.
                    return line.chars().take(max_chars).collect();
                }
                break;
            }
            used += line_chars + separator;
            selected.push_front(line);
        }
        selected
            .into_iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn bounded_line(line: &str) -> String {
    if line.chars().count() <= LSP_STDERR_LOG_MAX_CHARS {
        return line.to_owned();
    }
    line.chars().take(LSP_STDERR_LOG_MAX_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::{LSP_STDERR_LOG_MAX_CHARS, LspStderrLog, bounded_line};

    #[test]
    fn stderr_log_keeps_recent_lines_in_order() {
        let log = LspStderrLog::default();
        assert_eq!(log.tail_chars(64), "");

        log.push_line("first");
        log.push_line("second");
        log.push_line("third");

        assert_eq!(log.tail_chars(64), "first\nsecond\nthird");
    }

    #[test]
    fn stderr_log_tail_is_bounded_by_requested_chars() {
        let log = LspStderrLog::default();
        log.push_line("alpha");
        log.push_line("beta");
        log.push_line("gamma");

        // "beta" + newline + "gamma" is exactly 10 chars.
        assert_eq!(log.tail_chars(10), "beta\ngamma");
        // One char short only fits the newest line whole.
        assert_eq!(log.tail_chars(9), "gamma");
        // The newest line alone overflows the budget, so it is truncated.
        assert_eq!(log.tail_chars(4), "gamm");
    }

    #[test]
    fn stderr_log_tail_truncates_an_oversized_newest_line() {
        let log = LspStderrLog::default();
        log.push_line(&"z".repeat(32));

        assert_eq!(log.tail_chars(8), "zzzzzzzz");
    }

    #[test]
    fn stderr_log_evicts_oldest_lines_once_capacity_is_exceeded() {
        let log = LspStderrLog::default();
        let line_chars = 1024;
        let line = "x".repeat(line_chars);
        for _ in 0..(LSP_STDERR_LOG_MAX_CHARS / line_chars) + 4 {
            log.push_line(&line);
        }

        let tail = log.tail_chars(LSP_STDERR_LOG_MAX_CHARS);
        assert!(tail.chars().count() <= LSP_STDERR_LOG_MAX_CHARS);
        // The ring holds 64 lines of 1024 chars; the tail budget of 64 KiB
        // must additionally fit the joining newlines, so only 63 whole lines
        // are surfaced: 63 * 1024 + 62 separators <= 64 KiB.
        let surfaced_lines = LSP_STDERR_LOG_MAX_CHARS / line_chars - 1;
        assert_eq!(tail.lines().count(), surfaced_lines);
    }

    #[test]
    fn stderr_log_truncates_oversized_lines_to_ring_capacity() {
        let oversized = "y".repeat(LSP_STDERR_LOG_MAX_CHARS + 1);
        let bounded = bounded_line(&oversized);

        assert_eq!(bounded.chars().count(), LSP_STDERR_LOG_MAX_CHARS);
    }
}
