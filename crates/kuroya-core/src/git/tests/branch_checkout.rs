use super::*;

#[test]
fn list_local_branches_marks_current_branch_first() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-list-branches-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("tracked.txt"), "tracked\n").unwrap();
    commit_all(&repo, "initial");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &head, false).unwrap();

    let branches = list_local_branches(&root).unwrap();

    assert!(branches.iter().any(|branch| branch.name == "feature"));
    assert!(
        branches.iter().any(|branch| branch.name == "feature"
            && branch.committer_time_seconds == head.time().seconds())
    );
    assert!(branches.first().is_some_and(|branch| branch.is_current));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn checkout_branch_switches_head_to_local_branch() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-checkout-branch-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("tracked.txt"), "tracked\n").unwrap();
    commit_all(&repo, "initial");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &head, false).unwrap();

    super::super::checkout_branch(&root, "feature").unwrap();

    assert_eq!(repo.head().unwrap().shorthand(), Ok("feature"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn branch_mutation_block_label_covers_in_progress_repository_states() {
    let blocked = [
        (RepositoryState::Merge, "merge"),
        (RepositoryState::Revert, "revert"),
        (RepositoryState::RevertSequence, "revert"),
        (RepositoryState::CherryPick, "cherry-pick"),
        (RepositoryState::CherryPickSequence, "cherry-pick"),
        (RepositoryState::Bisect, "bisect"),
        (RepositoryState::Rebase, "rebase"),
        (RepositoryState::RebaseInteractive, "rebase"),
        (RepositoryState::RebaseMerge, "rebase"),
        (RepositoryState::ApplyMailbox, "apply"),
        (RepositoryState::ApplyMailboxOrRebase, "apply or rebase"),
    ];

    assert_eq!(
        super::super::repository_branch_mutation_block_label(RepositoryState::Clean),
        None
    );
    for (state, label) in blocked {
        assert_eq!(
            super::super::repository_branch_mutation_block_label(state),
            Some(label)
        );
    }
}

#[test]
fn checkout_branch_rejects_merge_state_and_leaves_head_unchanged() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-checkout-branch-merge-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("base.txt"), "base\n").unwrap();
    let base = commit_all_with_head_parent(&repo, "base");
    let branch_name = repo.head().unwrap().shorthand().unwrap().to_owned();
    let base_commit = repo.find_commit(base).unwrap();
    repo.branch("target", &base_commit, false).unwrap();
    repo.branch("feature", &base_commit, false).unwrap();
    fs::write(root.join("main.txt"), "main\n").unwrap();
    let original_head = commit_all_with_head_parent(&repo, "main");
    checkout_branch(&repo, "feature");
    fs::write(root.join("feature.txt"), "feature\n").unwrap();
    let merged = commit_all_with_head_parent(&repo, "feature");
    checkout_branch(&repo, &branch_name);
    merge_commit_into_head(&repo, merged);
    assert_eq!(repo.state(), RepositoryState::Merge);

    let error = super::super::checkout_branch(&root, "target")
        .unwrap_err()
        .to_string();

    assert!(error.contains("cannot switch or create branches while merge is in progress"));
    assert_eq!(repo.head().unwrap().shorthand(), Ok(branch_name.as_str()));
    assert_eq!(repo.head().unwrap().target(), Some(original_head));
    assert_eq!(repo.state(), RepositoryState::Merge);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn list_checkout_refs_respects_checkout_type_filter() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-checkout-type-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("tracked.txt"), "tracked\n").unwrap();
    commit_all(&repo, "initial");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &head, false).unwrap();
    repo.reference("refs/remotes/origin/main", head.id(), true, "remote")
        .unwrap();
    repo.reference("refs/remotes/origin/HEAD", head.id(), true, "remote")
        .unwrap();
    repo.tag_lightweight("v1.0", head.as_object(), false)
        .unwrap();

    let local = list_checkout_refs(&root, &[GitCheckoutType::Local]).unwrap();
    let remote_and_tags =
        list_checkout_refs(&root, &[GitCheckoutType::Remote, GitCheckoutType::Tags]).unwrap();

    assert!(
        local
            .iter()
            .any(|branch| { branch.name == "feature" && branch.kind == GitCheckoutType::Local })
    );
    assert!(
        local
            .iter()
            .all(|branch| branch.kind == GitCheckoutType::Local)
    );
    assert!(
        remote_and_tags.iter().any(|branch| {
            branch.name == "origin/main" && branch.kind == GitCheckoutType::Remote
        })
    );
    assert!(
        remote_and_tags
            .iter()
            .any(|branch| { branch.name == "v1.0" && branch.kind == GitCheckoutType::Tags })
    );
    assert!(
        remote_and_tags
            .iter()
            .all(|branch| branch.name != "origin/HEAD")
    );
    assert!(
        remote_and_tags
            .iter()
            .all(|branch| branch.kind != GitCheckoutType::Local)
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn checkout_ref_rejects_remote_branch_during_merge_without_creating_local_branch() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-checkout-remote-merge-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("base.txt"), "base\n").unwrap();
    let base = commit_all_with_head_parent(&repo, "base");
    let branch_name = repo.head().unwrap().shorthand().unwrap().to_owned();
    let base_commit = repo.find_commit(base).unwrap();
    repo.branch("feature", &base_commit, false).unwrap();
    fs::write(root.join("main.txt"), "main\n").unwrap();
    let original_head = commit_all_with_head_parent(&repo, "main");
    checkout_branch(&repo, "feature");
    fs::write(root.join("feature.txt"), "feature\n").unwrap();
    let merged = commit_all_with_head_parent(&repo, "feature");
    checkout_branch(&repo, &branch_name);
    repo.reference("refs/remotes/origin/remote-target", merged, true, "remote")
        .unwrap();
    merge_commit_into_head(&repo, merged);
    assert_eq!(repo.state(), RepositoryState::Merge);

    let error = checkout_ref(&root, "origin/remote-target", GitCheckoutType::Remote)
        .unwrap_err()
        .to_string();

    assert!(error.contains("cannot switch or create branches while merge is in progress"));
    assert_eq!(repo.head().unwrap().shorthand(), Ok(branch_name.as_str()));
    assert_eq!(repo.head().unwrap().target(), Some(original_head));
    assert!(
        repo.find_branch("remote-target", BranchType::Local)
            .is_err()
    );
    assert_eq!(repo.state(), RepositoryState::Merge);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn checkout_ref_switches_remote_branch_to_local_branch() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-checkout-remote-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("tracked.txt"), "tracked\n").unwrap();
    commit_all(&repo, "initial");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.reference("refs/remotes/origin/feature", head.id(), true, "remote")
        .unwrap();

    checkout_ref(&root, "origin/feature", GitCheckoutType::Remote).unwrap();

    assert_eq!(repo.head().unwrap().shorthand(), Ok("feature"));
    assert!(repo.find_branch("feature", BranchType::Local).is_ok());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn checkout_ref_rolls_back_new_remote_local_branch_when_checkout_fails() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-checkout-remote-rollback-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("tracked.txt"), "main\n").unwrap();
    commit_all(&repo, "initial");
    let main_branch = repo.head().unwrap().shorthand().unwrap().to_owned();
    let main = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature-source", &main, false).unwrap();
    checkout_branch(&repo, "feature-source");
    fs::write(root.join("tracked.txt"), "feature\n").unwrap();
    let feature_id = commit_all_with_head_parent(&repo, "feature");
    repo.reference("refs/remotes/origin/feature", feature_id, true, "remote")
        .unwrap();
    checkout_branch(&repo, &main_branch);
    fs::write(root.join("tracked.txt"), "local dirty\n").unwrap();

    let error = checkout_ref(&root, "origin/feature", GitCheckoutType::Remote)
        .unwrap_err()
        .to_string();

    assert!(!error.is_empty());
    assert_eq!(repo.head().unwrap().shorthand(), Ok(main_branch.as_str()));
    assert!(repo.find_branch("feature", BranchType::Local).is_err());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn checkout_ref_rejects_tag_checkout_during_merge() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-checkout-tag-merge-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("base.txt"), "base\n").unwrap();
    let base = commit_all_with_head_parent(&repo, "base");
    let branch_name = repo.head().unwrap().shorthand().unwrap().to_owned();
    let base_commit = repo.find_commit(base).unwrap();
    repo.tag_lightweight("v1.0", base_commit.as_object(), false)
        .unwrap();
    repo.branch("feature", &base_commit, false).unwrap();
    fs::write(root.join("main.txt"), "main\n").unwrap();
    let original_head = commit_all_with_head_parent(&repo, "main");
    checkout_branch(&repo, "feature");
    fs::write(root.join("feature.txt"), "feature\n").unwrap();
    let merged = commit_all_with_head_parent(&repo, "feature");
    checkout_branch(&repo, &branch_name);
    merge_commit_into_head(&repo, merged);
    assert_eq!(repo.state(), RepositoryState::Merge);

    let error = checkout_ref(&root, "v1.0", GitCheckoutType::Tags)
        .unwrap_err()
        .to_string();

    assert!(error.contains("cannot switch or create branches while merge is in progress"));
    assert_eq!(repo.head().unwrap().shorthand(), Ok(branch_name.as_str()));
    assert_eq!(repo.head().unwrap().target(), Some(original_head));
    assert_eq!(repo.state(), RepositoryState::Merge);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn checkout_ref_can_detach_head_at_tag() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-checkout-tag-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("tracked.txt"), "tracked\n").unwrap();
    commit_all(&repo, "initial");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.tag_lightweight("v1.0", head.as_object(), false)
        .unwrap();

    checkout_ref(&root, "v1.0", GitCheckoutType::Tags).unwrap();

    let checked_out = repo.head().unwrap();
    assert!(!checked_out.is_branch());
    assert_eq!(checked_out.target(), Some(head.id()));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn create_branch_rejects_merge_state_without_creating_branch() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-create-branch-merge-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("base.txt"), "base\n").unwrap();
    let base = commit_all_with_head_parent(&repo, "base");
    let branch_name = repo.head().unwrap().shorthand().unwrap().to_owned();
    let base_commit = repo.find_commit(base).unwrap();
    repo.branch("feature", &base_commit, false).unwrap();
    fs::write(root.join("main.txt"), "main\n").unwrap();
    let original_head = commit_all_with_head_parent(&repo, "main");
    checkout_branch(&repo, "feature");
    fs::write(root.join("feature.txt"), "feature\n").unwrap();
    let merged = commit_all_with_head_parent(&repo, "feature");
    checkout_branch(&repo, &branch_name);
    merge_commit_into_head(&repo, merged);
    assert_eq!(repo.state(), RepositoryState::Merge);

    let error = super::super::create_branch(&root, "feature/search")
        .unwrap_err()
        .to_string();

    assert!(error.contains("cannot switch or create branches while merge is in progress"));
    assert_eq!(repo.head().unwrap().shorthand(), Ok(branch_name.as_str()));
    assert_eq!(repo.head().unwrap().target(), Some(original_head));
    assert!(
        repo.find_branch("feature/search", BranchType::Local)
            .is_err()
    );
    assert_eq!(repo.state(), RepositoryState::Merge);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn create_branch_rolls_back_new_branch_when_set_head_fails() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-create-branch-set-head-fail-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("tracked.txt"), "tracked\n").unwrap();
    commit_all(&repo, "initial");
    let original_head = repo.head().unwrap().target().unwrap();
    let original_branch = repo.head().unwrap().shorthand().unwrap().to_owned();
    let head_lock = repo.path().join("HEAD.lock");
    fs::write(&head_lock, "locked").unwrap();

    let error = super::super::create_branch(&root, "feature/search").unwrap_err();

    assert!(!error.to_string().is_empty());
    assert_eq!(
        repo.head().unwrap().shorthand(),
        Ok(original_branch.as_str())
    );
    assert_eq!(repo.head().unwrap().target(), Some(original_head));
    assert!(
        repo.find_branch("feature/search", BranchType::Local)
            .is_err()
    );

    fs::remove_file(head_lock).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn create_branch_creates_and_switches_to_new_local_branch() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-create-branch-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("tracked.txt"), "tracked\n").unwrap();
    commit_all(&repo, "initial");

    super::super::create_branch(&root, "feature/search").unwrap();

    assert_eq!(repo.head().unwrap().shorthand(), Ok("feature/search"));
    let branches = list_local_branches(&root).unwrap();
    assert!(
        branches
            .iter()
            .any(|branch| branch.name == "feature/search")
    );
    assert!(
        branches
            .iter()
            .any(|branch| branch.name == "feature/search" && branch.is_current)
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn create_branch_rejects_existing_branch() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-create-existing-branch-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("tracked.txt"), "tracked\n").unwrap();
    commit_all(&repo, "initial");
    super::super::create_branch(&root, "feature/search").unwrap();

    let error = super::super::create_branch(&root, "feature/search").unwrap_err();

    assert!(
        error
            .to_string()
            .contains("branch feature/search already exists")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn delete_branch_removes_non_current_local_branch() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-delete-branch-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("tracked.txt"), "tracked\n").unwrap();
    commit_all(&repo, "initial");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature/search", &head, false).unwrap();

    delete_branch(&root, "feature/search").unwrap();

    let branches = list_local_branches(&root).unwrap();
    assert!(
        !branches
            .iter()
            .any(|branch| branch.name == "feature/search")
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn delete_branch_rejects_current_branch() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-delete-current-branch-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("tracked.txt"), "tracked\n").unwrap();
    commit_all(&repo, "initial");

    let current = repo.head().unwrap().shorthand().unwrap().to_owned();
    let error = delete_branch(&root, &current).unwrap_err();

    assert!(
        error
            .to_string()
            .contains(&format!("cannot delete the current branch {current}"))
    );
    assert!(
        list_local_branches(&root)
            .unwrap()
            .iter()
            .any(|branch| { branch.name == current && branch.is_current })
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rename_branch_updates_non_current_local_branch() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-rename-branch-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("tracked.txt"), "tracked\n").unwrap();
    commit_all(&repo, "initial");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature/search", &head, false).unwrap();

    rename_branch(&root, "feature/search", "feature/find").unwrap();

    let branches = list_local_branches(&root).unwrap();
    assert!(
        !branches
            .iter()
            .any(|branch| branch.name == "feature/search")
    );
    assert!(branches.iter().any(|branch| branch.name == "feature/find"));
    assert!(branches.iter().any(|branch| branch.is_current));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rename_branch_updates_current_branch_head() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-rename-current-branch-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("tracked.txt"), "tracked\n").unwrap();
    commit_all(&repo, "initial");
    let current = repo.head().unwrap().shorthand().unwrap().to_owned();

    rename_branch(&root, &current, "main-renamed").unwrap();

    assert_eq!(repo.head().unwrap().shorthand(), Ok("main-renamed"));
    let branches = list_local_branches(&root).unwrap();
    assert!(
        branches
            .iter()
            .any(|branch| branch.name == "main-renamed" && branch.is_current)
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rename_branch_rejects_existing_target() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-rename-existing-branch-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("tracked.txt"), "tracked\n").unwrap();
    commit_all(&repo, "initial");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature/search", &head, false).unwrap();
    repo.branch("feature/find", &head, false).unwrap();

    let error = rename_branch(&root, "feature/search", "feature/find").unwrap_err();

    assert!(
        error
            .to_string()
            .contains("branch feature/find already exists")
    );

    fs::remove_dir_all(root).unwrap();
}
