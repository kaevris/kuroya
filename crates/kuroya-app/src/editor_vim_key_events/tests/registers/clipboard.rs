use super::*;

#[test]
fn normal_mode_quote_plus_yank_line_pastes_back_from_the_clipboard_register() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(11, None, "alpha\nbeta\n".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for (key, modifiers) in [
        (Key::Quote, Modifiers::SHIFT),
        (Key::Equals, Modifiers::SHIFT),
        (Key::Y, Modifiers::NONE),
        (Key::Y, Modifiers::NONE),
        (Key::J, Modifiers::NONE),
        (Key::Quote, Modifiers::SHIFT),
        (Key::Equals, Modifiers::SHIFT),
        (Key::P, Modifiers::NONE),
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

    assert_eq!(buffer.text(), "alpha\nbeta\nalpha\n");
    assert!(pending.is_none());
}

#[test]
fn normal_mode_plus_and_star_spellings_share_the_clipboard_register() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(12, None, "alpha\nbeta\n".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for (key, modifiers) in [
        (Key::Quote, Modifiers::SHIFT),
        (Key::Equals, Modifiers::SHIFT),
        (Key::Y, Modifiers::NONE),
        (Key::Y, Modifiers::NONE),
        (Key::J, Modifiers::NONE),
        (Key::Quote, Modifiers::SHIFT),
        (Key::Num8, Modifiers::SHIFT),
        (Key::P, Modifiers::NONE),
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

    assert_eq!(buffer.text(), "alpha\nbeta\nalpha\n");
}

#[test]
fn normal_mode_clipboard_register_write_also_fills_unnamed() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(13, None, "alpha\nbeta\n".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for (key, modifiers) in [
        (Key::Quote, Modifiers::SHIFT),
        (Key::Equals, Modifiers::SHIFT),
        (Key::Y, Modifiers::NONE),
        (Key::Y, Modifiers::NONE),
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

    let put = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::P,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );

    assert!(put.handled);
    assert!(put.changed);
    assert_eq!(buffer.text(), "alpha\nbeta\nalpha\n");
}

#[test]
fn normal_mode_quote_plus_small_delete_pastes_from_the_clipboard_register() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(14, None, "abcdef".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for (key, modifiers) in [
        (Key::Quote, Modifiers::SHIFT),
        (Key::Equals, Modifiers::SHIFT),
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
    assert_eq!(buffer.text(), "bcdef");

    for (key, modifiers) in [
        (Key::Quote, Modifiers::SHIFT),
        (Key::Equals, Modifiers::SHIFT),
        (Key::P, Modifiers::NONE),
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

    assert_eq!(buffer.text(), "bacdef");
}
