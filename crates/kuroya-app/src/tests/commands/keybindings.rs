use crate::{
    KuroyaApp,
    app_startup_context::AppStartupContext,
    commands::keybinding_chord_for_command,
    keybinding_input::{CapturedKeybinding, capture_keybinding_event},
    keybinding_parse::{normalize_key_chord, parse_key_chord},
    keybindings::{
        assign_keybinding_chord, keybinding_items, keybinding_matches_query,
        remove_keybinding_assignment,
    },
    keybindings_panel_actions::PendingKeybindingsPanelActions,
    keybindings_runtime::malformed_keybinding_chord_rejection_reason,
    terminal::TerminalPane,
    ui_event_channel::ui_event_channel,
    workspace_state::settings_path,
};
use eframe::egui::{Event, Key, Modifiers};
use kuroya_core::{
    Command, EditorSettings, Workspace,
    keymap::{KeyBinding, Keymap},
};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tokio::runtime::Runtime;

#[test]
fn keybinding_query_matches_chord_or_command_label() {
    assert!(keybinding_matches_query(
        "Ctrl+Alt+K",
        "Keyboard Shortcuts",
        "keyboard"
    ));
    assert!(keybinding_matches_query(
        "Ctrl+Alt+K",
        "Keyboard Shortcuts",
        "alt+k"
    ));
    assert!(!keybinding_matches_query(
        "Ctrl+Alt+K",
        "Keyboard Shortcuts",
        "terminal"
    ));
}

#[test]
fn keybinding_query_matches_common_command_aliases() {
    let aliases = [
        ("Keyboard Shortcuts", "keybindings"),
        ("Navigate Back", "go back"),
        ("Search Terminal Output", "terminal find"),
        ("Next Terminal Search Result", "terminal find next"),
        ("Code Actions", "quick fix"),
        ("Accept Current Conflict", "accept ours"),
        ("Accept Incoming Conflict", "use theirs"),
        ("Accept Both Conflicts", "use both"),
    ];

    for (label, query) in aliases {
        assert!(
            keybinding_matches_query("", label, query),
            "{label:?} should match {query:?}"
        );
    }

    assert!(!keybinding_matches_query(
        "",
        "Keyboard Shortcuts",
        "terminal find"
    ));
}

#[test]
fn keybinding_query_matches_unassigned_and_preserves_ascii_only_folding() {
    assert!(keybinding_matches_query(
        "",
        "Reset Editor Split Widths",
        "UNASSIGNED"
    ));
    assert!(keybinding_matches_query(
        "",
        "R\u{00e9}sum\u{00e9}",
        "r\u{00e9}sum"
    ));
    assert!(!keybinding_matches_query(
        "",
        "R\u{00e9}sum\u{00e9}",
        "R\u{00c9}SUM"
    ));
}

#[test]
fn keybinding_items_include_unbound_bindable_commands() {
    let items = keybinding_items(&[KeyBinding {
        chord: "Ctrl+P".to_owned(),
        command: Command::ToggleQuickOpen,
    }]);

    assert!(
        items.iter().any(|(chord, command, _)| {
            command == &Command::ToggleQuickOpen && chord == "Ctrl+P"
        })
    );
    assert!(
        items
            .iter()
            .any(|(chord, command, _)| command == &Command::Undo && chord.is_empty())
    );
}

#[test]
fn bracket_keybindings_parse_and_capture_canonically() {
    let indent = parse_key_chord("Ctrl+]").expect("Ctrl+] should parse");
    assert_eq!(indent.logical_key, Key::CloseBracket);
    assert!(indent.modifiers.ctrl);

    let outdent = parse_key_chord("Ctrl+[").expect("Ctrl+[ should parse");
    assert_eq!(outdent.logical_key, Key::OpenBracket);
    assert!(outdent.modifiers.ctrl);

    let captured = capture_keybinding_event(&[Event::Key {
        key: Key::CloseBracket,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::CTRL,
    }]);
    assert_eq!(
        captured,
        Some(CapturedKeybinding::Chord("Ctrl+]".to_owned()))
    );
}

#[test]
fn keybinding_parser_normalizes_whitespace_casing_and_aliases() {
    assert_eq!(
        normalize_key_chord(" shift + control + p "),
        Some("Ctrl+Shift+P".to_owned())
    );
    assert_eq!(
        normalize_key_chord("OPTION + arrowleft"),
        Some("Alt+Left".to_owned())
    );
    assert_eq!(
        normalize_key_chord("command + slash"),
        Some("Cmd+/".to_owned())
    );
    assert_eq!(normalize_key_chord("Ctrl++P"), None);
}

#[test]
fn keybinding_parser_rejects_ambiguous_or_malformed_chords() {
    for chord in [
        "+P",
        "Ctrl++P",
        "Ctrl+Unknown+P",
        "Ctrl+P+Q",
        "Ctrl+Control+P",
        "Alt+Option+P",
        "Shift+Shift+P",
        "Cmd+Super+P",
        "Ctrl+\u{202e}P",
        "Ctrl+\u{2066}P",
        "Ctrl+Sh\tift+P",
    ] {
        assert!(
            parse_key_chord(chord).is_none(),
            "{chord:?} should not parse"
        );
    }

    let long_chord = format!("Ctrl+{}", "P".repeat(80));
    assert_eq!(parse_key_chord(&long_chord), None);
    assert_eq!(normalize_key_chord(&long_chord), None);
}

#[test]
fn default_keymap_chords_are_canonical_dispatchable_and_unique() {
    let keymap = Keymap::default();
    let mut seen_chords = HashSet::new();
    let mut seen_commands = Vec::new();

    for binding in &keymap.bindings {
        assert_eq!(
            normalize_key_chord(&binding.chord),
            Some(binding.chord.clone()),
            "{:?}",
            binding.command
        );
        assert!(
            parse_key_chord(&binding.chord).is_some(),
            "{:?}",
            binding.command
        );
        assert!(
            seen_chords.insert(binding.chord.clone()),
            "duplicate chord {}",
            binding.chord
        );
        assert!(
            !seen_commands
                .iter()
                .any(|command| command == &binding.command),
            "duplicate command {:?}",
            binding.command
        );
        seen_commands.push(binding.command.clone());
    }
}

#[test]
fn assigning_keybinding_rejects_conflicting_chord_without_mutating() {
    let mut bindings = vec![
        KeyBinding {
            chord: "Ctrl+P".to_owned(),
            command: Command::ToggleQuickOpen,
        },
        KeyBinding {
            chord: "Ctrl+F".to_owned(),
            command: Command::ToggleBufferFind,
        },
    ];

    let conflict = assign_keybinding_chord(&mut bindings, Command::ToggleBufferFind, "Ctrl+P");

    assert_eq!(conflict, Some(Command::ToggleQuickOpen));
    assert_eq!(
        keybinding_chord_for_command(&bindings, &Command::ToggleBufferFind),
        Some("Ctrl+F".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&bindings, &Command::ToggleQuickOpen),
        Some("Ctrl+P".to_owned())
    );
}

#[test]
fn assigning_keybinding_normalizes_chords_before_rejecting_conflict() {
    let mut bindings = vec![
        KeyBinding {
            chord: " control + p ".to_owned(),
            command: Command::ToggleQuickOpen,
        },
        KeyBinding {
            chord: "Ctrl+F".to_owned(),
            command: Command::ToggleBufferFind,
        },
    ];

    let conflict = assign_keybinding_chord(&mut bindings, Command::ToggleBufferFind, "CTRL + p");

    assert_eq!(conflict, Some(Command::ToggleQuickOpen));
    assert_eq!(
        keybinding_chord_for_command(&bindings, &Command::ToggleBufferFind),
        Some("Ctrl+F".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&bindings, &Command::ToggleQuickOpen),
        Some(" control + p ".to_owned())
    );
}

#[test]
fn removing_keybinding_assignment_removes_only_requested_command() {
    let mut bindings = vec![
        KeyBinding {
            chord: "Ctrl+P".to_owned(),
            command: Command::ToggleQuickOpen,
        },
        KeyBinding {
            chord: "Ctrl+F".to_owned(),
            command: Command::ToggleBufferFind,
        },
    ];

    assert!(remove_keybinding_assignment(
        &mut bindings,
        &Command::ToggleQuickOpen
    ));
    assert_eq!(
        keybinding_chord_for_command(&bindings, &Command::ToggleQuickOpen),
        None
    );
    assert_eq!(
        keybinding_chord_for_command(&bindings, &Command::ToggleBufferFind),
        Some("Ctrl+F".to_owned())
    );

    assert!(!remove_keybinding_assignment(&mut bindings, &Command::Undo));
    assert_eq!(bindings.len(), 1);
}

#[test]
fn captured_keybindings_are_canonical_and_safe() {
    let mut modifiers = Modifiers::CTRL;
    modifiers |= Modifiers::SHIFT;
    let captured = capture_keybinding_event(&[Event::Key {
        key: Key::P,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }]);
    assert_eq!(
        captured,
        Some(CapturedKeybinding::Chord("Ctrl+Shift+P".to_owned()))
    );

    let rejected = capture_keybinding_event(&[Event::Key {
        key: Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::SHIFT,
    }]);
    assert!(matches!(rejected, Some(CapturedKeybinding::Rejected(_))));

    let captured_escape = capture_keybinding_event(&[Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]);
    assert_eq!(captured_escape, Some(CapturedKeybinding::Escape));

    let captured_modified_escape = capture_keybinding_event(&[Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::CTRL,
    }]);
    assert_eq!(
        captured_modified_escape,
        Some(CapturedKeybinding::Chord("Ctrl+Escape".to_owned()))
    );
}

#[test]
fn keybinding_runtime_rejects_plain_text_or_malformed_assignments() {
    assert_eq!(
        malformed_keybinding_chord_rejection_reason("A"),
        Some("Use Ctrl, Alt, or Cmd with text shortcuts")
    );
    assert_eq!(
        malformed_keybinding_chord_rejection_reason("Ctrl+Unknown+P"),
        Some("That shortcut is not supported")
    );
    assert_eq!(malformed_keybinding_chord_rejection_reason("F3"), None);
    assert_eq!(malformed_keybinding_chord_rejection_reason("Ctrl+P"), None);

    let root = temp_root("keybinding-runtime-rejects-invalid");
    let mut app = app_for_keybindings_test(root.clone(), EditorSettings::default());
    let original_bindings = app.settings.keymap.bindings.clone();

    app.save_keybinding_chord(Command::Undo, "A".to_owned());

    assert_eq!(app.settings.keymap.bindings, original_bindings);
    assert_eq!(
        app.status,
        "Could not bind Undo: Use Ctrl, Alt, or Cmd with text shortcuts"
    );
    assert!(!settings_path(&root).exists());
}

#[test]
fn saving_keybinding_normalizes_chord_before_persisting() {
    let root = temp_root("keybinding-save-normalizes");
    let mut app = app_for_keybindings_test(root.clone(), EditorSettings::default());

    app.save_keybinding_chord(Command::Undo, " shift + control + z ".to_owned());

    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::Undo),
        Some("Ctrl+Shift+Z".to_owned())
    );
    assert_eq!(app.status, "Bound Undo to Ctrl+Shift+Z");
    let settings = fs::read_to_string(settings_path(&root)).expect("settings should be saved");
    assert!(settings.contains("chord = \"Ctrl+Shift+Z\""));
}

#[test]
fn saving_keybinding_rejects_conflicting_chord_without_persisting() {
    let root = temp_root("keybinding-save-rejects-conflict");
    let mut settings = EditorSettings::default();
    settings.keymap.bindings = vec![
        KeyBinding {
            chord: "Ctrl+P".to_owned(),
            command: Command::ToggleQuickOpen,
        },
        KeyBinding {
            chord: "Ctrl+Z".to_owned(),
            command: Command::Undo,
        },
    ];
    let mut app = app_for_keybindings_test(root.clone(), settings);
    let original_bindings = app.settings.keymap.bindings.clone();

    app.save_keybinding_chord(Command::Undo, "Ctrl+P".to_owned());

    assert_eq!(app.settings.keymap.bindings, original_bindings);
    assert_eq!(
        app.status,
        "Could not bind Undo to Ctrl+P: already used by Quick Open"
    );
    assert!(!settings_path(&root).exists());
}

#[test]
fn saving_keybinding_rolls_back_when_settings_save_fails() {
    let root = temp_root("keybinding-save-rollback");
    block_settings_directory(&root);
    let mut app = app_for_keybindings_test(root.clone(), EditorSettings::default());
    let original_bindings = app.settings.keymap.bindings.clone();

    app.save_keybinding_chord(Command::Undo, "Ctrl+Shift+Z".to_owned());

    assert_eq!(app.settings.keymap.bindings, original_bindings);
    assert!(app.status.starts_with("Could not save keybinding change: "));
    assert!(!settings_path(&root).exists());
    remove_root(&root);
}

#[test]
fn saving_keybinding_cleans_stale_duplicate_or_invalid_assignments() {
    let root = temp_root("keybinding-save-cleans-stale");
    let mut settings = EditorSettings::default();
    settings.keymap.bindings = vec![
        KeyBinding {
            chord: " control + p ".to_owned(),
            command: Command::ToggleQuickOpen,
        },
        KeyBinding {
            chord: "Ctrl+P".to_owned(),
            command: Command::ToggleBufferFind,
        },
        KeyBinding {
            chord: "Ctrl+Unknown+P".to_owned(),
            command: Command::ToggleTerminal,
        },
        KeyBinding {
            chord: "Z".to_owned(),
            command: Command::Redo,
        },
        KeyBinding {
            chord: "alt + arrowleft".to_owned(),
            command: Command::NavigateBack,
        },
        KeyBinding {
            chord: "Ctrl+Y".to_owned(),
            command: Command::Redo,
        },
        KeyBinding {
            chord: "Ctrl+Y".to_owned(),
            command: Command::Redo,
        },
    ];
    let mut app = app_for_keybindings_test(root.clone(), settings);

    app.save_keybinding_chord(Command::Undo, " ctrl + shift + z ".to_owned());

    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::Undo),
        Some("Ctrl+Shift+Z".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::ToggleQuickOpen),
        Some("Ctrl+P".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::NavigateBack),
        Some("Alt+Left".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::Redo),
        Some("Ctrl+Y".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::ToggleBufferFind),
        None
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::ToggleTerminal),
        None
    );
    assert_eq!(
        app.settings
            .keymap
            .bindings
            .iter()
            .filter(|binding| binding.command == Command::Redo)
            .count(),
        1
    );
    assert_eq!(
        app.status,
        "Bound Undo to Ctrl+Shift+Z; cleaned 4 stale shortcuts"
    );

    let settings = fs::read_to_string(settings_path(&root)).expect("settings should be saved");
    assert!(settings.contains("chord = \"Ctrl+P\""));
    assert!(settings.contains("chord = \"Alt+Left\""));
    assert!(settings.contains("chord = \"Ctrl+Y\""));
    assert!(settings.contains("chord = \"Ctrl+Shift+Z\""));
    assert!(!settings.contains("control + p"));
    assert!(!settings.contains("Ctrl+Unknown+P"));
    assert!(!settings.contains("chord = \"Z\""));
}

#[test]
fn removing_keybinding_prunes_stale_bindings_in_memory_and_saved_settings() {
    let root = temp_root("keybinding-remove-prunes-stale");
    let mut settings = EditorSettings::default();
    settings.keymap.bindings = vec![
        KeyBinding {
            chord: "Ctrl+Shift+Z".to_owned(),
            command: Command::Undo,
        },
        KeyBinding {
            chord: "Z".to_owned(),
            command: Command::ToggleTerminal,
        },
        KeyBinding {
            chord: "control + p".to_owned(),
            command: Command::ToggleQuickOpen,
        },
        KeyBinding {
            chord: "Ctrl+P".to_owned(),
            command: Command::ToggleBufferFind,
        },
        KeyBinding {
            chord: "Ctrl+Y".to_owned(),
            command: Command::Redo,
        },
        KeyBinding {
            chord: "Ctrl+Y".to_owned(),
            command: Command::NavigateForward,
        },
    ];
    let mut app = app_for_keybindings_test(root.clone(), settings);

    app.remove_keybinding_for_command(Command::Undo);

    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::Undo),
        None
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::ToggleQuickOpen),
        Some("Ctrl+P".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::Redo),
        Some("Ctrl+Y".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::ToggleTerminal),
        None
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::ToggleBufferFind),
        None
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::NavigateForward),
        None
    );
    assert_eq!(
        app.status,
        "Removed shortcut for Undo; cleaned 3 stale shortcuts"
    );

    let reloaded = EditorSettings::load_or_create_with_recovery(&settings_path(&root))
        .expect("settings should reload")
        .settings;
    assert_eq!(app.settings.keymap.bindings, reloaded.keymap.bindings);
}

#[test]
fn removing_keybinding_rolls_back_when_settings_save_fails() {
    let root = temp_root("keybinding-remove-rollback");
    block_settings_directory(&root);
    let mut settings = EditorSettings::default();
    settings.keymap.bindings = vec![KeyBinding {
        chord: "Ctrl+Shift+Z".to_owned(),
        command: Command::Undo,
    }];
    let mut app = app_for_keybindings_test(root.clone(), settings);
    let original_bindings = app.settings.keymap.bindings.clone();

    app.remove_keybinding_for_command(Command::Undo);

    assert_eq!(app.settings.keymap.bindings, original_bindings);
    assert!(
        app.status
            .starts_with("Could not save keybinding removal: ")
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::Undo),
        Some("Ctrl+Shift+Z".to_owned())
    );
    remove_root(&root);
}

#[test]
fn keybinding_panel_keeps_capture_active_after_invalid_chord() {
    let root = temp_root("keybinding-panel-invalid-capture");
    let mut app = app_for_keybindings_test(root.clone(), EditorSettings::default());
    let original_bindings = app.settings.keymap.bindings.clone();
    app.keybinding_capture_command = Some(Command::Undo);

    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Chord("A".to_owned())),
        ..PendingKeybindingsPanelActions::default()
    });

    assert_eq!(app.keybinding_capture_command, Some(Command::Undo));
    assert_eq!(app.settings.keymap.bindings, original_bindings);
    assert_eq!(app.status, "Use Ctrl, Alt, or Cmd with text shortcuts");
    assert!(!settings_path(&root).exists());
}

#[test]
fn keybinding_panel_cancel_capture_clears_state_and_status_without_saving() {
    let root = temp_root("keybinding-panel-cancel-capture");
    let mut app = app_for_keybindings_test(root.clone(), EditorSettings::default());
    let original_bindings = app.settings.keymap.bindings.clone();
    app.keybinding_capture_command = Some(Command::Undo);
    app.status = "Capturing shortcut; press keys, or press Esc twice to cancel".to_owned();

    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Cancel),
        ..PendingKeybindingsPanelActions::default()
    });

    assert_eq!(app.keybinding_capture_command, None);
    assert_eq!(app.settings.keymap.bindings, original_bindings);
    assert_eq!(app.status, "Canceled shortcut capture");
    assert!(!settings_path(&root).exists());
}

#[test]
fn keybinding_panel_first_escape_assigns_escape_and_keeps_capture_open() {
    let root = temp_root("keybinding-panel-first-escape-capture");
    let mut settings = EditorSettings::default();
    settings.keymap.bindings = vec![KeyBinding {
        chord: "Ctrl+Z".to_owned(),
        command: Command::Undo,
    }];
    let mut app = app_for_keybindings_test(root.clone(), settings);
    app.keybinding_capture_command = Some(Command::Undo);

    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Escape),
        ..PendingKeybindingsPanelActions::default()
    });

    assert_eq!(app.keybinding_capture_command, Some(Command::Undo));
    assert!(app.keybinding_escape_cancel.is_some());
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::Undo),
        Some("Escape".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::ToggleQuickOpen),
        None
    );
    assert_eq!(
        app.status,
        "Bound Undo to Escape; press Esc again to cancel"
    );
    let saved = fs::read_to_string(settings_path(&root)).expect("settings should be saved");
    assert!(saved.contains("chord = \"Escape\""));
}

#[test]
fn keybinding_panel_second_escape_restores_previous_bindings_and_cancels() {
    let root = temp_root("keybinding-panel-second-escape-cancels");
    let mut settings = EditorSettings::default();
    settings.keymap.bindings = vec![KeyBinding {
        chord: "Ctrl+Z".to_owned(),
        command: Command::Undo,
    }];
    let mut app = app_for_keybindings_test(root.clone(), settings);
    app.keybinding_capture_command = Some(Command::Undo);

    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Escape),
        ..PendingKeybindingsPanelActions::default()
    });
    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Escape),
        ..PendingKeybindingsPanelActions::default()
    });

    assert_eq!(app.keybinding_capture_command, None);
    assert!(app.keybinding_escape_cancel.is_none());
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::Undo),
        Some("Ctrl+Z".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::ToggleQuickOpen),
        None
    );
    assert_eq!(app.status, "Canceled shortcut capture");
    let saved = fs::read_to_string(settings_path(&root)).expect("settings should be saved");
    assert!(saved.contains("chord = \"Ctrl+Z\""));
    assert!(!saved.contains("chord = \"Escape\""));
}

#[test]
fn keybinding_panel_first_escape_rejects_conflict_and_keeps_capture_open() {
    let root = temp_root("keybinding-panel-first-escape-conflict");
    let mut settings = EditorSettings::default();
    settings.keymap.bindings = vec![
        KeyBinding {
            chord: "Escape".to_owned(),
            command: Command::ToggleQuickOpen,
        },
        KeyBinding {
            chord: "Ctrl+Z".to_owned(),
            command: Command::Undo,
        },
    ];
    let mut app = app_for_keybindings_test(root.clone(), settings);
    app.keybinding_capture_command = Some(Command::Undo);

    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Escape),
        ..PendingKeybindingsPanelActions::default()
    });

    assert_eq!(app.keybinding_capture_command, Some(Command::Undo));
    assert!(app.keybinding_escape_cancel.is_some());
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::Undo),
        Some("Ctrl+Z".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::ToggleQuickOpen),
        Some("Escape".to_owned())
    );
    assert_eq!(
        app.status,
        "Could not bind Undo to Escape: already used by Quick Open; press Esc again to cancel"
    );
    assert!(!settings_path(&root).exists());
}

#[test]
fn keybinding_panel_second_escape_after_conflict_cancels_without_saving() {
    let root = temp_root("keybinding-panel-second-escape-conflict-cancels");
    let mut settings = EditorSettings::default();
    settings.keymap.bindings = vec![
        KeyBinding {
            chord: "Escape".to_owned(),
            command: Command::ToggleQuickOpen,
        },
        KeyBinding {
            chord: "Ctrl+Z".to_owned(),
            command: Command::Undo,
        },
    ];
    let mut app = app_for_keybindings_test(root.clone(), settings);
    app.keybinding_capture_command = Some(Command::Undo);

    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Escape),
        ..PendingKeybindingsPanelActions::default()
    });
    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Escape),
        ..PendingKeybindingsPanelActions::default()
    });

    assert_eq!(app.keybinding_capture_command, None);
    assert!(app.keybinding_escape_cancel.is_none());
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::Undo),
        Some("Ctrl+Z".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::ToggleQuickOpen),
        Some("Escape".to_owned())
    );
    assert_eq!(app.status, "Canceled shortcut capture");
    assert!(!settings_path(&root).exists());
}

#[test]
fn keybinding_panel_cancel_after_escape_restores_previous_bindings() {
    let root = temp_root("keybinding-panel-cancel-after-escape-restores");
    let mut settings = EditorSettings::default();
    settings.keymap.bindings = vec![KeyBinding {
        chord: "Ctrl+Z".to_owned(),
        command: Command::Undo,
    }];
    let mut app = app_for_keybindings_test(root.clone(), settings);
    app.keybinding_capture_command = Some(Command::Undo);

    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Escape),
        ..PendingKeybindingsPanelActions::default()
    });
    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Cancel),
        ..PendingKeybindingsPanelActions::default()
    });

    assert_eq!(app.keybinding_capture_command, None);
    assert!(app.keybinding_escape_cancel.is_none());
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::Undo),
        Some("Ctrl+Z".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::ToggleQuickOpen),
        None
    );
    assert_eq!(app.status, "Canceled shortcut capture");
    let saved = fs::read_to_string(settings_path(&root)).expect("settings should be saved");
    assert!(saved.contains("chord = \"Ctrl+Z\""));
    assert!(!saved.contains("chord = \"Escape\""));
}

#[test]
fn keybinding_panel_chord_after_escape_restores_previous_bindings_before_saving() {
    let root = temp_root("keybinding-panel-chord-after-escape-restores");
    let mut settings = EditorSettings::default();
    settings.keymap.bindings = vec![KeyBinding {
        chord: "Ctrl+Z".to_owned(),
        command: Command::Undo,
    }];
    let mut app = app_for_keybindings_test(root.clone(), settings);
    app.keybinding_capture_command = Some(Command::Undo);

    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Escape),
        ..PendingKeybindingsPanelActions::default()
    });
    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Chord("Ctrl+K".to_owned())),
        ..PendingKeybindingsPanelActions::default()
    });

    assert_eq!(app.keybinding_capture_command, None);
    assert!(app.keybinding_escape_cancel.is_none());
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::Undo),
        Some("Ctrl+K".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::ToggleQuickOpen),
        None
    );
    assert_eq!(app.status, "Bound Undo to Ctrl+K");
    let saved = fs::read_to_string(settings_path(&root)).expect("settings should be saved");
    assert!(saved.contains("chord = \"Ctrl+K\""));
    assert!(!saved.contains("chord = \"Escape\""));
}

#[test]
fn keybinding_panel_rejected_after_escape_can_still_cancel_and_restore() {
    let root = temp_root("keybinding-panel-reject-after-escape-restores");
    let mut settings = EditorSettings::default();
    settings.keymap.bindings = vec![KeyBinding {
        chord: "Ctrl+Z".to_owned(),
        command: Command::Undo,
    }];
    let mut app = app_for_keybindings_test(root.clone(), settings);
    app.keybinding_capture_command = Some(Command::Undo);

    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Escape),
        ..PendingKeybindingsPanelActions::default()
    });
    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Rejected(
            "Use Ctrl, Alt, or Cmd with text shortcuts".to_owned(),
        )),
        ..PendingKeybindingsPanelActions::default()
    });

    assert_eq!(app.keybinding_capture_command, Some(Command::Undo));
    assert!(app.keybinding_escape_cancel.is_some());

    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Cancel),
        ..PendingKeybindingsPanelActions::default()
    });

    assert_eq!(app.keybinding_capture_command, None);
    assert!(app.keybinding_escape_cancel.is_none());
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::Undo),
        Some("Ctrl+Z".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::ToggleQuickOpen),
        None
    );
    assert_eq!(app.status, "Canceled shortcut capture");
}

#[test]
fn keybinding_panel_open_settings_after_escape_restores_previous_bindings() {
    let root = temp_root("keybinding-panel-open-settings-after-escape");
    let mut settings = EditorSettings::default();
    settings.keymap.bindings = vec![KeyBinding {
        chord: "Ctrl+Z".to_owned(),
        command: Command::Undo,
    }];
    let mut app = app_for_keybindings_test(root.clone(), settings);
    app.keybindings_open = true;
    app.keybinding_capture_command = Some(Command::Undo);
    let settings_path = settings_path(&root);

    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Escape),
        ..PendingKeybindingsPanelActions::default()
    });
    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        open_settings: true,
        ..PendingKeybindingsPanelActions::default()
    });

    assert!(!app.keybindings_open);
    assert_eq!(app.keybinding_capture_command, None);
    assert!(app.keybinding_escape_cancel.is_none());
    assert!(app.pending_open_paths.contains(&settings_path));
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::Undo),
        Some("Ctrl+Z".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::ToggleQuickOpen),
        None
    );
}

#[test]
fn keybinding_panel_toggle_command_after_escape_restores_previous_bindings() {
    let root = temp_root("keybinding-panel-toggle-after-escape");
    let mut settings = EditorSettings::default();
    settings.keymap.bindings = vec![KeyBinding {
        chord: "Ctrl+Z".to_owned(),
        command: Command::Undo,
    }];
    let mut app = app_for_keybindings_test(root, settings);
    app.keybindings_open = true;
    app.keybinding_capture_command = Some(Command::Undo);

    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Escape),
        ..PendingKeybindingsPanelActions::default()
    });

    assert!(app.run_ui_command(&Command::ToggleKeybindingsPanel));

    assert!(!app.keybindings_open);
    assert_eq!(app.keybinding_capture_command, None);
    assert!(app.keybinding_escape_cancel.is_none());
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::Undo),
        Some("Ctrl+Z".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::ToggleQuickOpen),
        None
    );
}

#[test]
fn keybinding_panel_second_escape_restores_pruned_bindings_after_stale_cleanup() {
    let root = temp_root("keybinding-panel-second-escape-prunes-stale");
    let mut settings = EditorSettings::default();
    settings.keymap.bindings = vec![
        KeyBinding {
            chord: "Ctrl+O".to_owned(),
            command: Command::ToggleQuickOpen,
        },
        KeyBinding {
            chord: "Ctrl+Z".to_owned(),
            command: Command::Undo,
        },
        KeyBinding {
            chord: "Z".to_owned(),
            command: Command::ToggleTerminal,
        },
        KeyBinding {
            chord: "control + p".to_owned(),
            command: Command::ToggleBufferFind,
        },
        KeyBinding {
            chord: "Ctrl+Y".to_owned(),
            command: Command::Redo,
        },
        KeyBinding {
            chord: "Ctrl+Y".to_owned(),
            command: Command::NavigateForward,
        },
    ];
    let mut app = app_for_keybindings_test(root.clone(), settings);
    app.keybinding_capture_command = Some(Command::Undo);

    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Escape),
        ..PendingKeybindingsPanelActions::default()
    });
    assert!(app.status.contains("cleaned 2 stale shortcuts"));

    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Escape),
        ..PendingKeybindingsPanelActions::default()
    });

    assert_eq!(app.keybinding_capture_command, None);
    assert!(app.keybinding_escape_cancel.is_none());
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::Undo),
        Some("Ctrl+Z".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::ToggleQuickOpen),
        Some("Ctrl+O".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::ToggleBufferFind),
        Some("Ctrl+P".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::Redo),
        Some("Ctrl+Y".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::ToggleTerminal),
        None
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::NavigateForward),
        None
    );
    assert_eq!(app.status, "Canceled shortcut capture");

    let reloaded = EditorSettings::load_or_create_with_recovery(&settings_path(&root))
        .expect("settings should reload")
        .settings;
    assert_eq!(app.settings.keymap.bindings, reloaded.keymap.bindings);
}

#[test]
fn keybinding_panel_escape_capture_finalizes_after_cancel_window() {
    let root = temp_root("keybinding-panel-escape-capture-expiry");
    let mut settings = EditorSettings::default();
    settings.keymap.bindings = vec![KeyBinding {
        chord: "Ctrl+Z".to_owned(),
        command: Command::Undo,
    }];
    let mut app = app_for_keybindings_test(root, settings);
    app.keybinding_capture_command = Some(Command::Undo);

    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Escape),
        ..PendingKeybindingsPanelActions::default()
    });
    assert!(app.status.ends_with("; press Esc again to cancel"));
    let deadline = app
        .keybinding_escape_cancel
        .as_ref()
        .expect("escape capture should wait for cancel")
        .deadline;

    assert!(app.finish_expired_keybinding_escape_capture(deadline));

    assert_eq!(app.keybinding_capture_command, None);
    assert!(app.keybinding_escape_cancel.is_none());
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::Undo),
        Some("Escape".to_owned())
    );
    assert_eq!(app.status, "Bound Undo to Escape");
}

#[test]
fn keybinding_panel_captured_chord_rejects_conflict_and_keeps_capture_open() {
    let root = temp_root("keybinding-panel-conflict-capture");
    let mut settings = EditorSettings::default();
    settings.keymap.bindings = vec![
        KeyBinding {
            chord: "Ctrl+P".to_owned(),
            command: Command::ToggleQuickOpen,
        },
        KeyBinding {
            chord: "Ctrl+Z".to_owned(),
            command: Command::Undo,
        },
    ];
    let mut app = app_for_keybindings_test(root.clone(), settings);
    app.keybinding_capture_command = Some(Command::Undo);

    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Chord("Ctrl+P".to_owned())),
        ..PendingKeybindingsPanelActions::default()
    });

    assert_eq!(app.keybinding_capture_command, Some(Command::Undo));
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::Undo),
        Some("Ctrl+Z".to_owned())
    );
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::ToggleQuickOpen),
        Some("Ctrl+P".to_owned())
    );
    assert_eq!(
        app.status,
        "Could not bind Undo to Ctrl+P: already used by Quick Open".to_owned()
    );
    assert!(!settings_path(&root).exists());
}

#[test]
fn keybinding_panel_captured_chord_persists_when_unbound() {
    let root = temp_root("keybinding-panel-valid-capture");
    let mut settings = EditorSettings::default();
    settings.keymap.bindings = vec![KeyBinding {
        chord: "Ctrl+Z".to_owned(),
        command: Command::Undo,
    }];
    let mut app = app_for_keybindings_test(root.clone(), settings);
    app.keybinding_capture_command = Some(Command::Undo);

    app.apply_keybindings_panel_actions(PendingKeybindingsPanelActions {
        captured: Some(CapturedKeybinding::Chord("Ctrl+K".to_owned())),
        ..PendingKeybindingsPanelActions::default()
    });

    assert_eq!(app.keybinding_capture_command, None);
    assert_eq!(
        keybinding_chord_for_command(&app.settings.keymap.bindings, &Command::Undo),
        Some("Ctrl+K".to_owned())
    );
    assert_eq!(app.status, "Bound Undo to Ctrl+K".to_owned());

    let saved = fs::read_to_string(settings_path(&root)).expect("settings should be saved");
    assert!(saved.contains("chord = \"Ctrl+K\""));
    assert!(!saved.contains("chord = \"Ctrl+Z\""));
    let reloaded = EditorSettings::load_or_create_with_recovery(&settings_path(&root))
        .expect("settings should reload")
        .settings;
    assert_eq!(
        keybinding_chord_for_command(&reloaded.keymap.bindings, &Command::Undo),
        Some("Ctrl+K".to_owned())
    );
    remove_root(&root);
}

fn app_for_keybindings_test(root: PathBuf, settings: EditorSettings) -> KuroyaApp {
    let (tx, rx) = ui_event_channel();
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

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("kuroya-{name}-{}-{nanos}", std::process::id()))
}

fn block_settings_directory(root: &Path) {
    fs::create_dir_all(root).unwrap();
    let settings = settings_path(root);
    fs::create_dir_all(settings.parent().unwrap().parent().unwrap()).unwrap();
    fs::write(settings.parent().unwrap(), "not a directory").unwrap();
}

fn remove_root(root: &Path) {
    match fs::remove_dir_all(root) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("failed to remove temp root: {error}"),
    }
}
