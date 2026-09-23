use super::super::VimKeyResult;
use super::*;

fn qa_ex_key_for_char(ch: char) -> (Key, Modifiers) {
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
        '$' => (Key::Num4, Modifiers::SHIFT),
        '#' => (Key::Num3, Modifiers::SHIFT),
        '%' => (Key::Num5, Modifiers::SHIFT),
        '&' => (Key::Num7, Modifiers::SHIFT),
        '~' => (Key::Backtick, Modifiers::SHIFT),
        '+' => (Key::Equals, Modifiers::SHIFT),
        'A'..='Z' => {
            let key = match ch {
                'A' => Key::A,
                'B' => Key::B,
                'C' => Key::C,
                'D' => Key::D,
                'E' => Key::E,
                'F' => Key::F,
                'G' => Key::G,
                'H' => Key::H,
                'I' => Key::I,
                'J' => Key::J,
                'K' => Key::K,
                'L' => Key::L,
                'M' => Key::M,
                'N' => Key::N,
                'O' => Key::O,
                'P' => Key::P,
                'Q' => Key::Q,
                'R' => Key::R,
                'S' => Key::S,
                'T' => Key::T,
                'U' => Key::U,
                'V' => Key::V,
                'W' => Key::W,
                'X' => Key::X,
                'Y' => Key::Y,
                'Z' => Key::Z,
                _ => unreachable!(),
            };
            (key, Modifiers::SHIFT)
        }
        '\\' => (Key::Backslash, Modifiers::NONE),
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
        '`' => (Key::Backtick, Modifiers::NONE),
        '\'' => (Key::Quote, Modifiers::NONE),
        '/' => (Key::Slash, Modifiers::NONE),
        ',' => (Key::Comma, Modifiers::NONE),
        '.' => (Key::Period, Modifiers::NONE),
        ' ' => (Key::Space, Modifiers::NONE),
        _ => panic!("qa_ex helper does not map {ch}"),
    }
}

fn qa_ex_open_colon(
    buffer: &mut TextBuffer,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
) {
    let result =
        handle_vim_editor_key_event(buffer, Key::Semicolon, Modifiers::SHIFT, mode, pending);
    assert!(result.handled);
}

fn qa_ex_type_text(
    buffer: &mut TextBuffer,
    text: &str,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
) {
    for ch in text.chars() {
        let (key, modifiers) = qa_ex_key_for_char(ch);
        let result = handle_vim_editor_key_event(buffer, key, modifiers, mode, pending);
        assert!(result.handled, "typing {ch}");
    }
}

fn qa_ex_enter(
    buffer: &mut TextBuffer,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
) -> VimKeyResult {
    handle_vim_editor_key_event(buffer, Key::Enter, Modifiers::NONE, mode, pending)
}

fn qa_ex_run(
    buffer: &mut TextBuffer,
    command: &str,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
) -> VimKeyResult {
    qa_ex_open_colon(buffer, mode, pending);
    qa_ex_type_text(buffer, command, mode, pending);
    qa_ex_enter(buffer, mode, pending)
}

fn qa_ex_cleanup_status_message(
    buffer: &mut TextBuffer,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
) {
    qa_ex_open_colon(buffer, mode, pending);
    let cancel = handle_vim_editor_key_event(buffer, Key::Escape, Modifiers::NONE, mode, pending);
    assert!(cancel.handled);
    assert!(pending.is_none());
}

#[test]
fn qa_ex_substitute_without_trailing_delimiter_is_accepted_like_vim() {
    let mut buffer = TextBuffer::from_text(200, None, "hello foo world".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let result = qa_ex_run(&mut buffer, "s/foo/bar", &mut mode, &mut pending);

    assert!(result.handled);
    assert!(
        result.changed,
        "Vim replaces with the omitted trailing slash"
    );
    assert_eq!(buffer.text(), "hello bar world");
    assert!(pending.is_none());
}

#[test]
fn qa_ex_substitute_accepts_alternate_hash_delimiter() {
    let mut buffer = TextBuffer::from_text(201, None, "a a\naa".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let result = qa_ex_run(&mut buffer, "%s#a#b#g", &mut mode, &mut pending);

    assert!(result.handled);
    assert!(result.changed);
    assert_eq!(buffer.text(), "b b\nbb");
    assert!(pending.is_none());
}

#[test]
fn qa_ex_substitute_escaped_delimiter_matches_literal_slash_in_query() {
    let mut buffer = TextBuffer::from_text(202, None, "a/b x".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let result = qa_ex_run(&mut buffer, "s/a\\/b/X/", &mut mode, &mut pending);

    assert!(result.handled);
    assert!(result.changed);
    assert_eq!(buffer.text(), "X x");
    assert!(pending.is_none());
}

#[test]
fn qa_ex_substitute_range_beyond_last_line_is_clamped() {
    let mut buffer = TextBuffer::from_text(203, None, "foo\nbar".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let result = qa_ex_run(&mut buffer, "1,99s/foo/x/", &mut mode, &mut pending);

    assert!(result.handled);
    assert!(result.changed);
    assert_eq!(buffer.text(), "x\nbar");
    assert!(pending.is_none());
}

#[test]
fn qa_ex_substitute_single_line_range_touches_only_that_line() {
    let mut buffer = TextBuffer::from_text(204, None, "foo\nfoo\nfoo".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let result = qa_ex_run(&mut buffer, "2s/foo/x/", &mut mode, &mut pending);

    assert!(result.handled);
    assert!(result.changed);
    assert_eq!(buffer.text(), "foo\nx\nfoo");
    assert!(pending.is_none());
}

#[test]
fn qa_ex_substitute_range_without_g_replaces_first_per_line_in_range() {
    let mut buffer = TextBuffer::from_text(205, None, "aa\naa\naa".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let result = qa_ex_run(&mut buffer, "1,2s/a/b/", &mut mode, &mut pending);

    assert!(result.handled);
    assert!(result.changed);
    assert_eq!(buffer.text(), "ba\nba\naa");
    assert!(pending.is_none());
}

#[test]
fn qa_ex_substitute_empty_replacement_deletes_matches() {
    let mut buffer = TextBuffer::from_text(206, None, "foo\nnoon".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let result = qa_ex_run(&mut buffer, "%s/oo//g", &mut mode, &mut pending);

    assert!(result.handled);
    assert!(result.changed);
    assert_eq!(buffer.text(), "f\nnn");
    assert!(pending.is_none());
}

#[test]
fn qa_ex_substitute_overlapping_candidates_match_non_overlapping() {
    let mut buffer = TextBuffer::from_text(207, None, "aaa".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let result = qa_ex_run(&mut buffer, "s/a/b/g", &mut mode, &mut pending);

    assert!(result.handled);
    assert!(result.changed);
    assert_eq!(buffer.text(), "bbb");
    assert!(pending.is_none());
}

#[test]
fn qa_ex_substitute_replacement_ampersand_and_tilde_stay_literal() {
    let mut buffer = TextBuffer::from_text(208, None, "a".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let result = qa_ex_run(&mut buffer, "s/a/&x/", &mut mode, &mut pending);

    assert!(result.handled);
    assert!(result.changed);
    assert_eq!(buffer.text(), "&x");
    assert!(pending.is_none());

    let mut buffer = TextBuffer::from_text(209, None, "a".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let result = qa_ex_run(&mut buffer, "s/a/~z/", &mut mode, &mut pending);

    assert!(result.handled);
    assert!(result.changed);
    assert_eq!(buffer.text(), "~z");
    assert!(pending.is_none());
}

#[test]
fn qa_ex_substitute_repeated_g_flags_are_tolerated() {
    let mut buffer = TextBuffer::from_text(210, None, "a a".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let result = qa_ex_run(&mut buffer, "s/a/b/gg", &mut mode, &mut pending);

    assert!(result.handled);
    assert!(result.changed);
    assert_eq!(buffer.text(), "b b");
    assert!(pending.is_none());
}

#[test]
fn qa_ex_substitute_ignore_case_flag_is_rejected_with_message() {
    let mut buffer = TextBuffer::from_text(211, None, "a a".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let result = qa_ex_run(&mut buffer, "s/a/b/gi", &mut mode, &mut pending);

    assert!(result.handled);
    assert!(!result.changed);
    assert_eq!(buffer.text(), "a a");
    assert_eq!(
        vim_pending_command_status_label(pending).as_deref(),
        Some("Unknown command: s/a/b/gi")
    );
    assert!(pending.is_none());
    qa_ex_cleanup_status_message(&mut buffer, &mut mode, &mut pending);
}

#[test]
fn qa_ex_substitute_empty_query_is_rejected_not_last_search() {
    let mut buffer = TextBuffer::from_text(212, None, "abc".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let result = qa_ex_run(&mut buffer, "s//b/", &mut mode, &mut pending);

    assert!(result.handled);
    assert!(!result.changed);
    assert_eq!(buffer.text(), "abc");
    assert_eq!(
        vim_pending_command_status_label(pending).as_deref(),
        Some("Unknown command: s//b/")
    );
    assert!(pending.is_none());
    qa_ex_cleanup_status_message(&mut buffer, &mut mode, &mut pending);
}

#[test]
fn qa_ex_substitute_backwards_range_reports_and_preserves_buffer() {
    let mut buffer = TextBuffer::from_text(213, None, "a\nb\na".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let result = qa_ex_run(&mut buffer, "3,1s/a/b/", &mut mode, &mut pending);

    assert!(result.handled);
    assert!(!result.changed);
    assert_eq!(buffer.text(), "a\nb\na");
    assert_eq!(
        vim_pending_command_status_label(pending).as_deref(),
        Some("Pattern not found: a")
    );
    assert!(pending.is_none());
    qa_ex_cleanup_status_message(&mut buffer, &mut mode, &mut pending);
}

#[test]
fn qa_ex_substitute_vim_range_shapes_are_rejected_with_message() {
    let spellings = [".,$s/a/b/", ".+1s/a/b/", "s\\a\\b\\"];
    for (index, spelling) in spellings.iter().copied().enumerate() {
        let mut buffer = TextBuffer::from_text(214 + index as u64, None, "ab".to_owned());
        buffer.set_single_cursor(0);
        let mut mode = EditorVimMode::Normal;
        let mut pending = None;

        let result = qa_ex_run(&mut buffer, spelling, &mut mode, &mut pending);

        assert!(result.handled, "spelling {spelling}");
        assert!(!result.changed, "spelling {spelling}");
        assert_eq!(buffer.text(), "ab", "spelling {spelling}");
        assert_eq!(
            vim_pending_command_status_label(pending).as_deref(),
            Some(format!("Unknown command: {spelling}").as_str()),
            "spelling {spelling}"
        );
        assert!(pending.is_none(), "spelling {spelling}");
        qa_ex_cleanup_status_message(&mut buffer, &mut mode, &mut pending);
    }
}

#[test]
fn qa_ex_goto_line_zero_lands_on_first_non_blank_of_first_line() {
    let mut buffer = TextBuffer::from_text(217, None, "  x\ny".to_owned());
    buffer.set_single_cursor(4);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let result = qa_ex_run(&mut buffer, "0", &mut mode, &mut pending);

    assert!(result.handled);
    assert!(!result.changed);
    assert_eq!(buffer.cursor(), buffer.line_column_to_char(0, 2));
    assert!(pending.is_none());
}

#[test]
fn qa_ex_goto_line_is_a_jump_that_refreshes_the_previous_context_mark() {
    let mut buffer = TextBuffer::from_text(218, None, "alpha\n  beta\ngamma".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let result = qa_ex_run(&mut buffer, "3", &mut mode, &mut pending);
    assert!(result.handled);
    assert_eq!(buffer.cursor(), buffer.line_column_to_char(2, 0));

    qa_ex_type_text(&mut buffer, "`'", &mut mode, &mut pending);
    assert_eq!(buffer.cursor(), buffer.line_column_to_char(0, 0));

    qa_ex_type_text(&mut buffer, "`'", &mut mode, &mut pending);
    assert_eq!(buffer.cursor(), buffer.line_column_to_char(2, 0));
    assert!(pending.is_none());
}

#[test]
fn qa_ex_goto_line_number_overflow_reports_unknown_command() {
    let mut buffer = TextBuffer::from_text(219, None, "one\ntwo".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let result = qa_ex_run(
        &mut buffer,
        "99999999999999999999999",
        &mut mode,
        &mut pending,
    );

    assert!(result.handled);
    assert!(!result.changed);
    assert_eq!(buffer.text(), "one\ntwo");
    assert_eq!(
        vim_pending_command_status_label(pending).as_deref(),
        Some("Unknown command: 99999999999999999999999")
    );
    assert!(pending.is_none());
    qa_ex_cleanup_status_message(&mut buffer, &mut mode, &mut pending);
}

#[test]
fn qa_ex_w_family_extended_spellings_are_rejected_with_message() {
    let spellings = ["W", "x!", "xa", "w file"];
    for (index, spelling) in spellings.iter().copied().enumerate() {
        let mut buffer = TextBuffer::from_text(220 + index as u64, None, "body".to_owned());
        buffer.set_single_cursor(0);
        let mut mode = EditorVimMode::Normal;
        let mut pending = None;

        let result = qa_ex_run(&mut buffer, spelling, &mut mode, &mut pending);

        assert!(result.handled, "spelling {spelling}");
        assert!(!result.changed, "spelling {spelling}");
        assert_eq!(result.command, None, "spelling {spelling}");
        assert_eq!(buffer.text(), "body", "spelling {spelling}");
        assert_eq!(
            vim_pending_command_status_label(pending).as_deref(),
            Some(format!("Unknown command: {spelling}").as_str()),
            "spelling {spelling}"
        );
        assert!(pending.is_none(), "spelling {spelling}");
        qa_ex_cleanup_status_message(&mut buffer, &mut mode, &mut pending);
    }
}

#[test]
fn qa_ex_backspace_edits_then_resumes_the_command() {
    let mut buffer = TextBuffer::from_text(224, None, "ab".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    qa_ex_open_colon(&mut buffer, &mut mode, &mut pending);
    qa_ex_type_text(&mut buffer, "s/ab/c/", &mut mode, &mut pending);
    assert_eq!(
        vim_pending_command_status_label(pending).as_deref(),
        Some(":s/ab/c/")
    );

    handle_vim_editor_key_event(
        &mut buffer,
        Key::Backspace,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
    );
    handle_vim_editor_key_event(
        &mut buffer,
        Key::Backspace,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
    );
    assert_eq!(
        vim_pending_command_status_label(pending).as_deref(),
        Some(":s/ab/")
    );

    qa_ex_type_text(&mut buffer, "d/", &mut mode, &mut pending);
    assert_eq!(
        vim_pending_command_status_label(pending).as_deref(),
        Some(":s/ab/d/")
    );

    let result = qa_ex_enter(&mut buffer, &mut mode, &mut pending);
    assert!(result.handled);
    assert!(result.changed, "resumed command executes as replace with d");
    assert_eq!(buffer.text(), "d");
    assert!(pending.is_none());
}

#[test]
fn qa_ex_ctrl_c_and_ctrl_open_bracket_cancel_without_residue() {
    let mut buffer = TextBuffer::from_text(225, None, "foo".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    qa_ex_open_colon(&mut buffer, &mut mode, &mut pending);
    qa_ex_type_text(&mut buffer, "s/a", &mut mode, &mut pending);
    let cancel = handle_vim_editor_key_event(
        &mut buffer,
        Key::C,
        Modifiers::CTRL,
        &mut mode,
        &mut pending,
    );
    assert!(cancel.handled);
    assert!(pending.is_none());
    assert_eq!(buffer.text(), "foo");

    qa_ex_open_colon(&mut buffer, &mut mode, &mut pending);
    assert_eq!(
        vim_pending_command_status_label(pending).as_deref(),
        Some(":")
    );

    let cancel = handle_vim_editor_key_event(
        &mut buffer,
        Key::OpenBracket,
        Modifiers::CTRL,
        &mut mode,
        &mut pending,
    );
    assert!(cancel.handled);
    assert!(pending.is_none());

    qa_ex_open_colon(&mut buffer, &mut mode, &mut pending);
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
fn qa_ex_colon_while_count_pending_discards_count_and_opens_input() {
    let mut buffer = TextBuffer::from_text(226, None, "one\n two\nthree".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let three = handle_vim_editor_key_event(
        &mut buffer,
        Key::Num3,
        Modifiers::NONE,
        &mut mode,
        &mut pending,
    );
    assert!(three.handled);
    assert_eq!(
        vim_pending_command_status_label(pending).as_deref(),
        Some("3")
    );

    qa_ex_open_colon(&mut buffer, &mut mode, &mut pending);
    assert_eq!(
        vim_pending_command_status_label(pending).as_deref(),
        Some(":")
    );

    qa_ex_type_text(&mut buffer, "2", &mut mode, &mut pending);
    let result = qa_ex_enter(&mut buffer, &mut mode, &mut pending);
    assert!(result.handled);
    assert_eq!(buffer.cursor(), buffer.line_column_to_char(1, 1));
    assert!(pending.is_none());
    assert_eq!(mode, EditorVimMode::Normal);
}

#[test]
fn qa_ex_ctrl_m_accepts_the_command_like_enter() {
    let mut buffer = TextBuffer::from_text(227, None, "one\ntwo".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    qa_ex_open_colon(&mut buffer, &mut mode, &mut pending);
    qa_ex_type_text(&mut buffer, "2", &mut mode, &mut pending);

    let result = handle_vim_editor_key_event(
        &mut buffer,
        Key::M,
        Modifiers::CTRL,
        &mut mode,
        &mut pending,
    );

    assert!(result.handled);
    assert_eq!(buffer.cursor(), buffer.line_column_to_char(1, 0));
    assert!(pending.is_none());
}

#[test]
fn qa_ex_status_label_tracks_goto_typing_step_by_step() {
    let mut buffer = TextBuffer::from_text(228, None, "a\nb\nc".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    qa_ex_open_colon(&mut buffer, &mut mode, &mut pending);
    assert_eq!(
        vim_pending_command_status_label(pending).as_deref(),
        Some(":")
    );

    for (typed, expected) in [("1", ":1"), (",", ":1,"), ("3", ":1,3")] {
        qa_ex_type_text(&mut buffer, typed, &mut mode, &mut pending);
        assert_eq!(
            vim_pending_command_status_label(pending).as_deref(),
            Some(expected),
            "after typing {typed}"
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
fn qa_ex_successful_substitute_clears_stale_error_message() {
    let mut buffer = TextBuffer::from_text(229, None, "a".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let bad = qa_ex_run(&mut buffer, "zz", &mut mode, &mut pending);
    assert!(bad.handled);
    assert_eq!(
        vim_pending_command_status_label(pending).as_deref(),
        Some("Unknown command: zz")
    );

    let result = qa_ex_run(&mut buffer, "s/a/b/", &mut mode, &mut pending);
    assert!(result.handled);
    assert!(result.changed);
    assert_eq!(buffer.text(), "b");
    assert!(pending.is_none());
    assert_eq!(
        vim_pending_command_status_label(pending),
        None,
        "success must clear the previous Unknown command feedback"
    );
}

#[test]
fn qa_ex_empty_command_enter_reports_unknown_empty_command() {
    let mut buffer = TextBuffer::from_text(230, None, "keep".to_owned());
    buffer.set_single_cursor(0);
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;

    let result = qa_ex_run(&mut buffer, "", &mut mode, &mut pending);

    assert!(result.handled);
    assert!(!result.changed);
    assert_eq!(buffer.text(), "keep");
    assert_eq!(
        vim_pending_command_status_label(pending).as_deref(),
        Some("Unknown command: ")
    );
    assert!(pending.is_none());
    qa_ex_cleanup_status_message(&mut buffer, &mut mode, &mut pending);
}
