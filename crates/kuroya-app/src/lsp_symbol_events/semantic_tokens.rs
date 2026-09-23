use crate::{
    KuroyaApp,
    lsp_symbol_events::position::lsp_span_within_buffer,
    path_display::{display_error_label_cow, display_path_label_cow},
    workspace_state::{buffer_id_path_version_matches, lsp_event_path_is_current},
};
use kuroya_core::{BufferId, LspSemanticToken, TextBuffer};
use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
};

pub(super) fn handle_semantic_tokens_result(
    app: &mut KuroyaApp,
    id: BufferId,
    path: PathBuf,
    version: u64,
    tokens: Option<Vec<LspSemanticToken>>,
    error: Option<String>,
) {
    if !lsp_event_path_is_current(&app.workspace.root, &path)
        || !buffer_id_path_version_matches(&app.buffers, id, &path, version)
    {
        return;
    }
    if let Some(error) = error {
        app.semantic_tokens.remove(&path);
        app.status = semantic_tokens_failed_status(&error);
    } else if let Some(tokens) = tokens {
        let Some(buffer) = app.buffer(id) else {
            return;
        };
        let tokens = valid_semantic_tokens_for_buffer(buffer, tokens);
        let count = tokens.len();
        if tokens.is_empty() {
            app.semantic_tokens.remove(&path);
        } else {
            app.semantic_tokens.insert(path.clone(), tokens);
        }
        app.status = semantic_tokens_loaded_status(count, &path);
    } else {
        app.semantic_tokens.remove(&path);
        app.status = semantic_tokens_load_failed_status(&path);
    }
}

fn semantic_tokens_failed_status(error: &str) -> String {
    let error = display_error_label_cow(error);
    let mut status = String::with_capacity("Semantic tokens failed: ".len() + error.len());
    status.push_str("Semantic tokens failed: ");
    status.push_str(error.as_ref());
    status
}

fn semantic_tokens_loaded_status(count: usize, path: &Path) -> String {
    let path = display_path_label_cow(path);
    let path = path.as_ref();
    if count == 0 {
        let mut status = String::with_capacity("No semantic tokens in ".len() + path.len());
        status.push_str("No semantic tokens in ");
        status.push_str(path);
        status
    } else {
        let mut status = String::with_capacity(28 + path.len());
        let _ = write!(status, "{count} semantic tokens in {path}");
        status
    }
}

fn semantic_tokens_load_failed_status(path: &Path) -> String {
    let path = display_path_label_cow(path);
    let path = path.as_ref();
    let mut status =
        String::with_capacity("Could not load semantic tokens for ".len() + path.len());
    status.push_str("Could not load semantic tokens for ");
    status.push_str(path);
    status
}

fn valid_semantic_tokens_for_buffer(
    buffer: &TextBuffer,
    tokens: Vec<LspSemanticToken>,
) -> Vec<LspSemanticToken> {
    let mut valid_tokens = Vec::with_capacity(tokens.len());
    for token in tokens {
        if lsp_span_within_buffer(buffer, token.line, token.column, token.length) {
            valid_tokens.push(token);
        }
    }
    valid_tokens
}

#[cfg(test)]
mod tests {
    use super::{
        handle_semantic_tokens_result, semantic_tokens_failed_status,
        semantic_tokens_load_failed_status, semantic_tokens_loaded_status,
        valid_semantic_tokens_for_buffer,
    };
    use crate::{
        KuroyaApp,
        app_startup_context::AppStartupContext,
        path_display::{DISPLAY_ERROR_LABEL_MAX_CHARS, DISPLAY_PATH_LABEL_MAX_CHARS},
        terminal::TerminalPane,
    };
    use kuroya_core::{BufferId, EditorSettings, LspSemanticToken, TextBuffer, Workspace};
    use std::{
        path::{Path, PathBuf},
        time::Instant,
    };
    use tokio::runtime::Runtime;

    #[test]
    fn valid_semantic_tokens_filter_out_of_bounds_spans() {
        let buffer = TextBuffer::from_text(1, None, "alpha\nbeta".to_owned());
        let tokens = valid_semantic_tokens_for_buffer(
            &buffer,
            vec![
                token(1, 1, 5, "function"),
                token(2, 2, 3, "variable"),
                // A length reaching past the line is a multi-line token (e.g.
                // block comment): kept at intake, clamped to the first line at
                // rendering time.
                token(1, 1, 6, "multi-line"),
                token(2, 5, 1, "at-end"),
                token(3, 1, 1, "missing"),
            ],
        );

        assert_eq!(
            tokens,
            vec![
                token(1, 1, 5, "function"),
                token(2, 2, 3, "variable"),
                token(1, 1, 6, "multi-line")
            ]
        );
    }

    #[test]
    fn valid_semantic_tokens_filter_utf16_surrogate_splits() {
        let buffer = TextBuffer::from_text(1, None, "😀x".to_owned());
        let tokens = valid_semantic_tokens_for_buffer(
            &buffer,
            vec![
                token(1, 1, 2, "emoji"),
                token(1, 3, 1, "variable"),
                token(1, 1, 1, "split-emoji"),
                token(1, 2, 1, "inside-surrogate"),
            ],
        );

        assert_eq!(
            tokens,
            vec![token(1, 1, 2, "emoji"), token(1, 3, 1, "variable")]
        );
    }

    #[test]
    fn semantic_tokens_failed_status_sanitizes_and_bounds_lsp_error_text() {
        let status = semantic_tokens_failed_status(&unsafe_error_text());

        assert_safe_status_text(&status);
        assert!(status.starts_with("Semantic tokens failed: "));
        assert!(status.contains("..."));
        assert!(
            status.chars().count()
                <= "Semantic tokens failed: ".chars().count() + DISPLAY_ERROR_LABEL_MAX_CHARS
        );
    }

    #[test]
    fn semantic_tokens_path_statuses_sanitize_and_bound_file_labels() {
        let path = unsafe_path();

        let loaded = semantic_tokens_loaded_status(3, &path);
        let empty = semantic_tokens_loaded_status(0, &path);
        let failed = semantic_tokens_load_failed_status(&path);

        assert_raw_path_keeps_hostile_text(&path);
        for (prefix, status) in [
            ("3 semantic tokens in ", loaded),
            ("No semantic tokens in ", empty),
            ("Could not load semantic tokens for ", failed),
        ] {
            assert_safe_status_text(&status);
            assert!(status.starts_with(prefix));
            assert!(status.contains("..."));
            assert!(
                status.chars().count() <= prefix.chars().count() + DISPLAY_PATH_LABEL_MAX_CHARS
            );
        }
    }

    #[test]
    fn semantic_tokens_result_preserves_raw_tokens_while_status_uses_path_label() {
        let root = PathBuf::from("workspace");
        let path = unsafe_path_under(&root);
        let mut app = app_for_test(root);
        let version = push_buffer(&mut app, 7, path.clone());
        let raw_token_type = unsafe_lsp_text("token-type");
        let raw_modifier = unsafe_lsp_text("modifier");
        let raw_token = LspSemanticToken {
            line: 1,
            column: 1,
            length: 2,
            token_type: raw_token_type.clone(),
            modifiers: vec![raw_modifier.clone()],
        };

        handle_semantic_tokens_result(
            &mut app,
            7,
            path.clone(),
            version,
            Some(vec![raw_token.clone()]),
            None,
        );

        assert_safe_status_text(&app.status);
        assert!(app.status.starts_with("1 semantic tokens in "));
        assert_eq!(app.semantic_tokens.get(&path), Some(&vec![raw_token]));
        assert!(raw_token_type.contains('\n'));
        assert!(raw_token_type.contains('\u{202e}'));
        assert!(raw_modifier.contains('\n'));
        assert!(raw_modifier.contains('\u{202e}'));
    }

    fn token(line: usize, column: usize, length: usize, token_type: &str) -> LspSemanticToken {
        LspSemanticToken {
            line,
            column,
            length,
            token_type: token_type.to_owned(),
            modifiers: Vec::new(),
        }
    }

    fn push_buffer(app: &mut KuroyaApp, id: BufferId, path: PathBuf) -> u64 {
        let buffer = TextBuffer::from_text(id, Some(path), "fn main() {}\n".to_owned());
        let version = buffer.version();
        app.buffers.push(buffer);
        version
    }

    fn unsafe_path() -> PathBuf {
        unsafe_path_under(Path::new("workspace"))
    }

    fn unsafe_path_under(root: &Path) -> PathBuf {
        root.join(format!(
            "bad\n{}\u{202e}tail.rs",
            "very-long-path-".repeat(DISPLAY_PATH_LABEL_MAX_CHARS)
        ))
    }

    fn unsafe_error_text() -> String {
        format!(
            "first\nsecond\u{202e}{}tail",
            "very-long-error-".repeat(DISPLAY_ERROR_LABEL_MAX_CHARS)
        )
    }

    fn unsafe_lsp_text(prefix: &str) -> String {
        format!(
            "{prefix}\nvalue\u{202e}{}tail",
            "very-long-lsp-text-".repeat(DISPLAY_ERROR_LABEL_MAX_CHARS)
        )
    }

    fn assert_raw_path_keeps_hostile_text(path: &Path) {
        let raw = path.to_string_lossy();
        assert!(raw.contains('\n'));
        assert!(raw.contains('\u{202e}'));
    }

    fn assert_safe_status_text(status: &str) {
        assert!(
            !status.chars().any(is_unsafe_status_char),
            "status contains unsafe display characters: {status:?}"
        );
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
