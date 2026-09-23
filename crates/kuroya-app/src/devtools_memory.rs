use crate::{
    KuroyaApp,
    editor_row_render_cache::{editor_render_cache_hits, editor_render_cache_misses},
    plugin_command_runtime::{PluginCommandModuleCacheStats, plugin_command_module_cache_stats},
    terminal::TerminalDiagnosticsStats,
    ui_text::count_label,
};
use eframe::egui::{self, RichText};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryDiagnosticsSummary {
    pub(crate) buffers: BufferMemoryDiagnostics,
    pub(crate) terminal: TerminalDiagnosticsStats,
    pub(crate) project: ProjectMemoryDiagnostics,
    pub(crate) diagnostics: DiagnosticMemoryDiagnostics,
    pub(crate) search: SearchMemoryDiagnostics,
    pub(crate) lsp: LspMemoryDiagnostics,
    pub(crate) plugins: PluginMemoryDiagnostics,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct BufferMemoryDiagnostics {
    pub(crate) buffers: usize,
    pub(crate) dirty_buffers: usize,
    pub(crate) bytes: usize,
    pub(crate) lines: usize,
    pub(crate) undo_entries: usize,
    pub(crate) redo_entries: usize,
    pub(crate) image_previews: usize,
    pub(crate) binary_previews: usize,
    pub(crate) read_only_buffers: usize,
    pub(crate) diff_cache_entries: usize,
    pub(crate) merge_conflict_cache_entries: usize,
    pub(crate) row_render_cache_entries: usize,
    pub(crate) row_render_cache_hits: u64,
    pub(crate) row_render_cache_misses: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ProjectMemoryDiagnostics {
    pub(crate) files: usize,
    pub(crate) symbols: usize,
    pub(crate) entries: usize,
    pub(crate) truncated: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct DiagnosticMemoryDiagnostics {
    pub(crate) total: usize,
    pub(crate) errors: usize,
    pub(crate) warnings: usize,
    pub(crate) infos: usize,
    pub(crate) hints: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SearchMemoryDiagnostics {
    pub(crate) matches: usize,
    pub(crate) truncated: bool,
    pub(crate) current_query: bool,
    pub(crate) has_error: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct LspMemoryDiagnostics {
    pub(crate) clients: usize,
    pub(crate) unavailable: usize,
    pub(crate) pending_restarts: usize,
    pub(crate) restart_attempts: usize,
    pub(crate) progress_tasks: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PluginMemoryDiagnostics {
    pub(crate) loaded: usize,
    pub(crate) errors: usize,
    pub(crate) runtimes: usize,
    pub(crate) commands: usize,
    pub(crate) languages: usize,
    pub(crate) themes: usize,
    pub(crate) syntaxes: usize,
    pub(crate) command_module_cache: PluginCommandModuleCacheStats,
}

impl KuroyaApp {
    pub(crate) fn memory_diagnostics_summary(&self) -> MemoryDiagnosticsSummary {
        let severity = self.diagnostics.severity_counts();
        MemoryDiagnosticsSummary {
            buffers: BufferMemoryDiagnostics {
                buffers: self.buffers.len(),
                dirty_buffers: self
                    .buffers
                    .iter()
                    .filter(|buffer| buffer.is_dirty())
                    .count(),
                bytes: self.buffers.iter().map(|buffer| buffer.len_bytes()).sum(),
                lines: self.buffers.iter().map(|buffer| buffer.len_lines()).sum(),
                undo_entries: self
                    .buffers
                    .iter()
                    .map(|buffer| buffer.undo_entry_count())
                    .sum(),
                redo_entries: self
                    .buffers
                    .iter()
                    .map(|buffer| buffer.redo_entry_count())
                    .sum(),
                image_previews: self.image_preview_buffers.len(),
                binary_previews: self.binary_preview_buffers.len(),
                read_only_buffers: self.manual_read_only_buffers.len(),
                diff_cache_entries: self.diff_cache.len(),
                merge_conflict_cache_entries: self.merge_conflict_cache.len(),
                row_render_cache_entries: self.editor_row_render_cache.len(),
                row_render_cache_hits: editor_render_cache_hits(),
                row_render_cache_misses: editor_render_cache_misses(),
            },
            terminal: self.terminal.diagnostics_stats(),
            project: ProjectMemoryDiagnostics {
                files: self.index.files().len(),
                symbols: self.index.symbols().len(),
                entries: self.index.all_entries().len(),
                truncated: self.index.truncated(),
            },
            diagnostics: DiagnosticMemoryDiagnostics {
                total: self.diagnostics.len(),
                errors: severity.errors,
                warnings: severity.warnings,
                infos: severity.infos,
                hints: severity.hints,
            },
            search: SearchMemoryDiagnostics {
                matches: self.project_search_result.matches.len(),
                truncated: self.project_search_result.truncated,
                current_query: self.project_search_results_match_current_query(),
                has_error: self.project_search_result.error.is_some(),
            },
            lsp: LspMemoryDiagnostics {
                clients: self.lsp_clients.len(),
                unavailable: self.lsp_unavailable.len(),
                pending_restarts: self.pending_lsp_restarts.len(),
                restart_attempts: self.lsp_restart_attempts.len(),
                progress_tasks: self.lsp_progress_titles.len(),
            },
            plugins: PluginMemoryDiagnostics {
                loaded: self.plugins.len(),
                errors: self.plugin_errors.len(),
                runtimes: self.plugin_runtimes.len(),
                commands: self.plugin_commands.len(),
                languages: self.plugin_languages.len(),
                themes: self.plugin_themes.len(),
                syntaxes: self.plugin_syntaxes.len(),
                command_module_cache: plugin_command_module_cache_stats(),
            },
        }
    }
}

pub(crate) fn render_memory_diagnostics_panel(
    ui: &mut egui::Ui,
    summary: &MemoryDiagnosticsSummary,
) {
    ui.label(RichText::new("Memory Diagnostics").strong());
    ui.label(RichText::new("Runtime counts that usually explain retained memory.").small());

    render_memory_grid(ui, "devtools-memory-buffers", |ui| {
        memory_row(
            ui,
            "Buffers",
            count_label(summary.buffers.buffers, "buffer", "buffers"),
        );
        memory_row(ui, "Dirty", summary.buffers.dirty_buffers.to_string());
        memory_row(ui, "Text", format_memory_bytes(summary.buffers.bytes));
        memory_row(ui, "Lines", summary.buffers.lines.to_string());
        memory_row(ui, "Undo entries", summary.buffers.undo_entries.to_string());
        memory_row(ui, "Redo entries", summary.buffers.redo_entries.to_string());
        memory_row(
            ui,
            "Image previews",
            summary.buffers.image_previews.to_string(),
        );
        memory_row(
            ui,
            "Binary previews",
            summary.buffers.binary_previews.to_string(),
        );
        memory_row(
            ui,
            "Read-only buffers",
            summary.buffers.read_only_buffers.to_string(),
        );
        memory_row(
            ui,
            "Diff cache",
            summary.buffers.diff_cache_entries.to_string(),
        );
        memory_row(
            ui,
            "Conflict cache",
            summary.buffers.merge_conflict_cache_entries.to_string(),
        );
        memory_row(
            ui,
            "Row render cache",
            summary.buffers.row_render_cache_entries.to_string(),
        );
        memory_row(
            ui,
            "Row cache hits",
            summary.buffers.row_render_cache_hits.to_string(),
        );
        memory_row(
            ui,
            "Row cache misses",
            summary.buffers.row_render_cache_misses.to_string(),
        );
    });

    render_memory_grid(ui, "devtools-memory-runtime", |ui| {
        memory_row(
            ui,
            "Terminal sessions",
            summary.terminal.sessions.to_string(),
        );
        memory_row(
            ui,
            "Terminal active",
            summary.terminal.active_sessions.to_string(),
        );
        memory_row(
            ui,
            "Terminal searchable lines",
            summary.terminal.searchable_lines.to_string(),
        );
        memory_row(
            ui,
            "Terminal searchable text",
            format_memory_bytes(summary.terminal.search_buffer_bytes),
        );
        memory_row(
            ui,
            "Scrollback limit",
            summary.terminal.configured_scrollback_rows.to_string(),
        );
        memory_row(ui, "LSP clients", summary.lsp.clients.to_string());
        memory_row(ui, "LSP unavailable", summary.lsp.unavailable.to_string());
        memory_row(ui, "LSP restarts", summary.lsp.pending_restarts.to_string());
        memory_row(ui, "LSP progress", summary.lsp.progress_tasks.to_string());
    });

    render_memory_grid(ui, "devtools-memory-project", |ui| {
        memory_row(ui, "Indexed files", summary.project.files.to_string());
        memory_row(ui, "Project entries", summary.project.entries.to_string());
        memory_row(ui, "Project symbols", summary.project.symbols.to_string());
        memory_row(ui, "Index truncated", yes_no(summary.project.truncated));
        memory_row(ui, "Search matches", summary.search.matches.to_string());
        memory_row(ui, "Search current", yes_no(summary.search.current_query));
        memory_row(ui, "Search truncated", yes_no(summary.search.truncated));
        memory_row(ui, "Search error", yes_no(summary.search.has_error));
        memory_row(ui, "Diagnostics", summary.diagnostics.total.to_string());
        memory_row(ui, "Errors", summary.diagnostics.errors.to_string());
        memory_row(ui, "Warnings", summary.diagnostics.warnings.to_string());
        memory_row(ui, "Info", summary.diagnostics.infos.to_string());
        memory_row(ui, "Hints", summary.diagnostics.hints.to_string());
    });

    render_memory_grid(ui, "devtools-memory-plugins", |ui| {
        memory_row(ui, "Plugins", summary.plugins.loaded.to_string());
        memory_row(ui, "Plugin errors", summary.plugins.errors.to_string());
        memory_row(ui, "Runtimes", summary.plugins.runtimes.to_string());
        memory_row(ui, "Commands", summary.plugins.commands.to_string());
        memory_row(ui, "Languages", summary.plugins.languages.to_string());
        memory_row(ui, "Themes", summary.plugins.themes.to_string());
        memory_row(ui, "Syntaxes", summary.plugins.syntaxes.to_string());
        memory_row(
            ui,
            "Command Wasm cache",
            format!(
                "{}/{}",
                summary.plugins.command_module_cache.entries,
                summary.plugins.command_module_cache.capacity
            ),
        );
    });
}

fn render_memory_grid(ui: &mut egui::Ui, id: &'static str, add_rows: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(4.0);
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([20.0, 3.0])
        .striped(true)
        .show(ui, add_rows);
}

fn memory_row(ui: &mut egui::Ui, label: &'static str, value: String) {
    ui.label(RichText::new(label).small());
    ui.monospace(value);
    ui.end_row();
}

fn yes_no(value: bool) -> String {
    if value {
        "yes".to_owned()
    } else {
        "no".to_owned()
    }
}

fn format_memory_bytes(bytes: usize) -> String {
    const KIB: usize = 1024;
    const MIB: usize = KIB * 1024;
    if bytes >= MIB {
        return format!("{:.1} MiB", bytes as f64 / MIB as f64);
    }
    if bytes >= KIB {
        return format!("{:.1} KiB", bytes as f64 / KIB as f64);
    }
    count_label(bytes, "byte", "bytes")
}

#[cfg(test)]
mod tests {
    use super::format_memory_bytes;

    #[test]
    fn memory_byte_labels_scale_without_hiding_small_values() {
        assert_eq!(format_memory_bytes(0), "0 bytes");
        assert_eq!(format_memory_bytes(1), "1 byte");
        assert_eq!(format_memory_bytes(1536), "1.5 KiB");
        assert_eq!(format_memory_bytes(2 * 1024 * 1024), "2.0 MiB");
    }
}
