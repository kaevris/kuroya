use super::*;

#[test]
fn handle_events_consumes_lsp_events_without_panicking() {
    let root = std::env::temp_dir().join("kuroya-ui-event-handler-lsp-test");
    let mut app = app_for_test(root.clone());

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::Lsp(LspUiEvent::WorkDoneProgressCreated {
            token: "progress-token".to_owned(),
        })
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert!(app.rx.try_recv().is_err());
}

#[test]
fn handle_events_is_bounded_per_frame() {
    let root = std::env::temp_dir().join("kuroya-ui-event-handler-budget-test");
    let mut app = app_for_test(root.clone());

    for index in 0..(UI_EVENT_DRAIN_BUDGET + 3) {
        assert!(crate::ui_event_channel::send_ui_event(
            &app.tx,
            UiEvent::Lsp(LspUiEvent::WorkDoneProgressCreated {
                token: format!("progress-token-{index}"),
            })
        ));
    }

    assert_eq!(app.handle_events(), UI_EVENT_DRAIN_BUDGET);
    assert!(app.rx.try_recv().is_ok());
}

#[test]
fn stale_same_root_local_history_loaded_after_reset_is_ignored() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let snapshot_path = root.join(".kuroya").join("history").join("1.main.rs.bak");
    let mut app = app_for_test(root.clone());
    let stale_generation = app.workspace_event_generation;

    app.reset_open_workspace_state();
    app.status = "before local history event".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::LocalHistoryLoaded {
            root,
            generation: stale_generation,
            path: path.clone(),
            snapshot_path,
            sequence: 1,
            text: "fn main() {}\n".to_owned(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert!(app.virtual_buffer_labels.is_empty());
    assert!(app.buffers.is_empty());
    assert_eq!(app.status, "before local history event");
}

#[test]
fn stale_same_root_local_history_failed_after_reset_is_ignored() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    let stale_generation = app.workspace_event_generation;

    app.reset_open_workspace_state();
    app.status = "before local history event".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::LocalHistoryFailed {
            root,
            generation: stale_generation,
            path,
            error: "stale local history failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert!(app.virtual_buffer_labels.is_empty());
    assert!(app.buffers.is_empty());
    assert_eq!(app.status, "before local history event");
}

#[test]
fn explorer_finished_from_other_workspace_is_ignored() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.status = "before explorer event".to_owned();
    let active_index_request = app.workspace_index_active_request_id;
    let old_root = PathBuf::from("old-workspace");
    let folder = old_root.join("created");

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::ExplorerOperationFinished {
            root: old_root,
            generation: app.workspace_event_generation,
            operation: ExplorerOperationResult::Created {
                path: folder,
                kind: ExplorerEntryKind::Folder,
            },
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert!(app.explorer_expanded.is_empty());
    assert!(app.pending_open_paths.is_empty());
    assert_eq!(app.workspace_index_active_request_id, active_index_request);
    assert_eq!(app.status, "before explorer event");
}

#[test]
fn stale_same_root_explorer_finished_after_reset_is_ignored() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    let stale_generation = app.workspace_event_generation;
    let folder = root.join("created");

    app.reset_open_workspace_state();
    app.status = "before explorer event".to_owned();
    let active_index_request = app.workspace_index_active_request_id;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::ExplorerOperationFinished {
            root,
            generation: stale_generation,
            operation: ExplorerOperationResult::Created {
                path: folder,
                kind: ExplorerEntryKind::Folder,
            },
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert!(app.explorer_expanded.is_empty());
    assert!(app.pending_open_paths.is_empty());
    assert_eq!(app.workspace_index_active_request_id, active_index_request);
    assert_eq!(app.status, "before explorer event");
}

#[test]
fn stale_same_root_explorer_failed_after_reset_is_ignored() {
    let root = PathBuf::from("workspace");
    let path = root.join("created");
    let mut app = app_for_test(root.clone());
    let stale_generation = app.workspace_event_generation;

    app.reset_open_workspace_state();
    app.status = "before explorer failure".to_owned();
    let active_index_request = app.workspace_index_active_request_id;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::ExplorerOperationFailed {
            root,
            generation: stale_generation,
            action: "create folder",
            path,
            error: "stale failure".to_owned(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert!(app.explorer_expanded.is_empty());
    assert_eq!(app.workspace_index_active_request_id, active_index_request);
    assert_eq!(app.status, "before explorer failure");
}

#[test]
fn current_explorer_failed_status_sanitizes_display_text() {
    let root = PathBuf::from("workspace");
    let path = root.join("bad\n\u{202e}tail.rs");
    let mut app = app_for_test(root.clone());
    let generation = app.workspace_event_generation;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::ExplorerOperationFailed {
            root,
            generation,
            action: "create\nfolder\u{202e}",
            path,
            error: "denied\nreason\u{202e}tail".to_owned(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(
        app.status,
        "Could not create folder bad tail.rs: denied reasontail"
    );
    assert!(!app.status.contains('\n'));
    assert!(!app.status.contains('\u{202e}'));
}

#[test]
fn current_explorer_failed_status_bounds_display_labels() {
    let root = PathBuf::from("workspace");
    let path = root.join(format!("{}tail.rs", "path-".repeat(80)));
    let error = format!("{}failure", "error-".repeat(80));
    let action: &'static str =
        Box::leak(format!("rename {}", "action-".repeat(32)).into_boxed_str());
    let mut app = app_for_test(root.clone());
    let generation = app.workspace_event_generation;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::ExplorerOperationFailed {
            root,
            generation,
            action,
            path: path.clone(),
            error: error.clone(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    let action_label = crate::path_display::sanitized_display_label_cow(
        action,
        EXPLORER_FAILURE_ACTION_LABEL_MAX_CHARS,
        "complete operation",
    );
    let path_label = crate::path_display::display_path_label_cow(&path);
    let error_label = crate::path_display::display_error_label_cow(&error);
    assert_eq!(
        app.status,
        format!(
            "Could not {} {}: {}",
            action_label.as_ref(),
            path_label.as_ref(),
            error_label.as_ref()
        )
    );
    assert!(action_label.as_ref().contains("..."));
    assert!(path_label.as_ref().contains("..."));
    assert!(error_label.as_ref().contains("..."));
    assert!(action_label.as_ref().chars().count() <= EXPLORER_FAILURE_ACTION_LABEL_MAX_CHARS);
    assert!(
        path_label.as_ref().chars().count() <= crate::path_display::DISPLAY_PATH_LABEL_MAX_CHARS
    );
    assert!(
        error_label.as_ref().chars().count() <= crate::path_display::DISPLAY_ERROR_LABEL_MAX_CHARS
    );
    assert!(
        app.status.chars().count()
            <= "Could not ".chars().count()
                + EXPLORER_FAILURE_ACTION_LABEL_MAX_CHARS
                + 1
                + crate::path_display::DISPLAY_PATH_LABEL_MAX_CHARS
                + ": ".chars().count()
                + crate::path_display::DISPLAY_ERROR_LABEL_MAX_CHARS
    );
}

#[test]
fn current_explorer_finished_still_applies() {
    let root = PathBuf::from("workspace");
    let folder = root.join("created");
    let mut app = app_for_test(root.clone());
    let generation = app.workspace_event_generation;
    let active_index_request = app.workspace_index_active_request_id;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::ExplorerOperationFinished {
            root,
            generation,
            operation: ExplorerOperationResult::Created {
                path: folder.clone(),
                kind: ExplorerEntryKind::Folder,
            },
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert!(app.explorer_expanded.contains(&folder));
    assert!(app.workspace_index_active_request_id > active_index_request);
    assert!(app.status.contains("Created folder"));
}

#[test]
fn stale_indexed_event_from_other_workspace_does_not_clear_current_in_flight_index() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.workspace_index_next_request_id = 2;
    app.workspace_index_active_request_id = 2;
    app.workspace_index_in_flight_request_id = Some(1);
    app.workspace_index_refresh_queued = true;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::Indexed {
            request_id: 1,
            root: PathBuf::from("old-workspace"),
            index: ProjectIndex::default(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.workspace_index_in_flight_request_id, Some(1));
    assert!(app.workspace_index_refresh_queued);
    assert_eq!(app.workspace_index_active_request_id, 2);
    assert_eq!(app.project_search_index_generation, 0);
}

#[test]
fn stale_same_root_indexed_event_after_reset_does_not_clear_current_in_flight_index() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.workspace_index_active_request_id = 2;
    app.workspace_index_in_flight_request_id = Some(2);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::Indexed {
            request_id: 1,
            root,
            index: ProjectIndex::default(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.workspace_index_in_flight_request_id, Some(2));
    assert_eq!(app.project_search_index_generation, 0);
}

#[test]
fn equivalent_root_indexed_event_finishes_in_flight_index() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let mut app = app_for_test(root.clone());
    app.workspace_index_next_request_id = 1;
    app.workspace_index_active_request_id = 1;
    app.workspace_index_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::Indexed {
            request_id: 1,
            root: event_root,
            index: ProjectIndex::default(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.workspace_index_next_request_id, 1);
    assert_eq!(app.workspace_index_in_flight_request_id, None);
    assert!(!app.workspace_index_refresh_queued);
    assert_eq!(app.workspace_index_active_request_id, 1);
    assert_eq!(app.project_search_index_generation, 1);
}

#[test]
fn completed_index_refresh_restarts_active_project_search() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.project_search = true;
    app.project_search_query = "needle".to_owned();
    app.project_search_result_query = "needle".to_owned();
    app.project_index_generation = 1;
    app.project_search_index_generation = 1;
    app.project_search_result_index_generation = 1;
    app.workspace_index_next_request_id = 2;
    app.workspace_index_active_request_id = 2;
    app.workspace_index_in_flight_request_id = Some(2);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::Indexed {
            request_id: 2,
            root,
            index: ProjectIndex::default(),
        }
    ));

    assert!(app.handle_events() >= 1);
    assert_eq!(app.workspace_index_in_flight_request_id, None);
    assert_eq!(app.project_search_index_generation, 2);
    assert_eq!(app.project_search_result_index_generation, 2);
    assert_eq!(app.project_search_active_request_id, 1);
    assert!(app.project_search_results_match_current_query());
}

#[test]
fn current_root_stale_indexed_event_drains_queued_refresh_without_applying_result() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.workspace_index_next_request_id = 2;
    app.workspace_index_active_request_id = 2;
    app.workspace_index_in_flight_request_id = Some(1);
    app.workspace_index_refresh_queued = true;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::Indexed {
            request_id: 1,
            root,
            index: ProjectIndex::default(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.project_search_index_generation, 0);
    assert_eq!(app.workspace_index_next_request_id, 3);
    assert_eq!(app.workspace_index_active_request_id, 3);
    assert_eq!(app.workspace_index_in_flight_request_id, Some(3));
    assert!(!app.workspace_index_refresh_queued);
}

#[test]
fn equivalent_root_stale_indexed_event_drains_queued_refresh_without_applying_result() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let mut app = app_for_test(root);
    app.workspace_index_next_request_id = 2;
    app.workspace_index_active_request_id = 2;
    app.workspace_index_in_flight_request_id = Some(1);
    app.workspace_index_refresh_queued = true;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::Indexed {
            request_id: 1,
            root: event_root,
            index: ProjectIndex::default(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.workspace_index_next_request_id, 3);
    assert_eq!(app.project_search_index_generation, 0);
    assert_eq!(app.workspace_index_active_request_id, 3);
    assert_eq!(app.workspace_index_in_flight_request_id, Some(3));
    assert!(!app.workspace_index_refresh_queued);
}

#[test]
fn current_git_scanned_event_finishes_in_flight_scan() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.git_scan_next_request_id = 1;
    app.git_scan_active_request_id = 1;
    app.git_scan_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitScanned {
            request_id: 1,
            root,
            scan_root: Some(PathBuf::from("workspace")),
            root_cache_entry: None,
            git: GitSnapshot::default(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.git_scan_next_request_id, 1);
    assert_eq!(app.git_scan_in_flight_request_id, None);
    assert!(!app.git_scan_refresh_queued);
    assert_eq!(app.git_scan_active_request_id, 1);
}

#[test]
fn current_git_scanned_event_with_scan_error_sets_failed_status() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.git_scan_next_request_id = 1;
    app.git_scan_active_request_id = 1;
    app.git_scan_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitScanned {
            request_id: 1,
            root,
            scan_root: Some(PathBuf::from("workspace")),
            root_cache_entry: None,
            git: GitSnapshot::default().with_scan_error("repository unavailable".to_owned()),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.status, "Git scan failed: repository unavailable");
    assert_eq!(app.git_scan_in_flight_request_id, None);
    assert!(!app.git_scan_refresh_queued);
}

#[test]
fn stale_git_scanned_event_with_scan_error_keeps_current_status() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root);
    app.status = "unchanged".to_owned();
    app.git_scan_next_request_id = 2;
    app.git_scan_active_request_id = 2;
    app.git_scan_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitScanned {
            request_id: 1,
            root: PathBuf::from("old-workspace"),
            scan_root: Some(PathBuf::from("old-workspace")),
            root_cache_entry: None,
            git: GitSnapshot::default().with_scan_error("stale failure".to_owned()),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.status, "unchanged");
}

#[test]
fn git_scan_failed_status_sanitizes_and_bounds_error_detail() {
    let status = git_scan_failed_status(&format!(
        "first line\nsecond line \u{202e}{}",
        "x".repeat(crate::path_display::DISPLAY_ERROR_LABEL_MAX_CHARS * 2)
    ));

    assert!(status.starts_with("Git scan failed: first line "));
    assert!(!status.contains('\n'));
    assert!(!status.contains('\u{202e}'));
    assert!(status.contains("..."));
    assert!(
        status.chars().count()
            <= "Git scan failed: ".chars().count()
                + crate::path_display::DISPLAY_ERROR_LABEL_MAX_CHARS
    );
}

#[test]
fn equivalent_root_git_scanned_event_finishes_in_flight_scan() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let mut app = app_for_test(root);
    app.git_scan_next_request_id = 1;
    app.git_scan_active_request_id = 1;
    app.git_scan_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitScanned {
            request_id: 1,
            root: event_root,
            scan_root: Some(PathBuf::from("workspace")),
            root_cache_entry: None,
            git: GitSnapshot::default(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.git_scan_in_flight_request_id, None);
    assert!(!app.git_scan_refresh_queued);
    assert_eq!(app.git_scan_active_request_id, 1);
}

#[test]
fn stale_git_scanned_event_from_other_workspace_does_not_clear_current_in_flight_scan() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root);
    app.git_scan_next_request_id = 2;
    app.git_scan_active_request_id = 2;
    app.git_scan_in_flight_request_id = Some(1);
    app.git_scan_refresh_queued = true;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitScanned {
            request_id: 1,
            root: PathBuf::from("old-workspace"),
            scan_root: Some(PathBuf::from("old-workspace")),
            root_cache_entry: None,
            git: GitSnapshot::default(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.git_scan_in_flight_request_id, Some(1));
    assert!(app.git_scan_refresh_queued);
    assert_eq!(app.git_scan_active_request_id, 2);
}

#[test]
fn stale_same_root_git_scanned_event_after_reset_does_not_clear_current_in_flight_scan() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.git_scan_active_request_id = 2;
    app.git_scan_in_flight_request_id = Some(2);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitScanned {
            request_id: 1,
            root,
            scan_root: Some(PathBuf::from("workspace")),
            root_cache_entry: None,
            git: GitSnapshot::default(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.git_scan_in_flight_request_id, Some(2));
    assert_eq!(app.git_scan_active_request_id, 2);
}

#[test]
fn current_root_stale_git_scanned_event_drains_queued_refresh_without_applying_result() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.settings.git_auto_repository_detection = GitAutoRepositoryDetection::True;
    app.git_scan_next_request_id = 2;
    app.git_scan_active_request_id = 2;
    app.git_scan_in_flight_request_id = Some(1);
    app.git_scan_refresh_queued = true;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitScanned {
            request_id: 1,
            root,
            scan_root: Some(PathBuf::from("workspace")),
            root_cache_entry: None,
            git: GitSnapshot::default(),
        }
    ));

    assert!(app.handle_events() >= 1);
    assert_eq!(app.git_scan_next_request_id, 3);
    assert_eq!(app.git_scan_active_request_id, 3);
    assert!(!app.git_scan_refresh_queued);
}

#[test]
fn equivalent_root_stale_git_scanned_event_drains_queued_refresh_without_applying_result() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let mut app = app_for_test(root);
    app.settings.git_auto_repository_detection = GitAutoRepositoryDetection::True;
    app.git_scan_next_request_id = 2;
    app.git_scan_active_request_id = 2;
    app.git_scan_in_flight_request_id = Some(1);
    app.git_scan_refresh_queued = true;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitScanned {
            request_id: 1,
            root: event_root,
            scan_root: Some(PathBuf::from("workspace")),
            root_cache_entry: None,
            git: GitSnapshot::default(),
        }
    ));

    assert!(app.handle_events() >= 1);
    assert_eq!(app.git_scan_next_request_id, 3);
    assert_eq!(app.git_scan_active_request_id, 3);
    assert_eq!(app.git_scan_in_flight_request_id, Some(3));
    assert!(!app.git_scan_refresh_queued);
}

#[test]
fn current_workspace_tasks_loaded_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.workspace_tasks_next_request_id = 1;
    app.workspace_tasks_active_request_id = 1;
    app.workspace_tasks_in_flight_request_id = Some(1);
    app.workspace_tasks_loading = true;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::WorkspaceTasksLoaded {
            request_id: 1,
            root,
            tasks: vec![workspace_task()],
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.workspace_tasks_in_flight_request_id, None);
    assert!(!app.workspace_tasks_reload_queued);
    assert_eq!(app.workspace_tasks_active_request_id, 1);
    assert!(!app.workspace_tasks_loading);
    assert!(app.workspace_tasks_loaded);
    assert_eq!(app.workspace_tasks.len(), 1);
}

#[test]
fn equivalent_root_workspace_tasks_loaded_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let mut app = app_for_test(root);
    app.workspace_tasks_next_request_id = 1;
    app.workspace_tasks_active_request_id = 1;
    app.workspace_tasks_in_flight_request_id = Some(1);
    app.workspace_tasks_loading = true;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::WorkspaceTasksLoaded {
            request_id: 1,
            root: event_root,
            tasks: vec![workspace_task()],
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.workspace_tasks_in_flight_request_id, None);
    assert!(!app.workspace_tasks_reload_queued);
    assert_eq!(app.workspace_tasks_active_request_id, 1);
    assert!(!app.workspace_tasks_loading);
    assert!(app.workspace_tasks_loaded);
    assert_eq!(app.workspace_tasks.len(), 1);
}

#[test]
fn current_workspace_tasks_failed_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.workspace_tasks_next_request_id = 1;
    app.workspace_tasks_active_request_id = 1;
    app.workspace_tasks_in_flight_request_id = Some(1);
    app.workspace_tasks_loading = true;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::WorkspaceTasksFailed {
            request_id: 1,
            root,
            error: "task load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.workspace_tasks_in_flight_request_id, None);
    assert!(!app.workspace_tasks_reload_queued);
    assert_eq!(app.workspace_tasks_active_request_id, 1);
    assert!(!app.workspace_tasks_loading);
    assert!(!app.workspace_tasks_loaded);
    assert_eq!(
        app.status,
        "Could not load workspace tasks: task load failed"
    );
}

#[test]
fn equivalent_root_workspace_tasks_failed_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let mut app = app_for_test(root);
    app.workspace_tasks_next_request_id = 1;
    app.workspace_tasks_active_request_id = 1;
    app.workspace_tasks_in_flight_request_id = Some(1);
    app.workspace_tasks_loading = true;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::WorkspaceTasksFailed {
            request_id: 1,
            root: event_root,
            error: "task load failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.workspace_tasks_in_flight_request_id, None);
    assert!(!app.workspace_tasks_reload_queued);
    assert_eq!(app.workspace_tasks_active_request_id, 1);
    assert!(!app.workspace_tasks_loading);
    assert!(!app.workspace_tasks_loaded);
    assert_eq!(
        app.status,
        "Could not load workspace tasks: task load failed"
    );
}

#[test]
fn stale_workspace_tasks_loaded_event_from_other_workspace_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root);
    app.workspace_tasks_next_request_id = 2;
    app.workspace_tasks_active_request_id = 2;
    app.workspace_tasks_in_flight_request_id = Some(1);
    app.workspace_tasks_reload_queued = true;
    app.workspace_tasks_loading = true;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::WorkspaceTasksLoaded {
            request_id: 1,
            root: PathBuf::from("old-workspace"),
            tasks: vec![workspace_task()],
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.workspace_tasks_in_flight_request_id, Some(1));
    assert!(app.workspace_tasks_reload_queued);
    assert_eq!(app.workspace_tasks_active_request_id, 2);
    assert!(app.workspace_tasks.is_empty());
}

#[test]
fn stale_same_root_workspace_tasks_loaded_event_after_reset_does_not_clear_current_in_flight_load()
{
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.workspace_tasks_active_request_id = 2;
    app.workspace_tasks_in_flight_request_id = Some(2);
    app.workspace_tasks_loading = true;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::WorkspaceTasksLoaded {
            request_id: 1,
            root,
            tasks: vec![workspace_task()],
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.workspace_tasks_in_flight_request_id, Some(2));
    assert_eq!(app.workspace_tasks_active_request_id, 2);
    assert!(app.workspace_tasks.is_empty());
}

#[test]
fn current_root_stale_workspace_tasks_loaded_event_drains_queued_reload_without_applying_result() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.workspace_trusted = false;
    app.workspace_tasks_open = true;
    app.workspace_tasks_next_request_id = 2;
    app.workspace_tasks_active_request_id = 2;
    app.workspace_tasks_in_flight_request_id = Some(1);
    app.workspace_tasks_reload_queued = true;
    app.workspace_tasks_loading = true;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::WorkspaceTasksLoaded {
            request_id: 1,
            root,
            tasks: vec![workspace_task()],
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.workspace_tasks_active_request_id, 3);
    assert_eq!(app.workspace_tasks_in_flight_request_id, None);
    assert!(!app.workspace_tasks_reload_queued);
    assert!(!app.workspace_tasks_loading);
    assert!(app.workspace_tasks.is_empty());
    assert_eq!(
        app.status,
        crate::workspace_tasks_runtime::workspace_tasks_restricted_status()
    );
}

#[test]
fn equivalent_root_stale_workspace_tasks_loaded_event_drains_queued_reload_without_applying_result()
{
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let mut app = app_for_test(root);
    app.workspace_trusted = false;
    app.workspace_tasks_open = true;
    app.workspace_tasks_next_request_id = 2;
    app.workspace_tasks_active_request_id = 2;
    app.workspace_tasks_in_flight_request_id = Some(1);
    app.workspace_tasks_reload_queued = true;
    app.workspace_tasks_loading = true;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::WorkspaceTasksLoaded {
            request_id: 1,
            root: event_root,
            tasks: vec![workspace_task()],
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.workspace_tasks_active_request_id, 3);
    assert_eq!(app.workspace_tasks_in_flight_request_id, None);
    assert!(!app.workspace_tasks_reload_queued);
    assert!(!app.workspace_tasks_loading);
    assert!(app.workspace_tasks.is_empty());
    assert_eq!(
        app.status,
        crate::workspace_tasks_runtime::workspace_tasks_restricted_status()
    );
}
