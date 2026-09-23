use crate::{
    KuroyaApp,
    file_runtime::file_path_open_buffer_or_known_openable,
    git_diff_state::DiffBufferSource,
    large_file_mode::buffer_uses_large_file_mode,
    navigation_targets::diff_hunk_index_at_buffer_line,
    source_control_diff_runtime::{SourceControlDiffOpenJob, SourceControlDiffText},
    source_control_history_runtime::source_control_history_commit_is_uncommitted,
    source_control_patch_runtime::{
        SourceControlPatchCopyInput, SourceControlPatchCopyRequest, SourceControlPatchText,
    },
    virtual_diff_runtime::VirtualDiffOpenJob,
    virtual_revision_runtime::{VirtualRevisionJump, VirtualRevisionOpenJob},
};
use eframe::egui::Context;
use kuroya_core::{
    BufferId, DiffOptions, GitChangeStage, GitCommitSummary, GitFileStatus, GitStashEntry,
    LanguageId, TextBuffer, diff_max_file_size_bytes,
};
use std::path::{Path, PathBuf};

mod status;

pub(crate) use status::{
    accessible_diff_label, diff_buffer_display_kind, diff_buffer_display_label,
    diff_label_for_path, source_control_accessible_diff_too_large_status,
    source_control_diff_base_open_missing_status, source_control_diff_base_open_unavailable_status,
    source_control_diff_buffer_patch_copy_empty_status,
    source_control_diff_buffer_patch_copy_success_status,
    source_control_diff_buffer_patch_copy_too_large_status,
    source_control_diff_buffer_patch_copy_unavailable_status,
    source_control_diff_hunk_base_open_missing_hunk_status,
    source_control_diff_hunk_base_open_no_hunk_status,
    source_control_diff_hunk_base_open_success_status,
    source_control_diff_hunk_base_open_unavailable_status,
    source_control_diff_hunk_discard_stale_status, source_control_diff_hunk_identity_stale_status,
    source_control_diff_hunk_patch_copy_empty_status,
    source_control_diff_hunk_patch_copy_no_hunk_status,
    source_control_diff_hunk_patch_copy_success_status,
    source_control_diff_hunk_patch_copy_unavailable_status,
    source_control_diff_hunk_source_open_missing_hunk_status,
    source_control_diff_hunk_source_open_missing_status,
    source_control_diff_hunk_source_open_no_hunk_status,
    source_control_diff_hunk_source_open_success_status,
    source_control_diff_hunk_source_open_unavailable_status,
    source_control_diff_refresh_unavailable_status,
    source_control_diff_source_open_unavailable_status,
    source_control_head_revision_failure_status, source_control_head_revision_missing_status,
    source_control_hunk_diff_open_missing_status, source_control_hunk_diff_open_success_status,
    source_control_index_revision_failure_status, source_control_index_revision_missing_status,
    source_control_open_all_stage_empty_status, source_control_open_all_stage_success_status,
};

#[cfg(test)]
pub(crate) use status::{
    join_unified_patches, source_control_all_patch_copy_empty_status,
    source_control_all_patch_copy_failure_status, source_control_all_patch_copy_success_status,
    source_control_commit_patch_copy_empty_status, source_control_commit_patch_copy_failure_status,
    source_control_commit_patch_copy_success_status, source_control_hunk_patch_copy_empty_status,
    source_control_hunk_patch_copy_failure_status, source_control_hunk_patch_copy_success_status,
    source_control_patch_copy_empty_status, source_control_patch_copy_failure_status,
    source_control_patch_copy_success_status, source_control_stage_patch_copy_empty_status,
    source_control_stage_patch_copy_failure_status, source_control_stage_patch_copy_success_status,
    source_control_stash_patch_copy_empty_status, source_control_stash_patch_copy_failure_status,
    source_control_stash_patch_copy_success_status,
};

#[cfg(test)]
use status::{
    SOURCE_CONTROL_DIFF_STATUS_MAX_CHARS, accessible_diff_label_cow,
    source_control_diff_display_label_cow,
};
use status::{
    source_control_diff_display_label, source_control_diff_path_label,
    source_control_diff_status_text,
};

impl KuroyaApp {
    pub(crate) fn buffer_file_or_diff_source_path(&self, id: BufferId) -> Option<PathBuf> {
        self.buffer(id)
            .and_then(|buffer| buffer.path().cloned())
            .or_else(|| {
                self.diff_buffer_sources
                    .get(&id)
                    .map(|source| source.path.clone())
            })
    }

    pub(crate) fn active_file_or_diff_source_path(&mut self, action: &str) -> Option<PathBuf> {
        let Some(id) = self.active else {
            self.status = source_control_diff_status_text(format!("No active file to {action}"));
            return None;
        };
        let Some(path) = self.buffer_file_or_diff_source_path(id) else {
            self.status =
                source_control_diff_status_text(format!("No file-backed buffer to {action}"));
            return None;
        };
        Some(path)
    }

    pub(crate) fn open_active_file_changes(&mut self) {
        let Some(id) = self.active else {
            self.status = "No active file for changes".to_owned();
            return;
        };
        if self.buffer(id).and_then(|buffer| buffer.path()).is_some() {
            self.open_buffer_changes(id);
            return;
        }

        let Some(path) = self.buffer_file_or_diff_source_path(id) else {
            self.status = "No file-backed buffer for changes".to_owned();
            return;
        };
        self.open_file_changes(path);
    }

    pub(crate) fn open_active_file_staged_changes(&mut self) {
        let Some(path) = self.active_file_or_diff_source_path("open staged changes") else {
            return;
        };
        self.open_staged_file_changes(path);
    }

    pub(crate) fn open_active_file_head_changes(&mut self) {
        let Some(id) = self.active else {
            self.status = "No active file to compare with HEAD".to_owned();
            return;
        };
        let path = self.buffer_file_or_diff_source_path(id);
        let Some(path) = path else {
            self.status = "No file-backed buffer to compare with HEAD".to_owned();
            return;
        };
        self.open_file_head_changes(path);
    }

    pub(crate) fn open_active_file_head_revision(&mut self) {
        let Some(path) = self.active_file_or_diff_source_path("open file at HEAD") else {
            return;
        };
        self.open_file_head_revision(path);
    }

    pub(crate) fn open_active_file_index_revision(&mut self) {
        let Some(path) = self.active_file_or_diff_source_path("open file at Index") else {
            return;
        };
        self.open_file_index_revision(path);
    }

    pub(crate) fn open_active_accessible_diff_viewer(&mut self) {
        let Some(id) = self.active else {
            self.status = "No active diff buffer".to_owned();
            return;
        };
        let Some(label) = self.virtual_buffer_labels.get(&id).cloned() else {
            self.status = "Open a diff buffer before opening the accessible diff viewer".to_owned();
            return;
        };
        let Some(buffer) = self.buffer(id) else {
            self.status = "No active diff buffer".to_owned();
            return;
        };
        if buffer.language() != LanguageId::Diff {
            self.status = "Open a diff buffer before opening the accessible diff viewer".to_owned();
            return;
        }
        if buffer_uses_large_file_mode(buffer) {
            self.status = source_control_accessible_diff_too_large_status(&label, buffer);
            return;
        }

        let diff = buffer.text();
        if diff.trim().is_empty() {
            self.status = "Active diff is empty".to_owned();
            return;
        }

        let source = self.diff_buffer_sources.get(&id).cloned();
        let target = source
            .as_ref()
            .map(|source| source_control_diff_path_label(&source.path))
            .unwrap_or_else(|| source_control_diff_display_label(&label));
        self.open_virtual_diff_buffer(
            accessible_diff_label(&label),
            diff,
            target.into_owned(),
            "accessible diff",
            source,
        );
    }

    pub(crate) fn open_buffer_changes(&mut self, id: BufferId) {
        let Some(buffer) = self.buffer(id) else {
            self.status = "No buffer for changes".to_owned();
            return;
        };
        let Some(path) = buffer.path().cloned() else {
            self.status = "No file-backed buffer for changes".to_owned();
            return;
        };
        self.spawn_source_control_diff_open(SourceControlDiffOpenJob::worktree(
            path.clone(),
            self.source_control_diff_text(path),
            None,
        ));
    }

    pub(crate) fn open_file_changes(&mut self, path: PathBuf) {
        let request_id = self.reserve_source_control_diff_open_request_id();
        self.open_file_changes_with_request_id(path, request_id);
    }

    fn open_file_changes_with_request_id(&mut self, path: PathBuf, request_id: u64) {
        let text = self.source_control_diff_text(path.clone());
        self.spawn_source_control_diff_open_with_request_id(
            SourceControlDiffOpenJob::worktree(path, text, None),
            request_id,
        );
    }

    pub(crate) fn open_all_file_changes(&mut self) {
        // Borrow the sorted snapshot entries and lift only the fields the
        // loop needs, so opening changes does not clone the entry list.
        let entries = self
            .git
            .entries_slice_sorted()
            .iter()
            .map(|entry| (entry.path.clone(), entry.stage))
            .collect::<Vec<_>>();
        if entries.is_empty() {
            self.status = "No source control changes".to_owned();
            return;
        }

        let count = entries.len();
        let request_id = self.reserve_source_control_diff_open_request_id();
        for (path, stage) in entries {
            match stage {
                GitChangeStage::Staged => {
                    self.open_staged_file_changes_with_request_id(path, request_id)
                }
                GitChangeStage::Unstaged => {
                    self.open_file_changes_with_request_id(path, request_id)
                }
            }
        }
        self.status = source_control_diff_status_text(format!("Opened changes for {count} files"));
    }

    pub(crate) fn open_all_unstaged_file_changes(&mut self) {
        self.open_all_file_changes_for_stage(GitChangeStage::Unstaged);
    }

    pub(crate) fn open_all_staged_file_changes(&mut self) {
        self.open_all_file_changes_for_stage(GitChangeStage::Staged);
    }

    fn open_all_file_changes_for_stage(&mut self, stage: GitChangeStage) {
        // Borrow the sorted snapshot entries and keep only the matching
        // paths, so stage-scoped opens do not clone the full entry list.
        let paths = self
            .git
            .entries_slice_sorted()
            .iter()
            .filter(|entry| entry.stage == stage)
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        let mut count = 0usize;
        let mut request_id = None;
        for path in paths {
            let request_id = *request_id
                .get_or_insert_with(|| self.reserve_source_control_diff_open_request_id());
            match stage {
                GitChangeStage::Staged => {
                    self.open_staged_file_changes_with_request_id(path, request_id)
                }
                GitChangeStage::Unstaged => {
                    self.open_file_changes_with_request_id(path, request_id)
                }
            }
            count += 1;
        }

        if count == 0 {
            self.status = source_control_open_all_stage_empty_status(stage);
        } else {
            self.status = source_control_open_all_stage_success_status(stage, count);
        }
    }

    pub(crate) fn open_staged_file_changes(&mut self, path: PathBuf) {
        let request_id = self.reserve_source_control_diff_open_request_id();
        self.open_staged_file_changes_with_request_id(path, request_id);
    }

    fn open_staged_file_changes_with_request_id(&mut self, path: PathBuf, request_id: u64) {
        self.spawn_source_control_diff_open_with_request_id(
            SourceControlDiffOpenJob::staged(path, None),
            request_id,
        );
    }

    pub(crate) fn copy_file_patch(&mut self, _ctx: &Context, path: PathBuf, stage: GitChangeStage) {
        let input = self.source_control_patch_copy_input(path.clone(), stage);
        self.spawn_source_control_patch_copy(
            SourceControlPatchCopyRequest::File { path, stage },
            vec![input],
        );
    }

    pub(crate) fn copy_all_changes_patch(&mut self, _ctx: &Context) {
        let inputs = self.source_control_patch_copy_inputs(self.git.entries_slice_sorted());
        self.spawn_source_control_patch_copy(SourceControlPatchCopyRequest::All, inputs);
    }

    pub(crate) fn copy_stage_patch(&mut self, _ctx: &Context, stage: GitChangeStage) {
        // Borrow the sorted snapshot entries and clone only the entries in
        // the requested stage.
        let entries = self
            .git
            .entries_slice_sorted()
            .iter()
            .filter(|entry| entry.stage == stage)
            .cloned()
            .collect::<Vec<_>>();
        let inputs = self.source_control_patch_copy_inputs(&entries);
        self.spawn_source_control_patch_copy(
            SourceControlPatchCopyRequest::Stage { stage },
            inputs,
        );
    }

    pub(crate) fn copy_active_file_patch(&mut self, ctx: &Context, stage: GitChangeStage) {
        let action = match stage {
            GitChangeStage::Staged => "copy staged patch",
            GitChangeStage::Unstaged => "copy patch",
        };
        let Some(path) = self.active_file_or_diff_source_path(action) else {
            return;
        };
        self.copy_file_patch(ctx, path, stage);
    }

    pub(crate) fn copy_source_control_hunk_patch(
        &mut self,
        _ctx: &Context,
        path: PathBuf,
        stage: GitChangeStage,
        hunk_index: usize,
    ) {
        let input = self.source_control_patch_copy_input(path.clone(), stage);
        self.spawn_source_control_patch_copy(
            SourceControlPatchCopyRequest::Hunk {
                path,
                stage,
                hunk_index,
            },
            vec![input],
        );
    }

    pub(crate) fn open_source_control_hunk_diff(
        &mut self,
        path: PathBuf,
        stage: GitChangeStage,
        hunk_index: usize,
    ) {
        match stage {
            GitChangeStage::Staged => self.spawn_source_control_diff_open(
                SourceControlDiffOpenJob::staged(path, Some(hunk_index)),
            ),
            GitChangeStage::Unstaged => {
                let text = self.source_control_diff_text(path.clone());
                self.spawn_source_control_diff_open(SourceControlDiffOpenJob::worktree(
                    path,
                    text,
                    Some(hunk_index),
                ));
            }
        };
    }

    pub(crate) fn open_file_head_changes(&mut self, path: PathBuf) {
        let text = self.source_control_diff_text(path.clone());
        self.spawn_source_control_diff_open(SourceControlDiffOpenJob::head(path, text));
    }

    pub(crate) fn open_file_head_revision(&mut self, path: PathBuf) {
        self.spawn_virtual_revision_open(VirtualRevisionOpenJob::head(path, None));
    }

    pub(crate) fn open_file_index_revision(&mut self, path: PathBuf) {
        self.spawn_virtual_revision_open(VirtualRevisionOpenJob::index(path, None));
    }

    pub(crate) fn open_commit_changes(&mut self, commit: GitCommitSummary) {
        if source_control_history_commit_is_uncommitted(&commit) {
            self.open_all_file_changes();
            return;
        }

        self.spawn_virtual_diff_open(VirtualDiffOpenJob::git_commit(commit));
    }

    pub(crate) fn copy_commit_patch(&mut self, ctx: &Context, commit: &GitCommitSummary) {
        if source_control_history_commit_is_uncommitted(commit) {
            self.copy_all_changes_patch(ctx);
            return;
        }

        self.spawn_source_control_patch_copy(
            SourceControlPatchCopyRequest::Commit {
                commit: commit.clone(),
            },
            Vec::new(),
        );
    }

    pub(crate) fn open_stash_changes(&mut self, stash: GitStashEntry) {
        self.spawn_virtual_diff_open(VirtualDiffOpenJob::git_stash(stash));
    }

    pub(crate) fn copy_stash_patch(&mut self, _ctx: &Context, stash: &GitStashEntry) {
        self.spawn_source_control_patch_copy(
            SourceControlPatchCopyRequest::Stash {
                stash: stash.clone(),
            },
            Vec::new(),
        );
    }

    pub(crate) fn can_copy_diff_buffer_patch(&self, id: BufferId) -> bool {
        self.buffer(id)
            .is_some_and(|buffer| buffer.language() == LanguageId::Diff)
            && self.virtual_buffer_labels.contains_key(&id)
    }

    pub(crate) fn copy_active_diff_patch(&mut self, ctx: &Context) {
        let Some(id) = self.active else {
            self.status = source_control_diff_buffer_patch_copy_unavailable_status();
            return;
        };
        self.copy_diff_buffer_patch(ctx, id);
    }

    pub(crate) fn copy_active_diff_hunk_patch(&mut self, ctx: &Context) {
        let Some(id) = self.active else {
            self.status = source_control_diff_hunk_patch_copy_unavailable_status();
            return;
        };
        self.copy_diff_buffer_hunk_patch(ctx, id);
    }

    pub(crate) fn refresh_active_diff(&mut self) {
        let Some(id) = self.active else {
            self.status = source_control_diff_refresh_unavailable_status();
            return;
        };
        self.refresh_diff_buffer(id);
    }

    pub(crate) fn refresh_diff_buffer(&mut self, id: BufferId) {
        let Some(source) = self.diff_buffer_sources.get(&id).cloned() else {
            self.status = source_control_diff_refresh_unavailable_status();
            return;
        };
        self.set_active_buffer(id);
        match source.hunk_stage {
            _ if source.saved_buffer_id.is_some() => {
                let Some(buffer_id) =
                    self.saved_diff_buffer_id(source.saved_buffer_id, &source.path)
                else {
                    self.status = source_control_diff_status_text(format!(
                        "No open buffer to compare with saved {}",
                        source_control_diff_path_label(&source.path)
                    ));
                    return;
                };
                self.open_buffer_saved_comparison(buffer_id);
            }
            _ if source.base_path.is_some() => {
                if let Some(base_path) = source.base_path {
                    self.open_file_comparison(base_path, source.path);
                }
            }
            Some(GitChangeStage::Staged) => self.open_staged_file_changes(source.path),
            Some(GitChangeStage::Unstaged) => self.open_file_changes(source.path),
            None => self.open_file_head_changes(source.path),
        }
    }

    pub(crate) fn copy_diff_buffer_patch(&mut self, ctx: &Context, id: BufferId) {
        let Some(label) = self
            .can_copy_diff_buffer_patch(id)
            .then(|| self.buffer_label(id))
        else {
            self.status = source_control_diff_buffer_patch_copy_unavailable_status();
            return;
        };
        let Some(buffer) = self.buffer(id) else {
            self.status = source_control_diff_buffer_patch_copy_unavailable_status();
            return;
        };
        if buffer_uses_large_file_mode(buffer) {
            self.status = source_control_diff_buffer_patch_copy_too_large_status(&label, buffer);
            return;
        }

        let diff = buffer.text();
        if diff.trim().is_empty() {
            self.status = source_control_diff_buffer_patch_copy_empty_status(&label);
            return;
        }

        ctx.copy_text(diff);
        self.status = source_control_diff_buffer_patch_copy_success_status(&label);
    }

    pub(crate) fn copy_diff_buffer_hunk_patch(&mut self, ctx: &Context, id: BufferId) {
        let Some(label) = self
            .can_copy_diff_buffer_patch(id)
            .then(|| self.buffer_label(id))
        else {
            self.status = source_control_diff_hunk_patch_copy_unavailable_status();
            return;
        };
        let Some(buffer) = self.buffer(id) else {
            self.status = source_control_diff_hunk_patch_copy_unavailable_status();
            return;
        };
        let line = buffer.cursor_position().line + 1;
        let Some(hunk_index) = diff_hunk_index_at_buffer_line(buffer, line) else {
            self.status = source_control_diff_hunk_patch_copy_no_hunk_status(&label, line);
            return;
        };
        let Some(patch) = hunk_patch_from_diff_buffer(buffer, hunk_index) else {
            self.status = source_control_diff_hunk_patch_copy_empty_status(&label, hunk_index);
            return;
        };

        ctx.copy_text(patch);
        self.status = source_control_diff_hunk_patch_copy_success_status(&label, hunk_index);
    }

    pub(crate) fn open_active_diff_source_file(&mut self) {
        let Some(id) = self.active else {
            self.status = source_control_diff_source_open_unavailable_status();
            return;
        };
        self.open_diff_source_file(id);
    }

    pub(crate) fn open_active_diff_base_file(&mut self) {
        let Some(id) = self.active else {
            self.status = source_control_diff_base_open_unavailable_status();
            return;
        };
        self.open_diff_base_file(id);
    }

    pub(crate) fn open_active_diff_hunk_base(&mut self) {
        let Some(id) = self.active else {
            self.status = source_control_diff_hunk_base_open_unavailable_status();
            return;
        };
        let Some(source) = self.diff_buffer_sources.get(&id).cloned() else {
            self.status = source_control_diff_hunk_base_open_unavailable_status();
            return;
        };
        let label = self.buffer_label(id);
        let Some(buffer) = self.buffer(id) else {
            self.status = source_control_diff_hunk_base_open_unavailable_status();
            return;
        };
        let line = buffer.cursor_position().line + 1;
        let Some(hunk_index) = diff_hunk_index_at_buffer_line(buffer, line) else {
            self.status = source_control_diff_hunk_base_open_no_hunk_status(&label, line);
            return;
        };
        let Some(base_line) = hunk_original_start_line_in_diff_buffer(buffer, hunk_index) else {
            self.status =
                source_control_diff_hunk_base_open_missing_hunk_status(&label, hunk_index);
            return;
        };

        if source.saved_buffer_id.is_some() {
            self.spawn_virtual_revision_open(VirtualRevisionOpenJob::saved(
                source.path,
                Some(VirtualRevisionJump {
                    line: base_line,
                    column: 1,
                    label,
                    hunk_index,
                }),
            ));
            return;
        }

        if let Some(base_path) = source.base_path.clone() {
            if !file_path_open_buffer_or_known_openable(
                &self.buffers,
                self.index.files(),
                &base_path,
                Path::exists,
            ) {
                self.status = source_control_diff_base_open_missing_status(&base_path);
                return;
            }
            self.open_file_at_known_openable(base_path.clone(), base_line, 1);
            self.status = source_control_diff_hunk_base_open_success_status(
                &label, &base_path, hunk_index, base_line,
            );
            return;
        }

        let jump = Some(VirtualRevisionJump {
            line: base_line,
            column: 1,
            label,
            hunk_index,
        });
        let job = match source.hunk_stage {
            Some(GitChangeStage::Unstaged) => VirtualRevisionOpenJob::index(source.path, jump),
            Some(GitChangeStage::Staged) | None => VirtualRevisionOpenJob::head(source.path, jump),
        };
        self.spawn_virtual_revision_open(job);
    }

    pub(crate) fn open_diff_base_file(&mut self, id: BufferId) {
        let Some(source) = self.diff_buffer_sources.get(&id).cloned() else {
            self.status = source_control_diff_base_open_unavailable_status();
            return;
        };
        if source.saved_buffer_id.is_some() {
            self.spawn_virtual_revision_open(VirtualRevisionOpenJob::saved(source.path, None));
            return;
        }
        if let Some(base_path) = source.base_path {
            if !file_path_open_buffer_or_known_openable(
                &self.buffers,
                self.index.files(),
                &base_path,
                Path::exists,
            ) {
                self.status = source_control_diff_base_open_missing_status(&base_path);
                return;
            }
            self.spawn_open_file(base_path);
            return;
        }
        match source.hunk_stage {
            Some(GitChangeStage::Unstaged) => self.open_file_index_revision(source.path),
            Some(GitChangeStage::Staged) | None => self.open_file_head_revision(source.path),
        }
    }

    pub(crate) fn open_diff_source_file(&mut self, id: BufferId) {
        let Some(source) = self.diff_buffer_sources.get(&id).cloned() else {
            self.status = source_control_diff_source_open_unavailable_status();
            return;
        };
        if !file_path_open_buffer_or_known_openable(
            &self.buffers,
            self.index.files(),
            &source.path,
            Path::exists,
        ) {
            self.status = source_control_diff_hunk_source_open_missing_status(&source.path);
            return;
        }
        self.spawn_open_file(source.path);
    }

    pub(crate) fn open_active_diff_hunk_source(&mut self) {
        let Some(id) = self.active else {
            self.status = source_control_diff_hunk_source_open_unavailable_status();
            return;
        };
        let Some(source) = self.diff_buffer_sources.get(&id).cloned() else {
            self.status = source_control_diff_hunk_source_open_unavailable_status();
            return;
        };
        if !file_path_open_buffer_or_known_openable(
            &self.buffers,
            self.index.files(),
            &source.path,
            Path::exists,
        ) {
            self.status = source_control_diff_hunk_source_open_missing_status(&source.path);
            return;
        }

        let label = self.buffer_label(id);
        let Some(buffer) = self.buffer(id) else {
            self.status = source_control_diff_hunk_source_open_unavailable_status();
            return;
        };
        let line = buffer.cursor_position().line + 1;
        let Some(hunk_index) = diff_hunk_index_at_buffer_line(buffer, line) else {
            self.status = source_control_diff_hunk_source_open_no_hunk_status(&label, line);
            return;
        };
        let Some(source_line) = hunk_modified_start_line_in_diff_buffer(buffer, hunk_index) else {
            self.status =
                source_control_diff_hunk_source_open_missing_hunk_status(&label, hunk_index);
            return;
        };

        self.open_file_at_known_openable(source.path.clone(), source_line, 1);
        self.status = source_control_diff_hunk_source_open_success_status(
            &label,
            &source.path,
            hunk_index,
            source_line,
        );
    }

    pub(crate) fn focus_diff_hunk(
        &mut self,
        id: BufferId,
        hunk_index: usize,
    ) -> Option<(String, usize)> {
        let (label, line) = {
            let buffer = self.buffer(id)?;
            let line = hunk_header_line_in_diff_buffer(buffer, hunk_index)?;
            (self.buffer_label(id), line)
        };

        if let Some(buffer) = self.buffer_mut(id) {
            let cursor = buffer.line_column_to_char(line.saturating_sub(1), 0);
            buffer.set_single_cursor(cursor);
        }
        self.pending_scroll_lines.insert(id, line.saturating_sub(1));
        Some((label, line))
    }

    fn source_control_patch_copy_inputs(
        &self,
        entries: &[kuroya_core::GitStatusEntry],
    ) -> Vec<SourceControlPatchCopyInput> {
        entries
            .iter()
            .map(|entry| self.source_control_patch_copy_input(entry.path.clone(), entry.stage))
            .collect()
    }

    fn source_control_patch_copy_input(
        &self,
        path: PathBuf,
        stage: GitChangeStage,
    ) -> SourceControlPatchCopyInput {
        match stage {
            GitChangeStage::Staged => SourceControlPatchCopyInput::staged(path),
            GitChangeStage::Unstaged => {
                let text = source_control_patch_text_source_for_status(
                    &path,
                    self.git.status_for(&path),
                    self.buffer_by_path(&path),
                    self.diff_options().max_file_size_bytes,
                );
                SourceControlPatchCopyInput::unstaged(path, text)
            }
        }
    }

    fn source_control_diff_text(&self, path: PathBuf) -> SourceControlDiffText {
        let status = self.git.status_for(&path);
        let open_buffer = self.buffer_by_path(&path);
        source_control_diff_text_source_for_status(
            path,
            status,
            open_buffer,
            self.diff_options().max_file_size_bytes,
        )
    }

    pub(crate) fn open_virtual_diff_buffer(
        &mut self,
        label: String,
        diff: String,
        target: String,
        kind: &str,
        source: Option<DiffBufferSource>,
    ) -> BufferId {
        let label =
            diff_buffer_display_label(label, self.settings.diff_only_show_accessible_viewer);
        let kind = diff_buffer_display_kind(kind, self.settings.diff_only_show_accessible_viewer);
        let target = source_control_diff_display_label(&target);
        if let Some(existing_id) = self
            .virtual_buffer_labels
            .iter()
            .find_map(|(id, existing)| (existing == &label).then_some(*id))
        {
            if let Some(buffer) = self.buffer_mut(existing_id) {
                buffer.replace_from_disk(diff);
                buffer.set_read_only(true);
            }
            self.pending_scroll_lines.remove(&existing_id);
            if let Some(source) = source {
                self.diff_buffer_sources.insert(existing_id, source);
            } else {
                self.diff_buffer_sources.remove(&existing_id);
            }
            self.set_active_buffer(existing_id);
            self.status = source_control_diff_status_text(format!("Updated {kind} for {target}"));
            return existing_id;
        }

        let id = self.next_id();
        let mut buffer = TextBuffer::from_text_with_language(id, None, diff, LanguageId::Diff);
        buffer.set_word_separators(self.settings.word_separators.clone());
        buffer.set_read_only(true);
        self.buffers.push(buffer);
        self.virtual_buffer_labels.insert(id, label);
        if let Some(source) = source {
            self.diff_buffer_sources.insert(id, source);
        }
        self.set_active_buffer(id);
        self.status = source_control_diff_status_text(format!("Opened {kind} for {target}"));
        id
    }

    pub(crate) fn open_virtual_revision_buffer(
        &mut self,
        label: String,
        path: PathBuf,
        text: String,
        target: String,
        kind: &str,
    ) -> BufferId {
        self.open_virtual_revision_buffer_in_pane(
            self.active_pane,
            label,
            path,
            text,
            target,
            kind,
            true,
        )
    }

    pub(crate) fn open_virtual_revision_buffer_in_pane(
        &mut self,
        pane_id: crate::workspace_state::PaneId,
        label: String,
        path: PathBuf,
        text: String,
        target: String,
        kind: &str,
        activate: bool,
    ) -> BufferId {
        let target = source_control_diff_display_label(&target);
        if let Some(existing_id) = self
            .virtual_buffer_labels
            .iter()
            .find_map(|(id, existing)| (existing == &label).then_some(*id))
        {
            if let Some(buffer) = self.buffer_mut(existing_id) {
                buffer.replace_from_disk(text);
                buffer.set_read_only(true);
            }
            self.pending_scroll_lines.remove(&existing_id);
            self.diff_buffer_sources.remove(&existing_id);
            if activate {
                self.set_active_buffer_in_pane(pane_id, existing_id);
            } else {
                self.assign_buffer_to_pane(pane_id, existing_id);
            }
            self.status = source_control_diff_status_text(format!("Updated {kind} for {target}"));
            return existing_id;
        }

        let id = self.next_id();
        let language = LanguageId::from_path(&path);
        let mut buffer = TextBuffer::from_text_with_language(id, None, text, language);
        buffer.set_word_separators(self.settings.word_separators.clone());
        buffer.set_read_only(true);
        self.buffers.push(buffer);
        self.virtual_buffer_labels.insert(id, label);
        if activate {
            self.set_active_buffer_in_pane(pane_id, id);
        } else {
            self.assign_buffer_to_pane(pane_id, id);
        }
        self.status = source_control_diff_status_text(format!("Opened {kind} for {target}"));
        id
    }

    pub(crate) fn stage_active_diff_hunk(&mut self) {
        let Some((source, hunk_index)) = self.active_diff_hunk_target() else {
            return;
        };
        if source.hunk_stage != Some(GitChangeStage::Unstaged) {
            self.status = "Open unstaged changes to stage the current hunk".to_owned();
            return;
        }
        let path = source.path;
        let Some(hunk_fingerprint) = self.cached_source_control_hunk_fingerprint(
            &path,
            GitChangeStage::Unstaged,
            hunk_index,
        ) else {
            self.status =
                source_control_diff_hunk_identity_stale_status("staging", &path, hunk_index);
            return;
        };
        self.stage_source_control_hunk(path, hunk_index, hunk_fingerprint);
    }

    pub(crate) fn unstage_active_diff_hunk(&mut self) {
        let Some((source, hunk_index)) = self.active_diff_hunk_target() else {
            return;
        };
        if source.hunk_stage != Some(GitChangeStage::Staged) {
            self.status = "Open staged changes to unstage the current hunk".to_owned();
            return;
        }
        let path = source.path;
        let Some(hunk_fingerprint) =
            self.cached_source_control_hunk_fingerprint(&path, GitChangeStage::Staged, hunk_index)
        else {
            self.status =
                source_control_diff_hunk_identity_stale_status("unstaging", &path, hunk_index);
            return;
        };
        self.unstage_source_control_hunk(path, hunk_index, hunk_fingerprint);
    }

    pub(crate) fn discard_active_diff_hunk(&mut self) {
        let Some((source, hunk_index)) = self.active_diff_hunk_target() else {
            return;
        };
        if source.hunk_stage != Some(GitChangeStage::Unstaged) {
            self.status = "Open unstaged changes to discard the current hunk".to_owned();
            return;
        }
        let path = source.path;
        let Some(hunk_fingerprint) = self.cached_source_control_hunk_fingerprint(
            &path,
            GitChangeStage::Unstaged,
            hunk_index,
        ) else {
            self.status = source_control_diff_hunk_discard_stale_status(&path, hunk_index);
            return;
        };
        self.discard_source_control_hunk(path, hunk_index, hunk_fingerprint);
    }

    fn active_diff_hunk_target(&mut self) -> Option<(DiffBufferSource, usize)> {
        let Some(id) = self.active else {
            self.status = "No active diff buffer".to_owned();
            return None;
        };
        let Some(source) = self.diff_buffer_sources.get(&id).cloned() else {
            self.status = "No source-control hunk at cursor".to_owned();
            return None;
        };
        let Some(buffer) = self.buffer(id) else {
            self.status = "No active diff buffer".to_owned();
            return None;
        };
        let line = buffer.cursor_position().line + 1;
        let Some(hunk_index) = diff_hunk_index_at_buffer_line(buffer, line) else {
            let label = source_control_diff_display_label(
                self.virtual_buffer_labels
                    .get(&id)
                    .map(String::as_str)
                    .unwrap_or("active diff"),
            );
            self.status =
                source_control_diff_status_text(format!("No diff hunk at {label}:{line}"));
            return None;
        };
        Some((source, hunk_index))
    }
}

fn source_control_diff_text_source_for_status(
    path: PathBuf,
    status: Option<GitFileStatus>,
    open_buffer: Option<&TextBuffer>,
    max_bytes: usize,
) -> SourceControlDiffText {
    if status == Some(GitFileStatus::Deleted) {
        return SourceControlDiffText::Deleted;
    }
    if let Some(buffer) = open_buffer {
        return SourceControlDiffText::open_buffer(buffer, max_bytes);
    }
    SourceControlDiffText::File(path)
}

fn source_control_patch_text_source_for_status(
    path: &Path,
    status: Option<GitFileStatus>,
    open_buffer: Option<&TextBuffer>,
    max_bytes: usize,
) -> SourceControlPatchText {
    if status == Some(GitFileStatus::Deleted) {
        return SourceControlPatchText::Deleted;
    }
    if let Some(buffer) = open_buffer {
        return SourceControlPatchText::open_buffer(buffer, max_bytes);
    }
    SourceControlPatchText::File(path.to_path_buf())
}

impl KuroyaApp {
    pub(crate) fn diff_options(&self) -> DiffOptions {
        DiffOptions {
            ignore_trim_whitespace: self.settings.diff_ignore_trim_whitespace,
            algorithm: self.settings.diff_algorithm,
            hide_unchanged_regions: self.settings.diff_hide_unchanged_regions,
            context_lines: self.settings.diff_context_lines,
            hide_unchanged_regions_minimum_line_count: self
                .settings
                .diff_hide_unchanged_regions_minimum_line_count,
            hide_unchanged_regions_reveal_line_count: self
                .settings
                .diff_hide_unchanged_regions_reveal_line_count,
            max_computation_time_ms: self.settings.diff_max_computation_time_ms,
            max_file_size_bytes: diff_max_file_size_bytes(self.settings.diff_max_file_size_mb),
        }
    }
}

pub(crate) fn hunk_patch_from_unified_diff(diff: &str, hunk_index: usize) -> Option<String> {
    let mut seen_hunks = 0;
    let mut current_file_start = None;
    let mut current_file_first_hunk = None;
    let mut target_start = None;
    let mut target_end = diff.len();
    let mut target_header = None;
    let mut offset = 0usize;

    for raw_line in diff.split_inclusive('\n') {
        let line_start = offset;
        offset += raw_line.len();
        let line = raw_line.trim_end_matches(['\r', '\n']);

        if line.starts_with("diff --git ") {
            if target_start.is_some() {
                target_end = line_start;
                break;
            }
            current_file_start = Some(line_start);
            current_file_first_hunk = None;
            continue;
        }

        if !diff_hunk_header_line(line) {
            continue;
        }

        if target_start.is_some() {
            target_end = line_start;
            break;
        }

        let first_hunk = *current_file_first_hunk.get_or_insert(line_start);
        if seen_hunks == hunk_index {
            target_start = Some(line_start);
            target_header = current_file_start.map(|file_start| (file_start, first_hunk));
        } else {
            seen_hunks += 1;
        }
    }

    let target_start = target_start?;
    let mut patch = String::with_capacity(
        target_end.saturating_sub(target_start)
            + target_header
                .map(|(start, end)| end.saturating_sub(start))
                .unwrap_or_default(),
    );
    if let Some((header_start, header_end)) = target_header {
        push_unified_diff_patch_lines(&mut patch, &diff[header_start..header_end]);
    }
    push_unified_diff_patch_lines(&mut patch, &diff[target_start..target_end]);

    (!patch.is_empty()).then_some(patch)
}

fn push_unified_diff_patch_lines(patch: &mut String, text: &str) {
    for line in text.lines() {
        patch.push_str(line);
        patch.push('\n');
    }
}

pub(crate) fn hunk_patch_from_diff_buffer(
    buffer: &TextBuffer,
    hunk_index: usize,
) -> Option<String> {
    let start = hunk_line_index_in_diff_buffer(buffer, hunk_index)?;
    let end = next_hunk_or_file_line_index(buffer, start + 1).unwrap_or(buffer.len_lines());
    let file_start = (0..=start)
        .rev()
        .find(|index| diff_buffer_line_starts_with(buffer, *index, "diff --git "));
    let header = file_start.map(|file_start| {
        let first_hunk_in_file = (file_start..=start)
            .find(|index| diff_buffer_line_is_hunk_header(buffer, *index))
            .unwrap_or(start);
        file_start..first_hunk_in_file
    });

    let mut patch = String::new();
    if let Some(header) = header {
        for index in header {
            push_diff_buffer_line(&mut patch, buffer, index);
        }
    }
    for index in start..end {
        push_diff_buffer_line(&mut patch, buffer, index);
    }

    (!patch.is_empty()).then_some(patch)
}

#[cfg(test)]
pub(crate) fn hunk_header_line_in_unified_diff(diff: &str, hunk_index: usize) -> Option<usize> {
    diff.lines()
        .enumerate()
        .filter_map(|(index, line)| diff_hunk_header_line(line).then_some(index + 1))
        .nth(hunk_index)
}

pub(crate) fn hunk_header_line_in_diff_buffer(
    buffer: &TextBuffer,
    hunk_index: usize,
) -> Option<usize> {
    hunk_line_index_in_diff_buffer(buffer, hunk_index).map(|index| index + 1)
}

#[cfg(test)]
pub(crate) fn hunk_modified_start_line_in_unified_diff(
    diff: &str,
    hunk_index: usize,
) -> Option<usize> {
    hunk_start_lines_in_unified_diff(diff, hunk_index).map(|(_, modified)| modified)
}

pub(crate) fn hunk_modified_start_line_in_diff_buffer(
    buffer: &TextBuffer,
    hunk_index: usize,
) -> Option<usize> {
    hunk_header_text_in_diff_buffer(buffer, hunk_index)
        .as_deref()
        .and_then(parse_hunk_modified_start_line)
        .map(|line| line.max(1))
}

#[cfg(test)]
pub(crate) fn hunk_original_start_line_in_unified_diff(
    diff: &str,
    hunk_index: usize,
) -> Option<usize> {
    hunk_start_lines_in_unified_diff(diff, hunk_index).map(|(original, _)| original)
}

pub(crate) fn hunk_start_lines_in_unified_diff(
    diff: &str,
    hunk_index: usize,
) -> Option<(usize, usize)> {
    let mut seen = 0usize;
    for line in diff.lines() {
        let Some(header) = parse_diff_hunk_header(line) else {
            continue;
        };
        if seen == hunk_index {
            return Some((header.original_start.max(1), header.modified_start.max(1)));
        }
        seen = seen.saturating_add(1);
    }
    None
}

pub(crate) fn hunk_original_start_line_in_diff_buffer(
    buffer: &TextBuffer,
    hunk_index: usize,
) -> Option<usize> {
    hunk_header_text_in_diff_buffer(buffer, hunk_index)
        .as_deref()
        .and_then(parse_hunk_original_start_line)
        .map(|line| line.max(1))
}

fn hunk_line_index_in_diff_buffer(buffer: &TextBuffer, hunk_index: usize) -> Option<usize> {
    let mut seen = 0usize;
    for index in 0..buffer.len_lines() {
        if diff_buffer_line_is_hunk_header(buffer, index) {
            if seen == hunk_index {
                return Some(index);
            }
            seen = seen.saturating_add(1);
        }
    }
    None
}

fn next_hunk_or_file_line_index(buffer: &TextBuffer, start: usize) -> Option<usize> {
    (start..buffer.len_lines()).find(|index| {
        diff_buffer_line_is_hunk_header(buffer, *index)
            || diff_buffer_line_starts_with(buffer, *index, "diff --git ")
    })
}

fn hunk_header_text_in_diff_buffer(buffer: &TextBuffer, hunk_index: usize) -> Option<String> {
    let index = hunk_line_index_in_diff_buffer(buffer, hunk_index)?;
    buffer.line_content_prefix(index, 4096)
}

fn diff_buffer_line_is_hunk_header(buffer: &TextBuffer, line_idx: usize) -> bool {
    if !buffer.line_starts_with(line_idx, "@@") {
        return false;
    }
    buffer
        .line_content_prefix(line_idx, 4096)
        .is_some_and(|prefix| diff_hunk_header_line(&prefix))
}

fn diff_buffer_line_starts_with(buffer: &TextBuffer, line_idx: usize, needle: &str) -> bool {
    buffer.line_starts_with(line_idx, needle)
}

fn push_diff_buffer_line(patch: &mut String, buffer: &TextBuffer, line_idx: usize) {
    if let Some(line) = buffer.line(line_idx) {
        let line = line.trim_end_matches(['\r', '\n']);
        if line_idx + 1 == buffer.len_lines() && line.is_empty() {
            return;
        }
        patch.push_str(line);
        patch.push('\n');
    }
}

fn diff_hunk_header_line(line: &str) -> bool {
    parse_diff_hunk_header(line).is_some()
}

fn parse_hunk_modified_start_line(header: &str) -> Option<usize> {
    parse_diff_hunk_header(header).map(|header| header.modified_start)
}

fn parse_hunk_original_start_line(header: &str) -> Option<usize> {
    parse_diff_hunk_header(header).map(|header| header.original_start)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DiffHunkHeader {
    original_start: usize,
    modified_start: usize,
}

fn parse_diff_hunk_header(line: &str) -> Option<DiffHunkHeader> {
    let line = line.trim_end_matches(['\r', '\n']);
    let mut parts = line.split_whitespace();
    let marker = parts.next()?;
    let marker_len = marker.len();
    if marker_len < 2 || !marker.bytes().all(|byte| byte == b'@') {
        return None;
    }

    let mut old_range_count = 0usize;
    let mut new_range_count = 0usize;
    let mut original_start = None;
    let mut modified_start = None;
    let mut has_non_empty_range = false;
    let mut closed = false;
    for part in parts.by_ref() {
        if part == marker {
            closed = true;
            break;
        }
        let (kind, start, count) = parse_hunk_range_token(part)?;
        has_non_empty_range |= count > 0;
        match kind {
            '-' => {
                old_range_count += 1;
                original_start = Some(start);
            }
            '+' => {
                new_range_count += 1;
                modified_start = Some(start);
            }
            _ => return None,
        }
    }
    if !closed
        || old_range_count != marker_len.saturating_sub(1)
        || new_range_count != 1
        || !has_non_empty_range
    {
        return None;
    }

    Some(DiffHunkHeader {
        original_start: original_start?,
        modified_start: modified_start?,
    })
}

fn parse_hunk_range_token(token: &str) -> Option<(char, usize, usize)> {
    let mut chars = token.chars();
    let kind = chars.next()?;
    if kind != '-' && kind != '+' {
        return None;
    }
    let range = chars.as_str();
    let (start, count) = range.split_once(',').unwrap_or((range, ""));
    if start.is_empty() || (range.contains(',') && count.is_empty()) || count.contains(',') {
        return None;
    }
    let start = start.parse::<usize>().ok()?;
    let count = if count.is_empty() {
        1
    } else {
        count.parse::<usize>().ok()?
    };
    if start == 0 && count > 0 {
        return None;
    }

    Some((kind, start, count))
}

#[cfg(test)]
mod tests;
