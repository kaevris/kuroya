use super::*;

#[test]
fn motion_audit_semicolon_after_till_repeats_force_advance() {
    let mut buffer = TextBuffer::from_text(1, None, "axbx".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    for (key, modifiers) in [(Key::T, Modifiers::NONE), (Key::X, Modifiers::NONE)] {
        let r = handle_vim_editor_key_event_with_state(
            &mut buffer,
            key,
            modifiers,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
        );
        assert!(r.handled);
    }
    assert_eq!(buffer.cursor(), 0, "tx with adjacent target stays");
    let r = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::Semicolon,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );
    assert!(r.handled);
    assert_eq!(
        buffer.cursor(),
        2,
        "; after t advances to before the NEXT x"
    );
    let r = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::Semicolon,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );
    assert!(r.handled);
    assert_eq!(buffer.cursor(), 2, "no further x: stays");

    let mut buffer = TextBuffer::from_text(1, None, "xabxabx".to_owned());
    buffer.set_single_cursor(5);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    for (key, modifiers) in [(Key::T, Modifiers::SHIFT), (Key::X, Modifiers::NONE)] {
        let r = handle_vim_editor_key_event_with_state(
            &mut buffer,
            key,
            modifiers,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
        );
        assert!(r.handled);
    }
    assert_eq!(buffer.cursor(), 4, "Tx lands just after the target x@3");
    let r = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::Comma,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );
    assert!(r.handled);
    assert_eq!(
        buffer.cursor(),
        5,
        ", after T forces past the old target to x@6"
    );
}

#[test]
fn motion_audit_h_and_l_stop_at_line_boundaries() {
    let mut buffer = TextBuffer::from_text(1, None, "ab\ncd".to_owned());
    buffer.set_single_cursor(1);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let r = handle_vim_editor_key_event(
        &mut buffer,
        Key::L,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
    );
    assert!(r.handled);
    assert_eq!(buffer.cursor(), 1, "l at line end stays off the newline");
    buffer.set_single_cursor(3);
    let r = handle_vim_editor_key_event(
        &mut buffer,
        Key::H,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
    );
    assert!(r.handled);
    assert_eq!(buffer.cursor(), 3, "h at column 0 stays");
    buffer.set_single_cursor(5);
    let r = handle_vim_editor_key_event(
        &mut buffer,
        Key::L,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
    );
    assert!(r.handled);
    assert_eq!(buffer.cursor(), 5, "l at buffer end stays");
}

#[test]
fn motion_audit_big_and_backward_words_stop_on_empty_lines() {
    let mut buffer = TextBuffer::from_text(1, None, "ab\n\ncd".to_owned());
    buffer.set_single_cursor(0);
    buffer.move_big_word_right();
    assert_eq!(buffer.cursor(), 3, "W stops on the empty line like w");
    buffer.set_single_cursor(5);
    buffer.move_big_word_left();
    assert_eq!(buffer.cursor(), 4, "B to the start of cd");
    buffer.move_big_word_left();
    assert_eq!(buffer.cursor(), 3, "B stops on the empty line");
    buffer.set_single_cursor(5);
    buffer.move_word_left();
    assert_eq!(buffer.cursor(), 4, "b to the start of cd");
    buffer.move_word_left();
    assert_eq!(buffer.cursor(), 3, "b stops on the empty line");
}
