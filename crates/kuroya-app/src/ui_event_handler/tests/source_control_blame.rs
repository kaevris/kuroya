use super::*;

#[test]
fn current_git_blame_loaded_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_blame_next_request_id = 1;
    app.source_control_blame_active_request_id = 1;
    app.source_control_blame_active_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_in_flight_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_pending_path = Some(path.clone());

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBlameLoaded {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            path: path.clone(),
            lines: vec![git_blame_line_for_test(1, "current")],
            text: "fn main() {}\n".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_blame_next_request_id, 1);
    assert_eq!(app.source_control_blame_active_request_id, 1);
    assert!(app.source_control_blame_active_request_ids.is_empty());
    assert!(app.source_control_blame_in_flight_request_ids.is_empty());
    assert!(app.source_control_blame_reload_queued_paths.is_empty());
    assert!(app.source_control_blame_open_view_paths.is_empty());
    assert_eq!(app.source_control_blame_pending_path, None);
    assert!(!app.source_control_blame_load_opens_view);
    assert_eq!(
        app.source_control_blame_cache
            .get(&path)
            .and_then(|lines| lines.first())
            .map(|line| line.summary.as_str()),
        Some("current")
    );
}

#[test]
fn equivalent_root_git_blame_loaded_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_blame_next_request_id = 1;
    app.source_control_blame_active_request_id = 1;
    app.source_control_blame_active_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_in_flight_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_pending_path = Some(path.clone());

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBlameLoaded {
            request_id: 1,
            root: event_root,
            operation_root: root,
            path: path.clone(),
            lines: vec![git_blame_line_for_test(1, "current")],
            text: "fn main() {}\n".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_blame_next_request_id, 1);
    assert_eq!(app.source_control_blame_active_request_id, 1);
    assert!(app.source_control_blame_active_request_ids.is_empty());
    assert!(app.source_control_blame_in_flight_request_ids.is_empty());
    assert!(app.source_control_blame_reload_queued_paths.is_empty());
    assert!(app.source_control_blame_open_view_paths.is_empty());
    assert_eq!(app.source_control_blame_pending_path, None);
    assert!(!app.source_control_blame_load_opens_view);
    assert_eq!(
        app.source_control_blame_cache
            .get(&path)
            .and_then(|lines| lines.first())
            .map(|line| line.summary.as_str()),
        Some("current")
    );
}

#[test]
fn current_git_blame_failed_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_blame_next_request_id = 1;
    app.source_control_blame_active_request_id = 1;
    app.source_control_blame_active_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_in_flight_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_pending_path = Some(path.clone());
    app.status = "before blame failure".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBlameFailed {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            path: path.clone(),
            error: "blame failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_blame_next_request_id, 1);
    assert_eq!(app.source_control_blame_active_request_id, 1);
    assert!(app.source_control_blame_active_request_ids.is_empty());
    assert!(app.source_control_blame_in_flight_request_ids.is_empty());
    assert!(app.source_control_blame_reload_queued_paths.is_empty());
    assert!(app.source_control_blame_open_view_paths.is_empty());
    assert_eq!(app.source_control_blame_pending_path, None);
    assert!(!app.source_control_blame_load_opens_view);
    assert_eq!(app.source_control_blame_cache.get(&path), Some(&Vec::new()));
    assert_eq!(app.status, "before blame failure");
}

#[test]
fn equivalent_root_git_blame_failed_event_finishes_in_flight_load() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_blame_next_request_id = 1;
    app.source_control_blame_active_request_id = 1;
    app.source_control_blame_active_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_in_flight_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_pending_path = Some(path.clone());
    app.status = "before blame failure".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBlameFailed {
            request_id: 1,
            root: event_root,
            operation_root: root,
            path: path.clone(),
            error: "blame failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_blame_next_request_id, 1);
    assert_eq!(app.source_control_blame_active_request_id, 1);
    assert!(app.source_control_blame_active_request_ids.is_empty());
    assert!(app.source_control_blame_in_flight_request_ids.is_empty());
    assert!(app.source_control_blame_reload_queued_paths.is_empty());
    assert!(app.source_control_blame_open_view_paths.is_empty());
    assert_eq!(app.source_control_blame_pending_path, None);
    assert!(!app.source_control_blame_load_opens_view);
    assert_eq!(app.source_control_blame_cache.get(&path), Some(&Vec::new()));
    assert_eq!(app.status, "before blame failure");
}

#[test]
fn current_git_blame_failed_open_view_reports_status_without_negative_cache() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_blame_next_request_id = 1;
    app.source_control_blame_active_request_id = 1;
    app.source_control_blame_active_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_in_flight_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_open_view_paths
        .insert(path.clone());
    app.source_control_blame_pending_path = Some(path.clone());

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBlameFailed {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            path: path.clone(),
            error: "blame failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert!(app.source_control_blame_active_request_ids.is_empty());
    assert!(app.source_control_blame_in_flight_request_ids.is_empty());
    assert!(app.source_control_blame_reload_queued_paths.is_empty());
    assert!(!app.source_control_blame_open_view_paths.contains(&path));
    assert_eq!(app.source_control_blame_pending_path, None);
    assert!(!app.source_control_blame_cache.contains_key(&path));
    assert!(app.status.contains("Could not blame"));
    assert!(app.status.contains("blame failed"));
}

#[test]
fn stale_git_blame_loaded_event_from_other_workspace_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root);
    app.source_control_blame_next_request_id = 2;
    app.source_control_blame_active_request_id = 2;
    app.source_control_blame_active_request_ids
        .insert(path.clone(), 2);
    app.source_control_blame_in_flight_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_reload_queued_paths
        .insert(path.clone());
    app.source_control_blame_cache
        .insert(path.clone(), vec![git_blame_line_for_test(1, "current")]);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBlameLoaded {
            request_id: 1,
            root: PathBuf::from("old-workspace"),
            operation_root: PathBuf::from("old-workspace"),
            path: path.clone(),
            lines: vec![git_blame_line_for_test(1, "stale")],
            text: "fn main() {}\n".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(
        app.source_control_blame_in_flight_request_ids.get(&path),
        Some(&1)
    );
    assert!(app.source_control_blame_reload_queued_paths.contains(&path));
    assert_eq!(
        app.source_control_blame_cache
            .get(&path)
            .and_then(|lines| lines.first())
            .map(|line| line.summary.as_str()),
        Some("current")
    );
}

#[test]
fn stale_git_blame_failed_event_from_other_workspace_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root);
    app.source_control_blame_next_request_id = 2;
    app.source_control_blame_active_request_id = 2;
    app.source_control_blame_active_request_ids
        .insert(path.clone(), 2);
    app.source_control_blame_in_flight_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_reload_queued_paths
        .insert(path.clone());
    app.source_control_blame_cache
        .insert(path.clone(), vec![git_blame_line_for_test(1, "current")]);
    app.status = "current blame load".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBlameFailed {
            request_id: 1,
            root: PathBuf::from("old-workspace"),
            operation_root: PathBuf::from("old-workspace"),
            path: path.clone(),
            error: "stale blame failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(
        app.source_control_blame_in_flight_request_ids.get(&path),
        Some(&1)
    );
    assert!(app.source_control_blame_reload_queued_paths.contains(&path));
    assert_eq!(
        app.source_control_blame_cache
            .get(&path)
            .and_then(|lines| lines.first())
            .map(|line| line.summary.as_str()),
        Some("current")
    );
    assert_eq!(app.status, "current blame load");
}

#[test]
fn stale_operation_root_git_blame_loaded_event_does_not_clear_current_in_flight_load() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_blame_next_request_id = 2;
    app.source_control_blame_active_request_id = 2;
    app.source_control_blame_active_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_in_flight_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_reload_queued_paths
        .insert(path.clone());
    app.source_control_blame_cache
        .insert(path.clone(), vec![git_blame_line_for_test(1, "current")]);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBlameLoaded {
            request_id: 1,
            root,
            operation_root: PathBuf::from("old-workspace"),
            path: path.clone(),
            lines: vec![git_blame_line_for_test(1, "stale")],
            text: "fn main() {}\n".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(
        app.source_control_blame_in_flight_request_ids.get(&path),
        Some(&1)
    );
    assert!(app.source_control_blame_reload_queued_paths.contains(&path));
    assert_eq!(
        app.source_control_blame_cache
            .get(&path)
            .and_then(|lines| lines.first())
            .map(|line| line.summary.as_str()),
        Some("current")
    );
}

#[test]
fn current_root_stale_git_blame_loaded_event_drains_queued_reload_without_applying_result() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_blame_next_request_id = 2;
    app.source_control_blame_active_request_id = 2;
    app.source_control_blame_active_request_ids
        .insert(path.clone(), 2);
    app.source_control_blame_in_flight_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_reload_queued_paths
        .insert(path.clone());
    app.source_control_blame_open_view_paths
        .insert(path.clone());
    app.source_control_blame_cache
        .insert(path.clone(), vec![git_blame_line_for_test(1, "current")]);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBlameLoaded {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            path: path.clone(),
            lines: vec![git_blame_line_for_test(1, "stale")],
            text: "fn main() {}\n".to_owned(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(
        app.source_control_blame_in_flight_request_ids.get(&path),
        Some(&3)
    );
    assert_eq!(app.source_control_blame_next_request_id, 3);
    assert_eq!(app.source_control_blame_active_request_id, 3);
    assert_eq!(
        app.source_control_blame_active_request_ids.get(&path),
        Some(&3)
    );
    assert!(!app.source_control_blame_reload_queued_paths.contains(&path));
    assert!(app.source_control_blame_open_view_paths.contains(&path));
    assert_eq!(app.source_control_blame_pending_path, Some(path.clone()));
    assert!(app.source_control_blame_load_opens_view);
    assert_eq!(
        app.source_control_blame_cache
            .get(&path)
            .and_then(|lines| lines.first())
            .map(|line| line.summary.as_str()),
        Some("current")
    );
    assert_eq!(
        app.status,
        format!(
            "Loading blame for {}",
            crate::path_display::compact_path(&path)
        )
    );
    assert_eq!(
        app.status,
        format!(
            "Loading blame for {}",
            crate::path_display::compact_path(&path)
        )
    );
}

#[test]
fn equivalent_root_stale_git_blame_loaded_event_drains_queued_reload_without_applying_result() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_blame_next_request_id = 2;
    app.source_control_blame_active_request_id = 2;
    app.source_control_blame_active_request_ids
        .insert(path.clone(), 2);
    app.source_control_blame_in_flight_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_reload_queued_paths
        .insert(path.clone());
    app.source_control_blame_open_view_paths
        .insert(path.clone());
    app.source_control_blame_cache
        .insert(path.clone(), vec![git_blame_line_for_test(1, "current")]);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBlameLoaded {
            request_id: 1,
            root: event_root,
            operation_root: root,
            path: path.clone(),
            lines: vec![git_blame_line_for_test(1, "stale")],
            text: "fn main() {}\n".to_owned(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(
        app.source_control_blame_in_flight_request_ids.get(&path),
        Some(&3)
    );
    assert_eq!(
        app.source_control_blame_active_request_ids.get(&path),
        Some(&3)
    );
    assert!(!app.source_control_blame_reload_queued_paths.contains(&path));
    assert!(app.source_control_blame_open_view_paths.contains(&path));
    assert_eq!(app.source_control_blame_pending_path, Some(path.clone()));
    assert!(app.source_control_blame_load_opens_view);
    assert_eq!(
        app.source_control_blame_cache
            .get(&path)
            .and_then(|lines| lines.first())
            .map(|line| line.summary.as_str()),
        Some("current")
    );
}

#[test]
fn current_root_stale_git_blame_failed_event_drains_queued_reload_without_applying_result() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_blame_next_request_id = 2;
    app.source_control_blame_active_request_id = 2;
    app.source_control_blame_active_request_ids
        .insert(path.clone(), 2);
    app.source_control_blame_in_flight_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_reload_queued_paths
        .insert(path.clone());
    app.source_control_blame_open_view_paths
        .insert(path.clone());
    app.source_control_blame_cache
        .insert(path.clone(), vec![git_blame_line_for_test(1, "current")]);
    app.status = "current blame load".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBlameFailed {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            path: path.clone(),
            error: "stale blame failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(
        app.source_control_blame_in_flight_request_ids.get(&path),
        Some(&3)
    );
    assert_eq!(app.source_control_blame_next_request_id, 3);
    assert_eq!(app.source_control_blame_active_request_id, 3);
    assert_eq!(
        app.source_control_blame_active_request_ids.get(&path),
        Some(&3)
    );
    assert!(!app.source_control_blame_reload_queued_paths.contains(&path));
    assert!(app.source_control_blame_open_view_paths.contains(&path));
    assert_eq!(app.source_control_blame_pending_path, Some(path.clone()));
    assert!(app.source_control_blame_load_opens_view);
    assert_eq!(
        app.source_control_blame_cache
            .get(&path)
            .and_then(|lines| lines.first())
            .map(|line| line.summary.as_str()),
        Some("current")
    );
    assert!(!app.status.contains("stale blame failed"));
    assert_eq!(
        app.status,
        format!(
            "Loading blame for {}",
            crate::path_display::compact_path(&path)
        )
    );
}

#[test]
fn equivalent_root_stale_git_blame_failed_event_drains_queued_reload_without_applying_result() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("src").join("..");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_blame_next_request_id = 2;
    app.source_control_blame_active_request_id = 2;
    app.source_control_blame_active_request_ids
        .insert(path.clone(), 2);
    app.source_control_blame_in_flight_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_reload_queued_paths
        .insert(path.clone());
    app.source_control_blame_open_view_paths
        .insert(path.clone());
    app.source_control_blame_cache
        .insert(path.clone(), vec![git_blame_line_for_test(1, "current")]);
    app.status = "current blame load".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBlameFailed {
            request_id: 1,
            root: event_root,
            operation_root: root,
            path: path.clone(),
            error: "stale blame failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(
        app.source_control_blame_in_flight_request_ids.get(&path),
        Some(&3)
    );
    assert!(!app.source_control_blame_reload_queued_paths.contains(&path));
    assert!(app.source_control_blame_open_view_paths.contains(&path));
    assert_eq!(
        app.source_control_blame_cache
            .get(&path)
            .and_then(|lines| lines.first())
            .map(|line| line.summary.as_str()),
        Some("current")
    );
    assert_eq!(
        app.status,
        format!(
            "Loading blame for {}",
            crate::path_display::compact_path(&path)
        )
    );
}

#[test]
fn queued_git_blame_loaded_event_finishes_final_request_and_opens_view() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    seed_queued_blame_final_request(&mut app, &path);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBlameLoaded {
            request_id: 3,
            root: root.clone(),
            operation_root: root,
            path: path.clone(),
            lines: vec![git_blame_line_for_test(1, "fresh")],
            text: "fn main() {}\n".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_blame_next_request_id, 3);
    assert_eq!(app.source_control_blame_active_request_id, 3);
    assert!(app.source_control_blame_active_request_ids.is_empty());
    assert!(app.source_control_blame_in_flight_request_ids.is_empty());
    assert!(app.source_control_blame_reload_queued_paths.is_empty());
    assert!(!app.source_control_blame_open_view_paths.contains(&path));
    assert!(app.source_control_blame_open_view_paths.is_empty());
    assert_eq!(app.source_control_blame_pending_path, None);
    assert!(!app.source_control_blame_load_opens_view);
    assert_eq!(
        app.source_control_blame_cache
            .get(&path)
            .and_then(|lines| lines.first())
            .map(|line| line.summary.as_str()),
        Some("fresh")
    );
    assert!(
        app.virtual_buffer_labels
            .values()
            .any(|label| label == "main.rs (Blame)")
    );
    let active_id = app.active.expect("blame view should become active");
    let active_buffer = app.buffer(active_id).expect("active blame buffer exists");
    assert_eq!(
        app.virtual_buffer_labels
            .get(&active_id)
            .map(String::as_str),
        Some("main.rs (Blame)")
    );
    assert!(active_buffer.is_read_only());
    assert!(active_buffer.text().contains("fresh"));
}

#[test]
fn queued_git_blame_failed_event_finishes_final_request_without_negative_cache() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    seed_queued_blame_final_request(&mut app, &path);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBlameFailed {
            request_id: 3,
            root: root.clone(),
            operation_root: root,
            path: path.clone(),
            error: "queued failure".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_blame_next_request_id, 3);
    assert_eq!(app.source_control_blame_active_request_id, 3);
    assert!(app.source_control_blame_active_request_ids.is_empty());
    assert!(app.source_control_blame_in_flight_request_ids.is_empty());
    assert!(app.source_control_blame_reload_queued_paths.is_empty());
    assert!(!app.source_control_blame_open_view_paths.contains(&path));
    assert!(app.source_control_blame_open_view_paths.is_empty());
    assert_eq!(app.source_control_blame_pending_path, None);
    assert!(!app.source_control_blame_load_opens_view);
    assert_eq!(
        app.source_control_blame_cache
            .get(&path)
            .and_then(|lines| lines.first())
            .map(|line| line.summary.as_str()),
        Some("old")
    );
    assert!(app.virtual_buffer_labels.is_empty());
    assert!(app.status.contains("Could not blame"));
    assert!(app.status.contains("queued failure"));
}

#[test]
fn path_specific_git_blame_result_survives_newer_request_for_other_path() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let other_path = root.join("src").join("lib.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_blame_next_request_id = 2;
    app.source_control_blame_active_request_id = 2;
    app.source_control_blame_active_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_active_request_ids
        .insert(other_path.clone(), 2);
    app.source_control_blame_in_flight_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_in_flight_request_ids
        .insert(other_path, 2);
    app.source_control_blame_open_view_paths
        .insert(path.clone());

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBlameLoaded {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            path: path.clone(),
            lines: vec![git_blame_line_for_test(1, "current")],
            text: "fn main() {}\n".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert!(app.source_control_blame_cache.contains_key(&path));
    assert!(
        app.virtual_buffer_labels
            .values()
            .any(|label| label == "main.rs (Blame)")
    );
    assert_eq!(
        app.status,
        format!(
            "Opened blame for {}",
            crate::path_display::compact_path(&path)
        )
    );
}

#[test]
fn duplicate_passive_git_blame_request_for_same_path_reuses_in_flight_load() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root);
    app.source_control_blame_next_request_id = 1;
    app.source_control_blame_active_request_id = 1;
    app.source_control_blame_active_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_in_flight_request_ids
        .insert(path.clone(), 1);

    app.request_file_blame(path.clone(), false);

    assert_eq!(app.source_control_blame_next_request_id, 1);
    assert_eq!(app.source_control_blame_active_request_id, 1);
    assert_eq!(
        app.source_control_blame_in_flight_request_ids.get(&path),
        Some(&1)
    );
    assert!(!app.source_control_blame_reload_queued_paths.contains(&path));
    assert!(!app.source_control_blame_open_view_paths.contains(&path));
}

#[test]
fn open_git_blame_promotes_existing_passive_in_flight_load() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_blame_next_request_id = 1;
    app.source_control_blame_active_request_id = 1;
    app.source_control_blame_active_request_ids
        .insert(path.clone(), 1);
    app.source_control_blame_in_flight_request_ids
        .insert(path.clone(), 1);

    app.request_file_blame(path.clone(), true);

    assert_eq!(app.source_control_blame_next_request_id, 1);
    assert_eq!(app.source_control_blame_active_request_id, 1);
    assert_eq!(
        app.source_control_blame_in_flight_request_ids.get(&path),
        Some(&1)
    );
    assert!(app.source_control_blame_open_view_paths.contains(&path));
    assert_eq!(
        app.status,
        format!(
            "Loading blame for {}",
            crate::path_display::compact_path(&path)
        )
    );

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBlameLoaded {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            path: path.clone(),
            lines: vec![git_blame_line_for_test(1, "current")],
            text: "fn main() {}\n".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert!(app.source_control_blame_cache.contains_key(&path));
    assert!(
        app.virtual_buffer_labels
            .values()
            .any(|label| label == "main.rs (Blame)")
    );
}

#[test]
fn stale_same_root_blame_loaded_after_reset_is_ignored() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_blame_next_request_id = 1;
    app.source_control_blame_active_request_id = 1;

    app.reset_open_workspace_state();
    app.source_control_blame_pending_path = Some(path.clone());
    app.source_control_blame_load_opens_view = true;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBlameLoaded {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            path: path.clone(),
            lines: Vec::new(),
            text: "fn main() {}\n".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_blame_active_request_id, 2);
    assert_eq!(app.source_control_blame_pending_path, Some(path.clone()));
    assert!(!app.source_control_blame_cache.contains_key(&path));
    assert!(
        app.buffers
            .iter()
            .all(|buffer| buffer.path() != Some(&path))
    );
}

#[test]
fn stale_same_root_blame_failed_after_reset_is_ignored() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_blame_next_request_id = 1;
    app.source_control_blame_active_request_id = 1;

    app.reset_open_workspace_state();
    app.source_control_blame_pending_path = Some(path.clone());
    app.source_control_blame_load_opens_view = true;
    app.status = "before blame event".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBlameFailed {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            path: path.clone(),
            error: "stale blame failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_blame_active_request_id, 2);
    assert_eq!(app.source_control_blame_pending_path, Some(path.clone()));
    assert!(!app.source_control_blame_cache.contains_key(&path));
    assert_eq!(app.status, "before blame event");
}

#[test]
fn stale_same_root_hunks_failed_after_reset_is_ignored() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.source_control_hunks_next_request_id = 1;
    app.source_control_hunks_active_request_id = 1;

    app.reset_open_workspace_state();
    app.source_control_hunks_open = true;
    app.source_control_hunk_path = Some(path.clone());
    app.source_control_hunk_stage = GitChangeStage::Unstaged;
    app.status = "before hunk event".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHunksFailed {
            request_id: 1,
            root: root.clone(),
            operation_root: root,
            path,
            stage: GitChangeStage::Unstaged,
            error: "stale hunk load".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_hunks_active_request_id, 2);
    assert_eq!(app.status, "before hunk event");
    assert!(app.source_control_hunks.is_empty());
}

#[test]
fn blame_setting_change_invalidates_active_request_monotonically() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root);
    app.settings.git_blame_ignore_whitespace = true;
    app.source_control_blame_ignore_whitespace = false;
    app.source_control_blame_next_request_id = 7;
    app.source_control_blame_active_request_id = 7;
    app.source_control_blame_pending_path = Some(path.clone());
    app.source_control_blame_load_opens_view = true;
    app.source_control_blame_active_request_ids
        .insert(path.clone(), 7);
    app.source_control_blame_in_flight_request_ids
        .insert(path.clone(), 7);
    app.source_control_blame_reload_queued_paths
        .insert(path.clone());
    app.source_control_blame_open_view_paths
        .insert(path.clone());
    app.source_control_blame_cache
        .insert(path.clone(), Vec::new());

    app.sync_source_control_blame_settings();

    assert!(app.source_control_blame_ignore_whitespace);
    assert_eq!(app.source_control_blame_next_request_id, 8);
    assert_eq!(app.source_control_blame_active_request_id, 8);
    assert_eq!(app.source_control_blame_pending_path, None);
    assert!(!app.source_control_blame_load_opens_view);
    assert!(app.source_control_blame_active_request_ids.is_empty());
    assert!(app.source_control_blame_in_flight_request_ids.is_empty());
    assert!(app.source_control_blame_reload_queued_paths.is_empty());
    assert!(app.source_control_blame_open_view_paths.is_empty());
    assert!(app.source_control_blame_cache.is_empty());
}

#[test]
fn stale_blame_loaded_after_blame_setting_change_is_ignored() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.settings.git_blame_ignore_whitespace = true;
    app.source_control_blame_ignore_whitespace = false;
    app.source_control_blame_next_request_id = 7;
    app.source_control_blame_active_request_id = 7;
    app.source_control_blame_pending_path = Some(path.clone());
    app.source_control_blame_load_opens_view = true;

    app.sync_source_control_blame_settings();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBlameLoaded {
            request_id: 7,
            root: root.clone(),
            operation_root: root,
            path: path.clone(),
            lines: Vec::new(),
            text: "fn main() {}\n".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_blame_active_request_id, 8);
    assert_eq!(app.source_control_blame_pending_path, None);
    assert!(!app.source_control_blame_cache.contains_key(&path));
    assert!(app.virtual_buffer_labels.is_empty());
}

#[test]
fn stale_blame_failed_after_blame_setting_change_is_ignored() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.settings.git_blame_ignore_whitespace = true;
    app.source_control_blame_ignore_whitespace = false;
    app.source_control_blame_next_request_id = 7;
    app.source_control_blame_active_request_id = 7;
    app.source_control_blame_active_request_ids
        .insert(path.clone(), 7);
    app.source_control_blame_in_flight_request_ids
        .insert(path.clone(), 7);
    app.source_control_blame_open_view_paths
        .insert(path.clone());
    app.source_control_blame_pending_path = Some(path.clone());
    app.source_control_blame_load_opens_view = true;
    app.status = "before blame event".to_owned();

    app.sync_source_control_blame_settings();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBlameFailed {
            request_id: 7,
            root: root.clone(),
            operation_root: root,
            path: path.clone(),
            error: "stale blame failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_blame_active_request_id, 8);
    assert_eq!(app.source_control_blame_pending_path, None);
    assert!(!app.source_control_blame_cache.contains_key(&path));
    assert!(app.virtual_buffer_labels.is_empty());
    assert_eq!(app.status, "before blame event");
}

#[test]
fn stale_blame_loaded_after_buffer_change_is_ignored() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.buffers.push(kuroya_core::TextBuffer::from_text(
        1,
        Some(path.clone()),
        "fn main() {}\n".to_owned(),
    ));
    app.source_control_blame_next_request_id = 7;
    app.source_control_blame_active_request_id = 7;
    app.source_control_blame_pending_path = Some(path.clone());
    app.source_control_blame_load_opens_view = true;

    app.mark_buffer_changed(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBlameLoaded {
            request_id: 7,
            root: root.clone(),
            operation_root: root,
            path: path.clone(),
            lines: Vec::new(),
            text: "fn main() {}\n".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_blame_active_request_id, 8);
    assert_eq!(app.source_control_blame_pending_path, None);
    assert!(!app.source_control_blame_cache.contains_key(&path));
    assert!(app.virtual_buffer_labels.is_empty());
}

#[test]
fn stale_blame_failed_after_buffer_change_is_ignored() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.buffers.push(kuroya_core::TextBuffer::from_text(
        1,
        Some(path.clone()),
        "fn main() {}\n".to_owned(),
    ));
    app.source_control_blame_next_request_id = 7;
    app.source_control_blame_active_request_id = 7;
    app.source_control_blame_active_request_ids
        .insert(path.clone(), 7);
    app.source_control_blame_in_flight_request_ids
        .insert(path.clone(), 7);
    app.source_control_blame_open_view_paths
        .insert(path.clone());
    app.source_control_blame_pending_path = Some(path.clone());
    app.source_control_blame_load_opens_view = true;
    app.status = "before blame event".to_owned();

    app.mark_buffer_changed(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitBlameFailed {
            request_id: 7,
            root: root.clone(),
            operation_root: root,
            path: path.clone(),
            error: "stale blame failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_blame_active_request_id, 8);
    assert_eq!(app.source_control_blame_pending_path, None);
    assert!(!app.source_control_blame_cache.contains_key(&path));
    assert!(app.virtual_buffer_labels.is_empty());
    assert_eq!(app.status, "before blame event");
}

#[test]
fn stale_hunks_loaded_after_buffer_change_is_ignored() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.buffers.push(kuroya_core::TextBuffer::from_text(
        1,
        Some(path.clone()),
        "fn main() {}\n".to_owned(),
    ));
    app.source_control_hunks_next_request_id = 7;
    app.source_control_hunks_active_request_id = 7;
    app.source_control_hunks_open = true;
    app.source_control_hunk_path = Some(path.clone());
    app.source_control_hunk_stage = GitChangeStage::Unstaged;

    app.mark_buffer_changed(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitHunksLoaded {
            request_id: 7,
            root: root.clone(),
            operation_root: root,
            path,
            stage: GitChangeStage::Unstaged,
            hunks: Vec::new(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.source_control_hunks_active_request_id, 8);
    assert!(!app.source_control_hunks_open);
    assert_eq!(app.source_control_hunk_path, None);
    assert!(app.source_control_hunks.is_empty());
}
