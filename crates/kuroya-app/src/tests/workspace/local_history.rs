use crate::{
    KuroyaApp, app_startup_context::AppStartupContext, file_history::LOCAL_HISTORY_MAX_BYTES,
    terminal::TerminalPane,
};
use kuroya_core::{Command, EditorSettings, LanguageId, TextBuffer, Workspace};
use std::{
    path::PathBuf,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tokio::runtime::Runtime;

#[test]
fn local_history_loaded_opens_snapshot_as_read_only_virtual_revision_buffer() {
    let root = temp_root("local-history-valid-loaded");
    let path = root.join("src/main.rs");
    let snapshot_path = root.join(".kuroya/history/src/7.main.rs.bak");
    let mut app = app_for_test(root.clone());

    app.apply_local_history_loaded(
        root,
        app.workspace_event_generation,
        path,
        snapshot_path,
        7,
        "fn old() {}\n".to_owned(),
    );

    let id = app
        .active
        .expect("local history buffer should become active");
    let buffer = app
        .buffer(id)
        .expect("local history buffer should be opened");

    assert_eq!(app.buffers.len(), 1);
    assert_eq!(buffer.text(), "fn old() {}\n");
    assert!(buffer.is_read_only());
    assert_eq!(buffer.path(), None);
    assert_eq!(buffer.language(), LanguageId::Rust);
    assert_eq!(
        app.virtual_buffer_labels.get(&id).map(String::as_str),
        Some("main.rs (Local History)")
    );
    assert!(!app.diff_buffer_sources.contains_key(&id));
    assert_eq!(
        app.status,
        "Opened local history for main.rs from 7.main.rs.bak"
    );
}

#[test]
fn local_history_loaded_rejects_binary_snapshot_text_without_opening_revision_buffer() {
    let root = temp_root("local-history-binary-loaded");
    let path = root.join("src/main.rs");
    let snapshot_path = root.join(".kuroya/history/src/1.main.rs.bak");
    let mut app = app_for_test(root.clone());

    app.apply_local_history_loaded(
        root,
        app.workspace_event_generation,
        path,
        snapshot_path,
        1,
        "old\0snapshot".to_owned(),
    );

    assert!(app.virtual_buffer_labels.is_empty());
    assert_eq!(
        app.status,
        "Could not open local history for main.rs: snapshot contains binary data"
    );
}

#[test]
fn local_history_loaded_rejects_oversized_snapshot_text_without_opening_revision_buffer() {
    let root = temp_root("local-history-oversized-loaded");
    let path = root.join("src/main.rs");
    let snapshot_path = root.join(".kuroya/history/src/1.main.rs.bak");
    let mut app = app_for_test(root.clone());

    app.apply_local_history_loaded(
        root,
        app.workspace_event_generation,
        path,
        snapshot_path,
        1,
        "x".repeat(usize::try_from(LOCAL_HISTORY_MAX_BYTES).unwrap() + 1),
    );

    assert!(app.virtual_buffer_labels.is_empty());
    assert_eq!(
        app.status,
        "Could not open local history for main.rs: snapshot exceeds local history size limit"
    );
}

#[test]
fn local_history_browser_command_requires_active_file_then_toggles_state() {
    let root = temp_root("local-history-browser-toggle");
    let path = root.join("src/main.rs");
    let mut app = app_for_test(root.clone());

    // Without a file-backed active buffer the browser must not open.
    assert!(app.run_ui_command(&Command::OpenLocalHistoryBrowser));
    assert!(!app.local_history_browser_open);
    assert_eq!(app.local_history_browser_path, None);
    assert_eq!(app.status, "No active file to browse local history");

    let buffer = TextBuffer::from_text(1, Some(path.clone()), "current".to_owned());
    let id = buffer.id();
    app.buffers.push(buffer);
    app.set_active_buffer(id);

    assert!(app.run_ui_command(&Command::OpenLocalHistoryBrowser));
    assert!(app.local_history_browser_open);
    assert_eq!(app.local_history_browser_path, Some(path.clone()));
    assert_eq!(app.local_history_browser_selected, 0);
    assert!(app.local_history_browser_snapshots.is_empty());
    assert!(app.local_history_browser_loading);

    assert!(app.run_ui_command(&Command::OpenLocalHistoryBrowser));
    assert!(!app.local_history_browser_open);
    assert_eq!(app.local_history_browser_path, None);
    assert!(app.local_history_browser_snapshots.is_empty());
    assert!(!app.local_history_browser_loading);
    assert_eq!(app.status, "Closed local history browser");
}

#[test]
fn local_history_browser_loaded_results_replace_rows_and_clamp_selection() {
    let root = temp_root("local-history-browser-loaded");
    let path = root.join("src/main.rs");
    let mut app = app_for_test(root.clone());
    app.local_history_browser_open = true;
    app.local_history_browser_path = Some(path.clone());
    app.local_history_browser_selected = 9;
    app.local_history_browser_loading = true;
    let generation = app.workspace_event_generation;

    // Stale generation results are dropped.
    app.apply_local_history_browser_loaded(
        root.clone(),
        generation + 1,
        path.clone(),
        vec![snapshot_for_test(3)],
    );
    assert!(app.local_history_browser_snapshots.is_empty());

    // Results for another file are dropped.
    app.apply_local_history_browser_loaded(
        root.clone(),
        generation,
        root.join("src/other.rs"),
        vec![snapshot_for_test(3)],
    );
    assert!(app.local_history_browser_snapshots.is_empty());

    app.apply_local_history_browser_loaded(
        root,
        generation,
        path,
        vec![
            snapshot_for_test(3),
            snapshot_for_test(2),
            snapshot_for_test(1),
        ],
    );
    assert_eq!(
        app.local_history_browser_snapshots
            .iter()
            .map(|snapshot| snapshot.sequence)
            .collect::<Vec<_>>(),
        vec![3, 2, 1]
    );
    assert_eq!(app.local_history_browser_selected, 2);
    assert!(!app.local_history_browser_loading);
}

#[test]
fn local_history_browser_selection_opens_read_only_revision_buffer_with_sequence_label() {
    let root = temp_root("local-history-browser-selection");
    let path = root.join("src/main.rs");
    let snapshot_path = root.join(".kuroya/history/src/7.main.rs.bak");
    let mut app = app_for_test(root.clone());

    app.apply_local_history_browser_snapshot_loaded(
        root.clone(),
        app.workspace_event_generation,
        path,
        snapshot_path,
        7,
        Some(snapshot_timestamp_for_test()),
        Ok("fn old() {}\n".to_owned()),
    );

    let id = app.active.expect("snapshot buffer should become active");
    let buffer = app.buffer(id).expect("snapshot buffer should be opened");
    assert_eq!(buffer.text(), "fn old() {}\n");
    assert!(buffer.is_read_only());
    assert_eq!(buffer.path(), None);
    assert_eq!(
        app.virtual_buffer_labels.get(&id).map(String::as_str),
        Some("main.rs (Local History #7, 2023-11-14 22:13:20 UTC)")
    );
    assert_eq!(
        app.status,
        "Opened local history snapshot 7 for main.rs from 7.main.rs.bak"
    );
}

#[test]
fn local_history_browser_selection_reports_failures_without_opening_buffers() {
    let root = temp_root("local-history-browser-failures");
    let path = root.join("src/main.rs");
    let snapshot_path = root.join(".kuroya/history/src/7.main.rs.bak");
    let mut app = app_for_test(root.clone());

    app.apply_local_history_browser_snapshot_loaded(
        root.clone(),
        app.workspace_event_generation,
        path.clone(),
        snapshot_path.clone(),
        7,
        None,
        Err("snapshot is no longer on disk".to_owned()),
    );
    assert!(app.virtual_buffer_labels.is_empty());
    assert_eq!(
        app.status,
        "Could not open local history snapshot 7 for main.rs: snapshot is no longer on disk"
    );

    app.apply_local_history_browser_snapshot_loaded(
        root,
        app.workspace_event_generation,
        path,
        snapshot_path,
        7,
        None,
        Ok("old\0snapshot".to_owned()),
    );
    assert!(app.virtual_buffer_labels.is_empty());
    assert_eq!(
        app.status,
        "Could not open local history snapshot 7 for main.rs: snapshot contains binary data"
    );
}

#[test]
fn local_history_browser_refreshes_enumeration_after_save_of_tracked_file() {
    let root = temp_root("local-history-browser-save-refresh");
    let path = root.join("src/main.rs");
    let mut app = app_for_test(root.clone());
    app.local_history_browser_open = true;
    app.local_history_browser_path = Some(path.clone());

    app.local_history_browser_loading = false;
    app.refresh_local_history_browser_after_save(&root.join("src/other.rs"));
    assert!(
        !app.local_history_browser_loading,
        "saves for unrelated files must not refresh the browser"
    );

    app.refresh_local_history_browser_after_save(&path);
    assert!(
        app.local_history_browser_loading,
        "a completed save for the tracked file re-enumerates snapshots"
    );
}

fn snapshot_for_test(sequence: u128) -> crate::file_history::LocalHistorySnapshot {
    crate::file_history::LocalHistorySnapshot {
        sequence,
        path: PathBuf::from(format!(".kuroya/history/src/{sequence}.main.rs.bak")),
        bytes: 3,
        modified: Some(snapshot_timestamp_for_test()),
    }
}

fn snapshot_timestamp_for_test() -> std::time::SystemTime {
    std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000)
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

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("kuroya-{name}-{}-{nanos}", std::process::id()))
}
