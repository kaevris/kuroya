use super::*;

#[test]
fn terminal_show_exit_alert_setting_is_configurable() {
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );

    pane.set_show_exit_alert(false);

    assert!(!pane.show_exit_alert);
}

#[test]
fn terminal_hide_on_last_closed_setting_is_configurable() {
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );

    pane.set_hide_on_last_closed(false);

    assert!(!pane.hide_on_last_closed);
}

#[test]
fn terminal_confirm_on_kill_setting_is_configurable() {
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );

    pane.set_confirm_on_kill(TerminalConfirmOnKill::Panel);

    assert_eq!(pane.confirm_on_kill, TerminalConfirmOnKill::Panel);
}

#[test]
fn terminal_tabs_show_actions_setting_controls_toolbar_actions() {
    let size = test_terminal_size();
    let first = session_without_command(1, size);
    let second = session_without_command(2, size);
    let mut pane = pane_with_sessions(vec![first, second], size);

    assert!(pane.terminal_action_buttons_visible());

    pane.set_tabs_show_actions(TerminalTabsShowActions::SingleTerminal);
    assert!(!pane.terminal_action_buttons_visible());

    pane.set_tabs_show_actions(TerminalTabsShowActions::Always);
    assert!(pane.terminal_action_buttons_visible());

    pane.set_tabs_show_actions(TerminalTabsShowActions::Never);
    assert!(!pane.terminal_action_buttons_visible());
}

#[test]
fn terminal_tabs_enabled_switches_to_active_session_dropdown() {
    let size = test_terminal_size();
    let first = session_without_command(1, size);
    let second = session_without_command(2, size);
    let mut pane = pane_with_sessions(vec![first, second], size);

    assert!(pane.terminal_session_tabs_visible());
    assert!(!pane.terminal_active_session_dropdown_visible());

    pane.set_tabs_enabled(false);

    assert!(!pane.terminal_session_tabs_visible());
    assert!(pane.terminal_active_session_dropdown_visible());

    pane.sessions.truncate(1);

    assert!(!pane.terminal_active_session_dropdown_visible());
}

#[test]
fn terminal_tabs_show_active_terminal_controls_compact_info() {
    let size = test_terminal_size();
    let first = session_without_command(1, size);
    let second = session_without_command(2, size);
    let mut pane = pane_with_sessions(vec![first, second], size);

    assert!(pane.terminal_session_tabs_visible());
    assert!(!pane.terminal_active_info_visible());

    pane.sessions.truncate(1);
    assert!(!pane.terminal_session_tabs_visible());
    assert!(pane.terminal_active_info_visible());

    pane.set_tabs_show_active_terminal(TerminalTabsShowActiveTerminal::Never);
    assert!(!pane.terminal_active_info_visible());

    pane.set_tabs_show_active_terminal(TerminalTabsShowActiveTerminal::Always);
    assert!(pane.terminal_active_info_visible());

    pane.sessions.push(session_without_command(2, size));
    assert!(pane.terminal_session_tabs_visible());
    assert!(!pane.terminal_active_info_visible());

    pane.set_tabs_hide_condition(TerminalTabsHideCondition::SingleGroup);
    pane.split_view = true;
    assert!(!pane.terminal_session_tabs_visible());
    assert!(pane.terminal_active_info_visible());

    pane.set_tabs_enabled(false);
    assert!(pane.terminal_active_session_dropdown_visible());
    assert!(!pane.terminal_active_info_visible());
}

#[test]
fn terminal_tabs_focus_mode_controls_tab_activation_focus() {
    let size = test_terminal_size();
    let first = session_without_command(1, size);
    let second = session_without_command(2, size);
    let mut pane = pane_with_sessions(vec![first, second], size);

    pane.activate_session_tab(1, true, false);
    assert_eq!(pane.active_session, 1);
    assert!(pane.focus_input_on_show);

    pane.focus_input_on_show = false;
    pane.set_tabs_focus_mode(TerminalTabsFocusMode::DoubleClick);
    pane.activate_session_tab(0, true, false);
    assert_eq!(pane.active_session, 0);
    assert!(!pane.focus_input_on_show);

    pane.activate_session_tab(1, true, true);

    assert_eq!(pane.active_session, 1);
    assert!(pane.focus_input_on_show);
}

#[test]
fn terminal_tabs_location_controls_side_rail_position() {
    let size = test_terminal_size();
    let first = session_without_command(1, size);
    let second = session_without_command(2, size);
    let mut pane = pane_with_sessions(vec![first, second], size);

    assert_eq!(pane.terminal_tabs_rail_location(), None);

    pane.set_tabs_location(TerminalTabsLocation::Left);
    assert_eq!(
        pane.terminal_tabs_rail_location(),
        Some(TerminalTabsLocation::Left)
    );

    pane.set_tabs_location(TerminalTabsLocation::Right);
    assert_eq!(
        pane.terminal_tabs_rail_location(),
        Some(TerminalTabsLocation::Right)
    );

    pane.set_tabs_enabled(false);
    assert_eq!(pane.terminal_tabs_rail_location(), None);
}

#[test]
fn terminal_tab_icon_and_color_settings_are_configurable() {
    let mut pane = TerminalPane::new(
        PathBuf::from("workspace"),
        TERMINAL_SCROLLBACK_ROWS,
        DEFAULT_TERMINAL_FONT_SIZE,
        DEFAULT_TERMINAL_LINE_HEIGHT,
    );

    pane.set_tabs_default_icon(" code ");
    pane.set_tabs_default_color(Some(" #3b78ff ".to_owned()));

    assert_eq!(pane.tabs_default_icon, "code");
    assert_eq!(pane.tabs_default_color.as_deref(), Some("#3b78ff"));

    pane.set_tabs_default_icon(" ");
    pane.set_tabs_default_color(Some(" ".to_owned()));

    assert_eq!(pane.tabs_default_icon, DEFAULT_TERMINAL_TABS_DEFAULT_ICON);
    assert_eq!(pane.tabs_default_color, None);
}

#[test]
fn terminal_tab_icon_setting_maps_supported_codicons() {
    assert_eq!(terminal_tab_icon_kind("code"), IconKind::Code);
    assert_eq!(
        terminal_tab_icon_kind("codicon-settings"),
        IconKind::Settings
    );
    assert_eq!(terminal_tab_icon_kind("git-branch"), IconKind::GitBranch);
    assert_eq!(terminal_tab_icon_kind("unknown-icon"), IconKind::Terminal);
}

#[test]
fn terminal_tab_hex_color_setting_parses_rgb_values() {
    assert_eq!(
        parse_terminal_tab_hex_color("#3b78ff"),
        Some(egui::Color32::from_rgb(59, 120, 255))
    );
    assert_eq!(parse_terminal_tab_hex_color("3b78ff"), None);
    assert_eq!(parse_terminal_tab_hex_color("#bad"), None);
}

#[test]
fn terminal_tabs_hide_condition_controls_session_tabs() {
    let size = test_terminal_size();
    let first = session_without_command(1, size);
    let second = session_without_command(2, size);
    let mut pane = pane_with_sessions(vec![first, second], size);

    assert!(pane.terminal_session_tabs_visible());

    pane.sessions.truncate(1);
    assert!(!pane.terminal_session_tabs_visible());

    pane.set_tabs_hide_condition(TerminalTabsHideCondition::Never);
    assert!(pane.terminal_session_tabs_visible());

    pane.sessions.push(session_without_command(2, size));
    pane.split_view = true;
    pane.set_tabs_hide_condition(TerminalTabsHideCondition::SingleGroup);
    assert!(!pane.terminal_session_tabs_visible());

    pane.split_view = false;
    assert!(pane.terminal_session_tabs_visible());
}

#[test]
fn terminal_session_labels_stay_shell_based_in_split_mode() {
    assert_eq!(terminal_session_label(7), "Terminal 7");
}

#[test]
fn terminal_agent_cli_title_setting_controls_tab_label() {
    let size = test_terminal_size();
    let mut session = session_without_command(1, size);
    session.parser.process(b"\x1b]2;Task Runner\x07");
    let mut pane = pane_with_sessions(vec![session], size);

    assert_eq!(
        pane.terminal_session_label(&pane.sessions[0]),
        "Task Runner 1"
    );

    pane.set_tabs_allow_agent_cli_title(false);
    assert_eq!(pane.terminal_session_label(&pane.sessions[0]), "Terminal 1");
}

#[test]
fn terminal_session_label_sanitizes_control_and_bidi_process_label_for_display() {
    let size = test_terminal_size();
    let mut session = session_without_command(1, size);
    let raw_label = "  Cargo\r\n\u{202e}Test\t\u{2066}Build\x1bDone  ".to_owned();
    session.process_label = Some(raw_label.clone());
    let pane = pane_with_sessions(vec![session], size);

    assert_eq!(
        pane.terminal_session_label(&pane.sessions[0]),
        "Cargo Test Build Done 1"
    );
    assert_eq!(
        pane.sessions[0].process_label.as_deref(),
        Some(raw_label.as_str())
    );
}

#[test]
fn terminal_session_label_sanitizes_restored_window_title_for_display() {
    let size = test_terminal_size();
    let mut session = session_without_command(1, size);
    let raw_title = "  Restored\r\n\u{202e}Title\t\u{2066}Done  ".to_owned();
    session.parser.callbacks_mut().window_title = Some(raw_title.clone());
    let pane = pane_with_sessions(vec![session], size);

    assert_eq!(
        pane.terminal_session_label(&pane.sessions[0]),
        "Restored Title Done 1"
    );
    assert_eq!(
        pane.sessions[0].parser.callbacks().window_title.as_deref(),
        Some(raw_title.as_str())
    );
}

#[test]
fn terminal_session_label_ignores_executable_path_window_title() {
    let size = test_terminal_size();
    let mut session = session_without_command(1, size);
    session.parser.callbacks_mut().window_title =
        Some(r"C:\WINDOWS\System32\WindowsPowerShell\v1.0\powershell.exe".to_owned());
    let pane = pane_with_sessions(vec![session], size);

    assert_eq!(pane.terminal_session_label(&pane.sessions[0]), "Terminal 1");
}

#[test]
fn terminal_custom_title_overrides_shell_and_sequence_labels() {
    let size = test_terminal_size();
    let mut session = session_without_command(2, size);
    session.custom_title = Some("  Build\r\n\u{202e}Main  ".to_owned());
    session.process_label = Some("Cargo".to_owned());
    session.parser.process(b"\x1b]2;Task Runner\x07");
    let pane = pane_with_sessions(vec![session], size);

    assert_eq!(pane.terminal_session_label(&pane.sessions[0]), "Build Main");
}

#[test]
fn terminal_session_label_caps_process_label_for_display() {
    let size = test_terminal_size();
    let mut session = session_without_command(1, size);
    session.process_label = Some("x".repeat(200));
    let pane = pane_with_sessions(vec![session], size);

    let label = pane.terminal_session_label(&pane.sessions[0]);

    assert_eq!(label.chars().count(), 120);
    assert_eq!(label, format!("{} 1", "x".repeat(118)));
}

#[test]
fn terminal_tabs_title_template_controls_tab_label() {
    let size = test_terminal_size();
    let mut session = session_without_command(2, size);
    session.initial_cwd = Some(PathBuf::from("workspace/tools"));
    session.parser.process(b"\x1b]2;Task Runner\x07");
    let mut pane = pane_with_sessions(vec![session], size);

    pane.set_tabs_title_template("${process}:${sequence}:${cwd}:${workspaceFolderName}");

    assert_eq!(
        pane.terminal_session_label(&pane.sessions[0]),
        "Terminal:Task Runner:workspace/tools:workspace"
    );

    pane.set_tabs_allow_agent_cli_title(false);
    assert_eq!(
        pane.terminal_session_label(&pane.sessions[0]),
        "Terminal::workspace/tools:workspace"
    );
}

#[test]
fn terminal_tabs_title_template_sanitizes_parts_for_display() {
    let size = test_terminal_size();
    let mut session = session_without_command(1, size);
    let raw_cwd = PathBuf::from("workspace\n\u{202d}tools");
    let raw_workspace = PathBuf::from("workspace\rroot");
    session.process_label = Some("Proc\r\n\u{202e}Label".to_owned());
    session.parser.callbacks_mut().window_title = Some("Seq\t\u{2066}Title".to_owned());
    session.initial_cwd = Some(raw_cwd.clone());
    let mut pane = pane_with_sessions(vec![session], size);
    pane.cwd = raw_workspace.clone();

    pane.set_tabs_title_template("${process}:${sequence}:${cwd}:${workspaceFolderName}");

    assert_eq!(
        pane.terminal_session_label(&pane.sessions[0]),
        "Proc Label:Seq Title:workspace tools:workspace root"
    );
    assert_eq!(
        pane.sessions[0].initial_cwd.as_deref(),
        Some(raw_cwd.as_path())
    );
    assert_eq!(pane.cwd, raw_workspace);
}

#[test]
fn terminal_launch_cwd_display_labels_sanitize_path_text_without_rewriting_path() {
    let path = PathBuf::from("workspace")
        .join("nested")
        .join("bad\r\n\u{202e}cwd\t\u{2066}done");

    assert_eq!(terminal_compact_path_for_test(&path), "bad cwd done");
    assert_eq!(
        terminal_path_tooltip_for_test(&path),
        path.display()
            .to_string()
            .replace("\r\n", " ")
            .replace('\t', " ")
            .replace(['\u{202e}', '\u{2066}'], "")
    );
    assert!(path.as_os_str().to_string_lossy().contains('\u{202e}'));
}

#[test]
fn terminal_tabs_title_template_caps_rendered_label_for_display() {
    let size = test_terminal_size();
    let mut session = session_without_command(1, size);
    session.parser.callbacks_mut().window_title = Some("s".repeat(200));
    let mut pane = pane_with_sessions(vec![session], size);

    pane.set_tabs_title_template("prefix-${sequence}");

    let label = pane.terminal_session_label(&pane.sessions[0]);

    assert_eq!(label.chars().count(), 120);
    assert!(label.starts_with("prefix-"));
}
