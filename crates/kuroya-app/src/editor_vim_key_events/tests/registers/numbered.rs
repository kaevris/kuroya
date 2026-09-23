use super::*;

const YANK_REGISTER_INDEX: usize = 27;
const SMALL_DELETE_REGISTER_INDEX: usize = 37;

fn register_at(index: usize) -> Option<EditorVimRegister> {
    vim_named_register(EditorVimNamedRegister {
        index,
        append: false,
    })
}

#[test]
fn yank_fills_zero_register_and_delete_does_not() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(1, None, "alpha\nbeta\n".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for (key, modifiers) in [
        (Key::Y, Modifiers::NONE),
        (Key::I, Modifiers::NONE),
        (Key::W, Modifiers::NONE),
        (Key::J, Modifiers::NONE),
        (Key::D, Modifiers::NONE),
        (Key::D, Modifiers::NONE),
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

    assert_eq!(
        register_at(YANK_REGISTER_INDEX)
            .as_ref()
            .map(|register| register.text.as_str()),
        Some("alpha")
    );
    assert_eq!(
        register_at(28)
            .as_ref()
            .map(|register| register.text.as_str()),
        Some("beta\n")
    );
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| register.text.as_str()),
        Some("beta\n")
    );
    assert_eq!(
        unnamed_register.map(|register| register.kind),
        Some(EditorVimRegisterKind::Linewise)
    );
}

#[test]
fn quote_zero_put_pastes_last_yank_after_small_delete() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(2, None, "alpha beta\n".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for (key, modifiers) in [
        (Key::Y, Modifiers::NONE),
        (Key::I, Modifiers::NONE),
        (Key::W, Modifiers::NONE),
        (Key::X, Modifiers::NONE),
        (Key::Quote, Modifiers::SHIFT),
        (Key::Num0, Modifiers::NONE),
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

    assert_eq!(buffer.text(), "lalphapha beta\n");
    assert!(pending.is_none());
}

#[test]
fn successive_line_deletes_shift_the_numbered_register_stack() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(3, None, "one\ntwo\nthree\n".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for (key, modifiers) in [
        (Key::D, Modifiers::NONE),
        (Key::D, Modifiers::NONE),
        (Key::D, Modifiers::NONE),
        (Key::D, Modifiers::NONE),
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

    assert_eq!(buffer.text(), "three\n");
    assert_eq!(
        register_at(28)
            .as_ref()
            .map(|register| register.text.as_str()),
        Some("two\n")
    );
    assert_eq!(
        register_at(29)
            .as_ref()
            .map(|register| register.text.as_str()),
        Some("one\n")
    );
    assert_eq!(register_at(30), None);

    buffer.set_single_cursor(buffer.line_column_to_char(0, 0));
    for (key, modifiers) in [
        (Key::Quote, Modifiers::SHIFT),
        (Key::Num2, Modifiers::NONE),
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
    assert_eq!(buffer.text(), "three\none\n");
    assert_eq!(buffer.cursor_position().line, 1);
    assert_eq!(buffer.cursor_position().column, 0);
}

#[test]
fn small_delete_register_holds_charwise_small_deletes_only() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(4, None, "alpha\nbeta\n".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    handle_vim_editor_key_event_with_state(
        &mut buffer,
        Key::X,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
    );

    assert_eq!(
        register_at(SMALL_DELETE_REGISTER_INDEX)
            .as_ref()
            .map(|register| register.text.as_str()),
        Some("a")
    );

    for (key, modifiers) in [(Key::D, Modifiers::NONE), (Key::D, Modifiers::NONE)] {
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

    assert_eq!(
        register_at(SMALL_DELETE_REGISTER_INDEX)
            .as_ref()
            .map(|register| register.text.as_str()),
        Some("a")
    );
    assert_eq!(
        register_at(28)
            .as_ref()
            .map(|register| register.text.as_str()),
        Some("lpha\n")
    );
}

#[test]
fn quote_minus_put_pastes_the_small_delete_register() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(5, None, "beta\n".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for (key, modifiers) in [
        (Key::X, Modifiers::NONE),
        (Key::Quote, Modifiers::SHIFT),
        (Key::Minus, Modifiers::NONE),
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

    assert_eq!(buffer.text(), "ebta\n");
    assert!(pending.is_none());
}

#[test]
fn multiline_charwise_visual_delete_fills_numbered_register() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(6, None, "one\ntwo\nthree\n".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for (key, modifiers) in [
        (Key::V, Modifiers::NONE),
        (Key::J, Modifiers::NONE),
        (Key::D, Modifiers::NONE),
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

    assert_eq!(buffer.text(), "wo\nthree\n");

    assert_eq!(
        register_at(28)
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some(("one\nt", EditorVimRegisterKind::Characterwise))
    );
}

#[test]
fn unsupported_register_name_cancels_pending_without_eating_next_key() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(7, None, "beta\n".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed_register = None;

    for (key, modifiers) in [
        (Key::Quote, Modifiers::SHIFT),
        (Key::Equals, Modifiers::NONE),
        (Key::X, Modifiers::NONE),
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

    assert_eq!(buffer.text(), "eta\n");
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| register.text.as_str()),
        Some("b")
    );
    assert!(pending.is_none());
}
