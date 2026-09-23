use crate::{
    KuroyaApp,
    buffer_close_lifecycle::buffer_close_status_label,
    fs_watcher::note_app_write,
    path_display::{display_error_label_cow, display_path_label_cow},
    save_lifecycle::{
        FinishedSaveRequest, apply_save_completion, finish_current_save_request,
        plan_lsp_save_sync, save_completion_status,
    },
    ui_events::UiEvent,
    ui_text::truncate_middle,
    workspace_state::paths_match_lexically,
};
use kuroya_core::{BufferId, TextBuffer};

const FILE_SAVE_EVENT_STATUS_MAX_CHARS: usize = 240;

impl KuroyaApp {
    pub(crate) fn handle_file_save_event(&mut self, event: UiEvent) {
        match event {
            UiEvent::FileSaved {
                root,
                generation,
                id,
                path,
                version,
            } => {
                if !self.workspace_event_is_current(&root, generation) {
                    return;
                }
                let queued_save_path = match finish_current_save_request(
                    id,
                    &mut self.in_flight_saves,
                    &mut self.queued_save_paths,
                ) {
                    FinishedSaveRequest::Current { queued_path } => queued_path,
                    FinishedSaveRequest::Stale => return,
                };
                let close_requested = self.close_after_save == Some(id);
                let path_changed = self.buffer(id).is_some_and(|buffer| match buffer.path() {
                    Some(buffer_path) => !paths_match_lexically(buffer_path, &path),
                    None => true,
                });
                if path_changed && let Some(target_id) = self.open_save_completion_target(id, &path)
                {
                    self.mark_buffer_changed_on_disk(target_id);
                    if close_requested {
                        self.restore_dirty_close_guard_after_close_save(id);
                    }
                    self.set_status_with_toast(file_save_event_status(format!(
                        "Saved {}; {} is already open",
                        display_path_label_cow(&path),
                        self.file_io_buffer_label(target_id)
                    )));
                    if let Some(queued_path) = queued_save_path {
                        self.resume_queued_save_after_current_event(id, queued_path);
                    }
                    self.advance_save_dependents_after_current_event();
                    return;
                }
                let old_diagnostic_path = self
                    .buffer(id)
                    .map(|buffer| self.diagnostic_path_for(buffer));
                note_app_write(&path);
                let still_dirty = self
                    .buffer_mut(id)
                    .is_some_and(|buffer| apply_save_completion(buffer, path.clone(), version));
                let had_pending_lsp_sync = self.pending_language_sync.remove(&id).is_some();
                let preserve_conflict_status = self
                    .save_conflict_buffer
                    .is_some_and(|conflict| conflict != id);
                if !still_dirty {
                    self.clear_buffer_changed_on_disk(id);
                }
                self.lossy_decoded_buffers.remove(&id);
                self.binary_preview_buffers.remove(&id);
                self.image_preview_buffers.remove(&id);
                self.dirty_reload_buffer = self.dirty_reload_buffer.filter(|dirty| *dirty != id);
                self.save_conflict_buffer =
                    self.save_conflict_buffer.filter(|conflict| *conflict != id);
                if let Some(old_diagnostic_path) = old_diagnostic_path {
                    if !paths_match_lexically(&old_diagnostic_path, &path) {
                        self.diagnostics.replace(old_diagnostic_path, Vec::new());
                    }
                }
                self.diff_cache.remove(&id);
                self.spawn_diagnostics_for(id);
                let lsp_sync = plan_lsp_save_sync(path_changed, had_pending_lsp_sync, still_dirty);
                if lsp_sync.open {
                    self.notify_lsp_open(id);
                } else if lsp_sync.change {
                    self.notify_lsp_change(id);
                }
                if lsp_sync.reschedule {
                    self.schedule_language_sync(id);
                }
                if lsp_sync.save {
                    self.notify_lsp_save(id);
                }
                self.spawn_git_refresh_for_saved_path(&path);
                self.refresh_local_history_browser_after_save(&path);
                if !preserve_conflict_status {
                    self.set_status_with_toast(save_completion_status(&path, still_dirty));
                }
                if let Some(queued_path) = queued_save_path {
                    self.resume_queued_save_after_current_event(id, queued_path);
                } else if close_requested && !still_dirty {
                    let close_status = (!preserve_conflict_status).then(|| {
                        self.buffer(id)
                            .map(|buffer| {
                                saved_and_closed_status(&buffer_close_status_label(
                                    &self.buffer_label_for(buffer),
                                ))
                            })
                            .unwrap_or_else(|| saved_and_closed_status("Untitled"))
                    });
                    self.close_after_save = None;
                    self.force_close_buffer(id);
                    self.begin_next_pending_close();
                    if self.dirty_close_buffer.is_none()
                        && let Some(close_status) = close_status
                        && !preserve_conflict_status
                    {
                        self.set_status_with_toast(close_status);
                    }
                } else if close_requested {
                    self.restore_dirty_close_guard_after_close_save(id);
                }
                self.advance_save_dependents_after_current_event();
            }
            UiEvent::FileSaveFailed {
                root,
                generation,
                id,
                path,
                error,
            } => {
                if !self.workspace_event_is_current(&root, generation) {
                    return;
                }
                let queued_save_path = match finish_current_save_request(
                    id,
                    &mut self.in_flight_saves,
                    &mut self.queued_save_paths,
                ) {
                    FinishedSaveRequest::Current { queued_path } => queued_path,
                    FinishedSaveRequest::Stale => return,
                };
                if let Some(queued_path) = queued_save_path {
                    self.set_status_with_toast(file_save_event_status(format!(
                        "Retrying queued save after {} failed",
                        display_path_label_cow(&path)
                    )));
                    self.resume_queued_save_after_current_event(id, queued_path);
                } else {
                    if self.close_after_save == Some(id) {
                        self.restore_dirty_close_guard_after_close_save(id);
                    }
                    self.set_status_with_toast(file_save_event_status(format!(
                        "Could not save {}: {}",
                        display_path_label_cow(&path),
                        display_error_label_cow(&error)
                    )));
                }
                self.pause_pending_workspace_switch_after_save_failure(id);
                self.pause_pending_exit_after_save_failure(id);
                self.pause_pending_source_control_commit_after_save_failure(id);
                self.pause_pending_source_control_stash_after_save_failure(id);
            }
            _ => {}
        }
    }

    fn resume_queued_save_after_current_event(
        &mut self,
        id: BufferId,
        queued_path: std::path::PathBuf,
    ) {
        if self.save_needs_observed_external_change_confirmation(id, &queued_path) {
            self.save_conflict_buffer.get_or_insert(id);
            self.set_active_buffer(id);
            self.set_status_with_toast(format!(
                "{} changed on disk",
                self.file_io_buffer_label(id)
            ));
            return;
        }

        self.spawn_save_to(id, queued_path);
    }

    fn advance_save_dependents_after_current_event(&mut self) {
        self.advance_pending_workspace_switch_after_save();
        self.advance_pending_exit_after_save();
        self.advance_pending_source_control_commit_after_save();
        self.advance_pending_source_control_stash_after_save();
    }

    fn open_save_completion_target(
        &self,
        source_id: BufferId,
        path: &std::path::Path,
    ) -> Option<BufferId> {
        self.buffers
            .iter()
            .filter(|buffer| buffer.id() != source_id)
            .find(|buffer| {
                buffer
                    .path()
                    .is_some_and(|buffer_path| paths_match_lexically(buffer_path, path))
            })
            .map(TextBuffer::id)
    }

    fn restore_dirty_close_guard_after_close_save(&mut self, id: BufferId) {
        if self.close_after_save == Some(id) {
            self.close_after_save = None;
        }
        self.restore_dirty_close_guard_for_buffer(id);
    }
}

fn saved_and_closed_status(label: &str) -> String {
    format!("Saved and closed {label}")
}

fn file_save_event_status(status: String) -> String {
    truncate_middle(&status, FILE_SAVE_EVENT_STATUS_MAX_CHARS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app_startup_context::AppStartupContext,
        app_state::{PendingFileReload, QueuedFileReload},
        lsp_client::LspClientHandle,
        terminal::TerminalPane,
    };
    use kuroya_core::{Diagnostic, DiagnosticSeverity, EditorSettings, TextBuffer, Workspace};
    use std::{path::PathBuf, time::Instant};
    use tokio::runtime::Runtime;

    #[test]
    fn stale_file_save_events_are_ignored_after_workspace_reset() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root.clone());
        let stale_generation = app.workspace_event_generation;
        app.status = "before".to_owned();
        app.reset_open_workspace_state();

        app.handle_file_save_event(UiEvent::FileSaved {
            root,
            generation: stale_generation,
            id: 1,
            path,
            version: 1,
        });

        assert_eq!(app.status, "before");
    }

    #[test]
    fn same_path_save_completion_records_recent_app_write_marker() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/kuroya-save-marker/main.rs");
        let mut app = app_for_test(root.clone());
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "dirty".to_owned());
        buffer.mark_dirty();
        let version = buffer.version();
        app.buffers.push(buffer);
        app.in_flight_saves.insert(7);

        assert!(!crate::fs_watcher::is_recent_app_write(&path));
        app.handle_file_save_event(UiEvent::FileSaved {
            root,
            generation: app.workspace_event_generation,
            id: 7,
            path: path.clone(),
            version,
        });

        assert!(!app.buffer(7).unwrap().is_dirty());
        assert!(crate::fs_watcher::is_recent_app_write(&path));
        assert!(!crate::fs_watcher::is_recent_app_write(
            &path.with_file_name("lib.rs")
        ));
    }

    #[test]
    fn equivalent_root_file_saved_event_is_applied() {
        let root = PathBuf::from("workspace");
        let event_root = root.join("src").join("..");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "dirty".to_owned());
        buffer.mark_dirty();
        let version = buffer.version();
        app.buffers.push(buffer);
        app.in_flight_saves.insert(7);

        app.handle_file_save_event(UiEvent::FileSaved {
            root: event_root,
            generation: app.workspace_event_generation,
            id: 7,
            path: path.clone(),
            version,
        });

        assert!(!app.buffer(7).unwrap().is_dirty());
        assert!(!app.in_flight_saves.contains(&7));
        assert!(app.status.contains("Saved"));
    }

    #[test]
    fn file_saved_equivalent_path_does_not_treat_path_as_changed() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let equivalent_path = root.join("src").join(".").join("main.rs");
        let mut app = app_for_test(root.clone());
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "dirty".to_owned());
        buffer.mark_dirty();
        let version = buffer.version();
        app.buffers.push(buffer);
        app.in_flight_saves.insert(7);
        app.pending_language_sync.insert(7, Instant::now());
        app.lsp_clients
            .insert("rust".to_owned(), LspClientHandle::accepting_for_test());
        app.diagnostics
            .replace(path.clone(), vec![diagnostic(&path, "before save")]);

        app.handle_file_save_event(UiEvent::FileSaved {
            root,
            generation: app.workspace_event_generation,
            id: 7,
            path: equivalent_path,
            version,
        });

        let methods = app
            .lsp_trace
            .iter()
            .map(|entry| entry.method.as_str())
            .collect::<Vec<_>>();
        assert!(methods.contains(&"textDocument/didChange"));
        assert!(methods.contains(&"textDocument/didSave"));
        assert!(!methods.contains(&"textDocument/didOpen"));
        assert_eq!(app.diagnostics.for_path(&path).len(), 1);
    }

    #[test]
    fn save_success_without_in_flight_request_is_ignored() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root.clone());
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "dirty".to_owned());
        buffer.mark_dirty();
        let version = buffer.version();
        app.buffers.push(buffer);
        app.external_change_buffers.insert(7);
        app.pending_language_sync.insert(7, Instant::now());
        app.status = "before".to_owned();

        app.handle_file_save_event(UiEvent::FileSaved {
            root,
            generation: app.workspace_event_generation,
            id: 7,
            path,
            version,
        });

        assert_eq!(app.status, "before");
        assert!(app.external_change_buffers.contains(&7));
        assert!(app.pending_language_sync.contains_key(&7));
        assert!(app.buffer(7).is_some_and(TextBuffer::is_dirty));
    }

    #[test]
    fn save_failure_without_in_flight_request_is_ignored() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root.clone());
        app.status = "before".to_owned();

        app.handle_file_save_event(UiEvent::FileSaveFailed {
            root,
            generation: app.workspace_event_generation,
            id: 7,
            path,
            error: "disk full".to_owned(),
        });

        assert_eq!(app.status, "before");
    }

    #[test]
    fn save_failure_status_sanitizes_path_and_error_text() {
        let root = PathBuf::from("workspace");
        let path = root.join(format!("bad\n{}\u{202e}.rs", "very-long-name-".repeat(16)));
        let mut app = app_for_test(root.clone());
        app.in_flight_saves.insert(7);

        app.handle_file_save_event(UiEvent::FileSaveFailed {
            root,
            generation: app.workspace_event_generation,
            id: 7,
            path,
            error: format!("disk\nfull \u{202e}{}", "x".repeat(256)),
        });

        assert!(app.status.starts_with("Could not save "));
        assert!(!app.status.contains('\n'));
        assert!(!app.status.contains('\u{202e}'));
        assert!(app.status.chars().count() <= FILE_SAVE_EVENT_STATUS_MAX_CHARS);
    }

    #[test]
    fn file_save_failed_for_close_request_restores_close_guard_and_keeps_queue() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root.clone());
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "dirty".to_owned());
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.in_flight_saves.insert(7);
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(8);

        app.handle_file_save_event(UiEvent::FileSaveFailed {
            root,
            generation: app.workspace_event_generation,
            id: 7,
            path,
            error: "disk full".to_owned(),
        });

        assert_eq!(app.close_after_save, None);
        assert_eq!(app.dirty_close_buffer, Some(7));
        assert_eq!(app.pending_close_buffers, vec![8]);
        assert!(!app.in_flight_saves.contains(&7));
        assert!(app.status.starts_with("Could not save main.rs: "));
    }

    #[test]
    fn file_save_failed_for_close_request_preserves_unrelated_dirty_close_guard() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let other_path = root.join("src/other.rs");
        let mut app = app_for_test(root.clone());
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "dirty".to_owned());
        buffer.mark_dirty();
        let mut other = TextBuffer::from_text(9, Some(other_path), "other".to_owned());
        other.mark_dirty();
        app.buffers.push(buffer);
        app.buffers.push(other);
        app.in_flight_saves.insert(7);
        app.close_after_save = Some(7);
        app.dirty_close_buffer = Some(9);
        app.pending_close_buffers.push(8);

        app.handle_file_save_event(UiEvent::FileSaveFailed {
            root,
            generation: app.workspace_event_generation,
            id: 7,
            path,
            error: "disk full".to_owned(),
        });

        assert_eq!(app.close_after_save, None);
        assert_eq!(app.dirty_close_buffer, Some(9));
        assert_eq!(app.pending_close_buffers, vec![7, 8]);
        assert!(!app.in_flight_saves.contains(&7));
        assert!(app.status.starts_with("Could not save main.rs: "));
    }

    #[test]
    fn file_saved_for_close_request_reports_saved_and_closed_status() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root.clone());
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "dirty".to_owned());
        buffer.mark_dirty();
        let version = buffer.version();
        app.buffers.push(buffer);
        app.in_flight_saves.insert(7);
        app.close_after_save = Some(7);

        app.handle_file_save_event(UiEvent::FileSaved {
            root,
            generation: app.workspace_event_generation,
            id: 7,
            path,
            version,
        });

        assert!(app.buffer(7).is_none());
        assert_eq!(app.close_after_save, None);
        assert_eq!(app.status, "Saved and closed main.rs");
    }

    #[test]
    fn file_saved_for_untitled_close_request_reports_saved_as_path() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root.clone());
        let mut buffer = TextBuffer::from_text(7, None, "dirty".to_owned());
        buffer.mark_dirty();
        let version = buffer.version();
        app.buffers.push(buffer);
        app.in_flight_saves.insert(7);
        app.close_after_save = Some(7);

        app.handle_file_save_event(UiEvent::FileSaved {
            root,
            generation: app.workspace_event_generation,
            id: 7,
            path,
            version,
        });

        assert!(app.buffer(7).is_none());
        assert_eq!(app.close_after_save, None);
        assert_eq!(app.status, "Saved and closed main.rs");
    }

    #[test]
    fn file_saved_for_close_request_with_newer_edits_restores_close_guard() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root.clone());
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "dirty".to_owned());
        buffer.mark_dirty();
        let saved_version = buffer.version();
        buffer.insert_at_cursor(" newer");
        app.buffers.push(buffer);
        app.in_flight_saves.insert(7);
        app.close_after_save = Some(7);

        app.handle_file_save_event(UiEvent::FileSaved {
            root,
            generation: app.workspace_event_generation,
            id: 7,
            path,
            version: saved_version,
        });

        assert!(app.buffer(7).is_some_and(TextBuffer::is_dirty));
        assert_eq!(app.close_after_save, None);
        assert_eq!(app.dirty_close_buffer, Some(7));
        assert!(app.status.contains("newer edits remain unsaved"));
    }

    #[test]
    fn file_saved_for_close_request_with_newer_edits_preserves_unrelated_dirty_close_guard() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let other_path = root.join("src/other.rs");
        let mut app = app_for_test(root.clone());
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "dirty".to_owned());
        buffer.mark_dirty();
        let saved_version = buffer.version();
        buffer.insert_at_cursor(" newer");
        let mut other = TextBuffer::from_text(9, Some(other_path), "other".to_owned());
        other.mark_dirty();
        app.buffers.push(buffer);
        app.buffers.push(other);
        app.in_flight_saves.insert(7);
        app.close_after_save = Some(7);
        app.dirty_close_buffer = Some(9);
        app.pending_close_buffers.push(8);

        app.handle_file_save_event(UiEvent::FileSaved {
            root,
            generation: app.workspace_event_generation,
            id: 7,
            path,
            version: saved_version,
        });

        assert!(app.buffer(7).is_some_and(TextBuffer::is_dirty));
        assert_eq!(app.close_after_save, None);
        assert_eq!(app.dirty_close_buffer, Some(9));
        assert_eq!(app.pending_close_buffers, vec![7, 8]);
        assert!(app.status.contains("newer edits remain unsaved"));
    }

    #[test]
    fn file_saved_save_as_target_opened_while_in_flight_keeps_single_path_owner() {
        let root = PathBuf::from("workspace");
        let target_path = root.join("src/main.rs");
        let mut app = app_for_test(root.clone());
        let mut source = TextBuffer::from_text(7, None, "source".to_owned());
        source.mark_dirty();
        let saved_version = source.version();
        let target = TextBuffer::from_text(8, Some(target_path.clone()), "target".to_owned());
        app.buffers.push(source);
        app.buffers.push(target);
        app.in_flight_saves.insert(7);

        app.handle_file_save_event(UiEvent::FileSaved {
            root,
            generation: app.workspace_event_generation,
            id: 7,
            path: target_path.clone(),
            version: saved_version,
        });

        assert!(app.buffer(7).is_some_and(TextBuffer::is_dirty));
        assert!(
            app.buffer(7)
                .and_then(TextBuffer::path)
                .is_none_or(|path| !paths_match_lexically(path, &target_path))
        );
        assert_eq!(
            app.buffers
                .iter()
                .filter(|buffer| buffer
                    .path()
                    .is_some_and(|path| paths_match_lexically(path, &target_path)))
                .count(),
            1
        );
        assert!(app.buffer_changed_on_disk(8));
        assert!(!app.in_flight_saves.contains(&7));
        assert!(app.status.starts_with("Saved "));
        assert!(app.status.ends_with(" is already open"));
    }

    #[test]
    fn file_saved_open_target_status_is_bounded_and_display_safe() {
        let root = PathBuf::from("workspace");
        let target_path = root.join(format!(
            "bad\n{}\u{202e}.rs",
            "very-long-name-".repeat(FILE_SAVE_EVENT_STATUS_MAX_CHARS)
        ));
        let mut app = app_for_test(root.clone());
        let mut source = TextBuffer::from_text(7, None, "source".to_owned());
        source.mark_dirty();
        let saved_version = source.version();
        let target = TextBuffer::from_text(8, Some(target_path.clone()), "target".to_owned());
        app.buffers.push(source);
        app.buffers.push(target);
        app.virtual_buffer_labels.insert(
            8,
            format!(
                "target\n{}\u{202e}",
                "very-long-label-".repeat(FILE_SAVE_EVENT_STATUS_MAX_CHARS)
            ),
        );
        app.in_flight_saves.insert(7);

        app.handle_file_save_event(UiEvent::FileSaved {
            root,
            generation: app.workspace_event_generation,
            id: 7,
            path: target_path,
            version: saved_version,
        });

        assert!(app.status.starts_with("Saved "));
        assert!(!app.status.contains('\n'));
        assert!(!app.status.contains('\u{202e}'));
        assert!(app.status.chars().count() <= FILE_SAVE_EVENT_STATUS_MAX_CHARS);
    }

    #[test]
    fn file_saved_save_as_target_opened_while_in_flight_resumes_queued_save() {
        let root = PathBuf::from("workspace");
        let target_path = root.join("src/main.rs");
        let queued_path = root.join("src/copy.rs");
        let mut app = app_for_test(root.clone());
        let mut source = TextBuffer::from_text(7, None, "source".to_owned());
        source.mark_dirty();
        let saved_version = source.version();
        source.insert_at_cursor(" newer");
        let target = TextBuffer::from_text(8, Some(target_path.clone()), "target".to_owned());
        app.buffers.push(source);
        app.buffers.push(target);
        app.in_flight_saves.insert(7);
        app.queued_save_paths.insert(7, queued_path.clone());

        app.handle_file_save_event(UiEvent::FileSaved {
            root,
            generation: app.workspace_event_generation,
            id: 7,
            path: target_path.clone(),
            version: saved_version,
        });

        assert!(app.buffer(7).is_some_and(TextBuffer::is_dirty));
        assert!(
            app.buffer(7)
                .and_then(TextBuffer::path)
                .is_none_or(|path| !paths_match_lexically(path, &target_path))
        );
        assert!(app.buffer_changed_on_disk(8));
        assert!(app.in_flight_saves.contains(&7));
        assert!(!app.queued_save_paths.contains_key(&7));
        assert!(app.status.starts_with("Saving "));
        assert!(app.status.contains("copy.rs"));
    }

    #[test]
    fn queued_save_after_external_change_opens_conflict_instead_of_writing() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root.clone());
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "dirty".to_owned());
        buffer.mark_dirty();
        let saved_version = buffer.version();
        buffer.insert_at_cursor(" newer");
        app.buffers.push(buffer);
        app.in_flight_saves.insert(7);
        app.queued_save_paths.insert(7, path.clone());
        app.external_change_buffers.insert(7);

        app.handle_file_save_event(UiEvent::FileSaved {
            root,
            generation: app.workspace_event_generation,
            id: 7,
            path,
            version: saved_version,
        });

        assert_eq!(app.save_conflict_buffer, Some(7));
        assert!(!app.in_flight_saves.contains(&7));
        assert!(!app.queued_save_paths.contains_key(&7));
        assert!(app.external_change_buffers.contains(&7));
        assert!(app.buffer(7).is_some_and(TextBuffer::is_dirty));
        assert!(app.status.ends_with(" changed on disk"));
    }

    #[test]
    fn queued_save_after_pending_clean_reload_opens_conflict_instead_of_writing() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root.clone());
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "dirty".to_owned());
        buffer.mark_dirty();
        let saved_version = buffer.version();
        buffer.insert_at_cursor(" newer");
        app.buffers.push(buffer);
        app.in_flight_saves.insert(7);
        app.queued_save_paths.insert(7, path.clone());
        app.in_flight_reloads.insert(
            7,
            PendingFileReload {
                request_id: 1,
                path: path.clone(),
                version: saved_version,
                force_dirty: false,
            },
        );

        app.handle_file_save_event(UiEvent::FileSaved {
            root,
            generation: app.workspace_event_generation,
            id: 7,
            path,
            version: saved_version,
        });

        assert_eq!(app.save_conflict_buffer, Some(7));
        assert!(!app.in_flight_saves.contains(&7));
        assert!(!app.queued_save_paths.contains_key(&7));
        assert!(app.observed_external_change_buffer_ids().contains(&7));
        assert!(app.buffer(7).is_some_and(TextBuffer::is_dirty));
        assert!(app.status.ends_with(" changed on disk"));
    }

    #[test]
    fn queued_save_after_queued_clean_reload_opens_conflict_instead_of_writing() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root.clone());
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "dirty".to_owned());
        buffer.mark_dirty();
        let saved_version = buffer.version();
        buffer.insert_at_cursor(" newer");
        app.buffers.push(buffer);
        app.in_flight_saves.insert(7);
        app.queued_save_paths.insert(7, path.clone());
        app.queued_file_reloads.insert(
            7,
            QueuedFileReload {
                path: path.clone(),
                force_dirty: false,
            },
        );

        app.handle_file_save_event(UiEvent::FileSaved {
            root,
            generation: app.workspace_event_generation,
            id: 7,
            path,
            version: saved_version,
        });

        assert_eq!(app.save_conflict_buffer, Some(7));
        assert!(!app.in_flight_saves.contains(&7));
        assert!(!app.queued_save_paths.contains_key(&7));
        assert!(app.observed_external_change_buffer_ids().contains(&7));
        assert!(app.buffer(7).is_some_and(TextBuffer::is_dirty));
        assert!(app.status.ends_with(" changed on disk"));
    }

    #[test]
    fn queued_save_after_queued_clean_reload_preserves_close_request_for_conflict() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root.clone());
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "dirty".to_owned());
        buffer.mark_dirty();
        let saved_version = buffer.version();
        buffer.insert_at_cursor(" newer");
        app.buffers.push(buffer);
        app.in_flight_saves.insert(7);
        app.queued_save_paths.insert(7, path.clone());
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(8);
        app.queued_file_reloads.insert(
            7,
            QueuedFileReload {
                path: path.clone(),
                force_dirty: false,
            },
        );

        app.handle_file_save_event(UiEvent::FileSaved {
            root,
            generation: app.workspace_event_generation,
            id: 7,
            path,
            version: saved_version,
        });

        assert_eq!(app.save_conflict_buffer, Some(7));
        assert_eq!(app.close_after_save, Some(7));
        assert_eq!(app.pending_close_buffers, vec![8]);
        assert!(!app.in_flight_saves.contains(&7));
        assert!(!app.queued_save_paths.contains_key(&7));
        assert!(app.buffer(7).is_some_and(TextBuffer::is_dirty));
        assert!(app.status.ends_with(" changed on disk"));
    }

    #[test]
    fn queued_save_after_equivalent_pending_clean_reload_opens_conflict() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let queued_path = root.join("src").join(".").join("main.rs");
        let mut app = app_for_test(root.clone());
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "dirty".to_owned());
        buffer.mark_dirty();
        let saved_version = buffer.version();
        buffer.insert_at_cursor(" newer");
        app.buffers.push(buffer);
        app.in_flight_saves.insert(7);
        app.queued_save_paths.insert(7, queued_path);
        app.in_flight_reloads.insert(
            7,
            PendingFileReload {
                request_id: 1,
                path: path.clone(),
                version: saved_version,
                force_dirty: false,
            },
        );

        app.handle_file_save_event(UiEvent::FileSaved {
            root,
            generation: app.workspace_event_generation,
            id: 7,
            path,
            version: saved_version,
        });

        assert_eq!(app.save_conflict_buffer, Some(7));
        assert!(!app.in_flight_saves.contains(&7));
        assert!(!app.queued_save_paths.contains_key(&7));
        assert!(app.status.ends_with(" changed on disk"));
    }

    #[test]
    fn queued_save_to_different_path_ignores_pending_reload_for_current_path() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let save_as_path = root.join("src/copy.rs");
        let mut app = app_for_test(root.clone());
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "dirty".to_owned());
        buffer.mark_dirty();
        let saved_version = buffer.version();
        buffer.insert_at_cursor(" newer");
        app.buffers.push(buffer);
        app.in_flight_saves.insert(7);
        app.queued_save_paths.insert(7, save_as_path.clone());
        app.in_flight_reloads.insert(
            7,
            PendingFileReload {
                request_id: 1,
                path: path.clone(),
                version: saved_version,
                force_dirty: false,
            },
        );

        app.handle_file_save_event(UiEvent::FileSaved {
            root,
            generation: app.workspace_event_generation,
            id: 7,
            path,
            version: saved_version,
        });

        assert_eq!(app.save_conflict_buffer, None);
        assert!(app.in_flight_saves.contains(&7));
        assert!(!app.queued_save_paths.contains_key(&7));
        assert!(app.status.starts_with("Saving "));
        assert!(app.status.contains("copy.rs"));
    }

    #[test]
    fn file_saved_preserves_unrelated_active_save_conflict_status() {
        let root = PathBuf::from("workspace");
        let safe_path = root.join("src/safe.rs");
        let changed_path = root.join("src/changed.rs");
        let mut app = app_for_test(root.clone());
        let mut safe_buffer = TextBuffer::from_text(1, Some(safe_path.clone()), "safe".to_owned());
        safe_buffer.mark_dirty();
        let safe_version = safe_buffer.version();
        let mut changed_buffer =
            TextBuffer::from_text(2, Some(changed_path.clone()), "changed".to_owned());
        changed_buffer.mark_dirty();
        app.buffers.push(safe_buffer);
        app.buffers.push(changed_buffer);
        app.in_flight_saves.insert(1);
        app.external_change_buffers.insert(2);
        app.save_conflict_buffer = Some(2);
        app.status = "changed.rs changed on disk".to_owned();

        app.handle_file_save_event(UiEvent::FileSaved {
            root,
            generation: app.workspace_event_generation,
            id: 1,
            path: safe_path,
            version: safe_version,
        });

        assert!(!app.buffer(1).unwrap().is_dirty());
        assert_eq!(app.save_conflict_buffer, Some(2));
        assert!(app.external_change_buffers.contains(&2));
        assert_eq!(app.status, "changed.rs changed on disk");
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

    fn diagnostic(path: &std::path::Path, message: &str) -> Diagnostic {
        Diagnostic {
            path: path.to_path_buf(),
            line: 1,
            column: 1,
            char_range: 0..1,
            severity: DiagnosticSeverity::Warning,
            source: "rust-analyzer".to_owned(),
            message: message.to_owned(),
            unused: false,
            deprecated: false,
        }
    }
}
