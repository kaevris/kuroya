use eframe::egui::{Key, Modifiers};
use kuroya_core::{TextBuffer, TextEdit};
use std::ops::Range;

use super::super::{EditorVimCaseConversion, VIM_MAX_COUNT, vim_line_range_for_count};

pub(in crate::editor_vim_key_events) fn vim_toggle_case_forward_chars(
    buffer: &mut TextBuffer,
    count: usize,
) -> bool {
    let start = buffer.cursor();
    let line_end = buffer.line_content_end_char(buffer.cursor_position().line);
    if start >= line_end {
        return false;
    }

    let end = start
        .saturating_add(count.clamp(1, VIM_MAX_COUNT))
        .min(line_end);
    vim_toggle_case_range(buffer, start..end, end)
}

pub(in crate::editor_vim_key_events) fn vim_convert_case_forward_chars(
    buffer: &mut TextBuffer,
    count: usize,
    conversion: EditorVimCaseConversion,
) -> bool {
    let start = buffer.cursor();
    let end = start
        .saturating_add(count.clamp(1, VIM_MAX_COUNT))
        .min(buffer.len_chars());
    if start >= end {
        return false;
    }

    vim_convert_case_range(buffer, start..end, start, conversion)
}

pub(in crate::editor_vim_key_events) fn vim_convert_case_lines(
    buffer: &mut TextBuffer,
    count: usize,
    conversion: EditorVimCaseConversion,
) -> bool {
    let Some(range) = vim_line_range_for_count(buffer, count) else {
        return false;
    };
    let cursor = buffer.cursor();
    vim_convert_case_range(buffer, range, cursor, conversion)
}

pub(in crate::editor_vim_key_events) fn vim_case_conversion_repeated_operator_key(
    conversion: EditorVimCaseConversion,
    key: Key,
    modifiers: Modifiers,
) -> bool {
    matches!(
        (conversion, key, modifiers.shift),
        (EditorVimCaseConversion::Lower, Key::U, false)
            | (EditorVimCaseConversion::Upper, Key::U, true)
    ) && !modifiers.command
        && !modifiers.alt
        && !modifiers.ctrl
}

pub(in crate::editor_vim_key_events) fn vim_toggle_case_range(
    buffer: &mut TextBuffer,
    range: Range<usize>,
    cursor: usize,
) -> bool {
    vim_convert_case_range(buffer, range, cursor, EditorVimCaseConversion::Toggle)
}

pub(in crate::editor_vim_key_events) fn vim_convert_case_range(
    buffer: &mut TextBuffer,
    range: Range<usize>,
    cursor: usize,
    conversion: EditorVimCaseConversion,
) -> bool {
    let mut edits = Vec::new();
    for idx in range.clone() {
        let Some(ch) = buffer.char_at(idx) else {
            continue;
        };
        let Some(converted) = convert_case_char(ch, conversion) else {
            continue;
        };
        edits.push(TextEdit {
            range: idx..idx + 1,
            inserted: converted,
        });
    }

    let changed = !edits.is_empty() && buffer.apply_edits(edits);
    buffer.set_single_cursor(cursor.min(buffer.len_chars()));
    changed
}

fn convert_case_char(ch: char, conversion: EditorVimCaseConversion) -> Option<String> {
    match conversion {
        EditorVimCaseConversion::Lower => {
            let lowered = ch.to_lowercase().collect::<String>();
            (lowered != ch.to_string()).then_some(lowered)
        }
        EditorVimCaseConversion::Upper => {
            let uppered = ch.to_uppercase().collect::<String>();
            (uppered != ch.to_string()).then_some(uppered)
        }
        EditorVimCaseConversion::Toggle if ch.is_lowercase() => {
            Some(ch.to_uppercase().collect::<String>())
        }
        EditorVimCaseConversion::Toggle if ch.is_uppercase() => {
            Some(ch.to_lowercase().collect::<String>())
        }
        EditorVimCaseConversion::Toggle => None,
    }
}
