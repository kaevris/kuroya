use super::*;
use std::path::PathBuf;

fn pane_for_cache_tests() -> TerminalPane {
    TerminalPane::new(PathBuf::from("workspace"), 100, 12.0, 1.2)
}

fn scroll_process_session_back(pane: &mut TerminalPane, index: usize) -> usize {
    pane.sessions[index].parser.screen_mut().set_size(2, 20);
    pane.sessions[index]
        .parser
        .process(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\n");
    pane.sessions[index].scroll_scrollback(2);
    pane.sessions[index].scrollback()
}

#[test]
fn terminal_visibility_cache_tracks_setting_changes() {
    let mut pane = pane_for_cache_tests();
    let _first = pane.add_process_session_for_test(1);
    let _second = pane.add_process_session_for_test(2);

    assert!(pane.terminal_session_tabs_visible());
    assert_eq!(pane.terminal_tabs_rail_location(), None);

    pane.set_tabs_enabled(false);
    assert!(!pane.terminal_session_tabs_visible());
    assert!(pane.terminal_active_session_dropdown_visible());
    assert_eq!(pane.terminal_tabs_rail_location(), None);

    pane.set_tabs_enabled(true);
    pane.set_tabs_location(TerminalTabsLocation::Right);
    assert_eq!(
        pane.terminal_tabs_rail_location(),
        Some(TerminalTabsLocation::Right)
    );

    pane.set_tabs_hide_condition(TerminalTabsHideCondition::SingleGroup);
    pane.split_view = true;
    assert!(!pane.terminal_session_tabs_visible());
    assert!(pane.terminal_active_info_visible());
}

#[test]
fn terminal_session_index_cache_rechecks_cached_slot() {
    let mut pane = pane_for_cache_tests();
    let _first = pane.add_process_session_for_test(10);
    let _second = pane.add_process_session_for_test(20);

    assert_eq!(
        pane.process_session_state_by_id(20),
        Some(super::super::TerminalProcessSessionState::Running)
    );

    pane.sessions.swap(0, 1);

    assert_eq!(
        pane.process_session_state_by_id(20),
        Some(super::super::TerminalProcessSessionState::Running)
    );
}

#[test]
fn shell_profile_change_restart_targets_started_shell_sessions_only() {
    let mut pane = pane_for_cache_tests();
    let mut shell = super::super::TerminalSession::new(1, pane.last_size, pane.scrollback_rows);
    shell.started = true;
    shell.initial_cwd = Some(PathBuf::from("workspace/tools"));
    shell.custom_title = Some("Tools".to_owned());
    pane.sessions.push(shell);

    assert_eq!(
        pane.profile_change_restart_session_state(0),
        Some((
            1,
            Some(PathBuf::from("workspace/tools")),
            Some("Tools".to_owned())
        ))
    );

    let _process_rx = pane.add_process_session_for_test(2);

    assert_eq!(pane.profile_change_restart_session_state(1), None);
}

#[test]
fn split_widths_sanitize_invalid_weights_without_poisoning_layout() {
    let mut pane = pane_for_cache_tests();
    let _first = pane.add_process_session_for_test(1);
    let _second = pane.add_process_session_for_test(2);
    pane.split_weights = vec![f32::NAN, 1.0];

    let widths = pane.split_widths(600.0, 7.0);

    assert_eq!(widths.len(), 2);
    assert!(widths.iter().all(|width| width.is_finite()));
    assert!(widths.iter().all(|width| *width >= split_min_width()));
    assert!(widths[1] > widths[0]);
    assert!((widths.iter().sum::<f32>() - 593.0).abs() < 0.01);
}

#[test]
fn split_widths_recover_after_non_finite_available_width() {
    let mut pane = pane_for_cache_tests();
    let _first = pane.add_process_session_for_test(1);
    let _second = pane.add_process_session_for_test(2);

    assert_eq!(pane.split_widths(f32::NAN, 7.0), vec![0.0, 0.0]);

    let widths = pane.split_widths(600.0, 7.0);
    assert_eq!(widths, vec![296.5, 296.5]);
}

#[test]
fn resize_split_ignores_non_finite_drag_delta() {
    let mut pane = pane_for_cache_tests();
    let _first = pane.add_process_session_for_test(1);
    let _second = pane.add_process_session_for_test(2);
    let before = pane.split_widths(600.0, 7.0);

    pane.resize_split_at(0, f32::NAN);

    assert_eq!(pane.split_widths(600.0, 7.0), before);
}

#[test]
fn pending_paste_targets_original_session_after_reorder() {
    let mut pane = pane_for_cache_tests();
    let first_rx = pane.add_process_session_for_test(1);
    let second_rx = pane.add_process_session_for_test(2);

    pane.request_paste_for_session(0);
    pane.sessions.swap(0, 1);
    pane.paste_text(0, "echo one".to_owned());

    match first_rx
        .try_recv()
        .expect("paste should target first session")
    {
        TerminalCommand::Input(input) => assert_eq!(input, "echo one"),
        TerminalCommand::Resize(_) | TerminalCommand::Close => panic!("expected paste input"),
    }
    assert!(second_rx.try_recv().is_err());
    assert_eq!(pane.pending_paste_session_id, None);
    assert_eq!(pane.active_session, 1);
}

#[test]
fn pending_paste_does_not_fall_back_to_stale_index() {
    let mut pane = pane_for_cache_tests();
    let first_rx = pane.add_process_session_for_test(1);
    let second_rx = pane.add_process_session_for_test(2);

    pane.request_paste_for_session(0);
    assert!(pane.close_session_by_id(1));
    pane.paste_text(0, "echo stale\r".to_owned());

    match first_rx
        .try_recv()
        .expect("closed session should receive close")
    {
        TerminalCommand::Close => {}
        TerminalCommand::Input(_) | TerminalCommand::Resize(_) => panic!("expected close"),
    }
    assert!(second_rx.try_recv().is_err());
    assert_eq!(pane.pending_paste_session_id, None);
}

#[test]
fn invalid_paste_request_clears_previous_pending_target() {
    let mut pane = pane_for_cache_tests();
    let first_rx = pane.add_process_session_for_test(1);
    let second_rx = pane.add_process_session_for_test(2);

    pane.request_paste_for_session(0);
    pane.request_paste_for_session(99);
    pane.paste_text(1, "echo second".to_owned());

    assert!(first_rx.try_recv().is_err());
    match second_rx
        .try_recv()
        .expect("paste should target explicit valid index")
    {
        TerminalCommand::Input(input) => assert_eq!(input, "echo second"),
        TerminalCommand::Resize(_) | TerminalCommand::Close => panic!("expected paste input"),
    }
    assert_eq!(pane.pending_paste_session_id, None);
}

#[test]
fn rename_terminal_updates_and_clears_custom_title() {
    let mut pane = pane_for_cache_tests();
    let _rx = pane.add_process_session_for_test(1);

    pane.begin_rename_session(0);
    assert_eq!(pane.pending_rename_session_id(), Some(1));
    assert_eq!(pane.rename_session_input, "test process");

    pane.rename_session_input = "  Build\r\n\u{202e}Main  ".to_owned();
    pane.submit_pending_rename();

    assert_eq!(pane.pending_rename_session_id(), None);
    assert_eq!(pane.rename_session_input, "");
    assert_eq!(pane.sessions[0].custom_title.as_deref(), Some("Build Main"));

    pane.begin_rename_session(0);
    pane.rename_session_input = "   ".to_owned();
    pane.submit_pending_rename();

    assert_eq!(pane.sessions[0].custom_title, None);
}

#[test]
fn paste_action_target_reuses_session_id_after_reorder() {
    let mut pane = pane_for_cache_tests();
    let first_rx = pane.add_process_session_for_test(1);
    let second_rx = pane.add_process_session_for_test(2);
    let target = pane.session_action_target(0).expect("first session target");

    pane.sessions.swap(0, 1);
    pane.send_paste_input_to_target(target, "echo first");

    match first_rx
        .try_recv()
        .expect("paste should follow target session id")
    {
        TerminalCommand::Input(input) => assert_eq!(input, "echo first"),
        TerminalCommand::Resize(_) | TerminalCommand::Close => panic!("expected paste input"),
    }
    assert!(second_rx.try_recv().is_err());
}

#[test]
fn paste_action_target_ignores_removed_session_id_without_clearing_selection() {
    let mut pane = pane_for_cache_tests();
    let first_rx = pane.add_process_session_for_test(1);
    let second_rx = pane.add_process_session_for_test(2);
    let target = pane.session_action_target(0).expect("first session target");
    pane.selected_session_id = Some(2);
    pane.selected_text = Some(super::super::TerminalTextSelection {
        session_id: 2,
        text: "selected".to_owned(),
        range: super::super::TerminalSelectionRange {
            start: super::super::TerminalCellPosition { row: 0, col: 0 },
            end: super::super::TerminalCellPosition { row: 0, col: 8 },
        },
    });

    assert!(pane.close_session_by_id(1));
    pane.send_paste_input_to_target(target, "echo stale");

    match first_rx
        .try_recv()
        .expect("closed session should receive close")
    {
        TerminalCommand::Close => {}
        TerminalCommand::Input(_) | TerminalCommand::Resize(_) => panic!("expected close"),
    }
    assert!(second_rx.try_recv().is_err());
    assert_eq!(pane.selected_session_id, Some(2));
    assert!(pane.selected_text.is_some());
}

#[test]
fn direct_input_is_bounded_at_utf8_boundary() {
    let mut pane = pane_for_cache_tests();
    let rx = pane.add_process_session_for_test(1);
    let input = format!("{}\u{e9}", "a".repeat(TERMINAL_INPUT_MAX_BYTES));

    pane.send_input(input);

    match rx.try_recv().expect("bounded input should be sent") {
        TerminalCommand::Input(input) => {
            assert_eq!(input.len(), TERMINAL_INPUT_MAX_BYTES);
            assert!(input.is_char_boundary(input.len()));
            assert!(input.ends_with('a'));
            assert!(!input.contains('\u{e9}'));
        }
        TerminalCommand::Resize(_) | TerminalCommand::Close => panic!("expected raw input"),
    }
}

#[test]
fn paste_input_is_bounded_without_truncating_bracket_markers() {
    let input = terminal_paste_input(
        format!("{}\u{e9}", "a".repeat(TERMINAL_INPUT_MAX_BYTES)),
        true,
        false,
    );

    assert!(input.starts_with(TERMINAL_BRACKETED_PASTE_PREFIX));
    assert!(input.ends_with(TERMINAL_BRACKETED_PASTE_SUFFIX));
    assert_eq!(input.len(), TERMINAL_INPUT_MAX_BYTES);

    let payload = &input[TERMINAL_BRACKETED_PASTE_PREFIX.len()
        ..input.len() - TERMINAL_BRACKETED_PASTE_SUFFIX.len()];
    assert_eq!(
        payload.len(),
        TERMINAL_INPUT_MAX_BYTES - TERMINAL_BRACKETED_PASTE_WRAPPER_BYTES
    );
    assert!(payload.ends_with('a'));
    assert!(!payload.contains('\u{e9}'));
}

#[test]
fn pending_multiline_paste_stores_bounded_text() {
    let mut pane = pane_for_cache_tests();
    let rx = pane.add_process_session_for_test(1);
    let text = format!("a\n{}\u{e9}", "b".repeat(TERMINAL_INPUT_MAX_BYTES));

    pane.paste_text(0, text);

    let pending = pane
        .pending_multiline_paste
        .as_ref()
        .expect("multiline paste should wait for confirmation");
    assert_eq!(pending.text.len(), TERMINAL_INPUT_MAX_BYTES);
    assert!(pending.text.is_char_boundary(pending.text.len()));
    assert!(pending.text.starts_with("a\n"));
    assert!(!pending.text.contains('\u{e9}'));
    assert_eq!(pane.pending_multiline_paste_line_count(), Some(2));
    assert!(rx.try_recv().is_err());
}

#[test]
fn pending_multiline_paste_line_count_ignores_stale_session_id() {
    let mut pane = pane_for_cache_tests();
    let rx = pane.add_process_session_for_test(1);
    pane.pending_multiline_paste = Some(super::super::TerminalPendingPaste {
        session_id: 99,
        text: "one\ntwo".to_owned(),
    });

    assert_eq!(pane.pending_multiline_paste_line_count(), None);

    pane.confirm_pending_multiline_paste();

    assert!(pane.pending_multiline_paste.is_none());
    assert!(rx.try_recv().is_err());
}

#[test]
fn pending_multiline_paste_targets_original_session_after_reorder() {
    let mut pane = pane_for_cache_tests();
    let first_rx = pane.add_process_session_for_test(1);
    let second_rx = pane.add_process_session_for_test(2);

    pane.paste_text(0, "one\ntwo".to_owned());
    assert_eq!(
        pane.pending_multiline_paste
            .as_ref()
            .map(|pending| pending.session_id),
        Some(1)
    );

    pane.sessions.swap(0, 1);
    assert_eq!(pane.pending_multiline_paste_line_count(), Some(2));
    pane.confirm_pending_multiline_paste();

    match first_rx
        .try_recv()
        .expect("pending paste should follow target session id")
    {
        TerminalCommand::Input(input) => assert_eq!(input, "one\ntwo"),
        TerminalCommand::Resize(_) | TerminalCommand::Close => panic!("expected paste input"),
    }
    assert!(second_rx.try_recv().is_err());
}

#[test]
fn paste_input_preserves_scrollback_when_command_queue_is_full() {
    let mut pane = pane_for_cache_tests();
    let _rx = pane.add_process_session_for_test(1);
    let before = scroll_process_session_back(&mut pane, 0);
    assert!(before > 0);

    let (tx, rx) = crossbeam_channel::bounded(1);
    tx.try_send(TerminalCommand::Input("occupied".to_owned()))
        .unwrap();
    pane.sessions[0].tx_command = Some(tx);

    pane.send_paste_input(0, "dropped");

    assert_eq!(pane.sessions[0].scrollback(), before);
    assert!(pane.sessions[0].started);
    assert!(pane.sessions[0].tx_command.is_some());
    match rx.try_recv().expect("existing queue item should remain") {
        TerminalCommand::Input(input) => assert_eq!(input, "occupied"),
        TerminalCommand::Resize(_) | TerminalCommand::Close => panic!("expected input"),
    }
    assert!(rx.try_recv().is_err());
}

#[test]
fn paste_input_preserves_scrollback_when_command_queue_disconnects() {
    let mut pane = pane_for_cache_tests();
    let _rx = pane.add_process_session_for_test(1);
    let before = scroll_process_session_back(&mut pane, 0);
    assert!(before > 0);

    let (tx, rx) = crossbeam_channel::bounded(1);
    drop(rx);
    pane.sessions[0].tx_command = Some(tx);

    pane.send_paste_input(0, "lost");

    assert_eq!(pane.sessions[0].scrollback(), before);
    assert!(!pane.sessions[0].started);
    assert!(pane.sessions[0].tx_command.is_none());
}

#[test]
fn alt_click_cursor_input_is_bounded_for_extreme_offsets() {
    let input = terminal_alt_click_cursor_input(
        super::super::TerminalCellPosition { row: 0, col: 0 },
        super::super::TerminalCellPosition {
            row: u16::MAX,
            col: u16::MAX,
        },
        u16::MAX,
    )
    .expect("bounded cursor input");

    assert_eq!(input, "\x1b[C".repeat(TERMINAL_CURSOR_INPUT_REPEAT_LIMIT));
    assert!(input.len() <= TERMINAL_INPUT_MAX_BYTES);
}

#[test]
fn mouse_wheel_input_rejects_positions_outside_screen() {
    let mut pane = pane_for_cache_tests();
    let rx = pane.add_process_session_for_test(1);
    pane.sessions[0].parser.process(b"\x1b[?1000h\x1b[?1006h");

    assert!(!pane.send_terminal_wheel_input(
        0,
        Some(super::super::TerminalCellPosition {
            row: u16::MAX,
            col: u16::MAX,
        }),
        1,
        Modifiers::NONE,
    ));
    assert!(rx.try_recv().is_err());
}

#[test]
fn sgr_mouse_input_preserves_protocol_format() {
    assert_eq!(terminal_sgr_mouse_input(64, 1, 1), "\x1b[<64;1;1M");
    assert_eq!(terminal_sgr_mouse_input(92, 80, 24), "\x1b[<92;80;24M");
    assert_eq!(
        terminal_sgr_mouse_input(u16::MAX, u32::MAX, u32::MAX),
        "\x1b[<65535;4294967295;4294967295M"
    );
}

#[test]
fn pending_close_targets_original_session_after_reorder() {
    let mut pane = pane_for_cache_tests();
    let first_rx = pane.add_process_session_for_test(1);
    let second_rx = pane.add_process_session_for_test(2);
    pane.active_session = 0;
    pane.set_confirm_on_kill(TerminalConfirmOnKill::Panel);

    pane.request_close_active_session();
    pane.sessions.swap(0, 1);
    pane.confirm_pending_kill();

    assert_eq!(
        pane.sessions
            .iter()
            .map(|session| session.id)
            .collect::<Vec<_>>(),
        vec![2]
    );
    match first_rx.try_recv().expect("first session should close") {
        TerminalCommand::Close => {}
        TerminalCommand::Input(_) | TerminalCommand::Resize(_) => panic!("expected close"),
    }
    assert!(second_rx.try_recv().is_err());
    assert_eq!(pane.pending_kill_session_id, None);
}

#[test]
fn pending_close_ignores_stopped_session() {
    let mut pane = pane_for_cache_tests();
    let first_rx = pane.add_process_session_for_test(1);
    pane.active_session = 0;
    pane.set_confirm_on_kill(TerminalConfirmOnKill::Panel);

    pane.request_close_active_session();
    pane.sessions[0].started = false;
    pane.confirm_pending_kill();

    assert_eq!(pane.sessions.len(), 1);
    assert_eq!(pane.sessions[0].id, 1);
    assert_eq!(pane.pending_kill_session_id, None);
    assert!(first_rx.try_recv().is_err());
}

#[test]
fn copy_selection_action_target_reuses_session_id_after_reorder() {
    let mut pane = pane_for_cache_tests();
    let _first_rx = pane.add_process_session_for_test(1);
    let _second_rx = pane.add_process_session_for_test(2);
    pane.sessions[0].parser.process(b"first\r\n");
    pane.sessions[1].parser.process(b"second\r\n");
    let target = pane.session_action_target(0).expect("first session target");

    pane.select_all_session(0);
    pane.sessions.swap(0, 1);

    assert!(pane.has_selection_for_session_target(target));
    assert_eq!(
        pane.copyable_text_for_session_target(target).as_deref(),
        Some("first")
    );
    assert_eq!(pane.copyable_text_for_session(0).as_deref(), Some("second"));
}

#[test]
fn raw_session_input_attempt_preserves_selection_state() {
    let mut pane = pane_for_cache_tests();
    let _rx = pane.add_process_session_for_test(1);
    pane.sessions[0].tx_command = None;
    pane.selected_session_id = Some(1);
    pane.selected_text = Some(super::super::TerminalTextSelection {
        session_id: 1,
        text: "selected".to_owned(),
        range: super::super::TerminalSelectionRange {
            start: super::super::TerminalCellPosition { row: 0, col: 0 },
            end: super::super::TerminalCellPosition { row: 0, col: 8 },
        },
    });

    pane.send_input("ignored\r");

    assert_eq!(pane.selected_session_id, Some(1));
    assert!(pane.selected_text.is_some());
}

#[test]
fn raw_session_input_ignores_stopped_session_with_stale_sender() {
    let mut pane = pane_for_cache_tests();
    let rx = pane.add_process_session_for_test(1);
    pane.sessions[0].started = false;
    pane.selected_session_id = Some(1);
    pane.selected_text = Some(super::super::TerminalTextSelection {
        session_id: 1,
        text: "selected".to_owned(),
        range: super::super::TerminalSelectionRange {
            start: super::super::TerminalCellPosition { row: 0, col: 0 },
            end: super::super::TerminalCellPosition { row: 0, col: 8 },
        },
    });

    pane.send_input("ignored\r");

    assert!(rx.try_recv().is_err());
    assert_eq!(pane.selected_session_id, Some(1));
    assert!(pane.selected_text.is_some());
}

#[test]
fn paste_request_ignores_stopped_process_with_stale_sender() {
    let mut pane = pane_for_cache_tests();
    let rx = pane.add_process_session_for_test(1);
    pane.sessions[0].started = false;
    pane.sessions[0].auto_start_shell = false;

    pane.request_paste_for_session(0);
    pane.paste_text(0, "ignored".to_owned());

    assert_eq!(pane.pending_paste_session_id, None);
    assert!(pane.pending_multiline_paste.is_none());
    assert!(rx.try_recv().is_err());
}

#[test]
fn resize_ignores_stopped_session_with_stale_sender() {
    let mut pane = pane_for_cache_tests();
    let rx = pane.add_process_session_for_test(1);
    pane.sessions[0].started = false;

    pane.resize_session_to_fit(0, 600.0, 180.0);

    assert!(rx.try_recv().is_err());
}

#[test]
fn mouse_wheel_input_ignores_stopped_session_with_stale_sender() {
    let mut pane = pane_for_cache_tests();
    let rx = pane.add_process_session_for_test(1);
    pane.sessions[0].parser.process(b"\x1b[?1000h\x1b[?1006h");
    pane.sessions[0].started = false;

    assert!(!pane.send_terminal_wheel_input(
        0,
        Some(super::super::TerminalCellPosition { row: 1, col: 2 }),
        1,
        Modifiers::NONE,
    ));
    assert!(rx.try_recv().is_err());
}

#[test]
fn terminal_response_flush_preserves_blocked_response_when_command_queue_is_full() {
    let mut pane = pane_for_cache_tests();
    let _rx = pane.add_process_session_for_test(1);
    let (tx, rx) = crossbeam_channel::bounded(1);
    pane.sessions[0].tx_command = Some(tx);
    pane.sessions[0]
        .parser
        .callbacks_mut()
        .pending_inputs
        .extend(["\x1b[1;1R".to_owned(), "\x1b[2;2R".to_owned()]);

    pane.sessions[0]
        .tx_command
        .as_ref()
        .unwrap()
        .try_send(TerminalCommand::Input("occupied".to_owned()))
        .unwrap();

    pane.sessions[0].flush_terminal_responses();

    assert_eq!(
        pane.sessions[0].parser.callbacks().pending_inputs,
        ["\x1b[1;1R".to_owned(), "\x1b[2;2R".to_owned()]
    );
    match rx.try_recv().expect("existing queue item should remain") {
        TerminalCommand::Input(input) => assert_eq!(input, "occupied"),
        TerminalCommand::Resize(_) | TerminalCommand::Close => panic!("expected input"),
    }
}

#[test]
fn terminal_response_flush_preserves_scrollback_position() {
    let mut pane = pane_for_cache_tests();
    let rx = pane.add_process_session_for_test(1);
    let before = scroll_process_session_back(&mut pane, 0);
    assert!(before > 0);

    // A cursor-position report synthesized while the user reads scrollback.
    pane.sessions[0]
        .parser
        .callbacks_mut()
        .pending_inputs
        .push("\x1b[1;1R".to_owned());

    pane.sessions[0].flush_terminal_responses();

    assert_eq!(pane.sessions[0].scrollback(), before);
    match rx
        .try_recv()
        .expect("synthesized response should reach the pty")
    {
        TerminalCommand::Input(input) => assert_eq!(input, "\x1b[1;1R"),
        TerminalCommand::Resize(_) | TerminalCommand::Close => panic!("expected input"),
    }
}

#[test]
fn user_input_still_jumps_scrollback_to_bottom() {
    let mut pane = pane_for_cache_tests();
    let _rx = pane.add_process_session_for_test(1);
    let before = scroll_process_session_back(&mut pane, 0);
    assert!(before > 0);

    pane.send_input("x");

    assert_eq!(pane.sessions[0].scrollback(), 0);
}

#[test]
fn paste_text_surfaces_truncation_notice_for_oversized_paste() {
    let mut pane = pane_for_cache_tests();
    let rx = pane.add_process_session_for_test(1);
    let oversized = format!("{}\u{e9}", "a".repeat(TERMINAL_INPUT_MAX_BYTES));

    pane.paste_text(0, oversized);

    match rx
        .try_recv()
        .expect("bounded paste should still be delivered")
    {
        TerminalCommand::Input(input) => assert_eq!(input.len(), TERMINAL_INPUT_MAX_BYTES),
        TerminalCommand::Resize(_) | TerminalCommand::Close => panic!("expected paste input"),
    }
    assert_eq!(
        pane.active_paste_notice()
            .map(|(message, _)| message.to_owned()),
        Some(super::super::TERMINAL_PASTE_TRUNCATED_NOTICE.to_owned())
    );
}

#[test]
fn paste_text_surfaces_busy_notice_when_command_queue_is_full() {
    let mut pane = pane_for_cache_tests();
    let _rx = pane.add_process_session_for_test(1);
    let (tx, rx) = crossbeam_channel::bounded(1);
    tx.try_send(TerminalCommand::Input("occupied".to_owned()))
        .unwrap();
    pane.sessions[0].tx_command = Some(tx);

    pane.paste_text(0, "late paste".to_owned());

    assert_eq!(
        pane.active_paste_notice()
            .map(|(message, _)| message.to_owned()),
        Some(super::super::TERMINAL_PASTE_BUSY_NOTICE.to_owned())
    );
    match rx.try_recv().expect("existing queue item should remain") {
        TerminalCommand::Input(input) => assert_eq!(input, "occupied"),
        TerminalCommand::Resize(_) | TerminalCommand::Close => panic!("expected input"),
    }
    assert!(rx.try_recv().is_err());
}

#[test]
fn paste_text_surfaces_notice_when_session_cannot_receive_input() {
    let mut pane = pane_for_cache_tests();
    let rx = pane.add_process_session_for_test(1);
    pane.sessions[0].started = false;
    pane.sessions[0].auto_start_shell = false;

    pane.paste_text(0, "ignored".to_owned());

    assert!(rx.try_recv().is_err());
    assert_eq!(
        pane.active_paste_notice()
            .map(|(message, _)| message.to_owned()),
        Some(super::super::TERMINAL_PASTE_UNDELIVERED_NOTICE.to_owned())
    );
}

#[test]
fn paste_notice_expires_after_ttl() {
    let mut pane = pane_for_cache_tests();
    pane.show_paste_notice(super::super::TERMINAL_PASTE_UNDELIVERED_NOTICE);
    assert!(pane.active_paste_notice().is_some());

    let created = std::time::Instant::now();
    pane.paste_notice = Some(super::super::TerminalPasteNotice {
        message: super::super::TERMINAL_PASTE_UNDELIVERED_NOTICE.to_owned(),
        created,
    });

    assert!(pane.active_paste_notice_at(created).is_some());
    assert!(
        pane.active_paste_notice_at(created + 2 * super::super::TERMINAL_PASTE_NOTICE_TTL)
            .is_none()
    );
    pane.expire_paste_notice_at(created + 2 * super::super::TERMINAL_PASTE_NOTICE_TTL);
    assert!(pane.paste_notice.is_none());
}

#[test]
fn raw_session_input_uses_clamped_active_session_and_preserves_bytes() {
    let mut pane = pane_for_cache_tests();
    let first_rx = pane.add_process_session_for_test(1);
    let second_rx = pane.add_process_session_for_test(2);
    pane.active_session = 99;

    pane.send_input("\0\x1b[31mraw\r\n\u{7}");

    assert!(first_rx.try_recv().is_err());
    match second_rx
        .try_recv()
        .expect("input should target clamped session")
    {
        TerminalCommand::Input(input) => assert_eq!(input, "\0\x1b[31mraw\r\n\u{7}"),
        TerminalCommand::Resize(_) | TerminalCommand::Close => panic!("expected raw input"),
    }
}
