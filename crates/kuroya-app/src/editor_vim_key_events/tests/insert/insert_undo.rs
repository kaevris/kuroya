use super::*;

#[test]
fn insert_session_undoes_in_one_step() {
    let mut buffer = TextBuffer::from_text(1, None, "one two three".to_owned());
    buffer.set_single_cursor(4);
    assert_eq!(buffer.undo_entry_count(), 0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    let mut last_change = None;

    for key in [Key::C, Key::I, Key::W] {
        let result = handle_vim_editor_key_event_with_repeat_state(
            &mut buffer,
            key,
            Modifiers::NONE,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
            &mut last_change,
        );
        assert!(result.handled);
    }
    assert_eq!(mode, EditorVimMode::Insert);
    assert_eq!(buffer.text(), "one  three");

    buffer.insert_at_cursors("Hello world");
    vim_record_inserted_text(&mut last_change, "Hello world");

    let escape = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::Escape,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(escape.handled);
    assert_eq!(mode, EditorVimMode::Normal);
    assert_eq!(buffer.text(), "one Hello world three");

    assert_eq!(buffer.undo_entry_count(), 1);

    let undo = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::U,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(undo.handled);
    assert_eq!(buffer.text(), "one two three");

    assert_eq!(buffer.selections()[0].anchor, 4);
    assert!(pending.is_none());
}

#[test]
fn open_line_and_typing_undo_in_one_step() {
    let mut buffer = TextBuffer::from_text(1, None, "one".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    let mut last_change = None;

    let open = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::O,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(open.handled);
    assert_eq!(mode, EditorVimMode::Insert);

    buffer.insert_at_cursors("hello");
    vim_record_inserted_text(&mut last_change, "hello");

    let escape = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::Escape,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(escape.handled);
    assert_eq!(buffer.text(), "one\nhello");
    assert_eq!(buffer.undo_entry_count(), 1);

    let undo = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::U,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(undo.handled);
    assert_eq!(buffer.text(), "one");
    assert!(pending.is_none());
}

#[test]
fn normal_mode_edits_keep_separate_undo_entries() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha beta\ngamma".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    let mut last_change = None;

    for key in [Key::X, Key::D, Key::D] {
        let result = handle_vim_editor_key_event_with_repeat_state(
            &mut buffer,
            key,
            Modifiers::NONE,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
            &mut last_change,
        );
        assert!(result.handled);
    }
    assert_eq!(buffer.text(), "gamma");
    assert_eq!(buffer.undo_entry_count(), 2);

    let undo_dd = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::U,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(undo_dd.handled);
    assert_eq!(buffer.text(), "lpha beta\ngamma");

    let undo_x = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::U,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(undo_x.handled);
    assert_eq!(buffer.text(), "alpha beta\ngamma");
}
