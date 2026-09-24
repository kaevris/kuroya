use super::*;

#[test]
fn current_git_branches_loaded_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_branch_picker_open = true;
    app.source_control_branch_next_request_id = 1;
    app.source_control_branch_active_request_id = 1;
    app.source_control_branch_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBranchesLoaded {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            branches: Vec::new(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_branch_in_flight_request_id, None);
    assert!(!app.source_control_branch_reload_queued);
    assert_eq!(app.source_control_branch_active_request_id, 1);
    assert_eq!(app.status, "No local git branches found");
}

#[test]
fn equivalent_root_git_branches_loaded_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let mut app = app_for_test(root.clone());
    app.source_control_branch_picker_open = true;
    app.source_control_branch_next_request_id = 1;
    app.source_control_branch_active_request_id = 1;
    app.source_control_branch_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBranchesLoaded {
            request_id: 1,
            root: event_root,
            operation_root: root,
            branches: vec![git_branch_for_test("main")],
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_branch_in_flight_request_id, None);
    assert!(!app.source_control_branch_reload_queued);
    assert_eq!(app.source_control_branch_active_request_id, 1);
    assert_eq!(
        app.source_control_branches,
        vec![git_branch_for_test("main")]
    );
    assert_eq!(app.status, "Loaded 1 git branch");
}

#[test]
fn stale_operation_root_git_branches_loaded_event_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_branch_picker_open = true;
    app.source_control_branch_next_request_id = 1;
    app.source_control_branch_active_request_id = 1;
    app.source_control_branch_in_flight_request_id = Some(1);
    app.source_control_branch_reload_queued = true;
    app.source_control_branches = vec![git_branch_for_test("current")];
    app.status = "current branch load".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBranchesLoaded {
            request_id: 1,
            root: root.clone(),
            operation_root: root.join("old-repo"),
            branches: vec![git_branch_for_test("stale")],
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_branch_in_flight_request_id, Some(1));
    assert!(app.source_control_branch_reload_queued);
    assert_eq!(app.source_control_branch_active_request_id, 1);
    assert_eq!(
        app.source_control_branches,
        vec![git_branch_for_test("current")]
    );
    assert_eq!(app.status, "current branch load");
}

#[test]
fn current_git_branches_failed_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_branch_picker_open = true;
    app.source_control_branch_next_request_id = 1;
    app.source_control_branch_active_request_id = 1;
    app.source_control_branch_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBranchesFailed {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            error: "branch load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_branch_in_flight_request_id, None);
    assert!(!app.source_control_branch_reload_queued);
    assert_eq!(app.source_control_branch_active_request_id, 1);
    assert_eq!(
        app.status,
        "Could not load git branches: branch load failed"
    );
}

#[test]
fn equivalent_root_git_branches_failed_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let mut app = app_for_test(root.clone());
    app.source_control_branch_picker_open = true;
    app.source_control_branch_next_request_id = 1;
    app.source_control_branch_active_request_id = 1;
    app.source_control_branch_in_flight_request_id = Some(1);
    app.source_control_branches = vec![git_branch_for_test("main")];

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBranchesFailed {
            request_id: 1,
            root: event_root,
            operation_root: root,
            error: "branch load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_branch_in_flight_request_id, None);
    assert!(!app.source_control_branch_reload_queued);
    assert_eq!(app.source_control_branch_active_request_id, 1);
    assert!(app.source_control_branches.is_empty());
    assert_eq!(
        app.status,
        "Could not load git branches: branch load failed"
    );
}

#[test]
fn stale_git_branches_failed_event_from_other_workspace_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root);
    app.source_control_branch_picker_open = true;
    app.source_control_branch_next_request_id = 2;
    app.source_control_branch_active_request_id = 2;
    app.source_control_branch_in_flight_request_id = Some(1);
    app.source_control_branch_reload_queued = true;
    app.source_control_branches = vec![git_branch_for_test("current")];
    app.status = "current branch load".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBranchesFailed {
            request_id: 1,
            root: PathBuf::from("old-workspace"),
            operation_root: PathBuf::from("old-workspace"),
            error: "stale branch load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_branch_in_flight_request_id, Some(1));
    assert!(app.source_control_branch_reload_queued);
    assert_eq!(app.source_control_branch_active_request_id, 2);
    assert_eq!(
        app.source_control_branches,
        vec![git_branch_for_test("current")]
    );
    assert_eq!(app.status, "current branch load");
}

#[test]
fn stale_git_branches_loaded_event_from_other_workspace_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root);
    app.source_control_branch_picker_open = true;
    app.source_control_branch_next_request_id = 2;
    app.source_control_branch_active_request_id = 2;
    app.source_control_branch_in_flight_request_id = Some(1);
    app.source_control_branch_reload_queued = true;
    app.source_control_branches = vec![git_branch_for_test("current")];

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBranchesLoaded {
            request_id: 1,
            root: PathBuf::from("old-workspace"),
            operation_root: PathBuf::from("old-workspace"),
            branches: vec![git_branch_for_test("stale")],
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_branch_in_flight_request_id, Some(1));
    assert!(app.source_control_branch_reload_queued);
    assert_eq!(app.source_control_branch_active_request_id, 2);
    assert_eq!(
        app.source_control_branches,
        vec![git_branch_for_test("current")]
    );
}

#[test]
fn stale_same_root_git_branches_loaded_event_after_reset_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_branch_picker_open = true;
    app.source_control_branch_active_request_id = 2;
    app.source_control_branch_in_flight_request_id = Some(2);
    app.source_control_branches = vec![git_branch_for_test("current")];
    app.status = "current branch load".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBranchesLoaded {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            branches: vec![git_branch_for_test("stale")],
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_branch_in_flight_request_id, Some(2));
    assert_eq!(app.source_control_branch_active_request_id, 2);
    assert_eq!(
        app.source_control_branches,
        vec![git_branch_for_test("current")]
    );
    assert_eq!(app.status, "current branch load");
}

#[test]
fn stale_same_root_git_branches_failed_event_after_reset_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_branch_picker_open = true;
    app.source_control_branch_active_request_id = 2;
    app.source_control_branch_in_flight_request_id = Some(2);
    app.source_control_branches = vec![git_branch_for_test("current")];
    app.status = "current branch load".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBranchesFailed {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            error: "stale branch load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_branch_in_flight_request_id, Some(2));
    assert_eq!(app.source_control_branch_active_request_id, 2);
    assert_eq!(
        app.source_control_branches,
        vec![git_branch_for_test("current")]
    );
    assert_eq!(app.status, "current branch load");
}

#[test]
fn current_root_stale_git_branches_loaded_event_drains_queued_reload_without_applying_result() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_branch_picker_open = true;
    app.source_control_branch_next_request_id = 2;
    app.source_control_branch_active_request_id = 2;
    app.source_control_branch_in_flight_request_id = Some(1);
    app.source_control_branch_reload_queued = true;
    app.source_control_branches = vec![git_branch_for_test("current")];

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBranchesLoaded {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            branches: vec![git_branch_for_test("stale")],
        }
    ));

    let handled = app.handle_single_event();
    assert!(
        handled == 1 || handled == 2,
        "stale event should drain with at most its background reload failure, got {handled}"
    );
    assert_eq!(app.source_control_branch_active_request_id, 3);
    assert_eq!(
        app.source_control_branch_in_flight_request_id,
        if handled == 1 { Some(3) } else { None }
    );
    assert!(!app.source_control_branch_reload_queued);
    if handled == 1 {
        assert_eq!(
            app.source_control_branches,
            vec![git_branch_for_test("current")]
        );
    } else {
        assert!(app.source_control_branches.is_empty());
    }
}

#[test]
fn equivalent_root_stale_git_branches_loaded_event_drains_queued_reload_without_applying_result() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let mut app = app_for_test(root.clone());
    app.source_control_branch_picker_open = true;
    app.source_control_branch_next_request_id = 2;
    app.source_control_branch_active_request_id = 2;
    app.source_control_branch_in_flight_request_id = Some(1);
    app.source_control_branch_reload_queued = true;
    app.source_control_branches = vec![git_branch_for_test("current")];

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBranchesLoaded {
            request_id: 1,
            root: event_root,
            operation_root: root,
            branches: vec![git_branch_for_test("stale")],
        }
    ));

    let handled = app.handle_single_event();
    assert!(
        handled == 1 || handled == 2,
        "stale event should drain with at most its background reload failure, got {handled}"
    );
    assert_eq!(app.source_control_branch_active_request_id, 3);
    assert_eq!(
        app.source_control_branch_in_flight_request_id,
        if handled == 1 { Some(3) } else { None }
    );
    assert!(!app.source_control_branch_reload_queued);
    if handled == 1 {
        assert_eq!(
            app.source_control_branches,
            vec![git_branch_for_test("current")]
        );
    } else {
        assert!(app.source_control_branches.is_empty());
    }
}

#[test]
fn current_root_queued_git_branches_loaded_event_clears_closed_panel_without_respawning() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_branch_picker_open = false;
    app.source_control_branch_next_request_id = 2;
    app.source_control_branch_active_request_id = 2;
    app.source_control_branch_in_flight_request_id = Some(1);
    app.source_control_branch_reload_queued = true;
    app.source_control_branches = vec![git_branch_for_test("current")];
    app.status = "branch picker closed".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBranchesLoaded {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            branches: vec![git_branch_for_test("stale")],
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_branch_active_request_id, 2);
    assert_eq!(app.source_control_branch_in_flight_request_id, None);
    assert!(!app.source_control_branch_reload_queued);
    assert_eq!(
        app.source_control_branches,
        vec![git_branch_for_test("current")]
    );
    assert_eq!(app.status, "branch picker closed");
}

#[test]
fn current_root_stale_git_branches_failed_event_drains_queued_reload_without_applying_result() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_branch_picker_open = true;
    app.source_control_branch_next_request_id = 2;
    app.source_control_branch_active_request_id = 2;
    app.source_control_branch_in_flight_request_id = Some(1);
    app.source_control_branch_reload_queued = true;
    app.source_control_branches = vec![git_branch_for_test("current")];

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBranchesFailed {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            error: "stale branch load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.source_control_branch_active_request_id, 3);
    assert_eq!(app.source_control_branch_in_flight_request_id, Some(3));
    assert!(!app.source_control_branch_reload_queued);
    assert_eq!(
        app.source_control_branches,
        vec![git_branch_for_test("current")]
    );
}

#[test]
fn equivalent_root_stale_git_branches_failed_event_drains_queued_reload_without_applying_result() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let mut app = app_for_test(root.clone());
    app.source_control_branch_picker_open = true;
    app.source_control_branch_next_request_id = 2;
    app.source_control_branch_active_request_id = 2;
    app.source_control_branch_in_flight_request_id = Some(1);
    app.source_control_branch_reload_queued = true;
    app.source_control_branches = vec![git_branch_for_test("current")];

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBranchesFailed {
            request_id: 1,
            root: event_root,
            operation_root: root,
            error: "stale branch load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.source_control_branch_active_request_id, 3);
    assert_eq!(app.source_control_branch_in_flight_request_id, Some(3));
    assert!(!app.source_control_branch_reload_queued);
    assert_eq!(
        app.source_control_branches,
        vec![git_branch_for_test("current")]
    );
}

#[test]
fn current_root_queued_git_branches_failed_event_clears_closed_panel_without_respawning() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_branch_picker_open = false;
    app.source_control_branch_next_request_id = 2;
    app.source_control_branch_active_request_id = 2;
    app.source_control_branch_in_flight_request_id = Some(1);
    app.source_control_branch_reload_queued = true;
    app.source_control_branches = vec![git_branch_for_test("current")];
    app.status = "branch picker closed".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBranchesFailed {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            error: "stale branch load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_branch_active_request_id, 2);
    assert_eq!(app.source_control_branch_in_flight_request_id, None);
    assert!(!app.source_control_branch_reload_queued);
    assert_eq!(
        app.source_control_branches,
        vec![git_branch_for_test("current")]
    );
    assert_eq!(app.status, "branch picker closed");
}

#[test]
fn closed_git_branch_mutation_event_does_not_start_hidden_branch_load() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_branch_picker_open = false;
    app.source_control_branch_next_request_id = 7;
    app.source_control_branch_active_request_id = 6;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBranchDeleteFinished {
            root: root.clone(),
            operation_root: root,
            branch: "feature/old".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_branch_next_request_id, 7);
    assert_eq!(app.source_control_branch_active_request_id, 6);
    assert_eq!(app.source_control_branch_in_flight_request_id, None);
    assert!(!app.source_control_branch_reload_queued);
    assert_eq!(app.status, "Deleted branch feature/old");
}

#[test]
fn current_git_stashes_loaded_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_stashes_open = true;
    app.source_control_stashes_next_request_id = 1;
    app.source_control_stashes_active_request_id = 1;
    app.source_control_stashes_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitStashesLoaded {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            stashes: Vec::new(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_stashes_in_flight_request_id, None);
    assert!(!app.source_control_stashes_reload_queued);
    assert_eq!(app.source_control_stashes_active_request_id, 1);
    assert_eq!(app.status, "No git stashes found");
}

#[test]
fn equivalent_root_git_stashes_loaded_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let mut app = app_for_test(root.clone());
    app.source_control_stashes_open = true;
    app.source_control_stashes_next_request_id = 1;
    app.source_control_stashes_active_request_id = 1;
    app.source_control_stashes_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitStashesLoaded {
            request_id: 1,
            root: event_root,
            operation_root: root,
            stashes: Vec::new(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_stashes_in_flight_request_id, None);
    assert!(!app.source_control_stashes_reload_queued);
    assert_eq!(app.source_control_stashes_active_request_id, 1);
    assert_eq!(app.status, "No git stashes found");
}

#[test]
fn current_git_stashes_failed_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_stashes_open = true;
    app.source_control_stashes_next_request_id = 1;
    app.source_control_stashes_active_request_id = 1;
    app.source_control_stashes_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitStashesFailed {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            error: "stash load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_stashes_in_flight_request_id, None);
    assert!(!app.source_control_stashes_reload_queued);
    assert_eq!(app.source_control_stashes_active_request_id, 1);
    assert_eq!(app.status, "Could not load git stashes: stash load failed");
}

#[test]
fn equivalent_root_git_stashes_failed_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let mut app = app_for_test(root.clone());
    app.source_control_stashes_open = true;
    app.source_control_stashes_next_request_id = 1;
    app.source_control_stashes_active_request_id = 1;
    app.source_control_stashes_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitStashesFailed {
            request_id: 1,
            root: event_root,
            operation_root: root,
            error: "stash load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_stashes_in_flight_request_id, None);
    assert!(!app.source_control_stashes_reload_queued);
    assert_eq!(app.source_control_stashes_active_request_id, 1);
    assert_eq!(app.status, "Could not load git stashes: stash load failed");
}

#[test]
fn stale_git_stashes_failed_event_from_other_workspace_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root);
    app.source_control_stashes_open = true;
    app.source_control_stashes_next_request_id = 2;
    app.source_control_stashes_active_request_id = 2;
    app.source_control_stashes_in_flight_request_id = Some(1);
    app.source_control_stashes_reload_queued = true;
    app.source_control_stashes = vec![git_stash_for_test(0, "current")];
    app.status = "current stash load".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitStashesFailed {
            request_id: 1,
            root: PathBuf::from("old-workspace"),
            operation_root: PathBuf::from("old-workspace"),
            error: "stale stash load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_stashes_in_flight_request_id, Some(1));
    assert!(app.source_control_stashes_reload_queued);
    assert_eq!(app.source_control_stashes_active_request_id, 2);
    assert_eq!(
        app.source_control_stashes,
        vec![git_stash_for_test(0, "current")]
    );
    assert_eq!(app.status, "current stash load");
}

#[test]
fn stale_git_stashes_loaded_event_from_other_workspace_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root);
    app.source_control_stashes_open = true;
    app.source_control_stashes_next_request_id = 2;
    app.source_control_stashes_active_request_id = 2;
    app.source_control_stashes_in_flight_request_id = Some(1);
    app.source_control_stashes_reload_queued = true;
    app.source_control_stashes = vec![git_stash_for_test(0, "current")];

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitStashesLoaded {
            request_id: 1,
            root: PathBuf::from("old-workspace"),
            operation_root: PathBuf::from("old-workspace"),
            stashes: vec![git_stash_for_test(1, "stale")],
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_stashes_in_flight_request_id, Some(1));
    assert!(app.source_control_stashes_reload_queued);
    assert_eq!(app.source_control_stashes_active_request_id, 2);
    assert_eq!(
        app.source_control_stashes,
        vec![git_stash_for_test(0, "current")]
    );
}

#[test]
fn stale_operation_root_git_stashes_loaded_event_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let stale_operation_root = root.join("old-repo");
    let mut app = app_for_test(root.clone());
    app.source_control_stashes_open = true;
    app.source_control_stashes_next_request_id = 1;
    app.source_control_stashes_active_request_id = 1;
    app.source_control_stashes_in_flight_request_id = Some(1);
    app.source_control_stashes_reload_queued = true;
    app.source_control_stashes = vec![git_stash_for_test(0, "current")];
    app.status = "current stash load".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitStashesLoaded {
            request_id: 1,
            root: root.clone(),
            operation_root: stale_operation_root,
            stashes: vec![git_stash_for_test(1, "stale")],
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_stashes_in_flight_request_id, Some(1));
    assert!(app.source_control_stashes_reload_queued);
    assert_eq!(app.source_control_stashes_active_request_id, 1);
    assert_eq!(
        app.source_control_stashes,
        vec![git_stash_for_test(0, "current")]
    );
    assert_eq!(app.status, "current stash load");
}

#[test]
fn stale_operation_root_git_stashes_failed_event_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let stale_operation_root = root.join("old-repo");
    let mut app = app_for_test(root.clone());
    app.source_control_stashes_open = true;
    app.source_control_stashes_next_request_id = 1;
    app.source_control_stashes_active_request_id = 1;
    app.source_control_stashes_in_flight_request_id = Some(1);
    app.source_control_stashes_reload_queued = true;
    app.source_control_stashes = vec![git_stash_for_test(0, "current")];
    app.status = "current stash load".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitStashesFailed {
            request_id: 1,
            root: root.clone(),
            operation_root: stale_operation_root,
            error: "stale stash load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_stashes_in_flight_request_id, Some(1));
    assert!(app.source_control_stashes_reload_queued);
    assert_eq!(app.source_control_stashes_active_request_id, 1);
    assert_eq!(
        app.source_control_stashes,
        vec![git_stash_for_test(0, "current")]
    );
    assert_eq!(app.status, "current stash load");
}

#[test]
fn stale_same_root_git_stashes_loaded_event_after_reset_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_stashes_open = true;
    app.source_control_stashes_active_request_id = 2;
    app.source_control_stashes_in_flight_request_id = Some(2);
    app.source_control_stashes = vec![git_stash_for_test(0, "current")];
    app.status = "current stash load".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitStashesLoaded {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            stashes: vec![git_stash_for_test(1, "stale")],
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_stashes_in_flight_request_id, Some(2));
    assert_eq!(app.source_control_stashes_active_request_id, 2);
    assert_eq!(
        app.source_control_stashes,
        vec![git_stash_for_test(0, "current")]
    );
    assert_eq!(app.status, "current stash load");
}

#[test]
fn stale_same_root_git_stashes_failed_event_after_reset_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_stashes_open = true;
    app.source_control_stashes_active_request_id = 2;
    app.source_control_stashes_in_flight_request_id = Some(2);
    app.source_control_stashes = vec![git_stash_for_test(0, "current")];
    app.status = "current stash load".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitStashesFailed {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            error: "stale stash load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_stashes_in_flight_request_id, Some(2));
    assert_eq!(app.source_control_stashes_active_request_id, 2);
    assert_eq!(
        app.source_control_stashes,
        vec![git_stash_for_test(0, "current")]
    );
    assert_eq!(app.status, "current stash load");
}

#[test]
fn current_root_stale_git_stashes_loaded_event_drains_queued_reload_without_applying_result() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_stashes_open = true;
    app.source_control_stashes_next_request_id = 2;
    app.source_control_stashes_active_request_id = 2;
    app.source_control_stashes_in_flight_request_id = Some(1);
    app.source_control_stashes_reload_queued = true;
    app.source_control_stashes = vec![git_stash_for_test(0, "current")];

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitStashesLoaded {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            stashes: vec![git_stash_for_test(1, "stale")],
        }
    ));

    let handled = app.handle_single_event();
    assert!(
        handled == 1 || handled == 2,
        "stale event should drain with at most its background reload failure, got {handled}"
    );
    assert_eq!(app.source_control_stashes_active_request_id, 3);
    assert_eq!(
        app.source_control_stashes_in_flight_request_id,
        if handled == 1 { Some(3) } else { None }
    );
    assert!(!app.source_control_stashes_reload_queued);
    if handled == 1 {
        assert_eq!(
            app.source_control_stashes,
            vec![git_stash_for_test(0, "current")]
        );
    } else {
        assert!(app.source_control_stashes.is_empty());
    }
}

#[test]
fn equivalent_root_stale_git_stashes_loaded_event_drains_queued_reload_without_applying_result() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let mut app = app_for_test(root.clone());
    app.source_control_stashes_open = true;
    app.source_control_stashes_next_request_id = 2;
    app.source_control_stashes_active_request_id = 2;
    app.source_control_stashes_in_flight_request_id = Some(1);
    app.source_control_stashes_reload_queued = true;
    app.source_control_stashes = vec![git_stash_for_test(0, "current")];

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitStashesLoaded {
            request_id: 1,
            root: event_root,
            operation_root: root,
            stashes: vec![git_stash_for_test(1, "stale")],
        }
    ));

    let handled = app.handle_single_event();
    assert!(
        handled == 1 || handled == 2,
        "stale event should drain with at most its background reload failure, got {handled}"
    );
    assert_eq!(app.source_control_stashes_active_request_id, 3);
    assert_eq!(
        app.source_control_stashes_in_flight_request_id,
        if handled == 1 { Some(3) } else { None }
    );
    assert!(!app.source_control_stashes_reload_queued);
    if handled == 1 {
        assert_eq!(
            app.source_control_stashes,
            vec![git_stash_for_test(0, "current")]
        );
    } else {
        assert!(app.source_control_stashes.is_empty());
    }
}

#[test]
fn current_root_queued_git_stashes_loaded_event_clears_closed_panel_without_respawning() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_stashes_open = false;
    app.source_control_stashes_next_request_id = 2;
    app.source_control_stashes_active_request_id = 2;
    app.source_control_stashes_in_flight_request_id = Some(1);
    app.source_control_stashes_reload_queued = true;
    app.source_control_stashes = vec![git_stash_for_test(0, "current")];
    app.status = "stash panel closed".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitStashesLoaded {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            stashes: vec![git_stash_for_test(1, "stale")],
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_stashes_active_request_id, 2);
    assert_eq!(app.source_control_stashes_in_flight_request_id, None);
    assert!(!app.source_control_stashes_reload_queued);
    assert_eq!(
        app.source_control_stashes,
        vec![git_stash_for_test(0, "current")]
    );
    assert_eq!(app.status, "stash panel closed");
}

#[test]
fn current_root_stale_git_stashes_failed_event_drains_queued_reload_without_applying_result() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_stashes_open = true;
    app.source_control_stashes_next_request_id = 2;
    app.source_control_stashes_active_request_id = 2;
    app.source_control_stashes_in_flight_request_id = Some(1);
    app.source_control_stashes_reload_queued = true;
    app.source_control_stashes = vec![git_stash_for_test(0, "current")];

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitStashesFailed {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            error: "stale stash load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.source_control_stashes_active_request_id, 3);
    assert_eq!(app.source_control_stashes_in_flight_request_id, Some(3));
    assert!(!app.source_control_stashes_reload_queued);
    assert_eq!(
        app.source_control_stashes,
        vec![git_stash_for_test(0, "current")]
    );
}

#[test]
fn equivalent_root_stale_git_stashes_failed_event_drains_queued_reload_without_applying_result() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let mut app = app_for_test(root.clone());
    app.source_control_stashes_open = true;
    app.source_control_stashes_next_request_id = 2;
    app.source_control_stashes_active_request_id = 2;
    app.source_control_stashes_in_flight_request_id = Some(1);
    app.source_control_stashes_reload_queued = true;
    app.source_control_stashes = vec![git_stash_for_test(0, "current")];

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitStashesFailed {
            request_id: 1,
            root: event_root,
            operation_root: root,
            error: "stale stash load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.source_control_stashes_active_request_id, 3);
    assert_eq!(app.source_control_stashes_in_flight_request_id, Some(3));
    assert!(!app.source_control_stashes_reload_queued);
    assert_eq!(
        app.source_control_stashes,
        vec![git_stash_for_test(0, "current")]
    );
}

#[test]
fn current_root_queued_git_stashes_failed_event_clears_closed_panel_without_respawning() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_stashes_open = false;
    app.source_control_stashes_next_request_id = 2;
    app.source_control_stashes_active_request_id = 2;
    app.source_control_stashes_in_flight_request_id = Some(1);
    app.source_control_stashes_reload_queued = true;
    app.source_control_stashes = vec![git_stash_for_test(0, "current")];
    app.status = "stash panel closed".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitStashesFailed {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            error: "stale stash load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_stashes_active_request_id, 2);
    assert_eq!(app.source_control_stashes_in_flight_request_id, None);
    assert!(!app.source_control_stashes_reload_queued);
    assert_eq!(
        app.source_control_stashes,
        vec![git_stash_for_test(0, "current")]
    );
    assert_eq!(app.status, "stash panel closed");
}

#[test]
fn closed_git_stash_mutation_event_does_not_start_hidden_stash_load() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.source_control_stashes_open = false;
    app.source_control_stashes_next_request_id = 7;
    app.source_control_stashes_active_request_id = 6;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitStashSaved {
            root: root.clone(),
            operation_root: root,
            short_oid: "abc1234".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_stashes_next_request_id, 7);
    assert_eq!(app.source_control_stashes_active_request_id, 6);
    assert_eq!(app.source_control_stashes_in_flight_request_id, None);
    assert!(!app.source_control_stashes_reload_queued);
    assert_eq!(app.status, "Saved git stash (abc1234)");
}
