use super::*;
use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

use super::super::EditorVimLastChange;

const NONE: Modifiers = Modifiers::NONE;
const SHIFT: Modifiers = Modifiers::SHIFT;

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
        vim_clear_marks();
        Self {
            buffer: TextBuffer::from_text(id, None, text.to_owned()),
            mode: EditorVimMode::Normal,
            pending: None,
            last_char_find: None,
            unnamed: None,
            last_change: None,
        }
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

    fn set_mark_a(&mut self) {
        self.keys(&[(Key::M, NONE), (Key::A, NONE)]);
    }

    fn quote_jump(&mut self, ch: char) {
        let (key, modifiers) = key_of(ch);
        self.keys(&[(Key::Quote, NONE), (key, modifiers)]);
    }

    fn backtick_jump(&mut self, ch: char) {
        let (key, modifiers) = key_of(ch);
        self.keys(&[(Key::Backtick, NONE), (key, modifiers)]);
    }

    fn run_ex(&mut self, command: &str) {
        self.keys(&[(Key::Colon, NONE)]);
        for ch in command.chars() {
            let (key, modifiers) = key_of(ch);
            self.keys(&[(key, modifiers)]);
        }
        self.keys(&[(Key::Enter, NONE)]);
    }

    fn search(&mut self, query: &str) {
        self.keys(&[(Key::Slash, NONE)]);
        for ch in query.chars() {
            let (key, modifiers) = key_of(ch);
            self.keys(&[(key, modifiers)]);
        }
        self.keys(&[(Key::Enter, NONE)]);
    }
}

fn key_of(ch: char) -> (Key, Modifiers) {
    match ch {
        'a' => (Key::A, NONE),
        'b' => (Key::B, NONE),
        'c' => (Key::C, NONE),
        'd' => (Key::D, NONE),
        'e' => (Key::E, NONE),
        'f' => (Key::F, NONE),
        'g' => (Key::G, NONE),
        'h' => (Key::H, NONE),
        'i' => (Key::I, NONE),
        'j' => (Key::J, NONE),
        'k' => (Key::K, NONE),
        'l' => (Key::L, NONE),
        'm' => (Key::M, NONE),
        'n' => (Key::N, NONE),
        'o' => (Key::O, NONE),
        'p' => (Key::P, NONE),
        'q' => (Key::Q, NONE),
        'r' => (Key::R, NONE),
        's' => (Key::S, NONE),
        't' => (Key::T, NONE),
        'u' => (Key::U, NONE),
        'v' => (Key::V, NONE),
        'w' => (Key::W, NONE),
        'x' => (Key::X, NONE),
        'y' => (Key::Y, NONE),
        'z' => (Key::Z, NONE),
        '0' => (Key::Num0, NONE),
        '1' => (Key::Num1, NONE),
        '2' => (Key::Num2, NONE),
        '3' => (Key::Num3, NONE),
        '4' => (Key::Num4, NONE),
        '5' => (Key::Num5, NONE),
        '6' => (Key::Num6, NONE),
        '7' => (Key::Num7, NONE),
        '8' => (Key::Num8, NONE),
        '9' => (Key::Num9, NONE),
        '/' => (Key::Slash, NONE),
        '<' => (Key::Comma, SHIFT),
        '>' => (Key::Period, SHIFT),
        _ => panic!("key_of does not map {ch}"),
    }
}

#[test]
fn qa_marks_gg_is_a_jump_and_two_quote_returns_to_pre_jump_position() {
    let mut vm = Vm::new(7001, "one\ntwo\nthree\nfour\n");
    vm.set_cursor(3, 0);

    vm.keys(&[(Key::G, NONE), (Key::G, NONE)]);
    assert_eq!(vm.cursor(), (0, 0), "gg moves to the first line");

    vm.keys(&[(Key::Quote, NONE), (Key::Quote, NONE)]);
    assert_eq!(
        vm.cursor(),
        (3, 0),
        "Vim: gg is a jump, so '' returns to the pre-jump position"
    );
}

#[test]
fn qa_marks_count_g_is_a_jump_and_two_quote_returns() {
    let mut vm = Vm::new(7002, "one\ntwo\nthree\nfour\n");
    vm.set_cursor(0, 0);

    vm.keys(&[(Key::Num3, NONE), (Key::G, SHIFT)]);
    assert_eq!(vm.cursor(), (2, 0), "3G moves to line 3");

    vm.keys(&[(Key::Quote, NONE), (Key::Quote, NONE)]);
    assert_eq!(
        vm.cursor(),
        (0, 0),
        "Vim: {{N}}G is a jump, so '' returns to the pre-jump position"
    );
}

#[test]
fn qa_marks_ex_goto_line_is_a_jump_and_two_quote_returns() {
    let mut vm = Vm::new(7003, "alpha\n  beta\ngamma\n");
    vm.set_cursor(0, 0);

    vm.run_ex("3");
    assert_eq!(vm.cursor(), (2, 0), ":3 lands on line 3");

    vm.keys(&[(Key::Quote, NONE), (Key::Quote, NONE)]);
    assert_eq!(
        vm.cursor(),
        (0, 0),
        "Vim: :{{N}} is a jump, so '' returns to the pre-jump position"
    );
}

#[test]
fn qa_marks_g_then_backtick_backtick_returns_to_exact_pre_jump_column() {
    let mut vm = Vm::new(7004, "one two three\nsecond line\n");
    vm.set_cursor(0, 3);

    vm.keys(&[(Key::G, SHIFT)]);
    assert_eq!(vm.cursor(), (1, 0), "G moves to the last line");

    vm.keys(&[(Key::Backtick, NONE), (Key::Backtick, NONE)]);
    assert_eq!(
        vm.cursor(),
        (0, 3),
        "Vim: `` returns to the exact pre-jump column (charwise context mark)"
    );
}

#[test]
fn qa_marks_forward_search_is_a_jump_and_two_quote_returns() {
    let mut vm = Vm::new(7005, "alpha\nbeta\ngamma\n");
    vm.set_cursor(0, 0);

    vm.search("gamma");
    assert_eq!(vm.cursor(), (2, 0), "/gamma lands on the match");

    vm.keys(&[(Key::Quote, NONE), (Key::Quote, NONE)]);
    assert_eq!(
        vm.cursor(),
        (0, 0),
        "Vim: /pat is a jump, so '' returns to the position before the search"
    );
}

#[test]
fn qa_marks_search_repeat_n_is_a_jump_refreshing_the_context_mark() {
    let mut vm = Vm::new(7006, "alpha\nbeta\ngamma\nbeta\n");
    vm.search("beta");
    vm.keys(&[(Key::G, SHIFT)]);
    vm.keys(&[(Key::K, NONE)]);
    vm.keys(&[(Key::N, NONE)]);
    assert_eq!(vm.cursor(), (3, 0), "n lands on the next match");

    vm.keys(&[(Key::Quote, NONE), (Key::Quote, NONE)]);
    assert_eq!(
        vm.cursor(),
        (2, 0),
        "Vim: n is a jump and must refresh '' to the pre-n position (2,0)"
    );
}

#[test]
fn qa_marks_percent_is_a_jump_and_two_quote_returns() {
    let mut vm = Vm::new(7007, "(one two)");
    vm.set_cursor(0, 0);

    vm.keys(&[(Key::Num5, SHIFT)]);
    assert_eq!(vm.cursor(), (0, 8), "% lands on the matching bracket");

    vm.keys(&[(Key::Quote, NONE), (Key::Quote, NONE)]);
    assert_eq!(
        vm.cursor(),
        (0, 0),
        "Vim: % is a jump, so '' returns to the pre-jump position"
    );
}

#[test]
fn qa_marks_next_paragraph_is_a_jump_and_two_quote_returns() {
    let mut vm = Vm::new(7008, "alpha\nbeta\n\ngamma\n");
    vm.set_cursor(0, 0);

    vm.keys(&[(Key::CloseBracket, SHIFT)]);
    let after = vm.cursor();
    assert_ne!(after, (0, 0), "}} leaves the first paragraph");

    vm.keys(&[(Key::Quote, NONE), (Key::Quote, NONE)]);
    assert_eq!(
        vm.cursor(),
        (0, 0),
        "Vim: }} is a jump, so '' returns to the pre-jump position"
    );
}

#[test]
fn qa_marks_star_is_a_jump_and_two_quote_returns() {
    let mut vm = Vm::new(7009, "alpha beta alpha");
    vm.set_cursor(0, 0);

    vm.keys(&[(Key::Num8, SHIFT)]);
    assert_eq!(vm.cursor(), (0, 11), "* lands on the next match");

    vm.keys(&[(Key::Quote, NONE), (Key::Quote, NONE)]);
    assert_eq!(
        vm.cursor(),
        (0, 0),
        "Vim: * is a jump (search), so '' returns to the pre-jump position"
    );
}

#[test]
fn qa_marks_backtick_jump_to_visual_marks_lands_on_exact_columns() {
    let mut vm = Vm::new(7010, "one two three\nsecond\n");
    vm.set_cursor(0, 0);
    vm.keys(&[(Key::V, NONE), (Key::W, NONE), (Key::Escape, NONE)]);
    vm.keys(&[(Key::G, SHIFT)]);

    vm.backtick_jump('<');
    assert_eq!(
        vm.cursor(),
        (0, 0),
        "Vim: `'< returns to the exact first column of the last visual area"
    );

    vm.keys(&[(Key::G, SHIFT)]);
    vm.backtick_jump('>');
    assert_eq!(
        vm.cursor(),
        (0, 4),
        "Vim: `'> returns to the exact last column of the last visual area"
    );
}

#[test]
fn qa_marks_visual_yank_updates_the_visual_bounds_marks() {
    let mut vm = Vm::new(7011, "one two three\nsecond\n");
    vm.set_cursor(0, 0);
    vm.keys(&[(Key::V, NONE), (Key::W, NONE), (Key::Y, NONE)]);
    assert_eq!(
        vm.mode,
        EditorVimMode::Normal,
        "visual y leaves visual mode"
    );
    vm.keys(&[(Key::G, SHIFT)]);

    vm.backtick_jump('<');
    assert_eq!(
        vm.cursor(),
        (0, 0),
        "Vim: visual y exits visual mode, so '< marks the yanked area start"
    );
}

#[test]
fn qa_marks_visual_delete_updates_the_visual_bounds_marks() {
    let mut vm = Vm::new(7012, "one two three\nsecond\n");
    vm.set_cursor(0, 0);
    vm.keys(&[(Key::V, NONE), (Key::W, NONE), (Key::D, NONE)]);

    assert_eq!(vm.buffer.text(), "wo three\nsecond\n", "vwd deletes");
    vm.keys(&[(Key::G, SHIFT)]);

    vm.backtick_jump('<');
    assert_eq!(
        vm.cursor(),
        (0, 0),
        "Vim: visual d exits visual mode, so '< marks the deleted area start"
    );
}

#[test]
fn qa_marks_successive_visual_areas_update_the_marks_to_the_latest() {
    let mut vm = Vm::new(7013, "one two three\nbeta gamma\n");
    vm.set_cursor(0, 0);

    vm.keys(&[(Key::V, NONE), (Key::W, NONE), (Key::Escape, NONE)]);

    vm.keys(&[(Key::J, NONE), (Key::W, NONE)]);
    vm.keys(&[(Key::V, NONE), (Key::B, NONE), (Key::Escape, NONE)]);

    vm.keys(&[(Key::G, SHIFT)]);
    vm.backtick_jump('<');
    assert_eq!(
        vm.cursor(),
        (1, 0),
        "Vim: '< tracks the LATEST visual area, not the first one"
    );

    vm.keys(&[(Key::G, SHIFT)]);
    vm.backtick_jump('>');
    assert_eq!(
        vm.cursor(),
        (1, 5),
        "Vim: '> tracks the LATEST visual area end"
    );
}

#[test]
fn qa_marks_visual_marks_ride_line_deletes_above_them() {
    let mut vm = Vm::new(7014, "one\ntwo three four\nfive\n");
    vm.set_cursor(1, 4);
    vm.keys(&[(Key::V, NONE), (Key::W, NONE), (Key::Escape, NONE)]);
    vm.set_cursor(0, 0);
    vm.keys(&[(Key::D, NONE), (Key::D, NONE)]);
    assert_eq!(vm.buffer.text(), "two three four\nfive\n");

    vm.keys(&[(Key::G, SHIFT)]);
    vm.backtick_jump('>');
    assert_eq!(
        vm.cursor(),
        (0, 10),
        "Vim: '<'/'> are positions and ride line deletes above them"
    );
}

#[test]
fn qa_marks_visual_bounds_marks_are_per_buffer() {
    vim_clear_named_registers();
    vim_clear_marks();
    let mut first = TextBuffer::from_text(7015, None, "one two\nsecond\n".to_owned());
    let mut second = TextBuffer::from_text(7016, None, "other text\nmore\n".to_owned());
    let mut mode = EditorVimMode::Normal;
    let mut pending: Option<EditorVimPendingKey> = None;
    let mut last_char_find = None;
    let mut unnamed = None;

    for (key, modifiers) in [(Key::V, NONE), (Key::W, NONE), (Key::Escape, NONE)] {
        assert!(
            handle_vim_editor_key_event_with_state(
                &mut first,
                key,
                modifiers,
                &mut mode,
                &mut pending,
                &mut last_char_find,
                &mut unnamed,
            )
            .handled
        );
    }

    let before = second.cursor();
    for (key, modifiers) in [(Key::Backtick, NONE), (Key::Comma, SHIFT)] {
        assert!(
            handle_vim_editor_key_event_with_state(
                &mut second,
                key,
                modifiers,
                &mut mode,
                &mut pending,
                &mut last_char_find,
                &mut unnamed,
            )
            .handled
        );
    }
    assert_eq!(
        second.cursor(),
        before,
        "Vim: '< is buffer-local; another buffer must not jump to it"
    );
}

#[test]
fn qa_marks_quote_jumps_first_non_blank_but_backtick_exact_even_in_whitespace() {
    let mut vm = Vm::new(7017, "    deep line\nmore here\n");
    vm.set_cursor(0, 2);
    vm.set_mark_a();
    vm.keys(&[(Key::G, SHIFT)]);

    vm.quote_jump('a');
    assert_eq!(
        vm.cursor(),
        (0, 4),
        "Vim: quote jump is linewise and lands on the first non-blank"
    );

    vm.keys(&[(Key::G, SHIFT)]);
    vm.backtick_jump('a');
    assert_eq!(
        vm.cursor(),
        (0, 2),
        "Vim: backtick jump is charwise and lands on the exact column, even inside whitespace"
    );
}

#[test]
fn qa_marks_context_mark_toggles_between_named_mark_and_origin() {
    let mut vm = Vm::new(7018, "one\ntwo\nthree\n");
    vm.set_cursor(2, 1);
    vm.set_mark_a();
    vm.set_cursor(0, 0);

    vm.quote_jump('a');
    assert_eq!(
        vm.cursor(),
        (2, 0),
        "'a is linewise: first non-blank of the marked line"
    );

    vm.keys(&[(Key::Quote, NONE), (Key::Quote, NONE)]);
    assert_eq!(vm.cursor(), (0, 0), "first '' returns to the origin");

    vm.keys(&[(Key::Quote, NONE), (Key::Quote, NONE)]);
    assert_eq!(
        vm.cursor(),
        (2, 0),
        "Vim: pressing '' repeatedly toggles between the two positions"
    );
}

#[test]
fn qa_marks_x_before_the_mark_pulls_the_mark_left() {
    let mut vm = Vm::new(7019, "abc def");
    vm.set_cursor(0, 6);
    vm.set_mark_a();
    vm.set_cursor(0, 1);
    vm.keys(&[(Key::X, NONE)]);
    assert_eq!(vm.buffer.text(), "ac def");

    vm.backtick_jump('a');
    assert_eq!(
        vm.cursor(),
        (0, 5),
        "Vim: deleting chars before a mark on the same line pulls the mark left"
    );
}

#[test]
fn qa_marks_x_on_the_marked_char_keeps_the_mark_at_the_deletion_point() {
    let mut vm = Vm::new(7020, "abcd");
    vm.set_cursor(0, 2);
    vm.set_mark_a();
    vm.keys(&[(Key::X, NONE)]);
    assert_eq!(vm.buffer.text(), "abd");

    vm.keys(&[(Key::G, SHIFT)]);
    vm.backtick_jump('a');
    assert_eq!(
        vm.cursor(),
        (0, 2),
        "Vim: a mark is only cleared when its LINE dies; x over the marked \
         char leaves the mark at the deletion point"
    );
}

#[test]
fn qa_marks_join_moves_marks_on_the_joined_line_to_the_join_point() {
    let mut vm = Vm::new(7021, "alpha\nbeta\ngamma\n");
    vm.set_cursor(1, 1);
    vm.set_mark_a();
    vm.set_cursor(0, 0);
    vm.keys(&[(Key::J, SHIFT)]);
    assert_eq!(vm.buffer.text(), "alpha beta\ngamma\n");

    vm.backtick_jump('a');
    assert_eq!(
        vm.cursor(),
        (0, 7),
        "Vim: J pulls the next line up, so a mark on it rides to the join point"
    );
}

#[test]
fn qa_marks_substitute_shifts_marks_after_the_changed_text() {
    let mut vm = Vm::new(7022, "one ONE two\n");
    vm.set_cursor(0, 8);
    vm.set_mark_a();
    vm.set_cursor(0, 0);

    vm.run_ex("s/one/1/");
    assert_eq!(vm.buffer.text(), "1 ONE two\n");

    vm.backtick_jump('a');
    assert_eq!(
        vm.cursor(),
        (0, 6),
        "Vim: :s shortens the line, so marks after the change shift left by 2"
    );
}

#[test]
fn qa_marks_charwise_put_shifts_marks_after_the_paste_on_the_same_line() {
    let mut vm = Vm::new(7023, "x world\n");
    vm.set_cursor(0, 2);
    vm.set_mark_a();
    vm.set_cursor(0, 0);
    vm.keys(&[(Key::Y, NONE), (Key::L, NONE)]);
    vm.keys(&[(Key::P, NONE)]);
    assert_eq!(vm.buffer.text(), "xx world\n");

    vm.backtick_jump('a');
    assert_eq!(
        vm.cursor(),
        (0, 3),
        "Vim: a charwise put before a mark shifts the mark right"
    );
}

#[test]
fn qa_marks_outdent_pulls_mark_columns_left_by_the_indent() {
    let mut vm = Vm::new(7024, "    deep\nmore\n");
    vm.set_cursor(0, 7);
    vm.set_mark_a();
    vm.set_cursor(0, 0);

    vm.keys_with_indent(&[(Key::Comma, SHIFT), (Key::Comma, SHIFT)], "    ");
    assert_eq!(vm.buffer.text(), "deep\nmore\n");

    vm.backtick_jump('a');
    assert_eq!(
        vm.cursor(),
        (0, 3),
        "Vim: << removes the indent, so marks past column 0 shift left"
    );
}

#[test]
fn qa_marks_linewise_put_below_the_marked_line_shifts_it_down() {
    let mut vm = Vm::new(7025, "one\ntwo\nthree\n");
    vm.set_cursor(2, 0);
    vm.set_mark_a();
    vm.set_cursor(0, 0);
    vm.keys(&[(Key::Y, NONE), (Key::Y, NONE)]);
    vm.keys(&[(Key::J, NONE)]);
    vm.keys(&[(Key::P, NONE)]);
    assert_eq!(vm.buffer.text(), "one\ntwo\none\nthree\n");

    vm.quote_jump('a');
    assert_eq!(
        vm.cursor(),
        (3, 0),
        "Vim: a linewise put above a marked line shifts the mark down"
    );
}

#[test]
fn qa_marks_visual_line_delete_pulls_marks_on_following_lines_up() {
    let mut vm = Vm::new(7026, "one\ntwo\nthree\n");
    vm.set_cursor(2, 0);
    vm.set_mark_a();
    vm.set_cursor(0, 0);
    vm.keys(&[(Key::V, SHIFT), (Key::J, NONE), (Key::D, NONE)]);
    assert_eq!(vm.buffer.text(), "three\n");

    vm.quote_jump('a');
    assert_eq!(
        vm.cursor(),
        (0, 0),
        "Vim: deleting the lines above moves the marked line up"
    );
}
