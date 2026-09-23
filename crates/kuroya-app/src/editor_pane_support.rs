use crate::{
    editor_text_geometry::visual_column_for_char_offset,
    lsp_edits::document_highlight_char_range,
    lsp_labels::{diagnostic_message_summary, diagnostic_priority},
    lsp_text_positions::{
        lsp_line_content_utf16_len, lsp_one_based_utf16_span_to_buffer_char_range,
    },
};
use eframe::egui::{self, Color32, pos2, vec2};
use kuroya_core::{
    Diagnostic, DiagnosticSeverity, GitBlameLine, LspCodeLens, LspDocumentHighlight, LspInlayHint,
    LspSemanticToken, TextBuffer,
};
use std::{
    collections::HashMap,
    ops::Range,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex, MutexGuard},
};

pub(crate) type DocumentHighlightSpan = (Range<usize>, Option<u8>);
pub(crate) type SemanticTokenSpan = (Range<usize>, String, Vec<String>);
pub(crate) type DiagnosticTagSpan = (Range<usize>, DiagnosticTagKind);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiagnosticTagKind {
    Unused,
    Deprecated,
}

pub(crate) fn fingerprint_fold_u64(hash: &mut u64, value: u64) {
    *hash = (*hash ^ value).wrapping_mul(0x100_0000_01b3);
}

pub(crate) fn fingerprint_fold_str(hash: &mut u64, value: &str) {
    for byte in value.as_bytes() {
        fingerprint_fold_u64(hash, u64::from(*byte));
    }
}

pub(crate) fn document_highlight_spans_for_buffer(
    buffer: &TextBuffer,
    highlights_path: Option<&Path>,
    highlights: &[LspDocumentHighlight],
) -> Vec<DocumentHighlightSpan> {
    let Some(highlights_path) = highlights_path else {
        return Vec::new();
    };
    if buffer
        .path()
        .is_none_or(|buffer_path| buffer_path.as_path() != highlights_path)
    {
        return Vec::new();
    }

    let mut spans = highlights
        .iter()
        .filter_map(|highlight| {
            document_highlight_char_range(buffer, highlight).map(|range| (range, highlight.kind))
        })
        .collect::<Vec<_>>();
    sort_document_highlight_spans(&mut spans);
    spans
}

pub(crate) fn diagnostic_line_maps(
    diagnostics: &[Diagnostic],
) -> (HashMap<usize, DiagnosticSeverity>, HashMap<usize, String>) {
    let mut diagnostics_by_line: HashMap<usize, (DiagnosticSeverity, usize, String)> =
        HashMap::new();
    for diagnostic in diagnostics {
        diagnostics_by_line
            .entry(diagnostic.line)
            .and_modify(|(severity, column, message)| {
                let current_priority = diagnostic_priority(*severity);
                let next_priority = diagnostic_priority(diagnostic.severity);
                if next_priority < current_priority
                    || (next_priority == current_priority && diagnostic.column < *column)
                {
                    *severity = diagnostic.severity;
                    *column = diagnostic.column;
                    *message = diagnostic_message_summary(&diagnostic.message);
                }
            })
            .or_insert_with(|| {
                (
                    diagnostic.severity,
                    diagnostic.column,
                    diagnostic_message_summary(&diagnostic.message),
                )
            });
    }

    let mut diagnostic_messages = HashMap::with_capacity(diagnostics_by_line.len());
    let diagnostics_by_line = diagnostics_by_line
        .into_iter()
        .map(|(line, (severity, _, message))| {
            diagnostic_messages.insert(line, message);
            (line, severity)
        })
        .collect();

    (diagnostics_by_line, diagnostic_messages)
}

pub(crate) fn diagnostic_tag_spans_for_buffer(
    buffer: &TextBuffer,
    diagnostics: &[Diagnostic],
    show_unused: bool,
    show_deprecated: bool,
) -> Vec<DiagnosticTagSpan> {
    let mut spans = Vec::new();
    for diagnostic in diagnostics {
        let Some(range) = diagnostic_tag_char_range(buffer, diagnostic) else {
            continue;
        };
        if show_unused && diagnostic.unused {
            spans.push((range.clone(), DiagnosticTagKind::Unused));
        }
        if show_deprecated && diagnostic.deprecated {
            spans.push((range, DiagnosticTagKind::Deprecated));
        }
    }
    sort_diagnostic_tag_spans(&mut spans);
    spans
}

fn diagnostic_tag_char_range(buffer: &TextBuffer, diagnostic: &Diagnostic) -> Option<Range<usize>> {
    if diagnostic.line == 0 || diagnostic.line > buffer.len_lines() {
        return None;
    }

    let line_idx = diagnostic.line - 1;
    let line_text = buffer.line(line_idx)?;
    let line_text = line_text.trim_end_matches(['\r', '\n']);
    let line_chars = line_text.chars().count();
    let start_column = diagnostic.column.saturating_sub(1).min(line_chars);
    if start_column >= line_chars {
        return None;
    }

    let width = diagnostic
        .char_range
        .end
        .saturating_sub(diagnostic.char_range.start)
        .max(1);
    let end_column = start_column.saturating_add(width).min(line_chars);
    let start = buffer.line_column_to_char(line_idx, start_column);
    let end = buffer.line_column_to_char(line_idx, end_column);
    (start < end).then_some(start..end)
}

pub(crate) fn semantic_token_spans_for_buffer(
    buffer: &TextBuffer,
    tokens: &[LspSemanticToken],
) -> Vec<SemanticTokenSpan> {
    let mut spans = tokens
        .iter()
        .filter_map(|token| semantic_token_char_range(buffer, token))
        .collect::<Vec<_>>();
    sort_semantic_token_spans(&mut spans);
    spans
}

const MAX_SEMANTIC_TOKEN_SPAN_CACHE_ENTRIES: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SemanticTokenSpanCacheKey {
    buffer_id: u64,
    path: Option<PathBuf>,
    buffer_version: u64,
    buffer_len_lines: usize,
    buffer_len_chars: usize,
    tokens_len: usize,
    tokens_fingerprint: u64,
}

#[derive(Default)]
struct SemanticTokenSpanCache {
    entries: HashMap<SemanticTokenSpanCacheKey, Arc<Vec<SemanticTokenSpan>>>,
}

impl SemanticTokenSpanCache {
    fn get(&self, key: &SemanticTokenSpanCacheKey) -> Option<Arc<Vec<SemanticTokenSpan>>> {
        self.entries.get(key).cloned()
    }

    fn insert(&mut self, key: SemanticTokenSpanCacheKey, spans: Arc<Vec<SemanticTokenSpan>>) {
        if self.entries.len() >= MAX_SEMANTIC_TOKEN_SPAN_CACHE_ENTRIES {
            self.entries.clear();
        }
        self.entries.insert(key, spans);
    }
}

fn semantic_token_span_cache() -> MutexGuard<'static, SemanticTokenSpanCache> {
    static SEMANTIC_TOKEN_SPAN_CACHE: LazyLock<Mutex<SemanticTokenSpanCache>> =
        LazyLock::new(|| Mutex::new(SemanticTokenSpanCache::default()));
    SEMANTIC_TOKEN_SPAN_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn cached_semantic_token_spans_for_buffer(
    buffer: &TextBuffer,
    path: Option<&Path>,
    tokens: Option<&[LspSemanticToken]>,
) -> Arc<Vec<SemanticTokenSpan>> {
    let Some(tokens) = tokens else {
        return Arc::default();
    };
    let mut hash = tokens.len() as u64;
    for token in tokens {
        fingerprint_fold_u64(&mut hash, token.line as u64);
        fingerprint_fold_u64(&mut hash, token.column as u64);
        fingerprint_fold_u64(&mut hash, token.length as u64);
        fingerprint_fold_str(&mut hash, &token.token_type);
        fingerprint_fold_u64(&mut hash, token.modifiers.len() as u64);
        for modifier in &token.modifiers {
            fingerprint_fold_str(&mut hash, modifier);
        }
    }
    let key = SemanticTokenSpanCacheKey {
        buffer_id: buffer.id(),
        path: path.map(Path::to_path_buf),
        buffer_version: buffer.version(),
        buffer_len_lines: buffer.len_lines(),
        buffer_len_chars: buffer.len_chars(),
        tokens_len: tokens.len(),
        tokens_fingerprint: hash,
    };
    let mut cache = semantic_token_span_cache();
    if let Some(spans) = cache.get(&key) {
        return spans;
    }
    let spans = Arc::new(semantic_token_spans_for_buffer(buffer, tokens));
    cache.insert(key, Arc::clone(&spans));
    spans
}

const MAX_RENDERABLE_ANNOTATION_CACHE_ENTRIES: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RenderableAnnotationKey {
    path: Option<PathBuf>,
    line_count: usize,
    source_len: usize,
    source_fingerprint: u64,
}

fn cached_renderable_annotations<T>(
    entries: &mut HashMap<RenderableAnnotationKey, Arc<Vec<T>>>,
    path: Option<&Path>,
    source: &[T],
    line_count: usize,
    source_fingerprint: u64,
    render: fn(&[T], usize) -> Vec<T>,
) -> Arc<Vec<T>> {
    let key = RenderableAnnotationKey {
        path: path.map(Path::to_path_buf),
        line_count,
        source_len: source.len(),
        source_fingerprint,
    };
    if let Some(cached) = entries.get(&key) {
        return Arc::clone(cached);
    }
    let renderable = Arc::new(render(source, line_count));
    if entries.len() >= MAX_RENDERABLE_ANNOTATION_CACHE_ENTRIES {
        entries.clear();
    }
    entries.insert(key, Arc::clone(&renderable));
    renderable
}

fn renderable_git_blame_lines_cache()
-> MutexGuard<'static, HashMap<RenderableAnnotationKey, Arc<Vec<GitBlameLine>>>> {
    static CACHE: LazyLock<Mutex<HashMap<RenderableAnnotationKey, Arc<Vec<GitBlameLine>>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn renderable_inlay_hints_cache()
-> MutexGuard<'static, HashMap<RenderableAnnotationKey, Arc<Vec<LspInlayHint>>>> {
    static CACHE: LazyLock<Mutex<HashMap<RenderableAnnotationKey, Arc<Vec<LspInlayHint>>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn renderable_code_lenses_cache()
-> MutexGuard<'static, HashMap<RenderableAnnotationKey, Arc<Vec<LspCodeLens>>>> {
    static CACHE: LazyLock<Mutex<HashMap<RenderableAnnotationKey, Arc<Vec<LspCodeLens>>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn git_blame_lines_fingerprint(lines: &[GitBlameLine]) -> u64 {
    let mut hash = lines.len() as u64;
    for line in lines {
        fingerprint_fold_u64(&mut hash, line.line_number as u64);
        fingerprint_fold_u64(&mut hash, line.author_time_seconds as u64);
        fingerprint_fold_str(&mut hash, &line.short_oid);
        fingerprint_fold_str(&mut hash, &line.author);
        fingerprint_fold_str(&mut hash, &line.summary);
    }
    hash
}

fn inlay_hints_fingerprint(hints: &[LspInlayHint]) -> u64 {
    let mut hash = hints.len() as u64;
    for hint in hints {
        fingerprint_fold_u64(&mut hash, hint.line as u64);
        fingerprint_fold_u64(&mut hash, hint.column as u64);
        fingerprint_fold_u64(&mut hash, hint.kind.map(u64::from).unwrap_or(u64::MAX));
        fingerprint_fold_str(&mut hash, &hint.label);
    }
    hash
}

fn code_lenses_fingerprint(lenses: &[LspCodeLens]) -> u64 {
    let mut hash = lenses.len() as u64;
    for lens in lenses {
        fingerprint_fold_u64(&mut hash, lens.line as u64);
        fingerprint_fold_u64(&mut hash, lens.column as u64);
        fingerprint_fold_str(&mut hash, &lens.title);
        fingerprint_fold_str(&mut hash, lens.command.as_deref().unwrap_or_default());
    }
    hash
}

pub(crate) fn cached_renderable_git_blame_lines(
    path: Option<&Path>,
    lines: &[GitBlameLine],
    line_count: usize,
) -> Arc<Vec<GitBlameLine>> {
    let mut entries = renderable_git_blame_lines_cache();
    cached_renderable_annotations(
        &mut entries,
        path,
        lines,
        line_count,
        git_blame_lines_fingerprint(lines),
        crate::editor_pane_data::renderable_git_blame_lines,
    )
}

pub(crate) fn cached_renderable_inlay_hints(
    path: Option<&Path>,
    hints: &[LspInlayHint],
    line_count: usize,
) -> Arc<Vec<LspInlayHint>> {
    let mut entries = renderable_inlay_hints_cache();
    cached_renderable_annotations(
        &mut entries,
        path,
        hints,
        line_count,
        inlay_hints_fingerprint(hints),
        crate::editor_pane_data::renderable_inlay_hints,
    )
}

pub(crate) fn cached_renderable_code_lenses(
    path: Option<&Path>,
    lenses: &[LspCodeLens],
    line_count: usize,
) -> Arc<Vec<LspCodeLens>> {
    let mut entries = renderable_code_lenses_cache();
    cached_renderable_annotations(
        &mut entries,
        path,
        lenses,
        line_count,
        code_lenses_fingerprint(lenses),
        crate::editor_pane_data::renderable_code_lenses,
    )
}

fn semantic_token_char_range(
    buffer: &TextBuffer,
    token: &LspSemanticToken,
) -> Option<SemanticTokenSpan> {
    if token.length == 0 || token.line == 0 || token.line > buffer.len_lines() {
        return None;
    }

    let start_utf16 = token.column.saturating_sub(1);
    let line_utf16_len = lsp_line_content_utf16_len(buffer, token.line - 1)?;
    let clamped_length = token.length.min(line_utf16_len.saturating_sub(start_utf16));
    let range = lsp_one_based_utf16_span_to_buffer_char_range(
        buffer,
        token.line,
        token.column,
        clamped_length,
    )?;
    Some((range, token.token_type.clone(), token.modifiers.clone()))
}

fn sort_document_highlight_spans(spans: &mut [DocumentHighlightSpan]) {
    spans.sort_by(|left, right| {
        left.0
            .start
            .cmp(&right.0.start)
            .then(left.0.end.cmp(&right.0.end))
            .then(left.1.cmp(&right.1))
    });
}

fn sort_diagnostic_tag_spans(spans: &mut [DiagnosticTagSpan]) {
    spans.sort_by(|left, right| {
        left.0
            .start
            .cmp(&right.0.start)
            .then(left.0.end.cmp(&right.0.end))
            .then(tag_kind_sort_key(left.1).cmp(&tag_kind_sort_key(right.1)))
    });
}

fn sort_semantic_token_spans(spans: &mut [SemanticTokenSpan]) {
    spans.sort_by(|left, right| {
        left.0
            .start
            .cmp(&right.0.start)
            .then(left.0.end.cmp(&right.0.end))
            .then(left.1.cmp(&right.1))
            .then(left.2.cmp(&right.2))
    });
}

fn tag_kind_sort_key(kind: DiagnosticTagKind) -> u8 {
    match kind {
        DiagnosticTagKind::Unused => 0,
        DiagnosticTagKind::Deprecated => 1,
    }
}

pub(crate) fn paint_char_range_highlight(
    painter: &egui::Painter,
    rect: egui::Rect,
    gutter_width: f32,
    char_width: f32,
    row_height: f32,
    snapshot_range: &Range<usize>,
    line_text: &str,
    tab_width: usize,
    range: &Range<usize>,
    color: Color32,
) {
    paint_char_range_highlight_with_corner_radius(
        painter,
        rect,
        gutter_width,
        char_width,
        row_height,
        snapshot_range,
        line_text,
        tab_width,
        range,
        color,
        2.0,
    );
}

pub(crate) fn paint_char_range_highlight_with_corner_radius(
    painter: &egui::Painter,
    rect: egui::Rect,
    gutter_width: f32,
    char_width: f32,
    row_height: f32,
    snapshot_range: &Range<usize>,
    line_text: &str,
    tab_width: usize,
    range: &Range<usize>,
    color: Color32,
    corner_radius: f32,
) {
    let start = range.start.max(snapshot_range.start);
    let end = range.end.min(snapshot_range.end);
    if start >= end {
        return;
    }

    let text_char_count = line_text.chars().count();
    let char_start = start
        .saturating_sub(snapshot_range.start)
        .min(text_char_count);
    let char_end = end
        .saturating_sub(snapshot_range.start)
        .min(text_char_count);
    let col_start = visual_column_for_char_offset(line_text, char_start, tab_width);
    let mut col_end = visual_column_for_char_offset(line_text, char_end, tab_width);
    if col_end <= col_start {
        col_end = col_start + 1;
    }
    painter.rect_filled(
        egui::Rect::from_min_size(
            pos2(
                rect.left() + gutter_width + col_start as f32 * char_width,
                rect.top() + 2.0,
            ),
            vec2((col_end - col_start) as f32 * char_width, row_height - 4.0),
        ),
        corner_radius,
        color,
    );
}

#[cfg(test)]
mod tests {
    use super::{
        DiagnosticTagKind, cached_renderable_code_lenses, cached_renderable_git_blame_lines,
        cached_renderable_inlay_hints, cached_semantic_token_spans_for_buffer,
        diagnostic_line_maps, diagnostic_tag_spans_for_buffer, document_highlight_spans_for_buffer,
        semantic_token_spans_for_buffer,
    };
    use kuroya_core::{
        Diagnostic, DiagnosticSeverity, GitBlameLine, LspCodeLens, LspDocumentHighlight,
        LspInlayHint, LspSemanticToken, TextBuffer,
    };
    use std::{path::PathBuf, sync::Arc};

    #[test]
    fn semantic_token_spans_convert_lsp_positions_to_buffer_ranges() {
        let buffer = TextBuffer::from_text(1, None, "fn main() {\n    value\n}".to_owned());
        let spans = semantic_token_spans_for_buffer(
            &buffer,
            &[
                LspSemanticToken {
                    line: 1,
                    column: 4,
                    length: 4,
                    token_type: "function".to_owned(),
                    modifiers: vec!["declaration".to_owned()],
                },
                LspSemanticToken {
                    line: 2,
                    column: 5,
                    length: 5,
                    token_type: "variable".to_owned(),
                    modifiers: Vec::new(),
                },
            ],
        );

        assert_eq!(
            spans,
            vec![
                (3..7, "function".to_owned(), vec!["declaration".to_owned()]),
                (16..21, "variable".to_owned(), Vec::new())
            ]
        );
    }

    #[test]
    fn semantic_token_spans_convert_utf16_columns_to_buffer_ranges() {
        let buffer = TextBuffer::from_text(1, None, "😀alpha".to_owned());
        let spans = semantic_token_spans_for_buffer(
            &buffer,
            &[
                LspSemanticToken {
                    line: 1,
                    column: 1,
                    length: 2,
                    token_type: "emoji".to_owned(),
                    modifiers: Vec::new(),
                },
                LspSemanticToken {
                    line: 1,
                    column: 3,
                    length: 5,
                    token_type: "identifier".to_owned(),
                    modifiers: Vec::new(),
                },
            ],
        );

        assert_eq!(
            spans,
            vec![
                (0..1, "emoji".to_owned(), Vec::new()),
                (1..6, "identifier".to_owned(), Vec::new())
            ]
        );
    }

    #[test]
    fn semantic_token_spans_skip_invalid_or_empty_ranges() {
        let buffer = TextBuffer::from_text(1, None, "value".to_owned());
        let spans = semantic_token_spans_for_buffer(
            &buffer,
            &[
                LspSemanticToken {
                    line: 0,
                    column: 1,
                    length: 5,
                    token_type: "variable".to_owned(),
                    modifiers: Vec::new(),
                },
                LspSemanticToken {
                    line: 1,
                    column: 10,
                    length: 5,
                    token_type: "variable".to_owned(),
                    modifiers: Vec::new(),
                },
                LspSemanticToken {
                    line: 2,
                    column: 1,
                    length: 5,
                    token_type: "variable".to_owned(),
                    modifiers: Vec::new(),
                },
            ],
        );

        assert!(spans.is_empty());
    }

    #[test]
    fn semantic_token_spans_are_sorted_for_row_slicing() {
        let buffer = TextBuffer::from_text(1, None, "one\ntwo\nthree".to_owned());
        let spans = semantic_token_spans_for_buffer(
            &buffer,
            &[
                semantic_token(3, 1, 5, "variable"),
                semantic_token(1, 1, 3, "keyword"),
                semantic_token(2, 1, 3, "function"),
            ],
        );

        assert_eq!(
            spans
                .iter()
                .map(|(range, token_type, _)| (range.clone(), token_type.as_str()))
                .collect::<Vec<_>>(),
            vec![(0..3, "keyword"), (4..7, "function"), (8..13, "variable")]
        );
    }

    #[test]
    fn semantic_token_spans_clamp_multi_line_tokens_to_the_first_line() {
        let buffer = TextBuffer::from_text(1, None, "/* d\nend */".to_owned());

        let spans =
            semantic_token_spans_for_buffer(&buffer, &[semantic_token(1, 1, 11, "comment")]);

        assert_eq!(spans, vec![(0..4, "comment".to_owned(), Vec::new())]);
    }

    #[test]
    fn document_highlight_spans_are_sorted_for_row_slicing() {
        let path = PathBuf::from("src/main.rs");
        let buffer = TextBuffer::from_text(1, Some(path.clone()), "one\ntwo\nthree".to_owned());
        let spans = document_highlight_spans_for_buffer(
            &buffer,
            Some(&path),
            &[
                document_highlight(3, 1, 3, 6, Some(2)),
                document_highlight(1, 1, 1, 4, Some(1)),
                document_highlight(2, 1, 2, 4, None),
            ],
        );

        assert_eq!(spans, vec![(0..3, Some(1)), (4..7, None), (8..13, Some(2))]);
    }

    #[test]
    fn diagnostic_line_maps_summarize_messages_for_inline_display() {
        let path = PathBuf::from("src/main.rs");
        let diagnostics = vec![
            Diagnostic {
                path: path.clone(),
                line: 2,
                column: 1,
                char_range: 0..1,
                severity: DiagnosticSeverity::Warning,
                source: "rust-analyzer".to_owned(),
                message: "first line\n\tsecond line\u{7}".to_owned(),
                unused: false,
                deprecated: false,
            },
            Diagnostic {
                path,
                line: 2,
                column: 4,
                char_range: 3..5,
                severity: DiagnosticSeverity::Error,
                source: "rust-analyzer".to_owned(),
                message: "later diagnostic".to_owned(),
                unused: false,
                deprecated: false,
            },
        ];

        let (diagnostics_by_line, diagnostic_messages) = diagnostic_line_maps(&diagnostics);

        assert_eq!(
            diagnostics_by_line.get(&2),
            Some(&DiagnosticSeverity::Error)
        );
        assert_eq!(
            diagnostic_messages.get(&2).map(String::as_str),
            Some("later diagnostic")
        );
    }

    #[test]
    fn diagnostic_line_maps_tie_break_same_severity_by_earlier_column() {
        let path = PathBuf::from("src/main.rs");
        let diagnostics = vec![
            Diagnostic {
                path: path.clone(),
                line: 3,
                column: 12,
                char_range: 11..15,
                severity: DiagnosticSeverity::Warning,
                source: "rust-analyzer".to_owned(),
                message: "later column".to_owned(),
                unused: false,
                deprecated: false,
            },
            Diagnostic {
                path,
                line: 3,
                column: 2,
                char_range: 1..5,
                severity: DiagnosticSeverity::Warning,
                source: "rust-analyzer".to_owned(),
                message: "earlier column".to_owned(),
                unused: false,
                deprecated: false,
            },
        ];

        let (diagnostics_by_line, diagnostic_messages) = diagnostic_line_maps(&diagnostics);

        assert_eq!(
            diagnostics_by_line.get(&3),
            Some(&DiagnosticSeverity::Warning)
        );
        assert_eq!(
            diagnostic_messages.get(&3).map(String::as_str),
            Some("earlier column")
        );
    }

    #[test]
    fn diagnostic_tag_spans_follow_tag_settings_and_buffer_ranges() {
        let path = PathBuf::from("src/main.rs");
        let buffer = TextBuffer::from_text(1, Some(path.clone()), "alpha beta\n".to_owned());
        let diagnostics = vec![
            Diagnostic {
                path: path.clone(),
                line: 1,
                column: 1,
                char_range: 0..5,
                severity: DiagnosticSeverity::Hint,
                source: "rust-analyzer".to_owned(),
                message: "unused".to_owned(),
                unused: true,
                deprecated: false,
            },
            Diagnostic {
                path,
                line: 1,
                column: 7,
                char_range: 6..10,
                severity: DiagnosticSeverity::Hint,
                source: "rust-analyzer".to_owned(),
                message: "deprecated".to_owned(),
                unused: false,
                deprecated: true,
            },
        ];

        assert_eq!(
            diagnostic_tag_spans_for_buffer(&buffer, &diagnostics, true, true),
            vec![
                (0..5, DiagnosticTagKind::Unused),
                (6..10, DiagnosticTagKind::Deprecated)
            ]
        );
        assert_eq!(
            diagnostic_tag_spans_for_buffer(&buffer, &diagnostics, false, true),
            vec![(6..10, DiagnosticTagKind::Deprecated)]
        );
    }

    #[test]
    fn diagnostic_tag_spans_are_sorted_for_row_slicing() {
        let path = PathBuf::from("src/main.rs");
        let buffer = TextBuffer::from_text(1, Some(path.clone()), "alpha\nbeta\n".to_owned());
        let diagnostics = vec![
            Diagnostic {
                path: path.clone(),
                line: 2,
                column: 1,
                char_range: 0..4,
                severity: DiagnosticSeverity::Hint,
                source: "rust-analyzer".to_owned(),
                message: "deprecated".to_owned(),
                unused: false,
                deprecated: true,
            },
            Diagnostic {
                path,
                line: 1,
                column: 1,
                char_range: 0..5,
                severity: DiagnosticSeverity::Hint,
                source: "rust-analyzer".to_owned(),
                message: "unused".to_owned(),
                unused: true,
                deprecated: false,
            },
        ];

        assert_eq!(
            diagnostic_tag_spans_for_buffer(&buffer, &diagnostics, true, true),
            vec![
                (0..5, DiagnosticTagKind::Unused),
                (6..10, DiagnosticTagKind::Deprecated),
            ]
        );
    }

    #[test]
    fn diagnostic_tag_spans_clamp_multi_line_sentinel_ranges_to_line_end() {
        let path = PathBuf::from("src/main.rs");
        let buffer = TextBuffer::from_text(1, Some(path.clone()), "alpha beta\n".to_owned());
        let diagnostics = vec![Diagnostic {
            path,
            line: 1,
            column: 7,

            char_range: 6..usize::MAX,
            severity: DiagnosticSeverity::Hint,
            source: "rust-analyzer".to_owned(),
            message: "multi line".to_owned(),
            unused: true,
            deprecated: false,
        }];

        assert_eq!(
            diagnostic_tag_spans_for_buffer(&buffer, &diagnostics, true, false),
            vec![(6..10, DiagnosticTagKind::Unused)]
        );
    }

    fn semantic_token(
        line: usize,
        column: usize,
        length: usize,
        token_type: &str,
    ) -> LspSemanticToken {
        LspSemanticToken {
            line,
            column,
            length,
            token_type: token_type.to_owned(),
            modifiers: Vec::new(),
        }
    }

    #[test]
    fn cached_semantic_token_spans_match_uncached_projection() {
        let path = PathBuf::from("cache-test/projection.rs");
        let buffer = TextBuffer::from_text(
            1,
            Some(path.clone()),
            "fn main() {\n    value\n}".to_owned(),
        );
        let tokens = [
            LspSemanticToken {
                line: 1,
                column: 4,
                length: 4,
                token_type: "function".to_owned(),
                modifiers: vec!["declaration".to_owned()],
            },
            semantic_token(2, 5, 5, "variable"),
        ];

        let cached = cached_semantic_token_spans_for_buffer(&buffer, Some(&path), Some(&tokens));
        let repeated = cached_semantic_token_spans_for_buffer(&buffer, Some(&path), Some(&tokens));

        assert_eq!(
            cached.as_slice(),
            vec![
                (3..7, "function".to_owned(), vec!["declaration".to_owned()]),
                (16..21, "variable".to_owned(), Vec::new())
            ]
        );
        assert_eq!(
            cached.as_slice(),
            semantic_token_spans_for_buffer(&buffer, &tokens)
        );

        assert!(Arc::ptr_eq(&cached, &repeated));

        assert!(cached_semantic_token_spans_for_buffer(&buffer, Some(&path), None).is_empty());
    }

    #[test]
    fn semantic_token_span_cache_invalidates_on_buffer_version_change() {
        let path = PathBuf::from("cache-test/version-change.rs");
        let mut buffer = TextBuffer::from_text(1, Some(path.clone()), "fn main() {}".to_owned());
        let tokens = [
            semantic_token(1, 1, 2, "keyword"),
            semantic_token(1, 4, 4, "function"),
        ];

        let before = cached_semantic_token_spans_for_buffer(&buffer, Some(&path), Some(&tokens));
        assert!(buffer.replace_range(0..0, "// "));
        let after = cached_semantic_token_spans_for_buffer(&buffer, Some(&path), Some(&tokens));

        assert!(!Arc::ptr_eq(&before, &after));
        assert_eq!(
            after.as_slice(),
            semantic_token_spans_for_buffer(&buffer, &tokens)
        );
    }

    #[test]
    fn semantic_token_span_cache_invalidates_on_token_list_replacement() {
        let path = PathBuf::from("cache-test/token-replacement.rs");
        let buffer = TextBuffer::from_text(1, Some(path.clone()), "fn main() {}".to_owned());
        let original = [semantic_token(1, 1, 2, "keyword")];
        let replaced = [
            semantic_token(1, 1, 4, "function"),
            semantic_token(1, 5, 4, "variable"),
        ];

        let before = cached_semantic_token_spans_for_buffer(&buffer, Some(&path), Some(&original));
        let after = cached_semantic_token_spans_for_buffer(&buffer, Some(&path), Some(&replaced));

        assert!(!Arc::ptr_eq(&before, &after));
        assert_eq!(
            after.as_slice(),
            semantic_token_spans_for_buffer(&buffer, &replaced)
        );
        assert_eq!(
            after.as_slice(),
            vec![
                (0..4, "function".to_owned(), Vec::new()),
                (4..8, "variable".to_owned(), Vec::new())
            ]
        );
    }

    #[test]
    fn cached_renderable_annotations_reuse_and_invalidate() {
        let path = PathBuf::from("cache-test/annotations.rs");
        let blame = [
            blame_line(1, "first"),
            blame_line(3, "past"),
            blame_line(0, "zero"),
        ];

        let cached = cached_renderable_git_blame_lines(Some(&path), &blame, 2);
        let repeated = cached_renderable_git_blame_lines(Some(&path), &blame, 2);
        assert_eq!(cached.as_slice(), vec![blame_line(1, "first")]);
        assert!(Arc::ptr_eq(&cached, &repeated));

        let replaced = [blame_line(1, "first"), blame_line(2, "second")];
        let invalidated = cached_renderable_git_blame_lines(Some(&path), &replaced, 2);
        assert!(!Arc::ptr_eq(&cached, &invalidated));
        assert_eq!(
            invalidated.as_slice(),
            vec![blame_line(1, "first"), blame_line(2, "second")]
        );

        let other = PathBuf::from("cache-test/annotations-other.rs");
        let other_cached = cached_renderable_git_blame_lines(Some(&other), &blame, 2);
        assert!(!Arc::ptr_eq(&cached, &other_cached));

        let hints = [inlay_hint(1, "ok"), inlay_hint(3, "past")];
        let hints_cached = cached_renderable_inlay_hints(Some(&path), &hints, 2);
        assert_eq!(hints_cached.as_slice(), vec![inlay_hint(1, "ok")]);

        let lenses = [code_lens(1, "Run"), code_lens(3, "past")];
        let lenses_cached = cached_renderable_code_lenses(Some(&path), &lenses, 2);
        assert_eq!(lenses_cached.as_slice(), vec![code_lens(1, "Run")]);
    }

    fn blame_line(line_number: usize, author: &str) -> GitBlameLine {
        GitBlameLine {
            line_number,
            short_oid: "abc1234".to_owned(),
            author: author.to_owned(),
            author_time_seconds: 0,
            summary: "summary".to_owned(),
        }
    }

    fn inlay_hint(line: usize, label: &str) -> LspInlayHint {
        LspInlayHint {
            line,
            column: 1,
            label: label.to_owned(),
            kind: None,
        }
    }

    fn code_lens(line: usize, title: &str) -> LspCodeLens {
        LspCodeLens {
            line,
            column: 1,
            title: title.to_owned(),
            command: None,
            command_arguments: None,
            resolve_payload: None,
        }
    }

    fn document_highlight(
        line: usize,
        column: usize,
        end_line: usize,
        end_column: usize,
        kind: Option<u8>,
    ) -> LspDocumentHighlight {
        LspDocumentHighlight {
            line,
            column,
            end_line,
            end_column,
            kind,
        }
    }
}
