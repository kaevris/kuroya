use super::*;

#[test]
fn normal_mode_visual_character_shift_period_indents_selected_line_span_and_repeats() {
    let mut buffer =
        TextBuffer::from_text(1, None, "one\ntwo\nthree\nfour\nfive\nsix\n".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    let mut last_change = None;

    for (key, modifiers) in [
        (Key::V, Modifiers::NONE),
        (Key::J, Modifiers::NONE),
        (Key::J, Modifiers::NONE),
    ] {
        let result = handle_vim_editor_key_event_with_state_and_indent(
            &mut buffer,
            key,
            modifiers,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
            &mut last_change,
            "  ",
        );
        assert!(result.handled);
        assert!(!result.changed);
    }

    let indent = handle_vim_editor_key_event_with_state_and_indent(
        &mut buffer,
        Key::Period,
        Modifiers::SHIFT,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
        "  ",
    );

    assert!(indent.handled);
    assert!(indent.changed);
    assert_eq!(indent.suppress_text, Some('>'));
    assert_eq!(buffer.text(), "  one\n  two\n  three\nfour\nfive\nsix\n");
    assert_eq!(buffer.cursor(), buffer.line_column_to_char(0, 2));
    assert!(!buffer.has_selection());
    assert_eq!(mode, EditorVimMode::Normal);
    assert!(pending.is_none());
    assert!(last_change.is_some());

    buffer.set_single_cursor(buffer.line_column_to_char(3, 0));
    let repeat = handle_vim_editor_key_event_with_state_and_indent(
        &mut buffer,
        Key::Period,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
        "  ",
    );

    assert!(repeat.handled);
    assert!(repeat.changed);
    assert_eq!(
        buffer.text(),
        "  one\n  two\n  three\n  four\n  five\n  six\n"
    );
    assert_eq!(buffer.cursor(), buffer.line_column_to_char(3, 2));
    assert!(pending.is_none());
    assert!(vim_events_include_mutation(
        &[
            key_event(Key::V, Modifiers::NONE),
            key_event(Key::J, Modifiers::NONE),
            key_event(Key::J, Modifiers::NONE),
            key_event(Key::Period, Modifiers::SHIFT),
        ],
        EditorVimMode::Normal,
        None,
    ));
}

#[test]
fn normal_mode_visual_character_count_shift_period_indents_by_that_many_units() {
    let mut buffer = TextBuffer::from_text(1, None, "one\ntwo\n".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    let mut last_change = None;

    for (key, modifiers) in [
        (Key::V, Modifiers::NONE),
        (Key::Num2, Modifiers::NONE),
        (Key::Period, Modifiers::SHIFT),
    ] {
        let result = handle_vim_editor_key_event_with_state_and_indent(
            &mut buffer,
            key,
            modifiers,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
            &mut last_change,
            "  ",
        );
        assert!(result.handled);
    }

    assert_eq!(buffer.text(), "    one\ntwo\n");
    assert_eq!(buffer.cursor(), buffer.line_column_to_char(0, 4));
    assert!(pending.is_none());
}

#[test]
fn normal_mode_visual_line_count_shift_period_indents_by_that_many_units() {
    let mut buffer = TextBuffer::from_text(8, None, "one\ntwo\n".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    let mut last_change = None;

    for (key, modifiers) in [
        (Key::V, Modifiers::SHIFT),
        (Key::Num2, Modifiers::NONE),
        (Key::Period, Modifiers::SHIFT),
    ] {
        let result = handle_vim_editor_key_event_with_state_and_indent(
            &mut buffer,
            key,
            modifiers,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
            &mut last_change,
            "  ",
        );
        assert!(result.handled);
    }

    assert_eq!(buffer.text(), "    one\ntwo\n");
    assert!(pending.is_none());
}
