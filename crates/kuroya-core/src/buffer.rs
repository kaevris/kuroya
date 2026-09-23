mod brackets;
mod edits;
mod find;
mod history;
mod line_ops;
mod lines;
mod merge_conflicts;
mod movement;
mod save;
mod selection;
mod text;
mod types;

pub(crate) use self::find::regex_query_is_line_local;
pub use self::find::validate_find_regex;
pub use self::save::clean_text_for_save;
pub use self::types::{
    AutoPairSettings, BracketColor, BracketPairGuide, BufferHistoryEditSnapshot,
    BufferHistoryEntrySnapshot, BufferHistorySnapshot, CursorPosition, LineSnapshot,
    RegexReplaceAllOptions, Selection, TextEdit, TextSnapshot,
};

use self::brackets::{
    auto_pair_close, auto_pair_close_enabled_for, auto_pair_enabled_for, bracket_color_depth,
    brackets_match, is_auto_pair_close, is_bracket, is_closing_bracket, is_opening_bracket,
    matching_pair, next_non_whitespace_char, previous_non_whitespace_char,
};
use self::edits::{
    adjust_index, apply_delta, edit_delta_after, edit_range_is_valid, edits_are_replayable,
    inserted_end_selections_after_edits, inserted_selections_after_edit, inserted_text_is_bounded,
    normalize_cursor_edits, normalize_edits, normalize_selections, transform_duplicate_position,
    transform_line_move_position, transform_selection_after_edits,
};
#[cfg(test)]
use self::find::{find_result_capacity, regex_match_ranges};
use self::history::{
    MergedUndoGroup, apply_history_edits_checked, apply_history_inverses_checked,
    coalesce_delete_history_entries, coalesce_typing_history_entries, history_entries_snapshot,
    history_entry_from_snapshot, history_stack_can_replay_redo, history_stack_can_replay_undo,
    identifier_insert_entry, merge_undo_history_group, rope_checksum, rope_diff_to_edit,
    selections_replayable_at_len, single_cursor_plain_delete_entry_matches,
};
#[cfg(test)]
use self::text::rope_slice_text;
use self::types::{CursorEdit, DeleteCoalesceKind, HistoryEntry, LineDuplicateEdit, LineMoveEdit};
#[cfg(test)]
use crate::conflicts::{MergeConflict, MergeConflictResolution};
use crate::syntax::{LanguageConfiguration, LanguageId};
#[cfg(test)]
use regex::Regex;
use ropey::Rope;
use std::{ops::Range, path::PathBuf};

pub type BufferId = u64;
const MAX_UNDO_ENTRIES: usize = 256;
const REGEX_FULL_TEXT_MAX_BYTES: usize = 2 * 1024 * 1024;
const MAX_EDIT_DELTA_CHARS: usize = isize::MAX as usize;
const MAX_BRACKET_SCAN_CHARS: usize = 200_000;
pub const DEFAULT_WORD_SEPARATORS: &str = "`~!@#$%^&*()-=+[{]}\\|;:'\",.<>/?";

#[derive(Debug, Clone)]
pub struct TextBuffer {
    id: BufferId,
    path: Option<PathBuf>,
    language: LanguageId,
    rope: Rope,
    version: u64,
    dirty: bool,

    baseline_checksum: u64,

    baseline_version: u64,
    selections: Vec<Selection>,
    word_separators: String,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    read_only: bool,

    undo_group_start: Option<usize>,
}

impl TextBuffer {
    pub fn new_untitled(id: BufferId) -> Self {
        Self::from_text(id, None, String::new())
    }

    pub fn from_text(id: BufferId, path: Option<PathBuf>, text: String) -> Self {
        let language = path
            .as_deref()
            .map(LanguageId::from_path)
            .unwrap_or(LanguageId::PlainText);
        Self::from_text_with_language(id, path, text, language)
    }

    pub fn from_text_with_language(
        id: BufferId,
        path: Option<PathBuf>,
        text: String,
        language: LanguageId,
    ) -> Self {
        let rope = Rope::from_str(&text);
        let baseline_checksum = rope_checksum(&rope);
        Self {
            id,
            path,
            language,
            rope,
            version: 0,
            dirty: false,
            baseline_checksum,
            baseline_version: 0,
            selections: vec![Selection::caret(0)],
            word_separators: DEFAULT_WORD_SEPARATORS.to_owned(),
            undo: Vec::new(),
            redo: Vec::new(),
            read_only: false,
            undo_group_start: None,
        }
    }

    pub fn id(&self) -> BufferId {
        self.id
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    pub fn set_path(&mut self, path: PathBuf) {
        self.language = LanguageId::from_path(&path);
        self.path = Some(path);
    }

    pub fn language(&self) -> LanguageId {
        self.language
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    pub fn mark_saved(&mut self) {
        self.baseline_checksum = rope_checksum(&self.rope);
        self.baseline_version = self.version;
        self.dirty = false;
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn mark_saved_if_text_matches(&mut self, disk_text: &str) -> bool {
        if !self.text_equals(disk_text) {
            return false;
        }
        self.mark_saved();
        true
    }

    pub fn replace_from_disk(&mut self, text: String) {
        self.rope = Rope::from_str(&text);
        self.finish_disk_replacement();
    }

    pub fn replace_from_disk_buffer(&mut self, replacement: TextBuffer) {
        self.rope = replacement.rope;
        self.finish_disk_replacement();
    }

    fn finish_disk_replacement(&mut self) {
        self.version = self.version.saturating_add(1);
        self.undo.clear();
        self.redo.clear();
        self.undo_group_start = None;
        self.selections =
            normalize_selections(std::mem::take(&mut self.selections), self.len_chars());
        self.mark_saved();
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn selections(&self) -> &[Selection] {
        &self.selections
    }

    pub fn has_selection(&self) -> bool {
        self.selections
            .iter()
            .any(|selection| !selection.is_caret())
    }

    pub fn set_word_separators(&mut self, separators: impl Into<String>) {
        self.word_separators = separators.into();
    }

    pub fn word_separators(&self) -> &str {
        &self.word_separators
    }

    pub fn undo_entry_count(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_entry_count(&self) -> usize {
        self.redo.len()
    }

    pub fn history_snapshot(
        &self,
        max_entries_per_stack: usize,
        max_bytes: usize,
    ) -> Option<BufferHistorySnapshot> {
        if self.undo.is_empty() && self.redo.is_empty() {
            return None;
        }

        let mut estimated_bytes = 0;
        let undo = history_entries_snapshot(
            &self.undo,
            max_entries_per_stack,
            max_bytes,
            &mut estimated_bytes,
        )?;
        let redo = history_entries_snapshot(
            &self.redo,
            max_entries_per_stack,
            max_bytes,
            &mut estimated_bytes,
        )?;
        if undo.is_empty() && redo.is_empty() {
            return None;
        }

        Some(BufferHistorySnapshot {
            len_chars: self.len_chars(),
            checksum: rope_checksum(&self.rope),
            undo,
            redo,
        })
    }

    pub fn restore_history_snapshot(&mut self, snapshot: BufferHistorySnapshot) -> bool {
        if snapshot.len_chars != self.len_chars() || snapshot.checksum != rope_checksum(&self.rope)
        {
            return false;
        }

        let Some(undo) = snapshot
            .undo
            .into_iter()
            .map(history_entry_from_snapshot)
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        let Some(redo) = snapshot
            .redo
            .into_iter()
            .map(history_entry_from_snapshot)
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };

        if !history_stack_can_replay_undo(&undo, &self.rope)
            || !history_stack_can_replay_redo(&redo, &self.rope)
        {
            return false;
        }

        self.undo = undo;
        self.redo = redo;
        self.undo_group_start = None;
        self.prune_undo();
        true
    }

    pub fn clear_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.undo_group_start = None;
    }

    pub fn begin_undo_group(&mut self) {
        if self.undo_group_start.is_none() {
            self.undo_group_start = Some(self.undo.len());
        }
    }

    pub fn end_undo_group(&mut self) {
        let Some(start) = self.undo_group_start.take() else {
            return;
        };
        if start >= self.undo.len() {
            return;
        }
        if self.undo.len() - start == 1 {
            self.undo[start].selections_after = self.selections.clone();
            return;
        }

        let entries = self.undo.split_off(start);
        match merge_undo_history_group(&entries, &self.rope) {
            MergedUndoGroup::ReplayFailed => self.undo.extend(entries),
            MergedUndoGroup::Unchanged => {}
            MergedUndoGroup::Merged(mut entry) => {
                entry.selections_after = self.selections.clone();
                self.undo.push(*entry);
                self.prune_undo();
            }
        }
    }

    pub fn apply_edit(&mut self, edit: TextEdit) {
        self.apply_transaction(vec![edit]);
    }

    pub fn apply_edits(&mut self, edits: Vec<TextEdit>) -> bool {
        self.apply_transaction(edits)
    }

    pub fn apply_edits_with_inserted_selection(
        &mut self,
        edits: Vec<TextEdit>,
        primary_edit: &TextEdit,
        inserted_selection: Range<usize>,
    ) -> bool {
        self.apply_edits_with_inserted_selections(edits, primary_edit, &[inserted_selection])
            .is_some()
    }

    pub fn apply_edits_with_inserted_selections(
        &mut self,
        edits: Vec<TextEdit>,
        primary_edit: &TextEdit,
        inserted_selections: &[Range<usize>],
    ) -> Option<Vec<Range<usize>>> {
        let edits = normalize_edits(edits, self.len_chars())?;
        let inserted_ranges = inserted_selections_after_edit(
            &edits,
            primary_edit,
            inserted_selections,
            self.len_chars(),
        )?;
        let selections_after = inserted_ranges
            .first()
            .map(|range| {
                vec![Selection {
                    anchor: range.start,
                    cursor: range.end,
                }]
            })
            .unwrap_or_else(|| inserted_end_selections_after_edits(&edits, self.len_chars()));
        self.apply_normalized_transaction_with_selections(edits, selections_after)
            .then_some(inserted_ranges)
    }

    pub fn insert_at_cursor(&mut self, text: &str) {
        self.insert_at_cursors(text);
    }

    pub fn insert_at_cursors(&mut self, text: &str) {
        self.apply_insert_at_cursors(text);
    }

    pub fn insert_texts_at_cursors(&mut self, texts: Vec<String>) -> bool {
        if texts.len() != self.selections.len() || texts.iter().all(String::is_empty) {
            return false;
        }

        let edits = self
            .selections
            .iter()
            .copied()
            .zip(texts)
            .map(|(selection, text)| {
                let cursor_offset = text.chars().count();
                CursorEdit {
                    edit: TextEdit {
                        range: selection.range(),
                        inserted: text,
                    },
                    cursor_offset,
                }
            })
            .collect();
        self.apply_transaction_with_cursor_offsets(edits)
    }

    pub fn insert_text_with_auto_pairs(&mut self, text: &str) -> bool {
        self.insert_text_with_auto_pair_settings(text, AutoPairSettings::default())
    }

    pub fn insert_text_with_auto_pair_settings(
        &mut self,
        text: &str,
        settings: AutoPairSettings,
    ) -> bool {
        self.insert_text_with_auto_pair_config(text, settings, self.language.configuration())
    }

    fn insert_text_with_auto_pair_config(
        &mut self,
        text: &str,
        settings: AutoPairSettings,
        language_config: LanguageConfiguration,
    ) -> bool {
        let mut chars = text.chars();
        let Some(ch) = chars.next() else {
            return false;
        };
        if chars.next().is_some() {
            return self.apply_insert_at_cursors(text);
        }

        if settings.overtype
            && is_auto_pair_close(ch, language_config)
            && auto_pair_close_enabled_for(ch, settings, language_config)
            && self.skip_auto_pair_close(ch)
        {
            return false;
        }
        if is_auto_pair_close(ch, language_config)
            && auto_pair_close_enabled_for(ch, settings, language_config)
            && self.insert_outdented_auto_pair_close(ch, language_config)
        {
            return true;
        }

        if let Some(close) = auto_pair_close(ch, language_config) {
            if !auto_pair_enabled_for(ch, settings) {
                return self.apply_insert_at_cursors(text);
            }

            if self.selections.iter().all(|selection| selection.is_caret()) {
                let inserted = format!("{ch}{close}");
                let edits = self
                    .selections
                    .iter()
                    .map(|selection| CursorEdit {
                        edit: TextEdit {
                            range: selection.range(),
                            inserted: inserted.clone(),
                        },
                        cursor_offset: 1,
                    })
                    .collect();
                return self.apply_transaction_with_cursor_offsets(edits);
            }

            if settings.surround {
                return self.surround_selections_with_pair(ch, close);
            }
        }

        self.apply_insert_at_cursors(text)
    }

    fn surround_selections_with_pair(&mut self, open: char, close: char) -> bool {
        let mut edits = Vec::new();
        let mut selections_after = Vec::new();
        let mut delta = 0_isize;
        let mut last_end = 0;

        for selection in self.selections.iter().copied() {
            let range = selection.range();
            if !edits.is_empty() && range.start < last_end {
                continue;
            }

            let selected = self.rope.slice(range.start..range.end).to_string();
            let inserted = format!("{open}{selected}{close}");
            let adjusted_start = apply_delta(range.start, delta);
            let selected_len = range.end.saturating_sub(range.start);
            let inner_start = adjusted_start + 1;
            let inner_end = inner_start + selected_len;
            let selection_after = if selection.is_caret() {
                Selection::caret(inner_start)
            } else if selection.anchor <= selection.cursor {
                Selection {
                    anchor: inner_start,
                    cursor: inner_end,
                }
            } else {
                Selection {
                    anchor: inner_end,
                    cursor: inner_start,
                }
            };

            edits.push(TextEdit { range, inserted });
            selections_after.push(selection_after);
            delta += 2;
            last_end = edits.last().map_or(last_end, |edit| edit.range.end);
        }

        self.apply_normalized_transaction_with_selections(edits, selections_after)
    }

    pub fn insert_newline_with_indent(&mut self) {
        self.insert_newline_with_indent_unit("    ");
    }

    pub fn insert_newline_with_indent_unit(&mut self, indent_unit: &str) {
        self.insert_newline_with_indent_overrides(indent_unit, &[]);
    }

    pub fn insert_newline_with_indent_overrides(
        &mut self,
        indent_unit: &str,
        indent_overrides: &[Option<String>],
    ) {
        self.insert_newline_with_language_config_and_indent_overrides(
            indent_unit,
            self.language.configuration(),
            indent_overrides,
        );
    }

    pub fn insert_newline_with_language_config(
        &mut self,
        indent_unit: &str,
        language_config: LanguageConfiguration,
    ) {
        self.insert_newline_with_language_config_and_indent_overrides(
            indent_unit,
            language_config,
            &[],
        );
    }

    pub fn insert_newline_with_language_config_and_indent_overrides(
        &mut self,
        indent_unit: &str,
        language_config: LanguageConfiguration,
        indent_overrides: &[Option<String>],
    ) {
        let newline = self.preferred_line_ending();
        let edits = self
            .selections
            .iter()
            .enumerate()
            .map(|(index, selection)| {
                let range = selection.range();
                let cursor = selection.cursor.min(self.len_chars());
                let line = self.char_position(cursor).line;
                let base_indent = self.line_indent(line);
                let line_prefix = self.line_prefix_before_cursor(line, range.start);
                let override_indent = indent_overrides
                    .get(index)
                    .and_then(|indent| indent.as_ref());
                let before = previous_non_whitespace_char(&self.rope, range.start);
                let after = next_non_whitespace_char(&self.rope, range.end);
                let should_indent = language_config.increase_indent_after_line(&line_prefix);
                let should_split_pair = before
                    .zip(after)
                    .is_some_and(|(open, close)| brackets_match(open, close));

                let (inserted, cursor_offset) = if should_split_pair {
                    let inner_indent = format!("{base_indent}{indent_unit}");
                    (
                        format!("{newline}{inner_indent}{newline}{base_indent}"),
                        newline.chars().count() + inner_indent.chars().count(),
                    )
                } else if should_indent {
                    let inserted = format!("{newline}{base_indent}{indent_unit}");
                    let cursor_offset = inserted.chars().count();
                    (inserted, cursor_offset)
                } else if let Some(indent) = override_indent {
                    let inserted = format!("{newline}{indent}");
                    let cursor_offset = inserted.chars().count();
                    (inserted, cursor_offset)
                } else {
                    let inserted = format!("{newline}{base_indent}");
                    let cursor_offset = inserted.chars().count();
                    (inserted, cursor_offset)
                };
                CursorEdit {
                    edit: TextEdit { range, inserted },
                    cursor_offset,
                }
            })
            .collect();
        self.apply_transaction_with_cursor_offsets(edits);
    }

    fn line_prefix_before_cursor(&self, line: usize, cursor: usize) -> String {
        if line >= self.len_lines() {
            return String::new();
        }

        let start = self.rope.line_to_char(line);
        let end = cursor.min(self.line_content_end_char(line)).max(start);
        self.rope.slice(start..end).to_string()
    }

    fn line_indent(&self, line: usize) -> String {
        self.line(line)
            .unwrap_or_default()
            .chars()
            .take_while(|ch| *ch == ' ' || *ch == '\t')
            .collect()
    }

    fn apply_insert_at_cursors(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        let edits = self
            .selections
            .iter()
            .map(|selection| TextEdit {
                range: selection.range(),
                inserted: text.to_owned(),
            })
            .collect();
        self.apply_typing_transaction(edits)
    }

    fn skip_auto_pair_close(&mut self, close: char) -> bool {
        if !self.selections.iter().all(|selection| {
            selection.is_caret()
                && selection.cursor < self.len_chars()
                && self.rope.char(selection.cursor) == close
        }) {
            return false;
        }

        let cursors = self
            .selections
            .iter()
            .map(|selection| selection.cursor + 1)
            .collect::<Vec<_>>();
        self.set_cursors(cursors);
        true
    }

    fn insert_outdented_auto_pair_close(
        &mut self,
        close: char,
        language_config: LanguageConfiguration,
    ) -> bool {
        let mut edits = Vec::new();
        for selection in self.selections.iter().copied() {
            if !selection.is_caret() {
                return false;
            }
            let cursor = selection.cursor.min(self.len_chars());
            let line = self.char_position(cursor).line;
            if !self.line_is_blank(line) {
                return false;
            }
            let Some(open_idx) = self.matching_open_before_cursor(cursor, close, language_config)
            else {
                return false;
            };
            let open_line = self.char_position(open_idx).line;
            let line_start = self.rope.line_to_char(line);
            let line_end = self.line_content_end_char(line);
            let inserted = format!("{}{}", self.line_indent(open_line), close);
            let cursor_offset = inserted.chars().count();
            edits.push(CursorEdit {
                edit: TextEdit {
                    range: line_start..line_end,
                    inserted,
                },
                cursor_offset,
            });
        }

        self.apply_transaction_with_cursor_offsets(edits)
    }

    fn matching_open_before_cursor(
        &self,
        cursor: usize,
        close: char,
        language_config: LanguageConfiguration,
    ) -> Option<usize> {
        let target_open = language_config
            .brackets()
            .iter()
            .find_map(|pair| (pair.close == close).then_some(pair.open))?;
        let mut expected_opens = Vec::new();
        for idx in (0..cursor.min(self.len_chars())).rev() {
            let ch = self.rope.char(idx);
            if let Some(open) = language_config
                .brackets()
                .iter()
                .find_map(|pair| (pair.close == ch).then_some(pair.open))
            {
                expected_opens.push(open);
            } else if language_config
                .brackets()
                .iter()
                .any(|pair| pair.open == ch)
            {
                if expected_opens.last().copied() == Some(ch) {
                    expected_opens.pop();
                } else if ch == target_open {
                    return Some(idx);
                }
            }
        }
        None
    }

    pub fn matching_bracket(&self) -> Option<(usize, usize)> {
        self.matching_bracket_for_cursor(self.cursor())
    }

    pub fn matching_brackets(&self) -> Vec<(usize, usize)> {
        self.selections
            .iter()
            .filter_map(|selection| self.matching_bracket_for_cursor(selection.cursor))
            .collect()
    }

    pub fn matching_brackets_including_enclosing(&self) -> Vec<(usize, usize)> {
        self.selections
            .iter()
            .filter_map(|selection| {
                self.matching_bracket_for_cursor(selection.cursor)
                    .or_else(|| self.enclosing_bracket_for_cursor(selection.cursor))
            })
            .collect()
    }

    pub fn bracket_block_selection_range_at(&self, cursor: usize) -> Option<Range<usize>> {
        let (a, b) = self.matching_bracket_for_cursor(cursor)?;
        let start = a.min(b).saturating_add(1);
        let end = a.max(b);
        (start < end).then_some(start..end)
    }

    pub fn bracket_colors_for_range(&self, range: Range<usize>) -> Vec<BracketColor> {
        self.bracket_colors_for_range_with_options(range, false)
    }

    pub fn bracket_colors_for_range_with_options(
        &self,
        range: Range<usize>,
        independent_color_pool_per_bracket_type: bool,
    ) -> Vec<BracketColor> {
        let start = range.start.min(self.len_chars());
        let end = range.end.min(self.len_chars()).max(start);
        if start == end {
            return Vec::new();
        }

        let context_start = start.saturating_sub(MAX_BRACKET_SCAN_CHARS);
        let mut stack = Vec::new();
        for idx in context_start..start {
            let ch = self.rope.char(idx);
            if is_opening_bracket(ch) {
                stack.push(ch);
            } else if is_closing_bracket(ch)
                && stack
                    .last()
                    .copied()
                    .is_some_and(|open| brackets_match(open, ch))
            {
                stack.pop();
            }
        }

        let mut colors = Vec::new();
        let scan_end = end.min(start.saturating_add(MAX_BRACKET_SCAN_CHARS));
        for idx in start..scan_end {
            let ch = self.rope.char(idx);
            if is_opening_bracket(ch) {
                let depth =
                    bracket_color_depth(&stack, ch, true, independent_color_pool_per_bracket_type);
                colors.push(BracketColor {
                    char_idx: idx,
                    depth,
                });
                stack.push(ch);
            } else if is_closing_bracket(ch) {
                let depth =
                    bracket_color_depth(&stack, ch, false, independent_color_pool_per_bracket_type);
                colors.push(BracketColor {
                    char_idx: idx,
                    depth,
                });
                if stack
                    .last()
                    .copied()
                    .is_some_and(|open| brackets_match(open, ch))
                {
                    stack.pop();
                }
            }
        }

        colors
    }

    pub fn bracket_colors_for_lines(&self, first_line: usize, count: usize) -> Vec<BracketColor> {
        self.bracket_colors_for_lines_with_options(first_line, count, false)
    }

    pub fn bracket_colors_for_lines_with_options(
        &self,
        first_line: usize,
        count: usize,
        independent_color_pool_per_bracket_type: bool,
    ) -> Vec<BracketColor> {
        if first_line >= self.len_lines() || count == 0 {
            return Vec::new();
        }

        let last_line = first_line
            .saturating_add(count)
            .min(self.len_lines())
            .saturating_sub(1);
        let start = self.rope.line_to_char(first_line);
        let end = if last_line + 1 < self.len_lines() {
            self.rope.line_to_char(last_line + 1)
        } else {
            self.len_chars()
        };
        self.bracket_colors_for_range_with_options(
            start..end,
            independent_color_pool_per_bracket_type,
        )
    }

    pub fn bracket_pair_guides(&self) -> Vec<BracketPairGuide> {
        #[derive(Clone, Copy)]
        struct StackEntry {
            ch: char,
            char_idx: usize,
            depth: usize,
        }

        let mut stack = Vec::<StackEntry>::new();
        let mut guides = Vec::new();
        for idx in 0..self.len_chars().min(MAX_BRACKET_SCAN_CHARS) {
            let ch = self.rope.char(idx);
            if is_opening_bracket(ch) {
                stack.push(StackEntry {
                    ch,
                    char_idx: idx,
                    depth: stack.len(),
                });
            } else if is_closing_bracket(ch)
                && stack
                    .last()
                    .is_some_and(|entry| brackets_match(entry.ch, ch))
                && let Some(entry) = stack.pop()
            {
                guides.push(BracketPairGuide {
                    open_idx: entry.char_idx,
                    close_idx: idx,
                    depth: entry.depth,
                });
            }
        }
        guides
    }

    fn matching_bracket_for_cursor(&self, cursor: usize) -> Option<(usize, usize)> {
        let cursor = cursor.min(self.len_chars());
        let anchor = if cursor > 0 && is_bracket(self.rope.char(cursor - 1)) {
            cursor - 1
        } else if cursor < self.len_chars() && is_bracket(self.rope.char(cursor)) {
            cursor
        } else {
            return None;
        };

        let bracket = self.rope.char(anchor);
        let target = matching_pair(bracket)?;
        if is_opening_bracket(bracket) {
            self.matching_closing_bracket_from(anchor, bracket, target)
        } else {
            self.matching_opening_bracket_from(anchor, bracket, target)
        }
    }

    fn matching_closing_bracket_from(
        &self,
        anchor: usize,
        bracket: char,
        target: char,
    ) -> Option<(usize, usize)> {
        let scan_end = self.len_chars().min(
            anchor
                .saturating_add(MAX_BRACKET_SCAN_CHARS)
                .saturating_add(1),
        );
        let mut depth = 0usize;
        for idx in anchor..scan_end {
            let ch = self.rope.char(idx);
            if ch == bracket {
                depth = depth.saturating_add(1);
            } else if ch == target {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some((anchor, idx));
                }
            }
        }
        None
    }

    fn matching_opening_bracket_from(
        &self,
        anchor: usize,
        bracket: char,
        target: char,
    ) -> Option<(usize, usize)> {
        let scan_start = anchor.saturating_sub(MAX_BRACKET_SCAN_CHARS);
        let mut depth = 0usize;
        for idx in (scan_start..=anchor).rev() {
            let ch = self.rope.char(idx);
            if ch == bracket {
                depth = depth.saturating_add(1);
            } else if ch == target {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some((anchor, idx));
                }
            }
        }
        None
    }

    fn enclosing_bracket_for_cursor(&self, cursor: usize) -> Option<(usize, usize)> {
        let cursor = cursor.min(self.len_chars());
        let mut stack = Vec::<(usize, char)>::new();
        let mut best: Option<(usize, usize)> = None;

        let scan_start = cursor.saturating_sub(MAX_BRACKET_SCAN_CHARS);
        let scan_end = self.len_chars().min(
            cursor
                .saturating_add(MAX_BRACKET_SCAN_CHARS)
                .saturating_add(1),
        );
        for idx in scan_start..scan_end {
            let ch = self.rope.char(idx);
            if is_opening_bracket(ch) {
                stack.push((idx, ch));
            } else if is_closing_bracket(ch)
                && stack
                    .last()
                    .copied()
                    .is_some_and(|(_, open)| brackets_match(open, ch))
                && let Some((open_idx, _)) = stack.pop()
                && open_idx < cursor
                && cursor < idx
            {
                let candidate = (open_idx, idx);
                if best.is_none_or(|(best_open, best_close)| {
                    idx.saturating_sub(open_idx) < best_close.saturating_sub(best_open)
                }) {
                    best = Some(candidate);
                }
            }
        }

        best
    }

    pub fn replace_text_from_ui(&mut self, new_text: &str) {
        let edit = rope_diff_to_edit(&self.rope, new_text);
        if edit.range.start != edit.range.end || !edit.inserted.is_empty() {
            self.apply_edit(edit);
        }
    }

    pub fn undo(&mut self) -> bool {
        if self.read_only {
            return false;
        }

        let Some(entry) = self.undo.pop() else {
            return false;
        };
        if !selections_replayable_at_len(&entry.selections_after, self.len_chars()) {
            self.undo.push(entry);
            return false;
        }

        let mut replay = self.rope.clone();
        if !apply_history_inverses_checked(&mut replay, &entry)
            || !selections_replayable_at_len(&entry.selections_before, replay.len_chars())
        {
            self.undo.push(entry);
            return false;
        }

        self.rope = replay;
        self.version = self.version.saturating_add(1);
        self.recompute_dirty_after_mutation();
        self.selections = entry.selections_before.clone();
        self.redo.push(entry);
        self.prune_undo();
        true
    }

    pub fn redo(&mut self) -> bool {
        if self.read_only {
            return false;
        }

        let Some(entry) = self.redo.pop() else {
            return false;
        };
        if !selections_replayable_at_len(&entry.selections_before, self.len_chars()) {
            self.redo.push(entry);
            return false;
        }

        let mut replay = self.rope.clone();
        if !apply_history_edits_checked(&mut replay, &entry)
            || !selections_replayable_at_len(&entry.selections_after, replay.len_chars())
        {
            self.redo.push(entry);
            return false;
        }

        self.rope = replay;
        self.version = self.version.saturating_add(1);
        self.recompute_dirty_after_mutation();
        self.selections = entry.selections_after.clone();
        self.undo.push(entry);
        self.prune_undo();
        true
    }

    fn apply_transaction(&mut self, edits: Vec<TextEdit>) -> bool {
        self.apply_transaction_with_history_options(edits, false)
    }

    fn apply_typing_transaction(&mut self, edits: Vec<TextEdit>) -> bool {
        self.apply_transaction_with_history_options(edits, true)
    }

    fn apply_delete_transaction(&mut self, edits: Vec<TextEdit>, kind: DeleteCoalesceKind) -> bool {
        self.apply_delete_transaction_with_history_options(edits, kind)
    }

    fn apply_delete_transaction_with_history_options(
        &mut self,
        edits: Vec<TextEdit>,
        kind: DeleteCoalesceKind,
    ) -> bool {
        if self.read_only {
            return false;
        }

        let Some(edits) = normalize_edits(edits, self.len_chars()) else {
            return false;
        };
        if edits.is_empty() {
            return false;
        }

        let selections_before = self.selections.clone();
        let inverses = self.apply_edits_inner(&edits);
        let selections_after = selections_before
            .iter()
            .copied()
            .map(|selection| transform_selection_after_edits(selection, &edits))
            .collect::<Vec<_>>();
        self.selections = normalize_selections(selections_after, self.len_chars());
        let entry = HistoryEntry {
            edits,
            inverses,
            selections_before,
            selections_after: self.selections.clone(),
            coalescible_typing: false,
            coalescible_delete: None,
        };
        let coalescible_delete = single_cursor_plain_delete_entry_matches(&entry, kind);
        self.push_undo_history_entry(HistoryEntry {
            coalescible_delete: coalescible_delete.then_some(kind),
            ..entry
        });
        self.redo.clear();
        true
    }

    fn apply_transaction_with_history_options(
        &mut self,
        edits: Vec<TextEdit>,
        coalesce_typing: bool,
    ) -> bool {
        if self.read_only {
            return false;
        }

        let Some(edits) = normalize_edits(edits, self.len_chars()) else {
            return false;
        };
        if edits.is_empty() {
            return false;
        }

        let selections_before = self.selections.clone();
        let inverses = self.apply_edits_inner(&edits);
        let selections_after = selections_before
            .iter()
            .copied()
            .map(|selection| transform_selection_after_edits(selection, &edits))
            .collect::<Vec<_>>();
        self.selections = normalize_selections(selections_after, self.len_chars());
        let entry = HistoryEntry {
            edits,
            inverses,
            selections_before,
            selections_after: self.selections.clone(),
            coalescible_typing: false,
            coalescible_delete: None,
        };
        let coalesce_typing = coalesce_typing && identifier_insert_entry(&entry);
        self.push_undo_history_entry(HistoryEntry {
            coalescible_typing: coalesce_typing,
            ..entry
        });
        self.redo.clear();
        true
    }

    fn push_undo_history_entry(&mut self, entry: HistoryEntry) {
        let grouping = self.undo_group_start.is_some();
        if !grouping
            && entry.coalescible_typing
            && self
                .undo
                .last_mut()
                .is_some_and(|previous| coalesce_typing_history_entries(previous, &entry))
        {
            self.prune_undo();
            return;
        }

        if !grouping
            && entry.coalescible_delete.is_some()
            && self
                .undo
                .last_mut()
                .is_some_and(|previous| coalesce_delete_history_entries(previous, &entry))
        {
            self.prune_undo();
            return;
        }

        self.undo.push(entry);
        self.prune_undo();
    }

    fn push_undo_history_entry_without_coalescing(
        &mut self,
        edits: Vec<TextEdit>,
        inverses: Vec<TextEdit>,
        selections_before: Vec<Selection>,
    ) {
        self.push_undo_history_entry(HistoryEntry {
            edits,
            inverses,
            selections_before,
            selections_after: self.selections.clone(),
            coalescible_typing: false,
            coalescible_delete: None,
        });
    }

    fn apply_transaction_with_cursor_offsets(&mut self, edits: Vec<CursorEdit>) -> bool {
        if self.read_only {
            return false;
        }

        let Some(edits) = normalize_cursor_edits(edits, self.len_chars()) else {
            return false;
        };
        if edits.is_empty() {
            return false;
        }

        let selections_before = self.selections.clone();
        let mut delta = 0_isize;
        let mut inverses = Vec::with_capacity(edits.len());
        let mut selections_after = Vec::with_capacity(edits.len());

        for cursor_edit in &edits {
            let edit = &cursor_edit.edit;
            let current_len = self.rope.len_chars();
            let adjusted_start = adjust_index(edit.range.start, delta, current_len);
            let adjusted_end = adjust_index(edit.range.end, delta, current_len).max(adjusted_start);
            let removed = self.rope.slice(adjusted_start..adjusted_end).to_string();
            self.rope.remove(adjusted_start..adjusted_end);
            self.rope.insert(adjusted_start, &edit.inserted);

            let inserted_len = edit.inserted.chars().count();
            let removed_len = edit.range.end.saturating_sub(edit.range.start);
            inverses.push(TextEdit {
                range: adjusted_start..adjusted_start + inserted_len,
                inserted: removed,
            });
            selections_after.push(Selection::caret(
                adjusted_start + cursor_edit.cursor_offset.min(inserted_len),
            ));
            delta += inserted_len as isize - removed_len as isize;
        }

        self.version = self.version.saturating_add(1);
        self.recompute_dirty_after_mutation();
        self.selections = normalize_selections(selections_after, self.len_chars());
        self.push_undo_history_entry_without_coalescing(
            edits
                .into_iter()
                .map(|cursor_edit| cursor_edit.edit)
                .collect(),
            inverses,
            selections_before,
        );
        self.redo.clear();
        true
    }

    fn apply_linewise_transaction(&mut self, edits: Vec<TextEdit>) -> bool {
        let Some(edits) = normalize_edits(edits, self.len_chars()) else {
            return false;
        };
        if edits.is_empty() {
            return false;
        }

        let selections_after = self
            .selections
            .iter()
            .map(|selection| transform_selection_after_edits(*selection, &edits))
            .collect::<Vec<_>>();
        self.apply_normalized_transaction_with_selections(edits, selections_after)
    }

    fn apply_line_duplicate_transaction(&mut self, edits: Vec<LineDuplicateEdit>) -> bool {
        if edits.is_empty() {
            return false;
        }

        let text_edits = edits
            .iter()
            .map(|duplicate| duplicate.edit.clone())
            .collect::<Vec<_>>();
        let selections_after = self
            .selections
            .iter()
            .map(|selection| Selection {
                anchor: transform_duplicate_position(selection.anchor, &edits, &text_edits),
                cursor: transform_duplicate_position(selection.cursor, &edits, &text_edits),
            })
            .collect::<Vec<_>>();
        let Some(text_edits) = normalize_edits(text_edits, self.len_chars()) else {
            return false;
        };
        self.apply_normalized_transaction_with_selections(text_edits, selections_after)
    }

    fn apply_line_move_transaction(&mut self, edits: Vec<LineMoveEdit>) -> bool {
        if edits.is_empty() {
            return false;
        }

        let text_edits = edits
            .iter()
            .map(|line_move| line_move.edit.clone())
            .collect::<Vec<_>>();
        let selections_after = self
            .selections
            .iter()
            .map(|selection| Selection {
                anchor: transform_line_move_position(self, selection.anchor, &edits, &text_edits),
                cursor: transform_line_move_position(self, selection.cursor, &edits, &text_edits),
            })
            .collect::<Vec<_>>();
        let Some(text_edits) = normalize_edits(text_edits, self.len_chars()) else {
            return false;
        };
        self.apply_normalized_transaction_with_selections(text_edits, selections_after)
    }

    fn apply_normalized_transaction_with_selections(
        &mut self,
        edits: Vec<TextEdit>,
        selections_after: Vec<Selection>,
    ) -> bool {
        if self.read_only {
            return false;
        }

        if edits.is_empty() || !edits_are_replayable(&edits, self.len_chars()) {
            return false;
        }

        let selections_before = self.selections.clone();
        let inverses = self.apply_edits_inner(&edits);
        self.selections = normalize_selections(selections_after, self.len_chars());
        self.push_undo_history_entry_without_coalescing(edits, inverses, selections_before);
        self.redo.clear();
        true
    }

    fn prune_undo(&mut self) {
        let excess = self.undo.len().saturating_sub(MAX_UNDO_ENTRIES);
        if excess > 0 {
            self.undo.drain(0..excess);
        }
    }

    fn apply_edits_inner(&mut self, edits: &[TextEdit]) -> Vec<TextEdit> {
        let mut delta = 0_isize;
        let mut inverses = Vec::with_capacity(edits.len());

        for edit in edits {
            let current_len = self.rope.len_chars();
            let adjusted_start = adjust_index(edit.range.start, delta, current_len);
            let adjusted_end = adjust_index(edit.range.end, delta, current_len).max(adjusted_start);
            let removed = self.rope.slice(adjusted_start..adjusted_end).to_string();
            self.rope.remove(adjusted_start..adjusted_end);
            self.rope.insert(adjusted_start, &edit.inserted);

            let inserted_len = edit.inserted.chars().count();
            let removed_len = edit.range.end.saturating_sub(edit.range.start);
            inverses.push(TextEdit {
                range: adjusted_start..adjusted_start + inserted_len,
                inserted: removed,
            });
            delta += inserted_len as isize - removed_len as isize;
        }

        self.version = self.version.saturating_add(1);
        self.recompute_dirty_after_mutation();
        inverses
    }

    fn recompute_dirty_after_mutation(&mut self) {
        self.dirty = self.version != self.baseline_version
            && rope_checksum(&self.rope) != self.baseline_checksum;
    }
}

#[cfg(test)]
mod tests;
