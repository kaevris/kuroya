use super::*;

#[test]
fn workspace_reset_invalidates_source_control_load_request_ids_monotonically() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root);
    let path = PathBuf::from("workspace").join("src").join("main.rs");
    app.source_control_branch_next_request_id = 1;
    app.source_control_branch_active_request_id = 1;
    app.source_control_branch_in_flight_request_id = Some(1);
    app.source_control_branch_reload_queued = true;
    app.source_control_history_next_request_id = 1;
    app.source_control_history_active_request_id = 1;
    app.source_control_history_in_flight_request_id = Some(1);
    app.source_control_history_reload_queued = true;
    app.source_control_stashes_next_request_id = 1;
    app.source_control_stashes_active_request_id = 1;
    app.source_control_stashes_in_flight_request_id = Some(1);
    app.source_control_stashes_reload_queued = true;
    app.source_control_hunks_next_request_id = 1;
    app.source_control_hunks_active_request_id = 1;
    app.source_control_hunks_in_flight_request_id = Some(1);
    app.source_control_hunks_reload_queued = true;
    app.source_control_blame_next_request_id = 1;
    app.source_control_blame_active_request_id = 1;
    app.source_control_blame_active_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_in_flight_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_reload_queued_paths
        .insert(path.clone());
    app.source_control_blame_open_view_paths.insert(path);

    app.reset_open_workspace_state();

    assert_eq!(app.source_control_branch_next_request_id, 2);
    assert_eq!(app.source_control_branch_active_request_id, 2);
    assert_eq!(app.source_control_branch_in_flight_request_id, None);
    assert!(!app.source_control_branch_reload_queued);
    assert_eq!(app.source_control_history_next_request_id, 2);
    assert_eq!(app.source_control_history_active_request_id, 2);
    assert_eq!(app.source_control_history_in_flight_request_id, None);
    assert!(!app.source_control_history_reload_queued);
    assert_eq!(app.source_control_stashes_next_request_id, 2);
    assert_eq!(app.source_control_stashes_active_request_id, 2);
    assert_eq!(app.source_control_stashes_in_flight_request_id, None);
    assert!(!app.source_control_stashes_reload_queued);
    assert_eq!(app.source_control_hunks_next_request_id, 2);
    assert_eq!(app.source_control_hunks_active_request_id, 2);
    assert_eq!(app.source_control_hunks_in_flight_request_id, None);
    assert!(!app.source_control_hunks_reload_queued);
    assert_eq!(app.source_control_blame_next_request_id, 2);
    assert_eq!(app.source_control_blame_active_request_id, 2);
    assert!(app.source_control_blame_active_request_ids.is_empty());
    assert!(app.source_control_blame_in_flight_request_ids.is_empty());
    assert!(app.source_control_blame_reload_queued_paths.is_empty());
    assert!(app.source_control_blame_open_view_paths.is_empty());
}

#[test]
fn current_git_history_loaded_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_history_open = true;
    app.source_control_history_loading = true;
    app.source_control_history_next_request_id = 1;
    app.source_control_history_active_request_id = 1;
    app.source_control_history_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHistoryLoaded {
            request_id: 1,
            limit: 25,
            root: root.clone(),
            operation_root: root,
            commits: vec![git_commit_for_test("current")],
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.source_control_history_in_flight_request_id, None);
    assert!(!app.source_control_history_reload_queued);
    assert!(!app.source_control_history_loading);
    assert_eq!(app.source_control_history_active_request_id, 1);
    assert_eq!(app.source_control_history_requested_limit, 25);
    assert_eq!(app.source_control_history.len(), 1);
    assert_eq!(app.source_control_history[0].summary, "current");
    assert_eq!(app.status, "Loaded 1 commit");
}

#[test]
fn equivalent_root_git_history_loaded_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let mut app = app_for_test(root);
    app.source_control_history_open = true;
    app.source_control_history_loading = true;
    app.source_control_history_next_request_id = 1;
    app.source_control_history_active_request_id = 1;
    app.source_control_history_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHistoryLoaded {
            request_id: 1,
            limit: 25,
            root: event_root.clone(),
            operation_root: event_root,
            commits: vec![git_commit_for_test("current")],
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.source_control_history_in_flight_request_id, None);
    assert!(!app.source_control_history_reload_queued);
    assert!(!app.source_control_history_loading);
    assert_eq!(app.source_control_history_active_request_id, 1);
    assert_eq!(app.source_control_history_requested_limit, 25);
    assert_eq!(app.source_control_history.len(), 1);
    assert_eq!(app.source_control_history[0].summary, "current");
    assert_eq!(app.status, "Loaded 1 commit");
}

#[test]
fn current_git_history_failed_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_history_open = true;
    app.source_control_history_loading = true;
    app.source_control_history = vec![git_commit_for_test("current")];
    app.source_control_history_next_request_id = 1;
    app.source_control_history_active_request_id = 1;
    app.source_control_history_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHistoryFailed {
            request_id: 1,
            limit: 25,
            root: root.clone(),
            operation_root: root,
            error: "history load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.source_control_history_in_flight_request_id, None);
    assert!(!app.source_control_history_reload_queued);
    assert!(!app.source_control_history_loading);
    assert_eq!(app.source_control_history_active_request_id, 1);
    assert_eq!(app.source_control_history_requested_limit, 25);
    assert!(app.source_control_history.is_empty());
    assert_eq!(
        app.status,
        "Could not load git history: history load failed"
    );
}

#[test]
fn equivalent_root_git_history_failed_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let mut app = app_for_test(root);
    app.source_control_history_open = true;
    app.source_control_history_loading = true;
    app.source_control_history = vec![git_commit_for_test("current")];
    app.source_control_history_next_request_id = 1;
    app.source_control_history_active_request_id = 1;
    app.source_control_history_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHistoryFailed {
            request_id: 1,
            limit: 25,
            root: event_root.clone(),
            operation_root: event_root,
            error: "history load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.source_control_history_in_flight_request_id, None);
    assert!(!app.source_control_history_reload_queued);
    assert!(!app.source_control_history_loading);
    assert_eq!(app.source_control_history_active_request_id, 1);
    assert_eq!(app.source_control_history_requested_limit, 25);
    assert!(app.source_control_history.is_empty());
    assert_eq!(
        app.status,
        "Could not load git history: history load failed"
    );
}

#[test]
fn stale_git_history_loaded_event_from_other_workspace_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root);
    app.source_control_history_open = true;
    app.source_control_history_loading = true;
    app.source_control_history_next_request_id = 2;
    app.source_control_history_active_request_id = 2;
    app.source_control_history_in_flight_request_id = Some(1);
    app.source_control_history_reload_queued = true;
    app.source_control_history = vec![git_commit_for_test("current")];

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHistoryLoaded {
            request_id: 1,
            limit: 25,
            root: PathBuf::from("old-workspace"),
            operation_root: PathBuf::from("old-workspace"),
            commits: vec![git_commit_for_test("stale")],
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.source_control_history_in_flight_request_id, Some(1));
    assert!(app.source_control_history_reload_queued);
    assert_eq!(app.source_control_history_active_request_id, 2);
    assert_eq!(app.source_control_history[0].summary, "current");
}

#[test]
fn stale_git_history_failed_event_from_other_workspace_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root);
    app.source_control_history_open = true;
    app.source_control_history_loading = true;
    app.source_control_history_next_request_id = 2;
    app.source_control_history_active_request_id = 2;
    app.source_control_history_in_flight_request_id = Some(1);
    app.source_control_history_reload_queued = true;
    app.source_control_history = vec![git_commit_for_test("current")];
    app.status = "current history load".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHistoryFailed {
            request_id: 1,
            limit: 25,
            root: PathBuf::from("old-workspace"),
            operation_root: PathBuf::from("old-workspace"),
            error: "stale history load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.source_control_history_in_flight_request_id, Some(1));
    assert!(app.source_control_history_reload_queued);
    assert_eq!(app.source_control_history_active_request_id, 2);
    assert_eq!(app.source_control_history[0].summary, "current");
    assert_eq!(app.status, "current history load");
}

#[test]
fn current_root_stale_git_history_loaded_event_drains_queued_reload_without_applying_result() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_history_open = true;
    app.source_control_history_loading = true;
    app.source_control_history_next_request_id = 2;
    app.source_control_history_active_request_id = 2;
    app.source_control_history_in_flight_request_id = Some(1);
    app.source_control_history_reload_queued = true;
    app.source_control_history_requested_limit = 50;
    app.source_control_history = vec![git_commit_for_test("current")];

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHistoryLoaded {
            request_id: 1,
            limit: 25,
            root: root.clone(),
            operation_root: root,
            commits: vec![git_commit_for_test("stale")],
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.source_control_history_active_request_id, 3);
    assert_eq!(app.source_control_history_in_flight_request_id, Some(3));
    assert!(!app.source_control_history_reload_queued);
    assert!(app.source_control_history_loading);
    assert_eq!(app.source_control_history_requested_limit, 50);
    assert_eq!(app.source_control_history[0].summary, "current");
}

#[test]
fn stale_operation_root_git_history_loaded_event_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let stale_operation_root = root.join("old-repo");
    let mut app = app_for_test(root.clone());
    app.source_control_history_open = true;
    app.source_control_history_loading = true;
    app.source_control_history_next_request_id = 1;
    app.source_control_history_active_request_id = 1;
    app.source_control_history_in_flight_request_id = Some(1);
    app.source_control_history_reload_queued = true;
    app.source_control_history_requested_limit = 50;
    app.source_control_history = vec![git_commit_for_test("current")];

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHistoryLoaded {
            request_id: 1,
            limit: 25,
            root: root.clone(),
            operation_root: stale_operation_root,
            commits: vec![git_commit_for_test("stale")],
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.source_control_history_active_request_id, 1);
    assert_eq!(app.source_control_history_in_flight_request_id, Some(1));
    assert!(app.source_control_history_reload_queued);
    assert!(app.source_control_history_loading);
    assert_eq!(app.source_control_history_requested_limit, 50);
    assert_eq!(app.source_control_history[0].summary, "current");
}

#[test]
fn stale_operation_root_git_history_failed_event_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let stale_operation_root = root.join("old-repo");
    let mut app = app_for_test(root.clone());
    app.source_control_history_open = true;
    app.source_control_history_loading = true;
    app.source_control_history_next_request_id = 1;
    app.source_control_history_active_request_id = 1;
    app.source_control_history_in_flight_request_id = Some(1);
    app.source_control_history_reload_queued = true;
    app.source_control_history_requested_limit = 50;
    app.source_control_history = vec![git_commit_for_test("current")];
    app.status = "current history load".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHistoryFailed {
            request_id: 1,
            limit: 25,
            root: root.clone(),
            operation_root: stale_operation_root,
            error: "stale history load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.source_control_history_active_request_id, 1);
    assert_eq!(app.source_control_history_in_flight_request_id, Some(1));
    assert!(app.source_control_history_reload_queued);
    assert!(app.source_control_history_loading);
    assert_eq!(app.source_control_history_requested_limit, 50);
    assert_eq!(app.source_control_history[0].summary, "current");
    assert_eq!(app.status, "current history load");
}

#[test]
fn equivalent_root_stale_git_history_loaded_event_drains_queued_reload_without_applying_result() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let mut app = app_for_test(root);
    app.source_control_history_open = true;
    app.source_control_history_loading = true;
    app.source_control_history_next_request_id = 2;
    app.source_control_history_active_request_id = 2;
    app.source_control_history_in_flight_request_id = Some(1);
    app.source_control_history_reload_queued = true;
    app.source_control_history_requested_limit = 50;
    app.source_control_history = vec![git_commit_for_test("current")];

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHistoryLoaded {
            request_id: 1,
            limit: 25,
            root: event_root.clone(),
            operation_root: event_root,
            commits: vec![git_commit_for_test("stale")],
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.source_control_history_active_request_id, 3);
    assert_eq!(app.source_control_history_in_flight_request_id, Some(3));
    assert!(!app.source_control_history_reload_queued);
    assert!(app.source_control_history_loading);
    assert_eq!(app.source_control_history_requested_limit, 50);
    assert_eq!(app.source_control_history[0].summary, "current");
}

#[test]
fn current_root_stale_git_history_failed_event_drains_queued_reload_without_applying_result() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_history_open = true;
    app.source_control_history_loading = true;
    app.source_control_history_next_request_id = 2;
    app.source_control_history_active_request_id = 2;
    app.source_control_history_in_flight_request_id = Some(1);
    app.source_control_history_reload_queued = true;
    app.source_control_history_requested_limit = 50;
    app.source_control_history = vec![git_commit_for_test("current")];
    app.status = "current history load".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHistoryFailed {
            request_id: 1,
            limit: 25,
            root: root.clone(),
            operation_root: root,
            error: "stale history load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.source_control_history_active_request_id, 3);
    assert_eq!(app.source_control_history_in_flight_request_id, Some(3));
    assert!(!app.source_control_history_reload_queued);
    assert!(app.source_control_history_loading);
    assert_eq!(app.source_control_history_requested_limit, 50);
    assert_eq!(app.source_control_history[0].summary, "current");
    assert_eq!(app.status, "Loading git history");
}

#[test]
fn equivalent_root_stale_git_history_failed_event_drains_queued_reload_without_applying_result() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let mut app = app_for_test(root);
    app.source_control_history_open = true;
    app.source_control_history_loading = true;
    app.source_control_history_next_request_id = 2;
    app.source_control_history_active_request_id = 2;
    app.source_control_history_in_flight_request_id = Some(1);
    app.source_control_history_reload_queued = true;
    app.source_control_history_requested_limit = 50;
    app.source_control_history = vec![git_commit_for_test("current")];
    app.status = "current history load".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHistoryFailed {
            request_id: 1,
            limit: 25,
            root: event_root.clone(),
            operation_root: event_root,
            error: "stale history load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.source_control_history_active_request_id, 3);
    assert_eq!(app.source_control_history_in_flight_request_id, Some(3));
    assert!(!app.source_control_history_reload_queued);
    assert!(app.source_control_history_loading);
    assert_eq!(app.source_control_history_requested_limit, 50);
    assert_eq!(app.source_control_history[0].summary, "current");
    assert_eq!(app.status, "Loading git history");
}

#[test]
fn current_root_queued_git_history_loaded_event_clears_closed_panel_without_respawning() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_history_open = false;
    app.source_control_history_loading = true;
    app.source_control_history_next_request_id = 2;
    app.source_control_history_active_request_id = 2;
    app.source_control_history_in_flight_request_id = Some(1);
    app.source_control_history_reload_queued = true;
    app.source_control_history = vec![git_commit_for_test("current")];
    app.status = "history closed".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHistoryLoaded {
            request_id: 1,
            limit: 25,
            root: root.clone(),
            operation_root: root,
            commits: vec![git_commit_for_test("stale")],
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.source_control_history_active_request_id, 2);
    assert_eq!(app.source_control_history_in_flight_request_id, None);
    assert!(!app.source_control_history_reload_queued);
    assert!(!app.source_control_history_loading);
    assert_eq!(app.source_control_history[0].summary, "current");
    assert_eq!(app.status, "history closed");
}

#[test]
fn current_root_queued_git_history_failed_event_clears_closed_panel_without_respawning() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_history_open = false;
    app.source_control_history_loading = true;
    app.source_control_history_next_request_id = 2;
    app.source_control_history_active_request_id = 2;
    app.source_control_history_in_flight_request_id = Some(1);
    app.source_control_history_reload_queued = true;
    app.source_control_history = vec![git_commit_for_test("current")];
    app.status = "history closed".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHistoryFailed {
            request_id: 1,
            limit: 25,
            root: root.clone(),
            operation_root: root,
            error: "stale history load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.source_control_history_active_request_id, 2);
    assert_eq!(app.source_control_history_in_flight_request_id, None);
    assert!(!app.source_control_history_reload_queued);
    assert!(!app.source_control_history_loading);
    assert_eq!(app.source_control_history[0].summary, "current");
    assert_eq!(app.status, "history closed");
}
