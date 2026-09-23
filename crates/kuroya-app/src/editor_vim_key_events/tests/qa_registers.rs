use super::*;
use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

use super::super::state::vim_registers_summary;

const ZERO_REGISTER_INDEX: usize = 27;
const SMALL_DELETE_REGISTER_INDEX: usize = 37;
const CLIPBOARD_REGISTER_INDEX: usize = 38;

const NONE: Modifiers = Modifiers::NONE;
const SHIFT: Modifiers = Modifiers::SHIFT;

struct Vm {
    buffer: TextBuffer,
    mode: EditorVimMode,
    pending: Option<EditorVimPendingKey>,
    last_char_find: Option<EditorVimCharFind>,
    unnamed: Option<EditorVimRegister>,
}

impl Vm {
    fn new(id: u64, text: &str) -> Self {
        vim_clear_named_registers();
        Self::without_clear(id, text)
    }

    fn without_clear(id: u64, text: &str) -> Self {
        Self {
            buffer: TextBuffer::from_text(id, None, text.to_owned()),
            mode: EditorVimMode::Normal,
            pending: None,
            last_char_find: None,
            unnamed: None,
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

fn linewise(text: &str) -> EditorVimRegister {
    EditorVimRegister {
        text: text.to_owned(),
        kind: EditorVimRegisterKind::Linewise,
    }
}

fn charwise(text: &str) -> EditorVimRegister {
    EditorVimRegister {
        text: text.to_owned(),
        kind: EditorVimRegisterKind::Characterwise,
    }
}

#[test]
fn n_ten_deletes_shift_the_stack_and_drop_the_oldest() {
    let mut vm = Vm::new(7800, "l0\nl1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nl10\n");
    let mut dds = Vec::new();
    for _ in 0..10 {
        dds.push((Key::D, NONE));
        dds.push((Key::D, NONE));
    }
    vm.keys(&dds);
    assert_eq!(vm.buffer.text(), "l10\n");
    assert_eq!(register_at(28).map(|r| r.text), Some("l9\n".to_owned()));
    assert_eq!(register_at(35).map(|r| r.text), Some("l2\n".to_owned()));
    assert_eq!(register_at(36).map(|r| r.text), Some("l1\n".to_owned()));
}

#[test]
fn n_cc_fills_the_numbered_stack_like_dd() {
    let mut vm = Vm::new(7801, "alpha\nbeta\n");
    vm.keys(&[(Key::C, NONE), (Key::C, NONE), (Key::Escape, NONE)]);
    assert_eq!(vm.buffer.text(), "\nbeta\n");
    assert_eq!(vm.mode, EditorVimMode::Normal);
    assert_eq!(
        vm.unnamed.as_ref().map(|r| (r.text.clone(), r.kind)),
        Some(("alpha\n".to_owned(), EditorVimRegisterKind::Linewise))
    );
    assert_eq!(
        register_at(28).map(|r| (r.text.clone(), r.kind)),
        Some(("alpha\n".to_owned(), EditorVimRegisterKind::Linewise))
    );
    assert_eq!(register_at(SMALL_DELETE_REGISTER_INDEX), None);
    assert_eq!(register_at(ZERO_REGISTER_INDEX), None);
}

#[test]
fn n_dw_is_a_small_delete_minus_only() {
    let mut vm = Vm::new(7802, "one two three\n");
    vm.keys(&[(Key::D, NONE), (Key::W, NONE)]);
    assert_eq!(vm.buffer.text(), "two three\n");
    assert_eq!(
        vm.unnamed.as_ref().map(|r| (r.text.clone(), r.kind)),
        Some(("one ".to_owned(), EditorVimRegisterKind::Characterwise))
    );
    assert_eq!(
        register_at(SMALL_DELETE_REGISTER_INDEX).map(|r| r.text),
        Some("one ".to_owned())
    );
    assert_eq!(register_at(ZERO_REGISTER_INDEX), None);
    assert_eq!(register_at(28), None);
}

#[test]
fn n_undo_does_not_restore_registers() {
    let mut vm = Vm::new(7803, "one\ntwo\nthree\n");
    vm.keys(&[(Key::D, NONE), (Key::D, NONE)]);
    assert_eq!(vm.buffer.text(), "two\nthree\n");
    vm.keys(&[(Key::U, NONE)]);
    assert_eq!(vm.buffer.text(), "one\ntwo\nthree\n");
    assert_eq!(register_at(28).map(|r| r.text), Some("one\n".to_owned()));
    assert_eq!(vm.unnamed.as_ref().map(|r| r.text.as_str()), Some("one\n"));
}

#[test]
fn n_dg_deletes_the_whole_buffer_linewise_into_one() {
    let mut vm = Vm::new(7804, "one\ntwo\nthree\n");
    vm.keys(&[(Key::D, NONE), (Key::G, SHIFT)]);
    assert_eq!(vm.buffer.text(), "");
    assert_eq!(
        register_at(28).map(|r| (r.text.clone(), r.kind)),
        Some((
            "one\ntwo\nthree\n".to_owned(),
            EditorVimRegisterKind::Linewise
        ))
    );
    vm.keys(&[(Key::P, NONE)]);
    assert_eq!(vm.buffer.text(), "one\ntwo\nthree\n");
}

#[test]
fn n_explicit_quote_one_dd_is_a_direct_write_without_shift() {
    let mut vm = Vm::new(7805, "one\ntwo\nthree\n");
    vm.keys(&[(Key::D, NONE), (Key::D, NONE)]);
    assert_eq!(register_at(28).map(|r| r.text), Some("one\n".to_owned()));
    vm.keys(&[
        (Key::Quote, SHIFT),
        (Key::Num1, NONE),
        (Key::D, NONE),
        (Key::D, NONE),
    ]);
    assert_eq!(vm.buffer.text(), "three\n");
    assert_eq!(register_at(28).map(|r| r.text), Some("two\n".to_owned()));
    assert_eq!(register_at(29), None, "explicit \"1dd must not shift \"2");
}

#[test]
fn n_named_linewise_delete_also_fills_the_numbered_stack() {
    let mut vm = Vm::new(7806, "one\ntwo\nthree\n");
    vm.keys(&[(Key::D, NONE), (Key::D, NONE)]);
    vm.keys(&[
        (Key::Quote, SHIFT),
        (Key::A, NONE),
        (Key::D, NONE),
        (Key::D, NONE),
    ]);
    assert_eq!(vm.buffer.text(), "three\n");
    assert_eq!(
        register_at(28).map(|r| r.text),
        Some("two\n".to_owned()),
        "Vim: \"add fills \"1 too"
    );
    assert_eq!(
        register_at(29).map(|r| r.text),
        Some("one\n".to_owned()),
        "Vim: the stack shifts under \"add"
    );
    assert_eq!(register_at(0).map(|r| r.text), Some("two\n".to_owned()));
    assert_eq!(vm.unnamed.as_ref().map(|r| r.text.as_str()), Some("two\n"));
}

#[test]
fn n_yy_normalizes_linewise_content() {
    let mut vm = Vm::new(7807, "\nbeta\n");
    vm.keys(&[(Key::Y, SHIFT)]);
    assert_eq!(
        register_at(ZERO_REGISTER_INDEX).map(|r| (r.text.clone(), r.kind)),
        Some(("\n".to_owned(), EditorVimRegisterKind::Linewise))
    );
    vm.keys(&[(Key::P, NONE)]);
    assert_eq!(vm.buffer.text(), "\n\nbeta\n");

    let mut vm = Vm::new(7808, "alpha");
    vm.keys(&[(Key::Y, SHIFT)]);
    assert_eq!(
        register_at(ZERO_REGISTER_INDEX).map(|r| (r.text.clone(), r.kind)),
        Some(("alpha\n".to_owned(), EditorVimRegisterKind::Linewise))
    );
    vm.keys(&[(Key::P, NONE)]);
    assert_eq!(vm.buffer.text(), "alpha\nalpha\n");
}

#[test]
fn a_append_linewise_onto_linewise_across_buffers() {
    let mut first = Vm::new(7809, "one\n");
    first.keys(&[(Key::Quote, SHIFT), (Key::A, NONE), (Key::Y, SHIFT)]);
    let mut second = Vm::without_clear(7810, "two\n");
    second.keys(&[(Key::Quote, SHIFT), (Key::A, SHIFT), (Key::Y, SHIFT)]);
    assert_eq!(
        register_at(0).map(|r| (r.text.clone(), r.kind)),
        Some(("one\ntwo\n".to_owned(), EditorVimRegisterKind::Linewise))
    );

    second.keys(&[(Key::Quote, SHIFT), (Key::A, NONE), (Key::P, NONE)]);
    assert_eq!(second.buffer.text(), "two\none\ntwo\n");
}

#[test]
fn a_append_charwise_batches_keep_order_and_kind() {
    let mut vm = Vm::new(7811, "1\n2\n3\n");
    for line in 0..3usize {
        vm.set_cursor(line, 0);
        let mut keys = vec![(Key::Quote, SHIFT)];
        keys.push(if line == 0 {
            (Key::A, NONE)
        } else {
            (Key::A, SHIFT)
        });
        keys.extend_from_slice(&[(Key::Y, NONE), (Key::I, NONE), (Key::W, NONE)]);
        vm.keys(&keys);
    }
    assert_eq!(
        register_at(0).map(|r| (r.text.clone(), r.kind)),
        Some(("123".to_owned(), EditorVimRegisterKind::Characterwise))
    );
    vm.set_cursor(0, 0);
    vm.keys(&[(Key::Quote, SHIFT), (Key::A, NONE), (Key::P, NONE)]);
    assert_eq!(vm.buffer.text(), "1123\n2\n3\n");
    assert_eq!(
        vm.char_cursor(),
        3,
        "charwise put cursor on the last pasted char"
    );
}

#[test]
fn a_charwise_append_onto_linewise_gets_its_own_line() {
    let mut vm = Vm::new(7812, "AAA\nbbb\nw\n");
    vm.keys(&[
        (Key::Quote, SHIFT),
        (Key::A, NONE),
        (Key::Y, SHIFT),
        (Key::J, NONE),
        (Key::Quote, SHIFT),
        (Key::A, SHIFT),
        (Key::Y, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
    ]);
    assert_eq!(
        register_at(0).map(|r| (r.text.clone(), r.kind)),
        Some(("AAA\nbbb\n".to_owned(), EditorVimRegisterKind::Linewise))
    );
}

#[test]
fn a_linewise_append_onto_charwise_inserts_line_break() {
    let mut vm = Vm::new(7813, "AAA\nl1\nw\n");
    vm.keys(&[
        (Key::Quote, SHIFT),
        (Key::A, NONE),
        (Key::Y, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
        (Key::J, NONE),
        (Key::Quote, SHIFT),
        (Key::A, SHIFT),
        (Key::Y, SHIFT),
    ]);
    assert_eq!(
        register_at(0).map(|r| (r.text.clone(), r.kind)),
        Some(("AAA\nl1\n".to_owned(), EditorVimRegisterKind::Linewise))
    );
    vm.set_cursor(2, 0);
    vm.keys(&[(Key::Quote, SHIFT), (Key::A, NONE), (Key::P, NONE)]);
    assert_eq!(vm.buffer.text(), "AAA\nl1\nw\nAAA\nl1\n");
}

#[test]
fn h_blackhole_yank_then_plain_put_is_a_noop() {
    let mut vm = Vm::new(7814, "alpha beta\n");
    vm.keys(&[
        (Key::Quote, SHIFT),
        (Key::Minus, SHIFT),
        (Key::Y, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
    ]);
    assert_eq!(vm.unnamed, None, "\"_y must not fill the unnamed register");
    let put = handle_vim_editor_key_event_with_state(
        &mut vm.buffer,
        Key::P,
        NONE,
        &mut vm.mode,
        &mut vm.pending,
        &mut vm.last_char_find,
        &mut vm.unnamed,
    );
    assert!(put.handled);
    assert!(!put.changed);
    assert_eq!(vm.buffer.text(), "alpha beta\n");
    assert!(vm.pending.is_none());
}

#[test]
fn h_blackhole_change_line_records_nothing() {
    let mut vm = Vm::with_unnamed(7815, "alpha\nbeta\n", charwise("SENTINEL"));
    vm.keys(&[
        (Key::Quote, SHIFT),
        (Key::Minus, SHIFT),
        (Key::C, NONE),
        (Key::C, NONE),
        (Key::Escape, NONE),
    ]);
    assert_eq!(vm.buffer.text(), "\nbeta\n");
    assert_eq!(vm.mode, EditorVimMode::Normal);
    assert_eq!(
        vm.unnamed.as_ref().map(|r| r.text.as_str()),
        Some("SENTINEL")
    );
    assert_eq!(register_at(28), None);
    assert_eq!(register_at(SMALL_DELETE_REGISTER_INDEX), None);
    assert_eq!(register_at(ZERO_REGISTER_INDEX), None);
}

#[test]
fn h_blackhole_x_and_shift_d_and_x_on_empty_line_record_nothing() {
    let mut vm = Vm::with_unnamed(7816, "abc\ndef\n", charwise("SENTINEL"));
    vm.keys(&[(Key::Quote, SHIFT), (Key::Minus, SHIFT), (Key::X, NONE)]);
    assert_eq!(vm.buffer.text(), "bc\ndef\n");
    assert_eq!(
        vm.unnamed.as_ref().map(|r| r.text.as_str()),
        Some("SENTINEL")
    );
    assert_eq!(register_at(SMALL_DELETE_REGISTER_INDEX), None);

    let mut vm = Vm::with_unnamed(7817, "hello world\n", charwise("SENTINEL"));
    vm.set_cursor(0, 1);
    vm.keys(&[(Key::Quote, SHIFT), (Key::Minus, SHIFT), (Key::D, SHIFT)]);
    assert_eq!(vm.buffer.text(), "h\n");
    assert_eq!(
        vm.unnamed.as_ref().map(|r| r.text.as_str()),
        Some("SENTINEL")
    );
    assert_eq!(register_at(SMALL_DELETE_REGISTER_INDEX), None);
    assert_eq!(register_at(28), None);

    let mut vm = Vm::with_unnamed(7818, "one\n\ntwo\n", charwise("SENTINEL"));
    vm.set_cursor(1, 0);
    vm.keys(&[(Key::X, NONE)]);
    assert_eq!(vm.buffer.text(), "one\n\ntwo\n");
    assert_eq!(
        vm.unnamed.as_ref().map(|r| r.text.as_str()),
        Some("SENTINEL")
    );
    assert_eq!(register_at(SMALL_DELETE_REGISTER_INDEX), None);
    assert_eq!(register_at(28), None);
}

#[test]
fn c_clipboard_linewise_delete_via_star_pastes_via_plus() {
    let mut vm = Vm::new(7819, "one\ntwo\nthree\n");
    vm.keys(&[
        (Key::Quote, SHIFT),
        (Key::Num8, SHIFT),
        (Key::D, NONE),
        (Key::D, NONE),
    ]);
    assert_eq!(vm.buffer.text(), "two\nthree\n");
    assert_eq!(
        register_at(CLIPBOARD_REGISTER_INDEX).map(|r| (r.text.clone(), r.kind)),
        Some(("one\n".to_owned(), EditorVimRegisterKind::Linewise))
    );
    vm.set_cursor(1, 0);
    vm.keys(&[(Key::Quote, SHIFT), (Key::Equals, SHIFT), (Key::P, NONE)]);

    assert_eq!(vm.buffer.text(), "two\nthree\none\n");
}

#[test]
fn c_clipboard_yank_leaves_zero_and_stack_alone() {
    let mut vm = Vm::new(7820, "one two\n");
    vm.keys(&[
        (Key::Quote, SHIFT),
        (Key::Equals, SHIFT),
        (Key::Y, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
    ]);
    assert_eq!(
        register_at(CLIPBOARD_REGISTER_INDEX).map(|r| (r.text.clone(), r.kind)),
        Some(("one".to_owned(), EditorVimRegisterKind::Characterwise))
    );
    assert_eq!(register_at(ZERO_REGISTER_INDEX), None);
    assert_eq!(register_at(28), None);
    assert_eq!(
        vm.unnamed.as_ref().map(|r| (r.text.clone(), r.kind)),
        Some(("one".to_owned(), EditorVimRegisterKind::Characterwise))
    );
}

#[test]
fn c_clipboard_mirror_persists_into_a_fresh_buffer() {
    let mut first = Vm::new(7821, "one\n");
    first.keys(&[(Key::Quote, SHIFT), (Key::Equals, SHIFT), (Key::Y, SHIFT)]);
    let mut second = Vm::without_clear(7822, "two\n");
    second.keys(&[(Key::Quote, SHIFT), (Key::Num8, SHIFT), (Key::P, NONE)]);
    assert_eq!(second.buffer.text(), "two\none\n");
}

#[test]
fn c_star_put_from_empty_clipboard_is_a_clean_noop() {
    let mut vm = Vm::new(7823, "alpha\n");
    vm.keys(&[(Key::Quote, SHIFT), (Key::Num8, SHIFT), (Key::P, NONE)]);
    assert_eq!(vm.buffer.text(), "alpha\n");
    assert!(vm.pending.is_none());
}

#[test]
fn c_clipboard_charwise_kind_survives_star_roundtrip() {
    let mut vm = Vm::new(7824, "ab cd\n");
    vm.keys(&[
        (Key::Quote, SHIFT),
        (Key::Num8, SHIFT),
        (Key::Y, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
    ]);
    vm.set_cursor(0, 3);
    vm.keys(&[(Key::Quote, SHIFT), (Key::Equals, SHIFT), (Key::P, NONE)]);
    assert_eq!(vm.buffer.text(), "ab cabd\n");
    assert_eq!(vm.char_cursor(), 5);
}

#[test]
fn p_charwise_p_and_p_leave_cursor_on_last_pasted_char() {
    let mut vm = Vm::with_unnamed(7825, "abc\n", charwise("XY"));
    vm.keys(&[(Key::P, SHIFT)]);
    assert_eq!(vm.buffer.text(), "XYabc\n");
    assert_eq!(
        vm.char_cursor(),
        1,
        "charwise P: cursor on the last pasted char"
    );

    let mut vm = Vm::with_unnamed(7826, "abc", charwise("X"));
    vm.set_cursor(0, 2);
    vm.keys(&[(Key::P, NONE)]);
    assert_eq!(vm.buffer.text(), "abcX");
    assert_eq!(
        vm.char_cursor(),
        3,
        "charwise p at buffer end: cursor on 'X'"
    );
}

#[test]
fn p_linewise_multi_line_p_cursor_on_the_first_put_line() {
    let mut vm = Vm::with_unnamed(7827, "a\nb\nc\n", linewise("  x\n  y\n"));
    vm.keys(&[(Key::P, NONE)]);
    assert_eq!(vm.buffer.text(), "a\n  x\n  y\nb\nc\n");
    assert_eq!(
        vm.cursor(),
        (1, 2),
        "Vim: linewise p cursor on the first put line's first non-blank"
    );
}

#[test]
fn p_linewise_counted_p_cursor_on_the_first_copy() {
    let mut vm = Vm::with_unnamed(7828, "a\nb\n", linewise("  x\n"));
    vm.keys(&[(Key::Num2, NONE), (Key::P, NONE)]);
    assert_eq!(vm.buffer.text(), "a\n  x\n  x\nb\n");
    assert_eq!(
        vm.cursor(),
        (1, 2),
        "Vim: counted linewise p cursor on the first copy"
    );
}

#[test]
fn n_explicit_quote_zero_dd_overwrites_the_yank_register() {
    let mut vm = Vm::new(7829, "one\ntwo\n");
    vm.keys(&[
        (Key::Quote, SHIFT),
        (Key::Num0, NONE),
        (Key::D, NONE),
        (Key::D, NONE),
    ]);
    assert_eq!(vm.buffer.text(), "two\n");
    assert_eq!(
        register_at(ZERO_REGISTER_INDEX).map(|r| (r.text.clone(), r.kind)),
        Some(("one\n".to_owned(), EditorVimRegisterKind::Linewise))
    );
    assert_eq!(vm.unnamed.as_ref().map(|r| r.text.as_str()), Some("one\n"));
}

#[test]
fn s_registers_summary_orders_truncates_and_escapes_newlines() {
    let mut vm = Vm::new(7830, "alpha\nbeta\nabcdefghijklmnop\ngamma\n");
    vm.set_cursor(0, 0);
    vm.keys(&[
        (Key::Quote, SHIFT),
        (Key::A, NONE),
        (Key::Y, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
    ]);
    vm.set_cursor(1, 0);
    vm.keys(&[
        (Key::Quote, SHIFT),
        (Key::B, NONE),
        (Key::Y, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
    ]);
    vm.set_cursor(2, 0);
    vm.keys(&[
        (Key::Quote, SHIFT),
        (Key::C, NONE),
        (Key::Y, NONE),
        (Key::I, NONE),
        (Key::W, NONE),
    ]);
    vm.set_cursor(3, 0);
    vm.keys(&[(Key::Quote, SHIFT), (Key::Equals, SHIFT), (Key::Y, SHIFT)]);
    vm.keys(&[(Key::D, NONE), (Key::D, NONE)]);
    vm.set_cursor(2, 0);
    vm.keys(&[(Key::Y, SHIFT)]);
    vm.set_cursor(0, 0);
    vm.keys(&[(Key::X, NONE)]);

    assert_eq!(
        vim_registers_summary().as_deref(),
        Some(concat!(
            "\"a alpha / \"b beta / \"c abcdefghijklmnop / ",
            "\"0 abcdefghijklmnop... / \"1 gamma\\n / \"- a / \"+ gamma\\n",
        )),
    );
}

#[test]
fn s_registers_summary_is_none_when_every_register_is_empty() {
    let _vm = Vm::new(7831, "body\n");
    assert_eq!(vim_registers_summary(), None);
}

#[test]
fn s_pending_labels_use_the_special_register_names() {
    let label_for = |index: usize, append: bool| {
        vim_pending_command_status_label(Some(EditorVimPendingKey::RegisterCommand {
            prefix_count: 1,
            command_count: None,
            register: EditorVimNamedRegister { index, append },
        }))
        .unwrap_or_default()
    };
    assert_eq!(label_for(SMALL_DELETE_REGISTER_INDEX, false), "\"-");
    assert_eq!(label_for(CLIPBOARD_REGISTER_INDEX, false), "\"+");
    assert_eq!(label_for(ZERO_REGISTER_INDEX, false), "\"0");
    assert_eq!(label_for(0, true), "\"A");
}
