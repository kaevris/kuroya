use super::*;

fn plugin_open_file_test_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("kuroya-plugin-open-file-{name}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("test workspace root should be created");
    root
}

fn plugin_open_file_event(plugin_id: &str, path: PathBuf) -> UiEvent {
    UiEvent::PluginOpenFileRequested {
        plugin_id: plugin_id.to_owned(),
        path,
    }
}

#[test]
fn plugin_open_file_request_for_existing_file_inside_root_opens_it() {
    let root = plugin_open_file_test_root("valid");
    let file = root.join("notes.md");
    std::fs::write(&file, b"# notes").expect("notes file should be written");
    let mut app = app_for_test(root.clone());

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        plugin_open_file_event("example.plugin", file.clone())
    ));

    assert!(app.handle_events() >= 1);
    assert!(
        app.pending_open_paths.contains(&file)
            || app
                .buffers
                .iter()
                .any(|buffer| buffer.path() == Some(&file))
    );

    drop(app);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn plugin_open_file_request_for_missing_path_sets_failure_status_without_opening() {
    let root = plugin_open_file_test_root("missing");
    let file = root.join("missing.md");
    let mut app = app_for_test(root.clone());
    app.status = "before".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        plugin_open_file_event("example.plugin", file.clone())
    ));

    assert_eq!(app.handle_events(), 1);
    assert!(!app.pending_open_paths.contains(&file));
    assert!(app.buffers.is_empty());
    assert_eq!(
        app.status,
        "Plugin example.plugin could not open missing.md"
    );

    drop(app);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn plugin_open_file_request_escaping_workspace_root_is_rejected() {
    let root = plugin_open_file_test_root("escape");
    let parent = root.parent().expect("temp root should have a parent");
    let outside = parent.join("kuroya-plugin-open-file-escape-outside.md");
    std::fs::write(&outside, b"outside").expect("outside file should be written");
    let escaped = root
        .join("..")
        .join(outside.file_name().expect("file name"));
    let mut app = app_for_test(root.clone());
    app.status = "before".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        plugin_open_file_event("example.plugin", escaped.clone())
    ));

    assert_eq!(app.handle_events(), 1);
    assert!(!app.pending_open_paths.contains(&escaped));
    assert!(app.buffers.is_empty());
    assert_eq!(
        app.status,
        "Plugin example.plugin could not open kuroya-plugin-open-file-escape-outside.md"
    );

    let _ = std::fs::remove_file(outside);
    drop(app);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn placeholder_workspace_plugin_open_request_is_dropped_with_no_folder_status() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.join("notes.md"));
    app.workspace_placeholder = true;
    app.status = "before".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        plugin_open_file_event("example.plugin", root.join("notes.md"))
    ));

    assert_eq!(app.handle_events(), 1);
    assert!(app.pending_open_paths.is_empty());
    assert!(app.buffers.is_empty());
    assert_eq!(app.status, "No folder open");
}
