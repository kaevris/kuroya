use std::{
    collections::{BTreeMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    path::Path,
};

use anyhow::anyhow;
use git2::{Diff, DiffFormat};

use super::paths::{GitDiffLabels, push_diff_file_header};
use super::{
    DEFAULT_DIFF_CONTEXT_LINES, DiffOptions, GitDiffHunk, GitLineChangeKind,
    MAX_DIFF_CONTEXT_LINES, MAX_DIFF_HIDE_UNCHANGED_REGIONS_MINIMUM_LINE_COUNT,
    MAX_DIFF_HIDE_UNCHANGED_REGIONS_REVEAL_LINE_COUNT, MAX_DIFF_MAX_COMPUTATION_TIME_MS,
    MAX_DIFF_MAX_FILE_SIZE_MB, MIN_DIFF_CONTEXT_LINES,
    MIN_DIFF_HIDE_UNCHANGED_REGIONS_MINIMUM_LINE_COUNT,
    MIN_DIFF_HIDE_UNCHANGED_REGIONS_REVEAL_LINE_COUNT, MIN_DIFF_MAX_COMPUTATION_TIME_MS,
    MIN_DIFF_MAX_FILE_SIZE_MB,
};

const BYTES_PER_MEGABYTE: usize = 1024 * 1024;
const MAX_EXACT_DIFF_CELLS: usize = 2_000_000;
const DIFF_EXACT_CELLS_PER_MS: usize = 2_000;

#[cfg(test)]
pub(super) fn line_change_kinds(
    old: &str,
    new: &str,
    max_lines: usize,
) -> BTreeMap<usize, GitLineChangeKind> {
    line_change_kinds_with_options(old, new, max_lines, DiffOptions::default())
}

pub(super) fn line_change_kinds_with_options(
    old: &str,
    new: &str,
    max_lines: usize,
    options: DiffOptions,
) -> BTreeMap<usize, GitLineChangeKind> {
    let lines = diff_lines_limited_with_options(old, new, max_lines, options);
    let mut changes = BTreeMap::new();
    let mut previous_new_line = None;
    let mut index = 0usize;

    while index < lines.len() {
        if let DiffLine::Context { new_line, .. } = &lines[index] {
            previous_new_line = Some(*new_line);
            index += 1;
            continue;
        }

        let change_end = lines[index..]
            .iter()
            .position(|line| !line.is_change())
            .map_or(lines.len(), |offset| index + offset);
        let mut deletion_count = 0usize;
        let mut insertions = Vec::with_capacity(change_end - index);
        while index < change_end {
            match &lines[index] {
                DiffLine::Delete { .. } => deletion_count += 1,
                DiffLine::Insert { new_line, .. } => insertions.push(*new_line),
                DiffLine::Context { .. } => {}
            }
            index += 1;
        }
        let next_new_line = lines.get(index).and_then(DiffLine::new_line);
        record_line_change_group(
            &mut changes,
            &insertions,
            deletion_count,
            previous_new_line,
            next_new_line,
        );
        previous_new_line = insertions
            .last()
            .copied()
            .or(next_new_line)
            .or(previous_new_line);
    }

    changes
}

fn record_line_change_group(
    changes: &mut BTreeMap<usize, GitLineChangeKind>,
    insertions: &[usize],
    deletion_count: usize,
    previous_new_line: Option<usize>,
    next_new_line: Option<usize>,
) {
    let paired = deletion_count.min(insertions.len());
    for line in insertions.iter().take(paired) {
        changes.insert(*line, GitLineChangeKind::Modified);
    }
    for line in insertions.iter().skip(paired) {
        changes.insert(*line, GitLineChangeKind::Added);
    }

    if deletion_count > paired {
        let anchor = insertions
            .first()
            .copied()
            .or(next_new_line)
            .or(previous_new_line)
            .unwrap_or(1);
        changes.entry(anchor).or_insert(GitLineChangeKind::Deleted);
    }
}

pub(super) fn unified_diff_for_texts(
    relative: &Path,
    old_text: Option<&str>,
    new_text: Option<&str>,
    options: DiffOptions,
) -> anyhow::Result<String> {
    let labels = GitDiffLabels::for_relative_path(relative);
    ensure_diff_inputs_within_limit(
        &labels.old_display_label,
        old_text,
        &labels.new_display_label,
        new_text,
        options,
    )?;
    if old_text.is_none() && new_text.unwrap_or_default().is_empty() {
        return Ok(String::new());
    }

    let hunk_text = unified_diff_hunks_with_options(
        old_text.unwrap_or_default(),
        new_text.unwrap_or_default(),
        options.context_lines,
        options,
    );
    if hunk_text.is_empty() {
        return Ok(String::new());
    }

    let mut diff = String::new();
    labels.push_diff_header(&mut diff);
    if old_text.is_none() {
        diff.push_str("new file mode 100644\n");
    } else if new_text.is_none() {
        diff.push_str("deleted file mode 100644\n");
    }
    push_diff_file_header(&mut diff, "---", labels.old_file_label(old_text));
    push_diff_file_header(&mut diff, "+++", labels.new_file_label(new_text));
    diff.push_str(&hunk_text);
    Ok(diff)
}

pub fn try_unified_diff_between_texts_with_options(
    old_display: &str,
    new_display: &str,
    old_text: &str,
    new_text: &str,
    options: DiffOptions,
) -> anyhow::Result<String> {
    let labels = GitDiffLabels::for_displays(old_display, new_display);
    ensure_diff_inputs_within_limit(
        &labels.old_display_label,
        Some(old_text),
        &labels.new_display_label,
        Some(new_text),
        options,
    )?;
    Ok(unified_diff_between_texts_with_labels(
        labels, old_text, new_text, options,
    ))
}

fn unified_diff_between_texts_with_labels(
    labels: GitDiffLabels,
    old_text: &str,
    new_text: &str,
    options: DiffOptions,
) -> String {
    let hunk_text =
        unified_diff_hunks_with_options(old_text, new_text, options.context_lines, options);
    if hunk_text.is_empty() {
        return String::new();
    }

    let mut diff = String::new();
    labels.push_diff_header(&mut diff);
    push_diff_file_header(&mut diff, "---", &labels.old_git_label);
    push_diff_file_header(&mut diff, "+++", &labels.new_git_label);
    diff.push_str(&hunk_text);
    diff
}

const PATCH_TRUNCATION_MARKER: &str = "\n… (diff truncated: exceeds size limit)\n";

pub(super) fn diff_to_patch_text(diff: &Diff<'_>, max_bytes: usize) -> anyhow::Result<String> {
    let mut patch = String::new();
    let mut truncated = false;
    diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
        if truncated {
            return true;
        }
        let mut chunk = String::new();
        match line.origin() {
            ' ' | '+' | '-' => chunk.push(line.origin()),
            _ => {}
        }
        chunk.push_str(&String::from_utf8_lossy(line.content()));
        if patch.len() + chunk.len() > max_bytes {
            truncated = true;
            return true;
        }
        patch.push_str(&chunk);
        true
    })?;
    if truncated {
        patch.push_str(PATCH_TRUNCATION_MARKER);
    }
    Ok(patch)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DiffLine {
    Context {
        old_line: usize,
        new_line: usize,
        text: String,
    },
    Delete {
        old_line: usize,
        text: String,
    },
    Insert {
        new_line: usize,
        text: String,
    },
}

impl DiffLine {
    fn old_line(&self) -> Option<usize> {
        match self {
            Self::Context { old_line, .. } | Self::Delete { old_line, .. } => Some(*old_line),
            Self::Insert { .. } => None,
        }
    }

    fn new_line(&self) -> Option<usize> {
        match self {
            Self::Context { new_line, .. } | Self::Insert { new_line, .. } => Some(*new_line),
            Self::Delete { .. } => None,
        }
    }

    fn is_change(&self) -> bool {
        !matches!(self, Self::Context { .. })
    }

    fn write_to(&self, output: &mut String) {
        match self {
            Self::Context { text, .. } => {
                output.push(' ');
                output.push_str(text);
            }
            Self::Delete { text, .. } => {
                output.push('-');
                output.push_str(text);
            }
            Self::Insert { text, .. } => {
                output.push('+');
                output.push_str(text);
            }
        }
        output.push('\n');
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DiffHunk {
    pub(super) old_start: usize,
    pub(super) old_lines: usize,
    pub(super) new_start: usize,
    pub(super) new_lines: usize,
    pub(super) additions: usize,
    pub(super) deletions: usize,
    pub(super) old_text: Vec<String>,
    pub(super) new_text: Vec<String>,
}

impl DiffHunk {
    pub(super) fn summary(&self, index: usize) -> GitDiffHunk {
        GitDiffHunk {
            index,
            fingerprint: self.fingerprint(),
            old_start: self.old_start,
            old_lines: self.old_lines,
            new_start: self.new_start,
            new_lines: self.new_lines,
            additions: self.additions,
            deletions: self.deletions,
            header: format!(
                "@@ -{},{} +{},{} @@",
                self.old_start, self.old_lines, self.new_start, self.new_lines
            ),
        }
    }

    pub(super) fn fingerprint(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.old_start.hash(&mut hasher);
        self.old_lines.hash(&mut hasher);
        self.new_start.hash(&mut hasher);
        self.new_lines.hash(&mut hasher);
        self.old_text.hash(&mut hasher);
        self.new_text.hash(&mut hasher);
        hasher.finish()
    }

    pub(super) fn old_range(&self) -> Option<std::ops::Range<usize>> {
        diff_hunk_line_range(self.old_start, self.old_lines)
    }

    pub(super) fn new_range(&self) -> Option<std::ops::Range<usize>> {
        diff_hunk_line_range(self.new_start, self.new_lines)
    }
}

fn diff_hunk_line_range(start_line: usize, line_count: usize) -> Option<std::ops::Range<usize>> {
    let start = if line_count == 0 {
        start_line.saturating_sub(1)
    } else {
        start_line.checked_sub(1)?
    };
    start.checked_add(line_count).map(|end| start..end)
}

#[cfg(test)]
pub(super) fn unified_diff_hunks(old: &str, new: &str, context: usize) -> String {
    unified_diff_hunks_with_options(old, new, context, DiffOptions::default())
}

fn unified_diff_hunks_with_options(
    old: &str,
    new: &str,
    context: usize,
    options: DiffOptions,
) -> String {
    let hunks = diff_hunks_with_context_with_options(old, new, context, options);
    if hunks.is_empty() {
        return String::new();
    }

    let mut output = String::new();
    for hunk in hunks {
        output.push_str(&hunk.header());
        output.push('\n');
        for line in hunk.lines {
            line.write_to(&mut output);
        }
    }

    output
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiffHunkLines {
    old_start: usize,
    old_lines: usize,
    new_start: usize,
    new_lines: usize,
    lines: Vec<DiffLine>,
}

impl DiffHunkLines {
    fn header(&self) -> String {
        format!(
            "@@ -{},{} +{},{} @@",
            self.old_start, self.old_lines, self.new_start, self.new_lines
        )
    }
}

pub(super) fn diff_hunks(old: &str, new: &str) -> Vec<DiffHunk> {
    let options = DiffOptions {
        hide_unchanged_regions_reveal_line_count: 0,
        ..DiffOptions::default()
    };
    diff_hunks_with_context_with_options(old, new, DEFAULT_DIFF_CONTEXT_LINES, options)
        .into_iter()
        .map(|hunk| {
            let mut additions = 0usize;
            let mut deletions = 0usize;
            let mut old_text = Vec::with_capacity(hunk.old_lines);
            let mut new_text = Vec::with_capacity(hunk.new_lines);
            for line in &hunk.lines {
                match line {
                    DiffLine::Context { text, .. } => {
                        old_text.push(text.clone());
                        new_text.push(text.clone());
                    }
                    DiffLine::Delete { text, .. } => {
                        deletions += 1;
                        old_text.push(text.clone());
                    }
                    DiffLine::Insert { text, .. } => {
                        additions += 1;
                        new_text.push(text.clone());
                    }
                }
            }
            DiffHunk {
                old_start: hunk.old_start,
                old_lines: hunk.old_lines,
                new_start: hunk.new_start,
                new_lines: hunk.new_lines,
                additions,
                deletions,
                old_text,
                new_text,
            }
        })
        .collect()
}

fn diff_hunks_with_context_with_options(
    old: &str,
    new: &str,
    context: usize,
    options: DiffOptions,
) -> Vec<DiffHunkLines> {
    let lines = diff_lines_with_options(old, new, options);
    let mut change_indices = Vec::with_capacity(lines.len());
    change_indices.extend(
        lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| line.is_change().then_some(index)),
    );
    if change_indices.is_empty() {
        return Vec::new();
    }
    let context = if options.hide_unchanged_regions {
        clamp_diff_context_lines(context)
    } else {
        lines.len()
    };
    let minimum_hidden_lines = if options.hide_unchanged_regions {
        clamp_diff_hide_unchanged_regions_minimum_line_count(
            options.hide_unchanged_regions_minimum_line_count,
        )
    } else {
        lines.len()
    };
    let mut hunks = Vec::with_capacity(change_indices.len());
    let mut cursor = 0usize;
    let mut previous_hunk_end = 0usize;

    while cursor < change_indices.len() {
        let first_change = change_indices[cursor];
        let mut last_change = first_change;
        while cursor + 1 < change_indices.len()
            && diff_change_gap_stays_visible(
                change_indices[cursor + 1]
                    .saturating_sub(last_change)
                    .saturating_sub(1),
                context,
                minimum_hidden_lines,
            )
        {
            cursor += 1;
            last_change = change_indices[cursor];
        }

        let start = first_change.saturating_sub(context).max(previous_hunk_end);
        let end = last_change
            .saturating_add(context)
            .saturating_add(1)
            .min(lines.len());
        let hunk = &lines[start..end];
        let mut old_start = None;
        let mut new_start = None;
        let mut old_len = 0usize;
        let mut new_len = 0usize;
        for line in hunk {
            if let Some(line) = line.old_line() {
                old_start.get_or_insert(line);
                old_len += 1;
            }
            if let Some(line) = line.new_line() {
                new_start.get_or_insert(line);
                new_len += 1;
            }
        }
        let old_start = old_start.unwrap_or_else(|| inferred_empty_hunk_start(hunk, true));
        let new_start = new_start.unwrap_or_else(|| inferred_empty_hunk_start(hunk, false));
        hunks.push(DiffHunkLines {
            old_start,
            old_lines: old_len,
            new_start,
            new_lines: new_len,
            lines: hunk.to_vec(),
        });

        previous_hunk_end = end;
        cursor += 1;
    }

    hunks
}

pub fn clamp_diff_context_lines(value: usize) -> usize {
    value.clamp(MIN_DIFF_CONTEXT_LINES, MAX_DIFF_CONTEXT_LINES)
}

pub fn clamp_diff_hide_unchanged_regions_minimum_line_count(value: usize) -> usize {
    value.clamp(
        MIN_DIFF_HIDE_UNCHANGED_REGIONS_MINIMUM_LINE_COUNT,
        MAX_DIFF_HIDE_UNCHANGED_REGIONS_MINIMUM_LINE_COUNT,
    )
}

pub fn clamp_diff_hide_unchanged_regions_reveal_line_count(value: usize) -> usize {
    value.clamp(
        MIN_DIFF_HIDE_UNCHANGED_REGIONS_REVEAL_LINE_COUNT,
        MAX_DIFF_HIDE_UNCHANGED_REGIONS_REVEAL_LINE_COUNT,
    )
}

pub fn clamp_diff_max_file_size_mb(value: usize) -> usize {
    value.clamp(MIN_DIFF_MAX_FILE_SIZE_MB, MAX_DIFF_MAX_FILE_SIZE_MB)
}

pub fn clamp_diff_max_computation_time_ms(value: usize) -> usize {
    value.clamp(
        MIN_DIFF_MAX_COMPUTATION_TIME_MS,
        MAX_DIFF_MAX_COMPUTATION_TIME_MS,
    )
}

pub fn diff_max_file_size_bytes(max_file_size_mb: usize) -> usize {
    let max_file_size_mb = clamp_diff_max_file_size_mb(max_file_size_mb);
    if max_file_size_mb == 0 {
        0
    } else {
        max_file_size_mb.saturating_mul(BYTES_PER_MEGABYTE)
    }
}

fn ensure_diff_inputs_within_limit(
    old_label: &str,
    old_text: Option<&str>,
    new_label: &str,
    new_text: Option<&str>,
    options: DiffOptions,
) -> anyhow::Result<()> {
    ensure_diff_text_within_limit(old_label, old_text, options)?;
    ensure_diff_text_within_limit(new_label, new_text, options)
}

fn ensure_diff_text_within_limit(
    label: &str,
    text: Option<&str>,
    options: DiffOptions,
) -> anyhow::Result<()> {
    let Some(text) = text else {
        return Ok(());
    };
    let max_bytes = options.max_file_size_bytes;
    if max_bytes == 0 {
        return Ok(());
    }
    let bytes = text.len();
    if bytes > max_bytes {
        anyhow::bail!("{label} is larger than {max_bytes} bytes");
    }
    Ok(())
}

pub(super) fn apply_hunk_to_old_text(
    old: &str,
    new: &str,
    hunk_index: usize,
    expected_fingerprint: Option<u64>,
) -> anyhow::Result<String> {
    let hunk = selected_hunk(old, new, hunk_index, expected_fingerprint)?;
    apply_hunk_with_original_endings(
        old,
        hunk.old_range()
            .ok_or_else(|| anyhow!("git hunk range is invalid"))?,
        &hunk.old_text,
        new,
        hunk.new_range()
            .ok_or_else(|| anyhow!("git hunk range is invalid"))?,
        &hunk.new_text,
    )
}

pub(super) fn apply_hunk_to_new_text(
    old: &str,
    new: &str,
    hunk_index: usize,
    expected_fingerprint: Option<u64>,
) -> anyhow::Result<String> {
    let hunk = selected_hunk(old, new, hunk_index, expected_fingerprint)?;
    apply_hunk_with_original_endings(
        new,
        hunk.new_range()
            .ok_or_else(|| anyhow!("git hunk range is invalid"))?,
        &hunk.new_text,
        old,
        hunk.old_range()
            .ok_or_else(|| anyhow!("git hunk range is invalid"))?,
        &hunk.old_text,
    )
}

fn selected_hunk(
    old: &str,
    new: &str,
    hunk_index: usize,
    expected_fingerprint: Option<u64>,
) -> anyhow::Result<DiffHunk> {
    let hunk = diff_hunks(old, new)
        .into_iter()
        .nth(hunk_index)
        .ok_or_else(|| anyhow!("git hunk {hunk_index} was not found"))?;
    if expected_fingerprint.is_some_and(|fingerprint| hunk.fingerprint() != fingerprint) {
        return Err(anyhow!("git hunk no longer matches the selected hunk"));
    }
    Ok(hunk)
}

struct SourceLine<'a> {
    raw: &'a str,
    content: &'a str,
}

fn split_source_lines(text: &str) -> Vec<SourceLine<'_>> {
    let mut lines = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let (raw, remainder) = match rest.find('\n') {
            Some(newline) => rest.split_at(newline + 1),
            None => (rest, ""),
        };
        lines.push(SourceLine {
            raw,
            content: source_line_content(raw),
        });
        rest = remainder;
    }
    lines
}

fn source_line_content(raw: &str) -> &str {
    raw.strip_suffix("\r\n")
        .or_else(|| raw.strip_suffix('\n'))
        .or_else(|| raw.strip_suffix('\r'))
        .unwrap_or(raw)
}

fn apply_hunk_with_original_endings(
    source: &str,
    source_range: std::ops::Range<usize>,
    source_expected: &[String],
    replacement_source: &str,
    replacement_range: std::ops::Range<usize>,
    replacement_expected: &[String],
) -> anyhow::Result<String> {
    let source_lines = split_source_lines(source);

    let mut contents = source_lines
        .iter()
        .map(|line| line.content.to_owned())
        .collect::<Vec<_>>();
    replace_hunk_lines(
        &mut contents,
        source_range.clone(),
        source_expected,
        replacement_expected.to_vec(),
    )?;

    let replacement_lines = split_source_lines(replacement_source);
    let mut raw_lines = Vec::with_capacity(
        source_lines
            .len()
            .saturating_sub(source_range.len())
            .saturating_add(replacement_range.len()),
    );
    raw_lines.extend(source_lines[..source_range.start].iter().map(|l| l.raw));
    raw_lines.extend(
        replacement_lines
            .get(replacement_range)
            .into_iter()
            .flatten()
            .map(|l| l.raw),
    );
    raw_lines.extend(source_lines[source_range.end..].iter().map(|l| l.raw));
    Ok(join_source_lines_with_endings(
        &raw_lines,
        text_ends_with_newline(source, replacement_source),
    ))
}

pub(super) fn replace_hunk_lines(
    lines: &mut Vec<String>,
    range: std::ops::Range<usize>,
    expected: &[String],
    replacement: Vec<String>,
) -> anyhow::Result<()> {
    if range.start > range.end || range.end > lines.len() {
        return Err(anyhow!("git hunk is outside the current file contents"));
    }
    if range.end - range.start != expected.len() {
        return Err(anyhow!("git hunk range is invalid"));
    }
    if &lines[range.clone()] != expected {
        return Err(anyhow!(
            "git hunk no longer matches the current file contents"
        ));
    }

    lines.splice(range, replacement);
    Ok(())
}

fn join_source_lines_with_endings(raw_lines: &[&str], final_newline: bool) -> String {
    let mut text = raw_lines.concat();
    let Some(last) = raw_lines.last() else {
        return String::new();
    };
    if final_newline {
        if !last.ends_with('\n') {
            text.push('\n');
        }
    } else if last.ends_with("\r\n") {
        text.truncate(text.len() - "\r\n".len());
    } else if last.ends_with('\n') {
        text.truncate(text.len() - 1);
    }
    text
}

fn text_ends_with_newline(primary: &str, fallback: &str) -> bool {
    primary.ends_with('\n') || (primary.is_empty() && fallback.ends_with('\n'))
}

fn inferred_empty_hunk_start(hunk: &[DiffLine], old_side: bool) -> usize {
    let reference = hunk.iter().find_map(|line| {
        if old_side {
            line.new_line()
        } else {
            line.old_line()
        }
    });
    reference.unwrap_or(1).saturating_sub(1)
}

fn diff_change_gap_stays_visible(
    unchanged_gap_lines: usize,
    context_lines: usize,
    minimum_hidden_lines: usize,
) -> bool {
    unchanged_gap_lines <= context_lines || unchanged_gap_lines < minimum_hidden_lines
}

fn exact_diff_cells_allowed(exact_cells: usize, max_computation_time_ms: usize) -> bool {
    if exact_cells > MAX_EXACT_DIFF_CELLS {
        return false;
    }
    let max_computation_time_ms = clamp_diff_max_computation_time_ms(max_computation_time_ms);
    if max_computation_time_ms == 0 {
        return true;
    }
    exact_cells <= max_computation_time_ms.saturating_mul(DIFF_EXACT_CELLS_PER_MS)
}

fn diff_lines_with_options(old: &str, new: &str, options: DiffOptions) -> Vec<DiffLine> {
    let old_lines = old.lines().collect::<Vec<_>>();
    let new_lines = new.lines().collect::<Vec<_>>();
    diff_lines_from_slices(&old_lines, &new_lines, options)
}

fn diff_lines_limited_with_options(
    old: &str,
    new: &str,
    max_lines: usize,
    options: DiffOptions,
) -> Vec<DiffLine> {
    let old_lines = old.lines().take(max_lines).collect::<Vec<_>>();
    let new_lines = new.lines().take(max_lines).collect::<Vec<_>>();
    diff_lines_from_slices(&old_lines, &new_lines, options)
}

fn diff_lines_from_slices(
    old_lines: &[&str],
    new_lines: &[&str],
    options: DiffOptions,
) -> Vec<DiffLine> {
    if options.algorithm.uses_advanced_diff() {
        let exact_cells = old_lines.len().saturating_mul(new_lines.len());
        if exact_diff_cells_allowed(exact_cells, options.max_computation_time_ms) {
            exact_diff_lines(old_lines, new_lines, options)
        } else {
            positional_diff_lines(old_lines, new_lines, options)
        }
    } else {
        positional_diff_lines(old_lines, new_lines, options)
    }
}

fn exact_diff_lines(old_lines: &[&str], new_lines: &[&str], options: DiffOptions) -> Vec<DiffLine> {
    let old_count = old_lines.len();
    let new_count = new_lines.len();
    let mut lcs = vec![0usize; (old_count + 1) * (new_count + 1)];
    let idx = |old: usize, new: usize| old * (new_count + 1) + new;

    for old in (0..old_count).rev() {
        for new in (0..new_count).rev() {
            lcs[idx(old, new)] = if diff_lines_equal(old_lines[old], new_lines[new], options) {
                lcs[idx(old + 1, new + 1)] + 1
            } else {
                lcs[idx(old + 1, new)].max(lcs[idx(old, new + 1)])
            };
        }
    }

    let mut lines = Vec::with_capacity(old_count.saturating_add(new_count));
    let mut old = 0usize;
    let mut new = 0usize;
    while old < old_count || new < new_count {
        if old < old_count
            && new < new_count
            && diff_lines_equal(old_lines[old], new_lines[new], options)
        {
            lines.push(DiffLine::Context {
                old_line: old + 1,
                new_line: new + 1,
                text: old_lines[old].to_owned(),
            });
            old += 1;
            new += 1;
        } else if old < old_count
            && (new >= new_count || lcs[idx(old + 1, new)] >= lcs[idx(old, new + 1)])
        {
            lines.push(DiffLine::Delete {
                old_line: old + 1,
                text: old_lines[old].to_owned(),
            });
            old += 1;
        } else if new < new_count {
            lines.push(DiffLine::Insert {
                new_line: new + 1,
                text: new_lines[new].to_owned(),
            });
            new += 1;
        }
    }

    lines
}

fn positional_diff_lines(
    old_lines: &[&str],
    new_lines: &[&str],
    options: DiffOptions,
) -> Vec<DiffLine> {
    let count = old_lines.len().max(new_lines.len());
    let mut lines = Vec::with_capacity(old_lines.len().saturating_add(new_lines.len()));
    for index in 0..count {
        match (old_lines.get(index), new_lines.get(index)) {
            (Some(old), Some(new)) if diff_lines_equal(old, new, options) => {
                lines.push(DiffLine::Context {
                    old_line: index + 1,
                    new_line: index + 1,
                    text: (*old).to_owned(),
                })
            }
            (Some(old), Some(new)) => {
                lines.push(DiffLine::Delete {
                    old_line: index + 1,
                    text: (*old).to_owned(),
                });
                lines.push(DiffLine::Insert {
                    new_line: index + 1,
                    text: (*new).to_owned(),
                });
            }
            (Some(old), None) => lines.push(DiffLine::Delete {
                old_line: index + 1,
                text: (*old).to_owned(),
            }),
            (None, Some(new)) => lines.push(DiffLine::Insert {
                new_line: index + 1,
                text: (*new).to_owned(),
            }),
            (None, None) => {}
        }
    }
    lines
}

fn diff_lines_equal(old: &str, new: &str, options: DiffOptions) -> bool {
    if options.ignore_trim_whitespace {
        old.trim() == new.trim()
    } else {
        old == new
    }
}
