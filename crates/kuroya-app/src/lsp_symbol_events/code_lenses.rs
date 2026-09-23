use crate::{
    KuroyaApp,
    lsp_runtime::lsp_command_queue_failed_status,
    lsp_text_positions::lsp_one_based_utf16_column_to_char_column,
    path_display::{display_error_label_cow, display_path_label_cow, sanitized_display_label_cow},
    workspace_state::{buffer_id_path_version_matches, lsp_event_path_is_current},
};
use kuroya_core::{BufferId, LspCodeLens, TextBuffer};
use std::{
    borrow::Cow,
    fmt::Write as _,
    path::{Path, PathBuf},
};

const MAX_CODE_LENS_RESOLVE_REQUESTS: usize = 128;
const CODE_LENS_STATUS_TITLE_MAX_CHARS: usize = 120;
const CODE_LENS_STATUS_COMMAND_MAX_CHARS: usize = 120;

pub(super) fn handle_code_lenses_result(
    app: &mut KuroyaApp,
    id: BufferId,
    path: PathBuf,
    version: u64,
    lenses: Option<Vec<LspCodeLens>>,
    error: Option<String>,
) {
    if !lsp_event_path_is_current(&app.workspace.root, &path)
        || !buffer_id_path_version_matches(&app.buffers, id, &path, version)
    {
        return;
    }
    if !app.settings.code_lens {
        app.code_lenses.remove(&path);
        return;
    }
    if let Some(error) = error {
        app.code_lenses.remove(&path);
        let error = display_error_label_cow(&error);
        app.status = format!("Code lenses failed: {}", error.as_ref());
    } else if let Some(lenses) = lenses {
        let Some(buffer) = app.buffer(id) else {
            return;
        };
        let lenses = valid_code_lenses_for_buffer(buffer, lenses);
        let (resolved, unresolved) = split_displayable_and_unresolved_code_lenses(lenses);
        let count = resolved.len();
        let resolving = unresolved.len();
        if resolved.is_empty() {
            if resolving == 0 {
                app.code_lenses.remove(&path);
            } else {
                app.code_lenses.insert(path.clone(), Vec::new());
            }
        } else {
            app.code_lenses.insert(path.clone(), resolved);
        }
        if !queue_code_lens_resolves(app, id, &path, version, unresolved) {
            return;
        }
        app.status = code_lenses_loaded_status(count, resolving, &path);
    } else {
        app.code_lenses.remove(&path);
        let path = display_path_label_cow(&path);
        app.status = format!("Could not load code lenses for {}", path.as_ref());
    }
}

pub(super) fn handle_code_lens_resolve_result(
    app: &mut KuroyaApp,
    id: BufferId,
    path: PathBuf,
    version: u64,
    lens: Option<LspCodeLens>,
    error: Option<String>,
) {
    if !lsp_event_path_is_current(&app.workspace.root, &path)
        || !buffer_id_path_version_matches(&app.buffers, id, &path, version)
    {
        return;
    }
    if !app.settings.code_lens {
        app.code_lenses.remove(&path);
        return;
    }
    if let Some(error) = error {
        let error = display_error_label_cow(&error);
        app.status = format!("Code lens resolve failed: {}", error.as_ref());
        return;
    }
    let Some(lens) = lens else {
        let path = display_path_label_cow(&path);
        app.status = format!(
            "Code lens resolve returned no displayable command for {}",
            path.as_ref()
        );
        return;
    };
    let Some(buffer) = app.buffer(id) else {
        return;
    };
    let Some(lens) = valid_code_lens_for_buffer(buffer, lens) else {
        return;
    };
    let Some(entry) = app.code_lenses.get_mut(&path) else {
        return;
    };
    entry.push(lens);
    sort_code_lenses_by_position(entry);
    dedup_code_lenses(entry);
    let path = display_path_label_cow(&path);
    app.status = format!("Resolved code lens in {}", path.as_ref());
}

pub(super) fn handle_code_lens_command_result(
    app: &mut KuroyaApp,
    id: BufferId,
    path: PathBuf,
    version: u64,
    title: String,
    command: String,
    error: Option<String>,
) {
    if !lsp_event_path_is_current(&app.workspace.root, &path)
        || !buffer_id_path_version_matches(&app.buffers, id, &path, version)
    {
        return;
    }
    if let Some(error) = error {
        let error = display_error_label_cow(&error);
        app.status = format!(
            "Code lens `{}` failed: {}",
            code_lens_status_title(&title),
            error.as_ref()
        );
    } else {
        app.status = format!(
            "Code lens `{}` executed ({})",
            code_lens_status_title(&title),
            code_lens_status_command(&command)
        );
    }
}

fn queue_code_lens_resolves(
    app: &mut KuroyaApp,
    id: BufferId,
    path: &std::path::Path,
    version: u64,
    lenses: Vec<LspCodeLens>,
) -> bool {
    if lenses.is_empty() {
        return true;
    }
    let Some(client) = app.ensure_lsp_for_buffer(id) else {
        return true;
    };
    for lens in lenses {
        if client.resolve_code_lens(id, path.to_path_buf(), version, lens) {
            app.record_lsp_client_trace("codeLens/resolve", code_lens_resolve_trace_detail(path));
        } else {
            app.status = lsp_command_queue_failed_status("codeLens/resolve");
            return false;
        }
    }
    true
}

fn code_lens_resolve_trace_detail(path: &std::path::Path) -> Cow<'_, str> {
    display_path_label_cow(path)
}

fn valid_code_lenses_for_buffer(buffer: &TextBuffer, lenses: Vec<LspCodeLens>) -> Vec<LspCodeLens> {
    let mut valid_lenses = Vec::with_capacity(lenses.len());
    for lens in lenses {
        if let Some(lens) = valid_code_lens_for_buffer(buffer, lens) {
            valid_lenses.push(lens);
        }
    }
    if valid_lenses.len() > 1 {
        sort_code_lenses_by_position(&mut valid_lenses);
    }
    valid_lenses
}

fn valid_code_lens_for_buffer(buffer: &TextBuffer, mut lens: LspCodeLens) -> Option<LspCodeLens> {
    let char_column = lsp_one_based_utf16_column_to_char_column(buffer, lens.line, lens.column)?;
    lens.column = char_column + 1;
    Some(lens)
}

fn split_displayable_and_unresolved_code_lenses(
    lenses: Vec<LspCodeLens>,
) -> (Vec<LspCodeLens>, Vec<LspCodeLens>) {
    let lens_count = lenses.len();
    let mut resolved = Vec::with_capacity(lens_count);
    let mut unresolved = Vec::with_capacity(lens_count.min(MAX_CODE_LENS_RESOLVE_REQUESTS));
    for lens in lenses {
        if lens.needs_resolve() {
            if unresolved.len() < MAX_CODE_LENS_RESOLVE_REQUESTS {
                unresolved.push(lens);
            }
        } else if !lens.title.is_empty() {
            resolved.push(lens);
        }
    }
    (resolved, unresolved)
}

fn code_lenses_loaded_status(count: usize, resolving: usize, path: &Path) -> String {
    let path = display_path_label_cow(path);
    let path = path.as_ref();
    if count == 0 {
        if resolving == 0 {
            let mut status = String::with_capacity("No code lenses in ".len() + path.len());
            status.push_str("No code lenses in ");
            status.push_str(path);
            return status;
        }

        let mut status =
            String::with_capacity("Resolving  code lenses in ".len() + 20 + path.len());
        let _ = write!(status, "Resolving {resolving} code lenses in {path}");
        return status;
    }

    let mut status = String::with_capacity(
        " code lenses in ".len()
            + 20
            + path.len()
            + if resolving > 0 {
                "; resolving ".len() + 20
            } else {
                0
            },
    );
    let _ = write!(status, "{count} code lenses in {path}");
    if resolving > 0 {
        let _ = write!(status, "; resolving {resolving}");
    }
    status
}

fn sort_code_lenses_by_position(lenses: &mut [LspCodeLens]) {
    lenses.sort_by(|left, right| {
        left.line
            .cmp(&right.line)
            .then(left.column.cmp(&right.column))
            .then_with(|| left.title.cmp(&right.title))
            .then_with(|| left.command.cmp(&right.command))
    });
}

fn dedup_code_lenses(lenses: &mut Vec<LspCodeLens>) {
    lenses.dedup_by(|left, right| {
        left.line == right.line
            && left.column == right.column
            && left.title == right.title
            && left.command == right.command
    });
}

fn code_lens_status_title(title: &str) -> Cow<'_, str> {
    sanitized_display_label_cow(title, CODE_LENS_STATUS_TITLE_MAX_CHARS, "code lens")
}

fn code_lens_status_command(command: &str) -> Cow<'_, str> {
    sanitized_display_label_cow(command, CODE_LENS_STATUS_COMMAND_MAX_CHARS, "command")
}

#[cfg(test)]
mod tests {
    use super::{
        CODE_LENS_STATUS_COMMAND_MAX_CHARS, CODE_LENS_STATUS_TITLE_MAX_CHARS,
        code_lens_resolve_trace_detail, code_lens_status_command, code_lens_status_title,
        handle_code_lens_command_result, handle_code_lens_resolve_result,
        handle_code_lenses_result, valid_code_lenses_for_buffer,
    };
    use crate::{
        KuroyaApp,
        app_startup_context::AppStartupContext,
        path_display::{DISPLAY_ERROR_LABEL_MAX_CHARS, DISPLAY_PATH_LABEL_MAX_CHARS},
        terminal::TerminalPane,
    };
    use kuroya_core::{EditorSettings, LspCodeLens, TextBuffer, Workspace};
    use serde_json::json;
    use std::{borrow::Cow, path::PathBuf, sync::Arc, time::Instant};
    use tokio::runtime::Runtime;

    #[test]
    fn valid_code_lenses_filter_out_of_bounds_positions() {
        let buffer = TextBuffer::from_text(1, None, "alpha\nbeta".to_owned());
        let lenses = valid_code_lenses_for_buffer(
            &buffer,
            vec![
                lens(1, 1, "Run"),
                lens(2, 5, "Debug"),
                lens(2, 6, "Past"),
                lens(4, 1, "Missing"),
            ],
        );

        assert_eq!(lenses, vec![lens(1, 1, "Run"), lens(2, 5, "Debug")]);
    }

    #[test]
    fn valid_code_lenses_convert_utf16_columns_to_char_columns() {
        let buffer = TextBuffer::from_text(1, None, "😀x".to_owned());
        let lenses = valid_code_lenses_for_buffer(
            &buffer,
            vec![
                lens(1, 1, "Start"),
                lens(1, 3, "After emoji"),
                lens(1, 2, "Inside surrogate"),
            ],
        );

        assert_eq!(lenses, vec![lens(1, 1, "Start"), lens(1, 2, "After emoji")]);
    }

    #[test]
    fn valid_code_lenses_sort_after_validation() {
        let buffer = TextBuffer::from_text(1, None, "alpha\nbeta\ngamma".to_owned());
        let lenses = valid_code_lenses_for_buffer(
            &buffer,
            vec![
                lens(2, 3, "Run z"),
                lens(1, 5, "Run b"),
                lens(1, 5, "Run a"),
                lens(3, 1, "Run last"),
                lens(1, 1, "Run first"),
            ],
        );

        assert_eq!(
            lenses,
            vec![
                lens(1, 1, "Run first"),
                lens(1, 5, "Run a"),
                lens(1, 5, "Run b"),
                lens(2, 3, "Run z"),
                lens(3, 1, "Run last"),
            ]
        );
    }

    #[test]
    fn code_lens_status_labels_borrow_clean_ascii_and_unicode() {
        match code_lens_status_title("Run Test") {
            Cow::Borrowed(label) => assert_eq!(label, "Run Test"),
            Cow::Owned(label) => panic!("expected borrowed title label, got {label:?}"),
        }

        let unicode_title = "Run \u{03bb}";
        match code_lens_status_title(unicode_title) {
            Cow::Borrowed(label) => assert_eq!(label, unicode_title),
            Cow::Owned(label) => panic!("expected borrowed unicode title, got {label:?}"),
        }

        match code_lens_status_command("rust-analyzer.runSingle") {
            Cow::Borrowed(label) => assert_eq!(label, "rust-analyzer.runSingle"),
            Cow::Owned(label) => panic!("expected borrowed command label, got {label:?}"),
        }

        let unicode_command = "rust-analyzer.\u{5b9f}\u{884c}";
        match code_lens_status_command(unicode_command) {
            Cow::Borrowed(label) => assert_eq!(label, unicode_command),
            Cow::Owned(label) => panic!("expected borrowed unicode command, got {label:?}"),
        }
    }

    #[test]
    fn code_lens_status_labels_own_dirty_truncated_and_fallback_values() {
        let dirty_title = code_lens_status_title("Run\n\u{202e}Target");
        assert_eq!(dirty_title.as_ref(), "Run Target");
        assert!(matches!(&dirty_title, Cow::Owned(_)));

        let long_title = "title-".repeat(CODE_LENS_STATUS_TITLE_MAX_CHARS);
        let truncated_title = code_lens_status_title(&long_title);
        assert!(truncated_title.contains("..."), "{truncated_title}");
        assert!(truncated_title.chars().count() <= CODE_LENS_STATUS_TITLE_MAX_CHARS);
        assert!(matches!(&truncated_title, Cow::Owned(_)));

        let fallback_title = code_lens_status_title("\n\t\u{202e}");
        assert_eq!(fallback_title.as_ref(), "code lens");
        assert!(matches!(&fallback_title, Cow::Owned(_)));

        let dirty_command = code_lens_status_command("cmd\n\u{202e}run");
        assert_eq!(dirty_command.as_ref(), "cmd run");
        assert!(matches!(&dirty_command, Cow::Owned(_)));

        let long_command = "command-".repeat(CODE_LENS_STATUS_COMMAND_MAX_CHARS);
        let truncated_command = code_lens_status_command(&long_command);
        assert!(truncated_command.contains("..."), "{truncated_command}");
        assert!(truncated_command.chars().count() <= CODE_LENS_STATUS_COMMAND_MAX_CHARS);
        assert!(matches!(&truncated_command, Cow::Owned(_)));

        let fallback_command = code_lens_status_command("\n\t\u{202e}");
        assert_eq!(fallback_command.as_ref(), "command");
        assert!(matches!(&fallback_command, Cow::Owned(_)));
    }

    #[test]
    fn code_lens_resolve_result_merges_resolved_lens() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root.clone());
        let buffer = TextBuffer::from_text(7, Some(path.clone()), "fn main() {}\n".to_owned());
        let version = buffer.version();
        app.buffers.push(buffer);
        app.settings.code_lens = true;
        app.code_lenses.insert(path.clone(), Vec::new());

        handle_code_lens_resolve_result(
            &mut app,
            7,
            path.clone(),
            version,
            Some(lens(1, 1, "Run Test")),
            None,
        );

        let lenses = app.code_lenses.get(&path).expect("resolved code lens");
        assert_eq!(lenses.len(), 1);
        assert_eq!(lenses[0].title, "Run Test");
        assert!(app.status.starts_with("Resolved code lens in "));
    }

    #[test]
    fn code_lens_resolve_result_without_active_batch_is_ignored() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root.clone());
        let buffer = TextBuffer::from_text(7, Some(path.clone()), "fn main() {}\n".to_owned());
        let version = buffer.version();
        app.buffers.push(buffer);
        app.settings.code_lens = true;
        app.status = "before".to_owned();

        handle_code_lens_resolve_result(
            &mut app,
            7,
            path.clone(),
            version,
            Some(lens(1, 1, "Run Test")),
            None,
        );

        assert!(!app.code_lenses.contains_key(&path));
        assert_eq!(app.status, "before");
    }

    #[test]
    fn unresolved_code_lenses_keep_active_batch_for_resolve_results() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root.clone());
        let buffer = TextBuffer::from_text(7, Some(path.clone()), "fn main() {}\n".to_owned());
        let version = buffer.version();
        app.buffers.push(buffer);
        app.settings.code_lens = true;
        app.lsp_clients.insert(
            "rust".to_owned(),
            crate::lsp_client::LspClientHandle::accepting_for_test(),
        );

        handle_code_lenses_result(
            &mut app,
            7,
            path.clone(),
            version,
            Some(vec![unresolved_lens(1, 1)]),
            None,
        );

        assert_eq!(app.code_lenses.get(&path), Some(&Vec::new()));
        assert!(app.status.starts_with("Resolving 1 code lenses in "));
    }

    #[test]
    fn stale_code_lens_resolve_result_is_ignored() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root.clone());
        let buffer = TextBuffer::from_text(7, Some(path.clone()), "fn main() {}\n".to_owned());
        let version = buffer.version();
        app.buffers.push(buffer);
        app.settings.code_lens = true;
        app.status = "before".to_owned();

        handle_code_lens_resolve_result(
            &mut app,
            7,
            path.clone(),
            version + 1,
            Some(lens(1, 1, "Run Test")),
            None,
        );

        assert!(!app.code_lenses.contains_key(&path));
        assert_eq!(app.status, "before");
    }

    #[test]
    fn code_lens_error_statuses_sanitize_and_bound_lsp_error_text() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let (mut app, version) = app_with_lsp_buffer(root.clone(), path.clone());
        let raw_error = unsafe_status_text("first line\nsecond line");

        handle_code_lenses_result(
            &mut app,
            7,
            path.clone(),
            version,
            None,
            Some(raw_error.clone()),
        );

        assert_safe_status_text(&app.status);
        assert_safe_status_error(&app.status, "Code lenses failed: ");

        let (mut app, version) = app_with_lsp_buffer(root, path.clone());
        handle_code_lens_resolve_result(&mut app, 7, path, version, None, Some(raw_error));

        assert_safe_status_text(&app.status);
        assert_safe_status_error(&app.status, "Code lens resolve failed: ");
    }

    #[test]
    fn code_lens_path_statuses_sanitize_and_bound_file_labels() {
        let root = PathBuf::from("workspace");
        let path = root.join(format!(
            "bad\n{}\u{202e}tail.rs",
            "very-long-path-".repeat(DISPLAY_PATH_LABEL_MAX_CHARS)
        ));
        let (mut app, version) = app_with_lsp_buffer(root, path.clone());

        handle_code_lenses_result(&mut app, 7, path.clone(), version, Some(Vec::new()), None);

        assert_safe_status_text(&app.status);
        assert!(app.status.starts_with("No code lenses in "));
        assert!(app.status.contains("..."));
        assert!(
            app.status.chars().count()
                <= "No code lenses in ".chars().count() + DISPLAY_PATH_LABEL_MAX_CHARS
        );

        handle_code_lens_resolve_result(&mut app, 7, path, version, None, None);

        assert_safe_status_text(&app.status);
        assert!(
            app.status
                .starts_with("Code lens resolve returned no displayable command for ")
        );
        assert!(app.status.contains("..."));
        assert!(
            app.status.chars().count()
                <= "Code lens resolve returned no displayable command for "
                    .chars()
                    .count()
                    + DISPLAY_PATH_LABEL_MAX_CHARS
        );
    }

    #[test]
    fn code_lens_resolve_trace_detail_sanitizes_and_bounds_file_label() {
        let path = PathBuf::from("workspace").join(format!(
            "lens\n{}\u{202e}tail.rs",
            "very-long-path-".repeat(DISPLAY_PATH_LABEL_MAX_CHARS)
        ));

        let detail = code_lens_resolve_trace_detail(&path);

        assert!(!detail.contains('\n'));
        assert!(!detail.contains('\u{202e}'));
        assert!(detail.contains("..."));
        assert!(detail.chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS);
    }

    #[test]
    fn code_lens_command_status_sanitizes_and_bounds_lsp_title_command_and_error() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let (mut app, version) = app_with_lsp_buffer(root.clone(), path.clone());
        let raw_title = unsafe_status_text("Run\nTests");
        let raw_command = unsafe_status_text("workspace.execute\nlens");
        let raw_error = unsafe_status_text("boom\nfailed");

        handle_code_lens_command_result(
            &mut app,
            7,
            path.clone(),
            version,
            raw_title.clone(),
            raw_command.clone(),
            Some(raw_error),
        );

        assert_safe_status_text(&app.status);
        assert_safe_status_title(&app.status, "Code lens `", "` failed: ");
        assert_safe_status_error(&app.status, "` failed: ");

        handle_code_lens_command_result(&mut app, 7, path, version, raw_title, raw_command, None);

        assert_safe_status_text(&app.status);
        assert_safe_status_title(&app.status, "Code lens `", "` executed (");
        assert_safe_status_command(&app.status);
    }

    #[test]
    fn resolved_code_lens_status_hardening_keeps_raw_lsp_title_command_and_payload() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let (mut app, version) = app_with_lsp_buffer(root, path.clone());
        app.code_lenses.insert(path.clone(), Vec::new());
        let raw_title = unsafe_status_text("Run\nTarget");
        let raw_command = unsafe_status_text("workspace.execute\nlens");
        let arguments = Arc::new(json!({
            "raw": raw_title.clone(),
            "command": raw_command.clone(),
        }));
        let payload = Arc::new(json!({
            "data": { "id": "raw\npayload", "direction": "\u{202e}" }
        }));
        let lens = LspCodeLens {
            line: 1,
            column: 1,
            title: raw_title.clone(),
            command: Some(raw_command.clone()),
            command_arguments: Some(arguments.clone()),
            resolve_payload: Some(payload.clone()),
        };

        handle_code_lens_resolve_result(&mut app, 7, path.clone(), version, Some(lens), None);

        let lenses = app.code_lenses.get(&path).expect("resolved code lens");
        assert_eq!(lenses.len(), 1);
        assert_eq!(lenses[0].title, raw_title);
        assert_eq!(lenses[0].command.as_deref(), Some(raw_command.as_str()));
        assert_eq!(lenses[0].command_arguments.as_ref(), Some(&arguments));
        assert_eq!(lenses[0].resolve_payload.as_ref(), Some(&payload));
        assert_eq!(app.status, format!("Resolved code lens in {}", "main.rs"));
    }

    fn lens(line: usize, column: usize, title: &str) -> LspCodeLens {
        LspCodeLens {
            line,
            column,
            title: title.to_owned(),
            command: None,
            command_arguments: None,
            resolve_payload: None,
        }
    }

    fn unresolved_lens(line: usize, column: usize) -> LspCodeLens {
        LspCodeLens {
            line,
            column,
            title: String::new(),
            command: None,
            command_arguments: None,
            resolve_payload: Some(Arc::new(json!({
                "range": {
                    "start": { "line": line.saturating_sub(1), "character": column.saturating_sub(1) },
                    "end": { "line": line.saturating_sub(1), "character": column.saturating_sub(1) }
                },
                "data": { "id": "lens" }
            }))),
        }
    }

    fn app_with_lsp_buffer(root: PathBuf, path: PathBuf) -> (KuroyaApp, u64) {
        let mut app = app_for_test(root);
        let buffer = TextBuffer::from_text(7, Some(path), "fn main() {}\n".to_owned());
        let version = buffer.version();
        app.buffers.push(buffer);
        app.settings.code_lens = true;
        (app, version)
    }

    fn unsafe_status_text(prefix: &str) -> String {
        format!(
            "{prefix}\u{202e}{}tail\u{2029}",
            "very-long-lsp-text-".repeat(DISPLAY_ERROR_LABEL_MAX_CHARS)
        )
    }

    fn assert_safe_status_text(status: &str) {
        assert!(
            !status.chars().any(is_unsafe_status_char),
            "status contains unsafe display characters: {status:?}"
        );
    }

    fn assert_safe_status_title(status: &str, prefix: &str, suffix: &str) {
        let title = status_segment(status, prefix, suffix);

        assert!(title.contains("..."), "{status}");
        assert!(title.chars().count() <= CODE_LENS_STATUS_TITLE_MAX_CHARS);
    }

    fn assert_safe_status_command(status: &str) {
        let command = status_segment(status, "` executed (", ")");

        assert!(command.contains("..."), "{status}");
        assert!(command.chars().count() <= CODE_LENS_STATUS_COMMAND_MAX_CHARS);
    }

    fn assert_safe_status_error(status: &str, prefix: &str) {
        let error = status
            .rsplit_once(prefix)
            .map(|(_, value)| value)
            .unwrap_or_else(|| panic!("unexpected status: {status}"));

        assert!(error.contains("..."), "{status}");
        assert!(error.chars().count() <= DISPLAY_ERROR_LABEL_MAX_CHARS);
    }

    fn status_segment<'a>(status: &'a str, prefix: &str, suffix: &str) -> &'a str {
        status
            .split_once(prefix)
            .and_then(|(_, value)| value.split_once(suffix).map(|(segment, _)| segment))
            .unwrap_or_else(|| panic!("unexpected status: {status}"))
    }

    fn is_unsafe_status_char(ch: char) -> bool {
        ch.is_control()
            || matches!(
                ch,
                '\u{061c}'
                    | '\u{200e}'
                    | '\u{200f}'
                    | '\u{2028}'..='\u{202e}'
                    | '\u{2066}'..='\u{2069}'
            )
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
}
