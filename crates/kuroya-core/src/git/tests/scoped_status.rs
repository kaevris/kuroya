use super::*;
use std::collections::HashMap;

fn scoped_test_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "kuroya-scoped-{label}-{}-{}",
        std::process::id(),
        unique_suffix()
    ))
}

fn scoped_status_for(repo: &Repository, paths: &[PathBuf]) -> GitScopedStatus {
    status_entries_for_paths(
        repo,
        paths,
        DEFAULT_GIT_STATUS_LIMIT,
        false,
        true,
        DEFAULT_GIT_DETECT_SUBMODULES_LIMIT,
        DEFAULT_GIT_SIMILARITY_THRESHOLD,
    )
    .unwrap()
}

fn merge_paths(
    snapshot: &mut GitSnapshot,
    root: &Path,
    repo: &Repository,
    paths: &[PathBuf],
) -> bool {
    let scoped = scoped_status_for(repo, paths);
    snapshot.merge_scoped_statuses(root, paths, scoped.entries, DEFAULT_GIT_STATUS_LIMIT)
}

#[test]
fn scoped_status_entries_match_full_scan_for_mixed_stage_paths() {
    let root = scoped_test_root("mixed");
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let tracked = root.join("tracked.txt");
    let doubled = root.join("doubled.txt");
    fs::write(&tracked, "one\n").unwrap();
    fs::write(&doubled, "base\n").unwrap();
    commit_all(&repo, "initial");

    fs::write(&tracked, "two\n").unwrap();
    let staged = root.join("staged.txt");
    fs::write(&staged, "staged\n").unwrap();
    stage_path(&root, &staged).unwrap();
    fs::write(&doubled, "staged edit\n").unwrap();
    stage_path(&root, &doubled).unwrap();
    fs::write(&doubled, "worktree edit\n").unwrap();
    let untracked = root.join("untracked.txt");
    fs::write(&untracked, "new\n").unwrap();

    let queried = vec![
        tracked.clone(),
        doubled.clone(),
        staged.clone(),
        untracked.clone(),
    ];
    let scoped = scoped_status_for(&repo, &queried);
    let full = GitSnapshot::scan_with_status_limit(&root, DEFAULT_GIT_STATUS_LIMIT);

    assert!(!scoped.status_limited);
    assert_eq!(scoped.entries, full.entries());
    assert_eq!(scoped.entries.len(), 5);
    let tuples = scoped
        .entries
        .iter()
        .map(|entry| (entry.path.clone(), entry.status, entry.stage))
        .collect::<Vec<_>>();
    assert_eq!(
        tuples,
        vec![
            (
                doubled.clone(),
                GitFileStatus::Modified,
                GitChangeStage::Staged
            ),
            (staged.clone(), GitFileStatus::Added, GitChangeStage::Staged),
            (
                doubled.clone(),
                GitFileStatus::Modified,
                GitChangeStage::Unstaged
            ),
            (
                tracked.clone(),
                GitFileStatus::Modified,
                GitChangeStage::Unstaged
            ),
            (
                untracked.clone(),
                GitFileStatus::Untracked,
                GitChangeStage::Unstaged
            ),
        ]
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn scoped_status_file_pathspec_finds_untracked_file_in_new_directory() {
    let root = scoped_test_root("untracked-dir-file");
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("base.txt"), "base\n").unwrap();
    commit_all(&repo, "initial");

    let dir = root.join("new_dir");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.txt"), "a\n").unwrap();
    fs::write(dir.join("b.txt"), "b\n").unwrap();

    let scoped = scoped_status_for(&repo, &[dir.join("a.txt")]);

    assert_eq!(scoped.entries.len(), 1);
    assert_eq!(scoped.entries[0].path, dir.join("a.txt"));
    assert_eq!(scoped.entries[0].status, GitFileStatus::Untracked);
    assert_eq!(scoped.entries[0].stage, GitChangeStage::Unstaged);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn scoped_status_directory_pathspec_reports_individual_files() {
    let root = scoped_test_root("dir-pathspec");
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("base.txt"), "base\n").unwrap();
    commit_all(&repo, "initial");

    let dir = root.join("new_dir");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.txt"), "a\n").unwrap();
    fs::write(dir.join("b.txt"), "b\n").unwrap();

    let scoped = scoped_status_for(&repo, std::slice::from_ref(&dir));
    let full = GitSnapshot::scan_with_status_limit(&root, DEFAULT_GIT_STATUS_LIMIT);

    let paths = scoped
        .entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<Vec<_>>();
    assert_eq!(paths, vec![dir.join("a.txt"), dir.join("b.txt")]);
    assert_eq!(scoped.entries, full.entries());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn scoped_status_ignores_changes_outside_queried_paths() {
    let root = scoped_test_root("outside");
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let tracked = root.join("tracked.txt");
    fs::write(&tracked, "one\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&tracked, "two\n").unwrap();
    let other = root.join("other_untracked.txt");
    fs::write(&other, "new\n").unwrap();

    let scoped = scoped_status_for(&repo, std::slice::from_ref(&tracked));

    assert_eq!(scoped.entries.len(), 1);
    assert_eq!(scoped.entries[0].path, tracked);
    assert_eq!(scoped.entries[0].status, GitFileStatus::Modified);
    assert_eq!(scoped.entries[0].stage, GitChangeStage::Unstaged);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn scoped_status_respects_status_limit() {
    let root = scoped_test_root("limit");
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let paths = vec![root.join("a.txt"), root.join("b.txt"), root.join("c.txt")];
    for path in &paths {
        fs::write(
            path,
            format!("{}\n", path.file_name().unwrap().to_string_lossy()),
        )
        .unwrap();
    }

    let limited = status_entries_for_paths(
        &repo,
        &paths,
        2,
        false,
        true,
        DEFAULT_GIT_DETECT_SUBMODULES_LIMIT,
        DEFAULT_GIT_SIMILARITY_THRESHOLD,
    )
    .unwrap();
    assert_eq!(limited.entries.len(), 2);
    assert_eq!(limited.entries[0].path, paths[0]);
    assert_eq!(limited.entries[1].path, paths[1]);
    assert!(limited.status_limited);

    let unlimited = scoped_status_for(&repo, &paths);
    assert_eq!(unlimited.entries.len(), 3);
    assert!(!unlimited.status_limited);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn scoped_status_without_paths_returns_empty() {
    let root = scoped_test_root("empty");
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    fs::write(root.join("new.txt"), "new\n").unwrap();

    let scoped = scoped_status_for(&repo, &[]);

    assert!(scoped.entries.is_empty());
    assert!(!scoped.status_limited);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn merge_scoped_statuses_updates_staged_addition_to_untracked() {
    let root = scoped_test_root("merge-unstage");
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let path = root.join("new.txt");
    fs::write(&path, "hello\n").unwrap();
    stage_path(&root, &path).unwrap();

    let mut snapshot = GitSnapshot::scan_with_status_limit(&root, DEFAULT_GIT_STATUS_LIMIT);
    assert_eq!(
        snapshot.entries()[0].status,
        GitFileStatus::Added,
        "precondition: entry starts staged"
    );
    assert_eq!(snapshot.counts().added, 1);

    unstage_path(&root, &path).unwrap();
    let changed = merge_paths(&mut snapshot, &root, &repo, std::slice::from_ref(&path));

    assert!(changed);
    assert_eq!(snapshot.len(), 1);
    let entries = snapshot.entries();
    assert_eq!(entries[0].path, path);
    assert_eq!(entries[0].status, GitFileStatus::Untracked);
    assert_eq!(entries[0].stage, GitChangeStage::Unstaged);
    assert_eq!(snapshot.status_for(&path), Some(GitFileStatus::Untracked));
    assert!(snapshot.has_stage_for(&path, GitChangeStage::Unstaged));
    assert!(!snapshot.has_stage_for(&path, GitChangeStage::Staged));
    let counts = snapshot.counts();
    assert_eq!(counts.added, 0);
    assert_eq!(counts.untracked, 1);
    assert_eq!(counts.total(), 1);
    assert!(!snapshot.status_limited());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn merge_scoped_statuses_removes_queried_paths_that_became_clean() {
    let root = scoped_test_root("merge-clean");
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let path = root.join("tracked.txt");
    fs::write(&path, "one\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&path, "two\n").unwrap();

    let mut snapshot = GitSnapshot::scan_with_status_limit(&root, DEFAULT_GIT_STATUS_LIMIT);
    assert_eq!(snapshot.len(), 1, "precondition: modified entry exists");

    discard_path(&root, &path).unwrap();
    let changed = merge_paths(&mut snapshot, &root, &repo, std::slice::from_ref(&path));

    assert!(changed);
    assert!(snapshot.is_empty());
    assert!(snapshot.entries().is_empty());
    assert_eq!(snapshot.status_for(&path), None);
    assert_eq!(snapshot.counts().total(), 0);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn merge_scoped_statuses_upserts_new_untracked_file() {
    let root = scoped_test_root("merge-upsert");
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let tracked = root.join("tracked.txt");
    fs::write(&tracked, "one\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&tracked, "two\n").unwrap();

    let mut snapshot = GitSnapshot::scan_with_status_limit(&root, DEFAULT_GIT_STATUS_LIMIT);
    assert_eq!(snapshot.counts().modified, 1);

    let fresh = root.join("fresh.txt");
    fs::write(&fresh, "new\n").unwrap();
    let changed = merge_paths(&mut snapshot, &root, &repo, std::slice::from_ref(&fresh));

    assert!(changed);
    assert_eq!(snapshot.len(), 2);
    assert_eq!(snapshot.status_for(&fresh), Some(GitFileStatus::Untracked));
    let counts = snapshot.counts();
    assert_eq!(counts.modified, 1);
    assert_eq!(counts.untracked, 1);
    assert_eq!(counts.total(), 2);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn merge_scoped_statuses_returns_false_when_results_match_snapshot() {
    let root = scoped_test_root("merge-noop");
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let tracked = root.join("tracked.txt");
    fs::write(&tracked, "one\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&tracked, "two\n").unwrap();

    let mut snapshot = GitSnapshot::scan_with_status_limit(&root, DEFAULT_GIT_STATUS_LIMIT);
    let entries_before = snapshot.entries();
    let counts_before = snapshot.counts();

    let changed = merge_paths(&mut snapshot, &root, &repo, std::slice::from_ref(&tracked));

    assert!(!changed);
    assert_eq!(snapshot.entries(), entries_before);
    assert_eq!(snapshot.counts(), counts_before);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn merge_scoped_statuses_keeps_stage_then_path_order() {
    let root = scoped_test_root("merge-order");
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let middle = root.join("m.txt");
    fs::write(&middle, "middle\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&middle, "changed\n").unwrap();

    let mut snapshot = GitSnapshot::scan_with_status_limit(&root, DEFAULT_GIT_STATUS_LIMIT);

    let first = root.join("a.txt");
    let last = root.join("z.txt");
    fs::write(&first, "a\n").unwrap();
    fs::write(&last, "z\n").unwrap();
    let changed = merge_paths(&mut snapshot, &root, &repo, &[first.clone(), last.clone()]);

    assert!(changed);
    let paths = snapshot
        .entries()
        .into_iter()
        .map(|entry| entry.path)
        .collect::<Vec<_>>();
    assert_eq!(paths, vec![first, middle, last]);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn merge_scoped_statuses_accepts_relative_queried_paths() {
    let root = scoped_test_root("merge-relative");
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let tracked = root.join("tracked.txt");
    fs::write(&tracked, "one\n").unwrap();
    commit_all(&repo, "initial");

    let mut snapshot = GitSnapshot::scan_with_status_limit(&root, DEFAULT_GIT_STATUS_LIMIT);
    assert!(snapshot.is_empty());

    let fresh = root.join("fresh.txt");
    fs::write(&fresh, "new\n").unwrap();
    let scoped = scoped_status_for(&repo, std::slice::from_ref(&fresh));
    let changed = snapshot.merge_scoped_statuses(
        &root,
        &[PathBuf::from("fresh.txt")],
        scoped.entries,
        DEFAULT_GIT_STATUS_LIMIT,
    );

    assert!(changed);
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot.status_for(&fresh), Some(GitFileStatus::Untracked));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn merge_scoped_statuses_sets_status_limited_when_results_reach_limit() {
    let root = scoped_test_root("merge-limit");
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);

    let mut snapshot = GitSnapshot::scan_with_status_limit(&root, DEFAULT_GIT_STATUS_LIMIT);
    assert!(!snapshot.status_limited());

    let paths = vec![root.join("a.txt"), root.join("b.txt"), root.join("c.txt")];
    for path in &paths {
        fs::write(path, "x\n").unwrap();
    }
    let scoped = scoped_status_for(&repo, &paths);
    let changed = snapshot.merge_scoped_statuses(&root, &paths, scoped.entries, 2);

    assert!(changed);
    assert!(snapshot.status_limited());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn merge_scoped_statuses_preserves_branch_divergence_and_scan_error() {
    let root = scoped_test_root("merge-metadata");
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let path = root.join("tracked.txt");
    fs::write(&path, "one\n").unwrap();
    commit_all(&repo, "initial");

    let divergence = GitRemoteDivergence {
        incoming: 1,
        outgoing: 2,
    };
    let mut statuses = HashMap::new();
    statuses.insert(
        path.clone(),
        super::super::GitStatusLookup::new(GitFileStatus::Modified, GitChangeStage::Unstaged),
    );
    let mut snapshot = GitSnapshot {
        root: Some(root.clone()),
        branch: Some("feature".to_owned()),
        entries: vec![GitStatusEntry {
            path: path.clone(),
            status: GitFileStatus::Modified,
            stage: GitChangeStage::Unstaged,
        }],
        statuses,
        counts: GitStatusCounts {
            modified: 1,
            ..GitStatusCounts::default()
        },
        status_limited: false,
        remote_divergence: Some(divergence),
        scan_error: None,
        revision: 1,
    };

    fs::write(&path, "one\n").unwrap();
    let scoped = scoped_status_for(&repo, std::slice::from_ref(&path));
    assert!(scoped.entries.is_empty());
    let changed =
        snapshot.merge_scoped_statuses(&root, &[path], scoped.entries, DEFAULT_GIT_STATUS_LIMIT);

    assert!(changed);
    assert!(snapshot.is_empty());
    assert_eq!(snapshot.branch(), Some("feature"));
    assert_eq!(snapshot.remote_divergence(), Some(divergence));
    assert!(snapshot.scan_error().is_none());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn merged_snapshot_matches_cold_rescan() {
    let root = scoped_test_root("merge-vs-rescan");
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let tracked = root.join("tracked.txt");
    fs::write(&tracked, "one\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&tracked, "two\n").unwrap();
    let staged = root.join("staged.txt");
    fs::write(&staged, "staged\n").unwrap();

    let mut snapshot = GitSnapshot::scan_with_status_limit(&root, DEFAULT_GIT_STATUS_LIMIT);
    stage_path(&root, &staged).unwrap();
    let changed = merge_paths(&mut snapshot, &root, &repo, std::slice::from_ref(&staged));

    assert!(changed);
    let fresh = GitSnapshot::scan_with_status_limit(&root, DEFAULT_GIT_STATUS_LIMIT);
    assert_eq!(snapshot.entries(), fresh.entries());
    assert_eq!(snapshot.counts(), fresh.counts());
    assert_eq!(snapshot.len(), fresh.len());
    assert_eq!(snapshot.status_for(&staged), fresh.status_for(&staged));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn discard_path_removes_untracked_file_inside_new_directory() {
    let root = scoped_test_root("discard-untracked-dir");
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("base.txt"), "base\n").unwrap();
    commit_all(&repo, "initial");

    let dir = root.join("new_dir");
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("a.txt");
    fs::write(&file, "a\n").unwrap();

    discard_path(&root, &file).unwrap();

    assert!(!file.exists());
    assert!(GitSnapshot::scan(&root).entries().is_empty());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn discard_paths_restores_staged_rename_queried_on_both_paths() {
    let root = scoped_test_root("discard-rename");
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let old = root.join("r1.txt");
    fs::write(&old, "same content\n").unwrap();
    commit_all(&repo, "initial");

    let new = root.join("r2.txt");
    fs::rename(&old, &new).unwrap();
    stage_paths(&root, [old.as_path(), new.as_path()]).unwrap();
    let staged = GitSnapshot::scan(&root).entries();
    assert_eq!(staged.len(), 1, "precondition: staged rename is one entry");
    assert_eq!(staged[0].status, GitFileStatus::Renamed);

    discard_paths(&root, [old.as_path(), new.as_path()]).unwrap();

    assert_eq!(
        fs::read_to_string(&old).unwrap().replace("\r\n", "\n"),
        "same content\n"
    );
    assert!(!new.exists());
    assert!(GitSnapshot::scan(&root).entries().is_empty());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn has_repository_tracks_snapshot_root() {
    assert!(!GitSnapshot::default().has_repository());

    let root = scoped_test_root("has-repository");
    fs::create_dir_all(&root).unwrap();
    Repository::init(&root).unwrap();
    let snapshot = GitSnapshot::scan_with_status_limit(&root, DEFAULT_GIT_STATUS_LIMIT);

    assert!(snapshot.has_repository());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn git_scoped_status_snapshot_updates_snapshot_without_cold_rescan() {
    let root = scoped_test_root("wrapper-merge");
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let tracked = root.join("tracked.txt");
    fs::write(&tracked, "one\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&tracked, "two\n").unwrap();
    let other = root.join("other_untracked.txt");
    fs::write(&other, "new\n").unwrap();

    let snapshot = GitSnapshot::scan_with_status_limit(&root, DEFAULT_GIT_STATUS_LIMIT);
    assert_eq!(snapshot.len(), 2, "precondition: both changes are visible");

    stage_path(&root, &tracked).unwrap();
    let updated = git_scoped_status_snapshot(
        &snapshot,
        std::slice::from_ref(&tracked),
        DEFAULT_GIT_STATUS_LIMIT,
        false,
        true,
        DEFAULT_GIT_DETECT_SUBMODULES_LIMIT,
        DEFAULT_GIT_SIMILARITY_THRESHOLD,
        false,
    )
    .expect("scoped refresh should succeed for an open repository");

    assert_eq!(updated.root(), snapshot.root());
    assert_eq!(updated.branch(), snapshot.branch());
    assert!(updated.has_stage_for(&tracked, GitChangeStage::Staged));
    assert!(!updated.has_stage_for(&tracked, GitChangeStage::Unstaged));
    assert_eq!(updated.status_for(&other), Some(GitFileStatus::Untracked));
    let fresh = GitSnapshot::scan_with_status_limit(&root, DEFAULT_GIT_STATUS_LIMIT);
    assert_eq!(updated.entries(), fresh.entries());
    assert_eq!(updated.counts(), fresh.counts());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn git_scoped_status_snapshot_returns_none_without_a_repository() {
    let snapshot = GitSnapshot::default();
    let path = PathBuf::from("kuroya-scoped-without-repo.txt");

    assert!(
        git_scoped_status_snapshot(
            &snapshot,
            std::slice::from_ref(&path),
            DEFAULT_GIT_STATUS_LIMIT,
            false,
            true,
            DEFAULT_GIT_DETECT_SUBMODULES_LIMIT,
            DEFAULT_GIT_SIMILARITY_THRESHOLD,
            false,
        )
        .is_none()
    );
}

#[test]
fn git_scoped_status_snapshot_returns_none_when_repository_cannot_be_opened() {
    let root = scoped_test_root("wrapper-unopenable");
    let snapshot = GitSnapshot {
        root: Some(root.clone()),
        ..GitSnapshot::default()
    };
    let path = root.join("tracked.txt");

    assert!(
        git_scoped_status_snapshot(
            &snapshot,
            std::slice::from_ref(&path),
            DEFAULT_GIT_STATUS_LIMIT,
            false,
            true,
            DEFAULT_GIT_DETECT_SUBMODULES_LIMIT,
            DEFAULT_GIT_SIMILARITY_THRESHOLD,
            false,
        )
        .is_none()
    );
}
