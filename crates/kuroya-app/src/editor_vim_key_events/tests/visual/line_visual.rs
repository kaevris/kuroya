use super::*;

#[test]
fn visual_line_mode_selects_and_deletes_whole_lines() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\nbeta\ngamma\n".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(1, 2));
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    let mut last_change = None;

    let start = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::V,
        Modifiers::SHIFT,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(start.handled);
    assert!(!start.changed);
    assert_eq!(
        pending,
        Some(EditorVimPendingKey::VisualLine {
            anchor: 8,
            cursor: 8,
            count: None,
        })
    );
    assert_eq!(buffer.selected_text().as_deref(), Some("beta\n"));

    let extend = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::J,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(extend.handled);
    assert_eq!(buffer.selected_text().as_deref(), Some("beta\ngamma\n"));

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

    assert_eq!(buffer.text(), "alpha");
    assert_eq!(mode, EditorVimMode::Normal);
    assert!(pending.is_none());
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some(("beta\ngamma\n", EditorVimRegisterKind::Linewise))
    );
}

#[test]
fn visual_line_mode_yanks_whole_lines_linewise() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\nbeta\ngamma\n".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    let mut last_change = None;

    for (key, modifiers) in [
        (Key::V, Modifiers::SHIFT),
        (Key::J, Modifiers::NONE),
        (Key::Y, Modifiers::NONE),
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
        assert!(!result.changed);
    }

    assert_eq!(buffer.text(), "alpha\nbeta\ngamma\n");
    assert_eq!(buffer.cursor(), 0);
    assert_eq!(mode, EditorVimMode::Normal);
    assert!(pending.is_none());
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some(("alpha\nbeta\n", EditorVimRegisterKind::Linewise))
    );
}

#[test]
fn visual_line_mode_swaps_ends_with_o() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\nbeta\ngamma\n".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    let mut last_change = None;

    for key in [Key::V, Key::J] {
        let result = handle_vim_editor_key_event_with_repeat_state(
            &mut buffer,
            key,
            if key == Key::V {
                Modifiers::SHIFT
            } else {
                Modifiers::NONE
            },
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
            &mut last_change,
        );
        assert!(result.handled);
    }
    assert_eq!(buffer.selected_text().as_deref(), Some("alpha\nbeta\n"));

    let swap = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::O,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(swap.handled);
    assert_eq!(buffer.selected_text().as_deref(), Some("alpha\nbeta\n"));

    let shrink = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::J,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(shrink.handled);
    assert_eq!(buffer.selected_text().as_deref(), Some("beta\n"));

    let delete = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::X,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(delete.handled);
    assert!(delete.changed);
    assert_eq!(buffer.text(), "alpha\ngamma\n");
}

#[test]
fn visual_line_mode_indents_the_selected_lines() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\nbeta\n".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    let mut last_change = None;

    for (key, modifiers) in [
        (Key::V, Modifiers::SHIFT),
        (Key::J, Modifiers::NONE),
        (Key::Period, Modifiers::SHIFT),
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

    assert_eq!(buffer.text(), "    alpha\n    beta\n");
    assert_eq!(mode, EditorVimMode::Normal);
    assert!(pending.is_none());
}

#[test]
fn visual_line_mode_escapes_and_v_switches_to_charwise() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\nbeta\n".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(1, 1));
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;
    let mut last_change = None;

    let start = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::V,
        Modifiers::SHIFT,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(start.handled);

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
    assert!(pending.is_none());
    assert_eq!(mode, EditorVimMode::Normal);
    assert_eq!(buffer.cursor(), 7);

    let start_again = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::V,
        Modifiers::SHIFT,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(start_again.handled);

    let charwise = handle_vim_editor_key_event_with_repeat_state(
        &mut buffer,
        Key::V,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &mut last_change,
    );
    assert!(charwise.handled);
    assert_eq!(
        pending,
        Some(EditorVimPendingKey::VisualCharacter {
            anchor: 7,
            cursor: 7,
        })
    );
}

#[test]
fn visual_line_key_sequences_report_mutations() {
    assert!(!vim_events_include_mutation(
        &[
            key_event(Key::V, Modifiers::SHIFT),
            key_event(Key::J, Modifiers::NONE),
        ],
        EditorVimMode::Normal,
        None,
    ));
    assert!(vim_events_include_mutation(
        &[
            key_event(Key::V, Modifiers::SHIFT),
            key_event(Key::J, Modifiers::NONE),
            key_event(Key::D, Modifiers::NONE),
        ],
        EditorVimMode::Normal,
        None,
    ));
}
