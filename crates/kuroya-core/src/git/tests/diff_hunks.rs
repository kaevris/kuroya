use super::*;

#[test]
fn line_diff_marks_changed_and_added_lines() {
    let changed = line_change_kinds("a\nb\nc\n", "a\nx\nc\nd\n", 100);
    assert!(changed.contains_key(&2));
    assert!(changed.contains_key(&4));
    assert_eq!(changed.len(), 2);
}

#[test]
fn line_change_kinds_distinguish_add_modify_and_delete_anchors() {
    let changed = line_change_kinds("a\nb\nc\nd\n", "a\nx\nc\ne\nf\n", 100);
    assert_eq!(changed.get(&2), Some(&GitLineChangeKind::Modified));
    assert_eq!(changed.get(&4), Some(&GitLineChangeKind::Modified));
    assert_eq!(changed.get(&5), Some(&GitLineChangeKind::Added));

    let deleted = line_change_kinds("a\nb\nc\n", "a\nc\n", 100);
    assert_eq!(deleted.get(&2), Some(&GitLineChangeKind::Deleted));
}

#[test]
fn line_change_kinds_can_ignore_trim_whitespace() {
    let options = DiffOptions {
        ignore_trim_whitespace: true,
        ..DiffOptions::default()
    };

    assert!(
        line_change_kinds_with_options("one\n  two\t\n", "one \ntwo\n", 100, options).is_empty()
    );

    assert!(!line_change_kinds("one\n  two\t\n", "one \ntwo\n", 100).is_empty());
}

#[test]
fn changed_line_kinds_against_head_accepts_lexically_equivalent_path() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-line-kinds-equivalent-path-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(root.join("src")).unwrap();
    let repo = Repository::init(&root).unwrap();
    configure_identity(&repo);
    let path = root.join("src/main.rs");
    fs::write(&path, "one\n").unwrap();
    commit_all(&repo, "initial");

    let noisy_path = root.join("src").join("..").join("src").join("main.rs");
    let changed = changed_line_kinds_against_head(&root, &noisy_path, "two\n", 100).unwrap();

    assert_eq!(changed.get(&1), Some(&GitLineChangeKind::Modified));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn diff_labels_sanitize_and_bound_display_surfaces() {
    let raw = format!(
        "src/{}\u{202E}\n/tail.rs",
        "a".repeat(super::super::MAX_GIT_PATH_LABEL_CHARS + 80)
    );
    let labels = super::super::GitDiffLabels::for_displays(&raw, "\u{202E}\n");

    assert!(labels.old_display_label.len() <= super::super::MAX_GIT_PATH_LABEL_CHARS);
    assert!(
        labels
            .old_display_label
            .contains(super::super::GIT_PATH_LABEL_OMISSION)
    );
    assert!(!labels.old_display_label.contains('\n'));
    assert!(!labels.old_display_label.contains('\u{202E}'));
    assert!(labels.old_git_label.len() <= super::super::MAX_GIT_PATH_LABEL_CHARS);
    assert!(labels.old_git_label.starts_with("a/src/"));
    assert!(
        labels
            .old_git_label
            .contains(super::super::GIT_PATH_LABEL_OMISSION)
    );
    assert_eq!(labels.new_display_label, "selected file");
    assert_eq!(labels.new_git_label, "b/selected file");
}

#[test]
fn unified_diff_for_texts_preserves_new_and_deleted_file_labels() {
    let path = PathBuf::from("src").join("main.rs");

    let added =
        super::super::unified_diff_for_texts(&path, None, Some("new\n"), DiffOptions::default())
            .unwrap();
    assert!(added.starts_with("diff --git a/src/main.rs b/src/main.rs\n"));
    assert!(added.contains("new file mode 100644\n"));
    assert!(added.contains("--- /dev/null\n+++ b/src/main.rs\n"));

    let deleted =
        super::super::unified_diff_for_texts(&path, Some("old\n"), None, DiffOptions::default())
            .unwrap();
    assert!(deleted.starts_with("diff --git a/src/main.rs b/src/main.rs\n"));
    assert!(deleted.contains("deleted file mode 100644\n"));
    assert!(deleted.contains("--- a/src/main.rs\n+++ /dev/null\n"));
}

#[test]
fn file_and_diff_apis_accept_lexically_equivalent_paths() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-git-diff-alias-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(root.join("src")).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("src").join("main.rs");
    let alias = root
        .join("src")
        .join("generated")
        .join("..")
        .join("main.rs");
    fs::write(&path, "one\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&path, "two\n").unwrap();
    stage_path(&root, &path).unwrap();
    fs::write(&path, "three\n").unwrap();

    assert_eq!(
        file_text_at_head(&root, &alias).unwrap(),
        Some("one\n".to_owned())
    );
    assert_eq!(
        file_text_at_index(&root, &alias).unwrap(),
        Some("two\n".to_owned())
    );
    assert!(
        unified_diff_against_head(&root, &alias, "three\n")
            .unwrap()
            .contains("+three\n")
    );
    assert!(
        unified_diff_against_index(&root, &alias)
            .unwrap()
            .contains("+two\n")
    );
    assert!(
        unified_diff_against_worktree(&root, &alias, "three\n")
            .unwrap()
            .contains("+three\n")
    );
    assert_eq!(staged_diff_hunks(&root, &alias).unwrap().len(), 1);
    assert_eq!(
        worktree_diff_hunks(&root, &alias, "three\n").unwrap().len(),
        1
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn hunk_apis_accept_lexically_equivalent_paths() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-git-hunk-alias-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(root.join("src")).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("src").join("main.rs");
    let alias = root.join("src").join("..").join("src").join("main.rs");
    let original = "one\ntwo\nthree\n";
    let current = "one\nTWO\nthree\n";
    fs::write(&path, original).unwrap();
    commit_all(&repo, "initial");
    fs::write(&path, current).unwrap();

    let hunks = worktree_diff_hunks(&root, &alias, current).unwrap();
    assert_eq!(hunks.len(), 1);
    stage_worktree_hunk(&root, &alias, current, 0, hunks[0].fingerprint).unwrap();

    let staged_hunks = staged_diff_hunks(&root, &alias).unwrap();
    assert_eq!(staged_hunks.len(), 1);
    unstage_staged_hunk(&root, &alias, 0, staged_hunks[0].fingerprint).unwrap();
    assert!(staged_diff_hunks(&root, &alias).unwrap().is_empty());

    let hunks = worktree_diff_hunks(&root, &alias, current).unwrap();
    let reverted = discard_worktree_hunk(&root, &alias, current, 0, hunks[0].fingerprint).unwrap();
    assert_eq!(reverted, original);
    assert_eq!(fs::read_to_string(&path).unwrap(), original);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn hunk_apis_reject_unresolved_conflicted_paths() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-git-hunk-conflict-{}-{}",
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
    commit_all_with_head_parent(&repo, "ours");
    checkout_branch(&repo, "feature");
    fs::write(&path, "theirs\n").unwrap();
    let merged = commit_all_with_head_parent(&repo, "theirs");
    checkout_branch(&repo, &branch_name);
    merge_commit_into_head(&repo, merged);
    assert_eq!(repo.state(), RepositoryState::Merge);
    assert!(repo.index().unwrap().has_conflicts());
    let current = fs::read_to_string(&path).unwrap();

    let errors = vec![
        worktree_diff_hunks(&root, &path, &current)
            .unwrap_err()
            .to_string(),
        staged_diff_hunks(&root, &path).unwrap_err().to_string(),
        stage_worktree_hunk(&root, &path, &current, 0, 0)
            .unwrap_err()
            .to_string(),
        unstage_staged_hunk(&root, &path, 0, 0)
            .unwrap_err()
            .to_string(),
        discard_worktree_hunk(&root, &path, &current, 0, 0)
            .unwrap_err()
            .to_string(),
    ];

    for error in errors {
        assert!(error.contains("unresolved conflicts"));
    }
    assert!(repo.index().unwrap().has_conflicts());

    fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn unified_diff_apis_accept_windows_workdir_case_aliases() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-git-diff-windows-alias-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("tracked.txt");
    fs::write(&path, "one\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&path, "two\n").unwrap();
    stage_path(&root, &path).unwrap();
    fs::write(&path, "three\n").unwrap();
    let alias_root = PathBuf::from(root.to_string_lossy().to_uppercase());
    let alias = alias_root.join("tracked.txt");

    let head = unified_diff_against_head(&root, &alias, "three\n").unwrap();
    let staged = unified_diff_against_index(&root, &alias).unwrap();
    let worktree = unified_diff_against_worktree(&root, &alias, "three\n").unwrap();

    assert!(head.contains("-one\n"));
    assert!(head.contains("+three\n"));
    assert!(staged.contains("-one\n"));
    assert!(staged.contains("+two\n"));
    assert!(worktree.contains("-two\n"));
    assert!(worktree.contains("+three\n"));

    fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn hunk_apis_accept_windows_workdir_case_aliases() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-git-hunk-windows-alias-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("tracked.txt");
    let alias_root = PathBuf::from(root.to_string_lossy().to_uppercase());
    let alias = alias_root.join("tracked.txt");
    let original = "one\ntwo\nthree\n";
    let current = "one\nTWO\nthree\n";
    fs::write(&path, original).unwrap();
    commit_all(&repo, "initial");
    fs::write(&path, current).unwrap();

    let hunks = worktree_diff_hunks(&root, &alias, current).unwrap();
    assert_eq!(hunks.len(), 1);
    stage_worktree_hunk(&root, &alias, current, 0, hunks[0].fingerprint).unwrap();
    let staged_hunks = staged_diff_hunks(&root, &alias).unwrap();
    assert_eq!(staged_hunks.len(), 1);
    unstage_staged_hunk(&root, &alias, 0, staged_hunks[0].fingerprint).unwrap();
    assert!(staged_diff_hunks(&root, &alias).unwrap().is_empty());

    let hunks = worktree_diff_hunks(&root, &alias, current).unwrap();
    let reverted = discard_worktree_hunk(&root, &alias, current, 0, hunks[0].fingerprint).unwrap();
    assert_eq!(reverted, original);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unified_diff_hunks_show_insertions_and_deletions_with_context() {
    let diff = unified_diff_hunks("a\nb\nc\n", "a\nx\nc\nd\n", 1);

    assert!(diff.contains("@@ -1,3 +1,4 @@"));
    assert!(diff.contains(" a\n"));
    assert!(diff.contains("-b\n"));
    assert!(diff.contains("+x\n"));
    assert!(diff.contains("+d\n"));
}

#[test]
fn unified_diff_hunks_handle_new_files() {
    let diff = unified_diff_hunks("", "one\ntwo\n", 1);

    assert!(diff.contains("@@ -0,0 +1,2 @@"));
    assert!(diff.contains("+one\n"));
    assert!(diff.contains("+two\n"));
}

#[test]
fn diff_hunk_ranges_reject_overflow() {
    let hunk = super::super::DiffHunk {
        old_start: usize::MAX,
        old_lines: 2,
        new_start: 1,
        new_lines: 0,
        additions: 0,
        deletions: 0,
        old_text: Vec::new(),
        new_text: Vec::new(),
    };

    assert!(hunk.old_range().is_none());
    assert_eq!(hunk.new_range(), Some(0..0));
}

#[test]
fn diff_hunk_ranges_reject_non_empty_zero_start() {
    let hunk = super::super::DiffHunk {
        old_start: 0,
        old_lines: 1,
        new_start: 0,
        new_lines: 0,
        additions: 0,
        deletions: 0,
        old_text: Vec::new(),
        new_text: Vec::new(),
    };

    assert!(hunk.old_range().is_none());
    assert_eq!(hunk.new_range(), Some(0..0));
}

#[test]
fn replace_hunk_lines_rejects_invalid_ranges() {
    let mut lines = vec!["one".to_owned()];

    let error = super::super::replace_hunk_lines(&mut lines, 2..3, &[], Vec::new())
        .unwrap_err()
        .to_string();

    assert!(error.contains("outside the current file contents"));
    assert_eq!(lines, vec!["one".to_owned()]);

    let reversed_start = 1;
    let reversed_end = 0;
    let error =
        super::super::replace_hunk_lines(&mut lines, reversed_start..reversed_end, &[], Vec::new())
            .unwrap_err()
            .to_string();

    assert!(error.contains("outside the current file contents"));
    assert_eq!(lines, vec!["one".to_owned()]);
}

#[test]
fn replace_hunk_lines_rejects_range_expected_length_mismatch() {
    let mut lines = vec!["one".to_owned(), "two".to_owned()];

    let error = super::super::replace_hunk_lines(&mut lines, 0..1, &[], Vec::new())
        .unwrap_err()
        .to_string();

    assert!(error.contains("git hunk range is invalid"));
    assert_eq!(lines, vec!["one".to_owned(), "two".to_owned()]);
}

#[test]
fn unified_diff_between_texts_uses_distinct_file_labels() {
    let diff = try_unified_diff_between_texts_with_options(
        "src/old.rs",
        "src/new.rs",
        "one\ntwo\n",
        "one\ndos\n",
        DiffOptions::default(),
    )
    .expect("small diff inputs should be within the size limit");

    assert!(diff.starts_with("diff --git a/src/old.rs b/src/new.rs\n"));
    assert!(diff.contains("--- a/src/old.rs\n"));
    assert!(diff.contains("+++ b/src/new.rs\n"));
    assert!(diff.contains("-two\n"));
    assert!(diff.contains("+dos\n"));
    assert!(
        try_unified_diff_between_texts_with_options(
            "a.txt",
            "b.txt",
            "same\n",
            "same\n",
            DiffOptions::default(),
        )
        .expect("small diff inputs should be within the size limit")
        .is_empty()
    );
}

#[test]
fn unified_diff_between_texts_respects_context_lines_option() {
    let options = DiffOptions {
        context_lines: 0,
        hide_unchanged_regions_minimum_line_count: 0,
        hide_unchanged_regions_reveal_line_count: 0,
        ..DiffOptions::default()
    };
    let diff = try_unified_diff_between_texts_with_options(
        "a.txt",
        "b.txt",
        "one\ntwo\nthree\nfour\nfive\n",
        "one\ntwo\nTHREE\nfour\nfive\n",
        options,
    )
    .unwrap();

    assert!(diff.contains("@@ -3,1 +3,1 @@"));
    assert!(diff.contains("-three\n"));
    assert!(diff.contains("+THREE\n"));
    assert!(!diff.contains(" two\n"));
    assert!(!diff.contains(" four\n"));
}

#[test]
fn unified_diff_between_texts_keeps_small_unchanged_regions_visible() {
    let options = DiffOptions {
        context_lines: 0,
        hide_unchanged_regions_minimum_line_count: 3,
        hide_unchanged_regions_reveal_line_count: 0,
        ..DiffOptions::default()
    };
    let diff = try_unified_diff_between_texts_with_options(
        "a.txt",
        "b.txt",
        "one\ntwo\nthree\nfour\n",
        "ONE\ntwo\nTHREE\nfour\n",
        options,
    )
    .unwrap();

    assert_eq!(diff.matches("@@").count(), 2);
    assert!(diff.contains("@@ -1,3 +1,3 @@"));
    assert!(diff.contains(" two\n"));
    assert!(!diff.contains(" four\n"));
}

#[test]
fn unified_diff_between_texts_keeps_reveal_lines_out_of_initial_grouping() {
    let options = DiffOptions {
        context_lines: 0,
        hide_unchanged_regions_minimum_line_count: 0,
        hide_unchanged_regions_reveal_line_count: 1,
        ..DiffOptions::default()
    };
    let diff = try_unified_diff_between_texts_with_options(
        "a.txt",
        "b.txt",
        "one\ntwo\nthree\nfour\n",
        "ONE\ntwo\nTHREE\nfour\n",
        options,
    )
    .unwrap();

    assert_eq!(diff.matches("@@").count(), 4);
    assert!(!diff.contains(" two\n"));
    assert!(!diff.contains(" four\n"));
}

#[test]
fn unified_diff_between_texts_can_show_unchanged_regions() {
    let options = DiffOptions {
        hide_unchanged_regions: false,
        context_lines: 0,
        ..DiffOptions::default()
    };
    let diff = try_unified_diff_between_texts_with_options(
        "a.txt",
        "b.txt",
        "one\ntwo\nthree\nfour\nfive\n",
        "one\ntwo\nTHREE\nfour\nfive\n",
        options,
    )
    .unwrap();

    assert!(diff.contains("@@ -1,5 +1,5 @@"));
    assert!(diff.contains(" one\n"));
    assert!(diff.contains(" two\n"));
    assert!(diff.contains("-three\n"));
    assert!(diff.contains("+THREE\n"));
    assert!(diff.contains(" four\n"));
    assert!(diff.contains(" five\n"));
}

#[test]
fn unified_diff_between_texts_legacy_algorithm_uses_positional_changes() {
    let options = DiffOptions {
        algorithm: DiffAlgorithm::Legacy,
        context_lines: 0,
        ..DiffOptions::default()
    };
    let diff = try_unified_diff_between_texts_with_options(
        "a.txt",
        "b.txt",
        "a\nb\nc\n",
        "b\na\nc\n",
        options,
    )
    .unwrap();

    assert!(diff.contains("-a\n+b\n-b\n+a\n"));
}

#[test]
fn unified_diff_between_texts_respects_max_computation_time_option() {
    let old = (0..46)
        .map(|line| format!("line-{line}\n"))
        .collect::<String>();
    let new = (1..46)
        .chain(std::iter::once(0))
        .map(|line| format!("line-{line}\n"))
        .collect::<String>();
    let options = DiffOptions {
        algorithm: DiffAlgorithm::Advanced,
        context_lines: 0,
        hide_unchanged_regions_minimum_line_count: 0,
        max_computation_time_ms: 1,
        ..DiffOptions::default()
    };
    let diff =
        try_unified_diff_between_texts_with_options("a.txt", "b.txt", &old, &new, options).unwrap();

    assert!(diff.contains("-line-0\n+line-1\n-line-1\n+line-2\n"));
}

#[test]
fn unified_diff_between_texts_can_ignore_trim_whitespace() {
    let options = DiffOptions {
        ignore_trim_whitespace: true,
        ..DiffOptions::default()
    };

    assert!(
        try_unified_diff_between_texts_with_options(
            "a.txt",
            "b.txt",
            "one\n  two\t\n",
            "one \n two\n",
            options,
        )
        .unwrap()
        .is_empty()
    );

    let diff = try_unified_diff_between_texts_with_options(
        "a.txt",
        "b.txt",
        "one\n  two\n",
        "one\n  three\n",
        options,
    )
    .unwrap();
    assert!(diff.contains("-  two\n"));
    assert!(diff.contains("+  three\n"));
}

#[test]
fn try_unified_diff_between_texts_respects_max_file_size_option() {
    let options = DiffOptions {
        max_file_size_bytes: 4,
        ..DiffOptions::default()
    };

    let error =
        try_unified_diff_between_texts_with_options("a.txt", "b.txt", "short", "new", options)
            .unwrap_err()
            .to_string();

    assert_eq!(error, "a.txt is larger than 4 bytes");
}

#[test]
fn try_unified_diff_between_texts_sanitizes_size_error_labels() {
    let options = DiffOptions {
        max_file_size_bytes: 4,
        ..DiffOptions::default()
    };
    let raw = format!(
        "\u{202E}{}\n.rs",
        "a".repeat(super::super::MAX_GIT_PATH_LABEL_CHARS + 80)
    );

    let error = try_unified_diff_between_texts_with_options(&raw, "b.txt", "short", "new", options)
        .unwrap_err()
        .to_string();

    assert!(!error.contains('\n'));
    assert!(!error.contains('\u{202E}'));
    assert!(error.contains(super::super::GIT_PATH_LABEL_OMISSION));
    assert!(
        error.len() <= super::super::MAX_GIT_PATH_LABEL_CHARS + " is larger than 4 bytes".len()
    );
}

#[test]
fn try_unified_diff_between_texts_treats_zero_max_file_size_as_no_limit() {
    let options = DiffOptions {
        max_file_size_bytes: 0,
        ..DiffOptions::default()
    };
    let old = "a".repeat(32);
    let new = "b".repeat(32);

    let diff = try_unified_diff_between_texts_with_options("a.txt", "b.txt", &old, &new, options)
        .expect("zero max file size should not limit diff inputs");

    assert!(diff.contains("-"));
    assert!(diff.contains("+"));
    assert_eq!(diff_max_file_size_bytes(0), 0);
}

#[test]
fn unified_diff_against_index_uses_staged_text_not_worktree() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-staged-diff-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("tracked.txt");
    fs::write(&path, "one\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&path, "two\n").unwrap();
    stage_path(&root, &path).unwrap();
    fs::write(&path, "three\n").unwrap();

    let diff = unified_diff_against_index(&root, &path).unwrap();

    assert!(diff.contains("-one\n"));
    assert!(diff.contains("+two\n"));
    assert!(!diff.contains("+three\n"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unified_diff_against_worktree_uses_index_as_baseline() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-worktree-diff-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("tracked.txt");
    fs::write(&path, "one\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&path, "two\n").unwrap();
    stage_path(&root, &path).unwrap();
    fs::write(&path, "three\n").unwrap();

    let diff = unified_diff_against_worktree(&root, &path, "three\n").unwrap();

    assert!(diff.contains("-two\n"));
    assert!(diff.contains("+three\n"));
    assert!(!diff.contains("-one\n"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unified_diff_against_head_includes_staged_and_worktree_text() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-head-diff-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("tracked.txt");
    fs::write(&path, "one\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&path, "two\n").unwrap();
    stage_path(&root, &path).unwrap();

    let diff = unified_diff_against_head(&root, &path, "three\n").unwrap();

    assert!(diff.contains("-one\n"));
    assert!(diff.contains("+three\n"));
    assert!(!diff.contains("-two\n"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn prepared_diff_helpers_return_loaded_revision_texts() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-prepared-diff-texts-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("tracked.txt");
    fs::write(&path, "one\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&path, "two\n").unwrap();
    stage_path(&root, &path).unwrap();

    let staged = staged_diff_with_texts(&root, &path, DiffOptions::default()).unwrap();
    assert!(staged.diff.contains("-one\n"));
    assert!(staged.diff.contains("+two\n"));
    assert_eq!(staged.head_text, Some("one\n".to_owned()));
    assert_eq!(staged.index_text, Some("two\n".to_owned()));

    let worktree =
        worktree_diff_with_index_text(&root, &path, "three\n", DiffOptions::default()).unwrap();
    assert!(worktree.diff.contains("-two\n"));
    assert!(worktree.diff.contains("+three\n"));
    assert_eq!(worktree.index_text, Some("two\n".to_owned()));

    let head = head_diff_with_text(&root, &path, "three\n", DiffOptions::default()).unwrap();
    assert!(head.diff.contains("-one\n"));
    assert!(head.diff.contains("+three\n"));
    assert_eq!(head.head_text, Some("one\n".to_owned()));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn file_text_at_head_reads_committed_text_without_worktree_changes() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-head-file-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("tracked.txt");
    let new_path = root.join("new.txt");
    fs::write(&path, "one\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&path, "two\n").unwrap();

    assert_eq!(
        file_text_at_head(&root, &path).unwrap(),
        Some("one\n".to_owned())
    );
    assert_eq!(file_text_at_head(&root, &new_path).unwrap(), None);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn file_text_at_index_reads_staged_text_without_worktree_changes() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-index-file-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("tracked.txt");
    let new_path = root.join("new.txt");
    fs::write(&path, "one\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&path, "two\n").unwrap();
    stage_path(&root, &path).unwrap();
    fs::write(&path, "three\n").unwrap();

    assert_eq!(
        file_text_at_index(&root, &path).unwrap(),
        Some("two\n".to_owned())
    );
    assert_eq!(file_text_at_index(&root, &new_path).unwrap(), None);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn worktree_diff_hunks_report_unstaged_hunk_metadata() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-worktree-hunks-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("tracked.txt");
    fs::write(&path, "one\ntwo\nthree\nfour\nfive\n").unwrap();
    commit_all(&repo, "initial");
    let current = "one\nchanged\nthree\nfour\nadded\nfive\n";

    let hunks = worktree_diff_hunks(&root, &path, current).unwrap();

    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0].index, 0);
    assert_eq!(hunks[0].additions, 2);
    assert_eq!(hunks[0].deletions, 1);
    assert!(hunks[0].header.starts_with("@@ -"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn stage_worktree_hunk_stages_only_selected_hunk() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-stage-hunk-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("tracked.txt");
    fs::write(
        &path,
        "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\n",
    )
    .unwrap();
    commit_all(&repo, "initial");
    let current = "one\nchanged\nthree\nfour\nfive\nsix\nseven\nchanged again\nnine\n";
    fs::write(&path, current).unwrap();
    let hunk_fingerprint = worktree_diff_hunks(&root, &path, current).unwrap()[0].fingerprint;

    stage_worktree_hunk(&root, &path, current, 0, hunk_fingerprint).unwrap();

    let staged = unified_diff_against_index(&root, &path).unwrap();
    let unstaged = unified_diff_against_worktree(&root, &path, current).unwrap();
    assert!(staged.contains("+changed\n"));
    assert!(!staged.contains("+changed again\n"));
    assert!(!unstaged.contains("+changed\n"));
    assert!(unstaged.contains("+changed again\n"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn stage_worktree_hunk_removes_deleted_file_from_index() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-stage-deleted-hunk-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("tracked.txt");
    fs::write(&path, "one\ntwo\n").unwrap();
    commit_all(&repo, "initial");
    fs::remove_file(&path).unwrap();
    let current = "";
    let hunks = worktree_diff_hunks(&root, &path, current).unwrap();
    assert_eq!(hunks.len(), 1);
    let hunk_fingerprint = hunks[0].fingerprint;

    stage_worktree_hunk(&root, &path, current, 0, hunk_fingerprint).unwrap();

    assert_eq!(file_text_at_index(&root, &path).unwrap(), None);
    let staged = unified_diff_against_index(&root, &path).unwrap();
    assert!(staged.contains("deleted file mode 100644\n"));
    assert!(staged.contains("-one\n"));
    assert!(staged.contains("-two\n"));
    assert!(
        worktree_diff_hunks(&root, &path, current)
            .unwrap()
            .is_empty()
    );
    assert!(
        unified_diff_against_worktree(&root, &path, current)
            .unwrap()
            .is_empty()
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn stage_worktree_hunk_keeps_empty_existing_file_in_index() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-stage-empty-hunk-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("tracked.txt");
    fs::write(&path, "one\ntwo\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&path, "").unwrap();
    let current = "";
    let hunks = worktree_diff_hunks(&root, &path, current).unwrap();
    assert_eq!(hunks.len(), 1);
    let hunk_fingerprint = hunks[0].fingerprint;

    stage_worktree_hunk(&root, &path, current, 0, hunk_fingerprint).unwrap();

    assert_eq!(
        file_text_at_index(&root, &path).unwrap(),
        Some(String::new())
    );
    let staged = unified_diff_against_index(&root, &path).unwrap();
    assert!(!staged.contains("deleted file mode 100644\n"));
    assert!(staged.contains("-one\n"));
    assert!(staged.contains("-two\n"));
    assert!(
        worktree_diff_hunks(&root, &path, current)
            .unwrap()
            .is_empty()
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn worktree_diff_hunks_do_not_report_staged_deleted_file_as_unstaged() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-staged-deleted-clean-worktree-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("tracked.txt");
    fs::write(&path, "one\ntwo\n").unwrap();
    commit_all(&repo, "initial");
    fs::remove_file(&path).unwrap();
    stage_path(&root, &path).unwrap();
    let current = "";

    assert_eq!(file_text_at_index(&root, &path).unwrap(), None);
    assert!(
        worktree_diff_hunks(&root, &path, current)
            .unwrap()
            .is_empty()
    );
    assert!(
        unified_diff_against_worktree(&root, &path, current)
            .unwrap()
            .is_empty()
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn stage_worktree_hunk_rejects_shifted_stale_hunk_index() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-stage-shifted-hunk-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("tracked.txt");
    let base = (1..=36)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(&path, &base).unwrap();
    commit_all(&repo, "initial");
    let mut original_lines = (1..=36)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>();
    original_lines[1] = "alpha".to_owned();
    original_lines[17] = "bravo".to_owned();
    original_lines[33] = "charlie".to_owned();
    let original = original_lines.join("\n") + "\n";
    let hunks = worktree_diff_hunks(&root, &path, &original).unwrap();
    assert_eq!(hunks.len(), 3);
    let hunk_fingerprint = hunks[1].fingerprint;

    let mut shifted_lines = (1..=36)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>();
    shifted_lines[17] = "bravo".to_owned();
    shifted_lines[33] = "charlie".to_owned();
    let shifted = shifted_lines.join("\n") + "\n";
    fs::write(&path, &shifted).unwrap();

    let error = stage_worktree_hunk(&root, &path, &shifted, 1, hunk_fingerprint)
        .unwrap_err()
        .to_string();

    assert!(error.contains("no longer matches the selected hunk"));
    assert_eq!(fs::read_to_string(&path).unwrap(), shifted);
    assert!(
        !unified_diff_against_index(&root, &path)
            .unwrap()
            .contains("+charlie\n")
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn staged_diff_hunks_report_staged_hunk_metadata() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-staged-hunks-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("tracked.txt");
    fs::write(&path, "one\ntwo\nthree\nfour\nfive\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(&path, "one\nchanged\nthree\nfour\nadded\nfive\n").unwrap();
    stage_path(&root, &path).unwrap();

    let hunks = staged_diff_hunks(&root, &path).unwrap();

    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0].index, 0);
    assert_eq!(hunks[0].additions, 2);
    assert_eq!(hunks[0].deletions, 1);
    assert!(hunks[0].header.starts_with("@@ -"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unstage_staged_hunk_unstages_only_selected_hunk() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-unstage-hunk-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("tracked.txt");
    fs::write(
        &path,
        "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\n",
    )
    .unwrap();
    commit_all(&repo, "initial");
    let current = "one\nchanged\nthree\nfour\nfive\nsix\nseven\nchanged again\nnine\n";
    fs::write(&path, current).unwrap();
    stage_path(&root, &path).unwrap();
    let hunk_fingerprint = staged_diff_hunks(&root, &path).unwrap()[0].fingerprint;

    unstage_staged_hunk(&root, &path, 0, hunk_fingerprint).unwrap();

    let staged = unified_diff_against_index(&root, &path).unwrap();
    let unstaged = unified_diff_against_worktree(&root, &path, current).unwrap();
    assert!(!staged.contains("+changed\n"));
    assert!(staged.contains("+changed again\n"));
    assert!(unstaged.contains("+changed\n"));
    assert!(!unstaged.contains("+changed again\n"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unstage_staged_hunk_rejects_shifted_stale_hunk_index() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-unstage-shifted-hunk-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("tracked.txt");
    let base = (1..=36)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(&path, &base).unwrap();
    commit_all(&repo, "initial");
    let mut original_lines = (1..=36)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>();
    original_lines[1] = "alpha".to_owned();
    original_lines[17] = "bravo".to_owned();
    original_lines[33] = "charlie".to_owned();
    let original = original_lines.join("\n") + "\n";
    fs::write(&path, &original).unwrap();
    stage_path(&root, &path).unwrap();
    let hunks = staged_diff_hunks(&root, &path).unwrap();
    assert_eq!(hunks.len(), 3);
    let hunk_fingerprint = hunks[1].fingerprint;

    let mut shifted_lines = (1..=36)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>();
    shifted_lines[17] = "bravo".to_owned();
    shifted_lines[33] = "charlie".to_owned();
    let shifted = shifted_lines.join("\n") + "\n";
    fs::write(&path, &shifted).unwrap();
    stage_path(&root, &path).unwrap();

    let error = unstage_staged_hunk(&root, &path, 1, hunk_fingerprint)
        .unwrap_err()
        .to_string();

    assert!(error.contains("no longer matches the selected hunk"));
    assert_eq!(file_text_at_index(&root, &path).unwrap().unwrap(), shifted);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn discard_worktree_hunk_reverts_only_selected_hunk() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-discard-hunk-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("tracked.txt");
    fs::write(
        &path,
        "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\n",
    )
    .unwrap();
    commit_all(&repo, "initial");
    let current = "one\nchanged\nthree\nfour\nfive\nsix\nseven\nchanged again\nnine\n";
    fs::write(&path, current).unwrap();

    let hunk_fingerprint =
        super::super::worktree_diff_hunks(&root, &path, current).unwrap()[0].fingerprint;

    let updated =
        super::super::discard_worktree_hunk(&root, &path, current, 0, hunk_fingerprint).unwrap();

    assert_eq!(
        updated,
        "one\ntwo\nthree\nfour\nfive\nsix\nseven\nchanged again\nnine\n"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), updated);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn discard_worktree_hunk_rejects_stale_hunk_fingerprint() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-discard-stale-hunk-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("tracked.txt");
    fs::write(
        &path,
        "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\n",
    )
    .unwrap();
    commit_all(&repo, "initial");
    let original = "one\nchanged\nthree\nfour\nfive\nsix\nseven\nchanged again\nnine\n";
    let stale = "one\nchanged\nthree\nfour\nfive\nsix\nseven\nchanged differently\nnine\n";
    let hunk_fingerprint =
        super::super::worktree_diff_hunks(&root, &path, original).unwrap()[1].fingerprint;
    fs::write(&path, stale).unwrap();

    let error = super::super::discard_worktree_hunk(&root, &path, stale, 1, hunk_fingerprint)
        .unwrap_err()
        .to_string();

    assert!(error.contains("no longer matches the selected hunk"));
    assert_eq!(fs::read_to_string(&path).unwrap(), stale);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn discard_worktree_hunk_rejects_shifted_stale_hunk_index() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-discard-shifted-hunk-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let path = root.join("tracked.txt");
    let base = (1..=36)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(&path, &base).unwrap();
    commit_all(&repo, "initial");
    let mut original_lines = (1..=36)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>();
    original_lines[1] = "alpha".to_owned();
    original_lines[17] = "bravo".to_owned();
    original_lines[33] = "charlie".to_owned();
    let original = original_lines.join("\n") + "\n";
    let hunks = super::super::worktree_diff_hunks(&root, &path, &original).unwrap();
    assert_eq!(hunks.len(), 3);
    let hunk_fingerprint = hunks[1].fingerprint;

    let mut shifted_lines = (1..=36)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>();
    shifted_lines[17] = "bravo".to_owned();
    shifted_lines[33] = "charlie".to_owned();
    let shifted = shifted_lines.join("\n") + "\n";
    assert_eq!(
        super::super::worktree_diff_hunks(&root, &path, &shifted)
            .unwrap()
            .len(),
        2
    );
    fs::write(&path, &shifted).unwrap();

    let error = super::super::discard_worktree_hunk(&root, &path, &shifted, 1, hunk_fingerprint)
        .unwrap_err()
        .to_string();

    assert!(error.contains("no longer matches the selected hunk"));
    assert_eq!(fs::read_to_string(&path).unwrap(), shifted);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn discard_worktree_hunk_removes_untracked_file_when_hunk_clears_file() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-discard-untracked-hunk-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    Repository::init(&root).unwrap();
    let path = root.join("new.txt");
    let current = "new\n";
    fs::write(&path, current).unwrap();

    let hunk_fingerprint =
        super::super::worktree_diff_hunks(&root, &path, current).unwrap()[0].fingerprint;

    let updated =
        super::super::discard_worktree_hunk(&root, &path, current, 0, hunk_fingerprint).unwrap();

    assert_eq!(updated, "");
    assert!(!path.exists());
    assert!(GitSnapshot::scan(&root).entries().is_empty());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn discard_worktree_hunk_preserves_crlf_line_endings() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-discard-crlf-hunk-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    repo.config()
        .unwrap()
        .set_bool("core.autocrlf", false)
        .unwrap();
    let path = root.join("tracked.txt");
    let base = "one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\neight\r\nnine\r\n";
    fs::write(&path, base).unwrap();
    commit_all(&repo, "initial");
    let current =
        "one\r\nchanged\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\nchanged again\r\nnine\r\n";
    fs::write(&path, current).unwrap();

    let hunks = worktree_diff_hunks(&root, &path, current).unwrap();
    assert_eq!(hunks.len(), 2);
    let updated = discard_worktree_hunk(&root, &path, current, 0, hunks[0].fingerprint).unwrap();

    let expected =
        "one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\nchanged again\r\nnine\r\n";
    assert_eq!(updated, expected);
    assert_eq!(fs::read(&path).unwrap(), expected.as_bytes());
    assert_eq!(
        updated.matches("\r\n").count(),
        updated.matches('\n').count()
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn stage_worktree_hunk_preserves_crlf_line_endings() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-stage-crlf-hunk-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    repo.config()
        .unwrap()
        .set_bool("core.autocrlf", false)
        .unwrap();
    let path = root.join("tracked.txt");
    let base = "one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\neight\r\nnine\r\n";
    fs::write(&path, base).unwrap();
    commit_all(&repo, "initial");
    let current =
        "one\r\nchanged\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\nchanged again\r\nnine\r\n";
    fs::write(&path, current).unwrap();

    let hunks = worktree_diff_hunks(&root, &path, current).unwrap();
    assert_eq!(hunks.len(), 2);
    stage_worktree_hunk(&root, &path, current, 0, hunks[0].fingerprint).unwrap();

    let staged = file_text_at_index(&root, &path).unwrap().unwrap();
    let expected = "one\r\nchanged\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\neight\r\nnine\r\n";
    assert_eq!(staged, expected);
    assert_eq!(staged.matches("\r\n").count(), staged.matches('\n').count());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn hunk_operations_preserve_each_lines_own_line_ending() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-hunk-mixed-endings-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    repo.config()
        .unwrap()
        .set_bool("core.autocrlf", false)
        .unwrap();
    let path = root.join("tracked.txt");
    let base = "alpha\nbeta\r\ngamma\ndelta\r\n";
    fs::write(&path, base).unwrap();
    commit_all(&repo, "initial");

    let changed_lf = "alpha\nbeta\r\nGAMMA\ndelta\r\n";
    fs::write(&path, changed_lf).unwrap();
    let hunks = worktree_diff_hunks(&root, &path, changed_lf).unwrap();
    assert_eq!(hunks.len(), 1);
    let updated = discard_worktree_hunk(&root, &path, changed_lf, 0, hunks[0].fingerprint).unwrap();
    assert_eq!(updated, base);
    assert_eq!(fs::read(&path).unwrap(), base.as_bytes());

    let changed_crlf = "alpha\nBETA\r\ngamma\ndelta\r\n";
    fs::write(&path, changed_crlf).unwrap();
    let hunks = worktree_diff_hunks(&root, &path, changed_crlf).unwrap();
    assert_eq!(hunks.len(), 1);
    stage_worktree_hunk(&root, &path, changed_crlf, 0, hunks[0].fingerprint).unwrap();
    assert_eq!(
        file_text_at_index(&root, &path).unwrap().unwrap(),
        changed_crlf
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn discard_worktree_hunk_preserves_remainder_eof_newline_state() {
    let root = canonical_test_temp_dir().join(format!(
        "kuroya-hunk-eof-newline-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    repo.config()
        .unwrap()
        .set_bool("core.autocrlf", false)
        .unwrap();

    let path = root.join("with_newline.txt");
    let base = "alpha\r\nbeta\r\n";
    fs::write(&path, base).unwrap();
    commit_all(&repo, "initial");
    let extended = "alpha\r\nbeta\r\ngamma\r\n";
    fs::write(&path, extended).unwrap();
    let hunks = worktree_diff_hunks(&root, &path, extended).unwrap();
    assert_eq!(hunks.len(), 1);
    let updated = discard_worktree_hunk(&root, &path, extended, 0, hunks[0].fingerprint).unwrap();
    assert_eq!(updated, base);
    assert_eq!(fs::read(&path).unwrap(), base.as_bytes());

    let path = root.join("without_newline.txt");
    let base = "alpha\r\nbeta";
    fs::write(&path, base).unwrap();
    commit_all_with_head_parent(&repo, "second");
    let extended = "alpha\r\nbeta\r\ngamma";
    fs::write(&path, extended).unwrap();
    let hunks = worktree_diff_hunks(&root, &path, extended).unwrap();
    assert_eq!(hunks.len(), 1);
    let updated = discard_worktree_hunk(&root, &path, extended, 0, hunks[0].fingerprint).unwrap();
    assert_eq!(updated, base);
    assert_eq!(fs::read(&path).unwrap(), base.as_bytes());

    fs::remove_dir_all(root).unwrap();
}
