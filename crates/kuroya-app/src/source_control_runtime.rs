use crate::{
    KuroyaApp,
    devtools_async_tasks::paths_detail,
    git_diff_state::DiffBufferSource,
    save_lifecycle::has_active_save_work,
    source_control_panel::{
        SOURCE_CONTROL_COMMIT_HISTORY_LIMIT, record_source_control_commit_history,
        source_control_auto_reveal_selection, source_control_filtered_entries,
        source_control_reveal_selection,
    },
    ui_events::UiEvent,
    workspace_event_guards::background_request_matches,
    workspace_state::workspace_event_matches,
    workspace_trust::workspace_path_contains_lexically,
};
use kuroya_core::{
    BufferId, GitChangeStage, GitSmartCommitChanges, TextBuffer,
    commit_changes_with_user_config_option as git_commit_changes, discard_paths, stage_paths,
    unstage_paths,
};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

mod status;

#[cfg(test)]
pub(crate) use status::source_control_branch_protection_pattern_matches;
#[cfg(test)]
use status::{
    SOURCE_CONTROL_COMMIT_HASH_DISPLAY_MAX_CHARS,
    SOURCE_CONTROL_PROTECTED_BRANCH_PATTERN_DISPLAY_MAX_CHARS, first_stale_source_control_path,
    git_commit_hash_display, git_commit_hash_display_cow, git_source_control_target,
    no_staged_changes_status, no_unstaged_changes_status,
    source_control_protected_branch_pattern_display,
    source_control_protected_branch_pattern_display_cow, source_control_stage_path_count,
};
pub(crate) use status::{
    SourceControlProtectedBranchCommitAction, git_commit_failure_status, git_commit_pending_status,
    git_commit_success_status, git_discard_failure_status, git_discard_pending_status,
    git_discard_success_status, git_progress_status, git_stage_failure_status,
    git_stage_pending_status, git_stage_success_status, git_unstage_failure_status,
    git_unstage_pending_status, git_unstage_success_status, source_control_commit_save_prompt_ids,
    source_control_commit_save_prompt_ids_for_commit,
    source_control_protected_branch_commit_action,
    source_control_protected_branch_new_branch_required_status,
    source_control_save_pause_external_change_status, source_control_save_pause_unsaved_status,
};
use status::{
    first_stale_source_control_operation_path, no_source_control_changes_status,
    smart_commit_path_count, source_control_has_stage, source_control_paths_for_stage,
    source_control_revealed_status, stale_source_control_discard_status,
    stale_source_control_stage_status,
};
#[cfg(test)]
pub(crate) use status::{
    source_control_protected_branch_prompt_body, source_control_protected_branch_prompt_title,
};

pub(crate) const SOURCE_CONTROL_EMPTY_COMMIT_MESSAGE_STATUS: &str =
    "Commit message cannot be empty";

pub(crate) fn source_control_mutation_restricted_status(action: &str) -> String {
    format!("Trust this workspace before {action}")
}

pub(crate) fn source_control_normalized_commit_message(message: impl AsRef<str>) -> Option<String> {
    let message = message.as_ref().trim();
    (!message.is_empty()).then(|| message.to_owned())
}

impl KuroyaApp {
    pub(crate) fn invalidate_source_control_load_requests(&mut self) {
        self.source_control_history_loading = false;
        invalidate_source_control_load_request_state(
            &mut self.source_control_branch_next_request_id,
            &mut self.source_control_branch_active_request_id,
            &mut self.source_control_branch_in_flight_request_id,
            &mut self.source_control_branch_reload_queued,
        );
        invalidate_source_control_load_request_state(
            &mut self.source_control_history_next_request_id,
            &mut self.source_control_history_active_request_id,
            &mut self.source_control_history_in_flight_request_id,
            &mut self.source_control_history_reload_queued,
        );
        invalidate_source_control_load_request_state(
            &mut self.source_control_stashes_next_request_id,
            &mut self.source_control_stashes_active_request_id,
            &mut self.source_control_stashes_in_flight_request_id,
            &mut self.source_control_stashes_reload_queued,
        );
        invalidate_source_control_load_request_state(
            &mut self.source_control_hunks_next_request_id,
            &mut self.source_control_hunks_active_request_id,
            &mut self.source_control_hunks_in_flight_request_id,
            &mut self.source_control_hunks_reload_queued,
        );
        self.invalidate_virtual_source_control_open_requests();
        invalidate_source_control_load_request_id_state(
            &mut self.source_control_blame_next_request_id,
            &mut self.source_control_blame_active_request_id,
        );
        self.source_control_blame_active_request_ids.clear();
        self.source_control_blame_in_flight_request_ids.clear();
        self.source_control_blame_reload_queued_paths.clear();
        self.source_control_blame_open_view_paths.clear();
    }

    pub(crate) fn invalidate_virtual_source_control_open_requests(&mut self) {
        invalidate_source_control_load_request_id_state(
            &mut self.virtual_diff_open_next_request_id,
            &mut self.virtual_diff_open_active_request_id,
        );
        invalidate_source_control_load_request_id_state(
            &mut self.virtual_revision_open_next_request_id,
            &mut self.virtual_revision_open_active_request_id,
        );
    }

    pub(crate) fn set_git_progress_status(&mut self, status: String) {
        if let Some(status) = git_progress_status(self.settings.git_show_progress, status) {
            self.status = status;
        }
    }

    pub(crate) fn source_control_git_operation_root(&self) -> PathBuf {
        source_control_git_operation_root_for_snapshot(&self.workspace.root, self.git.root())
    }

    pub(crate) fn source_control_git_operation_root_matches(&self, operation_root: &Path) -> bool {
        source_control_git_operation_root_matches_snapshot(
            &self.workspace.root,
            self.git.root(),
            operation_root,
        )
    }

    pub(crate) fn drain_pending_restored_source_control_loads(&mut self) {
        if !self.settings.git_enabled {
            self.clear_pending_restored_source_control_loads();
            return;
        }
        if self.git.root().is_none() {
            return;
        }
        if self.pending_restored_git_history_load {
            self.pending_restored_git_history_load = false;
            self.spawn_restored_git_history_load();
        }
        if self.pending_restored_git_stashes_load {
            self.pending_restored_git_stashes_load = false;
            self.spawn_restored_git_stashes_load();
        }
    }

    pub(crate) fn clear_pending_restored_source_control_loads(&mut self) {
        self.pending_restored_git_history_load = false;
        self.pending_restored_git_stashes_load = false;
    }

    pub(crate) fn require_trusted_source_control_mutation(&mut self, action: &str) -> bool {
        if self.workspace_trusted {
            true
        } else {
            self.status = source_control_mutation_restricted_status(action);
            false
        }
    }

    pub(crate) fn clear_pending_source_control_mutations_for_restricted_workspace(&mut self) {
        self.pending_source_control_discard = None;
        self.invalidate_source_control_branch_operation_requests();
        if let Some(target) = self.pending_source_control_smart_commit.take() {
            self.cancel_source_control_commit_request(target.request_id);
        }
        if let Some(target) = self.pending_source_control_empty_commit.take() {
            self.cancel_source_control_commit_request(target.request_id);
        }
        if let Some(target) = self.pending_source_control_protected_branch_commit.take() {
            self.cancel_source_control_commit_request(target.request_id);
        }
        if let Some(target) = self.pending_source_control_commit_save.take() {
            let request_id = match target {
                crate::transient_state::PendingSourceControlCommitSave::Confirm {
                    request_id,
                    ..
                }
                | crate::transient_state::PendingSourceControlCommitSave::Saving {
                    request_id,
                    ..
                } => request_id,
            };
            self.cancel_source_control_commit_request(request_id);
        }
        self.pending_source_control_stash_save = None;
    }

    pub(crate) fn reveal_active_file_in_source_control(&mut self) {
        let Some(id) = self.active else {
            self.status = "No active file to reveal in Source Control".to_owned();
            return;
        };

        if let Some(source) = self.diff_buffer_sources.get(&id).cloned() {
            self.reveal_file_in_source_control_with_preferred_stage(source.path, source.hunk_stage);
            return;
        }

        let Some(path) = self.buffer(id).and_then(|buffer| buffer.path().cloned()) else {
            self.status = "No file-backed buffer to reveal in Source Control".to_owned();
            return;
        };
        self.reveal_file_in_source_control_with_preferred_stage(
            path,
            Some(GitChangeStage::Unstaged),
        );
    }

    pub(crate) fn reveal_file_in_source_control(&mut self, path: PathBuf) {
        self.reveal_file_in_source_control_with_preferred_stage(path, None);
    }

    pub(crate) fn maybe_auto_reveal_active_file_in_source_control(&mut self) {
        let Some(id) = self.active else {
            return;
        };
        let (path, preferred_stage) =
            if let Some(source) = self.diff_buffer_sources.get(&id).cloned() {
                (source.path, source.hunk_stage)
            } else {
                let Some(path) = self.buffer(id).and_then(|buffer| buffer.path().cloned()) else {
                    return;
                };
                (path, Some(GitChangeStage::Unstaged))
            };

        let entries = crate::source_control_panel::source_control_sorted_entries(
            &self.workspace.root,
            source_control_filtered_entries(
                &self.workspace.root,
                self.git.entries_slice(),
                &self.source_control_query,
            ),
            self.source_control_sort,
        );
        let Some(selection) = source_control_auto_reveal_selection(
            &entries,
            &path,
            preferred_stage,
            self.settings.git_untracked_changes,
            self.settings.scm_auto_reveal,
            self.source_control,
            self.source_control_unstaged_collapsed,
            self.source_control_untracked_collapsed,
            self.source_control_staged_collapsed,
        ) else {
            return;
        };

        self.source_control_unstaged_collapsed = selection.unstaged_collapsed;
        self.source_control_untracked_collapsed = selection.untracked_collapsed;
        self.source_control_staged_collapsed = selection.staged_collapsed;
        self.source_control_selected = selection.selected;
    }

    fn reveal_file_in_source_control_with_preferred_stage(
        &mut self,
        path: PathBuf,
        preferred_stage: Option<GitChangeStage>,
    ) {
        if self.git.root().is_none() {
            self.status = "No git repository".to_owned();
            return;
        }

        let entries = crate::source_control_panel::source_control_sorted_entries(
            &self.workspace.root,
            self.git.entries_slice().to_vec(),
            self.source_control_sort,
        );
        let Some(selection) = source_control_reveal_selection(
            &entries,
            &path,
            preferred_stage,
            self.settings.git_untracked_changes,
            self.source_control_unstaged_collapsed,
            self.source_control_untracked_collapsed,
            self.source_control_staged_collapsed,
        ) else {
            self.status = no_source_control_changes_status(&path);
            return;
        };

        self.source_control = true;
        self.source_control_query.clear();
        self.source_control_unstaged_collapsed = selection.unstaged_collapsed;
        self.source_control_untracked_collapsed = selection.untracked_collapsed;
        self.source_control_staged_collapsed = selection.staged_collapsed;
        self.source_control_selected = selection.selected;
        self.status = source_control_revealed_status(&path);
    }

    pub(crate) fn stage_file_change(&mut self, path: PathBuf) {
        self.spawn_stage_changes(vec![path]);
    }

    pub(crate) fn stage_active_file_changes(&mut self) {
        if !self.require_trusted_source_control_mutation("staging changes") {
            return;
        }
        let Some(path) = self.active_source_control_path("stage changes in") else {
            return;
        };
        self.stage_file_change(path);
    }

    pub(crate) fn stage_all_changes(&mut self) {
        if !self.require_trusted_source_control_mutation("staging changes") {
            return;
        }
        let paths = self.source_control_paths_for_stage(GitChangeStage::Unstaged);

        if paths.is_empty() {
            self.status = "No unstaged changes to stage".to_owned();
            return;
        }

        self.spawn_stage_changes(paths);
    }

    pub(crate) fn unstage_file_change(&mut self, path: PathBuf) {
        self.spawn_unstage_changes(vec![path]);
    }

    pub(crate) fn unstage_active_file_changes(&mut self) {
        if !self.require_trusted_source_control_mutation("unstaging changes") {
            return;
        }
        let Some(path) = self.active_source_control_path("unstage changes in") else {
            return;
        };
        self.unstage_file_change(path);
    }

    pub(crate) fn unstage_all_changes(&mut self) {
        if !self.require_trusted_source_control_mutation("unstaging changes") {
            return;
        }
        let paths = self.source_control_paths_for_stage(GitChangeStage::Staged);

        if paths.is_empty() {
            self.status = "No staged changes to unstage".to_owned();
            return;
        }

        self.spawn_unstage_changes(paths);
    }

    fn source_control_paths_for_stage(&self, stage: GitChangeStage) -> Vec<PathBuf> {
        source_control_paths_for_stage(self.git.entries_slice(), stage)
    }

    pub(crate) fn commit_staged_changes(&mut self) {
        if !self.require_trusted_source_control_mutation("committing changes") {
            return;
        }
        let Some(message) =
            source_control_normalized_commit_message(self.source_control_commit_message.as_str())
        else {
            self.status = SOURCE_CONTROL_EMPTY_COMMIT_MESSAGE_STATUS.to_owned();
            return;
        };

        let entries = self.git.entries_slice();
        let has_staged_changes = source_control_has_stage(entries, GitChangeStage::Staged);
        let smart_commit = if !has_staged_changes && self.settings.git_enable_smart_commit {
            Some(self.settings.git_smart_commit_changes)
        } else {
            None
        };
        if !has_staged_changes {
            let smart_count =
                smart_commit_path_count(entries, self.settings.git_smart_commit_changes);
            if smart_count == 0 {
                if self.settings.git_confirm_empty_commits {
                    let request_id = self.reserve_source_control_commit_request();
                    self.begin_source_control_empty_commit_confirmation(request_id, message);
                } else {
                    self.request_commit_changes(message, None, true);
                }
                return;
            }
            if smart_commit.is_none() {
                if self.settings.git_suggest_smart_commit {
                    let request_id = self.reserve_source_control_commit_request();
                    self.begin_source_control_smart_commit_suggestion(
                        request_id,
                        message,
                        self.settings.git_smart_commit_changes,
                        smart_count,
                    );
                    return;
                }
                self.status = "No staged changes to commit".to_owned();
                return;
            }
        }

        self.request_commit_changes(message, smart_commit, false);
    }

    pub(crate) fn discard_active_file_changes(&mut self) {
        if !self.require_trusted_source_control_mutation("discarding changes") {
            return;
        }
        let Some(path) = self.active_source_control_path("discard changes in") else {
            return;
        };
        self.begin_discard_file_changes(path);
    }

    fn active_source_control_path(&mut self, action: &str) -> Option<PathBuf> {
        if self.git.root().is_none() {
            self.status = "No git repository".to_owned();
            return None;
        }

        let Some(id) = self.active else {
            self.status = format!("No active file to {action}");
            return None;
        };

        if let Some(source) = self.diff_buffer_sources.get(&id) {
            return Some(source.path.clone());
        }

        let Some(path) = self.buffer_file_or_diff_source_path(id) else {
            self.status = format!("No file-backed buffer to {action}");
            return None;
        };

        Some(path)
    }

    fn close_source_control_diff_buffers_for_operation(
        &mut self,
        paths: Option<&[PathBuf]>,
        stage: Option<GitChangeStage>,
    ) {
        if !self.settings.git_close_diff_on_operation {
            return;
        }
        let ids =
            source_control_diff_buffers_for_operation(&self.diff_buffer_sources, paths, stage);
        for id in ids {
            self.force_close_buffer(id);
        }
    }

    pub(crate) fn apply_git_commit_finished(
        &mut self,
        request_id: u64,
        root: PathBuf,
        short_oid: String,
        message: String,
        smart_commit: bool,
    ) {
        if !workspace_event_matches(&self.workspace.root, &root) {
            return;
        }
        match self.finish_source_control_commit_request(request_id) {
            SourceControlCommitRequestFinish::Active => {}
            SourceControlCommitRequestFinish::Stale => {
                self.spawn_git_scan();
                return;
            }
            SourceControlCommitRequestFinish::Unknown => return,
        }

        record_source_control_commit_history(
            &mut self.source_control_commit_history,
            &message,
            SOURCE_CONTROL_COMMIT_HISTORY_LIMIT,
        );
        self.source_control_commit_history_index = None;
        if source_control_normalized_commit_message(self.source_control_commit_message.as_str())
            .as_deref()
            == Some(message.as_str())
        {
            self.source_control_commit_message.clear();
        }
        self.close_source_control_diff_buffers_for_operation(None, None);
        self.spawn_git_scan();
        self.status = git_commit_success_status(&short_oid, smart_commit);
    }

    pub(crate) fn apply_git_commit_failed(
        &mut self,
        request_id: u64,
        root: PathBuf,
        error: String,
        smart_commit: bool,
    ) {
        if !workspace_event_matches(&self.workspace.root, &root) {
            return;
        }
        match self.finish_source_control_commit_request(request_id) {
            SourceControlCommitRequestFinish::Active => {}
            SourceControlCommitRequestFinish::Stale => {
                self.spawn_git_scan();
                return;
            }
            SourceControlCommitRequestFinish::Unknown => return,
        }

        self.spawn_git_scan();
        self.status = git_commit_failure_status(&error, smart_commit);
    }

    pub(crate) fn apply_git_stage_finished(&mut self, root: PathBuf, paths: Vec<PathBuf>) {
        if !workspace_event_matches(&self.workspace.root, &root) {
            return;
        }

        self.close_source_control_diff_buffers_for_operation(
            Some(&paths),
            Some(GitChangeStage::Unstaged),
        );
        self.status = git_stage_success_status(&paths);
        self.spawn_git_scoped_refresh(paths);
    }

    pub(crate) fn apply_git_stage_failed(
        &mut self,
        root: PathBuf,
        paths: Vec<PathBuf>,
        error: String,
    ) {
        if !workspace_event_matches(&self.workspace.root, &root) {
            return;
        }

        self.spawn_git_scan();
        self.status = git_stage_failure_status(&paths, &error);
    }

    pub(crate) fn apply_git_unstage_finished(&mut self, root: PathBuf, paths: Vec<PathBuf>) {
        if !workspace_event_matches(&self.workspace.root, &root) {
            return;
        }

        self.close_source_control_diff_buffers_for_operation(
            Some(&paths),
            Some(GitChangeStage::Staged),
        );
        self.status = git_unstage_success_status(&paths);
        self.spawn_git_scoped_refresh(paths);
    }

    pub(crate) fn apply_git_unstage_failed(
        &mut self,
        root: PathBuf,
        paths: Vec<PathBuf>,
        error: String,
    ) {
        if !workspace_event_matches(&self.workspace.root, &root) {
            return;
        }

        self.spawn_git_scan();
        self.status = git_unstage_failure_status(&paths, &error);
    }

    pub(crate) fn apply_git_discard_finished(&mut self, root: PathBuf, paths: Vec<PathBuf>) {
        if !workspace_event_matches(&self.workspace.root, &root) {
            return;
        }

        self.close_source_control_diff_buffers_for_operation(Some(&paths), None);
        self.spawn_index();
        self.status = git_discard_success_status(&paths);
        self.spawn_git_scoped_refresh(paths);
    }

    pub(crate) fn apply_git_discard_failed(
        &mut self,
        root: PathBuf,
        paths: Vec<PathBuf>,
        error: String,
    ) {
        if !workspace_event_matches(&self.workspace.root, &root) {
            return;
        }

        self.spawn_index();
        self.spawn_git_scan();
        self.status = git_discard_failure_status(&paths, &error);
    }

    fn spawn_stage_changes(&mut self, paths: Vec<PathBuf>) {
        if !self.require_trusted_source_control_mutation("staging changes") {
            return;
        }
        if paths.is_empty() {
            self.status = "No unstaged changes to stage".to_owned();
            return;
        }
        if !self.source_control_stage_paths_current(&paths, GitChangeStage::Unstaged) {
            return;
        }

        let event_root = self.workspace.root.clone();
        let git_root = self.source_control_git_operation_root();
        let tx = self.tx.clone();
        self.set_git_progress_status(git_stage_pending_status(&paths));
        self.record_async_task_started("Git Stage", paths_detail(&paths));
        self.runtime.spawn_blocking(move || {
            let result = stage_paths(&git_root, paths.iter().map(PathBuf::as_path));
            let event = match result {
                Ok(()) => UiEvent::GitStageFinished {
                    root: event_root,
                    paths,
                },
                Err(error) => UiEvent::GitStageFailed {
                    root: event_root,
                    paths,
                    error: error.to_string(),
                },
            };
            let _ = crate::ui_event_channel::send_ui_event(&tx, event);
        });
    }

    fn spawn_unstage_changes(&mut self, paths: Vec<PathBuf>) {
        if !self.require_trusted_source_control_mutation("unstaging changes") {
            return;
        }
        if paths.is_empty() {
            self.status = "No staged changes to unstage".to_owned();
            return;
        }
        if !self.source_control_stage_paths_current(&paths, GitChangeStage::Staged) {
            return;
        }

        let event_root = self.workspace.root.clone();
        let git_root = self.source_control_git_operation_root();
        let tx = self.tx.clone();
        self.set_git_progress_status(git_unstage_pending_status(&paths));
        self.record_async_task_started("Git Unstage", paths_detail(&paths));
        self.runtime.spawn_blocking(move || {
            let result = unstage_paths(&git_root, paths.iter().map(PathBuf::as_path));
            let event = match result {
                Ok(()) => UiEvent::GitUnstageFinished {
                    root: event_root,
                    paths,
                },
                Err(error) => UiEvent::GitUnstageFailed {
                    root: event_root,
                    paths,
                    error: error.to_string(),
                },
            };
            let _ = crate::ui_event_channel::send_ui_event(&tx, event);
        });
    }

    pub(crate) fn spawn_discard_changes(&mut self, paths: Vec<PathBuf>) {
        if !self.require_trusted_source_control_mutation("discarding changes") {
            return;
        }
        if paths.is_empty() {
            self.status = "No source control changes to discard".to_owned();
            return;
        }
        if !self.source_control_discard_paths_current(&paths) {
            return;
        }

        let event_root = self.workspace.root.clone();
        let git_root = self.source_control_git_operation_root();
        let tx = self.tx.clone();
        self.set_git_progress_status(git_discard_pending_status(&paths));
        self.record_async_task_started("Git Discard", paths_detail(&paths));
        self.runtime.spawn_blocking(move || {
            let result = discard_paths(&git_root, paths.iter().map(PathBuf::as_path));
            let event = match result {
                Ok(()) => UiEvent::GitDiscardFinished {
                    root: event_root,
                    paths,
                },
                Err(error) => UiEvent::GitDiscardFailed {
                    root: event_root,
                    paths,
                    error: error.to_string(),
                },
            };
            let _ = crate::ui_event_channel::send_ui_event(&tx, event);
        });
    }

    fn source_control_stage_paths_current(
        &mut self,
        paths: &[PathBuf],
        stage: GitChangeStage,
    ) -> bool {
        let operation_root = self.source_control_git_operation_root();
        let stale_path =
            first_stale_source_control_operation_path(paths, &operation_root, |path| {
                self.git.has_stage_for(path, stage)
            });
        if let Some(stale_path) = stale_path {
            self.status = stale_source_control_stage_status(stage, paths, stale_path);
            return false;
        }
        true
    }

    pub(crate) fn source_control_discard_paths_current(&mut self, paths: &[PathBuf]) -> bool {
        let operation_root = self.source_control_git_operation_root();
        let stale_path =
            first_stale_source_control_operation_path(paths, &operation_root, |path| {
                self.git.status_for(path).is_some()
            });
        if let Some(stale_path) = stale_path {
            self.status = stale_source_control_discard_status(paths, stale_path);
            return false;
        }
        true
    }

    #[cfg(test)]
    pub(crate) fn spawn_commit_changes(
        &mut self,
        message: String,
        smart_commit_changes: Option<GitSmartCommitChanges>,
        allow_empty: bool,
    ) {
        if !self.require_trusted_source_control_mutation("committing changes") {
            return;
        }
        let Some(message) = source_control_normalized_commit_message(message) else {
            self.status = SOURCE_CONTROL_EMPTY_COMMIT_MESSAGE_STATUS.to_owned();
            return;
        };
        let request_id = self.reserve_source_control_commit_request();
        self.spawn_commit_changes_with_request(
            request_id,
            message,
            smart_commit_changes,
            allow_empty,
        );
    }

    pub(crate) fn spawn_commit_changes_with_request(
        &mut self,
        request_id: u64,
        message: String,
        smart_commit_changes: Option<GitSmartCommitChanges>,
        allow_empty: bool,
    ) {
        if !self.source_control_commit_request_is_active(request_id) {
            return;
        }
        if !self.require_trusted_source_control_mutation("committing changes") {
            self.cancel_source_control_commit_request(request_id);
            return;
        }
        let Some(message) = source_control_normalized_commit_message(message) else {
            self.cancel_source_control_commit_request(request_id);
            self.status = SOURCE_CONTROL_EMPTY_COMMIT_MESSAGE_STATUS.to_owned();
            return;
        };
        self.mark_source_control_commit_request_in_flight(request_id);
        let event_root = self.workspace.root.clone();
        let git_root = self.source_control_git_operation_root();
        let tx = self.tx.clone();
        let smart_commit = smart_commit_changes.is_some();
        let short_hash_length = self.settings.git_commit_short_hash_length;
        let sign_off = self.settings.git_always_sign_off;
        let require_user_config = self.settings.git_require_user_config;
        self.set_git_progress_status(git_commit_pending_status(smart_commit));
        self.record_async_task_started(
            "Git Commit",
            if allow_empty {
                "empty commit"
            } else if smart_commit {
                "smart commit changes"
            } else {
                "staged changes"
            },
        );
        self.runtime.spawn_blocking(move || {
            let result = git_commit_changes(
                &git_root,
                &message,
                smart_commit_changes,
                short_hash_length,
                sign_off,
                allow_empty,
                require_user_config,
            );
            let event = match result {
                Ok(short_oid) => UiEvent::GitCommitFinished {
                    request_id,
                    root: event_root,
                    short_oid,
                    message,
                    smart_commit,
                },
                Err(error) => UiEvent::GitCommitFailed {
                    request_id,
                    root: event_root,
                    error: error.to_string(),
                    smart_commit,
                },
            };
            let _ = crate::ui_event_channel::send_ui_event(&tx, event);
        });
    }

    pub(crate) fn request_commit_changes(
        &mut self,
        message: String,
        smart_commit_changes: Option<GitSmartCommitChanges>,
        allow_empty: bool,
    ) {
        if !self.require_trusted_source_control_mutation("committing changes") {
            return;
        }
        let Some(message) = source_control_normalized_commit_message(message) else {
            self.status = SOURCE_CONTROL_EMPTY_COMMIT_MESSAGE_STATUS.to_owned();
            return;
        };
        let request_id = self.reserve_source_control_commit_request();
        self.request_commit_changes_with_request(
            request_id,
            message,
            smart_commit_changes,
            allow_empty,
        );
    }

    pub(crate) fn request_commit_changes_with_request(
        &mut self,
        request_id: u64,
        message: String,
        smart_commit_changes: Option<GitSmartCommitChanges>,
        allow_empty: bool,
    ) {
        if !self.source_control_commit_request_is_active(request_id) {
            return;
        }
        if !self.require_trusted_source_control_mutation("committing changes") {
            self.cancel_source_control_commit_request(request_id);
            return;
        }
        let Some(message) = source_control_normalized_commit_message(message) else {
            self.cancel_source_control_commit_request(request_id);
            self.status = SOURCE_CONTROL_EMPTY_COMMIT_MESSAGE_STATUS.to_owned();
            return;
        };
        let Some(branch) = self.git.branch().map(ToOwned::to_owned) else {
            self.request_commit_changes_after_branch_protection_with_request(
                request_id,
                message,
                smart_commit_changes,
                allow_empty,
            );
            return;
        };
        match source_control_protected_branch_commit_action(
            Some(&branch),
            &self.settings.git_branch_protection,
            self.settings.git_branch_protection_prompt,
        ) {
            SourceControlProtectedBranchCommitAction::Allow => {
                self.request_commit_changes_after_branch_protection_with_request(
                    request_id,
                    message,
                    smart_commit_changes,
                    allow_empty,
                );
            }
            SourceControlProtectedBranchCommitAction::Prompt { pattern } => {
                self.begin_source_control_protected_branch_commit_prompt(
                    request_id,
                    message,
                    smart_commit_changes,
                    allow_empty,
                    branch,
                    pattern,
                );
            }
            SourceControlProtectedBranchCommitAction::RequireNewBranch { pattern } => {
                self.cancel_source_control_commit_request(request_id);
                self.begin_git_branch_switcher();
                self.status =
                    source_control_protected_branch_new_branch_required_status(&branch, &pattern);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn request_commit_changes_after_branch_protection(
        &mut self,
        message: String,
        smart_commit_changes: Option<GitSmartCommitChanges>,
        allow_empty: bool,
    ) {
        if !self.require_trusted_source_control_mutation("committing changes") {
            return;
        }
        let Some(message) = source_control_normalized_commit_message(message) else {
            self.status = SOURCE_CONTROL_EMPTY_COMMIT_MESSAGE_STATUS.to_owned();
            return;
        };
        let request_id = self.reserve_source_control_commit_request();
        self.request_commit_changes_after_branch_protection_with_request(
            request_id,
            message,
            smart_commit_changes,
            allow_empty,
        );
    }

    pub(crate) fn request_commit_changes_after_branch_protection_with_request(
        &mut self,
        request_id: u64,
        message: String,
        smart_commit_changes: Option<GitSmartCommitChanges>,
        allow_empty: bool,
    ) {
        if !self.source_control_commit_request_is_active(request_id) {
            return;
        }
        if !self.require_trusted_source_control_mutation("committing changes") {
            self.cancel_source_control_commit_request(request_id);
            return;
        }
        let Some(message) = source_control_normalized_commit_message(message) else {
            self.cancel_source_control_commit_request(request_id);
            self.status = SOURCE_CONTROL_EMPTY_COMMIT_MESSAGE_STATUS.to_owned();
            return;
        };
        let ids = source_control_commit_save_prompt_ids_for_commit(
            &self.buffers,
            self.git.entries_slice(),
            self.settings.git_prompt_to_save_files_before_commit,
            smart_commit_changes,
        );
        if ids.is_empty() {
            self.spawn_commit_changes_with_request(
                request_id,
                message,
                smart_commit_changes,
                allow_empty,
            );
        } else {
            self.begin_source_control_commit_save_prompt(
                request_id,
                message,
                smart_commit_changes,
                allow_empty,
                ids,
            );
        }
    }

    pub(crate) fn advance_pending_source_control_commit_after_save(&mut self) {
        let Some(mut pending) = self.pending_source_control_commit_save.take() else {
            return;
        };
        pending.prune_invalid_buffer_ids(|id| self.buffer(id).is_some());
        let (request_id, message, smart_commit_changes, allow_empty, ids) = match pending {
            crate::transient_state::PendingSourceControlCommitSave::Saving {
                request_id,
                message,
                smart_commit_changes,
                allow_empty,
                ids,
            } => (request_id, message, smart_commit_changes, allow_empty, ids),
            pending => {
                self.pending_source_control_commit_save = Some(pending);
                return;
            }
        };
        if !self.require_trusted_source_control_mutation("committing changes") {
            self.cancel_source_control_commit_request(request_id);
            return;
        }
        if ids.iter().any(|id| {
            has_active_save_work(
                *id,
                &self.in_flight_saves,
                &self.queued_save_paths,
                &self.pending_format_on_save,
            )
        }) {
            self.pending_source_control_commit_save = Some(
                crate::transient_state::PendingSourceControlCommitSave::Saving {
                    request_id,
                    message,
                    smart_commit_changes,
                    allow_empty,
                    ids,
                },
            );
            return;
        }

        let changed_on_disk = self.pending_source_control_save_external_change_count(&ids);
        if changed_on_disk > 0 {
            self.pending_source_control_commit_save = Some(
                crate::transient_state::PendingSourceControlCommitSave::Confirm {
                    request_id,
                    message,
                    smart_commit_changes,
                    allow_empty,
                    ids,
                },
            );
            self.status =
                source_control_save_pause_external_change_status("Commit", changed_on_disk);
            return;
        }

        let still_dirty = ids
            .iter()
            .filter(|id| self.buffer(**id).is_some_and(TextBuffer::is_dirty))
            .count();
        if still_dirty == 0 {
            self.spawn_commit_changes_with_request(
                request_id,
                message,
                smart_commit_changes,
                allow_empty,
            );
        } else {
            self.pending_source_control_commit_save = Some(
                crate::transient_state::PendingSourceControlCommitSave::Confirm {
                    request_id,
                    message,
                    smart_commit_changes,
                    allow_empty,
                    ids,
                },
            );
            self.status = source_control_save_pause_unsaved_status("Commit", still_dirty);
        }
    }

    pub(crate) fn reserve_source_control_commit_request(&mut self) -> u64 {
        reserve_source_control_commit_request_id_state(
            &mut self.source_control_commit_next_request_id,
            &mut self.source_control_commit_active_request_id,
            &mut self.source_control_commit_in_flight_request_ids,
        )
    }

    fn mark_source_control_commit_request_in_flight(&mut self, request_id: u64) {
        mark_source_control_commit_request_in_flight_state(
            &mut self.source_control_commit_in_flight_request_ids,
            request_id,
        );
    }

    fn source_control_commit_request_is_active(&self, request_id: u64) -> bool {
        self.source_control_commit_active_request_id == request_id
    }

    fn finish_source_control_commit_request(
        &mut self,
        request_id: u64,
    ) -> SourceControlCommitRequestFinish {
        finish_source_control_commit_request_state(
            &mut self.source_control_commit_active_request_id,
            &mut self.source_control_commit_in_flight_request_ids,
            request_id,
        )
    }

    pub(crate) fn cancel_source_control_commit_request(&mut self, request_id: u64) {
        cancel_source_control_commit_request_state(
            &mut self.source_control_commit_active_request_id,
            &mut self.source_control_commit_in_flight_request_ids,
            request_id,
        );
    }

    pub(crate) fn pending_source_control_save_external_change_count(
        &self,
        ids: &[BufferId],
    ) -> usize {
        let changed_on_disk = self.observed_external_change_buffer_ids();
        ids.iter().filter(|id| changed_on_disk.contains(id)).count()
    }

    pub(crate) fn pause_pending_source_control_commit_after_save_failure(&mut self, id: BufferId) {
        let Some(pending) = self.pending_source_control_commit_save.take() else {
            return;
        };
        let (request_id, message, smart_commit_changes, allow_empty, ids) = match pending {
            crate::transient_state::PendingSourceControlCommitSave::Saving {
                request_id,
                message,
                smart_commit_changes,
                allow_empty,
                ids,
            } => (request_id, message, smart_commit_changes, allow_empty, ids),
            pending => {
                self.pending_source_control_commit_save = Some(pending);
                return;
            }
        };
        if ids.contains(&id) {
            self.pending_source_control_commit_save = Some(
                crate::transient_state::PendingSourceControlCommitSave::Confirm {
                    request_id,
                    message,
                    smart_commit_changes,
                    allow_empty,
                    ids,
                },
            );
        } else {
            self.pending_source_control_commit_save = Some(
                crate::transient_state::PendingSourceControlCommitSave::Saving {
                    request_id,
                    message,
                    smart_commit_changes,
                    allow_empty,
                    ids,
                },
            );
        }
    }
}

pub(crate) fn source_control_diff_buffers_for_operation(
    sources: &HashMap<BufferId, DiffBufferSource>,
    paths: Option<&[PathBuf]>,
    stage: Option<GitChangeStage>,
) -> Vec<BufferId> {
    let path_filter = SourceControlDiffBufferPathFilter::new(paths);
    let mut ids = Vec::with_capacity(sources.len());
    for (id, source) in sources {
        if source.hunk_stage.is_some()
            && stage.is_none_or(|stage| source.hunk_stage == Some(stage))
            && path_filter.contains(&source.path)
        {
            ids.push(*id);
        }
    }
    ids.sort_unstable();
    ids
}

enum SourceControlDiffBufferPathFilter<'a> {
    All,
    Empty,
    Single(&'a Path),
    Many(HashSet<&'a Path>),
}

impl<'a> SourceControlDiffBufferPathFilter<'a> {
    fn new(paths: Option<&'a [PathBuf]>) -> Self {
        match paths {
            None => Self::All,
            Some([]) => Self::Empty,
            Some([path]) => Self::Single(path.as_path()),
            Some(paths) => {
                let mut unique_paths = HashSet::with_capacity(paths.len());
                unique_paths.extend(paths.iter().map(|path| path.as_path()));
                Self::Many(unique_paths)
            }
        }
    }

    fn contains(&self, path: &Path) -> bool {
        match self {
            Self::All => true,
            Self::Empty => false,
            Self::Single(candidate) => *candidate == path,
            Self::Many(paths) => paths.contains(path),
        }
    }
}

pub(crate) fn source_control_load_event_matches(
    current_root: &std::path::Path,
    event_root: &std::path::Path,
    request_id: u64,
    active_request_id: u64,
) -> bool {
    source_control_root_matches_workspace(current_root, event_root)
        && background_request_matches(request_id, active_request_id)
}

pub(crate) fn source_control_panel_load_event_matches(
    panel_open: bool,
    current_root: &std::path::Path,
    event_root: &std::path::Path,
    request_id: u64,
    active_request_id: u64,
) -> bool {
    panel_open
        && source_control_load_event_matches(
            current_root,
            event_root,
            request_id,
            active_request_id,
        )
}

pub(crate) fn source_control_root_matches_workspace(
    workspace_root: &std::path::Path,
    git_root: &std::path::Path,
) -> bool {
    workspace_event_matches(workspace_root, git_root)
        || workspace_path_contains_lexically(workspace_root, git_root)
        || workspace_path_contains_lexically(git_root, workspace_root)
}

pub(crate) fn source_control_git_operation_root_for_snapshot(
    workspace_root: &std::path::Path,
    git_root: Option<&std::path::Path>,
) -> PathBuf {
    git_root
        .filter(|git_root| source_control_root_matches_workspace(workspace_root, git_root))
        .map(Path::to_path_buf)
        .unwrap_or_else(|| workspace_root.to_path_buf())
}

pub(crate) fn source_control_git_operation_root_matches_snapshot(
    workspace_root: &std::path::Path,
    git_root: Option<&std::path::Path>,
    operation_root: &std::path::Path,
) -> bool {
    if let Some(git_root) =
        git_root.filter(|git_root| source_control_root_matches_workspace(workspace_root, git_root))
    {
        workspace_event_matches(git_root, operation_root)
    } else {
        workspace_event_matches(workspace_root, operation_root)
    }
}

pub(crate) fn reserve_source_control_load_request_id_state(
    next_request_id: &mut u64,
    active_request_id: &mut u64,
) -> u64 {
    *next_request_id = next_source_control_load_request_id(*next_request_id);
    *active_request_id = *next_request_id;
    *active_request_id
}

fn next_source_control_load_request_id(current: u64) -> u64 {
    current.checked_add(1).filter(|id| *id != 0).unwrap_or(1)
}

fn next_source_control_commit_request_id(current: u64) -> u64 {
    current.checked_add(1).filter(|id| *id != 0).unwrap_or(1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceControlCommitRequestFinish {
    Active,
    Stale,
    Unknown,
}

pub(crate) fn reserve_source_control_commit_request_id_state(
    next_request_id: &mut u64,
    active_request_id: &mut u64,
    in_flight_request_ids: &mut HashSet<u64>,
) -> u64 {
    let mut request_id = next_source_control_commit_request_id(*next_request_id);
    while in_flight_request_ids.contains(&request_id) {
        request_id = next_source_control_commit_request_id(request_id);
    }
    *next_request_id = request_id;
    *active_request_id = request_id;
    request_id
}

pub(crate) fn mark_source_control_commit_request_in_flight_state(
    in_flight_request_ids: &mut HashSet<u64>,
    request_id: u64,
) {
    in_flight_request_ids.insert(request_id);
}

pub(crate) fn finish_source_control_commit_request_state(
    active_request_id: &mut u64,
    in_flight_request_ids: &mut HashSet<u64>,
    request_id: u64,
) -> SourceControlCommitRequestFinish {
    if !in_flight_request_ids.remove(&request_id) {
        return SourceControlCommitRequestFinish::Unknown;
    }
    if *active_request_id != request_id {
        return SourceControlCommitRequestFinish::Stale;
    }
    *active_request_id = 0;
    SourceControlCommitRequestFinish::Active
}

pub(crate) fn cancel_source_control_commit_request_state(
    active_request_id: &mut u64,
    in_flight_request_ids: &mut HashSet<u64>,
    request_id: u64,
) {
    if *active_request_id == request_id {
        *active_request_id = 0;
    }
    in_flight_request_ids.remove(&request_id);
}

pub(crate) fn begin_source_control_load_request_state(
    next_request_id: &mut u64,
    active_request_id: &mut u64,
    in_flight_request_id: &mut Option<u64>,
    reload_queued: &mut bool,
) -> Option<u64> {
    let request_id =
        reserve_source_control_load_request_id_state(next_request_id, active_request_id);
    if in_flight_request_id.is_some() {
        *reload_queued = true;
        None
    } else {
        *in_flight_request_id = Some(request_id);
        Some(request_id)
    }
}

pub(crate) fn finish_source_control_load_request_state(
    in_flight_request_id: &mut Option<u64>,
    reload_queued: &mut bool,
    request_id: u64,
) -> bool {
    if *in_flight_request_id != Some(request_id) {
        return false;
    }
    *in_flight_request_id = None;
    let should_spawn_queued_reload = *reload_queued;
    *reload_queued = false;
    should_spawn_queued_reload
}

pub(crate) fn invalidate_source_control_load_request_id_state(
    next_request_id: &mut u64,
    active_request_id: &mut u64,
) {
    let _ = reserve_source_control_load_request_id_state(next_request_id, active_request_id);
}

pub(crate) fn invalidate_source_control_load_request_state(
    next_request_id: &mut u64,
    active_request_id: &mut u64,
    in_flight_request_id: &mut Option<u64>,
    reload_queued: &mut bool,
) {
    invalidate_source_control_load_request_id_state(next_request_id, active_request_id);
    *in_flight_request_id = None;
    *reload_queued = false;
}

#[cfg(test)]
pub(crate) fn source_control_app_for_test(root: PathBuf, trusted: bool) -> KuroyaApp {
    let (tx, rx) = crate::ui_event_channel::ui_event_channel();
    let settings = kuroya_core::EditorSettings::default();
    let trusted_workspaces = if trusted {
        vec![root.clone()]
    } else {
        Vec::new()
    };
    KuroyaApp::from_startup_context(crate::app_startup_context::AppStartupContext {
        runtime: tokio::runtime::Runtime::new().expect("test runtime"),
        tx,
        rx,
        workspace: kuroya_core::Workspace::new(root.clone()),
        settings: settings.clone(),
        settings_panel_draft: settings,
        settings_editor_font_path: String::new(),
        settings_ui_font_path: String::new(),
        theme_picker_selected: 0,
        saved_session: None,
        terminal: crate::terminal::TerminalPane::new(root.clone(), 100, 12.0, 1.2),
        watcher: None,
        recent_projects: Vec::new(),
        trusted_workspaces,
        now: std::time::Instant::now(),
        startup_timings: Vec::new(),
    })
}

#[cfg(test)]
mod tests;
