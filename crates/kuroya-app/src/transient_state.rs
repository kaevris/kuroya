use crate::workspace_state::PaneId;
use kuroya_core::{BufferId, GitSmartCommitChanges, LspSignatureHelp};
use std::{
    collections::HashSet,
    ops::Range,
    path::{Path, PathBuf},
    time::Instant,
};

const BUFFER_ID_DEDUPE_INITIAL_CAPACITY_MAX: usize = 1024;

#[derive(Debug, Clone)]
pub(crate) struct EditorImePreedit {
    pub(crate) buffer_id: BufferId,
    pub(crate) text: String,
}

#[derive(Debug, Clone)]
pub(crate) struct FileJump {
    pub(crate) path: PathBuf,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) column_encoding: FileJumpColumnEncoding,
    pub(crate) selection_length: Option<usize>,
}

impl FileJump {
    pub(crate) fn char(path: PathBuf, line: usize, column: usize) -> Self {
        Self {
            path,
            line,
            column,
            column_encoding: FileJumpColumnEncoding::Char,
            selection_length: None,
        }
    }

    pub(crate) fn char_selection(
        path: PathBuf,
        line: usize,
        column: usize,
        selection_length: usize,
    ) -> Self {
        Self {
            path,
            line,
            column,
            column_encoding: FileJumpColumnEncoding::Char,
            selection_length: Some(selection_length),
        }
    }

    pub(crate) fn lsp_utf16(path: PathBuf, line: usize, column: usize) -> Self {
        Self {
            path,
            line,
            column,
            column_encoding: FileJumpColumnEncoding::LspUtf16,
            selection_length: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileJumpColumnEncoding {
    Char,
    LspUtf16,
}

#[derive(Debug, Clone)]
pub(crate) enum PendingWorkspaceSwitch {
    Confirm { target: PathBuf },
    ConfirmHomeDirectory { target: PathBuf },
    Saving { target: PathBuf, ids: Vec<BufferId> },
}

impl PendingWorkspaceSwitch {
    pub(crate) fn prune_invalid_buffer_ids(
        &mut self,
        is_valid: impl FnMut(BufferId) -> bool,
    ) -> bool {
        match self {
            Self::Confirm { .. } | Self::ConfirmHomeDirectory { .. } => false,
            Self::Saving { ids, .. } => prune_invalid_buffer_ids(ids, is_valid),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum PendingExit {
    Confirm,
    Saving { ids: Vec<BufferId> },
}

impl PendingExit {
    pub(crate) fn prune_invalid_buffer_ids(
        &mut self,
        is_valid: impl FnMut(BufferId) -> bool,
    ) -> bool {
        match self {
            Self::Confirm => false,
            Self::Saving { ids } => prune_invalid_buffer_ids(ids, is_valid),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PendingEditorFileDrop {
    pub(crate) paths: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingSourceControlDiscard {
    pub(crate) paths: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingSourceControlSmartCommit {
    pub(crate) request_id: u64,
    pub(crate) message: String,
    pub(crate) smart_commit_changes: GitSmartCommitChanges,
    pub(crate) change_count: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingSourceControlEmptyCommit {
    pub(crate) request_id: u64,
    pub(crate) message: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingSourceControlProtectedBranchCommit {
    pub(crate) request_id: u64,
    pub(crate) message: String,
    pub(crate) smart_commit_changes: Option<GitSmartCommitChanges>,
    pub(crate) allow_empty: bool,
    pub(crate) branch: String,
    pub(crate) pattern: String,
}

#[derive(Debug, Clone)]
pub(crate) enum PendingSourceControlCommitSave {
    Confirm {
        request_id: u64,
        message: String,
        smart_commit_changes: Option<GitSmartCommitChanges>,
        allow_empty: bool,
        ids: Vec<BufferId>,
    },
    Saving {
        request_id: u64,
        message: String,
        smart_commit_changes: Option<GitSmartCommitChanges>,
        allow_empty: bool,
        ids: Vec<BufferId>,
    },
}

impl PendingSourceControlCommitSave {
    pub(crate) fn prune_invalid_buffer_ids(
        &mut self,
        is_valid: impl FnMut(BufferId) -> bool,
    ) -> bool {
        match self {
            Self::Confirm { ids, .. } | Self::Saving { ids, .. } => {
                prune_invalid_buffer_ids(ids, is_valid)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum PendingSourceControlStashSave {
    Confirm { message: String, ids: Vec<BufferId> },
    Saving { message: String, ids: Vec<BufferId> },
}

impl PendingSourceControlStashSave {
    pub(crate) fn prune_invalid_buffer_ids(
        &mut self,
        is_valid: impl FnMut(BufferId) -> bool,
    ) -> bool {
        match self {
            Self::Confirm { ids, .. } | Self::Saving { ids, .. } => {
                prune_invalid_buffer_ids(ids, is_valid)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EditorMiddleClickScroll {
    pub(crate) pane_id: PaneId,
    pub(crate) buffer_id: BufferId,
    pub(crate) anchor_y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EditorInertialScroll {
    pub(crate) velocity_x: f32,
    pub(crate) velocity_y: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EditorSelectionDrag {
    pub(crate) pane_id: PaneId,
    pub(crate) buffer_id: BufferId,
    pub(crate) ranges: Vec<Range<usize>>,
    pub(crate) text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PendingLspHover {
    pub(crate) pane_id: PaneId,
    pub(crate) buffer_id: BufferId,
    pub(crate) char_idx: usize,
    pub(crate) version: u64,
    pub(crate) started_at: Instant,
    pub(crate) requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LspHoverRequestTarget {
    pub(crate) id: BufferId,
    pub(crate) path: PathBuf,
    pub(crate) version: u64,
    pub(crate) line: usize,
    pub(crate) column_one_based: usize,
}

impl LspHoverRequestTarget {
    pub(crate) fn from_request(
        id: BufferId,
        path: PathBuf,
        version: u64,
        line: usize,
        character: usize,
    ) -> Self {
        Self {
            id,
            path,
            version,
            line,
            column_one_based: character.saturating_add(1),
        }
    }

    pub(crate) fn matches(
        &self,
        id: BufferId,
        path: &Path,
        version: u64,
        line: usize,
        column_one_based: usize,
    ) -> bool {
        self.id == id
            && self.path.as_path() == path
            && self.version == version
            && self.line == line
            && self.column_one_based == column_one_based
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LspHoverPopup {
    pub(crate) id: BufferId,
    pub(crate) path: PathBuf,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) contents: String,
    pub(crate) opened_at: Instant,
}

#[derive(Debug, Clone)]
pub(crate) struct LspSignatureHelpPopup {
    pub(crate) id: BufferId,
    pub(crate) path: PathBuf,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) help: LspSignatureHelp,
}

pub(crate) fn prune_invalid_buffer_ids(
    ids: &mut Vec<BufferId>,
    mut is_valid: impl FnMut(BufferId) -> bool,
) -> bool {
    let original_len = ids.len();
    let mut seen = HashSet::with_capacity(ids.len().min(BUFFER_ID_DEDUPE_INITIAL_CAPACITY_MAX));
    ids.retain(|id| is_valid(*id) && seen.insert(*id));
    ids.len() != original_len
}

#[cfg(test)]
mod tests {
    use super::{
        PendingExit, PendingSourceControlCommitSave, PendingSourceControlStashSave,
        PendingWorkspaceSwitch, prune_invalid_buffer_ids,
    };
    use kuroya_core::BufferId;
    use std::path::PathBuf;

    #[test]
    fn prune_invalid_buffer_ids_preserves_valid_first_seen_order() {
        let mut ids = vec![7, 8, 7, 9, 10, 8];

        let changed = prune_invalid_buffer_ids(&mut ids, |id| id != 9);

        assert!(changed);
        assert_eq!(ids, vec![7, 8, 10]);
    }

    #[test]
    fn pending_workspace_and_exit_guards_prune_invalid_buffer_ids() {
        let mut workspace = PendingWorkspaceSwitch::Saving {
            target: PathBuf::from("workspace/next"),
            ids: vec![3, 4, 3, 5],
        };
        let mut exit = PendingExit::Saving { ids: vec![4, 6, 4] };

        assert!(workspace.prune_invalid_buffer_ids(|id| id != 5));
        assert!(exit.prune_invalid_buffer_ids(|id| id != 6));

        assert!(matches!(
            workspace,
            PendingWorkspaceSwitch::Saving { ref ids, .. } if ids == &vec![3, 4]
        ));
        assert!(matches!(
            exit,
            PendingExit::Saving { ref ids } if ids == &vec![4]
        ));
    }

    #[test]
    fn source_control_save_guards_prune_invalid_buffer_ids() {
        let mut commit = PendingSourceControlCommitSave::Confirm {
            request_id: 1,
            message: "commit".to_owned(),
            smart_commit_changes: None,
            allow_empty: false,
            ids: vec![1, 2, 1, 3],
        };
        let mut stash = PendingSourceControlStashSave::Saving {
            message: "stash".to_owned(),
            ids: vec![2, 4, 4],
        };

        assert!(commit.prune_invalid_buffer_ids(|id| id != 2));
        assert!(stash.prune_invalid_buffer_ids(|id| id != 4));

        assert!(matches!(
            commit,
            PendingSourceControlCommitSave::Confirm { ref ids, .. } if ids == &vec![1, 3]
        ));
        assert!(matches!(
            stash,
            PendingSourceControlStashSave::Saving { ref ids, .. } if ids == &vec![2]
        ));
    }

    #[test]
    fn confirmed_guards_without_ids_are_unchanged() {
        let mut workspace = PendingWorkspaceSwitch::Confirm {
            target: PathBuf::from("workspace/next"),
        };
        let mut exit = PendingExit::Confirm;

        assert!(!workspace.prune_invalid_buffer_ids(|_| false));
        assert!(!exit.prune_invalid_buffer_ids(|_| false));
    }

    #[test]
    fn prune_invalid_buffer_ids_keeps_large_valid_transient_lists() {
        let mut ids = (0..1500)
            .chain(0..1500)
            .map(|id| id as BufferId)
            .collect::<Vec<_>>();

        assert!(prune_invalid_buffer_ids(&mut ids, |_| true));

        assert_eq!(ids.len(), 1500);
        assert_eq!(ids.first().copied(), Some(0));
        assert_eq!(ids.last().copied(), Some(1499));
    }
}
