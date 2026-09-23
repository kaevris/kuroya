use crate::{
    KuroyaApp,
    path_display::{display_error_label_cow, sanitized_display_label_cow},
    popup_buttons::{PopupButtonKind, popup_button, popup_button_enabled},
    save_lifecycle::{buffer_display_name, dirty_buffer_save_block_reason, has_active_save_work},
    source_control_branch_picker::source_control_branch_display_name,
    source_control_runtime::{
        SOURCE_CONTROL_EMPTY_COMMIT_MESSAGE_STATUS, source_control_normalized_commit_message,
    },
    transient_state::{
        PendingSourceControlCommitSave, PendingSourceControlEmptyCommit,
        PendingSourceControlProtectedBranchCommit, PendingSourceControlSmartCommit,
        PendingSourceControlStashSave,
    },
    ui_text::count_label,
    workspace_state::settings_path,
};
use eframe::egui::{self, Align, Context, Key, RichText};
use kuroya_core::{BufferId, GitSmartCommitChanges, TextBuffer};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    path::PathBuf,
};

const SOURCE_CONTROL_PROTECTED_BRANCH_PATTERN_MAX_CHARS: usize = 160;
const SOURCE_CONTROL_COMMIT_MESSAGE_DISPLAY_MAX_CHARS: usize = 160;
const SOURCE_CONTROL_SAVE_PROMPT_FILE_LABEL_MAX_CHARS: usize = 96;
const SOURCE_CONTROL_SAVE_PROMPT_ID_DEDUPE_INITIAL_CAPACITY_MAX: usize = 1024;

impl KuroyaApp {
    pub(crate) fn begin_source_control_smart_commit_suggestion(
        &mut self,
        request_id: u64,
        message: String,
        smart_commit_changes: GitSmartCommitChanges,
        change_count: usize,
    ) {
        if self.source_control_commit_prompt_request_is_stale(request_id) {
            return;
        }
        let Some(message) = source_control_normalized_commit_message(message) else {
            self.pending_source_control_smart_commit = None;
            self.cancel_source_control_commit_request(request_id);
            self.status = SOURCE_CONTROL_EMPTY_COMMIT_MESSAGE_STATUS.to_owned();
            return;
        };
        self.pending_source_control_smart_commit = Some(PendingSourceControlSmartCommit {
            request_id,
            message,
            smart_commit_changes,
            change_count,
        });
    }

    fn confirm_source_control_smart_commit_once(&mut self) {
        let Some(target) = self.pending_source_control_smart_commit.take() else {
            return;
        };
        if self.source_control_commit_prompt_request_is_stale(target.request_id) {
            return;
        }
        if !self.require_trusted_source_control_mutation("committing changes") {
            self.cancel_source_control_commit_request(target.request_id);
            return;
        }
        self.request_commit_changes_with_request(
            target.request_id,
            target.message,
            Some(target.smart_commit_changes),
            false,
        );
    }

    fn confirm_source_control_smart_commit_always(&mut self) {
        let Some(target) = self.pending_source_control_smart_commit.take() else {
            return;
        };
        if self.source_control_commit_prompt_request_is_stale(target.request_id) {
            return;
        }
        if !self.require_trusted_source_control_mutation("committing changes") {
            self.cancel_source_control_commit_request(target.request_id);
            return;
        }
        self.settings.git_enable_smart_commit = true;
        self.settings_panel_draft.git_enable_smart_commit = true;
        self.save_source_control_smart_commit_settings("Smart commit enabled");
        self.request_commit_changes_with_request(
            target.request_id,
            target.message,
            Some(target.smart_commit_changes),
            false,
        );
    }

    fn disable_source_control_smart_commit_suggestions(&mut self) {
        if let Some(target) = self.pending_source_control_smart_commit.take() {
            self.cancel_source_control_commit_request(target.request_id);
        }
        self.settings.git_suggest_smart_commit = false;
        self.settings_panel_draft.git_suggest_smart_commit = false;
        self.save_source_control_smart_commit_settings("Smart commit suggestions disabled");
    }

    fn save_source_control_smart_commit_settings(&mut self, success: &str) {
        let path = settings_path(&self.workspace.root);
        if let Err(error) = self.settings.save(&path) {
            let error = error.to_string();
            self.status = source_control_smart_commit_settings_save_failure_status(success, &error);
        } else {
            self.status = success.to_owned();
        }
    }

    pub(crate) fn render_source_control_smart_commit(&mut self, ctx: &Context) {
        let Some(target) = self.pending_source_control_smart_commit.as_ref() else {
            return;
        };
        if self.source_control_commit_prompt_request_is_stale(target.request_id) {
            self.pending_source_control_smart_commit = None;
            return;
        }
        let mut commit_once = false;
        let mut commit_always = false;
        let mut disable_suggestion = false;
        let mut cancel = false;

        egui::Window::new("No Staged Changes")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 24.0,
            ))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([560.0, 168.0])
            .show(ctx, |ui| {
                ui.label(RichText::new("There are no staged changes to commit.").strong());
                ui.label(source_control_smart_commit_suggestion_body(
                    target.change_count,
                ));
                ui.label("You can change this later in Settings > Git > Smart Commit.");

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    cancel = true;
                }

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if popup_button(ui, "Cancel", PopupButtonKind::Secondary).clicked() {
                        cancel = true;
                    }
                    if popup_button(
                        ui,
                        source_control_smart_commit_never_button_label(),
                        PopupButtonKind::Secondary,
                    )
                    .clicked()
                    {
                        disable_suggestion = true;
                    }
                    if popup_button(
                        ui,
                        source_control_smart_commit_always_button_label(),
                        PopupButtonKind::Secondary,
                    )
                    .clicked()
                    {
                        commit_always = true;
                    }
                    if popup_button(
                        ui,
                        source_control_smart_commit_once_button_label(),
                        PopupButtonKind::Primary,
                    )
                    .clicked()
                    {
                        commit_once = true;
                    }
                });
            });

        if cancel {
            if let Some(target) = self.pending_source_control_smart_commit.take() {
                self.cancel_source_control_commit_request(target.request_id);
            }
            self.status = "Smart commit canceled".to_owned();
        } else if disable_suggestion {
            self.disable_source_control_smart_commit_suggestions();
        } else if commit_always {
            self.confirm_source_control_smart_commit_always();
        } else if commit_once {
            self.confirm_source_control_smart_commit_once();
        }
    }

    pub(crate) fn begin_source_control_empty_commit_confirmation(
        &mut self,
        request_id: u64,
        message: String,
    ) {
        if self.source_control_commit_prompt_request_is_stale(request_id) {
            return;
        }
        let Some(message) = source_control_normalized_commit_message(message) else {
            self.pending_source_control_empty_commit = None;
            self.cancel_source_control_commit_request(request_id);
            self.status = SOURCE_CONTROL_EMPTY_COMMIT_MESSAGE_STATUS.to_owned();
            return;
        };
        self.pending_source_control_empty_commit = Some(PendingSourceControlEmptyCommit {
            request_id,
            message,
        });
    }

    fn confirm_source_control_empty_commit(&mut self) {
        let Some(target) = self.pending_source_control_empty_commit.take() else {
            return;
        };
        if self.source_control_commit_prompt_request_is_stale(target.request_id) {
            return;
        }
        if !self.require_trusted_source_control_mutation("committing changes") {
            self.cancel_source_control_commit_request(target.request_id);
            return;
        }
        self.request_commit_changes_with_request(target.request_id, target.message, None, true);
    }

    pub(crate) fn render_source_control_empty_commit(&mut self, ctx: &Context) {
        let Some(target) = self.pending_source_control_empty_commit.as_ref() else {
            return;
        };
        if self.source_control_commit_prompt_request_is_stale(target.request_id) {
            self.pending_source_control_empty_commit = None;
            return;
        }
        let mut commit = false;
        let mut cancel = false;

        egui::Window::new("Confirm Empty Commit")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 24.0,
            ))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([520.0, 140.0])
            .show(ctx, |ui| {
                ui.label(RichText::new("There are no staged changes to commit.").strong());
                ui.label(source_control_empty_commit_confirmation_body(
                    &target.message,
                ));

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    cancel = true;
                }

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if popup_button(ui, "Cancel", PopupButtonKind::Secondary).clicked() {
                        cancel = true;
                    }
                    if popup_button(ui, "Create Empty Commit", PopupButtonKind::Primary).clicked() {
                        commit = true;
                    }
                });
            });

        if cancel {
            if let Some(target) = self.pending_source_control_empty_commit.take() {
                self.cancel_source_control_commit_request(target.request_id);
            }
            self.status = "Empty commit canceled".to_owned();
        } else if commit {
            self.confirm_source_control_empty_commit();
        }
    }

    pub(crate) fn begin_source_control_protected_branch_commit_prompt(
        &mut self,
        request_id: u64,
        message: String,
        smart_commit_changes: Option<GitSmartCommitChanges>,
        allow_empty: bool,
        branch: String,
        pattern: String,
    ) {
        if self.source_control_commit_prompt_request_is_stale(request_id) {
            return;
        }
        let Some(message) = source_control_normalized_commit_message(message) else {
            self.pending_source_control_protected_branch_commit = None;
            self.cancel_source_control_commit_request(request_id);
            self.status = SOURCE_CONTROL_EMPTY_COMMIT_MESSAGE_STATUS.to_owned();
            return;
        };
        self.pending_source_control_protected_branch_commit =
            Some(PendingSourceControlProtectedBranchCommit {
                request_id,
                message,
                smart_commit_changes,
                allow_empty,
                branch,
                pattern,
            });
    }

    fn confirm_source_control_protected_branch_commit(&mut self) {
        let Some(target) = self.pending_source_control_protected_branch_commit.take() else {
            return;
        };
        if self.source_control_commit_prompt_request_is_stale(target.request_id) {
            return;
        }
        if !self.require_trusted_source_control_mutation("committing changes") {
            self.cancel_source_control_commit_request(target.request_id);
            return;
        }
        self.request_commit_changes_after_branch_protection_with_request(
            target.request_id,
            target.message,
            target.smart_commit_changes,
            target.allow_empty,
        );
    }

    fn create_branch_for_source_control_protected_branch_commit(&mut self) {
        let Some(target) = self.pending_source_control_protected_branch_commit.take() else {
            return;
        };
        if self.source_control_commit_prompt_request_is_stale(target.request_id) {
            return;
        }
        self.cancel_source_control_commit_request(target.request_id);
        self.begin_git_branch_switcher();
        self.status = source_control_protected_branch_new_branch_required_status_display(
            &target.branch,
            &target.pattern,
        );
    }

    pub(crate) fn render_source_control_protected_branch_commit(&mut self, ctx: &Context) {
        let Some(target) = self.pending_source_control_protected_branch_commit.as_ref() else {
            return;
        };
        if self.source_control_commit_prompt_request_is_stale(target.request_id) {
            self.pending_source_control_protected_branch_commit = None;
            return;
        }
        let mut commit = false;
        let mut create_branch = false;
        let mut cancel = false;

        egui::Window::new("Protected Branch")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 24.0,
            ))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([560.0, 164.0])
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(source_control_protected_branch_prompt_title_display(
                        &target.branch,
                    ))
                    .strong(),
                );
                ui.label(source_control_protected_branch_prompt_body_display(
                    &target.branch,
                    &target.pattern,
                ));

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    cancel = true;
                }

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if popup_button(ui, "Cancel", PopupButtonKind::Secondary).clicked() {
                        cancel = true;
                    }
                    if popup_button(ui, "Create Branch", PopupButtonKind::Secondary).clicked() {
                        create_branch = true;
                    }
                    if popup_button(ui, "Commit Anyway", PopupButtonKind::Primary).clicked() {
                        commit = true;
                    }
                });
            });

        if cancel {
            if let Some(target) = self.pending_source_control_protected_branch_commit.take() {
                self.cancel_source_control_commit_request(target.request_id);
            }
            self.status = "Commit canceled".to_owned();
        } else if create_branch {
            self.create_branch_for_source_control_protected_branch_commit();
        } else if commit {
            self.confirm_source_control_protected_branch_commit();
        }
    }

    pub(crate) fn begin_source_control_commit_save_prompt(
        &mut self,
        request_id: u64,
        message: String,
        smart_commit_changes: Option<GitSmartCommitChanges>,
        allow_empty: bool,
        ids: Vec<BufferId>,
    ) {
        if self.source_control_commit_prompt_request_is_stale(request_id) {
            return;
        }
        let Some(message) = source_control_normalized_commit_message(message) else {
            self.pending_source_control_commit_save = None;
            self.cancel_source_control_commit_request(request_id);
            self.status = SOURCE_CONTROL_EMPTY_COMMIT_MESSAGE_STATUS.to_owned();
            return;
        };
        let ids = source_control_commit_save_prompt_valid_ids(ids, &self.buffers);
        if ids.is_empty() {
            self.pending_source_control_commit_save = None;
            self.spawn_commit_changes_with_request(
                request_id,
                message,
                smart_commit_changes,
                allow_empty,
            );
            return;
        }
        self.pending_source_control_commit_save = Some(PendingSourceControlCommitSave::Confirm {
            request_id,
            message,
            smart_commit_changes,
            allow_empty,
            ids,
        });
    }

    fn confirm_source_control_commit_without_saving(&mut self) {
        let Some(PendingSourceControlCommitSave::Confirm {
            request_id,
            message,
            smart_commit_changes,
            allow_empty,
            ..
        }) = self.pending_source_control_commit_save.take()
        else {
            return;
        };
        if self.source_control_commit_prompt_request_is_stale(request_id) {
            return;
        }
        if !self.require_trusted_source_control_mutation("committing changes") {
            self.cancel_source_control_commit_request(request_id);
            return;
        }
        self.spawn_commit_changes_with_request(
            request_id,
            message,
            smart_commit_changes,
            allow_empty,
        );
    }

    fn save_source_control_commit_files(&mut self) {
        let Some(pending) = self.pending_source_control_commit_save.take() else {
            return;
        };
        let PendingSourceControlCommitSave::Confirm {
            request_id,
            message,
            smart_commit_changes,
            allow_empty,
            ids,
        } = pending
        else {
            self.pending_source_control_commit_save = Some(pending);
            return;
        };
        if self.source_control_commit_prompt_request_is_stale(request_id) {
            return;
        }
        if !self.require_trusted_source_control_mutation("committing changes") {
            self.cancel_source_control_commit_request(request_id);
            return;
        }
        let changed_on_disk = self.observed_external_change_buffer_ids();
        if let Some(reason) = dirty_buffer_save_block_reason(
            &ids,
            &self.buffers,
            &changed_on_disk,
            &self.lossy_decoded_buffers,
            &self.binary_preview_buffers,
            "committing",
        ) {
            self.status = reason;
            self.pending_source_control_commit_save =
                Some(PendingSourceControlCommitSave::Confirm {
                    request_id,
                    message,
                    smart_commit_changes,
                    allow_empty,
                    ids,
                });
            return;
        }

        for id in ids.iter().copied() {
            self.spawn_save(id);
        }
        self.pending_source_control_commit_save = Some(PendingSourceControlCommitSave::Saving {
            request_id,
            message,
            smart_commit_changes,
            allow_empty,
            ids,
        });
        if !self.source_control_commit_prompt_request_is_stale(request_id) {
            self.advance_pending_source_control_commit_after_save();
        }
    }

    pub(crate) fn render_source_control_commit_save_prompt(&mut self, ctx: &Context) {
        let Some(pending) = self.pending_source_control_commit_save.as_ref() else {
            return;
        };
        let pending_request_id = source_control_commit_save_prompt_request_id(pending);
        if self.source_control_commit_prompt_request_is_stale(pending_request_id) {
            self.pending_source_control_commit_save = None;
            return;
        }
        let PendingSourceControlCommitSave::Confirm {
            request_id, ids, ..
        } = pending
        else {
            self.render_source_control_commit_saving_prompt(ctx);
            return;
        };
        let request_id = *request_id;
        let changed_on_disk = self.observed_external_change_buffer_ids();
        let save_block = dirty_buffer_save_block_reason(
            ids,
            &self.buffers,
            &changed_on_disk,
            &self.lossy_decoded_buffers,
            &self.binary_preview_buffers,
            "committing",
        );
        let can_save = save_block.is_none();
        let labels = source_control_commit_save_prompt_labels(&self.buffers, ids);
        let mut save = false;
        let mut commit = false;
        let mut cancel = false;

        egui::Window::new("Unsaved Changes Before Commit")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 24.0,
            ))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([560.0, 172.0])
            .show(ctx, |ui| {
                ui.label(RichText::new(labels.title.as_str()).strong());
                ui.label(labels.body.as_str());
                if let Some(reason) = &save_block {
                    ui.label(reason);
                }

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    cancel = true;
                }

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if popup_button(ui, "Cancel", PopupButtonKind::Secondary).clicked() {
                        cancel = true;
                    }
                    if popup_button(ui, "Commit Anyway", PopupButtonKind::Secondary).clicked() {
                        commit = true;
                    }
                    if popup_button_enabled(
                        ui,
                        can_save,
                        labels.primary_label.as_str(),
                        PopupButtonKind::Primary,
                    )
                    .clicked()
                    {
                        save = true;
                    }
                });
            });

        if cancel {
            self.pending_source_control_commit_save = None;
            self.cancel_source_control_commit_request(request_id);
            self.status = "Commit canceled".to_owned();
        } else if commit {
            self.confirm_source_control_commit_without_saving();
        } else if save {
            self.save_source_control_commit_files();
        }
    }

    fn render_source_control_commit_saving_prompt(&mut self, ctx: &Context) {
        if let Some(PendingSourceControlCommitSave::Saving { request_id, .. }) =
            self.pending_source_control_commit_save.as_ref()
        {
            if self.source_control_commit_prompt_request_is_stale(*request_id) {
                self.pending_source_control_commit_save = None;
                return;
            }
        }
        self.advance_pending_source_control_commit_after_save();
        let Some(PendingSourceControlCommitSave::Saving {
            request_id, ids, ..
        }) = self.pending_source_control_commit_save.as_ref()
        else {
            return;
        };
        let request_id = *request_id;
        if render_source_control_saving_prompt(
            ctx,
            "Commit Changes",
            "Saving before commit",
            &source_control_saving_prompt_body(
                "committing",
                source_control_save_remaining_count(
                    ids,
                    &self.buffers,
                    &self.in_flight_saves,
                    &self.queued_save_paths,
                    &self.pending_format_on_save,
                ),
            ),
        ) {
            self.pending_source_control_commit_save = None;
            self.cancel_source_control_commit_request(request_id);
            self.status = "Commit canceled; in-flight saves will still finish".to_owned();
        }
    }

    fn source_control_commit_prompt_request_is_stale(&self, request_id: u64) -> bool {
        let active_request_id = self.source_control_commit_active_request_id;
        active_request_id != 0 && active_request_id != request_id
    }

    pub(crate) fn begin_source_control_stash_save_prompt(
        &mut self,
        message: String,
        ids: Vec<BufferId>,
    ) {
        self.pending_source_control_stash_save =
            Some(PendingSourceControlStashSave::Confirm { message, ids });
    }

    fn confirm_source_control_stash_without_saving(&mut self) {
        let Some(PendingSourceControlStashSave::Confirm { message, .. }) =
            self.pending_source_control_stash_save.take()
        else {
            return;
        };
        if !self.require_trusted_source_control_mutation("creating a stash") {
            return;
        }
        self.spawn_git_stash_save(message);
    }

    fn save_source_control_stash_files(&mut self) {
        let Some(pending) = self.pending_source_control_stash_save.take() else {
            return;
        };
        let PendingSourceControlStashSave::Confirm { message, ids } = pending else {
            self.pending_source_control_stash_save = Some(pending);
            return;
        };
        if !self.require_trusted_source_control_mutation("creating a stash") {
            return;
        }
        let changed_on_disk = self.observed_external_change_buffer_ids();
        if let Some(reason) = dirty_buffer_save_block_reason(
            &ids,
            &self.buffers,
            &changed_on_disk,
            &self.lossy_decoded_buffers,
            &self.binary_preview_buffers,
            "stashing",
        ) {
            self.status = reason;
            self.pending_source_control_stash_save =
                Some(PendingSourceControlStashSave::Confirm { message, ids });
            return;
        }

        for id in ids.iter().copied() {
            self.spawn_save(id);
        }
        self.pending_source_control_stash_save =
            Some(PendingSourceControlStashSave::Saving { message, ids });
        self.advance_pending_source_control_stash_after_save();
    }

    pub(crate) fn render_source_control_stash_save_prompt(&mut self, ctx: &Context) {
        let Some(pending) = self.pending_source_control_stash_save.as_ref() else {
            return;
        };
        let PendingSourceControlStashSave::Confirm { ids, .. } = pending else {
            self.render_source_control_stash_saving_prompt(ctx);
            return;
        };
        let changed_on_disk = self.observed_external_change_buffer_ids();
        let save_block = dirty_buffer_save_block_reason(
            ids,
            &self.buffers,
            &changed_on_disk,
            &self.lossy_decoded_buffers,
            &self.binary_preview_buffers,
            "stashing",
        );
        let can_save = save_block.is_none();
        let labels = source_control_stash_save_prompt_labels(&self.buffers, ids);
        let mut save = false;
        let mut stash = false;
        let mut cancel = false;

        egui::Window::new("Unsaved Changes Before Stash")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 24.0,
            ))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([560.0, 172.0])
            .show(ctx, |ui| {
                ui.label(RichText::new(labels.title.as_str()).strong());
                ui.label(labels.body.as_str());
                if let Some(reason) = &save_block {
                    ui.label(reason);
                }

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    cancel = true;
                }

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if popup_button(ui, "Cancel", PopupButtonKind::Secondary).clicked() {
                        cancel = true;
                    }
                    if popup_button(ui, "Stash Anyway", PopupButtonKind::Secondary).clicked() {
                        stash = true;
                    }
                    if popup_button_enabled(
                        ui,
                        can_save,
                        labels.primary_label.as_str(),
                        PopupButtonKind::Primary,
                    )
                    .clicked()
                    {
                        save = true;
                    }
                });
            });

        if cancel {
            self.pending_source_control_stash_save = None;
            self.status = "Stash canceled".to_owned();
        } else if stash {
            self.confirm_source_control_stash_without_saving();
        } else if save {
            self.save_source_control_stash_files();
        }
    }

    fn render_source_control_stash_saving_prompt(&mut self, ctx: &Context) {
        self.advance_pending_source_control_stash_after_save();
        let Some(PendingSourceControlStashSave::Saving { ids, .. }) =
            self.pending_source_control_stash_save.as_ref()
        else {
            return;
        };
        if render_source_control_saving_prompt(
            ctx,
            "Save Stash",
            "Saving before stash",
            &source_control_saving_prompt_body(
                "stashing",
                source_control_save_remaining_count(
                    ids,
                    &self.buffers,
                    &self.in_flight_saves,
                    &self.queued_save_paths,
                    &self.pending_format_on_save,
                ),
            ),
        ) {
            self.pending_source_control_stash_save = None;
            self.status = "Stash canceled; in-flight saves will still finish".to_owned();
        }
    }
}

fn render_source_control_saving_prompt(
    ctx: &Context,
    window_title: &str,
    title: &str,
    body: &str,
) -> bool {
    let mut cancel = false;

    egui::Window::new(window_title)
        .max_size(crate::layout::popup_window_max_size_with_top_margin(
            ctx, 24.0,
        ))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .fixed_size([520.0, 132.0])
        .show(ctx, |ui| {
            ui.label(RichText::new(title).strong());
            ui.label(body);

            if ui.input(|input| input.key_pressed(Key::Escape)) {
                cancel = true;
            }

            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if popup_button(ui, "Cancel", PopupButtonKind::Secondary).clicked() {
                    cancel = true;
                }
            });
        });

    cancel
}

pub(crate) fn source_control_smart_commit_suggestion_body(change_count: usize) -> String {
    let label = if change_count == 1 {
        "1 eligible change"
    } else {
        return format!(
            "Would you like to stage {change_count} eligible changes and commit them directly?"
        );
    };
    format!("Would you like to stage {label} and commit it directly?")
}

pub(crate) fn source_control_smart_commit_once_button_label() -> &'static str {
    "Stage and Commit"
}

pub(crate) fn source_control_smart_commit_always_button_label() -> &'static str {
    "Always Stage and Commit"
}

pub(crate) fn source_control_smart_commit_never_button_label() -> &'static str {
    "Never Ask Again"
}

pub(crate) fn source_control_empty_commit_confirmation_body(message: &str) -> String {
    format!(
        "Create an empty commit with message \"{}\"?",
        source_control_commit_message_display(message)
    )
}

pub(crate) fn source_control_commit_save_prompt_title(count: usize) -> String {
    if count == 1 {
        "1 file has unsaved changes".to_owned()
    } else {
        format!("{count} files have unsaved changes")
    }
}

#[cfg(test)]
pub(crate) fn source_control_commit_save_prompt_body(
    buffers: &[kuroya_core::TextBuffer],
    ids: &[BufferId],
) -> String {
    source_control_save_prompt_body(buffers, ids, "committing", "commit")
}

#[cfg(test)]
pub(crate) fn source_control_stash_save_prompt_body(
    buffers: &[kuroya_core::TextBuffer],
    ids: &[BufferId],
) -> String {
    source_control_save_prompt_body(buffers, ids, "stashing", "stash")
}

struct SourceControlSavePromptLabels {
    title: String,
    body: String,
    primary_label: String,
}

fn source_control_commit_save_prompt_labels(
    buffers: &[kuroya_core::TextBuffer],
    ids: &[BufferId],
) -> SourceControlSavePromptLabels {
    source_control_save_prompt_labels(buffers, ids, "committing", "commit", "Commit")
}

fn source_control_stash_save_prompt_labels(
    buffers: &[kuroya_core::TextBuffer],
    ids: &[BufferId],
) -> SourceControlSavePromptLabels {
    source_control_save_prompt_labels(buffers, ids, "stashing", "stash", "Stash")
}

fn source_control_save_prompt_labels(
    buffers: &[kuroya_core::TextBuffer],
    ids: &[BufferId],
    before_action: &str,
    fallback_action: &str,
    primary_action: &str,
) -> SourceControlSavePromptLabels {
    let count = ids.len();
    SourceControlSavePromptLabels {
        title: source_control_commit_save_prompt_title(count),
        body: source_control_save_prompt_body(buffers, ids, before_action, fallback_action),
        primary_label: source_control_save_prompt_primary_label(count, primary_action),
    }
}

fn source_control_save_prompt_body(
    buffers: &[kuroya_core::TextBuffer],
    ids: &[BufferId],
    before_action: &str,
    fallback_action: &str,
) -> String {
    let Some(first) = source_control_first_save_prompt_file_label(buffers, ids) else {
        return format!("Save files before {before_action}, {fallback_action} anyway, or cancel.");
    };

    if ids.len() == 1 {
        format!("Save {first} before {before_action}, {fallback_action} anyway, or cancel.")
    } else {
        format!(
            "Save {first} and {} before {before_action}, {fallback_action} anyway, or cancel.",
            count_label(ids.len().saturating_sub(1), "other file", "other files")
        )
    }
}

fn source_control_first_save_prompt_file_label(
    buffers: &[kuroya_core::TextBuffer],
    ids: &[BufferId],
) -> Option<String> {
    let mut buffers_by_id = HashMap::with_capacity(buffers.len());
    for buffer in buffers {
        buffers_by_id.insert(buffer.id(), buffer);
    }
    ids.iter().find_map(|id| {
        buffers_by_id
            .get(id)
            .map(|buffer| source_control_save_prompt_file_label(&buffer_display_name(buffer)))
    })
}

fn source_control_commit_save_prompt_valid_ids(
    ids: Vec<BufferId>,
    buffers: &[TextBuffer],
) -> Vec<BufferId> {
    let dirty_buffer_ids = source_control_dirty_buffer_ids(buffers);
    let mut seen = HashSet::with_capacity(
        ids.len()
            .min(SOURCE_CONTROL_SAVE_PROMPT_ID_DEDUPE_INITIAL_CAPACITY_MAX),
    );
    let mut valid_ids = Vec::with_capacity(ids.len().min(dirty_buffer_ids.len()));
    for id in ids {
        if dirty_buffer_ids.contains(&id) && seen.insert(id) {
            valid_ids.push(id);
        }
    }
    valid_ids
}

pub(crate) fn source_control_save_prompt_primary_label(count: usize, action: &str) -> String {
    if count == 1 {
        format!("Save and {action}")
    } else {
        format!("Save All and {action}")
    }
}

pub(crate) fn source_control_saving_prompt_body(action: &str, remaining: usize) -> String {
    format!(
        "Saving {} before {action}.",
        count_label(remaining, "file", "files")
    )
}

pub(crate) fn source_control_save_remaining_count<T>(
    ids: &[BufferId],
    buffers: &[TextBuffer],
    in_flight_saves: &HashSet<BufferId>,
    queued_save_paths: &HashMap<BufferId, PathBuf>,
    pending_format_on_save: &HashMap<BufferId, T>,
) -> usize {
    let dirty_buffer_ids = source_control_dirty_buffer_ids(buffers);
    ids.iter()
        .filter(|id| {
            has_active_save_work(
                **id,
                in_flight_saves,
                queued_save_paths,
                pending_format_on_save,
            ) || dirty_buffer_ids.contains(*id)
        })
        .count()
}

fn source_control_dirty_buffer_ids(buffers: &[TextBuffer]) -> HashSet<BufferId> {
    let mut ids = HashSet::with_capacity(buffers.len());
    for buffer in buffers {
        if buffer.is_dirty() {
            ids.insert(buffer.id());
        }
    }
    ids
}

fn source_control_commit_save_prompt_request_id(pending: &PendingSourceControlCommitSave) -> u64 {
    match pending {
        PendingSourceControlCommitSave::Confirm { request_id, .. }
        | PendingSourceControlCommitSave::Saving { request_id, .. } => *request_id,
    }
}

fn source_control_commit_message_display(message: &str) -> String {
    source_control_commit_message_display_cow(message).into_owned()
}

fn source_control_commit_message_display_cow<'a>(message: &'a str) -> Cow<'a, str> {
    let trimmed = message.trim();
    let label = sanitized_display_label_cow(
        trimmed,
        SOURCE_CONTROL_COMMIT_MESSAGE_DISPLAY_MAX_CHARS,
        "commit message",
    );

    if trimmed.len() == message.len() {
        label
    } else {
        Cow::Owned(label.into_owned())
    }
}

fn source_control_save_prompt_file_label(label: &str) -> String {
    source_control_save_prompt_file_label_cow(label).into_owned()
}

fn source_control_save_prompt_file_label_cow<'a>(label: &'a str) -> Cow<'a, str> {
    sanitized_display_label_cow(
        label,
        SOURCE_CONTROL_SAVE_PROMPT_FILE_LABEL_MAX_CHARS,
        "file",
    )
}

pub(crate) fn source_control_smart_commit_settings_save_failure_status(
    success: &str,
    error: &str,
) -> String {
    let error = display_error_label_cow(error);
    format!("{success}, but settings save failed: {}", error.as_ref())
}

pub(crate) fn source_control_protected_branch_prompt_title_display(branch: &str) -> String {
    format!(
        "Commit to protected branch {}?",
        source_control_branch_display_name(branch)
    )
}

pub(crate) fn source_control_protected_branch_prompt_body_display(
    branch: &str,
    pattern: &str,
) -> String {
    format!(
        "Branch {} matches protected branch pattern {}.",
        source_control_branch_display_name(branch),
        source_control_protected_branch_pattern_display(pattern)
    )
}

pub(crate) fn source_control_protected_branch_new_branch_required_status_display(
    branch: &str,
    pattern: &str,
) -> String {
    format!(
        "Branch {} is protected by {}; create or switch branches before committing",
        source_control_branch_display_name(branch),
        source_control_protected_branch_pattern_display(pattern)
    )
}

fn source_control_protected_branch_pattern_display(pattern: &str) -> String {
    source_control_protected_branch_pattern_display_cow(pattern).into_owned()
}

fn source_control_protected_branch_pattern_display_cow<'a>(pattern: &'a str) -> Cow<'a, str> {
    sanitized_display_label_cow(
        pattern,
        SOURCE_CONTROL_PROTECTED_BRANCH_PATTERN_MAX_CHARS,
        "protected branch pattern",
    )
}

#[cfg(test)]
mod tests;
