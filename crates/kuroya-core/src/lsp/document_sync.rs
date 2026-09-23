use serde_json::Value;

/// Negotiated `textDocumentSync` change kind for a language server.
///
/// Parsed from `initialize` result `capabilities.textDocumentSync`, which is
/// either a bare [`TextDocumentSyncKind`] number or a
/// `TextDocumentSyncOptions` object with a `change` number. Anything that
/// cannot be determined falls back to [`TextDocumentSyncKindSetting::Full`],
/// which matches the full-document sync the client historically performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextDocumentSyncKindSetting {
    /// `TextDocumentSyncKind.None` (0): the server does not want content.
    None,
    /// `TextDocumentSyncKind.Full` (1): resend the whole document. Also the
    /// fallback when the sync kind is absent or unparseable.
    #[default]
    Full,
    /// `TextDocumentSyncKind.Incremental` (2): send ranged change events.
    Incremental,
}

/// Parses the `capabilities.textDocumentSync` JSON value into a sync setting.
///
/// Tolerant by design: numbers outside the spec (or non-numbers where a
/// number is expected) map to [`TextDocumentSyncKindSetting::Full`]. An
/// options object without a `change` field also maps to `Full` so unknown
/// servers keep today's full-document behavior.
pub fn parse_text_document_sync_kind(value: &Value) -> TextDocumentSyncKindSetting {
    match value
        .as_u64()
        .or_else(|| value.get("change").and_then(Value::as_u64))
    {
        Some(0) => TextDocumentSyncKindSetting::None,
        Some(2) => TextDocumentSyncKindSetting::Incremental,
        // 1, absent, out-of-range, and type-mismatched values all mean
        // "sync with full documents", the pre-negotiation behavior.
        _ => TextDocumentSyncKindSetting::Full,
    }
}

/// A single ranged LSP content change event.
///
/// The range is expressed in the text as it existed *before* the change,
/// with zero-based `line`s and zero-based `character`s counted in UTF-16
/// code units, exactly as LSP positions require.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentChange {
    pub start_line: usize,
    pub start_character: usize,
    pub end_line: usize,
    pub end_character: usize,
    pub text: String,
}

/// Computes the smallest ranged content change turning `old_text` into
/// `new_text`, or `None` when the texts are identical (a version bump with
/// unchanged text needs no `contentChanges` entry; LSP servers ignore no-op
/// edits, so the notification can be skipped entirely).
///
/// The diff trims the common character prefix and suffix, so its cost is
/// O(prefix + suffix + changed span) plus one scan of `old_text` up to the
/// end of the replaced range to build (line, UTF-16 character) positions.
/// Comparisons are byte-exact, so `\r\n` line endings participate in the
/// diff like any other content; the editor normalizes to `\n`, making mixed
/// endings rare in practice.
pub fn text_document_content_change_event(old_text: &str, new_text: &str) -> Option<ContentChange> {
    if old_text == new_text {
        return None;
    }

    let prefix_len = common_char_prefix_len(old_text, new_text);
    let (old_suffix_len, new_suffix_len) =
        common_char_suffix_lens(&old_text[prefix_len..], &new_text[prefix_len..]);

    let start_byte = prefix_len;
    let end_byte = old_text.len() - old_suffix_len;
    let text = new_text[prefix_len..new_text.len() - new_suffix_len].to_owned();
    let ((start_line, start_character), (end_line, end_character)) =
        lsp_positions_at_bytes(old_text, start_byte, end_byte);

    Some(ContentChange {
        start_line,
        start_character,
        end_line,
        end_character,
        text,
    })
}

/// Byte length of the longest common character prefix. A pure append (the
/// whole old text being a prefix of the new text) therefore yields the full
/// old length, which becomes an empty range at end-of-old.
fn common_char_prefix_len(old_text: &str, new_text: &str) -> usize {
    old_text
        .char_indices()
        .zip(new_text.chars())
        .take_while(|((_, old_char), new_char)| old_char == new_char)
        .map(|((index, old_char), _)| index + old_char.len_utf8())
        .last()
        .unwrap_or(0)
}

/// Byte lengths of the longest common character suffix within the tails that
/// follow the common prefix, so the suffix can never overlap the prefix.
fn common_char_suffix_lens(old_tail: &str, new_tail: &str) -> (usize, usize) {
    let mut old_end = old_tail.len();
    let mut new_end = new_tail.len();
    while old_end > 0 && new_end > 0 {
        let old_char = old_tail[..old_end].chars().next_back().unwrap_or('\0');
        let new_char = new_tail[..new_end].chars().next_back().unwrap_or('\0');
        if old_char != new_char {
            break;
        }
        old_end -= old_char.len_utf8();
        new_end -= new_char.len_utf8();
    }
    (old_tail.len() - old_end, new_tail.len() - new_end)
}

/// Converts two byte offsets of `text` (character boundaries,
/// `start_byte <= end_byte`) into zero-based (line, UTF-16 character)
/// positions in a single pass. Offsets at end-of-text resolve to the
/// position just past the final character.
fn lsp_positions_at_bytes(
    text: &str,
    start_byte: usize,
    end_byte: usize,
) -> ((usize, usize), (usize, usize)) {
    debug_assert!(start_byte <= end_byte && end_byte <= text.len());
    debug_assert!(text.is_char_boundary(start_byte));
    debug_assert!(text.is_char_boundary(end_byte));

    let mut line = 0;
    let mut character = 0;
    let mut start = None;
    let mut end = None;
    for (index, char) in text.char_indices() {
        if index == start_byte {
            start = Some((line, character));
        }
        if index == end_byte {
            end = Some((line, character));
            break;
        }
        if char == '\n' {
            line += 1;
            character = 0;
        } else {
            character += char.len_utf16();
        }
    }

    // Offsets at end-of-text never match a char index.
    let tail_position = (line, character);
    (start.unwrap_or(tail_position), end.unwrap_or(tail_position))
}

#[cfg(test)]
mod tests {
    use super::{
        ContentChange, TextDocumentSyncKindSetting, parse_text_document_sync_kind,
        text_document_content_change_event,
    };
    use serde_json::{Value, json};

    #[test]
    fn sync_kind_parses_bare_numbers() {
        assert_eq!(
            parse_text_document_sync_kind(&json!(0)),
            TextDocumentSyncKindSetting::None
        );
        assert_eq!(
            parse_text_document_sync_kind(&json!(1)),
            TextDocumentSyncKindSetting::Full
        );
        assert_eq!(
            parse_text_document_sync_kind(&json!(2)),
            TextDocumentSyncKindSetting::Incremental
        );
    }

    #[test]
    fn sync_kind_parses_options_objects() {
        assert_eq!(
            parse_text_document_sync_kind(&json!({"change": 2})),
            TextDocumentSyncKindSetting::Incremental
        );
        assert_eq!(
            parse_text_document_sync_kind(&json!({"change": 0, "openClose": true})),
            TextDocumentSyncKindSetting::None
        );
    }

    #[test]
    fn sync_kind_defaults_to_full_for_missing_or_invalid_values() {
        let missing_change = json!({"openClose": true});
        assert_eq!(
            parse_text_document_sync_kind(&missing_change),
            TextDocumentSyncKindSetting::Full
        );
        for invalid in [
            Value::Null,
            json!("incremental"),
            json!(-1),
            json!(3),
            json!({"change": "2"}),
            json!([]),
        ] {
            assert_eq!(
                parse_text_document_sync_kind(&invalid),
                TextDocumentSyncKindSetting::Full,
                "unexpected parse of {invalid}"
            );
        }
    }

    #[test]
    fn content_change_is_none_for_identical_text() {
        assert_eq!(
            text_document_content_change_event("same\n_text", "same\n_text"),
            None
        );
    }

    #[test]
    fn content_change_handles_mid_line_insertion() {
        let change = text_document_content_change_event("hello world", "hello brave world")
            .expect("insertion should produce a change");

        assert_eq!(
            change,
            ContentChange {
                start_line: 0,
                start_character: 6,
                end_line: 0,
                end_character: 6,
                text: "brave ".to_owned()
            }
        );
    }

    #[test]
    fn content_change_handles_pure_append_at_end_of_text() {
        let change = text_document_content_change_event("abc", "abcdef")
            .expect("append should produce a change");

        assert_eq!(
            change,
            ContentChange {
                start_line: 0,
                start_character: 3,
                end_line: 0,
                end_character: 3,
                text: "def".to_owned()
            }
        );
    }

    #[test]
    fn content_change_handles_new_line_insertion() {
        let change = text_document_content_change_event("ab", "ab\ncd")
            .expect("line insertion should produce a change");

        assert_eq!(
            change,
            ContentChange {
                start_line: 0,
                start_character: 2,
                end_line: 0,
                end_character: 2,
                text: "\ncd".to_owned()
            }
        );
    }

    #[test]
    fn content_change_handles_deletion_and_truncation_to_empty() {
        let deletion = text_document_content_change_event("hello world", "hello")
            .expect("deletion should produce a change");
        assert_eq!(
            deletion,
            ContentChange {
                start_line: 0,
                start_character: 5,
                end_line: 0,
                end_character: 11,
                text: String::new()
            }
        );

        let truncation = text_document_content_change_event("abc", "")
            .expect("truncation should produce a change");
        assert_eq!(
            truncation,
            ContentChange {
                start_line: 0,
                start_character: 0,
                end_line: 0,
                end_character: 3,
                text: String::new()
            }
        );

        let emptied_lines = text_document_content_change_event("one\ntwo\n", "one\n");
        assert_eq!(
            emptied_lines,
            Some(ContentChange {
                start_line: 1,
                start_character: 0,
                end_line: 2,
                end_character: 0,
                text: String::new()
            })
        );
    }

    #[test]
    fn content_change_handles_multi_line_replacement() {
        let change = text_document_content_change_event("a\nbc\nd", "a\nXY\nd")
            .expect("replacement should produce a change");

        assert_eq!(
            change,
            ContentChange {
                start_line: 1,
                start_character: 0,
                end_line: 1,
                end_character: 2,
                text: "XY".to_owned()
            }
        );
    }

    #[test]
    fn content_change_counts_astral_characters_as_utf16_units() {
        // U+1F980 CRAB is one char but two UTF-16 code units, so 'b' sits at
        // UTF-16 column 3 and the replacement range covers column 3..4.
        let change = text_document_content_change_event("a🦀b", "a🦀c")
            .expect("emoji edit should produce a change");
        assert_eq!(
            change,
            ContentChange {
                start_line: 0,
                start_character: 3,
                end_line: 0,
                end_character: 4,
                text: "c".to_owned()
            }
        );

        let change_after_emoji = text_document_content_change_event("😀ab", "😀ad")
            .expect("emoji prefix edit should produce a change");
        assert_eq!(
            change_after_emoji,
            ContentChange {
                start_line: 0,
                start_character: 3,
                end_line: 0,
                end_character: 4,
                text: "d".to_owned()
            }
        );
    }

    #[test]
    fn content_change_treats_crlf_as_plain_content() {
        // Byte-exact comparison: the terminator's `\r` participates in the
        // diff. The editor is LF-normalized, so this only guards against
        // position drift for externally supplied CRLF text.
        let change = text_document_content_change_event("a\r\nb", "a\r\nc")
            .expect("CRLF edit should produce a change");

        assert_eq!(
            change,
            ContentChange {
                start_line: 1,
                start_character: 0,
                end_line: 1,
                end_character: 1,
                text: "c".to_owned()
            }
        );
    }

    #[test]
    fn content_change_handles_first_edit_to_empty_document() {
        let change = text_document_content_change_event("", "hello")
            .expect("insertion into empty text should produce a change");

        assert_eq!(
            change,
            ContentChange {
                start_line: 0,
                start_character: 0,
                end_line: 0,
                end_character: 0,
                text: "hello".to_owned()
            }
        );
    }

    #[test]
    fn content_change_reroutes_replacement_of_entire_text() {
        let change = text_document_content_change_event("old\nlines", "brand new")
            .expect("full replacement should produce a change");

        assert_eq!(
            change,
            ContentChange {
                start_line: 0,
                start_character: 0,
                end_line: 1,
                end_character: 5,
                text: "brand new".to_owned()
            }
        );
    }
}
