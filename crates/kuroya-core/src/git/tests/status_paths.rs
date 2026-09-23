use super::*;

#[test]
fn git_status_counts_saturate_extreme_totals() {
    let mut counts = GitStatusCounts {
        modified: usize::MAX,
        added: 1,
        deleted: 1,
        renamed: 1,
        untracked: usize::MAX,
        conflicted: 1,
    };

    assert_eq!(counts.total(), usize::MAX);
    assert_eq!(counts.tracked_total(), usize::MAX);

    counts.record(GitFileStatus::Modified);
    counts.record(GitFileStatus::Untracked);

    assert_eq!(counts.modified, usize::MAX);
    assert_eq!(counts.untracked, usize::MAX);
}

#[test]
fn worktree_relative_path_accepts_lexically_equivalent_path() {
    assert_eq!(
        worktree_relative_path(
            Path::new("workspace"),
            &PathBuf::from("workspace")
                .join("src")
                .join("..")
                .join("src")
                .join("main.rs")
        )
        .unwrap(),
        PathBuf::from("src").join("main.rs")
    );
}

#[cfg(windows)]
#[test]
fn worktree_relative_path_matches_windows_case_aliases() {
    assert_eq!(
        worktree_relative_path(
            Path::new(r"C:\Repo"),
            &PathBuf::from(r"c:\repo").join("SRC").join("main.rs")
        )
        .unwrap(),
        PathBuf::from("SRC").join("main.rs")
    );
}

#[test]
fn worktree_relative_path_rejects_escape_and_reenter_alias() {
    assert!(
        worktree_relative_path(
            Path::new("workspace"),
            &PathBuf::from("workspace")
                .join("..")
                .join("workspace")
                .join("src")
                .join("main.rs")
        )
        .is_err()
    );
}

#[test]
fn worktree_relative_path_rejects_worktree_root() {
    let direct = worktree_relative_path(Path::new("workspace"), Path::new("workspace"))
        .unwrap_err()
        .to_string();
    assert!(direct.contains("worktree root"));

    let collapsed = worktree_relative_path(
        Path::new("workspace"),
        &PathBuf::from("workspace").join("src").join(".."),
    )
    .unwrap_err()
    .to_string();
    assert!(collapsed.contains("worktree root"));

    let current_dir = worktree_relative_path(Path::new("."), Path::new("."))
        .unwrap_err()
        .to_string();
    assert!(current_dir.contains("worktree root"));
}

#[test]
fn worktree_relative_path_handles_current_dir_root() {
    assert_eq!(
        worktree_relative_path(Path::new("."), Path::new("src/lib.rs")).unwrap(),
        PathBuf::from("src").join("lib.rs")
    );
    assert!(worktree_relative_path(Path::new("."), Path::new("../outside.rs")).is_err());
}

#[test]
fn git_path_label_strips_hidden_controls_and_bounds_long_paths() {
    assert_eq!(
        super::super::git_path_label("src/\u{202E}main.rs\nignored").unwrap(),
        "src/main.rsignored"
    );
    assert_eq!(super::super::git_path_label("\u{202E}\n"), None);

    let raw = format!(
        "{}{}",
        "a".repeat(super::super::MAX_GIT_PATH_LABEL_CHARS + 80),
        "/tail.rs"
    );
    let label = super::super::git_path_label(&raw).unwrap();

    assert!(label.len() <= super::super::MAX_GIT_PATH_LABEL_CHARS);
    assert!(label.starts_with(&"a".repeat(super::super::GIT_PATH_LABEL_HEAD_CHARS)));
    assert!(label.contains(super::super::GIT_PATH_LABEL_OMISSION));
    assert!(label.ends_with("/tail.rs"));
}

#[test]
fn git_path_label_bounds_multibyte_display_chars() {
    let raw = format!(
        "{}{}",
        "\u{00e9}".repeat(super::super::MAX_GIT_PATH_LABEL_CHARS + 80),
        "/tail.rs"
    );
    let label = super::super::git_path_label(&raw).unwrap();

    assert!(label.chars().count() <= super::super::MAX_GIT_PATH_LABEL_CHARS);
    assert!(label.contains(super::super::GIT_PATH_LABEL_OMISSION));
    assert!(label.ends_with("/tail.rs"));
}

#[test]
fn git_path_label_from_path_matches_string_labels_without_full_display() {
    let raw = PathBuf::from("src")
        .join(format!(
            "{}\u{202E}\n",
            "a".repeat(super::super::MAX_GIT_PATH_LABEL_CHARS + 80)
        ))
        .join("tail.rs");
    let expected = super::super::git_path_label(&super::super::git_path_display(&raw));
    let label = super::super::git_path_label_from_path(&raw);

    assert_eq!(label, expected);
    let label = label.unwrap();
    assert!(label.len() <= super::super::MAX_GIT_PATH_LABEL_CHARS);
    assert!(label.starts_with("src/"));
    assert!(label.contains(super::super::GIT_PATH_LABEL_OMISSION));
    assert!(label.ends_with("/tail.rs"));
}

#[test]
fn git_path_display_helpers_share_forward_slash_labels() {
    let path = PathBuf::from("src").join("main.rs");

    assert_eq!(super::super::git_path_display(&path), "src/main.rs");
    assert_eq!(
        super::super::git_path_display_with_prefix("a/", &path),
        "a/src/main.rs"
    );

    let labels = super::super::GitDiffLabels::for_relative_path(&path);
    assert_eq!(labels.old_git_label, "a/src/main.rs");
    assert_eq!(labels.new_git_label, "b/src/main.rs");
}

#[test]
fn status_path_matching_uses_raw_path_before_normalizing_display() {
    let mut requested = BTreeSet::new();
    requested.insert("src/main.rs".to_owned());

    assert!(super::super::path_matches_requested_keys(
        Some(Path::new("src/main.rs")),
        &requested
    ));
    assert!(!super::super::path_matches_requested_keys(
        Some(Path::new("src/lib.rs")),
        &requested
    ));

    let raw_requested = BTreeSet::from(["src/./main.rs"]);
    assert!(super::super::status_path_matches_requested_keys(
        "src/./main.rs",
        &raw_requested
    ));
    assert!(super::super::path_matches_requested_keys(
        Some(Path::new("src/./main.rs")),
        &raw_requested
    ));
}

#[test]
fn status_path_matching_accepts_borrowed_requested_keys() {
    let requested = BTreeSet::from(["src/main.rs"]);

    assert!(super::super::path_matches_requested_keys(
        Some(Path::new("src/main.rs")),
        &requested
    ));
}

#[test]
fn status_path_matching_rejects_paths_that_escape_worktree_root() {
    let requested = BTreeSet::from(["outside.rs"]);

    assert!(!super::super::path_matches_requested_keys(
        Some(Path::new("../outside.rs")),
        &requested
    ));
    assert!(!super::super::status_path_matches_requested_keys(
        "../outside.rs",
        &requested
    ));

    let raw_escape_requested = BTreeSet::from(["../outside.rs"]);
    assert!(!super::super::path_matches_requested_keys(
        Some(Path::new("../outside.rs")),
        &raw_escape_requested
    ));
    assert!(!super::super::status_path_matches_requested_keys(
        "../outside.rs",
        &raw_escape_requested
    ));

    let current_dir_requested = BTreeSet::from(["."]);
    assert!(!super::super::path_matches_requested_keys(
        Some(Path::new(".")),
        &current_dir_requested
    ));
    assert!(!super::super::status_path_matches_requested_keys(
        ".",
        &current_dir_requested
    ));
}

#[cfg(windows)]
#[test]
fn status_path_matching_falls_back_to_normalized_windows_display() {
    let mut requested = BTreeSet::new();
    requested.insert("src/main.rs".to_owned());

    assert!(super::super::path_matches_requested_keys(
        Some(Path::new(r"src\main.rs")),
        &requested
    ));
}

#[test]
fn stage_unstage_and_discard_accept_lexically_equivalent_paths() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-git-mutation-alias-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(root.join("src")).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("src").join("main.rs");
    let alias = root.join("src").join("..").join("src").join("main.rs");
    fs::write(&path, "one\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&path, "two\n").unwrap();

    stage_path(&root, &alias).unwrap();
    assert_eq!(
        file_text_at_index(&root, &path).unwrap(),
        Some("two\n".to_owned())
    );

    unstage_path(&root, &alias).unwrap();
    assert_eq!(
        file_text_at_index(&root, &path).unwrap(),
        Some("one\n".to_owned())
    );

    discard_path(&root, &alias).unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap().replace("\r\n", "\n"),
        "one\n"
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn git_status_counts_track_each_status_kind() {
    let mut counts = GitStatusCounts::default();
    counts.record(GitFileStatus::Modified);
    counts.record(GitFileStatus::Modified);
    counts.record(GitFileStatus::Added);
    counts.record(GitFileStatus::Deleted);
    counts.record(GitFileStatus::Renamed);
    counts.record(GitFileStatus::Untracked);
    counts.record(GitFileStatus::Conflicted);

    assert_eq!(counts.modified, 2);
    assert_eq!(counts.added, 1);
    assert_eq!(counts.deleted, 1);
    assert_eq!(counts.renamed, 1);
    assert_eq!(counts.untracked, 1);
    assert_eq!(counts.conflicted, 1);
    assert_eq!(counts.total(), 7);
    assert_eq!(counts.tracked_total(), 6);
}

#[test]
fn git_status_relative_path_normalizes_safe_raw_paths_and_rejects_escape_paths() {
    assert_eq!(
        super::super::git_status_relative_path("src/../Cargo.toml"),
        Some(PathBuf::from("Cargo.toml"))
    );
    assert_eq!(
        super::super::git_status_relative_path("src/./main.rs"),
        Some(PathBuf::from("src").join("main.rs"))
    );
    assert!(super::super::git_status_relative_path("").is_none());
    assert!(super::super::git_status_relative_path(".").is_none());
    assert!(super::super::git_status_relative_path("src/..").is_none());
    assert!(super::super::git_status_relative_path("../outside.rs").is_none());
    assert!(super::super::git_status_relative_path("src/../../workspace/main.rs").is_none());
    assert!(super::super::git_status_relative_path("/outside.rs").is_none());
    assert!(super::super::git_status_relative_path("src/main.rs\0tail").is_none());

    #[cfg(windows)]
    {
        assert!(super::super::git_status_relative_path("C:\\outside.rs").is_none());
        assert!(super::super::git_status_relative_path("C:outside.rs").is_none());
        assert!(super::super::git_status_relative_path("\\outside.rs").is_none());
        assert!(
            super::super::git_status_relative_path("src\\..\\..\\workspace\\main.rs").is_none()
        );
    }
}

#[test]
fn git_snapshot_entries_are_sorted_by_stage_then_path() {
    let first = PathBuf::from("a.rs");
    let second = PathBuf::from("z.rs");
    let mut statuses = HashMap::new();
    statuses.insert(
        second.clone(),
        super::super::GitStatusLookup::new(GitFileStatus::Deleted, GitChangeStage::Staged),
    );
    statuses.insert(
        first.clone(),
        super::super::GitStatusLookup::new(GitFileStatus::Modified, GitChangeStage::Unstaged),
    );

    let snapshot = GitSnapshot {
        root: Some(PathBuf::from(".")),
        branch: Some("main".to_owned()),
        entries: vec![
            super::super::GitStatusEntry {
                path: second.clone(),
                status: GitFileStatus::Deleted,
                stage: GitChangeStage::Staged,
            },
            super::super::GitStatusEntry {
                path: first.clone(),
                status: GitFileStatus::Modified,
                stage: GitChangeStage::Unstaged,
            },
        ],
        statuses,
        counts: GitStatusCounts::default(),
        status_limited: false,
        remote_divergence: None,
        scan_error: None,
        revision: 1,
    };

    let entries = snapshot.entries();

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].path, second);
    assert_eq!(entries[0].status, GitFileStatus::Deleted);
    assert_eq!(entries[0].stage, GitChangeStage::Staged);
    assert_eq!(entries[1].path, first);
    assert_eq!(entries[1].status, GitFileStatus::Modified);
    assert_eq!(entries[1].stage, GitChangeStage::Unstaged);
    assert_eq!(snapshot.entries_slice_sorted(), entries.as_slice());
}

#[test]
fn git_snapshot_reports_stage_membership_for_paths() {
    let staged = PathBuf::from("src/main.rs");
    let unstaged = PathBuf::from("README.md");
    let mut statuses = HashMap::new();
    statuses.insert(
        staged.clone(),
        super::super::GitStatusLookup::new(GitFileStatus::Modified, GitChangeStage::Staged),
    );
    statuses.insert(
        unstaged.clone(),
        super::super::GitStatusLookup::new(GitFileStatus::Modified, GitChangeStage::Unstaged),
    );
    let snapshot = GitSnapshot {
        root: Some(PathBuf::from(".")),
        branch: Some("main".to_owned()),
        entries: vec![
            super::super::GitStatusEntry {
                path: staged.clone(),
                status: GitFileStatus::Modified,
                stage: GitChangeStage::Staged,
            },
            super::super::GitStatusEntry {
                path: unstaged.clone(),
                status: GitFileStatus::Modified,
                stage: GitChangeStage::Unstaged,
            },
        ],
        statuses,
        counts: GitStatusCounts::default(),
        status_limited: false,
        remote_divergence: None,
        scan_error: None,
        revision: 1,
    };

    assert!(snapshot.has_stage_for(&staged, GitChangeStage::Staged));
    assert!(!snapshot.has_stage_for(&staged, GitChangeStage::Unstaged));
    assert!(snapshot.has_stage_for(&unstaged, GitChangeStage::Unstaged));
    assert!(!snapshot.has_stage_for(&unstaged, GitChangeStage::Staged));
}

#[test]
fn git_snapshot_status_lookup_tracks_both_stages_for_one_path() {
    let path = PathBuf::from("src/main.rs");
    let mut lookup =
        super::super::GitStatusLookup::new(GitFileStatus::Modified, GitChangeStage::Staged);
    lookup.record(GitFileStatus::Deleted, GitChangeStage::Unstaged);
    let mut statuses = HashMap::new();
    statuses.insert(path.clone(), lookup);
    let snapshot = GitSnapshot {
        root: Some(PathBuf::from(".")),
        branch: Some("main".to_owned()),
        entries: vec![
            super::super::GitStatusEntry {
                path: path.clone(),
                status: GitFileStatus::Modified,
                stage: GitChangeStage::Staged,
            },
            super::super::GitStatusEntry {
                path: path.clone(),
                status: GitFileStatus::Deleted,
                stage: GitChangeStage::Unstaged,
            },
        ],
        statuses,
        counts: GitStatusCounts::default(),
        status_limited: false,
        remote_divergence: None,
        scan_error: None,
        revision: 1,
    };

    assert_eq!(snapshot.status_for(&path), Some(GitFileStatus::Deleted));
    assert!(snapshot.has_stage_for(&path, GitChangeStage::Staged));
    assert!(snapshot.has_stage_for(&path, GitChangeStage::Unstaged));
}

#[test]
fn git_snapshot_scan_respects_status_limit() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-status-limit-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    Repository::init(&root).unwrap();
    fs::write(root.join("a.txt"), "a\n").unwrap();
    fs::write(root.join("b.txt"), "b\n").unwrap();
    fs::write(root.join("c.txt"), "c\n").unwrap();

    let limited = GitSnapshot::scan_with_status_limit(&root, 2);
    assert_eq!(limited.entries().len(), 2);
    assert_eq!(limited.counts().total(), 2);
    assert_eq!(limited.len(), 2);
    assert!(limited.status_limited());

    let empty = GitSnapshot::scan_with_status_limit(&root, 0);
    assert!(empty.entries().is_empty());
    assert_eq!(empty.counts().total(), 0);
    assert_eq!(empty.len(), 0);
    assert!(empty.status_limited());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn git_snapshot_parent_repository_policy_can_require_workspace_root() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-parent-repo-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    let child = root.join("packages/app");
    fs::create_dir_all(&child).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("tracked.txt"), "one\n").unwrap();
    commit_all(&repo, "initial");

    let parent_allowed = GitSnapshot::scan_with_status_options_and_parent_policy(
        &child,
        DEFAULT_GIT_STATUS_LIMIT,
        false,
        true,
        DEFAULT_GIT_DETECT_SUBMODULES_LIMIT,
        DEFAULT_GIT_SIMILARITY_THRESHOLD,
        true,
    );
    let parent_blocked = GitSnapshot::scan_with_status_options_and_parent_policy(
        &child,
        DEFAULT_GIT_STATUS_LIMIT,
        false,
        true,
        DEFAULT_GIT_DETECT_SUBMODULES_LIMIT,
        DEFAULT_GIT_SIMILARITY_THRESHOLD,
        false,
    );

    assert_eq!(parent_allowed.root(), Some(root.as_path()));
    assert!(parent_blocked.root().is_none());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn scan_failure_mapping_treats_missing_repository_as_no_error() {
    let missing = std::env::temp_dir().join(format!(
        "kuroya-scan-missing-{}-{}",
        std::process::id(),
        unique_suffix()
    ));

    let error = Repository::discover(&missing)
        .map(|_| ())
        .expect_err("discovery of a missing path should fail");
    assert_eq!(error.code(), git2::ErrorCode::NotFound);

    let snapshot = super::super::snapshot_from_scan_failure(Some(missing), &error);
    assert_eq!(snapshot.root(), None);
    assert!(snapshot.scan_error().is_none());
}

#[test]
fn scan_failure_mapping_reports_errors_with_bounded_messages() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-scan-broken-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    drop(repo);
    fs::write(
        root.join(".git").join("HEAD"),
        [0xff, 0x00, 0x0a, 0x01, 0x02],
    )
    .unwrap();

    let reopened = Repository::open(&root).unwrap();
    let mut options = git2::StatusOptions::new();
    options.include_untracked(true);
    let error = match reopened.statuses(Some(&mut options)) {
        Ok(_) => panic!("statuses over a corrupt repository should fail"),
        Err(error) => error,
    };
    assert_ne!(error.code(), git2::ErrorCode::NotFound);

    let snapshot = super::super::snapshot_from_scan_failure(Some(root.clone()), &error);
    assert_eq!(snapshot.root(), Some(root.as_path()));
    let message = snapshot.scan_error().expect("scan error should be set");
    assert!(!message.is_empty());
    assert!(!message.chars().any(char::is_control));
    assert!(message.chars().count() <= super::super::MAX_GIT_SCAN_ERROR_CHARS);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn git_snapshot_scan_surfaces_corrupt_repository_error_without_losing_root() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-corrupt-repo-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    Repository::init(&root).unwrap();
    fs::write(
        root.join(".git").join("HEAD"),
        [0xff, 0x00, 0x0a, 0x01, 0x02],
    )
    .unwrap();

    let snapshot = GitSnapshot::scan(&root);

    assert!(snapshot.scan_error().is_some());
    assert_eq!(snapshot.root(), Some(root.as_path()));
    assert!(snapshot.entries().is_empty());
    assert!(snapshot.branch().is_none());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn git_snapshot_scan_of_plain_folder_has_no_scan_error() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-no-repo-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();

    let snapshot = GitSnapshot::scan(&root);

    assert_eq!(snapshot.root(), None);
    assert!(snapshot.scan_error().is_none());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn path_is_committed_matches_head_tree_entries() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-path-committed-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(root.join("src")).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let committed = root.join("src/main.rs");
    let untracked = root.join("src/new.rs");
    fs::write(&committed, "fn main() {}\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&untracked, "new\n").unwrap();

    assert!(path_is_committed(&root, &committed).unwrap());
    assert!(path_is_committed(&root, &root.join("src")).unwrap());
    assert!(!path_is_committed(&root, &untracked).unwrap());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn git_snapshot_scan_can_ignore_submodule_modifications() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-ignore-submodules-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    let parent_root = root.join("parent");
    let child_root = root.join("child");
    fs::create_dir_all(&parent_root).unwrap();
    fs::create_dir_all(&child_root).unwrap();
    let child = Repository::init(&child_root).unwrap();
    fs::write(child_root.join("child.txt"), "clean\n").unwrap();
    commit_all(&child, "child initial");

    let parent = Repository::init(&parent_root).unwrap();
    let child_url = child_root.to_string_lossy().replace('\\', "/");
    let submodule_path = Path::new("vendor").join("child");
    let mut submodule = parent.submodule(&child_url, &submodule_path, true).unwrap();
    let checkout_path = parent_root.join(&submodule_path);
    if checkout_path.exists() {
        fs::remove_dir_all(&checkout_path).unwrap();
    }
    Repository::clone(&child_url, &checkout_path).unwrap();
    submodule.add_to_index(false).unwrap();
    submodule.add_finalize().unwrap();
    commit_all(&parent, "add submodule");

    fs::write(checkout_path.join("child.txt"), "dirty\n").unwrap();

    let visible = GitSnapshot::scan_with_status_options(
        &parent_root,
        DEFAULT_GIT_STATUS_LIMIT,
        false,
        true,
        DEFAULT_GIT_DETECT_SUBMODULES_LIMIT,
        DEFAULT_GIT_SIMILARITY_THRESHOLD,
    );
    let ignored = GitSnapshot::scan_with_status_options(
        &parent_root,
        DEFAULT_GIT_STATUS_LIMIT,
        true,
        true,
        DEFAULT_GIT_DETECT_SUBMODULES_LIMIT,
        DEFAULT_GIT_SIMILARITY_THRESHOLD,
    );

    assert!(
        visible
            .entries()
            .iter()
            .any(|entry| entry.path.ends_with(&submodule_path))
    );
    assert!(
        ignored
            .entries()
            .iter()
            .all(|entry| !entry.path.ends_with(&submodule_path))
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn git_snapshot_scan_respects_submodule_detection_limit() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-detect-submodules-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    let parent_root = root.join("parent");
    fs::create_dir_all(&parent_root).unwrap();
    let parent = Repository::init(&parent_root).unwrap();

    for name in ["a-child", "b-child"] {
        let child_root = root.join(name);
        fs::create_dir_all(&child_root).unwrap();
        let child = Repository::init(&child_root).unwrap();
        fs::write(child_root.join("child.txt"), "clean\n").unwrap();
        commit_all(&child, "child initial");

        let child_url = child_root.to_string_lossy().replace('\\', "/");
        let submodule_path = Path::new("vendor").join(name);
        let mut submodule = parent.submodule(&child_url, &submodule_path, true).unwrap();
        let checkout_path = parent_root.join(&submodule_path);
        if checkout_path.exists() {
            fs::remove_dir_all(&checkout_path).unwrap();
        }
        Repository::clone(&child_url, &checkout_path).unwrap();
        submodule.add_to_index(false).unwrap();
        submodule.add_finalize().unwrap();
    }
    commit_all(&parent, "add submodules");

    fs::write(parent_root.join("vendor/a-child/child.txt"), "dirty\n").unwrap();
    fs::write(parent_root.join("vendor/b-child/child.txt"), "dirty\n").unwrap();

    let limited = GitSnapshot::scan_with_status_options(
        &parent_root,
        DEFAULT_GIT_STATUS_LIMIT,
        false,
        true,
        1,
        DEFAULT_GIT_SIMILARITY_THRESHOLD,
    );
    let disabled = GitSnapshot::scan_with_status_options(
        &parent_root,
        DEFAULT_GIT_STATUS_LIMIT,
        false,
        false,
        DEFAULT_GIT_DETECT_SUBMODULES_LIMIT,
        DEFAULT_GIT_SIMILARITY_THRESHOLD,
    );

    let limited_paths = limited
        .entries()
        .into_iter()
        .map(|entry| entry.path)
        .collect::<Vec<_>>();
    assert!(
        limited_paths
            .iter()
            .any(|path| path.ends_with(Path::new("vendor").join("a-child")))
    );
    assert!(
        limited_paths
            .iter()
            .all(|path| !path.ends_with(Path::new("vendor").join("b-child")))
    );
    assert!(disabled.entries().is_empty());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn git_snapshot_reports_upstream_divergence() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-upstream-divergence-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let path = root.join("tracked.txt");
    fs::write(&path, "base\n").unwrap();
    commit_all(&repo, "base");
    let branch_name = repo.head().unwrap().shorthand().unwrap().to_owned();
    let upstream_name = format!("origin/{branch_name}");
    let upstream_ref = format!("refs/remotes/{upstream_name}");
    let base_oid = repo.head().unwrap().target().unwrap();
    repo.remote("origin", "https://example.com/repo.git")
        .unwrap();
    repo.reference(&upstream_ref, base_oid, true, "seed upstream")
        .unwrap();
    {
        let mut branch = repo.find_branch(&branch_name, BranchType::Local).unwrap();
        branch.set_upstream(Some(&upstream_name)).unwrap();
    }

    fs::write(&path, "local\n").unwrap();
    stage_path(&root, &path).unwrap();
    super::super::commit_staged_changes(&root, "local").unwrap();

    let base_commit = repo.find_commit(base_oid).unwrap();
    let tree = base_commit.tree().unwrap();
    let signature = Signature::now("Kuroya Test", "kuroya@example.com").unwrap();
    let remote_oid = repo
        .commit(
            None,
            &signature,
            &signature,
            "remote",
            &tree,
            &[&base_commit],
        )
        .unwrap();
    repo.reference(&upstream_ref, remote_oid, true, "move upstream")
        .unwrap();

    let divergence = GitSnapshot::scan(&root).remote_divergence().unwrap();

    assert_eq!(divergence.incoming, 1);
    assert_eq!(divergence.outgoing, 1);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn stage_path_moves_untracked_file_to_index() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-stage-path-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    Repository::init(&root).unwrap();
    let path = root.join("new.txt");
    fs::write(&path, "hello\n").unwrap();

    let before = GitSnapshot::scan(&root).entries();
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].status, GitFileStatus::Untracked);
    assert_eq!(before[0].stage, GitChangeStage::Unstaged);

    stage_path(&root, &path).unwrap();

    let after = GitSnapshot::scan(&root).entries();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].status, GitFileStatus::Added);
    assert_eq!(after[0].stage, GitChangeStage::Staged);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn stage_path_checks_relative_paths_against_worktree_root() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-stage-relative-path-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(root.join("nested")).unwrap();
    Repository::init(&root).unwrap();
    fs::write(root.join("nested/relative-stage.txt"), "hello\n").unwrap();

    stage_path(&root, Path::new("nested/relative-stage.txt")).unwrap();

    let after = GitSnapshot::scan(&root).entries();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].path, root.join("nested/relative-stage.txt"));
    assert_eq!(after[0].status, GitFileStatus::Added);
    assert_eq!(after[0].stage, GitChangeStage::Staged);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn stage_path_rejects_worktree_root() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-stage-root-path-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    Repository::init(&root).unwrap();

    let absolute = stage_path(&root, &root).unwrap_err().to_string();
    assert!(absolute.contains("worktree root"));

    let relative = stage_path(&root, Path::new(".")).unwrap_err().to_string();
    assert!(relative.contains("worktree root"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unstage_path_moves_added_file_back_to_untracked() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-unstage-path-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    Repository::init(&root).unwrap();
    let path = root.join("new.txt");
    fs::write(&path, "hello\n").unwrap();
    stage_path(&root, &path).unwrap();

    unstage_path(&root, &path).unwrap();

    let after = GitSnapshot::scan(&root).entries();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].status, GitFileStatus::Untracked);
    assert_eq!(after[0].stage, GitChangeStage::Unstaged);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn discard_path_restores_modified_file_from_head() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-discard-modified-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("tracked.txt");
    fs::write(&path, "original\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&path, "changed\n").unwrap();

    discard_path(&root, &path).unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap().replace("\r\n", "\n"),
        "original\n"
    );
    assert!(GitSnapshot::scan(&root).entries().is_empty());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn discard_path_removes_untracked_file() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-discard-untracked-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    Repository::init(&root).unwrap();
    let path = root.join("new.txt");
    fs::write(&path, "new\n").unwrap();

    discard_path(&root, &path).unwrap();

    assert!(!path.exists());
    assert!(GitSnapshot::scan(&root).entries().is_empty());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn discard_path_removes_staged_added_file() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-discard-staged-added-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    Repository::init(&root).unwrap();
    let path = root.join("new.txt");
    fs::write(&path, "new\n").unwrap();
    stage_path(&root, &path).unwrap();

    discard_path(&root, &path).unwrap();

    assert!(!path.exists());
    assert!(GitSnapshot::scan(&root).entries().is_empty());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn stage_path_can_resolve_merge_conflict_by_deleting_file() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-delete-resolution-merge-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let path = root.join("delete-me.txt");
    fs::write(&path, "base\n").unwrap();
    let base = commit_all_with_head_parent(&repo, "base");
    let branch_name = repo.head().unwrap().shorthand().unwrap().to_owned();
    {
        let base_commit = repo.find_commit(base).unwrap();
        repo.branch("feature", &base_commit, false).unwrap();
    }

    fs::remove_file(&path).unwrap();
    stage_path(&root, &path).unwrap();
    super::super::commit_staged_changes(&root, "delete").unwrap();
    let old_head = repo.head().unwrap().target().unwrap();
    checkout_branch(&repo, "feature");
    fs::write(&path, "theirs\n").unwrap();
    let merged = commit_all_with_head_parent(&repo, "modify");
    checkout_branch(&repo, &branch_name);
    merge_commit_into_head(&repo, merged);
    assert_eq!(repo.state(), RepositoryState::Merge);

    if path.exists() {
        fs::remove_file(&path).unwrap();
    }
    stage_path(&root, &path).unwrap();
    super::super::commit_staged_changes(&root, "merge feature").unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();

    assert_eq!(repo.state(), RepositoryState::Clean);
    assert_eq!(head.parent_count(), 2);
    assert_eq!(head.parent_id(0).unwrap(), old_head);
    assert_eq!(head.parent_id(1).unwrap(), merged);
    assert!(head.tree().unwrap().get_name("delete-me.txt").is_none());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn stage_paths_stages_both_sides_of_an_unstaged_rename() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-git-stage-rename-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let old = root.join("r1.txt");
    fs::write(&old, "same content\n").unwrap();
    commit_all(&repo, "initial");

    let new = root.join("r2.txt");
    fs::rename(&old, &new).unwrap();

    stage_paths(&root, [new.as_path()]).unwrap();

    let staged = GitSnapshot::scan(&root).entries();
    assert_eq!(staged.len(), 1, "a staged rename is a single entry");
    assert_eq!(staged[0].status, GitFileStatus::Renamed);
    assert_eq!(staged[0].stage, GitChangeStage::Staged);
    assert_eq!(file_text_at_index(&root, &old).unwrap(), None);
    assert_eq!(
        file_text_at_index(&root, &new).unwrap(),
        Some("same content\n".to_owned())
    );

    super::super::commit_staged_changes(&root, "rename r1 to r2").unwrap();
    let commit = repo.head().unwrap().peel_to_commit().unwrap();
    let parent_tree = commit.parent(0).unwrap().tree().unwrap();
    let commit_tree = commit.tree().unwrap();
    assert!(commit_tree.get_path(Path::new("r2.txt")).is_ok());
    assert!(commit_tree.get_path(Path::new("r1.txt")).is_err());

    let mut find_options = git2::DiffFindOptions::new();
    find_options.renames(true);
    let mut diff = repo
        .diff_tree_to_tree(
            Some(&parent_tree),
            Some(&commit_tree),
            Some(&mut git2::DiffOptions::new()),
        )
        .unwrap();
    diff.find_similar(Some(&mut find_options)).unwrap();
    let deltas = diff.deltas().collect::<Vec<_>>();
    assert_eq!(deltas.len(), 1, "the commit is a rename, not add + delete");
    assert_eq!(deltas[0].status(), git2::Delta::Renamed);
    assert_eq!(deltas[0].old_file().path(), Some(Path::new("r1.txt")));
    assert_eq!(deltas[0].new_file().path(), Some(Path::new("r2.txt")));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unstage_paths_restores_both_sides_of_a_staged_rename() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-git-unstage-rename-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
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
    assert_eq!(staged.len(), 1, "precondition: a staged rename");
    assert_eq!(staged[0].status, GitFileStatus::Renamed);
    assert_eq!(staged[0].stage, GitChangeStage::Staged);

    unstage_paths(&root, [new.as_path()]).unwrap();

    assert_eq!(
        file_text_at_index(&root, &old).unwrap(),
        Some("same content\n".to_owned())
    );
    assert_eq!(file_text_at_index(&root, &new).unwrap(), None);
    assert!(!old.exists());
    assert!(new.exists());

    let entries = GitSnapshot::scan(&root).entries();
    assert_eq!(entries.len(), 1, "the rename is unstaged again");
    assert_eq!(entries[0].status, GitFileStatus::Renamed);
    assert_eq!(entries[0].stage, GitChangeStage::Unstaged);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unstage_all_paths_unstages_a_rename_and_a_modify_together() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-git-unstage-all-rename-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let modified = root.join("base.txt");
    fs::write(&modified, "one\n").unwrap();
    let old = root.join("r1.txt");
    fs::write(&old, "same content\n").unwrap();
    commit_all(&repo, "initial");

    fs::write(&modified, "two\n").unwrap();
    stage_path(&root, &modified).unwrap();
    let new = root.join("r2.txt");
    fs::rename(&old, &new).unwrap();
    stage_paths(&root, [old.as_path(), new.as_path()]).unwrap();
    let staged = GitSnapshot::scan(&root).entries();
    assert_eq!(staged.len(), 2, "precondition: staged modify plus rename");

    unstage_paths(&root, [modified.as_path(), new.as_path()]).unwrap();

    assert_eq!(
        file_text_at_index(&root, &modified).unwrap(),
        Some("one\n".to_owned())
    );
    assert_eq!(
        file_text_at_index(&root, &old).unwrap(),
        Some("same content\n".to_owned())
    );
    assert_eq!(file_text_at_index(&root, &new).unwrap(), None);

    let mut entries = GitSnapshot::scan(&root).entries();
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    assert_eq!(entries.len(), 2, "everything is unstaged again");
    assert!(
        entries
            .iter()
            .all(|entry| entry.stage == GitChangeStage::Unstaged)
    );
    assert_eq!(entries[0].status, GitFileStatus::Modified);
    assert_eq!(entries[1].status, GitFileStatus::Renamed);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn stage_and_unstage_paths_keep_a_normal_modify_single_sided() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-git-stage-normal-modify-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let tracked = root.join("tracked.txt");
    fs::write(&tracked, "one\n").unwrap();
    commit_all(&repo, "initial");

    fs::write(&tracked, "two\n").unwrap();
    stage_path(&root, &tracked).unwrap();
    let staged = GitSnapshot::scan(&root).entries();
    assert_eq!(staged.len(), 1);
    assert_eq!(staged[0].status, GitFileStatus::Modified);
    assert_eq!(staged[0].stage, GitChangeStage::Staged);
    assert_eq!(
        file_text_at_index(&root, &tracked).unwrap(),
        Some("two\n".to_owned())
    );

    unstage_path(&root, &tracked).unwrap();
    assert_eq!(
        file_text_at_index(&root, &tracked).unwrap(),
        Some("one\n".to_owned())
    );
    let unstaged = GitSnapshot::scan(&root).entries();
    assert_eq!(unstaged.len(), 1);
    assert_eq!(unstaged[0].status, GitFileStatus::Modified);
    assert_eq!(unstaged[0].stage, GitChangeStage::Unstaged);
    assert_eq!(
        fs::read_to_string(&tracked).unwrap().replace("\r\n", "\n"),
        "two\n"
    );

    fs::remove_dir_all(root).unwrap();
}
