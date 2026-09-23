use super::super::{EditorVimLastChange, VimKeyResult};
use super::*;

struct Qa {
    buffer: TextBuffer,
    mode: EditorVimMode,
    pending: Option<EditorVimPendingKey>,
    last_char_find: Option<EditorVimCharFind>,
    unnamed_register: Option<EditorVimRegister>,
    last_change: Option<EditorVimLastChange>,
}

fn k(key: Key) -> (Key, Modifiers) {
    (key, Modifiers::NONE)
}

fn ks(key: Key) -> (Key, Modifiers) {
    (key, Modifiers::SHIFT)
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

    fn press(&mut self, (key, modifiers): (Key, Modifiers)) -> VimKeyResult {
        handle_vim_editor_key_event_with_repeat_state(
            &mut self.buffer,
            key,
            modifiers,
            &mut self.mode,
            &mut self.pending,
            &mut self.last_char_find,
            &mut self.unnamed_register,
            &mut self.last_change,
        )
    }

    fn keys(&mut self, keys: &[(Key, Modifiers)]) {
        for combo in keys {
            self.press(*combo);
        }
    }

    fn type_text(&mut self, text: &str) {
        self.buffer.insert_at_cursors(text);
        vim_record_inserted_text(&mut self.last_change, text);
    }

    fn undo(&mut self) {
        self.press((Key::U, Modifiers::NONE));
    }

    fn redo(&mut self) {
        self.press((Key::R, Modifiers::CTRL));
    }

    fn dot(&mut self) -> VimKeyResult {
        self.press((Key::Period, Modifiers::NONE))
    }

    fn text(&self) -> String {
        self.buffer.text()
    }

    fn undo_entries(&self) -> usize {
        self.buffer.undo_entry_count()
    }

    fn ex(&mut self, command: &str) {
        self.press((Key::Semicolon, Modifiers::SHIFT));
        for ch in command.chars() {
            self.press(ex_char_key(ch));
        }
        self.press(k(Key::Enter));
    }
}

fn ex_char_key(ch: char) -> (Key, Modifiers) {
    match ch {
        'a' => k(Key::A),
        'b' => k(Key::B),
        'c' => k(Key::C),
        'd' => k(Key::D),
        'e' => k(Key::E),
        'f' => k(Key::F),
        'g' => k(Key::G),
        'h' => k(Key::H),
        'i' => k(Key::I),
        'j' => k(Key::J),
        'k' => k(Key::K),
        'l' => k(Key::L),
        'm' => k(Key::M),
        'n' => k(Key::N),
        'o' => k(Key::O),
        'p' => k(Key::P),
        'q' => k(Key::Q),
        'r' => k(Key::R),
        's' => k(Key::S),
        't' => k(Key::T),
        'u' => k(Key::U),
        'v' => k(Key::V),
        'w' => k(Key::W),
        'x' => k(Key::X),
        'y' => k(Key::Y),
        'z' => k(Key::Z),
        '/' => k(Key::Slash),
        '%' => ks(Key::Num5),
        _ => panic!("qa helper does not map {ch}"),
    }
}

#[test]
fn qa_dd_then_dd_are_two_separate_undos() {
    let mut qa = Qa::at("l1\nl2\nl3\nl4", 0);
    qa.keys(&[k(Key::D), k(Key::D)]);
    assert_eq!(qa.text(), "l2\nl3\nl4");
    qa.keys(&[k(Key::D), k(Key::D)]);
    assert_eq!(qa.text(), "l3\nl4");

    assert_eq!(qa.undo_entries(), 2);

    qa.undo();
    assert_eq!(qa.text(), "l2\nl3\nl4");
    qa.undo();
    assert_eq!(qa.text(), "l1\nl2\nl3\nl4");
}

#[test]
fn qa_three_dd_collapses_to_one_undo() {
    let mut qa = Qa::at("l1\nl2\nl3\nl4", 0);
    qa.keys(&[k(Key::Num3), k(Key::D), k(Key::D)]);
    assert_eq!(qa.text(), "l4");

    assert_eq!(qa.undo_entries(), 1);
    qa.undo();
    assert_eq!(qa.text(), "l1\nl2\nl3\nl4");
}

#[test]
fn qa_visual_line_delete_of_five_lines_is_one_undo() {
    let mut qa = Qa::at("a\nb\nc\nd\ne\nf", 0);
    qa.keys(&[
        ks(Key::V),
        k(Key::J),
        k(Key::J),
        k(Key::J),
        k(Key::J),
        k(Key::D),
    ]);
    assert_eq!(qa.text(), "f");

    assert_eq!(qa.undo_entries(), 1);
    qa.undo();
    assert_eq!(qa.text(), "a\nb\nc\nd\ne\nf");
}

#[test]
fn qa_visual_gt_over_five_lines_is_one_undo_and_dot_repeats_after_undo() {
    let mut qa = Qa::at("a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl", 0);

    qa.keys(&[
        ks(Key::V),
        k(Key::J),
        k(Key::J),
        k(Key::J),
        k(Key::J),
        ks(Key::Period),
    ]);
    assert_eq!(
        qa.text(),
        "    a\n    b\n    c\n    d\n    e\nf\ng\nh\ni\nj\nk\nl"
    );
    assert_eq!(qa.undo_entries(), 1);

    qa.undo();
    assert_eq!(qa.text(), "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl");

    for _ in 0..6 {
        qa.press(k(Key::J));
    }
    qa.dot();
    assert_eq!(
        qa.text(),
        "a\nb\nc\nd\ne\nf\n    g\n    h\n    i\n    j\n    k\nl"
    );
}

#[test]
fn qa_visual_char_change_with_text_is_one_undo() {
    let mut qa = Qa::at("alpha beta\ngamma delta", 0);

    qa.keys(&[k(Key::V), k(Key::E), k(Key::C)]);
    assert_eq!(qa.mode, EditorVimMode::Insert);
    qa.type_text("mid");
    qa.press(k(Key::Escape));
    assert_eq!(qa.text(), "mid beta\ngamma delta");
    assert_eq!(qa.undo_entries(), 1);
    qa.undo();
    assert_eq!(qa.text(), "alpha beta\ngamma delta");
}

#[test]
fn qa_ex_substitute_undoes_in_one_step_and_redo_round_trips() {
    let mut qa = Qa::at("foo bar\nfoo baz", 0);
    qa.ex("s/foo/zoo/");
    assert_eq!(qa.text(), "zoo bar\nfoo baz");
    assert_eq!(qa.undo_entries(), 1);
    qa.undo();
    assert_eq!(qa.text(), "foo bar\nfoo baz");
    qa.redo();
    assert_eq!(qa.text(), "zoo bar\nfoo baz");
}

#[test]
fn qa_ex_substitute_repeats_with_dot() {
    let mut qa = Qa::at("foo bar\nfoo baz", 0);
    qa.ex("s/foo/zoo/");
    assert_eq!(qa.text(), "zoo bar\nfoo baz");
    qa.press(k(Key::J));
    qa.dot();

    assert_eq!(qa.text(), "zoo bar\nzoo baz");
}

#[test]
fn qa_ciw_insert_session_is_one_undo_and_redo_restores() {
    let mut qa = Qa::at("alpha beta", 0);
    qa.keys(&[k(Key::C), k(Key::I), k(Key::W)]);
    qa.type_text("core");
    qa.press(k(Key::Escape));
    assert_eq!(qa.text(), "core beta");
    assert_eq!(qa.undo_entries(), 1);
    qa.undo();
    assert_eq!(qa.text(), "alpha beta");
    qa.redo();
    assert_eq!(qa.text(), "core beta");
}

#[test]
fn qa_dot_repeats_dd_then_dj_linerewise() {
    let mut qa = Qa::at("l1\nl2\nl3", 0);
    qa.keys(&[k(Key::D), k(Key::D)]);
    qa.dot();
    assert_eq!(qa.text(), "l3");

    let mut qa = Qa::at("a\nb\nc\nd", 0);
    qa.keys(&[k(Key::D), k(Key::J)]);
    assert_eq!(qa.text(), "c\nd");
    qa.dot();

    assert_eq!(qa.text(), "");
}

#[test]
fn qa_dot_dj_at_last_pair_fails_without_corrupting() {
    let mut qa = Qa::at("a\nb", 0);
    qa.keys(&[k(Key::D), k(Key::J)]);
    assert_eq!(qa.text(), "");
    assert_eq!(qa.undo_entries(), 1);

    qa.dot();
    assert_eq!(qa.text(), "");
    assert_eq!(qa.undo_entries(), 1);
}

#[test]
fn qa_dot_repeats_x_with_counts_and_counted_dot() {
    let mut qa = Qa::at("abcdef", 0);
    qa.keys(&[k(Key::Num3), k(Key::X)]);
    assert_eq!(qa.text(), "def");
    qa.dot();
    assert_eq!(qa.text(), "");

    let mut qa = Qa::at("abcdef", 0);
    qa.press(k(Key::X));
    assert_eq!(qa.text(), "bcdef");
    qa.keys(&[k(Key::Num3)]);
    qa.dot();
    assert_eq!(qa.text(), "ef");
}

#[test]
fn qa_dot_repeats_x_forward_and_x_backward() {
    let mut qa = Qa::at("abcdef", 0);
    qa.press(k(Key::X));
    assert_eq!(qa.text(), "bcdef");
    qa.dot();
    assert_eq!(qa.text(), "cdef");

    let mut qa = Qa::at("abcd", 3);
    qa.press(ks(Key::X));
    assert_eq!(qa.text(), "abd");
    qa.dot();
    assert_eq!(qa.text(), "ad");
}

#[test]
fn qa_failed_x_does_not_clobber_last_change() {
    let mut qa = Qa::at("abcdef", 1);
    qa.press(k(Key::X));
    assert_eq!(qa.text(), "acdef");

    qa.press(k(Key::Num0));
    qa.press(ks(Key::X));
    assert_eq!(qa.text(), "acdef");
    qa.dot();
    assert_eq!(qa.text(), "cdef");
}

#[test]
fn qa_dot_repeats_substitute_char_with_text() {
    let mut qa = Qa::at("one two", 0);
    qa.keys(&[k(Key::S)]);
    qa.type_text("1");
    qa.press(k(Key::Escape));
    assert_eq!(qa.text(), "1ne two");

    qa.buffer.set_single_cursor(0);
    qa.press(k(Key::L));
    qa.dot();

    assert_eq!(qa.text(), "11e two");
}

#[test]
fn qa_dot_repeats_delete_to_line_end() {
    let mut qa = Qa::at("first rest\nsecond line", 0);
    qa.press(ks(Key::D));
    assert_eq!(qa.text(), "\nsecond line");
    qa.press(k(Key::J));
    qa.dot();
    assert_eq!(qa.text(), "\n");
}

#[test]
fn qa_dot_repeats_change_to_line_end_with_text() {
    let mut qa = Qa::at("alpha beta\ngamma", 0);
    qa.keys(&[ks(Key::C)]);
    qa.type_text("!");
    qa.press(k(Key::Escape));
    assert_eq!(qa.text(), "!\ngamma");

    qa.buffer.set_single_cursor(0);
    qa.press(k(Key::J));
    qa.dot();
    assert_eq!(qa.text(), "!\n!");
}

#[test]
fn qa_dot_repeats_ciw_with_text() {
    let mut qa = Qa::at("alpha beta gamma", 0);
    qa.keys(&[k(Key::C), k(Key::I), k(Key::W)]);
    qa.type_text("core");
    qa.press(k(Key::Escape));
    assert_eq!(qa.text(), "core beta gamma");
    qa.press(k(Key::W));
    qa.dot();
    assert_eq!(qa.text(), "core core gamma");
}

#[test]
fn qa_dot_repeats_open_line_with_text() {
    let mut qa = Qa::at("top\nbottom", 0);
    qa.keys(&[k(Key::O)]);
    qa.type_text("mid");
    qa.press(k(Key::Escape));
    assert_eq!(qa.text(), "top\nmid\nbottom");
    qa.press(k(Key::K));
    qa.dot();
    assert_eq!(qa.text(), "top\nmid\nmid\nbottom");
}

#[test]
fn qa_dot_repeats_join() {
    let mut qa = Qa::at("one\ntwo\nthree", 0);
    qa.press(ks(Key::J));
    assert_eq!(qa.text(), "one two\nthree");
    qa.dot();
    assert_eq!(qa.text(), "one two three");
}

#[test]
fn qa_dot_repeats_indent_and_outdent() {
    let mut qa = Qa::at("a\nb\nc", 0);
    qa.keys(&[ks(Key::Period), ks(Key::Period)]);
    assert_eq!(qa.text(), "    a\nb\nc");
    qa.press(k(Key::J));
    qa.dot();
    assert_eq!(qa.text(), "    a\n    b\nc");

    let mut qa = Qa::at("    a\n    b\n    c", 0);
    qa.keys(&[ks(Key::Comma), ks(Key::Comma)]);
    assert_eq!(qa.text(), "a\n    b\n    c");
    qa.press(k(Key::J));
    qa.dot();
    assert_eq!(qa.text(), "a\nb\n    c");
}

#[test]
fn qa_dot_repeats_visual_d_width() {
    let mut qa = Qa::at("alpha beta gamma", 0);

    qa.keys(&[k(Key::V), k(Key::L), k(Key::L), k(Key::L), k(Key::D)]);
    assert_eq!(qa.text(), "a beta gamma");
    qa.dot();

    assert_eq!(qa.text(), "ta gamma");
}

#[test]
fn qa_dot_repeats_visual_gt_line_count() {
    let mut qa = Qa::at("a\nb\nc\nd\ne\nf\ng\nh", 0);

    qa.keys(&[ks(Key::V), k(Key::J), k(Key::J), ks(Key::Period)]);
    assert_eq!(qa.text(), "    a\n    b\n    c\nd\ne\nf\ng\nh");
    for _ in 0..3 {
        qa.press(k(Key::J));
    }
    qa.dot();

    assert_eq!(qa.text(), "    a\n    b\n    c\n    d\n    e\n    f\ng\nh");
}

#[test]
fn qa_dot_with_no_last_change_is_noop() {
    let mut qa = Qa::at("hello", 0);
    let result = qa.dot();
    assert!(result.handled);
    assert!(!result.changed);
    assert_eq!(qa.text(), "hello");
    assert_eq!(qa.undo_entries(), 0);
}

#[test]
fn qa_yank_between_edit_and_dot_preserves_repeat() {
    let mut qa = Qa::at("alpha beta", 0);
    qa.press(k(Key::X));
    assert_eq!(qa.text(), "lpha beta");

    qa.keys(&[k(Key::Y), k(Key::W)]);
    assert!(
        qa.unnamed_register
            .as_ref()
            .map(|register| register.text.starts_with("lpha"))
            .unwrap_or(false)
    );
    qa.dot();

    assert_eq!(qa.text(), "pha beta");
}

#[test]
fn qa_undo_keeps_register_and_redo_does_not_rewrite() {
    let mut qa = Qa::at("alpha beta\ngamma", 0);
    qa.keys(&[k(Key::D), k(Key::D)]);
    let after_dd = qa.unnamed_register.clone();
    assert_eq!(
        after_dd.as_ref().map(|r| r.text.as_str()),
        Some("alpha beta\n")
    );

    qa.undo();

    assert_eq!(qa.unnamed_register, after_dd);

    qa.redo();

    assert_eq!(qa.unnamed_register, after_dd);
}

#[test]
fn qa_change_register_kept_on_undo() {
    let mut qa = Qa::at("alpha beta", 0);
    qa.keys(&[k(Key::C), k(Key::I), k(Key::W)]);
    qa.type_text("core");
    qa.press(k(Key::Escape));
    assert_eq!(
        qa.unnamed_register.as_ref().map(|r| r.text.as_str()),
        Some("alpha")
    );
    qa.undo();

    assert_eq!(
        qa.unnamed_register.as_ref().map(|r| r.text.as_str()),
        Some("alpha")
    );
}

#[test]
fn qa_dot_after_undo_reapplies_and_clears_redo_stack() {
    let mut qa = Qa::at("abcdef", 0);
    qa.press(k(Key::X));
    assert_eq!(qa.text(), "bcdef");
    qa.undo();
    assert_eq!(qa.text(), "abcdef");

    qa.dot();
    assert_eq!(qa.text(), "bcdef");

    assert_eq!(qa.buffer.redo_entry_count(), 0);
    let before = qa.text();
    qa.redo();
    assert_eq!(qa.text(), before);
}
