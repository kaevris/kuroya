use super::*;

#[test]
fn load_session_quarantines_corrupt_snapshot_and_starts_clean() {
    let workspace = temp_workspace("corrupt");
    let state = state_dir(&workspace);
    fs::create_dir_all(&state).unwrap();
    let session = state.join("session.json");
    fs::write(&session, "{not valid json").unwrap();

    assert_eq!(PersistedSession::load(&workspace).unwrap(), None);
    assert!(!session.exists());

    let quarantined = fs::read_dir(&state)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with("session.json.corrupt."))
        })
        .collect::<Vec<_>>();
    assert_eq!(quarantined.len(), 1);
    assert_eq!(
        fs::read_to_string(quarantined[0].path()).unwrap(),
        "{not valid json"
    );

    let clean = sample_session(&workspace, "clean recovery");
    save_session(&workspace, &clean).unwrap();
    assert_eq!(PersistedSession::load(&workspace).unwrap(), Some(clean));

    remove_workspace(&workspace);
}

#[test]
fn load_session_restores_latest_valid_backup_when_current_session_is_corrupt() {
    let workspace = temp_workspace("corrupt-with-backup");
    fs::create_dir_all(&workspace).unwrap();

    let first = sample_session(&workspace, "first recovery");
    let second = sample_session(&workspace, "second recovery");
    let third = sample_session(&workspace, "third recovery");
    save_session(&workspace, &first).unwrap();
    save_session(&workspace, &second).unwrap();
    save_session(&workspace, &third).unwrap();
    assert_eq!(session_snapshot_files_for_test(&workspace).len(), 2);

    let state = state_dir(&workspace);
    let session = state.join("session.json");
    fs::write(&session, "{not valid json").unwrap();

    assert_eq!(PersistedSession::load(&workspace).unwrap(), Some(second));
    assert!(!session.exists());
    assert_eq!(quarantined_session_files(&state).len(), 1);

    remove_workspace(&workspace);
}

#[test]
fn load_session_quarantines_non_file_current_session_and_restores_backup() {
    let workspace = temp_workspace("directory-session-with-backup");
    fs::create_dir_all(&workspace).unwrap();

    let first = sample_session(&workspace, "first recovery");
    let second = sample_session(&workspace, "second recovery");
    let third = sample_session(&workspace, "third recovery");
    save_session(&workspace, &first).unwrap();
    save_session(&workspace, &second).unwrap();
    save_session(&workspace, &third).unwrap();
    assert_eq!(session_snapshot_files_for_test(&workspace).len(), 2);

    let state = state_dir(&workspace);
    let session = state.join("session.json");
    fs::remove_file(&session).unwrap();
    fs::create_dir(&session).unwrap();

    assert_eq!(PersistedSession::load(&workspace).unwrap(), Some(second));
    assert!(!session.exists());
    let quarantined = quarantined_session_files(&state);
    assert_eq!(quarantined.len(), 1);
    assert!(quarantined[0].is_dir());

    remove_workspace(&workspace);
}

#[test]
fn load_session_restores_latest_valid_backup_when_current_session_is_missing() {
    let workspace = temp_workspace("missing-with-backup");
    fs::create_dir_all(&workspace).unwrap();

    let first = sample_session(&workspace, "first recovery");
    let second = sample_session(&workspace, "second recovery");
    let third = sample_session(&workspace, "third recovery");
    save_session(&workspace, &first).unwrap();
    save_session(&workspace, &second).unwrap();
    save_session(&workspace, &third).unwrap();
    assert_eq!(session_snapshot_files_for_test(&workspace).len(), 2);

    let session = state_dir(&workspace).join("session.json");
    fs::remove_file(&session).unwrap();

    assert_eq!(PersistedSession::load(&workspace).unwrap(), Some(second));
    assert!(!session.exists());

    remove_workspace(&workspace);
}

#[test]
fn load_session_quarantines_mismatched_current_session_and_restores_matching_backup() {
    let workspace = temp_workspace("mismatched-current");
    let other_workspace = temp_workspace("mismatched-current-other");
    fs::create_dir_all(&workspace).unwrap();

    let first = sample_session(&workspace, "first recovery");
    let second = sample_session(&workspace, "second recovery");
    save_session(&workspace, &first).unwrap();
    save_session(&workspace, &second).unwrap();

    let mismatched = sample_session(&other_workspace, "wrong workspace recovery");
    let state = state_dir(&workspace);
    let session = state.join("session.json");
    fs::write(&session, serde_json::to_string_pretty(&mismatched).unwrap()).unwrap();

    assert_eq!(PersistedSession::load(&workspace).unwrap(), Some(first));
    assert!(!session.exists());
    assert_eq!(
        quarantined_session_files_with_marker(&state, "mismatched").len(),
        1
    );

    remove_workspace(&workspace);
}

#[test]
fn load_session_skips_corrupt_backup_and_restores_older_valid_backup() {
    let workspace = temp_workspace("corrupt-backup");
    fs::create_dir_all(&workspace).unwrap();

    let first = sample_session(&workspace, "first recovery");
    let second = sample_session(&workspace, "second recovery");
    save_session(&workspace, &first).unwrap();
    save_session(&workspace, &second).unwrap();

    let snapshots = crate::persistence_storage::session_snapshots_dir(&workspace);
    fs::write(
        snapshots.join("session.999999999999999999999999999999.0.0000000000000000.json"),
        "{bad backup",
    )
    .unwrap();
    let state = state_dir(&workspace);
    let session = state.join("session.json");
    fs::write(&session, "{not valid json").unwrap();

    assert_eq!(PersistedSession::load(&workspace).unwrap(), Some(first));
    assert!(!session.exists());
    assert_eq!(quarantined_session_files(&state).len(), 1);
    assert_eq!(quarantined_session_files(&snapshots).len(), 1);

    remove_workspace(&workspace);
}

#[test]
fn load_session_skips_mismatched_backup_and_restores_older_valid_backup() {
    let workspace = temp_workspace("mismatched-backup");
    let other_workspace = temp_workspace("mismatched-backup-other");
    fs::create_dir_all(&workspace).unwrap();

    let first = sample_session(&workspace, "first recovery");
    let second = sample_session(&workspace, "second recovery");
    save_session(&workspace, &first).unwrap();
    save_session(&workspace, &second).unwrap();

    let snapshots = crate::persistence_storage::session_snapshots_dir(&workspace);
    let mismatched = sample_session(&other_workspace, "wrong workspace backup");
    fs::write(
        snapshots.join("session.999999999999999999999999999999.0.0000000000000000.json"),
        serde_json::to_string_pretty(&mismatched).unwrap(),
    )
    .unwrap();
    let state = state_dir(&workspace);
    let session = state.join("session.json");
    fs::write(&session, "{not valid json").unwrap();

    assert_eq!(PersistedSession::load(&workspace).unwrap(), Some(first));
    assert!(!session.exists());
    assert_eq!(quarantined_session_files(&state).len(), 1);
    assert_eq!(
        quarantined_session_files_with_marker(&snapshots, "mismatched").len(),
        1
    );

    remove_workspace(&workspace);
}

#[test]
fn load_session_ignores_snapshot_named_backup_directories() {
    let workspace = temp_workspace("backup-directory");
    fs::create_dir_all(&workspace).unwrap();

    let first = sample_session(&workspace, "first recovery");
    let second = sample_session(&workspace, "second recovery");
    save_session(&workspace, &first).unwrap();
    save_session(&workspace, &second).unwrap();

    let snapshots = crate::persistence_storage::session_snapshots_dir(&workspace);
    fs::create_dir_all(snapshots.join("session.zzz.json")).unwrap();
    let state = state_dir(&workspace);
    let session = state.join("session.json");
    fs::write(&session, "{not valid json").unwrap();

    assert_eq!(PersistedSession::load(&workspace).unwrap(), Some(first));
    assert!(!session.exists());
    assert_eq!(quarantined_session_files(&state).len(), 1);
    assert!(snapshots.join("session.zzz.json").is_dir());

    remove_workspace(&workspace);
}

#[test]
fn load_session_prefers_parsed_snapshot_names_over_unparsed_backups() {
    let workspace = temp_workspace("parsed-before-unparsed");
    let state = state_dir(&workspace);
    let snapshots = crate::persistence_storage::session_snapshots_dir(&workspace);
    fs::create_dir_all(&snapshots).unwrap();

    let old = sample_session(&workspace, "old unparsed recovery");
    let newer = sample_session(&workspace, "new parsed recovery");
    fs::write(
        snapshots.join("session.old.json"),
        serde_json::to_string_pretty(&old).unwrap(),
    )
    .unwrap();
    fs::write(
        snapshots.join("session.10.7.2.json"),
        serde_json::to_string_pretty(&newer).unwrap(),
    )
    .unwrap();
    fs::write(state.join("session.json"), "{not valid json").unwrap();

    assert_eq!(PersistedSession::load(&workspace).unwrap(), Some(newer));

    remove_workspace(&workspace);
}

#[test]
fn load_session_bounds_malformed_snapshot_scan_and_restores_generated_snapshot() {
    let workspace = temp_workspace("malformed-snapshot-scan-bound");
    let snapshots = crate::persistence_storage::session_snapshots_dir(&workspace);
    fs::create_dir_all(&snapshots).unwrap();

    for index in 0..(MAX_SESSION_SNAPSHOTS * 140) {
        fs::write(
            snapshots.join(format!("session.malformed-{index:04}.json")),
            "{not valid json",
        )
        .unwrap();
    }

    let valid = sample_session(&workspace, "bounded snapshot scan recovery");
    fs::write(
        snapshots.join("session.999999999999999999999999999999.0.0000000000000000.json"),
        serde_json::to_string_pretty(&valid).unwrap(),
    )
    .unwrap();

    assert_eq!(PersistedSession::load(&workspace).unwrap(), Some(valid));

    remove_workspace(&workspace);
}

#[test]
fn load_session_quarantines_oversized_snapshot_and_starts_clean() {
    let workspace = temp_workspace("oversized-session");
    let state = state_dir(&workspace);
    fs::create_dir_all(&state).unwrap();
    let session = state.join("session.json");
    fs::write(
        &session,
        vec![b'a'; usize::try_from(PERSISTED_SESSION_MAX_BYTES + 1).unwrap()],
    )
    .unwrap();

    assert_eq!(PersistedSession::load(&workspace).unwrap(), None);
    assert!(!session.exists());

    let quarantined = fs::read_dir(&state)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with("session.json.corrupt."))
        })
        .collect::<Vec<_>>();
    assert_eq!(quarantined.len(), 1);
    assert_eq!(
        fs::metadata(quarantined[0].path()).unwrap().len(),
        PERSISTED_SESSION_MAX_BYTES + 1
    );

    remove_workspace(&workspace);
}
