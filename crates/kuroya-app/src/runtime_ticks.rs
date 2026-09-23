use crate::{
    KuroyaApp,
    app_session::SessionSaveSnapshot,
    lsp_diagnostics_batch::LSP_DIAGNOSTIC_BATCH_DELAY,
    lsp_lifecycle::{
        LANGUAGE_SYNC_DEBOUNCE, due_language_sync_ids, lsp_lifecycle_target_for_buffer,
    },
    lsp_runtime::{lsp_diagnostics_source_key, lsp_server_configs_for_settings},
    path_display::compact_path,
    persistence,
    save_lifecycle::{
        SessionSaveRequest, autosave_buffer_ids, reserve_session_save,
        should_skip_session_persistence, stage_session_save_fingerprint,
        with_session_save_rotation_order,
    },
    transient_state::{PendingExit, PendingWorkspaceSwitch},
    ui_events::UiEvent,
    workspace_state::{PaneId, lsp_event_path_is_current},
};
use kuroya_core::{
    BufferId, EditorAutoSaveMode, TextBuffer, clamp_autosave_delay_ms,
    clamp_quick_suggestions_delay_ms, diagnostics::LspDiagnosticUnits,
};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

mod watcher;

pub(crate) const SIGNATURE_HELP_DEBOUNCE: Duration = Duration::from_millis(60);
pub(crate) const FORMAT_ON_TYPE_DEBOUNCE: Duration = Duration::from_millis(120);
pub(crate) const FORMAT_ON_SAVE_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const FORMAT_ON_SAVE_MAX_RETRIES: u8 = 1;

impl KuroyaApp {
    pub(crate) fn persist_session_if_needed(&mut self) -> bool {
        if self.workspace_placeholder {
            return false;
        }
        let now = Instant::now();
        if now.saturating_duration_since(self.last_session_save) < Duration::from_secs(2) {
            return false;
        }
        self.last_session_save = now;
        let fingerprint = self.session_structure_fingerprint();
        if should_skip_session_persistence(
            Some(fingerprint),
            self.last_session_structure_fingerprint,
            self.last_saved_session_structure_fingerprint,
            self.session_save_in_flight.is_some(),
            !self.queued_session_saves.is_empty(),
        ) {
            self.session_save_persisted_changes = false;
            return false;
        }

        self.session_save_persisted_changes = true;
        let root = self.workspace.root.clone();
        let session = self.build_session_save_snapshot();
        self.last_session_structure_fingerprint = Some(fingerprint);
        stage_session_save_fingerprint(&root, fingerprint);
        self.request_session_save(root, session)
    }

    pub(crate) fn request_session_save(
        &mut self,
        root: PathBuf,
        session: SessionSaveSnapshot,
    ) -> bool {
        let request = with_session_save_rotation_order(|order| {
            reserve_session_save(
                &root,
                session.clone(),
                &mut self.session_save_in_flight,
                &mut self.queued_session_saves,
                order,
            )
        });
        if request == SessionSaveRequest::Spawn {
            self.spawn_session_save(root, session);
            true
        } else {
            false
        }
    }

    pub(crate) fn spawn_session_save(&mut self, root: PathBuf, session: SessionSaveSnapshot) {
        let tx = self.tx.clone();
        self.record_async_task_started("Session Save", compact_path(&root));
        self.session_save_in_flight_snapshot = Some(session.clone());
        self.session_save_in_flight_task = Some(self.runtime.spawn(async move {
            let session = session.into_persisted_session();
            match persistence::save_session_async(root.clone(), session).await {
                Ok(()) => {
                    let _ = crate::ui_event_channel::send_critical_ui_event(
                        &tx,
                        UiEvent::SessionSaved { root },
                    );
                }
                Err(error) => {
                    let _ = crate::ui_event_channel::send_critical_ui_event(
                        &tx,
                        UiEvent::SessionSaveFailed {
                            root,
                            error: error.to_string(),
                        },
                    );
                }
            }
        }));
    }

    pub(crate) fn flush_pending_lsp_resync(&mut self) -> usize {
        let paths = std::mem::take(&mut self.pending_lsp_resync);
        let mut count = 0usize;
        for path in paths {
            let Some(id) = self.pending_lsp_resync_target(&path) else {
                continue;
            };
            self.schedule_language_sync(id);
            count = count.saturating_add(1);
        }
        count
    }

    fn pending_lsp_resync_target(&self, path: &Path) -> Option<BufferId> {
        let buffer = self.buffer_by_lexical_path(path)?;
        let (language, _) = lsp_lifecycle_target_for_buffer(
            buffer,
            &lsp_server_configs_for_settings(&self.settings),
            &self.plugin_languages,
            &self.lossy_decoded_buffers,
            &self.binary_preview_buffers,
        )?;
        (!self.live_lsp_clients_for_language(&language).is_empty()).then_some(buffer.id())
    }

    pub(crate) fn flush_pending_language_sync(&mut self) -> usize {
        self.flush_pending_lsp_resync();
        let now = Instant::now();
        let ids = due_language_sync_ids(&self.pending_language_sync, now, LANGUAGE_SYNC_DEBOUNCE);
        let mut count = 0usize;
        for id in ids {
            self.pending_language_sync.remove(&id);
            if self.buffer(id).is_none() {
                self.clear_static_diagnostics_request(id);
                continue;
            }
            self.spawn_diagnostics_for(id);
            self.notify_lsp_change(id);
            count = count.saturating_add(1);
        }
        count
    }

    pub(crate) fn flush_pending_completion_requests(&mut self) -> usize {
        let now = Instant::now();
        let delay = Duration::from_millis(clamp_quick_suggestions_delay_ms(
            self.settings.quick_suggestions_delay_ms,
        ) as u64);
        let dispatch = take_due_active_buffer_requests(
            &mut self.pending_completion_requests,
            now,
            delay,
            self.active,
        );
        let count = dispatch.len();
        for id in dispatch {
            self.request_lsp_completion_for_buffer(id, false);
        }
        count
    }

    pub(crate) fn flush_pending_signature_help_requests(&mut self) -> usize {
        let now = Instant::now();
        let dispatch = take_due_active_buffer_requests(
            &mut self.pending_signature_help_requests,
            now,
            SIGNATURE_HELP_DEBOUNCE,
            self.active,
        );
        let count = dispatch.len();
        for id in dispatch {
            self.request_lsp_signature_help_for_buffer(id, false);
        }
        count
    }

    pub(crate) fn flush_pending_format_on_type_requests(&mut self) -> usize {
        let now = Instant::now();
        let dispatch = take_due_active_buffer_requests(
            &mut self.pending_format_on_type_requests,
            now,
            FORMAT_ON_TYPE_DEBOUNCE,
            self.active,
        );
        let count = dispatch.len();
        for id in dispatch {
            let _ =
                self.request_lsp_formatting_for_buffer(id, Some("Formatting typed text in"), false);
        }
        count
    }

    pub(crate) fn flush_timed_out_format_on_save_requests(&mut self) -> usize {
        let now = Instant::now();
        let pending_ids = self
            .pending_format_on_save
            .keys()
            .copied()
            .collect::<HashSet<_>>();
        self.pending_format_on_save_started
            .retain(|id, _| pending_ids.contains(id));
        self.pending_format_on_save_retries
            .retain(|id, _| pending_ids.contains(id));
        for id in self.pending_format_on_save.keys().copied() {
            self.pending_format_on_save_started.entry(id).or_insert(now);
        }
        let due = self
            .pending_format_on_save_started
            .iter()
            .filter_map(|(id, started)| {
                (now.saturating_duration_since(*started) >= FORMAT_ON_SAVE_TIMEOUT).then_some(*id)
            })
            .collect::<Vec<_>>();
        let count = due.len();

        for id in due {
            let Some(pending) = self.pending_format_on_save.get(&id).cloned() else {
                self.pending_format_on_save_started.remove(&id);
                self.pending_format_on_save_retries.remove(&id);
                continue;
            };
            let retries = self.pending_format_on_save_retries(id);
            if retries < FORMAT_ON_SAVE_MAX_RETRIES {
                if let Some((current_version, current_format_path)) =
                    self.buffer(id).map(|buffer| {
                        (
                            buffer.version(),
                            buffer
                                .path()
                                .cloned()
                                .unwrap_or_else(|| pending.format_path.clone()),
                        )
                    })
                    && let Some(request_id) = self.request_lsp_formatting_for_buffer(
                        id,
                        Some("Retrying format before save"),
                        false,
                    )
                {
                    self.replace_pending_format_on_save(
                        id,
                        crate::app_state::PendingFormatOnSave {
                            save_path: pending.save_path,
                            format_path: current_format_path,
                            version: current_version,
                            request_id,
                        },
                        retries.saturating_add(1),
                    );
                    continue;
                }
            }

            let overwrite_external_change = self.format_on_save_overwrites_external_change(id);
            self.cancel_pending_format_on_save(id);
            self.format_on_save_bypass.insert(id);
            if overwrite_external_change {
                self.spawn_save_to_over_external_change(id, pending.save_path);
            } else {
                self.spawn_save_to(id, pending.save_path);
            }
        }

        count
    }

    pub(crate) fn flush_pending_lsp_diagnostics(&mut self) -> usize {
        let now = Instant::now();
        let diagnostics = self
            .pending_lsp_diagnostics
            .take_due_entries(now, LSP_DIAGNOSTIC_BATCH_DELAY);
        let mut count = 0usize;
        for entry in diagnostics {
            if let Some(source) = &entry.source
                && !self.lsp_lifecycle_event_matches(
                    &source.language,
                    &source.root,
                    source.generation,
                )
            {
                continue;
            }
            let path = entry.path;
            if !lsp_event_path_is_current(&self.workspace.root, &path) {
                continue;
            }

            let (path, diagnostics, units) = if let Some(buffer) =
                self.buffer_by_lexical_path(&path)
            {
                if entry
                    .version
                    .is_some_and(|version| version != buffer.version())
                {
                    continue;
                }
                let diagnostics = crate::lsp_diagnostics_batch::valid_lsp_diagnostics_for_buffer(
                    buffer,
                    entry.diagnostics,
                );
                let path = buffer.path().cloned().unwrap_or(path);
                (path, diagnostics, LspDiagnosticUnits::Chars)
            } else {
                (path, entry.diagnostics, LspDiagnosticUnits::Utf16)
            };

            match entry
                .source
                .as_ref()
                .map(|source| lsp_diagnostics_source_key(&source.language, source.generation))
            {
                Some(source_key) => {
                    self.diagnostics
                        .replace_lsp_source_units(path, &source_key, diagnostics, units)
                }
                None => self.diagnostics.replace_lsp_units(path, diagnostics, units),
            }
            count = count.saturating_add(1);
        }

        for path in self.diagnostics.raw_utf16_lsp_paths() {
            let Some(buffer) = self.buffer_by_lexical_path(&path).cloned() else {
                continue;
            };
            for (source_key, raw) in self.diagnostics.drain_raw_utf16_lsp_buckets(&path) {
                let converted =
                    crate::lsp_diagnostics_batch::valid_lsp_diagnostics_for_buffer(&buffer, raw);
                self.diagnostics.replace_lsp_source_units(
                    path.clone(),
                    &source_key,
                    converted,
                    LspDiagnosticUnits::Chars,
                );
                count = count.saturating_add(1);
            }
        }
        count
    }

    pub(crate) fn autosave_if_needed(&mut self, window_focused: bool) -> usize {
        let now = Instant::now();
        let mode = self.settings.effective_autosave_mode();
        let delay = Duration::from_millis(clamp_autosave_delay_ms(self.settings.autosave_delay_ms));
        let due = autosave_due_for_mode(
            mode,
            now.saturating_duration_since(self.last_autosave),
            delay,
            self.last_autosave_window_focused,
            window_focused,
            self.last_autosave_focused_pane,
            self.focused_pane,
        );
        self.last_autosave_window_focused = window_focused;
        self.last_autosave_focused_pane = self.focused_pane;
        if !due {
            return 0;
        }

        self.last_autosave = now;
        let blocked_buffers = self.autosave_blocked_buffer_ids();
        let dirty_ids = autosave_buffer_ids(
            &self.buffers,
            &self.external_change_buffers,
            &blocked_buffers,
            &self.lossy_decoded_buffers,
            &self.binary_preview_buffers,
        );
        let count = dirty_ids.len();
        for id in dirty_ids {
            self.spawn_save(id);
        }
        count
    }

    fn autosave_blocked_buffer_ids(&self) -> HashSet<BufferId> {
        let pending_reload_external_change = self.pending_reload_external_change_buffer_ids();
        let block_dirty_for_modal = matches!(
            self.pending_workspace_switch,
            Some(PendingWorkspaceSwitch::Confirm { .. })
        ) || matches!(self.pending_exit, Some(PendingExit::Confirm));
        let mut blocked = HashSet::with_capacity(
            self.pending_close_buffers
                .len()
                .saturating_add(pending_reload_external_change.len())
                .saturating_add(4)
                .saturating_add(if block_dirty_for_modal {
                    self.buffers.len()
                } else {
                    0
                }),
        );
        blocked.extend(self.pending_close_buffers.iter().copied());
        if let Some(id) = self.dirty_close_buffer {
            blocked.insert(id);
        }
        if let Some(id) = self.dirty_reload_buffer {
            blocked.insert(id);
        }
        if let Some(id) = self.save_conflict_buffer {
            blocked.insert(id);
        }
        if let Some(id) = self.close_after_save {
            blocked.insert(id);
        }
        blocked.extend(pending_reload_external_change);

        if block_dirty_for_modal {
            blocked.extend(
                self.buffers
                    .iter()
                    .filter(|buffer| buffer.is_dirty())
                    .map(TextBuffer::id),
            );
        }

        blocked
    }
}

#[cfg(test)]
pub(crate) fn due_completion_request_ids(
    pending: &std::collections::HashMap<BufferId, Instant>,
    now: Instant,
    delay: Duration,
) -> Vec<BufferId> {
    let mut ids = pending
        .iter()
        .filter_map(|(id, scheduled)| {
            (now.saturating_duration_since(*scheduled) >= delay).then_some(*id)
        })
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct DueActiveBufferRequests {
    dispatch: Vec<BufferId>,
    discard: Vec<BufferId>,
}

#[cfg(test)]
fn due_active_buffer_requests(
    pending: &HashMap<BufferId, Instant>,
    now: Instant,
    delay: Duration,
    active: Option<BufferId>,
) -> DueActiveBufferRequests {
    let mut dispatch = Vec::new();
    let mut discard = Vec::new();
    for id in due_completion_request_ids(pending, now, delay) {
        if Some(id) == active {
            dispatch.push(id);
        } else {
            discard.push(id);
        }
    }
    DueActiveBufferRequests { dispatch, discard }
}

fn take_due_active_buffer_requests(
    pending: &mut HashMap<BufferId, Instant>,
    now: Instant,
    delay: Duration,
    active: Option<BufferId>,
) -> Vec<BufferId> {
    let mut dispatch = Vec::with_capacity(usize::from(active.is_some()));
    pending.retain(|id, scheduled| {
        if now.saturating_duration_since(*scheduled) < delay {
            return true;
        }
        if Some(*id) == active {
            dispatch.push(*id);
        }
        false
    });
    dispatch
}

pub(crate) fn autosave_due_for_mode(
    mode: EditorAutoSaveMode,
    elapsed: Duration,
    delay: Duration,
    last_window_focused: bool,
    window_focused: bool,
    last_focused_pane: Option<PaneId>,
    focused_pane: Option<PaneId>,
) -> bool {
    match mode {
        EditorAutoSaveMode::Off => false,
        EditorAutoSaveMode::AfterDelay => elapsed >= delay,
        EditorAutoSaveMode::OnFocusChange => {
            window_lost_focus(last_window_focused, window_focused)
                || editor_focus_changed(last_focused_pane, focused_pane)
        }
        EditorAutoSaveMode::OnWindowChange => {
            window_lost_focus(last_window_focused, window_focused)
        }
    }
}

fn window_lost_focus(last_window_focused: bool, window_focused: bool) -> bool {
    last_window_focused && !window_focused
}

fn editor_focus_changed(last_focused_pane: Option<PaneId>, focused_pane: Option<PaneId>) -> bool {
    last_focused_pane.is_some() && last_focused_pane != focused_pane
}

#[cfg(test)]
mod tests {
    use super::{
        FORMAT_ON_SAVE_MAX_RETRIES, FORMAT_ON_SAVE_TIMEOUT, LANGUAGE_SYNC_DEBOUNCE,
        autosave_due_for_mode, due_active_buffer_requests, due_completion_request_ids,
        take_due_active_buffer_requests,
    };
    use crate::{
        KuroyaApp,
        app_startup_context::AppStartupContext,
        app_state::{PendingFileReload, PendingFormatOnSave, QueuedFileReload},
        lsp_client::LspClientHandle,
        lsp_diagnostics_batch::{LSP_DIAGNOSTIC_BATCH_DELAY, PendingLspDiagnosticsSource},
        terminal::TerminalPane,
    };
    use eframe::egui::Context;
    use kuroya_core::{Diagnostic, DiagnosticSeverity, EditorAutoSaveMode};
    use kuroya_core::{EditorSettings, TextBuffer, Workspace};
    use std::{
        collections::HashMap,
        path::PathBuf,
        time::{Duration, Instant},
    };
    use tokio::runtime::Runtime;

    #[test]
    fn autosave_modes_gate_delay_focus_and_window_triggers() {
        let delay = Duration::from_secs(4);

        assert!(!autosave_due_for_mode(
            EditorAutoSaveMode::Off,
            delay,
            delay,
            true,
            false,
            Some(1),
            Some(2),
        ));
        assert!(!autosave_due_for_mode(
            EditorAutoSaveMode::AfterDelay,
            Duration::from_secs(3),
            delay,
            true,
            true,
            Some(1),
            Some(2),
        ));
        assert!(autosave_due_for_mode(
            EditorAutoSaveMode::AfterDelay,
            delay,
            delay,
            true,
            true,
            None,
            None,
        ));
        assert!(autosave_due_for_mode(
            EditorAutoSaveMode::OnFocusChange,
            Duration::ZERO,
            delay,
            true,
            true,
            Some(1),
            Some(2),
        ));
        assert!(autosave_due_for_mode(
            EditorAutoSaveMode::OnFocusChange,
            Duration::ZERO,
            delay,
            true,
            false,
            Some(1),
            Some(1),
        ));
        assert!(autosave_due_for_mode(
            EditorAutoSaveMode::OnWindowChange,
            Duration::ZERO,
            delay,
            true,
            false,
            Some(1),
            Some(1),
        ));
        assert!(!autosave_due_for_mode(
            EditorAutoSaveMode::OnWindowChange,
            delay,
            delay,
            true,
            true,
            Some(1),
            Some(2),
        ));
    }

    #[test]
    fn focus_change_ignores_initial_focus_capture() {
        assert!(!autosave_due_for_mode(
            EditorAutoSaveMode::OnFocusChange,
            Duration::ZERO,
            Duration::from_secs(4),
            true,
            true,
            None,
            Some(1),
        ));
    }

    #[test]
    fn focus_change_autosave_queues_dirty_in_flight_buffer() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.settings.autosave = true;
        app.settings.autosave_mode = EditorAutoSaveMode::OnFocusChange;
        app.last_autosave_window_focused = true;
        app.last_autosave_focused_pane = Some(1);
        app.focused_pane = Some(2);
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "newer text".to_owned());
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.in_flight_saves.insert(7);

        assert_eq!(app.autosave_if_needed(true), 1);

        assert!(app.in_flight_saves.contains(&7));
        assert_eq!(app.queued_save_paths.get(&7), Some(&path));
        assert_eq!(app.last_autosave_focused_pane, Some(2));
    }

    #[test]
    fn focus_change_autosave_skips_close_after_save_buffer() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.settings.autosave = true;
        app.settings.autosave_mode = EditorAutoSaveMode::OnFocusChange;
        app.last_autosave_window_focused = true;
        app.last_autosave_focused_pane = Some(1);
        app.focused_pane = Some(2);
        let mut buffer = TextBuffer::from_text(7, Some(path), "newer text".to_owned());
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.in_flight_saves.insert(7);
        app.close_after_save = Some(7);

        assert_eq!(app.autosave_if_needed(true), 0);

        assert!(app.in_flight_saves.contains(&7));
        assert!(!app.queued_save_paths.contains_key(&7));
        assert_eq!(app.last_autosave_focused_pane, Some(2));
    }

    #[test]
    fn focus_change_autosave_skips_close_after_save_and_pending_close_buffers() {
        let root = PathBuf::from("workspace");
        let closing_path = root.join("src/closing.rs");
        let pending_path = root.join("src/pending.rs");
        let unrelated_path = root.join("src/unrelated.rs");
        let mut app = app_for_test(root);
        app.settings.autosave = true;
        app.settings.autosave_mode = EditorAutoSaveMode::OnFocusChange;
        app.last_autosave_window_focused = true;
        app.last_autosave_focused_pane = Some(1);
        app.focused_pane = Some(2);
        let mut closing = TextBuffer::from_text(7, Some(closing_path), "closing".to_owned());
        closing.mark_dirty();
        let mut pending = TextBuffer::from_text(8, Some(pending_path), "pending".to_owned());
        pending.mark_dirty();
        let mut unrelated = TextBuffer::from_text(9, Some(unrelated_path), "unrelated".to_owned());
        unrelated.mark_dirty();
        app.buffers.extend([closing, pending, unrelated]);
        app.in_flight_saves.insert(7);
        app.close_after_save = Some(7);
        app.pending_close_buffers.push(8);

        assert_eq!(app.autosave_if_needed(true), 1);

        assert!(app.in_flight_saves.contains(&7));
        assert!(!app.queued_save_paths.contains_key(&7));
        assert!(!app.in_flight_saves.contains(&8));
        assert!(!app.queued_save_paths.contains_key(&8));
        assert!(app.in_flight_saves.contains(&9));
        assert_eq!(app.close_after_save, Some(7));
        assert_eq!(app.pending_close_buffers, vec![8]);
        assert_eq!(app.last_autosave_focused_pane, Some(2));
    }

    #[test]
    fn autosave_after_delay_treats_future_last_autosave_as_not_due() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.settings.autosave = true;
        app.settings.autosave_mode = EditorAutoSaveMode::AfterDelay;
        app.last_autosave = Instant::now() + Duration::from_secs(60);
        let mut buffer = TextBuffer::from_text(7, Some(path), "newer text".to_owned());
        buffer.mark_dirty();
        app.buffers.push(buffer);

        assert_eq!(app.autosave_if_needed(true), 0);

        assert!(!app.in_flight_saves.contains(&7));
    }

    #[test]
    fn session_save_treats_future_last_save_as_not_due() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        app.last_session_save = Instant::now() + Duration::from_secs(60);

        assert!(!app.persist_session_if_needed());
        assert!(app.rx.try_recv().is_err());
    }

    #[test]
    fn session_save_skips_placeholder_workspace() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        app.workspace_placeholder = true;
        app.last_session_save = Instant::now() - Duration::from_secs(60);

        assert!(!app.persist_session_if_needed());
        assert!(app.rx.try_recv().is_err());
        assert!(app.session_save_in_flight.is_none());
    }

    #[test]
    fn unchanged_session_state_skips_redundant_session_snapshots() {
        let root = temp_session_root("fingerprint-skip");
        std::fs::create_dir_all(&root).unwrap();
        let mut app = app_for_test(root.clone());
        app.last_session_save = Instant::now() - Duration::from_secs(60);

        assert!(app.persist_session_if_needed());
        assert!(
            app.session_save_persisted_changes,
            "staging a save keeps the session-save wakeup armed"
        );

        if let Some(task) = app.session_save_in_flight_task.take() {
            app.runtime.block_on(task).expect("session save task");
        }
        app.handle_events();

        assert_eq!(
            app.last_saved_session_structure_fingerprint,
            app.last_session_structure_fingerprint
        );
        assert!(app.session_save_in_flight.is_none());

        app.last_session_save = Instant::now() - Duration::from_secs(60);

        assert!(!app.persist_session_if_needed());
        assert!(
            !app.session_save_persisted_changes,
            "a tick that persists nothing must disarm the session-save wakeup"
        );
        assert!(app.rx.try_recv().is_err());
        assert!(app.session_save_in_flight.is_none());

        drop(app);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn session_structure_change_triggers_new_session_snapshot_after_skip() {
        let root = temp_session_root("fingerprint-change");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root.clone());
        app.buffers
            .push(TextBuffer::from_text(7, Some(path), "text".to_owned()));
        let fingerprint = app.session_structure_fingerprint();
        app.last_session_structure_fingerprint = Some(fingerprint);
        app.last_saved_session_structure_fingerprint = Some(fingerprint);
        app.last_session_save = Instant::now() - Duration::from_secs(60);

        assert!(!app.persist_session_if_needed());
        assert!(!app.session_save_persisted_changes);

        app.buffer_mut(7).expect("buffer").mark_dirty();
        app.last_session_save = Instant::now() - Duration::from_secs(60);

        assert!(app.persist_session_if_needed());
        assert_ne!(app.last_session_structure_fingerprint, Some(fingerprint));
        assert!(
            app.session_save_persisted_changes,
            "a changed session re-arms the session-save wakeup"
        );
        assert!(app.session_save_in_flight.is_some());

        drop(app);
        let _ = std::fs::remove_dir_all(root);
    }

    fn temp_session_root(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("kuroya-{name}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn focus_change_autosave_skips_modal_guard_buffers_but_saves_unrelated_dirty_buffer() {
        let root = PathBuf::from("workspace");
        let dirty_close_path = root.join("src/dirty_close.rs");
        let dirty_reload_path = root.join("src/dirty_reload.rs");
        let conflict_path = root.join("src/conflict.rs");
        let unrelated_path = root.join("src/unrelated.rs");
        let mut app = app_for_test(root);
        app.settings.autosave = true;
        app.settings.autosave_mode = EditorAutoSaveMode::OnFocusChange;
        app.last_autosave_window_focused = true;
        app.last_autosave_focused_pane = Some(1);
        app.focused_pane = Some(2);
        let mut dirty_close =
            TextBuffer::from_text(7, Some(dirty_close_path), "dirty close".to_owned());
        dirty_close.mark_dirty();
        let mut dirty_reload =
            TextBuffer::from_text(8, Some(dirty_reload_path), "dirty reload".to_owned());
        dirty_reload.mark_dirty();
        let mut conflict = TextBuffer::from_text(9, Some(conflict_path), "conflict".to_owned());
        conflict.mark_dirty();
        let mut unrelated = TextBuffer::from_text(10, Some(unrelated_path), "unrelated".to_owned());
        unrelated.mark_dirty();
        app.buffers
            .extend([dirty_close, dirty_reload, conflict, unrelated]);
        app.dirty_close_buffer = Some(7);
        app.dirty_reload_buffer = Some(8);
        app.save_conflict_buffer = Some(9);

        assert_eq!(app.autosave_if_needed(true), 1);

        assert!(!app.in_flight_saves.contains(&7));
        assert!(!app.queued_save_paths.contains_key(&7));
        assert!(!app.in_flight_saves.contains(&8));
        assert!(!app.queued_save_paths.contains_key(&8));
        assert!(!app.in_flight_saves.contains(&9));
        assert!(!app.queued_save_paths.contains_key(&9));
        assert!(app.in_flight_saves.contains(&10));
        assert_eq!(app.dirty_close_buffer, Some(7));
        assert_eq!(app.dirty_reload_buffer, Some(8));
        assert_eq!(app.save_conflict_buffer, Some(9));
        assert_eq!(app.last_autosave_focused_pane, Some(2));
    }

    #[test]
    fn autosave_skips_dirty_buffer_with_pending_clean_reload() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.settings.autosave = true;
        app.settings.autosave_mode = EditorAutoSaveMode::OnFocusChange;
        app.last_autosave_window_focused = true;
        app.last_autosave_focused_pane = Some(1);
        app.focused_pane = Some(2);
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "newer text".to_owned());
        buffer.mark_dirty();
        let version = buffer.version();
        app.buffers.push(buffer);
        app.in_flight_reloads.insert(
            7,
            PendingFileReload {
                request_id: 1,
                path,
                version,
                force_dirty: false,
            },
        );

        assert_eq!(app.autosave_if_needed(true), 0);

        assert!(!app.in_flight_saves.contains(&7));
        assert_eq!(app.save_conflict_buffer, None);
    }

    #[test]
    fn autosave_skips_dirty_buffer_with_equivalent_pending_clean_reload() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let equivalent_path = root.join("src").join(".").join("main.rs");
        let mut app = app_for_test(root);
        app.settings.autosave = true;
        app.settings.autosave_mode = EditorAutoSaveMode::OnFocusChange;
        app.last_autosave_window_focused = true;
        app.last_autosave_focused_pane = Some(1);
        app.focused_pane = Some(2);
        let mut buffer = TextBuffer::from_text(7, Some(path), "newer text".to_owned());
        buffer.mark_dirty();
        let version = buffer.version();
        app.buffers.push(buffer);
        app.in_flight_reloads.insert(
            7,
            PendingFileReload {
                request_id: 1,
                path: equivalent_path,
                version,
                force_dirty: false,
            },
        );

        assert_eq!(app.autosave_if_needed(true), 0);

        assert!(!app.in_flight_saves.contains(&7));
        assert_eq!(app.save_conflict_buffer, None);
    }

    #[test]
    fn autosave_skips_dirty_buffer_with_queued_clean_reload() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.settings.autosave = true;
        app.settings.autosave_mode = EditorAutoSaveMode::OnFocusChange;
        app.last_autosave_window_focused = true;
        app.last_autosave_focused_pane = Some(1);
        app.focused_pane = Some(2);
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "newer text".to_owned());
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.queued_file_reloads.insert(
            7,
            QueuedFileReload {
                path,
                force_dirty: false,
            },
        );

        assert_eq!(app.autosave_if_needed(true), 0);

        assert!(!app.in_flight_saves.contains(&7));
        assert_eq!(app.save_conflict_buffer, None);
    }

    #[test]
    fn autosave_allows_dirty_buffer_with_force_dirty_queued_reload() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.settings.autosave = true;
        app.settings.autosave_mode = EditorAutoSaveMode::OnFocusChange;
        app.last_autosave_window_focused = true;
        app.last_autosave_focused_pane = Some(1);
        app.focused_pane = Some(2);
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "newer text".to_owned());
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.queued_file_reloads.insert(
            7,
            QueuedFileReload {
                path: path.clone(),
                force_dirty: true,
            },
        );

        assert_eq!(app.autosave_if_needed(true), 1);

        assert!(app.in_flight_saves.contains(&7));
        assert_eq!(app.save_conflict_buffer, None);
    }

    #[test]
    fn due_completion_request_ids_follow_quick_suggest_delay() {
        let now = Instant::now();
        let pending = HashMap::from([(1, now - Duration::from_millis(49)), (2, now)]);

        assert!(due_completion_request_ids(&pending, now, Duration::from_millis(50)).is_empty());
        assert_eq!(
            due_completion_request_ids(&pending, now, Duration::from_millis(49)),
            vec![1]
        );
        assert_eq!(
            due_completion_request_ids(&pending, now, Duration::ZERO)
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>(),
            std::collections::BTreeSet::from([1, 2])
        );
    }

    #[test]
    fn due_completion_requests_dispatch_only_active_buffer() {
        let now = Instant::now();
        let pending = HashMap::from([
            (1, now - Duration::from_millis(80)),
            (2, now - Duration::from_millis(80)),
            (3, now - Duration::from_millis(10)),
        ]);

        let due = due_active_buffer_requests(&pending, now, Duration::from_millis(50), Some(2));

        assert_eq!(due.dispatch, vec![2]);
        assert_eq!(due.discard, vec![1]);

        let due_without_active =
            due_active_buffer_requests(&pending, now, Duration::from_millis(50), None);
        assert!(due_without_active.dispatch.is_empty());
        assert_eq!(due_without_active.discard, vec![1, 2]);
    }

    #[test]
    fn take_due_active_buffer_requests_removes_dispatched_and_discarded_requests() {
        let now = Instant::now();
        let mut pending = HashMap::from([
            (1, now - Duration::from_millis(80)),
            (2, now - Duration::from_millis(80)),
            (3, now - Duration::from_millis(10)),
        ]);

        let dispatch =
            take_due_active_buffer_requests(&mut pending, now, Duration::from_millis(50), Some(2));

        assert_eq!(dispatch, vec![2]);
        assert_eq!(pending.keys().copied().collect::<Vec<_>>(), vec![3]);
    }

    #[test]
    fn flush_pending_lsp_resync_hands_off_failed_did_change_paths_to_language_sync() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(path.clone()),
            "alpha".to_owned(),
        ));
        app.lsp_clients
            .insert("rust".to_owned(), LspClientHandle::full_queue_for_test());

        app.notify_lsp_change(7);

        assert!(app.pending_lsp_resync.contains(&path));
        assert!(app.pending_language_sync.is_empty());

        assert_eq!(app.flush_pending_lsp_resync(), 1);

        assert!(app.pending_lsp_resync.is_empty());
        assert!(app.pending_language_sync.contains_key(&7));

        app.lsp_clients
            .insert("rust".to_owned(), LspClientHandle::accepting_for_test());
        app.pending_language_sync
            .insert(7, Instant::now() - LANGUAGE_SYNC_DEBOUNCE);

        assert_eq!(app.flush_pending_language_sync(), 1);

        assert!(
            app.lsp_trace
                .iter()
                .any(|entry| entry.method == "textDocument/didChange")
        );
        assert!(!app.pending_lsp_resync.contains(&path));
    }

    #[test]
    fn flush_pending_lsp_resync_drops_closed_buffers_and_unavailable_servers() {
        let root = PathBuf::from("workspace");
        let open_path = root.join("src/open.rs");
        let closed_path = root.join("src/closed.rs");
        let mut app = app_for_test(root.clone());
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(open_path.clone()),
            "alpha".to_owned(),
        ));
        app.pending_lsp_resync.push_back(open_path.clone());
        app.pending_lsp_resync.push_back(closed_path);

        assert_eq!(app.flush_pending_lsp_resync(), 0);

        assert!(app.pending_lsp_resync.is_empty());
        assert!(!app.pending_language_sync.contains_key(&7));

        app.lsp_clients
            .insert("rust".to_owned(), LspClientHandle::accepting_for_test());
        app.pending_lsp_resync.push_back(open_path);

        assert_eq!(app.flush_pending_lsp_resync(), 1);
        assert!(app.pending_language_sync.contains_key(&7));
    }

    #[test]
    fn typed_lsp_trigger_requests_are_pending_until_flushed_or_requested() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        let ctx = Context::default();

        app.schedule_lsp_signature_help_for_buffer(&ctx, 7);
        app.schedule_lsp_format_on_type_for_buffer(&ctx, 7);

        assert!(app.pending_signature_help_requests.contains_key(&7));
        assert!(app.pending_format_on_type_requests.contains_key(&7));

        assert!(!app.request_lsp_signature_help_for_buffer(7, false));
        assert!(
            app.request_lsp_formatting_for_buffer(7, None, false)
                .is_none()
        );

        assert!(!app.pending_signature_help_requests.contains_key(&7));
        assert!(!app.pending_format_on_type_requests.contains_key(&7));
    }

    #[test]
    fn flush_pending_language_sync_discards_stale_missing_buffer_ids() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        app.pending_language_sync
            .insert(7, Instant::now() - LANGUAGE_SYNC_DEBOUNCE);
        app.static_diagnostics_active_request_ids.insert(7, 1);
        app.static_diagnostics_in_flight_request_ids.insert(7, 1);
        app.static_diagnostics_reload_queued.insert(7);

        assert_eq!(app.flush_pending_language_sync(), 0);

        assert!(!app.pending_language_sync.contains_key(&7));
        assert!(!app.static_diagnostics_active_request_ids.contains_key(&7));
        assert!(
            !app.static_diagnostics_in_flight_request_ids
                .contains_key(&7)
        );
        assert!(!app.static_diagnostics_reload_queued.contains(&7));
    }

    #[test]
    fn flush_pending_lsp_diagnostics_ignores_stale_server_generation() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root.clone());
        app.lsp_clients.insert(
            "rust".to_owned(),
            LspClientHandle::disconnected_with_generation_for_test(2),
        );
        app.pending_lsp_diagnostics.queue_for_server(
            PendingLspDiagnosticsSource {
                language: "rust".to_owned(),
                root,
                generation: 1,
            },
            path.clone(),
            None,
            vec![diagnostic(&path, "stale diagnostic")],
            Instant::now() - LSP_DIAGNOSTIC_BATCH_DELAY,
        );

        assert_eq!(app.flush_pending_lsp_diagnostics(), 0);

        assert!(app.diagnostics.for_path(&path).is_empty());
    }

    #[test]
    fn flush_pending_lsp_diagnostics_accepts_current_equivalent_server_root() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let event_root = root.join("src").join("..");
        let mut app = app_for_test(root);
        app.lsp_clients.insert(
            "rust".to_owned(),
            LspClientHandle::disconnected_with_generation_for_test(2),
        );
        app.pending_lsp_diagnostics.queue_for_server(
            PendingLspDiagnosticsSource {
                language: "rust".to_owned(),
                root: event_root,
                generation: 2,
            },
            path.clone(),
            None,
            vec![diagnostic(&path, "current diagnostic")],
            Instant::now() - LSP_DIAGNOSTIC_BATCH_DELAY,
        );

        assert_eq!(app.flush_pending_lsp_diagnostics(), 1);

        assert_eq!(app.diagnostics.for_path(&path).len(), 1);
    }

    #[test]
    fn flush_pending_lsp_diagnostics_converts_closed_file_payloads_once_buffer_opens() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let queued_at = Instant::now() - LSP_DIAGNOSTIC_BATCH_DELAY;

        app.pending_lsp_diagnostics.queue(
            path.clone(),
            None,
            vec![positioned_diagnostic(&path, 1, 3, 2..7, "identifier")],
            queued_at,
        );

        assert_eq!(app.flush_pending_lsp_diagnostics(), 1);

        let stored = app.diagnostics.for_path(&path);
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].column, 3);
        assert_eq!(stored[0].char_range, 2..7);

        app.buffers.push(TextBuffer::from_text(
            7,
            Some(path.clone()),
            "\u{1f600}alpha\n".to_owned(),
        ));
        assert_eq!(app.flush_pending_lsp_diagnostics(), 1);

        assert_eq!(app.flush_pending_lsp_diagnostics(), 0);

        let converted = app.diagnostics.for_path(&path);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].column, 2);
        assert_eq!(converted[0].char_range, 1..6);
    }

    #[test]
    fn flush_pending_lsp_diagnostics_clamps_multi_line_sentinel_when_converting_closed_payloads() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let queued_at = Instant::now() - LSP_DIAGNOSTIC_BATCH_DELAY;

        app.pending_lsp_diagnostics.queue(
            path.clone(),
            None,
            vec![positioned_diagnostic(
                &path,
                1,
                3,
                2..usize::MAX,
                "multi line",
            )],
            queued_at,
        );

        assert_eq!(app.flush_pending_lsp_diagnostics(), 1);
        assert_eq!(app.diagnostics.for_path(&path)[0].char_range, 2..usize::MAX);

        app.buffers.push(TextBuffer::from_text(
            7,
            Some(path.clone()),
            "alpha\nbeta\n".to_owned(),
        ));
        assert_eq!(app.flush_pending_lsp_diagnostics(), 1);

        let converted = app.diagnostics.for_path(&path);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].column, 3);
        assert_eq!(converted[0].char_range, 2..5);
    }

    #[test]
    fn flush_pending_lsp_diagnostics_keeps_coattached_server_payloads_for_same_path() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);

        app.lsp_clients.insert(
            "rust".to_owned(),
            LspClientHandle::disconnected_with_generation_for_test(10),
        );
        app.lsp_clients.insert(
            "rust\u{0}rust-analyzer-obsidian".to_owned(),
            LspClientHandle::disconnected_with_generation_for_test(11),
        );

        let queued_at = Instant::now() - LSP_DIAGNOSTIC_BATCH_DELAY;
        app.pending_lsp_diagnostics.queue_for_server(
            PendingLspDiagnosticsSource {
                language: "rust".to_owned(),
                root: PathBuf::from("workspace"),
                generation: 10,
            },
            path.clone(),
            None,
            vec![diagnostic(&path, "primary server")],
            queued_at,
        );
        app.pending_lsp_diagnostics.queue_for_server(
            PendingLspDiagnosticsSource {
                language: "rust".to_owned(),
                root: PathBuf::from("workspace"),
                generation: 11,
            },
            path.clone(),
            None,
            vec![diagnostic(&path, "sibling server")],
            queued_at,
        );

        assert_eq!(app.flush_pending_lsp_diagnostics(), 2);

        let messages = app
            .diagnostics
            .for_path(&path)
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>();
        assert!(messages.contains(&"primary server"), "{messages:?}");
        assert!(messages.contains(&"sibling server"), "{messages:?}");
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn pending_format_on_save_before_timeout_does_not_fallback() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        insert_pending_format_on_save(&mut app, path, 0);
        app.pending_format_on_save_started.insert(7, Instant::now());

        assert_eq!(app.flush_timed_out_format_on_save_requests(), 0);

        assert!(app.pending_format_on_save.contains_key(&7));
        assert!(!app.in_flight_saves.contains(&7));
    }

    #[test]
    fn format_on_save_timeout_prunes_stale_timer_metadata() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        app.pending_format_on_save_started
            .insert(7, Instant::now() - FORMAT_ON_SAVE_TIMEOUT);
        app.pending_format_on_save_retries.insert(7, 1);

        assert_eq!(app.flush_timed_out_format_on_save_requests(), 0);

        assert!(app.pending_format_on_save_started.is_empty());
        assert!(app.pending_format_on_save_retries.is_empty());
    }

    #[test]
    fn timed_out_format_on_save_retries_once_when_lsp_accepts_request() {
        let root = std::env::temp_dir().join("kuroya-format-on-save-retry");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        insert_pending_format_on_save(&mut app, path, 0);
        app.lsp_clients
            .insert("rust".to_owned(), LspClientHandle::accepting_for_test());

        assert_eq!(app.flush_timed_out_format_on_save_requests(), 1);

        let pending = app.pending_format_on_save.get(&7).expect("pending save");
        assert_ne!(pending.request_id, 21);
        assert_eq!(app.pending_format_on_save_retries.get(&7), Some(&1));
        assert!(app.canceled_formatting_request_ids.contains(&21));
        assert!(!app.in_flight_saves.contains(&7));
    }

    #[test]
    fn timed_out_format_on_save_bypasses_format_after_max_retries() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        insert_pending_format_on_save(&mut app, path, FORMAT_ON_SAVE_MAX_RETRIES);

        assert_eq!(app.flush_timed_out_format_on_save_requests(), 1);

        assert!(!app.pending_format_on_save.contains_key(&7));
        assert!(!app.pending_format_on_save_started.contains_key(&7));
        assert!(!app.pending_format_on_save_retries.contains_key(&7));
        assert!(app.canceled_formatting_request_ids.contains(&21));
        assert!(app.in_flight_saves.contains(&7));
        assert!(!app.format_on_save_bypass.contains(&7));
        assert!(app.status.starts_with("Saving "));
    }

    fn insert_pending_format_on_save(app: &mut KuroyaApp, path: PathBuf, retries: u8) {
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "fn main() {}\n".to_owned());
        buffer.mark_dirty();
        let version = buffer.version();
        app.buffers.push(buffer);
        app.pending_format_on_save.insert(
            7,
            PendingFormatOnSave {
                save_path: path.clone(),
                format_path: path,
                version,
                request_id: 21,
            },
        );
        app.pending_format_on_save_started
            .insert(7, Instant::now() - FORMAT_ON_SAVE_TIMEOUT);
        app.pending_format_on_save_retries.insert(7, retries);
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

    fn positioned_diagnostic(
        path: &std::path::Path,
        line: usize,
        column: usize,
        range: std::ops::Range<usize>,
        message: &str,
    ) -> Diagnostic {
        Diagnostic {
            line,
            column,
            char_range: range,
            ..diagnostic(path, message)
        }
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
