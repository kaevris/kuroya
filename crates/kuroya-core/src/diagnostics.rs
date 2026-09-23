use crate::workspace_paths::lexical_normalize_cow;
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    ops::Range,
    path::{Path, PathBuf},
};

mod display_text;
mod static_analysis;

pub use display_text::diagnostic_display_text;
#[cfg(test)]
use display_text::{is_diagnostic_display_format_control, sanitize_diagnostic_display_text_cow};
#[cfg(test)]
use static_analysis::scan_common_markers;
pub use static_analysis::{analyze_text, static_diagnostics_scan_allowed};

pub const STATIC_DIAGNOSTIC_SOURCE: &str = "kuroya-static";
pub const STATIC_DIAGNOSTIC_SCAN_MAX_BYTES: usize = 4 * 1024 * 1024;
pub const STATIC_DIAGNOSTIC_MAX_RESULTS: usize = 2048;
pub const DIAGNOSTIC_SET_MAX_PATH_RESULTS: usize = 8192;
pub const DIAGNOSTIC_DISPLAY_TEXT_MAX_CHARS: usize = 512;
const RESERVED_LSP_DIAGNOSTIC_SOURCE_PREFIX: &str = "lsp: ";
const STATIC_DIAGNOSTIC_MAX_BRACKET_DEPTH: usize = 4096;
const DIAGNOSTIC_DISPLAY_TRUNCATION_MARKER: &str = "...";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LspDiagnosticUnits {
    Chars,

    Utf16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiagnosticSeverityCounts {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub hints: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub char_range: Range<usize>,
    pub severity: DiagnosticSeverity,
    pub source: String,
    pub message: String,
    #[serde(default)]
    pub unused: bool,
    #[serde(default)]
    pub deprecated: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DiagnosticSet {
    by_path: HashMap<PathBuf, Vec<Diagnostic>>,
    ordered_paths: Vec<PathBuf>,
    counts_by_severity: [usize; 4],

    lsp_buckets_by_path: HashMap<PathBuf, HashMap<String, Vec<Diagnostic>>>,

    raw_utf16_lsp_keys: HashSet<(PathBuf, String)>,
}

const DEFAULT_LSP_DIAGNOSTIC_SOURCE_KEY: &str = "kuroya:lsp-default";

impl DiagnosticSet {
    pub fn replace(&mut self, path: PathBuf, mut diagnostics: Vec<Diagnostic>) {
        let path = normalize_diagnostic_path_owned(path);

        self.lsp_buckets_by_path.remove(&path);
        self.raw_utf16_lsp_keys
            .retain(|(marker_path, _)| marker_path != &path);
        if diagnostics.is_empty() {
            self.remove_path_counts(&path);
            return;
        }

        normalize_diagnostics_for_path(&path, &mut diagnostics);
        limit_diagnostics_for_path(&mut diagnostics);
        sort_and_dedup_diagnostics(&mut diagnostics);
        self.remove_path_counts(&path);
        self.insert_path_diagnostics(path, diagnostics);
    }

    fn remove_path_counts(&mut self, path: &Path) {
        if let Some(existing) = self.by_path.remove(path) {
            self.subtract_counts(&existing);
        }
        self.remove_ordered_path(path);
    }

    fn insert_path_diagnostics(&mut self, path: PathBuf, diagnostics: Vec<Diagnostic>) {
        if diagnostics.is_empty() {
            self.by_path.remove(&path);
            self.remove_ordered_path(&path);
        } else {
            self.add_counts(&diagnostics);
            self.insert_ordered_path(&path);
            self.by_path.insert(path, diagnostics);
        }
    }

    fn insert_ordered_path(&mut self, path: &Path) {
        if let Err(index) = self
            .ordered_paths
            .binary_search_by(|candidate| candidate.as_path().cmp(path))
        {
            self.ordered_paths.insert(index, path.to_path_buf());
        }
    }

    fn remove_ordered_path(&mut self, path: &Path) {
        if let Ok(mut index) = self
            .ordered_paths
            .binary_search_by(|candidate| candidate.as_path().cmp(path))
        {
            while index > 0 && self.ordered_paths[index - 1].as_path() == path {
                index -= 1;
            }
            let duplicate_count = self.ordered_paths[index..]
                .partition_point(|candidate| candidate.as_path() == path);
            self.ordered_paths.drain(index..index + duplicate_count);
        }
    }

    fn add_counts(&mut self, diagnostics: &[Diagnostic]) {
        for diagnostic in diagnostics {
            self.counts_by_severity[severity_index(diagnostic.severity)] += 1;
        }
    }

    fn subtract_counts(&mut self, diagnostics: &[Diagnostic]) {
        for diagnostic in diagnostics {
            let count = &mut self.counts_by_severity[severity_index(diagnostic.severity)];
            *count = count.saturating_sub(1);
        }
    }

    pub fn replace_static(&mut self, path: PathBuf, diagnostics: Vec<Diagnostic>) {
        self.replace_matching_source(path, diagnostics, diagnostic_is_static);
    }

    pub fn replace_lsp(&mut self, path: PathBuf, diagnostics: Vec<Diagnostic>) {
        self.replace_lsp_source(path, DEFAULT_LSP_DIAGNOSTIC_SOURCE_KEY, diagnostics);
    }

    pub fn replace_lsp_units(
        &mut self,
        path: PathBuf,
        diagnostics: Vec<Diagnostic>,
        units: LspDiagnosticUnits,
    ) {
        self.replace_lsp_source_units(path, DEFAULT_LSP_DIAGNOSTIC_SOURCE_KEY, diagnostics, units);
    }

    pub fn replace_lsp_source(
        &mut self,
        path: PathBuf,
        source_key: &str,
        diagnostics: Vec<Diagnostic>,
    ) {
        self.replace_lsp_source_units(path, source_key, diagnostics, LspDiagnosticUnits::Chars);
    }

    pub fn replace_lsp_source_units(
        &mut self,
        path: PathBuf,
        source_key: &str,
        diagnostics: Vec<Diagnostic>,
        units: LspDiagnosticUnits,
    ) {
        let path = normalize_diagnostic_path_owned(path);
        let marker = (path.clone(), source_key.to_owned());
        if diagnostics.is_empty() {
            self.raw_utf16_lsp_keys.remove(&marker);
        } else {
            match units {
                LspDiagnosticUnits::Chars => {
                    self.raw_utf16_lsp_keys.remove(&marker);
                }
                LspDiagnosticUnits::Utf16 => {
                    self.raw_utf16_lsp_keys.insert(marker);
                }
            }
        }
        self.store_lsp_bucket(path, source_key, diagnostics);
    }

    pub fn raw_utf16_lsp_paths(&self) -> Vec<PathBuf> {
        let mut paths = self
            .raw_utf16_lsp_keys
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>();
        paths.sort_unstable();
        paths.dedup();
        paths
    }

    pub fn drain_raw_utf16_lsp_buckets(&mut self, path: &Path) -> Vec<(String, Vec<Diagnostic>)> {
        let path = normalize_diagnostic_path_cow(path).into_owned();
        let keys = self
            .raw_utf16_lsp_keys
            .iter()
            .filter(|(marker_path, _)| marker_path == &path)
            .map(|(_, key)| key.clone())
            .collect::<Vec<_>>();
        let mut drained = Vec::with_capacity(keys.len());
        for key in keys {
            self.raw_utf16_lsp_keys.remove(&(path.clone(), key.clone()));
            if let Some(buckets) = self.lsp_buckets_by_path.get_mut(&path)
                && let Some(diagnostics) = buckets.remove(&key)
            {
                drained.push((key, diagnostics));
            }
        }
        if !drained.is_empty() {
            self.rebuild_path_from_lsp_buckets(path);
        }
        drained
    }

    pub fn purge_lsp_source(&mut self, source_key: &str) {
        let mut purged_paths = Vec::new();
        self.lsp_buckets_by_path.retain(|path, buckets| {
            let purged = buckets.remove(source_key).is_some();
            if purged {
                purged_paths.push(path.clone());
            }
            !buckets.is_empty()
        });
        self.raw_utf16_lsp_keys.retain(|(_, key)| key != source_key);
        for path in purged_paths {
            self.rebuild_path_from_lsp_buckets(path);
        }
    }

    fn store_lsp_bucket(
        &mut self,
        path: PathBuf,
        source_key: &str,
        mut diagnostics: Vec<Diagnostic>,
    ) {
        reserve_lsp_diagnostic_sources(&mut diagnostics);
        normalize_diagnostics_for_path(&path, &mut diagnostics);
        limit_diagnostics_for_path(&mut diagnostics);
        let buckets = self.lsp_buckets_by_path.entry(path.clone()).or_default();
        if diagnostics.is_empty() {
            buckets.remove(source_key);
        } else {
            buckets.insert(source_key.to_owned(), diagnostics);
        }
        if buckets.is_empty() {
            self.lsp_buckets_by_path.remove(&path);
        }
        self.rebuild_path_from_lsp_buckets(path);
    }

    fn rebuild_path_from_lsp_buckets(&mut self, path: PathBuf) {
        let mut merged: Vec<Diagnostic> = Vec::new();
        if let Some(existing) = self.by_path.get(&path) {
            merged.extend(existing.iter().filter(|d| diagnostic_is_static(d)).cloned());
        }
        if let Some(buckets) = self.lsp_buckets_by_path.get(&path) {
            for bucket in buckets.values() {
                merged.extend(bucket.iter().cloned());
            }
        }
        self.remove_path_counts(&path);
        sort_and_dedup_diagnostics(&mut merged);
        limit_diagnostics_for_path(&mut merged);
        self.insert_path_diagnostics(path, merged);
    }

    fn replace_matching_source(
        &mut self,
        path: PathBuf,
        mut diagnostics: Vec<Diagnostic>,
        should_replace: impl Fn(&Diagnostic) -> bool,
    ) {
        let path = normalize_diagnostic_path_owned(path);
        normalize_diagnostics_for_path(&path, &mut diagnostics);
        limit_diagnostics_for_path(&mut diagnostics);
        let mut merged = self.by_path.remove(&path).unwrap_or_default();
        self.subtract_counts(&merged);
        self.remove_ordered_path(&path);
        merged.retain(|diagnostic| !should_replace(diagnostic));
        merged.reserve(
            diagnostics
                .len()
                .min(DIAGNOSTIC_SET_MAX_PATH_RESULTS.saturating_sub(merged.len())),
        );
        merged.append(&mut diagnostics);
        sort_and_dedup_diagnostics(&mut merged);
        limit_diagnostics_for_path(&mut merged);
        self.insert_path_diagnostics(path, merged);
    }

    pub fn for_path(&self, path: &Path) -> &[Diagnostic] {
        if let Some(diagnostics) = self.by_path.get(path) {
            return diagnostics;
        }
        match normalize_diagnostic_path_cow(path) {
            Cow::Borrowed(_) => &[],
            Cow::Owned(path) => self
                .by_path
                .get(path.as_path())
                .map(Vec::as_slice)
                .unwrap_or(&[]),
        }
    }

    pub fn for_line(&self, path: &Path, line: usize) -> Vec<&Diagnostic> {
        let diagnostics = self.for_path(path);
        let range = diagnostic_line_range(diagnostics, line);
        let mut line_diagnostics = Vec::with_capacity(range.len());
        line_diagnostics.extend(diagnostics[range].iter());
        line_diagnostics
    }

    pub fn iter_for_line(&self, path: &Path, line: usize) -> impl Iterator<Item = &Diagnostic> {
        let diagnostics = self.for_path(path);
        let range = diagnostic_line_range(diagnostics, line);
        diagnostics[range].iter()
    }

    pub fn all(&self) -> impl Iterator<Item = &Diagnostic> {
        self.by_path.values().flatten()
    }

    pub fn sorted(&self) -> impl Iterator<Item = &Diagnostic> {
        self.ordered_paths
            .iter()
            .filter_map(|path| self.by_path.get(path))
            .flatten()
    }

    pub fn all_sorted(&self) -> Vec<&Diagnostic> {
        let mut diagnostics = Vec::with_capacity(self.len());
        diagnostics.extend(self.sorted());
        diagnostics
    }

    pub fn get_sorted(&self, index: usize) -> Option<&Diagnostic> {
        let mut remaining = index;
        for path in &self.ordered_paths {
            let Some(diagnostics) = self.by_path.get(path) else {
                continue;
            };
            if remaining < diagnostics.len() {
                return diagnostics.get(remaining);
            }
            remaining -= diagnostics.len();
        }
        None
    }

    pub fn sorted_range(&self, range: Range<usize>) -> impl Iterator<Item = (usize, &Diagnostic)> {
        let total_len = self.len();
        let start = range.start.min(total_len);
        let end = range.end.min(total_len);
        let (path_index, diagnostic_index) = self.sorted_position(start);
        DiagnosticSortedRange {
            diagnostics: self,
            path_index,
            diagnostic_index,
            next_index: start,
            end_index: end,
        }
    }

    fn sorted_position(&self, index: usize) -> (usize, usize) {
        let mut remaining = index;
        for (path_index, path) in self.ordered_paths.iter().enumerate() {
            let Some(diagnostics) = self.by_path.get(path) else {
                continue;
            };
            if remaining < diagnostics.len() {
                return (path_index, remaining);
            }
            remaining = remaining.saturating_sub(diagnostics.len());
        }
        (self.ordered_paths.len(), 0)
    }

    pub fn first(&self) -> Option<&Diagnostic> {
        self.ordered_paths
            .iter()
            .filter_map(|path| self.by_path.get(path))
            .find_map(|diagnostics| diagnostics.first())
    }

    pub fn last(&self) -> Option<&Diagnostic> {
        self.ordered_paths
            .iter()
            .rev()
            .filter_map(|path| self.by_path.get(path))
            .find_map(|diagnostics| diagnostics.last())
    }

    pub fn next_after(&self, path: &Path, line: usize, column: usize) -> Option<&Diagnostic> {
        if self.is_empty() {
            return None;
        }

        let (path, search) = self.navigation_path_search(path);
        let path = path.as_ref();
        let start = search.unwrap_or_else(|index| index);
        self.next_in_paths(start..self.ordered_paths.len(), path, line, column)
            .or_else(|| self.next_in_paths(0..start, path, line, column))
            .or_else(|| self.first())
    }

    pub fn previous_before(&self, path: &Path, line: usize, column: usize) -> Option<&Diagnostic> {
        if self.is_empty() {
            return None;
        }

        let (path, search) = self.navigation_path_search(path);
        let path = path.as_ref();
        match search {
            Ok(index) => self
                .previous_in_paths((0..=index).rev(), path, line, column)
                .or_else(|| {
                    self.previous_in_paths(
                        ((index + 1)..self.ordered_paths.len()).rev(),
                        path,
                        line,
                        column,
                    )
                }),
            Err(index) => self
                .previous_in_paths((0..index).rev(), path, line, column)
                .or_else(|| {
                    self.previous_in_paths(
                        (index..self.ordered_paths.len()).rev(),
                        path,
                        line,
                        column,
                    )
                }),
        }
        .or_else(|| self.last())
    }

    fn navigation_path_search<'a>(&self, path: &'a Path) -> (Cow<'a, Path>, Result<usize, usize>) {
        let search = self.normalized_path_search(path);
        if search.is_ok() {
            return (Cow::Borrowed(path), search);
        }

        match normalize_diagnostic_path_cow(path) {
            Cow::Borrowed(_) => (Cow::Borrowed(path), search),
            Cow::Owned(path) => {
                let search = self.normalized_path_search(path.as_path());
                (Cow::Owned(path), search)
            }
        }
    }

    fn normalized_path_search(&self, path: &Path) -> Result<usize, usize> {
        self.ordered_paths
            .binary_search_by(|candidate| candidate.as_path().cmp(path))
    }

    fn next_in_paths(
        &self,
        path_indices: impl Iterator<Item = usize>,
        anchor_path: &Path,
        line: usize,
        column: usize,
    ) -> Option<&Diagnostic> {
        for index in path_indices {
            let path = self.ordered_paths.get(index)?;
            let diagnostics = self.by_path.get(path)?;
            if path.as_path() == anchor_path {
                if let Some(diagnostic) = first_diagnostic_after(diagnostics, line, column) {
                    return Some(diagnostic);
                }
            } else if let Some(diagnostic) = diagnostics.first() {
                return Some(diagnostic);
            }
        }
        None
    }

    fn previous_in_paths(
        &self,
        path_indices: impl Iterator<Item = usize>,
        anchor_path: &Path,
        line: usize,
        column: usize,
    ) -> Option<&Diagnostic> {
        for index in path_indices {
            let path = self.ordered_paths.get(index)?;
            let diagnostics = self.by_path.get(path)?;
            if path.as_path() == anchor_path {
                if let Some(diagnostic) = last_diagnostic_before(diagnostics, line, column) {
                    return Some(diagnostic);
                }
            } else if let Some(diagnostic) = diagnostics.last() {
                return Some(diagnostic);
            }
        }
        None
    }

    pub fn len(&self) -> usize {
        self.counts_by_severity.iter().sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn count_by_severity(&self, severity: DiagnosticSeverity) -> usize {
        self.counts_by_severity[severity_index(severity)]
    }

    pub fn severity_counts(&self) -> DiagnosticSeverityCounts {
        DiagnosticSeverityCounts {
            errors: self.counts_by_severity[severity_index(DiagnosticSeverity::Error)],
            warnings: self.counts_by_severity[severity_index(DiagnosticSeverity::Warning)],
            infos: self.counts_by_severity[severity_index(DiagnosticSeverity::Info)],
            hints: self.counts_by_severity[severity_index(DiagnosticSeverity::Hint)],
        }
    }
}

struct DiagnosticSortedRange<'a> {
    diagnostics: &'a DiagnosticSet,
    path_index: usize,
    diagnostic_index: usize,
    next_index: usize,
    end_index: usize,
}

impl<'a> Iterator for DiagnosticSortedRange<'a> {
    type Item = (usize, &'a Diagnostic);

    fn next(&mut self) -> Option<Self::Item> {
        while self.next_index < self.end_index {
            let path = self.diagnostics.ordered_paths.get(self.path_index)?;
            let Some(path_diagnostics) = self.diagnostics.by_path.get(path) else {
                self.path_index += 1;
                self.diagnostic_index = 0;
                continue;
            };
            let Some(diagnostic) = path_diagnostics.get(self.diagnostic_index) else {
                self.path_index += 1;
                self.diagnostic_index = 0;
                continue;
            };

            let index = self.next_index;
            self.next_index += 1;
            self.diagnostic_index += 1;
            if self.diagnostic_index >= path_diagnostics.len() {
                self.path_index += 1;
                self.diagnostic_index = 0;
            }
            return Some((index, diagnostic));
        }
        None
    }
}

fn diagnostic_is_static(diagnostic: &Diagnostic) -> bool {
    diagnostic.source == STATIC_DIAGNOSTIC_SOURCE
}

fn reserve_lsp_diagnostic_sources(diagnostics: &mut [Diagnostic]) {
    let mut reserved_static_source = None;
    for diagnostic in diagnostics {
        if diagnostic.source == STATIC_DIAGNOSTIC_SOURCE {
            diagnostic.source = reserved_static_source
                .get_or_insert_with(|| {
                    format!("{RESERVED_LSP_DIAGNOSTIC_SOURCE_PREFIX}{STATIC_DIAGNOSTIC_SOURCE}")
                })
                .clone();
        }
    }
}

pub fn severity_rank(severity: DiagnosticSeverity) -> u8 {
    severity_index(severity) as u8
}

fn severity_index(severity: DiagnosticSeverity) -> usize {
    match severity {
        DiagnosticSeverity::Error => 0,
        DiagnosticSeverity::Warning => 1,
        DiagnosticSeverity::Info => 2,
        DiagnosticSeverity::Hint => 3,
    }
}

fn sort_and_dedup_diagnostics(diagnostics: &mut Vec<Diagnostic>) {
    if diagnostics.len() < 2 {
        return;
    }

    diagnostics.sort_by(compare_diagnostics_in_path);
    diagnostics.dedup_by(|a, b| compare_diagnostics_in_path(a, b) == std::cmp::Ordering::Equal);
}

fn limit_diagnostics_for_path(diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.truncate(DIAGNOSTIC_SET_MAX_PATH_RESULTS);
}

fn normalize_diagnostics_for_path(path: &Path, diagnostics: &mut [Diagnostic]) {
    let mut target_path = None;
    for diagnostic in diagnostics {
        if diagnostic.path.as_path() != path {
            let target_path = target_path.get_or_insert_with(|| path.to_path_buf());
            diagnostic.path.clone_from(target_path);
        }
        diagnostic.line = diagnostic.line.max(1);
        diagnostic.column = diagnostic.column.max(1);
        if diagnostic.char_range.end < diagnostic.char_range.start {
            diagnostic.char_range.end = diagnostic.char_range.start;
        }
    }
}

#[cfg(test)]
fn normalize_diagnostic_path(path: &Path) -> PathBuf {
    normalize_diagnostic_path_cow(path).into_owned()
}

fn normalize_diagnostic_path_cow(path: &Path) -> Cow<'_, Path> {
    lexical_normalize_cow(path)
}

fn normalize_diagnostic_path_owned(path: PathBuf) -> PathBuf {
    match normalize_diagnostic_path_cow(&path) {
        Cow::Borrowed(_) => path,
        Cow::Owned(path) => path,
    }
}

fn first_diagnostic_after(
    diagnostics: &[Diagnostic],
    line: usize,
    column: usize,
) -> Option<&Diagnostic> {
    diagnostics.get(
        diagnostics
            .partition_point(|diagnostic| (diagnostic.line, diagnostic.column) <= (line, column)),
    )
}

fn last_diagnostic_before(
    diagnostics: &[Diagnostic],
    line: usize,
    column: usize,
) -> Option<&Diagnostic> {
    diagnostics
        .partition_point(|diagnostic| (diagnostic.line, diagnostic.column) < (line, column))
        .checked_sub(1)
        .and_then(|index| diagnostics.get(index))
}

fn diagnostic_line_range(diagnostics: &[Diagnostic], line: usize) -> Range<usize> {
    let start = diagnostics.partition_point(|diagnostic| diagnostic.line < line);
    let end = diagnostics.partition_point(|diagnostic| diagnostic.line <= line);
    start..end
}

fn compare_diagnostics_in_path(a: &Diagnostic, b: &Diagnostic) -> std::cmp::Ordering {
    a.line
        .cmp(&b.line)
        .then(a.column.cmp(&b.column))
        .then(a.char_range.start.cmp(&b.char_range.start))
        .then(a.char_range.end.cmp(&b.char_range.end))
        .then(severity_rank(a.severity).cmp(&severity_rank(b.severity)))
        .then(a.source.cmp(&b.source))
        .then(a.message.cmp(&b.message))
        .then(a.unused.cmp(&b.unused))
        .then(a.deprecated.cmp(&b.deprecated))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostic(path: &Path, severity: DiagnosticSeverity, source: &str) -> Diagnostic {
        Diagnostic {
            path: path.to_path_buf(),
            line: 1,
            column: 1,
            char_range: 0..1,
            severity,
            source: source.to_owned(),
            message: format!("{source} {severity:?}"),
            unused: false,
            deprecated: false,
        }
    }

    #[test]
    fn static_analysis_reports_common_code_markers() {
        let diagnostics = analyze_text(
            PathBuf::from("main.rs"),
            "fn main() { // TODO\nlet x = 1; \n]\n",
        );
        assert!(diagnostics.iter().any(|d| d.message == "TODO marker"));
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message == "Trailing whitespace")
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message == "Mismatched closing bracket")
        );
    }

    #[test]
    fn static_analysis_keeps_common_marker_source_order_and_ranges() {
        let diagnostics = analyze_text(
            PathBuf::from("main.rs"),
            "FIXME before TODO\nTODO then FIXME\n",
        );

        let markers = diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.message == "TODO marker" || diagnostic.message == "FIXME marker"
            })
            .map(|diagnostic| {
                (
                    diagnostic.message.as_str(),
                    diagnostic.line,
                    diagnostic.column,
                    diagnostic.char_range.clone(),
                    diagnostic.severity,
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            markers,
            vec![
                ("FIXME marker", 1, 1, 0..5, DiagnosticSeverity::Warning),
                ("TODO marker", 1, 14, 13..17, DiagnosticSeverity::Info),
                ("TODO marker", 2, 1, 18..22, DiagnosticSeverity::Info),
                ("FIXME marker", 2, 11, 28..33, DiagnosticSeverity::Warning),
            ]
        );
    }

    #[test]
    fn common_marker_scan_reports_first_marker_of_each_kind_in_source_order() {
        let markers = scan_common_markers("TODO one TODO two FIXME one FIXME two")
            .into_iter()
            .flatten()
            .map(|marker| (marker.marker.text(), marker.byte_index, marker.column))
            .collect::<Vec<_>>();

        assert_eq!(markers, vec![("TODO", 0, 1), ("FIXME", 18, 19)]);

        let markers = scan_common_markers("FIXME before TODO then FIXME then TODO")
            .into_iter()
            .flatten()
            .map(|marker| (marker.marker.text(), marker.byte_index, marker.column))
            .collect::<Vec<_>>();

        assert_eq!(markers, vec![("FIXME", 0, 1), ("TODO", 13, 14)]);
    }

    #[test]
    fn common_marker_scan_keeps_columns_after_multibyte_prefixes() {
        let markers = scan_common_markers("\u{03bb}\u{1f642}TODO then FIXME")
            .into_iter()
            .flatten()
            .map(|marker| (marker.marker.text(), marker.byte_index, marker.column))
            .collect::<Vec<_>>();

        assert_eq!(markers, vec![("TODO", 6, 3), ("FIXME", 16, 13)]);
    }

    #[test]
    fn static_analysis_bounds_reported_diagnostics() {
        let text = "TODO\n".repeat(STATIC_DIAGNOSTIC_MAX_RESULTS + 32);

        let diagnostics = analyze_text(PathBuf::from("main.rs"), &text);

        assert_eq!(diagnostics.len(), STATIC_DIAGNOSTIC_MAX_RESULTS);
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.message == "TODO marker")
        );
    }

    #[test]
    fn static_analysis_caps_deep_bracket_inputs() {
        let text = "(".repeat(STATIC_DIAGNOSTIC_MAX_BRACKET_DEPTH + 512);

        let diagnostics = analyze_text(PathBuf::from("main.rs"), &text);

        assert_eq!(diagnostics.len(), STATIC_DIAGNOSTIC_MAX_RESULTS);
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.message == "Unclosed bracket")
        );
    }

    #[test]
    fn static_analysis_skips_text_past_scan_byte_limit() {
        let text = format!(
            "{}\nTODO\n",
            "a".repeat(STATIC_DIAGNOSTIC_SCAN_MAX_BYTES + 1)
        );

        let diagnostics = analyze_text(PathBuf::from("main.rs"), &text);

        assert!(diagnostics.is_empty());
        assert!(static_diagnostics_scan_allowed(
            STATIC_DIAGNOSTIC_SCAN_MAX_BYTES
        ));
        assert!(!static_diagnostics_scan_allowed(
            STATIC_DIAGNOSTIC_SCAN_MAX_BYTES + 1
        ));
    }

    #[test]
    fn diagnostic_set_maintains_severity_counts_across_replacements() {
        let path = PathBuf::from("src/main.rs");
        let mut diagnostics = DiagnosticSet::default();

        diagnostics.replace(
            path.clone(),
            vec![
                diagnostic(&path, DiagnosticSeverity::Error, "kuroya-static"),
                diagnostic(&path, DiagnosticSeverity::Warning, "rust-analyzer"),
                diagnostic(&path, DiagnosticSeverity::Info, "rust-analyzer"),
            ],
        );
        assert_eq!(diagnostics.count_by_severity(DiagnosticSeverity::Error), 1);
        assert_eq!(
            diagnostics.count_by_severity(DiagnosticSeverity::Warning),
            1
        );
        assert_eq!(diagnostics.count_by_severity(DiagnosticSeverity::Info), 1);
        assert_eq!(
            diagnostics.severity_counts(),
            DiagnosticSeverityCounts {
                errors: 1,
                warnings: 1,
                infos: 1,
                hints: 0,
            }
        );

        diagnostics.replace_lsp(
            path.clone(),
            vec![diagnostic(&path, DiagnosticSeverity::Hint, "rust-analyzer")],
        );
        assert_eq!(diagnostics.count_by_severity(DiagnosticSeverity::Error), 1);
        assert_eq!(
            diagnostics.count_by_severity(DiagnosticSeverity::Warning),
            0
        );
        assert_eq!(diagnostics.count_by_severity(DiagnosticSeverity::Info), 0);
        assert_eq!(diagnostics.count_by_severity(DiagnosticSeverity::Hint), 1);
        assert_eq!(
            diagnostics.severity_counts(),
            DiagnosticSeverityCounts {
                errors: 1,
                warnings: 0,
                infos: 0,
                hints: 1,
            }
        );

        diagnostics.replace_static(path.clone(), Vec::new());
        assert_eq!(diagnostics.count_by_severity(DiagnosticSeverity::Error), 0);
        assert_eq!(diagnostics.count_by_severity(DiagnosticSeverity::Hint), 1);
        assert_eq!(
            diagnostics.severity_counts(),
            DiagnosticSeverityCounts {
                errors: 0,
                warnings: 0,
                infos: 0,
                hints: 1,
            }
        );

        diagnostics.replace(path, Vec::new());
        assert!(diagnostics.is_empty());
        assert_eq!(diagnostics.count_by_severity(DiagnosticSeverity::Hint), 0);
        assert_eq!(
            diagnostics.severity_counts(),
            DiagnosticSeverityCounts::default()
        );
    }

    #[test]
    fn lsp_diagnostics_cannot_claim_static_internal_source() {
        let path = PathBuf::from("src/main.rs");
        let mut diagnostics = DiagnosticSet::default();

        diagnostics.replace_static(
            path.clone(),
            vec![diagnostic(
                &path,
                DiagnosticSeverity::Error,
                STATIC_DIAGNOSTIC_SOURCE,
            )],
        );
        diagnostics.replace_lsp(
            path.clone(),
            vec![diagnostic(
                &path,
                DiagnosticSeverity::Warning,
                STATIC_DIAGNOSTIC_SOURCE,
            )],
        );

        let sources = diagnostics
            .for_path(&path)
            .iter()
            .map(|diagnostic| diagnostic.source.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            sources,
            vec![STATIC_DIAGNOSTIC_SOURCE, "lsp: kuroya-static"]
        );

        diagnostics.replace_lsp(path.clone(), Vec::new());
        let remaining = diagnostics.for_path(&path);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].source, STATIC_DIAGNOSTIC_SOURCE);
    }

    #[test]
    fn lsp_units_track_raw_utf16_buckets_until_converted() {
        let path = PathBuf::from("src/main.rs");
        let noisy = PathBuf::from("src/../src/main.rs");
        let mut diagnostics = DiagnosticSet::default();

        diagnostics.replace_lsp_units(
            noisy.clone(),
            vec![diagnostic(
                &noisy,
                DiagnosticSeverity::Warning,
                "rust-analyzer",
            )],
            LspDiagnosticUnits::Utf16,
        );
        assert_eq!(diagnostics.for_path(&path).len(), 1);
        assert_eq!(diagnostics.raw_utf16_lsp_paths(), vec![path.clone()]);

        diagnostics.replace_lsp_source_units(
            path.clone(),
            "rust\u{0}2",
            vec![diagnostic(
                &path,
                DiagnosticSeverity::Error,
                "rust-analyzer",
            )],
            LspDiagnosticUnits::Utf16,
        );
        assert_eq!(diagnostics.for_path(&path).len(), 2);

        let mut drained = diagnostics.drain_raw_utf16_lsp_buckets(&path);
        drained.sort_by(|left, right| left.0.cmp(&right.0));
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].0, DEFAULT_LSP_DIAGNOSTIC_SOURCE_KEY);
        assert_eq!(drained[1].0, "rust\u{0}2");
        assert!(diagnostics.raw_utf16_lsp_paths().is_empty());

        for (source_key, mut bucket) in drained {
            for diagnostic in &mut bucket {
                diagnostic.column = 1;
                diagnostic.char_range = 0..1;
            }
            diagnostics.replace_lsp_source_units(
                path.clone(),
                &source_key,
                bucket,
                LspDiagnosticUnits::Chars,
            );
        }
        assert_eq!(diagnostics.for_path(&path).len(), 2);
        assert!(diagnostics.raw_utf16_lsp_paths().is_empty());

        diagnostics.replace_lsp_units(
            path.clone(),
            vec![diagnostic(&path, DiagnosticSeverity::Hint, "rust-analyzer")],
            LspDiagnosticUnits::Utf16,
        );
        assert_eq!(diagnostics.raw_utf16_lsp_paths(), vec![path.clone()]);
        diagnostics.replace_lsp(path.clone(), Vec::new());
        assert!(diagnostics.raw_utf16_lsp_paths().is_empty());
    }

    #[test]
    fn lsp_units_markers_follow_full_replacement_and_purges() {
        let path = PathBuf::from("src/main.rs");
        let mut diagnostics = DiagnosticSet::default();
        let source_key = "rust\u{0}1";

        diagnostics.replace_lsp_source_units(
            path.clone(),
            source_key,
            vec![diagnostic(
                &path,
                DiagnosticSeverity::Error,
                "rust-analyzer",
            )],
            LspDiagnosticUnits::Utf16,
        );
        assert_eq!(diagnostics.raw_utf16_lsp_paths(), vec![path.clone()]);

        diagnostics.replace(
            path.clone(),
            vec![diagnostic(
                &path,
                DiagnosticSeverity::Warning,
                "kuroya-static",
            )],
        );
        assert!(diagnostics.raw_utf16_lsp_paths().is_empty());

        diagnostics.replace_lsp_source_units(
            path.clone(),
            source_key,
            vec![diagnostic(
                &path,
                DiagnosticSeverity::Error,
                "rust-analyzer",
            )],
            LspDiagnosticUnits::Utf16,
        );
        assert_eq!(diagnostics.raw_utf16_lsp_paths(), vec![path.clone()]);
        diagnostics.purge_lsp_source(source_key);
        assert!(diagnostics.raw_utf16_lsp_paths().is_empty());

        diagnostics.replace_lsp_units(path.clone(), Vec::new(), LspDiagnosticUnits::Utf16);
        assert!(diagnostics.raw_utf16_lsp_paths().is_empty());
    }

    #[test]
    fn lsp_source_buckets_keep_sibling_servers_on_shared_path() {
        let path = PathBuf::from("src/main.rs");
        let mut diagnostics = DiagnosticSet::default();
        let rust_key = "rust\u{0}10";
        let python_key = "python\u{0}11";

        diagnostics.replace_lsp_source(
            path.clone(),
            rust_key,
            vec![diagnostic(
                &path,
                DiagnosticSeverity::Error,
                "rust-analyzer",
            )],
        );
        diagnostics.replace_lsp_source(
            path.clone(),
            python_key,
            vec![diagnostic(&path, DiagnosticSeverity::Warning, "pyright")],
        );

        assert_eq!(diagnostics.for_path(&path).len(), 2);
        assert_eq!(diagnostics.count_by_severity(DiagnosticSeverity::Error), 1);
        assert_eq!(
            diagnostics.count_by_severity(DiagnosticSeverity::Warning),
            1
        );

        let mut updated = diagnostic(&path, DiagnosticSeverity::Error, "rust-analyzer");
        updated.line = 3;
        diagnostics.replace_lsp_source(path.clone(), rust_key, vec![updated]);
        assert_eq!(diagnostics.for_path(&path).len(), 2);
        assert_eq!(diagnostics.count_by_severity(DiagnosticSeverity::Error), 1);

        diagnostics.replace_lsp_source(path.clone(), rust_key, Vec::new());
        let remaining = diagnostics.for_path(&path);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].source, "pyright");
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn purge_lsp_source_clears_stopped_server_buckets_across_paths() {
        let a = PathBuf::from("src/a.rs");
        let b = PathBuf::from("src/b.rs");
        let mut diagnostics = DiagnosticSet::default();
        let stopped_key = "rust\u{0}10";
        let sibling_key = "rust\u{0}11";

        for path in [&a, &b] {
            diagnostics.replace_static(
                path.to_path_buf(),
                vec![diagnostic(
                    path,
                    DiagnosticSeverity::Info,
                    STATIC_DIAGNOSTIC_SOURCE,
                )],
            );
            diagnostics.replace_lsp_source(
                path.to_path_buf(),
                stopped_key,
                vec![diagnostic(path, DiagnosticSeverity::Error, "rust-analyzer")],
            );
            diagnostics.replace_lsp_source(
                path.to_path_buf(),
                sibling_key,
                vec![diagnostic(
                    path,
                    DiagnosticSeverity::Warning,
                    "rust-analyzer-obsidian",
                )],
            );
        }
        assert_eq!(diagnostics.len(), 6);

        diagnostics.purge_lsp_source(stopped_key);

        assert_eq!(diagnostics.len(), 4);
        for path in [&a, &b] {
            let remaining = diagnostics.for_path(path);
            assert_eq!(remaining.len(), 2);
            assert!(remaining.iter().all(|d| d.source != "rust-analyzer"));
        }
        assert_eq!(diagnostics.count_by_severity(DiagnosticSeverity::Error), 0);
        assert_eq!(
            diagnostics.count_by_severity(DiagnosticSeverity::Warning),
            2
        );
        assert_eq!(diagnostics.count_by_severity(DiagnosticSeverity::Info), 2);

        diagnostics.purge_lsp_source("missing\u{0}99");
        assert_eq!(diagnostics.len(), 4);
    }

    #[test]
    fn diagnostic_set_all_sorted_is_stable_across_paths_and_severity() {
        let mut diagnostics = DiagnosticSet::default();
        let a = PathBuf::from("src/a.rs");
        let b = PathBuf::from("src/b.rs");

        let mut warning = diagnostic(&a, DiagnosticSeverity::Warning, "rust-analyzer");
        warning.line = 2;
        warning.column = 4;
        warning.message = "later warning".to_owned();
        let mut error = diagnostic(&a, DiagnosticSeverity::Error, "rust-analyzer");
        error.line = 2;
        error.column = 4;
        error.message = "earlier error".to_owned();
        let mut other_path = diagnostic(&b, DiagnosticSeverity::Hint, "rust-analyzer");
        other_path.line = 1;
        other_path.column = 1;

        diagnostics.replace(b.clone(), vec![other_path]);
        diagnostics.replace(a.clone(), vec![warning, error]);

        let ordered = diagnostics
            .all_sorted()
            .into_iter()
            .map(|diagnostic| {
                (
                    diagnostic.path.clone(),
                    diagnostic.line,
                    diagnostic.column,
                    diagnostic.severity,
                    diagnostic.message.clone(),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            ordered,
            vec![
                (
                    a.clone(),
                    2,
                    4,
                    DiagnosticSeverity::Error,
                    "earlier error".to_owned(),
                ),
                (
                    a,
                    2,
                    4,
                    DiagnosticSeverity::Warning,
                    "later warning".to_owned(),
                ),
                (
                    b,
                    1,
                    1,
                    DiagnosticSeverity::Hint,
                    "rust-analyzer Hint".to_owned(),
                ),
            ]
        );
    }

    #[test]
    fn diagnostic_set_sorted_order_uses_full_tie_breakers_and_preserves_payloads() {
        let mut diagnostics = DiagnosticSet::default();
        let noisy = PathBuf::from("workspace/src/../src/main.rs");
        let normalized = PathBuf::from("workspace/src/main.rs");
        let stale_path = PathBuf::from("workspace/src/stale.rs");

        let mut later_range_error = diagnostic(&stale_path, DiagnosticSeverity::Error, "z-source");
        later_range_error.line = 3;
        later_range_error.column = 5;
        later_range_error.char_range = 12..18;
        later_range_error.message = "later range error".to_owned();
        later_range_error.unused = true;

        let mut earlier_range_warning =
            diagnostic(&stale_path, DiagnosticSeverity::Warning, "z-source");
        earlier_range_warning.line = 3;
        earlier_range_warning.column = 5;
        earlier_range_warning.char_range = 10..11;
        earlier_range_warning.message = "earlier range warning".to_owned();

        let mut same_range_source_a =
            diagnostic(&stale_path, DiagnosticSeverity::Info, "alpha-source");
        same_range_source_a.line = 3;
        same_range_source_a.column = 5;
        same_range_source_a.char_range = 20..21;
        same_range_source_a.message = "z message".to_owned();

        let mut same_range_source_b =
            diagnostic(&stale_path, DiagnosticSeverity::Info, "beta-source");
        same_range_source_b.line = 3;
        same_range_source_b.column = 5;
        same_range_source_b.char_range = 20..21;
        same_range_source_b.message = "a message".to_owned();
        same_range_source_b.deprecated = true;

        diagnostics.replace(
            noisy,
            vec![
                later_range_error,
                same_range_source_b,
                same_range_source_a,
                earlier_range_warning,
            ],
        );

        let ordered = diagnostics
            .all_sorted()
            .into_iter()
            .map(|diagnostic| {
                (
                    diagnostic.path.clone(),
                    diagnostic.char_range.clone(),
                    diagnostic.severity,
                    diagnostic.source.as_str(),
                    diagnostic.message.as_str(),
                    diagnostic.unused,
                    diagnostic.deprecated,
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            ordered,
            vec![
                (
                    normalized.clone(),
                    10..11,
                    DiagnosticSeverity::Warning,
                    "z-source",
                    "earlier range warning",
                    false,
                    false,
                ),
                (
                    normalized.clone(),
                    12..18,
                    DiagnosticSeverity::Error,
                    "z-source",
                    "later range error",
                    true,
                    false,
                ),
                (
                    normalized.clone(),
                    20..21,
                    DiagnosticSeverity::Info,
                    "alpha-source",
                    "z message",
                    false,
                    false,
                ),
                (
                    normalized,
                    20..21,
                    DiagnosticSeverity::Info,
                    "beta-source",
                    "a message",
                    false,
                    true,
                ),
            ]
        );
    }

    #[test]
    fn diagnostic_set_deduplicates_after_sorting_and_normalization() {
        let mut diagnostics = DiagnosticSet::default();
        let noisy = PathBuf::from("workspace/src/../src/main.rs");
        let normalized = PathBuf::from("workspace/src/main.rs");

        let mut duplicate = diagnostic(&noisy, DiagnosticSeverity::Error, "rust-analyzer");
        duplicate.line = 3;
        duplicate.column = 5;
        duplicate.char_range = 10..14;
        duplicate.message = "same diagnostic".to_owned();
        let mut same_after_normalization = duplicate.clone();
        same_after_normalization.path = normalized.clone();
        let mut distinct = duplicate.clone();
        distinct.message = "same location, different message".to_owned();

        diagnostics.replace_lsp(noisy, vec![distinct, duplicate, same_after_normalization]);

        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics.count_by_severity(DiagnosticSeverity::Error), 2);
        assert_eq!(
            diagnostics
                .all_sorted()
                .into_iter()
                .map(|diagnostic| (diagnostic.path.clone(), diagnostic.message.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (normalized.clone(), "same diagnostic"),
                (normalized, "same location, different message"),
            ]
        );
    }

    #[test]
    fn diagnostic_set_full_replacement_deduplicates_and_caps_path_payloads() {
        let path = PathBuf::from("src/main.rs");
        let mut diagnostics = DiagnosticSet::default();
        let mut payload = (0..DIAGNOSTIC_SET_MAX_PATH_RESULTS + 64)
            .map(|index| {
                let mut diagnostic = diagnostic(&path, DiagnosticSeverity::Warning, "rust");
                diagnostic.line = index + 1;
                diagnostic.message = format!("diagnostic {index}");
                diagnostic
            })
            .collect::<Vec<_>>();
        payload.push(payload[0].clone());

        diagnostics.replace(path.clone(), payload);

        assert_eq!(diagnostics.len(), DIAGNOSTIC_SET_MAX_PATH_RESULTS);
        assert_eq!(
            diagnostics
                .first()
                .map(|diagnostic| diagnostic.message.as_str()),
            Some("diagnostic 0")
        );
        let expected_last = format!("diagnostic {}", DIAGNOSTIC_SET_MAX_PATH_RESULTS - 1);
        assert_eq!(
            diagnostics
                .last()
                .map(|diagnostic| diagnostic.message.as_str()),
            Some(expected_last.as_str())
        );
        assert_eq!(
            diagnostics.for_path(&path).len(),
            DIAGNOSTIC_SET_MAX_PATH_RESULTS
        );
    }

    #[test]
    fn diagnostic_set_full_replacement_deduplicates_identical_payloads() {
        let path = PathBuf::from("src/main.rs");
        let noisy = PathBuf::from("src/../src/main.rs");
        let mut diagnostics = DiagnosticSet::default();
        let mut duplicate = diagnostic(&noisy, DiagnosticSeverity::Error, "rust-analyzer");
        duplicate.line = 7;
        duplicate.column = 3;
        duplicate.message = "same".to_owned();
        let mut same_after_normalization = duplicate.clone();
        same_after_normalization.path = path.clone();

        diagnostics.replace(path.clone(), vec![duplicate, same_after_normalization]);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics
                .first()
                .map(|diagnostic| diagnostic.path.as_path()),
            Some(path.as_path())
        );
    }

    #[test]
    fn diagnostic_display_text_cow_borrows_clean_messages() {
        let clean = "rust-analyzer expected item";
        assert!(matches!(
            sanitize_diagnostic_display_text_cow(clean, DIAGNOSTIC_DISPLAY_TEXT_MAX_CHARS),
            Cow::Borrowed(text) if text == clean
        ));
        assert_eq!(diagnostic_display_text(clean), clean);

        let exact_limit = "x".repeat(16);
        assert!(matches!(
            sanitize_diagnostic_display_text_cow(&exact_limit, exact_limit.chars().count()),
            Cow::Borrowed(text) if text == exact_limit.as_str()
        ));
    }

    #[test]
    fn diagnostic_display_text_cow_owns_normalized_messages() {
        let cases = [
            (" padded ", "padded"),
            ("Bad\u{0007}Message", "Bad Message"),
            ("Bad\tMessage\nNext", "Bad Message Next"),
            ("Bad  Message", "Bad Message"),
            ("\u{202e}", ""),
            ("", ""),
        ];

        for (input, expected) in cases {
            match sanitize_diagnostic_display_text_cow(input, DIAGNOSTIC_DISPLAY_TEXT_MAX_CHARS) {
                Cow::Owned(text) => assert_eq!(text, expected, "input {input:?}"),
                Cow::Borrowed(text) => panic!("expected owned diagnostic display text: {text:?}"),
            }
            assert_eq!(diagnostic_display_text(input), expected, "input {input:?}");
        }

        match sanitize_diagnostic_display_text_cow("clean", 0) {
            Cow::Owned(text) => assert_eq!(text, ""),
            Cow::Borrowed(text) => panic!("expected zero max to own empty text: {text:?}"),
        }

        let max_chars = DIAGNOSTIC_DISPLAY_TEXT_MAX_CHARS;
        let truncated_input = format!("{}z", "x".repeat(max_chars));
        let expected_truncated = format!(
            "{}{}",
            "x".repeat(max_chars - DIAGNOSTIC_DISPLAY_TRUNCATION_MARKER.chars().count()),
            DIAGNOSTIC_DISPLAY_TRUNCATION_MARKER
        );

        match sanitize_diagnostic_display_text_cow(&truncated_input, max_chars) {
            Cow::Owned(text) => assert_eq!(text, expected_truncated),
            Cow::Borrowed(text) => {
                panic!("expected truncated diagnostic display text to be owned: {text:?}")
            }
        }
        assert_eq!(
            diagnostic_display_text(&truncated_input),
            expected_truncated
        );
    }

    #[test]
    fn diagnostic_display_text_sanitizes_without_mutating_raw_payloads() {
        let path = PathBuf::from("src/main.rs");
        let raw_message = format!(
            " Bad\n\u{202e}Message {}",
            "x".repeat(DIAGNOSTIC_DISPLAY_TEXT_MAX_CHARS)
        );
        let mut raw = diagnostic(&path, DiagnosticSeverity::Error, "rust\u{202e}");
        raw.message = raw_message.clone();
        let mut diagnostics = DiagnosticSet::default();

        diagnostics.replace_lsp(path.clone(), vec![raw]);

        let stored = diagnostics.first().expect("diagnostic should be stored");
        assert_eq!(stored.message, raw_message);
        assert_eq!(stored.source, "rust\u{202e}");

        let display = diagnostic_display_text(&stored.message);
        assert!(display.starts_with("Bad Message "));
        assert!(display.ends_with(DIAGNOSTIC_DISPLAY_TRUNCATION_MARKER));
        assert!(display.chars().count() <= DIAGNOSTIC_DISPLAY_TEXT_MAX_CHARS);
        assert!(!display.chars().any(char::is_control));
        assert!(!display.chars().any(is_diagnostic_display_format_control));
    }

    #[test]
    fn diagnostic_set_sorted_order_tracks_path_removal_and_reinsert() {
        let mut diagnostics = DiagnosticSet::default();
        let a = PathBuf::from("src/a.rs");
        let b = PathBuf::from("src/b.rs");

        diagnostics.replace(
            b.clone(),
            vec![diagnostic(&b, DiagnosticSeverity::Hint, "rust-analyzer")],
        );
        diagnostics.replace(
            a.clone(),
            vec![diagnostic(&a, DiagnosticSeverity::Error, "rust-analyzer")],
        );

        assert_eq!(diagnostics.len(), 2);
        assert_eq!(
            diagnostics
                .all_sorted()
                .into_iter()
                .map(|diagnostic| diagnostic.path.clone())
                .collect::<Vec<_>>(),
            vec![a.clone(), b.clone()]
        );

        diagnostics.replace(a.clone(), Vec::new());
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics
                .all_sorted()
                .into_iter()
                .map(|diagnostic| diagnostic.path.clone())
                .collect::<Vec<_>>(),
            vec![b.clone()]
        );

        diagnostics.replace_lsp(
            a.clone(),
            vec![diagnostic(&a, DiagnosticSeverity::Warning, "rust-analyzer")],
        );
        assert_eq!(
            diagnostics
                .all_sorted()
                .into_iter()
                .map(|diagnostic| diagnostic.path.clone())
                .collect::<Vec<_>>(),
            vec![a, b]
        );
    }

    #[test]
    fn diagnostic_set_source_replacement_preserves_retained_payload_and_counts() {
        let noisy = PathBuf::from("workspace/src/../src/main.rs");
        let normalized = PathBuf::from("workspace/src/main.rs");
        let mut diagnostics = DiagnosticSet::default();

        let mut static_error = diagnostic(&noisy, DiagnosticSeverity::Error, "kuroya-static");
        static_error.line = 10;
        static_error.message = "static payload stays".to_owned();
        let mut lsp_warning = diagnostic(&noisy, DiagnosticSeverity::Warning, "rust-analyzer");
        lsp_warning.line = 2;
        lsp_warning.message = "old warning".to_owned();
        let mut lsp_info = diagnostic(&noisy, DiagnosticSeverity::Info, "rust-analyzer");
        lsp_info.line = 4;
        lsp_info.message = "old info".to_owned();

        diagnostics.replace(noisy.clone(), vec![static_error, lsp_warning, lsp_info]);

        let mut new_hint = diagnostic(&normalized, DiagnosticSeverity::Hint, "rust-analyzer");
        new_hint.line = 1;
        new_hint.message = "new hint".to_owned();
        diagnostics.replace_lsp(normalized.clone(), vec![new_hint]);

        assert_eq!(diagnostics.count_by_severity(DiagnosticSeverity::Error), 1);
        assert_eq!(
            diagnostics.count_by_severity(DiagnosticSeverity::Warning),
            0
        );
        assert_eq!(diagnostics.count_by_severity(DiagnosticSeverity::Info), 0);
        assert_eq!(diagnostics.count_by_severity(DiagnosticSeverity::Hint), 1);
        assert_eq!(
            diagnostics
                .all_sorted()
                .into_iter()
                .map(|diagnostic| (diagnostic.path.clone(), diagnostic.message.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (normalized.clone(), "new hint"),
                (normalized.clone(), "static payload stays"),
            ]
        );

        diagnostics.replace_lsp(noisy, Vec::new());

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics.count_by_severity(DiagnosticSeverity::Error), 1);
        assert_eq!(diagnostics.count_by_severity(DiagnosticSeverity::Hint), 0);
        assert_eq!(
            diagnostics
                .first()
                .map(|diagnostic| (diagnostic.path.clone(), diagnostic.message.as_str())),
            Some((normalized, "static payload stays"))
        );
    }

    #[test]
    fn diagnostic_set_path_removal_clears_duplicate_order_entries() {
        let path = PathBuf::from("src/main.rs");
        let mut diagnostics = DiagnosticSet::default();

        diagnostics.replace(
            path.clone(),
            vec![diagnostic(
                &path,
                DiagnosticSeverity::Error,
                "rust-analyzer",
            )],
        );
        diagnostics.ordered_paths.insert(0, path.clone());

        diagnostics.replace(path, Vec::new());

        assert!(diagnostics.ordered_paths.is_empty());
        assert!(diagnostics.all_sorted().is_empty());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn diagnostic_set_normalizes_replacement_payloads_for_target_path() {
        let target = PathBuf::from("src/main.rs");
        let mut diagnostics = DiagnosticSet::default();
        let mut malformed = diagnostic(
            Path::new("src/stale.rs"),
            DiagnosticSeverity::Error,
            "rust-analyzer",
        );
        malformed.line = 0;
        malformed.column = 0;
        malformed.char_range = std::ops::Range { start: 9, end: 3 };

        diagnostics.replace(target.clone(), vec![malformed]);

        let diagnostic = diagnostics.first().expect("diagnostic should be stored");
        assert_eq!(diagnostic.path, target);
        assert_eq!(diagnostic.line, 1);
        assert_eq!(diagnostic.column, 1);
        assert_eq!(diagnostic.char_range, 9..9);
    }

    #[test]
    fn diagnostic_set_normalizes_paths_lexically_for_lookup_and_merge() {
        let noisy = PathBuf::from("workspace/src/../src/main.rs");
        let normalized = PathBuf::from("workspace/src/main.rs");
        let mut diagnostics = DiagnosticSet::default();

        diagnostics.replace_static(
            noisy.clone(),
            vec![diagnostic(
                &noisy,
                DiagnosticSeverity::Info,
                "kuroya-static",
            )],
        );
        diagnostics.replace_lsp(
            normalized.clone(),
            vec![diagnostic(
                &normalized,
                DiagnosticSeverity::Warning,
                "rust-analyzer",
            )],
        );

        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics.for_path(&noisy).len(), 2);
        assert_eq!(diagnostics.for_path(&normalized).len(), 2);
        assert!(
            diagnostics
                .all_sorted()
                .into_iter()
                .all(|diagnostic| diagnostic.path == normalized)
        );

        diagnostics.replace_static(noisy, Vec::new());
        assert_eq!(diagnostics.for_path(&normalized).len(), 1);
        assert_eq!(
            diagnostics
                .first()
                .map(|diagnostic| diagnostic.source.as_str()),
            Some("rust-analyzer")
        );
    }

    #[test]
    fn diagnostic_path_normalization_borrows_clean_lookup_paths() {
        let clean = Path::new("workspace/src/main.rs");
        let noisy = Path::new("workspace/src/../src/main.rs");

        assert!(
            matches!(normalize_diagnostic_path_cow(clean), Cow::Borrowed(path) if path == clean)
        );
        match normalize_diagnostic_path_cow(noisy) {
            Cow::Owned(path) => assert_eq!(path, PathBuf::from("workspace/src/main.rs")),
            Cow::Borrowed(path) => panic!("expected noisy path to normalize: {path:?}"),
        }
    }

    #[test]
    fn diagnostic_navigation_path_search_borrows_exact_tracked_paths() {
        let path = PathBuf::from("workspace/src/main.rs");
        let noisy = PathBuf::from("workspace/src/../src/main.rs");
        let mut diagnostics = DiagnosticSet::default();

        diagnostics.replace(
            path.clone(),
            vec![diagnostic(
                &path,
                DiagnosticSeverity::Warning,
                "rust-analyzer",
            )],
        );

        match diagnostics.navigation_path_search(&path) {
            (Cow::Borrowed(anchor), Ok(0)) => assert_eq!(anchor, path.as_path()),
            other => panic!("expected borrowed exact navigation path, got {other:?}"),
        }
        match diagnostics.navigation_path_search(&noisy) {
            (Cow::Owned(anchor), Ok(0)) => assert_eq!(anchor.as_path(), path.as_path()),
            other => panic!("expected normalized noisy navigation path, got {other:?}"),
        }
    }

    #[cfg(windows)]
    #[test]
    fn diagnostic_paths_preserve_windows_drive_relative_parent_segments() {
        let escaped = PathBuf::from(r"C:..\src\main.rs");
        let plain = PathBuf::from(r"C:src\main.rs");
        let mut diagnostics = DiagnosticSet::default();

        assert_eq!(normalize_diagnostic_path(&escaped), escaped);
        assert_ne!(
            normalize_diagnostic_path(&escaped),
            normalize_diagnostic_path(&plain)
        );

        diagnostics.replace(
            escaped.clone(),
            vec![diagnostic(
                &escaped,
                DiagnosticSeverity::Warning,
                "rust-analyzer",
            )],
        );

        assert_eq!(diagnostics.for_path(&escaped).len(), 1);
        assert!(diagnostics.for_path(&plain).is_empty());
    }

    #[test]
    fn diagnostic_set_get_sorted_indexes_without_collecting_all_items() {
        let mut diagnostics = DiagnosticSet::default();
        let a = PathBuf::from("src/a.rs");
        let b = PathBuf::from("src/b.rs");
        let mut a_second = diagnostic(&a, DiagnosticSeverity::Warning, "rust-analyzer");
        a_second.line = 4;
        a_second.message = "a second".to_owned();
        let mut a_first = diagnostic(&a, DiagnosticSeverity::Error, "rust-analyzer");
        a_first.line = 2;
        a_first.message = "a first".to_owned();
        let mut b_first = diagnostic(&b, DiagnosticSeverity::Hint, "rust-analyzer");
        b_first.line = 1;
        b_first.message = "b first".to_owned();

        diagnostics.replace(b.clone(), vec![b_first]);
        diagnostics.replace(a.clone(), vec![a_second, a_first]);

        assert_eq!(
            (0..4)
                .map(|index| {
                    diagnostics.get_sorted(index).map(|diagnostic| {
                        (
                            diagnostic.path.clone(),
                            diagnostic.line,
                            diagnostic.message.as_str(),
                        )
                    })
                })
                .collect::<Vec<_>>(),
            vec![
                Some((a.clone(), 2, "a first")),
                Some((a, 4, "a second")),
                Some((b, 1, "b first")),
                None,
            ]
        );
    }

    #[test]
    fn diagnostic_set_iter_for_line_returns_sorted_line_window() {
        let mut diagnostics = DiagnosticSet::default();
        let path = PathBuf::from("src/main.rs");
        let mut previous_line = diagnostic(&path, DiagnosticSeverity::Warning, "rust-analyzer");
        previous_line.line = 2;
        previous_line.message = "previous line".to_owned();
        let mut later_target = diagnostic(&path, DiagnosticSeverity::Warning, "rust-analyzer");
        later_target.line = 4;
        later_target.column = 9;
        later_target.message = "later target".to_owned();
        let mut earlier_target = diagnostic(&path, DiagnosticSeverity::Error, "rust-analyzer");
        earlier_target.line = 4;
        earlier_target.column = 2;
        earlier_target.message = "earlier target".to_owned();
        let mut next_line = diagnostic(&path, DiagnosticSeverity::Hint, "rust-analyzer");
        next_line.line = 6;
        next_line.message = "next line".to_owned();

        diagnostics.replace(
            path.clone(),
            vec![next_line, later_target, previous_line, earlier_target],
        );

        let iter_messages = diagnostics
            .iter_for_line(&path, 4)
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>();
        let collected_messages = diagnostics
            .for_line(&path, 4)
            .into_iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>();

        assert_eq!(iter_messages, vec!["earlier target", "later target"]);
        assert_eq!(collected_messages, iter_messages);
        assert!(diagnostics.iter_for_line(&path, 5).next().is_none());
    }

    #[test]
    fn diagnostic_set_sorted_range_iterates_visible_window_without_collecting_all_items() {
        let mut diagnostics = DiagnosticSet::default();
        let a = PathBuf::from("src/a.rs");
        let b = PathBuf::from("src/b.rs");
        let c = PathBuf::from("src/c.rs");
        let mut a_first = diagnostic(&a, DiagnosticSeverity::Warning, "rust-analyzer");
        a_first.line = 2;
        a_first.message = "a first".to_owned();
        let mut b_first = diagnostic(&b, DiagnosticSeverity::Error, "rust-analyzer");
        b_first.line = 1;
        b_first.message = "b first".to_owned();
        let mut b_second = diagnostic(&b, DiagnosticSeverity::Hint, "rust-analyzer");
        b_second.line = 9;
        b_second.message = "b second".to_owned();
        let mut c_first = diagnostic(&c, DiagnosticSeverity::Info, "rust-analyzer");
        c_first.line = 1;
        c_first.message = "c first".to_owned();

        diagnostics.replace(c.clone(), vec![c_first]);
        diagnostics.replace(b.clone(), vec![b_second, b_first]);
        diagnostics.replace(a.clone(), vec![a_first]);

        assert_eq!(
            diagnostics
                .sorted_range(1..3)
                .map(|(index, diagnostic)| {
                    (
                        index,
                        diagnostic.path.clone(),
                        diagnostic.line,
                        diagnostic.message.as_str(),
                    )
                })
                .collect::<Vec<_>>(),
            vec![(1, b.clone(), 1, "b first"), (2, b.clone(), 9, "b second")]
        );
        assert!(diagnostics.sorted_range(4..9).next().is_none());
        let reversed_start = 3;
        let reversed_end = 1;
        assert!(
            diagnostics
                .sorted_range(reversed_start..reversed_end)
                .next()
                .is_none()
        );
    }

    #[test]
    fn diagnostic_set_navigates_next_and_previous_without_collecting_all_items() {
        let mut diagnostics = DiagnosticSet::default();
        let a = PathBuf::from("src/a.rs");
        let b = PathBuf::from("src/b.rs");
        let c = PathBuf::from("src/c.rs");
        let mut a_first = diagnostic(&a, DiagnosticSeverity::Warning, "rust-analyzer");
        a_first.line = 2;
        a_first.column = 4;
        a_first.message = "a first".to_owned();
        let mut a_second = diagnostic(&a, DiagnosticSeverity::Error, "rust-analyzer");
        a_second.line = 8;
        a_second.column = 1;
        a_second.message = "a second".to_owned();
        let mut c_only = diagnostic(&c, DiagnosticSeverity::Hint, "rust-analyzer");
        c_only.line = 1;
        c_only.column = 2;
        c_only.message = "c only".to_owned();

        diagnostics.replace(a.clone(), vec![a_second, a_first]);
        diagnostics.replace(c.clone(), vec![c_only]);

        assert_eq!(
            diagnostics.next_after(&a, 2, 4).map(|diagnostic| {
                (
                    diagnostic.path.clone(),
                    diagnostic.line,
                    diagnostic.column,
                    diagnostic.message.as_str(),
                )
            }),
            Some((a.clone(), 8, 1, "a second"))
        );
        assert_eq!(
            diagnostics.next_after(&b, 1, 1).map(|diagnostic| {
                (
                    diagnostic.path.clone(),
                    diagnostic.line,
                    diagnostic.column,
                    diagnostic.message.as_str(),
                )
            }),
            Some((c.clone(), 1, 2, "c only"))
        );
        assert_eq!(
            diagnostics
                .next_after(&c, 99, 99)
                .map(|diagnostic| diagnostic.path.clone()),
            Some(a.clone())
        );
        assert_eq!(
            diagnostics
                .previous_before(&c, 1, 2)
                .map(|diagnostic| diagnostic.message.as_str()),
            Some("a second")
        );
        assert_eq!(
            diagnostics
                .previous_before(&b, 1, 1)
                .map(|diagnostic| diagnostic.message.as_str()),
            Some("a second")
        );
        assert_eq!(
            diagnostics
                .previous_before(&a, 1, 1)
                .map(|diagnostic| diagnostic.path.clone()),
            Some(c)
        );
    }

    #[test]
    fn diagnostic_set_navigates_with_lexically_equivalent_anchor_paths() {
        let noisy = PathBuf::from("workspace/src/../src/main.rs");
        let normalized = PathBuf::from("workspace/src/main.rs");
        let mut diagnostics = DiagnosticSet::default();
        let mut first = diagnostic(&noisy, DiagnosticSeverity::Warning, "rust-analyzer");
        first.line = 2;
        first.column = 4;
        first.message = "first".to_owned();
        let mut second = diagnostic(&noisy, DiagnosticSeverity::Error, "rust-analyzer");
        second.line = 8;
        second.column = 1;
        second.message = "second".to_owned();

        diagnostics.replace(noisy.clone(), vec![second, first]);

        assert_eq!(
            diagnostics
                .next_after(&normalized, 2, 4)
                .map(|diagnostic| diagnostic.message.as_str()),
            Some("second")
        );
        assert_eq!(
            diagnostics
                .previous_before(&normalized, 8, 1)
                .map(|diagnostic| diagnostic.message.as_str()),
            Some("first")
        );
        assert_eq!(
            diagnostics
                .first()
                .map(|diagnostic| diagnostic.path.clone()),
            Some(normalized)
        );
    }
}
