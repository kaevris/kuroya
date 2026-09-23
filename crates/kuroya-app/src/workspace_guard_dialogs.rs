use crate::{KuroyaApp, transient_state::PendingWorkspaceSwitch};
use confirm::render_workspace_switch_confirm_guard;
use eframe::egui::Context;
use home_directory::render_workspace_switch_home_directory_guard;
use saving::render_workspace_switch_saving_guard;
use trust::render_workspace_trust_prompt;

mod confirm;
mod home_directory;
mod saving;
mod trust;

impl KuroyaApp {
    pub(crate) fn render_workspace_switch_guard(&mut self, ctx: &Context) {
        if self.exit_confirmed || self.pending_exit.is_some() {
            self.clear_pending_workspace_switch_for_exit();
            return;
        }
        if self.cancel_invalid_pending_workspace_switch() {
            return;
        }

        match self.pending_workspace_switch.as_ref() {
            Some(PendingWorkspaceSwitch::Confirm { target }) => {
                render_workspace_switch_confirm_guard(self, ctx, target.clone());
            }
            Some(PendingWorkspaceSwitch::ConfirmHomeDirectory { target }) => {
                render_workspace_switch_home_directory_guard(self, ctx, target.clone());
            }
            Some(PendingWorkspaceSwitch::Saving { .. }) => {
                render_workspace_switch_saving_guard(self, ctx);
            }
            None => {}
        }
    }

    pub(crate) fn render_workspace_trust_prompt_guard(&mut self, ctx: &Context) {
        if self.exit_confirmed || self.pending_exit.is_some() {
            return;
        }
        if self.workspace_placeholder || self.workspace_trusted {
            self.pending_workspace_trust_prompt = None;
            return;
        }
        if let Some(root) = self.pending_workspace_trust_prompt.clone() {
            render_workspace_trust_prompt(self, ctx, root);
        }
    }
}
