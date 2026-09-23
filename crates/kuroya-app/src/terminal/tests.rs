use super::{
    TERMINAL_COMMAND_CHANNEL_BOUND, TERMINAL_DRAIN_BYTE_BUDGET, TERMINAL_DRAIN_EVENT_BUDGET,
    TERMINAL_OUTPUT_CHANNEL_BOUND, TERMINAL_SEARCH_BUFFER_MAX_BYTES,
    TERMINAL_SEARCH_BUFFER_TRIM_TARGET_BYTES, TerminalCallbacks, TerminalCellPosition,
    TerminalCommandStatus, TerminalPane, TerminalPendingPaste, TerminalProcessSessionState,
    TerminalSearchCache, TerminalSearchCacheProgress, TerminalSearchCacheScope,
    TerminalSearchMatch, TerminalSelectionDrag, TerminalSelectionRange, TerminalSession,
    TerminalShellIntegrationMarker, TerminalTextSelection,
    actions::terminal_zoomed_font_size,
    actions::{terminal_alt_click_cursor_input, terminal_paste_input},
    links::{terminal_path_link_at_text_position, terminal_path_link_at_text_position_with_home},
    persistence::{
        max_persisted_terminal_scrollback_total_bytes_for_test,
        normalized_persisted_terminal_label_for_test, persisted_terminal_scrollback_for_test,
        restored_terminal_split_weights_for_test,
    },
    search::{
        TerminalVisibleSearchSpan, normalized_terminal_search_query_for_test, terminal_plain_text,
        terminal_search_control_sequence_max_chars_for_test,
        terminal_search_full_scan_max_bytes_for_test, terminal_search_match_limit_for_test,
        terminal_search_matches, terminal_search_query_max_chars_for_test,
        terminal_search_result_label_for_test,
        terminal_search_resume_point_from_line_count_for_test,
        terminal_search_scrollback_for_line_for_test, terminal_visible_search_spans,
        trim_terminal_search_buffer_for_test,
    },
    terminal_close_channel, terminal_command_channel,
    ui::{
        parse_terminal_tab_hex_color, push_terminal_text_run, terminal_ansi_palette_from_colors,
        terminal_bold_foreground_color, terminal_compact_path_for_test, terminal_contrast_color,
        terminal_foreground_color, terminal_path_tooltip_for_test, terminal_rendered_text_color,
        terminal_session_label, terminal_tab_icon_kind, terminal_text_input_from_event,
        terminal_text_runs_can_merge, terminal_word_selection_at_cell,
    },
};
use crate::{
    persistence::{PersistedTerminalProcessStatus, PersistedTerminalSession},
    terminal_process::{
        TerminalCommand, TerminalEvent, TerminalFinishReason, default_shell_label,
        terminal_shell_label,
    },
    terminal_support::{
        TERMINAL_SCROLLBACK_ROWS, initial_terminal_size, terminal_size_from_points,
    },
    ui_icons::IconKind,
};
use crossbeam_channel::{Receiver, Sender, bounded, unbounded};
use egui::{Event, ImeEvent, Modifiers, RawInput, vec2};
use kuroya_core::{
    DEFAULT_TERMINAL_ALT_CLICK_MOVES_CURSOR, DEFAULT_TERMINAL_BELL_DURATION_MS,
    DEFAULT_TERMINAL_COPY_ON_SELECTION, DEFAULT_TERMINAL_CURSOR_WIDTH,
    DEFAULT_TERMINAL_ENABLE_BELL, DEFAULT_TERMINAL_FAST_SCROLL_SENSITIVITY,
    DEFAULT_TERMINAL_FONT_SIZE, DEFAULT_TERMINAL_HIDE_ON_LAST_CLOSED,
    DEFAULT_TERMINAL_IGNORE_BRACKETED_PASTE_MODE, DEFAULT_TERMINAL_LETTER_SPACING,
    DEFAULT_TERMINAL_LINE_HEIGHT, DEFAULT_TERMINAL_MIN_COLUMNS, DEFAULT_TERMINAL_MIN_ROWS,
    DEFAULT_TERMINAL_MINIMUM_CONTRAST_RATIO, DEFAULT_TERMINAL_MOUSE_WHEEL_SCROLL_SENSITIVITY,
    DEFAULT_TERMINAL_MOUSE_WHEEL_ZOOM, DEFAULT_TERMINAL_SHOW_EXIT_ALERT,
    DEFAULT_TERMINAL_TABS_ALLOW_AGENT_CLI_TITLE, DEFAULT_TERMINAL_TABS_DEFAULT_ICON,
    DEFAULT_TERMINAL_TABS_ENABLED, DEFAULT_TERMINAL_TABS_TITLE, DEFAULT_TERMINAL_WORD_SEPARATORS,
    MAX_TERMINAL_FONT_SIZE, MIN_TERMINAL_CURSOR_WIDTH, MIN_TERMINAL_FONT_SIZE,
    MIN_TERMINAL_LETTER_SPACING, MIN_TERMINAL_LINE_HEIGHT, MIN_TERMINAL_MIN_COLUMNS,
    MIN_TERMINAL_MIN_ROWS, MIN_TERMINAL_SCROLL_SENSITIVITY, MIN_TERMINAL_SCROLLBACK_ROWS,
    TerminalConfirmOnExit, TerminalConfirmOnKill, TerminalCursorStyle, TerminalInactiveCursorStyle,
    TerminalMiddleClickBehavior, TerminalMultiLinePasteWarning, TerminalRightClickBehavior,
    TerminalSplitCwd, TerminalTabsFocusMode, TerminalTabsHideCondition, TerminalTabsLocation,
    TerminalTabsShowActions, TerminalTabsShowActiveTerminal,
};
use portable_pty::PtySize;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

fn test_terminal_size() -> PtySize {
    initial_terminal_size(DEFAULT_TERMINAL_MIN_ROWS, DEFAULT_TERMINAL_MIN_COLUMNS)
}

mod direct_input_close;
mod path_links;
mod process_output_state;
mod render_colors;
mod restore_snapshots;
mod search_cache;
mod shell_integration;
mod tabs_labels;

#[test]
fn terminal_text_input_accepts_committed_ime_text_only() {
    assert_eq!(
        terminal_text_input_from_event(&Event::Text("x".to_owned())),
        Some("x")
    );
    assert_eq!(
        terminal_text_input_from_event(&Event::Ime(ImeEvent::Commit("文".to_owned()))),
        Some("文")
    );
    assert_eq!(
        terminal_text_input_from_event(&Event::Ime(ImeEvent::Commit(String::new()))),
        None
    );
    assert_eq!(
        terminal_text_input_from_event(&Event::Ime(ImeEvent::Preedit("wen".to_owned()))),
        None
    );
    assert_eq!(
        terminal_text_input_from_event(&Event::Ime(ImeEvent::Enabled)),
        None
    );
    assert_eq!(
        terminal_text_input_from_event(&Event::Ime(ImeEvent::Disabled)),
        None
    );
}

#[test]
fn terminal_starts_hidden_stopped_and_without_sessions_by_default() {
    let pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );

    assert!(!pane.visible);
    assert!(pane.sessions.is_empty());
    assert_eq!(pane.active_session, 0);
    assert_eq!(pane.next_session_id, 1);
    assert!(!pane.focus_input_on_show);
    assert!(!pane.fullscreen);
    assert!(!pane.split_view);
    assert!(pane.split_weights.is_empty());
    assert!(!pane.search_open);
    assert!(pane.search_query.is_empty());
    assert_eq!(pane.search_match, 0);
    assert!(!pane.search_focus_on_show);
    assert_eq!(pane.pending_paste_session_id, None);
    assert_eq!(pane.pending_kill_session_id, None);
    assert_eq!(pane.selected_session_id, None);
    assert!(pane.selected_text.is_none());
    assert!(pane.selection_drag.is_none());
    assert!(pane.last_bell_at.is_none());
    assert_eq!(pane.scrollback_rows, TERMINAL_SCROLLBACK_ROWS);
    assert_eq!(pane.shell_path, None);
    assert!(pane.shell_args.is_empty());
    assert_eq!(pane.terminal_cwd, None);
    assert_eq!(pane.split_cwd, TerminalSplitCwd::Inherited);
    assert_eq!(pane.launch_cwd(), PathBuf::from("workspace"));
    assert_eq!(pane.min_rows, DEFAULT_TERMINAL_MIN_ROWS);
    assert_eq!(pane.min_columns, DEFAULT_TERMINAL_MIN_COLUMNS);
    assert_eq!(pane.font_size, DEFAULT_TERMINAL_FONT_SIZE);
    assert_eq!(pane.line_height, DEFAULT_TERMINAL_LINE_HEIGHT);
    assert_eq!(pane.letter_spacing, DEFAULT_TERMINAL_LETTER_SPACING);
    assert_eq!(pane.cursor_style, TerminalCursorStyle::Block);
    assert_eq!(pane.cursor_width, DEFAULT_TERMINAL_CURSOR_WIDTH);
    assert!(!pane.cursor_blinking);
    assert_eq!(
        pane.cursor_style_inactive,
        TerminalInactiveCursorStyle::Outline
    );
    assert!(pane.draw_bold_text_in_bright_colors);
    assert_eq!(
        pane.minimum_contrast_ratio,
        DEFAULT_TERMINAL_MINIMUM_CONTRAST_RATIO
    );
    assert_eq!(pane.enable_bell, DEFAULT_TERMINAL_ENABLE_BELL);
    assert_eq!(pane.bell_duration_ms, DEFAULT_TERMINAL_BELL_DURATION_MS);
    assert_eq!(pane.show_exit_alert, DEFAULT_TERMINAL_SHOW_EXIT_ALERT);
    assert_eq!(
        pane.hide_on_last_closed,
        DEFAULT_TERMINAL_HIDE_ON_LAST_CLOSED
    );
    assert_eq!(pane.confirm_on_kill, TerminalConfirmOnKill::Editor);
    assert_eq!(pane.tabs_enabled, DEFAULT_TERMINAL_TABS_ENABLED);
    assert_eq!(pane.tabs_default_icon, DEFAULT_TERMINAL_TABS_DEFAULT_ICON);
    assert_eq!(pane.tabs_default_color, None);
    assert_eq!(
        pane.tabs_allow_agent_cli_title,
        DEFAULT_TERMINAL_TABS_ALLOW_AGENT_CLI_TITLE
    );
    assert_eq!(pane.tabs_title_template, DEFAULT_TERMINAL_TABS_TITLE);
    assert_eq!(
        pane.tabs_hide_condition,
        TerminalTabsHideCondition::SingleTerminal
    );
    assert_eq!(
        pane.tabs_show_actions,
        TerminalTabsShowActions::SingleTerminalOrNarrow
    );
    assert_eq!(
        pane.tabs_show_active_terminal,
        TerminalTabsShowActiveTerminal::SingleTerminalOrNarrow
    );
    assert_eq!(pane.tabs_focus_mode, TerminalTabsFocusMode::SingleClick);
    assert_eq!(pane.tabs_location, TerminalTabsLocation::Top);
    assert_eq!(
        pane.right_click_behavior,
        TerminalRightClickBehavior::Default
    );
    assert_eq!(
        pane.middle_click_behavior,
        TerminalMiddleClickBehavior::Default
    );
    assert_eq!(
        pane.alt_click_moves_cursor,
        DEFAULT_TERMINAL_ALT_CLICK_MOVES_CURSOR
    );
    assert_eq!(pane.copy_on_selection, DEFAULT_TERMINAL_COPY_ON_SELECTION);
    assert_eq!(
        pane.ignore_bracketed_paste_mode,
        DEFAULT_TERMINAL_IGNORE_BRACKETED_PASTE_MODE
    );
    assert_eq!(
        pane.multi_line_paste_warning,
        TerminalMultiLinePasteWarning::Auto
    );
    assert!(pane.pending_multiline_paste.is_none());
    assert_eq!(pane.word_separators, DEFAULT_TERMINAL_WORD_SEPARATORS);
    assert_eq!(
        pane.mouse_wheel_scroll_sensitivity,
        DEFAULT_TERMINAL_MOUSE_WHEEL_SCROLL_SENSITIVITY
    );
    assert_eq!(
        pane.fast_scroll_sensitivity,
        DEFAULT_TERMINAL_FAST_SCROLL_SENSITIVITY
    );
    assert_eq!(pane.mouse_wheel_zoom, DEFAULT_TERMINAL_MOUSE_WHEEL_ZOOM);
}

#[test]
fn terminal_uses_configured_scrollback_rows_for_new_sessions() {
    let size = test_terminal_size();
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        25_000,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );
    pane.last_size = size;

    pane.open_new_session();

    assert_eq!(pane.scrollback_rows, 25_000);
    assert_eq!(pane.sessions[0].scrollback_rows, 25_000);
}

#[test]
fn terminal_can_open_new_session_at_explicit_cwd() {
    let size = test_terminal_size();
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );
    pane.last_size = size;

    pane.open_new_session_at(PathBuf::from("workspace/repo"));

    assert!(pane.visible);
    assert_eq!(pane.sessions.len(), 1);
    assert_eq!(
        pane.sessions[0].initial_cwd.as_deref(),
        Some(PathBuf::from("workspace/repo").as_path())
    );
}

#[test]
fn terminal_new_session_exits_split_view() {
    let size = test_terminal_size();
    let first = session_without_command(1, size);
    let second = session_without_command(2, size);
    let mut pane = pane_with_sessions(vec![first, second], size);
    pane.split_view = true;
    pane.next_session_id = 3;

    pane.open_new_session_at(PathBuf::from("workspace/tools"));

    assert_eq!(pane.sessions.len(), 3);
    assert_eq!(pane.active_session, 2);
    assert!(!pane.split_view);
    assert_eq!(
        pane.sessions[2].initial_cwd.as_deref(),
        Some(PathBuf::from("workspace/tools").as_path())
    );
}

#[test]
fn terminal_new_sessions_stop_at_session_cap() {
    let size = test_terminal_size();
    let sessions = (0..super::TERMINAL_MAX_SESSIONS)
        .map(|index| session_without_command(index + 1, size))
        .collect::<Vec<_>>();
    let mut pane = pane_with_sessions(sessions, size);
    assert_eq!(pane.sessions.len(), super::TERMINAL_MAX_SESSIONS);
    assert!(!pane.can_open_session());

    pane.open_new_session();
    pane.split_active_session();

    assert_eq!(pane.sessions.len(), super::TERMINAL_MAX_SESSIONS);
}

#[test]
fn terminal_scrollback_rows_are_clamped() {
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        1,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );

    assert_eq!(pane.scrollback_rows, MIN_TERMINAL_SCROLLBACK_ROWS);

    pane.set_scrollback_rows(25_000);

    assert_eq!(pane.scrollback_rows, 25_000);
}

#[test]
fn terminal_lowering_scrollback_reclaims_existing_parser_history() {
    let size = PtySize {
        rows: 3,
        cols: 24,
        pixel_width: 0,
        pixel_height: 0,
    };
    let output = (0..150)
        .map(|index| format!("line-{index:03}\r\n"))
        .collect::<String>();
    let mut pane = pane_with_session(session_with_output(1, size, output.as_bytes()), size);

    pane.sessions[0]
        .parser
        .screen_mut()
        .set_scrollback(usize::MAX);
    assert!(pane.sessions[0].scrollback() > MIN_TERMINAL_SCROLLBACK_ROWS);

    pane.set_scrollback_rows(MIN_TERMINAL_SCROLLBACK_ROWS);

    assert_eq!(
        pane.sessions[0].scrollback_rows,
        MIN_TERMINAL_SCROLLBACK_ROWS
    );
    pane.sessions[0]
        .parser
        .screen_mut()
        .set_scrollback(usize::MAX);
    assert!(pane.sessions[0].scrollback() <= MIN_TERMINAL_SCROLLBACK_ROWS);
    assert!(
        !pane.sessions[0]
            .parser
            .screen()
            .contents()
            .contains("line-000")
    );

    pane.sessions[0].parser.screen_mut().set_scrollback(0);
    assert!(pane.sessions[0].copyable_text().contains("line-149"));
}

#[test]
fn terminal_lowering_scrollback_in_alternate_screen_does_not_replay_search_buffer() {
    let size = PtySize {
        rows: 3,
        cols: 32,
        pixel_width: 0,
        pixel_height: 0,
    };
    let output = (0..10)
        .map(|index| format!("line-{index:03}\r\n"))
        .collect::<String>();
    let mut pane = pane_with_session(session_with_output(1, size, output.as_bytes()), size);
    let alternate_output = b"\x1b[?1049hALT-SCREEN-ONLY\r\n";
    pane.sessions[0].append_search_output(alternate_output);
    pane.sessions[0].parser.process(alternate_output);
    assert!(pane.sessions[0].parser.screen().alternate_screen());

    pane.set_scrollback_rows(MIN_TERMINAL_SCROLLBACK_ROWS);
    pane.sessions[0].parser.process(b"\x1b[?1049l");
    pane.sessions[0]
        .parser
        .screen_mut()
        .set_scrollback(usize::MAX);

    let contents = pane.sessions[0].parser.screen().contents();
    assert!(!contents.contains("ALT-SCREEN-ONLY"));
}

#[test]
fn terminal_shell_profile_is_configurable_and_trimmed() {
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );

    pane.set_shell_profile(
        Some(" pwsh.exe ".to_owned()),
        vec![
            " -NoLogo ".to_owned(),
            "".to_owned(),
            "-NoProfile".to_owned(),
        ],
    );

    assert_eq!(pane.shell_path.as_deref(), Some("pwsh.exe"));
    assert_eq!(pane.shell_args, ["-NoLogo", "-NoProfile"]);

    pane.set_shell_profile(Some(" ".to_owned()), vec![" ".to_owned()]);

    assert_eq!(pane.shell_path, None);
    assert!(pane.shell_args.is_empty());
}

#[test]
fn terminal_shell_label_falls_back_for_unsafe_profile_path() {
    assert_eq!(
        terminal_shell_label(Some("pwsh.exe\n-NoProfile")),
        default_shell_label()
    );
    assert_eq!(
        terminal_shell_label(Some("pwsh.exe\u{7}")),
        default_shell_label()
    );
}

#[test]
fn terminal_start_directory_is_configurable_and_resolves_from_workspace() {
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );

    pane.set_terminal_cwd(Some(" tools ".to_owned()));

    assert_eq!(pane.terminal_cwd, Some(PathBuf::from("tools")));
    assert_eq!(pane.launch_cwd(), PathBuf::from("workspace").join("tools"));

    pane.set_terminal_cwd(Some(" ".to_owned()));

    assert_eq!(pane.terminal_cwd, None);
    assert_eq!(pane.launch_cwd(), PathBuf::from("workspace"));
}

#[test]
fn terminal_start_directory_rejects_control_characters() {
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );

    for cwd in ["tools\nbad", "tools\rbad", "tools\tbad", "tools\u{7}bad"] {
        pane.set_terminal_cwd(Some(cwd.to_owned()));

        assert_eq!(pane.terminal_cwd, None);
        assert_eq!(pane.launch_cwd(), PathBuf::from("workspace"));
    }
}

#[test]
fn terminal_split_start_directory_uses_configured_policy() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(session_without_command(1, size), size);
    pane.cwd = PathBuf::from("workspace");
    pane.set_terminal_cwd(Some("tools".to_owned()));
    pane.sessions[0].initial_cwd = Some(PathBuf::from("workspace").join("tools"));

    pane.set_split_cwd(TerminalSplitCwd::WorkspaceRoot);
    assert_eq!(pane.split_launch_cwd(Some(0)), PathBuf::from("workspace"));

    pane.set_split_cwd(TerminalSplitCwd::Initial);
    assert_eq!(
        pane.split_launch_cwd(Some(0)),
        PathBuf::from("workspace").join("tools")
    );

    pane.set_split_cwd(TerminalSplitCwd::Inherited);
    assert_eq!(
        pane.split_launch_cwd(Some(0)),
        PathBuf::from("workspace").join("tools")
    );
}

#[test]
fn terminal_minimum_size_is_configurable_and_clamped() {
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );

    pane.set_minimum_size(12, 60);

    assert_eq!(pane.min_rows, 12);
    assert_eq!(pane.min_columns, 60);
    assert_eq!(pane.last_size.rows, 24);
    assert_eq!(pane.last_size.cols, 100);

    pane.set_minimum_size(0, 1);

    assert_eq!(pane.min_rows, MIN_TERMINAL_MIN_ROWS);
    assert_eq!(pane.min_columns, MIN_TERMINAL_MIN_COLUMNS);
}

#[test]
fn terminal_font_metrics_are_configurable_and_clamped() {
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        15.0,
        1.5,
    );

    assert_eq!(pane.font_size, 15.0);
    assert_eq!(pane.line_height, 1.5);

    pane.set_font_metrics(1.0, 0.1, -1.0);

    assert_eq!(pane.font_size, MIN_TERMINAL_FONT_SIZE);
    assert_eq!(pane.line_height, MIN_TERMINAL_LINE_HEIGHT);
    assert_eq!(pane.letter_spacing, MIN_TERMINAL_LETTER_SPACING);
}

#[test]
fn terminal_font_metrics_affect_session_resize_columns() {
    let size = test_terminal_size();
    let (mut pane, rx_command) = pane_with_command_session(size);

    pane.resize_session_to_fit(0, 600.0, 180.0);
    let default_resize = recv_resize(&rx_command);

    pane.set_font_metrics(
        20.0,
        DEFAULT_TERMINAL_LINE_HEIGHT,
        DEFAULT_TERMINAL_LETTER_SPACING,
    );
    pane.resize_session_to_fit(0, 600.0, 180.0);
    let large_font_resize = recv_resize(&rx_command);

    assert!(large_font_resize.cols < default_resize.cols);
}

#[test]
fn terminal_letter_spacing_affects_session_resize_columns() {
    let size = test_terminal_size();
    let (mut pane, rx_command) = pane_with_command_session(size);

    pane.resize_session_to_fit(0, 600.0, 180.0);
    let default_resize = recv_resize(&rx_command);

    pane.set_font_metrics(
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
        4.0,
    );
    pane.resize_session_to_fit(0, 600.0, 180.0);
    let spaced_resize = recv_resize(&rx_command);

    assert!(spaced_resize.cols < default_resize.cols);
}

#[test]
fn terminal_resize_to_same_size_is_not_requeued() {
    let size = test_terminal_size();
    let (mut pane, rx_command) = pane_with_command_session(size);

    pane.resize_session_to_fit(0, 600.0, 180.0);
    let first_resize = recv_resize(&rx_command);

    pane.resize_session_to_fit(0, 600.0, 180.0);
    assert!(rx_command.try_recv().is_err());

    pane.resize_session_to_fit(0, 720.0, 180.0);
    let second_resize = recv_resize(&rx_command);
    assert_ne!(second_resize.cols, first_resize.cols);
}

#[test]
fn terminal_ui_resizes_pty_to_drawable_content_rect() {
    let size = test_terminal_size();
    let (mut pane, rx_command) = pane_with_command_session(size);
    let mut command_bus = kuroya_core::CommandBus::default();
    let ctx = egui::Context::default();
    let input = RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            vec2(800.0, 260.0),
        )),
        ..RawInput::default()
    };

    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                ui.set_min_size(vec2(800.0, 260.0));
                pane.ui(ui, &mut command_bus);
            });
    });

    let resize = recv_resize(&rx_command);
    let expected = terminal_size_from_points(
        780.0,
        202.0,
        pane.font_size,
        pane.line_height,
        pane.letter_spacing,
        pane.min_rows,
        pane.min_columns,
    );
    assert_eq!(resize.cols, expected.cols);
    assert_eq!(resize.rows, expected.rows);
}

#[test]
fn terminal_resize_retries_when_command_queue_was_full() {
    let size = test_terminal_size();
    let (tx_command, rx_command) = bounded(1);
    tx_command
        .send(TerminalCommand::Input("queued".to_owned()))
        .unwrap();
    let mut pane = pane_with_session(session_with_command(1, size, tx_command), size);

    pane.resize_session_to_fit(0, 600.0, 180.0);

    assert_eq!(
        pane.sessions[0].parser.screen().size(),
        (size.rows, size.cols)
    );
    match rx_command.try_recv().unwrap() {
        TerminalCommand::Input(input) => assert_eq!(input, "queued"),
        TerminalCommand::Resize(_) | TerminalCommand::Close => panic!("expected queued input"),
    }

    pane.resize_session_to_fit(0, 600.0, 180.0);

    let resize = recv_resize(&rx_command);
    assert_eq!(
        pane.sessions[0].parser.screen().size(),
        (resize.rows, resize.cols)
    );
}

#[test]
fn terminal_cursor_settings_are_configurable_and_clamped() {
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );

    pane.set_cursor_settings(
        TerminalCursorStyle::Line,
        0.1,
        true,
        TerminalInactiveCursorStyle::None,
    );

    assert_eq!(pane.cursor_style, TerminalCursorStyle::Line);
    assert_eq!(pane.cursor_width, MIN_TERMINAL_CURSOR_WIDTH);
    assert!(pane.cursor_blinking);
    assert_eq!(
        pane.cursor_style_inactive,
        TerminalInactiveCursorStyle::None
    );
}

#[test]
fn terminal_right_click_behavior_is_configurable() {
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );

    pane.set_right_click_behavior(TerminalRightClickBehavior::Paste);

    assert_eq!(pane.right_click_behavior, TerminalRightClickBehavior::Paste);
}

#[test]
fn terminal_middle_click_behavior_is_configurable() {
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );

    pane.set_middle_click_behavior(TerminalMiddleClickBehavior::Paste);

    assert_eq!(
        pane.middle_click_behavior,
        TerminalMiddleClickBehavior::Paste
    );
}

#[test]
fn terminal_alt_click_moves_cursor_setting_is_configurable() {
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );

    pane.set_alt_click_moves_cursor(false);

    assert!(!pane.alt_click_moves_cursor);
}

#[test]
fn terminal_copy_on_selection_setting_is_configurable() {
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );

    pane.set_copy_on_selection(false);

    assert!(!pane.copy_on_selection);
}

#[test]
fn terminal_ignore_bracketed_paste_mode_setting_is_configurable() {
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );

    pane.set_ignore_bracketed_paste_mode(true);

    assert!(pane.ignore_bracketed_paste_mode);
}

#[test]
fn terminal_word_separators_setting_is_configurable() {
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );

    pane.set_word_separators(":");

    assert_eq!(pane.word_separators, ":");
}

#[test]
fn terminal_scroll_sensitivity_settings_are_configurable_and_clamped() {
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );

    pane.set_scroll_sensitivity(2.0, -1.0);

    assert_eq!(pane.mouse_wheel_scroll_sensitivity, 2.0);
    assert_eq!(
        pane.fast_scroll_sensitivity,
        MIN_TERMINAL_SCROLL_SENSITIVITY
    );
}

#[test]
fn terminal_mouse_wheel_zoom_setting_is_configurable() {
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );

    pane.set_mouse_wheel_zoom(true);

    assert!(pane.mouse_wheel_zoom);
}

#[test]
fn terminal_font_zoom_uses_wheel_direction_and_clamps() {
    assert_eq!(
        terminal_zoomed_font_size(DEFAULT_TERMINAL_FONT_SIZE, 1.0),
        Some(DEFAULT_TERMINAL_FONT_SIZE + 1.0)
    );
    assert_eq!(
        terminal_zoomed_font_size(DEFAULT_TERMINAL_FONT_SIZE, -1.0),
        Some(DEFAULT_TERMINAL_FONT_SIZE - 1.0)
    );
    assert_eq!(terminal_zoomed_font_size(MAX_TERMINAL_FONT_SIZE, 1.0), None);
    assert_eq!(
        terminal_zoomed_font_size(MIN_TERMINAL_FONT_SIZE, -1.0),
        None
    );
    assert_eq!(
        terminal_zoomed_font_size(DEFAULT_TERMINAL_FONT_SIZE, 0.0),
        None
    );
}

#[test]
fn terminal_zoom_updates_runtime_font_size() {
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );

    assert!(pane.zoom_terminal_font(1.0));

    assert_eq!(pane.font_size, DEFAULT_TERMINAL_FONT_SIZE + 1.0);
}

#[test]
fn terminal_render_text_runs_merge_plain_adjacent_cells() {
    let color = egui::Color32::WHITE;
    let mut runs = Vec::new();

    push_terminal_text_run(&mut runs, 0, 0, 1, "a", color, false, true);
    push_terminal_text_run(&mut runs, 0, 1, 1, "b", color, false, true);
    push_terminal_text_run(&mut runs, 0, 2, 1, "c", color, false, true);

    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].row, 0);
    assert_eq!(runs[0].start_col, 0);
    assert_eq!(runs[0].width_cols, 3);
    assert_eq!(runs[0].text, "abc");
    assert_eq!(runs[0].color, color);
    assert!(!runs[0].underline);
}

#[test]
fn terminal_render_text_runs_split_on_style_or_gaps() {
    let white = egui::Color32::WHITE;
    let red = egui::Color32::RED;
    let mut runs = Vec::new();

    push_terminal_text_run(&mut runs, 0, 0, 1, "a", white, false, true);
    push_terminal_text_run(&mut runs, 0, 1, 1, "b", red, false, true);
    push_terminal_text_run(&mut runs, 0, 2, 1, "c", red, true, true);
    push_terminal_text_run(&mut runs, 0, 4, 1, "d", red, true, true);
    push_terminal_text_run(&mut runs, 1, 0, 1, "e", red, true, true);

    assert_eq!(runs.len(), 5);
    assert_eq!(runs[0].text, "a");
    assert_eq!(runs[1].text, "b");
    assert_eq!(runs[2].text, "c");
    assert_eq!(runs[3].text, "d");
    assert_eq!(runs[4].text, "e");
}

#[test]
fn terminal_render_text_runs_keep_wide_cells_separate_then_resume_merging() {
    let color = egui::Color32::WHITE;
    let mut runs = Vec::new();

    push_terminal_text_run(&mut runs, 0, 0, 2, "界", color, false, false);
    push_terminal_text_run(&mut runs, 0, 2, 1, "a", color, false, true);
    push_terminal_text_run(&mut runs, 0, 3, 1, "b", color, false, true);

    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].text, "界");
    assert_eq!(runs[0].width_cols, 2);
    assert_eq!(runs[1].text, "ab");
    assert_eq!(runs[1].start_col, 2);
    assert_eq!(runs[1].width_cols, 2);
}

#[test]
fn terminal_render_text_runs_ignore_empty_or_zero_width_cells() {
    let mut runs = Vec::new();

    push_terminal_text_run(&mut runs, 0, 0, 1, "", egui::Color32::WHITE, false, true);
    push_terminal_text_run(&mut runs, 0, 0, 0, "a", egui::Color32::WHITE, false, true);

    assert!(runs.is_empty());
}

#[test]
fn terminal_render_text_runs_keep_cells_separate_when_merging_is_disabled() {
    let color = egui::Color32::WHITE;
    let mut runs = Vec::new();

    push_terminal_text_run(&mut runs, 0, 0, 1, "a", color, false, false);
    push_terminal_text_run(&mut runs, 0, 1, 1, "b", color, false, false);

    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].text, "a");
    assert_eq!(runs[1].text, "b");
}

#[test]
fn terminal_text_run_merging_requires_grid_aligned_metrics() {
    assert!(terminal_text_runs_can_merge(8.68, 8.7, 0.0));
    assert!(!terminal_text_runs_can_merge(10.68, 8.68, 2.0));
    assert!(!terminal_text_runs_can_merge(8.68, 7.9, 0.0));
    assert!(!terminal_text_runs_can_merge(f32::NAN, 8.68, 0.0));
    assert!(!terminal_text_runs_can_merge(8.68, f32::NAN, 0.0));
}

#[test]
fn terminal_bell_setting_tracks_parser_bells() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(session_without_command(8, size), size);
    pane.set_bell_settings(true, 750);

    pane.sessions[0]
        .tx_output
        .send(TerminalEvent::Output(b"\x07".to_vec()))
        .unwrap();
    pane.drain_output();

    assert!(pane.last_bell_at.is_some());
    assert!(pane.enable_bell);
    assert_eq!(pane.bell_duration_ms, 750);
}

#[test]
fn terminal_split_widths_follow_dragged_separator() {
    let size = test_terminal_size();
    let first = session_without_command(1, size);
    let second = session_without_command(2, size);
    let mut pane = pane_with_sessions(vec![first, second], size);

    assert_eq!(pane.split_widths(600.0, 7.0), vec![296.5, 296.5]);

    pane.resize_split_at(0, 80.0);
    let widths = pane.split_widths(600.0, 7.0);

    assert!(widths[0] > widths[1]);
    assert!((widths.iter().sum::<f32>() - 593.0).abs() < 0.01);
}

#[test]
fn terminal_split_resize_preserves_minimum_pane_widths() {
    let size = test_terminal_size();
    let first = session_without_command(1, size);
    let second = session_without_command(2, size);
    let mut pane = pane_with_sessions(vec![first, second], size);
    pane.split_widths(600.0, 7.0);

    pane.resize_split_at(0, -500.0);
    let widths = pane.split_widths(600.0, 7.0);

    assert!(widths[0] >= 160.0);
    assert!(widths[1] >= 160.0);
}

#[test]
fn terminal_split_resize_updates_each_session_screen_and_pty_columns() {
    let size = test_terminal_size();
    let (first_tx, first_rx) = unbounded();
    let (second_tx, second_rx) = unbounded();
    let first = session_with_command(1, size, first_tx);
    let second = session_with_command(2, size, second_tx);
    let mut pane = pane_with_sessions(vec![first, second], size);

    pane.split_widths(600.0, 7.0);
    pane.resize_split_at(0, 80.0);
    let widths = pane.split_widths(600.0, 7.0);
    pane.resize_session_to_fit(0, widths[0], 180.0);
    pane.resize_session_to_fit(1, widths[1], 180.0);

    let left_cols = pane.sessions[0].parser.screen().size().1;
    let right_cols = pane.sessions[1].parser.screen().size().1;
    assert!(left_cols > right_cols);

    let left_resize = recv_resize(&first_rx);
    let right_resize = recv_resize(&second_rx);
    assert!(left_resize.cols > right_resize.cols);
}

#[test]
fn terminal_relative_session_navigation_wraps_and_focuses_input() {
    let size = test_terminal_size();
    let first = session_without_command(1, size);
    let second = session_without_command(2, size);
    let third = session_without_command(3, size);
    let mut pane = pane_with_sessions(vec![first, second, third], size);
    pane.active_session = 2;
    pane.visible = false;

    pane.activate_relative_session(1);

    assert!(pane.visible);
    assert_eq!(pane.active_session, 0);
    assert!(pane.focus_input_on_show);

    pane.activate_relative_session(-1);

    assert_eq!(pane.active_session, 2);
}

#[test]
fn terminal_relative_session_navigation_opens_initial_session_when_empty() {
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );

    pane.activate_relative_session(1);

    assert!(pane.visible);
    assert_eq!(pane.sessions.len(), 1);
    assert_eq!(pane.active_session, 0);
    assert!(pane.focus_input_on_show);
}

#[test]
fn terminal_sessions_can_scroll_vt_scrollback() {
    let size = PtySize {
        rows: 3,
        cols: 20,
        pixel_width: 0,
        pixel_height: 0,
    };
    let mut session = session_without_command(1, size);
    session
        .parser
        .process(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\n");

    assert_eq!(session.scrollback(), 0);

    session.scroll_scrollback(2);
    assert!(session.scrollback() > 0);

    session.scroll_scrollback(-100);
    assert_eq!(session.scrollback(), 0);
}

#[test]
fn terminal_context_copy_text_trims_blank_screen_rows() {
    let size = PtySize {
        rows: 4,
        cols: 20,
        pixel_width: 0,
        pixel_height: 0,
    };
    let pane = pane_with_session(session_with_output(1, size, b"hello\r\nworld"), size);

    assert_eq!(
        pane.copyable_text_for_session(0).as_deref(),
        Some("hello\nworld")
    );
}

#[test]
fn terminal_context_clear_resets_visible_buffer_without_stopping_session() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(session_with_output(1, size, b"hello\r\nworld"), size);

    pane.clear_session(0);

    assert!(pane.sessions[0].copyable_text().is_empty());
    assert!(pane.sessions[0].search_buffer.is_empty());
    assert!(pane.sessions[0].started);
}

#[test]
fn terminal_output_clear_sequence_erases_parser_and_search_scrollback() {
    let size = PtySize {
        rows: 3,
        cols: 32,
        pixel_width: 0,
        pixel_height: 0,
    };
    let output = (0..12)
        .map(|index| format!("old-line-{index:02}\r\n"))
        .collect::<String>();
    let mut pane = pane_with_session(session_with_output(1, size, output.as_bytes()), size);
    pane.sessions[0]
        .parser
        .screen_mut()
        .set_scrollback(usize::MAX);
    assert!(pane.sessions[0].scrollback() > 0);
    assert!(pane.sessions[0].search_buffer.contains("old-line-00"));

    pane.sessions[0]
        .tx_output
        .send(TerminalEvent::Output(
            b"\x1b[H\x1b[2J\x1b[3Jprompt> ".to_vec(),
        ))
        .expect("test output channel accepts clear sequence");
    pane.drain_output();
    pane.sessions[0]
        .parser
        .screen_mut()
        .set_scrollback(usize::MAX);

    assert_eq!(pane.sessions[0].scrollback(), 0);
    assert!(!pane.sessions[0].search_buffer.contains("old-line"));
    assert!(pane.sessions[0].search_buffer.contains("prompt>"));
    assert_eq!(pane.sessions[0].copyable_text(), "prompt>");
}

#[test]
fn terminal_output_clear_sequence_preserves_fresh_post_clear_scrollback() {
    let size = PtySize {
        rows: 3,
        cols: 32,
        pixel_width: 0,
        pixel_height: 0,
    };
    let output = (0..8)
        .map(|index| format!("old-line-{index:02}\r\n"))
        .collect::<String>();
    let mut pane = pane_with_session(session_with_output(1, size, output.as_bytes()), size);
    let fresh_output = (0..8)
        .map(|index| format!("fresh-line-{index:02}\r\n"))
        .collect::<String>();
    let clear_and_fresh = format!("\x1b[H\x1b[2J\x1b[3J{fresh_output}");

    pane.sessions[0]
        .tx_output
        .send(TerminalEvent::Output(clear_and_fresh.into_bytes()))
        .expect("test output channel accepts clear sequence");
    pane.drain_output();
    pane.sessions[0]
        .parser
        .screen_mut()
        .set_scrollback(usize::MAX);

    let contents = pane.sessions[0].parser.screen().contents();
    assert!(pane.sessions[0].scrollback() > 0);
    assert!(!contents.contains("old-line"));
    assert!(contents.contains("fresh-line-00"));
    assert!(!pane.sessions[0].search_buffer.contains("old-line"));
    assert!(pane.sessions[0].search_buffer.contains("fresh-line-00"));
}

#[test]
fn terminal_context_select_all_tracks_selected_session_id() {
    let size = test_terminal_size();
    let pane_session = session_without_command(3, size);
    let mut pane = pane_with_session(pane_session, size);

    pane.select_all_session(0);

    assert_eq!(pane.selected_session_id, Some(3));
}

#[test]
fn terminal_context_paste_request_targets_session() {
    let size = test_terminal_size();
    let pane_session = session_without_command(4, size);
    let mut pane = pane_with_session(pane_session, size);

    pane.request_paste_for_session(0);

    assert_eq!(pane.pending_paste_session_id, Some(4));
    assert_eq!(pane.active_session, 0);
    assert!(pane.focus_input_on_show);
}

#[test]
fn terminal_copyable_text_prefers_selected_word() {
    let size = PtySize {
        rows: 4,
        cols: 20,
        pixel_width: 0,
        pixel_height: 0,
    };
    let mut pane = pane_with_session(session_with_output(6, size, b"hello world"), size);
    pane.selected_text = Some(super::TerminalTextSelection {
        session_id: 6,
        text: "world".to_owned(),
        range: super::TerminalSelectionRange {
            start: super::TerminalCellPosition { row: 0, col: 6 },
            end: super::TerminalCellPosition { row: 0, col: 11 },
        },
    });

    assert_eq!(pane.copyable_text_for_session(0).as_deref(), Some("world"));
}

#[test]
fn terminal_word_selection_uses_configured_word_separators() {
    let size = PtySize {
        rows: 4,
        cols: 30,
        pixel_width: 0,
        pixel_height: 0,
    };
    let session = session_with_output(6, size, b"foo:bar baz");

    let default_selection =
        terminal_word_selection_at_cell(&session, 0, 1, DEFAULT_TERMINAL_WORD_SEPARATORS).unwrap();
    let colon_selection = terminal_word_selection_at_cell(&session, 0, 1, ":").unwrap();

    assert_eq!(default_selection.text, "foo:bar");
    assert_eq!(colon_selection.text, "foo");
}

#[test]
fn terminal_word_selection_ignores_separator_cells() {
    let size = PtySize {
        rows: 4,
        cols: 30,
        pixel_width: 0,
        pixel_height: 0,
    };
    let session = session_with_output(6, size, b"foo:bar baz");

    let selection =
        terminal_word_selection_at_cell(&session, 0, 7, DEFAULT_TERMINAL_WORD_SEPARATORS);

    assert!(selection.is_none());
}

#[test]
fn terminal_text_range_selection_copies_visible_text() {
    let size = PtySize {
        rows: 4,
        cols: 20,
        pixel_width: 0,
        pixel_height: 0,
    };
    let mut pane = pane_with_session(
        session_with_output(6, size, b"hello world\r\nnext line"),
        size,
    );

    let text = pane.select_text_range_for_session(
        0,
        super::TerminalCellPosition { row: 0, col: 6 },
        super::TerminalCellPosition { row: 1, col: 3 },
    );

    assert_eq!(text.as_deref(), Some("world\nnext"));
    assert!(pane.has_selection_for_session(0));
}

#[test]
fn terminal_input_returns_scrolled_session_to_bottom() {
    let size = PtySize {
        rows: 3,
        cols: 20,
        pixel_width: 0,
        pixel_height: 0,
    };
    let (mut pane, rx_command) = pane_with_command_session(size);
    pane.sessions[0]
        .parser
        .process(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\n");
    pane.sessions[0].scroll_scrollback(2);

    pane.send_input("pwd\r");

    assert_eq!(pane.sessions[0].scrollback(), 0);
    match rx_command.try_recv().unwrap() {
        TerminalCommand::Input(input) => assert_eq!(input, "pwd\r"),
        TerminalCommand::Resize(_) | TerminalCommand::Close => panic!("expected terminal input"),
    }
}

#[test]
fn terminal_mouse_wheel_sends_sgr_mouse_tracking_input() {
    let size = test_terminal_size();
    let (mut pane, rx_command) = pane_with_command_session(size);
    pane.sessions[0].parser.process(b"\x1b[?1000h\x1b[?1006h");

    assert!(pane.send_terminal_wheel_input(
        0,
        Some(super::TerminalCellPosition { row: 1, col: 2 }),
        2,
        Modifiers::SHIFT,
    ));

    match rx_command.try_recv().unwrap() {
        TerminalCommand::Input(input) => {
            assert_eq!(input, "\x1b[<68;3;2M\x1b[<68;3;2M");
        }
        TerminalCommand::Resize(_) | TerminalCommand::Close => panic!("expected mouse input"),
    }
}

#[test]
fn terminal_mouse_wheel_input_reports_unhandled_when_command_queue_is_full() {
    let size = test_terminal_size();
    let (tx_command, rx_command) = bounded(1);
    tx_command
        .send(TerminalCommand::Input("queued".to_owned()))
        .unwrap();
    let mut pane = pane_with_session(session_with_command(1, size, tx_command), size);
    pane.sessions[0].parser.process(b"\x1b[?1000h\x1b[?1006h");

    assert!(!pane.send_terminal_wheel_input(
        0,
        Some(super::TerminalCellPosition { row: 1, col: 2 }),
        1,
        Modifiers::NONE,
    ));
    assert!(pane.sessions[0].started);
    assert!(pane.sessions[0].tx_command.is_some());
    match rx_command.try_recv().unwrap() {
        TerminalCommand::Input(input) => assert_eq!(input, "queued"),
        TerminalCommand::Resize(_) | TerminalCommand::Close => panic!("expected queued input"),
    }
    assert!(rx_command.try_recv().is_err());
}

#[test]
fn terminal_mouse_wheel_input_reports_unhandled_and_stops_on_disconnect() {
    let size = test_terminal_size();
    let (tx_command, rx_command) = bounded(1);
    drop(rx_command);
    let mut pane = pane_with_session(session_with_command(1, size, tx_command), size);
    pane.sessions[0].parser.process(b"\x1b[?1000h\x1b[?1006h");

    assert!(!pane.send_terminal_wheel_input(
        0,
        Some(super::TerminalCellPosition { row: 1, col: 2 }),
        1,
        Modifiers::NONE,
    ));
    assert!(!pane.sessions[0].started);
    assert!(pane.sessions[0].tx_command.is_none());
}

#[test]
fn terminal_mouse_wheel_sends_cursor_keys_on_alternate_screen_without_mouse_tracking() {
    let size = test_terminal_size();
    let (mut pane, rx_command) = pane_with_command_session(size);
    pane.sessions[0].parser.process(b"\x1b[?1049h");

    assert!(pane.send_terminal_wheel_input(
        0,
        Some(super::TerminalCellPosition { row: 0, col: 0 }),
        -3,
        Modifiers::NONE,
    ));

    match rx_command.try_recv().unwrap() {
        TerminalCommand::Input(input) => {
            assert_eq!(input, "\x1b[B\x1b[B\x1b[B");
        }
        TerminalCommand::Resize(_) | TerminalCommand::Close => {
            panic!("expected alternate-screen scroll input");
        }
    }
}

#[test]
fn terminal_fullscreen_toggles_and_focuses_input() {
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );

    pane.toggle_fullscreen();

    assert!(pane.is_fullscreen());
    assert!(pane.focus_input_on_show);

    pane.set_visible(false);

    assert!(!pane.is_fullscreen());
}

fn recv_resize(rx: &Receiver<TerminalCommand>) -> PtySize {
    match rx.try_recv().unwrap() {
        TerminalCommand::Resize(size) => size,
        TerminalCommand::Input(_) | TerminalCommand::Close => panic!("expected resize command"),
    }
}

fn pane_with_command_session(size: PtySize) -> (TerminalPane, Receiver<TerminalCommand>) {
    let (tx_command, rx_command) = unbounded();
    (
        pane_with_session(session_with_command(1, size, tx_command), size),
        rx_command,
    )
}

fn pane_with_session(session: TerminalSession, size: PtySize) -> TerminalPane {
    pane_with_sessions(vec![session], size)
}

fn pane_with_sessions(sessions: Vec<TerminalSession>, size: PtySize) -> TerminalPane {
    let split_weights = vec![1.0; sessions.len()];
    TerminalPane {
        visible: true,
        cwd: PathBuf::from("workspace"),
        terminal_cwd: None,
        split_cwd: TerminalSplitCwd::default(),
        active_session: 0,
        next_session_id: sessions.len() + 1,
        sessions,
        last_size: size,
        focus_input_on_show: false,
        fullscreen: false,
        split_view: false,
        split_weights,
        search_open: false,
        search_query: String::new(),
        search_match: 0,
        search_focus_on_show: false,
        search_cache: Default::default(),
        pending_paste_session_id: None,
        pending_kill_session_id: None,
        pending_rename_session_id: None,
        rename_session_input: String::new(),
        selected_session_id: None,
        selected_text: None,
        selection_drag: None,
        last_bell_at: None,
        scrollback_rows: TERMINAL_SCROLLBACK_ROWS,
        shell_path: None,
        shell_label: default_shell_label(),
        shell_args: Vec::new(),
        min_rows: DEFAULT_TERMINAL_MIN_ROWS,
        min_columns: DEFAULT_TERMINAL_MIN_COLUMNS,
        font_size: DEFAULT_TERMINAL_FONT_SIZE,
        line_height: DEFAULT_TERMINAL_LINE_HEIGHT,
        letter_spacing: DEFAULT_TERMINAL_LETTER_SPACING,
        cursor_style: TerminalCursorStyle::Block,
        cursor_width: DEFAULT_TERMINAL_CURSOR_WIDTH,
        cursor_blinking: false,
        cursor_style_inactive: TerminalInactiveCursorStyle::Outline,
        draw_bold_text_in_bright_colors: true,
        minimum_contrast_ratio: DEFAULT_TERMINAL_MINIMUM_CONTRAST_RATIO,
        enable_bell: DEFAULT_TERMINAL_ENABLE_BELL,
        bell_duration_ms: DEFAULT_TERMINAL_BELL_DURATION_MS,
        show_exit_alert: DEFAULT_TERMINAL_SHOW_EXIT_ALERT,
        hide_on_last_closed: DEFAULT_TERMINAL_HIDE_ON_LAST_CLOSED,
        confirm_on_kill: TerminalConfirmOnKill::default(),
        tabs_enabled: DEFAULT_TERMINAL_TABS_ENABLED,
        tabs_default_icon: DEFAULT_TERMINAL_TABS_DEFAULT_ICON.to_owned(),
        tabs_default_color: None,
        tabs_allow_agent_cli_title: DEFAULT_TERMINAL_TABS_ALLOW_AGENT_CLI_TITLE,
        tabs_title_template: DEFAULT_TERMINAL_TABS_TITLE.to_owned(),
        tabs_hide_condition: TerminalTabsHideCondition::default(),
        tabs_show_active_terminal: TerminalTabsShowActiveTerminal::default(),
        tabs_show_actions: TerminalTabsShowActions::default(),
        tabs_focus_mode: TerminalTabsFocusMode::default(),
        tabs_location: TerminalTabsLocation::default(),
        right_click_behavior: TerminalRightClickBehavior::Default,
        middle_click_behavior: TerminalMiddleClickBehavior::Default,
        alt_click_moves_cursor: DEFAULT_TERMINAL_ALT_CLICK_MOVES_CURSOR,
        copy_on_selection: DEFAULT_TERMINAL_COPY_ON_SELECTION,
        ignore_bracketed_paste_mode: DEFAULT_TERMINAL_IGNORE_BRACKETED_PASTE_MODE,
        multi_line_paste_warning: TerminalMultiLinePasteWarning::Auto,
        pending_multiline_paste: None,
        paste_notice: None,
        word_separators: DEFAULT_TERMINAL_WORD_SEPARATORS.to_owned(),
        mouse_wheel_scroll_sensitivity: DEFAULT_TERMINAL_MOUSE_WHEEL_SCROLL_SENSITIVITY,
        fast_scroll_sensitivity: DEFAULT_TERMINAL_FAST_SCROLL_SENSITIVITY,
        mouse_wheel_zoom: DEFAULT_TERMINAL_MOUSE_WHEEL_ZOOM,
        repaint_context: None,
    }
}

fn session_without_command(id: usize, size: PtySize) -> TerminalSession {
    let (tx_output, rx_output) = unbounded();
    TerminalSession {
        id,
        parser: vt100::Parser::new_with_callbacks(
            size.rows,
            size.cols,
            TERMINAL_SCROLLBACK_ROWS,
            TerminalCallbacks::default(),
        ),
        tx_command: None,
        tx_close: None,
        close_requested: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        rx_output,
        tx_output,
        started: true,
        auto_start_shell: true,
        initial_cwd: None,
        custom_title: None,
        process_label: None,
        last_process_exit_code: None,
        last_process_terminal_error: false,
        scrollback_rows: TERMINAL_SCROLLBACK_ROWS,
        search_buffer: String::new(),
        search_line_count: 0,
        search_generation: 0,
        search_edit_generation: 0,
        search_pending_carriage_return: false,
        search_ansi_state: Default::default(),
        search_utf8_tail: Vec::new(),
    }
}

fn session_with_output(id: usize, size: PtySize, output: &[u8]) -> TerminalSession {
    let mut session = session_without_command(id, size);
    session.append_search_output(output);
    session.parser.process(output);
    session
}

fn session_with_command(
    id: usize,
    size: PtySize,
    tx_command: Sender<TerminalCommand>,
) -> TerminalSession {
    TerminalSession {
        tx_command: Some(tx_command),
        ..session_without_command(id, size)
    }
}

fn temp_terminal_root(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "kuroya-terminal-{label}-{}-{nanos}",
        std::process::id()
    ))
}
