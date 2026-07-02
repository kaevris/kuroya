use super::*;
use crate::{panel_layout::PanelPlacement, project_search_state::ProjectSearchQuery};
use kuroya_core::Command;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

mod app_state;
mod session_snapshots;
mod workspace_snapshots;

fn temp_workspace(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "kuroya-persistence-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn remove_workspace(workspace: impl AsRef<Path>) {
    match fs::remove_dir_all(workspace.as_ref()) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("failed to remove workspace: {error}"),
    }
}

fn sample_session(workspace: &Path, text: &str) -> PersistedSession {
    PersistedSession {
        workspace_root: workspace.to_path_buf(),
        open_files: vec![workspace.join("src/main.rs")],
        active_path: Some(workspace.join("src/main.rs")),
        pane_paths: vec![Some(workspace.join("src/main.rs")), None],
        pane_weights: vec![0.6, 0.4],
        active_pane_index: Some(0),
        view_states: Vec::new(),
        pane_view_states: Vec::new(),
        history_states: Vec::new(),
        recovery_view_states: Vec::new(),
        recovery_history_states: Vec::new(),
        fold_states: Vec::new(),
        explorer_width: 260.0,
        explorer_expanded: vec![workspace.join("src")],
        explorer_revealed_path: Some(workspace.join("src/main.rs")),
        project_search_open: false,
        project_search_placement: PanelPlacement::DockedRight,
        project_search_width: 330.0,
        project_search_query: "needle".to_owned(),
        project_search_case_sensitive: true,
        project_search_whole_word: true,
        project_search_include: "src/**/*.rs".to_owned(),
        project_search_exclude: "target/**".to_owned(),
        project_search_recent: vec![ProjectSearchQuery {
            query: "needle".to_owned(),
            case_sensitive: true,
            whole_word: true,
            include: "src/**/*.rs".to_owned(),
            exclude: "target/**".to_owned(),
        }],
        buffer_find_open: true,
        buffer_find_query: "needle".to_owned(),
        buffer_find_replacement: "replacement".to_owned(),
        buffer_find_case_sensitive: true,
        buffer_find_whole_word: true,
        buffer_find_regex: false,
        buffer_find_preserve_case: true,
        buffer_find_query_history: vec!["needle".to_owned()],
        buffer_find_replacement_history: vec!["replacement".to_owned()],
        settings_panel_open: true,
        theme_picker_open: true,
        keybindings_open: true,
        symbols_panel_open: false,
        symbols_panel_placement: PanelPlacement::DockedRight,
        symbols_panel_width: 300.0,
        diagnostics_panel_open: false,
        diagnostics_panel_placement: PanelPlacement::DockedRight,
        diagnostics_panel_width: 340.0,
        source_control_open: true,
        source_control_placement: PanelPlacement::DockedRight,
        source_control_width: 360.0,
        source_control_query: "src".to_owned(),
        source_control_view: PersistedSourceControlViewMode::Tree,
        source_control_sort: PersistedSourceControlSortMode::Status,
        source_control_commit_message: "draft commit".to_owned(),
        source_control_commit_history: vec![
            "previous commit".to_owned(),
            "newer commit".to_owned(),
        ],
        source_control_stash_message: "stash draft".to_owned(),
        source_control_stashes_open: true,
        source_control_history_open: true,
        source_control_history_query: "fix author".to_owned(),
        source_control_unstaged_collapsed: true,
        source_control_untracked_collapsed: true,
        source_control_staged_collapsed: false,
        terminal_visible: true,
        terminal_height: 220.0,
        terminal_sessions: vec![PersistedTerminalSession {
            cwd: Some(workspace.to_path_buf()),
            scrollback: String::new(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: None,
            process_status: None,
            window_title: None,
        }],
        terminal_active_session: 0,
        terminal_split_view: false,
        terminal_split_weights: vec![1.0],
        recent_projects: vec![workspace.to_path_buf()],
        quick_open_recent_files: vec![workspace.join("src/main.rs")],
        quick_open_query_memory: Vec::new(),
        workspace_symbol_query_memory: Vec::new(),
        command_recent: vec![Command::ToggleQuickOpen],
        command_query_memory: Vec::new(),
        navigation_back: vec![PersistedNavigationLocation {
            path: workspace.join("src/lib.rs"),
            line: 4,
            column: 2,
        }],
        navigation_forward: vec![PersistedNavigationLocation {
            path: workspace.join("src/main.rs"),
            line: 8,
            column: 1,
        }],
        closed_files: vec![PersistedClosedFileEntry {
            path: workspace.join("src/closed.rs"),
            line: 2,
            column: 5,
        }],
        recovery: vec![RecoveredBuffer {
            path: Some(workspace.join("src/main.rs")),
            display_name: "main.rs".to_owned(),
            text: text.to_owned(),
        }],
        recovery_skipped: Vec::new(),
    }
}

fn assert_no_session_temps(workspace: &Path) {
    let state = state_dir(workspace);
    let temp_count = fs::read_dir(state)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.contains(".tmp."))
        })
        .count();
    assert_eq!(temp_count, 0);
}
