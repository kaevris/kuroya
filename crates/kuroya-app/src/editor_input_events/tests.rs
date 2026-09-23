use super::{
    ActiveSnippetEditSnapshot, EditorInputEvent, SnippetPostEditSnapshot,
    active_snippet_edit_snapshot_from_ranges, auto_indented_paste_text,
    editor_input_events_snapshot, editor_input_events_snapshot_with_vim_settings,
    editor_key_event_is_relevant_for_input_mode, editor_paste_transform_plan,
    editor_plain_text_key_event_is_redundant, editor_text_event_coalescing_allowed_for_mode,
    line_prefix_looks_inside_string, normalized_ime_preedit_text,
    paste_selector_visible_after_paste, paste_text_at_editor_cursors, reindent_multiline_paste,
    snippet_post_edit_snapshot, spread_paste_segments,
};
use crate::{
    KuroyaApp,
    app_startup_context::AppStartupContext,
    editor_input::normalized_editor_paste_text,
    editor_vim_key_events::EditorVimMode,
    terminal::{
        TerminalPane, terminal_input_id, terminal_rename_input_id, terminal_search_input_id,
    },
    transient_state::EditorImePreedit,
    ui_event_channel::ui_event_channel,
};
use eframe::egui::{Context, Event, ImeEvent, Key, Modifiers};
use kuroya_core::{
    Command, EditorMultiCursorPaste, EditorPasteAsShowPasteSelector, EditorSettings,
    EditorVimKeyOverride, EditorVimSettings, LspDocumentHighlight, Selection, TextBuffer,
    Workspace,
};
use std::{path::PathBuf, time::Instant};
use tokio::runtime::Runtime;

#[test]
fn editor_input_events_snapshot_keeps_only_editor_relevant_events() {
    let snapshot = editor_input_events_snapshot(
        &[
            Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            Event::Text("x".to_owned()),
            Event::Ime(ImeEvent::Preedit("wen".to_owned())),
            Event::Paste("clip".to_owned()),
            Event::Key {
                key: Key::Backspace,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
        ],
        false,
        EditorVimMode::Insert,
        None,
        false,
    );

    assert!(snapshot.includes_mutation);
    assert_eq!(
        snapshot.events,
        vec![
            EditorInputEvent::Text {
                text: "x".to_owned(),
                ime_commit: false,
            },
            EditorInputEvent::ImePreedit("wen".to_owned()),
            EditorInputEvent::Paste("clip".to_owned()),
            EditorInputEvent::Key {
                key: Key::Backspace,
                modifiers: Modifiers::NONE,
            },
        ]
    );
}

#[test]
fn cut_input_event_clears_stale_ime_preedit_when_buffer_changes() {
    let root = PathBuf::from("input-cut-ime-test");
    let mut app = app_for_input_test(root);
    let mut buffer = TextBuffer::from_text(1, None, "alpha beta".to_owned());
    buffer.set_selection(0, 5);
    app.buffers.push(buffer);
    app.panes[0].active = Some(1);
    app.active = Some(1);
    app.focused_pane = Some(1);
    app.ime_preedit = Some(EditorImePreedit {
        buffer_id: 1,
        text: "preedit".to_owned(),
    });

    let ctx = Context::default();
    ctx.input_mut(|input| input.events.push(Event::Cut));

    app.handle_editor_input(&ctx, 1, 1);

    assert!(app.ime_preedit.is_none());
    assert_eq!(
        app.buffer(1).map(TextBuffer::text),
        Some(" beta".to_owned())
    );
}

#[test]
fn normal_text_input_replaces_selection_and_leaves_no_highlighted_range() {
    let root = PathBuf::from("normal-input-selection-typing-test");
    let path = root.join("main.rs");
    let mut app = app_for_input_test(root);
    let mut buffer = TextBuffer::from_text(1, Some(path.clone()), "alpha beta".to_owned());
    buffer.set_selection(0, 5);
    app.settings.vim_keybindings = false;
    app.buffers.push(buffer);
    app.panes[0].active = Some(1);
    app.active = Some(1);
    app.focused_pane = Some(1);
    app.buffer_find_open = true;
    app.buffer_find_query = "alpha".to_owned();
    app.document_highlights_path = Some(path);
    app.document_highlights = vec![LspDocumentHighlight {
        line: 0,
        column: 0,
        end_line: 0,
        end_column: 5,
        kind: None,
    }];

    assert_eq!(app.find_matches_for_buffer_index(0), vec![0..5]);
    assert_eq!(app.buffer_find_cache.cached_buffer_ids_for_test(), vec![1]);
    app.buffer_find_open = false;

    let ctx = Context::default();
    ctx.input_mut(|input| input.events.push(Event::Text("x".to_owned())));

    app.handle_editor_input(&ctx, 1, 1);

    let buffer = app.buffer(1).expect("buffer remains loaded");
    assert_eq!(buffer.text(), "x beta");
    assert_eq!(buffer.selections(), &[Selection::caret(1)]);
    assert!(
        app.buffer_find_cache
            .cached_buffer_ids_for_test()
            .is_empty()
    );
    assert!(app.document_highlights_path.is_none());
    assert!(app.document_highlights.is_empty());

    let data = app.prepare_editor_pane_data(1, 0, 8.0, true, true);
    assert!(data.find_matches.is_empty());
    assert!(data.document_highlight_ranges.is_empty());
}

#[test]
fn editor_input_ignores_text_while_terminal_input_has_focus() {
    let root = PathBuf::from("terminal-focus-editor-input-test");
    let mut app = app_for_input_test(root);
    let buffer = TextBuffer::from_text(1, None, "alpha".to_owned());
    app.buffers.push(buffer);
    app.panes[0].active = Some(1);
    app.active = Some(1);
    app.focused_pane = Some(1);
    let _rx_command = app.terminal.add_process_session_for_test(7);

    let ctx = Context::default();
    ctx.memory_mut(|memory| memory.request_focus(terminal_input_id(7)));
    ctx.input_mut(|input| input.events.push(Event::Text("x".to_owned())));

    app.handle_editor_input(&ctx, 1, 1);

    assert_eq!(
        app.buffer(1).map(TextBuffer::text),
        Some("alpha".to_owned())
    );
    assert_eq!(app.focused_pane, Some(1));
}

#[test]
fn editor_input_still_accepts_text_when_terminal_visible_but_not_focused() {
    let root = PathBuf::from("terminal-visible-editor-input-test");
    let mut app = app_for_input_test(root);
    let mut buffer = TextBuffer::from_text(1, None, "alpha".to_owned());
    buffer.set_single_cursor(buffer.len_chars());
    app.buffers.push(buffer);
    app.panes[0].active = Some(1);
    app.active = Some(1);
    app.focused_pane = Some(1);
    let _rx_command = app.terminal.add_process_session_for_test(7);

    let ctx = Context::default();
    ctx.input_mut(|input| input.events.push(Event::Text("x".to_owned())));

    app.handle_editor_input(&ctx, 1, 1);

    assert_eq!(
        app.buffer(1).map(TextBuffer::text),
        Some("alphax".to_owned())
    );
}

#[test]
fn editor_input_ignores_text_while_terminal_search_has_focus() {
    let root = PathBuf::from("terminal-search-focus-editor-input-test");
    let mut app = app_for_input_test(root);
    let buffer = TextBuffer::from_text(1, None, "alpha".to_owned());
    app.buffers.push(buffer);
    app.panes[0].active = Some(1);
    app.active = Some(1);
    app.focused_pane = Some(1);
    let _rx_command = app.terminal.add_process_session_for_test(7);
    app.terminal.open_terminal_search();

    let ctx = Context::default();
    ctx.memory_mut(|memory| memory.request_focus(terminal_search_input_id()));
    ctx.input_mut(|input| input.events.push(Event::Text("x".to_owned())));

    app.handle_editor_input(&ctx, 1, 1);

    assert_eq!(
        app.buffer(1).map(TextBuffer::text),
        Some("alpha".to_owned())
    );
}

#[test]
fn editor_input_ignores_text_while_terminal_rename_has_focus() {
    let root = PathBuf::from("terminal-rename-focus-editor-input-test");
    let mut app = app_for_input_test(root);
    let buffer = TextBuffer::from_text(1, None, "alpha".to_owned());
    app.buffers.push(buffer);
    app.panes[0].active = Some(1);
    app.active = Some(1);
    app.focused_pane = Some(1);
    let _rx_command = app.terminal.add_process_session_for_test(7);
    app.terminal.begin_rename_session_for_test(0);

    let ctx = Context::default();
    ctx.memory_mut(|memory| memory.request_focus(terminal_rename_input_id(7)));
    ctx.input_mut(|input| input.events.push(Event::Text("x".to_owned())));

    app.handle_editor_input(&ctx, 1, 1);

    assert_eq!(
        app.buffer(1).map(TextBuffer::text),
        Some("alpha".to_owned())
    );
}

#[test]
fn vim_insert_text_collapses_stale_selection_before_typing() {
    let root = PathBuf::from("vim-insert-selection-typing-test");
    let mut app = app_for_input_test(root);
    let mut buffer = TextBuffer::from_text(1, None, "alpha beta".to_owned());
    buffer.set_selection(0, 5);
    app.settings.vim_keybindings = true;
    app.editor_vim_mode = EditorVimMode::Insert;
    app.buffers.push(buffer);
    app.panes[0].active = Some(1);
    app.active = Some(1);
    app.focused_pane = Some(1);

    let ctx = Context::default();
    ctx.input_mut(|input| input.events.push(Event::Text("x".to_owned())));

    app.handle_editor_input(&ctx, 1, 1);

    let buffer = app.buffer(1).expect("buffer remains loaded");
    assert_eq!(buffer.text(), "alphax beta");
    assert_eq!(buffer.selections(), &[Selection::caret(6)]);
}

#[test]
fn vim_insert_paste_collapses_stale_selection_before_inserting() {
    let root = PathBuf::from("vim-insert-selection-paste-test");
    let mut app = app_for_input_test(root);
    let mut buffer = TextBuffer::from_text(1, None, "alpha beta".to_owned());
    buffer.set_selection(0, 5);
    app.settings.vim_keybindings = true;
    app.editor_vim_mode = EditorVimMode::Insert;
    app.buffers.push(buffer);
    app.panes[0].active = Some(1);
    app.active = Some(1);
    app.focused_pane = Some(1);

    let ctx = Context::default();
    ctx.input_mut(|input| input.events.push(Event::Paste("XY".to_owned())));

    app.handle_editor_input(&ctx, 1, 1);

    let buffer = app.buffer(1).expect("buffer remains loaded");
    assert_eq!(buffer.text(), "alphaXY beta");
    assert_eq!(buffer.selections(), &[Selection::caret(7)]);
}

#[test]
fn vim_command_override_from_settings_reaches_command_bus() {
    let root = PathBuf::from("vim-command-override-input-test");
    let mut app = app_for_input_test(root);
    let mut buffer = TextBuffer::from_text(1, None, "alpha".to_owned());
    buffer.set_single_cursor(1);
    app.settings.vim_keybindings = true;
    app.settings.vim.key_overrides = vec![EditorVimKeyOverride {
        before: "K".to_owned(),
        after: String::new(),
        command: Some(Command::RequestHover),
    }];
    app.editor_vim_mode = EditorVimMode::Normal;
    app.buffers.push(buffer);
    app.panes[0].active = Some(1);
    app.active = Some(1);
    app.focused_pane = Some(1);

    let ctx = Context::default();
    ctx.input_mut(|input| {
        input.events.push(Event::Key {
            key: Key::K,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::SHIFT,
        });
    });

    app.handle_editor_input(&ctx, 1, 1);

    let buffer = app.buffer(1).expect("buffer remains loaded");
    assert_eq!(buffer.text(), "alpha");
    assert_eq!(buffer.cursor(), 1);
    assert_eq!(app.command_bus.pop(), Some(Command::RequestHover));
    assert_eq!(app.command_bus.pop(), None);
}

#[test]
fn editor_input_events_snapshot_uses_filtered_plain_events_for_mutation() {
    let snapshot = editor_input_events_snapshot(
        &[
            Event::Text("\u{0}".to_owned()),
            Event::Paste("\u{0}\u{1b}\u{7f}".to_owned()),
            Event::Copy,
        ],
        false,
        EditorVimMode::Insert,
        None,
        false,
    );

    assert!(!snapshot.includes_mutation);
    assert_eq!(snapshot.events, vec![EditorInputEvent::Copy]);
}

#[test]
fn vim_input_events_snapshot_preserves_raw_mutation_semantics() {
    let snapshot = editor_input_events_snapshot(
        &[
            Event::Text("\u{0}".to_owned()),
            Event::Paste("\u{0}".to_owned()),
        ],
        true,
        EditorVimMode::Insert,
        None,
        false,
    );

    assert!(snapshot.includes_mutation);
    assert!(snapshot.events.is_empty());
}

#[test]
fn vim_input_events_snapshot_respects_settings_for_mutation_preflight() {
    let disabled_x = EditorVimSettings {
        disabled_bindings: vec!["x".to_owned()],
        ..EditorVimSettings::default()
    };
    let disabled_snapshot = editor_input_events_snapshot_with_vim_settings(
        &[
            Event::Key {
                key: Key::X,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            Event::Text("x".to_owned()),
        ],
        true,
        EditorVimMode::Normal,
        None,
        false,
        &disabled_x,
    );

    assert!(!disabled_snapshot.includes_mutation);

    let remap_k_to_x = EditorVimSettings {
        key_overrides: vec![EditorVimKeyOverride {
            before: "K".to_owned(),
            after: "x".to_owned(),
            command: None,
        }],
        ..EditorVimSettings::default()
    };
    let remapped_snapshot = editor_input_events_snapshot_with_vim_settings(
        &[
            Event::Key {
                key: Key::K,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::SHIFT,
            },
            Event::Text("K".to_owned()),
        ],
        true,
        EditorVimMode::Normal,
        None,
        false,
        &remap_k_to_x,
    );

    assert!(remapped_snapshot.includes_mutation);

    let disabled_w = EditorVimSettings {
        disabled_bindings: vec!["w".to_owned()],
        ..EditorVimSettings::default()
    };
    let disabled_pending_snapshot = editor_input_events_snapshot_with_vim_settings(
        &[
            Event::Key {
                key: Key::D,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            Event::Text("d".to_owned()),
            Event::Key {
                key: Key::W,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            Event::Text("w".to_owned()),
        ],
        true,
        EditorVimMode::Normal,
        None,
        false,
        &disabled_w,
    );

    assert!(!disabled_pending_snapshot.includes_mutation);

    let remap_w_to_l = EditorVimSettings {
        key_overrides: vec![EditorVimKeyOverride {
            before: "w".to_owned(),
            after: "l".to_owned(),
            command: None,
        }],
        ..EditorVimSettings::default()
    };
    let remapped_pending_snapshot = editor_input_events_snapshot_with_vim_settings(
        &[
            Event::Key {
                key: Key::D,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            Event::Text("d".to_owned()),
            Event::Key {
                key: Key::W,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            Event::Text("w".to_owned()),
        ],
        true,
        EditorVimMode::Normal,
        None,
        false,
        &remap_w_to_l,
    );

    assert!(remapped_pending_snapshot.includes_mutation);
}

#[test]
fn vim_command_override_mutation_preflight_tracks_editor_edit_commands() {
    let hover = EditorVimSettings {
        key_overrides: vec![EditorVimKeyOverride {
            before: "K".to_owned(),
            after: String::new(),
            command: Some(Command::RequestHover),
        }],
        ..EditorVimSettings::default()
    };
    let hover_snapshot = editor_input_events_snapshot_with_vim_settings(
        &[Event::Key {
            key: Key::K,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::SHIFT,
        }],
        true,
        EditorVimMode::Normal,
        None,
        false,
        &hover,
    );

    assert!(!hover_snapshot.includes_mutation);

    let delete_lines = EditorVimSettings {
        key_overrides: vec![EditorVimKeyOverride {
            before: "K".to_owned(),
            after: String::new(),
            command: Some(Command::DeleteLines),
        }],
        ..EditorVimSettings::default()
    };
    let delete_snapshot = editor_input_events_snapshot_with_vim_settings(
        &[Event::Key {
            key: Key::K,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::SHIFT,
        }],
        true,
        EditorVimMode::Normal,
        None,
        false,
        &delete_lines,
    );

    assert!(delete_snapshot.includes_mutation);
}

#[test]
fn vim_insert_escape_settings_remap_counts_as_mutation_when_target_mutates() {
    let remap_escape_to_x = EditorVimSettings {
        key_overrides: vec![EditorVimKeyOverride {
            before: "<Esc>".to_owned(),
            after: "x".to_owned(),
            command: None,
        }],
        ..EditorVimSettings::default()
    };
    let remapped_snapshot = editor_input_events_snapshot_with_vim_settings(
        &[Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        true,
        EditorVimMode::Insert,
        None,
        false,
        &remap_escape_to_x,
    );

    assert!(remapped_snapshot.includes_mutation);

    let disabled_escape = EditorVimSettings {
        disabled_bindings: vec!["<Esc>".to_owned()],
        ..EditorVimSettings::default()
    };
    let disabled_snapshot = editor_input_events_snapshot_with_vim_settings(
        &[Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        true,
        EditorVimMode::Insert,
        None,
        false,
        &disabled_escape,
    );

    assert!(!disabled_snapshot.includes_mutation);
}

#[test]
fn editor_input_events_snapshot_classifies_key_mutation_after_filtering() {
    let navigation = editor_input_events_snapshot(
        &[
            Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            Event::Key {
                key: Key::ArrowRight,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
        ],
        false,
        EditorVimMode::Insert,
        None,
        false,
    );

    assert!(!navigation.includes_mutation);
    assert_eq!(
        navigation.events,
        vec![EditorInputEvent::Key {
            key: Key::ArrowRight,
            modifiers: Modifiers::NONE,
        }]
    );

    let undo = editor_input_events_snapshot(
        &[Event::Key {
            key: Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::CTRL,
        }],
        false,
        EditorVimMode::Insert,
        None,
        false,
    );

    assert!(undo.includes_mutation);
    assert_eq!(
        undo.events,
        vec![EditorInputEvent::Key {
            key: Key::Z,
            modifiers: Modifiers::CTRL,
        }]
    );
}

#[test]
fn editor_input_events_snapshot_sanitizes_paste_events() {
    let snapshot = editor_input_events_snapshot(
        &[Event::Paste("a\u{0}b\u{1b}c".to_owned())],
        false,
        EditorVimMode::Insert,
        None,
        false,
    );

    assert!(snapshot.includes_mutation);
    assert_eq!(
        snapshot.events,
        vec![EditorInputEvent::Paste("abc".to_owned())]
    );
}

#[test]
fn editor_input_events_snapshot_sanitizes_ime_preedit_events() {
    let snapshot = editor_input_events_snapshot(
        &[
            Event::Ime(ImeEvent::Preedit("w\nen".to_owned())),
            Event::Ime(ImeEvent::Preedit("\n\t".to_owned())),
        ],
        false,
        EditorVimMode::Insert,
        None,
        false,
    );

    assert_eq!(
        snapshot.events,
        vec![
            EditorInputEvent::ImePreedit("wen".to_owned()),
            EditorInputEvent::ImeClearPreedit,
        ]
    );
}

#[test]
fn editor_input_events_snapshot_keeps_printable_keys_for_vim() {
    let snapshot = editor_input_events_snapshot(
        &[
            Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            Event::Text("a".to_owned()),
        ],
        true,
        EditorVimMode::Normal,
        None,
        false,
    );

    assert_eq!(
        snapshot.events,
        vec![
            EditorInputEvent::Key {
                key: Key::A,
                modifiers: Modifiers::NONE,
            },
            EditorInputEvent::Text {
                text: "a".to_owned(),
                ime_commit: false,
            },
        ]
    );
}

#[test]
fn editor_input_events_snapshot_filters_redundant_printable_keys_in_vim_insert_mode() {
    let snapshot = editor_input_events_snapshot(
        &[
            Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            Event::Text("a".to_owned()),
            Event::Key {
                key: Key::B,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::SHIFT,
            },
            Event::Text("B".to_owned()),
            Event::Key {
                key: Key::Backspace,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
        ],
        true,
        EditorVimMode::Insert,
        None,
        false,
    );

    assert_eq!(
        snapshot.events,
        vec![
            EditorInputEvent::Text {
                text: "a".to_owned(),
                ime_commit: false,
            },
            EditorInputEvent::Text {
                text: "B".to_owned(),
                ime_commit: false,
            },
            EditorInputEvent::Key {
                key: Key::Backspace,
                modifiers: Modifiers::NONE,
            },
        ]
    );
}

#[test]
fn vim_insert_text_events_can_coalesce_after_printable_keys_are_filtered() {
    let snapshot = editor_input_events_snapshot(
        &[
            Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            Event::Text("a".to_owned()),
            Event::Key {
                key: Key::B,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            Event::Text("b".to_owned()),
        ],
        true,
        EditorVimMode::Insert,
        None,
        true,
    );

    assert_eq!(
        snapshot.events,
        vec![EditorInputEvent::Text {
            text: "ab".to_owned(),
            ime_commit: false,
        }]
    );
}

#[test]
fn text_events_coalesce_across_filtered_plain_events() {
    let snapshot = editor_input_events_snapshot(
        &[
            Event::Text("alpha".to_owned()),
            Event::Key {
                key: Key::B,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            Event::Text("_beta".to_owned()),
            Event::Text("\u{0}".to_owned()),
            Event::Text("9".to_owned()),
        ],
        false,
        EditorVimMode::Insert,
        None,
        true,
    );

    assert_eq!(
        snapshot.events,
        vec![EditorInputEvent::Text {
            text: "alpha_beta9".to_owned(),
            ime_commit: false,
        }]
    );
}

#[test]
fn text_event_coalescing_keeps_unicode_alphanumeric_runs() {
    let snapshot = editor_input_events_snapshot(
        &[
            Event::Text("\u{e5}".to_owned()),
            Event::Text("\u{3b2}".to_owned()),
            Event::Text(":".to_owned()),
        ],
        false,
        EditorVimMode::Insert,
        None,
        true,
    );

    assert_eq!(
        snapshot.events,
        vec![EditorInputEvent::Text {
            text: "\u{e5}\u{3b2}:".to_owned(),
            ime_commit: false,
        }]
    );
}

#[test]
fn plain_text_key_events_are_redundant_only_without_editor_modifiers() {
    assert!(editor_plain_text_key_event_is_redundant(
        Key::A,
        Modifiers::NONE
    ));
    assert!(editor_plain_text_key_event_is_redundant(
        Key::A,
        Modifiers::SHIFT
    ));
    assert!(!editor_plain_text_key_event_is_redundant(
        Key::A,
        Modifiers::CTRL
    ));
    assert!(!editor_plain_text_key_event_is_redundant(
        Key::ArrowRight,
        Modifiers::NONE
    ));
    assert!(!editor_plain_text_key_event_is_redundant(
        Key::Enter,
        Modifiers::NONE
    ));
}

#[test]
fn logical_printable_punctuation_key_events_are_redundant() {
    for key in [
        Key::Colon,
        Key::Plus,
        Key::Pipe,
        Key::Questionmark,
        Key::Exclamationmark,
        Key::OpenCurlyBracket,
        Key::CloseCurlyBracket,
    ] {
        assert!(editor_plain_text_key_event_is_redundant(
            key,
            Modifiers::NONE
        ));
        assert!(editor_plain_text_key_event_is_redundant(
            key,
            Modifiers::SHIFT
        ));
        assert!(!editor_plain_text_key_event_is_redundant(
            key,
            Modifiers::CTRL
        ));
    }
}

#[test]
fn editor_key_events_keep_all_printable_keys_only_for_vim_normal_mode() {
    assert!(editor_key_event_is_relevant_for_input_mode(
        Key::A,
        Modifiers::NONE,
        true,
        EditorVimMode::Normal
    ));
    assert!(!editor_key_event_is_relevant_for_input_mode(
        Key::A,
        Modifiers::NONE,
        true,
        EditorVimMode::Insert
    ));
    assert!(editor_key_event_is_relevant_for_input_mode(
        Key::A,
        Modifiers::CTRL,
        true,
        EditorVimMode::Insert
    ));
}

#[test]
fn text_event_coalescing_allows_vim_insert_mode_but_not_normal_mode() {
    assert!(editor_text_event_coalescing_allowed_for_mode(
        false,
        EditorVimMode::Normal
    ));
    assert!(editor_text_event_coalescing_allowed_for_mode(
        true,
        EditorVimMode::Insert
    ));
    assert!(!editor_text_event_coalescing_allowed_for_mode(
        true,
        EditorVimMode::Normal
    ));
}

#[test]
fn text_event_coalescing_respects_filtered_and_raw_event_boundaries() {
    let filtered_plain_key = editor_input_events_snapshot(
        &[
            Event::Text("a".to_owned()),
            Event::Key {
                key: Key::B,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            Event::Text("b".to_owned()),
        ],
        false,
        EditorVimMode::Insert,
        None,
        true,
    );
    assert_eq!(
        filtered_plain_key.events,
        vec![EditorInputEvent::Text {
            text: "ab".to_owned(),
            ime_commit: false,
        }]
    );

    let ignored_paste = editor_input_events_snapshot(
        &[
            Event::Text("a".to_owned()),
            Event::Paste("\u{0}\u{1b}".to_owned()),
            Event::Text("b".to_owned()),
        ],
        false,
        EditorVimMode::Insert,
        None,
        true,
    );
    assert_eq!(
        ignored_paste.events,
        vec![EditorInputEvent::Text {
            text: "ab".to_owned(),
            ime_commit: false,
        }]
    );

    let paste_boundary = editor_input_events_snapshot(
        &[
            Event::Text("a".to_owned()),
            Event::Paste("clip".to_owned()),
            Event::Text("b".to_owned()),
        ],
        false,
        EditorVimMode::Insert,
        None,
        true,
    );
    assert_eq!(
        paste_boundary.events,
        vec![
            EditorInputEvent::Text {
                text: "a".to_owned(),
                ime_commit: false,
            },
            EditorInputEvent::Paste("clip".to_owned()),
            EditorInputEvent::Text {
                text: "b".to_owned(),
                ime_commit: false,
            },
        ]
    );

    let key_boundary = editor_input_events_snapshot(
        &[
            Event::Text("a".to_owned()),
            Event::Key {
                key: Key::Backspace,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            Event::Text("b".to_owned()),
        ],
        false,
        EditorVimMode::Insert,
        None,
        true,
    );
    assert_eq!(
        key_boundary.events,
        vec![
            EditorInputEvent::Text {
                text: "a".to_owned(),
                ime_commit: false,
            },
            EditorInputEvent::Key {
                key: Key::Backspace,
                modifiers: Modifiers::NONE,
            },
            EditorInputEvent::Text {
                text: "b".to_owned(),
                ime_commit: false,
            },
        ]
    );

    let ime_commit_boundary = editor_input_events_snapshot(
        &[
            Event::Text("a".to_owned()),
            Event::Ime(ImeEvent::Commit("b".to_owned())),
        ],
        false,
        EditorVimMode::Insert,
        None,
        true,
    );
    assert_eq!(
        ime_commit_boundary.events,
        vec![
            EditorInputEvent::Text {
                text: "a".to_owned(),
                ime_commit: false,
            },
            EditorInputEvent::Text {
                text: "b".to_owned(),
                ime_commit: true,
            },
        ]
    );
}

#[test]
fn text_event_coalescing_merges_fast_path_runs_only() {
    let snapshot = editor_input_events_snapshot(
        &[
            Event::Text("a".to_owned()),
            Event::Text("b1".to_owned()),
            Event::Text(".".to_owned()),
            Event::Text("c".to_owned()),
            Event::Text("::".to_owned()),
            Event::Text("D".to_owned()),
            Event::Ime(ImeEvent::Commit("d".to_owned())),
            Event::Text("e".to_owned()),
            Event::Text("(".to_owned()),
            Event::Text("f".to_owned()),
            Event::Text("g".to_owned()),
        ],
        false,
        EditorVimMode::Insert,
        None,
        true,
    );

    assert_eq!(
        snapshot.events,
        vec![
            EditorInputEvent::Text {
                text: "ab1.c::D".to_owned(),
                ime_commit: false,
            },
            EditorInputEvent::Text {
                text: "d".to_owned(),
                ime_commit: true,
            },
            EditorInputEvent::Text {
                text: "e".to_owned(),
                ime_commit: false,
            },
            EditorInputEvent::Text {
                text: "(".to_owned(),
                ime_commit: false,
            },
            EditorInputEvent::Text {
                text: "fg".to_owned(),
                ime_commit: false,
            },
        ]
    );
}

#[test]
fn editor_input_events_snapshot_suppresses_exact_paste_text_echo() {
    let snapshot = editor_input_events_snapshot(
        &[
            Event::Paste("clip".to_owned()),
            Event::Text("clip".to_owned()),
            Event::Text("tail".to_owned()),
        ],
        false,
        EditorVimMode::Insert,
        None,
        true,
    );

    assert!(snapshot.includes_mutation);
    assert_eq!(
        snapshot.events,
        vec![
            EditorInputEvent::Paste("clip".to_owned()),
            EditorInputEvent::Text {
                text: "tail".to_owned(),
                ime_commit: false,
            },
        ]
    );
}

#[test]
fn editor_input_events_snapshot_keeps_text_after_filtered_plain_key() {
    let paste_then_type = editor_input_events_snapshot(
        &[
            Event::Paste("a".to_owned()),
            Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            Event::Text("a".to_owned()),
        ],
        false,
        EditorVimMode::Insert,
        None,
        true,
    );

    assert_eq!(
        paste_then_type.events,
        vec![
            EditorInputEvent::Paste("a".to_owned()),
            EditorInputEvent::Text {
                text: "a".to_owned(),
                ime_commit: false,
            },
        ]
    );

    let commit_then_type = editor_input_events_snapshot(
        &[
            Event::Ime(ImeEvent::Commit("a".to_owned())),
            Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            Event::Text("a".to_owned()),
        ],
        false,
        EditorVimMode::Insert,
        None,
        true,
    );

    assert_eq!(
        commit_then_type.events,
        vec![
            EditorInputEvent::Text {
                text: "a".to_owned(),
                ime_commit: true,
            },
            EditorInputEvent::Text {
                text: "a".to_owned(),
                ime_commit: false,
            },
        ]
    );
}

#[test]
fn editor_input_events_snapshot_preserves_paste_echo_after_ignored_text() {
    let snapshot = editor_input_events_snapshot(
        &[
            Event::Paste("clip".to_owned()),
            Event::Text("\u{0}".to_owned()),
            Event::Text("clip".to_owned()),
            Event::Text("tail".to_owned()),
        ],
        false,
        EditorVimMode::Insert,
        None,
        true,
    );

    assert!(snapshot.includes_mutation);
    assert_eq!(
        snapshot.events,
        vec![
            EditorInputEvent::Paste("clip".to_owned()),
            EditorInputEvent::Text {
                text: "tail".to_owned(),
                ime_commit: false,
            },
        ]
    );
}

#[test]
fn editor_input_events_snapshot_keeps_distinct_text_after_paste() {
    let snapshot = editor_input_events_snapshot(
        &[
            Event::Paste("clip".to_owned()),
            Event::Text("clipped".to_owned()),
        ],
        false,
        EditorVimMode::Insert,
        None,
        true,
    );

    assert_eq!(
        snapshot.events,
        vec![
            EditorInputEvent::Paste("clip".to_owned()),
            EditorInputEvent::Text {
                text: "clipped".to_owned(),
                ime_commit: false,
            },
        ]
    );
}

#[test]
fn editor_input_events_snapshot_suppresses_exact_ime_commit_text_echo() {
    let snapshot = editor_input_events_snapshot(
        &[
            Event::Ime(ImeEvent::Preedit("wen".to_owned())),
            Event::Ime(ImeEvent::Commit("\u{6587}".to_owned())),
            Event::Text("\u{6587}".to_owned()),
        ],
        false,
        EditorVimMode::Insert,
        None,
        true,
    );

    assert!(snapshot.includes_mutation);
    assert_eq!(
        snapshot.events,
        vec![
            EditorInputEvent::ImePreedit("wen".to_owned()),
            EditorInputEvent::Text {
                text: "\u{6587}".to_owned(),
                ime_commit: true,
            },
        ]
    );
}

#[test]
fn editor_input_events_snapshot_sanitizes_ime_commit_and_suppresses_echo() {
    let snapshot = editor_input_events_snapshot(
        &[
            Event::Ime(ImeEvent::Preedit("wenzi".to_owned())),
            Event::Ime(ImeEvent::Commit("\u{6587}\u{0}\u{85}\u{5b57}".to_owned())),
            Event::Text("\u{6587}\u{5b57}".to_owned()),
        ],
        false,
        EditorVimMode::Insert,
        None,
        true,
    );

    assert!(snapshot.includes_mutation);
    assert_eq!(
        snapshot.events,
        vec![
            EditorInputEvent::ImePreedit("wenzi".to_owned()),
            EditorInputEvent::Text {
                text: "\u{6587}\u{5b57}".to_owned(),
                ime_commit: true,
            },
        ]
    );
}

#[test]
fn editor_input_events_snapshot_keeps_distinct_text_after_ime_commit() {
    let snapshot = editor_input_events_snapshot(
        &[
            Event::Ime(ImeEvent::Commit("\u{6587}".to_owned())),
            Event::Text("a".to_owned()),
        ],
        false,
        EditorVimMode::Insert,
        None,
        true,
    );

    assert_eq!(
        snapshot.events,
        vec![
            EditorInputEvent::Text {
                text: "\u{6587}".to_owned(),
                ime_commit: true,
            },
            EditorInputEvent::Text {
                text: "a".to_owned(),
                ime_commit: false,
            },
        ]
    );
}

#[test]
fn active_snippet_edit_snapshot_captures_ranges_and_buffer_length() {
    let buffer = TextBuffer::from_text(1, None, "abc".to_owned());
    let first_range = 0..0;
    let second_range = 1..1;

    assert_eq!(
        active_snippet_edit_snapshot_from_ranges(&buffer, std::slice::from_ref(&first_range)),
        Some(ActiveSnippetEditSnapshot {
            ranges: std::iter::once(first_range).collect(),
            before_len: 3,
        })
    );
    assert_eq!(
        active_snippet_edit_snapshot_from_ranges(&buffer, std::slice::from_ref(&second_range)),
        None
    );
}

#[test]
fn snippet_post_edit_snapshot_captures_ranges_length_and_cursor() {
    let mut buffer = TextBuffer::from_text(1, None, "abcd".to_owned());
    buffer.set_cursors([1, 3]);

    assert_eq!(
        snippet_post_edit_snapshot(&buffer),
        SnippetPostEditSnapshot {
            ranges: vec![1..1, 3..3],
            after_len: 4,
            cursor: 3,
        }
    );
}

#[test]
fn spread_paste_segments_match_cursor_count_and_trim_final_newline() {
    assert_eq!(
        spread_paste_segments("one\r\ntwo\r\n", 2),
        Some(vec!["one".to_owned(), "two".to_owned()])
    );
    assert_eq!(spread_paste_segments("one\ntwo\nthree", 2), None);
    assert_eq!(spread_paste_segments("one\ntwo\nthree\n", 2), None);
    assert_eq!(spread_paste_segments("one\ntwo", 1), None);
}

#[test]
fn paste_text_at_editor_cursors_respects_spread_and_full_modes() {
    let mut spread = TextBuffer::from_text(1, None, "abcd".to_owned());
    spread.set_cursors([1, 3]);
    assert!(paste_text_at_editor_cursors(
        &mut spread,
        "X\nYY",
        EditorMultiCursorPaste::Spread,
        false,
        true
    ));
    assert_eq!(spread.text(), "aXbcYYd");

    let mut full = TextBuffer::from_text(1, None, "abcd".to_owned());
    full.set_cursors([1, 3]);
    assert!(paste_text_at_editor_cursors(
        &mut full,
        "X\nYY",
        EditorMultiCursorPaste::Full,
        false,
        true
    ));
    assert_eq!(full.text(), "aX\nYYbcX\nYYd");
}

#[test]
fn paste_text_at_editor_cursors_uses_full_paste_when_spread_segments_do_not_match() {
    let mut buffer = TextBuffer::from_text(1, None, "abcd".to_owned());
    buffer.set_cursors([1, 3]);

    assert!(paste_text_at_editor_cursors(
        &mut buffer,
        "X\nYY\nZZ",
        EditorMultiCursorPaste::Spread,
        false,
        true
    ));

    assert_eq!(buffer.text(), "aX\nYY\nZZbcX\nYY\nZZd");
}

#[test]
fn paste_text_at_editor_cursors_strips_controls_before_spread() {
    let mut buffer = TextBuffer::from_text(1, None, "abcd".to_owned());
    buffer.set_cursors([1, 3]);

    assert!(paste_text_at_editor_cursors(
        &mut buffer,
        "X\u{1b}\nYY\u{0}",
        EditorMultiCursorPaste::Spread,
        false,
        true
    ));

    assert_eq!(buffer.text(), "aXbcYYd");
}

#[test]
fn paste_text_at_editor_cursors_ignores_control_only_paste() {
    let mut buffer = TextBuffer::from_text(1, None, "abcd".to_owned());
    buffer.set_cursors([1, 3]);

    assert!(!paste_text_at_editor_cursors(
        &mut buffer,
        "\u{0}\u{1b}\u{7f}",
        EditorMultiCursorPaste::Full,
        false,
        true
    ));
    assert_eq!(buffer.text(), "abcd");
}

#[test]
fn normalized_editor_paste_text_keeps_text_layout_controls() {
    assert_eq!(
        normalized_editor_paste_text("a\u{0}b\tc\r\nd\u{1b}").as_deref(),
        Some("ab\tc\r\nd")
    );
    assert_eq!(normalized_editor_paste_text("\u{0}\u{1b}\u{7f}"), None);
}

#[test]
fn paste_as_disabled_bypasses_paste_transformations() {
    let plan =
        editor_paste_transform_plan(false, EditorMultiCursorPaste::Spread, true, false, true);

    assert_eq!(plan.multi_cursor_paste, EditorMultiCursorPaste::Full);
    assert!(!plan.auto_indent_on_paste);
    assert!(!plan.auto_indent_on_paste_within_string);
    assert!(!plan.format_on_paste);
    assert!(!paste_selector_visible_after_paste(
        false,
        EditorPasteAsShowPasteSelector::AfterPaste,
        plan,
        2,
        "one\ntwo"
    ));
}

#[test]
fn paste_selector_only_shows_when_transform_choices_are_available() {
    let plain = editor_paste_transform_plan(true, EditorMultiCursorPaste::Full, false, true, false);
    assert!(!paste_selector_visible_after_paste(
        true,
        EditorPasteAsShowPasteSelector::AfterPaste,
        plain,
        1,
        "plain"
    ));

    let spread =
        editor_paste_transform_plan(true, EditorMultiCursorPaste::Spread, false, true, false);
    assert!(paste_selector_visible_after_paste(
        true,
        EditorPasteAsShowPasteSelector::AfterPaste,
        spread,
        2,
        "one\ntwo"
    ));
    assert!(!paste_selector_visible_after_paste(
        true,
        EditorPasteAsShowPasteSelector::Never,
        spread,
        2,
        "one\ntwo"
    ));

    let auto_indent =
        editor_paste_transform_plan(true, EditorMultiCursorPaste::Full, true, true, false);
    assert!(paste_selector_visible_after_paste(
        true,
        EditorPasteAsShowPasteSelector::AfterPaste,
        auto_indent,
        1,
        "one\ntwo"
    ));
}

#[test]
fn paste_auto_indent_rebases_multiline_text_to_cursor_indent() {
    assert_eq!(
        reindent_multiline_paste("    if ready {\n        run();\n    }", "  "),
        "if ready {\n      run();\n  }"
    );

    let mut buffer = TextBuffer::from_text(1, None, "fn main() {\n    \n}".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(1, 4));
    assert!(paste_text_at_editor_cursors(
        &mut buffer,
        "if ready {\n    run();\n}",
        EditorMultiCursorPaste::Full,
        true,
        true
    ));
    assert_eq!(
        buffer.text(),
        "fn main() {\n    if ready {\n        run();\n    }\n}"
    );
}

#[test]
fn paste_auto_indent_can_skip_string_contexts() {
    let mut buffer = TextBuffer::from_text(1, None, "    let s = \"".to_owned());
    buffer.set_single_cursor(buffer.len_chars());

    assert_eq!(
        auto_indented_paste_text(&buffer, buffer.len_chars(), "one\n  two", false),
        "one\n  two"
    );
    assert_eq!(
        auto_indented_paste_text(&buffer, buffer.len_chars(), "one\n  two", true),
        "one\n      two"
    );
    assert!(line_prefix_looks_inside_string("let s = \"value"));
    assert!(!line_prefix_looks_inside_string("let s = \"value\""));
}

#[test]
fn paste_auto_indent_scans_prefix_without_allocating_full_line() {
    let buffer = TextBuffer::from_text(1, None, concat!("\t", r#"let s = "not \"#).to_owned());
    let cursor = buffer.len_chars();

    assert_eq!(
        auto_indented_paste_text(&buffer, cursor, "one\n  two", false),
        "one\n  two"
    );
    assert_eq!(
        auto_indented_paste_text(&buffer, cursor, "one\n  two", true),
        "one\n\t  two"
    );
}

#[test]
fn normalized_ime_preedit_text_sanitizes_and_bounds_input() {
    assert_eq!(
        normalized_ime_preedit_text("wen\nzi").as_deref(),
        Some("wenzi")
    );
    assert_eq!(normalized_ime_preedit_text("\n\t"), None);
    assert_eq!(
        normalized_ime_preedit_text(&"a".repeat(300))
            .unwrap()
            .chars()
            .count(),
        256
    );
}

#[test]
fn focused_editor_context_switch_discards_half_typed_vim_sequence() {
    use crate::editor_vim_key_events::{
        EditorVimPendingKey, vim_command_input_text_for_test, vim_search_input_text_for_test,
    };

    let root = PathBuf::from("vim-overlay-pending-test");
    let mut app = app_for_input_test(root);
    let buffer = TextBuffer::from_text(1, None, "alpha".to_owned());
    app.settings.vim_keybindings = true;
    app.editor_vim_mode = EditorVimMode::Normal;
    app.editor_vim_pending_key = Some(EditorVimPendingKey::DeleteMotionCount {
        operator_count: 1,
        motion_count: 2,
    });
    crate::editor_vim_key_events::vim_set_search_input_text_for_test("query");
    crate::editor_vim_key_events::vim_set_command_input_text_for_test("s/a/b");
    app.buffers.push(buffer);
    app.panes[0].active = Some(1);
    app.active = Some(1);
    app.focused_pane = Some(1);
    app.settings_panel_open = true;

    let ctx = Context::default();
    ctx.input_mut(|input| {
        input.events.push(Event::Key {
            key: Key::J,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
    });

    app.handle_editor_input(&ctx, 1, 1);

    // The overlay took the context: the pending operator and both input
    // thread-locals are gone and the buffer was not touched.
    assert_eq!(app.editor_vim_pending_key, None);
    assert_eq!(vim_search_input_text_for_test(), "");
    assert_eq!(vim_command_input_text_for_test(), "");
    assert_eq!(
        app.buffer(1).map(TextBuffer::text),
        Some("alpha".to_owned())
    );
}

#[test]
fn vim_pending_operator_does_not_apply_after_buffer_switch() {
    let root = PathBuf::from("vim-cross-buffer-pending-test");
    let mut app = app_for_input_test(root);
    app.buffers
        .push(TextBuffer::from_text(1, None, "alpha\nbeta".to_owned()));
    app.buffers
        .push(TextBuffer::from_text(2, None, "gamma".to_owned()));
    app.settings.vim_keybindings = true;
    app.editor_vim_mode = EditorVimMode::Normal;
    app.panes[0].active = Some(1);
    app.active = Some(1);
    app.focused_pane = Some(1);

    let ctx = Context::default();
    // `d` in buffer 1 starts a pending operator.
    ctx.input_mut(|input| {
        input.events.push(Event::Key {
            key: Key::D,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
    });
    app.handle_editor_input(&ctx, 1, 1);
    assert!(app.editor_vim_pending_key.is_some());

    // Clicking another tab activates buffer 2; the half-typed `d` must die.
    app.set_active_buffer(2);
    // egui events persist until the next frame pass, so drain them the way a
    // real frame boundary would before feeding the next keystroke.
    ctx.input_mut(|input| input.events.clear());

    ctx.input_mut(|input| {
        input.events.push(Event::Key {
            key: Key::J,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
    });
    app.handle_editor_input(&ctx, 1, 2);

    // `j` in buffer 2 is a plain motion: neither buffer was edited.
    assert_eq!(app.editor_vim_pending_key, None);
    assert_eq!(
        app.buffer(1).map(TextBuffer::text),
        Some("alpha\nbeta".to_owned())
    );
    assert_eq!(
        app.buffer(2).map(TextBuffer::text),
        Some("gamma".to_owned())
    );
}

#[test]
fn vim_dot_repeat_is_per_buffer_and_never_replays_across_buffers() {
    let root = PathBuf::from("vim-cross-buffer-dot-test");
    let mut app = app_for_input_test(root);
    app.buffers
        .push(TextBuffer::from_text(1, None, "alpha".to_owned()));
    app.buffers
        .push(TextBuffer::from_text(2, None, "beta".to_owned()));
    app.settings.vim_keybindings = true;
    app.editor_vim_mode = EditorVimMode::Normal;
    app.panes[0].active = Some(1);
    app.active = Some(1);
    app.focused_pane = Some(1);

    let ctx = Context::default();
    // `x` in buffer 1 records its last change for buffer 1 only.
    ctx.input_mut(|input| {
        input.events.push(Event::Key {
            key: Key::X,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
    });
    app.handle_editor_input(&ctx, 1, 1);
    assert_eq!(app.buffer(1).map(TextBuffer::text), Some("lpha".to_owned()));
    assert!(app.editor_vim_last_change.contains_key(&1));
    ctx.input_mut(|input| input.events.clear());

    app.set_active_buffer(2);

    // `.` in buffer 2 has no last change of its own: nothing happens.
    ctx.input_mut(|input| {
        input.events.push(Event::Key {
            key: Key::Period,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
    });
    app.handle_editor_input(&ctx, 1, 2);
    assert_eq!(app.buffer(2).map(TextBuffer::text), Some("beta".to_owned()));
    assert!(!app.editor_vim_last_change.contains_key(&2));
    ctx.input_mut(|input| input.events.clear());

    // Buffer 1's change is still recorded and still replays there.
    app.set_active_buffer(1);
    ctx.input_mut(|input| {
        input.events.push(Event::Key {
            key: Key::Period,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
    });
    app.handle_editor_input(&ctx, 1, 1);
    assert_eq!(app.buffer(1).map(TextBuffer::text), Some("pha".to_owned()));
}

#[test]
fn vim_normal_key_collapses_multi_cursor_carets_to_the_primary_cursor() {
    let root = PathBuf::from("vim-multi-cursor-collapse-test");
    let mut app = app_for_input_test(root);
    let mut buffer = TextBuffer::from_text(1, None, "abcdef".to_owned());
    buffer.set_cursors([1, 3]);
    app.buffers.push(buffer);
    app.settings.vim_keybindings = true;
    app.editor_vim_mode = EditorVimMode::Normal;
    app.panes[0].active = Some(1);
    app.active = Some(1);
    app.focused_pane = Some(1);

    let ctx = Context::default();
    ctx.input_mut(|input| {
        input.events.push(Event::Key {
            key: Key::X,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
    });

    app.handle_editor_input(&ctx, 1, 1);

    // `x` edits the primary cursor and the extra carets are gone instead of
    // staying visible while silently diverging.
    let buffer = app.buffer(1).expect("buffer remains loaded");
    assert_eq!(buffer.text(), "abcef");
    assert_eq!(buffer.selections(), &[Selection::caret(3)]);
}

#[test]
fn ime_preedit_is_not_painted_in_vim_normal_mode() {
    let root = PathBuf::from("vim-normal-ime-preedit-test");
    let mut app = app_for_input_test(root);
    app.buffers
        .push(TextBuffer::from_text(1, None, "alpha".to_owned()));
    app.settings.vim_keybindings = true;
    app.editor_vim_mode = EditorVimMode::Normal;
    app.panes[0].active = Some(1);
    app.active = Some(1);
    app.focused_pane = Some(1);

    let ctx = Context::default();
    ctx.input_mut(|input| {
        input
            .events
            .push(Event::Ime(ImeEvent::Preedit("wen".to_owned())));
    });

    app.handle_editor_input(&ctx, 1, 1);

    assert!(app.ime_preedit.is_none());
}

#[test]
fn ime_preedit_keeps_working_in_vim_insert_mode() {
    let root = PathBuf::from("vim-insert-ime-preedit-test");
    let mut app = app_for_input_test(root);
    app.buffers
        .push(TextBuffer::from_text(1, None, "alpha".to_owned()));
    app.settings.vim_keybindings = true;
    app.editor_vim_mode = EditorVimMode::Insert;
    app.panes[0].active = Some(1);
    app.active = Some(1);
    app.focused_pane = Some(1);

    let ctx = Context::default();
    ctx.input_mut(|input| {
        input
            .events
            .push(Event::Ime(ImeEvent::Preedit("wen".to_owned())));
    });

    app.handle_editor_input(&ctx, 1, 1);

    let preedit = app.ime_preedit.expect("preedit is stored in insert mode");
    assert_eq!(preedit.buffer_id, 1);
    assert_eq!(preedit.text, "wen");
}

#[test]
fn leaving_vim_insert_mode_clears_a_stale_ime_preedit() {
    let root = PathBuf::from("vim-insert-exit-ime-preedit-test");
    let mut app = app_for_input_test(root);
    app.buffers
        .push(TextBuffer::from_text(1, None, "alpha".to_owned()));
    app.settings.vim_keybindings = true;
    app.editor_vim_mode = EditorVimMode::Insert;
    app.panes[0].active = Some(1);
    app.active = Some(1);
    app.focused_pane = Some(1);

    let ctx = Context::default();
    ctx.input_mut(|input| {
        input
            .events
            .push(Event::Ime(ImeEvent::Preedit("wen".to_owned())));
    });
    app.handle_editor_input(&ctx, 1, 1);
    assert!(app.ime_preedit.is_some());
    ctx.input_mut(|input| input.events.clear());

    ctx.input_mut(|input| {
        input.events.push(Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
    });
    app.handle_editor_input(&ctx, 1, 1);

    assert_eq!(app.editor_vim_mode, EditorVimMode::Normal);
    assert!(app.ime_preedit.is_none());
}

fn app_for_input_test(root: PathBuf) -> KuroyaApp {
    let (tx, rx) = ui_event_channel();
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
