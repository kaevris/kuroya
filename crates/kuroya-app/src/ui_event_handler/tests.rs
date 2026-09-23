use super::*;
use crate::{
    app_startup_context::AppStartupContext,
    explorer::{ExplorerEntryKind, ExplorerOperationResult},
    lsp_ui_events::LspUiEvent,
    terminal::TerminalPane,
};
use kuroya_core::{
    Diagnostic, DiagnosticSeverity, EditorSettings, GitAutoRepositoryDetection, GitBlameLine,
    GitBranch, GitChangeStage, GitCheckoutType, GitCommitSummary, GitDiffHunk, GitSnapshot,
    GitStashEntry, ProjectIndex, TextBuffer, Workspace, WorkspaceTask, WorkspaceTaskKind,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Instant,
};
use tokio::runtime::Runtime;

fn git_branch_for_test(name: &str) -> GitBranch {
    GitBranch {
        name: name.to_owned(),
        is_current: false,
        kind: GitCheckoutType::Local,
        committer_time_seconds: 0,
    }
}

fn git_stash_for_test(index: usize, message: &str) -> GitStashEntry {
    GitStashEntry {
        index,
        short_oid: format!("stash{index}"),
        message: message.to_owned(),
    }
}

fn git_commit_for_test(summary: &str) -> GitCommitSummary {
    GitCommitSummary {
        oid: format!("{summary}-oid"),
        short_oid: format!("{summary}-sha"),
        summary: summary.to_owned(),
        author: "Ada".to_owned(),
        time_seconds: 1,
    }
}

fn git_hunk_for_test(index: usize) -> GitDiffHunk {
    GitDiffHunk {
        index,
        fingerprint: index as u64,
        old_start: 1,
        old_lines: 1,
        new_start: 1,
        new_lines: 1,
        additions: 1,
        deletions: 0,
        header: format!("@@ hunk {index} @@"),
    }
}

fn git_blame_line_for_test(line_number: usize, summary: &str) -> GitBlameLine {
    GitBlameLine {
        line_number,
        short_oid: format!("b{line_number}"),
        author: "Ada".to_owned(),
        author_time_seconds: 1,
        summary: summary.to_owned(),
    }
}

fn seed_queued_blame_final_request(app: &mut KuroyaApp, path: &Path) {
    app.source_control_blame_next_request_id = 3;
    app.source_control_blame_active_request_id = 3;
    app.source_control_blame_active_request_ids
        .insert(path.to_path_buf(), 3);
    app.source_control_blame_in_flight_request_ids
        .insert(path.to_path_buf(), 3);
    app.source_control_blame_open_view_paths
        .insert(path.to_path_buf());
    app.source_control_blame_pending_path = Some(path.to_path_buf());
    app.source_control_blame_load_opens_view = true;
    app.source_control_blame_cache
        .insert(path.to_path_buf(), vec![git_blame_line_for_test(1, "old")]);
}

mod plugin_buffer_text;
mod plugin_open_file;
mod source_control_blame;
mod source_control_history;
mod source_control_hunks;
mod source_control_refs;
mod workspace_events;
mod workspace_plugins_diagnostics;

fn workspace_task() -> WorkspaceTask {
    WorkspaceTask {
        name: "Build".to_owned(),
        command: "cargo".to_owned(),
        args: vec!["build".to_owned()],
        cwd: None,
        env: BTreeMap::new(),
        kind: WorkspaceTaskKind::Build,
        default: true,
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

fn static_diagnostic(path: &std::path::Path) -> Diagnostic {
    Diagnostic {
        path: path.to_path_buf(),
        line: 1,
        column: 1,
        char_range: 0..1,
        severity: DiagnosticSeverity::Warning,
        source: "kuroya-static".to_owned(),
        message: "static warning".to_owned(),
        unused: false,
        deprecated: false,
    }
}
