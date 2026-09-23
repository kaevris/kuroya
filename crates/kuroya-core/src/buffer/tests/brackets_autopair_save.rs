use super::*;

#[test]
fn bracket_matching_finds_pairs() {
    let mut buffer = TextBuffer::from_text(1, None, "fn main() { call(); }".to_owned());
    buffer.set_single_cursor(10);
    assert_eq!(buffer.matching_bracket(), Some((10, 20)));
}

#[test]
fn bracket_matching_accepts_cursor_after_bracket() {
    let mut buffer = TextBuffer::from_text(1, None, "fn main() { call(); }".to_owned());
    buffer.set_single_cursor(11);
    assert_eq!(buffer.matching_bracket(), Some((10, 20)));
}

#[test]
fn bracket_matching_uses_bounded_scan_window() {
    let gap = " ".repeat(MAX_BRACKET_SCAN_CHARS + 1);
    let buffer = TextBuffer::from_text(1, None, format!("({gap})"));

    assert_eq!(buffer.bracket_block_selection_range_at(0), None);
    assert_eq!(
        buffer.bracket_block_selection_range_at(buffer.len_chars()),
        None
    );
}

#[test]
fn bracket_block_selection_range_uses_clicked_bracket_pair_contents() {
    let buffer = TextBuffer::from_text(1, None, "fn main() { return value; }".to_owned());
    let open = buffer.text().chars().position(|ch| ch == '{').unwrap();
    let close = buffer.text().chars().position(|ch| ch == '}').unwrap();

    assert_eq!(
        buffer.bracket_block_selection_range_at(open),
        Some(open + 1..close)
    );
    assert_eq!(
        buffer.bracket_block_selection_range_at(open + 1),
        Some(open + 1..close)
    );
    assert_eq!(
        buffer.bracket_block_selection_range_at(close),
        Some(open + 1..close)
    );
    assert_eq!(
        buffer.bracket_block_selection_range_at(close + 1),
        Some(open + 1..close)
    );
    assert_eq!(buffer.bracket_block_selection_range_at(open + 2), None);
}

#[test]
fn bracket_block_selection_range_clamps_out_of_bounds_cursor() {
    let buffer = TextBuffer::from_text(1, None, "call(value)".to_owned());

    assert_eq!(
        buffer.bracket_block_selection_range_at(usize::MAX),
        Some(5..10)
    );
}

#[test]
fn bracket_block_selection_range_ignores_empty_pairs() {
    let buffer = TextBuffer::from_text(1, None, "let empty = {};".to_owned());
    let open = buffer.text().chars().position(|ch| ch == '{').unwrap();

    assert_eq!(buffer.bracket_block_selection_range_at(open), None);
}

#[test]
fn bracket_matching_can_include_enclosing_pair() {
    let mut buffer = TextBuffer::from_text(1, None, "fn main() { call(); }".to_owned());
    buffer.set_single_cursor(14);

    assert_eq!(buffer.matching_brackets(), Vec::<(usize, usize)>::new());
    assert_eq!(
        buffer.matching_brackets_including_enclosing(),
        vec![(10, 20)]
    );
}

#[test]
fn bracket_matching_including_enclosing_prefers_innermost_pair() {
    let mut buffer = TextBuffer::from_text(1, None, "{ [xy] }".to_owned());
    buffer.set_single_cursor(4);

    assert_eq!(buffer.matching_brackets_including_enclosing(), vec![(2, 5)]);
}

#[test]
fn bracket_colors_track_visible_nesting_depth() {
    let buffer = TextBuffer::from_text(1, None, "fn main() { call([x]); }".to_owned());
    let colors = buffer.bracket_colors_for_range(0..buffer.len_chars());
    let pairs = colors
        .into_iter()
        .map(|color| (color.char_idx, color.depth))
        .collect::<Vec<_>>();
    assert_eq!(
        pairs,
        vec![
            (7, 0),
            (8, 0),
            (10, 0),
            (16, 1),
            (17, 2),
            (19, 2),
            (20, 1),
            (23, 0)
        ]
    );
}

#[test]
fn bracket_colors_can_use_independent_depth_per_bracket_type() {
    let buffer = TextBuffer::from_text(1, None, "({[]})".to_owned());
    let shared = buffer
        .bracket_colors_for_range(0..buffer.len_chars())
        .into_iter()
        .map(|color| color.depth)
        .collect::<Vec<_>>();
    let independent = buffer
        .bracket_colors_for_range_with_options(0..buffer.len_chars(), true)
        .into_iter()
        .map(|color| color.depth)
        .collect::<Vec<_>>();

    assert_eq!(shared, vec![0, 1, 2, 2, 1, 0]);
    assert_eq!(independent, vec![0, 0, 0, 0, 0, 0]);
}

#[test]
fn bracket_pair_guides_track_matched_pairs_with_depth() {
    let buffer = TextBuffer::from_text(1, None, "fn main() {\n  call([x]);\n}".to_owned());
    let guides = buffer
        .bracket_pair_guides()
        .into_iter()
        .map(|guide| (guide.open_idx, guide.close_idx, guide.depth))
        .collect::<Vec<_>>();

    assert_eq!(
        guides,
        vec![(7, 8, 0), (19, 21, 2), (18, 22, 1), (10, 25, 0)]
    );
}

#[test]
fn bracket_pair_guides_stop_at_bounded_parser_window() {
    let padding = " ".repeat(MAX_BRACKET_SCAN_CHARS);
    let buffer = TextBuffer::from_text(1, None, format!("{padding}()"));

    assert!(buffer.bracket_pair_guides().is_empty());
}

#[test]
fn auto_pair_inserts_closing_bracket_and_keeps_cursor_between() {
    let mut buffer = TextBuffer::from_text(1, None, String::new());

    assert!(buffer.insert_text_with_auto_pairs("("));
    assert_eq!(buffer.text(), "()");
    assert_eq!(buffer.cursor(), 1);
    assert!(buffer.undo());
    assert_eq!(buffer.text(), "");
    assert!(buffer.redo());
    assert_eq!(buffer.text(), "()");
    assert_eq!(buffer.cursor(), 1);
}

#[test]
fn auto_pair_inserts_quotes() {
    let mut buffer = TextBuffer::from_text(1, None, String::new());

    assert!(buffer.insert_text_with_auto_pairs("\""));
    assert_eq!(buffer.text(), "\"\"");
    assert_eq!(buffer.cursor(), 1);
}

#[test]
fn auto_pair_respects_language_configuration() {
    let mut rust = TextBuffer::from_text_with_language(1, None, String::new(), LanguageId::Rust);
    assert!(rust.insert_text_with_auto_pairs("("));
    assert_eq!(rust.text(), "()");

    let mut diff = TextBuffer::from_text_with_language(1, None, String::new(), LanguageId::Diff);
    assert!(diff.insert_text_with_auto_pairs("("));
    assert_eq!(diff.text(), "(");
    assert_eq!(diff.cursor(), 1);
}

#[test]
fn auto_pair_settings_can_disable_brackets() {
    let mut buffer = TextBuffer::from_text(1, None, String::new());

    assert!(buffer.insert_text_with_auto_pair_settings(
        "(",
        AutoPairSettings {
            brackets: false,
            ..AutoPairSettings::default()
        },
    ));
    assert_eq!(buffer.text(), "(");
    assert_eq!(buffer.cursor(), 1);
}

#[test]
fn auto_pair_settings_can_disable_quotes() {
    let mut buffer = TextBuffer::from_text(1, None, String::new());

    assert!(buffer.insert_text_with_auto_pair_settings(
        "\"",
        AutoPairSettings {
            quotes: false,
            ..AutoPairSettings::default()
        },
    ));
    assert_eq!(buffer.text(), "\"");
    assert_eq!(buffer.cursor(), 1);
}

#[test]
fn auto_pair_settings_can_disable_surrounding_selection() {
    let mut buffer = TextBuffer::from_text(1, None, "value".to_owned());
    buffer.set_selection(0, 5);

    assert!(buffer.insert_text_with_auto_pair_settings(
        "(",
        AutoPairSettings {
            surround: false,
            ..AutoPairSettings::default()
        },
    ));
    assert_eq!(buffer.text(), "(");
    assert_eq!(buffer.cursor(), 1);
}

#[test]
fn auto_pair_skips_existing_closing_character() {
    let mut buffer = TextBuffer::from_text(1, None, "()".to_owned());
    buffer.set_single_cursor(1);

    assert!(!buffer.insert_text_with_auto_pairs(")"));
    assert_eq!(buffer.text(), "()");
    assert_eq!(buffer.cursor(), 2);
}

#[test]
fn auto_pair_overtype_setting_can_disable_skipping_existing_close() {
    let mut buffer = TextBuffer::from_text(1, None, "()".to_owned());
    buffer.set_single_cursor(1);

    assert!(buffer.insert_text_with_auto_pair_settings(
        ")",
        AutoPairSettings {
            overtype: false,
            ..AutoPairSettings::default()
        },
    ));
    assert_eq!(buffer.text(), "())");
    assert_eq!(buffer.cursor(), 2);
}

#[test]
fn auto_pair_skip_close_respects_language_configuration() {
    let mut diff = TextBuffer::from_text_with_language(1, None, "()".to_owned(), LanguageId::Diff);
    diff.set_single_cursor(1);

    assert!(diff.insert_text_with_auto_pairs(")"));
    assert_eq!(diff.text(), "())");
    assert_eq!(diff.cursor(), 2);
}

#[test]
fn auto_pair_close_outdents_blank_line_to_matching_opener() {
    let mut buffer = TextBuffer::from_text_with_language(
        1,
        None,
        "fn main() {\n    if ready {\n        \n".to_owned(),
        LanguageId::Rust,
    );
    buffer.set_single_cursor(buffer.line_column_to_char(2, 8));

    assert!(buffer.insert_text_with_auto_pairs("}"));

    assert_eq!(buffer.text(), "fn main() {\n    if ready {\n    }\n");
    assert_eq!(buffer.cursor_position().line, 2);
    assert_eq!(buffer.cursor_position().column, 5);
    assert!(buffer.undo());
    assert_eq!(buffer.text(), "fn main() {\n    if ready {\n        \n");
}

#[test]
fn auto_pair_close_outdent_skips_nonblank_lines() {
    let mut buffer = TextBuffer::from_text_with_language(
        1,
        None,
        "fn main() {\n    call".to_owned(),
        LanguageId::Rust,
    );
    buffer.set_single_cursor(buffer.len_chars());

    assert!(buffer.insert_text_with_auto_pairs("}"));

    assert_eq!(buffer.text(), "fn main() {\n    call}");
    assert_eq!(buffer.cursor_position().column, 9);
}

#[test]
fn auto_pair_applies_to_multiple_cursors() {
    let mut buffer = TextBuffer::from_text(1, None, "ab".to_owned());
    buffer.set_cursors([0, 2]);

    assert!(buffer.insert_text_with_auto_pairs("["));
    assert_eq!(buffer.text(), "[]ab[]");
    assert_eq!(
        buffer
            .cursor_positions()
            .into_iter()
            .map(|pos| pos.char_idx)
            .collect::<Vec<_>>(),
        vec![1, 5]
    );
}

#[test]
fn auto_pair_surrounds_selected_text() {
    let mut buffer = TextBuffer::from_text(1, None, "value".to_owned());
    buffer.set_selection(0, 5);

    assert!(buffer.insert_text_with_auto_pairs("("));
    assert_eq!(buffer.text(), "(value)");
    assert_eq!(
        buffer.selections(),
        &[Selection {
            anchor: 1,
            cursor: 6
        }]
    );
    assert!(buffer.undo());
    assert_eq!(buffer.text(), "value");
    assert_eq!(
        buffer.selections(),
        &[Selection {
            anchor: 0,
            cursor: 5
        }]
    );
    assert!(buffer.redo());
    assert_eq!(buffer.text(), "(value)");
    assert_eq!(
        buffer.selections(),
        &[Selection {
            anchor: 1,
            cursor: 6
        }]
    );
}

#[test]
fn auto_pair_surrounds_reversed_selection_and_preserves_direction() {
    let mut buffer = TextBuffer::from_text(1, None, "value".to_owned());
    buffer.set_selection(5, 0);

    assert!(buffer.insert_text_with_auto_pairs("\""));
    assert_eq!(buffer.text(), "\"value\"");
    assert_eq!(
        buffer.selections(),
        &[Selection {
            anchor: 6,
            cursor: 1
        }]
    );
}

#[test]
fn auto_pair_surrounds_multiple_selected_ranges() {
    let mut buffer = TextBuffer::from_text(1, None, "one two one".to_owned());
    buffer.set_selection(0, 3);
    assert!(buffer.select_next_occurrence());

    assert!(buffer.insert_text_with_auto_pairs("'"));
    assert_eq!(buffer.text(), "'one' two 'one'");
    assert_eq!(
        buffer.selections(),
        &[
            Selection {
                anchor: 1,
                cursor: 4
            },
            Selection {
                anchor: 11,
                cursor: 14
            }
        ]
    );
}

#[test]
fn paired_backspace_deletes_empty_auto_pair() {
    let mut buffer = TextBuffer::from_text(1, None, "()".to_owned());
    buffer.set_single_cursor(1);

    assert!(buffer.delete_backward());
    assert_eq!(buffer.text(), "");
    assert_eq!(buffer.cursor(), 0);
    assert!(buffer.undo());
    assert_eq!(buffer.text(), "()");
    assert_eq!(buffer.cursor(), 1);
    assert!(buffer.redo());
    assert_eq!(buffer.text(), "");
}

#[test]
fn paired_backspace_can_leave_closing_pair_when_auto_pair_delete_is_disabled() {
    let mut buffer = TextBuffer::from_text(1, None, "()".to_owned());
    buffer.set_single_cursor(1);

    assert!(buffer.delete_backward_with_auto_pair_delete(false));
    assert_eq!(buffer.text(), ")");
    assert_eq!(buffer.cursor(), 0);
}

#[test]
fn delete_forward_can_trim_next_line_indentation_when_joining_lines() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\n    beta\n".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(0, 5));

    assert!(buffer.delete_forward_with_trim_whitespace_on_delete());

    assert_eq!(buffer.text(), "alphabeta\n");
    assert_eq!(buffer.cursor_position().line, 0);
    assert_eq!(buffer.cursor_position().column, 5);
}

#[test]
fn delete_forward_trim_whitespace_falls_back_to_single_character_delete() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\n    beta\n".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(0, 2));

    assert!(buffer.delete_forward_with_trim_whitespace_on_delete());

    assert_eq!(buffer.text(), "alha\n    beta\n");
    assert_eq!(buffer.cursor_position().column, 2);
}

#[test]
fn paired_backspace_deletes_empty_quote_pair() {
    let mut buffer = TextBuffer::from_text(1, None, "\"\"".to_owned());
    buffer.set_single_cursor(1);

    assert!(buffer.delete_backward());
    assert_eq!(buffer.text(), "");
    assert_eq!(buffer.cursor(), 0);
}

#[test]
fn paired_backspace_respects_language_configuration() {
    let mut diff = TextBuffer::from_text_with_language(1, None, "()".to_owned(), LanguageId::Diff);
    diff.set_single_cursor(1);

    assert!(diff.delete_backward());
    assert_eq!(diff.text(), ")");
    assert_eq!(diff.cursor(), 0);
}

#[test]
fn paired_backspace_applies_to_multiple_cursors() {
    let mut buffer = TextBuffer::from_text(1, None, "()[]".to_owned());
    buffer.set_cursors([1, 3]);

    assert!(buffer.delete_backward());
    assert_eq!(buffer.text(), "");
    assert_eq!(
        buffer
            .cursor_positions()
            .into_iter()
            .map(|pos| pos.char_idx)
            .collect::<Vec<_>>(),
        vec![0]
    );
}

#[test]
fn multicursor_delete_backwards_applies_simultaneously() {
    let mut buffer = TextBuffer::from_text(1, None, "abcd".to_owned());
    buffer.set_cursors([1, 3]);
    assert!(buffer.delete_backward());
    assert_eq!(buffer.text(), "bd");
    assert_eq!(
        buffer
            .cursor_positions()
            .into_iter()
            .map(|pos| pos.char_idx)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
}

#[test]
fn multicursor_backspace_at_buffer_start_keeps_other_cursors() {
    let mut buffer = TextBuffer::from_text(1, None, "abcd".to_owned());
    buffer.set_cursors([0, 3]);

    assert!(buffer.delete_backward());

    assert_eq!(buffer.text(), "abd");
    assert_eq!(
        buffer
            .cursor_positions()
            .into_iter()
            .map(|pos| pos.char_idx)
            .collect::<Vec<_>>(),
        vec![0, 2]
    );
}

#[test]
fn multicursor_delete_forward_at_buffer_end_keeps_other_cursors() {
    let mut buffer = TextBuffer::from_text(1, None, "abcd".to_owned());
    buffer.set_cursors([1, 4]);

    assert!(buffer.delete_forward());

    assert_eq!(buffer.text(), "acd");
    assert_eq!(
        buffer
            .cursor_positions()
            .into_iter()
            .map(|pos| pos.char_idx)
            .collect::<Vec<_>>(),
        vec![1, 3]
    );
}

#[test]
fn undo_restores_adjacent_multicursor_deletes_in_original_order() {
    let mut buffer = TextBuffer::from_text(1, None, "abcd".to_owned());
    buffer.set_cursors([1, 2]);

    assert!(buffer.delete_backward());
    assert_eq!(buffer.text(), "cd");
    assert_eq!(buffer.selections(), &[Selection::caret(0)]);

    assert!(buffer.undo());
    assert_eq!(buffer.text(), "abcd");
    assert!(buffer.redo());
    assert_eq!(buffer.text(), "cd");
}

#[test]
fn save_cleanup_trims_trailing_whitespace_and_final_newlines() {
    assert_eq!(
        clean_text_for_save("one  \ntwo\t\n\n", true, true, true),
        "one\ntwo\n"
    );
    assert_eq!(
        clean_text_for_save("one  \r\ntwo\t\r\n\r\n", true, true, true),
        "one\r\ntwo\r\n"
    );
}

#[test]
fn save_cleanup_can_insert_final_newline() {
    assert_eq!(clean_text_for_save("one", false, true, false), "one\n");
    assert_eq!(clean_text_for_save("", false, true, false), "");
}

#[test]
fn buffer_save_cleanup_is_undoable_and_updates_text() {
    let mut buffer = TextBuffer::from_text(1, None, "one  \n\n".to_owned());
    assert!(buffer.apply_save_cleanup(true, true, true));
    assert_eq!(buffer.text(), "one\n");
    assert!(buffer.undo());
    assert_eq!(buffer.text(), "one  \n\n");
}

#[test]
fn buffer_save_cleanup_preserves_crlf_preference() {
    let mut buffer = TextBuffer::from_text(1, None, "one  \r\ntwo\t\r\n\r\n".to_owned());

    assert!(buffer.apply_save_cleanup(true, true, true));
    assert_eq!(buffer.text(), "one\r\ntwo\r\n");
}

#[test]
fn buffer_save_cleanup_skips_when_disabled() {
    let mut buffer = TextBuffer::from_text(1, None, "one  ".to_owned());

    assert!(!buffer.apply_save_cleanup(false, false, false));
    assert_eq!(buffer.text(), "one  ");
}
