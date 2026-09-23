use super::*;

#[test]
fn normal_mode_visual_character_escape_lands_cursor_on_selection_anchor() {
    let mut buffer = TextBuffer::from_text(1, None, "one two three".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for key in [Key::V, Key::W] {
        let result = handle_vim_editor_key_event_with_state(
            &mut buffer,
            key,
            Modifiers::NONE,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
        );
        assert!(result.handled);
    }
    assert_eq!(
        pending,
        Some(EditorVimPendingKey::VisualCharacter {
            anchor: 0,
            cursor: 4,
        })
    );

    let escape = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::Escape,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );

    assert!(escape.handled);
    assert_eq!(
        buffer.cursor(),
        0,
        "plain exit lands on the selection start"
    );
    assert!(pending.is_none());
}

#[test]
fn normal_mode_visual_character_v_toggle_off_lands_cursor_on_selection_start() {
    let mut buffer = TextBuffer::from_text(2, None, "one two three".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for key in [Key::V, Key::W, Key::V] {
        let result = handle_vim_editor_key_event_with_state(
            &mut buffer,
            key,
            Modifiers::NONE,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
        );
        assert!(result.handled);
    }

    assert_eq!(buffer.cursor(), 0);
    assert!(pending.is_none());
}

#[test]
fn normal_mode_visual_character_escape_after_reversed_selection_lands_on_lower_end() {
    let mut buffer = TextBuffer::from_text(3, None, "one two three".to_owned());
    buffer.set_single_cursor(4);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for key in [Key::V, Key::B, Key::O] {
        let result = handle_vim_editor_key_event_with_state(
            &mut buffer,
            key,
            Modifiers::NONE,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
        );
        assert!(result.handled);
    }
    assert_eq!(
        pending,
        Some(EditorVimPendingKey::VisualCharacter {
            anchor: 0,
            cursor: 4,
        })
    );

    let escape = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::Escape,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );

    assert!(escape.handled);
    assert_eq!(buffer.cursor(), 0);
}

#[test]
fn normal_mode_visual_line_escape_lands_cursor_on_the_anchor_line() {
    let mut buffer = TextBuffer::from_text(4, None, "alpha\nbeta\n".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for (key, modifiers) in [(Key::V, Modifiers::SHIFT), (Key::J, Modifiers::NONE)] {
        let result = handle_vim_editor_key_event_with_state(
            &mut buffer,
            key,
            modifiers,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
        );
        assert!(result.handled);
    }

    let escape = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::Escape,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );

    assert!(escape.handled);
    assert_eq!(buffer.cursor(), 0, "linewise exit lands on the anchor line");
}

#[test]
fn normal_mode_visual_exit_records_less_than_and_greater_than_marks() {
    vim_clear_marks();
    let mut buffer = TextBuffer::from_text(5, None, "one two\nthree four\nlast\n".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for (key, modifiers) in [
        (Key::V, Modifiers::NONE),
        (Key::W, Modifiers::NONE),
        (Key::J, Modifiers::NONE),
    ] {
        let result = handle_vim_editor_key_event_with_state(
            &mut buffer,
            key,
            modifiers,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
        );
        assert!(result.handled);
    }
    let escape = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::Escape,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );
    assert!(escape.handled);
    assert_eq!(buffer.cursor(), 0);

    let go_last = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::G,
        Modifiers::SHIFT,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );
    assert!(go_last.handled);
    assert_eq!(buffer.cursor(), buffer.line_column_to_char(2, 0));

    for (key, modifiers, _expected) in [
        (Key::Quote, Modifiers::NONE, 0),
        (Key::Comma, Modifiers::SHIFT, 0),
    ] {
        let result = handle_vim_editor_key_event_with_state(
            &mut buffer,
            key,
            modifiers,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
        );
        assert!(result.handled, "jump mark keystroke");
    }
    assert_eq!(
        buffer.cursor(),
        0,
        "`'<` jumps to the first line of the last visual area"
    );

    let go_last_again = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::G,
        Modifiers::SHIFT,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );
    assert!(go_last_again.handled);

    for (key, modifiers) in [
        (Key::Quote, Modifiers::NONE),
        (Key::Period, Modifiers::SHIFT),
    ] {
        let result = handle_vim_editor_key_event_with_state(
            &mut buffer,
            key,
            modifiers,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
        );
        assert!(result.handled, "jump mark keystroke");
    }
    assert_eq!(
        buffer.cursor(),
        buffer.line_column_to_char(1, 0),
        "`'>` jumps to the last line of the last visual area"
    );
}

#[test]
fn normal_mode_double_quote_jump_returns_to_position_before_last_jump() {
    vim_clear_marks();
    let mut buffer = TextBuffer::from_text(7, None, "alpha\nbeta\ngamma\n".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    let go_last = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::G,
        Modifiers::SHIFT,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );
    assert!(go_last.handled);
    assert_eq!(buffer.cursor(), buffer.line_column_to_char(2, 0));

    for _ in 0..2 {
        let result = handle_vim_editor_key_event_with_state(
            &mut buffer,
            Key::Quote,
            Modifiers::NONE,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
        );
        assert!(result.handled, "'' keystroke");
    }

    assert_eq!(
        buffer.cursor(),
        0,
        "`''` returns to the position before the last jump"
    );
}
