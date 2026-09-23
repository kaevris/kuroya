use crate::{KuroyaApp, path_display::display_error_label_cow};
use std::{
    collections::HashSet,
    ffi::OsStr,
    path::{Component, Path, PathBuf},
};

pub(crate) fn workspace_is_trusted(trusted_workspaces: &[PathBuf], root: &Path) -> bool {
    let Some(root_key) = workspace_trust_key(root) else {
        return false;
    };

    trusted_workspaces
        .iter()
        .any(|trusted| workspace_trust_path_matches_key(trusted, &root_key))
}

pub(crate) fn trust_workspace(trusted_workspaces: &[PathBuf], root: PathBuf) -> Vec<PathBuf> {
    let mut trusted = dedupe_trusted_workspaces(trusted_workspaces);
    let Some(root_key) = workspace_trust_key(&root) else {
        return trusted;
    };
    let root_already_trusted = trusted
        .iter()
        .any(|candidate| workspace_trust_path_matches_key(candidate, &root_key));
    if !root_already_trusted {
        trusted.insert(0, root);
    }
    trusted
}

pub(crate) fn revoke_workspace_trust(trusted_workspaces: &[PathBuf], root: &Path) -> Vec<PathBuf> {
    let Some(root_key) = workspace_trust_key(root) else {
        return dedupe_trusted_workspaces(trusted_workspaces);
    };

    let retained = trusted_workspaces
        .iter()
        .filter(|candidate| !workspace_trust_path_matches_key(candidate, &root_key))
        .cloned()
        .collect::<Vec<_>>();
    dedupe_trusted_workspaces(&retained)
}

pub(crate) fn trusted_workspace_paths_match(left: &Path, right: &Path) -> bool {
    let Some(right) = workspace_trust_key(right) else {
        return false;
    };
    workspace_trust_path_matches_key(left, &right)
}

pub(crate) fn workspace_trust_prompt_root(
    workspace_placeholder: bool,
    workspace_trusted: bool,
    root: PathBuf,
) -> Option<PathBuf> {
    (!workspace_placeholder && !workspace_trusted).then_some(root)
}

pub(crate) fn workspace_path_contains_lexically(root: &Path, path: &Path) -> bool {
    let Some(root) = workspace_trust_key(root) else {
        return false;
    };
    let Some(path) = workspace_trust_key(path) else {
        return false;
    };

    workspace_trust_key_contains(&root, &path)
}

pub(crate) fn workspace_path_stays_within_root_lexically(root: &Path, path: &Path) -> bool {
    let Some(root) = workspace_trust_key(root) else {
        return false;
    };
    let Some(path_key) = workspace_trust_key(path) else {
        return false;
    };

    if !workspace_trust_key_contains(&root, &path_key) {
        return false;
    }

    raw_path_walk_stays_within_workspace_root(&root, path)
}

fn workspace_trust_key_contains(root: &WorkspaceTrustKey, path: &WorkspaceTrustKey) -> bool {
    if root.prefix.is_none() && !root.rooted && root.components.is_empty() {
        return path.prefix.is_none()
            && !path.rooted
            && path
                .components
                .first()
                .is_none_or(|component| component != "..");
    }

    root.prefix == path.prefix
        && root.rooted == path.rooted
        && path.components.starts_with(&root.components)
}

fn raw_path_walk_stays_within_workspace_root(root: &WorkspaceTrustKey, path: &Path) -> bool {
    let mut current = WorkspaceTrustKey {
        prefix: None,
        rooted: false,
        components: Vec::new(),
    };

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                current.prefix = Some(normalize_workspace_trust_component(prefix.as_os_str()));
            }
            Component::RootDir => {
                current.rooted = true;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !workspace_trust_key_contains(root, &current)
                    || current.components.len() <= root.components.len()
                {
                    return false;
                }
                current.components.pop();
            }
            Component::Normal(component) => {
                current
                    .components
                    .push(normalize_workspace_trust_component(component));
            }
        }

        if !workspace_key_is_on_route_to_or_inside_root(root, &current) {
            return false;
        }
    }

    true
}

fn workspace_key_is_on_route_to_or_inside_root(
    root: &WorkspaceTrustKey,
    current: &WorkspaceTrustKey,
) -> bool {
    if !workspace_key_style_can_still_match(root, current) {
        return false;
    }

    if root.prefix.is_none() && !root.rooted && root.components.is_empty() {
        return current.prefix.is_none()
            && !current.rooted
            && current
                .components
                .first()
                .is_none_or(|component| component != "..");
    }

    root.components.starts_with(&current.components)
        || current.components.starts_with(&root.components)
}

fn workspace_key_style_can_still_match(
    root: &WorkspaceTrustKey,
    current: &WorkspaceTrustKey,
) -> bool {
    if current.prefix != root.prefix {
        return false;
    }
    current.rooted == root.rooted
        || (!current.rooted && root.rooted && current.components.is_empty())
}

fn dedupe_trusted_workspaces(trusted_workspaces: &[PathBuf]) -> Vec<PathBuf> {
    let mut deduped = Vec::with_capacity(trusted_workspaces.len());
    let mut seen_keys = HashSet::with_capacity(trusted_workspaces.len());
    for candidate in trusted_workspaces {
        let Some(candidate_key) = workspace_trust_key(candidate) else {
            continue;
        };
        if !seen_keys.insert(candidate_key) {
            continue;
        }
        deduped.push(candidate.clone());
    }
    deduped
}

fn workspace_trust_path_matches_key(path: &Path, key: &WorkspaceTrustKey) -> bool {
    workspace_trust_key(path).is_some_and(|candidate| candidate.eq(key))
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct WorkspaceTrustKey {
    prefix: Option<String>,
    rooted: bool,
    components: Vec<String>,
}

fn workspace_trust_key(path: &Path) -> Option<WorkspaceTrustKey> {
    if path.as_os_str().is_empty() {
        return None;
    }
    #[cfg(windows)]
    let normalized_path;
    #[cfg(windows)]
    let path = {
        let text = path.as_os_str().to_string_lossy();
        if text.starts_with(r"\\?\") {
            normalized_path = crate::native_paths::normalize_native_path(path.to_path_buf());
            normalized_path.as_path()
        } else {
            path
        }
    };

    let mut key = WorkspaceTrustKey {
        prefix: None,
        rooted: false,
        components: Vec::new(),
    };
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                key.prefix = Some(normalize_workspace_trust_component(prefix.as_os_str()));
            }
            Component::RootDir => key.rooted = true,
            Component::CurDir => {}
            Component::ParentDir => {
                if key
                    .components
                    .last()
                    .is_some_and(|component| component != "..")
                {
                    key.components.pop();
                } else {
                    key.components.push("..".to_owned());
                }
            }
            Component::Normal(component) => {
                key.components
                    .push(normalize_workspace_trust_component(component));
            }
        }
    }

    Some(key)
}

fn normalize_workspace_trust_component(component: &OsStr) -> String {
    let component = component.to_string_lossy();
    #[cfg(windows)]
    {
        if component.is_ascii() {
            let mut component = component.into_owned();
            component.make_ascii_lowercase();
            component
        } else {
            component.to_lowercase()
        }
    }
    #[cfg(not(windows))]
    {
        component.into_owned()
    }
}

impl KuroyaApp {
    /// Arms the "trust this workspace?" prompt for the current workspace when
    /// it is untrusted. Called after startup/session restore and after a
    /// workspace is opened, so declining keeps the prompt hidden until the
    /// workspace actually changes.
    pub(crate) fn arm_workspace_trust_prompt(&mut self) {
        self.pending_workspace_trust_prompt = workspace_trust_prompt_root(
            self.workspace_placeholder,
            self.workspace_trusted,
            self.workspace.root.clone(),
        );
    }

    pub(crate) fn keep_workspace_restricted(&mut self) {
        self.pending_workspace_trust_prompt = None;
    }

    pub(crate) fn trust_current_workspace(&mut self) {
        self.pending_workspace_trust_prompt = None;
        self.trusted_workspaces =
            trust_workspace(&self.trusted_workspaces, self.workspace.root.clone());
        self.workspace_trusted = true;
        self.reload_settings();
        for index in 0..self.buffers.len() {
            let id = self.buffers[index].id();
            self.notify_lsp_open(id);
        }
        self.spawn_workspace_task_load();
        self.spawn_plugin_discovery();
        if let Err(error) = self.save_app_state() {
            self.status = trusted_workspace_save_failure_status(&error.to_string());
        } else {
            self.status = "Trusted current workspace".to_owned();
        }
    }

    pub(crate) fn revoke_current_workspace_trust(&mut self) {
        self.trusted_workspaces =
            revoke_workspace_trust(&self.trusted_workspaces, &self.workspace.root);
        self.workspace_trusted = false;
        self.notify_lsp_close_all();
        for client in self.lsp_clients.values() {
            client.shutdown();
        }
        self.reset_workspace_lsp_clients();
        self.clear_workspace_tasks_for_restricted_workspace();
        self.reset_workspace_lsp_ui_state();
        self.clear_pending_source_control_mutations_for_restricted_workspace();
        self.invalidate_workspace_plugin_discovery();
        self.clear_workspace_plugins();
        self.reload_settings();
        if let Err(error) = self.save_app_state() {
            self.status = revoked_workspace_trust_save_failure_status(&error.to_string());
        } else {
            self.status = "Revoked current workspace trust".to_owned();
        }
    }
}

fn trusted_workspace_save_failure_status(error: &str) -> String {
    let error = display_error_label_cow(error);
    format!(
        "Trusted workspace, but could not save trust state: {}",
        error.as_ref()
    )
}

fn revoked_workspace_trust_save_failure_status(error: &str) -> String {
    let error = display_error_label_cow(error);
    format!(
        "Revoked workspace trust, but could not save trust state: {}",
        error.as_ref()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        folding::FoldedRange,
        lsp_completion_resolve::CompletionPreviewResolveKey,
        lsp_hover_cache::{LspHoverCacheKey, MAX_LSP_HOVER_CACHE_ENTRIES, store_hover_cache},
        lsp_rename_preview::LspRenamePreviewRow,
        path_display::DISPLAY_ERROR_LABEL_MAX_CHARS,
        snippet_session::SnippetSession,
        source_control_runtime::source_control_app_for_test,
        terminal_process::TerminalCommand,
        transient_state::{
            LspHoverPopup, LspHoverRequestTarget, LspSignatureHelpPopup, PendingLspHover,
            PendingSourceControlCommitSave, PendingSourceControlDiscard,
            PendingSourceControlEmptyCommit, PendingSourceControlProtectedBranchCommit,
            PendingSourceControlSmartCommit, PendingSourceControlStashSave,
        },
        workspace_tasks_runtime::{RunningWorkspaceTask, workspace_task_fingerprint},
    };
    use kuroya_core::{
        Diagnostic, DiagnosticSeverity, GitSmartCommitChanges, LspCallHierarchyCall,
        LspCallHierarchyItem, LspCodeAction, LspCodeLens, LspCompletionItem, LspDocumentHighlight,
        LspDocumentSymbol, LspFoldingRange, LspInlayHint, LspParameterInformation, LspReference,
        LspSemanticToken, LspSignatureHelp, LspSignatureInformation, LspTextEdit,
        LspTypeHierarchyItem, LspWorkspaceSymbol, WorkspaceTask, WorkspaceTaskKind,
    };
    use serde_json::json;
    use std::{
        collections::BTreeMap,
        fs,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn workspace_trust_tracks_exact_workspace_roots() {
        let first = PathBuf::from("workspace-a");
        let second = PathBuf::from("workspace-b");
        let trusted = trust_workspace(&[], first.clone());
        let trusted = trust_workspace(&trusted, first.clone());
        let trusted = trust_workspace(&trusted, second.clone());

        assert_eq!(trusted, vec![second.clone(), first.clone()]);
        assert!(workspace_is_trusted(&trusted, &first));
        assert!(workspace_is_trusted(&trusted, &second));
        assert!(!workspace_is_trusted(&trusted, Path::new("workspace-c")));

        let trusted = revoke_workspace_trust(&trusted, &second);
        assert_eq!(trusted, vec![first]);
    }

    #[test]
    fn workspace_trust_matches_lexically_equivalent_roots() {
        let equivalent = PathBuf::from("workspace").join("src").join("..");
        let trusted = trust_workspace(&[], equivalent.clone());
        let trusted = trust_workspace(&trusted, PathBuf::from("workspace"));

        assert_eq!(trusted, vec![equivalent.clone()]);
        assert!(workspace_is_trusted(&trusted, Path::new("workspace")));
        assert!(trusted_workspace_paths_match(
            &equivalent,
            Path::new("workspace")
        ));

        let trusted = revoke_workspace_trust(&trusted, Path::new("workspace"));
        assert!(trusted.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn workspace_trust_matches_windows_verbatim_disk_paths() {
        assert!(trusted_workspace_paths_match(
            Path::new(r"C:\Projects\Kuroya\empty-workspace"),
            Path::new(r"\\?\C:\Projects\Kuroya\empty-workspace")
        ));
        assert!(workspace_path_contains_lexically(
            Path::new(r"C:\Projects\Kuroya\empty-workspace"),
            Path::new(r"\\?\C:\Projects\Kuroya\empty-workspace\.kuroya\settings.toml")
        ));
    }

    #[test]
    fn trust_workspace_dedupes_existing_lexical_equivalent_roots() {
        let trusted = vec![
            PathBuf::from("workspace"),
            PathBuf::from("workspace").join("src").join(".."),
            PathBuf::from("other"),
            PathBuf::from("workspace").join("."),
        ];

        let trusted = trust_workspace(&trusted, PathBuf::from("other"));

        assert_eq!(
            trusted,
            vec![PathBuf::from("workspace"), PathBuf::from("other")]
        );
    }

    #[test]
    fn trust_workspace_drops_invalid_empty_roots_without_losing_raw_valid_paths() {
        let raw = PathBuf::from("workspace").join("src").join("..");
        let trusted = trust_workspace(&[PathBuf::new(), raw.clone()], PathBuf::new());

        assert_eq!(trusted, vec![raw]);
        assert!(!workspace_is_trusted(&trusted, Path::new("")));
    }

    #[test]
    fn revoke_workspace_trust_with_invalid_root_still_cleans_invalid_state() {
        let trusted = vec![
            PathBuf::new(),
            PathBuf::from("workspace"),
            PathBuf::from("workspace").join("."),
        ];

        let trusted = revoke_workspace_trust(&trusted, Path::new(""));

        assert_eq!(trusted, vec![PathBuf::from("workspace")]);
    }

    #[test]
    fn revoke_workspace_trust_cleans_invalid_duplicates_and_preserves_raw_paths() {
        let raw_workspace = PathBuf::from("workspace").join("src").join("..");
        let trusted = vec![
            PathBuf::new(),
            raw_workspace.clone(),
            PathBuf::from("workspace"),
            PathBuf::from("other"),
            PathBuf::from("other").join("."),
        ];

        let trusted = revoke_workspace_trust(&trusted, Path::new("missing"));

        assert_eq!(trusted, vec![raw_workspace, PathBuf::from("other")]);
    }

    #[test]
    fn workspace_path_contains_lexically_rejects_parent_escape() {
        let root = Path::new("workspace/current");

        assert!(workspace_path_contains_lexically(
            root,
            Path::new("workspace/current/src/main.rs")
        ));
        assert!(workspace_path_contains_lexically(
            root,
            Path::new("workspace/current/./src/../main.rs")
        ));
        assert!(!workspace_path_contains_lexically(
            root,
            Path::new("workspace/current/../old/src/main.rs")
        ));
        assert!(!workspace_path_contains_lexically(
            root,
            Path::new("workspace/currentness/src/main.rs")
        ));
        assert!(!workspace_path_contains_lexically(
            Path::new("workspace"),
            Path::new("../../../workspace/secret.rs")
        ));
    }

    #[test]
    fn workspace_path_stays_within_root_rejects_parent_reentry() {
        let root = Path::new("workspace/current");

        assert!(workspace_path_stays_within_root_lexically(
            root,
            Path::new("workspace/current/src/../main.rs")
        ));
        assert!(!workspace_path_stays_within_root_lexically(
            root,
            Path::new("workspace/current/../current/src/main.rs")
        ));
        assert!(!workspace_path_stays_within_root_lexically(
            root,
            Path::new("workspace/other/../current/src/main.rs")
        ));
        assert!(!workspace_path_stays_within_root_lexically(
            root,
            Path::new("workspace/current/src/../../current/main.rs")
        ));
    }

    #[test]
    fn workspace_path_stays_within_current_dir_root_rejects_escape() {
        assert!(workspace_path_stays_within_root_lexically(
            Path::new("."),
            Path::new("src/../main.rs")
        ));
        assert!(!workspace_path_stays_within_root_lexically(
            Path::new("."),
            Path::new("../workspace/main.rs")
        ));
    }

    #[test]
    fn workspace_path_contains_lexically_handles_current_dir_root() {
        assert!(workspace_path_contains_lexically(
            Path::new("."),
            Path::new("src/main.rs")
        ));
        assert!(!workspace_path_contains_lexically(
            Path::new("."),
            Path::new("../outside.rs")
        ));
        assert!(!workspace_path_contains_lexically(
            Path::new("."),
            Path::new("../../.kuroya/plugins/example/plugin.toml")
        ));
    }

    #[test]
    fn workspace_path_contains_lexically_keeps_root_style_separate() {
        assert!(!workspace_path_contains_lexically(
            Path::new("workspace"),
            Path::new("/workspace/src/main.rs")
        ));
    }

    #[cfg(windows)]
    #[test]
    fn workspace_trust_matches_windows_paths_case_insensitively() {
        let trusted = vec![PathBuf::from(r"C:\Repo\Project")];

        assert!(workspace_is_trusted(
            &trusted,
            Path::new(r"c:/repo/project/.")
        ));
    }

    #[cfg(windows)]
    #[test]
    fn workspace_path_contains_lexically_matches_windows_case_insensitively() {
        assert!(workspace_path_contains_lexically(
            Path::new(r"C:\Repo\Project"),
            Path::new(r"c:/repo/project/src/main.rs")
        ));
    }

    #[test]
    fn restricted_workspace_cleanup_clears_source_control_mutation_prompts() {
        let mut app = source_control_app_for_test(PathBuf::from("workspace"), true);
        app.pending_source_control_discard = Some(PendingSourceControlDiscard {
            paths: vec![PathBuf::from("workspace/src/main.rs")],
        });
        app.pending_source_control_smart_commit = Some(PendingSourceControlSmartCommit {
            request_id: 1,
            message: "ship it".to_owned(),
            smart_commit_changes: GitSmartCommitChanges::All,
            change_count: 1,
        });
        app.pending_source_control_empty_commit = Some(PendingSourceControlEmptyCommit {
            request_id: 2,
            message: "empty".to_owned(),
        });
        app.pending_source_control_protected_branch_commit =
            Some(PendingSourceControlProtectedBranchCommit {
                request_id: 3,
                message: "ship it".to_owned(),
                smart_commit_changes: None,
                allow_empty: false,
                branch: "main".to_owned(),
                pattern: "main".to_owned(),
            });
        app.pending_source_control_commit_save = Some(PendingSourceControlCommitSave::Confirm {
            request_id: 4,
            message: "ship it".to_owned(),
            smart_commit_changes: None,
            allow_empty: false,
            ids: vec![1],
        });
        app.pending_source_control_stash_save = Some(PendingSourceControlStashSave::Confirm {
            message: "work in progress".to_owned(),
            ids: vec![1],
        });
        let branch_operation_request = app.reserve_source_control_branch_operation_request();

        app.clear_pending_source_control_mutations_for_restricted_workspace();

        assert!(app.pending_source_control_discard.is_none());
        assert!(app.pending_source_control_smart_commit.is_none());
        assert!(app.pending_source_control_empty_commit.is_none());
        assert!(app.pending_source_control_protected_branch_commit.is_none());
        assert!(app.pending_source_control_commit_save.is_none());
        assert!(app.pending_source_control_stash_save.is_none());
        assert!(
            !app.source_control_branch_operation_in_flight_request_ids
                .contains(&branch_operation_request)
        );
        assert_ne!(
            app.source_control_branch_operation_active_request_id,
            branch_operation_request
        );
    }

    #[test]
    fn revoke_current_workspace_trust_clears_lsp_ui_state() {
        let root = temp_workspace("revoke-lsp-ui-state");
        fs::create_dir_all(root.join("src")).unwrap();
        let path = root.join("src/main.rs");
        let now = Instant::now();
        let mut app = source_control_app_for_test(root.clone(), true);

        app.diagnostics
            .replace_lsp(path.clone(), vec![diagnostic(&path, "stale")]);
        app.static_diagnostics_active_request_ids.insert(7, 1);
        app.static_diagnostics_in_flight_request_ids.insert(7, 1);
        app.static_diagnostics_reload_queued.insert(7);
        app.pending_lsp_diagnostics.queue(
            path.clone(),
            Some(1),
            vec![diagnostic(&path, "pending")],
            now,
        );
        app.diagnostics_panel_selected = 4;
        app.symbols_panel = true;
        app.document_symbols = vec![document_symbol(&path)];
        app.document_symbols_path = Some(path.clone());
        app.document_symbols_selected = 1;
        app.workspace_symbols_open = true;
        app.workspace_symbol_query = "main".to_owned();
        app.workspace_symbol_submitted_query = "main".to_owned();
        app.workspace_symbol_submitted_path = Some(path.clone());
        app.workspace_symbols = vec![workspace_symbol(&path)];
        app.workspace_symbols_selected = 1;
        app.completion_open = true;
        app.completion_items = vec![completion_item(&path)];
        app.completion_buffer_id = Some(7);
        app.completion_path = Some(path.clone());
        app.completion_version = Some(3);
        app.completion_line = 2;
        app.completion_column = 4;
        app.completion_prefix = "pri".to_owned();
        app.completion_selected = 1;
        app.completion_preview_resolve_in_flight
            .push(completion_resolve_key(&path));
        app.completion_preview_resolve_recent_attempts
            .push(completion_resolve_key(&path));
        app.snippet_session = SnippetSession::new_grouped(7, vec![vec![0..1]]);
        app.pending_completion_requests.insert(7, now);
        app.pending_signature_help_requests.insert(7, now);
        app.pending_format_on_type_requests.insert(7, now);
        app.signature_help = Some(LspSignatureHelpPopup {
            id: 7,
            path: path.clone(),
            line: 1,
            column: 1,
            help: signature_help(),
        });
        app.code_actions_open = true;
        app.code_actions = vec![code_action(&path)];
        app.code_actions_buffer_id = Some(7);
        app.code_actions_path = Some(path.clone());
        app.code_actions_version = Some(3);
        app.code_actions_line = 2;
        app.code_actions_column = 4;
        app.code_actions_selected = 1;
        app.references_open = true;
        app.references = vec![reference(&path)];
        app.references_path = Some(path.clone());
        app.references_line = 2;
        app.references_column = 4;
        app.references_selected = 1;
        app.call_hierarchy_open = true;
        let call_item = call_hierarchy_item(&path);
        app.call_hierarchy_root = Some(call_item.clone());
        app.call_hierarchy_incoming = vec![LspCallHierarchyCall {
            item: call_item.clone(),
            ranges: Vec::new(),
        }];
        app.call_hierarchy_outgoing = vec![LspCallHierarchyCall {
            item: call_item,
            ranges: Vec::new(),
        }];
        app.call_hierarchy_selected = 1;
        app.call_hierarchy_path = Some(path.clone());
        app.call_hierarchy_line = 2;
        app.call_hierarchy_column = 4;
        app.type_hierarchy_open = true;
        let type_item = type_hierarchy_item(&path);
        app.type_hierarchy_root = Some(type_item.clone());
        app.type_hierarchy_supertypes = vec![type_item.clone()];
        app.type_hierarchy_subtypes = vec![type_item];
        app.type_hierarchy_selected = 1;
        app.type_hierarchy_path = Some(path.clone());
        app.type_hierarchy_line = 2;
        app.type_hierarchy_column = 4;
        app.pending_lsp_hover = Some(PendingLspHover {
            pane_id: app.active_pane,
            buffer_id: 7,
            char_idx: 3,
            version: 3,
            started_at: now,
            requested: true,
        });
        app.lsp_hover_request = Some(LspHoverRequestTarget::from_request(
            7,
            path.clone(),
            3,
            1,
            2,
        ));
        app.lsp_hover = Some(LspHoverPopup {
            id: 7,
            path: path.clone(),
            line: 1,
            column: 2,
            contents: "hover".to_owned(),
            opened_at: now,
        });
        store_hover_cache(
            &mut app.lsp_hover_cache,
            LspHoverCacheKey::new(path.clone(), 3, 1, 2),
            "hover".to_owned(),
            MAX_LSP_HOVER_CACHE_ENTRIES,
        );
        app.document_highlights_path = Some(path.clone());
        app.document_highlights = vec![LspDocumentHighlight {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 4,
            kind: Some(1),
        }];
        app.folding_ranges
            .insert(path.clone(), vec![folding_range()]);
        app.inlay_hints.insert(path.clone(), vec![inlay_hint()]);
        app.code_lenses.insert(path.clone(), vec![code_lens()]);
        app.semantic_tokens
            .insert(path.clone(), vec![semantic_token()]);
        app.folded_ranges.insert(
            path.clone(),
            vec![FoldedRange {
                start_line: 1,
                end_line: 3,
            }],
        );
        app.pending_fold_line = Some((path.clone(), 2));
        app.lsp_rename_open = true;
        app.lsp_rename_input = "renamed".to_owned();
        app.lsp_rename_preview_open = true;
        app.lsp_rename_preview_new_name = "renamed".to_owned();
        app.lsp_rename_preview_edits = vec![text_edit(&path)];
        app.lsp_rename_preview_rows = vec![LspRenamePreviewRow::Header { path: path.clone() }];
        app.lsp_rename_preview_versions.insert(path, 3);

        app.revoke_current_workspace_trust();

        assert!(!app.workspace_trusted);
        assert!(app.diagnostics.is_empty());
        assert!(app.static_diagnostics_active_request_ids.is_empty());
        assert!(app.static_diagnostics_in_flight_request_ids.is_empty());
        assert!(app.static_diagnostics_reload_queued.is_empty());
        assert!(
            app.pending_lsp_diagnostics
                .take_due(
                    now + std::time::Duration::from_secs(1),
                    std::time::Duration::ZERO
                )
                .is_empty()
        );
        assert_eq!(app.diagnostics_panel_selected, 0);
        assert!(!app.symbols_panel);
        assert!(app.document_symbols.is_empty());
        assert!(app.document_symbols_path.is_none());
        assert_eq!(app.document_symbols_selected, 0);
        assert!(!app.workspace_symbols_open);
        assert!(app.workspace_symbol_query.is_empty());
        assert!(app.workspace_symbol_submitted_query.is_empty());
        assert!(app.workspace_symbol_submitted_path.is_none());
        assert!(app.workspace_symbols.is_empty());
        assert_eq!(app.workspace_symbols_selected, 0);
        assert!(!app.completion_open);
        assert!(app.completion_items.is_empty());
        assert!(app.completion_buffer_id.is_none());
        assert!(app.completion_path.is_none());
        assert!(app.completion_version.is_none());
        assert_eq!(app.completion_line, 0);
        assert_eq!(app.completion_column, 0);
        assert!(app.completion_prefix.is_empty());
        assert_eq!(app.completion_selected, 0);
        assert!(app.completion_preview_resolve_in_flight.is_empty());
        assert!(app.completion_preview_resolve_recent_attempts.is_empty());
        assert!(app.snippet_session.is_none());
        assert!(app.pending_completion_requests.is_empty());
        assert!(app.pending_signature_help_requests.is_empty());
        assert!(app.pending_format_on_type_requests.is_empty());
        assert!(app.signature_help.is_none());
        assert!(!app.code_actions_open);
        assert!(app.code_actions.is_empty());
        assert!(app.code_actions_buffer_id.is_none());
        assert!(app.code_actions_path.is_none());
        assert!(app.code_actions_version.is_none());
        assert_eq!(app.code_actions_line, 0);
        assert_eq!(app.code_actions_column, 0);
        assert_eq!(app.code_actions_selected, 0);
        assert!(!app.references_open);
        assert!(app.references.is_empty());
        assert!(app.references_path.is_none());
        assert_eq!(app.references_line, 0);
        assert_eq!(app.references_column, 0);
        assert_eq!(app.references_selected, 0);
        assert!(!app.call_hierarchy_open);
        assert!(app.call_hierarchy_root.is_none());
        assert!(app.call_hierarchy_incoming.is_empty());
        assert!(app.call_hierarchy_outgoing.is_empty());
        assert_eq!(app.call_hierarchy_selected, 0);
        assert!(app.call_hierarchy_path.is_none());
        assert_eq!(app.call_hierarchy_line, 0);
        assert_eq!(app.call_hierarchy_column, 0);
        assert!(!app.type_hierarchy_open);
        assert!(app.type_hierarchy_root.is_none());
        assert!(app.type_hierarchy_supertypes.is_empty());
        assert!(app.type_hierarchy_subtypes.is_empty());
        assert_eq!(app.type_hierarchy_selected, 0);
        assert!(app.type_hierarchy_path.is_none());
        assert_eq!(app.type_hierarchy_line, 0);
        assert_eq!(app.type_hierarchy_column, 0);
        assert!(app.pending_lsp_hover.is_none());
        assert!(app.lsp_hover_request.is_none());
        assert!(app.lsp_hover.is_none());
        assert!(app.lsp_hover_cache.is_empty());
        assert!(app.document_highlights_path.is_none());
        assert!(app.document_highlights.is_empty());
        assert!(app.folding_ranges.is_empty());
        assert!(app.inlay_hints.is_empty());
        assert!(app.code_lenses.is_empty());
        assert!(app.semantic_tokens.is_empty());
        assert!(app.folded_ranges.is_empty());
        assert!(app.pending_fold_line.is_none());
        assert!(!app.lsp_rename_open);
        assert!(app.lsp_rename_input.is_empty());
        assert!(!app.lsp_rename_preview_open);
        assert!(app.lsp_rename_preview_new_name.is_empty());
        assert!(app.lsp_rename_preview_edits.is_empty());
        assert!(app.lsp_rename_preview_rows.is_empty());
        assert!(app.lsp_rename_preview_versions.is_empty());

        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn revoke_current_workspace_trust_closes_running_workspace_task_sessions() {
        let workspace = temp_workspace("revoke-task-sessions");
        std::fs::create_dir_all(&workspace).unwrap();

        let mut app = source_control_app_for_test(workspace.clone(), true);
        let task = WorkspaceTask {
            name: "Build".to_owned(),
            command: "cargo".to_owned(),
            args: vec!["build".to_owned()],
            cwd: Some(workspace.clone()),
            env: BTreeMap::new(),
            kind: WorkspaceTaskKind::Build,
            default: true,
        };
        let task_rx = app.terminal.add_process_session_for_test(7);
        let unrelated_rx = app.terminal.add_process_session_for_test(8);
        app.workspace_tasks = vec![task.clone()];
        app.workspace_tasks_loaded = true;
        app.workspace_tasks_loading = true;
        app.pending_workspace_task_kind = Some(WorkspaceTaskKind::Build);
        app.running_workspace_tasks = vec![
            RunningWorkspaceTask {
                task_index: 0,
                fingerprint: workspace_task_fingerprint(&task),
                session_id: 7,
            },
            RunningWorkspaceTask {
                task_index: 0,
                fingerprint: workspace_task_fingerprint(&task),
                session_id: 99,
            },
        ];

        app.revoke_current_workspace_trust();

        assert!(!app.workspace_trusted);
        assert!(app.workspace_tasks.is_empty());
        assert!(!app.workspace_tasks_loaded);
        assert!(!app.workspace_tasks_loading);
        assert!(app.pending_workspace_task_kind.is_none());
        assert!(app.running_workspace_tasks.is_empty());
        assert_eq!(app.terminal.session_ids_for_test(), vec![8]);
        match task_rx.try_recv().unwrap() {
            TerminalCommand::Close => {}
            TerminalCommand::Input(_) | TerminalCommand::Resize(_) => {
                panic!("expected task terminal close command")
            }
        }
        assert!(unrelated_rx.try_recv().is_err());

        std::fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn workspace_trust_prompt_arms_only_for_untrusted_non_placeholder_roots() {
        let root = PathBuf::from("workspace");

        assert_eq!(
            workspace_trust_prompt_root(false, false, root.clone()),
            Some(root)
        );
        assert_eq!(
            workspace_trust_prompt_root(false, true, PathBuf::new()),
            None
        );
        assert_eq!(
            workspace_trust_prompt_root(true, false, PathBuf::new()),
            None
        );
        assert_eq!(
            workspace_trust_prompt_root(true, true, PathBuf::new()),
            None
        );
    }

    #[test]
    fn arm_workspace_trust_prompt_follows_current_workspace_trust_state() {
        let root = PathBuf::from("workspace");
        let mut app = source_control_app_for_test(root.clone(), false);

        app.arm_workspace_trust_prompt();
        assert_eq!(app.pending_workspace_trust_prompt, Some(root.clone()));

        app.workspace_trusted = true;
        app.arm_workspace_trust_prompt();
        assert_eq!(app.pending_workspace_trust_prompt, None);

        app.workspace_trusted = false;
        app.workspace_placeholder = true;
        app.arm_workspace_trust_prompt();
        assert_eq!(app.pending_workspace_trust_prompt, None);
    }

    #[test]
    fn trust_current_workspace_clears_prompt_and_reenables_restricted_features() {
        let root = temp_workspace("trust-prompt-reenable");
        fs::create_dir_all(&root).unwrap();
        let mut app = source_control_app_for_test(root.clone(), false);
        let app_state_path = root.join("app-state.json");
        app.app_state_path_override = Some(app_state_path.clone());
        app.pending_workspace_trust_prompt = Some(root.clone());

        app.trust_current_workspace();

        assert!(app.workspace_trusted);
        assert!(app.pending_workspace_trust_prompt.is_none());
        assert!(workspace_is_trusted(&app.trusted_workspaces, &root));
        assert_eq!(app.workspace_plugins_in_flight_request_id, Some(1));
        assert_eq!(app.workspace_tasks_in_flight_request_id, Some(1));
        let saved = fs::read_to_string(&app_state_path).unwrap();
        let saved: serde_json::Value = serde_json::from_str(&saved).unwrap();
        assert!(
            saved["trusted_workspaces"]
                .as_array()
                .expect("trusted workspaces array")
                .iter()
                .any(|path| path.as_str() == Some(root.to_string_lossy().as_ref()))
        );

        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn keep_workspace_restricted_clears_prompt_without_trusting() {
        let root = PathBuf::from("workspace");
        let mut app = source_control_app_for_test(root.clone(), false);
        app.pending_workspace_trust_prompt = Some(root);

        app.keep_workspace_restricted();

        assert!(app.pending_workspace_trust_prompt.is_none());
        assert!(!app.workspace_trusted);
        assert!(app.trusted_workspaces.is_empty());
    }

    #[test]
    fn workspace_trust_save_failure_status_sanitizes_error_text() {
        let error = format!("first line\nsecond line \u{202e}{}", "x".repeat(400));
        let trusted = trusted_workspace_save_failure_status(&error);
        let revoked = revoked_workspace_trust_save_failure_status(&error);

        assert_workspace_trust_status_error_is_safe(
            &trusted,
            "Trusted workspace, but could not save trust state: ",
        );
        assert_workspace_trust_status_error_is_safe(
            &revoked,
            "Revoked workspace trust, but could not save trust state: ",
        );
    }

    fn assert_workspace_trust_status_error_is_safe(status: &str, prefix: &str) {
        assert!(status.starts_with(prefix));
        assert!(!status.contains('\n'));
        assert!(!status.contains('\u{202e}'));
        assert!(status.contains("..."));
        assert!(status[prefix.len()..].chars().count() <= DISPLAY_ERROR_LABEL_MAX_CHARS);
    }

    fn diagnostic(path: &Path, message: &str) -> Diagnostic {
        Diagnostic {
            path: path.to_path_buf(),
            line: 1,
            column: 1,
            char_range: 0..1,
            severity: DiagnosticSeverity::Warning,
            source: "lsp".to_owned(),
            message: message.to_owned(),
            unused: false,
            deprecated: false,
        }
    }

    fn text_edit(path: &Path) -> LspTextEdit {
        LspTextEdit {
            path: path.to_path_buf(),
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 4,
            new_text: "renamed".to_owned(),
        }
    }

    fn document_symbol(path: &Path) -> LspDocumentSymbol {
        LspDocumentSymbol {
            name: "main".to_owned(),
            detail: None,
            kind: 12,
            path: path.to_path_buf(),
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 4,
            depth: 0,
        }
    }

    fn workspace_symbol(path: &Path) -> LspWorkspaceSymbol {
        LspWorkspaceSymbol {
            name: "main".to_owned(),
            detail: None,
            kind: 12,
            path: path.to_path_buf(),
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 4,
        }
    }

    fn completion_item(path: &Path) -> LspCompletionItem {
        LspCompletionItem {
            label: "println".to_owned(),
            detail: Some("macro".to_owned()),
            documentation: Some("Prints a line".to_owned()),
            kind: Some(3),
            deprecated: false,
            is_snippet: false,
            sort_text: None,
            filter_text: None,
            preselect: false,
            commit_characters: Vec::new(),
            insert_text: "println!".to_owned(),
            snippet_selection: None,
            snippet_tabstops: Vec::new(),
            snippet_tabstop_groups: Vec::new(),
            text_edit: Some(text_edit(path)),
            insert_text_edit: None,
            additional_text_edits: Vec::new(),
            resolve_payload: None,
        }
    }

    fn completion_resolve_key(path: &Path) -> CompletionPreviewResolveKey {
        CompletionPreviewResolveKey {
            id: 7,
            path: path.to_path_buf(),
            version: 3,
            line: 1,
            character: 2,
            selected: 0,
            item: Box::new(completion_item(path)),
        }
    }

    fn signature_help() -> LspSignatureHelp {
        LspSignatureHelp {
            signatures: vec![LspSignatureInformation {
                label: "println!(...)".to_owned(),
                documentation: None,
                parameters: vec![LspParameterInformation {
                    label: "args".to_owned(),
                    documentation: None,
                }],
            }],
            active_signature: 0,
            active_parameter: Some(0),
        }
    }

    fn code_action(path: &Path) -> LspCodeAction {
        LspCodeAction {
            title: "Apply fix".to_owned(),
            kind: Some("quickfix".to_owned()),
            edits: vec![text_edit(path)],
            document_changes: Vec::new(),
            resolve_payload: None,
        }
    }

    fn reference(path: &Path) -> LspReference {
        LspReference {
            path: path.to_path_buf(),
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 4,
        }
    }

    fn call_hierarchy_item(path: &Path) -> LspCallHierarchyItem {
        LspCallHierarchyItem {
            name: "main".to_owned(),
            detail: None,
            kind: 12,
            path: path.to_path_buf(),
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 4,
            raw: json!({"name": "main"}),
        }
    }

    fn type_hierarchy_item(path: &Path) -> LspTypeHierarchyItem {
        LspTypeHierarchyItem {
            name: "main".to_owned(),
            detail: None,
            kind: 12,
            path: path.to_path_buf(),
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 4,
            raw: json!({"name": "main"}),
        }
    }

    fn folding_range() -> LspFoldingRange {
        LspFoldingRange {
            start_line: 1,
            start_column: Some(1),
            end_line: 3,
            end_column: Some(1),
            kind: Some("region".to_owned()),
        }
    }

    fn inlay_hint() -> LspInlayHint {
        LspInlayHint {
            line: 1,
            column: 4,
            label: ": i32".to_owned(),
            kind: Some(1),
        }
    }

    fn code_lens() -> LspCodeLens {
        LspCodeLens {
            line: 1,
            column: 1,
            title: "Run".to_owned(),
            command: Some("run".to_owned()),
            command_arguments: None,
            resolve_payload: None,
        }
    }

    fn semantic_token() -> LspSemanticToken {
        LspSemanticToken {
            line: 1,
            column: 1,
            length: 4,
            token_type: "function".to_owned(),
            modifiers: Vec::new(),
        }
    }

    fn temp_workspace(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "kuroya-workspace-trust-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
