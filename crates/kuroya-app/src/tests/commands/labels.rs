use crate::commands::command_label;
use kuroya_core::{Command, WorkspaceTaskKind};
use std::path::PathBuf;

#[test]
fn command_label_covers_settings_commands() {
    assert_eq!(command_label(&Command::ToggleSettingsPanel), "Settings");
    assert_eq!(command_label(&Command::OpenSettingsFile), "Open Settings");
    assert_eq!(command_label(&Command::TrustWorkspace), "Trust Workspace");
    assert_eq!(
        command_label(&Command::RevokeWorkspaceTrust),
        "Revoke Workspace Trust"
    );
    assert_eq!(
        command_label(&Command::ToggleKeybindingsPanel),
        "Keyboard Shortcuts"
    );
    assert_eq!(
        command_label(&Command::SelectAllOccurrences),
        "Select All Occurrences"
    );
    assert_eq!(
        command_label(&Command::GoToMatchingBracket),
        "Go to Matching Bracket"
    );
    assert_eq!(
        command_label(&Command::ToggleLineComment),
        "Toggle Line Comment"
    );
    assert_eq!(command_label(&Command::SelectLines), "Select Lines");
    assert_eq!(command_label(&Command::ExpandSelection), "Expand Selection");
    assert_eq!(command_label(&Command::DeleteLines), "Delete Lines");
    assert_eq!(command_label(&Command::JoinLines), "Join Lines");
    assert_eq!(
        command_label(&Command::AddCursorsToLineEnds),
        "Add Cursors to Line Ends"
    );
    assert_eq!(
        command_label(&Command::ReloadActiveFromDisk),
        "Reload Active File"
    );
    assert_eq!(
        command_label(&Command::OpenActiveFileLatestLocalHistory),
        "Open Latest Local History Snapshot"
    );
    assert_eq!(
        command_label(&Command::OpenLocalHistoryBrowser),
        "Browse Local History"
    );
    assert_eq!(
        command_label(&Command::SaveWorkspaceSnapshot),
        "Save Workspace Snapshot"
    );
    assert_eq!(
        command_label(&Command::RestoreLatestWorkspaceSnapshot),
        "Restore Latest Workspace Snapshot"
    );
    assert_eq!(command_label(&Command::ToggleReadOnly), "Toggle Read Only");
    assert_eq!(command_label(&Command::ToggleMinimap), "Toggle Minimap");
    assert_eq!(
        command_label(&Command::ToggleStickyScroll),
        "Toggle Sticky Scroll"
    );
    assert_eq!(command_label(&Command::ToggleVimMode), "Toggle Vim Mode");
    assert_eq!(
        command_label(&Command::ReopenClosedFile),
        "Reopen Closed File"
    );
    assert_eq!(
        command_label(&Command::SelectActiveFileForCompare),
        "Select Active File for Compare"
    );
    assert_eq!(
        command_label(&Command::CompareActiveFileWithSelected),
        "Compare Active File with Selected"
    );
    assert_eq!(
        command_label(&Command::CompareActiveFileWithSaved),
        "Compare Active File with Saved"
    );
    assert_eq!(
        command_label(&Command::SelectFileForCompare("C:/repo/src/main.rs".into())),
        "Select for Compare main.rs"
    );
    assert_eq!(
        command_label(&Command::CompareFileWithSelected(
            "C:/repo/src/lib.rs".into()
        )),
        "Compare with Selected lib.rs"
    );
    assert_eq!(command_label(&Command::NextGitChange), "Next Git Change");
    assert_eq!(
        command_label(&Command::PreviousGitChange),
        "Previous Git Change"
    );
    assert_eq!(command_label(&Command::NextDiffHunk), "Next Diff Hunk");
    assert_eq!(
        command_label(&Command::PreviousDiffHunk),
        "Previous Diff Hunk"
    );
    assert_eq!(command_label(&Command::RefreshActiveDiff), "Refresh Diff");
    assert_eq!(
        command_label(&Command::SwapActiveDiffSides),
        "Swap Compare Sides"
    );
    assert_eq!(
        command_label(&Command::ToggleSourceControl),
        "Source Control"
    );
    assert_eq!(
        command_label(&Command::RevealActiveFileInExplorer),
        "Reveal in Explorer"
    );
    assert_eq!(
        command_label(&Command::RevealFileInExplorer("C:/repo/src/main.rs".into())),
        "Reveal in Explorer main.rs"
    );
    assert_eq!(
        command_label(&Command::RevealActiveFileInSourceControl),
        "Reveal in Source Control"
    );
    assert_eq!(
        command_label(&Command::RevealFileInSourceControl(
            "C:/repo/src/main.rs".into()
        )),
        "Reveal in Source Control main.rs"
    );
    assert_eq!(command_label(&Command::CopyActiveFilePath), "Copy Path");
    assert_eq!(
        command_label(&Command::CopyActiveFileRelativePath),
        "Copy Relative Path"
    );
    assert_eq!(
        command_label(&Command::CopyFilePath("C:/repo/src/main.rs".into())),
        "Copy Path main.rs"
    );
    assert_eq!(
        command_label(&Command::CopyFileRelativePath("C:/repo/src/main.rs".into())),
        "Copy Relative Path main.rs"
    );
    assert_eq!(
        command_label(&Command::OpenActiveFileChanges),
        "Open Changes"
    );
    assert_eq!(
        command_label(&Command::OpenActiveFileStagedChanges),
        "Open Staged Changes"
    );
    assert_eq!(
        command_label(&Command::OpenActiveFileHeadChanges),
        "Compare with HEAD"
    );
    assert_eq!(
        command_label(&Command::OpenActiveFileHeadRevision),
        "Open File at HEAD"
    );
    assert_eq!(
        command_label(&Command::OpenActiveFileIndexRevision),
        "Open File at Index"
    );
    assert_eq!(
        command_label(&Command::OpenFileHeadChanges("C:/repo/src/main.rs".into())),
        "Compare with HEAD main.rs"
    );
    assert_eq!(
        command_label(&Command::OpenFileHeadRevision("C:/repo/src/main.rs".into())),
        "Open File at HEAD main.rs"
    );
    assert_eq!(
        command_label(&Command::OpenFileIndexRevision(
            "C:/repo/src/main.rs".into()
        )),
        "Open File at Index main.rs"
    );
    assert_eq!(command_label(&Command::OpenAllChanges), "Open All Changes");
    assert_eq!(
        command_label(&Command::OpenAllUnstagedChanges),
        "Open All Unstaged Changes"
    );
    assert_eq!(
        command_label(&Command::OpenAllStagedChanges),
        "Open All Staged Changes"
    );
    assert_eq!(
        command_label(&Command::CopyAllChangesPatch),
        "Copy All Changes Patch"
    );
    assert_eq!(
        command_label(&Command::CopyUnstagedChangesPatch),
        "Copy Unstaged Changes Patch"
    );
    assert_eq!(
        command_label(&Command::CopyStagedChangesPatch),
        "Copy Staged Changes Patch"
    );
    assert_eq!(
        command_label(&Command::CopyActiveFilePatch),
        "Copy Active File Patch"
    );
    assert_eq!(
        command_label(&Command::CopyActiveFileStagedPatch),
        "Copy Active File Staged Patch"
    );
    assert_eq!(
        command_label(&Command::CopyFilePatch("C:/repo/src/main.rs".into())),
        "Copy Patch main.rs"
    );
    assert_eq!(
        command_label(&Command::CopyStagedFilePatch("C:/repo/src/main.rs".into())),
        "Copy Staged Patch main.rs"
    );
    assert_eq!(command_label(&Command::OpenActiveFileHunks), "Open Hunks");
    assert_eq!(
        command_label(&Command::OpenActiveFileStagedHunks),
        "Open Staged Hunks"
    );
    assert_eq!(
        command_label(&Command::OpenFileHunks("C:/repo/src/main.rs".into())),
        "Open Hunks main.rs"
    );
    assert_eq!(
        command_label(&Command::OpenStagedFileHunks("C:/repo/src/main.rs".into())),
        "Open Staged Hunks main.rs"
    );
    assert_eq!(command_label(&Command::OpenActiveFileBlame), "Open Blame");
    assert_eq!(
        command_label(&Command::StageActiveFileChanges),
        "Stage Active File Changes"
    );
    assert_eq!(
        command_label(&Command::StageAllChanges),
        "Stage All Changes"
    );
    assert_eq!(
        command_label(&Command::UnstageActiveFileChanges),
        "Unstage Active File Changes"
    );
    assert_eq!(
        command_label(&Command::UnstageAllChanges),
        "Unstage All Changes"
    );
    assert_eq!(
        command_label(&Command::DiscardActiveFileChanges),
        "Discard Active File Changes"
    );
    assert_eq!(
        command_label(&Command::DiscardAllChanges),
        "Discard All Changes"
    );
    assert_eq!(
        command_label(&Command::StageFileHunk {
            path: "C:/repo/src/main.rs".into(),
            hunk_index: 1,
            hunk_fingerprint: None,
        }),
        "Stage Hunk 1 main.rs"
    );
    assert_eq!(
        command_label(&Command::StageActiveFileHunk),
        "Stage Current Hunk"
    );
    assert_eq!(
        command_label(&Command::StageActiveDiffHunk),
        "Stage Current Diff Hunk"
    );
    assert_eq!(
        command_label(&Command::OpenActiveDiffBaseFile),
        "Open Diff Base File"
    );
    assert_eq!(
        command_label(&Command::OpenActiveDiffHunkBase),
        "Open Base at Current Diff Hunk"
    );
    assert_eq!(
        command_label(&Command::OpenActiveDiffSourceFile),
        "Open Diff Source File"
    );
    assert_eq!(
        command_label(&Command::OpenActiveDiffHunkSource),
        "Open Source at Current Diff Hunk"
    );
    assert_eq!(
        command_label(&Command::OpenActiveFileHunkDiff),
        "Open Current Hunk Diff"
    );
    assert_eq!(
        command_label(&Command::OpenActiveFileStagedHunkDiff),
        "Open Current Staged Hunk Diff"
    );
    assert_eq!(
        command_label(&Command::OpenActiveAccessibleDiffViewer),
        "Open Accessible Diff Viewer"
    );
    assert_eq!(
        command_label(&Command::CopyActiveFileHunkPatch),
        "Copy Current Hunk Patch"
    );
    assert_eq!(
        command_label(&Command::CopyActiveFileStagedHunkPatch),
        "Copy Current Staged Hunk Patch"
    );
    assert_eq!(
        command_label(&Command::CopyActiveDiffPatch),
        "Copy Diff Patch"
    );
    assert_eq!(
        command_label(&Command::CopyActiveDiffHunkPatch),
        "Copy Current Diff Hunk Patch"
    );
    assert_eq!(
        command_label(&Command::UnstageFileHunk {
            path: "C:/repo/src/main.rs".into(),
            hunk_index: 1,
            hunk_fingerprint: None,
        }),
        "Unstage Hunk 1 main.rs"
    );
    assert_eq!(
        command_label(&Command::UnstageActiveFileHunk),
        "Unstage Current Hunk"
    );
    assert_eq!(
        command_label(&Command::UnstageActiveDiffHunk),
        "Unstage Current Diff Hunk"
    );
    assert_eq!(
        command_label(&Command::DiscardFileHunk {
            path: "C:/repo/src/main.rs".into(),
            hunk_index: 1,
            hunk_fingerprint: None,
        }),
        "Discard Hunk 1 main.rs"
    );
    assert_eq!(
        command_label(&Command::DiscardActiveFileHunk),
        "Discard Current Hunk"
    );
    assert_eq!(
        command_label(&Command::DiscardActiveDiffHunk),
        "Discard Current Diff Hunk"
    );
    assert_eq!(
        command_label(&Command::CommitStagedChanges),
        "Commit Staged Changes"
    );
    assert_eq!(
        command_label(&Command::AcceptCurrentConflict),
        "Accept Current Conflict"
    );
    assert_eq!(
        command_label(&Command::AcceptIncomingConflict),
        "Accept Incoming Conflict"
    );
    assert_eq!(
        command_label(&Command::AcceptBothConflicts),
        "Accept Both Conflicts"
    );
    assert_eq!(
        command_label(&Command::ToggleGitBranchSwitcher),
        "Switch Git Branch"
    );
    assert_eq!(command_label(&Command::ToggleGitHistory), "Git History");
    assert_eq!(command_label(&Command::ToggleGitStashes), "Git Stashes");
    assert_eq!(
        command_label(&Command::OpenSourceControlInIntegratedTerminal),
        "Open Source Control in Integrated Terminal"
    );
    assert_eq!(command_label(&Command::SaveGitStash), "Save Git Stash");
    assert_eq!(
        command_label(&Command::ApplyGitStash(2)),
        "Apply Git Stash 2"
    );
    assert_eq!(command_label(&Command::PopGitStash(2)), "Pop Git Stash 2");
    assert_eq!(command_label(&Command::DropGitStash(2)), "Drop Git Stash 2");
    assert_eq!(
        command_label(&Command::NextProjectSearchResult),
        "Next Project Search Result"
    );
    assert_eq!(
        command_label(&Command::PreviousProjectSearchResult),
        "Previous Project Search Result"
    );
    assert_eq!(command_label(&Command::ToggleDevtools), "Internal Devtools");
    assert_eq!(
        command_label(&Command::ToggleTerminalSearch),
        "Search Terminal Output"
    );
    assert_eq!(
        command_label(&Command::NextTerminalSearchResult),
        "Next Terminal Search Result"
    );
    assert_eq!(
        command_label(&Command::PreviousTerminalSearchResult),
        "Previous Terminal Search Result"
    );
    assert_eq!(
        command_label(&Command::NextTerminalSession),
        "Next Terminal Session"
    );
    assert_eq!(
        command_label(&Command::PreviousTerminalSession),
        "Previous Terminal Session"
    );
    assert_eq!(
        command_label(&Command::ToggleWorkspaceTasks),
        "Workspace Tasks"
    );
    assert_eq!(
        command_label(&Command::RunWorkspaceTask(3)),
        "Run Workspace Task 3"
    );
    assert_eq!(
        command_label(&Command::RunWorkspaceTaskSnapshot {
            index: 3,
            fingerprint: 42,
        }),
        "Run Workspace Task 3"
    );
    assert_eq!(
        command_label(&Command::CancelWorkspaceTaskSnapshot {
            index: 3,
            fingerprint: 42,
        }),
        "Cancel Workspace Task 3"
    );
    assert_eq!(
        command_label(&Command::RunWorkspaceTaskKind(WorkspaceTaskKind::Build)),
        "Run Build Task"
    );
    assert_eq!(
        command_label(&Command::RunWorkspaceTaskKind(WorkspaceTaskKind::Test)),
        "Run Test Task"
    );
    assert_eq!(
        command_label(&Command::RunWorkspaceTaskKind(WorkspaceTaskKind::Run)),
        "Run Configuration"
    );
    assert_eq!(
        command_label(&Command::RunPluginCommand {
            plugin_id: "example.plugin".to_owned(),
            command_id: "example.sayHello".to_owned(),
        }),
        "Run Plugin Command example.plugin:example.sayHello"
    );
    assert_eq!(command_label(&Command::TrustWorkspace), "Trust Workspace");
    assert_eq!(
        command_label(&Command::RevokeWorkspaceTrust),
        "Revoke Workspace Trust"
    );
}

#[test]
fn command_label_sanitizes_and_bounds_path_fragments() {
    let path = PathBuf::from(format!(
        "C:/repo/src/bad\n{}\u{202e}tail.rs",
        "very-long-component-".repeat(16)
    ));

    let labels = [
        command_label(&Command::OpenFile(path.clone())),
        command_label(&Command::CopyFilePath(path.clone())),
        command_label(&Command::StageFileHunk {
            path,
            hunk_index: 7,
            hunk_fingerprint: Some(99),
        }),
    ];

    for label in labels {
        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));
        assert!(label.contains("..."));
        assert!(label.chars().count() <= 150);
    }
}

#[test]
fn command_label_sanitizes_and_bounds_plugin_command_identifiers() {
    let plugin_id = format!("plugin\n{}\u{202e}\u{0007}", "id-".repeat(64));
    let command_id = format!("command\r\n{}\u{2066}\u{001b}", "id-".repeat(64));
    let command = Command::RunPluginCommand {
        plugin_id: plugin_id.clone(),
        command_id: command_id.clone(),
    };

    let label = command_label(&command);

    assert!(!label.chars().any(char::is_control), "{label:?}");
    assert!(!label.chars().any(is_bidi_format_control), "{label:?}");
    assert!(label.contains("..."));
    assert!(
        label.chars().count() <= "Run Plugin Command ".chars().count() + 96 + ":".len() + 96,
        "{label:?}"
    );
    assert_eq!(
        command,
        Command::RunPluginCommand {
            plugin_id,
            command_id,
        }
    );
}

fn is_bidi_format_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{061c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
    )
}
