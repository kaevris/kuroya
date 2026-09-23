use super::*;
use crate::persistence_session::{
    MAX_SESSION_SNAPSHOTS, PERSISTED_SESSION_MAX_BYTES, session_bytes_for_write,
};
use crate::{
    buffer_find_history::MAX_BUFFER_FIND_HISTORY,
    command_palette_items::MAX_COMMAND_PALETTE_QUERY_MEMORY,
    lsp_workspace_symbol_ranking::MAX_WORKSPACE_SYMBOL_QUERY_MEMORY,
    persistence_models::{
        PERSISTED_SESSION_DISPLAY_TEXT_MAX_CHARS, PERSISTED_SESSION_PATH_TEXT_MAX_CHARS,
        PERSISTED_SESSION_PATHS_MAX, PERSISTED_SESSION_RECOVERY_BUFFERS_MAX,
        PERSISTED_SESSION_RECOVERY_TEXT_MAX_CHARS, PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
    },
    project_search_state::MAX_PROJECT_SEARCH_RECENT_QUERIES,
    quick_open::MAX_QUICK_OPEN_QUERY_MEMORY,
};

mod restore;
mod restore_backups;
mod saving;

#[test]
fn session_defaults_terminal_layout_for_older_snapshots() {
    let session: PersistedSession = serde_json::from_str(
        r#"{
                "workspace_root": "workspace",
                "open_files": [],
                "active_path": null,
                "pane_paths": [],
                "pane_weights": [],
                "recent_projects": [],
                "recovery": []
            }"#,
    )
    .unwrap();

    assert!(!session.terminal_visible);
    assert_eq!(session.terminal_height, 220.0);
    assert!(session.view_states.is_empty());
    assert!(session.history_states.is_empty());
    assert!(session.recovery_view_states.is_empty());
    assert!(session.recovery_history_states.is_empty());
    assert!(session.fold_states.is_empty());
    assert!(session.terminal_sessions.is_empty());
    assert_eq!(session.terminal_active_session, 0);
    assert!(!session.terminal_split_view);
    assert!(session.terminal_split_weights.is_empty());
    assert_eq!(session.explorer_width, 260.0);
    assert!(session.explorer_expanded.is_empty());
    assert!(session.explorer_revealed_path.is_none());
    assert!(!session.project_search_open);
    assert!(session.project_search_query.is_empty());
    assert!(!session.project_search_case_sensitive);
    assert!(!session.project_search_whole_word);
    assert!(!session.project_search_regex);
    assert!(session.project_search_include.is_empty());
    assert!(session.project_search_exclude.is_empty());
    assert!(session.project_search_recent.is_empty());
    assert!(!session.buffer_find_open);
    assert!(session.buffer_find_query.is_empty());
    assert!(session.buffer_find_replacement.is_empty());
    assert!(!session.buffer_find_case_sensitive);
    assert!(!session.buffer_find_whole_word);
    assert!(!session.buffer_find_regex);
    assert!(!session.buffer_find_preserve_case);
    assert!(session.buffer_find_query_history.is_empty());
    assert!(session.buffer_find_replacement_history.is_empty());
    assert!(!session.settings_panel_open);
    assert!(!session.theme_picker_open);
    assert!(!session.keybindings_open);
    assert!(!session.symbols_panel_open);
    assert_eq!(session.symbols_panel_placement, PanelPlacement::DockedRight);
    assert_eq!(session.symbols_panel_width, 300.0);
    assert!(!session.diagnostics_panel_open);
    assert_eq!(
        session.diagnostics_panel_placement,
        PanelPlacement::DockedRight
    );
    assert_eq!(session.diagnostics_panel_width, 340.0);
    assert!(!session.source_control_open);
    assert_eq!(
        session.source_control_placement,
        PanelPlacement::DockedRight
    );
    assert_eq!(session.source_control_width, 320.0);
    assert!(session.source_control_query.is_empty());
    assert_eq!(
        session.source_control_view,
        PersistedSourceControlViewMode::List
    );
    assert_eq!(
        session.source_control_sort,
        PersistedSourceControlSortMode::Path
    );
    assert!(session.source_control_commit_message.is_empty());
    assert!(session.source_control_commit_history.is_empty());
    assert!(session.source_control_stash_message.is_empty());
    assert!(session.source_control_stash_query.is_empty());
    assert!(!session.source_control_stashes_open);
    assert!(!session.source_control_history_open);
    assert!(session.source_control_history_query.is_empty());
    assert!(!session.source_control_unstaged_collapsed);
    assert!(!session.source_control_untracked_collapsed);
    assert!(!session.source_control_staged_collapsed);
    assert!(session.quick_open_recent_files.is_empty());
    assert!(session.quick_open_query_memory.is_empty());
    assert!(session.workspace_symbol_query_memory.is_empty());
    assert!(session.command_recent.is_empty());
    assert!(session.command_query_memory.is_empty());
    assert!(session.navigation_back.is_empty());
    assert!(session.navigation_forward.is_empty());
    assert!(session.closed_files.is_empty());
    assert!(session.recovery_skipped.is_empty());
}

#[test]
fn terminal_session_defaults_scrollback_offset_for_older_snapshots() {
    let session: PersistedTerminalSession = serde_json::from_str(
        r#"{
                "cwd": "workspace",
                "scrollback": "line\n",
                "process_label": "shell",
                "window_title": "terminal"
            }"#,
    )
    .unwrap();

    assert_eq!(session.scrollback_offset, 0);
    assert_eq!(session.process_status, None);
}

#[test]
fn terminal_session_restores_persisted_process_status() {
    let session: PersistedTerminalSession = serde_json::from_str(
        r#"{
                "cwd": "workspace",
                "scrollback": "line\n",
                "process_label": "cargo test",
                "process_status": {
                    "state": "exited",
                    "exit_code": 17
                },
                "window_title": "terminal"
            }"#,
    )
    .unwrap();

    assert_eq!(
        session.process_status,
        Some(PersistedTerminalProcessStatus::Exited {
            exit_code: Some(17)
        })
    );
}

#[test]
fn terminal_session_process_status_tolerates_future_states() {
    let session: PersistedTerminalSession = serde_json::from_str(
        r#"{
                "cwd": "workspace",
                "scrollback": "line\n",
                "process_label": "cargo test",
                "process_status": {
                    "state": "paused"
                }
            }"#,
    )
    .unwrap();

    assert_eq!(
        session.process_status,
        Some(PersistedTerminalProcessStatus::Unknown)
    );
}

#[test]
fn session_defaults_missing_legacy_required_fields() {
    let session: PersistedSession = serde_json::from_str(
        r#"{
                "workspace_root": "workspace"
            }"#,
    )
    .unwrap();

    assert_eq!(session.workspace_root, PathBuf::from("workspace"));
    assert!(session.open_files.is_empty());
    assert_eq!(session.active_path, None);
    assert!(session.pane_paths.is_empty());
    assert!(session.recent_projects.is_empty());
    assert!(session.recovery.is_empty());
}

#[test]
fn session_deserialize_bounds_restored_strings_paths_and_lists() {
    let workspace = temp_workspace("deserialize-bounds");
    let workspace_text = workspace.to_string_lossy().to_string();
    let long_volatile = "v".repeat(PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS + 8);
    let long_display = "d".repeat(PERSISTED_SESSION_DISPLAY_TEXT_MAX_CHARS + 8);
    let long_recovery = "r".repeat(PERSISTED_SESSION_RECOVERY_TEXT_MAX_CHARS + 8);
    let oversized_path = format!(
        "{}.rs",
        "p".repeat(PERSISTED_SESSION_PATH_TEXT_MAX_CHARS + 1)
    );
    let main = workspace.join("src/main.rs").to_string_lossy().to_string();
    let open_files = std::iter::once(oversized_path.clone())
        .chain((0..PERSISTED_SESSION_PATHS_MAX + 8).map(|index| {
            workspace
                .join(format!("src/file-{index}.rs"))
                .to_string_lossy()
                .to_string()
        }))
        .collect::<Vec<_>>();
    let project_search_recent = (0..MAX_PROJECT_SEARCH_RECENT_QUERIES + 8)
        .map(|_| {
            serde_json::json!({
                "query": long_volatile.clone(),
                "case_sensitive": true,
                "whole_word": true,
                "include": long_volatile.clone(),
                "exclude": long_volatile.clone()
            })
        })
        .collect::<Vec<_>>();
    let buffer_find_history = (0..MAX_BUFFER_FIND_HISTORY + 8)
        .map(|_| long_volatile.clone())
        .collect::<Vec<_>>();
    let quick_open_query_memory = (0..MAX_QUICK_OPEN_QUERY_MEMORY + 8)
        .map(|_| {
            serde_json::json!({
                "query": long_volatile.clone(),
                "path": main.clone(),
                "uses": 2
            })
        })
        .collect::<Vec<_>>();
    let workspace_symbol_query_memory = (0..MAX_WORKSPACE_SYMBOL_QUERY_MEMORY + 8)
        .map(|_| {
            serde_json::json!({
                "query": long_volatile.clone(),
                "path": main.clone(),
                "name": long_volatile.clone(),
                "kind": 12,
                "line": 4,
                "column": 2,
                "uses": 2
            })
        })
        .collect::<Vec<_>>();
    let command_query_memory = (0..MAX_COMMAND_PALETTE_QUERY_MEMORY + 8)
        .map(|_| {
            serde_json::json!({
                "query": long_volatile.clone(),
                "command": "ToggleQuickOpen",
                "uses": 2
            })
        })
        .collect::<Vec<_>>();
    let recovery = std::iter::once(serde_json::json!({
        "path": oversized_path.clone(),
        "display_name": long_display.clone(),
        "text": long_recovery.clone()
    }))
    .chain(
        (0..PERSISTED_SESSION_RECOVERY_BUFFERS_MAX + 8).map(|index| {
            serde_json::json!({
                "path": main.clone(),
                "display_name": format!("recovered-{index}.rs"),
                "text": "small recovery"
            })
        }),
    )
    .collect::<Vec<_>>();

    let session: PersistedSession = serde_json::from_value(serde_json::json!({
        "workspace_root": workspace_text,
        "open_files": open_files,
        "active_path": oversized_path.clone(),
        "pane_paths": [main.clone()],
        "project_search_query": long_volatile.clone(),
        "project_search_recent": project_search_recent,
        "buffer_find_query_history": buffer_find_history,
        "quick_open_query_memory": quick_open_query_memory,
        "workspace_symbol_query_memory": workspace_symbol_query_memory,
        "command_query_memory": command_query_memory,
        "recovery": recovery
    }))
    .unwrap();

    assert_eq!(session.open_files.len(), PERSISTED_SESSION_PATHS_MAX);
    // Over-length paths are pruned from path lists instead of being
    // restored as empty placeholder paths.
    assert_eq!(session.open_files[0], workspace.join("src/file-0.rs"));
    // An over-length single path restores as absent rather than empty.
    assert_eq!(session.active_path, None);
    assert_eq!(
        session.project_search_query.chars().count(),
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS
    );
    assert_eq!(
        session.project_search_recent.len(),
        MAX_PROJECT_SEARCH_RECENT_QUERIES
    );
    assert_eq!(
        session.project_search_recent[0].query.chars().count(),
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS
    );
    assert_eq!(
        session.buffer_find_query_history.len(),
        MAX_BUFFER_FIND_HISTORY
    );
    assert_eq!(
        session.buffer_find_query_history[0].chars().count(),
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS
    );
    assert_eq!(
        session.quick_open_query_memory.len(),
        MAX_QUICK_OPEN_QUERY_MEMORY
    );
    assert_eq!(
        session.quick_open_query_memory[0].query.chars().count(),
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS
    );
    assert_eq!(
        session.workspace_symbol_query_memory.len(),
        MAX_WORKSPACE_SYMBOL_QUERY_MEMORY
    );
    assert_eq!(
        session.workspace_symbol_query_memory[0]
            .name
            .chars()
            .count(),
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS
    );
    assert_eq!(
        session.command_query_memory.len(),
        MAX_COMMAND_PALETTE_QUERY_MEMORY
    );
    assert_eq!(
        session.command_query_memory[0].query.chars().count(),
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS
    );
    assert_eq!(
        session.recovery.len(),
        PERSISTED_SESSION_RECOVERY_BUFFERS_MAX
    );
    // The over-length path is dropped so no empty placeholder path is kept,
    // while the recovered text survives as an untitled buffer.
    assert_eq!(session.recovery[0].path, None);
    assert_eq!(
        session.recovery[0].display_name.chars().count(),
        PERSISTED_SESSION_DISPLAY_TEXT_MAX_CHARS
    );
    assert_eq!(
        session.recovery[0].text.chars().count(),
        PERSISTED_SESSION_RECOVERY_TEXT_MAX_CHARS
    );
}

#[test]
fn session_serializes_explorer_ui_state() {
    let session: PersistedSession = serde_json::from_str(
        r#"{
                "workspace_root": "workspace",
                "open_files": [],
                "active_path": null,
                "pane_paths": [],
                "pane_weights": [],
                "explorer_expanded": ["workspace/src", "workspace/crates/kuroya-app"],
                "explorer_revealed_path": "workspace/src/main.rs",
                "recent_projects": [],
                "recovery": []
            }"#,
    )
    .unwrap();

    assert_eq!(
        session.explorer_expanded,
        vec![
            PathBuf::from("workspace/src"),
            PathBuf::from("workspace/crates/kuroya-app")
        ]
    );
    assert_eq!(
        session.explorer_revealed_path,
        Some(PathBuf::from("workspace/src/main.rs"))
    );

    let encoded = serde_json::to_value(&session).unwrap();
    assert_eq!(encoded["explorer_expanded"][0], "workspace/src");
    assert_eq!(encoded["explorer_revealed_path"], "workspace/src/main.rs");
}

#[test]
fn session_serializes_project_search_state_and_drops_legacy_layout_fields() {
    let session: PersistedSession = serde_json::from_str(
        r#"{
                "workspace_root": "workspace",
                "open_files": [],
                "active_path": null,
                "pane_paths": [],
                "pane_weights": [],
                "project_search_open": true,
                "project_search_placement": "floating",
                "project_search_width": 414.0,
                "project_search_query": "SearchTerm",
                "project_search_case_sensitive": true,
                "project_search_whole_word": true,
                "project_search_regex": true,
                "project_search_include": "src/**/*.rs",
                "project_search_exclude": "target/**,*.snap",
                "project_search_recent": [
                    {
                        "query": "PreviousTerm",
                        "case_sensitive": true,
                        "whole_word": false,
                        "regex": true,
                        "include": "crates/**/*.rs",
                        "exclude": "target/**"
                    }
                ],
                "recent_projects": [],
                "recovery": []
            }"#,
    )
    .unwrap();

    assert!(session.project_search_open);
    assert_eq!(session.project_search_query, "SearchTerm");
    assert!(session.project_search_case_sensitive);
    assert!(session.project_search_whole_word);
    assert!(session.project_search_regex);
    assert_eq!(session.project_search_include, "src/**/*.rs");
    assert_eq!(session.project_search_exclude, "target/**,*.snap");
    assert_eq!(session.project_search_recent.len(), 1);
    assert_eq!(session.project_search_recent[0].query, "PreviousTerm");
    assert!(session.project_search_recent[0].case_sensitive);
    assert!(session.project_search_recent[0].regex);

    let encoded = serde_json::to_value(&session).unwrap();
    assert!(encoded.get("project_search_placement").is_none());
    assert!(encoded.get("project_search_width").is_none());
    assert_eq!(encoded["project_search_query"], "SearchTerm");
    assert_eq!(encoded["project_search_case_sensitive"], true);
    assert_eq!(encoded["project_search_whole_word"], true);
    assert_eq!(encoded["project_search_regex"], true);
    assert_eq!(encoded["project_search_include"], "src/**/*.rs");
    assert_eq!(encoded["project_search_exclude"], "target/**,*.snap");
    assert_eq!(encoded["project_search_recent"][0]["query"], "PreviousTerm");
    assert_eq!(encoded["project_search_recent"][0]["regex"], true);
}

#[test]
fn session_serializes_buffer_find_ui_state_and_history() {
    let session: PersistedSession = serde_json::from_str(
        r#"{
                "workspace_root": "workspace",
                "open_files": [],
                "active_path": null,
                "pane_paths": [],
                "pane_weights": [],
                "buffer_find_open": true,
                "buffer_find_query": "needle",
                "buffer_find_replacement": "replacement",
                "buffer_find_case_sensitive": true,
                "buffer_find_whole_word": true,
                "buffer_find_regex": true,
                "buffer_find_preserve_case": true,
                "buffer_find_query_history": ["needle", "previous"],
                "buffer_find_replacement_history": ["replacement", "other"],
                "recent_projects": [],
                "recovery": []
            }"#,
    )
    .unwrap();

    assert!(session.buffer_find_open);
    assert_eq!(session.buffer_find_query, "needle");
    assert_eq!(session.buffer_find_replacement, "replacement");
    assert!(session.buffer_find_case_sensitive);
    assert!(session.buffer_find_whole_word);
    assert!(session.buffer_find_regex);
    assert!(session.buffer_find_preserve_case);
    assert_eq!(
        session.buffer_find_query_history,
        vec!["needle".to_owned(), "previous".to_owned()]
    );
    assert_eq!(
        session.buffer_find_replacement_history,
        vec!["replacement".to_owned(), "other".to_owned()]
    );

    let encoded = serde_json::to_value(&session).unwrap();
    assert_eq!(encoded["buffer_find_open"], true);
    assert_eq!(encoded["buffer_find_query"], "needle");
    assert_eq!(encoded["buffer_find_replacement"], "replacement");
    assert_eq!(encoded["buffer_find_case_sensitive"], true);
    assert_eq!(encoded["buffer_find_whole_word"], true);
    assert_eq!(encoded["buffer_find_regex"], true);
    assert_eq!(encoded["buffer_find_preserve_case"], true);
    assert_eq!(encoded["buffer_find_query_history"][0], "needle");
    assert_eq!(encoded["buffer_find_replacement_history"][0], "replacement");
}

#[test]
fn session_serializes_persistent_ui_overlay_state() {
    let session: PersistedSession = serde_json::from_str(
        r#"{
                "workspace_root": "workspace",
                "open_files": [],
                "active_path": null,
                "pane_paths": [],
                "pane_weights": [],
                "settings_panel_open": true,
                "theme_picker_open": true,
                "keybindings_open": true,
                "recent_projects": [],
                "recovery": []
            }"#,
    )
    .unwrap();

    assert!(session.settings_panel_open);
    assert!(session.theme_picker_open);
    assert!(session.keybindings_open);

    let encoded = serde_json::to_value(&session).unwrap();
    assert_eq!(encoded["settings_panel_open"], true);
    assert_eq!(encoded["theme_picker_open"], true);
    assert_eq!(encoded["keybindings_open"], true);
}

#[test]
fn session_serializes_quick_open_query_memory() {
    let session: PersistedSession = serde_json::from_str(
        r#"{
                "workspace_root": "workspace",
                "open_files": [],
                "active_path": null,
                "pane_paths": [],
                "pane_weights": [],
                "quick_open_query_memory": [
                    {
                        "query": "main",
                        "path": "workspace/src/main.rs",
                        "uses": 3
                    }
                ],
                "recent_projects": [],
                "recovery": []
            }"#,
    )
    .unwrap();

    assert_eq!(
        session.quick_open_query_memory,
        vec![crate::quick_open::QuickOpenQueryMemoryEntry {
            query: "main".to_owned(),
            path: PathBuf::from("workspace/src/main.rs"),
            uses: 3,
        }]
    );

    let encoded = serde_json::to_value(&session).unwrap();
    assert_eq!(encoded["quick_open_query_memory"][0]["query"], "main");
    assert_eq!(
        encoded["quick_open_query_memory"][0]["path"],
        "workspace/src/main.rs"
    );
    assert_eq!(encoded["quick_open_query_memory"][0]["uses"], 3);
}

#[test]
fn session_serializes_workspace_symbol_query_memory() {
    let session: PersistedSession = serde_json::from_str(
        r#"{
                "workspace_root": "workspace",
                "open_files": [],
                "active_path": null,
                "pane_paths": [],
                "pane_weights": [],
                "workspace_symbol_query_memory": [
                    {
                        "query": "main",
                        "path": "workspace/src/main.rs",
                        "name": "main_symbol",
                        "kind": 12,
                        "line": 4,
                        "column": 2,
                        "uses": 3
                    }
                ],
                "recent_projects": [],
                "recovery": []
            }"#,
    )
    .unwrap();

    assert_eq!(
        session.workspace_symbol_query_memory,
        vec![
            crate::lsp_workspace_symbol_ranking::WorkspaceSymbolQueryMemoryEntry {
                query: "main".to_owned(),
                path: PathBuf::from("workspace/src/main.rs"),
                name: "main_symbol".to_owned(),
                kind: 12,
                line: 4,
                column: 2,
                uses: 3,
            }
        ]
    );

    let encoded = serde_json::to_value(&session).unwrap();
    assert_eq!(encoded["workspace_symbol_query_memory"][0]["query"], "main");
    assert_eq!(
        encoded["workspace_symbol_query_memory"][0]["path"],
        "workspace/src/main.rs"
    );
    assert_eq!(
        encoded["workspace_symbol_query_memory"][0]["name"],
        "main_symbol"
    );
    assert_eq!(encoded["workspace_symbol_query_memory"][0]["kind"], 12);
    assert_eq!(encoded["workspace_symbol_query_memory"][0]["line"], 4);
    assert_eq!(encoded["workspace_symbol_query_memory"][0]["column"], 2);
    assert_eq!(encoded["workspace_symbol_query_memory"][0]["uses"], 3);
}

#[test]
fn session_defaults_workspace_symbol_query_memory_entry_legacy_fields() {
    let session: PersistedSession = serde_json::from_str(
        r#"{
                "workspace_root": "workspace",
                "workspace_symbol_query_memory": [
                    {
                        "query": "main",
                        "path": "workspace/src/main.rs",
                        "name": "main_symbol",
                        "line": 4,
                        "column": 2
                    }
                ]
            }"#,
    )
    .unwrap();

    assert_eq!(session.workspace_symbol_query_memory.len(), 1);
    let entry = &session.workspace_symbol_query_memory[0];
    assert_eq!(entry.kind, 0);
    assert_eq!(entry.uses, 1);
}

#[test]
fn session_bytes_for_write_sanitizes_oversized_workspace_symbol_query_memory() {
    let workspace = temp_workspace("workspace-symbol-memory-sized");
    fs::create_dir_all(&workspace).unwrap();

    let mut session = sample_session(&workspace, "small recovery");
    session.workspace_symbol_query_memory = vec![
        crate::lsp_workspace_symbol_ranking::WorkspaceSymbolQueryMemoryEntry {
            query: format!(" Main\n{}\u{202e}Query ", "x".repeat(512)),
            path: workspace.join("src/main.rs"),
            name: format!(
                "Symbol {}\n\t{}\u{202e}",
                "x".repeat(usize::try_from(PERSISTED_SESSION_MAX_BYTES).unwrap() + 1024),
                "tail".repeat(64)
            ),
            kind: 12,
            line: 4,
            column: 2,
            uses: 3,
        },
    ];

    let bytes = session_bytes_for_write(&session).unwrap();

    assert!(u64::try_from(bytes.len()).unwrap() <= PERSISTED_SESSION_MAX_BYTES);
    let restored: PersistedSession = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored.workspace_symbol_query_memory.len(), 1);
    let entry = &restored.workspace_symbol_query_memory[0];
    assert!(entry.query.chars().count() <= 128);
    assert!(entry.name.chars().count() <= 256);
    assert!(!entry.query.contains('\n'));
    assert!(!entry.name.contains('\n'));
    assert!(!entry.name.contains('\t'));
    assert!(!entry.name.contains('\u{202e}'));

    remove_workspace(&workspace);
}

#[test]
fn session_bytes_for_write_sanitizes_workspace_symbol_query_memory() {
    let workspace = temp_workspace("workspace-symbol-memory-sanitized");
    fs::create_dir_all(&workspace).unwrap();

    let mut session = sample_session(&workspace, "small recovery");
    session.workspace_symbol_query_memory = vec![
        crate::lsp_workspace_symbol_ranking::WorkspaceSymbolQueryMemoryEntry {
            query: " Main\nQuery\u{202e} ".to_owned(),
            path: workspace.join("src/main.rs"),
            name: " Main\tSymbol\u{202e} ".to_owned(),
            kind: 12,
            line: 0,
            column: 0,
            uses: 0,
        },
        crate::lsp_workspace_symbol_ranking::WorkspaceSymbolQueryMemoryEntry {
            query: "main query".to_owned(),
            path: workspace.join("src/main.rs"),
            name: "Main Symbol".to_owned(),
            kind: 12,
            line: 4,
            column: 2,
            uses: 7,
        },
        crate::lsp_workspace_symbol_ranking::WorkspaceSymbolQueryMemoryEntry {
            query: "\u{202e}\t".to_owned(),
            path: workspace.join("src/lib.rs"),
            name: "Lib Symbol".to_owned(),
            kind: 12,
            line: 1,
            column: 1,
            uses: 3,
        },
        crate::lsp_workspace_symbol_ranking::WorkspaceSymbolQueryMemoryEntry {
            query: "other".to_owned(),
            path: workspace.join("..").join("outside/main.rs"),
            name: "Other Symbol".to_owned(),
            kind: 12,
            line: 1,
            column: 1,
            uses: 3,
        },
    ];

    let bytes = session_bytes_for_write(&session).unwrap();

    let restored: PersistedSession = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        restored.workspace_symbol_query_memory,
        vec![
            crate::lsp_workspace_symbol_ranking::WorkspaceSymbolQueryMemoryEntry {
                query: "main query".to_owned(),
                path: workspace.join("src/main.rs"),
                name: "Main Symbol".to_owned(),
                kind: 12,
                line: 1,
                column: 1,
                uses: 7,
            },
        ]
    );

    remove_workspace(&workspace);
}

#[test]
fn session_bytes_for_write_normalizes_workspace_paths() {
    let workspace = temp_workspace("write-path-normalization");
    fs::create_dir_all(&workspace).unwrap();
    let main = workspace.join("src/main.rs");
    let tools = workspace.join("tools");
    let messy_main = workspace.join("src").join("..").join("src").join("main.rs");
    let messy_tools = workspace.join("src").join("..").join("tools");
    let reentry = workspace
        .join("..")
        .join(workspace.file_name().unwrap())
        .join("secret.rs");
    let reentry_tools = workspace
        .join("..")
        .join(workspace.file_name().unwrap())
        .join("tools");
    let workspace_prefixed_tools = PathBuf::from(workspace.file_name().unwrap()).join("tools");
    let outside = workspace.join("..").join("outside.rs");

    let mut session = sample_session(&workspace, "small recovery");
    session.open_files = vec![messy_main.clone(), reentry.clone(), outside.clone()];
    session.active_path = Some(reentry.clone());
    session.pane_paths = vec![Some(messy_main.clone()), Some(reentry.clone())];
    session.quick_open_recent_files = vec![messy_main.clone(), reentry.clone(), outside.clone()];
    session.quick_open_query_memory = vec![
        crate::quick_open::QuickOpenQueryMemoryEntry {
            query: "main".to_owned(),
            path: messy_main.clone(),
            uses: 1,
        },
        crate::quick_open::QuickOpenQueryMemoryEntry {
            query: "secret".to_owned(),
            path: reentry.clone(),
            uses: 1,
        },
    ];
    session.recovery = vec![RecoveredBuffer {
        path: Some(reentry.clone()),
        display_name: "secret.rs".to_owned(),
        text: "reentry text survives".to_owned(),
    }];
    session.terminal_sessions = vec![
        PersistedTerminalSession {
            cwd: Some(workspace.clone()),
            ..Default::default()
        },
        PersistedTerminalSession {
            cwd: Some(messy_tools),
            ..Default::default()
        },
        PersistedTerminalSession {
            cwd: Some(workspace_prefixed_tools),
            ..Default::default()
        },
        PersistedTerminalSession {
            cwd: Some(reentry_tools),
            ..Default::default()
        },
        PersistedTerminalSession {
            cwd: Some(outside.clone()),
            ..Default::default()
        },
    ];
    session.recovery_skipped = vec![SkippedRecoveredBuffer {
        path: Some(reentry),
        display_name: "secret.rs".to_owned(),
        bytes: 42,
        reason: "too large".to_owned(),
    }];

    let bytes = session_bytes_for_write(&session).unwrap();

    let restored: PersistedSession = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored.open_files, vec![main.clone()]);
    assert_eq!(restored.active_path, None);
    assert_eq!(restored.pane_paths, vec![Some(main.clone()), None]);
    assert_eq!(restored.quick_open_recent_files, vec![main.clone()]);
    assert_eq!(restored.quick_open_query_memory.len(), 1);
    assert_eq!(restored.quick_open_query_memory[0].path, main);
    assert_eq!(restored.recovery[0].path, None);
    assert_eq!(restored.recovery[0].text, "reentry text survives");
    assert_eq!(restored.terminal_sessions[0].cwd, Some(workspace.clone()));
    assert_eq!(restored.terminal_sessions[1].cwd, Some(tools.clone()));
    assert_eq!(restored.terminal_sessions[2].cwd, Some(tools));
    assert_eq!(restored.terminal_sessions[3].cwd, None);
    assert_eq!(restored.terminal_sessions[4].cwd, None);
    assert_eq!(restored.recovery_skipped[0].path, None);

    remove_workspace(&workspace);
}

#[test]
fn session_serializes_command_palette_query_memory() {
    let session: PersistedSession = serde_json::from_str(
        r#"{
                "workspace_root": "workspace",
                "open_files": [],
                "active_path": null,
                "pane_paths": [],
                "pane_weights": [],
                "command_query_memory": [
                    {
                        "query": "git",
                        "command": "ToggleGitHistory",
                        "uses": 3
                    }
                ],
                "recent_projects": [],
                "recovery": []
            }"#,
    )
    .unwrap();

    assert_eq!(
        session.command_query_memory,
        vec![
            crate::command_palette_items::CommandPaletteQueryMemoryEntry {
                query: "git".to_owned(),
                command: Command::ToggleGitHistory,
                uses: 3,
            }
        ]
    );

    let encoded = serde_json::to_value(&session).unwrap();
    assert_eq!(encoded["command_query_memory"][0]["query"], "git");
    assert_eq!(
        encoded["command_query_memory"][0]["command"],
        "ToggleGitHistory"
    );
    assert_eq!(encoded["command_query_memory"][0]["uses"], 3);
}

#[test]
fn session_bytes_for_write_sanitizes_command_palette_query_memory() {
    let workspace = temp_workspace("command-query-memory-sanitized");
    fs::create_dir_all(&workspace).unwrap();

    let mut session = sample_session(&workspace, "small recovery");
    session.command_query_memory = vec![
        crate::command_palette_items::CommandPaletteQueryMemoryEntry {
            query: " Git\nHistory\u{202e} ".to_owned(),
            command: Command::ToggleGitHistory,
            uses: 0,
        },
        crate::command_palette_items::CommandPaletteQueryMemoryEntry {
            query: "git history".to_owned(),
            command: Command::ToggleGitHistory,
            uses: 9,
        },
        crate::command_palette_items::CommandPaletteQueryMemoryEntry {
            query: "\u{202e}\t\n".to_owned(),
            command: Command::ToggleGitHistory,
            uses: 5,
        },
        crate::command_palette_items::CommandPaletteQueryMemoryEntry {
            query: "Git History".to_owned(),
            command: Command::ToggleSourceControl,
            uses: 2,
        },
        crate::command_palette_items::CommandPaletteQueryMemoryEntry {
            query: format!("{} tail", "A".repeat(160)),
            command: Command::ToggleWorkspaceTasks,
            uses: 0,
        },
    ];
    let original_memory = session.command_query_memory.clone();

    let bytes = session_bytes_for_write(&session).unwrap();

    assert_eq!(session.command_query_memory, original_memory);
    let restored: PersistedSession = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored.command_query_memory.len(), 3);
    assert_eq!(
        restored.command_query_memory[0],
        crate::command_palette_items::CommandPaletteQueryMemoryEntry {
            query: "git history".to_owned(),
            command: Command::ToggleGitHistory,
            uses: 9,
        }
    );
    assert_eq!(
        restored.command_query_memory[1],
        crate::command_palette_items::CommandPaletteQueryMemoryEntry {
            query: "git history".to_owned(),
            command: Command::ToggleSourceControl,
            uses: 2,
        }
    );
    assert_eq!(restored.command_query_memory[2].query.chars().count(), 128);
    assert!(
        restored.command_query_memory[2]
            .query
            .chars()
            .all(|ch| ch == 'a')
    );
    assert_eq!(
        restored.command_query_memory[2].command,
        Command::ToggleWorkspaceTasks
    );
    assert_eq!(restored.command_query_memory[2].uses, 1);

    remove_workspace(&workspace);
}

#[test]
fn session_serializes_source_control_ui_state() {
    let session: PersistedSession = serde_json::from_str(
        r#"{
                "workspace_root": "workspace",
                "open_files": [],
                "active_path": null,
                "pane_paths": [],
                "pane_weights": [],
                "source_control_open": true,
                "source_control_placement": "dockedLeft",
                "source_control_width": 376.0,
                "source_control_query": "modified src",
                "source_control_view": "tree",
                "source_control_sort": "status",
                "source_control_commit_message": "keep draft",
                "source_control_commit_history": [
                    "older commit",
                    "newer commit"
                ],
                "source_control_stash_message": "stash draft",
                "source_control_stashes_open": true,
                "source_control_history_open": true,
                "source_control_history_query": "fix author",
                "source_control_unstaged_collapsed": true,
                "source_control_untracked_collapsed": true,
                "source_control_staged_collapsed": true,
                "recent_projects": [],
                "recovery": []
            }"#,
    )
    .unwrap();

    assert!(session.source_control_open);
    assert_eq!(session.source_control_placement, PanelPlacement::DockedLeft);
    assert_eq!(session.source_control_width, 376.0);
    assert_eq!(session.source_control_query, "modified src");
    assert_eq!(
        session.source_control_view,
        PersistedSourceControlViewMode::Tree
    );
    assert_eq!(
        session.source_control_sort,
        PersistedSourceControlSortMode::Status
    );
    assert_eq!(session.source_control_commit_message, "keep draft");
    assert_eq!(
        session.source_control_commit_history,
        vec!["older commit".to_owned(), "newer commit".to_owned()]
    );
    assert_eq!(session.source_control_stash_message, "stash draft");
    assert!(session.source_control_stashes_open);
    assert!(session.source_control_history_open);
    assert_eq!(session.source_control_history_query, "fix author");
    assert!(session.source_control_unstaged_collapsed);
    assert!(session.source_control_untracked_collapsed);
    assert!(session.source_control_staged_collapsed);

    let encoded = serde_json::to_value(&session).unwrap();
    assert_eq!(encoded["source_control_placement"], "dockedLeft");
    assert_eq!(encoded["source_control_view"], "tree");
    assert_eq!(encoded["source_control_sort"], "status");
    assert_eq!(encoded["source_control_commit_history"][0], "older commit");
    assert_eq!(encoded["source_control_stash_message"], "stash draft");
    assert_eq!(encoded["source_control_stashes_open"], true);
    assert_eq!(encoded["source_control_history_open"], true);
    assert_eq!(encoded["source_control_history_query"], "fix author");
}

#[test]
fn session_write_sanitizes_non_finite_split_weights() {
    let workspace = temp_workspace("non-finite-split-weights");
    let mut session = sample_session(&workspace, "weights recovery");
    session.pane_weights = vec![0.0, f32::NAN, -1.0];
    session.terminal_split_weights = vec![f32::INFINITY, 3.0];

    let bytes = session_bytes_for_write(&session).unwrap();
    let restored: PersistedSession = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(restored.pane_weights, vec![0.5, 0.5]);
    assert!(
        restored
            .terminal_split_weights
            .iter()
            .all(|weight| weight.is_finite() && *weight > 0.0)
    );
    assert_eq!(restored.terminal_split_weights, vec![1.0]);
}

#[test]
fn save_session_trims_terminal_scrollback_before_writing_unloadable_session() {
    let workspace = temp_workspace("trim-terminal");
    fs::create_dir_all(&workspace).unwrap();

    let mut session = sample_session(&workspace, "small recovery");
    session.terminal_sessions = (0..4)
        .map(|index| PersistedTerminalSession {
            cwd: Some(workspace.join(format!("term-{index}"))),
            scrollback: "terminal output\n".repeat(220 * 1024),
            scrollback_offset: index,
            custom_title: None,
            process_label: Some(format!("shell {index}")),
            process_status: None,
            window_title: Some(format!("terminal {index}")),
        })
        .collect();

    save_session(&workspace, &session).unwrap();

    let session_path = state_dir(&workspace).join("session.json");
    assert!(fs::metadata(session_path).unwrap().len() <= PERSISTED_SESSION_MAX_BYTES);
    let restored = PersistedSession::load(&workspace).unwrap().unwrap();
    assert_eq!(restored.recovery[0].text, "small recovery");
    assert_eq!(restored.terminal_sessions.len(), 4);
    assert!(
        restored
            .terminal_sessions
            .iter()
            .all(|session| session.scrollback.is_empty())
    );
    assert_eq!(
        restored.terminal_sessions[3].window_title.as_deref(),
        Some("terminal 3")
    );

    remove_workspace(&workspace);
}

#[test]
fn load_session_reads_workspace_local_legacy_state_dir() {
    let workspace = temp_workspace("legacy-workspace-session");
    fs::create_dir_all(&workspace).unwrap();
    let legacy_state = crate::persistence_storage::legacy_state_dir(&workspace);
    fs::create_dir_all(&legacy_state).unwrap();
    let session = sample_session(&workspace, "legacy recovery");
    fs::write(
        legacy_state.join("session.json"),
        serde_json::to_string_pretty(&session).unwrap(),
    )
    .unwrap();

    let loaded = PersistedSession::load(&workspace).unwrap();

    assert_eq!(loaded, Some(session));
    assert!(!state_dir(&workspace).join("session.json").exists());

    remove_workspace(&workspace);
}

#[test]
fn save_session_records_skipped_recovery_instead_of_writing_unloadable_session() {
    let workspace = temp_workspace("trim-recovery");
    fs::create_dir_all(&workspace).unwrap();

    let mut session = sample_session(&workspace, "small recovery");
    session.history_states = vec![BufferHistoryState {
        path: workspace.join("src/main.rs"),
        history: kuroya_core::BufferHistorySnapshot {
            len_chars: 3,
            checksum: 42,
            undo: Vec::new(),
            redo: Vec::new(),
        },
    }];
    session.recovery = vec![RecoveredBuffer {
        path: Some(workspace.join("src/large.rs")),
        display_name: "large.rs".to_owned(),
        text: "x".repeat(usize::try_from(PERSISTED_SESSION_MAX_BYTES).unwrap() + 1024),
    }];

    save_session(&workspace, &session).unwrap();

    let session_path = state_dir(&workspace).join("session.json");
    assert!(fs::metadata(session_path).unwrap().len() <= PERSISTED_SESSION_MAX_BYTES);
    let restored = PersistedSession::load(&workspace).unwrap().unwrap();
    assert!(restored.recovery.is_empty());
    assert_eq!(restored.history_states.len(), 1);
    assert_eq!(
        restored.history_states[0].path,
        workspace.join("src/main.rs")
    );
    assert_eq!(restored.recovery_skipped.len(), 1);
    assert_eq!(restored.recovery_skipped[0].display_name, "large.rs");
    assert!(
        restored.recovery_skipped[0]
            .reason
            .contains("omitted to keep session file under")
    );

    remove_workspace(&workspace);
}

#[test]
fn save_session_trims_volatile_text_before_writing_unloadable_session() {
    let workspace = temp_workspace("trim-volatile-text");
    fs::create_dir_all(&workspace).unwrap();

    let previous = sample_session(&workspace, "previous recovery");
    save_session(&workspace, &previous).unwrap();

    let mut session = sample_session(&workspace, "new recovery");
    session.project_search_query =
        "x".repeat(usize::try_from(PERSISTED_SESSION_MAX_BYTES).unwrap() + 1024);

    save_session(&workspace, &session).unwrap();

    let session_path = state_dir(&workspace).join("session.json");
    assert!(fs::metadata(session_path).unwrap().len() <= PERSISTED_SESSION_MAX_BYTES);
    let restored = PersistedSession::load(&workspace).unwrap().unwrap();
    assert_eq!(restored.recovery[0].text, "new recovery");
    assert_eq!(
        restored.project_search_query.chars().count(),
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS
    );
    assert!(restored.project_search_query.chars().all(|ch| ch == 'x'));
    assert!(restored.project_search_recent.is_empty());
    assert!(restored.buffer_find_query_history.is_empty());
    assert!(restored.buffer_find_replacement_history.is_empty());
    assert!(restored.source_control_commit_history.is_empty());
    let backups = session_snapshot_files_for_test(&workspace);
    assert_eq!(backups.len(), 1);
    assert_eq!(
        serde_json::from_str::<PersistedSession>(&fs::read_to_string(&backups[0]).unwrap())
            .unwrap(),
        previous
    );

    remove_workspace(&workspace);
}

#[test]
fn save_session_leaves_previous_file_when_session_cannot_be_trimmed_to_limit() {
    let workspace = temp_workspace("untrimmed-oversized");
    fs::create_dir_all(&workspace).unwrap();

    let previous = sample_session(&workspace, "previous recovery");
    save_session(&workspace, &previous).unwrap();

    let mut oversized = sample_session(&workspace, "new recovery");
    oversized.open_files = vec![PathBuf::from(
        "x".repeat(usize::try_from(PERSISTED_SESSION_MAX_BYTES).unwrap() + 1024),
    )];

    assert!(save_session(&workspace, &oversized).is_err());
    assert_eq!(PersistedSession::load(&workspace).unwrap(), Some(previous));
    assert!(session_snapshot_files_for_test(&workspace).is_empty());

    remove_workspace(&workspace);
}

#[test]
fn load_session_accepts_lexically_equivalent_workspace_root() {
    let workspace = temp_workspace("equivalent-root");
    let state = state_dir(&workspace);
    fs::create_dir_all(&state).unwrap();

    let mut session = sample_session(&workspace, "equivalent recovery");
    session.workspace_root = workspace.join("src").join("..");
    fs::write(
        state.join("session.json"),
        serde_json::to_string_pretty(&session).unwrap(),
    )
    .unwrap();

    assert_eq!(PersistedSession::load(&workspace).unwrap(), Some(session));
    assert!(quarantined_session_files_with_marker(&state, "mismatched").is_empty());

    remove_workspace(&workspace);
}

#[test]
fn load_session_normalizes_restored_workspace_paths_and_drops_escaped_paths() {
    let workspace = temp_workspace("normalize-restored-paths");
    let state = state_dir(&workspace);
    fs::create_dir_all(&state).unwrap();

    let messy_main = workspace.join("src").join("..").join("src").join("main.rs");
    let main = workspace.join("src").join("main.rs");
    let relative_lib = PathBuf::from("src").join("lib.rs");
    let lib = workspace.join("src").join("lib.rs");
    let messy_src = workspace.join(".").join("src").join(".");
    let src = workspace.join("src");
    let outside = workspace.join("..").join("outside.rs");

    let mut session = sample_session(&workspace, "inside recovery");
    session.open_files = vec![
        messy_main.clone(),
        main.clone(),
        outside.clone(),
        relative_lib,
    ];
    session.active_path = Some(messy_main.clone());
    session.pane_paths = vec![Some(messy_main.clone()), Some(outside.clone())];
    session.view_states = vec![
        BufferViewState {
            path: messy_main.clone(),
            cursor_line: 1,
            cursor_column: 1,
            scroll_line: 1,
            horizontal_scroll_offset: 0.0,
            selections: Vec::new(),
        },
        BufferViewState {
            path: outside.clone(),
            cursor_line: 1,
            cursor_column: 1,
            scroll_line: 1,
            horizontal_scroll_offset: 0.0,
            selections: Vec::new(),
        },
    ];
    session.pane_view_states = vec![
        PaneBufferViewState {
            pane_index: 0,
            path: messy_main.clone(),
            scroll_line: 1,
            horizontal_scroll_offset: 0.0,
        },
        PaneBufferViewState {
            pane_index: 1,
            path: outside.clone(),
            scroll_line: 1,
            horizontal_scroll_offset: 0.0,
        },
    ];
    session.history_states = vec![
        BufferHistoryState {
            path: messy_main.clone(),
            history: kuroya_core::BufferHistorySnapshot {
                len_chars: 0,
                checksum: 0,
                undo: Vec::new(),
                redo: Vec::new(),
            },
        },
        BufferHistoryState {
            path: outside.clone(),
            history: kuroya_core::BufferHistorySnapshot {
                len_chars: 0,
                checksum: 0,
                undo: Vec::new(),
                redo: Vec::new(),
            },
        },
    ];
    session.fold_states = vec![
        BufferFoldState {
            path: messy_main.clone(),
            ranges: vec![PersistedFoldRange {
                start_line: 1,
                end_line: 3,
            }],
        },
        BufferFoldState {
            path: outside.clone(),
            ranges: Vec::new(),
        },
    ];
    session.explorer_expanded = vec![workspace.clone(), messy_src.clone(), outside.clone()];
    session.explorer_revealed_path = Some(messy_main.clone());
    session.quick_open_recent_files = vec![messy_main.clone(), main.clone(), outside.clone()];
    session.quick_open_query_memory = vec![
        crate::quick_open::QuickOpenQueryMemoryEntry {
            query: "main".to_owned(),
            path: messy_main.clone(),
            uses: 2,
        },
        crate::quick_open::QuickOpenQueryMemoryEntry {
            query: "outside".to_owned(),
            path: outside.clone(),
            uses: 1,
        },
    ];
    session.workspace_symbol_query_memory = vec![
        crate::lsp_workspace_symbol_ranking::WorkspaceSymbolQueryMemoryEntry {
            query: "main".to_owned(),
            path: messy_main.clone(),
            name: "main".to_owned(),
            kind: 12,
            line: 1,
            column: 1,
            uses: 3,
        },
        crate::lsp_workspace_symbol_ranking::WorkspaceSymbolQueryMemoryEntry {
            query: "outside".to_owned(),
            path: outside.clone(),
            name: "outside".to_owned(),
            kind: 12,
            line: 1,
            column: 1,
            uses: 1,
        },
    ];
    session.navigation_back = vec![
        PersistedNavigationLocation {
            path: messy_main.clone(),
            line: 4,
            column: 2,
        },
        PersistedNavigationLocation {
            path: outside.clone(),
            line: 1,
            column: 1,
        },
    ];
    session.navigation_forward = vec![PersistedNavigationLocation {
        path: messy_main.clone(),
        line: 8,
        column: 1,
    }];
    session.closed_files = vec![
        PersistedClosedFileEntry {
            path: messy_main.clone(),
            line: 2,
            column: 5,
        },
        PersistedClosedFileEntry {
            path: outside.clone(),
            line: 1,
            column: 1,
        },
    ];
    session.recovery = vec![
        RecoveredBuffer {
            path: Some(messy_main.clone()),
            display_name: "main.rs".to_owned(),
            text: "inside recovery".to_owned(),
        },
        RecoveredBuffer {
            path: Some(outside),
            display_name: "outside.rs".to_owned(),
            text: "outside recovery text stays recoverable".to_owned(),
        },
    ];

    fs::write(
        state.join("session.json"),
        serde_json::to_string_pretty(&session).unwrap(),
    )
    .unwrap();

    let loaded = PersistedSession::load(&workspace).unwrap().unwrap();

    assert_eq!(loaded.open_files, vec![main.clone(), lib]);
    assert_eq!(loaded.active_path, Some(main.clone()));
    assert_eq!(loaded.pane_paths, vec![Some(main.clone()), None]);
    assert_eq!(loaded.view_states.len(), 1);
    assert_eq!(loaded.view_states[0].path, main);
    assert_eq!(loaded.pane_view_states.len(), 1);
    assert_eq!(loaded.pane_view_states[0].path, loaded.view_states[0].path);
    assert_eq!(loaded.history_states.len(), 1);
    assert_eq!(loaded.history_states[0].path, loaded.view_states[0].path);
    assert_eq!(loaded.fold_states.len(), 1);
    assert_eq!(loaded.fold_states[0].path, loaded.view_states[0].path);
    assert_eq!(loaded.explorer_expanded, vec![src]);
    assert_eq!(
        loaded.explorer_revealed_path,
        Some(loaded.view_states[0].path.clone())
    );
    assert_eq!(
        loaded.quick_open_recent_files,
        vec![loaded.view_states[0].path.clone()]
    );
    assert_eq!(loaded.quick_open_query_memory.len(), 1);
    assert_eq!(
        loaded.quick_open_query_memory[0].path,
        loaded.view_states[0].path
    );
    assert_eq!(loaded.workspace_symbol_query_memory.len(), 1);
    assert_eq!(
        loaded.workspace_symbol_query_memory[0].path,
        loaded.view_states[0].path
    );
    assert_eq!(loaded.navigation_back.len(), 1);
    assert_eq!(loaded.navigation_back[0].path, loaded.view_states[0].path);
    assert_eq!(loaded.navigation_forward.len(), 1);
    assert_eq!(
        loaded.navigation_forward[0].path,
        loaded.view_states[0].path
    );
    assert_eq!(loaded.closed_files.len(), 1);
    assert_eq!(loaded.closed_files[0].path, loaded.view_states[0].path);
    assert_eq!(
        loaded.recovery[0].path,
        Some(loaded.view_states[0].path.clone())
    );
    assert_eq!(loaded.recovery[1].path, None);
    assert_eq!(
        loaded.recovery[1].text,
        "outside recovery text stays recoverable"
    );

    remove_workspace(&workspace);
}

#[test]
fn load_session_prunes_stale_pane_only_saved_state() {
    let workspace = temp_workspace("prune-stale-pane-only-state");
    let state = state_dir(&workspace);
    fs::create_dir_all(&state).unwrap();

    let main = workspace.join("src/main.rs");
    let recovered = workspace.join("src/recovered.rs");
    let stale = workspace.join("src/stale.rs");
    let mut session = sample_session(&workspace, "path backed recovery");
    session.open_files = vec![main.clone()];
    session.active_path = Some(stale.clone());
    session.pane_paths = vec![
        Some(stale.clone()),
        Some(recovered.clone()),
        Some(main.clone()),
    ];
    session.view_states = vec![
        BufferViewState {
            path: stale.clone(),
            cursor_line: 1,
            cursor_column: 1,
            scroll_line: 1,
            horizontal_scroll_offset: 0.0,
            selections: Vec::new(),
        },
        BufferViewState {
            path: main.clone(),
            cursor_line: 2,
            cursor_column: 1,
            scroll_line: 2,
            horizontal_scroll_offset: 0.0,
            selections: Vec::new(),
        },
        BufferViewState {
            path: recovered.clone(),
            cursor_line: 3,
            cursor_column: 1,
            scroll_line: 3,
            horizontal_scroll_offset: 0.0,
            selections: Vec::new(),
        },
    ];
    session.pane_view_states = vec![
        PaneBufferViewState {
            pane_index: 0,
            path: stale.clone(),
            scroll_line: 1,
            horizontal_scroll_offset: 0.0,
        },
        PaneBufferViewState {
            pane_index: 1,
            path: recovered.clone(),
            scroll_line: 3,
            horizontal_scroll_offset: 0.0,
        },
        PaneBufferViewState {
            pane_index: 99,
            path: main.clone(),
            scroll_line: 2,
            horizontal_scroll_offset: 0.0,
        },
    ];
    session.history_states = vec![
        BufferHistoryState {
            path: stale.clone(),
            history: kuroya_core::BufferHistorySnapshot {
                len_chars: 0,
                checksum: 0,
                undo: Vec::new(),
                redo: Vec::new(),
            },
        },
        BufferHistoryState {
            path: recovered.clone(),
            history: kuroya_core::BufferHistorySnapshot {
                len_chars: 0,
                checksum: 0,
                undo: Vec::new(),
                redo: Vec::new(),
            },
        },
    ];
    session.fold_states = vec![
        BufferFoldState {
            path: stale,
            ranges: vec![PersistedFoldRange {
                start_line: 1,
                end_line: 3,
            }],
        },
        BufferFoldState {
            path: main.clone(),
            ranges: vec![
                PersistedFoldRange {
                    start_line: 1,
                    end_line: 3,
                },
                PersistedFoldRange {
                    start_line: 4,
                    end_line: 4,
                },
            ],
        },
    ];
    session.recovery = vec![RecoveredBuffer {
        path: Some(recovered.clone()),
        display_name: "recovered.rs".to_owned(),
        text: "path backed recovery".to_owned(),
    }];

    fs::write(
        state.join("session.json"),
        serde_json::to_string_pretty(&session).unwrap(),
    )
    .unwrap();

    let loaded = PersistedSession::load(&workspace).unwrap().unwrap();

    assert_eq!(loaded.active_path, None);
    assert_eq!(
        loaded.pane_paths,
        vec![None, Some(recovered.clone()), Some(main.clone())]
    );
    assert_eq!(
        loaded
            .view_states
            .iter()
            .map(|state| state.path.clone())
            .collect::<Vec<_>>(),
        vec![main.clone(), recovered.clone()]
    );
    assert_eq!(loaded.pane_view_states.len(), 1);
    assert_eq!(loaded.pane_view_states[0].path, recovered);
    assert_eq!(loaded.history_states.len(), 1);
    assert_eq!(
        loaded.history_states[0].path,
        loaded.recovery[0].path.clone().unwrap()
    );
    assert_eq!(loaded.fold_states.len(), 1);
    assert_eq!(loaded.fold_states[0].path, main);
    assert_eq!(loaded.fold_states[0].ranges.len(), 1);

    remove_workspace(&workspace);
}

#[test]
fn load_session_rejects_raw_parent_reentry_paths_that_normalize_inside_workspace() {
    let workspace = temp_workspace("raw-reentry-rejected");
    let state = state_dir(&workspace);
    fs::create_dir_all(&state).unwrap();
    let raw_reentry = workspace
        .join("..")
        .join(workspace.file_name().unwrap())
        .join("src/secret.rs");
    let mut session = sample_session(&workspace, "raw reentry recovery");
    session.open_files = vec![raw_reentry.clone()];
    session.active_path = Some(raw_reentry.clone());
    session.pane_paths = vec![Some(raw_reentry.clone())];
    session.recovery = vec![RecoveredBuffer {
        path: Some(raw_reentry),
        display_name: "secret.rs".to_owned(),
        text: "raw reentry recovery".to_owned(),
    }];

    fs::write(
        state.join("session.json"),
        serde_json::to_string_pretty(&session).unwrap(),
    )
    .unwrap();

    let loaded = PersistedSession::load(&workspace).unwrap().unwrap();

    assert!(loaded.open_files.is_empty());
    assert_eq!(loaded.active_path, None);
    assert_eq!(loaded.pane_paths, vec![None]);
    assert_eq!(loaded.recovery[0].path, None);
    assert_eq!(loaded.recovery[0].text, "raw reentry recovery");

    remove_workspace(&workspace);
}

fn quarantined_session_files(dir: &Path) -> Vec<PathBuf> {
    quarantined_session_files_with_marker(dir, "corrupt")
}

fn quarantined_session_files_with_marker(dir: &Path, marker: &str) -> Vec<PathBuf> {
    let marker = format!(".{marker}.");
    fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(&marker))
        })
        .collect()
}

fn session_snapshot_files_for_test(workspace: &Path) -> Vec<PathBuf> {
    let dir = crate::persistence_storage::session_snapshots_dir(workspace);
    if !dir.exists() {
        return Vec::new();
    }

    let mut files = fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("session.") && name.ends_with(".json"))
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}
