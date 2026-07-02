use super::*;

#[test]
fn terminal_session_snapshots_capture_session_layout() {
    let size = test_terminal_size();
    let mut first = session_without_command(1, size);
    first.initial_cwd = Some(PathBuf::from("workspace"));
    first.process_label = Some("Cargo Check".to_owned());
    first.parser.callbacks_mut().window_title = Some("cargo check".to_owned());
    first.replace_search_buffer("cargo check\nok\n".to_owned());
    first.parser.process("line\n".repeat(80).as_bytes());
    first.parser.screen_mut().set_scrollback(3);
    let mut second = session_without_command(2, size);
    second.initial_cwd = Some(PathBuf::from("workspace/tools"));
    let mut pane = pane_with_sessions(vec![first, second], size);
    pane.active_session = 1;
    pane.split_view = true;
    pane.split_weights = vec![0.3, 0.7];

    let snapshots = pane.terminal_session_snapshots();

    assert_eq!(snapshots.len(), 2);
    assert_eq!(
        snapshots[0].cwd.as_deref(),
        Some(std::path::Path::new("workspace"))
    );
    assert_eq!(snapshots[0].scrollback, "cargo check\nok\n");
    assert_eq!(snapshots[0].scrollback_offset, 3);
    assert_eq!(snapshots[0].process_label.as_deref(), Some("Cargo Check"));
    assert_eq!(
        snapshots[0].process_status,
        Some(PersistedTerminalProcessStatus::Running)
    );
    assert_eq!(snapshots[0].window_title.as_deref(), Some("cargo check"));
    assert_eq!(
        snapshots[1].cwd.as_deref(),
        Some(std::path::Path::new("workspace/tools"))
    );
    assert_eq!(pane.terminal_active_session_for_restore(), 1);
    assert!(pane.terminal_split_view_for_restore());
    assert_eq!(pane.terminal_split_weights_for_restore(), vec![0.3, 0.7]);
}

#[test]
fn terminal_split_weights_for_restore_sanitizes_non_finite_weights() {
    let size = test_terminal_size();
    let mut pane = pane_with_sessions(
        vec![
            session_without_command(1, size),
            session_without_command(2, size),
            session_without_command(3, size),
        ],
        size,
    );
    pane.split_weights = vec![0.5, f32::NAN, f32::INFINITY];

    assert_eq!(
        pane.terminal_split_weights_for_restore(),
        vec![0.5, 1.0, 1.0]
    );
}

#[test]
fn terminal_session_snapshots_capture_finished_process_statuses() {
    let size = test_terminal_size();
    let mut succeeded = session_without_command(1, size);
    succeeded.started = false;
    succeeded.process_label = Some("Cargo Check".to_owned());
    succeeded.last_process_exit_code = Some(0);

    let mut failed = session_without_command(2, size);
    failed.started = false;
    failed.process_label = Some("Cargo Test".to_owned());
    failed.last_process_exit_code = Some(17);

    let mut errored = session_without_command(3, size);
    errored.started = false;
    errored.process_label = Some("Cargo Build".to_owned());
    errored.last_process_terminal_error = true;

    let mut blank_label = session_without_command(4, size);
    blank_label.started = false;
    blank_label.process_label = Some("   ".to_owned());
    blank_label.last_process_exit_code = Some(9);

    let shell = session_without_command(5, size);
    let pane = pane_with_sessions(vec![succeeded, failed, errored, blank_label, shell], size);

    let statuses = pane
        .terminal_session_snapshots()
        .into_iter()
        .map(|snapshot| snapshot.process_status)
        .collect::<Vec<_>>();

    assert_eq!(
        statuses,
        vec![
            Some(PersistedTerminalProcessStatus::Exited { exit_code: Some(0) }),
            Some(PersistedTerminalProcessStatus::Exited {
                exit_code: Some(17)
            }),
            Some(PersistedTerminalProcessStatus::TerminalError),
            None,
            None,
        ]
    );
}

#[test]
fn terminal_session_snapshots_capture_queued_output_after_shutdown_drain() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(session_without_command(1, size), size);
    let mut expected = String::new();
    for index in 0..(TERMINAL_DRAIN_EVENT_BUDGET + 3) {
        let line = format!("queued before exit {index}\n");
        expected.push_str(&line);
        pane.sessions[0]
            .tx_output
            .send(TerminalEvent::Output(line.into_bytes()))
            .unwrap();
    }

    assert!(pane.terminal_session_snapshots()[0].scrollback.is_empty());
    assert_eq!(
        pane.drain_output_for_shutdown(),
        TERMINAL_DRAIN_EVENT_BUDGET + 3
    );
    assert!(!pane.has_pending_output());

    let snapshots = pane.terminal_session_snapshots();
    assert_eq!(snapshots[0].scrollback, expected);
}

#[test]
fn terminal_session_snapshots_keep_active_session_when_truncated() {
    let size = test_terminal_size();
    let mut sessions = Vec::new();
    for id in 1..=15 {
        let mut session = session_without_command(id, size);
        session.replace_search_buffer(format!("session {id}\n"));
        sessions.push(session);
    }
    let mut pane = pane_with_sessions(sessions, size);
    pane.active_session = 14;
    pane.split_weights = (1..=15).map(|weight| weight as f32).collect();

    let snapshots = pane.terminal_session_snapshots();

    assert_eq!(snapshots.len(), 12);
    assert_eq!(snapshots.last().unwrap().scrollback, "session 15\n");
    assert_eq!(pane.terminal_active_session_for_restore(), 11);
    assert_eq!(
        pane.terminal_split_weights_for_restore(),
        vec![
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 15.0
        ]
    );
}

#[test]
fn terminal_session_snapshots_bound_total_scrollback_and_prioritize_active_session() {
    let size = test_terminal_size();
    let mut sessions = Vec::new();
    for id in 1..=6 {
        let mut session = session_without_command(id, size);
        session.replace_search_buffer(format!(
            "{}active-marker-{id}\n",
            "line\n".repeat(70 * 1024)
        ));
        sessions.push(session);
    }
    let mut pane = pane_with_sessions(sessions, size);
    pane.active_session = 5;

    let snapshots = pane.terminal_session_snapshots();
    let total_scrollback_bytes = snapshots
        .iter()
        .map(|snapshot| snapshot.scrollback.len())
        .sum::<usize>();

    assert!(total_scrollback_bytes <= max_persisted_terminal_scrollback_total_bytes_for_test());
    assert!(snapshots[5].scrollback.ends_with("active-marker-6\n"));
    assert!(
        snapshots
            .iter()
            .any(|snapshot| snapshot.scrollback.is_empty())
    );
}

#[test]
fn terminal_session_snapshots_zero_offset_when_scrollback_is_omitted() {
    let size = PtySize {
        rows: 3,
        cols: 20,
        pixel_width: 0,
        pixel_height: 0,
    };
    let mut sessions = Vec::new();
    for id in 1..=6 {
        let mut session = session_without_command(id, size);
        session.replace_search_buffer(format!(
            "{}active-marker-{id}\n",
            "line\n".repeat(70 * 1024)
        ));
        session
            .parser
            .process("screen line\n".repeat(20).as_bytes());
        session.parser.screen_mut().set_scrollback(2);
        sessions.push(session);
    }
    let mut pane = pane_with_sessions(sessions, size);
    pane.active_session = 5;

    let snapshots = pane.terminal_session_snapshots();

    assert!(
        snapshots
            .iter()
            .any(|snapshot| snapshot.scrollback.is_empty())
    );
    assert!(
        snapshots
            .iter()
            .filter(|snapshot| snapshot.scrollback.is_empty())
            .all(|snapshot| snapshot.scrollback_offset == 0)
    );
    assert!(
        snapshots
            .iter()
            .filter(|snapshot| !snapshot.scrollback.is_empty())
            .any(|snapshot| snapshot.scrollback_offset > 0)
    );
}

#[test]
fn terminal_restores_session_metadata_and_auto_start_policy() {
    let root = temp_terminal_root("restore-metadata");
    let tools = root.join("tools");
    let trimmed = root.join("trimmed");
    fs::create_dir_all(&tools).unwrap();
    fs::create_dir_all(&trimmed).unwrap();
    let size = test_terminal_size();
    let mut pane = pane_with_sessions(Vec::new(), size);
    pane.cwd = root.clone();
    let snapshots = vec![
        PersistedTerminalSession {
            cwd: Some(root.clone()),
            scrollback: String::new(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: Some(" Cargo Test ".to_owned()),
            process_status: None,
            window_title: Some("kuroya workspace".to_owned()),
        },
        PersistedTerminalSession {
            cwd: Some(tools.clone()),
            scrollback: "cargo test\nfinished\n".to_owned(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: None,
            process_status: None,
            window_title: None,
        },
        PersistedTerminalSession {
            cwd: Some(trimmed),
            scrollback: "shell output\n".to_owned(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: Some("   ".to_owned()),
            process_status: None,
            window_title: None,
        },
    ];

    pane.restore_terminal_sessions(&snapshots, 4, true, &[0.25, f32::NAN], true);

    assert_eq!(pane.sessions.len(), 3);
    assert_eq!(pane.active_session, 2);
    assert!(pane.split_view);
    assert_eq!(pane.split_weights, vec![0.25, 1.0, 1.0]);
    assert!(pane.sessions.iter().all(|session| !session.started));
    assert!(!pane.sessions[0].auto_start_shell);
    assert!(pane.sessions[1].auto_start_shell);
    assert!(pane.sessions[2].auto_start_shell);
    assert_eq!(
        pane.sessions[0].initial_cwd.as_deref(),
        Some(root.as_path())
    );
    assert_eq!(
        pane.sessions[0].process_label.as_deref(),
        Some("Cargo Test")
    );
    assert_eq!(
        pane.sessions[0].parser.callbacks().window_title.as_deref(),
        Some("kuroya workspace")
    );
    assert!(pane.sessions[1].process_label.is_none());
    assert!(pane.sessions[2].process_label.is_none());
    assert!(pane.sessions[1].copyable_text().contains("cargo test"));
    assert_eq!(pane.sessions[1].search_buffer, "cargo test\nfinished\n");
    assert_eq!(pane.sessions[2].search_buffer, "shell output\n");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn terminal_restore_preserves_finished_process_statuses() {
    let size = test_terminal_size();
    let mut pane = pane_with_sessions(Vec::new(), size);
    let snapshots = vec![
        PersistedTerminalSession {
            cwd: Some(PathBuf::from("workspace")),
            scrollback: "cargo check\n".to_owned(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: Some("Cargo Check".to_owned()),
            process_status: Some(PersistedTerminalProcessStatus::Exited { exit_code: Some(0) }),
            window_title: None,
        },
        PersistedTerminalSession {
            cwd: Some(PathBuf::from("workspace")),
            scrollback: "cargo test\n".to_owned(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: Some("Cargo Test".to_owned()),
            process_status: Some(PersistedTerminalProcessStatus::Exited {
                exit_code: Some(17),
            }),
            window_title: None,
        },
        PersistedTerminalSession {
            cwd: Some(PathBuf::from("workspace")),
            scrollback: "cargo build\n".to_owned(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: Some("Cargo Build".to_owned()),
            process_status: Some(PersistedTerminalProcessStatus::TerminalError),
            window_title: None,
        },
        PersistedTerminalSession {
            cwd: Some(PathBuf::from("workspace")),
            scrollback: "cargo run\n".to_owned(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: Some("Cargo Run".to_owned()),
            process_status: Some(PersistedTerminalProcessStatus::Running),
            window_title: None,
        },
    ];

    pane.restore_terminal_sessions(&snapshots, 0, false, &[1.0; 4], true);

    assert!(pane.sessions.iter().all(|session| !session.started));
    assert_eq!(
        pane.sessions[0].command_status(),
        TerminalCommandStatus::Succeeded
    );
    assert_eq!(
        pane.sessions[1].command_status(),
        TerminalCommandStatus::Failed(17)
    );
    assert_eq!(
        pane.sessions[2].command_status(),
        TerminalCommandStatus::TerminalError
    );
    assert_eq!(
        pane.sessions[3].command_status(),
        TerminalCommandStatus::Stopped
    );
    assert_eq!(
        pane.process_session_state_by_id(1),
        Some(TerminalProcessSessionState::Exited(0))
    );
    assert_eq!(
        pane.process_session_state_by_id(2),
        Some(TerminalProcessSessionState::Exited(17))
    );
    assert_eq!(
        pane.process_session_state_by_id(3),
        Some(TerminalProcessSessionState::TerminalError)
    );
    assert_eq!(
        pane.process_session_state_by_id(4),
        Some(TerminalProcessSessionState::Stopped)
    );
    assert!(
        pane.sessions
            .iter()
            .all(|session| !session.auto_start_shell)
    );
}

#[test]
fn terminal_restore_disables_auto_start_when_not_allowed() {
    let root = temp_terminal_root("restore-no-auto-start");
    let tools = root.join("tools");
    fs::create_dir_all(&tools).unwrap();
    let size = test_terminal_size();
    let mut pane = pane_with_sessions(Vec::new(), size);
    pane.cwd = root.clone();
    let snapshots = vec![PersistedTerminalSession {
        cwd: Some(tools.clone()),
        scrollback: "restored shell\n".to_owned(),
        scrollback_offset: 0,
        custom_title: None,
        process_label: None,
        process_status: None,
        window_title: None,
    }];

    pane.restore_terminal_sessions(&snapshots, 0, false, &[1.0], false);
    pane.set_visible(true);

    assert_eq!(pane.sessions.len(), 1);
    assert!(!pane.sessions[0].auto_start_shell);
    assert!(!pane.sessions[0].started);
    assert_eq!(
        pane.sessions[0].initial_cwd.as_deref(),
        Some(tools.as_path())
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn terminal_restore_does_not_auto_start_shell_for_unlabeled_process_status() {
    let root = temp_terminal_root("restore-unlabeled-process-status");
    fs::create_dir_all(&root).unwrap();
    let size = test_terminal_size();
    let mut pane = pane_with_sessions(Vec::new(), size);
    pane.cwd = root.clone();
    let snapshots = vec![PersistedTerminalSession {
        cwd: Some(root.clone()),
        scrollback: "restored process\n".to_owned(),
        scrollback_offset: 0,
        custom_title: None,
        process_label: Some("   ".to_owned()),
        process_status: Some(PersistedTerminalProcessStatus::Running),
        window_title: None,
    }];

    pane.restore_terminal_sessions(&snapshots, 0, false, &[1.0], true);
    pane.set_visible(false);
    pane.set_visible(true);

    assert_eq!(pane.sessions.len(), 1);
    assert!(!pane.sessions[0].auto_start_shell);
    assert!(!pane.sessions[0].started);
    assert!(pane.sessions[0].tx_command.is_none());
    assert_eq!(
        pane.sessions[0].initial_cwd.as_deref(),
        Some(root.as_path())
    );
    assert_eq!(pane.sessions[0].search_buffer, "restored process\n");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn terminal_restore_accepts_current_dir_workspace_root_cwd() {
    let size = test_terminal_size();
    let mut pane = pane_with_sessions(Vec::new(), size);
    pane.cwd = PathBuf::from(".");
    let snapshots = vec![PersistedTerminalSession {
        cwd: Some(PathBuf::from(".")),
        scrollback: "current cwd\n".to_owned(),
        scrollback_offset: 0,
        custom_title: None,
        process_label: None,
        process_status: None,
        window_title: None,
    }];

    pane.restore_terminal_sessions(&snapshots, 0, true, &[1.0], true);

    assert_eq!(pane.sessions.len(), 1);
    assert!(pane.sessions[0].auto_start_shell);
    assert_eq!(
        pane.sessions[0].initial_cwd.as_deref(),
        Some(Path::new("."))
    );
}

#[test]
fn terminal_restore_preserves_scrollback_offset() {
    let size = PtySize {
        rows: 3,
        cols: 20,
        pixel_width: 0,
        pixel_height: 0,
    };
    let mut pane = pane_with_sessions(Vec::new(), size);
    let snapshots = vec![PersistedTerminalSession {
        cwd: Some(PathBuf::from("workspace")),
        scrollback: "line0\nline1\nline2\nline3\nline4\n".to_owned(),
        scrollback_offset: 2,
        custom_title: None,
        process_label: None,
        process_status: None,
        window_title: None,
    }];

    pane.restore_terminal_sessions(&snapshots, 0, false, &[1.0], true);

    assert_eq!(pane.sessions[0].scrollback(), 2);
}

#[test]
fn terminal_restore_bounds_total_scrollback_and_prioritizes_active_session() {
    let size = test_terminal_size();
    let mut snapshots = Vec::new();
    for id in 1..=6 {
        snapshots.push(PersistedTerminalSession {
            cwd: Some(PathBuf::from("workspace")),
            scrollback: format!("{}active-marker-{id}\n", "line\n".repeat(70 * 1024)),
            scrollback_offset: 0,
            custom_title: None,
            process_label: None,
            process_status: None,
            window_title: None,
        });
    }
    let mut pane = pane_with_sessions(Vec::new(), size);

    pane.restore_terminal_sessions(&snapshots, 5, false, &[1.0; 6], true);

    let total_scrollback_bytes = pane
        .sessions
        .iter()
        .map(|session| session.search_buffer.len())
        .sum::<usize>();
    assert!(total_scrollback_bytes <= max_persisted_terminal_scrollback_total_bytes_for_test());
    assert!(
        pane.sessions[pane.active_session]
            .search_buffer
            .ends_with("active-marker-6\n")
    );
    assert!(pane.sessions.len() < snapshots.len());
    assert!(
        pane.sessions
            .iter()
            .all(|session| !session.search_buffer.is_empty())
    );
}

#[test]
fn terminal_restore_keeps_active_session_when_snapshot_list_is_truncated() {
    let size = test_terminal_size();
    let mut snapshots = Vec::new();
    for id in 1..=15 {
        snapshots.push(PersistedTerminalSession {
            cwd: Some(PathBuf::from("workspace")),
            scrollback: format!("session {id}\n"),
            scrollback_offset: 0,
            custom_title: None,
            process_label: None,
            process_status: None,
            window_title: None,
        });
    }
    let split_weights = (1..=15).map(|weight| weight as f32).collect::<Vec<_>>();
    let mut pane = pane_with_sessions(Vec::new(), size);

    pane.restore_terminal_sessions(&snapshots, 14, true, &split_weights, true);

    assert_eq!(pane.sessions.len(), 12);
    assert_eq!(pane.active_session, 11);
    assert_eq!(
        pane.sessions
            .iter()
            .map(|session| session.search_buffer.as_str())
            .collect::<Vec<_>>(),
        vec![
            "session 1\n",
            "session 2\n",
            "session 3\n",
            "session 4\n",
            "session 5\n",
            "session 6\n",
            "session 7\n",
            "session 8\n",
            "session 9\n",
            "session 10\n",
            "session 11\n",
            "session 15\n",
        ]
    );
    assert_eq!(
        pane.split_weights,
        vec![
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 15.0
        ]
    );
}

#[test]
fn terminal_restore_keeps_shell_stopped_when_cwd_is_missing_or_stale() {
    let root = temp_terminal_root("restore-stale-cwd");
    fs::create_dir_all(&root).unwrap();
    let size = test_terminal_size();
    let mut pane = pane_with_sessions(Vec::new(), size);
    pane.cwd = root.clone();
    let snapshots = vec![
        PersistedTerminalSession {
            cwd: None,
            scrollback: "missing cwd\n".to_owned(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: None,
            process_status: None,
            window_title: None,
        },
        PersistedTerminalSession {
            cwd: Some(root.join("deleted")),
            scrollback: "stale cwd\n".to_owned(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: None,
            process_status: None,
            window_title: None,
        },
    ];

    pane.restore_terminal_sessions(&snapshots, 0, true, &[1.0, 1.0], true);
    pane.set_visible(false);
    pane.set_visible(true);

    assert_eq!(pane.sessions.len(), 2);
    assert!(pane.sessions.iter().all(|session| !session.started));
    assert!(
        pane.sessions
            .iter()
            .all(|session| !session.auto_start_shell)
    );
    assert!(
        pane.sessions
            .iter()
            .all(|session| { session.initial_cwd.as_deref() == Some(root.as_path()) })
    );
    assert_eq!(pane.sessions[0].search_buffer, "missing cwd\n");
    assert_eq!(pane.sessions[1].search_buffer, "stale cwd\n");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn terminal_restore_rejects_control_character_cwd() {
    let root = temp_terminal_root("restore-control-cwd");
    fs::create_dir_all(&root).unwrap();
    let size = test_terminal_size();
    let mut pane = pane_with_sessions(Vec::new(), size);
    pane.cwd = root.clone();
    let snapshots = vec![PersistedTerminalSession {
        cwd: Some(PathBuf::from("tools\nbad")),
        scrollback: "control cwd\n".to_owned(),
        scrollback_offset: 0,
        custom_title: None,
        process_label: None,
        process_status: None,
        window_title: None,
    }];

    pane.restore_terminal_sessions(&snapshots, 0, true, &[1.0], true);

    assert_eq!(pane.sessions.len(), 1);
    assert!(!pane.sessions[0].started);
    assert!(!pane.sessions[0].auto_start_shell);
    assert_eq!(
        pane.sessions[0].initial_cwd.as_deref(),
        Some(root.as_path())
    );
    assert_eq!(pane.sessions[0].search_buffer, "control cwd\n");
    fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn terminal_restore_accepts_case_insensitive_absolute_cwd_inside_workspace() {
    let root = temp_terminal_root("restore-case-insensitive-cwd");
    let tools = root.join("Tools");
    fs::create_dir_all(&tools).unwrap();
    let altered_case_tools = PathBuf::from(tools.to_string_lossy().to_ascii_lowercase());
    let size = test_terminal_size();
    let mut pane = pane_with_sessions(Vec::new(), size);
    pane.cwd = root.clone();
    let snapshots = vec![PersistedTerminalSession {
        cwd: Some(altered_case_tools.clone()),
        scrollback: "case cwd\n".to_owned(),
        scrollback_offset: 0,
        custom_title: None,
        process_label: None,
        process_status: None,
        window_title: None,
    }];

    pane.restore_terminal_sessions(&snapshots, 0, true, &[1.0], true);

    assert_eq!(pane.sessions.len(), 1);
    assert!(pane.sessions[0].auto_start_shell);
    assert_eq!(
        pane.sessions[0].initial_cwd.as_deref(),
        Some(altered_case_tools.as_path())
    );
    assert_eq!(pane.sessions[0].search_buffer, "case cwd\n");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn terminal_restore_accepts_workspace_prefixed_relative_cwd() {
    let parent = temp_terminal_root("restore-prefixed-cwd");
    let root = parent.join("workspace");
    let tools = root.join("tools");
    let nested_tools = root.join("workspace").join("tools");
    fs::create_dir_all(&tools).unwrap();
    fs::create_dir_all(&nested_tools).unwrap();
    let size = test_terminal_size();
    let mut pane = pane_with_sessions(Vec::new(), size);
    pane.cwd = root.clone();
    let snapshots = vec![PersistedTerminalSession {
        cwd: Some(PathBuf::from("workspace").join("tools")),
        scrollback: "prefixed cwd\n".to_owned(),
        scrollback_offset: 0,
        custom_title: None,
        process_label: None,
        process_status: None,
        window_title: None,
    }];

    pane.restore_terminal_sessions(&snapshots, 0, true, &[1.0], true);

    assert_eq!(pane.sessions.len(), 1);
    assert!(pane.sessions[0].auto_start_shell);
    assert_eq!(
        pane.sessions[0].initial_cwd.as_deref(),
        Some(tools.as_path())
    );
    assert_eq!(pane.sessions[0].search_buffer, "prefixed cwd\n");

    let round_trip = pane.terminal_session_snapshots();
    assert_eq!(round_trip.len(), 1);
    assert_eq!(round_trip[0].cwd.as_deref(), Some(tools.as_path()));
    let mut restored_again = pane_with_sessions(Vec::new(), size);
    restored_again.cwd = root.clone();
    restored_again.restore_terminal_sessions(&round_trip, 0, false, &[1.0], true);
    assert_eq!(
        restored_again.sessions[0].initial_cwd.as_deref(),
        Some(tools.as_path())
    );
    fs::remove_dir_all(parent).unwrap();
}

#[test]
fn terminal_restore_rejects_workspace_prefixed_relative_cwd_escape() {
    let parent = temp_terminal_root("restore-prefixed-escape-cwd");
    let root = parent.join("workspace");
    let inside_shadow = root.join("outside");
    fs::create_dir_all(&inside_shadow).unwrap();
    let size = test_terminal_size();
    let mut pane = pane_with_sessions(Vec::new(), size);
    pane.cwd = root.clone();
    let snapshots = vec![
        PersistedTerminalSession {
            cwd: Some(PathBuf::from("workspace").join("..")),
            scrollback: "prefixed parent escape\n".to_owned(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: None,
            process_status: None,
            window_title: None,
        },
        PersistedTerminalSession {
            cwd: Some(PathBuf::from("workspace/tools/../../outside")),
            scrollback: "prefixed nested escape\n".to_owned(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: None,
            process_status: None,
            window_title: None,
        },
    ];

    pane.restore_terminal_sessions(&snapshots, 0, true, &[1.0, 1.0], true);

    assert_eq!(pane.sessions.len(), 2);
    assert!(pane.sessions.iter().all(|session| !session.started));
    assert!(
        pane.sessions
            .iter()
            .all(|session| !session.auto_start_shell)
    );
    assert!(
        pane.sessions
            .iter()
            .all(|session| session.initial_cwd.as_deref() == Some(root.as_path()))
    );
    fs::remove_dir_all(parent).unwrap();
}

#[test]
fn terminal_restore_rejects_cwd_outside_workspace() {
    let parent = temp_terminal_root("restore-outside-cwd");
    let root = parent.join("workspace");
    let outside = parent.join("outside");
    let adjacent = parent.join("workspace-other");
    let inside = root.join("tools");
    fs::create_dir_all(&outside).unwrap();
    fs::create_dir_all(&adjacent).unwrap();
    fs::create_dir_all(&inside).unwrap();
    let size = test_terminal_size();
    let mut pane = pane_with_sessions(Vec::new(), size);
    pane.cwd = root.clone();
    let snapshots = vec![
        PersistedTerminalSession {
            cwd: Some(PathBuf::from("../outside")),
            scrollback: "relative escape\n".to_owned(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: None,
            process_status: None,
            window_title: None,
        },
        PersistedTerminalSession {
            cwd: Some(outside.clone()),
            scrollback: "absolute escape\n".to_owned(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: None,
            process_status: None,
            window_title: None,
        },
        PersistedTerminalSession {
            cwd: Some(PathBuf::from("../workspace-other")),
            scrollback: "sibling escape\n".to_owned(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: None,
            process_status: None,
            window_title: None,
        },
        PersistedTerminalSession {
            cwd: Some(
                PathBuf::from("workspace")
                    .join("..")
                    .join("workspace-other"),
            ),
            scrollback: "prefixed sibling escape\n".to_owned(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: None,
            process_status: None,
            window_title: None,
        },
        PersistedTerminalSession {
            cwd: Some(
                PathBuf::from("workspace")
                    .join("..")
                    .join("..")
                    .join("outside"),
            ),
            scrollback: "prefixed parent escape\n".to_owned(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: None,
            process_status: None,
            window_title: None,
        },
        PersistedTerminalSession {
            cwd: Some(PathBuf::from("tools")),
            scrollback: "inside cwd\n".to_owned(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: None,
            process_status: None,
            window_title: None,
        },
    ];

    pane.restore_terminal_sessions(&snapshots, 0, true, &[1.0; 6], true);

    assert_eq!(pane.sessions.len(), 6);
    assert!(!pane.sessions[0].started);
    assert!(!pane.sessions[1].started);
    assert!(!pane.sessions[2].started);
    assert!(!pane.sessions[3].started);
    assert!(!pane.sessions[4].started);
    assert!(!pane.sessions[5].started);
    assert!(!pane.sessions[0].auto_start_shell);
    assert!(!pane.sessions[1].auto_start_shell);
    assert!(!pane.sessions[2].auto_start_shell);
    assert!(!pane.sessions[3].auto_start_shell);
    assert!(!pane.sessions[4].auto_start_shell);
    assert!(pane.sessions[5].auto_start_shell);
    assert_eq!(
        pane.sessions[0].initial_cwd.as_deref(),
        Some(root.as_path())
    );
    assert_eq!(
        pane.sessions[1].initial_cwd.as_deref(),
        Some(root.as_path())
    );
    assert_eq!(
        pane.sessions[2].initial_cwd.as_deref(),
        Some(root.as_path())
    );
    assert_eq!(
        pane.sessions[3].initial_cwd.as_deref(),
        Some(root.as_path())
    );
    assert_eq!(
        pane.sessions[4].initial_cwd.as_deref(),
        Some(root.as_path())
    );
    assert_eq!(
        pane.sessions[5].initial_cwd.as_deref(),
        Some(inside.as_path())
    );
    assert_eq!(pane.sessions[0].search_buffer, "relative escape\n");
    assert_eq!(pane.sessions[1].search_buffer, "absolute escape\n");
    assert_eq!(pane.sessions[2].search_buffer, "sibling escape\n");
    assert_eq!(pane.sessions[3].search_buffer, "prefixed sibling escape\n");
    assert_eq!(pane.sessions[4].search_buffer, "prefixed parent escape\n");
    assert_eq!(pane.sessions[5].search_buffer, "inside cwd\n");
    fs::remove_dir_all(parent).unwrap();
}

#[test]
fn terminal_restore_rejects_relative_cwd_escape_reentry() {
    let parent = temp_terminal_root("restore-reentry-cwd");
    let root = parent.join("workspace");
    let tools = root.join("tools");
    fs::create_dir_all(&tools).unwrap();
    let size = test_terminal_size();
    let mut pane = pane_with_sessions(Vec::new(), size);
    pane.cwd = root.clone();
    let snapshots = vec![
        PersistedTerminalSession {
            cwd: Some(PathBuf::from("..").join("workspace")),
            scrollback: "relative reentry\n".to_owned(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: None,
            process_status: None,
            window_title: None,
        },
        PersistedTerminalSession {
            cwd: Some(
                PathBuf::from("tools")
                    .join("..")
                    .join("..")
                    .join("workspace")
                    .join("tools"),
            ),
            scrollback: "nested reentry\n".to_owned(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: None,
            process_status: None,
            window_title: None,
        },
        PersistedTerminalSession {
            cwd: Some(root.join("..").join("workspace").join("tools")),
            scrollback: "absolute reentry\n".to_owned(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: None,
            process_status: None,
            window_title: None,
        },
    ];

    pane.restore_terminal_sessions(&snapshots, 0, true, &[1.0, 1.0], true);

    assert_eq!(pane.sessions.len(), 3);
    assert!(pane.sessions.iter().all(|session| !session.started));
    assert!(
        pane.sessions
            .iter()
            .all(|session| !session.auto_start_shell)
    );
    assert!(
        pane.sessions
            .iter()
            .all(|session| session.initial_cwd.as_deref() == Some(root.as_path()))
    );
    fs::remove_dir_all(parent).unwrap();
}

#[test]
fn terminal_restore_clears_session_scoped_pending_state() {
    let size = test_terminal_size();
    let mut pane = pane_with_sessions(vec![session_without_command(1, size)], size);
    pane.pending_multiline_paste = Some(TerminalPendingPaste {
        session_id: 1,
        text: "one\ntwo".to_owned(),
    });
    pane.search_cache = TerminalSearchCache {
        scope: TerminalSearchCacheScope::Single {
            session_id: 1,
            generation: 7,
        },
        query: "old".to_owned(),
        matches: Vec::new(),
        progress: Default::default(),
    };
    pane.search_match = 3;
    let snapshots = vec![PersistedTerminalSession {
        cwd: Some(PathBuf::from("workspace")),
        scrollback: "restored\n".to_owned(),
        scrollback_offset: 0,
        custom_title: None,
        process_label: None,
        process_status: None,
        window_title: None,
    }];

    pane.restore_terminal_sessions(&snapshots, 0, false, &[1.0], true);

    assert!(pane.pending_multiline_paste.is_none());
    assert_eq!(pane.search_cache.scope, TerminalSearchCacheScope::Empty);
    assert!(pane.search_cache.query.is_empty());
    assert!(pane.search_cache.matches.is_empty());
    assert_eq!(pane.search_match, 0);
}

#[test]
fn terminal_restore_allocates_fresh_session_ids_after_existing_sessions() {
    let size = test_terminal_size();
    let mut pane = pane_with_sessions(
        vec![
            session_without_command(1, size),
            session_without_command(2, size),
        ],
        size,
    );
    let snapshots = vec![PersistedTerminalSession {
        cwd: Some(PathBuf::from("workspace")),
        scrollback: "restored\n".to_owned(),
        scrollback_offset: 0,
        custom_title: None,
        process_label: None,
        process_status: None,
        window_title: None,
    }];

    pane.restore_terminal_sessions(&snapshots, 0, false, &[1.0], true);

    assert_eq!(pane.sessions.len(), 1);
    assert_eq!(pane.sessions[0].id, 3);
    assert_eq!(pane.next_session_id, 4);
}

#[test]
fn terminal_restored_process_session_does_not_auto_start_shell() {
    let size = test_terminal_size();
    let mut pane = pane_with_sessions(Vec::new(), size);
    let snapshots = vec![PersistedTerminalSession {
        cwd: Some(PathBuf::from("workspace")),
        scrollback: "cargo test\nfinished\n".to_owned(),
        scrollback_offset: 0,
        custom_title: None,
        process_label: Some("Cargo Test".to_owned()),
        process_status: None,
        window_title: None,
    }];

    pane.restore_terminal_sessions(&snapshots, 0, false, &[1.0], true);
    pane.set_visible(false);
    pane.set_visible(true);

    assert!(!pane.sessions[0].started);
    assert!(!pane.sessions[0].auto_start_shell);
    assert!(pane.sessions[0].tx_command.is_none());
    assert_eq!(pane.sessions[0].search_buffer, "cargo test\nfinished\n");
}

#[test]
fn terminal_hide_show_preserves_restored_split_layout() {
    let size = test_terminal_size();
    let mut pane = pane_with_sessions(Vec::new(), size);
    let snapshots = vec![
        PersistedTerminalSession {
            cwd: Some(PathBuf::from("workspace")),
            scrollback: "left\n".to_owned(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: Some("Left".to_owned()),
            process_status: None,
            window_title: None,
        },
        PersistedTerminalSession {
            cwd: Some(PathBuf::from("workspace/tools")),
            scrollback: "right\n".to_owned(),
            scrollback_offset: 0,
            custom_title: None,
            process_label: Some("Right".to_owned()),
            process_status: None,
            window_title: None,
        },
    ];

    pane.restore_terminal_sessions(&snapshots, 1, true, &[0.4, 0.6], true);
    pane.set_visible(false);

    assert!(!pane.visible);
    assert!(pane.split_view);
    assert_eq!(pane.split_weights, vec![0.4, 0.6]);

    pane.set_visible(true);

    assert!(pane.visible);
    assert!(pane.split_view);
    assert_eq!(pane.active_session, 1);
    assert_eq!(pane.split_weights, vec![0.4, 0.6]);
    assert!(!pane.sessions[0].started);
    assert!(!pane.sessions[1].started);
}

#[test]
fn terminal_restore_split_weights_are_sized_and_sanitized() {
    assert_eq!(
        restored_terminal_split_weights_for_test(&[0.4, f32::INFINITY], 3),
        vec![0.4, 1.0, 1.0]
    );
    assert!(restored_terminal_split_weights_for_test(&[0.4], 0).is_empty());
}

#[test]
fn terminal_persisted_labels_are_trimmed_bounded_and_optional() {
    assert_eq!(
        normalized_persisted_terminal_label_for_test(Some("  cargo test  ")).as_deref(),
        Some("cargo test")
    );
    assert_eq!(
        normalized_persisted_terminal_label_for_test(Some("   ")),
        None
    );
    assert_eq!(normalized_persisted_terminal_label_for_test(None), None);

    let long = "x".repeat(150);
    let normalized = normalized_persisted_terminal_label_for_test(Some(&long)).unwrap();
    assert_eq!(normalized.chars().count(), 120);
}

#[test]
fn terminal_persisted_scrollback_is_bounded_to_recent_output() {
    let mut scrollback = "a".repeat(300 * 1024);
    scrollback.push_str("needle");

    let persisted = persisted_terminal_scrollback_for_test(&scrollback);

    assert!(persisted.len() <= 256 * 1024);
    assert!(persisted.ends_with("needle"));
}

#[test]
fn terminal_persisted_scrollback_prefers_utf8_line_boundary() {
    let prefix = "α".repeat(140 * 1024);
    let scrollback = format!("{prefix}\nfirst full line\nneedle\n");

    let persisted = persisted_terminal_scrollback_for_test(&scrollback);

    assert!(persisted.len() <= 256 * 1024);
    assert_eq!(persisted, "first full line\nneedle\n");
}

#[test]
fn terminal_persisted_scrollback_sanitizes_control_sequences() {
    let persisted = persisted_terminal_scrollback_for_test(
        "start\x1b[31mred\x1b[0m\r\n\x1b]0;ignored title\x07done\u{7}\nabc\u{8}d\n",
    );

    assert_eq!(persisted, "startred\ndone\nabd\n");
}

#[test]
fn terminal_restore_sanitizes_malformed_persisted_scrollback() {
    let size = test_terminal_size();
    let mut pane = pane_with_sessions(Vec::new(), size);
    let snapshots = vec![PersistedTerminalSession {
        cwd: Some(PathBuf::from("workspace")),
        scrollback: "ok \u{9b}31mred\u{9b}0m\r\n\x1b]0;stale title\x07done\u{8}!\n".to_owned(),
        scrollback_offset: 0,
        custom_title: None,
        process_label: None,
        process_status: None,
        window_title: None,
    }];

    pane.restore_terminal_sessions(&snapshots, 0, false, &[1.0], true);

    assert_eq!(pane.sessions[0].search_buffer, "ok red\ndon!\n");
    assert!(pane.sessions[0].copyable_text().contains("ok red"));
    assert_eq!(pane.sessions[0].parser.callbacks().window_title, None);
}
