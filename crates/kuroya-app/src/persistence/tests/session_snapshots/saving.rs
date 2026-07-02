use super::*;

#[test]
fn save_session_replaces_existing_snapshot_without_temp_files() {
    let workspace = temp_workspace("sync");
    fs::create_dir_all(&workspace).unwrap();

    let first = sample_session(&workspace, "first recovery");
    let second = sample_session(&workspace, "second recovery");
    save_session(&workspace, &first).unwrap();
    save_session(&workspace, &second).unwrap();

    assert_eq!(PersistedSession::load(&workspace).unwrap(), Some(second));
    let backups = session_snapshot_files_for_test(&workspace);
    assert_eq!(backups.len(), 1);
    assert_eq!(
        serde_json::from_str::<PersistedSession>(&fs::read_to_string(&backups[0]).unwrap())
            .unwrap(),
        first
    );
    assert_no_session_temps(&workspace);

    remove_workspace(&workspace);
}

#[test]
fn save_session_skips_duplicate_snapshot_bytes() {
    let workspace = temp_workspace("dedupe");
    fs::create_dir_all(&workspace).unwrap();

    let session = sample_session(&workspace, "same recovery");
    save_session(&workspace, &session).unwrap();
    save_session(&workspace, &session).unwrap();

    assert_eq!(PersistedSession::load(&workspace).unwrap(), Some(session));
    assert!(session_snapshot_files_for_test(&workspace).is_empty());
    assert_no_session_temps(&workspace);

    remove_workspace(&workspace);
}

#[test]
fn save_session_replaces_current_when_snapshot_dir_is_obstructed() {
    let workspace = temp_workspace("snapshot-dir-obstructed");
    fs::create_dir_all(&workspace).unwrap();

    let first = sample_session(&workspace, "first recovery");
    let second = sample_session(&workspace, "second recovery");
    save_session(&workspace, &first).unwrap();

    let snapshots = crate::persistence_storage::session_snapshots_dir(&workspace);
    fs::write(&snapshots, "not a directory").unwrap();

    save_session(&workspace, &second).unwrap();

    assert_eq!(PersistedSession::load(&workspace).unwrap(), Some(second));
    assert!(snapshots.is_file());
    assert_no_session_temps(&workspace);

    remove_workspace(&workspace);
}

#[test]
fn save_session_quarantines_oversized_existing_session_before_replacing() {
    let workspace = temp_workspace("oversized-before-save");
    let state = state_dir(&workspace);
    fs::create_dir_all(&state).unwrap();
    let path = state.join("session.json");
    fs::write(
        &path,
        vec![b'a'; usize::try_from(PERSISTED_SESSION_MAX_BYTES + 1).unwrap()],
    )
    .unwrap();

    let session = sample_session(&workspace, "replacement recovery");
    save_session(&workspace, &session).unwrap();

    assert_eq!(PersistedSession::load(&workspace).unwrap(), Some(session));
    assert_eq!(session_snapshot_files_for_test(&workspace).len(), 0);
    let quarantined = quarantined_session_files(&state);
    assert_eq!(quarantined.len(), 1);
    assert_eq!(
        fs::metadata(&quarantined[0]).unwrap().len(),
        PERSISTED_SESSION_MAX_BYTES + 1
    );
    assert_no_session_temps(&workspace);

    remove_workspace(&workspace);
}

#[tokio::test]
async fn save_session_async_replaces_existing_snapshot_without_temp_files() {
    let workspace = temp_workspace("async");
    fs::create_dir_all(&workspace).unwrap();

    let first = sample_session(&workspace, "first async recovery");
    let second = sample_session(&workspace, "second async recovery");
    save_session_async(workspace.clone(), first.clone())
        .await
        .unwrap();
    save_session_async(workspace.clone(), second.clone())
        .await
        .unwrap();

    assert_eq!(PersistedSession::load(&workspace).unwrap(), Some(second));
    let backups = session_snapshot_files_for_test(&workspace);
    assert_eq!(backups.len(), 1);
    assert_eq!(
        serde_json::from_str::<PersistedSession>(&fs::read_to_string(&backups[0]).unwrap())
            .unwrap(),
        first
    );
    assert_no_session_temps(&workspace);

    remove_workspace(&workspace);
}

#[tokio::test]
async fn save_session_async_replaces_current_when_snapshot_dir_is_obstructed() {
    let workspace = temp_workspace("async-snapshot-dir-obstructed");
    fs::create_dir_all(&workspace).unwrap();

    let first = sample_session(&workspace, "first async recovery");
    let second = sample_session(&workspace, "second async recovery");
    save_session_async(workspace.clone(), first).await.unwrap();

    let snapshots = crate::persistence_storage::session_snapshots_dir(&workspace);
    fs::write(&snapshots, "not a directory").unwrap();

    save_session_async(workspace.clone(), second.clone())
        .await
        .unwrap();

    assert_eq!(PersistedSession::load(&workspace).unwrap(), Some(second));
    assert!(snapshots.is_file());
    assert_no_session_temps(&workspace);

    remove_workspace(&workspace);
}

#[tokio::test]
async fn save_session_async_quarantines_oversized_existing_session_before_replacing() {
    let workspace = temp_workspace("async-oversized-before-save");
    let state = state_dir(&workspace);
    fs::create_dir_all(&state).unwrap();
    let path = state.join("session.json");
    fs::write(
        &path,
        vec![b'a'; usize::try_from(PERSISTED_SESSION_MAX_BYTES + 1).unwrap()],
    )
    .unwrap();

    let session = sample_session(&workspace, "async replacement recovery");
    save_session_async(workspace.clone(), session.clone())
        .await
        .unwrap();

    assert_eq!(PersistedSession::load(&workspace).unwrap(), Some(session));
    assert_eq!(session_snapshot_files_for_test(&workspace).len(), 0);
    let quarantined = quarantined_session_files(&state);
    assert_eq!(quarantined.len(), 1);
    assert_eq!(
        fs::metadata(&quarantined[0]).unwrap().len(),
        PERSISTED_SESSION_MAX_BYTES + 1
    );
    assert_no_session_temps(&workspace);

    remove_workspace(&workspace);
}

#[test]
fn session_snapshots_are_bounded_to_recent_backups() {
    let workspace = temp_workspace("bounded");
    fs::create_dir_all(&workspace).unwrap();

    for index in 0..(MAX_SESSION_SNAPSHOTS + 4) {
        save_session(
            &workspace,
            &sample_session(&workspace, &format!("recovery {index}")),
        )
        .unwrap();
    }

    let backups = session_snapshot_files_for_test(&workspace);
    assert_eq!(backups.len(), MAX_SESSION_SNAPSHOTS);
    let backup_texts = backups
        .iter()
        .map(|path| {
            serde_json::from_str::<PersistedSession>(&fs::read_to_string(path).unwrap())
                .unwrap()
                .recovery[0]
                .text
                .clone()
        })
        .collect::<Vec<_>>();

    assert!(!backup_texts.contains(&"recovery 0".to_owned()));
    assert!(!backup_texts.contains(&"recovery 1".to_owned()));
    assert!(backup_texts.contains(&"recovery 3".to_owned()));
    assert!(backup_texts.contains(&"recovery 10".to_owned()));

    remove_workspace(&workspace);
}
