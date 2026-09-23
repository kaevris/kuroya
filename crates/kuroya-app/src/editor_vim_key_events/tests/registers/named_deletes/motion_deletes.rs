use super::*;

#[test]
fn normal_mode_named_register_delete_motion_and_put_before() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(1, None, "alpha beta gamma".to_owned());
    buffer.set_single_cursor(0);
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
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some(("alpha ", EditorVimRegisterKind::Characterwise))
    );

    buffer.set_single_cursor(0);
    for key in [Key::Y, Key::W] {
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
        unnamed_register
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some(("beta ", EditorVimRegisterKind::Characterwise))
    );

    buffer.set_single_cursor(0);
    for (key, modifiers) in [
        (Key::Quote, Modifiers::SHIFT),
        (Key::A, Modifiers::NONE),
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

    assert_eq!(buffer.text(), "alpha beta gamma");
    assert!(pending.is_none());
    assert!(vim_events_include_mutation(
        &[
            Event::Key {
                key: Key::Quote,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::SHIFT,
            },
            Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            Event::Key {
                key: Key::D,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            Event::Key {
                key: Key::W,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
        ],
        EditorVimMode::Normal,
        None,
    ));
}

#[test]
fn normal_mode_named_register_accepts_multidigit_count_after_register() {
    vim_clear_named_registers();
    let text = (1..=14)
        .map(|line| format!("line {line:02}\n"))
        .collect::<Vec<_>>()
        .join("");
    let mut buffer = TextBuffer::from_text(1, None, text);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for (key, modifiers) in [
        (Key::Quote, Modifiers::SHIFT),
        (Key::A, Modifiers::NONE),
        (Key::Num1, Modifiers::NONE),
        (Key::Num2, Modifiers::NONE),
        (Key::D, Modifiers::NONE),
        (Key::D, Modifiers::NONE),
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

    let expected_deleted = (1..=12)
        .map(|line| format!("line {line:02}\n"))
        .collect::<Vec<_>>()
        .join("");
    let register_a = vim_named_register(EditorVimNamedRegister {
        index: 0,
        append: false,
    });
    assert_eq!(mode, EditorVimMode::Normal);
    assert_eq!(buffer.text(), "line 13\nline 14\n");
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some((expected_deleted.as_str(), EditorVimRegisterKind::Linewise))
    );
    assert_eq!(
        register_a
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some((expected_deleted.as_str(), EditorVimRegisterKind::Linewise))
    );
    assert!(pending.is_none());
    assert!(vim_events_include_mutation(
        &[
            key_event(Key::Quote, Modifiers::SHIFT),
            key_event(Key::A, Modifiers::NONE),
            key_event(Key::Num1, Modifiers::NONE),
            key_event(Key::Num2, Modifiers::NONE),
            key_event(Key::D, Modifiers::NONE),
            key_event(Key::D, Modifiers::NONE),
        ],
        EditorVimMode::Normal,
        None,
    ));

    vim_clear_named_registers();
    buffer = TextBuffer::from_text(1, None, "abcdefghijklmnop".to_owned());
    pending = None;
    unnamed_register = None;

    for (key, modifiers) in [
        (Key::Quote, Modifiers::SHIFT),
        (Key::B, Modifiers::NONE),
        (Key::Num1, Modifiers::NONE),
        (Key::Num2, Modifiers::NONE),
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

    let register_b = vim_named_register(EditorVimNamedRegister {
        index: 1,
        append: false,
    });
    assert_eq!(buffer.text(), "mnop");
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some(("abcdefghijkl", EditorVimRegisterKind::Characterwise))
    );
    assert_eq!(
        register_b
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some(("abcdefghijkl", EditorVimRegisterKind::Characterwise))
    );
    assert!(pending.is_none());
    assert!(vim_events_include_mutation(
        &[
            key_event(Key::Quote, Modifiers::SHIFT),
            key_event(Key::B, Modifiers::NONE),
            key_event(Key::Num1, Modifiers::NONE),
            key_event(Key::Num2, Modifiers::NONE),
            key_event(Key::X, Modifiers::NONE),
        ],
        EditorVimMode::Normal,
        None,
    ));
}
