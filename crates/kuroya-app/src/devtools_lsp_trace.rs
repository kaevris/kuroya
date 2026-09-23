use crate::{
    KuroyaApp,
    devtools_trace_id::next_devtools_trace_id,
    lsp_progress::{
        LspProgressKey, LspProgressSummaryItem, MAX_VISIBLE_LSP_PROGRESS_ITEMS,
        active_lsp_progress_summary,
    },
    lsp_ui_events::LspUiEvent,
    path_display::{display_error_label_cow, display_path_label_cow},
    ui_text::truncate_middle,
};
use eframe::egui::{self, RichText};
use kuroya_core::{LspWorkDoneProgress, LspWorkDoneProgressKind, LspWorkspaceDocumentChange};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet, VecDeque},
    fmt::Write as _,
};

mod text;

use text::{
    MAX_LSP_TRACE_DETAIL_CHARS, MAX_LSP_TRACE_FIELD_CHARS, MAX_LSP_TRACE_LANGUAGE_CHARS,
    MAX_LSP_TRACE_METHOD_CHARS, MAX_LSP_TRACE_TOKEN_CHARS, lsp_trace_detail_label,
    lsp_trace_display_label, lsp_trace_entry_display_labels, lsp_trace_field_label,
    normalize_lsp_trace_text, normalize_lsp_trace_text_or,
};
#[cfg(test)]
use text::{bounded_lsp_trace_text, bounded_lsp_trace_text_or, is_lsp_trace_format_control};

pub(crate) const MAX_LSP_TRACE_ENTRIES: usize = 160;
const MAX_LSP_TRACE_DISPLAY_ROWS: usize = MAX_LSP_TRACE_ENTRIES;
const DEVTOOLS_MONOSPACE_ROW_HEIGHT: f32 = 18.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LspTraceDirection {
    Client,
    Server,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LspTraceEntry {
    pub(crate) id: u64,
    pub(crate) direction: LspTraceDirection,
    pub(crate) method: String,
    pub(crate) detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LspTraceStats {
    pub(crate) entry_count: usize,
    pub(crate) client_count: usize,
    pub(crate) server_count: usize,
    pub(crate) method_count: usize,
}

impl KuroyaApp {
    pub(crate) fn record_lsp_client_trace(
        &mut self,
        method: impl Into<String>,
        detail: impl Into<String>,
    ) {
        self.record_lsp_trace(LspTraceDirection::Client, method, detail);
    }

    pub(crate) fn record_lsp_ui_event_trace(&mut self, event: &LspUiEvent) {
        let (method, detail) = lsp_ui_event_trace_label(event);
        self.record_lsp_trace(LspTraceDirection::Server, method, detail);
    }

    fn record_lsp_trace(
        &mut self,
        direction: LspTraceDirection,
        method: impl Into<String>,
        detail: impl Into<String>,
    ) {
        let method = method.into();
        let detail = detail.into();
        let id = next_devtools_trace_id(&mut self.next_lsp_trace_id);
        if self.settings.devtools_verbose_logging {
            let mut message =
                String::with_capacity(direction.label().len() + 2 + method.len() + detail.len());
            let _ = write!(message, "{} {} {}", direction.label(), method, detail);
            self.record_verbose_log("lsp", message);
        }
        record_lsp_trace_entry(
            &mut self.lsp_trace,
            LspTraceEntry {
                id,
                direction,
                method,
                detail,
            },
            MAX_LSP_TRACE_ENTRIES,
        );
    }
}

pub(crate) fn record_lsp_trace_entry(
    entries: &mut VecDeque<LspTraceEntry>,
    mut entry: LspTraceEntry,
    max_entries: usize,
) {
    if max_entries == 0 {
        entries.clear();
        return;
    }
    normalize_lsp_trace_text_or(&mut entry.method, MAX_LSP_TRACE_METHOD_CHARS, "method");
    normalize_lsp_trace_text(&mut entry.detail, MAX_LSP_TRACE_DETAIL_CHARS);
    while entries.len() >= max_entries {
        entries.pop_front();
    }
    entries.push_back(entry);
}

pub(crate) fn lsp_trace_stats(entries: &VecDeque<LspTraceEntry>) -> Option<LspTraceStats> {
    if entries.is_empty() {
        return None;
    }

    let mut client_count = 0usize;
    let mut server_count = 0usize;
    let mut methods: HashSet<Cow<'_, str>> = HashSet::with_capacity(entries.len());
    for entry in entries {
        match entry.direction {
            LspTraceDirection::Client => client_count += 1,
            LspTraceDirection::Server => server_count += 1,
        }
        methods.insert(lsp_trace_display_label(
            &entry.method,
            MAX_LSP_TRACE_METHOD_CHARS,
        ));
    }

    Some(LspTraceStats {
        entry_count: entries.len(),
        client_count,
        server_count,
        method_count: methods.len(),
    })
}

pub(crate) fn lsp_ui_event_trace_label(event: &LspUiEvent) -> (&'static str, String) {
    let (method, detail) = match event {
        LspUiEvent::PrepareRenameResult {
            path,
            line,
            column,
            range,
            error,
            ..
        } => (
            "textDocument/prepareRename",
            error
                .as_deref()
                .map(|error| display_error_label_cow(error).into_owned())
                .unwrap_or_else(|| {
                    format!(
                        "{}:{}:{} ({})",
                        display_path_label_cow(path),
                        line + 1,
                        column + 1,
                        if range.is_some() {
                            "renamable"
                        } else {
                            "empty"
                        }
                    )
                }),
        ),
        LspUiEvent::ServerResult { target, event } => {
            let (method, detail) = lsp_ui_event_trace_label(event);
            (
                method,
                format!(
                    "{} #{} at {}; {}",
                    lsp_trace_field_label(&target.language, MAX_LSP_TRACE_LANGUAGE_CHARS, "LSP"),
                    target.generation,
                    display_path_label_cow(&target.root),
                    detail
                ),
            )
        }
        LspUiEvent::Diagnostics {
            path, diagnostics, ..
        } => (
            "textDocument/publishDiagnostics",
            format!(
                "{} ({})",
                display_path_label_cow(path),
                item_count(diagnostics.len(), "diagnostic")
            ),
        ),
        LspUiEvent::BufferSynced { path, version, .. } => (
            "kuroya/bufferSynced",
            format!("{} v{version}", display_path_label_cow(path)),
        ),
        LspUiEvent::HoverResult {
            path,
            line,
            column,
            contents,
            ..
        } => (
            "textDocument/hover",
            format!(
                "{}:{}:{} ({})",
                display_path_label_cow(path),
                line + 1,
                column + 1,
                if contents.is_some() { "hit" } else { "empty" }
            ),
        ),
        LspUiEvent::DocumentHighlightsResult {
            path,
            line,
            column,
            highlights,
            error,
            ..
        } => (
            "textDocument/documentHighlight",
            result_detail(
                path,
                *line,
                *column,
                highlights.as_ref().map(Vec::len),
                "highlight",
                error.as_deref(),
            ),
        ),
        LspUiEvent::DefinitionResult {
            origin_path,
            origin_line,
            origin_column,
            definition,
            error,
            ..
        } => (
            "textDocument/definition",
            error
                .as_deref()
                .map(|error| display_error_label_cow(error).into_owned())
                .unwrap_or_else(|| {
                    format!(
                        "{}:{}:{} ({})",
                        display_path_label_cow(origin_path),
                        origin_line + 1,
                        origin_column + 1,
                        if definition.is_some() { "hit" } else { "empty" }
                    )
                }),
        ),
        LspUiEvent::CallHierarchyPrepared {
            path,
            line,
            column,
            items,
            error,
            ..
        } => (
            "textDocument/prepareCallHierarchy",
            result_detail(
                path,
                *line,
                *column,
                items.as_ref().map(Vec::len),
                "item",
                error.as_deref(),
            ),
        ),
        LspUiEvent::CallHierarchyIncomingResult {
            path, calls, error, ..
        } => (
            "callHierarchy/incomingCalls",
            path_result_detail(path, calls.as_ref().map(Vec::len), "call", error.as_deref()),
        ),
        LspUiEvent::CallHierarchyOutgoingResult {
            path, calls, error, ..
        } => (
            "callHierarchy/outgoingCalls",
            path_result_detail(path, calls.as_ref().map(Vec::len), "call", error.as_deref()),
        ),
        LspUiEvent::TypeHierarchyPrepared {
            path,
            line,
            column,
            items,
            error,
            ..
        } => (
            "textDocument/prepareTypeHierarchy",
            result_detail(
                path,
                *line,
                *column,
                items.as_ref().map(Vec::len),
                "item",
                error.as_deref(),
            ),
        ),
        LspUiEvent::TypeHierarchySupertypesResult {
            path,
            supertypes,
            error,
            ..
        } => (
            "typeHierarchy/supertypes",
            path_result_detail(
                path,
                supertypes.as_ref().map(Vec::len),
                "type",
                error.as_deref(),
            ),
        ),
        LspUiEvent::TypeHierarchySubtypesResult {
            path,
            subtypes,
            error,
            ..
        } => (
            "typeHierarchy/subtypes",
            path_result_detail(
                path,
                subtypes.as_ref().map(Vec::len),
                "type",
                error.as_deref(),
            ),
        ),
        LspUiEvent::ReferencesResult {
            path,
            line,
            column,
            references,
            error,
            ..
        } => (
            "textDocument/references",
            result_detail(
                path,
                *line,
                *column,
                references.as_ref().map(Vec::len),
                "reference",
                error.as_deref(),
            ),
        ),
        LspUiEvent::RenameResult {
            origin_path,
            origin_line,
            origin_column,
            edits,
            error,
            ..
        } => (
            "textDocument/rename",
            result_detail(
                origin_path,
                *origin_line,
                *origin_column,
                edits.as_ref().map(Vec::len),
                "edit",
                error.as_deref(),
            ),
        ),
        LspUiEvent::DocumentSymbolsResult { path, symbols, .. } => (
            "textDocument/documentSymbol",
            format!(
                "{} ({})",
                display_path_label_cow(path),
                item_count(symbols.as_ref().map(Vec::len).unwrap_or(0), "symbol")
            ),
        ),
        LspUiEvent::FoldingRangesResult {
            path,
            ranges,
            error,
            ..
        } => (
            "textDocument/foldingRange",
            path_result_detail(
                path,
                ranges.as_ref().map(Vec::len),
                "range",
                error.as_deref(),
            ),
        ),
        LspUiEvent::InlayHintsResult {
            path, hints, error, ..
        } => (
            "textDocument/inlayHint",
            path_result_detail(path, hints.as_ref().map(Vec::len), "hint", error.as_deref()),
        ),
        LspUiEvent::CodeLensesResult {
            path,
            lenses,
            error,
            ..
        } => (
            "textDocument/codeLens",
            path_result_detail(
                path,
                lenses.as_ref().map(Vec::len),
                "lens",
                error.as_deref(),
            ),
        ),
        LspUiEvent::CodeLensResolveResult {
            path, lens, error, ..
        } => (
            "codeLens/resolve",
            path_result_detail(path, lens.as_ref().map(|_| 1), "lens", error.as_deref()),
        ),
        LspUiEvent::CodeLensCommandResult {
            path, title, error, ..
        } => (
            "workspace/executeCommand",
            if let Some(error) = error {
                format!(
                    "{} `{}` failed: {}",
                    display_path_label_cow(path),
                    lsp_trace_field_label(title, MAX_LSP_TRACE_FIELD_CHARS, "command"),
                    display_error_label_cow(error)
                )
            } else {
                format!(
                    "{} `{}`",
                    display_path_label_cow(path),
                    lsp_trace_field_label(title, MAX_LSP_TRACE_FIELD_CHARS, "command")
                )
            },
        ),
        LspUiEvent::SemanticTokensResult {
            path,
            tokens,
            error,
            ..
        } => (
            "textDocument/semanticTokens/full",
            path_result_detail(
                path,
                tokens.as_ref().map(Vec::len),
                "token",
                error.as_deref(),
            ),
        ),
        LspUiEvent::WorkspaceSymbolsResult {
            query,
            symbols,
            error,
            ..
        } => (
            "workspace/symbol",
            query_result_detail(
                query,
                symbols.as_ref().map(Vec::len),
                "symbol",
                error.as_deref(),
            ),
        ),
        LspUiEvent::CompletionResult {
            path,
            line,
            column,
            items,
            error,
            ..
        } => (
            "textDocument/completion",
            result_detail(
                path,
                *line,
                *column,
                items.as_ref().map(Vec::len),
                "item",
                error.as_deref(),
            ),
        ),
        LspUiEvent::CompletionItemResolveResult {
            path,
            line,
            column,
            item,
            error,
            ..
        } => (
            "completionItem/resolve",
            result_detail(
                path,
                *line,
                *column,
                item.as_ref().map(|_| 1),
                "item",
                error.as_deref(),
            ),
        ),
        LspUiEvent::SignatureHelpResult {
            path,
            line,
            column,
            help,
            error,
            ..
        } => (
            "textDocument/signatureHelp",
            result_detail(
                path,
                *line,
                *column,
                help.as_ref().map(|help| help.signatures.len()),
                "signature",
                error.as_deref(),
            ),
        ),
        LspUiEvent::FormattingResult {
            path, edits, error, ..
        } => (
            "textDocument/formatting",
            path_result_detail(path, edits.as_ref().map(Vec::len), "edit", error.as_deref()),
        ),
        LspUiEvent::CodeActionsResult {
            path,
            line,
            column,
            actions,
            error,
            ..
        } => (
            "textDocument/codeAction",
            result_detail(
                path,
                *line,
                *column,
                actions.as_ref().map(Vec::len),
                "action",
                error.as_deref(),
            ),
        ),
        LspUiEvent::CodeActionResolveResult {
            path,
            line,
            column,
            action,
            error,
            ..
        } => (
            "codeAction/resolve",
            result_detail(
                path,
                *line,
                *column,
                action.as_ref().map(|_| 1),
                "action",
                error.as_deref(),
            ),
        ),
        LspUiEvent::WorkspaceApplyEditRequest {
            label,
            edits,
            document_changes,
            error,
            ..
        } => (
            "workspace/applyEdit",
            match error {
                Some(error) => format!("request failed: {}", display_error_label_cow(error)),
                None => format!(
                    "{} ({}, {})",
                    lsp_trace_field_label(
                        label.as_deref().unwrap_or("LSP workspace edit"),
                        MAX_LSP_TRACE_FIELD_CHARS,
                        "LSP workspace edit"
                    ),
                    item_count(edits.as_ref().map(Vec::len).unwrap_or(0), "edit"),
                    item_count(
                        workspace_document_change_resource_count(document_changes),
                        "resource operation"
                    )
                ),
            },
        ),
        LspUiEvent::WorkspaceEditFilesApplied {
            changed, failed, ..
        } => (
            "workspace/applyEdit",
            format!(
                "{}; {}",
                item_count(*changed, "file changed"),
                item_count(failed.len(), "failure")
            ),
        ),
        LspUiEvent::WorkDoneProgressCreated { token } => (
            "window/workDoneProgress/create",
            format!(
                "token {}",
                lsp_trace_field_label(token, MAX_LSP_TRACE_TOKEN_CHARS, "unknown")
            ),
        ),
        LspUiEvent::WorkDoneProgress { progress, .. } => {
            ("$/progress", work_done_progress_detail(progress))
        }
        LspUiEvent::ServerReady {
            language,
            root,
            generation,
        } => (
            "$/serverReady",
            format!(
                "{} #{generation} at {}",
                lsp_trace_field_label(language, MAX_LSP_TRACE_LANGUAGE_CHARS, "LSP"),
                display_path_label_cow(root)
            ),
        ),
        LspUiEvent::ServerStopped {
            language,
            root,
            generation,
        } => (
            "$/serverStopped",
            format!(
                "{} #{generation} at {}",
                lsp_trace_field_label(language, MAX_LSP_TRACE_LANGUAGE_CHARS, "LSP"),
                display_path_label_cow(root)
            ),
        ),
        LspUiEvent::ServerUnavailable {
            language,
            root,
            generation,
        } => (
            "$/serverUnavailable",
            format!(
                "{} #{generation} at {}",
                lsp_trace_field_label(language, MAX_LSP_TRACE_LANGUAGE_CHARS, "LSP"),
                display_path_label_cow(root)
            ),
        ),
        LspUiEvent::Status {
            root,
            generation,
            message,
            ..
        } => (
            "$/status",
            format!(
                "{} #{generation} at {}",
                lsp_trace_field_label(message, MAX_LSP_TRACE_FIELD_CHARS, "status"),
                display_path_label_cow(root)
            ),
        ),
    };
    (method, lsp_trace_detail_label(detail))
}

fn work_done_progress_detail(progress: &LspWorkDoneProgress) -> String {
    let kind = match progress.kind {
        LspWorkDoneProgressKind::Begin => "begin",
        LspWorkDoneProgressKind::Report => "report",
        LspWorkDoneProgressKind::End => "end",
    };
    let label = progress
        .title
        .as_deref()
        .or(progress.message.as_deref())
        .unwrap_or(&progress.token);
    let label = lsp_trace_field_label(label, MAX_LSP_TRACE_FIELD_CHARS, "LSP task");
    let mut detail = String::with_capacity(kind.len() + 1 + label.len() + 5);
    let _ = write!(detail, "{kind} {label}");
    if let Some(percentage) = progress.percentage {
        let _ = write!(detail, " {percentage}%");
    }
    detail
}

fn render_lsp_progress_panel(ui: &mut egui::Ui, titles: &HashMap<LspProgressKey, String>) {
    ui.label(RichText::new("LSP Progress").strong());
    if titles.is_empty() {
        ui.label(RichText::new("Idle").small());
        return;
    }
    let summary = active_lsp_progress_summary(titles, MAX_VISIBLE_LSP_PROGRESS_ITEMS);
    if summary.active_count == 0 {
        ui.label(RichText::new("Idle").small());
        return;
    }

    ui.horizontal(|ui| {
        ui.label(lsp_trace_plural_count_label(
            summary.active_count,
            "active task",
            "active tasks",
        ));
        let hidden_count = summary.hidden_count();
        if hidden_count > 0 {
            ui.label(lsp_trace_count_label_with_parts("+", hidden_count, " more"));
        }
    });

    for item in summary.items {
        ui.monospace(truncate_middle(&item.title, 72))
            .on_hover_ui(|ui| {
                ui.set_max_width(ui.spacing().tooltip_width);
                ui.label(lsp_progress_item_tooltip(&item));
            });
    }
}

fn lsp_progress_item_tooltip(item: &LspProgressSummaryItem) -> String {
    let language = lsp_trace_field_label(&item.language, MAX_LSP_TRACE_LANGUAGE_CHARS, "LSP");
    let root = display_path_label_cow(&item.root);
    let token = lsp_trace_field_label(&item.token, MAX_LSP_TRACE_TOKEN_CHARS, "unknown");
    let mut tooltip = String::with_capacity(language.len() + root.len() + token.len() + 24);
    let _ = write!(
        tooltip,
        "{} #{} at {}\ntoken {}",
        language, item.generation, root, token
    );
    tooltip
}

pub(crate) fn render_lsp_trace_panel(
    ui: &mut egui::Ui,
    entries: &VecDeque<LspTraceEntry>,
    titles: &HashMap<LspProgressKey, String>,
) {
    render_lsp_progress_panel(ui, titles);
    ui.separator();

    ui.label(RichText::new("LSP Trace").strong());
    if entries.is_empty() {
        ui.label(RichText::new("No LSP events recorded yet").small());
        return;
    }
    if let Some(stats) = lsp_trace_stats(entries) {
        ui.horizontal(|ui| {
            ui.label(lsp_trace_plural_count_label(
                stats.entry_count,
                "event",
                "events",
            ));
            ui.label(lsp_trace_count_label(stats.client_count, "client"));
            ui.label(lsp_trace_count_label(stats.server_count, "server"));
            ui.label(lsp_trace_plural_count_label(
                stats.method_count,
                "method",
                "methods",
            ));
        });
    }

    let row_height = devtools_row_height(ui);
    let display_row_count = lsp_trace_display_row_count(entries);
    egui::ScrollArea::vertical()
        .max_height(220.0)
        .auto_shrink([false, false])
        .show_rows(ui, row_height, display_row_count, |ui, rows| {
            for display_index in rows {
                let Some(entry) = lsp_trace_visible_row(entries, display_row_count, display_index)
                else {
                    continue;
                };
                ui.monospace(lsp_trace_row_label(entry));
            }
        });
}

fn lsp_trace_row_label(entry: &LspTraceEntry) -> String {
    let labels = lsp_trace_entry_display_labels(entry);
    let mut row = String::with_capacity(16 + labels.method.len() + labels.detail.len());
    let _ = write!(
        row,
        "#{:04} {} {} {}",
        entry.id,
        entry.direction.label(),
        labels.method,
        labels.detail
    );
    row
}

fn devtools_row_height(ui: &egui::Ui) -> f32 {
    ui.spacing()
        .interact_size
        .y
        .max(DEVTOOLS_MONOSPACE_ROW_HEIGHT)
}

fn lsp_trace_display_row_count(entries: &VecDeque<LspTraceEntry>) -> usize {
    entries.len().min(MAX_LSP_TRACE_DISPLAY_ROWS)
}

fn lsp_trace_visible_row(
    entries: &VecDeque<LspTraceEntry>,
    display_row_count: usize,
    display_index: usize,
) -> Option<&LspTraceEntry> {
    let display_row_count = display_row_count
        .min(entries.len())
        .min(MAX_LSP_TRACE_DISPLAY_ROWS);
    if display_index >= display_row_count {
        return None;
    }

    entries
        .len()
        .checked_sub(display_index + 1)
        .and_then(|index| entries.get(index))
}

fn result_detail(
    path: &std::path::Path,
    line: usize,
    column: usize,
    count: Option<usize>,
    item: &str,
    error: Option<&str>,
) -> String {
    let path = display_path_label_cow(path);
    if let Some(error) = error {
        return format!(
            "{}:{}:{} ({})",
            path,
            line + 1,
            column + 1,
            display_error_label_cow(error)
        );
    }
    format!(
        "{}:{}:{} ({})",
        path,
        line + 1,
        column + 1,
        item_count(count.unwrap_or(0), item)
    )
}

fn path_result_detail(
    path: &std::path::Path,
    count: Option<usize>,
    item: &str,
    error: Option<&str>,
) -> String {
    let path = display_path_label_cow(path);
    if let Some(error) = error {
        return format!("{} ({})", path, display_error_label_cow(error));
    }
    format!("{} ({})", path, item_count(count.unwrap_or(0), item))
}

fn query_result_detail(
    query: &str,
    count: Option<usize>,
    item: &str,
    error: Option<&str>,
) -> String {
    let query = lsp_trace_field_label(query, MAX_LSP_TRACE_FIELD_CHARS, "query");
    if let Some(error) = error {
        return format!("`{query}` ({})", display_error_label_cow(error));
    }
    format!("`{query}` ({})", item_count(count.unwrap_or(0), item))
}

fn item_count(count: usize, singular: &str) -> String {
    let mut label = String::with_capacity(
        lsp_trace_count_label_capacity(count, singular) + usize::from(count != 1),
    );
    push_lsp_trace_count_label(&mut label, count, singular);
    if count != 1 {
        label.push('s');
    }
    label
}

fn lsp_trace_count_label(count: usize, noun: &str) -> String {
    let mut label = String::with_capacity(lsp_trace_count_label_capacity(count, noun));
    push_lsp_trace_count_label(&mut label, count, noun);
    label
}

fn lsp_trace_plural_count_label(count: usize, singular: &str, plural: &str) -> String {
    lsp_trace_count_label(count, if count == 1 { singular } else { plural })
}

fn lsp_trace_count_label_with_parts(prefix: &str, count: usize, suffix: &str) -> String {
    let mut label =
        String::with_capacity(prefix.len() + lsp_trace_decimal_digit_count(count) + suffix.len());
    label.push_str(prefix);
    push_lsp_trace_usize_decimal(&mut label, count);
    label.push_str(suffix);
    label
}

fn push_lsp_trace_count_label(label: &mut String, count: usize, noun: &str) {
    push_lsp_trace_usize_decimal(label, count);
    label.push(' ');
    label.push_str(noun);
}

fn lsp_trace_count_label_capacity(count: usize, noun: &str) -> usize {
    lsp_trace_decimal_digit_count(count) + 1 + noun.len()
}

fn lsp_trace_decimal_digit_count(mut value: usize) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

fn push_lsp_trace_usize_decimal(output: &mut String, mut value: usize) {
    const MAX_USIZE_DECIMAL_DIGITS: usize = 39;

    let mut digits = [0_u8; MAX_USIZE_DECIMAL_DIGITS];
    let mut digit_count = 0;
    loop {
        digits[digit_count] = b'0' + (value % 10) as u8;
        digit_count += 1;
        value /= 10;
        if value == 0 {
            break;
        }
    }

    for digit in digits[..digit_count].iter().rev() {
        output.push(*digit as char);
    }
}

fn workspace_document_change_resource_count(changes: &[LspWorkspaceDocumentChange]) -> usize {
    changes
        .iter()
        .filter(|change| matches!(change, LspWorkspaceDocumentChange::Resource(_)))
        .count()
}

impl LspTraceDirection {
    fn label(self) -> &'static str {
        match self {
            Self::Client => "C -> S",
            Self::Server => "S -> C",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LspTraceDirection, LspTraceEntry, LspTraceStats, MAX_LSP_TRACE_DETAIL_CHARS,
        MAX_LSP_TRACE_DISPLAY_ROWS, MAX_LSP_TRACE_METHOD_CHARS, MAX_LSP_TRACE_TOKEN_CHARS,
        bounded_lsp_trace_text, bounded_lsp_trace_text_or, is_lsp_trace_format_control, item_count,
        lsp_progress_item_tooltip, lsp_trace_count_label, lsp_trace_count_label_with_parts,
        lsp_trace_display_label, lsp_trace_display_row_count, lsp_trace_plural_count_label,
        lsp_trace_row_label, lsp_trace_stats, lsp_trace_visible_row, lsp_ui_event_trace_label,
        record_lsp_trace_entry,
    };
    use crate::lsp_progress::LspProgressSummaryItem;
    use crate::lsp_ui_events::LspUiEvent;
    use kuroya_core::{Diagnostic, DiagnosticSeverity};
    use std::borrow::Cow;
    use std::collections::{BTreeMap, VecDeque};
    use std::path::PathBuf;

    fn assert_trace_detail_is_safe_and_bounded(detail: &str) {
        assert_trace_label_is_safe_and_bounded(detail, MAX_LSP_TRACE_DETAIL_CHARS);
    }

    fn assert_trace_label_is_safe_and_bounded(label: &str, max_chars: usize) {
        assert!(
            label.chars().count() <= max_chars,
            "trace label should be bounded: {label:?}"
        );
        assert!(
            !label.chars().any(char::is_control),
            "trace label should not contain control characters: {label:?}"
        );
        assert!(
            !label.chars().any(is_lsp_trace_format_control),
            "trace label should not contain bidi controls: {label:?}"
        );
    }

    fn test_trace_entry(id: u64) -> LspTraceEntry {
        LspTraceEntry {
            id,
            direction: LspTraceDirection::Client,
            method: format!("method/{id}"),
            detail: format!("detail {id}"),
        }
    }

    #[test]
    fn lsp_trace_display_label_borrows_clean_labels() {
        let ascii = "textDocument/hover";
        let unicode = "workspace/symbole-\u{00e9}";

        assert!(matches!(
            lsp_trace_display_label(ascii, MAX_LSP_TRACE_METHOD_CHARS),
            Cow::Borrowed(label) if label == ascii
        ));
        assert!(matches!(
            lsp_trace_display_label(unicode, MAX_LSP_TRACE_METHOD_CHARS),
            Cow::Borrowed(label) if label == unicode
        ));
    }

    #[test]
    fn lsp_trace_display_label_keeps_trace_format_control_policy() {
        let label = lsp_trace_display_label("alpha\u{034f}beta", MAX_LSP_TRACE_METHOD_CHARS);

        assert!(matches!(label, Cow::Owned(_)));
        assert_eq!(label.as_ref(), "alphabeta");
        assert_eq!(
            bounded_lsp_trace_text("ready\u{034f}", MAX_LSP_TRACE_DETAIL_CHARS),
            "ready"
        );
        assert_eq!(
            bounded_lsp_trace_text_or("\u{034f}", MAX_LSP_TRACE_METHOD_CHARS, "method"),
            "method"
        );
    }

    #[test]
    fn lsp_trace_entries_are_bounded() {
        let mut entries = VecDeque::new();
        record_lsp_trace_entry(
            &mut entries,
            LspTraceEntry {
                id: 1,
                direction: LspTraceDirection::Client,
                method: "one".to_owned(),
                detail: "a".to_owned(),
            },
            2,
        );
        record_lsp_trace_entry(
            &mut entries,
            LspTraceEntry {
                id: 2,
                direction: LspTraceDirection::Server,
                method: "two".to_owned(),
                detail: "b".to_owned(),
            },
            2,
        );
        record_lsp_trace_entry(
            &mut entries,
            LspTraceEntry {
                id: 3,
                direction: LspTraceDirection::Server,
                method: "three".to_owned(),
                detail: "c".to_owned(),
            },
            2,
        );

        assert_eq!(
            entries
                .iter()
                .map(|entry| (entry.id, entry.method.as_str(), entry.detail.as_str()))
                .collect::<Vec<_>>(),
            vec![(2, "two", "b"), (3, "three", "c")]
        );
    }

    #[test]
    fn lsp_trace_entries_sanitize_and_bound_payload_text() {
        let mut entries = VecDeque::new();
        let raw_method = format!(
            "textDocument/\n{}\u{0007}\u{202e}",
            "m".repeat(MAX_LSP_TRACE_METHOD_CHARS + 16)
        );
        let long_detail = format!("start {} end", "x".repeat(MAX_LSP_TRACE_DETAIL_CHARS + 32));
        let raw_detail = format!("alpha\r\n\tbeta\u{2066} {long_detail}");
        record_lsp_trace_entry(
            &mut entries,
            LspTraceEntry {
                id: 1,
                direction: LspTraceDirection::Client,
                method: raw_method.clone(),
                detail: raw_detail.clone(),
            },
            8,
        );

        let entry = entries.front().expect("trace entry should be stored");
        assert_trace_label_is_safe_and_bounded(&entry.method, MAX_LSP_TRACE_METHOD_CHARS);
        assert_trace_detail_is_safe_and_bounded(&entry.detail);
        assert_ne!(entry.method, raw_method);
        assert_ne!(entry.detail, raw_detail);
        assert!(entry.method.starts_with("textDocument/"));
        assert!(entry.method.contains("..."));
        assert!(entry.detail.contains("alpha beta"));
        assert!(entry.detail.contains("..."));
    }

    #[test]
    fn lsp_trace_row_label_sanitizes_and_caps_raw_payload_text() {
        let raw_method = format!(
            "textDocument/\n{}\u{0007}\u{202e}",
            "m".repeat(MAX_LSP_TRACE_METHOD_CHARS + 16)
        );
        let raw_detail = format!(
            "alpha\r\n\tbeta\u{2066} start {} end",
            "x".repeat(MAX_LSP_TRACE_DETAIL_CHARS + 32)
        );
        let entry = LspTraceEntry {
            id: 1,
            direction: LspTraceDirection::Client,
            method: raw_method.clone(),
            detail: raw_detail.clone(),
        };

        let row = lsp_trace_row_label(&entry);

        assert_trace_label_is_safe_and_bounded(
            &row,
            32 + MAX_LSP_TRACE_METHOD_CHARS + MAX_LSP_TRACE_DETAIL_CHARS,
        );
        assert!(row.starts_with("#0001 C -> S textDocument/"));
        assert!(row.contains("alpha beta"));
        assert!(row.contains("..."));
        assert_eq!(entry.method, raw_method);
        assert_eq!(entry.detail, raw_detail);
    }

    #[test]
    fn lsp_trace_row_label_sanitizes_short_hidden_controls() {
        let entry = LspTraceEntry {
            id: 1,
            direction: LspTraceDirection::Server,
            method: "textDocument/\u{200b}hover".to_owned(),
            detail: "ready\u{061c}\u{2060}\u{feff}".to_owned(),
        };

        let row = lsp_trace_row_label(&entry);

        assert_trace_label_is_safe_and_bounded(
            &row,
            32 + MAX_LSP_TRACE_METHOD_CHARS + MAX_LSP_TRACE_DETAIL_CHARS,
        );
        assert!(row.contains("textDocument/hover"));
        assert!(row.contains("ready"));
    }

    #[test]
    fn lsp_trace_display_rows_are_bounded_to_newest_entries() {
        let total = MAX_LSP_TRACE_DISPLAY_ROWS + 3;
        let entries = (1..=total)
            .map(|id| test_trace_entry(id as u64))
            .collect::<VecDeque<_>>();

        let display_row_count = lsp_trace_display_row_count(&entries);

        assert_eq!(display_row_count, MAX_LSP_TRACE_DISPLAY_ROWS);
        assert_eq!(
            lsp_trace_visible_row(&entries, display_row_count, 0).map(|entry| entry.id),
            Some(total as u64)
        );
        assert_eq!(
            lsp_trace_visible_row(&entries, display_row_count, display_row_count - 1)
                .map(|entry| entry.id),
            Some(4)
        );
        assert_eq!(
            lsp_trace_visible_row(&entries, display_row_count, display_row_count)
                .map(|entry| entry.id),
            None
        );
        assert_eq!(
            lsp_trace_visible_row(&entries, entries.len(), MAX_LSP_TRACE_DISPLAY_ROWS)
                .map(|entry| entry.id),
            None
        );
    }

    #[test]
    fn lsp_trace_visible_row_rejects_stale_display_indexes() {
        let entries = VecDeque::from([test_trace_entry(1), test_trace_entry(2)]);

        assert_eq!(
            lsp_trace_visible_row(&entries, 8, 0).map(|entry| entry.id),
            Some(2)
        );
        assert_eq!(
            lsp_trace_visible_row(&entries, 8, 1).map(|entry| entry.id),
            Some(1)
        );
        assert_eq!(
            lsp_trace_visible_row(&entries, 8, 2).map(|entry| entry.id),
            None
        );
        assert_eq!(
            lsp_trace_visible_row(&entries, 8, usize::MAX).map(|entry| entry.id),
            None
        );
        assert_eq!(
            lsp_trace_visible_row(&entries, 0, 0).map(|entry| entry.id),
            None
        );
    }

    #[test]
    fn lsp_trace_stats_summarize_directions_and_methods() {
        let entries = VecDeque::from([
            LspTraceEntry {
                id: 1,
                direction: LspTraceDirection::Client,
                method: "textDocument/didOpen".to_owned(),
                detail: "main.rs".to_owned(),
            },
            LspTraceEntry {
                id: 2,
                direction: LspTraceDirection::Server,
                method: "textDocument/publishDiagnostics".to_owned(),
                detail: "main.rs (1 diagnostic)".to_owned(),
            },
            LspTraceEntry {
                id: 3,
                direction: LspTraceDirection::Client,
                method: "textDocument/didOpen".to_owned(),
                detail: "lib.rs".to_owned(),
            },
        ]);

        assert_eq!(
            lsp_trace_stats(&entries),
            Some(LspTraceStats {
                entry_count: 3,
                client_count: 2,
                server_count: 1,
                method_count: 2,
            })
        );
    }

    #[test]
    fn lsp_trace_stats_are_empty_without_events() {
        assert_eq!(lsp_trace_stats(&VecDeque::new()), None);
    }

    #[test]
    fn lsp_trace_count_labels_preserve_display_text() {
        assert_eq!(
            lsp_trace_plural_count_label(0, "event", "events"),
            "0 events"
        );
        assert_eq!(
            lsp_trace_plural_count_label(1, "event", "events"),
            "1 event"
        );
        assert_eq!(
            lsp_trace_plural_count_label(2, "event", "events"),
            "2 events"
        );
        assert_eq!(lsp_trace_count_label(12_345, "client"), "12345 client");
        assert_eq!(
            lsp_trace_count_label_with_parts("+", 12_345, " more"),
            "+12345 more"
        );
        assert_eq!(
            lsp_trace_count_label(usize::MAX, "server"),
            format!("{} server", usize::MAX)
        );
    }

    #[test]
    fn lsp_trace_item_count_preserves_suffix_pluralization() {
        assert_eq!(item_count(0, "diagnostic"), "0 diagnostics");
        assert_eq!(item_count(1, "diagnostic"), "1 diagnostic");
        assert_eq!(item_count(2, "diagnostic"), "2 diagnostics");
        assert_eq!(item_count(2, "file changed"), "2 file changeds");
        assert_eq!(
            item_count(usize::MAX, "token"),
            format!("{} tokens", usize::MAX)
        );
    }

    #[test]
    fn lsp_progress_item_tooltip_sanitizes_server_identity_and_token() {
        let raw_language = "rust\nanalyzer\u{202e}".to_owned();
        let raw_token = format!(
            "token\n{}\u{2066}",
            "x".repeat(MAX_LSP_TRACE_TOKEN_CHARS + 16)
        );
        let item = LspProgressSummaryItem {
            language: raw_language.clone(),
            root: PathBuf::from("workspace"),
            generation: 42,
            token: raw_token.clone(),
            title: "Indexing".to_owned(),
        };

        let tooltip = lsp_progress_item_tooltip(&item);

        assert!(tooltip.starts_with("rust analyzer #42 at workspace\ntoken token"));
        assert!(tooltip.contains("..."));
        assert!(
            !tooltip.chars().any(|ch| ch != '\n' && ch.is_control()),
            "tooltip should only contain the expected line break: {tooltip:?}"
        );
        assert!(
            !tooltip.chars().any(is_lsp_trace_format_control),
            "tooltip should not contain bidi controls: {tooltip:?}"
        );
        assert_eq!(item.language, raw_language);
        assert_eq!(item.token, raw_token);
    }

    #[test]
    fn lsp_ui_event_trace_labels_summarize_payloads() {
        let event = LspUiEvent::Diagnostics {
            language: "rust".to_owned(),
            root: PathBuf::from("workspace"),
            generation: 1,
            path: PathBuf::from("src/main.rs"),
            version: Some(12),
            diagnostics: vec![
                Diagnostic {
                    path: PathBuf::from("src/main.rs"),
                    line: 1,
                    column: 2,
                    char_range: 0..1,
                    severity: DiagnosticSeverity::Error,
                    source: "rust-analyzer".to_owned(),
                    message: "first".to_owned(),
                    unused: false,
                    deprecated: false,
                },
                Diagnostic {
                    path: PathBuf::from("src/main.rs"),
                    line: 3,
                    column: 4,
                    char_range: 2..3,
                    severity: DiagnosticSeverity::Warning,
                    source: "rust-analyzer".to_owned(),
                    message: "second".to_owned(),
                    unused: false,
                    deprecated: false,
                },
            ],
        };

        assert_eq!(
            lsp_ui_event_trace_label(&event),
            (
                "textDocument/publishDiagnostics",
                "main.rs (2 diagnostics)".to_owned()
            )
        );
    }

    #[test]
    fn lsp_ui_event_trace_label_sanitizes_code_lens_command_strings() {
        let raw_title = format!(
            "Run\nTests \u{202e}{}",
            "t".repeat(MAX_LSP_TRACE_DETAIL_CHARS)
        );
        let raw_error = format!(
            "failed\r\nbecause \u{2066}{}",
            "e".repeat(MAX_LSP_TRACE_DETAIL_CHARS)
        );
        let event = LspUiEvent::CodeLensCommandResult {
            id: 1,
            path: PathBuf::from("src").join(format!(
                "main\n{}\u{202e}.rs",
                "p".repeat(MAX_LSP_TRACE_DETAIL_CHARS)
            )),
            version: 4,
            title: raw_title.clone(),
            command: "server.command".to_owned(),
            error: Some(raw_error.clone()),
        };

        let (method, detail) = lsp_ui_event_trace_label(&event);

        assert_eq!(method, "workspace/executeCommand");
        assert_trace_detail_is_safe_and_bounded(&detail);
        assert!(detail.contains("Run Tests"));
        assert!(detail.contains("failed:"));
        assert!(detail.contains("..."));
        if let LspUiEvent::CodeLensCommandResult { title, error, .. } = &event {
            assert_eq!(title, &raw_title);
            assert_eq!(error.as_deref(), Some(raw_error.as_str()));
        }
    }

    #[test]
    fn lsp_ui_event_trace_label_sanitizes_workspace_apply_edit_label_and_error() {
        let raw_label = format!(
            "Apply\nWorkspace Edit \u{202e}{}",
            "l".repeat(MAX_LSP_TRACE_DETAIL_CHARS)
        );
        let event = LspUiEvent::WorkspaceApplyEditRequest {
            language: "rust".to_owned(),
            root: PathBuf::from("workspace"),
            generation: 7,
            request_id: 11_u64.into(),
            label: Some(raw_label.clone()),
            edits: None,
            document_changes: Vec::new(),
            document_versions: BTreeMap::new(),
            error: None,
        };

        let (method, detail) = lsp_ui_event_trace_label(&event);

        assert_eq!(method, "workspace/applyEdit");
        assert_trace_detail_is_safe_and_bounded(&detail);
        assert!(detail.contains("Apply Workspace Edit"));
        assert!(detail.contains("0 edits"));
        assert!(detail.contains("..."));
        if let LspUiEvent::WorkspaceApplyEditRequest { label, .. } = &event {
            assert_eq!(label.as_deref(), Some(raw_label.as_str()));
        }

        let raw_error = format!(
            "request\nrejected \u{202e}{}",
            "e".repeat(MAX_LSP_TRACE_DETAIL_CHARS)
        );
        let error_event = LspUiEvent::WorkspaceApplyEditRequest {
            language: "rust".to_owned(),
            root: PathBuf::from("workspace"),
            generation: 7,
            request_id: 12_u64.into(),
            label: Some("unused".to_owned()),
            edits: None,
            document_changes: Vec::new(),
            document_versions: BTreeMap::new(),
            error: Some(raw_error.clone()),
        };

        let (_, error_detail) = lsp_ui_event_trace_label(&error_event);

        assert_trace_detail_is_safe_and_bounded(&error_detail);
        assert!(error_detail.starts_with("request failed: request rejected"));
        assert!(error_detail.contains("..."));
        if let LspUiEvent::WorkspaceApplyEditRequest { error, .. } = &error_event {
            assert_eq!(error.as_deref(), Some(raw_error.as_str()));
        }
    }

    #[test]
    fn lsp_ui_event_trace_label_sanitizes_progress_token_and_server_identity() {
        let raw_token = format!("token\n{}\u{2066}", "x".repeat(MAX_LSP_TRACE_DETAIL_CHARS));
        let event = LspUiEvent::WorkDoneProgressCreated {
            token: raw_token.clone(),
        };

        let (method, detail) = lsp_ui_event_trace_label(&event);

        assert_eq!(method, "window/workDoneProgress/create");
        assert_trace_detail_is_safe_and_bounded(&detail);
        assert!(detail.starts_with("token token"));
        assert!(detail.contains("..."));
        if let LspUiEvent::WorkDoneProgressCreated { token } = &event {
            assert_eq!(token, &raw_token);
        }

        let raw_language = format!(
            "rust\nanalyzer \u{202e}{}",
            "l".repeat(MAX_LSP_TRACE_DETAIL_CHARS)
        );
        let server_event = LspUiEvent::ServerReady {
            language: raw_language.clone(),
            root: PathBuf::from("workspace").join(format!(
                "root\n{}\u{2066}",
                "r".repeat(MAX_LSP_TRACE_DETAIL_CHARS)
            )),
            generation: 3,
        };

        let (server_method, server_detail) = lsp_ui_event_trace_label(&server_event);

        assert_eq!(server_method, "$/serverReady");
        assert_trace_detail_is_safe_and_bounded(&server_detail);
        assert!(server_detail.starts_with("rust analyzer"));
        assert!(server_detail.contains("#3 at"));
        assert!(server_detail.contains("..."));
        if let LspUiEvent::ServerReady { language, .. } = &server_event {
            assert_eq!(language, &raw_language);
        }
    }

    #[test]
    fn lsp_ui_event_trace_label_sanitizes_query_path_result_helpers() {
        let raw_query = format!(
            "Symbol\nQuery \u{202e}{}",
            "q".repeat(MAX_LSP_TRACE_DETAIL_CHARS)
        );
        let raw_error = format!(
            "lookup\nfailed \u{2066}{}",
            "e".repeat(MAX_LSP_TRACE_DETAIL_CHARS)
        );
        let query_event = LspUiEvent::WorkspaceSymbolsResult {
            id: 9,
            path: PathBuf::from("workspace"),
            query: raw_query.clone(),
            symbols: None,
            error: Some(raw_error.clone()),
        };

        let (method, detail) = lsp_ui_event_trace_label(&query_event);

        assert_eq!(method, "workspace/symbol");
        assert_trace_detail_is_safe_and_bounded(&detail);
        assert!(detail.starts_with("`Symbol Query"));
        assert!(detail.contains("lookup failed"));
        assert!(detail.contains("..."));

        let path_event = LspUiEvent::FoldingRangesResult {
            id: 10,
            path: PathBuf::from("src").join(format!(
                "fold\n{}\u{202e}.rs",
                "p".repeat(MAX_LSP_TRACE_DETAIL_CHARS)
            )),
            version: 2,
            ranges: None,
            error: Some(raw_error.clone()),
        };

        let (_, path_detail) = lsp_ui_event_trace_label(&path_event);

        assert_trace_detail_is_safe_and_bounded(&path_detail);
        assert!(path_detail.contains("fold"));
        assert!(path_detail.contains("lookup failed"));
        assert!(path_detail.contains("..."));
        if let LspUiEvent::WorkspaceSymbolsResult { query, error, .. } = &query_event {
            assert_eq!(query, &raw_query);
            assert_eq!(error.as_deref(), Some(raw_error.as_str()));
        }
    }
}
