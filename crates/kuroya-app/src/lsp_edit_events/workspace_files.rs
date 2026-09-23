use crate::{
    KuroyaApp,
    lsp_edits::{apply_lsp_edits_to_text, buffer_text_edits_from_lsp},
    lsp_lifecycle::{
        LSP_DISK_EDIT_MAX_BYTES, open_lsp_workspace_edit_block_reason, read_lsp_disk_edit_text,
    },
    lsp_ui_events::{LspWorkspaceApplyEditDiskResponse, LspWorkspaceApplyEditResponseTarget},
    path_display::{
        DISPLAY_PATH_LABEL_MAX_CHARS, display_error_label_cow, display_path_label_cow,
        sanitized_display_label_cow,
    },
    workspace_state::paths_match_lexically,
    workspace_trust::{
        workspace_path_contains_lexically, workspace_path_stays_within_root_lexically,
    },
};
use kuroya_core::{
    BufferId, LspRequestId, LspTextEdit, LspWorkspaceDocumentChange, LspWorkspaceResourceOperation,
    TextBuffer,
};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

const WORKSPACE_APPLY_EDIT_LABEL_MAX_CHARS: usize = DISPLAY_PATH_LABEL_MAX_CHARS;

impl KuroyaApp {
    pub(super) fn handle_lsp_workspace_edit_files_applied(
        &mut self,
        changed: usize,
        failed_count: usize,
        apply_edit_response: Option<LspWorkspaceApplyEditDiskResponse>,
    ) {
        if changed > 0 {
            self.spawn_index();
            self.spawn_git_auto_refresh();
        }
        if failed_count == 0 {
            self.status = format!("Applied LSP edits to {changed} files on disk");
        } else {
            self.status = format!("Applied LSP edits to {changed} files, {failed_count} failed");
        }

        if let Some(response) = apply_edit_response {
            let failed = failed_count + response.open_failed;
            let skipped = response.open_skipped;
            let applied = failed == 0 && skipped == 0;
            let failure_reason = apply_edit_failure_reason(failed, skipped);
            self.send_lsp_workspace_apply_edit_response(response.target, applied, failure_reason);
        }
    }

    pub(super) fn handle_lsp_workspace_apply_edit_request(
        &mut self,
        language: String,
        root: PathBuf,
        generation: u64,
        request_id: LspRequestId,
        label: Option<String>,
        edits: Option<Vec<LspTextEdit>>,
        document_changes: Vec<LspWorkspaceDocumentChange>,
        document_versions: BTreeMap<PathBuf, i32>,
        error: Option<String>,
    ) {
        if !self.workspace_apply_edit_event_is_current(&language, &root, generation) {
            return;
        }

        let target = LspWorkspaceApplyEditResponseTarget {
            language,
            generation,
            request_id,
        };

        if let Some(error) = error {
            self.status = lsp_workspace_edit_rejected_status(&error);
            self.send_lsp_workspace_apply_edit_response(target, false, Some(error));
            return;
        }

        let Some(edits) = edits else {
            let reason = "invalid workspace/applyEdit request".to_owned();
            self.status = lsp_workspace_edit_rejected_status(&reason);
            self.send_lsp_workspace_apply_edit_response(target, false, Some(reason));
            return;
        };

        if let Some(reason) = self.workspace_apply_edit_preflight(&edits, &document_versions) {
            self.status = lsp_workspace_edit_rejected_status(&reason);
            self.send_lsp_workspace_apply_edit_response(target, false, Some(reason));
            return;
        }

        let label = workspace_apply_edit_display_label_cow(label.as_deref());
        let document_change_summary = workspace_document_changes_summary(&document_changes);
        if document_change_summary.has_resource_operations {
            if let Some(reason) = self.workspace_apply_resource_edit_preflight_with_summary(
                &edits,
                &document_changes,
                document_change_summary,
            ) {
                self.status = lsp_workspace_edit_rejected_status(&reason);
                self.send_lsp_workspace_apply_edit_response(target, false, Some(reason));
                return;
            }
            let response = LspWorkspaceApplyEditDiskResponse {
                target,
                open_failed: 0,
                open_skipped: 0,
            };
            let queued = document_changes.len();
            self.spawn_lsp_workspace_document_changes(document_changes, Some(response));
            self.status = format!(
                "{}: queued {queued} ordered workspace change(s)",
                label.as_ref()
            );
            return;
        }

        let outcome =
            self.apply_lsp_workspace_edits_for_apply_edit(edits, label.as_ref(), target.clone());
        if outcome.disk_queued == 0 {
            self.send_lsp_workspace_apply_edit_response(
                target,
                outcome.applied(),
                outcome.failure_reason(),
            );
        }
    }

    pub(crate) fn apply_lsp_workspace_document_changes_for_action(
        &mut self,
        edits: &[LspTextEdit],
        document_changes: Vec<LspWorkspaceDocumentChange>,
        label: &str,
    ) -> bool {
        let label = workspace_edit_action_display_label_cow(label);
        if let Some(reason) = self.workspace_apply_edit_preflight(edits, &BTreeMap::new()) {
            self.status = workspace_edit_action_rejected_status(label.as_ref(), &reason);
            return false;
        }
        let document_change_summary = workspace_document_changes_summary(&document_changes);
        if let Some(reason) = self.workspace_apply_resource_edit_preflight_with_summary(
            edits,
            &document_changes,
            document_change_summary,
        ) {
            self.status = workspace_edit_action_rejected_status(label.as_ref(), &reason);
            return false;
        }
        let queued = document_changes.len();
        self.spawn_lsp_workspace_document_changes(document_changes, None);
        self.status = format!(
            "{}: queued {queued} ordered workspace change(s)",
            label.as_ref()
        );
        true
    }

    fn send_lsp_workspace_apply_edit_response(
        &mut self,
        target: LspWorkspaceApplyEditResponseTarget,
        applied: bool,
        failure_reason: Option<String>,
    ) {
        if let Some((_, client)) =
            self.lsp_client_entry_for_event(&target.language, target.generation)
        {
            if client.apply_workspace_edit_response(
                target.request_id,
                applied,
                failure_reason.clone(),
            ) {
                return;
            }
        }

        self.status = match failure_reason {
            Some(reason) => format!(
                "LSP workspace edit response failed: {}",
                display_error_label_cow(&reason)
            ),
            None => "LSP workspace edit response failed".to_owned(),
        };
    }

    fn workspace_apply_edit_event_is_current(
        &self,
        language: &str,
        root: &Path,
        generation: u64,
    ) -> bool {
        self.lsp_lifecycle_event_matches(language, root, generation)
    }

    fn workspace_apply_edit_preflight(
        &self,
        edits: &[LspTextEdit],
        document_versions: &BTreeMap<PathBuf, i32>,
    ) -> Option<String> {
        let mut target_cache = WorkspaceApplyEditTargetCache::default();
        for (path, version) in document_versions {
            if !path_is_within_workspace(&self.workspace.root, path) {
                return Some(format!(
                    "edit outside workspace: {}",
                    display_path_label_cow(path)
                ));
            }
            let target = target_cache.resolve(self, path);
            let Some(buffer) = target.open_buffer_id.and_then(|id| self.buffer(id)) else {
                return Some(format!(
                    "versioned workspace edit target is not open: {}",
                    display_path_label_cow(path)
                ));
            };
            if buffer_lsp_version(buffer.version()) != *version {
                return Some(format!(
                    "stale workspace edit for {}",
                    display_path_label_cow(path)
                ));
            }
        }

        let mut open_edit_batches: BTreeMap<BufferId, WorkspaceApplyOpenEditBatch> =
            BTreeMap::new();
        for edit in edits {
            if !path_is_within_workspace(&self.workspace.root, &edit.path) {
                return Some(format!(
                    "edit outside workspace: {}",
                    display_path_label_cow(&edit.path)
                ));
            }
            let target = target_cache.resolve(self, &edit.path);
            if target.open_buffer_id.is_none()
                && let Some(reason) =
                    workspace_edit_symlink_component_rejection(&self.workspace.root, &edit.path)
            {
                return Some(reason);
            }
            let Some(id) = target.open_buffer_id else {
                continue;
            };
            open_edit_batches
                .entry(id)
                .or_insert_with(|| WorkspaceApplyOpenEditBatch {
                    resolved_path: target.resolved_path.clone(),
                    edits: Vec::new(),
                })
                .edits
                .push(edit.clone());
        }

        if open_edit_batches.is_empty() {
            return None;
        }

        let changed_on_disk = self.observed_external_change_buffer_ids();
        for (id, batch) in open_edit_batches {
            if let Some(reason) = open_lsp_workspace_edit_block_reason(
                id,
                &changed_on_disk,
                &self.lossy_decoded_buffers,
                &self.binary_preview_buffers,
                &self.buffers,
            ) {
                return Some(format!(
                    "unsafe open buffer skipped: {} ({reason})",
                    display_path_label_cow(&batch.resolved_path)
                ));
            }
            let Some(buffer) = self.buffer(id) else {
                return Some(format!(
                    "open buffer disappeared: {}",
                    display_path_label_cow(&batch.resolved_path)
                ));
            };
            if buffer_text_edits_from_lsp(buffer, &batch.edits).is_none() {
                return Some(format!(
                    "invalid LSP edit range in {}",
                    display_path_label_cow(&batch.resolved_path)
                ));
            }
        }

        None
    }

    fn workspace_apply_resource_edit_preflight_with_summary(
        &self,
        edits: &[LspTextEdit],
        document_changes: &[LspWorkspaceDocumentChange],
        document_change_summary: WorkspaceDocumentChangeSummary,
    ) -> Option<String> {
        if document_change_summary.text_edit_count != edits.len() {
            return Some(
                "top-level text changes cannot be combined with resource operations yet".to_owned(),
            );
        }

        let mut resource_state = SimulatedWorkspaceResourceState::default();
        let mut open_buffers = WorkspaceResourceOpenBufferCache::new(&self.buffers);

        for change in document_changes {
            match change {
                LspWorkspaceDocumentChange::TextEdit {
                    path,
                    version,
                    edits,
                } => {
                    if let Some(reason) = self.workspace_apply_resource_path_preflight(path, "edit")
                    {
                        return Some(reason);
                    }
                    if version.is_some() {
                        return Some(format!(
                            "versioned ordered workspace edit target must be open: {}",
                            display_path_label_cow(path)
                        ));
                    }
                    if open_buffers.target_is_open(path) {
                        return Some(format!(
                            "ordered resource edit target is open: {}",
                            display_path_label_cow(path)
                        ));
                    }
                    match resource_state.path_state(path) {
                        Ok(SimulatedWorkspacePath::Missing) if edits.is_empty() => {}
                        Ok(SimulatedWorkspacePath::Missing) => {
                            return Some(format!(
                                "ordered workspace edit target does not exist: {}",
                                display_path_label_cow(path)
                            ));
                        }
                        Ok(SimulatedWorkspacePath::Directory) => {
                            return Some(format!(
                                "ordered workspace edit target is a directory: {}",
                                display_path_label_cow(path)
                            ));
                        }
                        Ok(SimulatedWorkspacePath::File(file)) => {
                            let text = match file.text(path) {
                                Ok(text) => text,
                                Err(error) => return Some(error),
                            };
                            let Some(text) = apply_lsp_edits_to_text(path, text, edits) else {
                                return Some(format!(
                                    "invalid LSP edit range in {}",
                                    display_path_label_cow(path)
                                ));
                            };
                            resource_state.set_path_state(
                                path,
                                SimulatedWorkspacePath::File(SimulatedWorkspaceFile::Memory(text)),
                            );
                        }
                        Err(error) => return Some(error),
                    }
                }
                LspWorkspaceDocumentChange::Resource(
                    LspWorkspaceResourceOperation::CreateFile {
                        path,
                        overwrite,
                        ignore_if_exists,
                    },
                ) => {
                    if let Some(reason) =
                        self.workspace_apply_resource_path_preflight(path, "create")
                    {
                        return Some(reason);
                    }
                    if open_buffers.target_is_open(path) {
                        return Some(format!(
                            "resource create target is open: {}",
                            display_path_label_cow(path)
                        ));
                    }
                    match resource_state.path_state(path) {
                        Ok(SimulatedWorkspacePath::Missing) => {
                            if let Some(parent) = resource_state.parent_file(path) {
                                return Some(format!(
                                    "resource create parent is a file: {}",
                                    display_path_label_cow(&parent)
                                ));
                            }
                            resource_state.set_path_state(
                                path,
                                SimulatedWorkspacePath::File(SimulatedWorkspaceFile::Memory(
                                    String::new(),
                                )),
                            );
                        }
                        Ok(SimulatedWorkspacePath::Directory) => {
                            return Some(format!(
                                "resource create cannot overwrite directory: {}",
                                display_path_label_cow(path)
                            ));
                        }
                        Ok(SimulatedWorkspacePath::File(file)) => {
                            if *overwrite {
                                if let Err(error) = file.text(path) {
                                    return Some(format!(
                                        "resource create cannot overwrite unsafe file: {} ({})",
                                        display_path_label_cow(path),
                                        display_error_label_cow(&error)
                                    ));
                                }
                                resource_state.set_path_state(
                                    path,
                                    SimulatedWorkspacePath::File(SimulatedWorkspaceFile::Memory(
                                        String::new(),
                                    )),
                                );
                            } else if !ignore_if_exists {
                                return Some(format!(
                                    "resource create target already exists: {}",
                                    display_path_label_cow(path)
                                ));
                            }
                        }
                        Err(error) => return Some(error),
                    }
                }
                LspWorkspaceDocumentChange::Resource(
                    LspWorkspaceResourceOperation::RenameFile {
                        old_path,
                        new_path,
                        overwrite,
                        ignore_if_exists,
                    },
                ) => {
                    if let Some(reason) =
                        self.workspace_apply_resource_path_preflight(old_path, "rename")
                    {
                        return Some(reason);
                    }
                    if let Some(reason) =
                        self.workspace_apply_resource_path_preflight(new_path, "rename")
                    {
                        return Some(reason);
                    }
                    if path_same_as_workspace_root(&self.workspace.root, old_path) {
                        return Some("cannot rename workspace root".to_owned());
                    }
                    if path_same_as_workspace_root(&self.workspace.root, new_path) {
                        return Some("cannot rename to workspace root".to_owned());
                    }
                    if paths_match_lexically(old_path, new_path) {
                        return Some(format!(
                            "resource rename source and target match: {}",
                            display_path_label_cow(old_path)
                        ));
                    }
                    let old_state = match resource_state.path_state(old_path) {
                        Ok(SimulatedWorkspacePath::Missing) => {
                            return Some(format!(
                                "resource rename source does not exist: {}",
                                display_path_label_cow(old_path)
                            ));
                        }
                        Ok(state) => state,
                        Err(error) => return Some(error),
                    };
                    if matches!(old_state, SimulatedWorkspacePath::Directory)
                        && workspace_path_contains_lexically(old_path, new_path)
                    {
                        return Some(format!(
                            "resource rename target is inside source: {}",
                            display_path_label_cow(new_path)
                        ));
                    }
                    if open_buffers
                        .any_path_affects_open_buffer(&[old_path.as_path(), new_path.as_path()])
                    {
                        return Some(format!(
                            "resource rename affects open buffer: {}",
                            display_path_label_cow(old_path)
                        ));
                    }
                    match resource_state.path_state(new_path) {
                        Ok(SimulatedWorkspacePath::Missing) => {
                            if let Some(parent) = resource_state.parent_file(new_path) {
                                return Some(format!(
                                    "resource rename parent is a file: {}",
                                    display_path_label_cow(&parent)
                                ));
                            }
                            resource_state.rename_path_state(old_path, new_path, old_state);
                        }
                        Ok(SimulatedWorkspacePath::Directory) => {
                            return Some(format!(
                                "resource rename cannot overwrite directory: {}",
                                display_path_label_cow(new_path)
                            ));
                        }
                        Ok(SimulatedWorkspacePath::File(file)) => {
                            if *overwrite {
                                if let Err(error) = file.text(new_path) {
                                    return Some(format!(
                                        "resource rename cannot overwrite unsafe file: {} ({})",
                                        display_path_label_cow(new_path),
                                        display_error_label_cow(&error)
                                    ));
                                }
                                resource_state.rename_path_state(old_path, new_path, old_state);
                            } else if !ignore_if_exists {
                                return Some(format!(
                                    "resource rename target already exists: {}",
                                    display_path_label_cow(new_path)
                                ));
                            }
                        }
                        Err(error) => return Some(error),
                    }
                }
                LspWorkspaceDocumentChange::Resource(
                    LspWorkspaceResourceOperation::DeleteFile {
                        path,
                        recursive,
                        ignore_if_not_exists,
                    },
                ) => {
                    if let Some(reason) =
                        self.workspace_apply_resource_path_preflight(path, "delete")
                    {
                        return Some(reason);
                    }
                    if path_same_as_workspace_root(&self.workspace.root, path) {
                        return Some("cannot delete workspace root".to_owned());
                    }
                    let state = match resource_state.path_state(path) {
                        Ok(SimulatedWorkspacePath::Missing) if *ignore_if_not_exists => None,
                        Ok(SimulatedWorkspacePath::Missing) => {
                            return Some(format!(
                                "resource delete target does not exist: {}",
                                display_path_label_cow(path)
                            ));
                        }
                        Ok(state) => Some(state),
                        Err(error) => return Some(error),
                    };
                    if let Some(SimulatedWorkspacePath::Directory) = state
                        && !*recursive
                    {
                        return Some(format!(
                            "resource delete requires recursive option: {}",
                            display_path_label_cow(path)
                        ));
                    }
                    if open_buffers.path_affects_open_buffer(path) {
                        return Some(format!(
                            "resource delete affects open buffer: {}",
                            display_path_label_cow(path)
                        ));
                    }
                    resource_state.set_path_state(path, SimulatedWorkspacePath::Missing);
                }
            }
        }

        None
    }

    fn workspace_apply_resource_path_preflight(&self, path: &Path, action: &str) -> Option<String> {
        if !path_is_within_workspace(&self.workspace.root, path) {
            return Some(format!(
                "resource {action} outside workspace: {}",
                display_path_label_cow(path)
            ));
        }
        if path_same_as_workspace_root(&self.workspace.root, path) && action == "create" {
            return Some("cannot create over workspace root".to_owned());
        }
        if let Some(reason) =
            workspace_resource_symlink_component_rejection(&self.workspace.root, path, action)
        {
            return Some(reason);
        }
        None
    }
}

fn apply_edit_failure_reason(failed: usize, skipped: usize) -> Option<String> {
    if failed > 0 {
        Some(format!("{failed} workspace edit file(s) failed"))
    } else if skipped > 0 {
        Some(format!("{skipped} unsafe open buffer(s) skipped"))
    } else {
        None
    }
}

#[cfg(test)]
fn workspace_apply_edit_display_label(label: Option<&str>) -> String {
    workspace_apply_edit_display_label_cow(label).into_owned()
}

#[cfg(test)]
fn workspace_edit_action_display_label(label: &str) -> String {
    workspace_edit_action_display_label_cow(label).into_owned()
}

fn workspace_apply_edit_display_label_cow<'a>(label: Option<&'a str>) -> Cow<'a, str> {
    workspace_edit_display_label_cow(label.unwrap_or("LSP workspace edit"), "LSP workspace edit")
}

fn workspace_edit_action_display_label_cow(label: &str) -> Cow<'_, str> {
    workspace_edit_display_label_cow(label, "LSP workspace edit")
}

fn workspace_edit_display_label_cow<'a>(label: &'a str, fallback: &str) -> Cow<'a, str> {
    sanitized_display_label_cow(label, WORKSPACE_APPLY_EDIT_LABEL_MAX_CHARS, fallback)
}

fn lsp_workspace_edit_rejected_status(reason: &str) -> String {
    format!(
        "LSP workspace edit rejected: {}",
        display_error_label_cow(reason)
    )
}

fn workspace_edit_action_rejected_status(label: &str, reason: &str) -> String {
    format!("{label} rejected: {}", display_error_label_cow(reason))
}

pub(crate) fn workspace_document_changes_contain_resource_operations(
    document_changes: &[LspWorkspaceDocumentChange],
) -> bool {
    document_changes
        .iter()
        .any(|change| matches!(change, LspWorkspaceDocumentChange::Resource(_)))
}

#[derive(Clone, Copy)]
struct WorkspaceDocumentChangeSummary {
    text_edit_count: usize,
    has_resource_operations: bool,
}

fn workspace_document_changes_summary(
    document_changes: &[LspWorkspaceDocumentChange],
) -> WorkspaceDocumentChangeSummary {
    let mut summary = WorkspaceDocumentChangeSummary {
        text_edit_count: 0,
        has_resource_operations: false,
    };
    for change in document_changes {
        match change {
            LspWorkspaceDocumentChange::TextEdit { edits, .. } => {
                summary.text_edit_count += edits.len();
            }
            LspWorkspaceDocumentChange::Resource(_) => {
                summary.has_resource_operations = true;
            }
        }
    }
    summary
}

#[derive(Default)]
struct WorkspaceApplyEditTargetCache {
    targets: BTreeMap<PathBuf, WorkspaceApplyEditTarget>,
}

impl WorkspaceApplyEditTargetCache {
    fn resolve(&mut self, app: &KuroyaApp, path: &Path) -> &WorkspaceApplyEditTarget {
        let key = workspace_apply_edit_target_cache_key(path);
        match self.targets.entry(key) {
            std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::btree_map::Entry::Vacant(entry) => {
                let target = app.resolve_lsp_workspace_edit_target(path);
                entry.insert(WorkspaceApplyEditTarget {
                    resolved_path: target.resolved_path,
                    open_buffer_id: target.open_buffer_id,
                })
            }
        }
    }
}

struct WorkspaceApplyEditTarget {
    resolved_path: PathBuf,
    open_buffer_id: Option<BufferId>,
}

struct WorkspaceApplyOpenEditBatch {
    resolved_path: PathBuf,
    edits: Vec<LspTextEdit>,
}

fn workspace_apply_edit_target_cache_key(path: &Path) -> PathBuf {
    lexically_normalize_path(path).unwrap_or_else(|| path.to_path_buf())
}

struct WorkspaceResourceOpenBufferCache<'a> {
    buffers: &'a [TextBuffer],
    exact_matches: BTreeMap<PathBuf, bool>,
    affected_matches: BTreeMap<PathBuf, bool>,
}

impl<'a> WorkspaceResourceOpenBufferCache<'a> {
    fn new(buffers: &'a [TextBuffer]) -> Self {
        Self {
            buffers,
            exact_matches: BTreeMap::new(),
            affected_matches: BTreeMap::new(),
        }
    }

    fn open_paths(&self) -> impl Iterator<Item = &Path> {
        self.buffers
            .iter()
            .filter_map(|buffer| buffer.path().map(PathBuf::as_path))
    }

    fn target_is_open(&mut self, path: &Path) -> bool {
        let key = workspace_resource_open_buffer_cache_key(path);
        if let Some(matches) = self.exact_matches.get(&key) {
            return *matches;
        }
        let matches = self
            .open_paths()
            .any(|candidate| paths_match_lexically(candidate, path));
        self.exact_matches.insert(key, matches);
        matches
    }

    fn path_affects_open_buffer(&mut self, path: &Path) -> bool {
        let key = workspace_resource_open_buffer_cache_key(path);
        if let Some(matches) = self.affected_matches.get(&key) {
            return *matches;
        }
        let matches = self.open_paths().any(|candidate| {
            paths_match_lexically(candidate, path)
                || workspace_path_contains_lexically(path, candidate)
        });
        self.affected_matches.insert(key, matches);
        matches
    }

    fn any_path_affects_open_buffer(&mut self, paths: &[&Path]) -> bool {
        paths.iter().any(|path| self.path_affects_open_buffer(path))
    }
}

fn workspace_resource_open_buffer_cache_key(path: &Path) -> PathBuf {
    lexically_normalize_path(path).unwrap_or_else(|| path.to_path_buf())
}

#[derive(Clone)]
enum SimulatedWorkspaceFile {
    Disk(PathBuf),
    Memory(String),
}

impl SimulatedWorkspaceFile {
    fn text(&self, fallback_path: &Path) -> Result<String, String> {
        match self {
            SimulatedWorkspaceFile::Disk(path) => {
                read_lsp_disk_edit_text(path, LSP_DISK_EDIT_MAX_BYTES)
            }
            SimulatedWorkspaceFile::Memory(text) => Ok(text.clone()),
        }
        .map_err(|error| {
            if error.is_empty() {
                format!("failed to read {}", display_path_label_cow(fallback_path))
            } else {
                error
            }
        })
    }
}

#[derive(Clone)]
enum SimulatedWorkspacePath {
    Missing,
    File(SimulatedWorkspaceFile),
    Directory,
}

#[derive(Default)]
struct SimulatedWorkspaceResourceState {
    paths: BTreeMap<PathBuf, SimulatedWorkspacePath>,
    disk_paths: BTreeMap<PathBuf, Result<SimulatedWorkspacePath, String>>,
    directory_renames: Vec<SimulatedWorkspaceDirectoryRename>,
}

#[derive(Clone)]
struct SimulatedWorkspaceDirectoryRename {
    old_path: PathBuf,
    new_path: PathBuf,
}

impl SimulatedWorkspaceResourceState {
    fn path_state(&mut self, path: &Path) -> Result<SimulatedWorkspacePath, String> {
        let key = simulated_workspace_path_key(path)?;
        if let Some(state) = self
            .matching_path_key_ref(&key)
            .and_then(|key| self.paths.get(key))
        {
            return Ok(state.clone());
        }
        if self.has_missing_ancestor(&key) {
            return Ok(SimulatedWorkspacePath::Missing);
        }
        if let Some(disk_path) = self.renamed_disk_path_for(&key) {
            let disk_key = simulated_workspace_path_key(&disk_path)?;
            return self.cached_disk_path_state(disk_key, &disk_path);
        }

        self.cached_disk_path_state(key, path)
    }

    fn cached_disk_path_state(
        &mut self,
        key: PathBuf,
        path: &Path,
    ) -> Result<SimulatedWorkspacePath, String> {
        if let Some(state) = self.disk_paths.get(&key) {
            return state.clone();
        }
        let state = simulated_disk_path_state(path);
        self.disk_paths.insert(key, state.clone());
        state
    }

    fn set_path_state(&mut self, path: &Path, state: SimulatedWorkspacePath) {
        if let Ok(key) = simulated_workspace_path_key(path) {
            if let Some(existing_key) = self.matching_path_key(&key) {
                self.paths.insert(existing_key, state);
            } else {
                self.paths.insert(key, state);
            }
        }
    }

    fn rename_path_state(
        &mut self,
        old_path: &Path,
        new_path: &Path,
        state: SimulatedWorkspacePath,
    ) {
        let directory_source = if matches!(state, SimulatedWorkspacePath::Directory) {
            simulated_workspace_path_key(old_path)
                .ok()
                .map(|old_key| self.renamed_disk_path_for(&old_key).unwrap_or(old_key))
        } else {
            None
        };
        let directory_target = if directory_source.is_some() {
            simulated_workspace_path_key(new_path).ok()
        } else {
            None
        };

        self.set_path_state(old_path, SimulatedWorkspacePath::Missing);
        self.set_path_state(new_path, state);

        if let (Some(old_path), Some(new_path)) = (directory_source, directory_target) {
            self.directory_renames
                .push(SimulatedWorkspaceDirectoryRename { old_path, new_path });
        }
    }

    fn parent_file(&mut self, path: &Path) -> Option<PathBuf> {
        let parent = path.parent()?;
        match self.path_state(parent).ok()? {
            SimulatedWorkspacePath::File(_) => Some(parent.to_path_buf()),
            SimulatedWorkspacePath::Missing | SimulatedWorkspacePath::Directory => None,
        }
    }

    fn matching_path_key(&self, path: &Path) -> Option<PathBuf> {
        self.matching_path_key_ref(path).cloned()
    }

    fn matching_path_key_ref(&self, path: &Path) -> Option<&PathBuf> {
        self.paths
            .keys()
            .find(|candidate| paths_match_lexically(candidate, path))
    }

    fn has_missing_ancestor(&self, path: &Path) -> bool {
        self.paths.iter().any(|(candidate, state)| {
            matches!(state, SimulatedWorkspacePath::Missing)
                && !paths_match_lexically(candidate, path)
                && workspace_path_contains_lexically(candidate, path)
        })
    }

    fn renamed_disk_path_for(&self, path: &Path) -> Option<PathBuf> {
        self.renamed_disk_path_for_depth(path, 0)
    }

    fn renamed_disk_path_for_depth(&self, path: &Path, depth: usize) -> Option<PathBuf> {
        if depth > self.directory_renames.len() {
            return None;
        }
        for rename in self.directory_renames.iter().rev() {
            let Some(suffix) = lexical_child_suffix(&rename.new_path, path) else {
                continue;
            };
            let mapped = if suffix.as_os_str().is_empty() {
                rename.old_path.clone()
            } else {
                rename.old_path.join(suffix)
            };
            if let Some(remapped) = self.renamed_disk_path_for_depth(&mapped, depth + 1) {
                return Some(remapped);
            }
            return Some(mapped);
        }
        None
    }
}

fn simulated_workspace_path_key(path: &Path) -> Result<PathBuf, String> {
    lexically_normalize_path(path).ok_or_else(|| {
        format!(
            "invalid workspace resource path: {}",
            display_path_label_cow(path)
        )
    })
}

fn workspace_edit_symlink_component_rejection(root: &Path, path: &Path) -> Option<String> {
    workspace_symlink_component_rejection(root, path, "edit target uses symlink")
}

fn workspace_resource_symlink_component_rejection(
    root: &Path,
    path: &Path,
    action: &str,
) -> Option<String> {
    workspace_symlink_component_rejection(
        root,
        path,
        &format!("resource {action} target uses symlink"),
    )
}

fn workspace_symlink_component_rejection(root: &Path, path: &Path, prefix: &str) -> Option<String> {
    match workspace_symlink_component(root, path) {
        Ok(Some(symlink)) => Some(format!("{prefix}: {}", display_path_label_cow(&symlink))),
        Ok(None) => None,
        Err(error) => Some(format!(
            "{prefix} path probe failed: {}",
            display_error_label_cow(&error)
        )),
    }
}

fn workspace_symlink_component(root: &Path, path: &Path) -> Result<Option<PathBuf>, String> {
    let root = simulated_workspace_path_key(root)?;
    let path = simulated_workspace_path_key(path)?;
    let Some(suffix) = lexical_child_suffix(&root, &path) else {
        return Ok(None);
    };

    let mut current = root;
    for component in suffix.components() {
        current.push(component.as_os_str());
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => return Ok(Some(current)),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
        }
    }

    Ok(None)
}

fn simulated_disk_path_state(path: &Path) -> Result<SimulatedWorkspacePath, String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(SimulatedWorkspacePath::Directory),
        Ok(_) => Ok(SimulatedWorkspacePath::File(SimulatedWorkspaceFile::Disk(
            path.to_path_buf(),
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(SimulatedWorkspacePath::Missing)
        }
        Err(error) => Err(error.to_string()),
    }
}

fn lexical_child_suffix(parent: &Path, path: &Path) -> Option<PathBuf> {
    if !workspace_path_contains_lexically(parent, path) {
        return None;
    }
    let mut suffix = PathBuf::new();
    for component in path.components().skip(parent.components().count()) {
        suffix.push(component.as_os_str());
    }
    Some(suffix)
}

fn path_same_as_workspace_root(root: &Path, path: &Path) -> bool {
    paths_match_lexically(root, path)
}

fn buffer_lsp_version(version: u64) -> i32 {
    version.min(i32::MAX as u64) as i32
}

fn path_is_within_workspace(root: &Path, path: &Path) -> bool {
    workspace_path_stays_within_root_lexically(root, path)
        && workspace_path_contains_lexically(root, path)
}

fn lexically_normalize_path(path: &Path) -> Option<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
            Component::Normal(component) => normalized.push(component),
        }
    }
    Some(normalized)
}

#[cfg(test)]
mod tests;
