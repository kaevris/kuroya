use crate::{
    KuroyaApp,
    app_session::{
        restored_explorer_expanded_paths, source_control_sort_mode_from_persisted,
        source_control_view_mode_from_persisted, workspace_descendant_path_for_session,
    },
    buffer_find_history::{
        MAX_BUFFER_FIND_HISTORY, buffer_find_history_enabled, normalize_buffer_find_query_history,
        normalize_buffer_find_replacement_history,
    },
    command_palette_items::{
        MAX_COMMAND_PALETTE_QUERY_MEMORY, MAX_COMMAND_PALETTE_RECENT_COMMANDS,
        normalize_command_palette_query_memory, normalize_recent_palette_commands,
    },
    explorer_runtime::explorer_ancestor_paths,
    file_runtime::file_path_known_openable,
    folding::{clamp_folded_ranges_for_line_count, folded_ranges_from_session},
    history::{
        CLOSED_FILE_HISTORY_LIMIT, NAVIGATION_HISTORY_LIMIT, normalize_closed_file_history,
        normalize_navigation_history,
    },
    layout::{
        clamp_diagnostics_panel_width, clamp_explorer_width, clamp_source_control_width,
        clamp_symbols_panel_width, clamp_terminal_height,
    },
    lsp_workspace_symbol_ranking::{
        MAX_WORKSPACE_SYMBOL_QUERY_MEMORY, normalize_workspace_symbol_query_memory,
    },
    path_display::sanitized_display_label_cow,
    persistence::{BufferViewState, PaneBufferViewState, PersistedSession, SkippedRecoveredBuffer},
    persistence_session::normalize_persisted_session_paths_for_restore,
    project_search_state::{MAX_PROJECT_SEARCH_RECENT_QUERIES, normalize_recent_project_searches},
    quick_open::{
        MAX_QUICK_OPEN_QUERY_MEMORY, MAX_QUICK_OPEN_RECENT_FILES,
        normalize_quick_open_query_memory, normalize_quick_open_recent_files,
    },
    session_state::{
        EditorPane, apply_buffer_history_state, apply_buffer_view_state,
        apply_recovered_buffer_history_state, apply_recovered_buffer_view_state,
        horizontal_scroll_offset_from_pane_view_state,
        horizontal_scroll_offset_from_recovered_view_state,
        horizontal_scroll_offset_from_view_state, pane_scroll_line_from_view_state,
    },
    source_control_panel::{
        SOURCE_CONTROL_COMMIT_HISTORY_LIMIT, normalize_source_control_commit_history,
    },
    theme::selected_theme_index_with_plugins,
    workspace_state::{
        PaneId, path_set_contains_exact_or_lexically, paths_match_exact_or_lexically,
        remove_path_map_entry_exact_or_lexically,
    },
    workspace_trust::trusted_workspace_paths_match,
};
use kuroya_core::{BufferId, TerminalHideOnStartup, TextBuffer};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    ffi::OsStr,
    fmt::Write as _,
    mem,
    path::{Component, Path, PathBuf},
};

mod panes;

const RESTORED_SESSION_STATUS_MAX_CHARS: usize = 160;
const RECOVERY_SKIP_GROUP_MAX: usize = 4;
const RECOVERY_SKIP_GROUP_LABEL_MAX_CHARS: usize = 48;

#[derive(Clone, Copy)]
struct PendingViewportScroll {
    line: usize,
    horizontal_offset: f32,
}

impl KuroyaApp {
    pub(crate) fn restore_session(&mut self, mut session: PersistedSession) {
        if self.has_keybinding_capture_in_progress() && !self.cancel_keybinding_capture() {
            return;
        }
        self.clear_session_restore_runtime_state();
        if !restore_session_workspace_root_matches(&self.workspace.root, &session.workspace_root) {
            clear_restore_path_state(&mut session);
        }
        normalize_persisted_session_paths_for_restore(&self.workspace.root, &mut session);
        self.merge_recent_projects(mem::take(&mut session.recent_projects));
        self.record_recent_project(self.workspace.root.clone());
        self.quick_open_recent_files = normalize_quick_open_recent_files(
            mem::take(&mut session.quick_open_recent_files),
            &self.workspace.root,
            MAX_QUICK_OPEN_RECENT_FILES,
        );
        self.quick_open_query_memory = normalize_quick_open_query_memory(
            mem::take(&mut session.quick_open_query_memory),
            &self.workspace.root,
            MAX_QUICK_OPEN_QUERY_MEMORY,
        );
        self.workspace_symbol_query_memory = normalize_workspace_symbol_query_memory(
            mem::take(&mut session.workspace_symbol_query_memory),
            &self.workspace.root,
            MAX_WORKSPACE_SYMBOL_QUERY_MEMORY,
        );
        self.command_recent = normalize_recent_palette_commands(
            mem::take(&mut session.command_recent),
            MAX_COMMAND_PALETTE_RECENT_COMMANDS,
        );
        self.command_query_memory = normalize_command_palette_query_memory(
            mem::take(&mut session.command_query_memory),
            MAX_COMMAND_PALETTE_QUERY_MEMORY,
        );
        self.navigation_back = normalize_navigation_history(
            mem::take(&mut session.navigation_back)
                .into_iter()
                .map(|location| location.into_navigation_location()),
            NAVIGATION_HISTORY_LIMIT,
        );
        self.navigation_forward = normalize_navigation_history(
            mem::take(&mut session.navigation_forward)
                .into_iter()
                .map(|location| location.into_navigation_location()),
            NAVIGATION_HISTORY_LIMIT,
        );
        self.closed_files = normalize_closed_file_history(
            mem::take(&mut session.closed_files)
                .into_iter()
                .map(|entry| entry.into_closed_file_entry()),
            CLOSED_FILE_HISTORY_LIMIT,
        );
        let restore_terminal = self.workspace_trusted
            && terminal_visible_after_startup(
                self.settings.terminal_hide_on_startup,
                session.terminal_visible,
                !session.terminal_sessions.is_empty(),
            );
        if !session.terminal_sessions.is_empty() {
            self.terminal.restore_terminal_sessions(
                &session.terminal_sessions,
                session.terminal_active_session,
                session.terminal_split_view,
                &session.terminal_split_weights,
                self.workspace_trusted,
            );
        }
        self.terminal.set_visible(restore_terminal);
        self.terminal_height = clamp_terminal_height(session.terminal_height);
        self.explorer_width = clamp_explorer_width(session.explorer_width);
        self.explorer_expanded =
            restored_explorer_expanded_paths(&self.workspace.root, session.explorer_expanded);
        self.explorer_revealed_path = session
            .explorer_revealed_path
            .and_then(|path| workspace_descendant_path_for_session(&self.workspace.root, &path));
        if let Some(path) = &self.explorer_revealed_path {
            self.explorer_expanded
                .extend(explorer_ancestor_paths(&self.workspace.root, path));
        }
        self.project_search = session.project_search_open;
        self.project_search_focus_query = self.project_search;
        self.project_search_query = mem::take(&mut session.project_search_query);
        self.project_search_case_sensitive = session.project_search_case_sensitive;
        self.project_search_whole_word = session.project_search_whole_word;
        self.project_search_regex = session.project_search_regex;
        self.project_search_include = mem::take(&mut session.project_search_include);
        self.project_search_exclude = mem::take(&mut session.project_search_exclude);
        self.project_search_recent = normalize_recent_project_searches(
            mem::take(&mut session.project_search_recent),
            MAX_PROJECT_SEARCH_RECENT_QUERIES,
        );
        self.buffer_find_open = session.buffer_find_open;
        self.buffer_find_query = mem::take(&mut session.buffer_find_query);
        self.buffer_find_replacement = mem::take(&mut session.buffer_find_replacement);
        self.buffer_find_case_sensitive = session.buffer_find_case_sensitive;
        self.buffer_find_whole_word = session.buffer_find_whole_word;
        self.buffer_find_regex = session.buffer_find_regex;
        self.buffer_find_preserve_case = session.buffer_find_preserve_case;
        self.buffer_find_match = 0;
        self.buffer_find_scope = None;
        self.buffer_find_cache.clear();
        self.buffer_find_query_history = if buffer_find_history_enabled(self.settings.find_history)
        {
            normalize_buffer_find_query_history(
                mem::take(&mut session.buffer_find_query_history),
                MAX_BUFFER_FIND_HISTORY,
            )
        } else {
            Default::default()
        };
        self.buffer_find_query_history_cursor = None;
        self.buffer_find_query_history_draft = None;
        self.buffer_find_replacement_history =
            if buffer_find_history_enabled(self.settings.find_replace_history) {
                normalize_buffer_find_replacement_history(
                    mem::take(&mut session.buffer_find_replacement_history),
                    MAX_BUFFER_FIND_HISTORY,
                )
            } else {
                Default::default()
            };
        self.buffer_find_replacement_history_cursor = None;
        self.buffer_find_replacement_history_draft = None;
        self.settings_panel_open = session.settings_panel_open;
        self.sync_settings_panel_inputs();
        self.theme_picker_open = session.theme_picker_open;
        self.theme_picker_selected =
            selected_theme_index_with_plugins(&self.settings.theme, &self.plugin_themes);
        self.keybindings_open = session.keybindings_open;
        self.keybindings_query.clear();
        self.keybindings_selected = 0;
        self.keybinding_capture_command = None;
        self.project_search_selected = 0;
        self.symbols_panel = session.symbols_panel_open;
        self.symbols_panel_placement = session.symbols_panel_placement;
        self.symbols_panel_width = clamp_symbols_panel_width(session.symbols_panel_width);
        self.diagnostics_panel = session.diagnostics_panel_open;
        self.diagnostics_panel_placement = session.diagnostics_panel_placement;
        self.diagnostics_panel_width =
            clamp_diagnostics_panel_width(session.diagnostics_panel_width);
        self.source_control = session.source_control_open;
        self.source_control_placement = session.source_control_placement;
        self.source_control_width = clamp_source_control_width(session.source_control_width);
        self.source_control_query = mem::take(&mut session.source_control_query);
        self.source_control_view =
            source_control_view_mode_from_persisted(session.source_control_view);
        self.source_control_sort =
            source_control_sort_mode_from_persisted(session.source_control_sort);
        self.source_control_commit_message = mem::take(&mut session.source_control_commit_message);
        self.source_control_commit_history = normalize_source_control_commit_history(
            mem::take(&mut session.source_control_commit_history),
            SOURCE_CONTROL_COMMIT_HISTORY_LIMIT,
        );
        self.source_control_commit_history_index = None;
        self.source_control_commit_next_request_id = 0;
        self.source_control_commit_active_request_id = 0;
        self.source_control_commit_in_flight_request_ids.clear();
        self.source_control_branch_operation_next_request_id = 0;
        self.source_control_branch_operation_active_request_id = 0;
        self.source_control_branch_operation_in_flight_request_ids
            .clear();
        self.source_control_stash_message = mem::take(&mut session.source_control_stash_message);
        self.source_control_stash_query = mem::take(&mut session.source_control_stash_query);
        self.source_control_stashes_open = session.source_control_stashes_open;
        self.source_control_stashes.clear();
        self.source_control_stash_selected = 0;
        self.source_control_stashes_next_request_id = 0;
        self.source_control_stashes_active_request_id = 0;
        self.source_control_stashes_in_flight_request_id = None;
        self.source_control_stashes_reload_queued = false;
        self.pending_restored_git_stashes_load =
            self.settings.git_enabled && self.source_control_stashes_open;
        self.source_control_history_open = session.source_control_history_open;
        self.source_control_history_query = mem::take(&mut session.source_control_history_query);
        self.source_control_history.clear();
        self.source_control_history_selected = 0;
        self.source_control_history_loading = false;
        self.source_control_history_requested_limit = 0;
        self.source_control_history_has_more = false;
        self.source_control_history_in_flight_request_id = None;
        self.source_control_history_reload_queued = false;
        self.pending_restored_git_history_load =
            self.settings.git_enabled && self.source_control_history_open;
        self.source_control_unstaged_collapsed = session.source_control_unstaged_collapsed;
        self.source_control_untracked_collapsed = session.source_control_untracked_collapsed;
        self.source_control_staged_collapsed = session.source_control_staged_collapsed;
        let session_view_states = mem::take(&mut session.view_states);
        let pane_view_states = mem::take(&mut session.pane_view_states);
        self.pending_view_states = HashMap::with_capacity(session_view_states.len());
        self.pending_view_states.extend(
            session_view_states
                .iter()
                .cloned()
                .map(|state| (state.path.clone(), state)),
        );
        self.pending_pane_view_states.clear();
        let session_history_states = mem::take(&mut session.history_states);
        self.pending_history_states = HashMap::with_capacity(session_history_states.len());
        self.pending_history_states.extend(
            session_history_states
                .into_iter()
                .map(|state| (state.path.clone(), state)),
        );
        let recovery_view_states = mem::take(&mut session.recovery_view_states);
        let mut pending_recovery_view_states = HashMap::with_capacity(recovery_view_states.len());
        pending_recovery_view_states.extend(
            recovery_view_states
                .into_iter()
                .map(|state| (state.recovery_index, state)),
        );
        let recovery_history_states = mem::take(&mut session.recovery_history_states);
        let mut pending_recovery_history_states =
            HashMap::with_capacity(recovery_history_states.len());
        pending_recovery_history_states.extend(
            recovery_history_states
                .into_iter()
                .map(|state| (state.recovery_index, state)),
        );
        self.folded_ranges = folded_ranges_from_session(&session.fold_states);

        let saved_open_file_order = session.open_files.clone();
        let mut restored_by_path = HashMap::with_capacity(session.recovery.len());
        for (recovery_index, recovered) in session.recovery.into_iter().enumerate() {
            let id = self.next_id();
            let path = recovered.path;
            let mut buffer = TextBuffer::from_text(id, path.clone(), recovered.text);
            buffer.set_word_separators(self.settings.word_separators.clone());
            if self.settings.read_only {
                buffer.set_read_only(true);
            }
            buffer.mark_dirty();
            if let Some(path) = path.as_ref() {
                clamp_folded_ranges_for_line_count(
                    &mut self.folded_ranges,
                    path,
                    buffer.len_lines(),
                );
                if let Some(view_state) =
                    remove_path_map_entry_exact_or_lexically(&mut self.pending_view_states, path)
                {
                    apply_buffer_view_state(&mut buffer, &view_state);
                }
                if let Some(history_state) =
                    remove_path_map_entry_exact_or_lexically(&mut self.pending_history_states, path)
                {
                    apply_buffer_history_state(&mut buffer, history_state);
                }
                restored_by_path.insert(path.clone(), id);
            } else {
                if let Some(view_state) = pending_recovery_view_states.remove(&recovery_index) {
                    let scroll_line = apply_recovered_buffer_view_state(&mut buffer, &view_state);
                    self.pending_scroll_lines.insert(id, scroll_line);
                    let horizontal_scroll_offset =
                        horizontal_scroll_offset_from_recovered_view_state(&view_state);
                    if horizontal_scroll_offset > 0.0 {
                        self.pending_horizontal_scroll_offsets
                            .insert(id, horizontal_scroll_offset);
                    }
                }
                if let Some(history_state) = pending_recovery_history_states.remove(&recovery_index)
                {
                    apply_recovered_buffer_history_state(&mut buffer, history_state);
                }
            }
            self.buffers.push(buffer);
            self.spawn_diagnostics_for(id);
            if path.is_some() {
                self.notify_lsp_open(id);
            }
        }

        reorder_restored_buffers_to_saved_order(&mut self.buffers, &saved_open_file_order);

        let pane_weights = session.pane_weights;
        let pane_ids_by_index = self.restore_session_panes(
            session.pane_paths,
            &pane_weights,
            session.active_pane_index,
            &restored_by_path,
        );

        let open_files = restorable_session_open_files(
            session.open_files,
            self.index.files(),
            &restored_by_path,
            Path::exists,
        );
        for path in open_files {
            self.spawn_open_file_with_activation(path, false);
        }
        self.restore_session_pane_view_states(
            &pane_ids_by_index,
            &pane_view_states,
            &session_view_states,
            &restored_by_path,
        );
        self.prune_unloadable_session_restore_paths(&restored_by_path);

        if let Some(active_path) = session.active_path {
            if let Some(id) = restored_buffer_id_for_path(&restored_by_path, &active_path) {
                let pane_id = self
                    .active_pane_holding_buffer(id)
                    .or_else(|| self.pane_id_for_buffer(id))
                    .unwrap_or(self.active_pane);
                self.set_active_buffer_in_pane(pane_id, id);
                self.pending_active_path = None;
            } else {
                self.pending_active_path =
                    path_set_contains_exact_or_lexically(&self.pending_open_paths, &active_path)
                        .then_some(active_path);
            }
        } else if let Some(id) = self.buffers.first().map(TextBuffer::id) {
            self.pending_active_path = None;
            self.set_active_buffer(id);
        } else {
            self.pending_active_path = None;
        }

        self.status = restored_session_status(self.buffers.len(), &session.recovery_skipped);
    }

    fn clear_session_restore_runtime_state(&mut self) {
        self.buffers.clear();
        self.virtual_buffer_labels.clear();
        self.diff_buffer_sources.clear();
        self.diff_cache.clear();
        self.diff_cache_pending.clear();
        self.merge_conflict_cache.clear();
        self.editor_bracket_overlay_cache.clear();
        self.minimap_line_length_cache.clear();
        self.minimap_section_header_cache.clear();
        self.line_render_protection_cache.clear();
        self.syntax_tree_cache.clear();
        self.lossy_decoded_buffers.clear();
        self.binary_preview_buffers.clear();
        self.image_preview_buffers.clear();
        self.manual_read_only_buffers.clear();
        self.clear_changed_on_disk_buffers();
        self.in_flight_reloads.clear();
        self.queued_file_reloads.clear();
        self.canceled_file_reloads.clear();
        self.canceled_file_reload_order.clear();
        self.active = None;
        self.pending_active_path = None;
        self.pending_open_paths.clear();
        self.pending_view_states.clear();
        self.pending_history_states.clear();
        self.pending_file_jump = None;
        self.pending_scroll_lines.clear();
        self.pending_horizontal_scroll_offsets.clear();
        self.pending_pane_scroll_lines.clear();
        self.pending_pane_horizontal_scroll_offsets.clear();
        self.editor_scroll_offsets.clear();
        self.editor_horizontal_scroll_offsets.clear();
        self.editor_scroll_targets.clear();
        self.editor_inertial_scrolls.clear();
        self.editor_middle_click_scroll = None;
        self.editor_selection_drag = None;
        self.editor_selection_clipboard = None;
        self.ime_preedit = None;
        self.pending_language_sync.clear();
        self.pending_lsp_resync.clear();
    }

    fn restore_session_pane_view_states(
        &mut self,
        pane_ids_by_index: &[Option<crate::workspace_state::PaneId>],
        pane_view_states: &[PaneBufferViewState],
        view_states: &[BufferViewState],
        restored_by_path: &HashMap<std::path::PathBuf, BufferId>,
    ) {
        let legacy_view_states = legacy_view_states_by_path(view_states);
        let mut buffers_with_pane_scroll =
            HashSet::with_capacity(pane_view_states.len().min(restored_by_path.len()));

        for view_state in pane_view_states {
            let Some(pane_id) = pane_ids_by_index
                .get(view_state.pane_index)
                .copied()
                .flatten()
            else {
                continue;
            };
            if let Some(id) = restored_buffer_id_for_path(restored_by_path, &view_state.path) {
                let Some(buffer) = self.buffer(id) else {
                    continue;
                };
                let scroll = pending_viewport_scroll(buffer, view_state);
                self.pending_pane_scroll_lines
                    .insert((pane_id, id), scroll.line);
                if scroll.horizontal_offset > 0.0 {
                    self.pending_pane_horizontal_scroll_offsets
                        .insert((pane_id, id), scroll.horizontal_offset);
                }
                buffers_with_pane_scroll.insert(id);
            } else {
                self.pending_pane_view_states
                    .insert(pane_id, view_state.clone());
            }
        }

        for id in &buffers_with_pane_scroll {
            self.pending_scroll_lines.remove(id);
            self.pending_horizontal_scroll_offsets.remove(id);
        }

        for (path, id) in restored_by_path {
            if buffers_with_pane_scroll.contains(id) {
                continue;
            }
            let Some(view_state) = legacy_view_state_for_path(&legacy_view_states, path) else {
                continue;
            };
            let Some(buffer) = self.buffer(*id) else {
                continue;
            };
            let scroll = pending_legacy_viewport_scroll(buffer, view_state);
            let mut pane_ids = self
                .panes
                .iter()
                .filter_map(|pane| (pane.active == Some(*id)).then_some(pane.id));
            let first_pane_id = pane_ids.next();
            if let Some(second_pane_id) = pane_ids.next() {
                for pane_id in first_pane_id
                    .into_iter()
                    .chain(std::iter::once(second_pane_id))
                    .chain(pane_ids)
                {
                    self.pending_pane_scroll_lines
                        .insert((pane_id, *id), scroll.line);
                    if scroll.horizontal_offset > 0.0 {
                        self.pending_pane_horizontal_scroll_offsets
                            .insert((pane_id, *id), scroll.horizontal_offset);
                    }
                }
            } else {
                self.pending_scroll_lines.insert(*id, scroll.line);
                if scroll.horizontal_offset > 0.0 {
                    self.pending_horizontal_scroll_offsets
                        .insert(*id, scroll.horizontal_offset);
                }
            }
        }
    }

    fn prune_unloadable_session_restore_paths(
        &mut self,
        restored_by_path: &HashMap<std::path::PathBuf, kuroya_core::BufferId>,
    ) {
        let mut pruned_pane_ids = HashSet::with_capacity(self.pending_pane_paths.len());
        {
            let pending_open_paths = &self.pending_open_paths;
            self.pending_pane_paths.retain(|pane_id, path| {
                let keep = path_set_contains_exact_or_lexically(pending_open_paths, path)
                    || restored_buffer_id_for_path(restored_by_path, path).is_some();
                if !keep {
                    pruned_pane_ids.insert(*pane_id);
                }
                keep
            });

            let pending_pane_paths = &self.pending_pane_paths;
            self.pending_pane_view_states.retain(|pane_id, state| {
                pending_pane_paths.contains_key(pane_id)
                    && path_set_contains_exact_or_lexically(pending_open_paths, &state.path)
            });
            self.pending_view_states
                .retain(|path, _| path_set_contains_exact_or_lexically(pending_open_paths, path));
            self.pending_history_states
                .retain(|path, _| path_set_contains_exact_or_lexically(pending_open_paths, path));
        }
        self.prune_unloadable_session_restore_panes(&pruned_pane_ids);
    }

    fn prune_unloadable_session_restore_panes(&mut self, pane_ids: &HashSet<PaneId>) {
        if pane_ids.is_empty() {
            return;
        }

        let active_pane_pruned = pane_ids.contains(&self.active_pane);
        self.panes.retain(|pane| !pane_ids.contains(&pane.id));
        self.pending_pane_scroll_lines
            .retain(|(pane_id, _), _| !pane_ids.contains(pane_id));
        self.pending_pane_horizontal_scroll_offsets
            .retain(|(pane_id, _), _| !pane_ids.contains(pane_id));

        if self.panes.is_empty() {
            self.panes.push(EditorPane {
                id: 1,
                active: None,
                weight: 1.0,
            });
            self.active_pane = 1;
            self.focused_pane = None;
            self.next_pane_id = self.next_pane_id.max(2);
            return;
        }

        if active_pane_pruned || !self.panes.iter().any(|pane| pane.id == self.active_pane) {
            self.active_pane = self.panes[0].id;
        }
        if self
            .focused_pane
            .is_some_and(|pane_id| pane_ids.contains(&pane_id))
        {
            self.focused_pane = None;
        }
        self.normalize_pane_weights();
    }
}

pub(crate) fn restore_session_workspace_root_matches(
    current_root: &Path,
    session_root: &Path,
) -> bool {
    session_root.as_os_str().is_empty() || trusted_workspace_paths_match(current_root, session_root)
}

fn restored_session_status(restored_count: usize, skipped: &[SkippedRecoveredBuffer]) -> String {
    let mut status = format!("Restored {restored_count} recovered buffers");
    if let Some(summary) = recovery_skip_summary(skipped) {
        let _ = write!(status, "; {summary}");
    }

    match sanitized_display_label_cow(
        &status,
        RESTORED_SESSION_STATUS_MAX_CHARS,
        "Restored buffers",
    ) {
        Cow::Borrowed(_) => status,
        Cow::Owned(label) => label,
    }
}

fn recovery_skip_summary(skipped: &[SkippedRecoveredBuffer]) -> Option<String> {
    if skipped.is_empty() {
        return None;
    }

    let mut groups: Vec<(Cow<'_, str>, usize)> = Vec::new();
    for entry in skipped {
        let label = recovery_skip_reason_label(&entry.reason);
        match groups.iter_mut().find(|(existing, _)| *existing == label) {
            Some(group) => group.1 += 1,
            None => {
                if groups.len() < RECOVERY_SKIP_GROUP_MAX {
                    groups.push((label, 1));
                }
            }
        }
    }
    let details = groups
        .iter()
        .map(|(label, count)| format!("{count} {label}"))
        .collect::<Vec<_>>()
        .join(", ");
    let noun = if skipped.len() == 1 {
        "buffer"
    } else {
        "buffers"
    };
    Some(format!(
        "skipped {} {noun} not snapshotted: {details}",
        skipped.len()
    ))
}

fn recovery_skip_reason_label(reason: &str) -> Cow<'_, str> {
    if reason.contains("count limit") {
        Cow::Borrowed("count limit")
    } else if reason.contains("per-buffer") {
        Cow::Borrowed("per-buffer limit")
    } else if reason.contains("total") {
        Cow::Borrowed("total limit")
    } else if reason.contains("duplicate") {
        Cow::Borrowed("duplicate path")
    } else {
        Cow::Owned(
            sanitized_display_label_cow(
                reason,
                RECOVERY_SKIP_GROUP_LABEL_MAX_CHARS,
                "unspecified reason",
            )
            .into_owned(),
        )
    }
}

fn clear_restore_path_state(session: &mut PersistedSession) {
    session.open_files.clear();
    session.active_path = None;
    session.pane_paths.clear();
    session.view_states.clear();
    session.pane_view_states.clear();
    session.history_states.clear();
    session.fold_states.clear();
    session.explorer_expanded.clear();
    session.explorer_revealed_path = None;
    session.quick_open_recent_files.clear();
    session.quick_open_query_memory.clear();
    session.workspace_symbol_query_memory.clear();
    session.navigation_back.clear();
    session.navigation_forward.clear();
    session.closed_files.clear();
    session.recovery_skipped.clear();
    for recovered in &mut session.recovery {
        recovered.path = None;
    }
}

fn legacy_view_states_by_path(view_states: &[BufferViewState]) -> HashMap<&Path, &BufferViewState> {
    let mut states = HashMap::with_capacity(view_states.len());
    for state in view_states {
        states.insert(state.path.as_path(), state);
    }
    states
}

fn legacy_view_state_for_path<'a>(
    view_states: &HashMap<&'a Path, &'a BufferViewState>,
    path: &Path,
) -> Option<&'a BufferViewState> {
    view_states.get(path).copied().or_else(|| {
        view_states.iter().find_map(|(candidate, state)| {
            paths_match_exact_or_lexically(candidate, path).then_some(*state)
        })
    })
}

fn restored_buffer_id_for_path(
    restored_by_path: &HashMap<PathBuf, BufferId>,
    path: &Path,
) -> Option<BufferId> {
    restored_by_path.get(path).copied().or_else(|| {
        restored_by_path.iter().find_map(|(candidate, id)| {
            paths_match_exact_or_lexically(candidate, path).then_some(*id)
        })
    })
}

fn pending_viewport_scroll(
    buffer: &TextBuffer,
    view_state: &PaneBufferViewState,
) -> PendingViewportScroll {
    PendingViewportScroll {
        line: pane_scroll_line_from_view_state(buffer, view_state),
        horizontal_offset: horizontal_scroll_offset_from_pane_view_state(view_state),
    }
}

fn pending_legacy_viewport_scroll(
    buffer: &TextBuffer,
    view_state: &BufferViewState,
) -> PendingViewportScroll {
    PendingViewportScroll {
        line: view_state
            .scroll_line
            .saturating_sub(1)
            .min(buffer.len_lines().saturating_sub(1)),
        horizontal_offset: horizontal_scroll_offset_from_view_state(view_state),
    }
}

fn reorder_restored_buffers_to_saved_order(buffers: &mut Vec<TextBuffer>, saved_order: &[PathBuf]) {
    if buffers.len() < 2 || saved_order.is_empty() {
        return;
    }
    let mut keyed: Vec<(usize, TextBuffer)> = std::mem::take(buffers)
        .into_iter()
        .map(|buffer| {
            let saved_position = buffer
                .path()
                .and_then(|path| {
                    saved_order
                        .iter()
                        .position(|saved| paths_match_exact_or_lexically(saved, path))
                })
                .unwrap_or(usize::MAX);
            (saved_position, buffer)
        })
        .collect();
    keyed.sort_by_key(|(saved_position, _)| *saved_position);
    buffers.extend(keyed.into_iter().map(|(_, buffer)| buffer));
}

fn restorable_session_open_files(
    open_files: Vec<PathBuf>,
    indexed_files: &[PathBuf],
    restored_by_path: &HashMap<PathBuf, BufferId>,
    mut path_exists: impl FnMut(&Path) -> bool,
) -> Vec<PathBuf> {
    let restored_path_keys = restore_path_keys(restored_by_path.keys());
    let mut considered_paths = HashSet::with_capacity(open_files.len());
    let mut considered_path_keys = HashSet::with_capacity(open_files.len());
    let mut restorable = Vec::with_capacity(open_files.len());
    for path in open_files {
        if restored_by_path.contains_key(&path)
            || restored_path_key_exists(&restored_path_keys, &path)
        {
            continue;
        }
        let path_key = restore_path_key(&path);
        if path_key
            .as_ref()
            .is_some_and(|key| considered_path_keys.contains(key))
        {
            continue;
        }
        if !considered_paths.insert(path.clone()) {
            continue;
        }

        if let Some(path_key) = path_key {
            considered_path_keys.insert(path_key);
        }
        if file_path_known_openable(indexed_files, &path, |path| path_exists(path)) {
            restorable.push(path);
        }
    }
    restorable
}

fn restore_path_keys<'a>(paths: impl Iterator<Item = &'a PathBuf>) -> HashSet<RestorePathKey> {
    let (path_count, _) = paths.size_hint();
    let mut keys = HashSet::with_capacity(path_count);
    keys.extend(paths.filter_map(|path| restore_path_key(path.as_path())));
    keys
}

fn restored_path_key_exists(restored_path_keys: &HashSet<RestorePathKey>, path: &Path) -> bool {
    restore_path_key(path).is_some_and(|key| restored_path_keys.contains(&key))
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct RestorePathKey {
    prefix: Option<String>,
    rooted: bool,
    components: Vec<String>,
}

fn restore_path_key(path: &Path) -> Option<RestorePathKey> {
    if path.as_os_str().is_empty() {
        return None;
    }

    let components = path.components();
    let (component_count, _) = components.size_hint();
    let mut key = RestorePathKey {
        prefix: None,
        rooted: false,
        components: Vec::with_capacity(component_count),
    };
    for component in components {
        match component {
            Component::Prefix(prefix) => {
                key.prefix = Some(normalize_restore_path_component(prefix.as_os_str()));
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
                    .push(normalize_restore_path_component(component));
            }
        }
    }

    Some(key)
}

fn normalize_restore_path_component(component: &OsStr) -> String {
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

pub(crate) fn terminal_visible_after_startup(
    hide_on_startup: TerminalHideOnStartup,
    persisted_visible: bool,
    restored_terminal_sessions: bool,
) -> bool {
    match hide_on_startup {
        TerminalHideOnStartup::Never => persisted_visible,
        TerminalHideOnStartup::WhenEmpty => persisted_visible && restored_terminal_sessions,
        TerminalHideOnStartup::Always => false,
    }
}

#[cfg(test)]
mod tests;
