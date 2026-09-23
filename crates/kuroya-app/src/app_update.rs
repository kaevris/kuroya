mod panels;

use crate::{
    KuroyaApp, devtools_repaint_diagnostics::RepaintFrameActivity, fonts, persistence, theme,
    ui_event_channel, workspace_state::paths_match_lexically,
};
use eframe::egui::Context;
use kuroya_core::window_zoom_factor;
use std::{mem, path::PathBuf, time::Instant};

impl KuroyaApp {
    pub(crate) fn drain_terminal_output_for_frame(&mut self) -> (usize, bool) {
        let terminal_events = self.terminal.drain_output();
        if !self.running_workspace_tasks.is_empty() {
            self.prune_finished_workspace_tasks();
        }
        let terminal_output_pending = self.terminal.has_pending_output();
        (terminal_events, terminal_output_pending)
    }
}

impl eframe::App for KuroyaApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Ingest status toasts before anything renders so the toast pipeline
        // stays alive even while the status bar (the render-side ingest) is
        // hidden.
        self.ingest_status_toast();
        let frame_start = Instant::now();
        let profiling = self.profiling_enabled();
        let mut profile_mark = frame_start;
        let zoom_factor = window_zoom_factor(self.settings.window_zoom_level);
        if (ctx.zoom_factor() - zoom_factor).abs() > f32::EPSILON {
            ctx.set_zoom_factor(zoom_factor);
        }
        self.handle_close_request(ctx);
        if self.fonts_dirty {
            fonts::install_fonts(ctx, &self.workspace.root, &self.settings);
            fonts::apply_typography(ctx, &self.settings);
            self.fonts_dirty = false;
        }
        if self.theme_dirty {
            theme::apply_theme(ctx, &self.settings.theme);
            self.theme_dirty = false;
        }
        self.terminal.set_repaint_context(ctx.clone());
        self.record_profile_mark(profiling, &mut profile_mark, "frame", "setup");
        let ui_events = self.handle_events_with_context(ctx);
        if self.exit_confirmed {
            if !self.launch_pending_update_installer_before_exit() {
                return;
            }
            self.exit_process();
        }
        self.dispatch_shortcuts(ctx);
        self.handle_editor_file_drops(ctx);
        self.record_profile_mark(profiling, &mut profile_mark, "frame", "events");
        let (terminal_events, terminal_output_pending) = self.drain_terminal_output_for_frame();
        let filesystem_changes = self.drain_file_watcher();
        let workspace_refreshes = self.flush_pending_workspace_refresh();
        let plugin_reloads = self.flush_pending_workspace_plugin_reload();
        let window_focused = ctx.input(|input| input.focused);
        let autosaves = self.autosave_if_needed(window_focused);
        let session_save_requested = self.persist_session_if_needed();
        let language_syncs = self.flush_pending_language_sync();
        let completion_requests = self.flush_pending_completion_requests();
        let signature_help_requests = self.flush_pending_signature_help_requests();
        let format_on_type_requests = self.flush_pending_format_on_type_requests();
        let format_on_save_timeouts = self.flush_timed_out_format_on_save_requests();
        let lsp_restarts = self.flush_pending_lsp_restarts();
        let lsp_diagnostics = self.flush_pending_lsp_diagnostics();
        let lsp_symbol_refreshes = self.flush_pending_lsp_symbol_refreshes();
        let update_checks = self.flush_due_update_checks(frame_start);
        self.record_profile_mark(profiling, &mut profile_mark, "frame", "runtime");
        let commands = self.drain_commands(ctx);
        self.sync_background_image(false);
        self.update_discord_presence();
        self.record_profile_mark(profiling, &mut profile_mark, "frame", "commands");
        let dropped_ui_events = ui_event_channel::take_dropped_ui_event_count();

        self.render_background_image(ctx);
        self.render_main_panels(ctx);
        self.render_active_overlays(ctx);
        self.record_profile_mark(profiling, &mut profile_mark, "frame", "render");
        let update_duration = frame_start.elapsed();
        if profiling {
            self.record_profile_sample("frame", "total", update_duration);
        }
        let startup_warmup = self.startup_repaint_warmup_active();
        // Frame timing stays unconditional: the GPU acceleration lag
        // detector consumes `frame_timings` even when devtools are closed.
        self.record_frame_timing(update_duration);
        self.maybe_show_gpu_acceleration_prompt();
        self.maybe_show_lsp_enable_prompt();
        let activity = RepaintFrameActivity {
            ui_events,
            dropped_ui_events,
            terminal_events,
            filesystem_changes,
            commands,
            language_syncs: language_syncs
                .saturating_add(completion_requests)
                .saturating_add(signature_help_requests)
                .saturating_add(format_on_type_requests)
                .saturating_add(format_on_save_timeouts)
                .saturating_add(lsp_symbol_refreshes),
            update_checks,
            lsp_restarts,
            lsp_diagnostics,
            workspace_refreshes,
            plugin_reloads,
            autosaves,
            session_save_requested,
            startup_warmup,
        };
        let repaint_after = self.next_frame_repaint_after(
            Instant::now(),
            activity,
            terminal_output_pending,
            profiling,
        );
        // Repaint samples are devtools/profiling data; skip recording them
        // on frames nobody observes, but keep the per-frame warmup counter
        // advancing so startup warmup still ends after its frame budget.
        if self.devtools_open || profiling {
            self.record_repaint_diagnostics(activity, update_duration, repaint_after);
        } else {
            self.advance_startup_warmup_frame_counter();
        }
        if repaint_after.is_zero() {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(repaint_after);
        }
    }

    fn on_exit(&mut self) {
        self.prepare_shutdown();
    }
}

impl KuroyaApp {
    fn exit_process(&mut self) -> ! {
        self.prepare_shutdown();
        std::process::exit(0);
    }

    /// Applies the startup session loaded off-thread by
    /// `spawn_startup_session_load`. This is the deferred counterpart of the
    /// synchronous startup sequence that used to run in `KuroyaApp::new`:
    /// restore the session (or surface the load warning), then handle the
    /// startup target that was held back so it always runs on top of the
    /// restored state (opening a folder re-saves the restored session for
    /// the previous workspace, so it must observe the restored state), then
    /// arm the workspace trust prompt in the same relative order.
    pub(crate) fn apply_startup_session_loaded(
        &mut self,
        startup_root: PathBuf,
        target: Option<crate::startup_arguments::StartupTarget>,
        session: Option<Box<persistence::PersistedSession>>,
        warning: Option<String>,
    ) {
        if self.workspace_placeholder || !paths_match_lexically(&self.workspace.root, &startup_root)
        {
            // The placeholder startup path never restores a session, and an
            // event for a workspace the app already left (startup folder
            // target) is stale.
            return;
        }
        if let Some(session) = session {
            self.restore_session(*session);
            // The startup git scan may have completed before this event; the
            // restored source-control loads gate on a scan, so redrive them
            // here when the scan already landed (this is a no-op otherwise
            // and the next scan drains them as before).
            self.drain_pending_restored_source_control_loads();
        } else if let Some(warning) = warning {
            self.status = warning;
        }
        match target {
            Some(crate::startup_arguments::StartupTarget::File(path)) => {
                self.spawn_open_file(path);
                self.arm_workspace_trust_prompt();
            }
            Some(crate::startup_arguments::StartupTarget::Folder(path)) => {
                self.open_workspace_now(path);
                self.arm_workspace_trust_prompt();
            }
            None => {}
        }
    }

    fn prepare_shutdown(&mut self) {
        if self.shutdown_prepared {
            return;
        }
        self.shutdown_prepared = true;
        self.notify_lsp_close_all();
        for client in self.lsp_clients.values() {
            client.shutdown();
        }
        self.abort_session_save_in_flight_for_shutdown();
        // Clear the Discord presence before the slower shutdown steps; the
        // wait is bounded (~500ms) so exit never blocks on a hung pipe.
        self.shutdown_discord_presence();
        let _ = self.save_app_state();
        let _ = self.terminal.drain_output_for_shutdown();
        if !self.workspace_placeholder {
            let _ = persistence::save_session(&self.workspace.root, &self.build_session());
        }
        self.flush_in_flight_session_save_for_shutdown();
        self.flush_queued_session_saves_for_shutdown();
        self.terminal.close_all_sessions_for_shutdown();
    }

    fn abort_session_save_in_flight_for_shutdown(&mut self) {
        if let Some(task) = self.session_save_in_flight_task.take() {
            task.abort();
            let _ = self.runtime.block_on(task);
        }
    }

    fn flush_in_flight_session_save_for_shutdown(&mut self) {
        let Some(root) = self.session_save_in_flight.take() else {
            self.session_save_in_flight_snapshot = None;
            return;
        };
        let Some(session) = self.session_save_in_flight_snapshot.take() else {
            return;
        };
        if !self.workspace_placeholder
            && crate::workspace_state::paths_match_lexically(&root, &self.workspace.root)
        {
            return;
        }
        let _ = persistence::save_session(&root, &session.into_persisted_session());
    }

    fn flush_queued_session_saves_for_shutdown(&mut self) {
        let current_root = self.workspace.root.clone();
        for (root, session) in mem::take(&mut self.queued_session_saves) {
            if !self.workspace_placeholder
                && crate::workspace_state::paths_match_lexically(&root, &current_root)
            {
                continue;
            }
            let _ = persistence::save_session(&root, &session.into_persisted_session());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app_startup_context::AppStartupContext, persistence::PersistedSession,
        startup_arguments::StartupTarget, terminal::TerminalPane,
        ui_event_channel::send_critical_ui_event, ui_events::UiEvent,
    };
    use kuroya_core::{EditorSettings, Workspace};
    use std::{
        fs,
        path::PathBuf,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };
    use tokio::runtime::Runtime;

    #[test]
    fn shutdown_flushes_queued_session_saves_for_other_workspaces() {
        let old_root = temp_root("shutdown-queued-old");
        let current_root = temp_root("shutdown-queued-current");
        fs::create_dir_all(&old_root).unwrap();
        fs::create_dir_all(&current_root).unwrap();
        let mut app = app_for_test(old_root.clone());
        app.source_control_commit_message = "queued old workspace".to_owned();
        let queued_old = app.build_session_save_snapshot();
        app.workspace = Workspace::new(current_root.clone());
        app.source_control_commit_message = "current workspace".to_owned();
        app.queued_session_saves
            .insert(old_root.clone(), queued_old);

        app.prepare_shutdown();

        let old_session = PersistedSession::load(&old_root).unwrap().unwrap();
        let current_session = PersistedSession::load(&current_root).unwrap().unwrap();
        assert_eq!(
            old_session.source_control_commit_message,
            "queued old workspace"
        );
        assert_eq!(
            current_session.source_control_commit_message,
            "current workspace"
        );
        assert!(app.queued_session_saves.is_empty());

        fs::remove_dir_all(old_root).unwrap();
        fs::remove_dir_all(current_root).unwrap();
    }

    #[test]
    fn shutdown_skips_queued_session_for_current_workspace() {
        let root = temp_root("shutdown-queued-current");
        fs::create_dir_all(&root).unwrap();
        let mut app = app_for_test(root.clone());
        app.source_control_commit_message = "queued stale current".to_owned();
        let queued_current = app.build_session_save_snapshot();
        app.source_control_commit_message = "fresh current".to_owned();
        app.queued_session_saves
            .insert(root.clone(), queued_current);

        app.prepare_shutdown();

        let session = PersistedSession::load(&root).unwrap().unwrap();
        assert_eq!(session.source_control_commit_message, "fresh current");
        assert!(app.queued_session_saves.is_empty());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shutdown_flushes_in_flight_session_save_for_other_workspace() {
        let old_root = temp_root("shutdown-in-flight-old");
        let current_root = temp_root("shutdown-in-flight-current");
        fs::create_dir_all(&old_root).unwrap();
        fs::create_dir_all(&current_root).unwrap();
        let mut app = app_for_test(old_root.clone());
        app.source_control_commit_message = "in-flight old workspace".to_owned();
        let in_flight_old = app.build_session_save_snapshot();
        app.session_save_in_flight = Some(old_root.clone());
        app.session_save_in_flight_snapshot = Some(in_flight_old);
        app.workspace = Workspace::new(current_root.clone());
        app.source_control_commit_message = "current workspace".to_owned();

        app.prepare_shutdown();

        let old_session = PersistedSession::load(&old_root).unwrap().unwrap();
        let current_session = PersistedSession::load(&current_root).unwrap().unwrap();
        assert_eq!(
            old_session.source_control_commit_message,
            "in-flight old workspace"
        );
        assert_eq!(
            current_session.source_control_commit_message,
            "current workspace"
        );
        assert!(app.session_save_in_flight.is_none());
        assert!(app.session_save_in_flight_snapshot.is_none());

        fs::remove_dir_all(old_root).unwrap();
        fs::remove_dir_all(current_root).unwrap();
    }

    #[test]
    fn shutdown_skips_in_flight_session_save_for_current_workspace() {
        let root = temp_root("shutdown-in-flight-current");
        fs::create_dir_all(&root).unwrap();
        let mut app = app_for_test(root.clone());
        app.source_control_commit_message = "in-flight stale current".to_owned();
        let in_flight_current = app.build_session_save_snapshot();
        app.session_save_in_flight = Some(root.clone());
        app.session_save_in_flight_snapshot = Some(in_flight_current);
        app.source_control_commit_message = "fresh current".to_owned();

        app.prepare_shutdown();

        let session = PersistedSession::load(&root).unwrap().unwrap();
        assert_eq!(session.source_control_commit_message, "fresh current");
        assert!(app.session_save_in_flight.is_none());
        assert!(app.session_save_in_flight_snapshot.is_none());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shutdown_waits_for_aborted_current_session_save_before_final_save() {
        let root = temp_root("shutdown-abort-current-save-task");
        fs::create_dir_all(&root).unwrap();
        let mut app = app_for_test(root.clone());
        app.source_control_commit_message = "stale async current".to_owned();
        let stale_session = app.build_session_save_snapshot().into_persisted_session();
        app.session_save_in_flight = Some(root.clone());
        app.session_save_in_flight_snapshot = Some(app.build_session_save_snapshot());
        let stale_root = root.clone();
        app.session_save_in_flight_task = Some(app.runtime.spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let _ = persistence::save_session_async(stale_root, stale_session).await;
        }));
        app.source_control_commit_message = "fresh current".to_owned();

        app.prepare_shutdown();
        std::thread::sleep(Duration::from_millis(100));

        let session = PersistedSession::load(&root).unwrap().unwrap();
        assert_eq!(session.source_control_commit_message, "fresh current");
        assert!(app.session_save_in_flight_task.is_none());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_session_loaded_event_restores_deferred_session() {
        let root = temp_root("deferred-restore");
        fs::create_dir_all(&root).unwrap();
        let restored = root.join("src/lib.rs");
        fs::create_dir_all(restored.parent().unwrap()).unwrap();
        fs::write(&restored, "pub fn lib() {}\n").unwrap();
        let other_project = temp_root("deferred-restore-other");
        fs::create_dir_all(&other_project).unwrap();
        let mut app = app_for_test(root.clone());
        let session = PersistedSession {
            workspace_root: root.clone(),
            open_files: vec![restored.clone()],
            active_path: Some(restored.clone()),
            recent_projects: vec![other_project.clone()],
            ..PersistedSession::default()
        };

        assert!(send_critical_ui_event(
            &app.tx,
            UiEvent::StartupSessionLoaded {
                root: root.clone(),
                target: None,
                session: Some(Box::new(session)),
                warning: None,
            },
        ));
        app.handle_events();

        // The restored open file is queued for its (async) load, the session
        // active path waits on that load, and the session's recent projects
        // were merged with the current root.
        assert!(app.pending_open_paths.contains(&restored));
        assert_eq!(app.pending_active_path, Some(restored));
        assert!(app.recent_projects.contains(&other_project));
        assert!(app.recent_projects.contains(&root));

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(other_project).unwrap();
    }

    #[test]
    fn startup_session_loaded_event_ignores_stale_workspace_root() {
        let root = temp_root("deferred-restore-stale");
        fs::create_dir_all(&root).unwrap();
        let other_root = temp_root("deferred-restore-stale-other");
        fs::create_dir_all(&other_root).unwrap();
        let mut app = app_for_test(root.clone());

        assert!(send_critical_ui_event(
            &app.tx,
            UiEvent::StartupSessionLoaded {
                root: other_root.clone(),
                target: None,
                session: Some(Box::new(PersistedSession {
                    workspace_root: other_root.clone(),
                    recent_projects: vec![other_root.clone()],
                    ..PersistedSession::default()
                })),
                warning: None,
            },
        ));
        app.handle_events();

        assert!(app.pending_open_paths.is_empty());
        assert!(!app.recent_projects.contains(&other_root));

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(other_root).unwrap();
    }

    #[test]
    fn startup_session_loaded_event_surfaces_warning_without_session() {
        let root = temp_root("deferred-restore-warning");
        fs::create_dir_all(&root).unwrap();
        let mut app = app_for_test(root.clone());

        assert!(send_critical_ui_event(
            &app.tx,
            UiEvent::StartupSessionLoaded {
                root: root.clone(),
                target: None,
                session: None,
                warning: Some("Could not load saved session: boom".to_owned()),
            },
        ));
        app.handle_events();

        assert_eq!(app.status, "Could not load saved session: boom");
        assert!(app.pending_open_paths.is_empty());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_session_loaded_event_opens_startup_target_after_restore() {
        let root = temp_root("deferred-restore-target");
        fs::create_dir_all(&root).unwrap();
        let restored = root.join("restored.rs");
        fs::write(&restored, "restored\n").unwrap();
        let target = root.join("target.rs");
        fs::write(&target, "target\n").unwrap();
        let mut app = app_for_test(root.clone());
        let session = PersistedSession {
            workspace_root: root.clone(),
            open_files: vec![restored.clone()],
            active_path: Some(restored.clone()),
            ..PersistedSession::default()
        };

        assert!(send_critical_ui_event(
            &app.tx,
            UiEvent::StartupSessionLoaded {
                root: root.clone(),
                target: Some(StartupTarget::File(target.clone())),
                session: Some(Box::new(session)),
                warning: None,
            },
        ));
        app.handle_events();

        // The session state was applied first, then the deferred startup
        // target was queued on top of it — the synchronous startup order.
        assert!(app.pending_open_paths.contains(&restored));
        assert!(app.pending_open_paths.contains(&target));

        fs::remove_dir_all(root).unwrap();
    }

    fn app_for_test(root: PathBuf) -> KuroyaApp {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let settings = EditorSettings::default();
        let mut app = KuroyaApp::from_startup_context(AppStartupContext {
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
            trusted_workspaces: vec![root.clone()],
            now: Instant::now(),
            startup_timings: Vec::new(),
        });
        app.app_state_path_override = Some(root.join("app-state.json"));
        app
    }

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "kuroya-app-update-{name}-{}-{nanos}",
            std::process::id()
        ))
    }
}
