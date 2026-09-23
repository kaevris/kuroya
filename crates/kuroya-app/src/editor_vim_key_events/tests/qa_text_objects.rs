use super::super::EditorVimLastChange;
use super::*;

fn qa_buffer(text: &str, cursor: usize) -> (TextBuffer, EditorVimMode) {
    let mut buffer = TextBuffer::from_text(1, None, text.to_owned());
    buffer.set_single_cursor(cursor);
    (buffer, EditorVimMode::Normal)
}

fn qa_press(
    buffer: &mut TextBuffer,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    unnamed_register: &mut Option<EditorVimRegister>,
    keys: &[(Key, Modifiers)],
) {
    for (key, modifiers) in keys {
        handle_vim_editor_key_event_with_state(
            buffer,
            *key,
            *modifiers,
            mode,
            pending,
            &mut None,
            unnamed_register,
        );
    }
}

fn qa_press_repeat(
    buffer: &mut TextBuffer,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    keys: &[(Key, Modifiers)],
) {
    for (key, modifiers) in keys {
        handle_vim_editor_key_event_with_repeat_state(
            buffer,
            *key,
            *modifiers,
            mode,
            pending,
            &mut None,
            unnamed_register,
            last_change,
        );
    }
}

fn k(key: Key) -> (Key, Modifiers) {
    (key, Modifiers::NONE)
}

fn ks(key: Key) -> (Key, Modifiers) {
    (key, Modifiers::SHIFT)
}

fn register_text(unnamed_register: &Option<EditorVimRegister>) -> Option<&str> {
    unnamed_register
        .as_ref()
        .map(|register| register.text.as_str())
}

#[test]
fn qa_finding_iw_on_blank_adjacent_to_a_word_selects_the_blank_run() {
    let (mut buffer, mut mode) = qa_buffer("foo bar", 3);
    let mut pending = None;
    let mut unnamed_register = None;

    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::I), k(Key::W)],
    );

    assert_eq!(buffer.text(), "foobar");
    assert_eq!(register_text(&unnamed_register), Some(" "));
}

#[test]
fn qa_finding_iw_on_punctuation_selects_the_punctuation_run() {
    let (mut buffer, mut mode) = qa_buffer("beta.gamma", 4);
    let mut pending = None;
    let mut unnamed_register = None;

    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::I), k(Key::W)],
    );

    assert_eq!(buffer.text(), "betagamma");
    assert_eq!(register_text(&unnamed_register), Some("."));
}

#[test]
fn qa_finding_iw_big_word_on_whitespace_selects_the_blank_run() {
    let (mut buffer, mut mode) = qa_buffer("foo   bar", 4);
    let mut pending = None;
    let mut unnamed_register = None;
    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::I), ks(Key::W)],
    );
    assert_eq!(buffer.text(), "foobar");
    assert_eq!(register_text(&unnamed_register), Some("   "));

    let (mut buffer, mut mode) = qa_buffer("foo   bar", 3);
    let mut pending = None;
    let mut unnamed_register = None;
    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::I), ks(Key::W)],
    );
    assert_eq!(buffer.text(), "foobar");
    assert_eq!(register_text(&unnamed_register), Some("   "));
}

#[test]
fn qa_finding_daw_on_whitespace_takes_the_following_word() {
    let (mut buffer, mut mode) = qa_buffer("foo bar", 3);
    let mut pending = None;
    let mut unnamed_register = None;
    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::A), k(Key::W)],
    );
    assert_eq!(buffer.text(), "foo");
    assert_eq!(register_text(&unnamed_register), Some(" bar"));

    let (mut buffer, mut mode) = qa_buffer("    foo", 1);
    let mut pending = None;
    let mut unnamed_register = None;
    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::A), k(Key::W)],
    );
    assert_eq!(buffer.text(), "");
    assert_eq!(register_text(&unnamed_register), Some("    foo"));
}

#[test]
fn qa_finding_i_quote_between_two_strings_uses_the_adjacent_quotes() {
    let (mut buffer, mut mode) = qa_buffer("say \"alpha\" ok \"beta\" end", 12);
    let mut pending = None;
    let mut unnamed_register = None;

    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::I), ks(Key::Quote)],
    );

    assert_eq!(buffer.text(), "say \"alpha\"\"beta\" end");
    assert_eq!(register_text(&unnamed_register), Some(" ok "));
}

#[test]
fn qa_finding_a_quote_includes_trailing_whitespace_else_leading() {
    let (mut buffer, mut mode) = qa_buffer("call 'beta' now", 6);
    let mut pending = None;
    let mut unnamed_register = None;
    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::A), k(Key::Quote)],
    );
    assert_eq!(buffer.text(), "call now");
    assert_eq!(register_text(&unnamed_register), Some("'beta' "));

    let (mut buffer, mut mode) = qa_buffer("a 'beta'", 4);
    let mut pending = None;
    let mut unnamed_register = None;
    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::A), k(Key::Quote)],
    );
    assert_eq!(buffer.text(), "a");
    assert_eq!(register_text(&unnamed_register), Some(" 'beta'"));
}

#[test]
fn qa_finding_count_two_on_quote_objects_includes_the_quotes() {
    let (mut buffer, mut mode) = qa_buffer("say \"hi\" now", 5);
    let mut pending = None;
    let mut unnamed_register = None;
    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::Num2), k(Key::D), k(Key::I), ks(Key::Quote)],
    );
    assert_eq!(buffer.text(), "say  now");
    assert_eq!(register_text(&unnamed_register), Some("\"hi\""));

    let (mut buffer, mut mode) = qa_buffer("x 'a' y 'b' z", 3);
    let mut pending = None;
    let mut unnamed_register = None;
    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::Num2), k(Key::D), k(Key::I), k(Key::Quote)],
    );
    assert_eq!(buffer.text(), "x  y 'b' z");
    assert_eq!(register_text(&unnamed_register), Some("'a'"));
}

#[test]
fn qa_finding_block_object_forward_search_crosses_lines() {
    let (mut buffer, mut mode) = qa_buffer("foo\n(bar)", 0);
    let mut pending = None;
    let mut unnamed_register = None;

    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::I), ks(Key::Num9)],
    );

    assert_eq!(buffer.text(), "foo\n()");
    assert_eq!(register_text(&unnamed_register), Some("bar"));
}

#[test]
fn qa_daw_prefers_trailing_whitespace_up_to_buffer_end() {
    let (mut buffer, mut mode) = qa_buffer("foo bar  ", 4);
    let mut pending = None;
    let mut unnamed_register = None;

    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::A), k(Key::W)],
    );

    assert_eq!(mode, EditorVimMode::Normal);
    assert_eq!(buffer.text(), "foo ");
    assert_eq!(register_text(&unnamed_register), Some("bar  "));
}

#[test]
fn qa_daw_at_line_end_falls_back_to_leading_and_never_eats_the_newline() {
    let (mut buffer, mut mode) = qa_buffer("foo bar\nbaz", 4);
    let mut pending = None;
    let mut unnamed_register = None;

    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::A), k(Key::W)],
    );

    assert_eq!(buffer.text(), "foo\nbaz");
    assert_eq!(register_text(&unnamed_register), Some(" bar"));
}

#[test]
fn qa_daw_on_a_single_char_word_takes_the_trailing_blank() {
    let (mut buffer, mut mode) = qa_buffer("a b c", 2);
    let mut pending = None;
    let mut unnamed_register = None;

    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::A), k(Key::W)],
    );

    assert_eq!(buffer.text(), "a c");
    assert_eq!(register_text(&unnamed_register), Some("b "));
}

#[test]
fn qa_diw_with_cursor_on_the_last_char_of_the_word() {
    let (mut buffer, mut mode) = qa_buffer("alpha beta", 9);
    let mut pending = None;
    let mut unnamed_register = None;

    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::I), k(Key::W)],
    );

    assert_eq!(mode, EditorVimMode::Normal);
    assert_eq!(buffer.text(), "alpha ");
    assert_eq!(register_text(&unnamed_register), Some("beta"));
}

#[test]
fn qa_di_paren_degenerate_cases_are_no_ops_that_spare_the_register() {
    let (mut buffer, mut mode) = qa_buffer("a (b) done", 7);
    let mut pending = None;
    let mut unnamed_register = None;
    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::I), ks(Key::Num9)],
    );
    assert_eq!(buffer.text(), "a (b) done");
    assert!(unnamed_register.is_none());

    let (mut buffer, mut mode) = qa_buffer("fn() tail", 3);
    let mut pending = None;
    let mut unnamed_register = None;
    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::I), ks(Key::Num9)],
    );
    assert_eq!(buffer.text(), "fn() tail");
    assert!(unnamed_register.is_none());
}

#[test]
fn qa_da_paren_with_cursor_on_the_close_bracket_includes_both_brackets() {
    let (mut buffer, mut mode) = qa_buffer("(value)", 6);
    let mut pending = None;
    let mut unnamed_register = None;

    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::A), ks(Key::Num9)],
    );

    assert_eq!(buffer.text(), "");
    assert_eq!(register_text(&unnamed_register), Some("(value)"));
}

#[test]
fn qa_d2i_paren_climbs_one_level_per_count() {
    let (mut buffer, mut mode) = qa_buffer("outer(inner(value))", 13);
    let mut pending = None;
    let mut unnamed_register = None;

    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::Num2), k(Key::D), k(Key::I), ks(Key::Num9)],
    );

    assert_eq!(buffer.text(), "outer()");
    assert_eq!(register_text(&unnamed_register), Some("inner(value)"));
}

#[test]
fn qa_di_paren_on_the_inner_open_paren_selects_the_inner_block() {
    let (mut buffer, mut mode) = qa_buffer("outer(inner(value))", 11);
    let mut pending = None;
    let mut unnamed_register = None;

    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::I), ks(Key::Num9)],
    );

    assert_eq!(buffer.text(), "outer(inner())");
    assert_eq!(register_text(&unnamed_register), Some("value"));
}

#[test]
fn qa_ib_spanning_lines_includes_the_newline() {
    let (mut buffer, mut mode) = qa_buffer("fn (a\nb) c", 5);
    let mut pending = None;
    let mut unnamed_register = None;

    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::I), ks(Key::Num9)],
    );

    assert_eq!(buffer.text(), "fn () c");
    assert_eq!(register_text(&unnamed_register), Some("a\nb"));
}

#[test]
fn qa_di_quote_with_cursor_on_either_quote_selects_the_inner_text() {
    for cursor in [4, 8] {
        let (mut buffer, mut mode) = qa_buffer("say \"foo\" now", cursor);
        let mut pending = None;
        let mut unnamed_register = None;
        qa_press(
            &mut buffer,
            &mut mode,
            &mut pending,
            &mut unnamed_register,
            &[k(Key::D), k(Key::I), ks(Key::Quote)],
        );
        assert_eq!(buffer.text(), "say \"\" now");
        assert_eq!(register_text(&unnamed_register), Some("foo"));
    }
}

#[test]
fn qa_di_quote_on_an_empty_string_is_an_error() {
    let (mut buffer, mut mode) = qa_buffer("say \"\" now", 5);
    let mut pending = None;
    let mut unnamed_register = None;

    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::I), ks(Key::Quote)],
    );

    assert_eq!(buffer.text(), "say \"\" now");
    assert!(unnamed_register.is_none());
}

#[test]
fn qa_di_quote_skips_escaped_quotes_when_pairing() {
    let (mut buffer, mut mode) = qa_buffer("say \"a \\\" b\" end", 10);
    let mut pending = None;
    let mut unnamed_register = None;

    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::I), ks(Key::Quote)],
    );

    assert_eq!(buffer.text(), "say \"\" end");
    assert_eq!(register_text(&unnamed_register), Some("a \\\" b"));
}

#[test]
fn qa_di_tick_ignores_a_lone_apostrophe_inside_a_double_quoted_string() {
    let (mut buffer, mut mode) = qa_buffer("call \"it's fine\" now", 12);
    let mut pending = None;
    let mut unnamed_register = None;

    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::I), k(Key::Quote)],
    );

    assert_eq!(buffer.text(), "call \"it's fine\" now");
    assert!(unnamed_register.is_none());
}

#[test]
fn qa_yi_paren_yanks_inner_block_characterwise() {
    let (mut buffer, mut mode) = qa_buffer("call(alpha)", 6);
    let mut pending = None;
    let mut unnamed_register = None;

    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::Y), k(Key::I), ks(Key::Num9)],
    );

    assert_eq!(mode, EditorVimMode::Normal);
    assert_eq!(buffer.text(), "call(alpha)");
    assert_eq!(
        unnamed_register
            .as_ref()
            .map(|register| (register.text.as_str(), register.kind)),
        Some(("alpha", EditorVimRegisterKind::Characterwise))
    );
}

#[test]
fn qa_di_bracket_deletes_the_innermost_content() {
    let (mut buffer, mut mode) = qa_buffer("arr [x] tail", 6);
    let mut pending = None;
    let mut unnamed_register = None;

    qa_press(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &[k(Key::D), k(Key::I), k(Key::OpenBracket)],
    );

    assert_eq!(buffer.text(), "arr [] tail");
    assert_eq!(register_text(&unnamed_register), Some("x"));
}

#[test]
fn qa_dot_repeat_reapplies_a_block_text_object_delete() {
    let (mut buffer, mut mode) = qa_buffer("a (b) c (d)", 3);
    let mut pending = None;
    let mut unnamed_register = None;
    let mut last_change = None;

    qa_press_repeat(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &mut last_change,
        &[k(Key::D), k(Key::I), ks(Key::Num9)],
    );
    assert_eq!(buffer.text(), "a () c (d)");

    qa_press_repeat(
        &mut buffer,
        &mut mode,
        &mut pending,
        &mut unnamed_register,
        &mut last_change,
        &[k(Key::W), k(Key::Period)],
    );

    assert_eq!(mode, EditorVimMode::Normal);
    assert_eq!(buffer.text(), "a () c ()");
    assert_eq!(register_text(&unnamed_register), Some("d"));
    assert!(pending.is_none());
}
