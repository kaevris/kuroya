use super::{
    MAX_SESSION_SNAPSHOTS, PERSISTED_SESSION_MAX_BYTES_USIZE,
    load_latest_session_snapshot_after_quarantine, normalize_persisted_session_paths_for_restore,
    prune_session_snapshots, prune_session_snapshots_async, session_bytes_for_write,
    sort_session_snapshot_paths, unique_session_snapshot_path, write_session_snapshot,
};
use crate::{
    layout::{
        DIAGNOSTICS_PANEL_MAX_WIDTH, EXPLORER_DEFAULT_WIDTH, SOURCE_CONTROL_MIN_WIDTH,
        SYMBOLS_PANEL_DEFAULT_WIDTH, TERMINAL_DEFAULT_HEIGHT,
    },
    persistence::{
        PaneBufferViewState, PersistedSession, PersistedTerminalSession, RecoveredBuffer,
        RecoveredBufferHistoryState, RecoveredBufferViewState, SkippedRecoveredBuffer,
    },
    persistence_models::{
        PERSISTED_SESSION_DISPLAY_TEXT_MAX_CHARS, PERSISTED_SESSION_RECOVERY_SKIPPED_MAX,
        PERSISTED_SESSION_RECOVERY_TEXT_MAX_CHARS, PERSISTED_SESSION_TERMINAL_SCROLLBACK_MAX_CHARS,
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
    },
    persistence_storage::session_snapshots_dir,
};
use kuroya_core::BufferHistorySnapshot;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn temp_workspace(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "kuroya-persistence-session-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn unique_session_snapshot_path_zero_pads_counter_for_lexical_sorting() {
    let path = unique_session_snapshot_path(Path::new("snapshots"));
    let file_name = path.file_name().unwrap().to_str().unwrap();
    let parts = file_name
        .strip_prefix("session.")
        .unwrap()
        .strip_suffix(".json")
        .unwrap()
        .split('.')
        .collect::<Vec<_>>();

    assert_eq!(parts.len(), 3);
    assert_eq!(parts[2].len(), 16);
}

#[test]
fn session_snapshot_paths_sort_counters_numerically() {
    let mut snapshots = vec![
        PathBuf::from("snapshots/session.10.7.10.json"),
        PathBuf::from("snapshots/session.zzz.json"),
        PathBuf::from("snapshots/session.10.7.2.json"),
        PathBuf::from("snapshots/session.11.1.0.json"),
    ];

    sort_session_snapshot_paths(&mut snapshots);

    let names = snapshots
        .iter()
        .map(|path| path.file_name().unwrap().to_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "session.zzz.json",
            "session.10.7.2.json",
            "session.10.7.10.json",
            "session.11.1.0.json",
        ]
    );
}

#[test]
fn failed_current_session_quarantine_does_not_block_snapshot_restore() {
    let workspace = temp_workspace("best-effort-quarantine");
    fs::create_dir_all(&workspace).unwrap();
    let mut snapshot = PersistedSession {
        workspace_root: workspace.clone(),
        recovery: vec![RecoveredBuffer {
            path: None,
            display_name: "restored.rs".to_owned(),
            text: "snapshot recovery".to_owned(),
        }],
        ..Default::default()
    };
    write_session_snapshot(&workspace, &serde_json::to_vec_pretty(&snapshot).unwrap()).unwrap();
    normalize_persisted_session_paths_for_restore(&workspace, &mut snapshot);

    let restored =
        load_latest_session_snapshot_after_quarantine(&workspace, |_| -> anyhow::Result<PathBuf> {
            anyhow::bail!("rename denied")
        })
        .unwrap();

    assert_eq!(restored, Some(snapshot));

    fs::remove_dir_all(workspace).unwrap();
}

#[test]
fn session_bytes_for_write_bounds_loadable_strings_when_candidate_already_fits() {
    let workspace = temp_workspace("write-string-bounds");
    let long_volatile = "v".repeat(PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS + 8);
    let long_display = "d".repeat(PERSISTED_SESSION_DISPLAY_TEXT_MAX_CHARS + 8);
    let long_scrollback = "s".repeat(PERSISTED_SESSION_TERMINAL_SCROLLBACK_MAX_CHARS + 8);
    let long_recovery = "r".repeat(PERSISTED_SESSION_RECOVERY_TEXT_MAX_CHARS + 8);
    let session = PersistedSession {
        workspace_root: workspace.clone(),
        open_files: vec![workspace.join("src/main.rs")],
        project_search_query: long_volatile.clone(),
        source_control_commit_message: long_volatile.clone(),
        source_control_commit_history: vec![long_volatile.clone()],
        terminal_sessions: vec![PersistedTerminalSession {
            scrollback: long_scrollback,
            custom_title: None,
            process_label: Some(long_display.clone()),
            window_title: Some(long_display.clone()),
            ..Default::default()
        }],
        recovery: vec![RecoveredBuffer {
            path: Some(workspace.join("src/main.rs")),
            display_name: long_display.clone(),
            text: long_recovery,
        }],
        recovery_skipped: vec![SkippedRecoveredBuffer {
            path: Some(workspace.join("src/large.rs")),
            display_name: long_display.clone(),
            bytes: 10,
            reason: long_display,
        }],
        ..Default::default()
    };
    let original_recovery_chars = session.recovery[0].text.chars().count();

    let bytes = session_bytes_for_write(&session).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(
        value["project_search_query"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS
    );
    assert_eq!(
        value["source_control_commit_message"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS
    );
    assert_eq!(
        value["source_control_commit_history"][0]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS
    );
    assert_eq!(
        value["terminal_sessions"][0]["scrollback"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        PERSISTED_SESSION_TERMINAL_SCROLLBACK_MAX_CHARS
    );
    assert_eq!(
        value["terminal_sessions"][0]["process_label"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        PERSISTED_SESSION_DISPLAY_TEXT_MAX_CHARS
    );
    assert_eq!(
        value["recovery"][0]["display_name"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        PERSISTED_SESSION_DISPLAY_TEXT_MAX_CHARS
    );
    assert_eq!(
        value["recovery"][0]["text"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        PERSISTED_SESSION_RECOVERY_TEXT_MAX_CHARS
    );
    assert_eq!(
        value["recovery_skipped"][0]["reason"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        PERSISTED_SESSION_DISPLAY_TEXT_MAX_CHARS
    );
    assert_eq!(
        session.recovery[0].text.chars().count(),
        original_recovery_chars
    );
}

#[test]
fn session_bytes_for_write_bounds_skipped_recovery_added_while_trimming() {
    let workspace = temp_workspace("write-skipped-recovery-bounds");
    let recovery_skipped = (0..PERSISTED_SESSION_RECOVERY_SKIPPED_MAX)
        .map(|index| SkippedRecoveredBuffer {
            path: Some(workspace.join(format!("skipped-{index}.rs"))),
            display_name: format!("skipped-{index}.rs"),
            bytes: index,
            reason: "previously skipped".to_owned(),
        })
        .collect::<Vec<_>>();
    let oversized_recovery_bytes = PERSISTED_SESSION_MAX_BYTES_USIZE + 1024;
    let session = PersistedSession {
        workspace_root: workspace.clone(),
        recovery: vec![RecoveredBuffer {
            path: Some(workspace.join("oversized.rs")),
            display_name: "oversized.rs".to_owned(),
            text: "r".repeat(oversized_recovery_bytes),
        }],
        recovery_skipped,
        ..Default::default()
    };

    let bytes = session_bytes_for_write(&session).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let recovery = value["recovery"].as_array().unwrap();
    let skipped = value["recovery_skipped"].as_array().unwrap();

    assert!(recovery.is_empty());
    assert_eq!(skipped.len(), PERSISTED_SESSION_RECOVERY_SKIPPED_MAX);
    assert_eq!(skipped[0]["display_name"].as_str(), Some("skipped-1.rs"));
    assert_eq!(
        skipped.last().unwrap()["display_name"].as_str(),
        Some("oversized.rs")
    );
    assert_eq!(
        skipped.last().unwrap()["bytes"].as_u64(),
        Some(oversized_recovery_bytes as u64)
    );
}

#[test]
fn session_bytes_for_write_clamps_restored_layout_scalars() {
    let workspace = temp_workspace("write-layout-scalars");
    let session = PersistedSession {
        workspace_root: workspace,
        explorer_width: f32::NAN,
        symbols_panel_width: f32::INFINITY,
        diagnostics_panel_width: 9999.0,
        source_control_width: 0.0,
        terminal_height: f32::NEG_INFINITY,
        ..Default::default()
    };

    let bytes = session_bytes_for_write(&session).unwrap();
    let saved: PersistedSession = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(saved.explorer_width, EXPLORER_DEFAULT_WIDTH);
    assert_eq!(saved.symbols_panel_width, SYMBOLS_PANEL_DEFAULT_WIDTH);
    assert_eq!(saved.diagnostics_panel_width, DIAGNOSTICS_PANEL_MAX_WIDTH);
    assert_eq!(saved.source_control_width, SOURCE_CONTROL_MIN_WIDTH);
    assert_eq!(saved.terminal_height, TERMINAL_DEFAULT_HEIGHT);
}

#[test]
fn restore_normalization_clamps_restored_layout_scalars() {
    let root = PathBuf::from("workspace").join("current");
    let mut session = PersistedSession {
        workspace_root: root.clone(),
        explorer_width: f32::NAN,
        symbols_panel_width: f32::INFINITY,
        diagnostics_panel_width: 9999.0,
        source_control_width: 0.0,
        terminal_height: f32::NEG_INFINITY,
        ..Default::default()
    };

    normalize_persisted_session_paths_for_restore(&root, &mut session);

    assert_eq!(session.explorer_width, EXPLORER_DEFAULT_WIDTH);
    assert_eq!(session.symbols_panel_width, SYMBOLS_PANEL_DEFAULT_WIDTH);
    assert_eq!(session.diagnostics_panel_width, DIAGNOSTICS_PANEL_MAX_WIDTH);
    assert_eq!(session.source_control_width, SOURCE_CONTROL_MIN_WIDTH);
    assert_eq!(session.terminal_height, TERMINAL_DEFAULT_HEIGHT);
}

#[test]
fn restore_prunes_stale_active_pane_and_mismatched_pane_view_states() {
    let root = PathBuf::from("workspace").join("current");
    let main = root.join("src/main.rs");
    let lib = root.join("src/lib.rs");
    let mut session = PersistedSession {
        open_files: vec![main.clone(), lib.clone()],
        active_path: Some(lib.clone()),
        pane_paths: vec![None, Some(lib.clone()), Some(main.clone())],
        active_pane_index: Some(0),
        pane_view_states: vec![
            PaneBufferViewState {
                pane_index: 0,
                path: main.clone(),
                scroll_line: 10,
                horizontal_scroll_offset: 0.0,
            },
            PaneBufferViewState {
                pane_index: 1,
                path: main.clone(),
                scroll_line: 20,
                horizontal_scroll_offset: 0.0,
            },
            PaneBufferViewState {
                pane_index: 1,
                path: lib.clone(),
                scroll_line: 30,
                horizontal_scroll_offset: 0.0,
            },
            PaneBufferViewState {
                pane_index: 2,
                path: main.clone(),
                scroll_line: 40,
                horizontal_scroll_offset: 0.0,
            },
            PaneBufferViewState {
                pane_index: 9,
                path: lib.clone(),
                scroll_line: 50,
                horizontal_scroll_offset: 0.0,
            },
        ],
        ..Default::default()
    };

    normalize_persisted_session_paths_for_restore(&root, &mut session);

    assert_eq!(session.active_pane_index, Some(1));
    assert_eq!(
        session
            .pane_view_states
            .iter()
            .map(|state| (state.pane_index, state.path.clone(), state.scroll_line))
            .collect::<Vec<_>>(),
        vec![(1, lib, 30), (2, main, 40)]
    );
}

#[test]
fn restore_path_normalization_rejects_stacked_parent_escapes() {
    let root = PathBuf::from("workspace").join("current");
    let main = root.join("main.rs");
    let mut session = PersistedSession {
        open_files: vec![
            root.join("src").join("..").join("main.rs"),
            root.join("src").join("..").join("..").join("outside.rs"),
            root.join("..").join("current").join("secret.rs"),
            PathBuf::from("src")
                .join("..")
                .join("..")
                .join("outside.rs"),
            PathBuf::from("..").join("current").join("secret.rs"),
            PathBuf::from("../../../workspace/current/secret.rs"),
        ],
        active_path: Some(
            PathBuf::from("src")
                .join("..")
                .join("..")
                .join("outside.rs"),
        ),
        pane_paths: vec![
            Some(root.join("src").join("..").join("main.rs")),
            Some(root.join("src").join("..").join("..").join("outside.rs")),
            Some(root.join("..").join("current").join("secret.rs")),
        ],
        quick_open_recent_files: vec![
            root.join("src").join("..").join("main.rs"),
            root.join("..").join("current").join("secret.rs"),
        ],
        quick_open_query_memory: vec![
            crate::quick_open::QuickOpenQueryMemoryEntry {
                query: "main".to_owned(),
                path: root.join("src").join("..").join("main.rs"),
                uses: 1,
            },
            crate::quick_open::QuickOpenQueryMemoryEntry {
                query: "secret".to_owned(),
                path: root.join("..").join("current").join("secret.rs"),
                uses: 1,
            },
        ],
        navigation_back: vec![
            crate::persistence::PersistedNavigationLocation {
                path: root.join("src").join("..").join("main.rs"),
                line: 1,
                column: 1,
            },
            crate::persistence::PersistedNavigationLocation {
                path: root.join("..").join("current").join("secret.rs"),
                line: 1,
                column: 1,
            },
        ],
        navigation_forward: vec![crate::persistence::PersistedNavigationLocation {
            path: root.join("..").join("current").join("secret.rs"),
            line: 1,
            column: 1,
        }],
        closed_files: vec![
            crate::persistence::PersistedClosedFileEntry {
                path: root.join("src").join("..").join("main.rs"),
                line: 1,
                column: 1,
            },
            crate::persistence::PersistedClosedFileEntry {
                path: root.join("..").join("current").join("secret.rs"),
                line: 1,
                column: 1,
            },
        ],
        recovery: vec![
            RecoveredBuffer {
                path: Some(root.join("src").join("..").join("main.rs")),
                display_name: "main.rs".to_owned(),
                text: "inside".to_owned(),
            },
            RecoveredBuffer {
                path: Some(
                    PathBuf::from("src")
                        .join("..")
                        .join("..")
                        .join("outside.rs"),
                ),
                display_name: "outside.rs".to_owned(),
                text: "outside".to_owned(),
            },
            RecoveredBuffer {
                path: Some(root.join("..").join("current").join("secret.rs")),
                display_name: "secret.rs".to_owned(),
                text: "reentry".to_owned(),
            },
        ],
        recovery_skipped: vec![
            SkippedRecoveredBuffer {
                path: Some(root.join("src").join("..").join("main.rs")),
                display_name: "main.rs".to_owned(),
                bytes: 10,
                reason: "inside".to_owned(),
            },
            SkippedRecoveredBuffer {
                path: Some(root.join("..").join("current").join("secret.rs")),
                display_name: "secret.rs".to_owned(),
                bytes: 10,
                reason: "reentry".to_owned(),
            },
        ],
        ..Default::default()
    };

    normalize_persisted_session_paths_for_restore(&root, &mut session);

    assert_eq!(session.open_files, vec![main.clone()]);
    assert_eq!(session.active_path, None);
    assert_eq!(session.pane_paths, vec![Some(main.clone()), None, None]);
    assert_eq!(session.quick_open_recent_files, vec![main.clone()]);
    assert_eq!(session.quick_open_query_memory.len(), 1);
    assert_eq!(session.quick_open_query_memory[0].path, main.clone());
    assert_eq!(session.navigation_back.len(), 1);
    assert_eq!(session.navigation_forward.len(), 0);
    assert_eq!(session.closed_files.len(), 1);
    assert_eq!(session.recovery[0].path, Some(main.clone()));
    assert_eq!(session.recovery[1].path, None);
    assert_eq!(session.recovery[2].path, None);
    assert_eq!(session.recovery_skipped[0].path, Some(main));
    assert_eq!(session.recovery_skipped[1].path, None);
}

#[test]
fn restore_path_normalization_dedupes_equivalent_path_lists() {
    let root = PathBuf::from("workspace").join("current");
    let main = root.join("src").join("main.rs");
    let src = root.join("src");
    let mut session = PersistedSession {
        open_files: vec![
            root.join("src").join(".").join("main.rs"),
            main.clone(),
            PathBuf::from("src").join("main.rs"),
            root.join("src").join("nested").join("..").join("main.rs"),
        ],
        explorer_expanded: vec![
            root.join(".").join("src"),
            src.clone(),
            PathBuf::from("src"),
            root.join("src").join(".."),
        ],
        quick_open_recent_files: vec![
            PathBuf::from("src").join("main.rs"),
            root.join("src").join(".").join("main.rs"),
            main.clone(),
        ],
        ..Default::default()
    };

    normalize_persisted_session_paths_for_restore(&root, &mut session);

    assert_eq!(session.open_files, vec![main.clone()]);
    assert_eq!(session.explorer_expanded, vec![src]);
    assert_eq!(session.quick_open_recent_files, vec![main]);
}

#[test]
fn restore_normalization_dedupes_equivalent_recovery_paths_and_remaps_state_indices() {
    let root = PathBuf::from("workspace").join("current");
    let main = root.join("src").join("main.rs");
    let main_alias = root.join("src").join(".").join("main.rs");
    let mut session = PersistedSession {
        recovery: vec![
            RecoveredBuffer {
                path: Some(main_alias),
                display_name: "main.rs".to_owned(),
                text: "older".to_owned(),
            },
            RecoveredBuffer {
                path: None,
                display_name: "scratch".to_owned(),
                text: "scratch".to_owned(),
            },
            RecoveredBuffer {
                path: Some(main.clone()),
                display_name: "main.rs".to_owned(),
                text: "newer".to_owned(),
            },
            RecoveredBuffer {
                path: None,
                display_name: "notes".to_owned(),
                text: "notes".to_owned(),
            },
        ],
        recovery_view_states: vec![
            recovery_view_state(0, 1),
            recovery_view_state(1, 2),
            recovery_view_state(3, 4),
        ],
        recovery_history_states: vec![
            recovery_history_state(0),
            recovery_history_state(1),
            recovery_history_state(3),
        ],
        ..Default::default()
    };

    normalize_persisted_session_paths_for_restore(&root, &mut session);

    assert_eq!(session.recovery.len(), 3);
    assert_eq!(session.recovery[0].path, None);
    assert_eq!(session.recovery[0].text, "scratch");
    assert_eq!(session.recovery[1].path, Some(main));
    assert_eq!(session.recovery[1].text, "newer");
    assert_eq!(session.recovery[2].path, None);
    assert_eq!(session.recovery[2].text, "notes");
    assert_eq!(
        session
            .recovery_view_states
            .iter()
            .map(|state| (state.recovery_index, state.cursor_line))
            .collect::<Vec<_>>(),
        vec![(0, 2), (2, 4)]
    );
    assert_eq!(
        session
            .recovery_history_states
            .iter()
            .map(|state| state.recovery_index)
            .collect::<Vec<_>>(),
        vec![0, 2]
    );
}

#[test]
fn restore_path_normalization_normalizes_terminal_cwd_paths() {
    let root = PathBuf::from("workspace").join("current");
    let tools = root.join("tools");
    let mut session = PersistedSession {
        terminal_sessions: vec![
            terminal_session_with_cwd(root.clone()),
            terminal_session_with_cwd(root.join("src").join("..").join("tools")),
            terminal_session_with_cwd(PathBuf::from("tools")),
            terminal_session_with_cwd(PathBuf::from("current").join("tools")),
            terminal_session_with_cwd(root.join("..").join("current").join("tools")),
            terminal_session_with_cwd(root.join("..").join("outside")),
        ],
        ..Default::default()
    };

    normalize_persisted_session_paths_for_restore(&root, &mut session);

    assert_eq!(
        session
            .terminal_sessions
            .iter()
            .map(|terminal| terminal.cwd.clone())
            .collect::<Vec<_>>(),
        vec![
            Some(root),
            Some(tools.clone()),
            Some(tools.clone()),
            Some(tools),
            None,
            None,
        ]
    );
}

#[test]
fn restore_path_normalization_handles_current_dir_root() {
    let mut session = PersistedSession {
        open_files: vec![
            PathBuf::from("src/lib.rs"),
            PathBuf::from("src").join(".").join("main.rs"),
            PathBuf::from("../outside.rs"),
            PathBuf::from("src")
                .join("..")
                .join("..")
                .join("outside.rs"),
            PathBuf::from("."),
            PathBuf::from("src").join(".."),
        ],
        active_path: Some(PathBuf::from("src").join(".").join("main.rs")),
        pane_paths: vec![
            Some(PathBuf::from("src").join("main.rs")),
            Some(PathBuf::from("../outside.rs")),
        ],
        recovery: vec![
            RecoveredBuffer {
                path: Some(PathBuf::from("src/lib.rs")),
                display_name: "lib.rs".to_owned(),
                text: "inside".to_owned(),
            },
            RecoveredBuffer {
                path: Some(PathBuf::from("../outside.rs")),
                display_name: "outside.rs".to_owned(),
                text: "outside".to_owned(),
            },
        ],
        terminal_sessions: vec![
            terminal_session_with_cwd(PathBuf::from(".")),
            terminal_session_with_cwd(PathBuf::from("src").join("..")),
            terminal_session_with_cwd(PathBuf::from("src/tools")),
            terminal_session_with_cwd(PathBuf::from("../outside")),
        ],
        ..Default::default()
    };

    normalize_persisted_session_paths_for_restore(Path::new("."), &mut session);

    assert_eq!(
        session.open_files,
        vec![PathBuf::from("src/lib.rs"), PathBuf::from("src/main.rs")]
    );
    assert_eq!(session.active_path, Some(PathBuf::from("src/main.rs")));
    assert_eq!(
        session.pane_paths,
        vec![Some(PathBuf::from("src/main.rs")), None]
    );
    assert_eq!(session.recovery[0].path, Some(PathBuf::from("src/lib.rs")));
    assert_eq!(session.recovery[1].path, None);
    assert_eq!(
        session
            .terminal_sessions
            .iter()
            .map(|terminal| terminal.cwd.clone())
            .collect::<Vec<_>>(),
        vec![
            Some(PathBuf::from(".")),
            Some(PathBuf::from(".")),
            Some(PathBuf::from("src/tools")),
            None,
        ]
    );
}

#[test]
fn session_snapshot_prune_keeps_newest_modified_not_newest_name() {
    let workspace = temp_workspace("prune-mtime");
    fs::create_dir_all(&workspace).unwrap();
    let dir = session_snapshots_dir(&workspace);
    fs::create_dir_all(&dir).unwrap();
    write_snapshot_with_modified(
        &dir,
        "session.1.0.0.json",
        snapshot_modified_time_for_index(0),
    );
    for index in 2..=10_u64 {
        write_snapshot_with_modified(
            &dir,
            &format!("session.{index}.0.0.json"),
            snapshot_modified_time_for_index(index - 1),
        );
    }

    prune_session_snapshots(&dir).unwrap();

    assert!(dir.join("session.1.0.0.json").exists());
    assert!(dir.join("session.8.0.0.json").exists());
    assert!(!dir.join("session.9.0.0.json").exists());
    assert!(!dir.join("session.10.0.0.json").exists());
    assert_eq!(count_snapshot_files(&dir), MAX_SESSION_SNAPSHOTS);

    fs::remove_dir_all(workspace).unwrap();
}

#[tokio::test]
async fn session_snapshot_prune_async_keeps_newest_modified_not_newest_name() {
    let workspace = temp_workspace("prune-mtime-async");
    fs::create_dir_all(&workspace).unwrap();
    let dir = session_snapshots_dir(&workspace);
    fs::create_dir_all(&dir).unwrap();
    write_snapshot_with_modified(
        &dir,
        "session.1.0.0.json",
        snapshot_modified_time_for_index(0),
    );
    for index in 2..=10_u64 {
        write_snapshot_with_modified(
            &dir,
            &format!("session.{index}.0.0.json"),
            snapshot_modified_time_for_index(index - 1),
        );
    }

    prune_session_snapshots_async(&dir).await.unwrap();

    assert!(dir.join("session.1.0.0.json").exists());
    assert!(!dir.join("session.9.0.0.json").exists());
    assert!(!dir.join("session.10.0.0.json").exists());
    assert_eq!(count_snapshot_files(&dir), MAX_SESSION_SNAPSHOTS);

    fs::remove_dir_all(workspace).unwrap();
}

#[test]
fn session_bytes_for_write_evicts_largest_recovery_entry_first() {
    let workspace = temp_workspace("write-evict-largest-recovery");
    let huge_bytes = PERSISTED_SESSION_MAX_BYTES_USIZE + 4096;
    let session = PersistedSession {
        workspace_root: workspace.clone(),
        recovery: vec![
            RecoveredBuffer {
                path: Some(workspace.join("src/small.rs")),
                display_name: "small.rs".to_owned(),
                text: "user text".to_owned(),
            },
            RecoveredBuffer {
                path: Some(workspace.join("src/huge.rs")),
                display_name: "huge.rs".to_owned(),
                text: "r".repeat(huge_bytes),
            },
        ],
        recovery_view_states: vec![recovery_view_state(0, 3), recovery_view_state(1, 7)],
        ..Default::default()
    };

    let bytes = session_bytes_for_write(&session).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    let recovery = value["recovery"].as_array().unwrap();
    assert_eq!(recovery.len(), 1);
    assert_eq!(recovery[0]["display_name"].as_str(), Some("small.rs"));
    assert_eq!(recovery[0]["text"].as_str(), Some("user text"));
    let view_states = value["recovery_view_states"].as_array().unwrap();
    assert_eq!(view_states.len(), 1);
    assert_eq!(view_states[0]["cursor_line"].as_u64(), Some(3));
    let skipped = value["recovery_skipped"].as_array().unwrap();
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0]["display_name"].as_str(), Some("huge.rs"));
    assert_eq!(skipped[0]["bytes"].as_u64(), Some(huge_bytes as u64));
}

fn write_snapshot_with_modified(dir: &Path, file_name: &str, modified: SystemTime) {
    let path = dir.join(file_name);
    fs::write(&path, "stale snapshot json").unwrap();
    let file = fs::File::options().write(true).open(&path).unwrap();
    file.set_modified(modified).unwrap();
}

fn snapshot_modified_time_for_index(seconds_before_base: u64) -> SystemTime {
    let base = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    base - Duration::from_secs(seconds_before_base)
}

fn count_snapshot_files(dir: &Path) -> usize {
    fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with("session.") && name.ends_with(".json"))
        })
        .count()
}

fn recovery_view_state(recovery_index: usize, cursor_line: usize) -> RecoveredBufferViewState {
    RecoveredBufferViewState {
        recovery_index,
        cursor_line,
        cursor_column: 0,
        scroll_line: cursor_line,
        horizontal_scroll_offset: 0.0,
        selections: Vec::new(),
    }
}

fn recovery_history_state(recovery_index: usize) -> RecoveredBufferHistoryState {
    RecoveredBufferHistoryState {
        recovery_index,
        history: BufferHistorySnapshot {
            len_chars: 0,
            checksum: 0,
            undo: Vec::new(),
            redo: Vec::new(),
        },
    }
}

fn terminal_session_with_cwd(cwd: PathBuf) -> PersistedTerminalSession {
    PersistedTerminalSession {
        cwd: Some(cwd),
        scrollback: String::new(),
        scrollback_offset: 0,
        custom_title: None,
        process_label: None,
        process_status: None,
        window_title: None,
    }
}
