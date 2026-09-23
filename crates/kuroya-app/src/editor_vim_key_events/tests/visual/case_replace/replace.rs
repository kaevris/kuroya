use super::*;

#[test]
fn normal_mode_visual_character_r_replaces_selection_with_printable_char() {
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
    let replace = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::R,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );

    assert!(replace.handled);
    assert!(!replace.changed);
    assert_eq!(replace.suppress_text, Some('r'));
    assert_eq!(
        pending,
        Some(EditorVimPendingKey::VisualCharacterReplace {
            anchor: 1,
            cursor: 3,
        })
    );
    assert_eq!(buffer.selected_text().as_deref(), Some("bcd"));

    let target = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::X,
        Modifiers::SHIFT,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );

    assert!(target.handled);
    assert!(target.changed);
    assert_eq!(target.suppress_text, Some('X'));
    assert_eq!(buffer.text(), "aXXXef ghij");
    assert_eq!(buffer.cursor(), 1);
    assert_eq!(mode, EditorVimMode::Normal);
    assert!(pending.is_none());
    assert!(!buffer.has_selection());
    assert!(unnamed_register.is_none());

    buffer.set_single_cursor(buffer.line_column_to_char(0, 7));
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
    assert_eq!(buffer.text(), "aXXXef XXXj");
    assert_eq!(buffer.cursor(), buffer.line_column_to_char(0, 9));
    assert!(pending.is_none());
    assert!(vim_events_include_mutation(
        &[
            key_event(Key::V, Modifiers::NONE),
            key_event(Key::L, Modifiers::NONE),
            key_event(Key::R, Modifiers::NONE),
            key_event(Key::X, Modifiers::SHIFT),
        ],
        EditorVimMode::Normal,
        None,
    ));
    assert!(!vim_events_include_mutation(
        &[
            key_event(Key::V, Modifiers::NONE),
            key_event(Key::L, Modifiers::NONE),
            key_event(Key::R, Modifiers::NONE),
        ],
        EditorVimMode::Normal,
        None,
    ));
}

#[test]
fn normal_mode_visual_r_keeps_newlines_in_multiline_selections() {
    let mut buffer = TextBuffer::from_text(1, None, "foo\nbar".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    let mut last_change = None;

    for (key, modifiers) in [
        (Key::V, Modifiers::NONE),
        (Key::J, Modifiers::NONE),
        (Key::Num4, Modifiers::SHIFT),
    ] {
        let result = handle_vim_editor_key_event_with_repeat_state(
            &mut buffer,
            key,
            modifiers,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
            &mut last_change,
        );
        assert!(result.handled);
    }
    assert_eq!(buffer.selected_text().as_deref(), Some("foo\nbar"));

    for (key, modifiers) in [(Key::R, Modifiers::NONE), (Key::X, Modifiers::SHIFT)] {
        let result = handle_vim_editor_key_event_with_repeat_state(
            &mut buffer,
            key,
            modifiers,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
            &mut last_change,
        );
        assert!(result.handled);
    }

    assert_eq!(buffer.text(), "XXX\nXXX");
    assert_eq!(buffer.cursor(), 0);
    assert!(pending.is_none());
}

#[test]
fn normal_mode_visual_r_replaces_flag_cluster_without_dangling_regional_indicator() {
    let mut buffer = TextBuffer::from_text(1, None, "a\u{1F1FA}\u{1F1F8}b".to_owned());
    buffer.set_single_cursor(1);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    let mut last_change = None;

    for (key, modifiers) in [(Key::V, Modifiers::NONE), (Key::R, Modifiers::NONE)] {
        let result = handle_vim_editor_key_event_with_repeat_state(
            &mut buffer,
            key,
            modifiers,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
            &mut last_change,
        );
        assert!(result.handled);
    }

    let replace = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::X,
        Modifiers::SHIFT,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );

    assert!(replace.handled);
    assert!(replace.changed);

    assert_eq!(buffer.text(), "aXb");
    assert_eq!(buffer.len_chars(), 3);
    assert!(pending.is_none());
}

#[test]
fn normal_mode_visual_r_replaces_zwj_family_cluster_without_splitting_it() {
    let mut buffer = TextBuffer::from_text(1, None, "a\u{1F468}\u{200D}\u{1F469}b".to_owned());
    buffer.set_single_cursor(1);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    let mut last_change = None;

    for (key, modifiers) in [(Key::V, Modifiers::NONE), (Key::R, Modifiers::NONE)] {
        let result = handle_vim_editor_key_event_with_repeat_state(
            &mut buffer,
            key,
            modifiers,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
            &mut last_change,
        );
        assert!(result.handled);
    }

    let replace = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::X,
        Modifiers::SHIFT,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );

    assert!(replace.changed);
    assert_eq!(buffer.text(), "aXb");
    assert_eq!(buffer.len_chars(), 3);
}

#[test]
fn normal_mode_visual_delete_over_flag_cluster_leaves_no_dangling_regional_indicator() {
    let mut buffer = TextBuffer::from_text(1, None, "a\u{1F1FA}\u{1F1F8}b".to_owned());
    buffer.set_single_cursor(1);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for key in [Key::V, Key::D] {
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

    assert_eq!(buffer.text(), "ab");
    assert_eq!(buffer.len_chars(), 2);
    assert!(pending.is_none());
}
