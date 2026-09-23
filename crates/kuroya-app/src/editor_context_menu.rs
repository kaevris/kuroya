use crate::{
    editor_context_menu::{
        buffer_edits::render_buffer_edit_context_menu, lsp_actions::render_lsp_context_menu,
    },
    editor_input::EditorContextAction,
};
use eframe::egui;
use kuroya_core::GitChangeStage;

mod buffer_edits;
mod lsp_actions;

#[derive(Clone, Copy)]
struct ContextMenuAction {
    enabled: bool,
    label: &'static str,
    action: EditorContextAction,
}

impl ContextMenuAction {
    const fn new(enabled: bool, label: &'static str, action: EditorContextAction) -> Self {
        Self {
            enabled,
            label,
            action,
        }
    }
}

pub(crate) fn render_editor_context_menu(
    ui: &mut egui::Ui,
    pending_action: &mut Option<EditorContextAction>,
    merge_conflict_action_line: Option<usize>,
    worktree_hunk_actions: bool,
    staged_hunk_actions: bool,
    source_control_unstaged_actions: bool,
    source_control_staged_actions: bool,
    source_control_discard_actions: bool,
    source_control_path_actions: bool,
    compare_saved_actions: bool,
    compare_file_actions: bool,
    compare_with_selected_actions: bool,
    diff_base_file_actions: bool,
    diff_source_file_actions: bool,
    diff_patch_actions: bool,
    diff_refresh_actions: bool,
    diff_swap_actions: bool,
    diff_stage: Option<GitChangeStage>,
) {
    let git_change_navigation_actions = source_control_path_actions && !diff_patch_actions;
    let merge_conflict_actions = merge_conflict_action_line.is_some();
    render_context_actions(ui, pending_action, &buffer_history_context_actions());
    ui.separator();
    if ui.button("Copy").clicked() {
        *pending_action = Some(EditorContextAction::Copy);
        ui.close();
    }
    if ui.button("Cut").clicked() {
        *pending_action = Some(EditorContextAction::Cut);
        ui.close();
    }
    if ui.button("Select All").clicked() {
        *pending_action = Some(EditorContextAction::SelectAll);
        ui.close();
    }
    if ui.button("Select Lines").clicked() {
        *pending_action = Some(EditorContextAction::SelectLines);
        ui.close();
    }
    if ui.button("Select Rectangular Block").clicked() {
        *pending_action = Some(EditorContextAction::SelectRectangularBlock);
        ui.close();
    }
    if ui.button("Expand Selection").clicked() {
        *pending_action = Some(EditorContextAction::ExpandSelection);
        ui.close();
    }
    ui.separator();
    if ui.button("Find Selection").clicked() {
        *pending_action = Some(EditorContextAction::FindSelection);
        ui.close();
    }
    render_lsp_context_menu(ui, pending_action);
    if merge_conflict_actions {
        ui.separator();
        if ui.button("Accept Current Conflict").clicked() {
            *pending_action = merge_conflict_context_action_current(merge_conflict_action_line);
            ui.close();
        }
        if ui.button("Accept Incoming Conflict").clicked() {
            *pending_action = merge_conflict_context_action_incoming(merge_conflict_action_line);
            ui.close();
        }
        if ui.button("Accept Both Conflicts").clicked() {
            *pending_action = merge_conflict_context_action_both(merge_conflict_action_line);
            ui.close();
        }
    }
    if let Some(stage) = diff_stage {
        ui.separator();
        render_diff_patch_context_menu(
            ui,
            pending_action,
            diff_patch_actions,
            diff_refresh_actions,
            diff_swap_actions,
        );
        render_source_file_context_menu(
            ui,
            pending_action,
            diff_source_file_actions,
            source_control_path_actions,
            source_control_discard_actions,
            git_change_navigation_actions,
            compare_saved_actions,
            compare_file_actions,
            compare_with_selected_actions,
            diff_base_file_actions,
            diff_base_file_actions && diff_patch_actions,
        );
        ui.separator();
        render_diff_hunk_context_menu(ui, pending_action, stage);
    } else if diff_patch_actions {
        ui.separator();
        render_diff_patch_context_menu(
            ui,
            pending_action,
            diff_patch_actions,
            diff_refresh_actions,
            diff_swap_actions,
        );
        render_source_file_context_menu(
            ui,
            pending_action,
            diff_source_file_actions,
            source_control_path_actions,
            source_control_discard_actions,
            git_change_navigation_actions,
            compare_saved_actions,
            compare_file_actions,
            compare_with_selected_actions,
            diff_base_file_actions,
            diff_base_file_actions && diff_patch_actions,
        );
    } else if worktree_hunk_actions {
        ui.separator();
        render_source_file_context_menu(
            ui,
            pending_action,
            false,
            source_control_path_actions,
            source_control_discard_actions,
            git_change_navigation_actions,
            compare_saved_actions,
            compare_file_actions,
            compare_with_selected_actions,
            false,
            false,
        );
        ui.separator();
        render_worktree_hunk_context_menu(ui, pending_action, staged_hunk_actions);
    } else if staged_hunk_actions {
        ui.separator();
        render_source_file_context_menu(
            ui,
            pending_action,
            false,
            source_control_path_actions,
            source_control_discard_actions,
            git_change_navigation_actions,
            compare_saved_actions,
            compare_file_actions,
            compare_with_selected_actions,
            false,
            false,
        );
        ui.separator();
        render_staged_hunk_context_menu(ui, pending_action);
    } else if source_control_path_actions || diff_source_file_actions {
        ui.separator();
        render_source_file_context_menu(
            ui,
            pending_action,
            diff_source_file_actions,
            source_control_path_actions,
            source_control_discard_actions,
            git_change_navigation_actions,
            compare_saved_actions,
            compare_file_actions,
            compare_with_selected_actions,
            diff_base_file_actions,
            false,
        );
    }
    if source_control_unstaged_actions
        || source_control_staged_actions
        || source_control_discard_actions
    {
        ui.separator();
        render_source_control_file_context_menu(
            ui,
            pending_action,
            source_control_unstaged_actions,
            source_control_staged_actions,
            source_control_discard_actions,
        );
    }
    ui.separator();
    render_context_actions(ui, pending_action, &buffer_path_context_actions());
    ui.separator();
    render_buffer_edit_context_menu(ui, pending_action);
}

fn merge_conflict_context_action_current(line: Option<usize>) -> Option<EditorContextAction> {
    line.map(EditorContextAction::AcceptCurrentConflictAtLine)
}

fn merge_conflict_context_action_incoming(line: Option<usize>) -> Option<EditorContextAction> {
    line.map(EditorContextAction::AcceptIncomingConflictAtLine)
}

fn merge_conflict_context_action_both(line: Option<usize>) -> Option<EditorContextAction> {
    line.map(EditorContextAction::AcceptBothConflictsAtLine)
}

fn render_diff_patch_context_menu(
    ui: &mut egui::Ui,
    pending_action: &mut Option<EditorContextAction>,
    diff_patch_actions: bool,
    diff_refresh_actions: bool,
    diff_swap_actions: bool,
) {
    render_context_action_button(
        ui,
        pending_action,
        diff_refresh_actions,
        "Refresh Diff",
        EditorContextAction::RefreshDiff,
    );
    render_context_action_button(
        ui,
        pending_action,
        diff_swap_actions,
        "Swap Compare Sides",
        EditorContextAction::SwapDiffSides,
    );
    render_context_action_button(
        ui,
        pending_action,
        diff_patch_actions,
        "Copy Patch",
        EditorContextAction::CopyDiffPatch,
    );
    render_context_action_button(
        ui,
        pending_action,
        diff_patch_actions,
        "Copy Hunk Patch",
        EditorContextAction::CopyDiffHunkPatch,
    );
    render_context_action_button(
        ui,
        pending_action,
        diff_patch_actions,
        "Previous Diff Hunk",
        EditorContextAction::PreviousDiffHunk,
    );
    render_context_action_button(
        ui,
        pending_action,
        diff_patch_actions,
        "Next Diff Hunk",
        EditorContextAction::NextDiffHunk,
    );
    render_context_action_button(
        ui,
        pending_action,
        diff_patch_actions,
        "Open Accessible Diff Viewer",
        EditorContextAction::OpenAccessibleDiffViewer,
    );
}

fn render_context_action_button(
    ui: &mut egui::Ui,
    pending_action: &mut Option<EditorContextAction>,
    enabled: bool,
    label: &str,
    action: EditorContextAction,
) {
    if enabled && ui.button(label).clicked() {
        *pending_action = Some(action);
        ui.close();
    }
}

fn render_context_actions(
    ui: &mut egui::Ui,
    pending_action: &mut Option<EditorContextAction>,
    actions: &[ContextMenuAction],
) {
    for action in actions {
        render_context_action_button(
            ui,
            pending_action,
            action.enabled,
            action.label,
            action.action,
        );
    }
}

fn render_source_file_context_menu(
    ui: &mut egui::Ui,
    pending_action: &mut Option<EditorContextAction>,
    diff_source_file_actions: bool,
    source_control_path_actions: bool,
    source_control_discard_actions: bool,
    git_change_navigation_actions: bool,
    compare_saved_actions: bool,
    compare_file_actions: bool,
    compare_with_selected_actions: bool,
    diff_base_file_actions: bool,
    diff_base_hunk_actions: bool,
) {
    let actions = source_file_context_actions(
        diff_base_file_actions,
        diff_base_hunk_actions,
        diff_source_file_actions,
        source_control_path_actions,
        source_control_discard_actions,
        git_change_navigation_actions,
        compare_saved_actions,
        compare_file_actions,
        compare_with_selected_actions,
    );
    render_context_actions(ui, pending_action, &actions);
}

fn source_file_context_actions(
    diff_base_file_actions: bool,
    diff_base_hunk_actions: bool,
    diff_source_file_actions: bool,
    source_control_path_actions: bool,
    source_control_discard_actions: bool,
    git_change_navigation_actions: bool,
    compare_saved_actions: bool,
    compare_file_actions: bool,
    compare_with_selected_actions: bool,
) -> [ContextMenuAction; 15] {
    [
        ContextMenuAction::new(
            diff_base_file_actions,
            "Open Diff Base File",
            EditorContextAction::OpenDiffBaseFile,
        ),
        ContextMenuAction::new(
            diff_base_hunk_actions,
            "Open Base at Current Hunk",
            EditorContextAction::OpenDiffBaseAtCurrentHunk,
        ),
        ContextMenuAction::new(
            diff_source_file_actions,
            "Open Diff Source File",
            EditorContextAction::OpenDiffSourceFile,
        ),
        ContextMenuAction::new(
            diff_source_file_actions,
            "Open Source at Current Hunk",
            EditorContextAction::OpenDiffSourceAtCurrentHunk,
        ),
        ContextMenuAction::new(
            diff_source_file_actions,
            "Open Blame",
            EditorContextAction::OpenDiffSourceBlame,
        ),
        ContextMenuAction::new(
            source_control_path_actions,
            "Compare with HEAD",
            EditorContextAction::OpenActiveFileHeadChanges,
        ),
        ContextMenuAction::new(
            source_control_path_actions,
            "Open File at HEAD",
            EditorContextAction::OpenActiveFileHeadRevision,
        ),
        ContextMenuAction::new(
            source_control_path_actions,
            "Open File at Index",
            EditorContextAction::OpenActiveFileIndexRevision,
        ),
        ContextMenuAction::new(
            compare_saved_actions,
            "Compare with Saved",
            EditorContextAction::CompareActiveFileWithSaved,
        ),
        ContextMenuAction::new(
            compare_file_actions,
            "Select for Compare",
            EditorContextAction::SelectActiveFileForCompare,
        ),
        ContextMenuAction::new(
            compare_with_selected_actions,
            "Compare with Selected",
            EditorContextAction::CompareActiveFileWithSelected,
        ),
        ContextMenuAction::new(
            git_change_navigation_actions,
            "Previous Git Change",
            EditorContextAction::PreviousGitChange,
        ),
        ContextMenuAction::new(
            git_change_navigation_actions,
            "Next Git Change",
            EditorContextAction::NextGitChange,
        ),
        ContextMenuAction::new(
            source_control_path_actions,
            "Reveal in Explorer",
            EditorContextAction::RevealActiveFileInExplorer,
        ),
        ContextMenuAction::new(
            source_control_discard_actions,
            "Reveal in Source Control",
            EditorContextAction::RevealActiveFileInSourceControl,
        ),
    ]
}

fn buffer_history_context_actions() -> [ContextMenuAction; 2] {
    [
        ContextMenuAction::new(true, "Undo", EditorContextAction::Undo),
        ContextMenuAction::new(true, "Redo", EditorContextAction::Redo),
    ]
}

fn buffer_path_context_actions() -> [ContextMenuAction; 2] {
    [
        ContextMenuAction::new(true, "Copy Path", EditorContextAction::CopyActivePath),
        ContextMenuAction::new(
            true,
            "Copy Relative Path",
            EditorContextAction::CopyActiveRelativePath,
        ),
    ]
}

fn render_worktree_hunk_context_menu(
    ui: &mut egui::Ui,
    pending_action: &mut Option<EditorContextAction>,
    staged_hunk_actions: bool,
) {
    if ui.button("Open Current Hunk Diff").clicked() {
        *pending_action = Some(EditorContextAction::OpenActiveFileHunkDiff);
        ui.close();
    }
    if ui.button("Copy Current Hunk Patch").clicked() {
        *pending_action = Some(EditorContextAction::CopyActiveFileHunkPatch);
        ui.close();
    }
    if ui.button("Stage Current Hunk").clicked() {
        *pending_action = Some(EditorContextAction::StageActiveFileHunk);
        ui.close();
    }
    if staged_hunk_actions && ui.button("Open Current Staged Hunk Diff").clicked() {
        *pending_action = Some(EditorContextAction::OpenActiveFileStagedHunkDiff);
        ui.close();
    }
    if staged_hunk_actions && ui.button("Copy Current Staged Hunk Patch").clicked() {
        *pending_action = Some(EditorContextAction::CopyActiveFileStagedHunkPatch);
        ui.close();
    }
    if staged_hunk_actions && ui.button("Unstage Current Hunk").clicked() {
        *pending_action = Some(EditorContextAction::UnstageActiveFileHunk);
        ui.close();
    }
    if ui.button("Discard Current Hunk").clicked() {
        *pending_action = Some(EditorContextAction::DiscardActiveFileHunk);
        ui.close();
    }
}

fn render_staged_hunk_context_menu(
    ui: &mut egui::Ui,
    pending_action: &mut Option<EditorContextAction>,
) {
    if ui.button("Open Current Staged Hunk Diff").clicked() {
        *pending_action = Some(EditorContextAction::OpenActiveFileStagedHunkDiff);
        ui.close();
    }
    if ui.button("Copy Current Staged Hunk Patch").clicked() {
        *pending_action = Some(EditorContextAction::CopyActiveFileStagedHunkPatch);
        ui.close();
    }
    if ui.button("Unstage Current Hunk").clicked() {
        *pending_action = Some(EditorContextAction::UnstageActiveFileHunk);
        ui.close();
    }
}

fn render_diff_hunk_context_menu(
    ui: &mut egui::Ui,
    pending_action: &mut Option<EditorContextAction>,
    stage: GitChangeStage,
) {
    match stage {
        GitChangeStage::Unstaged => {
            if ui.button("Stage Current Diff Hunk").clicked() {
                *pending_action = Some(EditorContextAction::StageActiveDiffHunk);
                ui.close();
            }
            if ui.button("Discard Current Diff Hunk").clicked() {
                *pending_action = Some(EditorContextAction::DiscardActiveDiffHunk);
                ui.close();
            }
        }
        GitChangeStage::Staged => {
            if ui.button("Unstage Current Diff Hunk").clicked() {
                *pending_action = Some(EditorContextAction::UnstageActiveDiffHunk);
                ui.close();
            }
        }
    }
}

fn render_source_control_file_context_menu(
    ui: &mut egui::Ui,
    pending_action: &mut Option<EditorContextAction>,
    unstaged_actions: bool,
    staged_actions: bool,
    discard_actions: bool,
) {
    if unstaged_actions && ui.button("Open Changes").clicked() {
        *pending_action = Some(EditorContextAction::OpenActiveFileChanges);
        ui.close();
    }
    if unstaged_actions && ui.button("Copy Patch").clicked() {
        *pending_action = Some(EditorContextAction::CopyActiveFilePatch);
        ui.close();
    }
    if staged_actions && ui.button("Open Staged Changes").clicked() {
        *pending_action = Some(EditorContextAction::OpenActiveFileStagedChanges);
        ui.close();
    }
    if staged_actions && ui.button("Copy Staged Patch").clicked() {
        *pending_action = Some(EditorContextAction::CopyActiveFileStagedPatch);
        ui.close();
    }
    if unstaged_actions && ui.button("Open Hunks").clicked() {
        *pending_action = Some(EditorContextAction::OpenActiveFileHunks);
        ui.close();
    }
    if staged_actions && ui.button("Open Staged Hunks").clicked() {
        *pending_action = Some(EditorContextAction::OpenActiveFileStagedHunks);
        ui.close();
    }
    if unstaged_actions && ui.button("Stage File Changes").clicked() {
        *pending_action = Some(EditorContextAction::StageActiveFileChanges);
        ui.close();
    }
    if staged_actions && ui.button("Unstage File Changes").clicked() {
        *pending_action = Some(EditorContextAction::UnstageActiveFileChanges);
        ui.close();
    }
    if discard_actions && ui.button("Discard File Changes").clicked() {
        *pending_action = Some(EditorContextAction::DiscardActiveFileChanges);
        ui.close();
    }
}

#[cfg(test)]
pub(crate) fn file_hunk_context_action_labels(
    worktree_enabled: bool,
    staged_enabled: bool,
) -> Vec<&'static str> {
    match (worktree_enabled, staged_enabled) {
        (true, true) => vec![
            "Open Current Hunk Diff",
            "Copy Current Hunk Patch",
            "Stage Current Hunk",
            "Open Current Staged Hunk Diff",
            "Copy Current Staged Hunk Patch",
            "Unstage Current Hunk",
            "Discard Current Hunk",
        ],
        (true, false) => vec![
            "Open Current Hunk Diff",
            "Copy Current Hunk Patch",
            "Stage Current Hunk",
            "Discard Current Hunk",
        ],
        (false, true) => vec![
            "Open Current Staged Hunk Diff",
            "Copy Current Staged Hunk Patch",
            "Unstage Current Hunk",
        ],
        (false, false) => Vec::new(),
    }
}

#[cfg(test)]
pub(crate) fn file_source_control_context_action_labels(
    unstaged_enabled: bool,
    staged_enabled: bool,
    discard_enabled: bool,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if unstaged_enabled {
        labels.push("Open Changes");
        labels.push("Copy Patch");
    }
    if staged_enabled {
        labels.push("Open Staged Changes");
        labels.push("Copy Staged Patch");
    }
    if unstaged_enabled {
        labels.push("Open Hunks");
    }
    if staged_enabled {
        labels.push("Open Staged Hunks");
    }
    if unstaged_enabled {
        labels.push("Stage File Changes");
    }
    if staged_enabled {
        labels.push("Unstage File Changes");
    }
    if discard_enabled {
        labels.push("Discard File Changes");
    }
    labels
}

#[cfg(test)]
fn enabled_context_action_labels(actions: &[ContextMenuAction]) -> Vec<&'static str> {
    actions
        .iter()
        .filter_map(|action| action.enabled.then_some(action.label))
        .collect()
}

#[cfg(test)]
pub(crate) fn file_source_context_action_labels(
    diff_base_file_actions: bool,
    diff_base_hunk_actions: bool,
    diff_source_file_actions: bool,
    source_control_path_actions: bool,
    source_control_discard_actions: bool,
    git_change_navigation_actions: bool,
    compare_saved_actions: bool,
    compare_file_actions: bool,
    compare_with_selected_actions: bool,
) -> Vec<&'static str> {
    enabled_context_action_labels(&source_file_context_actions(
        diff_base_file_actions,
        diff_base_hunk_actions,
        diff_source_file_actions,
        source_control_path_actions,
        source_control_discard_actions,
        git_change_navigation_actions,
        compare_saved_actions,
        compare_file_actions,
        compare_with_selected_actions,
    ))
}

#[cfg(test)]
pub(crate) fn buffer_history_context_action_labels() -> Vec<&'static str> {
    enabled_context_action_labels(&buffer_history_context_actions())
}

#[cfg(test)]
pub(crate) fn buffer_path_context_action_labels() -> Vec<&'static str> {
    enabled_context_action_labels(&buffer_path_context_actions())
}

#[cfg(test)]
pub(crate) fn diff_patch_context_action_labels(
    patch_enabled: bool,
    refresh_enabled: bool,
    swap_enabled: bool,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if refresh_enabled {
        labels.push("Refresh Diff");
    }
    if swap_enabled {
        labels.push("Swap Compare Sides");
    }
    if patch_enabled {
        labels.extend([
            "Copy Patch",
            "Copy Hunk Patch",
            "Previous Diff Hunk",
            "Next Diff Hunk",
            "Open Accessible Diff Viewer",
        ]);
    }
    labels
}

#[cfg(test)]
pub(crate) fn diff_hunk_context_action_labels(stage: Option<GitChangeStage>) -> Vec<&'static str> {
    match stage {
        Some(GitChangeStage::Unstaged) => {
            vec!["Stage Current Diff Hunk", "Discard Current Diff Hunk"]
        }
        Some(GitChangeStage::Staged) => vec!["Unstage Current Diff Hunk"],
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        buffer_history_context_actions, buffer_path_context_actions, enabled_context_action_labels,
        source_file_context_actions,
    };
    use crate::editor_input::EditorContextAction;

    #[test]
    fn source_file_context_actions_keep_labels_and_targets_together() {
        let actions =
            source_file_context_actions(true, true, true, true, true, true, true, true, true);

        assert_eq!(actions[0].label, "Open Diff Base File");
        assert!(actions[0].enabled);
        assert!(matches!(
            actions[0].action,
            EditorContextAction::OpenDiffBaseFile
        ));

        assert_eq!(actions[10].label, "Compare with Selected");
        assert!(actions[10].enabled);
        assert!(matches!(
            actions[10].action,
            EditorContextAction::CompareActiveFileWithSelected
        ));

        assert_eq!(actions[14].label, "Reveal in Source Control");
        assert!(actions[14].enabled);
        assert!(matches!(
            actions[14].action,
            EditorContextAction::RevealActiveFileInSourceControl
        ));

        assert!(
            actions
                .iter()
                .all(|action| action.label != "Copy Path" && action.label != "Copy Relative Path")
        );
    }

    #[test]
    fn source_file_context_actions_disable_unavailable_items() {
        let actions = source_file_context_actions(
            false, false, false, false, false, false, false, false, false,
        );

        assert!(actions.iter().all(|action| !action.enabled));
    }

    #[test]
    fn buffer_history_context_actions_render_for_normal_buffers() {
        let actions = buffer_history_context_actions();

        assert_eq!(
            enabled_context_action_labels(&actions),
            vec!["Undo", "Redo"]
        );
        assert!(matches!(actions[0].action, EditorContextAction::Undo));
        assert!(matches!(actions[1].action, EditorContextAction::Redo));
    }

    #[test]
    fn buffer_path_context_actions_render_without_git_state() {
        let actions = buffer_path_context_actions();

        assert_eq!(
            enabled_context_action_labels(&actions),
            vec!["Copy Path", "Copy Relative Path"]
        );
        assert!(matches!(
            actions[0].action,
            EditorContextAction::CopyActivePath
        ));
        assert!(matches!(
            actions[1].action,
            EditorContextAction::CopyActiveRelativePath
        ));
    }
}
