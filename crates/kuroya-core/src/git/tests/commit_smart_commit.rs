use super::*;

#[test]
fn commit_staged_changes_creates_commit_from_index() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-commit-staged-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let path = root.join("new.txt");
    fs::write(&path, "new\n").unwrap();
    stage_path(&root, &path).unwrap();

    let short_oid = super::super::commit_staged_changes(&root, "add new file").unwrap();

    assert_eq!(short_oid.len(), DEFAULT_GIT_COMMIT_SHORT_HASH_LENGTH);
    assert_eq!(
        repo.head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .message()
            .unwrap(),
        "add new file"
    );
    assert!(GitSnapshot::scan(&root).entries().is_empty());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn merge_head_commits_reads_multiple_oids_in_file_order() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-merge-head-order-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("one.txt"), "one\n").unwrap();
    let first = commit_all_with_head_parent(&repo, "first");
    fs::write(root.join("two.txt"), "two\n").unwrap();
    let second = commit_all_with_head_parent(&repo, "second");
    fs::write(
        repo.path().join("MERGE_HEAD"),
        format!("{first}\n{second}\n"),
    )
    .unwrap();

    let commits = super::super::merge_head_commits(&repo).unwrap();

    assert_eq!(
        commits.iter().map(|commit| commit.id()).collect::<Vec<_>>(),
        vec![first, second]
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn merge_head_commits_rejects_invalid_oid() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-merge-head-invalid-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    fs::write(repo.path().join("MERGE_HEAD"), "not-an-oid\n").unwrap();

    let error = super::super::merge_head_commits(&repo).unwrap_err();

    assert!(error.to_string().contains("could not parse"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn commit_staged_changes_creates_merge_commit_from_merge_head() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-commit-merge-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("base.txt"), "base\n").unwrap();
    let base = commit_all_with_head_parent(&repo, "base");
    let branch_name = repo.head().unwrap().shorthand().unwrap().to_owned();
    {
        let base_commit = repo.find_commit(base).unwrap();
        repo.branch("feature", &base_commit, false).unwrap();
    }

    fs::write(root.join("main.txt"), "main\n").unwrap();
    let old_head = commit_all_with_head_parent(&repo, "main");
    checkout_branch(&repo, "feature");
    fs::write(root.join("feature.txt"), "feature\n").unwrap();
    let merged = commit_all_with_head_parent(&repo, "feature");
    checkout_branch(&repo, &branch_name);
    merge_commit_into_head(&repo, merged);
    assert_eq!(repo.state(), RepositoryState::Merge);

    let short_oid = super::super::commit_staged_changes(&root, "merge feature").unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();

    assert_eq!(short_oid.len(), DEFAULT_GIT_COMMIT_SHORT_HASH_LENGTH);
    assert_eq!(repo.state(), RepositoryState::Clean);
    assert_eq!(head.parent_count(), 2);
    assert_eq!(head.parent_id(0).unwrap(), old_head);
    assert_eq!(head.parent_id(1).unwrap(), merged);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn commit_changes_allows_same_tree_merge_resolution() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-commit-same-tree-merge-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let path = root.join("tracked.txt");
    fs::write(&path, "base\n").unwrap();
    let base = commit_all_with_head_parent(&repo, "base");
    let branch_name = repo.head().unwrap().shorthand().unwrap().to_owned();
    {
        let base_commit = repo.find_commit(base).unwrap();
        repo.branch("feature", &base_commit, false).unwrap();
    }

    fs::write(&path, "ours\n").unwrap();
    let old_head = commit_all_with_head_parent(&repo, "ours");
    let old_head_tree = repo.find_commit(old_head).unwrap().tree_id();
    checkout_branch(&repo, "feature");
    fs::write(&path, "theirs\n").unwrap();
    let merged = commit_all_with_head_parent(&repo, "theirs");
    checkout_branch(&repo, &branch_name);
    merge_commit_into_head(&repo, merged);
    assert_eq!(repo.state(), RepositoryState::Merge);

    fs::write(&path, "ours\n").unwrap();
    stage_path(&root, &path).unwrap();
    fs::write(root.join("base.txt"), "unrelated worktree edit\n").unwrap();
    fs::write(root.join("untracked.txt"), "untracked\n").unwrap();

    super::super::commit_changes(&root, "merge feature", Some(GitSmartCommitChanges::All)).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();

    assert_eq!(repo.state(), RepositoryState::Clean);
    assert_eq!(head.parent_count(), 2);
    assert_eq!(head.parent_id(0).unwrap(), old_head);
    assert_eq!(head.parent_id(1).unwrap(), merged);
    assert_eq!(head.tree_id(), old_head_tree);
    assert!(head.tree().unwrap().get_name("untracked.txt").is_none());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn commit_changes_can_sign_off_with_git_identity() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-commit-sign-off-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let path = root.join("new.txt");
    fs::write(&path, "new\n").unwrap();
    stage_path(&root, &path).unwrap();

    let short_oid = super::super::commit_changes_with_options(
        &root,
        "add new file",
        None,
        DEFAULT_GIT_COMMIT_SHORT_HASH_LENGTH,
        true,
    )
    .unwrap();

    assert_eq!(short_oid.len(), DEFAULT_GIT_COMMIT_SHORT_HASH_LENGTH);
    assert_eq!(
        repo.head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .message()
            .unwrap(),
        "add new file\n\nSigned-off-by: Kuroya Test <kuroya@example.com>"
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn commit_changes_can_guess_identity_when_user_config_not_required() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-commit-guess-identity-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("new.txt");
    fs::write(&path, "new\n").unwrap();
    stage_path(&root, &path).unwrap();

    let short_oid = super::super::commit_changes_with_user_config_option(
        &root,
        "add new file",
        None,
        DEFAULT_GIT_COMMIT_SHORT_HASH_LENGTH,
        false,
        false,
        false,
    )
    .unwrap();

    assert_eq!(short_oid.len(), DEFAULT_GIT_COMMIT_SHORT_HASH_LENGTH);
    assert_eq!(
        repo.head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .message()
            .unwrap(),
        "add new file"
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn guessed_git_signature_uses_clean_environment_values() {
    let fake_env = |key: &str| match key {
        "GIT_AUTHOR_NAME" => Some("  Jane <Doe>  ".to_owned()),
        "COMPUTERNAME" => Some(" work station ".to_owned()),
        _ => None,
    };

    assert_eq!(
        super::super::guessed_git_signature_name_from(fake_env),
        "Jane Doe"
    );
    assert_eq!(
        super::super::guessed_git_signature_email_from("Jane Doe", fake_env),
        "Jane-Doe@work-station"
    );
}

#[test]
fn commit_staged_changes_rejects_empty_message() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-commit-empty-message-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    Repository::init(&root).unwrap();

    let error = super::super::commit_staged_changes(&root, "  ").unwrap_err();

    assert!(error.to_string().contains("commit message cannot be empty"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn commit_staged_changes_rejects_clean_index() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-commit-clean-index-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let path = root.join("tracked.txt");
    fs::write(&path, "tracked\n").unwrap();
    commit_all(&repo, "initial");

    let error = super::super::commit_staged_changes(&root, "nothing").unwrap_err();

    assert!(error.to_string().contains("no staged changes to commit"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn commit_changes_can_create_empty_commit_when_allowed() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-commit-empty-allowed-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let path = root.join("tracked.txt");
    fs::write(&path, "tracked\n").unwrap();
    commit_all(&repo, "initial");
    let parent = repo.head().unwrap().peel_to_commit().unwrap();

    let short_oid = super::super::commit_changes_with_empty_commit_option(
        &root,
        "empty checkpoint",
        None,
        DEFAULT_GIT_COMMIT_SHORT_HASH_LENGTH,
        false,
        true,
    )
    .unwrap();
    let commit = repo.head().unwrap().peel_to_commit().unwrap();

    assert_eq!(short_oid.len(), DEFAULT_GIT_COMMIT_SHORT_HASH_LENGTH);
    assert_eq!(commit.message().unwrap(), "empty checkpoint");
    assert_eq!(commit.tree_id(), parent.tree_id());
    assert_eq!(commit.parent_id(0).unwrap(), parent.id());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn commit_changes_smart_commit_stages_all_when_index_is_clean() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-smart-commit-all-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let tracked = root.join("tracked.txt");
    let untracked = root.join("new.txt");
    fs::write(&tracked, "tracked\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&tracked, "changed\n").unwrap();
    fs::write(&untracked, "new\n").unwrap();

    let short_oid =
        super::super::commit_changes(&root, "smart commit", Some(GitSmartCommitChanges::All))
            .unwrap();

    assert_eq!(short_oid.len(), DEFAULT_GIT_COMMIT_SHORT_HASH_LENGTH);
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.message().unwrap(), "smart commit");
    assert!(head.tree().unwrap().get_name("new.txt").is_some());
    assert!(GitSnapshot::scan(&root).entries().is_empty());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn commit_changes_smart_commit_tracked_skips_untracked_files() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-smart-commit-tracked-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let tracked = root.join("tracked.txt");
    let untracked = root.join("new.txt");
    fs::write(&tracked, "tracked\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&tracked, "changed\n").unwrap();
    fs::write(&untracked, "new\n").unwrap();

    super::super::commit_changes(
        &root,
        "smart commit tracked",
        Some(GitSmartCommitChanges::Tracked),
    )
    .unwrap();

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.message().unwrap(), "smart commit tracked");
    assert!(head.tree().unwrap().get_name("new.txt").is_none());
    assert_eq!(GitSnapshot::scan(&root).entries().len(), 1);

    fs::remove_dir_all(root).unwrap();
}
