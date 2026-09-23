use kuroya_core::{LspFoldingRange, TextBuffer};
use std::cmp::Ordering;

mod session;

pub(crate) use session::{
    clamp_folded_ranges_for_line_count, folded_ranges_from_session, session_fold_states,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FoldedRange {
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
}

pub(crate) fn best_folding_range_starting_at(
    ranges: &[LspFoldingRange],
    line: usize,
) -> Option<FoldedRange> {
    let start = ranges.partition_point(|range| range.start_line < line);
    ranges[start..]
        .iter()
        .take_while(|range| range.start_line == line)
        .filter(|range| is_valid_lsp_folding_range(range))
        .map(|range| FoldedRange {
            start_line: range.start_line,
            end_line: range.end_line,
        })
        .min_by(|a, b| {
            a.end_line
                .saturating_sub(a.start_line)
                .cmp(&b.end_line.saturating_sub(b.start_line))
                .then(a.end_line.cmp(&b.end_line))
        })
}

pub(crate) fn fallback_folding_ranges(buffer: &TextBuffer) -> Vec<LspFoldingRange> {
    let mut ranges = Vec::new();
    collect_bracket_folding_ranges(buffer, &mut ranges);
    collect_indentation_folding_ranges(buffer, &mut ranges);
    normalize_folding_ranges(&mut ranges);
    ranges
}

pub(crate) fn indentation_folding_ranges(buffer: &TextBuffer) -> Vec<LspFoldingRange> {
    let mut ranges = Vec::new();
    collect_indentation_folding_ranges(buffer, &mut ranges);
    normalize_folding_ranges(&mut ranges);
    ranges
}

pub(crate) fn toggle_folded_range(folded: &mut Vec<FoldedRange>, range: FoldedRange) -> bool {
    normalize_folded_ranges(folded);
    if !is_valid_folded_range(&range) {
        return false;
    }

    let search = folded.binary_search_by(|candidate| {
        candidate
            .start_line
            .cmp(&range.start_line)
            .then(candidate.end_line.cmp(&range.end_line))
    });
    if let Ok(index) = search {
        folded.remove(index);
        return false;
    }

    let index = search.unwrap_err();
    folded.insert(index, range);
    true
}

pub(crate) fn fold_import_ranges_by_default(
    folded: &mut Vec<FoldedRange>,
    ranges: &[LspFoldingRange],
    enabled: bool,
) -> usize {
    if !enabled {
        return 0;
    }

    let mut import_ranges = ranges
        .iter()
        .filter(|range| range.kind.as_deref().is_some_and(is_imports_folding_kind))
        .filter(|range| is_valid_lsp_folding_range(range))
        .map(|range| FoldedRange {
            start_line: range.start_line,
            end_line: range.end_line,
        })
        .collect::<Vec<_>>();
    if import_ranges.is_empty() {
        return 0;
    }
    import_ranges.sort_unstable_by(|a, b| {
        a.start_line
            .cmp(&b.start_line)
            .then_with(|| b.end_line.cmp(&a.end_line))
    });
    import_ranges.dedup();

    let mut added = 0;
    let mut covering_end_line = 0usize;
    for range in import_ranges {
        if range.end_line <= covering_end_line {
            continue;
        }
        covering_end_line = range.end_line;
        if !folded.contains(&range) {
            folded.push(range);
            added += 1;
        }
    }
    normalize_folded_ranges(folded);
    added
}

pub(crate) fn retain_folded_ranges_matching_folding_ranges(
    folded: &mut Vec<FoldedRange>,
    ranges: &[LspFoldingRange],
) {
    if folded.is_empty() {
        return;
    }

    normalize_folded_ranges(folded);
    if folded.is_empty() {
        return;
    }

    let ranges = SortedFoldingRangeLookup::new(ranges);
    if ranges.is_empty() {
        folded.clear();
        return;
    }

    let mut retained = Vec::with_capacity(folded.len());
    for range in folded.drain(..) {
        if ranges.contains_folded_range(&range) {
            retained.push(range);
        } else if let Some(replacement) = ranges.unambiguous_range_starting_at(range.start_line) {
            retained.push(replacement);
        }
    }
    *folded = retained;
    normalize_folded_ranges(folded);
    discard_crossing_folded_ranges(folded);
}

enum SortedFoldingRangeLookup<'a> {
    Borrowed(&'a [LspFoldingRange]),
    Owned(Vec<FoldedRange>),
}

impl<'a> SortedFoldingRangeLookup<'a> {
    fn new(ranges: &'a [LspFoldingRange]) -> Self {
        if lsp_folding_ranges_are_sorted_and_valid(ranges) {
            Self::Borrowed(ranges)
        } else {
            Self::Owned(normalized_folded_ranges_from_lsp(ranges))
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Self::Borrowed(ranges) => ranges.is_empty(),
            Self::Owned(ranges) => ranges.is_empty(),
        }
    }

    fn contains_folded_range(&self, range: &FoldedRange) -> bool {
        match self {
            Self::Borrowed(ranges) => sorted_ranges_contain_folded_range(ranges, range),
            Self::Owned(ranges) => sorted_ranges_contain_folded_range(ranges, range),
        }
    }

    fn unambiguous_range_starting_at(&self, line: usize) -> Option<FoldedRange> {
        match self {
            Self::Borrowed(ranges) => unambiguous_sorted_range_starting_at(ranges, line),
            Self::Owned(ranges) => unambiguous_sorted_range_starting_at(ranges, line),
        }
    }
}

trait FoldingRangeBounds {
    fn start_line(&self) -> usize;
    fn end_line(&self) -> usize;
}

impl FoldingRangeBounds for FoldedRange {
    fn start_line(&self) -> usize {
        self.start_line
    }

    fn end_line(&self) -> usize {
        self.end_line
    }
}

impl FoldingRangeBounds for LspFoldingRange {
    fn start_line(&self) -> usize {
        self.start_line
    }

    fn end_line(&self) -> usize {
        self.end_line
    }
}

fn sorted_ranges_contain_folded_range<T: FoldingRangeBounds>(
    ranges: &[T],
    range: &FoldedRange,
) -> bool {
    ranges
        .binary_search_by(|candidate| {
            compare_range_bounds(candidate, range.start_line, range.end_line)
        })
        .is_ok()
}

fn unambiguous_sorted_range_starting_at<T: FoldingRangeBounds>(
    ranges: &[T],
    line: usize,
) -> Option<FoldedRange> {
    let start = ranges.partition_point(|range| range.start_line() < line);
    let mut matching = ranges[start..]
        .iter()
        .take_while(|range| range.start_line() == line);
    let first = matching.next()?;
    let range = FoldedRange {
        start_line: first.start_line(),
        end_line: first.end_line(),
    };
    for candidate in matching {
        if candidate.end_line() != range.end_line {
            return None;
        }
    }
    Some(range)
}

fn compare_range_bounds<T: FoldingRangeBounds>(
    range: &T,
    start_line: usize,
    end_line: usize,
) -> Ordering {
    range
        .start_line()
        .cmp(&start_line)
        .then(range.end_line().cmp(&end_line))
}

fn lsp_folding_ranges_are_sorted_and_valid(ranges: &[LspFoldingRange]) -> bool {
    let mut previous = None;
    for range in ranges {
        if !is_valid_lsp_folding_range(range) {
            return false;
        }
        let current = (range.start_line, range.end_line);
        if previous.is_some_and(|previous| current < previous) {
            return false;
        }
        previous = Some(current);
    }
    true
}

fn normalized_folded_ranges_from_lsp(ranges: &[LspFoldingRange]) -> Vec<FoldedRange> {
    let mut folded = ranges
        .iter()
        .filter(|range| is_valid_lsp_folding_range(range))
        .map(|range| FoldedRange {
            start_line: range.start_line,
            end_line: range.end_line,
        })
        .collect::<Vec<_>>();
    normalize_folded_ranges(&mut folded);
    folded
}

pub(crate) fn remove_fold_containing_line(folded: &mut Vec<FoldedRange>, line: usize) -> bool {
    normalize_folded_ranges(folded);
    let Some(index) = folded
        .iter()
        .position(|range| range.start_line <= line && line <= range.end_line)
    else {
        return false;
    };

    folded.remove(index);
    true
}

pub(crate) fn remove_folds_hiding_line(folded: &mut Vec<FoldedRange>, line: usize) -> bool {
    normalize_folded_ranges(folded);
    let before = folded.len();
    folded.retain(|range| !(range.start_line < line && line <= range.end_line));
    folded.len() != before
}

fn is_imports_folding_kind(kind: &str) -> bool {
    kind.eq_ignore_ascii_case("imports")
}

pub(crate) fn normalize_folded_ranges(folded: &mut Vec<FoldedRange>) {
    folded.retain(is_valid_folded_range);
    folded.sort_unstable_by(|a, b| {
        a.start_line
            .cmp(&b.start_line)
            .then(a.end_line.cmp(&b.end_line))
    });
    folded.dedup();
}

fn discard_crossing_folded_ranges(folded: &mut Vec<FoldedRange>) {
    if folded.len() < 2 {
        return;
    }

    let mut active_ends = Vec::<usize>::new();
    let mut retained = Vec::with_capacity(folded.len());
    let mut pending_start_end: Option<(usize, usize)> = None;
    for range in folded.drain(..) {
        if pending_start_end.is_some_and(|(start_line, _)| start_line != range.start_line) {
            if let Some((_, end_line)) = pending_start_end.take() {
                active_ends.push(end_line);
            }
        }

        while active_ends
            .last()
            .is_some_and(|end_line| *end_line < range.start_line)
        {
            active_ends.pop();
        }
        if active_ends
            .last()
            .is_some_and(|end_line| range.end_line > *end_line)
        {
            continue;
        }

        match pending_start_end.as_mut() {
            Some((_, end_line)) => *end_line = (*end_line).max(range.end_line),
            None => pending_start_end = Some((range.start_line, range.end_line)),
        }
        retained.push(range);
    }
    *folded = retained;
}

pub(crate) fn visible_line_indices(line_count: usize, folded: &[FoldedRange]) -> Vec<usize> {
    if line_count == 0 {
        return Vec::new();
    }
    if folded.is_empty() {
        return (0..line_count).collect();
    }

    let normalized;
    let (folded, capacity) =
        if let Some(capacity) = visible_line_index_capacity_if_normalized(line_count, folded) {
            (folded, capacity)
        } else {
            normalized = {
                let mut ranges = folded.to_vec();
                normalize_folded_ranges(&mut ranges);
                ranges
            };
            (
                normalized.as_slice(),
                visible_line_index_capacity(line_count, &normalized),
            )
        };

    let mut indices = Vec::with_capacity(capacity);
    let mut range_index = 0usize;
    let mut next_visible_line = 1usize;

    while range_index < folded.len() && next_visible_line <= line_count {
        let range = folded[range_index];
        if range.start_line < next_visible_line {
            range_index += 1;
            continue;
        }
        if range.start_line > line_count {
            break;
        }

        indices.extend((next_visible_line - 1)..range.start_line);

        let start_line = range.start_line;
        let mut folded_end = range.end_line.min(line_count);
        range_index += 1;
        while let Some(range) = folded
            .get(range_index)
            .filter(|range| range.start_line == start_line)
        {
            folded_end = folded_end.max(range.end_line.min(line_count));
            range_index += 1;
        }

        next_visible_line = folded_end.saturating_add(1);
    }

    if next_visible_line <= line_count {
        indices.extend((next_visible_line - 1)..line_count);
    }

    indices
}

fn visible_line_index_capacity_if_normalized(
    line_count: usize,
    folded: &[FoldedRange],
) -> Option<usize> {
    visible_line_index_capacity_impl::<true>(line_count, folded)
}

fn visible_line_index_capacity(line_count: usize, folded: &[FoldedRange]) -> usize {
    visible_line_index_capacity_impl::<false>(line_count, folded).unwrap_or(line_count)
}

fn visible_line_index_capacity_impl<const VALIDATE_NORMALIZED: bool>(
    line_count: usize,
    folded: &[FoldedRange],
) -> Option<usize> {
    let mut visible_count = 0usize;
    let mut range_index = 0usize;
    let mut next_visible_line = 1usize;
    let mut previous = None;

    while range_index < folded.len() {
        let range = folded[range_index];
        if VALIDATE_NORMALIZED {
            if !folded_range_is_normalized_after(&range, previous) {
                return None;
            }
            previous = Some((range.start_line, range.end_line));
        }
        if next_visible_line > line_count {
            if VALIDATE_NORMALIZED {
                range_index += 1;
                continue;
            }
            break;
        }
        if range.start_line < next_visible_line {
            range_index += 1;
            continue;
        }
        if range.start_line > line_count {
            if VALIDATE_NORMALIZED {
                range_index += 1;
                continue;
            }
            break;
        }

        visible_count += range.start_line.saturating_sub(next_visible_line - 1);

        let start_line = range.start_line;
        let mut folded_end = range.end_line.min(line_count);
        range_index += 1;
        while let Some(range) = folded
            .get(range_index)
            .copied()
            .filter(|range| range.start_line == start_line)
        {
            if VALIDATE_NORMALIZED {
                if !folded_range_is_normalized_after(&range, previous) {
                    return None;
                }
                previous = Some((range.start_line, range.end_line));
            }
            folded_end = folded_end.max(range.end_line.min(line_count));
            range_index += 1;
        }

        next_visible_line = folded_end.saturating_add(1);
    }

    if next_visible_line <= line_count {
        visible_count += line_count - (next_visible_line - 1);
    }

    Some(visible_count)
}

pub(crate) fn visible_row_for_line(indices: &[usize], line_idx: usize) -> usize {
    match indices.binary_search(&line_idx) {
        Ok(row) => row,
        Err(row) => row.saturating_sub(1),
    }
}

pub(crate) fn folded_range_starting_at(folded: &[FoldedRange], line: usize) -> Option<FoldedRange> {
    let start = folded.partition_point(|range| range.start_line < line);
    folded[start..]
        .iter()
        .take_while(|range| range.start_line == line)
        .copied()
        .find(is_valid_folded_range)
}

fn is_valid_folded_range(range: &FoldedRange) -> bool {
    range.start_line > 0 && range.end_line > range.start_line
}

fn folded_range_is_normalized_after(range: &FoldedRange, previous: Option<(usize, usize)>) -> bool {
    if !is_valid_folded_range(range) {
        return false;
    }
    if let Some((previous_start, previous_end)) = previous
        && (range.start_line < previous_start
            || (range.start_line == previous_start && range.end_line <= previous_end))
    {
        return false;
    }
    true
}

fn is_valid_lsp_folding_range(range: &LspFoldingRange) -> bool {
    range.start_line > 0 && range.end_line > range.start_line
}

const FALLBACK_FOLDING_RANGE_LIMIT: usize = 1_000;

#[derive(Debug, Clone, Copy)]
struct IndentLine {
    line: usize,
    indent: usize,
}

#[derive(Debug, Clone, Copy)]
struct OpenIndentFold {
    start_line: usize,
    indent: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BracketScanState {
    Code,
    BlockComment,
}

fn collect_bracket_folding_ranges(buffer: &TextBuffer, ranges: &mut Vec<LspFoldingRange>) {
    if ranges.len() >= FALLBACK_FOLDING_RANGE_LIMIT {
        return;
    }

    let mut stack = Vec::new();
    let mut scan_state = BracketScanState::Code;
    for line_idx in 0..buffer.len_lines() {
        let Some(line) = buffer.line(line_idx) else {
            continue;
        };
        let keep_scanning = scan_foldable_bracket_chars(&line, &mut scan_state, |column, ch| {
            if let Some(close) = matching_close(ch) {
                stack.push((ch, close, line_idx + 1, column + 1));
            } else if is_closing_bracket(ch)
                && let Some(index) = stack.iter().rposition(|(_, close, _, _)| *close == ch)
            {
                let (_, _, start_line, start_column) = stack.remove(index);
                let end_line = line_idx + 1;
                if end_line > start_line {
                    ranges.push(fallback_range(
                        start_line,
                        Some(start_column),
                        end_line,
                        Some(column + 1),
                    ));
                    if ranges.len() >= FALLBACK_FOLDING_RANGE_LIMIT {
                        return false;
                    }
                }
            }
            true
        });
        if !keep_scanning || ranges.len() >= FALLBACK_FOLDING_RANGE_LIMIT {
            return;
        }
    }
}

fn scan_foldable_bracket_chars(
    line: &str,
    state: &mut BracketScanState,
    mut visit: impl FnMut(usize, char) -> bool,
) -> bool {
    let mut quote = None;
    let mut escaped = false;
    let mut chars = line.chars().enumerate().peekable();

    while let Some((column, ch)) = chars.next() {
        if *state == BracketScanState::BlockComment {
            if ch == '*' && chars.next_if(|(_, next)| *next == '/').is_some() {
                *state = BracketScanState::Code;
            }
            continue;
        }

        if let Some(quote_ch) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote_ch {
                quote = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' | '`' => quote = Some(ch),
            '/' if chars.peek().is_some_and(|(_, next)| *next == '/') => break,
            '/' if chars.next_if(|(_, next)| *next == '*').is_some() => {
                *state = BracketScanState::BlockComment;
            }
            _ if (matching_close(ch).is_some() || is_closing_bracket(ch)) && !visit(column, ch) => {
                return false;
            }
            _ => {}
        }
    }

    true
}

fn collect_indentation_folding_ranges(buffer: &TextBuffer, ranges: &mut Vec<LspFoldingRange>) {
    if ranges.len() >= FALLBACK_FOLDING_RANGE_LIMIT {
        return;
    }

    let mut stack: Vec<OpenIndentFold> = Vec::new();
    let mut previous_content_line: Option<IndentLine> = None;

    for line_idx in 0..buffer.len_lines() {
        let Some(line) = buffer.line(line_idx) else {
            continue;
        };
        if line.trim().is_empty() {
            continue;
        }
        let current = IndentLine {
            line: line_idx + 1,
            indent: indentation_width(&line),
        };

        if let Some(previous) = previous_content_line
            && current.indent > previous.indent
        {
            stack.push(OpenIndentFold {
                start_line: previous.line,
                indent: previous.indent,
            });
        }

        while stack
            .last()
            .is_some_and(|open| current.indent <= open.indent)
        {
            let Some(open) = stack.pop() else {
                break;
            };
            if let Some(previous) = previous_content_line
                && previous.line > open.start_line
            {
                ranges.push(fallback_range(open.start_line, None, previous.line, None));
                if ranges.len() >= FALLBACK_FOLDING_RANGE_LIMIT {
                    return;
                }
            }
        }

        previous_content_line = Some(current);
    }

    if let Some(end_line) = previous_content_line.map(|line| line.line) {
        while let Some(open) = stack.pop() {
            if end_line > open.start_line {
                ranges.push(fallback_range(open.start_line, None, end_line, None));
                if ranges.len() >= FALLBACK_FOLDING_RANGE_LIMIT {
                    return;
                }
            }
        }
    }
}

fn normalize_folding_ranges(ranges: &mut Vec<LspFoldingRange>) {
    ranges.retain(|range| range.start_line > 0 && range.end_line > range.start_line);
    ranges.sort_unstable_by(|a, b| {
        a.start_line
            .cmp(&b.start_line)
            .then(a.end_line.cmp(&b.end_line))
            .then(a.start_column.cmp(&b.start_column))
            .then(a.end_column.cmp(&b.end_column))
            .then(a.kind.cmp(&b.kind))
    });
    ranges.dedup();
    ranges.truncate(FALLBACK_FOLDING_RANGE_LIMIT);
}

fn fallback_range(
    start_line: usize,
    start_column: Option<usize>,
    end_line: usize,
    end_column: Option<usize>,
) -> LspFoldingRange {
    LspFoldingRange {
        start_line,
        start_column,
        end_line,
        end_column,
        kind: None,
    }
}

fn indentation_width(line: &str) -> usize {
    line.chars()
        .take_while(|ch| ch.is_whitespace() && *ch != '\n' && *ch != '\r')
        .map(|ch| if ch == '\t' { 4 } else { 1 })
        .sum()
}

fn matching_close(ch: char) -> Option<char> {
    match ch {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        _ => None,
    }
}

fn is_closing_bracket(ch: char) -> bool {
    matches!(ch, ')' | ']' | '}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retain_folded_ranges_reuses_sorted_ranges_and_ignores_exact_duplicates() {
        let mut folded = vec![
            FoldedRange {
                start_line: 2,
                end_line: 5,
            },
            FoldedRange {
                start_line: 4,
                end_line: 9,
            },
            FoldedRange {
                start_line: 8,
                end_line: 10,
            },
        ];
        let ranges = vec![
            folding_range(2, 6),
            folding_range_with_kind(2, 6, "region"),
            folding_range(4, 10),
            folding_range(4, 12),
            folding_range(8, 10),
        ];

        assert!(lsp_folding_ranges_are_sorted_and_valid(&ranges));

        retain_folded_ranges_matching_folding_ranges(&mut folded, &ranges);

        assert_eq!(
            folded,
            vec![
                FoldedRange {
                    start_line: 2,
                    end_line: 6,
                },
                FoldedRange {
                    start_line: 8,
                    end_line: 10,
                },
            ]
        );
    }

    #[test]
    fn retain_folded_ranges_normalizes_unsorted_lookup_ranges() {
        let mut folded = vec![
            FoldedRange {
                start_line: 9,
                end_line: 11,
            },
            FoldedRange {
                start_line: 3,
                end_line: 5,
            },
            FoldedRange {
                start_line: 6,
                end_line: 8,
            },
        ];
        let ranges = vec![
            folding_range(0, 8),
            folding_range(6, 9),
            folding_range(3, 6),
            folding_range(6, 10),
        ];

        assert!(!lsp_folding_ranges_are_sorted_and_valid(&ranges));

        retain_folded_ranges_matching_folding_ranges(&mut folded, &ranges);

        assert_eq!(
            folded,
            vec![FoldedRange {
                start_line: 3,
                end_line: 6,
            }]
        );
    }

    #[test]
    fn retain_folded_ranges_discards_crossing_ranges_after_remap() {
        let mut folded = vec![
            FoldedRange {
                start_line: 2,
                end_line: 5,
            },
            FoldedRange {
                start_line: 4,
                end_line: 7,
            },
            FoldedRange {
                start_line: 8,
                end_line: 10,
            },
        ];
        let ranges = vec![
            folding_range(2, 6),
            folding_range(4, 8),
            folding_range(8, 10),
        ];

        retain_folded_ranges_matching_folding_ranges(&mut folded, &ranges);

        assert_eq!(
            folded,
            vec![
                FoldedRange {
                    start_line: 2,
                    end_line: 6,
                },
                FoldedRange {
                    start_line: 8,
                    end_line: 10,
                },
            ]
        );
    }

    #[test]
    fn indentation_collector_stops_at_fallback_range_limit() {
        let buffer = TextBuffer::from_text(1, None, repeated_indented_blocks().to_owned());
        let mut ranges = Vec::new();

        collect_indentation_folding_ranges(&buffer, &mut ranges);

        assert_eq!(ranges.len(), FALLBACK_FOLDING_RANGE_LIMIT);
    }

    #[test]
    fn bracket_collector_stops_at_fallback_range_limit_with_many_same_line_closures() {
        let mut text = String::new();
        for _ in 0..(FALLBACK_FOLDING_RANGE_LIMIT + 25) {
            text.push_str("{\n");
        }
        text.push_str(&"}".repeat(FALLBACK_FOLDING_RANGE_LIMIT + 25));
        let buffer = TextBuffer::from_text(1, None, text);
        let mut ranges = Vec::new();

        collect_bracket_folding_ranges(&buffer, &mut ranges);

        assert_eq!(ranges.len(), FALLBACK_FOLDING_RANGE_LIMIT);
    }

    fn repeated_indented_blocks() -> String {
        let mut text = String::new();
        for index in 0..(FALLBACK_FOLDING_RANGE_LIMIT + 25) {
            text.push_str(&format!("block_{index}\n    child_{index}\n"));
        }
        text
    }

    fn folding_range(start_line: usize, end_line: usize) -> LspFoldingRange {
        LspFoldingRange {
            start_line,
            start_column: None,
            end_line,
            end_column: None,
            kind: None,
        }
    }

    fn folding_range_with_kind(start_line: usize, end_line: usize, kind: &str) -> LspFoldingRange {
        LspFoldingRange {
            start_line,
            start_column: None,
            end_line,
            end_column: None,
            kind: Some(kind.to_owned()),
        }
    }
}
