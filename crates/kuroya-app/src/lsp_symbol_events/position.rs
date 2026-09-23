use crate::lsp_text_positions::{
    lsp_line_content_utf16_len, lsp_one_based_utf16_position_to_buffer_char,
    lsp_one_based_utf16_span_to_buffer_char_range,
};
use kuroya_core::TextBuffer;

pub(super) fn lsp_position_within_buffer(buffer: &TextBuffer, line: usize, column: usize) -> bool {
    lsp_one_based_utf16_position_to_buffer_char(buffer, line, column).is_some()
}

/// Validates a one-based LSP span for intake, clamping to the first line.
///
/// Semantic tokens may span multiple lines (e.g. block comments), so a length
/// that reaches past the token's line is clamped to the line content instead
/// of rejecting the whole token; rendering caps the highlight to the line as
/// well. The span is accepted when its start is a representable position on
/// the line and at least one UTF-16 unit remains after clamping.
pub(super) fn lsp_span_within_buffer(
    buffer: &TextBuffer,
    line: usize,
    column: usize,
    length: usize,
) -> bool {
    if length == 0 || line == 0 {
        return false;
    }
    let Some(line_utf16_len) = lsp_line_content_utf16_len(buffer, line - 1) else {
        return false;
    };
    let start_utf16 = column.saturating_sub(1);
    let clamped_length = length.min(line_utf16_len.saturating_sub(start_utf16));
    lsp_one_based_utf16_span_to_buffer_char_range(buffer, line, column, clamped_length).is_some()
}

#[cfg(test)]
mod tests {
    use super::{lsp_position_within_buffer, lsp_span_within_buffer};
    use kuroya_core::TextBuffer;

    #[test]
    fn lsp_positions_allow_line_end_but_reject_missing_lines() {
        let buffer = TextBuffer::from_text(1, None, "alpha\nbeta".to_owned());

        assert!(lsp_position_within_buffer(&buffer, 1, 1));
        assert!(lsp_position_within_buffer(&buffer, 1, 6));
        assert!(lsp_position_within_buffer(&buffer, 2, 5));
        assert!(!lsp_position_within_buffer(&buffer, 0, 1));
        assert!(!lsp_position_within_buffer(&buffer, 1, 0));
        assert!(!lsp_position_within_buffer(&buffer, 1, 7));
        assert!(!lsp_position_within_buffer(&buffer, 3, 1));
    }

    #[test]
    fn lsp_positions_use_utf16_columns() {
        let buffer = TextBuffer::from_text(1, None, "😀x".to_owned());

        assert!(lsp_position_within_buffer(&buffer, 1, 1));
        assert!(lsp_position_within_buffer(&buffer, 1, 3));
        assert!(lsp_position_within_buffer(&buffer, 1, 4));
        assert!(!lsp_position_within_buffer(&buffer, 1, 2));
        assert!(!lsp_position_within_buffer(&buffer, 1, 5));
    }

    #[test]
    fn lsp_spans_clamp_multi_line_lengths_to_the_first_line() {
        let buffer = TextBuffer::from_text(1, None, "alpha\nbeta".to_owned());

        assert!(lsp_span_within_buffer(&buffer, 1, 1, 5));
        assert!(lsp_span_within_buffer(&buffer, 2, 2, 3));
        // Multi-line spans (block comments) clamp to their first line instead
        // of being rejected at intake.
        assert!(lsp_span_within_buffer(&buffer, 1, 1, 6));
        assert!(lsp_span_within_buffer(&buffer, 1, 3, 99));
        // A start at the line end clamps to an empty span and is rejected.
        assert!(!lsp_span_within_buffer(&buffer, 1, 6, 1));
        assert!(!lsp_span_within_buffer(&buffer, 2, 2, 0));
        assert!(!lsp_span_within_buffer(&buffer, 3, 1, 1));
    }

    #[test]
    fn lsp_spans_use_utf16_lengths() {
        let buffer = TextBuffer::from_text(1, None, "😀x".to_owned());

        assert!(lsp_span_within_buffer(&buffer, 1, 1, 2));
        assert!(lsp_span_within_buffer(&buffer, 1, 3, 1));
        assert!(!lsp_span_within_buffer(&buffer, 1, 1, 1));
        assert!(!lsp_span_within_buffer(&buffer, 1, 2, 1));
    }
}
