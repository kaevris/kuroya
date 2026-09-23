use super::TerminalPane;
use crate::{terminal_process::TerminalCommand, terminal_support::terminal_size_from_points};
use egui::Modifiers;
use kuroya_core::{
    DEFAULT_TERMINAL_FAST_SCROLL_SENSITIVITY, DEFAULT_TERMINAL_MOUSE_WHEEL_SCROLL_SENSITIVITY,
    TerminalConfirmOnExit, TerminalConfirmOnKill, TerminalCursorStyle, TerminalInactiveCursorStyle,
    TerminalMiddleClickBehavior, TerminalMultiLinePasteWarning, TerminalRightClickBehavior,
    TerminalSplitCwd, TerminalTabsFocusMode, TerminalTabsHideCondition, TerminalTabsLocation,
    TerminalTabsShowActions, TerminalTabsShowActiveTerminal, WorkspaceTask,
    clamp_terminal_bell_duration_ms, clamp_terminal_cursor_width, clamp_terminal_font_size,
    clamp_terminal_letter_spacing, clamp_terminal_line_height, clamp_terminal_min_columns,
    clamp_terminal_min_rows, clamp_terminal_minimum_contrast_ratio,
    clamp_terminal_scroll_sensitivity, clamp_terminal_scrollback_rows,
};
use std::{
    cell::Cell,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

mod input;
mod state;

#[cfg(test)]
use self::input::{
    TERMINAL_BRACKETED_PASTE_PREFIX, TERMINAL_BRACKETED_PASTE_SUFFIX,
    TERMINAL_BRACKETED_PASTE_WRAPPER_BYTES, TERMINAL_CURSOR_INPUT_REPEAT_LIMIT,
    terminal_sgr_mouse_input,
};
use self::input::{
    TERMINAL_INPUT_MAX_BYTES, bounded_terminal_input, terminal_alternate_scroll_input,
    terminal_mouse_wheel_input, terminal_paste_has_multiple_lines, terminal_paste_line_count,
    terminal_selection_from_points, trimmed_terminal_text,
};
pub(super) use self::input::{
    terminal_alt_click_cursor_input, terminal_paste_input, terminal_zoomed_font_size,
};
use self::state::{
    bounded_split_dimension, enforce_min_split_widths, non_empty_text, split_min_width,
    terminal_process_session_state, terminal_scoped_state_presence,
};

thread_local! {
    static TERMINAL_SESSION_INDEX_CACHE: Cell<Option<TerminalSessionIndexCache>> = const { Cell::new(None) };
    static TERMINAL_VISIBILITY_CACHE: Cell<Option<TerminalVisibilityState>> = const { Cell::new(None) };
}

#[derive(Clone, Copy)]
struct TerminalSessionIndexCache {
    pane_key: usize,
    session_id: usize,
    index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TerminalSessionActionTarget {
    index: usize,
    session_id: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct TerminalVisibilityInputs {
    pane_key: usize,
    session_count: usize,
    tabs_enabled: bool,
    tabs_hide_condition: TerminalTabsHideCondition,
    tabs_show_active_terminal: TerminalTabsShowActiveTerminal,
    tabs_show_actions: TerminalTabsShowActions,
    tabs_location: TerminalTabsLocation,
    split_view: bool,
}

#[derive(Clone, Copy)]
struct TerminalVisibilityState {
    inputs: TerminalVisibilityInputs,
    session_tabs_visible: bool,
    active_session_dropdown_visible: bool,
    active_info_visible: bool,
    action_buttons_visible: bool,
    tabs_rail_location: Option<TerminalTabsLocation>,
}

impl TerminalVisibilityState {
    fn new(inputs: TerminalVisibilityInputs) -> Self {
        let has_multiple_sessions = inputs.session_count > 1;
        let session_tabs_visible = inputs.tabs_enabled
            && match inputs.tabs_hide_condition {
                TerminalTabsHideCondition::Never => true,
                TerminalTabsHideCondition::SingleTerminal => has_multiple_sessions,
                TerminalTabsHideCondition::SingleGroup => {
                    has_multiple_sessions && !inputs.split_view
                }
            };
        let active_session_dropdown_visible = !inputs.tabs_enabled && has_multiple_sessions;
        let active_info_visible = inputs.session_count > 0
            && !session_tabs_visible
            && !active_session_dropdown_visible
            && match inputs.tabs_show_active_terminal {
                TerminalTabsShowActiveTerminal::Always => true,
                TerminalTabsShowActiveTerminal::SingleTerminal => inputs.session_count == 1,
                TerminalTabsShowActiveTerminal::SingleTerminalOrNarrow => {
                    inputs.session_count == 1 || !inputs.tabs_enabled || inputs.split_view
                }
                TerminalTabsShowActiveTerminal::Never => false,
            };
        let action_buttons_visible = match inputs.tabs_show_actions {
            TerminalTabsShowActions::Always => true,
            TerminalTabsShowActions::SingleTerminal => inputs.session_count <= 1,
            TerminalTabsShowActions::SingleTerminalOrNarrow => true,
            TerminalTabsShowActions::Never => false,
        };

        let tabs_rail_location = if session_tabs_visible {
            match inputs.tabs_location {
                TerminalTabsLocation::Top => None,
                TerminalTabsLocation::Left | TerminalTabsLocation::Right => {
                    Some(inputs.tabs_location)
                }
            }
        } else {
            None
        };

        Self {
            inputs,
            session_tabs_visible,
            active_session_dropdown_visible,
            active_info_visible,
            action_buttons_visible,
            tabs_rail_location,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalCommandQueueError {
    Full,
    Disconnected,
}

impl TerminalCommandQueueError {
    fn from_try_send(error: crossbeam_channel::TrySendError<TerminalCommand>) -> Self {
        match error {
            crossbeam_channel::TrySendError::Full(_) => Self::Full,
            crossbeam_channel::TrySendError::Disconnected(_) => Self::Disconnected,
        }
    }
}

enum TerminalInputQueueResult {
    Queued,
    Empty,
    Full(String),
    Disconnected,
}

/// Outcome of sending input to a session, enough to surface paste failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalInputDelivery {
    Delivered,
    Unavailable,
    Full,
    Disconnected,
}

impl TerminalInputDelivery {
    fn delivered(&self) -> bool {
        matches!(self, Self::Delivered)
    }
}

impl TerminalPane {
    fn cache_key(&self) -> usize {
        self as *const Self as usize
    }

    fn session_action_target(&self, index: usize) -> Option<TerminalSessionActionTarget> {
        self.sessions
            .get(index)
            .map(|session| TerminalSessionActionTarget {
                index,
                session_id: session.id,
            })
    }

    fn session_action_target_by_id(
        &self,
        session_id: usize,
    ) -> Option<TerminalSessionActionTarget> {
        let index = self.session_index_by_id(session_id)?;
        Some(TerminalSessionActionTarget { index, session_id })
    }

    fn resolve_session_action_target(
        &self,
        target: TerminalSessionActionTarget,
    ) -> Option<TerminalSessionActionTarget> {
        let index = self.index_for_session_action_target(target)?;
        Some(TerminalSessionActionTarget {
            index,
            session_id: target.session_id,
        })
    }

    fn index_for_session_action_target(
        &self,
        target: TerminalSessionActionTarget,
    ) -> Option<usize> {
        if self
            .sessions
            .get(target.index)
            .is_some_and(|session| session.id == target.session_id)
        {
            Some(target.index)
        } else {
            self.session_index_by_id(target.session_id)
        }
    }

    fn session_for_action_target(
        &self,
        target: TerminalSessionActionTarget,
    ) -> Option<&super::TerminalSession> {
        let target = self.resolve_session_action_target(target)?;
        self.sessions.get(target.index)
    }

    fn session_mut_for_action_target(
        &mut self,
        target: TerminalSessionActionTarget,
    ) -> Option<&mut super::TerminalSession> {
        let target = self.resolve_session_action_target(target)?;
        self.sessions.get_mut(target.index)
    }

    fn command_session_action_target(&self, index: usize) -> Option<TerminalSessionActionTarget> {
        self.session_action_target(index)
            .filter(|target| self.session_can_receive_terminal_input(*target))
    }

    fn session_can_receive_terminal_input(&self, target: TerminalSessionActionTarget) -> bool {
        self.session_for_action_target(target)
            .is_some_and(super::TerminalSession::has_command_target)
    }

    fn session_can_request_terminal_input(&self, target: TerminalSessionActionTarget) -> bool {
        self.session_for_action_target(target)
            .is_some_and(super::TerminalSession::can_request_terminal_input)
    }

    fn activate_session_action_target(&mut self, target: TerminalSessionActionTarget) -> bool {
        let Some(target) = self.resolve_session_action_target(target) else {
            return false;
        };
        self.active_session = target.index;
        self.focus_input_on_show = true;
        if self.visible {
            self.start_session_if_needed(target.index);
        }
        self.session_can_receive_terminal_input(target)
    }

    fn clear_terminal_input_selection_state(&mut self) {
        self.selected_session_id = None;
        self.selected_text = None;
        self.selection_drag = None;
    }

    fn session_index_by_id(&self, session_id: usize) -> Option<usize> {
        let pane_key = self.cache_key();
        if let Some(index) = TERMINAL_SESSION_INDEX_CACHE.with(|cache| {
            let cached = cache.get()?;
            (cached.pane_key == pane_key
                && cached.session_id == session_id
                && self
                    .sessions
                    .get(cached.index)
                    .is_some_and(|session| session.id == session_id))
            .then_some(cached.index)
        }) {
            return Some(index);
        }

        let index = self
            .sessions
            .iter()
            .position(|session| session.id == session_id)?;
        TERMINAL_SESSION_INDEX_CACHE.with(|cache| {
            cache.set(Some(TerminalSessionIndexCache {
                pane_key,
                session_id,
                index,
            }));
        });
        Some(index)
    }

    fn session_by_id(&self, session_id: usize) -> Option<&super::TerminalSession> {
        self.sessions.get(self.session_index_by_id(session_id)?)
    }

    fn terminal_visibility_state(&self) -> TerminalVisibilityState {
        let inputs = TerminalVisibilityInputs {
            pane_key: self.cache_key(),
            session_count: self.sessions.len(),
            tabs_enabled: self.tabs_enabled,
            tabs_hide_condition: self.tabs_hide_condition,
            tabs_show_active_terminal: self.tabs_show_active_terminal,
            tabs_show_actions: self.tabs_show_actions,
            tabs_location: self.tabs_location,
            split_view: self.split_view,
        };

        TERMINAL_VISIBILITY_CACHE.with(|cache| {
            if let Some(state) = cache.get()
                && state.inputs == inputs
            {
                return state;
            }

            let state = TerminalVisibilityState::new(inputs);
            cache.set(Some(state));
            state
        })
    }

    pub(crate) fn is_fullscreen(&self) -> bool {
        self.fullscreen
    }

    pub(crate) fn exit_confirmation_session_count(&self, behavior: TerminalConfirmOnExit) -> usize {
        match behavior {
            TerminalConfirmOnExit::Never => 0,
            TerminalConfirmOnExit::Always | TerminalConfirmOnExit::HasChildProcesses => self
                .sessions
                .iter()
                .filter(|session| session.started)
                .count(),
        }
    }

    pub(super) fn toggle_fullscreen(&mut self) {
        self.fullscreen = !self.fullscreen;
        self.focus_input_on_show = true;
    }

    pub(super) fn unsplit_sessions(&mut self) {
        self.split_view = false;
        self.focus_input_on_show = true;
    }

    pub(crate) fn set_scrollback_rows(&mut self, scrollback_rows: usize) {
        let scrollback_rows = clamp_terminal_scrollback_rows(scrollback_rows);
        self.scrollback_rows = scrollback_rows;
        for session in &mut self.sessions {
            session.set_scrollback_rows(scrollback_rows);
        }
    }

    pub(crate) fn set_shell_profile(
        &mut self,
        shell_path: Option<String>,
        shell_args: Vec<String>,
    ) {
        let shell_path = super::normalized_shell_path(shell_path);
        self.shell_label = crate::terminal_process::terminal_shell_label(shell_path.as_deref());
        self.shell_path = shell_path;
        self.shell_args = super::normalized_shell_args(shell_args);
    }

    pub(crate) fn restart_shell_sessions_for_profile_change(&mut self) -> usize {
        let mut restarted = 0usize;
        for index in 0..self.sessions.len() {
            if self.restart_shell_session_for_profile_change(index) {
                restarted += 1;
            }
        }
        if restarted > 0 {
            self.search_cache = super::TerminalSearchCache::default();
            self.search_match = 0;
        }
        restarted
    }

    fn restart_shell_session_for_profile_change(&mut self, index: usize) -> bool {
        let Some((session_id, initial_cwd, custom_title)) =
            self.profile_change_restart_session_state(index)
        else {
            return false;
        };
        let cwd = initial_cwd.unwrap_or_else(|| self.launch_cwd());

        if let Some(session) = self.sessions.get_mut(index) {
            session.close();
        }
        self.clear_session_scoped_state(session_id);

        let mut session =
            super::TerminalSession::new(session_id, self.last_size, self.scrollback_rows);
        session.initial_cwd = Some(cwd.clone());
        session.custom_title = custom_title;
        session.start(
            &cwd,
            self.last_size,
            self.shell_path.clone(),
            self.shell_args.clone(),
            self.show_exit_alert,
            self.repaint_context.clone(),
        );
        if let Some(slot) = self.sessions.get_mut(index) {
            *slot = session;
            true
        } else {
            false
        }
    }

    fn profile_change_restart_session_state(
        &self,
        index: usize,
    ) -> Option<(usize, Option<std::path::PathBuf>, Option<String>)> {
        let session = self.sessions.get(index)?;
        (session.process_label.is_none()
            && session.started
            && session.auto_start_shell
            && !session.close_requested.load(Ordering::SeqCst)
            && !matches!(
                session.command_status(),
                super::TerminalCommandStatus::Running
            ))
        .then(|| {
            (
                session.id,
                session.initial_cwd.clone(),
                session.custom_title.clone(),
            )
        })
    }

    pub(crate) fn set_terminal_cwd(&mut self, terminal_cwd: Option<String>) {
        self.terminal_cwd = super::normalized_terminal_cwd(terminal_cwd);
    }

    pub(crate) fn set_split_cwd(&mut self, split_cwd: TerminalSplitCwd) {
        self.split_cwd = split_cwd;
    }

    pub(crate) fn set_minimum_size(&mut self, min_rows: u16, min_columns: u16) {
        self.min_rows = clamp_terminal_min_rows(min_rows);
        self.min_columns = clamp_terminal_min_columns(min_columns);
        if self.sessions.is_empty() {
            self.last_size =
                crate::terminal_support::initial_terminal_size(self.min_rows, self.min_columns);
        }
    }

    pub(crate) fn set_font_metrics(
        &mut self,
        font_size: f32,
        line_height: f32,
        letter_spacing: f32,
    ) {
        self.font_size = clamp_terminal_font_size(font_size);
        self.line_height = clamp_terminal_line_height(line_height);
        self.letter_spacing = clamp_terminal_letter_spacing(letter_spacing);
    }

    pub(crate) fn set_cursor_settings(
        &mut self,
        cursor_style: TerminalCursorStyle,
        cursor_width: f32,
        cursor_blinking: bool,
        cursor_style_inactive: TerminalInactiveCursorStyle,
    ) {
        self.cursor_style = cursor_style;
        self.cursor_width = clamp_terminal_cursor_width(cursor_width);
        self.cursor_blinking = cursor_blinking;
        self.cursor_style_inactive = cursor_style_inactive;
    }

    pub(crate) fn set_draw_bold_text_in_bright_colors(&mut self, enabled: bool) {
        self.draw_bold_text_in_bright_colors = enabled;
    }

    pub(crate) fn set_minimum_contrast_ratio(&mut self, ratio: f32) {
        self.minimum_contrast_ratio = clamp_terminal_minimum_contrast_ratio(ratio);
    }

    pub(crate) fn set_bell_settings(&mut self, enabled: bool, duration_ms: u64) {
        self.enable_bell = enabled;
        self.bell_duration_ms = clamp_terminal_bell_duration_ms(duration_ms);
        if !enabled {
            self.last_bell_at = None;
        }
    }

    pub(crate) fn set_show_exit_alert(&mut self, show_exit_alert: bool) {
        self.show_exit_alert = show_exit_alert;
    }

    pub(crate) fn set_hide_on_last_closed(&mut self, hide_on_last_closed: bool) {
        self.hide_on_last_closed = hide_on_last_closed;
    }

    pub(crate) fn set_confirm_on_kill(&mut self, behavior: TerminalConfirmOnKill) {
        self.confirm_on_kill = behavior;
    }

    pub(crate) fn set_tabs_enabled(&mut self, enabled: bool) {
        self.tabs_enabled = enabled;
    }

    pub(crate) fn set_tabs_default_icon(&mut self, icon: impl AsRef<str>) {
        self.tabs_default_icon = super::normalized_terminal_tab_icon(icon.as_ref().to_owned());
    }

    pub(crate) fn set_tabs_default_color(&mut self, color: Option<String>) {
        self.tabs_default_color = super::normalized_terminal_tab_color(color);
    }

    pub(crate) fn set_tabs_allow_agent_cli_title(&mut self, allow: bool) {
        self.tabs_allow_agent_cli_title = allow;
    }

    pub(crate) fn set_tabs_title_template(&mut self, title: impl AsRef<str>) {
        self.tabs_title_template = super::normalized_terminal_tabs_title(title.as_ref().to_owned());
    }

    pub(crate) fn set_tabs_hide_condition(&mut self, behavior: TerminalTabsHideCondition) {
        self.tabs_hide_condition = behavior;
    }

    pub(crate) fn set_tabs_show_active_terminal(
        &mut self,
        behavior: TerminalTabsShowActiveTerminal,
    ) {
        self.tabs_show_active_terminal = behavior;
    }

    pub(crate) fn set_tabs_show_actions(&mut self, behavior: TerminalTabsShowActions) {
        self.tabs_show_actions = behavior;
    }

    pub(crate) fn set_tabs_focus_mode(&mut self, mode: TerminalTabsFocusMode) {
        self.tabs_focus_mode = mode;
    }

    pub(crate) fn set_tabs_location(&mut self, location: TerminalTabsLocation) {
        self.tabs_location = location;
    }

    pub(super) fn terminal_session_tabs_visible(&self) -> bool {
        self.terminal_visibility_state().session_tabs_visible
    }

    pub(super) fn terminal_active_session_dropdown_visible(&self) -> bool {
        self.terminal_visibility_state()
            .active_session_dropdown_visible
    }

    pub(super) fn terminal_tabs_rail_location(&self) -> Option<TerminalTabsLocation> {
        self.terminal_visibility_state().tabs_rail_location
    }

    pub(super) fn terminal_active_info_visible(&self) -> bool {
        self.terminal_visibility_state().active_info_visible
    }

    pub(super) fn terminal_action_buttons_visible(&self) -> bool {
        self.terminal_visibility_state().action_buttons_visible
    }

    pub(super) fn terminal_tab_click_focuses_input(
        &self,
        clicked: bool,
        double_clicked: bool,
    ) -> bool {
        match self.tabs_focus_mode {
            TerminalTabsFocusMode::SingleClick => clicked || double_clicked,
            TerminalTabsFocusMode::DoubleClick => double_clicked,
        }
    }

    pub(crate) fn set_right_click_behavior(&mut self, behavior: TerminalRightClickBehavior) {
        self.right_click_behavior = behavior;
    }

    pub(crate) fn set_middle_click_behavior(&mut self, behavior: TerminalMiddleClickBehavior) {
        self.middle_click_behavior = behavior;
    }

    pub(crate) fn set_alt_click_moves_cursor(&mut self, enabled: bool) {
        self.alt_click_moves_cursor = enabled;
    }

    pub(crate) fn set_copy_on_selection(&mut self, copy_on_selection: bool) {
        self.copy_on_selection = copy_on_selection;
    }

    pub(crate) fn set_ignore_bracketed_paste_mode(&mut self, ignore: bool) {
        self.ignore_bracketed_paste_mode = ignore;
    }

    pub(crate) fn set_multi_line_paste_warning(&mut self, behavior: TerminalMultiLinePasteWarning) {
        self.multi_line_paste_warning = behavior;
    }

    pub(crate) fn set_word_separators(&mut self, word_separators: impl Into<String>) {
        self.word_separators = word_separators.into();
    }

    pub(crate) fn set_scroll_sensitivity(
        &mut self,
        mouse_wheel_scroll_sensitivity: f32,
        fast_scroll_sensitivity: f32,
    ) {
        self.mouse_wheel_scroll_sensitivity = clamp_terminal_scroll_sensitivity(
            mouse_wheel_scroll_sensitivity,
            DEFAULT_TERMINAL_MOUSE_WHEEL_SCROLL_SENSITIVITY,
        );
        self.fast_scroll_sensitivity = clamp_terminal_scroll_sensitivity(
            fast_scroll_sensitivity,
            DEFAULT_TERMINAL_FAST_SCROLL_SENSITIVITY,
        );
    }

    pub(crate) fn set_mouse_wheel_zoom(&mut self, enabled: bool) {
        self.mouse_wheel_zoom = enabled;
    }

    pub(super) fn open_new_session(&mut self) {
        if !self.can_open_session() {
            return;
        }
        let id = self.next_session_id;
        self.next_session_id += 1;

        let session = super::TerminalSession::new(id, self.last_size, self.scrollback_rows);
        let cwd = self.launch_cwd();
        self.open_session_with_cwd(session, cwd);
    }

    pub(crate) fn open_new_session_at(&mut self, cwd: std::path::PathBuf) {
        if !self.can_open_session() {
            return;
        }
        let id = self.next_session_id;
        self.next_session_id += 1;

        let session = super::TerminalSession::new(id, self.last_size, self.scrollback_rows);
        self.open_session_with_cwd(session, cwd);
    }

    pub(crate) fn open_workspace_task(&mut self, task: &WorkspaceTask) -> Option<usize> {
        if !self.can_open_session() {
            return None;
        }
        let id = self.next_session_id;
        self.next_session_id += 1;

        let mut session = super::TerminalSession::new(id, self.last_size, self.scrollback_rows);
        let cwd = task.cwd.clone().unwrap_or_else(|| self.launch_cwd());
        session.start_process(
            &cwd,
            self.last_size,
            task.command.clone(),
            task.args.clone(),
            task.env.clone(),
            self.show_exit_alert,
            task.name.clone(),
            self.repaint_context.clone(),
        );
        self.activate_opened_session(session);
        Some(id)
    }

    fn open_session_with_cwd(
        &mut self,
        mut session: super::TerminalSession,
        cwd: std::path::PathBuf,
    ) {
        session.start(
            &cwd,
            self.last_size,
            self.shell_path.clone(),
            self.shell_args.clone(),
            self.show_exit_alert,
            self.repaint_context.clone(),
        );
        self.activate_opened_session(session);
    }

    fn activate_opened_session(&mut self, session: super::TerminalSession) {
        self.sessions.push(session);
        self.split_weights.push(1.0);
        self.active_session = self.sessions.len().saturating_sub(1);
        self.split_view = false;
        self.visible = true;
        self.focus_input_on_show = true;
        self.selected_session_id = None;
        self.selected_text = None;
        self.selection_drag = None;
    }

    pub(super) fn split_active_session(&mut self) {
        if !self.can_open_session() {
            return;
        }
        let parent_index = self.active_session_index();
        let id = self.next_session_id;
        self.next_session_id += 1;

        let mut session = super::TerminalSession::new(id, self.last_size, self.scrollback_rows);
        let cwd = self.split_launch_cwd(parent_index);
        session.start(
            &cwd,
            self.last_size,
            self.shell_path.clone(),
            self.shell_args.clone(),
            self.show_exit_alert,
            self.repaint_context.clone(),
        );
        self.sync_split_weights();
        let insert_index = parent_index
            .map(|index| index.saturating_add(1))
            .unwrap_or(self.sessions.len())
            .min(self.sessions.len());
        self.sessions.insert(insert_index, session);
        if let Some(parent_index) = parent_index
            && parent_index < self.split_weights.len()
        {
            let parent_weight = self.split_weights[parent_index];
            let split_weight = if parent_weight.is_finite() && parent_weight > 0.0 {
                parent_weight * 0.5
            } else {
                1.0
            };
            self.split_weights[parent_index] = split_weight;
            self.split_weights.insert(insert_index, split_weight);
        } else {
            self.split_weights.insert(insert_index, 1.0);
        }
        self.active_session = insert_index;
        self.split_view = self.sessions.len() > 1;
        self.visible = true;
        self.focus_input_on_show = true;
        self.selected_session_id = None;
        self.selected_text = None;
        self.selection_drag = None;
    }

    pub(crate) fn can_open_session(&self) -> bool {
        self.sessions.len() < super::TERMINAL_MAX_SESSIONS
    }

    pub(super) fn close_active_session(&mut self) {
        let Some(active) = self.active_session_index() else {
            self.visible = false;
            self.fullscreen = false;
            self.split_view = false;
            self.clear_all_session_scoped_state();
            return;
        };

        self.close_session_at(active);
    }

    pub(super) fn request_close_active_session(&mut self) {
        let Some(active) = self.active_session_index() else {
            self.close_active_session();
            return;
        };
        if self.should_confirm_on_kill(active) {
            if let Some(session) = self.sessions.get(active) {
                self.pending_kill_session_id = Some(session.id);
            }
        } else {
            self.close_session_at(active);
        }
    }

    pub(super) fn pending_kill_session_id(&self) -> Option<usize> {
        self.pending_kill_session_id
    }

    pub(super) fn confirm_pending_kill(&mut self) {
        let Some(session_id) = self.pending_kill_session_id.take() else {
            return;
        };
        if let Some(index) = self.session_index_by_id(session_id) {
            if !self
                .sessions
                .get(index)
                .is_some_and(|session| session.started)
            {
                return;
            }
            self.close_session_at(index);
        }
    }

    pub(super) fn cancel_pending_kill(&mut self) {
        self.pending_kill_session_id = None;
    }

    pub(super) fn begin_rename_session(&mut self, index: usize) {
        let Some(session) = self.sessions.get(index) else {
            return;
        };
        self.pending_rename_session_id = Some(session.id);
        self.rename_session_input = session
            .custom_title
            .as_deref()
            .or(session.process_label.as_deref())
            .and_then(super::normalized_terminal_custom_title)
            .unwrap_or_else(|| super::TERMINAL_DEFAULT_DISPLAY_LABEL.to_owned());
    }

    pub(super) fn pending_rename_session_id(&self) -> Option<usize> {
        self.pending_rename_session_id
    }

    pub(super) fn submit_pending_rename(&mut self) {
        let Some(session_id) = self.pending_rename_session_id.take() else {
            self.rename_session_input.clear();
            return;
        };
        let custom_title = super::normalized_terminal_custom_title(&self.rename_session_input);
        self.rename_session_input.clear();
        if let Some(index) = self.session_index_by_id(session_id)
            && let Some(session) = self.sessions.get_mut(index)
        {
            session.custom_title = custom_title;
        }
    }

    pub(super) fn cancel_pending_rename(&mut self) {
        self.pending_rename_session_id = None;
        self.rename_session_input.clear();
    }

    fn should_confirm_on_kill(&self, index: usize) -> bool {
        let Some(session) = self.sessions.get(index) else {
            return false;
        };
        session.started
            && matches!(
                self.confirm_on_kill,
                TerminalConfirmOnKill::Panel | TerminalConfirmOnKill::Always
            )
    }

    fn close_session_at(&mut self, close_index: usize) {
        if close_index >= self.sessions.len() {
            self.prune_stale_session_state();
            return;
        }

        let previously_active_session_id = self
            .active_session_index()
            .and_then(|index| self.sessions.get(index))
            .map(|session| session.id);
        let closed_session_id = self.sessions[close_index].id;
        let closed_active_session = previously_active_session_id == Some(closed_session_id);

        self.sessions[close_index].close();
        self.sessions.remove(close_index);
        self.clear_session_scoped_state(closed_session_id);
        if close_index < self.split_weights.len() {
            self.split_weights.remove(close_index);
        }
        if self.sessions.is_empty() {
            self.active_session = 0;
            if self.hide_on_last_closed {
                self.visible = false;
                self.fullscreen = false;
            }
            self.split_view = false;
            self.split_weights.clear();
            self.focus_input_on_show = false;
            self.clear_all_session_scoped_state();
        } else {
            self.active_session = if closed_active_session {
                close_index.min(self.sessions.len() - 1)
            } else {
                previously_active_session_id
                    .and_then(|session_id| self.session_index_by_id(session_id))
                    .unwrap_or_else(|| close_index.min(self.sessions.len() - 1))
            };
            if self.sessions.len() == 1 {
                self.split_view = false;
            }
            self.focus_input_on_show = true;
        }
    }

    pub(crate) fn close_session_by_id(&mut self, session_id: usize) -> bool {
        let Some(index) = self.session_index_by_id(session_id) else {
            return false;
        };
        self.close_session_at(index);
        true
    }

    pub(crate) fn close_all_sessions_for_shutdown(&mut self) {
        for session in &mut self.sessions {
            session.close();
        }
        self.sessions.clear();
        self.split_weights.clear();
        self.active_session = 0;
        self.visible = false;
        self.fullscreen = false;
        self.split_view = false;
        self.focus_input_on_show = false;
        self.clear_all_session_scoped_state();
    }

    pub(crate) fn process_session_state_by_id(
        &self,
        session_id: usize,
    ) -> Option<super::TerminalProcessSessionState> {
        self.session_by_id(session_id)
            .filter(|session| session.is_process())
            .map(terminal_process_session_state)
    }

    pub(super) fn active_session_index(&self) -> Option<usize> {
        (!self.sessions.is_empty()).then(|| self.active_session.min(self.sessions.len() - 1))
    }

    pub(super) fn set_active_session(&mut self, index: usize) {
        if index < self.sessions.len() {
            self.active_session = index;
            self.focus_input_on_show = true;
            if self.visible {
                self.start_session_if_needed(index);
            }
        }
    }

    pub(super) fn set_active_session_without_focus(&mut self, index: usize) {
        if index < self.sessions.len() {
            self.active_session = index;
        }
    }

    pub(super) fn activate_session_tab(
        &mut self,
        index: usize,
        clicked: bool,
        double_clicked: bool,
    ) {
        if index < self.sessions.len() {
            self.active_session = index;
            self.focus_input_on_show =
                self.terminal_tab_click_focuses_input(clicked, double_clicked);
            if self.visible {
                self.start_session_if_needed(index);
            }
        }
    }

    pub(crate) fn activate_relative_session(&mut self, delta: isize) {
        if delta == 0 {
            return;
        }
        if self.sessions.is_empty() {
            self.set_visible(true);
            return;
        }

        let len = self.sessions.len();
        let current = self.active_session.min(len - 1) as isize;
        let next = (current + delta).rem_euclid(len as isize) as usize;
        self.active_session = next;
        self.visible = true;
        self.focus_input_on_show = true;
        self.start_session_if_needed(next);
    }

    pub(super) fn send_input(&mut self, input: impl Into<String>) {
        let input = bounded_terminal_input(input.into(), TERMINAL_INPUT_MAX_BYTES);
        if input.is_empty() {
            return;
        }
        let Some(active) = self.active_session_index() else {
            return;
        };
        let Some(target) = self.command_session_action_target(active) else {
            return;
        };
        self.send_input_to_action_target(target, input, true);
    }

    fn send_input_to_action_target(
        &mut self,
        target: TerminalSessionActionTarget,
        input: String,
        clear_selection: bool,
    ) -> bool {
        let Some(target) = self.resolve_session_action_target(target) else {
            return false;
        };
        self.send_input_to_resolved_action_target(target, input, clear_selection)
            .delivered()
    }

    fn send_input_to_resolved_action_target(
        &mut self,
        target: TerminalSessionActionTarget,
        input: String,
        clear_selection: bool,
    ) -> TerminalInputDelivery {
        if input.is_empty()
            || !self.sessions.get(target.index).is_some_and(|session| {
                session.id == target.session_id && session.has_command_target()
            })
        {
            return TerminalInputDelivery::Unavailable;
        }
        if clear_selection {
            self.clear_terminal_input_selection_state();
        }
        let Some(session) = self
            .sessions
            .get_mut(target.index)
            .filter(|session| session.id == target.session_id && session.has_command_target())
        else {
            return TerminalInputDelivery::Unavailable;
        };
        match session.send_input(input) {
            Ok(()) => TerminalInputDelivery::Delivered,
            Err(TerminalCommandQueueError::Full) => TerminalInputDelivery::Full,
            Err(TerminalCommandQueueError::Disconnected) => {
                session.mark_stopped();
                TerminalInputDelivery::Disconnected
            }
        }
    }

    #[cfg(test)]
    pub(super) fn send_paste_input(&mut self, index: usize, text: impl Into<String>) {
        let Some(target) = self.command_session_action_target(index) else {
            return;
        };
        self.send_paste_input_to_target(target, text);
    }

    fn send_paste_input_to_target(
        &mut self,
        target: TerminalSessionActionTarget,
        text: impl Into<String>,
    ) {
        let ignore_bracketed_paste_mode = self.ignore_bracketed_paste_mode;
        let Some(target) = self.resolve_session_action_target(target) else {
            return;
        };
        let Some(bracketed_paste) = self.sessions.get(target.index).and_then(|session| {
            (session.id == target.session_id && session.has_command_target())
                .then(|| session.parser.screen().bracketed_paste())
        }) else {
            return;
        };
        let input = terminal_paste_input(text.into(), bracketed_paste, ignore_bracketed_paste_mode);
        match self.send_input_to_resolved_action_target(target, input, true) {
            TerminalInputDelivery::Delivered | TerminalInputDelivery::Unavailable => {}
            TerminalInputDelivery::Full => {
                self.show_paste_notice(super::TERMINAL_PASTE_BUSY_NOTICE);
            }
            TerminalInputDelivery::Disconnected => {
                self.show_paste_notice(super::TERMINAL_PASTE_UNDELIVERED_NOTICE);
            }
        }
    }

    pub(super) fn paste_text(&mut self, index: usize, text: String) {
        let pending_paste_session_id = self.pending_paste_session_id.take();
        if text.len() > TERMINAL_INPUT_MAX_BYTES {
            self.show_paste_notice(super::TERMINAL_PASTE_TRUNCATED_NOTICE);
        }
        let text = bounded_terminal_input(text, TERMINAL_INPUT_MAX_BYTES);
        if text.is_empty() {
            return;
        }
        let target = match pending_paste_session_id {
            Some(session_id) => self.session_action_target_by_id(session_id),
            None => self.session_action_target(index),
        };
        let Some(target) = target.and_then(|target| self.resolve_session_action_target(target))
        else {
            return;
        };
        if !self.activate_session_action_target(target) {
            self.show_paste_notice(super::TERMINAL_PASTE_UNDELIVERED_NOTICE);
            return;
        }
        let Some(target) = self.resolve_session_action_target(target) else {
            return;
        };
        if self.should_warn_for_multiline_paste(target, &text) {
            self.pending_multiline_paste = Some(super::TerminalPendingPaste {
                session_id: target.session_id,
                text,
            });
            return;
        }
        self.send_paste_input_to_target(target, text);
    }

    pub(super) fn pending_multiline_paste_line_count(&self) -> Option<usize> {
        let pending = self.pending_multiline_paste.as_ref()?;
        self.session_action_target_by_id(pending.session_id)?;
        Some(terminal_paste_line_count(&pending.text))
    }

    pub(super) fn confirm_pending_multiline_paste(&mut self) {
        let Some(pending) = self.pending_multiline_paste.take() else {
            return;
        };
        let Some(target) = self.session_action_target_by_id(pending.session_id) else {
            return;
        };
        if self.session_can_receive_terminal_input(target) {
            self.send_paste_input_to_target(target, pending.text);
        }
    }

    pub(super) fn cancel_pending_multiline_paste(&mut self) {
        self.pending_multiline_paste = None;
    }

    pub(super) fn show_paste_notice(&mut self, message: impl Into<String>) {
        self.paste_notice = Some(super::TerminalPasteNotice {
            message: message.into(),
            created: Instant::now(),
        });
    }

    /// Returns the active notice message and the time left before it expires.
    pub(super) fn active_paste_notice(&self) -> Option<(&str, Duration)> {
        self.active_paste_notice_at(Instant::now())
    }

    fn active_paste_notice_at(&self, now: Instant) -> Option<(&str, Duration)> {
        let notice = self.paste_notice.as_ref()?;
        let elapsed = now.checked_duration_since(notice.created)?;
        let remaining = super::TERMINAL_PASTE_NOTICE_TTL.checked_sub(elapsed)?;
        Some((notice.message.as_str(), remaining))
    }

    pub(super) fn expire_paste_notice(&mut self) {
        self.expire_paste_notice_at(Instant::now());
    }

    fn expire_paste_notice_at(&mut self, now: Instant) {
        if self.paste_notice.is_some() && self.active_paste_notice_at(now).is_none() {
            self.paste_notice = None;
        }
    }

    fn should_warn_for_multiline_paste(
        &self,
        target: TerminalSessionActionTarget,
        text: &str,
    ) -> bool {
        if !terminal_paste_has_multiple_lines(text) {
            return false;
        }
        match self.multi_line_paste_warning {
            TerminalMultiLinePasteWarning::Never => false,
            TerminalMultiLinePasteWarning::Always => true,
            TerminalMultiLinePasteWarning::Auto => self
                .session_for_action_target(target)
                .is_none_or(|session| {
                    !session.parser.screen().bracketed_paste() || self.ignore_bracketed_paste_mode
                }),
        }
    }

    pub(super) fn send_alt_click_cursor_input(
        &mut self,
        index: usize,
        target: super::TerminalCellPosition,
    ) {
        let Some(action_target) = self.command_session_action_target(index) else {
            return;
        };
        let Some(session) = self.session_for_action_target(action_target) else {
            return;
        };
        let screen = session.parser.screen();
        let (_, cols) = screen.size();
        let (row, col) = screen.cursor_position();
        let cursor = super::TerminalCellPosition { row, col };
        let Some(input) = terminal_alt_click_cursor_input(cursor, target, cols) else {
            return;
        };
        self.send_input_to_action_target(action_target, input, true);
    }

    pub(super) fn send_terminal_wheel_input(
        &mut self,
        index: usize,
        position: Option<super::TerminalCellPosition>,
        delta_rows: i32,
        modifiers: Modifiers,
    ) -> bool {
        let Some(target) = self.command_session_action_target(index) else {
            return false;
        };
        let Some(session) = self.session_for_action_target(target) else {
            return false;
        };
        let input = {
            let screen = session.parser.screen();
            position
                .and_then(|position| {
                    terminal_mouse_wheel_input(screen, position, delta_rows, modifiers)
                })
                .or_else(|| terminal_alternate_scroll_input(screen, delta_rows))
        };
        let Some(input) = input else {
            return false;
        };

        self.send_input_to_action_target(target, input, false)
    }

    pub(super) fn copyable_text_for_session(&self, index: usize) -> Option<String> {
        let target = self.session_action_target(index)?;
        self.copyable_text_for_session_target(target)
    }

    fn copyable_text_for_session_target(
        &self,
        target: TerminalSessionActionTarget,
    ) -> Option<String> {
        let session = self.session_for_action_target(target)?;
        if self.selected_session_id == Some(session.id) {
            return non_empty_text(session.copyable_text());
        }
        if let Some(selection) = &self.selected_text {
            if selection.session_id == session.id {
                return non_empty_text(selection.text.clone());
            }
        }
        non_empty_text(session.copyable_text())
    }

    pub(super) fn has_selection_for_session(&self, index: usize) -> bool {
        let Some(target) = self.session_action_target(index) else {
            return false;
        };
        self.has_selection_for_session_target(target)
    }

    fn has_selection_for_session_target(&self, target: TerminalSessionActionTarget) -> bool {
        let Some(session) = self.session_for_action_target(target) else {
            return false;
        };
        self.selected_session_id == Some(session.id)
            || self
                .selected_text
                .as_ref()
                .is_some_and(|selection| selection.session_id == session.id)
    }

    pub(super) fn clear_session(&mut self, index: usize) {
        let Some(target) = self.session_action_target(index) else {
            return;
        };
        if let Some(session) = self.session_mut_for_action_target(target) {
            session.clear_buffer();
            self.selected_session_id = None;
            self.selected_text = None;
            self.selection_drag = None;
        }
    }

    pub(super) fn select_all_session(&mut self, index: usize) {
        let Some(target) = self
            .session_action_target(index)
            .and_then(|target| self.resolve_session_action_target(target))
        else {
            return;
        };
        if self.session_for_action_target(target).is_some() {
            self.selected_session_id = Some(target.session_id);
            self.selected_text = None;
            self.selection_drag = None;
        }
    }

    pub(super) fn scroll_session_to_top(&mut self, index: usize) {
        if let Some(session) = self.sessions.get_mut(index) {
            session.scroll_to_top();
        }
    }

    pub(super) fn scroll_session_to_bottom(&mut self, index: usize) {
        if let Some(session) = self.sessions.get_mut(index) {
            session.scroll_to_bottom();
        }
    }

    pub(super) fn request_paste_for_session(&mut self, index: usize) {
        let Some(target) = self
            .session_action_target(index)
            .and_then(|target| self.resolve_session_action_target(target))
        else {
            self.pending_paste_session_id = None;
            return;
        };
        if !self.session_can_request_terminal_input(target) {
            self.pending_paste_session_id = None;
            return;
        }

        self.pending_paste_session_id = Some(target.session_id);
        self.active_session = target.index;
        self.focus_input_on_show = true;
        self.selected_session_id = None;
        self.selected_text = None;
        self.selection_drag = None;
    }

    pub(super) fn select_text_range_for_session(
        &mut self,
        index: usize,
        anchor: super::TerminalCellPosition,
        cursor: super::TerminalCellPosition,
    ) -> Option<String> {
        let target = self.session_action_target(index)?;
        let session = self.session_for_action_target(target)?;
        let selection = terminal_selection_from_points(session, anchor, cursor)?;
        let text = selection.text.clone();
        self.selected_session_id = None;
        self.selected_text = Some(selection);
        non_empty_text(text)
    }

    pub(super) fn resize_session_to_fit(&mut self, index: usize, width: f32, output_height: f32) {
        if self.sessions.get(index).is_none() {
            return;
        }
        let size = terminal_size_from_points(
            width,
            output_height,
            self.font_size,
            self.line_height,
            self.letter_spacing,
            self.min_rows,
            self.min_columns,
        );
        self.last_size = size;
        if let Some(session) = self.sessions.get_mut(index) {
            session.resize(size);
        }
    }

    pub(super) fn zoom_terminal_font(&mut self, wheel_delta_y: f32) -> bool {
        let Some(font_size) = terminal_zoomed_font_size(self.font_size, wheel_delta_y) else {
            return false;
        };
        self.font_size = font_size;
        true
    }

    pub(super) fn split_widths(&mut self, available_width: f32, separator_width: f32) -> Vec<f32> {
        self.sync_split_weights();
        let count = self.sessions.len();
        if count == 0 {
            return Vec::new();
        }

        let available_width = bounded_split_dimension(available_width);
        let separator_width = bounded_split_dimension(separator_width);
        let total_separator_width = separator_width * count.saturating_sub(1) as f32;
        let available_width = if total_separator_width.is_finite() {
            bounded_split_dimension(available_width - total_separator_width)
        } else {
            0.0
        };
        let weight_sum = self.split_weights.iter().sum::<f32>();
        let weight_sum = if weight_sum.is_finite() && weight_sum > 0.0 {
            weight_sum
        } else {
            0.0
        };
        let mut widths = Vec::with_capacity(count);
        if weight_sum > 0.0 {
            for weight in &self.split_weights {
                widths.push((*weight / weight_sum) * available_width);
            }
        } else {
            widths.resize(count, available_width / count as f32);
        }
        enforce_min_split_widths(&mut widths, available_width);
        self.split_weights.clone_from(&widths);
        widths
    }

    pub(super) fn resize_split_at(&mut self, left_index: usize, delta_x: f32) {
        self.sync_split_weights();
        if left_index + 1 >= self.split_weights.len() || !delta_x.is_finite() || delta_x == 0.0 {
            return;
        }

        let combined = self.split_weights[left_index] + self.split_weights[left_index + 1];
        if !combined.is_finite() || combined <= 0.0 {
            self.split_weights[left_index] = 1.0;
            self.split_weights[left_index + 1] = 1.0;
            return;
        }
        let min_width = split_min_width().min(combined / 2.0);
        let next_left =
            (self.split_weights[left_index] + delta_x).clamp(min_width, combined - min_width);
        self.split_weights[left_index] = next_left;
        self.split_weights[left_index + 1] = combined - next_left;
    }

    fn sync_split_weights(&mut self) {
        self.split_weights.resize(self.sessions.len(), 1.0);
        for weight in &mut self.split_weights {
            if !weight.is_finite() || *weight < 0.0 {
                *weight = 0.0;
            }
        }
        if self.split_weights.iter().all(|weight| *weight <= 0.0) {
            self.split_weights.fill(1.0);
        }
    }

    pub(super) fn launch_cwd(&self) -> std::path::PathBuf {
        match self.terminal_cwd.as_ref() {
            Some(path) if path.is_absolute() => path.clone(),
            Some(path) => self.cwd.join(path),
            None => self.cwd.clone(),
        }
    }

    pub(super) fn split_launch_cwd(&self, parent_index: Option<usize>) -> std::path::PathBuf {
        match self.split_cwd {
            TerminalSplitCwd::WorkspaceRoot => self.cwd.clone(),
            TerminalSplitCwd::Initial | TerminalSplitCwd::Inherited => parent_index
                .and_then(|index| self.sessions.get(index))
                .and_then(|session| session.initial_cwd.clone())
                .unwrap_or_else(|| self.launch_cwd()),
        }
    }

    pub(super) fn active_launch_cwd(&self) -> std::path::PathBuf {
        self.active_session_index()
            .and_then(|index| self.sessions.get(index))
            .and_then(|session| session.initial_cwd.clone())
            .unwrap_or_else(|| self.launch_cwd())
    }

    pub(super) fn prune_stale_session_state(&mut self) {
        if self.sessions.is_empty() {
            self.active_session = 0;
            self.split_view = false;
            self.split_weights.clear();
            self.clear_all_session_scoped_state();
            self.search_cache = super::TerminalSearchCache::default();
            self.search_match = 0;
            return;
        }

        self.active_session = self.active_session.min(self.sessions.len() - 1);
        if self.sessions.len() == 1 {
            self.split_view = false;
        }
        self.sync_split_weights();

        let state_presence = terminal_scoped_state_presence(
            self.sessions.as_slice(),
            self.pending_kill_session_id,
            self.pending_paste_session_id,
            self.pending_multiline_paste
                .as_ref()
                .map(|pending| pending.session_id),
            self.selected_session_id,
            self.selected_text
                .as_ref()
                .map(|selection| selection.session_id),
            self.selection_drag.map(|drag| drag.session_id),
        );
        if !state_presence.pending_kill_session_started {
            self.pending_kill_session_id = None;
        }
        let pending_rename_session_exists = self
            .pending_rename_session_id
            .and_then(|session_id| self.session_index_by_id(session_id))
            .is_some();
        if self.pending_rename_session_id.is_some() && !pending_rename_session_exists {
            self.cancel_pending_rename();
        }
        if !state_presence.pending_paste_session_exists {
            self.pending_paste_session_id = None;
        }
        if !state_presence.pending_multiline_paste_session_exists {
            self.pending_multiline_paste = None;
        }
        if !state_presence.selected_session_exists {
            self.selected_session_id = None;
        }
        if !state_presence.selected_text_session_exists {
            self.selected_text = None;
        }
        if !state_presence.selection_drag_session_exists {
            self.selection_drag = None;
        }
        self.prune_stale_terminal_search_cache();
    }

    fn clear_session_scoped_state(&mut self, session_id: usize) {
        if self.pending_paste_session_id == Some(session_id) {
            self.pending_paste_session_id = None;
        }
        if self.pending_kill_session_id == Some(session_id) {
            self.pending_kill_session_id = None;
        }
        if self.pending_rename_session_id == Some(session_id) {
            self.cancel_pending_rename();
        }
        if self
            .pending_multiline_paste
            .as_ref()
            .is_some_and(|pending| pending.session_id == session_id)
        {
            self.pending_multiline_paste = None;
        }
        if self.selected_session_id == Some(session_id) {
            self.selected_session_id = None;
        }
        if self
            .selected_text
            .as_ref()
            .is_some_and(|selection| selection.session_id == session_id)
        {
            self.selected_text = None;
        }
        if self
            .selection_drag
            .is_some_and(|drag| drag.session_id == session_id)
        {
            self.selection_drag = None;
        }
    }

    fn clear_all_session_scoped_state(&mut self) {
        self.pending_paste_session_id = None;
        self.pending_kill_session_id = None;
        self.pending_rename_session_id = None;
        self.rename_session_input.clear();
        self.pending_multiline_paste = None;
        self.selected_session_id = None;
        self.selected_text = None;
        self.selection_drag = None;
    }
}

impl super::TerminalSession {
    fn is_process(&self) -> bool {
        self.process_label.is_some()
    }

    fn has_command_target(&self) -> bool {
        self.started && self.tx_command.is_some() && !self.close_requested.load(Ordering::SeqCst)
    }

    fn can_request_terminal_input(&self) -> bool {
        self.has_command_target()
            || (self.auto_start_shell && !self.close_requested.load(Ordering::SeqCst))
    }

    fn send_input(&mut self, input: impl Into<String>) -> Result<(), TerminalCommandQueueError> {
        match self.queue_input(input.into()) {
            TerminalInputQueueResult::Queued | TerminalInputQueueResult::Empty => Ok(()),
            TerminalInputQueueResult::Full(_) => Err(TerminalCommandQueueError::Full),
            TerminalInputQueueResult::Disconnected => Err(TerminalCommandQueueError::Disconnected),
        }
    }

    fn queue_input(&mut self, input: String) -> TerminalInputQueueResult {
        self.queue_input_scrolling_to_bottom(input, true)
    }

    /// Queues a response the terminal itself synthesized (a cursor-position
    /// report and similar replies). Unlike user input this must not jump the
    /// scrollback to the bottom: the user may be reading history while the
    /// running program probes the cursor.
    fn queue_synthesized_input(&mut self, input: String) -> TerminalInputQueueResult {
        self.queue_input_scrolling_to_bottom(input, false)
    }

    fn queue_input_scrolling_to_bottom(
        &mut self,
        input: String,
        scroll_to_bottom: bool,
    ) -> TerminalInputQueueResult {
        if !self.has_command_target() {
            return TerminalInputQueueResult::Empty;
        }
        let Some(tx) = &self.tx_command else {
            return TerminalInputQueueResult::Empty;
        };

        let input = bounded_terminal_input(input, TERMINAL_INPUT_MAX_BYTES);
        if input.is_empty() {
            return TerminalInputQueueResult::Empty;
        }
        match tx.try_send(TerminalCommand::Input(input)) {
            Ok(()) => {
                if scroll_to_bottom {
                    self.parser.screen_mut().set_scrollback(0);
                }
                TerminalInputQueueResult::Queued
            }
            Err(crossbeam_channel::TrySendError::Full(TerminalCommand::Input(input))) => {
                TerminalInputQueueResult::Full(input)
            }
            Err(crossbeam_channel::TrySendError::Full(_)) => {
                TerminalInputQueueResult::Full(String::new())
            }
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                TerminalInputQueueResult::Disconnected
            }
        }
    }

    pub(super) fn flush_terminal_responses(&mut self) {
        if self.parser.callbacks_mut().pending_inputs.is_empty() {
            return;
        }

        let responses = std::mem::take(&mut self.parser.callbacks_mut().pending_inputs);
        let mut responses = responses.into_iter();
        let mut retained = Vec::new();
        while let Some(response) = responses.next() {
            match self.queue_synthesized_input(response) {
                TerminalInputQueueResult::Queued | TerminalInputQueueResult::Empty => {}
                TerminalInputQueueResult::Disconnected => {
                    self.mark_stopped();
                    break;
                }
                TerminalInputQueueResult::Full(input) => {
                    if !input.is_empty() {
                        retained.push(input);
                    }
                    retained.extend(responses);
                    break;
                }
            }
        }

        if !retained.is_empty() {
            let pending_inputs = &mut self.parser.callbacks_mut().pending_inputs;
            if pending_inputs.is_empty() {
                *pending_inputs = retained;
            } else {
                retained.append(pending_inputs);
                *pending_inputs = retained;
            }
        }
    }

    pub(super) fn resize(&mut self, size: portable_pty::PtySize) {
        let (rows, cols) = self.parser.screen().size();
        if rows == size.rows && cols == size.cols {
            return;
        }
        if self.has_command_target()
            && let Some(tx) = &self.tx_command
        {
            match tx
                .try_send(TerminalCommand::Resize(size))
                .map_err(TerminalCommandQueueError::from_try_send)
            {
                Ok(()) => self.parser.screen_mut().set_size(size.rows, size.cols),
                Err(TerminalCommandQueueError::Full) => {}
                Err(TerminalCommandQueueError::Disconnected) => self.mark_stopped(),
            }
            return;
        }
        self.parser.screen_mut().set_size(size.rows, size.cols);
    }

    pub(super) fn close(&mut self) {
        self.close_requested.store(true, Ordering::SeqCst);
        if let Some(tx) = self.tx_close.take() {
            let _ = tx.try_send(());
        }
        if let Some(tx) = self.tx_command.take() {
            let _ = tx.try_send(TerminalCommand::Close);
        }
        self.mark_stopped();
    }

    pub(super) fn scroll_scrollback(&mut self, delta_rows: i32) {
        let current = self.parser.screen().scrollback();
        let next = if delta_rows.is_positive() {
            current.saturating_add(delta_rows as usize)
        } else {
            current.saturating_sub(delta_rows.unsigned_abs() as usize)
        };
        self.parser.screen_mut().set_scrollback(next);
    }

    pub(super) fn scrollback(&self) -> usize {
        self.parser.screen().scrollback()
    }

    pub(super) fn copyable_text(&self) -> String {
        trimmed_terminal_text(&self.parser.screen().contents())
    }

    fn set_scrollback_rows(&mut self, scrollback_rows: usize) {
        let scrollback_rows = clamp_terminal_scrollback_rows(scrollback_rows);
        if self.scrollback_rows == scrollback_rows {
            return;
        }

        self.scrollback_rows = scrollback_rows;
        self.rebuild_parser_with_scrollback_rows(scrollback_rows);
    }

    fn rebuild_parser_with_scrollback_rows(&mut self, scrollback_rows: usize) {
        let (rows, cols) = self.parser.screen().size();
        let previous_scrollback = self.parser.screen().scrollback();
        let alternate_screen = self.parser.screen().alternate_screen();
        let retained_rows = if alternate_screen {
            Vec::new()
        } else {
            self.retained_parser_scrollback_rows(scrollback_rows)
        };
        let state = {
            let screen = self.parser.screen_mut();
            screen.set_scrollback(0);
            screen.state_formatted()
        };
        let callbacks = std::mem::take(self.parser.callbacks_mut());
        let mut parser = vt100::Parser::new_with_callbacks(rows, cols, scrollback_rows, callbacks);

        for row in &retained_rows {
            parser.process(row.as_bytes());
            parser.process(b"\r\n");
        }
        if alternate_screen {
            parser.process(b"\x1b[?1049h");
        }
        parser.process(&state);
        parser
            .screen_mut()
            .set_scrollback(previous_scrollback.min(retained_rows.len()));
        self.parser = parser;
    }

    pub(super) fn clear_scrollback_from_terminal(&mut self) {
        self.rebuild_parser_without_scrollback();
        self.replace_search_buffer(self.copyable_text());
    }

    fn rebuild_parser_without_scrollback(&mut self) {
        let (rows, cols) = self.parser.screen().size();
        let alternate_screen = self.parser.screen().alternate_screen();
        let state = {
            let screen = self.parser.screen_mut();
            screen.set_scrollback(0);
            screen.state_formatted()
        };
        let callbacks = std::mem::take(self.parser.callbacks_mut());
        let mut parser =
            vt100::Parser::new_with_callbacks(rows, cols, self.scrollback_rows, callbacks);

        if alternate_screen {
            parser.process(b"\x1b[?1049h");
        }
        parser.process(&state);
        self.parser = parser;
    }

    fn retained_parser_scrollback_rows(&mut self, limit: usize) -> Vec<String> {
        let screen_rows = usize::from(self.parser.screen().size().0).max(1);
        let cols = self.parser.screen().size().1;
        let screen = self.parser.screen_mut();
        let previous_scrollback = screen.scrollback();
        screen.set_scrollback(usize::MAX);
        let available = screen.scrollback();
        let retain = available.min(limit);
        let first_retained = available.saturating_sub(retain);
        let mut retained_rows = Vec::with_capacity(retain);
        let mut row_index = first_retained;

        while row_index < available {
            let offset = available - row_index;
            screen.set_scrollback(offset);
            let take = offset.min(screen_rows);
            retained_rows.extend(screen.rows(0, cols).take(take));
            row_index += take;
        }

        screen.set_scrollback(previous_scrollback.min(available));
        retained_rows
    }

    fn clear_buffer(&mut self) {
        let (rows, cols) = self.parser.screen().size();
        self.parser = vt100::Parser::new_with_callbacks(
            rows,
            cols,
            self.scrollback_rows,
            super::TerminalCallbacks::default(),
        );
        self.clear_search_buffer();
    }

    fn scroll_to_top(&mut self) {
        self.parser.screen_mut().set_scrollback(usize::MAX);
    }

    fn scroll_to_bottom(&mut self) {
        self.parser.screen_mut().set_scrollback(0);
    }
}

#[cfg(test)]
mod tests;
