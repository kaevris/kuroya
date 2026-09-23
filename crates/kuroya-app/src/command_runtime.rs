use crate::{
    KuroyaApp, command_runtime::file_workspace::run_file_workspace_command, commands::command_label,
};
use kuroya_core::Command;
use kuroya_core::MergeConflictResolution;
use std::fmt::Write;

mod file_workspace;

impl KuroyaApp {
    pub(crate) fn run_command(&mut self, command: Command) {
        if self.run_ui_command(&command) {
            return;
        }
        if command_requires_git(&command) && !self.settings.git_enabled {
            self.status = "Git is disabled".to_owned();
            return;
        }
        let Some(command) = run_file_workspace_command(self, command) else {
            return;
        };

        match command {
            command @ (Command::ReloadSettings
            | Command::NewFile
            | Command::OpenFile(_)
            | Command::OpenFileAt { .. }
            | Command::SelectFileForCompare(_)
            | Command::CompareFileWithSelected(_)
            | Command::RevealFileInExplorer(_)
            | Command::RevealFileInSourceControl(_)
            | Command::CopyActiveFilePath
            | Command::CopyActiveFileRelativePath
            | Command::CopyFilePath(_)
            | Command::CopyFileRelativePath(_)
            | Command::OpenFileChanges(_)
            | Command::OpenStagedFileChanges(_)
            | Command::OpenFileHeadChanges(_)
            | Command::OpenFileHeadRevision(_)
            | Command::OpenFileIndexRevision(_)
            | Command::OpenAllChanges
            | Command::OpenAllUnstagedChanges
            | Command::OpenAllStagedChanges
            | Command::CopyAllChangesPatch
            | Command::CopyUnstagedChangesPatch
            | Command::CopyStagedChangesPatch
            | Command::CopyActiveFilePatch
            | Command::CopyActiveFileStagedPatch
            | Command::CopyFilePatch(_)
            | Command::CopyStagedFilePatch(_)
            | Command::CopyActiveFileHunkPatch
            | Command::CopyActiveFileStagedHunkPatch
            | Command::OpenFileHunks(_)
            | Command::OpenStagedFileHunks(_)
            | Command::OpenFileBlame(_)
            | Command::CopyActiveDiffPatch
            | Command::CopyActiveDiffHunkPatch
            | Command::StageFileChange(_)
            | Command::StageAllChanges
            | Command::UnstageFileChange(_)
            | Command::UnstageAllChanges
            | Command::DiscardFileChanges(_)
            | Command::DiscardAllChanges
            | Command::StageFileHunk { .. }
            | Command::UnstageFileHunk { .. }
            | Command::DiscardFileHunk { .. }
            | Command::CommitStagedChanges
            | Command::ToggleGitBranchSwitcher
            | Command::ToggleGitHistory
            | Command::ToggleGitStashes
            | Command::SaveGitStash
            | Command::ApplyGitStash(_)
            | Command::PopGitStash(_)
            | Command::DropGitStash(_)
            | Command::OpenWorkspace(_)
            | Command::OpenWorkspacePrompt
            | Command::TrustWorkspace
            | Command::RevokeWorkspaceTrust
            | Command::CreateFileIn(_)
            | Command::CreateFolderIn(_)
            | Command::RenamePath(_)
            | Command::DeletePath(_)
            | Command::RefreshWorkspace
            | Command::SaveActive
            | Command::SaveAs
            | Command::SaveAll
            | Command::ReloadActiveFromDisk
            | Command::CheckForUpdates
            | Command::OpenSettingsFile
            | Command::ToggleSettingsPanel
            | Command::ToggleKeybindingsPanel
            | Command::ToggleThemePicker
            | Command::CycleTheme
            | Command::ToggleMinimap
            | Command::ToggleStickyScroll
            | Command::ToggleVimMode
            | Command::ToggleCommandPalette
            | Command::ToggleQuickOpen
            | Command::ToggleBufferFind
            | Command::ToggleGoToLine
            | Command::ToggleSymbolsPanel
            | Command::CycleSymbolsPanelPlacement
            | Command::ToggleWorkspaceSymbols
            | Command::OpenLocalHistoryBrowser
            | Command::ToggleWorkspaceTasks
            | Command::ToggleProjectSearch
            | Command::CycleProjectSearchPlacement
            | Command::ToggleDiagnosticsPanel
            | Command::CycleDiagnosticsPanelPlacement
            | Command::ToggleDevtools
            | Command::ToggleSourceControl
            | Command::CycleSourceControlPlacement
            | Command::OpenSourceControlInIntegratedTerminal
            | Command::ToggleTerminal
            | Command::NextTerminalSession
            | Command::PreviousTerminalSession
            | Command::NextTerminalSearchResult
            | Command::PreviousTerminalSearchResult) => {
                self.status = skipped_pre_dispatched_command_status(&command);
            }
            command @ (Command::ToggleTerminalSearch | Command::RunPluginCommand { .. }) => {
                self.status = skipped_pre_dispatched_command_status(&command);
            }
            Command::CloseActive => {
                if let Some(id) = self.active {
                    self.request_close_buffer(id);
                }
            }
            Command::ToggleReadOnly => self.toggle_active_read_only(),
            Command::OpenActiveFileLatestLocalHistory => {
                self.open_active_file_latest_local_history()
            }
            Command::SaveWorkspaceSnapshot => self.save_workspace_snapshot_now(),
            Command::RestoreLatestWorkspaceSnapshot => self.restore_latest_workspace_snapshot(),
            Command::ReopenClosedFile => self.reopen_closed_file(),
            Command::NextTab => self.activate_relative_tab(1),
            Command::PreviousTab => self.activate_relative_tab(-1),
            Command::NavigateBack => self.navigate_history(-1),
            Command::NavigateForward => self.navigate_history(1),
            command @ (Command::SelectLines
            | Command::SelectRectangularBlock
            | Command::ExpandSelection) => self.run_editor_command(command),
            Command::FindNext => self.goto_find_match(1),
            Command::FindPrevious => self.goto_find_match(-1),
            command @ (Command::SelectNextOccurrence
            | Command::SelectAllOccurrences
            | Command::ToggleLineComment) => self.run_editor_command(command),
            Command::NextDiagnostic => self.goto_diagnostic(1),
            Command::PreviousDiagnostic => self.goto_diagnostic(-1),
            Command::NextGitChange => self.goto_git_change(1),
            Command::PreviousGitChange => self.goto_git_change(-1),
            Command::NextDiffHunk => self.goto_active_diff_hunk(1),
            Command::PreviousDiffHunk => self.goto_active_diff_hunk(-1),
            Command::RefreshActiveDiff => self.refresh_active_diff(),
            Command::SwapActiveDiffSides => self.swap_active_diff_sides(),
            Command::RevealActiveFileInExplorer => self.reveal_active_file_in_explorer(),
            Command::RevealActiveFileInSourceControl => self.reveal_active_file_in_source_control(),
            Command::SelectActiveFileForCompare => self.select_active_file_for_compare(),
            Command::CompareActiveFileWithSelected => self.compare_active_file_with_selected(),
            Command::CompareActiveFileWithSaved => self.compare_active_file_with_saved(),
            Command::OpenActiveFileChanges => self.open_active_file_changes(),
            Command::OpenActiveFileStagedChanges => self.open_active_file_staged_changes(),
            Command::OpenActiveFileHeadChanges => self.open_active_file_head_changes(),
            Command::OpenActiveFileHeadRevision => self.open_active_file_head_revision(),
            Command::OpenActiveFileIndexRevision => self.open_active_file_index_revision(),
            Command::OpenActiveFileHunks => self.open_active_file_hunks(),
            Command::OpenActiveFileStagedHunks => self.open_active_file_staged_hunks(),
            Command::OpenActiveFileBlame => self.open_active_file_blame(),
            Command::StageActiveFileChanges => self.stage_active_file_changes(),
            Command::StageActiveFileHunk => self.stage_active_file_hunk(),
            Command::StageActiveDiffHunk => self.stage_active_diff_hunk(),
            Command::OpenActiveDiffBaseFile => self.open_active_diff_base_file(),
            Command::OpenActiveDiffHunkBase => self.open_active_diff_hunk_base(),
            Command::OpenActiveDiffSourceFile => self.open_active_diff_source_file(),
            Command::OpenActiveDiffHunkSource => self.open_active_diff_hunk_source(),
            Command::OpenActiveFileHunkDiff => {
                self.open_active_file_hunk_diff(kuroya_core::GitChangeStage::Unstaged);
            }
            Command::OpenActiveFileStagedHunkDiff => {
                self.open_active_file_hunk_diff(kuroya_core::GitChangeStage::Staged);
            }
            Command::OpenActiveAccessibleDiffViewer => self.open_active_accessible_diff_viewer(),
            Command::UnstageActiveFileChanges => self.unstage_active_file_changes(),
            Command::UnstageActiveFileHunk => self.unstage_active_file_hunk(),
            Command::UnstageActiveDiffHunk => self.unstage_active_diff_hunk(),
            Command::DiscardActiveFileChanges => self.discard_active_file_changes(),
            Command::DiscardActiveFileHunk => self.discard_active_file_hunk(),
            Command::DiscardActiveDiffHunk => self.discard_active_diff_hunk(),
            Command::AcceptCurrentConflict => {
                self.resolve_active_merge_conflict(MergeConflictResolution::Current);
            }
            Command::AcceptIncomingConflict => {
                self.resolve_active_merge_conflict(MergeConflictResolution::Incoming);
            }
            Command::AcceptBothConflicts => {
                self.resolve_active_merge_conflict(MergeConflictResolution::Both);
            }
            Command::GoToMatchingBracket => self.goto_matching_bracket(),
            Command::RequestDocumentHighlights => self.request_lsp_document_highlights(),
            Command::RequestHover => self.request_lsp_hover(),
            Command::GoToDefinition => self.request_lsp_definition(),
            Command::FindReferences => self.request_lsp_references(),
            Command::ShowCallHierarchy => self.request_lsp_call_hierarchy(),
            Command::ShowTypeHierarchy => self.request_lsp_type_hierarchy(),
            Command::RenameSymbol => self.begin_lsp_rename(),
            Command::RequestCompletions => self.request_lsp_completion(),
            Command::RequestSignatureHelp => self.request_lsp_signature_help(),
            Command::RequestFoldingRanges => self.request_lsp_folding_ranges(),
            Command::ToggleFold => self.toggle_fold_at_cursor(),
            Command::ExpandAllFolds => self.expand_all_folds(),
            Command::FormatDocument => self.request_lsp_formatting(),
            Command::RequestCodeActions => self.request_lsp_code_actions(),
            Command::RunWorkspaceTask(_) => {
                self.status = "Workspace task changed; run it again".to_owned();
            }
            Command::RunWorkspaceTaskSnapshot { index, fingerprint } => {
                self.run_workspace_task_snapshot(index, fingerprint)
            }
            Command::CancelWorkspaceTaskSnapshot { index, fingerprint } => {
                self.cancel_workspace_task_snapshot(index, fingerprint)
            }
            Command::RunWorkspaceTaskKind(kind) => self.run_workspace_task_kind(kind),
            Command::NextProjectSearchResult => self.goto_project_search_result(1),
            Command::PreviousProjectSearchResult => self.goto_project_search_result(-1),
            Command::FocusBuffer(id) => self.set_active_buffer(id),
            command @ (Command::Undo
            | Command::Redo
            | Command::IndentLines
            | Command::OutdentLines
            | Command::DeleteLines
            | Command::JoinLines
            | Command::DuplicateLines
            | Command::MoveLineUp
            | Command::MoveLineDown) => self.run_editor_command(command),
            Command::SplitEditorRight => {
                if let Some(id) = self.active {
                    self.split_buffer_right(id);
                }
            }
            Command::CloseEditorPane => self.close_active_pane(),
            Command::ResetEditorPaneWeights => self.reset_pane_weights(),
            command @ (Command::AddCursorAbove
            | Command::AddCursorBelow
            | Command::AddCursorsToLineEnds) => self.run_editor_command(command),
        }
    }
}

fn skipped_pre_dispatched_command_status(command: &Command) -> String {
    let label = command_label(command);
    let mut status =
        String::with_capacity("Skipped already-dispatched command: ".len() + label.len());
    let _ = write!(status, "Skipped already-dispatched command: {label}");
    status
}

pub(crate) fn command_requires_git(command: &Command) -> bool {
    matches!(
        command,
        Command::NextGitChange
            | Command::PreviousGitChange
            | Command::RevealActiveFileInSourceControl
            | Command::RevealFileInSourceControl(_)
            | Command::ToggleGitBranchSwitcher
            | Command::ToggleGitHistory
            | Command::ToggleGitStashes
            | Command::OpenSourceControlInIntegratedTerminal
            | Command::OpenActiveFileChanges
            | Command::OpenActiveFileStagedChanges
            | Command::OpenActiveFileHeadChanges
            | Command::OpenActiveFileHeadRevision
            | Command::OpenActiveFileIndexRevision
            | Command::OpenFileChanges(_)
            | Command::OpenStagedFileChanges(_)
            | Command::OpenFileHeadChanges(_)
            | Command::OpenFileHeadRevision(_)
            | Command::OpenFileIndexRevision(_)
            | Command::OpenAllChanges
            | Command::OpenAllUnstagedChanges
            | Command::OpenAllStagedChanges
            | Command::CopyAllChangesPatch
            | Command::CopyUnstagedChangesPatch
            | Command::CopyStagedChangesPatch
            | Command::CopyActiveFilePatch
            | Command::CopyActiveFileStagedPatch
            | Command::CopyFilePatch(_)
            | Command::CopyStagedFilePatch(_)
            | Command::OpenActiveFileHunks
            | Command::OpenActiveFileStagedHunks
            | Command::OpenFileHunks(_)
            | Command::OpenStagedFileHunks(_)
            | Command::OpenActiveFileBlame
            | Command::OpenFileBlame(_)
            | Command::OpenActiveFileHunkDiff
            | Command::OpenActiveFileStagedHunkDiff
            | Command::StageActiveFileChanges
            | Command::StageFileChange(_)
            | Command::StageAllChanges
            | Command::UnstageActiveFileChanges
            | Command::UnstageFileChange(_)
            | Command::UnstageAllChanges
            | Command::DiscardActiveFileChanges
            | Command::DiscardFileChanges(_)
            | Command::DiscardAllChanges
            | Command::StageFileHunk { .. }
            | Command::StageActiveFileHunk
            | Command::StageActiveDiffHunk
            | Command::CopyActiveFileHunkPatch
            | Command::CopyActiveFileStagedHunkPatch
            | Command::UnstageFileHunk { .. }
            | Command::UnstageActiveFileHunk
            | Command::UnstageActiveDiffHunk
            | Command::DiscardFileHunk { .. }
            | Command::DiscardActiveFileHunk
            | Command::DiscardActiveDiffHunk
            | Command::CommitStagedChanges
            | Command::SaveGitStash
            | Command::ApplyGitStash(_)
            | Command::PopGitStash(_)
            | Command::DropGitStash(_)
    )
}

#[cfg(test)]
mod tests {
    use super::{command_requires_git, skipped_pre_dispatched_command_status};
    use kuroya_core::Command;
    use std::path::PathBuf;

    #[test]
    fn skipped_pre_dispatched_command_status_names_command() {
        assert_eq!(
            skipped_pre_dispatched_command_status(&Command::ToggleQuickOpen),
            "Skipped already-dispatched command: Quick Open"
        );
        assert_eq!(
            skipped_pre_dispatched_command_status(&Command::CopyActiveFilePath),
            "Skipped already-dispatched command: Copy Path"
        );
    }

    #[test]
    fn command_requires_git_identifies_source_control_commands() {
        assert!(command_requires_git(&Command::CommitStagedChanges));
        assert!(command_requires_git(&Command::OpenFileHeadRevision(
            PathBuf::from("src/lib.rs")
        )));
        assert!(command_requires_git(&Command::ToggleGitBranchSwitcher));
        assert!(command_requires_git(&Command::ToggleGitHistory));
        assert!(command_requires_git(&Command::ToggleGitStashes));
        assert!(command_requires_git(
            &Command::OpenSourceControlInIntegratedTerminal
        ));
        assert!(command_requires_git(&Command::OpenActiveFileHunkDiff));
        assert!(command_requires_git(&Command::OpenActiveFileStagedHunkDiff));
        assert!(!command_requires_git(&Command::ToggleSourceControl));
        assert!(!command_requires_git(&Command::CompareActiveFileWithSaved));
    }
}
