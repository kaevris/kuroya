use super::{
    BufferHistoryEditSnapshot, BufferHistoryEntrySnapshot, DeleteCoalesceKind, HistoryEntry,
    Selection, TextEdit, adjust_index, edit_delta_after, edit_range_is_valid,
    inserted_text_is_bounded, normalize_selections,
};
use ropey::Rope;
use std::ops::Range;

pub(super) fn coalesce_typing_history_entries(
    previous: &mut HistoryEntry,
    next: &HistoryEntry,
) -> bool {
    if !previous.coalescible_typing
        || !next.coalescible_typing
        || !identifier_insert_run_entry(previous)
        || !identifier_insert_entry(next)
        || previous.selections_after != next.selections_before
    {
        return false;
    }

    if previous.edits.len() != next.edits.len()
        || !previous
            .inverses
            .iter()
            .zip(&next.edits)
            .all(|(previous_inverse, next_edit)| {
                previous_inverse.range.end == next_edit.range.start
            })
    {
        return false;
    }

    let mut merged_edits = previous.edits.clone();
    for (previous_edit, next_edit) in merged_edits.iter_mut().zip(&next.edits) {
        previous_edit.inserted.push_str(&next_edit.inserted);
    }
    let Some(merged_inverse_ranges) = insert_inverse_ranges(&merged_edits) else {
        return false;
    };

    previous.edits = merged_edits;
    for (inverse, range) in previous.inverses.iter_mut().zip(merged_inverse_ranges) {
        inverse.range = range;
    }
    previous.selections_after = next.selections_after.clone();
    true
}

pub(super) fn coalesce_delete_history_entries(
    previous: &mut HistoryEntry,
    next: &HistoryEntry,
) -> bool {
    let Some(kind) = previous.coalescible_delete else {
        return false;
    };
    if next.coalescible_delete != Some(kind)
        || !single_cursor_plain_delete_run_entry_matches(previous, kind)
        || !single_cursor_plain_delete_entry_matches(next, kind)
        || previous.selections_after != next.selections_before
    {
        return false;
    }

    let previous_edit = previous.edits[0].clone();
    let next_edit = &next.edits[0];
    let next_inverse = &next.inverses[0];
    match kind {
        DeleteCoalesceKind::Backward => {
            if next_edit.range.end != previous_edit.range.start {
                return false;
            }

            let mut inserted = next_inverse.inserted.clone();
            inserted.push_str(&previous.inverses[0].inserted);
            previous.edits[0].range.start = next_edit.range.start;
            previous.inverses[0].range = next_inverse.range.clone();
            previous.inverses[0].inserted = inserted;
        }
        DeleteCoalesceKind::Forward => {
            if next_edit.range.start != previous_edit.range.start {
                return false;
            }

            previous.edits[0].range.end = previous.edits[0]
                .range
                .end
                .saturating_add(next_edit.range.end.saturating_sub(next_edit.range.start));
            previous.inverses[0]
                .inserted
                .push_str(&next_inverse.inserted);
        }
    }

    previous.selections_after = next.selections_after.clone();
    true
}

fn identifier_insert_run_entry(entry: &HistoryEntry) -> bool {
    insert_entry_matches(entry, is_identifier_insert)
}

pub(super) fn identifier_insert_entry(entry: &HistoryEntry) -> bool {
    insert_entry_matches(entry, is_identifier_insert)
}

fn insert_entry_matches(entry: &HistoryEntry, inserted_matches: impl Fn(&str) -> bool) -> bool {
    if entry.edits.is_empty()
        || entry.edits.len() != entry.inverses.len()
        || entry.edits.len() != entry.selections_before.len()
        || entry.edits.len() != entry.selections_after.len()
    {
        return false;
    }

    let Some(inverse_ranges) = insert_inverse_ranges(&entry.edits) else {
        return false;
    };

    entry
        .edits
        .iter()
        .zip(&entry.inverses)
        .zip(&entry.selections_before)
        .zip(&entry.selections_after)
        .zip(inverse_ranges)
        .all(
            |((((edit, inverse), selection_before), selection_after), inverse_range)| {
                selection_before.is_caret()
                    && selection_after.is_caret()
                    && edit.range == (selection_before.cursor..selection_before.cursor)
                    && inverse.inserted.is_empty()
                    && inverse.range == inverse_range
                    && selection_after.cursor == inverse.range.end
                    && inserted_matches(&edit.inserted)
            },
        )
}

fn insert_inverse_ranges(edits: &[TextEdit]) -> Option<Vec<Range<usize>>> {
    let mut ranges = Vec::with_capacity(edits.len());
    let mut delta = 0usize;
    for edit in edits {
        if edit.range.start != edit.range.end {
            return None;
        }

        let start = edit.range.start.checked_add(delta)?;
        let inserted_len = edit.inserted.chars().count();
        let end = start.checked_add(inserted_len)?;
        ranges.push(start..end);
        delta = delta.checked_add(inserted_len)?;
    }
    Some(ranges)
}

fn is_identifier_insert(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|ch| ch == '_' || ch.is_alphanumeric())
}

pub(super) fn single_cursor_plain_delete_entry_matches(
    entry: &HistoryEntry,
    kind: DeleteCoalesceKind,
) -> bool {
    single_cursor_plain_delete_run_entry_matches(entry, kind)
        && entry.inverses[0].inserted.chars().count() == 1
}

fn single_cursor_plain_delete_run_entry_matches(
    entry: &HistoryEntry,
    kind: DeleteCoalesceKind,
) -> bool {
    let ([edit], [inverse], [selection_before], [selection_after]) = (
        entry.edits.as_slice(),
        entry.inverses.as_slice(),
        entry.selections_before.as_slice(),
        entry.selections_after.as_slice(),
    ) else {
        return false;
    };

    let deleted_len = inverse.inserted.chars().count();
    if deleted_len == 0 || inverse.inserted.chars().any(|ch| ch == '\n') {
        return false;
    }

    selection_before.is_caret()
        && selection_after.is_caret()
        && edit.inserted.is_empty()
        && edit.range.end == edit.range.start.saturating_add(deleted_len)
        && inverse.range.start == edit.range.start
        && inverse.range.start == inverse.range.end
        && match kind {
            DeleteCoalesceKind::Backward => {
                selection_before.cursor == edit.range.end
                    && selection_after.cursor == edit.range.start
            }
            DeleteCoalesceKind::Forward => {
                selection_before.cursor == edit.range.start
                    && selection_after.cursor == edit.range.start
            }
        }
}

pub(super) enum MergedUndoGroup {
    Unchanged,

    Merged(Box<HistoryEntry>),

    ReplayFailed,
}

pub(super) fn merge_undo_history_group(
    entries: &[HistoryEntry],
    current: &Rope,
) -> MergedUndoGroup {
    if entries.len() < 2 {
        return MergedUndoGroup::ReplayFailed;
    }

    let mut replay = current.clone();
    for entry in entries.iter().rev() {
        if !apply_history_inverses_checked(&mut replay, entry) {
            return MergedUndoGroup::ReplayFailed;
        }
    }
    if !selections_replayable_at_len(&entries[0].selections_before, replay.len_chars())
        || !selections_replayable_at_len(
            entries
                .last()
                .map(|entry| entry.selections_after.as_slice())
                .unwrap_or(&[]),
            current.len_chars(),
        )
    {
        return MergedUndoGroup::ReplayFailed;
    }

    let edit = rope_pair_diff_edit(&replay, current);
    if edit.range.start == edit.range.end && edit.inserted.is_empty() {
        return MergedUndoGroup::Unchanged;
    }

    let inserted_len = edit.inserted.chars().count();
    let removed = replay.slice(edit.range.clone()).to_string();
    MergedUndoGroup::Merged(Box::new(HistoryEntry {
        edits: vec![edit.clone()],
        inverses: vec![TextEdit {
            range: edit.range.start..edit.range.start + inserted_len,
            inserted: removed,
        }],
        selections_before: entries[0].selections_before.clone(),
        selections_after: entries
            .last()
            .map(|entry| entry.selections_after.clone())
            .unwrap_or_default(),
        coalescible_typing: false,
        coalescible_delete: None,
    }))
}

fn rope_pair_diff_edit(old: &Rope, new: &Rope) -> TextEdit {
    let old_len = old.len_chars();
    let new_len = new.len_chars();
    let mut prefix = 0;
    for (old_ch, new_ch) in old.chars().zip(new.chars()) {
        if old_ch != new_ch {
            break;
        }
        prefix += 1;
    }

    let max_suffix = old_len
        .saturating_sub(prefix)
        .min(new_len.saturating_sub(prefix));
    let mut suffix = 0;
    let old_suffix = old.chars_at(old_len).reversed();
    let new_suffix = new.chars_at(new_len).reversed();
    for (old_ch, new_ch) in old_suffix.zip(new_suffix) {
        if suffix >= max_suffix || old_ch != new_ch {
            break;
        }
        suffix += 1;
    }

    TextEdit {
        range: prefix..old_len.saturating_sub(suffix),
        inserted: new
            .slice(prefix..new_len.saturating_sub(suffix))
            .to_string(),
    }
}

pub(super) fn history_entries_snapshot(
    entries: &[HistoryEntry],
    max_entries: usize,
    max_bytes: usize,
    estimated_bytes: &mut usize,
) -> Option<Vec<BufferHistoryEntrySnapshot>> {
    if max_entries == 0 {
        return Some(Vec::new());
    }

    let mut snapshots = Vec::with_capacity(max_entries.min(entries.len()));
    for entry in entries.iter().rev().take(max_entries) {
        let next_estimated_bytes =
            estimated_bytes.saturating_add(history_entry_estimated_bytes(entry));
        if next_estimated_bytes > max_bytes {
            break;
        }

        *estimated_bytes = next_estimated_bytes;
        snapshots.push(history_entry_snapshot(entry));
    }

    snapshots.reverse();
    Some(snapshots)
}

fn history_entry_snapshot(entry: &HistoryEntry) -> BufferHistoryEntrySnapshot {
    BufferHistoryEntrySnapshot {
        edits: entry.edits.iter().map(history_edit_snapshot).collect(),
        inverses: entry.inverses.iter().map(history_edit_snapshot).collect(),
        selections_before: entry.selections_before.clone(),
        selections_after: entry.selections_after.clone(),
    }
}

pub(super) fn history_entry_from_snapshot(
    snapshot: BufferHistoryEntrySnapshot,
) -> Option<HistoryEntry> {
    let edits = snapshot
        .edits
        .into_iter()
        .map(history_edit_from_snapshot)
        .collect::<Option<Vec<_>>>()?;
    let inverses = snapshot
        .inverses
        .into_iter()
        .map(history_edit_from_snapshot)
        .collect::<Option<Vec<_>>>()?;

    Some(HistoryEntry {
        edits,
        inverses,
        selections_before: snapshot.selections_before,
        selections_after: snapshot.selections_after,
        coalescible_typing: false,
        coalescible_delete: None,
    })
}

pub(super) fn history_stack_can_replay_undo(entries: &[HistoryEntry], rope: &Rope) -> bool {
    let mut replay = rope.clone();
    for entry in entries.iter().rev() {
        if !selections_replayable_at_len(&entry.selections_after, replay.len_chars()) {
            return false;
        }
        let current = replay.clone();
        if !apply_history_inverses_checked(&mut replay, entry) {
            return false;
        }
        let mut round_trip = replay.clone();
        if !apply_history_edits_checked(&mut round_trip, entry)
            || !rope_text_eq(&round_trip, &current)
        {
            return false;
        }
        if !selections_replayable_at_len(&entry.selections_before, replay.len_chars()) {
            return false;
        }
    }
    true
}

pub(super) fn history_stack_can_replay_redo(entries: &[HistoryEntry], rope: &Rope) -> bool {
    let mut replay = rope.clone();
    for entry in entries.iter().rev() {
        if !selections_replayable_at_len(&entry.selections_before, replay.len_chars()) {
            return false;
        }
        let current = replay.clone();
        if !apply_history_edits_checked(&mut replay, entry) {
            return false;
        }
        let mut round_trip = replay.clone();
        if !apply_history_inverses_checked(&mut round_trip, entry)
            || !rope_text_eq(&round_trip, &current)
        {
            return false;
        }
        if !selections_replayable_at_len(&entry.selections_after, replay.len_chars()) {
            return false;
        }
    }
    true
}

pub(super) fn selections_replayable_at_len(selections: &[Selection], len_chars: usize) -> bool {
    !selections.is_empty()
        && selections
            .iter()
            .all(|selection| selection.anchor <= len_chars && selection.cursor <= len_chars)
        && normalize_selections(selections.to_vec(), len_chars) == selections
}

pub(super) fn apply_history_edits_checked(rope: &mut Rope, entry: &HistoryEntry) -> bool {
    if entry.edits.len() != entry.inverses.len() {
        return false;
    }

    let base_len = rope.len_chars();
    let mut delta = 0_isize;
    for (edit, inverse) in entry.edits.iter().zip(entry.inverses.iter()) {
        if !edit_range_is_valid(&edit.range, base_len) || !inserted_text_is_bounded(&edit.inserted)
        {
            return false;
        }
        let len_chars = rope.len_chars();
        let adjusted_start = adjust_index(edit.range.start, delta, len_chars);
        let adjusted_end = adjust_index(edit.range.end, delta, len_chars).max(adjusted_start);
        if adjusted_start > len_chars || adjusted_end > len_chars {
            return false;
        }
        let inserted_len = edit.inserted.chars().count();
        let Some(inverse_end) = adjusted_start.checked_add(inserted_len) else {
            return false;
        };
        if inverse.range.start != adjusted_start || inverse.range.end != inverse_end {
            return false;
        }
        let adjusted_range = adjusted_start..adjusted_end;
        if rope.slice(adjusted_range.clone()) != inverse.inserted.as_str() {
            return false;
        }
        rope.remove(adjusted_range);
        rope.insert(adjusted_start, &edit.inserted);
        let Some(next_delta) = edit_delta_after(
            delta,
            inserted_len,
            edit.range.end.saturating_sub(edit.range.start),
        ) else {
            return false;
        };
        delta = next_delta;
    }
    true
}

pub(super) fn apply_history_inverses_checked(rope: &mut Rope, entry: &HistoryEntry) -> bool {
    if entry.edits.len() != entry.inverses.len() {
        return false;
    }

    let mut edits = entry
        .edits
        .iter()
        .zip(entry.inverses.iter())
        .enumerate()
        .collect::<Vec<_>>();
    edits.sort_by(|(left_idx, (_, left)), (right_idx, (_, right))| {
        right
            .range
            .start
            .cmp(&left.range.start)
            .then(right_idx.cmp(left_idx))
    });
    let mut previous_start = rope.len_chars();
    for (_, (edit, inverse)) in edits {
        if inverse.range.start > inverse.range.end
            || inverse.range.end > rope.len_chars()
            || inverse.range.end > previous_start
            || !inserted_text_is_bounded(&inverse.inserted)
        {
            return false;
        }
        if rope.slice(inverse.range.clone()) != edit.inserted.as_str() {
            return false;
        }
        rope.remove(inverse.range.clone());
        rope.insert(inverse.range.start, &inverse.inserted);
        previous_start = inverse.range.start;
    }
    true
}

fn rope_text_eq(left: &Rope, right: &Rope) -> bool {
    left == right
}

fn history_edit_snapshot(edit: &TextEdit) -> BufferHistoryEditSnapshot {
    BufferHistoryEditSnapshot {
        start: edit.range.start,
        end: edit.range.end,
        inserted: edit.inserted.clone(),
    }
}

fn history_edit_from_snapshot(snapshot: BufferHistoryEditSnapshot) -> Option<TextEdit> {
    if snapshot.start > snapshot.end {
        return None;
    }

    Some(TextEdit {
        range: snapshot.start..snapshot.end,
        inserted: snapshot.inserted,
    })
}

fn history_entry_estimated_bytes(entry: &HistoryEntry) -> usize {
    let edit_bytes = entry
        .edits
        .iter()
        .chain(entry.inverses.iter())
        .map(|edit| edit.inserted.len().saturating_add(32))
        .sum::<usize>();
    let selection_bytes = entry
        .selections_before
        .len()
        .saturating_add(entry.selections_after.len())
        .saturating_mul(16);
    edit_bytes.saturating_add(selection_bytes)
}

pub(super) fn rope_checksum(rope: &Rope) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;
    for chunk in rope.chunks() {
        for byte in chunk.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    hash
}

pub(super) fn rope_diff_to_edit(old: &Rope, new: &str) -> TextEdit {
    let old_len = old.len_chars();
    let new_len = new.chars().count();
    let mut prefix = 0;
    for (old_ch, new_ch) in old.chars().zip(new.chars()) {
        if old_ch != new_ch {
            break;
        }
        prefix += 1;
    }

    let mut suffix = 0;
    let old_suffix_chars = old.chars_at(old_len).reversed();
    for (old_ch, new_ch) in old_suffix_chars.zip(new.chars().rev()) {
        if prefix + suffix >= old_len || prefix + suffix >= new_len || old_ch != new_ch {
            break;
        }
        suffix += 1;
    }

    let inserted = new
        .chars()
        .skip(prefix)
        .take(new_len.saturating_sub(prefix + suffix))
        .collect();

    TextEdit {
        range: prefix..old_len.saturating_sub(suffix),
        inserted,
    }
}
