use super::*;

#[test]
fn current_workspace_plugins_loaded_event_finishes_in_flight_discovery() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.workspace_plugins_next_request_id = 1;
    app.workspace_plugins_active_request_id = 1;
    app.workspace_plugins_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::WorkspacePluginsLoaded {
            request_id: 1,
            root,
            plugins: Vec::new(),
            errors: Vec::new(),
            syntax_load: crate::syntax::PluginSyntaxLoad::empty(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.workspace_plugins_in_flight_request_id, None);
    assert!(!app.workspace_plugins_reload_queued);
    assert_eq!(app.workspace_plugins_active_request_id, 1);
}

#[test]
fn equivalent_root_workspace_plugins_loaded_event_finishes_in_flight_discovery() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("plugins").join("..");
    let mut app = app_for_test(root);
    app.workspace_plugins_next_request_id = 1;
    app.workspace_plugins_active_request_id = 1;
    app.workspace_plugins_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::WorkspacePluginsLoaded {
            request_id: 1,
            root: event_root,
            plugins: Vec::new(),
            errors: Vec::new(),
            syntax_load: crate::syntax::PluginSyntaxLoad::empty(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.workspace_plugins_in_flight_request_id, None);
    assert!(!app.workspace_plugins_reload_queued);
    assert_eq!(app.workspace_plugins_active_request_id, 1);
}

#[test]
fn current_workspace_plugins_failed_event_finishes_in_flight_discovery() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.workspace_plugins_next_request_id = 1;
    app.workspace_plugins_active_request_id = 1;
    app.workspace_plugins_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::WorkspacePluginsFailed {
            request_id: 1,
            root,
            error: "discovery failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.workspace_plugins_in_flight_request_id, None);
    assert!(!app.workspace_plugins_reload_queued);
    assert_eq!(app.workspace_plugins_active_request_id, 1);
    assert_eq!(
        app.status,
        "Could not load workspace plugins: discovery failed"
    );
}

#[test]
fn equivalent_root_workspace_plugins_failed_event_finishes_in_flight_discovery() {
    let root = PathBuf::from("workspace");
    let event_root = root.join("plugins").join("..");
    let mut app = app_for_test(root);
    app.workspace_plugins_next_request_id = 1;
    app.workspace_plugins_active_request_id = 1;
    app.workspace_plugins_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::WorkspacePluginsFailed {
            request_id: 1,
            root: event_root,
            error: "discovery failed".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.workspace_plugins_in_flight_request_id, None);
    assert!(!app.workspace_plugins_reload_queued);
    assert_eq!(app.workspace_plugins_active_request_id, 1);
    assert_eq!(
        app.status,
        "Could not load workspace plugins: discovery failed"
    );
}

#[test]
fn stale_workspace_plugins_loaded_event_from_other_workspace_does_not_clear_current_in_flight_discovery()
 {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root);
    app.workspace_plugins_next_request_id = 2;
    app.workspace_plugins_active_request_id = 2;
    app.workspace_plugins_in_flight_request_id = Some(1);
    app.workspace_plugins_reload_queued = true;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::WorkspacePluginsLoaded {
            request_id: 1,
            root: PathBuf::from("old-workspace"),
            plugins: Vec::new(),
            errors: Vec::new(),
            syntax_load: crate::syntax::PluginSyntaxLoad::empty(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.workspace_plugins_in_flight_request_id, Some(1));
    assert!(app.workspace_plugins_reload_queued);
    assert_eq!(app.workspace_plugins_active_request_id, 2);
}

#[test]
fn stale_workspace_plugins_failed_event_from_other_workspace_does_not_clear_current_in_flight_discovery()
 {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root);
    app.workspace_plugins_next_request_id = 2;
    app.workspace_plugins_active_request_id = 2;
    app.workspace_plugins_in_flight_request_id = Some(1);
    app.workspace_plugins_reload_queued = true;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::WorkspacePluginsFailed {
            request_id: 1,
            root: PathBuf::from("old-workspace"),
            error: "stale".to_owned(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.workspace_plugins_in_flight_request_id, Some(1));
    assert!(app.workspace_plugins_reload_queued);
    assert_eq!(app.workspace_plugins_active_request_id, 2);
}

#[test]
fn stale_same_root_workspace_plugins_loaded_event_after_reset_does_not_clear_current_in_flight_discovery()
 {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.workspace_plugins_active_request_id = 2;
    app.workspace_plugins_in_flight_request_id = Some(2);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::WorkspacePluginsLoaded {
            request_id: 1,
            root,
            plugins: Vec::new(),
            errors: Vec::new(),
            syntax_load: crate::syntax::PluginSyntaxLoad::empty(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.workspace_plugins_in_flight_request_id, Some(2));
    assert_eq!(app.workspace_plugins_active_request_id, 2);
}

#[test]
fn current_root_stale_workspace_plugins_loaded_event_drains_queued_reload_without_applying_result()
{
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.workspace_trusted = false;
    app.workspace_plugins_next_request_id = 2;
    app.workspace_plugins_active_request_id = 2;
    app.workspace_plugins_in_flight_request_id = Some(1);
    app.workspace_plugins_reload_queued = true;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::WorkspacePluginsLoaded {
            request_id: 1,
            root,
            plugins: Vec::new(),
            errors: Vec::new(),
            syntax_load: crate::syntax::PluginSyntaxLoad::empty(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.workspace_plugins_active_request_id, 3);
    assert_eq!(app.workspace_plugins_in_flight_request_id, None);
    assert!(!app.workspace_plugins_reload_queued);
    assert_eq!(
        app.status,
        crate::startup_tasks::workspace_plugins_restricted_status()
    );
}

#[test]
fn equivalent_root_stale_workspace_plugins_loaded_event_drains_queued_reload_without_applying_result()
 {
    let root = PathBuf::from("workspace");
    let event_root = root.join("plugins").join("..");
    let mut app = app_for_test(root);
    app.workspace_trusted = false;
    app.workspace_plugins_next_request_id = 2;
    app.workspace_plugins_active_request_id = 2;
    app.workspace_plugins_in_flight_request_id = Some(1);
    app.workspace_plugins_reload_queued = true;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::WorkspacePluginsLoaded {
            request_id: 1,
            root: event_root,
            plugins: Vec::new(),
            errors: Vec::new(),
            syntax_load: crate::syntax::PluginSyntaxLoad::empty(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.workspace_plugins_active_request_id, 3);
    assert_eq!(app.workspace_plugins_in_flight_request_id, None);
    assert!(!app.workspace_plugins_reload_queued);
    assert_eq!(
        app.status,
        crate::startup_tasks::workspace_plugins_restricted_status()
    );
}

#[test]
fn equivalent_root_stale_workspace_plugins_failed_event_drains_queued_reload_without_applying_result()
 {
    let root = PathBuf::from("workspace");
    let event_root = root.join("plugins").join("..");
    let mut app = app_for_test(root);
    app.workspace_trusted = false;
    app.workspace_plugins_next_request_id = 2;
    app.workspace_plugins_active_request_id = 2;
    app.workspace_plugins_in_flight_request_id = Some(1);
    app.workspace_plugins_reload_queued = true;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::WorkspacePluginsFailed {
            request_id: 1,
            root: event_root,
            error: "stale failure".to_owned(),
        }
    ));

    assert_eq!(app.handle_single_event(), 1);
    assert_eq!(app.workspace_plugins_active_request_id, 3);
    assert_eq!(app.workspace_plugins_in_flight_request_id, None);
    assert!(!app.workspace_plugins_reload_queued);
    assert_eq!(
        app.status,
        crate::startup_tasks::workspace_plugins_restricted_status()
    );
}

#[test]
fn matching_static_diagnostics_event_applies_and_finishes_request() {
    let root = PathBuf::from("workspace");
    let path = root.join("src/main.rs");
    let mut app = app_for_test(root);
    app.buffers.push(TextBuffer::from_text(
        7,
        Some(path.clone()),
        "fn main() {}\n".to_owned(),
    ));
    let version = app.buffer(7).expect("buffer").version();
    app.static_diagnostics_active_request_ids.insert(7, 1);
    app.static_diagnostics_in_flight_request_ids.insert(7, 1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::DiagnosticsComputed {
            request_id: 1,
            id: 7,
            path: path.clone(),
            version,
            diagnostics: vec![static_diagnostic(&path)],
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.diagnostics.for_path(&path).len(), 1);
    assert!(app.static_diagnostics_active_request_ids.is_empty());
    assert!(app.static_diagnostics_in_flight_request_ids.is_empty());
    assert!(app.static_diagnostics_reload_queued.is_empty());
}

#[test]
fn stale_static_diagnostics_event_is_ignored_for_newer_request() {
    let root = PathBuf::from("workspace");
    let path = root.join("src/main.rs");
    let mut app = app_for_test(root);
    app.buffers.push(TextBuffer::from_text(
        7,
        Some(path.clone()),
        "fn main() {}\n".to_owned(),
    ));
    let version = app.buffer(7).expect("buffer").version();
    app.static_diagnostics_active_request_ids.insert(7, 2);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::DiagnosticsComputed {
            request_id: 1,
            id: 7,
            path: path.clone(),
            version,
            diagnostics: vec![static_diagnostic(&path)],
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert!(app.diagnostics.for_path(&path).is_empty());
    assert_eq!(app.static_diagnostics_active_request_ids.get(&7), Some(&2));
}

#[test]
fn protected_static_diagnostics_request_invalidates_in_flight_work() {
    let root = PathBuf::from("workspace");
    let path = root.join("src/main.rs");
    let mut app = app_for_test(root);
    app.buffers.push(TextBuffer::from_text(
        7,
        Some(path.clone()),
        "fn main() {}\n".to_owned(),
    ));
    let version = app.buffer(7).expect("buffer").version();
    app.lossy_decoded_buffers.insert(7);
    app.static_diagnostics_next_request_id = 1;
    app.static_diagnostics_active_request_ids.insert(7, 1);
    app.static_diagnostics_in_flight_request_ids.insert(7, 1);
    app.static_diagnostics_reload_queued.insert(7);

    app.spawn_diagnostics_for(7);

    assert_eq!(app.static_diagnostics_next_request_id, 2);
    assert_eq!(app.static_diagnostics_active_request_ids.get(&7), Some(&2));
    assert!(app.static_diagnostics_in_flight_request_ids.is_empty());
    assert!(app.static_diagnostics_reload_queued.is_empty());

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::DiagnosticsComputed {
            request_id: 1,
            id: 7,
            path: path.clone(),
            version,
            diagnostics: vec![static_diagnostic(&path)],
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert!(app.diagnostics.for_path(&path).is_empty());
}
