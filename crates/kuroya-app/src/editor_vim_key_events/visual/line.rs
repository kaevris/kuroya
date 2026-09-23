use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

use super::super::commands::{
    vim_change_line_span_into_registers, vim_delete_line_span_into_registers,
    vim_yank_line_span_into_registers,
};
use super::super::{
    EditorVimLastChange, EditorVimMode, EditorVimPendingKey, EditorVimRegister,
    EditorVimRepeatAction, VIM_MAX_COUNT, VimKeyResult, vim_convert_case_range, vim_count_digit,
    vim_escape_key, vim_push_count_digit,
};
use super::character_action::vim_put_over_visual_lines;
use super::motion::vim_visual_character_motion_key;
use super::selection::{
    vim_exit_visual_selection, vim_visual_character_clamped_cursor, vim_visual_character_line_span,
};
use super::{
    vim_indent_visual_character_lines, vim_join_visual_character_lines,
    vim_outdent_visual_character_lines, vim_visual_character_case_conversion,
    vim_visual_character_change_key, vim_visual_character_delete_key,
    vim_visual_character_indent_key, vim_visual_character_join_key,
    vim_visual_character_motion_target, vim_visual_character_outdent_key,
    vim_visual_character_replace_key, vim_visual_character_yank_key,
};

pub(in crate::editor_vim_key_events) fn vim_set_visual_line_selection(
    buffer: &mut TextBuffer,
    anchor: usize,
    cursor: usize,
) {
    match vim_visual_character_line_span(buffer, anchor, cursor) {
        Some((span, _, _)) => buffer.set_selection(span.start, span.end),
        None => buffer.set_single_cursor(cursor.min(buffer.len_chars())),
    }
}

fn vim_visual_line_exit_key(key: Key, modifiers: Modifiers) -> bool {
    key == Key::V && modifiers.shift && !modifiers.command && !modifiers.alt && !modifiers.ctrl
}

pub(in crate::editor_vim_key_events) fn handle_vim_visual_line_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    anchor: usize,
    cursor: usize,
    count: Option<usize>,
    indent_unit: &str,
    suppress_text: Option<char>,
) -> VimKeyResult {
    if vim_escape_key(key, modifiers) || vim_visual_line_exit_key(key, modifiers) {
        *pending = None;
        vim_exit_visual_selection(buffer, anchor, cursor);
        return VimKeyResult::handled(suppress_text);
    }
    if modifiers.command || modifiers.alt || modifiers.ctrl {
        vim_restore_visual_line_pending(pending, anchor, cursor, count);
        return VimKeyResult::ignored();
    }

    if key == Key::V && !modifiers.shift {
        let anchor = vim_visual_character_clamped_cursor(buffer, anchor);
        let cursor = vim_visual_character_clamped_cursor(buffer, cursor);
        if cursor > anchor {
            buffer.set_selection(anchor, cursor);
        } else if cursor < anchor {
            buffer.set_selection(cursor, anchor);
        } else {
            super::vim_set_visual_character_selection(buffer, anchor, cursor);
        }
        *pending = Some(EditorVimPendingKey::VisualCharacter { anchor, cursor });
        return VimKeyResult::handled(suppress_text);
    }
    if count.is_none()
        && let Some(digit) = vim_count_digit(key, modifiers, false)
    {
        *pending = Some(EditorVimPendingKey::VisualLine {
            anchor,
            cursor,
            count: Some(digit),
        });
        return VimKeyResult::handled(suppress_text);
    }
    if let Some(count) = count
        && let Some(digit) = vim_count_digit(key, modifiers, true)
    {
        *pending = Some(EditorVimPendingKey::VisualLine {
            anchor,
            cursor,
            count: Some(vim_push_count_digit(count, digit)),
        });
        return VimKeyResult::handled(suppress_text);
    }

    if key == Key::O {
        vim_set_visual_line_selection(buffer, cursor, anchor);
        *pending = Some(EditorVimPendingKey::VisualLine {
            anchor: cursor,
            cursor: anchor,
            count: None,
        });
        return VimKeyResult::handled(suppress_text);
    }
    if vim_visual_character_yank_key(key, modifiers) {
        if let Some((span, _, _)) = vim_visual_character_line_span(buffer, anchor, cursor) {
            vim_yank_line_span_into_registers(buffer, span.clone(), unnamed_register, None);
            buffer.set_single_cursor(span.start);
        } else {
            buffer.set_single_cursor(vim_visual_character_clamped_cursor(buffer, cursor));
        }

        super::vim_record_visual_bounds_marks(buffer, anchor, cursor);
        *pending = None;
        return VimKeyResult::handled(suppress_text);
    }
    if key == Key::P && !modifiers.command && !modifiers.alt && !modifiers.ctrl {
        let (put_anchor, put_cursor) = vim_visual_line_put_bounds(buffer, anchor, cursor);
        let source = unnamed_register.clone();
        let changed = vim_put_over_visual_lines(
            buffer,
            put_anchor,
            put_cursor,
            source.as_ref(),
            unnamed_register,
            count,
        );
        *pending = None;
        return if changed {
            VimKeyResult::changed(suppress_text)
        } else {
            VimKeyResult::handled(suppress_text)
        };
    }
    if vim_visual_character_delete_key(key, modifiers) {
        let repeat_count = vim_visual_line_repeat_count(buffer, anchor, cursor);
        let changed =
            if let Some((span, _, _)) = vim_visual_character_line_span(buffer, anchor, cursor) {
                vim_delete_line_span_into_registers(buffer, span, unnamed_register, None)
            } else {
                buffer.set_single_cursor(vim_visual_character_clamped_cursor(buffer, cursor));
                false
            };
        *pending = None;
        return super::super::vim_repeatable_change_result(
            changed,
            last_change,
            EditorVimRepeatAction::DeleteLines,
            repeat_count,
            suppress_text,
        );
    }
    if vim_visual_character_indent_key(key, modifiers) {
        let repeat_count = vim_visual_line_repeat_count(buffer, anchor, cursor);
        let changed = vim_indent_visual_character_lines(
            buffer,
            anchor,
            cursor,
            count.unwrap_or(1),
            indent_unit,
        );
        *pending = None;
        return super::super::vim_repeatable_change_result(
            changed,
            last_change,
            EditorVimRepeatAction::IndentLines,
            repeat_count,
            suppress_text,
        );
    }
    if vim_visual_character_outdent_key(key, modifiers) {
        let repeat_count = vim_visual_line_repeat_count(buffer, anchor, cursor);
        let changed = vim_outdent_visual_character_lines(
            buffer,
            anchor,
            cursor,
            count.unwrap_or(1),
            indent_unit,
        );
        *pending = None;
        return super::super::vim_repeatable_change_result(
            changed,
            last_change,
            EditorVimRepeatAction::OutdentLines,
            repeat_count,
            suppress_text,
        );
    }
    if vim_visual_character_join_key(key, modifiers) {
        let amount = count
            .unwrap_or(0)
            .max(vim_visual_line_repeat_count(buffer, anchor, cursor))
            .max(1);
        let changed = vim_join_visual_character_lines(buffer, anchor, cursor, amount);
        *pending = None;
        return super::super::vim_repeatable_change_result(
            changed,
            last_change,
            EditorVimRepeatAction::JoinLines,
            amount.saturating_sub(1).max(1),
            suppress_text,
        );
    }

    if let Some(conversion) = vim_visual_character_case_conversion(key, modifiers) {
        let Some((span, _, _)) = vim_visual_character_line_span(buffer, anchor, cursor) else {
            vim_restore_visual_line_pending(pending, anchor, cursor, count);
            return VimKeyResult::handled(suppress_text);
        };
        let repeat_count = span.end.saturating_sub(span.start).clamp(1, VIM_MAX_COUNT);
        let changed = vim_convert_case_range(buffer, span.clone(), span.start, conversion);
        *pending = None;
        return super::super::vim_repeatable_change_result(
            changed,
            last_change,
            EditorVimRepeatAction::ConvertCaseForwardChars(conversion),
            repeat_count,
            suppress_text,
        );
    }

    if vim_visual_character_replace_key(key, modifiers) {
        *pending = Some(EditorVimPendingKey::VisualLineReplace { anchor, cursor });
        return VimKeyResult::handled(suppress_text);
    }

    if vim_visual_character_change_key(key, modifiers) {
        let repeat_count = vim_visual_line_repeat_count(buffer, anchor, cursor);
        let changed = if let Some((span, _, _)) =
            vim_visual_character_line_span(buffer, anchor, cursor)
        {
            let insert_at = span.start;
            let changed = vim_change_line_span_into_registers(buffer, span, unnamed_register, None);
            if changed {
                buffer.set_single_cursor(insert_at.min(buffer.len_chars()));
            }
            changed
        } else {
            buffer.set_single_cursor(vim_visual_character_clamped_cursor(buffer, cursor));
            false
        };
        *pending = None;
        if changed {
            *mode = EditorVimMode::Insert;
        }
        return super::super::vim_repeatable_change_result(
            changed,
            last_change,
            EditorVimRepeatAction::ChangeLines,
            repeat_count,
            suppress_text,
        );
    }
    if key == Key::G && !modifiers.shift {
        *pending = Some(EditorVimPendingKey::VisualLineGo {
            anchor,
            cursor,
            count,
        });
        return VimKeyResult::handled(suppress_text);
    }
    if let Some(target) =
        vim_visual_character_motion_target(buffer, cursor, count.unwrap_or(1), key, modifiers)
    {
        vim_set_visual_line_selection(buffer, anchor, target);

        *pending = Some(EditorVimPendingKey::VisualLine {
            anchor,
            cursor: target,
            count: None,
        });
        return VimKeyResult::handled(suppress_text);
    }

    vim_restore_visual_line_pending(pending, anchor, cursor, count);
    if suppress_text.is_some() {
        VimKeyResult::handled(suppress_text)
    } else {
        VimKeyResult::ignored()
    }
}

pub(in crate::editor_vim_key_events) fn handle_vim_visual_line_go_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    pending: &mut Option<EditorVimPendingKey>,
    anchor: usize,
    cursor: usize,
    count: Option<usize>,
    suppress_text: Option<char>,
) -> VimKeyResult {
    let resolve =
        |buffer: &mut TextBuffer, pending: &mut Option<EditorVimPendingKey>, target| match target {
            Some(target) => {
                vim_set_visual_line_selection(buffer, anchor, target);

                *pending = Some(EditorVimPendingKey::VisualLine {
                    anchor,
                    cursor: target,
                    count: None,
                });
            }
            None => vim_restore_visual_line_pending(pending, anchor, cursor, count),
        };

    if vim_escape_key(key, modifiers) {
        *pending = None;
        buffer.set_single_cursor(vim_visual_character_clamped_cursor(buffer, cursor));
        return VimKeyResult::handled(suppress_text);
    }
    if modifiers.command || modifiers.alt || modifiers.ctrl {
        vim_restore_visual_line_pending(pending, anchor, cursor, count);
        return VimKeyResult::ignored();
    }
    if key == Key::G && !modifiers.shift {
        let target = buffer.line_column_to_char(0, 0);
        resolve(buffer, pending, Some(target));
        return VimKeyResult::handled(suppress_text);
    }
    if key == Key::G && modifiers.shift {
        let last_line = buffer.len_lines().saturating_sub(1);
        let target = buffer.line_column_to_char(last_line, 0);
        resolve(buffer, pending, Some(target));
        return VimKeyResult::handled(suppress_text);
    }

    resolve(buffer, pending, None);
    if suppress_text.is_some() {
        VimKeyResult::handled(suppress_text)
    } else {
        VimKeyResult::ignored()
    }
}

pub(in crate::editor_vim_key_events) fn vim_restore_visual_line_pending(
    pending: &mut Option<EditorVimPendingKey>,
    anchor: usize,
    cursor: usize,
    count: Option<usize>,
) {
    *pending = Some(EditorVimPendingKey::VisualLine {
        anchor,
        cursor,
        count,
    });
}

fn vim_visual_line_repeat_count(buffer: &TextBuffer, anchor: usize, cursor: usize) -> usize {
    vim_visual_character_line_span(buffer, anchor, cursor)
        .map(|(_, _, line_count)| line_count)
        .unwrap_or(1)
}

fn vim_visual_line_put_bounds(buffer: &TextBuffer, anchor: usize, cursor: usize) -> (usize, usize) {
    let far = anchor.max(cursor);
    if far > anchor.min(cursor)
        && far > 0
        && far <= buffer.len_chars()
        && buffer.char_position(far).column == 0
    {
        let adjusted = far - 1;
        return if cursor >= anchor {
            (anchor, adjusted)
        } else {
            (adjusted, cursor)
        };
    }
    (anchor, cursor)
}

pub(in crate::editor_vim_key_events) fn vim_visual_line_pending_after_key(
    pending: Option<EditorVimPendingKey>,
    key: Key,
    modifiers: Modifiers,
    printable_key_char: Option<char>,
) -> Option<Option<EditorVimPendingKey>> {
    let (anchor, cursor, count) = match pending {
        Some(EditorVimPendingKey::VisualLine {
            anchor,
            cursor,
            count,
        }) => (anchor, cursor, count),
        Some(EditorVimPendingKey::VisualLineGo {
            anchor,
            cursor,
            count,
        }) => {
            if vim_escape_key(key, modifiers) {
                return Some(None);
            }
            if modifiers.command || modifiers.alt || modifiers.ctrl {
                return None;
            }
            if key == Key::G {
                return Some(Some(EditorVimPendingKey::VisualLine {
                    anchor,
                    cursor,
                    count: None,
                }));
            }
            return printable_key_char
                .is_some()
                .then_some(Some(EditorVimPendingKey::VisualLine {
                    anchor,
                    cursor,
                    count,
                }));
        }
        _ => return None,
    };

    if vim_escape_key(key, modifiers) || vim_visual_line_exit_key(key, modifiers) {
        return Some(None);
    }
    if modifiers.command || modifiers.alt || modifiers.ctrl {
        return None;
    }
    if key == Key::V && !modifiers.shift {
        return Some(Some(EditorVimPendingKey::VisualCharacter {
            anchor,
            cursor,
        }));
    }
    let next_count = if count.is_none() {
        vim_count_digit(key, modifiers, false).map(Some)
    } else {
        vim_count_digit(key, modifiers, true)
            .map(|digit| Some(vim_push_count_digit(count.unwrap_or(0), digit)))
    };
    if let Some(new_count) = next_count {
        return Some(Some(EditorVimPendingKey::VisualLine {
            anchor,
            cursor,
            count: new_count,
        }));
    }
    if key == Key::O {
        return Some(Some(EditorVimPendingKey::VisualLine {
            anchor: cursor,
            cursor: anchor,
            count: None,
        }));
    }
    if vim_visual_character_yank_key(key, modifiers)
        || vim_visual_character_delete_key(key, modifiers)
        || vim_visual_character_indent_key(key, modifiers)
        || vim_visual_character_outdent_key(key, modifiers)
        || vim_visual_character_join_key(key, modifiers)
        || vim_visual_character_case_conversion(key, modifiers).is_some()
        || vim_visual_character_change_key(key, modifiers)
        || (key == Key::P && !modifiers.command && !modifiers.alt && !modifiers.ctrl)
    {
        return Some(None);
    }
    if vim_visual_character_replace_key(key, modifiers) {
        return Some(Some(EditorVimPendingKey::VisualLineReplace {
            anchor,
            cursor,
        }));
    }
    if key == Key::G && !modifiers.shift {
        return Some(Some(EditorVimPendingKey::VisualLineGo {
            anchor,
            cursor,
            count,
        }));
    }
    if vim_visual_character_motion_key(key, modifiers) {
        return Some(Some(EditorVimPendingKey::VisualLine {
            anchor,
            cursor,
            count: None,
        }));
    }
    printable_key_char.is_some().then_some(pending)
}
