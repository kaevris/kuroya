use crate::{
    KuroyaApp,
    path_display::{display_error_label_cow, display_path_label_cow, sanitized_display_label_cow},
    ui_event_handler::{
        background::{
            handle_cached_index_event, handle_diagnostics_computed_event, handle_git_scanned_event,
            handle_indexed_event, handle_search_finished_event, handle_search_progress_event,
            handle_workspace_plugins_failed_event, handle_workspace_plugins_loaded_event,
        },
        session::{handle_session_save_failed_event, handle_session_saved_event},
    },
    ui_events::UiEvent,
    workspace_state::workspace_event_matches,
    workspace_trust::workspace_path_stays_within_root_lexically,
};
use eframe::egui::Context;

mod background;
mod session;

const UI_EVENT_DRAIN_BUDGET: usize = 512;
const EXPLORER_FAILURE_ACTION_LABEL_MAX_CHARS: usize = 64;
const PLUGIN_OPEN_FILE_PLUGIN_ID_MAX_CHARS: usize = 48;

impl KuroyaApp {
    #[cfg(test)]
    pub(crate) fn handle_events(&mut self) -> usize {
        self.drain_ui_events(None)
    }

    #[cfg(test)]
    pub(crate) fn handle_single_event(&mut self) -> usize {
        self.drain_ui_events_with_budget(None, 1)
    }

    pub(crate) fn handle_events_with_context(&mut self, ctx: &Context) -> usize {
        self.drain_ui_events(Some(ctx))
    }

    fn drain_ui_events(&mut self, ctx: Option<&Context>) -> usize {
        self.drain_ui_events_with_budget(ctx, UI_EVENT_DRAIN_BUDGET)
    }

    fn drain_ui_events_with_budget(&mut self, ctx: Option<&Context>, budget: usize) -> usize {
        let mut handled = 0usize;
        while handled < budget {
            let Ok(event) = self.rx.try_recv() else {
                break;
            };
            handled = handled.saturating_add(1);
            let event = match self.handle_lsp_event(event) {
                Some(event) => event,
                None => continue,
            };
            self.record_async_task_ui_event(&event);
            match event {
                UiEvent::CachedIndex {
                    request_id,
                    root,
                    index,
                } => {
                    handle_cached_index_event(self, request_id, root, index);
                }
                UiEvent::Indexed {
                    request_id,
                    root,
                    index,
                } => {
                    let root_is_current = workspace_event_matches(&self.workspace.root, &root);
                    let spawn_queued_index =
                        root_is_current && self.finish_workspace_index_request(request_id);
                    if !handle_indexed_event(self, request_id, root, index) {
                        if spawn_queued_index {
                            self.spawn_index();
                        }
                        continue;
                    }
                    if spawn_queued_index {
                        self.spawn_index();
                    } else if self.project_search_should_refresh_after_index() {
                        self.spawn_project_search();
                    }
                }
                event @ (UiEvent::FileLoaded { .. }
                | UiEvent::ImageFileLoaded { .. }
                | UiEvent::FileLoadFailed { .. }
                | UiEvent::FileReloaded { .. }
                | UiEvent::ImageFileReloaded { .. }
                | UiEvent::FileReloadFailed { .. }
                | UiEvent::FileSaved { .. }
                | UiEvent::FileSaveFailed { .. }) => {
                    self.handle_file_event(event);
                }
                UiEvent::LocalHistoryLoaded {
                    root,
                    generation,
                    path,
                    snapshot_path,
                    sequence,
                    text,
                } => {
                    self.apply_local_history_loaded(
                        root,
                        generation,
                        path,
                        snapshot_path,
                        sequence,
                        text,
                    );
                }
                UiEvent::LocalHistoryFailed {
                    root,
                    generation,
                    path,
                    error,
                } => {
                    self.apply_local_history_failed(root, generation, path, error);
                }
                UiEvent::LocalHistoryBrowserLoaded {
                    root,
                    generation,
                    path,
                    snapshots,
                } => {
                    self.apply_local_history_browser_loaded(root, generation, path, snapshots);
                }
                UiEvent::LocalHistoryBrowserSnapshotLoaded {
                    root,
                    generation,
                    path,
                    snapshot_path,
                    sequence,
                    modified,
                    result,
                } => {
                    self.apply_local_history_browser_snapshot_loaded(
                        root,
                        generation,
                        path,
                        snapshot_path,
                        sequence,
                        modified,
                        result,
                    );
                }
                UiEvent::SessionSaved { root } => {
                    handle_session_saved_event(self, root);
                }
                UiEvent::SessionSaveFailed { root, error } => {
                    handle_session_save_failed_event(self, root, error);
                }
                UiEvent::OpenWorkspacePicked { request_id, path } => {
                    self.apply_open_workspace_picked(request_id, path);
                }
                UiEvent::OpenWorkspacePickerCanceled { request_id } => {
                    self.apply_open_workspace_picker_canceled(request_id);
                }
                UiEvent::OpenWorkspacePickerFailed { request_id, error } => {
                    self.apply_open_workspace_picker_failed(request_id, error);
                }
                UiEvent::SettingsFontPicked {
                    root,
                    generation,
                    target,
                    path,
                } => {
                    if self.workspace_event_is_current(&root, generation) {
                        self.apply_settings_font_picked(target, path);
                    }
                }
                UiEvent::SettingsFontPickerCanceled {
                    root,
                    generation,
                    target,
                } => {
                    if self.workspace_event_is_current(&root, generation) {
                        self.apply_settings_font_picker_canceled(target);
                    }
                }
                UiEvent::SettingsFontPickerFailed {
                    root,
                    generation,
                    target,
                    error,
                } => {
                    if self.workspace_event_is_current(&root, generation) {
                        self.apply_settings_font_picker_failed(target, error);
                    }
                }
                UiEvent::SettingsBackgroundImagePicked {
                    root,
                    generation,
                    path,
                } => {
                    if self.workspace_event_is_current(&root, generation) {
                        self.apply_settings_background_image_picked(path);
                    }
                }
                UiEvent::SettingsBackgroundImagePickerCanceled { root, generation } => {
                    if self.workspace_event_is_current(&root, generation) {
                        self.apply_settings_background_image_picker_canceled();
                    }
                }
                UiEvent::SettingsBackgroundImagePickerFailed {
                    root,
                    generation,
                    error,
                } => {
                    if self.workspace_event_is_current(&root, generation) {
                        self.apply_settings_background_image_picker_failed(error);
                    }
                }
                UiEvent::EditorBackgroundImageLoaded {
                    request_id,
                    path,
                    loaded,
                } => {
                    if self.apply_background_image_loaded(request_id, path, loaded) {
                        if let Some(ctx) = ctx {
                            ctx.request_repaint();
                        }
                    }
                }
                UiEvent::EditorBackgroundImageLoadFailed {
                    request_id,
                    path,
                    error,
                } => {
                    if self.apply_background_image_load_failed(request_id, &path, &error) {
                        if let Some(ctx) = ctx {
                            ctx.request_repaint();
                        }
                    }
                }
                UiEvent::ExplorerCreatePathPicked {
                    root,
                    generation,
                    kind,
                    path,
                } => {
                    if self.workspace_event_is_current(&root, generation) {
                        self.apply_explorer_create_path_picked(kind, path);
                    }
                }
                UiEvent::ExplorerCreatePathPickerCanceled {
                    root,
                    generation,
                    kind,
                } => {
                    if self.workspace_event_is_current(&root, generation) {
                        self.apply_explorer_create_path_picker_canceled(kind);
                    }
                }
                UiEvent::ExplorerCreatePathPickerFailed {
                    root,
                    generation,
                    kind,
                    error,
                } => {
                    if self.workspace_event_is_current(&root, generation) {
                        self.apply_explorer_create_path_picker_failed(kind, error);
                    }
                }
                UiEvent::ExplorerOperationFinished {
                    root,
                    generation,
                    operation,
                } => {
                    if self.workspace_event_is_current(&root, generation) {
                        self.apply_explorer_operation(operation);
                    }
                }
                UiEvent::ExplorerOperationFailed {
                    root,
                    generation,
                    action,
                    path,
                    error,
                } => {
                    if !self.workspace_event_is_current(&root, generation) {
                        continue;
                    }
                    self.set_status_with_toast(explorer_operation_failed_status(
                        action, &path, &error,
                    ));
                    self.spawn_index();
                    self.spawn_git_auto_refresh();
                }
                UiEvent::SearchFinished {
                    request_id,
                    index_generation,
                    workspace_root,
                    query,
                    case_sensitive,
                    whole_word,
                    include_globs,
                    exclude_globs,
                    max_file_bytes,
                    max_results,
                    result,
                } => {
                    if !handle_search_finished_event(
                        self,
                        request_id,
                        index_generation,
                        workspace_root,
                        query,
                        case_sensitive,
                        whole_word,
                        include_globs,
                        exclude_globs,
                        max_file_bytes,
                        max_results,
                        result,
                    ) {
                        continue;
                    }
                }
                UiEvent::SearchProgress {
                    request_id,
                    index_generation,
                    workspace_root,
                    query,
                    case_sensitive,
                    whole_word,
                    include_globs,
                    exclude_globs,
                    max_file_bytes,
                    max_results,
                    progress,
                } => {
                    if !handle_search_progress_event(
                        self,
                        request_id,
                        index_generation,
                        workspace_root,
                        query,
                        case_sensitive,
                        whole_word,
                        include_globs,
                        exclude_globs,
                        max_file_bytes,
                        max_results,
                        progress,
                    ) {
                        continue;
                    }
                }
                UiEvent::GitScanned {
                    request_id,
                    root,
                    scan_root,
                    root_cache_entry,
                    git,
                } => {
                    let scan_error = git.scan_error().map(str::to_owned);
                    let root_is_current = workspace_event_matches(&self.workspace.root, &root);
                    let spawn_queued_git_scan =
                        root_is_current && self.finish_git_scan_request(request_id);
                    if !handle_git_scanned_event(
                        self,
                        request_id,
                        root,
                        scan_root,
                        root_cache_entry,
                        git,
                    ) {
                        if spawn_queued_git_scan {
                            self.spawn_git_scan();
                        }
                        continue;
                    }
                    if let Some(error) = scan_error {
                        self.set_status_with_toast(git_scan_failed_status(&error));
                    }
                    if spawn_queued_git_scan {
                        self.spawn_git_scan();
                    }
                }
                UiEvent::GitStageFinished { root, paths } => {
                    self.apply_git_stage_finished(root, paths);
                }
                UiEvent::GitStageFailed { root, paths, error } => {
                    self.apply_git_stage_failed(root, paths, error);
                }
                UiEvent::GitUnstageFinished { root, paths } => {
                    self.apply_git_unstage_finished(root, paths);
                }
                UiEvent::GitUnstageFailed { root, paths, error } => {
                    self.apply_git_unstage_failed(root, paths, error);
                }
                UiEvent::GitDiscardFinished { root, paths } => {
                    self.apply_git_discard_finished(root, paths);
                }
                UiEvent::GitDiscardFailed { root, paths, error } => {
                    self.apply_git_discard_failed(root, paths, error);
                }
                UiEvent::GitCommitFinished {
                    request_id,
                    root,
                    short_oid,
                    message,
                    smart_commit,
                } => {
                    self.apply_git_commit_finished(
                        request_id,
                        root,
                        short_oid,
                        message,
                        smart_commit,
                    );
                }
                UiEvent::GitCommitFailed {
                    request_id,
                    root,
                    error,
                    smart_commit,
                } => {
                    self.apply_git_commit_failed(request_id, root, error, smart_commit);
                }
                UiEvent::GitBranchesLoaded {
                    request_id,
                    root,
                    operation_root,
                    branches,
                } => {
                    let event_is_current = workspace_event_matches(&self.workspace.root, &root)
                        && self.source_control_git_operation_root_matches(&operation_root);
                    let spawn_queued_branch_load =
                        event_is_current && self.finish_source_control_branch_request(request_id);
                    self.apply_git_branches_loaded(request_id, root, operation_root, branches);
                    if spawn_queued_branch_load && self.source_control_branch_picker_open {
                        self.spawn_git_branch_list();
                    }
                }
                UiEvent::GitBranchesFailed {
                    request_id,
                    root,
                    operation_root,
                    error,
                } => {
                    let event_is_current = workspace_event_matches(&self.workspace.root, &root)
                        && self.source_control_git_operation_root_matches(&operation_root);
                    let spawn_queued_branch_load =
                        event_is_current && self.finish_source_control_branch_request(request_id);
                    self.apply_git_branches_failed(request_id, root, operation_root, error);
                    if spawn_queued_branch_load && self.source_control_branch_picker_open {
                        self.spawn_git_branch_list();
                    }
                }
                UiEvent::GitBranchSwitchFinished {
                    request_id,
                    root,
                    operation_root,
                    branch,
                } => {
                    self.apply_git_branch_switch_finished(request_id, root, operation_root, branch);
                }
                UiEvent::GitBranchSwitchFailed {
                    request_id,
                    root,
                    operation_root,
                    branch,
                    error,
                } => {
                    self.apply_git_branch_switch_failed(
                        request_id,
                        root,
                        operation_root,
                        branch,
                        error,
                    );
                }
                UiEvent::GitBranchCreateFinished {
                    request_id,
                    root,
                    operation_root,
                    branch,
                } => {
                    self.apply_git_branch_create_finished(request_id, root, operation_root, branch);
                }
                UiEvent::GitBranchCreateFailed {
                    request_id,
                    root,
                    operation_root,
                    branch,
                    error,
                } => {
                    self.apply_git_branch_create_failed(
                        request_id,
                        root,
                        operation_root,
                        branch,
                        error,
                    );
                }
                UiEvent::GitBranchDeleteFinished {
                    root,
                    operation_root,
                    branch,
                } => {
                    self.apply_git_branch_delete_finished(root, operation_root, branch);
                }
                UiEvent::GitBranchDeleteFailed {
                    root,
                    operation_root,
                    branch,
                    error,
                } => {
                    self.apply_git_branch_delete_failed(root, operation_root, branch, error);
                }
                UiEvent::GitBranchRenameFinished {
                    root,
                    operation_root,
                    old_branch,
                    new_branch,
                } => {
                    self.apply_git_branch_rename_finished(
                        root,
                        operation_root,
                        old_branch,
                        new_branch,
                    );
                }
                UiEvent::GitBranchRenameFailed {
                    root,
                    operation_root,
                    old_branch,
                    new_branch,
                    error,
                } => {
                    self.apply_git_branch_rename_failed(
                        root,
                        operation_root,
                        old_branch,
                        new_branch,
                        error,
                    );
                }
                UiEvent::GitHistoryLoaded {
                    request_id,
                    limit,
                    root,
                    operation_root,
                    commits,
                } => {
                    let event_is_current = workspace_event_matches(&self.workspace.root, &root)
                        && self.source_control_git_operation_root_matches(&operation_root);
                    let finished_history_load = event_is_current
                        && self.source_control_history_in_flight_request_id == Some(request_id);
                    let spawn_queued_history_load =
                        event_is_current && self.finish_source_control_history_request(request_id);
                    self.apply_git_history_loaded(request_id, limit, root, operation_root, commits);
                    if spawn_queued_history_load && self.source_control_history_open {
                        let limit = self.source_control_history_requested_limit;
                        self.spawn_git_history_load(limit);
                    } else if finished_history_load && !self.source_control_history_open {
                        self.source_control_history_loading = false;
                    }
                }
                UiEvent::GitHistoryFailed {
                    request_id,
                    limit,
                    root,
                    operation_root,
                    error,
                } => {
                    let event_is_current = workspace_event_matches(&self.workspace.root, &root)
                        && self.source_control_git_operation_root_matches(&operation_root);
                    let finished_history_load = event_is_current
                        && self.source_control_history_in_flight_request_id == Some(request_id);
                    let spawn_queued_history_load =
                        event_is_current && self.finish_source_control_history_request(request_id);
                    self.apply_git_history_failed(request_id, limit, root, operation_root, error);
                    if spawn_queued_history_load && self.source_control_history_open {
                        let limit = self.source_control_history_requested_limit;
                        self.spawn_git_history_load(limit);
                    } else if finished_history_load && !self.source_control_history_open {
                        self.source_control_history_loading = false;
                    }
                }
                UiEvent::GitBlameLoaded {
                    request_id,
                    root,
                    operation_root,
                    path,
                    lines,
                    text,
                } => {
                    let event_is_current = workspace_event_matches(&self.workspace.root, &root)
                        && self.source_control_git_operation_root_matches(&operation_root);
                    let spawn_queued_blame_load = event_is_current
                        && self.finish_source_control_blame_request(&path, request_id);
                    self.apply_git_blame_loaded(
                        request_id,
                        root,
                        operation_root,
                        path.clone(),
                        lines,
                        text,
                    );
                    if spawn_queued_blame_load {
                        self.request_file_blame(path, false);
                    }
                }
                UiEvent::GitBlameFailed {
                    request_id,
                    root,
                    operation_root,
                    path,
                    error,
                } => {
                    let event_is_current = workspace_event_matches(&self.workspace.root, &root)
                        && self.source_control_git_operation_root_matches(&operation_root);
                    let spawn_queued_blame_load = event_is_current
                        && self.finish_source_control_blame_request(&path, request_id);
                    self.apply_git_blame_failed(
                        request_id,
                        root,
                        operation_root,
                        path.clone(),
                        error,
                    );
                    if spawn_queued_blame_load {
                        self.request_file_blame(path, false);
                    }
                }
                UiEvent::GitStashesLoaded {
                    request_id,
                    root,
                    operation_root,
                    stashes,
                } => {
                    let event_is_current = workspace_event_matches(&self.workspace.root, &root)
                        && self.source_control_git_operation_root_matches(&operation_root);
                    let spawn_queued_stashes_load =
                        event_is_current && self.finish_source_control_stashes_request(request_id);
                    self.apply_git_stashes_loaded(request_id, root, operation_root, stashes);
                    if spawn_queued_stashes_load && self.source_control_stashes_open {
                        self.spawn_git_stashes_load();
                    }
                }
                UiEvent::GitStashesFailed {
                    request_id,
                    root,
                    operation_root,
                    error,
                } => {
                    let event_is_current = workspace_event_matches(&self.workspace.root, &root)
                        && self.source_control_git_operation_root_matches(&operation_root);
                    let spawn_queued_stashes_load =
                        event_is_current && self.finish_source_control_stashes_request(request_id);
                    self.apply_git_stashes_failed(request_id, root, operation_root, error);
                    if spawn_queued_stashes_load && self.source_control_stashes_open {
                        self.spawn_git_stashes_load();
                    }
                }
                UiEvent::GitStashSaved {
                    root,
                    operation_root,
                    short_oid,
                } => {
                    self.apply_git_stash_saved(root, operation_root, short_oid);
                }
                UiEvent::GitStashSaveFailed {
                    root,
                    operation_root,
                    error,
                } => {
                    self.apply_git_stash_save_failed(root, operation_root, error);
                }
                UiEvent::GitStashApplied {
                    root,
                    operation_root,
                    index,
                } => {
                    self.apply_git_stash_applied(root, operation_root, index);
                }
                UiEvent::GitStashApplyFailed {
                    root,
                    operation_root,
                    index,
                    error,
                } => {
                    self.apply_git_stash_apply_failed(root, operation_root, index, error);
                }
                UiEvent::GitStashPopped {
                    root,
                    operation_root,
                    index,
                } => {
                    self.apply_git_stash_popped(root, operation_root, index);
                }
                UiEvent::GitStashPopFailed {
                    root,
                    operation_root,
                    index,
                    error,
                } => {
                    self.apply_git_stash_pop_failed(root, operation_root, index, error);
                }
                UiEvent::GitStashDropped {
                    root,
                    operation_root,
                    index,
                } => {
                    self.apply_git_stash_dropped(root, operation_root, index);
                }
                UiEvent::GitStashDropFailed {
                    root,
                    operation_root,
                    index,
                    error,
                } => {
                    self.apply_git_stash_drop_failed(root, operation_root, index, error);
                }
                UiEvent::GitHunksLoaded {
                    request_id,
                    root,
                    operation_root,
                    path,
                    stage,
                    hunks,
                } => {
                    let event_is_current = workspace_event_matches(&self.workspace.root, &root)
                        && self.source_control_git_operation_root_matches(&operation_root);
                    let spawn_queued_hunk_load =
                        event_is_current && self.finish_source_control_hunks_request(request_id);
                    self.apply_git_hunks_loaded(
                        request_id,
                        root,
                        operation_root,
                        path,
                        stage,
                        hunks,
                    );
                    if spawn_queued_hunk_load
                        && self.source_control_hunks_open
                        && let Some(path) = self.source_control_hunk_path.clone()
                    {
                        self.spawn_git_hunk_list(path);
                    }
                }
                UiEvent::GitHunksFailed {
                    request_id,
                    root,
                    operation_root,
                    path,
                    stage,
                    error,
                } => {
                    let event_is_current = workspace_event_matches(&self.workspace.root, &root)
                        && self.source_control_git_operation_root_matches(&operation_root);
                    let spawn_queued_hunk_load =
                        event_is_current && self.finish_source_control_hunks_request(request_id);
                    self.apply_git_hunks_failed(
                        request_id,
                        root,
                        operation_root,
                        path,
                        stage,
                        error,
                    );
                    if spawn_queued_hunk_load
                        && self.source_control_hunks_open
                        && let Some(path) = self.source_control_hunk_path.clone()
                    {
                        self.spawn_git_hunk_list(path);
                    }
                }
                UiEvent::GitHunkStaged {
                    root,
                    path,
                    hunk_index,
                } => {
                    self.apply_git_hunk_staged(root, path, hunk_index);
                }
                UiEvent::GitHunkStageFailed {
                    root,
                    path,
                    hunk_index,
                    error,
                } => {
                    self.apply_git_hunk_stage_failed(root, path, hunk_index, error);
                }
                UiEvent::GitHunkUnstaged {
                    root,
                    path,
                    hunk_index,
                } => {
                    self.apply_git_hunk_unstaged(root, path, hunk_index);
                }
                UiEvent::GitHunkUnstageFailed {
                    root,
                    path,
                    hunk_index,
                    error,
                } => {
                    self.apply_git_hunk_unstage_failed(root, path, hunk_index, error);
                }
                UiEvent::GitHunkDiscarded {
                    root,
                    path,
                    hunk_index,
                    text,
                    expected_buffer,
                } => {
                    self.apply_git_hunk_discarded(root, path, hunk_index, text, expected_buffer);
                }
                UiEvent::GitHunkDiscardFailed {
                    root,
                    path,
                    hunk_index,
                    error,
                } => {
                    self.apply_git_hunk_discard_failed(root, path, hunk_index, error);
                }
                UiEvent::GitPatchCopyFinished {
                    root,
                    operation_root,
                    generation,
                    request_id,
                    request,
                    result,
                } => {
                    self.apply_source_control_patch_copy_finished(
                        ctx,
                        root,
                        operation_root,
                        generation,
                        request_id,
                        request,
                        result,
                    );
                }
                UiEvent::SourceControlDiffOpenFinished {
                    root,
                    operation_root,
                    generation,
                    request_id,
                    request,
                    result,
                } => {
                    self.apply_source_control_diff_open_finished(
                        root,
                        operation_root,
                        generation,
                        request_id,
                        request,
                        result,
                    );
                }
                UiEvent::VirtualDiffOpenFinished {
                    root,
                    generation,
                    request_id,
                    request,
                    result,
                } => {
                    self.apply_virtual_diff_open_finished(
                        root, generation, request_id, request, result,
                    );
                }
                UiEvent::VirtualRevisionOpenFinished {
                    root,
                    generation,
                    request_id,
                    request,
                    result,
                } => {
                    self.apply_virtual_revision_open_finished(
                        root, generation, request_id, request, result,
                    );
                }
                UiEvent::FallbackFoldingRangesLoaded {
                    id,
                    path,
                    version,
                    ranges,
                } => {
                    self.apply_fallback_folding_ranges_loaded(id, path, version, ranges);
                }
                UiEvent::EditorDiffLinesComputed {
                    root,
                    id,
                    path,
                    version,
                    ignore_trim_whitespace,
                    lines,
                } => {
                    self.apply_editor_diff_lines_computed(
                        root,
                        id,
                        path,
                        version,
                        ignore_trim_whitespace,
                        lines,
                    );
                }
                UiEvent::WorkspaceTasksLoaded {
                    request_id,
                    root,
                    tasks,
                } => {
                    let root_is_current = workspace_event_matches(&self.workspace.root, &root);
                    let spawn_queued_task_load =
                        root_is_current && self.finish_workspace_task_load_request(request_id);
                    self.apply_workspace_tasks_loaded(request_id, root, tasks);
                    if spawn_queued_task_load {
                        self.spawn_workspace_task_load();
                    }
                }
                UiEvent::WorkspaceTasksFailed {
                    request_id,
                    root,
                    error,
                } => {
                    let root_is_current = workspace_event_matches(&self.workspace.root, &root);
                    let spawn_queued_task_load =
                        root_is_current && self.finish_workspace_task_load_request(request_id);
                    self.apply_workspace_tasks_failed(request_id, root, error);
                    if spawn_queued_task_load {
                        self.spawn_workspace_task_load();
                    }
                }
                UiEvent::WorkspacePluginsLoaded {
                    request_id,
                    root,
                    plugins,
                    errors,
                    syntax_load,
                } => {
                    let root_is_current = workspace_event_matches(&self.workspace.root, &root);
                    let spawn_queued_plugin_discovery = root_is_current
                        && self.finish_workspace_plugin_discovery_request(request_id);
                    if !handle_workspace_plugins_loaded_event(
                        self,
                        request_id,
                        root,
                        plugins,
                        errors,
                        syntax_load,
                    ) {
                        if spawn_queued_plugin_discovery {
                            self.spawn_plugin_discovery();
                        }
                        continue;
                    }
                    if spawn_queued_plugin_discovery {
                        self.spawn_plugin_discovery();
                    }
                }
                UiEvent::WorkspacePluginsFailed {
                    request_id,
                    root,
                    error,
                } => {
                    let root_is_current = workspace_event_matches(&self.workspace.root, &root);
                    let spawn_queued_plugin_discovery = root_is_current
                        && self.finish_workspace_plugin_discovery_request(request_id);
                    if !handle_workspace_plugins_failed_event(self, request_id, root, error) {
                        if spawn_queued_plugin_discovery {
                            self.spawn_plugin_discovery();
                        }
                        continue;
                    }
                    if spawn_queued_plugin_discovery {
                        self.spawn_plugin_discovery();
                    }
                }
                UiEvent::PluginCommandFinished {
                    root,
                    generation,
                    plugin_id,
                    command_id,
                    result,
                } => {
                    if self.workspace_event_is_current(&root, generation) {
                        self.apply_plugin_command_finished(plugin_id, command_id, result);
                    }
                }
                UiEvent::PluginOpenFileRequested { plugin_id, path } => {
                    self.apply_plugin_open_file_requested(plugin_id, path);
                }
                UiEvent::PluginBufferTextApply {
                    plugin_id,
                    path,
                    text,
                } => {
                    self.apply_plugin_buffer_text_apply(plugin_id, path, &text);
                }
                UiEvent::DiagnosticsComputed {
                    request_id,
                    id,
                    path,
                    version,
                    diagnostics,
                } => {
                    handle_diagnostics_computed_event(
                        self,
                        request_id,
                        id,
                        path,
                        version,
                        diagnostics,
                    );
                    if self.finish_static_diagnostics_request(id, request_id) {
                        self.spawn_diagnostics_for(id);
                    }
                }
                UiEvent::QuickOpenRanked {
                    request_id,
                    key,
                    results,
                } => {
                    if self.apply_quick_open_ranked_results(request_id, &key, results)
                        && let Some(ctx) = ctx
                    {
                        ctx.request_repaint();
                    }
                }
                UiEvent::ExplorerDirectoryLoaded {
                    root,
                    generation,
                    request_token,
                    directory,
                    snapshot,
                } => {
                    if self.apply_explorer_directory_loaded_event(
                        &root,
                        generation,
                        request_token,
                        &directory,
                        snapshot,
                    ) && let Some(ctx) = ctx
                    {
                        ctx.request_repaint();
                    }
                }
                UiEvent::UpdateCheckFinished(outcome) => {
                    self.apply_update_check_finished(outcome);
                }
                UiEvent::UpdateCheckFailed { error } => {
                    self.apply_update_check_failed(error);
                }
                UiEvent::UpdateInstallerReady(update) => {
                    self.apply_update_installer_ready(update);
                }
                UiEvent::UpdateDownloadProgress {
                    latest_version,
                    asset_name,
                    bytes_downloaded,
                } => {
                    self.apply_update_download_progress(
                        latest_version,
                        asset_name,
                        bytes_downloaded,
                    );
                }
                UiEvent::UpdateDownloadFailed { available, error } => {
                    self.apply_update_download_failed(available, error);
                }
                UiEvent::StartupSessionLoaded {
                    root,
                    target,
                    session,
                    warning,
                } => {
                    self.apply_startup_session_loaded(root, target, session, warning);
                }
                UiEvent::LspInstallProgress {
                    language,
                    display_name,
                    bytes_downloaded,
                } => {
                    self.apply_lsp_install_progress(&language, &display_name, bytes_downloaded);
                }
                UiEvent::LspInstallFinished {
                    language,
                    display_name,
                    result,
                } => {
                    self.apply_lsp_install_finished(&language, &display_name, result);
                }
                UiEvent::Lsp(_) => continue,
            }
        }
        handled
    }
}

fn explorer_operation_failed_status(action: &str, path: &std::path::Path, error: &str) -> String {
    let action = sanitized_display_label_cow(
        action,
        EXPLORER_FAILURE_ACTION_LABEL_MAX_CHARS,
        "complete operation",
    );
    let path = display_path_label_cow(path);
    let error = display_error_label_cow(error);
    format!(
        "Could not {} {}: {}",
        action.as_ref(),
        path.as_ref(),
        error.as_ref()
    )
}

fn git_scan_failed_status(error: &str) -> String {
    format!("Git scan failed: {}", display_error_label_cow(error))
}

impl KuroyaApp {
    fn apply_plugin_open_file_requested(&mut self, plugin_id: String, path: std::path::PathBuf) {
        if self.workspace_placeholder {
            self.set_status_with_toast("No folder open");
            return;
        }
        if !plugin_open_file_path_is_openable(&self.workspace.root, &path) {
            self.set_status_with_toast(plugin_open_file_rejected_status(&plugin_id, &path));
            return;
        }
        self.spawn_open_file(path);
    }

    fn apply_plugin_buffer_text_apply(
        &mut self,
        _plugin_id: String,
        path: std::path::PathBuf,
        text: &str,
    ) {
        let Some(id) = self
            .buffers
            .iter()
            .find(|buffer| buffer.path() == Some(&path))
            .map(kuroya_core::TextBuffer::id)
        else {
            self.set_status_with_toast(plugin_buffer_text_skipped_status(&path));
            return;
        };
        if let Some(buffer) = self.buffer_mut(id) {
            buffer.replace_text_from_ui(text);
        }
        self.mark_buffer_changed(id);
        self.set_status_with_toast(plugin_buffer_text_updated_status(&path));
    }
}

fn plugin_open_file_path_is_openable(root: &std::path::Path, path: &std::path::Path) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
        && workspace_path_stays_within_root_lexically(root, path)
}

fn plugin_open_file_rejected_status(plugin_id: &str, path: &std::path::Path) -> String {
    let name = display_path_label_cow(path);
    match plugin_open_file_plugin_label(plugin_id) {
        Some(plugin) => format!("Plugin {plugin} could not open {}", name.as_ref()),
        None => format!("Plugin could not open {}", name.as_ref()),
    }
}

fn plugin_open_file_plugin_label(plugin_id: &str) -> Option<String> {
    let trimmed = plugin_id.trim();
    if trimmed.is_empty() || trimmed.chars().count() > PLUGIN_OPEN_FILE_PLUGIN_ID_MAX_CHARS {
        return None;
    }
    Some(
        sanitized_display_label_cow(trimmed, PLUGIN_OPEN_FILE_PLUGIN_ID_MAX_CHARS, "plugin")
            .into_owned(),
    )
}

fn plugin_buffer_text_updated_status(path: &std::path::Path) -> String {
    let name = display_path_label_cow(path);
    format!("Plugin updated {}", name.as_ref())
}

fn plugin_buffer_text_skipped_status(path: &std::path::Path) -> String {
    let name = display_path_label_cow(path);
    format!("Plugin changes skipped: {} is not open", name.as_ref())
}

#[cfg(test)]
mod tests;
