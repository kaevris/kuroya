use super::{GitChangeStage, GitFileStatus};
use git2::Status;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GitStatusCounts {
    pub modified: usize,
    pub added: usize,
    pub deleted: usize,
    pub renamed: usize,
    pub untracked: usize,
    pub conflicted: usize,
}

impl GitStatusCounts {
    pub fn total(self) -> usize {
        self.modified
            .saturating_add(self.added)
            .saturating_add(self.deleted)
            .saturating_add(self.renamed)
            .saturating_add(self.untracked)
            .saturating_add(self.conflicted)
    }

    pub fn tracked_total(self) -> usize {
        self.modified
            .saturating_add(self.added)
            .saturating_add(self.deleted)
            .saturating_add(self.renamed)
            .saturating_add(self.conflicted)
    }

    pub(super) fn record(&mut self, status: GitFileStatus) {
        match status {
            GitFileStatus::Modified => self.modified = self.modified.saturating_add(1),
            GitFileStatus::Added => self.added = self.added.saturating_add(1),
            GitFileStatus::Deleted => self.deleted = self.deleted.saturating_add(1),
            GitFileStatus::Renamed => self.renamed = self.renamed.saturating_add(1),
            GitFileStatus::Untracked => self.untracked = self.untracked.saturating_add(1),
            GitFileStatus::Conflicted => self.conflicted = self.conflicted.saturating_add(1),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStatusEntry {
    pub path: PathBuf,
    pub status: GitFileStatus,
    pub stage: GitChangeStage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct GitStatusLookup {
    staged: Option<GitFileStatus>,
    unstaged: Option<GitFileStatus>,
}

impl GitStatusLookup {
    pub(super) fn new(status: GitFileStatus, stage: GitChangeStage) -> Self {
        let mut lookup = Self {
            staged: None,
            unstaged: None,
        };
        lookup.record(status, stage);
        lookup
    }

    pub(super) fn record(&mut self, status: GitFileStatus, stage: GitChangeStage) {
        match stage {
            GitChangeStage::Staged => self.staged = Some(status),
            GitChangeStage::Unstaged => self.unstaged = Some(status),
        }
    }

    pub(super) fn merge(&mut self, other: Self) {
        if other.unstaged.is_some() {
            self.unstaged = other.unstaged;
        }
        if self.staged.is_none() {
            self.staged = other.staged;
        }
    }

    pub(super) fn status(&self) -> GitFileStatus {
        match (self.unstaged, self.staged) {
            (Some(unstaged), _) => unstaged,
            (None, Some(staged)) => staged,
            (None, None) => GitFileStatus::Modified,
        }
    }

    pub(super) fn has_stage(&self, stage: GitChangeStage) -> bool {
        match stage {
            GitChangeStage::Staged => self.staged.is_some(),
            GitChangeStage::Unstaged => self.unstaged.is_some(),
        }
    }

    pub(super) fn kinds(&self) -> impl Iterator<Item = GitFileStatus> {
        [self.staged, self.unstaged].into_iter().flatten()
    }
}

pub(super) fn counts_from_status_lookups<'a>(
    lookups: impl Iterator<Item = &'a GitStatusLookup>,
) -> GitStatusCounts {
    let mut counts = GitStatusCounts::default();
    for lookup in lookups {
        for kind in lookup.kinds() {
            counts.record(kind);
        }
    }
    counts
}

pub(super) fn status_entries(
    status: Status,
) -> impl Iterator<Item = (GitFileStatus, GitChangeStage)> {
    let mut entries = [None, None];
    if status.is_conflicted() {
        entries[0] = Some((GitFileStatus::Conflicted, GitChangeStage::Unstaged));
        return entries.into_iter().flatten();
    }

    let mut index = 0;
    if let Some(kind) = index_status_to_kind(status) {
        entries[index] = Some((kind, GitChangeStage::Staged));
        index += 1;
    }
    if let Some(kind) = worktree_status_to_kind(status) {
        entries[index] = Some((kind, GitChangeStage::Unstaged));
    }
    entries.into_iter().flatten()
}

fn index_status_to_kind(status: Status) -> Option<GitFileStatus> {
    if status.contains(Status::INDEX_NEW) {
        Some(GitFileStatus::Added)
    } else if status.contains(Status::INDEX_DELETED) {
        Some(GitFileStatus::Deleted)
    } else if status.contains(Status::INDEX_RENAMED) {
        Some(GitFileStatus::Renamed)
    } else if status.contains(Status::INDEX_MODIFIED) || status.contains(Status::INDEX_TYPECHANGE) {
        Some(GitFileStatus::Modified)
    } else {
        None
    }
}

fn worktree_status_to_kind(status: Status) -> Option<GitFileStatus> {
    if status.contains(Status::WT_NEW) {
        Some(GitFileStatus::Untracked)
    } else if status.contains(Status::WT_DELETED) {
        Some(GitFileStatus::Deleted)
    } else if status.contains(Status::WT_RENAMED) {
        Some(GitFileStatus::Renamed)
    } else if status.contains(Status::WT_MODIFIED) || status.contains(Status::WT_TYPECHANGE) {
        Some(GitFileStatus::Modified)
    } else {
        None
    }
}
