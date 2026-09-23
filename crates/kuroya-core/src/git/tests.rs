use std::{
    collections::{BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
};

use git2::{BranchType, Oid, Repository, RepositoryState, Signature, build::CheckoutBuilder};

use super::{
    DEFAULT_GIT_COMMIT_SHORT_HASH_LENGTH, DEFAULT_GIT_DETECT_SUBMODULES_LIMIT,
    DEFAULT_GIT_SIMILARITY_THRESHOLD, DEFAULT_GIT_STATUS_LIMIT, DiffAlgorithm, DiffOptions,
    GitChangeStage, GitCheckoutType, GitFileStatus, GitLineChangeKind, GitRemoteDivergence,
    GitScopedStatus, GitSmartCommitChanges, GitSnapshot, GitStatusCounts, GitStatusEntry,
    GitTimelineDate, MAX_GIT_COMMIT_DIFF_PATCH_BYTES, changed_line_kinds_against_head,
    checkout_ref, delete_branch, diff_max_file_size_bytes, discard_path, discard_paths,
    discard_worktree_hunk, file_text_at_head, file_text_at_index, git_scoped_status_snapshot,
    head_diff_with_text, line_change_kinds, line_change_kinds_with_options, list_checkout_refs,
    list_commit_history, list_commit_history_with_short_hash_length,
    list_commit_history_with_timeline_date, list_local_branches, list_stashes, path_is_committed,
    rename_branch, stage_path, stage_paths, stage_worktree_hunk, staged_diff_hunks,
    staged_diff_with_texts, status_entries_for_paths, try_unified_diff_between_texts_with_options,
    unified_diff_against_head, unified_diff_against_index, unified_diff_against_worktree,
    unified_diff_for_commit, unified_diff_for_stash, unified_diff_hunks, unstage_path,
    unstage_paths, unstage_staged_hunk, worktree_diff_hunks, worktree_diff_with_index_text,
    worktree_relative_path,
};

mod branch_checkout;
mod commit_smart_commit;
mod diff_hunks;
mod revision;
mod scoped_status;
mod stash_history_blame;
mod status_paths;

fn configure_identity(repo: &Repository) {
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Kuroya Test").unwrap();
    config.set_str("user.email", "kuroya@example.com").unwrap();
}

fn commit_all(repo: &Repository, message: &str) {
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = Signature::now("Kuroya Test", "kuroya@example.com").unwrap();
    repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[])
        .unwrap();
}

fn commit_all_with_head_parent(repo: &Repository, message: &str) -> Oid {
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = Signature::now("Kuroya Test", "kuroya@example.com").unwrap();
    let parents = repo
        .head()
        .ok()
        .and_then(|head| head.peel_to_commit().ok())
        .into_iter()
        .collect::<Vec<_>>();
    let parent_refs = parents.iter().collect::<Vec<_>>();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parent_refs,
    )
    .unwrap()
}

fn checkout_branch(repo: &Repository, branch: &str) {
    repo.set_head(&format!("refs/heads/{branch}")).unwrap();
    let mut checkout = CheckoutBuilder::new();
    checkout.force();
    repo.checkout_head(Some(&mut checkout)).unwrap();
}

fn merge_commit_into_head(repo: &Repository, commit: Oid) {
    let annotated = repo.find_annotated_commit(commit).unwrap();
    let mut checkout = CheckoutBuilder::new();
    checkout.allow_conflicts(true).conflict_style_merge(true);
    repo.merge(&[&annotated], None, Some(&mut checkout))
        .unwrap();
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}
