use super::*;

#[test]
fn normal_mode_visual_character_d_deletes_selection_into_register() {
    let mut buffer = TextBuffer::from_text(1, None, "abcdef".to_owned());
    buffer.set_single_cursor(4);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for key in [Key::V, Key::H, Key::H] {
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

    assert_eq!(buffer.selected_text().as_deref(), Some("cde"));
    let delete = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::D,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );

    assert!(delete.handled);
    assert!(delete.changed);
    assert_eq!(buffer.text(), "abf");
    assert_eq!(buffer.cursor(), 2);
    assert_eq!(mode, EditorVimMode::Normal);
    assert!(pending.is_none());
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some(("cde", EditorVimRegisterKind::Characterwise))
    );
    assert!(vim_events_include_mutation(
        &[
            key_event(Key::V, Modifiers::NONE),
            key_event(Key::H, Modifiers::NONE),
            key_event(Key::H, Modifiers::NONE),
            key_event(Key::D, Modifiers::NONE),
        ],
        EditorVimMode::Normal,
        None,
    ));
}

#[test]
fn visual_character_d_over_empty_line_keeps_the_empty_line() {
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

    let delete = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::D,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );

    assert!(delete.handled);
    assert!(delete.changed);
    assert_eq!(buffer.text(), "\nthree\n");
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| register.text.as_str()),
        Some("one\n")
    );
}

#[test]
fn visual_character_d_deletes_whole_grapheme_cluster_at_buffer_end() {
    let mut buffer = TextBuffer::from_text(1, None, "x\u{1F1FA}\u{1F1F8}".to_owned());
    buffer.set_single_cursor(buffer.len_chars());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    let enter = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::V,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );
    assert!(enter.handled);
    assert!(!enter.changed);

    let delete = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::D,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );

    assert!(delete.handled);
    assert!(delete.changed);
    assert_eq!(buffer.text(), "x");
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| register.text.as_str()),
        Some("\u{1F1FA}\u{1F1F8}")
    );
}

#[test]
fn visual_character_dollar_motion_lands_on_last_grapheme_cluster_start() {
    let mut buffer = TextBuffer::from_text(1, None, "ab\u{1F1FA}\u{1F1F8}".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

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

    assert_eq!(
        pending,
        Some(EditorVimPendingKey::VisualCharacter {
            anchor: 0,
            cursor: 2,
        })
    );

    let delete = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::D,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );

    assert!(delete.handled);
    assert!(delete.changed);
    assert_eq!(buffer.text(), "");
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| register.text.as_str()),
        Some("ab\u{1F1FA}\u{1F1F8}")
    );
}

#[test]
fn normal_mode_visual_d_repeats_with_period() {
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

    let delete = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::D,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(delete.handled);
    assert!(delete.changed);
    assert_eq!(buffer.text(), "aef ghij");
    assert_eq!(mode, EditorVimMode::Normal);

    buffer.set_single_cursor(buffer.line_column_to_char(0, 1));
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
    assert_eq!(buffer.text(), "aghij");
    assert!(pending.is_none());
}
