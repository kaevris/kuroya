use eframe::egui::{Key, Modifiers};
use kuroya_core::{BufferId, TextBuffer};
use std::cell::RefCell;

use super::{
    EditorVimNamedRegister, EditorVimRegister, EditorVimRegisterKind,
    vim_line_first_non_whitespace_char, vim_printable_key_char,
};

const VIM_ALPHA_MARK_SLOT_COUNT: usize = 52;

const VIM_PREVIOUS_CONTEXT_MARK_SLOT: usize = VIM_ALPHA_MARK_SLOT_COUNT;

const VIM_VISUAL_START_MARK_SLOT: usize = VIM_PREVIOUS_CONTEXT_MARK_SLOT + 1;

const VIM_VISUAL_END_MARK_SLOT: usize = VIM_VISUAL_START_MARK_SLOT + 1;
const VIM_MARK_SLOT_COUNT: usize = VIM_VISUAL_END_MARK_SLOT + 1;
const VIM_NAMED_REGISTER_COUNT: usize = 26;
const VIM_BLACK_HOLE_REGISTER_INDEX: usize = VIM_NAMED_REGISTER_COUNT;
const VIM_YANK_REGISTER_INDEX: usize = VIM_BLACK_HOLE_REGISTER_INDEX + 1;
const VIM_NUMBERED_REGISTER_BASE: usize = VIM_YANK_REGISTER_INDEX + 1;
const VIM_NUMBERED_REGISTER_COUNT: usize = 9;
const VIM_SMALL_DELETE_REGISTER_INDEX: usize =
    VIM_NUMBERED_REGISTER_BASE + VIM_NUMBERED_REGISTER_COUNT;

const VIM_CLIPBOARD_REGISTER_INDEX: usize = VIM_SMALL_DELETE_REGISTER_INDEX + 1;
const VIM_REGISTER_STORE_SIZE: usize = VIM_CLIPBOARD_REGISTER_INDEX + 1;
const VIM_MARK_BUFFER_LIMIT: usize = 128;
const VIM_REGISTER_SUMMARY_ENTRY_MAX_CHARS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EditorVimRegisterWriteScope {
    Yank,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EditorVimMark {
    line: usize,
    column: usize,
}

#[derive(Debug, Clone)]
struct EditorVimBufferMarks {
    buffer_key: BufferId,
    marks: [Option<EditorVimMark>; VIM_MARK_SLOT_COUNT],
}

impl EditorVimBufferMarks {
    fn new(buffer_key: BufferId) -> Self {
        Self {
            buffer_key,
            marks: [None; VIM_MARK_SLOT_COUNT],
        }
    }
}

thread_local! {
    static VIM_MARKS: RefCell<Vec<EditorVimBufferMarks>> = const { RefCell::new(Vec::new()) };
    static VIM_NAMED_REGISTERS: RefCell<[Option<EditorVimRegister>; VIM_REGISTER_STORE_SIZE]> =
        RefCell::new(std::array::from_fn(|_| None));
}

pub(super) fn vim_mark_name_for_key(key: Key, modifiers: Modifiers) -> Option<char> {
    vim_printable_key_char(key, modifiers).filter(|ch| ch.is_ascii_alphabetic())
}

pub(super) fn vim_jump_mark_name_for_key(key: Key, modifiers: Modifiers) -> Option<char> {
    let ch = vim_printable_key_char(key, modifiers)?;
    match ch {
        '\'' | '`' | '<' | '>' => Some(ch),
        _ => ch.is_ascii_alphabetic().then_some(ch),
    }
}

fn vim_mark_slot(mark: char) -> Option<usize> {
    if mark.is_ascii_lowercase() {
        Some((mark as u8 - b'a') as usize)
    } else if mark.is_ascii_uppercase() {
        Some(26 + (mark as u8 - b'A') as usize)
    } else {
        match mark {
            '\'' | '`' => Some(VIM_PREVIOUS_CONTEXT_MARK_SLOT),
            '<' => Some(VIM_VISUAL_START_MARK_SLOT),
            '>' => Some(VIM_VISUAL_END_MARK_SLOT),
            _ => None,
        }
    }
}

pub(super) fn vim_named_register_for_key(
    key: Key,
    modifiers: Modifiers,
) -> Option<EditorVimNamedRegister> {
    let ch = vim_printable_key_char(key, modifiers)?;
    vim_named_register_for_char(ch)
}

fn vim_named_register_for_char(ch: char) -> Option<EditorVimNamedRegister> {
    if ch == '_' {
        Some(EditorVimNamedRegister {
            index: VIM_BLACK_HOLE_REGISTER_INDEX,
            append: false,
        })
    } else if ch == '-' {
        Some(EditorVimNamedRegister {
            index: VIM_SMALL_DELETE_REGISTER_INDEX,
            append: false,
        })
    } else if ch == '+' || ch == '*' {
        Some(EditorVimNamedRegister {
            index: VIM_CLIPBOARD_REGISTER_INDEX,
            append: false,
        })
    } else if ch.is_ascii_digit() {
        Some(EditorVimNamedRegister {
            index: VIM_YANK_REGISTER_INDEX + (ch as u8 - b'0') as usize,
            append: false,
        })
    } else {
        ch.is_ascii_alphabetic().then(|| EditorVimNamedRegister {
            index: (ch.to_ascii_lowercase() as u8 - b'a') as usize,
            append: ch.is_ascii_uppercase(),
        })
    }
}

pub(super) fn vim_named_register_label(register: EditorVimNamedRegister) -> char {
    if register.index == VIM_BLACK_HOLE_REGISTER_INDEX {
        return '_';
    }
    if register.index == VIM_SMALL_DELETE_REGISTER_INDEX {
        return '-';
    }
    if register.index == VIM_CLIPBOARD_REGISTER_INDEX {
        return '+';
    }
    if register.index >= VIM_YANK_REGISTER_INDEX
        && register.index < VIM_NUMBERED_REGISTER_BASE + VIM_NUMBERED_REGISTER_COUNT
    {
        let digit = register.index - VIM_YANK_REGISTER_INDEX;
        return char::from(b'0' + digit as u8);
    }
    let label = u8::try_from(register.index)
        .ok()
        .and_then(|index| {
            (usize::from(index) < VIM_NAMED_REGISTER_COUNT).then_some((b'a' + index) as char)
        })
        .unwrap_or('?');
    if register.append {
        label.to_ascii_uppercase()
    } else {
        label
    }
}

fn vim_named_register_is_black_hole(register: EditorVimNamedRegister) -> bool {
    register.index == VIM_BLACK_HOLE_REGISTER_INDEX
}

fn vim_named_register_is_numbered(register: EditorVimNamedRegister) -> bool {
    register.index >= VIM_YANK_REGISTER_INDEX
        && register.index < VIM_NUMBERED_REGISTER_BASE + VIM_NUMBERED_REGISTER_COUNT
}

pub(super) fn vim_named_register(register: EditorVimNamedRegister) -> Option<EditorVimRegister> {
    if vim_named_register_is_black_hole(register) || register.index >= VIM_REGISTER_STORE_SIZE {
        return None;
    }
    VIM_NAMED_REGISTERS.with(|registers| registers.borrow()[register.index].clone())
}

fn vim_store_named_register(register: EditorVimNamedRegister, value: &EditorVimRegister) {
    if vim_named_register_is_black_hole(register) || register.index >= VIM_REGISTER_STORE_SIZE {
        return;
    }
    VIM_NAMED_REGISTERS.with(|registers| {
        let mut registers = registers.borrow_mut();
        if register.append
            && let Some(existing) = &mut registers[register.index]
        {
            if existing.kind == EditorVimRegisterKind::Linewise {
                if !existing.text.ends_with('\n') {
                    existing.text.push('\n');
                }
                existing.text.push_str(&value.text);
                if value.kind != EditorVimRegisterKind::Linewise {
                    existing.text.push('\n');
                }
                existing.kind = EditorVimRegisterKind::Linewise;
            } else if value.kind == EditorVimRegisterKind::Linewise {
                existing.text.push('\n');
                existing.text.push_str(&value.text);
                if !existing.text.ends_with('\n') {
                    existing.text.push('\n');
                }
                existing.kind = EditorVimRegisterKind::Linewise;
            } else {
                existing.text.push_str(&value.text);
            }
        } else {
            registers[register.index] = Some(value.clone());
        }
    });
}

pub(super) fn vim_write_registers(
    unnamed_register: &mut Option<EditorVimRegister>,
    named_register: Option<EditorVimNamedRegister>,
    value: EditorVimRegister,
) {
    vim_write_registers_with_scope(
        unnamed_register,
        named_register,
        value,
        EditorVimRegisterWriteScope::Yank,
    );
}

pub(super) fn vim_write_registers_with_scope(
    unnamed_register: &mut Option<EditorVimRegister>,
    named_register: Option<EditorVimNamedRegister>,
    value: EditorVimRegister,
    scope: EditorVimRegisterWriteScope,
) {
    if named_register.is_some_and(vim_named_register_is_black_hole) {
        return;
    }
    match named_register {
        Some(register) => {
            vim_store_named_register(register, &value);

            if scope == EditorVimRegisterWriteScope::Delete
                && !vim_named_register_is_numbered(register)
                && (value.kind == EditorVimRegisterKind::Linewise || value.text.contains('\n'))
            {
                vim_shift_numbered_registers(&value);
            }
        }
        None => match scope {
            EditorVimRegisterWriteScope::Yank => {
                vim_store_register_at_index(VIM_YANK_REGISTER_INDEX, &value);
            }
            EditorVimRegisterWriteScope::Delete => {
                if value.kind == EditorVimRegisterKind::Linewise || value.text.contains('\n') {
                    vim_shift_numbered_registers(&value);
                } else {
                    vim_store_register_at_index(VIM_SMALL_DELETE_REGISTER_INDEX, &value);
                }
            }
        },
    }
    *unnamed_register = Some(value);
}

fn vim_store_register_at_index(index: usize, value: &EditorVimRegister) {
    VIM_NAMED_REGISTERS.with(|registers| {
        registers.borrow_mut()[index] = Some(value.clone());
    });
}

fn vim_shift_numbered_registers(value: &EditorVimRegister) {
    VIM_NAMED_REGISTERS.with(|registers| {
        let mut registers = registers.borrow_mut();
        for offset in (1..VIM_NUMBERED_REGISTER_COUNT).rev() {
            let shifted = registers[VIM_NUMBERED_REGISTER_BASE + offset - 1].clone();
            registers[VIM_NUMBERED_REGISTER_BASE + offset] = shifted;
        }
        registers[VIM_NUMBERED_REGISTER_BASE] = Some(value.clone());
    });
}

pub(crate) fn vim_clear_named_registers() {
    VIM_NAMED_REGISTERS.with(|registers| {
        registers.borrow_mut().fill(None);
    });
}

pub(super) fn vim_registers_summary() -> Option<String> {
    VIM_NAMED_REGISTERS.with(|registers| {
        let mut summary = String::new();
        for (index, register) in registers.borrow().iter().enumerate() {
            let Some(register) = register else {
                continue;
            };
            if register.text.is_empty() {
                continue;
            }
            if !summary.is_empty() {
                summary.push_str(" / ");
            }
            summary.push('"');
            summary.push(vim_named_register_label(EditorVimNamedRegister {
                index,
                append: false,
            }));
            summary.push(' ');
            let mut content = register.text.chars();
            for _ in 0..VIM_REGISTER_SUMMARY_ENTRY_MAX_CHARS {
                match content.next() {
                    Some('\n') => summary.push_str("\\n"),
                    Some(ch) => summary.push(ch),
                    None => break,
                }
            }
            if content.next().is_some() {
                summary.push_str("...");
            }
        }
        (!summary.is_empty()).then_some(summary)
    })
}

pub(crate) fn vim_clear_marks() {
    VIM_MARKS.with(|marks| {
        marks.borrow_mut().clear();
    });
}

pub(crate) fn vim_forget_marks_for_buffer(buffer_id: BufferId) {
    VIM_MARKS.with(|marks| {
        marks
            .borrow_mut()
            .retain(|entry| entry.buffer_key != buffer_id);
    });
}

#[cfg(test)]
pub(crate) fn vim_marks_are_empty_for_test() -> bool {
    VIM_MARKS.with(|marks| marks.borrow().is_empty())
}

fn vim_buffer_mark_key(buffer: &TextBuffer) -> BufferId {
    buffer.id()
}

fn vim_edit_marks_for_buffer<F>(buffer: &TextBuffer, edit: F)
where
    F: FnOnce(&mut EditorVimBufferMarks),
{
    let buffer_key = vim_buffer_mark_key(buffer);
    VIM_MARKS.with(|marks| {
        let mut marks = marks.borrow_mut();
        if let Some(existing) = marks
            .iter_mut()
            .find(|entry| entry.buffer_key == buffer_key)
        {
            edit(existing);
            return;
        }

        if marks.len() >= VIM_MARK_BUFFER_LIMIT {
            marks.remove(0);
        }
        let mut entry = EditorVimBufferMarks::new(buffer_key);
        edit(&mut entry);
        marks.push(entry);
    });
}

fn vim_store_mark(buffer: &TextBuffer, mark: char, position: (usize, usize)) {
    let Some(slot) = vim_mark_slot(mark) else {
        return;
    };
    vim_edit_marks_for_buffer(buffer, |entry| {
        entry.marks[slot] = Some(EditorVimMark {
            line: position.0,
            column: position.1,
        });
    });
}

pub(super) fn vim_set_mark(buffer: &TextBuffer, mark: char) -> bool {
    if vim_mark_slot(mark).is_none() {
        return false;
    };
    let position = buffer.cursor_position();
    vim_store_mark(buffer, mark, (position.line, position.column));
    true
}

pub(super) fn vim_set_previous_context_mark(buffer: &TextBuffer) {
    let position = buffer.cursor_position();
    vim_store_mark(buffer, '\'', (position.line, position.column));
}

pub(super) fn vim_set_visual_bounds_marks(buffer: &TextBuffer, start: usize, end: usize) {
    let start_position = buffer.char_position(start);
    let end_position = buffer.char_position(end);
    vim_store_mark(buffer, '<', (start_position.line, start_position.column));
    vim_store_mark(buffer, '>', (end_position.line, end_position.column));
}

fn vim_mark_for_buffer(buffer: &TextBuffer, mark: char) -> Option<EditorVimMark> {
    let slot = vim_mark_slot(mark)?;
    let buffer_key = vim_buffer_mark_key(buffer);
    VIM_MARKS.with(|marks| {
        marks
            .borrow()
            .iter()
            .find(|entry| entry.buffer_key == buffer_key)
            .and_then(|entry| entry.marks[slot])
    })
}

pub(super) fn vim_jump_to_mark(buffer: &mut TextBuffer, mark: char, linewise: bool) -> bool {
    let Some(mark) = vim_mark_for_buffer(buffer, mark) else {
        return false;
    };

    vim_set_previous_context_mark(buffer);
    let line = mark.line.min(buffer.len_lines().saturating_sub(1));
    let cursor = if linewise {
        vim_line_first_non_whitespace_char(buffer, line)
    } else {
        buffer.line_column_to_char(line, mark.column)
    };
    buffer.set_single_cursor(cursor);
    true
}

pub(super) fn vim_visual_bounds_marks(
    buffer: &TextBuffer,
) -> Option<((usize, usize), (usize, usize))> {
    let start = vim_mark_for_buffer(buffer, '<')?;
    let end = vim_mark_for_buffer(buffer, '>')?;
    Some(((start.line, start.column), (end.line, end.column)))
}

pub(super) fn vim_store_visual_bounds_mark_positions(
    buffer: &TextBuffer,
    start: (usize, usize),
    end: (usize, usize),
) {
    vim_store_mark(buffer, '<', start);
    vim_store_mark(buffer, '>', end);
}

pub(super) fn vim_adjust_marks_for_edit(
    buffer_id: BufferId,
    deleted_start: (usize, usize),
    deleted_end: (usize, usize),
    inserted_text: &str,
) {
    let inserted_lines = inserted_text.bytes().filter(|byte| *byte == b'\n').count();
    let inserted_last_line_columns = inserted_text
        .rsplit('\n')
        .next()
        .map(str::chars)
        .unwrap_or_else(|| "".chars())
        .count();
    let pure_insertion = deleted_start == deleted_end;
    VIM_MARKS.with(|marks| {
        let mut marks = marks.borrow_mut();
        let Some(entry) = marks.iter_mut().find(|entry| entry.buffer_key == buffer_id) else {
            return;
        };
        for slot in entry.marks.iter_mut() {
            let Some(mark) = slot.as_mut() else {
                continue;
            };
            let position = (mark.line, mark.column);
            if position < deleted_start {
                continue;
            }
            let same_line_range = deleted_start.0 == deleted_end.0;
            if position >= deleted_end {
                if same_line_range {
                    if mark.line == deleted_start.0 {
                        mark.column -= deleted_end.1 - deleted_start.1;
                    }
                } else if mark.line == deleted_end.0 {
                    mark.line = deleted_start.0;
                    mark.column = deleted_start.1 + (mark.column - deleted_end.1);
                } else {
                    mark.line -= deleted_end.0 - deleted_start.0;
                }
            } else if same_line_range {
                mark.column = deleted_start.1;
            } else {
                *slot = None;
                continue;
            }

            let rides_insertion = if inserted_lines == 0 {
                mark.column > deleted_start.1 || (mark.column == deleted_start.1 && !pure_insertion)
            } else {
                mark.column >= deleted_start.1
            };
            if inserted_lines == 0 {
                if mark.line == deleted_start.0 && rides_insertion {
                    mark.column += inserted_last_line_columns;
                }
            } else if mark.line > deleted_start.0 {
                mark.line += inserted_lines;
            } else if mark.line == deleted_start.0 && rides_insertion {
                mark.line += inserted_lines;
                mark.column = inserted_last_line_columns + (mark.column - deleted_start.1);
            }
        }
    });
}
