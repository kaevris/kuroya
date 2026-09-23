use super::edits::join_line_contents;
use super::{LineDuplicateEdit, LineMoveEdit, TextBuffer, TextEdit};
use std::ops::Range;

impl TextBuffer {
    pub fn indent_lines(&mut self, indent_unit: &str) -> bool {
        if indent_unit.is_empty() {
            return false;
        }

        let edits = self
            .selected_line_indices()
            .into_iter()
            .filter(|line| !self.line_is_blank(*line))
            .map(|line| {
                let line_start = self.rope.line_to_char(line);
                TextEdit {
                    range: line_start..line_start,
                    inserted: indent_unit.to_owned(),
                }
            })
            .collect::<Vec<_>>();
        self.apply_linewise_transaction(edits)
    }

    pub fn outdent_lines(&mut self, indent_unit: &str) -> bool {
        let indent_width = indent_unit.chars().count().max(1);
        let edits = self
            .selected_line_indices()
            .into_iter()
            .filter_map(|line| {
                let line_start = self.rope.line_to_char(line);
                let remove_len = self.line_outdent_len(line, indent_width);
                (remove_len > 0).then_some(TextEdit {
                    range: line_start..line_start + remove_len,
                    inserted: String::new(),
                })
            })
            .collect::<Vec<_>>();
        self.apply_linewise_transaction(edits)
    }

    pub fn toggle_line_comments(&mut self, comment_prefix: &str) -> bool {
        self.toggle_line_comments_with_options(comment_prefix, true, true)
    }

    pub fn toggle_line_comments_with_options(
        &mut self,
        comment_prefix: &str,
        insert_space: bool,
        ignore_empty_lines: bool,
    ) -> bool {
        if comment_prefix.is_empty() {
            return false;
        }

        let lines = self.selected_line_indices();
        let nonblank_lines = lines
            .iter()
            .copied()
            .filter(|line| !self.line_is_blank(*line))
            .collect::<Vec<_>>();
        let target_lines = if !ignore_empty_lines || nonblank_lines.is_empty() {
            lines
        } else {
            nonblank_lines
        };
        let uncomment = target_lines
            .iter()
            .all(|line| self.line_comment_range(*line, comment_prefix).is_some());

        let edits = if uncomment {
            target_lines
                .into_iter()
                .filter_map(|line| {
                    self.line_comment_range(line, comment_prefix)
                        .map(|range| TextEdit {
                            range,
                            inserted: String::new(),
                        })
                })
                .collect::<Vec<_>>()
        } else {
            let inserted = if insert_space {
                format!("{comment_prefix} ")
            } else {
                comment_prefix.to_owned()
            };
            target_lines
                .into_iter()
                .filter(|line| self.line_comment_range(*line, comment_prefix).is_none())
                .map(|line| {
                    let char_idx = self.line_first_non_whitespace_char(line);
                    TextEdit {
                        range: char_idx..char_idx,
                        inserted: inserted.clone(),
                    }
                })
                .collect::<Vec<_>>()
        };

        self.apply_linewise_transaction(edits)
    }

    pub fn delete_lines(&mut self) -> bool {
        let edits = self
            .selected_line_blocks()
            .into_iter()
            .filter_map(|block| {
                let range = self.line_delete_char_range(block);
                (range.start != range.end).then_some(TextEdit {
                    range,
                    inserted: String::new(),
                })
            })
            .collect::<Vec<_>>();

        self.apply_linewise_transaction(edits)
    }

    pub fn join_lines(&mut self) -> bool {
        let edits = self
            .selected_line_blocks()
            .into_iter()
            .flat_map(|block| self.join_line_edits(block))
            .collect::<Vec<_>>();

        self.apply_linewise_transaction(edits)
    }

    pub fn duplicate_lines(&mut self) -> bool {
        let edits = self
            .selected_line_blocks()
            .into_iter()
            .filter_map(|block| {
                let source_range = self.line_block_char_range(block);
                let mut inserted = self.rope.slice(source_range.clone()).to_string();
                let duplicate_start_offset =
                    if source_range.end == self.len_chars() && !inserted.ends_with('\n') {
                        inserted.insert(0, '\n');
                        1
                    } else {
                        0
                    };
                (!inserted.is_empty()).then_some(LineDuplicateEdit {
                    edit: TextEdit {
                        range: source_range.end..source_range.end,
                        inserted,
                    },
                    source_range,
                    duplicate_start_offset,
                })
            })
            .collect::<Vec<_>>();

        self.apply_line_duplicate_transaction(edits)
    }

    pub fn move_lines_up(&mut self) -> bool {
        let blocks = self.selected_line_blocks();
        if blocks.is_empty() || blocks.iter().any(|block| block.start == 0) {
            return false;
        }

        let edits = blocks
            .into_iter()
            .map(|block| {
                let previous_line = block.start - 1;
                let region_range = self.line_block_char_range(previous_line..block.end);
                let previous = self.line_content_text(previous_line);
                let moved = self.line_content_texts(block.clone());
                let trailing_newline = self.range_ends_with_newline(region_range.clone());
                let mut replacement_lines = moved;
                replacement_lines.push(previous);
                LineMoveEdit {
                    edit: TextEdit {
                        range: region_range,
                        inserted: join_line_contents(&replacement_lines, trailing_newline),
                    },
                    block,
                    moved_block_local_start: 0,
                    replacement_lines,
                    trailing_newline,
                }
            })
            .collect::<Vec<_>>();

        self.apply_line_move_transaction(edits)
    }

    pub fn move_lines_down(&mut self) -> bool {
        let blocks = self.selected_line_blocks();
        if blocks.is_empty() || blocks.iter().any(|block| block.end >= self.len_lines()) {
            return false;
        }

        let edits = blocks
            .into_iter()
            .map(|block| {
                let next_line = block.end;
                let region_range = self.line_block_char_range(block.start..next_line + 1);
                let next = self.line_content_text(next_line);
                let moved = self.line_content_texts(block.clone());
                let trailing_newline = self.range_ends_with_newline(region_range.clone());
                let mut replacement_lines = Vec::with_capacity(moved.len() + 1);
                replacement_lines.push(next);
                replacement_lines.extend(moved);
                LineMoveEdit {
                    edit: TextEdit {
                        range: region_range,
                        inserted: join_line_contents(&replacement_lines, trailing_newline),
                    },
                    block,
                    moved_block_local_start: 1,
                    replacement_lines,
                    trailing_newline,
                }
            })
            .collect::<Vec<_>>();

        self.apply_line_move_transaction(edits)
    }

    pub(super) fn selected_line_blocks(&self) -> Vec<Range<usize>> {
        let lines = self.selected_line_indices();
        let mut blocks: Vec<Range<usize>> = Vec::new();
        for line in lines {
            let Some(last) = blocks.last_mut() else {
                blocks.push(line..line + 1);
                continue;
            };

            if line == last.end {
                last.end += 1;
            } else {
                blocks.push(line..line + 1);
            }
        }
        blocks
    }

    pub(super) fn selected_line_indices(&self) -> Vec<usize> {
        let mut lines = Vec::new();
        for selection in &self.selections {
            let range = selection.range();
            let start_line = self.char_position(range.start).line;
            let mut end_line = self.char_position(range.end).line;
            if range.start != range.end
                && end_line > start_line
                && self.rope.line_to_char(end_line) == range.end
            {
                end_line -= 1;
            }

            lines.extend(start_line..=end_line);
        }

        lines.sort_unstable();
        lines.dedup();
        lines
    }

    pub(super) fn line_block_char_range(&self, block: Range<usize>) -> Range<usize> {
        let start_line = block.start.min(self.len_lines().saturating_sub(1));
        let start = self.rope.line_to_char(start_line);
        let end = if block.end >= self.len_lines() {
            self.len_chars()
        } else {
            self.rope.line_to_char(block.end)
        };
        start..end.max(start)
    }

    fn line_delete_char_range(&self, block: Range<usize>) -> Range<usize> {
        if self.len_chars() == 0 || self.len_lines() == 0 {
            return 0..0;
        }

        let line_count = self.len_lines();
        let start_line = block.start.min(line_count.saturating_sub(1));
        let end_line = block.end.min(line_count);
        let mut start = self.rope.line_to_char(start_line);
        let end = if end_line >= line_count {
            self.len_chars()
        } else {
            self.rope.line_to_char(end_line)
        };

        if end_line >= line_count
            && start_line > 0
            && let Some(line_break_start) = self.line_break_start_before(start)
        {
            start = line_break_start;
        }

        start..end.max(start)
    }

    fn join_line_edits(&self, block: Range<usize>) -> Vec<TextEdit> {
        if self.len_lines() < 2 {
            return Vec::new();
        }

        let last_join_line = if block.end.saturating_sub(block.start) <= 1 {
            block.start.saturating_add(1)
        } else {
            block.end.saturating_sub(1)
        }
        .min(self.len_lines().saturating_sub(1));

        (block.start..last_join_line)
            .filter_map(|line| {
                let next_line = line + 1;
                let start = self.line_trimmed_content_end_char(line);
                let end = self.line_first_non_whitespace_char(next_line);
                (start < end).then_some(TextEdit {
                    range: start..end,
                    inserted: self.join_line_separator(line, next_line).to_owned(),
                })
            })
            .collect()
    }

    fn line_break_start_before(&self, char_idx: usize) -> Option<usize> {
        if char_idx == 0 || self.rope.char(char_idx - 1) != '\n' {
            return None;
        }

        let newline = char_idx - 1;
        Some(if newline > 0 && self.rope.char(newline - 1) == '\r' {
            newline - 1
        } else {
            newline
        })
    }

    fn line_trimmed_content_end_char(&self, line: usize) -> usize {
        let start = self
            .rope
            .line_to_char(line.min(self.len_lines().saturating_sub(1)));
        let mut end = self.line_content_end_char(line);
        while end > start && matches!(self.rope.char(end - 1), ' ' | '\t') {
            end -= 1;
        }
        end
    }

    fn line_last_non_whitespace_char(&self, line: usize) -> Option<char> {
        let start = self
            .rope
            .line_to_char(line.min(self.len_lines().saturating_sub(1)));
        let end = self.line_trimmed_content_end_char(line);
        (end > start).then(|| self.rope.char(end - 1))
    }

    fn line_first_non_whitespace_char_value(&self, line: usize) -> Option<char> {
        let start = self.line_first_non_whitespace_char(line);
        (start < self.line_content_end_char(line)).then(|| self.rope.char(start))
    }

    fn join_line_separator(&self, left_line: usize, right_line: usize) -> &'static str {
        let Some(left) = self.line_last_non_whitespace_char(left_line) else {
            return "";
        };
        let Some(right) = self.line_first_non_whitespace_char_value(right_line) else {
            return "";
        };

        if matches!(left, '(' | '[' | '.' | ':' | '/' | '\\')
            || matches!(right, ')' | ']' | '}' | ',' | ';' | '.' | ':')
        {
            ""
        } else {
            " "
        }
    }

    fn line_content_text(&self, line: usize) -> String {
        self.line(line)
            .unwrap_or_default()
            .trim_end_matches(['\r', '\n'])
            .to_owned()
    }

    fn line_content_texts(&self, block: Range<usize>) -> Vec<String> {
        block
            .map(|line| self.line_content_text(line))
            .collect::<Vec<_>>()
    }

    pub(super) fn line_first_non_whitespace_char(&self, line: usize) -> usize {
        let start = self
            .rope
            .line_to_char(line.min(self.len_lines().saturating_sub(1)));
        let end = self.line_content_end_char(line);
        let mut idx = start;
        while idx < end && matches!(self.rope.char(idx), ' ' | '\t') {
            idx += 1;
        }
        idx
    }

    pub fn line_is_blank(&self, line: usize) -> bool {
        self.line_first_non_whitespace_char(line) >= self.line_content_end_char(line)
    }

    fn line_comment_range(&self, line: usize, comment_prefix: &str) -> Option<Range<usize>> {
        let start = self.line_first_non_whitespace_char(line);
        let end = self.line_content_end_char(line);
        let prefix_len = comment_prefix.chars().count();
        let prefix_end = start.checked_add(prefix_len)?;
        if prefix_len == 0 || prefix_end > end {
            return None;
        }

        if self.rope.slice(start..prefix_end) != comment_prefix {
            return None;
        }

        let remove_end = if prefix_end < end && self.rope.char(prefix_end) == ' ' {
            prefix_end + 1
        } else {
            prefix_end
        };
        Some(start..remove_end)
    }

    fn range_ends_with_newline(&self, range: Range<usize>) -> bool {
        range.end > range.start && self.rope.char(range.end - 1) == '\n'
    }

    fn line_outdent_len(&self, line: usize, indent_width: usize) -> usize {
        let start = self.rope.line_to_char(line);
        let end = self.line_content_end_char(line);
        if start >= end {
            return 0;
        }

        if self.rope.char(start) == '\t' {
            return 1;
        }

        let mut remove_len = 0;
        while remove_len < indent_width
            && start + remove_len < end
            && self.rope.char(start + remove_len) == ' '
        {
            remove_len += 1;
        }
        remove_len
    }
}

/// Number of chars kept behind an index when testing grapheme boundaries.
/// Mirrors the context window used by the movement module's boundary check.
const GRAPHEME_SNAP_CONTEXT_CHARS: usize = 32;

impl TextBuffer {
    /// Nearest grapheme cluster boundary at or before `idx`; an index inside a
    /// multi-char cluster snaps to the start of that cluster. Backs the vim
    /// visual range math so selection endpoints never split a cluster.
    pub fn snap_back_to_grapheme_boundary(&self, idx: usize) -> usize {
        let mut idx = idx.min(self.len_chars());
        while idx > 0 && !self.is_grapheme_boundary_index(idx) {
            idx -= 1;
        }
        idx
    }

    /// Nearest grapheme cluster boundary at or after `idx`; an index inside a
    /// multi-char cluster snaps to the end of that cluster.
    pub fn snap_forward_to_grapheme_boundary(&self, idx: usize) -> usize {
        let len = self.len_chars();
        let mut idx = idx.min(len);
        while idx < len && !self.is_grapheme_boundary_index(idx) {
            idx += 1;
        }
        idx
    }

    /// Returns `true` when a grapheme cluster boundary exists immediately
    /// before `char_index`, reusing the movement module's `&str`-based
    /// boundary check over a short context window.
    fn is_grapheme_boundary_index(&self, char_index: usize) -> bool {
        let len = self.len_chars();
        if char_index == 0 || char_index >= len {
            return true;
        }
        let context_start = char_index.saturating_sub(GRAPHEME_SNAP_CONTEXT_CHARS);
        let mut context = String::with_capacity((char_index - context_start + 1) * 4);
        for index in context_start..=char_index {
            context.push(self.rope.char(index));
        }
        let byte_index = context.len() - self.rope.char(char_index).len_utf8();
        super::movement::is_grapheme_boundary(&context, byte_index)
    }
}
