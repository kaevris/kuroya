use crate::{KuroyaApp, editor_input::EditorContextAction};
use kuroya_core::{BufferId, Command, MergeConflictResolution, TextBuffer};

impl KuroyaApp {
    pub(crate) fn run_editor_buffer_context_action(
        &mut self,
        buffer_id: BufferId,
        action: EditorContextAction,
    ) -> bool {
        match action {
            EditorContextAction::SelectAll => {
                if let Some(buffer) = self.buffer_mut(buffer_id) {
                    buffer.select_all();
                    self.status = "Selected all".to_owned();
                }
            }
            EditorContextAction::SelectLines => {
                if let Some(buffer) = self.buffer_mut(buffer_id)
                    && buffer.select_lines()
                {
                    self.status = "Selected lines".to_owned();
                }
            }
            EditorContextAction::SelectRectangularBlock => {
                let multi_cursor_limit = self.settings.multi_cursor_limit.max(1);
                if let Some(buffer) = self.buffer_mut(buffer_id) {
                    if buffer.select_rectangular_block_with_limit(multi_cursor_limit) {
                        let count = buffer.selections().len();
                        self.status = format!("Selected rectangular block across {count} lines");
                    } else {
                        self.status = "No rectangular selection range".to_owned();
                    }
                }
            }
            EditorContextAction::ExpandSelection => {
                self.expand_selection_for_buffer(buffer_id);
            }
            EditorContextAction::Undo => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::Undo);
            }
            EditorContextAction::Redo => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::Redo);
            }
            EditorContextAction::DuplicateLines => {
                let changed = self
                    .buffer_mut(buffer_id)
                    .is_some_and(TextBuffer::duplicate_lines);
                if changed {
                    self.mark_buffer_changed(buffer_id);
                    self.status = "Duplicated lines".to_owned();
                }
            }
            EditorContextAction::MoveLineUp => {
                let changed = self
                    .buffer_mut(buffer_id)
                    .is_some_and(TextBuffer::move_lines_up);
                if changed {
                    self.mark_buffer_changed(buffer_id);
                    self.status = "Moved lines up".to_owned();
                }
            }
            EditorContextAction::MoveLineDown => {
                let changed = self
                    .buffer_mut(buffer_id)
                    .is_some_and(TextBuffer::move_lines_down);
                if changed {
                    self.mark_buffer_changed(buffer_id);
                    self.status = "Moved lines down".to_owned();
                }
            }
            EditorContextAction::ToggleLineComment => {
                self.toggle_line_comment_for_buffer(buffer_id);
            }
            EditorContextAction::DeleteLines => {
                let changed = self
                    .buffer_mut(buffer_id)
                    .is_some_and(TextBuffer::delete_lines);
                if changed {
                    self.mark_buffer_changed(buffer_id);
                    self.status = "Deleted lines".to_owned();
                }
            }
            EditorContextAction::JoinLines => {
                let changed = self
                    .buffer_mut(buffer_id)
                    .is_some_and(TextBuffer::join_lines);
                if changed {
                    self.mark_buffer_changed(buffer_id);
                    self.status = "Joined lines".to_owned();
                }
            }
            EditorContextAction::AddCursorsToLineEnds => {
                let multi_cursor_limit = self.settings.multi_cursor_limit.max(1);
                let changed = self.buffer_mut(buffer_id).is_some_and(|buffer| {
                    buffer.add_cursors_to_line_ends_with_limit(multi_cursor_limit)
                });
                if changed {
                    self.status = "Added cursors to selected line ends".to_owned();
                }
            }
            EditorContextAction::AcceptCurrentConflictAtLine(line) => {
                self.resolve_merge_conflict_for_buffer_at_line(
                    buffer_id,
                    line,
                    MergeConflictResolution::Current,
                );
            }
            EditorContextAction::AcceptIncomingConflictAtLine(line) => {
                self.resolve_merge_conflict_for_buffer_at_line(
                    buffer_id,
                    line,
                    MergeConflictResolution::Incoming,
                );
            }
            EditorContextAction::AcceptBothConflictsAtLine(line) => {
                self.resolve_merge_conflict_for_buffer_at_line(
                    buffer_id,
                    line,
                    MergeConflictResolution::Both,
                );
            }
            EditorContextAction::OpenDiffBaseFile => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::OpenActiveDiffBaseFile);
            }
            EditorContextAction::OpenDiffBaseAtCurrentHunk => {
                self.set_active_buffer(buffer_id);
                self.open_active_diff_hunk_base();
            }
            EditorContextAction::OpenDiffSourceFile => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::OpenActiveDiffSourceFile);
            }
            EditorContextAction::OpenDiffSourceAtCurrentHunk => {
                self.set_active_buffer(buffer_id);
                self.open_active_diff_hunk_source();
            }
            EditorContextAction::OpenDiffSourceBlame => {
                if let Some(path) = self
                    .diff_buffer_sources
                    .get(&buffer_id)
                    .map(|source| source.path.clone())
                {
                    self.command_bus.push(Command::OpenFileBlame(path));
                } else {
                    self.status = "No source file for this diff".to_owned();
                }
            }
            EditorContextAction::OpenActiveFileHeadChanges => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::OpenActiveFileHeadChanges);
            }
            EditorContextAction::OpenActiveFileHeadRevision => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::OpenActiveFileHeadRevision);
            }
            EditorContextAction::OpenActiveFileIndexRevision => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::OpenActiveFileIndexRevision);
            }
            EditorContextAction::OpenActiveFileChanges => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::OpenActiveFileChanges);
            }
            EditorContextAction::OpenActiveFileStagedChanges => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::OpenActiveFileStagedChanges);
            }
            EditorContextAction::OpenAccessibleDiffViewer => {
                self.set_active_buffer(buffer_id);
                self.command_bus
                    .push(Command::OpenActiveAccessibleDiffViewer);
            }
            EditorContextAction::CopyActiveFilePatch => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::CopyActiveFilePatch);
            }
            EditorContextAction::CopyActiveFileStagedPatch => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::CopyActiveFileStagedPatch);
            }
            EditorContextAction::OpenActiveFileHunks => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::OpenActiveFileHunks);
            }
            EditorContextAction::OpenActiveFileStagedHunks => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::OpenActiveFileStagedHunks);
            }
            EditorContextAction::OpenActiveFileHunkDiff => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::OpenActiveFileHunkDiff);
            }
            EditorContextAction::OpenActiveFileStagedHunkDiff => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::OpenActiveFileStagedHunkDiff);
            }
            EditorContextAction::CopyActiveFileHunkPatch => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::CopyActiveFileHunkPatch);
            }
            EditorContextAction::CopyActiveFileStagedHunkPatch => {
                self.set_active_buffer(buffer_id);
                self.command_bus
                    .push(Command::CopyActiveFileStagedHunkPatch);
            }
            EditorContextAction::SelectActiveFileForCompare => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::SelectActiveFileForCompare);
            }
            EditorContextAction::CompareActiveFileWithSelected => {
                self.set_active_buffer(buffer_id);
                self.command_bus
                    .push(Command::CompareActiveFileWithSelected);
            }
            EditorContextAction::CompareActiveFileWithSaved => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::CompareActiveFileWithSaved);
            }
            EditorContextAction::RefreshDiff => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::RefreshActiveDiff);
            }
            EditorContextAction::SwapDiffSides => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::SwapActiveDiffSides);
            }
            EditorContextAction::PreviousDiffHunk => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::PreviousDiffHunk);
            }
            EditorContextAction::NextDiffHunk => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::NextDiffHunk);
            }
            EditorContextAction::PreviousGitChange => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::PreviousGitChange);
            }
            EditorContextAction::NextGitChange => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::NextGitChange);
            }
            EditorContextAction::RevealActiveFileInExplorer => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::RevealActiveFileInExplorer);
            }
            EditorContextAction::RevealActiveFileInSourceControl => {
                self.set_active_buffer(buffer_id);
                self.command_bus
                    .push(Command::RevealActiveFileInSourceControl);
            }
            EditorContextAction::StageActiveFileChanges => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::StageActiveFileChanges);
            }
            EditorContextAction::StageActiveFileHunk => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::StageActiveFileHunk);
            }
            EditorContextAction::StageActiveDiffHunk => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::StageActiveDiffHunk);
            }
            EditorContextAction::UnstageActiveFileChanges => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::UnstageActiveFileChanges);
            }
            EditorContextAction::UnstageActiveFileHunk => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::UnstageActiveFileHunk);
            }
            EditorContextAction::UnstageActiveDiffHunk => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::UnstageActiveDiffHunk);
            }
            EditorContextAction::DiscardActiveFileChanges => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::DiscardActiveFileChanges);
            }
            EditorContextAction::DiscardActiveFileHunk => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::DiscardActiveFileHunk);
            }
            EditorContextAction::DiscardActiveDiffHunk => {
                self.set_active_buffer(buffer_id);
                self.command_bus.push(Command::DiscardActiveDiffHunk);
            }
            EditorContextAction::IndentLines => {
                let tab = self.indent_options_for_buffer(buffer_id).unit;
                let changed = self
                    .buffer_mut(buffer_id)
                    .is_some_and(|buffer| buffer.indent_lines(&tab));
                if changed {
                    self.mark_buffer_changed(buffer_id);
                    self.status = "Indented lines".to_owned();
                }
            }
            EditorContextAction::OutdentLines => {
                let tab = self.indent_options_for_buffer(buffer_id).unit;
                let changed = self
                    .buffer_mut(buffer_id)
                    .is_some_and(|buffer| buffer.outdent_lines(&tab));
                if changed {
                    self.mark_buffer_changed(buffer_id);
                    self.status = "Outdented lines".to_owned();
                }
            }
            _ => return false,
        }

        true
    }
}
