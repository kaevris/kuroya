#[cfg(test)]
use crate::terminal_process::TerminalFinishReason;
use crate::{
    path_display::sanitized_display_label_cow,
    terminal_process::{TerminalCommand, TerminalEvent, terminal_shell_label},
    terminal_support::initial_terminal_size,
};
use crossbeam_channel::{Receiver, Sender, bounded};
use egui::{Context, Id, KeyboardShortcut};
#[cfg(test)]
use kuroya_core::{
    DEFAULT_TERMINAL_ALT_CLICK_MOVES_CURSOR, DEFAULT_TERMINAL_BELL_DURATION_MS,
    DEFAULT_TERMINAL_COPY_ON_SELECTION, DEFAULT_TERMINAL_CURSOR_WIDTH,
    DEFAULT_TERMINAL_ENABLE_BELL, DEFAULT_TERMINAL_HIDE_ON_LAST_CLOSED,
    DEFAULT_TERMINAL_IGNORE_BRACKETED_PASTE_MODE, DEFAULT_TERMINAL_LETTER_SPACING,
    DEFAULT_TERMINAL_MIN_COLUMNS, DEFAULT_TERMINAL_MIN_ROWS,
    DEFAULT_TERMINAL_MINIMUM_CONTRAST_RATIO, DEFAULT_TERMINAL_MOUSE_WHEEL_ZOOM,
    DEFAULT_TERMINAL_SHOW_EXIT_ALERT, DEFAULT_TERMINAL_TABS_ALLOW_AGENT_CLI_TITLE,
    DEFAULT_TERMINAL_TABS_ENABLED, DEFAULT_TERMINAL_WORD_SEPARATORS,
};
use kuroya_core::{
    DEFAULT_TERMINAL_FAST_SCROLL_SENSITIVITY, DEFAULT_TERMINAL_MOUSE_WHEEL_SCROLL_SENSITIVITY,
    DEFAULT_TERMINAL_TABS_DEFAULT_ICON, DEFAULT_TERMINAL_TABS_TITLE, TerminalConfirmOnKill,
    TerminalCursorStyle, TerminalInactiveCursorStyle, TerminalMiddleClickBehavior,
    TerminalMultiLinePasteWarning, TerminalRightClickBehavior, TerminalSplitCwd,
    TerminalTabsFocusMode, TerminalTabsHideCondition, TerminalTabsLocation,
    TerminalTabsShowActions, TerminalTabsShowActiveTerminal, clamp_terminal_bell_duration_ms,
    clamp_terminal_cursor_width, clamp_terminal_font_size, clamp_terminal_letter_spacing,
    clamp_terminal_line_height, clamp_terminal_min_columns, clamp_terminal_min_rows,
    clamp_terminal_minimum_contrast_ratio, clamp_terminal_scrollback_rows,
};
use portable_pty::PtySize;
use std::{
    borrow::Cow,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};
use vt100::Callbacks;

mod actions;
mod lifecycle;
mod links;
mod persistence;
mod search;
mod ui;

pub(crate) fn shortcut_is_terminal_input(shortcut: &KeyboardShortcut) -> bool {
    ui::terminal_key_input(shortcut.logical_key, shortcut.modifiers).is_some()
}

pub(super) fn terminal_input_id(session_id: usize) -> Id {
    Id::new(("terminal-input", session_id))
}

pub(super) fn terminal_search_input_id() -> Id {
    Id::new("terminal-search-input")
}

pub(super) fn terminal_rename_input_id(session_id: usize) -> Id {
    Id::new(("terminal-rename-input", session_id))
}

#[cfg(test)]
mod tests;

const TERMINAL_SEARCH_BUFFER_MAX_BYTES: usize = 2 * 1024 * 1024;
const TERMINAL_SEARCH_BUFFER_TRIM_TARGET_BYTES: usize = TERMINAL_SEARCH_BUFFER_MAX_BYTES * 3 / 4;
const TERMINAL_WINDOW_TITLE_MAX_CHARS: usize = 120;
const TERMINAL_DEFAULT_DISPLAY_LABEL: &str = "Terminal";
const TERMINAL_PASTE_NOTICE_TTL: Duration = Duration::from_secs(5);
pub(super) const TERMINAL_PASTE_TRUNCATED_NOTICE: &str = "Paste truncated to 1 MiB";
pub(super) const TERMINAL_PASTE_BUSY_NOTICE: &str = "Terminal busy - paste not fully delivered";
pub(super) const TERMINAL_PASTE_UNDELIVERED_NOTICE: &str = "Paste could not be delivered";
pub(super) const TERMINAL_DRAIN_EVENT_BUDGET: usize = 512;
pub(super) const TERMINAL_DRAIN_BYTE_BUDGET: usize = 512 * 1024;
const TERMINAL_COMMAND_CHANNEL_BOUND: usize = TERMINAL_DRAIN_EVENT_BUDGET * 8;
const TERMINAL_OUTPUT_CHANNEL_BOUND: usize = TERMINAL_DRAIN_EVENT_BUDGET * 4;
const TERMINAL_MAX_SESSIONS: usize = 32;

pub struct TerminalPane {
    pub visible: bool,
    cwd: PathBuf,
    terminal_cwd: Option<PathBuf>,
    split_cwd: TerminalSplitCwd,
    sessions: Vec<TerminalSession>,
    active_session: usize,
    next_session_id: usize,
    last_size: PtySize,
    focus_input_on_show: bool,
    fullscreen: bool,
    split_view: bool,
    split_weights: Vec<f32>,
    search_open: bool,
    search_query: String,
    search_match: usize,
    search_focus_on_show: bool,
    search_cache: TerminalSearchCache,
    pending_paste_session_id: Option<usize>,
    pending_kill_session_id: Option<usize>,
    pending_rename_session_id: Option<usize>,
    rename_session_input: String,
    selected_session_id: Option<usize>,
    selected_text: Option<TerminalTextSelection>,
    last_bell_at: Option<Instant>,
    scrollback_rows: usize,
    shell_path: Option<String>,
    shell_label: String,
    shell_args: Vec<String>,
    min_rows: u16,
    min_columns: u16,
    font_size: f32,
    line_height: f32,
    letter_spacing: f32,
    cursor_style: TerminalCursorStyle,
    cursor_width: f32,
    cursor_blinking: bool,
    cursor_style_inactive: TerminalInactiveCursorStyle,
    draw_bold_text_in_bright_colors: bool,
    minimum_contrast_ratio: f32,
    enable_bell: bool,
    bell_duration_ms: u64,
    show_exit_alert: bool,
    hide_on_last_closed: bool,
    confirm_on_kill: TerminalConfirmOnKill,
    tabs_enabled: bool,
    tabs_default_icon: String,
    tabs_default_color: Option<String>,
    tabs_allow_agent_cli_title: bool,
    tabs_title_template: String,
    tabs_hide_condition: TerminalTabsHideCondition,
    tabs_show_active_terminal: TerminalTabsShowActiveTerminal,
    tabs_show_actions: TerminalTabsShowActions,
    tabs_focus_mode: TerminalTabsFocusMode,
    tabs_location: TerminalTabsLocation,
    right_click_behavior: TerminalRightClickBehavior,
    middle_click_behavior: TerminalMiddleClickBehavior,
    alt_click_moves_cursor: bool,
    copy_on_selection: bool,
    ignore_bracketed_paste_mode: bool,
    multi_line_paste_warning: TerminalMultiLinePasteWarning,
    pending_multiline_paste: Option<TerminalPendingPaste>,
    paste_notice: Option<TerminalPasteNotice>,
    word_separators: String,
    mouse_wheel_scroll_sensitivity: f32,
    fast_scroll_sensitivity: f32,
    mouse_wheel_zoom: bool,
    selection_drag: Option<TerminalSelectionDrag>,
    repaint_context: Option<Context>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TerminalDiagnosticsStats {
    pub(crate) sessions: usize,
    pub(crate) configured_scrollback_rows: usize,
    pub(crate) searchable_lines: usize,
    pub(crate) search_buffer_bytes: usize,
    pub(crate) active_sessions: usize,
}

struct TerminalSession {
    id: usize,
    parser: vt100::Parser<TerminalCallbacks>,
    tx_command: Option<Sender<TerminalCommand>>,
    tx_close: Option<Sender<()>>,
    close_requested: Arc<AtomicBool>,
    rx_output: Receiver<TerminalEvent>,
    tx_output: Sender<TerminalEvent>,
    started: bool,
    auto_start_shell: bool,
    scrollback_rows: usize,
    initial_cwd: Option<PathBuf>,
    custom_title: Option<String>,
    process_label: Option<String>,
    last_process_exit_code: Option<i32>,
    last_process_terminal_error: bool,
    search_buffer: String,
    search_line_count: usize,
    search_generation: u64,
    search_edit_generation: u64,
    search_pending_carriage_return: bool,
    search_ansi_state: search::TerminalSearchAnsiState,
    search_utf8_tail: Vec<u8>,
}

impl TerminalPane {
    pub(crate) fn active_terminal_search_screen_match(
        &self,
        screen: &vt100::Screen,
    ) -> Option<(u16, (usize, usize))> {
        if !self.search_open || self.search_cache.matches.is_empty() {
            return None;
        }
        let selected = self.search_match.min(self.search_cache.matches.len() - 1);
        let current = &self.search_cache.matches[selected];
        let line_count = self
            .sessions
            .iter()
            .find(|session| session.id == current.session_id)
            .map(|session| session.search_line_count)?;
        let (rows, _) = screen.size();
        let rows = usize::from(rows);
        let latest_top = line_count.saturating_sub(rows);
        let top = latest_top.saturating_sub(screen.scrollback());
        let row = current.line.checked_sub(top)?;
        if row >= rows {
            return None;
        }
        Some((u16::try_from(row).ok()?, (current.start, current.end)))
    }

    pub(crate) fn diagnostics_stats(&self) -> TerminalDiagnosticsStats {
        TerminalDiagnosticsStats {
            sessions: self.sessions.len(),
            configured_scrollback_rows: self.scrollback_rows,
            searchable_lines: self
                .sessions
                .iter()
                .map(|session| session.search_line_count)
                .sum(),
            search_buffer_bytes: self
                .sessions
                .iter()
                .map(|session| session.search_buffer.len())
                .sum(),
            active_sessions: self
                .sessions
                .iter()
                .filter(|session| session.started)
                .count(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TerminalSearchMatch {
    session_id: usize,
    line: usize,
    start: usize,
    end: usize,
    preview: Arc<String>,
}

#[derive(Default)]
struct TerminalSearchCache {
    scope: TerminalSearchCacheScope,
    query: String,
    matches: Vec<TerminalSearchMatch>,
    progress: TerminalSearchCacheProgress,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum TerminalSearchCacheScope {
    #[default]
    Empty,
    Single {
        session_id: usize,
        generation: u64,
    },
    Split {
        sessions: Vec<(usize, u64)>,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum TerminalSearchCacheProgress {
    #[default]
    Empty,
    Single(TerminalSearchCacheSessionProgress),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TerminalSearchCacheSessionProgress {
    session_id: usize,
    edit_generation: u64,
    resume_byte: usize,
    resume_line: usize,
}

struct TerminalTextSelection {
    session_id: usize,
    text: String,
    range: TerminalSelectionRange,
}

struct TerminalPendingPaste {
    session_id: usize,
    text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TerminalPasteNotice {
    message: String,
    created: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TerminalCellPosition {
    row: u16,
    col: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TerminalSelectionRange {
    start: TerminalCellPosition,
    end: TerminalCellPosition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TerminalSelectionDrag {
    session_id: usize,
    anchor: TerminalCellPosition,
}

#[derive(Default)]
struct TerminalCallbacks {
    pending_inputs: Vec<String>,
    pending_bells: usize,
    pending_clear_scrollback: bool,
    window_title: Option<String>,
    shell_integration: TerminalShellIntegrationState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalShellIntegrationMarker {
    PromptStart,
    PromptEnd,
    CommandStart,
    CommandFinish,
}

#[derive(Default, Debug, PartialEq, Eq)]
struct TerminalShellIntegrationState {
    last_marker: Option<TerminalShellIntegrationMarker>,
    prompt_active: bool,
    command_running: bool,
    last_command_exit_code: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalCommandStatus {
    Unknown,
    Prompt,
    Running,
    Succeeded,
    Failed(i32),
    TerminalError,
    Finished,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminalProcessSessionState {
    Running,
    Exited(i32),
    TerminalError,
    Stopped,
}

impl Callbacks for TerminalCallbacks {
    fn audible_bell(&mut self, _screen: &mut vt100::Screen) {
        self.pending_bells = self.pending_bells.saturating_add(1);
    }

    fn visual_bell(&mut self, _screen: &mut vt100::Screen) {
        self.pending_bells = self.pending_bells.saturating_add(1);
    }

    fn set_window_title(&mut self, _screen: &mut vt100::Screen, title: &[u8]) {
        self.window_title = normalized_terminal_window_title(title);
    }

    fn unhandled_osc(&mut self, _screen: &mut vt100::Screen, params: &[&[u8]]) {
        if let Some((marker, exit_code)) = parse_terminal_shell_integration_marker(params) {
            self.apply_shell_integration_marker(marker, exit_code);
        }
    }

    fn unhandled_csi(
        &mut self,
        screen: &mut vt100::Screen,
        i1: Option<u8>,
        _i2: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        let is_cursor_position_query = c == 'n'
            && matches!(i1, None | Some(b'?'))
            && params.first().and_then(|param| param.first()) == Some(&6);
        if is_cursor_position_query {
            let (row, col) = screen.cursor_position();
            self.pending_inputs
                .push(format!("\x1b[{};{}R", row + 1, col + 1));
        }
        if csi_erases_scrollback(i1, params, c) {
            self.pending_clear_scrollback = true;
        }
    }
}

fn csi_erases_scrollback(i1: Option<u8>, params: &[&[u16]], c: char) -> bool {
    c == 'J'
        && matches!(i1, None | Some(b'?'))
        && params.first().and_then(|param| param.first()).copied() == Some(3)
}

impl TerminalCallbacks {
    fn reset_shell_integration(&mut self) {
        self.shell_integration = TerminalShellIntegrationState::default();
    }

    fn apply_shell_integration_marker(
        &mut self,
        marker: TerminalShellIntegrationMarker,
        exit_code: Option<i32>,
    ) {
        self.shell_integration.last_marker = Some(marker);
        match marker {
            TerminalShellIntegrationMarker::PromptStart => {
                self.shell_integration.prompt_active = true;
                self.shell_integration.command_running = false;
            }
            TerminalShellIntegrationMarker::PromptEnd => {
                self.shell_integration.prompt_active = false;
            }
            TerminalShellIntegrationMarker::CommandStart => {
                self.shell_integration.prompt_active = false;
                self.shell_integration.command_running = true;
                self.shell_integration.last_command_exit_code = None;
            }
            TerminalShellIntegrationMarker::CommandFinish => {
                self.shell_integration.command_running = false;
                self.shell_integration.last_command_exit_code = exit_code;
            }
        }
    }
}

fn parse_terminal_shell_integration_marker(
    params: &[&[u8]],
) -> Option<(TerminalShellIntegrationMarker, Option<i32>)> {
    let [namespace, marker, rest @ ..] = params else {
        return None;
    };
    if *namespace != b"133" && *namespace != b"633" {
        return None;
    }

    match *marker {
        b"A" => Some((TerminalShellIntegrationMarker::PromptStart, None)),
        b"B" => Some((TerminalShellIntegrationMarker::PromptEnd, None)),
        b"C" => Some((TerminalShellIntegrationMarker::CommandStart, None)),
        b"D" => Some((
            TerminalShellIntegrationMarker::CommandFinish,
            rest.first()
                .and_then(|code| std::str::from_utf8(code).ok())
                .and_then(|code| code.parse::<i32>().ok()),
        )),
        _ => None,
    }
}

impl TerminalPane {
    pub(crate) fn set_repaint_context(&mut self, ctx: Context) {
        self.repaint_context = Some(ctx);
    }

    pub(crate) fn repaint_context(&self) -> Option<Context> {
        self.repaint_context.clone()
    }

    pub(crate) fn input_focused(&self, ctx: &Context) -> bool {
        self.visible
            && ctx.memory(|memory| {
                let search_focused =
                    self.search_open && memory.has_focus(terminal_search_input_id());
                let rename_focused = self.pending_rename_session_id.is_some_and(|session_id| {
                    memory.has_focus(terminal_rename_input_id(session_id))
                });
                self.sessions
                    .iter()
                    .any(|session| memory.has_focus(terminal_input_id(session.id)))
                    || search_focused
                    || rename_focused
            })
    }

    #[cfg(test)]
    pub(crate) fn add_process_session_for_test(
        &mut self,
        session_id: usize,
    ) -> Receiver<TerminalCommand> {
        let (tx_command, rx_command) = crossbeam_channel::unbounded();
        let mut session = TerminalSession::new(session_id, self.last_size, self.scrollback_rows);
        session.started = true;
        session.tx_command = Some(tx_command);
        session.process_label = Some("test process".to_owned());

        self.next_session_id = self.next_session_id.max(session_id.saturating_add(1));
        self.sessions.push(session);
        self.split_weights.push(1.0);
        self.active_session = self.sessions.len().saturating_sub(1);
        self.visible = true;
        rx_command
    }

    #[cfg(test)]
    pub(crate) fn begin_rename_session_for_test(&mut self, index: usize) {
        self.begin_rename_session(index);
    }

    #[cfg(test)]
    pub(crate) fn finish_process_session_for_test(
        &self,
        session_id: usize,
        process_exit_code: Option<i32>,
    ) -> bool {
        self.sessions
            .iter()
            .find(|session| session.id == session_id)
            .is_some_and(|session| {
                session
                    .tx_output
                    .send(TerminalEvent::Finished {
                        message: None,
                        process_exit_code,
                        reason: TerminalFinishReason::ProcessExit,
                    })
                    .is_ok()
            })
    }

    #[cfg(test)]
    pub(crate) fn fail_process_session_for_test(&self, session_id: usize) -> bool {
        self.sessions
            .iter()
            .find(|session| session.id == session_id)
            .is_some_and(|session| {
                session
                    .tx_output
                    .send(TerminalEvent::Finished {
                        message: None,
                        process_exit_code: None,
                        reason: TerminalFinishReason::TerminalError,
                    })
                    .is_ok()
            })
    }

    #[cfg(test)]
    pub(crate) fn session_ids_for_test(&self) -> Vec<usize> {
        self.sessions.iter().map(|session| session.id).collect()
    }

    #[cfg(test)]
    pub fn new(cwd: PathBuf, scrollback_rows: usize, font_size: f32, line_height: f32) -> Self {
        Self::with_settings(
            cwd,
            scrollback_rows,
            None,
            Vec::new(),
            None,
            TerminalSplitCwd::default(),
            DEFAULT_TERMINAL_MIN_ROWS,
            DEFAULT_TERMINAL_MIN_COLUMNS,
            font_size,
            line_height,
            DEFAULT_TERMINAL_LETTER_SPACING,
            TerminalCursorStyle::default(),
            DEFAULT_TERMINAL_CURSOR_WIDTH,
            false,
            TerminalInactiveCursorStyle::default(),
            true,
            DEFAULT_TERMINAL_MINIMUM_CONTRAST_RATIO,
            DEFAULT_TERMINAL_ENABLE_BELL,
            DEFAULT_TERMINAL_BELL_DURATION_MS,
            DEFAULT_TERMINAL_SHOW_EXIT_ALERT,
            DEFAULT_TERMINAL_HIDE_ON_LAST_CLOSED,
            TerminalConfirmOnKill::default(),
            DEFAULT_TERMINAL_TABS_ENABLED,
            DEFAULT_TERMINAL_TABS_DEFAULT_ICON.to_owned(),
            None,
            DEFAULT_TERMINAL_TABS_ALLOW_AGENT_CLI_TITLE,
            DEFAULT_TERMINAL_TABS_TITLE.to_owned(),
            TerminalTabsHideCondition::default(),
            TerminalTabsShowActiveTerminal::default(),
            TerminalTabsShowActions::default(),
            TerminalTabsFocusMode::default(),
            TerminalTabsLocation::default(),
            TerminalRightClickBehavior::default(),
            TerminalMiddleClickBehavior::default(),
            DEFAULT_TERMINAL_ALT_CLICK_MOVES_CURSOR,
            DEFAULT_TERMINAL_COPY_ON_SELECTION,
            DEFAULT_TERMINAL_IGNORE_BRACKETED_PASTE_MODE,
            TerminalMultiLinePasteWarning::default(),
            DEFAULT_TERMINAL_WORD_SEPARATORS.to_owned(),
            DEFAULT_TERMINAL_MOUSE_WHEEL_SCROLL_SENSITIVITY,
            DEFAULT_TERMINAL_FAST_SCROLL_SENSITIVITY,
            DEFAULT_TERMINAL_MOUSE_WHEEL_ZOOM,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_settings(
        cwd: PathBuf,
        scrollback_rows: usize,
        shell_path: Option<String>,
        shell_args: Vec<String>,
        terminal_cwd: Option<String>,
        split_cwd: TerminalSplitCwd,
        min_rows: u16,
        min_columns: u16,
        font_size: f32,
        line_height: f32,
        letter_spacing: f32,
        cursor_style: TerminalCursorStyle,
        cursor_width: f32,
        cursor_blinking: bool,
        cursor_style_inactive: TerminalInactiveCursorStyle,
        draw_bold_text_in_bright_colors: bool,
        minimum_contrast_ratio: f32,
        enable_bell: bool,
        bell_duration_ms: u64,
        show_exit_alert: bool,
        hide_on_last_closed: bool,
        confirm_on_kill: TerminalConfirmOnKill,
        tabs_enabled: bool,
        tabs_default_icon: String,
        tabs_default_color: Option<String>,
        tabs_allow_agent_cli_title: bool,
        tabs_title_template: String,
        tabs_hide_condition: TerminalTabsHideCondition,
        tabs_show_active_terminal: TerminalTabsShowActiveTerminal,
        tabs_show_actions: TerminalTabsShowActions,
        tabs_focus_mode: TerminalTabsFocusMode,
        tabs_location: TerminalTabsLocation,
        right_click_behavior: TerminalRightClickBehavior,
        middle_click_behavior: TerminalMiddleClickBehavior,
        alt_click_moves_cursor: bool,
        copy_on_selection: bool,
        ignore_bracketed_paste_mode: bool,
        multi_line_paste_warning: TerminalMultiLinePasteWarning,
        word_separators: String,
        mouse_wheel_scroll_sensitivity: f32,
        fast_scroll_sensitivity: f32,
        mouse_wheel_zoom: bool,
    ) -> Self {
        let min_rows = clamp_terminal_min_rows(min_rows);
        let min_columns = clamp_terminal_min_columns(min_columns);
        let size = initial_terminal_size(min_rows, min_columns);
        let scrollback_rows = clamp_terminal_scrollback_rows(scrollback_rows);
        let font_size = clamp_terminal_font_size(font_size);
        let line_height = clamp_terminal_line_height(line_height);
        let letter_spacing = clamp_terminal_letter_spacing(letter_spacing);
        let cursor_width = clamp_terminal_cursor_width(cursor_width);
        let minimum_contrast_ratio = clamp_terminal_minimum_contrast_ratio(minimum_contrast_ratio);
        let bell_duration_ms = clamp_terminal_bell_duration_ms(bell_duration_ms);
        let shell_path = normalized_shell_path(shell_path);
        let shell_label = terminal_shell_label(shell_path.as_deref());
        let shell_args = normalized_shell_args(shell_args);
        let mouse_wheel_scroll_sensitivity = kuroya_core::clamp_terminal_scroll_sensitivity(
            mouse_wheel_scroll_sensitivity,
            DEFAULT_TERMINAL_MOUSE_WHEEL_SCROLL_SENSITIVITY,
        );
        let fast_scroll_sensitivity = kuroya_core::clamp_terminal_scroll_sensitivity(
            fast_scroll_sensitivity,
            DEFAULT_TERMINAL_FAST_SCROLL_SENSITIVITY,
        );
        Self {
            visible: false,
            cwd,
            terminal_cwd: normalized_terminal_cwd(terminal_cwd),
            split_cwd,
            sessions: Vec::new(),
            active_session: 0,
            next_session_id: 1,
            last_size: size,
            focus_input_on_show: false,
            fullscreen: false,
            split_view: false,
            split_weights: Vec::new(),
            search_open: false,
            search_query: String::new(),
            search_match: 0,
            search_focus_on_show: false,
            search_cache: TerminalSearchCache::default(),
            pending_paste_session_id: None,
            pending_kill_session_id: None,
            pending_rename_session_id: None,
            rename_session_input: String::new(),
            selected_session_id: None,
            selected_text: None,
            last_bell_at: None,
            scrollback_rows,
            shell_path,
            shell_label,
            shell_args,
            min_rows,
            min_columns,
            font_size,
            line_height,
            letter_spacing,
            cursor_style,
            cursor_width,
            cursor_blinking,
            cursor_style_inactive,
            draw_bold_text_in_bright_colors,
            minimum_contrast_ratio,
            enable_bell,
            bell_duration_ms,
            show_exit_alert,
            hide_on_last_closed,
            confirm_on_kill,
            tabs_enabled,
            tabs_default_icon: normalized_terminal_tab_icon(tabs_default_icon),
            tabs_default_color: normalized_terminal_tab_color(tabs_default_color),
            tabs_allow_agent_cli_title,
            tabs_title_template: normalized_terminal_tabs_title(tabs_title_template),
            tabs_hide_condition,
            tabs_show_active_terminal,
            tabs_show_actions,
            tabs_focus_mode,
            tabs_location,
            right_click_behavior,
            middle_click_behavior,
            alt_click_moves_cursor,
            copy_on_selection,
            ignore_bracketed_paste_mode,
            multi_line_paste_warning,
            pending_multiline_paste: None,
            paste_notice: None,
            word_separators,
            mouse_wheel_scroll_sensitivity,
            fast_scroll_sensitivity,
            mouse_wheel_zoom,
            selection_drag: None,
            repaint_context: None,
        }
    }
}

fn normalized_shell_path(shell_path: Option<String>) -> Option<String> {
    normalized_owned_setting_text(shell_path)
        .filter(|path| !contains_terminal_shell_profile_unsafe_char(path))
}

fn normalized_shell_args(shell_args: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::with_capacity(shell_args.len());
    for arg in shell_args {
        if let Some(arg) = normalized_owned_setting_text(Some(arg))
            .filter(|arg| !contains_terminal_shell_profile_unsafe_char(arg))
        {
            normalized.push(arg);
        }
    }
    normalized
}

fn contains_terminal_shell_profile_unsafe_char(value: &str) -> bool {
    value.chars().any(|ch| {
        ch.is_control()
            || matches!(
                ch,
                '\u{061c}'
                    | '\u{200e}'
                    | '\u{200f}'
                    | '\u{202a}'..='\u{202e}'
                    | '\u{2066}'..='\u{2069}'
            )
    })
}

fn normalized_terminal_cwd(terminal_cwd: Option<String>) -> Option<PathBuf> {
    terminal_cwd
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty() && !contains_terminal_cwd_control(path))
        .map(PathBuf::from)
}

fn contains_terminal_cwd_control(path: &str) -> bool {
    path.chars().any(char::is_control)
}

fn normalized_terminal_tab_icon(icon: String) -> String {
    let trimmed = icon.trim();
    if trimmed.is_empty() {
        DEFAULT_TERMINAL_TABS_DEFAULT_ICON.to_owned()
    } else if trimmed.len() == icon.len() {
        icon
    } else {
        trimmed.to_owned()
    }
}

fn normalized_terminal_tabs_title(title: String) -> String {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        DEFAULT_TERMINAL_TABS_TITLE.to_owned()
    } else if trimmed.len() == title.len() {
        title
    } else {
        trimmed.to_owned()
    }
}

fn normalized_terminal_tab_color(color: Option<String>) -> Option<String> {
    normalized_owned_setting_text(color)
}

fn normalized_owned_setting_text(value: Option<String>) -> Option<String> {
    let value = value?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else if trimmed.len() == value.len() {
        Some(value)
    } else {
        Some(trimmed.to_owned())
    }
}

fn normalized_terminal_window_title(title: &[u8]) -> Option<String> {
    let title = String::from_utf8_lossy(title);
    let title = normalized_terminal_window_title_cow(&title);
    if title.is_empty() {
        None
    } else {
        Some(title.into_owned())
    }
}

fn normalized_terminal_custom_title(title: &str) -> Option<String> {
    normalized_terminal_window_title(title.as_bytes())
}

fn normalized_terminal_window_title_cow(title: &str) -> Cow<'_, str> {
    sanitized_display_label_cow(title, TERMINAL_WINDOW_TITLE_MAX_CHARS, "")
}

impl TerminalSession {
    fn new(id: usize, size: PtySize, scrollback_rows: usize) -> Self {
        let (tx_output, rx_output) = terminal_output_channel();
        let scrollback_rows = clamp_terminal_scrollback_rows(scrollback_rows);
        Self {
            id,
            parser: vt100::Parser::new_with_callbacks(
                size.rows,
                size.cols,
                scrollback_rows,
                TerminalCallbacks::default(),
            ),
            tx_command: None,
            tx_close: None,
            close_requested: Arc::new(AtomicBool::new(false)),
            rx_output,
            tx_output,
            started: false,
            auto_start_shell: true,
            scrollback_rows,
            initial_cwd: None,
            custom_title: None,
            process_label: None,
            last_process_exit_code: None,
            last_process_terminal_error: false,
            search_buffer: String::new(),
            search_line_count: 0,
            search_generation: 0,
            search_edit_generation: 0,
            search_pending_carriage_return: false,
            search_ansi_state: search::TerminalSearchAnsiState::default(),
            search_utf8_tail: Vec::new(),
        }
    }

    fn command_status(&self) -> TerminalCommandStatus {
        if !self.started {
            if self.last_process_terminal_error {
                return TerminalCommandStatus::TerminalError;
            }
            return match self.last_process_exit_code {
                Some(0) => TerminalCommandStatus::Succeeded,
                Some(exit_code) => TerminalCommandStatus::Failed(exit_code),
                None => TerminalCommandStatus::Stopped,
            };
        }

        let shell = &self.parser.callbacks().shell_integration;
        if shell.command_running {
            return TerminalCommandStatus::Running;
        }
        if let Some(exit_code) = shell.last_command_exit_code {
            return if exit_code == 0 {
                TerminalCommandStatus::Succeeded
            } else {
                TerminalCommandStatus::Failed(exit_code)
            };
        }
        if shell.last_marker == Some(TerminalShellIntegrationMarker::CommandFinish) {
            return TerminalCommandStatus::Finished;
        }
        if shell.prompt_active {
            return TerminalCommandStatus::Prompt;
        }
        TerminalCommandStatus::Unknown
    }

    fn reset_shell_integration_state(&mut self) {
        self.parser.callbacks_mut().reset_shell_integration();
    }
}

fn terminal_output_channel() -> (Sender<TerminalEvent>, Receiver<TerminalEvent>) {
    bounded(TERMINAL_OUTPUT_CHANNEL_BOUND)
}

fn terminal_command_channel() -> (Sender<TerminalCommand>, Receiver<TerminalCommand>) {
    bounded(TERMINAL_COMMAND_CHANNEL_BOUND)
}

fn terminal_close_channel() -> (Sender<()>, Receiver<()>) {
    bounded(1)
}

#[cfg(test)]
mod window_title_tests {
    use std::borrow::Cow;

    use super::{
        TERMINAL_WINDOW_TITLE_MAX_CHARS, normalized_terminal_window_title,
        normalized_terminal_window_title_cow,
    };

    #[test]
    fn terminal_window_title_cow_borrows_clean_ascii_and_unicode() {
        assert!(matches!(
            normalized_terminal_window_title_cow("Build succeeded"),
            Cow::Borrowed("Build succeeded")
        ));

        let unicode = "Build \u{03bb} ready";
        match normalized_terminal_window_title_cow(unicode) {
            Cow::Borrowed(title) => assert_eq!(title, unicode),
            Cow::Owned(title) => panic!("expected borrowed title, got {title:?}"),
        }
    }

    #[test]
    fn terminal_window_title_cow_owns_dirty_truncated_and_blank_hostile_titles() {
        let long = "title-".repeat(TERMINAL_WINDOW_TITLE_MAX_CHARS);
        let cases = [
            "  Build succeeded  ",
            "Build\r\nsucceeded",
            "\u{202e}Build succeeded",
            "\r\n\t\u{202e}\u{2066}",
            long.as_str(),
        ];

        for title in cases {
            assert!(
                matches!(normalized_terminal_window_title_cow(title), Cow::Owned(_)),
                "expected owned title for {title:?}"
            );
        }

        assert_eq!(
            normalized_terminal_window_title_cow("\r\n\t\u{202e}\u{2066}").as_ref(),
            ""
        );
    }

    #[test]
    fn terminal_window_title_normalization_sanitizes_control_bidi_and_bounds_length() {
        let raw = format!(
            "  Build\r\n{}\u{202e}\u{2066}tail  ",
            "title-".repeat(TERMINAL_WINDOW_TITLE_MAX_CHARS)
        );

        let title = normalized_terminal_window_title(raw.as_bytes()).expect("window title");

        assert!(title.starts_with("Build title-"));
        assert!(title.contains("..."));
        assert!(!title.contains('\n'));
        assert!(!title.contains('\u{202e}'));
        assert!(!title.contains('\u{2066}'));
        assert!(title.chars().count() <= TERMINAL_WINDOW_TITLE_MAX_CHARS);
    }

    #[test]
    fn terminal_window_title_normalization_drops_blank_hostile_titles() {
        assert_eq!(
            normalized_terminal_window_title("\r\n\t\u{202e}\u{2066}".as_bytes()),
            None
        );
    }

    #[test]
    fn terminal_window_title_normalization_keeps_clean_titles() {
        assert_eq!(
            normalized_terminal_window_title(b"Build succeeded"),
            Some("Build succeeded".to_owned())
        );
        assert_eq!(
            normalized_terminal_window_title("Build \u{03bb} ready".as_bytes()),
            Some("Build \u{03bb} ready".to_owned())
        );
    }

    #[test]
    fn terminal_window_title_normalization_keeps_invalid_utf8_lossy_and_sanitized() {
        let mut raw = b"  Build\xff\r\nready".to_vec();
        raw.extend_from_slice("\u{202e}".as_bytes());

        assert_eq!(
            normalized_terminal_window_title(&raw),
            Some("Build\u{fffd} ready".to_owned())
        );
    }
}

#[cfg(test)]
mod shell_setting_tests {
    use super::{normalized_shell_args, normalized_shell_path};

    #[test]
    fn terminal_shell_settings_reject_control_and_bidi_values() {
        assert_eq!(
            normalized_shell_path(Some(" pwsh.exe ".to_owned())).as_deref(),
            Some("pwsh.exe")
        );
        assert_eq!(
            normalized_shell_path(Some("pwsh.exe\n-NoProfile".to_owned())),
            None
        );
        assert_eq!(
            normalized_shell_path(Some("pwsh.exe\u{202e}".to_owned())),
            None
        );

        assert_eq!(
            normalized_shell_args(vec![
                " -NoLogo ".to_owned(),
                "-NoProfile\u{2066}".to_owned(),
                "\u{7}".to_owned(),
                "-ExecutionPolicy".to_owned(),
            ]),
            vec!["-NoLogo".to_owned(), "-ExecutionPolicy".to_owned()]
        );
    }
}
