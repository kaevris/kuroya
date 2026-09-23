pub(super) use super::super::*;
pub(super) use crate::{
    KuroyaApp,
    app_startup_context::AppStartupContext,
    app_state::{PendingFileReload, QueuedFileReload},
    large_file_mode::LARGE_FILE_MODE_MAX_LINES,
    lsp_client::LspClientHandle,
    lsp_ui_events::LspUiEvent,
    path_display::DISPLAY_ERROR_LABEL_MAX_CHARS,
    terminal::TerminalPane,
};
pub(super) use kuroya_core::{
    EditorSettings, LspRequestId, LspTextEdit, LspWorkspaceDocumentChange, TextBuffer, Workspace,
    lsp::path_to_file_uri, parse_apply_workspace_edit_request,
};
pub(super) use serde_json::json;
pub(super) use std::{
    borrow::Cow,
    collections::BTreeMap,
    fs,
    path::PathBuf,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
pub(super) use tokio::runtime::Runtime;

pub(super) fn assert_workspace_status_is_display_safe(status: &str) {
    assert!(
        !status.chars().any(is_unsafe_status_display_char),
        "{status:?}"
    );
}

pub(super) fn assert_status_error_detail_is_bounded(status: &str) {
    for prefix in [
        "LSP workspace edit response failed: ",
        "LSP workspace edit rejected: ",
    ] {
        if let Some(detail) = status.strip_prefix(prefix) {
            assert!(
                detail.chars().count() <= DISPLAY_ERROR_LABEL_MAX_CHARS,
                "{status:?}"
            );
            return;
        }
    }
    panic!("unexpected workspace edit rejection status: {status:?}");
}

pub(super) fn is_unsafe_status_display_char(ch: char) -> bool {
    ch.is_control()
        || matches!(
            ch,
            '\u{061c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
        )
}

pub(super) fn edit(path: &std::path::Path, new_text: &str) -> LspTextEdit {
    LspTextEdit {
        path: path.to_path_buf(),
        start_line: 1,
        start_column: 1,
        end_line: 1,
        end_column: 1,
        new_text: new_text.to_owned(),
    }
}

#[cfg(unix)]
pub(super) fn create_file_symlink_for_test(
    target: &std::path::Path,
    link: &std::path::Path,
) -> bool {
    std::os::unix::fs::symlink(target, link).is_ok()
}

#[cfg(windows)]
pub(super) fn create_file_symlink_for_test(
    target: &std::path::Path,
    link: &std::path::Path,
) -> bool {
    std::os::windows::fs::symlink_file(target, link).is_ok()
}

#[cfg(not(any(unix, windows)))]
pub(super) fn create_file_symlink_for_test(
    _target: &std::path::Path,
    _link: &std::path::Path,
) -> bool {
    false
}

pub(super) fn app_for_test(root: PathBuf) -> KuroyaApp {
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

pub(super) fn drain_until_lsp_workspace_event(app: &mut KuroyaApp) {
    for _ in 0..500 {
        if app.handle_events() > 0 {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for LSP workspace edit event");
}

pub(super) fn assert_owned_cow_eq(label: Cow<'_, str>, expected: &str) {
    assert_eq!(label.as_ref(), expected);
    assert!(matches!(label, Cow::Owned(_)));
}

pub(super) fn temp_workspace(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("kuroya-lsp-{name}-{}-{nanos}", std::process::id()))
}
