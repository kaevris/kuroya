use crate::{
    app_tabs::{
        diff_tab_context_action_labels, file_tab_compare_context_action_labels,
        file_tab_path_context_action_labels,
    },
    buffer_find::{
        buffer_find_scope_from_selection, live_find_query_should_move_cursor, next_find_match_index,
    },
    command_ui_runtime::find_query_seed_from_selection,
    editor_context_menu::{
        buffer_history_context_action_labels, buffer_path_context_action_labels,
        diff_hunk_context_action_labels, diff_patch_context_action_labels,
        file_hunk_context_action_labels, file_source_context_action_labels,
        file_source_control_context_action_labels,
    },
    editor_input::{editor_events_include_mutation, protected_preview_edit_block_reason},
    editor_pane_chrome::{
        diff_editor_toolbar_action_labels, diff_editor_toolbar_action_labels_for_width,
        diff_editor_toolbar_button_labels_for_width,
    },
    editor_tabs::buffer_tab_label,
    file_io::decode_text_bytes,
};

use eframe::egui::{Event, ImeEvent, Key, Modifiers};
use kuroya_core::{
    EditorFindAutoFindInSelection, EditorFindSeedSearchStringFromSelection, GitChangeStage,
    TextBuffer,
};
use std::collections::HashSet;

#[test]
fn byte_file_decode_marks_invalid_utf8_as_lossy() {
    let valid = decode_text_bytes(b"fn main() {}\n".to_vec());
    assert!(!valid.lossy);
    assert!(!valid.binary);
    assert_eq!(valid.text, "fn main() {}\n");

    let invalid = decode_text_bytes(vec![b'o', b'k', 0xff, b'\n']);
    assert!(invalid.lossy);
    assert!(!invalid.binary);
    assert!(invalid.text.contains('\u{FFFD}'));

    let binary = decode_text_bytes(vec![b'o', b'k', 0, b'\n']);
    assert!(!binary.lossy);
    assert!(binary.binary);
    assert_eq!(binary.text, "ok\0\n");
}

#[test]
fn protected_preview_edit_guards_report_read_only_reason() {
    let lossy = HashSet::from([1]);
    let binary = HashSet::from([2]);

    assert_eq!(
        protected_preview_edit_block_reason(1, &lossy, &binary),
        Some("UTF-8 replacement previews are read-only")
    );
    assert_eq!(
        protected_preview_edit_block_reason(2, &lossy, &binary),
        Some("binary previews are read-only")
    );
    assert_eq!(
        protected_preview_edit_block_reason(3, &lossy, &binary),
        None
    );
}

#[test]
fn editor_events_classify_mutations_without_blocking_navigation() {
    assert!(!editor_events_include_mutation(&[Event::Copy]));
    assert!(!editor_events_include_mutation(&[Event::Key {
        key: Key::ArrowRight,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]));
    assert!(editor_events_include_mutation(&[Event::Text(
        "x".to_owned()
    )]));
    assert!(!editor_events_include_mutation(&[Event::Text(
        "\u{85}".to_owned()
    )]));
    assert!(editor_events_include_mutation(&[Event::Ime(
        ImeEvent::Commit("文".to_owned())
    )]));
    assert!(!editor_events_include_mutation(&[Event::Ime(
        ImeEvent::Commit(String::new())
    )]));
    assert!(!editor_events_include_mutation(&[Event::Ime(
        ImeEvent::Preedit("wen".to_owned())
    )]));
    assert!(!editor_events_include_mutation(&[Event::Ime(
        ImeEvent::Enabled
    )]));
    assert!(!editor_events_include_mutation(&[Event::Ime(
        ImeEvent::Disabled
    )]));
    assert!(editor_events_include_mutation(&[Event::Paste(
        "x".to_owned()
    )]));
    assert!(!editor_events_include_mutation(&[Event::Paste(
        "\u{0}\u{1b}\u{7f}".to_owned()
    )]));
    assert!(editor_events_include_mutation(&[Event::Paste(
        "\t".to_owned()
    )]));
    assert!(editor_events_include_mutation(&[Event::Key {
        key: Key::Backspace,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::CTRL,
    }]));
    assert!(editor_events_include_mutation(&[Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::CTRL,
    }]));
}

#[test]
fn find_query_seed_follows_vs_code_modes_and_word_fallback() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha beta\ngamma".to_owned());
    buffer.set_selection(6, 10);

    assert_eq!(
        find_query_seed_from_selection(
            Some(&buffer),
            EditorFindSeedSearchStringFromSelection::Always,
        )
        .as_deref(),
        Some("beta")
    );
    assert_eq!(
        find_query_seed_from_selection(
            Some(&buffer),
            EditorFindSeedSearchStringFromSelection::Selection,
        )
        .as_deref(),
        Some("beta")
    );
    assert_eq!(
        find_query_seed_from_selection(
            Some(&buffer),
            EditorFindSeedSearchStringFromSelection::Never,
        ),
        None
    );

    buffer.set_selection(0, 12);
    assert_eq!(
        find_query_seed_from_selection(
            Some(&buffer),
            EditorFindSeedSearchStringFromSelection::Always,
        ),
        None
    );

    buffer.set_single_cursor(2);
    assert_eq!(
        find_query_seed_from_selection(
            Some(&buffer),
            EditorFindSeedSearchStringFromSelection::Always,
        )
        .as_deref(),
        Some("alpha")
    );
    assert_eq!(
        find_query_seed_from_selection(
            Some(&buffer),
            EditorFindSeedSearchStringFromSelection::Selection,
        ),
        None
    );
}

#[test]
fn find_scope_from_selection_follows_auto_find_modes() {
    let mut buffer = TextBuffer::from_text(1, None, "alpha beta\ngamma".to_owned());
    buffer.set_selection(0, 5);

    assert_eq!(
        buffer_find_scope_from_selection(&buffer, EditorFindAutoFindInSelection::Never),
        None
    );
    assert_eq!(
        buffer_find_scope_from_selection(&buffer, EditorFindAutoFindInSelection::Always),
        Some(0..5)
    );
    assert_eq!(
        buffer_find_scope_from_selection(&buffer, EditorFindAutoFindInSelection::Multiline),
        None
    );

    buffer.set_selection(0, 12);
    assert_eq!(
        buffer_find_scope_from_selection(&buffer, EditorFindAutoFindInSelection::Multiline),
        Some(0..12)
    );
}

#[test]
fn find_navigation_loop_setting_controls_wraparound() {
    assert_eq!(next_find_match_index(0, 3, -1, true), Some(2));
    assert_eq!(next_find_match_index(2, 3, 1, true), Some(0));
    assert_eq!(next_find_match_index(0, 3, -1, false), None);
    assert_eq!(next_find_match_index(2, 3, 1, false), None);
    assert_eq!(next_find_match_index(1, 3, -1, false), Some(0));
    assert_eq!(next_find_match_index(1, 3, 1, false), Some(2));
    assert_eq!(next_find_match_index(99, 3, 1, false), None);
}

#[test]
fn live_find_cursor_movement_requires_both_type_settings() {
    assert!(live_find_query_should_move_cursor(true, true));
    assert!(!live_find_query_should_move_cursor(false, true));
    assert!(!live_find_query_should_move_cursor(true, false));
    assert!(!live_find_query_should_move_cursor(false, false));
}

#[test]
fn buffer_tab_label_marks_dirty_and_external_changes() {
    assert_eq!(buffer_tab_label("main.rs", false, false, false), "main.rs");
    assert_eq!(buffer_tab_label("main.rs", true, false, false), "* main.rs");
    assert_eq!(buffer_tab_label("main.rs", false, true, false), "! main.rs");
    assert_eq!(
        buffer_tab_label("main.rs", false, false, true),
        "RO main.rs"
    );
    assert_eq!(
        buffer_tab_label("main.rs", true, true, true),
        "! * RO main.rs"
    );
}

#[test]
fn diff_editor_context_actions_follow_diff_stage() {
    assert_eq!(
        diff_patch_context_action_labels(true, false, false),
        vec![
            "Copy Patch",
            "Copy Hunk Patch",
            "Previous Diff Hunk",
            "Next Diff Hunk",
            "Open Accessible Diff Viewer"
        ]
    );
    assert_eq!(
        diff_patch_context_action_labels(false, true, true),
        vec!["Refresh Diff", "Swap Compare Sides"]
    );
    assert!(diff_patch_context_action_labels(false, false, false).is_empty());
    assert_eq!(
        diff_hunk_context_action_labels(Some(GitChangeStage::Unstaged)),
        vec!["Stage Current Diff Hunk", "Discard Current Diff Hunk"]
    );
    assert_eq!(
        diff_hunk_context_action_labels(Some(GitChangeStage::Staged)),
        vec!["Unstage Current Diff Hunk"]
    );
    assert!(diff_hunk_context_action_labels(None).is_empty());
}

#[test]
fn diff_editor_toolbar_actions_follow_diff_stage() {
    assert_eq!(
        diff_editor_toolbar_action_labels(
            Some(GitChangeStage::Unstaged),
            true,
            true,
            true,
            true,
            false
        ),
        vec![
            "Refresh Diff",
            "Previous Diff Hunk",
            "Next Diff Hunk",
            "Copy Diff Patch",
            "Copy Current Diff Hunk Patch",
            "Open Accessible Diff Viewer",
            "Open Diff Base File",
            "Open Base at Current Diff Hunk",
            "Open Diff Source File",
            "Open Source at Current Diff Hunk",
            "Stage Current Diff Hunk",
            "Discard Current Diff Hunk"
        ]
    );
    assert_eq!(
        diff_editor_toolbar_action_labels(
            Some(GitChangeStage::Staged),
            true,
            true,
            true,
            true,
            false
        ),
        vec![
            "Refresh Diff",
            "Previous Diff Hunk",
            "Next Diff Hunk",
            "Copy Diff Patch",
            "Copy Current Diff Hunk Patch",
            "Open Accessible Diff Viewer",
            "Open Diff Base File",
            "Open Base at Current Diff Hunk",
            "Open Diff Source File",
            "Open Source at Current Diff Hunk",
            "Unstage Current Diff Hunk"
        ]
    );
    assert_eq!(
        diff_editor_toolbar_action_labels(None, false, true, false, false, false),
        vec![
            "Previous Diff Hunk",
            "Next Diff Hunk",
            "Copy Diff Patch",
            "Copy Current Diff Hunk Patch",
            "Open Accessible Diff Viewer",
        ]
    );
    assert_eq!(
        diff_editor_toolbar_action_labels(None, true, true, true, true, true),
        vec![
            "Refresh Diff",
            "Swap Compare Sides",
            "Previous Diff Hunk",
            "Next Diff Hunk",
            "Copy Diff Patch",
            "Copy Current Diff Hunk Patch",
            "Open Accessible Diff Viewer",
            "Open Diff Base File",
            "Open Base at Current Diff Hunk",
            "Open Diff Source File",
            "Open Source at Current Diff Hunk"
        ]
    );
    assert_eq!(
        diff_editor_toolbar_action_labels(
            Some(GitChangeStage::Unstaged),
            true,
            false,
            true,
            true,
            false
        ),
        vec![
            "Refresh Diff",
            "Open Diff Base File",
            "Open Diff Source File"
        ]
    );
    assert_eq!(
        diff_editor_toolbar_action_labels_for_width(
            Some(GitChangeStage::Unstaged),
            true,
            true,
            true,
            true,
            false,
            320.0
        ),
        vec![
            "Refresh Diff",
            "Previous Diff Hunk",
            "Next Diff Hunk",
            "Open Diff Base File",
            "Stage Current Diff Hunk",
            "Discard Current Diff Hunk"
        ]
    );
    assert_eq!(
        diff_editor_toolbar_button_labels_for_width(
            Some(GitChangeStage::Unstaged),
            true,
            true,
            true,
            true,
            false,
            320.0,
            true
        ),
        vec!["Ref", "Prev", "Next", "Base", "File", "Stage", "Disc"]
    );
    assert!(
        diff_editor_toolbar_action_labels_for_width(
            Some(GitChangeStage::Unstaged),
            true,
            true,
            true,
            true,
            false,
            f32::NAN,
        )
        .is_empty()
    );
}

#[test]
fn worktree_editor_context_actions_follow_git_changes() {
    assert_eq!(
        file_hunk_context_action_labels(true, false),
        vec![
            "Open Current Hunk Diff",
            "Copy Current Hunk Patch",
            "Stage Current Hunk",
            "Discard Current Hunk"
        ]
    );
    assert_eq!(
        file_hunk_context_action_labels(false, true),
        vec![
            "Open Current Staged Hunk Diff",
            "Copy Current Staged Hunk Patch",
            "Unstage Current Hunk"
        ]
    );
    assert_eq!(
        file_hunk_context_action_labels(true, true),
        vec![
            "Open Current Hunk Diff",
            "Copy Current Hunk Patch",
            "Stage Current Hunk",
            "Open Current Staged Hunk Diff",
            "Copy Current Staged Hunk Patch",
            "Unstage Current Hunk",
            "Discard Current Hunk"
        ]
    );
    assert!(file_hunk_context_action_labels(false, false).is_empty());
}

#[test]
fn editor_context_actions_include_file_source_control_actions() {
    assert_eq!(
        file_source_control_context_action_labels(true, false, true),
        vec![
            "Open Changes",
            "Copy Patch",
            "Open Hunks",
            "Stage File Changes",
            "Discard File Changes"
        ]
    );
    assert_eq!(
        file_source_control_context_action_labels(false, true, true),
        vec![
            "Open Staged Changes",
            "Copy Staged Patch",
            "Open Staged Hunks",
            "Unstage File Changes",
            "Discard File Changes"
        ]
    );
    assert_eq!(
        file_source_control_context_action_labels(true, true, true),
        vec![
            "Open Changes",
            "Copy Patch",
            "Open Staged Changes",
            "Copy Staged Patch",
            "Open Hunks",
            "Open Staged Hunks",
            "Stage File Changes",
            "Unstage File Changes",
            "Discard File Changes"
        ]
    );
    assert!(file_source_control_context_action_labels(false, false, false).is_empty());
}

#[test]
fn editor_context_actions_include_source_file_navigation() {
    assert_eq!(
        file_source_context_action_labels(true, true, true, true, true, false, true, true, true),
        vec![
            "Open Diff Base File",
            "Open Base at Current Hunk",
            "Open Diff Source File",
            "Open Source at Current Hunk",
            "Open Blame",
            "Compare with HEAD",
            "Open File at HEAD",
            "Open File at Index",
            "Compare with Saved",
            "Select for Compare",
            "Compare with Selected",
            "Reveal in Explorer",
            "Reveal in Source Control"
        ]
    );
    assert_eq!(
        file_source_context_action_labels(
            false, false, false, true, false, true, false, true, false
        ),
        vec![
            "Compare with HEAD",
            "Open File at HEAD",
            "Open File at Index",
            "Select for Compare",
            "Previous Git Change",
            "Next Git Change",
            "Reveal in Explorer"
        ]
    );
    assert!(
        file_source_context_action_labels(
            false, false, false, false, false, false, false, false, false
        )
        .is_empty()
    );
}

#[test]
fn editor_context_menu_offers_history_and_path_rows_without_git_state() {
    assert_eq!(buffer_history_context_action_labels(), vec!["Undo", "Redo"]);
    assert_eq!(
        buffer_path_context_action_labels(),
        vec!["Copy Path", "Copy Relative Path"]
    );
}

#[test]
fn diff_tab_context_actions_can_open_source_file_and_current_hunk() {
    assert_eq!(
        diff_tab_context_action_labels(
            Some(GitChangeStage::Unstaged),
            true,
            true,
            true,
            true,
            false
        ),
        vec![
            "Refresh Diff",
            "Copy Patch",
            "Copy Current Hunk Patch",
            "Previous Diff Hunk",
            "Next Diff Hunk",
            "Open Diff Base File",
            "Open Base at Current Hunk",
            "Open Diff Source File",
            "Open Source at Current Hunk",
            "Open Blame",
            "Stage Current Diff Hunk",
            "Discard Current Diff Hunk"
        ]
    );
    assert_eq!(
        diff_tab_context_action_labels(Some(GitChangeStage::Staged), true, true, true, true, false),
        vec![
            "Refresh Diff",
            "Copy Patch",
            "Copy Current Hunk Patch",
            "Previous Diff Hunk",
            "Next Diff Hunk",
            "Open Diff Base File",
            "Open Base at Current Hunk",
            "Open Diff Source File",
            "Open Source at Current Hunk",
            "Open Blame",
            "Unstage Current Diff Hunk"
        ]
    );
    assert_eq!(
        diff_tab_context_action_labels(
            Some(GitChangeStage::Unstaged),
            false,
            true,
            true,
            true,
            false
        ),
        vec![
            "Refresh Diff",
            "Copy Patch",
            "Copy Current Hunk Patch",
            "Previous Diff Hunk",
            "Next Diff Hunk",
            "Open Diff Base File",
            "Open Base at Current Hunk",
            "Stage Current Diff Hunk",
            "Discard Current Diff Hunk"
        ]
    );
    assert_eq!(
        diff_tab_context_action_labels(None, true, true, true, true, true),
        vec![
            "Refresh Diff",
            "Swap Compare Sides",
            "Copy Patch",
            "Copy Current Hunk Patch",
            "Previous Diff Hunk",
            "Next Diff Hunk",
            "Open Diff Base File",
            "Open Base at Current Hunk",
            "Open Diff Source File",
            "Open Source at Current Hunk",
            "Open Blame"
        ]
    );
    assert_eq!(
        diff_tab_context_action_labels(None, false, true, false, false, false),
        vec![
            "Copy Patch",
            "Copy Current Hunk Patch",
            "Previous Diff Hunk",
            "Next Diff Hunk"
        ]
    );
    assert_eq!(
        diff_tab_context_action_labels(None, false, false, true, true, false),
        vec!["Refresh Diff", "Open Diff Base File"]
    );
    assert!(diff_tab_context_action_labels(None, false, false, false, false, false).is_empty());
}

#[test]
fn file_tab_context_actions_support_file_compare_flow() {
    assert_eq!(
        file_tab_compare_context_action_labels(true, false, false),
        vec!["Select for Compare"]
    );
    assert_eq!(
        file_tab_compare_context_action_labels(true, true, true),
        vec![
            "Compare with Saved",
            "Select for Compare",
            "Compare with Selected"
        ]
    );
    assert_eq!(
        file_tab_compare_context_action_labels(true, false, true),
        vec!["Select for Compare", "Compare with Selected"]
    );
    assert!(file_tab_compare_context_action_labels(false, false, false).is_empty());
}

#[test]
fn file_tab_context_actions_include_delete_for_named_files() {
    assert_eq!(
        file_tab_path_context_action_labels(true),
        vec!["Copy Path", "Copy Relative Path", "Delete"]
    );
    assert_eq!(
        file_tab_path_context_action_labels(false),
        vec!["Copy Path", "Copy Relative Path"]
    );
}
