use super::*;

struct Qa {
    buffer: TextBuffer,
    mode: EditorVimMode,
    pending: Option<EditorVimPendingKey>,
    last_char_find: Option<EditorVimCharFind>,
    unnamed_register: Option<EditorVimRegister>,
    last_change: Option<crate::editor_vim_key_events::EditorVimLastChange>,
}

impl Qa {
    fn at(text: &str, cursor: usize) -> Self {
        let mut buffer = TextBuffer::from_text(1, None, text.to_owned());
        buffer.set_single_cursor(cursor);
        Self {
            buffer,
            mode: EditorVimMode::Normal,
            pending: None,
            last_char_find: None,
            unnamed_register: None,
            last_change: None,
        }
    }

    fn press(&mut self, key: Key, modifiers: Modifiers) {
        let result = handle_vim_editor_key_event_with_repeat_state(
            &mut self.buffer,
            key,
            modifiers,
            &mut self.mode,
            &mut self.pending,
            &mut self.last_char_find,
            &mut self.unnamed_register,
            &mut self.last_change,
        );
        assert!(result.handled, "expected {key:?} to be handled");
    }

    fn press_keys(&mut self, keys: &[Key]) {
        for key in keys {
            self.press(*key, Modifiers::NONE);
        }
    }

    fn type_text(&mut self, text: &str) {
        self.buffer.insert_at_cursors(text);
        vim_record_inserted_text(&mut self.last_change, text);
    }

    fn cursor(&self) -> usize {
        self.buffer.cursor()
    }

    fn entered(mut self, events: &[(Key, Modifiers)]) -> Self {
        for (key, modifiers) in events {
            self.press(*key, *modifiers);
        }
        assert_eq!(self.mode, EditorVimMode::Insert);
        self
    }

    fn host_backspace(&mut self) {
        let result = handle_vim_editor_key_event_with_repeat_state(
            &mut self.buffer,
            Key::Backspace,
            Modifiers::NONE,
            &mut self.mode,
            &mut self.pending,
            &mut self.last_char_find,
            &mut self.unnamed_register,
            &mut self.last_change,
        );
        assert!(
            !result.handled,
            "plain Backspace must stay host-driven in insert mode"
        );
        assert!(
            self.buffer.delete_backward_with_auto_pair_delete(false),
            "host backspace deleted nothing"
        );
        vim_record_insert_replay_key_with_auto_indent(
            &mut self.last_change,
            Key::Backspace,
            Modifiers::NONE,
            true,
        );
    }
}

fn exit_left_deviation(
    label: &str,
    mut qa: Qa,
    exit: (Key, Modifiers),
    expected: usize,
) -> Option<String> {
    qa.type_text("Z");
    qa.press(exit.0, exit.1);
    if qa.mode != EditorVimMode::Normal {
        return Some(format!("{label}: exit key did not leave insert mode"));
    }
    if qa.cursor() != expected {
        return Some(format!(
            "{label}: cursor at {} after exit, Vim lands it on the last typed char at {expected} \
             (`:h insert.txt`: leaving insert moves the cursor one column left)",
            qa.cursor()
        ));
    }
    None
}

#[test]
fn qa_i_entry_keeps_cursor_and_inserts_before_it() {
    let mut qa = Qa::at("hello", 2);
    qa.press(Key::I, Modifiers::NONE);
    assert_eq!(qa.mode, EditorVimMode::Insert);
    assert_eq!(qa.cursor(), 2, "`i` must not move the cursor");

    qa.type_text("XY");
    assert_eq!(qa.buffer.text(), "heXYllo");
    assert_eq!(qa.cursor(), 4);
}

#[test]
fn qa_a_entry_sits_after_cursor_char() {
    let mut qa = Qa::at("hello", 0);
    qa.press(Key::A, Modifiers::NONE);
    assert_eq!(qa.mode, EditorVimMode::Insert);
    assert_eq!(qa.cursor(), 1, "`a` inserts after the cursor char");

    qa.type_text("X");
    assert_eq!(qa.buffer.text(), "hXello");
}

#[test]
fn qa_a_on_empty_line_behaves_like_i() {
    let mut qa = Qa::at("", 0);
    qa.press(Key::A, Modifiers::NONE);
    assert_eq!(qa.mode, EditorVimMode::Insert);
    assert_eq!(qa.cursor(), 0, "`a` on an empty line cannot move right");

    qa.type_text("x");
    assert_eq!(qa.buffer.text(), "x");
}

#[test]
fn qa_a_at_end_of_line_appends_at_eol() {
    let mut qa = Qa::at("hi", 1);
    qa.press(Key::A, Modifiers::NONE);
    assert_eq!(
        qa.cursor(),
        2,
        "`a` on the last char appends at end of line"
    );

    qa.type_text("!");
    assert_eq!(qa.buffer.text(), "hi!");
}

#[test]
fn qa_shift_i_lands_on_first_non_blank() {
    let mut qa = Qa::at("    indented", 0);
    qa.press(Key::I, Modifiers::SHIFT);
    assert_eq!(qa.mode, EditorVimMode::Insert);
    assert_eq!(qa.cursor(), 4, "`I` inserts before the first non-blank");

    qa.type_text("X");
    assert_eq!(qa.buffer.text(), "    Xindented");
}

#[test]
fn qa_shift_a_lands_on_line_end() {
    let mut qa = Qa::at("  tail", 0);
    qa.press(Key::A, Modifiers::SHIFT);
    assert_eq!(qa.mode, EditorVimMode::Insert);
    assert_eq!(
        qa.cursor(),
        6,
        "`A` inserts after the last char of the line"
    );

    qa.type_text("!");
    assert_eq!(qa.buffer.text(), "  tail!");
}

#[test]
fn qa_o_copies_indent_and_lands_after_it_on_new_line() {
    let mut qa = Qa::at("    foo", 0);
    qa.press(Key::O, Modifiers::NONE);
    assert_eq!(qa.mode, EditorVimMode::Insert);
    assert_eq!(qa.buffer.text(), "    foo\n    ", "`o` copies the indent");
    assert_eq!(
        qa.cursor(),
        12,
        "cursor sits after the copied indent, where typing starts"
    );

    qa.type_text("bar");
    assert_eq!(qa.buffer.text(), "    foo\n    bar");
}

#[test]
fn qa_o_on_last_line_without_trailing_newline() {
    let mut qa = Qa::at("alpha\nbeta", 9);
    qa.press(Key::O, Modifiers::NONE);
    assert_eq!(qa.mode, EditorVimMode::Insert);
    assert_eq!(qa.buffer.text(), "alpha\nbeta\n");
    assert_eq!(
        qa.cursor(),
        11,
        "cursor sits on the new empty line, ready to type"
    );

    qa.type_text("x");
    assert_eq!(qa.buffer.text(), "alpha\nbeta\nx");
}

#[test]
fn qa_shift_c_changes_to_end_of_line() {
    let mut qa = Qa::at("hello world", 6);
    qa.press(Key::C, Modifiers::SHIFT);
    assert_eq!(qa.mode, EditorVimMode::Insert);
    assert_eq!(qa.buffer.text(), "hello ");
    assert_eq!(qa.cursor(), 6, "`C` leaves the cursor at the change start");

    qa.type_text("there");
    qa.press(Key::Escape, Modifiers::NONE);
    assert_eq!(qa.buffer.text(), "hello there");
    assert_eq!(qa.buffer.undo_entry_count(), 1);

    qa.press(Key::U, Modifiers::NONE);
    assert_eq!(qa.buffer.text(), "hello world");
}

#[test]
fn qa_s_deletes_char_under_cursor_and_keeps_column() {
    let mut qa = Qa::at("abc", 1);
    qa.press(Key::S, Modifiers::NONE);
    assert_eq!(qa.mode, EditorVimMode::Insert);
    assert_eq!(qa.buffer.text(), "ac");
    assert_eq!(qa.cursor(), 1, "`s` replaces the char under the cursor");
}

#[test]
fn qa_s_on_empty_line_just_enters_insert() {
    let mut qa = Qa::at("", 0);
    qa.press(Key::S, Modifiers::NONE);
    assert_eq!(qa.mode, EditorVimMode::Insert);
    assert_eq!(qa.buffer.text(), "", "`s` on an empty line changes nothing");

    qa.type_text("x");
    assert_eq!(qa.buffer.text(), "x");
}

#[test]
fn qa_cw_deletes_word_and_cursor_at_change_start() {
    let mut qa = Qa::at("foo bar", 0);
    qa.press_keys(&[Key::C, Key::W]);
    assert_eq!(qa.mode, EditorVimMode::Insert);
    assert_eq!(qa.buffer.text(), " bar");
    assert_eq!(qa.cursor(), 0, "`cw` leaves the cursor at the change start");
}

#[test]
fn qa_ciw_deletes_inner_word_and_cursor_at_word_start() {
    let mut qa = Qa::at("one two three", 4);
    qa.press_keys(&[Key::C, Key::I, Key::W]);
    assert_eq!(qa.mode, EditorVimMode::Insert);
    assert_eq!(qa.buffer.text(), "one  three");
    assert_eq!(qa.cursor(), 4, "`ciw` leaves the cursor at the word start");
}

#[test]
fn qa_cc_clears_line_and_cursor_at_line_start() {
    let mut qa = Qa::at("alpha\nbeta", 7);
    qa.press_keys(&[Key::C, Key::C]);
    assert_eq!(qa.mode, EditorVimMode::Insert);
    assert_eq!(
        qa.buffer.text(),
        "alpha\n",
        "`cc` clears the line, keeping it"
    );
    assert_eq!(
        qa.cursor(),
        6,
        "`cc` leaves the cursor at the empty line start"
    );
}

#[test]
fn qa_escape_lands_cursor_on_last_typed_char() {
    let mut qa = Qa::at("abc", 0);
    qa.press(Key::I, Modifiers::NONE);
    qa.type_text("XYZ");
    assert_eq!(qa.buffer.text(), "XYZabc");
    assert_eq!(qa.cursor(), 3);

    qa.press(Key::Escape, Modifiers::NONE);
    assert_eq!(qa.mode, EditorVimMode::Normal);
    assert_eq!(qa.cursor(), 2, "Esc lands on the last typed char 'Z'");

    qa.press(Key::X, Modifiers::NONE);
    assert_eq!(
        qa.buffer.text(),
        "XYabc",
        "`x` after insert-exit deletes 'Z'"
    );
}

#[test]
fn qa_ctrl_open_bracket_lands_cursor_on_last_typed_char() {
    let mut qa = Qa::at("abc", 0);
    qa.press(Key::I, Modifiers::NONE);
    qa.type_text("XYZ");
    qa.press(Key::OpenBracket, Modifiers::CTRL);
    assert_eq!(qa.mode, EditorVimMode::Normal);
    assert_eq!(qa.cursor(), 2, "Ctrl+[ lands on the last typed char 'Z'");
}

#[test]
fn qa_long_session_with_punctuation_undoes_in_one_step() {
    let mut qa = Qa::at("one two three", 4);
    qa.press_keys(&[Key::C, Key::I, Key::W]);
    qa.type_text("Hello, (world)! [ok]");
    qa.press(Key::Escape, Modifiers::NONE);

    assert_eq!(qa.buffer.text(), "one Hello, (world)! [ok] three");
    assert_eq!(
        qa.buffer.undo_entry_count(),
        1,
        "a whole insert session is one undo step"
    );

    qa.press(Key::U, Modifiers::NONE);
    assert_eq!(qa.buffer.text(), "one two three");
}

#[test]
fn qa_two_sessions_create_two_undo_steps() {
    let mut qa = Qa::at("ab", 0);
    qa.press(Key::I, Modifiers::NONE);
    qa.type_text("X");
    qa.press(Key::Escape, Modifiers::NONE);

    qa.press_keys(&[Key::L, Key::L]);
    qa.press(Key::I, Modifiers::NONE);
    qa.type_text("Y");
    qa.press(Key::Escape, Modifiers::NONE);

    assert_eq!(qa.buffer.text(), "XaYb");
    assert_eq!(qa.buffer.undo_entry_count(), 2, "one undo step per session");

    qa.press(Key::U, Modifiers::NONE);
    assert_eq!(qa.buffer.text(), "Xab");
    qa.press(Key::U, Modifiers::NONE);
    assert_eq!(qa.buffer.text(), "ab");
}

#[test]
fn qa_o_text_escape_is_one_undo_step() {
    let mut qa = Qa::at("one", 0);
    qa.press(Key::O, Modifiers::NONE);
    qa.type_text("hello");
    qa.press(Key::Escape, Modifiers::NONE);

    assert_eq!(qa.buffer.text(), "one\nhello");
    assert_eq!(qa.buffer.undo_entry_count(), 1);

    qa.press(Key::U, Modifiers::NONE);
    assert_eq!(qa.buffer.text(), "one");
}

#[test]
fn qa_i_escape_without_typing_creates_no_undo_entry() {
    let mut qa = Qa::at("abc", 0);
    qa.press(Key::I, Modifiers::NONE);
    qa.press(Key::Escape, Modifiers::NONE);

    assert_eq!(qa.mode, EditorVimMode::Normal);
    assert_eq!(
        qa.buffer.undo_entry_count(),
        0,
        "an empty insert session must not pollute history"
    );
    assert!(qa.pending.is_none());

    qa.press(Key::X, Modifiers::NONE);
    assert_eq!(qa.buffer.text(), "bc");
    assert_eq!(qa.buffer.undo_entry_count(), 1);
    qa.press(Key::U, Modifiers::NONE);
    assert_eq!(qa.buffer.text(), "abc");
}

#[test]
fn qa_cw_escape_without_typing_single_undo() {
    let mut qa = Qa::at("foo bar", 0);
    qa.press_keys(&[Key::C, Key::W]);
    qa.press(Key::Escape, Modifiers::NONE);

    assert_eq!(qa.buffer.text(), " bar");
    assert_eq!(qa.buffer.undo_entry_count(), 1);

    qa.press(Key::U, Modifiers::NONE);
    assert_eq!(qa.buffer.text(), "foo bar");
}

#[test]
fn qa_backspace_mid_session_still_one_undo() {
    let mut qa = Qa::at("hi", 0);
    qa.press(Key::I, Modifiers::NONE);
    qa.type_text("abc");
    qa.host_backspace();
    qa.type_text("de");
    qa.press(Key::Escape, Modifiers::NONE);

    assert_eq!(qa.buffer.text(), "abdehi");
    assert_eq!(qa.buffer.undo_entry_count(), 1);

    qa.press(Key::U, Modifiers::NONE);
    assert_eq!(qa.buffer.text(), "hi");
}

#[test]
fn qa_dot_repeats_ciw_insert_on_another_word() {
    let mut qa = Qa::at("one two three", 4);
    qa.press_keys(&[Key::C, Key::I, Key::W]);
    qa.type_text("X");
    qa.press(Key::Escape, Modifiers::NONE);
    assert_eq!(qa.buffer.text(), "one X three");

    qa.press(Key::W, Modifiers::NONE);
    qa.press(Key::Period, Modifiers::NONE);
    assert_eq!(
        qa.buffer.text(),
        "one X X",
        "`.` replays ciw + typed text on the next word"
    );
    assert_eq!(
        qa.mode,
        EditorVimMode::Normal,
        "`.` replays the WHOLE recorded session, including its final Esc (`:h .`)"
    );
}

#[test]
fn qa_dot_replays_append_after_cursor() {
    let mut qa = Qa::at("ab cd", 0);
    qa.press(Key::A, Modifiers::NONE);
    qa.type_text("X");
    qa.press(Key::Escape, Modifiers::NONE);
    assert_eq!(qa.buffer.text(), "aXb cd");

    qa.press(Key::W, Modifiers::NONE);
    qa.press(Key::Period, Modifiers::NONE);
    qa.press(Key::Escape, Modifiers::NONE);
    assert_eq!(
        qa.buffer.text(),
        "aXb cXd",
        "`.` replays the append session"
    );
}

#[test]
fn qa_count_prefix_2i_repeats_inserted_text() {
    let mut qa = Qa::at("", 0);
    qa.press(Key::Num2, Modifiers::NONE);
    qa.press(Key::I, Modifiers::NONE);
    assert_eq!(qa.mode, EditorVimMode::Insert);
    assert!(qa.pending.is_none(), "count consumed on entry");

    qa.type_text("ab");
    qa.press(Key::Escape, Modifiers::NONE);

    assert_eq!(qa.buffer.text(), "abab", "`2i` types the text twice");
}

#[test]
fn qa_counted_append_and_open_line_expand_on_escape_in_one_undo() {
    let mut qa = Qa::at("ab", 0);
    qa.press(Key::Num3, Modifiers::NONE);
    qa.press(Key::A, Modifiers::SHIFT);
    qa.type_text("xyz");
    qa.press(Key::Escape, Modifiers::NONE);
    assert_eq!(qa.buffer.text(), "abxyzxyzxyz");
    assert_eq!(qa.cursor(), 10, "cursor rests on the last typed char");
    assert_eq!(qa.buffer.undo_entry_count(), 1);

    let mut qa = Qa::at("one", 0);
    qa.press_keys(&[Key::Num3, Key::O]);
    qa.type_text("foo");
    qa.press(Key::Escape, Modifiers::NONE);
    assert_eq!(qa.buffer.text(), "one\nfoo\nfoo\nfoo");
    assert_eq!(qa.buffer.undo_entry_count(), 1);

    let mut qa = Qa::at("ab", 0);
    qa.press_keys(&[Key::Num4, Key::I]);
    qa.press(Key::Escape, Modifiers::NONE);
    assert_eq!(qa.buffer.text(), "ab");
    assert_eq!(qa.buffer.undo_entry_count(), 0);
}

#[test]
fn qa_stale_count_from_2i_does_not_leak_into_x() {
    let mut qa = Qa::at("abc", 0);
    qa.press(Key::Num2, Modifiers::NONE);
    qa.press(Key::I, Modifiers::NONE);
    qa.press(Key::Escape, Modifiers::NONE);
    assert_eq!(qa.mode, EditorVimMode::Normal);
    assert!(qa.pending.is_none());

    qa.press(Key::X, Modifiers::NONE);
    assert_eq!(
        qa.buffer.text(),
        "bc",
        "the count typed before `i` must not repeat the following `x`"
    );
}

#[test]
fn qa_escape_mid_operator_cancels_cleanly() {
    let mut qa = Qa::at("foo bar", 0);
    qa.press(Key::D, Modifiers::NONE);
    assert!(qa.pending.is_some(), "`d` opens a pending operator");

    qa.press(Key::Escape, Modifiers::NONE);
    assert_eq!(qa.mode, EditorVimMode::Normal);
    assert!(qa.pending.is_none(), "Esc cancels the pending operator");
    assert_eq!(qa.buffer.text(), "foo bar");
    assert_eq!(qa.buffer.undo_entry_count(), 0, "no history pollution");

    qa.press_keys(&[Key::D, Key::W]);
    assert_eq!(qa.buffer.text(), "bar");
}

#[test]
fn qa_exit_lands_left_for_every_entry_path() {
    let mut deviations = Vec::new();

    let cases: Vec<(&str, Qa, (Key, Modifiers), usize)> = vec![
        (
            "i (typed, mid-line)",
            Qa::at("abc", 2).entered(&[(Key::I, Modifiers::NONE)]),
            (Key::Escape, Modifiers::NONE),
            2,
        ),
        (
            "i (typed, empty buffer)",
            Qa::at("", 0).entered(&[(Key::I, Modifiers::NONE)]),
            (Key::Escape, Modifiers::NONE),
            0,
        ),
        (
            "a",
            Qa::at("hello", 0).entered(&[(Key::A, Modifiers::NONE)]),
            (Key::Escape, Modifiers::NONE),
            1,
        ),
        (
            "a via Ctrl+[",
            Qa::at("hello", 0).entered(&[(Key::A, Modifiers::NONE)]),
            (Key::OpenBracket, Modifiers::CTRL),
            1,
        ),
        (
            "I",
            Qa::at("    hello", 0).entered(&[(Key::I, Modifiers::SHIFT)]),
            (Key::Escape, Modifiers::NONE),
            4,
        ),
        (
            "A",
            Qa::at("  hi", 0).entered(&[(Key::A, Modifiers::SHIFT)]),
            (Key::Escape, Modifiers::NONE),
            4,
        ),
        (
            "o",
            Qa::at("foo", 1).entered(&[(Key::O, Modifiers::NONE)]),
            (Key::Escape, Modifiers::NONE),
            4,
        ),
        (
            "O",
            Qa::at("foo", 1).entered(&[(Key::O, Modifiers::SHIFT)]),
            (Key::Escape, Modifiers::NONE),
            0,
        ),
        (
            "s",
            Qa::at("abc", 1).entered(&[(Key::S, Modifiers::NONE)]),
            (Key::Escape, Modifiers::NONE),
            1,
        ),
        (
            "S",
            Qa::at("abc", 0).entered(&[(Key::S, Modifiers::SHIFT)]),
            (Key::Escape, Modifiers::NONE),
            0,
        ),
        (
            "C",
            Qa::at("abc", 1).entered(&[(Key::C, Modifiers::SHIFT)]),
            (Key::Escape, Modifiers::NONE),
            1,
        ),
        (
            "cw",
            Qa::at("foo bar", 0).entered(&[(Key::C, Modifiers::NONE), (Key::W, Modifiers::NONE)]),
            (Key::Escape, Modifiers::NONE),
            0,
        ),
        (
            "ciw",
            Qa::at("one two", 4).entered(&[
                (Key::C, Modifiers::NONE),
                (Key::I, Modifiers::NONE),
                (Key::W, Modifiers::NONE),
            ]),
            (Key::Escape, Modifiers::NONE),
            4,
        ),
    ];

    for (label, qa, exit, expected) in cases {
        if let Some(deviation) = exit_left_deviation(label, qa, exit, expected) {
            deviations.push(deviation);
        }
    }

    let mut col_zero_untyped = Qa::at("abc", 0).entered(&[(Key::I, Modifiers::NONE)]);
    col_zero_untyped.press(Key::Escape, Modifiers::NONE);
    assert_eq!(col_zero_untyped.cursor(), 0, "col 0 exit must not move");

    let mut col_zero_line2_untyped = Qa::at("x\ny", 2).entered(&[(Key::I, Modifiers::NONE)]);
    col_zero_line2_untyped.press(Key::Escape, Modifiers::NONE);
    assert_eq!(
        col_zero_line2_untyped.cursor(),
        2,
        "col 0 exit on a later line must not move"
    );

    assert!(
        deviations.is_empty(),
        "insert-exit cursor deviations:\n  {}",
        deviations.join("\n  ")
    );
}

#[test]
fn qa_undo_then_redo_restores_insert_session_identically() {
    let mut qa = Qa::at("abc", 0);
    qa.press(Key::I, Modifiers::NONE);
    qa.type_text("XY");
    qa.press(Key::Escape, Modifiers::NONE);
    assert_eq!(qa.buffer.text(), "XYabc");
    let cursor_after_session = qa.cursor();

    qa.press(Key::U, Modifiers::NONE);
    assert_eq!(qa.buffer.text(), "abc");

    qa.press(Key::R, Modifiers::CTRL);
    assert_eq!(
        qa.buffer.text(),
        "XYabc",
        "Ctrl+R restores the insert session"
    );
    assert_eq!(
        qa.cursor(),
        cursor_after_session,
        "redo puts the cursor back where the session ended"
    );
    assert_eq!(
        qa.mode,
        EditorVimMode::Normal,
        "redo does not re-enter insert"
    );
}

#[test]
fn qa_shift_s_clears_line_and_session_is_one_undo() {
    let mut qa = Qa::at("alpha\nbeta", 7);
    qa.press(Key::S, Modifiers::SHIFT);
    assert_eq!(qa.mode, EditorVimMode::Insert);
    assert_eq!(qa.buffer.text(), "alpha\n", "`S` clears the cursor line");

    qa.type_text("gamma");
    qa.press(Key::Escape, Modifiers::NONE);
    assert_eq!(qa.buffer.text(), "alpha\ngamma");
    assert_eq!(qa.buffer.undo_entry_count(), 1);

    qa.press(Key::U, Modifiers::NONE);
    assert_eq!(qa.buffer.text(), "alpha\nbeta");
}

#[test]
fn qa_shift_o_opens_line_above_first_line_at_column_zero() {
    let mut qa = Qa::at("foo", 0);
    qa.press(Key::O, Modifiers::SHIFT);
    assert_eq!(qa.mode, EditorVimMode::Insert);
    assert_eq!(qa.buffer.text(), "\nfoo", "`O` opens a line above");
    assert_eq!(qa.cursor(), 0, "cursor sits on the new line at column 0");

    qa.type_text("x");
    assert_eq!(qa.buffer.text(), "x\nfoo");

    let mut qa = Qa::at("    foo", 6);
    qa.press(Key::O, Modifiers::SHIFT);
    assert_eq!(qa.buffer.text(), "    \n    foo", "`O` copies the indent");
    assert_eq!(qa.cursor(), 4, "cursor sits after the copied indent");

    qa.type_text("bar");
    assert_eq!(qa.buffer.text(), "    bar\n    foo");
}

#[test]
fn qa_i_types_into_empty_buffer_and_undoes_cleanly() {
    let mut qa = Qa::at("", 0);
    qa.press(Key::I, Modifiers::NONE);
    qa.type_text("hello");
    assert_eq!(qa.cursor(), 5);
    qa.press(Key::Escape, Modifiers::NONE);

    assert_eq!(qa.buffer.text(), "hello");
    assert_eq!(qa.buffer.undo_entry_count(), 1);

    qa.press(Key::U, Modifiers::NONE);
    assert_eq!(qa.buffer.text(), "", "undo empties the buffer again");
}

#[test]
fn qa_i_on_empty_middle_line_types_there() {
    let mut qa = Qa::at("one\n\ntwo", 4);
    qa.press(Key::I, Modifiers::NONE);
    assert_eq!(qa.cursor(), 4, "`i` stays on the empty middle line");

    qa.type_text("X");
    qa.press(Key::Escape, Modifiers::NONE);
    assert_eq!(qa.buffer.text(), "one\nX\ntwo");
}

#[test]
fn qa_o_escape_without_typing_single_undo_restores_line() {
    let mut qa = Qa::at("one", 0);
    qa.press(Key::O, Modifiers::NONE);
    qa.press(Key::Escape, Modifiers::NONE);

    assert_eq!(qa.buffer.text(), "one\n", "`o<Esc>` still creates the line");
    assert_eq!(
        qa.buffer.undo_entry_count(),
        1,
        "the opened line is one undo step"
    );

    qa.press(Key::U, Modifiers::NONE);
    assert_eq!(qa.buffer.text(), "one");
}
