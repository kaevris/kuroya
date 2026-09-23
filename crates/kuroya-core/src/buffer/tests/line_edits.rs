use super::*;

#[test]
fn typed_input_uses_rope_cursor() {
    let mut buffer = TextBuffer::from_text(1, None, "ac".to_owned());
    buffer.set_single_cursor(1);
    buffer.insert_at_cursor("b");
    assert_eq!(buffer.text(), "abc");
    assert_eq!(buffer.cursor(), 2);
    assert!(buffer.delete_backward());
    assert_eq!(buffer.text(), "ac");
}

#[test]
fn newline_reuses_current_indent() {
    let mut buffer = TextBuffer::from_text(1, None, "    let x = 1;".to_owned());
    buffer.set_single_cursor(buffer.len_chars());
    buffer.insert_newline_with_indent();
    assert_eq!(buffer.text(), "    let x = 1;\n    ");
}

#[test]
fn newline_indents_after_opening_bracket() {
    let mut buffer = TextBuffer::from_text(1, None, "    if ready {".to_owned());
    buffer.set_single_cursor(buffer.len_chars());
    buffer.insert_newline_with_indent_unit("  ");
    assert_eq!(buffer.text(), "    if ready {\n      ");
}

#[test]
fn newline_uses_language_specific_indent_rules() {
    let mut python =
        TextBuffer::from_text_with_language(1, None, "if ready:".to_owned(), LanguageId::Python);
    python.set_single_cursor(python.len_chars());
    python.insert_newline_with_indent_unit("    ");
    assert_eq!(python.text(), "if ready:\n    ");

    let mut plain = TextBuffer::from_text(1, None, "label:".to_owned());
    plain.set_single_cursor(plain.len_chars());
    plain.insert_newline_with_indent_unit("    ");
    assert_eq!(plain.text(), "label:\n");
}

#[test]
fn newline_splits_matching_bracket_pair() {
    let mut buffer = TextBuffer::from_text(1, None, "    fn main() {}".to_owned());
    buffer.set_single_cursor(buffer.len_chars() - 1);
    buffer.insert_newline_with_indent_unit("  ");
    assert_eq!(buffer.text(), "    fn main() {\n      \n    }");
    assert_eq!(buffer.cursor_position().line, 1);
    assert_eq!(buffer.cursor_position().column, 6);
}

#[test]
fn newline_can_use_per_cursor_indent_overrides() {
    let mut buffer = TextBuffer::from_text_with_language(
        1,
        None,
        "fn main() {\nlet x = 1;\n}".to_owned(),
        LanguageId::Rust,
    );
    buffer.set_single_cursor(buffer.line_content_end_char(1));
    buffer.insert_newline_with_indent_overrides("  ", &[Some("  ".to_owned())]);

    assert_eq!(buffer.text(), "fn main() {\nlet x = 1;\n  \n}");
    assert_eq!(buffer.cursor_position().line, 2);
    assert_eq!(buffer.cursor_position().column, 2);
}

#[test]
fn newline_in_crlf_buffer_inserts_crlf_without_stray_cr() {
    let mut buffer = TextBuffer::from_text(1, None, "abc\r\ndef".to_owned());
    buffer.set_single_cursor(buffer.line_content_end_char(0));

    buffer.insert_newline_with_indent();

    assert_eq!(buffer.text(), "abc\r\n\r\ndef");
}

#[test]
fn newline_at_content_end_in_crlf_buffer_appends_crlf() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\r\nbeta".to_owned());
    buffer.set_single_cursor(buffer.len_chars());

    buffer.insert_newline_with_indent();

    assert_eq!(buffer.text(), "alpha\r\nbeta\r\n");
}

#[test]
fn newline_mid_line_in_crlf_buffer_splits_line_with_crlf() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\r\nbeta gamma".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(1, 5));

    buffer.insert_newline_with_indent();

    assert_eq!(buffer.text(), "alpha\r\nbeta \r\ngamma");
}

#[test]
fn indent_lines_indents_selected_line_range() {
    let mut buffer =
        TextBuffer::from_text(1, None, "fn main() {\nlet x = 1;\nlet y = 2;\n}".to_owned());
    let start = buffer.line_column_to_char(1, 0);
    let end = buffer.line_content_end_char(2);
    buffer.set_selection(start, end);

    assert!(buffer.indent_lines("  "));
    assert_eq!(buffer.text(), "fn main() {\n  let x = 1;\n  let y = 2;\n}");
    assert_eq!(
        buffer.selected_text().as_deref(),
        Some("  let x = 1;\n  let y = 2;")
    );
    assert!(buffer.undo());
    assert_eq!(buffer.text(), "fn main() {\nlet x = 1;\nlet y = 2;\n}");
    assert!(buffer.redo());
    assert_eq!(buffer.text(), "fn main() {\n  let x = 1;\n  let y = 2;\n}");
}

#[test]
fn indent_lines_leaves_blank_and_whitespace_only_lines_empty() {
    let mut buffer = TextBuffer::from_text(1, None, "one\n\ntwo\n  \nthree".to_owned());
    buffer.select_all();

    assert!(buffer.indent_lines("    "));
    assert_eq!(buffer.text(), "    one\n\n    two\n  \n    three");

    assert!(buffer.undo());
    assert_eq!(buffer.text(), "one\n\ntwo\n  \nthree");
}

#[test]
fn outdent_lines_removes_tabs_or_up_to_indent_unit_spaces() {
    let mut buffer = TextBuffer::from_text(1, None, "\talpha\n    beta\n gamma".to_owned());
    buffer.select_all();

    assert!(buffer.outdent_lines("  "));
    assert_eq!(buffer.text(), "alpha\n  beta\ngamma");
    assert_eq!(
        buffer.selected_text().as_deref(),
        Some("alpha\n  beta\ngamma")
    );
}

#[test]
fn toggle_line_comments_comments_and_uncomments_selected_lines() {
    let mut buffer = TextBuffer::from_text(1, None, "fn main() {\n    println!();\n}\n".to_owned());
    let end = buffer.line_column_to_char(2, 0);
    buffer.set_selection(0, end);

    assert!(buffer.toggle_line_comments("//"));
    assert_eq!(buffer.text(), "// fn main() {\n    // println!();\n}\n");
    assert!(buffer.toggle_line_comments("//"));
    assert_eq!(buffer.text(), "fn main() {\n    println!();\n}\n");
}

#[test]
fn toggle_line_comments_skips_blank_lines_when_selection_has_code() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\n\n  beta\n".to_owned());
    buffer.select_all();

    assert!(buffer.toggle_line_comments("//"));
    assert_eq!(buffer.text(), "// alpha\n\n  // beta\n");
}

#[test]
fn toggle_line_comments_comments_blank_line_when_it_is_the_only_target() {
    let mut buffer = TextBuffer::from_text(1, None, "    \n".to_owned());
    buffer.set_single_cursor(0);

    assert!(buffer.toggle_line_comments("#"));
    assert_eq!(buffer.text(), "    # \n");
}

#[test]
fn toggle_line_comments_can_omit_space_after_comment_token() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\n  beta\n".to_owned());
    buffer.select_all();

    assert!(buffer.toggle_line_comments_with_options("//", false, true));
    assert_eq!(buffer.text(), "//alpha\n  //beta\n");
    assert!(buffer.toggle_line_comments_with_options("//", false, true));
    assert_eq!(buffer.text(), "alpha\n  beta\n");
}

#[test]
fn toggle_line_comments_can_include_empty_lines_with_code() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\n\n  beta\n".to_owned());
    buffer.select_all();

    assert!(buffer.toggle_line_comments_with_options("//", true, false));
    assert_eq!(buffer.text(), "// alpha\n// \n  // beta\n");
}

#[test]
fn toggle_line_comments_does_not_double_comment_mixed_selection() {
    let mut buffer = TextBuffer::from_text(1, None, "// alpha\nbeta\n".to_owned());
    buffer.select_all();

    assert!(buffer.toggle_line_comments("//"));
    assert_eq!(buffer.text(), "// alpha\n// beta\n");
}

#[test]
fn line_indent_preserves_multicursor_columns() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\nbeta\ngamma".to_owned());
    buffer.set_cursors([
        buffer.line_column_to_char(0, 0),
        buffer.line_column_to_char(1, 2),
    ]);

    assert!(buffer.indent_lines("    "));
    assert_eq!(buffer.text(), "    alpha\n    beta\ngamma");
    assert_eq!(
        buffer
            .cursor_positions()
            .into_iter()
            .map(|position| (position.line, position.column))
            .collect::<Vec<_>>(),
        vec![(0, 4), (1, 6)]
    );
}

#[test]
fn selection_ending_at_next_line_start_does_not_indent_next_line() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\nbeta\ngamma".to_owned());
    buffer.set_selection(
        buffer.line_column_to_char(0, 0),
        buffer.line_column_to_char(1, 0),
    );

    assert!(buffer.indent_lines("  "));
    assert_eq!(buffer.text(), "  alpha\nbeta\ngamma");
}

#[test]
fn delete_lines_removes_current_line_and_moves_cursor_to_next_line() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\nbeta\ngamma".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(1, 2));

    assert!(buffer.delete_lines());
    assert_eq!(buffer.text(), "alpha\ngamma");
    assert_eq!(buffer.cursor_position().line, 1);
    assert_eq!(buffer.cursor_position().column, 0);
    assert!(buffer.undo());
    assert_eq!(buffer.text(), "alpha\nbeta\ngamma");
    assert!(buffer.redo());
    assert_eq!(buffer.text(), "alpha\ngamma");
}

#[test]
fn delete_lines_removes_selected_line_range() {
    let mut buffer = TextBuffer::from_text(1, None, "a\nb\nc\nd".to_owned());
    buffer.set_selection(
        buffer.line_column_to_char(1, 0),
        buffer.line_content_end_char(2),
    );

    assert!(buffer.delete_lines());
    assert_eq!(buffer.text(), "a\nd");
    assert_eq!(buffer.cursor_position().line, 1);
    assert_eq!(buffer.cursor_position().column, 0);
}

#[test]
fn delete_lines_removes_previous_newline_for_last_line() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\nbeta".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(1, 1));

    assert!(buffer.delete_lines());
    assert_eq!(buffer.text(), "alpha");
    assert_eq!(buffer.cursor_position().line, 0);
    assert_eq!(buffer.cursor_position().column, 5);
}

#[test]
fn delete_lines_removes_previous_crlf_for_last_line() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\r\nbeta".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(1, 1));

    assert!(buffer.delete_lines());
    assert_eq!(buffer.text(), "alpha");
}

#[test]
fn join_lines_joins_current_line_with_next_and_trims_indent() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha   \n    beta\ngamma".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(0, 2));

    assert!(buffer.join_lines());
    assert_eq!(buffer.text(), "alpha beta\ngamma");
    assert_eq!(buffer.cursor_position().line, 0);
    assert_eq!(buffer.cursor_position().column, 2);
}

#[test]
fn join_lines_joins_selected_line_block() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\n  beta\n  gamma\ndelta".to_owned());
    buffer.set_selection(
        buffer.line_column_to_char(0, 0),
        buffer.line_content_end_char(2),
    );

    assert!(buffer.join_lines());
    assert_eq!(buffer.text(), "alpha beta gamma\ndelta");
}

#[test]
fn join_lines_avoids_spaces_around_bracket_punctuation() {
    let mut buffer = TextBuffer::from_text(1, None, "call(\n  value\n)".to_owned());
    buffer.set_single_cursor(0);

    assert!(buffer.join_lines());
    assert_eq!(buffer.text(), "call(value\n)");
    assert!(buffer.join_lines());
    assert_eq!(buffer.text(), "call(value)");
}

#[test]
fn join_lines_noops_on_final_line_without_next_line() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\nbeta".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(1, 0));

    assert!(!buffer.join_lines());
    assert_eq!(buffer.text(), "alpha\nbeta");
}

#[test]
fn duplicate_lines_copies_current_line_and_moves_cursor_to_copy() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\nbeta\ngamma".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(1, 1));

    assert!(buffer.duplicate_lines());
    assert_eq!(buffer.text(), "alpha\nbeta\nbeta\ngamma");
    assert_eq!(buffer.cursor_position().line, 2);
    assert_eq!(buffer.cursor_position().column, 1);
    assert!(buffer.undo());
    assert_eq!(buffer.text(), "alpha\nbeta\ngamma");
    assert!(buffer.redo());
    assert_eq!(buffer.text(), "alpha\nbeta\nbeta\ngamma");
}

#[test]
fn duplicate_lines_handles_last_line_without_trailing_newline() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\nbeta".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(1, 1));

    assert!(buffer.duplicate_lines());
    assert_eq!(buffer.text(), "alpha\nbeta\nbeta");
    assert_eq!(buffer.cursor_position().line, 2);
    assert_eq!(buffer.cursor_position().column, 1);
}

#[test]
fn move_lines_up_swaps_with_previous_line() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\nbeta\ngamma".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(1, 1));

    assert!(buffer.move_lines_up());
    assert_eq!(buffer.text(), "beta\nalpha\ngamma");
    assert_eq!(buffer.cursor_position().line, 0);
    assert_eq!(buffer.cursor_position().column, 1);
}

#[test]
fn move_lines_down_preserves_selected_block() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\nbeta\ngamma\ndelta".to_owned());
    buffer.set_selection(
        buffer.line_column_to_char(1, 0),
        buffer.line_content_end_char(2),
    );

    assert!(buffer.move_lines_down());
    assert_eq!(buffer.text(), "alpha\ndelta\nbeta\ngamma");
    assert_eq!(buffer.selected_text().as_deref(), Some("beta\ngamma"));
}

#[test]
fn move_exact_line_selection_down_keeps_selection_on_moved_line() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\nbeta\ngamma".to_owned());
    buffer.set_selection(
        buffer.line_column_to_char(1, 0),
        buffer.line_column_to_char(2, 0),
    );

    assert!(buffer.move_lines_down());
    assert_eq!(buffer.text(), "alpha\ngamma\nbeta");
    assert_eq!(buffer.selected_text().as_deref(), Some("beta"));
}

#[test]
fn line_move_boundaries_are_noops() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha\nbeta".to_owned());
    buffer.set_single_cursor(0);
    assert!(!buffer.move_lines_up());
    assert_eq!(buffer.text(), "alpha\nbeta");

    buffer.set_single_cursor(buffer.line_column_to_char(1, 0));
    assert!(!buffer.move_lines_down());
    assert_eq!(buffer.text(), "alpha\nbeta");
}
