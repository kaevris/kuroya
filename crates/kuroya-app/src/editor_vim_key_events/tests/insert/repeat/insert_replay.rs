use super::*;

#[test]
fn normal_mode_period_replays_inserted_text() {
    let mut buffer = TextBuffer::from_text(1, None, "ab".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    let mut last_change = None;

    let insert = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::I,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(insert.handled);
    assert_eq!(mode, EditorVimMode::Insert);
    buffer.insert_at_cursors("X");
    vim_record_inserted_text(&mut last_change, "X");
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
    let repeat = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::Period,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(repeat.handled);
    assert!(repeat.changed);
    assert_eq!(mode, EditorVimMode::Normal);
    assert_eq!(buffer.text(), "XXab");
}

#[test]
fn normal_mode_period_replays_substitute_inserted_text() {
    let mut buffer = TextBuffer::from_text(1, None, "abc".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    let mut last_change = None;

    let substitute = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::S,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(substitute.handled);
    assert_eq!(mode, EditorVimMode::Insert);
    buffer.insert_at_cursors("X");
    vim_record_inserted_text(&mut last_change, "X");
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
    buffer.set_single_cursor(1);
    let repeat = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::Period,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );

    assert!(repeat.handled);
    assert!(repeat.changed);
    assert_eq!(mode, EditorVimMode::Normal);
    assert_eq!(buffer.text(), "XXc");
}

#[test]
fn normal_mode_period_replays_insert_backspace() {
    let mut buffer = TextBuffer::from_text(1, None, String::new());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    let mut last_change = None;

    let insert = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::I,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(insert.handled);
    assert_eq!(mode, EditorVimMode::Insert);
    buffer.insert_at_cursors("a");
    vim_record_inserted_text(&mut last_change, "a");
    assert!(buffer.delete_backward_with_auto_pair_delete(false));
    vim_record_insert_replay_key_with_auto_indent(
        &mut last_change,
        Key::Backspace,
        Modifiers::NONE,
        false,
    );
    buffer.insert_at_cursors("b");
    vim_record_inserted_text(&mut last_change, "b");
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
    let repeat = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::Period,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(repeat.handled);
    assert!(repeat.changed);
    assert_eq!(mode, EditorVimMode::Normal);
    assert_eq!(buffer.text(), "bb");
}

#[test]
fn normal_mode_period_replays_insert_enter_and_tab() {
    let mut buffer = TextBuffer::from_text(1, None, String::new());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    let mut last_change = None;

    let insert = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::I,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(insert.handled);
    buffer.insert_at_cursors("a");
    vim_record_inserted_text(&mut last_change, "a");
    buffer.insert_at_cursors("\n");
    vim_record_insert_replay_key_with_auto_indent(
        &mut last_change,
        Key::Enter,
        Modifiers::NONE,
        false,
    );
    buffer.insert_at_cursors("    ");
    vim_record_insert_replay_key_with_auto_indent(
        &mut last_change,
        Key::Tab,
        Modifiers::NONE,
        false,
    );
    buffer.insert_at_cursors("b");
    vim_record_inserted_text(&mut last_change, "b");
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

    buffer.set_single_cursor(buffer.len_chars());
    let repeat = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::Period,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(repeat.handled);
    assert!(repeat.changed);
    assert_eq!(mode, EditorVimMode::Normal);
    assert_eq!(buffer.text(), "a\n    ba\n    b");
}
