use super::brackets::auto_pair_matches;
use super::edits::{CharGroup, is_word_char_with_separators, normalize_selections};
use super::{DeleteCoalesceKind, Selection, TextBuffer, TextEdit};
use std::ops::Range;

impl TextBuffer {
    pub fn delete_backward(&mut self) -> bool {
        self.delete_backward_with_auto_pair_delete(true)
    }

    pub fn delete_backward_with_auto_pair_delete(&mut self, delete_auto_pairs: bool) -> bool {
        let language_config = self.language.configuration();
        let edits = self
            .selections
            .iter()
            .filter_map(|selection| {
                let range = selection.range();
                if !selection.is_caret() {
                    Some(range)
                } else if selection.cursor > 0 {
                    let start = self.previous_grapheme_boundary(selection.cursor);
                    let end = if delete_auto_pairs
                        && selection.cursor < self.len_chars()
                        && auto_pair_matches(
                            self.rope.char(start),
                            self.rope.char(selection.cursor),
                            language_config,
                        ) {
                        selection.cursor + 1
                    } else {
                        selection.cursor
                    };
                    Some(start..end)
                } else {
                    None
                }
            })
            .map(|range| TextEdit {
                range,
                inserted: String::new(),
            })
            .collect();
        self.apply_delete_transaction(edits, DeleteCoalesceKind::Backward)
    }

    pub fn delete_forward(&mut self) -> bool {
        let edits = self
            .selections
            .iter()
            .filter_map(|selection| {
                let range = selection.range();
                if !selection.is_caret() {
                    Some(range)
                } else if selection.cursor < self.len_chars() {
                    Some(selection.cursor..self.next_grapheme_boundary(selection.cursor))
                } else {
                    None
                }
            })
            .map(|range| TextEdit {
                range,
                inserted: String::new(),
            })
            .collect();
        self.apply_delete_transaction(edits, DeleteCoalesceKind::Forward)
    }

    pub fn delete_forward_with_trim_whitespace_on_delete(&mut self) -> bool {
        let edits = self
            .selections
            .iter()
            .filter_map(|selection| {
                let range = selection.range();
                if !selection.is_caret() {
                    Some(range)
                } else if selection.cursor < self.len_chars() {
                    self.trim_whitespace_on_delete_range(selection.cursor)
                        .or_else(|| Some(selection.cursor..selection.cursor + 1))
                } else {
                    None
                }
            })
            .map(|range| TextEdit {
                range,
                inserted: String::new(),
            })
            .collect();
        self.apply_transaction(edits)
    }

    fn trim_whitespace_on_delete_range(&self, cursor: usize) -> Option<Range<usize>> {
        let position = self.char_position(cursor);
        if position.line + 1 >= self.len_lines()
            || cursor != self.line_content_end_char(position.line)
        {
            return None;
        }

        let next_content_start = self.line_first_non_whitespace_char(position.line + 1);
        (next_content_start > cursor).then_some(cursor..next_content_start)
    }

    pub fn delete_word_backward(&mut self) -> bool {
        let edits = self
            .selections
            .iter()
            .filter_map(|selection| {
                let range = selection.range();
                if !selection.is_caret() {
                    Some(range)
                } else if selection.cursor > 0 {
                    Some(self.previous_word_boundary(selection.cursor)..selection.cursor)
                } else {
                    None
                }
            })
            .map(|range| TextEdit {
                range,
                inserted: String::new(),
            })
            .collect();
        self.apply_transaction(edits)
    }

    pub fn delete_word_forward(&mut self) -> bool {
        let edits = self
            .selections
            .iter()
            .filter_map(|selection| {
                let range = selection.range();
                if !selection.is_caret() {
                    Some(range)
                } else if selection.cursor < self.len_chars() {
                    Some(selection.cursor..self.next_word_boundary(selection.cursor))
                } else {
                    None
                }
            })
            .map(|range| TextEdit {
                range,
                inserted: String::new(),
            })
            .collect();
        self.apply_transaction(edits)
    }

    pub fn delete_selection_ranges(&mut self) -> bool {
        if !self.has_selection() {
            return false;
        }

        let edits = self
            .selections
            .iter()
            .filter_map(|selection| {
                let range = selection.range();
                (range.start != range.end).then_some(TextEdit {
                    range,
                    inserted: String::new(),
                })
            })
            .collect();
        self.apply_transaction(edits)
    }

    pub fn delete_selection_or_lines(&mut self) -> bool {
        if self.has_selection() {
            self.delete_selection_ranges()
        } else {
            self.delete_lines()
        }
    }

    pub fn move_left(&mut self) {
        let cursors = self
            .selections
            .iter()
            .map(|selection| self.previous_grapheme_boundary(selection.cursor))
            .collect::<Vec<_>>();
        self.set_cursors(cursors);
    }

    pub fn extend_left(&mut self) {
        self.extend_cursors(|_, selection| selection.cursor.saturating_sub(1));
    }

    pub fn move_right(&mut self) {
        let cursors = self
            .selections
            .iter()
            .map(|selection| self.next_grapheme_boundary(selection.cursor))
            .collect::<Vec<_>>();
        self.set_cursors(cursors);
    }

    pub fn extend_right(&mut self) {
        self.extend_cursors(|buffer, selection| (selection.cursor + 1).min(buffer.len_chars()));
    }

    pub fn move_word_left(&mut self) {
        let cursors = self
            .selections
            .iter()
            .map(|selection| self.previous_word_boundary(selection.cursor))
            .collect::<Vec<_>>();
        self.set_cursors(cursors);
    }

    pub fn move_big_word_left(&mut self) {
        let cursors = self
            .selections
            .iter()
            .map(|selection| self.previous_big_word_boundary(selection.cursor))
            .collect::<Vec<_>>();
        self.set_cursors(cursors);
    }

    pub fn extend_word_left(&mut self) {
        self.extend_cursors(|buffer, selection| buffer.previous_word_boundary(selection.cursor));
    }

    pub fn move_word_right(&mut self) {
        let cursors = self
            .selections
            .iter()
            .map(|selection| self.next_word_boundary(selection.cursor))
            .collect::<Vec<_>>();
        self.set_cursors(cursors);
    }

    pub fn move_big_word_right(&mut self) {
        let cursors = self
            .selections
            .iter()
            .map(|selection| self.next_big_word_boundary(selection.cursor))
            .collect::<Vec<_>>();
        self.set_cursors(cursors);
    }

    pub fn move_word_end(&mut self) {
        let cursors = self
            .selections
            .iter()
            .map(|selection| self.next_word_end(selection.cursor))
            .collect::<Vec<_>>();
        self.set_cursors(cursors);
    }

    pub fn move_big_word_end(&mut self) {
        let cursors = self
            .selections
            .iter()
            .map(|selection| self.next_big_word_end(selection.cursor))
            .collect::<Vec<_>>();
        self.set_cursors(cursors);
    }

    pub fn move_previous_word_end(&mut self) {
        let cursors = self
            .selections
            .iter()
            .map(|selection| self.previous_word_end(selection.cursor))
            .collect::<Vec<_>>();
        self.set_cursors(cursors);
    }

    pub fn extend_word_right(&mut self) {
        self.extend_cursors(|buffer, selection| buffer.next_word_boundary(selection.cursor));
    }

    pub fn move_up(&mut self) {
        let cursors = self
            .selections
            .iter()
            .map(|selection| {
                let pos = self.char_position(selection.cursor);
                if pos.line == 0 {
                    selection.cursor
                } else {
                    self.line_column_to_char(pos.line - 1, pos.column)
                }
            })
            .collect::<Vec<_>>();
        self.set_cursors(cursors);
    }

    pub fn extend_up(&mut self) {
        self.extend_cursors(|buffer, selection| {
            let pos = buffer.char_position(selection.cursor);
            if pos.line == 0 {
                selection.cursor
            } else {
                buffer.line_column_to_char(pos.line - 1, pos.column)
            }
        });
    }

    pub fn move_down(&mut self) {
        let cursors = self
            .selections
            .iter()
            .map(|selection| {
                let pos = self.char_position(selection.cursor);
                if pos.line + 1 >= self.len_lines() {
                    selection.cursor
                } else {
                    self.line_column_to_char(pos.line + 1, pos.column)
                }
            })
            .collect::<Vec<_>>();
        self.set_cursors(cursors);
    }

    pub fn extend_down(&mut self) {
        self.extend_cursors(|buffer, selection| {
            let pos = buffer.char_position(selection.cursor);
            if pos.line + 1 >= buffer.len_lines() {
                selection.cursor
            } else {
                buffer.line_column_to_char(pos.line + 1, pos.column)
            }
        });
    }

    pub fn move_line_start(&mut self) {
        let cursors = self
            .selections
            .iter()
            .map(|selection| self.smart_line_start_char(selection.cursor))
            .collect::<Vec<_>>();
        self.set_cursors(cursors);
    }

    pub fn move_line_column_start(&mut self) {
        let cursors = self
            .selections
            .iter()
            .map(|selection| {
                let pos = self.char_position(selection.cursor);
                self.rope.line_to_char(pos.line)
            })
            .collect::<Vec<_>>();
        self.set_cursors(cursors);
    }

    pub fn move_line_first_non_whitespace(&mut self) {
        let cursors = self
            .selections
            .iter()
            .map(|selection| {
                let pos = self.char_position(selection.cursor);
                self.line_first_non_whitespace_char(pos.line)
            })
            .collect::<Vec<_>>();
        self.set_cursors(cursors);
    }

    pub fn extend_line_start(&mut self) {
        self.extend_cursors(|buffer, selection| buffer.smart_line_start_char(selection.cursor));
    }

    fn smart_line_start_char(&self, cursor: usize) -> usize {
        let pos = self.char_position(cursor);
        let line_start = self.rope.line_to_char(pos.line);
        let content_start = self.line_first_non_whitespace_char(pos.line);
        let content_end = self.line_content_end_char(pos.line);

        if content_start >= content_end || cursor == content_start {
            line_start
        } else {
            content_start
        }
    }

    pub fn move_line_end(&mut self) {
        let cursors = self
            .selections
            .iter()
            .map(|selection| {
                let pos = self.char_position(selection.cursor);
                self.line_content_end_char(pos.line)
            })
            .collect::<Vec<_>>();
        self.set_cursors(cursors);
    }

    pub fn extend_line_end(&mut self) {
        self.extend_cursors(|buffer, selection| {
            let pos = buffer.char_position(selection.cursor);
            buffer.line_content_end_char(pos.line)
        });
    }

    fn extend_cursors(&mut self, next_cursor: impl Fn(&Self, Selection) -> usize) {
        let len_chars = self.len_chars();
        self.selections = normalize_selections(
            self.selections
                .iter()
                .map(|selection| Selection {
                    anchor: selection.anchor.min(len_chars),
                    cursor: next_cursor(self, *selection).min(len_chars),
                })
                .collect(),
            len_chars,
        );
    }

    fn previous_word_boundary(&self, cursor: usize) -> usize {
        let mut idx = cursor.min(self.len_chars());
        if idx == 0 {
            return 0;
        }

        idx -= 1;
        while idx > 0 && self.char_group(self.rope.char(idx)) == CharGroup::Whitespace {
            if self.rope.char(idx) == '\n' && self.line_ending_here_is_empty(idx) {
                // Vim's `b` stops on an empty line like `w` does.
                return idx;
            }
            idx -= 1;
        }

        let group = self.char_group(self.rope.char(idx));
        while idx > 0 && self.char_group(self.rope.char(idx - 1)) == group {
            idx -= 1;
        }
        idx
    }

    fn next_word_boundary(&self, cursor: usize) -> usize {
        let len = self.len_chars();
        let mut idx = cursor.min(len);
        if idx >= len {
            return len;
        }

        while idx < len && self.char_group(self.rope.char(idx)) == CharGroup::Whitespace {
            idx += 1;
        }
        if idx >= len {
            return len;
        }

        let group = self.char_group(self.rope.char(idx));
        while idx < len && self.char_group(self.rope.char(idx)) == group {
            idx += 1;
        }
        idx
    }

    fn previous_big_word_boundary(&self, cursor: usize) -> usize {
        let mut idx = cursor.min(self.len_chars());
        if idx == 0 {
            return 0;
        }

        idx -= 1;
        while idx > 0 && self.rope.char(idx).is_whitespace() {
            if self.rope.char(idx) == '\n' && self.line_ending_here_is_empty(idx) {
                // Vim's `B` stops on an empty line.
                return idx;
            }
            idx -= 1;
        }

        while idx > 0 && !self.rope.char(idx - 1).is_whitespace() {
            idx -= 1;
        }
        idx
    }

    fn next_big_word_boundary(&self, cursor: usize) -> usize {
        let len = self.len_chars();
        let mut idx = cursor.min(len);
        if idx >= len {
            return len;
        }

        if !self.rope.char(idx).is_whitespace() {
            while idx < len && !self.rope.char(idx).is_whitespace() {
                idx += 1;
            }
        }
        while idx < len && self.rope.char(idx).is_whitespace() {
            if self.rope.char(idx) == '\n' && self.line_after_is_empty(idx) {
                // Vim's `W` stops on an empty line like `w` does: the cursor
                // parks on the empty line itself.
                return idx + 1;
            }
            idx += 1;
        }
        idx
    }

    /// `true` when the line terminated by the newline at `newline_idx` holds
    /// no content, so the newline is the line's first character.
    fn line_ending_here_is_empty(&self, newline_idx: usize) -> bool {
        self.line_column_to_char(self.char_position(newline_idx).line, 0) == newline_idx
    }

    /// `true` when the line following the newline at `newline_idx` holds no
    /// content.
    fn line_after_is_empty(&self, newline_idx: usize) -> bool {
        let next_line = self.char_position(newline_idx).line + 1;
        if next_line >= self.len_lines() {
            return false;
        }
        self.line_content_end_char(next_line) == self.line_column_to_char(next_line, 0)
    }

    fn next_word_end(&self, cursor: usize) -> usize {
        let len = self.len_chars();
        let mut idx = cursor.min(len);
        if idx >= len {
            return len;
        }

        if self.char_group(self.rope.char(idx)) != CharGroup::Whitespace {
            let end = self.next_word_boundary(idx).saturating_sub(1);
            if end > idx {
                return end;
            }
            idx = (idx + 1).min(len);
        }

        while idx < len && self.char_group(self.rope.char(idx)) == CharGroup::Whitespace {
            idx += 1;
        }
        if idx >= len {
            return len;
        }
        self.next_word_boundary(idx).saturating_sub(1)
    }

    fn next_big_word_end(&self, cursor: usize) -> usize {
        let len = self.len_chars();
        let mut idx = cursor.min(len);
        if idx >= len {
            return len;
        }

        if !self.rope.char(idx).is_whitespace() {
            let end = self.big_word_end_at(idx);
            if end > idx {
                return end;
            }
            idx = (idx + 1).min(len);
        }

        while idx < len && self.rope.char(idx).is_whitespace() {
            idx += 1;
        }
        if idx >= len {
            return len;
        }
        self.big_word_end_at(idx)
    }

    fn big_word_end_at(&self, cursor: usize) -> usize {
        let len = self.len_chars();
        let mut idx = cursor.min(len);
        while idx < len && !self.rope.char(idx).is_whitespace() {
            idx += 1;
        }
        idx.saturating_sub(1)
    }

    fn previous_word_end(&self, cursor: usize) -> usize {
        let len = self.len_chars();
        let idx = cursor.min(len);
        if idx == 0 {
            return 0;
        }

        let mut probe = idx - 1;
        if idx < len {
            let current_group = self.char_group(self.rope.char(idx));
            if current_group != CharGroup::Whitespace {
                while probe > 0 && self.char_group(self.rope.char(probe)) == current_group {
                    probe -= 1;
                }
                if self.char_group(self.rope.char(probe)) == current_group {
                    return 0;
                }
            }
        }

        while probe > 0 && self.char_group(self.rope.char(probe)) == CharGroup::Whitespace {
            probe -= 1;
        }
        if self.char_group(self.rope.char(probe)) == CharGroup::Whitespace {
            0
        } else {
            probe
        }
    }

    fn char_group(&self, ch: char) -> CharGroup {
        if ch.is_whitespace() {
            CharGroup::Whitespace
        } else if self.is_word_char(ch) {
            CharGroup::Word
        } else {
            CharGroup::Symbol
        }
    }

    pub(super) fn is_word_char(&self, ch: char) -> bool {
        is_word_char_with_separators(ch, &self.word_separators)
    }

    pub(super) fn is_whole_word_match(
        &self,
        text: &str,
        start_byte: usize,
        end_byte: usize,
    ) -> bool {
        let before = text[..start_byte].chars().next_back();
        let after = text[end_byte..].chars().next();
        !before.is_some_and(|ch| self.is_word_char(ch))
            && !after.is_some_and(|ch| self.is_word_char(ch))
    }

    /// Nearest grapheme cluster boundary at or before `idx - 1`; for a cursor
    /// inside a cluster this snaps to the start of that cluster.
    fn previous_grapheme_boundary(&self, idx: usize) -> usize {
        let mut idx = idx.min(self.len_chars());
        if idx == 0 {
            return 0;
        }
        idx -= 1;
        while idx > 0 && !self.is_grapheme_boundary_char(idx) {
            idx -= 1;
        }
        idx
    }

    /// Nearest grapheme cluster boundary at or after `idx + 1`; for a cursor
    /// inside a cluster this snaps to the end of that cluster.
    fn next_grapheme_boundary(&self, idx: usize) -> usize {
        let len = self.len_chars();
        let mut idx = (idx.min(len) + 1).min(len);
        while idx < len && !self.is_grapheme_boundary_char(idx) {
            idx += 1;
        }
        idx
    }

    /// Returns `true` when a grapheme cluster boundary exists immediately
    /// before `char_index`. Proximate chars are copied into a short window so
    /// the `&str`-based boundary check can be reused.
    fn is_grapheme_boundary_char(&self, char_index: usize) -> bool {
        let len = self.len_chars();
        if char_index == 0 || char_index >= len {
            return true;
        }
        let context_start = char_index.saturating_sub(GRAPHEME_CONTEXT_CHARS);
        let mut context = String::with_capacity((char_index - context_start + 1) * 4);
        for index in context_start..=char_index {
            context.push(self.rope.char(index));
        }
        let byte_index = context.len() - self.rope.char(char_index).len_utf8();
        is_grapheme_boundary(&context, byte_index)
    }
}

/// Number of chars kept behind a cursor when testing grapheme boundaries.
/// Long enough for ZWJ emoji families and regional-indicator flags; longer
/// runs are approximated.
const GRAPHEME_CONTEXT_CHARS: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq)]
enum GraphemeKind {
    Other,
    Extend,
    Zwj,
    RegionalIndicator,
    Cr,
    Lf,
    Control,
}

fn grapheme_kind(ch: char) -> GraphemeKind {
    match ch {
        '\u{000D}' => GraphemeKind::Cr,
        '\u{000A}' => GraphemeKind::Lf,
        '\u{0000}'..='\u{0009}'
        | '\u{000B}'..='\u{000C}'
        | '\u{000E}'..='\u{001F}'
        | '\u{007F}'..='\u{009F}' => GraphemeKind::Control,
        '\u{200D}' => GraphemeKind::Zwj,
        '\u{1F1E6}'..='\u{1F1FF}' => GraphemeKind::RegionalIndicator,
        _ if is_grapheme_extend(ch) => GraphemeKind::Extend,
        _ => GraphemeKind::Other,
    }
}

/// Returns `true` when a grapheme boundary may exist between `prev` and
/// `cur`. Approximates UAX #29 extended-grapheme-cluster rules GB3-GB5, GB9,
/// GB11 and GB12/GB13; Hangul jamo, `Prepend` and `SpacingMark` sequences are
/// not modeled. `regional_indicator_run` counts the contiguous run of
/// regional indicators ending at `prev` (inclusive).
fn is_grapheme_break(prev: char, cur: char, regional_indicator_run: usize) -> bool {
    use GraphemeKind::{Control, Cr, Extend, Lf, RegionalIndicator, Zwj};
    match (grapheme_kind(prev), grapheme_kind(cur)) {
        (Cr, Lf) => false,
        (Cr | Lf | Control, _) | (_, Cr | Lf | Control) => true,
        (_, Extend | Zwj) | (Zwj, _) => false,
        (RegionalIndicator, RegionalIndicator) => regional_indicator_run.is_multiple_of(2),
        _ => true,
    }
}

/// Returns `true` when `idx` (a byte offset) falls on an approximate extended
/// grapheme cluster boundary in `text`. Dependency-free subset of the
/// `unicode-segmentation` rules: never breaks inside combining-mark sequences
/// (`e` + U+0301), ZWJ emoji sequences, variation-selector or skin-tone
/// sequences, CRLF, or regional-indicator flag pairs.
pub fn is_grapheme_boundary(text: &str, idx: usize) -> bool {
    if idx == 0 || idx >= text.len() {
        return true;
    }
    if !text.is_char_boundary(idx) {
        return false;
    }
    let prev = text[..idx]
        .chars()
        .next_back()
        .expect("char boundary implies non-empty prefix");
    let cur = text[idx..]
        .chars()
        .next()
        .expect("char boundary implies non-empty suffix");
    let mut regional_indicator_run =
        usize::from(grapheme_kind(prev) == GraphemeKind::RegionalIndicator);
    let mut cursor = idx - prev.len_utf8();
    while let Some(ch) = text[..cursor].chars().next_back() {
        if grapheme_kind(ch) != GraphemeKind::RegionalIndicator {
            break;
        }
        regional_indicator_run += 1;
        cursor -= ch.len_utf8();
    }
    is_grapheme_break(prev, cur, regional_indicator_run)
}

/// Inclusive `char` ranges treated as grapheme `Extend` (combining marks,
/// variation selectors, emoji modifiers). This is a practical subset of the
/// full Unicode `Grapheme_Extend` tables covering the most common scripts.
const GRAPHEME_EXTEND_RANGES: &[(char, char)] = &[
    ('\u{0300}', '\u{036F}'), // combining diacritical marks
    ('\u{0483}', '\u{0489}'), // combining cyrillic
    ('\u{0591}', '\u{05BD}'), // hebrew points
    ('\u{05BF}', '\u{05BF}'),
    ('\u{05C1}', '\u{05C2}'),
    ('\u{05C4}', '\u{05C5}'),
    ('\u{05C7}', '\u{05C7}'),
    ('\u{0610}', '\u{061A}'), // arabic
    ('\u{064B}', '\u{065F}'),
    ('\u{0670}', '\u{0670}'),
    ('\u{06D6}', '\u{06DC}'),
    ('\u{06DF}', '\u{06E4}'),
    ('\u{06E7}', '\u{06E8}'),
    ('\u{06EA}', '\u{06ED}'),
    ('\u{0711}', '\u{0711}'),
    ('\u{0730}', '\u{074A}'),
    ('\u{07A6}', '\u{07B0}'), // thaana
    ('\u{07EB}', '\u{07F3}'),
    ('\u{0816}', '\u{082D}'), // samaritan
    ('\u{0859}', '\u{085B}'), // mandaic
    ('\u{08D3}', '\u{08E1}'),
    ('\u{08E3}', '\u{0902}'), // arabic extended / devanagari
    ('\u{093A}', '\u{093A}'),
    ('\u{093C}', '\u{093C}'),
    ('\u{0941}', '\u{0948}'),
    ('\u{094D}', '\u{094D}'),
    ('\u{0951}', '\u{0957}'),
    ('\u{0962}', '\u{0963}'),
    ('\u{0981}', '\u{0981}'), // bengali
    ('\u{09BC}', '\u{09BC}'),
    ('\u{09C1}', '\u{09C4}'),
    ('\u{09CD}', '\u{09CD}'),
    ('\u{09E2}', '\u{09E3}'),
    ('\u{0A01}', '\u{0A02}'), // gurmukhi
    ('\u{0A3C}', '\u{0A3C}'),
    ('\u{0A41}', '\u{0A42}'),
    ('\u{0A47}', '\u{0A48}'),
    ('\u{0A4B}', '\u{0A4D}'),
    ('\u{0A70}', '\u{0A71}'),
    ('\u{0A81}', '\u{0A82}'), // gujarati
    ('\u{0ABC}', '\u{0ABC}'),
    ('\u{0AC1}', '\u{0ACD}'),
    ('\u{0B01}', '\u{0B01}'), // oriya
    ('\u{0B3C}', '\u{0B3C}'),
    ('\u{0B3F}', '\u{0B3F}'),
    ('\u{0B41}', '\u{0B44}'),
    ('\u{0B4D}', '\u{0B4D}'),
    ('\u{0BC0}', '\u{0BC0}'), // tamil
    ('\u{0BCD}', '\u{0BCD}'),
    ('\u{0C00}', '\u{0C00}'), // telugu
    ('\u{0C3E}', '\u{0C40}'),
    ('\u{0C46}', '\u{0C48}'),
    ('\u{0C4A}', '\u{0C4D}'),
    ('\u{0CBC}', '\u{0CBC}'), // kannada
    ('\u{0CCC}', '\u{0CCD}'),
    ('\u{0D01}', '\u{0D01}'), // malayalam
    ('\u{0D41}', '\u{0D44}'),
    ('\u{0D4D}', '\u{0D4D}'),
    ('\u{0DCA}', '\u{0DCA}'), // sinhala
    ('\u{0DD2}', '\u{0DD4}'),
    ('\u{0DD6}', '\u{0DD6}'),
    ('\u{0E31}', '\u{0E31}'), // thai
    ('\u{0E34}', '\u{0E3A}'),
    ('\u{0E47}', '\u{0E4E}'),
    ('\u{0EB1}', '\u{0EB1}'), // lao
    ('\u{0EB4}', '\u{0EBC}'),
    ('\u{0EC8}', '\u{0ECD}'),
    ('\u{0F35}', '\u{0F35}'), // tibetan
    ('\u{0F37}', '\u{0F37}'),
    ('\u{0F39}', '\u{0F39}'),
    ('\u{0F71}', '\u{0F84}'),
    ('\u{0F86}', '\u{0F87}'),
    ('\u{102D}', '\u{1030}'), // myanmar
    ('\u{1032}', '\u{1037}'),
    ('\u{1039}', '\u{103A}'),
    ('\u{1058}', '\u{1059}'),
    ('\u{1071}', '\u{1074}'),
    ('\u{1082}', '\u{1082}'),
    ('\u{1085}', '\u{1086}'),
    ('\u{108D}', '\u{108D}'),
    ('\u{135D}', '\u{135F}'), // ethiopic
    ('\u{1712}', '\u{1714}'), // tagalog
    ('\u{1732}', '\u{1734}'), // hanunoo
    ('\u{1752}', '\u{1753}'), // buhid
    ('\u{1772}', '\u{1773}'), // tagbanwa
    ('\u{17B4}', '\u{17D3}'), // khmer
    ('\u{17DD}', '\u{17DD}'),
    ('\u{180B}', '\u{180F}'), // mongolian free variation selectors
    ('\u{1920}', '\u{1922}'), // limbu
    ('\u{1927}', '\u{1928}'),
    ('\u{1932}', '\u{1932}'),
    ('\u{1939}', '\u{193B}'),
    ('\u{1A17}', '\u{1A18}'), // buginese
    ('\u{1AB0}', '\u{1ACE}'), // combining diacritical marks extended
    ('\u{1B00}', '\u{1B03}'), // balinese
    ('\u{1B34}', '\u{1B34}'),
    ('\u{1B6B}', '\u{1B73}'),
    ('\u{1CD0}', '\u{1CD2}'), // vedic
    ('\u{1DC0}', '\u{1DFF}'), // combining diacritical marks supplement
    ('\u{200C}', '\u{200C}'), // zero width non-joiner
    ('\u{20D0}', '\u{20F0}'), // combining marks for symbols
    ('\u{2CEF}', '\u{2CF1}'), // coptic
    ('\u{2D7F}', '\u{2D7F}'), // tifinagh joiner
    ('\u{2DE0}', '\u{2DFF}'), // combining cyrillic extended
    ('\u{302A}', '\u{302D}'), // cjk ideographic marks
    ('\u{3099}', '\u{309A}'), // kana voicing marks
    ('\u{A66F}', '\u{A672}'), // cyrillic combining
    ('\u{A69E}', '\u{A69F}'),
    ('\u{A802}', '\u{A802}'),   // phags-pa
    ('\u{A926}', '\u{A92D}'),   // javanese
    ('\u{A947}', '\u{A951}'),   // rejang
    ('\u{FB1E}', '\u{FB1E}'),   // hebrew point judeo-spanish
    ('\u{FE00}', '\u{FE0F}'),   // variation selectors
    ('\u{FE20}', '\u{FE2F}'),   // combining half marks
    ('\u{FF9E}', '\u{FF9F}'),   // halfwidth kana voicing marks
    ('\u{101FD}', '\u{101FD}'), // phaistos disc
    ('\u{10376}', '\u{1037A}'), // old persian
    ('\u{11038}', '\u{11046}'), // brahmi
    ('\u{11127}', '\u{1112B}'), // chakma
    ('\u{16AF0}', '\u{16AF4}'), // bassa vah
    ('\u{16B30}', '\u{16B36}'), // pahawh hmong
    ('\u{1D165}', '\u{1D169}'), // musical symbols
    ('\u{1D16D}', '\u{1D172}'),
    ('\u{1D17B}', '\u{1D182}'),
    ('\u{1D242}', '\u{1D244}'),
    ('\u{1DA00}', '\u{1DA36}'), // signwriting
    ('\u{1DA3B}', '\u{1DA6C}'),
    ('\u{1E944}', '\u{1E94A}'), // adlam
    ('\u{1F3FB}', '\u{1F3FF}'), // emoji skin tone modifiers
    ('\u{E0100}', '\u{E01EF}'), // variation selectors supplement
];

fn is_grapheme_extend(ch: char) -> bool {
    GRAPHEME_EXTEND_RANGES
        .iter()
        .any(|&(start, end)| (start..=end).contains(&ch))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";

    fn buffer_with(text: &str) -> TextBuffer {
        TextBuffer::from_text(1, None, text.to_owned())
    }

    #[test]
    fn is_grapheme_boundary_keeps_ascii_positions() {
        let text = "ab\n";
        for idx in 0..=text.len() {
            assert!(
                is_grapheme_boundary(text, idx),
                "expected boundary at {idx}"
            );
        }
    }

    #[test]
    fn is_grapheme_boundary_never_splits_combining_sequence() {
        let text = "e\u{0301}";
        assert!(is_grapheme_boundary(text, 0));
        assert!(!is_grapheme_boundary(text, 1));
        assert!(is_grapheme_boundary(text, text.len()));
    }

    #[test]
    fn is_grapheme_boundary_never_splits_zwj_family() {
        let text = format!("a{FAMILY}b");
        let family_start = 1;
        let family_end = family_start + FAMILY.len();
        for idx in family_start + 1..family_end {
            if text.is_char_boundary(idx) {
                assert!(!is_grapheme_boundary(&text, idx), "no boundary at {idx}");
            }
        }
        assert!(is_grapheme_boundary(&text, family_start));
        assert!(is_grapheme_boundary(&text, family_end));
    }

    #[test]
    fn is_grapheme_boundary_pairs_regional_indicators() {
        let text = "\u{1F1FA}\u{1F1F8}\u{1F1EB}\u{1F1F7}";
        assert!(is_grapheme_boundary(text, 0));
        assert!(!is_grapheme_boundary(text, 4));
        assert!(is_grapheme_boundary(text, 8));
        assert!(!is_grapheme_boundary(text, 12));
        assert!(is_grapheme_boundary(text, 16));
    }

    #[test]
    fn is_grapheme_boundary_keeps_crlf_together() {
        let text = "a\r\nb";
        assert!(!is_grapheme_boundary(text, 2));
        assert!(is_grapheme_boundary(text, 1));
        assert!(is_grapheme_boundary(text, 3));
    }

    #[test]
    fn is_grapheme_boundary_rejects_mid_char_offsets() {
        let text = "a\u{1F468}b";
        assert!(!is_grapheme_boundary(text, 2));
    }

    #[test]
    fn move_left_steps_over_grapheme_clusters() {
        let mut buffer = buffer_with(&format!("a{FAMILY}b"));
        buffer.set_single_cursor(buffer.len_chars());
        buffer.move_left();
        assert_eq!(buffer.selections(), &[Selection::caret(6)]);
        buffer.move_left();
        assert_eq!(buffer.selections(), &[Selection::caret(1)]);
        buffer.move_left();
        assert_eq!(buffer.selections(), &[Selection::caret(0)]);
    }

    #[test]
    fn move_right_steps_over_grapheme_clusters() {
        let mut buffer = buffer_with(&format!("a{FAMILY}b"));
        buffer.move_right();
        assert_eq!(buffer.selections(), &[Selection::caret(1)]);
        buffer.move_right();
        assert_eq!(buffer.selections(), &[Selection::caret(6)]);
        buffer.move_right();
        assert_eq!(buffer.selections(), &[Selection::caret(7)]);
    }

    #[test]
    fn movement_snaps_mid_cluster_cursors_in_direction_of_travel() {
        let mut buffer = buffer_with(&format!("a{FAMILY}b"));
        buffer.set_single_cursor(3);
        buffer.move_left();
        assert_eq!(buffer.selections(), &[Selection::caret(1)]);
        buffer.set_single_cursor(3);
        buffer.move_right();
        assert_eq!(buffer.selections(), &[Selection::caret(6)]);
    }

    #[test]
    fn move_steps_over_combining_marks() {
        let mut buffer = buffer_with("cafe\u{0301}");
        buffer.set_single_cursor(buffer.len_chars());
        buffer.move_left();
        assert_eq!(buffer.selections(), &[Selection::caret(3)]);
        buffer.move_right();
        assert_eq!(buffer.selections(), &[Selection::caret(5)]);
    }

    #[test]
    fn single_byte_movement_and_deletion_unchanged() {
        let mut buffer = buffer_with("abc");
        buffer.set_single_cursor(1);
        buffer.move_left();
        assert_eq!(buffer.selections(), &[Selection::caret(0)]);
        buffer.move_right();
        buffer.move_right();
        assert_eq!(buffer.selections(), &[Selection::caret(2)]);
        assert!(buffer.delete_forward());
        assert_eq!(buffer.text(), "ab");
        assert_eq!(buffer.selections(), &[Selection::caret(2)]);
        assert!(buffer.delete_backward());
        assert_eq!(buffer.text(), "a");
        assert_eq!(buffer.selections(), &[Selection::caret(1)]);
    }

    #[test]
    fn delete_backward_removes_whole_clusters() {
        let mut buffer = buffer_with(&format!("a{FAMILY}b"));
        buffer.set_single_cursor(buffer.len_chars());
        assert!(buffer.delete_backward());
        assert_eq!(buffer.text(), format!("a{FAMILY}"));
        assert!(buffer.delete_backward());
        assert_eq!(buffer.text(), "a");

        let mut buffer = buffer_with("cafe\u{0301}");
        buffer.set_single_cursor(buffer.len_chars());
        assert!(buffer.delete_backward());
        assert_eq!(buffer.text(), "caf");
    }

    #[test]
    fn delete_forward_removes_whole_clusters() {
        let mut buffer = buffer_with(&format!("a{FAMILY}b"));
        assert!(buffer.delete_forward());
        assert_eq!(buffer.text(), format!("{FAMILY}b"));
        assert!(buffer.delete_forward());
        assert_eq!(buffer.text(), "b");

        let mut buffer = buffer_with("e\u{0301}x");
        assert!(buffer.delete_forward());
        assert_eq!(buffer.text(), "x");
    }

    #[test]
    fn multicursor_delete_backward_never_splits_clusters() {
        let mut buffer = buffer_with(&format!("{FAMILY} {FAMILY}"));
        buffer.set_cursors([5, 11]);
        assert!(buffer.delete_backward());
        assert_eq!(buffer.text(), " ");
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
    fn delete_backward_removes_crlf_as_one_cluster() {
        let mut buffer = buffer_with("a\r\nb");
        // A boundary exists between the LF and the following char (GB4), so a
        // cursor after 'b' deletes only that char...
        buffer.set_single_cursor(buffer.len_chars());
        assert!(buffer.delete_backward());
        assert_eq!(buffer.text(), "a\r\n");
        assert_eq!(buffer.selections(), &[Selection::caret(3)]);
        // ...while a cursor directly after the LF deletes CRLF as one cluster.
        let mut buffer = buffer_with("a\r\nb");
        buffer.set_single_cursor(3);
        assert!(buffer.delete_backward());
        assert_eq!(buffer.text(), "ab");
        assert_eq!(buffer.selections(), &[Selection::caret(1)]);
    }
}
