use std::borrow::Cow;

pub(super) const TERMINAL_SEARCH_CONTROL_SEQUENCE_MAX_CHARS: usize = 4096;

const C1_DCS: char = '\u{90}';
const C1_CSI: char = '\u{9b}';
const C1_ST: char = '\u{9c}';
const C1_OSC: char = '\u{9d}';
const C1_SOS: char = '\u{98}';
const C1_PM: char = '\u{9e}';
const C1_APC: char = '\u{9f}';

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::terminal) enum TerminalSearchAnsiState {
    #[default]
    Ground,
    Escape,
    Csi(usize),
    CsiOverflow,
    Osc(usize),
    OscEscape(usize),
    OscOverflow,
    OscOverflowEscape,
    ControlString(usize),
    ControlStringEscape(usize),
    ControlStringOverflow,
    ControlStringOverflowEscape,
}

pub(in crate::terminal) fn terminal_plain_text(bytes: &[u8]) -> String {
    let mut output = String::new();
    let mut pending_carriage_return = false;
    let mut ansi_state = TerminalSearchAnsiState::default();
    let mut utf8_tail = Vec::new();
    let mut line_count = 0usize;
    append_terminal_plain_text(
        &mut output,
        &mut line_count,
        &mut pending_carriage_return,
        &mut ansi_state,
        &mut utf8_tail,
        bytes,
    );
    flush_terminal_search_utf8_tail_as_replacement(
        &mut output,
        &mut line_count,
        &mut pending_carriage_return,
        &mut ansi_state,
        &mut utf8_tail,
    );
    if pending_carriage_return {
        output.push('\n');
    }
    output
}

pub(super) fn append_terminal_plain_text(
    output: &mut String,
    line_count: &mut usize,
    pending_carriage_return: &mut bool,
    state: &mut TerminalSearchAnsiState,
    utf8_tail: &mut Vec<u8>,
    bytes: &[u8],
) -> bool {
    if append_terminal_plain_text_fast_path(
        output,
        line_count,
        pending_carriage_return,
        state,
        utf8_tail,
        bytes,
    ) {
        return true;
    }

    let bytes = if utf8_tail.is_empty() {
        Cow::Borrowed(bytes)
    } else {
        let mut combined = Vec::with_capacity(utf8_tail.len() + bytes.len());
        combined.extend_from_slice(utf8_tail);
        combined.extend_from_slice(bytes);
        utf8_tail.clear();
        Cow::Owned(combined)
    };
    let mut changed = false;
    let mut remaining = bytes.as_ref();

    loop {
        match std::str::from_utf8(remaining) {
            Ok(text) => {
                changed |= append_terminal_plain_text_chars(
                    output,
                    line_count,
                    pending_carriage_return,
                    state,
                    text.chars(),
                );
                break;
            }
            Err(error) => {
                let valid_up_to = error.valid_up_to();
                if valid_up_to > 0 {
                    let text = std::str::from_utf8(&remaining[..valid_up_to]).unwrap_or_default();
                    changed |= append_terminal_plain_text_chars(
                        output,
                        line_count,
                        pending_carriage_return,
                        state,
                        text.chars(),
                    );
                }

                match error.error_len() {
                    Some(invalid_len) => {
                        let invalid_byte = remaining[valid_up_to];

                        let ch =
                            terminal_search_c1_control_char(invalid_byte).unwrap_or('\u{fffd}');
                        changed |= append_terminal_plain_text_chars(
                            output,
                            line_count,
                            pending_carriage_return,
                            state,
                            std::iter::once(ch),
                        );
                        remaining = &remaining[valid_up_to + invalid_len..];
                    }
                    None => {
                        utf8_tail.extend_from_slice(&remaining[valid_up_to..]);
                        break;
                    }
                }
            }
        }
    }

    changed
}

fn flush_terminal_search_utf8_tail_as_replacement(
    output: &mut String,
    line_count: &mut usize,
    pending_carriage_return: &mut bool,
    state: &mut TerminalSearchAnsiState,
    utf8_tail: &mut Vec<u8>,
) -> bool {
    if utf8_tail.is_empty() {
        return false;
    }

    utf8_tail.clear();
    append_terminal_plain_text_chars(
        output,
        line_count,
        pending_carriage_return,
        state,
        std::iter::once('\u{fffd}'),
    )
}

fn append_terminal_plain_text_fast_path(
    output: &mut String,
    line_count: &mut usize,
    pending_carriage_return: &bool,
    state: &TerminalSearchAnsiState,
    utf8_tail: &[u8],
    bytes: &[u8],
) -> bool {
    if bytes.is_empty()
        || *pending_carriage_return
        || *state != TerminalSearchAnsiState::Ground
        || !utf8_tail.is_empty()
    {
        return false;
    }

    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    if !text.chars().all(is_terminal_search_fast_path_char) {
        return false;
    }

    let starts_new_line = output.is_empty() || output.ends_with('\n');
    *line_count = line_count.saturating_add(terminal_search_line_count_delta_for_append(
        text,
        starts_new_line,
    ));
    output.reserve(text.len());
    output.push_str(text);
    true
}

fn is_terminal_search_fast_path_char(ch: char) -> bool {
    matches!(ch, '\n' | '\t')
        || (!ch.is_control()
            && !matches!(
                ch,
                C1_CSI | C1_OSC | C1_DCS | C1_SOS | C1_PM | C1_APC | C1_ST
            ))
}

fn terminal_search_line_count_delta_for_append(text: &str, starts_new_line: bool) -> usize {
    if text.is_empty() {
        return 0;
    }

    usize::from(starts_new_line)
        .saturating_add(terminal_search_newline_count(text))
        .saturating_sub(usize::from(text.ends_with('\n')))
}

fn terminal_search_newline_count(buffer: &str) -> usize {
    buffer
        .as_bytes()
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
}

fn terminal_search_c1_control_char(byte: u8) -> Option<char> {
    match byte {
        0x80..=0x9f => char::from_u32(u32::from(byte)),
        _ => None,
    }
}

fn append_terminal_plain_text_chars(
    output: &mut String,
    line_count: &mut usize,
    pending_carriage_return: &mut bool,
    state: &mut TerminalSearchAnsiState,
    chars: impl IntoIterator<Item = char>,
) -> bool {
    let mut changed = false;

    for ch in chars {
        match *state {
            TerminalSearchAnsiState::Ground => match ch {
                '\u{1b}' => {
                    changed |=
                        apply_pending_carriage_return(output, line_count, pending_carriage_return);
                    *state = TerminalSearchAnsiState::Escape;
                }
                C1_CSI => {
                    changed |=
                        apply_pending_carriage_return(output, line_count, pending_carriage_return);
                    *state = TerminalSearchAnsiState::Csi(0);
                }
                C1_OSC => {
                    changed |=
                        apply_pending_carriage_return(output, line_count, pending_carriage_return);
                    *state = TerminalSearchAnsiState::Osc(0);
                }
                C1_DCS | C1_SOS | C1_PM | C1_APC => {
                    changed |=
                        apply_pending_carriage_return(output, line_count, pending_carriage_return);
                    *state = TerminalSearchAnsiState::ControlString(0);
                }
                '\r' => {
                    *pending_carriage_return = true;
                }
                '\n' => {
                    push_terminal_search_char(output, line_count, '\n');
                    *pending_carriage_return = false;
                    changed = true;
                }
                '\u{8}' => {
                    changed |=
                        apply_pending_carriage_return(output, line_count, pending_carriage_return);
                    if !output.ends_with('\n') {
                        changed |= pop_terminal_search_char(output, line_count);
                    }
                }
                '\t' => {
                    apply_pending_carriage_return(output, line_count, pending_carriage_return);
                    push_terminal_search_char(output, line_count, ch);
                    changed = true;
                }
                ch if ch.is_control() => {
                    changed |=
                        apply_pending_carriage_return(output, line_count, pending_carriage_return);
                }
                _ => {
                    apply_pending_carriage_return(output, line_count, pending_carriage_return);
                    push_terminal_search_char(output, line_count, ch);
                    changed = true;
                }
            },
            TerminalSearchAnsiState::Escape => {
                *state = match ch {
                    '[' => TerminalSearchAnsiState::Csi(0),
                    ']' => TerminalSearchAnsiState::Osc(0),
                    'P' | 'X' | '^' | '_' => TerminalSearchAnsiState::ControlString(0),
                    C1_CSI => TerminalSearchAnsiState::Csi(0),
                    C1_OSC => TerminalSearchAnsiState::Osc(0),
                    C1_DCS | C1_SOS | C1_PM | C1_APC => TerminalSearchAnsiState::ControlString(0),
                    _ => TerminalSearchAnsiState::Ground,
                };
            }
            TerminalSearchAnsiState::Csi(count) => {
                if ('@'..='~').contains(&ch) {
                    *state = TerminalSearchAnsiState::Ground;
                } else {
                    *state = terminal_search_bounded_ansi_state(
                        count,
                        TerminalSearchAnsiState::Csi,
                        TerminalSearchAnsiState::CsiOverflow,
                    );
                }
            }
            TerminalSearchAnsiState::CsiOverflow => {
                if ('@'..='~').contains(&ch) {
                    *state = TerminalSearchAnsiState::Ground;
                }
            }
            TerminalSearchAnsiState::Osc(count) => match ch {
                '\u{7}' | C1_ST => *state = TerminalSearchAnsiState::Ground,
                '\u{1b}' => {
                    *state = terminal_search_bounded_ansi_state(
                        count,
                        TerminalSearchAnsiState::OscEscape,
                        TerminalSearchAnsiState::OscOverflowEscape,
                    );
                }
                _ => {
                    *state = terminal_search_bounded_ansi_state(
                        count,
                        TerminalSearchAnsiState::Osc,
                        TerminalSearchAnsiState::OscOverflow,
                    );
                }
            },
            TerminalSearchAnsiState::OscEscape(count) => {
                *state = if ch == '\\' || ch == C1_ST {
                    TerminalSearchAnsiState::Ground
                } else {
                    terminal_search_bounded_ansi_state(
                        count,
                        TerminalSearchAnsiState::Osc,
                        TerminalSearchAnsiState::OscOverflow,
                    )
                };
            }
            TerminalSearchAnsiState::OscOverflow => match ch {
                '\u{7}' | C1_ST => *state = TerminalSearchAnsiState::Ground,
                '\u{1b}' => *state = TerminalSearchAnsiState::OscOverflowEscape,
                ch if is_terminal_search_overflow_recovery_char(ch) => {
                    *state = TerminalSearchAnsiState::Ground;
                    changed |= append_terminal_search_overflow_recovery_char(
                        output,
                        line_count,
                        pending_carriage_return,
                        ch,
                    );
                }
                _ => {}
            },
            TerminalSearchAnsiState::OscOverflowEscape => {
                *state = if ch == '\\' || ch == C1_ST {
                    TerminalSearchAnsiState::Ground
                } else if is_terminal_search_overflow_recovery_char(ch) {
                    changed |= append_terminal_search_overflow_recovery_char(
                        output,
                        line_count,
                        pending_carriage_return,
                        ch,
                    );
                    TerminalSearchAnsiState::Ground
                } else {
                    TerminalSearchAnsiState::OscOverflow
                };
            }
            TerminalSearchAnsiState::ControlString(count) => {
                if ch == C1_ST {
                    *state = TerminalSearchAnsiState::Ground;
                } else if ch == '\u{1b}' {
                    *state = terminal_search_bounded_ansi_state(
                        count,
                        TerminalSearchAnsiState::ControlStringEscape,
                        TerminalSearchAnsiState::ControlStringOverflowEscape,
                    );
                } else {
                    *state = terminal_search_bounded_ansi_state(
                        count,
                        TerminalSearchAnsiState::ControlString,
                        TerminalSearchAnsiState::ControlStringOverflow,
                    );
                }
            }
            TerminalSearchAnsiState::ControlStringEscape(count) => {
                *state = if ch == '\\' || ch == C1_ST {
                    TerminalSearchAnsiState::Ground
                } else {
                    terminal_search_bounded_ansi_state(
                        count,
                        TerminalSearchAnsiState::ControlString,
                        TerminalSearchAnsiState::ControlStringOverflow,
                    )
                };
            }
            TerminalSearchAnsiState::ControlStringOverflow => match ch {
                C1_ST => *state = TerminalSearchAnsiState::Ground,
                '\u{1b}' => *state = TerminalSearchAnsiState::ControlStringOverflowEscape,
                ch if is_terminal_search_overflow_recovery_char(ch) => {
                    *state = TerminalSearchAnsiState::Ground;
                    changed |= append_terminal_search_overflow_recovery_char(
                        output,
                        line_count,
                        pending_carriage_return,
                        ch,
                    );
                }
                _ => {}
            },
            TerminalSearchAnsiState::ControlStringOverflowEscape => {
                *state = if ch == '\\' || ch == C1_ST {
                    TerminalSearchAnsiState::Ground
                } else if is_terminal_search_overflow_recovery_char(ch) {
                    changed |= append_terminal_search_overflow_recovery_char(
                        output,
                        line_count,
                        pending_carriage_return,
                        ch,
                    );
                    TerminalSearchAnsiState::Ground
                } else {
                    TerminalSearchAnsiState::ControlStringOverflow
                };
            }
        }
    }

    changed
}

fn terminal_search_bounded_ansi_state(
    count: usize,
    state: impl FnOnce(usize) -> TerminalSearchAnsiState,
    overflow: TerminalSearchAnsiState,
) -> TerminalSearchAnsiState {
    let count = count.saturating_add(1);
    if count > TERMINAL_SEARCH_CONTROL_SEQUENCE_MAX_CHARS {
        overflow
    } else {
        state(count)
    }
}

fn is_terminal_search_overflow_recovery_char(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\r' | '\n')
}

fn append_terminal_search_overflow_recovery_char(
    output: &mut String,
    line_count: &mut usize,
    pending_carriage_return: &mut bool,
    ch: char,
) -> bool {
    match ch {
        '\r' => {
            *pending_carriage_return = true;
            false
        }
        '\n' => {
            push_terminal_search_char(output, line_count, '\n');
            *pending_carriage_return = false;
            true
        }
        _ => {
            apply_pending_carriage_return(output, line_count, pending_carriage_return);
            push_terminal_search_char(output, line_count, ch);
            true
        }
    }
}

fn apply_pending_carriage_return(
    output: &mut String,
    line_count: &mut usize,
    pending_carriage_return: &mut bool,
) -> bool {
    if !std::mem::take(pending_carriage_return) {
        return false;
    }

    let line_start = output.rfind('\n').map_or(0, |index| index + 1);
    if line_start == output.len() {
        return false;
    }
    output.truncate(line_start);
    *line_count = line_count.saturating_sub(1);
    true
}

fn push_terminal_search_char(output: &mut String, line_count: &mut usize, ch: char) {
    if output.is_empty() || output.ends_with('\n') {
        *line_count = line_count.saturating_add(1);
    }
    output.push(ch);
}

fn pop_terminal_search_char(output: &mut String, line_count: &mut usize) -> bool {
    let Some(_) = output.pop() else {
        return false;
    };
    if output.is_empty() || output.ends_with('\n') {
        *line_count = line_count.saturating_sub(1);
    }
    true
}
