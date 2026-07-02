use super::*;
use crate::{
    persistence_models::PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
    persistence_session::PERSISTED_SESSION_MAX_BYTES,
    persistence_workspace_snapshots::{MAX_WORKSPACE_SNAPSHOTS, workspace_snapshot_files},
};

#[test]
fn workspace_snapshot_save_and_load_latest_round_trip() {
    let workspace = temp_workspace("workspace-snapshot-round-trip");
    fs::create_dir_all(&workspace).unwrap();

    let first = sample_session(&workspace, "first workspace snapshot");
    let second = sample_session(&workspace, "second workspace snapshot");
    save_workspace_snapshot(&workspace, &first).unwrap();
    let latest_path = save_workspace_snapshot(&workspace, &second).unwrap();

    let loaded = load_latest_workspace_snapshot(&workspace).unwrap().unwrap();
    assert_eq!(loaded.path, latest_path);
    assert_eq!(loaded.session, second);
    assert_eq!(workspace_snapshot_files(&workspace).unwrap().len(), 2);
    assert_no_session_temps(&workspace);

    remove_workspace(&workspace);
}

#[test]
fn workspace_snapshot_save_reuses_duplicate_latest_snapshot() {
    let workspace = temp_workspace("workspace-snapshot-duplicate-latest");
    fs::create_dir_all(&workspace).unwrap();

    let first = sample_session(&workspace, "same workspace snapshot");
    let first_path = save_workspace_snapshot(&workspace, &first).unwrap();
    let duplicate_path = save_workspace_snapshot(&workspace, &first).unwrap();

    assert_eq!(duplicate_path, first_path);
    assert_eq!(
        workspace_snapshot_files(&workspace).unwrap(),
        vec![first_path]
    );

    let changed = sample_session(&workspace, "changed workspace snapshot");
    let changed_path = save_workspace_snapshot(&workspace, &changed).unwrap();

    assert_ne!(changed_path, duplicate_path);
    assert_eq!(workspace_snapshot_files(&workspace).unwrap().len(), 2);
    let loaded = load_latest_workspace_snapshot(&workspace).unwrap().unwrap();
    assert_eq!(loaded.path, changed_path);
    assert_eq!(loaded.session, changed);

    remove_workspace(&workspace);
}

#[test]
fn workspace_snapshot_save_rejects_mismatched_workspace_root() {
    let workspace = temp_workspace("workspace-snapshot-save-mismatch");
    let other_workspace = temp_workspace("workspace-snapshot-save-mismatch-other");
    fs::create_dir_all(&workspace).unwrap();

    let mismatched = sample_session(&other_workspace, "wrong workspace snapshot");
    let error = save_workspace_snapshot(&workspace, &mismatched)
        .expect_err("mismatched snapshot should be rejected");

    assert!(
        error
            .to_string()
            .contains("workspace snapshot session root does not match workspace root")
    );
    assert!(workspace_snapshot_files(&workspace).unwrap().is_empty());

    remove_workspace(&workspace);
}

#[test]
fn workspace_snapshot_load_skips_corrupt_latest_snapshot() {
    let workspace = temp_workspace("workspace-snapshot-corrupt");
    fs::create_dir_all(&workspace).unwrap();

    let valid = sample_session(&workspace, "valid workspace snapshot");
    save_workspace_snapshot(&workspace, &valid).unwrap();
    let snapshot_dir = crate::persistence_storage::workspace_snapshots_dir(&workspace);
    fs::write(
        snapshot_dir.join("workspace.999999999999999999999999999999.0.0000000000000000.json"),
        "{not valid json",
    )
    .unwrap();

    let loaded = load_latest_workspace_snapshot(&workspace).unwrap().unwrap();
    assert_eq!(loaded.session, valid);
    assert_eq!(workspace_snapshot_files(&workspace).unwrap().len(), 1);

    let quarantined = fs::read_dir(snapshot_dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.contains(".corrupt."))
        })
        .count();
    assert_eq!(quarantined, 1);

    remove_workspace(&workspace);
}

#[test]
fn workspace_snapshot_load_skips_mismatched_latest_snapshot() {
    let workspace = temp_workspace("workspace-snapshot-mismatched");
    let other_workspace = temp_workspace("workspace-snapshot-mismatched-other");
    fs::create_dir_all(&workspace).unwrap();

    let valid = sample_session(&workspace, "valid workspace snapshot");
    let valid_path = save_workspace_snapshot(&workspace, &valid).unwrap();
    let mismatched = sample_session(&other_workspace, "wrong workspace snapshot");
    let snapshot_dir = crate::persistence_storage::workspace_snapshots_dir(&workspace);
    fs::write(
        snapshot_dir.join("workspace.999999999999999999999999999999.0.0000000000000000.json"),
        serde_json::to_string_pretty(&mismatched).unwrap(),
    )
    .unwrap();

    let loaded = load_latest_workspace_snapshot(&workspace).unwrap().unwrap();
    assert_eq!(loaded.path, valid_path);
    assert_eq!(loaded.session, valid);
    assert_eq!(
        workspace_snapshot_files(&workspace).unwrap(),
        vec![valid_path]
    );

    let quarantined = fs::read_dir(snapshot_dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.contains(".mismatched."))
        })
        .count();
    assert_eq!(quarantined, 1);

    remove_workspace(&workspace);
}

#[test]
fn workspace_snapshot_load_ignores_snapshot_named_directories() {
    let workspace = temp_workspace("workspace-snapshot-directory");
    fs::create_dir_all(&workspace).unwrap();

    let valid = sample_session(&workspace, "valid workspace snapshot");
    let valid_path = save_workspace_snapshot(&workspace, &valid).unwrap();
    let snapshot_dir = crate::persistence_storage::workspace_snapshots_dir(&workspace);
    fs::create_dir_all(
        snapshot_dir.join("workspace.999999999999999999999999999999.0.0000000000000000.json"),
    )
    .unwrap();

    let loaded = load_latest_workspace_snapshot(&workspace).unwrap().unwrap();
    assert_eq!(loaded.path, valid_path);
    assert_eq!(loaded.session, valid);
    assert_eq!(
        workspace_snapshot_files(&workspace).unwrap(),
        vec![valid_path]
    );

    remove_workspace(&workspace);
}

#[test]
fn workspace_snapshot_load_prefers_generated_names_over_unparsed_backups() {
    let workspace = temp_workspace("workspace-snapshot-generated-before-unparsed");
    fs::create_dir_all(&workspace).unwrap();

    let newer = sample_session(&workspace, "new generated workspace snapshot");
    let newer_path = save_workspace_snapshot(&workspace, &newer).unwrap();
    let old = sample_session(&workspace, "old unparsed workspace snapshot");
    let snapshot_dir = crate::persistence_storage::workspace_snapshots_dir(&workspace);
    fs::write(
        snapshot_dir.join("workspace.zzz.json"),
        serde_json::to_string_pretty(&old).unwrap(),
    )
    .unwrap();

    let loaded = load_latest_workspace_snapshot(&workspace).unwrap().unwrap();
    assert_eq!(loaded.path, newer_path);
    assert_eq!(loaded.session, newer);

    remove_workspace(&workspace);
}

#[test]
fn workspace_snapshot_load_normalizes_restored_paths() {
    let workspace = temp_workspace("workspace-snapshot-normalized-paths");
    fs::create_dir_all(&workspace).unwrap();

    let messy_main = workspace.join("src").join("..").join("src").join("main.rs");
    let main = workspace.join("src").join("main.rs");
    let outside = workspace.join("..").join("outside.rs");
    let mut session = sample_session(&workspace, "inside workspace snapshot");
    session.open_files = vec![messy_main.clone(), main.clone(), outside.clone()];
    session.active_path = Some(messy_main.clone());
    session.pane_paths = vec![Some(messy_main.clone()), Some(outside.clone())];
    session.recovery = vec![
        RecoveredBuffer {
            path: Some(messy_main),
            display_name: "main.rs".to_owned(),
            text: "inside workspace snapshot".to_owned(),
        },
        RecoveredBuffer {
            path: Some(outside),
            display_name: "outside.rs".to_owned(),
            text: "outside workspace snapshot text".to_owned(),
        },
    ];

    let snapshot_path = save_workspace_snapshot(&workspace, &session).unwrap();
    let loaded = load_latest_workspace_snapshot(&workspace).unwrap().unwrap();

    assert_eq!(loaded.path, snapshot_path);
    assert_eq!(loaded.session.open_files, vec![main.clone()]);
    assert_eq!(loaded.session.active_path, Some(main.clone()));
    assert_eq!(loaded.session.pane_paths, vec![Some(main.clone()), None]);
    assert_eq!(loaded.session.recovery[0].path, Some(main));
    assert_eq!(loaded.session.recovery[1].path, None);
    assert_eq!(
        loaded.session.recovery[1].text,
        "outside workspace snapshot text"
    );

    remove_workspace(&workspace);
}

#[test]
fn workspace_snapshots_are_bounded_to_recent_entries() {
    let workspace = temp_workspace("workspace-snapshot-bounded");
    fs::create_dir_all(&workspace).unwrap();

    for index in 0..(MAX_WORKSPACE_SNAPSHOTS + 4) {
        save_workspace_snapshot(
            &workspace,
            &sample_session(&workspace, &format!("workspace snapshot {index}")),
        )
        .unwrap();
    }

    let snapshots = workspace_snapshot_files(&workspace).unwrap();
    assert_eq!(snapshots.len(), MAX_WORKSPACE_SNAPSHOTS);
    let snapshot_texts = snapshots
        .iter()
        .map(|path| {
            serde_json::from_str::<PersistedSession>(&fs::read_to_string(path).unwrap())
                .unwrap()
                .recovery[0]
                .text
                .clone()
        })
        .collect::<Vec<_>>();

    assert!(!snapshot_texts.contains(&"workspace snapshot 0".to_owned()));
    assert!(!snapshot_texts.contains(&"workspace snapshot 1".to_owned()));
    assert!(snapshot_texts.contains(&format!(
        "workspace snapshot {}",
        MAX_WORKSPACE_SNAPSHOTS + 3
    )));

    remove_workspace(&workspace);
}

#[test]
fn workspace_snapshot_save_trims_volatile_state_to_file_limit() {
    let workspace = temp_workspace("workspace-snapshot-trims-volatile-state");
    fs::create_dir_all(&workspace).unwrap();

    let mut session = sample_session(&workspace, "small recovered buffer");
    session.terminal_sessions[0].scrollback =
        "x".repeat(usize::try_from(PERSISTED_SESSION_MAX_BYTES).unwrap() + 1024);

    let snapshot_path = save_workspace_snapshot(&workspace, &session).unwrap();

    assert!(fs::metadata(&snapshot_path).unwrap().len() <= PERSISTED_SESSION_MAX_BYTES);
    let loaded = load_latest_workspace_snapshot(&workspace).unwrap().unwrap();
    assert_eq!(loaded.path, snapshot_path);
    assert_eq!(loaded.session.terminal_sessions[0].scrollback, "");
    assert_eq!(loaded.session.recovery[0].text, "small recovered buffer");
    assert_eq!(workspace_snapshot_files(&workspace).unwrap().len(), 1);

    remove_workspace(&workspace);
}

#[test]
fn workspace_snapshot_save_trims_volatile_text_to_file_limit() {
    let workspace = temp_workspace("workspace-snapshot-trims-volatile-text");
    fs::create_dir_all(&workspace).unwrap();

    let mut session = sample_session(&workspace, "small recovered buffer");
    session.project_search_query =
        "x".repeat(usize::try_from(PERSISTED_SESSION_MAX_BYTES).unwrap() + 1024);

    let snapshot_path = save_workspace_snapshot(&workspace, &session).unwrap();

    assert!(fs::metadata(&snapshot_path).unwrap().len() <= PERSISTED_SESSION_MAX_BYTES);
    let loaded = load_latest_workspace_snapshot(&workspace).unwrap().unwrap();
    assert_eq!(loaded.path, snapshot_path);
    assert_eq!(loaded.session.recovery[0].text, "small recovered buffer");
    assert_eq!(
        loaded.session.project_search_query.chars().count(),
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS
    );
    assert!(loaded.session.project_search_recent.is_empty());
    assert_eq!(workspace_snapshot_files(&workspace).unwrap().len(), 1);

    remove_workspace(&workspace);
}

#[test]
fn workspace_snapshot_records_skipped_recovery_instead_of_writing_unloadable_snapshot() {
    let workspace = temp_workspace("workspace-snapshot-trims-recovery");
    fs::create_dir_all(&workspace).unwrap();

    let mut session = sample_session(&workspace, "small recovered buffer");
    session.history_states = vec![BufferHistoryState {
        path: workspace.join("src/main.rs"),
        history: kuroya_core::BufferHistorySnapshot {
            len_chars: 3,
            checksum: 42,
            undo: Vec::new(),
            redo: Vec::new(),
        },
    }];
    session.recovery = vec![RecoveredBuffer {
        path: Some(workspace.join("src/large.rs")),
        display_name: "large.rs".to_owned(),
        text: "x".repeat(usize::try_from(PERSISTED_SESSION_MAX_BYTES).unwrap() + 1024),
    }];

    let snapshot_path = save_workspace_snapshot(&workspace, &session).unwrap();

    assert!(fs::metadata(&snapshot_path).unwrap().len() <= PERSISTED_SESSION_MAX_BYTES);
    let loaded = load_latest_workspace_snapshot(&workspace).unwrap().unwrap();
    assert_eq!(loaded.path, snapshot_path);
    assert!(loaded.session.recovery.is_empty());
    assert_eq!(loaded.session.history_states.len(), 1);
    assert_eq!(
        loaded.session.history_states[0].path,
        workspace.join("src/main.rs")
    );
    assert_eq!(loaded.session.recovery_skipped.len(), 1);
    assert_eq!(loaded.session.recovery_skipped[0].display_name, "large.rs");
    assert!(
        loaded.session.recovery_skipped[0]
            .reason
            .contains("omitted to keep session file under")
    );

    remove_workspace(&workspace);
}
