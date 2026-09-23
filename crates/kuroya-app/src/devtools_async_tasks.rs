mod details;

use crate::{
    KuroyaApp, devtools_trace_id::next_devtools_trace_id,
    source_control_diff_runtime::source_control_diff_open_detail,
    source_control_patch_runtime::source_control_patch_copy_detail, ui_events::UiEvent,
    virtual_diff_runtime::virtual_diff_open_detail,
    virtual_revision_runtime::virtual_revision_task_detail,
};
use details::{
    async_task_detail, bounded_async_task_text, branch_detail, explorer_action_detail,
    explorer_operation_detail, failed, finished, git_commit_task_detail, query_detail,
};
#[cfg(test)]
use details::{
    async_task_detail_cow, async_task_detail_with_max, async_task_detail_with_max_cow,
    bounded_async_task_text_cow,
};
pub(crate) use details::{
    branch_operation_detail, branch_rename_detail, file_reload_task_detail, git_scan_task_detail,
    hunk_detail, index_detail, path_detail, paths_detail, plugin_command_task_detail,
};
use eframe::egui::{self, RichText};
use std::{
    collections::VecDeque,
    fmt::Write as _,
    time::{Duration, Instant},
};

pub(crate) const MAX_ASYNC_TASK_TRACE_ENTRIES: usize = 160;
pub(crate) const MAX_ACTIVE_ASYNC_TASKS: usize = 80;
pub(crate) const MAX_ASYNC_TASK_NAME_CHARS: usize = 120;
pub(crate) const MAX_ASYNC_TASK_DETAIL_CHARS: usize = 512;
pub(crate) const MAX_ASYNC_TASK_ELAPSED_MS: f32 = 3_600_000.0;
const DEVTOOLS_MONOSPACE_ROW_HEIGHT: f32 = 18.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncTaskOutcome {
    Started,
    Finished,
    Failed,
}

#[derive(Debug, Clone)]
pub(crate) struct AsyncTaskActiveEntry {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) detail: String,
    pub(crate) started_at: Instant,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AsyncTaskTraceEntry {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) detail: String,
    pub(crate) outcome: AsyncTaskOutcome,
    pub(crate) elapsed_ms: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AsyncTaskDiagnosticStats {
    pub(crate) active_count: usize,
    pub(crate) trace_count: usize,
    pub(crate) started_count: usize,
    pub(crate) finished_count: usize,
    pub(crate) failed_count: usize,
    pub(crate) unmatched_outcome_count: usize,
    pub(crate) average_elapsed_ms: Option<f32>,
    pub(crate) max_elapsed_ms: Option<f32>,
}

impl KuroyaApp {
    pub(crate) fn record_async_task_started(
        &mut self,
        name: impl Into<String>,
        detail: impl Into<String>,
    ) {
        let id = next_devtools_trace_id(&mut self.next_async_task_id);
        let name = name.into();
        let detail = detail.into();
        let name = bounded_async_task_text(&name, MAX_ASYNC_TASK_NAME_CHARS);
        let detail = bounded_async_task_text(&detail, MAX_ASYNC_TASK_DETAIL_CHARS);
        let started_at = Instant::now();
        if self.settings.devtools_verbose_logging {
            self.record_verbose_log("async", format!("started {name} {detail}"));
        }
        push_active_async_task_sanitized(
            &mut self.active_async_tasks,
            AsyncTaskActiveEntry {
                id,
                name: name.clone(),
                detail: detail.clone(),
                started_at,
            },
            MAX_ACTIVE_ASYNC_TASKS,
        );
        record_async_task_trace_entry_sanitized(
            &mut self.async_task_trace,
            AsyncTaskTraceEntry {
                id,
                name,
                detail,
                outcome: AsyncTaskOutcome::Started,
                elapsed_ms: None,
            },
            MAX_ASYNC_TASK_TRACE_ENTRIES,
        );
    }

    pub(crate) fn record_async_task_ui_event(&mut self, event: &UiEvent) {
        let Some(label) = async_task_event_label(event) else {
            return;
        };
        self.record_async_task_outcome(label.name, label.detail, label.outcome);
    }

    fn record_async_task_outcome(
        &mut self,
        name: impl Into<String>,
        detail: impl Into<String>,
        outcome: AsyncTaskOutcome,
    ) {
        let name = name.into();
        let detail = detail.into();
        let name = bounded_async_task_text(&name, MAX_ASYNC_TASK_NAME_CHARS);
        let detail = bounded_async_task_text(&detail, MAX_ASYNC_TASK_DETAIL_CHARS);
        let finished = finish_matching_async_task_sanitized(
            &mut self.active_async_tasks,
            &name,
            &detail,
            Instant::now(),
        );
        let id = finished
            .as_ref()
            .map(|finished| finished.entry.id)
            .unwrap_or_else(|| next_devtools_trace_id(&mut self.next_async_task_id));
        let elapsed_ms = finished
            .as_ref()
            .map(|finished| duration_ms(finished.elapsed));
        if let Some(finished) = finished.as_ref().filter(|_| self.profiling_enabled()) {
            self.record_profile_sample("async", format!("{name} {detail}"), finished.elapsed);
        }
        if self.settings.devtools_verbose_logging {
            let mut message = String::with_capacity(24 + name.len() + detail.len());
            message.push_str(outcome.label());
            if let Some(elapsed) = elapsed_ms {
                let _ = write!(message, " {:.1} ms", elapsed);
            }
            let _ = write!(message, " {name} {detail}");
            self.record_verbose_log("async", message);
        }
        record_async_task_trace_entry_sanitized(
            &mut self.async_task_trace,
            AsyncTaskTraceEntry {
                id,
                name,
                detail,
                outcome,
                elapsed_ms,
            },
            MAX_ASYNC_TASK_TRACE_ENTRIES,
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AsyncTaskEventLabel {
    pub(crate) name: &'static str,
    pub(crate) detail: String,
    pub(crate) outcome: AsyncTaskOutcome,
}

#[derive(Debug)]
pub(crate) struct FinishedAsyncTask {
    pub(crate) entry: AsyncTaskActiveEntry,
    pub(crate) elapsed: Duration,
}

pub(crate) fn async_task_event_label(event: &UiEvent) -> Option<AsyncTaskEventLabel> {
    match event {
        UiEvent::CachedIndex { .. } => None,
        UiEvent::Indexed { root, .. } => Some(finished("Index Workspace", path_detail(root))),
        UiEvent::FileLoaded { path, .. } | UiEvent::ImageFileLoaded { path, .. } => {
            Some(finished("File Load", path_detail(path)))
        }
        UiEvent::FileLoadFailed { path, .. } => Some(failed("File Load", path_detail(path))),
        UiEvent::FileReloaded {
            request_id, path, ..
        }
        | UiEvent::ImageFileReloaded {
            request_id, path, ..
        } => Some(finished(
            "File Reload",
            file_reload_task_detail(*request_id, path),
        )),
        UiEvent::FileReloadFailed {
            request_id, path, ..
        } => Some(failed(
            "File Reload",
            file_reload_task_detail(*request_id, path),
        )),
        UiEvent::FileSaved { path, .. } => Some(finished("File Save", path_detail(path))),
        UiEvent::FileSaveFailed { path, .. } => Some(failed("File Save", path_detail(path))),
        UiEvent::LocalHistoryLoaded { path, .. } => {
            Some(finished("Local History", path_detail(path)))
        }
        UiEvent::LocalHistoryFailed { path, .. } => {
            Some(failed("Local History", path_detail(path)))
        }
        UiEvent::LocalHistoryBrowserLoaded { path, .. } => {
            Some(finished("Local History Browser", path_detail(path)))
        }
        UiEvent::LocalHistoryBrowserSnapshotLoaded { path, result, .. } => match result {
            Ok(_) => Some(finished("Local History", path_detail(path))),
            Err(_) => Some(failed("Local History", path_detail(path))),
        },
        UiEvent::SessionSaved { root } => Some(finished("Session Save", path_detail(root))),
        UiEvent::SessionSaveFailed { root, .. } => Some(failed("Session Save", path_detail(root))),
        UiEvent::OpenWorkspacePicked { .. }
        | UiEvent::OpenWorkspacePickerCanceled { .. }
        | UiEvent::OpenWorkspacePickerFailed { .. }
        | UiEvent::SettingsFontPicked { .. }
        | UiEvent::SettingsFontPickerCanceled { .. }
        | UiEvent::SettingsFontPickerFailed { .. }
        | UiEvent::SettingsBackgroundImagePicked { .. }
        | UiEvent::SettingsBackgroundImagePickerCanceled { .. }
        | UiEvent::SettingsBackgroundImagePickerFailed { .. }
        | UiEvent::EditorBackgroundImageLoaded { .. }
        | UiEvent::EditorBackgroundImageLoadFailed { .. }
        | UiEvent::ExplorerCreatePathPicked { .. }
        | UiEvent::ExplorerCreatePathPickerCanceled { .. }
        | UiEvent::ExplorerCreatePathPickerFailed { .. } => None,
        UiEvent::ExplorerOperationFinished { operation, .. } => Some(finished(
            "Explorer Operation",
            explorer_operation_detail(operation),
        )),
        UiEvent::ExplorerOperationFailed { action, .. } => {
            Some(failed("Explorer Operation", explorer_action_detail(action)))
        }
        UiEvent::SearchProgress { .. } => None,
        UiEvent::SearchFinished { query, .. } => {
            Some(finished("Project Search", query_detail(query)))
        }
        UiEvent::GitScanned {
            request_id, root, ..
        } => Some(finished(
            "Git Scan",
            git_scan_task_detail(*request_id, root),
        )),
        UiEvent::GitStageFinished { paths, .. } => Some(finished("Git Stage", paths_detail(paths))),
        UiEvent::GitStageFailed { paths, .. } => Some(failed("Git Stage", paths_detail(paths))),
        UiEvent::GitUnstageFinished { paths, .. } => {
            Some(finished("Git Unstage", paths_detail(paths)))
        }
        UiEvent::GitUnstageFailed { paths, .. } => Some(failed("Git Unstage", paths_detail(paths))),
        UiEvent::GitDiscardFinished { paths, .. } => {
            Some(finished("Git Discard", paths_detail(paths)))
        }
        UiEvent::GitDiscardFailed { paths, .. } => Some(failed("Git Discard", paths_detail(paths))),
        UiEvent::GitCommitFinished { smart_commit, .. } => Some(finished(
            "Git Commit",
            git_commit_task_detail(*smart_commit),
        )),
        UiEvent::GitCommitFailed { smart_commit, .. } => {
            Some(failed("Git Commit", git_commit_task_detail(*smart_commit)))
        }
        UiEvent::GitBranchesLoaded { root, .. } => {
            Some(finished("Git Branches", path_detail(root)))
        }
        UiEvent::GitBranchesFailed { root, .. } => Some(failed("Git Branches", path_detail(root))),
        UiEvent::GitBranchSwitchFinished {
            request_id, branch, ..
        } => Some(finished(
            "Git Branch Switch",
            branch_operation_detail(*request_id, branch),
        )),
        UiEvent::GitBranchSwitchFailed {
            request_id, branch, ..
        } => Some(failed(
            "Git Branch Switch",
            branch_operation_detail(*request_id, branch),
        )),
        UiEvent::GitBranchCreateFinished {
            request_id, branch, ..
        } => Some(finished(
            "Git Branch Create",
            branch_operation_detail(*request_id, branch),
        )),
        UiEvent::GitBranchCreateFailed {
            request_id, branch, ..
        } => Some(failed(
            "Git Branch Create",
            branch_operation_detail(*request_id, branch),
        )),
        UiEvent::GitBranchDeleteFinished { branch, .. } => {
            Some(finished("Git Branch Delete", branch_detail(branch)))
        }
        UiEvent::GitBranchDeleteFailed { branch, .. } => {
            Some(failed("Git Branch Delete", branch_detail(branch)))
        }
        UiEvent::GitBranchRenameFinished {
            old_branch,
            new_branch,
            ..
        } => Some(finished(
            "Git Branch Rename",
            branch_rename_detail(old_branch, new_branch),
        )),
        UiEvent::GitBranchRenameFailed {
            old_branch,
            new_branch,
            ..
        } => Some(failed(
            "Git Branch Rename",
            branch_rename_detail(old_branch, new_branch),
        )),
        UiEvent::GitHistoryLoaded { root, .. } => Some(finished("Git History", path_detail(root))),
        UiEvent::GitHistoryFailed { root, .. } => Some(failed("Git History", path_detail(root))),
        UiEvent::GitBlameLoaded { path, .. } => Some(finished("Git Blame", path_detail(path))),
        UiEvent::GitBlameFailed { path, .. } => Some(failed("Git Blame", path_detail(path))),
        UiEvent::GitStashesLoaded { root, .. } => Some(finished("Git Stashes", path_detail(root))),
        UiEvent::GitStashesFailed { root, .. } => Some(failed("Git Stashes", path_detail(root))),
        UiEvent::GitStashSaved { .. } => {
            Some(finished("Git Stash Save", async_task_detail("worktree")))
        }
        UiEvent::GitStashSaveFailed { .. } => {
            Some(failed("Git Stash Save", async_task_detail("worktree")))
        }
        UiEvent::GitStashApplied { index, .. } => {
            Some(finished("Git Stash Apply", index_detail(*index)))
        }
        UiEvent::GitStashApplyFailed { index, .. } => {
            Some(failed("Git Stash Apply", index_detail(*index)))
        }
        UiEvent::GitStashPopped { index, .. } => {
            Some(finished("Git Stash Pop", index_detail(*index)))
        }
        UiEvent::GitStashPopFailed { index, .. } => {
            Some(failed("Git Stash Pop", index_detail(*index)))
        }
        UiEvent::GitStashDropped { index, .. } => {
            Some(finished("Git Stash Drop", index_detail(*index)))
        }
        UiEvent::GitStashDropFailed { index, .. } => {
            Some(failed("Git Stash Drop", index_detail(*index)))
        }
        UiEvent::GitHunksLoaded { path, .. } => Some(finished("Git Hunks", path_detail(path))),
        UiEvent::GitHunksFailed { path, .. } => Some(failed("Git Hunks", path_detail(path))),
        UiEvent::GitHunkStaged {
            path, hunk_index, ..
        } => Some(finished("Git Hunk Stage", hunk_detail(path, *hunk_index))),
        UiEvent::GitHunkStageFailed {
            path, hunk_index, ..
        } => Some(failed("Git Hunk Stage", hunk_detail(path, *hunk_index))),
        UiEvent::GitHunkUnstaged {
            path, hunk_index, ..
        } => Some(finished("Git Hunk Unstage", hunk_detail(path, *hunk_index))),
        UiEvent::GitHunkUnstageFailed {
            path, hunk_index, ..
        } => Some(failed("Git Hunk Unstage", hunk_detail(path, *hunk_index))),
        UiEvent::GitHunkDiscarded {
            path, hunk_index, ..
        } => Some(finished("Git Hunk Discard", hunk_detail(path, *hunk_index))),
        UiEvent::GitHunkDiscardFailed {
            path, hunk_index, ..
        } => Some(failed("Git Hunk Discard", hunk_detail(path, *hunk_index))),
        UiEvent::GitPatchCopyFinished {
            request, result, ..
        } => {
            let label = if result.is_ok() { finished } else { failed };
            Some(label(
                "Git Patch Copy",
                source_control_patch_copy_detail(request),
            ))
        }
        UiEvent::SourceControlDiffOpenFinished {
            request, result, ..
        } => {
            let label = if result.is_ok() { finished } else { failed };
            Some(label(
                "Git Diff Open",
                source_control_diff_open_detail(request),
            ))
        }
        UiEvent::VirtualDiffOpenFinished {
            request, result, ..
        } => {
            let label = if result.is_ok() { finished } else { failed };
            Some(label("Virtual Diff", virtual_diff_open_detail(request)))
        }
        UiEvent::VirtualRevisionOpenFinished {
            request_id,
            request,
            result,
            ..
        } => {
            let label = if result.is_ok() { finished } else { failed };
            Some(label(
                "Virtual Revision",
                virtual_revision_task_detail(*request_id, request),
            ))
        }
        UiEvent::FallbackFoldingRangesLoaded { path, .. } => {
            Some(finished("Fallback Folding", path_detail(path)))
        }
        UiEvent::EditorDiffLinesComputed { path, .. } => {
            Some(finished("Editor Diff Lines", path_detail(path)))
        }
        UiEvent::WorkspaceTasksLoaded { root, .. } => {
            Some(finished("Workspace Tasks", path_detail(root)))
        }
        UiEvent::WorkspaceTasksFailed { root, .. } => {
            Some(failed("Workspace Tasks", path_detail(root)))
        }
        UiEvent::WorkspacePluginsLoaded { root, .. } => {
            Some(finished("Workspace Plugins", path_detail(root)))
        }
        UiEvent::WorkspacePluginsFailed { root, .. } => {
            Some(failed("Workspace Plugins", path_detail(root)))
        }
        UiEvent::PluginCommandFinished {
            command_id, result, ..
        } => {
            let label = if result.is_ok() { finished } else { failed };
            Some(label(
                "Plugin Command",
                plugin_command_task_detail(command_id),
            ))
        }
        UiEvent::DiagnosticsComputed { path, .. } => {
            Some(finished("Static Diagnostics", path_detail(path)))
        }
        UiEvent::UpdateCheckFinished(_) => Some(finished(
            "Update Check",
            async_task_detail("GitHub Releases"),
        )),
        UiEvent::UpdateCheckFailed { .. } => {
            Some(failed("Update Check", async_task_detail("GitHub Releases")))
        }
        UiEvent::UpdateInstallerReady(update) => Some(finished(
            "Update Download",
            async_task_detail(&update.latest_version),
        )),
        UiEvent::UpdateDownloadProgress { .. } => None,
        UiEvent::UpdateDownloadFailed { available, .. } => Some(failed(
            "Update Download",
            async_task_detail(&available.latest_version),
        )),
        UiEvent::QuickOpenRanked { .. } => None,
        UiEvent::PluginOpenFileRequested { .. } => None,
        UiEvent::PluginBufferTextApply { .. } => None,
        UiEvent::ExplorerDirectoryLoaded { .. } => None,
        UiEvent::StartupSessionLoaded { .. } => None,
        UiEvent::Lsp(_) => None,
    }
}

#[cfg(test)]
pub(crate) fn finish_matching_async_task(
    active: &mut VecDeque<AsyncTaskActiveEntry>,
    name: &str,
    detail: &str,
    now: Instant,
) -> Option<FinishedAsyncTask> {
    let name = bounded_async_task_text(name, MAX_ASYNC_TASK_NAME_CHARS);
    let detail = bounded_async_task_text(detail, MAX_ASYNC_TASK_DETAIL_CHARS);
    finish_matching_async_task_sanitized(active, &name, &detail, now)
}

fn finish_matching_async_task_sanitized(
    active: &mut VecDeque<AsyncTaskActiveEntry>,
    name: &str,
    detail: &str,
    now: Instant,
) -> Option<FinishedAsyncTask> {
    let index = active
        .iter()
        .position(|entry| entry.name == name && entry.detail == detail)?;
    let entry = active.remove(index)?;
    Some(FinishedAsyncTask {
        elapsed: now.saturating_duration_since(entry.started_at),
        entry,
    })
}

#[cfg(test)]
pub(crate) fn record_async_task_trace_entry(
    entries: &mut VecDeque<AsyncTaskTraceEntry>,
    mut entry: AsyncTaskTraceEntry,
    max_entries: usize,
) {
    entry.name = bounded_async_task_text(&entry.name, MAX_ASYNC_TASK_NAME_CHARS);
    entry.detail = bounded_async_task_text(&entry.detail, MAX_ASYNC_TASK_DETAIL_CHARS);
    record_async_task_trace_entry_sanitized(entries, entry, max_entries);
}

fn record_async_task_trace_entry_sanitized(
    entries: &mut VecDeque<AsyncTaskTraceEntry>,
    mut entry: AsyncTaskTraceEntry,
    max_entries: usize,
) {
    entry.elapsed_ms = entry.elapsed_ms.map(bounded_async_elapsed_ms);
    push_bounded(entries, entry, max_entries);
}

#[cfg(test)]
pub(crate) fn push_active_async_task(
    active: &mut VecDeque<AsyncTaskActiveEntry>,
    mut entry: AsyncTaskActiveEntry,
    max_entries: usize,
) {
    entry.name = bounded_async_task_text(&entry.name, MAX_ASYNC_TASK_NAME_CHARS);
    entry.detail = bounded_async_task_text(&entry.detail, MAX_ASYNC_TASK_DETAIL_CHARS);
    push_active_async_task_sanitized(active, entry, max_entries);
}

fn push_active_async_task_sanitized(
    active: &mut VecDeque<AsyncTaskActiveEntry>,
    entry: AsyncTaskActiveEntry,
    max_entries: usize,
) {
    push_bounded(active, entry, max_entries);
}

pub(crate) fn render_async_task_panel(
    ui: &mut egui::Ui,
    active: &VecDeque<AsyncTaskActiveEntry>,
    trace: &VecDeque<AsyncTaskTraceEntry>,
) {
    ui.label(RichText::new("Async Tasks").strong());
    ui.label(format!("{} active", active.len()));
    if let Some(stats) = async_task_diagnostic_stats(active, trace) {
        ui.horizontal(|ui| {
            ui.label(format!("{} traced", stats.trace_count));
            ui.label(format!("{} finished", stats.finished_count));
            ui.label(format!("{} failed", stats.failed_count));
            if stats.unmatched_outcome_count > 0 {
                ui.label(format!("{} unmatched", stats.unmatched_outcome_count));
            }
            if let Some(elapsed) = stats.average_elapsed_ms {
                ui.label(format!("Avg {:.1} ms", elapsed));
            }
            if let Some(elapsed) = stats.max_elapsed_ms {
                ui.label(format!("Max {:.1} ms", elapsed));
            }
        });
    }

    if !active.is_empty() {
        let now = Instant::now();
        let row_height = devtools_row_height(ui);
        egui::ScrollArea::vertical()
            .max_height(92.0)
            .auto_shrink([false, false])
            .show_rows(ui, row_height, active.len(), |ui, rows| {
                let len = active.len();
                for display_index in rows {
                    let Some(entry) = reversed_vec_deque_item(active, len, display_index) else {
                        continue;
                    };
                    let mut row = String::with_capacity(32 + entry.name.len() + entry.detail.len());
                    let _ = write!(
                        row,
                        "#{:04} running {:.1} ms {} {}",
                        entry.id,
                        bounded_async_elapsed_ms(duration_ms(
                            now.saturating_duration_since(entry.started_at)
                        )),
                        entry.name,
                        entry.detail
                    );
                    ui.monospace(row);
                }
            });
    }

    if trace.is_empty() {
        ui.label(RichText::new("No async tasks recorded yet").small());
        return;
    }

    let row_height = devtools_row_height(ui);
    egui::ScrollArea::vertical()
        .max_height(180.0)
        .auto_shrink([false, false])
        .show_rows(ui, row_height, trace.len(), |ui, rows| {
            let len = trace.len();
            for display_index in rows {
                let Some(entry) = reversed_vec_deque_item(trace, len, display_index) else {
                    continue;
                };
                let mut row = String::with_capacity(32 + entry.name.len() + entry.detail.len());
                let _ = write!(row, "#{:04} {}", entry.id, entry.outcome.label());
                if let Some(elapsed) = entry.elapsed_ms.map(bounded_async_elapsed_ms) {
                    let _ = write!(row, " {:.1} ms", elapsed);
                }
                let _ = write!(row, " {} {}", entry.name, entry.detail);
                ui.monospace(row);
            }
        });
}

fn devtools_row_height(ui: &egui::Ui) -> f32 {
    ui.spacing()
        .interact_size
        .y
        .max(DEVTOOLS_MONOSPACE_ROW_HEIGHT)
}

fn reversed_vec_deque_item<T>(items: &VecDeque<T>, len: usize, display_index: usize) -> Option<&T> {
    len.checked_sub(display_index + 1)
        .and_then(|index| items.get(index))
}

pub(crate) fn async_task_diagnostic_stats(
    active: &VecDeque<AsyncTaskActiveEntry>,
    trace: &VecDeque<AsyncTaskTraceEntry>,
) -> Option<AsyncTaskDiagnosticStats> {
    if active.is_empty() && trace.is_empty() {
        return None;
    }

    let mut started_count = 0usize;
    let mut finished_count = 0usize;
    let mut failed_count = 0usize;
    let mut unmatched_outcome_count = 0usize;
    let mut elapsed_sum = 0.0;
    let mut elapsed_count = 0usize;
    let mut max_elapsed_ms = 0.0_f32;

    for entry in trace {
        match entry.outcome {
            AsyncTaskOutcome::Started => started_count += 1,
            AsyncTaskOutcome::Finished => {
                finished_count += 1;
                if entry.elapsed_ms.is_none() {
                    unmatched_outcome_count += 1;
                }
            }
            AsyncTaskOutcome::Failed => {
                failed_count += 1;
                if entry.elapsed_ms.is_none() {
                    unmatched_outcome_count += 1;
                }
            }
        }
        if let Some(elapsed_ms) = entry.elapsed_ms {
            let elapsed_ms = bounded_async_elapsed_ms(elapsed_ms);
            elapsed_sum += elapsed_ms;
            elapsed_count += 1;
            max_elapsed_ms = max_elapsed_ms.max(elapsed_ms);
        }
    }

    Some(AsyncTaskDiagnosticStats {
        active_count: active.len(),
        trace_count: trace.len(),
        started_count,
        finished_count,
        failed_count,
        unmatched_outcome_count,
        average_elapsed_ms: (elapsed_count > 0).then_some(elapsed_sum / elapsed_count as f32),
        max_elapsed_ms: (elapsed_count > 0).then_some(max_elapsed_ms),
    })
}

fn push_bounded<T>(items: &mut VecDeque<T>, item: T, max_items: usize) {
    if max_items == 0 {
        items.clear();
        return;
    }
    while items.len() >= max_items {
        items.pop_front();
    }
    items.push_back(item);
}

fn bounded_async_elapsed_ms(elapsed_ms: f32) -> f32 {
    if elapsed_ms.is_nan() {
        0.0
    } else {
        elapsed_ms.clamp(0.0, MAX_ASYNC_TASK_ELAPSED_MS)
    }
}

fn duration_ms(duration: Duration) -> f32 {
    duration.as_secs_f32() * 1000.0
}

impl AsyncTaskOutcome {
    fn label(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Finished => "finished",
            Self::Failed => "failed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AsyncTaskActiveEntry, AsyncTaskEventLabel, AsyncTaskOutcome, AsyncTaskTraceEntry,
        MAX_ASYNC_TASK_DETAIL_CHARS, MAX_ASYNC_TASK_ELAPSED_MS, MAX_ASYNC_TASK_NAME_CHARS,
        async_task_diagnostic_stats, async_task_event_label, branch_rename_detail,
        file_reload_task_detail, finish_matching_async_task, git_scan_task_detail, hunk_detail,
        index_detail, path_detail, paths_detail, plugin_command_task_detail,
        push_active_async_task, record_async_task_trace_entry,
    };
    use crate::{
        source_control_patch_runtime::{
            SourceControlPatchCopyOutcome, SourceControlPatchCopyRequest,
        },
        ui_events::UiEvent,
        virtual_revision_runtime::{
            VirtualRevisionOpenOutcome, VirtualRevisionOpenRequest, virtual_revision_task_detail,
        },
    };
    use kuroya_core::{GitChangeStage, SearchOptions, SearchResult, TextBuffer};
    use std::{
        borrow::Cow,
        collections::VecDeque,
        path::PathBuf,
        time::{Duration, Instant},
    };

    #[test]
    fn async_task_trace_entries_are_bounded() {
        let mut entries = VecDeque::new();
        record_async_task_trace_entry(
            &mut entries,
            AsyncTaskTraceEntry {
                id: 1,
                name: "One".to_owned(),
                detail: "a".to_owned(),
                outcome: AsyncTaskOutcome::Started,
                elapsed_ms: None,
            },
            2,
        );
        record_async_task_trace_entry(
            &mut entries,
            AsyncTaskTraceEntry {
                id: 2,
                name: "Two".to_owned(),
                detail: "b".to_owned(),
                outcome: AsyncTaskOutcome::Finished,
                elapsed_ms: Some(1.0),
            },
            2,
        );
        record_async_task_trace_entry(
            &mut entries,
            AsyncTaskTraceEntry {
                id: 3,
                name: "Three".to_owned(),
                detail: "c".to_owned(),
                outcome: AsyncTaskOutcome::Failed,
                elapsed_ms: Some(2.0),
            },
            2,
        );

        assert_eq!(
            entries
                .iter()
                .map(|entry| (entry.id, entry.name.as_str(), entry.detail.as_str()))
                .collect::<Vec<_>>(),
            vec![(2, "Two", "b"), (3, "Three", "c")]
        );
    }

    #[test]
    fn async_task_entries_normalize_and_cap_payload_text_and_elapsed() {
        let started_at = Instant::now();
        let mut active = VecDeque::new();
        let long_name = format!(
            "Name\u{202e} {}",
            "n".repeat(MAX_ASYNC_TASK_NAME_CHARS + 16)
        );
        let long_detail = format!(
            "alpha\r\n\tbeta\u{202e} {}",
            "d".repeat(MAX_ASYNC_TASK_DETAIL_CHARS + 32)
        );

        push_active_async_task(
            &mut active,
            AsyncTaskActiveEntry {
                id: 1,
                name: long_name.clone(),
                detail: long_detail.clone(),
                started_at,
            },
            8,
        );

        let active_entry = active.front().expect("active task should be stored");
        assert!(active_entry.name.chars().count() <= MAX_ASYNC_TASK_NAME_CHARS);
        assert!(active_entry.detail.chars().count() <= MAX_ASYNC_TASK_DETAIL_CHARS);
        assert!(!active_entry.detail.contains('\n'));
        assert!(!active_entry.detail.contains('\t'));
        assert!(
            !active_entry.name.chars().any(is_unsafe_display_char),
            "name should not include control or bidi formatting chars: {}",
            active_entry.name
        );
        assert!(
            !active_entry.detail.chars().any(is_unsafe_display_char),
            "detail should not include control or bidi formatting chars: {}",
            active_entry.detail
        );
        assert!(active_entry.name.contains("..."));
        assert!(active_entry.detail.contains("alpha beta"));
        assert!(active_entry.detail.contains("..."));

        let finished = finish_matching_async_task(
            &mut active,
            &long_name,
            &long_detail,
            started_at + Duration::from_millis(5),
        )
        .expect("sanitized task detail should still match");
        assert_eq!(finished.entry.id, 1);

        let mut trace = VecDeque::new();
        record_async_task_trace_entry(
            &mut trace,
            AsyncTaskTraceEntry {
                id: 2,
                name: "Trace\n\u{202e}Name".to_owned(),
                detail: long_detail,
                outcome: AsyncTaskOutcome::Finished,
                elapsed_ms: Some(f32::INFINITY),
            },
            8,
        );

        let trace_entry = trace.front().expect("trace entry should be stored");
        assert_eq!(trace_entry.name, "Trace Name");
        assert!(trace_entry.detail.chars().count() <= MAX_ASYNC_TASK_DETAIL_CHARS);
        assert_eq!(trace_entry.elapsed_ms, Some(MAX_ASYNC_TASK_ELAPSED_MS));
    }

    #[test]
    fn async_task_detail_cow_borrows_clean_ascii_and_unicode() {
        assert!(matches!(
            super::async_task_detail_cow("clean task detail"),
            Cow::Borrowed("clean task detail")
        ));
        assert!(matches!(
            super::bounded_async_task_text_cow("Clean Task", MAX_ASYNC_TASK_NAME_CHARS),
            Cow::Borrowed("Clean Task")
        ));

        let unicode = "clean-\u{03bb} detail";
        match super::async_task_detail_cow(unicode) {
            Cow::Borrowed(label) => assert_eq!(label, unicode),
            Cow::Owned(label) => panic!("expected borrowed detail, got {label:?}"),
        }
        match super::bounded_async_task_text_cow(unicode, MAX_ASYNC_TASK_DETAIL_CHARS) {
            Cow::Borrowed(label) => assert_eq!(label, unicode),
            Cow::Owned(label) => panic!("expected borrowed bounded text, got {label:?}"),
        }
    }

    #[test]
    fn async_task_detail_cow_owns_dirty_truncated_and_fallback_output() {
        assert_owned_cow(
            super::async_task_detail_with_max_cow("alpha\r\n\tbeta\u{202e}", 64, "fallback"),
            "alpha beta",
        );
        assert_owned_cow(
            super::async_task_detail_with_max_cow("abcdefghijklmnopqrstuvwxyz", 12, "fallback"),
            "abcd...vwxyz",
        );
        assert_owned_cow(
            super::async_task_detail_with_max_cow(" \r\n\t", 64, "fallback detail"),
            "fallback detail",
        );
        assert_owned_cow(super::async_task_detail_cow(" \r\n\t"), "task detail");
    }

    #[test]
    fn bounded_async_task_text_keeps_blank_display_text_blank() {
        assert_eq!(
            super::bounded_async_task_text(" \r\n\t", MAX_ASYNC_TASK_NAME_CHARS),
            ""
        );
        assert_eq!(
            super::bounded_async_task_text("\u{202e}\u{200f}\r\n\t", MAX_ASYNC_TASK_DETAIL_CHARS),
            ""
        );
    }

    #[test]
    fn bounded_async_task_text_cow_keeps_blank_fallback_blank() {
        assert_owned_cow(
            super::bounded_async_task_text_cow(" \r\n\t", MAX_ASYNC_TASK_NAME_CHARS),
            "",
        );
        assert_owned_cow(
            super::bounded_async_task_text_cow(
                "\u{202e}\u{200f}\r\n\t",
                MAX_ASYNC_TASK_DETAIL_CHARS,
            ),
            "",
        );
    }

    #[test]
    fn async_task_sanitizer_wrappers_match_cow_helpers() {
        let cases = [
            ("clean task", MAX_ASYNC_TASK_DETAIL_CHARS, "task detail"),
            ("alpha\r\n\tbeta\u{202e}", 64, "fallback detail"),
            ("abcdefghijklmnopqrstuvwxyz", 12, "fallback detail"),
            (" \r\n\t", 64, "fallback detail"),
        ];

        for (value, max_chars, fallback) in cases {
            assert_eq!(
                super::async_task_detail_with_max(value, max_chars, fallback),
                super::async_task_detail_with_max_cow(value, max_chars, fallback).as_ref()
            );
        }
        assert_eq!(
            super::async_task_detail("worktree"),
            super::async_task_detail_cow("worktree").as_ref()
        );
        assert_eq!(
            super::bounded_async_task_text("Task Name", MAX_ASYNC_TASK_NAME_CHARS),
            super::bounded_async_task_text_cow("Task Name", MAX_ASYNC_TASK_NAME_CHARS).as_ref()
        );
    }

    #[test]
    fn matching_async_task_finish_removes_active_entry_and_reports_elapsed() {
        let started_at = Instant::now();
        let mut active = VecDeque::from([AsyncTaskActiveEntry {
            id: 7,
            name: "Project Search".to_owned(),
            detail: "`needle`".to_owned(),
            started_at,
        }]);

        let finished = finish_matching_async_task(
            &mut active,
            "Project Search",
            "`needle`",
            started_at + Duration::from_millis(25),
        )
        .expect("task should match");

        assert!(active.is_empty());
        assert_eq!(finished.entry.id, 7);
        assert_eq!(finished.elapsed, Duration::from_millis(25));
    }

    #[test]
    fn async_task_diagnostic_stats_summarize_trace_health() {
        let started_at = Instant::now();
        let active = VecDeque::from([AsyncTaskActiveEntry {
            id: 9,
            name: "Index Workspace".to_owned(),
            detail: "workspace".to_owned(),
            started_at,
        }]);
        let trace = VecDeque::from([
            AsyncTaskTraceEntry {
                id: 7,
                name: "Project Search".to_owned(),
                detail: "`needle`".to_owned(),
                outcome: AsyncTaskOutcome::Started,
                elapsed_ms: None,
            },
            AsyncTaskTraceEntry {
                id: 7,
                name: "Project Search".to_owned(),
                detail: "`needle`".to_owned(),
                outcome: AsyncTaskOutcome::Finished,
                elapsed_ms: Some(12.0),
            },
            AsyncTaskTraceEntry {
                id: 8,
                name: "Git Scan".to_owned(),
                detail: "workspace".to_owned(),
                outcome: AsyncTaskOutcome::Failed,
                elapsed_ms: Some(MAX_ASYNC_TASK_ELAPSED_MS + 18.0),
            },
        ]);

        let stats =
            async_task_diagnostic_stats(&active, &trace).expect("active or trace entries exist");

        assert_eq!(stats.active_count, 1);
        assert_eq!(stats.trace_count, 3);
        assert_eq!(stats.started_count, 1);
        assert_eq!(stats.finished_count, 1);
        assert_eq!(stats.failed_count, 1);
        assert_eq!(stats.unmatched_outcome_count, 0);
        assert_close(
            stats.average_elapsed_ms.unwrap(),
            (12.0 + MAX_ASYNC_TASK_ELAPSED_MS) / 2.0,
        );
        assert_close(stats.max_elapsed_ms.unwrap(), MAX_ASYNC_TASK_ELAPSED_MS);
    }

    #[test]
    fn async_task_diagnostic_stats_count_unmatched_outcomes() {
        let trace = VecDeque::from([
            AsyncTaskTraceEntry {
                id: 7,
                name: "Project Search".to_owned(),
                detail: "`needle`".to_owned(),
                outcome: AsyncTaskOutcome::Started,
                elapsed_ms: None,
            },
            AsyncTaskTraceEntry {
                id: 8,
                name: "Git Scan".to_owned(),
                detail: "workspace".to_owned(),
                outcome: AsyncTaskOutcome::Finished,
                elapsed_ms: None,
            },
            AsyncTaskTraceEntry {
                id: 9,
                name: "LSP".to_owned(),
                detail: "rust".to_owned(),
                outcome: AsyncTaskOutcome::Failed,
                elapsed_ms: None,
            },
            AsyncTaskTraceEntry {
                id: 10,
                name: "Index Workspace".to_owned(),
                detail: "workspace".to_owned(),
                outcome: AsyncTaskOutcome::Finished,
                elapsed_ms: Some(24.0),
            },
        ]);

        let stats =
            async_task_diagnostic_stats(&VecDeque::new(), &trace).expect("trace entries exist");

        assert_eq!(stats.started_count, 1);
        assert_eq!(stats.finished_count, 2);
        assert_eq!(stats.failed_count, 1);
        assert_eq!(stats.unmatched_outcome_count, 2);
        assert_eq!(stats.average_elapsed_ms, Some(24.0));
        assert_eq!(stats.max_elapsed_ms, Some(24.0));
    }

    #[test]
    fn async_task_diagnostic_stats_are_empty_without_activity() {
        assert_eq!(
            async_task_diagnostic_stats(&VecDeque::new(), &VecDeque::new()),
            None
        );
    }

    #[test]
    fn ui_events_summarize_async_task_outcomes() {
        let event = UiEvent::SearchFinished {
            request_id: 1,
            index_generation: 1,
            workspace_root: PathBuf::from("."),
            query: "needle".to_owned(),
            case_sensitive: false,
            whole_word: false,
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            max_file_bytes: SearchOptions::default().max_file_bytes,
            max_results: SearchOptions::default().max_results,
            result: SearchResult::default(),
        };

        assert_eq!(
            async_task_event_label(&event),
            Some(super::AsyncTaskEventLabel {
                name: "Project Search",
                detail: "`needle`".to_owned(),
                outcome: AsyncTaskOutcome::Finished,
            })
        );

        let event = UiEvent::WorkspacePluginsLoaded {
            request_id: 2,
            root: PathBuf::from("workspace"),
            plugins: Vec::new(),
            errors: Vec::new(),
            syntax_load: crate::syntax::PluginSyntaxLoad::empty(),
        };
        assert_eq!(
            async_task_event_label(&event),
            Some(super::AsyncTaskEventLabel {
                name: "Workspace Plugins",
                detail: "workspace".to_owned(),
                outcome: AsyncTaskOutcome::Finished,
            })
        );

        let root = PathBuf::from("workspace");
        let event = UiEvent::GitScanned {
            request_id: 3,
            root: root.clone(),
            scan_root: Some(root.clone()),
            root_cache_entry: None,
            git: kuroya_core::GitSnapshot::default(),
        };
        assert_eq!(
            async_task_event_label(&event),
            Some(super::AsyncTaskEventLabel {
                name: "Git Scan",
                detail: git_scan_task_detail(3, &root),
                outcome: AsyncTaskOutcome::Finished,
            })
        );
    }

    #[test]
    fn plugin_command_finished_label_matches_started_task_detail() {
        let command_id = format!("run\n{}\u{202e}", "command-fragment-".repeat(16));
        let started_detail = plugin_command_task_detail(&command_id);
        let event = UiEvent::PluginCommandFinished {
            root: PathBuf::from("workspace"),
            generation: 4,
            plugin_id: "example.plugin".to_owned(),
            command_id,
            result: Err("failed".to_owned()),
        };
        let label = async_task_event_label(&event).expect("plugin command event label");
        let mut active = VecDeque::from([AsyncTaskActiveEntry {
            id: 7,
            name: "Plugin Command".to_owned(),
            detail: started_detail,
            started_at: Instant::now(),
        }]);

        assert_eq!(label.name, "Plugin Command");
        assert_eq!(label.outcome, AsyncTaskOutcome::Failed);
        let finished =
            finish_matching_async_task(&mut active, label.name, &label.detail, Instant::now())
                .expect("finished plugin command should match active task");
        assert_eq!(finished.entry.id, 7);
        assert!(active.is_empty());
    }

    #[test]
    fn search_event_labels_sanitize_query_detail_without_mutating_event_payload() {
        let query = format!(
            "alpha\nbeta\u{202e} {}",
            "query-fragment-".repeat(MAX_ASYNC_TASK_DETAIL_CHARS)
        );
        let event = UiEvent::SearchFinished {
            request_id: 1,
            index_generation: 1,
            workspace_root: PathBuf::from("."),
            query: query.clone(),
            case_sensitive: false,
            whole_word: false,
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            max_file_bytes: SearchOptions::default().max_file_bytes,
            max_results: SearchOptions::default().max_results,
            result: SearchResult::default(),
        };

        let label = async_task_event_label(&event).expect("search event should produce a label");

        assert_eq!(label.name, "Project Search");
        assert_eq!(label.outcome, AsyncTaskOutcome::Finished);
        assert_sanitized_detail(&label.detail);
        assert!(label.detail.starts_with('`'));
        assert!(label.detail.ends_with('`'));
        assert!(label.detail.contains("alpha beta"));
        assert!(label.detail.contains("..."));
        assert_eq!(label.detail.chars().count(), MAX_ASYNC_TASK_DETAIL_CHARS);
        match event {
            UiEvent::SearchFinished { query: raw, .. } => assert_eq!(raw, query),
            _ => unreachable!("event variant should remain search"),
        }
    }

    #[test]
    fn branch_event_labels_sanitize_branch_details() {
        let root = PathBuf::from("workspace");
        let branch = format!(
            "feature\nserver\u{202e}/{}",
            "branch-fragment-".repeat(MAX_ASYNC_TASK_DETAIL_CHARS)
        );
        let event = UiEvent::GitBranchSwitchFinished {
            request_id: 1,
            root: root.clone(),
            operation_root: root.clone(),
            branch,
        };

        let label =
            async_task_event_label(&event).expect("branch switch event should produce a label");

        assert_eq!(label.name, "Git Branch Switch");
        assert_sanitized_detail(&label.detail);
        assert!(label.detail.contains("feature server/"));
        assert!(label.detail.contains("..."));

        let old_branch = format!(
            "main\nold\u{202e}/{}",
            "old-branch-fragment-".repeat(MAX_ASYNC_TASK_DETAIL_CHARS)
        );
        let new_branch = format!(
            "feature\nnew\u{202e}/{}",
            "new-branch-fragment-".repeat(MAX_ASYNC_TASK_DETAIL_CHARS)
        );
        let event = UiEvent::GitBranchRenameFinished {
            root: root.clone(),
            operation_root: root,
            old_branch,
            new_branch,
        };

        let label =
            async_task_event_label(&event).expect("branch rename event should produce a label");

        assert_eq!(label.name, "Git Branch Rename");
        assert_sanitized_detail(&label.detail);
        assert!(label.detail.contains("main old/"));
        assert!(label.detail.contains("feature new/"));
        assert!(label.detail.contains(" -> "));
        assert!(label.detail.matches("...").count() >= 2);
        assert_eq!(label.detail.chars().count(), MAX_ASYNC_TASK_DETAIL_CHARS);
    }

    #[test]
    fn explorer_failure_labels_sanitize_action_detail() {
        let action: &'static str = Box::leak(
            format!(
                "create\nfolder\u{202e} {}",
                "action-fragment-".repeat(MAX_ASYNC_TASK_DETAIL_CHARS)
            )
            .into_boxed_str(),
        );
        let event = UiEvent::ExplorerOperationFailed {
            root: PathBuf::from("workspace"),
            generation: 1,
            action,
            path: PathBuf::from("workspace/file.rs"),
            error: "denied".to_owned(),
        };

        let label =
            async_task_event_label(&event).expect("explorer failure event should produce a label");

        assert_eq!(label.name, "Explorer Operation");
        assert_eq!(label.outcome, AsyncTaskOutcome::Failed);
        assert_sanitized_detail(&label.detail);
        assert!(label.detail.contains("create folder"));
        assert!(label.detail.contains("..."));
        assert_eq!(label.detail.chars().count(), MAX_ASYNC_TASK_DETAIL_CHARS);
    }

    #[test]
    fn file_reload_async_task_labels_include_request_id() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");

        assert_async_label(
            UiEvent::FileReloaded {
                root: root.clone(),
                generation: 1,
                request_id: 42,
                id: 7,
                path: path.clone(),
                buffer: TextBuffer::from_text(7, Some(path.clone()), "new".to_owned()),
                elapsed: Duration::ZERO,
                version: 3,
                force_dirty: false,
                lossy: false,
                binary: false,
            },
            "File Reload",
            file_reload_task_detail(42, &path),
        );

        assert_eq!(
            async_task_event_label(&UiEvent::FileReloadFailed {
                root,
                generation: 1,
                request_id: 43,
                id: 7,
                path: path.clone(),
                error: "denied".to_owned(),
                version: 3,
                force_dirty: false,
            }),
            Some(AsyncTaskEventLabel {
                name: "File Reload",
                detail: file_reload_task_detail(43, &path),
                outcome: AsyncTaskOutcome::Failed,
            })
        );
    }

    #[test]
    fn file_reload_async_task_matching_keeps_same_path_requests_distinct() {
        let started_at = Instant::now();
        let path = PathBuf::from("workspace/src/main.rs");
        let mut active = VecDeque::from([
            AsyncTaskActiveEntry {
                id: 1,
                name: "File Reload".to_owned(),
                detail: file_reload_task_detail(10, &path),
                started_at,
            },
            AsyncTaskActiveEntry {
                id: 2,
                name: "File Reload".to_owned(),
                detail: file_reload_task_detail(11, &path),
                started_at,
            },
        ]);

        let finished = finish_matching_async_task(
            &mut active,
            "File Reload",
            &file_reload_task_detail(10, &path),
            started_at + Duration::from_millis(5),
        )
        .expect("matching request id should finish the intended reload task");

        assert_eq!(finished.entry.id, 1);
        let expected_remaining_detail = file_reload_task_detail(11, &path);
        assert_eq!(
            active
                .iter()
                .map(|entry| (entry.id, entry.detail.as_str()))
                .collect::<Vec<_>>(),
            vec![(2, expected_remaining_detail.as_str())]
        );
    }

    #[test]
    fn virtual_revision_async_task_labels_include_request_id() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let request = VirtualRevisionOpenRequest::Saved {
            path: path.clone(),
            jump: None,
        };

        assert_async_label(
            UiEvent::VirtualRevisionOpenFinished {
                root: root.clone(),
                generation: 1,
                request_id: 42,
                request: request.clone(),
                result: Ok(VirtualRevisionOpenOutcome::Status("done".to_owned())),
            },
            "Virtual Revision",
            virtual_revision_task_detail(42, &request),
        );

        assert_eq!(
            async_task_event_label(&UiEvent::VirtualRevisionOpenFinished {
                root,
                generation: 1,
                request_id: 43,
                request: request.clone(),
                result: Err("denied".to_owned()),
            }),
            Some(AsyncTaskEventLabel {
                name: "Virtual Revision",
                detail: virtual_revision_task_detail(43, &request),
                outcome: AsyncTaskOutcome::Failed,
            })
        );
    }

    #[test]
    fn virtual_revision_async_task_matching_keeps_same_path_requests_distinct() {
        let started_at = Instant::now();
        let request = VirtualRevisionOpenRequest::Saved {
            path: PathBuf::from("workspace/src/main.rs"),
            jump: None,
        };
        let mut active = VecDeque::from([
            AsyncTaskActiveEntry {
                id: 1,
                name: "Virtual Revision".to_owned(),
                detail: virtual_revision_task_detail(10, &request),
                started_at,
            },
            AsyncTaskActiveEntry {
                id: 2,
                name: "Virtual Revision".to_owned(),
                detail: virtual_revision_task_detail(11, &request),
                started_at,
            },
        ]);

        let finished = finish_matching_async_task(
            &mut active,
            "Virtual Revision",
            &virtual_revision_task_detail(10, &request),
            started_at + Duration::from_millis(5),
        )
        .expect("matching request id should finish the intended virtual revision task");

        assert_eq!(finished.entry.id, 1);
        let expected_remaining_detail = virtual_revision_task_detail(11, &request);
        assert_eq!(
            active
                .iter()
                .map(|entry| (entry.id, entry.detail.as_str()))
                .collect::<Vec<_>>(),
            vec![(2, expected_remaining_detail.as_str())]
        );
    }

    #[test]
    fn source_control_async_events_share_start_detail_formatting() {
        let root = PathBuf::from("workspace");
        let path = PathBuf::from("src/main.rs");
        let paths = vec![path.clone()];

        assert_async_label(
            UiEvent::GitStageFinished {
                root: root.clone(),
                paths: paths.clone(),
            },
            "Git Stage",
            paths_detail(&paths),
        );
        assert_async_label(
            UiEvent::GitBranchesLoaded {
                request_id: 1,
                root: root.clone(),
                operation_root: root.clone(),
                branches: Vec::new(),
            },
            "Git Branches",
            path_detail(&root),
        );
        assert_async_label(
            UiEvent::GitBranchRenameFinished {
                root: root.clone(),
                operation_root: root.clone(),
                old_branch: "main".to_owned(),
                new_branch: "feature".to_owned(),
            },
            "Git Branch Rename",
            branch_rename_detail("main", "feature"),
        );
        assert_async_label(
            UiEvent::GitStashApplied {
                root: root.clone(),
                operation_root: root.clone(),
                index: 2,
            },
            "Git Stash Apply",
            index_detail(2),
        );
        assert_async_label(
            UiEvent::GitHunksLoaded {
                request_id: 1,
                root: root.clone(),
                operation_root: root.clone(),
                path: path.clone(),
                stage: GitChangeStage::Unstaged,
                hunks: Vec::new(),
            },
            "Git Hunks",
            path_detail(&path),
        );
        assert_async_label(
            UiEvent::GitHunkStaged {
                root: root.clone(),
                path: path.clone(),
                hunk_index: 3,
            },
            "Git Hunk Stage",
            hunk_detail(&path, 3),
        );
        assert_async_label(
            UiEvent::GitCommitFinished {
                request_id: 1,
                root: root.clone(),
                short_oid: "12345678".to_owned(),
                message: "ship it".to_owned(),
                smart_commit: false,
            },
            "Git Commit",
            "staged changes".to_owned(),
        );
        assert_async_label(
            UiEvent::GitCommitFinished {
                request_id: 2,
                root: root.clone(),
                short_oid: "12345678".to_owned(),
                message: "ship it".to_owned(),
                smart_commit: true,
            },
            "Git Commit",
            "smart commit".to_owned(),
        );
        assert_async_label(
            UiEvent::GitPatchCopyFinished {
                root: root.clone(),
                operation_root: root,
                generation: 0,
                request_id: 1,
                request: SourceControlPatchCopyRequest::Hunk {
                    path: path.clone(),
                    stage: GitChangeStage::Unstaged,
                    hunk_index: 3,
                },
                result: Ok(SourceControlPatchCopyOutcome::Empty),
            },
            "Git Patch Copy",
            path_detail(&path),
        );
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.01,
            "expected {actual} to be close to {expected}"
        );
    }

    fn assert_sanitized_detail(detail: &str) {
        assert!(
            detail.chars().count() <= MAX_ASYNC_TASK_DETAIL_CHARS,
            "detail should be bounded: {detail}"
        );
        assert!(
            !detail.chars().any(is_unsafe_display_char),
            "detail should not include control or bidi formatting chars: {detail}"
        );
    }

    fn assert_owned_cow(label: Cow<'_, str>, expected: &str) {
        match label {
            Cow::Owned(label) => assert_eq!(label, expected),
            Cow::Borrowed(label) => panic!("expected owned label, got borrowed {label:?}"),
        }
    }

    fn is_unsafe_display_char(ch: char) -> bool {
        ch.is_control()
            || matches!(
                ch,
                '\u{061c}'
                    | '\u{200e}'
                    | '\u{200f}'
                    | '\u{2028}'
                    | '\u{2029}'
                    | '\u{202a}'..='\u{202e}'
                    | '\u{2066}'..='\u{2069}'
            )
    }

    fn assert_async_label(event: UiEvent, name: &'static str, detail: String) {
        assert_eq!(
            async_task_event_label(&event),
            Some(AsyncTaskEventLabel {
                name,
                detail,
                outcome: AsyncTaskOutcome::Finished,
            })
        );
    }
}
