use eframe::egui::{Key, Modifiers};
use kuroya_core::{Command, TextBuffer};
use std::cell::RefCell;

use super::input_edit::{
    EditorVimInputEdit, vim_clear_input, vim_delete_input_word_backward, vim_input_control_edit,
    vim_pop_input, vim_push_input, vim_take_input,
};
use super::search::{vim_search_input_accept_key, vim_search_input_cancel_key};
use super::state::{vim_adjust_marks_for_edit, vim_registers_summary};
use super::{
    EditorVimLastChange, EditorVimPendingKey, VimKeyResult, vim_line_first_non_whitespace_char,
    vim_printable_key_char, vim_set_previous_context_mark,
};

const VIM_COMMAND_SUBSTITUTE_MAX_MATCHES: usize = 10_000;
const VIM_COMMAND_STATUS_ECHO_MAX_CHARS: usize = 96;

thread_local! {
    pub(super) static VIM_COMMAND_INPUT: RefCell<String> = const { RefCell::new(String::new()) };

    static VIM_COMMAND_STATUS_MESSAGE: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub(super) fn handle_vim_command_input_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    pending: &mut Option<EditorVimPendingKey>,
    last_change: &mut Option<EditorVimLastChange>,
    suppress_text: Option<char>,
) -> VimKeyResult {
    if vim_command_input_accept_key(key, modifiers) {
        *pending = None;
        return vim_finish_pending_command(buffer, last_change);
    }

    if let Some(edit) = vim_command_input_control_edit(key, modifiers) {
        match edit {
            EditorVimInputEdit::DeleteCharBackward => vim_pop_input(&VIM_COMMAND_INPUT),
            EditorVimInputEdit::Clear => vim_clear_command_input(),
            EditorVimInputEdit::DeleteWordBackward => {
                vim_delete_input_word_backward(&VIM_COMMAND_INPUT)
            }
        }
        *pending = Some(EditorVimPendingKey::CommandInput);
        return VimKeyResult::handled(None);
    }

    if let Some(ch) = vim_printable_key_char(key, modifiers) {
        vim_push_input(&VIM_COMMAND_INPUT, ch);
        *pending = Some(EditorVimPendingKey::CommandInput);
        return VimKeyResult::handled(suppress_text);
    }

    *pending = Some(EditorVimPendingKey::CommandInput);
    VimKeyResult::handled(None)
}

pub(super) fn vim_command_input_control_edit(
    key: Key,
    modifiers: Modifiers,
) -> Option<EditorVimInputEdit> {
    vim_input_control_edit(key, modifiers)
}

pub(super) fn vim_command_input_accept_key(key: Key, modifiers: Modifiers) -> bool {
    vim_search_input_accept_key(key, modifiers)
}

pub(super) fn vim_command_input_cancel_key(key: Key, modifiers: Modifiers) -> bool {
    vim_search_input_cancel_key(key, modifiers)
}

pub(crate) fn vim_clear_command_input() {
    vim_clear_input(&VIM_COMMAND_INPUT);
    vim_clear_command_status_message();
}

pub(crate) fn vim_command_status_message() -> Option<String> {
    VIM_COMMAND_STATUS_MESSAGE.with(|message| message.borrow().clone())
}

fn vim_set_command_status_message(message: String) {
    VIM_COMMAND_STATUS_MESSAGE.with(|slot| *slot.borrow_mut() = Some(message));
}

fn vim_clear_command_status_message() {
    VIM_COMMAND_STATUS_MESSAGE.with(|slot| *slot.borrow_mut() = None);
}

fn vim_finish_pending_command(
    buffer: &mut TextBuffer,
    last_change: &mut Option<EditorVimLastChange>,
) -> VimKeyResult {
    let command = vim_take_input(&VIM_COMMAND_INPUT);
    let Some(parsed) = vim_parse_ex_command(&command) else {
        vim_set_command_status_message(format!(
            "Unknown command: {}",
            vim_echo_command_text(&command)
        ));
        return VimKeyResult::handled(None);
    };

    match parsed {
        EditorVimExCommand::Substitute(substitute) => {
            let replaced = vim_apply_substitute_command(buffer, &substitute);
            if replaced {
                *last_change = Some(EditorVimLastChange {
                    action: super::EditorVimRepeatAction::Substitute {
                        query: substitute.query,
                        replacement: substitute.replacement,
                        global: substitute.global,
                    },
                    count: 1,
                    insert_replay: Vec::new(),
                });
                vim_clear_command_status_message();
                VimKeyResult::changed(None)
            } else {
                vim_set_command_status_message(format!(
                    "Pattern not found: {}",
                    vim_echo_command_text(&substitute.query)
                ));
                VimKeyResult::handled(None)
            }
        }
        EditorVimExCommand::GoToLine(line) => {
            vim_set_previous_context_mark(buffer);
            let last_line = buffer.len_lines().saturating_sub(1);
            let line = line.saturating_sub(1).min(last_line);
            buffer.set_single_cursor(vim_line_first_non_whitespace_char(buffer, line));
            vim_clear_command_status_message();
            VimKeyResult::handled(None)
        }
        EditorVimExCommand::Registers | EditorVimExCommand::Display => {
            match vim_registers_summary() {
                Some(summary) => vim_set_command_status_message(summary),
                None => vim_clear_command_status_message(),
            }
            VimKeyResult::handled(None)
        }
        EditorVimExCommand::Write | EditorVimExCommand::WriteForce => {
            vim_ex_command_result(Command::SaveActive)
        }

        EditorVimExCommand::WriteQuit
        | EditorVimExCommand::WriteQuitForce
        | EditorVimExCommand::Exit
        | EditorVimExCommand::Quit
        | EditorVimExCommand::QuitForce => vim_ex_command_result(Command::CloseActive),
        EditorVimExCommand::WriteAll => vim_ex_command_result(Command::SaveAll),
    }
}

fn vim_ex_command_result(command: Command) -> VimKeyResult {
    vim_clear_command_status_message();
    VimKeyResult::command(command, None)
}

fn vim_echo_command_text(text: &str) -> String {
    text.chars()
        .take(VIM_COMMAND_STATUS_ECHO_MAX_CHARS)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EditorVimExCommand {
    Substitute(EditorVimSubstituteCommand),

    GoToLine(usize),
    Registers,
    Display,
    Write,
    WriteForce,
    WriteQuit,
    WriteQuitForce,
    Exit,
    Quit,
    QuitForce,
    WriteAll,
}

fn vim_parse_ex_command(command: &str) -> Option<EditorVimExCommand> {
    if let Some(substitute) = vim_parse_substitute_command(command) {
        return Some(EditorVimExCommand::Substitute(substitute));
    }
    if let Ok(line) = command.parse::<usize>() {
        return Some(EditorVimExCommand::GoToLine(line));
    }
    let parsed = match command.trim() {
        "registers" => EditorVimExCommand::Registers,
        "display" => EditorVimExCommand::Display,
        "w" => EditorVimExCommand::Write,
        "w!" => EditorVimExCommand::WriteForce,
        "wq" => EditorVimExCommand::WriteQuit,
        "wq!" => EditorVimExCommand::WriteQuitForce,
        "x" => EditorVimExCommand::Exit,
        "q" => EditorVimExCommand::Quit,
        "q!" => EditorVimExCommand::QuitForce,
        "wqa" | "wqall" => EditorVimExCommand::WriteAll,
        _ => return None,
    };
    Some(parsed)
}

fn vim_apply_substitute_command(
    buffer: &mut TextBuffer,
    substitute: &EditorVimSubstituteCommand,
) -> bool {
    let Some((first_line, last_line)) = substitute_line_span(buffer, substitute.range) else {
        return false;
    };
    vim_apply_substitute_within_lines(
        buffer,
        &substitute.query,
        &substitute.replacement,
        substitute.global,
        first_line,
        last_line,
    )
}

pub(in crate::editor_vim_key_events) fn vim_apply_substitute_repeat(
    buffer: &mut TextBuffer,
    query: &str,
    replacement: &str,
    global: bool,
) -> bool {
    let line = buffer.cursor_position().line;
    vim_apply_substitute_within_lines(buffer, query, replacement, global, line, line)
}

fn vim_apply_substitute_within_lines(
    buffer: &mut TextBuffer,
    query: &str,
    replacement: &str,
    global: bool,
    first_line: usize,
    last_line: usize,
) -> bool {
    let matches =
        buffer.find_matches_with_options(query, VIM_COMMAND_SUBSTITUTE_MAX_MATCHES, true, false);

    let mut ranges = Vec::new();
    let mut previous_line: Option<usize> = None;
    for range in matches {
        let line = buffer.char_position(range.start).line;
        if line < first_line || line > last_line {
            continue;
        }

        if !global && previous_line == Some(line) {
            continue;
        }
        previous_line = Some(line);
        ranges.push(range);
    }
    if ranges.is_empty() {
        return false;
    }

    for range in ranges.iter().rev() {
        let deleted_start = buffer.char_position(range.start);
        let deleted_end = buffer.char_position(range.end);
        vim_adjust_marks_for_edit(
            buffer.id(),
            (deleted_start.line, deleted_start.column),
            (deleted_end.line, deleted_end.column),
            replacement,
        );
    }

    buffer.replace_match_ranges(ranges, replacement) > 0
}

fn substitute_line_span(
    buffer: &TextBuffer,
    range: EditorVimSubstituteRange,
) -> Option<(usize, usize)> {
    let last_line = buffer.len_lines().saturating_sub(1);
    let span = match range {
        EditorVimSubstituteRange::WholeFile => (0, last_line),
        EditorVimSubstituteRange::CurrentLine => {
            let line = buffer.cursor_position().line;
            (line, line)
        }
        EditorVimSubstituteRange::Lines(first, last) => (
            first.saturating_sub(1).min(last_line),
            last.saturating_sub(1).min(last_line),
        ),
    };
    (span.0 <= span.1).then_some(span)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditorVimSubstituteRange {
    CurrentLine,
    WholeFile,

    Lines(usize, usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EditorVimSubstituteCommand {
    range: EditorVimSubstituteRange,
    query: String,
    replacement: String,
    global: bool,
}

fn vim_parse_substitute_command(command: &str) -> Option<EditorVimSubstituteCommand> {
    let (range, rest) = if let Some(rest) = command.strip_prefix('%') {
        (EditorVimSubstituteRange::WholeFile, rest)
    } else {
        match vim_parse_leading_line_range(command) {
            Some((lines, rest)) => (lines, rest),
            None => (EditorVimSubstituteRange::CurrentLine, command),
        }
    };

    let rest = rest.strip_prefix('s')?;
    let mut chars = rest.chars().peekable();
    let delimiter = chars.next()?;
    if delimiter.is_ascii_alphanumeric() || delimiter.is_whitespace() || delimiter == '\\' {
        return None;
    }

    let query = vim_take_substitute_part(&mut chars, delimiter, false)?;
    let replacement = vim_take_substitute_part(&mut chars, delimiter, true)?;
    let flags = chars.collect::<String>();
    if query.is_empty() || flags.chars().any(|flag| flag != 'g') {
        return None;
    }

    Some(EditorVimSubstituteCommand {
        range,
        query,
        replacement,
        global: flags.contains('g'),
    })
}

fn vim_parse_leading_line_range(command: &str) -> Option<(EditorVimSubstituteRange, &str)> {
    let (first, rest) = vim_take_leading_line_number(command)?;
    if let Some(rest) = rest.strip_prefix(',') {
        let (second, rest) = vim_take_leading_line_number(rest)?;
        return Some((EditorVimSubstituteRange::Lines(first, second), rest));
    }
    Some((EditorVimSubstituteRange::Lines(first, first), rest))
}

fn vim_take_leading_line_number(command: &str) -> Option<(usize, &str)> {
    let digits = command.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let number = command[..digits].parse::<usize>().ok()?;
    Some((number, &command[digits..]))
}

fn vim_take_substitute_part(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    delimiter: char,
    allow_end_of_input: bool,
) -> Option<String> {
    let mut part = String::new();
    let mut escaped = false;
    for ch in chars.by_ref() {
        if escaped {
            if ch != delimiter && ch != '\\' {
                part.push('\\');
            }
            part.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
        } else if ch == delimiter {
            return Some(part);
        } else {
            part.push(ch);
        }
    }

    (allow_end_of_input && !escaped).then_some(part)
}
