use super::super::VimKeyResult;
use super::*;
use kuroya_core::Command;

fn run_ex_command(
    buffer: &mut TextBuffer,
    command: &str,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
) -> VimKeyResult {
    let colon =
        handle_vim_editor_key_event(buffer, Key::Semicolon, Modifiers::SHIFT, mode, pending);
    assert!(colon.handled);

    for ch in command.chars() {
        let (key, modifiers) = printable_key_event_for_char(ch);
        let result = handle_vim_editor_key_event(buffer, key, modifiers, mode, pending);
        assert!(result.handled, "typing {ch} in {command}");
    }

    handle_vim_editor_key_event(buffer, Key::Enter, Modifiers::NONE, mode, pending)
}

fn printable_key_event_for_char(ch: char) -> (Key, Modifiers) {
    match ch {
        '0' => (Key::Num0, Modifiers::NONE),
        '1' => (Key::Num1, Modifiers::NONE),
        '2' => (Key::Num2, Modifiers::NONE),
        '3' => (Key::Num3, Modifiers::NONE),
        '4' => (Key::Num4, Modifiers::NONE),
        '5' => (Key::Num5, Modifiers::NONE),
        '6' => (Key::Num6, Modifiers::NONE),
        '7' => (Key::Num7, Modifiers::NONE),
        '8' => (Key::Num8, Modifiers::NONE),
        '9' => (Key::Num9, Modifiers::NONE),
        '!' => (Key::Num1, Modifiers::SHIFT),
        'a' => (Key::A, Modifiers::NONE),
        'b' => (Key::B, Modifiers::NONE),
        'c' => (Key::C, Modifiers::NONE),
        'd' => (Key::D, Modifiers::NONE),
        'e' => (Key::E, Modifiers::NONE),
        'f' => (Key::F, Modifiers::NONE),
        'g' => (Key::G, Modifiers::NONE),
        'h' => (Key::H, Modifiers::NONE),
        'i' => (Key::I, Modifiers::NONE),
        'j' => (Key::J, Modifiers::NONE),
        'k' => (Key::K, Modifiers::NONE),
        'l' => (Key::L, Modifiers::NONE),
        'm' => (Key::M, Modifiers::NONE),
        'n' => (Key::N, Modifiers::NONE),
        'o' => (Key::O, Modifiers::NONE),
        'p' => (Key::P, Modifiers::NONE),
        'q' => (Key::Q, Modifiers::NONE),
        'r' => (Key::R, Modifiers::NONE),
        's' => (Key::S, Modifiers::NONE),
        't' => (Key::T, Modifiers::NONE),
        'u' => (Key::U, Modifiers::NONE),
        'v' => (Key::V, Modifiers::NONE),
        'w' => (Key::W, Modifiers::NONE),
        'x' => (Key::X, Modifiers::NONE),
        'y' => (Key::Y, Modifiers::NONE),
        'z' => (Key::Z, Modifiers::NONE),
        _ => panic!("test helper does not map {ch}"),
    }
}

#[test]
fn normal_mode_ex_line_number_moves_cursor_to_first_non_blank_of_that_line() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\n  beta\ngamma".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let result = run_ex_command(&mut buffer, "2", &mut mode, &mut pending);

    assert!(result.handled);
    assert!(!result.changed);
    assert_eq!(result.command, None);
    assert_eq!(
        buffer.cursor(),
        buffer.line_column_to_char(1, 2),
        "cursor lands on the first non-blank of line 2"
    );
    assert_eq!(mode, EditorVimMode::Normal);
    assert!(pending.is_none(), "visual (and pending) state is cleared");
}

#[test]
fn normal_mode_ex_line_number_beyond_last_line_clamps_like_vim() {
    let mut buffer = TextBuffer::from_text(2, None, "alpha\ngamma".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let result = run_ex_command(&mut buffer, "99", &mut mode, &mut pending);

    assert!(result.handled);
    assert_eq!(buffer.cursor(), buffer.line_column_to_char(1, 0));
    assert!(pending.is_none());
}

#[test]
fn normal_mode_ex_save_and_quit_spellings_return_the_expected_commands() {
    let cases: [(&str, Command); 9] = [
        ("w", Command::SaveActive),
        ("w!", Command::SaveActive),
        ("wq", Command::CloseActive),
        ("wq!", Command::CloseActive),
        ("x", Command::CloseActive),
        ("q", Command::CloseActive),
        ("q!", Command::CloseActive),
        ("wqa", Command::SaveAll),
        ("wqall", Command::SaveAll),
    ];

    for (spelling, expected) in cases {
        let mut buffer = TextBuffer::from_text(3, None, "body".to_owned());
        let mut mode = EditorVimMode::Normal;
        let mut pending = None;

        let result = run_ex_command(&mut buffer, spelling, &mut mode, &mut pending);

        assert!(result.handled, "spelling {spelling}");
        assert!(!result.changed, "spelling {spelling}");
        assert_eq!(result.command, Some(expected), "spelling {spelling}");
        assert_eq!(buffer.text(), "body", "spelling {spelling}");
        assert!(pending.is_none(), "spelling {spelling}");
    }
}

#[test]
fn normal_mode_ex_registers_lists_recent_yank_in_the_status_bar() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(4, None, "hello world".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    for (key, modifiers) in [
        (Key::Y, Modifiers::NONE),
        (Key::I, Modifiers::NONE),
        (Key::W, Modifiers::NONE),
    ] {
        let result =
            handle_vim_editor_key_event(&mut buffer, key, modifiers, &mut mode, &mut pending);
        assert!(result.handled);
    }

    let result = run_ex_command(&mut buffer, "registers", &mut mode, &mut pending);

    assert!(result.handled);
    assert_eq!(
        vim_pending_command_status_label(pending).as_deref(),
        Some("\"0 hello"),
    );

    let colon = handle_vim_editor_key_event(
        &mut buffer,
        Key::Semicolon,
        Modifiers::SHIFT,
        &mut mode,
        &mut pending,
    );
    assert!(colon.handled);
    assert_eq!(
        vim_pending_command_status_label(pending).as_deref(),
        Some(":")
    );
    let cancel = handle_vim_editor_key_event(
        &mut buffer,
        Key::Escape,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
    );
    assert!(cancel.handled);
}

#[test]
fn normal_mode_ex_display_is_an_alias_for_registers_and_truncates_long_text() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(5, None, "0123456789abcdefghij".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    for (key, modifiers) in [
        (Key::Y, Modifiers::NONE),
        (Key::I, Modifiers::NONE),
        (Key::W, Modifiers::NONE),
    ] {
        let result =
            handle_vim_editor_key_event(&mut buffer, key, modifiers, &mut mode, &mut pending);
        assert!(result.handled);
    }

    let result = run_ex_command(&mut buffer, "display", &mut mode, &mut pending);

    assert!(result.handled);
    assert_eq!(
        vim_pending_command_status_label(pending).as_deref(),
        Some("\"0 0123456789abcdef..."),
    );

    let colon = handle_vim_editor_key_event(
        &mut buffer,
        Key::Semicolon,
        Modifiers::SHIFT,
        &mut mode,
        &mut pending,
    );
    assert!(colon.handled);
    let cancel = handle_vim_editor_key_event(
        &mut buffer,
        Key::Escape,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
    );
    assert!(cancel.handled);
}

#[test]
fn normal_mode_ex_registers_lists_nothing_when_every_register_is_empty() {
    vim_clear_named_registers();
    let mut buffer = TextBuffer::from_text(6, None, "body".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let result = run_ex_command(&mut buffer, "registers", &mut mode, &mut pending);

    assert!(result.handled);
    assert_eq!(vim_pending_command_status_label(pending), None);
}
