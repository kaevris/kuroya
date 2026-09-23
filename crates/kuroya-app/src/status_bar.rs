use crate::{
    KuroyaApp,
    editor_vim_key_events::{
        vim_mode_status_label, vim_pending_command_status_label,
        vim_pending_key_sequence_status_label, vim_pending_search_status_label,
    },
    large_file_mode::buffer_uses_large_file_mode,
    ui_icons::IconKind,
    ui_text::count_label,
};
use egui::{self, Align, RichText};
use kuroya_core::{BufferId, DiagnosticSet, LanguageId, PluginLanguageRegistry, TextBuffer};
use std::{borrow::Cow, collections::HashSet, fmt::Write as _};

pub(crate) mod items;
mod warnings;

use items::{
    git_status_label, normalize_status_bar_text, prepare_status_item, status_item,
    status_item_width,
};
use warnings::render_status_warnings;

const STATUS_MESSAGE_MAX_CHARS: usize = 180;
const STATUS_LANGUAGE_MAX_CHARS: usize = 48;
const STATUS_BAR_ITEM_MIN_FREE_WIDTH: f32 = 60.0;

impl KuroyaApp {
    pub(crate) fn render_status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            self.ingest_status_toast();
            let disk_change_state = self.status_bar_disk_change_state();
            let git_blame_label = self.active_git_blame_status_bar_text();
            let active = self.active_buffer();
            let active_lossy_decoded = self
                .active
                .is_some_and(|id| self.lossy_decoded_buffers.contains(&id));
            let active_binary_preview = self
                .active
                .is_some_and(|id| self.binary_preview_buffers.contains(&id));
            let active_image_preview = self
                .active
                .is_some_and(|id| self.image_preview_buffers.contains_key(&id));
            let active_read_only = active.is_some_and(kuroya_core::TextBuffer::is_read_only);
            let active_large_file_mode = active.is_some_and(buffer_uses_large_file_mode);
            let language = active
                .map(|buffer| status_language_label(buffer, &self.plugin_languages))
                .unwrap_or(Cow::Borrowed("No file"));
            let lines = active.map(|buffer| buffer.len_lines()).unwrap_or_default();
            let cursor_label = active.map(cursor_status_label);

            let cursor_item = prepare_status_item(
                cursor_label.as_deref().unwrap_or("Ln -, Col -"),
                "Cursor position",
            );
            let language_item = prepare_status_item(
                language_line_status_label(language.as_ref(), lines),
                "Language and line count",
            );
            let diagnostics_tooltip = diagnostics_status_tooltip(&self.diagnostics);
            let diagnostics_item = prepare_status_item(
                diagnostics_status_label(&self.diagnostics),
                diagnostics_tooltip.as_ref(),
            );
            if self.settings.vim_keybindings {
                if let Some(vim_status_label) =
                    vim_pending_left_status_label(self.editor_vim_pending_key, &self.settings.vim)
                {
                    status_item(
                        ui,
                        IconKind::Command,
                        prepare_status_item(vim_status_label, "Vim pending input"),
                    );
                }
            }

            let fits = |item: &items::PreparedStatusItem<'_>, ui: &mut egui::Ui| {
                ui.available_width() >= status_item_width(ui, item) + STATUS_BAR_ITEM_MIN_FREE_WIDTH
            };

            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                status_item(ui, IconKind::Cursor, cursor_item);
                status_item(ui, IconKind::Code, language_item);
                status_item(ui, IconKind::Diagnostics, diagnostics_item);

                if let Some(buffer) = active {
                    let encoding_item = prepare_status_item(
                        encoding_status_label(buffer, active_binary_preview),
                        "File encoding",
                    );
                    if fits(&encoding_item, ui) {
                        status_item(ui, IconKind::File, encoding_item);
                    }
                }

                render_status_warnings(
                    ui,
                    disk_change_state.active_changed_on_disk,
                    active_binary_preview && !active_image_preview,
                    active_image_preview,
                    active_lossy_decoded,
                    active_read_only,
                    active_large_file_mode,
                    disk_change_state.external_change_count,
                    self.settings.word_wrap,
                );

                let git_item = prepare_status_item(git_status_label(&self.git), "Git status");
                if fits(&git_item, ui) {
                    status_item(ui, IconKind::GitBranch, git_item);
                }
                if self.settings.vim_keybindings {
                    let vim_item = prepare_status_item(
                        vim_mode_status_label(self.editor_vim_mode, self.editor_vim_pending_key),
                        "Vim mode",
                    );
                    if fits(&vim_item, ui) {
                        status_item(ui, IconKind::Command, vim_item);
                    }
                }
                if let Some(git_blame_label) = git_blame_label.as_deref() {
                    let blame_item =
                        prepare_status_item(git_blame_label, "Git blame for active line");
                    if fits(&blame_item, ui) {
                        status_item(ui, IconKind::GitBranch, blame_item);
                    }
                }
                let lsp_item = prepare_status_item(
                    count_status_label(self.lsp_clients.len(), "lsp"),
                    "Language servers",
                );
                if fits(&lsp_item, ui) {
                    status_item(ui, IconKind::Lsp, lsp_item);
                }
                let panes_item = prepare_status_item(
                    count_status_label(self.panes.len(), "panes"),
                    "Editor panes",
                );
                if fits(&panes_item, ui) {
                    status_item(ui, IconKind::Panes, panes_item);
                }
                let trust_item = prepare_status_item(
                    if self.workspace_placeholder {
                        "no folder"
                    } else if self.workspace_trusted {
                        "trusted"
                    } else {
                        "restricted"
                    },
                    "Workspace trust",
                );
                if fits(&trust_item, ui) {
                    status_item(ui, IconKind::Settings, trust_item);
                }
                let theme_item = prepare_status_item(self.settings.theme.name.as_str(), "Theme");
                if fits(&theme_item, ui) {
                    status_item(ui, IconKind::Theme, theme_item);
                }
                let renderer_item = prepare_status_item("wgpu", "Renderer backend");
                if fits(&renderer_item, ui) {
                    ui.label(RichText::new("wgpu").small());
                    ui.separator();
                }
            });
        });
    }

    fn status_bar_disk_change_state(&self) -> StatusBarDiskChangeState {
        if !self.has_pending_reload_external_change_sources() {
            return self.status_bar_disk_change_state_from_ids(
                self.external_change_buffers.iter().copied(),
            );
        }

        let changed_on_disk = self.observed_external_change_buffer_ids();
        self.status_bar_disk_change_state_from_ids(changed_on_disk)
    }

    fn status_bar_disk_change_state_from_ids(
        &self,
        changed_on_disk: impl IntoIterator<Item = BufferId>,
    ) -> StatusBarDiskChangeState {
        let mut active_changed_on_disk = false;
        let mut external_change_count = 0usize;
        let mut seen = HashSet::new();
        for id in changed_on_disk {
            if !seen.insert(id) {
                continue;
            }
            if self.buffer(id).is_none() {
                continue;
            }
            external_change_count = external_change_count.saturating_add(1);
            if self.active == Some(id) {
                active_changed_on_disk = true;
            }
        }

        StatusBarDiskChangeState {
            active_changed_on_disk,
            external_change_count,
        }
    }
}

fn vim_pending_left_status_label(
    pending: Option<crate::editor_vim_key_events::EditorVimPendingKey>,
    vim_settings: &kuroya_core::EditorVimSettings,
) -> Option<String> {
    vim_pending_key_sequence_status_label(pending, vim_settings)
        .or_else(|| vim_pending_command_status_label(pending))
        .or_else(|| vim_pending_search_status_label(pending))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StatusBarDiskChangeState {
    active_changed_on_disk: bool,
    external_change_count: usize,
}

pub(crate) fn status_bar_message(status: &str) -> Cow<'_, str> {
    normalize_status_bar_text(status, STATUS_MESSAGE_MAX_CHARS).unwrap_or(Cow::Borrowed(""))
}

pub(crate) fn status_message_is_persistent(status: &str) -> bool {
    let lowered = status.to_ascii_lowercase();
    [
        "could not",
        "failed",
        "failure",
        "error",
        "denied",
        "unable",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

pub(crate) fn status_language_label<'a>(
    buffer: &TextBuffer,
    plugin_languages: &'a PluginLanguageRegistry,
) -> Cow<'a, str> {
    if buffer.language() != LanguageId::PlainText {
        return Cow::Borrowed(language_id_status_label(buffer.language()));
    }

    buffer
        .path()
        .and_then(|path| plugin_languages.language_for_path(path))
        .map(|language| status_language_display_label(language.display_name()))
        .unwrap_or_else(|| Cow::Borrowed(language_id_status_label(buffer.language())))
}

fn status_language_display_label(language: &str) -> Cow<'_, str> {
    normalize_status_bar_text(language, STATUS_LANGUAGE_MAX_CHARS)
        .unwrap_or(Cow::Borrowed("PlainText"))
}

/// Best-effort encoding label for the active buffer from reachable open-file metadata.
/// Per-buffer BOM state is not tracked, so "UTF-8 BOM" reflects a BOM character still
/// present in the buffer text; freshly decoded files have the BOM stripped and are
/// surfaced via the "(UTF-8 BOM)" note in the file-open status message instead.
fn encoding_status_label(buffer: &TextBuffer, binary_preview: bool) -> &'static str {
    if binary_preview {
        let first_line_prefix = buffer
            .line_content_prefix(0, crate::file_decode::UTF16_LABEL_DETECT_CHARS)
            .unwrap_or_default();
        if let Some(label) = crate::file_decode::utf16_unsupported_label(&first_line_prefix) {
            return label;
        }
        return "binary";
    }
    if buffer.line_starts_with(0, "\u{FEFF}") {
        return "UTF-8 BOM";
    }
    "UTF-8"
}

fn language_id_status_label(language: LanguageId) -> &'static str {
    match language {
        LanguageId::Rust => "Rust",
        LanguageId::Toml => "Toml",
        LanguageId::Json => "Json",
        LanguageId::Sql => "Sql",
        LanguageId::Markdown => "Markdown",
        LanguageId::PowerShell => "PowerShell",
        LanguageId::Python => "Python",
        LanguageId::TypeScript => "TypeScript",
        LanguageId::JavaScript => "JavaScript",
        LanguageId::Css => "Css",
        LanguageId::Html => "Html",
        LanguageId::Yaml => "Yaml",
        LanguageId::Go => "Go",
        LanguageId::Java => "Java",
        LanguageId::C => "C",
        LanguageId::Cpp => "Cpp",
        LanguageId::CSharp => "CSharp",
        LanguageId::Php => "PHP",
        LanguageId::Ruby => "Ruby",
        LanguageId::Lua => "Lua",
        LanguageId::Dart => "Dart",
        LanguageId::Kotlin => "Kotlin",
        LanguageId::Swift => "Swift",
        LanguageId::Vue => "Vue",
        LanguageId::Svelte => "Svelte",
        LanguageId::Xml => "XML",
        LanguageId::Dockerfile => "Dockerfile",
        LanguageId::Terraform => "Terraform",
        LanguageId::Shell => "Shell",
        LanguageId::Diff => "Diff",
        LanguageId::PlainText => "PlainText",
    }
}

fn language_line_status_label(language: &str, lines: usize) -> String {
    let language = normalize_status_bar_text(language, STATUS_LANGUAGE_MAX_CHARS)
        .unwrap_or(Cow::Borrowed("Unknown"));
    let mut label = String::with_capacity(language.len() + 2 + decimal_digit_count(lines) + 6);
    label.push_str(language.as_ref());
    label.push_str("  ");
    let _ = write!(label, "{lines} lines");
    label
}

fn count_status_label(count: usize, noun: &str) -> String {
    let mut label = String::with_capacity(decimal_digit_count(count) + 1 + noun.len());
    let _ = write!(label, "{count} {noun}");
    label
}

fn cursor_status_label(buffer: &TextBuffer) -> String {
    let cursors = buffer.cursor_positions();
    let primary = cursors
        .last()
        .copied()
        .unwrap_or_else(|| buffer.cursor_position());
    let line = one_based_status_position(primary.line);
    let column = one_based_status_position(primary.column);
    let cursor_count = cursors.len();
    let mut label = String::with_capacity(
        9 + decimal_digit_count(line)
            + decimal_digit_count(column)
            + if cursor_count > 1 {
                10 + decimal_digit_count(cursor_count)
            } else {
                0
            },
    );
    let _ = write!(label, "Ln {line}, Col {column}");
    if cursor_count > 1 {
        let _ = write!(label, "  {cursor_count} cursors");
    }
    label
}

fn one_based_status_position(zero_based: usize) -> usize {
    zero_based.saturating_add(1)
}

fn decimal_digit_count(mut value: usize) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

pub(crate) fn diagnostics_status_label(diagnostics: &DiagnosticSet) -> String {
    count_label(diagnostics.len(), "diagnostic", "diagnostics")
}

pub(crate) fn diagnostics_status_tooltip(diagnostics: &DiagnosticSet) -> Cow<'static, str> {
    if diagnostics.is_empty() {
        return Cow::Borrowed("No diagnostics");
    }

    let counts = diagnostics.severity_counts();
    let mut summary = String::from("Diagnostics: ");
    push_diagnostic_count(&mut summary, counts.errors, "error", "errors");
    push_diagnostic_count(&mut summary, counts.warnings, "warning", "warnings");
    push_diagnostic_count(&mut summary, counts.infos, "info", "info");
    push_diagnostic_count(&mut summary, counts.hints, "hint", "hints");
    Cow::Owned(summary)
}

fn push_diagnostic_count(summary: &mut String, count: usize, singular: &str, plural: &str) {
    if count == 0 {
        return;
    }

    if !summary.ends_with(": ") {
        summary.push_str(", ");
    }
    let noun = if count == 1 { singular } else { plural };
    let _ = write!(summary, "{count} {noun}");
}

#[cfg(test)]
mod tests {
    use super::{
        STATUS_LANGUAGE_MAX_CHARS, STATUS_MESSAGE_MAX_CHARS, StatusBarDiskChangeState,
        diagnostics_status_label, diagnostics_status_tooltip, encoding_status_label,
        language_line_status_label, one_based_status_position, status_bar_message,
        status_language_label,
    };
    use crate::file_decode::UTF16_UNSUPPORTED_NOTICE_LE;
    use crate::{
        KuroyaApp,
        app_startup_context::AppStartupContext,
        app_state::{PendingFileReload, QueuedFileReload},
        terminal::TerminalPane,
    };
    use kuroya_core::{
        Diagnostic, DiagnosticSeverity, EditorSettings, PluginCapabilities, PluginContributions,
        PluginDescriptor, PluginLanguageContribution, PluginLanguageRegistry, PluginManifest,
        TextBuffer, Workspace,
    };
    use std::{path::PathBuf, time::Instant};
    use tokio::runtime::Runtime;

    #[test]
    fn status_language_label_uses_plugin_language_for_plain_text_extensions() {
        let plugin = PluginDescriptor {
            root: PathBuf::from("workspace/.kuroya/plugins/example"),
            manifest: PluginManifest {
                api_version: "1".to_owned(),
                id: "example.plugin".to_owned(),
                name: "Example".to_owned(),
                version: "0.1.0".to_owned(),
                entry: None,
                activation_events: Vec::new(),
                capabilities: PluginCapabilities {
                    languages: true,
                    ..PluginCapabilities::default()
                },
                contributes: PluginContributions {
                    languages: vec![PluginLanguageContribution {
                        id: "example-lang".to_owned(),
                        extensions: vec!["ex".to_owned()],
                        aliases: vec!["ExampleLang".to_owned()],
                    }],
                    ..PluginContributions::default()
                },
            },
        };
        let registry = PluginLanguageRegistry::from_plugins(&[plugin]);
        let buffer =
            TextBuffer::from_text(1, Some(PathBuf::from("src/main.ex")), "example".to_owned());

        assert_eq!(status_language_label(&buffer, &registry), "ExampleLang");

        let rust = TextBuffer::from_text(2, Some(PathBuf::from("src/main.rs")), String::new());
        assert_eq!(status_language_label(&rust, &registry), "Rust");
    }

    #[test]
    fn status_language_label_sanitizes_plugin_display_names() {
        let plugin = PluginDescriptor {
            root: PathBuf::from("workspace/.kuroya/plugins/hostile"),
            manifest: PluginManifest {
                api_version: "1".to_owned(),
                id: "hostile.plugin".to_owned(),
                name: "Hostile".to_owned(),
                version: "0.1.0".to_owned(),
                entry: None,
                activation_events: Vec::new(),
                capabilities: PluginCapabilities {
                    languages: true,
                    ..PluginCapabilities::default()
                },
                contributes: PluginContributions {
                    languages: vec![PluginLanguageContribution {
                        id: "hostile-lang".to_owned(),
                        extensions: vec!["hx".to_owned()],
                        aliases: vec![format!(
                            "  Hostile\n{}\u{202e}",
                            "x".repeat(STATUS_LANGUAGE_MAX_CHARS)
                        )],
                    }],
                    ..PluginContributions::default()
                },
            },
        };
        let registry = PluginLanguageRegistry::from_plugins(&[plugin]);
        let buffer = TextBuffer::from_text(1, Some(PathBuf::from("src/main.hx")), String::new());

        let label = status_language_label(&buffer, &registry);
        assert_status_text_is_safe(label.as_ref(), STATUS_LANGUAGE_MAX_CHARS);
        assert!(label.starts_with("Hostile "));
        assert!(label.contains("..."));

        let line_label = language_line_status_label(" \nLanguage\u{202e}", usize::MAX);
        assert!(!line_label.chars().any(char::is_control));
        assert!(!line_label.chars().any(is_bidi_format_control));
        assert!(line_label.starts_with("Language"));
    }

    #[test]
    fn diagnostics_status_tooltip_summarizes_severities() {
        let path = PathBuf::from("src/main.rs");
        let mut diagnostics = kuroya_core::DiagnosticSet::default();
        diagnostics.replace(
            path.clone(),
            vec![
                diagnostic(&path, DiagnosticSeverity::Error),
                diagnostic(&path, DiagnosticSeverity::Warning),
                {
                    let mut warning = diagnostic(&path, DiagnosticSeverity::Warning);
                    warning.line = 2;
                    warning
                },
                diagnostic(&path, DiagnosticSeverity::Hint),
            ],
        );

        assert_eq!(diagnostics_status_label(&diagnostics), "4 diagnostics");
        assert_eq!(
            diagnostics_status_tooltip(&diagnostics),
            "Diagnostics: 1 error, 2 warnings, 1 hint"
        );
    }

    #[test]
    fn diagnostics_status_tooltip_names_empty_state() {
        let diagnostics = kuroya_core::DiagnosticSet::default();

        assert_eq!(diagnostics_status_label(&diagnostics), "0 diagnostics");
        assert_eq!(diagnostics_status_tooltip(&diagnostics), "No diagnostics");
    }

    #[test]
    fn status_position_label_saturates_stale_max_positions() {
        assert_eq!(one_based_status_position(0), 1);
        assert_eq!(one_based_status_position(41), 42);
        assert_eq!(one_based_status_position(usize::MAX), usize::MAX);
    }

    #[test]
    fn encoding_status_label_reports_utf8_bom_utf16_and_binary() {
        let plain = TextBuffer::from_text(1, None, "fn main() {}\n".to_owned());
        assert_eq!(encoding_status_label(&plain, false), "UTF-8");

        let bom = TextBuffer::from_text(2, None, "\u{FEFF}fn main() {}\n".to_owned());
        assert_eq!(encoding_status_label(&bom, false), "UTF-8 BOM");

        let utf16 = TextBuffer::from_text(3, None, UTF16_UNSUPPORTED_NOTICE_LE.to_owned());
        assert_eq!(encoding_status_label(&utf16, true), "UTF-16 LE");

        let binary = TextBuffer::from_text(4, None, "ok\0\n".to_owned());
        assert_eq!(encoding_status_label(&binary, true), "binary");
    }

    #[test]
    fn status_bar_message_is_single_line_bounded_and_strips_format_controls() {
        let text = format!(
            "  saving\n{}{}\u{202e}\u{2066}done  ",
            "x".repeat(STATUS_MESSAGE_MAX_CHARS),
            "\u{0000}",
        );

        let message = status_bar_message(&text);

        assert!(message.chars().count() <= STATUS_MESSAGE_MAX_CHARS);
        assert!(!message.chars().any(char::is_control));
        assert!(!message.contains('\u{202e}'));
        assert!(!message.contains('\u{2066}'));
        assert!(message.starts_with("saving "));
        assert_eq!(status_bar_message("\n\t\u{0}\u{202e}"), "");
    }

    #[test]
    fn status_bar_disk_change_state_uses_markers_without_pending_reloads() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        app.active = Some(2);
        app.buffers
            .push(TextBuffer::from_text(1, None, "one".to_owned()));
        app.buffers
            .push(TextBuffer::from_text(2, None, "two".to_owned()));
        app.mark_buffer_changed_on_disk(1);
        app.mark_buffer_changed_on_disk(2);

        assert_eq!(
            app.status_bar_disk_change_state(),
            StatusBarDiskChangeState {
                active_changed_on_disk: true,
                external_change_count: 2,
            }
        );
    }

    #[test]
    fn status_bar_disk_change_state_treats_active_pending_clean_reload_as_changed_on_disk() {
        let root = PathBuf::from("workspace");
        let active_path = root.join("src/main.rs");
        let inactive_path = root.join("src/lib.rs");
        let mut app = app_for_test(root);
        app.active = Some(1);
        app.buffers.push(TextBuffer::from_text(
            1,
            Some(active_path.clone()),
            "main".to_owned(),
        ));
        app.buffers.push(TextBuffer::from_text(
            2,
            Some(inactive_path.clone()),
            "lib".to_owned(),
        ));
        app.in_flight_reloads.insert(
            1,
            PendingFileReload {
                request_id: 1,
                path: active_path,
                version: app.buffer(1).expect("buffer should exist").version(),
                force_dirty: false,
            },
        );
        app.queued_file_reloads.insert(
            2,
            QueuedFileReload {
                path: inactive_path,
                force_dirty: false,
            },
        );

        assert_eq!(
            app.status_bar_disk_change_state(),
            StatusBarDiskChangeState {
                active_changed_on_disk: true,
                external_change_count: 2,
            }
        );
    }

    #[test]
    fn status_bar_disk_change_state_dedupes_markers_and_ignores_forced_or_mismatched_reloads() {
        let root = PathBuf::from("workspace");
        let active_path = root.join("src/main.rs");
        let forced_path = root.join("src/forced.rs");
        let mismatch_path = root.join("src/mismatch.rs");
        let other_path = root.join("src/other.rs");
        let mut app = app_for_test(root);
        app.active = Some(1);
        app.buffers.push(TextBuffer::from_text(
            1,
            Some(active_path.clone()),
            "main".to_owned(),
        ));
        app.buffers.push(TextBuffer::from_text(
            2,
            Some(forced_path.clone()),
            "forced".to_owned(),
        ));
        app.buffers.push(TextBuffer::from_text(
            3,
            Some(mismatch_path),
            "mismatch".to_owned(),
        ));
        app.mark_buffer_changed_on_disk(1);
        app.in_flight_reloads.insert(
            1,
            PendingFileReload {
                request_id: 1,
                path: active_path,
                version: app.buffer(1).expect("buffer should exist").version(),
                force_dirty: false,
            },
        );
        app.queued_file_reloads.insert(
            2,
            QueuedFileReload {
                path: forced_path,
                force_dirty: true,
            },
        );
        app.in_flight_reloads.insert(
            3,
            PendingFileReload {
                request_id: 2,
                path: other_path,
                version: app.buffer(3).expect("buffer should exist").version(),
                force_dirty: false,
            },
        );

        assert_eq!(
            app.status_bar_disk_change_state(),
            StatusBarDiskChangeState {
                active_changed_on_disk: true,
                external_change_count: 1,
            }
        );
    }

    #[test]
    fn status_bar_disk_change_state_ignores_stale_marker_ids() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        app.active = Some(1);
        app.buffers
            .push(TextBuffer::from_text(1, None, "one".to_owned()));
        app.mark_buffer_changed_on_disk(1);
        app.mark_buffer_changed_on_disk(99);

        assert_eq!(
            app.status_bar_disk_change_state(),
            StatusBarDiskChangeState {
                active_changed_on_disk: true,
                external_change_count: 1,
            }
        );

        app.active = Some(99);
        assert_eq!(
            app.status_bar_disk_change_state(),
            StatusBarDiskChangeState {
                active_changed_on_disk: false,
                external_change_count: 1,
            }
        );
    }

    #[test]
    fn status_bar_disk_change_state_dedupes_duplicate_source_ids() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        app.active = Some(2);
        app.buffers
            .push(TextBuffer::from_text(1, None, "one".to_owned()));
        app.buffers
            .push(TextBuffer::from_text(2, None, "two".to_owned()));

        assert_eq!(
            app.status_bar_disk_change_state_from_ids([1, 2, 1, 2, 99]),
            StatusBarDiskChangeState {
                active_changed_on_disk: true,
                external_change_count: 2,
            }
        );
    }

    fn diagnostic(path: &std::path::Path, severity: DiagnosticSeverity) -> Diagnostic {
        Diagnostic {
            path: path.to_path_buf(),
            line: 1,
            column: 1,
            char_range: 0..1,
            message: "diagnostic".to_owned(),
            severity,
            source: "test".to_owned(),
            unused: false,
            deprecated: false,
        }
    }

    fn app_for_test(root: PathBuf) -> KuroyaApp {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let settings = EditorSettings::default();
        KuroyaApp::from_startup_context(AppStartupContext {
            runtime: Runtime::new().expect("test runtime"),
            tx,
            rx,
            workspace: Workspace::new(root.clone()),
            settings: settings.clone(),
            settings_panel_draft: settings,
            settings_editor_font_path: String::new(),
            settings_ui_font_path: String::new(),
            theme_picker_selected: 0,
            saved_session: None,
            terminal: TerminalPane::new(root.clone(), 100, 12.0, 1.2),
            watcher: None,
            recent_projects: Vec::new(),
            trusted_workspaces: vec![root],
            now: Instant::now(),
            startup_timings: Vec::new(),
        })
    }

    fn assert_status_text_is_safe(text: &str, max_chars: usize) {
        assert!(
            text.chars().count() <= max_chars,
            "status text should be bounded: {text:?}"
        );
        assert!(
            !text.chars().any(char::is_control),
            "status text should not contain controls: {text:?}"
        );
        assert!(
            !text.chars().any(is_bidi_format_control),
            "status text should not contain bidi controls: {text:?}"
        );
    }

    fn is_bidi_format_control(ch: char) -> bool {
        matches!(
            ch,
            '\u{061c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
        )
    }
}
