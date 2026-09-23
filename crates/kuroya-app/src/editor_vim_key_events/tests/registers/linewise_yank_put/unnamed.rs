use super::*;

#[test]
fn normal_mode_yank_lines_and_put_use_linewise_register() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\nbeta\ngamma\ndelta".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(1, 0));
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for key in [Key::Num2, Key::Y, Key::Y] {
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

    assert_eq!(buffer.text(), "alpha\nbeta\ngamma\ndelta");
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| register.text.as_str()),
        Some("beta\ngamma\n")
    );
    assert!(pending.is_none());

    buffer.set_single_cursor(buffer.line_column_to_char(3, 0));
    let put_after = handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::P,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );

    assert!(put_after.handled);
    assert!(put_after.changed);
    assert_eq!(put_after.suppress_text, Some('p'));
    assert_eq!(buffer.text(), "alpha\nbeta\ngamma\ndelta\nbeta\ngamma\n");

    assert_eq!(buffer.cursor_position().line, 4);
    assert_eq!(buffer.cursor_position().column, 0);

    buffer.set_single_cursor(buffer.line_column_to_char(0, 0));
    for (key, modifiers) in [(Key::Num2, Modifiers::NONE), (Key::P, Modifiers::SHIFT)] {
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

    assert_eq!(
        buffer.text(),
        "beta\ngamma\nbeta\ngamma\nalpha\nbeta\ngamma\ndelta\nbeta\ngamma\n"
    );
    assert_eq!(buffer.cursor(), 0);
    assert!(pending.is_none());
    assert!(!vim_events_include_mutation(
        &[
            Event::Key {
                key: Key::Y,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            Event::Key {
                key: Key::Y,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
        ],
        EditorVimMode::Normal,
        None,
    ));
    assert!(vim_events_include_mutation(
        &[Event::Key {
            key: Key::P,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        EditorVimMode::Normal,
        None,
    ));
}
