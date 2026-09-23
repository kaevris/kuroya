use crate::KuroyaApp;
use eframe::egui::{Context, PointerButton, Pos2, Rect};

#[derive(Debug, Clone, Copy)]
pub(crate) struct PopupDismissalGuard {
    child_popup_was_open: bool,
}

impl PopupDismissalGuard {
    pub(crate) fn capture(ctx: &Context) -> Self {
        Self {
            child_popup_was_open: eframe::egui::Popup::is_any_open(ctx),
        }
    }

    pub(crate) fn clicked_outside(self, ctx: &Context, window_rect: Rect) -> bool {
        let (primary_clicked, pointer_position) = ctx.input(|input| {
            (
                input.pointer.button_clicked(PointerButton::Primary),
                input.pointer.interact_pos(),
            )
        });
        primary_click_is_outside_window(
            self.child_popup_was_open,
            primary_clicked,
            pointer_position,
            window_rect,
        )
    }
}

fn primary_click_is_outside_window(
    child_popup_was_open: bool,
    primary_clicked: bool,
    pointer_position: Option<Pos2>,
    window_rect: Rect,
) -> bool {
    !child_popup_was_open
        && primary_clicked
        && pointer_position.is_some_and(|position| !window_rect.contains(position))
}

impl KuroyaApp {
    pub(crate) fn render_active_overlays(&mut self, ctx: &Context) {
        self.render_status_toasts(ctx);
        if self.project_search {
            self.render_project_search_overlay(ctx);
        }
        if self.quick_open {
            self.render_quick_open(ctx);
        }
        if self.buffer_find_open {
            self.render_buffer_find(ctx);
        }
        if self.goto_line_open {
            self.render_goto_line(ctx);
        }
        if self.workspace_symbols_open {
            self.render_workspace_symbols(ctx);
        }
        if self.local_history_browser_open {
            self.render_local_history_browser(ctx);
        }
        if self.workspace_tasks_open {
            self.render_workspace_tasks_panel(ctx);
        }
        if self.lsp_hover.is_some() {
            self.render_lsp_hover(ctx);
        }
        if self.signature_help.is_some() {
            self.render_signature_help(ctx);
        }
        if self.lsp_rename_open {
            self.render_lsp_rename(ctx);
        }
        if self.lsp_rename_preview_open {
            self.render_lsp_rename_preview(ctx);
        }
        if self.completion_open {
            self.render_completion_popup(ctx);
        }
        if self.references_open {
            self.render_references_popup(ctx);
        }
        if self.call_hierarchy_open {
            self.render_call_hierarchy_popup(ctx);
        }
        if self.type_hierarchy_open {
            self.render_type_hierarchy_popup(ctx);
        }
        if self.code_actions_open {
            self.render_code_actions_popup(ctx);
        }
        if self.command_palette {
            self.render_command_palette(ctx);
        }
        if self.source_control_branch_picker_open {
            self.render_git_branch_switcher(ctx);
        }
        if self.source_control_history_open {
            self.render_git_history_panel(ctx);
        }
        if self.source_control_stashes_open {
            self.render_git_stashes_panel(ctx);
        }
        if self.source_control_hunks_open {
            self.render_git_hunks_panel(ctx);
        }
        if self.save_as_open {
            self.render_save_as(ctx);
        }
        if self.settings_panel_open {
            self.render_settings_panel(ctx);
        }
        if self.theme_picker_open {
            self.render_theme_picker(ctx);
        }
        if self.keybindings_open {
            self.render_keybindings_panel(ctx);
        }
        if self.devtools_open {
            self.render_devtools_overlay(ctx);
        }
        if self.gpu_acceleration_prompt.is_some() {
            self.render_gpu_acceleration_prompt(ctx);
            self.render_lsp_enable_prompt(ctx);
        }
        if self.available_update.is_some() {
            self.render_update_prompt(ctx);
        }
        if self.pending_update_install.is_some() {
            self.render_update_ready_prompt(ctx);
        }
        if self.dirty_close_buffer.is_some() {
            self.render_unsaved_close(ctx);
        }
        if self.dirty_reload_buffer.is_some() {
            self.render_reload_from_disk(ctx);
        }
        if self.save_conflict_buffer.is_some() {
            self.render_save_conflict(ctx);
        }
        if self.pending_workspace_switch.is_some() {
            self.render_workspace_switch_guard(ctx);
        }
        if self.pending_workspace_trust_prompt.is_some() {
            self.render_workspace_trust_prompt_guard(ctx);
        }
        if self.pending_exit.is_some() {
            self.render_exit_guard(ctx);
        }
        if self.explorer_file_action.is_some() {
            self.render_explorer_file_action(ctx);
        }
        if self.explorer_delete_target.is_some() {
            self.render_explorer_delete(ctx);
        }
        if self.pending_editor_file_drop.is_some() {
            self.render_editor_file_drop_selector(ctx);
        }
        if self.pending_source_control_discard.is_some() {
            self.render_source_control_discard(ctx);
        }
        if self.pending_source_control_smart_commit.is_some() {
            self.render_source_control_smart_commit(ctx);
        }
        if self.pending_source_control_empty_commit.is_some() {
            self.render_source_control_empty_commit(ctx);
        }
        if self
            .pending_source_control_protected_branch_commit
            .is_some()
        {
            self.render_source_control_protected_branch_commit(ctx);
        }
        if self.pending_source_control_commit_save.is_some() {
            self.render_source_control_commit_save_prompt(ctx);
        }
        if self.pending_source_control_stash_save.is_some() {
            self.render_source_control_stash_save_prompt(ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::primary_click_is_outside_window;
    use eframe::egui::{Rect, pos2};

    #[test]
    fn popup_dismissal_requires_primary_click_outside_without_child_popup() {
        let window = Rect::from_min_max(pos2(10.0, 10.0), pos2(30.0, 30.0));

        assert!(primary_click_is_outside_window(
            false,
            true,
            Some(pos2(40.0, 20.0)),
            window
        ));
        assert!(!primary_click_is_outside_window(
            false,
            true,
            Some(pos2(20.0, 20.0)),
            window
        ));
        assert!(!primary_click_is_outside_window(
            true,
            true,
            Some(pos2(40.0, 20.0)),
            window
        ));
        assert!(!primary_click_is_outside_window(false, false, None, window));
    }
}
