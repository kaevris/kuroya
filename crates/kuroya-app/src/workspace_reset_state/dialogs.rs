use crate::{KuroyaApp, theme::selected_theme_index_with_plugins};
use kuroya_core::{SearchOptions, SearchResult};

impl KuroyaApp {
    pub(super) fn reset_workspace_dialog_and_search_state(&mut self) {
        self.explorer_expanded.clear();
        self.explorer_file_action = None;
        self.explorer_file_input.clear();
        self.explorer_delete_target = None;
        self.quick_open = false;
        self.quick_open_query.clear();
        self.quick_open_selected = 0;
        self.quick_open_recent_files.clear();
        self.quick_open_query_memory.clear();
        self.quick_open_results_cache = None;
        self.workspace_symbol_query_memory.clear();
        self.buffer_find_open = false;
        self.buffer_find_query.clear();
        self.buffer_find_replacement.clear();
        self.buffer_find_query_history.clear();
        self.buffer_find_query_history_cursor = None;
        self.buffer_find_query_history_draft = None;
        self.buffer_find_replacement_history.clear();
        self.buffer_find_replacement_history_cursor = None;
        self.buffer_find_replacement_history_draft = None;
        self.buffer_find_match = 0;
        self.buffer_find_case_sensitive = false;
        self.buffer_find_whole_word = false;
        self.buffer_find_regex = false;
        self.buffer_find_preserve_case = false;
        self.buffer_find_scope = None;
        self.buffer_find_cache.clear();
        self.goto_line_open = false;
        self.goto_line_input.clear();
        self.command_palette = false;
        self.command_query.clear();
        self.command_selected = 0;
        self.command_recent.clear();
        self.command_query_memory.clear();
        self.command_palette_results_cache = None;
        self.open_workspace_picker_in_flight = false;
        self.open_workspace_picker_request_id = next_open_workspace_picker_invalidation_request_id(
            self.open_workspace_picker_request_id,
        );
        self.open_workspace_open = false;
        self.open_workspace_path.clear();
        self.settings_panel_open = false;
        self.sync_settings_panel_inputs();
        self.theme_picker_open = false;
        self.theme_picker_selected =
            selected_theme_index_with_plugins(&self.settings.theme, &self.plugin_themes);
        self.keybindings_open = false;
        self.keybindings_query.clear();
        self.keybindings_selected = 0;
        self.keybinding_capture_command = None;
        self.pending_editor_file_drop = None;
        self.pending_source_control_discard = None;
        self.pending_source_control_smart_commit = None;
        self.pending_source_control_empty_commit = None;
        self.pending_source_control_protected_branch_commit = None;
        self.pending_source_control_commit_save = None;
        self.pending_source_control_stash_save = None;
        self.vim_reset_session_state();
        self.project_search = false;
        self.project_search_focus_query = false;
        self.project_search_query.clear();
        self.project_search_result = SearchResult::default();
        self.project_search_result_query.clear();
        self.project_search_result_index_generation = 0;
        self.project_search_result_case_sensitive = false;
        self.project_search_result_whole_word = false;
        self.project_search_result_regex = false;
        self.project_search_result_include_globs.clear();
        self.project_search_result_exclude_globs.clear();
        self.project_search_result_max_file_bytes = SearchOptions::default().max_file_bytes;
        self.project_search_result_max_results = SearchOptions::default().max_results;
        self.project_search_include.clear();
        self.project_search_exclude.clear();
        self.project_search_recent.clear();
        self.project_search_selected = 0;
        self.invalidate_project_search_requests();
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
        self.source_control_stashes_open = false;
        self.source_control_stash_message.clear();
        self.source_control_stashes.clear();
        self.source_control_stash_selected = 0;
        self.source_control_hunks_open = false;
        self.source_control_hunk_path = None;
        self.source_control_hunk_stage = kuroya_core::GitChangeStage::Unstaged;
        self.source_control_hunks.clear();
        self.source_control_hunk_selected = 0;
        self.source_control_blame_load_opens_view = false;
        self.source_control_blame_pending_path = None;
        self.source_control_blame_ignore_whitespace = self.settings.git_blame_ignore_whitespace;
        self.source_control_blame_cache.clear();
    }
}

fn next_open_workspace_picker_invalidation_request_id(current: u64) -> u64 {
    match current.wrapping_add(1) {
        0 => 1,
        request_id => request_id,
    }
}
