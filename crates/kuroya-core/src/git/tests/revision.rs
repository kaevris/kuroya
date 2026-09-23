use super::*;

fn revision_test_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "kuroya-revision-{label}-{}-{}",
        std::process::id(),
        unique_suffix()
    ))
}

fn dirty_repo_root(label: &str) -> PathBuf {
    let root = revision_test_root(label);
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("tracked.txt"), "one\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(root.join("tracked.txt"), "two\n").unwrap();
    let untracked = root.join("new.txt");
    fs::write(&untracked, "new\n").unwrap();
    root
}

/// Clean worktree with one committed file so a scoped merge that reports the
/// file as modified is always a real change against the scanned snapshot.
fn clean_repo_root(label: &str) -> PathBuf {
    let root = revision_test_root(label);
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("tracked.txt"), "one\n").unwrap();
    commit_all(&repo, "initial");
    root
}

#[test]
fn default_snapshot_starts_at_revision_zero() {
    assert_eq!(GitSnapshot::default().revision(), 0);
}

#[test]
fn scan_sets_snapshot_revision() {
    let root = dirty_repo_root("scan");

    let snapshot = GitSnapshot::scan_with_status_limit(&root, DEFAULT_GIT_STATUS_LIMIT);

    assert_eq!(snapshot.revision(), 1);
    assert_eq!(snapshot.entries().len(), 2);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn changing_scoped_merge_bumps_revision() {
    let root = clean_repo_root("merge-change");

    let mut snapshot = GitSnapshot::scan_with_status_limit(&root, DEFAULT_GIT_STATUS_LIMIT);
    let before = snapshot.revision();
    let path = root.join("tracked.txt");

    let changed = snapshot.merge_scoped_statuses(
        &root,
        std::slice::from_ref(&path),
        vec![GitStatusEntry {
            path: path.clone(),
            status: GitFileStatus::Modified,
            stage: GitChangeStage::Unstaged,
        }],
        DEFAULT_GIT_STATUS_LIMIT,
    );

    assert!(changed);
    assert_eq!(snapshot.revision(), before + 1);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unchanged_scoped_merge_keeps_revision() {
    let root = clean_repo_root("merge-unchanged");

    let mut snapshot = GitSnapshot::scan_with_status_limit(&root, DEFAULT_GIT_STATUS_LIMIT);
    let path = root.join("tracked.txt");
    let entry = GitStatusEntry {
        path: path.clone(),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    };
    assert!(snapshot.merge_scoped_statuses(
        &root,
        std::slice::from_ref(&path),
        vec![entry.clone()],
        DEFAULT_GIT_STATUS_LIMIT,
    ));
    let before = snapshot.revision();

    let changed = snapshot.merge_scoped_statuses(
        &root,
        std::slice::from_ref(&path),
        vec![entry],
        DEFAULT_GIT_STATUS_LIMIT,
    );

    assert!(!changed);
    assert_eq!(snapshot.revision(), before);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn entries_slice_sorted_matches_entries_order() {
    let root = dirty_repo_root("slice-sorted");
    let staged = root.join("staged.txt");
    fs::write(&staged, "staged\n").unwrap();
    stage_path(&root, &staged).unwrap();

    let snapshot = GitSnapshot::scan_with_status_limit(&root, DEFAULT_GIT_STATUS_LIMIT);

    let slice = snapshot.entries_slice_sorted();
    let cloned = snapshot.entries();
    assert_eq!(slice, cloned.as_slice());
    let tuples = slice
        .iter()
        .map(|entry| (entry.stage, entry.path.clone()))
        .collect::<Vec<_>>();
    let mut sorted = tuples.clone();
    sorted.sort();
    assert_eq!(tuples, sorted);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn advance_revision_past_keeps_revisions_monotonic() {
    let mut snapshot = GitSnapshot::default();
    assert_eq!(snapshot.revision(), 0);

    snapshot.advance_revision_past(0);
    assert_eq!(snapshot.revision(), 1);

    snapshot.advance_revision_past(1);
    assert_eq!(snapshot.revision(), 2);

    snapshot.advance_revision_past(5);
    assert_eq!(snapshot.revision(), 6);
}
