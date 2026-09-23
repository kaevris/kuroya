use crate::{
    KuroyaApp,
    devtools_async_tasks::{hunk_detail, path_detail},
    file_io::{file_size_exceeds_limit, file_too_large_message, read_utf8_text_file_with_limit},
    path_display::{display_error_label_cow, display_path_label_cow},
    source_control_runtime::{
        finish_source_control_load_request_state, invalidate_source_control_load_request_state,
        source_control_panel_load_event_matches,
    },
    ui_events::UiEvent,
    workspace_state::workspace_event_matches,
};
use eframe::egui::Context;
use kuroya_core::{
    BufferId, GitChangeStage, GitDiffHunk, GitFileStatus, TextBuffer, TextSnapshot,
    discard_worktree_hunk, stage_worktree_hunk, staged_diff_hunks, unstage_staged_hunk,
    worktree_diff_hunks,
};
use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
enum SourceControlHunkText {
    Ready(String),
    Snapshot(TextSnapshot),
    File(PathBuf),
    TooLarge { bytes: usize },
}

impl SourceControlHunkText {
    fn load(self, max_bytes: usize) -> anyhow::Result<String> {
        match self {
            Self::Ready(text) => {
                if text_exceeds_max_bytes(text.len(), max_bytes) {
                    anyhow::bail!("{}", open_buffer_too_large_message(text.len(), max_bytes));
                }
                Ok(text)
            }
            Self::Snapshot(text) => {
                let bytes = text.len_bytes();
                if text_exceeds_max_bytes(bytes, max_bytes) {
                    anyhow::bail!("{}", open_buffer_too_large_message(bytes, max_bytes));
                }
                Ok(text.text())
            }
            Self::File(path) => read_utf8_text_file_with_limit(&path, max_bytes).map_err(|error| {
                let error = error.to_string();
                anyhow::anyhow!(
                    "could not read {}: {}",
                    display_path_label_cow(&path),
                    display_error_label_cow(&error)
                )
            }),
            Self::TooLarge { bytes } => {
                anyhow::bail!("{}", open_buffer_too_large_message(bytes, max_bytes))
            }
        }
    }
}

fn source_control_hunk_text_for_open_buffer(
    buffer: &TextBuffer,
    max_bytes: usize,
) -> SourceControlHunkText {
    let bytes = buffer.len_bytes();
    if text_exceeds_max_bytes(bytes, max_bytes) {
        SourceControlHunkText::TooLarge { bytes }
    } else {
        SourceControlHunkText::Snapshot(buffer.text_snapshot())
    }
}

fn source_control_hunk_text_source_for_status(
    path: &Path,
    status: Option<GitFileStatus>,
    open_buffer: Option<&TextBuffer>,
    max_bytes: usize,
) -> SourceControlHunkText {
    if status == Some(GitFileStatus::Deleted) {
        return SourceControlHunkText::Ready(String::new());
    }
    if let Some(buffer) = open_buffer {
        return source_control_hunk_text_for_open_buffer(buffer, max_bytes);
    }
    SourceControlHunkText::File(path.to_path_buf())
}

fn text_exceeds_max_bytes(bytes: usize, max_bytes: usize) -> bool {
    file_size_exceeds_limit(
        u64::try_from(bytes).unwrap_or(u64::MAX),
        u64::try_from(max_bytes).unwrap_or(u64::MAX),
    )
}

fn open_buffer_too_large_message(bytes: usize, max_bytes: usize) -> String {
    file_too_large_message(
        u64::try_from(bytes).unwrap_or(u64::MAX),
        u64::try_from(max_bytes).unwrap_or(u64::MAX),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CachedGitHunkLookup {
    Found(usize),
    Empty,
    NoHunkAtLine,
    Ambiguous,
    MissingCache,
}

fn source_control_hunk_selection_after_reload(
    selected: usize,
    previous_hunks: &[GitDiffHunk],
    reloaded_hunks: &[GitDiffHunk],
) -> usize {
    let fallback = selected.min(reloaded_hunks.len().saturating_sub(1));
    let Some(previous_hunk) = previous_hunks.get(selected) else {
        return fallback;
    };
    reloaded_hunks
        .iter()
        .position(|hunk| source_control_hunk_identity_matches(previous_hunk, hunk))
        .unwrap_or(fallback)
}

fn source_control_hunk_identity_matches(left: &GitDiffHunk, right: &GitDiffHunk) -> bool {
    left.old_start == right.old_start
        && left.old_lines == right.old_lines
        && left.new_start == right.new_start
        && left.new_lines == right.new_lines
        && left.header == right.header
}

fn cached_git_hunk_index_at_new_line(
    cache_open: bool,
    cache_path: Option<&Path>,
    cache_stage: GitChangeStage,
    target_path: &Path,
    target_stage: GitChangeStage,
    hunks: &[GitDiffHunk],
    line: usize,
) -> CachedGitHunkLookup {
    if !cache_open || cache_path != Some(target_path) || cache_stage != target_stage {
        return CachedGitHunkLookup::MissingCache;
    }
    if hunks.is_empty() {
        return CachedGitHunkLookup::Empty;
    }
    let hunk_lookup = git_hunk_lookup_at_new_line(hunks, line);
    match hunk_lookup {
        GitHunkLineLookup::Found(index) => CachedGitHunkLookup::Found(index),
        GitHunkLineLookup::Missing => CachedGitHunkLookup::NoHunkAtLine,
        GitHunkLineLookup::Ambiguous => CachedGitHunkLookup::Ambiguous,
    }
}

fn hunk_discard_should_replace_open_buffer(
    id: BufferId,
    version: u64,
    dirty: bool,
    expected_buffer: Option<(BufferId, u64)>,
) -> bool {
    match expected_buffer {
        Some((expected_id, expected_version)) => {
            id == expected_id && version == expected_version && !dirty
        }
        None => !dirty,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HunkDiscardOpenBufferUpdate {
    Replace,
    AlreadyApplied,
    MarkChangedOnDisk,
}

fn hunk_discard_open_buffer_update(
    id: BufferId,
    version: u64,
    dirty: bool,
    already_has_discarded_text: bool,
    expected_buffer: Option<(BufferId, u64)>,
) -> HunkDiscardOpenBufferUpdate {
    if hunk_discard_should_replace_open_buffer(id, version, dirty, expected_buffer) {
        return HunkDiscardOpenBufferUpdate::Replace;
    }
    if !dirty && already_has_discarded_text {
        return HunkDiscardOpenBufferUpdate::AlreadyApplied;
    }
    HunkDiscardOpenBufferUpdate::MarkChangedOnDisk
}

fn source_control_hunk_load_target_matches(
    current_path: Option<&Path>,
    current_stage: GitChangeStage,
    event_path: &Path,
    event_stage: GitChangeStage,
) -> bool {
    current_path == Some(event_path) && current_stage == event_stage
}

fn source_control_hunk_open_path_matches(
    panel_open: bool,
    current_path: Option<&Path>,
    event_path: &Path,
) -> bool {
    panel_open && current_path == Some(event_path)
}

impl KuroyaApp {
    pub(crate) fn open_active_file_hunks(&mut self) {
        let Some(path) = self.active_file_or_diff_source_path("open hunks") else {
            return;
        };
        self.begin_source_control_hunks(path);
    }

    pub(crate) fn open_active_file_staged_hunks(&mut self) {
        let Some(path) = self.active_file_or_diff_source_path("open staged hunks") else {
            return;
        };
        self.begin_source_control_staged_hunks(path);
    }

    pub(crate) fn stage_active_file_hunk(&mut self) {
        if !self.require_trusted_source_control_mutation("staging hunks") {
            return;
        }
        let Some((path, hunk_index)) = self.active_file_worktree_hunk_target("stage") else {
            return;
        };
        let Some(hunk_fingerprint) = self.required_source_control_hunk_fingerprint(
            &path,
            GitChangeStage::Unstaged,
            hunk_index,
            "stage",
        ) else {
            return;
        };
        self.stage_source_control_hunk(path, hunk_index, hunk_fingerprint);
    }

    pub(crate) fn discard_active_file_hunk(&mut self) {
        if !self.require_trusted_source_control_mutation("discarding hunks") {
            return;
        }
        let Some((path, hunk_index)) = self.active_file_worktree_hunk_target("discard") else {
            return;
        };
        let Some(hunk_fingerprint) = self.required_source_control_hunk_fingerprint(
            &path,
            GitChangeStage::Unstaged,
            hunk_index,
            "discard",
        ) else {
            return;
        };
        self.discard_source_control_hunk(path, hunk_index, hunk_fingerprint);
    }

    pub(crate) fn unstage_active_file_hunk(&mut self) {
        if !self.require_trusted_source_control_mutation("unstaging hunks") {
            return;
        }
        let Some((path, hunk_index)) = self.active_file_staged_hunk_target("unstage") else {
            return;
        };
        let Some(hunk_fingerprint) = self.required_source_control_hunk_fingerprint(
            &path,
            GitChangeStage::Staged,
            hunk_index,
            "unstage",
        ) else {
            return;
        };
        self.unstage_source_control_hunk(path, hunk_index, hunk_fingerprint);
    }

    pub(crate) fn copy_active_file_hunk_patch(&mut self, ctx: &Context, stage: GitChangeStage) {
        let target = match stage {
            GitChangeStage::Unstaged => self.active_file_worktree_hunk_target("copy patch from"),
            GitChangeStage::Staged => self.active_file_staged_hunk_target("copy staged patch from"),
        };
        let Some((path, hunk_index)) = target else {
            return;
        };
        self.copy_source_control_hunk_patch(ctx, path, stage, hunk_index);
    }

    pub(crate) fn open_active_file_hunk_diff(&mut self, stage: GitChangeStage) {
        let target = match stage {
            GitChangeStage::Unstaged => self.active_file_worktree_hunk_target("open diff for"),
            GitChangeStage::Staged => self.active_file_staged_hunk_target("open staged diff for"),
        };
        let Some((path, hunk_index)) = target else {
            return;
        };
        self.open_source_control_hunk_diff(path, stage, hunk_index);
    }

    pub(crate) fn begin_source_control_hunks(&mut self, path: PathBuf) {
        self.begin_source_control_hunks_for_stage(path, GitChangeStage::Unstaged);
    }

    pub(crate) fn begin_source_control_staged_hunks(&mut self, path: PathBuf) {
        self.begin_source_control_hunks_for_stage(path, GitChangeStage::Staged);
    }

    pub(crate) fn clear_source_control_hunks_for_path(&mut self, path: &Path) {
        if self.source_control_hunk_path.as_deref() != Some(path) {
            return;
        }

        self.source_control_hunks_open = false;
        self.source_control_hunk_path = None;
        self.source_control_hunk_stage = GitChangeStage::Unstaged;
        self.source_control_hunks.clear();
        self.source_control_hunk_selected = 0;
        invalidate_source_control_load_request_state(
            &mut self.source_control_hunks_next_request_id,
            &mut self.source_control_hunks_active_request_id,
            &mut self.source_control_hunks_in_flight_request_id,
            &mut self.source_control_hunks_reload_queued,
        );
    }

    fn begin_source_control_hunks_for_stage(&mut self, path: PathBuf, stage: GitChangeStage) {
        self.source_control_hunks_open = true;
        self.git_panel_open_generation += 1;
        self.source_control_hunk_path = Some(path.clone());
        self.source_control_hunk_stage = stage;
        self.source_control_hunk_selected = 0;
        self.spawn_git_hunk_list(path);
    }

    pub(crate) fn stage_source_control_hunk(
        &mut self,
        path: PathBuf,
        hunk_index: usize,
        hunk_fingerprint: u64,
    ) {
        if !self.require_trusted_source_control_mutation("staging hunks") {
            return;
        }
        let text =
            self.source_control_hunk_text_source(&path, self.diff_options().max_file_size_bytes);
        self.spawn_stage_git_hunk(path, hunk_index, hunk_fingerprint, text);
    }

    pub(crate) fn unstage_source_control_hunk(
        &mut self,
        path: PathBuf,
        hunk_index: usize,
        hunk_fingerprint: u64,
    ) {
        if !self.require_trusted_source_control_mutation("unstaging hunks") {
            return;
        }
        self.spawn_unstage_git_hunk(path, hunk_index, hunk_fingerprint);
    }

    pub(crate) fn discard_source_control_hunk(
        &mut self,
        path: PathBuf,
        hunk_index: usize,
        hunk_fingerprint: u64,
    ) {
        if !self.require_trusted_source_control_mutation("discarding hunks") {
            return;
        }
        let max_bytes = self.diff_options().max_file_size_bytes;
        let status = self.git.status_for(&path);
        let open_buffer = self.buffer_by_lexical_path(&path);
        let expected_buffer = if let Some(buffer) = open_buffer {
            if buffer.is_dirty() {
                self.status = git_hunk_discard_dirty_buffer_status(&path);
                return;
            }
            Some((buffer.id(), buffer.version()))
        } else {
            None
        };
        let text =
            source_control_hunk_text_source_for_status(&path, status, open_buffer, max_bytes);
        self.spawn_discard_git_hunk(path, hunk_index, hunk_fingerprint, text, expected_buffer);
    }

    pub(crate) fn reject_stale_source_control_hunk_discard(
        &mut self,
        path: PathBuf,
        hunk_index: usize,
    ) {
        let status = git_hunk_discard_missing_identity_status(&path, hunk_index);
        self.begin_source_control_hunks_for_stage(path, GitChangeStage::Unstaged);
        self.status = status;
    }

    pub(crate) fn reject_stale_source_control_hunk_stage(
        &mut self,
        path: PathBuf,
        hunk_index: usize,
    ) {
        let status = git_hunk_stage_missing_identity_status(&path, hunk_index);
        self.begin_source_control_hunks_for_stage(path, GitChangeStage::Unstaged);
        self.status = status;
    }

    pub(crate) fn reject_stale_source_control_hunk_unstage(
        &mut self,
        path: PathBuf,
        hunk_index: usize,
    ) {
        let status = git_hunk_unstage_missing_identity_status(&path, hunk_index);
        self.begin_source_control_hunks_for_stage(path, GitChangeStage::Staged);
        self.status = status;
    }

    pub(crate) fn apply_git_hunks_loaded(
        &mut self,
        request_id: u64,
        root: PathBuf,
        operation_root: PathBuf,
        path: PathBuf,
        stage: GitChangeStage,
        hunks: Vec<GitDiffHunk>,
    ) {
        if !self.source_control_hunk_load_event_matches(
            request_id,
            &root,
            &operation_root,
            &path,
            stage,
        ) {
            return;
        }

        let count = hunks.len();
        let selected = source_control_hunk_selection_after_reload(
            self.source_control_hunk_selected,
            &self.source_control_hunks,
            &hunks,
        );
        self.source_control_hunks = hunks;
        self.source_control_hunk_selected = selected;
        self.status = git_hunk_list_success_status(stage, &path, count);
    }

    pub(crate) fn apply_git_hunks_failed(
        &mut self,
        request_id: u64,
        root: PathBuf,
        operation_root: PathBuf,
        path: PathBuf,
        stage: GitChangeStage,
        error: String,
    ) {
        if !self.source_control_hunk_load_event_matches(
            request_id,
            &root,
            &operation_root,
            &path,
            stage,
        ) {
            return;
        }

        self.source_control_hunks.clear();
        self.status = git_hunk_list_failure_status(stage, &path, &error);
    }

    pub(crate) fn apply_git_hunk_staged(
        &mut self,
        root: PathBuf,
        path: PathBuf,
        hunk_index: usize,
    ) {
        if !workspace_event_matches(&self.workspace.root, &root) {
            return;
        }

        self.spawn_git_scoped_refresh(vec![path.clone()]);
        let status = git_hunk_stage_success_status(&path, hunk_index);
        if source_control_hunk_open_path_matches(
            self.source_control_hunks_open,
            self.source_control_hunk_path.as_deref(),
            &path,
        ) {
            self.spawn_git_hunk_list(path);
        }
        self.status = status;
    }

    pub(crate) fn apply_git_hunk_stage_failed(
        &mut self,
        root: PathBuf,
        path: PathBuf,
        hunk_index: usize,
        error: String,
    ) {
        if !workspace_event_matches(&self.workspace.root, &root) {
            return;
        }

        self.spawn_git_scan();
        self.status = git_hunk_stage_failure_status(&path, hunk_index, &error);
    }

    pub(crate) fn apply_git_hunk_unstaged(
        &mut self,
        root: PathBuf,
        path: PathBuf,
        hunk_index: usize,
    ) {
        if !workspace_event_matches(&self.workspace.root, &root) {
            return;
        }

        self.spawn_git_scoped_refresh(vec![path.clone()]);
        let status = git_hunk_unstage_success_status(&path, hunk_index);
        if source_control_hunk_open_path_matches(
            self.source_control_hunks_open,
            self.source_control_hunk_path.as_deref(),
            &path,
        ) {
            self.spawn_git_hunk_list(path);
        }
        self.status = status;
    }

    pub(crate) fn apply_git_hunk_unstage_failed(
        &mut self,
        root: PathBuf,
        path: PathBuf,
        hunk_index: usize,
        error: String,
    ) {
        if !workspace_event_matches(&self.workspace.root, &root) {
            return;
        }

        self.spawn_git_scan();
        self.status = git_hunk_unstage_failure_status(&path, hunk_index, &error);
    }

    pub(crate) fn apply_git_hunk_discarded(
        &mut self,
        root: PathBuf,
        path: PathBuf,
        hunk_index: usize,
        text: String,
        expected_buffer: Option<(BufferId, u64)>,
    ) {
        if !workspace_event_matches(&self.workspace.root, &root) {
            return;
        }

        if let Some((id, version, dirty, already_has_discarded_text)) =
            self.buffer_by_lexical_path(&path).map(|buffer| {
                (
                    buffer.id(),
                    buffer.version(),
                    buffer.is_dirty(),
                    buffer.text() == text.as_str(),
                )
            })
        {
            match hunk_discard_open_buffer_update(
                id,
                version,
                dirty,
                already_has_discarded_text,
                expected_buffer,
            ) {
                HunkDiscardOpenBufferUpdate::Replace => {
                    if let Some(buffer) = self.buffer_mut(id) {
                        buffer.replace_from_disk(text);
                    }
                    self.clear_buffer_changed_on_disk(id);
                    self.diff_cache.remove(&id);
                    self.spawn_diagnostics_for(id);
                    self.notify_lsp_change(id);
                }
                HunkDiscardOpenBufferUpdate::AlreadyApplied => {}
                HunkDiscardOpenBufferUpdate::MarkChangedOnDisk => {
                    self.mark_buffer_changed_on_disk(id);
                }
            }
        }

        self.spawn_index();
        self.spawn_git_scoped_refresh(vec![path.clone()]);
        let status = git_hunk_discard_success_status(&path, hunk_index);
        if source_control_hunk_open_path_matches(
            self.source_control_hunks_open,
            self.source_control_hunk_path.as_deref(),
            &path,
        ) {
            self.spawn_git_hunk_list(path);
        }
        self.status = status;
    }

    pub(crate) fn apply_git_hunk_discard_failed(
        &mut self,
        root: PathBuf,
        path: PathBuf,
        hunk_index: usize,
        error: String,
    ) {
        if !workspace_event_matches(&self.workspace.root, &root) {
            return;
        }

        self.spawn_index();
        self.spawn_git_scan();
        self.status = git_hunk_discard_failure_status(&path, hunk_index, &error);
    }

    fn source_control_hunk_text_source(
        &self,
        path: &Path,
        max_bytes: usize,
    ) -> SourceControlHunkText {
        source_control_hunk_text_source_for_status(
            path,
            self.git.status_for(path),
            self.buffer_by_lexical_path(path),
            max_bytes,
        )
    }

    fn source_control_hunk_load_event_matches(
        &self,
        request_id: u64,
        root: &Path,
        operation_root: &Path,
        path: &Path,
        stage: GitChangeStage,
    ) -> bool {
        source_control_panel_load_event_matches(
            self.source_control_hunks_open,
            &self.workspace.root,
            root,
            request_id,
            self.source_control_hunks_active_request_id,
        ) && self.source_control_git_operation_root_matches(operation_root)
            && source_control_hunk_load_target_matches(
                self.source_control_hunk_path.as_deref(),
                self.source_control_hunk_stage,
                path,
                stage,
            )
    }

    fn active_file_worktree_hunk_target(&mut self, action: &str) -> Option<(PathBuf, usize)> {
        self.active_file_cached_hunk_target(action, GitChangeStage::Unstaged)
    }

    fn active_file_staged_hunk_target(&mut self, action: &str) -> Option<(PathBuf, usize)> {
        self.active_file_cached_hunk_target(action, GitChangeStage::Staged)
    }

    pub(crate) fn cached_source_control_hunk_fingerprint(
        &self,
        path: &Path,
        stage: GitChangeStage,
        hunk_index: usize,
    ) -> Option<u64> {
        if !self.source_control_hunks_open
            || self.source_control_hunk_path.as_deref() != Some(path)
            || self.source_control_hunk_stage != stage
        {
            return None;
        }
        let mut matches = self
            .source_control_hunks
            .iter()
            .filter(|hunk| hunk.index == hunk_index);
        let fingerprint = matches.next()?.fingerprint;
        if matches.next().is_some() {
            return None;
        }
        Some(fingerprint)
    }

    pub(crate) fn required_source_control_hunk_fingerprint(
        &mut self,
        path: &Path,
        stage: GitChangeStage,
        hunk_index: usize,
        action: &str,
    ) -> Option<u64> {
        match self.cached_source_control_hunk_fingerprint(path, stage, hunk_index) {
            Some(fingerprint) => Some(fingerprint),
            None => {
                self.begin_source_control_hunks_for_stage(path.to_path_buf(), stage);
                self.status = git_hunk_target_loading_status(stage, path, action);
                None
            }
        }
    }

    fn active_file_cached_hunk_target(
        &mut self,
        action: &str,
        stage: GitChangeStage,
    ) -> Option<(PathBuf, usize)> {
        let Some(id) = self.active else {
            self.status = format!("No active file to {action} a hunk");
            return None;
        };
        let Some(buffer) = self.buffer(id) else {
            self.status = format!("No file-backed buffer to {action} a hunk");
            return None;
        };
        let Some(path) = buffer.path() else {
            self.status = format!("No file-backed buffer to {action} a hunk");
            return None;
        };
        let line = buffer.cursor_position().line + 1;

        let lookup = cached_git_hunk_index_at_new_line(
            self.source_control_hunks_open,
            self.source_control_hunk_path.as_deref(),
            self.source_control_hunk_stage,
            path,
            stage,
            &self.source_control_hunks,
            line,
        );
        match lookup {
            CachedGitHunkLookup::Found(hunk_index) => Some((path.to_path_buf(), hunk_index)),
            CachedGitHunkLookup::Empty => {
                self.status = format!(
                    "No {} hunks in {}",
                    hunk_stage_label(stage),
                    display_path_label_cow(path)
                );
                None
            }
            CachedGitHunkLookup::NoHunkAtLine => {
                self.status = format!(
                    "Move the cursor into a {} hunk in {}",
                    hunk_stage_label(stage),
                    display_path_label_cow(path)
                );
                None
            }
            CachedGitHunkLookup::Ambiguous => {
                let status = git_hunk_target_loading_status(stage, path, action);
                let path = path.to_path_buf();
                self.begin_source_control_hunks_for_stage(path, stage);
                self.status = status;
                None
            }
            CachedGitHunkLookup::MissingCache => {
                let status = git_hunk_target_loading_status(stage, path, action);
                let path = path.to_path_buf();
                self.begin_source_control_hunks_for_stage(path, stage);
                self.status = status;
                None
            }
        }
    }

    pub(crate) fn spawn_git_hunk_list(&mut self, path: PathBuf) -> bool {
        let Some(request_id) = self.begin_source_control_hunks_request() else {
            self.set_git_progress_status(git_hunk_list_pending_status(
                self.source_control_hunk_stage,
                &path,
            ));
            return false;
        };
        let stage = self.source_control_hunk_stage;
        let text =
            if stage == GitChangeStage::Unstaged {
                Some(self.source_control_hunk_text_source(
                    &path,
                    self.diff_options().max_file_size_bytes,
                ))
            } else {
                None
            };
        let event_root = self.workspace.root.clone();
        let git_root = self.source_control_git_operation_root();
        let max_bytes = self.diff_options().max_file_size_bytes;
        let tx = self.tx.clone();
        self.set_git_progress_status(git_hunk_list_pending_status(stage, &path));
        self.record_async_task_started("Git Hunks", path_detail(&path));
        self.runtime.spawn_blocking(move || {
            let result = match stage {
                GitChangeStage::Staged => staged_diff_hunks(&git_root, &path),
                GitChangeStage::Unstaged => text
                    .map(|text| text.load(max_bytes))
                    .unwrap_or_else(|| Ok(String::new()))
                    .and_then(|text| worktree_diff_hunks(&git_root, &path, &text)),
            };
            let event = match result {
                Ok(hunks) => UiEvent::GitHunksLoaded {
                    request_id,
                    root: event_root,
                    operation_root: git_root,
                    path,
                    stage,
                    hunks,
                },
                Err(error) => UiEvent::GitHunksFailed {
                    request_id,
                    root: event_root,
                    operation_root: git_root,
                    path,
                    stage,
                    error: error.to_string(),
                },
            };
            let _ = crate::ui_event_channel::send_ui_event(&tx, event);
        });
        true
    }

    fn begin_source_control_hunks_request(&mut self) -> Option<u64> {
        begin_source_control_hunks_request_state(
            &mut self.source_control_hunks_next_request_id,
            &mut self.source_control_hunks_active_request_id,
            &mut self.source_control_hunks_in_flight_request_id,
            &mut self.source_control_hunks_reload_queued,
        )
    }

    pub(crate) fn finish_source_control_hunks_request(&mut self, request_id: u64) -> bool {
        finish_source_control_load_request_state(
            &mut self.source_control_hunks_in_flight_request_id,
            &mut self.source_control_hunks_reload_queued,
            request_id,
        )
    }

    fn spawn_stage_git_hunk(
        &mut self,
        path: PathBuf,
        hunk_index: usize,
        hunk_fingerprint: u64,
        text: SourceControlHunkText,
    ) {
        if !self.require_trusted_source_control_mutation("staging hunks") {
            return;
        }
        let event_root = self.workspace.root.clone();
        let git_root = self.source_control_git_operation_root();
        let max_bytes = self.diff_options().max_file_size_bytes;
        let tx = self.tx.clone();
        self.set_git_progress_status(git_hunk_stage_pending_status(&path, hunk_index));
        self.record_async_task_started("Git Hunk Stage", hunk_detail(&path, hunk_index));
        self.runtime.spawn_blocking(move || {
            let result = text.load(max_bytes).and_then(|text| {
                stage_worktree_hunk(&git_root, &path, &text, hunk_index, hunk_fingerprint)
            });
            let event = match result {
                Ok(()) => UiEvent::GitHunkStaged {
                    root: event_root,
                    path,
                    hunk_index,
                },
                Err(error) => UiEvent::GitHunkStageFailed {
                    root: event_root,
                    path,
                    hunk_index,
                    error: error.to_string(),
                },
            };
            let _ = crate::ui_event_channel::send_ui_event(&tx, event);
        });
    }

    fn spawn_unstage_git_hunk(&mut self, path: PathBuf, hunk_index: usize, hunk_fingerprint: u64) {
        if !self.require_trusted_source_control_mutation("unstaging hunks") {
            return;
        }
        let event_root = self.workspace.root.clone();
        let git_root = self.source_control_git_operation_root();
        let tx = self.tx.clone();
        self.set_git_progress_status(git_hunk_unstage_pending_status(&path, hunk_index));
        self.record_async_task_started("Git Hunk Unstage", hunk_detail(&path, hunk_index));
        self.runtime.spawn_blocking(move || {
            let result = unstage_staged_hunk(&git_root, &path, hunk_index, hunk_fingerprint);
            let event = match result {
                Ok(()) => UiEvent::GitHunkUnstaged {
                    root: event_root,
                    path,
                    hunk_index,
                },
                Err(error) => UiEvent::GitHunkUnstageFailed {
                    root: event_root,
                    path,
                    hunk_index,
                    error: error.to_string(),
                },
            };
            let _ = crate::ui_event_channel::send_ui_event(&tx, event);
        });
    }

    fn spawn_discard_git_hunk(
        &mut self,
        path: PathBuf,
        hunk_index: usize,
        hunk_fingerprint: u64,
        text: SourceControlHunkText,
        expected_buffer: Option<(BufferId, u64)>,
    ) {
        if !self.require_trusted_source_control_mutation("discarding hunks") {
            return;
        }
        let event_root = self.workspace.root.clone();
        let git_root = self.source_control_git_operation_root();
        let max_bytes = self.diff_options().max_file_size_bytes;
        let tx = self.tx.clone();
        self.set_git_progress_status(git_hunk_discard_pending_status(&path, hunk_index));
        self.record_async_task_started("Git Hunk Discard", hunk_detail(&path, hunk_index));
        self.runtime.spawn_blocking(move || {
            let result = text.load(max_bytes).and_then(|text| {
                discard_worktree_hunk(&git_root, &path, &text, hunk_index, hunk_fingerprint)
            });
            let event = match result {
                Ok(text) => UiEvent::GitHunkDiscarded {
                    root: event_root,
                    path,
                    hunk_index,
                    text,
                    expected_buffer,
                },
                Err(error) => UiEvent::GitHunkDiscardFailed {
                    root: event_root,
                    path,
                    hunk_index,
                    error: error.to_string(),
                },
            };
            let _ = crate::ui_event_channel::send_ui_event(&tx, event);
        });
    }
}

#[cfg(test)]
pub(crate) fn worktree_hunk_index_at_line(hunks: &[GitDiffHunk], line: usize) -> Option<usize> {
    git_hunk_index_at_new_line(hunks, line)
}

#[cfg(test)]
pub(crate) fn git_hunk_index_at_new_line(hunks: &[GitDiffHunk], line: usize) -> Option<usize> {
    match git_hunk_lookup_at_new_line(hunks, line) {
        GitHunkLineLookup::Found(index) => Some(index),
        GitHunkLineLookup::Missing | GitHunkLineLookup::Ambiguous => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GitHunkLineLookup {
    Found(usize),
    Missing,
    Ambiguous,
}

fn git_hunk_lookup_at_new_line(hunks: &[GitDiffHunk], line: usize) -> GitHunkLineLookup {
    let mut matched_index = None;
    for hunk in hunks {
        let Some(range) = git_hunk_new_line_range(hunk) else {
            continue;
        };
        if !range.contains(line) {
            continue;
        }
        if matched_index.replace(hunk.index).is_some() {
            return GitHunkLineLookup::Ambiguous;
        }
    }
    matched_index
        .map(GitHunkLineLookup::Found)
        .unwrap_or(GitHunkLineLookup::Missing)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GitHunkLineRange {
    start: usize,
    end: usize,
}

impl GitHunkLineRange {
    fn contains(self, line: usize) -> bool {
        self.start <= line && line <= self.end
    }
}

fn git_hunk_new_line_range(hunk: &GitDiffHunk) -> Option<GitHunkLineRange> {
    if hunk.new_start == 0 && hunk.new_lines > 0 {
        return None;
    }
    let start = hunk.new_start.max(1);
    let end = if hunk.new_lines == 0 {
        start
    } else {
        start.saturating_add(hunk.new_lines.saturating_sub(1))
    };
    Some(GitHunkLineRange { start, end })
}

fn begin_source_control_hunks_request_state(
    next_request_id: &mut u64,
    active_request_id: &mut u64,
    in_flight_request_id: &mut Option<u64>,
    reload_queued: &mut bool,
) -> Option<u64> {
    let request_id = reserve_source_control_hunks_request_id_state(
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

fn reserve_source_control_hunks_request_id_state(
    next_request_id: &mut u64,
    active_request_id: &mut u64,
    reserved_request_id: Option<u64>,
) -> u64 {
    let mut request_id = next_source_control_hunks_request_id(*next_request_id);
    if Some(request_id) == reserved_request_id {
        request_id = next_source_control_hunks_request_id(request_id);
    }
    *next_request_id = request_id;
    *active_request_id = request_id;
    request_id
}

fn next_source_control_hunks_request_id(current: u64) -> u64 {
    match current.wrapping_add(1) {
        0 => 1,
        request_id => request_id,
    }
}

pub(crate) fn git_hunk_list_pending_status(stage: GitChangeStage, path: &Path) -> String {
    hunk_status_with_path("Loading ", hunk_stage_label(stage), " hunks in ", path, "")
}

pub(crate) fn git_hunk_list_success_status(
    stage: GitChangeStage,
    path: &Path,
    count: usize,
) -> String {
    let stage = hunk_stage_label(stage);
    match count {
        0 => hunk_status_with_path("No ", stage, " hunks in ", path, ""),
        1 => hunk_status_with_path("Loaded 1 ", stage, " hunk in ", path, ""),
        _ => hunk_count_status("Loaded ", count, stage, " hunks in ", path),
    }
}

pub(crate) fn git_hunk_list_failure_status(
    stage: GitChangeStage,
    path: &Path,
    error: &str,
) -> String {
    hunk_failure_status(
        "Could not load ",
        hunk_stage_label(stage),
        " hunks in ",
        path,
        error,
    )
}

pub(crate) fn git_hunk_target_loading_status(
    stage: GitChangeStage,
    path: &Path,
    action: &str,
) -> String {
    let path = display_path_label_cow(path);
    let stage = hunk_stage_label(stage);
    let mut status = String::with_capacity(
        "Loading ".len()
            + stage.len()
            + " hunks in ".len()
            + path.len()
            + "; retry ".len()
            + action.len()
            + " after they finish".len(),
    );
    status.push_str("Loading ");
    status.push_str(stage);
    status.push_str(" hunks in ");
    status.push_str(&path);
    status.push_str("; retry ");
    status.push_str(action);
    status.push_str(" after they finish");
    status
}

pub(crate) fn git_hunk_stage_pending_status(path: &Path, hunk_index: usize) -> String {
    hunk_action_status("Staging", path, hunk_index)
}

pub(crate) fn git_hunk_stage_success_status(path: &Path, hunk_index: usize) -> String {
    hunk_action_status("Staged", path, hunk_index)
}

pub(crate) fn git_hunk_stage_missing_identity_status(path: &Path, hunk_index: usize) -> String {
    hunk_reload_before_status(path, "staging", hunk_index)
}

pub(crate) fn git_hunk_stage_failure_status(path: &Path, hunk_index: usize, error: &str) -> String {
    hunk_action_failure_status("stage", path, hunk_index, error)
}

pub(crate) fn git_hunk_unstage_pending_status(path: &Path, hunk_index: usize) -> String {
    hunk_action_status("Unstaging", path, hunk_index)
}

pub(crate) fn git_hunk_unstage_success_status(path: &Path, hunk_index: usize) -> String {
    hunk_action_status("Unstaged", path, hunk_index)
}

pub(crate) fn git_hunk_unstage_missing_identity_status(path: &Path, hunk_index: usize) -> String {
    hunk_reload_before_status(path, "unstaging", hunk_index)
}

pub(crate) fn git_hunk_unstage_failure_status(
    path: &Path,
    hunk_index: usize,
    error: &str,
) -> String {
    hunk_action_failure_status("unstage", path, hunk_index, error)
}

pub(crate) fn git_hunk_discard_pending_status(path: &Path, hunk_index: usize) -> String {
    hunk_action_status("Discarding", path, hunk_index)
}

pub(crate) fn git_hunk_discard_dirty_buffer_status(path: &Path) -> String {
    hunk_status_with_path("Save or reload ", "", "", path, " before discarding hunks")
}

pub(crate) fn git_hunk_discard_missing_identity_status(path: &Path, hunk_index: usize) -> String {
    hunk_reload_before_status(path, "discarding", hunk_index)
}

pub(crate) fn hunk_stage_label(stage: GitChangeStage) -> &'static str {
    match stage {
        GitChangeStage::Staged => "staged",
        GitChangeStage::Unstaged => "unstaged",
    }
}

pub(crate) fn git_hunk_discard_success_status(path: &Path, hunk_index: usize) -> String {
    hunk_action_status("Discarded", path, hunk_index)
}

pub(crate) fn git_hunk_discard_failure_status(
    path: &Path,
    hunk_index: usize,
    error: &str,
) -> String {
    hunk_action_failure_status("discard", path, hunk_index, error)
}

fn hunk_status_with_path(
    prefix: &str,
    stage: &str,
    between: &str,
    path: &Path,
    suffix: &str,
) -> String {
    let path = display_path_label_cow(path);
    let mut status = String::with_capacity(
        prefix.len() + stage.len() + between.len() + path.len() + suffix.len(),
    );
    status.push_str(prefix);
    status.push_str(stage);
    status.push_str(between);
    status.push_str(&path);
    status.push_str(suffix);
    status
}

fn hunk_count_status(
    prefix: &str,
    count: usize,
    stage: &str,
    between: &str,
    path: &Path,
) -> String {
    let path = display_path_label_cow(path);
    let mut status = String::with_capacity(
        prefix.len() + decimal_len(count) + 1 + stage.len() + between.len() + path.len(),
    );
    status.push_str(prefix);
    let _ = write!(status, "{count}");
    status.push(' ');
    status.push_str(stage);
    status.push_str(between);
    status.push_str(&path);
    status
}

fn hunk_failure_status(
    prefix: &str,
    stage: &str,
    between: &str,
    path: &Path,
    error: &str,
) -> String {
    let path = display_path_label_cow(path);
    let error = display_error_label_cow(error);
    let mut status = String::with_capacity(
        prefix.len() + stage.len() + between.len() + path.len() + ": ".len() + error.len(),
    );
    status.push_str(prefix);
    status.push_str(stage);
    status.push_str(between);
    status.push_str(&path);
    status.push_str(": ");
    status.push_str(&error);
    status
}

fn hunk_action_status(action: &str, path: &Path, hunk_index: usize) -> String {
    let path = display_path_label_cow(path);
    let mut status = String::with_capacity(
        action.len() + " hunk ".len() + decimal_len(hunk_index) + " in ".len() + path.len(),
    );
    status.push_str(action);
    status.push_str(" hunk ");
    let _ = write!(status, "{hunk_index}");
    status.push_str(" in ");
    status.push_str(&path);
    status
}

fn hunk_reload_before_status(path: &Path, action: &str, hunk_index: usize) -> String {
    let path = display_path_label_cow(path);
    let mut status = String::with_capacity(
        "Reload hunks in ".len()
            + path.len()
            + " before ".len()
            + action.len()
            + " hunk ".len()
            + decimal_len(hunk_index),
    );
    status.push_str("Reload hunks in ");
    status.push_str(&path);
    status.push_str(" before ");
    status.push_str(action);
    status.push_str(" hunk ");
    let _ = write!(status, "{hunk_index}");
    status
}

fn hunk_action_failure_status(action: &str, path: &Path, hunk_index: usize, error: &str) -> String {
    let path = display_path_label_cow(path);
    let error = display_error_label_cow(error);
    let mut status = String::with_capacity(
        "Could not ".len()
            + action.len()
            + " hunk ".len()
            + decimal_len(hunk_index)
            + " in ".len()
            + path.len()
            + ": ".len()
            + error.len(),
    );
    status.push_str("Could not ");
    status.push_str(action);
    status.push_str(" hunk ");
    let _ = write!(status, "{hunk_index}");
    status.push_str(" in ");
    status.push_str(&path);
    status.push_str(": ");
    status.push_str(&error);
    status
}

fn decimal_len(mut value: usize) -> usize {
    let mut len = 1;
    while value >= 10 {
        len += 1;
        value /= 10;
    }
    len
}

#[cfg(test)]
mod tests;
