use crate::{
    KuroyaApp,
    file_reload_runtime::file_paths_match_lexically,
    path_display::display_path_label_cow,
    popup_buttons::{PopupButtonKind, popup_button},
    save_lifecycle::protected_preview_save_block_reason_for_buffer,
    ui_text::truncate_middle,
};
use eframe::egui::{self, Align, Context, Key, TextEdit};
use kuroya_core::{BufferId, normalize_child_path};
use std::path::PathBuf;

const SAVE_AS_STATUS_MAX_CHARS: usize = 240;

impl KuroyaApp {
    pub(crate) fn begin_save_as(&mut self, id: BufferId) {
        let Some((save_block, suggested_path)) = self.buffer(id).map(|buffer| {
            (
                protected_preview_save_block_reason_for_buffer(
                    buffer,
                    &self.lossy_decoded_buffers,
                    &self.binary_preview_buffers,
                ),
                buffer
                    .path()
                    .cloned()
                    .unwrap_or_else(|| PathBuf::from(format!("untitled-{id}.txt")))
                    .display()
                    .to_string(),
            )
        }) else {
            return;
        };
        if let Some(reason) = save_block {
            self.block_protected_preview_save(id, reason);
            return;
        }

        self.save_as_buffer = Some(id);
        self.save_as_path = suggested_path;
        self.save_as_open = true;
        self.status = "Choose a save path".to_owned();
    }

    fn resolve_save_as_path(&self) -> Result<ResolvedSaveAsPath, SaveAsPathError> {
        let raw = self.save_as_path.as_str();
        if raw.trim().is_empty() {
            return Err(SaveAsPathError::Empty);
        }

        let path = PathBuf::from(raw);
        let (save_path, guard_path) = if path.is_absolute() {
            (path.clone(), path)
        } else if let Some(path) = normalize_child_path(&self.workspace.root, &path) {
            (self.workspace.root.join(PathBuf::from(raw)), path)
        } else {
            return Err(SaveAsPathError::RelativePathEscapesWorkspace);
        };
        Ok(ResolvedSaveAsPath {
            save_path,
            guard_path,
        })
    }

    pub(crate) fn render_save_as(&mut self, ctx: &Context) {
        let mut save = false;
        let mut cancel = false;

        egui::Window::new("Save As")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 24.0,
            ))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([560.0, 150.0])
            .show(ctx, |ui| {
                ui.label("Path");
                let response = ui.add(
                    TextEdit::singleline(&mut self.save_as_path)
                        .hint_text("Relative or absolute file path")
                        .desired_width(f32::INFINITY),
                );
                response.request_focus();

                if ui.input(|input| input.key_pressed(Key::Enter)) {
                    save = true;
                }
                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    cancel = true;
                }

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if popup_button(ui, "Cancel", PopupButtonKind::Secondary).clicked() {
                        cancel = true;
                    }
                    if popup_button(ui, "Save", PopupButtonKind::Primary).clicked() {
                        save = true;
                    }
                });
            });

        if cancel {
            self.cancel_save_as();
        } else if save {
            self.confirm_save_as();
        }
    }

    fn confirm_save_as(&mut self) {
        let Some(id) = self.save_as_buffer else {
            self.clear_stale_save_as_target();
            return;
        };

        match self.resolve_save_as_path() {
            Ok(resolved) => {
                if self.buffer(id).is_none() {
                    self.clear_stale_save_as_target();
                    return;
                }

                if let Some(conflict) = self.save_as_target_conflict(id, &resolved.guard_path) {
                    let save_path_label = display_path_label_cow(&resolved.save_path);
                    match conflict {
                        SaveAsTargetConflict::ObservedExternalChange(target_id) => {
                            self.status = save_as_status(format!(
                                "Cannot save as {}; {} changed on disk",
                                save_path_label.as_ref(),
                                self.file_io_buffer_label(target_id)
                            ));
                        }
                        SaveAsTargetConflict::OpenBuffer(target_id) => {
                            self.status = save_as_status(format!(
                                "Cannot save as {}; {} is already open",
                                save_path_label.as_ref(),
                                self.file_io_buffer_label(target_id)
                            ));
                        }
                    }
                    return;
                }
                self.save_as_open = false;
                self.save_as_buffer = None;
                self.spawn_save_to(id, resolved.save_path);
            }
            Err(SaveAsPathError::Empty) => {
                self.status = "Save path is empty".to_owned();
            }
            Err(SaveAsPathError::RelativePathEscapesWorkspace) => {
                self.status = "Relative save path must stay inside the workspace".to_owned();
            }
        }
    }

    fn save_as_target_conflict(
        &self,
        source_id: BufferId,
        path: &std::path::Path,
    ) -> Option<SaveAsTargetConflict> {
        let mut first_open_target = None;
        for buffer in &self.buffers {
            let target_id = buffer.id();
            if target_id == source_id
                || !buffer
                    .path()
                    .is_some_and(|target_path| file_paths_match_lexically(target_path, path))
            {
                continue;
            }

            first_open_target.get_or_insert(target_id);
            if self.save_as_target_has_observed_external_change(target_id, path) {
                return Some(SaveAsTargetConflict::ObservedExternalChange(target_id));
            }
        }

        first_open_target.map(SaveAsTargetConflict::OpenBuffer)
    }

    fn save_as_target_has_observed_external_change(
        &self,
        target_id: BufferId,
        path: &std::path::Path,
    ) -> bool {
        self.external_change_buffers.contains(&target_id)
            || self
                .in_flight_reloads
                .get(&target_id)
                .is_some_and(|reload| {
                    !reload.force_dirty && file_paths_match_lexically(&reload.path, path)
                })
            || self
                .queued_file_reloads
                .get(&target_id)
                .is_some_and(|reload| {
                    !reload.force_dirty && file_paths_match_lexically(&reload.path, path)
                })
    }

    fn cancel_save_as(&mut self) {
        if let Some(id) = self.save_as_buffer
            && self.close_after_save == Some(id)
        {
            self.close_after_save = None;
            self.restore_dirty_close_guard_for_buffer(id);
        }
        self.save_as_open = false;
        self.save_as_buffer = None;
        self.status = "Save canceled".to_owned();
    }

    fn clear_stale_save_as_target(&mut self) {
        if let Some(id) = self.save_as_buffer
            && self.close_after_save == Some(id)
        {
            self.close_after_save = None;
            self.pending_close_buffers.retain(|pending| *pending != id);
        }
        self.save_as_open = false;
        self.save_as_buffer = None;
        self.status = "Save target is no longer open".to_owned();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedSaveAsPath {
    save_path: PathBuf,
    guard_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveAsPathError {
    Empty,
    RelativePathEscapesWorkspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveAsTargetConflict {
    ObservedExternalChange(BufferId),
    OpenBuffer(BufferId),
}

fn save_as_status(status: String) -> String {
    truncate_middle(&status, SAVE_AS_STATUS_MAX_CHARS)
}

#[cfg(test)]
mod tests {
    use super::SAVE_AS_STATUS_MAX_CHARS;
    use crate::{
        KuroyaApp,
        app_startup_context::AppStartupContext,
        app_state::{PendingFileReload, QueuedFileReload},
        terminal::TerminalPane,
    };
    use kuroya_core::{EditorSettings, TextBuffer, Workspace};
    use std::{path::PathBuf, time::Instant};
    use tokio::runtime::Runtime;

    #[test]
    fn begin_save_as_blocks_read_only_untitled_buffers() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::new_untitled(7);
        buffer.mark_dirty();
        buffer.set_read_only(true);
        app.buffers.push(buffer);

        app.begin_save_as(7);

        assert!(!app.save_as_open);
        assert_eq!(app.save_as_buffer, None);
        assert_eq!(app.status, "Cannot save Untitled; buffer is read-only");
    }

    #[test]
    fn save_and_close_untitled_opens_save_as_and_preserves_close_request() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root.clone());
        let mut buffer = TextBuffer::new_untitled(7);
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(8);

        app.spawn_save(7);

        assert!(app.save_as_open);
        assert_eq!(app.save_as_buffer, Some(7));
        assert_eq!(app.save_as_path, "untitled-7.txt");
        assert_eq!(app.close_after_save, Some(7));
        assert_eq!(app.pending_close_buffers, vec![8]);
        assert_eq!(app.dirty_close_buffer, None);
        assert!(!app.in_flight_saves.contains(&7));
        assert_eq!(app.status, "Choose a save path");
    }

    #[test]
    fn save_as_default_untitled_path_resolves_once_for_relative_workspace_root() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root.clone());
        let mut buffer = TextBuffer::new_untitled(7);
        buffer.mark_dirty();
        app.buffers.push(buffer);

        app.begin_save_as(7);
        app.in_flight_saves.insert(7);
        app.confirm_save_as();

        assert!(!app.save_as_open);
        assert_eq!(app.save_as_buffer, None);
        assert_eq!(
            app.queued_save_paths.get(&7),
            Some(&root.join("untitled-7.txt"))
        );
        assert_ne!(
            app.queued_save_paths.get(&7),
            Some(&root.join(root.join("untitled-7.txt")))
        );
    }

    #[test]
    fn save_as_preserves_raw_relative_target_path_for_save_request() {
        let root = PathBuf::from("workspace");
        let raw_relative = PathBuf::from("src").join("..").join("saved.rs");
        let mut app = app_for_test(root.clone());
        let mut buffer = TextBuffer::new_untitled(7);
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.in_flight_saves.insert(7);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.save_as_path = raw_relative.display().to_string();

        app.confirm_save_as();

        assert!(!app.save_as_open);
        assert_eq!(app.save_as_buffer, None);
        assert_eq!(
            app.queued_save_paths.get(&7),
            Some(&root.join(raw_relative))
        );
    }

    #[test]
    fn save_as_preserves_leading_and_trailing_spaces_in_raw_path() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root.clone());
        let mut buffer = TextBuffer::new_untitled(7);
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.in_flight_saves.insert(7);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.save_as_path = " src/spaced.rs ".to_owned();

        app.confirm_save_as();

        assert_eq!(
            app.queued_save_paths.get(&7),
            Some(&root.join(" src/spaced.rs "))
        );
    }

    #[test]
    fn stale_save_as_target_closes_dialog_without_spawning_save() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.save_as_path = "src/main.rs".to_owned();
        app.close_after_save = Some(7);
        app.pending_close_buffers.extend([7, 8]);

        app.confirm_save_as();

        assert!(!app.save_as_open);
        assert_eq!(app.save_as_buffer, None);
        assert_eq!(app.close_after_save, None);
        assert_eq!(app.pending_close_buffers, vec![8]);
        assert!(!app.in_flight_saves.contains(&7));
        assert_eq!(app.status, "Save target is no longer open");
    }

    #[test]
    fn save_as_confirm_from_close_request_starts_save_and_keeps_close_after_save() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::new_untitled(7);
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.save_as_path = "src/main.rs".to_owned();
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(8);

        app.confirm_save_as();

        assert!(!app.save_as_open);
        assert_eq!(app.save_as_buffer, None);
        assert_eq!(app.close_after_save, Some(7));
        assert_eq!(app.pending_close_buffers, vec![8]);
        assert!(app.in_flight_saves.contains(&7));
        assert!(!app.queued_save_paths.contains_key(&7));
        assert!(app.status.starts_with("Saving "));
        assert!(app.status.contains("main.rs"));
    }

    #[test]
    fn save_as_confirm_open_clean_target_keeps_dialog_and_close_request() {
        let root = std::env::current_dir()
            .expect("current directory")
            .join("workspace");
        let target_path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut source = TextBuffer::new_untitled(7);
        source.mark_dirty();
        let target = TextBuffer::from_text(8, Some(target_path.clone()), "target".to_owned());
        app.buffers.push(source);
        app.buffers.push(target);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.save_as_path = target_path.display().to_string();
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(9);

        app.confirm_save_as();

        assert!(app.save_as_open);
        assert_eq!(app.save_as_buffer, Some(7));
        assert_eq!(app.save_conflict_buffer, None);
        assert_eq!(app.close_after_save, Some(7));
        assert_eq!(app.pending_close_buffers, vec![9]);
        assert!(!app.in_flight_saves.contains(&7));
        assert!(app.status.starts_with("Cannot save as "));
        assert!(app.status.ends_with(" is already open"));
    }

    #[test]
    fn save_as_confirm_open_dirty_target_keeps_dialog_and_close_request() {
        let root = std::env::current_dir()
            .expect("current directory")
            .join("workspace");
        let target_path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut source = TextBuffer::new_untitled(7);
        source.mark_dirty();
        let mut target = TextBuffer::from_text(8, Some(target_path.clone()), "target".to_owned());
        target.mark_dirty();
        app.buffers.push(source);
        app.buffers.push(target);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.save_as_path = target_path.display().to_string();
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(9);

        app.confirm_save_as();

        assert!(app.save_as_open);
        assert_eq!(app.save_as_buffer, Some(7));
        assert_eq!(app.save_conflict_buffer, None);
        assert_eq!(app.close_after_save, Some(7));
        assert_eq!(app.pending_close_buffers, vec![9]);
        assert!(!app.in_flight_saves.contains(&7));
        assert!(app.buffer(8).is_some_and(TextBuffer::is_dirty));
        assert!(app.status.starts_with("Cannot save as "));
        assert!(app.status.ends_with(" is already open"));
    }

    #[test]
    fn save_as_confirm_equivalent_open_target_keeps_dialog_and_close_request() {
        let root = std::env::current_dir()
            .expect("current directory")
            .join("workspace");
        let target_path = root.join("src/main.rs");
        let equivalent_path = root.join("src").join("..").join("src/main.rs");
        let mut app = app_for_test(root);
        let mut source = TextBuffer::new_untitled(7);
        source.mark_dirty();
        let target = TextBuffer::from_text(8, Some(target_path), "target".to_owned());
        app.buffers.push(source);
        app.buffers.push(target);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.save_as_path = equivalent_path.display().to_string();
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(9);

        app.confirm_save_as();

        assert!(app.save_as_open);
        assert_eq!(app.save_as_buffer, Some(7));
        assert_eq!(app.close_after_save, Some(7));
        assert_eq!(app.pending_close_buffers, vec![9]);
        assert!(!app.in_flight_saves.contains(&7));
        assert!(app.status.starts_with("Cannot save as "));
        assert!(app.status.ends_with(" is already open"));
    }

    #[test]
    fn save_as_confirm_relative_equivalent_open_target_keeps_dialog_and_close_request() {
        let root = std::env::current_dir()
            .expect("current directory")
            .join("workspace");
        let target_path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut source = TextBuffer::new_untitled(7);
        source.mark_dirty();
        let target = TextBuffer::from_text(8, Some(target_path), "target".to_owned());
        app.buffers.push(source);
        app.buffers.push(target);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.save_as_path = "src/../src/main.rs".to_owned();
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(9);

        app.confirm_save_as();

        assert!(app.save_as_open);
        assert_eq!(app.save_as_buffer, Some(7));
        assert_eq!(app.close_after_save, Some(7));
        assert_eq!(app.pending_close_buffers, vec![9]);
        assert!(!app.in_flight_saves.contains(&7));
        assert!(app.status.starts_with("Cannot save as "));
        assert!(app.status.ends_with(" is already open"));
    }

    #[test]
    fn save_as_empty_path_from_close_request_keeps_dialog_and_close_request() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::new_untitled(7);
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.save_as_path = "  ".to_owned();
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(8);

        app.confirm_save_as();

        assert!(app.save_as_open);
        assert_eq!(app.save_as_buffer, Some(7));
        assert_eq!(app.close_after_save, Some(7));
        assert_eq!(app.pending_close_buffers, vec![8]);
        assert_eq!(app.dirty_close_buffer, None);
        assert!(!app.in_flight_saves.contains(&7));
        assert!(!app.queued_save_paths.contains_key(&7));
        assert_eq!(app.status, "Save path is empty");
    }

    #[test]
    fn save_as_relative_parent_escape_keeps_dialog_and_close_request() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::new_untitled(7);
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.save_as_path = "../outside.rs".to_owned();
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(8);

        app.confirm_save_as();

        assert!(app.save_as_open);
        assert_eq!(app.save_as_buffer, Some(7));
        assert_eq!(app.close_after_save, Some(7));
        assert_eq!(app.pending_close_buffers, vec![8]);
        assert_eq!(app.dirty_close_buffer, None);
        assert!(!app.in_flight_saves.contains(&7));
        assert!(!app.queued_save_paths.contains_key(&7));
        assert_eq!(
            app.status,
            "Relative save path must stay inside the workspace"
        );
    }

    #[test]
    fn save_as_confirm_current_path_conflict_keeps_close_request() {
        let root = std::env::current_dir()
            .expect("current directory")
            .join("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "dirty".to_owned());
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.external_change_buffers.insert(7);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.save_as_path = path.display().to_string();
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(8);

        app.confirm_save_as();

        assert!(!app.save_as_open);
        assert_eq!(app.save_as_buffer, None);
        assert_eq!(app.save_conflict_buffer, Some(7));
        assert_eq!(app.close_after_save, Some(7));
        assert_eq!(app.pending_close_buffers, vec![8]);
        assert!(!app.in_flight_saves.contains(&7));
        assert!(app.status.ends_with(" changed on disk"));
    }

    #[test]
    fn save_as_confirm_target_external_change_keeps_dialog_and_close_request() {
        let root = std::env::current_dir()
            .expect("current directory")
            .join("workspace");
        let target_path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut source = TextBuffer::new_untitled(7);
        source.mark_dirty();
        let target = TextBuffer::from_text(8, Some(target_path.clone()), "target".to_owned());
        app.buffers.push(source);
        app.buffers.push(target);
        app.external_change_buffers.insert(8);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.save_as_path = target_path.display().to_string();
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(9);

        app.confirm_save_as();

        assert!(app.save_as_open);
        assert_eq!(app.save_as_buffer, Some(7));
        assert_eq!(app.save_conflict_buffer, None);
        assert_eq!(app.close_after_save, Some(7));
        assert_eq!(app.pending_close_buffers, vec![9]);
        assert!(!app.in_flight_saves.contains(&7));
        assert!(app.external_change_buffers.contains(&8));
        assert!(app.status.starts_with("Cannot save as "));
        assert!(app.status.ends_with(" changed on disk"));
    }

    #[test]
    fn save_as_confirm_target_queued_clean_reload_keeps_dialog_and_close_request() {
        let root = std::env::current_dir()
            .expect("current directory")
            .join("workspace");
        let target_path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut source = TextBuffer::new_untitled(7);
        source.mark_dirty();
        let target = TextBuffer::from_text(8, Some(target_path.clone()), "target".to_owned());
        app.buffers.push(source);
        app.buffers.push(target);
        app.queued_file_reloads.insert(
            8,
            QueuedFileReload {
                path: target_path.clone(),
                force_dirty: false,
            },
        );
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.save_as_path = target_path.display().to_string();
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(9);

        app.confirm_save_as();

        assert!(app.save_as_open);
        assert_eq!(app.save_as_buffer, Some(7));
        assert_eq!(app.save_conflict_buffer, None);
        assert_eq!(app.close_after_save, Some(7));
        assert_eq!(app.pending_close_buffers, vec![9]);
        assert!(!app.in_flight_saves.contains(&7));
        assert!(app.queued_file_reloads.contains_key(&8));
        assert!(app.status.starts_with("Cannot save as "));
        assert!(app.status.ends_with(" changed on disk"));
    }

    #[test]
    fn save_as_confirm_target_pending_clean_reload_keeps_dialog_and_close_request() {
        let root = std::env::current_dir()
            .expect("current directory")
            .join("workspace");
        let target_path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut source = TextBuffer::new_untitled(7);
        source.mark_dirty();
        let target = TextBuffer::from_text(8, Some(target_path.clone()), "target".to_owned());
        let target_version = target.version();
        app.buffers.push(source);
        app.buffers.push(target);
        app.in_flight_reloads.insert(
            8,
            PendingFileReload {
                request_id: 1,
                path: target_path.clone(),
                version: target_version,
                force_dirty: false,
            },
        );
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.save_as_path = target_path.display().to_string();
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(9);

        app.confirm_save_as();

        assert!(app.save_as_open);
        assert_eq!(app.save_as_buffer, Some(7));
        assert_eq!(app.save_conflict_buffer, None);
        assert_eq!(app.close_after_save, Some(7));
        assert_eq!(app.pending_close_buffers, vec![9]);
        assert!(!app.in_flight_saves.contains(&7));
        assert!(app.in_flight_reloads.contains_key(&8));
        assert!(app.status.starts_with("Cannot save as "));
        assert!(app.status.ends_with(" changed on disk"));
    }

    #[test]
    fn save_as_confirm_duplicate_target_external_change_scans_all_matching_buffers() {
        let root = std::env::current_dir()
            .expect("current directory")
            .join("workspace");
        let target_path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut source = TextBuffer::new_untitled(7);
        source.mark_dirty();
        let clean_target = TextBuffer::from_text(8, Some(target_path.clone()), "clean".to_owned());
        let stale_target = TextBuffer::from_text(9, Some(target_path.clone()), "stale".to_owned());
        app.buffers.push(source);
        app.buffers.push(clean_target);
        app.buffers.push(stale_target);
        app.external_change_buffers.insert(9);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.save_as_path = target_path.display().to_string();
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(10);

        app.confirm_save_as();

        assert!(app.save_as_open);
        assert_eq!(app.save_as_buffer, Some(7));
        assert_eq!(app.save_conflict_buffer, None);
        assert_eq!(app.close_after_save, Some(7));
        assert_eq!(app.pending_close_buffers, vec![10]);
        assert!(!app.in_flight_saves.contains(&7));
        assert!(app.external_change_buffers.contains(&9));
        assert!(app.status.starts_with("Cannot save as "));
        assert!(app.status.ends_with(" changed on disk"));
    }

    #[test]
    fn save_as_target_conflict_status_is_bounded_and_display_safe() {
        let root = PathBuf::from("workspace");
        let long_name = format!(
            "bad{}.rs",
            "very-long-component-".repeat(SAVE_AS_STATUS_MAX_CHARS)
        );
        let target_path = root.join(&long_name);
        let mut app = app_for_test(root);
        let mut source = TextBuffer::new_untitled(7);
        source.mark_dirty();
        let target = TextBuffer::from_text(8, Some(target_path.clone()), "target".to_owned());
        app.buffers.push(source);
        app.buffers.push(target);
        app.external_change_buffers.insert(8);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.save_as_path = long_name;

        app.confirm_save_as();

        assert!(app.status.contains("Cannot save as"));
        assert!(!app.status.contains('\n'));
        assert!(!app.status.contains('\u{202e}'));
        assert!(app.status.chars().count() <= SAVE_AS_STATUS_MAX_CHARS);
    }

    #[test]
    fn save_as_confirm_from_close_request_blocks_read_only_transition() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::new_untitled(7);
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.save_as_path = "src/main.rs".to_owned();
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(8);
        app.buffer_mut(7)
            .expect("buffer should exist")
            .set_read_only(true);

        app.confirm_save_as();

        assert!(!app.save_as_open);
        assert_eq!(app.save_as_buffer, None);
        assert_eq!(app.close_after_save, None);
        assert_eq!(app.dirty_close_buffer, Some(7));
        assert_eq!(app.pending_close_buffers, vec![8]);
        assert!(!app.in_flight_saves.contains(&7));
        assert!(app.status.contains("buffer is read-only"));
    }

    #[test]
    fn save_as_confirm_from_close_request_blocks_binary_preview_transition() {
        let root = PathBuf::from("workspace");
        let current_path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text(7, Some(current_path), "dirty".to_owned());
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.save_as_path = "src/copy.rs".to_owned();
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(8);
        app.binary_preview_buffers.insert(7);

        app.confirm_save_as();

        assert!(!app.save_as_open);
        assert_eq!(app.save_as_buffer, None);
        assert_eq!(app.close_after_save, None);
        assert_eq!(app.dirty_close_buffer, Some(7));
        assert_eq!(app.pending_close_buffers, vec![8]);
        assert!(!app.in_flight_saves.contains(&7));
        assert!(app.status.contains("binary previews are read-only"));
    }

    #[test]
    fn save_as_confirm_from_close_request_blocks_lossy_preview_transition() {
        let root = PathBuf::from("workspace");
        let current_path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text(7, Some(current_path), "dirty".to_owned());
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.save_as_path = "src/copy.rs".to_owned();
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(8);
        app.lossy_decoded_buffers.insert(7);

        app.confirm_save_as();

        assert!(!app.save_as_open);
        assert_eq!(app.save_as_buffer, None);
        assert_eq!(app.close_after_save, None);
        assert_eq!(app.dirty_close_buffer, Some(7));
        assert_eq!(app.pending_close_buffers, vec![8]);
        assert!(!app.in_flight_saves.contains(&7));
        assert!(app.status.contains("replacement characters"));
    }

    #[test]
    fn save_as_cancel_restores_dirty_close_guard_and_preserves_pending_queue() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::new_untitled(7);
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(8);

        app.cancel_save_as();

        assert!(!app.save_as_open);
        assert_eq!(app.save_as_buffer, None);
        assert_eq!(app.close_after_save, None);
        assert_eq!(app.dirty_close_buffer, Some(7));
        assert_eq!(app.pending_close_buffers, vec![8]);
        assert_eq!(app.status, "Save canceled");
    }

    #[test]
    fn save_as_cancel_does_not_replace_unrelated_dirty_close_guard() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/other.rs");
        let mut app = app_for_test(root);
        let mut save_as_buffer = TextBuffer::new_untitled(7);
        save_as_buffer.mark_dirty();
        let mut guarded_buffer = TextBuffer::from_text(9, Some(path), "dirty".to_owned());
        guarded_buffer.mark_dirty();
        app.buffers.push(save_as_buffer);
        app.buffers.push(guarded_buffer);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.close_after_save = Some(7);
        app.dirty_close_buffer = Some(9);
        app.pending_close_buffers.push(8);

        app.cancel_save_as();

        assert!(!app.save_as_open);
        assert_eq!(app.save_as_buffer, None);
        assert_eq!(app.close_after_save, None);
        assert_eq!(app.dirty_close_buffer, Some(9));
        assert_eq!(app.pending_close_buffers, vec![7, 8]);
        assert_eq!(app.status, "Save canceled");
    }

    #[test]
    fn save_as_cancel_clean_close_target_preserves_pending_queue() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        let buffer = TextBuffer::new_untitled(7);
        app.buffers.push(buffer);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(8);

        app.cancel_save_as();

        assert!(!app.save_as_open);
        assert_eq!(app.save_as_buffer, None);
        assert_eq!(app.close_after_save, None);
        assert_eq!(app.dirty_close_buffer, None);
        assert_eq!(app.pending_close_buffers, vec![8]);
        assert_eq!(app.status, "Save canceled");
    }

    #[test]
    fn save_as_cancel_preserves_unrelated_close_after_save() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::new_untitled(7);
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.close_after_save = Some(9);
        app.pending_close_buffers.push(8);

        app.cancel_save_as();

        assert!(!app.save_as_open);
        assert_eq!(app.save_as_buffer, None);
        assert_eq!(app.close_after_save, Some(9));
        assert_eq!(app.dirty_close_buffer, None);
        assert_eq!(app.pending_close_buffers, vec![8]);
        assert_eq!(app.status, "Save canceled");
    }

    #[test]
    fn save_as_cancel_does_not_duplicate_pending_close_buffer() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/other.rs");
        let mut app = app_for_test(root);
        let mut save_as_buffer = TextBuffer::new_untitled(7);
        save_as_buffer.mark_dirty();
        let mut guarded_buffer = TextBuffer::from_text(9, Some(path), "dirty".to_owned());
        guarded_buffer.mark_dirty();
        app.buffers.push(save_as_buffer);
        app.buffers.push(guarded_buffer);
        app.save_as_open = true;
        app.save_as_buffer = Some(7);
        app.close_after_save = Some(7);
        app.dirty_close_buffer = Some(9);
        app.pending_close_buffers.extend([7, 8]);

        app.cancel_save_as();

        assert!(!app.save_as_open);
        assert_eq!(app.save_as_buffer, None);
        assert_eq!(app.close_after_save, None);
        assert_eq!(app.dirty_close_buffer, Some(9));
        assert_eq!(app.pending_close_buffers, vec![7, 8]);
        assert_eq!(app.status, "Save canceled");
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
