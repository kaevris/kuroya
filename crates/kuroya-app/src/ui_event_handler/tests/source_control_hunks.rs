use super::*;

#[test]
fn current_git_hunks_loaded_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_hunks_open = true;
    app.source_control_hunk_path = Some(path.clone());
    app.source_control_hunk_stage = GitChangeStage::Unstaged;
    app.source_control_hunks_next_request_id = 1;
    app.source_control_hunks_active_request_id = 1;
    app.source_control_hunks_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHunksLoaded {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            path,
            stage: GitChangeStage::Unstaged,
            hunks: vec![git_hunk_for_test(7)],
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_hunks_in_flight_request_id, None);
    assert!(!app.source_control_hunks_reload_queued);
    assert_eq!(app.source_control_hunks_active_request_id, 1);
    assert_eq!(app.source_control_hunks.len(), 1);
    assert_eq!(app.source_control_hunks[0].index, 7);
}

#[test]
fn equivalent_root_git_hunks_loaded_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root);
    app.source_control_hunks_open = true;
    app.source_control_hunk_path = Some(path.clone());
    app.source_control_hunk_stage = GitChangeStage::Unstaged;
    app.source_control_hunks_next_request_id = 1;
    app.source_control_hunks_active_request_id = 1;
    app.source_control_hunks_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHunksLoaded {
            request_id: 1,
            root: event_root,
            operation_root: PathBuf::from("workspace"),
            path,
            stage: GitChangeStage::Unstaged,
            hunks: vec![git_hunk_for_test(7)],
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_hunks_in_flight_request_id, None);
    assert!(!app.source_control_hunks_reload_queued);
    assert_eq!(app.source_control_hunks_active_request_id, 1);
    assert_eq!(app.source_control_hunks.len(), 1);
    assert_eq!(app.source_control_hunks[0].index, 7);
}

#[test]
fn current_git_hunks_failed_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_hunks_open = true;
    app.source_control_hunk_path = Some(path.clone());
    app.source_control_hunk_stage = GitChangeStage::Unstaged;
    app.source_control_hunks = vec![git_hunk_for_test(7)];
    app.source_control_hunks_next_request_id = 1;
    app.source_control_hunks_active_request_id = 1;
    app.source_control_hunks_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHunksFailed {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            path,
            stage: GitChangeStage::Unstaged,
            error: "hunk load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_hunks_in_flight_request_id, None);
    assert!(!app.source_control_hunks_reload_queued);
    assert_eq!(app.source_control_hunks_active_request_id, 1);
    assert!(app.source_control_hunks.is_empty());
    assert!(app.status.contains("Could not load unstaged hunks"));
    assert!(app.status.contains("hunk load failed"));
}

#[test]
fn equivalent_root_git_hunks_failed_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root);
    app.source_control_hunks_open = true;
    app.source_control_hunk_path = Some(path.clone());
    app.source_control_hunk_stage = GitChangeStage::Unstaged;
    app.source_control_hunks = vec![git_hunk_for_test(7)];
    app.source_control_hunks_next_request_id = 1;
    app.source_control_hunks_active_request_id = 1;
    app.source_control_hunks_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHunksFailed {
            request_id: 1,
            root: event_root,
            operation_root: PathBuf::from("workspace"),
            path,
            stage: GitChangeStage::Unstaged,
            error: "hunk load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_hunks_in_flight_request_id, None);
    assert!(!app.source_control_hunks_reload_queued);
    assert_eq!(app.source_control_hunks_active_request_id, 1);
    assert!(app.source_control_hunks.is_empty());
    assert!(app.status.contains("Could not load unstaged hunks"));
    assert!(app.status.contains("hunk load failed"));
}

#[test]
fn stale_git_hunks_loaded_event_from_other_workspace_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root);
    app.source_control_hunks_open = true;
    app.source_control_hunk_path = Some(path.clone());
    app.source_control_hunk_stage = GitChangeStage::Unstaged;
    app.source_control_hunks_next_request_id = 2;
    app.source_control_hunks_active_request_id = 2;
    app.source_control_hunks_in_flight_request_id = Some(1);
    app.source_control_hunks_reload_queued = true;
    app.source_control_hunks = vec![git_hunk_for_test(7)];

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHunksLoaded {
            request_id: 1,
            root: PathBuf::from("old-workspace"),
            operation_root: PathBuf::from("old-workspace"),
            path,
            stage: GitChangeStage::Unstaged,
            hunks: vec![git_hunk_for_test(8)],
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_hunks_in_flight_request_id, Some(1));
    assert!(app.source_control_hunks_reload_queued);
    assert_eq!(app.source_control_hunks_active_request_id, 2);
    assert_eq!(app.source_control_hunks[0].index, 7);
}

#[test]
fn stale_git_hunks_failed_event_from_other_workspace_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root);
    app.source_control_hunks_open = true;
    app.source_control_hunk_path = Some(path.clone());
    app.source_control_hunk_stage = GitChangeStage::Unstaged;
    app.source_control_hunks_next_request_id = 2;
    app.source_control_hunks_active_request_id = 2;
    app.source_control_hunks_in_flight_request_id = Some(1);
    app.source_control_hunks_reload_queued = true;
    app.source_control_hunks = vec![git_hunk_for_test(7)];
    app.status = "current hunk load".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHunksFailed {
            request_id: 1,
            root: PathBuf::from("old-workspace"),
            operation_root: PathBuf::from("old-workspace"),
            path,
            stage: GitChangeStage::Unstaged,
            error: "stale hunk load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_hunks_in_flight_request_id, Some(1));
    assert!(app.source_control_hunks_reload_queued);
    assert_eq!(app.source_control_hunks_active_request_id, 2);
    assert_eq!(app.source_control_hunks[0].index, 7);
    assert_eq!(app.status, "current hunk load");
}

#[test]
fn stale_operation_root_git_hunks_loaded_event_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let stale_operation_root = root.join("old-repo");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_hunks_open = true;
    app.source_control_hunk_path = Some(path.clone());
    app.source_control_hunk_stage = GitChangeStage::Unstaged;
    app.source_control_hunks_next_request_id = 1;
    app.source_control_hunks_active_request_id = 1;
    app.source_control_hunks_in_flight_request_id = Some(1);
    app.source_control_hunks_reload_queued = true;
    app.source_control_hunks = vec![git_hunk_for_test(7)];
    app.status = "current hunk load".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHunksLoaded {
            request_id: 1,
            root: root.clone(),
            operation_root: stale_operation_root,
            path,
            stage: GitChangeStage::Unstaged,
            hunks: vec![git_hunk_for_test(8)],
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_hunks_in_flight_request_id, Some(1));
    assert!(app.source_control_hunks_reload_queued);
    assert_eq!(app.source_control_hunks_active_request_id, 1);
    assert_eq!(app.source_control_hunks[0].index, 7);
    assert_eq!(app.status, "current hunk load");
}

#[test]
fn stale_operation_root_git_hunks_failed_event_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let stale_operation_root = root.join("old-repo");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_hunks_open = true;
    app.source_control_hunk_path = Some(path.clone());
    app.source_control_hunk_stage = GitChangeStage::Unstaged;
    app.source_control_hunks_next_request_id = 1;
    app.source_control_hunks_active_request_id = 1;
    app.source_control_hunks_in_flight_request_id = Some(1);
    app.source_control_hunks_reload_queued = true;
    app.source_control_hunks = vec![git_hunk_for_test(7)];
    app.status = "current hunk load".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHunksFailed {
            request_id: 1,
            root: root.clone(),
            operation_root: stale_operation_root,
            path,
            stage: GitChangeStage::Unstaged,
            error: "stale hunk load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_hunks_in_flight_request_id, Some(1));
    assert!(app.source_control_hunks_reload_queued);
    assert_eq!(app.source_control_hunks_active_request_id, 1);
    assert_eq!(app.source_control_hunks[0].index, 7);
    assert_eq!(app.status, "current hunk load");
}

#[test]
fn current_root_stale_git_hunks_loaded_event_drains_queued_reload_without_applying_result() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_hunks_open = true;
    app.source_control_hunk_path = Some(path.clone());
    app.source_control_hunk_stage = GitChangeStage::Unstaged;
    app.source_control_hunks_next_request_id = 2;
    app.source_control_hunks_active_request_id = 2;
    app.source_control_hunks_in_flight_request_id = Some(1);
    app.source_control_hunks_reload_queued = true;
    app.source_control_hunks = vec![git_hunk_for_test(7)];

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHunksLoaded {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            path,
            stage: GitChangeStage::Unstaged,
            hunks: vec![git_hunk_for_test(8)],
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.source_control_hunks_active_request_id, 3);
    assert_eq!(app.source_control_hunks_in_flight_request_id, Some(3));
    assert!(!app.source_control_hunks_reload_queued);
    assert_eq!(app.source_control_hunks[0].index, 7);
}

#[test]
fn equivalent_root_stale_git_hunks_loaded_event_drains_queued_reload_without_applying_result() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root);
    app.source_control_hunks_open = true;
    app.source_control_hunk_path = Some(path.clone());
    app.source_control_hunk_stage = GitChangeStage::Unstaged;
    app.source_control_hunks_next_request_id = 2;
    app.source_control_hunks_active_request_id = 2;
    app.source_control_hunks_in_flight_request_id = Some(1);
    app.source_control_hunks_reload_queued = true;
    app.source_control_hunks = vec![git_hunk_for_test(7)];

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHunksLoaded {
            request_id: 1,
            root: event_root,
            operation_root: PathBuf::from("workspace"),
            path,
            stage: GitChangeStage::Unstaged,
            hunks: vec![git_hunk_for_test(8)],
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.source_control_hunks_active_request_id, 3);
    assert_eq!(app.source_control_hunks_in_flight_request_id, Some(3));
    assert!(!app.source_control_hunks_reload_queued);
    assert_eq!(app.source_control_hunks[0].index, 7);
}

#[test]
fn current_root_stale_git_hunks_failed_event_drains_queued_reload_without_applying_result() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_hunks_open = true;
    app.source_control_hunk_path = Some(path.clone());
    app.source_control_hunk_stage = GitChangeStage::Unstaged;
    app.source_control_hunks_next_request_id = 2;
    app.source_control_hunks_active_request_id = 2;
    app.source_control_hunks_in_flight_request_id = Some(1);
    app.source_control_hunks_reload_queued = true;
    app.source_control_hunks = vec![git_hunk_for_test(7)];
    app.status = "current hunk load".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHunksFailed {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            path,
            stage: GitChangeStage::Unstaged,
            error: "stale hunk load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.source_control_hunks_active_request_id, 3);
    assert_eq!(app.source_control_hunks_in_flight_request_id, Some(3));
    assert!(!app.source_control_hunks_reload_queued);
    assert_eq!(app.source_control_hunks[0].index, 7);
    assert!(app.status.contains("Loading unstaged hunks"));
}

#[test]
fn equivalent_root_stale_git_hunks_failed_event_drains_queued_reload_without_applying_result() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root);
    app.source_control_hunks_open = true;
    app.source_control_hunk_path = Some(path.clone());
    app.source_control_hunk_stage = GitChangeStage::Unstaged;
    app.source_control_hunks_next_request_id = 2;
    app.source_control_hunks_active_request_id = 2;
    app.source_control_hunks_in_flight_request_id = Some(1);
    app.source_control_hunks_reload_queued = true;
    app.source_control_hunks = vec![git_hunk_for_test(7)];
    app.status = "current hunk load".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHunksFailed {
            request_id: 1,
            root: event_root,
            operation_root: PathBuf::from("workspace"),
            path,
            stage: GitChangeStage::Unstaged,
            error: "stale hunk load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_hunks_active_request_id, 3);
    assert_eq!(app.source_control_hunks_in_flight_request_id, Some(3));
    assert!(!app.source_control_hunks_reload_queued);
    assert_eq!(app.source_control_hunks[0].index, 7);
    assert!(app.status.contains("Loading unstaged hunks"));
}

#[test]
fn current_root_queued_git_hunks_loaded_event_clears_closed_panel_without_respawning() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_hunks_open = false;
    app.source_control_hunk_path = Some(path.clone());
    app.source_control_hunk_stage = GitChangeStage::Unstaged;
    app.source_control_hunks_next_request_id = 2;
    app.source_control_hunks_active_request_id = 2;
    app.source_control_hunks_in_flight_request_id = Some(1);
    app.source_control_hunks_reload_queued = true;
    app.source_control_hunks = vec![git_hunk_for_test(7)];
    app.status = "hunks closed".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHunksLoaded {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            path,
            stage: GitChangeStage::Unstaged,
            hunks: vec![git_hunk_for_test(8)],
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_hunks_active_request_id, 2);
    assert_eq!(app.source_control_hunks_in_flight_request_id, None);
    assert!(!app.source_control_hunks_reload_queued);
    assert_eq!(app.source_control_hunks[0].index, 7);
    assert_eq!(app.status, "hunks closed");
}

#[test]
fn current_root_queued_git_hunks_failed_event_clears_closed_panel_without_respawning() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_hunks_open = false;
    app.source_control_hunk_path = Some(path.clone());
    app.source_control_hunk_stage = GitChangeStage::Unstaged;
    app.source_control_hunks_next_request_id = 2;
    app.source_control_hunks_active_request_id = 2;
    app.source_control_hunks_in_flight_request_id = Some(1);
    app.source_control_hunks_reload_queued = true;
    app.source_control_hunks = vec![git_hunk_for_test(7)];
    app.status = "hunks closed".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHunksFailed {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            path,
            stage: GitChangeStage::Unstaged,
            error: "stale hunk load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_hunks_active_request_id, 2);
    assert_eq!(app.source_control_hunks_in_flight_request_id, None);
    assert!(!app.source_control_hunks_reload_queued);
    assert_eq!(app.source_control_hunks[0].index, 7);
    assert_eq!(app.status, "hunks closed");
}

#[test]
fn closed_git_hunk_mutation_event_does_not_start_hidden_hunk_load() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_hunks_open = false;
    app.source_control_hunk_path = Some(path.clone());
    app.source_control_hunks_next_request_id = 7;
    app.source_control_hunks_active_request_id = 6;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHunkStaged {
            root,
            path,
            hunk_index: 2,
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_hunks_next_request_id, 7);
    assert_eq!(app.source_control_hunks_active_request_id, 6);
    assert_eq!(app.source_control_hunks_in_flight_request_id, None);
    assert!(!app.source_control_hunks_reload_queued);
}
