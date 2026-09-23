#[cfg(test)]
use crate::path_display::{DISPLAY_PATH_LABEL_MAX_CHARS, sanitized_display_label};
use crate::{
    KuroyaApp,
    popup_buttons::{PopupButtonKind, popup_button},
};
use eframe::egui::{self, Align, Context, Key, RichText};
use kuroya_core::BufferId;

impl KuroyaApp {
    pub(crate) fn render_reload_from_disk(&mut self, ctx: &Context) {
        let Some((id, label)) = self.reload_guard_target() else {
            return;
        };

        let mut reload = false;
        let mut cancel = false;
        let mut window_open = true;

        egui::Window::new("Reload File")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 24.0,
            ))
            .open(&mut window_open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([500.0, 144.0])
            .show(ctx, |ui| {
                ui.label(RichText::new(label).strong());
                ui.label(reload_guard_body());

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    cancel = true;
                }

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if popup_button(ui, "Cancel", PopupButtonKind::Secondary).clicked() {
                        cancel = true;
                    }
                    if popup_button(ui, reload_guard_action_label(), PopupButtonKind::Danger)
                        .clicked()
                    {
                        reload = true;
                    }
                });
            });

        if cancel || !window_open {
            self.cancel_reload_guard(id);
        } else if reload {
            self.confirm_reload_guard(id);
        }
    }

    fn reload_guard_target(&mut self) -> Option<(BufferId, String)> {
        let id = self.dirty_reload_buffer?;
        let Some(buffer) = self.buffer(id) else {
            self.dirty_reload_buffer = None;
            return None;
        };
        if buffer.path().is_none() {
            self.dirty_reload_buffer = None;
            self.status = "Cannot reload an untitled buffer".to_owned();
            return None;
        }
        if !buffer.is_dirty() {
            self.dirty_reload_buffer = None;
            return None;
        }

        Some((id, self.buffer_label_for(buffer)))
    }

    fn cancel_reload_guard(&mut self, id: BufferId) {
        if self.dirty_reload_buffer == Some(id) {
            self.dirty_reload_buffer = None;
            self.status = "Reload canceled".to_owned();
        }
    }

    fn confirm_reload_guard(&mut self, id: BufferId) {
        if self.dirty_reload_buffer == Some(id) {
            if !self
                .buffer(id)
                .is_some_and(|buffer| buffer.path().is_some() && buffer.is_dirty())
            {
                self.dirty_reload_buffer = None;
                return;
            }
            self.dirty_reload_buffer = None;
            let advance_pending_close = self.clear_reload_guard_close_request(id);
            self.discard_and_reload_buffer_from_disk(id);
            if advance_pending_close {
                self.begin_next_pending_close();
            }
        }
    }

    fn clear_reload_guard_close_request(&mut self, id: BufferId) -> bool {
        if self.close_after_save == Some(id) {
            self.close_after_save = None;
            self.pending_close_buffers.retain(|pending| *pending != id);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
fn reload_guard_display_label(label: &str) -> String {
    sanitized_display_label(label, DISPLAY_PATH_LABEL_MAX_CHARS, "Untitled")
}

fn reload_guard_body() -> &'static str {
    "Discard local changes and reload the file from disk? This cannot be undone."
}

fn reload_guard_action_label() -> &'static str {
    "Discard and Reload"
}

#[cfg(test)]
mod tests {
    use super::{
        DISPLAY_PATH_LABEL_MAX_CHARS, reload_guard_action_label, reload_guard_body,
        reload_guard_display_label,
    };
    use crate::{
        KuroyaApp,
        app_startup_context::AppStartupContext,
        app_state::{PendingFileReload, PendingFormatOnSave, QueuedFileReload},
        terminal::TerminalPane,
    };
    use kuroya_core::{EditorSettings, TextBuffer, Workspace};
    use std::{path::PathBuf, time::Instant};
    use tokio::runtime::Runtime;

    #[test]
    fn reload_guard_target_drops_stale_buffer_ids() {
        let mut app = app_for_test(PathBuf::from("workspace"));
        app.dirty_reload_buffer = Some(9);

        assert_eq!(app.reload_guard_target(), None);
        assert_eq!(app.dirty_reload_buffer, None);
    }

    #[test]
    fn reload_guard_target_drops_untitled_buffers() {
        let mut app = app_for_test(PathBuf::from("workspace"));
        let mut buffer = TextBuffer::new_untitled(1);
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.dirty_reload_buffer = Some(1);

        assert_eq!(app.reload_guard_target(), None);
        assert_eq!(app.dirty_reload_buffer, None);
        assert_eq!(app.status, "Cannot reload an untitled buffer");
    }

    #[test]
    fn reload_guard_target_keeps_named_buffers() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text(1, Some(path), "changed".to_owned());
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.dirty_reload_buffer = Some(1);

        assert_eq!(app.reload_guard_target(), Some((1, "main.rs".to_owned())));
        assert_eq!(app.dirty_reload_buffer, Some(1));
    }

    #[test]
    fn reload_guard_target_drops_clean_named_buffers() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.buffers
            .push(TextBuffer::from_text(1, Some(path), "clean".to_owned()));
        app.dirty_reload_buffer = Some(1);
        app.status = "before".to_owned();

        assert_eq!(app.reload_guard_target(), None);
        assert_eq!(app.dirty_reload_buffer, None);
        assert_eq!(app.status, "before");
    }

    #[test]
    fn confirm_reload_guard_rejects_stale_clean_targets() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.buffers.push(TextBuffer::from_text(
            1,
            Some(path.clone()),
            "clean".to_owned(),
        ));
        app.dirty_reload_buffer = Some(1);
        app.pending_format_on_save.insert(
            1,
            PendingFormatOnSave {
                save_path: path.clone(),
                format_path: path,
                version: 1,
                request_id: 1,
            },
        );

        app.confirm_reload_guard(1);

        assert_eq!(app.dirty_reload_buffer, None);
        assert!(app.pending_format_on_save.contains_key(&1));
    }

    #[test]
    fn confirm_reload_guard_cancels_deferred_reload_work_before_forced_reload() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text(1, Some(path.clone()), "changed".to_owned());
        buffer.mark_dirty();
        let version = buffer.version();
        app.buffers.push(buffer);
        app.dirty_reload_buffer = Some(1);
        let pending = PendingFileReload {
            request_id: 9,
            path: path.clone(),
            version,
            force_dirty: false,
        };
        app.in_flight_reloads.insert(1, pending.clone());
        app.queued_file_reloads.insert(
            1,
            QueuedFileReload {
                path: path.clone(),
                force_dirty: false,
            },
        );

        app.confirm_reload_guard(1);

        assert_eq!(app.dirty_reload_buffer, None);
        assert!(app.canceled_file_reloads.contains(&(1, pending)));
        assert!(!app.queued_file_reloads.contains_key(&1));
        let forced_reload = app
            .in_flight_reloads
            .get(&1)
            .expect("forced reload should replace stale reload work");
        assert_eq!(forced_reload.path, path);
        assert_eq!(forced_reload.version, version);
        assert!(forced_reload.force_dirty);
    }

    #[test]
    fn confirm_reload_guard_clears_matching_save_conflict_and_matching_close_request() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let next_path = root.join("src/next.rs");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text(1, Some(path.clone()), "changed".to_owned());
        buffer.mark_dirty();
        let version = buffer.version();
        let mut next = TextBuffer::from_text(2, Some(next_path), "next".to_owned());
        next.mark_dirty();
        app.buffers.push(buffer);
        app.buffers.push(next);
        app.dirty_reload_buffer = Some(1);
        app.save_conflict_buffer = Some(1);
        app.close_after_save = Some(1);
        app.pending_close_buffers.push(2);

        app.confirm_reload_guard(1);

        assert_eq!(app.dirty_reload_buffer, None);
        assert_eq!(app.save_conflict_buffer, None);
        assert_eq!(app.close_after_save, None);
        assert_eq!(app.dirty_close_buffer, Some(2));
        assert!(app.pending_close_buffers.is_empty());
        let forced_reload = app
            .in_flight_reloads
            .get(&1)
            .expect("forced reload should start after confirming discard");
        assert_eq!(forced_reload.path, path);
        assert_eq!(forced_reload.version, version);
        assert!(forced_reload.force_dirty);
    }

    #[test]
    fn confirm_reload_guard_removes_duplicate_matching_pending_closes_only() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let next_path = root.join("src/next.rs");
        let last_path = root.join("src/last.rs");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text(1, Some(path), "changed".to_owned());
        buffer.mark_dirty();
        let mut next = TextBuffer::from_text(2, Some(next_path), "next".to_owned());
        next.mark_dirty();
        let mut last = TextBuffer::from_text(3, Some(last_path), "last".to_owned());
        last.mark_dirty();
        app.buffers.push(buffer);
        app.buffers.push(next);
        app.buffers.push(last);
        app.dirty_reload_buffer = Some(1);
        app.close_after_save = Some(1);
        app.pending_close_buffers.extend([1, 2, 1, 3]);

        app.confirm_reload_guard(1);

        assert_eq!(app.close_after_save, None);
        assert_eq!(app.dirty_close_buffer, Some(2));
        assert_eq!(app.pending_close_buffers, vec![3]);
        assert!(app.in_flight_reloads.contains_key(&1));
    }

    #[test]
    fn reload_guard_copy_uses_destructive_action_label() {
        assert_eq!(
            reload_guard_body(),
            "Discard local changes and reload the file from disk? This cannot be undone."
        );
        assert_eq!(reload_guard_action_label(), "Discard and Reload");
    }

    #[test]
    fn reload_guard_display_label_falls_back_for_blank_control_text() {
        assert_eq!(reload_guard_display_label("\n\u{202e}\u{0007}"), "Untitled");
    }

    #[test]
    fn reload_guard_target_sanitizes_controls_bidi_and_bounds_label() {
        let root = PathBuf::from("workspace");
        let path = root.join(format!(
            "alpha\n{}\u{202e}omega.rs",
            "very-long-component-".repeat(16)
        ));
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text(1, Some(path), "changed".to_owned());
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.dirty_reload_buffer = Some(1);

        let Some((id, label)) = app.reload_guard_target() else {
            panic!("expected reload guard target");
        };

        assert_eq!(id, 1);
        assert!(label.starts_with("alpha "));
        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));
        assert!(label.contains("..."));
        assert!(label.chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS);
        assert_eq!(app.dirty_reload_buffer, Some(1));
    }

    #[test]
    fn reload_guard_target_falls_back_for_blank_control_labels() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text(1, Some(path), "changed".to_owned());
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.virtual_buffer_labels
            .insert(1, "\n\u{202e}\u{0007}".to_owned());
        app.dirty_reload_buffer = Some(1);

        assert_eq!(app.reload_guard_target(), Some((1, "Untitled".to_owned())));
        assert_eq!(app.dirty_reload_buffer, Some(1));
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
