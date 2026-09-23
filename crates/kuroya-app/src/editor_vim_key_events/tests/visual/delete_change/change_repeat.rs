use super::*;

#[test]
fn normal_mode_visual_character_c_changes_selection_and_repeats_insert() {
    let mut buffer = TextBuffer::from_text(1, None, "abcdef ghij".to_owned());
    buffer.set_single_cursor(1);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    let mut last_change = None;

    for key in [Key::V, Key::Num2, Key::L] {
        let result = handle_vim_editor_key_event_with_repeat_state(
            &mut buffer,
            key,
            Modifiers::NONE,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
            &mut last_change,
        );
        assert!(result.handled);
        assert!(!result.changed);
    }

    assert_eq!(buffer.selected_text().as_deref(), Some("bcd"));
    let change = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::C,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );

    assert!(change.handled);
    assert!(change.changed);
    assert_eq!(change.suppress_text, Some('c'));
    assert_eq!(buffer.text(), "aef ghij");
    assert_eq!(buffer.cursor(), 1);
    assert_eq!(mode, EditorVimMode::Insert);
    assert!(pending.is_none());
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some(("bcd", EditorVimRegisterKind::Characterwise))
    );

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

    buffer.set_single_cursor(5);
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
    assert_eq!(buffer.text(), "aXef Xj");
    assert_eq!(mode, EditorVimMode::Normal);
    assert!(pending.is_none());
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some(("ghi", EditorVimRegisterKind::Characterwise))
    );
    assert!(vim_events_include_mutation(
        &[
            key_event(Key::V, Modifiers::NONE),
            key_event(Key::Num2, Modifiers::NONE),
            key_event(Key::L, Modifiers::NONE),
            key_event(Key::C, Modifiers::NONE),
        ],
        EditorVimMode::Normal,
        None,
    ));
}
