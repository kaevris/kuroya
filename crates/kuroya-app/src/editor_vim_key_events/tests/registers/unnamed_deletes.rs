use super::*;

#[test]
fn normal_mode_x_fills_unnamed_register_after_yank_and_put_pastes_it() {
    vim_clear_named_registers();

    let mut buffer = TextBuffer::from_text(1, None, "alpha\nbeta\n".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for (key, modifiers) in [
        (Key::Y, Modifiers::NONE),
        (Key::Y, Modifiers::NONE),
        (Key::J, Modifiers::NONE),
        (Key::X, Modifiers::NONE),
        (Key::P, Modifiers::NONE),
    ] {
        handle_vim_editor_key_event_with_state(
            &mut buffer,
            key,
            modifiers,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
        );
    }

    assert_eq!(buffer.text(), "alpha\nebta\n");
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| register.text.as_str()),
        Some("b")
    );
}

#[test]
fn normal_mode_capital_x_fills_unnamed_register() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(2, None, "beta\n".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(0, 1));
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for (key, modifiers) in [(Key::X, Modifiers::SHIFT), (Key::P, Modifiers::NONE)] {
        handle_vim_editor_key_event_with_state(
            &mut buffer,
            key,
            modifiers,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
        );
    }

    assert_eq!(buffer.text(), "ebta\n");
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| register.text.as_str()),
        Some("b")
    );
}

#[test]
fn normal_mode_substitute_fills_unnamed_register() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(3, None, "beta\n".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for (key, modifiers) in [(Key::S, Modifiers::NONE), (Key::Escape, Modifiers::NONE)] {
        handle_vim_editor_key_event_with_state(
            &mut buffer,
            key,
            modifiers,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
        );
    }

    assert_eq!(mode, EditorVimMode::Normal);
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| register.text.as_str()),
        Some("b")
    );
    assert_eq!(
        unnamed_register.as_ref().map(|register| register.kind),
        Some(EditorVimRegisterKind::Characterwise)
    );

    buffer.set_single_cursor(buffer.line_column_to_char(0, 2));
    let put = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::P,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );
    assert!(put.changed);

    assert_eq!(buffer.text(), "etab\n");
}

#[test]
fn normal_mode_delete_to_line_end_fills_unnamed_register() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(4, None, "alpha beta\n".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(0, 5));
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    let mut last_change = None;

    let result = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::D,
        Modifiers::SHIFT,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );

    assert!(result.changed);
    assert_eq!(buffer.text(), "alpha\n");
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| register.text.as_str()),
        Some(" beta")
    );

    buffer.set_single_cursor(buffer.line_column_to_char(0, 1));
    handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::Period,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert_eq!(buffer.text(), "a\n");
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| register.text.as_str()),
        Some("lpha")
    );
}

#[test]
fn normal_mode_change_to_line_end_fills_unnamed_register() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(5, None, "alpha beta\n".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(0, 5));
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    let result = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::C,
        Modifiers::SHIFT,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );

    assert!(result.changed);
    assert_eq!(mode, EditorVimMode::Insert);
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| register.text.as_str()),
        Some(" beta")
    );
}

#[test]
fn normal_mode_dot_repeat_x_fills_unnamed_register() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(6, None, "aabbcc\n".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    let mut last_change = None;

    for (key, modifiers) in [
        (Key::X, Modifiers::NONE),
        (Key::Period, Modifiers::NONE),
        (Key::P, Modifiers::NONE),
    ] {
        handle_vim_editor_key_event_with_repeat_state(
            &mut buffer,
            key,
            modifiers,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
            &mut last_change,
        );
    }

    assert_eq!(buffer.text(), "babcc\n");
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| register.text.as_str()),
        Some("a")
    );
}

#[test]
fn normal_mode_black_hole_x_leaves_unnamed_register_untouched() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(7, None, "beta\n".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = Some(EditorVimRegister {
        text: "seed".to_owned(),
        kind: EditorVimRegisterKind::Characterwise,
    });

    for (key, modifiers) in [
        (Key::Quote, Modifiers::SHIFT),
        (Key::Minus, Modifiers::SHIFT),
        (Key::X, Modifiers::NONE),
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

    assert_eq!(buffer.text(), "eta\n");
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| register.text.as_str()),
        Some("seed")
    );
}
