use super::*;

#[test]
fn load_session_starts_clean_when_snapshot_dir_is_obstructed() {
    let workspace = temp_workspace("snapshot-dir-obstructed-restore");
    let state = state_dir(&workspace);
    fs::create_dir_all(&state).unwrap();

    let snapshots = crate::persistence_storage::session_snapshots_dir(&workspace);
    fs::write(&snapshots, "not a directory").unwrap();
    let session = state.join("session.json");
    fs::write(&session, "{not valid json").unwrap();

    assert_eq!(PersistedSession::load(&workspace).unwrap(), None);
    assert!(!session.exists());
    assert!(snapshots.is_file());
    assert_eq!(quarantined_session_files(&state).len(), 1);

    remove_workspace(&workspace);
}
