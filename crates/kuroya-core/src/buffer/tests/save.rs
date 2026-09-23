use super::*;

#[test]
fn trim_final_newlines_strips_mixed_line_endings_and_reappends_preferred_once() {
    assert_eq!(
        clean_text_for_save("a\nb\r\n\r\n", false, false, true),
        "a\nb\r\n"
    );
    assert_eq!(
        clean_text_for_save("a\r\nb\n\n", false, false, true),
        "a\r\nb\n"
    );
    assert_eq!(clean_text_for_save("\n\r\n", false, false, true), "\n");
}

#[test]
fn buffer_save_cleanup_trims_mixed_final_newlines() {
    let mut buffer = TextBuffer::from_text(1, None, "a\r\nb\n\n\r\n".to_owned());

    assert!(buffer.apply_save_cleanup(false, false, true));
    assert_eq!(buffer.text(), "a\r\nb\r\n");
}

#[test]
fn preferred_line_ending_majority_vote() {
    let lf_majority = TextBuffer::from_text(1, None, "a\nb\nc\r\n".to_owned());
    assert_eq!(lf_majority.preferred_line_ending(), "\n");

    let crlf_majority = TextBuffer::from_text(2, None, "a\r\nb\r\nc\n".to_owned());
    assert_eq!(crlf_majority.preferred_line_ending(), "\r\n");
}

#[test]
fn preferred_line_ending_tie_breaks_with_first_line_ending() {
    let crlf_first = TextBuffer::from_text(3, None, "a\r\nb\n".to_owned());
    assert_eq!(crlf_first.preferred_line_ending(), "\r\n");

    let lf_first = TextBuffer::from_text(4, None, "a\nb\r\n".to_owned());
    assert_eq!(lf_first.preferred_line_ending(), "\n");

    assert_eq!(
        clean_text_for_save("a\r\nb\nc", false, true, false),
        "a\r\nb\nc\r\n"
    );
    assert_eq!(
        clean_text_for_save("a\nb\r\nc", false, true, false),
        "a\nb\r\nc\n"
    );
}

#[test]
fn preferred_line_ending_defaults_to_lf_for_empty_or_newline_free_buffers() {
    let empty = TextBuffer::from_text(5, None, String::new());
    assert_eq!(empty.preferred_line_ending(), "\n");

    let single_line = TextBuffer::from_text(6, None, "single".to_owned());
    assert_eq!(single_line.preferred_line_ending(), "\n");
}

#[test]
fn lone_trailing_cr_counts_as_final_newline_on_save() {
    assert_eq!(
        clean_text_for_save("hello\r", false, true, false),
        "hello\r"
    );
    assert_eq!(clean_text_for_save("hello\r", false, true, true), "hello\r");

    let mut buffer = TextBuffer::from_text(7, None, "hello\r".to_owned());
    assert!(!buffer.apply_save_cleanup(false, true, false));
    assert_eq!(buffer.text(), "hello\r");
}
