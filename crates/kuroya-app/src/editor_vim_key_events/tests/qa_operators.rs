use super::*;

fn normal_buffer(text: &str, cursor: usize) -> (TextBuffer, EditorVimMode) {
    let mut buffer = TextBuffer::from_text(1, None, text.to_owned());
    buffer.set_single_cursor(cursor);
    (buffer, EditorVimMode::Normal)
}

fn press(
    buffer: &mut TextBuffer,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    unnamed_register: &mut Option<EditorVimRegister>,
    keys: &[(Key, Modifiers)],
) {
    let mut last_char_find = None;
    press_with_last_char_find(
        buffer,
        mode,
        pending,
        &mut last_char_find,
        unnamed_register,
        keys,
    );
}

fn press_with_last_char_find(
    buffer: &mut TextBuffer,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    last_char_find: &mut Option<EditorVimCharFind>,
    unnamed_register: &mut Option<EditorVimRegister>,
    keys: &[(Key, Modifiers)],
) {
    for (key, modifiers) in keys {
        let result = handle_vim_editor_key_event_with_state(
            buffer,
            *key,
            *modifiers,
            mode,
            pending,
            last_char_find,
            unnamed_register,
        );
        assert!(result.handled, "key {:?} must be handled", key);
    }
}

#[test]
fn normal_mode_d0_at_the_line_start_is_a_no_op_and_touches_no_register() {
    let (mut buffer, mut mode) = normal_buffer("alpha", 0);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::D, Modifiers::NONE), (Key::Num0, Modifiers::NONE)],
    );

    assert_eq!(buffer.text(), "alpha");
    assert_eq!(buffer.cursor(), 0);
    assert!(
        unnamed_register.is_none(),
        "d0 must not clobber the register"
    );
    assert!(pending.is_none());
}

#[test]
fn normal_mode_d_dollar_on_an_empty_line_is_a_no_op() {
    let (mut buffer, mut mode) = normal_buffer("one\n\ntwo", 4);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::D, Modifiers::NONE), (Key::Num4, Modifiers::SHIFT)],
    );

    assert_eq!(buffer.text(), "one\n\ntwo");
    assert!(unnamed_register.is_none());
    assert_eq!(mode, EditorVimMode::Normal);
    assert!(pending.is_none());
}

#[test]
fn normal_mode_df_char_without_a_match_leaves_buffer_and_register_untouched() {
    let (mut buffer, mut mode) = normal_buffer("abc", 0);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[
            (Key::D, Modifiers::NONE),
            (Key::F, Modifiers::NONE),
            (Key::Z, Modifiers::NONE),
        ],
    );

    assert_eq!(buffer.text(), "abc");
    assert!(
        unnamed_register.is_none(),
        "a failed motion must not write the register"
    );
    assert_eq!(mode, EditorVimMode::Normal);
    assert!(pending.is_none());
}

#[test]
fn normal_mode_dt_char_immediately_ahead_deletes_nothing() {
    let (mut buffer, mut mode) = normal_buffer("abc", 0);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[
            (Key::D, Modifiers::NONE),
            (Key::T, Modifiers::NONE),
            (Key::B, Modifiers::NONE),
        ],
    );

    assert_eq!(buffer.text(), "abc");
    assert!(unnamed_register.is_none());
    assert_eq!(mode, EditorVimMode::Normal);
    assert!(pending.is_none());
}

#[test]
fn normal_mode_df_includes_the_target_while_dt_stops_before_it() {
    let (mut buffer, mut mode) = normal_buffer("a_b_c", 0);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[
            (Key::D, Modifiers::NONE),
            (Key::F, Modifiers::NONE),
            (Key::B, Modifiers::NONE),
        ],
    );
    assert_eq!(buffer.text(), "_c");
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("a_b", EditorVimRegisterKind::Characterwise))
    );

    let (mut buffer, mut mode) = normal_buffer("a_b_c", 0);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[
            (Key::D, Modifiers::NONE),
            (Key::T, Modifiers::NONE),
            (Key::B, Modifiers::NONE),
        ],
    );
    assert_eq!(buffer.text(), "b_c");
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("a_", EditorVimRegisterKind::Characterwise))
    );
}

#[test]
fn normal_mode_d2f_deletes_through_the_nth_occurrence_of_the_target() {
    let (mut buffer, mut mode) = normal_buffer("a_b_a_b_c", 0);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[
            (Key::D, Modifiers::NONE),
            (Key::Num2, Modifiers::NONE),
            (Key::F, Modifiers::NONE),
            (Key::B, Modifiers::NONE),
        ],
    );

    assert_eq!(buffer.text(), "_c");
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("a_b_a_b", EditorVimRegisterKind::Characterwise))
    );
}

#[test]
fn normal_mode_df_never_crosses_the_line_break() {
    let (mut buffer, mut mode) = normal_buffer("abx\ncdx", 0);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[
            (Key::D, Modifiers::NONE),
            (Key::F, Modifiers::NONE),
            (Key::X, Modifiers::NONE),
        ],
    );

    assert_eq!(buffer.text(), "\ncdx");
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("abx", EditorVimRegisterKind::Characterwise))
    );
}

#[test]
fn normal_mode_d_semicolon_repeats_the_last_char_find_as_a_delete_motion() {
    let (mut buffer, mut mode) = normal_buffer("abxcdxc", 0);
    let mut pending = None;
    let mut unnamed_register = None;
    let mut last_char_find = None;

    press_with_last_char_find(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &[
            (Key::D, Modifiers::NONE),
            (Key::F, Modifiers::NONE),
            (Key::X, Modifiers::NONE),
        ],
    );
    assert_eq!(buffer.text(), "cdxc");

    press_with_last_char_find(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &[(Key::D, Modifiers::NONE), (Key::Semicolon, Modifiers::NONE)],
    );

    assert_eq!(buffer.text(), "c");
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("cdx", EditorVimRegisterKind::Characterwise))
    );
}

#[test]
fn normal_mode_d_capital_f_and_d_capital_t_include_the_char_under_the_cursor() {
    let (mut buffer, mut mode) = normal_buffer("xabcdx", 3);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[
            (Key::D, Modifiers::NONE),
            (Key::F, Modifiers::SHIFT),
            (Key::X, Modifiers::NONE),
        ],
    );
    assert_eq!(buffer.text(), "dx", "`dFx` is inclusive of the cursor char");
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("xabc", EditorVimRegisterKind::Characterwise))
    );

    let (mut buffer, mut mode) = normal_buffer("xabcdx", 3);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[
            (Key::D, Modifiers::NONE),
            (Key::T, Modifiers::SHIFT),
            (Key::X, Modifiers::NONE),
        ],
    );
    assert_eq!(
        buffer.text(),
        "xdx",
        "`dTx` runs from after the target through the cursor char"
    );
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("abc", EditorVimRegisterKind::Characterwise))
    );
}

#[test]
fn normal_mode_d_comma_repeats_the_last_char_find_in_the_flipped_direction() {
    let (mut buffer, mut mode) = normal_buffer("xabcxdx", 2);
    let mut pending = None;
    let mut unnamed_register = None;
    let mut last_char_find = None;

    press_with_last_char_find(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &[
            (Key::D, Modifiers::NONE),
            (Key::F, Modifiers::NONE),
            (Key::X, Modifiers::NONE),
        ],
    );
    assert_eq!(buffer.text(), "xadx");

    press_with_last_char_find(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut last_char_find,
        &mut unnamed_register,
        &[(Key::D, Modifiers::NONE), (Key::Comma, Modifiers::NONE)],
    );

    assert_eq!(buffer.text(), "x");
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("xad", EditorVimRegisterKind::Characterwise))
    );
}

#[test]
fn normal_mode_yf_yanks_charwise_through_the_target_and_keeps_the_cursor() {
    let (mut buffer, mut mode) = normal_buffer("abcxdef", 0);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[
            (Key::Y, Modifiers::NONE),
            (Key::F, Modifiers::NONE),
            (Key::X, Modifiers::NONE),
        ],
    );

    assert_eq!(buffer.text(), "abcxdef", "yank must not change the buffer");
    assert_eq!(
        buffer.cursor(),
        0,
        "a forward yank leaves the cursor in place"
    );
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("abcx", EditorVimRegisterKind::Characterwise))
    );
}

#[test]
fn normal_mode_charwise_put_before_the_cursor_lands_on_the_last_pasted_char() {
    let (mut buffer, mut mode) = normal_buffer("foo bar", 4);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[
            (Key::Y, Modifiers::NONE),
            (Key::E, Modifiers::NONE),
            (Key::P, Modifiers::SHIFT),
        ],
    );

    assert_eq!(buffer.text(), "foo barbar");
    assert_eq!(buffer.cursor(), 6);
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("bar", EditorVimRegisterKind::Characterwise))
    );
}

#[test]
fn normal_mode_yy_put_round_trip_pastes_the_line_below_on_its_first_non_blank() {
    let (mut buffer, mut mode) = normal_buffer("one\ntwo", 0);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::Y, Modifiers::NONE), (Key::Y, Modifiers::NONE)],
    );
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("one\n", EditorVimRegisterKind::Linewise))
    );

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::P, Modifiers::NONE)],
    );

    assert_eq!(buffer.text(), "one\none\ntwo");
    assert_eq!(
        buffer.cursor(),
        4,
        "`p` parks the cursor on the pasted line"
    );
}

#[test]
fn normal_mode_linewise_and_charwise_operators_fill_registers_of_the_right_kind() {
    let (mut buffer, mut mode) = normal_buffer("one\ntwo\nthree", 0);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::Y, Modifiers::NONE), (Key::J, Modifiers::NONE)],
    );
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("one\ntwo\n", EditorVimRegisterKind::Linewise))
    );

    let (mut buffer, mut mode) = normal_buffer("one two", 0);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::D, Modifiers::NONE), (Key::W, Modifiers::NONE)],
    );
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("one ", EditorVimRegisterKind::Characterwise))
    );
}

#[test]
fn normal_mode_dj_on_the_last_line_and_dk_on_the_first_line_are_boundary_no_ops() {
    let (mut buffer, mut mode) = normal_buffer("one\ntwo", 4);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::D, Modifiers::NONE), (Key::J, Modifiers::NONE)],
    );
    assert_eq!(buffer.text(), "one\ntwo");
    assert!(unnamed_register.is_none(), "`dj` fails on the last line");

    let (mut buffer, mut mode) = normal_buffer("one\ntwo", 0);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::D, Modifiers::NONE), (Key::K, Modifiers::NONE)],
    );
    assert_eq!(buffer.text(), "one\ntwo");
    assert!(unnamed_register.is_none(), "`dk` fails on the first line");
}

#[test]
fn normal_mode_d3j_deletes_the_cursor_line_and_three_lines_below() {
    let (mut buffer, mut mode) = normal_buffer("1\n2\n3\n4\n5", 0);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[
            (Key::D, Modifiers::NONE),
            (Key::Num3, Modifiers::NONE),
            (Key::J, Modifiers::NONE),
        ],
    );

    assert_eq!(buffer.text(), "5");
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("1\n2\n3\n4\n", EditorVimRegisterKind::Linewise))
    );
}

#[test]
fn normal_mode_2d3j_multiplies_the_counts_and_stays_linewise() {
    let (mut buffer, mut mode) = normal_buffer("1\n2\n3\n4\n5", 0);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[
            (Key::Num2, Modifiers::NONE),
            (Key::D, Modifiers::NONE),
            (Key::Num3, Modifiers::NONE),
            (Key::J, Modifiers::NONE),
        ],
    );

    assert_eq!(buffer.text(), "");
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("1\n2\n3\n4\n5\n", EditorVimRegisterKind::Linewise))
    );
}

#[test]
fn normal_mode_d5j_past_the_end_clamps_to_the_last_line() {
    let (mut buffer, mut mode) = normal_buffer("1\n2\n3", 2);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[
            (Key::D, Modifiers::NONE),
            (Key::Num5, Modifiers::NONE),
            (Key::J, Modifiers::NONE),
        ],
    );

    assert_eq!(buffer.text(), "1");
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("2\n3\n", EditorVimRegisterKind::Linewise))
    );
}

#[test]
fn normal_mode_d2gg_deletes_from_the_cursor_line_to_line_two() {
    let (mut buffer, mut mode) = normal_buffer("l1\nl2\nl3\nl4", 9);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[
            (Key::D, Modifiers::NONE),
            (Key::Num2, Modifiers::NONE),
            (Key::G, Modifiers::NONE),
            (Key::G, Modifiers::NONE),
        ],
    );

    assert_eq!(buffer.text(), "l1");
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("l2\nl3\nl4\n", EditorVimRegisterKind::Linewise))
    );
}

#[test]
fn normal_mode_d2_capital_g_deletes_from_the_cursor_line_to_line_two() {
    let (mut buffer, mut mode) = normal_buffer("l1\nl2\nl3\nl4", 0);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[
            (Key::D, Modifiers::NONE),
            (Key::Num2, Modifiers::NONE),
            (Key::G, Modifiers::SHIFT),
        ],
    );

    assert_eq!(buffer.text(), "l3\nl4");
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("l1\nl2\n", EditorVimRegisterKind::Linewise))
    );
}

#[test]
fn normal_mode_dw_on_the_last_char_of_the_buffer_deletes_it_without_the_newline() {
    let (mut buffer, mut mode) = normal_buffer("foo", 2);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::D, Modifiers::NONE), (Key::W, Modifiers::NONE)],
    );
    assert_eq!(buffer.text(), "fo");
    assert_eq!(
        unnamed_register.as_ref().map(|r| r.text.as_str()),
        Some("o")
    );

    let (mut buffer, mut mode) = normal_buffer("go\nstop", 6);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::D, Modifiers::NONE), (Key::W, Modifiers::NONE)],
    );
    assert_eq!(
        buffer.text(),
        "go\nsto",
        "`dw` on the final char keeps the newline"
    );
    assert_eq!(
        unnamed_register.as_ref().map(|r| r.text.as_str()),
        Some("p")
    );
}

#[test]
fn normal_mode_d100w_stops_at_the_last_word_and_never_eats_the_final_newline() {
    let (mut buffer, mut mode) = normal_buffer("foo\nbar\n", 0);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[
            (Key::D, Modifiers::NONE),
            (Key::Num1, Modifiers::NONE),
            (Key::Num0, Modifiers::NONE),
            (Key::Num0, Modifiers::NONE),
            (Key::W, Modifiers::NONE),
        ],
    );

    assert_eq!(buffer.text(), "\n");
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("foo\nbar", EditorVimRegisterKind::Characterwise))
    );
}

#[test]
fn normal_mode_db_and_dh_before_the_buffer_start_are_no_ops() {
    let (mut buffer, mut mode) = normal_buffer("word", 0);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::D, Modifiers::NONE), (Key::B, Modifiers::NONE)],
    );
    assert_eq!(buffer.text(), "word");
    assert!(unnamed_register.is_none());

    let (mut buffer, mut mode) = normal_buffer("word", 0);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::D, Modifiers::NONE), (Key::H, Modifiers::NONE)],
    );
    assert_eq!(buffer.text(), "word");
    assert!(unnamed_register.is_none());
}

#[test]
fn normal_mode_dl_on_the_last_char_deletes_it_like_x() {
    let (mut buffer, mut mode) = normal_buffer("abc", 2);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::D, Modifiers::NONE), (Key::L, Modifiers::NONE)],
    );

    assert_eq!(buffer.text(), "ab", "`dl` on the buffer's last char is `x`");
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("c", EditorVimRegisterKind::Characterwise))
    );

    let (mut buffer, mut mode) = normal_buffer("ab\ncd", 1);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::D, Modifiers::NONE), (Key::L, Modifiers::NONE)],
    );

    assert_eq!(buffer.text(), "a\ncd", "`dl` on a line's last char is `x`");
    assert_eq!(
        unnamed_register.as_ref().map(|r| r.text.as_str()),
        Some("b")
    );
}

#[test]
fn normal_mode_cw_from_whitespace_behaves_like_dw() {
    let (mut buffer, mut mode) = normal_buffer("foo   \nbar", 3);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::C, Modifiers::NONE), (Key::W, Modifiers::NONE)],
    );

    assert_eq!(mode, EditorVimMode::Insert);
    assert_eq!(buffer.text(), "foo\nbar");
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("   ", EditorVimRegisterKind::Characterwise))
    );
}

#[test]
fn normal_mode_cw_on_the_last_char_of_a_word_changes_only_that_char() {
    let (mut buffer, mut mode) = normal_buffer("bar", 2);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::C, Modifiers::NONE), (Key::W, Modifiers::NONE)],
    );

    assert_eq!(mode, EditorVimMode::Insert);
    assert_eq!(buffer.text(), "ba");
    assert_eq!(
        unnamed_register.as_ref().map(|r| r.text.as_str()),
        Some("r")
    );
}

#[test]
fn normal_mode_c_dollar_matches_capital_c_deleting_to_the_line_end_charwise() {
    let (mut buffer, mut mode) = normal_buffer("keep this tail", 5);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::C, Modifiers::NONE), (Key::Num4, Modifiers::SHIFT)],
    );
    assert_eq!(mode, EditorVimMode::Insert);
    assert_eq!(buffer.text(), "keep ");
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("this tail", EditorVimRegisterKind::Characterwise))
    );

    let (mut buffer, mut mode) = normal_buffer("keep this tail", 5);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::C, Modifiers::SHIFT)],
    );
    assert_eq!(mode, EditorVimMode::Insert);
    assert_eq!(buffer.text(), "keep ");
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("this tail", EditorVimRegisterKind::Characterwise))
    );
}

#[test]
fn normal_mode_capital_c_on_an_empty_line_still_enters_insert_mode() {
    let (mut buffer, mut mode) = normal_buffer("one\n\ntwo", 4);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::C, Modifiers::SHIFT)],
    );

    assert_eq!(mode, EditorVimMode::Insert);
    assert_eq!(buffer.text(), "one\n\ntwo");
    assert!(unnamed_register.is_none());
}

#[test]
fn normal_mode_capital_d_on_an_empty_line_is_a_no_op() {
    let (mut buffer, mut mode) = normal_buffer("one\n\ntwo", 4);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::D, Modifiers::SHIFT)],
    );

    assert_eq!(mode, EditorVimMode::Normal);
    assert_eq!(buffer.text(), "one\n\ntwo");
    assert!(unnamed_register.is_none());
}

#[test]
fn normal_mode_yw_on_the_last_word_of_a_line_yanks_the_word_not_the_newline() {
    let (mut buffer, mut mode) = normal_buffer("alpha beta\ngamma", 6);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::Y, Modifiers::NONE), (Key::W, Modifiers::NONE)],
    );

    assert_eq!(buffer.text(), "alpha beta\ngamma");
    assert_eq!(buffer.cursor(), 6);
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("beta", EditorVimRegisterKind::Characterwise))
    );
}

#[test]
fn normal_mode_dd_on_the_last_line_of_a_buffer_without_a_trailing_newline_deletes_the_line() {
    let (mut buffer, mut mode) = normal_buffer("a\nb", 2);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::D, Modifiers::NONE), (Key::D, Modifiers::NONE)],
    );

    assert_eq!(buffer.text(), "a");
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("b\n", EditorVimRegisterKind::Linewise))
    );
}

#[test]
fn normal_mode_dd_on_an_empty_line_deletes_the_line_and_keeps_the_rest() {
    let (mut buffer, mut mode) = normal_buffer("a\n\nc", 2);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::D, Modifiers::NONE), (Key::D, Modifiers::NONE)],
    );

    assert_eq!(buffer.text(), "a\nc");
    assert_eq!(
        unnamed_register.as_ref().map(|r| (r.text.as_str(), r.kind)),
        Some(("\n", EditorVimRegisterKind::Linewise))
    );
}

#[test]
fn normal_mode_d_percent_without_a_bracket_under_the_cursor_is_a_no_op() {
    let (mut buffer, mut mode) = normal_buffer("hello world", 0);
    let mut pending = None;
    let mut unnamed_register = None;

    press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[(Key::D, Modifiers::NONE), (Key::Num5, Modifiers::SHIFT)],
    );

    assert_eq!(buffer.text(), "hello world");
    assert!(unnamed_register.is_none());
    assert_eq!(mode, EditorVimMode::Normal);
    assert!(pending.is_none());
}
