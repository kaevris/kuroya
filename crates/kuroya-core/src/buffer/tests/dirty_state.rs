//! Tests for the content-derived dirty flag backed by the saved baseline
//! (`mark_saved` / `recompute_dirty_after_mutation`) and the provenance
//! override (`mark_dirty`).

use super::*;

#[test]
fn typing_is_dirty_and_undo_back_to_saved_text_is_clean() {
    let mut buffer = TextBuffer::from_text(1, None, "hello".to_owned());
    assert!(!buffer.is_dirty());

    buffer.insert_at_cursor("x");
    assert_eq!(buffer.text(), "xhello");
    assert!(buffer.is_dirty());

    assert!(buffer.undo());
    assert_eq!(buffer.text(), "hello");
    assert!(!buffer.is_dirty());
}

#[test]
fn redo_after_undo_to_saved_text_is_dirty_again() {
    let mut buffer = TextBuffer::from_text(1, None, "hello".to_owned());
    buffer.insert_at_cursor("x");
    assert!(buffer.undo());
    assert!(!buffer.is_dirty());

    assert!(buffer.redo());
    assert_eq!(buffer.text(), "xhello");
    assert!(buffer.is_dirty());
}

#[test]
fn mark_saved_rebases_clean_state_until_next_content_change() {
    let mut buffer = TextBuffer::from_text(1, None, "hello".to_owned());
    buffer.insert_at_cursor("x");
    assert!(buffer.is_dirty());

    buffer.mark_saved();
    assert!(!buffer.is_dirty());

    // The cursor sits after the previously inserted "x", so "y" lands there.
    buffer.insert_at_cursor("y");
    assert_eq!(buffer.text(), "xyhello");
    assert!(buffer.is_dirty());

    // Delete the "y" with a separate (non-coalescing) transaction so the text
    // returns exactly to the saved baseline.
    buffer.delete_backward();
    assert_eq!(buffer.text(), "xhello");
    // Content matches the saved baseline again — the dirty flag clears.
    assert!(!buffer.is_dirty());
}

#[test]
fn edit_that_restores_saved_text_becomes_clean() {
    let mut buffer = TextBuffer::from_text(1, None, "abc".to_owned());
    buffer.mark_saved();

    buffer.insert_at_cursor("x");
    assert_eq!(buffer.text(), "xabc");
    assert!(buffer.is_dirty());

    buffer.apply_edit(TextEdit {
        range: 0..1,
        inserted: String::new(),
    });
    assert_eq!(buffer.text(), "abc");
    assert!(!buffer.is_dirty());
}

#[test]
fn no_op_replacement_of_saved_text_stays_clean() {
    let mut buffer = TextBuffer::from_text(1, None, "abc".to_owned());
    buffer.mark_saved();

    buffer.apply_edit(TextEdit {
        range: 1..2,
        inserted: "b".to_owned(),
    });

    assert_eq!(buffer.text(), "abc");
    assert!(!buffer.is_dirty());
}

#[test]
fn cursor_offset_transaction_path_recomputes_dirty() {
    let mut buffer = TextBuffer::from_text(1, None, "ab".to_owned());
    buffer.mark_saved();

    assert!(buffer.insert_texts_at_cursors(vec!["x".to_owned()]));
    assert_eq!(buffer.text(), "xab");
    assert!(buffer.is_dirty());

    assert!(buffer.undo());
    assert!(!buffer.is_dirty());
}

#[test]
fn mark_dirty_forces_dirty_even_when_content_matches_baseline() {
    let mut buffer = TextBuffer::from_text(1, None, "abc".to_owned());
    buffer.mark_saved();
    assert!(!buffer.is_dirty());

    // Provenance-based override: forced dirty wins even though the content
    // still matches the saved baseline.
    buffer.mark_dirty();
    assert!(buffer.is_dirty());

    // The forced flag persists until the next content mutation recomputes
    // the content-derived value.
    buffer.insert_at_cursor("z");
    assert!(buffer.is_dirty());
    assert!(buffer.undo());
    assert!(!buffer.is_dirty());
}

#[test]
fn mark_saved_if_text_matches_adopts_baseline_only_on_equal_text() {
    let mut buffer = TextBuffer::from_text(1, None, "recovered".to_owned());
    buffer.mark_dirty();
    assert!(buffer.is_dirty());

    assert!(!buffer.mark_saved_if_text_matches("other"));
    assert!(buffer.is_dirty());

    assert!(buffer.mark_saved_if_text_matches("recovered"));
    assert!(!buffer.is_dirty());

    // The adopted baseline drives later content-derived transitions.
    buffer.insert_at_cursor("!");
    assert!(buffer.is_dirty());
    assert!(buffer.undo());
    assert!(!buffer.is_dirty());
}

#[test]
fn replace_from_disk_rebases_saved_state_to_disk_text() {
    let mut buffer = TextBuffer::from_text(1, None, "old".to_owned());
    buffer.insert_at_cursor("dirty");
    assert!(buffer.is_dirty());
    let previous_version = buffer.version();

    buffer.replace_from_disk("fresh".to_owned());
    assert_eq!(buffer.text(), "fresh");
    assert!(!buffer.is_dirty());
    assert!(buffer.version() > previous_version);

    buffer.insert_at_cursor("q");
    assert!(buffer.is_dirty());
    assert!(buffer.undo());
    assert!(!buffer.is_dirty());
}

#[test]
fn disk_buffer_replacement_rebases_saved_state() {
    let mut buffer = TextBuffer::from_text(1, None, "old".to_owned());
    assert!(buffer.replace_range(0..3, "dirty"));
    assert!(buffer.is_dirty());

    buffer.replace_from_disk_buffer(TextBuffer::from_text(2, None, "new".to_owned()));

    assert_eq!(buffer.text(), "new");
    assert!(!buffer.is_dirty());
    // Disk replacement intentionally clears undo history (the on-disk file is
    // the source of truth for a clean buffer), so there is nothing to undo.
    assert!(!buffer.undo());
    assert!(!buffer.is_dirty());
}
