use crate::{
    KuroyaApp,
    devtools_async_tasks::{index_detail, path_detail},
    path_display::sanitized_display_label_cow,
    save_lifecycle::has_active_save_work,
    source_control_runtime::{
        finish_source_control_load_request_state, source_control_commit_save_prompt_ids,
        source_control_save_pause_external_change_status, source_control_save_pause_unsaved_status,
    },
    ui_events::UiEvent,
    workspace_event_guards::background_request_matches,
    workspace_state::workspace_event_matches,
};
use kuroya_core::{
    BufferId, GitStashEntry, TextBuffer, apply_stash, drop_stash,
    list_stashes_with_short_hash_length, pop_stash, save_stash_with_user_config_option,
};
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

#[cfg(test)]
const SOURCE_CONTROL_STASH_DISPLAY_MAX_CHARS: usize = 160;
const SOURCE_CONTROL_STASH_STATUS_DETAIL_MAX_CHARS: usize = 240;
const SOURCE_CONTROL_STASH_HASH_DISPLAY_MAX_CHARS: usize = 64;

impl KuroyaApp {
    pub(crate) fn begin_git_stashes_panel(&mut self) {
        self.source_control_stashes_open = true;
        self.git_panel_open_generation += 1;
        self.source_control_stash_selected = 0;
        if self.source_control_stash_message.trim().is_empty() {
            self.source_control_stash_message = source_control_stash_message_from_inputs(
                "",
                &self.source_control_commit_message,
                self.settings.git_use_commit_input_as_stash_message,
            );
        }
        self.spawn_git_stashes_load();
    }

    pub(crate) fn save_git_stash_from_input(&mut self) {
        if !self.require_trusted_source_control_mutation("creating a stash") {
            return;
        }
        let message = source_control_stash_message_from_inputs(
            &self.source_control_stash_message,
            &self.source_control_commit_message,
            self.settings.git_use_commit_input_as_stash_message,
        );
        let ids = source_control_commit_save_prompt_ids(
            &self.buffers,
            self.git.entries_slice(),
            self.settings.git_prompt_to_save_files_before_stash,
        );
        if ids.is_empty() {
            self.spawn_git_stash_save(message);
        } else {
            self.begin_source_control_stash_save_prompt(message, ids);
        }
    }

    pub(crate) fn spawn_git_stash_save(&mut self, message: String) {
        if !self.require_trusted_source_control_mutation("creating a stash") {
            return;
        }
        let event_root = self.workspace.root.clone();
        let git_root = self.source_control_git_operation_root();
        let operation_root = git_root.clone();
        let short_hash_length = self.settings.git_commit_short_hash_length;
        let require_user_config = self.settings.git_require_user_config;
        let tx = self.tx.clone();
        self.set_git_progress_status(git_stash_save_pending_status());
        self.record_async_task_started("Git Stash Save", "worktree");
        self.runtime.spawn_blocking(move || {
            let result = save_stash_with_user_config_option(
                &git_root,
                &message,
                short_hash_length,
                require_user_config,
            );
            let event = match result {
                Ok(short_oid) => UiEvent::GitStashSaved {
                    root: event_root,
                    operation_root,
                    short_oid,
                },
                Err(error) => UiEvent::GitStashSaveFailed {
                    root: event_root,
                    operation_root,
                    error: error.to_string(),
                },
            };
            let _ = crate::ui_event_channel::send_ui_event(&tx, event);
        });
    }

    pub(crate) fn advance_pending_source_control_stash_after_save(&mut self) {
        let Some(mut pending) = self.pending_source_control_stash_save.take() else {
            return;
        };
        pending.prune_invalid_buffer_ids(|id| self.buffer(id).is_some());
        let crate::transient_state::PendingSourceControlStashSave::Saving { message, ids } =
            pending
        else {
            self.pending_source_control_stash_save = Some(pending);
            return;
        };
        if !self.require_trusted_source_control_mutation("creating a stash") {
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
            self.pending_source_control_stash_save = Some(
                crate::transient_state::PendingSourceControlStashSave::Saving { message, ids },
            );
            return;
        }

        let changed_on_disk = self.pending_source_control_save_external_change_count(&ids);
        if changed_on_disk > 0 {
            self.pending_source_control_stash_save = Some(
                crate::transient_state::PendingSourceControlStashSave::Confirm { message, ids },
            );
            self.status =
                source_control_save_pause_external_change_status("Stash", changed_on_disk);
            return;
        }

        let still_dirty = ids
            .iter()
            .filter(|id| self.buffer(**id).is_some_and(TextBuffer::is_dirty))
            .count();
        if still_dirty == 0 {
            self.pending_source_control_stash_save = None;
            self.spawn_git_stash_save(message);
        } else {
            self.pending_source_control_stash_save = Some(
                crate::transient_state::PendingSourceControlStashSave::Confirm { message, ids },
            );
            self.status = source_control_save_pause_unsaved_status("Stash", still_dirty);
        }
    }

    pub(crate) fn pause_pending_source_control_stash_after_save_failure(&mut self, id: BufferId) {
        let Some(pending) = self.pending_source_control_stash_save.take() else {
            return;
        };
        let crate::transient_state::PendingSourceControlStashSave::Saving { message, ids } =
            pending
        else {
            self.pending_source_control_stash_save = Some(pending);
            return;
        };
        if ids.contains(&id) {
            self.pending_source_control_stash_save = Some(
                crate::transient_state::PendingSourceControlStashSave::Confirm { message, ids },
            );
        } else {
            self.pending_source_control_stash_save = Some(
                crate::transient_state::PendingSourceControlStashSave::Saving { message, ids },
            );
        }
    }

    pub(crate) fn apply_git_stash(&mut self, index: usize) {
        if !self.require_trusted_source_control_mutation("applying a stash") {
            return;
        }
        let event_root = self.workspace.root.clone();
        let git_root = self.source_control_git_operation_root();
        let operation_root = git_root.clone();
        let tx = self.tx.clone();
        self.set_git_progress_status(git_stash_apply_pending_status(index));
        self.record_async_task_started("Git Stash Apply", index_detail(index));
        self.runtime.spawn_blocking(move || {
            let result = apply_stash(&git_root, index);
            let event = match result {
                Ok(()) => UiEvent::GitStashApplied {
                    root: event_root,
                    operation_root,
                    index,
                },
                Err(error) => UiEvent::GitStashApplyFailed {
                    root: event_root,
                    operation_root,
                    index,
                    error: error.to_string(),
                },
            };
            let _ = crate::ui_event_channel::send_ui_event(&tx, event);
        });
    }

    pub(crate) fn pop_git_stash(&mut self, index: usize) {
        if !self.require_trusted_source_control_mutation("popping a stash") {
            return;
        }
        let event_root = self.workspace.root.clone();
        let git_root = self.source_control_git_operation_root();
        let operation_root = git_root.clone();
        let tx = self.tx.clone();
        self.set_git_progress_status(git_stash_pop_pending_status(index));
        self.record_async_task_started("Git Stash Pop", index_detail(index));
        self.runtime.spawn_blocking(move || {
            let result = pop_stash(&git_root, index);
            let event = match result {
                Ok(()) => UiEvent::GitStashPopped {
                    root: event_root,
                    operation_root,
                    index,
                },
                Err(error) => UiEvent::GitStashPopFailed {
                    root: event_root,
                    operation_root,
                    index,
                    error: error.to_string(),
                },
            };
            let _ = crate::ui_event_channel::send_ui_event(&tx, event);
        });
    }

    pub(crate) fn drop_git_stash(&mut self, index: usize) {
        if !self.require_trusted_source_control_mutation("dropping a stash") {
            return;
        }
        let event_root = self.workspace.root.clone();
        let git_root = self.source_control_git_operation_root();
        let operation_root = git_root.clone();
        let tx = self.tx.clone();
        self.set_git_progress_status(git_stash_drop_pending_status(index));
        self.record_async_task_started("Git Stash Drop", index_detail(index));
        self.runtime.spawn_blocking(move || {
            let result = drop_stash(&git_root, index);
            let event = match result {
                Ok(()) => UiEvent::GitStashDropped {
                    root: event_root,
                    operation_root,
                    index,
                },
                Err(error) => UiEvent::GitStashDropFailed {
                    root: event_root,
                    operation_root,
                    index,
                    error: error.to_string(),
                },
            };
            let _ = crate::ui_event_channel::send_ui_event(&tx, event);
        });
    }

    pub(crate) fn apply_git_stashes_loaded(
        &mut self,
        request_id: u64,
        root: PathBuf,
        operation_root: PathBuf,
        stashes: Vec<GitStashEntry>,
    ) {
        if !self.source_control_stash_load_event_matches(request_id, &root, &operation_root) {
            return;
        }

        let selected_stash = source_control_stash_selected_identity(
            &self.source_control_stashes,
            self.source_control_stash_selected,
        );
        let count = stashes.len();
        let stashes = source_control_stashes_for_display(stashes);
        self.source_control_stash_selected = source_control_stash_selection_after_reload(
            selected_stash.as_ref(),
            self.source_control_stash_selected,
            &stashes,
        );
        self.source_control_stashes = stashes;
        self.status = git_stash_list_success_status(count);
    }

    pub(crate) fn apply_git_stashes_failed(
        &mut self,
        request_id: u64,
        root: PathBuf,
        operation_root: PathBuf,
        error: String,
    ) {
        if !self.source_control_stash_load_event_matches(request_id, &root, &operation_root) {
            return;
        }

        self.source_control_stashes.clear();
        self.status = git_stash_list_failure_status(&error);
    }

    pub(crate) fn apply_git_stash_saved(
        &mut self,
        root: PathBuf,
        operation_root: PathBuf,
        short_oid: String,
    ) {
        if !self.source_control_stash_operation_event_matches(&root, &operation_root) {
            return;
        }

        self.source_control_stash_message.clear();
        self.spawn_index();
        self.spawn_git_scan();
        if self.source_control_stashes_open {
            self.spawn_git_stashes_load();
        }
        self.status = git_stash_save_success_status(&short_oid);
    }

    pub(crate) fn apply_git_stash_save_failed(
        &mut self,
        root: PathBuf,
        operation_root: PathBuf,
        error: String,
    ) {
        if !self.source_control_stash_operation_event_matches(&root, &operation_root) {
            return;
        }

        self.status = git_stash_save_failure_status(&error);
    }

    pub(crate) fn apply_git_stash_applied(
        &mut self,
        root: PathBuf,
        operation_root: PathBuf,
        index: usize,
    ) {
        if !self.source_control_stash_operation_event_matches(&root, &operation_root) {
            return;
        }

        self.spawn_index();
        self.spawn_git_scan();
        self.status = git_stash_apply_success_status(index);
    }

    pub(crate) fn apply_git_stash_apply_failed(
        &mut self,
        root: PathBuf,
        operation_root: PathBuf,
        index: usize,
        error: String,
    ) {
        if !self.source_control_stash_operation_event_matches(&root, &operation_root) {
            return;
        }

        self.spawn_index();
        self.spawn_git_scan();
        self.reload_git_stashes_panel_after_operation();
        self.status = git_stash_apply_failure_status(index, &error);
    }

    pub(crate) fn apply_git_stash_popped(
        &mut self,
        root: PathBuf,
        operation_root: PathBuf,
        index: usize,
    ) {
        if !self.source_control_stash_operation_event_matches(&root, &operation_root) {
            return;
        }

        self.spawn_index();
        self.spawn_git_scan();
        if self.source_control_stashes_open {
            self.spawn_git_stashes_load();
        }
        self.status = git_stash_pop_success_status(index);
    }

    pub(crate) fn apply_git_stash_pop_failed(
        &mut self,
        root: PathBuf,
        operation_root: PathBuf,
        index: usize,
        error: String,
    ) {
        if !self.source_control_stash_operation_event_matches(&root, &operation_root) {
            return;
        }

        self.spawn_index();
        self.spawn_git_scan();
        self.reload_git_stashes_panel_after_operation();
        self.status = git_stash_pop_failure_status(index, &error);
    }

    pub(crate) fn apply_git_stash_dropped(
        &mut self,
        root: PathBuf,
        operation_root: PathBuf,
        index: usize,
    ) {
        if !self.source_control_stash_operation_event_matches(&root, &operation_root) {
            return;
        }

        if self.source_control_stashes_open {
            self.spawn_git_stashes_load();
        }
        self.status = git_stash_drop_success_status(index);
    }

    pub(crate) fn apply_git_stash_drop_failed(
        &mut self,
        root: PathBuf,
        operation_root: PathBuf,
        index: usize,
        error: String,
    ) {
        if !self.source_control_stash_operation_event_matches(&root, &operation_root) {
            return;
        }

        self.reload_git_stashes_panel_after_operation();
        self.status = git_stash_drop_failure_status(index, &error);
    }

    pub(crate) fn spawn_git_stashes_load(&mut self) -> bool {
        let Some(request_id) = self.begin_source_control_stashes_request() else {
            self.set_git_progress_status(git_stash_list_pending_status());
            return false;
        };
        let event_root = self.workspace.root.clone();
        let git_root = self.source_control_git_operation_root();
        let operation_root = git_root.clone();
        let tx = self.tx.clone();
        let short_hash_length = self.settings.git_commit_short_hash_length;
        self.set_git_progress_status(git_stash_list_pending_status());
        self.record_async_task_started("Git Stashes", path_detail(&event_root));
        self.runtime.spawn_blocking(move || {
            let result = list_stashes_with_short_hash_length(&git_root, short_hash_length);
            let event = match result {
                Ok(stashes) => UiEvent::GitStashesLoaded {
                    request_id,
                    root: event_root,
                    operation_root,
                    stashes,
                },
                Err(error) => UiEvent::GitStashesFailed {
                    request_id,
                    root: event_root,
                    operation_root,
                    error: error.to_string(),
                },
            };
            let _ = crate::ui_event_channel::send_ui_event(&tx, event);
        });
        true
    }

    pub(crate) fn spawn_restored_git_stashes_load(&mut self) -> bool {
        if !self.settings.git_enabled || !self.source_control_stashes_open {
            return false;
        }
        self.spawn_git_stashes_load()
    }

    fn begin_source_control_stashes_request(&mut self) -> Option<u64> {
        begin_source_control_stashes_request_state(
            &mut self.source_control_stashes_next_request_id,
            &mut self.source_control_stashes_active_request_id,
            &mut self.source_control_stashes_in_flight_request_id,
            &mut self.source_control_stashes_reload_queued,
        )
    }

    pub(crate) fn finish_source_control_stashes_request(&mut self, request_id: u64) -> bool {
        finish_source_control_load_request_state(
            &mut self.source_control_stashes_in_flight_request_id,
            &mut self.source_control_stashes_reload_queued,
            request_id,
        )
    }

    fn source_control_stash_operation_event_matches(
        &self,
        root: &Path,
        operation_root: &Path,
    ) -> bool {
        workspace_event_matches(&self.workspace.root, root)
            && self.source_control_git_operation_root_matches(operation_root)
    }

    fn source_control_stash_load_event_matches(
        &self,
        request_id: u64,
        root: &Path,
        operation_root: &Path,
    ) -> bool {
        self.source_control_stashes_open
            && background_request_matches(request_id, self.source_control_stashes_active_request_id)
            && workspace_event_matches(&self.workspace.root, root)
            && self.source_control_git_operation_root_matches(operation_root)
    }

    fn reload_git_stashes_panel_after_operation(&mut self) {
        if self.source_control_stashes_open {
            self.spawn_git_stashes_load();
        }
    }
}

fn begin_source_control_stashes_request_state(
    next_request_id: &mut u64,
    active_request_id: &mut u64,
    in_flight_request_id: &mut Option<u64>,
    reload_queued: &mut bool,
) -> Option<u64> {
    let request_id = reserve_source_control_stashes_request_id_state(
        next_request_id,
        active_request_id,
        *in_flight_request_id,
    );
    if in_flight_request_id.is_some() {
        *reload_queued = true;
        None
    } else {
        *in_flight_request_id = Some(request_id);
        Some(request_id)
    }
}

fn reserve_source_control_stashes_request_id_state(
    next_request_id: &mut u64,
    active_request_id: &mut u64,
    reserved_request_id: Option<u64>,
) -> u64 {
    let mut request_id = next_source_control_stashes_request_id(*next_request_id);
    if Some(request_id) == reserved_request_id {
        request_id = next_source_control_stashes_request_id(request_id);
    }
    *next_request_id = request_id;
    *active_request_id = request_id;
    request_id
}

fn next_source_control_stashes_request_id(current: u64) -> u64 {
    match current.wrapping_add(1) {
        0 => 1,
        request_id => request_id,
    }
}

pub(crate) fn git_stash_list_pending_status() -> String {
    "Loading git stashes".to_owned()
}

pub(crate) fn git_stash_list_success_status(count: usize) -> String {
    match count {
        0 => "No git stashes found".to_owned(),
        1 => "Loaded 1 git stash".to_owned(),
        _ => format!("Loaded {count} git stashes"),
    }
}

pub(crate) fn git_stash_list_failure_status(error: &str) -> String {
    source_control_stash_failure_status("Could not load git stashes: ", error)
}

pub(crate) fn git_stash_save_pending_status() -> String {
    "Saving git stash".to_owned()
}

pub(crate) fn git_stash_save_success_status(short_oid: &str) -> String {
    let short_oid = source_control_stash_hash_label(short_oid);
    let mut status = String::with_capacity("Saved git stash ()".len() + short_oid.as_str().len());
    status.push_str("Saved git stash (");
    status.push_str(short_oid.as_str());
    status.push(')');
    status
}

pub(crate) fn git_stash_save_failure_status(error: &str) -> String {
    source_control_stash_failure_status("Could not save git stash: ", error)
}

pub(crate) fn source_control_stash_message_from_inputs(
    stash_message: &str,
    commit_message: &str,
    use_commit_input: bool,
) -> String {
    let stash_message = stash_message.trim();
    if !stash_message.is_empty() {
        return stash_message.to_owned();
    }
    if use_commit_input {
        return commit_message.trim().to_owned();
    }
    String::new()
}

pub(crate) fn source_control_stashes_for_display(
    mut stashes: Vec<GitStashEntry>,
) -> Vec<GitStashEntry> {
    stashes.sort_by(|left, right| {
        left.index
            .cmp(&right.index)
            .then_with(|| left.short_oid.cmp(&right.short_oid))
            .then_with(|| left.message.cmp(&right.message))
    });
    stashes
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceControlStashSelectionIdentity {
    index: usize,
    short_oid: String,
    message: String,
}

pub(crate) fn source_control_stash_selected_identity(
    stashes: &[GitStashEntry],
    selected: usize,
) -> Option<SourceControlStashSelectionIdentity> {
    stashes
        .get(selected)
        .map(SourceControlStashSelectionIdentity::from_stash)
}

impl SourceControlStashSelectionIdentity {
    fn from_stash(stash: &GitStashEntry) -> Self {
        Self {
            index: stash.index,
            short_oid: stash.short_oid.clone(),
            message: stash.message.clone(),
        }
    }
}

pub(crate) fn source_control_stash_selection_after_reload(
    selected_stash: Option<&SourceControlStashSelectionIdentity>,
    previous_selected: usize,
    stashes: &[GitStashEntry],
) -> usize {
    if let Some(selected_stash) = selected_stash {
        if let Some(selected) = source_control_stash_unique_position(stashes, |stash| {
            stash.index == selected_stash.index
                && stash.short_oid == selected_stash.short_oid
                && stash.message == selected_stash.message
        }) {
            return selected;
        }

        if let Some(selected) = source_control_stash_unique_position(stashes, |stash| {
            !selected_stash.short_oid.is_empty() && stash.short_oid == selected_stash.short_oid
        }) {
            return selected;
        }
    }

    previous_selected.min(stashes.len().saturating_sub(1))
}

fn source_control_stash_unique_position(
    stashes: &[GitStashEntry],
    mut matches: impl FnMut(&GitStashEntry) -> bool,
) -> Option<usize> {
    let mut positions = stashes
        .iter()
        .enumerate()
        .filter_map(|(index, stash)| matches(stash).then_some(index));
    let position = positions.next()?;
    positions.next().is_none().then_some(position)
}

pub(crate) fn git_stash_apply_pending_status(index: usize) -> String {
    source_control_stash_operation_status("Applying", index)
}

pub(crate) fn git_stash_apply_success_status(index: usize) -> String {
    source_control_stash_operation_status("Applied", index)
}

pub(crate) fn git_stash_apply_failure_status(index: usize, error: &str) -> String {
    source_control_stash_operation_failure_status("apply", index, error)
}

pub(crate) fn git_stash_pop_pending_status(index: usize) -> String {
    source_control_stash_operation_status("Popping", index)
}

pub(crate) fn git_stash_pop_success_status(index: usize) -> String {
    source_control_stash_operation_status("Popped", index)
}

pub(crate) fn git_stash_pop_failure_status(index: usize, error: &str) -> String {
    source_control_stash_operation_failure_status("pop", index, error)
}

pub(crate) fn git_stash_drop_pending_status(index: usize) -> String {
    source_control_stash_operation_status("Dropping", index)
}

pub(crate) fn git_stash_drop_success_status(index: usize) -> String {
    source_control_stash_operation_status("Dropped", index)
}

pub(crate) fn git_stash_drop_failure_status(index: usize, error: &str) -> String {
    source_control_stash_operation_failure_status("drop", index, error)
}

#[cfg(test)]
pub(crate) fn source_control_stash_display_text(value: &str, fallback: &str) -> String {
    source_control_stash_display_label(value, SOURCE_CONTROL_STASH_DISPLAY_MAX_CHARS, fallback)
        .into_string()
}

fn source_control_stash_failure_status(prefix: &str, error: &str) -> String {
    let error = source_control_stash_status_detail_label(error);
    let mut status = String::with_capacity(prefix.len() + error.as_str().len());
    status.push_str(prefix);
    status.push_str(error.as_str());
    status
}

fn source_control_stash_operation_status(prefix: &str, index: usize) -> String {
    format!("{prefix} git stash {index}")
}

fn source_control_stash_operation_failure_status(
    action: &str,
    index: usize,
    error: &str,
) -> String {
    let error = source_control_stash_status_detail_label(error);
    format!("Could not {action} git stash {index}: {}", error.as_str())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceControlStashDisplayLabel<'a> {
    text: Cow<'a, str>,
}

impl<'a> SourceControlStashDisplayLabel<'a> {
    fn new(value: &'a str, max_chars: usize, fallback: &str) -> Self {
        Self {
            text: sanitized_display_label_cow(value, max_chars, fallback),
        }
    }

    fn as_str(&self) -> &str {
        self.text.as_ref()
    }

    #[cfg(test)]
    fn into_string(self) -> String {
        self.text.into_owned()
    }
}

#[cfg(test)]
fn source_control_stash_display_label<'a>(
    value: &'a str,
    max_chars: usize,
    fallback: &str,
) -> SourceControlStashDisplayLabel<'a> {
    SourceControlStashDisplayLabel::new(value, max_chars, fallback)
}

#[cfg(test)]
fn source_control_stash_status_detail(value: &str) -> String {
    source_control_stash_status_detail_label(value).into_string()
}

fn source_control_stash_status_detail_label(value: &str) -> SourceControlStashDisplayLabel<'_> {
    SourceControlStashDisplayLabel::new(
        value,
        SOURCE_CONTROL_STASH_STATUS_DETAIL_MAX_CHARS,
        "unknown error",
    )
}

#[cfg(test)]
fn source_control_stash_hash_display(value: &str) -> String {
    source_control_stash_hash_label(value).into_string()
}

fn source_control_stash_hash_label(value: &str) -> SourceControlStashDisplayLabel<'_> {
    SourceControlStashDisplayLabel::new(
        value,
        SOURCE_CONTROL_STASH_HASH_DISPLAY_MAX_CHARS,
        "unknown",
    )
}

#[cfg(test)]
mod tests {
    use crate::{
        app_state::{PendingFileReload, PendingFormatOnSave, QueuedFileReload},
        source_control_runtime::{
            source_control_app_for_test, source_control_mutation_restricted_status,
        },
        transient_state::PendingSourceControlStashSave,
    };
    use kuroya_core::{BufferId, GitStashEntry, TextBuffer};
    use std::path::PathBuf;

    fn assert_restricted_status(app: &crate::KuroyaApp, action: &str) {
        assert_eq!(
            app.status,
            source_control_mutation_restricted_status(action)
        );
    }

    fn git_stash_for_test(index: usize, message: &str) -> GitStashEntry {
        GitStashEntry {
            index,
            short_oid: format!("stash{index}"),
            message: message.to_owned(),
        }
    }

    fn git_stash_with_oid_for_test(index: usize, short_oid: &str, message: &str) -> GitStashEntry {
        GitStashEntry {
            index,
            short_oid: short_oid.to_owned(),
            message: message.to_owned(),
        }
    }

    fn insert_dirty_pending_format_on_save(
        app: &mut crate::KuroyaApp,
        id: BufferId,
        path: PathBuf,
    ) {
        let mut buffer = TextBuffer::from_text(id, Some(path.clone()), "fn main() {}\n".to_owned());
        let version = buffer.version();
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.pending_format_on_save.insert(
            id,
            PendingFormatOnSave {
                save_path: path.clone(),
                format_path: path,
                version,
                request_id: 1,
            },
        );
    }

    fn assert_stash_fragment_is_safe(value: &str, max_chars: usize) {
        assert!(!value.contains('\n'));
        assert!(!value.contains('\u{202e}'));
        assert!(value.chars().count() <= max_chars);
    }

    #[test]
    fn stash_display_fragments_sanitize_bound_and_fall_back() {
        let display = super::source_control_stash_display_text(
            &format!("first line\u{202e}\nsecond line {}", "x".repeat(400)),
            "No message",
        );

        assert!(display.contains("first line second line"));
        assert!(display.contains("..."));
        assert_stash_fragment_is_safe(&display, super::SOURCE_CONTROL_STASH_DISPLAY_MAX_CHARS);
        assert_eq!(
            super::source_control_stash_display_text("\u{202e}\n", "No message"),
            "No message"
        );
    }

    #[test]
    fn stash_status_and_hash_fragments_use_specific_fallbacks() {
        let status_detail = super::source_control_stash_status_detail(&format!(
            "first line\u{202e}\nsecond line {}",
            "x".repeat(400)
        ));
        let hash =
            super::source_control_stash_hash_display(&format!("1234\u{2066}\n{}", "a".repeat(120)));

        assert!(status_detail.contains("first line second line"));
        assert!(status_detail.contains("..."));
        assert_stash_fragment_is_safe(
            &status_detail,
            super::SOURCE_CONTROL_STASH_STATUS_DETAIL_MAX_CHARS,
        );
        assert!(hash.starts_with("1234 "));
        assert!(hash.contains("..."));
        assert_stash_fragment_is_safe(&hash, super::SOURCE_CONTROL_STASH_HASH_DISPLAY_MAX_CHARS);
        assert_eq!(
            super::source_control_stash_status_detail("\u{202e}\n"),
            "unknown error"
        );
        assert_eq!(
            super::source_control_stash_hash_display("\u{2066}\n"),
            "unknown"
        );
    }

    #[test]
    fn stashes_for_display_preserves_raw_fields_while_labels_are_sanitized() {
        let raw_oid = format!("12\u{202e}\n34{}", "a".repeat(120));
        let raw_message = format!("On main:\u{2066}\nwork{}", "b".repeat(320));
        let stashes = super::source_control_stashes_for_display(vec![
            git_stash_for_test(3, "later"),
            GitStashEntry {
                index: 1,
                short_oid: raw_oid.clone(),
                message: raw_message.clone(),
            },
        ]);

        assert_eq!(stashes[0].index, 1);
        assert_eq!(stashes[0].short_oid, raw_oid);
        assert_eq!(stashes[0].message, raw_message);

        let oid_label = super::source_control_stash_hash_display(&stashes[0].short_oid);
        let message_label =
            super::source_control_stash_display_text(&stashes[0].message, "No message");

        assert_stash_fragment_is_safe(
            &oid_label,
            super::SOURCE_CONTROL_STASH_HASH_DISPLAY_MAX_CHARS,
        );
        assert_stash_fragment_is_safe(
            &message_label,
            super::SOURCE_CONTROL_STASH_DISPLAY_MAX_CHARS,
        );
        assert_ne!(oid_label, stashes[0].short_oid);
        assert_ne!(message_label, stashes[0].message);
    }

    #[test]
    fn untrusted_workspace_rejects_stash_mutations() {
        let mut app = source_control_app_for_test(PathBuf::from("workspace"), false);

        app.save_git_stash_from_input();
        assert_restricted_status(&app, "creating a stash");

        app.spawn_git_stash_save("work in progress".to_owned());
        assert_restricted_status(&app, "creating a stash");

        app.apply_git_stash(0);
        assert_restricted_status(&app, "applying a stash");

        app.pop_git_stash(0);
        assert_restricted_status(&app, "popping a stash");

        app.drop_git_stash(0);
        assert_restricted_status(&app, "dropping a stash");

        app.pending_source_control_stash_save = Some(PendingSourceControlStashSave::Saving {
            message: "work in progress".to_owned(),
            ids: vec![1],
        });
        app.advance_pending_source_control_stash_after_save();
        assert_restricted_status(&app, "creating a stash");
        assert!(app.pending_source_control_stash_save.is_none());
    }

    #[test]
    fn pending_stash_waits_for_pending_format_on_save() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = source_control_app_for_test(root, true);
        insert_dirty_pending_format_on_save(&mut app, 7, path);
        app.pending_source_control_stash_save = Some(PendingSourceControlStashSave::Saving {
            message: "work in progress".to_owned(),
            ids: vec![7],
        });

        app.advance_pending_source_control_stash_after_save();

        assert!(matches!(
            app.pending_source_control_stash_save,
            Some(PendingSourceControlStashSave::Saving { ids, .. }) if ids == vec![7]
        ));
        assert!(!app.status.starts_with("Stash paused;"));
        assert!(app.active_async_tasks.is_empty());
        assert!(app.async_task_trace.is_empty());
    }

    #[test]
    fn pending_stash_pauses_on_clean_external_change_after_save() {
        let root = PathBuf::from("workspace");
        let main_path = root.join("src/main.rs");
        let lib_path = root.join("src/lib.rs");
        let mut app = source_control_app_for_test(root, true);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(main_path),
            "fn main() {}\n".to_owned(),
        ));
        app.buffers.push(TextBuffer::from_text(
            8,
            Some(lib_path),
            "pub fn helper() {}\n".to_owned(),
        ));
        app.external_change_buffers.insert(7);
        app.pending_source_control_stash_save = Some(PendingSourceControlStashSave::Saving {
            message: "work in progress".to_owned(),
            ids: vec![8, 7],
        });

        app.advance_pending_source_control_stash_after_save();

        assert!(matches!(
            app.pending_source_control_stash_save,
            Some(PendingSourceControlStashSave::Confirm {
                ref message,
                ref ids,
            }) if message == "work in progress" && ids == &vec![8, 7]
        ));
        assert_eq!(app.status, "Stash paused; 1 file changed on disk");
        assert!(app.active_async_tasks.is_empty());
        assert!(app.async_task_trace.is_empty());
    }

    #[test]
    fn pending_stash_pauses_on_clean_pending_reload_after_save() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = source_control_app_for_test(root, true);
        let buffer = TextBuffer::from_text(7, Some(path.clone()), "fn main() {}\n".to_owned());
        let version = buffer.version();
        app.buffers.push(buffer);
        app.in_flight_reloads.insert(
            7,
            PendingFileReload {
                request_id: 1,
                path,
                version,
                force_dirty: false,
            },
        );
        app.pending_source_control_stash_save = Some(PendingSourceControlStashSave::Saving {
            message: "work in progress".to_owned(),
            ids: vec![7],
        });

        app.advance_pending_source_control_stash_after_save();

        assert!(app.external_change_buffers.is_empty());
        assert!(matches!(
            app.pending_source_control_stash_save,
            Some(PendingSourceControlStashSave::Confirm {
                ref message,
                ref ids,
            }) if message == "work in progress" && ids == &vec![7]
        ));
        assert_eq!(app.status, "Stash paused; 1 file changed on disk");
        assert!(app.active_async_tasks.is_empty());
        assert!(app.async_task_trace.is_empty());
    }

    #[test]
    fn pending_stash_pauses_on_queued_clean_reload_after_save() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = source_control_app_for_test(root, true);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(path.clone()),
            "fn main() {}\n".to_owned(),
        ));
        app.queued_file_reloads.insert(
            7,
            QueuedFileReload {
                path,
                force_dirty: false,
            },
        );
        app.pending_source_control_stash_save = Some(PendingSourceControlStashSave::Saving {
            message: "work in progress".to_owned(),
            ids: vec![7],
        });

        app.advance_pending_source_control_stash_after_save();

        assert!(app.external_change_buffers.is_empty());
        assert!(matches!(
            app.pending_source_control_stash_save,
            Some(PendingSourceControlStashSave::Confirm {
                ref message,
                ref ids,
            }) if message == "work in progress" && ids == &vec![7]
        ));
        assert_eq!(app.status, "Stash paused; 1 file changed on disk");
        assert!(app.active_async_tasks.is_empty());
        assert!(app.async_task_trace.is_empty());
    }

    #[test]
    fn stash_reload_sorts_stashes_and_preserves_selected_stash_index() {
        let root = PathBuf::from("workspace");
        let mut app = source_control_app_for_test(root.clone(), true);
        app.source_control_stashes_open = true;
        app.source_control_stashes_active_request_id = 7;
        app.source_control_stashes = vec![
            git_stash_for_test(0, "latest"),
            git_stash_for_test(2, "selected"),
        ];
        app.source_control_stash_selected = 1;

        app.apply_git_stashes_loaded(
            7,
            root.clone(),
            root,
            vec![
                git_stash_for_test(3, "newer old"),
                git_stash_for_test(2, "selected refreshed"),
                git_stash_for_test(0, "latest refreshed"),
            ],
        );

        assert_eq!(
            app.source_control_stashes
                .iter()
                .map(|stash| stash.index)
                .collect::<Vec<_>>(),
            vec![0, 2, 3]
        );
        assert_eq!(app.source_control_stash_selected, 1);
        assert_eq!(app.source_control_stashes[1].message, "selected refreshed");
        assert_eq!(app.status, "Loaded 3 git stashes");
    }

    #[test]
    fn stash_reload_preserves_selected_identity_after_index_shift() {
        let root = PathBuf::from("workspace");
        let mut app = source_control_app_for_test(root.clone(), true);
        app.source_control_stashes_open = true;
        app.source_control_stashes_active_request_id = 9;
        app.source_control_stashes = vec![
            git_stash_with_oid_for_test(0, "latest", "latest"),
            git_stash_with_oid_for_test(4, "selected", "selected"),
        ];
        app.source_control_stash_selected = 1;

        app.apply_git_stashes_loaded(
            9,
            root.clone(),
            root,
            vec![
                git_stash_with_oid_for_test(0, "latest", "latest refreshed"),
                git_stash_with_oid_for_test(1, "new", "newer stash"),
                git_stash_with_oid_for_test(5, "selected", "selected after shift"),
            ],
        );

        assert_eq!(app.source_control_stash_selected, 2);
        assert_eq!(app.source_control_stashes[2].index, 5);
        assert_eq!(app.source_control_stashes[2].short_oid, "selected");
        assert_eq!(app.status, "Loaded 3 git stashes");
    }

    #[test]
    fn stash_reload_does_not_follow_ambiguous_selected_identity() {
        let previous = vec![
            git_stash_with_oid_for_test(0, "latest", "latest"),
            git_stash_with_oid_for_test(4, "selected", "selected"),
        ];
        let selected_identity = super::source_control_stash_selected_identity(&previous, 1);
        let stashes = super::source_control_stashes_for_display(vec![
            git_stash_with_oid_for_test(0, "latest", "latest refreshed"),
            git_stash_with_oid_for_test(5, "selected", "first duplicate"),
            git_stash_with_oid_for_test(6, "selected", "second duplicate"),
        ]);

        assert_eq!(
            super::source_control_stash_selection_after_reload(
                selected_identity.as_ref(),
                0,
                &stashes,
            ),
            0
        );
    }

    #[test]
    fn stash_reload_does_not_preserve_reused_index_without_identity_match() {
        let previous = vec![
            git_stash_with_oid_for_test(0, "latest", "latest"),
            git_stash_with_oid_for_test(4, "selected", "selected"),
        ];
        let selected_identity = super::source_control_stash_selected_identity(&previous, 1);
        let stashes = super::source_control_stashes_for_display(vec![
            git_stash_with_oid_for_test(0, "latest", "latest refreshed"),
            git_stash_with_oid_for_test(4, "different", "different stash"),
        ]);

        assert_eq!(
            super::source_control_stash_selection_after_reload(
                selected_identity.as_ref(),
                0,
                &stashes,
            ),
            0
        );
    }

    #[test]
    fn stash_reload_clamps_selection_when_previous_stash_is_gone() {
        let root = PathBuf::from("workspace");
        let mut app = source_control_app_for_test(root.clone(), true);
        app.source_control_stashes_open = true;
        app.source_control_stashes_active_request_id = 4;
        app.source_control_stashes = vec![
            git_stash_for_test(0, "latest"),
            git_stash_for_test(4, "selected"),
        ];
        app.source_control_stash_selected = 1;

        app.apply_git_stashes_loaded(
            4,
            root.clone(),
            root,
            vec![git_stash_for_test(0, "latest refreshed")],
        );

        assert_eq!(app.source_control_stash_selected, 0);
        assert_eq!(app.source_control_stashes[0].index, 0);
        assert_eq!(app.status, "Loaded 1 git stash");
    }

    #[test]
    fn stash_load_rejects_related_workspace_roots() {
        let root = PathBuf::from("workspace");
        let related_root = root.join("nested");
        let mut app = source_control_app_for_test(root.clone(), true);
        app.source_control_stashes_open = true;
        app.source_control_stashes_active_request_id = 7;
        app.source_control_stashes = vec![git_stash_for_test(0, "unchanged")];
        app.status = "unchanged".to_owned();

        app.apply_git_stashes_loaded(
            7,
            related_root.clone(),
            root.clone(),
            vec![git_stash_for_test(1, "ignored")],
        );
        app.apply_git_stashes_failed(7, related_root, root, "ignored".to_owned());

        assert_eq!(app.source_control_stashes.len(), 1);
        assert_eq!(app.source_control_stashes[0].index, 0);
        assert_eq!(app.source_control_stashes[0].message, "unchanged");
        assert_eq!(app.status, "unchanged");
    }

    #[test]
    fn stash_load_request_ids_wrap_to_nonzero_after_max() {
        let mut next_request_id = u64::MAX;
        let mut active_request_id = u64::MAX;
        let mut in_flight = None;
        let mut queued = false;

        assert_eq!(
            super::begin_source_control_stashes_request_state(
                &mut next_request_id,
                &mut active_request_id,
                &mut in_flight,
                &mut queued,
            ),
            Some(1)
        );

        assert_eq!(next_request_id, 1);
        assert_eq!(active_request_id, 1);
        assert_eq!(in_flight, Some(1));
        assert!(!queued);
    }

    #[test]
    fn queued_stash_load_request_ids_skip_current_in_flight_after_wrap() {
        let mut next_request_id = u64::MAX;
        let mut active_request_id = 1;
        let mut in_flight = Some(1);
        let mut queued = false;

        assert_eq!(
            super::begin_source_control_stashes_request_state(
                &mut next_request_id,
                &mut active_request_id,
                &mut in_flight,
                &mut queued,
            ),
            None
        );

        assert_eq!(next_request_id, 2);
        assert_eq!(active_request_id, 2);
        assert_eq!(in_flight, Some(1));
        assert!(queued);
    }

    #[test]
    fn stale_stash_load_after_wrapped_queued_reload_does_not_apply() {
        let root = PathBuf::from("workspace");
        let mut app = source_control_app_for_test(root.clone(), true);
        app.source_control_stashes_open = true;
        app.source_control_stashes_next_request_id = u64::MAX;
        app.source_control_stashes_active_request_id = 1;
        app.source_control_stashes_in_flight_request_id = Some(1);
        app.source_control_stashes = vec![git_stash_for_test(0, "current")];
        app.status = "unchanged".to_owned();

        assert!(!app.spawn_git_stashes_load());
        app.status = "unchanged".to_owned();
        app.apply_git_stashes_loaded(1, root.clone(), root, vec![git_stash_for_test(1, "stale")]);

        assert_eq!(app.source_control_stashes_next_request_id, 2);
        assert_eq!(app.source_control_stashes_active_request_id, 2);
        assert_eq!(app.source_control_stashes_in_flight_request_id, Some(1));
        assert!(app.source_control_stashes_reload_queued);
        assert_eq!(
            app.source_control_stashes,
            vec![git_stash_for_test(0, "current")]
        );
        assert_eq!(app.status, "unchanged");
    }

    #[test]
    fn stash_apply_failure_refreshes_index_and_git_status() {
        let root = PathBuf::from("workspace");
        let mut app = source_control_app_for_test(root.clone(), true);

        app.apply_git_stash_apply_failed(
            root.clone(),
            root,
            2,
            "conflict while applying".to_owned(),
        );

        assert_eq!(
            app.status,
            "Could not apply git stash 2: conflict while applying"
        );
        assert_eq!(app.workspace_index_active_request_id, 1);
        assert_eq!(app.workspace_index_in_flight_request_id, Some(1));
        assert!(!app.workspace_index_refresh_queued);
        assert_eq!(app.git_scan_active_request_id, 1);
        assert_eq!(app.git_scan_in_flight_request_id, Some(1));
        assert!(!app.git_scan_refresh_queued);
    }

    #[test]
    fn stash_pop_failure_refreshes_index_and_git_status() {
        let root = PathBuf::from("workspace");
        let mut app = source_control_app_for_test(root.clone(), true);

        app.apply_git_stash_pop_failed(root.clone(), root, 1, "conflict while popping".to_owned());

        assert_eq!(
            app.status,
            "Could not pop git stash 1: conflict while popping"
        );
        assert_eq!(app.workspace_index_active_request_id, 1);
        assert_eq!(app.workspace_index_in_flight_request_id, Some(1));
        assert!(!app.workspace_index_refresh_queued);
        assert_eq!(app.git_scan_active_request_id, 1);
        assert_eq!(app.git_scan_in_flight_request_id, Some(1));
        assert!(!app.git_scan_refresh_queued);
    }

    #[test]
    fn stale_stash_failure_does_not_refresh_current_workspace() {
        let root = PathBuf::from("workspace");
        let mut app = source_control_app_for_test(root.clone(), true);
        app.status = "unchanged".to_owned();

        app.apply_git_stash_apply_failed(
            PathBuf::from("other"),
            PathBuf::from("other"),
            0,
            "ignored".to_owned(),
        );
        app.apply_git_stash_pop_failed(
            PathBuf::from("other"),
            PathBuf::from("other"),
            0,
            "ignored".to_owned(),
        );
        app.apply_git_stash_apply_failed(
            root.clone(),
            root.join("old-repo"),
            0,
            "ignored".to_owned(),
        );

        assert_eq!(app.status, "unchanged");
        assert_eq!(app.workspace_index_active_request_id, 0);
        assert_eq!(app.workspace_index_in_flight_request_id, None);
        assert!(!app.workspace_index_refresh_queued);
        assert_eq!(app.git_scan_active_request_id, 0);
        assert_eq!(app.git_scan_in_flight_request_id, None);
        assert!(!app.git_scan_refresh_queued);
    }

    #[test]
    fn stash_failure_queues_refresh_when_index_and_git_scan_are_in_flight() {
        let root = PathBuf::from("workspace");
        let mut app = source_control_app_for_test(root.clone(), true);
        app.workspace_index_next_request_id = 4;
        app.workspace_index_active_request_id = 4;
        app.workspace_index_in_flight_request_id = Some(4);
        app.git_scan_next_request_id = 8;
        app.git_scan_active_request_id = 8;
        app.git_scan_in_flight_request_id = Some(8);

        app.apply_git_stash_apply_failed(
            root.clone(),
            root,
            3,
            "conflict while applying".to_owned(),
        );

        assert_eq!(
            app.status,
            "Could not apply git stash 3: conflict while applying"
        );
        assert_eq!(app.workspace_index_active_request_id, 5);
        assert_eq!(app.workspace_index_in_flight_request_id, Some(4));
        assert!(app.workspace_index_refresh_queued);
        assert_eq!(app.git_scan_active_request_id, 9);
        assert_eq!(app.git_scan_in_flight_request_id, Some(8));
        assert!(app.git_scan_refresh_queued);
    }

    #[test]
    fn stash_apply_failure_refreshes_open_stash_panel_without_hiding_failure_status() {
        let root = PathBuf::from("workspace");
        let mut app = source_control_app_for_test(root.clone(), true);
        app.source_control_stashes_open = true;

        app.apply_git_stash_apply_failed(root.clone(), root, 2, "missing stash".to_owned());

        assert_eq!(app.source_control_stashes_active_request_id, 1);
        assert_eq!(app.source_control_stashes_in_flight_request_id, Some(1));
        assert!(!app.source_control_stashes_reload_queued);
        assert_eq!(app.status, "Could not apply git stash 2: missing stash");
    }

    #[test]
    fn stash_drop_failure_queues_stash_panel_reload_when_list_load_is_in_flight() {
        let root = PathBuf::from("workspace");
        let mut app = source_control_app_for_test(root.clone(), true);
        app.source_control_stashes_open = true;
        app.source_control_stashes_next_request_id = 4;
        app.source_control_stashes_active_request_id = 4;
        app.source_control_stashes_in_flight_request_id = Some(4);

        app.apply_git_stash_drop_failed(root.clone(), root, 3, "missing stash".to_owned());

        assert_eq!(app.source_control_stashes_active_request_id, 5);
        assert_eq!(app.source_control_stashes_in_flight_request_id, Some(4));
        assert!(app.source_control_stashes_reload_queued);
        assert_eq!(app.status, "Could not drop git stash 3: missing stash");
    }
}
