use super::*;

fn put_over_selection(
    initial_text: &str,
    initial_register: Option<EditorVimRegister>,
) -> (String, usize, Option<EditorVimRegister>) {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(1, None, initial_text.to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = initial_register;

    for (key, modifiers) in [
        (Key::V, Modifiers::NONE),
        (Key::I, Modifiers::NONE),
        (Key::W, Modifiers::NONE),
        (Key::P, Modifiers::NONE),
    ] {
        handle_vim_editor_key_event_with_state(
            &mut buffer,
            key,
            modifiers,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
        );
    }

    (buffer.text(), buffer.cursor(), unnamed_register)
}

#[test]
fn visual_character_put_replaces_word_and_records_replaced_text() {
    let (text, cursor, unnamed_register) = put_over_selection(
        "one two three\n",
        Some(EditorVimRegister {
            text: "ZZ".to_owned(),
            kind: EditorVimRegisterKind::Characterwise,
        }),
    );

    assert_eq!(text, "ZZ two three\n");

    assert_eq!(cursor, 0);

    assert_eq!(
        unnamed_register.map(|register| (register.text, register.kind)),
        Some(("one".to_owned(), EditorVimRegisterKind::Characterwise))
    );
}

#[test]
fn visual_put_with_linewise_register_inserts_whole_lines_at_selection() {
    let (text, cursor, unnamed_register) = put_over_selection(
        "one two\n",
        Some(EditorVimRegister {
            text: "L1\nL2\n".to_owned(),
            kind: EditorVimRegisterKind::Linewise,
        }),
    );

    assert_eq!(text, "L1\nL2\n two\n");
    assert_eq!(cursor, 0);
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| register.text.as_str()),
        Some("one")
    );
}

#[test]
fn visual_put_without_register_content_is_a_noop() {
    let (text, _, unnamed_register) = put_over_selection("one two three\n", None);

    assert_eq!(text, "one two three\n");
    assert_eq!(unnamed_register, None);
}

#[test]
fn visual_line_put_replaces_selected_lines_and_records_them() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(2, None, "one\ntwo\nthree\n".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(1, 0));
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = Some(EditorVimRegister {
        text: "XX\n".to_owned(),
        kind: EditorVimRegisterKind::Linewise,
    });

    for (key, modifiers) in [(Key::V, Modifiers::SHIFT), (Key::P, Modifiers::NONE)] {
        handle_vim_editor_key_event_with_state(
            &mut buffer,
            key,
            modifiers,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed_register,
        );
    }

    assert_eq!(buffer.text(), "one\nXX\nthree\n");
    assert_eq!(buffer.cursor_position().line, 1);
    assert_eq!(buffer.cursor_position().column, 0);

    assert_eq!(
        unnamed_register.map(|register| (register.text, register.kind)),
        Some(("two\n".to_owned(), EditorVimRegisterKind::Linewise))
    );
}
