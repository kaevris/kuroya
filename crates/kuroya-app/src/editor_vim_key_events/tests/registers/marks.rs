use super::*;
use eframe::egui::Key;
use kuroya_core::TextBuffer;

use super::super::super::state::{
    vim_adjust_marks_for_edit, vim_clear_marks, vim_forget_marks_for_buffer, vim_jump_to_mark,
    vim_marks_are_empty_for_test,
};

fn press_keys(buffer: &mut TextBuffer, keys: &[(Key, eframe::egui::Modifiers)]) {
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    for (key, modifiers) in keys {
        handle_vim_editor_key_event_with_state(
            buffer,
            *key,
            *modifiers,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
        );
    }
}

fn set_mark_on_line(buffer: &mut TextBuffer, line: usize, column: usize) {
    buffer.set_single_cursor(buffer.line_column_to_char(line, column));
    press_keys(
        buffer,
        &[(Key::M, Modifiers::NONE), (Key::A, Modifiers::NONE)],
    );
}

#[test]
fn marks_survive_buffer_reallocation_and_stay_per_buffer() {
    vim_clear_named_registers();
    vim_clear_marks();
    let mut buffers = vec![
        TextBuffer::from_text(11, None, "alpha\nbeta\n".to_owned()),
        TextBuffer::from_text(12, None, "one\ntwo\n".to_owned()),
    ];
    set_mark_on_line(&mut buffers[0], 1, 0);

    for index in 0..64usize {
        buffers.push(TextBuffer::from_text(
            100 + index as u64,
            None,
            String::new(),
        ));
    }

    let first = &mut buffers[0];
    first.set_single_cursor(0);
    assert!(vim_jump_to_mark(first, 'a', true));
    let position = first.cursor_position();
    assert_eq!((position.line, position.column), (1, 0));

    let second = &mut buffers[1];
    assert!(!vim_jump_to_mark(second, 'a', true));
}

#[test]
fn closing_buffer_purges_its_marks_without_touching_other_buffers() {
    vim_clear_named_registers();
    vim_clear_marks();
    let mut first = TextBuffer::from_text(21, None, "alpha\nbeta\n".to_owned());
    let mut second = TextBuffer::from_text(22, None, "one\ntwo\n".to_owned());
    set_mark_on_line(&mut first, 1, 0);
    set_mark_on_line(&mut second, 1, 0);

    vim_forget_marks_for_buffer(21);

    assert!(!vim_jump_to_mark(&mut first, 'a', true));
    assert!(vim_jump_to_mark(&mut second, 'a', true));
}

#[test]
fn workspace_style_clear_empties_the_mark_store() {
    vim_clear_named_registers();
    vim_clear_marks();
    let mut buffer = TextBuffer::from_text(31, None, "alpha\nbeta\n".to_owned());
    set_mark_on_line(&mut buffer, 0, 0);
    assert!(!vim_marks_are_empty_for_test());

    vim_clear_marks();

    assert!(vim_marks_are_empty_for_test());
    assert!(!vim_jump_to_mark(&mut buffer, 'a', true));
}

#[test]
fn open_line_above_mark_shifts_it_down() {
    vim_clear_named_registers();
    vim_clear_marks();
    let mut buffer = TextBuffer::from_text(41, None, "one\ntwo\nthree\n".to_owned());
    set_mark_on_line(&mut buffer, 1, 0);

    buffer.set_single_cursor(buffer.line_column_to_char(0, 0));
    press_keys(
        &mut buffer,
        &[(Key::O, Modifiers::SHIFT), (Key::Escape, Modifiers::NONE)],
    );

    buffer.set_single_cursor(0);
    press_keys(
        &mut buffer,
        &[(Key::Quote, Modifiers::NONE), (Key::A, Modifiers::NONE)],
    );
    assert_eq!(buffer.cursor_position().line, 2);
    assert_eq!(buffer.cursor_position().column, 0);
}

#[test]
fn open_line_below_keeps_marks_on_the_lines_below_the_cursor() {
    vim_clear_named_registers();
    vim_clear_marks();
    let mut buffer = TextBuffer::from_text(42, None, "one\ntwo\n".to_owned());
    set_mark_on_line(&mut buffer, 1, 0);

    buffer.set_single_cursor(buffer.line_column_to_char(0, 0));
    press_keys(
        &mut buffer,
        &[(Key::O, Modifiers::NONE), (Key::Escape, Modifiers::NONE)],
    );
    assert_eq!(buffer.text(), "one\n\ntwo\n");

    buffer.set_single_cursor(0);
    press_keys(
        &mut buffer,
        &[(Key::Quote, Modifiers::NONE), (Key::A, Modifiers::NONE)],
    );
    assert_eq!(buffer.cursor_position().line, 2);
    assert_eq!(buffer.cursor_position().column, 0);
}

#[test]
fn delete_line_above_mark_shifts_it_up() {
    vim_clear_named_registers();
    vim_clear_marks();
    let mut buffer = TextBuffer::from_text(43, None, "one\ntwo\nthree\n".to_owned());
    set_mark_on_line(&mut buffer, 2, 0);

    buffer.set_single_cursor(buffer.line_column_to_char(0, 0));
    press_keys(
        &mut buffer,
        &[(Key::D, Modifiers::NONE), (Key::D, Modifiers::NONE)],
    );
    assert_eq!(buffer.text(), "two\nthree\n");

    buffer.set_single_cursor(0);
    press_keys(
        &mut buffer,
        &[(Key::Quote, Modifiers::NONE), (Key::A, Modifiers::NONE)],
    );
    assert_eq!(buffer.cursor_position().line, 1);
}

#[test]
fn delete_marked_line_clears_the_mark() {
    vim_clear_named_registers();
    vim_clear_marks();
    let mut buffer = TextBuffer::from_text(44, None, "one\ntwo\nthree\n".to_owned());
    set_mark_on_line(&mut buffer, 1, 0);

    buffer.set_single_cursor(buffer.line_column_to_char(1, 0));
    press_keys(
        &mut buffer,
        &[(Key::D, Modifiers::NONE), (Key::D, Modifiers::NONE)],
    );
    assert_eq!(buffer.text(), "one\nthree\n");

    let cursor_before = buffer.cursor();
    assert!(!vim_jump_to_mark(&mut buffer, 'a', true));
    assert_eq!(buffer.cursor(), cursor_before);
}

#[test]
fn mark_adjustment_shifts_columns_on_same_line_insert() {
    vim_clear_marks();
    let mut buffer = TextBuffer::from_text(45, None, "hello world\n".to_owned());
    set_mark_on_line(&mut buffer, 0, 6);

    vim_adjust_marks_for_edit(45, (0, 0), (0, 0), ">>");

    buffer.set_single_cursor(0);
    assert!(vim_jump_to_mark(&mut buffer, 'a', false));
    assert_eq!(buffer.cursor_position().column, 8);
}

#[test]
fn mark_adjustment_clears_marks_inside_deleted_span() {
    vim_clear_marks();
    let mut buffer = TextBuffer::from_text(46, None, "one\ntwo\n".to_owned());
    set_mark_on_line(&mut buffer, 1, 2);

    vim_adjust_marks_for_edit(46, (1, 0), (2, 0), "");

    assert!(!vim_jump_to_mark(&mut buffer, 'a', true));
}

#[test]
fn mark_adjustment_pulls_back_marks_below_deleted_lines() {
    vim_clear_marks();
    let mut buffer = TextBuffer::from_text(47, None, "one\ntwo\nthree\nfour\n".to_owned());
    set_mark_on_line(&mut buffer, 3, 0);

    vim_adjust_marks_for_edit(47, (0, 0), (2, 0), "");

    buffer.set_single_cursor(0);
    assert!(vim_jump_to_mark(&mut buffer, 'a', true));
    assert_eq!(buffer.cursor_position().line, 1);
}
