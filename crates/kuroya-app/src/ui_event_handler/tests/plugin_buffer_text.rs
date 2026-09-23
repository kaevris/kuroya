use super::*;

fn plugin_buffer_text_test_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("kuroya-plugin-buffer-text-{name}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("test workspace root should be created");
    root
}

fn plugin_buffer_text_apply_event(path: PathBuf, text: &str) -> UiEvent {
    UiEvent::PluginBufferTextApply {
        plugin_id: "example.plugin".to_owned(),
        path,
        text: text.to_owned(),
    }
}

#[test]
fn plugin_buffer_text_apply_replaces_open_buffer_text_and_invalidates_caches() {
    let root = plugin_buffer_text_test_root("apply");
    let file = root.join("notes.md");
    let mut app = app_for_test(root.clone());
    app.buffers.push(TextBuffer::from_text(
        1,
        Some(file.clone()),
        "old text\n{\n".to_owned(),
    ));
    app.set_active_buffer(1);
    let version_before = app.buffer(1).expect("open buffer").version();
    let buffer = app.buffer(1).expect("open buffer").clone();
    app.editor_bracket_overlay_cache
        .bracket_pair_guides(&buffer);
    app.minimap_line_length_cache
        .sampled_lengths_for(&buffer, 1, 80, true);
    assert!(app.editor_bracket_overlay_cache.contains_buffer_for_test(1));
    assert!(app.minimap_line_length_cache.contains_buffer_for_test(1));

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        plugin_buffer_text_apply_event(file.clone(), "plugin text")
    ));

    assert_eq!(app.handle_events(), 1);
    let buffer = app.buffer(1).expect("open buffer");
    assert_eq!(buffer.text_snapshot().text(), "plugin text");
    assert!(buffer.version() > version_before);
    assert!(!app.editor_bracket_overlay_cache.contains_buffer_for_test(1));
    assert!(!app.minimap_line_length_cache.contains_buffer_for_test(1));
    assert_eq!(app.status, "Plugin updated notes.md");

    // the replacement lands in undo history like a manual edit
    assert!(app.buffer_mut(1).expect("open buffer").undo());
    assert_eq!(
        app.buffer(1).expect("open buffer").text_snapshot().text(),
        "old text\n{\n"
    );

    drop(app);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn plugin_buffer_text_apply_skips_when_buffer_is_not_open() {
    let root = plugin_buffer_text_test_root("skip");
    let file = root.join("missing.md");
    let mut app = app_for_test(root.clone());
    app.status = "before".to_owned();

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        plugin_buffer_text_apply_event(file, "plugin text")
    ));

    assert_eq!(app.handle_events(), 1);
    assert!(app.buffers.is_empty());
    assert_eq!(app.status, "Plugin changes skipped: missing.md is not open");

    drop(app);
    let _ = std::fs::remove_dir_all(root);
}
