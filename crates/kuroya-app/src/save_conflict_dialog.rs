use crate::{
    KuroyaApp,
    path_display::{DISPLAY_PATH_LABEL_MAX_CHARS, sanitized_display_label_cow},
    popup_buttons::{PopupButtonKind, popup_button},
};
use eframe::egui::{self, Align, Context, Key, RichText};
use kuroya_core::{BufferId, TextBuffer};
use std::borrow::Cow;

impl KuroyaApp {
    pub(crate) fn render_save_conflict(&mut self, ctx: &Context) {
        let Some(id) = self.save_conflict_buffer else {
            return;
        };
        let Some(buffer) = self.buffer(id) else {
            self.clear_missing_save_conflict_buffer(id);
            return;
        };

        let conflict_still_current = buffer
            .path()
            .is_some_and(|path| self.save_needs_observed_external_change_confirmation(id, path));
        if !conflict_still_current {
            self.save_conflict_buffer = None;
            self.spawn_save(id);
            return;
        }

        let label = save_conflict_display_label(&self.buffer_label_for(buffer));
        let mut overwrite = false;
        let mut reload = false;
        let mut cancel = false;

        egui::Window::new("Save File Conflict")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 24.0,
            ))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([540.0, 166.0])
            .show(ctx, |ui| {
                ui.label(RichText::new(label).strong());
                ui.label("This file changed on disk after your local edits.");
                ui.label(save_conflict_resolution_body());

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    cancel = true;
                }

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if popup_button(ui, "Cancel", PopupButtonKind::Secondary).clicked() {
                        cancel = true;
                    }
                    if popup_button(
                        ui,
                        save_conflict_reload_button_label(),
                        PopupButtonKind::Danger,
                    )
                    .clicked()
                    {
                        reload = true;
                    }
                    if popup_button(ui, "Overwrite", PopupButtonKind::Danger).clicked() {
                        overwrite = true;
                    }
                });
            });

        if cancel {
            self.cancel_save_conflict(id);
        } else if reload {
            self.discard_save_conflict_and_reload(id);
        } else if overwrite {
            self.overwrite_save_conflict(id);
        }
    }

    fn cancel_save_conflict(&mut self, id: BufferId) {
        self.save_conflict_buffer = None;
        self.restore_dirty_close_guard_after_save_conflict(id);
        self.status = "Save canceled".to_owned();
    }

    fn discard_save_conflict_and_reload(&mut self, id: BufferId) {
        self.save_conflict_buffer = None;
        let advance_pending_close = self.clear_save_conflict_close_request(id);
        self.discard_and_reload_buffer_from_disk(id);
        if advance_pending_close {
            self.begin_next_pending_close();
        }
    }

    fn overwrite_save_conflict(&mut self, id: BufferId) {
        self.save_conflict_buffer = None;
        self.force_save_over_external_change(id);
    }

    fn clear_save_conflict_close_request(&mut self, id: BufferId) -> bool {
        if self.close_after_save == Some(id) {
            self.close_after_save = None;
            self.pending_close_buffers.retain(|pending| *pending != id);
            true
        } else {
            false
        }
    }

    fn restore_dirty_close_guard_after_save_conflict(&mut self, id: BufferId) {
        if !self.clear_save_conflict_close_request(id) {
            return;
        }
        if !self.buffer(id).is_some_and(TextBuffer::is_dirty) {
            self.pending_close_buffers.retain(|pending| *pending != id);
            return;
        }

        self.restore_dirty_close_guard_for_buffer(id);
    }

    fn clear_missing_save_conflict_buffer(&mut self, id: BufferId) {
        self.save_conflict_buffer = None;
        self.clear_save_conflict_close_request(id);
    }
}

fn save_conflict_display_label(label: &str) -> String {
    save_conflict_display_label_cow(label).into_owned()
}

fn save_conflict_display_label_cow(label: &str) -> Cow<'_, str> {
    sanitized_display_label_cow(label, DISPLAY_PATH_LABEL_MAX_CHARS, "Untitled")
}

fn save_conflict_resolution_body() -> &'static str {
    "Overwrite the disk version, discard local edits and reload, or cancel the save."
}

fn save_conflict_reload_button_label() -> &'static str {
    "Discard and Reload"
}

#[cfg(test)]
mod tests {
    use super::{
        DISPLAY_PATH_LABEL_MAX_CHARS, save_conflict_display_label, save_conflict_display_label_cow,
        save_conflict_reload_button_label, save_conflict_resolution_body,
    };
    use crate::{
        KuroyaApp,
        app_startup_context::AppStartupContext,
        app_state::{PendingFileReload, QueuedFileReload},
        terminal::TerminalPane,
    };
    use kuroya_core::{EditorSettings, TextBuffer, Workspace};
    use std::{borrow::Cow, path::PathBuf, time::Instant};
    use tokio::runtime::Runtime;

    #[test]
    fn save_conflict_display_label_cow_borrows_clean_ascii_and_unicode() {
        assert!(matches!(
            save_conflict_display_label_cow("clean-file.rs"),
            Cow::Borrowed("clean-file.rs")
        ));

        let unicode = "resume-\u{5909}\u{66f4}.rs";
        match save_conflict_display_label_cow(unicode) {
            Cow::Borrowed(label) => assert_eq!(label, unicode),
            Cow::Owned(label) => panic!("expected borrowed label, got {label:?}"),
        }
    }

    #[test]
    fn save_conflict_display_label_cow_owns_dirty_truncated_and_fallback_output() {
        let dirty = save_conflict_display_label_cow("bad\nname.rs\u{202e}");
        assert_eq!(dirty.as_ref(), "bad name.rs");
        assert!(matches!(dirty, Cow::Owned(_)));

        let long = format!(
            "head-{}-tail",
            "segment-".repeat(DISPLAY_PATH_LABEL_MAX_CHARS)
        );
        let truncated = save_conflict_display_label_cow(&long);
        assert!(truncated.contains("..."));
        assert!(truncated.chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS);
        assert!(matches!(truncated, Cow::Owned(_)));

        let fallback = save_conflict_display_label_cow("\n\u{202e}\u{0007}\u{2029}");
        assert_eq!(fallback.as_ref(), "Untitled");
        assert!(matches!(fallback, Cow::Owned(_)));
    }

    #[test]
    fn save_conflict_display_label_wrapper_matches_cow_helper() {
        let long = format!(
            "head-{}-tail",
            "segment-".repeat(DISPLAY_PATH_LABEL_MAX_CHARS)
        );
        let cases = [
            "clean-file.rs".to_owned(),
            "resume-\u{5909}\u{66f4}.rs".to_owned(),
            "bad\nname.rs\u{202e}".to_owned(),
            "\n\u{202e}\u{0007}\u{2029}".to_owned(),
            long,
        ];

        for value in cases {
            assert_eq!(
                save_conflict_display_label(&value),
                save_conflict_display_label_cow(&value).into_owned()
            );
        }
    }

    #[test]
    fn save_conflict_display_label_sanitizes_controls_bidi_and_bounds_length() {
        let raw = format!(
            "alpha\n{}\u{202e}\u{0007}omega.rs",
            "very-long-component-".repeat(16)
        );

        let label = save_conflict_display_label(&raw);

        assert!(label.starts_with("alpha "));
        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{0007}'));
        assert!(!label.contains('\u{202e}'));
        assert!(label.contains("..."));
        assert!(label.chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS);
    }

    #[test]
    fn save_conflict_display_label_falls_back_for_blank_control_text() {
        assert_eq!(
            save_conflict_display_label("\n\u{202e}\u{0007}\u{2029}"),
            "Untitled"
        );
    }

    #[test]
    fn save_conflict_copy_names_discarding_reload_action() {
        assert_eq!(
            save_conflict_resolution_body(),
            "Overwrite the disk version, discard local edits and reload, or cancel the save."
        );
        assert_eq!(save_conflict_reload_button_label(), "Discard and Reload");
    }

    #[test]
    fn discard_reload_cancels_deferred_reload_work_before_forced_reload() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let next_path = root.join("src/next.rs");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "local".to_owned());
        buffer.mark_dirty();
        let version = buffer.version();
        let mut next = TextBuffer::from_text(8, Some(next_path), "next".to_owned());
        next.mark_dirty();
        app.buffers.push(buffer);
        app.buffers.push(next);
        app.save_conflict_buffer = Some(7);
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(8);
        let pending = PendingFileReload {
            request_id: 99,
            path: path.clone(),
            version,
            force_dirty: false,
        };
        app.in_flight_reloads.insert(7, pending.clone());
        app.queued_file_reloads.insert(
            7,
            QueuedFileReload {
                path: path.clone(),
                force_dirty: false,
            },
        );

        app.discard_save_conflict_and_reload(7);

        assert_eq!(app.save_conflict_buffer, None);
        assert_eq!(app.close_after_save, None);
        assert_eq!(app.dirty_close_buffer, Some(8));
        assert!(app.pending_close_buffers.is_empty());
        assert!(app.canceled_file_reloads.contains(&(7, pending)));
        assert!(!app.queued_file_reloads.contains_key(&7));
        let forced_reload = app
            .in_flight_reloads
            .get(&7)
            .expect("forced reload should start after canceling stale reload work");
        assert_eq!(forced_reload.path, path);
        assert_eq!(forced_reload.version, version);
        assert!(forced_reload.force_dirty);
    }

    #[test]
    fn discard_reload_removes_matching_close_request_and_continues_pending_close_queue() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let next_path = root.join("src/next.rs");
        let last_path = root.join("src/last.rs");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text(7, Some(path), "local".to_owned());
        buffer.mark_dirty();
        let mut next = TextBuffer::from_text(8, Some(next_path), "next".to_owned());
        next.mark_dirty();
        let mut last = TextBuffer::from_text(9, Some(last_path), "last".to_owned());
        last.mark_dirty();
        app.buffers.push(buffer);
        app.buffers.push(next);
        app.buffers.push(last);
        app.save_conflict_buffer = Some(7);
        app.close_after_save = Some(7);
        app.pending_close_buffers.extend([7, 8, 7, 9]);

        app.discard_save_conflict_and_reload(7);

        assert_eq!(app.save_conflict_buffer, None);
        assert_eq!(app.close_after_save, None);
        assert_eq!(app.dirty_close_buffer, Some(8));
        assert_eq!(app.pending_close_buffers, vec![9]);
        assert!(app.in_flight_reloads.contains_key(&7));
    }

    #[test]
    fn cancel_save_conflict_from_close_request_restores_dirty_close_guard() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text(7, Some(path), "local".to_owned());
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.save_conflict_buffer = Some(7);
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(8);

        app.cancel_save_conflict(7);

        assert_eq!(app.save_conflict_buffer, None);
        assert_eq!(app.close_after_save, None);
        assert_eq!(app.dirty_close_buffer, Some(7));
        assert_eq!(app.pending_close_buffers, vec![8]);
        assert!(!app.in_flight_saves.contains(&7));
        assert!(!app.queued_save_paths.contains_key(&7));
        assert_eq!(app.status, "Save canceled");
        assert!(app.buffer(7).is_some_and(TextBuffer::is_dirty));
    }

    #[test]
    fn cancel_save_conflict_from_close_request_preserves_queued_clean_reload_guard() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "local".to_owned());
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.save_conflict_buffer = Some(7);
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(8);
        app.queued_file_reloads.insert(
            7,
            QueuedFileReload {
                path: path.clone(),
                force_dirty: false,
            },
        );

        app.cancel_save_conflict(7);

        assert_eq!(app.save_conflict_buffer, None);
        assert_eq!(app.close_after_save, None);
        assert_eq!(app.dirty_close_buffer, Some(7));
        assert_eq!(app.pending_close_buffers, vec![8]);
        assert!(app.queued_file_reloads.contains_key(&7));
        assert!(app.save_needs_observed_external_change_confirmation(7, &path));
        assert_eq!(app.status, "Save canceled");
    }

    #[test]
    fn cancel_save_conflict_from_close_request_preserves_in_flight_clean_reload_guard() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "local".to_owned());
        buffer.mark_dirty();
        let version = buffer.version();
        app.buffers.push(buffer);
        app.save_conflict_buffer = Some(7);
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(8);
        let pending = PendingFileReload {
            request_id: 99,
            path: path.clone(),
            version,
            force_dirty: false,
        };
        app.in_flight_reloads.insert(7, pending.clone());

        app.cancel_save_conflict(7);

        assert_eq!(app.save_conflict_buffer, None);
        assert_eq!(app.close_after_save, None);
        assert_eq!(app.dirty_close_buffer, Some(7));
        assert_eq!(app.pending_close_buffers, vec![8]);
        assert_eq!(app.in_flight_reloads.get(&7), Some(&pending));
        assert!(!app.canceled_file_reloads.contains(&(7, pending)));
        assert!(app.save_needs_observed_external_change_confirmation(7, &path));
        assert_eq!(app.status, "Save canceled");
    }

    #[test]
    fn cancel_save_conflict_does_not_duplicate_pending_close_buffer() {
        let root = PathBuf::from("workspace");
        let conflict_path = root.join("src/conflict.rs");
        let active_path = root.join("src/active.rs");
        let mut app = app_for_test(root);
        let mut conflict_buffer =
            TextBuffer::from_text(7, Some(conflict_path), "conflict".to_owned());
        conflict_buffer.mark_dirty();
        let mut active_buffer = TextBuffer::from_text(9, Some(active_path), "active".to_owned());
        active_buffer.mark_dirty();
        app.buffers.push(conflict_buffer);
        app.buffers.push(active_buffer);
        app.save_conflict_buffer = Some(7);
        app.close_after_save = Some(7);
        app.dirty_close_buffer = Some(9);
        app.pending_close_buffers.extend([7, 8]);

        app.cancel_save_conflict(7);

        assert_eq!(app.save_conflict_buffer, None);
        assert_eq!(app.close_after_save, None);
        assert_eq!(app.dirty_close_buffer, Some(9));
        assert_eq!(app.pending_close_buffers, vec![7, 8]);
        assert_eq!(
            app.pending_close_buffers
                .iter()
                .filter(|pending| **pending == 7)
                .count(),
            1
        );
    }

    #[test]
    fn cancel_unrelated_save_conflict_preserves_close_request() {
        let root = PathBuf::from("workspace");
        let close_path = root.join("src/close.rs");
        let conflict_path = root.join("src/conflict.rs");
        let mut app = app_for_test(root);
        let mut close_buffer = TextBuffer::from_text(7, Some(close_path), "close".to_owned());
        close_buffer.mark_dirty();
        let mut conflict_buffer =
            TextBuffer::from_text(2, Some(conflict_path), "conflict".to_owned());
        conflict_buffer.mark_dirty();
        app.buffers.push(close_buffer);
        app.buffers.push(conflict_buffer);
        app.save_conflict_buffer = Some(2);
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(8);

        app.cancel_save_conflict(2);

        assert_eq!(app.save_conflict_buffer, None);
        assert_eq!(app.close_after_save, Some(7));
        assert_eq!(app.dirty_close_buffer, None);
        assert_eq!(app.pending_close_buffers, vec![8]);
        assert_eq!(app.status, "Save canceled");
    }

    #[test]
    fn missing_save_conflict_buffer_clears_matching_close_queue() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        app.save_conflict_buffer = Some(7);
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(8);

        app.clear_missing_save_conflict_buffer(7);

        assert_eq!(app.save_conflict_buffer, None);
        assert_eq!(app.close_after_save, None);
        assert_eq!(app.pending_close_buffers, vec![8]);
    }

    #[test]
    fn overwrite_save_conflict_preserves_close_request_and_cancels_reload_work() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "local".to_owned());
        buffer.mark_dirty();
        let version = buffer.version();
        app.buffers.push(buffer);
        app.save_conflict_buffer = Some(7);
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(8);
        let pending = PendingFileReload {
            request_id: 99,
            path: path.clone(),
            version,
            force_dirty: false,
        };
        app.in_flight_reloads.insert(7, pending.clone());
        app.queued_file_reloads.insert(
            7,
            QueuedFileReload {
                path,
                force_dirty: false,
            },
        );

        app.overwrite_save_conflict(7);

        assert_eq!(app.save_conflict_buffer, None);
        assert_eq!(app.close_after_save, Some(7));
        assert_eq!(app.pending_close_buffers, vec![8]);
        assert!(app.canceled_file_reloads.contains(&(7, pending)));
        assert!(!app.queued_file_reloads.contains_key(&7));
        assert!(app.in_flight_saves.contains(&7));
        assert!(app.status.starts_with("Saving "));
    }

    #[test]
    fn stale_save_conflict_from_close_request_resumes_save_and_preserves_queue() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text(7, Some(path), "local".to_owned());
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.save_conflict_buffer = Some(7);
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(8);

        app.render_save_conflict(&eframe::egui::Context::default());

        assert_eq!(app.save_conflict_buffer, None);
        assert_eq!(app.close_after_save, Some(7));
        assert_eq!(app.pending_close_buffers, vec![8]);
        assert!(app.in_flight_saves.contains(&7));
        assert!(app.status.starts_with("Saving "));
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
}
