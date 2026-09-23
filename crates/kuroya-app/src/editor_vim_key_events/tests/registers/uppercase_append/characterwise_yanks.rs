use super::*;

#[test]
fn normal_mode_uppercase_named_register_appends_characterwise_yanks() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(1, None, "alpha beta gamma".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for (key, modifiers) in [
        (Key::Quote, Modifiers::SHIFT),
        (Key::A, Modifiers::NONE),
        (Key::D, Modifiers::NONE),
        (Key::W, Modifiers::NONE),
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

    assert_eq!(buffer.text(), "beta gamma");
    buffer.set_single_cursor(0);
    for (key, modifiers) in [
        (Key::Quote, Modifiers::SHIFT),
        (Key::A, Modifiers::SHIFT),
        (Key::Y, Modifiers::NONE),
        (Key::W, Modifiers::NONE),
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
        assert!(!result.changed);
    }

    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some(("beta ", EditorVimRegisterKind::Characterwise))
    );

    buffer.set_single_cursor(0);
    for (key, modifiers) in [
        (Key::Quote, Modifiers::SHIFT),
        (Key::A, Modifiers::SHIFT),
        (Key::P, Modifiers::SHIFT),
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

    assert_eq!(buffer.text(), "alpha beta beta gamma");
    assert!(pending.is_none());
}
