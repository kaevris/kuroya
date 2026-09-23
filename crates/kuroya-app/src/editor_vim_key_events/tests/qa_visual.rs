use super::*;
use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

use super::super::state::vim_forget_marks_for_buffer;
use super::super::{EditorVimLastChange, VimKeyResult};

const NONE: Modifiers = Modifiers::NONE;
const SHIFT: Modifiers = Modifiers::SHIFT;
const CTRL: Modifiers = Modifiers::CTRL;

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
        Self {
            buffer: TextBuffer::from_text(id, None, text.to_owned()),
            mode: EditorVimMode::Normal,
            pending: None,
            last_char_find: None,
            unnamed: None,
            last_change: None,
        }
    }

    fn keys(&mut self, keys: &[(Key, Modifiers)]) -> VimKeyResult {
        let mut result = VimKeyResult::ignored();
        for (key, modifiers) in keys {
            result = handle_vim_editor_key_event_with_repeat_state(
                &mut self.buffer,
                *key,
                *modifiers,
                &mut self.mode,
                &mut self.pending,
                &mut self.last_char_find,
                &mut self.unnamed,
                &mut self.last_change,
            );
        }
        result
    }

    fn set_cursor(&mut self, line: usize, column: usize) {
        let cursor = self.buffer.line_column_to_char(line, column);
        self.buffer.set_single_cursor(cursor);
    }

    fn char_cursor(&self) -> usize {
        self.buffer.cursor()
    }
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

#[test]
fn qa_gv_reselects_the_last_visual_area() {
    let mut vm = Vm::new(9101, "alpha\nbeta\ngamma\n");
    vm.keys(&[(Key::V, NONE), (Key::J, NONE), (Key::Y, NONE)]);
    assert_eq!(vm.char_cursor(), 0, "y exits visual onto the area start");

    vm.keys(&[(Key::G, NONE), (Key::V, NONE)]);

    let reselected = matches!(
        vm.pending,
        Some(EditorVimPendingKey::VisualCharacter {
            anchor: 0,
            cursor: 6
        }) | Some(EditorVimPendingKey::VisualCharacter {
            anchor: 6,
            cursor: 0
        })
    );
    assert!(
        reselected,
        "gv must reselect the last visual area (chars 0..6); actual pending {:?}",
        vm.pending
    );
    assert_eq!(
        vm.buffer.selected_text().as_deref(),
        Some("alpha\nb"),
        "gv must restore the exact area"
    );
}

#[test]
fn qa_shift_v_inside_charwise_visual_switches_to_linewise() {
    let mut vm = Vm::new(9102, "alpha\nbeta\n");
    vm.keys(&[(Key::V, NONE), (Key::J, NONE)]);
    assert_eq!(
        vm.buffer.selected_text().as_deref(),
        Some("alpha\nb"),
        "charwise area before the switch"
    );

    vm.keys(&[(Key::V, SHIFT)]);

    assert_eq!(
        vm.pending,
        Some(EditorVimPendingKey::VisualLine {
            anchor: 0,
            cursor: 6,
            count: None,
        }),
        "V inside v must switch to linewise over the same lines; actual {:?}",
        vm.pending
    );
    assert_eq!(
        vm.buffer.selected_text().as_deref(),
        Some("alpha\nbeta\n"),
        "the linewise area covers whole lines"
    );
}

#[test]
fn qa_v_inside_linewise_visual_keeps_the_anchor() {
    let mut vm = Vm::new(9103, "alpha\nbeta\n");
    vm.set_cursor(0, 1);
    vm.keys(&[(Key::V, SHIFT), (Key::J, NONE)]);
    vm.keys(&[(Key::V, NONE)]);

    assert_eq!(
        vm.pending,
        Some(EditorVimPendingKey::VisualCharacter {
            anchor: 1,
            cursor: 7,
        }),
        "v inside V must keep the anchor at (0,1); actual {:?}",
        vm.pending
    );
    assert_eq!(
        vm.buffer.selected_text().as_deref(),
        Some("lpha\nb"),
        "the charwise area spans anchor to cursor"
    );
}

#[test]
fn qa_charwise_motion_count_does_not_leak_into_put() {
    let mut vm = Vm::new(9104, "one two three\n");
    vm.unnamed = Some(charwise("ZZ"));
    vm.keys(&[
        (Key::V, NONE),
        (Key::Num2, NONE),
        (Key::W, NONE),
        (Key::P, NONE),
    ]);
    assert_eq!(
        vm.buffer.text(),
        "ZZhree\n",
        "v2wp must paste once (a doubled paste means the motion count leaked)"
    );
    assert_eq!(
        vm.unnamed.as_ref().map(|register| register.text.as_str()),
        Some("one two t"),
        "the replaced selection lands in the unnamed register"
    );
}

#[test]
fn qa_linewise_motion_count_must_not_leak_into_put() {
    let mut vm = Vm::new(9105, "one\ntwo\nthree\nfour\n");
    vm.set_cursor(1, 0);
    vm.unnamed = Some(linewise("R1\n"));
    vm.keys(&[
        (Key::V, SHIFT),
        (Key::Num2, NONE),
        (Key::J, NONE),
        (Key::P, NONE),
    ]);

    assert_eq!(
        vm.buffer.text(),
        "one\nR1\nfour\n",
        "V2jp must paste the register once (actual {:?})",
        vm.buffer.text()
    );
}

#[test]
fn qa_linewise_o_discards_the_typed_count() {
    let mut vm = Vm::new(9106, "alpha\nbeta\ngamma\n");
    vm.keys(&[
        (Key::V, SHIFT),
        (Key::Num3, NONE),
        (Key::O, NONE),
        (Key::Period, SHIFT),
    ]);

    assert_eq!(
        vm.buffer.text(),
        "    alpha\nbeta\ngamma\n",
        "V3o> must indent by one shiftwidth (actual {:?})",
        vm.buffer.text()
    );
}

#[test]
fn qa_charwise_o_discards_the_typed_count() {
    let mut vm = Vm::new(9107, "abcd\nefgh\n");
    vm.set_cursor(0, 1);
    vm.keys(&[
        (Key::V, NONE),
        (Key::L, NONE),
        (Key::Num2, NONE),
        (Key::O, NONE),
    ]);
    assert_eq!(
        vm.pending,
        Some(EditorVimPendingKey::VisualCharacter {
            anchor: 2,
            cursor: 1,
        }),
        "o must swap the ends and drop the count; actual {:?}",
        vm.pending
    );
    assert_eq!(vm.buffer.selected_text().as_deref(), Some("bc"));
}

#[test]
fn qa_linewise_counted_motion_delete_is_unaffected_by_the_count() {
    let mut vm = Vm::new(9108, "l0\nl1\nl2\nl3\nl4\n");
    vm.set_cursor(1, 0);
    vm.keys(&[
        (Key::V, SHIFT),
        (Key::Num2, NONE),
        (Key::J, NONE),
        (Key::D, NONE),
    ]);
    assert_eq!(vm.buffer.text(), "l0\nl4\n");
    assert_eq!(
        vm.unnamed,
        Some(linewise("l1\nl2\nl3\n")),
        "V2jd must store the lines linewise"
    );
    assert!(vm.pending.is_none());

    let mut vm = Vm::new(9109, "l0\nl1\nl2\n");
    vm.keys(&[(Key::V, SHIFT), (Key::X, NONE)]);
    assert_eq!(vm.buffer.text(), "l1\nl2\n");
    assert_eq!(vm.unnamed, Some(linewise("l0\n")));
}

#[test]
fn qa_charwise_counted_motion_yank_spans_two_words() {
    let mut vm = Vm::new(9110, "one two three\n");
    vm.keys(&[
        (Key::V, NONE),
        (Key::Num2, NONE),
        (Key::W, NONE),
        (Key::Y, NONE),
    ]);
    assert_eq!(vm.unnamed, Some(charwise("one two t")));
    assert_eq!(vm.char_cursor(), 0, "y exits onto the area start");
    assert_eq!(vm.buffer.text(), "one two three\n");
    assert!(vm.pending.is_none());
}

#[test]
fn qa_linewise_u_lowercases_the_selected_lines() {
    let mut vm = Vm::new(9111, "AbC\nDeF\nghi\n");
    vm.keys(&[(Key::V, SHIFT), (Key::J, NONE), (Key::U, NONE)]);

    assert_eq!(
        vm.buffer.text(),
        "abc\ndef\nghi\n",
        "Vu must lowercase the selected lines (actual {:?})",
        vm.buffer.text()
    );
    assert!(
        vm.pending.is_none(),
        "the operator completes and leaves visual mode; actual {:?}",
        vm.pending
    );
}

#[test]
fn qa_linewise_r_replaces_chars_keeping_newlines() {
    let mut vm = Vm::new(9112, "abc\ndef\n");
    vm.keys(&[
        (Key::V, SHIFT),
        (Key::J, NONE),
        (Key::R, NONE),
        (Key::Minus, NONE),
    ]);

    assert_eq!(
        vm.buffer.text(),
        "---\n---\n",
        "Vr- must replace the selected chars (actual {:?})",
        vm.buffer.text()
    );
    assert!(vm.pending.is_none(), "actual {:?}", vm.pending);
}

#[test]
fn qa_linewise_c_deletes_the_lines_and_enters_insert() {
    let mut vm = Vm::new(9113, "one\ntwo\nthree\n");
    vm.set_cursor(1, 0);
    vm.keys(&[(Key::V, SHIFT), (Key::C, NONE)]);
    assert_eq!(
        vm.mode,
        EditorVimMode::Insert,
        "Vc must enter insert mode; actual {:?}",
        vm.mode
    );
    vm.keys(&[(Key::Escape, NONE)]);

    assert_eq!(
        vm.buffer.text(),
        "one\n\nthree\n",
        "Vc plus Esc must leave an empty line where the deleted line was (actual {:?})",
        vm.buffer.text()
    );
}

#[test]
fn qa_visual_g_joins_lines_without_spaces() {
    let mut vm = Vm::new(9114, "abc\ndef\n");
    vm.keys(&[
        (Key::V, NONE),
        (Key::J, NONE),
        (Key::G, NONE),
        (Key::J, SHIFT),
    ]);

    assert_eq!(
        vm.buffer.text(),
        "abcdef\n",
        "vj gJ must join without a space (actual {:?})",
        vm.buffer.text()
    );
    assert!(vm.pending.is_none(), "actual {:?}", vm.pending);
}

#[test]
fn qa_dot_during_visual_is_swallowed_then_repeats_after_exit() {
    let mut vm = Vm::new(9115, "abcd\n");
    vm.keys(&[(Key::X, NONE)]);
    assert_eq!(vm.buffer.text(), "bcd\n", "seed the last change with x");

    vm.keys(&[(Key::V, NONE), (Key::L, NONE)]);
    assert_eq!(
        vm.pending,
        Some(EditorVimPendingKey::VisualCharacter {
            anchor: 0,
            cursor: 1,
        })
    );
    let dot = vm.keys(&[(Key::Period, NONE)]);
    assert!(!dot.changed, "`.` in visual must not repeat");
    assert_eq!(
        vm.buffer.text(),
        "bcd\n",
        "`.` in visual must not touch the buffer"
    );
    assert_eq!(
        vm.pending,
        Some(EditorVimPendingKey::VisualCharacter {
            anchor: 0,
            cursor: 1,
        }),
        "the selection survives the swallowed `.`"
    );

    vm.keys(&[(Key::Escape, NONE)]);
    vm.keys(&[(Key::Period, NONE)]);
    assert_eq!(
        vm.buffer.text(),
        "cd\n",
        "after leaving visual, `.` replays the pre-visual x"
    );
}

#[test]
fn qa_ctrl_scroll_during_visual_must_keep_the_selection() {
    let mut vm = Vm::new(9116, "alpha\nbeta\ngamma\n");
    vm.keys(&[(Key::V, NONE), (Key::L, NONE)]);
    let result = vm.keys(&[(Key::D, CTRL)]);

    assert!(
        matches!(
            vm.pending,
            Some(
                EditorVimPendingKey::VisualCharacter { .. }
                    | EditorVimPendingKey::VisualCharacterCount { .. }
            )
        ),
        "Ctrl+D during visual must keep visual mode active; actual pending {:?} (result.handled={})",
        vm.pending,
        result.handled
    );
    assert_eq!(vm.buffer.text(), "alpha\nbeta\ngamma\n");
}

#[test]
fn qa_unhandled_ctrl_keys_during_visual_never_crash_or_mutate() {
    let mut vm = Vm::new(9117, "alpha\nbeta\n");
    vm.keys(&[(Key::V, NONE), (Key::L, NONE)]);
    for key in [Key::V, Key::O, Key::R, Key::G] {
        let result = vm.keys(&[(key, CTRL)]);
        assert!(!result.changed, "ctrl+{key:?} must not mutate the buffer");
        assert!(vm.pending.is_none(), "actual behavior: the area is dropped");
        assert_eq!(vm.buffer.text(), "alpha\nbeta\n");
        assert_eq!(vm.mode, EditorVimMode::Normal);
    }
}

#[test]
fn qa_vj0_includes_the_newline_but_v_dollar_stops_at_line_end() {
    let mut vm = Vm::new(9117, "abc\ndef\n");
    vm.set_cursor(0, 1);
    vm.keys(&[
        (Key::V, NONE),
        (Key::J, NONE),
        (Key::Num0, NONE),
        (Key::Y, NONE),
    ]);
    assert_eq!(
        vm.unnamed,
        Some(charwise("bc\nd")),
        "vj0 spans across the newline"
    );
    assert_eq!(vm.char_cursor(), 1);

    let mut vm = Vm::new(9118, "abc\ndef\n");
    vm.set_cursor(0, 1);
    vm.keys(&[(Key::V, NONE), (Key::Num4, SHIFT), (Key::Y, NONE)]);
    assert_eq!(
        vm.unnamed,
        Some(charwise("bc")),
        "v$ must stop before the newline"
    );
}

#[test]
fn qa_v_dollar_j_yanks_through_the_line_end() {
    let mut vm = Vm::new(9119, "abc\ndef\n");
    vm.set_cursor(0, 1);
    vm.keys(&[
        (Key::V, NONE),
        (Key::Num4, SHIFT),
        (Key::J, NONE),
        (Key::Y, NONE),
    ]);
    assert_eq!(vm.unnamed, Some(charwise("bc\ndef")));
    assert_eq!(vm.char_cursor(), 1);
}

#[test]
fn qa_single_char_selection_delete_and_yank() {
    let mut vm = Vm::new(9120, "abc\n");
    vm.set_cursor(0, 1);
    vm.keys(&[(Key::V, NONE), (Key::D, NONE)]);
    assert_eq!(vm.buffer.text(), "ac\n");
    assert_eq!(vm.unnamed, Some(charwise("b")));
    assert_eq!(vm.char_cursor(), 1, "cursor lands where the char was");
    assert!(vm.pending.is_none());

    vm.keys(&[(Key::V, NONE), (Key::Y, NONE)]);
    assert_eq!(vm.unnamed, Some(charwise("c")));
    assert_eq!(vm.char_cursor(), 1);
}

#[test]
fn qa_reversed_selection_yank_delete_and_paste_roundtrip() {
    let mut vm = Vm::new(9121, "alpha\nbeta\n");
    vm.set_cursor(0, 3);
    vm.keys(&[
        (Key::V, NONE),
        (Key::H, NONE),
        (Key::H, NONE),
        (Key::Y, NONE),
    ]);
    assert_eq!(vm.unnamed, Some(charwise("lph")));
    assert_eq!(vm.char_cursor(), 1, "reversed y still exits at the start");

    vm.set_cursor(0, 3);
    vm.keys(&[
        (Key::V, NONE),
        (Key::H, NONE),
        (Key::H, NONE),
        (Key::D, NONE),
    ]);
    assert_eq!(vm.buffer.text(), "aa\nbeta\n");
    assert_eq!(vm.char_cursor(), 1);

    vm.keys(&[(Key::P, NONE)]);
    assert_eq!(
        vm.buffer.text(),
        "aalph\nbeta\n",
        "p after the cursor (on the second 'a') yields aalph, like Vim"
    );
}

#[test]
fn qa_reversed_selection_case_conversion() {
    let mut vm = Vm::new(9122, "aBCDe\n");
    vm.set_cursor(0, 4);
    vm.keys(&[(Key::V, NONE), (Key::Num0, NONE), (Key::U, NONE)]);
    assert_eq!(vm.buffer.text(), "abcde\n");
    assert!(vm.pending.is_none());
}

#[test]
fn qa_visual_yank_does_not_become_the_repeatable_change() {
    let mut vm = Vm::new(9123, "abcd efgh\n");
    vm.keys(&[(Key::X, NONE)]);
    assert_eq!(vm.buffer.text(), "bcd efgh\n");

    vm.keys(&[(Key::V, NONE), (Key::W, NONE), (Key::Y, NONE)]);

    assert_eq!(vm.unnamed, Some(charwise("bcd e")));
    assert!(vm.last_change.is_some(), "the seeded x is still the change");

    vm.keys(&[(Key::Period, NONE)]);
    assert_eq!(
        vm.buffer.text(),
        "cd efgh\n",
        "`.` must replay the x, not anything from the visual yank"
    );
}

#[test]
fn qa_linewise_visual_delete_repeats_with_period() {
    let mut vm = Vm::new(9124, "alpha\nbeta\ngamma\n");
    vm.keys(&[(Key::V, SHIFT), (Key::D, NONE)]);
    assert_eq!(vm.buffer.text(), "beta\ngamma\n");
    assert_eq!(vm.unnamed, Some(linewise("alpha\n")));

    vm.keys(&[(Key::Period, NONE)]);
    assert_eq!(
        vm.buffer.text(),
        "gamma\n",
        "`.` must repeat the linewise delete"
    );
}

#[test]
fn qa_marks_are_updated_by_every_visual_area() {
    vim_forget_marks_for_buffer(9125);
    let mut vm = Vm::new(9125, "one two\nthree four\nlast\n");
    vm.keys(&[
        (Key::V, NONE),
        (Key::W, NONE),
        (Key::J, NONE),
        (Key::Escape, NONE),
    ]);
    assert_eq!(vm.char_cursor(), 0, "esc lands on the area start");

    vm.keys(&[(Key::G, SHIFT)]);
    vm.keys(&[(Key::Backtick, NONE), (Key::Comma, SHIFT)]);
    assert_eq!(vm.char_cursor(), 0, "`'< is the exact area start (0,0)");

    vm.keys(&[(Key::G, SHIFT)]);
    vm.keys(&[(Key::Backtick, NONE), (Key::Period, SHIFT)]);
    assert_eq!(
        vm.char_cursor(),
        12,
        "`'> is the exact area end (1,4) = char 12"
    );

    vm.set_cursor(2, 0);
    vm.keys(&[(Key::V, NONE), (Key::Num4, SHIFT), (Key::Escape, NONE)]);
    vm.keys(&[(Key::Backtick, NONE), (Key::Period, SHIFT)]);
    assert_eq!(
        vm.char_cursor(),
        22,
        "`'> must track the newest area (2,4) = char 22"
    );
    vm.keys(&[(Key::Backtick, NONE), (Key::Comma, SHIFT)]);
    assert_eq!(
        vm.char_cursor(),
        19,
        "`'< must track the newest area (2,0) = char 19"
    );
}

#[test]
fn qa_linewise_outdent_with_shift_comma() {
    let mut vm = Vm::new(9126, "    ind\n    ent\nlast\n");
    vm.keys(&[(Key::V, SHIFT), (Key::J, NONE), (Key::Comma, SHIFT)]);
    assert_eq!(
        vm.buffer.text(),
        "ind\nent\nlast\n",
        "V< must outdent the selected lines (actual {:?})",
        vm.buffer.text()
    );
    assert!(vm.pending.is_none());
}
