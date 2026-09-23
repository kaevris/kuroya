use kuroya_core::TextBuffer;
use std::ops::Range;

mod delimited;
mod paragraph;
mod sentence;
mod word;

use self::delimited::{vim_block_text_object_range, vim_quote_text_object_range};
use self::paragraph::vim_paragraph_text_object_range;
use self::sentence::vim_sentence_text_object_range;
use self::word::{vim_inner_big_word_range, vim_inner_word_range, vim_outer_word_range};
use super::super::{EditorVimTextObjectKind, EditorVimTextObjectScope};

pub(in crate::editor_vim_key_events) fn vim_text_object_range(
    buffer: &mut TextBuffer,
    count: usize,
    scope: EditorVimTextObjectScope,
    kind: EditorVimTextObjectKind,
) -> Option<Range<usize>> {
    let original_cursor = buffer.cursor();
    let inner = match kind {
        EditorVimTextObjectKind::Word => vim_inner_word_range(buffer, count),
        EditorVimTextObjectKind::BigWord => vim_inner_big_word_range(buffer, count),
        EditorVimTextObjectKind::Block { open, close } => {
            return vim_block_text_object_range(buffer, count, scope, open, close);
        }
        EditorVimTextObjectKind::Quote { quote } => {
            return vim_quote_text_object_range(buffer, count, scope, quote);
        }
        EditorVimTextObjectKind::Paragraph => {
            return vim_paragraph_text_object_range(buffer, count, scope);
        }
        EditorVimTextObjectKind::Sentence => {
            return vim_sentence_text_object_range(buffer, count, scope);
        }
    }?;
    match scope {
        EditorVimTextObjectScope::Inner => Some(inner),
        EditorVimTextObjectScope::Outer => {
            Some(vim_outer_word_range(buffer, inner, original_cursor))
        }
    }
}

fn vim_line_start_char(buffer: &TextBuffer, line: usize) -> usize {
    if line >= buffer.len_lines() {
        buffer.len_chars()
    } else {
        buffer.line_column_to_char(line, 0)
    }
}
