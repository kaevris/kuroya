use super::common::*;

#[test]
fn workspace_apply_edit_rejects_dot_segment_workspace_escape() {
    let root = PathBuf::from("workspace");
    let escaped = root.join("..").join("outside.rs");
    let mut app = app_for_test(root.clone());
    app.lsp_clients.insert(
        "rust".to_owned(),
        LspClientHandle::disconnected_with_generation_for_test(1),
    );

    app.handle_lsp_edit_event(LspUiEvent::WorkspaceApplyEditRequest {
        language: "rust".to_owned(),
        root,
        generation: 1,
        request_id: LspRequestId::String("apply-1".to_owned()),
        label: Some("Escape".to_owned()),
        edits: Some(vec![edit(&escaped, "outside\n")]),
        document_changes: Vec::new(),
        document_versions: BTreeMap::new(),
        error: None,
    });

    assert!(app.status.contains("edit outside workspace"));
    assert!(app.status.contains("outside.rs"));
}

#[test]
fn workspace_apply_edit_rejects_parent_reentry_text_edit() {
    let root = PathBuf::from("workspace");
    let reentry = root
        .join("..")
        .join("workspace")
        .join("src")
        .join("main.rs");
    let mut app = app_for_test(root.clone());
    app.lsp_clients.insert(
        "rust".to_owned(),
        LspClientHandle::disconnected_with_generation_for_test(1),
    );

    app.handle_lsp_edit_event(LspUiEvent::WorkspaceApplyEditRequest {
        language: "rust".to_owned(),
        root,
        generation: 1,
        request_id: LspRequestId::String("apply-reentry".to_owned()),
        label: Some("Reentry".to_owned()),
        edits: Some(vec![edit(&reentry, "outside\n")]),
        document_changes: Vec::new(),
        document_versions: BTreeMap::new(),
        error: None,
    });

    assert!(app.status.contains("edit outside workspace"));
    assert!(app.status.contains("main.rs"));
}

#[test]
fn workspace_apply_edit_rejects_disk_text_edit_symlink_target_before_queueing() {
    let root = temp_workspace("text-edit-symlink-target");
    fs::create_dir_all(root.join("src")).unwrap();
    let target = root.join("src").join("target.rs");
    let link = root.join("src").join("link.rs");
    fs::write(&target, "target\n").unwrap();
    if !create_file_symlink_for_test(&target, &link) {
        fs::remove_dir_all(root).unwrap();
        return;
    }
    let mut app = app_for_test(root.clone());
    app.lsp_clients.insert(
        "rust".to_owned(),
        LspClientHandle::disconnected_with_generation_for_test(1),
    );

    app.handle_lsp_edit_event(LspUiEvent::WorkspaceApplyEditRequest {
        language: "rust".to_owned(),
        root: root.clone(),
        generation: 1,
        request_id: LspRequestId::Number(50),
        label: Some("Symlink edit".to_owned()),
        edits: Some(vec![edit(&link, "changed\n")]),
        document_changes: Vec::new(),
        document_versions: BTreeMap::new(),
        error: None,
    });

    assert!(
        app.status.contains("edit target uses symlink"),
        "{}",
        app.status
    );
    assert_workspace_status_is_display_safe(&app.status);
    assert_status_error_detail_is_bounded(&app.status);
    assert_eq!(fs::read_to_string(&target).unwrap(), "target\n");
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn workspace_apply_edit_rejects_resource_operation_workspace_escape() {
    let root = PathBuf::from("workspace");
    let path = root.join("..").join("outside.rs");
    let mut app = app_for_test(root.clone());
    app.lsp_clients.insert(
        "rust".to_owned(),
        LspClientHandle::disconnected_with_generation_for_test(1),
    );

    app.handle_lsp_edit_event(LspUiEvent::WorkspaceApplyEditRequest {
        language: "rust".to_owned(),
        root,
        generation: 1,
        request_id: LspRequestId::Number(29),
        label: Some("Create file".to_owned()),
        edits: Some(Vec::new()),
        document_changes: vec![LspWorkspaceDocumentChange::Resource(
            kuroya_core::LspWorkspaceResourceOperation::CreateFile {
                path,
                overwrite: false,
                ignore_if_exists: false,
            },
        )],
        document_versions: BTreeMap::new(),
        error: None,
    });

    assert!(app.status.contains("resource create outside workspace"));
}

#[test]
fn workspace_apply_edit_rejects_resource_operation_parent_reentry() {
    let root = PathBuf::from("workspace");
    let path = root.join("..").join("workspace").join("created.rs");
    let mut app = app_for_test(root.clone());
    app.lsp_clients.insert(
        "rust".to_owned(),
        LspClientHandle::disconnected_with_generation_for_test(1),
    );

    app.handle_lsp_edit_event(LspUiEvent::WorkspaceApplyEditRequest {
        language: "rust".to_owned(),
        root,
        generation: 1,
        request_id: LspRequestId::Number(49),
        label: Some("Create file".to_owned()),
        edits: Some(Vec::new()),
        document_changes: vec![LspWorkspaceDocumentChange::Resource(
            kuroya_core::LspWorkspaceResourceOperation::CreateFile {
                path,
                overwrite: false,
                ignore_if_exists: false,
            },
        )],
        document_versions: BTreeMap::new(),
        error: None,
    });

    assert!(app.status.contains("resource create outside workspace"));
}

#[test]
fn workspace_apply_edit_rejects_resource_delete_symlink_target_before_queueing() {
    let root = temp_workspace("resource-delete-symlink-target");
    fs::create_dir_all(root.join("src")).unwrap();
    let target = root.join("src").join("target.rs");
    let link = root.join("src").join("link.rs");
    fs::write(&target, "target\n").unwrap();
    if !create_file_symlink_for_test(&target, &link) {
        fs::remove_dir_all(root).unwrap();
        return;
    }
    let mut app = app_for_test(root.clone());
    app.lsp_clients.insert(
        "rust".to_owned(),
        LspClientHandle::disconnected_with_generation_for_test(1),
    );

    app.handle_lsp_edit_event(LspUiEvent::WorkspaceApplyEditRequest {
        language: "rust".to_owned(),
        root: root.clone(),
        generation: 1,
        request_id: LspRequestId::Number(51),
        label: Some("Delete symlink".to_owned()),
        edits: Some(Vec::new()),
        document_changes: vec![LspWorkspaceDocumentChange::Resource(
            kuroya_core::LspWorkspaceResourceOperation::DeleteFile {
                path: link.clone(),
                recursive: false,
                ignore_if_not_exists: false,
            },
        )],
        document_versions: BTreeMap::new(),
        error: None,
    });

    assert!(
        app.status.contains("resource delete target uses symlink"),
        "{}",
        app.status
    );
    assert_workspace_status_is_display_safe(&app.status);
    assert_status_error_detail_is_bounded(&app.status);
    assert_eq!(fs::read_to_string(&target).unwrap(), "target\n");
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn workspace_apply_edit_rejects_resource_operations_before_text_mutation() {
    let root = PathBuf::from("workspace");
    let path = root.join("src/main.rs");
    let resource_path = root.join("src/new.rs");
    let mut app = app_for_test(root.clone());
    app.lsp_clients.insert(
        "rust".to_owned(),
        LspClientHandle::disconnected_with_generation_for_test(1),
    );
    app.buffers.push(TextBuffer::from_text(
        8,
        Some(path.clone()),
        "fn main() {}\n".to_owned(),
    ));

    app.handle_lsp_edit_event(LspUiEvent::WorkspaceApplyEditRequest {
        language: "rust".to_owned(),
        root,
        generation: 1,
        request_id: LspRequestId::Number(30),
        label: Some("Mixed edit".to_owned()),
        edits: Some(vec![edit(&path, "mod changed;\n")]),
        document_changes: vec![LspWorkspaceDocumentChange::Resource(
            kuroya_core::LspWorkspaceResourceOperation::CreateFile {
                path: resource_path,
                overwrite: false,
                ignore_if_exists: false,
            },
        )],
        document_versions: BTreeMap::new(),
        error: None,
    });

    assert_eq!(app.buffer(8).expect("buffer").text(), "fn main() {}\n");
    assert!(
        app.status
            .contains("top-level text changes cannot be combined with resource operations yet")
    );
}

#[test]
fn workspace_apply_edit_rejects_json_resource_escape_before_text_mutation() {
    for case in ["create", "rename-old", "rename-new", "delete"] {
        let root = temp_workspace(&format!("json-resource-escape-{case}"));
        fs::create_dir_all(root.join("src")).unwrap();
        let root =
            crate::native_paths::normalize_native_path(fs::canonicalize(&root).unwrap_or(root));
        let path = root.join("src/main.rs");
        fs::write(&path, "fn main() {}\n").unwrap();
        let outside_name = format!("{}-outside.rs", root.file_name().unwrap().to_string_lossy());
        let escaped_path = root.join("..").join(&outside_name);
        let escaped_uri = path_to_file_uri(&escaped_path);
        assert!(escaped_uri.contains("/../"), "{escaped_uri}");
        let resolved_outside = root.parent().unwrap().join(outside_name);
        let mut app = app_for_test(root.clone());
        app.lsp_clients.insert(
            "rust".to_owned(),
            LspClientHandle::disconnected_with_generation_for_test(1),
        );
        app.buffers.push(TextBuffer::from_text(
            8,
            Some(path.clone()),
            "fn main() {}\n".to_owned(),
        ));
        let resource_change = match case {
            "create" => json!({
                "kind": "create",
                "uri": escaped_uri
            }),
            "rename-old" => json!({
                "kind": "rename",
                "oldUri": escaped_uri,
                "newUri": path_to_file_uri(&root.join("src/renamed.rs"))
            }),
            "rename-new" => json!({
                "kind": "rename",
                "oldUri": path_to_file_uri(&path),
                "newUri": escaped_uri
            }),
            "delete" => json!({
                "kind": "delete",
                "uri": escaped_uri
            }),
            _ => unreachable!(),
        };
        let expected_status = match case {
            "create" => "resource create outside workspace",
            "rename-old" | "rename-new" => "resource rename outside workspace",
            "delete" => "resource delete outside workspace",
            _ => unreachable!(),
        };
        let value = json!({
            "jsonrpc": "2.0",
            "id": 48,
            "method": "workspace/applyEdit",
            "params": {
                "label": "Escaping resource operation",
                "edit": {
                    "documentChanges": [
                        resource_change,
                        {
                            "textDocument": {
                                "uri": path_to_file_uri(&path),
                                "version": null
                            },
                            "edits": [{
                                "range": {
                                    "start": { "line": 0, "character": 0 },
                                    "end": { "line": 0, "character": 0 }
                                },
                                "newText": "mod changed;\n"
                            }]
                        }
                    ]
                }
            }
        });
        let request = parse_apply_workspace_edit_request(&value).unwrap();

        app.handle_lsp_edit_event(LspUiEvent::WorkspaceApplyEditRequest {
            language: "rust".to_owned(),
            root: root.clone(),
            generation: 1,
            request_id: request.id,
            label: request.label,
            edits: Some(request.edits),
            document_changes: request.document_changes,
            document_versions: request.document_versions,
            error: None,
        });

        assert_eq!(app.buffer(8).expect("buffer").text(), "fn main() {}\n");
        assert_eq!(fs::read_to_string(&path).unwrap(), "fn main() {}\n");
        assert!(!resolved_outside.exists());
        assert!(app.status.contains(expected_status), "{}", app.status);
        assert_workspace_status_is_display_safe(&app.status);
        assert_status_error_detail_is_bounded(&app.status);
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn workspace_apply_edit_resource_preflight_failure_does_not_mutate_disk() {
    let root = temp_workspace("resource-preflight");
    fs::create_dir_all(&root).unwrap();
    let created = root.join("created.rs");
    let missing = root.join("missing.rs");
    let mut app = app_for_test(root.clone());
    app.lsp_clients.insert(
        "rust".to_owned(),
        LspClientHandle::disconnected_with_generation_for_test(1),
    );

    app.handle_lsp_edit_event(LspUiEvent::WorkspaceApplyEditRequest {
        language: "rust".to_owned(),
        root: root.clone(),
        generation: 1,
        request_id: LspRequestId::Number(33),
        label: Some("Create then fail".to_owned()),
        edits: Some(Vec::new()),
        document_changes: vec![
            LspWorkspaceDocumentChange::Resource(
                kuroya_core::LspWorkspaceResourceOperation::CreateFile {
                    path: created.clone(),
                    overwrite: false,
                    ignore_if_exists: false,
                },
            ),
            LspWorkspaceDocumentChange::Resource(
                kuroya_core::LspWorkspaceResourceOperation::DeleteFile {
                    path: missing,
                    recursive: false,
                    ignore_if_not_exists: false,
                },
            ),
        ],
        document_versions: BTreeMap::new(),
        error: None,
    });

    assert!(
        app.status.contains("resource delete target does not exist"),
        "{}",
        app.status
    );
    assert!(!created.exists());
    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn workspace_apply_edit_rejects_resource_file_parent_before_mutating_disk() {
    let root = temp_workspace("resource-parent-file");
    fs::create_dir_all(&root).unwrap();
    let parent_file = root.join("src");
    let child_path = parent_file.join("child.rs");
    let old_path = root.join("old.rs");
    fs::write(&parent_file, "parent\n").unwrap();
    fs::write(&old_path, "old\n").unwrap();
    let mut app = app_for_test(root.clone());
    app.lsp_clients.insert(
        "rust".to_owned(),
        LspClientHandle::disconnected_with_generation_for_test(1),
    );

    app.handle_lsp_edit_event(LspUiEvent::WorkspaceApplyEditRequest {
        language: "rust".to_owned(),
        root: root.clone(),
        generation: 1,
        request_id: LspRequestId::Number(47),
        label: Some("Create under file".to_owned()),
        edits: Some(Vec::new()),
        document_changes: vec![LspWorkspaceDocumentChange::Resource(
            kuroya_core::LspWorkspaceResourceOperation::CreateFile {
                path: child_path.clone(),
                overwrite: false,
                ignore_if_exists: false,
            },
        )],
        document_versions: BTreeMap::new(),
        error: None,
    });

    assert!(
        app.status.contains("resource create parent is a file"),
        "{}",
        app.status
    );
    assert!(!child_path.exists());
    assert_eq!(fs::read_to_string(&parent_file).unwrap(), "parent\n");

    app.handle_lsp_edit_event(LspUiEvent::WorkspaceApplyEditRequest {
        language: "rust".to_owned(),
        root: root.clone(),
        generation: 1,
        request_id: LspRequestId::Number(48),
        label: Some("Rename under file".to_owned()),
        edits: Some(Vec::new()),
        document_changes: vec![LspWorkspaceDocumentChange::Resource(
            kuroya_core::LspWorkspaceResourceOperation::RenameFile {
                old_path: old_path.clone(),
                new_path: child_path.clone(),
                overwrite: false,
                ignore_if_exists: false,
            },
        )],
        document_versions: BTreeMap::new(),
        error: None,
    });

    assert!(
        app.status.contains("resource rename parent is a file"),
        "{}",
        app.status
    );
    assert_eq!(fs::read_to_string(&old_path).unwrap(), "old\n");
    assert!(!child_path.exists());
    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn simulated_workspace_resource_state_reuses_cached_disk_path_state() {
    let root = temp_workspace("resource-state-cache");
    fs::create_dir_all(root.join("src")).unwrap();
    let path = root.join("src").join("cached.rs");
    fs::write(&path, "cached\n").unwrap();
    let mut state = SimulatedWorkspaceResourceState::default();

    assert!(matches!(
        state.path_state(&path).unwrap(),
        SimulatedWorkspacePath::File(SimulatedWorkspaceFile::Disk(_))
    ));
    assert_eq!(state.disk_paths.len(), 1);

    fs::remove_file(&path).unwrap();
    assert!(matches!(
        state.path_state(&path).unwrap(),
        SimulatedWorkspacePath::File(SimulatedWorkspaceFile::Disk(_))
    ));
    assert_eq!(state.disk_paths.len(), 1);

    state.set_path_state(&path, SimulatedWorkspacePath::Missing);
    assert!(matches!(
        state.path_state(&path).unwrap(),
        SimulatedWorkspacePath::Missing
    ));
    assert_eq!(state.disk_paths.len(), 1);
    fs::remove_dir_all(root).unwrap();
}
