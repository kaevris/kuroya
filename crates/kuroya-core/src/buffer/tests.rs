use super::*;

mod brackets_autopair_save;
mod core;
mod dirty_state;
mod find_replace;
mod history;
mod line_edits;
mod merge_conflicts;
mod save;
mod selections;

fn selection_positions(buffer: &TextBuffer) -> Vec<((usize, usize), (usize, usize))> {
    buffer
        .selections()
        .iter()
        .map(|selection| {
            let anchor = buffer.char_position(selection.anchor);
            let cursor = buffer.char_position(selection.cursor);
            ((anchor.line, anchor.column), (cursor.line, cursor.column))
        })
        .collect()
}
