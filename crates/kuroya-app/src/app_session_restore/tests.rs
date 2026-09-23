use super::*;
use crate::{
    app_startup_context::AppStartupContext,
    buffer_find::BufferFindScope,
    commands::keybinding_chord_for_command,
    keybinding_input::CapturedKeybinding,
    keybindings_panel_actions::PendingKeybindingsPanelActions,
    lsp_client::LspClientHandle,
    persistence::{
        BufferHistoryState, BufferViewState, PaneBufferViewState, PersistedSession,
        PersistedTerminalSession, RecoveredBuffer, SkippedRecoveredBuffer,
    },
    quick_open::QuickOpenQueryMemoryEntry,
    terminal::TerminalPane,
    transient_state::EditorImePreedit,
    workspace_state::settings_path,
};
use kuroya_core::{
    BufferHistorySnapshot, Command, EditorSettings, GitCommitSummary, GitStashEntry, TextBuffer,
    Workspace, keymap::KeyBinding,
};
use std::{
    cell::Cell,
    collections::HashMap,
    env, fs,
    path::PathBuf,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tokio::runtime::Runtime;

#[test]
fn restore_session_prunes_missing_pending_paths_and_active_wait() {
    let root = temp_root("missing-active");
    let existing = root.join("src/lib.rs");
    let missing = root.join("src/main.rs");
    fs::create_dir_all(existing.parent().unwrap()).unwrap();
    fs::write(&existing, "pub fn lib() {}\n").unwrap();

    let mut app = app_for_test(root.clone());
    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        open_files: vec![missing.clone(), existing.clone()],
        active_path: Some(missing.clone()),
        pane_paths: vec![Some(missing.clone()), Some(existing.clone())],
        view_states: vec![view_state(missing.clone()), view_state(existing.clone())],
        history_states: vec![
            history_state(missing.clone()),
            history_state(existing.clone()),
        ],
        recent_projects: Vec::new(),
        ..PersistedSession::default()
    });

    assert_eq!(app.pending_active_path, None);
    assert!(!app.pending_open_paths.contains(&missing));
    assert!(app.pending_open_paths.contains(&existing));
    assert!(!app.pending_pane_paths.values().any(|path| path == &missing));
    assert!(
        app.pending_pane_paths
            .values()
            .any(|path| path == &existing)
    );
    assert_eq!(app.panes.len(), 1);
    assert_eq!(app.panes[0].active, None);
    assert_eq!(
        app.pending_pane_paths.get(&app.panes[0].id),
        Some(&existing)
    );
    assert_eq!(app.active_pane, app.panes[0].id);
    assert!(!app.pending_view_states.contains_key(&missing));
    assert!(app.pending_view_states.contains_key(&existing));
    assert!(!app.pending_history_states.contains_key(&missing));
    assert!(app.pending_history_states.contains_key(&existing));

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_open_files_skip_recovered_equivalent_paths_before_openability_probe() {
    let recovered = PathBuf::from("workspace/src/main.rs");
    let equivalent = PathBuf::from("workspace/src/../src/main.rs");
    let restored_by_path = HashMap::from([(recovered, 7)]);
    let probes = Cell::new(0usize);

    let open_files =
        restorable_session_open_files(vec![equivalent], &[], &restored_by_path, |_| {
            probes.set(probes.get() + 1);
            true
        });

    assert!(open_files.is_empty());
    assert_eq!(probes.get(), 0);
}

#[cfg(windows)]
#[test]
fn restore_open_files_skip_recovered_case_equivalent_paths_before_openability_probe() {
    let recovered = PathBuf::from("C:/workspace/src/main.rs");
    let equivalent = PathBuf::from("c:/workspace/SRC/../src/MAIN.rs");
    let restored_by_path = HashMap::from([(recovered, 7)]);
    let probes = Cell::new(0usize);

    let open_files =
        restorable_session_open_files(vec![equivalent], &[], &restored_by_path, |_| {
            probes.set(probes.get() + 1);
            true
        });

    assert!(open_files.is_empty());
    assert_eq!(probes.get(), 0);
}

#[test]
fn restore_open_files_skip_duplicate_candidates_before_reprobing() {
    let path = PathBuf::from("workspace/src/generated.rs");
    let equivalent = PathBuf::from("workspace/src/../src/generated.rs");
    let restored_by_path = HashMap::new();
    let probes = Cell::new(0usize);

    let open_files = restorable_session_open_files(
        vec![path.clone(), equivalent],
        &[],
        &restored_by_path,
        |_| {
            probes.set(probes.get() + 1);
            true
        },
    );

    assert_eq!(open_files, vec![path]);
    assert_eq!(probes.get(), 1);
}

#[test]
fn build_session_drops_uncontained_opened_paths() {
    let root = PathBuf::from("workspace").join("current");
    let main = root.join("src").join("main.rs");
    let lib_alias = root.join("src").join("..").join("src").join("lib.rs");
    let outside = root.join("..").join("outside.rs");
    let reentry = root.join("..").join("current").join("secret.rs");
    let mut app = app_for_test(root.clone());
    let mut main_buffer = TextBuffer::from_text(1, Some(main.clone()), "main\n".to_owned());
    main_buffer.mark_dirty();
    app.buffers.push(main_buffer);
    let mut outside_buffer =
        TextBuffer::from_text(2, Some(outside.clone()), "outside\n".to_owned());
    outside_buffer.mark_dirty();
    app.buffers.push(outside_buffer);
    let mut lib_buffer = TextBuffer::from_text(3, Some(lib_alias), "lib\n".to_owned());
    lib_buffer.mark_dirty();
    app.buffers.push(lib_buffer);
    let mut reentry_buffer = TextBuffer::from_text(4, Some(reentry.clone()), "secret\n".to_owned());
    reentry_buffer.mark_dirty();
    app.buffers.push(reentry_buffer);
    app.active = Some(2);
    app.panes[0].active = Some(2);
    app.quick_open_recent_files
        .extend([main.clone(), outside.clone(), reentry.clone()]);
    app.quick_open_query_memory
        .push_back(QuickOpenQueryMemoryEntry {
            query: "main".to_owned(),
            path: main.clone(),
            uses: 1,
        });
    app.quick_open_query_memory
        .push_back(QuickOpenQueryMemoryEntry {
            query: "secret".to_owned(),
            path: reentry.clone(),
            uses: 1,
        });

    let session = app.build_session();

    assert_eq!(
        session.open_files,
        vec![main.clone(), root.join("src").join("lib.rs")]
    );
    assert_eq!(session.active_path, None);
    assert_eq!(session.pane_paths, vec![None]);
    assert_eq!(session.quick_open_recent_files, vec![main.clone()]);
    assert_eq!(session.quick_open_query_memory.len(), 1);
    assert_eq!(session.quick_open_query_memory[0].path, main.clone());
    assert!(
        session
            .view_states
            .iter()
            .all(|state| state.path != outside)
    );
    assert!(
        session
            .view_states
            .iter()
            .all(|state| state.path != reentry)
    );
    assert!(
        session
            .history_states
            .iter()
            .all(|state| state.path != outside && state.path != reentry)
    );
    assert!(
        session
            .recovery
            .iter()
            .any(|entry| entry.path.as_deref() == Some(main.as_path()))
    );
    assert!(
        session
            .recovery
            .iter()
            .filter(|entry| entry.path.is_none())
            .any(|entry| entry.text == "outside\n")
    );
    assert!(
        session
            .recovery
            .iter()
            .filter(|entry| entry.path.is_none())
            .any(|entry| entry.text == "secret\n")
    );
}

#[test]
fn restore_session_rejects_direct_uncontained_opened_paths() {
    let root = temp_root("direct-uncontained-opened");
    fs::create_dir_all(root.join("src")).unwrap();
    let main = root.join("src").join("main.rs");
    fs::write(&main, "main\n").unwrap();
    let outside = root.join("..").join("outside.rs");
    fs::write(&outside, "outside\n").unwrap();
    let reentry = root
        .join("..")
        .join(root.file_name().expect("root name"))
        .join("src")
        .join("main.rs");
    let mut app = app_for_test(root.clone());

    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        open_files: vec![main.clone(), outside.clone(), reentry.clone()],
        active_path: Some(reentry.clone()),
        pane_paths: vec![Some(reentry.clone()), Some(main.clone())],
        view_states: vec![
            view_state(main.clone()),
            view_state(outside.clone()),
            view_state(reentry.clone()),
        ],
        history_states: vec![
            history_state(main.clone()),
            history_state(outside.clone()),
            history_state(reentry.clone()),
        ],
        quick_open_recent_files: vec![main.clone(), outside.clone(), reentry.clone()],
        quick_open_query_memory: vec![
            QuickOpenQueryMemoryEntry {
                query: "main".to_owned(),
                path: main.clone(),
                uses: 1,
            },
            QuickOpenQueryMemoryEntry {
                query: "secret".to_owned(),
                path: reentry.clone(),
                uses: 1,
            },
        ],
        recent_projects: Vec::new(),
        recovery: Vec::new(),
        ..PersistedSession::default()
    });

    assert!(app.pending_open_paths.contains(&main));
    assert_eq!(
        app.quick_open_recent_files.iter().collect::<Vec<_>>(),
        vec![&main]
    );
    assert_eq!(app.quick_open_query_memory.len(), 1);
    assert_eq!(app.quick_open_query_memory[0].path, main.clone());
    assert!(!app.pending_open_paths.contains(&outside));
    assert!(!app.pending_open_paths.contains(&reentry));
    assert_eq!(app.pending_active_path, None);
    assert!(
        app.pending_pane_paths
            .values()
            .all(|path| path != &outside && path != &reentry)
    );
    assert!(app.pending_view_states.contains_key(&main));
    assert!(!app.pending_view_states.contains_key(&outside));
    assert!(!app.pending_view_states.contains_key(&reentry));
    assert!(app.pending_history_states.contains_key(&main));
    assert!(!app.pending_history_states.contains_key(&outside));
    assert!(!app.pending_history_states.contains_key(&reentry));
    drop(app);
    fs::remove_dir_all(root).unwrap();
    let _ = fs::remove_file(outside);
}

#[test]
fn restore_session_rejects_mismatched_workspace_path_state() {
    let root = temp_root("direct-mismatched-root");
    let other_root = temp_root("direct-mismatched-other");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(other_root.join("src")).unwrap();
    let root_main = root.join("src").join("main.rs");
    let other_main = other_root.join("src").join("main.rs");
    fs::write(&root_main, "current\n").unwrap();
    fs::write(&other_main, "other\n").unwrap();
    let mut app = app_for_test(root.clone());

    app.restore_session(PersistedSession {
        workspace_root: other_root.clone(),
        open_files: vec![PathBuf::from("src").join("main.rs"), other_main.clone()],
        active_path: Some(PathBuf::from("src").join("main.rs")),
        pane_paths: vec![Some(PathBuf::from("src").join("main.rs"))],
        view_states: vec![view_state(PathBuf::from("src").join("main.rs"))],
        history_states: vec![history_state(PathBuf::from("src").join("main.rs"))],
        recovery: vec![RecoveredBuffer {
            path: Some(other_main.clone()),
            display_name: "main.rs".to_owned(),
            text: "recover me\n".to_owned(),
        }],
        recovery_skipped: vec![SkippedRecoveredBuffer {
            path: Some(other_root.join("src").join("large.rs")),
            display_name: "large.rs".to_owned(),
            bytes: 4096,
            reason: "oversized".to_owned(),
        }],
        recent_projects: Vec::new(),
        ..PersistedSession::default()
    });

    assert!(app.pending_open_paths.is_empty());
    assert!(app.pending_pane_paths.is_empty());
    assert_eq!(app.pending_active_path, None);
    assert!(app.pending_view_states.is_empty());
    assert!(app.pending_history_states.is_empty());
    assert_eq!(app.buffers.len(), 1);
    assert_eq!(app.buffers[0].path(), None);
    assert_eq!(app.buffers[0].text(), "recover me\n");
    assert_eq!(app.status, "Restored 1 recovered buffers");
    drop(app);
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(other_root).unwrap();
}

#[test]
fn restore_session_reports_actual_recovery_skip_reasons() {
    let root = temp_root("restore-skip-reasons");
    fs::create_dir_all(&root).unwrap();

    let mut app = app_for_test(root.clone());
    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        recent_projects: Vec::new(),
        recovery: vec![RecoveredBuffer {
            path: None,
            display_name: "kept.rs".to_owned(),
            text: "recovered\n".to_owned(),
        }],
        recovery_skipped: vec![
            SkippedRecoveredBuffer {
                path: None,
                display_name: "large-a.rs".to_owned(),
                bytes: 4096,
                reason: "exceeds per-buffer recovery limit (1024 bytes)".to_owned(),
            },
            SkippedRecoveredBuffer {
                path: None,
                display_name: "large-b.rs".to_owned(),
                bytes: 4096,
                reason: "exceeds per-buffer recovery limit (1024 bytes)".to_owned(),
            },
            SkippedRecoveredBuffer {
                path: None,
                display_name: "dupe.rs".to_owned(),
                bytes: 32,
                reason: "duplicate recovery path already captured by newer dirty buffer".to_owned(),
            },
            SkippedRecoveredBuffer {
                path: None,
                display_name: "late.rs".to_owned(),
                bytes: 4096,
                reason: "exceeds total recovery limit (4096 bytes)".to_owned(),
            },
        ],
        ..PersistedSession::default()
    });

    assert_eq!(app.buffers.len(), 1);
    assert_eq!(
        app.status,
        "Restored 1 recovered buffers; skipped 4 buffers not snapshotted: \
         2 per-buffer limit, 1 duplicate path, 1 total limit"
    );
    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_session_notifies_lsp_for_recovered_dirty_disk_buffers() {
    let root = temp_root("restore-lsp-open");
    let source = root.join("src").join("main.rs");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, b"fn main() {}\n").unwrap();

    let mut app = app_for_test(root.clone());
    app.lsp_clients
        .insert("rust".to_owned(), LspClientHandle::accepting_for_test());

    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        recovery: vec![RecoveredBuffer {
            path: Some(source.clone()),
            display_name: "main.rs".to_owned(),
            text: "fn main() { recovered }\n".to_owned(),
        }],
        recent_projects: Vec::new(),
        ..PersistedSession::default()
    });

    assert_eq!(app.buffers.len(), 1);
    assert!(app.buffers[0].path().is_some());
    assert!(app.buffers[0].is_dirty());
    assert!(
        app.lsp_trace
            .iter()
            .any(|entry| entry.method == "textDocument/didOpen")
    );

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_session_skips_lsp_open_for_unpathed_recovered_buffers() {
    let root = temp_root("restore-lsp-open-unpathed");
    fs::create_dir_all(&root).unwrap();

    let mut app = app_for_test(root.clone());
    app.lsp_clients
        .insert("rust".to_owned(), LspClientHandle::accepting_for_test());

    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        recovery: vec![RecoveredBuffer {
            path: None,
            display_name: "scratch.rs".to_owned(),
            text: "recovered\n".to_owned(),
        }],
        recent_projects: Vec::new(),
        ..PersistedSession::default()
    });

    assert_eq!(app.buffers.len(), 1);
    assert_eq!(app.buffers[0].path(), None);
    assert!(app.lsp_trace.is_empty());
}

#[test]
fn restored_session_status_bounds_unknown_skip_reasons() {
    let skipped = vec![SkippedRecoveredBuffer {
        path: None,
        display_name: "legacy.rs".to_owned(),
        bytes: 4096,
        reason: format!("legacy reason \u{202e}{}", "x".repeat(300)),
    }];

    let status = restored_session_status(2, &skipped);

    assert!(status.starts_with("Restored 2 recovered buffers; skipped 1 buffer not snapshotted: "));
    assert!(!status.contains('\u{202e}'));
    assert!(status.chars().count() <= RESTORED_SESSION_STATUS_MAX_CHARS);

    let empty_status = restored_session_status(3, &[]);
    assert_eq!(empty_status, "Restored 3 recovered buffers");
}

#[test]
fn terminal_restore_visibility_requires_workspace_trust() {
    let root = temp_root("terminal-restore-untrusted");
    fs::create_dir_all(&root).unwrap();

    let mut app = app_for_test_with_trust(root.clone(), false);
    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        terminal_visible: true,
        terminal_sessions: vec![PersistedTerminalSession {
            cwd: Some(root.clone()),
            scrollback: "restored shell\n".to_owned(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: None,
            process_status: None,
            window_title: None,
        }],
        recent_projects: Vec::new(),
        recovery: Vec::new(),
        ..PersistedSession::default()
    });

    let session = app.build_session();

    assert!(!session.terminal_visible);
    assert_eq!(session.terminal_sessions.len(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_session_normalizes_explorer_revealed_path_and_expanded_ancestors() {
    let root = temp_root("explorer-revealed-normalized");
    fs::create_dir_all(&root).unwrap();
    let noisy_src = root.join("src").join("..").join("src");
    let noisy_revealed = noisy_src.join("nested").join("main.rs");

    let mut app = app_for_test(root.clone());
    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        explorer_expanded: vec![root.join("src").join(".."), noisy_src],
        explorer_revealed_path: Some(noisy_revealed),
        recent_projects: Vec::new(),
        recovery: Vec::new(),
        ..PersistedSession::default()
    });

    assert_eq!(
        app.explorer_revealed_path,
        Some(root.join("src").join("nested").join("main.rs"))
    );
    assert!(app.explorer_expanded.contains(&root.join("src")));
    assert!(
        app.explorer_expanded
            .contains(&root.join("src").join("nested"))
    );
    assert!(!app.explorer_expanded.contains(&root.join("src").join("..")));

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_session_drops_escaped_explorer_paths() {
    let root = temp_root("explorer-escaped-paths");
    fs::create_dir_all(&root).unwrap();
    let escaped = root.join("..").join("old").join("src");

    let mut app = app_for_test(root.clone());
    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        explorer_expanded: vec![root.join("src").join(".."), escaped.clone()],
        explorer_revealed_path: Some(escaped.join("main.rs")),
        recent_projects: Vec::new(),
        recovery: Vec::new(),
        ..PersistedSession::default()
    });

    assert!(app.explorer_expanded.is_empty());
    assert_eq!(app.explorer_revealed_path, None);

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_session_clears_stale_focus_when_active_file_is_pending() {
    let root = temp_root("pending-active-focus");
    let existing = root.join("src/lib.rs");
    fs::create_dir_all(existing.parent().unwrap()).unwrap();
    fs::write(&existing, "pub fn lib() {}\n").unwrap();

    let mut app = app_for_test(root.clone());
    app.focused_pane = Some(app.active_pane);
    app.last_autosave_focused_pane = Some(app.active_pane);
    let stale_pane = app.active_pane;

    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        open_files: vec![existing.clone()],
        active_path: Some(existing.clone()),
        pane_paths: vec![Some(existing.clone())],
        active_pane_index: Some(0),
        view_states: vec![view_state(existing.clone())],
        history_states: vec![history_state(existing.clone())],
        recent_projects: Vec::new(),
        ..PersistedSession::default()
    });

    assert_eq!(app.panes.len(), 1);
    assert_ne!(app.panes[0].id, stale_pane);
    assert_eq!(app.active_pane, app.panes[0].id);
    assert_eq!(app.panes[0].active, None);
    assert_eq!(
        app.pending_pane_paths.get(&app.active_pane),
        Some(&existing)
    );
    assert_eq!(app.pending_active_path, Some(existing));
    assert_eq!(app.focused_pane, None);
    assert_eq!(app.last_autosave_focused_pane, None);

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_session_clears_stale_buffer_runtime_state_before_recovery() {
    let root = temp_root("stale-buffer-runtime-restore");
    fs::create_dir_all(&root).unwrap();
    let stale_path = root.join("src/old.rs");
    let mut app = app_for_test(root.clone());
    let stale_id = 7;

    app.buffers.push(TextBuffer::from_text(
        stale_id,
        Some(stale_path.clone()),
        "stale".to_owned(),
    ));
    app.active = Some(stale_id);
    app.panes[0].active = Some(stale_id);
    app.virtual_buffer_labels
        .insert(stale_id, "stale label".to_owned());
    app.lossy_decoded_buffers.insert(stale_id);
    app.binary_preview_buffers.insert(stale_id);
    app.manual_read_only_buffers.insert(stale_id);
    assert!(app.mark_buffer_changed_on_disk(stale_id));
    app.pending_open_paths.insert(stale_path.clone());
    app.pending_active_path = Some(stale_path);
    app.pending_scroll_lines.insert(stale_id, 4);
    app.pending_horizontal_scroll_offsets.insert(stale_id, 32.0);
    app.editor_scroll_offsets
        .insert((app.active_pane, stale_id), 96.0);
    app.editor_horizontal_scroll_offsets
        .insert((app.active_pane, stale_id), 48.0);
    app.editor_scroll_targets
        .insert((app.active_pane, stale_id), 120.0);
    app.ime_preedit = Some(EditorImePreedit {
        buffer_id: stale_id,
        text: "stale ime".to_owned(),
    });
    app.pending_language_sync.insert(stale_id, Instant::now());

    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        recovery: vec![RecoveredBuffer {
            path: None,
            display_name: "scratch.rs".to_owned(),
            text: "restored".to_owned(),
        }],
        recent_projects: Vec::new(),
        ..PersistedSession::default()
    });

    let restored_id = app.buffers[0].id();
    assert_ne!(restored_id, stale_id);
    assert_eq!(app.buffers.len(), 1);
    assert_eq!(app.buffers[0].text(), "restored");
    assert_eq!(app.active, Some(restored_id));
    assert!(app.virtual_buffer_labels.is_empty());
    assert!(app.lossy_decoded_buffers.is_empty());
    assert!(app.binary_preview_buffers.is_empty());
    assert!(app.manual_read_only_buffers.is_empty());
    assert_eq!(app.changed_on_disk_buffer_count(), 0);
    assert!(app.pending_open_paths.is_empty());
    assert_eq!(app.pending_active_path, None);
    assert!(app.pending_scroll_lines.is_empty());
    assert!(app.pending_horizontal_scroll_offsets.is_empty());
    assert!(app.editor_scroll_offsets.is_empty());
    assert!(app.editor_horizontal_scroll_offsets.is_empty());
    assert!(app.editor_scroll_targets.is_empty());
    assert!(app.ime_preedit.is_none());
    assert!(app.pending_language_sync.is_empty());

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_session_normalizes_quick_open_recent_navigation_state() {
    let root = temp_root("quick-open-restore");
    fs::create_dir_all(&root).unwrap();
    let main = root.join("src/main.rs");

    let mut app = app_for_test(root.clone());
    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        quick_open_recent_files: vec![
            root.join("src/../src/main.rs"),
            main.clone(),
            root.join("../outside.rs"),
        ],
        quick_open_query_memory: vec![
            QuickOpenQueryMemoryEntry {
                query: " Main ".to_owned(),
                path: root.join("src/../src/main.rs"),
                uses: 4,
            },
            QuickOpenQueryMemoryEntry {
                query: "main".to_owned(),
                path: main.clone(),
                uses: 2,
            },
            QuickOpenQueryMemoryEntry {
                query: "main".to_owned(),
                path: root.join("../outside.rs"),
                uses: 8,
            },
        ],
        recent_projects: Vec::new(),
        ..PersistedSession::default()
    });

    assert_eq!(
        app.quick_open_recent_files
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec![main.clone()]
    );
    assert_eq!(app.quick_open_query_memory.len(), 1);
    assert_eq!(app.quick_open_query_memory[0].query, "main");
    assert_eq!(app.quick_open_query_memory[0].path, main);
    assert_eq!(app.quick_open_query_memory[0].uses, 4);

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_session_collapses_only_missing_path_panes_to_default_pane() {
    let root = temp_root("only-missing-panes");
    fs::create_dir_all(&root).unwrap();
    let missing_main = root.join("src/main.rs");
    let missing_lib = root.join("src/lib.rs");

    let mut app = app_for_test(root.clone());
    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        open_files: vec![missing_main.clone(), missing_lib.clone()],
        active_path: Some(missing_lib.clone()),
        pane_paths: vec![Some(missing_main.clone()), Some(missing_lib.clone())],
        pane_weights: vec![0.25, 0.75],
        active_pane_index: Some(1),
        view_states: vec![view_state(missing_main.clone()), view_state(missing_lib)],
        history_states: vec![history_state(missing_main)],
        recent_projects: Vec::new(),
        ..PersistedSession::default()
    });

    assert!(app.pending_open_paths.is_empty());
    assert!(app.pending_pane_paths.is_empty());
    assert_eq!(app.pending_active_path, None);
    assert_eq!(app.panes.len(), 1);
    assert_eq!(app.panes[0].active, None);
    assert_eq!(app.panes[0].weight, 1.0);
    assert_eq!(app.active_pane, app.panes[0].id);
    assert!(app.pending_pane_scroll_lines.is_empty());
    assert!(app.pending_pane_horizontal_scroll_offsets.is_empty());
    assert!(app.pending_view_states.is_empty());
    assert!(app.pending_history_states.is_empty());

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_session_restores_duplicate_file_pane_scrolls_independently() {
    let root = temp_root("duplicate-pane-scroll");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("src/main.rs");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "one\ntwo\nthree\nfour\nfive\n").unwrap();

    let mut app = app_for_test(root.clone());
    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        active_path: Some(path.clone()),
        pane_paths: vec![Some(path.clone()), Some(path.clone())],
        pane_weights: vec![0.4, 0.6],
        active_pane_index: Some(1),
        view_states: vec![BufferViewState {
            path: path.clone(),
            cursor_line: 3,
            cursor_column: 2,
            scroll_line: 5,
            horizontal_scroll_offset: 90.0,
            selections: Vec::new(),
        }],
        pane_view_states: vec![
            PaneBufferViewState {
                pane_index: 0,
                path: path.clone(),
                scroll_line: 2,
                horizontal_scroll_offset: 12.0,
            },
            PaneBufferViewState {
                pane_index: 1,
                path: path.clone(),
                scroll_line: 4,
                horizontal_scroll_offset: 24.0,
            },
        ],
        recovery: vec![RecoveredBuffer {
            path: Some(path.clone()),
            display_name: "main.rs".to_owned(),
            text: "one\ntwo\nthree\nfour\nfive\n".to_owned(),
        }],
        recent_projects: Vec::new(),
        ..PersistedSession::default()
    });

    assert_eq!(app.panes.len(), 2);
    let first_pane = app.panes[0].id;
    let second_pane = app.panes[1].id;
    let restored_id = app.buffers[0].id();
    assert_eq!(app.panes[0].active, Some(restored_id));
    assert_eq!(app.panes[1].active, Some(restored_id));
    assert_eq!(app.active_pane, second_pane);
    assert_eq!(
        app.pending_pane_scroll_lines
            .get(&(first_pane, restored_id)),
        Some(&1)
    );
    assert_eq!(
        app.pending_pane_scroll_lines
            .get(&(second_pane, restored_id)),
        Some(&3)
    );
    assert_eq!(
        app.pending_pane_horizontal_scroll_offsets
            .get(&(first_pane, restored_id)),
        Some(&12.0)
    );
    assert_eq!(
        app.pending_pane_horizontal_scroll_offsets
            .get(&(second_pane, restored_id)),
        Some(&24.0)
    );
    assert!(!app.pending_scroll_lines.contains_key(&restored_id));
    assert!(
        !app.pending_horizontal_scroll_offsets
            .contains_key(&restored_id)
    );

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_session_legacy_view_state_still_restores_path_scroll() {
    let root = temp_root("legacy-path-scroll");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("src/main.rs");

    let mut app = app_for_test(root.clone());
    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        active_path: Some(path.clone()),
        pane_paths: vec![Some(path.clone())],
        view_states: vec![BufferViewState {
            path: path.clone(),
            cursor_line: 2,
            cursor_column: 1,
            scroll_line: 4,
            horizontal_scroll_offset: 32.0,
            selections: Vec::new(),
        }],
        recovery: vec![RecoveredBuffer {
            path: Some(path.clone()),
            display_name: "main.rs".to_owned(),
            text: "one\ntwo\nthree\nfour\nfive\n".to_owned(),
        }],
        recent_projects: Vec::new(),
        ..PersistedSession::default()
    });

    let restored_id = app.buffers[0].id();
    assert_eq!(app.pending_scroll_lines.get(&restored_id), Some(&3));
    assert_eq!(
        app.pending_horizontal_scroll_offsets.get(&restored_id),
        Some(&32.0)
    );
    assert!(app.pending_pane_scroll_lines.is_empty());
    assert!(app.pending_pane_horizontal_scroll_offsets.is_empty());

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn restore_session_applies_case_equivalent_recovered_path_state() {
    let root = temp_root("case-equivalent-recovered-state");
    fs::create_dir_all(&root).unwrap();
    let recovered_path = root.join("SRC").join("Main.rs");
    let state_path = root.join("src").join("main.rs");

    let mut app = app_for_test(root.clone());
    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        active_path: Some(state_path.clone()),
        view_states: vec![BufferViewState {
            path: state_path.clone(),
            cursor_line: 2,
            cursor_column: 2,
            scroll_line: 3,
            horizontal_scroll_offset: 18.0,
            selections: Vec::new(),
        }],
        history_states: vec![history_state(state_path)],
        recovery: vec![RecoveredBuffer {
            path: Some(recovered_path.clone()),
            display_name: "Main.rs".to_owned(),
            text: "one\ntwo\nthree\n".to_owned(),
        }],
        recent_projects: Vec::new(),
        ..PersistedSession::default()
    });

    let restored_id = app.buffers[0].id();
    let restored = app.buffer(restored_id).expect("recovered buffer");
    assert_eq!(restored.path(), Some(&recovered_path));
    assert_eq!(restored.cursor_position().line, 1);
    assert_eq!(restored.cursor_position().column, 1);
    assert_eq!(app.active, Some(restored_id));
    assert_eq!(app.pending_scroll_lines.get(&restored_id), Some(&2));
    assert_eq!(
        app.pending_horizontal_scroll_offsets.get(&restored_id),
        Some(&18.0)
    );
    assert!(app.pending_view_states.is_empty());
    assert!(app.pending_history_states.is_empty());

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_session_legacy_view_state_applies_to_duplicate_file_panes() {
    let root = temp_root("legacy-duplicate-pane-scroll");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("src/main.rs");

    let mut app = app_for_test(root.clone());
    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        active_path: Some(path.clone()),
        pane_paths: vec![Some(path.clone()), Some(path.clone())],
        view_states: vec![BufferViewState {
            path: path.clone(),
            cursor_line: 2,
            cursor_column: 1,
            scroll_line: 4,
            horizontal_scroll_offset: 32.0,
            selections: Vec::new(),
        }],
        recovery: vec![RecoveredBuffer {
            path: Some(path.clone()),
            display_name: "main.rs".to_owned(),
            text: "one\ntwo\nthree\nfour\nfive\n".to_owned(),
        }],
        recent_projects: Vec::new(),
        ..PersistedSession::default()
    });

    let restored_id = app.buffers[0].id();
    let first_pane = app.panes[0].id;
    let second_pane = app.panes[1].id;
    assert_eq!(
        app.pending_pane_scroll_lines
            .get(&(first_pane, restored_id)),
        Some(&3)
    );
    assert_eq!(
        app.pending_pane_scroll_lines
            .get(&(second_pane, restored_id)),
        Some(&3)
    );
    assert_eq!(
        app.pending_pane_horizontal_scroll_offsets
            .get(&(first_pane, restored_id)),
        Some(&32.0)
    );
    assert_eq!(
        app.pending_pane_horizontal_scroll_offsets
            .get(&(second_pane, restored_id)),
        Some(&32.0)
    );
    assert!(!app.pending_scroll_lines.contains_key(&restored_id));
    assert!(
        !app.pending_horizontal_scroll_offsets
            .contains_key(&restored_id)
    );

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn pathless_recovered_buffer_restores_view_and_history() {
    let root = temp_root("pathless-recovery-state");
    fs::create_dir_all(&root).unwrap();
    let recovered_text = "alpha\nbravo\ncharlie".to_owned();

    let mut app = app_for_test(root.clone());
    let id = app.next_id();
    let mut buffer = TextBuffer::from_text(id, None, recovered_text.clone());
    buffer.set_single_cursor(buffer.len_chars());
    buffer.insert_at_cursor("\ndelta");
    assert!(buffer.undo());
    buffer.set_single_cursor(buffer.line_column_to_char(2, 3));
    buffer.mark_dirty();
    app.buffers.push(buffer);
    app.set_active_buffer(id);
    let row_height =
        crate::session_state::editor_row_height(app.settings.font_size, app.settings.line_height);
    app.editor_scroll_offsets
        .insert((app.active_pane, id), row_height * 2.0);
    app.editor_horizontal_scroll_offsets
        .insert((app.active_pane, id), 64.0);

    let session = app.build_session();

    assert_eq!(session.recovery.len(), 1);
    assert_eq!(session.recovery[0].path, None);
    assert!(session.view_states.is_empty());
    assert!(session.history_states.is_empty());
    assert_eq!(session.recovery_view_states.len(), 1);
    assert_eq!(session.recovery_view_states[0].recovery_index, 0);
    assert_eq!(session.recovery_view_states[0].cursor_line, 3);
    assert_eq!(session.recovery_view_states[0].cursor_column, 4);
    assert_eq!(session.recovery_view_states[0].scroll_line, 3);
    assert_eq!(
        session.recovery_view_states[0].horizontal_scroll_offset,
        64.0
    );
    assert_eq!(session.recovery_history_states.len(), 1);
    assert_eq!(session.recovery_history_states[0].recovery_index, 0);

    let mut restored_app = app_for_test(root.clone());
    restored_app.restore_session(session);
    let restored_id = restored_app.buffers[0].id();
    {
        let restored = &restored_app.buffers[0];
        assert_eq!(restored.path(), None);
        assert_eq!(restored.text(), recovered_text);
        assert_eq!(restored.cursor_position().line, 2);
        assert_eq!(restored.cursor_position().column, 3);
    }
    assert_eq!(
        restored_app.pending_scroll_lines.get(&restored_id),
        Some(&2)
    );
    assert_eq!(
        restored_app
            .pending_horizontal_scroll_offsets
            .get(&restored_id),
        Some(&64.0)
    );

    let restored = restored_app.buffer_mut(restored_id).unwrap();
    assert!(restored.redo());
    assert_eq!(restored.text(), "alpha\nbravo\ncharlie\ndelta");
    assert!(restored.undo());
    assert_eq!(restored.text(), recovered_text);

    drop(restored_app);
    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn pathless_recovery_state_indices_follow_deduped_recovery_entries() {
    let root = temp_root("pathless-recovery-dedup-indices");
    let path = root.join("src/main.rs");
    fs::create_dir_all(path.parent().unwrap()).unwrap();

    let mut app = app_for_test(root.clone());
    let older_id = app.next_id();
    let mut older = TextBuffer::from_text(older_id, Some(path.clone()), "older".to_owned());
    older.mark_dirty();
    app.buffers.push(older);

    let pathless_id = app.next_id();
    let recovered_text = "alpha\nbravo\ncharlie".to_owned();
    let mut pathless = TextBuffer::from_text(pathless_id, None, recovered_text.clone());
    pathless.set_single_cursor(pathless.len_chars());
    pathless.insert_at_cursor("\ndelta");
    assert!(pathless.undo());
    pathless.set_single_cursor(pathless.line_column_to_char(2, 3));
    pathless.mark_dirty();
    app.buffers.push(pathless);

    let newer_id = app.next_id();
    let mut newer = TextBuffer::from_text(newer_id, Some(path.clone()), "newer".to_owned());
    newer.mark_dirty();
    app.buffers.push(newer);
    app.set_active_buffer(pathless_id);
    let row_height =
        crate::session_state::editor_row_height(app.settings.font_size, app.settings.line_height);
    app.editor_scroll_offsets
        .insert((app.active_pane, pathless_id), row_height * 2.0);
    app.editor_horizontal_scroll_offsets
        .insert((app.active_pane, pathless_id), 64.0);

    let session = app.build_session();

    assert_eq!(session.recovery.len(), 2);
    assert_eq!(session.recovery[0].path, None);
    assert_eq!(session.recovery[1].path.as_deref(), Some(path.as_path()));
    assert_eq!(session.recovery_view_states.len(), 1);
    assert_eq!(session.recovery_view_states[0].recovery_index, 0);
    assert_eq!(session.recovery_view_states[0].cursor_line, 3);
    assert_eq!(session.recovery_view_states[0].cursor_column, 4);
    assert_eq!(session.recovery_history_states.len(), 1);
    assert_eq!(session.recovery_history_states[0].recovery_index, 0);

    let mut restored_app = app_for_test(root.clone());
    restored_app.restore_session(session);
    let restored_pathless_id = restored_app
        .buffers
        .iter()
        .find(|buffer| buffer.path().is_none())
        .map(TextBuffer::id)
        .expect("pathless recovered buffer should restore");
    {
        let restored = restored_app.buffer(restored_pathless_id).unwrap();
        assert_eq!(restored.text(), recovered_text);
        assert_eq!(restored.cursor_position().line, 2);
        assert_eq!(restored.cursor_position().column, 3);
    }
    assert_eq!(
        restored_app.pending_scroll_lines.get(&restored_pathless_id),
        Some(&2)
    );
    assert_eq!(
        restored_app
            .pending_horizontal_scroll_offsets
            .get(&restored_pathless_id),
        Some(&64.0)
    );

    let restored = restored_app.buffer_mut(restored_pathless_id).unwrap();
    assert!(restored.redo());
    assert_eq!(restored.text(), "alpha\nbravo\ncharlie\ndelta");
    assert!(restored.undo());
    assert_eq!(restored.text(), recovered_text);

    drop(restored_app);
    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_session_restores_project_search_toggles() {
    let root = temp_root("project-search-toggles");
    fs::create_dir_all(&root).unwrap();

    let mut app = app_for_test(root.clone());

    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        project_search_open: true,
        project_search_query: "needle".to_owned(),
        project_search_case_sensitive: true,
        project_search_whole_word: true,
        project_search_regex: true,
        project_search_include: "src/**/*.rs".to_owned(),
        project_search_exclude: "target/**".to_owned(),
        recent_projects: Vec::new(),
        recovery: Vec::new(),
        ..PersistedSession::default()
    });

    assert!(app.project_search);
    assert_eq!(app.project_search_query, "needle");
    assert!(app.project_search_case_sensitive);
    assert!(app.project_search_whole_word);
    assert!(app.project_search_regex);
    assert_eq!(app.project_search_include, "src/**/*.rs");
    assert_eq!(app.project_search_exclude, "target/**");

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn build_session_persists_project_search_toggles() {
    let root = temp_root("project-search-toggles-save");
    fs::create_dir_all(&root).unwrap();

    let mut app = app_for_test(root.clone());
    app.project_search_regex = true;

    let session = app.build_session();

    assert!(session.project_search_regex);

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_session_orders_recovered_buffers_by_saved_open_file_order() {
    let root = temp_root("recovered-buffer-saved-order");
    fs::create_dir_all(&root).unwrap();
    let first = root.join("src/a.rs");
    let last = root.join("src/c.rs");
    // Saved tab order was [a.rs, b.rs, c.rs]; a.rs and c.rs were dirty (so
    // they are recovered buffers, listed out of saved order here) while
    // b.rs was clean and is only referenced by open_files.
    let mut app = app_for_test(root.clone());

    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        open_files: vec![first.clone(), root.join("src/b.rs"), last.clone()],
        recovery: vec![
            RecoveredBuffer {
                path: Some(last.clone()),
                display_name: "c.rs".to_owned(),
                text: "recovered last\n".to_owned(),
            },
            RecoveredBuffer {
                path: Some(first.clone()),
                display_name: "a.rs".to_owned(),
                text: "recovered first\n".to_owned(),
            },
        ],
        recent_projects: Vec::new(),
        ..PersistedSession::default()
    });

    let restored_paths = app
        .buffers
        .iter()
        .map(|buffer| buffer.path().expect("recovered buffer path").to_path_buf())
        .collect::<Vec<_>>();
    assert_eq!(restored_paths, vec![first, last]);

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_session_restores_buffer_find_ui_state_and_resets_transients() {
    let root = temp_root("buffer-find-state");
    fs::create_dir_all(&root).unwrap();

    let mut app = app_for_test(root.clone());
    app.buffer_find_match = 12;
    app.buffer_find_scope = Some(BufferFindScope {
        buffer_id: 42,
        range: 2..8,
    });
    app.buffer_find_query_history_cursor = Some(0);
    app.buffer_find_query_history_draft = Some("draft query".to_owned());
    app.buffer_find_replacement_history_cursor = Some(0);
    app.buffer_find_replacement_history_draft = Some("draft replacement".to_owned());

    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        buffer_find_open: true,
        buffer_find_query: "Needle".to_owned(),
        buffer_find_replacement: "Replacement".to_owned(),
        buffer_find_case_sensitive: true,
        buffer_find_whole_word: true,
        buffer_find_regex: true,
        buffer_find_preserve_case: true,
        buffer_find_query_history: vec!["Needle".to_owned(), "Previous".to_owned()],
        buffer_find_replacement_history: vec!["Replacement".to_owned(), "Earlier".to_owned()],
        recent_projects: Vec::new(),
        recovery: Vec::new(),
        ..PersistedSession::default()
    });

    assert!(app.buffer_find_open);
    assert_eq!(app.buffer_find_query, "Needle");
    assert_eq!(app.buffer_find_replacement, "Replacement");
    assert!(app.buffer_find_case_sensitive);
    assert!(app.buffer_find_whole_word);
    assert!(app.buffer_find_regex);
    assert!(app.buffer_find_preserve_case);
    assert_eq!(app.buffer_find_match, 0);
    assert_eq!(app.buffer_find_scope, None);
    assert_eq!(
        app.buffer_find_query_history
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["Needle".to_owned(), "Previous".to_owned()]
    );
    assert_eq!(
        app.buffer_find_replacement_history
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["Replacement".to_owned(), "Earlier".to_owned()]
    );
    assert_eq!(app.buffer_find_query_history_cursor, None);
    assert_eq!(app.buffer_find_query_history_draft, None);
    assert_eq!(app.buffer_find_replacement_history_cursor, None);
    assert_eq!(app.buffer_find_replacement_history_draft, None);

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn build_session_persists_buffer_find_ui_state() {
    let root = temp_root("buffer-find-save");
    fs::create_dir_all(&root).unwrap();

    let mut app = app_for_test(root.clone());
    app.buffer_find_open = true;
    app.buffer_find_query = "Needle".to_owned();
    app.buffer_find_replacement = "Replacement".to_owned();
    app.buffer_find_case_sensitive = true;
    app.buffer_find_whole_word = true;
    app.buffer_find_regex = true;
    app.buffer_find_preserve_case = true;

    let session = app.build_session();

    assert!(session.buffer_find_open);
    assert_eq!(session.buffer_find_query, "Needle");
    assert_eq!(session.buffer_find_replacement, "Replacement");
    assert!(session.buffer_find_case_sensitive);
    assert!(session.buffer_find_whole_word);
    assert!(session.buffer_find_regex);
    assert!(session.buffer_find_preserve_case);

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_session_restores_persistent_ui_overlay_state_and_resets_transients() {
    let root = temp_root("persistent-ui-overlays");
    fs::create_dir_all(&root).unwrap();

    let mut app = app_for_test(root.clone());
    app.settings_panel_draft.font_size = 99.0;
    app.settings_editor_font_path = "stale-editor-font".to_owned();
    app.settings_ui_font_path = "stale-ui-font".to_owned();
    app.theme_picker_selected = usize::MAX;
    app.keybindings_query = "stale query".to_owned();
    app.keybindings_selected = 9;
    app.keybinding_capture_command = Some(Command::ToggleTerminal);

    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        settings_panel_open: true,
        theme_picker_open: true,
        keybindings_open: true,
        recent_projects: Vec::new(),
        recovery: Vec::new(),
        ..PersistedSession::default()
    });

    assert!(app.settings_panel_open);
    assert_eq!(app.settings_panel_draft.font_size, app.settings.font_size);
    assert!(app.settings_editor_font_path.is_empty());
    assert!(app.settings_ui_font_path.is_empty());
    assert!(app.theme_picker_open);
    assert_eq!(
        app.theme_picker_selected,
        selected_theme_index_with_plugins(&app.settings.theme, &app.plugin_themes)
    );
    assert!(app.keybindings_open);
    assert!(app.keybindings_query.is_empty());
    assert_eq!(app.keybindings_selected, 0);
    assert!(app.keybinding_capture_command.is_none());

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_session_cancels_pending_escape_keybinding_capture_before_resetting_overlays() {
    let root = temp_root("restore-session-cancels-keybinding-capture");
    fs::create_dir_all(&root).unwrap();
    let mut app = app_for_test(root.clone());
    app.settings.keymap.bindings = vec![
        KeyBinding {
            chord: "Escape".to_owned(),
            command: Command::ToggleQuickOpen,
        },
        KeyBinding {
            chord: "Ctrl+Z".to_owned(),
            command: Command::Undo,
        },
    ];
    app.keybindings_open = true;
    app.keybinding_capture_command = Some(Command::Undo);

    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Escape),
        ..PendingKeybindingsPanelActions::default()
    });
    assert_eq!(app.keybinding_capture_command, Some(Command::Undo));
    assert!(app.keybinding_escape_cancel.is_some());
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::Undo),
        Some("Ctrl+Z".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::ToggleQuickOpen),
        Some("Escape".to_owned())
    );

    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        keybindings_open: false,
        recent_projects: Vec::new(),
        recovery: Vec::new(),
        ..PersistedSession::default()
    });

    assert!(!app.keybindings_open);
    assert_eq!(app.keybinding_capture_command, None);
    assert!(app.keybinding_escape_cancel.is_none());
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::Undo),
        Some("Ctrl+Z".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::ToggleQuickOpen),
        Some("Escape".to_owned())
    );
    assert!(!settings_path(&root).exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn build_session_persists_persistent_ui_overlay_state() {
    let root = temp_root("persistent-ui-overlay-save");
    fs::create_dir_all(&root).unwrap();

    let mut app = app_for_test(root.clone());
    app.settings_panel_open = true;
    app.theme_picker_open = true;
    app.keybindings_open = true;

    let session = app.build_session();

    assert!(session.settings_panel_open);
    assert!(session.theme_picker_open);
    assert!(session.keybindings_open);

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn build_session_persists_normalized_command_palette_query_memory() {
    let root = temp_root("command-query-memory-save");
    fs::create_dir_all(&root).unwrap();

    let mut app = app_for_test(root.clone());
    app.command_query_memory = vec![
        crate::command_palette_items::CommandPaletteQueryMemoryEntry {
            query: " Git ".to_owned(),
            command: Command::ToggleSourceControl,
            uses: 0,
        },
        crate::command_palette_items::CommandPaletteQueryMemoryEntry {
            query: "git".to_owned(),
            command: Command::ToggleSourceControl,
            uses: 9,
        },
        crate::command_palette_items::CommandPaletteQueryMemoryEntry {
            query: "\u{202e}\t".to_owned(),
            command: Command::ToggleGitHistory,
            uses: 4,
        },
        crate::command_palette_items::CommandPaletteQueryMemoryEntry {
            query: "Git".to_owned(),
            command: Command::ToggleGitHistory,
            uses: 2,
        },
    ]
    .into_iter()
    .collect();

    let session = app.build_session();

    assert_eq!(
        session.command_query_memory,
        vec![
            crate::command_palette_items::CommandPaletteQueryMemoryEntry {
                query: "git".to_owned(),
                command: Command::ToggleSourceControl,
                uses: 9,
            },
            crate::command_palette_items::CommandPaletteQueryMemoryEntry {
                query: "git".to_owned(),
                command: Command::ToggleGitHistory,
                uses: 2,
            },
        ]
    );

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_session_normalizes_workspace_symbol_query_memory() {
    let root = temp_root("workspace-symbol-memory-restore");
    fs::create_dir_all(&root).unwrap();

    let mut app = app_for_test(root.clone());
    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        workspace_symbol_query_memory: vec![
            crate::lsp_workspace_symbol_ranking::WorkspaceSymbolQueryMemoryEntry {
                query: " Main ".to_owned(),
                path: root.join("src/main.rs"),
                name: "main_symbol".to_owned(),
                kind: 12,
                line: 0,
                column: 0,
                uses: 0,
            },
            crate::lsp_workspace_symbol_ranking::WorkspaceSymbolQueryMemoryEntry {
                query: "main".to_owned(),
                path: root.join("src/main.rs"),
                name: "main_symbol".to_owned(),
                kind: 12,
                line: 4,
                column: 2,
                uses: 7,
            },
            crate::lsp_workspace_symbol_ranking::WorkspaceSymbolQueryMemoryEntry {
                query: "\u{202e}\t".to_owned(),
                path: root.join("src/lib.rs"),
                name: "lib_symbol".to_owned(),
                kind: 12,
                line: 1,
                column: 1,
                uses: 3,
            },
        ],
        recent_projects: Vec::new(),
        recovery: Vec::new(),
        ..PersistedSession::default()
    });

    assert_eq!(
        app.workspace_symbol_query_memory,
        std::collections::VecDeque::from([
            crate::lsp_workspace_symbol_ranking::WorkspaceSymbolQueryMemoryEntry {
                query: "main".to_owned(),
                path: root.join("src/main.rs"),
                name: "main_symbol".to_owned(),
                kind: 12,
                line: 1,
                column: 1,
                uses: 7,
            },
        ])
    );

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn build_session_persists_normalized_workspace_symbol_query_memory() {
    let root = temp_root("workspace-symbol-memory-save");
    fs::create_dir_all(&root).unwrap();

    let mut app = app_for_test(root.clone());
    app.workspace_symbol_query_memory = vec![
        crate::lsp_workspace_symbol_ranking::WorkspaceSymbolQueryMemoryEntry {
            query: " Main ".to_owned(),
            path: root.join("src/main.rs"),
            name: "main_symbol".to_owned(),
            kind: 12,
            line: 0,
            column: 0,
            uses: 0,
        },
        crate::lsp_workspace_symbol_ranking::WorkspaceSymbolQueryMemoryEntry {
            query: "main".to_owned(),
            path: root.join("src/main.rs"),
            name: "main_symbol".to_owned(),
            kind: 12,
            line: 4,
            column: 2,
            uses: 7,
        },
        crate::lsp_workspace_symbol_ranking::WorkspaceSymbolQueryMemoryEntry {
            query: "other".to_owned(),
            path: root.join("..").join("outside/main.rs"),
            name: "other_symbol".to_owned(),
            kind: 12,
            line: 1,
            column: 1,
            uses: 3,
        },
    ]
    .into_iter()
    .collect();

    let session = app.build_session();

    assert_eq!(
        session.workspace_symbol_query_memory,
        vec![
            crate::lsp_workspace_symbol_ranking::WorkspaceSymbolQueryMemoryEntry {
                query: "main".to_owned(),
                path: root.join("src/main.rs"),
                name: "main_symbol".to_owned(),
                kind: 12,
                line: 1,
                column: 1,
                uses: 7,
            },
        ]
    );

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_session_restores_source_control_commit_history_and_resets_index() {
    let root = temp_root("source-control-commit-history");
    fs::create_dir_all(&root).unwrap();

    let mut app = app_for_test(root.clone());
    app.source_control_commit_history_index = Some(1);

    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        source_control_commit_message: "draft".to_owned(),
        source_control_commit_history: vec![
            " older ".to_owned(),
            "repeat".to_owned(),
            "newer".to_owned(),
            "repeat".to_owned(),
            "latest".to_owned(),
        ],
        recent_projects: Vec::new(),
        recovery: Vec::new(),
        ..PersistedSession::default()
    });

    assert_eq!(app.source_control_commit_message, "draft");
    assert_eq!(
        app.source_control_commit_history,
        vec![
            "older".to_owned(),
            "newer".to_owned(),
            "repeat".to_owned(),
            "latest".to_owned()
        ]
    );
    assert_eq!(app.source_control_commit_history_index, None);

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn build_session_persists_normalized_source_control_commit_history() {
    let root = temp_root("source-control-commit-history-save");
    fs::create_dir_all(&root).unwrap();

    let mut app = app_for_test(root.clone());
    app.source_control_commit_history = vec![
        " older ".to_owned(),
        "repeat".to_owned(),
        "newer".to_owned(),
        "repeat".to_owned(),
        "latest".to_owned(),
    ];

    let session = app.build_session();

    assert_eq!(
        session.source_control_commit_history,
        vec![
            "older".to_owned(),
            "newer".to_owned(),
            "repeat".to_owned(),
            "latest".to_owned()
        ]
    );

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_session_restores_source_control_stash_draft() {
    let root = temp_root("source-control-stash-draft");
    fs::create_dir_all(&root).unwrap();

    let mut app = app_for_test(root.clone());

    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        source_control_stash_message: "stash these edits".to_owned(),
        source_control_stash_query: "filter stashes".to_owned(),
        recent_projects: Vec::new(),
        recovery: Vec::new(),
        ..PersistedSession::default()
    });

    assert_eq!(app.source_control_stash_message, "stash these edits");
    assert_eq!(app.source_control_stash_query, "filter stashes");

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_session_restores_source_control_stash_panel_state_and_resets_transients() {
    let root = temp_root("source-control-stash-panel");
    fs::create_dir_all(&root).unwrap();

    let mut app = app_for_test(root.clone());
    app.source_control_stashes = vec![git_stash_for_test(1, "stale stash")];
    app.source_control_stash_selected = 4;
    app.source_control_stashes_next_request_id = 12;
    app.source_control_stashes_active_request_id = 11;
    app.source_control_stashes_in_flight_request_id = Some(11);
    app.source_control_stashes_reload_queued = true;

    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        source_control_stash_message: "stash these edits".to_owned(),
        source_control_stashes_open: true,
        recent_projects: Vec::new(),
        recovery: Vec::new(),
        ..PersistedSession::default()
    });

    assert!(app.source_control_stashes_open);
    assert_eq!(app.source_control_stash_message, "stash these edits");
    assert!(app.source_control_stashes.is_empty());
    assert_eq!(app.source_control_stash_selected, 0);
    assert_eq!(app.source_control_stashes_next_request_id, 0);
    assert_eq!(app.source_control_stashes_active_request_id, 0);
    assert_eq!(app.source_control_stashes_in_flight_request_id, None);
    assert!(!app.source_control_stashes_reload_queued);
    assert!(app.pending_restored_git_stashes_load);

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn build_session_persists_source_control_stash_draft() {
    let root = temp_root("source-control-stash-draft-save");
    fs::create_dir_all(&root).unwrap();

    let mut app = app_for_test(root.clone());
    app.source_control_stash_message = "stash these edits".to_owned();
    app.source_control_stash_query = "filter stashes".to_owned();

    let session = app.build_session();

    assert_eq!(session.source_control_stash_message, "stash these edits");
    assert_eq!(session.source_control_stash_query, "filter stashes");

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn build_session_persists_source_control_stash_panel_state() {
    let root = temp_root("source-control-stash-panel-save");
    fs::create_dir_all(&root).unwrap();

    let mut app = app_for_test(root.clone());
    app.source_control_stashes_open = true;

    let session = app.build_session();

    assert!(session.source_control_stashes_open);

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restored_git_stashes_load_preserves_restored_stash_draft() {
    let root = temp_root("source-control-stash-restored-load");
    fs::create_dir_all(&root).unwrap();

    let mut app = app_for_test(root.clone());
    app.settings.git_enabled = true;
    app.source_control_stashes_open = true;
    app.source_control_stash_message = "restored stash draft".to_owned();
    app.source_control_commit_message = "commit fallback".to_owned();

    assert!(app.spawn_restored_git_stashes_load());

    assert_eq!(app.source_control_stash_message, "restored stash draft");
    assert_eq!(app.source_control_stashes_next_request_id, 1);
    assert_eq!(app.source_control_stashes_active_request_id, 1);
    assert_eq!(app.source_control_stashes_in_flight_request_id, Some(1));
    assert!(!app.source_control_stashes_reload_queued);

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_session_restores_source_control_history_panel_state_and_resets_transients() {
    let root = temp_root("source-control-history-panel");
    fs::create_dir_all(&root).unwrap();

    let mut app = app_for_test(root.clone());
    app.source_control_history = vec![git_commit_for_test("stale")];
    app.source_control_history_selected = 3;
    app.source_control_history_loading = true;
    app.source_control_history_requested_limit = 50;
    app.source_control_history_has_more = true;
    app.source_control_history_in_flight_request_id = Some(7);
    app.source_control_history_reload_queued = true;

    app.restore_session(PersistedSession {
        workspace_root: root.clone(),
        source_control_history_open: true,
        source_control_history_query: "fix author".to_owned(),
        recent_projects: Vec::new(),
        recovery: Vec::new(),
        ..PersistedSession::default()
    });

    assert!(app.source_control_history_open);
    assert_eq!(app.source_control_history_query, "fix author");
    assert!(app.source_control_history.is_empty());
    assert_eq!(app.source_control_history_selected, 0);
    assert!(!app.source_control_history_loading);
    assert_eq!(app.source_control_history_requested_limit, 0);
    assert!(!app.source_control_history_has_more);
    assert_eq!(app.source_control_history_in_flight_request_id, None);
    assert!(!app.source_control_history_reload_queued);
    assert!(app.pending_restored_git_history_load);

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn build_session_persists_source_control_history_panel_state() {
    let root = temp_root("source-control-history-panel-save");
    fs::create_dir_all(&root).unwrap();

    let mut app = app_for_test(root.clone());
    app.source_control_history_open = true;
    app.source_control_history_query = "fix author".to_owned();

    let session = app.build_session();

    assert!(session.source_control_history_open);
    assert_eq!(session.source_control_history_query, "fix author");

    drop(app);
    fs::remove_dir_all(root).unwrap();
}

fn app_for_test(root: PathBuf) -> KuroyaApp {
    app_for_test_with_trust(root, true)
}

fn app_for_test_with_trust(root: PathBuf, trusted: bool) -> KuroyaApp {
    let (tx, rx) = crate::ui_event_channel::ui_event_channel();
    let settings = EditorSettings::default();
    let trusted_workspaces = trusted.then(|| root.clone()).into_iter().collect();
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
        trusted_workspaces,
        now: Instant::now(),
        startup_timings: Vec::new(),
    })
}

fn view_state(path: PathBuf) -> BufferViewState {
    BufferViewState {
        path,
        cursor_line: 1,
        cursor_column: 1,
        scroll_line: 1,
        horizontal_scroll_offset: 0.0,
        selections: Vec::new(),
    }
}

fn history_state(path: PathBuf) -> BufferHistoryState {
    BufferHistoryState {
        path,
        history: BufferHistorySnapshot {
            len_chars: 0,
            checksum: 0,
            undo: Vec::new(),
            redo: Vec::new(),
        },
    }
}

fn git_commit_for_test(summary: &str) -> GitCommitSummary {
    GitCommitSummary {
        oid: "0123456789abcdef0123456789abcdef01234567".to_owned(),
        short_oid: "0123456".to_owned(),
        summary: summary.to_owned(),
        author: "Test Author".to_owned(),
        time_seconds: 1,
    }
}

fn git_stash_for_test(index: usize, message: &str) -> GitStashEntry {
    GitStashEntry {
        index,
        short_oid: "0123456".to_owned(),
        message: message.to_owned(),
    }
}

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    env::temp_dir().join(format!(
        "kuroya-restore-{name}-{}-{nanos}",
        std::process::id()
    ))
}
