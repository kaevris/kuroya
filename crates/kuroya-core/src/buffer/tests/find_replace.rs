use super::*;
use crate::buffer::find::find_regex_memo_contains;

#[test]
fn find_matches_returns_char_ranges() {
    let buffer = TextBuffer::from_text(1, None, "alpha\nbeta \u{03b1} beta".to_owned());
    assert_eq!(buffer.find_matches("beta", 8), vec![6..10, 13..17]);
    assert_eq!(buffer.find_matches("\u{03b1}", 8), vec![11..12]);
    assert_eq!(buffer.find_matches("beta", 1), vec![6..10]);
}

#[test]
fn find_matches_preserves_non_overlapping_literal_behavior() {
    let buffer = TextBuffer::from_text(1, None, "aaaa".to_owned());

    assert_eq!(buffer.find_matches("aa", 8), vec![0..2, 2..4]);
}

#[test]
fn find_matches_prefix_scan_preserves_non_overlapping_matches() {
    let buffer = TextBuffer::from_text(1, None, "ababa".to_owned());

    assert_eq!(buffer.find_matches("aba", 8), vec![0..3]);
}

#[test]
fn find_matches_literal_queries_scan_across_rope_chunks_without_line_allocations() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha beta gamma".to_owned());
    assert!(buffer.replace_range(6..10, "b\u{00e9}ta"));

    assert_eq!(buffer.find_matches("b\u{00e9}ta", 8), vec![6..10]);
}

#[test]
fn find_matches_single_char_queries_scan_across_rope_chunks() {
    let mut buffer = TextBuffer::from_text(1, None, "abcabc".to_owned());
    assert!(buffer.replace_range(2..2, "\u{03b1}"));

    assert_eq!(buffer.find_matches("\u{03b1}", 8), vec![2..3]);
    assert_eq!(buffer.find_matches("a", 8), vec![0..1, 4..5]);
}

#[test]
fn find_matches_preserves_multiline_queries() {
    let buffer = TextBuffer::from_text(1, None, "foo\nbar foo\nbar".to_owned());

    assert_eq!(buffer.find_matches("foo\nbar", 8), vec![0..7, 8..15]);
}

#[test]
fn find_matches_multiline_queries_can_require_whole_words() {
    let buffer = TextBuffer::from_text(1, None, "a foo\nbar b xfoo\nbar".to_owned());

    assert_eq!(
        buffer.find_matches_with_options("foo\nbar", 8, true, true),
        vec![2..9]
    );
}

#[test]
fn find_matches_can_ignore_ascii_case() {
    let buffer = TextBuffer::from_text(1, None, "Alpha alpha ALPHA".to_owned());

    assert_eq!(
        buffer.find_matches_with_options("alpha", 8, false, false),
        vec![0..5, 6..11, 12..17]
    );
}

#[test]
fn find_matches_preallocates_for_small_match_limits() {
    assert_eq!(find_result_capacity(0), 0);
    assert_eq!(find_result_capacity(1), 1);
    assert_eq!(find_result_capacity(8), 8);
}

#[test]
fn find_matches_case_insensitive_preserves_ascii_only_folding() {
    let buffer = TextBuffer::from_text(1, None, "\u{00e9} E e".to_owned());

    assert_eq!(
        buffer.find_matches_with_options("e", 8, false, false),
        vec![2..3, 4..5]
    );
}

#[test]
fn find_matches_can_require_whole_words() {
    let buffer = TextBuffer::from_text(1, None, "alpha alphabet alpha_1 alpha".to_owned());

    assert_eq!(
        buffer.find_matches_with_options("alpha", 8, true, true),
        vec![0..5, 23..28]
    );
}

#[test]
fn regex_find_matches_return_char_ranges_and_respect_options() {
    let buffer = TextBuffer::from_text(1, None, "item-1 Item-22 item-x".to_owned());

    assert_eq!(
        buffer
            .find_regex_matches_with_options(r"item-\d+", 8, true, false)
            .unwrap(),
        vec![0..6]
    );
    assert_eq!(
        buffer
            .find_regex_matches_with_options(r"item-\d+", 8, false, false)
            .unwrap(),
        vec![0..6, 7..14]
    );
}

#[test]
fn regex_line_local_detection_keeps_line_break_capable_patterns_on_full_text_path() {
    assert!(regex_query_is_line_local(r"item-\d+"));
    assert!(!regex_query_is_line_local(r"foo\s+bar"));
    assert!(!regex_query_is_line_local(r"(?s)foo.*bar"));
}

#[test]
fn regex_find_matches_preallocates_for_small_match_limits() {
    let regex = Regex::new("a").unwrap();
    assert_eq!(
        regex_match_ranges(&regex, "a a a", false, 1, |_, _| true),
        vec![0..1]
    );
}

#[test]
fn regex_find_matches_line_local_queries_across_rope_lines() {
    let buffer = TextBuffer::from_text(1, None, "item-1\nitem-22\nitem-x".to_owned());

    assert_eq!(
        buffer
            .find_regex_matches_with_options(r"item-\d+", 8, true, false)
            .unwrap(),
        vec![0..6, 7..14]
    );
}

#[test]
fn regex_find_matches_line_local_offsets_include_unicode_and_crlf() {
    let buffer = TextBuffer::from_text(1, None, "\u{03b1}-0\r\nitem-22\r\nitem-333".to_owned());

    assert_eq!(
        buffer
            .find_regex_matches_with_options(r"item-\d+", 8, true, false)
            .unwrap(),
        vec![5..12, 14..22]
    );
}

#[test]
fn regex_find_matches_line_local_queries_across_split_rope_chunks() {
    let mut buffer = TextBuffer::from_text(1, None, "item-1\nitem-x".to_owned());
    assert!(buffer.replace_range(6..6, "\nitem-22"));

    assert_eq!(
        buffer
            .find_regex_matches_with_options(r"item-\d+", 8, true, false)
            .unwrap(),
        vec![0..6, 7..14]
    );
}

#[test]
fn regex_find_matches_preserve_multiline_regex_queries() {
    let buffer = TextBuffer::from_text(1, None, "foo\nbar foo bar foo\nbaz".to_owned());

    assert_eq!(
        buffer
            .find_regex_matches_with_options(r"foo\s+bar", 8, true, false)
            .unwrap(),
        vec![0..7, 8..15]
    );
    assert_eq!(
        buffer
            .find_regex_matches_with_options(r"(?s)foo.*bar", 8, true, false)
            .unwrap(),
        vec![0..15]
    );
}

#[test]
fn regex_full_text_fallback_is_bounded_for_large_buffers() {
    let prefix_line = format!("{}\n", "x".repeat(512));
    let prefix = prefix_line.repeat(REGEX_FULL_TEXT_MAX_BYTES / prefix_line.len() + 1);
    let match_start = prefix.chars().count();
    let large_text = format!("{prefix}foo\nbar");
    let buffer = TextBuffer::from_text(1, None, large_text);

    assert_eq!(
        buffer
            .find_regex_matches_with_options(r"foo\s+bar", 8, true, false)
            .unwrap(),
        Vec::<Range<usize>>::new()
    );
    assert_eq!(
        buffer
            .find_regex_matches_with_options(r"foo", 8, true, false)
            .unwrap(),
        vec![match_start..match_start + 3]
    );
}

#[test]
fn validate_find_regex_checks_syntax_without_buffer_text() {
    assert!(validate_find_regex(r"item-\d+", false).is_ok());
    assert!(validate_find_regex("(", true).is_err());
}

#[test]
fn find_regex_compilation_is_memoized_with_identical_match_ranges() {
    let buffer = TextBuffer::from_text(1, None, "a1 b2 a1 a4".to_owned());
    let query = r"a\d";

    let first = buffer
        .find_regex_matches_with_options(query, 8, true, false)
        .expect("regex should compile");
    assert_eq!(first, vec![0..2, 6..8, 9..11]);
    assert!(find_regex_memo_contains(query, true));

    let second = buffer
        .find_regex_matches_with_options(query, 8, true, false)
        .expect("regex should compile");
    assert_eq!(first, second);

    let case_insensitive = buffer
        .find_regex_matches_with_options(r"A\d", 8, false, false)
        .expect("regex should compile");
    assert_eq!(case_insensitive, first);
    assert!(find_regex_memo_contains(r"A\d", false));
}

#[test]
fn replace_range_replaces_selected_match() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha beta alpha".to_owned());
    assert!(buffer.replace_range(6..10, "gamma"));
    assert_eq!(buffer.text(), "alpha gamma alpha");
    assert!(buffer.undo());
    assert_eq!(buffer.text(), "alpha beta alpha");
}

#[test]
fn replace_all_matches_is_one_undo_step() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha beta alpha".to_owned());
    let replaced = buffer.replace_all_matches("alpha", "omega", true, false, 16);
    assert_eq!(replaced, 2);
    assert_eq!(buffer.text(), "omega beta omega");
    assert!(buffer.undo());
    assert_eq!(buffer.text(), "alpha beta alpha");
}

#[test]
fn replace_all_matches_respects_options() {
    let mut buffer = TextBuffer::from_text(1, None, "Alpha alpha alphabet alpha_1".to_owned());
    let replaced = buffer.replace_all_matches("alpha", "x", false, true, 16);
    assert_eq!(replaced, 2);
    assert_eq!(buffer.text(), "x x alphabet alpha_1");
}

#[test]
fn replace_all_matches_can_preserve_case() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha Alpha ALPHA mixedAlpha".to_owned());
    let replaced = buffer.replace_all_matches_with_options("alpha", "omega", false, true, 16, true);
    assert_eq!(replaced, 3);
    assert_eq!(buffer.text(), "omega Omega OMEGA mixedAlpha");

    let replacement_len = buffer.replace_range_with_options(0..5, "VALUE", true);
    assert_eq!(replacement_len, Some(5));
    assert_eq!(buffer.text(), "value Omega OMEGA mixedAlpha");
}

#[test]
fn regex_replace_expands_capture_groups_and_respects_scope() {
    let mut buffer = TextBuffer::from_text(1, None, "item-1 item-22 item-333".to_owned());
    let replaced = buffer
        .replace_all_regex_matches(r"item-(\d+)", "value-$1", true, false, Some(7..14), 16)
        .unwrap();
    assert_eq!(replaced, 1);
    assert_eq!(buffer.text(), "item-1 value-22 item-333");

    let replacement_len = buffer
        .replace_regex_match(0..6, r"item-(\d+)", "value-$1", true, false, false)
        .unwrap();
    assert_eq!(replacement_len, Some(7));
    assert_eq!(buffer.text(), "value-1 value-22 item-333");
}

#[test]
fn regex_replace_line_local_queries_across_rope_lines() {
    let mut buffer = TextBuffer::from_text(1, None, "item-1\nitem-22\nitem-x".to_owned());
    let replaced = buffer
        .replace_all_regex_matches(r"item-(\d+)", "value-$1", true, false, None, 16)
        .unwrap();
    assert_eq!(replaced, 2);
    assert_eq!(buffer.text(), "value-1\nvalue-22\nitem-x");
}

#[test]
fn regex_replace_line_local_scope_tracks_unicode_and_crlf_offsets() {
    let mut buffer = TextBuffer::from_text(1, None, "\u{03b1}-0\r\nitem-22\r\nitem-333".to_owned());
    let replaced = buffer
        .replace_all_regex_matches(r"item-(\d+)", "value-$1", true, false, Some(14..22), 16)
        .unwrap();

    assert_eq!(replaced, 1);
    assert_eq!(buffer.text(), "\u{03b1}-0\r\nitem-22\r\nvalue-333");
}

#[test]
fn regex_replace_preserves_multiline_regex_queries() {
    let mut buffer = TextBuffer::from_text(1, None, "foo\nbar foo bar".to_owned());
    let replaced = buffer
        .replace_all_regex_matches(r"foo\s+bar", "match", true, false, None, 16)
        .unwrap();
    assert_eq!(replaced, 2);
    assert_eq!(buffer.text(), "match match");
}

#[test]
fn regex_replace_full_text_fallback_is_bounded_for_large_buffers() {
    let prefix_line = format!("{}\n", "x".repeat(512));
    let prefix = prefix_line.repeat(REGEX_FULL_TEXT_MAX_BYTES / prefix_line.len() + 1);
    let match_start = prefix.chars().count();
    let large_text = format!("{prefix}foo\nbar item-1");
    let mut buffer = TextBuffer::from_text(1, None, large_text.clone());

    let replacement_len = buffer
        .replace_regex_match(
            match_start..match_start + 7,
            r"foo\s+bar",
            "match",
            true,
            false,
            false,
        )
        .unwrap();
    assert_eq!(replacement_len, None);
    assert_eq!(buffer.text(), large_text);

    let replaced = buffer
        .replace_all_regex_matches(r"foo\s+bar", "match", true, false, None, 16)
        .unwrap();
    assert_eq!(replaced, 0);
    assert_eq!(buffer.text(), large_text);

    let replaced = buffer
        .replace_all_regex_matches(r"item-(\d+)", "value-$1", true, false, None, 16)
        .unwrap();
    assert_eq!(replaced, 1);
    assert!(buffer.text().ends_with("foo\nbar value-1"));
}

#[test]
fn regex_replace_can_preserve_case_after_capture_expansion() {
    let mut buffer = TextBuffer::from_text(1, None, "item-1 Item-22 ITEM-333".to_owned());
    let replaced = buffer
        .replace_all_regex_matches_with_options(
            r"item-(\d+)",
            "value-$1",
            RegexReplaceAllOptions {
                case_sensitive: false,
                whole_word: false,
                scope: None,
                max_matches: 16,
                preserve_case: true,
            },
        )
        .unwrap();
    assert_eq!(replaced, 3);
    assert_eq!(buffer.text(), "value-1 Value-22 VALUE-333");
}

#[test]
fn find_match_ranges_map_to_cursor_lines() {
    let buffer = TextBuffer::from_text(1, None, "alpha\nbeta\ngamma beta".to_owned());
    let matches = buffer.find_matches("beta", 8);
    assert_eq!(
        matches
            .iter()
            .map(|range| buffer.char_position(range.start).line)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
}
