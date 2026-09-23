use super::*;
use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

use super::super::super::EditorVimLastChange;
use super::super::super::state::{vim_clear_marks, vim_forget_marks_for_buffer, vim_jump_to_mark};

const YANK_REGISTER_INDEX: usize = 27;
const SMALL_DELETE_REGISTER_INDEX: usize = 37;

struct Vm {
    buffer: TextBuffer,
    mode: EditorVimMode,
    pending: Option<EditorVimPendingKey>,
    last_char_find: Option<EditorVimCharFind>,
    unnamed: Option<EditorVimRegister>,
    last_change: Option<EditorVimLastChange>,
}

impl Vm {
    fn new(id: u64, text: &str) -> Self {
        vim_clear_named_registers();
        Self {
            buffer: TextBuffer::from_text(id, None, text.to_owned()),
            mode: EditorVimMode::Normal,
            pending: None,
            last_char_find: None,
            unnamed: None,
            last_change: None,
        }
    }

    fn with_unnamed(id: u64, text: &str, register: EditorVimRegister) -> Self {
        let mut vm = Self::new(id, text);
        vm.unnamed = Some(register);
        vm
    }

    fn keys(&mut self, keys: &[(Key, Modifiers)]) {
        for (key, modifiers) in keys {
            handle_vim_editor_key_event_with_state(
                &mut self.buffer,
                *key,
                *modifiers,
                &mut self.mode,
                &mut self.pending,
                &mut self.last_char_find,
                &mut self.unnamed,
            );
        }
    }

    fn keys_with_indent(&mut self, keys: &[(Key, Modifiers)], indent_unit: &str) {
        for (key, modifiers) in keys {
            handle_vim_editor_key_event_with_state_and_indent(
                &mut self.buffer,
                *key,
                *modifiers,
                &mut self.mode,
                &mut self.pending,
                &mut self.last_char_find,
                &mut self.unnamed,
                &mut self.last_change,
                indent_unit,
            );
        }
    }

    fn set_cursor(&mut self, line: usize, column: usize) {
        let cursor = self.buffer.line_column_to_char(line, column);
        self.buffer.set_single_cursor(cursor);
    }

    fn cursor(&self) -> (usize, usize) {
        let position = self.buffer.cursor_position();
        (position.line, position.column)
    }

    fn char_cursor(&self) -> usize {
        self.buffer.cursor()
    }
}

fn register_at(index: usize) -> Option<EditorVimRegister> {
    vim_named_register(EditorVimNamedRegister {
        index,
        append: false,
    })
}

fn charwise(text: &str) -> EditorVimRegister {
    EditorVimRegister {
        text: text.to_owned(),
        kind: EditorVimRegisterKind::Characterwise,
    }
}

fn linewise(text: &str) -> EditorVimRegister {
    EditorVimRegister {
        text: text.to_owned(),
        kind: EditorVimRegisterKind::Linewise,
    }
}

const NONE: Modifiers = Modifiers::NONE;
const SHIFT: Modifiers = Modifiers::SHIFT;

#[test]
fn a_yank_never_shifts_the_numbered_stack() {
    let mut vm = Vm::new(7001, "one\ntwo\nthree\n");
    vm.keys(&[
        (Key::Y, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
        (Key::J, NONE),
        (Key::Y, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
        (Key::J, NONE),
        (Key::Y, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
    ]);
    assert_eq!(
        register_at(YANK_REGISTER_INDEX).map(|r| r.text),
        Some("three".to_owned())
    );
    assert_eq!(
        register_at(28).map(|r| r.text),
        None,
        "a yank must not fill \"1"
    );
    assert_eq!(register_at(29), None);
}

#[test]
fn a_small_delete_then_linewise_delete_keeps_numbered_stack_clean() {
    let mut vm = Vm::new(7002, "alpha\nbeta\n");
    vm.keys(&[(Key::X, NONE), (Key::D, NONE), (Key::D, NONE)]);
    assert_eq!(
        register_at(28).map(|r| r.text),
        Some("lpha\n".to_owned()),
        "\"1 must hold only the linewise delete"
    );
    assert_eq!(register_at(29), None);
    assert_eq!(
        register_at(SMALL_DELETE_REGISTER_INDEX).map(|r| r.text),
        Some("a".to_owned()),
        "\"- must survive the linewise delete"
    );
}

#[test]
fn a_blackhole_delete_records_nothing_anywhere() {
    let mut vm = Vm::with_unnamed(7003, "alpha\nbeta\n", charwise("SENTINEL"));
    vm.keys(&[
        (Key::Quote, SHIFT),
        (Key::Minus, SHIFT),
        (Key::D, NONE),
        (Key::D, NONE),
    ]);
    assert_eq!(vm.buffer.text(), "beta\n");
    assert_eq!(
        vm.unnamed.as_ref().map(|r| r.text.as_str()),
        Some("SENTINEL"),
        "\"_dd must not touch the unnamed register"
    );
    assert_eq!(register_at(28), None, "\"_dd must not fill \"1");
    assert_eq!(
        register_at(YANK_REGISTER_INDEX),
        None,
        "\"_dd must not touch \"0"
    );

    vm.set_cursor(0, 0);
    vm.keys(&[(Key::Quote, SHIFT), (Key::Minus, SHIFT), (Key::X, NONE)]);
    assert_eq!(vm.buffer.text(), "eta\n");
    assert_eq!(
        vm.unnamed.as_ref().map(|r| r.text.as_str()),
        Some("SENTINEL")
    );
    assert_eq!(register_at(SMALL_DELETE_REGISTER_INDEX), None);
}

#[test]
fn a_blackhole_put_is_a_clean_noop() {
    let mut vm = Vm::new(7004, "alpha\n");
    vm.keys(&[
        (Key::Y, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
        (Key::Quote, SHIFT),
        (Key::Minus, SHIFT),
        (Key::P, NONE),
    ]);
    assert_eq!(vm.buffer.text(), "alpha\n");
    assert!(vm.pending.is_none());
}

#[test]
fn a_explicit_register_delete_bypasses_zero_but_fills_unnamed_and_shifts_stack() {
    let mut vm = Vm::new(7005, "one\ntwo\nthree\n");
    vm.keys(&[
        (Key::Quote, SHIFT),
        (Key::A, NONE),
        (Key::D, NONE),
        (Key::D, NONE),
    ]);
    assert_eq!(vm.buffer.text(), "two\nthree\n");
    assert_eq!(
        register_at(0).map(|r| (r.text.clone(), r.kind)),
        Some(("one\n".to_owned(), EditorVimRegisterKind::Linewise))
    );
    assert_eq!(
        register_at(YANK_REGISTER_INDEX),
        None,
        "\"add must not fill \"0"
    );
    assert_eq!(
        register_at(28).map(|r| r.text),
        Some("one\n".to_owned()),
        "\"add must fill \"1 like every whole-line delete"
    );
    assert_eq!(
        vm.unnamed.as_ref().map(|r| r.text.as_str()),
        Some("one\n"),
        "unnamed still receives the deleted text"
    );
    assert_eq!(
        vm.unnamed.map(|r| r.kind),
        Some(EditorVimRegisterKind::Linewise)
    );
}

#[test]
fn a_explicit_register_yank_does_not_fill_zero() {
    let mut vm = Vm::new(7006, "one two\n");
    vm.keys(&[
        (Key::Quote, SHIFT),
        (Key::A, NONE),
        (Key::Y, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
    ]);
    assert_eq!(register_at(0).map(|r| r.text), Some("one".to_owned()));
    assert_eq!(
        register_at(YANK_REGISTER_INDEX),
        None,
        "\"ayiw must not fill \"0"
    );
}

#[test]
fn a_uppercase_append_merges_text_and_kinds() {
    let mut vm = Vm::new(7007, "one two\nl1\nl2\n");
    vm.keys(&[
        (Key::Quote, SHIFT),
        (Key::A, NONE),
        (Key::Y, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
        (Key::W, NONE),
        (Key::Quote, SHIFT),
        (Key::A, SHIFT),
        (Key::Y, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
    ]);
    assert_eq!(
        register_at(0).map(|r| (r.text.clone(), r.kind)),
        Some(("onetwo".to_owned(), EditorVimRegisterKind::Characterwise))
    );

    vm.set_cursor(2, 0);
    vm.keys(&[(Key::Quote, SHIFT), (Key::A, SHIFT), (Key::Y, SHIFT)]);
    assert_eq!(
        register_at(0).map(|r| (r.text.clone(), r.kind)),
        Some(("onetwo\nl2\n".to_owned(), EditorVimRegisterKind::Linewise)),
        "appending linewise text onto charwise text inserts a line break \
         between them and turns the register linewise"
    );
}

#[test]
fn a_append_into_virgin_register_stores() {
    let mut vm = Vm::new(7008, "one\n");
    vm.keys(&[
        (Key::Quote, SHIFT),
        (Key::A, SHIFT),
        (Key::Y, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
    ]);
    assert_eq!(register_at(0).map(|r| r.text), Some("one".to_owned()));
}

#[test]
fn a_quote_zero_after_change_still_holds_the_yank() {
    let mut vm = Vm::new(7009, "alpha beta\n");
    vm.keys(&[(Key::Y, NONE), (Key::I, NONE), (Key::W, NONE)]);
    assert_eq!(
        register_at(YANK_REGISTER_INDEX).map(|r| r.text),
        Some("alpha".to_owned())
    );

    vm.set_cursor(0, 6);
    vm.keys(&[
        (Key::C, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
        (Key::Escape, NONE),
    ]);
    assert_eq!(vm.buffer.text(), "alpha \n");
    assert_eq!(
        register_at(YANK_REGISTER_INDEX).map(|r| r.text),
        Some("alpha".to_owned()),
        "ciw must not overwrite \"0"
    );
    assert_eq!(
        vm.unnamed.as_ref().map(|r| r.text.as_str()),
        Some("beta"),
        "the changed text goes to unnamed"
    );
    assert_eq!(
        register_at(SMALL_DELETE_REGISTER_INDEX).map(|r| r.text),
        Some("beta".to_owned()),
        "small change also fills \"- like Vim"
    );
}

#[test]
fn a_unsupported_register_cancels_and_next_key_is_fresh() {
    let mut vm = Vm::new(7010, "beta\n");

    vm.keys(&[(Key::Quote, SHIFT), (Key::Comma, NONE), (Key::X, NONE)]);
    assert_eq!(vm.buffer.text(), "eta\n");
    assert_eq!(vm.unnamed.as_ref().map(|r| r.text.as_str()), Some("b"));
    assert!(vm.pending.is_none());
}

#[test]
fn a_count_before_and_after_register_prefix_both_work() {
    for (id, keys) in [
        (
            7011u64,
            vec![
                (Key::Num2, NONE),
                (Key::Quote, SHIFT),
                (Key::A, NONE),
                (Key::D, NONE),
                (Key::D, NONE),
            ],
        ),
        (
            7012,
            vec![
                (Key::Quote, SHIFT),
                (Key::A, NONE),
                (Key::Num2, NONE),
                (Key::D, NONE),
                (Key::D, NONE),
            ],
        ),
    ] {
        let mut vm = Vm::new(id, "one\ntwo\nthree\n");
        vm.keys(&keys);
        assert_eq!(vm.buffer.text(), "three\n", "case {id}");
        assert_eq!(
            register_at(0).map(|r| r.text),
            Some("one\ntwo\n".to_owned()),
            "\"a must hold both deleted lines (case {id})"
        );
        assert!(vm.pending.is_none());
    }
}

#[test]
fn a_two_digit_count_after_register() {
    let text = (0..13)
        .map(|i| format!("l{i}"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let mut vm = Vm::new(7013, &text);
    vm.keys(&[
        (Key::Quote, SHIFT),
        (Key::A, NONE),
        (Key::Num1, NONE),
        (Key::Num2, NONE),
        (Key::D, NONE),
        (Key::D, NONE),
    ]);
    assert_eq!(vm.buffer.text(), "l12\n");
    let deleted = (0..12)
        .map(|i| format!("l{i}"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_eq!(register_at(0).map(|r| r.text), Some(deleted));
}

#[test]
fn a_yw_at_last_word_of_buffer() {
    let mut vm = Vm::new(7014, "foo bar");
    vm.set_cursor(0, 4);
    vm.keys(&[(Key::Y, NONE), (Key::W, NONE)]);
    assert_eq!(
        vm.unnamed.as_ref().map(|r| (r.text.clone(), r.kind)),
        Some(("bar".to_owned(), EditorVimRegisterKind::Characterwise)),
        "yw at the buffer end must not include a newline"
    );

    vm.keys(&[(Key::P, NONE)]);
    assert_eq!(vm.buffer.text(), "foo bbarar");
}

#[test]
fn b_x_fills_unnamed_small_delete_and_leaves_zero_alone() {
    let mut vm = Vm::new(7100, "abc\n");
    vm.keys(&[(Key::Y, NONE), (Key::I, NONE), (Key::W, NONE)]);
    vm.set_cursor(0, 0);
    vm.keys(&[(Key::X, NONE)]);
    assert_eq!(vm.buffer.text(), "bc\n");
    assert_eq!(
        vm.unnamed.as_ref().map(|r| (r.text.clone(), r.kind)),
        Some(("a".to_owned(), EditorVimRegisterKind::Characterwise))
    );
    assert_eq!(
        register_at(SMALL_DELETE_REGISTER_INDEX).map(|r| r.text),
        Some("a".to_owned())
    );
    assert_eq!(
        register_at(YANK_REGISTER_INDEX).map(|r| r.text),
        Some("abc".to_owned()),
        "x must not touch \"0"
    );
    assert_eq!(register_at(28), None, "x must not shift \"1");
}

#[test]
fn b_x_count_clamps_at_the_line_end() {
    let mut vm = Vm::new(7101, "ab\ncd\n");
    vm.set_cursor(0, 0);
    vm.keys(&[(Key::Num5, NONE), (Key::X, NONE)]);
    eprintln!(
        "DIAG x-eol: text={:?} unnamed={:?} reg1={:?} small={:?}",
        vm.buffer.text(),
        vm.unnamed.as_ref().map(|r| (r.text.clone(), r.kind)),
        register_at(28).map(|r| (r.text.clone(), r.kind)),
        register_at(SMALL_DELETE_REGISTER_INDEX).map(|r| (r.text.clone(), r.kind)),
    );
    assert_eq!(vm.buffer.text(), "\ncd\n", "5x must stop at the line end");
    assert_eq!(
        vm.unnamed.as_ref().map(|r| (r.text.clone(), r.kind)),
        Some(("ab".to_owned(), EditorVimRegisterKind::Characterwise)),
        "the register must hold the two deleted chars as a small delete"
    );
    assert_eq!(
        register_at(SMALL_DELETE_REGISTER_INDEX).map(|r| r.text),
        Some("ab".to_owned())
    );
    assert_eq!(
        register_at(28),
        None,
        "a single-line 5x must not fill \"1 (actual \"1: {:?})",
        register_at(28).map(|r| r.text)
    );
}

#[test]
fn b_x_count_at_last_char_clamps_at_the_line_end() {
    let mut vm = Vm::new(7102, "ab\ncd\n");
    vm.set_cursor(0, 1);
    vm.keys(&[(Key::Num5, NONE), (Key::X, NONE)]);
    eprintln!(
        "DIAG x-at-eol-char: text={:?} unnamed={:?} reg1={:?}",
        vm.buffer.text(),
        vm.unnamed.as_ref().map(|r| (r.text.clone(), r.kind)),
        register_at(28).map(|r| (r.text.clone(), r.kind)),
    );
    assert_eq!(vm.buffer.text(), "a\ncd\n", "5x must not eat the newline");
}

#[test]
fn b_shift_x_count_clamps_at_the_line_start() {
    let mut vm = Vm::new(7103, "ab\ncd\n");
    vm.set_cursor(1, 0);
    vm.keys(&[(Key::Num5, NONE), (Key::X, SHIFT)]);
    eprintln!(
        "DIAG x-at-bol: text={:?} unnamed={:?} reg1={:?}",
        vm.buffer.text(),
        vm.unnamed.as_ref().map(|r| (r.text.clone(), r.kind)),
        register_at(28).map(|r| (r.text.clone(), r.kind)),
    );
    assert_eq!(vm.buffer.text(), "ab\ncd\n", "5X at BOL must be a no-op");
    assert_eq!(
        vm.unnamed.as_ref().map(|r| r.text.as_str()),
        None,
        "a failed X must not touch the unnamed register"
    );

    let mut vm = Vm::new(7104, "ab\ncd\n");
    vm.set_cursor(1, 1);
    vm.keys(&[(Key::Num5, NONE), (Key::X, SHIFT)]);
    assert_eq!(
        vm.buffer.text(),
        "ab\nd\n",
        "5X must stop at the line start (actual: {:?})",
        vm.buffer.text()
    );
    assert_eq!(vm.unnamed.as_ref().map(|r| r.text.as_str()), Some("c"));
}

#[test]
fn b_shift_d_single_line_goes_to_small_delete_and_unnamed() {
    let mut vm = Vm::new(7105, "hello world\n");
    vm.set_cursor(0, 5);
    vm.keys(&[(Key::D, SHIFT)]);
    assert_eq!(vm.buffer.text(), "hello\n");
    assert_eq!(
        vm.unnamed.as_ref().map(|r| (r.text.clone(), r.kind)),
        Some((" world".to_owned(), EditorVimRegisterKind::Characterwise))
    );
    assert_eq!(
        register_at(SMALL_DELETE_REGISTER_INDEX).map(|r| r.text),
        Some(" world".to_owned())
    );
    assert_eq!(register_at(28), None, "single-line D must not shift \"1");
}

#[test]
fn b_shift_d_on_empty_line_is_a_noop() {
    let mut vm = Vm::with_unnamed(7106, "one\n\ntwo\n", charwise("SENTINEL"));
    vm.set_cursor(1, 0);
    vm.keys(&[(Key::D, SHIFT)]);
    assert_eq!(vm.buffer.text(), "one\n\ntwo\n");
    assert_eq!(
        vm.unnamed.as_ref().map(|r| r.text.as_str()),
        Some("SENTINEL"),
        "D on an empty line must not touch the unnamed register"
    );
}

#[test]
fn b_shift_d_counted_spans_lines_and_shifts_stack() {
    let mut vm = Vm::new(7107, "one\ntwo\nthree\n");
    vm.set_cursor(1, 1);
    vm.keys(&[(Key::Num2, NONE), (Key::D, SHIFT)]);
    assert_eq!(vm.buffer.text(), "one\nt\n");
    assert_eq!(
        register_at(28).map(|r| r.text),
        Some("wo\nthree".to_owned()),
        "2D spanning two lines must land in \"1"
    );
}

#[test]
fn b_shift_c_fills_unnamed_and_small_delete() {
    let mut vm = Vm::new(7108, "hello world\n");
    vm.set_cursor(0, 5);
    vm.keys(&[(Key::C, SHIFT), (Key::Escape, NONE)]);
    assert_eq!(vm.buffer.text(), "hello\n");
    assert_eq!(
        vm.unnamed.as_ref().map(|r| r.text.as_str()),
        Some(" world"),
        "C records the replaced text in unnamed"
    );
    assert_eq!(
        register_at(YANK_REGISTER_INDEX),
        None,
        "C must not fill \"0"
    );
}

#[test]
fn b_s_fills_unnamed_and_small_delete() {
    let mut vm = Vm::new(7109, "abc\n");
    vm.keys(&[(Key::S, NONE)]);
    assert_eq!(vm.mode, EditorVimMode::Insert, "s must enter insert mode");
    vm.keys(&[(Key::Escape, NONE)]);
    assert_eq!(vm.buffer.text(), "bc\n");
    assert_eq!(vm.unnamed.as_ref().map(|r| r.text.as_str()), Some("a"));
    assert_eq!(
        register_at(SMALL_DELETE_REGISTER_INDEX).map(|r| r.text),
        Some("a".to_owned())
    );
    assert_eq!(
        register_at(YANK_REGISTER_INDEX),
        None,
        "s must not fill \"0"
    );
}

#[test]
fn b_yy_on_last_line_without_trailing_newline_still_linewise() {
    let mut vm = Vm::new(7110, "alpha\nbeta");
    vm.set_cursor(1, 0);
    vm.keys(&[(Key::Y, SHIFT)]);
    assert_eq!(
        vm.unnamed.as_ref().map(|r| (r.text.clone(), r.kind)),
        Some(("beta\n".to_owned(), EditorVimRegisterKind::Linewise)),
        "yy must normalize the yank to a linewise register"
    );
    vm.keys(&[(Key::P, NONE)]);
    assert_eq!(vm.buffer.text(), "alpha\nbeta\nbeta\n");
}

fn set_mark_a(vm: &mut Vm, line: usize, column: usize) {
    vim_clear_marks();
    vm.set_cursor(line, column);
    vm.keys(&[(Key::M, NONE), (Key::A, NONE)]);
}

#[test]
fn c_three_opens_above_move_mark_with_content() {
    let mut vm = Vm::new(7200, "one\ntwo\nthree\n");
    set_mark_a(&mut vm, 2, 1);

    vm.set_cursor(0, 0);
    for _ in 0..3 {
        vm.keys(&[(Key::O, SHIFT), (Key::Escape, NONE)]);
        vm.set_cursor(0, 0);
    }
    assert_eq!(vm.buffer.text(), "\n\n\none\ntwo\nthree\n");

    vm.set_cursor(0, 0);
    vm.keys(&[(Key::Quote, NONE), (Key::A, NONE)]);
    assert_eq!(
        vm.cursor(),
        (5, 0),
        "'a (linewise) must land on the moved content of line 2"
    );
}

#[test]
fn c_open_on_the_marked_line_moves_the_mark_to_the_content() {
    let mut vm = Vm::new(7240, "one\ntwo\n");
    set_mark_a(&mut vm, 1, 0);

    vm.set_cursor(1, 0);
    vm.keys(&[(Key::O, SHIFT), (Key::Escape, NONE)]);
    assert_eq!(vm.buffer.text(), "one\n\ntwo\n");
    vm.set_cursor(0, 0);
    vm.keys(&[(Key::Quote, NONE), (Key::A, NONE)]);
    assert_eq!(
        vm.cursor(),
        (2, 0),
        "'a must follow its content down (actual {:?})",
        vm.cursor()
    );
}

#[test]
fn c_dd_on_marked_last_line_clears_the_mark() {
    let mut vm = Vm::new(7201, "one\ntwo\n");
    set_mark_a(&mut vm, 1, 1);

    vm.set_cursor(1, 0);
    vm.keys(&[(Key::D, NONE), (Key::D, NONE)]);
    assert_eq!(
        vm.buffer.text(),
        "one",
        "the editor's rope model drops the final line break here"
    );

    let before = vm.buffer.cursor();
    let jumped = vim_jump_to_mark(&mut vm.buffer, 'a', false);
    assert!(
        !jumped,
        "dd of the marked last line must clear the mark (actual mark position {:?})",
        vm.buffer.cursor_position()
    );
    assert_eq!(vm.buffer.cursor(), before);
}

#[test]
fn c_dd_spanning_the_marked_line_clears_it() {
    let mut vm = Vm::new(7202, "one\ntwo\nthree\nfour\n");
    set_mark_a(&mut vm, 2, 0);

    vm.set_cursor(1, 0);
    vm.keys(&[(Key::Num2, NONE), (Key::D, NONE), (Key::D, NONE)]);
    assert_eq!(vm.buffer.text(), "one\nfour\n");
    assert!(
        !vim_jump_to_mark(&mut vm.buffer, 'a', true),
        "2dd across the marked line must clear the mark"
    );
}

#[test]
fn c_indent_keeps_col0_mark_at_col0() {
    let mut vm = Vm::new(7203, "hello\n");
    set_mark_a(&mut vm, 0, 0);

    vm.keys_with_indent(&[(Key::Period, SHIFT), (Key::Period, SHIFT)], "  ");
    assert_eq!(vm.buffer.text(), "  hello\n");
    vm.set_cursor(0, 0);
    assert!(vim_jump_to_mark(&mut vm.buffer, 'a', false));
    assert_eq!(
        vm.cursor().1,
        0,
        ">> must not drag a col-0 mark into the indent"
    );
}

#[test]
fn c_indent_shifts_marks_past_the_indent() {
    let mut vm = Vm::new(7204, "hello\n");
    set_mark_a(&mut vm, 0, 2);

    vm.keys_with_indent(&[(Key::Period, SHIFT), (Key::Period, SHIFT)], "  ");
    assert_eq!(vm.buffer.text(), "  hello\n");
    vm.set_cursor(0, 0);
    assert!(vim_jump_to_mark(&mut vm.buffer, 'a', false));
    assert_eq!(
        vm.buffer.cursor_position().column,
        4,
        "2 + indent width (actual {:?})",
        vm.buffer.cursor_position()
    );
}

#[test]
fn c_linewise_put_above_shifts_marks() {
    let mut vm = Vm::new(7205, "one\ntwo\nthree\n");
    set_mark_a(&mut vm, 2, 0);

    vm.set_cursor(2, 0);
    vm.unnamed = Some(linewise("x\ny\n"));
    vm.keys(&[(Key::P, SHIFT)]);
    assert_eq!(vm.buffer.text(), "one\ntwo\nx\ny\nthree\n");

    vm.set_cursor(0, 0);
    vm.keys(&[(Key::Quote, NONE), (Key::A, NONE)]);
    eprintln!("DIAG put-above-mark: cursor-after-jump={:?}", vm.cursor());
    assert_eq!(vm.cursor(), (4, 0), "'a must follow its line down by two");
}

#[test]
fn c_undo_does_not_restore_marks_documented() {
    let mut vm = Vm::new(7206, "one\ntwo\nthree\n");
    set_mark_a(&mut vm, 1, 0);

    vm.set_cursor(0, 0);
    vm.keys(&[(Key::O, NONE), (Key::Escape, NONE)]);
    vm.keys(&[(Key::O, NONE), (Key::Escape, NONE)]);
    assert_eq!(vm.buffer.text(), "one\n\n\ntwo\nthree\n");
    vm.set_cursor(0, 0);
    vm.keys(&[(Key::Quote, NONE), (Key::A, NONE)]);
    assert_eq!(vm.cursor(), (3, 0), "mark rides the inserts");

    vm.keys(&[(Key::U, NONE), (Key::U, NONE)]);
    assert_eq!(vm.buffer.text(), "one\ntwo\nthree\n");
    vm.set_cursor(0, 0);
    vm.keys(&[(Key::Quote, NONE), (Key::A, NONE)]);

    assert_eq!(
        vm.cursor(),
        (3, 0),
        "actual behavior: undo leaves the mark stale; Vim would restore line 1"
    );
}

#[test]
fn c_marks_survive_close_and_are_purged_for_reopened_id() {
    vim_clear_named_registers();
    vim_clear_marks();
    let mut first = TextBuffer::from_text(7207, None, "alpha\nbeta\n".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending = None;
    let mut last_char_find = None;
    let mut unnamed = None;
    first.set_single_cursor(first.line_column_to_char(1, 0));
    for (key, modifiers) in [(Key::M, NONE), (Key::A, NONE)] {
        handle_vim_editor_key_event_with_state(
            &mut first,
            key,
            modifiers,
            &mut mode,
            &mut pending,
            &mut last_char_find,
            &mut unnamed,
        );
    }

    vim_forget_marks_for_buffer(7207);
    let mut reopened = TextBuffer::from_text(7207, None, "alpha\nbeta\n".to_owned());
    assert!(
        !vim_jump_to_mark(&mut reopened, 'a', true),
        "a closed buffer's marks must not leak into a reopened buffer with the same id"
    );
}

#[test]
fn c_marks_are_isolated_across_two_buffers() {
    vim_clear_named_registers();
    let mut a = Vm::new(7208, "l0\nl1\nl2\n");
    let mut b = Vm::new(7209, "m0\nm1\nm2\n");
    vim_clear_marks();
    a.set_cursor(2, 0);
    a.keys(&[(Key::M, NONE), (Key::A, NONE)]);
    b.set_cursor(0, 0);
    b.keys(&[(Key::M, NONE), (Key::A, NONE)]);

    assert!(vim_jump_to_mark(&mut a.buffer, 'a', true));
    assert_eq!(a.cursor().0, 2);
    assert!(vim_jump_to_mark(&mut b.buffer, 'a', true));
    assert_eq!(b.cursor().0, 0, "B's 'a must be B's own mark");
}

#[test]
fn c_jump_to_mark_then_counted_dd_interplay() {
    let mut vm = Vm::new(7210, "one\ntwo\nthree\nfour\n");
    set_mark_a(&mut vm, 2, 0);

    vm.set_cursor(0, 0);
    vm.keys(&[(Key::Quote, NONE), (Key::A, NONE)]);
    assert_eq!(vm.cursor().0, 2);
    vm.keys(&[(Key::Num3, NONE), (Key::D, NONE), (Key::D, NONE)]);
    assert_eq!(
        vm.buffer.text(),
        "one\ntwo",
        "3dd from the mark clamps to EOF"
    );
    assert!(
        !vim_jump_to_mark(&mut vm.buffer, 'a', true),
        "the marked line was deleted; 'a must fail"
    );
    assert_eq!(
        register_at(28).map(|r| r.text),
        Some("three\nfour\n".to_owned())
    );
}

#[test]
fn d_viwp_toggles_two_words_repeatedly() {
    let mut vm = Vm::new(7300, "one two\n");
    vm.set_cursor(0, 4);
    vm.keys(&[(Key::Y, NONE), (Key::I, NONE), (Key::W, NONE)]);

    vm.set_cursor(0, 0);
    vm.keys(&[
        (Key::V, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
        (Key::P, NONE),
    ]);
    assert_eq!(vm.buffer.text(), "two two\n");
    assert_eq!(
        vm.unnamed.as_ref().map(|r| r.text.as_str()),
        Some("one"),
        "replaced text lands in unnamed"
    );

    vm.keys(&[
        (Key::V, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
        (Key::P, NONE),
    ]);
    assert_eq!(vm.buffer.text(), "one two\n");

    vm.keys(&[
        (Key::V, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
        (Key::P, NONE),
    ]);
    assert_eq!(vm.buffer.text(), "two two\n");
}

#[test]
fn d_visual_p_where_pasted_text_equals_selection() {
    let mut vm = Vm::new(7301, "abc def\n");
    vm.set_cursor(0, 0);
    vm.keys(&[(Key::Y, NONE), (Key::I, NONE), (Key::W, NONE)]);
    vm.keys(&[
        (Key::V, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
        (Key::P, NONE),
    ]);
    assert_eq!(vm.buffer.text(), "abc def\n");
    assert_eq!(vm.unnamed.as_ref().map(|r| r.text.as_str()), Some("abc"));
}

#[test]
fn d_visual_p_with_count_pastes_count_copies() {
    let mut vm = Vm::with_unnamed(7302, "one two\n", charwise("ZZ"));
    vm.keys(&[
        (Key::V, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
        (Key::Num3, NONE),
        (Key::P, NONE),
    ]);
    assert_eq!(
        vm.buffer.text(),
        "ZZZZZZ two\n",
        "{{Visual}}3p must paste the register 3 times (actual {:?})",
        vm.buffer.text()
    );
}

#[test]
fn d_visual_register_prefix_put_pastes_that_register() {
    let mut vm = Vm::new(7303, "one two\n");
    vm.keys(&[
        (Key::Quote, SHIFT),
        (Key::A, NONE),
        (Key::Y, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
    ]);

    vm.set_cursor(0, 4);
    vm.keys(&[
        (Key::V, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
        (Key::Quote, SHIFT),
        (Key::A, NONE),
        (Key::P, NONE),
    ]);
    assert_eq!(
        vm.buffer.text(),
        "one one\n",
        "\"a p in visual mode must paste register a over the selection (actual {:?})",
        vm.buffer.text()
    );
}

#[test]
fn d_visual_p_with_empty_register_is_a_clean_noop() {
    let mut vm = Vm::new(7304, "one two\n");
    vm.keys(&[
        (Key::V, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
        (Key::P, NONE),
    ]);
    assert_eq!(
        vm.buffer.text(),
        "one two\n",
        "p with no register must not change text"
    );
    assert!(
        vm.pending.is_none(),
        "the visual selection must collapse cleanly"
    );
}

#[test]
fn d_quote_minus_put_when_small_delete_register_is_empty() {
    let mut vm = Vm::new(7305, "one two\n");
    vm.keys(&[(Key::Quote, SHIFT), (Key::Minus, NONE), (Key::P, NONE)]);
    assert_eq!(vm.buffer.text(), "one two\n");
    assert!(vm.pending.is_none());
}

#[test]
fn d_visual_line_put_with_linewise_register() {
    let mut vm = Vm::with_unnamed(7306, "one\ntwo\nthree\n", linewise("L1\nL2\n"));
    vm.set_cursor(1, 0);
    vm.keys(&[(Key::V, SHIFT), (Key::P, NONE)]);
    assert_eq!(vm.buffer.text(), "one\nL1\nL2\nthree\n");
    assert_eq!(
        vm.unnamed.as_ref().map(|r| (r.text.clone(), r.kind)),
        Some(("two\n".to_owned(), EditorVimRegisterKind::Linewise)),
        "Vp records the replaced lines"
    );
    assert_eq!(
        vm.cursor(),
        (1, 0),
        "cursor on the first non-blank of the first pasted line"
    );
}

#[test]
fn d_visual_line_put_with_charwise_register_keeps_lines() {
    let mut vm = Vm::with_unnamed(7307, "one\ntwo\nthree\n", charwise("XYZ"));
    vm.set_cursor(1, 0);
    vm.keys(&[(Key::V, SHIFT), (Key::P, NONE)]);
    assert_eq!(
        vm.buffer.text(),
        "one\nXYZ\nthree\n",
        "Vp with a charwise register must not join the surrounding lines (actual {:?})",
        vm.buffer.text()
    );
}

#[test]
fn d_visual_char_put_with_linewise_register() {
    let mut vm = Vm::with_unnamed(7308, "one two\n", linewise("L1\nL2\n"));
    vm.keys(&[
        (Key::V, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
        (Key::P, NONE),
    ]);
    assert_eq!(vm.buffer.text(), "L1\nL2\n two\n");
}

#[test]
fn d_visual_shift_p_behaves_like_p() {
    let mut vm = Vm::with_unnamed(7309, "one two\n", charwise("ZZ"));
    vm.keys(&[
        (Key::V, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
        (Key::P, SHIFT),
    ]);
    assert_eq!(vm.buffer.text(), "ZZ two\n");
    assert_eq!(vm.unnamed.as_ref().map(|r| r.text.as_str()), Some("one"));
}

#[test]
fn e_p_multi_line_cursor_on_first_pasted_line_first_non_blank() {
    let mut vm = Vm::with_unnamed(7400, "a\nb\nc\n", linewise("  x\n  y\n"));
    vm.keys(&[(Key::P, NONE)]);
    assert_eq!(vm.buffer.text(), "a\n  x\n  y\nb\nc\n");
    assert_eq!(
        vm.cursor(),
        (1, 2),
        "p lands on the first pasted line's first non-blank"
    );
}

#[test]
fn e_shift_p_multi_line_cursor_on_first_pasted_line_first_non_blank() {
    let mut vm = Vm::with_unnamed(7401, "a\nb\n", linewise("  x\n  y\n"));
    vm.keys(&[(Key::P, SHIFT)]);
    assert_eq!(vm.buffer.text(), "  x\n  y\na\nb\n");
    assert_eq!(vm.cursor(), (0, 2), "P lands on the first pasted line");
}

#[test]
fn e_linewise_put_into_empty_buffer() {
    let mut vm = Vm::with_unnamed(7402, "", linewise("x\n"));
    vm.keys(&[(Key::P, NONE)]);
    assert_eq!(vm.buffer.text(), "x\n");
}

#[test]
fn e_charwise_put_into_empty_buffer() {
    let mut vm = Vm::with_unnamed(7403, "", charwise("x"));
    vm.keys(&[(Key::P, NONE)]);
    assert_eq!(vm.buffer.text(), "x");
    assert_eq!(vm.char_cursor(), 0);
}

#[test]
fn e_put_after_last_line_without_trailing_newline() {
    let mut vm = Vm::with_unnamed(7404, "a\nb\nc", linewise("x\n"));
    vm.set_cursor(2, 0);
    vm.keys(&[(Key::P, NONE)]);
    assert_eq!(vm.buffer.text(), "a\nb\nc\nx\n");
    assert_eq!(
        vm.cursor(),
        (3, 0),
        "cursor on the pasted line's first non-blank"
    );
}

#[test]
fn e_pasted_line_count_cursor_with_count() {
    let mut vm = Vm::with_unnamed(7405, "a\nb\n", linewise("  x\n"));
    vm.keys(&[(Key::Num2, NONE), (Key::P, NONE)]);
    assert_eq!(vm.buffer.text(), "a\n  x\n  x\nb\n");
    assert_eq!(vm.cursor(), (1, 2), "2p lands on the first pasted copy");
}

#[test]
fn e_charwise_count_put_cursor_on_last_pasted_char() {
    let mut vm = Vm::with_unnamed(7406, "abc\n", charwise("xy"));
    vm.keys(&[(Key::Num3, NONE), (Key::P, NONE)]);
    assert_eq!(vm.buffer.text(), "axyxyxybc\n");
    assert_eq!(
        vm.char_cursor(),
        6,
        "charwise p leaves the cursor on the last pasted char"
    );
}

#[test]
fn f_vjd_over_blank_line_keeps_the_blank_line() {
    let mut vm = Vm::new(7500, "one\n\ntwo\n");
    vm.keys(&[(Key::V, NONE), (Key::J, NONE), (Key::D, NONE)]);
    assert_eq!(vm.buffer.text(), "\ntwo\n");
}

#[test]
fn f_v_dollar_d_deletes_whole_flag_cluster() {
    let mut vm = Vm::new(7501, "ab\u{1F1FA}\u{1F1F8}cd\n");
    vm.set_cursor(0, 2);
    vm.keys(&[(Key::V, NONE), (Key::Num4, SHIFT), (Key::D, NONE)]);
    assert_eq!(
        vm.buffer.text(),
        "ab\n",
        "v$d must delete the whole flag cluster plus the rest, no dangling scalars"
    );
}

#[test]
fn f_v_l_d_steps_whole_flag_cluster() {
    let mut vm = Vm::new(7502, "ab\u{1F1FA}\u{1F1F8}cd\n");
    vm.set_cursor(0, 2);
    vm.keys(&[(Key::V, NONE), (Key::L, NONE), (Key::D, NONE)]);
    assert_eq!(
        vm.buffer.text(),
        "abd\n",
        "v l selects the flag cluster plus 'c' (cluster-aware motion), then d"
    );
}

#[test]
fn f_v_l_d_steps_whole_zwj_family() {
    let family = "a\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}b\n";
    let mut vm = Vm::new(7503, family);
    vm.set_cursor(0, 1);
    vm.keys(&[(Key::V, NONE), (Key::L, NONE), (Key::D, NONE)]);
    assert_eq!(
        vm.buffer.text(),
        "a\n",
        "v l d must remove the whole ZWJ family plus 'b', no dangling ZWJ (actual {:?})",
        vm.buffer.text()
    );
}

#[test]
fn f_visual_upper_expands_sharp_s_and_keeps_accents() {
    let mut vm = Vm::new(7504, "x stra\u{00DF}e\n");
    vm.set_cursor(0, 2);
    vm.keys(&[(Key::V, NONE), (Key::E, NONE), (Key::U, SHIFT)]);
    assert_eq!(
        vm.buffer.text(),
        "x STRASSE\n",
        "gU must expand \u{00DF} to SS without splitting clusters"
    );
}

#[test]
fn f_visual_lower_accents() {
    let mut vm = Vm::new(7505, "x \u{00DC}N\u{00CF}CODE\n");
    vm.set_cursor(0, 2);
    vm.keys(&[(Key::V, NONE), (Key::E, NONE), (Key::U, NONE)]);
    assert_eq!(vm.buffer.text(), "x \u{00FC}n\u{00EF}code\n");
}

#[test]
fn f_visual_lower_turkish_dotted_capital_no_panic_no_split() {
    let mut vm = Vm::new(7506, "x \u{0130}f\n");
    vm.set_cursor(0, 2);
    vm.keys(&[(Key::V, NONE), (Key::E, NONE), (Key::U, NONE)]);
    assert_eq!(
        vm.buffer.text(),
        "x i\u{307}f\n",
        "locale-free lowercase of \u{0130}"
    );
}

#[test]
fn g_dj_on_last_line_is_a_full_noop() {
    let mut vm = Vm::with_unnamed(7600, "one\ntwo\n", charwise("SENTINEL"));
    vm.set_cursor(1, 0);
    vm.keys(&[(Key::D, NONE), (Key::J, NONE)]);
    assert_eq!(vm.buffer.text(), "one\ntwo\n");
    assert_eq!(
        vm.unnamed.as_ref().map(|r| r.text.as_str()),
        Some("SENTINEL"),
        "dj on the last line must not touch the unnamed register"
    );
    assert_eq!(
        register_at(28),
        None,
        "dj on the last line must not fill \"1"
    );
    assert!(vm.pending.is_none());
}

#[test]
fn g_dk_on_first_line_is_a_full_noop() {
    let mut vm = Vm::with_unnamed(7601, "one\ntwo\n", charwise("SENTINEL"));
    vm.keys(&[(Key::D, NONE), (Key::K, NONE)]);
    assert_eq!(vm.buffer.text(), "one\ntwo\n");
    assert_eq!(
        vm.unnamed.as_ref().map(|r| r.text.as_str()),
        Some("SENTINEL")
    );
    assert_eq!(register_at(28), None);
    assert!(vm.pending.is_none());
}

#[test]
fn g_d2j_clamps_to_the_last_line() {
    let mut vm = Vm::new(7602, "1\n2\n3\n4\n");
    vm.set_cursor(0, 0);
    vm.keys(&[(Key::D, NONE), (Key::Num2, NONE), (Key::J, NONE)]);
    assert_eq!(vm.buffer.text(), "4\n");
    assert_eq!(
        register_at(28).map(|r| r.text),
        Some("1\n2\n3\n".to_owned())
    );

    let mut vm = Vm::new(7603, "1\n2\n3\n4\n");
    vm.set_cursor(2, 0);
    vm.keys(&[(Key::D, NONE), (Key::Num5, NONE), (Key::J, NONE)]);
    assert_eq!(vm.buffer.text(), "1\n2");
    assert_eq!(register_at(28).map(|r| r.text), Some("3\n4\n".to_owned()));
}

#[test]
fn g_dj_one_before_last_line_deletes_two_lines() {
    let mut vm = Vm::new(7604, "one\ntwo\nthree\n");
    vm.set_cursor(1, 0);
    vm.keys(&[(Key::D, NONE), (Key::J, NONE)]);
    assert_eq!(vm.buffer.text(), "one");
    assert_eq!(
        register_at(28).map(|r| r.text),
        Some("two\nthree\n".to_owned())
    );
}
