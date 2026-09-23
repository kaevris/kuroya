use crate::{BufferId, WorkspaceTaskKind};
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, path::PathBuf};

pub const COMMAND_BUS_MAX_PENDING: usize = 2048;
pub const COMMAND_METADATA_IDENTIFIER_MAX_CHARS: usize = 256;
pub const COMMAND_METADATA_IDENTIFIER_MAX_BYTES: usize = COMMAND_METADATA_IDENTIFIER_MAX_CHARS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    NewFile,
    OpenFile(PathBuf),
    OpenFileAt {
        path: PathBuf,
        line: usize,
        column: usize,
    },
    SelectActiveFileForCompare,
    CompareActiveFileWithSelected,
    CompareActiveFileWithSaved,
    SelectFileForCompare(PathBuf),
    CompareFileWithSelected(PathBuf),
    OpenWorkspace(PathBuf),
    OpenWorkspacePrompt,
    TrustWorkspace,
    RevokeWorkspaceTrust,
    CreateFileIn(PathBuf),
    CreateFolderIn(PathBuf),
    RenamePath(PathBuf),
    DeletePath(PathBuf),
    RefreshWorkspace,
    SaveActive,
    SaveAs,
    SaveAll,
    ReloadActiveFromDisk,
    OpenActiveFileLatestLocalHistory,
    OpenLocalHistoryBrowser,
    SaveWorkspaceSnapshot,
    RestoreLatestWorkspaceSnapshot,
    ToggleReadOnly,
    ToggleMinimap,
    ToggleStickyScroll,
    ToggleVimMode,
    ReloadSettings,
    CheckForUpdates,
    OpenSettingsFile,
    ToggleSettingsPanel,
    ToggleKeybindingsPanel,
    ToggleThemePicker,
    CycleTheme,
    CloseActive,
    ReopenClosedFile,
    NextTab,
    PreviousTab,
    NavigateBack,
    NavigateForward,
    ToggleCommandPalette,
    ToggleQuickOpen,
    ToggleBufferFind,
    ToggleGoToLine,
    SelectLines,
    SelectRectangularBlock,
    ExpandSelection,
    FindNext,
    FindPrevious,
    SelectNextOccurrence,
    SelectAllOccurrences,
    GoToMatchingBracket,
    ToggleLineComment,
    NextDiagnostic,
    PreviousDiagnostic,
    NextGitChange,
    PreviousGitChange,
    NextDiffHunk,
    PreviousDiffHunk,
    RefreshActiveDiff,
    SwapActiveDiffSides,
    ToggleSourceControl,
    CycleSourceControlPlacement,
    RevealActiveFileInExplorer,
    RevealFileInExplorer(PathBuf),
    RevealActiveFileInSourceControl,
    RevealFileInSourceControl(PathBuf),
    CopyActiveFilePath,
    CopyActiveFileRelativePath,
    CopyFilePath(PathBuf),
    CopyFileRelativePath(PathBuf),
    OpenActiveFileChanges,
    OpenActiveFileStagedChanges,
    OpenActiveFileHeadChanges,
    OpenActiveFileHeadRevision,
    OpenActiveFileIndexRevision,
    OpenFileChanges(PathBuf),
    OpenStagedFileChanges(PathBuf),
    OpenFileHeadChanges(PathBuf),
    OpenFileHeadRevision(PathBuf),
    OpenFileIndexRevision(PathBuf),
    OpenAllChanges,
    OpenAllUnstagedChanges,
    OpenAllStagedChanges,
    CopyAllChangesPatch,
    CopyUnstagedChangesPatch,
    CopyStagedChangesPatch,
    CopyActiveFilePatch,
    CopyActiveFileStagedPatch,
    CopyFilePatch(PathBuf),
    CopyStagedFilePatch(PathBuf),
    OpenActiveFileHunks,
    OpenActiveFileStagedHunks,
    OpenFileHunks(PathBuf),
    OpenStagedFileHunks(PathBuf),
    OpenActiveFileBlame,
    OpenFileBlame(PathBuf),
    StageActiveFileChanges,
    StageFileChange(PathBuf),
    StageAllChanges,
    UnstageActiveFileChanges,
    UnstageFileChange(PathBuf),
    UnstageAllChanges,
    DiscardActiveFileChanges,
    DiscardFileChanges(PathBuf),
    DiscardAllChanges,
    StageFileHunk {
        path: PathBuf,
        hunk_index: usize,
        #[serde(default)]
        hunk_fingerprint: Option<u64>,
    },
    StageActiveFileHunk,
    StageActiveDiffHunk,
    OpenActiveDiffBaseFile,
    OpenActiveDiffHunkBase,
    OpenActiveDiffSourceFile,
    OpenActiveDiffHunkSource,
    OpenActiveFileHunkDiff,
    OpenActiveFileStagedHunkDiff,
    OpenActiveAccessibleDiffViewer,
    CopyActiveFileHunkPatch,
    CopyActiveFileStagedHunkPatch,
    CopyActiveDiffPatch,
    CopyActiveDiffHunkPatch,
    UnstageFileHunk {
        path: PathBuf,
        hunk_index: usize,
        #[serde(default)]
        hunk_fingerprint: Option<u64>,
    },
    UnstageActiveFileHunk,
    UnstageActiveDiffHunk,
    DiscardFileHunk {
        path: PathBuf,
        hunk_index: usize,
        #[serde(default)]
        hunk_fingerprint: Option<u64>,
    },
    DiscardActiveFileHunk,
    DiscardActiveDiffHunk,
    CommitStagedChanges,
    AcceptCurrentConflict,
    AcceptIncomingConflict,
    AcceptBothConflicts,
    ToggleGitBranchSwitcher,
    ToggleGitHistory,
    ToggleGitStashes,
    OpenSourceControlInIntegratedTerminal,
    SaveGitStash,
    ApplyGitStash(usize),
    PopGitStash(usize),
    DropGitStash(usize),
    RequestDocumentHighlights,
    RequestHover,
    GoToDefinition,
    FindReferences,
    ShowCallHierarchy,
    ShowTypeHierarchy,
    RenameSymbol,
    ToggleSymbolsPanel,
    CycleSymbolsPanelPlacement,
    ToggleWorkspaceSymbols,
    ToggleWorkspaceTasks,
    RunWorkspaceTask(usize),
    RunWorkspaceTaskSnapshot {
        index: usize,
        fingerprint: u64,
    },
    CancelWorkspaceTaskSnapshot {
        index: usize,
        fingerprint: u64,
    },
    RunWorkspaceTaskKind(WorkspaceTaskKind),
    RunPluginCommand {
        plugin_id: String,
        command_id: String,
    },
    RequestCompletions,
    RequestSignatureHelp,
    RequestFoldingRanges,
    ToggleFold,
    ExpandAllFolds,
    FormatDocument,
    RequestCodeActions,
    ToggleProjectSearch,
    CycleProjectSearchPlacement,
    NextProjectSearchResult,
    PreviousProjectSearchResult,
    ToggleDiagnosticsPanel,
    CycleDiagnosticsPanelPlacement,
    ToggleDevtools,
    ToggleTerminal,
    ToggleTerminalSearch,
    NextTerminalSearchResult,
    PreviousTerminalSearchResult,
    NextTerminalSession,
    PreviousTerminalSession,
    SplitEditorRight,
    CloseEditorPane,
    ResetEditorPaneWeights,
    IndentLines,
    OutdentLines,
    DeleteLines,
    JoinLines,
    DuplicateLines,
    MoveLineUp,
    MoveLineDown,
    AddCursorAbove,
    AddCursorBelow,
    AddCursorsToLineEnds,
    FocusBuffer(BufferId),
    Undo,
    Redo,
}

impl Command {
    pub fn normalize_keymap_metadata(&mut self) -> bool {
        match self {
            Command::RunPluginCommand {
                plugin_id,
                command_id,
            } => {
                normalize_command_identifier_in_place(plugin_id)
                    | normalize_command_identifier_in_place(command_id)
            }
            _ => false,
        }
    }

    pub fn is_stable_keymap_command(&self) -> bool {
        match self {
            Command::OpenFile(_)
            | Command::OpenFileAt { .. }
            | Command::SelectFileForCompare(_)
            | Command::CompareFileWithSelected(_)
            | Command::OpenWorkspace(_)
            | Command::CreateFileIn(_)
            | Command::CreateFolderIn(_)
            | Command::RenamePath(_)
            | Command::DeletePath(_)
            | Command::RevealFileInExplorer(_)
            | Command::RevealFileInSourceControl(_)
            | Command::CopyFilePath(_)
            | Command::CopyFileRelativePath(_)
            | Command::OpenFileChanges(_)
            | Command::OpenStagedFileChanges(_)
            | Command::OpenFileHeadChanges(_)
            | Command::OpenFileHeadRevision(_)
            | Command::OpenFileIndexRevision(_)
            | Command::CopyFilePatch(_)
            | Command::CopyStagedFilePatch(_)
            | Command::OpenFileHunks(_)
            | Command::OpenStagedFileHunks(_)
            | Command::OpenFileBlame(_)
            | Command::StageFileChange(_)
            | Command::UnstageFileChange(_)
            | Command::DiscardFileChanges(_)
            | Command::StageFileHunk { .. }
            | Command::UnstageFileHunk { .. }
            | Command::DiscardFileHunk { .. }
            | Command::ApplyGitStash(_)
            | Command::PopGitStash(_)
            | Command::DropGitStash(_)
            | Command::RunWorkspaceTask(_)
            | Command::RunWorkspaceTaskSnapshot { .. }
            | Command::CancelWorkspaceTaskSnapshot { .. }
            | Command::FocusBuffer(_) => false,
            Command::RunPluginCommand {
                plugin_id,
                command_id,
            } => command_identifier_is_valid(plugin_id) && command_identifier_is_valid(command_id),
            _ => true,
        }
    }
}

fn normalize_command_identifier_in_place(value: &mut String) -> bool {
    let original_len = value.len();
    let leading_len = original_len - value.trim_start().len();
    let trailing_start = value.trim_end().len();

    if leading_len == 0 && trailing_start == original_len {
        return false;
    }

    if leading_len >= trailing_start {
        value.clear();
        return true;
    }

    if trailing_start < original_len {
        value.drain(trailing_start..);
    }
    if leading_len > 0 {
        value.drain(..leading_len);
    }

    true
}

fn command_identifier_is_valid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= COMMAND_METADATA_IDENTIFIER_MAX_BYTES
        && value.chars().count() <= COMMAND_METADATA_IDENTIFIER_MAX_CHARS
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':'))
}

#[derive(Debug)]
pub struct CommandBus {
    queue: VecDeque<Command>,
}

impl Default for CommandBus {
    fn default() -> Self {
        Self {
            queue: VecDeque::with_capacity(COMMAND_BUS_MAX_PENDING),
        }
    }
}

impl CommandBus {
    pub fn push(&mut self, command: Command) -> bool {
        if self.queue.len() >= COMMAND_BUS_MAX_PENDING {
            return false;
        }
        self.queue.push_back(command);
        true
    }

    pub fn pop(&mut self) -> Option<Command> {
        self.queue.pop_front()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn drain(&mut self) -> impl Iterator<Item = Command> + '_ {
        self.queue.drain(..)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        COMMAND_BUS_MAX_PENDING, COMMAND_METADATA_IDENTIFIER_MAX_CHARS, Command, CommandBus,
    };
    use crate::WorkspaceTaskKind;
    use std::path::PathBuf;

    #[test]
    fn command_identifier_normalization_trims_in_place_and_reports_changes() {
        let mut clean = String::with_capacity(64);
        clean.push_str("example.plugin:run");
        let clean_capacity = clean.capacity();

        assert!(!super::normalize_command_identifier_in_place(&mut clean));
        assert_eq!(clean, "example.plugin:run");
        assert_eq!(clean.capacity(), clean_capacity);

        let mut padded = String::with_capacity(64);
        padded.push_str("  example plugin:run  ");
        let padded_capacity = padded.capacity();

        assert!(super::normalize_command_identifier_in_place(&mut padded));
        assert_eq!(padded, "example plugin:run");
        assert_eq!(padded.capacity(), padded_capacity);

        let mut whitespace = String::with_capacity(16);
        whitespace.push_str(" \t\n ");
        let whitespace_capacity = whitespace.capacity();

        assert!(super::normalize_command_identifier_in_place(
            &mut whitespace
        ));
        assert!(whitespace.is_empty());
        assert_eq!(whitespace.capacity(), whitespace_capacity);
    }

    #[test]
    fn command_keymap_metadata_trims_and_validates_plugin_ids() {
        let mut command = Command::RunPluginCommand {
            plugin_id: " example.plugin ".to_owned(),
            command_id: " command:run ".to_owned(),
        };

        assert!(command.normalize_keymap_metadata());
        assert_eq!(
            command,
            Command::RunPluginCommand {
                plugin_id: "example.plugin".to_owned(),
                command_id: "command:run".to_owned(),
            }
        );
        assert!(command.is_stable_keymap_command());

        let mut invalid = Command::RunPluginCommand {
            plugin_id: "example plugin".to_owned(),
            command_id: "run".to_owned(),
        };
        assert!(!invalid.normalize_keymap_metadata());
        assert!(!invalid.is_stable_keymap_command());

        assert!(
            !Command::RunPluginCommand {
                plugin_id: "example\u{202e}.plugin".to_owned(),
                command_id: "run".to_owned(),
            }
            .is_stable_keymap_command()
        );

        let too_long = "x".repeat(COMMAND_METADATA_IDENTIFIER_MAX_CHARS + 1);
        assert!(
            !Command::RunPluginCommand {
                plugin_id: "example.plugin".to_owned(),
                command_id: too_long,
            }
            .is_stable_keymap_command()
        );
    }

    #[test]
    fn command_keymap_metadata_rejects_stale_context_commands() {
        assert!(!Command::OpenFile(PathBuf::from("src/main.rs")).is_stable_keymap_command());
        assert!(
            !Command::RunWorkspaceTaskSnapshot {
                index: 0,
                fingerprint: 42,
            }
            .is_stable_keymap_command()
        );
        assert!(!Command::FocusBuffer(7).is_stable_keymap_command());
        assert!(Command::RunWorkspaceTaskKind(WorkspaceTaskKind::Build).is_stable_keymap_command());
        assert!(Command::ToggleQuickOpen.is_stable_keymap_command());
    }

    #[test]
    fn command_bus_preallocates_pending_queue_capacity() {
        let bus = CommandBus::default();

        assert!(bus.queue.capacity() >= COMMAND_BUS_MAX_PENDING);
    }

    #[test]
    fn command_bus_preserves_fifo_order() {
        let mut bus = CommandBus::default();

        assert!(bus.push(Command::ToggleQuickOpen));
        assert!(bus.push(Command::ToggleTerminal));

        assert_eq!(bus.pop(), Some(Command::ToggleQuickOpen));
        assert_eq!(bus.pop(), Some(Command::ToggleTerminal));
        assert_eq!(bus.pop(), None);
    }

    #[test]
    fn command_bus_pending_queue_is_bounded() {
        let mut bus = CommandBus::default();

        for _ in 0..COMMAND_BUS_MAX_PENDING {
            assert!(bus.push(Command::ToggleQuickOpen));
        }

        assert!(!bus.push(Command::ToggleTerminal));
        assert_eq!(bus.len(), COMMAND_BUS_MAX_PENDING);
        assert!(
            bus.drain()
                .all(|command| command == Command::ToggleQuickOpen)
        );
        assert!(bus.is_empty());
    }
}
