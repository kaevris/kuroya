use super::*;

#[test]
fn save_stash_records_worktree_changes_and_lists_entry() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-save-stash-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let tracked = root.join("tracked.txt");
    let untracked = root.join("new.txt");
    fs::write(&tracked, "original\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&tracked, "changed\n").unwrap();
    fs::write(&untracked, "new\n").unwrap();

    let short_oid = super::super::save_stash(&root, "wip edits").unwrap();
    let stashes = list_stashes(&root).unwrap();

    assert_eq!(short_oid.len(), DEFAULT_GIT_COMMIT_SHORT_HASH_LENGTH);
    assert_eq!(stashes.len(), 1);
    assert_eq!(stashes[0].index, 0);
    assert_eq!(
        stashes[0].short_oid.len(),
        DEFAULT_GIT_COMMIT_SHORT_HASH_LENGTH
    );
    assert!(stashes[0].message.contains("wip edits"));
    assert_eq!(
        fs::read_to_string(&tracked).unwrap().replace("\r\n", "\n"),
        "original\n"
    );
    assert!(!untracked.exists());
    assert!(GitSnapshot::scan(&root).entries().is_empty());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn save_stash_can_guess_identity_when_user_config_not_required() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-save-stash-guess-identity-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let tracked = root.join("tracked.txt");
    fs::write(&tracked, "original\n").unwrap();
    commit_all(&repo, "initial");
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "").unwrap();
    config.set_str("user.email", "").unwrap();
    drop(config);
    fs::write(&tracked, "changed\n").unwrap();

    assert!(Repository::discover(&root).unwrap().signature().is_err());
    assert!(
        super::super::save_stash_with_user_config_option(
            &root,
            "wip edits",
            DEFAULT_GIT_COMMIT_SHORT_HASH_LENGTH,
            true,
        )
        .is_err()
    );

    let short_oid = super::super::save_stash_with_user_config_option(
        &root,
        "wip edits",
        DEFAULT_GIT_COMMIT_SHORT_HASH_LENGTH,
        false,
    )
    .unwrap();
    let stashes = list_stashes(&root).unwrap();

    assert_eq!(short_oid.len(), DEFAULT_GIT_COMMIT_SHORT_HASH_LENGTH);
    assert_eq!(stashes.len(), 1);
    assert!(stashes[0].message.contains("wip edits"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn apply_and_drop_stash_restores_changes_and_removes_entry() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-apply-drop-stash-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let tracked = root.join("tracked.txt");
    let untracked = root.join("new.txt");
    fs::write(&tracked, "original\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&tracked, "changed\n").unwrap();
    fs::write(&untracked, "new\n").unwrap();
    super::super::save_stash(&root, "wip edits").unwrap();

    super::super::apply_stash(&root, 0).unwrap();

    assert_eq!(
        fs::read_to_string(&tracked).unwrap().replace("\r\n", "\n"),
        "changed\n"
    );
    assert_eq!(
        fs::read_to_string(&untracked)
            .unwrap()
            .replace("\r\n", "\n"),
        "new\n"
    );

    super::super::drop_stash(&root, 0).unwrap();

    assert!(list_stashes(&root).unwrap().is_empty());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn pop_stash_restores_changes_and_removes_entry() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-pop-stash-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let tracked = root.join("tracked.txt");
    let untracked = root.join("new.txt");
    fs::write(&tracked, "original\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&tracked, "changed\n").unwrap();
    fs::write(&untracked, "new\n").unwrap();
    super::super::save_stash(&root, "wip edits").unwrap();

    super::super::pop_stash(&root, 0).unwrap();

    assert_eq!(
        fs::read_to_string(&tracked).unwrap().replace("\r\n", "\n"),
        "changed\n"
    );
    assert_eq!(
        fs::read_to_string(&untracked)
            .unwrap()
            .replace("\r\n", "\n"),
        "new\n"
    );
    assert!(list_stashes(&root).unwrap().is_empty());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unified_diff_for_stash_returns_patch_against_stash_base() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-stash-diff-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let tracked = root.join("tracked.txt");
    let untracked = root.join("new.txt");
    fs::write(&tracked, "original\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&tracked, "changed\n").unwrap();
    fs::write(&untracked, "new\n").unwrap();
    super::super::save_stash(&root, "preview edits").unwrap();

    let diff = unified_diff_for_stash(&root, 0).unwrap();

    assert!(diff.contains("diff --git a/tracked.txt b/tracked.txt"));
    assert!(diff.contains("-original\n"));
    assert!(diff.contains("+changed\n"));
    assert!(diff.contains("diff --git a/new.txt b/new.txt"));
    assert!(diff.contains("+new\n"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn list_commit_history_returns_head_history_with_metadata() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-commit-history-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let path = root.join("tracked.txt");
    fs::write(&path, "one\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&path, "two\n").unwrap();
    stage_path(&root, &path).unwrap();
    super::super::commit_staged_changes(&root, "second").unwrap();

    let commits = list_commit_history(&root, 8).unwrap();

    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].summary, "second");
    assert_eq!(commits[0].author, "Kuroya Test");
    assert_eq!(
        commits[0].short_oid.len(),
        DEFAULT_GIT_COMMIT_SHORT_HASH_LENGTH
    );
    assert!(commits.iter().any(|commit| commit.summary == "initial"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn list_commit_history_respects_short_hash_length() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-commit-history-hash-length-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("tracked.txt"), "one\n").unwrap();
    commit_all(&repo, "initial");

    let commits = list_commit_history_with_short_hash_length(&root, 1, 12).unwrap();

    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].short_oid.len(), 12);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn list_commit_history_can_use_authored_or_committed_date() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-commit-history-timeline-date-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("tracked.txt"), "one\n").unwrap();
    stage_path(&root, &root.join("tracked.txt")).unwrap();
    let mut index = repo.index().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let author = Signature::new(
        "Kuroya Test",
        "kuroya@example.com",
        &git2::Time::new(1_000, 0),
    )
    .unwrap();
    let committer = Signature::new(
        "Kuroya Test",
        "kuroya@example.com",
        &git2::Time::new(2_000, 0),
    )
    .unwrap();
    repo.commit(Some("HEAD"), &author, &committer, "dated", &tree, &[])
        .unwrap();

    let committed = list_commit_history_with_timeline_date(
        &root,
        1,
        DEFAULT_GIT_COMMIT_SHORT_HASH_LENGTH,
        GitTimelineDate::Committed,
    )
    .unwrap();
    let authored = list_commit_history_with_timeline_date(
        &root,
        1,
        DEFAULT_GIT_COMMIT_SHORT_HASH_LENGTH,
        GitTimelineDate::Authored,
    )
    .unwrap();

    assert_eq!(committed[0].time_seconds, 2_000);
    assert_eq!(authored[0].time_seconds, 1_000);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn list_commit_history_respects_limit() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-commit-history-limit-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let path = root.join("tracked.txt");
    fs::write(&path, "one\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&path, "two\n").unwrap();
    stage_path(&root, &path).unwrap();
    super::super::commit_staged_changes(&root, "second").unwrap();

    let commits = list_commit_history(&root, 1).unwrap();

    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].summary, "second");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn list_commit_history_clamps_extreme_limit_before_allocating() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-commit-history-extreme-limit-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    fs::write(root.join("tracked.txt"), "one\n").unwrap();
    commit_all(&repo, "initial");

    let commits = list_commit_history(&root, usize::MAX).unwrap();

    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].summary, "initial");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unified_diff_for_commit_returns_patch_against_parent() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-commit-diff-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let path = root.join("tracked.txt");
    fs::write(&path, "one\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&path, "two\n").unwrap();
    stage_path(&root, &path).unwrap();
    super::super::commit_staged_changes(&root, "second").unwrap();
    let commits = list_commit_history(&root, 1).unwrap();

    let diff = unified_diff_for_commit(&root, &commits[0].oid).unwrap();

    assert!(diff.contains("diff --git a/tracked.txt b/tracked.txt"));
    assert!(diff.contains("-one\n"));
    assert!(diff.contains("+two\n"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unified_diff_for_commit_renders_detected_rename_as_rename_delta() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-commit-diff-rename-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let old_path = root.join("original.txt");
    let new_path = root.join("renamed.txt");
    let body = (0..40)
        .map(|line| format!("line {line}\n"))
        .collect::<String>();
    fs::write(&old_path, &body).unwrap();
    commit_all(&repo, "initial");
    fs::rename(&old_path, &new_path).unwrap();
    commit_all_with_head_parent(&repo, "rename file");
    let commits = list_commit_history(&root, 1).unwrap();

    let diff = unified_diff_for_commit(&root, &commits[0].oid).unwrap();

    assert!(diff.contains("rename from original.txt"), "diff: {diff}");
    assert!(diff.contains("rename to renamed.txt"), "diff: {diff}");
    assert!(
        !diff.contains("deleted file mode") && !diff.contains("new file mode"),
        "rename rendered as delete plus add: {diff}"
    );
    assert!(!diff.contains("diff truncated"), "diff: {diff}");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unified_diff_for_commit_truncates_oversized_patch_text() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-commit-diff-truncated-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let path = root.join("huge.txt");
    let line = "0123456789abcdef0123456789abcdef01234567\n";
    let mut body = String::with_capacity(line.len() * 90_000);
    for _ in 0..90_000 {
        body.push_str(line);
    }
    fs::write(&path, &body).unwrap();
    commit_all(&repo, "add huge file");
    let commits = list_commit_history(&root, 1).unwrap();

    let diff = unified_diff_for_commit(&root, &commits[0].oid).unwrap();

    assert!(diff.starts_with("diff --git a/huge.txt b/huge.txt"));
    assert!(diff.ends_with("\n… (diff truncated: exceeds size limit)\n"));
    assert!(
        diff.len() <= MAX_GIT_COMMIT_DIFF_PATCH_BYTES + 64,
        "patch text is {} bytes, expected at most {} plus the marker",
        diff.len(),
        MAX_GIT_COMMIT_DIFF_PATCH_BYTES
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn list_commit_history_returns_empty_for_unborn_head() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-commit-history-unborn-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    assert!(repo.head().is_err());

    let commits = list_commit_history(&root, 10).unwrap();

    assert!(commits.is_empty());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn blame_file_returns_line_authors_and_summaries() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-blame-file-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let path = root.join("tracked.txt");
    fs::write(&path, "one\ntwo\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&path, "one\nchanged\n").unwrap();
    stage_path(&root, &path).unwrap();
    super::super::commit_staged_changes(&root, "change second line").unwrap();

    let blame = super::super::blame_file(&root, &path).unwrap();

    assert_eq!(blame.len(), 2);
    assert_eq!(blame[0].line_number, 1);
    assert_eq!(blame[0].author, "Kuroya Test");
    assert_eq!(blame[0].summary, "initial");
    assert_eq!(blame[1].line_number, 2);
    assert_eq!(blame[1].summary, "change second line");
    assert_eq!(
        blame[1].short_oid.len(),
        DEFAULT_GIT_COMMIT_SHORT_HASH_LENGTH
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn blame_file_can_ignore_whitespace_only_changes() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-blame-ignore-whitespace-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let path = root.join("tracked.txt");
    fs::write(&path, "one\ntwo\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&path, "one\n  two\n").unwrap();
    stage_path(&root, &path).unwrap();
    super::super::commit_staged_changes(&root, "indent second line").unwrap();

    let normal_blame = super::super::blame_file_with_options(&root, &path, false).unwrap();
    let ignore_whitespace_blame =
        super::super::blame_file_with_options(&root, &path, true).unwrap();

    assert_eq!(normal_blame[1].summary, "indent second line");
    assert_eq!(ignore_whitespace_blame[1].summary, "initial");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn git_text_reader_enforces_size_limit_before_blame() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-git-text-reader-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("tracked.txt");

    fs::write(&path, "hello").unwrap();
    assert_eq!(
        super::super::read_git_text_file_with_limit(&path, 5).unwrap(),
        "hello"
    );

    let error = super::super::read_git_text_file_with_limit(&path, 4)
        .unwrap_err()
        .to_string();

    assert!(error.contains("larger than 4 bytes"));

    let missing = root.join("missing.txt");
    let error = super::super::read_git_text_file_with_limit(&missing, 4)
        .unwrap_err()
        .to_string();
    assert!(error.contains("could not read"));
    assert!(error.contains("missing.txt"));

    fs::remove_dir_all(root).unwrap();
}
