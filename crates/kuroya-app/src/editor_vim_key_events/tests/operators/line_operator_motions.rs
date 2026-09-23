use super::*;

#[test]
fn normal_mode_dj_deletes_the_cursor_line_and_the_one_below() {
    let mut buffer = TextBuffer::from_text(1, None, "one\ntwo\nthree".to_owned());
    buffer.set_single_cursor(2);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for (index, key) in [Key::D, Key::J].into_iter().enumerate() {
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
        assert_eq!(result.changed, index == 1, "only `j` executes the delete");
    }

    assert_eq!(buffer.text(), "three");
    assert_eq!(buffer.cursor(), 0);
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some(("one\ntwo\n", EditorVimRegisterKind::Linewise))
    );
    assert!(pending.is_none());
}

#[test]
fn normal_mode_dk_deletes_the_line_above_the_cursor_line() {
    let mut buffer = TextBuffer::from_text(1, None, "one\ntwo\nthree".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(1, 1));
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for key in [Key::D, Key::K] {
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

    assert_eq!(buffer.text(), "three");
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some(("one\ntwo\n", EditorVimRegisterKind::Linewise))
    );
    assert!(pending.is_none());
}

#[test]
fn normal_mode_dg_deletes_to_the_end_of_the_file() {
    let mut buffer = TextBuffer::from_text(1, None, "one\ntwo\nthree\n".to_owned());
    buffer.set_single_cursor(2);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for (key, modifiers) in [(Key::D, Modifiers::NONE), (Key::G, Modifiers::SHIFT)] {
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

    assert_eq!(buffer.text(), "");
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some(("one\ntwo\nthree\n", EditorVimRegisterKind::Linewise))
    );
    assert!(pending.is_none());
}

#[test]
fn normal_mode_dgg_deletes_to_the_beginning_of_the_file() {
    let mut buffer = TextBuffer::from_text(1, None, "one\ntwo\nthree\n".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(2, 1));
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for key in [Key::D, Key::G, Key::G] {
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

    assert_eq!(buffer.text(), "");
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some(("one\ntwo\nthree\n", EditorVimRegisterKind::Linewise))
    );
    assert!(pending.is_none());
}

#[test]
fn normal_mode_yg_yanks_to_the_end_of_the_file_linewise() {
    let mut buffer = TextBuffer::from_text(1, None, "one\ntwo\nthree".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for (key, modifiers) in [(Key::Y, Modifiers::NONE), (Key::G, Modifiers::SHIFT)] {
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

    assert_eq!(buffer.text(), "one\ntwo\nthree");
    assert_eq!(buffer.cursor(), 0);
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some(("one\ntwo\nthree\n", EditorVimRegisterKind::Linewise))
    );
    assert!(pending.is_none());
}

#[test]
fn normal_mode_2dj_deletes_seven_lines_linewise() {
    let mut buffer = TextBuffer::from_text(
        1,
        None,
        "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight".to_owned(),
    );
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for key in [Key::Num2, Key::D, Key::Num3, Key::J] {
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

    assert_eq!(buffer.text(), "eight");
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| register.text.as_str()),
        Some("one\ntwo\nthree\nfour\nfive\nsix\nseven\n")
    );
    assert!(pending.is_none());
}
