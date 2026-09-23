use super::*;

#[test]
fn normal_mode_black_hole_register_delete_discards_without_replacing_unnamed() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(1, None, "alpha beta gamma".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(0, 6));
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

    assert_eq!(buffer.text(), "alpha gamma");
    assert_eq!(buffer.cursor(), buffer.line_column_to_char(0, 6));
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some(("seed", EditorVimRegisterKind::Characterwise))
    );
    assert!(pending.is_none());
    assert!(vim_events_include_mutation(
        &[
            key_event(Key::Quote, Modifiers::SHIFT),
            key_event(Key::Minus, Modifiers::SHIFT),
            key_event(Key::D, Modifiers::NONE),
            key_event(Key::W, Modifiers::NONE),
        ],
        EditorVimMode::Normal,
        None,
    ));
}

#[test]
fn normal_mode_black_hole_register_yank_discards_without_replacing_unnamed() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(1, None, "alpha beta gamma".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(0, 6));
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

    assert_eq!(buffer.text(), "alpha beta gamma");
    assert_eq!(buffer.cursor(), buffer.line_column_to_char(0, 6));
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some(("seed", EditorVimRegisterKind::Characterwise))
    );
    assert!(pending.is_none());
    assert!(!vim_events_include_mutation(
        &[
            key_event(Key::Quote, Modifiers::SHIFT),
            key_event(Key::Minus, Modifiers::SHIFT),
            key_event(Key::Y, Modifiers::NONE),
            key_event(Key::W, Modifiers::NONE),
        ],
        EditorVimMode::Normal,
        None,
    ));
}
