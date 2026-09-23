use super::*;

#[test]
fn normal_mode_v_enters_characterwise_visual_and_yanks_motion_selection() {
    let mut buffer = TextBuffer::from_text(1, None, "abcdef".to_owned());
    buffer.set_single_cursor(1);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

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

    assert_eq!(mode, EditorVimMode::Normal);
    assert_eq!(buffer.selected_text().as_deref(), Some("bcd"));
    assert_eq!(
        pending,
        Some(EditorVimPendingKey::VisualCharacter {
            anchor: 1,
            cursor: 3,
        })
    );

    let yank = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::Y,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );

    assert!(yank.handled);
    assert!(!yank.changed);
    assert_eq!(buffer.text(), "abcdef");
    assert_eq!(buffer.cursor(), 1);
    assert!(!buffer.has_selection());
    assert!(pending.is_none());
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some(("bcd", EditorVimRegisterKind::Characterwise))
    );
    assert!(!vim_events_include_mutation(
        &[
            key_event(Key::V, Modifiers::NONE),
            key_event(Key::Num2, Modifiers::NONE),
            key_event(Key::L, Modifiers::NONE),
            key_event(Key::Y, Modifiers::NONE),
        ],
        EditorVimMode::Normal,
        None,
    ));
}

#[test]
fn visual_character_y_over_empty_line_yanks_without_trailing_newline_and_pastes() {
    let mut buffer = TextBuffer::from_text(1, None, "one\n\nthree\n".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for key in [Key::V, Key::J] {
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

    let yank = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::Y,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );

    assert!(yank.handled);
    assert!(!yank.changed);
    assert_eq!(buffer.text(), "one\n\nthree\n");
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some(("one\n", EditorVimRegisterKind::Characterwise))
    );
    assert!(pending.is_none());

    buffer.set_single_cursor(buffer.line_column_to_char(2, 0));
    let put_before = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::P,
        Modifiers::SHIFT,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );

    assert!(put_before.handled);
    assert!(put_before.changed);
    assert_eq!(buffer.text(), "one\n\none\nthree\n");
}
