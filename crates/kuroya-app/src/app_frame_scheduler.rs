use crate::{
    KuroyaApp,
    devtools_repaint_diagnostics::RepaintFrameActivity,
    devtools_trace_id::next_devtools_trace_id,
    lsp_diagnostics_batch::LSP_DIAGNOSTIC_BATCH_DELAY,
    lsp_lifecycle::LANGUAGE_SYNC_DEBOUNCE,
    lsp_runtime::LSP_SYMBOL_REFRESH_DEBOUNCE,
    runtime_ticks::{FORMAT_ON_TYPE_DEBOUNCE, SIGNATURE_HELP_DEBOUNCE},
    transient_state::{
        PendingExit, PendingSourceControlCommitSave, PendingSourceControlStashSave,
        PendingWorkspaceSwitch,
    },
    update_checker::automatic_update_wakeup_after,
};
use kuroya_core::{EditorAutoSaveMode, clamp_autosave_delay_ms, clamp_quick_suggestions_delay_ms};
use std::time::{Duration, Instant};

pub(crate) const ACTIVE_REPAINT_INTERVAL: Duration = Duration::from_millis(16);
pub(crate) const DEVTOOLS_REPAINT_INTERVAL: Duration = Duration::from_millis(80);
pub(crate) const PENDING_FORMAT_SAVE_REPAINT_INTERVAL: Duration = Duration::from_millis(80);
pub(crate) const SESSION_SAVE_INTERVAL: Duration = Duration::from_secs(2);
pub(crate) const STARTUP_REPAINT_WARMUP_FRAMES: u64 = 12;
/// Upper bound on how long an idle window sleeps between repaints. Runtime
/// work either schedules its own wakeup (debounces, save dialogs, update
/// checks) or arrives through the UI event wake hook, so this is only a
/// safety net for anything unscheduled — not a heartbeat.
const MAX_REPAINT_AFTER: Duration = Duration::from_secs(30);

impl KuroyaApp {
    pub(crate) fn startup_repaint_warmup_active(&self) -> bool {
        startup_repaint_warmup_active_for_frame(self.next_repaint_diagnostic_id)
    }

    /// Counts one rendered frame toward startup repaint warmup without
    /// recording a repaint diagnostic sample; `record_repaint_diagnostics`
    /// advances the same counter when it records a sample.
    pub(crate) fn advance_startup_warmup_frame_counter(&mut self) {
        next_devtools_trace_id(&mut self.next_repaint_diagnostic_id);
    }

    pub(crate) fn next_frame_repaint_after(
        &self,
        now: Instant,
        activity: RepaintFrameActivity,
        terminal_output_pending: bool,
        profiling: bool,
    ) -> Duration {
        if immediate_frame_repaint_needed(activity, terminal_output_pending) {
            return Duration::ZERO;
        }

        let runtime_delay = self.next_runtime_wakeup_after(
            now,
            session_save_wake_due(activity, self.session_save_persisted_changes),
        );
        non_immediate_frame_repaint_after(activity, self.devtools_open || profiling, runtime_delay)
    }

    fn next_runtime_wakeup_after(&self, now: Instant, session_save_wake_due: bool) -> Duration {
        let mut next = session_save_wake_due.then(|| {
            session_save_wakeup_after(self.workspace_placeholder, self.last_session_save, now)
        });
        if next.is_some_and(|delay| delay.is_zero()) {
            return Duration::ZERO;
        }

        if record_earliest(
            &mut next,
            debounced_wakeup_after(
                self.pending_language_sync.values().copied(),
                now,
                LANGUAGE_SYNC_DEBOUNCE,
            ),
        ) {
            return Duration::ZERO;
        }
        if record_earliest(
            &mut next,
            debounced_wakeup_after(
                self.pending_completion_requests.values().copied(),
                now,
                Duration::from_millis(clamp_quick_suggestions_delay_ms(
                    self.settings.quick_suggestions_delay_ms,
                ) as u64),
            ),
        ) {
            return Duration::ZERO;
        }
        if record_earliest(
            &mut next,
            debounced_wakeup_after(
                self.pending_signature_help_requests.values().copied(),
                now,
                SIGNATURE_HELP_DEBOUNCE,
            ),
        ) {
            return Duration::ZERO;
        }
        if record_earliest(
            &mut next,
            debounced_wakeup_after(
                self.pending_format_on_type_requests.values().copied(),
                now,
                FORMAT_ON_TYPE_DEBOUNCE,
            ),
        ) {
            return Duration::ZERO;
        }
        if record_earliest(
            &mut next,
            absolute_wakeup_after(self.pending_lsp_restarts.values().copied(), now),
        ) {
            return Duration::ZERO;
        }
        if record_earliest(
            &mut next,
            debounced_wakeup_after(
                self.pending_lsp_symbol_refreshes.values().copied(),
                now,
                LSP_SYMBOL_REFRESH_DEBOUNCE,
            ),
        ) {
            return Duration::ZERO;
        }
        if record_earliest(
            &mut next,
            self.pending_lsp_diagnostics
                .next_due_after(now, LSP_DIAGNOSTIC_BATCH_DELAY),
        ) {
            return Duration::ZERO;
        }
        if record_earliest(
            &mut next,
            pending_format_save_wakeup_after(!self.pending_format_on_save.is_empty()),
        ) {
            return Duration::ZERO;
        }
        if record_earliest(
            &mut next,
            pending_source_control_save_wakeup_after(
                matches!(
                    self.pending_source_control_commit_save,
                    Some(PendingSourceControlCommitSave::Saving { .. })
                ) || matches!(
                    self.pending_source_control_stash_save,
                    Some(PendingSourceControlStashSave::Saving { .. })
                ),
            ),
        ) {
            return Duration::ZERO;
        }
        if record_earliest(
            &mut next,
            pending_guard_save_wakeup_after(
                matches!(
                    self.pending_workspace_switch,
                    Some(PendingWorkspaceSwitch::Saving { .. })
                ) || matches!(self.pending_exit, Some(PendingExit::Saving { .. })),
            ),
        ) {
            return Duration::ZERO;
        }
        if record_earliest(&mut next, self.autosave_wakeup_after(now)) {
            return Duration::ZERO;
        }
        if record_earliest(
            &mut next,
            automatic_update_wakeup_after(
                self.next_automatic_update_check_at,
                now,
                self.update_check_in_flight
                    || self.update_download_in_flight
                    || self.available_update.is_some()
                    || self.pending_update_install.is_some(),
            ),
        ) {
            return Duration::ZERO;
        }
        if record_earliest(
            &mut next,
            self.workspace_refresh_wakeup()
                .map(|due| delay_until(now, due)),
        ) {
            return Duration::ZERO;
        }
        if record_earliest(
            &mut next,
            self.workspace_plugin_reload_wakeup()
                .map(|due| delay_until(now, due)),
        ) {
            return Duration::ZERO;
        }
        // No scheduled runtime work: let the window sleep until the safety
        // poll bound instead of heartbeating for an unchanged session.
        next.unwrap_or(MAX_REPAINT_AFTER)
    }

    fn autosave_wakeup_after(&self, now: Instant) -> Option<Duration> {
        if self.settings.effective_autosave_mode() != EditorAutoSaveMode::AfterDelay {
            return None;
        }
        let delay = Duration::from_millis(clamp_autosave_delay_ms(self.settings.autosave_delay_ms));
        Some(delayed_wakeup_after(self.last_autosave, now, delay))
    }
}

#[cfg(test)]
pub(crate) fn frame_repaint_after(
    activity: RepaintFrameActivity,
    terminal_output_pending: bool,
    devtools_live: bool,
    runtime_delay: Duration,
) -> Duration {
    if immediate_frame_repaint_needed(activity, terminal_output_pending) {
        Duration::ZERO
    } else {
        non_immediate_frame_repaint_after(activity, devtools_live, runtime_delay)
    }
}

fn non_immediate_frame_repaint_after(
    activity: RepaintFrameActivity,
    devtools_live: bool,
    runtime_delay: Duration,
) -> Duration {
    let runtime_delay = bounded_repaint_after(runtime_delay);
    if activity.keeps_frame_active() {
        runtime_delay.min(ACTIVE_REPAINT_INTERVAL)
    } else if devtools_live {
        runtime_delay.min(DEVTOOLS_REPAINT_INTERVAL)
    } else {
        runtime_delay
    }
}

fn bounded_repaint_after(delay: Duration) -> Duration {
    delay.min(MAX_REPAINT_AFTER)
}

fn immediate_frame_repaint_needed(
    activity: RepaintFrameActivity,
    terminal_output_pending: bool,
) -> bool {
    terminal_output_pending || activity.needs_immediate_repaint()
}

fn debounced_wakeup_after(
    scheduled: impl Iterator<Item = Instant>,
    now: Instant,
    delay: Duration,
) -> Option<Duration> {
    earliest_wakeup_after(
        scheduled.filter_map(|instant| instant.checked_add(delay)),
        now,
    )
}

fn absolute_wakeup_after(
    due_times: impl Iterator<Item = Instant>,
    now: Instant,
) -> Option<Duration> {
    earliest_wakeup_after(due_times, now)
}

fn delayed_wakeup_after(scheduled: Instant, now: Instant, delay: Duration) -> Duration {
    scheduled
        .checked_add(delay)
        .map(|due| delay_until(now, due))
        .unwrap_or(MAX_REPAINT_AFTER)
}

fn session_save_wakeup_after(
    workspace_placeholder: bool,
    last_session_save: Instant,
    now: Instant,
) -> Duration {
    if workspace_placeholder {
        return SESSION_SAVE_INTERVAL;
    }
    delayed_wakeup_after(last_session_save, now, SESSION_SAVE_INTERVAL)
}

/// The session-save heartbeat only stays armed while session state may hold
/// unsaved changes: either the last save tick staged a save, or this frame
/// ran runtime activity that could have touched session state.
fn session_save_wake_due(
    activity: RepaintFrameActivity,
    session_save_persisted_changes: bool,
) -> bool {
    session_save_persisted_changes || activity.keeps_frame_active()
}

fn pending_format_save_wakeup_after(has_pending_format_save: bool) -> Option<Duration> {
    pending_save_wakeup_after(has_pending_format_save)
}

fn pending_source_control_save_wakeup_after(has_source_control_save: bool) -> Option<Duration> {
    pending_save_wakeup_after(has_source_control_save)
}

fn pending_guard_save_wakeup_after(has_guard_save: bool) -> Option<Duration> {
    pending_save_wakeup_after(has_guard_save)
}

fn pending_save_wakeup_after(has_pending_save: bool) -> Option<Duration> {
    has_pending_save.then_some(PENDING_FORMAT_SAVE_REPAINT_INTERVAL)
}

fn earliest_wakeup_after(
    due_times: impl Iterator<Item = Instant>,
    now: Instant,
) -> Option<Duration> {
    due_times.map(|due| delay_until(now, due)).min()
}

fn record_earliest(next: &mut Option<Duration>, candidate: Option<Duration>) -> bool {
    if let Some(candidate) = candidate {
        *next = Some(next.map_or(candidate, |current| current.min(candidate)));
        candidate.is_zero()
    } else {
        false
    }
}

fn delay_until(now: Instant, due: Instant) -> Duration {
    due.saturating_duration_since(now)
}

fn startup_repaint_warmup_active_for_frame(next_repaint_diagnostic_id: u64) -> bool {
    next_repaint_diagnostic_id < STARTUP_REPAINT_WARMUP_FRAMES
}

#[cfg(test)]
mod tests {
    use super::{
        ACTIVE_REPAINT_INTERVAL, DEVTOOLS_REPAINT_INTERVAL, MAX_REPAINT_AFTER,
        PENDING_FORMAT_SAVE_REPAINT_INTERVAL, SESSION_SAVE_INTERVAL, STARTUP_REPAINT_WARMUP_FRAMES,
        absolute_wakeup_after, debounced_wakeup_after, delayed_wakeup_after, frame_repaint_after,
        immediate_frame_repaint_needed, pending_format_save_wakeup_after,
        pending_guard_save_wakeup_after, pending_source_control_save_wakeup_after,
        session_save_wake_due, session_save_wakeup_after,
    };
    use crate::{
        KuroyaApp, app_startup_context::AppStartupContext,
        devtools_repaint_diagnostics::RepaintFrameActivity,
        startup_tasks::WORKSPACE_PLUGIN_RELOAD_DEBOUNCE, terminal::TerminalPane,
    };
    use kuroya_core::{EditorSettings, Workspace};
    use std::{
        path::PathBuf,
        time::{Duration, Instant},
    };
    use tokio::runtime::Runtime;

    #[test]
    fn runtime_wakeup_wakes_for_pending_workspace_refresh() {
        let mut app = app_for_test(PathBuf::from("workspace"));
        app.schedule_workspace_refresh();

        assert_eq!(
            app.next_runtime_wakeup_after(Instant::now(), false),
            Duration::ZERO
        );
    }

    #[test]
    fn runtime_wakeup_bounds_pending_plugin_reload_rescheduling() {
        let mut app = app_for_test(PathBuf::from("workspace"));
        let now = Instant::now();
        app.schedule_workspace_plugin_reload();
        for _ in 0..8 {
            app.schedule_workspace_plugin_reload();
            let delay = app.next_runtime_wakeup_after(now, false);
            assert!(delay <= WORKSPACE_PLUGIN_RELOAD_DEBOUNCE, "{delay:?}");
        }
    }

    #[test]
    fn clean_idle_session_does_not_contribute_session_save_wakeup() {
        let mut app = app_for_test(PathBuf::from("workspace"));
        app.settings.autosave = false;
        app.next_automatic_update_check_at = Instant::now() + Duration::from_secs(3_600);
        app.session_save_persisted_changes = false;
        app.last_session_save = Instant::now() - Duration::from_secs(30);

        assert!(
            app.next_runtime_wakeup_after(Instant::now(), false) > SESSION_SAVE_INTERVAL,
            "a session whose last save persisted nothing must not arm the heartbeat"
        );
    }

    #[test]
    fn dirty_session_still_contributes_session_save_wakeup() {
        let mut app = app_for_test(PathBuf::from("workspace"));
        app.settings.autosave = false;
        app.next_automatic_update_check_at = Instant::now() + Duration::from_secs(3_600);
        app.session_save_persisted_changes = true;
        app.last_session_save = Instant::now() - Duration::from_secs(30);

        assert_eq!(
            app.next_runtime_wakeup_after(Instant::now(), true),
            Duration::ZERO
        );
    }

    #[test]
    fn idle_clean_session_sleeps_past_the_session_save_interval() {
        let mut app = app_for_test(PathBuf::from("workspace"));
        app.settings.autosave = false;
        app.next_automatic_update_check_at = Instant::now() + Duration::from_secs(3_600);
        app.session_save_persisted_changes = false;
        app.last_session_save = Instant::now() - Duration::from_secs(30);

        let repaint_after = app.next_frame_repaint_after(
            Instant::now(),
            RepaintFrameActivity::default(),
            false,
            false,
        );

        assert!(
            repaint_after > SESSION_SAVE_INTERVAL,
            "expected the idle window to sleep past the heartbeat, got {repaint_after:?}"
        );
    }

    #[test]
    fn runtime_activity_keeps_session_save_wakeup_armed() {
        let active = RepaintFrameActivity {
            commands: 1,
            ..RepaintFrameActivity::default()
        };

        assert!(session_save_wake_due(active, false));
        assert!(session_save_wake_due(RepaintFrameActivity::default(), true));
        assert!(!session_save_wake_due(
            RepaintFrameActivity::default(),
            false
        ));
    }

    #[test]
    fn frame_repaint_is_immediate_while_terminal_output_is_pending() {
        let active = RepaintFrameActivity {
            commands: 1,
            ..RepaintFrameActivity::default()
        };

        assert_eq!(
            frame_repaint_after(active, true, true, Duration::from_secs(2)),
            Duration::ZERO
        );
        assert!(immediate_frame_repaint_needed(active, true));
    }

    #[test]
    fn frame_repaint_is_immediate_after_ui_event_backpressure() {
        let backpressure = RepaintFrameActivity {
            dropped_ui_events: 1,
            ..RepaintFrameActivity::default()
        };

        assert_eq!(
            frame_repaint_after(backpressure, false, true, Duration::from_secs(2)),
            Duration::ZERO
        );
        assert!(immediate_frame_repaint_needed(backpressure, false));
    }

    #[test]
    fn idle_frame_repaint_is_immediate_when_runtime_work_is_due() {
        assert_eq!(
            frame_repaint_after(
                RepaintFrameActivity::default(),
                false,
                false,
                Duration::ZERO,
            ),
            Duration::ZERO
        );
    }

    #[test]
    fn frame_repaint_caps_active_frames_without_hiding_nearer_runtime_work() {
        let active = RepaintFrameActivity {
            commands: 1,
            ..RepaintFrameActivity::default()
        };

        assert_eq!(
            frame_repaint_after(active, false, false, Duration::from_secs(2)),
            ACTIVE_REPAINT_INTERVAL
        );
        assert_eq!(
            frame_repaint_after(active, false, false, Duration::from_millis(4)),
            Duration::from_millis(4)
        );
    }

    #[test]
    fn frame_repaint_keeps_startup_warmup_responsive_without_hiding_runtime_work() {
        let startup = RepaintFrameActivity {
            startup_warmup: true,
            ..RepaintFrameActivity::default()
        };

        assert_eq!(
            frame_repaint_after(startup, false, false, Duration::from_secs(2)),
            ACTIVE_REPAINT_INTERVAL
        );
        assert_eq!(
            frame_repaint_after(startup, false, false, Duration::from_millis(3)),
            Duration::from_millis(3)
        );
    }

    #[test]
    fn startup_repaint_warmup_is_bounded_by_recorded_frame_count() {
        assert!(super::startup_repaint_warmup_active_for_frame(0));
        assert!(super::startup_repaint_warmup_active_for_frame(
            STARTUP_REPAINT_WARMUP_FRAMES - 1
        ));
        assert!(!super::startup_repaint_warmup_active_for_frame(
            STARTUP_REPAINT_WARMUP_FRAMES
        ));
    }

    #[test]
    fn frame_repaint_keeps_devtools_live_without_forcing_idle_heartbeat() {
        assert_eq!(
            frame_repaint_after(
                RepaintFrameActivity::default(),
                false,
                true,
                Duration::from_secs(2),
            ),
            DEVTOOLS_REPAINT_INTERVAL
        );
        assert_eq!(
            frame_repaint_after(
                RepaintFrameActivity::default(),
                false,
                true,
                Duration::from_millis(12),
            ),
            Duration::from_millis(12)
        );
        assert_eq!(
            frame_repaint_after(
                RepaintFrameActivity::default(),
                false,
                false,
                Duration::from_secs(2),
            ),
            Duration::from_secs(2)
        );
    }

    #[test]
    fn idle_frame_repaint_is_bounded_to_runtime_poll_limit() {
        assert_eq!(
            frame_repaint_after(
                RepaintFrameActivity::default(),
                false,
                false,
                Duration::from_secs(60),
            ),
            MAX_REPAINT_AFTER
        );
    }

    #[test]
    fn placeholder_workspace_does_not_schedule_session_save_wakeup() {
        let now = Instant::now();

        assert_eq!(
            session_save_wakeup_after(true, now - Duration::from_secs(30), now),
            super::SESSION_SAVE_INTERVAL
        );
    }

    #[test]
    fn real_workspace_session_save_wakeup_is_due_after_interval() {
        let now = Instant::now();

        assert_eq!(
            session_save_wakeup_after(false, now - Duration::from_secs(30), now),
            Duration::ZERO
        );
    }

    #[test]
    fn debounced_wakeup_uses_nearest_due_time() {
        let now = Instant::now();
        assert_eq!(
            debounced_wakeup_after(
                [
                    now - Duration::from_millis(20),
                    now + Duration::from_millis(10)
                ]
                .into_iter(),
                now,
                Duration::from_millis(50),
            ),
            Some(Duration::from_millis(30))
        );
    }

    #[test]
    fn debounced_wakeup_is_immediate_once_delay_has_elapsed() {
        let now = Instant::now();

        assert_eq!(
            debounced_wakeup_after(
                [now - Duration::from_millis(50)].into_iter(),
                now,
                Duration::from_millis(50),
            ),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn debounced_wakeup_skips_unrepresentable_due_times_without_panicking() {
        let now = Instant::now();

        assert_eq!(
            debounced_wakeup_after([now].into_iter(), now, Duration::MAX),
            None
        );
        assert_eq!(
            debounced_wakeup_after(
                [now, now - Duration::from_millis(4)].into_iter(),
                now,
                Duration::from_millis(10)
            ),
            Some(Duration::from_millis(6))
        );
    }

    #[test]
    fn delayed_wakeup_caps_unrepresentable_due_times_without_panicking() {
        let now = Instant::now();

        assert_eq!(
            delayed_wakeup_after(now, now, Duration::MAX),
            MAX_REPAINT_AFTER
        );
    }

    #[test]
    fn absolute_wakeup_is_immediate_for_due_lsp_restart() {
        let now = Instant::now();

        assert_eq!(
            absolute_wakeup_after([now - Duration::from_millis(1)].into_iter(), now),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn pending_format_save_wakeup_keeps_save_dialogs_advancing() {
        assert_eq!(
            pending_format_save_wakeup_after(true),
            Some(PENDING_FORMAT_SAVE_REPAINT_INTERVAL)
        );
        assert_eq!(pending_format_save_wakeup_after(false), None);
    }

    #[test]
    fn source_control_save_wakeup_keeps_saving_dialogs_advancing() {
        assert_eq!(
            pending_source_control_save_wakeup_after(true),
            Some(PENDING_FORMAT_SAVE_REPAINT_INTERVAL)
        );
        assert_eq!(pending_source_control_save_wakeup_after(false), None);
    }

    #[test]
    fn guard_save_wakeup_keeps_exit_and_workspace_saving_dialogs_advancing() {
        assert_eq!(
            pending_guard_save_wakeup_after(true),
            Some(PENDING_FORMAT_SAVE_REPAINT_INTERVAL)
        );
        assert_eq!(pending_guard_save_wakeup_after(false), None);
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
