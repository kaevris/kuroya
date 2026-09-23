use crate::{
    KuroyaApp,
    app_startup_context::terminal_root_for_workspace,
    layout::{
        DIAGNOSTICS_PANEL_DEFAULT_WIDTH, EXPLORER_DEFAULT_WIDTH, SOURCE_CONTROL_DEFAULT_WIDTH,
        SYMBOLS_PANEL_DEFAULT_WIDTH, TERMINAL_DEFAULT_HEIGHT,
    },
    panel_layout::PanelPlacement,
    terminal::TerminalPane,
};
use kuroya_core::{DiagnosticSet, ProjectIndex};

mod dialogs;
mod panes;

impl KuroyaApp {
    pub(crate) fn reset_workspace_lsp_clients(&mut self) {
        self.lsp_clients.clear();
        self.lsp_unavailable.clear();
        self.lsp_restart_attempts.clear();
        self.pending_lsp_restarts.clear();
        self.pending_lsp_symbol_refreshes.clear();
        self.lsp_progress_titles.clear();
        self.document_highlights_path = None;
        self.document_highlights.clear();
        self.folding_ranges.clear();
        self.inlay_hints.clear();
        self.code_lenses.clear();
        self.semantic_tokens.clear();
        self.folded_ranges.clear();
        self.pending_fold_line = None;
    }

    pub(crate) fn reset_open_workspace_state(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        if self.has_keybinding_capture_in_progress() && !self.cancel_keybinding_capture() {
            self.keybinding_capture_command = None;
            self.keybinding_escape_cancel = None;
            warnings.push(keybinding_capture_force_cleared_warning());
        }
        self.pending_workspace_switch = None;
        self.pending_exit = None;
        self.exit_confirmed = false;
        self.shutdown_prepared = false;
        self.workspace_event_generation = self.workspace_event_generation.wrapping_add(1);
        self.reset_workspace_document_state();
        self.reset_workspace_dialog_and_search_state();
        self.reset_workspace_save_and_pane_state();
        self.reset_workspace_lsp_only_ui_state();
        self.reset_workspace_trusted_feature_state();
        self.reset_workspace_layout_state();
        warnings
    }

    fn reset_workspace_document_state(&mut self) {
        self.index = ProjectIndex::default();
        self.project_index_generation = 0;
        self.project_search_index_generation = 0;
        self.project_search_metadata_cache.clear();
        self.invalidate_workspace_index_requests();
        self.invalidate_git_scan();
        self.pending_workspace_refresh = None;
        self.explorer_revealed_path = None;
        self.clear_explorer_directory_cache();
        self.explorer_compare_path = None;
        self.editor_inertial_scrolls.clear();
        self.editor_selection_drag = None;
        self.editor_selection_clipboard = None;
        self.buffers.clear();

        self.vim_reset_session_state();
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
        self.clear_changed_on_disk_buffers();
        self.in_flight_reloads.clear();
        self.queued_file_reloads.clear();
        self.canceled_file_reloads.clear();
        self.canceled_file_reload_order.clear();
        self.lossy_decoded_buffers.clear();
        self.binary_preview_buffers.clear();
        self.image_preview_buffers.clear();
        self.manual_read_only_buffers.clear();
    }

    pub(crate) fn reset_workspace_lsp_ui_state(&mut self) {
        self.reset_workspace_lsp_only_ui_state();
        self.reset_workspace_trusted_feature_state();
    }

    fn reset_workspace_lsp_only_ui_state(&mut self) {
        self.diagnostics = DiagnosticSet::default();
        self.static_diagnostics_active_request_ids.clear();
        self.static_diagnostics_in_flight_request_ids.clear();
        self.static_diagnostics_reload_queued.clear();
        self.diagnostics_panel_selected = 0;
        self.pending_lsp_diagnostics.clear();
        self.pending_lsp_symbol_refreshes.clear();
        self.symbols_panel = false;
        self.document_symbols.clear();
        self.document_symbols_path = None;
        self.document_symbols_selected = 0;
        self.inlay_hints.clear();
        self.code_lenses.clear();
        self.semantic_tokens.clear();
        self.workspace_symbols_open = false;
        self.workspace_symbol_query.clear();
        self.workspace_symbol_submitted_query.clear();
        self.workspace_symbol_submitted_path = None;
        self.workspace_symbols.clear();
        self.workspace_symbols_selected = 0;
        self.local_history_browser_open = false;
        self.local_history_browser_path = None;
        self.local_history_browser_snapshots.clear();
        self.local_history_browser_selected = 0;
        self.local_history_browser_loading = false;
        self.completion_open = false;
        self.completion_items.clear();
        self.completion_buffer_id = None;
        self.completion_path = None;
        self.completion_version = None;
        self.completion_line = 0;
        self.completion_column = 0;
        self.completion_prefix.clear();
        self.completion_selected = 0;
        self.completion_preview_resolve_in_flight.clear();
        self.completion_preview_resolve_recent_attempts.clear();
        self.snippet_session = None;
        if !self.settings.suggest_share_suggest_selections {
            self.completion_recent_labels.clear();
            self.completion_recent_prefix_labels.clear();
        }
        self.pending_completion_requests.clear();
        self.pending_signature_help_requests.clear();
        self.pending_format_on_type_requests.clear();
        self.signature_help = None;
        self.code_actions_open = false;
        self.code_actions.clear();
        self.code_actions_buffer_id = None;
        self.code_actions_path = None;
        self.code_actions_version = None;
        self.code_actions_line = 0;
        self.code_actions_column = 0;
        self.code_actions_selected = 0;
        self.references_open = false;
        self.references.clear();
        self.references_path = None;
        self.references_line = 0;
        self.references_column = 0;
        self.references_selected = 0;
        self.call_hierarchy_open = false;
        self.call_hierarchy_root = None;
        self.call_hierarchy_incoming.clear();
        self.call_hierarchy_outgoing.clear();
        self.call_hierarchy_selected = 0;
        self.call_hierarchy_path = None;
        self.call_hierarchy_line = 0;
        self.call_hierarchy_column = 0;
        self.type_hierarchy_open = false;
        self.type_hierarchy_root = None;
        self.type_hierarchy_supertypes.clear();
        self.type_hierarchy_subtypes.clear();
        self.type_hierarchy_selected = 0;
        self.type_hierarchy_path = None;
        self.type_hierarchy_line = 0;
        self.type_hierarchy_column = 0;
        self.pending_lsp_hover = None;
        self.lsp_hover_request = None;
        self.lsp_hover = None;
        self.lsp_hover_cache.clear();
        self.lsp_rename_open = false;
        self.lsp_rename_input.clear();
        self.lsp_rename_preview_open = false;
        self.lsp_rename_preview_new_name.clear();
        self.lsp_rename_preview_edits.clear();
        self.lsp_rename_preview_rows.clear();
        self.lsp_rename_preview_versions.clear();
    }

    fn reset_workspace_trusted_feature_state(&mut self) {
        self.workspace_tasks_open = false;
        self.workspace_tasks.clear();
        self.workspace_tasks_selected = 0;
        self.invalidate_workspace_task_load_requests();
        self.workspace_tasks_loading = false;
        self.workspace_tasks_loaded = false;
        self.pending_workspace_task_kind = None;
        self.running_workspace_tasks.clear();
        self.invalidate_workspace_plugin_discovery();
        self.pending_workspace_plugin_reload = None;
        self.clear_workspace_plugins();
    }

    fn reset_workspace_layout_state(&mut self) {
        self.explorer_width = EXPLORER_DEFAULT_WIDTH;
        self.source_control = false;
        self.source_control_placement = PanelPlacement::default();
        self.source_control_width = SOURCE_CONTROL_DEFAULT_WIDTH;
        self.source_control_query.clear();
        self.source_control_commit_message.clear();
        self.source_control_commit_history.clear();
        self.source_control_commit_history_index = None;
        self.source_control_commit_next_request_id = 0;
        self.source_control_commit_active_request_id = 0;
        self.source_control_commit_in_flight_request_ids.clear();
        self.source_control_selected = 0;
        self.source_control_unstaged_collapsed = false;
        self.source_control_untracked_collapsed = false;
        self.source_control_staged_collapsed = false;
        self.source_control_branch_picker_open = false;
        self.source_control_branch_query.clear();
        self.source_control_branch_rename_from = None;
        self.source_control_branches.clear();
        self.source_control_branch_selected = 0;
        self.source_control_branch_operation_next_request_id = 0;
        self.source_control_branch_operation_active_request_id = 0;
        self.source_control_branch_operation_in_flight_request_ids
            .clear();
        self.source_control_history_open = false;
        self.source_control_history_query.clear();
        self.source_control_history.clear();
        self.source_control_history_selected = 0;
        self.source_control_history_loading = false;
        self.source_control_history_requested_limit = 0;
        self.source_control_history_has_more = false;
        self.pending_restored_git_history_load = false;
        self.source_control_stashes_open = false;
        self.source_control_stash_message.clear();
        self.source_control_stash_query.clear();
        self.source_control_stashes.clear();
        self.source_control_stash_selected = 0;
        self.pending_restored_git_stashes_load = false;
        self.source_control_hunks_open = false;
        self.source_control_hunk_path = None;
        self.source_control_hunk_stage = kuroya_core::GitChangeStage::Unstaged;
        self.source_control_hunks.clear();
        self.source_control_hunk_selected = 0;
        self.invalidate_source_control_load_requests();
        self.source_control_blame_load_opens_view = false;
        self.source_control_blame_pending_path = None;
        self.source_control_blame_ignore_whitespace = self.settings.git_blame_ignore_whitespace;
        self.source_control_blame_cache.clear();
        self.symbols_panel_placement = PanelPlacement::default();
        self.symbols_panel_width = SYMBOLS_PANEL_DEFAULT_WIDTH;
        self.diagnostics_panel = false;
        self.diagnostics_panel_placement = PanelPlacement::default();
        self.diagnostics_panel_width = DIAGNOSTICS_PANEL_DEFAULT_WIDTH;
        let terminal_repaint_context = self.terminal.repaint_context();
        self.terminal.close_all_sessions_for_shutdown();
        self.terminal = TerminalPane::with_settings(
            terminal_root_for_workspace(&self.workspace.root),
            self.settings.terminal_scrollback_rows,
            self.settings.terminal_shell_path.clone(),
            self.settings.terminal_shell_args.clone(),
            self.settings.terminal_cwd.clone(),
            self.settings.terminal_split_cwd,
            self.settings.terminal_min_rows,
            self.settings.terminal_min_columns,
            self.settings.terminal_font_size,
            self.settings.terminal_line_height,
            self.settings.terminal_letter_spacing,
            self.settings.terminal_cursor_style,
            self.settings.terminal_cursor_width,
            self.settings.terminal_cursor_blinking,
            self.settings.terminal_cursor_style_inactive,
            self.settings.terminal_draw_bold_text_in_bright_colors,
            self.settings.terminal_minimum_contrast_ratio,
            self.settings.terminal_enable_bell,
            self.settings.terminal_bell_duration_ms,
            self.settings.terminal_show_exit_alert,
            self.settings.terminal_hide_on_last_closed,
            self.settings.terminal_confirm_on_kill,
            self.settings.terminal_tabs_enabled,
            self.settings.terminal_tabs_default_icon.clone(),
            self.settings.terminal_tabs_default_color.clone(),
            self.settings.terminal_tabs_allow_agent_cli_title,
            self.settings.terminal_tabs_title.clone(),
            self.settings.terminal_tabs_hide_condition,
            self.settings.terminal_tabs_show_active_terminal,
            self.settings.terminal_tabs_show_actions,
            self.settings.terminal_tabs_focus_mode,
            self.settings.terminal_tabs_location,
            self.settings.terminal_right_click_behavior,
            self.settings.terminal_middle_click_behavior,
            self.settings.terminal_alt_click_moves_cursor,
            self.settings.terminal_copy_on_selection,
            self.settings.terminal_ignore_bracketed_paste_mode,
            self.settings.terminal_enable_multi_line_paste_warning,
            self.settings.terminal_word_separators.clone(),
            self.settings.terminal_mouse_wheel_scroll_sensitivity,
            self.settings.terminal_fast_scroll_sensitivity,
            self.settings.terminal_mouse_wheel_zoom,
        );
        if let Some(ctx) = terminal_repaint_context {
            self.terminal.set_repaint_context(ctx);
        }
        self.terminal_height = TERMINAL_DEFAULT_HEIGHT;
        self.terminal_open_height_pending = false;
    }
}

fn keybinding_capture_force_cleared_warning() -> String {
    "Shortcut capture rollback failed during workspace switch; capture was discarded".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app_startup_context::AppStartupContext,
        app_state::{PendingFileReload, QueuedFileReload},
        commands::keybinding_chord_for_command,
        keybinding_input::CapturedKeybinding,
        keybindings_panel_actions::PendingKeybindingsPanelActions,
        lsp_workspace_symbol_ranking::WorkspaceSymbolQueryMemoryEntry,
        terminal::TerminalPane,
        transient_state::{PendingExit, PendingWorkspaceSwitch},
        workspace_state::settings_path,
        workspace_tasks_runtime::RunningWorkspaceTask,
    };
    use kuroya_core::{
        EditorMatchBrackets, EditorSettings, TextBuffer, Workspace, WorkspaceTaskKind,
        keymap::KeyBinding,
    };
    use std::{
        path::PathBuf,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };
    use tokio::runtime::Runtime;

    #[test]
    fn reset_open_workspace_state_clears_workspace_symbol_query_memory() {
        let root = temp_root("workspace-symbol-memory-reset");
        let mut app = app_for_test(root.clone());
        app.workspace_symbol_query_memory
            .push_back(WorkspaceSymbolQueryMemoryEntry {
                query: "main".to_owned(),
                path: root.join("src/main.rs"),
                name: "main_symbol".to_owned(),
                kind: 12,
                line: 1,
                column: 1,
                uses: 2,
            });

        app.reset_open_workspace_state();

        assert!(app.workspace_symbol_query_memory.is_empty());
    }

    #[test]
    fn reset_open_workspace_state_clears_vim_marks_registers_and_unnamed_register() {
        use crate::editor_vim_key_events::{
            EditorVimMode, EditorVimPendingKey, vim_marks_are_empty_for_test,
        };
        use eframe::egui::{Key, Modifiers};

        let root = temp_root("workspace-reset-clears-vim-state");
        let mut app = app_for_test(root.clone());
        app.buffers
            .push(TextBuffer::from_text(7, None, "alpha\nbeta\n".to_owned()));

        let mut mode = EditorVimMode::Normal;
        let mut pending: Option<EditorVimPendingKey> = None;
        let mut last_char_find = None;
        let mut unnamed_register = None;
        let mut last_change = None;
        let settings = EditorSettings::default();
        for (key, modifiers) in [
            (Key::M, Modifiers::NONE),
            (Key::A, Modifiers::NONE),
            (Key::Y, Modifiers::NONE),
            (Key::I, Modifiers::NONE),
            (Key::W, Modifiers::NONE),
        ] {
            let buffer = app.buffers.get_mut(0).expect("test buffer");
            crate::editor_vim_key_events::handle_vim_editor_key_event_with_settings_and_indent(
                buffer,
                key,
                modifiers,
                &mut mode,
                &mut pending,
                &mut last_char_find,
                &mut unnamed_register,
                &mut last_change,
                &settings.vim,
                "    ",
            );
        }
        assert!(!vim_marks_are_empty_for_test());
        assert!(unnamed_register.is_some());

        app.reset_open_workspace_state();

        assert!(vim_marks_are_empty_for_test());
        assert_eq!(app.editor_vim_unnamed_register, None);
    }

    #[test]
    fn reset_open_workspace_state_cancels_pending_escape_keybinding_capture_before_resetting() {
        let root = temp_root("workspace-reset-cancels-keybinding-capture");
        let mut app = app_for_test(root.clone());
        app.settings.keymap.bindings = vec![
            KeyBinding {
                chord: "Escape".to_owned(),
                command: kuroya_core::Command::ToggleQuickOpen,
            },
            KeyBinding {
                chord: "Ctrl+Z".to_owned(),
                command: kuroya_core::Command::Undo,
            },
        ];
        app.keybindings_open = true;
        app.keybinding_capture_command = Some(kuroya_core::Command::Undo);

        app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
            captured: Some(CapturedKeybinding::Escape),
            ..PendingKeybindingsPanelActions::default()
        });
        assert_eq!(
            app.keybinding_capture_command,
            Some(kuroya_core::Command::Undo)
        );
        assert!(app.keybinding_escape_cancel.is_some());
        assert_eq!(
            keybinding_chord_for_command(
                &app.settings.keymap.bindings,
                &kuroya_core::Command::Undo
            ),
            Some("Ctrl+Z".to_owned())
        );
        assert_eq!(
            keybinding_chord_for_command(
                &app.settings.keymap.bindings,
                &kuroya_core::Command::ToggleQuickOpen
            ),
            Some("Escape".to_owned())
        );

        app.reset_open_workspace_state();

        assert!(!app.keybindings_open);
        assert_eq!(app.keybinding_capture_command, None);
        assert!(app.keybinding_escape_cancel.is_none());
        assert_eq!(
            keybinding_chord_for_command(
                &app.settings.keymap.bindings,
                &kuroya_core::Command::Undo
            ),
            Some("Ctrl+Z".to_owned())
        );
        assert_eq!(
            keybinding_chord_for_command(
                &app.settings.keymap.bindings,
                &kuroya_core::Command::ToggleQuickOpen
            ),
            Some("Escape".to_owned())
        );
        assert!(!settings_path(&root).exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reset_open_workspace_state_cancels_escape_armed_capture_without_force_clear() {
        let root = temp_root("workspace-reset-capture-escape-armed");
        std::fs::create_dir_all(&root).unwrap();
        let mut app = app_for_test(root.clone());
        app.settings.keymap.bindings = vec![KeyBinding {
            chord: "Ctrl+Z".to_owned(),
            command: kuroya_core::Command::Undo,
        }];
        app.keybindings_open = true;
        app.keybinding_capture_command = Some(kuroya_core::Command::Undo);

        app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
            captured: Some(CapturedKeybinding::Escape),
            ..PendingKeybindingsPanelActions::default()
        });
        let pending_escape_cancel = app
            .keybinding_escape_cancel
            .as_ref()
            .cloned()
            .expect("escape capture should be armed");
        assert!(!pending_escape_cancel.saved_escape_binding);
        assert_eq!(
            keybinding_chord_for_command(
                &app.settings.keymap.bindings,
                &kuroya_core::Command::Undo
            ),
            Some("Ctrl+Z".to_owned())
        );

        assert!(!settings_path(&root).exists());
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(root.join("src/main.rs")),
            "fn main() {}\n".to_owned(),
        ));
        let generation_before = app.workspace_event_generation;

        let warnings = app.reset_open_workspace_state();

        assert!(warnings.is_empty());
        assert_eq!(
            app.workspace_event_generation,
            generation_before.wrapping_add(1)
        );
        assert!(app.buffers.is_empty());
        assert!(!app.keybindings_open);
        assert_eq!(app.keybinding_capture_command, None);
        assert!(app.keybinding_escape_cancel.is_none());
        drop(app);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reset_open_workspace_state_clears_save_as_dialog() {
        let root = temp_root("save-as-reset");
        let mut app = app_for_test(root.clone());
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.save_as_path = root.join("src/main.rs").display().to_string();

        app.reset_open_workspace_state();

        assert!(!app.save_as_open);
        assert_eq!(app.save_as_buffer, None);
        assert!(app.save_as_path.is_empty());
    }

    #[test]
    fn reset_open_workspace_state_invalidates_open_workspace_picker_without_zero_request_id() {
        let root = temp_root("open-workspace-picker-request-reset");
        let mut app = app_for_test(root);
        app.open_workspace_picker_in_flight = true;
        app.open_workspace_picker_request_id = u64::MAX;

        app.reset_open_workspace_state();

        assert!(!app.open_workspace_picker_in_flight);
        assert_eq!(app.open_workspace_picker_request_id, 1);
    }

    #[test]
    fn reset_open_workspace_state_invalidates_trusted_feature_requests_once() {
        let root = temp_root("trusted-feature-request-reset");
        let mut app = app_for_test(root);
        app.workspace_tasks_next_request_id = 10;
        app.workspace_tasks_active_request_id = 10;
        app.workspace_tasks_in_flight_request_id = Some(10);
        app.workspace_tasks_reload_queued = true;
        app.workspace_tasks_open = true;
        app.workspace_tasks_loading = true;
        app.workspace_tasks_loaded = true;
        app.pending_workspace_task_kind = Some(WorkspaceTaskKind::Build);
        app.running_workspace_tasks.push(RunningWorkspaceTask {
            task_index: 0,
            fingerprint: 99,
            session_id: 7,
        });
        app.workspace_plugins_next_request_id = 20;
        app.workspace_plugins_active_request_id = 20;
        app.workspace_plugins_in_flight_request_id = Some(20);
        app.workspace_plugins_reload_queued = true;
        app.schedule_workspace_plugin_reload();

        app.reset_open_workspace_state();

        assert_eq!(app.workspace_tasks_next_request_id, 11);
        assert_eq!(app.workspace_tasks_active_request_id, 11);
        assert_eq!(app.workspace_tasks_in_flight_request_id, None);
        assert!(!app.workspace_tasks_reload_queued);
        assert!(!app.workspace_tasks_open);
        assert!(!app.workspace_tasks_loading);
        assert!(!app.workspace_tasks_loaded);
        assert_eq!(app.pending_workspace_task_kind, None);
        assert!(app.running_workspace_tasks.is_empty());
        assert_eq!(app.workspace_plugins_next_request_id, 21);
        assert_eq!(app.workspace_plugins_active_request_id, 21);
        assert_eq!(app.workspace_plugins_in_flight_request_id, None);
        assert!(!app.workspace_plugins_reload_queued);
        assert_eq!(app.pending_workspace_plugin_reload, None);
    }

    #[test]
    fn reset_open_workspace_state_invalidates_source_control_restore_requests_once() {
        let root = temp_root("source-control-request-reset");
        let mut app = app_for_test(root);
        app.source_control_history_next_request_id = 10;
        app.source_control_history_active_request_id = 10;
        app.source_control_history_in_flight_request_id = Some(10);
        app.source_control_history_reload_queued = true;
        app.pending_restored_git_history_load = true;
        app.source_control_stashes_next_request_id = 20;
        app.source_control_stashes_active_request_id = 20;
        app.source_control_stashes_in_flight_request_id = Some(20);
        app.source_control_stashes_reload_queued = true;
        app.pending_restored_git_stashes_load = true;

        app.reset_open_workspace_state();

        assert_eq!(app.source_control_history_next_request_id, 11);
        assert_eq!(app.source_control_history_active_request_id, 11);
        assert_eq!(app.source_control_history_in_flight_request_id, None);
        assert!(!app.source_control_history_reload_queued);
        assert!(!app.pending_restored_git_history_load);
        assert_eq!(app.source_control_stashes_next_request_id, 21);
        assert_eq!(app.source_control_stashes_active_request_id, 21);
        assert_eq!(app.source_control_stashes_in_flight_request_id, None);
        assert!(!app.source_control_stashes_reload_queued);
        assert!(!app.pending_restored_git_stashes_load);
    }

    #[test]
    fn reset_open_workspace_state_clears_stale_workspace_switch_action() {
        let root = temp_root("stale-switch-action-reset-root");
        let target = temp_root("stale-switch-action-reset-target");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        let mut app = app_for_test(root.clone());
        app.pending_workspace_switch = Some(PendingWorkspaceSwitch::Saving {
            target: target.clone(),
            ids: vec![7],
        });

        app.reset_open_workspace_state();
        app.advance_pending_workspace_switch_after_save();

        assert_eq!(app.workspace.root, root);
        assert!(app.pending_workspace_switch.is_none());

        drop(app);
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn reset_open_workspace_state_clears_stale_exit_action() {
        let root = temp_root("stale-exit-action-reset");
        let mut app = app_for_test(root);
        app.exit_confirmed = true;
        app.pending_exit = Some(PendingExit::Saving { ids: vec![7] });

        app.reset_open_workspace_state();
        app.advance_pending_exit_after_save();

        assert!(!app.exit_confirmed);
        assert!(app.pending_exit.is_none());
    }

    #[test]
    fn reset_open_workspace_state_clears_deferred_reload_tracking() {
        let root = temp_root("deferred-reload-reset");
        let path = root.join("src/main.rs");
        let queued_path = root.join("src/queued.rs");
        let mut app = app_for_test(root);
        assert!(
            app.in_flight_reloads
                .insert(
                    7,
                    PendingFileReload {
                        request_id: 1,
                        path: path.clone(),
                        version: 2,
                        force_dirty: false,
                    },
                )
                .is_none()
        );
        assert!(
            app.queued_file_reloads
                .insert(
                    7,
                    QueuedFileReload {
                        path: queued_path,
                        force_dirty: true,
                    },
                )
                .is_none()
        );
        let canceled = PendingFileReload {
            request_id: 2,
            path,
            version: 3,
            force_dirty: true,
        };
        assert!(app.canceled_file_reloads.insert((7, canceled.clone())));
        app.canceled_file_reload_order.push_back((7, canceled));
        app.dirty_reload_buffer = Some(7);
        assert!(app.mark_buffer_changed_on_disk(7));

        app.reset_open_workspace_state();

        assert!(app.in_flight_reloads.is_empty());
        assert!(app.queued_file_reloads.is_empty());
        assert!(app.canceled_file_reloads.is_empty());
        assert!(app.canceled_file_reload_order.is_empty());
        assert_eq!(app.dirty_reload_buffer, None);
        assert_eq!(app.changed_on_disk_buffer_count(), 0);
    }

    #[test]
    fn reset_open_workspace_state_clears_editor_bracket_overlay_cache() {
        let root = temp_root("bracket-overlay-reset");
        let mut app = app_for_test(root.clone());
        let buffer = TextBuffer::from_text(7, Some(root.join("src/main.rs")), "{}\n".to_owned());

        app.editor_bracket_overlay_cache
            .bracket_colors_for_lines(&buffer, 0, 1, false);
        app.editor_bracket_overlay_cache
            .bracket_pair_guides(&buffer);
        app.editor_bracket_overlay_cache
            .bracket_matches(&buffer, EditorMatchBrackets::Near);
        assert!(app.editor_bracket_overlay_cache.contains_buffer_for_test(7));

        app.reset_open_workspace_state();

        assert!(!app.editor_bracket_overlay_cache.contains_buffer_for_test(7));
    }

    #[test]
    fn reset_open_workspace_state_clears_minimap_caches() {
        let root = temp_root("minimap-cache-reset");
        let mut app = app_for_test(root.clone());
        let buffer = TextBuffer::from_text(
            7,
            Some(root.join("src/main.rs")),
            "#region Setup\nfn main() {}\n".to_owned(),
        );

        app.minimap_line_length_cache
            .sampled_lengths_for(&buffer, 2, 80, true);
        app.minimap_section_header_cache
            .headers_for(&buffer, true, false, "");
        assert!(app.minimap_line_length_cache.contains_buffer_for_test(7));
        assert!(app.minimap_section_header_cache.contains_buffer_for_test(7));

        app.reset_open_workspace_state();

        assert!(!app.minimap_line_length_cache.contains_buffer_for_test(7));
        assert!(!app.minimap_section_header_cache.contains_buffer_for_test(7));
    }

    fn app_for_test(root: PathBuf) -> KuroyaApp {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let settings = EditorSettings::default();
        KuroyaApp::from_startup_context(AppStartupContext {
            runtime: Runtime::new().expect("test runtime"),
            tx,
            rx,
            workspace: Workspace::new(root.clone()),
            settings: settings.clone(),
            settings_panel_draft: settings,
            settings_editor_font_path: String::new(),
            settings_ui_font_path: String::new(),
            theme_picker_selected: 0,
            saved_session: None,
            terminal: TerminalPane::new(root.clone(), 100, 12.0, 1.2),
            watcher: None,
            recent_projects: Vec::new(),
            trusted_workspaces: vec![root],
            now: Instant::now(),
            startup_timings: Vec::new(),
        })
    }

    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "kuroya-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
