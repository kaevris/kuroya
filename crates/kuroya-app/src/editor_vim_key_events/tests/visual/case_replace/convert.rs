use super::*;

#[test]
fn normal_mode_visual_character_case_commands_convert_selection() {
    let mut buffer = TextBuffer::from_text(1, None, "aBcD ef".to_owned());
    buffer.set_single_cursor(1);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = Some(EditorVimRegister {
        text: "kept".to_owned(),
        kind: EditorVimRegisterKind::Characterwise,
    });

    for key in [Key::V, Key::Num2, Key::L] {
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
        assert!(!result.changed);
    }

    assert_eq!(buffer.selected_text().as_deref(), Some("BcD"));
    let lower = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::U,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );

    assert!(lower.handled);
    assert!(lower.changed);
    assert_eq!(lower.suppress_text, Some('u'));
    assert_eq!(buffer.text(), "abcd ef");
    assert_eq!(buffer.cursor(), 1);
    assert!(!buffer.has_selection());
    assert_eq!(mode, EditorVimMode::Normal);
    assert!(pending.is_none());
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some(("kept", EditorVimRegisterKind::Characterwise))
    );

    buffer = TextBuffer::from_text(1, None, "abCd".to_owned());
    buffer.set_single_cursor(3);
    mode = EditorVimMode::Normal;
    pending = None;

    for key in [Key::V, Key::H] {
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

    assert_eq!(buffer.selected_text().as_deref(), Some("Cd"));
    let upper = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::U,
        Modifiers::SHIFT,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );

    assert!(upper.handled);
    assert!(upper.changed);
    assert_eq!(upper.suppress_text, Some('U'));
    assert_eq!(buffer.text(), "abCD");
    assert_eq!(buffer.cursor(), 2);
    assert!(!buffer.has_selection());
    assert!(pending.is_none());

    buffer = TextBuffer::from_text(1, None, "aBcD".to_owned());
    buffer.set_single_cursor(1);
    mode = EditorVimMode::Normal;
    pending = None;

    for key in [Key::V, Key::Num2, Key::L] {
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

    assert_eq!(buffer.selected_text().as_deref(), Some("BcD"));
    let toggle = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::Backtick,
        Modifiers::SHIFT,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );

    assert!(toggle.handled);
    assert!(toggle.changed);
    assert_eq!(toggle.suppress_text, Some('~'));
    assert_eq!(buffer.text(), "abCd");
    assert_eq!(buffer.cursor(), 1);
    assert!(!buffer.has_selection());
    assert!(pending.is_none());
    assert!(vim_events_include_mutation(
        &[
            key_event(Key::V, Modifiers::NONE),
            key_event(Key::L, Modifiers::NONE),
            key_event(Key::U, Modifiers::NONE),
        ],
        EditorVimMode::Normal,
        None,
    ));
    assert!(vim_events_include_mutation(
        &[
            key_event(Key::V, Modifiers::NONE),
            key_event(Key::L, Modifiers::NONE),
            key_event(Key::U, Modifiers::SHIFT),
        ],
        EditorVimMode::Normal,
        None,
    ));
    assert!(vim_events_include_mutation(
        &[
            key_event(Key::V, Modifiers::NONE),
            key_event(Key::L, Modifiers::NONE),
            key_event(Key::Backtick, Modifiers::SHIFT),
        ],
        EditorVimMode::Normal,
        None,
    ));
}

#[test]
fn normal_mode_visual_character_case_commands_convert_unicode_letters() {
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    let mut buffer = TextBuffer::from_text(1, None, "Ünïcode".to_owned());
    for key in [Key::V, Key::End] {
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
        assert!(!result.changed);
    }
    let upper = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::U,
        Modifiers::SHIFT,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );
    assert!(upper.handled);
    assert!(upper.changed);
    assert_eq!(buffer.text(), "ÜNÏCODE");

    buffer = TextBuffer::from_text(1, None, "Ünïcode".to_owned());
    for key in [Key::V, Key::End] {
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
        assert!(!result.changed);
    }
    let lower = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::U,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );
    assert!(lower.handled);
    assert!(lower.changed);
    assert_eq!(buffer.text(), "ünïcode");

    buffer = TextBuffer::from_text(1, None, "ß".to_owned());
    for key in [Key::V, Key::End] {
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
        assert!(!result.changed);
    }
    let sharp_s_upper = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::U,
        Modifiers::SHIFT,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );
    assert!(sharp_s_upper.handled);
    assert!(sharp_s_upper.changed);
    assert_eq!(buffer.text(), "SS");

    buffer = TextBuffer::from_text(1, None, "Ünï".to_owned());
    for key in [Key::V, Key::End] {
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
        assert!(!result.changed);
    }
    let toggle = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::Backtick,
        Modifiers::SHIFT,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );
    assert!(toggle.handled);
    assert!(toggle.changed);
    assert_eq!(buffer.text(), "üNÏ");
}
