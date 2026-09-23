use std::borrow::Cow;

use crate::path_display::{display_path_label_cow, sanitized_display_label_cow};
use kuroya_core::keymap::KeyBinding;
use kuroya_core::{Command, WorkspaceTaskKind};

#[cfg(test)]
pub(crate) use crate::command_catalog::command_catalog;

const PLUGIN_COMMAND_LABEL_FRAGMENT_MAX_CHARS: usize = 96;

pub(crate) fn keybinding_chord_for_command(
    bindings: &[KeyBinding],
    command: &Command,
) -> Option<String> {
    bindings
        .iter()
        .find(|binding| &binding.command == command)
        .map(|binding| binding.chord.clone())
}

pub(crate) fn command_label(command: &Command) -> String {
    match command {
        Command::NewFile => "New File".to_owned(),
        Command::OpenFile(path) => format!("Open File {}", command_path_label(path)),
        Command::OpenFileAt { path, line, column } => {
            format!("Open File {}:{}:{}", command_path_label(path), line, column)
        }
        Command::SelectActiveFileForCompare => "Select Active File for Compare".to_owned(),
        Command::CompareActiveFileWithSelected => "Compare Active File with Selected".to_owned(),
        Command::CompareActiveFileWithSaved => "Compare Active File with Saved".to_owned(),
        Command::SelectFileForCompare(path) => {
            format!("Select for Compare {}", command_path_label(path))
        }
        Command::CompareFileWithSelected(path) => {
            format!("Compare with Selected {}", command_path_label(path))
        }
        Command::OpenWorkspace(path) => format!("Open Folder {}", command_path_label(path)),
        Command::OpenWorkspacePrompt => "Open Folder".to_owned(),
        Command::TrustWorkspace => "Trust Workspace".to_owned(),
        Command::RevokeWorkspaceTrust => "Revoke Workspace Trust".to_owned(),
        Command::CreateFileIn(path) => format!("Create File in {}", command_path_label(path)),
        Command::CreateFolderIn(path) => format!("Create Folder in {}", command_path_label(path)),
        Command::RenamePath(path) => format!("Rename {}", command_path_label(path)),
        Command::DeletePath(path) => format!("Delete {}", command_path_label(path)),
        Command::RefreshWorkspace => "Refresh Workspace".to_owned(),
        Command::SaveActive => "Save Active File".to_owned(),
        Command::SaveAs => "Save As".to_owned(),
        Command::SaveAll => "Save All Files".to_owned(),
        Command::ReloadActiveFromDisk => "Reload Active File".to_owned(),
        Command::OpenActiveFileLatestLocalHistory => {
            "Open Latest Local History Snapshot".to_owned()
        }
        Command::OpenLocalHistoryBrowser => "Browse Local History".to_owned(),
        Command::SaveWorkspaceSnapshot => "Save Workspace Snapshot".to_owned(),
        Command::RestoreLatestWorkspaceSnapshot => "Restore Latest Workspace Snapshot".to_owned(),
        Command::ToggleReadOnly => "Toggle Read Only".to_owned(),
        Command::ToggleMinimap => "Toggle Minimap".to_owned(),
        Command::ToggleStickyScroll => "Toggle Sticky Scroll".to_owned(),
        Command::ToggleVimMode => "Toggle Vim Mode".to_owned(),
        Command::ReloadSettings => "Reload Settings".to_owned(),
        Command::CheckForUpdates => "Check for Updates".to_owned(),
        Command::ToggleSettingsPanel => "Settings".to_owned(),
        Command::OpenSettingsFile => "Open Settings".to_owned(),
        Command::ToggleKeybindingsPanel => "Keyboard Shortcuts".to_owned(),
        Command::ToggleThemePicker => "Themes".to_owned(),
        Command::CycleTheme => "Cycle Theme".to_owned(),
        Command::CloseActive => "Close Active File".to_owned(),
        Command::ReopenClosedFile => "Reopen Closed File".to_owned(),
        Command::NextTab => "Next Tab".to_owned(),
        Command::PreviousTab => "Previous Tab".to_owned(),
        Command::NavigateBack => "Navigate Back".to_owned(),
        Command::NavigateForward => "Navigate Forward".to_owned(),
        Command::ToggleCommandPalette => "Command Palette".to_owned(),
        Command::ToggleQuickOpen => "Quick Open".to_owned(),
        Command::ToggleBufferFind => "Find in File".to_owned(),
        Command::ToggleGoToLine => "Go to Line".to_owned(),
        Command::SelectLines => "Select Lines".to_owned(),
        Command::SelectRectangularBlock => "Select Rectangular Block".to_owned(),
        Command::ExpandSelection => "Expand Selection".to_owned(),
        Command::FindNext => "Find Next".to_owned(),
        Command::FindPrevious => "Find Previous".to_owned(),
        Command::SelectNextOccurrence => "Select Next Occurrence".to_owned(),
        Command::SelectAllOccurrences => "Select All Occurrences".to_owned(),
        Command::GoToMatchingBracket => "Go to Matching Bracket".to_owned(),
        Command::ToggleLineComment => "Toggle Line Comment".to_owned(),
        Command::NextDiagnostic => "Next Diagnostic".to_owned(),
        Command::PreviousDiagnostic => "Previous Diagnostic".to_owned(),
        Command::NextGitChange => "Next Git Change".to_owned(),
        Command::PreviousGitChange => "Previous Git Change".to_owned(),
        Command::NextDiffHunk => "Next Diff Hunk".to_owned(),
        Command::PreviousDiffHunk => "Previous Diff Hunk".to_owned(),
        Command::RefreshActiveDiff => "Refresh Diff".to_owned(),
        Command::SwapActiveDiffSides => "Swap Compare Sides".to_owned(),
        Command::ToggleSourceControl => "Source Control".to_owned(),
        Command::CycleSourceControlPlacement => "Cycle Source Control Placement".to_owned(),
        Command::RevealActiveFileInExplorer => "Reveal in Explorer".to_owned(),
        Command::RevealFileInExplorer(path) => {
            format!("Reveal in Explorer {}", command_path_label(path))
        }
        Command::RevealActiveFileInSourceControl => "Reveal in Source Control".to_owned(),
        Command::RevealFileInSourceControl(path) => {
            format!("Reveal in Source Control {}", command_path_label(path))
        }
        Command::CopyActiveFilePath => "Copy Path".to_owned(),
        Command::CopyActiveFileRelativePath => "Copy Relative Path".to_owned(),
        Command::CopyFilePath(path) => format!("Copy Path {}", command_path_label(path)),
        Command::CopyFileRelativePath(path) => {
            format!("Copy Relative Path {}", command_path_label(path))
        }
        Command::OpenActiveFileChanges => "Open Changes".to_owned(),
        Command::OpenActiveFileStagedChanges => "Open Staged Changes".to_owned(),
        Command::OpenActiveFileHeadChanges => "Compare with HEAD".to_owned(),
        Command::OpenActiveFileHeadRevision => "Open File at HEAD".to_owned(),
        Command::OpenActiveFileIndexRevision => "Open File at Index".to_owned(),
        Command::OpenFileChanges(path) => format!("Open Changes {}", command_path_label(path)),
        Command::OpenStagedFileChanges(path) => {
            format!("Open Staged Changes {}", command_path_label(path))
        }
        Command::OpenFileHeadChanges(path) => {
            format!("Compare with HEAD {}", command_path_label(path))
        }
        Command::OpenFileHeadRevision(path) => {
            format!("Open File at HEAD {}", command_path_label(path))
        }
        Command::OpenFileIndexRevision(path) => {
            format!("Open File at Index {}", command_path_label(path))
        }
        Command::OpenAllChanges => "Open All Changes".to_owned(),
        Command::OpenAllUnstagedChanges => "Open All Unstaged Changes".to_owned(),
        Command::OpenAllStagedChanges => "Open All Staged Changes".to_owned(),
        Command::CopyAllChangesPatch => "Copy All Changes Patch".to_owned(),
        Command::CopyUnstagedChangesPatch => "Copy Unstaged Changes Patch".to_owned(),
        Command::CopyStagedChangesPatch => "Copy Staged Changes Patch".to_owned(),
        Command::CopyActiveFilePatch => "Copy Active File Patch".to_owned(),
        Command::CopyActiveFileStagedPatch => "Copy Active File Staged Patch".to_owned(),
        Command::CopyFilePatch(path) => format!("Copy Patch {}", command_path_label(path)),
        Command::CopyStagedFilePatch(path) => {
            format!("Copy Staged Patch {}", command_path_label(path))
        }
        Command::OpenActiveFileHunks => "Open Hunks".to_owned(),
        Command::OpenActiveFileStagedHunks => "Open Staged Hunks".to_owned(),
        Command::OpenFileHunks(path) => format!("Open Hunks {}", command_path_label(path)),
        Command::OpenStagedFileHunks(path) => {
            format!("Open Staged Hunks {}", command_path_label(path))
        }
        Command::OpenActiveFileBlame => "Open Blame".to_owned(),
        Command::OpenFileBlame(path) => format!("Open Blame {}", command_path_label(path)),
        Command::StageActiveFileChanges => "Stage Active File Changes".to_owned(),
        Command::StageFileChange(path) => format!("Stage Changes {}", command_path_label(path)),
        Command::StageAllChanges => "Stage All Changes".to_owned(),
        Command::UnstageActiveFileChanges => "Unstage Active File Changes".to_owned(),
        Command::UnstageFileChange(path) => format!("Unstage Changes {}", command_path_label(path)),
        Command::UnstageAllChanges => "Unstage All Changes".to_owned(),
        Command::DiscardActiveFileChanges => "Discard Active File Changes".to_owned(),
        Command::DiscardFileChanges(path) => {
            format!("Discard Changes {}", command_path_label(path))
        }
        Command::DiscardAllChanges => "Discard All Changes".to_owned(),
        Command::StageFileHunk {
            path, hunk_index, ..
        } => {
            format!("Stage Hunk {hunk_index} {}", command_path_label(path))
        }
        Command::StageActiveFileHunk => "Stage Current Hunk".to_owned(),
        Command::StageActiveDiffHunk => "Stage Current Diff Hunk".to_owned(),
        Command::OpenActiveDiffBaseFile => "Open Diff Base File".to_owned(),
        Command::OpenActiveDiffHunkBase => "Open Base at Current Diff Hunk".to_owned(),
        Command::OpenActiveDiffSourceFile => "Open Diff Source File".to_owned(),
        Command::OpenActiveDiffHunkSource => "Open Source at Current Diff Hunk".to_owned(),
        Command::OpenActiveFileHunkDiff => "Open Current Hunk Diff".to_owned(),
        Command::OpenActiveFileStagedHunkDiff => "Open Current Staged Hunk Diff".to_owned(),
        Command::OpenActiveAccessibleDiffViewer => "Open Accessible Diff Viewer".to_owned(),
        Command::CopyActiveFileHunkPatch => "Copy Current Hunk Patch".to_owned(),
        Command::CopyActiveFileStagedHunkPatch => "Copy Current Staged Hunk Patch".to_owned(),
        Command::CopyActiveDiffPatch => "Copy Diff Patch".to_owned(),
        Command::CopyActiveDiffHunkPatch => "Copy Current Diff Hunk Patch".to_owned(),
        Command::UnstageFileHunk {
            path, hunk_index, ..
        } => {
            format!("Unstage Hunk {hunk_index} {}", command_path_label(path))
        }
        Command::UnstageActiveFileHunk => "Unstage Current Hunk".to_owned(),
        Command::UnstageActiveDiffHunk => "Unstage Current Diff Hunk".to_owned(),
        Command::DiscardFileHunk {
            path, hunk_index, ..
        } => {
            format!("Discard Hunk {hunk_index} {}", command_path_label(path))
        }
        Command::DiscardActiveFileHunk => "Discard Current Hunk".to_owned(),
        Command::DiscardActiveDiffHunk => "Discard Current Diff Hunk".to_owned(),
        Command::CommitStagedChanges => "Commit Staged Changes".to_owned(),
        Command::AcceptCurrentConflict => "Accept Current Conflict".to_owned(),
        Command::AcceptIncomingConflict => "Accept Incoming Conflict".to_owned(),
        Command::AcceptBothConflicts => "Accept Both Conflicts".to_owned(),
        Command::ToggleGitBranchSwitcher => "Switch Git Branch".to_owned(),
        Command::ToggleGitHistory => "Git History".to_owned(),
        Command::ToggleGitStashes => "Git Stashes".to_owned(),
        Command::OpenSourceControlInIntegratedTerminal => {
            "Open Source Control in Integrated Terminal".to_owned()
        }
        Command::SaveGitStash => "Save Git Stash".to_owned(),
        Command::ApplyGitStash(index) => format!("Apply Git Stash {index}"),
        Command::PopGitStash(index) => format!("Pop Git Stash {index}"),
        Command::DropGitStash(index) => format!("Drop Git Stash {index}"),
        Command::RequestDocumentHighlights => "Document Highlights".to_owned(),
        Command::RequestHover => "Show Hover".to_owned(),
        Command::GoToDefinition => "Go to Definition".to_owned(),
        Command::FindReferences => "Find References".to_owned(),
        Command::ShowCallHierarchy => "Show Call Hierarchy".to_owned(),
        Command::ShowTypeHierarchy => "Show Type Hierarchy".to_owned(),
        Command::RenameSymbol => "Rename Symbol".to_owned(),
        Command::ToggleSymbolsPanel => "File Symbols".to_owned(),
        Command::CycleSymbolsPanelPlacement => "Cycle File Symbols Placement".to_owned(),
        Command::ToggleWorkspaceSymbols => "Workspace Symbols".to_owned(),
        Command::ToggleWorkspaceTasks => "Workspace Tasks".to_owned(),
        Command::RunWorkspaceTask(index) => format!("Run Workspace Task {index}"),
        Command::RunWorkspaceTaskSnapshot { index, .. } => {
            format!("Run Workspace Task {index}")
        }
        Command::CancelWorkspaceTaskSnapshot { index, .. } => {
            format!("Cancel Workspace Task {index}")
        }
        Command::RunWorkspaceTaskKind(kind) => workspace_task_kind_command_label(*kind),
        Command::RunPluginCommand {
            plugin_id,
            command_id,
        } => {
            let plugin_id_label = command_plugin_identifier_label_cow(plugin_id, "plugin");
            let command_id_label = command_plugin_identifier_label_cow(command_id, "command");
            format!(
                "Run Plugin Command {}:{}",
                plugin_id_label, command_id_label
            )
        }
        Command::RequestCompletions => "Show Completions".to_owned(),
        Command::RequestSignatureHelp => "Signature Help".to_owned(),
        Command::RequestFoldingRanges => "Load Folding Ranges".to_owned(),
        Command::ToggleFold => "Toggle Fold".to_owned(),
        Command::ExpandAllFolds => "Expand All Folds".to_owned(),
        Command::FormatDocument => "Format Document".to_owned(),
        Command::RequestCodeActions => "Code Actions".to_owned(),
        Command::ToggleProjectSearch => "Project Search".to_owned(),
        Command::CycleProjectSearchPlacement => "Legacy Project Search Placement".to_owned(),
        Command::NextProjectSearchResult => "Next Project Search Result".to_owned(),
        Command::PreviousProjectSearchResult => "Previous Project Search Result".to_owned(),
        Command::ToggleDiagnosticsPanel => "Diagnostics".to_owned(),
        Command::CycleDiagnosticsPanelPlacement => "Cycle Diagnostics Placement".to_owned(),
        Command::ToggleDevtools => "Internal Devtools".to_owned(),
        Command::ToggleTerminal => "Toggle Terminal".to_owned(),
        Command::ToggleTerminalSearch => "Search Terminal Output".to_owned(),
        Command::NextTerminalSearchResult => "Next Terminal Search Result".to_owned(),
        Command::PreviousTerminalSearchResult => "Previous Terminal Search Result".to_owned(),
        Command::NextTerminalSession => "Next Terminal Session".to_owned(),
        Command::PreviousTerminalSession => "Previous Terminal Session".to_owned(),
        Command::SplitEditorRight => "Split Editor Right".to_owned(),
        Command::CloseEditorPane => "Close Editor Pane".to_owned(),
        Command::ResetEditorPaneWeights => "Reset Editor Split Widths".to_owned(),
        Command::IndentLines => "Indent Lines".to_owned(),
        Command::OutdentLines => "Outdent Lines".to_owned(),
        Command::DeleteLines => "Delete Lines".to_owned(),
        Command::JoinLines => "Join Lines".to_owned(),
        Command::DuplicateLines => "Duplicate Lines".to_owned(),
        Command::MoveLineUp => "Move Line Up".to_owned(),
        Command::MoveLineDown => "Move Line Down".to_owned(),
        Command::AddCursorAbove => "Add Cursor Above".to_owned(),
        Command::AddCursorBelow => "Add Cursor Below".to_owned(),
        Command::AddCursorsToLineEnds => "Add Cursors to Line Ends".to_owned(),
        Command::FocusBuffer(id) => format!("Focus Buffer {id}"),
        Command::Undo => "Undo".to_owned(),
        Command::Redo => "Redo".to_owned(),
    }
}

fn command_path_label(path: &std::path::Path) -> Cow<'_, str> {
    display_path_label_cow(path)
}

#[cfg(test)]
fn command_plugin_identifier_label(value: &str, fallback: &str) -> String {
    command_plugin_identifier_label_cow(value, fallback).into_owned()
}

fn command_plugin_identifier_label_cow<'a>(value: &'a str, fallback: &str) -> Cow<'a, str> {
    sanitized_display_label_cow(value, PLUGIN_COMMAND_LABEL_FRAGMENT_MAX_CHARS, fallback)
}

fn workspace_task_kind_command_label(kind: WorkspaceTaskKind) -> String {
    match kind {
        WorkspaceTaskKind::Build => "Run Build Task".to_owned(),
        WorkspaceTaskKind::Test => "Run Test Task".to_owned(),
        WorkspaceTaskKind::Run => "Run Configuration".to_owned(),
        WorkspaceTaskKind::Custom => "Run Workspace Task".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Command, Cow, PLUGIN_COMMAND_LABEL_FRAGMENT_MAX_CHARS, command_label,
        command_plugin_identifier_label, command_plugin_identifier_label_cow,
    };

    #[test]
    fn command_plugin_identifier_label_cow_borrows_clean_ascii_and_unicode() {
        assert!(matches!(
            command_plugin_identifier_label_cow("example.plugin", "plugin"),
            Cow::Borrowed("example.plugin")
        ));

        let unicode = "plugin-\u{03bb}.command";
        match command_plugin_identifier_label_cow(unicode, "plugin") {
            Cow::Borrowed(label) => assert_eq!(label, unicode),
            Cow::Owned(label) => panic!("expected borrowed label, got {label:?}"),
        }
    }

    #[test]
    fn command_plugin_identifier_label_cow_owns_dirty_truncated_and_fallback_output() {
        let dirty = command_plugin_identifier_label_cow("plugin\nid\u{202e}", "plugin");
        assert_eq!(dirty.as_ref(), "plugin id");
        assert!(matches!(dirty, Cow::Owned(_)));

        let long = format!(
            "plugin-{}-tail",
            "x".repeat(PLUGIN_COMMAND_LABEL_FRAGMENT_MAX_CHARS * 2)
        );
        let truncated = command_plugin_identifier_label_cow(&long, "plugin");
        assert!(truncated.contains("..."));
        assert_eq!(
            truncated.chars().count(),
            PLUGIN_COMMAND_LABEL_FRAGMENT_MAX_CHARS
        );
        assert!(matches!(truncated, Cow::Owned(_)));

        let fallback = command_plugin_identifier_label_cow("\u{200b}\u{202e}", "plugin");
        assert_eq!(fallback.as_ref(), "plugin");
        assert!(matches!(fallback, Cow::Owned(_)));
    }

    #[test]
    fn command_plugin_identifier_label_string_wrapper_matches_cow_output() {
        let long = format!(
            "command-{}-tail",
            "x".repeat(PLUGIN_COMMAND_LABEL_FRAGMENT_MAX_CHARS * 2)
        );
        let cases = [
            ("example.plugin", "plugin"),
            ("plugin\nid\u{202e}", "plugin"),
            ("\u{200b}\u{202e}", "plugin"),
            (long.as_str(), "command"),
        ];

        for (value, fallback) in cases {
            assert_eq!(
                command_plugin_identifier_label(value, fallback),
                command_plugin_identifier_label_cow(value, fallback).into_owned()
            );
        }
    }

    #[test]
    fn command_label_run_plugin_command_matches_identifier_wrapper_labels() {
        let plugin_id = format!("plugin\n{}\u{202e}\u{0007}", "id-".repeat(64));
        let command_id = format!("command\r\n{}\u{2066}\u{001b}", "id-".repeat(64));
        let command = Command::RunPluginCommand {
            plugin_id: plugin_id.clone(),
            command_id: command_id.clone(),
        };

        assert_eq!(
            command_label(&command),
            format!(
                "Run Plugin Command {}:{}",
                command_plugin_identifier_label(&plugin_id, "plugin"),
                command_plugin_identifier_label(&command_id, "command")
            )
        );
        assert_eq!(
            command,
            Command::RunPluginCommand {
                plugin_id,
                command_id,
            }
        );
    }
}
