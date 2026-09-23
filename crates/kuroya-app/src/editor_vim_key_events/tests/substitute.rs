use super::*;

#[test]
fn normal_mode_percent_substitute_replaces_all_literal_matches() {
    let mut buffer = TextBuffer::from_text(1, None, "foo foo\nkeep foo".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    for (index, (key, modifiers, suppressed)) in [
        (Key::Semicolon, Modifiers::SHIFT, Some(':')),
        (Key::Num5, Modifiers::SHIFT, Some('%')),
        (Key::S, Modifiers::NONE, Some('s')),
        (Key::Slash, Modifiers::NONE, Some('/')),
        (Key::F, Modifiers::NONE, Some('f')),
        (Key::O, Modifiers::NONE, Some('o')),
        (Key::O, Modifiers::NONE, Some('o')),
        (Key::Slash, Modifiers::NONE, Some('/')),
        (Key::B, Modifiers::NONE, Some('b')),
        (Key::A, Modifiers::NONE, Some('a')),
        (Key::R, Modifiers::NONE, Some('r')),
        (Key::Slash, Modifiers::NONE, Some('/')),
        (Key::G, Modifiers::NONE, Some('g')),
        (Key::Enter, Modifiers::NONE, None),
    ]
    .into_iter()
    .enumerate()
    {
        let result =
            handle_vim_editor_key_event(&mut buffer, key, modifiers, &mut mode, &mut pending);
        assert!(result.handled, "step {index}");
        assert_eq!(result.suppress_text, suppressed, "step {index}");
        assert_eq!(result.changed, index == 13, "step {index}");
    }

    assert_eq!(buffer.text(), "bar bar\nkeep bar");
    assert_eq!(mode, EditorVimMode::Normal);
    assert!(pending.is_none());
}

#[test]
fn normal_mode_percent_substitute_shows_pending_command_status() {
    let mut buffer = TextBuffer::from_text(2, None, "foo".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    for (key, modifiers, status) in [
        (Key::Semicolon, Modifiers::SHIFT, ":"),
        (Key::Num5, Modifiers::SHIFT, ":%"),
        (Key::S, Modifiers::NONE, ":%s"),
        (Key::Slash, Modifiers::NONE, ":%s/"),
        (Key::F, Modifiers::NONE, ":%s/f"),
    ] {
        let result =
            handle_vim_editor_key_event(&mut buffer, key, modifiers, &mut mode, &mut pending);
        assert!(result.handled);
        assert!(!result.changed);
        assert_eq!(
            vim_pending_command_status_label(pending).as_deref(),
            Some(status)
        );
    }

    let cancel = handle_vim_editor_key_event(
        &mut buffer,
        Key::Escape,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
    );

    assert!(cancel.handled);
    assert!(pending.is_none());
}

#[test]
fn normal_mode_percent_substitute_counts_as_possible_mutation() {
    assert!(vim_events_include_mutation(
        &[
            key_event(Key::Semicolon, Modifiers::SHIFT),
            key_event(Key::Num5, Modifiers::SHIFT),
            key_event(Key::S, Modifiers::NONE),
            key_event(Key::Slash, Modifiers::NONE),
            key_event(Key::F, Modifiers::NONE),
            key_event(Key::O, Modifiers::NONE),
            key_event(Key::O, Modifiers::NONE),
            key_event(Key::Slash, Modifiers::NONE),
            key_event(Key::B, Modifiers::NONE),
            key_event(Key::A, Modifiers::NONE),
            key_event(Key::R, Modifiers::NONE),
            key_event(Key::Slash, Modifiers::NONE),
            key_event(Key::G, Modifiers::NONE),
            key_event(Key::Enter, Modifiers::NONE),
        ],
        EditorVimMode::Normal,
        None,
    ));
}

fn type_command(
    buffer: &mut TextBuffer,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    command: &[(Key, Modifiers, Option<char>)],
) {
    for (index, (key, modifiers, suppressed)) in command.iter().copied().enumerate() {
        let result = handle_vim_editor_key_event(buffer, key, modifiers, mode, pending);
        assert!(result.handled, "step {index}");
        assert_eq!(result.suppress_text, suppressed, "step {index}");
    }
}

#[test]
fn normal_mode_bare_substitute_replaces_first_occurrence_on_current_line() {
    let mut buffer = TextBuffer::from_text(1, None, "foo foo\nkeep foo".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let command = [
        (Key::Semicolon, Modifiers::SHIFT, Some(':')),
        (Key::S, Modifiers::NONE, Some('s')),
        (Key::Slash, Modifiers::NONE, Some('/')),
        (Key::F, Modifiers::NONE, Some('f')),
        (Key::O, Modifiers::NONE, Some('o')),
        (Key::O, Modifiers::NONE, Some('o')),
        (Key::Slash, Modifiers::NONE, Some('/')),
        (Key::B, Modifiers::NONE, Some('b')),
        (Key::A, Modifiers::NONE, Some('a')),
        (Key::R, Modifiers::NONE, Some('r')),
        (Key::Slash, Modifiers::NONE, Some('/')),
        (Key::Enter, Modifiers::NONE, None),
    ];
    type_command(&mut buffer, &mut mode, &mut pending, &command);

    assert_eq!(buffer.text(), "bar foo\nkeep foo");
    assert_eq!(mode, EditorVimMode::Normal);
    assert!(pending.is_none());
}

#[test]
fn normal_mode_current_line_substitute_with_g_replaces_every_occurrence() {
    let mut buffer = TextBuffer::from_text(1, None, "foo foo\nkeep foo".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let command = [
        (Key::Semicolon, Modifiers::SHIFT, Some(':')),
        (Key::S, Modifiers::NONE, Some('s')),
        (Key::Slash, Modifiers::NONE, Some('/')),
        (Key::F, Modifiers::NONE, Some('f')),
        (Key::O, Modifiers::NONE, Some('o')),
        (Key::O, Modifiers::NONE, Some('o')),
        (Key::Slash, Modifiers::NONE, Some('/')),
        (Key::B, Modifiers::NONE, Some('b')),
        (Key::A, Modifiers::NONE, Some('a')),
        (Key::R, Modifiers::NONE, Some('r')),
        (Key::Slash, Modifiers::NONE, Some('/')),
        (Key::G, Modifiers::NONE, Some('g')),
        (Key::Enter, Modifiers::NONE, None),
    ];
    type_command(&mut buffer, &mut mode, &mut pending, &command);

    assert_eq!(buffer.text(), "bar bar\nkeep foo");
    assert!(pending.is_none());
}

#[test]
fn normal_mode_line_ranged_substitute_covers_only_that_range() {
    let mut buffer = TextBuffer::from_text(1, None, "foo\nfoo\nfoo".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let command = [
        (Key::Semicolon, Modifiers::SHIFT, Some(':')),
        (Key::Num2, Modifiers::NONE, Some('2')),
        (Key::Comma, Modifiers::NONE, Some(',')),
        (Key::Num3, Modifiers::NONE, Some('3')),
        (Key::S, Modifiers::NONE, Some('s')),
        (Key::Slash, Modifiers::NONE, Some('/')),
        (Key::F, Modifiers::NONE, Some('f')),
        (Key::O, Modifiers::NONE, Some('o')),
        (Key::O, Modifiers::NONE, Some('o')),
        (Key::Slash, Modifiers::NONE, Some('/')),
        (Key::B, Modifiers::NONE, Some('b')),
        (Key::A, Modifiers::NONE, Some('a')),
        (Key::R, Modifiers::NONE, Some('r')),
        (Key::Slash, Modifiers::NONE, Some('/')),
        (Key::Enter, Modifiers::NONE, None),
    ];
    type_command(&mut buffer, &mut mode, &mut pending, &command);

    assert_eq!(buffer.text(), "foo\nbar\nbar");
    assert!(pending.is_none());
}

#[test]
fn normal_mode_unknown_ex_command_reports_to_the_status_bar() {
    let mut buffer = TextBuffer::from_text(1, None, "foo".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let command = [
        (Key::Semicolon, Modifiers::SHIFT, Some(':')),
        (Key::Z, Modifiers::NONE, Some('z')),
        (Key::Enter, Modifiers::NONE, None),
    ];
    type_command(&mut buffer, &mut mode, &mut pending, &command);

    assert_eq!(buffer.text(), "foo");
    assert!(pending.is_none());
    assert_eq!(
        vim_pending_command_status_label(pending).as_deref(),
        Some("Unknown command: z"),
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
    assert!(pending.is_none());
}

#[test]
fn normal_mode_substitute_without_a_match_reports_pattern_not_found() {
    let mut buffer = TextBuffer::from_text(1, None, "foo".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let command = [
        (Key::Semicolon, Modifiers::SHIFT, Some(':')),
        (Key::S, Modifiers::NONE, Some('s')),
        (Key::Slash, Modifiers::NONE, Some('/')),
        (Key::X, Modifiers::NONE, Some('x')),
        (Key::Slash, Modifiers::NONE, Some('/')),
        (Key::Y, Modifiers::NONE, Some('y')),
        (Key::Slash, Modifiers::NONE, Some('/')),
        (Key::Enter, Modifiers::NONE, None),
    ];
    type_command(&mut buffer, &mut mode, &mut pending, &command);

    assert_eq!(buffer.text(), "foo");
    assert_eq!(
        vim_pending_command_status_label(pending).as_deref(),
        Some("Pattern not found: x"),
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
