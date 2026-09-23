use crate::{Diagnostic, DiagnosticSeverity};
use serde::Deserialize;
use serde_json::{Value, json};
use std::ops::Range;
use std::path::{Path, PathBuf};

use super::{
    LSP_DIAGNOSTIC_TAG_DEPRECATED, LSP_DIAGNOSTIC_TAG_UNNECESSARY, LspRange,
    MAX_LSP_DIAGNOSTIC_MESSAGE_CHARS, MAX_LSP_DIAGNOSTIC_SOURCE_CHARS,
    MAX_LSP_DIAGNOSTICS_PER_FILE, ParsedLspPosition, bounded_lsp_text, file_uri_to_path,
    lsp_range_value, one_based_lsp_position_component, parse_lsp_range_bounds,
    parse_lsp_struct_range_bounds,
};

#[derive(Debug, Clone, Deserialize)]
pub struct PublishDiagnosticsParams {
    pub uri: String,
    #[serde(default)]
    pub version: Option<u64>,
    #[serde(default)]
    pub diagnostics: Vec<LspDiagnostic>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LspDiagnostic {
    pub range: LspRange,
    #[serde(default)]
    pub severity: Option<u8>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub tags: Vec<u8>,
    pub message: String,
}

pub fn diagnostics_from_lsp(
    params: PublishDiagnosticsParams,
) -> Option<(PathBuf, Option<u64>, Vec<Diagnostic>)> {
    let path = file_uri_to_path(&params.uri)?;
    let version = params.version;
    let mut diagnostics =
        Vec::with_capacity(params.diagnostics.len().min(MAX_LSP_DIAGNOSTICS_PER_FILE));
    for diagnostic in params
        .diagnostics
        .into_iter()
        .take(MAX_LSP_DIAGNOSTICS_PER_FILE)
    {
        // publishDiagnostics is full-state replacement, so aborting on one
        // malformed entry would keep stale diagnostics alive; skip the entry
        // and keep every parseable sibling instead.
        if let Some(diagnostic) = diagnostic_from_lsp_struct(diagnostic, &path) {
            diagnostics.push(diagnostic);
        }
    }

    Some((path, version, diagnostics))
}

pub fn parse_publish_diagnostics(value: &Value) -> Option<(PathBuf, Option<u64>, Vec<Diagnostic>)> {
    if value.get("method")?.as_str()? != "textDocument/publishDiagnostics" {
        return None;
    }
    let params = value.get("params")?;
    let path = file_uri_to_path(params.get("uri")?.as_str()?)?;
    let version = params.get("version").and_then(Value::as_u64);
    let lsp_diagnostics: &[Value] = match params.get("diagnostics") {
        Some(diagnostics) => diagnostics.as_array()?.as_slice(),
        None => &[],
    };
    let mut diagnostics =
        Vec::with_capacity(lsp_diagnostics.len().min(MAX_LSP_DIAGNOSTICS_PER_FILE));

    for diagnostic in lsp_diagnostics.iter().take(MAX_LSP_DIAGNOSTICS_PER_FILE) {
        // Same full-state replacement contract as `diagnostics_from_lsp`:
        // skip malformed entries rather than discarding the whole publish.
        if let Some(diagnostic) = diagnostic_from_lsp_value(diagnostic, &path) {
            diagnostics.push(diagnostic);
        }
    }

    Some((path, version, diagnostics))
}

pub(super) fn lsp_code_action_diagnostic(diagnostic: &Diagnostic) -> Value {
    let start_line = diagnostic.line.saturating_sub(1);
    let start_character = diagnostic.column.saturating_sub(1);
    let width = diagnostic
        .char_range
        .end
        .saturating_sub(diagnostic.char_range.start)
        .max(1);
    json!({
        "range": lsp_range_value(
            start_line,
            start_character,
            start_line,
            start_character.saturating_add(width),
        ),
        "severity": lsp_diagnostic_severity(diagnostic.severity),
        "source": diagnostic.source.as_str(),
        "message": diagnostic.message.as_str()
    })
}

/// Builds the line-relative `Diagnostic::char_range` from an LSP range.
///
/// LSP ranges may span multiple lines, but `end.character` belongs to the end
/// line; using it as a column on the start line collapses a multi-line range
/// to a bogus start-line width. When the range ends on a later line, store a
/// sentinel end that consumers clamp to the start line's content length
/// instead.
fn lsp_diagnostic_char_range(
    start: ParsedLspPosition,
    end: ParsedLspPosition,
    column: usize,
) -> Range<usize> {
    if end.line > start.line {
        start.character..usize::MAX
    } else {
        start.character..end.character.max(column)
    }
}

fn diagnostic_from_lsp_struct(diagnostic: LspDiagnostic, path: &Path) -> Option<Diagnostic> {
    let (start, end) = parse_lsp_struct_range_bounds(&diagnostic.range)?;
    let line = one_based_lsp_position_component(start.line)?;
    let column = one_based_lsp_position_component(start.character)?;
    Some(Diagnostic {
        path: path.to_path_buf(),
        line,
        column,
        char_range: lsp_diagnostic_char_range(start, end, column),
        severity: lsp_severity(diagnostic.severity),
        source: diagnostic
            .source
            .and_then(|source| bounded_lsp_text(&source, MAX_LSP_DIAGNOSTIC_SOURCE_CHARS))
            .unwrap_or_else(|| "lsp".to_owned()),
        unused: diagnostic.tags.contains(&LSP_DIAGNOSTIC_TAG_UNNECESSARY),
        deprecated: diagnostic.tags.contains(&LSP_DIAGNOSTIC_TAG_DEPRECATED),
        message: bounded_lsp_text(&diagnostic.message, MAX_LSP_DIAGNOSTIC_MESSAGE_CHARS)
            .unwrap_or_else(|| "LSP diagnostic".to_owned()),
    })
}

fn diagnostic_from_lsp_value(value: &Value, path: &Path) -> Option<Diagnostic> {
    let (start, end) = parse_lsp_range_bounds(value.get("range")?)?;
    let line = one_based_lsp_position_component(start.line)?;
    let column = one_based_lsp_position_component(start.character)?;
    let severity = value
        .get("severity")
        .and_then(Value::as_u64)
        .and_then(|severity| u8::try_from(severity).ok());
    let source = value
        .get("source")
        .and_then(Value::as_str)
        .and_then(|source| bounded_lsp_text(source, MAX_LSP_DIAGNOSTIC_SOURCE_CHARS))
        .unwrap_or_else(|| "lsp".to_owned());
    let mut unused = false;
    let mut deprecated = false;
    if let Some(tags) = value.get("tags").and_then(Value::as_array) {
        for tag in tags {
            match tag.as_u64().and_then(|tag| u8::try_from(tag).ok()) {
                Some(LSP_DIAGNOSTIC_TAG_UNNECESSARY) => unused = true,
                Some(LSP_DIAGNOSTIC_TAG_DEPRECATED) => deprecated = true,
                _ => {}
            }
        }
    }
    let message = value
        .get("message")?
        .as_str()
        .and_then(|message| bounded_lsp_text(message, MAX_LSP_DIAGNOSTIC_MESSAGE_CHARS))
        .unwrap_or_else(|| "LSP diagnostic".to_owned());

    Some(Diagnostic {
        path: path.to_path_buf(),
        line,
        column,
        char_range: lsp_diagnostic_char_range(start, end, column),
        severity: lsp_severity(severity),
        source,
        unused,
        deprecated,
        message,
    })
}

fn lsp_diagnostic_severity(severity: DiagnosticSeverity) -> u8 {
    match severity {
        DiagnosticSeverity::Error => 1,
        DiagnosticSeverity::Warning => 2,
        DiagnosticSeverity::Info => 3,
        DiagnosticSeverity::Hint => 4,
    }
}

/// LSP omits `severity` on errors by convention and editors such as VS Code
/// render a missing severity as Error, so default to Error rather than Info.
fn lsp_severity(severity: Option<u8>) -> DiagnosticSeverity {
    match severity.unwrap_or(1) {
        1 => DiagnosticSeverity::Error,
        2 => DiagnosticSeverity::Warning,
        3 => DiagnosticSeverity::Info,
        _ => DiagnosticSeverity::Hint,
    }
}
